//! Party members' panels (WP-4B3): the circle, the HP rings and the MP ring of each companion,
//! built the way the game's party panel class does (0x154a78, objects of 0x1570 bytes, two of
//! them at 0x355720 and 0x355724 for slots 0 and 1, 0x13d738). `party::party_hud` fills `PartyMembers`
//! from the members' status blocks each tick (WP-P3b); with no member nothing is drawn.
//!
//! Objects (gaug sequence groups, decimal; 0x154a78): circle 71 at +0x384 (Z 0), ring 64 at
//! +0x514 (Z 100), ring 65 at +0x6a4 (Z 1000), ring 66 at +0x834 (Z 10000), the member's own
//! `face` at +0x40 (0x153af0 with the member index, slot + 1), and five mask objects (0x13e010):
//! +0x9d0 and +0x10c0 of type 0 (a 64x64 square at Z 0, read as a depth clear) and +0xc20, +0xe70,
//! +0x1310 of type 1 at Z 200, 2000 and 20000, one for each ring. A slot's panel sits `unk20` of
//! group 71 times the slot below the first (`FUN_00237698(panel + 0x39c)` times +0x30 at the end
//! of 0x154a78 through 0x138ea8).
//!
//! Masks (0x13e010, drawn by 0x13ea18 with 0x13e968): a type-1 mask is two rectangles written to
//! the depth buffer only (TEST 0x32001: alpha test never with ZB_ONLY, Z test always), so the ring
//! drawn after it at a lower Z is hidden where they lie. The first is a 64x64 square whose centre
//! is (-32, -32) from the ring's centre, turned about that centre by the mask's angle (+0x110;
//! positive is clockwise on screen); the second is an axis-aligned rectangle at +0x140 with its
//! position and size set per frame. Sora's own rule (0x1552a0) with this geometry gives exactly the
//! 270 degree clockwise sweep from west that docs/hud.md records, which fixes the reading.
//!
//! Party rules: HP (0x154818, called twice by 0x1551a0: slot 1 with the trail value at panel +0x24
//! over the maximum, slot 2 with HP over maximum from the member's +0x48c): angle = f * pi - pi/2,
//! second rectangle the north-west quadrant (position (-64, -64), size 64x64) while f < 0.5, gone
//! (size 0) otherwise. Hidden: at f = 0 the west half, at f = 0.5 the north-west quadrant, at f = 1
//! the north-east quadrant; the visible arc grows from south clockwise through west to north by f x
//! 180 degrees, so the ring art is the west half of a ring (inferred from the rule; the art's
//! extent was not measured). MP (0x154930 with 0x166790: +0x180 over +0x184): angle = (1 - f) x pi,
//! second rectangle the north-east quadrant while f < 0.5, a 1x1 point at (-64, 0) otherwise; the
//! visible arc grows from south counter-clockwise through east to north by f x 180 degrees.
//! Constants: 0x358930 = 0.5, 0x3792e8 = pi, 0x3792ec = -pi/2, 0x3792f0 = 1, 0x3792f4 = pi.
//! The per-frame 0x155238 only runs while the panel has a member (+0x34) without flag 0x100.

use std::f32::consts::PI;

use std::collections::HashMap;

use bevy::prelude::*;

use super::hud_gauge::GAUG;
use super::hud_gfx::{self, Edge, HudScene, Node};
use crate::assets::Disc;

/// Panels the game has (0x13d738 allocates two).
pub const SLOTS: usize = 2;

mod g {
    pub const CIRCLE: usize = 71;
    /// The HP ring drawn for the value before the last change (0x1551a0 slot 1), the current HP
    /// (slot 2) and the MP ring.
    pub const RING_TRAIL: usize = 64;
    pub const RING_HP: usize = 65;
    pub const RING_MP: usize = 66;
}

/// One companion as the panel reads it: the two values of +0x48c (HP, maximum), +0x180 and +0x184
/// (MP, maximum), and the scene file of the member's `face` pair when it was loaded.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct PartyMember {
    pub hp: i32,
    pub max_hp: i32,
    pub mp: i32,
    pub max_mp: i32,
    pub face: Option<usize>,
}

/// The party, in slot order (filled by `party::party_hud`); empty draws nothing.
/// `KH2_HUDPARTY=<hp>:<max>:<mp>:<mpmax>[;...]` fills it instead, for a look at the geometry.
#[derive(Resource, Default, Clone, Debug)]
pub struct PartyMembers(pub Vec<PartyMember>);

#[derive(Clone, Copy, Default)]
struct Slot {
    circle: usize,
    rings: [usize; 3],
    face: Option<usize>,
    last_hp: i32,
    trail_hp: i32,
    shown: bool,
}

#[derive(Resource, Default)]
pub struct PartyNodes {
    built: bool,
    slots: [Slot; SLOTS],
    /// Scene file of each member model's `face` pair, by model name (`None`: not loadable).
    faces: HashMap<String, Option<usize>>,
    /// The model each slot's portrait was set up for: a change (a member leaves or swaps slots) reloads it.
    models: [Option<String>; SLOTS],
}

/// Index of the `face` pair in a member's object file: the first two adjacent entries tagged `face` (IMGD, SEQD;
/// member resource +0x7b0 tag "face", docs/hud.md), not simply the file's last pair.
pub fn face_pair(tags: &[&str]) -> Option<usize> {
    tags.windows(2).position(|w| w[0] == "face" && w[1] == "face")
}

/// The member's `face` picture from the bytes of `obj/<model>.apdx`.
fn load_face(data: &[u8], images: &mut Assets<Image>) -> Option<hud_gfx::SeqFile> {
    let entries = kh2_bar::parse(data).ok()?;
    let i = face_pair(&entries.iter().map(|e| e.tag).collect::<Vec<_>>())?;
    let img = kh2_image::imgd::decode(entries[i].data)?;
    let seq = kh2_image::seqd::parse(entries[i + 1].data)?;
    Some(hud_gfx::SeqFile::new(&img, seq, images))
}

/// The model each slot's portrait comes from. The member's own object file holds the `face` pair
/// (member resource +0x7b0, tag "face", fetched by 0x16b998 as for Sora's `P_EX100`), so the model name
/// is the key. With no member in the field, the harness fill `KH2_HUDPARTY` shows slot 0 as Donald
/// (`P_EX020`) and slot 1 as Goofy (`P_EX030`), the members' slots in docs/party.md (the panel of slot 1
/// lies above that of slot 0, as in the user's picture of the original: Goofy on top, Donald below).
fn model_of(slot: usize, members: &[&str]) -> Option<String> {
    if let Some(m) = members.get(slot) {
        return Some((*m).to_string());
    }
    if !members.is_empty() {
        return None;
    }
    ["P_EX020", "P_EX030"].get(slot).map(|m| (*m).to_string())
}

/// Fraction of a value over its maximum as 0x166730 computes it (float division; 0 for no maximum).
pub fn fraction(value: i32, max: i32) -> f32 {
    if max <= 0 { 0.0 } else { (value as f32 / max as f32).clamp(0.0, 1.0) }
}

/// The visible wedge of the HP ring for fraction `f`: (start angle clockwise from west, sweep).
/// Starts at south and runs clockwise through west.
pub fn hp_wedge(f: f32) -> (f32, f32) {
    (1.5 * PI, f * PI)
}

/// The visible wedge of the MP ring: ends at south, runs counter-clockwise through east; as a
/// clockwise wedge it starts `f x 180` degrees before south.
pub fn mp_wedge(f: f32) -> (f32, f32) {
    (1.5 * PI - f * PI, f * PI)
}

fn build(scene: &mut HudScene, nodes: &mut PartyNodes) {
    let step = scene.files.get(GAUG).and_then(|f| f.seq.groups.get(g::CIRCLE)).map(|g| g.unk20 as f32).unwrap_or(0.0);
    for s in 0..SLOTS {
        let mut circle = Node::new(GAUG, None);
        circle.visible = false;
        // Party panels sit beside Sora's ring: on the RIGHT screen edge (WP-FIX-12).
        circle.edge = Edge::Right;
        // The panel's position is the circle group's own anchor; a later slot's is `unk20` below.
        circle.off = [0.0, step * s as f32];
        let circle = scene.add(circle);
        scene.set_group(circle, g::CIRCLE);
        let mut rings = [0usize; 3];
        for (k, group) in [g::RING_TRAIL, g::RING_HP, g::RING_MP].into_iter().enumerate() {
            let mut n = Node::new(GAUG, Some(circle));
            n.visible = false;
            let i = scene.add(n);
            scene.set_group(i, group);
            rings[k] = i;
        }
        nodes.slots[s] = Slot { circle, rings, ..Slot::default() };
    }
    nodes.built = true;
}

fn demo() -> Option<Vec<PartyMember>> {
    let v = std::env::var("KH2_HUDPARTY").ok()?;
    let members = v
        .split(';')
        .filter_map(|m| {
            let n: Vec<i32> = m.split(':').filter_map(|x| x.trim().parse().ok()).collect();
            (n.len() >= 4).then(|| PartyMember { hp: n[0], max_hp: n[1], mp: n[2], max_mp: n[3], face: None })
        })
        .collect::<Vec<_>>();
    (!members.is_empty()).then_some(members)
}

#[allow(clippy::too_many_arguments)]
pub fn update(
    mut scene: ResMut<HudScene>,
    mut party: ResMut<PartyMembers>,
    mut nodes: ResMut<PartyNodes>,
    mut demo_done: Local<bool>,
    disc: Option<Res<Disc>>,
    mut images: ResMut<Assets<Image>>,
    in_field: Query<&crate::party::PartyMember>,
) {
    if scene.files.len() <= GAUG {
        return;
    }
    if !*demo_done {
        *demo_done = true;
        if let Some(m) = demo() {
            party.0 = m;
        }
    }
    if !nodes.built {
        build(&mut scene, &mut nodes);
    }
    // Load each shown member's face once (the `face` pair of `obj/<model>.apdx`).
    if let Some(disc) = disc {
        let mut by_slot: Vec<(u8, &str)> = in_field.iter().map(|m| (m.slot, m.model.as_str())).collect();
        by_slot.sort_by_key(|(s, _)| *s);
        let names: Vec<&str> = by_slot.iter().map(|(_, m)| *m).collect();
        for s in 0..SLOTS {
            let want = if s < party.0.len() { model_of(s, &names) } else { None };
            if nodes.models[s] != want {
                // The slot's member changed (left, or swapped slots): drop the old portrait, load the new one.
                if nodes.models[s].is_some() {
                    if let Some(f) = nodes.slots[s].face.take() {
                        scene.nodes[f].visible = false;
                    }
                    if let Some(p) = party.0.get_mut(s) {
                        p.face = None;
                    }
                    info!("hud: party slot {s}: portrait {:?} -> {:?}", nodes.models[s], want);
                }
                nodes.models[s] = want.clone();
            }
            if s >= party.0.len() || party.0[s].face.is_some() {
                continue;
            }
            let Some(model) = want else { continue };
            let file = match nodes.faces.get(&model) {
                Some(f) => *f,
                None => {
                    let f = disc.read(&format!("obj/{model}.apdx")).and_then(|d| load_face(&d, &mut images)).map(|f| {
                        scene.files.push(f);
                        scene.files.len() - 1
                    });
                    nodes.faces.insert(model, f);
                    f
                }
            };
            party.0[s].face = file;
        }
    }
    for s in 0..SLOTS {
        let slot = &mut nodes.slots[s];
        let Some(m) = party.0.get(s).copied() else {
            if slot.shown {
                slot.shown = false;
                scene.nodes[slot.circle].visible = false;
                for r in slot.rings {
                    scene.nodes[r].visible = false;
                }
                if let Some(f) = slot.face {
                    scene.nodes[f].visible = false;
                }
            }
            continue;
        };
        if !slot.shown {
            slot.shown = true;
            slot.last_hp = m.hp;
            slot.trail_hp = m.hp;
            scene.nodes[slot.circle].visible = true;
            for r in slot.rings {
                scene.nodes[r].visible = true;
            }
        }
        // The member's own face, a child of the circle, added when its file is known.
        if let (None, Some(file)) = (slot.face, m.face) {
            let i = scene.add(Node::new(file, Some(slot.circle)));
            scene.set_group(i, super::hud_gauge::FACE_NORMAL);
            slot.face = Some(i);
        }
        if let Some(f) = slot.face {
            scene.nodes[f].visible = true;
        }
        // The trail value (panel +0x24): its update for the party was not read; Sora's rule
        // (0x1574d8: the HP before a drop, the new HP after a rise) is used, marked as inferred.
        if m.hp != slot.last_hp {
            slot.trail_hp = if slot.last_hp < m.hp { m.hp } else { slot.last_hp };
            slot.last_hp = m.hp;
        }
        let wedges = [hp_wedge(fraction(slot.trail_hp, m.max_hp)), hp_wedge(fraction(m.hp, m.max_hp)), mp_wedge(fraction(m.mp, m.max_mp))];
        for (r, (start, sweep)) in slot.rings.into_iter().zip(wedges) {
            let n = &mut scene.nodes[r];
            n.sweep_start = start;
            n.sweep = Some(sweep);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn faces_come_from_the_members_models() {
        assert_eq!(model_of(0, &["P_EX020", "P_EX030"]).as_deref(), Some("P_EX020"));
        assert_eq!(model_of(1, &["P_EX020"]), None, "a member that is not in the field has no face");
        assert_eq!(model_of(0, &[]).as_deref(), Some("P_EX020"), "the harness fill shows Donald in slot 0");
        assert_eq!(model_of(1, &[]).as_deref(), Some("P_EX030"));
    }

    #[test]
    fn the_face_pair_is_found_by_tag_not_by_position() {
        assert_eq!(face_pair(&["mdlx", "face", "face", "eff", "eff2"]), Some(1));
        assert_eq!(face_pair(&["face", "mdlx"]), None);
        assert_eq!(face_pair(&[]), None);
    }

    /// The members' portraits load through the tag lookup (skipped without a disc), and it agrees with the old
    /// "last pair" reading for the three models the HUD uses.
    #[test]
    fn member_faces_load_by_tag() {
        let Ok(iso) = std::env::var("KH2_ISO") else {
            eprintln!("skipped: no disc (set KH2_ISO)");
            return;
        };
        let Ok(mut a) = kh2_archive::Archive::open(&iso) else { return };
        let mut images = Assets::<Image>::default();
        for m in ["P_EX020", "P_EX030", "P_EX100"] {
            let d = a.read(&format!("obj/{m}.apdx")).unwrap();
            let by_tag = load_face(&d, &mut images).unwrap_or_else(|| panic!("{m}: no face pair by tag"));
            let last = hud_gfx::SeqFile::from_bar(&d, &mut images).pop().unwrap();
            assert_eq!((by_tag.width, by_tag.height), (last.width, last.height), "{m}");
        }
    }

    #[test]
    fn wedges_follow_the_mask_rules() {
        // HP: nothing at 0, the south-west quadrant at a half, the west half at full.
        assert_eq!(hp_wedge(0.0), (1.5 * PI, 0.0));
        assert_eq!(hp_wedge(0.5), (1.5 * PI, PI / 2.0));
        assert_eq!(hp_wedge(1.0), (1.5 * PI, PI));
        // MP: ends at south; at a half it is the south-east quadrant (starts at east).
        let (s, w) = mp_wedge(0.5);
        assert!((s - PI).abs() < 1e-6 && (w - PI / 2.0).abs() < 1e-6);
        let (s, w) = mp_wedge(1.0);
        assert!((s - PI / 2.0).abs() < 1e-6 && (w - PI).abs() < 1e-6);
        assert_eq!(fraction(30, 60), 0.5);
        assert_eq!(fraction(5, 0), 0.0);
        assert_eq!(fraction(90, 60), 1.0);
    }
}
