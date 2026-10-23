//! The area-name card: the name of the area, drawn as text when an area starts (WP-4B3).
//!
//! What the game does (`SLUS_210.05`): the area start-up 0x149bb8 calls 0x15ade8 when the
//! world/area pair differs from the one before (0x105070 compares the two bytes at 0x33e830 with
//! 0x33e848) and `FUN_001b9550` is zero (not traced). 0x15ade8 looks the current place up
//! (0x19e178: the place table `msg/us/place.bin`, then the message tables) and hands the text to
//! the card object at 0x358ba8 through 0x15a5e0 with a timer of 120 (float at 0x379420) and the
//! value 0x14. The card's update (0x15a640, vtable 0x358bb8) counts the timer down by one per
//! frame while no event runs (`FUN_00196418`) and hides itself (0x139778) when it reaches zero, or
//! at once during an event. Its draw (0x15a840) is one call of 0x1a8d18: font 2 (the outline
//! font), the text, x 256, y 192, colour 0x80ffffff, size 0 (the font's own) and alignment 4
//! (centred on x, bottom at y). Verified in code.
//!
//! When the world object carries a picture for the place (`FUN_00149210` / `FUN_00149218` return
//! its image and sequence), 0x15ade8 shows that picture through 0x15a6f0 instead (the object at
//! 0x358ba4, groups 0 and 1 by language); those two functions are not in the decompiled set and
//! the port has no such picture, so only the text form is drawn.

use bevy::prelude::*;
use kh2_image::text::{Style, Token};

use super::hud_gfx::{HudScene, Node};
use super::hud_text::Label;
use crate::assets::CurrentArea;

/// Frames the card stays up (0x379420).
pub const DURATION: f32 = 120.0;
/// Where the text is drawn and how (0x15a840).
pub const POS: [f32; 2] = [256.0, 192.0];
pub const FONT: usize = 2;
pub const ALIGN: u8 = 4;
pub const COLOR: [u8; 4] = [0xff, 0xff, 0xff, 0x80];

/// The card's state: the name shown and the frames left.
#[derive(Resource, Default, Debug)]
pub struct AreaCard {
    pub tokens: Option<Vec<Token>>,
    pub timer: f32,
    /// The area the card was last raised for.
    shown_for: Option<String>,
}

#[derive(Resource, Default)]
pub struct CardNodes {
    node: Option<usize>,
}

/// World code and area number of an area name such as `tt04`.
pub fn split_area(name: &str) -> Option<(&str, usize)> {
    let world = name.get(..2)?;
    let n = name.get(2..)?.parse().ok()?;
    Some((world, n))
}

/// `KH2_HUDCARD=<world>:<area>` shows that place's name instead of the current area's and keeps it
/// up (for screenshots); `KH2_HUDCARD=1` keeps the current area's card up.
fn forced() -> Option<(Option<(String, usize)>, bool)> {
    let v = std::env::var("KH2_HUDCARD").ok()?;
    let p: Vec<&str> = v.split(':').collect();
    match (p.first(), p.get(1).and_then(|n| n.parse().ok())) {
        (Some(w), Some(n)) if w.len() == 2 => Some((Some((w.to_string(), n)), true)),
        _ => Some((None, v != "0")),
    }
}

pub fn update(time: Res<Time>, area: Res<CurrentArea>, mut card: ResMut<AreaCard>, mut nodes: ResMut<CardNodes>, mut scene: ResMut<HudScene>) {
    if scene.text.is_none() {
        return;
    }
    let force = forced();
    // A new area raises the card (the game compares the world/area pair with the previous one).
    if card.shown_for.as_deref() != Some(area.name.as_str()) {
        card.shown_for = Some(area.name.clone());
        let (world, n) = match force.as_ref().and_then(|f| f.0.clone()) {
            Some((w, n)) => (w, n),
            None => match split_area(&area.name) {
                Some((w, n)) => (w.to_string(), n),
                None => return,
            },
        };
        card.tokens = scene.text.as_ref().and_then(|t| t.place_name(&world, n));
        card.timer = if card.tokens.is_some() { DURATION } else { 0.0 };
        match &card.tokens {
            Some(t) => info!("hud: area card for {world}{n}: {:?}", kh2_image::text::to_text(t)),
            None => info!("hud: no place name for {world}{n}"),
        }
        if let Some(node) = nodes.node {
            scene.nodes[node].label = None;
            scene.nodes[node].label_time = 0.0;
        }
    }
    let node = *nodes.node.get_or_insert_with(|| {
        let mut n = Node::new(usize::MAX, None);
        n.pos = POS;
        n.visible = false;
        scene.add(n)
    });
    let hold = force.is_some_and(|f| f.1);
    if card.timer > 0.0 {
        if !hold {
            // One per logic frame (60 Hz); the game decrements by DAT_00356f44 = 1.0 per update.
            card.timer -= time.delta_secs() * 60.0;
        }
        let n = &mut scene.nodes[node];
        if n.label.is_none() {
            n.label = card.tokens.clone().map(|tokens| Label { tokens, style: Style { font: FONT, align: ALIGN, color: COLOR, ..Style::default() }, offset: [0.0, 0.0] });
            n.label_time = 0.0;
        }
        n.visible = n.label.is_some();
    } else {
        scene.nodes[node].visible = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn area_names_split_into_world_and_number() {
        assert_eq!(split_area("tt04"), Some(("tt", 4)));
        assert_eq!(split_area("hb00"), Some(("hb", 0)));
        assert_eq!(split_area("tt"), None);
        assert_eq!(split_area("t"), None);
    }
}
