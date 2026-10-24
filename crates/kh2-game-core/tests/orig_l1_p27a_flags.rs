//! Oracle tests of the story flag setter's side-effect functions (core.save, WP-L1-P27a). The setter `FUN_0017ff60` runs the list of ops its
//! world's entry holds for the flag; each op calls a small function that the port keeps as a private helper of
//! `progress_flags::set_flag_full`: `FUN_00195260` (op 2, an exit lock's state and message), `FUN_00195288` (op 3, open), `FUN_001952a0`
//! (op 4, lock and clear the world point through `FUN_00194f68` and `FUN_00194fd8`), `FUN_00195300` (op 5, open and clear the point),
//! `FUN_001a7730` (op 6, the world's record byte) and `FUN_001b4e80` (op 0xd, the world's record). `orig_save2.rs` compares the setter with
//! every op mixed; here each test draws its lists from the one op (or pair) it is named for, so the function's effect is checked on its own
//! over 3,000 flags, and the original runs the function itself (the setter is not stubbed). The tables are laid out as in `orig_save2.rs`.
//!
//! Stand-ins: the ten calls after the list (`FUN_001803b0`, `FUN_00180450`, `FUN_001805d0`, `FUN_00180558`, `FUN_001d39d0`, `FUN_001d3238`,
//! `FUN_001b6da0`, `FUN_001be9c8`, `FUN_001d5718`, `FUN_00195130`) are hooks that log (address, flag), and the port's tail logs the same.
//!
//! Skips without a disc (KH2_ISO or local/iso.txt); fails without one when KH2_REQUIRE_DISC=1.

use kh2_ee::{Ee, reg};
use kh2_factory::fuzz::*;
use kh2_game_core::progress_flags::*;
use kh2_game_core::save_block::LEN;

const SAVE: u32 = 0x33e860;
const SAVE_GLOBAL: (u32, u32, bool) = (SAVE, LEN as u32, true);
const LOG: u32 = 0x01f0_0000;
const LOG_LEN: u32 = 0x4000;
const ENT: u32 = 0x01e0_0000;
const ENT_PAGE: u32 = 0x1000;
const LOCKS: u32 = 0x01e2_0000;
const POINTS: u32 = 0x01e2_1000;
const SEED: u64 = 0x7e17;

fn log(e: &mut Ee, kind: u32, a: u32, b: u32) {
    let n = e.r32(LOG);
    let at = LOG + 4 + 12 * n;
    e.w32(at, kind);
    e.w32(at + 4, a);
    e.w32(at + 8, b);
    e.w32(LOG, n + 1);
    e.gpr[reg::V0] = 0;
}

fn write_log(c: &mut Case, at: usize, log: &[u32]) {
    let g = c.global(at);
    g[..4].copy_from_slice(&(log.len() as u32 / 3).to_le_bytes());
    for (i, v) in log.iter().enumerate() {
        g[4 + 4 * i..8 + 4 * i].copy_from_slice(&v.to_le_bytes());
    }
}

/// A list of ops for one flag slot of `page`: `leaf` lists carry no op 0xc.
fn list(rng: &mut Rng, leaf: bool, ops: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    for _ in 0..rng.below(5) {
        let op = ops[rng.below(ops.len() as u64) as usize];
        let op = if op == 0xc && leaf { 7 } else { op };
        let flag = |rng: &mut Rng| (rng.below(19) as u16) << 10 | rng.below(0x3c0) as u16;
        let leaf_flag = |rng: &mut Rng| (rng.below(19) as u16) << 10 | (0x3c0 + rng.below(0x40) as u16);
        let mut h: Vec<u16> = Vec::new();
        let mut bytes: Vec<u8> = Vec::new();
        match op {
            1 => {
                for _ in 0..1 + rng.below(3) {
                    bytes.push(rng.below(60) as u8);
                    for _ in 0..3 {
                        bytes.push(if rng.below(4) == 0 { 0xff } else { rng.word() as u8 });
                    }
                }
            }
            2 => h = vec![rng.below(25) as u16, rng.word() as u16, rng.word() as u16],
            3..=5 => h = vec![rng.below(25) as u16],
            6 => h = vec![rng.below(300) as u16],
            7 => h = (0..1 + rng.below(3)).map(|_| flag(rng)).collect(),
            8 | 9 => h = (0..1 + rng.below(3)).map(|_| rng.below(0x300) as u16).collect(),
            0xc => h = (0..1 + rng.below(3)).map(|_| leaf_flag(rng)).collect(),
            _ => h = (0..1 + rng.below(2)).map(|_| rng.word() as u16).collect(),
        }
        for v in &h {
            bytes.extend(v.to_le_bytes());
        }
        out.push(op);
        out.push((bytes.len() / 2) as u8);
        out.extend(bytes);
    }
    out.push(0);
    out
}

/// One page's entry: 1,024 offsets, then the lists (about 20 for the flags below the leaf range, 20 for the leaf range).
fn entry(rng: &mut Rng, ops: &[u8]) -> Vec<u8> {
    let mut e = vec![0u8; ENT_PAGE as usize];
    let mut at = 0x800usize;
    let mut offsets: [Vec<u16>; 2] = [Vec::new(), Vec::new()];
    for (leaf, offs) in offsets.iter_mut().enumerate() {
        for _ in 0..20 {
            let l = list(rng, leaf == 1, ops);
            if at + l.len() > e.len() {
                break;
            }
            e[at..at + l.len()].copy_from_slice(&l);
            offs.push(at as u16);
            at += l.len();
        }
    }
    // The empty list: a zero byte at the end.
    let empty = at as u16;
    for slot in 0..0x400usize {
        let pool = &offsets[(slot >= 0x3c0) as usize];
        let o = if rng.below(3) == 0 && !pool.is_empty() { pool[rng.below(pool.len() as u64) as usize] } else { empty };
        e[2 * slot..2 * slot + 2].copy_from_slice(&o.to_le_bytes());
    }
    e
}

struct Tail(Vec<u32>);

impl SetterTail for Tail {
    fn call(&mut self, _save: &mut [u8], addr: u32, arg: i32) {
        self.0.extend([addr, arg as u32, 0]);
    }
}

fn setter_machine(what: &str, rng: &mut Rng, ops: &[u8]) -> Option<(Ee, Vec<Vec<u8>>, Vec<u8>, Vec<u8>)> {
    let elf = kh2_factory::exe_or_skip(what)?;
    let mut ee = Ee::new();
    ee.load_elf(&elf).expect("elf");
    // The ten calls after the list log (address, flag or 0).
    macro_rules! tail {
        ($addr:expr, $arg:expr) => {
            ee.hooks.insert($addr, |e| {
                let a = if $arg { e.gpr[reg::A0] as u32 } else { 0 };
                log(e, $addr, a, 0)
            })
        };
    }
    tail!(0x1803b0, true);
    tail!(0x180450, false);
    tail!(0x1805d0, true);
    tail!(0x180558, false);
    tail!(0x1d39d0, false);
    tail!(0x1d3238, true);
    tail!(0x1b6da0, true);
    tail!(0x1be9c8, true);
    tail!(0x1d5718, false);
    tail!(0x195130, true);
    let entries: Vec<Vec<u8>> = (0..19).map(|_| entry(rng, ops)).collect();
    for (p, e) in entries.iter().enumerate() {
        ee.write(ENT + p as u32 * ENT_PAGE, e);
        ee.w32(0x35ccc0 + 4 * p as u32, ENT + p as u32 * ENT_PAGE);
    }
    // Lock and world point tables: bytes below 4 so that the point lookup matches often.
    let small = |rng: &mut Rng, n: usize| (0..n).map(|_| rng.below(4) as u8).collect::<Vec<u8>>();
    let locks = small(rng, 0x100);
    let points = small(rng, 0x36 * 4);
    ee.write(LOCKS, &locks);
    ee.write(POINTS, &points);
    ee.w32(0x1c8a988, LOCKS);
    ee.w32(0x1c8a984, POINTS);
    Some((ee, entries, locks, points))
}


/// The setter against the original over lists made only of `ops`; `ranges` are the flag ranges drawn (every page, then the leaf range).
fn setter_case(label: &str, seed: u64, ops: &[u8]) {
    let mut rng = Rng::new(seed);
    let Some((mut ee, entries, locks, points)) = setter_machine(&format!("oracle test of {label}"), &mut rng, ops) else { return };
    let refs: Vec<&[u8]> = entries.iter().map(|e| e.as_slice()).collect();
    static ALL: [Arg; 1] = [Arg::Range(0, 19 * 1024 - 1)];
    static LEAF: [Arg; 1] = [Arg::Range(0x3c0, 0x3ff)];
    for (args, cases) in [(&ALL, 3000u32), (&LEAF, 600)] {
        let spec = Spec { addr: 0x17ff60, args, floats: 0, ret: Ret::None, globals: &[SAVE_GLOBAL, (LOG, LOG_LEN, false)], stubs: &[], nulls: false };
        ee.write(LOG, &vec![0; LOG_LEN as usize]);
        let st = fuzz_with(&mut ee, &spec, cases, SEED, |c| {
            let id = c.int(0);
            let tables = SetterTables { entries: &refs, locks: &locks, points: &points };
            let mut tail = Tail(Vec::new());
            set_flag_full(c.global(0), &tables, &mut tail, id);
            write_log(c, 1, &tail.0);
            Out::none()
        });
        eprintln!("{label}: {} cases equal, {} interpreter steps in {:.3} s", st.cases, st.steps, st.secs);
    }
}

/// `FUN_00195260(index, state, message)`: the exit lock entry gets its state and message halfwords.
#[test]
fn orig_195260_matches_the_game() {
    setter_case("0x195260 (op 2)", 0x195260, &[2, 2, 2, 7]);
}

/// `FUN_00195288(index)`: the lock entry's state is cleared.
#[test]
fn orig_195288_matches_the_game() {
    setter_case("0x195288 (op 3)", 0x195288, &[3, 3, 2, 7]);
}

/// `FUN_001952a0(index)`: the lock gets state 0xffff and its world point loses its bit (lookup `FUN_00194f68`, bit clear `FUN_00194fd8`).
#[test]
fn orig_1952a0_matches_the_game() {
    setter_case("0x1952a0 (op 4)", 0x1952a0, &[4, 4, 3, 7]);
}

/// `FUN_00195300(index)`: the lock is opened and its world point loses its bit.
#[test]
fn orig_195300_matches_the_game() {
    setter_case("0x195300 (op 5)", 0x195300, &[5, 5, 3, 7]);
}

/// `FUN_00194f68` (the world point lookup by the lock's two bytes) and `FUN_00194fd8` (the bit clear): reached through ops 4 and 5 over
/// small lock and point tables, so the lookup hits often.
#[test]
fn orig_194fd8_matches_the_game() {
    setter_case("0x194fd8 (ops 4 and 5)", 0x194fd8, &[4, 5, 4, 5, 7]);
}

/// `FUN_001a7730(world, value)`: `value - 1` goes to the record byte `world + 0x5c`.
#[test]
fn orig_1a7730_matches_the_game() {
    setter_case("0x1a7730 (op 6)", 0x1a7730, &[6, 6, 6, 7]);
}

/// `FUN_001b4e80(world)` (the world's 4-byte record) as op 0xd uses it: the halfword's high and low byte go to bytes 1 and 2.
#[test]
fn orig_1b4e80_matches_the_game() {
    setter_case("0x1b4e80 (op 0xd)", 0x1b4e80, &[0xd, 0xd, 0xd, 7]);
}
