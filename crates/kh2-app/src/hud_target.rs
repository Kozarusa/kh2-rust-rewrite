//! The target's HP gauge (top right) and the lock-on reticle.
//!
//! Gauge: one 200 pixel bar per 200 HP, drawn right to left from the "HP" tag; a back bar for the
//! top segment, a trail (the HP before the last hit, flashing then fading) and the current
//! segment (from the game's gauge class that the party panel's neighbour in `0x3b1c` bytes).
//! Reticle: sequence group 25 of the field file, placed at the target's projected position.

use bevy::prelude::*;

use crate::enemy::Enemy;
use super::hud_gfx::{viewport_to_hud, HudScene, Node};
use super::hud_gauge::GAUG;
use crate::sora::Sora;

/// The field file is the first pair of the world's `.2dd`.
const FIEL: usize = 0;
/// HP per bar segment and pixels per HP.
const SEGMENT: i32 = 200;
/// Seconds an enemy's gauge stays up after it was last hit while not locked on. The game's rule
/// for dropping the gauge was not found; this value is a stand-in.
const HOLD: f32 = 5.0;

mod g {
    pub const TAG: usize = 38;
    pub const FRAME: usize = 42;
    pub const BAR: usize = 39;
    pub const CAP: usize = 40;
    pub const TRAIL: usize = 41;
    pub const CUR: usize = 43;
    pub const RETICLE: usize = 25;
}

#[derive(Resource, Default)]
pub struct TargetHud {
    built: bool,
    /// tag, frame, back bar, cap, trail, current
    gauge: [usize; 6],
    reticle: usize,
    /// The enemy hit last, and seconds its gauge stays up.
    shown: Option<Entity>,
    hold: f32,
    /// The enemy the gauge currently follows.
    bound: Option<Entity>,
    last_hp: i32,
    prev_hp: i32,
    reticle_for: Option<Entity>,
}

/// HP bookkeeping: remembers each enemy's HP to see which one was hit.
#[derive(Default)]
pub struct Seen(std::collections::HashMap<Entity, i32>);

pub fn update(
    time: Res<Time>,
    mut scene: ResMut<HudScene>,
    mut hud: ResMut<TargetHud>,
    mut seen: Local<Seen>,
    sora: Query<&Sora>,
    enemies: Query<(Entity, &GlobalTransform, &Enemy)>,
    cam: Query<(&Camera, &GlobalTransform), With<Camera3d>>,
) {
    if scene.files.len() <= GAUG {
        return;
    }
    if !hud.built {
        build(&mut scene, &mut hud);
    }
    let h = &mut *hud;
    let lock = sora.single().ok().and_then(|s| s.lock);
    // The enemy that lost HP most recently keeps its gauge up for a while.
    for (e, _, en) in &enemies {
        let last = seen.0.insert(e, en.hp).unwrap_or(en.hp);
        if en.hp < last {
            h.shown = Some(e);
            h.hold = HOLD;
        }
    }
    seen.0.retain(|e, _| enemies.contains(*e));
    h.hold -= time.delta_secs();
    let target = lock.or(if h.hold > 0.0 { h.shown } else { None }).and_then(|e| enemies.get(e).ok());

    // Gauge.
    let Some((e, _, en)) = target else {
        for &i in &h.gauge {
            scene.nodes[i].visible = false;
        }
        scene.nodes[h.reticle].visible = false;
        h.reticle_for = None;
        h.bound = None;
        return;
    };
    let (hp, max) = (en.hp.max(0), en.stats.max_hp.max(1));
    if h.bound != Some(e) {
        // New target: the gauge starts at its HP with no trail.
        h.bound = Some(e);
        h.prev_hp = hp;
        h.last_hp = hp;
    } else if hp != h.last_hp {
        h.prev_hp = h.last_hp;
        h.last_hp = hp;
        scene.set_group(h.gauge[4], g::TRAIL);
    }
    // The game's segment arithmetic: C division truncating toward zero.
    let segs = (max - 1) / SEGMENT;
    let cur_idx = (hp - 1) / SEGMENT;
    let prev_idx = (h.prev_hp - 1) / SEGMENT;
    let mut v_max = max - segs * SEGMENT;
    let v_cur = hp - cur_idx * SEGMENT;
    let mut v_prev = h.prev_hp - prev_idx * SEGMENT;
    if v_max < v_cur || segs != cur_idx {
        v_max = SEGMENT;
    }
    if prev_idx != cur_idx {
        v_prev = SEGMENT;
    }
    for i in h.gauge {
        scene.nodes[i].visible = true;
    }
    for (node, v) in [(2, v_max), (4, v_prev), (5, v_cur)] {
        let n = &mut scene.nodes[h.gauge[node]];
        n.ext.scale[1] = v as f32 / 100.0;
        n.off = [-(v as f32), 0.0];
    }
    scene.nodes[h.gauge[3]].off = [-(v_max as f32), 0.0];

    // Reticle at the locked enemy only.
    let locked = lock.filter(|&l| l == e);
    scene.nodes[h.reticle].visible = locked.is_some();
    if locked.is_some() {
        if h.reticle_for != Some(e) {
            h.reticle_for = Some(e);
            scene.set_group(h.reticle, g::RETICLE);
        }
        let at = cam.single().ok().zip(enemies.get(e).ok()).and_then(|((c, ct), (_, gt, _))| {
            let vp = c.world_to_viewport(ct, gt.translation() + Vec3::Y * 0.9).ok()?;
            Some(viewport_to_hud(vp, c.logical_viewport_size()?))
        });
        if let Some(p) = at {
            scene.nodes[h.reticle].pos = [p.x, p.y];
        }
    } else {
        h.reticle_for = None;
    }
}

fn build(scene: &mut HudScene, h: &mut TargetHud) {
    let add = |scene: &mut HudScene, file: usize, parent: Option<usize>, group: usize| {
        let i = scene.add(Node::new(file, parent));
        scene.set_group(i, group);
        scene.nodes[i].visible = false;
        i
    };
    let tag = add(scene, GAUG, None, g::TAG);
    h.gauge = [
        tag,
        add(scene, GAUG, Some(tag), g::FRAME),
        add(scene, GAUG, Some(tag), g::BAR),
        add(scene, GAUG, Some(tag), g::CAP),
        add(scene, GAUG, Some(tag), g::TRAIL),
        add(scene, GAUG, Some(tag), g::CUR),
    ];
    h.reticle = add(scene, FIEL, None, g::RETICLE);
    h.built = true;
}
