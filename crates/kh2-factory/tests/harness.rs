//! Self-tests of the fuzz harness on tiny hand-assembled MIPS functions (no disc needed): a matching port passes, a
//! wrong port fails, and the three audits fire: a result that never varies, a write past a block, and a write the
//! spec does not list.

use kh2_ee::Ee;
use kh2_factory::fuzz::*;

const AT: u32 = 0x0010_0000;
const JR_RA: u32 = 0x03e0_0008;

/// An interpreter with `body` as the delay slot of `jr ra` at [`AT`].
fn ee_with(body: u32) -> Ee {
    let mut ee = Ee::new();
    ee.w32(AT, JR_RA);
    ee.w32(AT + 4, body);
    ee
}

const ADD: Spec = Spec { addr: AT, args: &[Arg::Int(&[]), Arg::Int(&[])], floats: 0, ret: Ret::Int, globals: &[], stubs: &[], nulls: true };

#[test]
fn harness_accepts_a_matching_port() {
    // addu v0, a0, a1
    let mut ee = ee_with(0x0085_1021);
    let st = fuzz_with(&mut ee, &ADD, 2000, 1, |c| Out::int(c.int(0).wrapping_add(c.int(1)) as i64));
    assert_eq!(st.cases, 2000);
    assert!(st.steps >= 4000, "two instructions a case: {}", st.steps);
}

#[test]
#[should_panic(expected = "v0 game")]
fn harness_rejects_a_wrong_port() {
    let mut ee = ee_with(0x0085_1021);
    fuzz_with(&mut ee, &ADD, 2000, 1, |c| Out::int(c.int(0).wrapping_sub(c.int(1)) as i64));
}

#[test]
#[should_panic(expected = "the test checks nothing")]
fn harness_rejects_a_test_whose_result_never_varies() {
    // addiu v0, zero, 7
    let mut ee = ee_with(0x2402_0007);
    fuzz_with(&mut ee, &ADD, 100, 1, |_| Out::int(7));
}

const BLOCK: Spec = Spec {
    addr: AT,
    args: &[Arg::Ptr(Block { size: 0x10, fields: &[(0, 4, Kind::Int)], subs: &[] }), Arg::Int(&[])],
    floats: 0,
    ret: Ret::None,
    globals: &[],
    stubs: &[],
    nulls: true,
};

#[test]
fn harness_compares_blocks_byte_by_byte() {
    // sw a1, 4(a0)
    let mut ee = ee_with(0xac85_0004);
    fuzz_with(&mut ee, &BLOCK, 500, 2, |c| {
        let v = c.int(1);
        c.block(0)[4..8].copy_from_slice(&v.to_le_bytes());
        Out::none()
    });
}

#[test]
#[should_panic(expected = "wrote past the block")]
fn harness_catches_a_block_that_is_too_small() {
    // sw a1, 0x20(a0): past the 0x10-byte block, inside its guard zone.
    let mut ee = ee_with(0xac85_0020);
    fuzz_with(&mut ee, &BLOCK, 50, 2, |_| Out::none());
}

#[test]
#[should_panic(expected = "which the spec does not list")]
fn harness_catches_a_write_outside_the_spec() {
    // sw a1, 0x100(zero)
    let mut ee = ee_with(0xac05_0100);
    fuzz_with(&mut ee, &BLOCK, 50, 2, |_| Out::none());
}
