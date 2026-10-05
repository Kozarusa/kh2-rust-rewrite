//! IMGD images (and IMGZ, a count-prefixed run of IMGDs).
//!
//! Header (64 bytes) per OpenKH `docs/kh2/file/type/image.md`: magic `IMGD`, version, pixel and
//! palette offsets and sizes, width, height, GS pixel mode (0x13 = 8-bit, 0x14 = 4-bit), image
//! type (3 = 32-bit, 4 = 4-bit, 5 = 8-bit) and flags (bit 2: pixels are in GS memory order).

use kh2_ps2::gs;

use crate::{rd16, rd32};

#[derive(Debug, Clone)]
pub struct Image {
    pub width: u32,
    pub height: u32,
    /// RGBA8, row-major, straight alpha (PS2 alpha 0x80 expanded to 0xff).
    pub rgba: Vec<u8>,
    pub swizzled: bool,
}

pub fn is_imgd(b: &[u8]) -> bool {
    b.starts_with(b"IMGD")
}

/// Decodes one IMGD. `None` when the header is not an IMGD or the data is truncated.
pub fn decode(b: &[u8]) -> Option<Image> {
    if !is_imgd(b) || b.len() < 0x40 {
        return None;
    }
    let (poff, psz) = (rd32(b, 8) as usize, rd32(b, 12) as usize);
    let (coff, csz) = (rd32(b, 0x10) as usize, rd32(b, 0x14) as usize);
    let (w, h) = (rd16(b, 0x1c) as usize, rd16(b, 0x1e) as usize);
    let kind = rd16(b, 0x34);
    let flags = rd32(b, 0x3c);
    let swizzled = flags & 4 != 0;
    let pix = b.get(poff..poff + psz)?;
    let mut rgba = vec![0u8; w * h * 4];
    match kind {
        3 => {
            for i in 0..w * h {
                if let Some(c) = pix.get(i * 4..i * 4 + 4) {
                    rgba[i * 4..i * 4 + 4].copy_from_slice(&[c[0], c[1], c[2], gs::alpha(c[3])]);
                }
            }
        }
        4 | 5 => {
            let pal = b.get(coff..coff + csz)?;
            let four = kind == 4;
            let idx = if swizzled { unswizzle(pix, w, h, four) } else { linear(pix, w, h, four) };
            for (i, &ix) in idx.iter().enumerate().take(w * h) {
                // Eight-bit palettes keep the GS CLUT order (bits 3 and 4 swapped) in swizzled images.
                let k = if !four && swizzled { (ix as usize & 0xe7) | (ix as usize & 8) << 1 | (ix as usize & 16) >> 1 } else { ix as usize };
                let c = pal.get(k * 4..k * 4 + 4).unwrap_or(&[255, 0, 255, 0x80]);
                rgba[i * 4..i * 4 + 4].copy_from_slice(&[c[0], c[1], c[2], gs::alpha(c[3])]);
            }
        }
        _ => return None,
    }
    Some(Image { width: w as u32, height: h as u32, rgba, swizzled })
}

fn linear(pix: &[u8], w: usize, h: usize, four: bool) -> Vec<u8> {
    if four {
        (0..w * h).map(|p| pix.get(p / 2).map(|v| if p & 1 == 0 { v & 15 } else { v >> 4 }).unwrap_or(0)).collect()
    } else {
        (0..w * h).map(|p| pix.get(p).copied().unwrap_or(0)).collect()
    }
}

/// Pixel data is the PSMCT32 upload canvas (whole GS pages in raster order); writing it into GS
/// memory and reading it back as PSMT8/PSMT4 gives the linear indices.
fn unswizzle(pix: &[u8], w: usize, h: usize, four: bool) -> Vec<u8> {
    let (pw, ph) = (128, if four { 128 } else { 64 });
    let (bw, bh) = (w.div_ceil(pw).max(1), h.div_ceil(ph).max(1));
    let mut canvas = vec![0u8; 64 * bw * 32 * bh * 4];
    let n = pix.len().min(canvas.len());
    canvas[..n].copy_from_slice(&pix[..n]);
    let mem = gs::encode32(&canvas, bw, bh);
    let lin = if four { gs::decode4(&mem, bw, bh) } else { gs::decode8(&mem, bw, bh) };
    let stride = pw * bw;
    let mut out = vec![0u8; w * h];
    for y in 0..h {
        for x in 0..w {
            let p = x + stride * y;
            out[x + w * y] = if four {
                let v = lin.get(p / 2).copied().unwrap_or(0);
                if p & 1 == 1 { v & 15 } else { v >> 4 }
            } else {
                lin.get(p).copied().unwrap_or(0)
            };
        }
    }
    out
}

/// Splits an IMGZ into its IMGDs (header: magic `IMGZ`, 256, header length 16, count; IMGDs follow
/// back to back). Each IMGD is returned as a slice starting at its own header.
pub fn split_imgz(b: &[u8]) -> Vec<&[u8]> {
    if !b.starts_with(b"IMGZ") {
        return Vec::new();
    }
    let n = rd32(b, 12) as usize;
    let mut at = rd32(b, 8) as usize;
    let mut out = Vec::new();
    for _ in 0..n {
        let Some(s) = b.get(at..) else { break };
        if !is_imgd(s) {
            break;
        }
        // An IMGD ends at the end of its palette or pixel block, whichever comes last.
        let end = (rd32(s, 8) + rd32(s, 12)).max(rd32(s, 0x10) + rd32(s, 0x14)) as usize;
        out.push(&s[..end.min(s.len())]);
        at += end;
    }
    out
}
