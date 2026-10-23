//! Oracle fuzz test of `kh2_ard::kinds::fill_request` (orig 0x1ccb10, group kind 7) against the original in kh2-ee.
//! The request object is a 0x40-byte block of random bytes; the pointer global `DAT_003601d0` is pointed at it before the run
//! (the harness lays block 0 at its arena base), and a fourth, unused argument carries that block so the harness allocates it.
//! Skips without a disc (KH2_ISO or local/iso.txt); fails without one when KH2_REQUIRE_DISC=1.

use kh2_ee::Ee;
use kh2_factory::fuzz::*;

const OBJ: Block = Block { size: 0x40, fields: &[(4, 4, Kind::Int), (8, 4, Kind::Int), (0x34, 4, Kind::Int), (0x38, 4, Kind::Int)], subs: &[] };

const SPEC: Spec = Spec {
    addr: 0x1ccb10,
    args: &[Arg::Int(&[0, 1, 0x100, 0x1_00]), Arg::Int(&[]), Arg::Int(&[0, 1, 0x100, 0xff]), Arg::Ptr(OBJ)],
    floats: 0,
    ret: Ret::None,
    globals: &[(0x3601d0, 4, false)],
    stubs: &[],
    nulls: false,
};

#[test]
fn orig_1ccb10_matches_the_game() {
    let Some(elf) = kh2_factory::exe_or_skip("oracle test of 0x1ccb10") else { return };
    let mut ee = Ee::new();
    ee.load_elf(&elf).expect("elf");
    ee.w32(0x3601d0, block_addr(0));
    let st = fuzz_with(&mut ee, &SPEC, 2000, 0x1ccb10, |c| {
        let (player, value, flag) = (c.int(0) as u32, c.int(1) as u32, c.int(2) as u8);
        kh2_ard::kinds::fill_request(Some(c.block(0)), player, value, flag);
        Out::none()
    });
    eprintln!("{} cases equal", st.cases);
}
