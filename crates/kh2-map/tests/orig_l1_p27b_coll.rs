//! Oracle test of the attribute kind wrapper `0x143e40` (WP-L1-P27b): kh2-ee runs the original from the user's own executable on
//! worlds built by the game's own `0x142ba8` (random COCT files, hooks for the drawing callees, as in `orig_l1_p22.rs`, whose
//! world builder this file copies), and the port is `MapCollision::attr_kind`.
//!
//! Skips without a disc (KH2_ISO or local/iso.txt); fails without one when KH2_REQUIRE_DISC=1.

#![allow(dead_code)]

use kh2_ee::{Ee, reg};
use kh2_factory::fuzz::*;
use kh2_map::geom::*;
use kh2_map::mapcoll::*;
use kh2_map::mapobj::*;
use kh2_map::oct::*;
use std::cell::RefCell;
use std::rc::Rc;

const OBJ: u32 = 0x0190_0000;

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


/// orig 0x143e40: the kind nibble (low four bits) of the attribute word of polygon `poly` of the walking map, or of the second map
/// collision when asked for and filled (the attribute pointer comes from 0x143e00).
#[test]
fn orig_143e40_matches_the_game() {
    let mut second = 0;
    let mut kinds = std::collections::BTreeSet::new();
    worlds("0x143e40", 30, |r| [true, r.below(2) == 0, false, false, false], |w, r, n| {
        for _ in 0..12 {
            let flag = r.below(2) == 0;
            let use_second = flag && w.coll.second.is_some();
            let limit = if use_second { poly_count(w, 1) } else { w.polys as u64 };
            let p = r.below(limit) as u16;
            let got = call(&mut w.ee, 0x143e40, &[p as u64, flag as u64]) as u32;
            assert_eq!(got, w.coll.attr_kind(p, flag).unwrap(), "world {n} poly {p} flag {flag}");
            second += use_second as usize;
            kinds.insert(got);
        }
    });
    assert!((second > 10 && kinds.len() > 3) || kh2_factory::exe().is_none(), "{second} second, kinds {kinds:?}");
}

fn poly_count(w: &mut World, k: usize) -> u64 {
    let g = w.guest_obj(k);
    w.ee.r32(g + 0x28) as u64
}
