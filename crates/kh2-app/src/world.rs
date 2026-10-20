//! Map collision from the area's COCT (falls back to render triangles when an area has none).
//!
//! Everything here is in game units (centimetres) in map-model space, Y up. Characters are
//! vertical capsules; floors are found with a downward probe, walls push the capsule out along
//! the polygon normal (sliding), ceilings stop upward motion.

use bevy::prelude::*;
use std::collections::HashMap;

const CELL: f32 = 400.0;
/// A triangle that spans more cells than this goes to `World::big` instead of the grid (map eh22 has a
/// floor 2.9 million units wide, which as grid entries needed 2 GB).
const MAX_GRID_CELLS: i64 = 1024;
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
    /// Triangles that cover more than `MAX_GRID_CELLS` cells (a sky-sized floor): kept out of the grid,
    /// which would need one entry per cell, and tested directly by every query.
    big: Vec<u32>,
    pub min: Vec3,
    pub max: Vec3,
    /// True when built from COCT data.
    pub coct: bool,
    /// The COCT mesh each triangle came from, indexed like `tris` (empty or short for worlds that did not
    /// come from `from_coct`; use `mesh_of`).
    tri_mesh: Vec<u16>,
    /// The mesh group (a bit number of the map visibility word) of each triangle, like `tri_mesh`.
    tri_group: Vec<u8>,
    /// The groups the game's walker visits: bit `g` set means meshes of group `g` are solid
    /// (`FUN_00141768` sets a mesh's enabled byte from the map visibility word). All ones until the
    /// area script's word is applied (`area::run_area_script`); BDX host calls 1.36 / 1.37 change single bits
    /// (`set_group`). Every query skips off groups (`near`); `group_enabled` answers for one triangle.
    pub enabled_groups: u64,
    /// The area word has been written into `enabled_groups` (once per loaded world: after that only host
    /// calls 1.36 / 1.37 change a bit, `FUN_0015e6c0` copies the word to the live mask a single time).
    area_word_applied: bool,
}

/// Group value of a triangle that has no COCT mesh.
pub const NO_GROUP: u8 = 0xff;

#[derive(Debug, Clone, Copy)]
pub struct Ground {
    pub y: f32,
    pub normal: Vec3,
    #[allow(dead_code)] // ground flags word of the collision hit; no reader yet
    pub flags: u32,
    pub material: u8,
}

fn cell(v: f32) -> i32 {
    (v / CELL).floor() as i32
}

impl World {
    fn push(&mut self, v: [Vec3; 3], flags: u32, material: u8) {
        self.push_in(v, flags, material, 0, NO_GROUP);
    }

    fn push_in(&mut self, v: [Vec3; 3], flags: u32, material: u8, mesh: u16, group: u8) {
        let n = (v[1] - v[0]).cross(v[2] - v[0]);
        if n.length_squared() < 1e-6 {
            return;
        }
        let lo = v[0].min(v[1]).min(v[2]);
        let hi = v[0].max(v[1]).max(v[2]);
        let id = self.tris.len() as u32;
        self.tris.push(Tri { v, n: n.normalize(), flags, material, lo, hi });
        self.tri_mesh.resize(id as usize, 0);
        self.tri_group.resize(id as usize, NO_GROUP);
        self.tri_mesh.push(mesh);
        self.tri_group.push(group);
        self.min = self.min.min(lo);
        self.max = self.max.max(hi);
        let cells = (cell(hi.x) as i64 - cell(lo.x) as i64 + 1) * (cell(hi.z) as i64 - cell(lo.z) as i64 + 1);
        if cells > MAX_GRID_CELLS {
            self.big.push(id);
            return;
        }
        for x in cell(lo.x)..=cell(hi.x) {
            for z in cell(lo.z)..=cell(hi.z) {
                self.grid.entry((x, z)).or_default().push(id);
            }
        }
    }

    fn empty() -> World {
        World { min: Vec3::splat(f32::MAX), max: Vec3::splat(f32::MIN), enabled_groups: u64::MAX, ..default() }
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
                w.push_in(v, p.flags, p.material, p.mesh, c.group_of(p));
            }
        }
        w
    }

    /// The COCT mesh index of triangle `i` (0 when it has none).
    #[allow(dead_code)] // read by the eh10 test; the walker gates by group only
    pub fn mesh_of(&self, i: usize) -> u16 {
        self.tri_mesh.get(i).copied().unwrap_or(0)
    }

    /// The mesh group of triangle `i`, `NO_GROUP` when it has none.
    pub fn group_of(&self, i: usize) -> u8 {
        self.tri_group.get(i).copied().unwrap_or(NO_GROUP)
    }

    /// Whether the game's walker would visit triangle `i` under `enabled_groups`. Triangles without a
    /// group always count.
    pub fn group_enabled(&self, i: usize) -> bool {
        match self.group_of(i) {
            NO_GROUP => true,
            g if g < 64 => self.enabled_groups >> g & 1 != 0,
            _ => false,
        }
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
        for &i in &self.big {
            let t = &self.tris[i as usize];
            if t.hi.x >= lo.x && t.lo.x <= hi.x && t.hi.y >= lo.y && t.lo.y <= hi.y && t.hi.z >= lo.z && t.lo.z <= hi.z {
                out.push(i);
            }
        }
        out.sort_unstable();
        out.dedup();
        // The game's walker FUN_00141b50 visits no mesh whose enabled byte is 0, and every collision query
        // (the ground probe FUN_00142760, the capsule sweep FUN_00164138, the ray cast FUN_00141f08 and so
        // the ceiling and wall tests) goes through it, so a triangle of an off group is in no query.
        out.retain(|&i| self.group_enabled(i as usize));
    }

    /// Writes the area script's map visibility word into the live mask, once per world (`FUN_001047a8` ->
    /// `FUN_00149b40` -> `FUN_0015e6c0`). Returns whether it was written by this call.
    pub fn apply_area_word(&mut self, word: u64) -> bool {
        if self.area_word_applied {
            return false;
        }
        self.area_word_applied = true;
        self.enabled_groups = word;
        true
    }

    /// Switches mesh group `g` on or off (`FUN_0015e9a8` / `FUN_0015e928`: bit `g` of the live mask
    /// `DAT_0035a810`, then the enabled byte of every mesh of the group by `FUN_00141840` / `FUN_00141800`).
    /// Bits 64 and above do not exist in the word and are ignored.
    pub fn set_group(&mut self, g: u32, on: bool) {
        if g < 64 {
            if on { self.enabled_groups |= 1 << g } else { self.enabled_groups &= !(1 << g) }
        }
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

    /// Moves a vertical capsule (feet at `pos`) horizontally by `delta`, sliding along the walls that
    /// stop the party (`kh2_map::stops_party`: PARTY_COLLIDE, not nibble 4, not the solver-skip bit).
    /// Returns the wall normals touched.
    pub fn slide(&self, pos: &mut Vec3, delta: Vec3, radius: f32, bottom: f32, top: f32) -> Vec<Vec3> {
        self.slide_with(pos, delta, radius, bottom, top, true)
    }

    /// `slide`, with the walking filter optional: `party` false stops at every wall-like polygon (the
    /// enemies' simple mover, whose collision kind was not read).
    pub fn slide_with(&self, pos: &mut Vec3, delta: Vec3, radius: f32, bottom: f32, top: f32, party: bool) -> Vec<Vec3> {
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
                    if t.n.y.abs() >= FLOOR_NY || (party && self.coct && !kh2_map::stops_party(t.flags)) {
                        continue;
                    }
                    // Meshes of a group outside the map visibility word are not visited by the game's walker.
                    if self.coct && !self.group_enabled(i as usize) {
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
            .enumerate()
            .filter(|(i, t)| t.n.y > 0.95 && self.group_enabled(*i))
            .max_by(|a, b| area(a.1).total_cmp(&area(b.1)))
            .map(|(_, t)| (t.v[0] + t.v[1] + t.v[2]) / 3.0);
        match best {
            Some(p) => Vec3::new(p.x, self.ground(p, p.y + 50.0, 500.0).map_or(p.y, |g| g.y), p.z),
            None => self.spawn_point_band(),
        }
    }

    /// The height band with the most upward-facing area, at its point closest to the centre.
    pub fn spawn_point_band(&self) -> Vec3 {
        let mut bands: HashMap<i32, f32> = HashMap::new();
        let floor = |t: &Tri| (t.n.y > 0.8).then(|| ((t.v[1] - t.v[0]).cross(t.v[2] - t.v[0]).length() * 0.5, (t.v[0].y + t.v[1].y + t.v[2].y) / 3.0));
        for (i, t) in self.tris.iter().enumerate() {
            if !self.group_enabled(i) {
                continue;
            }
            if let Some((area, y)) = floor(t) {
                *bands.entry((y / 100.0).round() as i32).or_default() += area;
            }
        }
        let Some((&band, _)) = bands.iter().max_by(|a, b| a.1.total_cmp(b.1)) else { return Vec3::ZERO };
        let mid = (self.min + self.max) * 0.5;
        let best = self
            .tris
            .iter()
            .enumerate()
            .filter(|(i, t)| self.group_enabled(*i) && floor(t).is_some_and(|(_, y)| (y / 100.0).round() as i32 == band))
            .map(|(_, t)| (t.v[0] + t.v[1] + t.v[2]) / 3.0)
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
            .filter(|&&i| self.group_enabled(i as usize))
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

    /// The nearest triangle carrying any of `mask`'s flags whose FRONT face the segment `p`..`q`
    /// (metres) crosses: (hit point in metres, surface normal). Floors and ceilings count as much as
    /// walls. This is the game's segment cast FUN_00143aa0 -> FUN_00141f08: the polygons of the
    /// swept box pass the mask handler (FUN_00141b50 -> FUN_0032e3b8; handler 4 at 0x32e420 is
    /// `attribute & 0x80`, with no orientation test), each is cut by FUN_001623a0 only when the
    /// segment runs against its normal (`n . dir < 0`) with the parameter in 0..=1, and FUN_00141d90
    /// keeps the hit nearest the start.
    pub fn cast_m(&self, p: Vec3, q: Vec3, mask: u32) -> Option<(Vec3, Vec3)> {
        let (p, q) = (p * Self::M, q * Self::M);
        let d = q - p;
        let len = d.length();
        if len < 1e-3 || !p.is_finite() || !q.is_finite() {
            return None;
        }
        let dir = d / len;
        let mut ids = Vec::new();
        self.near(p.min(q), p.max(q), &mut ids);
        ids.iter()
            .filter(|&&i| self.group_enabled(i as usize))
            .map(|&i| &self.tris[i as usize])
            .filter(|t| t.flags & mask != 0 && t.n.dot(dir) < 0.0)
            .filter_map(|t| ray_tri(p, dir, &t.v).filter(|&x| x <= len).map(|x| (x, t.n)))
            .min_by(|a, b| a.0.total_cmp(&b.0))
            .map(|(x, n)| ((p + dir * x) / Self::M, n))
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
        self.slide_with(&mut p, Vec3::new(vel.x, 0.0, vel.z) * dt * Self::M, RADIUS, STEP, 150.0, false);
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

#[cfg(test)]
mod tests {
    use super::*;

    /// A floor millions of units wide (map eh22) stays out of the cell grid, so building the world is
    /// cheap, and ground queries still find it.
    #[test]
    fn a_floor_wider_than_the_grid_limit_is_found_without_grid_entries() {
        let big = 1_440_000.0;
        let mut w = World::empty();
        w.push([Vec3::new(-big, 0.0, -big), Vec3::new(big, 0.0, -big), Vec3::new(0.0, 0.0, big)], 0, 0);
        w.push([Vec3::new(-100.0, 50.0, -100.0), Vec3::new(100.0, 50.0, -100.0), Vec3::new(0.0, 50.0, 100.0)], 0, 0);
        assert_eq!(w.big.len(), 1);
        assert!(w.grid.len() < 64, "{} cells", w.grid.len());
        let mut out = Vec::new();
        w.near(Vec3::new(-10.0, -10.0, -50.0), Vec3::new(10.0, 60.0, -40.0), &mut out);
        assert_eq!(out, vec![0, 1]);
        w.near(Vec3::new(5000.0, -10.0, -50.0), Vec3::new(5010.0, 60.0, -40.0), &mut out);
        assert_eq!(out, vec![0]);
    }

    /// WP-F2 item A: tt00 is not a flat box. Its 16 collision polygons are the 20 m box plus a
    /// 150-unit step across z = 0: a riser (normal -z) and a raised floor (y = 150) for z > 0, all
    /// carrying the attack-blocking flag. Sora spawns at (115, 0, -35) facing +z, 35 units from the
    /// riser, so a swing there crosses the raised floor from above and the cast reports terrain; a
    /// swing on the low floor facing -z crosses nothing (harness runs in docs/movement.md).
    #[test]
    fn tt00_has_an_attack_blocking_step_in_front_of_the_spawn() {
        let Some(iso) = crate::assets::iso_path().ok().filter(|p| p.exists()) else { return };
        let disc = crate::assets::Disc::open(&iso);
        let read = |p: &str| disc.read(p).unwrap_or_else(|| panic!("{p}: not readable"));
        let c = kh2_map::Coct::from_map(&read("map/us/tt00.map")).unwrap();
        assert_eq!(c.polygons.len(), 16);
        assert!(c.polygons.iter().all(|p| p.flags & kh2_map::flags::ATTACK_COLLIDE != 0), "every tt00 polygon blocks attacks");
        let raised: Vec<_> = c.polygons.iter().filter(|p| p.normal[1] > 0.9 && p.verts[0][1] == 150.0).collect();
        let risers: Vec<_> = c.polygons.iter().filter(|p| p.normal[2] < -0.9 && p.verts[0][2] == 0.0).collect();
        assert_eq!((raised.len(), risers.len()), (2, 2), "two raised-floor and two riser triangles");
        let w = World::from_coct(&c);
        // The cast of the ground swing seen in the harness: the first keyblade volume's centre goes from
        // (121, 187, 47) to (124, 115, 68) and crosses the raised floor at y = 150 from above.
        let (p, n) = w.cast_m(Vec3::new(1.21, 1.87, 0.47), Vec3::new(1.24, 1.15, 0.68), kh2_map::flags::ATTACK_COLLIDE).expect("the step");
        assert!((p.y - 1.5).abs() < 1e-4 && n.y > 0.9, "{p:?} {n:?}");
        // The same movement on the low floor, 3 m back from the step: nothing.
        assert_eq!(w.cast_m(Vec3::new(1.21, 1.87, -3.5), Vec3::new(1.24, 1.15, -3.3), kh2_map::flags::ATTACK_COLLIDE), None);
        // The riser is one-sided: a segment through it from the raised side (z > 0 toward z < 0) is ignored.
        assert_eq!(w.cast_m(Vec3::new(1.0, 1.0, 0.5), Vec3::new(1.0, 1.0, -0.5), kh2_map::flags::ATTACK_COLLIDE), None);
        assert!(w.cast_m(Vec3::new(1.0, 1.0, -0.5), Vec3::new(1.0, 1.0, 0.5), kh2_map::flags::ATTACK_COLLIDE).is_some());
    }

    /// The world as the player's slide saw it before WP-G3: every polygon carries the party bit (and
    /// nothing else), so every wall-like polygon stops.
    fn old_world(c: &kh2_map::Coct) -> World {
        let mut c = c.clone();
        for p in &mut c.polygons {
            p.flags = kh2_map::flags::PARTY_COLLIDE;
        }
        World::from_coct(&c)
    }

    /// Walks a grounded Sora-sized motor from `start` (x, z; the floor under `from_y`) along `dir` for 30
    /// ticks of 8 units (a run) and returns where it ends.
    fn walk(w: &World, start: [f32; 2], from_y: f32, dir: Vec3) -> Vec3 {
        let p = Vec3::new(start[0], from_y, start[1]);
        let g = w.ground(p, from_y + 60.0, 200.0).expect("floor under the start");
        let mut m = crate::motor::Motor { pos: p.with_y(g.y), grounded: true, ..Default::default() };
        for _ in 0..30 {
            m.move_by(w, dir * 8.0, None);
        }
        m.pos
    }

    /// One scripted walk of the invisible-wall test: the world pair, start (x, z), floor search height,
    /// direction, the wall's plane coordinate on the walking axis, and whether the walk goes toward +axis.
    type WalkCase<'a> = (&'a World, &'a World, [f32; 2], f32, Vec3, f32, bool);

    /// Walks a grounded Sora-sized motor from `start` straight at `target` (x, z) in runs of 8 units per tick
    /// until it is within 60 units or 600 ticks passed; returns the end position.
    fn walk_to(w: &World, start: Vec3, target: Vec3) -> Vec3 {
        let g = w.ground(start, start.y + 60.0, 300.0).expect("floor under the start");
        let mut m = crate::motor::Motor { pos: start.with_y(g.y), grounded: true, ..Default::default() };
        for _ in 0..600 {
            let d = (target - m.pos).with_y(0.0);
            if d.length() < 60.0 {
                break;
            }
            m.move_by(w, d.normalize() * 8.0, None);
        }
        m.pos
    }

    /// WP-IW1 (user report 2026-10-20: invisible walls keep Sora out of every enemy spawn region of eh10). The
    /// map's COCT meshes carry a group number (byte +0x11) and the game's walker `FUN_00141b50` visits only
    /// meshes whose group bit is in the map visibility word (`FUN_00141768`, built from the area script's
    /// opcodes 1, 0x1a and 0x1b). eh10 has arena walls in groups 3 to 8 (attribute 0x172, solid for the party)
    /// that the area script of program 0,1,0 leaves out of the word 0x407. Without the group filter Sora
    /// starting at the app's stand-in spawn point cannot reach the group b_00 volume; with the word set he does.
    #[test]
    fn eh10_arena_walls_of_hidden_groups_do_not_block_the_spawn_volume() {
        let Some(iso) = crate::assets::iso_path().ok().filter(|p| p.exists()) else { return };
        let disc = crate::assets::Disc::open(&iso);
        let map = disc.read("map/us/eh10.map").unwrap();
        let coct = kh2_map::Coct::from_map(&map).unwrap();
        let (ard, _) = kh2_ard::Area::parse(&disc.read("ard/eh10.ard").unwrap()).unwrap();
        let word = kh2_ard::run::run_area(&ard, [0, 1, 0], 0).map_visibility;
        assert_eq!(word, 0x407, "visibility word of eh10 program 0,1,0");
        let mut all = World::from_coct(&coct);
        let mut gated = World::from_coct(&coct);
        gated.enabled_groups = word;
        // The walls of groups 3 to 8 exist in the mesh table and are party walls.
        let hidden = |w: &World| (0..w.tris.len()).filter(|&i| !w.group_enabled(i) && w.tris[i].n.y.abs() < FLOOR_NY && kh2_map::stops_party(w.tris[i].flags)).count();
        assert_eq!((hidden(&all), hidden(&gated) >= 100), (0, true));
        // Every triangle carries its mesh and the mesh's group.
        assert!((0..gated.tris.len()).all(|i| coct.meshes[gated.mesh_of(i) as usize].group == gated.group_of(i)));
        // The kind-2 volume of group b_00 (the one the play-test spawns into).
        let vol = ard.spawns.values().flatten().filter(|g| g.kind & 0xff == 2).flat_map(|g| g.activators.iter()).next().expect("a kind-2 volume");
        let target = crate::area::game_to_app(vol.position);
        let cam = World::from_meshes(&kh2_model::map_file(&map).unwrap().map.meshes);
        let (_, spawn) = crate::assets::spawn_point(&all, &cam);
        let entrances: Vec<Vec3> = (0..2u8)
            .filter_map(|n| kh2_ard::run::player_start(&ard, &kh2_ard::run::run_area(&ard, [0, 1, 0], n), n).map(|(_, e)| crate::area::game_to_app(e.position)))
            .collect();
        assert_eq!(entrances.len(), 2);
        // `walk_to` stops within 60 units of the target; a walk that ends farther out was stopped by a wall.
        let arrived = |p: Vec3| (p - target).with_y(0.0).length() < 60.0;
        let starts = [("startup spawn", spawn), ("entrance 0", entrances[0]), ("entrance 1", entrances[1])];
        for (name, start) in starts {
            let end = walk_to(&gated, start, target);
            assert!(arrived(end), "{name}: with the group filter Sora arrives at the volume ({start:?} -> {end:?}, target {target:?})");
        }
        // Without the filter (every group solid, as the app did) the startup spawn and entrance 0 are walled in
        // (entrance 1 is outside the arena walls and arrives either way), and each blocked walk ends far from
        // where the gated walk ends.
        all.enabled_groups = u64::MAX;
        let mut blocked = Vec::new();
        for (name, s) in starts {
            let (ungated, with_filter) = (walk_to(&all, s, target), walk_to(&gated, s, target));
            eprintln!("iw1 eh10 {name}: start {s:?}, ungated ends {ungated:?}, gated ends {with_filter:?}, target {target:?}");
            if !arrived(ungated) {
                assert!((ungated - with_filter).length() > 100.0, "{name}: the blocked and the gated walk end apart ({ungated:?} vs {with_filter:?})");
                blocked.push(name);
            }
        }
        assert_eq!(blocked, ["startup spawn", "entrance 0"], "the ungated world walls in exactly these starts");
    }

    /// WP-IW1: he05 and al01 (the other areas of the report). `kh2 coct-walls` finds no hidden-group wall between
    /// these entrances and spawn volumes (straight lines at knee height cross no party wall), so Sora walks
    /// from the entrance into the volume with the group filter set from the area script's word.
    #[test]
    fn he05_and_al01_entrances_walk_into_their_clear_spawn_volumes() {
        let Some(iso) = crate::assets::iso_path().ok().filter(|p| p.exists()) else { return };
        let disc = crate::assets::Disc::open(&iso);
        // (area, entrance, spawn group id of a kind-2 volume 'kh2 coct-walls' reports as clear).
        for (area, entrance, group) in [("he05", 0u8, 56u16), ("he05", 1, 58), ("al01", 0, 39), ("al01", 1, 36), ("al01", 1, 48)] {
            let coct = kh2_map::Coct::from_map(&disc.read(&format!("map/us/{area}.map")).unwrap()).unwrap();
            let (ard, _) = kh2_ard::Area::parse(&disc.read(&format!("ard/{area}.ard")).unwrap()).unwrap();
            let run = kh2_ard::run::run_area(&ard, [0, 1, 0], entrance);
            let mut w = World::from_coct(&coct);
            w.enabled_groups = run.map_visibility;
            let (_, e) = kh2_ard::run::player_start(&ard, &run, entrance).expect("player entity");
            let vol = ard.spawns.values().flatten().find(|g| g.kind & 0xff == 2 && g.id == group).and_then(|g| g.activators.first()).expect("volume");
            let (start, target) = (crate::area::game_to_app(e.position), crate::area::game_to_app(vol.position));
            let end = walk_to(&w, start, target);
            let left = (end - target).with_y(0.0).length();
            assert!(vol.contains([end.x, -end.y, -end.z]) || left < 80.0, "{area} entrance {entrance} -> group {group}: ended {end:?}, {left:.0} from {target:?}");
        }
    }

    /// WP-IW1: a wall in mesh group 3 answers the slide, the capsule touch, the ray cast and the motor's sweep only
    /// while bit 3 is in `enabled_groups` (FUN_00141768 gates the game's walker the same way for every query).
    #[test]
    fn hidden_group_walls_are_ignored_by_every_query() {
        let flags = kh2_map::flags::PARTY_COLLIDE | kh2_map::flags::ATTACK_COLLIDE;
        let wall = kh2_map::Polygon { verts: [[100.0, -500.0, -500.0], [100.0, 500.0, -500.0], [100.0, 500.0, 500.0], [100.0, -500.0, 500.0]], quad: true, normal: [-1.0, 0.0, 0.0], d: 100.0, flags, material: 0, mesh: 0 };
        let coct = kh2_map::Coct { polygons: vec![wall], meshes: vec![kh2_map::Mesh { group: 3, ..Default::default() }], ..Default::default() };
        let mut w = World::from_coct(&coct);
        let (a, b) = (Vec3::new(0.0, 50.0, 0.0), Vec3::new(200.0, 50.0, 0.0));
        let probe = |w: &World| {
            let mut pos = Vec3::ZERO;
            w.slide(&mut pos, Vec3::new(200.0, 0.0, 0.0), 30.0, 10.0, 100.0);
            (pos.x < 100.0, w.touching_m(a / World::M, b / World::M, 0.3 / World::M, flags).is_some(), w.cast_m(a / World::M, b / World::M, flags).is_some(), crate::motor::sweep_wall(w, a, b, 10.0, 0x10).is_some())
        };
        w.enabled_groups = 1 << 3;
        assert_eq!(probe(&w), (true, true, true, true), "group on: the wall answers every query");
        w.enabled_groups = !(1 << 3);
        assert_eq!(probe(&w), (false, false, false, false), "group off: no query sees the wall");
    }

    /// WP-IW2: the floor, ceiling, vertical wall test, ray cast and start-position searches also go through the
    /// walker (`FUN_00142760` ground probe, `FUN_00141f08` ray, `FUN_00164138` sweep all call `FUN_00141b50`), so a
    /// box of a switched-off group is no floor, ceiling or wall, and a group switched back on is solid again.
    #[test]
    fn hidden_groups_are_no_floor_ceiling_wall_or_spawn_point() {
        let poly = |verts: [[f32; 3]; 4], normal: [f32; 3], mesh: u16| kh2_map::Polygon { verts, quad: true, normal, d: 0.0, flags: kh2_map::flags::PARTY_COLLIDE, material: 0, mesh };
        // Mesh 0 (group 3): floor at y = 0 and ceiling at y = 300 over x, z in -500..500, and a wall at x = 100.
        // Mesh 1 (group 9): a larger second floor at y = 80 over x, z in -600..600.
        let polygons = vec![
            poly([[-500.0, 0.0, -500.0], [500.0, 0.0, -500.0], [500.0, 0.0, 500.0], [-500.0, 0.0, 500.0]], [0.0, 1.0, 0.0], 0),
            poly([[-500.0, 300.0, -500.0], [-500.0, 300.0, 500.0], [500.0, 300.0, 500.0], [500.0, 300.0, -500.0]], [0.0, -1.0, 0.0], 0),
            poly([[100.0, -500.0, -500.0], [100.0, 500.0, -500.0], [100.0, 500.0, 500.0], [100.0, -500.0, 500.0]], [-1.0, 0.0, 0.0], 0),
            poly([[-600.0, 80.0, -600.0], [600.0, 80.0, -600.0], [600.0, 80.0, 600.0], [-600.0, 80.0, 600.0]], [0.0, 1.0, 0.0], 1),
        ];
        let meshes = vec![kh2_map::Mesh { group: 3, ..Default::default() }, kh2_map::Mesh { group: 9, ..Default::default() }];
        let mut w = World::from_coct(&kh2_map::Coct { polygons, meshes, ..Default::default() });
        let (a, b) = (Vec3::new(0.0, 0.5, 0.0), Vec3::new(2.0, 0.5, 0.0));
        let probe = |w: &World| {
            let p = Vec3::new(37.0, 0.0, 11.0);
            (
                w.ground(p, 400.0, 1000.0).map(|g| g.y),
                w.ceiling(p, 10.0, 400.0),
                w.blocked_m(a, b),
                w.raycast_m(Vec3::new(0.37, 2.0, 0.11), Vec3::new(0.37, -1.0, 0.11)).map(|d| (d * 100.0).round()),
                w.spawn_point().y,
            )
        };
        // Both groups on: the higher floor (group 9) wins the ground probe and the ray hits it first.
        w.enabled_groups = u64::MAX;
        assert_eq!(probe(&w), (Some(80.0), Some(300.0), true, Some(120.0), 80.0));
        // Group 9 off: the group 3 floor answers.
        w.set_group(9, false);
        assert_eq!(probe(&w), (Some(0.0), Some(300.0), true, Some(200.0), 0.0));
        // Group 3 off as well: no floor, ceiling, wall or ray hit; the start search falls back to the origin.
        w.set_group(3, false);
        assert_eq!(probe(&w), (None, None, false, None, 0.0));
        assert_eq!(w.spawn_point_band(), Vec3::ZERO);
        // Switching a group on again makes it solid, a bit above 63 is ignored.
        w.set_group(9, true);
        w.set_group(64, true);
        assert_eq!(w.enabled_groups, !(1u64 << 3));
        assert_eq!(probe(&w).0, Some(80.0));
    }

    /// WP-G3 (user report 2026-10-11: invisible walls in Tram Common and Station Heights). The player's
    /// slide stopped at every wall-like polygon; the game's walking collider keeps only polygons with
    /// PARTY_COLLIDE (handler 0 of the table at 0x378f7c), not nibble 4, not bit 0x100000. Tram Common
    /// (tt07) and Station Heights (tt06) carry hundreds of wall polygons with attack-only (0x82) or
    /// object-only (0x22, 0x80002) attributes. Each case walks into one from the front: the old world
    /// stops Sora at the plane, the new one lets him through; the real walls (0x172, 0x1f2) stop him in
    /// both.
    #[test]
    fn invisible_walls_of_tram_common_and_station_heights_do_not_stop_sora() {
        let Some(iso) = crate::assets::iso_path().ok().filter(|p| p.exists()) else { return };
        let disc = crate::assets::Disc::open(&iso);
        let load = |n: usize| {
            let c = kh2_map::Coct::from_map(&disc.read(&format!("map/us/tt{n:02}.map")).unwrap()).unwrap();
            (World::from_coct(&c), old_world(&c))
        };
        let (heights, heights_old) = load(6);
        let (tram, tram_old) = load(7);
        // The classes exist: wall-like polygons that do not stop the party.
        let count = |w: &World, attr: u32| w.tris.iter().filter(|t| t.n.y.abs() < FLOOR_NY && t.flags == attr).count();
        assert!(count(&tram, 0x80002) >= 60 && count(&tram, 0x22) >= 100 && count(&tram, 0x82) >= 400);
        assert!(count(&heights, 0x22) >= 7 && count(&heights, 0x82) >= 300);
        let cases: [WalkCase; 5] = [
            // Tram Common: the big volume at z = 2100..2500, x = 550..1600 (0x80002 on its -z face).
            (&tram, &tram_old, [550.0, 1980.0], 0.0, Vec3::Z, 2100.0, true),
            // The same volume from inside, through its 0x22 face (z = 2100, normal +z), walking -z.
            (&tram, &tram_old, [550.0, 2220.0], 5.0, Vec3::NEG_Z, 2100.0, false),
            // A low attack-only wall at x = 3159 (0x82, normal +x), walking -x.
            (&tram, &tram_old, [3279.0, 301.0], 0.0, Vec3::NEG_X, 3159.0, false),
            // Station Heights: an object-only wall at x = -1250 (0x22, normal +x), walking -x.
            (&heights, &heights_old, [-1130.0, -217.0], 0.0, Vec3::NEG_X, -1250.0, false),
            // An attack-only wall at x = 1400 (0x82, normal -x) on the lower level, walking +x.
            (&heights, &heights_old, [1280.0, 340.0], -550.0, Vec3::X, 1400.0, true),
        ];
        for (i, (new, old, start, y, dir, plane, positive)) in cases.into_iter().enumerate() {
            let (a, b) = (walk(old, start, y, dir), walk(new, start, y, dir));
            let coord = |p: Vec3| if dir.x != 0.0 { p.x } else { p.z };
            let before = |p: Vec3| if positive { coord(p) < plane } else { coord(p) > plane };
            let past = |p: Vec3| if positive { coord(p) > plane + 30.0 } else { coord(p) < plane - 30.0 };
            assert!(before(a), "case {i}: the old slide stops in front of the plane ({a:?})");
            assert!(past(b), "case {i}: the walking filter lets Sora through ({b:?})");
        }
        // Real walls keep stopping: Station Heights x = 313 (0x172, normal -x) walking +x at z = 374 on the
        // lower level; Tram Common z = -1700 (0x1f2, normal +z) walking -z at x = 3637, 2037 and 4283.
        let real: [(&World, &World, [f32; 2], f32, Vec3); 4] = [
            (&heights, &heights_old, [227.0, 374.0], -550.0, Vec3::X),
            (&tram, &tram_old, [3637.0, -1580.0], 0.0, Vec3::NEG_Z),
            (&tram, &tram_old, [2037.0, -1580.0], 0.0, Vec3::NEG_Z),
            (&tram, &tram_old, [4283.0, -1580.0], 0.0, Vec3::NEG_Z),
        ];
        for (i, (new, old, start, y, dir)) in real.into_iter().enumerate() {
            let (a, b) = (walk(old, start, y, dir), walk(new, start, y, dir));
            let travelled = |p: Vec3| (p - Vec3::new(start[0], p.y, start[1])).dot(dir);
            assert!(travelled(b) < 150.0 && (travelled(a) - travelled(b)).abs() < 1.0, "real wall {i}: stops Sora as before ({a:?} -> {b:?})");
        }
    }
}

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
