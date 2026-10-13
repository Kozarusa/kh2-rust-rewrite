//! The mission HUD (WP-7E): the orb counters and the match clock of the Struggle, drawn from the pictures of the
//! mission file (`msn/us/<name>.bar`, the (image, sequence) pair each header record names).
//!
//! What the game does (`SLUS_210.05`):
//! - **Counter** (record opcode 5; its show method `FUN_001cf8d8` builds the object `FUN_0014f540` ->
//!   `FUN_0014fa70`, drawn by `FUN_0014ff98`): a frame group chosen by the record's side (0: 0x24, 1: 0x25,
//!   2: 0x23); group 3 placed at the frame's anchor x (`FUN_00237550`) and the stacking y of the side's column
//!   (`FUN_0014f8b0`, 0 here: no other element stacks above [inferred]); group 9 at group 3's anchor; one digit
//!   node per digit of the maximum (`FUN_0017c6e0`), node i at ((n - 1 - i) * w, 0) from group 3 with w the
//!   digit group's width (+0x20 of the sequence group, `FUN_00237698`). The value's digits go through the table
//!   at 0x3585f0 (digit -> group; node 0 the units); leading zeros take group 0x16 (blank), the units never.
//!   With bit 0x40000000 of the maximum the maximum follows (groups 0x17 and the table at 0x358618); the
//!   Struggle's counters do not use it.
//! - **Clock** (record opcode 4; show `FUN_001cf748` -> `FUN_0014f2f8` -> `FUN_00150a30` on the object at
//!   `DAT_00358528`, laid out by `FUN_00150858`, drawn by `FUN_00150d40`): frame group by side (0: 0x1a, 1: 0x1b,
//!   2: 0x19); group 3 at the frame's anchor x; minutes, separator 0x17, seconds, separator 0x18, hundredths,
//!   two digits each (table 0x358680), placed one after the other by their widths; hidden from one hour on.
//!   Counting down with no minutes left, seconds 2, 1, 0 start groups 0x1f, 0x1e, 0x1d once each (with sound
//!   0x2b, not played here) and the end group 0x1c at 0.
//!
//! Children are placed from their parent's position plus their own offset (`FUN_00138ed0`, `FUN_00138ea8`);
//! the frame node itself stands at (0, 0): the sequence carries the screen position [inferred].

use bevy::prelude::*;

// The INFORMATION bar (WP-I1) sits beside this file; a submodule so `hud.rs` stays untouched.
#[path = "hud_info.rs"]
pub(crate) mod hud_info;

use super::hud_gfx::{Edge, HudScene, Node, SeqFile};
use crate::assets::Disc;
use crate::struggle::{Clock, Element, Mission};

/// Groups the code names directly (immediates of `FUN_0014fa70`, `FUN_0014ff98`, `FUN_00150a70`, `FUN_00150d40`).
const COUNTER_FRAME: [usize; 3] = [0x24, 0x25, 0x23];
const CLOCK_FRAME: [usize; 3] = [0x1a, 0x1b, 0x19];
const BLANK: usize = 0x16;
const SEP_MIN: usize = 0x17;
const SEP_SEC: usize = 0x18;
const WARN: [usize; 3] = [0x1d, 0x1e, 0x1f];
const TIME_UP: usize = 0x1c;
/// The hundredths factor of `FUN_0014c7f0` (float at 0x379168).
const FRAC_ADDR: u32 = 0x379168;

/// Digit tables of the executable: counter value 0x3585f0, clock 0x358680 (digit d -> sequence group).
#[derive(Clone, Debug, Default)]
struct Tables {
    counter: [usize; 10],
    clock: [usize; 10],
    frac: f32,
}

fn read_tables(elf: &[u8]) -> Option<Tables> {
    let word = |va: u32| crate::struggle::exe_word(elf, va);
    let mut t = Tables { frac: f32::from_bits(word(FRAC_ADDR)?), ..Tables::default() };
    for d in 0..10 {
        t.counter[d] = word(0x3585f0 + 4 * d as u32)? as usize;
        t.clock[d] = word(0x358680 + 4 * d as u32)? as usize;
    }
    Some(t)
}

/// The decimal digits of `v` for `n` nodes (node 0 the units) as groups, leading zeros blanked except the units.
pub fn counter_groups(v: i32, n: usize, table: &[usize; 10], blank: usize) -> Vec<usize> {
    let mut digits: Vec<usize> = (0..n).map(|i| ((v.max(0) / 10i32.pow(i as u32)) % 10) as usize).collect();
    let mut i = n.saturating_sub(1);
    while i > 0 && digits[i] == 0 {
        digits[i] = usize::MAX;
        i -= 1;
    }
    digits.into_iter().map(|d| if d == usize::MAX { blank } else { table[d] }).collect()
}

/// Number of digits of a maximum (`FUN_0017c6e0`).
pub fn digits_of(max: i32) -> usize {
    let (mut n, mut p) = (0, 1i64);
    while (max as i64) >= p && max > 0 {
        p *= 10;
        n += 1;
    }
    n
}

struct CounterHud {
    slot: usize,
    file: usize,
    frame: usize,
    g3: usize,
    g9: usize,
    digits: Vec<usize>,
    shown: Option<i32>,
}

struct ClockHud {
    file: usize,
    frame: usize,
    g3: usize,
    g9: usize,
    /// Minutes tens, units, seconds tens, units, hundredths tens, units.
    digits: [usize; 6],
    seps: [usize; 2],
    warn: [usize; 4],
    played: [bool; 4],
    shown: Option<i32>,
}

#[derive(Resource, Default)]
pub struct MissionHud {
    mission: Option<u32>,
    tables: Option<Tables>,
    loaded: bool,
    counters: Vec<CounterHud>,
    clock: Option<ClockHud>,
    edge: Vec<(usize, Edge)>,
    /// The layout pictures (WP-7E2): (mission, picture index, group) -> the node playing it.
    layouts: Vec<((u32, u8, usize), usize)>,
    /// The layout picture that has played through (its group finished): it stays gone until the event is over, so
    /// it is not started again by the next frame's check (WP-G1).
    layout_done: Option<(u32, u8, usize)>,
    /// Field HUD nodes this module hid while a layout hides the field HUD, to show again after it.
    hidden: Vec<usize>,
    /// The INFORMATION bar (draw tag 7: an event's field-HUD hide leaves it up).
    info: hud_info::InfoBar,
}


/// The target gauge's root group in the field gauge file (`hud_target`, the "HP" tag, group 38).
const TARGET_TAG: usize = 38;

/// The layout picture of the event playing (WP-7E2): from the event frame of its opcode 0x50 until the event
/// ends, a group of the record's picture (`Layout::group`: 1, or 2 for the failure) is drawn above the field (the 2D object at draw priority 26000,
/// `FUN_00225998`). The node stands at (0, 0): the sequence carries the screen position [inferred, as for the
/// counters' frames].
/// What a layout picture's node does this frame: `Hide` when its event does not want it, when it has played
/// through (the original's 2D object plays its group once and destroys itself, `FUN_00138aa8` vtable +0x14, so
/// nothing of it stays on screen), `Start` from its first frame, or `Keep` playing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PictureAct {
    Hide,
    Start,
    Keep,
}

pub fn picture_act(wanted: bool, done: bool, visible: bool, finished: bool) -> PictureAct {
    if !wanted || done || (visible && finished) {
        PictureAct::Hide
    } else if visible {
        PictureAct::Keep
    } else {
        PictureAct::Start
    }
}

fn layout_picture(mission: &Mission, hud: &mut MissionHud, scene: &mut HudScene, images: &mut Assets<Image>) {
    let want = mission.m.as_deref().and_then(|a| {
        let p = a.play.as_ref().filter(|p| p.picture_shown())?;
        let (i, s) = p.layout.picture_entries()?;
        Some((a, (a.id, p.layout.picture, p.layout.group()), i, s))
    });
    if want.is_none() {
        hud.layout_done = None;
    }
    for (key, node) in hud.layouts.clone() {
        if node == usize::MAX {
            continue;
        }
        let n = &mut scene.nodes[node];
        let wanted = want.as_ref().is_some_and(|w| w.1 == key);
        if picture_act(wanted, hud.layout_done == Some(key), n.visible, n.player.finished()) == PictureAct::Hide {
            if wanted && n.visible {
                info!("hud: mission layout picture {} group {} played through: removed", key.1, key.2);
                hud.layout_done = Some(key);
            }
            n.visible = false;
        }
    }
    let Some((a, key, i, s)) = want else { return };
    if hud.layout_done == Some(key) {
        return;
    }
    let node = match hud.layouts.iter().find(|(k, _)| *k == key) {
        Some((_, n)) => *n,
        None => {
            let file = kh2_bar::parse(&a.file).ok().and_then(|entries| {
                let img = kh2_image::imgd::decode(entries.get(i)?.data)?;
                let seq = kh2_image::seqd::parse(entries.get(s)?.data)?;
                scene.files.push(SeqFile::new(&img, seq, images));
                Some(scene.files.len() - 1)
            });
            let Some(file) = file else {
                warn!("hud: mission {} layout picture {} (entries {i}/{s}) not readable", a.name, key.1);
                hud.layouts.push((key, usize::MAX));
                return;
            };
            let n = add(scene, file, key.2);
            scene.nodes[n].visible = false;
            info!("hud: mission layout picture {} (entries {i}/{s}): group {}", key.1, key.2);
            hud.layouts.push((key, n));
            n
        }
    };
    if node == usize::MAX {
        return;
    }
    if !scene.nodes[node].visible {
        // The picture starts from its first frame each time its event shows it.
        scene.set_group(node, key.2);
        scene.nodes[node].visible = true;
    }
}

/// Every node of this module (the counters, the clock, the layout pictures).
fn own_nodes(hud: &MissionHud) -> Vec<usize> {
    let mut v: Vec<usize> = hud.counters.iter().flat_map(|c| [c.frame, c.g3, c.g9].into_iter().chain(c.digits.iter().copied())).collect();
    if let Some(c) = &hud.clock {
        v.extend([c.frame, c.g3, c.g9].into_iter().chain(c.digits).chain(c.seps).chain(c.warn));
    }
    v.extend(hud.layouts.iter().map(|(_, n)| *n));
    v.extend(hud.info.nodes());
    v
}

/// While a layout's event kind hides the field HUD (`FUN_00196760` -> `FUN_0014ef48`: draw tags 3 to 6), every
/// other node of the scene is hidden; the ones hidden here come back when it ends (`FUN_00196790`) [inferred: which
/// of the app's HUD modules are the tags 3 to 5 was not traced; tag 6 is the target gauge].
fn field_hud_hide(hidden_now: bool, hud: &mut MissionHud, scene: &mut HudScene) {
    if hidden_now {
        let own = own_nodes(hud);
        for (i, n) in scene.nodes.iter_mut().enumerate() {
            if n.visible && !own.contains(&i) {
                n.visible = false;
                if !hud.hidden.contains(&i) {
                    hud.hidden.push(i);
                }
            }
        }
    } else {
        for i in hud.hidden.drain(..) {
            if let Some(n) = scene.nodes.get_mut(i) {
                n.visible = true;
            }
        }
    }
}

/// The target gauge (`hud_target`, field gauge file group 38 and the nodes under it) is hidden while a counter
/// stands on screen side 1, where the gauge would cover it and the TIME plate. [inferred] The game binds the gauge
/// only when the player's ability bit 19 (Scan, ability 138) is set (`FUN_0019b4a8`: status +0x1cc bit 0x80000)
/// and the target's +0x108 bit 0x4000 is clear; no mission rule that hides it was found (searched: `FUN_0019b4a8`,
/// `FUN_0019b3f0`, `FUN_00154498`, `FUN_00153f98`, `FUN_0013da78`, `FUN_0014f8b0`, the record handlers
/// `FUN_001b9378`, `FUN_001d6110`; the opponents' scripts set only +0x108 flags 3 and 6 through host 1.70).
fn hide_target_gauge(scene: &mut HudScene) {
    let gaug = super::hud_gauge::GAUG;
    let roots: Vec<bool> = (0..scene.nodes.len())
        .map(|i| {
            let mut k = i;
            while let Some(p) = scene.nodes[k].parent {
                k = p;
            }
            scene.nodes[k].file == gaug && scene.nodes[k].player.group == Some(TARGET_TAG)
        })
        .collect();
    for (n, under) in scene.nodes.iter_mut().zip(roots) {
        if under {
            n.visible = false;
        }
    }
}

fn add(scene: &mut HudScene, file: usize, group: usize) -> usize {
    let mut n = Node::new(file, None);
    n.visible = true;
    let i = scene.add(n);
    scene.set_group(i, group);
    i
}

/// The anchor a node's group gives its children: the translation of its anchor animation (flag 2) at the
/// node's frame. [inferred] The pivot is left out: `Seqd::anchor` adds it, and with it the Struggle pictures'
/// anchors (pivots -256 / -100) would put the digits off the screen; the translations put them beside the
/// ORBS plates and under the TIME plate.
fn anchor(scene: &HudScene, node: usize) -> [f32; 2] {
    let n = &scene.nodes[node];
    let (Some(g), Some(f)) = (n.player.group, scene.files.get(n.file)) else { return [0.0; 2] };
    let Some(grp) = f.seq.groups.get(g) else { return [0.0; 2] };
    let frame = n.player.frame();
    f.seq
        .anims
        .iter()
        .skip(grp.first)
        .take(grp.count)
        .find(|a| a.flags & 2 != 0 && frame >= a.frame_start && frame <= a.frame_end)
        .map_or([0.0; 2], |a| {
            let span = (a.frame_end - a.frame_start).max(1) as f32;
            let t = ((frame - a.frame_start) as f32 / span).clamp(0.0, 1.0);
            let l = |v: [i32; 2]| v[0] as f32 + (v[1] - v[0]) as f32 * t;
            [l(a.translate_x), l(a.translate_y)]
        })
}

/// The width a node's group advances by (+0x20 of the sequence group, `FUN_00237698`).
fn width(scene: &HudScene, node: usize) -> f32 {
    let n = &scene.nodes[node];
    n.player.group.and_then(|g| scene.files.get(n.file).and_then(|f| f.seq.groups.get(g))).map_or(0.0, |g| g.unk20 as f32)
}

fn set(scene: &mut HudScene, node: usize, group: usize) {
    if scene.nodes[node].player.group != Some(group) {
        scene.set_group(node, group);
    }
}

/// The picture of an element as a sequence file in the scene.
fn load_picture(file: &[u8], e: &Element, scene: &mut HudScene, images: &mut Assets<Image>) -> Option<usize> {
    let (i, s) = e.picture_entries()?;
    let entries = kh2_bar::parse(file).ok()?;
    let img = kh2_image::imgd::decode(entries.get(i)?.data)?;
    let seq = kh2_image::seqd::parse(entries.get(s)?.data)?;
    scene.files.push(SeqFile::new(&img, seq, images));
    Some(scene.files.len() - 1)
}

pub fn update(disc: Res<Disc>, time: Res<Time>, mission: Res<Mission>, mut hud: ResMut<MissionHud>, mut scene: ResMut<HudScene>, mut images: ResMut<Assets<Image>>) {
    if !hud.loaded {
        hud.loaded = true;
        hud.tables = disc.read_root("SLUS_210.05").and_then(|e| read_tables(&e));
        if hud.tables.is_none() {
            warn!("hud: the mission digit tables are not readable from SLUS_210.05; no mission HUD");
        }
    }
    hud_info::update(&mut scene, &mission, &mut hud.info, time.delta_secs());
    let Some(t) = hud.tables.clone() else { return };
    layout_picture(&mission, &mut hud, &mut scene, &mut images);
    field_hud_hide(mission.hud_hidden(), &mut hud, &mut scene);
    let Some(a) = mission.m.as_deref() else {
        // No manager: the elements go (FUN_001b90d0 destroys them with the manager).
        if hud.mission.take().is_some() {
            for n in hud.counters.drain(..).flat_map(|c| [c.frame, c.g3, c.g9].into_iter().chain(c.digits)).collect::<Vec<_>>() {
                scene.nodes[n].visible = false;
            }
            if let Some(c) = hud.clock.take() {
                for n in [c.frame, c.g3, c.g9].into_iter().chain(c.digits).chain(c.seps).chain(c.warn) {
                    scene.nodes[n].visible = false;
                }
            }
        }
        return;
    };
    if !a.shown {
        return;
    }
    if a.elements.iter().any(|e| e.op == 5 && e.side == 1) {
        hide_target_gauge(&mut scene);
    }
    if hud.mission != Some(a.id) {
        hud.mission = Some(a.id);
        hud.counters.clear();
        hud.clock = None;
        hud.edge.clear();
        for e in &a.elements {
            let Some(file) = load_picture(&a.file, e, &mut scene, &mut images) else {
                warn!("hud: mission {} element {:?}: picture not readable", a.name, e);
                continue;
            };
            let side = (e.side as usize).min(2);
            match e.op {
                5 => {
                    let Some(c) = a.counters.get(e.slot as usize) else { continue };
                    let frame = add(&mut scene, file, COUNTER_FRAME[side]);
                    let g3 = add(&mut scene, file, 3);
                    let g9 = add(&mut scene, file, 9);
                    let digits = (0..digits_of(c.max)).map(|_| add(&mut scene, file, t.counter[0])).collect::<Vec<_>>();
                    info!("hud: mission counter slot {} (side {side}): {} digits, frame group {:#x}", e.slot, digits.len(), COUNTER_FRAME[side]);
                    hud.counters.push(CounterHud { slot: e.slot as usize, file, frame, g3, g9, digits, shown: None });
                }
                4 => {
                    let frame = add(&mut scene, file, CLOCK_FRAME[side]);
                    let g3 = add(&mut scene, file, 3);
                    let g9 = add(&mut scene, file, 9);
                    let digits = [(); 6].map(|_| add(&mut scene, file, t.clock[0]));
                    let seps = [add(&mut scene, file, SEP_MIN), add(&mut scene, file, SEP_SEC)];
                    let warn = [WARN[0], WARN[1], WARN[2], TIME_UP].map(|g| {
                        let n = add(&mut scene, file, g);
                        scene.nodes[n].visible = false;
                        n
                    });
                    info!("hud: mission clock (side {side}), frame group {:#x}", CLOCK_FRAME[side]);
                    hud.clock = Some(ClockHud { file, frame, g3, g9, digits, seps, warn, played: [false; 4], shown: None });
                }
                _ => {}
            }
        }
    }
    let hud = &mut *hud;
    // Counters (FUN_0014ff98).
    for c in &mut hud.counters {
        let v = a.counter_value(c.slot);
        if c.shown != Some(v) {
            c.shown = Some(v);
            for (node, g) in c.digits.iter().zip(counter_groups(v, c.digits.len(), &t.counter, BLANK)) {
                set(&mut scene, *node, g);
            }
        }
        let fa = anchor(&scene, c.frame);
        let g3 = [fa[0], 0.0];
        let ga = anchor(&scene, c.g3);
        scene.nodes[c.g3].pos = g3;
        scene.nodes[c.g9].pos = [g3[0] + ga[0], g3[1] + ga[1]];
        // The digits hang from group 3 (+0x180 = the group-3 node): its position plus its anchor plus their offset.
        let n = c.digits.len();
        for (i, node) in c.digits.clone().into_iter().enumerate() {
            let w = width(&scene, node);
            scene.nodes[node].pos = [g3[0] + ga[0] + (n - 1 - i) as f32 * w, g3[1] + ga[1]];
        }
        if std::env::var("KH2_HUDLOG").is_ok() && c.shown.is_some() {
            info!("hud: mission counter {} layout: frame anchor {fa:?}, group 3 anchor {ga:?}, digit width {}", c.slot, width(&scene, c.digits[0]));
        }
        let _ = c.file;
    }
    // The clock (FUN_00150858 layout, FUN_00150d40 draw).
    if let Some(c) = hud.clock.as_mut() {
        let frames = a.clock.shown;
        let (h, m, s, f) = Clock::split(frames, t.frac);
        let all = [c.frame, c.g3, c.g9].into_iter().chain(c.digits).chain(c.seps).collect::<Vec<_>>();
        for n in &all {
            scene.nodes[*n].visible = h == 0;
        }
        if c.shown != Some(frames) && h == 0 {
            c.shown = Some(frames);
            for (k, v) in [m, s, f].into_iter().enumerate() {
                set(&mut scene, c.digits[2 * k], t.clock[((v / 10) % 10) as usize]);
                set(&mut scene, c.digits[2 * k + 1], t.clock[(v % 10) as usize]);
            }
        }
        let fa = anchor(&scene, c.frame);
        let g3 = [fa[0], 0.0];
        scene.nodes[c.g3].pos = g3;
        let ga = anchor(&scene, c.g3);
        scene.nodes[c.g9].pos = [g3[0] + ga[0], g3[1] + ga[1]];
        let order = [c.digits[0], c.digits[1], c.seps[0], c.digits[2], c.digits[3], c.seps[1], c.digits[4], c.digits[5]];
        let mut x = 0.0;
        for node in order {
            scene.nodes[node].pos = [g3[0] + ga[0] + x, g3[1] + ga[1]];
            x += width(&scene, node);
        }
        // The last seconds of a countdown (FUN_00150d40): 2, 1, 0 once each, then the end group at 0.
        if a.clock.down && h == 0 && m == 0 {
            // Seconds 2, 1, 0 start groups 0x1f, 0x1e, 0x1d (`WARN[sec]`).
            for sec in [2usize, 1, 0] {
                if s as usize == sec && !c.played[sec] {
                    c.played[sec] = true;
                    scene.nodes[c.warn[sec]].visible = true;
                    scene.set_group(c.warn[sec], WARN[sec]);
                    info!("hud: mission clock: {sec} seconds left (group {:#x})", WARN[sec]);
                }
            }
            if frames == 0 && !c.played[3] && c.played[0] {
                c.played[3] = true;
                scene.nodes[c.warn[3]].visible = true;
                scene.set_group(c.warn[3], TIME_UP);
                info!("hud: mission clock: time up (group {TIME_UP:#x})");
            }
        }
        // A last-seconds group goes when it has played (its object is only drawn while `FUN_00139fe0` says it runs).
        for n in c.warn {
            if scene.nodes[n].visible && scene.nodes[n].player.finished() {
                scene.nodes[n].visible = false;
            }
        }
        let _ = c.file;
    }
    // Widescreen anchoring (the user's rule, docs/hud.md): an element left of the centre keeps its distance
    // from the left edge, right of it from the right edge.
    let groups: Vec<(Vec<usize>, f32)> = hud
        .counters
        .iter()
        .map(|c| ([c.frame, c.g3, c.g9].into_iter().chain(c.digits.iter().copied()).collect(), anchor(&scene, c.g3)[0]))
        .collect();
    for (nodes, x) in groups {
        let edge = if x < 200.0 {
            Edge::Left
        } else if x > 312.0 {
            Edge::Right
        } else {
            Edge::Center
        };
        for n in nodes {
            scene.nodes[n].edge = edge;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_struggle_pictures_have_the_groups_the_code_uses() {
        let Some(iso) = crate::assets::iso_path().ok().filter(|p| p.exists()) else {
            eprintln!("skipped: no disc (set KH2_ISO)");
            return;
        };
        let disc = Disc::open(&iso);
        let file = disc.read("msn/us/TT04_MS00.bar").expect("mission file");
        let entries = kh2_bar::parse(&file).unwrap();
        for (img, seq, groups) in [(12, 13, vec![0x24, 0x25, 3, 9, 12, 21, BLANK]), (16, 17, vec![0x19, 3, 9, 12, 21, SEP_MIN, SEP_SEC, 0x1c, 0x1d, 0x1f])] {
            assert!(kh2_image::imgd::decode(entries[img].data).is_some());
            let s = kh2_image::seqd::parse(entries[seq].data).expect("sequence");
            for g in groups {
                let grp = s.groups.get(g).unwrap_or_else(|| panic!("group {g:#x} of entry {seq}"));
                let anims: Vec<_> = s.anims.iter().skip(grp.first).take(grp.count).map(|a| (a.flags, a.translate_x, a.translate_y, a.frame_start, a.frame_end)).collect();
                eprintln!("entry {seq} group {g:#x}: x {} y {} width {} anims {anims:?} anchor {:?}", grp.x, grp.y, grp.unk20, s.anchor(g, 0, &kh2_image::seqd::Ext::default()));
            }
        }
    }

    #[test]
    fn a_layout_picture_goes_when_it_has_played_through_or_its_event_is_over() {
        // wanted, done, visible, finished
        assert_eq!(picture_act(true, false, false, false), PictureAct::Start);
        assert_eq!(picture_act(true, false, true, false), PictureAct::Keep);
        // Played through: removed, whatever the event still wants (the original's 2D object destroys itself).
        assert_eq!(picture_act(true, false, true, true), PictureAct::Hide);
        // Removed once: not started again by the next frame's check while the same event goes on.
        assert_eq!(picture_act(true, true, false, true), PictureAct::Hide);
        // The event is over: gone.
        assert_eq!(picture_act(false, false, true, false), PictureAct::Hide);
    }

    #[test]
    fn the_start_picture_group_plays_through_in_90_ticks() {
        let Some(iso) = crate::assets::iso_path().ok().filter(|p| p.exists()) else {
            eprintln!("skipped: no disc (set KH2_ISO)");
            return;
        };
        let disc = Disc::open(&iso);
        let file = disc.read("msn/us/TT04_MS00.bar").expect("mission file");
        let entries = kh2_bar::parse(&file).unwrap();
        let seq = kh2_image::seqd::parse(entries[4].data).expect("st_t sequence");
        let mut player = kh2_image::seqd::Player::default();
        player.set_group(&seq, 1);
        let mut ticks = 0;
        while !player.finished() && ticks < 1000 {
            player.tick(1.0);
            ticks += 1;
        }
        // One tick starts the group, then 90 frames play; the node is then removed (`picture_act`).
        assert!(player.finished() && (90..=92).contains(&ticks), "{ticks}");
    }

    #[test]
    fn counter_digits_blank_their_leading_zeros() {
        let table = [12, 13, 14, 15, 16, 17, 18, 19, 20, 21];
        assert_eq!(counter_groups(100, 3, &table, BLANK), vec![12, 12, 13]);
        assert_eq!(counter_groups(95, 3, &table, BLANK), vec![17, 21, BLANK]);
        assert_eq!(counter_groups(7, 3, &table, BLANK), vec![19, BLANK, BLANK]);
        assert_eq!(counter_groups(0, 3, &table, BLANK), vec![12, BLANK, BLANK]);
        assert_eq!((digits_of(200), digits_of(99), digits_of(100), digits_of(0)), (3, 2, 3, 0));
    }
}
