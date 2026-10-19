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
//! Cursor: the Gummi ship (`WM_CURSOR`). Its real states (`FUN_002cca58`, `FUN_002ccb70`, ...) were not read
//! [GAP], so this first version stands Sora at the cursor's record and moves the cursor with him: the stick
//! walks Sora on the map's collision, the follow camera follows him, and his body and the party are hidden
//! [I]. The route markers get no motion (their update was not read).
//!
//! `KH2_WMLOG=1` logs what is spawned.

use std::sync::Arc;

use bevy::mesh::skinning::SkinnedMeshInverseBindposes;
use bevy::prelude::*;
use kh2_tables::tables as t;

use crate::area::{app_yaw, game_to_app, AreaRecord, WORLD_MAP};
use crate::assets::{AreaPart, Disc};
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
        app.init_resource::<MapSpawned>().add_systems(
            Update,
            (map_spawn.after(crate::area::run_area_script), map_start, map_cursor_follow, map_hide_party).chain(),
        );
    }
}

fn wmlog() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| std::env::var("KH2_WMLOG").is_ok_and(|v| !v.is_empty() && v != "0"))
}

/// The map area the objects were spawned for.
#[derive(Resource, Default)]
struct MapSpawned(Option<String>);

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
                let at = game_to_app(e.position);
                let on_floor = world.ground(at, at.y + 100.0, 400.0).map(|g| g.y);
                let yaw = app_yaw(e.rotation[1]);
                let (slot, tint, note) = match class {
                    Class::Symbol => {
                        let w = symbol_world(&exe, e.object);
                        let record = w.map_or([0; 4], |w| progress.as_ref().map_or([0; 4], |p| p.story().world_record(w as usize)));
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

#[cfg(test)]
mod tests {
    use super::*;

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
}
