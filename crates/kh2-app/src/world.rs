//! Map collision from the area's COCT (falls back to render triangles when an area has none).
//!
//! Everything here is in game units (centimetres) in map-model space, Y up. Characters are
//! vertical capsules; floors are found with a downward probe, walls push the capsule out along
//! the polygon normal (sliding), ceilings stop upward motion.

use bevy::prelude::*;
use std::collections::HashMap;

const CELL: f32 = 400.0;
/// Surfaces at most this steep count as floor (cos of 60°).
pub const FLOOR_NY: f32 = 0.5;

#[derive(Clone, Copy)]
pub struct Tri {
    pub v: [Vec3; 3],
    pub n: Vec3,
    pub flags: u32,
    /// COCT ground material (0 for areas without COCT).
    pub material: u8,
    pub lo: Vec3,
    pub hi: Vec3,
}

#[derive(Resource, Default)]
pub struct World {
    pub tris: Vec<Tri>,
    grid: HashMap<(i32, i32), Vec<u32>>,
    pub min: Vec3,
    pub max: Vec3,
    /// True when built from COCT data.
    pub coct: bool,
}

#[derive(Debug, Clone, Copy)]
pub struct Ground {
    pub y: f32,
    pub normal: Vec3,
    pub flags: u32,
    pub material: u8,
}

fn cell(v: f32) -> i32 {
    (v / CELL).floor() as i32
}

impl World {
    fn push(&mut self, v: [Vec3; 3], flags: u32, material: u8) {
        let n = (v[1] - v[0]).cross(v[2] - v[0]);
        if n.length_squared() < 1e-6 {
            return;
        }
        let lo = v[0].min(v[1]).min(v[2]);
        let hi = v[0].max(v[1]).max(v[2]);
        let id = self.tris.len() as u32;
        self.tris.push(Tri { v, n: n.normalize(), flags, material, lo, hi });
        self.min = self.min.min(lo);
        self.max = self.max.max(hi);
        for x in cell(lo.x)..=cell(hi.x) {
            for z in cell(lo.z)..=cell(hi.z) {
                self.grid.entry((x, z)).or_default().push(id);
            }
        }
    }

    fn empty() -> World {
        World { min: Vec3::splat(f32::MAX), max: Vec3::splat(f32::MIN), ..default() }
    }

    pub fn from_coct(c: &kh2_map::Coct) -> World {
        let mut w = World::empty();
        w.coct = true;
        for p in &c.polygons {
            for t in p.triangles() {
                let v = t.map(Vec3::from);
                // Orient the triangle with the polygon's plane normal.
                let n = (v[1] - v[0]).cross(v[2] - v[0]);
                let pn = Vec3::from(p.normal);
                let v = if n.dot(pn) < 0.0 { [v[0], v[2], v[1]] } else { v };
                w.push(v, p.flags, p.material);
            }
        }
        w
    }

    pub fn from_meshes(meshes: &[kh2_model::Mesh]) -> World {
        let mut w = World::empty();
        for m in meshes.iter().filter(|m| !m.transparent) {
            for t in m.indices.as_chunks::<3>().0 {
                let v = [t[0], t[1], t[2]].map(|i| Vec3::from(m.positions[i as usize]));
                w.push(v, 0, 0);
            }
        }
        w
    }

    fn near(&self, lo: Vec3, hi: Vec3, out: &mut Vec<u32>) {
        out.clear();
        if !(lo.is_finite() && hi.is_finite()) || (hi - lo).max_element() > 20000.0 {
            return;
        }
        for x in cell(lo.x)..=cell(hi.x) {
            for z in cell(lo.z)..=cell(hi.z) {
                if let Some(v) = self.grid.get(&(x, z)) {
                    for &i in v {
                        let t = &self.tris[i as usize];
                        if t.hi.x >= lo.x && t.lo.x <= hi.x && t.hi.y >= lo.y && t.lo.y <= hi.y && t.hi.z >= lo.z && t.lo.z <= hi.z {
                            out.push(i);
                        }
                    }
                }
            }
        }
        out.sort_unstable();
        out.dedup();
    }

    /// Highest floor under (x, z) between `from_y - down` and `from_y`.
    pub fn ground(&self, p: Vec3, from_y: f32, down: f32) -> Option<Ground> {
        let mut ids = Vec::new();
        self.near(Vec3::new(p.x, from_y - down, p.z), Vec3::new(p.x, from_y, p.z), &mut ids);
        let mut best: Option<Ground> = None;
        for &i in &ids {
            let t = &self.tris[i as usize];
            if t.n.y < FLOOR_NY {
                continue;
            }
            if let Some(y) = height_at(t, p.x, p.z)
                && y <= from_y
                && y >= from_y - down
                && best.is_none_or(|b| y > b.y)
            {
                best = Some(Ground { y, normal: t.n, flags: t.flags, material: t.material });
            }
        }
        best
    }

    /// Lowest ceiling above (x, z) between `from_y` and `from_y + up`.
    pub fn ceiling(&self, p: Vec3, from_y: f32, up: f32) -> Option<f32> {
        let mut ids = Vec::new();
        self.near(Vec3::new(p.x, from_y, p.z), Vec3::new(p.x, from_y + up, p.z), &mut ids);
        ids.iter()
            .map(|&i| &self.tris[i as usize])
            .filter(|t| t.n.y < -FLOOR_NY)
            .filter_map(|t| height_at(t, p.x, p.z))
            .filter(|&y| y >= from_y && y <= from_y + up)
            .min_by(f32::total_cmp)
    }

    /// Moves a vertical capsule (feet at `pos`) horizontally by `delta`, sliding along walls.
    /// Returns the wall normals touched.
    pub fn slide(&self, pos: &mut Vec3, delta: Vec3, radius: f32, bottom: f32, top: f32) -> Vec<Vec3> {
        let mut touched = Vec::new();
        let len = delta.length();
        if !len.is_finite() {
            return touched;
        }
        let steps = ((len / (radius * 0.5)).ceil() as usize).clamp(1, 32);
        let step = delta / steps as f32;
        let mut ids = Vec::new();
        for _ in 0..steps {
            *pos += step;
            // Resolve penetration against wall-like triangles at the capsule's height.
            for _ in 0..3 {
                let lo = *pos + Vec3::new(-radius, bottom, -radius);
                let hi = *pos + Vec3::new(radius, top, radius);
                self.near(lo, hi, &mut ids);
                let mut moved = false;
                for &i in &ids {
                    let t = &self.tris[i as usize];
                    if t.n.y.abs() >= FLOOR_NY {
                        continue;
                    }
                    // Closest point between the capsule axis segment and the triangle.
                    let a = *pos + Vec3::Y * (bottom + radius);
                    let b = *pos + Vec3::Y * (top - radius).max(bottom + radius);
                    let (q, c) = closest_segment_triangle(a, b, t);
                    // One-sided: COCT walls face the walkable side; ignore them from behind so
                    // back-to-back faces cannot push the capsule back and forth.
                    if self.coct && t.n.dot(c - t.v[0]) < 0.0 {
                        continue;
                    }
                    let d = c - q;
                    let dist = d.length();
                    if dist < radius {
                        let mut n = if dist > 1e-4 { d / dist } else { t.n };
                        n.y = 0.0;
                        let n = n.normalize_or_zero();
                        if n == Vec3::ZERO {
                            continue;
                        }
                        let push = radius - dist + 0.01;
                        *pos += n * push;
                        touched.push(n);
                        moved = true;
                    }
                }
                if !moved {
                    break;
                }
            }
        }
        touched
    }

    /// A start position: the centre of the largest flat floor triangle (open ground rather than
    /// the tops of props). Stand-in until spawn points are read from the area's event data.
    pub fn spawn_point(&self) -> Vec3 {
        let best = self
            .tris
            .iter()
            .filter(|t| t.n.y > 0.95)
            .max_by(|a, b| area(a).total_cmp(&area(b)))
            .map(|t| (t.v[0] + t.v[1] + t.v[2]) / 3.0);
        match best {
            Some(p) => Vec3::new(p.x, self.ground(p, p.y + 50.0, 500.0).map_or(p.y, |g| g.y), p.z),
            None => self.spawn_point_band(),
        }
    }

    /// The height band with the most upward-facing area, at its point closest to the centre.
    pub fn spawn_point_band(&self) -> Vec3 {
        let mut bands: HashMap<i32, f32> = HashMap::new();
        let floor = |t: &Tri| (t.n.y > 0.8).then(|| ((t.v[1] - t.v[0]).cross(t.v[2] - t.v[0]).length() * 0.5, (t.v[0].y + t.v[1].y + t.v[2].y) / 3.0));
        for t in &self.tris {
            if let Some((area, y)) = floor(t) {
                *bands.entry((y / 100.0).round() as i32).or_default() += area;
            }
        }
        let Some((&band, _)) = bands.iter().max_by(|a, b| a.1.total_cmp(b.1)) else { return Vec3::ZERO };
        let mid = (self.min + self.max) * 0.5;
        let best = self
            .tris
            .iter()
            .filter(|t| floor(t).is_some_and(|(_, y)| (y / 100.0).round() as i32 == band))
            .map(|t| (t.v[0] + t.v[1] + t.v[2]) / 3.0)
            .min_by(|a, b| a.xz().distance_squared(mid.xz()).total_cmp(&b.xz().distance_squared(mid.xz())));
        let p = best.unwrap_or(mid);
        Vec3::new(p.x, self.ground(p, p.y + 50.0, 500.0).map_or(p.y, |g| g.y), p.z)
    }

    /// First triangle with any of `mask`'s collision flags that the capsule (segment `a`-`b`, radius
    /// `r`, metres) touches: (contact point in metres, surface normal).
    pub fn touching_m(&self, a: Vec3, b: Vec3, r: f32, mask: u32) -> Option<(Vec3, Vec3)> {
        let (a, b, r) = (a * Self::M, b * Self::M, r * Self::M);
        let mut ids = Vec::new();
        self.near(a.min(b) - Vec3::splat(r), a.max(b) + Vec3::splat(r), &mut ids);
        ids.iter()
            .map(|&i| &self.tris[i as usize])
            .filter(|t| t.flags & mask != 0 && t.n.y.abs() < FLOOR_NY)
            .filter_map(|t| {
                let (q, p) = closest_segment_triangle(a, b, t);
                let d = q.distance(p);
                (d <= r).then_some((d, q / Self::M, t.n))
            })
            .min_by(|x, y| x.0.total_cmp(&y.0))
            .map(|(_, q, n)| (q, n))
    }

    /// Distance along the segment to the first triangle (any orientation).
    #[allow(dead_code)]
    pub fn raycast(&self, p: Vec3, q: Vec3) -> Option<f32> {
        let d = q - p;
        let len = d.length();
        if len < 1e-3 || !p.is_finite() || !q.is_finite() {
            return None;
        }
        let dir = d / len;
        let mut ids = Vec::new();
        self.near(p.min(q), p.max(q), &mut ids);
        ids.iter().filter_map(|&i| ray_tri(p, dir, &self.tris[i as usize].v).filter(|&t| t <= len)).min_by(f32::total_cmp)
    }
}

/// Metre-space helpers for the enemies' simple mover and the camera.
impl World {
    const M: f32 = 100.0;

    pub fn ground_m(&self, x: f32, z: f32, from_y: f32) -> Option<f32> {
        self.ground(Vec3::new(x, 0.0, z) * Self::M, from_y * Self::M, 10_000.0).map(|g| g.y / Self::M)
    }

    pub fn blocked_m(&self, p: Vec3, q: Vec3) -> bool {
        let (p, q) = (p * Self::M, q * Self::M);
        let mut ids = Vec::new();
        self.near(p.min(q), p.max(q), &mut ids);
        let d = q - p;
        let len = d.length();
        len > 1e-3 && ids.iter().any(|&i| {
            let t = &self.tris[i as usize];
            t.n.y.abs() < FLOOR_NY && ray_tri(p, d / len, &t.v).is_some_and(|x| x <= len)
        })
    }

    #[allow(dead_code)]
    pub fn raycast_m(&self, p: Vec3, q: Vec3) -> Option<f32> {
        self.raycast(p * Self::M, q * Self::M).map(|d| d / Self::M)
    }

    /// Gravity-integrated move with wall sliding, step-up and ground snapping (metres).
    pub fn step_m(&self, pos: &mut Vec3, vel: &mut Vec3, dt: f32, gravity: f32, fall_max: f32, grounded: &mut bool) {
        const RADIUS: f32 = 30.0;
        const STEP: f32 = 40.0;
        if !pos.is_finite() || !vel.is_finite() {
            *vel = Vec3::ZERO;
            if !pos.is_finite() {
                *pos = self.spawn_point() / Self::M;
            }
        }
        let mut p = *pos * Self::M;
        self.slide(&mut p, Vec3::new(vel.x, 0.0, vel.z) * dt * Self::M, RADIUS, STEP, 150.0);
        vel.y = (vel.y - gravity * dt).max(-fall_max);
        let next = p.y + vel.y * dt * Self::M;
        match self.ground(p, p.y + STEP, STEP + (p.y - next).max(0.0) + 4.0) {
            Some(g) if next <= g.y || (*grounded && vel.y <= 0.0) => {
                p.y = g.y;
                vel.y = vel.y.max(0.0);
                *grounded = true;
            }
            _ => {
                p.y = next;
                *grounded = false;
            }
        }
        *pos = p / Self::M;
        if p.y < self.min.y - 5000.0 {
            *pos = self.spawn_point() / Self::M;
            *vel = Vec3::ZERO;
        }
    }
}

fn area(t: &Tri) -> f32 {
    (t.v[1] - t.v[0]).cross(t.v[2] - t.v[0]).length() * 0.5
}

/// Render-mesh triangles for camera occlusion (the camera is not stopped by invisible collision).
#[derive(Resource, Default)]
pub struct CameraWorld(pub World);

fn height_at(t: &Tri, x: f32, z: f32) -> Option<f32> {
    let [a, b, c] = t.v;
    let v0 = Vec2::new(b.x - a.x, b.z - a.z);
    let v1 = Vec2::new(c.x - a.x, c.z - a.z);
    let v2 = Vec2::new(x - a.x, z - a.z);
    let den = v0.x * v1.y - v1.x * v0.y;
    if den.abs() < 1e-6 {
        return None;
    }
    let v = (v2.x * v1.y - v1.x * v2.y) / den;
    let w = (v0.x * v2.y - v2.x * v0.y) / den;
    const E: f32 = -1e-4;
    (v >= E && w >= E && v + w <= 1.0 - E).then_some(a.y + v * (b.y - a.y) + w * (c.y - a.y))
}

fn closest_point_triangle(p: Vec3, t: &Tri) -> Vec3 {
    // Ericson, Real-Time Collision Detection, 5.1.5.
    let [a, b, c] = t.v;
    let (ab, ac, ap) = (b - a, c - a, p - a);
    let (d1, d2) = (ab.dot(ap), ac.dot(ap));
    if d1 <= 0.0 && d2 <= 0.0 {
        return a;
    }
    let bp = p - b;
    let (d3, d4) = (ab.dot(bp), ac.dot(bp));
    if d3 >= 0.0 && d4 <= d3 {
        return b;
    }
    let vc = d1 * d4 - d3 * d2;
    if vc <= 0.0 && d1 >= 0.0 && d3 <= 0.0 {
        return a + ab * (d1 / (d1 - d3));
    }
    let cp = p - c;
    let (d5, d6) = (ab.dot(cp), ac.dot(cp));
    if d6 >= 0.0 && d5 <= d6 {
        return c;
    }
    let vb = d5 * d2 - d1 * d6;
    if vb <= 0.0 && d2 >= 0.0 && d6 <= 0.0 {
        return a + ac * (d2 / (d2 - d6));
    }
    let va = d3 * d6 - d5 * d4;
    if va <= 0.0 && (d4 - d3) >= 0.0 && (d5 - d6) >= 0.0 {
        return b + (c - b) * ((d4 - d3) / ((d4 - d3) + (d5 - d6)));
    }
    let denom = 1.0 / (va + vb + vc);
    a + ab * (vb * denom) + ac * (vc * denom)
}

/// Closest points (on triangle, on segment) between segment ab and a triangle (iterative).
fn closest_segment_triangle(a: Vec3, b: Vec3, t: &Tri) -> (Vec3, Vec3) {
    let mut s = 0.5f32;
    let ab = b - a;
    let mut q = closest_point_triangle(a + ab * s, t);
    for _ in 0..4 {
        let len2 = ab.length_squared();
        s = if len2 > 1e-6 { ((q - a).dot(ab) / len2).clamp(0.0, 1.0) } else { 0.0 };
        q = closest_point_triangle(a + ab * s, t);
    }
    (q, a + ab * s)
}

/// Möller–Trumbore ray/triangle intersection (two-sided).
fn ray_tri(o: Vec3, d: Vec3, v: &[Vec3; 3]) -> Option<f32> {
    let (e1, e2) = (v[1] - v[0], v[2] - v[0]);
    let p = d.cross(e2);
    let det = e1.dot(p);
    if det.abs() < 1e-8 {
        return None;
    }
    let inv = 1.0 / det;
    let s = o - v[0];
    let u = s.dot(p) * inv;
    if !(0.0..=1.0).contains(&u) {
        return None;
    }
    let q = s.cross(e1);
    let w = d.dot(q) * inv;
    if w < 0.0 || u + w > 1.0 {
        return None;
    }
    let t = e2.dot(q) * inv;
    (t >= 0.0).then_some(t)
}
