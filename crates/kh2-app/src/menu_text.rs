//! Text for the pause menu: the game's fonts and system messages turned into quads that
//! `menu_gfx` draws. The HUD has the same engine in `hud_text.rs`; that file and `hud_gfx.rs` are
//! private to the HUD module and being edited by other work, so the menu keeps its own small copy
//! of the loader and of the quad conversion (the layout itself is `kh2_image::text::layout`, shared).

use std::collections::HashMap;

use bevy::prelude::*;
use kh2_image::font::{Atlas, Font, FontSet};
use kh2_image::seqd::Quad;
use kh2_image::text::{self, ColorTable, GlyphQuad, Kind, Messages, Places, Style, Token};

use crate::assets::Disc;
use crate::menu_gfx::Tex;

/// Fonts, messages and colours, and the texture index of each atlas page.
pub struct MenuText {
    set: FontSet,
    pub messages: Option<Messages>,
    places: Option<Places>,
    colors: Option<ColorTable>,
    /// Texture index by (atlas, palette row).
    files: HashMap<(u8, usize), usize>,
    icon_file: usize,
    /// Colour column of the font-style table (the current world's index in `text::WORLDS`).
    pub world: usize,
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

impl MenuText {
    /// Reads the font and message files from the disc and adds the atlas textures to `texs`.
    pub fn load(disc: &Disc, world: &str, images: &mut Assets<Image>, texs: &mut Vec<Tex>) -> Option<MenuText> {
        let info = disc.read("msg/us/fontinfo.bar")?;
        let set = FontSet::parse(&info, &disc.read("msg/us/fontimage.bar")?)?;
        let messages = disc.read("msg/us/sys.bar").and_then(|b| Messages::from_bar(&b));
        let places = disc.read("msg/us/place.bin").and_then(|b| Places::parse(&b));
        let colors = disc.read("03system.bin").and_then(|b| {
            let e = kh2_bar::parse(&b).ok()?;
            ColorTable::parse(e.iter().find(|e| e.tag == "ftst")?.data)
        });
        let mut map = HashMap::new();
        let mut add = |img: (usize, usize, Vec<u8>), texs: &mut Vec<Tex>| {
            let image = kh2_image::imgd::Image { width: img.0 as u32, height: img.1 as u32, rgba: img.2, swizzled: false };
            texs.push(Tex::new(&image, images));
            texs.len() - 1
        };
        for id in 0..6 {
            let f = Font::get(id);
            let key = (atlas_key(f.atlas), f.clut_row());
            map.entry(key).or_insert_with(|| add(set.atlas_rgba(f.atlas, f.clut_row()), texs));
        }
        let icon_file = add(set.icons_rgba(), texs);
        let world = text::WORLDS.iter().position(|w| *w == world).unwrap_or(0);
        Some(MenuText { set, messages, places, colors, files: map, icon_file, world })
    }

    /// A label for system message `id` (bit 15 of a table field is ignored).
    pub fn message(&self, id: u32, style: Style, offset: [f32; 2]) -> Option<Label> {
        let tokens = self.messages.as_ref()?.tokens(id)?;
        Some(Label { tokens, style: Style { world: self.world, ..style }, offset })
    }

    /// A label of literal text (digits, names): the message bytes of a plain string.
    pub fn literal(&self, text: &str, style: Style, offset: [f32; 2]) -> Label {
        Label { tokens: text.chars().filter_map(char_token).collect(), style: Style { world: self.world, ..style }, offset }
    }

    /// The name of area `area` of the world with code `world`, through the place table, then the
    /// system message table.
    pub fn place_name(&self, world: &str, area: usize) -> Option<Vec<Token>> {
        let id = self.places.as_ref()?.message(world, area)?;
        self.messages.as_ref()?.tokens(id as u32)
    }

    /// The quads of `label` placed at `origin`, shadows first, with the texture each one uses.
    pub fn quads(&self, label: &Label, origin: [f32; 2], fade: f32) -> Vec<(usize, Quad)> {
        let font = Font::get(label.style.font);
        let at = [origin[0] + label.offset[0], origin[1] + label.offset[1]];
        let l = text::layout(&self.set, self.colors.as_ref(), &label.tokens, &label.style, at);
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
}

/// The token of a plain character, through the game's own glyph table.
fn char_token(c: char) -> Option<Token> {
    if c == ' ' {
        return Some(Token::Space);
    }
    (0u16..0x200).find(|&g| text::glyph_char(g) == Some(c)).map(Token::Glyph)
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
