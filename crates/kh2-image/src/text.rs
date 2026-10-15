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
    /// 0x0e: skew of the glyph quads in half pixels (the draw of font mode 4 shifts the top edge
    /// right and the bottom edge left by it; the other modes ignore it). 0x1a8210 case 0xe stores
    /// `byte << 3` in 1/16 pixel units, 0x1a9818 applies it.
    Skew(u8),
    /// 0x11: set the pen: x and y in pixels from the message origin (two i16; 0x1a8210 case 0x11).
    Position(i16, i16),
    /// 0x14: take this much off the typewriter budget (a pause of so many units, u16).
    Wait(u16),
    /// 0x15: a 32 pixel slot the owning window fills (an item or button icon, callback +0x18 with
    /// the slot's index and this value); the pen advances 32 pixels.
    Slot(u16),
    /// 0x16: a 32 pixel button icon drawn by the owning window (callback +0x30 with this value).
    Button(u8),
    /// 0x17: typewriter: `cost` units of budget per glyph or icon, and the budget span over which
    /// the newest glyphs fade in (read at construction by 0x1a8978 into the message object +0x5c
    /// and +0x4c).
    Typewriter { cost: u8, fade: u8 },
    /// A command forwarded to the window that owns the message and ignored by the layout: 0x05
    /// (three u16, callback +0x1c, scanned when a page ends, 0x1a8b48), 0x06 (callback +0x24),
    /// 0x08 (callback +0x28), 0x0f (callback +0x2c), 0x12 (0x197b70 with a u16), 0x13 (two u16
    /// stored at the callback's +4 and +8).
    Callback(u8, Vec<u8>),
    /// Any other command, kept so a message can be shown or re-encoded.
    Command(u8, Vec<u8>),
    /// 0x00 or 0x10.
    End,
}

/// Pixels the pen advances for a slot or button (0x1a8210 cases 0x15 and 0x16 add 0x200 GS units).
pub const SLOT_ADVANCE: f32 = 32.0;

fn u16_at(a: &[u8], k: usize) -> u16 {
    u16::from_le_bytes([a.get(k).copied().unwrap_or(0), a.get(k + 1).copied().unwrap_or(0)])
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
            0xe => out.push(Token::Skew(arg(0))),
            0x11 => out.push(Token::Position(u16_at(a, 0) as i16, u16_at(a, 2) as i16)),
            0x14 => out.push(Token::Wait(u16_at(a, 0))),
            0x15 => out.push(Token::Slot(u16_at(a, 0))),
            0x16 => out.push(Token::Button(arg(0))),
            0x17 => out.push(Token::Typewriter { cost: arg(0), fade: arg(1) }),
            0x05 | 0x06 | 0x08 | 0x0f | 0x12 | 0x13 => out.push(Token::Callback(c, a.to_vec())),
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
            Token::Slot(v) => s.push_str(&format!("[slot {v}]")),
            Token::Button(v) => s.push_str(&format!("[button {v}]")),
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
    /// Parses a message BAR: `msg/<lang>/sys.bar` (entry `sys`) or a world's `msg/<lang>/<world>.bar`.
    /// The game binds the first type-2 entry whatever its tag (0x190b70 calls 0x1684c0 with type 2
    /// and no tag); `sys` is preferred here when present.
    pub fn from_bar(bar: &[u8]) -> Option<Messages> {
        let entries = kh2_bar::parse(bar).ok()?;
        let sys = entries.iter().find(|e| e.tag == "sys" && e.kind == 2).or_else(|| entries.iter().find(|e| e.kind == 2))?.data;
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

/// The place-name table `msg/<lang>/place.bin` (path at 0x37b3f0, loaded by 0x19e088): a BAR whose
/// type-2 entries are tagged with a world code (`tt`, `hb`, ...) and hold one 4-byte record per
/// area: a u16 message id and a u16 offset to extra data (0x19e0c8 indexes it by area, 0x19e178
/// looks the id up in the message tables, 0x19e1b8 follows the offset).
#[derive(Debug, Clone, Default)]
pub struct Places {
    worlds: Vec<(String, Vec<(u16, u16)>)>,
}

impl Places {
    pub fn parse(bar: &[u8]) -> Option<Places> {
        let entries = kh2_bar::parse(bar).ok()?;
        let mut worlds = Vec::new();
        for e in entries.iter().filter(|e| e.kind == 2) {
            let tag = e.tag.trim_end_matches('\0').to_string();
            let records = (0..e.data.len() / 4).map(|i| (u16_at(e.data, i * 4), u16_at(e.data, i * 4 + 2))).collect();
            worlds.push((tag, records));
        }
        Some(Places { worlds })
    }

    /// The message id of area `area` of `world` (a two-letter code), or None when the table has
    /// no entry or the id is 0 (0x19e178 shows nothing for id 0).
    pub fn message(&self, world: &str, area: usize) -> Option<u16> {
        let w = self.worlds.iter().find(|w| w.0 == world)?;
        let (id, _) = *w.1.get(area)?;
        (id != 0).then_some(id)
    }

    pub fn worlds(&self) -> impl Iterator<Item = (&str, usize)> {
        self.worlds.iter().map(|w| (w.0.as_str(), w.1.len()))
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
    /// Pixels the top edge is moved right and the bottom edge left (font mode 4 with command
    /// 0x0e; 0 for everything else).
    pub skew: f32,
}

/// How to lay a message out.
#[derive(Debug, Clone)]
pub struct Style {
    /// Font id 0 to 5.
    pub font: usize,
    /// Nominal glyph size in pixels (`None` = the font's own size).
    pub size: Option<f32>,
    /// Default colour: the vertex colour as the game's product `texel * colour >> 7` takes it (so 0xff is
    /// full brightness, 0x80 half). The default is the window text colour of the font-style table (`ftst`,
    /// 0x1a7df8: 0xe5e5e5); alpha 0x80 = opaque.
    pub color: [u8; 4],
    /// Alignment: 0 left, 1 right, 2 centre, 3 centre both ways, 4 centre x and bottom, 5 bottom,
    /// applied to the message block (the game's table at 0x37b5e4).
    pub align: u8,
    /// Text substituted for `Token::Variable`.
    pub variable: Vec<Token>,
    /// Colour column of the `ftst` table.
    pub world: usize,
    /// Draws of this message so far: the typewriter budget. The game keeps a float at +0x1c of the
    /// message object that every draw (0x1a80d8) raises by 1.0 before the layout starts with it; a
    /// glyph or icon costs the message's `Typewriter.cost`, a `Wait` its value, and the layout stops
    /// when the budget goes negative. Without a `Typewriter` command the cost is 0 and nothing is cut.
    pub time: f32,
}

impl Default for Style {
    fn default() -> Style {
        Style { font: 0, size: None, color: [0xe5, 0xe5, 0xe5, 0x80], align: 0, variable: Vec::new(), world: 0, time: 0.0 }
    }
}

/// Result of a layout.
#[derive(Debug, Clone, Default)]
pub struct Layout {
    pub quads: Vec<GlyphQuad>,
    /// Width of the widest line and total height, in pixels.
    pub size: [f32; 2],
    /// False when the typewriter budget ran out before the end of the text.
    pub complete: bool,
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
    /// Skew in pixels (command 0x0e, half a pixel per unit).
    skew: f32,
}

/// A run of quads placed on one baseline: a line, or the part of a line after a `Position`.
#[derive(Default)]
struct Line {
    quads: Vec<GlyphQuad>,
    width: f32,
    height: f32,
    /// The pen y of a `Position` (pixels from the origin): the baseline is `y - height` and no line
    /// gap is added before it (0x1a8210 case 0x11 sets the state's y to `origin + y - line height`).
    abs: Option<f32>,
    /// The extra gap (command 0x0c) in force when the line ended.
    gap_after: f32,
}

/// Lays a message out with its top-left at `origin` (pixels; the first line's top is `origin[1]`).
pub fn layout(set: &FontSet, colors: Option<&ColorTable>, msg: &[Token], style: &Style, origin: [f32; 2]) -> Layout {
    let font = Font::get(style.font);
    let base_s = style.size.map(|v| v / font.size as f32).unwrap_or(1.0);
    let mut st = State { s: base_s, x: 1.0, gap: 0.0, color: style.color, skew: 0.0 };
    // The typewriter command is read when the message object is built (0x1a8978), so it applies
    // from the first glyph wherever it sits in the text.
    let (mut cost, mut fade) = (0.0f32, 0.0f32);
    for t in msg {
        if let Token::Typewriter { cost: c, fade: f } = t {
            cost = *c as f32;
            fade = *f as f32;
        }
    }
    // The draw raises the message's counter by one before it starts (0x1a80d8).
    let mut budget = style.time + 1.0;
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
    let mut lines: Vec<Line> = Vec::new();
    let mut cur = Line::default();
    let (mut x, mut line_h) = (0.0f32, font.size as f32 * st.s);
    let mut last_glyph: Option<u16> = None;
    let mut complete = true;
    // The colour of a glyph while it fades in: the budget over the fade span, as the alpha (0x1a8698).
    let faded = |color: [u8; 4], budget: f32| {
        let mut c = color;
        if fade > 0.0 && budget / fade < 1.0 {
            c[3] = (budget / fade * 128.0).max(0.0) as u8;
        }
        c
    };
    for t in &flat {
        // The budget is tested before every token; once it is negative the rest is not drawn.
        if budget < 0.0 {
            complete = false;
            break;
        }
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
                    cur.quads.push(GlyphQuad {
                        kind: Kind::Glyph(page as u8),
                        pos: [x - shift, 0.0, x - shift + w, font.cell_h as f32 * st.s],
                        uv: [u0 as f32, v0 as f32, u1 as f32, v1 as f32],
                        color: faded(st.color, budget),
                        // Only the mode 4 draw (0x1a9818) takes the skew; 0x1a95a0 has no such input.
                        skew: if font.mode == 4 { st.skew } else { 0.0 },
                    });
                }
                x += round(adv * st.s * st.x);
                budget -= cost;
            }
            Token::Space => {
                x += round((font.space_basis / 3) as f32 * st.s * st.x);
            }
            Token::Newline => {
                let mut line = std::mem::take(&mut cur);
                line.width = x;
                line.height = line_h;
                line.gap_after = st.gap;
                lines.push(line);
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
                cur.quads.push(GlyphQuad {
                    kind: Kind::Icon,
                    // The icon spans 20 pixels above the baseline to 4 below it (y is relative to the
                    // baseline until the line is placed).
                    pos: [x, -20.0, x + ICON_CELL as f32, 4.0],
                    uv: [u0 as f32, v0 as f32, u1 as f32, v1 as f32],
                    color: st.color,
                    skew: 0.0,
                });
                x += set.icon_width(*i as usize) as f32;
                line_h = line_h.max(round(font.size as f32 * st.s));
                budget -= cost;
            }
            Token::Size(v) => st.s = *v as f32 / font.size as f32,
            Token::Width(v) => st.x = *v as f32 / 100.0,
            Token::Gap(v) => st.gap = *v as f32,
            Token::Skew(v) => st.skew = *v as f32 * 0.5,
            Token::Position(dx, dy) => {
                // What was drawn on this line keeps its place; the pen moves to (dx, dy) from the
                // origin and the rest of the line sits on the baseline `dy - line height`.
                if !cur.quads.is_empty() {
                    let mut seg = std::mem::take(&mut cur);
                    seg.width = x;
                    seg.height = line_h;
                    lines.push(seg);
                }
                cur.abs = Some(*dy as f32);
                x = *dx as f32;
                last_glyph = None;
            }
            Token::Wait(n) => budget -= *n as f32,
            Token::Slot(_) | Token::Button(_) => {
                // The owning window draws the icon; the pen moves 32 pixels (0x1a8210, and the
                // measuring pass 0x191108 adds the same).
                x += SLOT_ADVANCE;
                line_h = line_h.max(round(font.size as f32 * st.s));
            }
            _ => {}
        }
    }
    if !cur.quads.is_empty() || lines.is_empty() {
        cur.width = x;
        cur.height = line_h;
        lines.push(cur);
    }
    // Place the lines: the first baseline is the top plus its height; each next baseline is the
    // previous plus the line gap, the extra gap and that line's height; a positioned segment's
    // baseline is its pen y less its height.
    let mut out = Layout { complete, ..Layout::default() };
    let mut baseline = 0.0;
    let mut total_h = 0.0f32;
    let max_w = lines.iter().map(|l| l.width).fold(0.0, f32::max);
    for (i, line) in lines.iter().enumerate() {
        baseline = match line.abs {
            Some(y) => y - line.height,
            None if i == 0 => line.height,
            None => baseline + font.line_gap as f32 + lines[i - 1].gap_after + line.height,
        };
        // Alignment within the block (per line, the game aligns each line on its own width).
        let dx = match style.align {
            1 => -line.width,
            2..=4 => -(line.width / 2.0).floor(),
            _ => 0.0,
        };
        for q in &line.quads {
            let mut q = *q;
            let (y0, y1) = match q.kind {
                Kind::Icon => (baseline + q.pos[1], baseline + q.pos[3]),
                Kind::Glyph(_) => (baseline - (q.pos[3] - q.pos[1]), baseline),
            };
            q.pos = [origin[0] + dx + q.pos[0], origin[1] + y0, origin[0] + dx + q.pos[2], origin[1] + y1];
            out.quads.push(q);
        }
        total_h = total_h.max(baseline);
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
/// game's offsets and recoloured black with alpha `SHADOW_ALPHA` scaled by `alpha`, the alpha of
/// the colour the message was drawn with (0x1a80d8 sets the shadow colour once from it through
/// 0x1a8ce0, so a glyph's own colour commands and the typewriter fade do not change it), to be
/// drawn before the real quads. Mode 1 is one copy 2 pixels down and right; mode 2 four copies 2
/// pixels left, down, right, up; both are drawn without the skew (0x1a8698 uses the plain draw).
pub fn shadows(quads: &[GlyphQuad], mode: u8, alpha: u8) -> Vec<GlyphQuad> {
    let offs: &[[f32; 2]] = match mode {
        1 => &[[2.0, 2.0]],
        2 => &[[-2.0, 0.0], [0.0, 2.0], [2.0, 0.0], [0.0, -2.0]],
        _ => &[],
    };
    let a = ((SHADOW_ALPHA * alpha as u32) >> 7).min(255) as u8;
    let mut out = Vec::new();
    for q in quads {
        if matches!(q.kind, Kind::Icon) {
            continue;
        }
        for o in offs {
            let mut s = *q;
            s.pos = [q.pos[0] + o[0], q.pos[1] + o[1], q.pos[2] + o[0], q.pos[3] + o[1]];
            s.color = [0, 0, 0, a];
            s.skew = 0.0;
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
        assert_eq!(t[2], Token::Position(0x1e, 0x1c));
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
        let q = GlyphQuad { kind: Kind::Glyph(0), pos: [0.0, 0.0, 10.0, 10.0], uv: [0.0; 4], color: [0x80, 0x80, 0x80, 0x10], skew: 3.0 };
        let s = shadows(&[q], 1, 0x40);
        assert_eq!(s.len(), 1);
        // 0x60 x 0x40 >> 7 from the message colour, not the glyph's own 0x10; shadows are not skewed.
        assert_eq!(s[0].color, [0, 0, 0, 0x30]);
        assert_eq!(s[0].skew, 0.0);
        assert_eq!(shadows(&[q], 2, 0x80).len(), 4);
        assert!(shadows(&[q], 0, 0x80).is_empty());
    }

    #[test]
    fn phase_three_commands_tokenise() {
        // skew 4, position (-10, 300), wait 0x120, slot 7, button 3, typewriter (2, 6), callback 0x13.
        let b = [0x0e, 4, 0x11, 0xf6, 0xff, 0x2c, 0x01, 0x14, 0x20, 0x01, 0x15, 7, 0, 0x16, 3, 0x17, 2, 6, 0x13, 1, 0, 2, 0, 0x00];
        let t = tokens(&b);
        assert_eq!(
            t,
            vec![
                Token::Skew(4),
                Token::Position(-10, 300),
                Token::Wait(0x120),
                Token::Slot(7),
                Token::Button(3),
                Token::Typewriter { cost: 2, fade: 6 },
                Token::Callback(0x13, vec![1, 0, 2, 0]),
                Token::End
            ]
        );
        assert_eq!(to_text(&t), "[slot 7][button 3]");
    }

    /// A font set with every width 10 and empty atlases, for layout rules that need no pixels.
    fn fake_fonts() -> FontSet {
        FontSet::synthetic(10)
    }

    #[test]
    fn typewriter_cuts_and_fades() {
        let set = fake_fonts();
        // Cost 1 per glyph, fade over 2 units: "ABCDE".
        let msg: Vec<Token> = std::iter::once(Token::Typewriter { cost: 1, fade: 2 }).chain((14..19).map(Token::Glyph)).collect();
        let at = |time: f32| layout(&set, None, &msg, &Style { time, ..Style::default() }, [0.0, 0.0]);
        // Draw 0: budget 1 -> A (budget 1, alpha 1/2 x 128), B (budget 0, alpha 0), then C is refused.
        let l = at(0.0);
        assert_eq!(l.quads.len(), 2);
        assert_eq!(l.quads[0].color[3], 64);
        assert_eq!(l.quads[1].color[3], 0);
        assert!(!l.complete);
        // Draw 5: budget 6 -> all five glyphs, the first three at full alpha (budget / fade >= 1).
        let l = at(5.0);
        assert_eq!(l.quads.len(), 5);
        assert_eq!(l.quads[2].color[3], 0x80);
        // The last glyph is drawn with budget 2: 2 / 2 is not below 1, so it is not faded either.
        assert_eq!(l.quads[4].color[3], 0x80);
        assert!(l.complete);
        // A wait takes from the budget: with 100 units nothing after it is drawn at time 0.
        let msg2 = [Token::Glyph(14), Token::Wait(100), Token::Glyph(15)];
        assert_eq!(layout(&set, None, &msg2, &Style::default(), [0.0, 0.0]).quads.len(), 1);
        // Without a typewriter command nothing is cut whatever the time.
        let msg3: Vec<Token> = (14..19).map(Token::Glyph).collect();
        assert_eq!(layout(&set, None, &msg3, &Style::default(), [0.0, 0.0]).quads.len(), 5);
    }

    #[test]
    fn position_slots_and_skew() {
        let set = fake_fonts();
        let font = Font::get(4);
        // Font 4 (24x32 cells, size 24): "A", pen to (100, 200), "B" skewed by 3 units (1.5 px), a slot, "C".
        let msg = [Token::Glyph(14), Token::Position(100, 200), Token::Skew(3), Token::Glyph(15), Token::Slot(1), Token::Glyph(16)];
        let l = layout(&set, None, &msg, &Style { font: 4, ..Style::default() }, [0.0, 0.0]);
        assert_eq!(l.quads.len(), 3);
        // A sits on the first baseline (24) at x 0 with no skew.
        assert_eq!(l.quads[0].pos[3], 24.0);
        assert_eq!((l.quads[0].pos[0], l.quads[0].skew), (0.0, 0.0));
        // B starts at x 100 on the baseline 200 - 24 (0x1a8210 case 0x11) and is skewed.
        assert_eq!(l.quads[1].pos[0], 100.0);
        assert_eq!(l.quads[1].pos[3], 200.0 - font.size as f32);
        assert_eq!(l.quads[1].skew, 1.5);
        // C is one advance (10) plus a 32 pixel slot further.
        assert_eq!(l.quads[2].pos[0], 100.0 + 10.0 + SLOT_ADVANCE);
        // Skew is only taken by mode 4: the same text in font 1 has none.
        let l1 = layout(&set, None, &msg, &Style { font: 1, ..Style::default() }, [0.0, 0.0]);
        assert_eq!(l1.quads[1].skew, 0.0);
    }
}
