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
/// Pips the row holds (0x153ff8 stops at 16).
const PIPS: usize = 16;
/// Distance of the free-aim point from the camera in metres (the game uses 300 units; any distance on the
/// camera's axis lands at the same screen point).
const FREE_DISTANCE: f32 = 10.0;

mod g {
    pub const TAG: usize = 38;
    pub const FRAME: usize = 42;
    pub const BAR: usize = 39;
    pub const CAP: usize = 40;
    pub const TRAIL: usize = 41;
    pub const CUR: usize = 43;
    /// Pip row: the back (45), a filled pip (44) and a pip just emptied (46).
    pub const PIP_BACK: usize = 45;
    pub const PIP_FULL: usize = 44;
    pub const PIP_LOST: usize = 46;
    /// Lock-on reticle on a target, the free-aim reticle, and the alert bubble above the reticle.
    pub const RETICLE: usize = 25;
    pub const FREE: usize = 3;
    pub const BUBBLE: usize = 20;
}

/// Game-state input of the lock-on bubble: true while the locked target shows its alert bubble (the game
/// shows it when the target's flag +0x588 bit 1 is set and one of its timers +0xac8 or +0xacc is positive,
/// 0x19bbc0). The enemy module fills it in; `KH2_HUDBUBBLE=1` shows it.
#[derive(Resource, Default, Clone, Copy)]
pub struct TargetBubble(pub bool);

#[derive(Resource, Default)]
pub struct TargetHud {
    built: bool,
    /// tag, frame, back bar, cap, trail, current
    gauge: [usize; 6],
    /// Back and fill node of each pip of the row (one pip per 200 HP segment past the first, at most 16).
    pips: [[usize; 2]; PIPS],
    reticle: usize,
    bubble: usize,
    /// The enemy hit last, and seconds its gauge stays up.
    shown: Option<Entity>,
    hold: f32,
    /// The enemy the gauge currently follows.
    bound: Option<Entity>,
    last_hp: i32,
    prev_hp: i32,
}

/// HP bookkeeping: remembers each enemy's HP to see which one was hit.
#[derive(Default)]
pub struct Seen(std::collections::HashMap<Entity, i32>);

#[allow(clippy::too_many_arguments)]
pub fn update(
    time: Res<Time>,
    mut scene: ResMut<HudScene>,
    mut hud: ResMut<TargetHud>,
    mut seen: Local<Seen>,
    bubble: Res<TargetBubble>,
    sora: Query<&Sora>,
    enemies: Query<(Entity, &GlobalTransform, &Enemy)>,
    cam: Query<(&Camera, &GlobalTransform), (With<Camera3d>, Without<crate::menu::preview::PreviewCamera>)>,
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

    // Reticle (0x11a9b0): on the locked enemy group 25, otherwise the free-aim point a fixed distance
    // ahead of the camera (0x11a5f8, group 3). Both are projected with the 3D camera.
    let locked = lock.filter(|&l| enemies.contains(l));
    let want = if locked.is_some() { g::RETICLE } else { g::FREE };
    let at = cam.single().ok().and_then(|(c, ct)| {
        let point = match locked.and_then(|e| enemies.get(e).ok()) {
            Some((_, gt, _)) => gt.translation() + Vec3::Y * 0.9,
            None => ct.translation() + *ct.forward() * FREE_DISTANCE,
        };
        let vp = c.world_to_viewport(ct, point).ok()?;
        Some(viewport_to_hud(vp, c.logical_viewport_size()?))
    });
    // The original creates the group 3 icon at the free-aim point (0x11a9b0), but nothing is visible
    // there in play (user's observation of the original), and which picture group 3 is was only read
    // from a screenshot. Until that is traced, only a locked target shows the reticle.
    let shown_reticle = at.is_some() && locked.is_some();
    scene.nodes[h.reticle].visible = shown_reticle;
    if let Some(p) = at {
        if scene.nodes[h.reticle].player.group != Some(want) {
            scene.set_group(h.reticle, want);
        }
        scene.nodes[h.reticle].pos = [p.x, p.y];
    }
    let alert = bubble.0 || std::env::var("KH2_HUDBUBBLE").is_ok_and(|v| v != "0");
    scene.nodes[h.bubble].visible = shown_reticle && locked.is_some() && alert;

    // `KH2_HUDTARGETTEST=<hp>:<max>` shows the gauge for those values without an enemy (display only).
    let test = std::env::var("KH2_HUDTARGETTEST").ok().and_then(|v| {
        let mut it = v.split(':').filter_map(|x| x.parse::<i32>().ok());
        Some((it.next()?, it.next()?))
    });
    // Gauge.
    let (e, hp, max) = if let Some((hp, max)) = test {
        (Entity::PLACEHOLDER, hp.max(0), max.max(1))
    } else {
        let Some((e, _, en)) = target else {
            for &i in h.gauge.iter().chain(h.pips.iter().flatten()) {
                scene.nodes[i].visible = false;
            }
            h.bound = None;
            return;
        };
        (e, en.hp.max(0), en.stats.max_hp.max(1))
    };
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
    // The long frame (group 42) is hidden by the game unless the HP is in the top segment of a gauge
    // that has more than one (0x154498 sets +0x3b19 and hides the object at +0x1bc otherwise).
    scene.nodes[h.gauge[1]].visible = frame_shown(hp, max);
    for (node, v) in [(2, v_max), (4, v_prev), (5, v_cur)] {
        let n = &mut scene.nodes[h.gauge[node]];
        n.ext.scale[1] = v as f32 / 100.0;
        n.off = [-(v as f32), 0.0];
    }
    scene.nodes[h.gauge[3]].off = [-(v_max as f32), 0.0];

    // Pip row (0x153ff8, 0x1543f0, 0x154758): one back pip per segment below the top one, and a
    // filled pip for each full segment left under the current one (a pip just emptied plays its own
    // group), up to the segments the previous HP had.
    let n_seg = (((max - 1) / SEGMENT).max(0) as usize).min(PIPS);
    let (cur_seg, prev_seg) = (((hp - 1) / SEGMENT).max(0) as usize, ((h.prev_hp - 1) / SEGMENT).max(0) as usize);
    for k in 0..PIPS {
        let [back, fill] = h.pips[k];
        scene.nodes[back].visible = k < n_seg;
        let show = n_seg > 0 && k < prev_seg.max(cur_seg).min(PIPS);
        scene.nodes[fill].visible = show;
        let want = if k < cur_seg { g::PIP_FULL } else { g::PIP_LOST };
        if show && scene.nodes[fill].player.group != Some(want) {
            scene.set_group(fill, want);
        }
    }
}

/// Whether the gauge's frame (group 42, 205 pixels wide) is drawn (0x154498): only when the current HP
/// sits in the top segment of a maximum that spans more than one segment (`cur_idx == segs != 0`, C
/// division truncating toward zero). An enemy of up to 200 HP gets the bar alone, so its gauge is as
/// long as its HP; drawing the frame always made every gauge look 200 HP long (WP-FIX-12).
fn frame_shown(hp: i32, max: i32) -> bool {
    let cur_idx = (hp - 1) / SEGMENT;
    cur_idx == (max - 1) / SEGMENT && cur_idx != 0
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
    // The pips sit left of the tag in a row, each one the back group's `unk20` field further (0x237698).
    let step = scene.files.get(GAUG).and_then(|f| f.seq.groups.get(g::PIP_BACK)).map(|g| g.unk20 as f32).unwrap_or(0.0);
    for k in 0..PIPS {
        let back = add(scene, GAUG, Some(tag), g::PIP_BACK);
        let fill = add(scene, GAUG, Some(tag), g::PIP_FULL);
        for n in [back, fill] {
            scene.nodes[n].off = [-step * k as f32, 0.0];
        }
        h.pips[k] = [back, fill];
    }
    h.reticle = add(scene, FIEL, None, g::FREE);
    h.bubble = add(scene, FIEL, Some(h.reticle), g::BUBBLE);
    h.built = true;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frame_shows_only_in_the_top_segment_of_a_multi_segment_gauge() {
        // One segment (max 1..=200): never, however much HP is left; so a 10 HP enemy's gauge is 10 pixels.
        assert!(!frame_shown(10, 10));
        assert!(!frame_shown(200, 200));
        assert!(!frame_shown(0, 200));
        // 450 HP: three segments. The frame is up while the HP is in the top one (401..=450), not below it.
        assert!(frame_shown(450, 450));
        assert!(frame_shown(401, 450));
        assert!(!frame_shown(400, 450));
        assert!(!frame_shown(1, 450));
        // A maximum on a segment boundary (400 = two segments).
        assert!(frame_shown(400, 400));
        assert!(frame_shown(201, 400));
        assert!(!frame_shown(200, 400));
        // Dead (0 HP): the C division truncates toward zero, so segment 0: no frame.
        assert!(!frame_shown(0, 450));
    }
}
