//! Oracle tests of the exit lock query `FUN_00195358` (with its table lookup `FUN_00195208`) and the `wldf` record search `FUN_001b68c0`
//! (WP-L1-P27a). kh2-ee runs the original from the user's executable on tables laid out in guest memory, the port
//! (`kh2_ard::progress::exit_locked`, `lock_index`, `worldmap::Wldf::find`) runs on the same bytes, and the answers are compared over
//! thousands of random cases.
//!
//! Stand-ins: the lock query asks `FUN_001d32c0` (the world 0x10 story-flag table, which the port takes as its last argument) when the
//! lock halfword is 0; it is a hook that answers a scripted 0 or 1, and the port is given the same value. The record search runs for real
//! with `FUN_0030ff40` and its compare function.
//!
//! Skips without a disc; fails without one when `KH2_REQUIRE_DISC=1`.

use kh2_ard::progress::{exit_locked, lock_index};
use kh2_ard::worldmap::Wldf;
use kh2_ee::{Ee, reg};
use kh2_factory::fuzz::{Rng, STACK};

const TABLE_PTR: u32 = 0x01c8_a988;
const LOCKS: u32 = 0x33f918;
const TABLE: u32 = 0x01f0_0000;

fn machine(what: &str) -> Option<Ee> {
    let elf = kh2_factory::exe_or_skip(what)?;
    let mut ee = Ee::new();
    ee.load_elf(&elf).expect("elf");
    Some(ee)
}

/// A lock table: a count and `world, area` byte pairs drawn small so a lookup hits, sometimes with duplicates.
fn lock_table(rng: &mut Rng) -> Vec<u8> {
    let n = rng.below(26) as usize;
    let mut t = (n as u16).to_le_bytes().to_vec();
    for _ in 0..n {
        t.push(rng.below(5) as u8);
        t.push(rng.below(8) as u8);
    }
    t
}

fn setup_locks(ee: &mut Ee, rng: &mut Rng, table: &[u8]) -> Vec<u16> {
    ee.write(TABLE, table);
    ee.w32(TABLE_PTR, TABLE);
    let words: Vec<u16> = (0..26).map(|_| [0u16, 0, 1, 2, 0xffff, rng.word() as u16][rng.below(6) as usize]).collect();
    for (i, w) in words.iter().enumerate() {
        // The halfword of lock `i` is at `0x33f918 + 4 * i`; the message halfword beside it is random.
        ee.write(LOCKS + 4 * i as u32, &w.to_le_bytes());
        ee.write(LOCKS + 4 * i as u32 + 2, &(rng.word() as u16).to_le_bytes());
    }
    words
}

/// `FUN_00195208(world, area)`: the index of the pair in the lock table, or -1.
#[test]
fn orig_195208_matches_the_game() {
    let Some(mut ee) = machine("oracle test of 0x195208") else { return };
    let mut rng = Rng::new(0x195208);
    let (mut hits, mut misses) = (0, 0);
    for case in 0..4000 {
        let table = lock_table(&mut rng);
        setup_locks(&mut ee, &mut rng, &table);
        let (world, area) = (rng.below(6) as u8, rng.below(9) as u8);
        let game = ee.call(0x195208, &[world as u64, area as u64], &[], STACK).expect("original") as i32;
        let port = lock_index(&table, world, area).map_or(-1, |i| i as i32);
        assert_eq!(game, port, "case {case}: ({world}, {area}) in {table:x?}");
        if port >= 0 { hits += 1 } else { misses += 1 }
    }
    assert!(hits > 500 && misses > 500, "{hits} hits, {misses} misses");
}

/// `FUN_00195358(world, area)`: 0 for an open exit, -2 for a locked one.
#[test]
fn orig_195358_matches_the_game() {
    let Some(mut ee) = machine("oracle test of 0x195358") else { return };
    let mut rng = Rng::new(0x195358);
    let answer = std::rc::Rc::new(std::cell::Cell::new(0u64));
    let a = answer.clone();
    ee.hooks.insert(0x1d32c0, move |e: &mut Ee| e.gpr[reg::V0] = u128::from(a.get()));
    let (mut locked, mut open, mut by_story) = (0, 0, 0);
    for case in 0..6000 {
        let table = lock_table(&mut rng);
        let words = setup_locks(&mut ee, &mut rng, &table);
        let (world, area) = (rng.below(6) as u8, rng.below(9) as u8);
        answer.set(rng.below(2));
        let game = ee.call(0x195358, &[world as u64, area as u64], &[], STACK).expect("original") as u32;
        let port = exit_locked(&table, |i| words[i], world, area, answer.get() != 0);
        assert_eq!(game, if port { 0xffff_fffe } else { 0 }, "case {case}: ({world}, {area}) in {table:x?}, halfwords {words:x?}, story answer {}", answer.get());
        if port { locked += 1 } else { open += 1 }
        by_story += (port && lock_index(&table, world, area).is_some_and(|i| words[i] == 0)) as u32;
    }
    assert!(locked > 500 && open > 500 && by_story > 50, "{locked} locked, {open} open, {by_story} locked by the story table");
}

/// The `wldf` table: a word, the record count at `+4` and 16-byte records from `+8`; the original's pointer is `DAT_01d5d790` and
/// `FUN_001b68c0(key)` is a binary search on the first byte of the records, which the port repeats probe for probe.
#[test]
fn orig_1b68c0_matches_the_game() {
    let Some(mut ee) = machine("oracle test of 0x1b68c0") else { return };
    let mut rng = Rng::new(0x1b68c0);
    let (mut found, mut missing) = (0, 0);
    for case in 0..4000 {
        // Sorted keys mostly (what the data has), sometimes a repeated key or an unsorted table, over 0 to 147 records.
        let n = [0usize, 1, 2, 3, 7, 16, 100, 147][rng.below(8) as usize].min(rng.below(150) as usize + 1);
        let mut keys: Vec<u8> = (0..n).map(|_| rng.word() as u8).collect();
        if rng.below(4) != 0 {
            keys.sort_unstable();
        }
        if rng.below(4) == 0 && n > 2 {
            keys[n / 2] = keys[n / 2 - 1];
        }
        let mut entry = vec![0u8; 8 + 16 * n];
        entry[4..8].copy_from_slice(&(n as u32).to_le_bytes());
        for (i, k) in keys.iter().enumerate() {
            let r = 8 + 16 * i;
            for b in &mut entry[r..r + 16] {
                *b = rng.word() as u8;
            }
            entry[r] = *k;
        }
        ee.write(TABLE, &entry);
        ee.w32(0x1d5d790, TABLE);
        let port = Wldf::parse(&entry);
        let key = if rng.below(2) == 0 && n > 0 { keys[rng.below(n as u64) as usize] } else { rng.word() as u8 };
        let at = ee.call(0x1b68c0, &[key as u64], &[], STACK).expect("original") as u32;
        let game: Option<[u8; 16]> = (at != 0).then(|| std::array::from_fn(|k| ee.r8(at + k as u32)));
        let want = port.find(key).map(|r| {
            let mut b = [0u8; 16];
            b[0] = r.index;
            b[1] = r.kind;
            b[4..8].copy_from_slice(&r.story_flag.to_le_bytes());
            b[8..12].copy_from_slice(&r.event.to_le_bytes());
            b[12..16].copy_from_slice(&r.list.to_le_bytes());
            b
        });
        // The record's address in the table tells which record the search picked (a repeated key picks by probe order).
        if let (Some(g), Some(w)) = (game, want) {
            assert_eq!((g[0], g[1], &g[4..]), (w[0], w[1], &w[4..]), "case {case}: key {key} in {n} records, keys {keys:x?}");
            found += 1;
        } else {
            assert_eq!(game.is_some(), want.is_some(), "case {case}: key {key} in {n} records, keys {keys:x?}");
            missing += 1;
        }
    }
    assert!(found > 500 && missing > 100, "{found} found, {missing} missing");
}
