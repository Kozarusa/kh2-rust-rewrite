//! Oracle fuzz tests for the pure leaves between 0x236a81 and 0x2f7ed8 (work package L1-P11c; specs from `factory brief`, tests
//! written in the `factory gen` shape). The ports are in `kh2_game_core::leaves_c`. Globals a leaf touches are listed in the spec,
//! randomised before every case and compared after it (the two constant leaves list an unrelated global, so the harness sees varying inputs and checks the leaf leaves it alone). Callees are not stubbed (0x236c00 calls the SDK float-to-int leaf 0x326940).
//! Skips without a disc (KH2_ISO or local/iso.txt); fails without one when KH2_REQUIRE_DISC=1.

use kh2_factory::fuzz::*;
use kh2_game_core::leaves_c as l;

macro_rules! oracle {
    ($name:ident, $spec:expr, |$c:ident| $body:expr) => {
        #[test]
        fn $name() {
            let spec: Spec = $spec;
            let Some(st) = fuzz(&spec, 4000, |$c| $body) else { return };
            eprintln!("{} cases equal, {} interpreter steps in {:.3} s", st.cases, st.steps, st.secs);
        }
    };
}

const OBJ: Block = Block { size: 0x140, fields: &[(0xa9, 1, Kind::Int), (0xaa, 1, Kind::Int), (0xcc, 4, Kind::Int), (0x114, 4, Kind::Float), (0x118, 4, Kind::Float), (0x11c, 4, Kind::Float), (0x120, 4, Kind::Float), (0x130, 4, Kind::Float), (0x134, 4, Kind::Float), (0x138, 4, Kind::Float)], subs: &[] };

const fn spec(addr: u32, args: &'static [Arg], floats: u8, ret: Ret, globals: &'static [(u32, u32, bool)]) -> Spec {
    Spec { addr, args, floats, ret, globals, stubs: &[], nulls: true }
}

oracle!(orig_236ae0_matches_the_game, spec(0x236ae0, &[Arg::Ptr(OBJ)], 3, Ret::None, &[]), |c| {
    let (x, y, z) = (c.float(0), c.float(1), c.float(2));
    l::set_vec3(c.block(0), x, y, z);
    Out::none()
});
oracle!(orig_236b28_matches_the_game, spec(0x236b28, &[Arg::Ptr(OBJ)], 1, Ret::None, &[]), |c| {
    let x = c.float(0);
    l::set_vec_x(c.block(0), x);
    Out::none()
});
oracle!(orig_236b70_matches_the_game, spec(0x236b70, &[Arg::Ptr(OBJ)], 1, Ret::None, &[]), |c| {
    let x = c.float(0);
    l::set_vec_y(c.block(0), x);
    Out::none()
});
oracle!(orig_236bb8_matches_the_game, spec(0x236bb8, &[Arg::Ptr(OBJ)], 1, Ret::None, &[]), |c| {
    let x = c.float(0);
    l::set_vec_z(c.block(0), x);
    Out::none()
});
oracle!(orig_236c00_matches_the_game, spec(0x236c00, &[Arg::Ptr(OBJ)], 4, Ret::None, &[]), |c| {
    let (a, r, g, b) = (c.float(0), c.float(1), c.float(2), c.float(3));
    l::set_color(c.block(0), a, r, g, b);
    Out::none()
});
oracle!(orig_23b880_matches_the_game, spec(0x23b880, &[], 0, Ret::None, &[(0x36ca70, 4, true)]), |c| {
    l::set_flag_36ca71(&mut c.global(0)[1..]);
    Out::none()
});
oracle!(orig_23b8a0_matches_the_game, spec(0x23b8a0, &[Arg::Int(&[0, 1, 2])], 0, Ret::None, &[(0x36ca74, 12, true)]), |c| {
    let v = c.int(0) as u32;
    l::push_36ca78(c.global(0), v);
    Out::none()
});
oracle!(orig_23b8d8_matches_the_game, spec(0x23b8d8, &[], 0, Ret::None, &[(0x36ca74, 12, true)]), |c| {
    l::pop_36ca78(c.global(0));
    Out::none()
});
oracle!(orig_23c820_matches_the_game, spec(0x23c820, &[], 0, Ret::Int, &[(0x36cadc, 4, true)]), |c| {
    Out::int(l::region_stack_nonempty(c.global(0)) as i64)
});
oracle!(orig_239d88_matches_the_game, spec(0x239d88, &[], 0, Ret::None, &[(0x36ca18, 8, true)]), |c| {
    l::clear_36ca18(c.global(0));
    Out::none()
});
oracle!(orig_25a848_matches_the_game, spec(0x25a848, &[], 0, Ret::None, &[(0x36cf30, 8, true)]), |c| {
    l::clear_36cf32(&mut c.global(0)[2..]);
    Out::none()
});
oracle!(orig_2a1370_matches_the_game, spec(0x2a1370, &[], 0, Ret::None, &[(0x36d3c8, 8, true)]), |c| {
    l::clear_36d3c8(c.global(0));
    Out::none()
});
oracle!(orig_2f7b80_matches_the_game, spec(0x2f7b80, &[], 0, Ret::None, &[(0x3719f0, 8, true)]), |c| {
    l::clear_3719f0(c.global(0));
    Out::none()
});
oracle!(orig_2c4300_matches_the_game, spec(0x2c4300, &[], 0, Ret::None, &[(0x36f680, 12, true)]), |c| {
    l::clear_36f680(c.global(0));
    Out::none()
});
oracle!(orig_295b68_matches_the_game, spec(0x295b68, &[], 0, Ret::None, &[(0x36d1d4, 0x1c, true)]), |c| {
    l::copy_menu_modes(c.global(0));
    Out::none()
});
oracle!(orig_2d5da8_matches_the_game, spec(0x2d5da8, &[Arg::Ptr(Block { size: 0x10, fields: &[(0x0, 4, Kind::Int)], subs: &[] }), Arg::Int(&[0]), Arg::Int(&[0])], 0, Ret::None, &[]), |c| {
    let (v, m) = (c.int(1) as u32, c.int(2) as u32);
    l::set_masked_bits(c.block(0), v, m);
    Out::none()
});
oracle!(orig_2d56d8_matches_the_game, spec(0x2d56d8, &[], 0, Ret::Int, &[(0x36fd60, 4, true)]), |c| {
    Out::int(l::minigame_present(c.global(0)) as i64)
});
oracle!(orig_2df3c8_matches_the_game, spec(0x2df3c8, &[Arg::Ptr(Block { size: 0xd0, fields: &[(0xc0, 1, Kind::Int)], subs: &[] })], 0, Ret::Int, &[]), |c| {
    Out::int(l::flip_byte_c0(c.block(0)) as i64)
});
oracle!(orig_2ea008_matches_the_game, spec(0x2ea008, &[Arg::Ptr(Block { size: 0x10, fields: &[(0x0, 4, Kind::Ptr)], subs: &[(0x0, Block { size: 0x10, fields: &[(0x8, 4, Kind::Int)], subs: &[] })] })], 0, Ret::Int, &[]), |c| {
    let Some(h) = c.deref(0, 0) else { return Out::none() };
    Out::int(l::first_ptr_word8(c.block(h)) as i32 as i64)
});
oracle!(orig_2c3a70_matches_the_game, spec(0x2c3a70, &[Arg::Ptr(Block { size: 0x10, fields: &[(0x8, 2, Kind::Int)], subs: &[] })], 0, Ret::Int, &[]), |c| {
    Out::int(l::half_at_8(c.block(0)) as i64)
});
oracle!(orig_2c3ef8_matches_the_game, spec(0x2c3ef8, &[Arg::Ptr(Block { size: 0x144, fields: &[(0x0, 4, Kind::Int),(0xc, 4, Kind::Int),(0x18, 4, Kind::Int),(0x24, 4, Kind::Int),(0x30, 4, Kind::Int),(0x3c, 4, Kind::Int),(0x48, 4, Kind::Int),(0x54, 4, Kind::Int),(0x60, 4, Kind::Int),(0x6c, 4, Kind::Int),(0x78, 4, Kind::Int),(0x84, 4, Kind::Int),(0x90, 4, Kind::Int),(0x9c, 4, Kind::Int),(0xa8, 4, Kind::Int),(0xb4, 4, Kind::Int),(0xc0, 4, Kind::Int),(0xcc, 4, Kind::Int),(0xd8, 4, Kind::Int),(0xe4, 4, Kind::Int),(0xf0, 4, Kind::Int),(0xfc, 4, Kind::Int),(0x108, 4, Kind::Int),(0x114, 4, Kind::Int),(0x120, 4, Kind::Int),(0x12c, 4, Kind::Int),(0x138, 4, Kind::Int)], subs: &[] }), Arg::Int(&[0, 1, 2, 3])], 0, Ret::None, &[]), |c| {
    let k = c.int(1) as u32;
    l::mark_record(c.block(0), k);
    Out::none()
});
oracle!(orig_2f7ec0_matches_the_game, spec(0x2f7ec0, &[Arg::Int(&[0, 1, 2])], 0, Ret::Int, &[(0x371a00, 4, true)]), |c| {
    let i = c.int(0);
    Out::int(l::table_entry_addr(c.global(0), i) as i32 as i64)
});
oracle!(orig_2bdce8_matches_the_game, spec(0x2bdce8, &[], 0, Ret::Int64, &[(0x36ca18, 4, true)]), |_c| Out::int(l::always_zero() as i64));
oracle!(orig_2ee2e0_matches_the_game, spec(0x2ee2e0, &[], 0, Ret::None, &[(0x36ca18, 4, true)]), |_c| {
    l::empty();
    Out::none()
});
