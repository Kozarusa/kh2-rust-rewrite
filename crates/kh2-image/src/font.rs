//! The game's text fonts: glyph atlases and widths from `msg/<lang>/fontimage.bar` and
//! `msg/<lang>/fontinfo.bar`. Layout and rules were read from the game's text drawing code in a
//! local decompilation (docs/hud.md lists the addresses); the file layouts were found by
//! decoding the files and checking them against that code.
//!
//! `fontimage.bar` holds three type-36 entries: `sys` (a 512x256 4-bit atlas of 18x24 cells),
//! `evt` (512x512, 24x32 cells) and `icon` (a 256x160 8-bit atlas of 24x24 icons followed by a
//! 256-entry palette). The glyph atlases store 2-bit coverage in the low bits of each nibble; the
//! game samples them through rows of the icon palette (rows 8 and 9 for the plain font), so the
//! palette rows are the alpha ramps and are read from the file here. `fontinfo.bar` holds one
//! advance width per glyph (`sys`, `evt`) or per icon (`icon`).

/// Number of 4-bit pixels in a glyph atlas row.
pub const ATLAS_W: usize = 512;
pub const SYS_H: usize = 256;
pub const EVT_H: usize = 512;
pub const ICON_W: usize = 256;
pub const ICON_H: usize = 160;
/// Icon cell size, and icons per row.
pub const ICON_CELL: usize = 24;
pub const ICON_COLS: usize = ICON_W / ICON_CELL;

/// Which glyph atlas a font draws from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Atlas {
    Sys,
    Evt,
}

/// One of the game's six text fonts (the descriptors the game builds at 0x1a9e60 for any language
/// but Japanese; the cell heights come from the override at 0x1a9148).
#[derive(Debug, Clone, Copy)]
pub struct Font {
    /// The font's mode: 0 plain, 1 drop shadow, 2 outline, 3 plain with the thin ramp, 4 the shaded
    /// ramp with the skewed draw, 5 plain.
    pub mode: u8,
    pub atlas: Atlas,
    pub cell_w: u32,
    pub cell_h: u32,
    /// Nominal glyph size (descriptor byte +0x11): a line is this many pixels high at scale 1.
    pub size: u32,
    /// Descriptor byte +0x10: a space is a third of it.
    pub space_basis: u32,
    /// Extra pixels the game adds between lines (descriptor +0x14).
    pub line_gap: u32,
}

impl Font {
    /// Font `id` 0 to 5.
    pub fn get(id: usize) -> Font {
        let sys = |mode, gap| Font { mode, atlas: Atlas::Sys, cell_w: 18, cell_h: 24, size: 18, space_basis: 18, line_gap: gap };
        let evt = |mode| Font { mode, atlas: Atlas::Evt, cell_w: 24, cell_h: 32, size: 24, space_basis: 24, line_gap: 2 };
        match id {
            0 => sys(0, 6),
            1 => sys(1, 6),
            2 => sys(2, 6),
            3 => evt(3),
            4 => evt(4),
            _ => sys(5, 6),
        }
    }

    pub fn atlas_h(&self) -> usize {
        match self.atlas {
            Atlas::Sys => SYS_H,
            Atlas::Evt => EVT_H,
        }
    }

    pub fn cols(&self) -> usize {
        ATLAS_W / self.cell_w as usize
    }

    pub fn rows(&self) -> usize {
        self.atlas_h() / self.cell_h as usize
    }

    /// Glyphs in one page of the atlas.
    pub fn per_page(&self) -> usize {
        self.cols() * self.rows()
    }

    /// Palette row (16 entries) the font's mode uses for page 0; page 1 uses the next row.
    pub fn clut_row(&self) -> usize {
        match self.mode {
            3 => 10,
            4 => 12,
            _ => 8,
        }
    }

    /// Texel rectangle of glyph `g` in its atlas and the page it is on (page 0 = the low two bits
    /// of each nibble, page 1 = the high two bits; pages 2 and up live in a second plane that the
    /// US data does not fill).
    pub fn cell(&self, g: usize) -> Option<(usize, [u32; 4])> {
        let per = self.per_page();
        let page = g / per;
        if page > 1 {
            return None;
        }
        let i = g % per;
        let (cw, ch) = (self.cell_w, self.cell_h);
        let (x, y) = ((i % self.cols()) as u32 * cw, (i / self.cols()) as u32 * ch);
        Some((page, [x, y, x + cw, y + ch]))
    }
}

/// The font files, decoded.
#[derive(Debug, Clone)]
pub struct FontSet {
    /// Advance widths in pixels at scale 1: `sys`, `evt`, `icon`.
    pub widths: [Vec<u8>; 3],
    sys: Vec<u8>,
    evt: Vec<u8>,
    icons: Vec<u8>,
    /// The 256-entry palette in logical order (RGBA, alpha 0x80 = opaque).
    pub clut: Vec<[u8; 4]>,
}

/// Unpacks 4-bit pixels, low nibble first.
fn nibbles(b: &[u8]) -> Vec<u8> {
    b.iter().flat_map(|&v| [v & 15, v >> 4]).collect()
}

impl FontSet {
    /// Reads `fontinfo.bar` and `fontimage.bar`.
    pub fn parse(info: &[u8], image: &[u8]) -> Option<FontSet> {
        let info = kh2_bar::parse(info).ok()?;
        let img = kh2_bar::parse(image).ok()?;
        let width = |tag: &str| info.iter().find(|e| e.tag == tag && e.kind == 2).map(|e| e.data.to_vec());
        let plane = |tag: &str| img.iter().find(|e| e.tag == tag && e.kind == 36).map(|e| e.data);
        let (sys, evt, icon) = (plane("sys")?, plane("evt")?, plane("icon")?);
        if sys.len() < ATLAS_W * SYS_H / 2 || evt.len() < ATLAS_W * EVT_H / 2 || icon.len() < ICON_W * ICON_H + 1024 {
            return None;
        }
        // The palette is a PSMCT32 CLUT in GS order: logical entry n sits at n with bits 3 and 4 swapped.
        let pal = &icon[ICON_W * ICON_H..ICON_W * ICON_H + 1024];
        let swap = |n: usize| (n & 0xe7) | (n & 8) << 1 | (n & 16) >> 1;
        let clut = (0..256).map(|n| {
            let o = swap(n) * 4;
            [pal[o], pal[o + 1], pal[o + 2], pal[o + 3]]
        });
        Some(FontSet {
            widths: [width("sys")?, width("evt")?, width("icon")?],
            sys: nibbles(&sys[..ATLAS_W * SYS_H / 2]),
            evt: nibbles(&evt[..ATLAS_W * EVT_H / 2]),
            icons: icon[..ICON_W * ICON_H].to_vec(),
            clut: clut.collect(),
        })
    }

    /// Advance width of glyph `g` of `font`, in pixels at scale 1.
    pub fn width(&self, font: &Font, g: usize) -> u32 {
        let table = match font.atlas {
            Atlas::Sys => &self.widths[0],
            Atlas::Evt => &self.widths[1],
        };
        table.get(g).copied().unwrap_or(0) as u32
    }

    /// Advance width of icon `i`.
    pub fn icon_width(&self, i: usize) -> u32 {
        self.widths[2].get(i).copied().unwrap_or(0) as u32
    }

    /// RGBA pixels of a glyph atlas as the game's palette row turns them: `row` is the palette
    /// row (`Font::clut_row` plus the page). Colour is the palette's, alpha the palette's (0x80 =
    /// opaque, expanded to 0xff).
    pub fn atlas_rgba(&self, atlas: Atlas, row: usize) -> (usize, usize, Vec<u8>) {
        let (src, h) = match atlas {
            Atlas::Sys => (&self.sys, SYS_H),
            Atlas::Evt => (&self.evt, EVT_H),
        };
        let mut out = Vec::with_capacity(ATLAS_W * h * 4);
        for &n in src.iter() {
            let c = self.clut[(row * 16 + n as usize) & 255];
            out.extend_from_slice(&[c[0].saturating_mul(2), c[1].saturating_mul(2), c[2].saturating_mul(2), kh2_ps2::gs::alpha(c[3])]);
        }
        (ATLAS_W, h, out)
    }

    /// RGBA pixels of the icon atlas (8-bit indices through the palette).
    pub fn icons_rgba(&self) -> (usize, usize, Vec<u8>) {
        let mut out = Vec::with_capacity(ICON_W * ICON_H * 4);
        for &ix in &self.icons {
            // The 8-bit pixels use the CLUT in its GS order as well, so the logical index is the raw one.
            let c = self.clut[ix as usize];
            out.extend_from_slice(&[c[0].saturating_mul(2), c[1].saturating_mul(2), c[2].saturating_mul(2), kh2_ps2::gs::alpha(c[3])]);
        }
        (ICON_W, ICON_H, out)
    }

    /// Texel rectangle of icon `i`.
    pub fn icon_cell(&self, i: usize) -> [u32; 4] {
        let (x, y) = ((i % ICON_COLS) * ICON_CELL, (i / ICON_COLS) * ICON_CELL);
        [x as u32, y as u32, (x + ICON_CELL) as u32, (y + ICON_CELL) as u32]
    }
}
