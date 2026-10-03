//! The VU1 memory image KH2's model microprograms read (layout per OpenKH `VpuPacket`).
//!
//! Header (4 qwords of i32): type, color-ptr-inc, magic, vertex-buffer-ptr | strip count, strip
//! offset, matrix-count offset, matrix offset | color count, color offset, weight-group count,
//! weight-group-count offset | vertex count, vertex offset, vertex-index offset, matrix count.
//! Offsets are in qwords from the start of VU1 memory.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StripOp {
    /// Push the vertex, draw nothing.
    Stock,
    /// Draw a triangle with the two previous vertices.
    Draw,
    /// Draw with reversed winding.
    DrawInverse,
    /// Draw both windings.
    DrawBoth,
}

#[derive(Debug, Clone, Copy)]
pub struct StripNode {
    pub u: i32,
    pub v: i32,
    pub index: usize,
    pub op: StripOp,
}

#[derive(Debug, Clone, Default)]
pub struct VpuPacket {
    pub strip: Vec<StripNode>,
    /// Per strip node, 0..=255 where 128 = 1.0 (PS2 convention).
    pub colors: Vec<[u8; 4]>,
    /// xyz + w (w = blend weight for multi-bone vertices).
    pub vertices: Vec<[f32; 4]>,
    /// How many consecutive vertices belong to each matrix slot.
    pub vertex_range: Vec<usize>,
    /// Weighted groups: group size (1-based amount) -> list of groups of flat vertex indices.
    pub weight_groups: Vec<Vec<Vec<usize>>>,
}

impl VpuPacket {
    pub fn read(mem: &[[u32; 4]]) -> VpuPacket {
        let h = |q: usize, l: usize| mem.get(q).map(|x| x[l] as i32).unwrap_or(0);
        let (strip_n, strip_o) = (h(1, 0).max(0) as usize, h(1, 1).max(0) as usize);
        let (mcount_o, color_n, color_o) = (h(1, 2).max(0) as usize, h(2, 0).max(0) as usize, h(2, 1).max(0) as usize);
        let (wg_n, wg_o) = (h(2, 2).max(0) as usize, h(2, 3).max(0) as usize);
        let (vert_n, vert_o, matrix_n) = (h(3, 0).max(0) as usize, h(3, 1).max(0) as usize, h(3, 3).max(0) as usize);
        let q = |i: usize| mem.get(i).copied().unwrap_or([0; 4]);

        let strip = (0..strip_n)
            .map(|i| {
                let x = q(strip_o + i);
                StripNode {
                    u: x[0] as u16 as i16 as i32,
                    v: x[1] as u16 as i16 as i32,
                    index: x[2] as usize & 0xFF,
                    op: match x[3] & 0x30 {
                        0x10 => StripOp::Stock,
                        0x20 => StripOp::Draw,
                        0x30 => StripOp::DrawInverse,
                        _ => StripOp::DrawBoth,
                    },
                }
            })
            .collect();
        let colors = (0..color_n).map(|i| q(color_o + i).map(|c| c.min(255) as u8)).collect();
        let vertices = (0..vert_n).map(|i| q(vert_o + i).map(f32::from_bits)).collect();
        // Matrix counts are packed one i32 per lane.
        let lane = |base: usize, i: usize| q(base + i / 4)[i % 4] as usize;
        let vertex_range = (0..matrix_n).map(|i| lane(mcount_o, i)).collect();
        let mut weight_groups = Vec::new();
        if wg_n > 0 {
            let counts: Vec<usize> = (0..wg_n).map(|i| lane(wg_o, i)).collect();
            // Groups follow the count list, each size class aligned to a qword.
            let mut word = wg_o * 4 + wg_n;
            for (amount, &count) in counts.iter().enumerate() {
                word = word.div_ceil(4) * 4;
                let mut groups = Vec::with_capacity(count);
                for _ in 0..count {
                    let g: Vec<usize> = (0..amount + 1).map(|k| q((word + k) / 4)[(word + k) % 4] as usize).collect();
                    word += amount + 1;
                    groups.push(g);
                }
                weight_groups.push(groups);
            }
        }
        VpuPacket { strip, colors, vertices, vertex_range, weight_groups }
    }

    /// Triangles over strip nodes (indices into `strip`), using a 4-entry ring like the VU code.
    pub fn triangles(&self) -> Vec<[usize; 3]> {
        let mut tris = Vec::new();
        let mut ring = [0usize; 4];
        for (i, n) in self.strip.iter().enumerate() {
            ring[i & 3] = i;
            if i < 2 {
                continue;
            }
            let (a, b, c) = (ring[(i + 4 - 2) & 3], ring[(i + 4 - 1) & 3], ring[i & 3]);
            match n.op {
                StripOp::Stock => {}
                StripOp::Draw => tris.push([c, a, b]),
                StripOp::DrawInverse => tris.push([c, b, a]),
                StripOp::DrawBoth => {
                    tris.push([c, a, b]);
                    tris.push([c, b, a]);
                }
            }
        }
        tris
    }
}
