//! Oracle tests for the second half of the save block ports (core.save, WP-L1-P17): the world flag queue and its boot
//! (`FUN_001b6a18`, `FUN_001b68f8`, `FUN_001b6e20`, `FUN_001b6e60`, `FUN_001b6870`, `FUN_002c6d60`), the full story flag setter
//! `FUN_0017ff60` with its list ops, the play time `FUN_0014ca28`, the defeat counter read `FUN_00187e68`, the experience still
//! needed `FUN_001878b0` and the two table lookups `FUN_001a6810` and `FUN_001be4b8`. kh2-ee runs the original from the user's
//! executable on random inputs and the port runs on copies of the same bytes; every byte of every block and global the function may
//! touch is compared (the harness of `kh2_factory::fuzz`, 1,000 to 4,000 cases per function).
//!
//! Stand-ins, all documented here and nowhere else:
//! - The world flag routines work on the whole save block as a randomised global. `FUN_001b68c0` (the `wldf` record lookup) is replaced
//!   by a hook that answers a pointer into a table of 16-byte records at `RECS` (byte 0 the index, byte 1 the kind 0..2, byte 2 random);
//!   `FUN_001b6a48` (the list runner) logs its two arguments. The port reads the same table and logs the same triple, and the log is a
//!   compared global, so every call is checked in order.
//! - The setter runs for real, with the loaded tables laid out in memory: the 19 world entries behind the pointer table at `0x35ccc0`,
//!   the lock table behind `DAT_01c8a988` and the world point table behind `DAT_01c8a984`. The ten calls after its list
//!   (`FUN_001803b0`, `FUN_00180450`, `FUN_001805d0`, `FUN_00180558`, `FUN_001d39d0`, `FUN_001d3238`, `FUN_001b6da0`, `FUN_001be9c8`,
//!   `FUN_001d5718`, `FUN_00195130`) are replaced by hooks that log (address, flag); `FUN_001d39d0` is also reached from list op 8.
//!   The entries hold well-formed lists written for the test (ops 1..9, 0xc, 0xd and unknown ops; flags of op 0xc are drawn from a
//!   leaf range whose lists have no op 0xc, so the recursion ends), with lock indexes below 25 and flag ids inside the 19 pages, which
//!   is what the game's data holds.
//!
//! Skips without a disc (KH2_ISO or local/iso.txt); fails without one when KH2_REQUIRE_DISC=1.

use std::sync::Arc;

use kh2_ee::{Ee, reg};
use kh2_factory::fuzz::*;
use kh2_game_core::progress::{ItemIndex, Progress};
use kh2_game_core::progress_flags::*;
use kh2_game_core::save_block::{self as sb, LEN, at, party};
use kh2_tables::tables::{self as t, Lvup};

const STACK: u32 = 0x01ff_0000;

const SAVE: u32 = 0x33e860;
const PARTY: u32 = 0x33fec0;
const SAVE_GLOBAL: (u32, u32, bool) = (SAVE, LEN as u32, true);
const LOG: u32 = 0x01f0_0000;
const LOG_LEN: u32 = 0x4000;
const RECS: u32 = 0x01f1_0000;
const RECS_LEN: u32 = 0x2000;
const ENT: u32 = 0x01e0_0000;
const ENT_PAGE: u32 = 0x1000;
const LOCKS: u32 = 0x01e2_0000;
const POINTS: u32 = 0x01e2_1000;
const SEED: u64 = 0x7e17;
const WORLD: usize = at::WORLD_RECORDS;

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

// --- the world flag queue ---------------------------------------------------------------------------------------------

/// The `wldf` stand-in table: 256 records of 16 bytes (index, kind 0..2, random byte, random rest).
fn records(rng: &mut Rng) -> Vec<u8> {
    let mut v = vec![0u8; RECS_LEN as usize];
    for (i, r) in v.chunks_mut(16).enumerate() {
        for b in r.iter_mut() {
            *b = rng.word() as u8;
        }
        r[0] = i as u8;
        r[1] = rng.below(3) as u8;
    }
    v
}

fn world_machine(what: &str) -> Option<Ee> {
    let elf = kh2_factory::exe_or_skip(what)?;
    let mut ee = Ee::new();
    ee.load_elf(&elf).expect("elf");
    ee.hooks.insert(0x1b68c0, |e| e.gpr[reg::V0] = (RECS + 16 * (e.gpr[reg::A0] as u32 & 0x1ff)) as u128);
    ee.hooks.insert(0x1b6a48, |e| {
        let (a, b) = (e.gpr[reg::A0] as u32, e.gpr[5] as u32 & 0xff);
        log(e, 4, a, b)
    });
    let mut rng = Rng::new(SEED);
    ee.write(RECS, &records(&mut rng));
    Some(ee)
}

const WORLD_GLOBALS: [(u32, u32, bool); 3] = [SAVE_GLOBAL, (LOG, LOG_LEN, false), (RECS, RECS_LEN, false)];

/// The port's side of the stand-ins.
struct Sim {
    recs: Vec<u8>,
    second: bool,
    log: Vec<u32>,
}

impl WorldEnv for Sim {
    fn record_byte(&self, index: u32, offset: usize) -> u8 {
        self.recs[16 * (index as usize & 0x1ff) + offset]
    }
    fn second_part(&self) -> bool {
        self.second
    }
    fn run_list(&mut self, _world: &mut [u8], key: u8, hook: bool) {
        self.log.extend([4, RECS + 16 * key as u32, hook as u32]);
    }
}

fn sim(c: &mut Case) -> Sim {
    let second = flag_102b(&c.global(0)[at::FLAGS..]);
    Sim { recs: c.global(2).to_vec(), second, log: Vec::new() }
}

fn world_run(addr: u32, args: &'static [Arg], ret: Ret, cases: u32, mut port: impl FnMut(&mut Case) -> Out) {
    let Some(mut ee) = world_machine(&format!("oracle test of {addr:#x}")) else { return };
    let spec = Spec { addr, args, floats: 0, ret, globals: &WORLD_GLOBALS, stubs: &[], nulls: false };
    ee.write(LOG, &vec![0; LOG_LEN as usize]);
    let st = fuzz_with(&mut ee, &spec, cases, SEED, &mut port);
    eprintln!("{addr:#x}: {} cases equal, {} interpreter steps in {:.3} s", st.cases, st.steps, st.secs);
}

#[test]
fn orig_1b6a18_matches_the_game() {
    static ARGS: [Arg; 1] = [Arg::Range(0, 0x1ff)];
    world_run(0x1b6a18, &ARGS, Ret::Int, 2000, |c| {
        let i = c.int(0) as u32;
        Out::int(world_flag_check(&c.global(0)[WORLD..], i) as i64)
    });
}

#[test]
fn orig_1b68f8_matches_the_game() {
    // Indexes 0..0x1ff: the four rival ones are drawn with their neighbours; the flag words of the case are random, so a flag is
    // set about half the time and the queue count is anything from 0 to 255 (255 writes the count byte itself).
    static ARGS: [Arg; 1] = [Arg::Range(0, 0x1ff)];
    static RIVALS: [Arg; 1] = [Arg::Range(0x20, 0x78)];
    for args in [&ARGS, &RIVALS] {
        world_run(0x1b68f8, args, Ret::None, 4000, |c| {
            let i = c.int(0) as u32 & 0x1ff;
            let s = sim(c);
            world_flag_set(&mut c.global(0)[WORLD..], &s, i);
            Out::none()
        });
    }
}

#[test]
fn orig_1b6e20_matches_the_game() {
    static ARGS: [Arg; 1] = [Arg::Ptr(Block { size: 0x100, fields: &[], subs: &[] })];
    world_run(0x1b6e20, &ARGS, Ret::None, 2000, |c| {
        let q = world_take_queue(&mut c.global(0)[WORLD..]);
        c.block(0)[..q.len()].copy_from_slice(&q);
        Out::none()
    });
}

#[test]
fn orig_1b6e60_matches_the_game() {
    world_run(0x1b6e60, &[], Ret::None, 2000, |c| {
        let mut s = sim(c);
        world_drain(&mut c.global(0)[WORLD..], &mut s, false);
        let log = s.log;
        write_log(c, 1, &log);
        Out::none()
    });
}

#[test]
fn orig_1b6870_matches_the_game() {
    world_run(0x1b6870, &[], Ret::None, 2000, |c| {
        let mut s = sim(c);
        world_boot(&mut c.global(0)[WORLD..], &mut s);
        let log = s.log;
        write_log(c, 1, &log);
        Out::none()
    });
}

#[test]
fn orig_2c6d60_matches_the_game() {
    static ARGS: [Arg; 2] = [Arg::Ptr(Block { size: 0x10, fields: &[], subs: &[] }), Arg::Ptr(Block { size: 0x10, fields: &[], subs: &[] })];
    world_run(0x2c6d60, &ARGS, Ret::Int, 2000, |c| {
        let s = sim(c);
        let (a, b) = (c.block(0)[0], c.block(1)[0]);
        Out::int(world_record_order(&s, a, b) as i64)
    });
}

// --- the full story flag setter ---------------------------------------------------------------------------------------

/// A list of ops for one flag slot of `page`: `leaf` lists carry no op 0xc.
fn list(rng: &mut Rng, leaf: bool) -> Vec<u8> {
    let mut out = Vec::new();
    for _ in 0..rng.below(5) {
        let op = [1, 1, 2, 3, 4, 5, 6, 7, 8, 9, 0xd, 0xa, 0xb, 0xe, 0x10, 0xc, 0xc][rng.below(17) as usize];
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
fn entry(rng: &mut Rng) -> Vec<u8> {
    let mut e = vec![0u8; ENT_PAGE as usize];
    let mut at = 0x800usize;
    let mut offsets: [Vec<u16>; 2] = [Vec::new(), Vec::new()];
    for (leaf, offs) in offsets.iter_mut().enumerate() {
        for _ in 0..20 {
            let l = list(rng, leaf == 1);
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

fn setter_machine(what: &str, rng: &mut Rng) -> Option<(Ee, Vec<Vec<u8>>, Vec<u8>, Vec<u8>)> {
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
    let entries: Vec<Vec<u8>> = (0..19).map(|_| entry(rng)).collect();
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

#[test]
fn orig_17ff60_matches_the_game() {
    let mut rng = Rng::new(0x17ff60);
    let Some((mut ee, entries, locks, points)) = setter_machine("oracle test of 0x17ff60", &mut rng) else { return };
    let refs: Vec<&[u8]> = entries.iter().map(|e| e.as_slice()).collect();
    // Every flag of the 19 pages, then the range around flag 0x8d0 (the one flag with its own clear) and the leaf range.
    static ALL: [Arg; 1] = [Arg::Range(0, 19 * 1024 - 1)];
    static NEAR_8D0: [Arg; 1] = [Arg::Range(0x8c0, 0x8e0)];
    static LEAF: [Arg; 1] = [Arg::Range(0x3c0, 0x3ff)];
    for (args, cases) in [(&ALL, 4000u32), (&NEAR_8D0, 1000), (&LEAF, 1000)] {
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
        eprintln!("0x17ff60: {} cases equal, {} interpreter steps in {:.3} s", st.cases, st.steps, st.secs);
    }
}

// --- play time, defeat counter, experience still needed -----------------------------------------------------------------

#[test]
fn orig_14ca28_matches_the_game() {
    let Some(mut ee) = lookup_machine("oracle test of 0x14ca28") else { return };
    // The counting word and the current world byte are inputs outside the block: every world value around the 0x13 limit, counting on and off.
    // The counters are random words, mostly above the cap (that path is the common one); about one case in twenty is below it and counts.
    for (counting, world) in [(1u32, 0u8), (1, 1), (1, 5), (1, 0x12), (1, 0x13), (1, 0x14), (1, 0x80), (1, 0xff), (0, 0), (0, 0x12)] {
        ee.w32(0x33e6fc, counting);
        ee.write(0x33e830, &[world]);
        let st = fuzz_with(&mut ee, &PLAY_TIME, 600, SEED, |c| {
            sb::play_time_tick(c.global(0), counting != 0, world);
            Out::none()
        });
        eprintln!("0x14ca28 (counting {counting}, world {world:#x}): {} cases equal", st.cases);
    }
}

static PLAY_TIME: Spec = Spec { addr: 0x14ca28, args: &[], floats: 0, ret: Ret::None, globals: &[SAVE_GLOBAL], stubs: &[], nulls: false };

#[test]
fn orig_187e68_matches_the_game() {
    // The counters are drawn small often; ids with a counter only (the original reads address 0 for any other id).
    static COUNTERS: [FieldSpec; 59] = {
        let mut a = [(0, 0, Kind::Int); 59];
        let mut i = 0;
        while i < 47 {
            a[i] = (0xff0 + 4 * i as u32, 4, Kind::Int);
            i += 1;
        }
        while i < 59 {
            a[i] = (0x1170 + 4 * (i as u32 - 47), 4, Kind::Int);
            i += 1;
        }
        a
    };
    static LOW: [Arg; 2] = [Arg::Ptr(Block { size: party::LEN as u32, fields: &COUNTERS, subs: &[] }), Arg::Range(1000, 1046)];
    static HIGH: [Arg; 2] = [Arg::Ptr(Block { size: party::LEN as u32, fields: &COUNTERS, subs: &[] }), Arg::Range(3000, 3011)];
    for args in [&LOW, &HIGH] {
        let spec = Spec { addr: 0x187e68, args, floats: 0, ret: Ret::Int, globals: &[], stubs: &[], nulls: false };
        let Some(st) = fuzz(&spec, 2000, |c| {
            let id = c.int(1);
            Out::int(sb::defeat_count(c.block(0), id) as i64)
        }) else {
            return;
        };
        eprintln!("0x187e68: {} cases equal, {} interpreter steps in {:.3} s", st.cases, st.steps, st.secs);
    }
}

/// `FUN_001878b0(record, member)` on the disc's `lvup` blob (laid out behind `DAT_01d5d6b4`) with random party blocks: the member's level
/// byte (1..=255; level 0 reads before the table in the original and is left out), the experience total (small and large, negative too).
#[test]
fn orig_1878b0_matches_the_game() {
    let Some(elf) = kh2_factory::exe_or_skip("oracle test of 0x1878b0") else { return };
    let Some(mut a) = kh2_factory::disc("oracle test of 0x1878b0") else { return };
    let blob = kh2_tables::subfile(&mut a, &t::LVUP).expect("lvup");
    let tables = kh2_tables::rt::decode_lvup::<Lvup>(&blob).expect("lvup tables");
    let mut ee = Ee::new();
    ee.load_elf(&elf).expect("elf");
    ee.write(0x0184_0000, &blob);
    ee.w32(0x1d5d6b4, 0x0184_0000);
    let items = Arc::new(ItemIndex::default());
    let mut rng = Rng::new(0x1878b0);
    for case in 0..3000 {
        let mut bytes: Vec<u8> = (0..party::LEN).map(|_| rng.word() as u8).collect();
        let member = 1 + rng.below(15) as i32;
        let off = sb::member_offset(member).expect("record");
        let level = if rng.below(8) == 0 { 99 + rng.below(157) as u8 } else { 1 + rng.below(98) as u8 };
        bytes[off + 0xf] = level;
        let exp = match rng.below(4) {
            0 => rng.below(2000) as i32,
            1 => rng.below(10_000_000) as i32,
            2 => rng.word() as i32,
            _ => -(rng.below(100) as i32),
        };
        bytes[party::EXPERIENCE..party::EXPERIENCE + 4].copy_from_slice(&exp.to_le_bytes());
        ee.write(PARTY, &bytes);
        let game = ee.call(0x1878b0, &[(PARTY as usize + off) as u64, member as u64], &[], STACK).unwrap_or_else(|f| panic!("case {case}: {f:?}")) as u32 as i32;
        let port = Progress::from_block(&bytes, items.clone()).experience_to_next(member as u16, &tables);
        assert_eq!(game, port, "case {case}: member {member}, level {level}, experience {exp}");
    }
    eprintln!("0x1878b0: 3000 cases equal");
}

/// A table of random records laid out at `at` behind the pointer cell `cell`.
fn lookup_machine(what: &str) -> Option<Ee> {
    let elf = kh2_factory::exe_or_skip(what)?;
    let mut ee = Ee::new();
    ee.load_elf(&elf).expect("elf");
    Some(ee)
}

/// `FUN_001a6810(a, b, c)` over a chest table behind `DAT_01c95590`: the record bytes 4 to 7 are drawn small so that searches match.
#[test]
fn orig_1a6810_matches_the_game() {
    let Some(mut ee) = lookup_machine("oracle test of 0x1a6810") else { return };
    let tbl = 0x01f2_0000u32;
    ee.w32(0x1c95590, tbl);
    let mut rng = Rng::new(0x1a6810);
    let mut found = 0;
    for case in 0..4000 {
        let n = rng.below(40) as usize;
        let mut t = vec![0u8; 4 + 12 * n];
        for b in t.iter_mut() {
            *b = rng.word() as u8;
        }
        t[2..4].copy_from_slice(&(n as u16).to_le_bytes());
        for r in 0..n {
            let o = 4 + 12 * r;
            t[o + 4] = if rng.below(3) == 0 { rng.below(3) as u8 } else { 0 };
            for k in 5..8 {
                t[o + k] = rng.below(4) as u8;
            }
        }
        let arg = |rng: &mut Rng| if rng.below(20) == 0 { 256 + rng.below(4) } else { rng.below(4) };
        let (a, b, c) = (arg(&mut rng), arg(&mut rng), arg(&mut rng));
        ee.write(tbl, &t);
        let game = ee.call(0x1a6810, &[a, b, c], &[], STACK).unwrap_or_else(|f| panic!("case {case}: {f:?}")) as u32;
        let port = sb::find_chest(&t, a, b, c).map_or(0, |o| tbl + o as u32);
        found += (port != 0) as u32;
        assert_eq!(game, port, "case {case}: ({a}, {b}, {c}) in {n} records");
    }
    assert!(found > 300, "the searches must find a record often ({found})");
    eprintln!("0x1a6810: 4000 cases equal, {found} found");
}

/// `FUN_001be4b8(id)` over a table behind `DAT_01d5dd58` (count word at `+4`, records of 16 bytes from `+8`).
#[test]
fn orig_1be4b8_matches_the_game() {
    let Some(mut ee) = lookup_machine("oracle test of 0x1be4b8") else { return };
    let tbl = 0x01f3_0000u32;
    ee.w32(0x1d5dd58, tbl);
    let mut rng = Rng::new(0x1be4b8);
    let mut found = 0;
    for case in 0..4000 {
        let n = rng.below(40) as i32 - 2;
        let mut t = vec![0u8; 8 + 16 * n.max(0) as usize];
        for b in t.iter_mut() {
            *b = rng.word() as u8;
        }
        t[4..8].copy_from_slice(&n.to_le_bytes());
        for r in 0..n.max(0) as usize {
            t[8 + 16 * r] = rng.below(12) as u8;
        }
        let id = if rng.below(20) == 0 { 256 + rng.below(4) } else { rng.below(12) };
        ee.write(tbl, &t);
        let game = ee.call(0x1be4b8, &[id], &[], STACK).unwrap_or_else(|f| panic!("case {case}: {f:?}")) as u32;
        let port = sb::find_record_by_id(&t, id).map_or(0, |o| tbl + o as u32);
        found += (port != 0) as u32;
        assert_eq!(game, port, "case {case}: id {id} in {n} records");
    }
    assert!(found > 300, "the searches must find a record often ({found})");
    eprintln!("0x1be4b8: 4000 cases equal, {found} found");
}
