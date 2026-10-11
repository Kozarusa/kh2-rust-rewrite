//! Sora's HP ring / bar and MP bar, assembled and driven the way the game's party panel does
//! (the panel class with the `0x3b90`-byte object; see docs/hud.md for the addresses).
//!
//! HP: the first 60 points fill a 270 degree ring (pi/40 per point) that starts at the west end
//! and runs clockwise; points beyond 60 extend a bar to the left at 3 pixels each. Three layers
//! use the same rule: maximum HP (dark), the HP before the last hit (the trail) and the current
//! HP (green). HP at or below a quarter of the maximum switches the panel to its danger look.
//! A hit flashes the circle and shows Sora's hurt face once, then the face of the panel's state.
//! MP: a blue bar one pixel per point.

use std::f32::consts::{FRAC_PI_2, PI};

use bevy::prelude::*;

use crate::data::GameData;
use super::hud_gfx::{Edge, HudScene, Node};
use crate::sora::Sora;

/// Sequence pair order in the world's field file: fiel, gaug, mapj, miss.
pub const GAUG: usize = 1;
/// The normal face group of a character's `face` pair (the party panels use it too).
pub const FACE_NORMAL: usize = 3;

/// HP below which the ring (instead of the bar) shows the value, and points per ring step.
const RING_HP: i32 = 60;
const RING_PER_HP: f32 = PI / 40.0;
/// Bar pixels per HP beyond the ring.
const BAR_PER_HP: f32 = 3.0;
/// Width of the bar sprites (they are drawn 100 pixels long and scaled).
const BAR_SPRITE: f32 = 100.0;

/// Animation groups of the gauge file, by the roles in the panel. The `[usize; 3]` sets are indexed
/// by the panel state (`PanelState`): the table at 0x358970 (read by 0x1558a0) and the face switch 0x153dd0.
mod g {
    pub const CIRCLE: [usize; 3] = [137, 132, 137];
    /// The circle on a hit: 135 for an ordinary hit (event 1, 0x1571d0). The game uses 134 and 133 for
    /// the two other hit kinds (events 2 and 3, 0x1571e8 and 0x157200), which this port does not tell apart.
    pub const CIRCLE_HIT: usize = 135;
    pub const RING_BACK: [usize; 3] = [121, 122, 121];
    pub const RING_DEFAULT: usize = 121;
    pub const RING_TRAIL: usize = 126;
    pub const RING_GREEN: usize = 128;
    pub const BAR_BACK: [usize; 3] = [124, 125, 124];
    pub const BAR_TRAIL: usize = 127;
    pub const BAR_GREEN: usize = 129;
    pub const CAP: usize = 123;
    pub const MP_LABEL: usize = 116;
    pub const MP_BACK: usize = 119;
    pub const MP_FILL: usize = 115;
    pub const MP_CAP: usize = 120;
    /// Sora's face file by panel state (0x153dd0): 3 the normal face, 0 the tired face pulsing red
    /// (danger), 2 the eyes-closed face in red (knocked out).
    pub const FACE: [usize; 3] = [3, 0, 2];
    /// The hurt face (teeth gritted, red fading to normal over 30 frames, held 30 more; plays once):
    /// started on every hit by 0x153ee8 with the state's face as its follow-up.
    pub const FACE_HIT: usize = 1;
}

/// The panel's state (panel +0x18, set every frame by 0x157d40): knocked out when the actor's flag
/// +0x7f8 bit 4 is set (0x166710), otherwise danger when `hp <= max >> 2` (0x1666e8 -> 0x1a0be0).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PanelState {
    Normal = 0,
    Danger = 1,
    KnockedOut = 2,
}

impl PanelState {
    pub fn of(hp: i32, max: i32, ko: bool) -> PanelState {
        if ko {
            PanelState::KnockedOut
        } else if hp <= max >> 2 {
            PanelState::Danger
        } else {
            PanelState::Normal
        }
    }
}

/// What reaches the panel in a frame. The game's damage handler (0x175340) posts event 1 to the
/// member's panel (0x13dd20; events 2 and 3 for two other hit kinds) and the HP change code
/// (0x186238) posts event 4 when a value rises (0x13de10); the panel runs them at the start of its
/// frame (0x157e48). Without an event, a change of the danger state switches the groups (0x1572a0,
/// 0x157310 through 0x157230). The port sees Sora's HP only, so a drop stands for a hit and a rise
/// for a heal.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PanelEvent {
    None,
    Hit,
    Heal,
    Switch,
}

impl PanelEvent {
    pub fn of(last_hp: i32, hp: i32, last: PanelState, now: PanelState) -> PanelEvent {
        if hp < last_hp {
            PanelEvent::Hit
        } else if hp > last_hp {
            PanelEvent::Heal
        } else if last != now {
            PanelEvent::Switch
        } else {
            PanelEvent::None
        }
    }
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
    /// Modes 2 and 3: the fill inside the current segment, 0.0 to 1.0 (`FUN_00155bf0`: the timer's
    /// remainder over the segment size, full on the last segment of a full timer). `None` uses
    /// `fraction`. The summon module fills it in mode 3.
    pub time_fill: Option<f32>,
}

impl Default for DriveGauge {
    fn default() -> DriveGauge {
        DriveGauge { enabled: false, mode: DriveMode::Normal, points: 0, max: 5, fraction: 0, time_fill: None }
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
    state: Option<PanelState>,
    /// The face group last logged (`hud: portrait face group N`).
    face_group: Option<usize>,
    last_hp: i32,
    lag_hp: i32,
    /// The danger alarm is sounding (`DAT_0036f58c`, set by 0x2c1528 and cleared by 0x2c1550).
    alarm: bool,
}

/// The danger alarm: system sound 9 (`FUN_002c1528` -> `FUN_002c14a8(9, 0)` -> `FUN_002be49c(2, 9, 0,
/// 0x3fff, 0x40)`; the 2 is the driver SLOT of the boot bank `se000.seb`, game bank 0), the app's
/// `PlaySe::number(9)` = `se 0:9`.
pub const DANGER_ALARM_SE: u32 = 9;

/// The alarm's rule, run every panel frame (0x157310 without an event; 0x157040 on a hit, 0x157108 on a heal):
/// not knocked out, in danger and the alarm silent -> it starts; knocked out, or the alarm sounding and the
/// danger over -> it stops (`FUN_002c1550`: `FUN_002c14f0(9, 0x3c, 0)`, a fade of 0x3c frames). Returns the
/// change: `Some(true)` start, `Some(false)` stop.
pub fn alarm_step(alarm: &mut bool, state: PanelState) -> Option<bool> {
    match (state, *alarm) {
        (PanelState::Danger, false) => {
            *alarm = true;
            Some(true)
        }
        (PanelState::Normal | PanelState::KnockedOut, true) => {
            *alarm = false;
            Some(false)
        }
        _ => None,
    }
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
    time: Res<Time>,
    mut demo: Local<bool>,
    mut se: MessageWriter<crate::audio::PlaySe>,
) {
    let Ok(s) = sora.single() else { return };
    let now = time.elapsed_secs();
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
    if let Some(h) = hp_timeline(now) {
        hp = h.clamp(0, max);
    }
    if !panel.built {
        build(&mut scene, &mut panel, face.0, hp);
    }
    let p = &mut *panel;
    panel_frame(&mut scene, p, hp, max, ko_test());
    // The danger alarm (WP-F1 item 13). The app's mixer has no stop: the sound plays once per start, and the
    // game's fade-out (0x2c1550, 0x3c frames) is only logged.
    match alarm_step(&mut p.alarm, PanelState::of(hp, max, ko_test())) {
        Some(true) => {
            info!("hud: danger alarm starts (0x2c1528: system sound 9, se000, hp {hp}/{max})");
            se.write(crate::audio::PlaySe::number(DANGER_ALARM_SE, None));
        }
        Some(false) => info!("hud: danger alarm stops (0x2c1550: fade 0x3c frames; not ported, the sound runs out) hp {hp}/{max}"),
        None => {}
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
    let reveal = match d.time_fill {
        // Modes 2 and 3 (FUN_00155bf0): the fill inside the segment; the game turns its mask by
        // `value * 23 / count` degrees (0x1557f8), approximated by the same left-edge reveal.
        Some(f) if d.mode != DriveMode::Normal => f.clamp(0.0, 1.0),
        _ if full => 1.0,
        _ => d.fraction.min(99) as f32 / 100.0,
    };
    scene.nodes[fill].reveal = Some(reveal);
    scene.nodes[max].visible = full && d.mode == DriveMode::Normal;
    for n in [bar, fill, digit] {
        scene.nodes[n].visible = true;
    }
}

/// One frame of the panel's reaction to Sora's HP: the event handlers run by 0x157e48 (hit 0x157040,
/// heal 0x157108) or, without an event, the state switch 0x157230. Then the HP trail (0x1574d8).
pub(crate) fn panel_frame(scene: &mut HudScene, p: &mut SoraPanel, hp: i32, max: i32, ko: bool) {
    let state = PanelState::of(hp, max, ko);
    let s = state as usize;
    let last = p.state.unwrap_or(PanelState::Normal);
    match PanelEvent::of(p.last_hp, hp, last, state) {
        PanelEvent::Hit => {
            // The circle flashes red and shakes, then returns to the state's circle (its follow-up
            // at +0x1afc); the back layers take the state's groups; the face shows the hurt face once
            // and then the state's face (0x153ee8 with mode 1: group 1, follow-up from 0x153dd0).
            scene.set_group_then(p.circle, g::CIRCLE_HIT, g::CIRCLE[s]);
            scene.set_group(p.ring[0], g::RING_BACK[s]);
            scene.set_group(p.bar[0], g::BAR_BACK[s]);
            if let Some(f) = p.face {
                scene.set_group_then(f, g::FACE_HIT, g::FACE[s]);
            }
        }
        // A heal (0x157108) sets the same groups as the state switch. The game skips the circle on the
        // first heal event after the panel is built (flag +0x3b8b); when that first event comes in the
        // game was not traced, and the port has no start-up event, so every heal resets the circle here.
        PanelEvent::Heal | PanelEvent::Switch => {
            scene.set_group(p.circle, g::CIRCLE[s]);
            scene.set_group(p.ring[0], g::RING_BACK[s]);
            scene.set_group(p.bar[0], g::BAR_BACK[s]);
            if let Some(f) = p.face {
                scene.set_group(f, g::FACE[s]);
            }
        }
        PanelEvent::None => {}
    }
    p.state = Some(state);
    // On a hit the trail starts from the HP before it; on a heal it snaps to the new HP.
    if hp != p.last_hp {
        p.lag_hp = if p.last_hp < hp { hp } else { p.last_hp };
        p.last_hp = hp;
        scene.set_group(p.ring[1], g::RING_TRAIL);
        scene.set_group(p.bar[1], g::BAR_TRAIL);
    }
    // Log every face change, including the hand-over from the hurt face to the state's face.
    if let Some(f) = p.face {
        let group = scene.nodes[f].player.group;
        if group != p.face_group {
            p.face_group = group;
            if let Some(gr) = group {
                info!("hud: portrait face group {gr} (state {state:?}, hp {hp}/{max})");
            }
        }
    }
}

/// `KH2_HUDHP=<s>:<hp>,<s>:<hp>,...`: from each time (seconds since start) on, the panel shows that HP
/// instead of Sora's (display only), so a run can show a hit, the danger state and a heal at known
/// times. Values are clamped to Sora's maximum.
fn hp_timeline(now: f32) -> Option<i32> {
    static STEPS: std::sync::OnceLock<Vec<(f32, i32)>> = std::sync::OnceLock::new();
    let steps = STEPS.get_or_init(|| {
        let v = std::env::var("KH2_HUDHP").unwrap_or_default();
        v.split(',').filter_map(|s| s.split_once(':')).filter_map(|(t, h)| Some((t.trim().parse().ok()?, h.trim().parse().ok()?))).collect()
    });
    steps.iter().filter(|(t, _)| *t <= now).max_by(|a, b| a.0.total_cmp(&b.0)).map(|&(_, h)| h)
}

/// `KH2_HUDKO=1`: show the knocked-out state (the actor's +0x7f8 bit 4, which the port does not
/// track: the death path that sets it, 0x186530 -> the actor's method +0x5c -> 0x186568 -> 0x186580,
/// is not ported). Display only.
fn ko_test() -> bool {
    static KO: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *KO.get_or_init(|| std::env::var("KH2_HUDKO").is_ok_and(|v| v == "1"))
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
    Some(DriveGauge { enabled: true, mode, points: n(0, 0), max: n(1, 5), fraction: n(2, 0), time_fill: None })
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
    // The HP ring, MP bar and drive gauge (its children) keep their 4:3 distance from the RIGHT screen edge (WP-FIX-12).
    scene.nodes[p.circle].edge = Edge::Right;
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

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::hud_gfx::SeqFile;
    use kh2_image::seqd::{Anim, AnimGroup, Seqd};

    /// A sequence of `n` groups, each one animation over frames 0..=9 that loops forever, except
    /// the listed `(group, plays, frames)` (shaped like the real files: the face's group 1 plays once
    /// over 60 frames, the gauge's circle 135 once over 31).
    fn seq(n: usize, once: &[(usize, u16, i32)]) -> SeqFile {
        let mut s = Seqd::default();
        for i in 0..n {
            let (loops, frames) = once.iter().find(|o| o.0 == i).map(|o| (o.1, o.2)).unwrap_or((0, 10));
            s.anims.push(Anim { frame_end: frames - 1, ..Anim::default() });
            s.groups.push(AnimGroup { first: i, count: 1, loops, loop_start: frames - 1, ..AnimGroup::default() });
        }
        SeqFile { seq: s, width: 1, height: 1, linear: Handle::default(), nearest: Handle::default() }
    }

    fn panel() -> (HudScene, SoraPanel, usize) {
        let mut scene = HudScene::default();
        scene.files = vec![seq(1, &[]), seq(160, &[(g::CIRCLE_HIT, 1, 31)]), seq(4, &[(g::FACE_HIT, 1, 60)])];
        let mut p = SoraPanel::default();
        build(&mut scene, &mut p, Some(2), 20);
        let face = p.face.unwrap();
        (scene, p, face)
    }

    fn ticks(scene: &mut HudScene, n: usize) {
        for _ in 0..n {
            scene.advance(1.0 / 60.0);
        }
    }

    #[test]
    fn panel_state_follows_the_game_rule() {
        // 0x157d40: knocked out first, then danger at hp <= max >> 2 (integer shift), else normal.
        assert_eq!(PanelState::of(5, 20, false), PanelState::Danger);
        assert_eq!(PanelState::of(6, 20, false), PanelState::Normal);
        assert_eq!(PanelState::of(4, 18, false), PanelState::Danger);
        assert_eq!(PanelState::of(5, 18, false), PanelState::Normal);
        assert_eq!(PanelState::of(20, 20, true), PanelState::KnockedOut);
        assert_eq!(g::FACE, [3, 0, 2]);
    }

    #[test]
    fn a_hit_shows_the_hurt_face_once_then_the_state_face() {
        let (mut scene, mut p, face) = panel();
        let group = |scene: &HudScene, n: usize| scene.nodes[n].player.group;
        panel_frame(&mut scene, &mut p, 20, 20, false);
        assert_eq!(group(&scene, face), Some(3));
        // A hit that keeps HP above a quarter: hurt face (group 1) and the circle's hit flash (135).
        panel_frame(&mut scene, &mut p, 15, 20, false);
        assert_eq!(group(&scene, face), Some(g::FACE_HIT));
        assert_eq!(group(&scene, p.circle), Some(g::CIRCLE_HIT));
        // The flash is 31 frames, played once: then the circle's state group.
        ticks(&mut scene, 31);
        assert_eq!(group(&scene, p.circle), Some(g::CIRCLE_HIT));
        ticks(&mut scene, 1);
        assert_eq!(group(&scene, p.circle), Some(137));
        // The hurt face is 60 frames: it hands over to the normal face on the 61st tick.
        ticks(&mut scene, 28);
        assert_eq!(group(&scene, face), Some(g::FACE_HIT));
        ticks(&mut scene, 1);
        assert_eq!(group(&scene, face), Some(3));
        // A hit into danger: the hurt face first, then the danger face; the back layers turn red at once.
        panel_frame(&mut scene, &mut p, 4, 20, false);
        assert_eq!(group(&scene, face), Some(g::FACE_HIT));
        assert_eq!(group(&scene, p.ring[0]), Some(122));
        ticks(&mut scene, 61);
        assert_eq!(group(&scene, face), Some(0));
        assert_eq!(group(&scene, p.circle), Some(132));
        // No change: nothing restarts.
        panel_frame(&mut scene, &mut p, 4, 20, false);
        assert_eq!(group(&scene, face), Some(0));
        // A heal out of danger switches straight to the normal face.
        panel_frame(&mut scene, &mut p, 20, 20, false);
        assert_eq!(group(&scene, face), Some(3));
        assert_eq!(group(&scene, p.circle), Some(137));
        // A heal during the hurt face cuts it short.
        panel_frame(&mut scene, &mut p, 10, 20, false);
        ticks(&mut scene, 5);
        panel_frame(&mut scene, &mut p, 12, 20, false);
        assert_eq!(group(&scene, face), Some(3));
    }

    #[test]
    fn danger_without_a_hit_switches_the_face_directly() {
        // A panel built at low HP takes the danger groups on its first frame (0x157230).
        let mut scene = HudScene::default();
        scene.files = vec![seq(1, &[]), seq(160, &[]), seq(4, &[])];
        let mut p = SoraPanel::default();
        build(&mut scene, &mut p, Some(2), 3);
        panel_frame(&mut scene, &mut p, 3, 20, false);
        assert_eq!(scene.nodes[p.face.unwrap()].player.group, Some(0));
        assert_eq!(scene.nodes[p.circle].player.group, Some(132));
        // The knocked-out state: face group 2, normal circle (the table's third column).
        panel_frame(&mut scene, &mut p, 3, 20, true);
        assert_eq!(scene.nodes[p.face.unwrap()].player.group, Some(2));
        assert_eq!(scene.nodes[p.circle].player.group, Some(137));
    }

    /// The danger alarm (0x157310 / 0x157040 / 0x157108): one start when danger begins, nothing while it
    /// lasts, one stop when it ends or Sora is knocked out; sound 9 of the boot bank is `PlaySe::number(9)`.
    #[test]
    fn the_danger_alarm_starts_once_and_stops_when_danger_ends() {
        let st = |hp: i32, ko: bool| PanelState::of(hp, 20, ko);
        let mut alarm = false;
        assert_eq!(alarm_step(&mut alarm, st(20, false)), None);
        assert_eq!(alarm_step(&mut alarm, st(6, false)), None, "6 > 20 >> 2: no danger");
        assert_eq!(alarm_step(&mut alarm, st(5, false)), Some(true), "5 <= 5: danger starts the alarm");
        assert_eq!(alarm_step(&mut alarm, st(3, false)), None, "a second hit in danger: nothing new");
        assert_eq!(alarm_step(&mut alarm, st(4, false)), None, "a heal that stays in danger: nothing");
        assert_eq!(alarm_step(&mut alarm, st(12, false)), Some(false), "healed out of danger: the stop");
        assert_eq!(alarm_step(&mut alarm, st(2, false)), Some(true));
        assert_eq!(alarm_step(&mut alarm, st(0, true)), Some(false), "knocked out: the stop (0x157310's first branch)");
        assert_eq!(alarm_step(&mut alarm, st(0, true)), None);
        assert_eq!(DANGER_ALARM_SE, 9, "bank 0 (se000.seb, driver slot 2), sound 9");
    }
}
