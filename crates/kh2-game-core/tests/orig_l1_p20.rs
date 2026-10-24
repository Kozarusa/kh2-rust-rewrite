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
use kh2_game_core::area::ctl::{AreaCtl, AreaHost};
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

/// A shared log of stand-in calls: the callee and its argument registers.
type CallLog = Rc<RefCell<Vec<(u32, Vec<u64>)>>>;

/// Hooks that log the call (callee and the first `n` argument registers) into `log`; they return 0.
fn log_hooks(ee: &mut Ee, log: &CallLog, addrs: &[(u32, usize)]) {
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
        for (i, (g, p)) in game.iter().zip(ramp.0.iter()).enumerate() {
            assert_eq!(g.to_bits(), p.to_bits(), "case {n}: word {i}: args {a} {b} {c}: game {g} port {p}");
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
    mode_switch_check(0x1021c0, &[(0x167878, 0), (0x14a220, 0)], modes::disable);
}

#[test]
fn orig_1021e0_matches_the_game() {
    mode_switch_check(0x1021e0, &[(0x1678b0, 0), (0x14a230, 0)], modes::enable);
}

// ---------------------------------------------------------------------------------------------------------------------------------
// The task runner with coroutine threads: the original's own switch code runs natively.

const ALLOC: u32 = 0x0170_0000;
const VT: u32 = 0x0170_0100;
const ALLOC_FN: u32 = 0x0170_0200;
const PTR: u32 = 0x0171_0f00;
const BODY: u32 = 0x0171_1000;
const PLAIN: u32 = 0x0171_2000;
const ENTER: u32 = 0x0171_3000;
const EXIT: u32 = 0x0171_3010;
const MODE_VT: u32 = 0x0171_3100;
const PLAN: u32 = 0x0172_0000;
const PLAN_ENTRIES: u32 = 400;
const MGRS: u32 = 0x0180_0000;
const MODES_AT: u32 = 0x0188_0000;
const TASKS: u32 = 0x0190_0000;
const THREADS: u32 = 0x0140_0000;
const THREAD_STRIDE: u32 = 0x4000;
const ARENA_LEN: u32 = 0x0010_0000;
const RUNNING: u32 = 0x33e6c0;
const JMP_DEPTH: u32 = 0x33e858;
const JMP_BASE: u32 = 0x3b21d8;
const JMP_START: u32 = 8;
const STEP_WORD: u32 = 0x356f40;
const ROOT_WORD: u32 = 0x33e69c;
const NESTED_STACK: u32 = 0x01ef_0000;

/// Events: a plain task ran, a task block, a thread block or a manager block was released, a mode method was called.
const EV_PLAIN: u32 = 1;
const EV_TASK: u32 = 2;
const EV_THREAD: u32 = 3;
const EV_MGR: u32 = 4;
const EV_ENTER: u32 = 5;
const EV_EXIT: u32 = 6;
type Ev = (u32, u32, u32);

/// A tiny assembler for the coroutine body.
mod asm {
    fn i(op: u32, rs: u32, rt: u32, imm: i32) -> u32 {
        (op << 26) | (rs << 21) | (rt << 16) | (imm as u32 & 0xffff)
    }
    pub const ZERO: u32 = 0;
    pub const SP: u32 = 29;
    pub const RA: u32 = 31;
    pub const S0: u32 = 16;
    pub const A0: u32 = 4;
    pub const A1: u32 = 5;
    pub const T0: u32 = 8;
    pub const T1: u32 = 9;
    pub const T2: u32 = 10;
    pub const T3: u32 = 11;
    pub const T4: u32 = 12;
    pub fn addiu(rt: u32, rs: u32, imm: i32) -> u32 {
        i(9, rs, rt, imm)
    }
    pub fn lui(rt: u32, imm: u32) -> u32 {
        i(0xf, 0, rt, imm as i32)
    }
    pub fn lw(rt: u32, off: i32, rs: u32) -> u32 {
        i(0x23, rs, rt, off)
    }
    pub fn sw(rt: u32, off: i32, rs: u32) -> u32 {
        i(0x2b, rs, rt, off)
    }
    pub fn ld(rt: u32, off: i32, rs: u32) -> u32 {
        i(0x37, rs, rt, off)
    }
    pub fn sd(rt: u32, off: i32, rs: u32) -> u32 {
        i(0x3f, rs, rt, off)
    }
    pub fn beq(rs: u32, rt: u32, words: i32) -> u32 {
        i(4, rs, rt, words)
    }
    pub fn daddu(rd: u32, rs: u32, rt: u32) -> u32 {
        (rs << 21) | (rt << 16) | (rd << 11) | 0x2d
    }
    pub fn jal(addr: u32) -> u32 {
        (3 << 26) | (addr >> 2)
    }
    pub fn jr(rs: u32) -> u32 {
        (rs << 21) | 8
    }
    pub fn mtc1(rt: u32, fs: u32) -> u32 {
        (0x11 << 26) | (4 << 21) | (rt << 16) | (fs << 11)
    }
    pub const NOP: u32 = 0;
}

/// The coroutine body: each call reads the next two-word entry of the plan stream (kind, value) at the pointer word `PTR` and
/// advances the pointer; kind 0 returns, kind 1 calls `FUN_00102438(task, value)`, any other kind calls `FUN_00102460(value as a
/// float, task)`; after a yield it reads the next entry.
fn body_code() -> Vec<u32> {
    use asm::*;
    vec![
        addiu(SP, SP, -32),
        sd(RA, 0, SP),
        sd(S0, 8, SP),
        daddu(S0, A0, ZERO),
        // 4: loop
        lui(T0, PTR >> 16),
        lw(T1, (PTR & 0xffff) as i32, T0),
        lw(T2, 0, T1),
        lw(T3, 4, T1),
        addiu(T1, T1, 8),
        sw(T1, (PTR & 0xffff) as i32, T0),
        beq(T2, ZERO, 16), // 10 -> 27
        NOP,
        addiu(T4, ZERO, 2),
        beq(T2, T4, 7), // 13 -> 21
        NOP,
        daddu(A0, S0, ZERO),
        daddu(A1, T3, ZERO),
        jal(0x102438),
        NOP,
        beq(ZERO, ZERO, -16), // 19 -> 4
        NOP,
        // 21: wait by time
        mtc1(T3, 12),
        daddu(A0, S0, ZERO),
        jal(0x102460),
        NOP,
        beq(ZERO, ZERO, -22), // 25 -> 4
        NOP,
        // 27: return
        ld(RA, 0, SP),
        ld(S0, 8, SP),
        jr(RA),
        addiu(SP, SP, 32),
    ]
}

#[derive(Default)]
struct Sh {
    next_task: u32,
    next_thread: u32,
    next_mgr: u32,
    last_mgr: u32,
    log: Vec<Ev>,
}

fn sx(v: u32) -> u64 {
    v as i32 as i64 as u64
}

fn plan_entry(ee: &mut Ee) -> (u32, u32) {
    let p = ee.r32(PTR);
    ee.w32(PTR, p + 8);
    (ee.r32(p), ee.r32(p + 4))
}

/// What the original side needs to know to hook a mode (the mode thread tests): the plain tasks the enter method creates in the
/// mode's manager, (bits, priority) per mode address.
#[derive(Default)]
struct ModeSide {
    enter_plan: HashMap<u32, Vec<(u32, i32)>>,
}

/// The original and the port side by side.
struct TW {
    ee: Ee,
    sh: Rc<RefCell<Sh>>,
    ms: Rc<RefCell<ModeSide>>,
    run: PortRun,
    tasks: Tasks,
    compared: u64,
}

/// The port's host of the runner tests and the mode tests.
struct PortRun {
    map: HashMap<u32, u32>,
    plan: Vec<(u32, u32)>,
    pos: usize,
    log: Vec<Ev>,
    step: f32,
    next_task: u32,
    next_thread: u32,
    next_mgr: u32,
    last_jmp: Option<u32>,
    modes: Modes,
    threads: HashMap<u32, ModeThread>,
    enter_plan: HashMap<u32, Vec<(u32, i32)>>,
    /// Calls the port makes to other units from a task body (the field exit's release calls).
    calls: Log,
}

impl PortRun {
    fn new() -> PortRun {
        PortRun {
            map: HashMap::new(),
            plan: Vec::new(),
            pos: 0,
            log: Vec::new(),
            step: 1.0,
            next_task: 0,
            next_thread: 0,
            next_mgr: 0,
            last_jmp: None,
            modes: Modes::new(),
            threads: HashMap::new(),
            enter_plan: HashMap::new(),
            calls: Log::default(),
        }
    }
    fn tr(&self, h: u32) -> u32 {
        if h == 0 { 0 } else { self.map.get(&h).copied().unwrap_or(h) }
    }
    fn next(&mut self) -> (u32, u32) {
        let e = self.plan[self.pos];
        self.pos += 1;
        e
    }
    fn bind_task(&mut self, h: u32) {
        self.map.insert(h, TASKS + self.next_task * 0x1000);
        self.next_task += 1;
    }
    fn bind_thread(&mut self, h: u32) {
        self.map.insert(h, THREADS + self.next_thread * THREAD_STRIDE);
        self.next_thread += 1;
    }
    fn bind_mgr(&mut self, h: u32) {
        self.map.insert(h, MGRS + self.next_mgr * 0x1000);
        self.next_mgr += 1;
    }
    /// One plan entry applied to a body step: returns how the step ended.
    fn body_step(&mut self, tasks: &mut Tasks, task: u32) -> Switch {
        let (kind, value) = self.next();
        match kind {
            0 => Switch::Returned,
            1 => {
                tasks.yield_frames(task, value);
                Switch::Yielded
            }
            _ => {
                tasks.wait_time(task, f32::from_bits(value), self.step);
                Switch::Yielded
            }
        }
    }
    fn coroutine_step(&mut self, tasks: &mut Tasks, function: u32, task: u32, started: bool) -> Switch {
        let owner = tasks.task(task).unwrap().owner();
        assert_eq!(tasks.jump_top(), owner + 0x20, "the runner pushed the owner's jump buffer");
        self.last_jmp = Some(self.tr(owner) + 0x20);
        match function {
            BODY => self.body_step(tasks, task),
            modes::BODY => self.mode_step(tasks, task),
            field::EXIT_FN => {
                if started {
                    field::exit(&mut self.calls, tasks, task);
                    Switch::Yielded
                } else {
                    Switch::Returned
                }
            }
            f => panic!("unknown coroutine function {f:#x}"),
        }
    }
    /// One step of the mode thread of `task`, with the mode's manager operations done on `tasks` through this host.
    fn mode_step(&mut self, tasks: &mut Tasks, task: u32) -> Switch {
        let mode = tasks.task(task).unwrap().arg();
        let mut thread = self.threads.remove(&task).unwrap_or_default();
        let mut modes = std::mem::take(&mut self.modes);
        let step = {
            let mut env = Env { tasks, run: self };
            thread.step(task, mode, &mut modes, &mut env)
        };
        self.modes = modes;
        match step {
            Step::Yield(n) => {
                self.threads.insert(task, thread);
                tasks.yield_frames(task, n);
                Switch::Yielded
            }
            // The task slot is reused by later tasks: a finished thread leaves no state behind.
            Step::Done => Switch::Returned,
        }
    }
}

impl RunHost for PortRun {
    fn call(&mut self, tasks: &mut Tasks, function: u32, task: u32) {
        assert_eq!(function, PLAIN, "only plain stand-ins are scheduled");
        let a = self.tr(task);
        self.log.push((EV_PLAIN, a, 0));
        let (_, value) = self.next();
        if value & 1 == 1 {
            tasks.task_mut(task).unwrap().set_function(0);
        }
    }
    fn start(&mut self, tasks: &mut Tasks, function: u32, task: u32) -> Switch {
        self.coroutine_step(tasks, function, task, true)
    }
    fn resume(&mut self, tasks: &mut Tasks, task: u32) -> Switch {
        let function = tasks.task(task).unwrap().function();
        self.coroutine_step(tasks, function, task, false)
    }
    fn free_thread(&mut self, thread: u32) {
        let a = self.tr(thread);
        self.log.push((EV_THREAD, a, 0));
    }
    fn task_freed(&mut self, task: u32) {
        let a = self.tr(task);
        self.log.push((EV_TASK, a, 0));
    }
    fn manager_freed(&mut self, manager: u32) {
        let a = self.tr(manager);
        self.log.push((EV_MGR, a, 0));
    }
}

/// The mode thread's environment on the port side.
struct Env<'a> {
    tasks: &'a mut Tasks,
    run: &'a mut PortRun,
}

impl ModeEnv for Env<'_> {
    fn pass_all(&mut self, mgr: u32) {
        let step = self.run.step;
        self.tasks.pass_all(mgr, &mut Runner { host: self.run, step });
    }
    fn kill_all(&mut self, mgr: u32) {
        let step = self.run.step;
        self.tasks.kill_all(mgr, &mut Runner { host: self.run, step });
    }
    fn count(&mut self, mgr: u32) -> i64 {
        self.tasks.count(mgr) as i64
    }
    fn destroy(&mut self, mgr: u32) {
        let step = self.run.step;
        self.tasks.destroy_manager(mgr, &mut Runner { host: self.run, step });
    }
    fn enter(&mut self, modes: &mut Modes, mode: u32, task: u32) {
        let a = self.run.tr(mode);
        let t = self.run.tr(task);
        self.run.log.push((EV_ENTER, a, t));
        let m = self.tasks.create_manager(ALLOC);
        self.run.bind_mgr(m);
        modes.mode_mut(mode).set_manager(m);
        for (bits, prio) in self.run.enter_plan.get(&mode).cloned().unwrap_or_default() {
            let t = self.tasks.create_task(m, bits, prio, PLAIN);
            self.run.bind_task(t);
        }
    }
    fn exit(&mut self, _modes: &mut Modes, mode: u32, task: u32) {
        let a = self.run.tr(mode);
        let t = self.run.tr(task);
        self.run.log.push((EV_EXIT, a, t));
    }
}

impl TW {
    fn new(what: &str) -> Option<TW> {
        let mut ee = machine(what)?;
        let sh = Rc::new(RefCell::new(Sh::default()));
        let ms = Rc::new(RefCell::new(ModeSide::default()));
        // The allocator slots of the managers.
        let s = sh.clone();
        ee.hooks.insert(0x32c4b0, move |ee: &mut Ee| {
            let size = ee.gpr[5] as u32;
            let a = {
                let mut s = s.borrow_mut();
                if size == 0x48 {
                    let a = TASKS + s.next_task * 0x1000;
                    s.next_task += 1;
                    a
                } else {
                    assert!(size <= THREAD_STRIDE, "thread block of {size:#x} bytes");
                    let a = THREADS + s.next_thread * THREAD_STRIDE;
                    s.next_thread += 1;
                    a
                }
            };
            ee.write(a, &vec![0u8; if size == 0x48 { 0x1000 } else { size as usize }]);
            ee.gpr[2] = a as u128;
        });
        let s = sh.clone();
        ee.hooks.insert(ALLOC_FN, move |ee: &mut Ee| {
            assert_eq!(ee.gpr[5] as u32, 0x120, "the manager block is 0x120 bytes");
            let a = {
                let mut s = s.borrow_mut();
                let a = MGRS + s.next_mgr * 0x1000;
                s.next_mgr += 1;
                s.last_mgr = a;
                a
            };
            ee.write(a, &[0u8; 0x1000]);
            ee.gpr[2] = a as u128;
        });
        let s = sh.clone();
        ee.hooks.insert(0x32c4d8, move |ee: &mut Ee| {
            let (owner, p) = (ee.gpr[4] as u32, ee.gpr[5] as u32);
            let kind = if (TASKS..TASKS + ARENA_LEN).contains(&p) {
                EV_TASK
            } else if (THREADS..THREADS + ARENA_LEN).contains(&p) {
                EV_THREAD
            } else {
                assert_eq!(p, owner, "a free of something that is neither a task, a thread nor the manager");
                EV_MGR
            };
            s.borrow_mut().log.push((kind, p, 0));
        });
        // A plain task: logs, then finishes itself when the next plan entry's value is odd.
        let s = sh.clone();
        ee.hooks.insert(PLAIN, move |ee: &mut Ee| {
            let task = ee.gpr[4] as u32;
            s.borrow_mut().log.push((EV_PLAIN, task, 0));
            let (_, value) = plan_entry(ee);
            if value & 1 == 1 {
                ee.w32(task, 0);
            }
        });
        // The mode methods: enter makes the mode's manager (through the original) and its plain tasks, exit logs.
        let (s, m) = (sh.clone(), ms.clone());
        ee.hooks.insert(ENTER, move |ee: &mut Ee| {
            let (mode, task) = (ee.gpr[4] as u32, ee.gpr[5] as u32);
            s.borrow_mut().log.push((EV_ENTER, mode, task));
            ee.call(0x1024a0, &[ALLOC as u64], &[], NESTED_STACK).unwrap();
            let mgr = s.borrow().last_mgr;
            ee.w32(mode + 4, mgr);
            let plan = m.borrow().enter_plan.get(&mode).cloned().unwrap_or_default();
            for (bits, prio) in plan {
                ee.call(0x102778, &[mgr as u64, bits as u64, sx(prio as u32), PLAIN as u64], &[], NESTED_STACK).unwrap();
            }
        });
        let s = sh.clone();
        ee.hooks.insert(EXIT, move |ee: &mut Ee| {
            s.borrow_mut().log.push((EV_EXIT, ee.gpr[4] as u32, ee.gpr[5] as u32));
        });
        ee.w32(ALLOC, VT);
        ee.w32(VT, ALLOC_FN);
        ee.w32(MODE_VT + 4, ENTER);
        ee.w32(MODE_VT + 8, EXIT);
        for (i, w) in body_code().into_iter().enumerate() {
            ee.w32(BODY + 4 * i as u32, w);
        }
        Some(TW { ee, sh, ms, run: PortRun::new(), tasks: Tasks::new(), compared: 0 })
    }

    /// Fresh state for a case: the arenas' counters, the jump stack, the step, and a random plan stream on both sides.
    fn reset(&mut self, r: &mut Rng, step: f32) {
        *self.sh.borrow_mut() = Sh::default();
        self.ms.borrow_mut().enter_plan.clear();
        self.run = PortRun::new();
        self.tasks = Tasks::new();
        self.ee.w32(RUNNING, 0);
        self.ee.w32(JMP_DEPTH, JMP_START);
        self.ee.w32(STEP_WORD, step.to_bits());
        self.ee.w32(modes::HEAD_WORD, 0);
        self.ee.w32(modes::TAIL_WORD, 0);
        self.run.step = step;
        let mut plan = Vec::new();
        for _ in 0..PLAN_ENTRIES {
            let e = match r.below(10) {
                0..=1 => (0, r.word() as u32),
                2..=6 => (1, r.below(4) as u32),
                _ => (2, (r.float().abs() * 2.0).to_bits()),
            };
            plan.push(e);
        }
        for (i, &(k, v)) in plan.iter().enumerate() {
            self.ee.w32(PLAN + 8 * i as u32, k);
            self.ee.w32(PLAN + 8 * i as u32 + 4, v);
        }
        self.ee.w32(PTR, PLAN);
        self.run.plan = plan;
    }

    fn call(&mut self, addr: u32, args: &[u64]) -> u32 {
        call(&mut self.ee, addr, args, &[]) as u32
    }

    fn manager(&mut self) -> u32 {
        self.call(0x1024a0, &[ALLOC as u64]);
        let em = self.sh.borrow().last_mgr;
        let h = self.tasks.create_manager(ALLOC);
        self.run.bind_mgr(h);
        assert_eq!(self.run.tr(h), em, "manager addresses run in step");
        h
    }

    fn plain(&mut self, mgr: u32, bits: u32, prio: i32) -> u32 {
        let m = self.run.tr(mgr);
        let e = self.call(0x102778, &[m as u64, bits as u64, sx(prio as u32), PLAIN as u64]);
        let h = self.tasks.create_task(mgr, bits, prio, PLAIN);
        self.run.bind_task(h);
        assert_eq!(e, self.run.tr(h), "the original returns the new task");
        h
    }

    fn coroutine(&mut self, mgr: u32, bits: u32, prio: i32, function: u32, stack: u32) -> u32 {
        let m = self.run.tr(mgr);
        let e = self.call(0x102808, &[m as u64, bits as u64, sx(prio as u32), function as u64, stack as u64]);
        let h = self.tasks.create_coroutine(mgr, bits, prio, function, stack);
        let th = self.tasks.task(h).unwrap().thread();
        self.run.bind_thread(th);
        self.run.bind_task(h);
        assert_eq!(e, self.run.tr(h), "the original returns the new task");
        h
    }

    /// Random tasks on `mgr`: plain ones and body coroutines; returns the handles.
    fn fill(&mut self, r: &mut Rng, mgr: u32, max: u64) -> Vec<u32> {
        let mut out = Vec::new();
        for _ in 0..r.below(max + 1) {
            let bits = if r.below(3) == 0 { 0 } else { r.below(16) as u32 };
            let prio = r.below(5) as i32 - 2;
            let h = if r.below(3) == 0 {
                self.plain(mgr, bits, prio)
            } else {
                let stack = [0x400, 0x800, 0x1000, 0x2000][r.below(4) as usize];
                self.coroutine(mgr, bits, prio, BODY, stack)
            };
            out.push(h);
        }
        out
    }

    /// One pass on both sides: all tasks, or by mask and value.
    fn pass(&mut self, mgr: u32, mask: u32, value: u32, all: bool) {
        let m = self.run.tr(mgr);
        let step = self.run.step;
        if all {
            self.call(0x1029d8, &[m as u64]);
            self.tasks.pass_all(mgr, &mut Runner { host: &mut self.run, step });
        } else {
            self.call(0x1028a0, &[m as u64, sx(mask), sx(value)]);
            self.tasks.pass(mgr, mask, value, &mut Runner { host: &mut self.run, step });
        }
    }

    /// Compares everything the original can have touched.
    fn verify(&mut self, case: u32) {
        let tr = |run: &PortRun, b: &mut [u8], offs: &[usize]| {
            for &o in offs {
                let v = u32::from_le_bytes(b[o..o + 4].try_into().unwrap());
                b[o..o + 4].copy_from_slice(&run.tr(v).to_le_bytes());
            }
        };
        for m in self.tasks.live_managers() {
            let a = self.run.map[&m];
            let mut mine = self.tasks.manager(m).unwrap().0;
            tr(&self.run, &mut mine, &[8, 0xc, 0x10, 0x110]);
            let game: Vec<u8> = (0..MANAGER_SIZE as u32).map(|i| self.ee.r8(a + i)).collect();
            // Bytes 0x20..0x100 are the register save area of the context switch (hardware): not modelled.
            if let Some(o) = (0..MANAGER_SIZE).filter(|o| !(0x20..0x100).contains(o)).find(|&o| game[o] != mine[o]) {
                panic!("case {case}: manager {a:#x} byte +{o:#x}: game {:02x} port {:02x}", game[o], mine[o]);
            }
        }
        for t in self.tasks.live_tasks() {
            let a = self.run.map[&t];
            let mut mine = self.tasks.task(t).unwrap().0;
            tr(&self.run, &mut mine, &[0x24, 0x34, 0x38, 0x3c]);
            let game: Vec<u8> = (0..TASK_SIZE as u32).map(|i| self.ee.r8(a + i)).collect();
            if let Some(o) = (0..TASK_SIZE).find(|&o| game[o] != mine[o]) {
                panic!("case {case}: task {a:#x} byte +{o:#x}: game {:02x} port {:02x}", game[o], mine[o]);
            }
            let th = self.tasks.task(t).unwrap().thread();
            if th != 0 {
                let ta = self.run.map[&th];
                let mine = self.tasks.thread(th).unwrap().0;
                let game: Vec<u8> = (0..THREAD_HEADER as u32).map(|i| self.ee.r8(ta + i)).collect();
                if let Some(o) = (0xf0..THREAD_HEADER).find(|&o| game[o] != mine[o]) {
                    panic!("case {case}: thread {ta:#x} byte +{o:#x}: game {:02x} port {:02x}", game[o], mine[o]);
                }
            }
            self.compared += 1;
        }
        assert_eq!(self.ee.r32(RUNNING), self.run.tr(self.tasks.running), "case {case}: the running-task word");
        assert_eq!(self.ee.r32(JMP_DEPTH), JMP_START, "case {case}: the jump stack is back where it started");
        assert_eq!(self.ee.r32(PTR), PLAN + 8 * self.run.pos as u32, "case {case}: plan entries consumed");
        if let Some(j) = self.run.last_jmp {
            assert_eq!(self.ee.r32(JMP_BASE + 4 * (JMP_START - 1)), j, "case {case}: the pushed jump buffer");
        }
        let game = std::mem::take(&mut self.sh.borrow_mut().log);
        assert_eq!(game, self.run.log, "case {case}: events (kind, a, b) game vs port");
        self.run.log.clear();
    }
}

fn random_step(r: &mut Rng) -> f32 {
    match r.below(5) {
        0 => 1.0,
        1 => 2.0,
        2 => 0.5,
        3 => 0.0,
        _ => r.float().abs() * 2.0,
    }
}

#[test]
fn orig_102808_matches_the_game() {
    let Some(mut w) = TW::new("oracle test of 0x102808") else { return };
    for n in 0..1000u32 {
        let mut r = Rng::new(0x102808 + n as u64);
        let step = random_step(&mut r);
        w.reset(&mut r, step);
        let m = w.manager();
        let hs = w.fill(&mut r, m, 5);
        for &h in &hs {
            let t = w.tasks.task(h).unwrap();
            assert_eq!(t.thread() != 0, t.function() == BODY, "case {n}: coroutines have threads");
            if t.thread() != 0 {
                let th = w.tasks.thread(t.thread()).unwrap();
                assert!(th.first_run() == 0 && th.wait() == 0 && th.timer() == 0.0 && th.stack() >= 0x400);
            }
        }
        w.verify(n);
    }
    assert!(w.compared > 0);
}

/// Cases of the runner: tasks on a manager, then passes (all, or by mask and value), compared after every pass. Returns how often
/// each event kind happened, to check the generator reaches the paths.
fn runner_cases(w: &mut TW, cases: u32, seed: u64) -> [u64; 8] {
    let mut tally = [0u64; 8];
    for n in 0..cases {
        let mut r = Rng::new(seed + n as u64);
        let step = random_step(&mut r);
        w.reset(&mut r, step);
        let m = w.manager();
        let _ = w.fill(&mut r, m, 6);
        for _ in 0..1 + r.below(7) {
            let all = r.below(2) == 0;
            let (mask, value) = if r.below(2) == 0 { (1u32 << r.below(4), 0) } else { (r.below(16) as u32, r.below(16) as u32) };
            w.pass(m, mask, value, all);
            for e in &w.run.log {
                tally[(e.0 as usize).min(7)] += 1;
            }
            w.verify(n);
        }
    }
    tally
}

#[test]
fn orig_102340_matches_the_game() {
    let Some(mut w) = TW::new("oracle test of 0x102340") else { return };
    let t = runner_cases(&mut w, 3000, 0x102340);
    eprintln!("runner events {t:?}, tasks compared {}", w.compared);
    assert!(t[EV_PLAIN as usize] > 500 && t[EV_TASK as usize] > 500 && t[EV_THREAD as usize] > 200, "the generator reaches the paths: {t:?}");
    assert!(w.compared > 5000);
}

// ---------------------------------------------------------------------------------------------------------------------------------
// Yield and wait, with the context switch itself stubbed (a hook that logs the two buffers it would switch between).

fn switch_world(what: &str) -> Option<(TW, CallLog)> {
    let mut w = TW::new(what)?;
    let log = Rc::new(RefCell::new(Vec::new()));
    log_hooks(&mut w.ee, &log, &[(0x105510, 2)]);
    Some((w, log))
}

#[test]
fn orig_102438_matches_the_game() {
    let Some((mut w, log)) = switch_world("oracle test of 0x102438") else { return };
    for n in 0..1000u32 {
        let mut r = Rng::new(0x102438 + n as u64);
        w.reset(&mut r, 1.0);
        log.borrow_mut().clear();
        let m = w.manager();
        let t = w.coroutine(m, 0, 0, BODY, 0x400);
        let frames = match r.below(3) {
            0 => r.below(5) as u32,
            1 => r.word() as u32,
            _ => (-(r.below(5) as i32)) as u32,
        };
        let task = w.run.tr(t);
        w.call(0x102438, &[task as u64, frames as u64]);
        w.tasks.yield_frames(t, frames);
        let th = w.tasks.task(t).unwrap().thread();
        assert_eq!(w.tasks.thread(th).unwrap().wait(), frames as i32);
        let expect = vec![(0x105510, vec![w.run.tr(th) as u64, w.run.tr(m) as u64 + 0x20])];
        assert_eq!(*log.borrow(), expect, "case {n}: the switch is asked for between the thread and the owner's buffer");
        w.verify(n);
    }
}

#[test]
fn orig_102460_matches_the_game() {
    let Some((mut w, log)) = switch_world("oracle test of 0x102460") else { return };
    for n in 0..1000u32 {
        let mut r = Rng::new(0x102460 + n as u64);
        let step = random_step(&mut r);
        w.reset(&mut r, step);
        log.borrow_mut().clear();
        let m = w.manager();
        let t = w.coroutine(m, 0, 0, BODY, 0x400);
        let time = rf(&mut r);
        let task = w.run.tr(t);
        call(&mut w.ee, 0x102460, &[task as u64], &[time]);
        w.tasks.wait_time(t, time, step);
        let th = w.tasks.task(t).unwrap().thread();
        let expect = vec![(0x105510, vec![w.run.tr(th) as u64, w.run.tr(m) as u64 + 0x20])];
        assert_eq!(*log.borrow(), expect, "case {n}");
        w.verify(n);
    }
}

// ---------------------------------------------------------------------------------------------------------------------------------
// Game modes: start (0x102070) and the thread body (0x101e98) with the original's own coroutines.

impl TW {
    /// A mode object at slot `i` on both sides; returns its address (the port's id).
    fn mode(&mut self, r: &mut Rng, i: u32) -> u32 {
        let id = MODES_AT + i * 0x100;
        let mut b = Mode::default();
        b.set_vtable(MODE_VT);
        b.set_mask([1, 2, 4, 3, 6, 8][r.below(6) as usize]);
        b.set_priority(r.below(5) as u32);
        b.set_state(if r.below(6) == 0 { 3 } else { 0 });
        b.set_stack([0x800, 0x1000, 0x2000][r.below(3) as usize]);
        b.set_callback20(r.word() as u32);
        b.set_callback24(r.word() as u32);
        self.ee.write(id, &b.0);
        self.run.modes.insert(id, b);
        let mut plan = Vec::new();
        for _ in 0..r.below(4) {
            plan.push((r.below(16) as u32, r.below(5) as i32 - 2));
        }
        self.ms.borrow_mut().enter_plan.insert(id, plan.clone());
        self.run.enter_plan.insert(id, plan);
        id
    }

    /// Starts the mode on both sides; returns the stand-in call logs (game, port).
    fn start_mode(&mut self, mode: u32, root: u32, calls: &CallLog) -> Vec<(u32, Vec<u64>)> {
        calls.borrow_mut().clear();
        let before = self.tasks.live_tasks();
        self.call(0x102070, &[mode as u64]);
        let mut host = Log::default();
        let mut modes = std::mem::take(&mut self.run.modes);
        let t = modes.start(mode, &mut self.tasks, root, &mut host);
        self.run.modes = modes;
        if let Some(t) = t {
            let th = self.tasks.task(t).unwrap().thread();
            self.run.bind_thread(th);
            self.run.bind_task(t);
            assert!(!before.contains(&t));
            let e = self.sh.borrow().next_task;
            assert_eq!(self.run.tr(t) + 0x1000, TASKS + e * 0x1000, "the original made the same task");
        }
        assert_eq!(*calls.borrow(), host.0, "stand-in calls of the start");
        host.0
    }

    fn verify_modes(&mut self, case: u32, ids: &[u32]) {
        for &id in ids {
            let mut mine = self.run.modes.mode(id).0;
            let v = u32::from_le_bytes(mine[4..8].try_into().unwrap());
            mine[4..8].copy_from_slice(&self.run.tr(v).to_le_bytes());
            let game: Vec<u8> = (0..modes::MODE_SIZE as u32).map(|i| self.ee.r8(id + i)).collect();
            if let Some(o) = (0..modes::MODE_SIZE).find(|&o| game[o] != mine[o]) {
                panic!("case {case}: mode {id:#x} byte +{o:#x}: game {:02x} port {:02x}", game[o], mine[o]);
            }
        }
        assert_eq!((self.ee.r32(modes::HEAD_WORD), self.ee.r32(modes::TAIL_WORD)), (self.run.modes.head, self.run.modes.tail), "case {case}: list words");
    }
}

/// Cases with several modes: starts at random passes, stop requests at random passes, passes over the root manager.
fn mode_cases(w: &mut TW, cases: u32, seed: u64) -> [u64; 8] {
    let calls = Rc::new(RefCell::new(Vec::new()));
    log_hooks(&mut w.ee, &calls, &[(0x167878, 0), (0x14a220, 0)]);
    let mut tally = [0u64; 8];
    for n in 0..cases {
        let mut r = Rng::new(seed + n as u64);
        let step = random_step(&mut r);
        w.reset(&mut r, step);
        let root = w.manager();
        let ra = w.run.tr(root);
        w.ee.w32(ROOT_WORD, ra);
        let count = 1 + r.below(4) as u32;
        let ids: Vec<u32> = (0..count).map(|i| w.mode(&mut r, i)).collect();
        let passes = 6 + r.below(30);
        let mut start_at: Vec<(u64, u32)> = ids.iter().map(|&id| (r.below(passes / 2 + 1), id)).collect();
        start_at.sort();
        let mut next_start = 0;
        for p in 0..passes {
            while next_start < start_at.len() && start_at[next_start].0 <= p {
                let id = start_at[next_start].1;
                let started = w.run.modes.mode(id).state() != 3;
                w.start_mode(id, root, &calls);
                tally[0] += started as u64;
                w.verify(n);
                w.verify_modes(n, &ids);
                next_start += 1;
            }
            // A stop request for a running mode.
            if r.below(6) == 0 {
                let id = ids[r.below(ids.len() as u64) as usize];
                if w.run.modes.mode(id).state() == 1 {
                    w.ee.w32(id + 0x1c, 3);
                    w.run.modes.mode_mut(id).set_state(3);
                    tally[1] += 1;
                }
            }
            w.pass(root, 0, 0, true);
            for e in &w.run.log {
                tally[(e.0 as usize).min(7)] += 1;
            }
            w.verify(n);
            w.verify_modes(n, &ids);
        }
    }
    tally
}

#[test]
fn orig_102070_matches_the_game() {
    let Some(mut w) = TW::new("oracle test of 0x102070") else { return };
    let t = mode_cases(&mut w, 1500, 0x102070);
    eprintln!("mode events {t:?}, tasks compared {}", w.compared);
    assert!(t[0] > 1000, "modes were started");
}

#[test]
fn orig_101e98_matches_the_game() {
    let Some(mut w) = TW::new("oracle test of 0x101e98") else { return };
    let t = mode_cases(&mut w, 3000, 0x101e98);
    eprintln!("mode events {t:?}, tasks compared {}", w.compared);
    assert!(t[EV_ENTER as usize] > 1000 && t[EV_EXIT as usize] > 500 && t[1] > 300, "the generator reaches enter, exit and stop: {t:?}");
}

// ---------------------------------------------------------------------------------------------------------------------------------
// Units that call other units: the original's callees are stand-ins that log and answer from a seeded policy; the port's host does
// the same. Memory windows are copied from the original's memory to the port before a case and compared byte by byte after it.

fn mix(a: u64, b: u64, c: u64) -> u64 {
    let mut x = a ^ b.wrapping_mul(0x9e37_79b9_7f4a_7c15) ^ c.wrapping_mul(0xbf58_476d_1ce4_e5b9);
    x ^= x >> 31;
    x = x.wrapping_mul(0x94d0_49bb_1331_11eb);
    x ^ (x >> 29)
}

type HEv = (u32, Vec<u32>);

/// The answers of the stand-ins, shared by the two sides.
#[derive(Default)]
struct Pol {
    seed: u64,
    counts: HashMap<u32, u32>,
    /// Per callee: calls with an index below `n` answer `before`, later ones `after`.
    rules: HashMap<u32, (u32, u64, u64)>,
}

impl Pol {
    fn ret(&mut self, addr: u32) -> u64 {
        let k = {
            let c = self.counts.entry(addr).or_insert(0);
            let k = *c;
            *c += 1;
            k
        };
        match self.rules.get(&addr) {
            Some(&(n, before, after)) => {
                if k < n {
                    before
                } else {
                    after
                }
            }
            None => mix(self.seed, addr as u64, k as u64) % 3,
        }
    }
}

#[derive(Default)]
struct Stubs {
    pol: Pol,
    log: Vec<HEv>,
}

fn install_stubs(ee: &mut Ee, st: &Rc<RefCell<Stubs>>, addrs: &[u32]) {
    for &addr in addrs {
        let s = st.clone();
        ee.hooks.insert(addr, move |ee: &mut Ee| {
            let args: Vec<u32> = (4..10).map(|i| ee.gpr[i] as u32).collect();
            let mut s = s.borrow_mut();
            s.log.push((addr, args));
            let v = s.pol.ret(addr);
            ee.gpr[2] = v as i64 as u128;
        });
    }
}

/// The port's host: memory windows and the same call policy.
struct PortHost {
    regs: Vec<(u32, Vec<u8>)>,
    pol: Pol,
    log: Vec<HEv>,
}

impl PortHost {
    fn snapshot(ee: &mut Ee, wins: &[(u32, usize)]) -> PortHost {
        let regs = wins.iter().map(|&(a, n)| (a, (0..n as u32).map(|i| ee.r8(a + i)).collect())).collect();
        PortHost { regs, pol: Pol::default(), log: Vec::new() }
    }
    fn slot(&mut self, a: u32, n: usize) -> &mut [u8] {
        for (base, d) in self.regs.iter_mut() {
            if a >= *base && (a - *base) as usize + n <= d.len() {
                let o = (a - *base) as usize;
                return &mut d[o..o + n];
            }
        }
        panic!("the port touched {a:#x} (+{n}), outside every window")
    }
    fn bytes(&mut self, a: u32, n: usize) -> Vec<u8> {
        self.slot(a, n).to_vec()
    }
    fn put(&mut self, a: u32, d: &[u8]) {
        self.slot(a, d.len()).copy_from_slice(d)
    }
}

impl AreaHost for PortHost {
    fn call(&mut self, callee: u32, args: &[u64]) -> u64 {
        self.log.push((callee, args.iter().map(|a| *a as u32).collect()));
        self.pol.ret(callee)
    }
    fn vcall(&mut self, _obj: u32, _slot: u32, _args: &[u64]) -> u64 {
        panic!("no virtual calls in these units")
    }
    fn read8(&mut self, a: u32) -> u8 {
        self.slot(a, 1)[0]
    }
    fn read16(&mut self, a: u32) -> u16 {
        u16::from_le_bytes(self.slot(a, 2).try_into().unwrap())
    }
    fn read32(&mut self, a: u32) -> u32 {
        u32::from_le_bytes(self.slot(a, 4).try_into().unwrap())
    }
    fn write8(&mut self, a: u32, v: u8) {
        self.slot(a, 1)[0] = v
    }
    fn write32(&mut self, a: u32, v: u32) {
        self.slot(a, 4).copy_from_slice(&v.to_le_bytes())
    }
}

/// Compares the call logs and the memory windows of a finished case.
fn finish(ee: &mut Ee, st: &Rc<RefCell<Stubs>>, port: &PortHost, case: u32, what: &str) {
    let g = st.borrow().log.clone();
    let p = &port.log;
    assert_eq!(g.len(), p.len(), "{what} case {case}: number of calls\n game {g:x?}\n port {p:x?}");
    for (k, (ge, pe)) in g.iter().zip(p.iter()).enumerate() {
        assert!(
            ge.0 == pe.0 && ge.1.len() >= pe.1.len() && ge.1[..pe.1.len()] == pe.1[..],
            "{what} case {case}: call {k}\n game {ge:x?}\n port {pe:x?}\n all game {g:x?}\n all port {p:x?}"
        );
    }
    for (a, d) in &port.regs {
        for (i, &b) in d.iter().enumerate() {
            let gb = ee.r8(a + i as u32);
            assert_eq!(gb, b, "{what} case {case}: byte {:#x}: game {gb:02x} port {b:02x}\n calls {g:x?}", a + i as u32);
        }
    }
}

/// Fills `wins` of the original's memory with random bytes and starts a case: stubs reset, policy seeded.
fn begin(ee: &mut Ee, st: &Rc<RefCell<Stubs>>, r: &mut Rng, wins: &[(u32, usize)], rules: &[(u32, (u32, u64, u64))]) -> PortHost {
    for &(a, n) in wins {
        ee.write(a, &rnd_bytes(r, n));
    }
    host_policy(ee, st, r, wins, rules)
}

/// As [`begin`] but the windows already hold the case's memory.
fn host_policy(ee: &mut Ee, st: &Rc<RefCell<Stubs>>, r: &mut Rng, wins: &[(u32, usize)], rules: &[(u32, (u32, u64, u64))]) -> PortHost {
    let seed = r.word();
    {
        let mut s = st.borrow_mut();
        s.log.clear();
        s.pol = Pol { seed, counts: HashMap::new(), rules: rules.iter().cloned().collect() };
    }
    let mut p = PortHost::snapshot(ee, wins);
    p.pol = Pol { seed, counts: HashMap::new(), rules: rules.iter().cloned().collect() };
    p
}

fn stubbed(what: &str, addrs: &[u32]) -> Option<(Ee, Rc<RefCell<Stubs>>)> {
    let mut ee = machine(what)?;
    let st = Rc::new(RefCell::new(Stubs::default()));
    install_stubs(&mut ee, &st, addrs);
    Some((ee, st))
}

#[test]
fn orig_102b98_matches_the_game() {
    let Some((mut ee, st)) = stubbed("oracle test of 0x102b98", &[]) else { return };
    let mut r = Rng::new(0x102b98);
    for n in 0..200 {
        let wins = [(0x33e6f0, 0x30)];
        let mut p = begin(&mut ee, &st, &mut r, &wins, &[]);
        call(&mut ee, 0x102b98, &[], &[]);
        field::table_init(&mut p);
        finish(&mut ee, &st, &p, n, "0x102b98");
    }
}

#[test]
fn orig_102b28_matches_the_game() {
    let Some((mut ee, st)) = stubbed("oracle test of 0x102b28", &[0x2c10b0, 0x136b90, 0x136c40]) else { return };
    let mut r = Rng::new(0x102b28);
    for n in 0..50 {
        let mut p = begin(&mut ee, &st, &mut r, &[], &[]);
        call(&mut ee, 0x102b28, &[], &[]);
        field::audio_reset(&mut p);
        assert_eq!(p.log.len(), 3);
        finish(&mut ee, &st, &p, n, "0x102b28");
    }
}

impl TW {
    /// Ee-side stand-ins for a case on the task world: they log into `st` and answer from its policy.
    fn stubs(&mut self, addrs: &[u32]) -> Rc<RefCell<Stubs>> {
        let st = Rc::new(RefCell::new(Stubs::default()));
        install_stubs(&mut self.ee, &st, addrs);
        st
    }
}

/// The field start (0x102a60): everything it calls is a stand-in except the audio reset and the manager creation, which are the
/// original's own (the manager's allocator is the fake of the task world).
#[test]
fn orig_102a60_matches_the_game() {
    let Some(mut w) = TW::new("oracle test of 0x102a60") else { return };
    let st = w.stubs(&[0x167908, 0x105fd8, 0x104f88, 0x2c10b0, 0x136b90, 0x136c40, 0x149f70, 0x1078f0, 0x168928, 0x104ca8, 0x104c68, 0x223278, 0x13ee88, 0x13ef68]);
    let mut tally = [0u32; 2];
    for n in 0..1000u32 {
        let mut r = Rng::new(0x102a60 + n as u64);
        w.reset(&mut r, 1.0);
        let wins = [(0x33e6e4, 4), (0x33e70c, 4), (0x33e848, 8), (0x33e86c, 8)];
        let next = if r.below(2) == 0 { 0 } else { r.word() as u32 | 1 };
        // The allocator the stand-in 0x105fd8 "creates" is the task world's fake object.
        let rules = [(0x105fd8, (u32::MAX, ALLOC as u64, ALLOC as u64))];
        let mut p = begin(&mut w.ee, &st, &mut r, &wins, &rules);
        w.ee.w32(0x33e70c, next);
        p.put(0x33e70c, &next.to_le_bytes());
        let mut ctl = AreaCtl { task_mgr: r.word() as u32, ..AreaCtl::default() };
        let prev = p.bytes(0x33e848, 6);
        ctl.prev.copy_from_slice(&prev);
        call(&mut w.ee, 0x102a60, &[], &[]);
        let mut tasks = std::mem::take(&mut w.tasks);
        field::start(&mut p, &mut tasks, &mut ctl);
        w.tasks = tasks;
        // The manager the original made is the first of the world; the port's handle maps to it.
        let m = ctl.task_mgr;
        w.run.bind_mgr(m);
        assert_eq!(w.ee.r32(0x33e6e4), w.run.tr(m), "case {n}: the field manager word");
        p.put(0x33e6e4, &w.ee.r32(0x33e6e4).to_le_bytes());
        p.put(0x33e848, &ctl.prev);
        finish(&mut w.ee, &st, &p, n, "0x102a60");
        w.verify(n);
        tally[(next != 0) as usize] += 1;
    }
    assert!(tally[0] > 300 && tally[1] > 300, "both area paths ran: {tally:?}");
}

/// The field exit task runs as a coroutine: the original's switch code runs natively, the four release calls are stand-ins.
#[test]
fn orig_102b50_matches_the_game() {
    let Some(mut w) = TW::new("oracle test of 0x102b50") else { return };
    let st = w.stubs(&[0x136be0, 0x136c90, 0x104ce0, 0x2c10b0]);
    for n in 0..300u32 {
        let mut r = Rng::new(0x102b50 + n as u64);
        w.reset(&mut r, 1.0);
        let mut p = host_policy(&mut w.ee, &st, &mut r, &[], &[]);
        let m = w.manager();
        let t = w.coroutine(m, 0, 0, field::EXIT_FN, 0x800);
        assert_eq!(w.tasks.task(t).unwrap().function(), field::EXIT_FN);
        // Three passes: the call and the yield, the wait of one frame, the return.
        for _ in 0..3 {
            w.pass(m, 0, 0, true);
        }
        w.verify(n);
        p.log = std::mem::take(&mut w.run.calls.0).into_iter().map(|(c, a)| (c, a.iter().map(|x| *x as u32).collect())).collect();
        finish(&mut w.ee, &st, &p, n, "0x102b50");
    }
}

#[test]
fn orig_102bf8_matches_the_game() {
    let Some(mut w) = TW::new("oracle test of 0x102bf8") else { return };
    for n in 0..500u32 {
        let mut r = Rng::new(0x102bf8 + n as u64);
        w.reset(&mut r, 1.0);
        let m = w.manager();
        w.ee.w32(0x33e6e4, w.run.tr(m));
        let ctl = AreaCtl { task_mgr: m, ..AreaCtl::default() };
        let (bits, prio, stack) = (r.below(16) as u32, r.below(5) as i32 - 2, [0x400u32, 0x800, 0x1000][r.below(3) as usize]);
        let e = w.call(0x102bf8, &[bits as u64, sx(prio as u32), BODY as u64, stack as u64]);
        let h = field::create_coroutine(&mut w.tasks, &ctl, bits, prio, BODY, stack);
        let th = w.tasks.task(h).unwrap().thread();
        w.run.bind_thread(th);
        w.run.bind_task(h);
        assert_eq!(e, w.run.tr(h), "case {n}: the original returns the new task");
        w.verify(n);
    }
}

#[test]
fn orig_1024f8_matches_the_game() {
    let Some(mut w) = TW::new("oracle test of 0x1024f8") else { return };
    let st = w.stubs(&[0x105fd8]);
    for n in 0..200u32 {
        let mut r = Rng::new(0x1024f8 + n as u64);
        w.reset(&mut r, 1.0);
        let rules = [(0x105fd8, (u32::MAX, ALLOC as u64, ALLOC as u64))];
        let mut p = host_policy(&mut w.ee, &st, &mut r, &[], &rules);
        let (region, size) = (r.word() as u32, r.word() as u32);
        w.call(0x1024f8, &[region as u64, size as u64]);
        let h = w.tasks.create_manager_over(&mut p, region, size);
        w.run.bind_mgr(h);
        assert_eq!(w.run.tr(h), w.sh.borrow().last_mgr);
        finish(&mut w.ee, &st, &p, n, "0x1024f8");
        w.verify(n);
    }
}

// ---------------------------------------------------------------------------------------------------------------------------------
// The frame task, the frame unit start, the camp poll

const VS_WIN: (u32, usize) = (0x356f18, 0x3c);

/// A random frame block as the frame step needs it: a small limit so the original's wait loops stay short.
fn random_vsync(ee: &mut Ee, r: &mut Rng) -> Vsync {
    let mut v = Vsync::default();
    for b in v.0.iter_mut() {
        *b = r.word() as u8;
    }
    v.set_limit(r.below(4) as u32);
    v.set_since_step(if r.below(4) == 0 { r.word() as u32 } else { r.below(6) as u32 });
    v.set_frame_type(r.below(3) as u8);
    v.set_speed(if r.below(3) == 0 { [1.0, 0.5, 2.0][r.below(3) as usize] } else { r.float() });
    for o in [0x28, 0x2c, 0x30] {
        v.0[o..o + 4].copy_from_slice(&r.float().to_le_bytes());
    }
    ee.write(0x356f18, &v.0);
    v
}

/// The frame wait of the original is a stand-in that adds a vblank to the counter at 0x356f38.
fn hook_wait(ee: &mut Ee) {
    ee.hooks.insert(0x14d1a8, |e: &mut Ee| {
        let c = e.r32(0x356f38);
        e.w32(0x356f38, c.wrapping_add(1));
    });
}

fn vs_from(p: &mut PortHost) -> Vsync {
    Vsync(p.bytes(VS_WIN.0, VS_WIN.1).try_into().unwrap())
}

#[test]
fn orig_14ccf8_matches_the_game() {
    let Some((mut ee, st)) = stubbed("oracle test of 0x14ccf8", &[0x1b6048, 0x2f5ee0, 0x14ddb0, 0x14e318]) else { return };
    hook_wait(&mut ee);
    let mut r = Rng::new(0x14ccf8);
    for n in 0..1500 {
        random_vsync(&mut ee, &mut r);
        let mut p = host_policy(&mut ee, &st, &mut r, &[VS_WIN], &[]);
        call(&mut ee, 0x14ccf8, &[], &[]);
        let mut v = vs_from(&mut p);
        frame::frame_task(&mut v, &mut p, |v| v.set_since_step(v.since_step().wrapping_add(1)));
        p.put(VS_WIN.0, &v.0);
        assert_eq!(p.log.len(), 3 + 1, "case {n}");
        finish_floats(&mut ee, &st, &p, n, "0x14ccf8");
    }
}

/// [`finish`] with the frame block's float words compared within the rounding of the two FPUs.
fn finish_floats(ee: &mut Ee, st: &Rc<RefCell<Stubs>>, port: &PortHost, case: u32, what: &str) {
    let mut p = PortHost { regs: Vec::new(), pol: Pol::default(), log: port.log.clone() };
    for (a, d) in &port.regs {
        let mut keep = d.clone();
        if *a == VS_WIN.0 {
            for o in (0x24..0x34).step_by(4) {
                let g = ee.f32_at(a + o as u32);
                let m = f32::from_le_bytes(d[o..o + 4].try_into().unwrap());
                assert!(feq(g, m), "{what} case {case}: float +{o:#x}: game {g} port {m}");
                keep[o..o + 4].copy_from_slice(&g.to_le_bytes());
            }
        }
        p.regs.push((*a, keep));
    }
    finish(ee, st, &p, case, what);
}

#[test]
fn orig_14cd78_matches_the_game() {
    let Some(mut w) = TW::new("oracle test of 0x14cd78") else { return };
    let st = w.stubs(&[0x14c678, 0x2f9db0, 0x14d0a0]);
    for n in 0..300u32 {
        let mut r = Rng::new(0x14cd78 + n as u64);
        w.reset(&mut r, 1.0);
        let root = w.manager();
        w.ee.w32(ROOT_WORD, w.run.tr(root));
        for h in 0..r.below(3) {
            w.plain(root, h as u32, r.below(800_000) as i32);
        }
        random_vsync(&mut w.ee, &mut r);
        let mut p = host_policy(&mut w.ee, &st, &mut r, &[VS_WIN], &[]);
        call(&mut w.ee, 0x14cd78, &[], &[]);
        let mut v = vs_from(&mut p);
        let mut tasks = std::mem::take(&mut w.tasks);
        let t = frame::frame_unit_start(&mut v, &mut p, &mut tasks, root);
        w.tasks = tasks;
        w.run.bind_task(t);
        p.put(VS_WIN.0, &v.0);
        finish_floats(&mut w.ee, &st, &p, n, "0x14cd78");
        w.verify(n);
        assert_eq!(w.tasks.task(*w.tasks.list(root).last().unwrap()).unwrap().function(), kh2_game_core::task::FRAME_TASK, "case {n}: the frame task runs last");
    }
}

/// The camp poll: the four menu leaves and the pad/frame step are stand-ins; the pause timers and the flags are memory.
#[test]
fn orig_14a080_matches_the_game() {
    let stubs = [0x2c0bd0, 0x1acf10, 0x2fee00, 0x23e860, 0x2f5f08, 0x23e8b0, 0x1674f8, 0x14cdd0];
    let Some((mut ee, st)) = stubbed("oracle test of 0x14a080", &stubs) else { return };
    let mut r = Rng::new(0x14a080);
    let mut tally = [0u32; 4];
    for n in 0..3000 {
        let wins = [VS_WIN, (0x356398, 2)];
        let open = r.below(5) as u32;
        let idle = r.below(5) as u32;
        let rel = r.below(4) as u32;
        let rules = [(0x2c0bd0, (open, 1, 0)), (0x1acf10, (idle, 1, 0)), (0x2fee00, (rel, 1, 2))];
        let mut p = begin(&mut ee, &st, &mut r, &wins, &rules);
        // The stop flag is mostly clear; a pause flag and the timer's pause bits are random.
        let stop = (r.below(8) == 0) as u8;
        ee.write(0x356398, &[stop]);
        p.put(0x356398, &[stop]);
        let bits = if r.below(2) == 0 { 0 } else { r.below(8) as u32 };
        ee.w32(0x356f1c, bits);
        p.put(0x356f1c, &bits.to_le_bytes());
        let mut ctl = AreaCtl { poll_stop: stop, poll_paused: p.bytes(0x356399, 1)[0] & 1, ..AreaCtl::default() };
        ee.write(0x356399, &[ctl.poll_paused]);
        p.put(0x356399, &[ctl.poll_paused]);
        let mut v = vs_from(&mut p);
        call(&mut ee, 0x14a080, &[], &[]);
        camp::poll_task(&mut p, &mut ctl, &mut v);
        p.put(VS_WIN.0, &v.0);
        p.put(0x356398, &[ctl.poll_stop, ctl.poll_paused]);
        finish(&mut ee, &st, &p, n, "0x14a080");
        tally[(p.log.len().min(30) / 8).min(3)] += 1;
    }
    assert!(tally[2] + tally[3] > 100 && tally[0] > 100, "the generator reaches short and long polls: {tally:?}");
}

// ---------------------------------------------------------------------------------------------------------------------------------
// The pad unit and the vibration

const PAD_LOGICAL: u32 = 0x35b7c0;
const PAD_SETS: u32 = 0x35b888;

/// Random pad globals in the original's memory (the set pointer table is the executable's own and stays).
fn random_pad(ee: &mut Ee, r: &mut Rng) {
    ee.write(PAD_LOGICAL, &rnd_bytes(r, 0xc0));
    for a in [0x35b880u32, 0x35b8ac, 0x522150, 0x522168, 0x52216c, 0x522170] {
        ee.w32(a, r.word() as u32);
    }
}

fn pad_from(ee: &mut Ee) -> PadUnit {
    let mut sets = [0u32; 6];
    for (i, s) in sets.iter_mut().enumerate() {
        *s = ee.r32(PAD_SETS + 4 * i as u32);
    }
    let mut p = PadUnit::new(sets);
    for (i, b) in p.logical.iter_mut().enumerate() {
        for (o, x) in b.iter_mut().enumerate() {
            *x = ee.r8(PAD_LOGICAL + 0x60 * i as u32 + o as u32);
        }
    }
    p.scale = [ee.f32_at(0x522168), ee.f32_at(0x52216c), ee.f32_at(0x522170)];
    p.active = ee.r32(0x35b8ac);
    p.active_number = ee.r32(0x522150);
    p.word_880 = ee.r32(0x35b880);
    p
}

fn same_pad(ee: &mut Ee, p: &PadUnit, case: u32, what: &str) {
    let g = pad_from(ee);
    for i in 0..2 {
        if let Some(o) = (0..p.logical[i].len()).find(|&o| g.logical[i][o] != p.logical[i][o]) {
            panic!("{what} case {case}: logical block {i} byte +{o:#x}: game {:02x} port {:02x}", g.logical[i][o], p.logical[i][o]);
        }
    }
    for w in 0..3 {
        assert_eq!(g.scale[w].to_bits(), p.scale[w].to_bits(), "{what} case {case}: scale word {w}");
    }
    assert_eq!((g.active, g.active_number, g.word_880), (p.active, p.active_number, p.word_880), "{what} case {case}: active table, set number, word 0x35b880");
}

#[test]
fn orig_167908_matches_the_game() {
    let Some(mut ee) = machine("oracle test of 0x167908") else { return };
    let mut r = Rng::new(0x167908);
    for n in 0..600 {
        random_pad(&mut ee, &mut r);
        let mut p = pad_from(&mut ee);
        let set = r.below(6) as u32;
        call(&mut ee, 0x167908, &[set as u64], &[]);
        p.select_set(set);
        same_pad(&mut ee, &p, n, "0x167908");
        assert!(set == 0 || p.active != 0, "the executable's set table has the set");
    }
}

#[test]
fn orig_167600_matches_the_game() {
    let Some(mut ee) = machine("oracle test of 0x167600") else { return };
    let mut r = Rng::new(0x167600);
    for n in 0..300 {
        random_pad(&mut ee, &mut r);
        let mut p = pad_from(&mut ee);
        call(&mut ee, 0x167600, &[], &[]);
        p.init_blocks();
        same_pad(&mut ee, &p, n, "0x167600");
        assert_eq!(p.active_number, 0);
        assert_eq!(p.logical[1][0x44..0x48], PadUnit::record_address(1).to_le_bytes());
    }
}

#[test]
fn orig_166fd0_matches_the_game() {
    let Some(mut w) = TW::new("oracle test of 0x166fd0") else { return };
    let st = w.stubs(&[0x167588]);
    for n in 0..300u32 {
        let mut r = Rng::new(0x166fd0 + n as u64);
        w.reset(&mut r, 1.0);
        let root = w.manager();
        w.ee.w32(ROOT_WORD, w.run.tr(root));
        for h in 0..r.below(3) {
            w.plain(root, h as u32, r.below(200_000) as i32);
        }
        random_pad(&mut w.ee, &mut r);
        let mut p = pad_from(&mut w.ee);
        let constants = [w.ee.f32_at(0x379680), w.ee.f32_at(0x379684), w.ee.f32_at(0x379688)];
        let _ = host_policy(&mut w.ee, &st, &mut r, &[], &[]);
        call(&mut w.ee, 0x166fd0, &[], &[]);
        let mut tasks = std::mem::take(&mut w.tasks);
        let mut created = Vec::new();
        p.start(constants, |bits, prio, function| created.push((tasks.create_task(root, bits, prio, function), function)));
        w.tasks = tasks;
        assert_eq!(created.len(), 1);
        w.run.bind_task(created[0].0);
        assert_eq!(w.tasks.task(created[0].0).unwrap().priority(), 110_000);
        same_pad(&mut w.ee, &p, n, "0x166fd0");
        w.verify(n);
    }
}

#[test]
fn orig_167038_matches_the_game() {
    let Some((mut ee, st)) = stubbed("oracle test of 0x167038", &[0x167550, 0x167840]) else { return };
    let mut r = Rng::new(0x167038);
    for n in 0..20 {
        let _ = host_policy(&mut ee, &st, &mut r, &[], &[]);
        call(&mut ee, 0x167038, &[], &[]);
        let mut p = PadUnit::new([0; 6]);
        let mut seen = Vec::new();
        p.task(|a| seen.push(a));
        let game: Vec<u32> = st.borrow().log.iter().map(|e| e.0).collect();
        assert_eq!(game, seen, "case {n}");
        assert_eq!(seen, vec![0x167550, 0x167840]);
    }
}

const CURVES: u32 = 0x0183_0000;
const REC0: u32 = 0x35b4c0;

/// Random vibration state, with its curves in the original's memory; returns the port's copy and the curve bytes.
fn random_vibration(ee: &mut Ee, r: &mut Rng) -> (Vibration, Vec<u8>) {
    let mut arena = vec![0u8; 0x400];
    let mut counts = [0i32; 2];
    let mut curves = [0u32; 2];
    for c in 0..2 {
        let n = r.below(7) as usize;
        counts[c] = n as i32;
        curves[c] = CURVES + (c as u32) * 0x100;
        let mut t = r.below(6) as i32 - 1;
        for k in 0..n {
            let o = c * 0x100 + k * 8;
            arena[o..o + 4].copy_from_slice(&t.to_le_bytes());
            let v = match r.below(4) {
                0 => r.word() as i32,
                1 => r.below(300) as i32 - 150,
                _ => r.below(256) as i32,
            };
            arena[o + 4..o + 8].copy_from_slice(&v.to_le_bytes());
            t += r.below(9) as i32;
        }
    }
    ee.write(CURVES, &arena);
    let v = Vibration {
        counts,
        curves,
        state: [0, 1, 2, 2, 2, 3, -1, r.word() as i32][r.below(8) as usize],
        time: if r.below(3) == 0 { r.float().abs() * 10.0 } else { r.below(40) as f32 },
        length: [r.below(60) as i32 + 1, 0, -3, r.word() as i32][r.below(4) as usize],
        use_curves: [1, 1, 0, r.word() as u32][r.below(4) as usize],
    };
    ee.w32(0x36ca08, v.counts[0] as u32);
    ee.w32(0x36ca0c, v.counts[1] as u32);
    ee.w32(0x36ca10, v.curves[0]);
    ee.w32(0x36ca14, v.curves[1]);
    ee.w32(0x36ca18, v.state as u32);
    ee.w32(0x36ca1c, v.time.to_bits());
    ee.w32(0x36ca20, v.length as u32);
    ee.w32(0x36ca24, v.use_curves);
    ee.write(REC0, &rnd_bytes(r, 0x180));
    (v, arena)
}

fn same_vibration(ee: &mut Ee, v: &Vibration, rec: &[u8], case: u32, what: &str) {
    let g = (
        [ee.r32(0x36ca08) as i32, ee.r32(0x36ca0c) as i32],
        [ee.r32(0x36ca10), ee.r32(0x36ca14)],
        ee.r32(0x36ca18) as i32,
        ee.r32(0x36ca1c),
        ee.r32(0x36ca20) as i32,
        ee.r32(0x36ca24),
    );
    assert_eq!(g, (v.counts, v.curves, v.state, v.time.to_bits(), v.length, v.use_curves), "{what} case {case}: the vibration block");
    for i in 0..0x180u32 {
        assert_eq!(ee.r8(REC0 + i), rec[i as usize], "{what} case {case}: pad record byte +{i:#x}");
    }
}

fn vibration_cases(addr: u32, task: bool) {
    let Some(mut ee) = machine(&format!("oracle test of {addr:#x}")) else { return };
    let mut r = Rng::new(addr as u64);
    let mut tally = [0u32; 4];
    for n in 0..4000 {
        let (mut v, arena) = random_vibration(&mut ee, &mut r);
        let mut rec: Vec<u8> = (0..0x180u32).map(|i| ee.r8(REC0 + i)).collect();
        let one = ee.f32_at(0x37f454);
        let read = |a: u32| u32::from_le_bytes(arena[(a - CURVES) as usize..(a - CURVES) as usize + 4].try_into().unwrap());
        if task {
            let mode = [0u32, 1, 2, 2, 3][r.below(5) as usize];
            ee.w32(0x1dc0dd0, mode);
            let block = 0x0191_0000u32;
            ee.write(block, &rnd_bytes(&mut r, 0x48));
            ee.w32(block, 0x1234_5678);
            call(&mut ee, addr, &[block as u64], &[]);
            let ends = v.task(mode, one, read, &mut rec);
            assert_eq!(ee.r32(block) == 0, ends, "case {n}: the task ends exactly when the port says so");
            tally[ends as usize] += 1;
        } else {
            call(&mut ee, addr, &[], &[]);
            let before = v.state;
            v.step(one, read, &mut rec);
            tally[(before == 2) as usize] += 1;
            tally[2 + (v.state == 0 && before != 0) as usize] += 1;
        }
        same_vibration(&mut ee, &v, &rec, n, &format!("{addr:#x}"));
    }
    let used = if task { &tally[..2] } else { &tally[..] };
    assert!(used.iter().all(|&t| t > 100), "the generator reaches both outcomes of each choice: {tally:?}");
}

#[test]
fn orig_239b20_matches_the_game() {
    vibration_cases(0x239b20, false);
}

#[test]
fn orig_239ac8_matches_the_game() {
    vibration_cases(0x239ac8, true);
}
