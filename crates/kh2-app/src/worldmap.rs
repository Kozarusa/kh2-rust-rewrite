//! The world map's objects (WP-WM3, stage W5 of docs/worldmap.md).
//!
//! The map is an ordinary field area (`wm00`); its screen is made of objects of `00objentry` type 0x11
//! (`FUN_0016e8c0` -> `FUN_002c58c8`), told apart by the row's second byte: 0 a world symbol
//! (`FUN_002c64d0`), 1 a Gummi route marker (`FUN_002c8b08`), 2 the cursor (`FUN_002ccf28`). They come from
//! the groups the area's `map` program loaded (`m_50` the twelve symbols, `m_51` the nine markers, `m_00` and
//! `m_01` the 22 cursor records of which only the entrance's one is created, `FUN_001981a0`).
//!
//! Symbols: the world is found by the object id in the 12 pairs at 0x383338 (low byte of the second word),
//! the look from byte 0 of that world's record (`FUN_001b4e80`, `progress::Story::world_record`): states 0 and 1
//! play motion slot 0xa1, 2 and above slot 0x97 (`FUN_002c66b8` -> `FUN_0017e2d8(.., slot)`), and the colour
//! word of the state (the table at 0x383398, bytes r g b a, 0x80 = full) is stored at +0x3ac [V code]. State 0
//! has the colour word 0, i.e. alpha 0: such a symbol is not drawn [I]. Byte 3 bit 0 makes effect 3 and
//! state 3 and up effect 5 (`FUN_0016a530/0016a570`), bit 1 sets a flag bit (`+0x108` bit 6), and the TWTNW symbol
//! (world 0x12) adds a ring of kind 6 or 8 by story flag 0x481c (`FUN_002f7cf8`): the effects are not ported
//! (logged, [GAP]).
//!
//! Cursor: the Gummi ship (`WM_CURSOR`). Its movement is the player class's (WP-WM8: its vtable keeps the player's walk
//! slots), so the stick walks Sora on the map's collision and the ship is drawn at his place; his body and the party are
//! hidden [I]. The original's area camera type 4 sets no follow target (docs/worldmap.md section 10 "W8"), so the follow
//! camera is a stand-in [I].
//!
//! Route markers (`WM_GUMI_01`) have no model: their look is three of the map's `BOB` props each, shown or hidden by the route's
//! state byte (`map_props`, WP-WM8).
//!
//! The two forced events (WP-WM7, `FUN_002ccd88`): a confirm on the Twilight Town symbol (object 0x3a2) with world flag 1 set
//! and 0x13 clear plays event 0x72d (1837), on the TWTNW symbol (0x456) with world flags 0x35 set and 0x37 clear event 0x72c
//! (1836), and no prompt follows. The event is a program of the `wm` talk table (`libretto-wm.bar`, `FUN_001b6740`) run by the
//! executor `FUN_002c4c58`, see [`Script`] and docs/worldmap.md section 10 "W7 events".
//!
//! `KH2_WMLOG=1` logs what is spawned.

use std::collections::VecDeque;
use std::sync::Arc;

use bevy::mesh::skinning::SkinnedMeshInverseBindposes;
use bevy::prelude::*;
use kh2_ard::libretto::{Cut, Libretto};
use kh2_tables::tables as t;

use crate::area::{self, app_yaw, game_to_app, AreaRecord, WORLD_MAP};
use crate::assets::{AreaPart, Disc};
use crate::camera::rules::View;
use crate::camera::CamEvent;
use crate::progress::Progress;
use crate::skel::{spawn_skinned, Animator, Rig};
use crate::sora::Sora;
use crate::world::World;
use crate::SCALE;

/// `00objentry` ObjectType of the map's objects (`FUN_0016e8c0` case 0x11).
pub const OBJECT_WORLDMAP: u8 = 0x11;

/// The symbol table (object id, world) and the colour table, in the executable.
const SYMBOL_TABLE: usize = 0x383338;
const SYMBOLS: usize = 12;
const COLOUR_TABLE: usize = 0x383398;
/// Bytes between the executable's addresses and its file offsets.
const EXE_BIAS: usize = 0xff000;
/// The motion slots of a symbol (`FUN_002c66b8`): normal and dim.
const SLOT_NORMAL: usize = 0x97;
const SLOT_DIM: usize = 0xa1;
/// Story flag of the TWTNW ring (`FUN_002c66b8`).
const RING_FLAG: u16 = 0x481c;
/// World byte of The World That Never Was.
const TWTNW: u8 = 0x12;

pub struct WorldMapPlugin;

impl Plugin for WorldMapPlugin {
    fn build(&self, app: &mut App) {
        use crate::schedule::{sim_running, Frame, Sim};
        app.init_resource::<MapSpawned>()
            .init_resource::<WmEvent>()
            .add_systems(
                Update,
                (map_spawn.after(crate::area::run_area_script), map_props, map_start, map_cursor_follow, map_hide_party, map_restore_sora).chain(),
            )
            .add_systems(Startup, spawn_event_fade)
            .add_systems(FixedUpdate, (event_start, event_step, event_animate, event_camera).chain().in_set(Sim::EffectTick).run_if(sim_running))
            .add_systems(Update, event_ui.after(Frame::Pad).before(Frame::Camera))
            .add_systems(Update, event_hud.in_set(Frame::Hud).after(crate::intro::event_player::field_hud_hide));
    }
}

fn wmlog() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| std::env::var("KH2_WMLOG").is_ok_and(|v| !v.is_empty() && v != "0"))
}

/// The map area the objects were spawned for.
#[derive(Resource, Default)]
struct MapSpawned(Option<String>, Vec<u8>);

/// What a symbol looks like for its world's record (`FUN_002c66b8`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Look {
    /// The motion slot played.
    pub slot: usize,
    /// The colour word's bytes as multipliers (r, g, b, a), 1.0 for 0x80.
    pub tint: [f32; 4],
    /// Effect 3 exists (byte 3 bit 0; `FUN_0016a530(obj, 3, 1, 0)`, deleted when clear).
    pub effect3: bool,
    /// Effect 5 exists (state 3 and up; `FUN_0016a570(obj, 5, 1, 0, 0)`, deleted below).
    pub effect5: bool,
    /// Bit 6 of the object's flag word +0x108: set unless the state is 2 or more with bit 1 of byte 3 clear.
    pub flag40: bool,
}

impl Look {
    /// Whether the symbol is drawn: the colour word of state 0 is zero [I].
    pub fn visible(&self) -> bool {
        self.tint[3] > 0.0
    }
}

/// `FUN_002c66b8` for a world record: `colour` is the word at 0x383398 + 4 * state.
pub fn symbol_look(record: [u8; 4], colour: u32) -> Look {
    let state = record[0];
    let c = colour.to_le_bytes().map(|b| b as f32 / 128.0);
    Look {
        slot: if state < 2 { SLOT_DIM } else { SLOT_NORMAL },
        tint: c,
        effect3: record[3] & 1 != 0,
        effect5: state > 2,
        flag40: !(state > 1 && record[3] & 2 == 0),
    }
}

/// The state Twilight Town shows in when its own record is 0 (hidden). [inferred, app only] The app can land only in Twilight Town, so a
/// hidden Twilight Town symbol would leave the map with nothing to touch and no way out; a story that has not revealed it (the app's
/// baseline never reveals a world, or a state that reveals the other worlds only) shows it in its normal state anyway.
pub fn symbol_state(world: Option<u8>, state: u8) -> u8 {
    if world == Some(area::TWILIGHT_TOWN) && state == 0 { 2 } else { state }
}

/// The ring kind of the TWTNW symbol (`FUN_002f7cf8`): 6 with story flag 0x481c clear, 8 set; none in states 0 and 1.
pub fn ring_kind(state: u8, flag_set: bool) -> Option<u32> {
    (state > 1).then_some(if flag_set { 8 } else { 6 })
}

/// The world of an object id in the symbol table (`FUN_002c64d0`): the low byte of the second word.
pub fn symbol_world(exe: &[u8], object: u32) -> Option<u8> {
    let word = |va: usize| exe.get(va.checked_sub(EXE_BIAS)?..va.checked_sub(EXE_BIAS)? + 4).map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]));
    (0..SYMBOLS).find_map(|i| (word(SYMBOL_TABLE + 8 * i)? == object).then(|| word(SYMBOL_TABLE + 8 * i + 4).map(|w| w as u8))?)
}

/// The colour word of a symbol state (0x383398 + 4 * state); a state beyond the table's six reads the neutral colour.
pub fn colour_word(exe: &[u8], state: u8) -> u32 {
    let at = COLOUR_TABLE + 4 * state.min(5) as usize - EXE_BIAS;
    exe.get(at..at + 4).map_or(0x8080_8080, |b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
}

/// The three classes of the map's objects, by the objentry row's second byte (`FUN_002c58c8`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Class {
    Symbol,
    Marker,
    Cursor,
}

pub fn class_of(sub_type: u8) -> Option<Class> {
    match sub_type {
        0 => Some(Class::Symbol),
        1 => Some(Class::Marker),
        2 => Some(Class::Cursor),
        _ => None,
    }
}

/// One map object: its body and what is still to be applied to it.
#[derive(Component)]
pub struct MapPiece {
    pub object: u32,
    body: Entity,
    /// Motion slot to start once the animator exists, and the colour to apply once the meshes do.
    fresh: Option<Fresh>,
}

struct Fresh {
    slot: Option<usize>,
    tint: Option<[f32; 4]>,
}

/// The cursor: follows Sora's walk (first version, [I]).
#[derive(Component)]
pub struct MapCursor;

#[allow(clippy::too_many_arguments)]
fn map_spawn(
    mut commands: Commands,
    disc: Res<Disc>,
    rec: Res<AreaRecord>,
    mut spawned: ResMut<MapSpawned>,
    world: Option<Res<World>>,
    progress: Option<Res<Progress>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut images: ResMut<Assets<Image>>,
    mut bindposes: ResMut<Assets<SkinnedMeshInverseBindposes>>,
    mut sora: Query<(&mut Transform, &mut Sora)>,
) {
    let Some(run) = rec.run.as_ref() else { return };
    if rec.world != WORLD_MAP || spawned.0.as_deref() == Some(run.name.as_str()) {
        return;
    }
    let Some(world) = world else { return };
    spawned.0 = Some(run.name.clone());
    spawned.1.clear();
    let objs = disc.0.lock().ok().and_then(|mut a| kh2_tables::load::<t::Objentry>(&mut a, &t::OBJENTRY).ok()).map(|d| d.rows).unwrap_or_default();
    let exe = disc.read_root("SLUS_210.05").unwrap_or_default();
    let mut rigs: Vec<(String, Option<Arc<Rig>>)> = Vec::new();
    let mut counts = [0usize; 3];
    for g in run.script.groups.iter() {
        let Some(groups) = run.ard.spawns.get(&g.tag) else { continue };
        for grp in groups {
            for e in &grp.entities {
                if !kh2_ard::run::entity_for_entrance(e, rec.entrance) {
                    continue;
                }
                let Some(row) = objs.iter().find(|r| r.object_id == e.object).filter(|r| r.object_type == OBJECT_WORLDMAP) else { continue };
                let Some(class) = class_of(row.sub_type) else { continue };
                if class == Class::Marker {
                    spawned.1.push(e.raw[0x20]);
                }
                let model = row.model_name.clone();
                let rig = match rigs.iter().find(|(m, _)| *m == model) {
                    Some((_, r)) => r.clone(),
                    None => {
                        let r = load_rig(&disc, &model, &row.animation_name);
                        rigs.push((model.clone(), r.clone()));
                        r
                    }
                };
                let Some(rig) = rig else {
                    if wmlog() {
                        info!("worldmap: {model}: no model; not spawned");
                    }
                    continue;
                };
                let at = if class == Class::Cursor { cursor_override().unwrap_or_else(|| game_to_app(e.position)) } else { game_to_app(e.position) };
                let on_floor = world.ground(at, at.y + 100.0, 400.0).map(|g| g.y);
                let yaw = app_yaw(e.rotation[1]);
                let (slot, tint, note) = match class {
                    Class::Symbol => {
                        let w = symbol_world(&exe, e.object);
                        let mut record = w.map_or([0; 4], |w| progress.as_ref().map_or([0; 4], |p| p.story().world_record(w as usize)));
                        record[0] = symbol_state(w, record[0]);
                        let look = symbol_look(record, colour_word(&exe, record[0]));
                        let mut note = format!("world {w:?} record {record:02x?} -> motion slot {:#x}, tint {:.2?}", look.slot, look.tint);
                        if !look.visible() {
                            note += ", not drawn (colour word 0) [I]";
                        }
                        if look.effect3 || look.effect5 {
                            note += &format!(", effect 3 {} effect 5 {} not ported [GAP]", look.effect3, look.effect5);
                        }
                        if w == Some(TWTNW) {
                            let flag = progress.as_ref().is_some_and(|p| p.story().test(RING_FLAG));
                            match ring_kind(record[0], flag) {
                                Some(k) => note += &format!(", ring kind {k} not ported [GAP]"),
                                None => note += ", no ring in this state",
                            }
                        }
                        (look.visible().then_some(look.slot), Some(look.tint), note)
                    }
                    Class::Marker => (None, None, format!("route {}: no update read, rest pose [GAP]", e.raw[0x20])),
                    Class::Cursor => (None, None, format!("entrance {} record, follows Sora's walk [I]", e.spawn_arg)),
                };
                counts[class as usize] += 1;
                let shown = !matches!(class, Class::Symbol) || tint.is_none_or(|t| t[3] > 0.0);
                let start = if class == Class::Cursor { Vec3::new(at.x, on_floor.unwrap_or(at.y), at.z) } else { at };
                let root = commands
                    .spawn((
                        Transform::from_translation(start * SCALE).with_rotation(Quat::from_rotation_y(yaw)),
                        if shown { Visibility::default() } else { Visibility::Hidden },
                        AreaPart,
                    ))
                    .id();
                let body = spawn_skinned(&mut commands, root, rig, &mut meshes, &mut materials, &mut images, &mut bindposes, SCALE);
                commands.entity(root).insert(MapPiece { object: e.object, body, fresh: Some(Fresh { slot, tint }) });
                if class == Class::Cursor {
                    commands.entity(root).insert(MapCursor);
                    if let Ok((mut tf, mut s)) = sora.single_mut() {
                        crate::sora::respawn(&mut s, start);
                        s.motor.yaw = yaw;
                        tf.translation = start * SCALE;
                        tf.rotation = Quat::from_rotation_y(yaw);
                    }
                }
                if wmlog() {
                    info!("worldmap: {} group {} object {:#x} ({model}) at ({:.0}, {:.0}, {:.0}): {note}", run.name, g.tag, e.object, at.x, at.y, at.z);
                }
            }
        }
    }
    info!("worldmap: {}: {} symbols, {} route markers, {} cursor spawned (entrance {})", run.name, counts[0], counts[1], counts[2], rec.entrance);
}

/// `KH2_WMAT=<x>,<z>`: the cursor starts at this game position (harness aid for looking at one place of the map).
fn cursor_override() -> Option<Vec3> {
    let v = std::env::var("KH2_WMAT").ok()?;
    let (x, z) = v.split_once(',')?;
    Some(game_to_app([x.trim().parse().ok()?, 0.0, z.trim().parse().ok()?]))
}

/// The model and motion set of an objentry row (as `npc.rs` loads a townsperson's).
fn load_rig(disc: &Disc, model: &str, anim: &str) -> Option<Arc<Rig>> {
    let anim = Some(anim.trim_end_matches(".mset")).filter(|a| !a.is_empty()).unwrap_or(model);
    let file = disc.read(&format!("obj/{model}.mdlx"));
    let m = file.as_deref().and_then(kh2_model::mdlx);
    if m.is_none() {
        let no_block = file.as_deref().and_then(|f| kh2_bar::parse(f).ok()).is_some_and(|e| !e.iter().any(|x| x.kind == 4));
        warn!(
            "worldmap: obj/{model}.mdlx: {}",
            if no_block { "holds no model block (collision only; its look is the effect file, docs/worldmap.md section 10 W5), not drawn" } else if file.is_some() { "read, but the model reader returns nothing" } else { "not on the disc" }
        );
    }
    let m = m?;
    // The route marker has no motion set on the disc: it is drawn in its rest pose.
    let s = disc.read(&format!("obj/{anim}.mset")).and_then(|d| kh2_anim::Mset::parse(&d)).unwrap_or(kh2_anim::Mset { slots: Vec::new() });
    let apdx = Some(format!("obj/{model}.apdx")).filter(|a| disc.read(a).is_some());
    Some(Arc::new(Rig { model: m, mset: s, apdx }))
}

/// Starts the symbols' motions and applies their colours once the body's animator and meshes exist.
fn map_start(
    mut pieces: Query<&mut MapPiece>,
    mut anims: Query<&mut Animator>,
    children: Query<&Children>,
    mut mats: Query<&mut MeshMaterial3d<StandardMaterial>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    for mut p in &mut pieces {
        let Some(f) = p.fresh.as_ref() else { continue };
        let Ok(mut anim) = anims.get_mut(p.body) else { continue };
        if let Some(slot) = f.slot {
            if anim.rig.mset.has(slot) {
                let looping = anim.rig.mset.get(slot).and_then(|s| s.motion.as_ref()).is_some_and(|m| m.frame_return < m.frame_end);
                anim.restart(Some(slot), looping, 0.0);
            } else if wmlog() {
                info!("worldmap: object {:#x}: motion slot {slot:#x} is not in its set; rest pose", p.object);
            }
        }
        if let Some(t) = f.tint {
            for d in children.iter_descendants(p.body) {
                if let Ok(mut m) = mats.get_mut(d)
                    && let Some(mut copy) = materials.get(&m.0).cloned()
                {
                    let c = copy.base_color.to_linear();
                    copy.base_color = Color::linear_rgba(c.red * t[0], c.green * t[1], c.blue * t[2], c.alpha);
                    m.0 = materials.add(copy);
                }
            }
        }
        p.fresh = None;
    }
}

/// The cursor stands where Sora stands (his stick walk moves it) and Sora's body is not drawn [I].
fn map_cursor_follow(mut cursor: Query<&mut Transform, With<MapCursor>>, sora: Query<(&Transform, &Sora), Without<MapCursor>>, mut vis: Query<&mut Visibility>) {
    let Ok((stf, s)) = sora.single() else { return };
    for mut tf in &mut cursor {
        tf.translation = stf.translation;
        tf.rotation = Quat::from_rotation_y(s.motor.yaw);
        for e in std::iter::once(s.body).chain(s.weapons.iter().copied()) {
            if let Ok(mut v) = vis.get_mut(e) {
                *v = Visibility::Hidden;
            }
        }
    }
}

/// While the map is up the party is not drawn (the manager's party reset code 1, `FUN_001abfe8`, was not read: [I]).
fn map_hide_party(rec: Res<AreaRecord>, mut members: Query<&mut Visibility, With<crate::party::PartyMember>>) {
    if rec.world != WORLD_MAP {
        return;
    }
    for mut v in &mut members {
        *v = Visibility::Hidden;
    }
}

/// Sora's body shows again once the map is left (a landing): the cursor follow hid it, and nothing else sets it back.
/// His weapons are set by his own tick.
fn map_restore_sora(rec: Res<AreaRecord>, sora: Query<&Sora>, mut vis: Query<&mut Visibility>, mut hid: Local<bool>) {
    if rec.world == WORLD_MAP {
        *hid = true;
        return;
    }
    if !*hid {
        return;
    }
    let Ok(s) = sora.single() else { return };
    if let Ok(mut v) = vis.get_mut(s.body) {
        *v = Visibility::Inherited;
        *hid = false;
    }
}

// ---- The map's props and the route markers (WP-WM8) ----------------------------------------------------------------------

/// The map's prop table: BAR entry type 0x15 (`FUN_00149348` keeps it at map +0x44, `FUN_001491e0`), a u32 offset to the records
/// and a u32 byte size, then records of 0x68 bytes (`FUN_0015bd40` divides the size by 0x68).
const PROP_TABLE_KIND: u16 = 0x15;
const PROP_RECORD: usize = 0x68;
/// The route table in the executable: three scene-entry numbers per route, indexed by the route number (`FUN_002c8b50` stores
/// `route * 3 + 0x38342d` at +0xae4 of a marker).
const ROUTE_TABLE: usize = 0x38342d;
/// Route `r` has its state byte at `0x3419c3 + r` (`FUN_001c0950`): inside the world region of `progress::Story`.
const ROUTE_STATE: u32 = 0x3419c3;
/// The most routes a marker can name (`m_51` numbers them 1 to 9).
const ROUTES: u8 = 9;

/// One placed prop of the table (`FUN_0015bdb8` reads each record: position words 0..2, rotation 3..5, scale 6..8, the model number
/// +0x24 counted from the first `BOB` model, the visibility group +0x28).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Prop {
    pub pos: [f32; 3],
    pub rot: [f32; 3],
    pub scale: [f32; 3],
    pub model: u32,
    pub group: u32,
}

/// The records of a prop table entry; a table that runs past the entry keeps the records the bytes hold.
pub fn parse_props(b: &[u8]) -> Vec<Prop> {
    let word = |o: usize| b.get(o..o + 4).map(|w| u32::from_le_bytes([w[0], w[1], w[2], w[3]]));
    let (Some(off), Some(size)) = (word(0), word(4)) else { return Vec::new() };
    let off = off as usize;
    let count = (size as usize / PROP_RECORD).min(b.len().saturating_sub(off) / PROP_RECORD);
    (0..count)
        .map(|k| {
            let at = off + k * PROP_RECORD;
            let f = |o: usize| f32::from_bits(word(at + o).unwrap_or(0));
            Prop {
                pos: [f(0), f(4), f(8)],
                rot: [f(12), f(16), f(20)],
                scale: [f(24), f(28), f(32)],
                model: word(at + 36).unwrap_or(0),
                group: word(at + 40).unwrap_or(0),
            }
        })
        .collect()
}

/// Whether the map's visibility word shows a prop's group (`FUN_0015bdb8`: `(1 << group) & mask`; only those props are made).
pub fn prop_made(group: u32, visibility: u64) -> bool {
    1u64.checked_shl(group).is_some_and(|bit| bit & visibility != 0)
}

/// The three scene entries of a route (`FUN_002c8d18`'s argument): numbers in the order the props were made.
pub fn route_entries(exe: &[u8], route: u8) -> Option<[usize; 3]> {
    (route <= ROUTES).then_some(())?;
    let at = (ROUTE_TABLE + 3 * route as usize).checked_sub(EXE_BIAS)?;
    exe.get(at..at + 3).map(|b| [b[0] as usize, b[1] as usize, b[2] as usize])
}

/// `FUN_002c8d18(entries, state)`: state 1 shows only the first entry, state 2 only the other two, any other state none
/// (`FUN_00160a98` clears the entry's hide bit 4 of +0x180, `FUN_00160ac8` sets it [inferred: bit 4 hides]).
pub fn route_shows(state: u8, entry: usize) -> bool {
    match state {
        1 => entry == 0,
        2 => entry != 0,
        _ => false,
    }
}

/// Whether each prop the map made is shown, in creation order. A marker's three entries follow its route's state; the markers
/// are applied in order, so the last one naming an entry decides it.
pub fn prop_visibility(made: usize, routes: &[(u8, [usize; 3])], state: impl Fn(u8) -> u8) -> Vec<bool> {
    let mut shown = vec![true; made];
    for (route, entries) in routes {
        let st = state(*route);
        for (k, n) in entries.iter().enumerate() {
            if let Some(s) = shown.get_mut(*n) {
                *s = route_shows(st, k);
            }
        }
    }
    shown
}

/// The `BOB` models of a map file in file order: the n-th type 4 block tagged `BOB` with the n-th type 7 texture block of that tag
/// (`FUN_00149348` stores them behind the `MAP` and sky models, `FUN_00149220` hands out model `n`). The blocks are skeletal models
/// (model type 3, as an MDLX's), so each pair is wrapped in a two-entry BAR for `kh2_model::mdlx`; the vertices come out in the rest pose.
pub fn bob_models(file: &[u8]) -> Vec<kh2_model::Model> {
    let Ok(e) = kh2_bar::parse(file) else { return Vec::new() };
    let mut textures = e.iter().filter(|x| x.tag == "BOB" && x.kind == 7);
    e.iter()
        .filter(|x| x.tag == "BOB" && x.kind == 4)
        .map(|m| {
            let t = textures.next().map_or(&[][..], |t| t.data);
            kh2_model::mdlx(&bar_of(&[(4, m.data), (7, t)])).unwrap_or_default()
        })
        .collect()
}

/// A BAR holding the given (type, data) entries, tags empty.
fn bar_of(entries: &[(u16, &[u8])]) -> Vec<u8> {
    let mut out = b"BAR".to_vec();
    out.extend_from_slice(&(entries.len() as u32).to_le_bytes());
    out.extend_from_slice(&[0; 8]);
    let mut at = 16 + 16 * entries.len();
    for (kind, data) in entries {
        out.extend_from_slice(&kind.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(&[0; 4]);
        out.extend_from_slice(&(at as u32).to_le_bytes());
        out.extend_from_slice(&(data.len() as u32).to_le_bytes());
        at += data.len();
    }
    for (_, data) in entries {
        out.extend_from_slice(data);
    }
    out
}

/// A prop's place in the app: the record's position and angles are in the game's frame and the models are in the app's, which is the
/// game's turned half a turn about x (`game_to_app`), so the angles about y and z change sign [inferred, as for the stage matrix].
pub fn prop_transform(p: &Prop) -> Transform {
    Transform {
        translation: game_to_app(p.pos) * SCALE,
        rotation: Quat::from_rotation_z(-p.rot[2]) * Quat::from_rotation_y(-p.rot[1]) * Quat::from_rotation_x(p.rot[0]),
        scale: Vec3::from(p.scale) * SCALE,
    }
}

/// Which map the props were made for.
#[derive(Default)]
struct PropsMade(Option<String>);

/// A prop of the map (`FUN_0015f8e0`'s object).
#[derive(Component)]
pub struct PropPiece;

/// Makes the map's props (`FUN_0015bdb8`) once the area loaded and hides the route entries a marker's state hides
/// (`FUN_002c8b50` -> `FUN_002c8bc8` -> `FUN_002c8d18`, run every tick; the route state only changes with the story, so once at the start).
#[allow(clippy::too_many_arguments)]
fn map_props(
    mut commands: Commands,
    disc: Res<Disc>,
    rec: Res<AreaRecord>,
    spawned: Res<MapSpawned>,
    mut made: Local<PropsMade>,
    progress: Option<Res<Progress>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut images: ResMut<Assets<Image>>,
) {
    let Some(run) = rec.run.as_ref() else { return };
    if rec.world != WORLD_MAP || spawned.0.as_deref() != Some(run.name.as_str()) || made.0.as_deref() == Some(run.name.as_str()) {
        return;
    }
    made.0 = Some(run.name.clone());
    let Some(file) = disc.read(&format!("map/us/{}.map", run.name)) else { return };
    let table = kh2_bar::parse(&file).ok().and_then(|e| e.iter().find(|x| x.kind == PROP_TABLE_KIND).map(|x| parse_props(x.data))).unwrap_or_default();
    let models = bob_models(&file);
    let exe = disc.read_root("SLUS_210.05").unwrap_or_default();
    let list: Vec<&Prop> = table.iter().filter(|p| prop_made(p.group, run.script.map_visibility)).collect();
    let routes: Vec<(u8, [usize; 3])> = spawned.1.iter().filter_map(|r| route_entries(&exe, *r).map(|e| (*r, e))).collect();
    let state = |r: u8| progress.as_ref().map_or(0, |p| p.story().world_byte(ROUTE_STATE + r as u32));
    let shown = prop_visibility(list.len(), &routes, state);
    let mut drawn = 0;
    for (n, p) in list.iter().enumerate() {
        let Some(model) = models.get(p.model as usize).filter(|m| !m.meshes.is_empty()) else { continue };
        let root = commands.spawn((prop_transform(p), if shown[n] { Visibility::default() } else { Visibility::Hidden }, AreaPart, PropPiece)).id();
        crate::assets::spawn_model(&mut commands, root, model, &mut meshes, &mut materials, &mut images);
        drawn += 1;
    }
    let by_route: Vec<String> = routes.iter().map(|(r, e)| format!("{r}: state {} entries {e:?}", state(*r))).collect();
    info!("worldmap: {}: {} props in the table, {} made (visibility {:#x}), {} drawn; routes {{{}}}", run.name, table.len(), list.len(), run.script.map_visibility, drawn, by_route.join(", "));
}

// ---- The forced events (WP-WM7) ------------------------------------------------------------------------------------------

/// The events `FUN_002ccd88` starts through `FUN_002c6a40` (the numbers are the libretto's: 1837 and 1836).
pub const EVENT_TWILIGHT_TOWN: u16 = 0x72d;
pub const EVENT_TWTNW: u16 = 0x72c;
/// The symbol objects the checks name (the Twilight Town symbol and the TWTNW symbol).
const SYMBOL_TWILIGHT_TOWN: u32 = 0x3a2;
const SYMBOL_TWTNW: u32 = 0x456;
/// Frames the map ignores the confirm button after an event has ended: the press that closed its last message must not open a prompt.
const QUIET_FRAMES: u8 = 12;
/// The event actors' slots (`FUN_002c4c58` op 0x102 stores the object at `+0x2c + 4 * slot`; `FUN_002c5230` removes four).
const SLOTS: usize = 4;
/// The talk table of the `wm` world (`FUN_001b6740` finds the event's program in it).
const TALK_TABLE: &str = "libretto-wm.bar";

/// `FUN_002ccd88`: the event a confirm on the symbol `object` starts, if any. `flag` reads the world flags (`FUN_001b6a18`). The
/// Twilight Town symbol starts 1837 while flag 1 is set and 0x13 clear, the TWTNW symbol 1836 while 0x35 is set and 0x37 clear; the
/// original's answer 1 means "an event was started, no prompt follows".
pub fn forced_event(object: u32, flag: impl Fn(u32) -> bool) -> Option<u16> {
    if object == SYMBOL_TWILIGHT_TOWN && flag(1) && !flag(0x13) {
        return Some(EVENT_TWILIGHT_TOWN);
    }
    if object == SYMBOL_TWTNW && flag(0x35) && !flag(0x37) {
        return Some(EVENT_TWTNW);
    }
    None
}

/// The symbol object of a world (the reverse of [`symbol_world`] over the 12 pairs at 0x383338).
pub fn symbol_object(exe: &[u8], world: u8) -> Option<u32> {
    let word = |va: usize| exe.get(va.checked_sub(EXE_BIAS)?..va.checked_sub(EXE_BIAS)? + 4).map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]));
    (0..SYMBOLS).find_map(|i| (word(SYMBOL_TABLE + 8 * i + 4)? as u8 == world).then(|| word(SYMBOL_TABLE + 8 * i))?)
}

/// The stage the event's actors stand on: a position and a rotation (radians) the program sets with ops 0x10b and 0x10c
/// (`FUN_002c9160`, `FUN_002c91d0`: the matrix at `0x1e35400` is built from the rotation at `0x1e353f0` and the position at
/// `0x1e353e0`, and each actor's world matrix is its own matrix times it, `FUN_002c8ff8`).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Stage {
    pub pos: [f32; 3],
    pub rot: [f32; 3],
}

/// `FUN_0011b568`: the matrix of three angles is built by turning about x, then y, then z (`FUN_002fb580`, `FUN_002fb628`,
/// `FUN_002fb4d8`; each multiplies the four rows by the axis matrix). Read as the usual right-handed turns [inferred: the sign of the
/// axis matrices was not traced, and kh2-ee does not run their VU0 macro code (the broadcast multiplies return equal rows); the sign is
/// the one under which Chip, at the left with a turn of -20 degrees, and Dale, at the right with +20, turn toward each other, and the
/// stage's pitch of 10 degrees looks down on the consoles: the opposite sign shows the floor from below].
fn game_rotation(rot: [f32; 3]) -> Quat {
    Quat::from_rotation_z(rot[2]) * Quat::from_rotation_y(rot[1]) * Quat::from_rotation_x(rot[0])
}

/// Where an actor at `pos` with `rot` (the program's op 0x109 and 0x10a values) is on the stage, in the app's space (centimetres) and
/// facing: the world matrix of `FUN_002c8ff8`, then the game-to-app axis flip (`game_to_app`, a half turn about x) and the model's
/// half turn about y (an object at yaw 0 faces `PI` here, `app_yaw`).
pub fn actor_place(stage: &Stage, pos: [f32; 3], rot: [f32; 3]) -> (Vec3, Quat) {
    let g = game_rotation(stage.rot);
    let at = g * Vec3::from(pos) + Vec3::from(stage.pos);
    let flip = Quat::from_rotation_x(std::f32::consts::PI);
    (game_to_app(at.to_array()), flip * (g * game_rotation(rot)) * flip.inverse() * Quat::from_rotation_y(std::f32::consts::PI))
}

/// Where the stage and its camera stand in the app's world, in centimetres: far above the map, clear of its symbols, one of which (Disney
/// Castle) stands at the game's origin where the program's stage is [inferred: the stage matrix is read as relative to a camera of its own,
/// as the 59000 cm wide cockpit model and the stage's small coordinates suggest; the original's camera for these events was not found].
pub const STAGE_ORIGIN: Vec3 = Vec3::new(0.0, 200_000.0, 0.0);

/// The program of an event as the executor `FUN_002c4c58` runs it: the commands in order, each one at once unless it waits. Waits
/// are the talk reader's op 2 (`FUN_00102460`, frames), op 0x104's duration, and op 1, which holds until its message window has
/// closed (`FUN_002c6010`: the window's state 7). Fades (ops 5 and 6) do not wait: the programs follow them with an op 2.
#[derive(Clone, Debug, Default)]
pub struct Script {
    cmds: Vec<Cut>,
    at: usize,
    wait: f32,
    window: bool,
}

impl Script {
    pub fn new(cmds: Vec<Cut>) -> Script {
        Script { cmds, ..Script::default() }
    }

    /// A message window is up and the program waits for it.
    pub fn waiting_for_window(&self) -> bool {
        self.window
    }

    /// Every command ran and nothing is being waited for.
    pub fn done(&self) -> bool {
        self.at >= self.cmds.len() && self.wait <= 0.0 && !self.window
    }

    /// Advances `frames` game frames (60 a second); `closed`: the message window the program waits for has closed. Returns the
    /// commands that ran, the one that made the program wait last.
    pub fn step(&mut self, frames: f32, closed: bool) -> Vec<Cut> {
        if self.window {
            if !closed {
                return Vec::new();
            }
            self.window = false;
        }
        if self.wait > 0.0 {
            self.wait -= frames;
            if self.wait > 0.0 {
                return Vec::new();
            }
            self.wait = 0.0;
        }
        let mut ran = Vec::new();
        while let Some(cut) = self.cmds.get(self.at).cloned() {
            self.at += 1;
            let hold = match &cut {
                Cut::Wait(f) => *f,
                Cut::Fly { frames, .. } => *frames,
                Cut::Message(_) => {
                    self.window = true;
                    0.0
                }
                _ => 0.0,
            };
            ran.push(cut);
            if self.window {
                break;
            }
            if hold > 0.0 {
                self.wait = hold;
                break;
            }
        }
        ran
    }
}

/// A screen fade (ops 5 and 6, `FUN_0014a7c8` out and `FUN_0014a7f0` in): alpha from `from` to `to` over `frames`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Fade {
    pub from: f32,
    pub to: f32,
    pub frames: f32,
    pub t: f32,
}

impl Fade {
    pub fn alpha(&self) -> f32 {
        if self.frames <= 0.0 { self.to } else { self.from + (self.to - self.from) * (self.t / self.frames).clamp(0.0, 1.0) }
    }
}

/// One event actor.
struct Cast {
    root: Entity,
    body: Entity,
    pos: [f32; 3],
    rot: [f32; 3],
    /// The motion to start once the body's animator exists, and the ones queued after it (ops 0x103 and 0x111).
    pending: Option<Arc<kh2_anim::Slot>>,
    queue: VecDeque<Arc<kh2_anim::Slot>>,
}

/// A playing event.
struct Play {
    id: u16,
    script: Script,
    /// The entries of the event's file (op 0x101): (type, bytes).
    file: Vec<(u16, Vec<u8>)>,
    notice: bool,
    stage: Stage,
    cast: [Option<Cast>; SLOTS],
    /// The stage camera is on (from op 0x106).
    camera: bool,
}

/// The world map's forced events: [`WmEvent::request`] starts one, `FUN_002c6a40`.
#[derive(Resource, Default)]
pub struct WmEvent {
    play: Option<Play>,
    request: Option<u16>,
    quiet: u8,
    fade: Option<Fade>,
    table: Option<Option<Libretto>>,
    objs: Option<Vec<t::Objentry>>,
    rigs: Vec<(String, Option<Arc<Rig>>)>,
    /// The HUD nodes that were visible when the event began: `FUN_00196760` (called by `FUN_002c6a40`) hides the field HUD until it ends.
    hud: Option<Vec<usize>>,
}

impl WmEvent {
    /// `FUN_002c6a40(event)`.
    pub fn request(&mut self, id: u16) {
        if !self.busy() {
            self.request = Some(id);
        }
    }

    /// An event is asked for, plays, or has just ended: the map's own buttons are not read.
    pub fn busy(&self) -> bool {
        self.play.is_some() || self.request.is_some() || self.quiet > 0
    }

    /// The screen fade the event drives, alpha 0..1.
    pub fn fade_alpha(&self) -> f32 {
        self.fade.map_or(0.0, |f| f.alpha())
    }
}

/// The full-screen node the event's fades paint (under the area jump's own fader).
#[derive(Component)]
struct EventFade;

fn spawn_event_fade(mut commands: Commands) {
    commands.spawn((
        EventFade,
        Node { position_type: PositionType::Absolute, left: Val::Px(0.0), top: Val::Px(0.0), width: Val::Percent(100.0), height: Val::Percent(100.0), ..default() },
        BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.0)),
        GlobalZIndex(999),
    ));
}

/// The motion of entry `entry` of the event's file: the entry is a BAR with the motion (type 9) and its triggers (type 16), as an
/// `.anb` file is (`FUN_002f54d0` takes the entry's data).
fn entry_motion(file: &[(u16, Vec<u8>)], entry: u32) -> Option<Arc<kh2_anim::Slot>> {
    let (_, data) = file.get(entry as usize)?;
    let bar = kh2_bar::parse(data).ok()?;
    let motion = bar.iter().find(|x| x.kind == 9).and_then(|x| kh2_anim::Motion::parse(x.data))?;
    let triggers = bar.iter().find(|x| x.kind == 16).map(|x| kh2_anim::Triggers::parse(x.data)).unwrap_or_default();
    Some(Arc::new(kh2_anim::Slot { tag: format!("entry {entry}"), motion: Some(motion), triggers }))
}

/// The entries of `libretto/<region>/wm<event>.bar` (op 0x101, `FUN_002c5598`).
fn event_file(disc: &Disc, id: u16) -> Vec<(u16, Vec<u8>)> {
    let name = format!("libretto/{}/wm{id}.bar", crate::intro::event_player::REGION);
    let Some(bytes) = disc.read(&name) else {
        warn!("worldmap: event {id}: {name} is not on the disc");
        return Vec::new();
    };
    kh2_bar::parse(&bytes).map(|es| es.iter().map(|e| (e.kind, e.data.to_vec())).collect()).unwrap_or_default()
}

/// `FUN_002c6a40` finds the event's program in the talk table and starts it.
fn event_start(mut ev: ResMut<WmEvent>, disc: Res<Disc>, rec: Res<AreaRecord>, time: Res<Time<Real>>, mut env_done: Local<bool>) {
    if ev.play.is_some() {
        return;
    }
    // Harness aid: `KH2_WMEVENT=<event number>` plays that event on the map `KH2_WMEVENT_AT` seconds (default 6) after the start.
    if !*env_done && rec.world == WORLD_MAP && rec.run.is_some() {
        let at = std::env::var("KH2_WMEVENT_AT").ok().and_then(|v| v.parse::<f32>().ok()).unwrap_or(6.0);
        if let Some(id) = std::env::var("KH2_WMEVENT").ok().and_then(|v| v.parse::<u16>().ok())
            && time.elapsed_secs() >= at
        {
            *env_done = true;
            ev.request = Some(id);
        }
    }
    let Some(id) = ev.request.take() else { return };
    let table = ev.table.get_or_insert_with(|| disc.read(TALK_TABLE).and_then(|f| Libretto::from_bar(&f))).clone();
    let Some(program) = table.and_then(|l| l.program(id)) else {
        warn!("worldmap: event {id:#x}: no program in {TALK_TABLE}");
        return;
    };
    let cmds: Vec<Cut> = program.iter().map(|c| c.cut()).collect();
    info!("worldmap: event {id:#x} ({id}) starts (FUN_002c6a40): {} commands; control and the pause menu are held until it ends", cmds.len());
    ev.play = Some(Play { id, script: Script::new(cmds), file: Vec::new(), notice: false, stage: Stage::default(), cast: [None, None, None, None], camera: false });
}

/// Queues entries after an actor's motion (`FUN_002c5340`); `start` first makes the first one the actor's motion (`FUN_002c5278`).
fn queue_motions(c: &mut Cast, file: &[(u16, Vec<u8>)], entries: &[u32], start: bool) {
    let mut slots = entries.iter().filter_map(|e| entry_motion(file, *e));
    if start {
        c.queue.clear();
        c.pending = slots.next();
    }
    c.queue.extend(slots);
}

#[allow(clippy::too_many_arguments)]
fn event_step(
    mut commands: Commands,
    time: Res<Time<Fixed>>,
    disc: Res<Disc>,
    mut ev: ResMut<WmEvent>,
    mut win: Option<ResMut<crate::hud::hud_msgwin::ScriptWindow>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut images: ResMut<Assets<Image>>,
    mut bindposes: ResMut<Assets<SkinnedMeshInverseBindposes>>,
) {
    let ev = &mut *ev;
    let frames = time.delta_secs() * 60.0;
    if let Some(f) = ev.fade.as_mut() {
        f.t += frames;
        if f.t >= f.frames && f.to == 0.0 {
            ev.fade = None;
        }
    }
    let Some(play) = ev.play.as_mut() else {
        ev.quiet = ev.quiet.saturating_sub(1);
        return;
    };
    let closed = play.script.waiting_for_window() && win.as_mut().is_some_and(|w| std::mem::take(&mut w.closed));
    for cut in play.script.step(frames, closed) {
        match cut {
            Cut::FadeOut(f) => ev.fade = Some(Fade { from: ev.fade.map_or(0.0, |x| x.alpha()), to: 1.0, frames: f, t: 0.0 }),
            Cut::FadeIn(f) => ev.fade = Some(Fade { from: ev.fade.map_or(1.0, |x| x.alpha()), to: 0.0, frames: f, t: 0.0 }),
            Cut::Message(id) => {
                if let Some(w) = win.as_mut() {
                    // `FUN_002c5db0`: the first class of the message windows, the second after op 0x107.
                    w.open = Some((id, usize::from(play.notice)));
                    w.closed = false;
                }
                info!("worldmap: event {:#x}: message {id:#x} (FUN_002c5db0)", play.id);
            }
            Cut::Notice => play.notice = true,
            Cut::LoadFile => {
                play.file = event_file(&disc, play.id);
                info!("worldmap: event {:#x}: libretto file with {} entries (FUN_002c5598)", play.id, play.file.len());
            }
            Cut::Ready => play.camera = true,
            Cut::Actor { slot, object } => {
                let slot = slot as usize % SLOTS;
                if let Some(old) = play.cast[slot].take() {
                    commands.entity(old.root).despawn();
                }
                let objs = ev.objs.get_or_insert_with(|| disc.0.lock().ok().and_then(|mut a| kh2_tables::load::<t::Objentry>(&mut a, &t::OBJENTRY).ok()).map(|d| d.rows).unwrap_or_default());
                let row = objs.iter().find(|r| r.object_id == object);
                let model = row.map(|r| r.model_name.clone());
                let anim = row.map(|r| r.animation_name.clone()).unwrap_or_default();
                let rig = model.as_ref().and_then(|m| match ev.rigs.iter().find(|(n, _)| n == m) {
                    Some((_, r)) => r.clone(),
                    None => {
                        let r = load_rig(&disc, m, &anim);
                        ev.rigs.push((m.clone(), r.clone()));
                        r
                    }
                });
                match rig {
                    Some(rig) => {
                        let root = commands.spawn((Transform::default(), Visibility::default(), AreaPart)).id();
                        let body = spawn_skinned(&mut commands, root, rig, &mut meshes, &mut materials, &mut images, &mut bindposes, SCALE);
                        play.cast[slot] = Some(Cast { root, body, pos: [0.0; 3], rot: [0.0; 3], pending: None, queue: VecDeque::new() });
                        info!("worldmap: event {:#x}: slot {slot} is object {object:#x} ({}) (FUN_002c5108)", play.id, model.unwrap_or_default());
                    }
                    None => warn!("worldmap: event {:#x}: slot {slot} object {object:#x}: model {model:?} is not readable", play.id),
                }
            }
            Cut::Play { slot, entries } => {
                if let Some(c) = play.cast[slot as usize % SLOTS].as_mut() {
                    queue_motions(c, &play.file, &entries, true);
                }
            }
            Cut::Queue { slot, entries } => {
                if let Some(c) = play.cast[slot as usize % SLOTS].as_mut() {
                    queue_motions(c, &play.file, &entries, false);
                }
            }
            Cut::Place { slot, pos } => {
                if let Some(c) = play.cast[slot as usize % SLOTS].as_mut() {
                    c.pos = pos;
                }
            }
            Cut::Turn { slot, rot } => {
                if let Some(c) = play.cast[slot as usize % SLOTS].as_mut() {
                    c.rot = rot;
                }
            }
            Cut::StagePos(p) => play.stage.pos = p,
            Cut::StageRot(r) => play.stage.rot = r,
            Cut::Remove(slot) => {
                if let Some(c) = play.cast[slot as usize % SLOTS].take() {
                    commands.entity(c.root).despawn();
                }
            }
            Cut::RemoveAll => {
                for c in play.cast.iter_mut().filter_map(Option::take) {
                    commands.entity(c.root).despawn();
                }
            }
            Cut::Voice(entry) => info!("worldmap: event {:#x}: voice entry {entry} (FUN_002c5630): the app has no voice path for it [GAP]", play.id),
            Cut::LoadModel(_) | Cut::Wait(_) => {}
            other => info!("worldmap: event {:#x}: {other:?} not ported", play.id),
        }
    }
    for c in play.cast.iter().flatten() {
        let (at, facing) = actor_place(&play.stage, c.pos, c.rot);
        commands.entity(c.root).insert(Transform::from_translation((at + STAGE_ORIGIN) * SCALE).with_rotation(facing));
    }
    if play.script.done() {
        for c in play.cast.iter_mut().filter_map(Option::take) {
            commands.entity(c.root).despawn();
        }
        info!("worldmap: event {:#x} over (FUN_002c6b58): control and the pause menu back", play.id);
        ev.play = None;
        ev.quiet = QUIET_FRAMES;
    }
}

/// Starts the queued motions once an actor's animator exists, and the next queued one when the last has played.
fn event_animate(mut ev: ResMut<WmEvent>, mut anims: Query<&mut Animator>) {
    let Some(play) = ev.play.as_mut() else { return };
    for c in play.cast.iter_mut().flatten() {
        let Ok(mut a) = anims.get_mut(c.body) else { continue };
        if c.pending.is_none() && a.finished {
            c.pending = c.queue.pop_front();
        }
        if let Some(m) = c.pending.take() {
            a.play_external(m, 0.0);
        }
    }
}

/// The field HUD is hidden while the event plays (`FUN_002c6a40` -> `FUN_00196760` -> `FUN_0014ef48`, draw tags 3 to 6 [inferred: the HUD
/// nodes that were visible when the event began, but the map panels' own, are the ones turned off, and shown again when it ends, as
/// `event_player::field_hud_hide` does; the message window comes up later and is not among them]). Runs after the HUD modules each frame.
fn event_hud(mut ev: ResMut<WmEvent>, mut scene: ResMut<crate::hud::HudScene>, ui: Res<crate::worldmap_ui::MapUi>) {
    if ev.play.is_none() {
        if let Some(hidden) = ev.hud.take() {
            for i in hidden {
                if let Some(n) = scene.nodes.get_mut(i) {
                    n.visible = true;
                }
            }
        }
        return;
    }
    let panel = ui.panel_file();
    let shown = ev.hud.get_or_insert_with(|| scene.nodes.iter().enumerate().filter(|(_, n)| n.visible && Some(n.file) != panel).map(|(i, _)| i).collect()).clone();
    for i in shown {
        if let Some(n) = scene.nodes.get_mut(i) {
            n.visible = false;
        }
    }
}

/// The stage camera: the camera stands at the game's origin looking along its +z (the app's -z) while the program's stage is on [inferred:
/// the program only moves the stage in front of it, op 0x10b and 0x10c, and no camera command exists]. Set every 60 Hz tick before the
/// field camera runs, as the event player's track is.
fn event_camera(ev: Res<WmEvent>, mut cam: ResMut<CamEvent>) {
    if let Some(play) = ev.play.as_ref()
        && play.camera
    {
        let fov = cam.behind.map_or(1.0, |b| b.fov);
        cam.track = Some((View { pos: STAGE_ORIGIN, look: STAGE_ORIGIN + Vec3::new(0.0, 0.0, -100.0), fov, roll: 0.0 }, false));
    }
}

/// The pad is the event's while it plays (`FUN_00196280`), and the fade node shows the event's fade.
fn event_ui(ev: Res<WmEvent>, mut pad: ResMut<crate::sora::Pad>, mut fade: Query<&mut BackgroundColor, With<EventFade>>) {
    if ev.play.is_some() {
        pad.lock();
    }
    for mut n in &mut fade {
        n.0 = Color::srgba(0.0, 0.0, 0.0, ev.fade_alpha());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_symbols_start_their_event_only_while_the_world_flags_say_so() {
        let flags = |set: &'static [u32]| move |i: u32| set.contains(&i);
        // The Twilight Town symbol: flag 1 set and 0x13 clear.
        assert_eq!(forced_event(0x3a2, flags(&[1])), Some(EVENT_TWILIGHT_TOWN));
        assert_eq!(forced_event(0x3a2, flags(&[1, 0x35])), Some(EVENT_TWILIGHT_TOWN), "the other symbol's flags do not matter");
        assert_eq!(forced_event(0x3a2, flags(&[])), None);
        assert_eq!(forced_event(0x3a2, flags(&[1, 0x13])), None, "the story went on: the prompt opens");
        assert_eq!(forced_event(0x3a2, flags(&[0x13])), None);
        // The TWTNW symbol: 0x35 set and 0x37 clear.
        assert_eq!(forced_event(0x456, flags(&[0x35])), Some(EVENT_TWTNW));
        assert_eq!(forced_event(0x456, flags(&[0x35, 1])), Some(EVENT_TWTNW));
        assert_eq!(forced_event(0x456, flags(&[0x35, 0x37])), None);
        assert_eq!(forced_event(0x456, flags(&[1])), None, "flag 1 belongs to the other symbol");
        assert_eq!(forced_event(0x456, flags(&[])), None);
        // No other symbol has an event, whatever is set.
        assert_eq!(forced_event(0x3a3, flags(&[1, 0x35])), None);
        assert_eq!((EVENT_TWILIGHT_TOWN, EVENT_TWTNW), (1837, 1836));
    }

    #[test]
    fn a_program_waits_where_the_executor_waits() {
        let mut s = Script::new(vec![Cut::FadeOut(12.0), Cut::Wait(12.0), Cut::LoadFile, Cut::FadeIn(12.0), Cut::Message(0x4bb6), Cut::Wait(5.0), Cut::RemoveAll]);
        // The fade does not wait; the wait that follows it holds 12 frames.
        assert_eq!(s.step(1.0, false), [Cut::FadeOut(12.0), Cut::Wait(12.0)]);
        assert!(s.step(5.0, false).is_empty() && s.step(5.0, false).is_empty(), "10 of 12 frames");
        assert_eq!(s.step(2.0, false), [Cut::LoadFile, Cut::FadeIn(12.0), Cut::Message(0x4bb6)], "the wait is over; the message holds the program");
        assert!(s.waiting_for_window() && !s.done());
        assert!(s.step(60.0, false).is_empty(), "a window that has not closed holds it however long");
        assert_eq!(s.step(1.0, true), [Cut::Wait(5.0)]);
        assert!(!s.done());
        assert!(s.step(4.0, false).is_empty());
        assert_eq!(s.step(1.0, false), [Cut::RemoveAll]);
        assert!(s.done());
        assert!(s.step(1.0, false).is_empty());
    }

    #[test]
    fn a_fade_runs_from_one_alpha_to_the_other_over_its_frames() {
        let f = Fade { from: 0.0, to: 1.0, frames: 12.0, t: 6.0 };
        assert_eq!(f.alpha(), 0.5);
        assert_eq!(Fade { t: 99.0, ..f }.alpha(), 1.0);
        assert_eq!(Fade { from: 1.0, to: 0.0, frames: 0.0, t: 0.0 }.alpha(), 0.0, "no frames: at once");
    }

    #[test]
    fn an_actor_stands_where_the_stage_puts_it() {
        let flat = Stage { pos: [2.0, 38.0, 192.0], rot: [0.0; 3] };
        let (at, facing) = actor_place(&flat, [-33.0, 0.0, -20.0], [0.0; 3]);
        assert!((at - Vec3::new(-31.0, -38.0, -172.0)).length() < 1e-3, "the game's y and z flip into the app's: {at}");
        // An actor at yaw 0 faces the app's +z, back at a camera that looks along -z (`app_yaw(0)` is `PI`).
        assert!((facing * Vec3::NEG_Z - Vec3::Z).length() < 1e-4 || (facing * Vec3::Z - Vec3::Z).length() < 1e-4, "{:?}", facing * Vec3::Z);
        // A stage turned about y turns the actor's place around the stage's origin: a point 100 along x ends 100 along z.
        let turned = Stage { pos: [0.0, 0.0, 0.0], rot: [0.0, std::f32::consts::FRAC_PI_2, 0.0] };
        let (at, _) = actor_place(&turned, [100.0, 0.0, 0.0], [0.0; 3]);
        assert!(at.x.abs() < 1e-3 && (at.z.abs() - 100.0).abs() < 1e-3, "{at}");
        // The stage's translation is not turned with it.
        let (at, _) = actor_place(&Stage { pos: [50.0, 0.0, 0.0], rot: [0.0, 1.0, 0.0] }, [0.0; 3], [0.0; 3]);
        assert!((at - Vec3::new(50.0, 0.0, 0.0)).length() < 1e-3);
    }

    /// The two programs on the disc (the `wm` talk table, `FUN_001b6740`): what the executor does in each, by the opcodes of
    /// `FUN_002c4c58`, and that each event's file holds an animation entry for every `Play` and `Queue` and a voice entry for every `Voice`.
    #[test]
    fn the_programs_of_events_1836_and_1837_run_through_the_executors_ops_on_the_disc() {
        let Ok(iso) = std::env::var("KH2_ISO") else { return };
        let disc = Disc::open(std::path::Path::new(&iso));
        let table = Libretto::from_bar(&disc.read(TALK_TABLE).expect("libretto-wm.bar")).expect("the talk table");
        let exe = disc.read_root("SLUS_210.05").expect("executable");
        assert_eq!((symbol_object(&exe, 2), symbol_object(&exe, TWTNW), symbol_object(&exe, 0x7f)), (Some(0x3a2), Some(0x456), None));
        for (id, objects, messages, voices) in [(EVENT_TWTNW, vec![0x862u32, 0x54, 0x5c, 0x5d], vec![0x4bb6u32, 0x4bb7], 2usize), (EVENT_TWILIGHT_TOWN, vec![0x863, 0x895, 0x896], vec![0x4bb8, 0x4bb9], 2)] {
            let cmds: Vec<Cut> = table.program(id).unwrap_or_else(|| panic!("event {id} has no program")).iter().map(|c| c.cut()).collect();
            assert!(!cmds.iter().any(|c| matches!(c, Cut::Other(_))), "event {id} only uses ops the executor knows: {cmds:?}");
            assert_eq!(cmds.first(), Some(&Cut::FadeOut(12.0)), "event {id} fades out first (op 5)");
            assert_eq!(cmds.last(), Some(&Cut::FadeIn(12.0)), "and fades in last (op 6)");
            assert_eq!(cmds.iter().filter(|c| matches!(c, Cut::RemoveAll)).count(), 1, "event {id} removes its actors once");
            let made: Vec<u32> = cmds.iter().filter_map(|c| if let Cut::Actor { object, .. } = c { Some(*object) } else { None }).collect();
            assert_eq!(made, objects, "event {id}'s actors");
            let said: Vec<u32> = cmds.iter().filter_map(|c| if let Cut::Message(m) = c { Some(*m) } else { None }).collect();
            assert_eq!(said, messages, "event {id}'s messages");
            let file = event_file(&disc, id);
            assert!(file.len() >= 8, "event {id}: libretto/us/wm{id}.bar has entries");
            let voice = cmds.iter().filter(|c| matches!(c, Cut::Voice(_))).count();
            assert_eq!(voice, voices);
            for c in &cmds {
                match c {
                    Cut::Play { entries, .. } | Cut::Queue { entries, .. } => {
                        for e in entries {
                            assert!(entry_motion(&file, *e).is_some(), "event {id}: entry {e} is a motion");
                        }
                    }
                    Cut::Voice(e) => assert_eq!(file.get(*e as usize).map(|f| f.1.starts_with(b"VAGp")), Some(true), "event {id}: entry {e} is a VAG voice"),
                    _ => {}
                }
            }
            // Played through, the script hands the executor every command once and ends.
            let mut s = Script::new(cmds.clone());
            let mut ran = 0;
            for _ in 0..100_000 {
                let closed = s.waiting_for_window();
                ran += s.step(1.0, closed).len();
                if s.done() {
                    break;
                }
            }
            assert!(s.done() && ran == cmds.len(), "event {id} ran {ran} of {} commands", cmds.len());
        }
    }

    #[test]
    fn a_symbol_takes_its_motion_and_colour_from_its_worlds_state() {
        let dim = symbol_look([0, 0, 0, 0], 0x8020_2020);
        assert_eq!((dim.slot, dim.effect3, dim.effect5, dim.flag40), (SLOT_DIM, false, false, true));
        assert!((dim.tint[0] - 0.25).abs() < 1e-6 && (dim.tint[3] - 1.0).abs() < 1e-6);
        let hidden = symbol_look([0, 0, 0, 0], 0);
        assert!(!hidden.visible(), "colour word 0 is alpha 0");
        let on = symbol_look([2, 0, 0, 1], 0x8080_8080);
        assert_eq!((on.slot, on.effect3, on.effect5, on.flag40, on.visible()), (SLOT_NORMAL, true, false, false, true));
        // Bit 1 of byte 3 keeps the flag set from state 2 on; effect 5 starts at state 3.
        let top = symbol_look([3, 0, 0, 2], 0x8080_8080);
        assert_eq!((top.effect3, top.effect5, top.flag40), (false, true, true));
        assert_eq!((ring_kind(1, true), ring_kind(2, false), ring_kind(2, true)), (None, Some(6), Some(8)));
        assert_eq!((class_of(0), class_of(1), class_of(2), class_of(3)), (Some(Class::Symbol), Some(Class::Marker), Some(Class::Cursor), None));
    }

    #[test]
    fn a_hidden_twilight_town_shows_normally_and_other_worlds_follow_their_records() {
        assert_eq!(symbol_state(Some(area::TWILIGHT_TOWN), 0), 2, "baseline, or only the other worlds revealed");
        assert_eq!(symbol_state(Some(area::TWILIGHT_TOWN), 1), 1, "a dim state is the record's own");
        assert_eq!(symbol_state(Some(4), 0), 0, "other worlds stay hidden");
        assert_eq!(symbol_state(None, 0), 0);
    }

    #[test]
    fn the_symbol_table_and_colours_read_from_the_executable() {
        let Ok(iso) = std::env::var("KH2_ISO") else { return };
        let exe = Disc::open(std::path::Path::new(&iso)).read_root("SLUS_210.05").expect("SLUS_210.05");
        let worlds: Vec<u8> = (0..12u32).map(|i| symbol_world(&exe, if i < 11 { 0x3a2 + i } else { 0x456 }).expect("symbol in table")).collect();
        let mut sorted = worlds.clone();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(sorted.len(), 12, "twelve distinct worlds: {worlds:?}");
        assert_eq!((worlds[0], worlds[11]), (2, TWTNW), "0x3a2 is Twilight Town, 0x456 The World That Never Was");
        assert_eq!(symbol_world(&exe, 0x3a1), None, "the cursor is not a symbol");
        assert_eq!((colour_word(&exe, 0), colour_word(&exe, 1), colour_word(&exe, 2)), (0, 0x8020_2020, 0x8080_8080));
    }

    #[test]
    fn a_routes_state_shows_its_first_entry_or_its_other_two() {
        // FUN_002c8d18: state 1 shows entry 0 only, state 2 entries 1 and 2 only, the others (0, 3 and up) none.
        for (state, want) in [(0u8, [false, false, false]), (1, [true, false, false]), (2, [false, true, true]), (3, [false, false, false])] {
            assert_eq!([0, 1, 2].map(|k| route_shows(state, k)), want, "state {state}");
        }
        // Two routes name entries 0..2 and 3..5; an entry no route names stays shown.
        let routes = [(1u8, [0usize, 1, 2]), (2, [3, 4, 5])];
        let shown = prop_visibility(7, &routes, |r| if r == 1 { 1 } else { 2 });
        assert_eq!(shown, [true, false, false, false, true, true, true]);
        assert_eq!(prop_visibility(3, &[(1, [0, 1, 9])], |_| 1), [true, false, true], "an entry past the made props is ignored, an unnamed one stays shown");
    }

    #[test]
    fn a_prop_table_reads_its_records_and_the_visibility_groups_pick_the_made_ones() {
        let mut b = vec![0u8; 8 + 2 * PROP_RECORD];
        b[0..4].copy_from_slice(&8u32.to_le_bytes());
        b[4..8].copy_from_slice(&((2 * PROP_RECORD) as u32).to_le_bytes());
        let put = |b: &mut Vec<u8>, rec: usize, word: usize, v: u32| b[8 + rec * PROP_RECORD + word * 4..][..4].copy_from_slice(&v.to_le_bytes());
        put(&mut b, 1, 0, 100f32.to_bits());
        put(&mut b, 1, 4, 0.5f32.to_bits());
        put(&mut b, 1, 6, 2f32.to_bits());
        put(&mut b, 1, 9, 7);
        put(&mut b, 1, 10, 3);
        let props = parse_props(&b);
        assert_eq!(props.len(), 2);
        assert_eq!((props[1].pos[0], props[1].rot[1], props[1].scale[0], props[1].model, props[1].group), (100.0, 0.5, 2.0, 7, 3));
        // A size that claims more records than the bytes hold keeps the ones that are there; a short entry has none.
        b[4..8].copy_from_slice(&(10 * PROP_RECORD as u32).to_le_bytes());
        assert_eq!(parse_props(&b).len(), 2);
        assert!(parse_props(&b[..6]).is_empty());
        assert!(prop_made(0, 0x3) && prop_made(1, 0x3) && !prop_made(2, 0x3) && !prop_made(64, u64::MAX));
    }

    #[test]
    fn the_wm00_props_hold_the_route_entries_at_each_markers_site() {
        let Ok(iso) = std::env::var("KH2_ISO") else { return };
        let disc = Disc::open(std::path::Path::new(&iso));
        let exe = disc.read_root("SLUS_210.05").expect("SLUS_210.05");
        let file = disc.read("map/us/wm00.map").expect("wm00.map");
        let table = kh2_bar::parse(&file).unwrap().iter().find(|x| x.kind == PROP_TABLE_KIND).map(|x| parse_props(x.data)).expect("prop table");
        assert_eq!(table.len(), 77, "records of the table");
        // Map program 0 shows 0x3: the props of groups 0 and 1 are made, in table order, and the route entries index that list.
        let made: Vec<&Prop> = table.iter().filter(|p| prop_made(p.group, 0x3)).collect();
        assert_eq!(made.len(), 57);
        let mut seen = std::collections::BTreeSet::new();
        for route in 1..=ROUTES {
            let e = route_entries(&exe, route).expect("route row");
            assert!(e.iter().all(|n| *n < made.len() && seen.insert(*n)), "route {route} entries {e:?} are made props and not shared");
            // The two first entries are the same site (two models at one place); the third sits beside them or, for a model
            // that carries its own world coordinates, at the origin.
            let (a, b) = (made[e[0]], made[e[1]]);
            assert_eq!((a.pos, a.rot), (b.pos, b.rot), "route {route}: entries {} and {} stand at one site", e[0], e[1]);
            assert!(a.pos[0].abs() > 1000.0, "the site is away from the origin");
        }
        assert_eq!(route_entries(&exe, 1), Some([0, 9, 18]));
        assert_eq!(route_entries(&exe, 0), Some([0, 0, 0]), "route 0 has no marker; its row is zero");
        assert_eq!(route_entries(&exe, 10), None);
        assert_eq!(seen.len(), 27, "nine routes of three entries each");
        // The BOB models the records name exist.
        let models = bob_models(&file);
        assert!(made.iter().all(|p| (p.model as usize) < models.len()), "{} models", models.len());
        assert!(models[made[0].model as usize].meshes.len() > 0 && models[made[9].model as usize].meshes.len() > 0, "the marker models have meshes");
    }
}
