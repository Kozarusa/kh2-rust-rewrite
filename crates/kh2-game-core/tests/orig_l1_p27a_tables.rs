//! Data tables of the pad unit (WP-L1-P27a): the three logical-button tables the ledger keeps at `0x379768` (the field set, set 1),
//! `0x379ab8` (set 3) and `0x379b88` (set 5). They are data the port never copies: it reads the user's executable at run time and names
//! only the rows it uses (`kh2_game_core::pad::logical`). Each test below reads its table from the executable through kh2-ee's loaded image
//! and checks that every row the port names is a row of that table, that the port's table walk (`pad::apply_table`) gives the named logical
//! bit for the row's physical button alone, and that the set pointer table at `0x35b888` holds the table's address at the slot the pad code
//! selects it from. `button_tables.rs` in kh2-ard does the same from the crate that cannot see game-core; these tests name each table so the
//! ledger can tell which was compared.
//!
//! Skips without a disc; fails without one when `KH2_REQUIRE_DISC=1`.

use kh2_ee::Ee;
use kh2_game_core::pad::{self, logical};

const SET_POINTERS: u32 = 0x35b888;

fn machine(what: &str) -> Option<Ee> {
    let elf = kh2_factory::exe_or_skip(what)?;
    let mut ee = Ee::new();
    ee.load_elf(&elf).expect("elf");
    Some(ee)
}

/// The rows of the table at `at`: 16 bytes each (a 64-bit physical mask and the 64-bit logical bits), up to the zero mask.
fn table(ee: &mut Ee, at: u32) -> Vec<(u64, u64)> {
    let mut rows = Vec::new();
    let mut a = at;
    loop {
        let (mask, bits) = (ee.r64(a), ee.r64(a + 8));
        if mask == 0 {
            return rows;
        }
        rows.push((mask, bits));
        a += 16;
        assert!(rows.len() < 200, "table at {at:#x} has no end");
    }
}

fn check(name: &str, rows: &[(u64, u64)], port: &[logical::Row]) {
    assert!(!rows.is_empty(), "{name}: the table has rows");
    for r in port {
        assert!(rows.contains(&(r.physical, r.logical)), "{name}: the executable has no row {:#x} -> {:#x} ({})", r.physical, r.logical, r.name);
        assert_eq!(pad::apply_table(rows, r.physical) & r.logical, r.logical, "{name}: {} alone", r.name);
    }
}

/// The field set, table `0x379768` (set 1, chosen by the field start).
#[test]
fn orig_379768_matches_the_game() {
    let Some(mut ee) = machine("oracle test of 0x379768") else { return };
    assert_eq!(ee.r32(SET_POINTERS + 4), 0x379768, "set 1 is this table");
    let rows = table(&mut ee, 0x379768);
    check("field set", &rows, &logical::FIELD);
    // Up gives the shared direction bit and the field's own bit (the shared table `0x379698` applies whatever the set).
    let shared = table(&mut ee, 0x379698);
    let up = pad::apply_table(&shared, logical::PHYS_UP) | pad::apply_table(&rows, logical::PHYS_UP);
    assert_eq!(up & (logical::UP | 0x4000), logical::UP | 0x4000, "field Up: shared and set bits");
}

/// Set 3, table `0x379ab8`.
#[test]
fn orig_379ab8_matches_the_game() {
    let Some(mut ee) = machine("oracle test of 0x379ab8") else { return };
    assert_eq!(ee.r32(SET_POINTERS + 12), 0x379ab8, "set 3 is this table");
    let rows = table(&mut ee, 0x379ab8);
    check("set 3", &rows, &logical::SET3);
}

/// Set 5, table `0x379b88` (and its twin set 4, `0x379af8`, whose d-pad rows the port names the same).
#[test]
fn orig_379b88_matches_the_game() {
    let Some(mut ee) = machine("oracle test of 0x379b88") else { return };
    assert_eq!(ee.r32(SET_POINTERS + 20), 0x379b88, "set 5 is this table");
    assert_eq!(ee.r32(SET_POINTERS + 16), 0x379af8, "set 4 is its twin");
    let rows = table(&mut ee, 0x379b88);
    check("set 5", &rows, &logical::SET5_DPAD);
    let twin = table(&mut ee, 0x379af8);
    check("set 4", &twin, &logical::SET5_DPAD);
}
