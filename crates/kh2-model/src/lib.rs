//! KH2 models → plain triangle meshes. No engine types here; `kh2-app` turns these into Bevy assets.
//!
//! * Map models (BAR type 4 named `MAP`/`SK0`/`BOB`, model type 2): DMA chains of VIF packets.
//! * Skeletal models (MDLX type 4, model type 3): bones + DMA chains whose packets carry
//!   bone-local vertices, assigned to bones by matrix slot and blended by weight groups.
//!
//! Layouts per OpenKH `docs/kh2/file/model.md`, `docs/kh2/file/type/mdlx.md`,
//! `OpenKh.Kh2/Mdlx.Model.cs` and `OpenKh.Engine/Parsers/*` (Apache-2.0); see CREDITS.md.

pub mod collision;
pub mod texanim;
pub mod texture;

use glam::{Mat4, Quat, Vec3, Vec4};
use kh2_ps2::{dma, vif::Vif, vpu::VpuPacket};

pub use texture::{Texture, Wrap};

/// Every model block starts with a 0x90-byte area the game uses at runtime; offsets are relative to its end.
pub const RESERVED: usize = 0x90;

#[derive(Debug, Clone, Default)]
pub struct Mesh {
    pub positions: Vec<[f32; 3]>,
    pub uvs: Vec<[f32; 2]>,
    /// PS2 vertex colour scaled so 1.0 = neutral (0x80).
    pub colors: Vec<[f32; 4]>,
    pub indices: Vec<u32>,
    pub texture: usize,
    pub transparent: bool,
    /// Skinning (empty for maps): up to 4 bones per vertex and their weights.
    pub joints: Vec<[u16; 4]>,
    pub weights: Vec<[f32; 4]>,
    /// UV-scroll entry the mesh scrolls by: 0 none, k the texture footer's `UVSC` entry with
    /// index k - 1. Map meshes: bits 1..5 of the mesh entry's +0xc word (FUN_0011f560 reads
    /// `(+0x2c >> 1) & 0x1f`); skeletal meshes: bits 22..26 of the mesh header's flags (FUN_00121a78).
    pub uv_scroll: u8,
}

#[derive(Debug, Clone, Copy)]
pub struct Bone {
    pub parent: i32,
    pub scale: Vec3,
    pub rotation: Vec3,
    pub translation: Vec3,
    /// Segment length used by IK chains (the scale vector's W).
    pub length: f32,
}

impl Bone {
    /// Euler XYZ (radians) applied X, then Y, then Z.
    pub fn local_rotation(&self) -> Quat {
        Quat::from_rotation_z(self.rotation.z) * Quat::from_rotation_y(self.rotation.y) * Quat::from_rotation_x(self.rotation.x)
    }
}

#[derive(Debug, Clone, Default)]
pub struct Model {
    pub meshes: Vec<Mesh>,
    pub bones: Vec<Bone>,
    pub textures: Vec<Texture>,
    /// Object collision shapes (MDLX only).
    pub shapes: Vec<collision::Shape>,
    /// Texture animations (sprite regions and UV scrolls) from the texture container's footer.
    pub anims: texanim::Animations,
}

fn u32_at(b: &[u8], o: usize) -> u32 {
    b.get(o..o + 4).map(|s| u32::from_le_bytes(s.try_into().unwrap())).unwrap_or(0)
}

fn f32_at(b: &[u8], o: usize) -> f32 {
    f32::from_bits(u32_at(b, o))
}

/// Builds a map model (type 2) from its model block (BAR type 4 data, including the reserved area).
pub fn map_model(block: &[u8]) -> Vec<Mesh> {
    let Some(b) = block.get(RESERVED..) else { return Vec::new() };
    if u32_at(b, 0) != 2 {
        return Vec::new();
    }
    let chains = u32_at(b, 0x10) as usize;
    let mut meshes = Vec::with_capacity(chains);
    for c in 0..chains.min(10_000) {
        let e = 0x20 + 16 * c;
        let (start, texture) = (u32_at(b, e) as usize, u32_at(b, e + 4) as usize);
        let transparent = b.get(e + 0x0A..e + 0x0C).is_some_and(|f| f != [0, 0]);
        let uv_scroll = (u32_at(b, e + 0x0C) >> 1 & 0x1f) as u8;
        let code = dma::chain_to_vif(b, start);
        let mut mesh = Mesh { texture, transparent, uv_scroll, ..Default::default() };
        Vif::default().run(&code, |mem| append_packet(&mut mesh, &VpuPacket::read(mem), |p, i| p.vertices.get(i).map(|v| [v[0], v[1], v[2]])));
        if !mesh.indices.is_empty() {
            meshes.push(mesh);
        }
    }
    meshes
}

/// Appends one VPU packet's strip as triangles; `pos` resolves a strip node's vertex index.
fn append_packet(mesh: &mut Mesh, p: &VpuPacket, pos: impl Fn(&VpuPacket, usize) -> Option<[f32; 3]>) {
    let base = mesh.positions.len() as u32;
    for (i, n) in p.strip.iter().enumerate() {
        mesh.positions.push(pos(p, n.index).unwrap_or_default());
        mesh.uvs.push([n.u as f32 / 4096.0, n.v as f32 / 4096.0]);
        let c = p.colors.get(i).copied().unwrap_or([128; 4]);
        mesh.colors.push(c.map(|x| x as f32 / 128.0));
    }
    for t in p.triangles() {
        mesh.indices.extend(t.map(|k| base + k as u32));
    }
}

/// Parses a skeletal MDLX (the whole BAR file). Vertices are posed in the model's rest pose.
pub fn mdlx(file: &[u8]) -> Option<Model> {
    let entries = kh2_bar::parse(file).ok()?;
    let block = entries.iter().find(|e| e.kind == 4)?.data;
    let tex = entries.iter().find(|e| e.kind == 7);
    let textures = tex.map(|e| texture::decode(e.data)).unwrap_or_default();
    let anims = tex.map(|e| texanim::parse(e.data)).unwrap_or_default();
    let b = block.get(RESERVED..)?;
    if u32_at(b, 0) != 3 {
        return None;
    }
    let bone_count = u32_at(b, 0x10) as u16 as usize;
    let bone_off = u32_at(b, 0x14) as usize;
    let chains = u32_at(b, 0x1C) as usize;
    let bones: Vec<Bone> = (0..bone_count)
        .map(|i| {
            let o = bone_off + 64 * i;
            let v = |k: usize| Vec3::new(f32_at(b, o + k), f32_at(b, o + k + 4), f32_at(b, o + k + 8));
            Bone { parent: u32_at(b, o + 4) as i32, scale: v(0x10), rotation: v(0x20), translation: v(0x30), length: f32_at(b, o + 0x1C) }
        })
        .collect();
    let rest = rest_pose(&bones);

    let mut meshes = Vec::new();
    for c in 0..chains.min(1000) {
        let h = 0x20 + 0x20 * c;
        let (flags, texture) = (u32_at(b, h), u32_at(b, h + 4) as usize);
        let (dma_off, slots_off) = (u32_at(b, h + 0x10) as usize, u32_at(b, h + 0x14) as usize);
        // Matrix-slot list: bone indices per packet, packets separated by -1.
        let n = u32_at(b, slots_off) as usize;
        let mut packets = vec![dma_off];
        let mut slots: Vec<Vec<usize>> = vec![Vec::new()];
        let mut cursor = dma_off + 0x10;
        for k in 0..n.min(100_000) {
            let v = u32_at(b, slots_off + 4 + 4 * k) as i32;
            if v == -1 {
                packets.push(cursor + 0x10);
                cursor += 0x20;
                slots.push(Vec::new());
            } else {
                cursor += 0x10;
                slots.last_mut().unwrap().push(v as usize);
            }
        }
        let mut mesh = Mesh { texture, transparent: flags & 1 != 0, uv_scroll: (flags >> 22 & 0x1f) as u8, ..Default::default() };
        for (pk, slot) in packets.iter().zip(&slots) {
            let tag = dma::tag_at(b, *pk);
            let code = b.get(tag.addr..tag.addr + tag.qwc * 16).unwrap_or(&[]);
            let mut vif = Vif::default();
            let mut mem = None;
            vif.run(code, |m| {
                if mem.is_none() {
                    mem = Some(m.to_vec())
                }
            });
            let p = VpuPacket::read(mem.as_deref().unwrap_or(&vif.mem));
            append_skinned(&mut mesh, &p, slot, &rest);
        }
        if !mesh.indices.is_empty() {
            meshes.push(mesh);
        }
    }
    let shapes = entries.iter().find(|e| e.kind == 23).map(|e| collision::parse(e.data)).unwrap_or_default();
    Some(Model { meshes, bones, textures, shapes, anims })
}

/// Absolute rest-pose matrices (rotation then translation; scale is not applied by the game here).
pub fn rest_pose(bones: &[Bone]) -> Vec<Mat4> {
    let mut rot = vec![Quat::IDENTITY; bones.len()];
    let mut pos = vec![Vec3::ZERO; bones.len()];
    for (i, bone) in bones.iter().enumerate() {
        let (pr, pt) = match usize::try_from(bone.parent).ok().filter(|&p| p < i) {
            Some(p) => (rot[p], pos[p]),
            None => (Quat::IDENTITY, Vec3::ZERO),
        };
        pos[i] = pt + pr * bone.translation;
        rot[i] = pr * bone.local_rotation();
    }
    rot.iter().zip(&pos).map(|(r, t)| Mat4::from_rotation_translation(*r, *t)).collect()
}

/// One skeletal packet: resolve each final vertex from (bone, bone-local vertex) pairs.
fn append_skinned(mesh: &mut Mesh, p: &VpuPacket, slots: &[usize], rest: &[Mat4]) {
    // Flat list: every packet vertex with the bone of the matrix slot it falls in.
    let mut flat = Vec::with_capacity(p.vertices.len());
    for (s, &count) in p.vertex_range.iter().enumerate() {
        let bone = slots.get(s).copied().unwrap_or(0);
        for _ in 0..count {
            flat.push((bone, flat.len()));
        }
    }
    let groups: Vec<Vec<(usize, usize)>> = if p.weight_groups.is_empty() {
        flat.iter().map(|&f| vec![f]).collect()
    } else {
        p.weight_groups.iter().flatten().map(|g| g.iter().filter_map(|&i| flat.get(i).copied()).collect()).collect()
    };
    let resolved: Vec<([f32; 3], [u16; 4], [f32; 4])> = groups
        .iter()
        .map(|g| {
            let mut out = Vec3::ZERO;
            let (mut j, mut w) = ([0u16; 4], [0f32; 4]);
            for (k, &(bone, vi)) in g.iter().enumerate() {
                let v = p.vertices.get(vi).copied().unwrap_or([0.0; 4]);
                let m = rest.get(bone).copied().unwrap_or(Mat4::IDENTITY);
                let weight = if g.len() == 1 { 1.0 } else { v[3] };
                out += if g.len() == 1 {
                    m.transform_point3(Vec3::new(v[0], v[1], v[2]))
                } else {
                    (m * Vec4::new(v[0], v[1], v[2], v[3])).truncate()
                };
                if k < 4 {
                    j[k] = bone as u16;
                    w[k] = weight;
                }
            }
            (out.to_array(), j, w)
        })
        .collect();
    let base = mesh.positions.len() as u32;
    for n in &p.strip {
        let (pos, j, w) = resolved.get(n.index).copied().unwrap_or(([0.0; 3], [0; 4], [1.0, 0.0, 0.0, 0.0]));
        mesh.positions.push(pos);
        mesh.uvs.push([n.u as f32 / 4096.0, n.v as f32 / 4096.0]);
        mesh.colors.push([1.0; 4]);
        mesh.joints.push(j);
        mesh.weights.push(w);
    }
    for t in p.triangles() {
        mesh.indices.extend(t.map(|k| base + k as u32));
    }
}

/// A map file (`.map`, a BAR): the `MAP` model + textures, plus optional sky (`SK0`).
pub struct MapFile {
    pub map: Model,
    pub sky: Option<Model>,
}

pub fn map_file(file: &[u8]) -> Option<MapFile> {
    let e = kh2_bar::parse(file).ok()?;
    let pair = |tag: &str| -> Option<Model> {
        let m = e.iter().find(|x| x.tag == tag && x.kind == 4)?;
        let t = e.iter().find(|x| x.tag == tag && x.kind == 7);
        Some(Model {
            meshes: map_model(m.data),
            textures: t.map(|t| texture::decode(t.data)).unwrap_or_default(),
            anims: t.map(|t| texanim::parse(t.data)).unwrap_or_default(),
            ..Default::default()
        })
    };
    Some(MapFile { map: pair("MAP")?, sky: pair("SK0") })
}
