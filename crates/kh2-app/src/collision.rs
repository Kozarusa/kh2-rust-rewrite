//! Ground and wall queries against the map's render triangles (a stand-in until COCT collision
//! is parsed). Triangles are bucketed in a 2D grid over XZ.

use bevy::prelude::*;
use std::collections::HashMap;

const CELL: f32 = 4.0; // metres

#[derive(Resource, Default)]
pub struct Collider {
    tris: Vec<[Vec3; 3]>,
    grid: HashMap<(i32, i32), Vec<u32>>,
    min: Vec3,
    max: Vec3,
}

fn cell(v: f32) -> i32 {
    (v / CELL).floor() as i32
}

impl Collider {
    pub fn from_meshes(meshes: &[kh2_model::Mesh], scale: f32) -> Collider {
        let mut c = Collider { min: Vec3::splat(f32::MAX), max: Vec3::splat(f32::MIN), ..default() };
        for m in meshes.iter().filter(|m| !m.transparent) {
            for t in m.indices.as_chunks::<3>().0 {
                let v = [t[0], t[1], t[2]].map(|i| Vec3::from(m.positions[i as usize]) * scale);
                let n = (v[1] - v[0]).cross(v[2] - v[0]);
                if n.length_squared() < 1e-10 {
                    continue;
                }
                let id = c.tris.len() as u32;
                c.tris.push(v);
                let lo = v[0].min(v[1]).min(v[2]);
                let hi = v[0].max(v[1]).max(v[2]);
                c.min = c.min.min(lo);
                c.max = c.max.max(hi);
                for x in cell(lo.x)..=cell(hi.x) {
                    for z in cell(lo.z)..=cell(hi.z) {
                        c.grid.entry((x, z)).or_default().push(id);
                    }
                }
            }
        }
        c
    }

    /// Highest walkable surface at (x, z) at or below `from_y`.
    pub fn ground(&self, x: f32, z: f32, from_y: f32) -> Option<f32> {
        let mut best: Option<f32> = None;
        for &i in self.grid.get(&(cell(x), cell(z)))?.iter() {
            let [a, b, c] = self.tris[i as usize];
            let n = (b - a).cross(c - a).normalize();
            if n.y.abs() < 0.5 {
                continue; // too steep to stand on
            }
            if let Some(y) = height_at(a, b, c, x, z)
                && y <= from_y
                && best.is_none_or(|b| y > b)
            {
                best = Some(y);
            }
        }
        best
    }

    /// Distance along the segment `p`→`q` to the first triangle hit (any orientation).
    pub fn first_hit(&self, p: Vec3, q: Vec3) -> Option<f32> {
        let d = q - p;
        let len = d.length();
        if len < 1e-5 {
            return None;
        }
        let dir = d / len;
        let mut best: Option<f32> = None;
        for x in cell(p.x.min(q.x))..=cell(p.x.max(q.x)) {
            for z in cell(p.z.min(q.z))..=cell(p.z.max(q.z)) {
                for &i in self.grid.get(&(x, z)).into_iter().flatten() {
                    let [a, b, c] = self.tris[i as usize];
                    if let Some(t) = ray_tri(p, dir, a, b, c).filter(|&t| t <= len) {
                        best = Some(best.map_or(t, |b: f32| b.min(t)));
                    }
                }
            }
        }
        best
    }

    /// True if a horizontal segment at height `y` from `p` to `q` crosses a wall.
    pub fn blocked(&self, p: Vec3, q: Vec3) -> bool {
        let d = q - p;
        let len = d.length();
        if len < 1e-5 {
            return false;
        }
        let dir = d / len;
        let mut ids: Vec<u32> = Vec::new();
        for x in cell(p.x.min(q.x))..=cell(p.x.max(q.x)) {
            for z in cell(p.z.min(q.z))..=cell(p.z.max(q.z)) {
                if let Some(v) = self.grid.get(&(x, z)) {
                    ids.extend(v);
                }
            }
        }
        ids.iter().any(|&i| {
            let [a, b, c] = self.tris[i as usize];
            let n = (b - a).cross(c - a).normalize();
            n.y.abs() < 0.5 && ray_tri(p, dir, a, b, c).is_some_and(|t| t <= len)
        })
    }

    /// A start position on the main floor: the height band with the most upward-facing area
    /// (streets and room floors, not roofs), at the point of that band closest to the map centre.
    /// Stand-in until spawn points are read from the area's event data.
    pub fn spawn_point(&self) -> Vec3 {
        let mut bands: HashMap<i32, f32> = HashMap::new();
        let floor = |t: &[Vec3; 3]| {
            let n = (t[1] - t[0]).cross(t[2] - t[0]);
            (n.normalize().y.abs() > 0.8).then(|| (n.length() * 0.5, (t[0].y + t[1].y + t[2].y) / 3.0))
        };
        for t in &self.tris {
            if let Some((area, y)) = floor(t) {
                *bands.entry(y.round() as i32).or_default() += area;
            }
        }
        let Some((&band, _)) = bands.iter().max_by(|a, b| a.1.total_cmp(b.1)) else { return Vec3::ZERO };
        let mid = (self.min + self.max) * 0.5;
        let best = self
            .tris
            .iter()
            .filter(|t| floor(t).is_some_and(|(_, y)| y.round() as i32 == band))
            .map(|t| (t[0] + t[1] + t[2]) / 3.0)
            .min_by(|a, b| a.xz().distance_squared(mid.xz()).total_cmp(&b.xz().distance_squared(mid.xz())));
        let p = best.unwrap_or(mid);
        Vec3::new(p.x, self.ground(p.x, p.z, p.y + 0.5).unwrap_or(p.y) + 0.05, p.z)
    }
}

fn height_at(a: Vec3, b: Vec3, c: Vec3, x: f32, z: f32) -> Option<f32> {
    let v0 = Vec2::new(b.x - a.x, b.z - a.z);
    let v1 = Vec2::new(c.x - a.x, c.z - a.z);
    let v2 = Vec2::new(x - a.x, z - a.z);
    let den = v0.x * v1.y - v1.x * v0.y;
    if den.abs() < 1e-8 {
        return None;
    }
    let v = (v2.x * v1.y - v1.x * v2.y) / den;
    let w = (v0.x * v2.y - v2.x * v0.y) / den;
    (v >= 0.0 && w >= 0.0 && v + w <= 1.0).then_some(a.y + v * (b.y - a.y) + w * (c.y - a.y))
}

/// Möller–Trumbore ray/triangle intersection (two-sided). Returns the hit distance.
fn ray_tri(o: Vec3, d: Vec3, a: Vec3, b: Vec3, c: Vec3) -> Option<f32> {
    let (e1, e2) = (b - a, c - a);
    let p = d.cross(e2);
    let det = e1.dot(p);
    if det.abs() < 1e-8 {
        return None;
    }
    let inv = 1.0 / det;
    let s = o - a;
    let u = s.dot(p) * inv;
    if !(0.0..=1.0).contains(&u) {
        return None;
    }
    let q = s.cross(e1);
    let v = d.dot(q) * inv;
    if v < 0.0 || u + v > 1.0 {
        return None;
    }
    let t = e2.dot(q) * inv;
    (t >= 0.0).then_some(t)
}
