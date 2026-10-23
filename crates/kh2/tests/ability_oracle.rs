//! Oracle checks behind docs/systems/combat-effects/abilities.md "Ability readers" (WP-A1): Second Chance and Once More as
//! the game's own code applies them, run in kh2-ee on the user's executable and compared with the
//! rule the app ports (`kh2_app::abilities::survives_lethal_hit`, `kh2_app::abilities::lethal_hit_damage`,
//! called directly). Read-only; skips without a disc.
//!
//! `FUN_00185a68(actor, hit)` (the HP step of the receive handler) runs for real; it calls the
//! actor's vtable slot +0x58, which is Sora's `FUN_00176d78` (also real, with its callees
//! `FUN_001768a8`, `FUN_00185a08`, `FUN_00185be0`), and slot +0x78 (the HP method), replaced by a
//! stub that stores the signed amount it is handed: that amount is the result compared.

use kh2_ee::Ee;

const STACK: u32 = 0x01ff_0000;
const ACTOR: u32 = 0x0180_0000;
const STATUS: u32 = 0x0181_0000;
const HIT: u32 = 0x0182_0000;
const ATK: u32 = 0x0182_1000;
const VTABLE: u32 = 0x0183_0000;
const STUB: u32 = 0x0183_1000;
const OUT: u32 = 0x0184_0000;

fn open() -> Option<Ee> {
    let path = kh2_factory::disc_path("ability_oracle")?;
    let mut a = kh2_archive::Archive::open(&path).ok()?;
    let elf = ["SLUS_210.05", "SLUS_210.05;1"].iter().find_map(|p| a.iso().read(p).ok())?;
    let mut ee = Ee::new();
    ee.load_elf(&elf).expect("elf");
    // HP method stub: lui t0, 0x0184; sw a1, 0(t0); jr ra; nop.
    for (i, w) in [0x3c08_0184u32, 0xad05_0000, 0x03e0_0008, 0].iter().enumerate() {
        ee.w32(STUB + 4 * i as u32, *w);
    }
    ee.w32(VTABLE + 0x58, 0x0017_6d78);
    ee.w32(VTABLE + 0x78, STUB);
    Some(ee)
}

/// The app's own rule (`kh2_app::abilities`).
fn port(second_chance: bool, once_more: bool, hp: i32, min: i32, dmg: i32, motion: Option<usize>) -> i32 {
    kh2_app::abilities::lethal_hit_damage(hp, min, dmg, kh2_app::abilities::survives_lethal_hit(second_chance, once_more, hp, motion))
}

#[allow(clippy::too_many_arguments)]
fn game(ee: &mut Ee, second_chance: bool, once_more: bool, hp: i32, min: i32, dmg: i32, motion: Option<usize>) -> i32 {
    for base in [ACTOR, STATUS, HIT, ATK] {
        for o in (0..0xc00).step_by(4) {
            ee.w32(base + o, 0);
        }
    }
    ee.w32(ACTOR, VTABLE);
    ee.w32(ACTOR + 0x48c, STATUS);
    if let Some(m) = motion {
        ee.w32(ACTOR + 0x140, 1);
        ee.w32(ACTOR + 0x154, m as u32);
    }
    ee.w32(STATUS, hp as u32);
    ee.w32(STATUS + 4, 100);
    ee.w32(STATUS + 8, min as u32);
    // Block word 3 bit 31 (127) and word 4 bit 0 (128).
    ee.w32(STATUS + 0x1cc + 12, if second_chance { 0x8000_0000 } else { 0 });
    ee.w32(STATUS + 0x1cc + 16, u32::from(once_more));
    ee.w32(HIT + 0x20, ATK);
    ee.w32(HIT + 0x28, dmg as u32);
    ee.w32(OUT, 0xdead_beef);
    ee.call(0x185a68, &[ACTOR as u64, HIT as u64], &[], STACK).expect("FUN_00185a68");
    -(ee.r32(OUT) as i32)
}

#[test]
fn second_chance_and_once_more_match_the_game() {
    let Some(mut ee) = open() else { return };
    let mut n = 0;
    for sc in [false, true] {
        for om in [false, true] {
            for hp in [1, 2, 3, 7, 20] {
                for min in [0, 1] {
                    for dmg in [1, 2, 5, 19, 40] {
                        for motion in [None, Some(0), Some(9), Some(10), Some(25), Some(39), Some(40)] {
                            let g = game(&mut ee, sc, om, hp, min, dmg, motion);
                            let p = port(sc, om, hp, min, dmg, motion);
                            assert_eq!(g, p, "second chance {sc} once more {om} hp {hp} min {min} dmg {dmg} motion {motion:?}");
                            n += 1;
                        }
                    }
                }
            }
        }
    }
    eprintln!("{n} cases equal");
}

/// The app's own Scan gate (`kh2_app::abilities::scan_binds_gauge`).
fn scan_port(player_status: bool, scan: bool, listed: bool, flags: u32, target_status: bool) -> bool {
    kh2_app::abilities::scan_binds_gauge(player_status, scan, listed, flags, target_status)
}

/// Scan (WP-A2): `FUN_0019b4a8(marker)` runs for real with its callees `FUN_0019b548`, `FUN_001768a8`,
/// `FUN_0019a3f8`, `FUN_001b80e8` and `FUN_0019a440`; only the gauge bind `FUN_00154688` is replaced
/// by a stub that returns 0x1234 (it builds the HUD object and does not decide anything). The result
/// compared is whether the marker's +0x20 was set.
#[test]
fn scan_gate_matches_the_game() {
    let Some(mut ee) = open() else { return };
    const MARKER: u32 = 0x0185_0000;
    const PLAYER: u32 = 0x0186_0000;
    const PSTATUS: u32 = 0x0187_0000;
    const TARGET: u32 = 0x0188_0000;
    const TSTATUS: u32 = 0x0189_0000;
    const OTHER: u32 = 0x018a_0000;
    const LIST: u32 = 0x01d5_d7a0;
    // lui v0, 0; ori v0, v0, 0x1234; jr ra; nop.
    for (i, w) in [0x3c02_0000u32, 0x3442_1234, 0x03e0_0008, 0].iter().enumerate() {
        ee.w32(0x0015_4688 + 4 * i as u32, *w);
    }
    let mut n = 0;
    for ps in [false, true] {
        for scan in [false, true] {
            for listed in [false, true] {
                for flags in [0u32, 3, 0x4000, 0x8_0000, 0x1000_0000] {
                    for ts in [false, true] {
                        for base in [MARKER, PLAYER, PSTATUS, TARGET, TSTATUS, OTHER] {
                            for o in (0..0x900).step_by(4) {
                                ee.w32(base + o, 0);
                            }
                        }
                        ee.w32(MARKER + 4, TARGET);
                        ee.w32(MARKER + 0x28, PLAYER);
                        ee.w32(MARKER + 0x24, 1);
                        ee.w32(PLAYER + 0x48c, if ps { PSTATUS } else { 0 });
                        ee.w32(PSTATUS + 0x1cc, if scan { 0x8_0000 } else { 0 });
                        // The actor list: another actor first, then the target when listed.
                        ee.w32(LIST, OTHER);
                        ee.w32(OTHER + 0x8b4, if listed { TARGET } else { 0 });
                        ee.w32(TARGET + 0x108, flags);
                        ee.w32(TARGET + 0x48c, if ts { TSTATUS } else { 0 });
                        ee.call(0x19b4a8, &[MARKER as u64], &[], STACK).expect("FUN_0019b4a8");
                        let g = ee.r32(MARKER + 0x20) == 0x1234;
                        assert_eq!(g, scan_port(ps, scan, listed, flags, ts), "player status {ps} scan {scan} listed {listed} flags {flags:#x} target status {ts}");
                        n += 1;
                    }
                }
            }
        }
    }
    eprintln!("{n} Scan cases equal");
}
