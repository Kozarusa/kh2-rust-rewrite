//! Model textures (BAR type 7). Layout per OpenKH `docs/kh2/file/raw-texture.md` and
//! `OpenKh.Kh2/ModelTexture.cs` (Apache-2.0).
//!
//! Header (i32): magic 0, color count, texture-transfer count, GS-info count, offset-data offset,
//! transfer offset (CLUT transfer first), GS-info offset, picture offset, palette offset.

use kh2_ps2::gs;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Wrap {
    Repeat,
    Clamp,
}

/// The part of a texture the GS samples along one axis, and how coordinates outside it wrap.
/// From the GS-info's CLAMP register (WMS/WMT with MINU/MAXU, MINV/MAXV):
/// * REPEAT (0) and CLAMP (1): the whole axis, repeated or clamped.
/// * REGION_CLAMP (2): texels `MIN..=MAX`, coordinates clamped into them.
/// * REGION_REPEAT (3): texel `(t & MSK) | FIX` (MSK in the MIN field, FIX in the MAX field):
///   with `MSK + 1` a power of two and FIX outside the mask bits, that is the run of `MSK + 1`
///   texels from FIX, repeated. Other masks (none in the files checked) are kept as plain REPEAT.
///
/// Texture atlases use these modes to pack several images (each with its own CLUT) into one GS
/// page; sampling the whole page instead shows the neighbours in the wrong palette (docs/render.md).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Axis {
    pub start: u32,
    pub len: u32,
    pub wrap: Wrap,
}

impl Axis {
    pub fn from_clamp(mode: usize, min: u32, max: u32, size: u32) -> Axis {
        let whole = |wrap| Axis { start: 0, len: size, wrap };
        match mode {
            1 => whole(Wrap::Clamp),
            2 => {
                let (a, b) = (min.min(size - 1), max.min(size - 1));
                if b < a { whole(Wrap::Clamp) } else { Axis { start: a, len: b - a + 1, wrap: Wrap::Clamp } }
            }
            3 => {
                let len = min + 1;
                if len.is_power_of_two() && max & min == 0 && max + len <= size {
                    Axis { start: max, len, wrap: Wrap::Repeat }
                } else {
                    whole(Wrap::Repeat)
                }
            }
            _ => whole(Wrap::Repeat),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Texture {
    pub width: u32,
    pub height: u32,
    /// RGBA8, row-major, straight alpha.
    pub rgba: Vec<u8>,
    pub wrap_u: Wrap,
    pub wrap_v: Wrap,
    /// The sampled region per axis (U, V); see [`Axis`].
    pub region: [Axis; 2],
    /// UV-scroll spans `[MINU + 1, MINV + 1]` from the GS-info's CLAMP register, present only when
    /// its horizontal wrap mode (WMS) is REPEAT or REGION_REPEAT: the draw code applies a UV scroll
    /// only then (FUN_0011f560 maps, FUN_00121a78 models; both test `CLAMP & 3` alone).
    pub uv_span: Option<[u32; 2]>,
}

const TRANSFER: usize = 0x90;
const GSINFO: usize = 0xA0;

/// The GS registers of one GS-info entry that the texture and its animations need.
#[derive(Debug, Clone, Copy)]
pub(crate) struct GsInfo {
    pub psm: usize,
    pub width: usize,
    pub height: usize,
    /// TBP0 and TBW of TEX0, and the DBP of the image transfer the entry uses.
    pub tbp0: usize,
    pub tbw: usize,
    pub dbp: usize,
    cbp: usize,
    csa: usize,
    upload_psm: usize,
    upload: (usize, usize),
    data_off: usize,
    clamp: u64,
}

pub(crate) fn gs_info(b: &[u8], i: usize) -> Option<GsInfo> {
    if b.len() < 36 || i32_at(b, 0) != 0 || i >= i32_at(b, 12) {
        return None;
    }
    let n_tex = i32_at(b, 8);
    let (off_data, off_tr, off_gs) = (i32_at(b, 16), i32_at(b, 20), i32_at(b, 24));
    let clut_dbp = bits(u64_at(b, off_tr + 0x20), 32, 14);
    let ti = *b.get(off_data + i).unwrap_or(&0) as usize;
    let tr = off_tr + TRANSFER * (1 + ti.min(n_tex.saturating_sub(1)));
    let g = off_gs + GSINFO * i;
    // The entry must lie inside the container, and the GS limits a texture to 1024 x 1024 (TEX0 TW/TH <= 10).
    b.get(g..g + GSINFO)?;
    let tex0 = u64_at(b, g + 0x70);
    let (tw, th) = (bits(tex0, 26, 4), bits(tex0, 30, 4));
    if tw > 10 || th > 10 {
        return None;
    }
    Some(GsInfo {
        psm: bits(tex0, 20, 6),
        width: 1 << tw,
        height: 1 << th,
        tbp0: bits(tex0, 0, 14),
        tbw: bits(tex0, 14, 6),
        dbp: bits(u64_at(b, tr + 0x20), 32, 14),
        cbp: bits(tex0, 37, 14).wrapping_sub(clut_dbp),
        csa: bits(tex0, 56, 5),
        upload_psm: bits(u64_at(b, tr + 0x20), 56, 6),
        upload: (bits(u64_at(b, tr + 0x40), 0, 12), bits(u64_at(b, tr + 0x40), 32, 12)),
        data_off: i32_at(b, tr + 0x74),
        clamp: u64_at(b, g + 0x80),
    })
}

/// The colour lookup of a 4- or 8-bit GS-info entry (empty for direct-colour formats).
pub(crate) fn lut(b: &[u8], g: &GsInfo) -> Vec<[u8; 4]> {
    let n = match g.psm {
        0x13 => 256,
        0x14 => 16,
        _ => return Vec::new(),
    };
    let (colors, off_pal) = (i32_at(b, 4), i32_at(b, 32));
    let pal = b.get(off_pal..off_pal + colors * 4).unwrap_or(&[]);
    (0..n)
        .map(|k| {
            let o = gs::clut_index(k, g.cbp, g.csa) * 4;
            pal.get(o..o + 4).map(|c| [c[0], c[1], c[2], gs::alpha(c[3])]).unwrap_or([255, 0, 255, 255])
        })
        .collect()
}

/// Converts linear pixel data in the GS-info's own format (what a TEXA sprite holds: indices for
/// PSMT8/PSMT4, the GS transfer's nibble order with pixel 0 in the low nibble) into RGBA8.
pub(crate) fn pixels_to_rgba(b: &[u8], g: &GsInfo, src: &[u8], w: usize, h: usize) -> Vec<u8> {
    let table = lut(b, g);
    let mut out = vec![0u8; w * h * 4];
    for p in 0..w * h {
        let c = match g.psm {
            0x13 => table[*src.get(p).unwrap_or(&0) as usize],
            0x14 => table[src.get(p / 2).map_or(0, |v| if p & 1 == 1 { v >> 4 } else { v & 15 }) as usize],
            _ => src.get(p * 4..p * 4 + 4).map_or([0; 4], |c| [c[0], c[1], c[2], gs::alpha(c[3])]),
        };
        out[p * 4..p * 4 + 4].copy_from_slice(&c);
    }
    out
}

pub(crate) fn i32_at(b: &[u8], o: usize) -> usize {
    b.get(o..o + 4).map(|s| i32::from_le_bytes(s.try_into().unwrap()).max(0) as usize).unwrap_or(0)
}

fn u64_at(b: &[u8], o: usize) -> u64 {
    b.get(o..o + 8).map(|s| u64::from_le_bytes(s.try_into().unwrap())).unwrap_or(0)
}

fn bits(v: u64, at: u32, n: u32) -> usize {
    ((v >> at) & ((1u64 << n) - 1)) as usize
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clamp_modes_give_the_sampled_region() {
        let a = |start, len, wrap| Axis { start, len, wrap };
        assert_eq!(Axis::from_clamp(0, 0, 0, 256), a(0, 256, Wrap::Repeat));
        assert_eq!(Axis::from_clamp(1, 5, 9, 256), a(0, 256, Wrap::Clamp));
        // REGION_CLAMP: MIN..=MAX.
        assert_eq!(Axis::from_clamp(2, 128, 191, 256), a(128, 64, Wrap::Clamp));
        // REGION_REPEAT: (t & MSK) | FIX.
        assert_eq!(Axis::from_clamp(3, 127, 0, 256), a(0, 128, Wrap::Repeat));
        assert_eq!(Axis::from_clamp(3, 63, 64, 128), a(64, 64, Wrap::Repeat));
        // A mask that is not a run of low bits, or a FIX inside the mask, stays plain REPEAT.
        assert_eq!(Axis::from_clamp(3, 0x5f, 0, 128), a(0, 128, Wrap::Repeat));
        assert_eq!(Axis::from_clamp(3, 63, 32, 128), a(0, 128, Wrap::Repeat));
    }
}

/// Decodes every GS-info entry of a texture container into an RGBA image.
pub fn decode(b: &[u8]) -> Vec<Texture> {
    if b.len() < 36 || i32_at(b, 0) != 0 {
        return Vec::new();
    }
    let n_gs = i32_at(b, 12);
    let mut out = Vec::with_capacity(n_gs.min(b.len() / GSINFO));
    for i in 0..n_gs {
        let Some(g) = gs_info(b, i) else { break };
        let (upload_psm, (rrw, rrh), data_off, clamp, psm) = (g.upload_psm, g.upload, g.data_off, g.clamp, g.psm);
        let (w, h) = (g.width, g.height);
        let wrap = |m: usize| if m == 1 || m == 2 { Wrap::Clamp } else { Wrap::Repeat };
        let mut rgba = vec![255u8; w * h * 4];
        let src = b.get(data_off..).unwrap_or(&[]);
        match psm {
            0x13 | 0x14 => {
                let four = psm == 0x14;
                let (bw, bh) = (w.div_ceil(128).max(1), if four { h.div_ceil(128) } else { h.div_ceil(64) }.max(1));
                let page_h = if four { 128 } else { 64 };
                // Indices (one byte per pixel) after undoing the PSMCT32 upload swizzle.
                let idx: Vec<u8> = if upload_psm == 0 {
                    // Lay the upload rectangle into a linear 32-bit canvas of whole pages, then swizzle.
                    let cw = 64 * bw;
                    // One PSMCT32 page (64x32 px, 8 KiB) holds one PSMT8 page (128x64) or PSMT4 page (128x128).
                    let mut canvas = vec![0u8; cw * 32 * bh * 4];
                    let (uw, uh) = (rrw.max(1), rrh.max(1));
                    for y in 0..uh {
                        let s = y * uw * 4;
                        let d = y * cw * 4;
                        if let (Some(row), Some(dst)) = (src.get(s..s + uw * 4), canvas.get_mut(d..d + uw * 4)) {
                            dst.copy_from_slice(row);
                        }
                    }
                    let mem = gs::encode32(&canvas, bw, bh);
                    let lin = if four { gs::decode4(&mem, bw, bh) } else { gs::decode8(&mem, bw, bh) };
                    let stride = 128 * bw;
                    let mut idx = vec![0u8; w * h];
                    for y in 0..h.min(page_h * bh) {
                        for x in 0..w.min(stride) {
                            let p = x + stride * y;
                            idx[x + w * y] = if four {
                                let v = lin.get(p / 2).copied().unwrap_or(0);
                                if p & 1 == 1 { v & 15 } else { v >> 4 }
                            } else {
                                lin.get(p).copied().unwrap_or(0)
                            };
                        }
                    }
                    idx
                } else if four {
                    (0..w * h).map(|p| src.get(p / 2).map(|v| if p & 1 == 1 { v >> 4 } else { v & 15 }).unwrap_or(0)).collect()
                } else {
                    (0..w * h).map(|p| src.get(p).copied().unwrap_or(0)).collect()
                };
                let n = if four { 16 } else { 256 };
                let lut = lut(b, &g);
                for (p, &ix) in idx.iter().enumerate() {
                    rgba[p * 4..p * 4 + 4].copy_from_slice(&lut[ix as usize % n]);
                }
            }
            _ => {
                for p in 0..w * h {
                    if let Some(c) = src.get(p * 4..p * 4 + 4) {
                        rgba[p * 4..p * 4 + 4].copy_from_slice(&[c[0], c[1], c[2], gs::alpha(c[3])]);
                    }
                }
            }
        }
        out.push(Texture {
            width: w as u32,
            height: h as u32,
            rgba,
            wrap_u: wrap(bits(clamp, 0, 2)),
            wrap_v: wrap(bits(clamp, 2, 2)),
            region: [
                Axis::from_clamp(bits(clamp, 0, 2), bits(clamp, 4, 10) as u32, bits(clamp, 14, 10) as u32, w as u32),
                Axis::from_clamp(bits(clamp, 2, 2), bits(clamp, 24, 10) as u32, bits(clamp, 34, 10) as u32, h as u32),
            ],
            uv_span: matches!(bits(clamp, 0, 2), 0 | 3).then(|| [bits(clamp, 4, 10) as u32 + 1, bits(clamp, 24, 10) as u32 + 1]),
        });
    }
    out
}

#[cfg(test)]
mod bound_tests {
    use super::*;

    fn container(n_gs: i32, tex0: u64) -> Vec<u8> {
        let mut b = vec![0u8; 0x200];
        b[12..16].copy_from_slice(&n_gs.to_le_bytes());
        b[8..12].copy_from_slice(&1i32.to_le_bytes());
        b[16..20].copy_from_slice(&0x40i32.to_le_bytes());
        b[20..24].copy_from_slice(&0x60i32.to_le_bytes());
        b[24..28].copy_from_slice(&0x100i32.to_le_bytes());
        b[0x100 + 0x70..0x100 + 0x78].copy_from_slice(&tex0.to_le_bytes());
        b
    }

    #[test]
    fn huge_entry_count_stops_at_the_data() {
        let b = container(i32::MAX, 0);
        assert!(decode(&b).len() < 4);
    }

    #[test]
    fn oversize_texture_dimensions_are_rejected() {
        let b = container(1, (14u64 << 26) | (14u64 << 30));
        assert!(gs_info(&b, 0).is_none());
        assert!(decode(&b).is_empty());
    }
}
