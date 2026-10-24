//! Oracle closure of the core leaves (WP-L1-P27a). The ports are in `kh2_game_core::leaves_a`, `leaves_b`, `leaves_c` and `rng`; the
//! same comparisons already run in `orig_leaves_a.rs`, `orig_leaves_b.rs`, `orig_leaves_c.rs` and `orig_rng.rs`, but there the test
//! functions are produced by `macro_rules!` and the ledger scan reads only literal `fn` items. Each test here is a literal
//! `orig_<addr>_matches_the_game` function (so the scan marks the address verified) over the same harness shape: kh2-ee runs the
//! original from the user's executable on random globals or blocks (the `factory gen` harness, `fuzz` with 2,000 to 4,000 cases), the
//! port runs on copies, and the return value and every global or block byte are compared. No callee is stubbed.
//!
//! The test names carry a `p27a` suffix only where the same address already has a differently shaped test; the plain names are used
//! otherwise.
//!
//! Skips without a disc (KH2_ISO or local/iso.txt); fails without one when KH2_REQUIRE_DISC=1.

use kh2_ee::Ee;
use kh2_factory::fuzz::*;
use kh2_game_core::{leaves_a as la, leaves_b as lb, leaves_c as lc, rng::Lcg};

fn report(st: Stats) {
    eprintln!("{} cases equal, {} interpreter steps in {:.3} s", st.cases, st.steps, st.secs);
}

fn world(what: &str) -> Option<Ee> {
    let elf = kh2_factory::exe_or_skip(what)?;
    let mut ee = Ee::new();
    ee.load_elf(&elf).expect("elf");
    Some(ee)
}

/// A getter of one global: no arguments, the global randomised, the result compared with the port's.
fn getter(addr: u32, global: u32, len: u32, port: impl Fn(&mut [u8]) -> u64) {
    let spec = Spec { addr, args: &[], floats: 0, ret: Ret::Int, globals: Box::leak(Box::new([(global, len, true)])), stubs: &[], nulls: true };
    let Some(st) = fuzz(&spec, 2000, |c| Out::int(port(c.global(0)) as i64)) else { return };
    report(st);
}

/// A function returning a constant address: the original, called with random argument registers it must ignore, returns the
/// port's constant.
fn constant(addr: u32, want: u32) {
    let Some(mut ee) = world(&format!("oracle test of {addr:#x}")) else { return };
    let mut r = Rng::new(addr as u64);
    for _ in 0..300 {
        let got = ee.call(addr, &[r.word(), r.word()], &[], STACK).expect("original") as u32;
        assert_eq!(got, want, "{addr:#x}");
    }
}

/// A setter of one byte: the byte is randomised, the original runs, and the byte must equal what the port leaves.
fn setter(addr: u32, at: u32, port: impl Fn(&mut [u8])) {
    let Some(mut ee) = world(&format!("oracle test of {addr:#x}")) else { return };
    let mut r = Rng::new(addr as u64);
    let mut seen = std::collections::HashSet::new();
    for _ in 0..300 {
        let before = r.word() as u8;
        ee.write(at, &[before]);
        ee.call(addr, &[], &[], STACK).expect("original");
        let mut g = [before];
        port(&mut g[..]);
        assert_eq!(ee.r8(at), g[0], "{addr:#x}: byte was {before:#x}");
        seen.insert(before);
    }
    assert!(seen.len() > 50, "the byte was varied");
}

/// The block-and-globals shape of `leaves_c.rs`: run `port` on the generated case.
fn oracle(spec: Spec, port: impl FnMut(&mut Case) -> Out) {
    let Some(st) = fuzz(&spec, 4000, port) else { return };
    report(st);
}

const fn spec(addr: u32, args: &'static [Arg], floats: u8, ret: Ret, globals: &'static [(u32, u32, bool)]) -> Spec {
    Spec { addr, args, floats, ret, globals, stubs: &[], nulls: true }
}

// ---- leaves_a ----

#[test]
fn orig_102e00_matches_the_game() {
    getter(0x102e00, 0x33e73c, 4, |g| la::word_33e73c(g) as u64);
}

#[test]
fn orig_102ee0_matches_the_game() {
    getter(0x102ee0, 0x33e740, 1, |g| la::byte_33e740(g) as u64);
}

#[test]
fn orig_103408_matches_the_game() {
    getter(0x103408, 0x33e780, 4, |g| la::word_33e780(g) as u64);
}

#[test]
fn orig_103418_matches_the_game() {
    getter(0x103418, 0x33e774, 4, |g| la::word_33e774(g) as u64);
}

#[test]
fn orig_103428_matches_the_game() {
    getter(0x103428, 0x33e778, 4, |g| la::word_33e778(g) as u64);
}

#[test]
fn orig_103fb0_matches_the_game() {
    getter(0x103fb0, 0x33e7d0, 4, |g| la::pause_param(g) as u64);
}

#[test]
fn orig_104058_matches_the_game() {
    getter(0x104058, 0x33e7d4, 4, |g| la::word_33e7d4(g) as u64);
}

#[test]
fn orig_104078_matches_the_game() {
    getter(0x104078, 0x33e7dc, 4, |g| la::word_33e7dc_at_least_one(g) as u64);
}

#[test]
fn orig_104120_matches_the_game() {
    getter(0x104120, 0x33e7cc, 4, |g| la::word_33e7cc(g) as u64);
}

#[test]
fn orig_104498_matches_the_game() {
    getter(0x104498, 0x33e80c, 4, |g| la::word_33e80c(g) as u64);
}

#[test]
fn orig_102f18_matches_the_game() {
    constant(0x102f18, la::title_mode());
}

#[test]
fn orig_103d68_matches_the_game() {
    constant(0x103d68, la::menu_mode());
}

#[test]
fn orig_104048_matches_the_game() {
    constant(0x104048, la::gummi_battle_mode());
}

#[test]
fn orig_104488_matches_the_game() {
    constant(0x104488, la::gummi_menu_mode());
}

#[test]
fn orig_14a938_matches_the_game() {
    constant(0x14a938, la::block_356a20());
}

#[test]
fn orig_102ef0_matches_the_game() {
    setter(0x102ef0, 0x33e740, |g| la::clear_byte_33e740(g));
}

#[test]
fn orig_14a220_matches_the_game() {
    setter(0x14a220, 0x35639a, |g| la::soft_reset_disable(g));
}

#[test]
fn orig_14a230_matches_the_game() {
    setter(0x14a230, 0x35639a, |g| la::soft_reset_enable(g));
}

// ---- leaves_b ----

/// A 64-bit store through the first argument into the block (the five rectangle setters).
fn rect_setter(addr: u32, off: u32, size: u32, port: fn(&mut [u8], u64)) {
    let fields: &'static [FieldSpec] = Box::leak(Box::new([(off, 8, Kind::Int)]));
    let spec = Spec {
        addr,
        args: Box::leak(Box::new([Arg::Ptr(Block { size, fields, subs: &[] }), Arg::Int(&[])])),
        floats: 0,
        ret: Ret::None,
        globals: &[],
        stubs: &[],
        nulls: false,
    };
    let Some(st) = fuzz(&spec, 2000, |c| {
        let v = c.ints[1];
        port(c.block(0), v);
        Out::none()
    }) else {
        return;
    };
    report(st);
}
#[test]
fn orig_14b350_matches_the_game() {
    rect_setter(0x14b350, 0xc0, 0xd0, lb::rect_set_gs_prim);
}

#[test]
fn orig_14b378_matches_the_game() {
    rect_setter(0x14b378, 0x40, 0x50, lb::rect_set_gs_frame);
}

#[test]
fn orig_14b388_matches_the_game() {
    rect_setter(0x14b388, 0x80, 0x90, lb::rect_set_gs_test);
}

#[test]
fn orig_14b398_matches_the_game() {
    rect_setter(0x14b398, 0x60, 0x70, lb::rect_set_gs_tex0);
}

#[test]
fn orig_14b3a0_matches_the_game() {
    rect_setter(0x14b3a0, 0x70, 0x80, lb::rect_set_gs_tex1);
}

/// The two effect parameter flag setters (`block +8` and the words at +0x30 and +0x34 are compared).
fn effect_flag(addr: u32, port: fn(&mut [u8], u32, u32)) {
    const F: &[FieldSpec] = &[(8, 4, Kind::Int), (0x30, 4, Kind::Int), (0x34, 4, Kind::Int)];
    let spec = Spec {
        addr,
        args: &[Arg::Ptr(Block { size: 0x40, fields: F, subs: &[] }), Arg::Int(&[]), Arg::Int(&[])],
        floats: 0,
        ret: Ret::None,
        globals: &[],
        stubs: &[],
        nulls: false,
    };
    let Some(st) = fuzz(&spec, 1000, |c| {
        let (a, b) = (c.ints[1] as u32, c.ints[2] as u32);
        port(c.block(0), a, b);
        Out::none()
    }) else {
        return;
    };
    report(st);
}
#[test]
fn orig_1da138_matches_the_game() {
    effect_flag(0x1da138, lb::effect_set_param_40000);
}

#[test]
fn orig_1da158_matches_the_game() {
    effect_flag(0x1da158, lb::effect_set_param_80000);
}

// ---- leaves_c ----

const OBJ: Block = Block { size: 0x140, fields: &[(0xa9, 1, Kind::Int), (0xaa, 1, Kind::Int), (0xcc, 4, Kind::Int), (0x114, 4, Kind::Float), (0x118, 4, Kind::Float), (0x11c, 4, Kind::Float), (0x120, 4, Kind::Float), (0x130, 4, Kind::Float), (0x134, 4, Kind::Float), (0x138, 4, Kind::Float)], subs: &[] };

#[test]
fn orig_236ae0_matches_the_game() {
    oracle(spec(0x236ae0, &[Arg::Ptr(OBJ)], 3, Ret::None, &[]), |c| {
    let (x, y, z) = (c.float(0), c.float(1), c.float(2));
    lc::set_vec3(c.block(0), x, y, z);
    Out::none()
});
}

#[test]
fn orig_236b28_matches_the_game() {
    oracle(spec(0x236b28, &[Arg::Ptr(OBJ)], 1, Ret::None, &[]), |c| {
    let x = c.float(0);
    lc::set_vec_x(c.block(0), x);
    Out::none()
});
}

#[test]
fn orig_236b70_matches_the_game() {
    oracle(spec(0x236b70, &[Arg::Ptr(OBJ)], 1, Ret::None, &[]), |c| {
    let x = c.float(0);
    lc::set_vec_y(c.block(0), x);
    Out::none()
});
}

#[test]
fn orig_236bb8_matches_the_game() {
    oracle(spec(0x236bb8, &[Arg::Ptr(OBJ)], 1, Ret::None, &[]), |c| {
    let x = c.float(0);
    lc::set_vec_z(c.block(0), x);
    Out::none()
});
}

#[test]
fn orig_236c00_matches_the_game() {
    oracle(spec(0x236c00, &[Arg::Ptr(OBJ)], 4, Ret::None, &[]), |c| {
    let (a, r, g, b) = (c.float(0), c.float(1), c.float(2), c.float(3));
    lc::set_color(c.block(0), a, r, g, b);
    Out::none()
});
}

#[test]
fn orig_23b880_matches_the_game() {
    oracle(spec(0x23b880, &[], 0, Ret::None, &[(0x36ca70, 4, true)]), |c| {
    lc::set_flag_36ca71(&mut c.global(0)[1..]);
    Out::none()
});
}

#[test]
fn orig_23b8a0_matches_the_game() {
    oracle(spec(0x23b8a0, &[Arg::Int(&[0, 1, 2])], 0, Ret::None, &[(0x36ca74, 12, true)]), |c| {
    let v = c.int(0) as u32;
    lc::push_36ca78(c.global(0), v);
    Out::none()
});
}

#[test]
fn orig_23b8d8_matches_the_game() {
    oracle(spec(0x23b8d8, &[], 0, Ret::None, &[(0x36ca74, 12, true)]), |c| {
    lc::pop_36ca78(c.global(0));
    Out::none()
});
}

#[test]
fn orig_23c820_matches_the_game() {
    oracle(spec(0x23c820, &[], 0, Ret::Int, &[(0x36cadc, 4, true)]), |c| {
    Out::int(lc::region_stack_nonempty(c.global(0)) as i64)
});
}

#[test]
fn orig_239d88_matches_the_game() {
    oracle(spec(0x239d88, &[], 0, Ret::None, &[(0x36ca18, 8, true)]), |c| {
    lc::clear_36ca18(c.global(0));
    Out::none()
});
}

#[test]
fn orig_25a848_matches_the_game() {
    oracle(spec(0x25a848, &[], 0, Ret::None, &[(0x36cf30, 8, true)]), |c| {
    lc::clear_36cf32(&mut c.global(0)[2..]);
    Out::none()
});
}

#[test]
fn orig_2a1370_matches_the_game() {
    oracle(spec(0x2a1370, &[], 0, Ret::None, &[(0x36d3c8, 8, true)]), |c| {
    lc::clear_36d3c8(c.global(0));
    Out::none()
});
}

#[test]
fn orig_2f7b80_matches_the_game() {
    oracle(spec(0x2f7b80, &[], 0, Ret::None, &[(0x3719f0, 8, true)]), |c| {
    lc::clear_3719f0(c.global(0));
    Out::none()
});
}

#[test]
fn orig_2c4300_matches_the_game() {
    oracle(spec(0x2c4300, &[], 0, Ret::None, &[(0x36f680, 12, true)]), |c| {
    lc::clear_36f680(c.global(0));
    Out::none()
});
}

#[test]
fn orig_295b68_matches_the_game() {
    oracle(spec(0x295b68, &[], 0, Ret::None, &[(0x36d1d4, 0x1c, true)]), |c| {
    lc::copy_menu_modes(c.global(0));
    Out::none()
});
}

#[test]
fn orig_2d5da8_matches_the_game() {
    oracle(spec(0x2d5da8, &[Arg::Ptr(Block { size: 0x10, fields: &[(0x0, 4, Kind::Int)], subs: &[] }), Arg::Int(&[0]), Arg::Int(&[0])], 0, Ret::None, &[]), |c| {
    let (v, m) = (c.int(1) as u32, c.int(2) as u32);
    lc::set_masked_bits(c.block(0), v, m);
    Out::none()
});
}

#[test]
fn orig_2d56d8_matches_the_game() {
    oracle(spec(0x2d56d8, &[], 0, Ret::Int, &[(0x36fd60, 4, true)]), |c| {
    Out::int(lc::minigame_present(c.global(0)) as i64)
});
}

#[test]
fn orig_2df3c8_matches_the_game() {
    oracle(spec(0x2df3c8, &[Arg::Ptr(Block { size: 0xd0, fields: &[(0xc0, 1, Kind::Int)], subs: &[] })], 0, Ret::Int, &[]), |c| {
    Out::int(lc::flip_byte_c0(c.block(0)) as i64)
});
}

#[test]
fn orig_2ea008_matches_the_game() {
    oracle(spec(0x2ea008, &[Arg::Ptr(Block { size: 0x10, fields: &[(0x0, 4, Kind::Ptr)], subs: &[(0x0, Block { size: 0x10, fields: &[(0x8, 4, Kind::Int)], subs: &[] })] })], 0, Ret::Int, &[]), |c| {
    let Some(h) = c.deref(0, 0) else { return Out::none() };
    Out::int(lc::first_ptr_word8(c.block(h)) as i32 as i64)
});
}

#[test]
fn orig_2c3a70_matches_the_game() {
    oracle(spec(0x2c3a70, &[Arg::Ptr(Block { size: 0x10, fields: &[(0x8, 2, Kind::Int)], subs: &[] })], 0, Ret::Int, &[]), |c| {
    Out::int(lc::half_at_8(c.block(0)) as i64)
});
}

#[test]
fn orig_2c3ef8_matches_the_game() {
    oracle(spec(0x2c3ef8, &[Arg::Ptr(Block { size: 0x144, fields: &[(0x0, 4, Kind::Int),(0xc, 4, Kind::Int),(0x18, 4, Kind::Int),(0x24, 4, Kind::Int),(0x30, 4, Kind::Int),(0x3c, 4, Kind::Int),(0x48, 4, Kind::Int),(0x54, 4, Kind::Int),(0x60, 4, Kind::Int),(0x6c, 4, Kind::Int),(0x78, 4, Kind::Int),(0x84, 4, Kind::Int),(0x90, 4, Kind::Int),(0x9c, 4, Kind::Int),(0xa8, 4, Kind::Int),(0xb4, 4, Kind::Int),(0xc0, 4, Kind::Int),(0xcc, 4, Kind::Int),(0xd8, 4, Kind::Int),(0xe4, 4, Kind::Int),(0xf0, 4, Kind::Int),(0xfc, 4, Kind::Int),(0x108, 4, Kind::Int),(0x114, 4, Kind::Int),(0x120, 4, Kind::Int),(0x12c, 4, Kind::Int),(0x138, 4, Kind::Int)], subs: &[] }), Arg::Int(&[0, 1, 2, 3])], 0, Ret::None, &[]), |c| {
    let k = c.int(1) as u32;
    lc::mark_record(c.block(0), k);
    Out::none()
});
}

#[test]
fn orig_2f7ec0_matches_the_game() {
    oracle(spec(0x2f7ec0, &[Arg::Int(&[0, 1, 2])], 0, Ret::Int, &[(0x371a00, 4, true)]), |c| {
    let i = c.int(0);
    Out::int(lc::table_entry_addr(c.global(0), i) as i32 as i64)
});
}

#[test]
fn orig_2bdce8_matches_the_game() {
    oracle(spec(0x2bdce8, &[], 0, Ret::Int64, &[(0x36ca18, 4, true)]), |_c| Out::int(lc::always_zero() as i64));
}

#[test]
fn orig_2ee2e0_matches_the_game() {
    oracle(spec(0x2ee2e0, &[], 0, Ret::None, &[(0x36ca18, 4, true)]), |_c| {
    lc::empty();
    Out::none()
});
}

// ---- thunk entry points and the seed ----

use std::cell::RefCell;
use std::rc::Rc;

const OBJ_AT: u32 = 0x0180_0000;
const HOOK: u32 = 0x0170_0000;
/// The calls a hooked address receives: (a0, a1, a2).
type Log = Rc<RefCell<Vec<(u32, u32, u32)>>>;

fn hook(ee: &mut Ee, at: u32, log: &Log) {
    let l = log.clone();
    ee.hooks.insert(at, move |ee: &mut Ee| l.borrow_mut().push((ee.gpr[4] as u32, ee.gpr[5] as u32, ee.gpr[6] as u32)));
}

/// The allocation thunk entered at `entry` (0x106250): slot 0 of the object's table is called as `slot0(object, size, 0)`; the
/// hooked slot receives the call and the port's `alloc_call` names the same target and arguments.
fn alloc_entry(entry: u32) {
    let Some(mut ee) = world(&format!("oracle test of {entry:#x}")) else { return };
    let log: Log = Rc::default();
    let mut r = Rng::new(entry as u64);
    for n in 0..400u32 {
        let (obj, vt) = (OBJ_AT + n * 0x40, OBJ_AT + 0x10_0000 + n * 0x10);
        let func = HOOK + (n % 8) * 0x10;
        hook(&mut ee, func, &log);
        ee.w32(obj, vt);
        ee.w32(vt, func);
        let size = r.word();
        log.borrow_mut().clear();
        ee.call(entry, &[size, obj as u64], &[], STACK).expect("original");
        let got = log.borrow().clone();
        assert_eq!(got.len(), 1, "case {n}: one virtual call");
        let want = la::alloc_call(size as u32, obj, |a| ee.r32(a));
        assert_eq!((func, [got[0].0, got[0].1, got[0].2]), (want.func, want.args), "case {n}");
    }
}

#[test]
fn orig_106250_matches_the_game() {
    alloc_entry(0x106250);
}

/// The free thunk entered at 0x1062b0: nothing for a null block, otherwise slot 1 of the table behind the word before the block.
#[test]
fn orig_1062b0_matches_the_game() {
    let Some(mut ee) = world("oracle test of 0x1062b0") else { return };
    let log: Log = Rc::default();
    let mut r = Rng::new(0x1062b0);
    let (mut nulls, mut frees) = (0, 0);
    for n in 0..400u32 {
        let (hdr, vt, block) = (OBJ_AT + 0x20_0000 + n * 0x40, OBJ_AT + 0x30_0000 + n * 0x40, OBJ_AT + 0x40_0000 + n * 0x40);
        let func = HOOK + 0x100 + (n % 8) * 0x10;
        hook(&mut ee, func, &log);
        ee.w32(block - 4, hdr);
        ee.w32(hdr, vt);
        ee.w32(vt + 4, func);
        let p: u64 = if r.below(4) == 0 { 0 } else { block as u64 };
        log.borrow_mut().clear();
        ee.call(0x1062b0, &[p], &[], STACK).expect("original");
        let got = log.borrow().clone();
        match la::free_call(p, |a| ee.r32(a)) {
            None => {
                nulls += 1;
                assert!(got.is_empty(), "case {n}: null frees nothing");
            }
            Some(c) => {
                frees += 1;
                assert_eq!(got.len(), 1, "case {n}: one virtual call");
                assert_eq!((c.func, c.args), (func, [got[0].0, got[0].1, 0]), "case {n}");
            }
        }
    }
    assert!(nulls > 0 && frees > 0, "both branches reached: {nulls} nulls, {frees} frees");
}

/// The seed setter 0x1062f0: the generator word (eight bytes at `*(0x373cf0) + 0xa8`, i.e. 0x373aa8) becomes the low 32 bits of
/// the argument. The word is randomised first, so the clearing of the high half is seen.
#[test]
fn orig_1062f0_matches_the_game() {
    let spec = Spec {
        addr: 0x1062f0,
        args: &[Arg::Int(&[0, 1, -1, 0x87_5b2f, 0x7fff_ffff])],
        floats: 0,
        ret: Ret::None,
        globals: &[(0x373aa8, 8, true)],
        stubs: &[],
        nulls: true,
    };
    let Some(st) = fuzz(&spec, 4000, |c| {
        let mut g = Lcg(u64::from_le_bytes(c.global(0)[..8].try_into().unwrap()));
        g.seed(c.int(0) as u32 as u64);
        c.global(0)[..8].copy_from_slice(&g.0.to_le_bytes());
        Out::none()
    }) else {
        return;
    };
    report(st);
}
