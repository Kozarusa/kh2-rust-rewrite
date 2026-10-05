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

/// Sora's MP recharge state (player +0x1bc progress and +0x1c0 total, floats). The MP bar switches to its
/// recharge look while `total > 0`: a full-size back bar and a fill that grows with `progress / total`
/// (0x157628, 0x1556a0). The player module fills it in; `KH2_HUDMPRECHARGE=<progress>:<total>` shows it.
#[derive(Resource, Clone, Copy, Debug, Default)]
pub struct MpRecharge {
    pub progress: f32,
    pub total: f32,
}

/// MP bar groups (label, back, fill, cap): normal and recharge sets (0x157628).
const MP_NORMAL: [usize; 4] = [116, 119, 115, 120];
const MP_RECHARGE: [usize; 4] = [110, 113, 109, 114];

/// Which art the drive gauge uses (the player's mode byte at +0x1af in the game: 0 normal, 2 and 3 the
/// two form variants, 4 locked behind chains).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum DriveMode {
    #[default]
    Normal,
    FormA,
    FormB,
    Locked,
}

/// Game-state input of the drive gauge. The player module fills it in; `KH2_HUDDRIVE=<points>:<max>:<fraction>[:mode]`
/// (mode 0 normal, 2, 3 or 4) shows it by hand. `enabled` is false until the player has a drive gauge.
#[derive(Resource, Clone, Debug)]
pub struct DriveGauge {
    pub enabled: bool,
    pub mode: DriveMode,
    /// Whole drive points (player +0x1b1) and the most the gauge holds (+0x1b2).
    pub points: u8,
    pub max: u8,
    /// Progress to the next point, 0 to 99 (+0x1b0).
    pub fraction: u8,
}

impl Default for DriveGauge {
    fn default() -> DriveGauge {
        DriveGauge { enabled: false, mode: DriveMode::Normal, points: 0, max: 5, fraction: 0 }
    }
}

/// Digit groups by drive points for the three art sets (executable tables 0x3589c0, 0x3589e8,
/// 0x358998), read at run time.
#[derive(Resource, Default)]
pub struct DriveTables(pub Option<[[u32; 10]; 3]>);

impl DriveTables {
    pub fn from_exe(exe: &[u8]) -> DriveTables {
        let w = |va: usize| exe.get(va - 0xff000..va - 0xff000 + 4).map(|b| u32::from_le_bytes(b.try_into().unwrap()));
        let table = |va: usize| {
            let mut t = [0u32; 10];
            for (i, v) in t.iter_mut().enumerate() {
                *v = w(va + 4 * i)?;
            }
            Some(t)
        };
        DriveTables(table(0x3589c0).zip(table(0x3589e8)).zip(table(0x358998)).map(|((a, b), c)| [a, b, c]))
    }
}

/// Drive gauge groups (gaug sequence) by mode: the bar's intro and idle groups (0x155a30, 0x155a88),
/// the fill (0x155b08) and the MAX tag and gain burst.
mod dg {
    use super::DriveMode;
    pub fn intro(m: DriveMode) -> usize {
        match m {
            DriveMode::FormA => 50,
            DriveMode::FormB => 140,
            DriveMode::Locked => 18,
            DriveMode::Normal => 21,
        }
    }
    pub fn idle(m: DriveMode) -> usize {
        match m {
            DriveMode::FormA => 51,
            DriveMode::FormB => 141,
            DriveMode::Locked => 19,
            DriveMode::Normal => 23,
        }
    }
    pub fn fill(m: DriveMode) -> usize {
        match m {
            DriveMode::FormA => 49,
            DriveMode::FormB => 139,
            _ => 25,
        }
    }
    pub const MAX: usize = 20;
    pub const BURST: usize = 22;
}

#[derive(Resource, Default)]
pub struct SoraPanel {
    drive: [usize; 5],
    drive_mode: Option<DriveMode>,
    recharging: bool,
    drive_points: Option<u8>,
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

#[allow(clippy::too_many_arguments)]
pub fn update(
    mut scene: ResMut<HudScene>,
    mut panel: ResMut<SoraPanel>,
    face: Res<FaceFile>,
    data: Res<GameData>,
    mut drive: ResMut<DriveGauge>,
    mut recharge: ResMut<MpRecharge>,
    drive_tables: Res<DriveTables>,
    sora: Query<&Sora>,
    mut demo: Local<bool>,
) {
    let Ok(s) = sora.single() else { return };
    if !*demo {
        *demo = true;
        if let Some(d) = drive_test() {
            *drive = d;
        }
        if let Some(r) = recharge_test() {
            *recharge = r;
        }
    }
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
    let charging = recharge.total > 0.0;
    if charging != p.recharging {
        p.recharging = charging;
        let set = if charging { MP_RECHARGE } else { MP_NORMAL };
        for (node, g) in p.mp.into_iter().zip(set) {
            scene.set_group(node, g);
        }
    }
    if charging {
        // A full-size back bar (scale 1.0 = 100 pixels) and a fill by the recharge ratio.
        let ratio = (recharge.progress / recharge.total).clamp(0.0, 1.0);
        scene.nodes[p.mp[1]].ext.scale[1] = 1.0;
        scene.nodes[p.mp[2]].ext.scale[1] = ratio;
        scene.nodes[p.mp[3]].off = [-BAR_SPRITE, 0.0];
    } else {
        scene.nodes[p.mp[1]].ext.scale[1] = mp_max / BAR_SPRITE;
        scene.nodes[p.mp[2]].ext.scale[1] = mp / BAR_SPRITE;
        scene.nodes[p.mp[3]].off = [-mp_max, 0.0];
    }
    update_drive(&mut scene, p, &drive, &drive_tables);
}

/// The drive gauge (objects 12 to 15 of the panel, see docs/hud.md): the bar slides in, then idles;
/// the orange fill follows the progress to the next point; the digit shows the whole points; MAX
/// shows when the gauge is full; a burst plays when a point is gained.
fn update_drive(scene: &mut HudScene, p: &mut SoraPanel, d: &DriveGauge, tables: &DriveTables) {
    let [bar, fill, digit, max, burst] = p.drive;
    let Some(table) = tables.0.as_ref() else { return };
    if !d.enabled {
        for n in p.drive {
            scene.nodes[n].visible = false;
        }
        p.drive_mode = None;
        p.drive_points = None;
        return;
    }
    // A mode change restarts the bar from its intro group.
    if p.drive_mode != Some(d.mode) {
        p.drive_mode = Some(d.mode);
        scene.nodes[bar].visible = true;
        scene.set_group(bar, dg::intro(d.mode));
        scene.set_group(fill, dg::fill(d.mode));
        p.drive_points = None;
    }
    if scene.nodes[bar].player.group == Some(dg::intro(d.mode)) && scene.nodes[bar].player.finished() {
        scene.set_group(bar, dg::idle(d.mode));
    }
    let set = match d.mode {
        DriveMode::FormA => 1,
        DriveMode::FormB => 2,
        _ => 0,
    };
    let pts = d.points.min(9);
    if p.drive_points != Some(pts) {
        // A gain plays the burst (the game also plays sound 0x20 or 0x21, not ported here).
        if p.drive_points.is_some_and(|old| pts > old) {
            scene.set_group(burst, dg::BURST);
            scene.nodes[burst].visible = true;
        }
        p.drive_points = Some(pts);
        scene.set_group(digit, table[set][pts as usize] as usize);
    }
    if scene.nodes[burst].player.finished() {
        scene.nodes[burst].visible = false;
    }
    let full = d.points >= d.max;
    scene.nodes[fill].reveal = Some(if full { 1.0 } else { d.fraction.min(99) as f32 / 100.0 });
    scene.nodes[max].visible = full && d.mode == DriveMode::Normal;
    for n in [bar, fill, digit] {
        scene.nodes[n].visible = true;
    }
}

fn recharge_test() -> Option<MpRecharge> {
    let v = std::env::var("KH2_HUDMPRECHARGE").ok()?;
    let mut it = v.split(':').filter_map(|x| x.parse::<f32>().ok());
    Some(MpRecharge { progress: it.next()?, total: it.next()? })
}

/// `KH2_HUDDRIVE=<points>:<max>:<fraction>[:mode]`: draw the drive gauge with those values.
fn drive_test() -> Option<DriveGauge> {
    let v = std::env::var("KH2_HUDDRIVE").ok()?;
    let p: Vec<&str> = v.split(':').collect();
    let n = |i: usize, d: u8| p.get(i).and_then(|x| x.parse::<u8>().ok()).unwrap_or(d);
    let mode = match n(3, 0) {
        2 => DriveMode::FormA,
        3 => DriveMode::FormB,
        4 => DriveMode::Locked,
        _ => DriveMode::Normal,
    };
    Some(DriveGauge { enabled: true, mode, points: n(0, 0), max: n(1, 5), fraction: n(2, 0) })
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
    // Drive gauge: the bar, its fill, the digit, the MAX tag and the gain burst (children of the bar).
    let blank = |scene: &mut HudScene, parent: usize| {
        let mut n = Node::new(GAUG, Some(parent));
        n.visible = false;
        scene.add(n)
    };
    let bar = blank(scene, p.circle);
    p.drive = [bar, blank(scene, bar), blank(scene, bar), blank(scene, bar), blank(scene, bar)];
    scene.set_group(p.drive[3], dg::MAX);
    p.last_hp = hp;
    p.lag_hp = hp;
    p.built = true;
}
