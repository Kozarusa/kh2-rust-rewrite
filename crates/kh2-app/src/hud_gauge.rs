//! Sora's HP ring / bar and MP bar, assembled and driven the way the game's party panel does
//! (the panel class with the `0x3b90`-byte object; see docs/hud.md for the addresses).
//!
//! HP: the first 60 points fill a 270 degree ring (pi/40 per point) that starts at the west end
//! and runs clockwise; points beyond 60 extend a bar to the left at 3 pixels each. Three layers
//! use the same rule: maximum HP (dark), the HP before the last hit (the trail) and the current
//! HP (green). HP at or below a quarter of the maximum switches the panel to its danger look.
//! MP: a blue bar one pixel per point.

use std::f32::consts::{FRAC_PI_2, PI};

use bevy::prelude::*;

use crate::data::GameData;
use super::hud_gfx::{HudScene, Node};
use crate::sora::Sora;

/// Sequence pair order in the world's field file: fiel, gaug, mapj, miss.
pub const GAUG: usize = 1;

/// HP below which the ring (instead of the bar) shows the value, and points per ring step.
const RING_HP: i32 = 60;
const RING_PER_HP: f32 = PI / 40.0;
/// Bar pixels per HP beyond the ring.
const BAR_PER_HP: f32 = 3.0;
/// Width of the bar sprites (they are drawn 100 pixels long and scaled).
const BAR_SPRITE: f32 = 100.0;

/// Animation groups of the gauge file, by the roles in the panel.
mod g {
    pub const CIRCLE: [usize; 2] = [137, 132];
    pub const RING_BACK: [usize; 2] = [121, 122];
    pub const RING_DEFAULT: usize = 121;
    pub const RING_TRAIL: usize = 126;
    pub const RING_GREEN: usize = 128;
    pub const BAR_BACK: [usize; 2] = [124, 125];
    pub const BAR_TRAIL: usize = 127;
    pub const BAR_GREEN: usize = 129;
    pub const CAP: usize = 123;
    pub const MP_LABEL: usize = 116;
    pub const MP_BACK: usize = 119;
    pub const MP_FILL: usize = 115;
    pub const MP_CAP: usize = 120;
    /// Sora's face file: normal, and the two red variants the game uses at low HP.
    pub const FACE: [usize; 2] = [3, 0];
}

/// Which `HudScene` file Sora's face is (set when the files are loaded).
#[derive(Resource, Default)]
pub struct FaceFile(pub Option<usize>);

#[derive(Resource, Default)]
pub struct SoraPanel {
    built: bool,
    circle: usize,
    face: Option<usize>,
    ring: [usize; 3],
    bar: [usize; 3],
    cap: usize,
    mp: [usize; 4],
    danger: bool,
    last_hp: i32,
    lag_hp: i32,
}

/// Where a HP value sits: on the ring (sweep in radians) or as a bar (length in pixels).
enum Fill {
    Ring(f32),
    Bar(f32),
}

fn fill(hp: i32) -> Fill {
    if hp < RING_HP { Fill::Ring(hp.max(0) as f32 * RING_PER_HP) } else { Fill::Bar((hp - RING_HP) as f32 * BAR_PER_HP) }
}

pub fn update(
    mut scene: ResMut<HudScene>,
    mut panel: ResMut<SoraPanel>,
    face: Res<FaceFile>,
    data: Res<GameData>,
    sora: Query<&Sora>,
) {
    let Ok(s) = sora.single() else { return };
    if scene.files.len() <= GAUG {
        return;
    }
    let (mut hp, mut max) = (s.hp.max(0), s.stats.max_hp.max(1));
    let (mut mp, mut mp_max) = (s.mp.clamp(0, data.sora_max_mp), data.sora_max_mp.max(1));
    // `KH2_HUDTEST=hp,max,mp,mpmax`: draw other values than the game state holds (screenshots of
    // states the first levels never reach, such as the HP bar past 60). Display only.
    if let Some([a, b, c, d]) = test_values() {
        (hp, max, mp, mp_max) = (a, b, c, d);
    }
    if !panel.built {
        build(&mut scene, &mut panel, face.0, hp);
    }
    let p = &mut *panel;
    // Low HP: at or below a quarter of the maximum (the game's integer `hp <= max >> 2`).
    let danger = hp <= max >> 2;
    if danger != p.danger {
        p.danger = danger;
        let m = danger as usize;
        scene.set_group(p.circle, g::CIRCLE[m]);
        scene.set_group(p.ring[0], g::RING_BACK[m]);
        scene.set_group(p.bar[0], g::BAR_BACK[m]);
        if let Some(f) = p.face {
            scene.set_group(f, g::FACE[m]);
        }
    }
    // On a hit the trail starts from the HP before it; on a heal it snaps to the new HP.
    if hp != p.last_hp {
        p.lag_hp = if p.last_hp < hp { hp } else { p.last_hp };
        p.last_hp = hp;
        scene.set_group(p.ring[1], g::RING_TRAIL);
        scene.set_group(p.bar[1], g::BAR_TRAIL);
    }
    for (layer, value) in [(0, max), (1, p.lag_hp), (2, hp)] {
        let (ring, bar) = (p.ring[layer], p.bar[layer]);
        match fill(value) {
            Fill::Ring(sweep) => {
                scene.nodes[ring].sweep = Some(sweep);
                scene.nodes[bar].visible = false;
            }
            Fill::Bar(len) => {
                scene.nodes[ring].sweep = None;
                let n = &mut scene.nodes[bar];
                n.visible = true;
                n.ext.scale[1] = len / BAR_SPRITE;
            }
        }
    }
    // The cap marks the end of the maximum: on the ring at the sweep, or at the bar's far end.
    let cap = &mut scene.nodes[p.cap];
    match fill(max) {
        Fill::Ring(sweep) => {
            cap.ext.rot = Some([0.0, 0.0, -sweep]);
            cap.off = [0.0, 0.0];
        }
        Fill::Bar(len) => {
            cap.ext.rot = Some([0.0, 0.0, FRAC_PI_2]);
            cap.off = [-len, 0.0];
        }
    }
    let (mp, mp_max) = (mp as f32, mp_max as f32);
    scene.nodes[p.mp[1]].ext.scale[1] = mp_max / BAR_SPRITE;
    scene.nodes[p.mp[2]].ext.scale[1] = mp / BAR_SPRITE;
    scene.nodes[p.mp[3]].off = [-mp_max, 0.0];
}

fn test_values() -> Option<[i32; 4]> {
    static VALUES: std::sync::OnceLock<Option<[i32; 4]>> = std::sync::OnceLock::new();
    *VALUES.get_or_init(|| {
        let v: Vec<i32> = std::env::var("KH2_HUDTEST").ok()?.split(',').filter_map(|x| x.trim().parse().ok()).collect();
        (v.len() == 4).then(|| [v[0], v[1].max(1), v[2], v[3].max(1)])
    })
}

/// Creates the panel's objects in the game's creation order; each one starts its group.
fn build(scene: &mut HudScene, p: &mut SoraPanel, face_file: Option<usize>, hp: i32) {
    let add = |scene: &mut HudScene, file: usize, parent: Option<usize>, group: usize| {
        let i = scene.add(Node::new(file, parent));
        scene.set_group(i, group);
        i
    };
    p.circle = add(scene, GAUG, None, g::CIRCLE[0]);
    p.ring = [
        add(scene, GAUG, Some(p.circle), g::RING_BACK[0]),
        add(scene, GAUG, Some(p.circle), g::RING_DEFAULT),
        add(scene, GAUG, Some(p.circle), g::RING_GREEN),
    ];
    p.bar = [
        add(scene, GAUG, Some(p.circle), g::BAR_BACK[0]),
        add(scene, GAUG, Some(p.circle), g::BAR_TRAIL),
        add(scene, GAUG, Some(p.circle), g::BAR_GREEN),
    ];
    p.cap = add(scene, GAUG, Some(p.circle), g::CAP);
    p.face = face_file.map(|f| add(scene, f, Some(p.circle), g::FACE[0]));
    let label = add(scene, GAUG, Some(p.circle), g::MP_LABEL);
    p.mp = [
        label,
        add(scene, GAUG, Some(label), g::MP_BACK),
        add(scene, GAUG, Some(label), g::MP_FILL),
        add(scene, GAUG, Some(label), g::MP_CAP),
    ];
    for i in p.bar {
        scene.nodes[i].ext.scale[1] = 0.0;
    }
    p.last_hp = hp;
    p.lag_hp = hp;
    p.built = true;
}
