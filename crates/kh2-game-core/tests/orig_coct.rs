//! Oracle tests of the collision object's queries (WP-L1-P10): kh2-ee runs the original from the user's own
//! executable; the port is `kh2_map::oct` / `kh2_map::geom`.
//!
//! The leaf functions (boxes) are plain factory-style fuzz tests. The queries need a built collision object, so
//! each world below is a random COCT file written into emulator memory and built by the game's own
//! `FUN_00141500` (nothing is stubbed; the moving-object list `DAT_005210c0` stays empty, so `FUN_00143840`
//! runs the map path only, which is what a plain field does). The same file bytes go to `Oct::new`. Colliders
//! are random records around the generated polygons; the port and the original run on copies and every field
//! of the record (floats with a relative tolerance of 1e-5) is compared, together with the return value.
//! Skips without a disc (KH2_ISO or local/iso.txt); fails without one when KH2_REQUIRE_DISC=1.

use kh2_ee::Ee;
use kh2_factory::fuzz::*;
use kh2_map::geom::*;
use kh2_map::oct::*;

// ---------------------------------------------------------------------------------------------------------
// Leaves (pure functions)
// ---------------------------------------------------------------------------------------------------------

const FBOX: Block = Block {
    size: 24,
    fields: &[(0, 4, Kind::Float), (4, 4, Kind::Float), (8, 4, Kind::Float), (12, 4, Kind::Float), (16, 4, Kind::Float), (20, 4, Kind::Float)],
    subs: &[],
};

const QBOX: Block = Block { size: 12, fields: &[(0, 4, Kind::Int), (4, 4, Kind::Int), (8, 4, Kind::Int)], subs: &[] };

fn floats6(b: &[u8]) -> FBox {
    std::array::from_fn(|k| f32::from_le_bytes(b[4 * k..4 * k + 4].try_into().unwrap()))
}

fn qbox_of(b: &[u8]) -> QBox {
    std::array::from_fn(|k| i16::from_le_bytes(b[2 * k..2 * k + 2].try_into().unwrap()))
}

fn put6(dst: &mut [u8], b: &FBox) {
    for (k, f) in b.iter().enumerate() {
        dst[4 * k..4 * k + 4].copy_from_slice(&f.to_le_bytes());
    }
}

/// orig 0x144c28
#[test]
fn orig_144c28_matches_the_game() {
    static SPEC: Spec = Spec { addr: 0x144c28, args: &[Arg::Ptr(FBOX), Arg::Ptr(FBOX)], floats: 0, ret: Ret::Int, globals: &[], stubs: &[], nulls: false };
    let Some(st) = fuzz(&SPEC, 4000, |c| Out::int(fbox_overlap(&floats6(&c.blocks[0]), &floats6(&c.blocks[1])) as i64)) else { return };
    eprintln!("{} cases equal, {} interpreter steps in {:.3} s", st.cases, st.steps, st.secs);
}

/// orig 0x145498
#[test]
fn orig_145498_matches_the_game() {
    static SPEC: Spec = Spec { addr: 0x145498, args: &[Arg::Ptr(QBOX), Arg::Ptr(QBOX)], floats: 0, ret: Ret::Int, globals: &[], stubs: &[], nulls: false };
    let Some(st) = fuzz(&SPEC, 4000, |c| Out::int(qbox_overlap(&qbox_of(&c.blocks[0]), &qbox_of(&c.blocks[1])) as i64)) else { return };
    eprintln!("{} cases equal, {} interpreter steps in {:.3} s", st.cases, st.steps, st.secs);
}

/// orig 0x144778
#[test]
fn orig_144778_matches_the_game() {
    static SPEC: Spec = Spec { addr: 0x144778, args: &[Arg::Ptr(FBOX), Arg::Ptr(FBOX), Arg::Ptr(FBOX)], floats: 0, ret: Ret::None, globals: &[], stubs: &[], nulls: false };
    let Some(st) = fuzz(&SPEC, 4000, |c| {
        let u = fbox_union(&floats6(&c.blocks[1]), &floats6(&c.blocks[2]));
        put6(&mut c.blocks[0], &u);
        Out::none()
    }) else { return };
    eprintln!("{} cases equal, {} interpreter steps in {:.3} s", st.cases, st.steps, st.secs);
}

/// orig 0x1446f8
#[test]
fn orig_1446f8_matches_the_game() {
    static SPEC: Spec = Spec { addr: 0x1446f8, args: &[Arg::Ptr(FBOX), Arg::Ptr(FBOX), Arg::Ptr(FBOX)], floats: 0, ret: Ret::None, globals: &[], stubs: &[], nulls: false };
    let Some(st) = fuzz(&SPEC, 4000, |c| {
        let (a, b) = (floats6(&c.blocks[1]), floats6(&c.blocks[2]));
        let o = fbox_corners([a[0], a[1], a[2]], [b[0], b[1], b[2]]);
        put6(&mut c.blocks[0], &o);
        Out::none()
    }) else { return };
    eprintln!("{} cases equal, {} interpreter steps in {:.3} s", st.cases, st.steps, st.secs);
}

/// orig 0x1443a8 (six floats in registers a-then-stack are not fuzzed: the function is a plain store, covered by
/// every test that builds a box through it).
///
/// orig 0x144d68: the float box to the quantized box, levels 0 to 3, inputs over and under the clamps.
#[test]
fn orig_144d68_matches_the_game() {
    static SPEC: Spec = Spec {
        addr: 0x144d68,
        args: &[Arg::Ptr(Block { size: 12, fields: &[], subs: &[] }), Arg::Ptr(FBOX), Arg::Range(0, 3)],
        floats: 0,
        ret: Ret::None,
        globals: &[],
        stubs: &[],
        nulls: false,
    };
    let Some(st) = fuzz(&SPEC, 4000, |c| {
        let b = floats6(&c.blocks[1]);
        let q = fbox_to_qbox(&b, c.int(2) as usize);
        for (k, v) in q.iter().enumerate() {
            c.blocks[0][2 * k..2 * k + 2].copy_from_slice(&v.to_le_bytes());
        }
        Out::none()
    }) else { return };
    eprintln!("{} cases equal, {} interpreter steps in {:.3} s", st.cases, st.steps, st.secs);
}

// ---------------------------------------------------------------------------------------------------------
// A random COCT file
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

fn urange(r: &mut Rng, lo: f32, hi: f32) -> f32 {
    lo + (r.word() >> 40) as f32 / (1u64 << 24) as f32 * (hi - lo)
}

fn pick<T: Copy>(r: &mut Rng, s: &[T]) -> T {
    s[r.below(s.len() as u64) as usize]
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
        meshes.push((i, j, r.below(4) as u8, b));
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
// The rig: one built object in emulator memory, the port's object over the same bytes
// ---------------------------------------------------------------------------------------------------------

const FILE_AT: u32 = 0x0120_0000;
const OBJ_AT: u32 = 0x0198_0000;
const COLL_AT: u32 = 0x019a_0000;
const HIT_AT: u32 = 0x019b_0000;
const OUT_AT: u32 = 0x019c_0000;

struct Rig {
    ee: Ee,
    oct: Oct,
    polys: usize,
    /// The centre of each polygon, so colliders and rays can be put where something is.
    centres: Vec<[f32; 3]>,
}

fn feq(a: f32, b: f32) -> bool {
    a.to_bits() == b.to_bits() || (a - b).abs() <= 1e-5 * (1.0 + a.abs().max(b.abs()))
}

impl Rig {
    fn new(elf: &[u8], r: &mut Rng) -> Rig {
        let n = 5 + r.below(12) as usize;
        let span = urange(r, 120.0, 450.0);
        // One world in five is a dense cluster of walls around the origin, so a sphere there touches more than nine
        // polygons (the walking solver gives up past nine contacts).
        let dense = r.below(5) == 0;
        let polys: Vec<Poly> = (0..if dense { 28 } else { n }).map(|k| if dense { dense_wall(r, k) } else { random_poly(r, span) }).collect();
        let level = r.below(4) as u32;
        let file = build_file(r, &polys, level);
        let groups = pick(r, &[0xfu64, 0x7, 0x3, 0x5, 0xb, 1 << 63 | 0xf, 0]);
        let centres = polys
            .iter()
            .map(|p| {
                let b = bounds(&p.v);
                [(b[0] + b[3]) / 2.0, (b[1] + b[4]) / 2.0, (b[2] + b[5]) / 2.0]
            })
            .collect();
        Rig::from_file(elf, &file, groups, centres)
    }

    /// The game's object over `file` (built by `FUN_00141500`) and the port's over the same bytes.
    fn from_file(elf: &[u8], file: &[u8], groups: u64, centres: Vec<[f32; 3]>) -> Rig {
        let mut ee = Ee::new();
        ee.load_elf(elf).expect("elf");
        // The identity matrix the game fills at boot (FUN_0011b460 copies it into every new object).
        for i in 0..16 {
            ee.wf32(0x354a60 + 4 * i, if i % 5 == 0 { 1.0 } else { 0.0 });
        }
        ee.write(FILE_AT, file);
        ee.call(0x141500, &[OBJ_AT as u64, FILE_AT as u64, groups], &[], STACK).expect("FUN_00141500");
        ee.w32(0x355934, OBJ_AT);
        ee.w32(0x5210c0, 0);
        ee.w32(0x5210c4, 0);
        let oct = Oct::new(file, groups).expect("port object");
        Rig { ee, oct, polys: centres.len(), centres }
    }

    /// A random collider around a random polygon.
    fn collider(&mut self, r: &mut Rng) -> Collider {
        let mut c = Collider::default();
        for b in c.0.iter_mut() {
            *b = r.word() as u8;
        }
        let span = 500.0;
        let near = self.centres[r.below(self.polys as u64) as usize];
        let start = if r.below(4) == 0 {
            [urange(r, -span, span), urange(r, -120.0, 40.0), urange(r, -span, span), 1.0]
        } else {
            [near[0] + urange(r, -200.0, 200.0), urange(r, -150.0, 40.0), near[2] + urange(r, -200.0, 200.0), 1.0]
        };
        let len = pick(r, &[0.0, 0.0, 10.0, 40.0, 150.0, 400.0]) * urange(r, 0.2, 1.0);
        let ang = urange(r, 0.0, std::f32::consts::TAU);
        let dy = if r.below(3) == 0 { urange(r, -60.0, 60.0) } else { 0.0 };
        let delta = [ang.cos() * len, dy, ang.sin() * len, 1.0];
        let radius = urange(r, 20.0, 120.0);
        let lift = urange(r, 20.0, 160.0);
        c.set_v4(coll::START, start);
        c.set_v4(coll::DELTA, delta);
        c.set_f32(coll::RADIUS, radius);
        c.set_f32(coll::HEIGHT, urange(r, 20.0, 130.0));
        let mut end = start;
        end[0] += delta[0];
        end[1] += delta[1];
        end[2] += delta[2];
        c.set_v4(coll::END, end);
        c.set_f32(coll::LIFT, lift);
        c.set_f32(coll::EXTRA, pick(r, &[0.0, 0.0, 30.0]));
        c.set_f32(coll::FLOOR_Y, urange(r, -100.0, 100.0));
        let kind = pick(r, &[0u64, 0, 0, 0, 1, 3, 6, 2, 4, 5, 7, 15]);
        let mut flags = r.word() & !0x7f_u64 & !(0xfu64 << 32);
        flags |= kind << 3 | (r.below(4) << 1);
        flags |= (r.below(5) == 0) as u64;
        c.set_flags(flags);
        c
    }

    fn put(&mut self, c: &Collider) {
        self.ee.write(COLL_AT, &c.0);
    }

    fn get(&mut self) -> Collider {
        Collider(self.ee.read::<{ coll::SIZE }>(COLL_AT))
    }
}

/// Every record field compared: floats with a relative tolerance, the rest byte for byte.
fn coll_diff(game: &Collider, port: &Collider) -> Option<String> {
    let float_word = |o: usize| (o < 0x28) || (0x30..0x60).contains(&o) || o == 0x68 || o == 0x6c || o == 0x80;
    let mut o = 0;
    while o < coll::SIZE {
        if o % 16 == 12 && o < 0x60 {
            // The w lane of a vector: the VU ops write xyz and leave w as the destination register held it, so it is
            // not part of the contract (nothing reads it).
            o += 4;
        } else if o % 4 == 0 && float_word(o) {
            let (a, b) = (game.f32_at(o), port.f32_at(o));
            if !feq(a, b) {
                return Some(format!("float +{o:#x}: game {a} port {b}"));
            }
            o += 4;
        } else {
            if game.0[o] != port.0[o] {
                return Some(format!("byte +{o:#x}: game {:02x} port {:02x}", game.0[o], port.0[o]));
            }
            o += 1;
        }
    }
    None
}

fn rigs(what: &str, worlds: usize, per: usize, mut body: impl FnMut(&mut Rig, &mut Rng, usize)) {
    let Some(elf) = kh2_factory::exe_or_skip(what) else { return };
    let mut r = Rng::new(0x0c0c_7001 ^ worlds as u64);
    let t0 = std::time::Instant::now();
    for w in 0..worlds {
        let mut rig = Rig::new(&elf, &mut r);
        for k in 0..per {
            body(&mut rig, &mut r, w * per + k);
        }
    }
    eprintln!("{what}: {} cases in {:.2} s", worlds * per, t0.elapsed().as_secs_f64());
}

/// orig 0x141b50: the polygon walker (filter armed by 0x141b30, handler table 0x32e3b8), over all attribute filters and handler kinds.
#[test]
fn orig_141b50_matches_the_game() {
    let mut nonempty = 0;
    rigs("0x141b50", 30, 70, |rig, r, n| {
        let filt = match r.below(4) {
            0 => Filter::ALL,
            1 => Filter::WALK,
            2 => Filter { mask: 0x200, value: 0x200, invert: false, b: false },
            _ => Filter { mask: r.word() as u32 & 0xf, value: r.below(16) as u32, invert: r.below(2) == 0, b: r.below(2) == 0 },
        };
        let kind = r.below(17) as u32;
        let near = rig.centres[r.below(rig.polys as u64) as usize];
        let (hx, hy, hz) = (urange(r, 20.0, 500.0), urange(r, 20.0, 400.0), urange(r, 20.0, 500.0));
        let fb = fbox(near[0] - hx, near[1] - hy, near[2] - hz, near[0] + hx, near[1] + hy, near[2] + hz);
        let q: QBox = fbox_to_qbox(&fb, rig.oct.level());
        // setSelectMask(oct, mask, value, invert, b)
        rig.ee.call(0x141b30, &[OBJ_AT as u64, filt.mask as u64, filt.value as u64, filt.invert as u64, filt.b as u64], &[], STACK).unwrap();
        rig.ee.w32(OBJ_AT + 0x3c, HIT_AT);
        for (k, v) in q.iter().enumerate() {
            rig.ee.write(OUT_AT + 2 * k as u32, &v.to_le_bytes());
        }
        rig.ee.write(OUT_AT + 0x20, &0u16.to_le_bytes());
        rig.ee.call(0x141b50, &[OBJ_AT as u64, 0, OUT_AT as u64, (OUT_AT + 0x20) as u64, kind as u64], &[], STACK).unwrap();
        let count = u16::from_le_bytes(rig.ee.read::<2>(OUT_AT + 0x20));
        let game: Vec<u16> = (0..count).map(|i| u16::from_le_bytes(rig.ee.read::<2>(HIT_AT + 2 * i as u32))).collect();
        let mut mine = Vec::new();
        rig.oct.collect(filt, &q, kind, &mut mine);
        assert_eq!(game, mine, "case {n}: filter {filt:?} kind {kind} box {q:?}");
        nonempty += !game.is_empty() as usize;
    });
    assert!(nonempty > 200, "only {nonempty} cases collected a polygon: the test checks too little");
}

/// orig 0x164138 (the sphere sweep with 0x163f98, 0x1632b8, 0x1625e0, 0x162678, 0x162028, 0x1626f0, 0x330c80,
/// 0x141f08 and the leaves) over random colliders.
#[test]
fn orig_164138_matches_the_game() {
    let mut contacts = 0;
    let mut moved = 0;
    rigs("0x164138", 40, 60, |rig, r, n| {
        let c0 = rig.collider(r);
        rig.put(&c0);
        let ret = rig.ee.call(0x164138, &[OBJ_AT as u64, COLL_AT as u64], &[], STACK).unwrap();
        let game = rig.get();
        let mut mine = c0.clone();
        let out = rig.oct.sweep(&mut mine);
        if let Some(d) = coll_diff(&game, &mine) {
            panic!("case {n}: {d}\nstart {:?} delta {:?} r {} lift {} flags {:#x}", c0.v4(0), c0.v4(0x10), c0.f32_at(0x20), c0.f32_at(0x6c), c0.flags());
        }
        assert_eq!(ret & 1, out, "case {n}: return");
        contacts += (game.flags() >> 34 & 1) as usize;
        moved += (game.v4(coll::END)[0] != c0.v4(coll::START)[0] + c0.v4(coll::DELTA)[0]) as usize;
    });
    assert!(contacts > 100 && moved > 100, "contacts {contacts}, pushed {moved}: the test checks too little");
}

/// orig 0x1423d8 (the floor ray with 0x141910 and the ray cast 0x141f08).
#[test]
fn orig_1423d8_matches_the_game() {
    let mut hits = 0;
    rigs("0x1423d8", 40, 60, |rig, r, n| {
        let c0 = rig.collider(r);
        rig.put(&c0);
        let ret = rig.ee.call(0x1423d8, &[OBJ_AT as u64, COLL_AT as u64], &[], STACK).unwrap();
        let game = rig.get();
        let mut mine = c0.clone();
        let out = rig.oct.floor_ray(&mut mine);
        if let Some(d) = coll_diff(&game, &mine) {
            panic!("case {n}: {d}");
        }
        assert_eq!(ret & 1, out as u64, "case {n}: return");
        hits += out as usize;
    });
    assert!(hits > 100, "only {hits} floor hits");
}

/// orig 0x142760: the vertical probe.
#[test]
fn orig_142760_matches_the_game() {
    let mut seen = 0;
    rigs("0x142760", 40, 60, |rig, r, n| {
        let c0 = rig.collider(r);
        rig.put(&c0);
        rig.ee.w32(OBJ_AT + 0x3c, HIT_AT);
        let count = rig.ee.call(0x142760, &[OBJ_AT as u64, COLL_AT as u64], &[], STACK).unwrap() as u16;
        let game_hits: Vec<u16> = (0..count).map(|i| u16::from_le_bytes(rig.ee.read::<2>(HIT_AT + 2 * i as u32))).collect();
        let mine = rig.oct.vertical_probe(&c0);
        assert_eq!(game_hits, mine, "case {n}: probe");
        seen += !mine.is_empty() as usize;
    });
    assert!(seen > 200, "only {seen} probes found a polygon");
}

/// orig 0x1425a0: the closest floor among the polygons the vertical probe collected.
#[test]
fn orig_1425a0_matches_the_game() {
    let mut found = 0;
    rigs("0x1425a0", 40, 60, |rig, r, n| {
        let c0 = rig.collider(r);
        rig.put(&c0);
        rig.ee.w32(OBJ_AT + 0x3c, HIT_AT);
        let count = rig.ee.call(0x142760, &[OBJ_AT as u64, COLL_AT as u64], &[], STACK).unwrap() as u16;
        let mine = rig.oct.vertical_probe(&c0);
        rig.ee.w32(OUT_AT, 0x1234_5678);
        let idx = rig.ee.call(0x1425a0, &[OBJ_AT as u64, OUT_AT as u64, count as u64, COLL_AT as u64], &[], STACK).unwrap() as u16;
        let game_y = rig.ee.f32_at(OUT_AT);
        let (my_idx, my_y) = rig.oct.closest_floor(&mine, &c0);
        assert_eq!(idx, my_idx, "case {n}: closest floor index");
        if let Some(y) = my_y {
            assert!(feq(game_y, y), "case {n}: floor y game {game_y} port {y}");
            found += 1;
        } else {
            assert_eq!(rig.ee.r32(OUT_AT), 0x1234_5678, "case {n}: the game wrote a height the port did not");
        }
    });
    assert!(found > 200, "only {found} closest floors");
}

/// orig 0x141910: taking a floor polygon into the collider, in both modes of flag bit 0.
#[test]
fn orig_141910_matches_the_game() {
    rigs("0x141910", 30, 80, |rig, r, n| {
        let c0 = rig.collider(r);
        rig.put(&c0);
        let poly = if r.below(6) == 0 { 0xffff } else { r.below(rig.polys as u64) as u16 };
        let hit = [urange(r, -600.0, 600.0), urange(r, -150.0, 150.0), urange(r, -600.0, 600.0), 1.0];
        for (k, f) in hit.iter().enumerate() {
            rig.ee.wf32(OUT_AT + 4 * k as u32, *f);
        }
        rig.ee.call(0x141910, &[OBJ_AT as u64, poly as u64, COLL_AT as u64, OUT_AT as u64], &[], STACK).unwrap();
        let game = rig.get();
        let mut mine = c0.clone();
        rig.oct.accept_floor(&mut mine, poly, hit);
        if let Some(d) = coll_diff(&game, &mine) {
            panic!("case {n}: {d} (poly {poly:#x}, flags {:#x})", c0.flags());
        }
    });
}

/// orig 0x141f08: the ray cast under each attribute filter.
#[test]
fn orig_141f08_matches_the_game() {
    let mut hits = 0;
    rigs("0x141f08", 40, 60, |rig, r, n| {
        let filt = if r.below(2) == 0 { Filter::WALK } else { Filter::ALL };
        let kind = r.below(16) as u32;
        let near = rig.centres[r.below(rig.polys as u64) as usize];
        let p0 = [near[0] + urange(r, -200.0, 200.0), near[1] + urange(r, -300.0, 0.0), near[2] + urange(r, -200.0, 200.0), 1.0];
        let p1 = [near[0] + urange(r, -200.0, 200.0), near[1] + urange(r, 0.0, 300.0), near[2] + urange(r, -200.0, 200.0), 1.0];
        rig.ee.call(0x141b30, &[OBJ_AT as u64, filt.mask as u64, filt.value as u64, filt.invert as u64, 0], &[], STACK).unwrap();
        rig.ee.w32(OBJ_AT + 0x3c, HIT_AT);
        for (k, f) in p0.iter().chain(&p1).enumerate() {
            rig.ee.wf32(OUT_AT + 4 * k as u32, *f);
        }
        rig.ee.write(OUT_AT + 0x40, &[0, 0, 0, 0]);
        let ret = rig.ee.call(0x141f08, &[OBJ_AT as u64, OUT_AT as u64, (OUT_AT + 0x60) as u64, (OUT_AT + 0x40) as u64, kind as u64], &[], STACK).unwrap();
        let game_poly = u16::from_le_bytes(rig.ee.read::<2>(OUT_AT + 0x40));
        let (hit, poly) = rig.oct.ray(filt, p0, p1, kind);
        assert_eq!(game_poly, poly, "case {n}: polygon");
        assert_eq!(ret & 1, hit.is_some() as u64, "case {n}: return");
        if let Some(h) = hit {
            hits += 1;
            for (k, f) in h.iter().enumerate().take(3) {
                let g = rig.ee.f32_at(OUT_AT + 0x60 + 4 * k as u32);
                assert!(feq(g, *f), "case {n}: hit lane {k} game {g} port {f}");
            }
        }
    });
    assert!(hits > 100, "only {hits} ray hits");
}

/// orig 0x143840 (the full walking step on a map with no moving collision objects).
#[test]
fn orig_143840_matches_the_game() {
    let mut contacts = 0;
    let mut floors = 0;
    rigs("0x143840", 40, 60, |rig, r, n| {
        let c0 = rig.collider(r);
        rig.put(&c0);
        let ret = rig.ee.call(0x143840, &[COLL_AT as u64], &[], STACK).unwrap();
        let game = rig.get();
        let mut mine = c0.clone();
        let out = rig.oct.walk_step(&mut mine);
        if let Some(d) = coll_diff(&game, &mine) {
            panic!("case {n}: {d}\nstart {:?} delta {:?} r {} lift {} flags {:#x}", c0.v4(0), c0.v4(0x10), c0.f32_at(0x20), c0.f32_at(0x6c), c0.flags());
        }
        assert_eq!(ret & 1, out, "case {n}: return");
        contacts += (game.flags() >> 34 & 1) as usize;
        floors += (game.flags() >> 33 & 1 == 0) as usize;
    });
    assert!(contacts > 100 && floors > 100, "contacts {contacts}, floors {floors}");
}

// ---------------------------------------------------------------------------------------------------------
// The sphere and ray helpers called directly (the sweep above covers them only through the whole solver)
// ---------------------------------------------------------------------------------------------------------

const LAB_AT: u32 = 0x01a0_0000;
const LAB_VERTS: u32 = LAB_AT + 0x100;
const LAB_P: u32 = LAB_AT + 0x200;
const LAB_PLANE: u32 = LAB_AT + 0x240;
const LAB_OUT: u32 = LAB_AT + 0x280;
const LAB_RAY: u32 = LAB_AT + 0x2c0;
const LAB_VTX: u32 = LAB_AT + 0x400;

struct Lab {
    ee: Ee,
}

impl Lab {
    fn new(what: &str) -> Option<Lab> {
        let elf = kh2_factory::exe_or_skip(what)?;
        let mut ee = Ee::new();
        ee.load_elf(&elf).expect("elf");
        Some(Lab { ee })
    }
    fn put(&mut self, at: u32, v: V4) {
        for (k, f) in v.iter().enumerate() {
            self.ee.wf32(at + 4 * k as u32, *f);
        }
    }
    fn get(&mut self, at: u32) -> V4 {
        std::array::from_fn(|k| self.ee.f32_at(at + 4 * k as u32))
    }
    /// The polygon record `FUN_001418a0` produces: four vertex pointers and the count.
    fn put_verts(&mut self, at: u32, v: &Verts) {
        for k in 0..4 {
            let p = LAB_VTX + 16 * k as u32;
            self.put(p, v.v[k]);
            self.ee.w32(at + 4 * k as u32, p);
        }
        self.ee.w32(at + 0x10, v.n as u32);
    }
    fn call(&mut self, addr: u32, args: &[u64]) -> u64 {
        self.ee.call(addr, args, &[], STACK).unwrap_or_else(|f| panic!("the original faulted at {addr:#x}: {f:?}"))
    }
}

fn v3(r: &mut Rng, span: f32) -> V4 {
    [urange(r, -span, span), urange(r, -span, span), urange(r, -span, span), urange(r, -2.0, 2.0)]
}

/// A random triangle or quad, roughly planar, a few tens to hundreds of units across.
fn rand_verts(r: &mut Rng) -> Verts {
    let a = v3(r, 150.0);
    let u = v3(r, 200.0);
    let w = v3(r, 200.0);
    let jitter = if r.below(2) == 0 { 0.0 } else { 6.0 };
    let mut v = [[0.0; 4]; 4];
    let pts = [a, add(a, u), add(add(a, u), w), add(a, w)];
    for k in 0..4 {
        v[k] = pts[k];
        if jitter != 0.0 {
            for lane in v[k].iter_mut().take(3) {
                *lane += urange(r, -jitter, jitter);
            }
        }
    }
    let n = if r.below(3) == 0 { 3 } else { 4 };
    Verts { v, n }
}

fn unit_plane(r: &mut Rng, v: &Verts) -> V4 {
    let e1 = sub(v.v[1], v.v[0]);
    let e2 = sub(v.v[2], v.v[0]);
    let n = norm3(cross3([e1[0], e1[1], e1[2]], [e2[0], e2[1], e2[2]]));
    let sign = if r.below(2) == 0 { 1.0 } else { -1.0 };
    let n = [n[0] * sign, n[1] * sign, n[2] * sign];
    [n[0], n[1], n[2], -(n[0] * v.v[0][0] + n[1] * v.v[0][1] + n[2] * v.v[0][2])]
}

/// A point near the polygon (inside its span, off its plane by up to `off`).
fn point_near(r: &mut Rng, v: &Verts, plane: V4, off: f32) -> V4 {
    let mut c = [0.0; 4];
    for p in v.v.iter().take(v.n) {
        for k in 0..3 {
            c[k] += p[k] / v.n as f32;
        }
    }
    let t = urange(r, -off, off);
    let mut o = [c[0] + urange(r, -60.0, 60.0), c[1] + urange(r, -60.0, 60.0), c[2] + urange(r, -60.0, 60.0), 1.0];
    for k in 0..3 {
        o[k] += plane[k] * t;
    }
    o
}

fn same4(a: V4, b: V4, lanes: usize) -> bool {
    (0..lanes).all(|k| feq(a[k], b[k]))
}

fn lab_cases(what: &str, n: usize, mut f: impl FnMut(&mut Lab, &mut Rng, usize)) {
    let Some(mut lab) = Lab::new(what) else { return };
    let mut r = Rng::new(0x1ab0 ^ n as u64);
    for k in 0..n {
        f(&mut lab, &mut r, k);
    }
}

/// orig 0x1625e0
#[test]
fn orig_1625e0_matches_the_game() {
    let (mut yes, mut no) = (0, 0);
    lab_cases("0x1625e0", 3000, |lab, r, n| {
        let v = rand_verts(r);
        let plane = unit_plane(r, &v);
        let p = point_near(r, &v, plane, 120.0);
        let radius = urange(r, 5.0, 90.0);
        lab.put(LAB_P, p);
        lab.ee.wf32(LAB_P + 0x10, radius);
        lab.put(LAB_PLANE, plane);
        lab.ee.wf32(LAB_OUT, 12345.0);
        let ret = lab.call(0x1625e0, &[LAB_P as u64, LAB_PLANE as u64, LAB_OUT as u64]);
        let mine = plane_gap(p, plane, radius);
        assert_eq!(ret & 1, mine.is_some() as u64, "case {n}");
        let d = dot3(p, plane) + plane[3];
        assert!(feq(lab.ee.f32_at(LAB_OUT), d), "case {n}: gap {} vs {d}", lab.ee.f32_at(LAB_OUT));
        if mine.is_some() {
            yes += 1
        } else {
            no += 1
        }
    });
    assert!(yes > 300 && no > 300, "{yes} / {no}");
}

/// orig 0x162678
#[test]
fn orig_162678_matches_the_game() {
    let (mut inside, mut outside) = (0, 0);
    lab_cases("0x162678", 3000, |lab, r, n| {
        let v = rand_verts(r);
        let plane = unit_plane(r, &v);
        let p = point_near(r, &v, plane, 30.0);
        let gap = urange(r, 0.0, 40.0);
        lab.put_verts(LAB_VERTS, &v);
        lab.put(LAB_P, p);
        lab.put(LAB_PLANE, plane);
        lab.ee.wf32(LAB_OUT + 0x20, gap);
        let ret = lab.call(0x162678, &[LAB_P as u64, LAB_VERTS as u64, LAB_PLANE as u64, (LAB_OUT + 0x20) as u64, LAB_OUT as u64]);
        let game = lab.get(LAB_OUT);
        let (ok, contact) = face_contact(p, &v, plane, gap);
        assert_eq!(ret & 1, ok as u64, "case {n}");
        // The w lane is not compared (the VU ops write xyz only).
        assert!(same4(game, contact, 3), "case {n}: contact {game:?} vs {contact:?}");
        if ok {
            inside += 1
        } else {
            outside += 1
        }
    });
    assert!(inside > 300 && outside > 300, "{inside} / {outside}");
}

/// orig 0x162028
#[test]
fn orig_162028_matches_the_game() {
    let (mut inside, mut outside) = (0, 0);
    lab_cases("0x162028", 3000, |lab, r, n| {
        let v = rand_verts(r);
        let plane = unit_plane(r, &v);
        let p = point_near(r, &v, plane, 8.0);
        lab.put_verts(LAB_VERTS, &v);
        lab.put(LAB_P, p);
        lab.put(LAB_PLANE, plane);
        let ret = lab.call(0x162028, &[LAB_VERTS as u64, LAB_P as u64, LAB_PLANE as u64]);
        let ok = inside_edges(&v, p, plane);
        assert_eq!(ret & 1, ok as u64, "case {n}");
        if ok {
            inside += 1
        } else {
            outside += 1
        }
    });
    assert!(inside > 300 && outside > 300, "{inside} / {outside}");
}

/// orig 0x161ed0
#[test]
fn orig_161ed0_matches_the_game() {
    let (mut inside, mut outside) = (0, 0);
    lab_cases("0x161ed0", 3000, |lab, r, n| {
        let v = rand_verts(r);
        let plane = unit_plane(r, &v);
        let p = point_near(r, &v, plane, 8.0);
        lab.put_verts(LAB_VERTS, &v);
        lab.put(LAB_P, p);
        let ret = lab.call(0x161ed0, &[LAB_VERTS as u64, LAB_P as u64]);
        let ok = inside_xz(&v, p);
        assert_eq!(ret & 1, ok as u64, "case {n}");
        if ok {
            inside += 1
        } else {
            outside += 1
        }
    });
    assert!(inside > 300 && outside > 300, "{inside} / {outside}");
}

/// orig 0x1623a0
#[test]
fn orig_1623a0_matches_the_game() {
    let (mut hit, mut miss) = (0, 0);
    lab_cases("0x1623a0", 3000, |lab, r, n| {
        let v = rand_verts(r);
        let plane = unit_plane(r, &v);
        let o = point_near(r, &v, plane, 150.0);
        let origin = [o[0], o[1], o[2], 1.0];
        let tgt = point_near(r, &v, plane, 150.0);
        let d = sub(tgt, origin);
        let dir = [d[0], d[1], d[2], 1.0];
        lab.put_verts(LAB_VERTS, &v);
        lab.put(LAB_RAY, origin);
        lab.put(LAB_RAY + 16, dir);
        lab.put(LAB_PLANE, plane);
        lab.put(LAB_OUT, [7.0, 8.0, 9.0, 10.0]);
        let ret = lab.call(0x1623a0, &[LAB_RAY as u64, LAB_VERTS as u64, LAB_PLANE as u64, LAB_OUT as u64]);
        let game = lab.get(LAB_OUT);
        let (ok, point) = ray_triangle(origin, dir, &v, plane);
        assert_eq!(ret & 1, ok as u64, "case {n}");
        assert!(same4(game, point.unwrap_or([7.0, 8.0, 9.0, 10.0]), 4), "case {n}: out {game:?} vs {point:?}");
        if ok {
            hit += 1
        } else {
            miss += 1
        }
    });
    assert!(hit > 150 && miss > 300, "{hit} / {miss}");
}

/// orig 0x1626f0
#[test]
fn orig_1626f0_matches_the_game() {
    let (mut edge, mut none) = (0, 0);
    lab_cases("0x1626f0", 4000, |lab, r, n| {
        let v = rand_verts(r);
        let plane = unit_plane(r, &v);
        let p = point_near(r, &v, plane, 70.0);
        let radius = urange(r, 5.0, 90.0);
        lab.put_verts(LAB_VERTS, &v);
        lab.put(LAB_P, p);
        lab.ee.wf32(LAB_P + 0x10, radius);
        lab.put(LAB_OUT, [7.0, 8.0, 9.0, 10.0]);
        let ret = lab.call(0x1626f0, &[LAB_P as u64, LAB_VERTS as u64, LAB_OUT as u64]);
        let game = lab.get(LAB_OUT);
        let (hit, out) = edge_contact_out(&Sphere { c: p, r: radius }, &v);
        assert_eq!(ret & 1, hit.is_some() as u64, "case {n}");
        assert!(same4(game, out, 4), "case {n}: out {game:?} vs {out:?}");
        if hit.is_some() {
            edge += 1
        } else {
            none += 1
        }
    });
    assert!(edge > 400 && none > 400, "{edge} / {none}");
}

/// orig 0x163248
#[test]
fn orig_163248_matches_the_game() {
    let mut seen = [0; 3];
    lab_cases("0x163248", 3000, |lab, r, n| {
        let v = rand_verts(r);
        let plane = unit_plane(r, &v);
        let off = if r.below(3) == 0 { 0.002 } else { 40.0 };
        let p = point_near(r, &v, plane, off);
        lab.put(LAB_P, p);
        lab.put(LAB_OUT, v.v[0]);
        lab.put(LAB_PLANE, plane);
        let ret = lab.call(0x163248, &[LAB_P as u64, LAB_OUT as u64, LAB_PLANE as u64]) as u32;
        let d = dot3(sub(v.v[0], p), plane);
        let mine = plane_side(p, v.v[0], plane);
        assert_eq!(ret, mine, "case {n}: gap {d}");
        seen[mine as usize] += 1;
    });
    assert!(seen[0] > 300 && seen[1] > 300, "{seen:?}");
}

/// orig 0x330c80
#[test]
fn orig_330c80_matches_the_game() {
    lab_cases("0x330c80", 2000, |lab, r, n| {
        let mut c = Collider::default();
        for b in c.0.iter_mut() {
            *b = r.word() as u8;
        }
        let kind = pick(r, &[0u64, 0, 8, 0x30]);
        c.set_flags(c.flags() & !0x78 | kind);
        let attr = pick(r, &[0u32, 0x800, 0x1000, 0x1800, 0x10, 0x1810]);
        let poly = r.below(300) as u16;
        let contact = v3(r, 100.0);
        lab.ee.write(COLL_AT, &c.0);
        lab.ee.w32(OUT_AT, attr);
        lab.put(OUT_AT + 0x10, contact);
        lab.call(0x330c80, &[OBJ_AT as u64, COLL_AT as u64, OUT_AT as u64, poly as u64, (OUT_AT + 0x10) as u64]);
        let game = Collider(lab.ee.read::<{ coll::SIZE }>(COLL_AT));
        let mut mine = c.clone();
        hit_callback(&mut mine, attr, poly, contact);
        assert_eq!(game.0, mine.0, "case {n}");
    });
}

impl Rig {
    fn put_ray(&mut self, origin: V4, dir: V4) {
        for (k, f) in origin.iter().chain(dir.iter()).enumerate() {
            self.ee.wf32(LAB_RAY + 4 * k as u32, *f);
        }
    }
}

/// orig 0x141d90 (the nearest hit among a list of polygons) with the polygon accessors 0x1418a0, 0x142190 and
/// 0x1422b0 checked over every polygon of the object.
#[test]
fn orig_141d90_matches_the_game() {
    let mut found = 0;
    rigs("0x141d90", 40, 60, |rig, r, n| {
        if n % 60 == 0 {
            for p in 0..rig.polys as u16 {
                let rec = rig.ee.r32(OBJ_AT + 0xc) + 16 * p as u32;
                rig.ee.call(0x1418a0, &[OBJ_AT as u64, rec as u64, OUT_AT as u64], &[], STACK).unwrap();
                let verts = rig.oct.poly_verts(p);
                assert_eq!(rig.ee.r32(OUT_AT + 0x10) as usize, verts.n, "poly {p}: vertex count");
                for k in 0..verts.n {
                    let ptr = rig.ee.r32(OUT_AT + 4 * k as u32);
                    let g = std::array::from_fn(|l| rig.ee.f32_at(ptr + 4 * l as u32));
                    assert!(same4(g, verts.v[k], 4), "poly {p} vertex {k}");
                }
                let plane_ptr = rig.ee.call(0x142190, &[OBJ_AT as u64, p as u64], &[], STACK).unwrap() as u32;
                let g: V4 = std::array::from_fn(|l| rig.ee.f32_at(plane_ptr + 4 * l as u32));
                assert!(same4(g, rig.oct.plane(p), 4), "poly {p}: plane");
                let attr_ptr = rig.ee.call(0x1422b0, &[OBJ_AT as u64, p as u64], &[], STACK).unwrap() as u32;
                assert_eq!(rig.ee.r32(attr_ptr), rig.oct.attr(p), "poly {p}: attribute");
            }
        }
        let near = rig.centres[r.below(rig.polys as u64) as usize];
        let origin = [near[0] + urange(r, -150.0, 150.0), near[1] + urange(r, -250.0, 0.0), near[2] + urange(r, -150.0, 150.0), 1.0];
        let d = [urange(r, -100.0, 100.0), urange(r, 100.0, 600.0), urange(r, -100.0, 100.0), 1.0];
        let all: Vec<u16> = (0..rig.polys as u16).filter(|_| r.below(4) != 0).collect();
        for (i, p) in all.iter().enumerate() {
            rig.ee.write(HIT_AT + 2 * i as u32, &p.to_le_bytes());
        }
        rig.ee.w32(OBJ_AT + 0x3c, HIT_AT);
        rig.put_ray(origin, d);
        rig.ee.write(OUT_AT + 0x40, &[0; 16]);
        let ret = rig.ee.call(0x141d90, &[OBJ_AT as u64, LAB_RAY as u64, all.len() as u64, (OUT_AT + 0x40) as u64], &[], STACK).unwrap() as u32;
        let mut best = None;
        let mut best_poly = 0xffffu32;
        let mut best_d2 = 1.0e14f32;
        for &p in &all {
            let verts = rig.oct.poly_verts(p);
            let plane = rig.oct.plane(p);
            if let (true, Some(h)) = ray_triangle(origin, d, &verts, plane) {
                let off = sub(h, origin);
                let d2 = dot3(off, off);
                if d2 < best_d2 {
                    best_d2 = d2;
                    best = Some(h);
                    best_poly = p as u32;
                }
            }
        }
        assert_eq!(ret, best_poly, "case {n}");
        if let Some(h) = best {
            let g: V4 = std::array::from_fn(|l| rig.ee.f32_at(OUT_AT + 0x40 + 4 * l as u32));
            assert!(same4(g, h, 4), "case {n}: hit {g:?} vs {h:?}");
            found += 1;
        }
    });
    assert!(found > 100, "only {found} hits");
}

/// orig 0x1632b8: the solver on a list of collected polygons, called directly; its output record is `coll +0x30`
/// as `FUN_00163f98` passes it.
#[test]
fn orig_1632b8_matches_the_game() {
    let mut pushed = 0;
    rigs("0x1632b8", 40, 60, |rig, r, n| {
        let c0 = rig.collider(r);
        let near = rig.centres[r.below(rig.polys as u64) as usize];
        let radius = urange(r, 20.0, 110.0);
        let centre = [near[0] + urange(r, -120.0, 120.0), near[1] + urange(r, -120.0, 60.0), near[2] + urange(r, -120.0, 120.0), 1.0];
        let all: Vec<u16> = (0..rig.polys as u16).filter(|_| r.below(5) != 0).collect();
        for (i, p) in all.iter().enumerate() {
            rig.ee.write(HIT_AT + 2 * i as u32, &p.to_le_bytes());
        }
        rig.ee.w32(OBJ_AT + 0x3c, HIT_AT);
        rig.put(&c0);
        rig.put_ray(centre, [radius, 0.0, 0.0, 0.0]);
        let h = radius * 1.5;
        let sbox = fbox(centre[0] - h, centre[1] - h, centre[2] - h, centre[0] + h, centre[1] + h, centre[2] + h);
        for (k, f) in sbox.iter().enumerate() {
            rig.ee.wf32(LAB_PLANE + 4 * k as u32, *f);
        }
        let ret = rig.ee.call(0x1632b8, &[OBJ_AT as u64, all.len() as u64, LAB_RAY as u64, LAB_PLANE as u64, (COLL_AT + 0x30) as u64, COLL_AT as u64], &[], STACK).unwrap() as i32;
        let game = rig.get();
        let game_centre = [rig.ee.f32_at(LAB_RAY), rig.ee.f32_at(LAB_RAY + 4), rig.ee.f32_at(LAB_RAY + 8), 0.0];
        let mut mine = c0.clone();
        let mut s = Sphere { c: centre, r: radius };
        let res = rig.oct.solve(&mut mine, &mut s, &all, &sbox);
        assert_eq!(ret, res, "case {n}: contacts");
        assert!(same4(game_centre, [s.c[0], s.c[1], s.c[2], 0.0], 3), "case {n}: centre {game_centre:?} vs {:?}", s.c);
        if let Some(d) = coll_diff(&game, &mine) {
            panic!("case {n}: {d}");
        }
        pushed += (res > 0) as usize;
    });
    assert!(pushed > 150, "only {pushed} pushed cases");
}

/// orig 0x163f98: one sweep step (the solver repeated while it keeps finding contacts), called directly.
#[test]
fn orig_163f98_matches_the_game() {
    let mut contact = 0;
    rigs("0x163f98", 40, 60, |rig, r, n| {
        let c0 = rig.collider(r);
        let near = rig.centres[r.below(rig.polys as u64) as usize];
        let radius = c0.f32_at(coll::RADIUS);
        let centre = [near[0] + urange(r, -120.0, 120.0), near[1] + urange(r, -120.0, 60.0), near[2] + urange(r, -120.0, 120.0), 1.0];
        let prev = [centre[0] + urange(r, -30.0, 30.0), centre[1], centre[2] + urange(r, -30.0, 30.0), 1.0];
        let all: Vec<u16> = (0..rig.polys as u16).filter(|_| r.below(5) != 0).collect();
        for (i, p) in all.iter().enumerate() {
            rig.ee.write(HIT_AT + 2 * i as u32, &p.to_le_bytes());
        }
        rig.ee.w32(OBJ_AT + 0x3c, HIT_AT);
        rig.put(&c0);
        rig.put_ray(centre, [radius, 0.0, 0.0, 0.0]);
        rig.put_prev(prev);
        rig.ee.w32(LAB_OUT, 0);
        let half = radius * 1.5;
        // The float parameter travels in f12 on its own: the integer arguments start at a0.
        rig.ee.call(0x163f98, &[OBJ_AT as u64, COLL_AT as u64, LAB_RAY as u64, all.len() as u64, LAB_OUT as u64, (LAB_OUT + 0x80) as u64], &[half], STACK).unwrap();
        let game = rig.get();
        let game_depth = rig.ee.r32(LAB_OUT) as i32;
        let mut mine = c0.clone();
        let mut s = Sphere { c: centre, r: radius };
        let mut depth = 0;
        rig.oct.step_sphere(half, &mut mine, &mut s, &all, &mut depth, prev);
        if let Some(d) = coll_diff(&game, &mine) {
            panic!("case {n}: {d}
depth game {game_depth} port {depth}; centre {centre:?} prev {prev:?} r {radius}; flags {:#x} end0 {:?}; game end {:?} port end {:?}; hits {all:?}", c0.flags(), c0.v4(0x30), game.v4(0x30), mine.v4(0x30));
        }
        contact += (game.flags() >> 34 & 1) as usize;
    });
    assert!(contact > 100, "only {contact} contacts");
}

impl Rig {
    fn put_prev(&mut self, v: V4) {
        for (k, f) in v.iter().enumerate() {
            self.ee.wf32(LAB_OUT + 0x80 + 4 * k as u32, *f);
        }
    }
}

// ---------------------------------------------------------------------------------------------------------
// Real maps
// ---------------------------------------------------------------------------------------------------------

/// The walking step on the collision of real areas, with Sora's collider (radius 90, lift 115, height 90): a start
/// on a random floor triangle, a run-sized move in a random direction, kind 0 in both solver modes.
#[test]
fn orig_143840_matches_the_game_on_real_maps() {
    let Some(elf) = kh2_factory::exe_or_skip("0x143840 on real maps") else { return };
    let Some(iso) = kh2_factory::disc_path("0x143840 on real maps") else { return };
    let disc = kh2_game_core::assets::Disc::open(std::path::Path::new(&iso));
    let mut r = Rng::new(0x7ea1);
    let (mut cases, mut contacts, mut areas) = (0, 0, 0);
    for area in ["tt00", "tt04", "tt07", "tt21", "he05", "al01", "eh10", "hb00", "nm00", "bb00"] {
        let Some(data) = disc.read(&format!("map/us/{area}.map")) else { continue };
        let Some(entry) = kh2_bar::parse(&data).into_iter().flatten().find(|e| e.kind == 6 && kh2_map::Coct::parse(e.data).is_some()) else { continue };
        let coct = kh2_map::Coct::parse(entry.data).unwrap();
        let floors: Vec<_> = coct.polygons.iter().filter(|p| p.normal[1] > 0.5).flat_map(|p| p.triangles()).collect();
        if floors.is_empty() {
            continue;
        }
        areas += 1;
        for groups in [u64::MAX, 0x3f] {
            let mut rig = Rig::from_file(&elf, entry.data, groups, vec![[0.0; 3]]);
            for n in 0..120 {
                let t = floors[r.below(floors.len() as u64) as usize];
                let (u, v) = (urange(&mut r, 0.0, 1.0), urange(&mut r, 0.0, 1.0));
                let (u, v) = if u + v > 1.0 { (1.0 - u, 1.0 - v) } else { (u, v) };
                let at: [f32; 3] = std::array::from_fn(|k| t[0][k] + u * (t[1][k] - t[0][k]) + v * (t[2][k] - t[0][k]));
                // The game's space: y down, z flipped.
                let feet = [at[0], -at[1] - if r.below(4) == 0 { urange(&mut r, 0.0, 60.0) } else { 0.0 }, -at[2], 1.0];
                let mut c = Collider::default();
                let ang = urange(&mut r, 0.0, std::f32::consts::TAU);
                let len = pick(&mut r, &[0.0, 8.0, 20.0, 60.0, 150.0]);
                c.set_v4(coll::START, feet);
                c.set_v4(coll::DELTA, [ang.cos() * len, if r.below(4) == 0 { urange(&mut r, -30.0, 30.0) } else { 0.0 }, ang.sin() * len, 1.0]);
                c.set_f32(coll::RADIUS, 90.0);
                c.set_f32(coll::HEIGHT, 90.0);
                c.set_f32(coll::LIFT, 115.0);
                c.set_flags(r.below(2) << 1 | (r.word() & 0xffff_ffff_0000_0000 & !(0xfu64 << 32)));
                rig.put(&c);
                let ret = rig.ee.call(0x143840, &[COLL_AT as u64], &[], STACK).unwrap();
                let game = rig.get();
                let mut mine = c.clone();
                let out = rig.oct.walk_step(&mut mine);
                if let Some(d) = coll_diff(&game, &mine) {
                    panic!("{area} groups {groups:#x} case {n}: {d}
start {:?} delta {:?} flags {:#x}", c.v4(0), c.v4(0x10), c.flags());
                }
                assert_eq!(ret & 1, out, "{area} case {n}: return");
                cases += 1;
                contacts += (game.flags() >> 34 & 1) as usize;
            }
        }
    }
    eprintln!("{areas} areas, {cases} cases, {contacts} with a contact");
    assert!(areas >= 3 && contacts > 50, "{areas} areas, {contacts} contacts");
}

const FILE2_AT: u32 = 0x0140_0000;
const OBJ2_AT: u32 = 0x0199_0000;

impl Rig {
    /// A second built object (the floor colour map and floor kind slots hold objects of their own).
    fn aux(&mut self, r: &mut Rng) -> Oct {
        let n = 4 + r.below(8) as usize;
        let polys: Vec<Poly> = (0..n).map(|_| random_poly(r, 300.0)).collect();
        let level = r.below(4) as u32;
        let file = build_file(r, &polys, level);
        self.ee.write(FILE2_AT, &file);
        let groups = 0xfu64;
        self.ee.call(0x141500, &[OBJ2_AT as u64, FILE2_AT as u64, groups], &[], STACK).expect("FUN_00141500");
        Oct::new(&file, groups).expect("port object")
    }
}

/// orig 0x1434b8 (the floor kind probe) and 0x143560 (the floor query), with the optional colour map and kind
/// slots empty, filled with one object, or both.
#[test]
fn orig_143560_matches_the_game() {
    let (mut with_floor, mut kinds_seen, mut colours_seen) = (0, 0, 0);
    rigs("0x143560", 40, 60, |rig, r, n| {
        let aux = if n % 60 == 0 { Some(rig.aux(r)) } else { None };
        // The aux object is rebuilt once per world and kept in a static-like slot through the loop index.
        thread_local!(static AUX: std::cell::RefCell<Option<Oct>> = const { std::cell::RefCell::new(None) });
        if let Some(a) = aux {
            AUX.with(|s| *s.borrow_mut() = Some(a));
        }
        let aux = AUX.with(|s| s.borrow().clone()).unwrap();
        let mut c0 = rig.collider(r);
        c0.set_u32(coll::MOVING, 0);
        rig.put(&c0);
        // The floor ray fills coll +0x60 the way the walking step does.
        rig.ee.call(0x1423d8, &[OBJ_AT as u64, COLL_AT as u64], &[], STACK).unwrap();
        let mut c1 = rig.get();
        c1.set_u32(coll::MOVING, 0);
        rig.put(&c1);
        let (colour_slot, kind_slot) = match n % 4 {
            0 => (0, 0),
            1 => (OBJ2_AT, 0),
            2 => (0, OBJ2_AT),
            _ => (OBJ2_AT, OBJ2_AT),
        };
        rig.ee.w32(0x35593c, colour_slot);
        rig.ee.w32(0x355944, kind_slot);
        rig.ee.write(OUT_AT, &[0xccu8; 0x40]);
        rig.ee.call(0x143560, &[COLL_AT as u64, OUT_AT as u64], &[], STACK).unwrap();
        let game: Vec<u8> = rig.ee.read::<0x40>(OUT_AT).to_vec();
        let info = rig.oct.floor_info(&c1, (colour_slot != 0).then_some(&aux), (kind_slot != 0).then_some(&aux));
        let f = |o: usize| f32::from_le_bytes(game[o..o + 4].try_into().unwrap());
        let u = |o: usize| u32::from_le_bytes(game[o..o + 4].try_into().unwrap());
        for k in 0..3 {
            assert!(feq(f(4 * k), info.normal[k]), "case {n}: normal lane {k}: {} vs {}", f(4 * k), info.normal[k]);
            assert!(feq(f(0x10 + 4 * k), info.opposite[k]), "case {n}: opposite lane {k}");
        }
        assert_eq!(u(0x24), info.poly, "case {n}: polygon");
        assert_eq!(u(0x28), info.material, "case {n}: material");
        assert_eq!(u(0x2c), info.colour_byte, "case {n}: colour byte");
        assert_eq!(u(0x30), info.kind, "case {n}: kind");
        assert_eq!(game[0x34], info.flag, "case {n}: flag");
        // The colour words at +0x20 are the colour table's and are not ported; nothing past +0x34 may change.
        assert!(game[0x35..0x40].iter().all(|&b| b == 0xcc), "case {n}: the original wrote past +0x34");
        with_floor += (info.poly != 0xffff) as usize;
        kinds_seen += (info.kind != 0) as usize;
        colours_seen += (info.colour_byte != 0) as usize;
    });
    assert!(with_floor > 100 && kinds_seen > 20 && colours_seen > 20, "{with_floor} floors, {kinds_seen} kinds, {colours_seen} colour bytes");
}

/// orig 0x144548: the bounds of a polygon's three or four vertices.
#[test]
fn orig_144548_matches_the_game() {
    lab_cases("0x144548", 3000, |lab, r, n| {
        let v = rand_verts(r);
        lab.put_verts(LAB_VERTS, &v);
        lab.ee.write(LAB_OUT, &[0xeeu8; 32]);
        lab.call(0x144548, &[LAB_OUT as u64, LAB_VERTS as u64]);
        let game: FBox = std::array::from_fn(|k| lab.ee.f32_at(LAB_OUT + 4 * k as u32));
        assert_eq!(game, fbox_of_polygon(&v.v, v.n), "case {n}");
    });
}

/// orig 0x1443a8: a box from six floats.
#[test]
fn orig_1443a8_matches_the_game() {
    static SPEC: Spec = Spec { addr: 0x1443a8, args: &[Arg::Ptr(FBOX)], floats: 6, ret: Ret::None, globals: &[], stubs: &[], nulls: false };
    let Some(st) = fuzz(&SPEC, 2000, |c| {
        let b = fbox(c.floats[0], c.floats[1], c.floats[2], c.floats[3], c.floats[4], c.floats[5]);
        put6(&mut c.blocks[0], &b);
        Out::none()
    }) else { return };
    eprintln!("{} cases equal, {} interpreter steps in {:.3} s", st.cases, st.steps, st.secs);
}

/// The roof question of WP-L1-P10: the original has no ceiling probe in its collision clusters, and a body that
/// jumps under a roof is stopped by the same sphere sweep that stops it at a wall. Here the game's own walking step
/// runs on a floor with a roof 300 above it (Sora's collider, rising 250): the sphere's top stops at the roof, one
/// radius plus 0.008 under it, and the port agrees. A roof polygon the party handler rejects (no bit 0x10) does not
/// stop the rise.
#[test]
fn a_roof_stops_a_jump_in_the_game_through_the_sweep() {
    use kh2_map::build::{BuildPoly, coct_file};
    let Some(elf) = kh2_factory::exe_or_skip("roof") else { return };
    let solid = kh2_map::flags::PARTY_COLLIDE;
    for (roof_attr, stops) in [(solid, true), (0x20u32, false)] {
        let file = coct_file(&[
            BuildPoly::new(&[[-2000.0, 0.0, -2000.0], [2000.0, 0.0, -2000.0], [2000.0, 0.0, 2000.0], [-2000.0, 0.0, 2000.0]], 1 | solid),
            BuildPoly::new(&[[-2000.0, -300.0, -2000.0], [-2000.0, -300.0, 2000.0], [2000.0, -300.0, 2000.0], [2000.0, -300.0, -2000.0]], roof_attr),
        ]);
        let mut rig = Rig::from_file(&elf, &file, 1, vec![[0.0; 3]]);
        let mut c = Collider::default();
        c.set_v4(coll::START, [0.0, 0.0, 0.0, 1.0]);
        c.set_v4(coll::DELTA, [0.0, -250.0, 0.0, 1.0]);
        c.set_v4(coll::END, [0.0, -250.0, 0.0, 1.0]);
        c.set_f32(coll::RADIUS, 90.0);
        c.set_f32(coll::HEIGHT, 90.0);
        c.set_f32(coll::LIFT, 115.0);
        // Airborne: bit 0 of the flag word (FUN_00182690 sets it for a body that is not walking), which makes the
        // floor ray report instead of snapping the end back down to the floor.
        c.set_flags(1);
        rig.put(&c);
        rig.ee.call(0x143840, &[COLL_AT as u64], &[], STACK).unwrap();
        let game = rig.get();
        let mut mine = c.clone();
        rig.oct.walk_step(&mut mine);
        assert_eq!(coll_diff(&game, &mine), None);
        let foot = game.v4(coll::END)[1];
        if stops {
            assert!((foot - (-300.0 + 90.0 + 0.008 + 115.0)).abs() < 1e-3, "the head stops at the roof: foot y {foot}");
            assert_eq!(game.flags() >> 34 & 1, 1, "a contact");
        } else {
            assert!((foot - -250.0).abs() < 1e-3, "no stop: foot y {foot}");
        }
    }
}

/// orig 0x141500 (the build), 0x141768 (the group word into the meshes' enabled bytes), 0x141840 and 0x141800 (one group
/// on or off), 0x141720 (the nibble-5 flag), 0x141888 (the box level) and 0x1421b8 (the polygon normal): the object the
/// port builds over the same file bytes agrees with the one the game built.
#[test]
fn the_built_object_matches_the_game() {
    let mut five = 0;
    rigs("0x141500", 40, 1, |rig, r, n| {
        let check = |rig: &mut Rig, what: &str| {
            let meshes = rig.ee.r32(OBJ_AT + 8);
            let count = rig.ee.r32(OBJ_AT + 0x24) as usize;
            assert_eq!(count, rig.oct.mesh_count(), "world {n}: mesh count");
            for m in 0..count {
                let on = rig.ee.r8(meshes + 0x14 * m as u32 + 0x10) != 0;
                assert_eq!(on, rig.oct.mesh_enabled(m), "world {n} {what}: mesh {m}");
            }
        };
        check(rig, "built");
        assert_eq!(rig.ee.r8(OBJ_AT + 0xd2) != 0, rig.oct.has_nibble_five(), "world {n}: nibble 5");
        assert_eq!(rig.ee.call(0x141888, &[OBJ_AT as u64], &[], STACK).unwrap() as usize, rig.oct.level(), "world {n}: level");
        five += rig.oct.has_nibble_five() as usize;
        for round in 0..6 {
            let word = r.word();
            rig.ee.call(0x141768, &[OBJ_AT as u64, word], &[], STACK).unwrap();
            rig.oct.set_groups(word);
            check(rig, "word");
            let g = r.below(70) as u8;
            let on = r.below(2) == 0;
            rig.ee.call(if on { 0x141840 } else { 0x141800 }, &[OBJ_AT as u64, g as u64], &[], STACK).unwrap();
            rig.oct.set_group(g, on);
            check(rig, "group");
            let p = r.below(rig.polys as u64) as u16;
            rig.ee.call(0x1421b8, &[OUT_AT as u64, OBJ_AT as u64, p as u64], &[], STACK).unwrap();
            let g4: V4 = std::array::from_fn(|l| rig.ee.f32_at(OUT_AT + 4 * l as u32));
            assert!(same4(g4, rig.oct.normal(p), 4), "world {n} round {round}: normal {g4:?}");
        }
    });
    assert!(five > 0, "no world had an attribute with nibble 5");
}
