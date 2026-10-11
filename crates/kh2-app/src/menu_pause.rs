//! The PAUSE band: what the pause button shows in combat instead of the camp menu (docs/menus.md 1.8).
//!
//! The picture is `menu/us/pause.2ld` (the file the menu boot `FUN_0024f558` preloads as list index 0, docs/menus.md
//! 1.3): one 64x64 image that holds the letters P A U S E and a few sparkles, and one sequence of 19 animation
//! groups. Read from the file: group 0 is a full-screen black at half alpha (the dim), group 14 the dark band with
//! the thin violet rule at its top and bottom edge, group 15 the five letters that slide in one after the other
//! and then hold (the group stops at its last frame). The groups 2 to 13 and 16 to 18 (white flash, fades, the
//! pill that opens into the band, sparkles) are not played: no code that plays them was found [GAP].
//!
//! The rule that chooses this screen instead of the camp menu was NOT found in the executable [GAP]: the pause
//! button's task `FUN_00138758` opens the camp (`FUN_0024f0b8(0, -1)`) whatever the battle state is, and no
//! caller of the `pause.2ld` buffer outside the menu loader exists in the decompilation (`FUN_0024fa20` has two
//! callers, `FUN_0024f908` and `FUN_002a04f8`, both menu code). The port follows the user's frame of the original
//! (PCSX2): "in combat" is the game's own battle flag (`audio_bgm::BattleFlag`, word 0x1c8a7d8, `FUN_001903d8`)
//! [inferred]. The dim (50 %, group 0) and the layout (band centred on the picture, letters centred in the band)
//! are [inferred] from the file and that frame.

use bevy::prelude::*;
use kh2_image::layd::Layd;

use crate::assets::Disc;
use crate::menu_gfx::{self, MenuScene, Node};
use crate::schedule::Pause;

/// Groups of `pause.2ld` sequence 0 that the band plays.
mod group {
    pub const BAND: usize = 14;
    pub const LETTERS: usize = 15;
}
/// Brightness of the dimmed picture: group 0 is black at alpha 64 of 128.
pub const DIM_BRIGHTNESS: f32 = 0.5;
/// Middle line of the picture in menu units (the 448 line picture sits 16 lines up, `menu_gfx::Y_BIAS`).
const CENTER_Y: f32 = menu_gfx::SCREEN[1] / 2.0 - menu_gfx::Y_BIAS;
/// Group 14's band is centred 32.5 lines below its origin (rows -19 to 84 of the rasterised group).
const BAND_CENTER: f32 = 32.5;

pub struct PausePlugin;

impl Plugin for PausePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Band>().add_systems(Startup, load.after(super::load)).add_systems(Update, tick.before(crate::schedule::Frame::Popups));
    }
}

/// The band's nodes in the menu scene (empty until the file is loaded).
#[derive(Resource, Default)]
pub struct Band {
    nodes: Option<[usize; 2]>,
}

impl Band {
    /// The file was read: the band can be shown.
    pub fn is_ready(&self) -> bool {
        self.nodes.is_some()
    }
}

fn load(disc: Res<Disc>, mut images: ResMut<Assets<Image>>, mut scene: ResMut<MenuScene>, mut band: ResMut<Band>) {
    if scene.texs.len() < 2 {
        return; // the menu itself did not load
    }
    let Some(two) = disc.read("menu/us/pause.2ld").and_then(|b| kh2_image::layd::parse_2ld(&b)) else {
        warn!("pause band: menu/us/pause.2ld is not readable; the combat pause shows nothing");
        return;
    };
    let (Some(image), Layd { seqs, .. }) = (two.images.first(), two.layd) else { return };
    let Some(seq) = seqs.into_iter().next() else { return };
    let tex = scene.texs.len();
    scene.texs.push(menu_gfx::Tex::new(image, &mut images));
    let seq_index = scene.seqs.len();
    scene.seqs.push(seq);
    let mut make = |pos: [f32; 2]| {
        let mut n = Node::new(Some(seq_index));
        n.tex = tex;
        n.pos = pos;
        n.visible = false;
        scene.add(n)
    };
    let nodes = [make([menu_gfx::SCREEN[0] / 2.0, CENTER_Y - BAND_CENTER]), make([menu_gfx::SCREEN[0] / 2.0, CENTER_Y])];
    band.nodes = Some(nodes);
}

/// Shows the band and freezes the simulation (the same frame as the open input).
pub fn open(band: &Band, scene: &mut MenuScene, pause: &mut Pause) -> bool {
    let Some([b, l]) = band.nodes else { return false };
    pause.band = true;
    scene.hidden = false;
    scene.dim = super::dim_alpha(DIM_BRIGHTNESS);
    for (id, g) in [(b, group::BAND), (l, group::LETTERS)] {
        scene.nodes[id].visible = true;
        scene.set_group(id, g);
    }
    true
}

/// Hides the band; the simulation runs again from the next 60 Hz tick.
pub fn close(band: &Band, scene: &mut MenuScene, pause: &mut Pause) {
    pause.band = false;
    scene.hidden = true;
    scene.dim = 0.0;
    for id in band.nodes.into_iter().flatten() {
        scene.nodes[id].visible = false;
    }
}

fn tick(pause: Res<Pause>, mut scene: ResMut<MenuScene>, real: Res<Time<Real>>) {
    if pause.band {
        scene.advance(real.delta_secs());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scene() -> (MenuScene, Band) {
        let mut scene = MenuScene::default();
        for _ in 0..2 {
            let mut n = Node::new(None);
            n.visible = false;
            scene.add(n);
        }
        scene.hidden = true;
        (scene, Band { nodes: Some([0, 1]) })
    }

    #[test]
    fn opening_the_band_freezes_the_simulation_and_closing_releases_it() {
        let (mut scene, band) = scene();
        let mut pause = Pause::default();
        assert!(open(&band, &mut scene, &mut pause));
        assert!(pause.band && pause.frozen(), "the same call that shows the band stops the simulation");
        assert!(!scene.hidden && scene.dim > 0.0 && scene.nodes.iter().all(|n| n.visible));
        close(&band, &mut scene, &mut pause);
        assert!(!pause.frozen());
        assert!(scene.hidden && scene.dim == 0.0 && scene.nodes.iter().all(|n| !n.visible));
    }

    #[test]
    fn without_the_file_the_band_does_not_open() {
        let (mut scene, _) = scene();
        let mut pause = Pause::default();
        assert!(!open(&Band::default(), &mut scene, &mut pause));
        assert!(!pause.frozen());
    }

    #[test]
    fn the_band_dims_to_half() {
        // Group 0 of pause.2ld: black at alpha 64 of 128.
        assert_eq!(DIM_BRIGHTNESS, 0.5);
    }
}
