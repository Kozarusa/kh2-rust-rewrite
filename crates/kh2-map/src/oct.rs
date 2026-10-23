//! The game's built collision object and its queries (WP-L1-P10, spec `docs/systems/core-runtime/collision.md`
//! section 9). `FUN_00141500` builds the object from a COCT file (the map BAR entry of type 6); every
//! query below takes the object and a collider record and mirrors one original function.
//!
//! Scope. The object here is the identity-placed one (the map's own collision): the placement matrices of
//! `FUN_00142858` and the branches the original takes for a moved object (`oct +0xd1 == 0`) belong to the
//! moving collision objects (lifts and platforms: the pool at 0x35a8a0), which this package leaves out.
//! `FUN_00143840` therefore runs with an empty moving-object list, which is what a plain field has.
//!
//! Layout. The queries read the file in place by byte offsets like the original (a polygon record is 16
//! bytes, a mesh 0x14, a tree node 0x20), the collider is the 0x88-byte record every query takes (the actor
//! keeps it at `actor +0x590`; `coll +0x20` is `actor +0x5b0`, the body radius read from the model's body
//! shape by `FUN_00169af0`, 90 for Sora).

use crate::geom::*;

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

/// The polygon walker's attribute filter (`FUN_00141b30`: `oct +0x40`, `+0x44`, `+0xd0`, `+0xd3`).
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
}

fn rd32(b: &[u8], o: usize) -> u32 {
    u32::from_le_bytes(b[o..o + 4].try_into().unwrap())
}

impl Oct {
    /// orig 0x141500: builds the object over `file` with the map's mesh-group word `groups` (`FUN_00141768`).
    /// `None` when the file is too short for its own header.
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
        };
        o.on = vec![false; o.n_meshes];
        o.set_groups(groups);
        o.has_five = (0..attr_words).any(|i| o.attr_word(i) & 0xf == 5);
        Some(o)
    }

    /// orig 0x141768: clears every mesh, then enables the meshes of each set bit of the group word.
    pub fn set_groups(&mut self, groups: u64) {
        self.on.iter_mut().for_each(|b| *b = false);
        for g in 0..64u32 {
            if groups >> g & 1 != 0 {
                self.set_group(g as u8, true);
            }
        }
    }

    /// orig 0x141840 / 0x141800: enable or disable every mesh of one group.
    pub fn set_group(&mut self, group: u8, on: bool) {
        for m in 0..self.n_meshes {
            if self.file[self.meshes + 0x14 * m + 0x11] == group {
                self.on[m] = on;
            }
        }
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

    /// The level of the quantized boxes (`FUN_00141888`: the file's header word +0xc).
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

    /// orig 0x141f08 with 0x141d90: the nearest polygon the segment `p0`..`p1` crosses among the polygons the
    /// walker collects under `filt` and `kind`. Returns the hit point and polygon (0xffff when none).
    pub fn ray(&self, filt: Filter, p0: V4, p1: V4, kind: u32) -> (Option<V4>, u16) {
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
        (best, poly)
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
            let end = c.v4(coll::END);
            let plane = self.plane(poly);
            // orig 0x163248: which side of the polygon's plane the end point is on.
            let d = dot3(sub(verts.v[0], end), plane);
            let side = if d < -PLANE_SIDE {
                1
            } else if d <= PLANE_SIDE {
                2
            } else {
                0
            };
            if side == 0 {
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
        end[3] = ONE;
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
            c.set_v4(coll::END, end);
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
        let flags = c.flags();
        let final_y;
        if flags & 0x78 == 0x30 {
            final_y = c.f32_at(coll::END + 4);
        } else {
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
            final_y = c.f32_at(coll::END + 4);
        }
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
            let plane = self.plane(p);
            if mode == 1 && attr & 0xf != 1 {
                // The extra body-height probe: the same face test with the centre moved down.
                let shift = if walking { c.f32_at(coll::RADIUS) } else { c.f32_at(coll::LIFT) - c.f32_at(coll::HEIGHT) * 0.5 };
                let mut probe = s.c;
                probe[1] += shift;
                if let Some(d) = plane_gap(probe, plane, s.r) {
                    let verts = self.poly_verts(p);
                    if fbox_overlap(&fbox_of_polygon(&verts.v, verts.n), sbox) {
                        let (inside, contact) = face_contact(probe, &verts, plane, d);
                        if inside {
                            let moved = add(contact, scale(plane, s.r + PUSH_EPS));
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
                        s.c = add(contact, scale(plane, s.r + PUSH_EPS));
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
            if mode == 1 && attr & 0xf != 1 && !walking {
                let shift = c.f32_at(coll::LIFT) - c.f32_at(coll::HEIGHT) * 1.02;
                let mut probe = s.c;
                probe[1] += shift;
                if let Some(contact) = edge_contact(&Sphere { c: probe, r: s.r }, &verts) {
                    let toward = normalize4(sub(probe, contact));
                    let moved = add(contact, scale(toward, s.r + PUSH_EPS));
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
                let toward = normalize4(sub(s.c, contact));
                s.c = add(contact, scale(toward, s.r + PUSH_EPS));
                contacts += 1;
                if walking && 9 < contacts {
                    return -1;
                }
            }
        }
        c.set_v4(coll::END, s.c);
        contacts
    }

    /// orig 0x143840 on a map with no moving collision objects: the sweep, then the floor ray.
    /// Returns the sweep's contact bit.
    pub fn walk_step(&self, c: &mut Collider) -> u64 {
        c.set_u16(coll::POLY_A, 0xffff);
        c.set_flags(c.flags() & !FLAG_CONTACT);
        c.set_u16(coll::POLY_B, 0xffff);
        let r = self.sweep(c);
        c.set_u32(coll::MOVING, 0);
        if self.floor_ray(c) {
            c.set_u32(coll::MOVING, 0);
        }
        r
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
    (d <= radius && 0.0 <= d).then_some(d)
}

/// orig 0x162678: the centre projected onto the plane, and whether that point lies inside the polygon.
pub fn face_contact(centre: V4, verts: &Verts, plane: V4, gap: f32) -> (bool, V4) {
    let contact = sub(centre, scale(plane, gap));
    (inside_edges(verts, contact, plane), contact)
}

/// orig 0x162028: is `p` on the inner side of every edge of the polygon (the edge cross product against the
/// plane normal not above 0.01)?
pub fn inside_edges(verts: &Verts, p: V4, normal: V4) -> bool {
    let next = next_table(verts.n);
    for i in 0..verts.n {
        let e = sub(verts.v[next[i]], verts.v[i]);
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
    let next = next_table(verts.n);
    for i in 0..verts.n {
        let e = sub(verts.v[next[i]], verts.v[i]);
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
        if t <= ONE && 0.0 <= t {
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
    let next = next_table(verts.n);
    let r2 = s.r * s.r;
    let eps = [EDGE_EPS, EDGE_EPS, EDGE_EPS, ONE];
    let mut found: Vec<V4> = Vec::new();
    let mut scratch = [0.0; 4];
    for i in 0..verts.n {
        let a = verts.v[i];
        let b = verts.v[next[i]];
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
