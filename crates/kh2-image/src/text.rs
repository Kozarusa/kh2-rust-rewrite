//! System messages and text layout: `msg/<lang>/sys.bar` (the `sys` entry), the message byte
//! code, and the line layout the game's text drawing performs (addresses in docs/hud.md).
//!
//! A message is a run of bytes: a byte of 0x20 or more is one glyph (glyph index = byte - 0x20);
//! 0x19 to 0x1f followed by a byte pick a glyph past 0xdf; every other byte below 0x20 is a
//! command whose length comes from the table the game keeps at 0x37ae50. Layout follows the
//! interpreter at 0x1a8210: glyphs are drawn from a cell of the atlas, scaled by a size and a
//! width factor, and advance by the font's width table.

use std::collections::HashMap;

use crate::font::{Font, FontSet, ICON_CELL};

/// Length in bytes of each command (index = first byte), from the game's table at 0x37ae50.
const COMMAND_LEN: [u8; 32] = [0, 1, 1, 1, 2, 7, 2, 5, 4, 2, 2, 2, 2, 1, 2, 6, 1, 5, 3, 5, 3, 3, 2, 3, 0, 2, 2, 2, 2, 2, 2, 2];

/// Highest message byte that is a command (0x19 and up start a two-byte glyph code).
const LAST_COMMAND: u8 = 0x18;

/// Count of world codes the game knows, in index order (the index selects the colour column).
pub const WORLDS: [&str; 19] = ["zz", "es", "tt", "di", "hb", "bb", "he", "al", "mu", "po", "lk", "lm", "dc", "wi", "nm", "wm", "ca", "tr", "eh"];

#[derive(Debug, Clone, PartialEq)]
pub enum Token {
    /// A glyph of the font (index into the atlas, 0-based).
    Glyph(u16),
    /// 0x01: a space.
    Space,
    /// 0x02: end of line.
    Newline,
    /// 0x03: back to the default colour and size.
    Reset,
    /// 0x04: colour from the `ftst` table (index).
    Palette(u8),
    /// 0x07: colour as RGBA bytes (0x80 = 1.0).
    Rgba([u8; 4]),
    /// 0x09: an icon from the icon atlas.
    Icon(u8),
    /// 0x0a: glyph size in pixels.
    Size(u8),
    /// 0x0b: glyph width in percent.
    Width(u8),
    /// 0x0c: extra pixels between lines.
    Gap(u8),
    /// 0x0d: a value the caller substitutes (a name or a number).
    Variable,
    /// Any other command, kept so a message can be shown or re-encoded.
    Command(u8, Vec<u8>),
    /// 0x00 or 0x10.
    End,
}

/// Splits message bytes into tokens (ends at the first end command).
pub fn tokens(b: &[u8]) -> Vec<Token> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < b.len() {
        let c = b[i];
        if c >= 0x20 {
            out.push(Token::Glyph(c as u16 - 0x20));
            i += 1;
            continue;
        }
        if c > LAST_COMMAND {
            // 0x19..=0x1f: one more byte, glyph 0xe0 + 0x100 * (c - 0x19) + next.
            let next = b.get(i + 1).copied().unwrap_or(0) as u16;
            out.push(Token::Glyph((c as u16 - 0x19) * 0x100 + next + 0xe0));
            i += 2;
            continue;
        }
        let len = (COMMAND_LEN[c as usize] as usize).max(1);
        let a = b.get(i + 1..(i + len).min(b.len())).unwrap_or(&[]);
        let arg = |k: usize| a.get(k).copied().unwrap_or(0);
        match c {
            0 | 0x10 => {
                out.push(Token::End);
                break;
            }
            1 => out.push(Token::Space),
            2 => out.push(Token::Newline),
            3 => out.push(Token::Reset),
            4 => out.push(Token::Palette(arg(0))),
            7 => out.push(Token::Rgba([arg(0), arg(1), arg(2), arg(3)])),
            9 => out.push(Token::Icon(arg(0))),
            0xa => out.push(Token::Size(arg(0))),
            0xb => out.push(Token::Width(arg(0))),
            0xc => out.push(Token::Gap(arg(0))),
            0xd => out.push(Token::Variable),
            _ => out.push(Token::Command(c, a.to_vec())),
        }
        i += len;
    }
    out
}

/// The character a glyph shows, where it is a plain letter, digit or sign (read off the atlas).
pub fn glyph_char(g: u16) -> Option<char> {
    let g = g as usize;
    Some(match g {
        1..=10 => (b'0' + (g - 1) as u8) as char,
        11 => '+',
        12 => '-',
        13 => '\u{d7}',
        14..=39 => (b'A' + (g - 14) as u8) as char,
        40 => '!',
        41 => '?',
        42 => '%',
        43 => '/',
        44 => '\u{203b}',
        45 => '\u{3001}',
        46 => '\u{3002}',
        47 => '.',
        48 => ',',
        49 => ';',
        50 => ':',
        51 => '\u{2026}',
        52 | 53 => '\u{2013}',
        54 => '~',
        55 => '\'',
        58 => '(',
        59 => ')',
        60 => '\u{300c}',
        61 => '\u{300d}',
        62 => '\u{300e}',
        63 => '\u{300f}',
        64 | 65 => '"',
        66 => '[',
        67 => ']',
        68 => '<',
        69 => '>',
        70 => '-',
        71 => '\u{2014}',
        112..=121 => (b'0' + (g - 112) as u8) as char,
        122..=147 => (b'a' + (g - 122) as u8) as char,
        148 => '\u{152}',
        149 => '\u{153}',
        150 => '\u{df}',
        151 => '\u{e0}',
        152 => '\u{e1}',
        153 => '\u{e2}',
        154 => '\u{e4}',
        155 => '\u{e8}',
        156 => '\u{e9}',
        157 => '\u{ea}',
        158 => '\u{eb}',
        159 => '\u{ec}',
        160 => '\u{ed}',
        161 => '\u{ee}',
        162 => '\u{ef}',
        163 => '\u{f1}',
        164 => '\u{f2}',
        165 => '\u{f3}',
        166 => '\u{f4}',
        167 => '\u{f6}',
        168 => '\u{f9}',
        169 => '\u{fa}',
        170 => '\u{fb}',
        171 => '\u{fc}',
        172 => '\u{b0}',
        173 => '\u{2014}',
        174 => '\u{bb}',
        175 => '\u{ab}',
        176 => '\u{c0}',
        177 => '\u{c1}',
        178 => '\u{c2}',
        179 => '\u{c4}',
        180 => '\u{c8}',
        181 => '\u{c9}',
        182 => '\u{ca}',
        183 => '\u{cb}',
        184 => '\u{cc}',
        185 => '\u{cd}',
        186 => '\u{ce}',
        187 => '\u{cf}',
        188 => '\u{d1}',
        189 => '\u{d2}',
        190 => '\u{d3}',
        191 => '\u{d4}',
        192 => '\u{d6}',
        193 => '\u{d9}',
        194 => '\u{da}',
        195 => '\u{db}',
        196 => '\u{dc}',
        197 => '\u{a1}',
        198 => '\u{bf}',
        199 => '\u{c7}',
        200 => '\u{e7}',
        201 | 202 => '"',
        203 | 204 => '\'',
        207 => '\u{2605}',
        208 => '\u{2606}',
        209 => '\u{25a0}',
        210 => '\u{25a1}',
        211 => '\u{25b2}',
        212 => '\u{25b3}',
        213 => '\u{25cf}',
        214 => '\u{25cb}',
        215 => '\u{266a}',
        216 => '\u{266b}',
        217 => '\u{2192}',
        218 => '\u{2190}',
        219 => '\u{2191}',
        220 => '\u{2193}',
        221 => '\u{b7}',
        222 => '\u{2665}',
        224..=249 => (b'A' + (g - 224) as u8) as char,
        _ => return None,
    })
}

/// Readable text of a token run: letters and signs as characters, icons as `[icon N]`, glyphs the
/// map does not know as `<gNN>`. Commands other than space and newline are left out.
pub fn to_text(t: &[Token]) -> String {
    let mut s = String::new();
    for k in t {
        match k {
            Token::Glyph(g) => match glyph_char(*g) {
                Some(c) => s.push(c),
                None => s.push_str(&format!("<g{g}>")),
            },
            Token::Space => s.push(' '),
            Token::Newline => s.push('\n'),
            Token::Icon(i) => s.push_str(&format!("[icon {i}]")),
            Token::Variable => s.push_str("{}"),
            _ => {}
        }
    }
    s
}

/// The `sys` message table: u32 version, u32 count, then count x {u32 id, u32 offset}.
#[derive(Debug, Clone)]
pub struct Messages {
    data: Vec<u8>,
    index: HashMap<u32, usize>,
}

impl Messages {
    /// Parses the BAR (`msg/<lang>/sys.bar`) whose `sys` entry holds the table.
    pub fn from_bar(bar: &[u8]) -> Option<Messages> {
        let entries = kh2_bar::parse(bar).ok()?;
        let sys = entries.iter().find(|e| e.tag == "sys" && e.kind == 2)?.data;
        Messages::from_entry(sys)
    }

    pub fn from_entry(sys: &[u8]) -> Option<Messages> {
        let n = u32::from_le_bytes(sys.get(4..8)?.try_into().ok()?) as usize;
        let mut index = HashMap::with_capacity(n);
        for i in 0..n {
            let p = 8 + 8 * i;
            let id = u32::from_le_bytes(sys.get(p..p + 4)?.try_into().ok()?);
            let off = u32::from_le_bytes(sys.get(p + 4..p + 8)?.try_into().ok()?) as usize;
            if off > sys.len() {
                return None;
            }
            index.entry(id).or_insert(off);
        }
        Some(Messages { data: sys.to_vec(), index })
    }

    pub fn len(&self) -> usize {
        self.index.len()
    }

    pub fn is_empty(&self) -> bool {
        self.index.is_empty()
    }

    pub fn ids(&self) -> impl Iterator<Item = u32> + '_ {
        self.index.keys().copied()
    }

    /// Message bytes for `id`. Table fields that name text (`Cmd.MessageId`, `Item.Name`) carry
    /// bit 15 set; the id in this file is the low 15 bits, which is what is looked up here.
    pub fn bytes(&self, id: u32) -> Option<&[u8]> {
        self.data.get(*self.index.get(&(id & 0x7fff))?..)
    }

    pub fn tokens(&self, id: u32) -> Option<Vec<Token>> {
        self.bytes(id).map(tokens)
    }

    /// Readable text of message `id`.
    pub fn text(&self, id: u32) -> Option<String> {
        self.tokens(id).map(|t| to_text(&t))
    }
}

/// The font-style colour table (`ftst` in `03system.bin`): u32 version, u32 count, then count
/// records of 0x50 bytes: a u32 id and one RGBA word per world.
#[derive(Debug, Clone, Default)]
pub struct ColorTable {
    rows: Vec<(u32, Vec<[u8; 4]>)>,
}

impl ColorTable {
    pub fn parse(b: &[u8]) -> Option<ColorTable> {
        let n = u32::from_le_bytes(b.get(4..8)?.try_into().ok()?) as usize;
        let mut rows = Vec::new();
        for i in 0..n {
            let r = b.get(8 + i * 0x50..8 + (i + 1) * 0x50)?;
            let id = u32::from_le_bytes(r[..4].try_into().ok()?);
            rows.push((id, r[4..].chunks(4).map(|c| [c[0], c[1], c[2], c[3]]).collect()));
        }
        Some(ColorTable { rows })
    }

    /// Colour `id` for world index `world` (an index of `WORLDS`).
    pub fn color(&self, id: u32, world: usize) -> Option<[u8; 4]> {
        self.rows.iter().find(|r| r.0 == id).and_then(|r| r.1.get(world).copied())
    }
}

/// What a laid-out quad draws.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Kind {
    /// A glyph in the atlas page: (atlas page 0 or 1).
    Glyph(u8),
    /// An icon of the icon atlas.
    Icon,
}

/// One rectangle of text: position in pixels, texel rectangle, colour (0x80 = 1.0).
#[derive(Debug, Clone, Copy)]
pub struct GlyphQuad {
    pub kind: Kind,
    /// Left, top, right, bottom on screen.
    pub pos: [f32; 4],
    /// Left, top, right, bottom in the atlas, in texels.
    pub uv: [f32; 4],
    pub color: [u8; 4],
}

/// How to lay a message out.
#[derive(Debug, Clone)]
pub struct Style {
    /// Font id 0 to 5.
    pub font: usize,
    /// Nominal glyph size in pixels (`None` = the font's own size).
    pub size: Option<f32>,
    /// Default colour (0x80 = 1.0 per channel).
    pub color: [u8; 4],
    /// Alignment: 0 left, 1 right, 2 centre, 3 centre both ways, 4 centre x and bottom, 5 bottom,
    /// applied to the message block (the game's table at 0x37b5e4).
    pub align: u8,
    /// Text substituted for `Token::Variable`.
    pub variable: Vec<Token>,
    /// Colour column of the `ftst` table.
    pub world: usize,
}

impl Default for Style {
    fn default() -> Style {
        Style { font: 0, size: None, color: [0x80; 4], align: 0, variable: Vec::new(), world: 0 }
    }
}

/// Result of a layout.
#[derive(Debug, Clone, Default)]
pub struct Layout {
    pub quads: Vec<GlyphQuad>,
    /// Width of the widest line and total height, in pixels.
    pub size: [f32; 2],
}

/// Alpha of the shadow the game draws under shadowed fonts: black with alpha 0x60 (0x80 = 1.0),
/// scaled by the text colour's alpha (0x1a8de0 sets 0x60000000, 0x1a8ce0 scales it).
pub const SHADOW_ALPHA: u32 = 0x60;

/// Rounds the way the game does: (int)(x + 0.5).
fn round(x: f32) -> f32 {
    (x + 0.5).floor()
}

struct State {
    s: f32,
    x: f32,
    gap: f32,
    color: [u8; 4],
}

/// Lays a message out with its top-left at `origin` (pixels; the first line's top is `origin[1]`).
pub fn layout(set: &FontSet, colors: Option<&ColorTable>, msg: &[Token], style: &Style, origin: [f32; 2]) -> Layout {
    let font = Font::get(style.font);
    let base_s = style.size.map(|v| v / font.size as f32).unwrap_or(1.0);
    let mut st = State { s: base_s, x: 1.0, gap: 0.0, color: style.color };
    // Expand the variable once.
    let mut flat: Vec<&Token> = Vec::with_capacity(msg.len());
    for t in msg {
        match t {
            Token::Variable => flat.extend(style.variable.iter()),
            Token::End => break,
            t => flat.push(t),
        }
    }
    // Split into lines (a newline ends a line), each laid out from the state the previous one left.
    // Each line: its quads, width and height.
    let mut lines: Vec<(Vec<GlyphQuad>, f32, f32)> = Vec::new();
    let mut cur: Vec<GlyphQuad> = Vec::new();
    let (mut x, mut line_h) = (0.0f32, font.size as f32 * st.s);
    let mut last_glyph: Option<u16> = None;
    for t in &flat {
        match t {
            Token::Glyph(g) => {
                let mut g = *g;
                let mut shift = 0.0;
                if g == 0x46 && last_glyph == Some(0x46) {
                    // Two of the long dash in a row overlap: the second is the 0x47 variant, drawn
                    // two pixels (scaled) to the left.
                    g = 0x47;
                    shift = round(2.0 * st.s * st.x);
                    last_glyph = Some(0x46);
                } else {
                    last_glyph = Some(g);
                }
                let adv = set.width(&font, g as usize) as f32;
                line_h = line_h.max(font.size as f32 * st.s);
                if let Some((page, [u0, v0, u1, v1])) = font.cell(g as usize) {
                    let w = font.cell_w as f32 * st.s * st.x;
                    cur.push(GlyphQuad {
                        kind: Kind::Glyph(page as u8),
                        pos: [x - shift, 0.0, x - shift + w, font.cell_h as f32 * st.s],
                        uv: [u0 as f32, v0 as f32, u1 as f32, v1 as f32],
                        color: st.color,
                    });
                }
                x += round(adv * st.s * st.x);
            }
            Token::Space => {
                x += round((font.space_basis / 3) as f32 * st.s * st.x);
            }
            Token::Newline => {
                lines.push((std::mem::take(&mut cur), x, line_h));
                x = 0.0;
                line_h = font.size as f32 * st.s;
                last_glyph = None;
            }
            Token::Reset => {
                st.color = style.color;
                st.s = base_s;
                st.x = 1.0;
            }
            Token::Palette(id) => {
                if let Some(c) = colors.and_then(|t| t.color(*id as u32, style.world)) {
                    st.color = c;
                }
            }
            Token::Rgba(c) => st.color = *c,
            Token::Icon(i) => {
                let [u0, v0, u1, v1] = set.icon_cell(*i as usize);
                cur.push(GlyphQuad {
                    kind: Kind::Icon,
                    // The icon spans 20 pixels above the baseline to 4 below it (y is relative to the
                    // baseline until the line is placed).
                    pos: [x, -20.0, x + ICON_CELL as f32, 4.0],
                    uv: [u0 as f32, v0 as f32, u1 as f32, v1 as f32],
                    color: st.color,
                });
                x += set.icon_width(*i as usize) as f32;
                line_h = line_h.max(round(font.size as f32 * st.s));
            }
            Token::Size(v) => st.s = *v as f32 / font.size as f32,
            Token::Width(v) => st.x = *v as f32 / 100.0,
            Token::Gap(v) => st.gap = *v as f32,
            _ => {}
        }
    }
    if !cur.is_empty() || lines.is_empty() {
        lines.push((cur, x, line_h));
    }
    // Place the lines: the first baseline is the top plus its height; each next baseline is the
    // previous plus the line gap, the extra gap and that line's height.
    let mut out = Layout::default();
    let mut baseline = 0.0;
    let mut total_h = 0.0;
    let max_w = lines.iter().map(|l| l.1).fold(0.0, f32::max);
    for (i, (line, width, height)) in lines.iter().enumerate() {
        baseline += if i == 0 { *height } else { font.line_gap as f32 + st.gap + *height };
        // Alignment within the block (per line, the game aligns each line on its own width).
        let dx = match style.align {
            1 => -*width,
            2..=4 => -(*width / 2.0).floor(),
            _ => 0.0,
        };
        for q in line {
            let mut q = *q;
            let (y0, y1) = match q.kind {
                Kind::Icon => (baseline + q.pos[1], baseline + q.pos[3]),
                Kind::Glyph(_) => (baseline - (q.pos[3] - q.pos[1]), baseline),
            };
            q.pos = [origin[0] + dx + q.pos[0], origin[1] + y0, origin[0] + dx + q.pos[2], origin[1] + y1];
            out.quads.push(q);
        }
        total_h = baseline;
    }
    let dy = match style.align {
        3 => -(total_h / 2.0).floor(),
        4 | 5 => -total_h,
        _ => 0.0,
    };
    if dy != 0.0 {
        for q in &mut out.quads {
            q.pos[1] += dy;
            q.pos[3] += dy;
        }
    }
    out.size = [max_w, total_h];
    out
}

/// Draws the shadow or outline the font's mode asks for: returns copies of the quads moved by the
/// game's offsets and recoloured black with alpha `SHADOW_ALPHA` (scaled by the colour's alpha),
/// to be drawn before the real quads. Mode 1 is one copy 2 pixels down and right; mode 2 four
/// copies 2 pixels left, down, right, up.
pub fn shadows(quads: &[GlyphQuad], mode: u8) -> Vec<GlyphQuad> {
    let offs: &[[f32; 2]] = match mode {
        1 => &[[2.0, 2.0]],
        2 => &[[-2.0, 0.0], [0.0, 2.0], [2.0, 0.0], [0.0, -2.0]],
        _ => &[],
    };
    let mut out = Vec::new();
    for q in quads {
        if matches!(q.kind, Kind::Icon) {
            continue;
        }
        for o in offs {
            let mut s = *q;
            s.pos = [q.pos[0] + o[0], q.pos[1] + o[1], q.pos[2] + o[0], q.pos[3] + o[1]];
            s.color = [0, 0, 0, ((SHADOW_ALPHA * q.color[3] as u32) >> 7).min(255) as u8];
            out.push(s);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_commands_by_their_lengths() {
        // "Hi", a 5-byte position command whose arguments contain zero bytes, then 'A', end.
        let b = [0x35, 0xa2, 0x11, 0x1e, 0x00, 0x1c, 0x00, 0x2e, 0x00];
        let t = tokens(&b);
        assert_eq!(t.len(), 5);
        assert_eq!(t[2], Token::Command(0x11, vec![0x1e, 0, 0x1c, 0]));
        assert_eq!(to_text(&t), "HiA");
    }

    #[test]
    fn two_byte_glyph_codes() {
        assert_eq!(tokens(&[0x19, 0x04, 0x00]), vec![Token::Glyph(0xe4), Token::End]);
        assert_eq!(tokens(&[0x1a, 0x00]), vec![Token::Glyph(0x1e0)]);
    }

    #[test]
    fn space_newline_and_icons() {
        let t = tokens(&[0x2e, 0x01, 0x2f, 0x02, 0x09, 0x0a, 0x00]);
        assert_eq!(t, vec![Token::Glyph(14), Token::Space, Token::Glyph(15), Token::Newline, Token::Icon(10), Token::End]);
        assert_eq!(to_text(&t), "A B\n[icon 10]");
    }

    #[test]
    fn indexes_a_synthetic_table() {
        let mut e = Vec::new();
        e.extend_from_slice(&1u32.to_le_bytes());
        e.extend_from_slice(&2u32.to_le_bytes());
        for (id, off) in [(0x10u32, 24u32), (0x11, 27)] {
            e.extend_from_slice(&id.to_le_bytes());
            e.extend_from_slice(&off.to_le_bytes());
        }
        e.extend_from_slice(&[0x2e, 0x2f, 0x00, 0x30, 0x00]);
        let m = Messages::from_entry(&e).unwrap();
        assert_eq!(m.len(), 2);
        assert_eq!(m.text(0x8010).as_deref(), Some("AB"));
        assert_eq!(m.text(0x11).as_deref(), Some("C"));
        assert!(m.text(0x12).is_none());
    }

    #[test]
    fn shadow_alpha_follows_the_colour() {
        let q = GlyphQuad { kind: Kind::Glyph(0), pos: [0.0, 0.0, 10.0, 10.0], uv: [0.0; 4], color: [0x80, 0x80, 0x80, 0x40] };
        let s = shadows(&[q], 1);
        assert_eq!(s.len(), 1);
        assert_eq!(s[0].color, [0, 0, 0, 0x30]);
        assert_eq!(shadows(&[q], 2).len(), 4);
        assert!(shadows(&[q], 0).is_empty());
    }
}
