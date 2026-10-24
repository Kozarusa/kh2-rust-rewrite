//! Oracle tests for the small address and accessor functions of the save and world flag units (WP-L1-P18a2): the region word getter
//! `FUN_00105cd8`, the journal address `FUN_002b70e0` (over `FUN_001057a8`), the world record address `FUN_001a71c0`, the item table
//! walk `FUN_001a6730` and the route state address `FUN_001c0950`. kh2-ee runs the original from the user's executable and the port runs on
//! the same inputs. No callee is replaced. The item table walk runs on tables laid out in memory behind the pointer word `0x1c95588` by
//! a hand loop (the fuzz harness draws random words, which are almost never a mapped table).
//!
//! Skips without a disc (KH2_ISO or local/iso.txt); fails without one when KH2_REQUIRE_DISC=1.

use kh2_ee::Ee;
use kh2_factory::fuzz::*;
use kh2_game_core::progress_flags::course_address;
use kh2_game_core::save_block::{self as sb, BootEnv};

fn report(addr: u32, st: Stats) {
    eprintln!("{addr:#x}: {} cases equal, {} interpreter steps in {:.3} s", st.cases, st.steps, st.secs);
}

#[test]
fn orig_105cd8_matches_the_game() {
    static SPEC: Spec = Spec { addr: 0x105cd8, args: &[], floats: 0, ret: Ret::Int, globals: &[(0x349d44, 4, true)], stubs: &[], nulls: false };
    let Some(st) = fuzz(&SPEC, 1000, |c| {
        let region = u32::from_le_bytes(c.global(0).try_into().unwrap());
        Out::int(sb::region_word(&BootEnv { region, fixups: Vec::new() }) as i32 as i64)
    }) else {
        return;
    };
    report(0x105cd8, st);
}

/// The address getter answers a constant, so the fuzz harness (which refuses a test whose cases all agree) cannot run it: one call.
#[test]
fn orig_2b70e0_matches_the_game() {
    let Some(elf) = kh2_factory::exe_or_skip("oracle test of 0x2b70e0") else { return };
    let mut ee = Ee::new();
    ee.load_elf(&elf).expect("elf");
    let v = ee.call(0x2b70e0, &[], &[], STACK).expect("FUN_002b70e0") as u32;
    assert_eq!(v, sb::journal_seen_address());
    assert_eq!(v, 0x33e860 + 0x38f0);
}

/// Worlds below zero pick the current world byte (`0x33e830`, randomised), the others are used as given.
#[test]
fn orig_1a71c0_matches_the_game() {
    static SPEC: Spec = Spec { addr: 0x1a71c0, args: &[Arg::Int(&[0, 1, 18, 19, -1, i32::MIN, i32::MAX])], floats: 0, ret: Ret::Int, globals: &[(0x33e830, 1, true)], stubs: &[], nulls: false };
    let Some(st) = fuzz(&SPEC, 2000, |c| {
        let cur = c.global(0)[0];
        Out::int(sb::world_record_address(i64::from(c.int(0)), cur) as i32 as i64)
    }) else {
        return;
    };
    report(0x1a71c0, st);
}

#[test]
fn orig_1c0950_matches_the_game() {
    static SPEC: Spec = Spec { addr: 0x1c0950, args: &[Arg::Int(&[0, 1, 21, 255, -1, i32::MIN, i32::MAX])], floats: 0, ret: Ret::Int, globals: &[], stubs: &[], nulls: false };
    let Some(st) = fuzz(&SPEC, 2000, |c| Out::int(course_address(c.int(0)) as i32 as i64)) else { return };
    report(0x1c0950, st);
}

/// The walk over tables of 0 to 6 records (and a few counts the original would run off with), from no record, from every record, from a
/// record past the end, and from addresses that are no record at all.
#[test]
fn orig_1a6730_matches_the_game() {
    let Some(elf) = kh2_factory::exe_or_skip("oracle test of 0x1a6730") else { return };
    let mut ee = Ee::new();
    ee.load_elf(&elf).expect("elf");
    const TABLE: u32 = 0x0180_0000;
    let mut rng = Rng::new(0x1a67);
    let (mut firsts, mut nexts, mut ends) = (0u32, 0u32, 0u32);
    for n in 0..600 {
        let count: i32 = match n % 12 {
            0..=8 => (n % 7) as i32,
            9 => -1,
            10 => 0x100,
            _ => rng.word() as i32 & 0xff,
        };
        ee.w32(0x1c95588, TABLE);
        ee.w32(TABLE + 4, count as u32);
        let first = TABLE + 8;
        let mut prevs = vec![0i64, first as i64, (first + 0x18 * count.max(0) as u32) as i64, (first + 0x18 * count.max(0) as u32 + 0x18) as i64];
        prevs.extend((0..4).map(|_| (first as i64 + (rng.word() % 0x400) as i64) - 0x20));
        prevs.push(-8);
        for prev in prevs {
            let game = ee.call(0x1a6730, &[prev as u64], &[], STACK).expect("FUN_001a6730") as u32;
            let port = sb::item_table_each(prev, TABLE, count);
            assert_eq!(port, game, "case {n}: prev {prev:#x}, count {count}");
            match (prev == 0, game == 0) {
                (true, _) => firsts += 1,
                (false, true) => ends += 1,
                _ => nexts += 1,
            }
        }
    }
    assert!(firsts > 0 && nexts > 0 && ends > 0, "all three outcomes occur ({firsts} first, {nexts} next, {ends} end)");
}
