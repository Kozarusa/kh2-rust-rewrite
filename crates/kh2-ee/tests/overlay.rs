//! Code overlays: the four overlays of the user's disc (`OVL.IDX`/`OVL.IMG`) load at 0x01F18000 and
//! their functions can be called. Skipped without a disc (`KH2_ISO`).

use kh2_ee::{Ee, OVERLAY_BASE};

const STACK: u32 = 0x01ff_0000;

fn disc() -> Option<(kh2_archive::Archive, Vec<u8>)> {
    let path = std::env::var("KH2_ISO").ok().filter(|p| std::path::Path::new(p).exists())?;
    let mut a = kh2_archive::Archive::open(path).ok()?;
    let boot = a.boot_executable().to_string();
    let elf = a.iso().read(&boot).ok()?;
    Some((a, elf))
}

/// Overlay 2 holds a pair of tiny accessors over a table in its own data area: the setter at
/// 0x01F18D30 stores `a1` at `table[a0]` (table at 0x01F2E538, words), the getter at 0x01F18D48
/// returns `table[a0]`. Overlay 3 starts with a function returning the address of one of its
/// tables (0x01F188A8). Each overlay replaces the previous one in the shared window.
#[test]
fn overlay_functions_run_after_loading_the_overlay_from_the_disc() {
    let Some((mut a, elf)) = disc() else {
        eprintln!("skipped: no disc image (set KH2_ISO)");
        return;
    };
    assert_eq!(a.overlays().len(), 4, "the USA disc has four code overlays");
    let mut ee = Ee::new();
    ee.load_elf(&elf).expect("elf");

    let ovl2 = a.read_overlay(2).expect("overlay 2");
    let end = ee.load_overlay(&ovl2).expect("fits in the window");
    assert_eq!(end, OVERLAY_BASE + ovl2.len() as u32);
    let (set, get, table) = (0x01F1_8D30u32, 0x01F1_8D48u32, 0x01F1_E538u32);
    for (idx, val) in [(0u64, 0x1234_5678u64), (5, 0xDEAD_BEEF), (9, 0)] {
        ee.call(set, &[idx, val], &[], STACK).expect("setter");
        assert_eq!(ee.r32(table + 4 * idx as u32), val as u32, "the setter wrote the table");
        let got = ee.call(get, &[idx], &[], STACK).expect("getter");
        assert_eq!(got as u32, val as u32, "the getter reads it back");
    }

    // Loading overlay 3 replaces the code in the window; its first function returns an address.
    let before = ee.r32(set);
    let ovl3 = a.read_overlay(3).expect("overlay 3");
    ee.load_overlay(&ovl3).expect("fits");
    assert_ne!(ee.r32(set), before, "the window now holds overlay 3");
    assert_eq!(ee.call(0x01F1_88A8, &[], &[], STACK).expect("overlay 3 function") as u32, 0x01F8_3020);
}

/// Every overlay fits the window and ends below the end of RAM.
#[test]
fn all_four_overlays_fit_the_window() {
    let Some((mut a, elf)) = disc() else {
        eprintln!("skipped: no disc image (set KH2_ISO)");
        return;
    };
    let mut ee = Ee::new();
    ee.load_elf(&elf).expect("elf");
    for k in 0..4 {
        let code = a.read_overlay(k).expect("overlay");
        let end = ee.load_overlay(&code).expect("fits");
        assert!(end <= 0x0200_0000, "overlay {k} ends at {end:#x}");
        assert!(end > OVERLAY_BASE);
    }
}
