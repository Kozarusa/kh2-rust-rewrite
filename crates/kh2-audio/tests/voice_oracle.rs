//! Runs the game's own voice-line pick (`FUN_001ae538` at 0x1ae538, with its callees
//! `FUN_001ae6a0`, `FUN_001ae440`, `FUN_001802e0` and the 64-bit divide `FUN_00325b38`) in the
//! `kh2-ee` interpreter on the user's executable and compares every result and every generator
//! state with `kh2_audio::voice::pick_line`. Skipped without a disc (KH2_ISO or local/iso.txt).

use kh2_audio::voice::{other_party_line, pick_line, resolve_line};
use kh2_ee::Ee;

const RNG: u32 = 0x3716a0;
const RECORD: u32 = 0x01f0_0000;
const STACK: u32 = 0x01ff_0000;
const PICK: u32 = 0x1ae538;

fn exe() -> Option<Vec<u8>> {
    let path = kh2_factory::disc_path("voice_oracle")?;
    let mut a = kh2_archive::Archive::open(&path).ok()?;
    ["SLUS_210.05", "SLUS_210.05;1"].iter().find_map(|p| a.iso().read(p).ok())
}

/// The game's pick for a record of `pairs` with the generator at `seed`: (line, new seed).
fn oracle(ee: &mut Ee, pairs: &[i8; 10], speaker: u16, other: u16, seed: u32) -> (i32, u32) {
    let mut rec = [0u8; 0xe];
    rec[0] = speaker as u8;
    for (i, p) in pairs.iter().enumerate() {
        rec[4 + i] = *p as u8;
    }
    ee.write(RECORD, &rec);
    ee.w32(RNG, seed);
    let v = ee.call(PICK, &[RECORD as u64, speaker as u64, other as u64], &[], STACK).expect("call");
    (v as i32, ee.r32(RNG))
}

#[test]
fn pick_line_matches_the_game_for_every_state_bit_pattern() {
    let Some(elf) = exe() else {
        eprintln!("skipped: no disc");
        return;
    };
    let mut ee = Ee::new();
    ee.load_elf(&elf).expect("elf");
    // No story flags set (the save area at 0x33f6b0 is zero): Auron is refused.
    let flags = |_: u16| false;
    let records: [([i8; 10], u16, u16); 7] = [
        ([7, 30, 1, 0, 2, 0, 3, 0, -1, 0], 1, 0),
        ([9, 50, 4, 0, 5, 0, -1, 0, -1, 0], 1, 0),
        ([0, 0, -1, 0, -1, 0, -1, 0, -1, 0], 1, 0),
        ([12, 10, 13, 20, 14, 30, 15, 0, 16, 0], 1, 2),
        ([-3, 0, 20, 0, -1, 0, -1, 0, -1, 0], 1, 3),
        ([-3, 40, 21, 0, 22, 0, 23, 0, 24, 0], 2, 15),
        ([-3, 0, -3, 0, -1, 0, -1, 0, -1, 0], 1, 5),
    ];
    // Seeds across the whole range, including every top-bit pattern (the divide is 64-bit and the
    // state is loaded as a 32-bit word).
    let mut seeds: Vec<u32> = (0..512u32).map(|i| i.wrapping_mul(0x0083_1f27).wrapping_add(0x1234_5678)).collect();
    seeds.extend([1, 0x7fff_ffff, 0x8000_0000, 0xffff_ffff, 0x2545_f491, 0x8000_0001]);
    let mut checked = 0;
    for (pairs, speaker, other) in &records {
        for &s in &seeds {
            let (want, want_seed) = oracle(&mut ee, pairs, *speaker, *other, s);
            let mut seed = s;
            let got = pick_line(pairs, &mut seed, |l| resolve_line(l, *speaker, *other, &flags));
            assert_eq!((got, seed), (want, want_seed), "pairs {pairs:?} speaker {speaker} other {other} seed {s:#x}");
            checked += 1;
        }
    }
    assert!(checked > 3000);
}

#[test]
fn other_party_line_matches_the_game() {
    let Some(elf) = exe() else {
        eprintln!("skipped: no disc");
        return;
    };
    let mut ee = Ee::new();
    ee.load_elf(&elf).expect("elf");
    let flags = |_: u16| false;
    for speaker in 0..18u16 {
        for other in 0..18u16 {
            let v = ee.call(0x1ae440, &[speaker as u64, other as u64], &[], STACK).expect("call") as u32 as i32;
            assert_eq!(other_party_line(speaker, other, &flags), v, "speaker {speaker} other {other}");
        }
    }
}
