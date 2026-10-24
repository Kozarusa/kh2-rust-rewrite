//! Oracle tests for the task and frame remainder (WP-L1-P20): kh2-ee runs the original from the user's executable and the port
//! (`kh2_game_core::{task, frame, pad}`) runs on the same inputs; return values, written words and blocks, and the ordered list of
//! calls a unit makes to other units are compared after every case.
//!
//! The harness is hand-written instead of `factory gen` because most of these functions work on linked lists, coroutine threads
//! and call sequences the generator cannot lay out. It follows the factory's rules: the original runs from the user's executable,
//! cases come from a seeded generator (a failing case number reproduces), and every compared case counts.
//!
//! Stand-ins (stubs), all documented here and nowhere else:
//! - Leaves and sequences: every function a unit calls that is not ported in this package is a stand-in that logs its address and
//!   arguments (the callers are listed in each test).
//! - The task blocks: the manager's allocator slots (`0x32c4b0` allocate, `0x32c4d8` free) are hooks that hand out zeroed blocks
//!   from arenas (tasks of 0x48 bytes, threads of any other size) and log the frees; the allocator object behind
//!   `FUN_001024a0` is a fake with one hooked method.
//! - The coroutine bodies of the runner tests are a short routine written into the original's memory by `Asm` below: it reads
//!   the next entry of a plan stream (return, yield n frames, wait t) and calls the original's own `FUN_00102438` or
//!   `FUN_00102460`. The context switches (`FUN_00105440`, `FUN_001054a8`, `FUN_00105510`) are NOT stubbed: they run natively in
//!   kh2-ee, so the runner, the yields and the finishing of a thread are the original's own code. The port's host follows the
//!   same plan stream.
//!
//! Skips without a disc (KH2_ISO or local/iso.txt); fails without one when KH2_REQUIRE_DISC=1.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use kh2_ee::Ee;
use kh2_factory::fuzz::{Rng, STACK};
use kh2_game_core::frame::{self, Ramp, Vsync};
use kh2_game_core::pad::{PadUnit, Vibration};
use kh2_game_core::task::modes::{self, Mode, ModeEnv, ModeThread, Modes, Step};
use kh2_game_core::task::*;

const SAVE: u32 = 0x33e860;
const SAVE_LEN: usize = 0x1700;
const OUT: u32 = 0x0180_0000;

fn machine(what: &str) -> Option<Ee> {
    let elf = kh2_factory::exe_or_skip(what)?;
    let mut ee = Ee::new();
    ee.load_elf(&elf).expect("elf");
    Some(ee)
}

fn call(ee: &mut Ee, addr: u32, args: &[u64], fargs: &[f32]) -> u64 {
    ee.call(addr, args, fargs, STACK).unwrap_or_else(|f| panic!("the original at {addr:#x} faulted: {f:?}"))
}

fn rnd_bytes(r: &mut Rng, n: usize) -> Vec<u8> {
    (0..n).map(|_| r.word() as u8).collect()
}

fn wide(r: &mut Rng) -> u64 {
    match r.below(6) {
        0 => 0,
        1 => 1,
        2 => 0xffff,
        3 => u64::MAX,
        4 => r.below(0x20000),
        _ => r.word(),
    }
}

/// Floats for the EE: finite, the factory's generator plus a few large and small magnitudes.
fn rf(r: &mut Rng) -> f32 {
    match r.below(12) {
        0 => 1e30,
        1 => -1e30,
        2 => 1e-30,
        _ => r.float(),
    }
}

/// Float words are compared within the rounding of the two FPUs (relative 1e-5) unless they are bit equal.
fn feq(a: f32, b: f32) -> bool {
    a.to_bits() == b.to_bits() || (a - b).abs() <= 1e-5 * (1.0 + a.abs().max(b.abs()))
}

#[derive(Default)]
struct Log(Vec<(u32, Vec<u64>)>);

impl CallHost for Log {
    fn call(&mut self, callee: u32, args: &[u64]) -> u64 {
        self.0.push((callee, args.to_vec()));
        0
    }
}

/// Hooks that log the call (callee and the first `n` argument registers) into `log`; they return 0.
fn log_hooks(ee: &mut Ee, log: &Rc<RefCell<Vec<(u32, Vec<u64>)>>>, addrs: &[(u32, usize)]) {
    for &(a, n) in addrs {
        let l = log.clone();
        ee.hooks.insert(a, move |ee: &mut Ee| {
            let args: Vec<u64> = (0..n).map(|i| ee.gpr[4 + i] as u64).collect();
            l.borrow_mut().push((a, args));
            ee.gpr[2] = 0;
        });
    }
}

// ---------------------------------------------------------------------------------------------------------------------------------
// Leaves

#[test]
fn orig_14c7e8_matches_the_game() {
    let Some(mut ee) = machine("oracle test of 0x14c7e8") else { return };
    let mut r = Rng::new(0x14c7e8);
    for n in 0..500 {
        let x = wide(&mut r);
        assert_eq!(call(&mut ee, 0x14c7e8, &[x], &[]), frame::clock_pass(x), "case {n}");
    }
}

#[test]
fn orig_14ccf0_matches_the_game() {
    let Some(mut ee) = machine("oracle test of 0x14ccf0") else { return };
    let mut r = Rng::new(0x14ccf0);
    for n in 0..200 {
        ee.gpr[2] = r.word() as u128;
        let (v0, a0) = (ee.gpr[2] as u64, r.word());
        let before = ee.ram[0x33e860..0x33e900].to_vec();
        call(&mut ee, 0x14ccf0, &[a0], &[]);
        assert_eq!(ee.gpr[2] as u64, v0, "case {n}: v0 untouched");
        assert_eq!(&ee.ram[0x33e860..0x33e900], &before[..], "case {n}: memory untouched");
        frame::frame_task_prelude();
    }
}

#[test]
fn orig_14eca0_matches_the_game() {
    let Some(mut ee) = machine("oracle test of 0x14eca0") else { return };
    let mut r = Rng::new(0x14eca0);
    for n in 0..4000 {
        let (a, b) = (rf(&mut r), rf(&mut r));
        let c = if r.below(4) == 0 { 0.0 } else { rf(&mut r) };
        ee.write(OUT, &rnd_bytes(&mut r, 16));
        let tail = ee.r32(OUT + 12);
        call(&mut ee, 0x14eca0, &[OUT as u64], &[a, b, c]);
        let game: Vec<f32> = (0..3).map(|i| ee.f32_at(OUT + 4 * i)).collect();
        let mut ramp = Ramp::default();
        ramp.set(a, b, c);
        for i in 0..3 {
            assert_eq!(game[i].to_bits(), ramp.0[i].to_bits(), "case {n}: word {i}: args {a} {b} {c}: game {} port {}", game[i], ramp.0[i]);
        }
        assert_eq!(ee.r32(OUT + 12), tail, "case {n}: the fourth word stays");
    }
}

#[test]
fn orig_1675e0_matches_the_game() {
    let Some(mut ee) = machine("oracle test of 0x1675e0") else { return };
    let mut r = Rng::new(0x1675e0);
    for n in 0..1000 {
        let i = match r.below(4) {
            0 => r.below(4) as i32,
            1 => -(r.below(4) as i32),
            _ => r.word() as i32,
        };
        let v = call(&mut ee, 0x1675e0, &[i as i64 as u64], &[]) as u32;
        assert_eq!(v, PadUnit::record_address(i), "case {n}: index {i}");
    }
}

#[test]
fn orig_167058_matches_the_game() {
    let Some(mut ee) = machine("oracle test of 0x167058") else { return };
    let mut r = Rng::new(0x167058);
    for n in 0..500 {
        let (a, b, c) = (rf(&mut r), rf(&mut r), rf(&mut r));
        for w in 0..3 {
            ee.w32(0x522168 + 4 * w, r.word() as u32);
        }
        call(&mut ee, 0x167058, &[], &[a, b, c]);
        let mut p = PadUnit::new([0; 6]);
        p.set_scale(a, b, c);
        for w in 0..3 {
            assert_eq!(ee.r32(0x522168 + 4 * w), p.scale[w as usize].to_bits(), "case {n}: word {w}");
        }
    }
}

#[test]
fn orig_102c30_matches_the_game() {
    let Some(mut ee) = machine("oracle test of 0x102c30") else { return };
    assert_eq!(call(&mut ee, 0x102c30, &[], &[]) as u32, field::mode_block());
}

/// Random (a, b) pairs with the one pair the static init reacts to over-represented.
fn init_pair(r: &mut Rng) -> (u64, u64) {
    match r.below(4) {
        0 => (1, 0xffff),
        1 => (1, wide(r)),
        2 => (wide(r), 0xffff),
        _ => (wide(r), wide(r)),
    }
}

#[test]
fn orig_102240_matches_the_game() {
    let Some(mut ee) = machine("oracle test of 0x102240") else { return };
    let mut r = Rng::new(0x102240);
    let mut fired = 0;
    for n in 0..1000 {
        let (a, b) = init_pair(&mut r);
        let (h, t) = (r.word() as u32 | 1, r.word() as u32 | 1);
        ee.w32(0x3990c0, h);
        ee.w32(0x3990c4, t);
        call(&mut ee, 0x102240, &[a, b], &[]);
        let mut m = Modes::new();
        m.head = h;
        m.tail = t;
        m.static_init(a, b);
        assert_eq!((ee.r32(0x3990c0), ee.r32(0x3990c4)), (m.head, m.tail), "case {n}: args {a:#x} {b:#x}");
        fired += (m.head == 0) as u32;
    }
    assert!(fired > 100, "the init branch ran {fired} times");
}

#[test]
fn orig_102268_matches_the_game() {
    let Some(mut ee) = machine("oracle test of 0x102268") else { return };
    let mut r = Rng::new(0x102268);
    for n in 0..50 {
        let (h, t) = (r.word() as u32 | 1, r.word() as u32 | 1);
        ee.w32(0x3990c0, h);
        ee.w32(0x3990c4, t);
        call(&mut ee, 0x102268, &[r.word()], &[]);
        let mut m = Modes::new();
        m.head = h;
        m.tail = t;
        m.static_init_wrapper();
        assert_eq!((ee.r32(0x3990c0), ee.r32(0x3990c4)), (m.head, m.tail), "case {n}");
        assert_eq!(m.head, 0);
    }
}

fn ramp_words(ee: &mut Ee) -> Ramp {
    Ramp([ee.f32_at(frame::SPEED_RAMP_BASE), ee.f32_at(frame::SPEED_RAMP_BASE + 4), ee.f32_at(frame::SPEED_RAMP_BASE + 8)])
}

fn ramp_check(addr: u32, wrapper: bool) {
    let Some(mut ee) = machine(&format!("oracle test of {addr:#x}")) else { return };
    let mut r = Rng::new(addr as u64);
    let mut fired = 0;
    for n in 0..1000 {
        let (a, b) = init_pair(&mut r);
        let start = [rf(&mut r), rf(&mut r), rf(&mut r)];
        for (i, &f) in start.iter().enumerate() {
            ee.wf32(frame::SPEED_RAMP_BASE + 4 * i as u32, f);
        }
        let mut port = Ramp(start);
        if wrapper {
            call(&mut ee, addr, &[a], &[]);
            port.static_init_wrapper();
        } else {
            call(&mut ee, addr, &[a, b], &[]);
            port.static_init(a, b);
        }
        let game = ramp_words(&mut ee);
        for i in 0..3 {
            assert_eq!(game.0[i].to_bits(), port.0[i].to_bits(), "case {n}: word {i}: args {a:#x} {b:#x}");
        }
        fired += (port.0[2] == frame::SPEED_RAMP_INIT_END && port.0 != Ramp(start).0) as u32;
    }
    assert!(fired > 50 || wrapper, "the init branch ran {fired} times");
}

#[test]
fn orig_14cfb0_matches_the_game() {
    ramp_check(0x14cfb0, false);
}

#[test]
fn orig_14cfe8_matches_the_game() {
    ramp_check(0x14cfe8, true);
}

fn random_save(ee: &mut Ee, r: &mut Rng) -> Vec<u8> {
    let mut s = rnd_bytes(r, SAVE_LEN);
    // Play times are below the cap in a real save, but the splitter takes any word.
    for off in [0x1604usize, 0x1608] {
        if r.below(2) == 0 {
            s[off..off + 4].copy_from_slice(&(r.below(0x0d00_0000) as u32).to_le_bytes());
        }
    }
    ee.write(SAVE, &s);
    s
}

#[test]
fn orig_14c910_matches_the_game() {
    let Some(mut ee) = machine("oracle test of 0x14c910") else { return };
    let mut r = Rng::new(0x14c910);
    for n in 0..300 {
        let s = random_save(&mut ee, &mut r);
        assert_eq!(call(&mut ee, 0x14c910, &[], &[]) as u32, frame::play_time(&s), "case {n}");
    }
}

#[test]
fn orig_14c930_matches_the_game() {
    let Some(mut ee) = machine("oracle test of 0x14c930") else { return };
    let mut r = Rng::new(0x14c930);
    for n in 0..300 {
        let s = random_save(&mut ee, &mut r);
        assert_eq!(call(&mut ee, 0x14c930, &[], &[]) as u32, frame::second_time(&s), "case {n}");
    }
}

fn hms_words(ee: &mut Ee) -> (u32, u32, u32, u32) {
    (ee.r32(OUT), ee.r32(OUT + 4), ee.r32(OUT + 8), ee.r32(OUT + 12))
}

#[test]
fn orig_14c950_matches_the_game() {
    let Some(mut ee) = machine("oracle test of 0x14c950") else { return };
    let mut r = Rng::new(0x14c950);
    for n in 0..500 {
        let s = random_save(&mut ee, &mut r);
        ee.write(OUT, &[0xaa; 16]);
        call(&mut ee, 0x14c950, &[OUT as u64, OUT as u64 + 4, OUT as u64 + 8, OUT as u64 + 12], &[]);
        assert_eq!(hms_words(&mut ee), frame::play_time_hms(&s), "case {n}: play time {:#x}", frame::play_time(&s));
    }
}

#[test]
fn orig_14c9b0_matches_the_game() {
    let Some(mut ee) = machine("oracle test of 0x14c9b0") else { return };
    let mut r = Rng::new(0x14c9b0);
    for n in 0..500 {
        let mut s = random_save(&mut ee, &mut r);
        // World counters are below the cap in a real save.
        for w in 0..0x20usize {
            if r.below(2) == 0 {
                let off = 0x160c + w * 4;
                s[off..off + 4].copy_from_slice(&(r.below(0x0d00_0000) as u32).to_le_bytes());
            }
        }
        ee.write(SAVE, &s);
        let world = r.below(0x20) as i32;
        ee.write(OUT, &[0xaa; 16]);
        call(&mut ee, 0x14c9b0, &[OUT as u64, OUT as u64 + 4, OUT as u64 + 8, OUT as u64 + 12, world as u64], &[]);
        assert_eq!(hms_words(&mut ee), frame::world_time_hms(&s, world), "case {n}: world {world}");
    }
}

// ---------------------------------------------------------------------------------------------------------------------------------
// Modes: the list, the callbacks

const MODE_ARENA: u32 = 0x0181_0000;
const CB: u32 = 0x0171_0100;

/// Builds a random mode list in the original's memory and the port's; returns the port's modes and their addresses (list order).
fn random_modes(ee: &mut Ee, r: &mut Rng, with_callbacks: bool) -> (Modes, Vec<u32>) {
    let n = r.below(6) as usize;
    let mut ids: Vec<u32> = (0..n as u32).map(|i| MODE_ARENA + i * 0x100).collect();
    // Shuffle the list order.
    for i in (1..ids.len()).rev() {
        ids.swap(i, r.below(i as u64 + 1) as usize);
    }
    let mut m = Modes::new();
    for (k, &id) in ids.iter().enumerate() {
        let mut b = Mode::default();
        for o in (0..modes::MODE_SIZE).step_by(4) {
            let w = r.word() as u32;
            b.0[o..o + 4].copy_from_slice(&w.to_le_bytes());
        }
        b.set_state([0, 1, 3, 1, 2][r.below(5) as usize]);
        b.set_next(ids.get(k + 1).copied().unwrap_or(0));
        if with_callbacks {
            b.set_callback20(if r.below(3) == 0 { 0 } else { CB });
            b.set_callback24(if r.below(3) == 0 { 0 } else { CB + 0x10 });
        }
        ee.write(id, &b.0);
        m.insert(id, b);
    }
    m.head = ids.first().copied().unwrap_or(0);
    m.tail = ids.last().copied().unwrap_or(0);
    ee.w32(modes::HEAD_WORD, m.head);
    ee.w32(modes::TAIL_WORD, m.tail);
    (m, ids)
}

#[test]
fn orig_102170_matches_the_game() {
    let Some(mut ee) = machine("oracle test of 0x102170") else { return };
    let mut r = Rng::new(0x102170);
    for n in 0..500 {
        let (m, _) = random_modes(&mut ee, &mut r, false);
        ee.write(OUT, &[0xaa; 64]);
        let count = call(&mut ee, 0x102170, &[OUT as u64], &[]) as u32;
        let mut out = Vec::new();
        assert_eq!(count as i32, m.running(&mut out), "case {n}: count");
        for (i, &id) in out.iter().enumerate() {
            assert_eq!(ee.r32(OUT + 4 * i as u32), id, "case {n}: entry {i}");
        }
        assert_eq!(ee.r32(OUT + 4 * out.len() as u32), 0xaaaa_aaaa, "case {n}: nothing past the last entry");
    }
}

#[test]
fn orig_102200_matches_the_game() {
    let Some(mut ee) = machine("oracle test of 0x102200") else { return };
    let mut r = Rng::new(0x102200);
    for n in 0..500 {
        let (m, _) = random_modes(&mut ee, &mut r, false);
        assert_eq!(call(&mut ee, 0x102200, &[], &[]) as u32, m.first_running(), "case {n}");
    }
}

fn callback_check(addr: u32, slot: u32) {
    let Some(mut ee) = machine(&format!("oracle test of {addr:#x}")) else { return };
    let log = Rc::new(RefCell::new(Vec::new()));
    log_hooks(&mut ee, &log, &[(CB, 1), (CB + 0x10, 1)]);
    let mut r = Rng::new(addr as u64);
    let mut called = 0;
    for n in 0..500 {
        let (m, ids) = random_modes(&mut ee, &mut r, true);
        let Some(&id) = ids.first() else { continue };
        let arg = wide(&mut r);
        log.borrow_mut().clear();
        call(&mut ee, addr, &[id as u64, arg], &[]);
        let mut port = Log::default();
        if slot == 0x20 {
            m.call_callback20(id, arg, &mut port);
        } else {
            m.call_callback24(id, arg, &mut port);
        }
        assert_eq!(*log.borrow(), port.0, "case {n}");
        called += port.0.len();
    }
    assert!(called > 100, "the callback was called {called} times");
}

#[test]
fn orig_102120_matches_the_game() {
    callback_check(0x102120, 0x20);
}

#[test]
fn orig_102148_matches_the_game() {
    callback_check(0x102148, 0x24);
}

fn mode_switch_check(addr: u32, calls: &[(u32, usize)], port: impl Fn(&mut Log)) {
    let Some(mut ee) = machine(&format!("oracle test of {addr:#x}")) else { return };
    let log = Rc::new(RefCell::new(Vec::new()));
    log_hooks(&mut ee, &log, calls);
    call(&mut ee, addr, &[], &[]);
    let mut p = Log::default();
    port(&mut p);
    assert_eq!(*log.borrow(), p.0);
    assert_eq!(p.0.len(), calls.len());
}

#[test]
fn orig_1021c0_matches_the_game() {
    mode_switch_check(0x1021c0, &[(0x167878, 0), (0x14a220, 0)], |p| modes::disable(p));
}

#[test]
fn orig_1021e0_matches_the_game() {
    mode_switch_check(0x1021e0, &[(0x1678b0, 0), (0x14a230, 0)], |p| modes::enable(p));
}
