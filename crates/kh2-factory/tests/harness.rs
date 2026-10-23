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

#[test]
#[should_panic(expected = "block 0 byte")]
fn harness_rejects_a_wrong_block_value() {
    // sw a1, 4(a0); the port stores one more.
    let mut ee = ee_with(0xac85_0004);
    fuzz_with(&mut ee, &BLOCK, 500, 2, |c| {
        let v = c.int(1).wrapping_add(1);
        c.block(0)[4..8].copy_from_slice(&v.to_le_bytes());
        Out::none()
    });
}

const FLOATS: Spec = Spec {
    addr: AT,
    args: &[Arg::Ptr(Block { size: 8, fields: &[(0, 4, Kind::Float), (4, 4, Kind::Float)], subs: &[] })],
    floats: 0,
    ret: Ret::Int,
    globals: &[],
    stubs: &[],
    nulls: false,
};

/// Returns 1 when the float at +0 of a0 is less than the float at +4, else 0.
fn ee_float_less() -> Ee {
    let mut ee = Ee::new();
    let code = [
        0xc480_0000, // lwc1 f0, 0(a0)
        0xc481_0004, // lwc1 f1, 4(a0)
        0x4601_0034, // c.lt.s f0, f1
        0x4501_0003, // bc1t +3 (to the last pair)
        0,           // nop
        JR_RA,
        0x0000_1021, // addu v0, zero, zero
        JR_RA,
        0x2402_0001, // addiu v0, zero, 1
    ];
    for (i, w) in code.iter().enumerate() {
        ee.w32(AT + 4 * i as u32, *w);
    }
    ee
}

fn lt(c: &Case, strict: bool) -> Out {
    let f = |o: usize| f32::from_le_bytes(c.blocks[0][o..o + 4].try_into().unwrap());
    Out::int(if strict { f(0) < f(4) } else { f(0) <= f(4) } as i64)
}

#[test]
fn harness_accepts_a_matching_float_compare() {
    let mut ee = ee_float_less();
    // fuzz_with panics on the first difference, so a returned Stats means all 2000 cases agreed.
    let stats = fuzz_with(&mut ee, &FLOATS, 2000, 3, |c| lt(c, true));
    assert_eq!(stats.cases, 2000, "every generated case ran and matched");
    assert!(stats.steps > 0, "the original code actually executed");
}

#[test]
#[should_panic(expected = "v0 game")]
fn harness_catches_less_equal_for_less_through_exactly_equal_floats() {
    let mut ee = ee_float_less();
    fuzz_with(&mut ee, &FLOATS, 2000, 3, |c| lt(c, false));
}

#[test]
fn float_edge_values_reach_the_inputs() {
    let mut rng = Rng::new(9);
    let mut pool = Vec::new();
    let v: Vec<f32> = (0..4000).map(|_| rng.float_pooled(&mut pool)).collect();
    assert!(v.iter().all(|f| f.is_finite()), "the EE has no infinities or NaN");
    assert!(v.iter().any(|f| f.to_bits() == (-0.0f32).to_bits()) && v.iter().any(|f| *f == 1.0) && v.iter().any(|f| *f == -1.0));
}
