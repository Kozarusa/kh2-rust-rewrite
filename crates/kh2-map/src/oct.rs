//! The game's built collision object and its queries (WP-L1-P10, spec `docs/systems/core-runtime/collision.md`
//! section 9). `FUN_00141500` builds the object from a COCT file (the map BAR entry of type 6); every
//! query below takes the object and a collider record and mirrors one original function.
//!
//! Placement. Every object carries a placement matrix (`oct +0x50`), its inverse (`+0x90`) and the identity flag
//! (`+0xd1`, WP-L1-P15). The map's own collision is the identity case and takes the original's short paths; a
//! moving collision object (the pool of `pool.rs`) maps the points of every query into its frame with the inverse
//! and the results back with the matrix, exactly where the original does. `FUN_00143840` over the moving-object
//! pool is `Oct::walk_step_with`; `walk_step` is the empty-pool case a plain field has.
//!
//! Layout. The queries read the file in place by byte offsets like the original (a polygon record is 16
//! bytes, a mesh 0x14, a tree node 0x20), the collider is the 0x88-byte record every query takes (the actor
//! keeps it at `actor +0x590`; `coll +0x20` is `actor +0x5b0`, the body radius read from the model's body
//! shape by `FUN_00169af0`, 90 for Sora).

use crate::geom::*;
use crate::pool::MovingPool;

/// Offsets inside the collider record.
pub mod coll {
    /// The start of the move (v4).
    pub const START: usize = 0x00;
    /// The move vector (v4).
    pub const DELTA: usize = 0x10;
    /// The sphere radius (float).
    pub const RADIUS: usize = 0x20;
    /// The body height (float).
    pub const HEIGHT: usize = 0x24;
    /// The end point the sweep leaves (v4).
    pub const END: usize = 0x30;
    /// The contact of a polygon with attribute bit 0x800 (v4) and its polygon (u16 at 0x62).
    pub const CONTACT_A: usize = 0x40;
    pub const POLY_A: usize = 0x62;
    /// The same for attribute bit 0x1000.
    pub const CONTACT_B: usize = 0x50;
    pub const POLY_B: usize = 0x64;
    /// The floor polygon (u16).
    pub const FLOOR_POLY: usize = 0x60;
    /// The floor height (float).
    pub const FLOOR_Y: usize = 0x68;
    /// The lift: the sphere centre rides this far above the foot (float).
    pub const LIFT: usize = 0x6c;
    /// The flag word (u64): bit 0 the point-test mode, bits 1..2 the solver mode, bits 3..6 the collision kind,
    /// bit 32 no floor ahead / too deep, bit 33 no floor polygon, bit 34 a contact happened.
    pub const FLAGS: usize = 0x70;
    /// The moving object under the foot (u32).
    pub const MOVING: usize = 0x78;
    /// The owner id (u32): moving objects of the same owner are not collided with.
    pub const OWNER: usize = 0x7c;
    /// The extra step height (float).
    pub const EXTRA: usize = 0x80;
    /// Size of the record.
    pub const SIZE: usize = 0x88;
}

/// The collider record: raw bytes with typed accessors, so fields the queries do not touch stay as they were.
#[derive(Clone, Debug, PartialEq)]
pub struct Collider(pub [u8; coll::SIZE]);

impl Default for Collider {
    fn default() -> Self {
        Collider([0; coll::SIZE])
    }
}

impl Collider {
    pub fn from_bytes(b: &[u8]) -> Collider {
        let mut c = Collider::default();
        let n = b.len().min(coll::SIZE);
        c.0[..n].copy_from_slice(&b[..n]);
        c
    }
    pub fn f32_at(&self, o: usize) -> f32 {
        f32::from_le_bytes(self.0[o..o + 4].try_into().unwrap())
    }
    pub fn set_f32(&mut self, o: usize, v: f32) {
        self.0[o..o + 4].copy_from_slice(&v.to_le_bytes());
    }
    pub fn v4(&self, o: usize) -> V4 {
        [self.f32_at(o), self.f32_at(o + 4), self.f32_at(o + 8), self.f32_at(o + 12)]
    }
    pub fn set_v4(&mut self, o: usize, v: V4) {
        for (k, f) in v.iter().enumerate() {
            self.set_f32(o + 4 * k, *f);
        }
    }
    pub fn u16_at(&self, o: usize) -> u16 {
        u16::from_le_bytes(self.0[o..o + 2].try_into().unwrap())
    }
    pub fn set_u16(&mut self, o: usize, v: u16) {
        self.0[o..o + 2].copy_from_slice(&v.to_le_bytes());
    }
    pub fn set_u32(&mut self, o: usize, v: u32) {
        self.0[o..o + 4].copy_from_slice(&v.to_le_bytes());
    }
    pub fn u32_at(&self, o: usize) -> u32 {
        u32::from_le_bytes(self.0[o..o + 4].try_into().unwrap())
    }
    /// The owner of the collider (`coll +0x7c`): the moving-object sweeps skip the objects with the same owner.
    pub fn owner(&self) -> u32 {
        self.u32_at(coll::OWNER)
    }
    pub fn flags(&self) -> u64 {
        u64::from_le_bytes(self.0[coll::FLAGS..coll::FLAGS + 8].try_into().unwrap())
    }
    pub fn set_flags(&mut self, v: u64) {
        self.0[coll::FLAGS..coll::FLAGS + 8].copy_from_slice(&v.to_le_bytes());
    }
    /// The collision kind: bits 3..6 of the flag word, the argument of the polygon walker's handler table.
    pub fn kind(&self) -> u32 {
        (self.flags() >> 3) as u32 & 0xf
    }
    /// The solver mode: bits 1..2 of the flag word.
    fn mode(&self) -> u32 {
        (self.flags() >> 1) as u32 & 3
    }
}

/// Flag bits of `coll +0x70` above 32.
pub const FLAG_NO_FLOOR_AHEAD: u64 = 1 << 32;
pub const FLAG_NO_FLOOR_POLY: u64 = 1 << 33;
pub const FLAG_CONTACT: u64 = 1 << 34;

/// The polygon walker's attribute filter.
///
/// orig 0x141b30: sets `oct +0x40` (mask), `+0x44` (value), `+0xd0` (invert) and `+0xd3` (the nibble-5 exception).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Filter {
    pub mask: u32,
    pub value: u32,
    pub invert: bool,
    pub b: bool,
}

impl Filter {
    /// No attribute filter (the vertical probe, most rays).
    pub const ALL: Filter = Filter { mask: 0, value: 0, invert: false, b: false };
    /// The walking filter armed by the sphere sweep and the floor ray: the attribute's low nibble is not 4.
    pub const WALK: Filter = Filter { mask: 0xf, value: 4, invert: true, b: false };
}

/// orig 0x32e3b8: does the handler of collision kind `kind` accept an attribute word? (The table at 0x378f7c;
/// code 0x32e3dc..0x32e44c. [inferred] that the attribute reaches the handler as its third argument: the
/// oracle test fixes it.)
pub fn kind_accepts(kind: u32, attr: u32) -> bool {
    match kind {
        0 | 6 => attr & 0x10 != 0,
        1 => attr & 0x20 != 0,
        2 => attr & 0x40 != 0,
        3 => attr & 0x10 != 0 || attr & 0x80000 != 0,
        4 => attr & 0x80 != 0,
        5 => attr & 0x100 != 0,
        _ => true,
    }
}

/// A polygon's vertices (orig 0x1418a0): three or four.
#[derive(Clone, Copy, Debug)]
pub struct Verts {
    pub v: [V4; 4],
    pub n: usize,
}

/// The floor facts `FUN_00143560` writes into the actor's ground record (`out +0x00..0x34`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FloorInfo {
    /// `out +0x00`: the floor normal (straight up, `(0, -1, 0)`, without a floor polygon).
    pub normal: V4,
    /// `out +0x10`: the negated normal.
    pub opposite: V4,
    /// `out +0x24`: the floor polygon, 0xffff for none.
    pub poly: u32,
    /// `out +0x28`: byte 0 of the polygon record (the footstep and dust material).
    pub material: u32,
    /// `out +0x2c`: byte 1 of the floor colour map polygon.
    pub colour_byte: u32,
    /// Byte 0 of the floor colour map polygon (indexes the colour table that fills `out +0x20`).
    pub colour_class: u8,
    /// `out +0x30`: the floor kind of `Oct::floor_kind`.
    pub kind: u32,
    /// `out +0x34`: attribute bit 16 of the floor polygon.
    pub flag: u8,
}

/// The sphere the solver pushes out of polygons: centre (v4) and radius.
#[derive(Clone, Copy, Debug)]
pub struct Sphere {
    pub c: V4,
    pub r: f32,
}

// Constants of the executable's data (small rule values; read from the user's executable and checked by the
// oracle tests): 0x378f50.. and 0x379500..
const ONE: f32 = 1.0;
const FAR: f32 = 1.0e14;
const FLOOR_NONE: f32 = 320_000.0;
const PROBE_DEPTH: f32 = 320_000.0;
const CLOSE_LIMIT: f32 = 1.0e7;
const RAY_CLAMP: f32 = 100.0;
const RAY_LIFTS: f32 = 3.0;
const EDGE_EPS: f32 = 0.008;
const PUSH_EPS: f32 = 0.008;
const PLANE_SIDE: f32 = 0.001;
const INSIDE_EPS: f32 = 0.01;
const SWEEP_BOX: f32 = 1.5;
/// 0x378f78: a placement basis row shorter than this (squared) collapses the object.
const COLLAPSE_LIMIT: f32 = 0.01;
/// 0x379538: the w lane of the sweep's end point.
const SWEEP_END_W: f32 = ONE;
/// 0x378fc8 and 0x37955c: the floor height the walking step starts its moving-object floor with (they hold the same
/// value, so the step's "floor changed" test fires only when a moving floor was found).
pub(crate) const FLOOR_OUT_NONE: f32 = 10_000_000.0;
/// 0x379550: the start of the nearest-foot search of `nearest_edge`.
const NEAREST_EDGE_START: f32 = f32::from_bits(0x56b5_e620);
/// 0x379540 and 0x379544: the w lane and the scale of `edge_direction_gap`.
const EDGE_DIR_W: f32 = 1.0;
const EDGE_DIR_SCALE: f32 = 10.0;
const NEXT3: [usize; 3] = [1, 2, 0];
const NEXT4: [usize; 4] = [1, 2, 3, 0];

fn next_table(n: usize) -> &'static [usize] {
    if n == 3 { &NEXT3 } else { &NEXT4 }
}

/// The built collision object (orig 0x141500) over a COCT file image.
#[derive(Clone, Debug)]
pub struct Oct {
    file: Vec<u8>,
    nodes: usize,
    meshes: usize,
    polys: usize,
    verts: usize,
    planes: usize,
    boxes: usize,
    attrs: usize,
    n_meshes: usize,
    /// Byte +0x10 of each mesh: visited by the walker.
    on: Vec<bool>,
    /// `oct +0xd2`: some attribute has nibble 5 (`FUN_00141720`).
    has_five: bool,
    /// The group word the enabled bytes stand for (bit `g` set: the meshes of group `g` are on).
    word: u64,
    /// `oct +0x50`: the placement matrix (the identity for the map).
    m: M4,
    /// `oct +0x90`: its inverse.
    inv: M4,
    /// `oct +0xd1`: the placement is the identity (the queries skip the frame mapping).
    identity: bool,
}

fn rd32(b: &[u8], o: usize) -> u32 {
    u32::from_le_bytes(b[o..o + 4].try_into().unwrap())
}

impl Oct {
    /// orig 0x141500: builds the object over `file` with the map's mesh-group word `groups` (the identity placement
    /// only). `None` when the file is too short for its own header.
    ///
    /// orig 0x141720: also looks for an attribute with nibble 5 (`oct +0xd2`).
    pub fn new(file: &[u8], groups: u64) -> Option<Oct> {
        if file.len() < 0x50 {
            return None;
        }
        let at = |k: usize| rd32(file, k) as usize;
        let n_meshes = rd32(file, 0x24) as i32 / 0x14;
        let attr_words = (rd32(file, 0x4c) >> 2) as usize;
        let mut o = Oct {
            file: file.to_vec(),
            nodes: at(0x18),
            meshes: at(0x20),
            polys: at(0x28),
            verts: at(0x30),
            planes: at(0x38),
            boxes: at(0x40),
            attrs: at(0x48),
            n_meshes: n_meshes.max(0) as usize,
            on: Vec::new(),
            has_five: false,
            word: 0,
            m: M4_IDENTITY,
            inv: M4_IDENTITY,
            identity: true,
        };
        o.on = vec![false; o.n_meshes];
        o.set_groups(groups);
        o.has_five = (0..attr_words).any(|i| o.attr_word(i) & 0xf == 5);
        Some(o)
    }

    /// orig 0x141768: clears every mesh, then enables the meshes of each set bit of the group word.
    pub fn set_groups(&mut self, groups: u64) {
        self.word = 0;
        self.on.iter_mut().for_each(|b| *b = false);
        for g in 0..64u32 {
            if groups >> g & 1 != 0 {
                self.set_group(g as u8, true);
            }
        }
    }

    /// orig 0x141840: enables every mesh of one group.
    ///
    /// orig 0x141800: disables every mesh of one group.
    pub fn set_group(&mut self, group: u8, on: bool) {
        if group < 64 {
            self.word = if on { self.word | 1 << group } else { self.word & !(1 << group) };
        }
        for m in 0..self.n_meshes {
            if self.file[self.meshes + 0x14 * m + 0x11] == group {
                self.on[m] = on;
            }
        }
    }

    /// orig 0x142858: places the object. Stores the matrix (`+0x50`) and its inverse (`+0x90`: the outer products of
    /// the three basis rows, one divide by the determinant, the translation row negated through the inverse) and sets
    /// the identity flag (`+0xd1`) when the matrix is the identity exactly.
    pub fn place(&mut self, m: &M4) {
        self.identity = *m == M4_IDENTITY;
        self.m = *m;
        let (a, b, c, d) = (m[0], m[1], m[2], m[3]);
        let r0 = cross_qe(b, c);
        let r1 = cross_qe(c, a);
        let r2 = cross_qe(a, b);
        let det = dot3(a, r0);
        // The z lane sums z first, then x, then y (the add-broadcast pair of that lane); x and y sum in dot order.
        let t = [dot3(d, r0), dot3(d, r1), (d[2] * r2[2] + d[0] * r2[0]) + d[1] * r2[1]];
        let q = vu_div(ONE, det);
        // The scaling multiplies the three direction lanes of each row; the w lanes are 0, 0, 0 and the matrix's own w (1 for every placement the game builds) [inferred from the oracle].
        let row = |x: f32, y: f32, z: f32, w: f32| [x * q, y * q, z * q, w];
        self.inv = [
            row(r0[0], r1[0], r2[0], 0.0),
            row(r0[1], r1[1], r2[1], 0.0),
            row(r0[2], r1[2], r2[2], 0.0),
            row(0.0 - t[0], 0.0 - t[1], 0.0 - t[2], d[3]),
        ];
    }

    /// The placement matrix (`oct +0x50`).
    pub fn placement(&self) -> &M4 {
        &self.m
    }

    /// The inverse placement (`oct +0x90`).
    pub fn placement_inverse(&self) -> &M4 {
        &self.inv
    }

    /// `oct +0xd1`: the placement is the identity.
    pub fn is_identity(&self) -> bool {
        self.identity
    }

    /// orig 0x142a88: a collapsed object: some basis row of the placement has a squared length below the limit.
    pub fn collapsed(&self) -> bool {
        self.m.iter().take(3).any(|row| dot3(*row, *row) < COLLAPSE_LIMIT)
    }

    /// A point into the object's frame (the inverse placement, skipped for the identity).
    fn to_local(&self, v: V4) -> V4 {
        if self.identity { v } else { xf(&self.inv, v) }
    }

    /// A point back out of the object's frame.
    fn to_world(&self, v: V4) -> V4 {
        if self.identity { v } else { xf(&self.m, v) }
    }

    /// Whether the walker visits mesh `m` (its enabled byte).
    pub fn mesh_enabled(&self, m: usize) -> bool {
        self.on.get(m).copied().unwrap_or(false)
    }

    /// The number of meshes.
    pub fn mesh_count(&self) -> usize {
        self.n_meshes
    }

    /// `oct +0xd2`: some attribute word has the nibble 5.
    pub fn has_nibble_five(&self) -> bool {
        self.has_five
    }

    /// The group word the meshes' enabled bytes stand for.
    pub fn groups(&self) -> u64 {
        self.word
    }

    fn u16_at(&self, o: usize) -> u16 {
        u16::from_le_bytes(self.file[o..o + 2].try_into().unwrap())
    }
    fn i16_at(&self, o: usize) -> i16 {
        i16::from_le_bytes(self.file[o..o + 2].try_into().unwrap())
    }
    fn f32_at(&self, o: usize) -> f32 {
        f32::from_le_bytes(self.file[o..o + 4].try_into().unwrap())
    }
    fn v4_at(&self, o: usize) -> V4 {
        [self.f32_at(o), self.f32_at(o + 4), self.f32_at(o + 8), self.f32_at(o + 12)]
    }
    fn qbox_at(&self, o: usize) -> QBox {
        std::array::from_fn(|k| self.i16_at(o + 2 * k))
    }
    fn attr_word(&self, i: usize) -> u32 {
        rd32(&self.file, self.attrs + 4 * i)
    }

    /// The level of the quantized boxes.
    ///
    /// orig 0x141888: the file's header word +0xc.
    pub fn level(&self) -> usize {
        (rd32(&self.file, 0x0c) as usize).min(3)
    }

    /// The polygon record `p` (16 bytes).
    fn poly(&self, p: u16) -> usize {
        self.polys + 16 * p as usize
    }

    /// orig 0x1422b0: the attribute word of polygon `p`.
    pub fn attr(&self, p: u16) -> u32 {
        self.attr_word(self.u16_at(self.poly(p) + 0xe) as usize)
    }

    /// orig 0x142190: the plane of polygon `p` (normal xyz and distance w).
    pub fn plane(&self, p: u16) -> V4 {
        self.v4_at(self.planes + 16 * self.u16_at(self.poly(p) + 10) as usize)
    }

    /// Byte +0 and +1 of the polygon record (the floor colour lookup reads them).
    pub fn poly_bytes(&self, p: u16) -> (u8, u8) {
        let o = self.poly(p);
        (self.file[o], self.file[o + 1])
    }

    /// orig 0x1418a0: the vertices of polygon `p`.
    pub fn poly_verts(&self, p: u16) -> Verts {
        let o = self.poly(p);
        let vtx = |k: usize| self.v4_at(self.verts + 16 * self.u16_at(o + k) as usize);
        let mut v = [[0.0; 4]; 4];
        v[0] = vtx(2);
        v[1] = vtx(4);
        v[2] = vtx(6);
        let fourth = self.u16_at(o + 8);
        if fourth != 0xffff {
            v[3] = vtx(8);
            Verts { v, n: 4 }
        } else {
            Verts { v, n: 3 }
        }
    }

    /// orig 0x141b50: the polygons whose tree boxes overlap the quantized box `q`, in tree order, appended to
    /// `out` (at most 0x3ff). `kind` picks the handler of `kind_accepts`.
    pub fn collect(&self, filt: Filter, q: &QBox, kind: u32, out: &mut Vec<u16>) {
        self.collect_node(filt, 0, q, kind, out);
    }

    fn collect_node(&self, filt: Filter, node: usize, q: &QBox, kind: u32, out: &mut Vec<u16>) {
        let n = self.nodes + 0x20 * node;
        if !qbox_overlap(&self.qbox_at(n + 0x10), q) {
            return;
        }
        let (first, last) = (self.u16_at(n + 0x1c), self.u16_at(n + 0x1e));
        for m in first..last {
            let mo = self.meshes + 0x14 * m as usize;
            if !(self.on.get(m as usize).copied().unwrap_or(false) && qbox_overlap(&self.qbox_at(mo), q)) {
                continue;
            }
            let (pf, pl) = (self.u16_at(mo + 0xc), self.u16_at(mo + 0xe));
            for p in pf..pl {
                let po = self.poly(p);
                let attr = self.attr_word(self.u16_at(po + 0xe) as usize);
                let masked = attr & filt.mask;
                let keep = if filt.invert { masked != filt.value } else { masked == filt.value };
                if !keep {
                    continue;
                }
                let skip_handler = filt.b && self.has_five && attr & 0xf == 5;
                if !skip_handler && !kind_accepts(kind, attr) {
                    continue;
                }
                let bx = self.u16_at(po + 0xc);
                if bx == 0xffff || qbox_overlap(&self.qbox_at(self.boxes + 0xc * bx as usize), q) {
                    if out.len() > 0x3fe {
                        return;
                    }
                    out.push(p);
                }
            }
        }
        for i in 0..8 {
            let c = self.i16_at(n + 2 * i);
            if c == -1 {
                break;
            }
            self.collect_node(filt, c as usize, q, kind, out);
        }
    }

    /// orig 0x141f08: the nearest polygon the segment `p0`..`p1` crosses among the polygons the walker collects
    /// under `filt` and `kind`. Returns the hit point and polygon (0xffff when none).
    ///
    /// orig 0x141d90: the nearest-hit search over the collected polygons.
    ///
    /// A moved object maps both points into its frame first and the hit back out (orig 0x142858's inverse and matrix).
    pub fn ray(&self, filt: Filter, p0: V4, p1: V4, kind: u32) -> (Option<V4>, u16) {
        let (p0, p1) = (self.to_local(p0), self.to_local(p1));
        let b = fbox_corners([p0[0], p0[1], p0[2]], [p1[0], p1[1], p1[2]]);
        let q = fbox_to_qbox(&b, self.level());
        let mut hits = Vec::new();
        self.collect(filt, &q, kind, &mut hits);
        let origin = [p0[0], p0[1], p0[2], ONE];
        let d = sub(p1, p0);
        let dir = [d[0], d[1], d[2], ONE];
        let mut best: Option<V4> = None;
        let mut poly = 0xffffu16;
        let mut best_d2 = FAR;
        for &p in &hits {
            let verts = self.poly_verts(p);
            let plane = self.plane(p);
            let (true, Some(hit)) = ray_triangle(origin, dir, &verts, plane) else { continue };
            let off = sub(hit, origin);
            let d2 = dot3(off, off);
            if d2 < best_d2 {
                best = Some(hit);
                poly = p;
                best_d2 = d2;
            }
        }
        (best.map(|h| self.to_world(h)), poly)
    }

    /// orig 0x143960: the ray cast of the floor-style walking filter (the sweep's filter with the nibble-5 exception
    /// armed, `oct +0xd3`) on the walking map (slot 0x355934).
    pub fn ray_walk_b(&self, p0: V4, p1: V4, kind: u32) -> (Option<V4>, u16) {
        self.ray(Filter { b: true, ..Filter::WALK }, p0, p1, kind)
    }

    /// orig 0x143ce8: the ray cast that keeps one floor kind only (attribute nibble equal to `floor_kind`) on the
    /// walking map.
    pub fn ray_floor_kind(&self, p0: V4, p1: V4, floor_kind: u32, kind: u32) -> (Option<V4>, u16) {
        self.ray(Filter { mask: 0xf, value: floor_kind, invert: false, b: false }, p0, p1, kind)
    }

    /// orig 0x142760: the polygons under the collider's end point, every polygon of kind 0xf, from one lift above
    /// the foot to `PROBE_DEPTH` below.
    pub fn vertical_probe(&self, c: &Collider) -> Vec<u16> {
        let e = c.v4(coll::END);
        let lift = c.f32_at(coll::LIFT);
        let b = fbox(e[0], e[1] - lift, e[2], e[0], e[1] + PROBE_DEPTH, e[2]);
        let q = fbox_to_qbox(&b, self.level());
        let mut hits = Vec::new();
        self.collect(Filter::ALL, &q, 0xf, &mut hits);
        hits
    }

    /// orig 0x1425a0: of the polygons in `hits` (from the vertical probe) the one whose plane height at the end
    /// point is nearest the raised probe point, with that height. Index 0xffff when none.
    pub fn closest_floor(&self, hits: &[u16], c: &Collider) -> (u16, Option<f32>) {
        let end = c.v4(coll::END);
        let mut raised = end;
        raised[1] -= c.f32_at(coll::LIFT);
        let mut best = 0xffffu16;
        let mut out = None;
        let mut limit = CLOSE_LIMIT;
        for &p in hits {
            let plane = self.plane(p);
            if 0.0 <= dot3(raised, plane) + plane[3] {
                let verts = self.poly_verts(p);
                if inside_xz(&verts, end) {
                    let y = (-plane[3] - (plane[2] * end[2] + plane[0] * end[0])) / plane[1];
                    let gap = (raised[1] - y).abs();
                    if gap < limit {
                        out = Some(y);
                        limit = gap;
                        best = p;
                    }
                }
            }
        }
        (best, out)
    }

    /// orig 0x141910: takes the floor polygon `poly` found at `hit` into the collider (no polygon: `coll +0x60`
    /// = 0xffff and bit 32; otherwise the end snaps to the hit when it is within the step, else bit 32).
    pub fn accept_floor(&self, c: &mut Collider, poly: u16, hit: V4) {
        let mut flags = c.flags();
        if poly == 0xffff {
            c.set_u16(coll::FLOOR_POLY, 0xffff);
            c.set_flags(flags | FLAG_NO_FLOOR_AHEAD);
            return;
        }
        if flags & 1 == 0 {
            let mut top = c.f32_at(coll::START + 4);
            let dy = c.f32_at(coll::DELTA + 4);
            if 0.0 < dy {
                top += dy;
            }
            let limit = (top + (c.f32_at(coll::LIFT) - c.f32_at(coll::HEIGHT)) + c.f32_at(coll::EXTRA)) - ONE;
            if hit[1] <= limit {
                c.set_flags(flags & !FLAG_NO_FLOOR_AHEAD);
                c.set_v4(coll::END, hit);
                c.set_u16(coll::FLOOR_POLY, poly);
                return;
            }
            flags |= FLAG_NO_FLOOR_AHEAD;
        } else {
            let verts = self.poly_verts(poly);
            let end = self.to_local(c.v4(coll::END));
            let plane = self.plane(poly);
            if plane_side(end, verts.v[0], plane) == 0 {
                flags = if self.attr(poly) & 0xf == 2 { flags | FLAG_NO_FLOOR_AHEAD } else { flags & !FLAG_NO_FLOOR_AHEAD };
                c.set_flags(flags);
                c.set_v4(coll::END, hit);
                c.set_u16(coll::FLOOR_POLY, poly);
                return;
            }
            flags |= FLAG_NO_FLOOR_AHEAD;
        }
        c.set_flags(flags);
        c.set_u16(coll::FLOOR_POLY, poly);
    }

    /// orig 0x1423d8: the floor ray at the collider's end point. Returns whether a floor polygon was hit.
    pub fn floor_ray(&self, c: &mut Collider) -> bool {
        let end = c.v4(coll::END);
        let lift = c.f32_at(coll::LIFT);
        let dy = c.f32_at(coll::DELTA + 4).abs();
        let mut up = dy;
        if c.flags() & 0x78 == 0 {
            up = if 0.0 <= dy { if dy <= RAY_CLAMP { dy } else { RAY_CLAMP } } else { 0.0 };
        }
        let mut p0 = end;
        p0[1] = (end[1] - lift) - up;
        let mut p1 = end;
        p1[1] = end[1] + lift * RAY_LIFTS;
        let (hit, poly) = self.ray(Filter::WALK, p0, p1, c.kind());
        match hit {
            None => {
                c.set_flags(c.flags() | FLAG_NO_FLOOR_POLY);
                c.set_f32(coll::FLOOR_Y, FLOOR_NONE);
                self.accept_floor(c, poly, end);
                false
            }
            Some(h) => {
                c.set_f32(coll::FLOOR_Y, h[1]);
                c.set_flags(c.flags() & !FLAG_NO_FLOOR_POLY);
                self.accept_floor(c, poly, h);
                true
            }
        }
    }

    /// orig 0x164138: the collider's sphere swept along its move against the map. Leaves the end in
    /// `coll +0x30` and returns bit 34 of the flag word (a contact happened).
    pub fn sweep(&self, c: &mut Collider) -> u64 {
        if c.flags() & 0x78 == 0x30 {
            c.set_flags(c.flags() & !6 | 4);
        }
        let radius = c.f32_at(coll::RADIUS);
        let lift = c.f32_at(coll::LIFT);
        let mut start = c.v4(coll::START);
        start[1] -= lift;
        let mut end = add(start, c.v4(coll::DELTA));
        end[3] = SWEEP_END_W;
        // A moved object sweeps in its own frame: both ends through the inverse placement (orig 0x142858).
        start = self.to_local(start);
        end = self.to_local(end);
        let delta = sub(end, start);
        let len = dot3(delta, delta).sqrt();
        let whole = (len / radius) as i32;
        let steps = whole.wrapping_add(1);
        let step = if whole == 0 { delta } else { scale(delta, vu_div(1.0, steps as f32)) };
        let half = radius * SWEEP_BOX;
        let end_box = fbox(end[0] - half, end[1] - half, end[2] - half, end[0] + half, end[1] + half, end[2] + half);
        let start_box = fbox(start[0], start[1], start[2], start[0], start[1], start[2]);
        let q = fbox_to_qbox(&fbox_union(&start_box, &end_box), self.level());
        let mut hits = Vec::new();
        self.collect(Filter::WALK, &q, c.kind(), &mut hits);
        if hits.is_empty() {
            c.set_v4(coll::END, self.to_world(end));
            let y = c.f32_at(coll::END + 4);
            c.set_f32(coll::END + 4, y + lift);
            return 0;
        }
        let mut s = Sphere { c: start, r: radius };
        let mut left = steps;
        while left > 0 {
            let prev = s.c;
            s.c = add(s.c, step);
            let mut depth = 0;
            self.step_sphere(half, c, &mut s, &hits, &mut depth, prev);
            s.c = c.v4(coll::END);
            left -= 1;
        }
        if !self.identity {
            c.set_v4(coll::END, self.to_world(c.v4(coll::END)));
        }
        let flags = c.flags();
        if flags & 0x78 != 0x30 {
            let e = c.v4(coll::END);
            if start[0] != e[0] || start[1] != e[1] || start[2] != e[2] {
                let mut p0 = c.v4(coll::START);
                p0[1] -= lift;
                let (hit, _) = self.ray(Filter::WALK, p0, e, c.kind());
                if hit.is_some() {
                    c.set_v4(coll::END, c.v4(coll::START));
                    return 0;
                }
            }
        }
        let final_y = c.f32_at(coll::END + 4);
        c.set_f32(coll::END + 4, final_y + lift);
        flags >> 34 & 1
    }

    /// orig 0x163f98: one solver pass for one sweep step, repeated (up to the depth limit) while it keeps finding
    /// contacts.
    pub fn step_sphere(&self, half: f32, c: &mut Collider, s: &mut Sphere, hits: &[u16], depth: &mut i32, prev: V4) {
        *depth += 1;
        let bx = fbox(s.c[0] - half, s.c[1] - half, s.c[2] - half, s.c[0] + half, s.c[1] + half, s.c[2] + half);
        let res = self.solve(c, s, hits, &bx);
        match res {
            -1 => {
                c.set_v4(coll::END, prev);
            }
            0 => {
                c.set_v4(coll::END, s.c);
                return;
            }
            _ => {
                if *depth > 5 {
                    return;
                }
                s.c = c.v4(coll::END);
                self.step_sphere(half, c, s, hits, depth, prev);
            }
        }
        c.set_flags(c.flags() | FLAG_CONTACT);
    }

    /// orig 0x1632b8: pushes the sphere out of the collected polygons: faces first, then (for polygons the face
    /// pass left alone) edges and corners. Writes the sphere's new centre to `coll +0x30` and returns the number of contacts, -1 when there were more than nine
    /// (the walking mode gives up).
    pub fn solve(&self, c: &mut Collider, s: &mut Sphere, hits: &[u16], sbox: &FBox) -> i32 {
        let mode = c.mode();
        let walking = c.flags() & 1 == 0;
        let mut contacts = 0;
        let mut deferred: Vec<u16> = Vec::new();
        for &p in hits {
            let attr = self.attr(p);
            if attr & 0x100000 != 0 {
                continue;
            }
                // The body-height probe passes run for the identity-placed object only (the original tests oct +0xd1).
            let plane = self.plane(p);
            if self.identity && mode == 1 && attr & 0xf != 1 {
                // The extra body-height probe: the same face test with the centre moved down.
                let shift = if walking { c.f32_at(coll::RADIUS) } else { c.f32_at(coll::LIFT) - c.f32_at(coll::HEIGHT) * 0.5 };
                let mut probe = s.c;
                probe[1] += shift;
                if let Some(d) = plane_gap(probe, plane, s.r) {
                    let verts = self.poly_verts(p);
                    if fbox_overlap(&fbox_of_polygon(&verts.v, verts.n), sbox) {
                        let (inside, contact) = face_contact(probe, &verts, plane, d);
                        if inside {
                            let moved = add(contact, mul_xyz(plane, s.r + PUSH_EPS));
                            hit_callback(c, attr, p, contact);
                            let mut centre = moved;
                            centre[1] -= shift;
                            s.c = centre;
                        }
                    }
                }
            }
            if mode == 2 && attr & 0xf == 1 {
                continue;
            }
            if let Some(d) = plane_gap(s.c, plane, s.r) {
                let verts = self.poly_verts(p);
                if fbox_overlap(&fbox_of_polygon(&verts.v, verts.n), sbox) {
                    let (inside, contact) = face_contact(s.c, &verts, plane, d);
                    if !inside {
                        deferred.push(p);
                    } else {
                        hit_callback(c, attr, p, contact);
                        // The multiplies write x, y and z only: the plane keeps its distance in w, so w ends as
                        // (centre w - distance) + distance (VU dest masks, read from the instruction words).
                        s.c = add(contact, mul_xyz(plane, s.r + PUSH_EPS));
                        contacts += 1;
                        if walking && 9 < contacts {
                            return -1;
                        }
                    }
                }
            }
        }
        for &p in &deferred {
            let attr = self.attr(p);
            if attr & 0x100000 != 0 {
                continue;
            }
            let verts = self.poly_verts(p);
            if self.identity && mode == 1 && attr & 0xf != 1 && !walking {
                let shift = c.f32_at(coll::LIFT) - c.f32_at(coll::HEIGHT) * 1.02;
                let mut probe = s.c;
                probe[1] += shift;
                if let Some(contact) = edge_contact(&Sphere { c: probe, r: s.r }, &verts) {
                    let toward = normalize_xyz(sub(probe, contact));
                    let moved = add(mul_xyz(toward, s.r + PUSH_EPS), contact);
                    hit_callback(c, attr, p, contact);
                    let mut centre = moved;
                    centre[1] -= shift;
                    s.c = centre;
                }
            }
            if mode == 2 && attr & 0xf == 1 {
                continue;
            }
            if let Some(contact) = edge_contact(s, &verts) {
                hit_callback(c, attr, p, contact);
                let toward = normalize_xyz(sub(s.c, contact));
                s.c = add(mul_xyz(toward, s.r + PUSH_EPS), contact);
                contacts += 1;
                if walking && 9 < contacts {
                    return -1;
                }
            }
        }
        c.set_v4(coll::END, s.c);
        contacts
    }

    /// orig 0x1434b8: the floor kind under the collider's end point: bits 14..15 of the attribute word of the
    /// closest floor polygon of this object (the game's slot 0x355944), 0 without one.
    pub fn floor_kind(&self, c: &Collider) -> u32 {
        let hits = self.vertical_probe(c);
        match self.closest_floor(&hits, c).0 {
            0xffff => 0,
            p => self.attr(p) >> 14 & 3,
        }
    }

    /// orig 0x1421b8: the unit plane normal of polygon `p` as the original hands it out (w = 1). A moved object
    /// rotates it by the placement's basis rows (the translation row replaced by `(0, 0, 0, 1)`) and renormalizes.
    pub fn normal(&self, p: u16) -> V4 {
        let mut n = self.plane(p);
        n[3] = ONE;
        if self.identity {
            return n;
        }
        let rows = [self.m[0], self.m[1], self.m[2], [0.0, 0.0, 0.0, ONE]];
        let rotated = xf(&rows, n);
        // The divide scales the three direction lanes; w stays as the rotation left it.
        let q = vu_div(ONE, dot3(rotated, rotated).sqrt());
        [rotated[0] * q, rotated[1] * q, rotated[2] * q, rotated[3]]
    }

    /// orig 0x1422d8: the vertices of polygon `p` in world space (a moved object maps each through the placement).
    /// Empty for the polygon index 0xffff.
    pub fn verts_world(&self, p: u16) -> Vec<V4> {
        if p == 0xffff {
            return Vec::new();
        }
        let v = self.poly_verts(p);
        v.v.iter().take(v.n).map(|x| self.to_world(*x)).collect()
    }

    /// orig 0x143560: what stands under the collider, from the floor polygon `coll +0x60` that the floor ray
    /// found (`self` is the walking map, the game's slot 0x355934). `colour` is the floor colour map (slot
    /// 0x35593c) and `kinds` the floor kind object (slot 0x355944), each optional like the original's slot.
    /// The colour table words themselves (`FUN_0015c690` and `FUN_00153540`, the lighting colour of the floor)
    /// are not ported: the result carries the colour map polygon's class byte for the caller to look up.
    pub fn floor_info(&self, c: &Collider, colour: Option<&Oct>, kinds: Option<&Oct>) -> FloorInfo {
        self.floor_info_on(c, colour, kinds, None)
    }

    /// orig 0x143560 with a moving object under the foot: `moving` is the collision object of the entry `coll +0x78`
    /// names (the normal, the attribute flag and the material byte then come from it instead of the map).
    pub fn floor_info_on(&self, c: &Collider, colour: Option<&Oct>, kinds: Option<&Oct>, moving: Option<&Oct>) -> FloorInfo {
        let floor = moving.unwrap_or(self);
        let mut out = FloorInfo { normal: [0.0; 4], opposite: [0.0; 4], poly: 0, material: 0, colour_byte: 0, colour_class: 0, kind: 0, flag: 0 };
        if let Some(map) = colour {
            let hits = map.vertical_probe(c);
            let (p, _) = map.closest_floor(&hits, c);
            if p != 0xffff {
                let (class, byte) = map.poly_bytes(p);
                out.colour_class = class;
                out.colour_byte = byte as u32;
            }
        }
        out.kind = kinds.map_or(0, |k| k.floor_kind(c));
        let poly = c.u16_at(coll::FLOOR_POLY);
        out.poly = poly as u32;
        if poly == 0xffff {
            out.normal = [0.0, -ONE, 0.0, ONE];
            out.opposite = [0.0, ONE, 0.0, ONE];
        } else {
            out.normal = floor.normal(poly);
            out.flag = (floor.attr(poly) >> 16 & 1) as u8;
            out.material = floor.poly_bytes(poly).0 as u32;
            out.opposite = sub([0.0, 0.0, 0.0, ONE], out.normal);
        }
        out
    }

    /// orig 0x143840 on a map with no moving collision objects: the sweep, then the floor ray.
    /// Returns the sweep's contact bit.
    pub fn walk_step(&self, c: &mut Collider) -> u64 {
        self.walk_step_with(&MovingPool::empty(), c)
    }

    /// orig 0x143840: the full walking step over the map and the moving collision objects of `pool`. The pool sweep
    /// comes first (a hit rewrites the delta so the map sweep continues from the pushed end), then the map sweep, then
    /// the floor ray against every moving object (the highest floor wins; a hit rewrites the delta and sweeps the map
    /// again) or, with none, against the map. Returns the OR of the sweeps' contact bits.
    pub fn walk_step_with(&self, pool: &MovingPool, c: &mut Collider) -> u64 {
        c.set_u16(coll::POLY_A, 0xffff);
        c.set_flags(c.flags() & !FLAG_CONTACT);
        c.set_u16(coll::POLY_B, 0xffff);
        let mut floor = FLOOR_OUT_NONE;
        let mut r = pool.sweep_all(c);
        r |= self.sweep(c);
        c.set_u32(coll::MOVING, 0);
        if pool.floor_ray_all(c, &mut floor) == 0 {
            if self.floor_ray(c) {
                c.set_u32(coll::MOVING, 0);
            }
        } else {
            c.set_v4(coll::DELTA, sub(c.v4(coll::END), c.v4(coll::START)));
            r |= self.sweep(c);
        }
        if floor != FLOOR_OUT_NONE {
            c.set_f32(coll::FLOOR_Y, floor);
            c.set_flags(c.flags() & !FLAG_NO_FLOOR_POLY);
        }
        r
    }
}

/// orig 0x163248: which side of a polygon's plane the point is on, by the gap from the polygon's first vertex along the
/// normal: 1 when the vertex is more than a thousandth behind the point's plane position, 2 within a thousandth, 0
/// (strictly in front) otherwise.
pub fn plane_side(p: V4, first_vertex: V4, plane: V4) -> u32 {
    let d = dot3(sub(first_vertex, p), plane);
    if d < -PLANE_SIDE {
        1
    } else if d <= PLANE_SIDE {
        2
    } else {
        0
    }
}

/// orig 0x330c80: records the polygon and contact point for attributes 0x800 and 0x1000 (collision kind 0 only).
pub fn hit_callback(c: &mut Collider, attr: u32, poly: u16, contact: V4) {
    if c.flags() & 0x78 == 0 {
        if attr & 0x800 != 0 {
            c.set_u16(coll::POLY_A, poly);
            c.set_v4(coll::CONTACT_A, contact);
        }
        if attr & 0x1000 != 0 {
            c.set_u16(coll::POLY_B, poly);
            c.set_v4(coll::CONTACT_B, contact);
        }
    }
}

/// orig 0x1625e0: the sphere's signed gap to the plane when the centre is in front of it and within the radius.
pub fn plane_gap(centre: V4, plane: V4, radius: f32) -> Option<f32> {
    let d = dot3(centre, plane) + plane[3];
    (0.0..=radius).contains(&d).then_some(d)
}

/// orig 0x162678: the centre projected onto the plane, and whether that point lies inside the polygon.
pub fn face_contact(centre: V4, verts: &Verts, plane: V4, gap: f32) -> (bool, V4) {
    let contact = sub(centre, mul_xyz(plane, gap));
    (inside_edges(verts, contact, plane), contact)
}

/// orig 0x162028: is `p` on the inner side of every edge of the polygon (the edge cross product against the
/// plane normal not above 0.01)?
pub fn inside_edges(verts: &Verts, p: V4, normal: V4) -> bool {
    for (i, &nx) in next_table(verts.n).iter().enumerate().take(verts.n) {
        let e = sub(verts.v[nx], verts.v[i]);
        let q = sub(p, verts.v[i]);
        let cr = cross_qe(q, e);
        if INSIDE_EPS < dot3(cr, normal) {
            return false;
        }
    }
    true
}

/// orig 0x161ed0: is `p` inside the polygon seen from above (every edge's cross product has a non-negative y)?
pub fn inside_xz(verts: &Verts, p: V4) -> bool {
    for (i, &nx) in next_table(verts.n).iter().enumerate().take(verts.n) {
        let e = sub(verts.v[nx], verts.v[i]);
        let q = sub(p, verts.v[i]);
        if cross_qe(q, e)[1] < 0.0 {
            return false;
        }
    }
    true
}

/// orig 0x1623a0: where the ray `origin` + t * `dir` (0 <= t <= 1) crosses the polygon's plane when it comes
/// from the front. Returns whether the crossing is inside the polygon, and the point as the original leaves it in
/// its output record (`None` when the ray never reaches the plane, which leaves the record alone).
pub fn ray_triangle(origin: V4, dir: V4, verts: &Verts, plane: V4) -> (bool, Option<V4>) {
    let den = dot3(plane, dir);
    if den < 0.0 {
        let t = (dot3(plane, verts.v[0]) - dot3(plane, origin)) / den;
        if (0.0..=ONE).contains(&t) {
            let mut hit = add(scale(dir, t), origin);
            hit[3] = ONE;
            return (inside_edges(verts, hit, plane), Some(hit));
        }
    }
    (false, None)
}

/// orig 0x1626f0: the contact of the sphere with the polygon's edges and corners: the foot of the sphere
/// centre on each edge line that lies inside the (slightly shifted) edge box and within the radius; else the
/// nearest corner within the radius. The nearest of several wins. Returns the contact and the output record as
/// the original leaves it (a failed search leaves the last edge's foot there).
pub fn edge_contact_out(s: &Sphere, verts: &Verts) -> (Option<V4>, V4) {
    let r2 = s.r * s.r;
    let eps = [EDGE_EPS, EDGE_EPS, EDGE_EPS, ONE];
    let mut found: Vec<V4> = Vec::new();
    let mut scratch = [0.0; 4];
    for (i, &nx) in next_table(verts.n).iter().enumerate().take(verts.n) {
        let a = verts.v[i];
        let b = verts.v[nx];
        let d = sub(b, a);
        let d = [d[0], d[1], d[2], ONE];
        let a1 = [a[0], a[1], a[2], ONE];
        let to_a = sub(a1, s.c);
        let t = -(dot3(to_a, d) / dot3(d, d));
        let foot = add(scale(d, t), a1);
        scratch = foot;
        let off = sub(s.c, foot);
        if dot3(off, off) <= r2 {
            let lo = add(a, eps);
            let hi = add(b, eps);
            let between = |k: usize| {
                let f = foot[k];
                if hi[k] < f { f < lo[k] } else { lo[k] <= f }
            };
            if between(0) || between(1) || between(2) {
                found.push(foot);
            }
        }
    }
    let hit = match found.len() {
        0 => {
            let mut best = r2;
            let mut idx = 4;
            for (i, v) in verts.v.iter().enumerate().take(verts.n) {
                let off = sub(s.c, *v);
                let d2 = dot3(off, off);
                if d2 <= best {
                    best = d2;
                    idx = i;
                }
            }
            (idx != 4).then(|| verts.v[idx])
        }
        1 => Some(found[0]),
        _ => {
            let mut best = FAR;
            let mut pick = 0;
            for (j, f) in found.iter().enumerate() {
                let off = sub(s.c, *f);
                let d2 = dot3(off, off);
                if d2 < best {
                    best = d2;
                    pick = j;
                }
            }
            Some(found[pick])
        }
    };
    (hit, hit.unwrap_or(scratch))
}

fn edge_contact(s: &Sphere, verts: &Verts) -> Option<V4> {
    edge_contact_out(s, verts).0
}

/// orig 0x164780: of the polygon's edges, the foot of the perpendicular from `p` on each edge line that falls inside the
/// edge's box widened by the epsilon vector; with several the foot nearest `p` wins. Returns the winning edge's two end
/// vertices (`None` when no foot qualifies) and the foot record as the original leaves it (the last edge's foot when
/// none qualifies, written by every edge pass).
pub fn nearest_edge(verts: &Verts, p: V4) -> (Option<(V4, V4)>, V4) {
    let eps = [EDGE_EPS, EDGE_EPS, EDGE_EPS, ONE];
    let mut found: Vec<(V4, V4, V4)> = Vec::new();
    let mut out = [0.0; 4];
    for (i, &nx) in next_table(verts.n).iter().enumerate().take(verts.n) {
        let a = verts.v[i];
        let b = verts.v[nx];
        let mut d = sub(b, a);
        d[3] = ONE;
        let a1 = [a[0], a[1], a[2], ONE];
        let diff = sub(a1, p);
        let t = -(dot3(diff, d) / dot3(d, d));
        let foot = add(scale(d, t), a1);
        out = foot;
        let lo = add(a, eps);
        let hi = add(b, eps);
        let between = |k: usize| {
            let f = foot[k];
            if hi[k] < f { f < lo[k] } else { lo[k] <= f }
        };
        if between(0) || between(1) || between(2) {
            found.push((foot, a, b));
        }
    }
    let pick = match found.len() {
        0 => return (None, out),
        1 => 0,
        _ => {
            let mut best = NEAREST_EDGE_START;
            let mut pick = 0;
            for (j, f) in found.iter().enumerate() {
                let off = sub(p, f.0);
                let d2 = dot3(off, off);
                if d2 < best {
                    best = d2;
                    pick = j;
                }
            }
            pick
        }
    };
    let (foot, a, b) = found[pick];
    (Some((a, b)), foot)
}

/// orig 0x164690: the length of `normalize(b - a) - dir`, scaled by the angle constant: how far an edge's direction is
/// from `dir` [inferred: an angle measure; its use inside the sphere pass 0x164ce8 was not read].
pub fn edge_direction_gap(a: V4, dir: V4, b: V4) -> f32 {
    let mut d = sub(b, a);
    d[3] = EDGE_DIR_W;
    let n = normalize4(d);
    let off = sub(n, dir);
    dot3(off, off).sqrt() * EDGE_DIR_SCALE
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::build::{BuildPoly, coct_file};
    use crate::flags::PARTY_COLLIDE;

    /// A floor the party stands on: PARTY_STAND and the party-collide bit the kind-0 handler needs (a floor without
    /// bit 0x10 is invisible to the party's sweep and floor ray).
    const FLOOR: u32 = 1 | PARTY_COLLIDE;

    /// Sora's collider: radius 90, lift 115, height 90 (`actor +0x5b0`, `+0x5fc`, `+0x5b4`), kind 0.
    fn sora(feet: [f32; 3], delta: [f32; 3]) -> Collider {
        let mut c = Collider::default();
        c.set_v4(coll::START, [feet[0], feet[1], feet[2], 1.0]);
        c.set_v4(coll::DELTA, [delta[0], delta[1], delta[2], 1.0]);
        c.set_v4(coll::END, [feet[0] + delta[0], feet[1] + delta[1], feet[2] + delta[2], 1.0]);
        c.set_f32(coll::RADIUS, 90.0);
        c.set_f32(coll::HEIGHT, 90.0);
        c.set_f32(coll::LIFT, 115.0);
        c
    }

    fn floor(y: f32, attr: u32) -> BuildPoly {
        BuildPoly::new(&[[-2000.0, y, -2000.0], [2000.0, y, -2000.0], [2000.0, y, 2000.0], [-2000.0, y, 2000.0]], attr)
    }

    /// A wall on the plane x = 300 whose front faces -x (the walkable side).
    fn wall(attr: u32) -> BuildPoly {
        BuildPoly::new(&[[300.0, 0.0, -800.0], [300.0, -400.0, -800.0], [300.0, -400.0, 800.0], [300.0, 0.0, 800.0]], attr)
    }

    #[test]
    fn the_sphere_stops_one_radius_from_a_wall() {
        let oct = Oct::new(&coct_file(&[floor(0.0, FLOOR), wall(PARTY_COLLIDE)]), u64::MAX).unwrap();
        let mut c = sora([0.0, 0.0, 0.0], [400.0, 0.0, 0.0]);
        let hit = oct.sweep(&mut c);
        assert_eq!(hit, 1, "a contact");
        let end = c.v4(coll::END);
        // The centre stops 90 + 0.008 short of the plane; the foot is 115 below the centre.
        assert!((end[0] - (300.0 - 90.0 - 0.008)).abs() < 1e-3, "{end:?}");
        assert!(end[1].abs() < 1e-3, "{end:?}");
        // A wall the party does not collide with (attribute without bit 0x10) lets it through.
        let oct = Oct::new(&coct_file(&[floor(0.0, FLOOR), wall(0x20)]), u64::MAX).unwrap();
        let mut c = sora([0.0, 0.0, 0.0], [400.0, 0.0, 0.0]);
        assert_eq!(oct.sweep(&mut c), 0);
        assert!((c.v4(coll::END)[0] - 400.0).abs() < 1e-3);
    }

    #[test]
    fn a_wall_of_a_switched_off_group_is_not_visited() {
        let file = coct_file(&[floor(0.0, FLOOR).in_group(0), wall(PARTY_COLLIDE).in_group(3)]);
        let mut oct = Oct::new(&file, 1 << 3).unwrap();
        let mut c = sora([0.0, 0.0, 0.0], [400.0, 0.0, 0.0]);
        assert_eq!(oct.sweep(&mut c), 1);
        oct.set_group(3, false);
        let mut c = sora([0.0, 0.0, 0.0], [400.0, 0.0, 0.0]);
        assert_eq!(oct.sweep(&mut c), 0, "FUN_00141800 turns the group's meshes off");
        oct.set_groups(1 << 3);
        let mut c = sora([0.0, 0.0, 0.0], [400.0, 0.0, 0.0]);
        assert_eq!(oct.sweep(&mut c), 1, "FUN_00141768 sets them from the word again");
    }

    #[test]
    fn the_floor_ray_snaps_the_foot_to_the_floor_and_a_deep_drop_does_not_snap() {
        let oct = Oct::new(&coct_file(&[floor(0.0, FLOOR)]), u64::MAX).unwrap();
        // The foot 5 above the floor: within the step, so the end snaps and bit 32 stays clear.
        let mut c = sora([100.0, -5.0, 50.0], [0.0, 0.0, 0.0]);
        assert!(oct.floor_ray(&mut c));
        assert!(c.v4(coll::END)[1].abs() < 1e-4 && c.f32_at(coll::FLOOR_Y).abs() < 1e-4);
        assert_eq!(c.flags() & (FLAG_NO_FLOOR_AHEAD | FLAG_NO_FLOOR_POLY), 0);
        assert_eq!(c.u16_at(coll::FLOOR_POLY), 0);
        // 60 above: the floor is found but too far below the foot (lift - height - 1 = 24): bit 32 set, no snap.
        let mut c = sora([100.0, -60.0, 50.0], [0.0, 0.0, 0.0]);
        assert!(oct.floor_ray(&mut c));
        assert_eq!(c.flags() & FLAG_NO_FLOOR_AHEAD, FLAG_NO_FLOOR_AHEAD);
        assert!((c.v4(coll::END)[1] - -60.0).abs() < 1e-4);
        // Off the floor's edge: no polygon, the sentinel height and bits 32 and 33.
        let mut c = sora([5000.0, -5.0, 50.0], [0.0, 0.0, 0.0]);
        assert!(!oct.floor_ray(&mut c));
        assert_eq!(c.f32_at(coll::FLOOR_Y), 320_000.0);
        assert_eq!(c.flags() & (FLAG_NO_FLOOR_AHEAD | FLAG_NO_FLOOR_POLY), FLAG_NO_FLOOR_AHEAD | FLAG_NO_FLOOR_POLY);
        assert_eq!(c.u16_at(coll::FLOOR_POLY), 0xffff);
    }

    /// WP-L1-P10 item "ceiling": the clusters have no ceiling probe; a roof stops a rising body because the sphere
    /// sweep pushes the sphere out of any polygon the kind-0 handler accepts, whichever way it faces. The head
    /// (the sphere's top, one radius above its centre, which is one lift above the foot) stops at the roof.
    #[test]
    fn a_roof_stops_a_jump_through_the_sphere_sweep() {
        // The roof on y = -300 (3 m up in the Y-down space), its front facing down into the room.
        let roof = |attr| BuildPoly::new(&[[-2000.0, -300.0, -2000.0], [-2000.0, -300.0, 2000.0], [2000.0, -300.0, 2000.0], [2000.0, -300.0, -2000.0]], attr);
        let oct = Oct::new(&coct_file(&[floor(0.0, FLOOR), roof(PARTY_COLLIDE)]), u64::MAX).unwrap();
        let mut c = sora([0.0, 0.0, 0.0], [0.0, -250.0, 0.0]);
        assert_eq!(oct.sweep(&mut c), 1);
        let foot = c.v4(coll::END)[1];
        // centre.y = -300 + 90 + 0.008, foot = centre + 115: the head touches the roof.
        assert!((foot - (-300.0 + 90.0 + 0.008 + 115.0)).abs() < 1e-3, "{foot}");
        // A roof without the party-collide bit does not stop the jump.
        let oct = Oct::new(&coct_file(&[floor(0.0, FLOOR), roof(0x20)]), u64::MAX).unwrap();
        let mut c = sora([0.0, 0.0, 0.0], [0.0, -250.0, 0.0]);
        assert_eq!(oct.sweep(&mut c), 0);
        assert!((c.v4(coll::END)[1] - -250.0).abs() < 1e-3);
    }

    #[test]
    fn floor_info_reports_the_floor_the_ray_found() {
        let oct = Oct::new(&coct_file(&[floor(0.0, FLOOR | 1 << 16)]), u64::MAX).unwrap();
        let mut c = sora([100.0, -5.0, 50.0], [0.0, 0.0, 0.0]);
        oct.floor_ray(&mut c);
        let info = oct.floor_info(&c, None, None);
        assert_eq!((info.poly, info.flag, info.kind), (0, 1, 0));
        assert_eq!(info.normal, [0.0, -1.0, 0.0, 1.0], "straight up in the Y-down space");
        let mut off = sora([5000.0, -5.0, 50.0], [0.0, 0.0, 0.0]);
        oct.floor_ray(&mut off);
        let info = oct.floor_info(&off, None, None);
        assert_eq!((info.poly, info.normal), (0xffff, [0.0, -1.0, 0.0, 1.0]));
    }
}
