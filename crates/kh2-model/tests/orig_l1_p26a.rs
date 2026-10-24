//! Oracle tests for model lighting and projected shadows (L1-P26a). kh2-ee runs the original from the user's executable on
//! generated inputs and the port (`kh2_model::light`, `kh2_model::draw::shadow`) runs on copies of the same inputs.
//!
//! * The pure leaves (matrix determinant, cofactor, inverse, the light matrices, the light block accessors) use the factory
//!   harness (`factory gen` shape: `fuzz(&SPEC, n)`, blocks compared byte for byte, floats with the harness tolerance).
//! * The routines that read and write the game's memory (the box corners, the record store, the shadow projection and the shadow
//!   pass) use a hand harness at the end of this file: one image of the globals and a heap is laid into kh2-ee and copied into the
//!   port's `Mem`, both run, and every byte of both windows is compared, except the words listed as masked.
//!
//! Stand-ins (the same pure function on both sides, keyed by the case): the frustum test 0x146648, the occluder test 0x145fa0 and
//! the projection builder 0x135028 belong to other packages (L1-P25a, and 0x135028 has no package yet); nothing else is stubbed.
//!
//! Skips without a disc (KH2_ISO or local/iso.txt); fails without one when KH2_REQUIRE_DISC=1.

use kh2_factory::fuzz::*;
use kh2_model::light::*;

fn mat(b: &[u8]) -> Mat4 {
    std::array::from_fn(|i| std::array::from_fn(|j| f32::from_le_bytes(b[16 * i + 4 * j..16 * i + 4 * j + 4].try_into().unwrap())))
}

fn put_mat(b: &mut [u8], m: &Mat4) {
    for (i, row) in m.iter().enumerate() {
        for (j, x) in row.iter().enumerate() {
            b[16 * i + 4 * j..16 * i + 4 * j + 4].copy_from_slice(&x.to_le_bytes());
        }
    }
}

fn vec4(b: &[u8], o: usize) -> [f32; 4] {
    std::array::from_fn(|i| f32::from_le_bytes(b[o + 4 * i..o + 4 * i + 4].try_into().unwrap()))
}

fn put_vec4(b: &mut [u8], o: usize, v: &[f32; 4]) {
    for (i, x) in v.iter().enumerate() {
        b[o + 4 * i..o + 4 * i + 4].copy_from_slice(&x.to_le_bytes());
    }
}

fn floats(b: &[u8], n: usize) -> Vec<f32> {
    (0..n).map(|i| f32::from_le_bytes(b[4 * i..4 * i + 4].try_into().unwrap())).collect()
}

fn light_block(b: &[u8]) -> LightBlock {
    [vec4(b, 0), vec4(b, 0x10), vec4(b, 0x20)]
}

fn put_light_block(b: &mut [u8], l: &LightBlock) {
    for (i, v) in l.iter().enumerate() {
        put_vec4(b, 16 * i, v);
    }
}

fn put_matrices(b: &mut [u8], m: &LightMatrices) {
    for (i, v) in m.iter().enumerate() {
        put_vec4(b, 16 * i, v);
    }
}

// ---- 0x11bb28, 0x11bb78, 0x11bc08 -------------------------------------------------------------------------------------------

const SPEC_11BB28: Spec = Spec {
    addr: 0x11bb28,
    args: &[Arg::Ptr(Block { size: 0x30, fields: &[(0x0, 36, Kind::Float)], subs: &[] })],
    floats: 0,
    ret: Ret::Float,
    globals: &[],
    stubs: &[],
    nulls: true,
};

#[test]
fn orig_11bb28_matches_the_game() {
    let Some(st) = fuzz(&SPEC_11BB28, 4000, |c| {
        let f = floats(c.block(0), 9);
        Out::float(det3(&f.try_into().unwrap()))
    }) else {
        return;
    };
    eprintln!("{} cases equal, {} interpreter steps in {:.3} s", st.cases, st.steps, st.secs);
}

const SPEC_11BB78: Spec = Spec {
    addr: 0x11bb78,
    args: &[Arg::Ptr(Block { size: 0x40, fields: &[(0x0, 64, Kind::Float)], subs: &[] }), Arg::Range(0, 3), Arg::Range(0, 3)],
    floats: 0,
    ret: Ret::Float,
    globals: &[],
    stubs: &[],
    // The original reads through the pointer unconditionally.
    nulls: false,
};

#[test]
fn orig_11bb78_matches_the_game() {
    let Some(st) = fuzz(&SPEC_11BB78, 4000, |c| {
        let m = mat(c.block(0));
        Out::float(cofactor(&m, c.int(1) as usize, c.int(2) as usize))
    }) else {
        return;
    };
    eprintln!("{} cases equal, {} interpreter steps in {:.3} s", st.cases, st.steps, st.secs);
}

const SPEC_11BC08: Spec = Spec {
    addr: 0x11bc08,
    args: &[Arg::Ptr(Block { size: 0x40, fields: &[(0x0, 64, Kind::Float)], subs: &[] })],
    floats: 0,
    ret: Ret::Float,
    globals: &[],
    stubs: &[],
    nulls: false,
};

#[test]
fn orig_11bc08_matches_the_game() {
    let Some(st) = fuzz(&SPEC_11BC08, 4000, |c| {
        let mut m = mat(c.block(0));
        let det = invert(&mut m);
        put_mat(c.block(0), &m);
        Out::float(det)
    }) else {
        return;
    };
    eprintln!("{} cases equal, {} interpreter steps in {:.3} s", st.cases, st.steps, st.secs);
}

// ---- 0x134bd8 ---------------------------------------------------------------------------------------------------------------

const SPEC_134BD8: Spec = Spec {
    addr: 0x134bd8,
    args: &[
        Arg::Ptr(Block { size: 0x70, fields: &[(0x0, 112, Kind::Float)], subs: &[] }),
        Arg::Ptr(Block { size: 0x40, fields: &[(0x0, 64, Kind::Float)], subs: &[] }),
        Arg::Ptr(Block { size: 0x54, fields: &[(0x0, 84, Kind::Float)], subs: &[] }),
    ],
    floats: 0,
    ret: Ret::None,
    globals: &[],
    stubs: &[],
    nulls: false,
};

#[test]
fn orig_134bd8_matches_the_game() {
    let Some(st) = fuzz(&SPEC_134BD8, 4000, |c| {
        let m = mat(c.block(1));
        let packed: [f32; 21] = floats(c.block(2), 21).try_into().unwrap();
        let out = light_matrices_packed(&m, &packed);
        put_matrices(c.block(0), &out);
        Out::none()
    }) else {
        return;
    };
    eprintln!("{} cases equal, {} interpreter steps in {:.3} s", st.cases, st.steps, st.secs);
}

// ---- the light block leaves -------------------------------------------------------------------------------------------------

const SPEC_134F40: Spec = Spec {
    addr: 0x134f40,
    args: &[Arg::Ptr(Block { size: 0x30, fields: &[(0x0, 32, Kind::Float)], subs: &[] })],
    floats: 0,
    ret: Ret::None,
    globals: &[],
    stubs: &[],
    nulls: false,
};

#[test]
fn orig_134f40_matches_the_game() {
    let Some(st) = fuzz(&SPEC_134F40, 4000, |c| {
        let mut l = light_block(c.block(0));
        light_init(&mut l);
        put_light_block(c.block(0), &l);
        Out::none()
    }) else {
        return;
    };
    eprintln!("{} cases equal, {} interpreter steps in {:.3} s", st.cases, st.steps, st.secs);
}

const SPEC_VEC_COPY: Spec = Spec {
    addr: 0x134f90,
    args: &[Arg::Ptr(Block { size: 0x10, fields: &[(0x0, 16, Kind::Int)], subs: &[] }), Arg::Ptr(Block { size: 0x10, fields: &[(0x0, 16, Kind::Int)], subs: &[] })],
    floats: 0,
    ret: Ret::None,
    globals: &[],
    stubs: &[],
    nulls: false,
};

#[test]
fn orig_134f90_matches_the_game() {
    let Some(st) = fuzz(&SPEC_VEC_COPY, 4000, |c| {
        let src = vec4(c.block(1), 0);
        let mut dst = vec4(c.block(0), 0);
        vec_copy(&mut dst, &src);
        put_vec4(c.block(0), 0, &dst);
        Out::none()
    }) else {
        return;
    };
    eprintln!("{} cases equal, {} interpreter steps in {:.3} s", st.cases, st.steps, st.secs);
}

const SPEC_LIGHT_POSITION: Spec = Spec {
    addr: 0x134fa8,
    args: &[Arg::Ptr(Block { size: 0x30, fields: &[(0x0, 48, Kind::Int)], subs: &[] }), Arg::Ptr(Block { size: 0x10, fields: &[(0x0, 16, Kind::Int)], subs: &[] })],
    floats: 0,
    ret: Ret::None,
    globals: &[],
    stubs: &[],
    nulls: false,
};

#[test]
fn orig_134fa8_matches_the_game() {
    let Some(st) = fuzz(&SPEC_LIGHT_POSITION, 4000, |c| {
        let l = light_block(c.block(0));
        let mut out = vec4(c.block(1), 0);
        light_position(&l, &mut out);
        put_vec4(c.block(1), 0, &out);
        Out::none()
    }) else {
        return;
    };
    eprintln!("{} cases equal, {} interpreter steps in {:.3} s", st.cases, st.steps, st.secs);
}

const SPEC_SET_COLOUR: Spec = Spec {
    addr: 0x134fc0,
    args: &[Arg::Ptr(Block { size: 0x30, fields: &[(0x0, 48, Kind::Int)], subs: &[] }), Arg::Ptr(Block { size: 0x10, fields: &[(0x0, 16, Kind::Int)], subs: &[] })],
    floats: 0,
    ret: Ret::None,
    globals: &[],
    stubs: &[],
    nulls: false,
};

#[test]
fn orig_134fc0_matches_the_game() {
    let Some(st) = fuzz(&SPEC_SET_COLOUR, 4000, |c| {
        let mut l = light_block(c.block(0));
        let colour = vec4(c.block(1), 0);
        light_set_colour(&mut l, &colour);
        put_light_block(c.block(0), &l);
        Out::none()
    }) else {
        return;
    };
    eprintln!("{} cases equal, {} interpreter steps in {:.3} s", st.cases, st.steps, st.secs);
}

const SPEC_GET_COLOUR: Spec = Spec {
    addr: 0x134fe0,
    args: &[Arg::Ptr(Block { size: 0x30, fields: &[(0x0, 48, Kind::Int)], subs: &[] }), Arg::Ptr(Block { size: 0x10, fields: &[(0x0, 16, Kind::Int)], subs: &[] })],
    floats: 0,
    ret: Ret::None,
    globals: &[],
    stubs: &[],
    nulls: false,
};

#[test]
fn orig_134fe0_matches_the_game() {
    let Some(st) = fuzz(&SPEC_GET_COLOUR, 4000, |c| {
        let l = light_block(c.block(0));
        let mut out = vec4(c.block(1), 0);
        light_colour(&l, &mut out);
        put_vec4(c.block(1), 0, &out);
        Out::none()
    }) else {
        return;
    };
    eprintln!("{} cases equal, {} interpreter steps in {:.3} s", st.cases, st.steps, st.secs);
}

// ===================================================================================================================================
// The hand harness: functions that read and write the game's memory.
// ===================================================================================================================================

use kh2_ee::{Ee, reg};
use kh2_model::draw::shadow::*;
use kh2_model::texanim::objects::Mem;
use std::cell::Cell;
use std::rc::Rc;

/// The port's memory image covers the globals the routines touch and a heap in the executable's bss, which nothing else uses
/// while a case runs. Only the windows below are laid into kh2-ee and compared.
const IMG_BASE: u32 = 0x0034_0000;
const IMG_SIZE: usize = 0x0015_0000;
const HEAP: u32 = 0x0040_0000;
const HEAP_SIZE: u32 = 0x1_0000;
/// (start, length): the record count and table word, the bump pointer, the default colour bytes, the global light block, the
/// shadow block and its object pointer, and the heap.
const WINDOWS: [(u32, u32); 6] = [(0x349e68, 0x10), (0x34e0a8, 4), (0x34e0d0, 4), (0x354e30, 0x30), (0x484378, 0x48), (HEAP, HEAP_SIZE)];

// Heap layout (offsets from HEAP).
const TREE: u32 = HEAP;
const NODES: u32 = HEAP + 0x1000;
const ENTRIES: u32 = HEAP + 0x2000;
const LIST: u32 = HEAP + 0x3000;
const MAP: u32 = HEAP + 0x4000;
const MODEL0: u32 = HEAP + 0x4100;
const MODEL1: u32 = HEAP + 0x4200;
const OBJPTRS: u32 = HEAP + 0x5000;
const OBJ_BASE: u32 = HEAP + 0x6000;
const OBJ_STRIDE: u32 = 0x200;
const MODE: u32 = HEAP + 0x9000;
const GROUPS: u32 = HEAP + 0x9100;
const MATS: u32 = HEAP + 0x9200;
const SCRATCH: u32 = HEAP + 0x9800;
const TABLE: u32 = HEAP + 0xa000;
const ARENA: u32 = HEAP + 0xc000;

/// One case: the image, the call's integer arguments and the bytes to leave out of the comparison.
struct Setup {
    img: Mem,
    ints: Vec<u64>,
    mask: Vec<(u32, u32)>,
}

/// The stand-ins for the routines other packages own, as one pure function of the case seed on both sides.
fn mixu(a: u32, b: u32) -> u32 {
    (a ^ b.rotate_left(7)).wrapping_mul(0x9e37_79b1).rotate_left(11) ^ b.wrapping_mul(0x85eb_ca6b)
}

fn box_hash(seed: u32, salt: u32, bx: &[f32; 6]) -> u32 {
    bx.iter().fold(mixu(seed, salt), |h, f| mixu(h, f.to_bits()))
}

fn stand_in_visible(seed: u32, view: u32, bx: &[f32; 6]) -> i64 {
    [0, 1, 2, 1, 2, 3][(box_hash(seed, view, bx) % 6) as usize]
}

fn stand_in_occluded(seed: u32, tree: u32, bx: &[f32; 6]) -> i64 {
    if box_hash(seed, tree ^ 0x55, bx) % 4 == 0 { 1 } else { 0 }
}

fn stand_in_project(seed: u32, light: &[[f32; 4]; 2], corners: &[[f32; 4]; 8], kind: i32) -> (Mat4, Mat4) {
    let mut h = mixu(seed, kind as u32);
    for v in light.iter().chain(corners.iter()) {
        for x in v {
            h = mixu(h, x.to_bits());
        }
    }
    let make = |salt: u32| -> Mat4 {
        std::array::from_fn(|i| std::array::from_fn(|j| (((mixu(h, salt * 16 + (i * 4 + j) as u32) >> 3) % 4096) as f32 - 2048.0) / 64.0))
    };
    (make(1), make(2))
}

struct Oracle {
    seed: u32,
}

impl ShadowHost for Oracle {
    fn box_visible(&mut self, _m: &Mem, view: u32, bx: &[f32; 6]) -> i64 {
        stand_in_visible(self.seed, view, bx)
    }
    fn occluded(&mut self, _m: &Mem, tree: u32, bx: &[f32; 6]) -> i64 {
        stand_in_occluded(self.seed, tree, bx)
    }
    fn project(&mut self, _m: &Mem, light: &[[f32; 4]; 2], corners: &[[f32; 4]; 8], kind: i32) -> (Mat4, Mat4) {
        stand_in_project(self.seed, light, corners, kind)
    }
}

fn read_box(e: &mut Ee, a: u32) -> [f32; 6] {
    std::array::from_fn(|i| e.f32_at(a + 4 * i as u32))
}

fn rig(what: &str) -> Option<(Ee, Rc<Cell<u32>>)> {
    let elf = kh2_factory::exe_or_skip(what)?;
    let mut ee = Ee::new();
    ee.load_elf(&elf).expect("elf");
    let seed = Rc::new(Cell::new(0u32));
    let s = seed.clone();
    ee.hooks.insert(0x146648, move |e: &mut Ee| {
        let (view, a) = (e.gpr[reg::A0] as u32, e.gpr[reg::A0 + 1] as u32);
        let bx = read_box(e, a);
        e.gpr[reg::V0] = stand_in_visible(s.get(), view, &bx) as u64 as u128;
    });
    let s = seed.clone();
    ee.hooks.insert(0x145fa0, move |e: &mut Ee| {
        let (tree, a) = (e.gpr[reg::A0] as u32, e.gpr[reg::A0 + 1] as u32);
        let bx = read_box(e, a);
        e.gpr[reg::V0] = stand_in_occluded(s.get(), tree, &bx) as u64 as u128;
    });
    let s = seed.clone();
    ee.hooks.insert(0x135028, move |e: &mut Ee| {
        let (l, c, out_a, out_b, kind) = (e.gpr[reg::A0] as u32, e.gpr[reg::A0 + 1] as u32, e.gpr[reg::A0 + 2] as u32, e.gpr[reg::A0 + 3] as u32, e.gpr[reg::T0] as i32);
        let light: [[f32; 4]; 2] = std::array::from_fn(|v| std::array::from_fn(|i| e.f32_at(l + 16 * v as u32 + 4 * i as u32)));
        let corners: [[f32; 4]; 8] = std::array::from_fn(|v| std::array::from_fn(|i| e.f32_at(c + 16 * v as u32 + 4 * i as u32)));
        let (a, b) = stand_in_project(s.get(), &light, &corners, kind);
        for (base, m) in [(out_a, a), (out_b, b)] {
            for i in 0..4 {
                for j in 0..4 {
                    e.wf32(base + 16 * i as u32 + 4 * j as u32, m[i][j]);
                }
            }
        }
    });
    Some((ee, seed))
}

fn put_windows(ee: &mut Ee, img: &Mem) {
    for &(a, n) in &WINDOWS {
        let o = (a - IMG_BASE) as usize;
        ee.write(a, &img.bytes[o..o + n as usize]);
    }
}

/// How much the original wrote outside the windows and the stack is checked on the first cases.
const AUDIT: u32 = 6;

/// Runs `cases` cases of the original at `addr` against `port` and compares every window byte except the masked ones.
/// `check` sees the machine after the original ran (for results that live on its stack). Returns the number of cases where the
/// port changed the image, or `None` without a disc.
fn oracle(
    what: &str,
    addr: u32,
    cases: u32,
    mut build: impl FnMut(&mut Rng) -> Setup,
    mut port: impl FnMut(&mut Mem, &mut Oracle, &Setup),
    mut check: impl FnMut(&mut Ee, &Setup, &Mem),
) -> Option<u32> {
    let (mut ee, seed) = rig(what)?;
    let mut rng = Rng::new(addr as u64 ^ 0x26a);
    let mut changed = 0;
    for n in 0..cases {
        let setup = build(&mut rng);
        seed.set(n.wrapping_mul(0x9e37_79b1) ^ 0x1234_5678);
        put_windows(&mut ee, &setup.img);
        let before = (n < AUDIT).then(|| ee.ram.clone());
        if let Err(f) = ee.call(addr, &setup.ints, &[], STACK) {
            panic!("{what} case {n}: the original at {addr:#x} faulted: {f:?} (ints {:x?})", setup.ints);
        }
        if let Some(before) = before {
            let inside = |a: u32| WINDOWS.iter().any(|&(s, l)| (s..s + l).contains(&a)) || (STACK - 0x10000..STACK + 0x100).contains(&a);
            if let Some(a) = (0..ee.ram.len()).find(|&i| before[i] != ee.ram[i] && !inside(i as u32)) {
                panic!("{what} case {n}: the original wrote {a:#x}, outside the windows and the stack");
            }
        }
        let mut pm = setup.img.clone();
        let mut host = Oracle { seed: seed.get() };
        port(&mut pm, &mut host, &setup);
        for &(a, len) in &WINDOWS {
            for o in a..a + len {
                if setup.mask.iter().any(|&(s, l)| (s..s + l).contains(&o)) {
                    continue;
                }
                let (g, p) = (ee.ram[o as usize], pm.r8(o));
                assert!(
                    g == p,
                    "{what} case {n}: byte {o:#x} game {g:02x} port {p:02x} (input {:02x}; ints {:x?})",
                    setup.img.r8(o),
                    setup.ints
                );
            }
        }
        check(&mut ee, &setup, &pm);
        if WINDOWS.iter().any(|&(a, len)| (a..a + len).any(|o| setup.img.r8(o) != pm.r8(o))) {
            changed += 1;
        }
    }
    eprintln!("{what}: {cases} cases equal, {changed} changed the image");
    Some(changed)
}

// ---- generators -------------------------------------------------------------------------------------------------------------

fn chance(r: &mut Rng, n: u64) -> bool {
    r.below(n) == 0
}

fn fill(m: &mut Mem, r: &mut Rng, from: u32, len: u32) {
    for o in 0..len {
        m.w8(from + o, r.word() as u8);
    }
}

/// A coordinate on a coarse grid so boxes overlap often.
fn grid(r: &mut Rng) -> f32 {
    (r.below(17) as f32 - 8.0) * 0.5
}

/// A float for matrices and lights: the harness's mix of edge values and magnitudes.
fn wide(r: &mut Rng) -> f32 {
    r.float()
}

fn put_box(m: &mut Mem, r: &mut Rng, at: u32) {
    let lo = [grid(r), grid(r), grid(r)];
    for k in 0..3 {
        m.wf32(at + 4 * k as u32, lo[k]);
        let hi = lo[k] + (r.below(9) as f32) * 0.5;
        m.wf32(at + 12 + 4 * k as u32, if chance(r, 12) { lo[k] - 1.0 } else { hi });
    }
}

fn put_mat_mem(m: &mut Mem, at: u32, v: &Mat4) {
    for (i, row) in v.iter().enumerate() {
        for (j, x) in row.iter().enumerate() {
            m.wf32(at + 16 * i as u32 + 4 * j as u32, *x);
        }
    }
}

fn rand_mat(r: &mut Rng) -> Mat4 {
    let mut m: Mat4 = std::array::from_fn(|_| std::array::from_fn(|_| wide(r)));
    if chance(r, 6) {
        let row = r.below(4) as usize;
        m[row] = [0.0; 4];
    }
    m
}

/// A base image: the bump allocator over the arena, the default colour, the global light block, the record table.
fn base_image(r: &mut Rng) -> Mem {
    let mut m = Mem::new(IMG_BASE, IMG_SIZE);
    m.w32(BUMP, ARENA);
    fill(&mut m, r, BASE_COLOUR, 4);
    for k in 0..12 {
        m.wf32(0x354e30 + 4 * k, wide(r));
    }
    let count = r.below(6) as u32;
    m.w32(RECORD_COUNT, count);
    m.w32(RECORD_TABLE, TABLE);
    for slot in 0..count {
        fill(&mut m, r, TABLE + slot * RECORD_SIZE, RECORD_SIZE);
        if chance(r, 2) {
            m.w32(TABLE + slot * RECORD_SIZE + 4, 0);
        }
    }
    m
}

/// `n` objects at `OBJ_BASE`: a box, the alpha and colour bytes, the flags, the layer, and the lift at +0x104.
fn put_objects(m: &mut Mem, r: &mut Rng, n: u32) {
    for k in 0..n {
        let o = OBJ_BASE + k * OBJ_STRIDE;
        put_box(m, r, o + 0xd0);
        m.w8(o + 0x10c, if chance(r, 6) { 0 } else { r.below(256) as u8 });
        m.wf32(o + 0x104, grid(r));
        let mut flags = r.word() as u32 & 0x7ff;
        if chance(r, 2) {
            flags |= 0x40;
        }
        m.w32(o + 0xf8, flags);
        fill(m, r, o + 0xe8, 4);
        m.w8(o + 0x11e, r.below(6) as u8);
    }
}

/// A null-terminated pointer list at `OBJPTRS` of the first `n` objects (`n` may be 0).
fn put_obj_list(m: &mut Mem, n: u32) {
    for k in 0..n {
        m.w32(OBJPTRS + 4 * k, OBJ_BASE + k * OBJ_STRIDE);
    }
    m.w32(OBJPTRS + 4 * n, 0);
}

/// A map block: the model type chain (+0x14 -> +4 -> a word that is 2 in half the cases) and the two matrices.
fn put_map(m: &mut Mem, r: &mut Rng) {
    m.w32(MAP + 0x14, MODEL0);
    m.w32(MODEL0 + 4, MODEL1);
    m.w32(MODEL1, if chance(r, 2) { 2 } else { r.below(4) as u32 });
    put_mat_mem(m, MAP + 0x40, &rand_mat(r));
    put_mat_mem(m, MAP + 0x80, &rand_mat(r));
}

fn put_groups(m: &mut Mem, r: &mut Rng, at: u32) {
    let n = if chance(r, 5) { 0 } else { r.below(40) as u32 };
    for k in 0..n {
        m.w16(at + 2 * k, r.below(0x7fff) as u16);
    }
    m.w16(at + 2 * n, 0xffff);
}

fn put_mode(m: &mut Mem, r: &mut Rng) {
    fill(m, r, MODE, 0x38);
    let w = m.r32(MODE);
    m.w32(MODE, if chance(r, 2) { w | 1 } else { w & !1 });
}

// ---- 0x134960, 0x135870, 0x135880, 0x1358a0, 0x134f88 ------------------------------------------------------------------------

#[test]
fn orig_134960_matches_the_game() {
    const OUT: u32 = HEAP;
    const M: u32 = HEAP + 0x100;
    const LIGHTS: [u32; 3] = [HEAP + 0x200, HEAP + 0x300, HEAP + 0x400];
    let Some(changed) = oracle(
        "0x134960",
        0x134960,
        3000,
        |r| {
            let mut img = base_image(r);
            fill(&mut img, r, OUT, 0x70);
            put_mat_mem(&mut img, M, &rand_mat(r));
            let mut ints = vec![OUT as u64, M as u64];
            for l in LIGHTS {
                for k in 0..12 {
                    img.wf32(l + 4 * k, wide(r));
                }
                ints.push(if chance(r, 4) { 0 } else { l as u64 });
            }
            Setup { img, ints, mask: vec![] }
        },
        |m, _h, s| {
            let mat: Mat4 = std::array::from_fn(|i| std::array::from_fn(|j| m.rf32(M + 16 * i as u32 + 4 * j as u32)));
            let mut blocks: [Option<LightBlock>; 3] = [None, None, None];
            for k in 0..3 {
                if s.ints[2 + k] != 0 {
                    blocks[k] = Some(std::array::from_fn(|v| std::array::from_fn(|i| m.rf32(LIGHTS[k] + 16 * v as u32 + 4 * i as u32))));
                }
            }
            let out = light_matrices(&mat, [blocks[0].as_ref(), blocks[1].as_ref(), blocks[2].as_ref()], &global_ambient(m));
            for (v, row) in out.iter().enumerate() {
                for (i, x) in row.iter().enumerate() {
                    m.wf32(OUT + 16 * v as u32 + 4 * i as u32, *x);
                }
            }
        },
        |_, _, _| {},
    ) else {
        return;
    };
    assert!(changed > 2500, "{changed}");
}

/// The global light block accessors: 0x135870 returns the block's address, 0x135880 stores a vector at 0x354e50 and 0x1358a0
/// reads it back; 0x134f88 is an empty function that touches nothing.
#[test]
fn orig_135870_135880_1358a0_134f88_match_the_game() {
    const V: u32 = HEAP;
    const OUT: u32 = HEAP + 0x40;
    let Some((mut ee, _)) = rig("0x135870") else { return };
    let mut rng = Rng::new(0x135870);
    assert_eq!(ee.call(0x135870, &[], &[], STACK).unwrap() as u32, global_light_block());
    for n in 0..500 {
        let mut img = base_image(&mut rng);
        let v: [f32; 4] = std::array::from_fn(|_| wide(&mut rng));
        for (i, x) in v.iter().enumerate() {
            img.wf32(V + 4 * i as u32, *x);
        }
        put_windows(&mut ee, &img);
        ee.call(0x135880, &[V as u64], &[], STACK).unwrap();
        let mut pm = img.clone();
        set_global_ambient(&mut pm, &v);
        for o in 0x354e30..0x354e60 {
            assert_eq!(ee.ram[o as usize], pm.r8(o), "0x135880 case {n}: byte {o:#x}");
        }
        assert_eq!(global_ambient(&pm), v);
        ee.call(0x1358a0, &[OUT as u64], &[], STACK).unwrap();
        for i in 0..4 {
            assert_eq!(ee.f32_at(OUT + 4 * i).to_bits(), global_ambient(&pm)[i as usize].to_bits(), "0x1358a0 case {n}");
        }
        // 0x134f88 changes nothing at all.
        let snapshot = ee.ram.clone();
        ee.call(0x134f88, &[], &[], STACK).unwrap();
        assert!(snapshot == ee.ram, "0x134f88 wrote memory");
    }
}
