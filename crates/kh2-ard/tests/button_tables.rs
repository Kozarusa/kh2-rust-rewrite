//! The logical-button data tables of the pad unit, checked against the port's pad code (WP-L1-P20; the ledger keeps the three tables
//! `0x379768`, `0x379ab8`, `0x379b88` as `keep read`: they are data the port reads from the user's executable at run time, never
//! copies).
//!
//! What the tables are: the pad task (`0x167038`, priority 110000) turns each pad's physical button word (the active-low read of the
//! pad, flipped so a pressed button is a 1 bit, plus synthetic bits the pad read adds: 0x01000000 and 0x02000000 for the confirm
//! and cancel choice, 0x04000000 / 0x08000000 for the right-stick camera setting, and the stick directions as bits
//! 0x00100000..0x00800000 and 0x00010000..0x00080000) into a 64-bit logical word (`FUN_001676a0`). A table is a list of 16-byte
//! rows ending at a zero mask: a 64-bit physical mask and the 64-bit logical bits it gives when every bit of the mask is pressed
//! (`FUN_00167660`). One table, `0x379698`, always applies; a second, the active button set, is picked by number (`FUN_00167908`,
//! the six pointers at `0x35b888`: set 0 none, set 1 the field `0x379768` chosen by the field start `0x102a60`, set 2 `0x379988`,
//! set 3 `0x379ab8`, set 4 `0x379af8`, set 5 `0x379b88`) and ORs its bits in.
//!
//! The test reads the tables from the executable (`KH2_ISO`, or `KH2_EXE`; skipped without, failed when `KH2_REQUIRE_DISC=1`) and
//! checks them against `kh2_game_core::pad`, which is included here by path (this crate cannot depend on kh2-game-core, and
//! the file uses only `std` for that reason): the port's constants must be rows of the executable's tables, the sets must be
//! where the pad code expects them, and applying the executable's table to a physical word must give the logical bits the
//! port's constants name.

#[allow(dead_code)]
#[path = "../../kh2-game-core/src/pad.rs"]
mod pad;

use kh2_ee::Ee;
use pad::logical::{self, Row};

const SHARED: u32 = 0x379698;
const SETS: [u32; 5] = [0x379768, 0x379988, 0x379ab8, 0x379af8, 0x379b88];
const SET_POINTERS: u32 = 0x35b888;

fn machine() -> Option<Ee> {
    let elf = kh2_factory::exe_or_skip("button tables")?;
    let mut ee = Ee::new();
    ee.load_elf(&elf).expect("elf");
    Some(ee)
}

/// The rows of the table at `at`: up to the zero mask.
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

fn has(rows: &[(u64, u64)], physical: u64, logical: u64) -> bool {
    rows.contains(&(physical, logical))
}

fn check_rows(name: &str, rows: &[(u64, u64)], port: &[Row]) {
    for r in port {
        assert!(has(rows, r.physical, r.logical), "{name}: the executable has no row {:#x} -> {:#x} ({})", r.physical, r.logical, r.name);
        // Pressing that button alone gives its logical bit.
        assert_eq!(pad::apply_table(rows, r.physical) & r.logical, r.logical, "{name}: {} alone", r.name);
    }
}

#[test]
fn the_pad_codes_button_rows_are_in_the_executables_tables() {
    let Some(mut ee) = machine() else { return };
    let shared = table(&mut ee, SHARED);
    let sets: Vec<Vec<(u64, u64)>> = SETS.iter().map(|&a| table(&mut ee, a)).collect();

    // The shared table: the reset combination, Start, the synthetic confirm and cancel bits, and the d-pad with its stick twin.
    assert!(has(&shared, logical::PHYS_RESET, logical::RESET), "reset combination");
    assert!(has(&shared, logical::SYNTH_CONFIRM, logical::CONFIRM) && has(&shared, logical::SYNTH_CANCEL, logical::CANCEL), "confirm and cancel");
    for s in logical::SHARED.iter() {
        assert!(has(&shared, s.physical, s.logical), "shared: {:#x} -> {:#x}", s.physical, s.logical);
        if s.stick != 0 {
            assert!(has(&shared, s.stick, s.logical), "shared: stick {:#x} -> {:#x}", s.stick, s.logical);
        }
    }
    assert_eq!(shared.len(), 12, "the shared table has 12 rows");

    // The sets: field (1), set 3, set 5 (and its twin 4 for the d-pad rows).
    check_rows("field set", &sets[0], &logical::FIELD);
    check_rows("set 3", &sets[2], &logical::SET3);
    check_rows("set 5", &sets[4], &logical::SET5_DPAD);
    check_rows("set 4", &sets[3], &logical::SET5_DPAD);

    // The field pad: Up pressed gives the shared direction bit and the field's own bit.
    let up = pad::apply_table(&shared, logical::PHYS_UP) | pad::apply_table(&sets[0], logical::PHYS_UP);
    assert_eq!(up & (logical::UP | 0x4000), logical::UP | 0x4000, "field Up: shared and set bits");
    // Start gives its logical bit through the shared table, which applies whatever the set.
    assert_eq!(pad::apply_table(&shared, logical::PHYS_START), logical::START);
    // The reset combination is a mask of six buttons: five of them are not enough.
    assert_eq!(pad::apply_table(&shared, logical::PHYS_RESET) & logical::RESET, logical::RESET);
    assert_eq!(pad::apply_table(&shared, logical::PHYS_RESET & !logical::PHYS_L1) & logical::RESET, 0);

    // The six pointers the set selection reads: none for set 0, the tables in order after it.
    let sel = pad::PadUnit::new(core::array::from_fn(|i| ee.r32(SET_POINTERS + 4 * i as u32)));
    assert_eq!(sel.sets[0], 0, "set 0 has no table");
    assert_eq!(&sel.sets[1..], &SETS[..], "sets 1 to 5 are the five tables, in address order");
}

#[test]
fn the_pad_unit_selects_the_sets_the_tables_belong_to() {
    let Some(mut ee) = machine() else { return };
    let sets: [u32; 6] = core::array::from_fn(|i| ee.r32(SET_POINTERS + 4 * i as u32));
    let mut unit = pad::PadUnit::new(sets);
    for n in 0..6u32 {
        unit.select_set(n);
        assert_eq!(unit.active, sets[n as usize]);
        assert_eq!(unit.active_number, n);
    }
    // The stick constants the start stores come from three floats at fixed addresses of the executable.
    let c: [f32; 3] = core::array::from_fn(|i| ee.f32_at(pad::STICK_CONSTANTS[i]));
    assert!(c.iter().all(|x| x.is_finite() && *x > 0.0), "stick and repeat constants {c:?}");
    unit.set_scale(c[0], c[1], c[2]);
    assert_eq!(unit.scale, c);
}
