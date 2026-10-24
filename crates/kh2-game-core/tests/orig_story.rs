//! Oracle tests for the story-flag ports of `progress_flags.rs` (core.story). Each test runs the original in kh2-ee on the same random
//! inputs as the port and compares the return value and every byte of the memory the function may touch. Layout follows `factory gen`
//! (spec, `fuzz_with`, 4000 cases) but is written by hand where the original calls things the harness cannot stub with a plain zero:
//!
//! - the setter `FUN_0017ff60` (and the record-bit setter `FUN_0019d7e0`, the item removal `FUN_001a1ba8`) are replaced by a hook that
//!   appends (kind, a0, a1) to a log in memory at `LOG`; the port's [`StoryEnv`] appends the same triple, and the log is a compared
//!   global, so every call the original makes is checked, not only the memory it changes. The hooks do not change the flag pages,
//!   and the port's environment does not either.
//! - the item lookup `FUN_001a1cc8` is replaced by a read of a random 256-byte table at `ITEMS` (index = item & 0xff), the same table the
//!   port's environment reads.
//!
//! Tables the original reads from the executable (0x37a650, 0x37a6f8) are left as the executable has them and passed to the port as
//! bytes. Skips without a disc (KH2_ISO or local/iso.txt); fails without one when KH2_REQUIRE_DISC=1.

use kh2_ee::{Ee, reg};
use kh2_factory::fuzz::*;
use kh2_game_core::progress_flags::*;

const FLAGS: u32 = 0x33f6b0;
const CASES: u32 = 4000;
const LOG: u32 = 0x01f0_0000;
const ITEMS: u32 = 0x01f0_1000;
const GAUGE: u32 = 0x340c80;
const SEED: u64 = 0x6b68_3272;

fn log(e: &mut Ee, kind: u32, a: u32, b: u32) {
    let n = e.r32(LOG);
    let at = LOG + 4 + 12 * n;
    e.w32(at, kind);
    e.w32(at + 4, a);
    e.w32(at + 8, b);
    e.w32(LOG, n + 1);
    e.gpr[reg::V0] = 0;
}

fn machine(what: &str) -> Option<Ee> {
    let elf = kh2_factory::exe_or_skip(what)?;
    let mut ee = Ee::new();
    ee.load_elf(&elf).expect("elf");
    // The setter, the item removal and the record-bit setter log their arguments; the item lookup reads the random table.
    ee.hooks.insert(0x17ff60, |e| log(e, 1, e.gpr[reg::A0] as u32, 0));
    ee.hooks.insert(0x1a1ba8, |e| log(e, 2, e.gpr[reg::A0] as u32, e.gpr[5] as u32));
    ee.hooks.insert(0x19d7e0, |e| log(e, 3, e.gpr[reg::A0] as u32, e.gpr[5] as u32));
    ee.hooks.insert(0x1a1cc8, |e| {
        let item = e.gpr[reg::A0] as u32 & 0xff;
        e.gpr[reg::V0] = e.r8(ITEMS + item) as u128;
    });
    Some(ee)
}

fn run(what: &str, spec: &Spec, port: impl FnMut(&mut Case) -> Out) {
    let Some(mut ee) = machine(what) else { return };
    let st = fuzz_with(&mut ee, spec, CASES, SEED, port);
    eprintln!("{what}: {} cases equal, {} interpreter steps in {:.3} s", st.cases, st.steps, st.secs);
}

/// The port's side of the environment: the flag pages and item table of the case, the gauge byte, and the log the hooks write.
struct Sim {
    flags: Vec<u8>,
    items: Vec<u8>,
    gauge: u8,
    log: Vec<u32>,
}

impl StoryEnv for Sim {
    fn flag(&self, id: i32) -> bool {
        check_flag(&self.flags, id)
    }
    fn set_flag(&mut self, id: i32) {
        self.log.extend([1, id as u32, 0]);
    }
    fn owned_count(&self, item: i32) -> u32 {
        self.items[item as usize & 0xff] as u32
    }
    fn remove_item(&mut self, item: i32, member: i32) {
        self.log.extend([2, item as u32, member as u32]);
    }
    fn drive_gauge_2(&self) -> u8 {
        self.gauge
    }
}

// Global indexes of the environment specs: flags, log, items, gauge bytes, then a table.
const ENV_GLOBALS: [(u32, u32, bool); 4] = [(FLAGS, FLAGS_LEN as u32, true), (LOG, 0x100, false), (ITEMS, 0x100, true), (GAUGE, 0x20, true)];

fn sim(c: &mut Case) -> Sim {
    Sim { flags: c.global(0).to_vec(), items: c.global(2).to_vec(), gauge: c.global(3)[0x12], log: Vec::new() }
}

fn write_log(c: &mut Case, at: usize, log: &[u32]) {
    let g = c.global(at);
    g[..4].copy_from_slice(&(log.len() as u32 / 3).to_le_bytes());
    for (i, v) in log.iter().enumerate() {
        g[4 + 4 * i..8 + 4 * i].copy_from_slice(&v.to_le_bytes());
    }
}

macro_rules! flag_spec {
    ($addr:expr, $args:expr, $ret:expr) => {
        Spec { addr: $addr, args: $args, floats: 0, ret: $ret, globals: &[(FLAGS, FLAGS_LEN as u32, true)], stubs: &[], nulls: false }
    };
}

#[test]
fn orig_1802e0_matches_the_game() {
    // Every id 0..=0xffff (the highest reaches word 31 of page 63, byte 0x85c), then the ids around 0xffff, the one id the original skips.
    static ALL: [Arg; 1] = [Arg::Range(0, 0xffff)];
    static EDGE: [Arg; 1] = [Arg::Range(0xfff0, 0xffff)];
    for args in [&ALL, &EDGE] {
        run("0x1802e0", &flag_spec!(0x1802e0, args, Ret::Int), |c| {
            let id = c.int(0);
            Out::int(check_flag(c.global(0), id) as i64)
        });
    }
}

#[test]
fn orig_180298_matches_the_game() {
    static ALL: [Arg; 1] = [Arg::Range(0, 0xffff)];
    run("0x180298", &flag_spec!(0x180298, &ALL, Ret::None), |c| {
        let id = c.int(0);
        clear_flag(c.global(0), id);
        Out::none()
    });
}

#[test]
fn orig_180330_matches_the_game() {
    run("0x180330", &flag_spec!(0x180330, &[], Ret::Int), |c| Out::int(flag_102b(c.global(0)) as i64));
}

#[test]
fn orig_180850_matches_the_game() {
    run("0x180850", &flag_spec!(0x180850, &[], Ret::Int), |c| Out::int(flag_8d0(c.global(0)) as i64));
}

#[test]
fn orig_180638_matches_the_game() {
    run("0x180638", &flag_spec!(0x180638, &[], Ret::Int), |c| Out::int(gate_81c(c.global(0)) as i64));
}

#[test]
fn orig_180680_matches_the_game() {
    run("0x180680", &flag_spec!(0x180680, &[], Ret::Int), |c| Out::int(gate_821(c.global(0)) as i64));
}

#[test]
fn orig_1806c8_matches_the_game() {
    run("0x1806c8", &flag_spec!(0x1806c8, &[], Ret::Int), |c| Out::int(gate_849(c.global(0)) as i64));
}

#[test]
fn orig_180710_matches_the_game() {
    run("0x180710", &flag_spec!(0x180710, &[], Ret::Int), |c| Out::int(gate_834(c.global(0)) as i64));
}

#[test]
fn orig_180758_matches_the_game() {
    run("0x180758", &flag_spec!(0x180758, &[], Ret::Int), |c| Out::int(gate_8aa(c.global(0)) as i64));
}

#[test]
fn orig_1807a0_matches_the_game() {
    run("0x1807a0", &flag_spec!(0x1807a0, &[], Ret::Int), |c| Out::int(gate_142b_2001(c.global(0)) as i64));
}

#[test]
fn orig_1807f8_matches_the_game() {
    run("0x1807f8", &flag_spec!(0x1807f8, &[], Ret::Int), |c| Out::int(gate_142b_2018(c.global(0)) as i64));
}

#[test]
fn orig_180350_matches_the_game() {
    // Two flag ids, drawn from a small set so the pair often shares a word and the target is often already set.
    static ARGS: [Arg; 2] = [Arg::Range(0x2c20, 0x2c3f), Arg::Range(0x2c20, 0x2c3f)];
    let spec = Spec { addr: 0x180350, args: &ARGS, floats: 0, ret: Ret::Int, globals: &ENV_GLOBALS, stubs: &[], nulls: false };
    run("0x180350", &spec, |c| {
        let (a, b) = (c.int(0), c.int(1));
        let mut s = sim(c);
        let r = set_when(&mut s, a, b);
        write_log(c, 1, &s.log);
        Out::int(r as i64)
    });
}

#[test]
fn orig_1803b0_matches_the_game() {
    // The five removal ids, then ids outside them (a wide range and the neighbours of the five); the random item table makes
    // ITEMS[0x20] non-zero in nearly every case and zero in some, so the pair loop runs and is skipped.
    static FIVE: [Arg; 1] = [Arg::Int(&[0x2409, 0x2411, 0x2419, 0x2421, 0x2428])];
    static WIDE: [Arg; 1] = [Arg::Range(0, 0x3000)];
    static NEAR: [Arg; 1] = [Arg::Int(&[0x2408, 0x240a, 0x2410, 0x2412, 0x2418, 0x241a, 0x2420, 0x2422, 0x2427, 0x2429, 0, 1, -1])];
    let mut globals = ENV_GLOBALS.to_vec();
    globals.push((0x37a650, 40, false));
    let globals: &'static [(u32, u32, bool)] = Box::leak(globals.into_boxed_slice());
    for args in [&FIVE, &WIDE, &NEAR] {
        let spec = Spec { addr: 0x1803b0, args, floats: 0, ret: Ret::None, globals, stubs: &[], nulls: false };
        run("0x1803b0", &spec, |c| {
            let id = c.int(0);
            let pairs = c.global(4).to_vec();
            let mut s = sim(c);
            story_chain(&mut s, &pairs, id);
            write_log(c, 1, &s.log);
            Out::none()
        });
    }
}

#[test]
fn orig_180450_matches_the_game() {
    let spec = Spec { addr: 0x180450, args: &[], floats: 0, ret: Ret::None, globals: &ENV_GLOBALS, stubs: &[], nulls: false };
    run("0x180450", &spec, |c| {
        let mut s = sim(c);
        staged_flags(&mut s);
        write_log(c, 1, &s.log);
        Out::none()
    });
}

#[test]
fn orig_180558_matches_the_game() {
    let spec = Spec { addr: 0x180558, args: &[], floats: 0, ret: Ret::None, globals: &ENV_GLOBALS, stubs: &[], nulls: false };
    run("0x180558", &spec, |c| {
        let mut s = sim(c);
        second_part_flags(&mut s);
        write_log(c, 1, &s.log);
        Out::none()
    });
}

#[test]
fn orig_1805d0_matches_the_game() {
    let Some(mut ee) = machine("0x1805d0") else { return };
    // The flag ids of the table, tried exactly and with their neighbours (read from the executable at run time).
    let ids: Vec<i32> = (0..14).map(|k| ee.r32(0x37a6f8 + 12 * k) as i32).collect();
    let ids: &'static [i32] = Box::leak(ids.into_boxed_slice());
    let args: &'static [Arg] = Box::leak(Box::new([Arg::Int(ids)]));
    let spec = Spec {
        addr: 0x1805d0,
        args,
        floats: 0,
        ret: Ret::None,
        globals: &[(LOG, 0x100, false), (0x37a6f8, 0xa8, false)],
        stubs: &[],
        nulls: false,
    };
    let st = fuzz_with(&mut ee, &spec, CASES, SEED, |c| {
        let id = c.int(0);
        let table = c.global(1).to_vec();
        let mut log = Vec::new();
        record_bits_for_flag(&table, id, |g, b| log.extend([3, g as u32, b as u32]));
        write_log(c, 0, &log);
        Out::none()
    });
    eprintln!("0x1805d0: {} cases equal", st.cases);
}

#[test]
fn orig_1a7780_matches_the_game() {
    static ARGS: [Arg; 1] = [Arg::Range(0, 0xfff)];
    let spec = Spec { addr: 0x1a7780, args: &ARGS, floats: 0, ret: Ret::None, globals: &[(0x33fe20, 0x200, true)], stubs: &[], nulls: false };
    run("0x1a7780", &spec, |c| {
        let n = c.int(0) as u32;
        save_flag_reset(c.global(0), n);
        Out::none()
    });
}

#[test]
fn orig_1a77b8_matches_the_game() {
    static ARGS: [Arg; 1] = [Arg::Range(0, 0xfff)];
    let spec = Spec { addr: 0x1a77b8, args: &ARGS, floats: 0, ret: Ret::Int, globals: &[(0x33fe20, 0x200, true)], stubs: &[], nulls: false };
    run("0x1a77b8", &spec, |c| {
        let n = c.int(0) as u32;
        Out::int(save_flag_check(c.global(0), n) as i64)
    });
}

const RECORD: Block = Block { size: 0x18, fields: &[], subs: &[] };

#[test]
fn orig_1a77e8_matches_the_game() {
    static ARGS: [Arg; 1] = [Arg::Ptr(RECORD)];
    let spec = Spec { addr: 0x1a77e8, args: &ARGS, floats: 0, ret: Ret::None, globals: &[], stubs: &[], nulls: false };
    run("0x1a77e8", &spec, |c| {
        reset_record(c.block(0));
        Out::none()
    });
}

// The eight-entry table: entries of (id word, aux word, switch byte); small values are drawn often so searches match.
const fn entry_fields(free_last: bool) -> [FieldSpec; 24] {
    let mut f = [(0, 4, Kind::Int); 24];
    let mut k = 0;
    while k < 8 {
        let o = 12 * k as u32;
        // A pointer-kind word is always 0 in the generated block: entry 7's id word stays free for the writers.
        f[3 * k] = (o, 4, if free_last && k == 7 { Kind::Ptr } else { Kind::Int });
        f[3 * k + 1] = (o + 4, 4, Kind::Int);
        f[3 * k + 2] = (o + 8, 1, Kind::Int);
        k += 1;
    }
    f
}

static ANY_FIELDS: [FieldSpec; 24] = entry_fields(false);
static FREE_FIELDS: [FieldSpec; 24] = entry_fields(true);
const TABLE: Block = Block { size: 0x60, fields: &ANY_FIELDS, subs: &[] };
/// The writers dereference a null pointer when no entry matches and none is free (the port does nothing then); keep the last entry free.
const TABLE_FREE: Block = Block { size: 0x60, fields: &FREE_FIELDS, subs: &[] };

fn table_spec(addr: u32, args: &'static [Arg], ret: Ret) -> Spec {
    Spec { addr, args, floats: 0, ret, globals: &[], stubs: &[], nulls: false }
}

fn index_to_addr(i: Option<usize>) -> i64 {
    i.map_or(0, |k| (block_addr(0) + (k * RECOM_ENTRY) as u32) as i64)
}

#[test]
fn orig_19de70_matches_the_game() {
    static ARGS: [Arg; 1] = [Arg::Ptr(TABLE)];
    run("0x19de70", &table_spec(0x19de70, &ARGS, Ret::None), |c| {
        recom_clear(c.block(0));
        Out::none()
    });
}

#[test]
fn orig_19dea0_matches_the_game() {
    static ARGS: [Arg; 1] = [Arg::Ptr(TABLE)];
    run("0x19dea0", &table_spec(0x19dea0, &ARGS, Ret::Int), |c| Out::int(index_to_addr(recom_first_free(c.block(0)))));
}

#[test]
fn orig_19ded0_matches_the_game() {
    static ARGS: [Arg; 3] = [Arg::Ptr(TABLE), Arg::Range(0, 3), Arg::Range(-1, 3)];
    run("0x19ded0", &table_spec(0x19ded0, &ARGS, Ret::Int), |c| {
        let (a, b) = (c.int(1), c.int(2));
        Out::int(index_to_addr(recom_find(c.block(0), a, b)))
    });
}

#[test]
fn orig_19df08_matches_the_game() {
    static ARGS: [Arg; 3] = [Arg::Ptr(TABLE_FREE), Arg::Range(0, 3), Arg::Range(-1, 3)];
    run("0x19df08", &table_spec(0x19df08, &ARGS, Ret::None), |c| {
        let (a, b) = (c.int(1), c.int(2));
        recom_switch_on(c.block(0), a, b);
        Out::none()
    });
}

#[test]
fn orig_19df68_matches_the_game() {
    static ARGS: [Arg; 3] = [Arg::Ptr(TABLE_FREE), Arg::Range(0, 3), Arg::Range(-1, 3)];
    run("0x19df68", &table_spec(0x19df68, &ARGS, Ret::None), |c| {
        let (a, b) = (c.int(1), c.int(2));
        recom_switch_off(c.block(0), a, b);
        Out::none()
    });
}

#[test]
fn orig_19dfc0_matches_the_game() {
    static ARGS: [Arg; 3] = [Arg::Ptr(TABLE), Arg::Range(0, 3), Arg::Range(-1, 3)];
    run("0x19dfc0", &table_spec(0x19dfc0, &ARGS, Ret::None), |c| {
        let (a, b) = (c.int(1), c.int(2));
        recom_remove(c.block(0), a, b);
        Out::none()
    });
}

#[test]
fn orig_19dfe8_matches_the_game() {
    static ARGS: [Arg; 3] = [Arg::Ptr(TABLE), Arg::Range(0, 3), Arg::Range(-1, 3)];
    run("0x19dfe8", &table_spec(0x19dfe8, &ARGS, Ret::Int), |c| {
        let (a, b) = (c.int(1), c.int(2));
        Out::int(recom_is_on(c.block(0), a, b) as i64)
    });
}

#[test]
fn orig_19e038_matches_the_game() {
    static ARGS: [Arg; 3] = [Arg::Ptr(TABLE), Arg::Range(0, 3), Arg::Range(-1, 3)];
    run("0x19e038", &table_spec(0x19e038, &ARGS, Ret::Int), |c| {
        let (a, b) = (c.int(1), c.int(2));
        Out::int(recom_is_off(c.block(0), a, b) as i64)
    });
}
