//! The PAUSE band: what the pause button shows in combat instead of the camp menu (docs/systems/ui/menus-pause.md 1.8).
//!
//! The picture is `menu/us/pause.2ld` (the file the menu boot `FUN_0024f558` preloads as list index 0, docs/systems/ui/menus-pause.md
//! 1.3): one 64x64 image that holds the letters P A U S E and a few sparkles, and one sequence of 19 animation
//! groups. Read from the file: group 0 is a full-screen black at half alpha (the dim), group 15 is the whole
//! band: the dark gradient body (rows -15 to 15), the thin violet rule at its top and bottom edge (rows -17/-15 and
//! 15/17) and the five letters that slide in one after the other and then hold. Group 14 is a second, taller
//! band (rows -53 to 53, rules at +-52) that is NOT the PAUSE screen: drawing it too put two extra rules far
//! above and below the real band (the user's 2026-10-12 frame). The groups 1 to 14 and 16 to 18 (opaque black,
//! white flash, fades, the pill that opens, sparkles) are not played: no code that plays them was found [GAP].
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
    /// The band with its two violet rules and the five sliding letters, all in one group (`dump_pause_groups`).
    pub const BAND: usize = 15;
}
/// Brightness of the dimmed picture: group 0 is black at alpha 64 of 128.
pub const DIM_BRIGHTNESS: f32 = 0.5;
/// Middle line of the picture in menu units (the 448 line picture sits 16 lines up, `menu_gfx::Y_BIAS`).
const CENTER_Y: f32 = menu_gfx::SCREEN[1] / 2.0 - menu_gfx::Y_BIAS;

pub struct PausePlugin;

impl Plugin for PausePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Band>().add_systems(Startup, load.after(super::load)).add_systems(Update, tick.before(crate::schedule::Frame::Popups));
    }
}

/// The band's nodes in the menu scene (empty until the file is loaded).
#[derive(Resource, Default)]
pub struct Band {
    node: Option<usize>,
}

impl Band {
    /// The file was read: the band can be shown.
    pub fn is_ready(&self) -> bool {
        self.node.is_some()
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
    let mut n = Node::new(Some(seq_index));
    n.tex = tex;
    n.pos = [menu_gfx::SCREEN[0] / 2.0, CENTER_Y];
    n.visible = false;
    band.node = Some(scene.add(n));
}

/// Shows the band and freezes the simulation (the same frame as the open input).
pub fn open(band: &Band, scene: &mut MenuScene, pause: &mut Pause) -> bool {
    let Some(id) = band.node else { return false };
    pause.band = true;
    scene.hidden = false;
    scene.dim = super::dim_alpha(DIM_BRIGHTNESS);
    scene.nodes[id].visible = true;
    scene.set_group(id, group::BAND);
    true
}

/// Hides the band; the simulation runs again from the next 60 Hz tick.
pub fn close(band: &Band, scene: &mut MenuScene, pause: &mut Pause) {
    pause.band = false;
    scene.hidden = true;
    scene.dim = 0.0;
    if let Some(id) = band.node {
        scene.nodes[id].visible = false;
    }
}

fn tick(pause: Res<Pause>, mut scene: ResMut<MenuScene>, real: Res<Time<Real>>, mut hud: Query<&mut Camera, With<crate::hud::HudCamera>>) {
    if pause.band {
        scene.advance(real.delta_secs());
    }
    // The original shows no HUD while the PAUSE band is up: the HUD camera is off, back on at the close.
    for mut cam in &mut hud {
        if cam.is_active == pause.band {
            cam.is_active = !pause.band;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scene() -> (MenuScene, Band) {
        let mut scene = MenuScene::default();
        for _ in 0..1 {
            let mut n = Node::new(None);
            n.visible = false;
            scene.add(n);
        }
        scene.hidden = true;
        (scene, Band { node: Some(0) })
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

    /// `cargo test -p kh2-app dump_pause_groups -- --nocapture` (needs `KH2_ISO`): prints every group of pause.2ld.
    #[test]
    fn dump_pause_groups() {
        let Ok(iso) = std::env::var("KH2_ISO") else { return };
        let mut a = kh2_archive::Archive::open(&iso).unwrap();
        let Some(two) = a.read("menu/us/pause.2ld").ok().and_then(|b| kh2_image::layd::parse_2ld(&b)) else { return };
        let seq = &two.layd.seqs[0];
        for (gi, g) in seq.groups.iter().enumerate() {
            println!("group {gi}: loops {} flags {:#x} loop_start {} end {} x {} y {} size {}", g.loops, g.flags, g.loop_start, g.frame_end, g.x, g.y, g.size);
            for an in seq.anims.iter().skip(g.first).take(g.count) {
                let sg = &seq.sprite_groups[an.group];
                let parts: Vec<_> = seq.parts.iter().skip(sg.start).take(sg.count).map(|p| [p.left, p.top, p.right, p.bottom]).collect();
                println!(
                    "  anim fr {}..{} flags {:#x} blend {} col {:?}->{:?} tx {:?} ty {:?} scale {:?} sx {:?} sy {:?} parts {:?}",
                    an.frame_start, an.frame_end, an.flags, an.blend, an.color_start, an.color_end, an.translate_x, an.translate_y, an.scale, an.scale_x, an.scale_y, parts
                );
            }
        }
    }

    #[test]
    fn the_band_dims_to_half() {
        // Group 0 of pause.2ld: black at alpha 64 of 128.
        assert_eq!(DIM_BRIGHTNESS, 0.5);
    }
}
