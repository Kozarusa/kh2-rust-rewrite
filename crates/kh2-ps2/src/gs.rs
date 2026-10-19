//! GS local-memory swizzling. KH2 uploads texture pages as PSMCT32 and samples them as PSMT8 or
//! PSMT4, so the linear pixels are recovered by writing them in the 32-bit layout and reading them
//! back in the 8/4-bit layout. Block/column tables describe GS hardware; values as published in
//! OpenKH `OpenKh.Kh2/Ps2.Reform{32,8,4}.cs` (Apache-2.0).

const BLOCK32: [usize; 32] =
    [0, 1, 4, 5, 16, 17, 20, 21, 2, 3, 6, 7, 18, 19, 22, 23, 8, 9, 12, 13, 24, 25, 28, 29, 10, 11, 14, 15, 26, 27, 30, 31];
const COL32: [usize; 16] = [0, 1, 4, 5, 8, 9, 12, 13, 2, 3, 6, 7, 10, 11, 14, 15];
const BLOCK8: [usize; 32] = BLOCK32;
const COL8_0: [usize; 64] = [
    0x00, 0x04, 0x10, 0x14, 0x20, 0x24, 0x30, 0x34, 0x02, 0x06, 0x12, 0x16, 0x22, 0x26, 0x32, 0x36, 0x08, 0x0c, 0x18,
    0x1c, 0x28, 0x2c, 0x38, 0x3c, 0x0a, 0x0e, 0x1a, 0x1e, 0x2a, 0x2e, 0x3a, 0x3e, 0x21, 0x25, 0x31, 0x35, 0x01, 0x05,
    0x11, 0x15, 0x23, 0x27, 0x33, 0x37, 0x03, 0x07, 0x13, 0x17, 0x29, 0x2d, 0x39, 0x3d, 0x09, 0x0d, 0x19, 0x1d, 0x2b,
    0x2f, 0x3b, 0x3f, 0x0b, 0x0f, 0x1b, 0x1f,
];
const COL8_1: [usize; 64] = [
    0x20, 0x24, 0x30, 0x34, 0x00, 0x04, 0x10, 0x14, 0x22, 0x26, 0x32, 0x36, 0x02, 0x06, 0x12, 0x16, 0x28, 0x2c, 0x38,
    0x3c, 0x08, 0x0c, 0x18, 0x1c, 0x2a, 0x2e, 0x3a, 0x3e, 0x0a, 0x0e, 0x1a, 0x1e, 0x01, 0x05, 0x11, 0x15, 0x21, 0x25,
    0x31, 0x35, 0x03, 0x07, 0x13, 0x17, 0x23, 0x27, 0x33, 0x37, 0x09, 0x0d, 0x19, 0x1d, 0x29, 0x2d, 0x39, 0x3d, 0x0b,
    0x0f, 0x1b, 0x1f, 0x2b, 0x2f, 0x3b, 0x3f,
];
const BLOCK4: [usize; 32] =
    [0, 2, 8, 10, 1, 3, 9, 11, 4, 6, 12, 14, 5, 7, 13, 15, 16, 18, 24, 26, 17, 19, 25, 27, 20, 22, 28, 30, 21, 23, 29, 31];
const COL4_0: [usize; 128] = [
    0x00, 0x20, 0x80, 0xa0, 0x100, 0x120, 0x180, 0x1a0, 0x08, 0x28, 0x88, 0xa8, 0x108, 0x128, 0x188, 0x1a8, 0x10, 0x30,
    0x90, 0xb0, 0x110, 0x130, 0x190, 0x1b0, 0x18, 0x38, 0x98, 0xb8, 0x118, 0x138, 0x198, 0x1b8, 0x40, 0x60, 0xc0, 0xe0,
    0x140, 0x160, 0x1c0, 0x1e0, 0x48, 0x68, 0xc8, 0xe8, 0x148, 0x168, 0x1c8, 0x1e8, 0x50, 0x70, 0xd0, 0xf0, 0x150, 0x170,
    0x1d0, 0x1f0, 0x58, 0x78, 0xd8, 0xf8, 0x158, 0x178, 0x1d8, 0x1f8, 0x104, 0x124, 0x184, 0x1a4, 0x04, 0x24, 0x84, 0xa4,
    0x10c, 0x12c, 0x18c, 0x1ac, 0x0c, 0x2c, 0x8c, 0xac, 0x114, 0x134, 0x194, 0x1b4, 0x14, 0x34, 0x94, 0xb4, 0x11c, 0x13c,
    0x19c, 0x1bc, 0x1c, 0x3c, 0x9c, 0xbc, 0x144, 0x164, 0x1c4, 0x1e4, 0x44, 0x64, 0xc4, 0xe4, 0x14c, 0x16c, 0x1cc, 0x1ec,
    0x4c, 0x6c, 0xcc, 0xec, 0x154, 0x174, 0x1d4, 0x1f4, 0x54, 0x74, 0xd4, 0xf4, 0x15c, 0x17c, 0x1dc, 0x1fc, 0x5c, 0x7c,
    0xdc, 0xfc,
];
const COL4_1: [usize; 128] = [
    0x100, 0x120, 0x180, 0x1a0, 0x00, 0x20, 0x80, 0xa0, 0x108, 0x128, 0x188, 0x1a8, 0x08, 0x28, 0x88, 0xa8, 0x110, 0x130,
    0x190, 0x1b0, 0x10, 0x30, 0x90, 0xb0, 0x118, 0x138, 0x198, 0x1b8, 0x18, 0x38, 0x98, 0xb8, 0x140, 0x160, 0x1c0, 0x1e0,
    0x40, 0x60, 0xc0, 0xe0, 0x148, 0x168, 0x1c8, 0x1e8, 0x48, 0x68, 0xc8, 0xe8, 0x150, 0x170, 0x1d0, 0x1f0, 0x50, 0x70,
    0xd0, 0xf0, 0x158, 0x178, 0x1d8, 0x1f8, 0x58, 0x78, 0xd8, 0xf8, 0x04, 0x24, 0x84, 0xa4, 0x104, 0x124, 0x184, 0x1a4,
    0x0c, 0x2c, 0x8c, 0xac, 0x10c, 0x12c, 0x18c, 0x1ac, 0x14, 0x34, 0x94, 0xb4, 0x114, 0x134, 0x194, 0x1b4, 0x1c, 0x3c,
    0x9c, 0xbc, 0x11c, 0x13c, 0x19c, 0x1bc, 0x44, 0x64, 0xc4, 0xe4, 0x144, 0x164, 0x1c4, 0x1e4, 0x4c, 0x6c, 0xcc, 0xec,
    0x14c, 0x16c, 0x1cc, 0x1ec, 0x54, 0x74, 0xd4, 0xf4, 0x154, 0x174, 0x1d4, 0x1f4, 0x5c, 0x7c, 0xdc, 0xfc, 0x15c, 0x17c,
    0x1dc, 0x1fc,
];

/// Linear PSMCT32 image (`bw` pages of 64 px wide, `bh` pages of 32 px tall) -> GS memory bytes.
pub fn encode32(lin: &[u8], bw: usize, bh: usize) -> Vec<u8> {
    let mut mem = vec![0u8; lin.len()];
    for py in 0..bh {
        for px in 0..bw {
            let page = 0x2000 * (px + bw * py);
            for by in 0..4 {
                for bx in 0..8 {
                    let block = 0x100 * BLOCK32[bx + by * 8];
                    for col in 0..4 {
                        for (k, &c) in COL32.iter().enumerate() {
                            let x = px * 64 + bx * 8 + k % 8;
                            let y = py * 32 + by * 8 + 2 * col + k / 8;
                            let src = 4 * (x + 64 * bw * y);
                            let dst = page + block + 0x40 * col + 4 * c;
                            if src + 4 <= lin.len() && dst + 4 <= mem.len() {
                                mem[dst..dst + 4].copy_from_slice(&lin[src..src + 4]);
                            }
                        }
                    }
                }
            }
        }
    }
    mem
}

/// GS memory -> linear PSMT8 image (`bw` pages of 128 px, `bh` pages of 64 px).
pub fn decode8(mem: &[u8], bw: usize, bh: usize) -> Vec<u8> {
    let mut out = vec![0u8; mem.len()];
    for py in 0..bh {
        for px in 0..bw {
            let page = 0x2000 * (px + bw * py);
            for by in 0..4 {
                for bx in 0..8 {
                    let block = 0x100 * BLOCK8[bx + 8 * by];
                    for col in 0..4 {
                        let table = if col & 1 == 0 { &COL8_0 } else { &COL8_1 };
                        for (k, &t) in table.iter().enumerate() {
                            let x = px * 128 + bx * 16 + k % 16;
                            let y = py * 64 + by * 16 + 4 * col + k / 16;
                            let (src, dst) = (page + block + 0x40 * col + t, x + 128 * bw * y);
                            if src < mem.len() && dst < out.len() {
                                out[dst] = mem[src];
                            }
                        }
                    }
                }
            }
        }
    }
    out
}

/// GS memory -> linear PSMT4 image (`bw` pages of 128 px, `bh` pages of 128 px).
/// Output packs two pixels per byte, even pixel in the high nibble.
pub fn decode4(mem: &[u8], bw: usize, bh: usize) -> Vec<u8> {
    let mut out = vec![0u8; mem.len()];
    for py in 0..bh {
        for px in 0..bw {
            let page = 0x2000 * (px + bw * py);
            for by in 0..8 {
                for bx in 0..4 {
                    let block = 0x100 * BLOCK4[bx + 4 * by];
                    for col in 0..4 {
                        let table = if col & 1 == 0 { &COL4_0 } else { &COL4_1 };
                        for (k, &t) in table.iter().enumerate() {
                            let src = page + block + 0x40 * col + t / 8;
                            let nib = mem.get(src).map(|b| (b >> (t % 8)) & 15).unwrap_or(0);
                            let x = px * 128 + bx * 32 + k % 32;
                            let y = py * 128 + by * 16 + 4 * col + k / 32;
                            let p = x + 128 * bw * y;
                            if let Some(o) = out.get_mut(p / 2) {
                                *o = if p & 1 == 1 { (*o & 0xF0) | nib } else { (*o & 0x0F) | (nib << 4) };
                            }
                        }
                    }
                }
            }
        }
    }
    out
}

/// Index of palette entry `index` in a linearly uploaded CLUT area, given the texture's CBP
/// (relative to the CLUT upload's base) and CSA (16-colour block offset for 4-bit textures).
pub fn clut_index(index: usize, cbp: usize, csa: usize) -> usize {
    (index & 7) + (index & 8) * 8 + (index & 16) / 2 + (index & !31) * 4 + (cbp & 7) * 4 + (cbp & 8) * 0x80
        + (cbp & 16) * 2
        + (cbp & !31) * 0x40
        + (csa & 1) * 8
        + (csa & 14) * 0x40
}

/// PS2 alpha (0x80 = opaque) -> 8-bit alpha.
pub fn alpha(a: u8) -> u8 {
    (a as u16 * 2).min(255) as u8
}

/// GS local memory (4 MiB) with the hardware swizzles, for formats that upload and sample at
/// arbitrary block pointers (effect textures and their CLUTs).
pub struct Vram {
    pub mem: Vec<u8>,
}

impl Default for Vram {
    fn default() -> Vram {
        Vram { mem: vec![0; 4 << 20] }
    }
}

impl Vram {
    /// Byte address of PSMCT32 pixel (x, y) in a buffer at block pointer `bp`, `bw` × 64 px wide.
    pub fn addr32(bp: usize, bw: usize, x: usize, y: usize) -> usize {
        let page = (y / 32) * bw.max(1) + x / 64;
        let block = BLOCK32[(x % 64) / 8 + ((y % 32) / 8) * 8];
        let word = COL32[(x % 8) + (y % 2) * 8];
        let col = (y % 8) / 2;
        (((bp + page * 32 + block) * 64 + col * 16 + word) * 4) & ((4 << 20) - 1)
    }

    /// Byte address of PSMT8 pixel (x, y); `bw` is in 64-pixel units like TBW/DBW (pages are 128 wide).
    pub fn addr8(bp: usize, bw: usize, x: usize, y: usize) -> usize {
        let page = (y / 64) * (bw / 2).max(1) + x / 128;
        let block = BLOCK8[(x % 128) / 16 + ((y % 64) / 16) * 8];
        let col = (y % 16) / 4;
        let table = if col & 1 == 0 { &COL8_0 } else { &COL8_1 };
        let byte = table[(x % 16) + (y % 4) * 16];
        ((bp + page * 32 + block) * 256 + col * 64 + byte) & ((4 << 20) - 1)
    }

    /// Host → local transfer of a linear PSMT8 rectangle.
    #[allow(clippy::too_many_arguments)] // a GS transfer is the BITBLTBUF/TRXPOS/TRXREG fields; public API shared with other crates
    pub fn write8(&mut self, dbp: usize, dbw: usize, x0: usize, y0: usize, w: usize, h: usize, data: &[u8]) {
        for y in 0..h {
            for x in 0..w {
                if let Some(&v) = data.get(x + y * w) {
                    self.mem[Self::addr8(dbp, dbw, x0 + x, y0 + y)] = v;
                }
            }
        }
    }

    /// Host → local transfer of a linear PSMCT32 rectangle (RGBA, alpha 0..0x80).
    #[allow(clippy::too_many_arguments)] // same transfer fields as write8
    pub fn write32(&mut self, dbp: usize, dbw: usize, x0: usize, y0: usize, w: usize, h: usize, data: &[u8]) {
        for y in 0..h {
            for x in 0..w {
                let s = 4 * (x + y * w);
                if let Some(px) = data.get(s..s + 4) {
                    let d = Self::addr32(dbp, dbw, x0 + x, y0 + y);
                    self.mem[d..d + 4].copy_from_slice(px);
                }
            }
        }
    }

    /// Decodes a PSMT8 texture with a CSM1 PSMCT32 CLUT to RGBA8 (alpha expanded to 0..255).
    /// `tex0` is the GS TEX0 register value.
    pub fn texture8(&self, tex0: u64) -> (usize, usize, Vec<u8>) {
        let tbp = (tex0 & 0x3fff) as usize;
        let tbw = ((tex0 >> 14) & 0x3f) as usize;
        let w = 1usize << ((tex0 >> 26) & 0xf);
        let h = 1usize << ((tex0 >> 30) & 0xf);
        let cbp = ((tex0 >> 37) & 0x3fff) as usize;
        let mut clut = [[0u8; 4]; 256];
        for (i, c) in clut.iter_mut().enumerate() {
            let j = (i & 0xe7) | (i & 8) << 1 | (i & 16) >> 1;
            let a = Self::addr32(cbp, 1, j % 16, j / 16);
            *c = [self.mem[a], self.mem[a + 1], self.mem[a + 2], alpha(self.mem[a + 3])];
        }
        let mut out = vec![0u8; w * h * 4];
        for y in 0..h {
            for x in 0..w {
                let idx = self.mem[Self::addr8(tbp, tbw, x, y)] as usize;
                out[4 * (x + y * w)..4 * (x + y * w) + 4].copy_from_slice(&clut[idx]);
            }
        }
        (w, h, out)
    }
}

#[cfg(test)]
mod vram_tests {
    use super::*;

    #[test]
    fn psmt8_round_trip_with_clut() {
        let mut v = Vram::default();
        let (w, h) = (64usize, 32usize);
        let pix: Vec<u8> = (0..w * h).map(|i| (i * 7 % 256) as u8).collect();
        v.write8(0x2c40, 2, 0, 0, w, h, &pix);
        // Palette: entry k (linear upload order) = (k, 255 - k, 0, 0x80).
        let pal: Vec<u8> = (0..256).flat_map(|k| [k as u8, 255 - k as u8, 0, 0x80]).collect();
        v.write32(0x2e00, 1, 0, 0, 16, 16, &pal);
        let tex0 = 0x2c40u64 | 2 << 14 | 0x13 << 20 | 6 << 26 | 5 << 30 | 0x2e00u64 << 37;
        let (tw, th, rgba) = v.texture8(tex0);
        assert_eq!((tw, th), (64, 32));
        for i in [0, 1, 17, 200, 2047] {
            let idx = pix[i] as usize;
            let k = (idx & 0xe7) | (idx & 8) << 1 | (idx & 16) >> 1;
            assert_eq!(&rgba[4 * i..4 * i + 4], &[k as u8, 255 - k as u8, 0, 255]);
        }
    }

    #[test]
    fn addr32_matches_page_encoder() {
        // encode32 lays out one 64x32 page; addr32 must agree with it.
        let lin: Vec<u8> = (0..64 * 32 * 4).map(|i| (i / 4 % 251) as u8).collect();
        let mem = encode32(&lin, 1, 1);
        for (x, y) in [(0, 0), (7, 1), (13, 9), (63, 31)] {
            let a = Vram::addr32(0, 1, x, y);
            assert_eq!(mem[a], lin[4 * (x + 64 * y)]);
        }
    }
}
