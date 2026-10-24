//! Oracle tests for the save remainder (core.save, WP-L1-P21): the save block's static initialiser `FUN_001057b8` and its entry
//! `FUN_00105890`, the game mode getter `FUN_00105750`, the address getter `FUN_001057a8`, the story-flag event map `FUN_00195130` (with
//! its helper `FUN_00194fa8`), the item-to-equipment lookup `FUN_001a67b0` over the table search `FUN_001d0120`, the give list
//! `FUN_001d7508`, the empty `FUN_001ddff0`, the card unit's buffer frees `FUN_0023cac0`, `FUN_0023e410`, `FUN_0023c8a8` and the
//! static initialiser of the world record at `0x36cee0` (`FUN_00254038`, `FUN_002540a8`). kh2-ee runs the original from the user's
//! executable on random inputs and the port runs on copies of the same bytes; every byte of every block and global the function may touch is
//! compared (the harness of `kh2_factory::fuzz`, 1,000 to 3,000 cases per function).
//!
//! Stand-ins, all documented here and nowhere else:
//! - `FUN_001057b8` runs for real on a save block full of random bytes (every callee below it is the real original); the object block's
//!   fix-up list is the executable's own (`0x37c7d8`), passed to the port as words. The whole block (0xb4e0 bytes) is compared.
//! - The table search runs on tables laid out in memory behind the pointer word `0x1d6a920` (a count word at `+4`, records of 16 bytes
//!   from `+8` sorted by their first halfword). Four tables are used: a dense one with small ids, one with ids at the halfword's sign
//!   boundary, an empty one and a one-record one. The comparator at `0x333ff8` runs for real.
//! - The give list's callee `FUN_001a18d8` is replaced by a hook that logs its three arguments; the port logs the same triple through its
//!   closure and the log is a compared global, so every call is checked in order.
//! - The card unit's two release routines `FUN_001062b0` and `FUN_00106280` are replaced by hooks that log (kind, pointer); the unit's ten
//!   pointer words are set by hand to a random mix of zero and non-zero values (a hand loop: the fuzz harness draws random bytes, which are
//!   almost never zero), and every word and the log are compared after the call.
//!
//! Skips without a disc (KH2_ISO or local/iso.txt); fails without one when KH2_REQUIRE_DISC=1.

use kh2_ee::{Ee, reg};
use kh2_factory::fuzz::*;
use kh2_game_core::save_block::{self as sb, CardWords, LEN, Release, SAVE_ADDR};

const SAVE_GLOBAL: (u32, u32, bool) = (SAVE_ADDR, LEN as u32, true);
const FIXUPS: u32 = 0x37c7d8;
const FIXUPS_GLOBAL: (u32, u32, bool) = (FIXUPS, 0x50, false);
const LOG: u32 = 0x01f0_0000;
const LOG_LEN: u32 = 0x1000;
const TABLE: u32 = 0x01e0_0000;
const TABLE_PTR: u32 = 0x1d6a920;

fn words(b: &[u8]) -> Vec<u32> {
    b.chunks(4).map(|c| u32::from_le_bytes(c.try_into().unwrap())).collect()
}

fn report(addr: u32, st: Stats) {
    eprintln!("{addr:#x}: {} cases equal, {} interpreter steps in {:.3} s", st.cases, st.steps, st.secs);
}

// --- the getters and the empty function ------------------------------------------------------------------------------------

#[test]
fn orig_105750_matches_the_game() {
    static SPEC: Spec = Spec { addr: 0x105750, args: &[Arg::Ptr(Block { size: 0x1700, fields: &[], subs: &[] })], floats: 0, ret: Ret::Int, globals: &[], stubs: &[], nulls: false };
    let Some(st) = fuzz(&SPEC, 1000, |c| Out::int(sb::game_level(c.block(0)) as i32 as i64)) else { return };
    report(0x105750, st);
}

/// The address getter answers a constant, so the fuzz harness (which refuses a test whose cases all agree) cannot run it: one call.
#[test]
fn orig_1057a8_matches_the_game() {
    let Some(elf) = kh2_factory::exe_or_skip("oracle test of 0x1057a8") else { return };
    let mut ee = Ee::new();
    ee.load_elf(&elf).expect("elf");
    let v = ee.call(0x1057a8, &[], &[], STACK).expect("FUN_001057a8") as u32;
    assert_eq!(v, sb::save_ram_address());
    assert_eq!(v, 0x33e860);
}

/// The empty function changes no byte of memory.
#[test]
fn orig_1ddff0_matches_the_game() {
    let Some(elf) = kh2_factory::exe_or_skip("oracle test of 0x1ddff0") else { return };
    let mut ee = Ee::new();
    ee.load_elf(&elf).expect("elf");
    ee.w32(SAVE_ADDR, 0x1234_5678);
    let before = ee.ram.clone();
    ee.call(0x1ddff0, &[], &[], STACK).expect("FUN_001ddff0");
    let changed = (0..before.len()).filter(|&i| before[i] != ee.ram[i] && !(STACK - 0x1000..STACK + 0x100).contains(&(i as u32))).count();
    assert_eq!(changed, 0, "the original's empty function wrote {changed} bytes outside the stack");
    sb::object_block_hook();
}

// --- the static initialisers ---------------------------------------------------------------------------------------------------

/// Phases 0 to 2 and tokens 0xfffe to 0x10000: the branch (1, 0xffff) runs in about one case of nine, phase 0 (the empty function) in about
/// as many.
static INIT_ARGS: [Arg; 2] = [Arg::Range(0, 2), Arg::Range(0xfffe, 0x10000)];

#[test]
fn orig_1057b8_matches_the_game() {
    static SPEC: Spec = Spec { addr: 0x1057b8, args: &INIT_ARGS, floats: 0, ret: Ret::None, globals: &[SAVE_GLOBAL, FIXUPS_GLOBAL], stubs: &[], nulls: false };
    let Some(st) = fuzz(&SPEC, 1500, |c| {
        let (phase, token) = (c.int(0), c.ints[1] as i64);
        let fixups = words(c.global(1));
        sb::static_init(c.global(0), &fixups, phase, token);
        Out::none()
    }) else {
        return;
    };
    report(0x1057b8, st);
}

#[test]
fn orig_105890_matches_the_game() {
    static SPEC: Spec = Spec { addr: 0x105890, args: &[], floats: 0, ret: Ret::None, globals: &[SAVE_GLOBAL, FIXUPS_GLOBAL], stubs: &[], nulls: false };
    let Some(st) = fuzz(&SPEC, 1000, |c| {
        let fixups = words(c.global(1));
        sb::static_init_boot(c.global(0), &fixups);
        Out::none()
    }) else {
        return;
    };
    report(0x105890, st);
}

#[test]
fn orig_254038_matches_the_game() {
    static SPEC: Spec = Spec { addr: 0x254038, args: &INIT_ARGS, floats: 0, ret: Ret::None, globals: &[(0x36cee0, 4, true)], stubs: &[], nulls: false };
    let Some(st) = fuzz(&SPEC, 1500, |c| {
        let (phase, token) = (c.int(0), c.ints[1] as i64);
        sb::static_init_world_slot(c.global(0), phase, token);
        Out::none()
    }) else {
        return;
    };
    report(0x254038, st);
}

/// The entry always leaves the same four bytes, so the fuzz harness (which wants varying results) cannot run it: a hand loop over random
/// starting bytes compares the word after each call.
#[test]
fn orig_2540a8_matches_the_game() {
    let Some(elf) = kh2_factory::exe_or_skip("oracle test of 0x2540a8") else { return };
    let mut ee = Ee::new();
    ee.load_elf(&elf).expect("elf");
    let mut rng = Rng::new(0x2540);
    for n in 0..300 {
        let mut slot = [0u8; 4];
        slot.iter_mut().for_each(|b| *b = rng.word() as u8);
        ee.write(0x36cee0, &slot);
        ee.call(0x2540a8, &[], &[], STACK).expect("FUN_002540a8");
        let game: Vec<u8> = (0..4).map(|i| ee.r8(0x36cee0 + i)).collect();
        sb::static_init_world_slot_boot(&mut slot);
        assert_eq!(slot.to_vec(), game, "case {n}");
        assert_ne!(slot, [0; 4], "the slot is initialised");
    }
}

// --- the story-flag event map ------------------------------------------------------------------------------------------------

#[test]
fn orig_194fa8_matches_the_game() {
    static SPEC: Spec = Spec { addr: 0x194fa8, args: &[Arg::Range(0, 95)], floats: 0, ret: Ret::None, globals: &[SAVE_GLOBAL], stubs: &[], nulls: false };
    let Some(st) = fuzz(&SPEC, 1000, |c| {
        let bit = c.int(0) as u32;
        sb::set_world_point_bit(c.global(0), bit);
        Out::none()
    }) else {
        return;
    };
    report(0x194fa8, st);
}

/// Every flag the map lists, with its neighbours, plus random ids.
#[test]
fn orig_195130_matches_the_game() {
    static SPEC: Spec = Spec {
        addr: 0x195130,
        args: &[Arg::Int(&[0x1877, 0x1822, 0x2822, 0x2023, 0x3012, 0x401e, 0x1061, 0x8b8, 0x141f])],
        floats: 0,
        ret: Ret::None,
        globals: &[SAVE_GLOBAL],
        stubs: &[],
        nulls: false,
    };
    let Some(st) = fuzz(&SPEC, 3000, |c| {
        let flag = c.int(0);
        sb::story_flag_world_point(c.global(0), flag);
        Out::none()
    }) else {
        return;
    };
    report(0x195130, st);
}

// --- the table search and the item-to-equipment lookup -----------------------------------------------------------------------------

/// A table of `count` records: sorted distinct ids drawn from `pool`, the rest of each record random.
fn equipment_table(rng: &mut Rng, count: usize, pool: &[u16]) -> Vec<u8> {
    let mut ids: Vec<u16> = Vec::new();
    while ids.len() < count {
        let id = pool[rng.below(pool.len() as u64) as usize];
        if !ids.contains(&id) {
            ids.push(id);
        }
    }
    ids.sort_unstable();
    let mut t: Vec<u8> = (0..8 + 16 * count).map(|_| rng.word() as u8).collect();
    t[4..8].copy_from_slice(&(count as u32).to_le_bytes());
    for (i, id) in ids.iter().enumerate() {
        t[8 + 16 * i..10 + 16 * i].copy_from_slice(&id.to_le_bytes());
    }
    t
}

/// The four tables of the search tests.
fn tables() -> Vec<Vec<u8>> {
    let mut rng = Rng::new(0x1d01);
    let dense: Vec<u16> = (0..60).collect();
    let edge: Vec<u16> = vec![0, 1, 2, 3, 0x7fff, 0x8000, 0x8001, 0xfffe, 0xffff, 5, 9, 17, 31, 0x4000];
    vec![equipment_table(&mut rng, 30, &dense), equipment_table(&mut rng, 14, &edge), equipment_table(&mut rng, 0, &dense), equipment_table(&mut rng, 1, &dense)]
}

fn table_ids(t: &[u8]) -> Vec<i32> {
    let n = u32::from_le_bytes(t[4..8].try_into().unwrap()) as usize;
    (0..n).map(|i| u16::from_le_bytes([t[8 + 16 * i], t[9 + 16 * i]]) as i32).collect()
}

fn table_machine(what: &str, t: &[u8]) -> Option<Ee> {
    let elf = kh2_factory::exe_or_skip(what)?;
    let mut ee = Ee::new();
    ee.load_elf(&elf).expect("elf");
    ee.write(TABLE, t);
    ee.w32(TABLE_PTR, TABLE);
    Some(ee)
}

fn found(off: Option<usize>) -> Out {
    Out::int(off.map_or(0, |o| TABLE + o as u32) as i32 as i64)
}

#[test]
fn orig_1d0120_matches_the_game() {
    for (k, t) in tables().iter().enumerate() {
        let Some(mut ee) = table_machine("oracle test of 0x1d0120", t) else { return };
        if t[4..8] == [0; 4] {
            // No record: every key answers 0, so the fuzz harness (which wants varying results) cannot run it; a hand loop does.
            let mut rng = Rng::new(0x1d02);
            for _ in 0..300 {
                let key = rng.int(&[]);
                let game = ee.call(0x1d0120, &[key as i64 as u64], &[], STACK).expect("FUN_001d0120") as u32;
                assert_eq!(game, 0);
                assert_eq!(sb::find_equipment_record(t, i64::from(key)), None);
            }
            continue;
        }
        let consts: &'static [i32] = Box::leak(table_ids(t).into_boxed_slice());
        let args: &'static [Arg] = Box::leak(Box::new([Arg::Int(consts)]));
        let globals: &'static [(u32, u32, bool)] = Box::leak(Box::new([(TABLE, t.len() as u32, false)]));
        let spec = Spec { addr: 0x1d0120, args, floats: 0, ret: Ret::Int, globals, stubs: &[], nulls: false };
        let st = fuzz_with(&mut ee, &spec, 1000, 0x1d00 + k as u64, |c| {
            let key = c.ints[0] as i32 as i64;
            let table = c.global(0).to_vec();
            found(sb::find_equipment_record(&table, key))
        });
        report(0x1d0120, st);
    }
}

#[test]
fn orig_1a67b0_matches_the_game() {
    static ITEM: Block = Block { size: 8, fields: &[(2, 1, Kind::Int), (4, 2, Kind::Int), (6, 2, Kind::Int)], subs: &[] };
    for (k, t) in tables().iter().enumerate() {
        let Some(mut ee) = table_machine("oracle test of 0x1a67b0", t) else { return };
        let globals: &'static [(u32, u32, bool)] = Box::leak(Box::new([(TABLE, t.len() as u32, false)]));
        let args: &'static [Arg] = Box::leak(Box::new([Arg::Ptr(ITEM)]));
        let spec = Spec { addr: 0x1a67b0, args, floats: 0, ret: Ret::Int, globals, stubs: &[], nulls: false };
        let st = fuzz_with(&mut ee, &spec, 2000, 0x1a60 + k as u64, |c| {
            let table = c.global(0).to_vec();
            let item = c.block(0).to_vec();
            found(sb::item_equipment_record(&item, &table))
        });
        report(0x1a67b0, st);
    }
}

// --- the give list ------------------------------------------------------------------------------------------------------------

fn log3(e: &mut Ee, a: u32, b: u32, c: u32) {
    let n = e.r32(LOG);
    let at = LOG + 4 + 12 * n;
    e.w32(at, a);
    e.w32(at + 4, b);
    e.w32(at + 8, c);
    e.w32(LOG, n + 1);
    e.gpr[reg::V0] = 0;
}

#[test]
fn orig_1d7508_matches_the_game() {
    static FIELDS: [(u32, u8, Kind); 32] = {
        let mut f = [(0u32, 2u8, Kind::Int); 32];
        let mut i = 0;
        while i < 32 {
            f[i].0 = 2 * i as u32;
            i += 1;
        }
        f
    };
    static SPEC: Spec = Spec {
        addr: 0x1d7508,
        args: &[Arg::Ptr(Block { size: 0x40, fields: &FIELDS, subs: &[] }), Arg::Range(-3, 24)],
        floats: 0,
        ret: Ret::None,
        globals: &[(LOG, LOG_LEN, false)],
        stubs: &[],
        nulls: false,
    };
    let Some(elf) = kh2_factory::exe_or_skip("oracle test of 0x1d7508") else { return };
    let mut ee = Ee::new();
    ee.load_elf(&elf).expect("elf");
    ee.hooks.insert(0x1a18d8, |e| {
        let (a, b, c) = (e.gpr[reg::A0] as u32, e.gpr[5] as u32, e.gpr[6] as u32 & 0xff);
        log3(e, a, b, c)
    });
    let st = fuzz_with(&mut ee, &SPEC, 2000, 0x1d75, |c| {
        let count = c.int(1);
        let mut ids: Vec<u16> = c.block(0).chunks(2).map(|h| u16::from_le_bytes([h[0], h[1]])).collect();
        let mut log: Vec<u32> = Vec::new();
        sb::give_list(&mut ids, count, |id| log.extend([id as u32, sb::GIVE_LIST_TARGET as u32, 0]));
        for (h, v) in c.block(0).chunks_mut(2).zip(&ids) {
            h.copy_from_slice(&v.to_le_bytes());
        }
        let g = c.global(0);
        g[..4].copy_from_slice(&(log.len() as u32 / 3).to_le_bytes());
        for (i, v) in log.iter().enumerate() {
            g[4 + 4 * i..8 + 4 * i].copy_from_slice(&v.to_le_bytes());
        }
        Out::none()
    });
    report(0x1d7508, st);
}

// --- the card unit's frees ---------------------------------------------------------------------------------------------------

/// The unit's words in memory order: ready, name, path, file, info, list, table, table allocation, then (two words on) the CRC flag and table.
const CARD_ADDRS: [u32; 10] = [0x36cb08, 0x36cb0c, 0x36cb10, 0x36cb14, 0x36cb18, 0x36cb1c, 0x36cb20, 0x36cb24, 0x36cb38, 0x36cb3c];

fn card_to_words(c: &CardWords) -> [u32; 10] {
    [c.ready, c.name, c.path, c.file, c.info, c.list, c.table, c.table_alloc, c.crc_flag, c.crc_table]
}

fn card_from_words(w: [u32; 10]) -> CardWords {
    CardWords { ready: w[0], name: w[1], path: w[2], file: w[3], info: w[4], list: w[5], table: w[6], table_alloc: w[7], crc_flag: w[8], crc_table: w[9] }
}

fn card_case(addr: u32, min_frees: usize, port: fn(&mut CardWords, &mut Vec<(u32, u32)>)) {
    let Some(elf) = kh2_factory::exe_or_skip(&format!("oracle test of {addr:#x}")) else { return };
    let mut ee = Ee::new();
    ee.load_elf(&elf).expect("elf");
    ee.hooks.insert(0x1062b0, |e| {
        let a = e.gpr[reg::A0] as u32;
        log3(e, 1, a, 0)
    });
    ee.hooks.insert(0x106280, |e| {
        let a = e.gpr[reg::A0] as u32;
        log3(e, 2, a, 0)
    });
    let mut rng = Rng::new(0x23c0 + u64::from(addr));
    let mut frees = 0usize;
    for n in 0..1500 {
        // Each word is zero in about a third of the cases, otherwise a random non-zero address-like value.
        let words: [u32; 10] = std::array::from_fn(|_| if rng.below(3) == 0 { 0 } else { (rng.word() as u32 | 1) & 0x01ff_fff0 | 0x0100_0000 });
        for (a, w) in CARD_ADDRS.iter().zip(words) {
            ee.w32(*a, w);
        }
        ee.write(LOG, &vec![0u8; LOG_LEN as usize]);
        ee.call(addr, &[], &[], STACK).expect("the original");
        let game: Vec<u32> = CARD_ADDRS.iter().map(|&a| ee.r32(a)).collect();
        let count = ee.r32(LOG) as usize;
        let game_log: Vec<(u32, u32)> = (0..count).map(|i| (ee.r32(LOG + 4 + 12 * i as u32), ee.r32(LOG + 8 + 12 * i as u32))).collect();
        let mut mine = card_from_words(words);
        let mut my_log = Vec::new();
        port(&mut mine, &mut my_log);
        assert_eq!(card_to_words(&mine).to_vec(), game, "case {n}: words {words:x?}");
        assert_eq!(my_log, game_log, "case {n}: frees, words {words:x?}");
        frees += game_log.len();
    }
    assert!(frees > min_frees,"the test frees too little ({frees})");
}

fn record(log: &mut Vec<(u32, u32)>) -> impl FnMut(Release, u32) + '_ {
    |kind, p| log.push((if kind == Release::Delete { 1 } else { 2 }, p))
}

#[test]
fn orig_23cac0_matches_the_game() {
    card_case(0x23cac0, 3000, |c, log| c.free_buffers(record(log)));
}

#[test]
fn orig_23e410_matches_the_game() {
    card_case(0x23e410, 600, |c, log| c.free_crc(record(log)));
}

#[test]
fn orig_23c8a8_matches_the_game() {
    card_case(0x23c8a8, 3000, |c, log| c.shutdown(record(log)));
}
