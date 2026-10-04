//! DPD models: a 0x10-byte header (number, RSD number, texture/CLUT base, packet size) and an
//! OMD body: header (flags, primitive/vertex/normal offsets, counts, GS ALPHA value), primitive
//! blocks (0x10-byte header with type, count and TEX0; then the primitives), int16 xyz vertices.
//!
//! Layout per OpenKH `OpenKh.Kh2/DpdModel.cs` (Apache-2.0); primitive sizes and the quad layout
//! (4 colors, 4 vertex indices, 4 UVs in 1/4096) were read from the game's weapon-trail code.

use crate::rd::*;

#[derive(Clone, Debug)]
pub struct Model {
    pub number: i16,
    pub flags: u32,
    pub prim_count: usize,
    pub vertex_count: usize,
    /// GS ALPHA register value (0x48: Cs·As + Cd, additive; 0x44: blend).
    pub alpha: u8,
    pub alpha_fix: u8,
    pub blocks: Vec<PrimBlock>,
    pub vertices: Vec<[i16; 3]>,
}

#[derive(Clone, Debug)]
pub struct PrimBlock {
    pub kind: u8,
    pub tex0: u64,
    pub prims: Vec<Prim>,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Prim {
    pub colors: [[u8; 4]; 4],
    pub indices: [u16; 4],
    pub uvs: [[u16; 2]; 4],
}

/// Primitive size in bytes per OMD primitive type.
pub fn prim_size(kind: u8) -> usize {
    match kind {
        0 => 0x14,
        1 => 0x1c,
        4 => 0x18,
        6 => 0x28,
        _ => 0x18,
    }
}

impl Model {
    pub fn parse(b: &[u8]) -> Model {
        let omd = 0x10;
        let p_off = omd + i32_at(b, omd + 4).max(0) as usize;
        let v_off = omd + i32_at(b, omd + 8).max(0) as usize;
        let vertex_count = i16_at(b, omd + 0x12).max(0) as usize;
        let mut blocks = Vec::new();
        let mut o = p_off;
        while o + 0x10 <= b.len() && u8_at(b, o + 1) != 0xff {
            let kind = u8_at(b, o + 1);
            let n = i16_at(b, o + 2).max(0) as usize;
            let tex0 = u64_at(b, o + 8);
            let size = prim_size(kind);
            let prims = (0..n)
                .map(|k| {
                    let q = o + 0x10 + k * size;
                    let mut p = Prim::default();
                    for c in 0..4 {
                        for ch in 0..4 {
                            p.colors[c][ch] = u8_at(b, q + c * 4 + ch);
                        }
                    }
                    if kind == 4 || kind == 6 {
                        for c in 0..4 {
                            p.indices[c] = u16_at(b, q + 0x10 + 2 * c);
                        }
                    }
                    if kind == 6 {
                        for c in 0..4 {
                            p.uvs[c] = [u16_at(b, q + 0x18 + 4 * c), u16_at(b, q + 0x1a + 4 * c)];
                        }
                    }
                    p
                })
                .collect();
            blocks.push(PrimBlock { kind, tex0, prims });
            o += 0x10 + size * n;
        }
        Model {
            number: i16_at(b, 0),
            flags: u32_at(b, omd),
            prim_count: i16_at(b, omd + 0x10).max(0) as usize,
            vertex_count,
            alpha: u8_at(b, omd + 0x16),
            alpha_fix: u8_at(b, omd + 0x17),
            blocks,
            vertices: (0..vertex_count)
                .map(|k| [i16_at(b, v_off + 6 * k), i16_at(b, v_off + 6 * k + 2), i16_at(b, v_off + 6 * k + 4)])
                .collect(),
        }
    }
}
