//! Text for the HUD: the game's fonts and system messages (`msg/us/*.bar`), turned into quads
//! that `hud_gfx` draws with the sequence quads. See docs/systems/ui/hud-panels.md ("Text") and `kh2_image::text`.

use std::collections::HashMap;

use bevy::prelude::*;
use kh2_image::font::{Atlas, Font, FontSet};
use kh2_image::seqd::{Quad, Seqd};
use kh2_image::text::{self, ColorTable, GlyphQuad, Kind, Messages, Places, Style, Token};

use super::hud_gfx::SeqFile;
use crate::assets::Disc;

/// Fonts, messages and colours, and the scene files that hold the atlas textures.
pub struct TextSys {
    pub set: FontSet,
    pub messages: Option<Messages>,
    /// The world's own messages (`msg/us/<world>.bar`, 0x190dd0 builds the path; the game binds it
    /// as message table 2 at 0x190e70 and 0x190c40 searches tables 0, 1, 2 in order).
    pub world_messages: Option<Messages>,
    /// The place-name table (`msg/us/place.bin`).
    pub places: Option<Places>,
    pub colors: Option<ColorTable>,
    /// Scene file index by (atlas, palette row).
    files: HashMap<(u8, usize), usize>,
    icon_file: usize,
    /// Colour column of the font-style table (the current world's index in `text::WORLDS`).
    pub world: usize,
    /// Scene file of the message-window frames: the `md_m` sequence of `fontinfo.bar` with the
    /// `md_m` image of the world's message file (0x190b70 loads it into the window texture
    /// 0x1c95710 when the world's file is bound, 0x168ba8). `None` when the world file has no image.
    pub frame_file: Option<usize>,
}

/// A text drawn by a node: the message, how to lay it out, and where it sits from the node's origin.
#[derive(Clone)]
pub struct Label {
    pub tokens: Vec<Token>,
    pub style: Style,
    pub offset: [f32; 2],
}

fn atlas_key(a: Atlas) -> u8 {
    match a {
        Atlas::Sys => 0,
        Atlas::Evt => 1,
    }
}

impl TextSys {
    /// Reads the font and message files from the disc and adds the atlas textures to `files`.
    pub fn load(disc: &Disc, world: &str, images: &mut Assets<Image>, files: &mut Vec<SeqFile>) -> Option<TextSys> {
        let info = disc.read("msg/us/fontinfo.bar")?;
        let set = FontSet::parse(&info, &disc.read("msg/us/fontimage.bar")?)?;
        let messages = disc.read("msg/us/sys.bar").and_then(|b| Messages::from_bar(&b));
        let world_bar = disc.read(&format!("msg/us/{world}.bar"));
        let world_messages = world_bar.as_ref().and_then(|b| Messages::from_bar(b));
        let frame_file = frame_file(&info, world_bar.as_deref(), images, files);
        let places = disc.read("msg/us/place.bin").and_then(|b| Places::parse(&b));
        let colors = disc.read("03system.bin").and_then(|b| {
            let e = kh2_bar::parse(&b).ok()?;
            ColorTable::parse(e.iter().find(|e| e.tag == "ftst")?.data)
        });
        let mut map = HashMap::new();
        let mut add = |img: (usize, usize, Vec<u8>), files: &mut Vec<SeqFile>| {
            let image = kh2_image::imgd::Image { width: img.0 as u32, height: img.1 as u32, rgba: img.2, swizzled: false };
            files.push(SeqFile::new(&image, Seqd::default(), images));
            files.len() - 1
        };
        // The rows the six fonts draw with (page 0).
        for id in 0..6 {
            let f = Font::get(id);
            let key = (atlas_key(f.atlas), f.clut_row());
            map.entry(key).or_insert_with(|| add(set.atlas_rgba_ps2(f.atlas, f.clut_row()), files));
        }
        let icon_file = add(set.icons_rgba_ps2(), files);
        let world = text::WORLDS.iter().position(|w| *w == world).unwrap_or(0);
        Some(TextSys { set, messages, world_messages, places, colors, files: map, icon_file, world, frame_file })
    }

    /// Colour `id` of the font-style table for the current world (0x1a7df8), white if missing. (The atlas texels are the\n    /// palette's own, so 0x80 is a mid grey and white needs 0xff: see docs/systems/ui/hud-panels.md "Text colour".)
    pub fn color(&self, id: u32) -> [u8; 4] {
        self.colors.as_ref().and_then(|c| c.color(id, self.world)).unwrap_or([0xff, 0xff, 0xff, 0x80])
    }

    /// Lays `tokens` out with `style` and tells whether the typewriter has reached the end
    /// (`time` is the label's draw count, as in `quads`).
    pub fn complete(&self, tokens: &[Token], style: &Style, time: f32) -> bool {
        text::layout(&self.set, self.colors.as_ref(), tokens, &Style { time, ..style.clone() }, [0.0, 0.0]).complete
    }

    /// A label for system message `id` (bit 15 of a table field is ignored).
    pub fn message(&self, id: u32, style: Style, offset: [f32; 2]) -> Option<Label> {
        let tokens = self.messages.as_ref()?.tokens(id)?;
        Some(Label { tokens, style: Style { world: self.world, ..style }, offset })
    }

    /// The message lookup of 0x190c40: the system table first, then the world's table; a missing
    /// id falls back to message 0x8adc.
    pub fn lookup(&self, id: u32) -> Option<Vec<Token>> {
        let tables = [self.messages.as_ref(), self.world_messages.as_ref()];
        tables.iter().flatten().find_map(|m| m.tokens(id)).or_else(|| self.messages.as_ref()?.tokens(0x8adc))
    }

    /// The name of area `area` of the world with code `world`, through the place table.
    pub fn place_name(&self, world: &str, area: usize) -> Option<Vec<Token>> {
        let id = self.places.as_ref()?.message(world, area)?;
        self.lookup(id as u32)
    }

    /// The quads of `label` placed at `origin`, shadows first, with the scene file each one uses.
    /// `time` is how many draws the label has had (the typewriter budget, `Style::time`).
    pub fn quads(&self, label: &Label, origin: [f32; 2], fade: f32, time: f32) -> Vec<(usize, Quad)> {
        let font = Font::get(label.style.font);
        let at = [origin[0] + label.offset[0], origin[1] + label.offset[1]];
        let style = Style { time, ..label.style.clone() };
        let l = text::layout(&self.set, self.colors.as_ref(), &label.tokens, &style, at);
        let mut all = text::shadows(&l.quads, font.mode, label.style.color[3]);
        all.extend(l.quads.iter().copied());
        all.iter()
            .filter_map(|q| {
                let file = match q.kind {
                    Kind::Glyph(page) => *self.files.get(&(atlas_key(font.atlas), font.clut_row() + page as usize))?,
                    Kind::Icon => self.icon_file,
                };
                Some((file, to_quad(q, fade)))
            })
            .collect()
    }

    /// Measured size of a label (width of the widest line, height).
    pub fn measure(&self, label: &Label) -> [f32; 2] {
        text::layout(&self.set, self.colors.as_ref(), &label.tokens, &label.style, [0.0, 0.0]).size
    }
}

/// The message-window frame file: `md_m` sequence of `fontinfo.bar`, `md_m` image of the world's
/// message file (the system file's is empty, so there is no fallback to it).
fn frame_file(info: &[u8], world_bar: Option<&[u8]>, images: &mut Assets<Image>, files: &mut Vec<SeqFile>) -> Option<usize> {
    let find = |bar: &[u8], kind: u16| kh2_bar::parse(bar).ok()?.iter().find(|e| e.tag == "md_m" && e.kind == kind && !e.data.is_empty()).map(|e| e.data.to_vec());
    let seq = kh2_image::seqd::parse(&find(info, 25)?)?;
    let img = kh2_image::imgd::decode(&find(world_bar?, 24)?)?;
    files.push(SeqFile::new(&img, seq, images));
    Some(files.len() - 1)
}

fn to_quad(q: &GlyphQuad, fade: f32) -> Quad {
    let [x0, y0, x1, y1] = q.pos;
    let [u0, v0, u1, v1] = q.uv;
    let mut c = q.color;
    c[3] = (c[3] as f32 * fade).min(255.0) as u8;
    // The skewed draw (0x1a9818) moves the top edge right and the bottom edge left.
    let k = q.skew;
    Quad { pos: [[x0 + k, y0], [x1 + k, y0], [x0 - k, y1], [x1 - k, y1]], uv: [[u0, v0], [u1, v0], [u0, v1], [u1, v1]], color: [c; 4], blend: 0, nearest: false }
}

/// `KH2_HUDTEXT=<hex message id>[:font[:size[:x:y]]]` (several separated by `,`): draws system
/// messages at fixed places to check the text engine. Display only.
pub fn demo(mut scene: ResMut<super::hud_gfx::HudScene>, mut done: Local<bool>) {
    use super::hud_gfx::Node;
    if *done || scene.text.is_none() {
        return;
    }
    *done = true;
    let Ok(spec) = std::env::var("KH2_HUDTEXT") else { return };
    let mut y = 40.0;
    for item in spec.split(',') {
        let p: Vec<&str> = item.split(':').collect();
        let Some(id) = p.first().and_then(|v| u32::from_str_radix(v.trim_start_matches("0x"), 16).ok()) else { continue };
        let font = p.get(1).and_then(|v| v.parse().ok()).unwrap_or(1);
        let size = p.get(2).and_then(|v| v.parse::<f32>().ok());
        let x = p.get(3).and_then(|v| v.parse().ok()).unwrap_or(24.0);
        let ypos = p.get(4).and_then(|v| v.parse().ok()).unwrap_or(y);
        let Some(label) = scene.text.as_ref().and_then(|t| t.message(id, Style { font, size, color: [0xe5, 0xe5, 0xe5, 0x80], ..Style::default() }, [0.0, 0.0])) else { continue };
        let h = scene.text.as_ref().map(|t| t.measure(&label)[1]).unwrap_or(20.0);
        let mut n = Node::new(usize::MAX, None);
        n.pos = [x, ypos];
        n.label = Some(label);
        scene.add(n);
        y += h + 8.0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Text colours above 0x80 (the light bar colour 0xb4, the window colour 0xe5, 0xff) reach the
    /// quad unchanged: the atlas texels are the palette's own (`atlas_rgba_ps2`), so the renderer's
    /// `colour / 128` gives the game's `texel * colour >> 7` without any per-caller halving.
    #[test]
    fn quad_keeps_text_colours_above_0x80() {
        for colour in [[0xb4, 0x64, 0x00, 0x80], [0xe5, 0xe5, 0xe5, 0x80], [0xff, 0x01, 0x00, 0x80]] {
            let g = GlyphQuad { kind: Kind::Glyph(0), pos: [0.0, 0.0, 10.0, 10.0], uv: [0.0; 4], color: colour, skew: 0.0 };
            let q = to_quad(&g, 1.0);
            assert_eq!(q.color[0], colour);
            assert_eq!(q.color[3], colour);
        }
    }

    #[test]
    fn fade_scales_only_alpha() {
        let g = GlyphQuad { kind: Kind::Glyph(0), pos: [0.0; 4], uv: [0.0; 4], color: [0xe5, 0xb4, 0x64, 0x80], skew: 0.0 };
        assert_eq!(to_quad(&g, 0.5).color[0], [0xe5, 0xb4, 0x64, 0x40]);
    }
}
