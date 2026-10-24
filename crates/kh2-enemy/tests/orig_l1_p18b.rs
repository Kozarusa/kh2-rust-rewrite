//! Oracle tests of the object bit-array functions behind the script host calls 1.52, 1.53 and 1.54, run in kh2-ee on the
//! user's executable: `bit_array_set` (orig 0x19dd88), `bit_array_set_second` (0x19ddb0) and `bit_array_clear_both` (0x19dde0).
//! Each case lays 16 random words at an address, calls the original with that address and a bit number, and compares all 64
//! bytes with the port's result on a copy of the same bytes (the port works on a byte slice at a base offset). No callee
//! is stubbed (the three functions are leaves). Skips without a disc (fails with `KH2_REQUIRE_DISC=1`).

use kh2_ee::Ee;
use kh2_enemy::enemy_host::{bit_array_clear_both, bit_array_set, bit_array_set_second};
use kh2_factory::fuzz::Rng;

const STACK: u32 = 0x01ff_0000;
const BLOCK: u32 = 0x0180_0000;
/// The port's base offset inside its slice (the guest's base address is BLOCK + OFFSET).
const OFFSET: usize = 0x10;

fn run(addr: u32, seed: u64, port: impl Fn(&mut [u8], usize, u32)) {
    let Some(elf) = kh2_factory::exe_or_skip("oracle test of the bit-array functions") else { return };
    let mut ee = Ee::new();
    ee.load_elf(&elf).expect("elf");
    let mut rng = Rng::new(seed);
    let mut changed = 0;
    for case in 0..3000 {
        let bytes: Vec<u8> = (0..64).map(|_| rng.word() as u8).collect();
        // Bit numbers over the words the 64 bytes hold after the base, mostly in the first two words.
        let n = match rng.below(3) {
            0 => rng.below(64) as u32,
            1 => rng.below(320) as u32,
            _ => rng.word() as u32 % 416,
        };
        ee.write(BLOCK, &bytes);
        ee.call(addr, &[(BLOCK + OFFSET as u32) as u64, n as u64], &[], STACK).unwrap_or_else(|f| panic!("case {case}: {f:?}"));
        let game: Vec<u8> = (0..64).map(|i| ee.r8(BLOCK + i)).collect();
        let mut mine = bytes.clone();
        port(&mut mine, OFFSET, n);
        assert_eq!(game, mine, "case {case}: bit {n}");
        changed += (game != bytes) as u32;
    }
    assert!(changed > 1000, "cases that changed a bit: {changed}");
}

#[test]
fn orig_19dd88_matches_the_game() {
    run(0x19dd88, 0x19dd88, bit_array_set);
}

#[test]
fn orig_19ddb0_matches_the_game() {
    run(0x19ddb0, 0x19ddb0, bit_array_set_second);
}

#[test]
fn orig_19dde0_matches_the_game() {
    run(0x19dde0, 0x19dde0, bit_array_clear_both);
}
