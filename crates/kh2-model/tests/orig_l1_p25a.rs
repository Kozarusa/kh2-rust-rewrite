//! Oracle tests of the view, projection and frustum rules (WP-L1-P25a, `kh2_model::draw`): kh2-ee runs the original
//! from the user's own executable (VU0 macro instructions, the clip flags and the FPU included) and the port runs on
//! the same inputs.
//!
//! Two kinds of test. The plain ones are factory fuzz tests (`fuzz(&SPEC, n, ..)`): the harness draws the inputs and
//! compares the return value and every block byte by byte (a block with no typed fields is compared exactly, so
//! plane and matrix outputs are compared bit for bit). Random inputs seldom make a box lie wholly inside a view
//! volume, so the clip and plane tests also run over structured cases: a perspective clip matrix and boxes placed
//! inside, outside and across it, and planes with the origin on their negative side. Nothing of the original is
//! stubbed. Skips without a disc (KH2_ISO or local/iso.txt); fails without one when KH2_REQUIRE_DISC=1.

use kh2_ee::Ee;
use kh2_factory::fuzz::*;
use kh2_model::draw::frustum::*;
use kh2_model::draw::*;

const F4: Block = Block { size: 16, fields: &[(0, 16, Kind::Float)], subs: &[] };
const M64: Block = Block { size: 64, fields: &[(0, 64, Kind::Float)], subs: &[] };
const BOX: Block = Block { size: 24, fields: &[(0, 24, Kind::Float)], subs: &[] };
const PLANES: Block = Block { size: 80, fields: &[(0, 80, Kind::Float)], subs: &[] };
const POINTS8: Block = Block { size: 128, fields: &[(0, 128, Kind::Float)], subs: &[] };
const CORNERS4: Block = Block { size: 64, fields: &[(0, 64, Kind::Float)], subs: &[] };
/// An output block compared as exact bytes.
const RAW16: Block = Block { size: 16, fields: &[], subs: &[] };
const RAW80: Block = Block { size: 80, fields: &[], subs: &[] };
const RAW64: Block = Block { size: 64, fields: &[], subs: &[] };
/// The projection block: only the words the function touches are typed floats.
const PROJ: Block = Block { size: 0x54, fields: &[(0x40, 12, Kind::Float), (0x4c, 8, Kind::Float)], subs: &[] };
/// The camera block (0x90 bytes) compared exactly; the setters write floats into random bytes.
const CAMERA: Block = Block { size: 0x90, fields: &[], subs: &[] };

fn f4(b: &[u8], o: usize) -> V4 {
    std::array::from_fn(|k| f32::from_le_bytes(b[o + 4 * k..o + 4 * k + 4].try_into().unwrap()))
}

fn m4(b: &[u8]) -> M4 {
    std::array::from_fn(|i| f4(b, 16 * i))
}

fn fb(b: &[u8]) -> FBox {
    std::array::from_fn(|k| f32::from_le_bytes(b[4 * k..4 * k + 4].try_into().unwrap()))
}

fn pts8(b: &[u8]) -> [V4; 8] {
    std::array::from_fn(|i| f4(b, 16 * i))
}

fn planes5(b: &[u8]) -> [Plane; 5] {
    std::array::from_fn(|i| f4(b, 16 * i))
}

fn put(dst: &mut [u8], at: usize, vals: &[f32]) {
    for (k, f) in vals.iter().enumerate() {
        dst[at + 4 * k..at + 4 * k + 4].copy_from_slice(&f.to_le_bytes());
    }
}

fn put_v(dst: &mut [u8], at: usize, v: &[V4]) {
    for (i, r) in v.iter().enumerate() {
        put(dst, at + 16 * i, r);
    }
}

fn report(st: Stats) {
    eprintln!("{} cases equal, {} interpreter steps in {:.3} s", st.cases, st.steps, st.secs);
}

// ---------------------------------------------------------------------------------------------------------------------
// c1: view and projection
// ---------------------------------------------------------------------------------------------------------------------

/// orig 0x106868: the view index word is returned as it is.
#[test]
fn orig_106868_matches_the_game() {
    static SPEC: Spec = Spec { addr: 0x106868, args: &[], floats: 0, ret: Ret::Int, globals: &[(0x349e40, 4, true)], stubs: &[], nulls: false };
    let Some(st) = fuzz(&SPEC, 1500, |c| Out::int(view_index(u32::from_le_bytes(c.globals[0][..4].try_into().unwrap())) as i32 as i64)) else { return };
    report(st);
}

/// orig 0x106878: the projection block of the view: the selector's low byte and the override byte pick the array.
#[test]
fn orig_106878_matches_the_game() {
    static SPEC: Spec = Spec { addr: 0x106878, args: &[Arg::Int(&[0, 1, 0x100, 0x1ff])], floats: 0, ret: Ret::Int, globals: &[(0x349e40, 4, true), (0x349e44, 1, true)], stubs: &[], nulls: false };
    let Some(st) = fuzz(&SPEC, 3000, |c| {
        let index = u32::from_le_bytes(c.globals[0][..4].try_into().unwrap());
        Out::int(projection_address(c.int(0) as u32, c.globals[1][0], index) as i32 as i64)
    }) else { return };
    report(st);
}

/// orig 0x1068c0: the camera block of the view, chosen like the projection block.
#[test]
fn orig_1068c0_matches_the_game() {
    static SPEC: Spec = Spec { addr: 0x1068c0, args: &[Arg::Int(&[0, 1, 0x100, 0x1ff])], floats: 0, ret: Ret::Int, globals: &[(0x349e40, 4, true), (0x349e44, 1, true)], stubs: &[], nulls: false };
    let Some(st) = fuzz(&SPEC, 3000, |c| {
        let index = u32::from_le_bytes(c.globals[0][..4].try_into().unwrap());
        Out::int(camera_address(c.int(0) as u32, c.globals[1][0], index) as i32 as i64)
    }) else { return };
    report(st);
}

/// orig 0x106918 and 0x106928: the projection constructor and destructor do nothing (the block they are given stays as it is).
#[test]
fn orig_106918_matches_the_game() {
    static SPEC: Spec = Spec { addr: 0x106918, args: &[Arg::Ptr(RAW16)], floats: 0, ret: Ret::None, globals: &[], stubs: &[], nulls: false };
    let Some(st) = fuzz(&SPEC, 100, |_| {
        projection_construct();
        Out::none()
    }) else { return };
    report(st);
}

#[test]
fn orig_106928_matches_the_game() {
    static SPEC: Spec = Spec { addr: 0x106928, args: &[Arg::Ptr(RAW16)], floats: 0, ret: Ret::None, globals: &[], stubs: &[], nulls: false };
    let Some(st) = fuzz(&SPEC, 100, |_| {
        projection_destruct();
        Out::none()
    }) else { return };
    report(st);
}

/// orig 0x106930 and 0x106940: the cull matrix and the view-clip-GS matrix of a projection block are at `+0x80` and
/// `+0xc0` (the address is returned, nothing is read).
#[test]
fn orig_106930_matches_the_game() {
    static SPEC: Spec = Spec { addr: 0x106930, args: &[Arg::Int(&[0x3b21f0, 0x520000])], floats: 0, ret: Ret::Int, globals: &[], stubs: &[], nulls: false };
    let Some(st) = fuzz(&SPEC, 1500, |c| Out::int(projection_cull_matrix(c.int(0) as u32) as i32 as i64)) else { return };
    report(st);
}

#[test]
fn orig_106940_matches_the_game() {
    static SPEC: Spec = Spec { addr: 0x106940, args: &[Arg::Int(&[0x3b21f0, 0x520000])], floats: 0, ret: Ret::Int, globals: &[], stubs: &[], nulls: false };
    let Some(st) = fuzz(&SPEC, 1500, |c| Out::int(projection_view_clip_gs_matrix(c.int(0) as u32) as i32 as i64)) else { return };
    report(st);
}

/// orig 0x106e48: focal length and the two fields of view of a projection block. The original's `atan` (0x2ffd38) runs;
/// float fields are compared within 1e-5 relative by the harness.
#[test]
fn orig_106e48_matches_the_game() {
    static SPEC: Spec = Spec { addr: 0x106e48, args: &[Arg::Ptr(PROJ)], floats: 1, ret: Ret::None, globals: &[], stubs: &[], nulls: false };
    let Some(st) = fuzz(&SPEC, 4000, |c| {
        let focal = c.float(0);
        set_field_of_view(&mut c.blocks[0], focal);
        Out::none()
    }) else { return };
    report(st);
}

/// The same with the values the game uses (focal 512, scales 0.8 and 0.8666667) and sweeps around them.
#[test]
fn orig_106e48_matches_the_game_on_game_scales() {
    let Some(elf) = kh2_factory::exe_or_skip("0x106e48 game scales") else { return };
    let mut ee = Ee::new();
    ee.load_elf(&elf).expect("elf");
    let mut r = Rng::new(0x106e48);
    for n in 0..400 {
        let focal = [512.0f32, 256.0, 1024.0, 0.5, 4.0][r.below(5) as usize] * (0.5 + r.below(100) as f32 / 100.0);
        let sx = f32::from_bits(0x3f4c_cccc) * (0.5 + r.below(100) as f32 / 100.0);
        let sy = f32::from_bits(0x3f5d_dddd) * (0.5 + r.below(100) as f32 / 100.0);
        let mut b = vec![0u8; 0x54];
        put(&mut b, 0x4c, &[sx, sy]);
        ee.write(0x0180_0000, &b);
        ee.call(0x106e48, &[0x0180_0000], &[focal], STACK).expect("call");
        set_field_of_view(&mut b, focal);
        let got: [u8; 0x54] = ee.read(0x0180_0000);
        for o in [0x40usize, 0x44, 0x48] {
            let (a, p) = (f32::from_le_bytes(got[o..o + 4].try_into().unwrap()), f32::from_le_bytes(b[o..o + 4].try_into().unwrap()));
            assert!((a - p).abs() <= 1e-5 * (1.0 + a.abs()), "case {n} +{o:#x}: game {a} port {p}");
        }
    }
}

fn camera_setter(addr: u32, f: fn(&mut [u8], f32, f32, f32)) {
    let spec = Spec { addr, args: &[Arg::Ptr(CAMERA)], floats: 3, ret: Ret::None, globals: &[], stubs: &[], nulls: false };
    let Some(st) = fuzz(&spec, 2000, |c| {
        let (x, y, z) = (c.float(0), c.float(1), c.float(2));
        f(&mut c.blocks[0], x, y, z);
        Out::none()
    }) else { return };
    report(st);
}

/// orig 0x107190, 0x1071a0, 0x1071b0: camera position, reference and up vector.
#[test]
fn orig_107190_matches_the_game() {
    camera_setter(0x107190, camera_set_position);
}

#[test]
fn orig_1071a0_matches_the_game() {
    camera_setter(0x1071a0, camera_set_reference);
}

#[test]
fn orig_1071b0_matches_the_game() {
    camera_setter(0x1071b0, camera_set_up_vector);
}

/// orig 0x1071c0: camera roll.
#[test]
fn orig_1071c0_matches_the_game() {
    static SPEC: Spec = Spec { addr: 0x1071c0, args: &[Arg::Ptr(CAMERA)], floats: 1, ret: Ret::None, globals: &[], stubs: &[], nulls: false };
    let Some(st) = fuzz(&SPEC, 1500, |c| {
        let r = c.float(0);
        camera_set_roll(&mut c.blocks[0], r);
        Out::none()
    }) else { return };
    report(st);
}

/// orig 0x106f28 and 0x106f48: the camera constructor and destructor store the vtable word.
#[test]
fn orig_106f28_matches_the_game() {
    static SPEC: Spec = Spec { addr: 0x106f28, args: &[Arg::Ptr(CAMERA)], floats: 0, ret: Ret::None, globals: &[], stubs: &[], nulls: false };
    let Some(st) = fuzz(&SPEC, 500, |c| {
        camera_construct(&mut c.blocks[0]);
        Out::none()
    }) else { return };
    report(st);
}

#[test]
fn orig_106f48_matches_the_game() {
    static SPEC: Spec = Spec { addr: 0x106f48, args: &[Arg::Ptr(CAMERA)], floats: 0, ret: Ret::None, globals: &[], stubs: &[], nulls: false };
    let Some(st) = fuzz(&SPEC, 500, |c| {
        camera_destruct(&mut c.blocks[0]);
        Out::none()
    }) else { return };
    report(st);
}

/// orig 0x1070e8: position, reference and up vector from three vectors.
#[test]
fn orig_1070e8_matches_the_game() {
    static SPEC: Spec = Spec { addr: 0x1070e8, args: &[Arg::Ptr(CAMERA), Arg::Ptr(F4), Arg::Ptr(F4), Arg::Ptr(F4)], floats: 0, ret: Ret::None, globals: &[], stubs: &[], nulls: false };
    let Some(st) = fuzz(&SPEC, 2000, |c| {
        let (a, b, d): ([u8; 16], [u8; 16], [u8; 16]) = (c.blocks[1][..16].try_into().unwrap(), c.blocks[2][..16].try_into().unwrap(), c.blocks[3][..16].try_into().unwrap());
        camera_set_three(&mut c.blocks[0], &a, &b, &d);
        Out::none()
    }) else { return };
    report(st);
}

/// orig 0x107130: position and reference from two vectors.
#[test]
fn orig_107130_matches_the_game() {
    static SPEC: Spec = Spec { addr: 0x107130, args: &[Arg::Ptr(CAMERA), Arg::Ptr(F4), Arg::Ptr(F4)], floats: 0, ret: Ret::None, globals: &[], stubs: &[], nulls: false };
    let Some(st) = fuzz(&SPEC, 2000, |c| {
        let (a, b): ([u8; 16], [u8; 16]) = (c.blocks[1][..16].try_into().unwrap(), c.blocks[2][..16].try_into().unwrap());
        camera_set_two(&mut c.blocks[0], &a, &b);
        Out::none()
    }) else { return };
    report(st);
}

// ---------------------------------------------------------------------------------------------------------------------
// c2: frustum, plane and box tests
// ---------------------------------------------------------------------------------------------------------------------

/// orig 0x146598: the 64-byte matrix copy.
#[test]
fn orig_146598_matches_the_game() {
    static SPEC: Spec = Spec { addr: 0x146598, args: &[Arg::Ptr(RAW64), Arg::Ptr(M64)], floats: 0, ret: Ret::None, globals: &[], stubs: &[], nulls: false };
    let Some(st) = fuzz(&SPEC, 2000, |c| {
        let m = matrix_copy(&m4(&c.blocks[1]));
        put_v(&mut c.blocks[0], 0, &m);
        Out::none()
    }) else { return };
    report(st);
}

/// orig 0x1465c8: clip-code test of eight points (the harness's random inputs).
#[test]
fn orig_1465c8_matches_the_game() {
    static SPEC: Spec = Spec { addr: 0x1465c8, args: &[Arg::Ptr(M64), Arg::Ptr(POINTS8)], floats: 0, ret: Ret::Int, globals: &[], stubs: &[], nulls: false };
    let mut seen = [0u32; 3];
    let Some(st) = fuzz(&SPEC, 6000, |c| {
        let r = clip_points(&m4(&c.blocks[0]), &pts8(&c.blocks[1]));
        seen[r as usize] += 1;
        Out::int(r as i64)
    }) else { return };
    eprintln!("outcomes outside/inside/crossing: {seen:?}");
    report(st);
}

/// orig 0x146648: clip-code test of a box (it builds the eight corners with 0x144a50 and calls 0x1465c8).
#[test]
fn orig_146648_matches_the_game() {
    static SPEC: Spec = Spec { addr: 0x146648, args: &[Arg::Ptr(M64), Arg::Ptr(BOX)], floats: 0, ret: Ret::Int, globals: &[], stubs: &[], nulls: false };
    let Some(st) = fuzz(&SPEC, 6000, |c| Out::int(clip_box(&m4(&c.blocks[0]), &fb(&c.blocks[1])) as i64)) else { return };
    report(st);
}

/// orig 0x163070: the plane through three points (output compared bit for bit).
#[test]
fn orig_163070_matches_the_game() {
    static SPEC: Spec = Spec { addr: 0x163070, args: &[Arg::Ptr(RAW16), Arg::Ptr(F4), Arg::Ptr(F4), Arg::Ptr(F4)], floats: 0, ret: Ret::None, globals: &[], stubs: &[], nulls: false };
    let Some(st) = fuzz(&SPEC, 6000, |c| {
        let p = plane_from_points(f4(&c.blocks[1], 0), f4(&c.blocks[2], 0), f4(&c.blocks[3], 0));
        put(&mut c.blocks[0], 0, &p);
        Out::none()
    }) else { return };
    report(st);
}

/// orig 0x1469c8: the five planes of an occluder volume (output compared bit for bit).
#[test]
fn orig_1469c8_matches_the_game() {
    static SPEC: Spec = Spec { addr: 0x1469c8, args: &[Arg::Ptr(RAW80), Arg::Ptr(F4), Arg::Ptr(CORNERS4)], floats: 0, ret: Ret::None, globals: &[], stubs: &[], nulls: false };
    let Some(st) = fuzz(&SPEC, 4000, |c| {
        let eye = f4(&c.blocks[1], 0);
        let corners: [V4; 4] = std::array::from_fn(|i| f4(&c.blocks[2], 16 * i));
        let p = occluder_planes(eye, &corners);
        put_v(&mut c.blocks[0], 0, &p);
        Out::none()
    }) else { return };
    report(st);
}

/// orig 0x146b20: box against five planes (the harness's random inputs).
#[test]
fn orig_146b20_matches_the_game() {
    static SPEC: Spec = Spec { addr: 0x146b20, args: &[Arg::Ptr(PLANES), Arg::Ptr(BOX)], floats: 0, ret: Ret::Int, globals: &[], stubs: &[], nulls: false };
    let Some(st) = fuzz(&SPEC, 6000, |c| Out::int(box_inside_planes(&planes5(&c.blocks[0]), &fb(&c.blocks[1])) as i64)) else { return };
    report(st);
}

/// orig 0x146bc8: eight points against five planes (the harness's random inputs).
#[test]
fn orig_146bc8_matches_the_game() {
    static SPEC: Spec = Spec { addr: 0x146bc8, args: &[Arg::Ptr(PLANES), Arg::Ptr(POINTS8)], floats: 0, ret: Ret::Int, globals: &[], stubs: &[], nulls: false };
    let Some(st) = fuzz(&SPEC, 6000, |c| Out::int(points_inside_planes(&planes5(&c.blocks[0]), &pts8(&c.blocks[1])) as i64)) else { return };
    report(st);
}

// ---- structured cases ------------------------------------------------------------------------------------------------

fn urange(r: &mut Rng, lo: f32, hi: f32) -> f32 {
    lo + (r.word() >> 40) as f32 / (1u64 << 24) as f32 * (hi - lo)
}

/// A perspective-like clip matrix (rows are the basis vectors' images: x' = f x, y' = f y, z' = z - near,
/// w' = z; the point times matrix form of the game puts the lanes in the columns).
fn perspective(r: &mut Rng) -> M4 {
    let (fx, fy) = (urange(r, 0.5, 2.0), urange(r, 0.5, 2.0));
    let (a, b) = (urange(r, 1.0, 4.0), urange(r, -1000.0, -10.0));
    [[fx, 0.0, 0.0, 0.0], [0.0, fy, 0.0, 0.0], [0.0, 0.0, a, 1.0], [0.0, 0.0, b, 0.0]]
}

fn some_box(r: &mut Rng) -> FBox {
    let c = [urange(r, -80.0, 80.0), urange(r, -80.0, 80.0), urange(r, -20.0, 400.0)];
    let h = [urange(r, 0.0, 60.0), urange(r, 0.0, 60.0), urange(r, 0.0, 60.0)];
    [c[0] - h[0], c[1] - h[1], c[2] - h[2], c[0] + h[0], c[1] + h[1], c[2] + h[2]]
}

/// orig 0x1465c8 and 0x146648 over boxes placed in front of, beside and behind a perspective view (all three answers
/// occur), with a corner exactly on a side and with w = 0 points.
#[test]
fn orig_1465c8_matches_the_game_on_boxes_around_a_view() {
    let Some(elf) = kh2_factory::exe_or_skip("0x1465c8 structured") else { return };
    let mut ee = Ee::new();
    ee.load_elf(&elf).expect("elf");
    let mut r = Rng::new(0x1465c8);
    let mut seen = [0u32; 3];
    for n in 0..3000 {
        let m = perspective(&mut r);
        let mut bx = some_box(&mut r);
        if n % 7 == 0 {
            // corners exactly on a clip side: x' = |w'| for the box's max x at depth z
            bx[3] = bx[2].max(1.0) / m[0][0];
        }
        let mut mb = vec![0u8; 64];
        put_v(&mut mb, 0, &m);
        let mut bb = vec![0u8; 24];
        put(&mut bb, 0, &bx);
        ee.write(0x0180_0000, &mb);
        ee.write(0x0181_0000, &bb);
        let want = clip_box(&m, &bx);
        let got = ee.call(0x146648, &[0x0180_0000, 0x0181_0000], &[], STACK).expect("call") as u32;
        assert_eq!(got, want, "case {n}: box {bx:?}");
        seen[want as usize] += 1;
        let mut pb = vec![0u8; 128];
        let corners = [
            [bx[0], bx[1], bx[2], 1.0],
            [bx[0], bx[4], bx[2], 0.0],
            [bx[3], bx[4], bx[2], 1.0],
            [bx[3], bx[1], bx[5], 1.0],
            [bx[0], bx[1], bx[5], 1.0],
            [bx[0], bx[4], bx[5], 1.0],
            [bx[3], bx[4], bx[5], 1.0],
            [bx[3], bx[1], bx[5], 0.0],
        ];
        put_v(&mut pb, 0, &corners);
        ee.write(0x0182_0000, &pb);
        let want = clip_points(&m, &corners);
        let got = ee.call(0x1465c8, &[0x0180_0000, 0x0182_0000], &[], STACK).expect("call") as u32;
        assert_eq!(got, want, "case {n}: points {corners:?}");
    }
    assert!(seen.iter().all(|&k| k > 100), "every outcome must occur: {seen:?}");
}

/// orig 0x146b20 and 0x146bc8 over the five planes of a view volume with the origin inside: boxes inside, across and
/// outside, and a box touching a plane exactly (d = 0 answers false).
#[test]
fn orig_146b20_matches_the_game_on_boxes_around_a_volume() {
    let Some(elf) = kh2_factory::exe_or_skip("0x146b20 structured") else { return };
    let mut ee = Ee::new();
    ee.load_elf(&elf).expect("elf");
    let mut r = Rng::new(0x146b20);
    let mut inside = 0;
    for n in 0..4000 {
        // unit normals along or near the axes, offsets so that the origin is on the negative side
        let mut planes = [[0.0f32; 4]; 5];
        for (i, p) in planes.iter_mut().enumerate() {
            let axis = i % 3;
            let sign = if r.below(2) == 0 { 1.0 } else { -1.0 };
            let mut v = [urange(&mut r, -0.3, 0.3), urange(&mut r, -0.3, 0.3), urange(&mut r, -0.3, 0.3)];
            v[axis] = sign;
            if n % 11 == 0 && axis == 0 {
                v[0] = -0.0;
            }
            let len = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
            *p = [v[0] / len, v[1] / len, v[2] / len, -urange(&mut r, 5.0, 100.0)];
        }
        let bx = {
            let c = [urange(&mut r, -60.0, 60.0), urange(&mut r, -60.0, 60.0), urange(&mut r, -60.0, 60.0)];
            let h = [urange(&mut r, 0.0, 30.0), urange(&mut r, 0.0, 30.0), urange(&mut r, 0.0, 30.0)];
            [c[0] - h[0], c[1] - h[1], c[2] - h[2], c[0] + h[0], c[1] + h[1], c[2] + h[2]]
        };
        let mut pb = vec![0u8; 80];
        put_v(&mut pb, 0, &planes);
        let mut bb = vec![0u8; 24];
        put(&mut bb, 0, &bx);
        ee.write(0x0180_0000, &pb);
        ee.write(0x0181_0000, &bb);
        let want = box_inside_planes(&planes, &bx);
        let got = ee.call(0x146b20, &[0x0180_0000, 0x0181_0000], &[], STACK).expect("call") as u32;
        assert_eq!(got, want as u32, "case {n}: planes {planes:?} box {bx:?}");
        inside += want as u32;
        let corners = kh2_box_corners(&bx);
        let mut cb = vec![0u8; 128];
        put_v(&mut cb, 0, &corners);
        ee.write(0x0182_0000, &cb);
        let want = points_inside_planes(&planes, &corners);
        let got = ee.call(0x146bc8, &[0x0180_0000, 0x0182_0000], &[], STACK).expect("call") as u32;
        assert_eq!(got, want as u32, "case {n}: points");
    }
    assert!(inside > 200, "inside cases: {inside}");
}

fn kh2_box_corners(b: &FBox) -> [V4; 8] {
    [
        [b[0], b[1], b[2], 1.0],
        [b[0], b[4], b[2], 1.0],
        [b[3], b[4], b[2], 1.0],
        [b[3], b[1], b[2], 1.0],
        [b[0], b[1], b[5], 1.0],
        [b[0], b[4], b[5], 1.0],
        [b[3], b[4], b[5], 1.0],
        [b[3], b[1], b[5], 1.0],
    ]
}
