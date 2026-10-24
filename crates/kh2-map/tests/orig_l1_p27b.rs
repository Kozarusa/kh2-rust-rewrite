//! Oracle tests of the map object getters (WP-L1-P27b): kh2-ee runs the original from the user's own executable and the port is
//! `kh2_map::mapobj::MapObject`. The eighteen getters were compared in `orig_l1_p22.rs` through a macro, which the ledger scan does not
//! see as one named test per address; here every address has its own named test, the same comparison.
//!
//! Each case lays a random map object record (all bytes random) at a free guest address, calls the getter with the record and an
//! index below the table length, and compares the returned word with the port's. Nothing is stubbed.
//!
//! Skips without a disc (KH2_ISO or local/iso.txt); fails without one when KH2_REQUIRE_DISC=1.

use kh2_ee::Ee;
use kh2_factory::fuzz::{Rng, STACK};
use kh2_map::mapobj::*;

const OBJ: u32 = 0x0190_0000;

fn game(what: &str) -> Option<Ee> {
    let elf = kh2_factory::exe_or_skip(what)?;
    let mut ee = Ee::new();
    ee.load_elf(&elf).expect("elf");
    Some(ee)
}

/// Lays a random record at `OBJ`, calls `addr(OBJ, index)` and compares the returned word with `port`.
fn getter(addr: u32, index_below: u64, port: impl Fn(&MapObject, usize) -> u32) {
    let Some(mut ee) = game(&format!("{addr:#x}")) else { return };
    let mut r = Rng::new(addr as u64);
    let mut distinct = std::collections::BTreeSet::new();
    for n in 0..400 {
        let bytes: Vec<u8> = (0..MAP_OBJECT_SIZE).map(|_| r.word() as u8).collect();
        ee.write(OBJ, &bytes);
        let idx = r.below(index_below.max(1)) as usize;
        let got = ee.call(addr, &[OBJ as u64, idx as u64], &[], STACK).unwrap_or_else(|f| panic!("the original faulted at {addr:#x}: {f:?}")) as u32;
        assert_eq!(got, port(&MapObject::from_bytes(&bytes), idx), "case {n}: index {idx}");
        distinct.insert(got);
    }
    assert!(distinct.len() > 100, "the getter must return varied words");
}

/// orig 0x1491a0: the model class word (`+0x24`).
#[test]
fn orig_1491a0_matches_the_game() {
    getter(0x1491a0, 1, |o, _| o.model_class());
}

/// orig 0x1491a8: the word at `+0x28`.
#[test]
fn orig_1491a8_matches_the_game() {
    getter(0x1491a8, 1, |o, _| o.texture_data());
}

/// orig 0x1491b0: the word at `+0x2c`.
#[test]
fn orig_1491b0_matches_the_game() {
    getter(0x1491b0, 1, |o, _| o.draw_tree());
}

/// orig 0x1491b8: the word at `+0x30`.
#[test]
fn orig_1491b8_matches_the_game() {
    getter(0x1491b8, 1, |o, _| o.collision_main());
}

/// orig 0x1491c0: the word at `+0x34`.
#[test]
fn orig_1491c0_matches_the_game() {
    getter(0x1491c0, 1, |o, _| o.collision_second());
}

/// orig 0x1491c8: the word at `+0x38`.
#[test]
fn orig_1491c8_matches_the_game() {
    getter(0x1491c8, 1, |o, _| o.collision_colour());
}

/// orig 0x1491d0: the word at `+0x3c`.
#[test]
fn orig_1491d0_matches_the_game() {
    getter(0x1491d0, 1, |o, _| o.collision_aux());
}

/// orig 0x1491d8: the word at `+0x40`.
#[test]
fn orig_1491d8_matches_the_game() {
    getter(0x1491d8, 1, |o, _| o.collision_kinds());
}

/// orig 0x1491e0: the property table (`+0x44`).
#[test]
fn orig_1491e0_matches_the_game() {
    getter(0x1491e0, 1, |o, _| o.prop_table());
}

/// orig 0x1491e8: the word at `+0x48`.
#[test]
fn orig_1491e8_matches_the_game() {
    getter(0x1491e8, 1, |o, _| o.draw_table());
}

/// orig 0x1491f0: the word at `+0x4c`.
#[test]
fn orig_1491f0_matches_the_game() {
    getter(0x1491f0, 1, |o, _| o.draw_table_b());
}

/// orig 0x1491f8: the word at `+0x50`.
#[test]
fn orig_1491f8_matches_the_game() {
    getter(0x1491f8, 1, |o, _| o.word_50());
}

/// orig 0x149200: entry `n` of the table at `+0x54` (indices 0..4).
#[test]
fn orig_149200_matches_the_game() {
    getter(0x149200, 4, |o, n| o.entry(n));
}

/// orig 0x149210: the place picture word (`+0x64`).
#[test]
fn orig_149210_matches_the_game() {
    getter(0x149210, 1, |o, _| o.place_picture());
}

/// orig 0x149218: the second place picture word (`+0x68`).
#[test]
fn orig_149218_matches_the_game() {
    getter(0x149218, 1, |o, _| o.place_picture_b());
}

/// orig 0x149220: the first word of map model `n`.
#[test]
fn orig_149220_matches_the_game() {
    getter(0x149220, MAP_MODELS as u64, |o, n| o.model_word(n));
}

/// orig 0x149238: the second word of map model `n`.
#[test]
fn orig_149238_matches_the_game() {
    getter(0x149238, MAP_MODELS as u64, |o, n| o.model_word_b(n));
}

/// orig 0x149250: the third word of map model `n`.
#[test]
fn orig_149250_matches_the_game() {
    getter(0x149250, MAP_MODELS as u64, |o, n| o.model_word_c(n));
}
