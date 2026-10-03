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

#[derive(Debug, Clone)]
pub struct Texture {
    pub width: u32,
    pub height: u32,
    /// RGBA8, row-major, straight alpha.
    pub rgba: Vec<u8>,
    pub wrap_u: Wrap,
    pub wrap_v: Wrap,
}

const TRANSFER: usize = 0x90;
const GSINFO: usize = 0xA0;

fn i32_at(b: &[u8], o: usize) -> usize {
    b.get(o..o + 4).map(|s| i32::from_le_bytes(s.try_into().unwrap()).max(0) as usize).unwrap_or(0)
}

fn u64_at(b: &[u8], o: usize) -> u64 {
    b.get(o..o + 8).map(|s| u64::from_le_bytes(s.try_into().unwrap())).unwrap_or(0)
}

fn bits(v: u64, at: u32, n: u32) -> usize {
    ((v >> at) & ((1u64 << n) - 1)) as usize
}

/// Decodes every GS-info entry of a texture container into an RGBA image.
pub fn decode(b: &[u8]) -> Vec<Texture> {
    if b.len() < 36 || i32_at(b, 0) != 0 {
        return Vec::new();
    }
    let (colors, n_tex, n_gs) = (i32_at(b, 4), i32_at(b, 8), i32_at(b, 12));
    let (off_data, off_tr, off_gs, off_pal) = (i32_at(b, 16), i32_at(b, 20), i32_at(b, 24), i32_at(b, 32));
    let pal = b.get(off_pal..off_pal + colors * 4).unwrap_or(&[]);
    let clut_dbp = bits(u64_at(b, off_tr + 0x20), 32, 14);
    let mut out = Vec::with_capacity(n_gs);
    for i in 0..n_gs {
        let ti = *b.get(off_data + i).unwrap_or(&0) as usize;
        let tr = off_tr + TRANSFER * (1 + ti.min(n_tex.saturating_sub(1)));
        let upload_psm = bits(u64_at(b, tr + 0x20), 56, 6);
        let (rrw, rrh) = (bits(u64_at(b, tr + 0x40), 0, 12), bits(u64_at(b, tr + 0x40), 32, 12));
        let data_off = i32_at(b, tr + 0x74);
        let g = off_gs + GSINFO * i;
        let tex0 = u64_at(b, g + 0x70);
        let clamp = u64_at(b, g + 0x80);
        let (psm, tw, th) = (bits(tex0, 20, 6), bits(tex0, 26, 4), bits(tex0, 30, 4));
        let (cbp, csa) = (bits(tex0, 37, 14).wrapping_sub(clut_dbp), bits(tex0, 56, 5));
        let (w, h) = (1usize << tw, 1usize << th);
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
                let lut: Vec<[u8; 4]> = (0..n)
                    .map(|k| {
                        let o = gs::clut_index(k, cbp, csa) * 4;
                        pal.get(o..o + 4).map(|c| [c[0], c[1], c[2], gs::alpha(c[3])]).unwrap_or([255, 0, 255, 255])
                    })
                    .collect();
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
        });
    }
    out
}
