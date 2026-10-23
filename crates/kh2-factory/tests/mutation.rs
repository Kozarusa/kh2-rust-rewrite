//! A mutated port of 0x239468 (`<=` for `<` between its two floats) must fail the same oracle spec the real port
//! passes: the fuzz inputs reach exactly-equal floats. Needs the disc (skips without it, fails in gate mode).

use kh2_factory::fuzz::*;
use kh2_factory::layout::Bytes;

const SPEC: Spec = Spec {
    addr: 0x239468,
    args: &[Arg::Ptr(Block { size: 0x60, fields: &[(0x38, 4, Kind::Int), (0x40, 4, Kind::Float), (0x44, 4, Kind::Float), (0x50, 1, Kind::Int)], subs: &[] })],
    floats: 0,
    ret: Ret::Int,
    globals: &[],
    stubs: &[],
    nulls: true,
};

fn mutated(rec: &[u8]) -> bool {
    let s = rec.u32_at(0x38);
    s.wrapping_sub(1) < 2 && (rec.u8_at(0x50) != 0 || rec.f32_at(0x40) <= rec.f32_at(0x44))
}

#[test]
fn mutating_239468_to_less_equal_is_caught() {
    if kh2_factory::exe_or_skip("mutation test of 0x239468").is_none() {
        return;
    }
    let r = std::panic::catch_unwind(|| fuzz(&SPEC, 4000, |c| Out::int(mutated(c.block(0)) as i64)));
    assert!(r.is_err(), "the <= mutation passed the oracle test");
}
