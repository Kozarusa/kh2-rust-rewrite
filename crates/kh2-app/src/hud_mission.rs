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
//! Children are placed from their parent's position plus their own offset (`FUN_00138ed0`, `FUN_00138ea8`);
//! the frame node itself stands at (0, 0): the sequence carries the screen position [inferred].

use bevy::prelude::*;

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

pub fn update(disc: Res<Disc>, mission: Res<Mission>, mut hud: ResMut<MissionHud>, mut scene: ResMut<HudScene>, mut images: ResMut<Assets<Image>>) {
    if !hud.loaded {
        hud.loaded = true;
        hud.tables = disc.read_root("SLUS_210.05").and_then(|e| read_tables(&e));
        if hud.tables.is_none() {
            warn!("hud: the mission digit tables are not readable from SLUS_210.05; no mission HUD");
        }
    }
    let Some(t) = hud.tables.clone() else { return };
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
            for (k, sec) in [2, 1, 0].into_iter().enumerate() {
                let slot = 2 - k;
                if s == sec && !c.played[slot] {
                    c.played[slot] = true;
                    scene.nodes[c.warn[slot]].visible = true;
                    scene.set_group(c.warn[slot], WARN[slot]);
                    info!("hud: mission clock: {sec} seconds left (group {:#x})", WARN[slot]);
                }
            }
            if frames == 0 && !c.played[3] && c.played[0] {
                c.played[3] = true;
                scene.nodes[c.warn[3]].visible = true;
                scene.set_group(c.warn[3], TIME_UP);
                info!("hud: mission clock: time up (group {TIME_UP:#x})");
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
    fn counter_digits_blank_their_leading_zeros() {
        let table = [12, 13, 14, 15, 16, 17, 18, 19, 20, 21];
        assert_eq!(counter_groups(100, 3, &table, BLANK), vec![12, 12, 13]);
        assert_eq!(counter_groups(95, 3, &table, BLANK), vec![17, 21, BLANK]);
        assert_eq!(counter_groups(7, 3, &table, BLANK), vec![19, BLANK, BLANK]);
        assert_eq!(counter_groups(0, 3, &table, BLANK), vec![12, BLANK, BLANK]);
        assert_eq!((digits_of(200), digits_of(99), digits_of(100), digits_of(0)), (3, 2, 3, 0));
    }
}
