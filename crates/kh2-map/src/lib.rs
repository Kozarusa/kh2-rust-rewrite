//! COCT collision (map BAR type 6). Layout per OpenKH `docs/kh2/file/type/coct.md` (Apache-2.0).
//!
//! A bounding-volume tree (mesh groups → meshes) over polygons: each polygon is a triangle or a
//! quad (triangle fan), with a precomputed plane and a surface-flag word. Coordinates are stored
//! in the game's world space, which is map-model space with Y and Z negated (the same flip the
//! game applies to motion roots and object matrices); [`Coct::parse`] returns map-model space.
//! Checked with `kh2 coct-check`: COCT floors coincide with the render mesh only this way.

pub mod build;
pub mod geom;
pub mod mapcoll;
pub mod mapobj;
pub mod oct;
pub mod pool;
pub mod sphere;

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
    /// Attribute bit 0x100000: the sphere solver FUN_001632b8 skips the polygon altogether (it tests
    /// `attribute & 0x100000` before any plane test, in all three of its passes).
    pub const SOLVER_SKIP: u32 = 1 << 20;
}

/// Does a polygon with attribute word `attr` stop the walking party member (collision kind 0)?
///
/// The player's collider (FUN_00143840 -> FUN_00164138) sweeps with `FUN_00141b50`, which keeps a
/// polygon when its attribute nibble is not 4 (`FUN_00141b30(.., 0xf, 4, 1, ..)`: mask 0xf, value 4,
/// inverted) and the handler of the collider's kind accepts it. Party members have kind 0 (their
/// constructors clear bits 3..6 of +0x600, FUN_001746e0 / FUN_00174800), and handler 0 of the table at
/// 0x378f7c (0x32e438) tests `attribute & 0x10`, PARTY_COLLIDE. The solver (FUN_001632b8) then skips
/// polygons with attribute bit 0x100000.
pub fn stops_party(attr: u32) -> bool {
    attr & flags::PARTY_COLLIDE != 0 && attr & 0xf != 4 && attr & flags::SOLVER_SKIP == 0
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
    /// Ground material: byte 0 of the polygon record. The game copies it into the actor's ground
    /// contact data (FUN_00143560 stores `*(u8 *)polygon` at actor +0x648) and the footstep sound
    /// and dust code index by it (FUN_00183730, FUN_0016baa0, FUN_001b1c90). 0 plays no footstep.
    pub material: u8,
    /// Index of the mesh (table 2 entry) the polygon belongs to.
    pub mesh: u16,
}

impl Polygon {
    pub fn triangles(&self) -> impl Iterator<Item = [[f32; 3]; 3]> + '_ {
        let v = &self.verts;
        std::iter::once([v[0], v[1], v[2]]).chain(self.quad.then_some([v[0], v[2], v[3]]))
    }
}

/// One entry of the mesh table (table 1 of the header, 20 bytes). Earlier versions of the parser
/// dropped everything after the polygon range (WP-IW1). The game's walker `FUN_00141b50` skips a mesh
/// whose byte +0x10 is 0; `FUN_00141768` (called by `FUN_00141500` with the map's visibility word)
/// clears that byte in every mesh and then sets it in the meshes whose byte +0x11 is a set bit of the
/// word (`FUN_00141840`). So a mesh is solid only while its group bit is in the visibility word
/// (docs/systems/core-runtime/collision.md section 8).
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Mesh {
    /// First polygon index (+12).
    pub first: u16,
    /// One past the last polygon index (+14).
    pub last: u16,
    /// Byte +0x10 as stored on the disc; the game overwrites it when the collision is built.
    pub stored_enabled: u8,
    /// Byte +0x11: the mesh group, a bit number of the 64-bit visibility word.
    pub group: u8,
    /// Halfword +0x12 (not read by the walker; meaning unknown).
    pub unknown: u16,
}

impl Mesh {
    /// Whether the game's walker visits this mesh when the map's visibility word is `word`
    /// (`FUN_00141768`: bit `group` of the word; groups of 64 or more are never enabled).
    pub fn enabled(&self, word: u64) -> bool {
        self.group < 64 && word >> self.group & 1 != 0
    }
}

/// One node of the bounding-volume tree (table 0 of the header, 0x20 bytes, `FUN_00141b50`): up to 8
/// child node indices from +0 (ended by -1), the bounding box at +0x10, the range of meshes at
/// +0x1c and +0x1e.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Node {
    pub first_mesh: u16,
    pub last_mesh: u16,
}

#[derive(Debug, Clone, Default)]
pub struct Coct {
    pub polygons: Vec<Polygon>,
    /// The mesh table; `Polygon::mesh` indexes it.
    pub meshes: Vec<Mesh>,
    /// The tree nodes (table 0).
    pub nodes: Vec<Node>,
    /// The file image the polygons were read from: `oct::Oct` runs the game's own queries over it in place
    /// (empty for a `Coct` built by hand).
    pub raw: Vec<u8>,
}

impl Coct {
    /// The mesh group of the mesh a polygon belongs to (0 when the mesh is unknown).
    pub fn group_of(&self, p: &Polygon) -> u8 {
        self.meshes.get(p.mesh as usize).map_or(0, |m| m.group)
    }
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
        let (t1, l1) = table(0)?;
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
        let meshes = (0..l2 / 20)
            .map(|m| {
                let o = t2 + 20 * m;
                Some(Mesh { first: i16_at(b, o + 12)? as u16, last: i16_at(b, o + 14)? as u16, stored_enabled: *b.get(o + 16)?, group: *b.get(o + 17)?, unknown: i16_at(b, o + 18)? as u16 })
            })
            .collect::<Option<Vec<_>>>()?;
        let nodes = (0..l1 / 32)
            .map(|g| {
                let o = t1 + 32 * g;
                Some(Node { first_mesh: i16_at(b, o + 0x1c)? as u16, last_mesh: i16_at(b, o + 0x1e)? as u16 })
            })
            .collect::<Option<Vec<_>>>()?;
        let mut polygons = Vec::with_capacity(l3 / 16);
        for (i, &mesh) in mesh_of.iter().enumerate() {
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
            let material = *b.get(o)?;
            polygons.push(Polygon { verts, quad, normal, d, flags, material, mesh });
        }
        Some(Coct { polygons, meshes, nodes, raw: b.to_vec() })
    }

    /// The map's collision: the first BAR entry of type 6 (COCT) in a `.map` file.
    pub fn from_map(file: &[u8]) -> Option<Coct> {
        let e = kh2_bar::parse(file).ok()?;
        e.iter().filter(|x| x.kind == 6).find_map(|x| Coct::parse(x.data))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The walking filter: PARTY_COLLIDE, nibble not 4, bit 0x100000 clear (attributes seen on the disc).
    #[test]
    fn walking_filter_keeps_party_collide_only() {
        for attr in [0x172u32, 0x1f2, 0x1172, 0x3f2, 0x3f1, 0x972, 0x803f1, 0x7f1] {
            assert!(stops_party(attr), "{attr:#x} is a real wall");
        }
        // Attack-only (0x82), object-only (0x22, 0x62, 0x80002), nibble 4 (0x284 has no party bit either).
        for attr in [0x82u32, 0x22, 0x62, 0x80002, 0x284] {
            assert!(!stops_party(attr), "{attr:#x} lets the party through");
        }
        assert!(!stops_party(0x174), "attribute nibble 4 is filtered by FUN_00141b50");
        assert!(!stops_party(0x172 | flags::SOLVER_SKIP), "bit 0x100000 is skipped by FUN_001632b8");
    }
}
