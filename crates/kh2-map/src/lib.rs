//! COCT collision (map BAR type 6). Layout per OpenKH `docs/kh2/file/type/coct.md` (Apache-2.0).
//!
//! A bounding-volume tree (mesh groups → meshes) over polygons: each polygon is a triangle or a
//! quad (triangle fan), with a precomputed plane and a surface-flag word. Coordinates are stored
//! in the game's world space, which is map-model space with Y and Z negated (the same flip the
//! game applies to motion roots and object matrices); [`Coct::parse`] returns map-model space.
//! Checked with `kh2 coct-check`: COCT floors coincide with the render mesh only this way.

pub mod flags {
    pub const PARTY_STAND: u32 = 1 << 0;
    pub const ENTITY_FALL_OVERRIDE: u32 = 1 << 1;
    pub const PLAYER_FALL: u32 = 1 << 2;
    pub const FALL2: u32 = 1 << 3;
    pub const PARTY_COLLIDE: u32 = 1 << 4;
    pub const OBJECT_COLLIDE: u32 = 1 << 5;
    pub const ATTACK_COLLIDE: u32 = 1 << 7;
    pub const LEDGE_GRAB: u32 = 1 << 10;
    pub const BARRIER: u32 = 1 << 11;
}

#[derive(Debug, Clone, Copy)]
pub struct Polygon {
    /// 3 or 4 vertices (quads are fans: 0-1-2, 0-2-3).
    pub verts: [[f32; 3]; 4],
    pub quad: bool,
    /// Plane normal (unit) and distance, in map space: n·p + d = 0 on the surface.
    pub normal: [f32; 3],
    pub d: f32,
    pub flags: u32,
    /// Index of the mesh (table 2 entry) the polygon belongs to.
    pub mesh: u16,
}

impl Polygon {
    pub fn triangles(&self) -> impl Iterator<Item = [[f32; 3]; 3]> + '_ {
        let v = &self.verts;
        std::iter::once([v[0], v[1], v[2]]).chain(self.quad.then_some([v[0], v[2], v[3]]))
    }
}

#[derive(Debug, Clone, Default)]
pub struct Coct {
    pub polygons: Vec<Polygon>,
}

fn rd<const N: usize>(b: &[u8], o: usize) -> Option<[u8; N]> {
    b.get(o..o + N)?.try_into().ok()
}
fn u32_at(b: &[u8], o: usize) -> Option<u32> {
    rd::<4>(b, o).map(u32::from_le_bytes)
}
fn i16_at(b: &[u8], o: usize) -> Option<i16> {
    rd::<2>(b, o).map(i16::from_le_bytes)
}
fn f32_at(b: &[u8], o: usize) -> Option<f32> {
    rd::<4>(b, o).map(f32::from_le_bytes)
}

impl Coct {
    pub fn parse(b: &[u8]) -> Option<Coct> {
        if b.get(0..4)? != b"COCT" {
            return None;
        }
        let table = |k: usize| -> Option<(usize, usize)> { Some((u32_at(b, 24 + 8 * k)? as usize, u32_at(b, 28 + 8 * k)? as usize)) };
        let (_, _) = table(0)?;
        let (t2, l2) = table(1)?;
        let (t3, l3) = table(2)?;
        let (t4, _) = table(3)?;
        let (t5, _) = table(4)?;
        let (t7, _) = table(6)?;
        let vertex = |i: i16| -> Option<[f32; 3]> {
            let o = t4 + 16 * i as usize;
            Some([f32_at(b, o)?, -f32_at(b, o + 4)?, -f32_at(b, o + 8)?])
        };
        // Mesh of each polygon, from table 2's index ranges.
        let mut mesh_of = vec![0u16; l3 / 16];
        for m in 0..l2 / 20 {
            let o = t2 + 20 * m;
            let (first, last) = (i16_at(b, o + 12)? as u16 as usize, i16_at(b, o + 14)? as u16 as usize);
            for p in first..last.min(mesh_of.len()) {
                mesh_of[p] = m as u16;
            }
        }
        let mut polygons = Vec::with_capacity(l3 / 16);
        for i in 0..l3 / 16 {
            let o = t3 + 16 * i;
            let idx = [i16_at(b, o + 2)?, i16_at(b, o + 4)?, i16_at(b, o + 6)?, i16_at(b, o + 8)?];
            let quad = idx[3] >= 0;
            let mut verts = [[0.0; 3]; 4];
            for k in 0..if quad { 4 } else { 3 } {
                verts[k] = vertex(idx[k])?;
            }
            let po = t5 + 16 * i16_at(b, o + 10)? as u16 as usize;
            let normal = [f32_at(b, po)?, -f32_at(b, po + 4)?, -f32_at(b, po + 8)?];
            // Stored plane N·V + D = 0 with V = (x, -y, -z): the same flip applies to N, D stays.
            let d = f32_at(b, po + 12)?;
            let flags = u32_at(b, t7 + 4 * i16_at(b, o + 14)? as u16 as usize)?;
            polygons.push(Polygon { verts, quad, normal, d, flags, mesh: mesh_of[i] });
        }
        Some(Coct { polygons })
    }

    /// The map's collision: the first BAR entry of type 6 (COCT) in a `.map` file.
    pub fn from_map(file: &[u8]) -> Option<Coct> {
        let e = kh2_bar::parse(file).ok()?;
        e.iter().filter(|x| x.kind == 6).find_map(|x| Coct::parse(x.data))
    }
}
