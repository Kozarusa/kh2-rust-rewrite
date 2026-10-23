//! Oracle checks behind `docs/systems/drive-limits-summons/contract.md`: the summon gauge rules as the doc states them, compared
//! with the game's own leaf routines run in kh2-ee on the user's executable. Read-only; skips with a
//! note when no disc image is configured (KH2_ISO or local/iso.txt).
//!
//! - FUN_001a0480 (gauge mode set): mode byte, segment count and the timer for a summon level.
//! - FUN_001a0420 (whole segments left in the timer).
//! - FUN_00155bf0 (the HUD's segment digit and the fill of the current segment in modes 2 and 3).
//! - FUN_001c65f0 (whether the summon's arrival plays as an event: the config option, the per-summon
//!   "seen" bit and the player's +0x810 word).

use kh2_ee::Ee;

const STACK: u32 = 0x01ff_0000;
/// Pointer to the `sstm` pref record (FUN_001a0480 and FUN_001a0420 read `[DAT_00360130] + 0x68`).
const SSTM_PTR: u32 = 0x0036_0130;
const SSTM: u32 = 0x0180_0000;
const STATUS: u32 = 0x0181_0000;
const OBJECT: u32 = 0x0182_0000;
const PANEL: u32 = 0x0183_0000;
const OUT: u32 = 0x0184_0000;
/// The config block FUN_001057a8 returns; FUN_0023eed8 reads bits 9..10 of its word +0x31b0.
const CONFIG: u32 = 0x0033_e860;
/// Per-summon "arrival seen" bits (FUN_001c65f0 sets them, FUN_001c6930 / FUN_001c6bb0 clear them).
const SEEN: u32 = 0x01d5_e370;

fn open() -> Option<(Ee, Vec<u32>)> {
    let path = kh2_factory::disc_path("summon_oracle")?;
    let mut a = kh2_archive::Archive::open(&path).ok()?;
    let elf = ["SLUS_210.05", "SLUS_210.05;1"].iter().find_map(|p| a.iso().read(p).ok())?;
    let sstm = kh2_tables::player::load_sstm(&mut a).ok()?.words.clone();
    let mut ee = Ee::new();
    ee.load_elf(&elf).expect("elf");
    for (i, w) in sstm.iter().enumerate() {
        ee.w32(SSTM + 4 * i as u32, *w);
    }
    ee.w32(SSTM_PTR, SSTM);
    Some((ee, sstm))
}

fn clear(ee: &mut Ee, at: u32, len: u32) {
    for o in (0..len).step_by(4) {
        ee.w32(at + o, 0);
    }
}

/// The doc's rule for FUN_001a0480: mode byte +0x1af, segments +0x1b3 = level + 3, timer +0x1b4 and
/// its maximum +0x1b8 = sstm[0x68] * (level + 3).
fn mode_set(unit: f32, level: i32) -> (u8, u8, f32) {
    (3, (level + 3) as u8, unit * (level + 3) as f32)
}

/// The doc's rule for FUN_001a0420: whole segments in the timer, one fewer when the timer is full.
fn whole_segments(unit: f32, t: f32, max: f32) -> i32 {
    let n = (t / unit) as i32;
    if t == max { n - 1 } else { n }
}

/// The doc's rule for FUN_00155bf0: (digit, fill, segment size).
fn hud_segments(t: f32, max: f32, count: u8) -> (u32, i32, i32) {
    let seg = (max / count as f32) as i32;
    if seg == 0 {
        return (0, 0, 0);
    }
    let digit = (t as i32 / seg) as u32;
    if digit == count as u32 { (digit - 1, seg, seg) } else { (digit, t as i32 % seg, seg) }
}

#[test]
fn summon_gauge_routines_match_the_doc() {
    let Some((mut ee, sstm)) = open() else { return };
    let unit = f32::from_bits(sstm[0x68 / 4]);
    for level in 0..=6 {
        clear(&mut ee, STATUS, 0x200);
        ee.call(0x1a0480, &[STATUS as u64, 3, level as u64], &[], STACK).expect("FUN_001a0480");
        let got = (ee.r8(STATUS + 0x1af), ee.r8(STATUS + 0x1b3), ee.f32_at(STATUS + 0x1b4));
        assert_eq!(got, mode_set(unit, level), "level {level}");
        assert_eq!(ee.f32_at(STATUS + 0x1b8), got.2);
        let max = got.2;
        // Whole segments for a sweep of timer values, including full, exact boundaries and empty.
        let mut t = max;
        while t >= 0.0 {
            ee.wf32(STATUS + 0x1b4, t);
            let n = ee.call(0x1a0420, &[STATUS as u64], &[], STACK).expect("FUN_001a0420") as i32;
            assert_eq!(n, whole_segments(unit, t, max), "level {level} t {t}");
            // The HUD reads the same block through panel +0x34 -> object +0x48c.
            ee.w32(PANEL + 0x34, OBJECT);
            ee.w32(OBJECT + 0x48c, STATUS);
            let d = ee.call(0x155bf0, &[PANEL as u64, OUT as u64, (OUT + 4) as u64], &[], STACK).expect("FUN_00155bf0") as u32;
            let (fill, seg) = (ee.r32(OUT) as i32, ee.r32(OUT + 4) as i32);
            assert_eq!((d, fill, seg), hud_segments(t, max, (level + 3) as u8), "hud level {level} t {t}");
            t -= unit / 4.0 + 1.5;
        }
    }
    println!("FUN_001a0480, FUN_001a0420, FUN_00155bf0 agree with the doc for summon levels 0..=6 (segment {unit} units)");
}

/// The doc's rule for FUN_001c65f0: `None` = no event, else whether the seen bit gets set.
fn arrival_is_event(option: u32, seen: bool, player_810: u32) -> bool {
    if player_810 != 0 {
        return false;
    }
    match option {
        1 => true,
        2 => !seen,
        _ => false,
    }
}

#[test]
fn summon_arrival_event_choice_matches_the_doc() {
    let Some((mut ee, _)) = open() else { return };
    let base = ee.r32(CONFIG + 0x31b0) & !(3 << 9);
    for option in 0..4u32 {
        for seen in [false, true] {
            for p810 in [0u32, 1] {
                for index in [1u32, 2, 3, 4] {
                    ee.w32(CONFIG + 0x31b0, base | option << 9);
                    ee.w32(SEEN, if seen { 1 << index } else { 0 });
                    clear(&mut ee, OBJECT, 0x900);
                    ee.w32(OBJECT + 0x810, p810);
                    let got = ee.call(0x1c65f0, &[index as u64, OBJECT as u64], &[], STACK).expect("FUN_001c65f0") != 0;
                    let want = arrival_is_event(option, seen, p810);
                    assert_eq!(got, want, "option {option} seen {seen} +0x810 {p810} summon {index}");
                    // A played event marks the summon as seen.
                    let bit = ee.r32(SEEN) & (1 << index) != 0;
                    assert_eq!(bit, seen || got, "seen bit after option {option} seen {seen} summon {index}");
                }
            }
        }
    }
    println!("FUN_001c65f0 agrees with the doc for every option, seen bit, +0x810 and summon index");
}
