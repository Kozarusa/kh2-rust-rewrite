//! The whole headless runtime (HYB-6): the interpreter, the Rust kernel, the hardware pages and
//! the Rust IOP joined, with the game's frame boundary and a boot event log.
//!
//! * Frames: the main loop of `main` (0x101e70, boot contract 1.2 and 4) runs the task list once
//!   per pass; a probe on the loop head counts passes. The frame boundary of the contract is the
//!   return from the VBlank wait at the end of the frame task, a few instructions before the next
//!   pass, so "at the loop head" sees the whole state of the previous frame.
//! * Boot events: the init calls of `main` (1.2), every server bind and every module load, in the
//!   order they happen, for the comparison with the traced order.

use std::cell::RefCell;
use std::rc::Rc;

use kh2_ee::{Ee, Fault};

use crate::hw::HwMachine;
use crate::iop::{Disc, Handle, Mem};
use crate::kernel::RunStop;

/// The head of the main loop of `main` [V: contract 1.2, row "loop"].
pub const MAIN_LOOP: u32 = 0x101e70;
/// The overlay loader `0x167ee0(name, fn, arg)` [V: contract 6].
pub const OVERLAY_LOAD: u32 = 0x167ee0;
/// The game's own vsync counter, raised by its VBlank handler [V: contract 4].
pub const GAME_VSYNC: u32 = 0x356f34;
/// The frame counter of the main loop, (+1) mod 1000 per pass [V: contract 1.2].
pub const FRAME_COUNTER: u32 = 0x33e6a0;

/// World, area and entrance bytes of the current place [V: RIG.md 1.1].
pub const PLACE: u32 = 0x33e830;
/// The pointer to the player object; its position is at +0x70 (x, y, z floats) [V: RIG.md 1.1].
pub const PLAYER_PTR: u32 = 0x35c998;
pub const PLAYER_POS: u32 = 0x70;

/// Where the game is: (world, area, entrance) and the player position if a player object exists.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Place {
    pub world: u8,
    pub area: u8,
    pub entrance: u8,
    pub player: Option<[f32; 3]>,
}

/// The init calls `main` 0x101d68 makes, in order [V: contract 1.2 rows 1 to 26].
pub const MAIN_INIT_CALLS: [u32; 26] = [
    0x1058d0, 0x1024f8, 0x16f1d8, 0x2fa288, 0x2fbdf8, 0x1062f0, 0x166ee8, 0x138860, 0x105cb0, 0x102df0, 0x1688a0, 0x101d50, 0x138888,
    0x1da458, 0x223180, 0x2305b0, 0x11b198, 0x102dc8, 0x102b98, 0x103328, 0x239db8, 0x103f38, 0x1043f8, 0x2d2058, 0x105638, 0x102070,
];

/// One event of the boot, as logged.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BootEvent {
    /// `main` called its init function at this address (first call only).
    Init(u32),
    /// A server id was bound.
    Bind(u32),
    /// The loadfile server loaded the module with this file name (upper case, no device or version).
    Module(String),
    /// An overlay was requested through 0x167ee0.
    Overlay(String),
}

/// One pass of the main loop.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Frame {
    /// Retired instructions at the loop head.
    pub steps: u64,
    /// Vsyncs of the virtual clock.
    pub vsync: u64,
}

pub struct Hle {
    pub hw: HwMachine,
    pub iop: Handle,
    pub frames: Rc<RefCell<Vec<Frame>>>,
    pub events: Rc<RefCell<Vec<(u64, BootEvent)>>>,
    /// Stop the run at the next loop head (set by [`Hle::run_frames`]).
    stop_at: Rc<RefCell<Option<usize>>>,
}

impl Hle {
    /// Loads the executable, installs every layer and the logs. `disc` is the user's disc.
    pub fn new(elf: &[u8], disc: Option<Box<dyn Disc>>) -> Option<Hle> {
        let mut hw = HwMachine::new();
        hw.load_elf(elf)?;
        let iop = hw.machine.install_iop(disc);
        let frames: Rc<RefCell<Vec<Frame>>> = Rc::default();
        let events: Rc<RefCell<Vec<(u64, BootEvent)>>> = Rc::default();
        let stop_at: Rc<RefCell<Option<usize>>> = Rc::default();
        let ee = &mut hw.machine.ee;
        for &a in &MAIN_INIT_CALLS {
            let ev = events.clone();
            let mut seen = false;
            ee.probes.insert(a, move |ee: &mut Ee| {
                if !seen {
                    seen = true;
                    ev.borrow_mut().push((ee.steps, BootEvent::Init(a)));
                }
            });
        }
        let ev = events.clone();
        ee.probes.insert(OVERLAY_LOAD, move |ee: &mut Ee| {
            let name = ee.cstr(ee.gpr[4] as u32, 64);
            ev.borrow_mut().push((ee.steps, BootEvent::Overlay(name)));
        });
        let (f, s, k) = (frames.clone(), stop_at.clone(), hw.machine.kernel.clone());
        ee.probes.insert(MAIN_LOOP, move |ee: &mut Ee| {
            let vsync = k.borrow().clock.vsyncs;
            let mut f = f.borrow_mut();
            f.push(Frame { steps: ee.steps, vsync });
            if s.borrow().is_some_and(|n| f.len() >= n) {
                ee.request_stop();
            }
        });
        Some(Hle { hw, iop, frames, events, stop_at })
    }

    /// The current place and the player position.
    pub fn place(&mut self) -> Place {
        let ee = &mut self.hw.machine.ee;
        let (world, area, entrance) = (ee.r8(PLACE), ee.r8(PLACE + 1), ee.r8(PLACE + 2));
        let p = ee.r32(PLAYER_PTR);
        let player = (0x10_0000..0x200_0000).contains(&p).then(|| [0, 4, 8].map(|o| ee.f32_at(p + PLAYER_POS + o)));
        Place { world, area, entrance, player }
    }

    /// Passes of the main loop so far.
    pub fn frame_count(&self) -> usize {
        self.frames.borrow().len()
    }

    /// Binds and module loads from the IOP logs merged with the init calls, in step order.
    pub fn boot_order(&self) -> Vec<BootEvent> {
        let mut all = self.events.borrow().clone();
        let iop = self.iop.borrow();
        for (steps, ev) in &iop.timeline {
            all.push((*steps, ev.clone()));
        }
        all.sort_by_key(|e| e.0);
        all.into_iter().map(|e| e.1).collect()
    }

    /// Runs until `n` more main-loop passes have started, or the budget is used.
    pub fn run_frames(&mut self, n: usize, max_steps: u64) -> Result<RunStop, Fault> {
        let target = self.frame_count() + n;
        *self.stop_at.borrow_mut() = Some(target);
        let start = self.hw.machine.ee.steps;
        let r = loop {
            let used = self.hw.machine.ee.steps - start;
            if used >= max_steps {
                break Ok(RunStop::Budget);
            }
            match self.hw.run(max_steps - used) {
                Ok(RunStop::Requested) if self.frame_count() >= target => break Ok(RunStop::Requested),
                Ok(RunStop::Budget) => {
                    // The idle limit ended the call; keep going while the budget lasts.
                    if self.hw.machine.ee.steps == start + used {
                        break Ok(RunStop::Budget);
                    }
                }
                other => break other,
            }
        };
        *self.stop_at.borrow_mut() = None;
        r
    }
}
