//! The sound driver's side (boot contract 2.4, "Sound command ring").
//!
//! The game binds the `libssl` server (id 0x11) once and sends it the EE addresses of the command
//! ring (512 slots of 0x40 bytes), of eight stream records and of a status word; the reply gives
//! the IOP addresses of the ring and of a second block. After that no RPC is made for sound: a
//! command takes the first slot whose word 0 is zero, writes the command id to word 0 and up to
//! six arguments to words 1 to 6, sets word 15 to 1 and sends the slot to the IOP with
//! `sceSifSetDma` (plus a 16-byte notify packet). The IOP clears word 0 of the EE slot when it has
//! consumed the command; the game counts slots still holding the ids of bank and wave loads
//! (0xb to 0xe, and 0xf).
//!
//! Here the DMA syscall is wrapped: a transfer of 0x40 bytes into the IOP ring is decoded into a
//! [`SoundCommand`], logged with the frame number, and the slot's word 0 is cleared (at once for
//! most ids; for the loads 0xb to 0xf after [`LOAD_DELAY`] cycles of virtual time [inferred]).
//! Meanings of the ids belong to kh2-audio (docs/audio.md); here they stay numbers.

use std::cell::RefCell;
use std::rc::Rc;

use kh2_ee::{Ee, reg};

use super::{Handle, Mem, Request, Server};
use crate::kernel::VBLANK_CYCLES;

pub const SERVER_ID: u32 = 0x11;
/// Number of slots and bytes per slot of the ring [V: 0x2bd8a8].
pub const SLOTS: u32 = 512;
pub const SLOT_BYTES: u32 = 0x40;
/// The IOP address of the ring the server reports [V: 0x16e770 in PCSX2; any value would do].
pub const IOP_RING: u32 = 0x0016_e770;
/// The second IOP block the server reports [inferred: unused by the game in boot].
pub const IOP_STATUS: u32 = 0x0017_e770;
/// Command ids of bank and wave loads; the IOP finishes them later.
pub const LOAD_IDS: std::ops::RangeInclusive<u32> = 0xb..=0xf;
/// Virtual time the IOP takes for a load [inferred].
pub const LOAD_DELAY: u64 = VBLANK_CYCLES;
/// Bytes of one stream record and how many there are [V: 0x2bf764].
pub const STREAM_RECORD: usize = 0x68;
pub const STREAM_RECORDS: usize = 8;
/// The functions that poll the ring for pending loads; delayed clears are applied before they run [V: 0x2be1a8, 0x2be378].
pub const COUNT_PENDING_LOADS: u32 = 0x2be1a8;
pub const COUNT_PENDING_FLAGS: u32 = 0x2be378;

/// One decoded ring slot.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SoundCommand {
    /// The frame (vsync) of the virtual clock the command was sent in.
    pub vsync: u64,
    pub slot: u32,
    pub id: u32,
    pub args: [u32; 6],
    /// Word 15 of the slot (the game sets it to 1).
    pub flag: u32,
}

/// A slot's id, arguments and flag from its 0x40 bytes.
pub fn decode_slot(bytes: &[u8]) -> (u32, [u32; 6], u32) {
    let w = |i: usize| bytes.get(4 * i..4 * i + 4).map_or(0, |b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]));
    (w(0), [w(1), w(2), w(3), w(4), w(5), w(6)], w(15))
}

#[derive(Default)]
pub struct Ring {
    pub bound: bool,
    pub ee_ring: u32,
    pub streams: u32,
    pub status: u32,
    pub log: Vec<SoundCommand>,
    /// (due time, EE address of the slot, command id) of loads not yet consumed.
    pending: Vec<(u64, u32, u32)>,
}

impl Ring {
    /// The binding call of the sound driver: the EE blocks it will use. Stream records and the
    /// status word start zero (no stream is active).
    pub fn bind(&mut self, mem: &mut dyn Mem, streams: u32, ring: u32, status: u32) {
        self.bound = true;
        self.ee_ring = ring;
        self.streams = streams;
        self.status = status;
        if streams != 0 {
            mem.wr(streams, &vec![0u8; STREAM_RECORD * STREAM_RECORDS]);
        }
        if status != 0 {
            mem.w32(status, 0);
        }
    }

    /// A SIF DMA transfer list (`src, dest, size, attr` per entry). Transfers of one slot into the
    /// IOP ring are commands.
    pub fn on_dma(&mut self, mem: &mut dyn Mem, descs: &[[u32; 4]], vsync: u64, now: u64) {
        if !self.bound {
            return;
        }
        for d in descs {
            let (src, dest, size) = (d[0], d[1], d[2]);
            if size != SLOT_BYTES || !(IOP_RING..IOP_RING + SLOTS * SLOT_BYTES).contains(&dest) || !(dest - IOP_RING).is_multiple_of(SLOT_BYTES) {
                continue;
            }
            let slot = (dest - IOP_RING) / SLOT_BYTES;
            let bytes = mem.rd(src, SLOT_BYTES as usize);
            let (id, args, flag) = decode_slot(&bytes);
            self.log.push(SoundCommand { vsync, slot, id, args, flag });
            if LOAD_IDS.contains(&id) {
                self.pending.push((now + LOAD_DELAY, src, id));
            } else {
                mem.w32(src, 0);
            }
        }
    }

    /// Clears the slots of loads whose time has come (only if the slot still holds that command).
    pub fn tick(&mut self, mem: &mut dyn Mem, now: u64) {
        if self.pending.is_empty() {
            return;
        }
        let mut still = Vec::new();
        for (due, addr, id) in std::mem::take(&mut self.pending) {
            if due > now {
                still.push((due, addr, id));
            } else if mem.r32(addr) == id {
                mem.w32(addr, 0);
            }
        }
        self.pending = still;
    }

    /// The commands sent in one frame, in order.
    pub fn commands_in_frame(&self, vsync: u64) -> Vec<&SoundCommand> {
        self.log.iter().filter(|c| c.vsync == vsync).collect()
    }

    /// Loads still waiting for their slot to be cleared.
    pub fn pending_loads(&self) -> usize {
        self.pending.len()
    }
}

/// The `libssl` server: function 0 binds the driver (send block: mode, EE stream records, EE ring,
/// EE status word; reply: IOP ring address, second IOP address) [V: 0x2bdb4c]. Functions 1 to 4 were
/// not seen; they answer zeros.
pub struct Libssl {
    ring: Rc<RefCell<Ring>>,
}

impl Libssl {
    pub fn new(ring: Rc<RefCell<Ring>>) -> Libssl {
        Libssl { ring }
    }
}

impl Server for Libssl {
    fn ids(&self) -> &[u32] {
        &[SERVER_ID]
    }

    fn call(&mut self, mem: &mut dyn Mem, req: &Request) -> Option<Vec<u8>> {
        if req.fno != 0 {
            return None;
        }
        self.ring.borrow_mut().bind(mem, req.word(1), req.word(2), req.word(3));
        let mut r = IOP_RING.to_le_bytes().to_vec();
        r.extend_from_slice(&IOP_STATUS.to_le_bytes());
        Some(r)
    }
}

pub(super) fn install(ee: &mut Ee, iop: &Handle) {
    for n in [0x77i32, -0x77] {
        // Keep the kernel's own handler (it returns the transfer id) and look at the list first.
        let old = ee.syscalls.insert(n, |_: &mut Ee| {});
        let h = iop.clone();
        ee.syscalls.insert(n, move |ee: &mut Ee| {
            let (list, count) = (ee.gpr[reg::A0] as u32, ee.gpr[reg::A0 + 1] as u32);
            {
                let iop = h.borrow();
                if iop.sound.borrow().bound && count > 0 && count <= 32 {
                    let raw = ee.rd(list, 16 * count as usize);
                    let descs: Vec<[u32; 4]> = raw
                        .chunks(16)
                        .map(|c| [0, 4, 8, 12].map(|o| u32::from_le_bytes([c[o], c[o + 1], c[o + 2], c[o + 3]])))
                        .collect();
                    let now = iop.kernel.borrow().clock.now(ee.steps);
                    iop.sound.borrow_mut().on_dma(ee, &descs, now / VBLANK_CYCLES, now);
                }
            }
            if let Some(o) = &old {
                let mut f = o.borrow_mut();
                (*f)(ee);
            }
        });
    }
    for addr in [COUNT_PENDING_LOADS, COUNT_PENDING_FLAGS] {
        let h = iop.clone();
        ee.probes.insert(addr, move |ee: &mut Ee| h.borrow_mut().tick(ee));
    }
}
