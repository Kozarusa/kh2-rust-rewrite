//! Oracle tests for the task and frame rows that already had a port (WP-L1-P18a1): kh2-ee runs the original from the user's executable
//! and the port (`kh2_game_core::{task, frame}`) runs on the same inputs.
//!
//! The harness is hand-written (the units make tasks and call other units, which the factory generator cannot lay out). It follows the
//! factory's rules: the original runs from the user's executable, cases come from a seeded generator (a failing case number reproduces)
//! and every compared case counts.
//!
//! Stand-ins (stubs), all documented here:
//! - `0x102778` (the task creator, verified by WP-L1-P20's tests) in the wrappers `0x102bc8` and `0x14cb50`: it logs its four
//!   arguments and returns a marker; the port's tasks are compared field by field against those arguments.
//! - `0x102498` (the print call) in `0x14cb08`, `0x14d1f8` (signals the vblank semaphore) and the cache flush `0x318650` in `0x14cc88`:
//!   they log their arguments. The ramp (`0x14ed20`, `0x14ecd8`, `0x14eca0`), the speed setter `0x14cbc0`, the play-time tick
//!   `0x14ca28` and the timer tick `0x14c7b8` run natively in the original.
//!
//! Skips without a disc (KH2_ISO or local/iso.txt); fails without one when KH2_REQUIRE_DISC=1.

use std::cell::RefCell;
use std::rc::Rc;

use kh2_ee::Ee;
use kh2_factory::fuzz::{Rng, STACK};
use kh2_game_core::area::ctl::{AreaCtl, AreaHost};
use kh2_game_core::frame::{self, Ramp, Vsync};
use kh2_game_core::task::{field, Tasks};

const SAVE: u32 = 0x33e860;
const SAVE_LEN: usize = 0x1700;
const VS: u32 = 0x356f18;
const VS_LEN: usize = 0x3c;

fn machine(what: &str) -> Option<Ee> {
    let elf = kh2_factory::exe_or_skip(what)?;
    let mut ee = Ee::new();
    ee.load_elf(&elf).expect("elf");
    Some(ee)
}

fn call(ee: &mut Ee, addr: u32, args: &[u64], fargs: &[f32]) -> u64 {
    ee.call(addr, args, fargs, STACK).unwrap_or_else(|f| panic!("the original at {addr:#x} faulted: {f:?}"))
}

/// Floats for the EE: finite, with a few large and small magnitudes.
fn rf(r: &mut Rng) -> f32 {
    match r.below(12) {
        0 => 1e30,
        1 => -1e30,
        2 => 1e-30,
        3 => 0.0,
        _ => r.float(),
    }
}

type CallLog = Rc<RefCell<Vec<(u32, Vec<u64>)>>>;

/// Hooks that log the call (callee and the first `n` argument registers) into the log; they return `ret`.
fn log_hooks(ee: &mut Ee, addrs: &[(u32, usize)], ret: u64) -> CallLog {
    let log: CallLog = Rc::new(RefCell::new(Vec::new()));
    for &(a, n) in addrs {
        let l = log.clone();
        ee.hooks.insert(a, move |ee: &mut Ee| {
            let args: Vec<u64> = (0..n).map(|i| ee.gpr[4 + i] as u64).collect();
            l.borrow_mut().push((a, args));
            ee.gpr[2] = ret as u128;
        });
    }
    log
}

/// The port's side of the calls: logs them like the hooks do.
#[derive(Default)]
struct Log(Vec<(u32, Vec<u64>)>);

impl AreaHost for Log {
    fn call(&mut self, callee: u32, args: &[u64]) -> u64 {
        self.0.push((callee, args.to_vec()));
        0
    }
    fn vcall(&mut self, _obj: u32, _slot: u32, _args: &[u64]) -> u64 {
        unreachable!("no virtual call in these units")
    }
    fn read8(&mut self, _addr: u32) -> u8 {
        unreachable!("no memory read in these units")
    }
    fn read16(&mut self, _addr: u32) -> u16 {
        unreachable!("no memory read in these units")
    }
    fn read32(&mut self, _addr: u32) -> u32 {
        unreachable!("no memory read in these units")
    }
    fn write8(&mut self, _addr: u32, _v: u8) {
        unreachable!("no memory write in these units")
    }
    fn write32(&mut self, _addr: u32, _v: u32) {
        unreachable!("no memory write in these units")
    }
}

const MARKER: u64 = 0x1234_5678;

#[test]
fn orig_102bc8_matches_the_game() {
    let Some(mut ee) = machine("oracle test of 0x102bc8") else { return };
    let log = log_hooks(&mut ee, &[(0x102778, 4)], MARKER);
    let mut r = Rng::new(0x102bc8);
    for n in 0..1000 {
        let mut tasks = Tasks::new();
        let mgr = tasks.create_manager(r.word() as u32);
        ee.w32(0x33e6e4, mgr);
        let ctl = AreaCtl { task_mgr: mgr, ..AreaCtl::default() };
        let bits = r.word() as u32;
        let prio = r.word() as u32 as i32;
        let function = r.word() as u32;
        log.borrow_mut().clear();
        let v0 = call(&mut ee, 0x102bc8, &[bits as u64, prio as i64 as u64, function as u64], &[]);
        let t = field::create_task(&mut tasks, &ctl, bits, prio, function);
        assert_eq!(v0, MARKER, "case {n}: the wrapper returns the creator's answer");
        let l = log.borrow();
        assert_eq!(l.len(), 1, "case {n}");
        let a = &l[0].1;
        let task = tasks.task(t).expect("task");
        assert_eq!((a[0] as u32, a[1] as u32, a[2] as u32 as i32, a[3] as u32), (mgr, bits, prio, function), "case {n}: arguments of the creator");
        assert_eq!((task.owner(), task.bits(), task.priority(), task.function()), (mgr, bits, prio, function), "case {n}: the port's task");
    }
}

fn random_ramp(ee: &mut Ee, r: &mut Rng) -> Ramp {
    let mut ramp = Ramp([r.float(), r.float(), r.float()]);
    // Make "reached" and "not reached" both common: the end near the start, the change of either sign.
    match r.below(4) {
        0 => ramp.0[2] = ramp.0[0],
        1 => ramp.0[1] = -ramp.0[1],
        _ => {}
    }
    for (i, f) in ramp.0.iter().enumerate() {
        ee.wf32(frame::SPEED_RAMP_BASE + 4 * i as u32, *f);
    }
    ramp
}

fn game_ramp(ee: &mut Ee) -> Ramp {
    Ramp([ee.f32_at(frame::SPEED_RAMP_BASE), ee.f32_at(frame::SPEED_RAMP_BASE + 4), ee.f32_at(frame::SPEED_RAMP_BASE + 8)])
}

fn same_ramp(game: &Ramp, port: &Ramp, n: u32, what: &str) {
    for i in 0..3 {
        assert_eq!(game.0[i].to_bits(), port.0[i].to_bits(), "{what} case {n}: word {i}: game {} port {}", game.0[i], port.0[i]);
    }
}

#[test]
fn orig_14ecd8_matches_the_game() {
    let Some(mut ee) = machine("oracle test of 0x14ecd8") else { return };
    let mut r = Rng::new(0x14ecd8);
    let mut tally = [0u32; 2];
    for n in 0..4000 {
        let mut ramp = random_ramp(&mut ee, &mut r);
        let v0 = call(&mut ee, 0x14ecd8, &[frame::SPEED_RAMP_BASE as u64], &[]) as u32;
        let reached = ramp.check();
        assert_eq!(v0 == 0, reached, "case {n}");
        same_ramp(&game_ramp(&mut ee), &ramp, n, "0x14ecd8");
        tally[reached as usize] += 1;
    }
    assert!(tally[0] > 300 && tally[1] > 300, "both answers ran: {tally:?}");
}

#[test]
fn orig_14ed20_matches_the_game() {
    let Some(mut ee) = machine("oracle test of 0x14ed20") else { return };
    let mut r = Rng::new(0x14ed20);
    let mut tally = [0u32; 2];
    for n in 0..4000 {
        let mut ramp = random_ramp(&mut ee, &mut r);
        let before = ramp;
        let v0 = call(&mut ee, 0x14ed20, &[frame::SPEED_RAMP_BASE as u64], &[]) as u32;
        let reached = ramp.advance();
        assert_eq!(v0 == 0, reached, "case {n}: v0 {v0:#x} ramp {:?} port {:?} game {:?}", before.0, ramp.0, game_ramp(&mut ee).0);
        same_ramp(&game_ramp(&mut ee), &ramp, n, "0x14ed20");
        tally[reached as usize] += 1;
    }
    assert!(tally[0] > 300 && tally[1] > 300, "both answers ran: {tally:?}");
}

/// A random frame block with the speed word as a normal float.
fn random_vsync(ee: &mut Ee, r: &mut Rng) -> Vsync {
    let mut v = Vsync::default();
    for b in v.0.iter_mut() {
        *b = r.word() as u8;
    }
    v.set_speed(r.float());
    if r.below(2) == 0 {
        v.0[4..8].fill(0);
    }
    ee.write(VS, &v.0);
    v
}

fn game_vsync(ee: &mut Ee) -> Vsync {
    Vsync(ee.read::<VS_LEN>(VS))
}

#[test]
fn orig_14cb08_matches_the_game() {
    let Some(mut ee) = machine("oracle test of 0x14cb08") else { return };
    let log = log_hooks(&mut ee, &[(0x102498, 1)], 0);
    let mut r = Rng::new(0x14cb08);
    let mut tally = [0u32; 2];
    for n in 0..3000 {
        let mut ramp = random_ramp(&mut ee, &mut r);
        let mut v = random_vsync(&mut ee, &mut r);
        let arg = r.word();
        log.borrow_mut().clear();
        call(&mut ee, 0x14cb08, &[arg], &[]);
        let mut host = Log::default();
        frame::game_speed_task(&mut ramp, &mut v, &mut host, arg);
        same_ramp(&game_ramp(&mut ee), &ramp, n, "0x14cb08");
        let gv = game_vsync(&mut ee);
        assert_eq!(gv, v, "case {n}: the frame block");
        assert_eq!(*log.borrow(), host.0, "case {n}: the print call");
        tally[host.0.len()] += 1;
    }
    assert!(tally[0] > 300 && tally[1] > 300, "both paths ran: {tally:?}");
}

#[test]
fn orig_14cb50_matches_the_game() {
    let Some(mut ee) = machine("oracle test of 0x14cb50") else { return };
    let log = log_hooks(&mut ee, &[(0x102778, 4)], MARKER);
    let mut r = Rng::new(0x14cb50);
    for n in 0..2000 {
        let mut tasks = Tasks::new();
        let root = tasks.create_manager(r.word() as u32);
        ee.w32(0x33e69c, root);
        let mut ramp = Ramp::default();
        for i in 0..3 {
            ee.wf32(frame::SPEED_RAMP_BASE + 4 * i, rf(&mut r));
        }
        let (a, b) = (rf(&mut r), rf(&mut r));
        let c = if r.below(4) == 0 { 0.0 } else { rf(&mut r) };
        log.borrow_mut().clear();
        call(&mut ee, 0x14cb50, &[], &[a, b, c]);
        let t = frame::game_speed_request(&mut ramp, &mut tasks, root, a, b, c);
        same_ramp(&game_ramp(&mut ee), &ramp, n, "0x14cb50");
        let l = log.borrow();
        assert_eq!(l.len(), 1, "case {n}");
        let task = tasks.task(t).expect("task");
        assert_eq!(l[0].1.iter().map(|x| *x as u32).collect::<Vec<_>>(), vec![root, 0, 800_000, 0x14cb08], "case {n}: arguments of the creator");
        assert_eq!((task.owner(), task.bits(), task.priority(), task.function()), (root, 0, 800_000, 0x14cb08), "case {n}: the port's task");
    }
}

#[test]
fn orig_14cc88_matches_the_game() {
    let Some(mut ee) = machine("oracle test of 0x14cc88") else { return };
    let log = log_hooks(&mut ee, &[(0x14d1f8, 0), (0x318650, 1)], 0);
    let mut r = Rng::new(0x14cc88);
    let mut tally = [0u32; 4];
    for n in 0..3000 {
        let mut save: Vec<u8> = (0..SAVE_LEN).map(|_| r.word() as u8).collect();
        // The play time sits near its cap often enough for the cap to matter.
        for off in [0x1604usize, 0x160c, 0x1610, 0x1614] {
            let v: u32 = match r.below(4) {
                0 => 0xcdf_e5c3,
                1 => 0xcdf_e5c4,
                2 => 0xcdf_e5c2,
                _ => r.below(0xd00_0000) as u32,
            };
            save[off..off + 4].copy_from_slice(&v.to_le_bytes());
        }
        ee.write(SAVE, &save);
        let counting = r.below(2) == 0;
        let world = if r.below(4) == 0 { r.word() as u8 } else { r.below(0x16) as u8 };
        ee.w32(0x33e6fc, if counting { r.word() as u32 | 1 } else { 0 });
        ee.write(0x33e830, &[world]);
        let mut v = random_vsync(&mut ee, &mut r);
        log.borrow_mut().clear();
        let v0 = call(&mut ee, 0x14cc88, &[], &[]);
        let mut host = Log::default();
        let p0 = frame::vblank_handler(&mut v, &mut save, counting, world, &mut host);
        assert_eq!(v0, p0, "case {n}: the result");
        assert_eq!(ee.read::<SAVE_LEN>(SAVE).to_vec(), save, "case {n}: the save block");
        assert_eq!(game_vsync(&mut ee), v, "case {n}: the frame block");
        let seen: Vec<(u32, Vec<u64>)> = log.borrow().iter().filter(|(a, _)| *a == 0x14d1f8).cloned().collect();
        assert_eq!(seen, host.0, "case {n}: the semaphore signal");
        let flushes: Vec<u64> = log.borrow().iter().filter(|(a, _)| *a == 0x318650).map(|(_, a)| a[0]).collect();
        assert_eq!(flushes, vec![0, 2], "case {n}: the two cache flushes");
        tally[(counting && world < 0x13) as usize + 2 * (v.paused() as usize)] += 1;
    }
    assert!(tally.iter().all(|&t| t > 100), "all paths ran: {tally:?}");
}
