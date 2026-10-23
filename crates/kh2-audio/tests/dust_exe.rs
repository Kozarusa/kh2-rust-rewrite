//! `kh2_audio::footstep::DustRules` reads the dust tables of `FUN_001b1c90` from the user's
//! executable (the byte table at 0x37b9b8 and the floats at 0x37b598, 0x37b59c, 0x37b9d8). The same
//! executable is loaded into the `kh2-ee` interpreter and its memory is compared word by word with
//! what the rules answer. Skipped without a disc (KH2_ISO or local/iso.txt).

use kh2_audio::footstep::DustRules;
use kh2_ee::Ee;

fn exe() -> Option<Vec<u8>> {
    let path = kh2_factory::disc_path("dust_exe")?;
    let mut a = kh2_archive::Archive::open(&path).ok()?;
    ["SLUS_210.05", "SLUS_210.05;1"].iter().find_map(|p| a.iso().read(p).ok())
}

#[test]
fn dust_rules_match_the_executable_memory() {
    let Some(elf) = exe() else {
        eprintln!("skipped: no disc");
        return;
    };
    let rules = DustRules::from_exe(&elf).expect("USA executable");
    let mut ee = Ee::new();
    ee.load_elf(&elf).expect("elf");
    // Every material the table covers: the byte the interpreter holds is the class.
    for m in 0..32u32 {
        let word = ee.r32(0x37b9b8 + (m & !3));
        let byte = (word >> ((m & 3) * 8)) as u8;
        assert_eq!(rules.class(m as u8), byte, "material {m}");
    }
    // Past the table the rules answer "no dust" (the game reads beyond it).
    assert_eq!(rules.class(32), 0);
    assert_eq!(rules.class(255), 0);
    assert_eq!(rules.probe_above, ee.f32_at(0x37b598));
    assert_eq!(rules.probe_below, ee.f32_at(0x37b59c));
    assert_eq!(rules.foot_reach, ee.f32_at(0x37b9d8));
    // Materials 1 and 3, the floors tt02 stands on, have a dust class.
    for m in [1u8, 3] {
        assert_ne!(rules.class(m), 0, "material {m} has dust");
    }
    // The masks of the stop step on material 1: its class bit and kind bit 8 + 0x20.
    let class = rules.class(1) as u32;
    assert_eq!(rules.mask(1, 8), Some(1u64 << class | 1u64 << 0x28));
}
