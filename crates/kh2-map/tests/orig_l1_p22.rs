//! Oracle tests of the collision and map object remainder (WP-L1-P22): kh2-ee runs the original from the user's own
//! executable and the port is `kh2_map::{geom, mapobj, mapcoll, sphere}`.
//!
//! Three kinds of test. The box helpers are plain factory-style fuzz tests (`fuzz(&SPEC, n, ..)`). The map object, the
//! model instance and the flag bytes are laid out in guest memory as random bytes, one original call is made, and
//! the whole record is compared with the port's. The collision wrappers and the sphere pass run over worlds: a
//! random COCT file written into emulator memory and built by the game's own `FUN_00141500`, then installed in the
//! globals the wrappers read (`DAT_00355930..4c`), with the same file bytes handed to the port.
//! Callees that are drawing, audio or the cache are replaced by hooks that record their arguments (named in each test).
//! Skips without a disc (KH2_ISO or local/iso.txt); fails without one when KH2_REQUIRE_DISC=1.

use kh2_ee::{Ee, reg};
use kh2_factory::fuzz::*;
use kh2_map::geom::*;
use kh2_map::mapcoll::*;
use kh2_map::mapobj::*;
use kh2_map::oct::*;
use std::cell::RefCell;
use std::rc::Rc;

fn game(what: &str) -> Option<Ee> {
    let elf = kh2_factory::exe_or_skip(what)?;
    let mut ee = Ee::new();
    ee.load_elf(&elf).expect("elf");
    Some(ee)
}

fn call(ee: &mut Ee, addr: u32, args: &[u64]) -> u64 {
    ee.call(addr, args, &[], STACK).unwrap_or_else(|f| panic!("the original faulted at {addr:#x}: {f:?}"))
}

fn rand_bytes(r: &mut Rng, n: usize) -> Vec<u8> {
    (0..n).map(|_| r.word() as u8).collect()
}

fn feq(a: f32, b: f32) -> bool {
    a.to_bits() == b.to_bits() || (a - b).abs() <= 1e-5 * (1.0 + a.abs().max(b.abs()))
}

fn same4(a: V4, b: V4, lanes: usize) -> bool {
    (0..lanes).all(|k| feq(a[k], b[k]))
}

fn urange(r: &mut Rng, lo: f32, hi: f32) -> f32 {
    lo + (r.word() >> 40) as f32 / (1u64 << 24) as f32 * (hi - lo)
}

fn pick<T: Copy>(r: &mut Rng, s: &[T]) -> T {
    s[r.below(s.len() as u64) as usize]
}

fn v4f(b: &[u8], o: usize) -> V4 {
    std::array::from_fn(|k| f32::from_le_bytes(b[o + 4 * k..o + 4 * k + 4].try_into().unwrap()))
}

fn floats6(b: &[u8]) -> FBox {
    std::array::from_fn(|k| f32::from_le_bytes(b[4 * k..4 * k + 4].try_into().unwrap()))
}

fn put_f(dst: &mut [u8], at: usize, vals: &[f32]) {
    for (k, f) in vals.iter().enumerate() {
        dst[at + 4 * k..at + 4 * k + 4].copy_from_slice(&f.to_le_bytes());
    }
}

// ---------------------------------------------------------------------------------------------------------
// Boxes
// ---------------------------------------------------------------------------------------------------------

const FBOX: Block = Block {
    size: 24,
    fields: &[(0, 4, Kind::Float), (4, 4, Kind::Float), (8, 4, Kind::Float), (12, 4, Kind::Float), (16, 4, Kind::Float), (20, 4, Kind::Float)],
    subs: &[],
};

/// Two points one after the other (32 bytes of floats).
const TWO_POINTS: Block = Block { size: 32, fields: &[(0, 32, Kind::Float)], subs: &[] };

/// A matrix of sixteen floats.
const MATRIX: Block = Block { size: 64, fields: &[(0, 64, Kind::Float)], subs: &[] };

/// Output blocks compared as raw bytes.
const RAW24: Block = Block { size: 24, fields: &[], subs: &[] };
const RAW16: Block = Block { size: 16, fields: &[], subs: &[] };
const RAW128: Block = Block { size: 128, fields: &[], subs: &[] };

/// A short box (six shorts).
const QBOX: Block = Block { size: 12, fields: &[(0, 4, Kind::Int), (4, 4, Kind::Int), (8, 4, Kind::Int)], subs: &[] };

/// orig 0x144368: the box constructor clears its six floats; the bytes after them stay.
#[test]
fn orig_144368_matches_the_game() {
    let Some(mut ee) = game("0x144368") else { return };
    let mut r = Rng::new(0x144368);
    for n in 0..300 {
        let junk = rand_bytes(&mut r, 40);
        ee.write(0x0180_0000, &junk);
        call(&mut ee, 0x144368, &[0x0180_0000]);
        let after = ee.read::<40>(0x0180_0000);
        let mut want = junk.clone();
        put_f(&mut want, 0, &fbox_clear());
        assert_eq!(after.to_vec(), want, "case {n}");
        assert!(junk[..24].iter().any(|&b| b != 0));
    }
}

/// orig 0x1443e0: the box constructor from two corners (it forwards to 0x1446f8).
#[test]
fn orig_1443e0_matches_the_game() {
    static SPEC: Spec = Spec { addr: 0x1443e0, args: &[Arg::Ptr(FBOX), Arg::Ptr(FBOX), Arg::Ptr(FBOX)], floats: 0, ret: Ret::None, globals: &[], stubs: &[], nulls: false };
    let Some(st) = fuzz(&SPEC, 4000, |c| {
        let (a, b) = (floats6(&c.blocks[1]), floats6(&c.blocks[2]));
        let o = fbox_new([a[0], a[1], a[2]], [b[0], b[1], b[2]]);
        put_f(&mut c.blocks[0], 0, &o);
        Out::none()
    }) else { return };
    eprintln!("{} cases equal, {} interpreter steps in {:.3} s", st.cases, st.steps, st.secs);
}

/// orig 0x144410: the box union wrapper (it forwards to 0x144778).
#[test]
fn orig_144410_matches_the_game() {
    static SPEC: Spec = Spec { addr: 0x144410, args: &[Arg::Ptr(FBOX), Arg::Ptr(FBOX), Arg::Ptr(FBOX)], floats: 0, ret: Ret::None, globals: &[], stubs: &[], nulls: false };
    let Some(st) = fuzz(&SPEC, 4000, |c| {
        let o = fbox_merge(&floats6(&c.blocks[1]), &floats6(&c.blocks[2]));
        put_f(&mut c.blocks[0], 0, &o);
        Out::none()
    }) else { return };
    eprintln!("{} cases equal, {} interpreter steps in {:.3} s", st.cases, st.steps, st.secs);
}

/// orig 0x144530: the box of two points stored one after the other.
#[test]
fn orig_144530_matches_the_game() {
    static SPEC: Spec = Spec { addr: 0x144530, args: &[Arg::Ptr(FBOX), Arg::Ptr(TWO_POINTS)], floats: 0, ret: Ret::None, globals: &[], stubs: &[], nulls: false };
    let Some(st) = fuzz(&SPEC, 4000, |c| {
        let p = [v4f(&c.blocks[1], 0), v4f(&c.blocks[1], 16)];
        put_f(&mut c.blocks[0], 0, &fbox_of_points(&p));
        Out::none()
    }) else { return };
    eprintln!("{} cases equal, {} interpreter steps in {:.3} s", st.cases, st.steps, st.secs);
}

/// orig 0x1446d8: the box set from six floats (registers f12 to f17) into the box at a0.
#[test]
fn orig_1446d8_matches_the_game() {
    static SPEC: Spec = Spec { addr: 0x1446d8, args: &[Arg::Ptr(RAW24)], floats: 6, ret: Ret::None, globals: &[], stubs: &[], nulls: false };
    let Some(st) = fuzz(&SPEC, 3000, |c| {
        let o = fbox_set(c.floats[0], c.floats[1], c.floats[2], c.floats[3], c.floats[4], c.floats[5]);
        for (k, f) in o.iter().enumerate() {
            c.blocks[0][4 * k..4 * k + 4].copy_from_slice(&f.to_bits().to_le_bytes());
        }
        Out::none()
    }) else { return };
    eprintln!("{} cases equal, {} interpreter steps in {:.3} s", st.cases, st.steps, st.secs);
}

/// orig 0x144820: the centre of a box.
#[test]
fn orig_144820_matches_the_game() {
    static SPEC: Spec = Spec { addr: 0x144820, args: &[Arg::Ptr(FBOX), Arg::Ptr(RAW16)], floats: 0, ret: Ret::None, globals: &[], stubs: &[], nulls: false };
    let Some(st) = fuzz(&SPEC, 4000, |c| {
        let o = fbox_centre(&floats6(&c.blocks[0]));
        put_f(&mut c.blocks[1], 0, &o);
        Out::none()
    }) else { return };
    eprintln!("{} cases equal, {} interpreter steps in {:.3} s", st.cases, st.steps, st.secs);
}

/// orig 0x1448c0: the box transformed in place by a matrix (the eight corners of 0x144a50 through the matrix).
#[test]
fn orig_1448c0_matches_the_game() {
    static SPEC: Spec = Spec { addr: 0x1448c0, args: &[Arg::Ptr(FBOX), Arg::Ptr(MATRIX)], floats: 0, ret: Ret::None, globals: &[], stubs: &[], nulls: false };
    let Some(st) = fuzz(&SPEC, 4000, |c| {
        let m: M4 = std::array::from_fn(|i| v4f(&c.blocks[1], 16 * i));
        let o = fbox_transform(&floats6(&c.blocks[0]), &m);
        put_f(&mut c.blocks[0], 0, &o);
        Out::none()
    }) else { return };
    eprintln!("{} cases equal, {} interpreter steps in {:.3} s", st.cases, st.steps, st.secs);
}

/// orig 0x144a50: the eight corners of a box.
#[test]
fn orig_144a50_matches_the_game() {
    static SPEC: Spec = Spec { addr: 0x144a50, args: &[Arg::Ptr(FBOX), Arg::Ptr(RAW128)], floats: 0, ret: Ret::None, globals: &[], stubs: &[], nulls: false };
    let Some(st) = fuzz(&SPEC, 3000, |c| {
        let o = fbox_corners8(&floats6(&c.blocks[0]));
        for (k, v) in o.iter().enumerate() {
            for (l, f) in v.iter().enumerate() {
                c.blocks[1][16 * k + 4 * l..16 * k + 4 * l + 4].copy_from_slice(&f.to_bits().to_le_bytes());
            }
        }
        Out::none()
    }) else { return };
    eprintln!("{} cases equal, {} interpreter steps in {:.3} s", st.cases, st.steps, st.secs);
}

/// orig 0x144af8: the eight corners as the negated list.
#[test]
fn orig_144af8_matches_the_game() {
    static SPEC: Spec = Spec { addr: 0x144af8, args: &[Arg::Ptr(FBOX), Arg::Ptr(RAW128)], floats: 0, ret: Ret::None, globals: &[], stubs: &[], nulls: false };
    let Some(st) = fuzz(&SPEC, 3000, |c| {
        let o = fbox_corners8_negated(&floats6(&c.blocks[0]));
        for (k, v) in o.iter().enumerate() {
            for (l, f) in v.iter().enumerate() {
                c.blocks[1][16 * k + 4 * l..16 * k + 4 * l + 4].copy_from_slice(&f.to_bits().to_le_bytes());
            }
        }
        Out::none()
    }) else { return };
    eprintln!("{} cases equal, {} interpreter steps in {:.3} s", st.cases, st.steps, st.secs);
}

/// orig 0x144ca8: does the first box contain the second.
#[test]
fn orig_144ca8_matches_the_game() {
    static SPEC: Spec = Spec { addr: 0x144ca8, args: &[Arg::Ptr(FBOX), Arg::Ptr(FBOX)], floats: 0, ret: Ret::Int, globals: &[], stubs: &[], nulls: false };
    let Some(st) = fuzz(&SPEC, 6000, |c| Out::int(fbox_contains(&floats6(&c.blocks[0]), &floats6(&c.blocks[1])) as i64)) else { return };
    eprintln!("{} cases equal, {} interpreter steps in {:.3} s", st.cases, st.steps, st.secs);
}

/// orig 0x144d48: the short box constructor clears its six shorts; the bytes after them stay.
#[test]
fn orig_144d48_matches_the_game() {
    let Some(mut ee) = game("0x144d48") else { return };
    let mut r = Rng::new(0x144d48);
    for n in 0..300 {
        let junk = rand_bytes(&mut r, 32);
        ee.write(0x0180_0000, &junk);
        call(&mut ee, 0x144d48, &[0x0180_0000]);
        let after = ee.read::<32>(0x0180_0000);
        let mut want = junk.clone();
        for (k, v) in qbox_clear().iter().enumerate() {
            want[2 * k..2 * k + 2].copy_from_slice(&v.to_le_bytes());
        }
        assert_eq!(after.to_vec(), want, "case {n}");
    }
}

/// orig 0x145000: the short box to a float box at levels 0 to 3 (the level is a 64-bit register in the original,
/// tested against zero as a whole).
#[test]
fn orig_145000_matches_the_game() {
    static SPEC: Spec = Spec { addr: 0x145000, args: &[Arg::Ptr(QBOX), Arg::Range(0, 3), Arg::Ptr(RAW24)], floats: 0, ret: Ret::None, globals: &[], stubs: &[], nulls: false };
    let Some(st) = fuzz(&SPEC, 4000, |c| {
        let q: QBox = std::array::from_fn(|k| i16::from_le_bytes(c.blocks[0][2 * k..2 * k + 2].try_into().unwrap()));
        let o = qbox_to_fbox(&q, c.int(1) as usize);
        for (k, f) in o.iter().enumerate() {
            c.blocks[1][4 * k..4 * k + 4].copy_from_slice(&f.to_bits().to_le_bytes());
        }
        Out::none()
    }) else { return };
    eprintln!("{} cases equal, {} interpreter steps in {:.3} s", st.cases, st.steps, st.secs);
}

const LAB_AT: u32 = 0x01a0_0000;
const LAB_VERTS: u32 = LAB_AT + 0x100;
const LAB_OUT: u32 = LAB_AT + 0x280;
const LAB_VTX: u32 = LAB_AT + 0x400;

fn put4(ee: &mut Ee, at: u32, v: V4) {
    for (k, f) in v.iter().enumerate() {
        ee.wf32(at + 4 * k as u32, *f);
    }
}

fn get4(ee: &mut Ee, at: u32) -> V4 {
    std::array::from_fn(|k| ee.f32_at(at + 4 * k as u32))
}

fn rand_v4(r: &mut Rng, span: f32) -> V4 {
    [urange(r, -span, span), urange(r, -span, span), urange(r, -span, span), urange(r, -2.0, 2.0)]
}

/// The polygon record `FUN_001418a0` produces: four vertex pointers and the count.
fn put_verts(ee: &mut Ee, at: u32, v: &Verts) {
    for k in 0..4 {
        let p = LAB_VTX + 16 * k as u32;
        put4(ee, p, v.v[k]);
        ee.w32(at + 4 * k as u32, p);
    }
    ee.w32(at + 0x10, v.n as u32);
}

fn rand_verts(r: &mut Rng) -> Verts {
    let mut v = [[0.0; 4]; 4];
    for p in v.iter_mut() {
        *p = rand_v4(r, 200.0);
    }
    Verts { v, n: if r.below(3) == 0 { 3 } else { 4 } }
}

/// orig 0x144500: the polygon bounds wrapper (it forwards to 0x144548).
#[test]
fn orig_144500_matches_the_game() {
    let Some(mut ee) = game("0x144500") else { return };
    let mut r = Rng::new(0x144500);
    for n in 0..3000 {
        let v = rand_verts(&mut r);
        put_verts(&mut ee, LAB_VERTS, &v);
        ee.write(LAB_OUT, &[0xeeu8; 32]);
        call(&mut ee, 0x144500, &[LAB_OUT as u64, LAB_VERTS as u64]);
        let game: FBox = std::array::from_fn(|k| ee.f32_at(LAB_OUT + 4 * k as u32));
        assert_eq!(game, fbox_around(&v.v, v.n), "case {n}");
    }
}

// ---------------------------------------------------------------------------------------------------------
// The map object
// ---------------------------------------------------------------------------------------------------------

const OBJ: u32 = 0x0190_0000;
const MODEL: u32 = 0x0191_0000;
const BUF: u32 = 0x0192_0000;

/// Lays a random record at `OBJ`, calls `addr` with `(OBJ, index)` and compares the returned word with the port.
fn getter(addr: u32, index_below: u64, port: impl Fn(&MapObject, usize) -> u32) {
    let Some(mut ee) = game(&format!("{addr:#x}")) else { return };
    let mut r = Rng::new(addr as u64);
    let mut distinct = std::collections::BTreeSet::new();
    for n in 0..400 {
        let bytes = rand_bytes(&mut r, MAP_OBJECT_SIZE);
        ee.write(OBJ, &bytes);
        let idx = r.below(index_below.max(1)) as usize;
        let got = call(&mut ee, addr, &[OBJ as u64, idx as u64]) as u32;
        assert_eq!(got, port(&MapObject::from_bytes(&bytes), idx), "case {n}: index {idx}");
        distinct.insert(got);
    }
    assert!(distinct.len() > 100, "the getter must return varied words");
}

macro_rules! getter_test {
    ($name:ident, $addr:expr, $below:expr, $port:expr) => {
        #[test]
        fn $name() {
            getter($addr, $below, $port);
        }
    };
}

getter_test!(orig_1491a0_matches_the_game, 0x1491a0, 1, |o, _| o.model_class());
getter_test!(orig_1491a8_matches_the_game, 0x1491a8, 1, |o, _| o.texture_data());
getter_test!(orig_1491b0_matches_the_game, 0x1491b0, 1, |o, _| o.draw_tree());
getter_test!(orig_1491b8_matches_the_game, 0x1491b8, 1, |o, _| o.collision_main());
getter_test!(orig_1491c0_matches_the_game, 0x1491c0, 1, |o, _| o.collision_second());
getter_test!(orig_1491c8_matches_the_game, 0x1491c8, 1, |o, _| o.collision_colour());
getter_test!(orig_1491d0_matches_the_game, 0x1491d0, 1, |o, _| o.collision_aux());
getter_test!(orig_1491d8_matches_the_game, 0x1491d8, 1, |o, _| o.collision_kinds());
getter_test!(orig_1491e0_matches_the_game, 0x1491e0, 1, |o, _| o.prop_table());
getter_test!(orig_1491e8_matches_the_game, 0x1491e8, 1, |o, _| o.draw_table());
getter_test!(orig_1491f0_matches_the_game, 0x1491f0, 1, |o, _| o.draw_table_b());
getter_test!(orig_1491f8_matches_the_game, 0x1491f8, 1, |o, _| o.word_50());
getter_test!(orig_149200_matches_the_game, 0x149200, 4, |o, n| o.entry(n));
getter_test!(orig_149210_matches_the_game, 0x149210, 1, |o, _| o.place_picture());
getter_test!(orig_149218_matches_the_game, 0x149218, 1, |o, _| o.place_picture_b());
getter_test!(orig_149220_matches_the_game, 0x149220, MAP_MODELS as u64, |o, n| o.model_word(n));
getter_test!(orig_149238_matches_the_game, 0x149238, MAP_MODELS as u64, |o, n| o.model_word_b(n));
getter_test!(orig_149250_matches_the_game, 0x149250, MAP_MODELS as u64, |o, n| o.model_word_c(n));

/// Runs `addr(OBJ)` on a random record and compares the whole record with `port` applied to a copy.
fn record_op(addr: u32, port: impl Fn(&mut MapObject)) {
    let Some(mut ee) = game(&format!("{addr:#x}")) else { return };
    let mut r = Rng::new(addr as u64);
    for n in 0..200 {
        let bytes = rand_bytes(&mut r, MAP_OBJECT_SIZE + 16);
        ee.write(OBJ, &bytes);
        call(&mut ee, addr, &[OBJ as u64]);
        let after = ee.read::<{ MAP_OBJECT_SIZE + 16 }>(OBJ);
        let mut mine = MapObject::from_bytes(&bytes);
        port(&mut mine);
        assert_eq!(&after[..MAP_OBJECT_SIZE], mine.bytes(), "case {n}");
        assert_eq!(&after[MAP_OBJECT_SIZE..], &bytes[MAP_OBJECT_SIZE..], "case {n}: bytes past the record");
    }
}

/// orig 0x1490f0: the map object's field clear.
#[test]
fn orig_1490f0_matches_the_game() {
    record_op(0x1490f0, |o| o.clear());
}

/// orig 0x1490c0: the map object's static initialiser (it clears the record).
#[test]
fn orig_1490c0_matches_the_game() {
    record_op(0x1490c0, |o| o.init());
}

/// orig 0x1490d8: the map object's reset (it clears the record).
#[test]
fn orig_1490d8_matches_the_game() {
    record_op(0x1490d8, |o| o.reset());
}

fn put_str(ee: &mut Ee, at: u32, s: &str) {
    ee.write(at, s.as_bytes());
    ee.write(at + s.len() as u32, &[0]);
}

fn rand_name(r: &mut Rng, max: u64) -> String {
    let n = 1 + r.below(max);
    (0..n).map(|_| (b'a' + r.below(26) as u8) as char).collect()
}

/// The area name getter `FUN_00105010` (verified elsewhere) is replaced by one that returns the string at `AREA_AT`;
/// the language directory pointer `DAT_00349d40` points at `LANG_AT`.
const AREA_AT: u32 = 0x0193_0000;
const LANG_AT: u32 = 0x0193_1000;

fn name_world(ee: &mut Ee, r: &mut Rng) -> (String, String) {
    ee.hooks.insert(0x105010, |ee: &mut Ee| ee.gpr[reg::V0] = AREA_AT as u128);
    let area = rand_name(r, 7);
    let lang = rand_name(r, 3);
    put_str(ee, AREA_AT, &area);
    put_str(ee, LANG_AT, &lang);
    ee.w32(0x349d40, LANG_AT);
    (lang, area)
}

/// orig 0x149268: the map file name written to the buffer in a1 (`map/%s/%s.map` of the language directory and the
/// area name).
#[test]
fn orig_149268_matches_the_game() {
    let Some(mut ee) = game("0x149268") else { return };
    let mut r = Rng::new(0x149268);
    for n in 0..200 {
        let (lang, area) = name_world(&mut ee, &mut r);
        ee.write(BUF, &[0x55u8; 64]);
        call(&mut ee, 0x149268, &[0, BUF as u64]);
        let got = ee.read::<64>(BUF);
        let mut want = map_file_name(&lang, &area);
        want.push(0);
        assert_eq!(&got[..want.len()], &want[..], "case {n}");
        assert!(got[want.len()..].iter().all(|&b| b == 0x55), "case {n}: nothing written past the terminator");
    }
}

/// orig 0x1492b8: the default file name goes into the object's name buffer only when its first byte is zero.
#[test]
fn orig_1492b8_matches_the_game() {
    let Some(mut ee) = game("0x1492b8") else { return };
    let mut r = Rng::new(0x1492b8);
    let (mut empty, mut set) = (0, 0);
    for n in 0..300 {
        let (lang, area) = name_world(&mut ee, &mut r);
        let mut bytes = rand_bytes(&mut r, MAP_OBJECT_SIZE);
        if r.below(2) == 0 {
            bytes[0] = 0;
        }
        ee.write(OBJ, &bytes);
        call(&mut ee, 0x1492b8, &[OBJ as u64]);
        let after = ee.read::<0x40>(OBJ);
        let mut want = bytes[..0x40].to_vec();
        if bytes[0] == 0 {
            let mut name = map_file_name(&lang, &area);
            name.push(0);
            want[..name.len()].copy_from_slice(&name);
            empty += 1;
        } else {
            set += 1;
        }
        assert_eq!(after.to_vec(), want, "case {n}");
    }
    assert!(empty > 50 && set > 50);
}

/// orig 0x1492e8: the map layout build copies the file name into the object's name buffer (a string copy including the
/// terminator) after measuring it.
#[test]
fn orig_1492e8_matches_the_game() {
    let Some(mut ee) = game("0x1492e8") else { return };
    let mut r = Rng::new(0x1492e8);
    for n in 0..300 {
        let name = rand_name(&mut r, 24);
        put_str(&mut ee, BUF, &name);
        let bytes = rand_bytes(&mut r, MAP_OBJECT_SIZE);
        ee.write(OBJ, &bytes);
        call(&mut ee, 0x1492e8, &[OBJ as u64, BUF as u64]);
        let after = ee.read::<MAP_OBJECT_SIZE>(OBJ);
        let mut mine = MapObject::from_bytes(&bytes);
        mine.set_name(name.as_bytes());
        assert_eq!(after.to_vec(), mine.bytes().to_vec(), "case {n}");
    }
}

/// orig 0x149328: the map file read request is the cache's `ReadRequest(object, 0x438, 0)`.
#[test]
fn orig_149328_matches_the_game() {
    let Some(mut ee) = game("0x149328") else { return };
    let seen = Rc::new(RefCell::new(Vec::new()));
    let s = seen.clone();
    ee.hooks.insert(0x16fac8, move |ee: &mut Ee| s.borrow_mut().push((ee.gpr[reg::A0] as u32, ee.gpr[reg::A0 + 1] as u32, ee.gpr[reg::A0 + 2] as u32)));
    let mut r = Rng::new(0x149328);
    for n in 0..50 {
        let obj = 0x0190_0000 + 0x1000 * r.below(64) as u32;
        seen.borrow_mut().clear();
        call(&mut ee, 0x149328, &[obj as u64]);
        let (a1, a2) = MapObject::read_request();
        assert_eq!(*seen.borrow(), vec![(obj, a1, a2)], "case {n}");
    }
}

/// orig 0x149ad0: the map buffer free hands the cache the buffer handle at `+0x20`.
#[test]
fn orig_149ad0_matches_the_game() {
    let Some(mut ee) = game("0x149ad0") else { return };
    let seen = Rc::new(RefCell::new(Vec::new()));
    let s = seen.clone();
    ee.hooks.insert(0x16fd70, move |ee: &mut Ee| s.borrow_mut().push(ee.gpr[reg::A0] as u32));
    let mut r = Rng::new(0x149ad0);
    for n in 0..100 {
        let bytes = rand_bytes(&mut r, MAP_OBJECT_SIZE);
        ee.write(OBJ, &bytes);
        seen.borrow_mut().clear();
        call(&mut ee, 0x149ad0, &[OBJ as u64]);
        assert_eq!(*seen.borrow(), vec![MapObject::from_bytes(&bytes).buffer_handle()], "case {n}");
    }
}

// ---------------------------------------------------------------------------------------------------------
// The model instance and the flag bytes
// ---------------------------------------------------------------------------------------------------------

/// A random model instance at `MODEL`, `DAT_00355948` pointing at it; calls `addr(args)`; compares the instance.
fn model_op(addr: u32, args: impl Fn(&mut Rng) -> Vec<u64>, port: impl Fn(&mut MapModel, &[u64]) -> Option<u32>) {
    let Some(mut ee) = game(&format!("{addr:#x}")) else { return };
    let mut r = Rng::new(addr as u64);
    for n in 0..200 {
        let bytes = rand_bytes(&mut r, MAP_MODEL_SIZE + 16);
        ee.write(MODEL, &bytes);
        ee.w32(0x355948, MODEL);
        let a = args(&mut r);
        let ret = call(&mut ee, addr, &a) as u32;
        let after = ee.read::<{ MAP_MODEL_SIZE + 16 }>(MODEL);
        let mut mine = MapModel::from_bytes(&bytes);
        if let Some(w) = port(&mut mine, &a) {
            assert_eq!(ret, w, "case {n}: return");
        }
        assert_eq!(&after[..MAP_MODEL_SIZE], mine.bytes(), "case {n}");
        assert_eq!(&after[MAP_MODEL_SIZE..], &bytes[MAP_MODEL_SIZE..], "case {n}: past the instance");
    }
}

/// orig 0x143208: the three colour bytes of the model instance.
#[test]
fn orig_143208_matches_the_game() {
    model_op(0x143208, |r| vec![r.word() & 0xff, r.word() & 0xff, r.word() & 0xff], |m, a| {
        m.set_colour(a[0] as u8, a[1] as u8, a[2] as u8);
        None
    });
}

/// orig 0x143230: the first word of the model instance.
#[test]
fn orig_143230_matches_the_game() {
    model_op(0x143230, |_| vec![], |m, _| Some(m.handle()));
}

/// orig 0x1430d8: the model instance switch byte cleared.
#[test]
fn orig_1430d8_matches_the_game() {
    model_op(0x1430d8, |_| vec![], |m, _| {
        m.clear_switch();
        None
    });
}

/// orig 0x1430e8: the model instance switch byte set (real code; its only caller is the twin of 0x15edb0).
#[test]
fn orig_1430e8_matches_the_game() {
    model_op(0x1430e8, |_| vec![], |m, _| {
        m.set_switch();
        None
    });
}

/// The four flag setters: the bytes `0x35594c..0x35594f` start random; only the named one changes.
fn flag_op(addr: u32, port: impl Fn(&mut MapCollision)) {
    let Some(mut ee) = game(&format!("{addr:#x}")) else { return };
    let mut r = Rng::new(addr as u64);
    for n in 0..100 {
        let before: [u8; 4] = std::array::from_fn(|_| r.word() as u8);
        ee.write(0x35594c, &before);
        call(&mut ee, addr, &[]);
        let after = ee.read::<4>(0x35594c);
        let mut m = MapCollision { flag_4c: before[0], flag_4d: before[1], flag_4e: before[2], flag_4f: before[3], ..Default::default() };
        port(&mut m);
        assert_eq!(after, [m.flag_4c, m.flag_4d, m.flag_4e, m.flag_4f], "case {n}");
    }
}

/// orig 0x143180
#[test]
fn orig_143180_matches_the_game() {
    flag_op(0x143180, |m| m.set_flag_4e());
}

/// orig 0x143190
#[test]
fn orig_143190_matches_the_game() {
    flag_op(0x143190, |m| m.clear_flag_4e());
}

/// orig 0x1431b0
#[test]
fn orig_1431b0_matches_the_game() {
    flag_op(0x1431b0, |m| m.set_flag_4f());
}

/// orig 0x1431c0
#[test]
fn orig_1431c0_matches_the_game() {
    flag_op(0x1431c0, |m| m.clear_flag_4f());
}

/// orig 0x144328: the collision unit's static initialiser changes nothing but its own stack.
#[test]
fn orig_144328_matches_the_game() {
    let Some(mut ee) = game("0x144328") else { return };
    let before = ee.ram.clone();
    call(&mut ee, 0x144328, &[]);
    coct_static_init();
    let diff = (0..before.len()).find(|&i| before[i] != ee.ram[i] && !(STACK - 0x10000..STACK + 0x100).contains(&(i as u32)));
    assert_eq!(diff, None, "the initialiser wrote to memory");
}

// ---------------------------------------------------------------------------------------------------------
// A random COCT file (the generator of orig_coct.rs)
// ---------------------------------------------------------------------------------------------------------

struct Poly {
    v: Vec<[f32; 3]>,
    attr: u32,
    material: u8,
    boxed: bool,
}

fn sub3(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
fn cross3(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}
fn norm3(a: [f32; 3]) -> [f32; 3] {
    let l = (a[0] * a[0] + a[1] * a[1] + a[2] * a[2]).sqrt().max(1e-6);
    [a[0] / l, a[1] / l, a[2] / l]
}

fn random_attr(r: &mut Rng, wall: bool) -> u32 {
    let nib = pick(r, &[0u32, 1, 2, 3, 4, 5, 6, 8, 0xa]);
    let mut a = nib;
    if wall && r.below(4) != 0 {
        a |= 0x10;
    }
    for bit in [0x10u32, 0x20, 0x40, 0x80, 0x100, 0x800, 0x1000, 0x80000, 0x200, 0x4000, 0x8000, 0x10000] {
        if r.below(5) == 0 {
            a |= bit;
        }
    }
    if r.below(14) == 0 {
        a |= 0x100000;
    }
    a
}

/// A planar polygon of a random kind around `(cx, cz)`: floors (normal up in the game's Y-down space), walls,
/// slopes and triangles.
fn random_poly(r: &mut Rng, span: f32) -> Poly {
    let cx = urange(r, -span, span);
    let cz = urange(r, -span, span);
    let y0 = urange(r, -80.0, 80.0);
    let kind = r.below(10);
    let sz = urange(r, 80.0, 500.0);
    let (a, u, w): ([f32; 3], [f32; 3], [f32; 3]) = match kind {
        0..=2 => ([cx, y0, cz], [sz, 0.0, 0.0], [0.0, 0.0, sz * urange(r, 0.4, 1.2)]),
        3..=6 => {
            let ang = urange(r, 0.0, std::f32::consts::TAU);
            let h = urange(r, 120.0, 320.0);
            ([cx, y0, cz], [ang.cos() * sz, 0.0, ang.sin() * sz], [0.0, -h, 0.0])
        }
        7 => {
            let slope = urange(r, -0.9, 0.9);
            ([cx, y0, cz], [sz, slope * sz, 0.0], [0.0, 0.0, sz])
        }
        _ => ([cx, y0, cz], [sz, urange(r, -40.0, 40.0), 0.0], [0.0, urange(r, -40.0, 40.0), sz]),
    };
    let tri = r.below(3) == 0;
    let p0 = a;
    let p1 = [a[0] + u[0], a[1] + u[1], a[2] + u[2]];
    let p2 = [p1[0] + w[0], p1[1] + w[1], p1[2] + w[2]];
    let p3 = [a[0] + w[0], a[1] + w[1], a[2] + w[2]];
    let mut v = if tri { vec![p0, p1, p3] } else { vec![p0, p1, p2, p3] };
    // Floors face up (negative y): flip the winding when the normal points down.
    let n = cross3(sub3(v[1], v[0]), sub3(v[2], v[0]));
    let want_up = kind <= 2 || kind >= 7;
    if (want_up && n[1] > 0.0) || (!want_up && r.below(2) == 0) {
        v.reverse();
    }
    let wall = (3..=6).contains(&kind);
    Poly { v, attr: random_attr(r, wall), material: r.below(8) as u8, boxed: r.below(2) == 0 }
}

/// A wall of 100 to 260 units a few tens of units from the origin, any heading, party-solid.
fn dense_wall(r: &mut Rng, k: usize) -> Poly {
    // The first dozen are parallel and a few units apart, so one sphere pushed out of each in turn counts a dozen
    // contacts; the rest face every way.
    let stack = k < 12;
    let ang = if stack { 0.4 } else { urange(r, 0.0, std::f32::consts::TAU) };
    let (c, s) = (ang.cos(), ang.sin());
    let off = if stack { [c * (k as f32 * 3.0 - 18.0), s * (k as f32 * 3.0 - 18.0)] } else { [urange(r, -50.0, 50.0), urange(r, -50.0, 50.0)] };
    let half = if stack { 140.0 } else { urange(r, 50.0, 130.0) };
    let (ux, uz) = (-s * half, c * half);
    let (a, b) = ([off[0] - ux, off[1] - uz], [off[0] + ux, off[1] + uz]);
    let h = urange(r, 150.0, 300.0);
    let mut v = vec![[a[0], 0.0, a[1]], [b[0], 0.0, b[1]], [b[0], -h, b[1]], [a[0], -h, a[1]]];
    if !stack && r.below(2) == 0 {
        v.reverse();
    }
    Poly { v, attr: 0x172, material: 1, boxed: r.below(2) == 0 }
}

fn bounds(v: &[[f32; 3]]) -> FBox {
    let mut b = [f32::MAX, f32::MAX, f32::MAX, f32::MIN, f32::MIN, f32::MIN];
    for p in v {
        for k in 0..3 {
            b[k] = b[k].min(p[k]);
            b[k + 3] = b[k + 3].max(p[k]);
        }
    }
    b
}

fn merge(a: FBox, b: FBox) -> FBox {
    std::array::from_fn(|k| if k < 3 { a[k].min(b[k]) } else { a[k].max(b[k]) })
}

fn pad(mut v: Vec<u8>) -> Vec<u8> {
    while !v.len().is_multiple_of(16) {
        v.push(0);
    }
    v
}

/// The COCT file image: header, tree nodes, meshes, polygons, vertices, planes, boxes, attributes.
fn build_file(r: &mut Rng, polys: &[Poly], level: u32) -> Vec<u8> {
    let n_mesh = (3 + r.below(4) as usize).min(polys.len());
    let per = polys.len().div_ceil(n_mesh);
    let mut meshes = Vec::new(); // (first, last, group, bounds)
    let mut i = 0;
    while i < polys.len() {
        let j = (i + per).min(polys.len());
        let b = polys[i..j].iter().map(|p| bounds(&p.v)).reduce(merge).unwrap();
        meshes.push((i, j, pick(r, &[0u8, 1, 2, 3, 3, 62, 64, 70, 200]), b));
        i = j;
    }
    let all = meshes.iter().map(|m| m.3).reduce(merge).unwrap();
    let half = meshes.len() / 2;
    let q = |b: &FBox| fbox_to_qbox(b, level as usize);
    let node = |children: &[i16], b: &FBox, first: usize, last: usize| {
        let mut n = vec![0u8; 0x20];
        for k in 0..8 {
            let c = children.get(k).copied().unwrap_or(-1);
            n[2 * k..2 * k + 2].copy_from_slice(&c.to_le_bytes());
        }
        for (k, v) in q(b).iter().enumerate() {
            n[0x10 + 2 * k..0x12 + 2 * k].copy_from_slice(&v.to_le_bytes());
        }
        n[0x1c..0x1e].copy_from_slice(&(first as u16).to_le_bytes());
        n[0x1e..0x20].copy_from_slice(&(last as u16).to_le_bytes());
        n
    };
    let b1 = meshes[..half].iter().map(|m| m.3).reduce(merge).unwrap_or(all);
    let b2 = meshes[half..].iter().map(|m| m.3).reduce(merge).unwrap_or(all);
    let mut nodes = node(&[1, 2], &all, 0, 0);
    nodes.extend(node(&[], &b1, 0, half));
    nodes.extend(node(&[], &b2, half, meshes.len()));
    let mut mesh_t = Vec::new();
    for m in &meshes {
        for v in q(&m.3) {
            mesh_t.extend(v.to_le_bytes());
        }
        mesh_t.extend((m.0 as u16).to_le_bytes());
        mesh_t.extend((m.1 as u16).to_le_bytes());
        mesh_t.push(r.below(2) as u8); // the stored enable byte: overwritten when the object is built
        mesh_t.push(m.2);
        mesh_t.extend(0u16.to_le_bytes());
    }
    let (mut poly_t, mut vert_t, mut plane_t, mut box_t, mut attr_t) = (Vec::new(), Vec::new(), Vec::new(), Vec::new(), Vec::new());
    for (pi, p) in polys.iter().enumerate() {
        let base = vert_t.len() / 16;
        for v in &p.v {
            for f in [v[0], v[1], v[2], 1.0f32] {
                vert_t.extend(f.to_le_bytes());
            }
        }
        let nrm = norm3(cross3(sub3(p.v[1], p.v[0]), sub3(p.v[2], p.v[0])));
        let d = -(nrm[0] * p.v[0][0] + nrm[1] * p.v[0][1] + nrm[2] * p.v[0][2]);
        for f in [nrm[0], nrm[1], nrm[2], d] {
            plane_t.extend(f.to_le_bytes());
        }
        let box_idx = if p.boxed {
            let idx = box_t.len() / 12;
            for v in q(&bounds(&p.v)) {
                box_t.extend(v.to_le_bytes());
            }
            idx as u16
        } else {
            0xffff
        };
        poly_t.push(p.material);
        poly_t.push(r.below(256) as u8);
        for k in 0..4 {
            let idx = if k < p.v.len() { (base + k) as u16 } else { 0xffff };
            poly_t.extend(idx.to_le_bytes());
        }
        poly_t.extend((pi as u16).to_le_bytes());
        poly_t.extend(box_idx.to_le_bytes());
        poly_t.extend((pi as u16).to_le_bytes());
        attr_t.extend(p.attr.to_le_bytes());
    }
    let tables = [nodes, mesh_t, poly_t, vert_t, plane_t, box_t, attr_t];
    let mut file = vec![0u8; 0x50];
    file[..4].copy_from_slice(b"COCT");
    file[0x0c..0x10].copy_from_slice(&level.to_le_bytes());
    for (k, t) in tables.iter().enumerate() {
        let at = file.len();
        file[0x18 + 8 * k..0x1c + 8 * k].copy_from_slice(&(at as u32).to_le_bytes());
        file[0x1c + 8 * k..0x20 + 8 * k].copy_from_slice(&(t.len() as u32).to_le_bytes());
        file.extend(pad(t.clone()));
    }
    file
}


// ---------------------------------------------------------------------------------------------------------
// Worlds: the map object, built by the game's own 0x142ba8
// ---------------------------------------------------------------------------------------------------------

/// A random attribute word: the kind nibble, the walker bits, the ledge bit 0x400 and the camera bits 0x60000.
fn random_attr_p22(r: &mut Rng, wall: bool) -> u32 {
    let nib = pick(r, &[0u32, 1, 2, 3, 4, 5, 6, 8, 0xa]);
    let mut a = nib;
    if wall && r.below(4) != 0 {
        a |= 0x10;
    }
    for bit in [0x10u32, 0x20, 0x40, 0x80, 0x100, 0x800, 0x1000, 0x80000, 0x200, 0x4000, 0x8000, 0x10000] {
        if r.below(5) == 0 {
            a |= bit;
        }
    }
    if r.below(3) == 0 {
        a |= 0x400;
    }
    if r.below(5) == 0 {
        a |= 0x20000;
    }
    if r.below(8) == 0 {
        a |= 0x40000;
    }
    if r.below(14) == 0 {
        a |= 0x100000;
    }
    a
}

const FILES_AT: u32 = 0x0120_0000;
const FILE_STRIDE: u32 = 0x0008_0000;
const HEAP_AT: u32 = 0x0170_0000;
const CLASS_AT: u32 = 0x0194_0000;
const VTABLE_AT: u32 = 0x0194_0100;
const CREATE_FN: u32 = 0x0194_0200;
const DESTROY_FN: u32 = 0x0194_0210;
const SEG_AT: u32 = 0x019a_0000;
const OUT_AT: u32 = 0x019b_0000;
const MAT_AT: u32 = 0x019d_0000;
/// The slot globals in the order of `MapCollision`: main, second, colour, aux, kinds.
const SLOT_GLOBALS: [u32; 5] = [0x355934, 0x355938, 0x35593c, 0x355940, 0x355944];
/// The map object words that name the five files.
const FILE_WORDS: [usize; 5] = [0x30, 0x34, 0x38, 0x3c, 0x40];
const GROUP_WORD: u64 = 0xb;

struct World {
    ee: Ee,
    coll: MapCollision,
    set_up: SetUp,
    /// The pairs the game's set-up handed to `FUN_0014ea50`.
    seen_special: Rc<RefCell<Vec<(u32, u32)>>>,
    /// Every allocation of the set-up: (size, address).
    allocs: Rc<RefCell<Vec<(u32, u32)>>>,
    /// The addresses `LoadDataFree` and the model destroy call saw, in order (the destroy call is `DESTROY_MARK`).
    freed: Rc<RefCell<Vec<u32>>>,
    /// The centres of the main map's polygons.
    centres: Vec<[f32; 3]>,
    polys: usize,
    flag: u8,
}

const DESTROY_MARK: u32 = 0xdead_0001;

type Pairs = Rc<RefCell<Vec<(u32, u32)>>>;

fn install_hooks(ee: &mut Ee) -> (Pairs, Pairs, Rc<RefCell<Vec<u32>>>) {
    let bump = Rc::new(std::cell::Cell::new(HEAP_AT));
    let allocs = Rc::new(RefCell::new(Vec::new()));
    let (b, a) = (bump.clone(), allocs.clone());
    ee.hooks.insert(0x106220, move |ee: &mut Ee| {
        let size = ee.gpr[reg::A0] as u32;
        let p = b.get();
        b.set(p + ((size + 15) & !15));
        a.borrow_mut().push((size, p));
        ee.gpr[reg::V0] = p as u128;
    });
    let freed = Rc::new(RefCell::new(Vec::new()));
    let f = freed.clone();
    ee.hooks.insert(0x106280, move |ee: &mut Ee| f.borrow_mut().push(ee.gpr[reg::A0] as u32));
    let f = freed.clone();
    ee.hooks.insert(DESTROY_FN, move |_ee: &mut Ee| f.borrow_mut().push(DESTROY_MARK));
    ee.hooks.insert(CREATE_FN, |ee: &mut Ee| ee.gpr[reg::V0] = 0x0c0d_0e0f);
    let special = Rc::new(RefCell::new(Vec::new()));
    let s = special.clone();
    ee.hooks.insert(0x14ea50, move |ee: &mut Ee| s.borrow_mut().push((ee.gpr[reg::A0] as u32, ee.gpr[reg::A0 + 1] as u32)));
    // Drawing, audio and model callees the collision half does not need: the drawn octree build and release, the
    // model instance's own set-up calls, the draw tables, the atexit registration of the static record.
    for noop in [
        0x145748u32, 0x1459d0, 0x145b40, 0x145b80, 0x1084f8, 0x11e018, 0x11e8c0, 0x11e638, 0x14e9e0, 0x14ead0, 0x15c018, 0x15c1e8, 0x1531f8, 0x153378,
        0x30fe80,
    ] {
        ee.hooks.insert(noop, |_ee: &mut Ee| {});
    }
    (special, allocs, freed)
}

/// Writes the map object and the files, runs the game's set-up `FUN_00142ba8` and the port's, and returns both.
fn world(what: &str, r: &mut Rng, present: [bool; 5]) -> Option<World> {
    let mut ee = game(what)?;
    // The identity matrix the game fills at boot (FUN_0011b460 copies it into every new object).
    for i in 0..16 {
        ee.wf32(0x354a60 + 4 * i, if i % 5 == 0 { 1.0 } else { 0.0 });
    }
    let (seen_special, allocs, freed) = install_hooks(&mut ee);
    ee.w32(CLASS_AT, VTABLE_AT);
    ee.w32(VTABLE_AT + 0x24, CREATE_FN);
    ee.w32(VTABLE_AT + 0x28, DESTROY_FN);
    let mut obj = MapObject::default();
    obj.set_word(0x24, CLASS_AT);
    obj.set_word(0x28, if r.below(2) == 0 { 0 } else { 0x0abc_0000 });
    obj.set_word(0x2c, 0x0def_0000);
    obj.set_word(0x48, 0x0111_0000);
    obj.set_word(0x4c, if r.below(2) == 0 { 0 } else { 0x0222_0000 });
    let mut files: [Option<Vec<u8>>; 5] = Default::default();
    let mut centres = Vec::new();
    let mut polys_main = 0;
    for (k, &p) in present.iter().enumerate() {
        if !p {
            continue;
        }
        let dense = k == 0 && r.below(5) == 0;
        let n = 5 + r.below(12) as usize;
        let span = urange(r, 120.0, 450.0);
        let polys: Vec<Poly> = (0..if dense { 28 } else { n })
            .map(|j| {
                let mut p = if dense { dense_wall(r, j) } else { random_poly(r, span) };
                p.attr |= random_attr_p22(r, false) & (0x400 | 0x20000 | 0x40000);
                p
            })
            .collect();
        let level = r.below(4) as u32;
        let file = build_file(r, &polys, level);
        let at = FILES_AT + FILE_STRIDE * k as u32;
        ee.write(at, &file);
        obj.set_word(FILE_WORDS[k], at);
        if k == 0 {
            polys_main = polys.len();
            centres = polys
                .iter()
                .map(|p| {
                    let b = bounds(&p.v);
                    [(b[0] + b[3]) / 2.0, (b[1] + b[4]) / 2.0, (b[2] + b[5]) / 2.0]
                })
                .collect();
        }
        files[k] = Some(file);
    }
    ee.write(OBJ, obj.bytes());
    let flag = r.below(256) as u8;
    ee.write(0x35594c, &[0xaa; 4]);
    call(&mut ee, 0x142ba8, &[OBJ as u64, 0x77, flag as u64, GROUP_WORD]);
    let mut coll = MapCollision::default();
    let refs = MapFiles {
        main: files[0].as_deref(),
        second: files[1].as_deref(),
        colour: files[2].as_deref(),
        aux: files[3].as_deref(),
        kinds: files[4].as_deref(),
    };
    let set_up = coll.set_up(&refs, GROUP_WORD, flag);
    Some(World { ee, coll, set_up, seen_special, allocs, freed, centres, polys: polys_main, flag })
}

impl World {
    fn slot(&self, k: usize) -> Option<&CollSlot> {
        [&self.coll.main, &self.coll.second, &self.coll.colour, &self.coll.aux, &self.coll.kinds][k].as_ref()
    }

    /// The guest object of slot `k` (0 when the slot is empty).
    fn guest_obj(&mut self, k: usize) -> u32 {
        self.ee.r32(SLOT_GLOBALS[k])
    }

    /// Compares the guest's slot `k` with the port's: presence, mesh enabled bytes and the nibble-5 flag.
    fn check_slot(&mut self, k: usize, what: &str) {
        let g = self.guest_obj(k);
        let Some(slot) = self.slot(k).map(|s| s.oct.clone()) else {
            assert_eq!(g, 0, "{what}: slot {k} must be empty");
            return;
        };
        assert_ne!(g, 0, "{what}: slot {k} must be built");
        let meshes = self.ee.r32(g + 8);
        let count = self.ee.r32(g + 0x24) as usize;
        assert_eq!(count, slot.mesh_count(), "{what}: slot {k} mesh count");
        for m in 0..count {
            let on = self.ee.r8(meshes + 0x14 * m as u32 + 0x10) != 0;
            assert_eq!(on, slot.mesh_enabled(m), "{what}: slot {k} mesh {m}");
        }
        assert_eq!(self.ee.r8(g + 0xd2) != 0, slot.has_nibble_five(), "{what}: slot {k} nibble 5");
    }

    fn main_obj(&mut self) -> u32 {
        self.guest_obj(0)
    }

    fn near(&self, r: &mut Rng) -> [f32; 3] {
        self.centres[r.below(self.polys as u64) as usize]
    }

    /// A random segment around a polygon of the main map, written at `SEG_AT` (start then end).
    fn segment(&mut self, r: &mut Rng) -> [V4; 2] {
        let near = self.near(r);
        let p0 = [near[0] + urange(r, -200.0, 200.0), near[1] + urange(r, -300.0, 0.0), near[2] + urange(r, -200.0, 200.0), 1.0];
        let p1 = [near[0] + urange(r, -200.0, 200.0), near[1] + urange(r, 0.0, 300.0), near[2] + urange(r, -200.0, 200.0), 1.0];
        put4(&mut self.ee, SEG_AT, p0);
        put4(&mut self.ee, SEG_AT + 16, p1);
        [p0, p1]
    }
}

fn worlds(what: &str, n: usize, present: impl Fn(&mut Rng) -> [bool; 5], mut body: impl FnMut(&mut World, &mut Rng, usize)) {
    let mut r = Rng::new(0x22_0000 ^ n as u64 ^ (what.len() as u64 * 977));
    let mut any = false;
    for w in 0..n {
        let pres = present(&mut r);
        let Some(mut world) = world(what, &mut r, pres) else { return };
        any = true;
        body(&mut world, &mut r, w);
    }
    assert!(any);
}

fn all_but_random(r: &mut Rng) -> [bool; 5] {
    [true, r.below(2) == 0, r.below(2) == 0, r.below(2) == 0, r.below(2) == 0]
}

/// orig 0x142ba8, the collision half: the flag bytes, the five slots built over the map object's files with the group
/// word, and the polygons with attribute bits 0x60000 handed to the camera. (The drawn octree, the model instance and
/// the draw tables are hooked out; the model instance is allocated by the game and has no port here.)
#[test]
fn orig_142ba8_matches_the_game() {
    let (mut specials, mut fives) = (0, 0);
    worlds("0x142ba8", 40, all_but_random, |w, _r, n| {
        for k in 0..5 {
            w.check_slot(k, &format!("world {n}"));
        }
        assert_eq!(w.ee.read::<4>(0x35594c), [w.coll.flag_4c, w.coll.flag_4d, w.coll.flag_4e, w.coll.flag_4f], "world {n}: flags");
        assert_eq!(w.coll.flag_4c, w.flag);
        assert_eq!(*w.seen_special.borrow(), w.set_up.special, "world {n}: camera polygons");
        // The allocations: the octree 0x5380, the model 0x90, one 0xe0 object per present slot.
        let sizes: Vec<u32> = w.allocs.borrow().iter().map(|a| a.0).collect();
        let present = (0..5).filter(|&k| w.slot(k).is_some()).count();
        assert_eq!(sizes.iter().filter(|&&s| s == 0xe0).count(), present, "world {n}: allocations {sizes:x?}");
        assert_eq!(&sizes[..2], &[0x5380, 0x90]);
        assert_eq!(w.ee.r32(0x355948), w.allocs.borrow()[1].1, "world {n}: the model instance");
        assert_eq!(w.ee.r32(0x355930), w.allocs.borrow()[0].1, "world {n}: the drawn octree");
        specials += w.set_up.special.len();
        fives += w.slot(0).is_some_and(|s| s.oct.has_nibble_five()) as usize;
    });
    assert!(specials > 20 && fives > 3, "{specials} camera polygons, {fives} worlds with nibble 5");
}

/// orig 0x142e68, the collision half: the five slots released (the order of the frees is the floor kind object, the
/// fourth object, the colour map, the second map, the model instance's own destroy and free, the walking map, the drawn
/// octree) and every global cleared.
#[test]
fn orig_142e68_matches_the_game() {
    worlds("0x142e68", 30, all_but_random, |w, _r, n| {
        let allocs = w.allocs.borrow().clone();
        let addr = |k: usize| {
            // The k-th present slot's allocation, in the order the set-up made them (main, second, colour, aux, kinds).
            let order: Vec<usize> = (0..5).filter(|&j| w.slot(j).is_some()).collect();
            order.iter().position(|&j| j == k).map(|i| allocs[2 + i].1)
        };
        let want: Vec<u32> = [addr(4), addr(3), addr(2), addr(1)]
            .into_iter()
            .flatten()
            .chain([DESTROY_MARK, allocs[1].1])
            .chain(addr(0))
            .chain([allocs[0].1])
            .collect();
        w.freed.borrow_mut().clear();
        call(&mut w.ee, 0x142e68, &[]);
        assert_eq!(*w.freed.borrow(), want, "world {n}: the frees");
        let model = w.coll.release();
        assert!(model.is_none(), "the set-up half leaves the model instance to the caller");
        for (k, g) in SLOT_GLOBALS.iter().enumerate() {
            assert_eq!(w.ee.r32(*g), 0, "world {n}: slot {k}");
            assert!(w.slot(k).is_none());
        }
        assert_eq!((w.ee.r32(0x355948), w.ee.r32(0x355930)), (0, 0));
    });
}

/// orig 0x142f78 and 0x143028: one mesh group off or on in every slot (the drawn group calls are hooked out).
fn group_op(addr: u32, on: bool) {
    let mut switched = 0;
    worlds(&format!("{addr:#x}"), 30, all_but_random, |w, r, n| {
        for round in 0..6 {
            let g = if r.below(3) == 0 { pick(r, &[62u8, 63, 64, 70, 200, 255]) } else { r.below(6) as u8 };
            call(&mut w.ee, addr, &[g as u64]);
            if on {
                w.coll.group_on(g);
            } else {
                w.coll.group_off(g);
            }
            for k in 0..5 {
                w.check_slot(k, &format!("world {n} round {round} group {g}"));
            }
            switched += 1;
        }
    });
    assert!(switched > 100 || kh2_factory::exe().is_none());
}

#[test]
fn orig_143028_matches_the_game() {
    group_op(0x143028, true);
}

#[test]
fn orig_142f78_matches_the_game() {
    group_op(0x142f78, false);
}

/// orig 0x141898: the hit polygon table address store (`oct +0x3c`).
#[test]
fn orig_141898_matches_the_game() {
    static SPEC: Spec = Spec { addr: 0x141898, args: &[Arg::Ptr(Block { size: 0xe0, fields: &[], subs: &[] }), Arg::Int(&[0])], floats: 0, ret: Ret::None, globals: &[], stubs: &[], nulls: false };
    let Some(st) = fuzz(&SPEC, 2000, |c| {
        let addr = c.int(1) as u32;
        set_hit_table_address(&mut c.blocks[0], addr);
        Out::none()
    }) else { return };
    eprintln!("{} cases equal, {} interpreter steps in {:.3} s", st.cases, st.steps, st.secs);
}

/// orig 0x142120, 0x143dc0: the polygon vertex record of the static buffer (four vertex pointers and the count); 0x143dc0
/// is the same on the walking map slot.
#[test]
fn orig_142120_matches_the_game() {
    polygon_record(0x142120, true);
}

#[test]
fn orig_143dc0_matches_the_game() {
    polygon_record(0x143dc0, false);
}

fn polygon_record(addr: u32, direct: bool) {
    worlds(&format!("{addr:#x}"), 25, |_| [true; 5], |w, r, n| {
        let obj = w.main_obj();
        for _ in 0..12 {
            let p = r.below(w.polys as u64) as u16;
            w.ee.write(0x484360, &[0xeeu8; 20]);
            let ret = if direct { call(&mut w.ee, addr, &[obj as u64, p as u64]) as u32 } else { call(&mut w.ee, addr, &[p as u64]); 0x484360 };
            if direct {
                assert_eq!(ret, 0x484360, "world {n}: the static record");
            }
            let v = polygon_vertices(&w.coll.main.as_ref().unwrap().oct, p);
            assert_eq!(w.ee.r32(0x484360 + 0x10) as usize, v.n, "world {n} poly {p}: count");
            for k in 0..v.n {
                let ptr = w.ee.r32(0x484360 + 4 * k as u32);
                let g = get4(&mut w.ee, ptr);
                assert!(same4(g, v.v[k], 4), "world {n} poly {p} vertex {k}");
            }
        }
    });
}

/// orig 0x143de0: the polygon plane of the walking map.
#[test]
fn orig_143de0_matches_the_game() {
    worlds("0x143de0", 25, |_| [true; 5], |w, r, n| {
        for _ in 0..12 {
            let p = r.below(w.polys as u64) as u16;
            let ptr = call(&mut w.ee, 0x143de0, &[p as u64]) as u32;
            let g = get4(&mut w.ee, ptr);
            assert!(same4(g, w.coll.polygon_plane(p).unwrap(), 4), "world {n} poly {p}");
        }
    });
}

/// orig 0x143e68: the polygon vertices of the walking map in world space.
#[test]
fn orig_143e68_matches_the_game() {
    worlds("0x143e68", 25, |_| [true; 5], |w, r, n| {
        for _ in 0..12 {
            let p = if r.below(8) == 0 { 0xffff } else { r.below(w.polys as u64) as u16 };
            let count = call(&mut w.ee, 0x143e68, &[p as u64, OUT_AT as u64]) as usize;
            let mine = w.coll.polygon_verts_world(p).unwrap();
            assert_eq!(count, mine.len(), "world {n} poly {p}");
            for (k, v) in mine.iter().enumerate() {
                let g = get4(&mut w.ee, OUT_AT + 16 * k as u32);
                assert!(same4(g, *v, 4), "world {n} poly {p} vertex {k}");
            }
        }
    });
}

/// orig 0x143d68 and 0x143e00: the polygon normal and attribute of the walking map, or of the second map collision when
/// asked for (and filled).
#[test]
fn orig_143d68_matches_the_game() {
    let mut second = 0;
    worlds("0x143d68", 30, |r| [true, r.below(2) == 0, false, false, false], |w, r, n| {
        for _ in 0..12 {
            let flag = r.below(2) == 0;
            let have_second = w.coll.second.is_some();
            let cnt_main = w.polys;
            // A polygon index valid in the object the call will use (a wrong one reads past the tables).
            let use_second = flag && have_second;
            let limit = if use_second { poly_count(w, 1) } else { cnt_main as u64 };
            let p = r.below(limit) as u16;
            call(&mut w.ee, 0x143d68, &[OUT_AT as u64, p as u64, flag as u64]);
            let g = get4(&mut w.ee, OUT_AT);
            assert!(same4(g, w.coll.normal(p, flag).unwrap(), 4), "world {n} poly {p} flag {flag}");
            second += use_second as usize;
        }
    });
    assert!(second > 10 || kh2_factory::exe().is_none(), "{second}");
}

fn poly_count(w: &mut World, k: usize) -> u64 {
    let g = w.guest_obj(k);
    w.ee.r32(g + 0x28) as u64
}

#[test]
fn orig_143e00_matches_the_game() {
    let mut second = 0;
    worlds("0x143e00", 30, |r| [true, r.below(2) == 0, false, false, false], |w, r, n| {
        for _ in 0..12 {
            let flag = r.below(2) == 0;
            let use_second = flag && w.coll.second.is_some();
            let limit = if use_second { poly_count(w, 1) } else { w.polys as u64 };
            let p = r.below(limit) as u16;
            let ptr = call(&mut w.ee, 0x143e00, &[p as u64, flag as u64]) as u32;
            assert_eq!(w.ee.r32(ptr), w.coll.attr(p, flag).unwrap(), "world {n} poly {p} flag {flag}");
            second += use_second as usize;
        }
    });
    assert!(second > 10 || kh2_factory::exe().is_none(), "{second}");
}

/// orig 0x1439f8 and 0x1439d8: the segment cast against the walking map with the empty filter; 0x1439d8 keeps the
/// polygon to itself.
#[test]
fn orig_1439f8_matches_the_game() {
    ray_op(0x1439f8);
}

#[test]
fn orig_1439d8_matches_the_game() {
    ray_op(0x1439d8);
}

fn ray_op(addr: u32) {
    let mut hits = 0;
    worlds(&format!("{addr:#x}"), 30, |_| [true; 5], |w, r, n| {
        let obj = w.main_obj();
        for _ in 0..30 {
            let seg = w.segment(r);
            let kind = r.below(16) as u32;
            w.ee.w32(obj + 0x3c, OUT_AT + 0x800);
            put4(&mut w.ee, OUT_AT, [7.0, 8.0, 9.0, 10.0]);
            w.ee.write(OUT_AT + 0x40, &0x5555u16.to_le_bytes());
            let args: Vec<u64> = if addr == 0x1439f8 {
                vec![SEG_AT as u64, OUT_AT as u64, (OUT_AT + 0x40) as u64, kind as u64]
            } else {
                vec![SEG_AT as u64, OUT_AT as u64, kind as u64]
            };
            let ret = call(&mut w.ee, addr, &args);
            let (hit, poly) = w.coll.ray(&seg, kind).unwrap();
            assert_eq!(ret & 1, hit.is_some() as u64, "world {n}: return");
            let g = get4(&mut w.ee, OUT_AT);
            match hit {
                Some(h) => {
                    assert!(same4(g, h, 4), "world {n}: hit {g:?} vs {h:?}");
                    if addr == 0x1439f8 {
                        assert_eq!(u16::from_le_bytes(w.ee.read::<2>(OUT_AT + 0x40)), poly, "world {n}: polygon");
                    }
                    hits += 1;
                }
                None => assert!(same4(g, [7.0, 8.0, 9.0, 10.0], 4), "world {n}: untouched on a miss"),
            }
            assert_eq!(w.coll.ray_hit(&seg, kind).unwrap(), hit);
        }
    });
    assert!(hits > 100 || kh2_factory::exe().is_none(), "{hits}");
}

/// orig 0x143360 (and 0x141690): is the polygon in a switched-on mesh? Run after random group edits.
#[test]
fn orig_143360_matches_the_game() {
    let (mut yes, mut no) = (0, 0);
    worlds("0x143360", 30, |_| [true; 5], |w, r, n| {
        for _ in 0..8 {
            let g = r.below(5) as u8;
            let on = r.below(2) == 0;
            call(&mut w.ee, if on { 0x143028 } else { 0x142f78 }, &[g as u64]);
            if on {
                w.coll.group_on(g);
            } else {
                w.coll.group_off(g);
            }
            for _ in 0..10 {
                let p = if r.below(8) == 0 { 0xffff } else { r.below(w.polys as u64 + 3) as u16 };
                let got = call(&mut w.ee, 0x143360, &[p as u64]) & 1 != 0;
                assert_eq!(got, w.coll.polygon_on(p), "world {n} poly {p}");
                if got {
                    yes += 1;
                } else {
                    no += 1;
                }
            }
        }
    });
    assert!((yes > 100 && no > 100) || kh2_factory::exe().is_none(), "{yes} / {no}");
}

/// orig 0x142818: the box of the root node as floats.
#[test]
fn orig_142818_matches_the_game() {
    worlds("0x142818", 40, |_| [true; 5], |w, r, n| {
        let k = r.below(5) as usize;
        let obj = w.guest_obj(k);
        put_f_guest(&mut w.ee, OUT_AT, &[9.0; 6]);
        call(&mut w.ee, 0x142818, &[obj as u64, 0, OUT_AT as u64]);
        let got: FBox = std::array::from_fn(|i| w.ee.f32_at(OUT_AT + 4 * i as u32));
        assert_eq!(got, w.slot(k).unwrap().root_box(), "world {n} slot {k}");
    });
}

fn put_f_guest(ee: &mut Ee, at: u32, v: &[f32]) {
    for (k, f) in v.iter().enumerate() {
        ee.wf32(at + 4 * k as u32, *f);
    }
}

/// orig 0x142b10: the walk over the polygons with attribute bits 0x60000 (the camera registration is recorded).
#[test]
fn orig_142b10_matches_the_game() {
    let mut total = 0;
    worlds("0x142b10", 40, |_| [true; 5], |w, r, n| {
        let k = r.below(5) as usize;
        let obj = w.guest_obj(k);
        w.seen_special.borrow_mut().clear();
        call(&mut w.ee, 0x142b10, &[obj as u64]);
        let mine = w.slot(k).unwrap().special_polygons();
        assert_eq!(*w.seen_special.borrow(), mine, "world {n} slot {k}");
        total += mine.len();
    });
    assert!(total > 20 || kh2_factory::exe().is_none(), "{total}");
}

/// orig 0x143778, the collision side: the colour map's class byte under a segment. The colour table and lighting calls
/// (`FUN_0015c690`, `FUN_00153540`) are replaced by a recorder of the class the table is asked for.
#[test]
fn orig_143778_matches_the_game() {
    let (mut with_class, mut without) = (0, 0);
    worlds("0x143778", 40, |r| [true, false, r.below(3) != 0, false, false], |w, r, n| {
        let seen = Rc::new(RefCell::new(Vec::new()));
        let s = seen.clone();
        w.ee.hooks.insert(0x15c690, move |ee: &mut Ee| s.borrow_mut().push(ee.gpr[reg::A0 + 1] as u32 & 0xff));
        for _ in 0..30 {
            let seg = w.segment(r);
            seen.borrow_mut().clear();
            call(&mut w.ee, 0x143778, &[OUT_AT as u64, SEG_AT as u64]);
            let mut want = vec![0u32];
            if let Some(c) = w.coll.floor_colour_class(&seg) {
                want.push(c as u32);
                with_class += 1;
            } else {
                without += 1;
            }
            assert_eq!(*seen.borrow(), want, "world {n}");
        }
    });
    assert!((with_class > 50 && without > 50) || kh2_factory::exe().is_none(), "{with_class} / {without}");
}

/// orig 0x143438: the floor facts under a point for the footsteps, with the colour map and kind slots filled or empty
/// (the colour table words at `+0x20` are drawing and are stubbed; nothing past `+0x34` may change).
#[test]
fn orig_143438_matches_the_game() {
    let (mut floors, mut kinds, mut colours) = (0, 0, 0);
    worlds("0x143438", 40, |r| [true, false, r.below(2) == 0, false, r.below(2) == 0], |w, r, n| {
        w.ee.hooks.insert(0x15c690, |_ee: &mut Ee| {});
        w.ee.hooks.insert(0x153540, |_ee: &mut Ee| {});
        for _ in 0..25 {
            let near = w.near(r);
            let p = [near[0] + urange(r, -150.0, 150.0), near[1] + urange(r, -120.0, 40.0), near[2] + urange(r, -150.0, 150.0), 1.0];
            put4(&mut w.ee, SEG_AT, p);
            w.ee.write(OUT_AT, &[0xccu8; 0x40]);
            call(&mut w.ee, 0x143438, &[SEG_AT as u64, OUT_AT as u64]);
            let game: Vec<u8> = w.ee.read::<0x40>(OUT_AT).to_vec();
            let info = w.coll.footstep_info(p).unwrap();
            let f = |o: usize| f32::from_le_bytes(game[o..o + 4].try_into().unwrap());
            let u = |o: usize| u32::from_le_bytes(game[o..o + 4].try_into().unwrap());
            for k in 0..3 {
                assert!(feq(f(4 * k), info.normal[k]), "world {n}: normal lane {k}: {} vs {}", f(4 * k), info.normal[k]);
                assert!(feq(f(0x10 + 4 * k), info.opposite[k]), "world {n}: opposite lane {k}");
            }
            assert_eq!(u(0x24), info.poly, "world {n}: polygon");
            assert_eq!(u(0x28), info.material, "world {n}: material");
            assert_eq!(u(0x2c), info.colour_byte, "world {n}: colour byte");
            assert_eq!(u(0x30), info.kind, "world {n}: kind");
            assert_eq!(game[0x34], info.flag, "world {n}: flag");
            assert!(game[0x35..0x40].iter().all(|&b| b == 0xcc), "world {n}: the original wrote past +0x34");
            // The query builds its collider with "no floor polygon", so the normal is always straight up.
            assert_eq!((info.poly, info.normal), (0xffff, [0.0, -1.0, 0.0, 1.0]), "world {n}");
            floors += 1;
            kinds += (info.kind != 0) as usize;
            colours += (info.colour_byte != 0) as usize;
        }
    });
    assert!((floors > 30 && kinds > 5 && colours > 5) || kh2_factory::exe().is_none(), "{floors} queries, {kinds} kinds, {colours} colour bytes");
}

fn random_placement(r: &mut Rng) -> M4 {
    let yaw = urange(r, 0.0, std::f32::consts::TAU);
    let (sy, cy) = yaw.sin_cos();
    let s = pick(r, &[1.0f32, 1.0, 0.8, 1.3]);
    let mut m = [[0.0; 4]; 4];
    m[0] = [cy * s, 0.0, -sy * s, 0.0];
    m[1] = [0.0, s, 0.0, 0.0];
    m[2] = [sy * s, 0.0, cy * s, 0.0];
    m[3] = [urange(r, -300.0, 300.0), urange(r, -40.0, 40.0), urange(r, -300.0, 300.0), 1.0];
    m
}

fn put_m4(ee: &mut Ee, at: u32, m: &M4) {
    for (i, row) in m.iter().enumerate() {
        put4(ee, at + 16 * i as u32, *row);
    }
}

/// Runs the sphere pass of the game (`0x164ce8` directly on the object in `a0`, or the wrapper `0x143388` on the walking
/// map) against the port on `oct`.
fn sphere_cases(addr: u32, placed: bool) {
    let (mut found, mut miss) = (0, 0);
    worlds(&format!("{addr:#x}"), 30, |_| [true; 5], |w, r, n| {
        if placed {
            let m = random_placement(r);
            put_m4(&mut w.ee, MAT_AT, &m);
            let obj = w.main_obj();
            call(&mut w.ee, 0x142858, &[obj as u64, MAT_AT as u64]);
            w.coll.main.as_mut().unwrap().oct.place(&m);
            for c in w.centres.iter_mut() {
                let t = xf(&m, [c[0], c[1], c[2], 1.0]);
                *c = [t[0], t[1], t[2]];
            }
        }
        let obj = w.main_obj();
        for _ in 0..30 {
            let near = w.near(r);
            let point = [near[0] + urange(r, -120.0, 120.0), near[1] + urange(r, -150.0, 30.0), near[2] + urange(r, -120.0, 120.0), 1.0];
            let half = urange(r, 40.0, 260.0);
            let bounds = fbox(point[0] - half, point[1] - half, point[2] - half, point[0] + half, point[1] + half, point[2] + half);
            let dir = [urange(r, -1.0, 1.0), urange(r, -1.0, 1.0), urange(r, -1.0, 1.0), 1.0];
            put4(&mut w.ee, SEG_AT, point);
            put_f_guest(&mut w.ee, SEG_AT + 0x20, &bounds);
            put4(&mut w.ee, SEG_AT + 0x40, dir);
            let sentinel = [[1.0, 2.0, 3.0, 4.0], [5.0, 6.0, 7.0, 8.0], [9.0, 10.0, 11.0, 12.0]];
            for (k, s) in sentinel.iter().enumerate() {
                put4(&mut w.ee, OUT_AT + 0x20 * k as u32, *s);
            }
            w.ee.write(OUT_AT + 0x80, &0x5555u16.to_le_bytes());
            let outs = [OUT_AT as u64, (OUT_AT + 0x20) as u64, (OUT_AT + 0x40) as u64];
            let ret = if addr == 0x164ce8 {
                call(&mut w.ee, addr, &[obj as u64, SEG_AT as u64, (SEG_AT + 0x20) as u64, outs[0], outs[1], outs[2], (SEG_AT + 0x40) as u64, (OUT_AT + 0x80) as u64])
            } else {
                call(&mut w.ee, addr, &[SEG_AT as u64, (SEG_AT + 0x20) as u64, outs[0], outs[1], outs[2], (SEG_AT + 0x40) as u64, (OUT_AT + 0x80) as u64])
            };
            let mine = if addr == 0x164ce8 {
                w.coll.main.as_ref().unwrap().oct.ledge_edge_search(point, &bounds, dir)
            } else {
                w.coll.sphere_pass(point, &bounds, dir)
            };
            assert_eq!(ret as u32, mine.is_some() as u32, "world {n}: return");
            let g: Vec<V4> = (0..3).map(|k| get4(&mut w.ee, OUT_AT + 0x20 * k)).collect();
            let poly = u16::from_le_bytes(w.ee.read::<2>(OUT_AT + 0x80));
            match mine {
                Some(h) => {
                    assert!(same4(g[0], h.foot, 4), "world {n}: foot {:?} vs {:?}", g[0], h.foot);
                    assert!(same4(g[1], h.edge_a, 4), "world {n}: edge a {:?} vs {:?}", g[1], h.edge_a);
                    assert!(same4(g[2], h.edge_b, 4), "world {n}: edge b {:?} vs {:?}", g[2], h.edge_b);
                    assert_eq!(poly, h.poly, "world {n}: polygon");
                    found += 1;
                }
                None => {
                    for k in 0..3 {
                        assert!(same4(g[k as usize], sentinel[k as usize], 4), "world {n}: output {k} untouched on a miss");
                    }
                    assert_eq!(poly, 0x5555);
                    miss += 1;
                }
            }
        }
    });
    assert!((found > 40 && miss > 40) || kh2_factory::exe().is_none(), "{found} found / {miss} missed");
}

/// orig 0x164ce8: the ledge edge search, on the map with the identity placement and on a placed map object.
#[test]
fn orig_164ce8_matches_the_game() {
    sphere_cases(0x164ce8, false);
    sphere_cases(0x164ce8, true);
}

/// orig 0x143388: the sphere pass on the walking map slot.
#[test]
fn orig_143388_matches_the_game() {
    sphere_cases(0x143388, false);
}
