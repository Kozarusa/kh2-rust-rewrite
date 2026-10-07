//! Party members, stages 1 and 2 of docs/party.md section 9: Donald and Goofy (whoever the world's
//! party names) stand where the area's `map` script puts `FRIEND_1` / `FRIEND_2`, with their models,
//! motion sets and weapons, and follow the player under the friend AI's follow rules: the follow
//! points, the speed modes, the steer, the player's trail and the warp (catch-up). In battle (the
//! game's battle flag, FUN_001903d8) they choose targets and actions and attack (stage 2).
//!
//! Who: the world's party slot bytes start from preset 1 (`party_rules::apply_preset`, the writer of
//! the new-game bytes is a gap, docs/party.md section 10); `FRIEND_1` is slot byte 1 and `FRIEND_2`
//! slot byte 2 (FUN_0016e208 -> FUN_0017cd20), resolved through the world's merged `memt` record.
//! The member takes the first free party slot (FUN_0017cd08) and the follow position slot + 1.
//!
//! Each 60 Hz tick (`party_tick`) runs the party manager (FUN_00111d18: the trail, the follow points)
//! and then, per member that is free (not in an attack motion), the decision loop FUN_00111070: the
//! far counter, out of battle the follow (stage 1), in battle the target (FUN_0010df08, style 0
//! FUN_0010d0c0) and the action choice (FUN_0010fcd0, `party_battle`); the steer; the movement; then
//! the queued attack command (FUN_001112e8 -> FUN_00174cf8 -> FUN_00110790: the entry's motion through
//! the player-type remap FUN_0017da30) and the warp check (FUN_00110eb0). The locomotion motion is the
//! motion id of the speed (0 stand, 1 walk, 2 run) [inferred]; the member turns toward its movement at
//! its `prty` turn speed [inferred].
//!
//! Log: `KH2_PARTYLOG=1` prints speed-mode changes, warps, the targets and the chosen actions, the
//! actions per second and a status line per second; the spawn lines always print. Harness:
//! `KH2_NO_PARTY=1` spawns nobody; `KH2_PARTY_WARP=<s>` at that time asks every member for a warp the
//! way host function 0x2eec20 does (FUN_00110e08: far counter 1000, no cooldown, nearest trail
//! point); `KH2_PARTY_WARP_ONSCREEN=1` lets that forced warp happen in view (a harness aid to see it:
//! the game warps only off screen); `KH2_PARTY_WEAPON_BONE=<n>` overrides the weapons' bone (the bone
//! search aid).

use std::collections::HashMap;
use std::sync::Arc;

use bevy::color::Alpha;
use bevy::mesh::skinning::SkinnedMeshInverseBindposes;
use bevy::prelude::*;
use kh2_anim::id;
use kh2_tables::tables as t;

use crate::assets::{AreaPart, CurrentArea, Disc};
use crate::combat::{Fighter, Hit, Team};
use crate::data::GameData;
use crate::motor::Motor;
use crate::party_battle::{self as battle, Choice, ClassAi, Handler, MemberNow};
use crate::party_rules::{self as rules, Consts, Follow, FollowIn, Thresholds, Trail};
use crate::schedule::Sim;
use crate::skel::{spawn_skinned, Animator, Rig};
use crate::sora::Sora;
use crate::world::World;
use crate::SCALE;

/// Blend of the locomotion motions, as Sora's (`sora::BLEND_LOCOMOTION`).
const BLEND: f32 = 8.0;
/// The frame unit the timers count down by (`DAT_00356f40`, one 60 Hz frame).
const FRAME: f32 = 1.0;

pub struct PartyPlugin;

impl Plugin for PartyPlugin {
    fn build(&self, app: &mut App) {
        let data = PartyData::load(app.world().resource::<Disc>());
        app.insert_resource(data)
            .init_resource::<PartySlots>()
            .insert_resource(Manager { log: std::env::var("KH2_PARTYLOG").is_ok_and(|v| !v.is_empty() && v != "0"), rng: 0x2545_f491, ..Default::default() })
            .add_systems(FixedUpdate, (spawn_party, party_tick, party_push_apart, party_fade, party_hits).chain().in_set(PartyTick).in_set(Sim::Enemy))
            .add_systems(FixedUpdate, member_hands.in_set(Sim::Enemy).before(PartyTick));
        crate::friend_ai::register(app);
    }
}

/// The party's own systems (spawn, tick, fade); the members' scripts run after them.
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct PartyTick;

/// The tables the weapon lookup reads (FUN_0016ef28 -> FUN_001845b0 `went`, FUN_00184640 `wmst`).
#[derive(Default)]
struct WeaponTables {
    items: Vec<t::Item>,
    went: Vec<u8>,
    wmst: Vec<[String; 2]>,
}

/// The disc and executable data the party needs.
#[derive(Resource)]
pub struct PartyData {
    consts: Option<Consts>,
    /// Per AI class, its parameter block and action records (`party_battle::ClassAi`).
    ai: HashMap<u8, ClassAi>,
    /// FUN_0017da30's slot orders (0x37a5c8).
    orders: [[u8; 4]; 4],
    /// 0x3784ac: style 2 halves the battle distance (FUN_0010df08).
    half: f32,
    /// 0x378564 / 0x378568: FUN_0010e5e8's stop when the target is right above.
    stop: (f32, f32),
    objs: Vec<t::Objentry>,
    memt: Option<kh2_tables::player::Memt>,
    prty: Vec<t::PrefPrty>,
    weapons: WeaponTables,
    /// Loaded rigs by model name (`None`: not loadable).
    rigs: HashMap<String, Option<Arc<Rig>>>,
}

impl PartyData {
    fn load(disc: &Disc) -> PartyData {
        let exe = disc.read_root("SLUS_210.05");
        let consts = exe.as_deref().and_then(Consts::from_exe);
        if consts.is_none() {
            warn!("party: the friend AI's constants are not readable from SLUS_210.05; nobody follows");
        }
        let mut ai = HashMap::new();
        let (mut orders, mut half, mut stop) = ([[2, 3, 1, 0]; 4], 0.5, (0.0, 0.0));
        if let Some(elf) = exe.as_deref() {
            for class in 0..rules::CLASSES as u8 {
                if let Some(c) = ClassAi::from_exe(elf, class) {
                    ai.insert(class, c);
                }
            }
            orders = battle::motion_orders(elf).unwrap_or(orders);
            let f = |va: u32| rules::exe_bytes(elf, va, 4).map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]));
            half = f(0x3784ac).unwrap_or(half);
            stop = (f(0x378564).unwrap_or(0.0), f(0x378568).unwrap_or(0.0));
        }
        let (objs, memt, prty, weapons) = match disc.0.lock() {
            Ok(mut a) => (
                kh2_tables::load::<t::Objentry>(&mut a, &t::OBJENTRY).map(|d| d.rows).unwrap_or_default(),
                kh2_tables::player::load_memt(&mut a).ok(),
                kh2_tables::load::<t::PrefPrty>(&mut a, &t::PREF_PRTY).map(|d| d.rows).unwrap_or_default(),
                WeaponTables {
                    items: kh2_tables::load::<t::Item>(&mut a, &t::ITEM).map(|d| d.rows).unwrap_or_default(),
                    went: kh2_tables::subfile(&mut a, &t::WENT).unwrap_or_default(),
                    wmst: kh2_tables::subfile(&mut a, &t::WMST).map(|b| crate::drive::rules::parse_wmst(&b)).unwrap_or_default(),
                },
            ),
            Err(_) => (Vec::new(), None, Vec::new(), WeaponTables::default()),
        };
        PartyData { consts, ai, orders, half, stop, objs, memt, prty, weapons, rigs: HashMap::new() }
    }

    /// FUN_0017da30's slot order of a motion-state row.
    pub(crate) fn order(&self, row: usize) -> [u8; 4] {
        self.orders[row.min(3)]
    }

    /// The model (`obj/<model>.mdlx`) with the motion set its `objentry` row names.
    fn rig(&mut self, disc: &Disc, row: &t::Objentry) -> Option<Arc<Rig>> {
        let model = row.model_name.clone();
        let anim = row.animation_name.trim_end_matches(".mset").to_string();
        self.rigs
            .entry(model.clone())
            .or_insert_with(|| {
                let m = disc.read(&format!("obj/{model}.mdlx")).and_then(|d| kh2_model::mdlx(&d))?;
                let s = disc.read(&format!("obj/{anim}.mset")).and_then(|d| kh2_anim::Mset::parse(&d))?;
                let apdx = Some(format!("obj/{model}.apdx")).filter(|a| disc.read(a).is_some());
                Some(Arc::new(Rig { model: m, mset: s, apdx }))
            })
            .clone()
    }

    /// FUN_0017c718's hand object `hand` of a member: FUN_0016ef28 takes the member's saved weapon
    /// word (`item`), FUN_001845b0 the weapon object of the member's moveset for the item's
    /// `Flag1 | Rank << 8` slot (`went`), FUN_00184640 the hand's motion set name (`wmst`). `None` when
    /// any step names nothing.
    fn weapon_files(&self, row: &t::Objentry, item: u16, hand: usize) -> Option<crate::drive::WeaponFiles> {
        let it = self.weapons.items.iter().find(|i| i.id == item)?;
        let slot = it.flag1 as u16 | (it.rank as u16) << 8;
        let ms = row.neo_moveset as usize;
        let id = crate::drive::rules::weapon_object(&self.weapons.went, ms, slot)?;
        let w = self.objs.iter().find(|w| w.object_id == id)?;
        let mset = self.weapons.wmst.get(ms).and_then(|n| n.get(hand)).filter(|s| !s.is_empty()).cloned();
        Some(crate::drive::WeaponFiles { model: w.model_name.clone(), mset })
    }
}

/// The current world's party: the four slot bytes (`0x340c9c + world x 4`: player, the two
/// companions, the partner; low five bits the member index, 0x10 empty).
#[derive(Resource, Default, Debug)]
pub struct PartySlots {
    pub world: u16,
    pub bytes: [u8; 4],
}

/// The party manager's state (FUN_00112168 / FUN_00111d18).
#[derive(Resource, Default)]
struct Manager {
    area: Option<String>,
    trail: Trail,
    /// `DAT_0034e980`: 2 warps any time, 1 only out of battle, 0 never (FUN_00112270).
    warp_mode: u8,
    tick: u64,
    rng: u32,
    log: bool,
    forced_done: bool,
    in_battle: bool,
}

impl Manager {
    /// A random number below `n` ([inferred] stand-in for FUN_0010f420; the generator's state is not
    /// the game's).
    fn random(&mut self, n: u32) -> u32 {
        self.rng ^= self.rng << 13;
        self.rng ^= self.rng >> 17;
        self.rng ^= self.rng << 5;
        self.rng % n.max(1)
    }

    /// FUN_0010f438: 0..1 ([inferred] stand-in as above).
    fn roll(&mut self) -> f32 {
        self.random(20001) as f32 / 20000.0
    }

    /// FUN_00106408: a random value in `[a, b]`.
    fn between(&mut self, (a, b): (f32, f32)) -> f32 {
        a + self.roll() * (b - a)
    }
}

/// The member's target (+0x9f4): an enemy fighter and one of its hurt shapes (by index among its
/// BATTLE / TARGET shapes).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Target {
    entity: Entity,
    shape: usize,
}

/// An attack motion that runs (the member is not free: +0x160 bits clear).
#[derive(Clone, Copy, Debug)]
struct Attacking {
    record: usize,
    slot: usize,
}

/// One party member in the field.
#[derive(Component)]
pub struct PartyMember {
    /// Party slot 0 or 1 (+0xb48); the follow position is slot + 1 (+0xb70).
    pub slot: u8,
    pub neo: u16,
    pub class: u8,
    pub model: String,
    pub body: Entity,
    motor: Motor,
    follow: Follow,
    thresholds: Thresholds,
    /// +0x110 / +0x114 (the `prty` row of the NeoStatus, [inferred]) and the turn rate; +0x118 the jump
    /// height ([inferred] the row's JumpHeight).
    walk: f32,
    run: f32,
    turn: f32,
    jump: f32,
    /// The body shape (+0x5b0 / +0x5b8 radius, +0x5b4 height).
    radius: f32,
    height: f32,
    /// +0xcd8 far counter, +0xcdc warp cooldown, +0xce0 warps since the last trail point.
    pub far: i32,
    pub cooldown: f32,
    pub warps: i32,
    /// +0xb7c bit 1 (a warp's fade-in runs) and bit 0x8000 (the forced warp of FUN_00110e08).
    warping: bool,
    forced: bool,
    /// Frames into the warp's fade-in.
    fade: Option<f32>,
    alpha: f32,
    applied_alpha: f32,
    original_modes: HashMap<AssetId<StandardMaterial>, AlphaMode>,
    /// The last movement step was blocked ([inferred] reading of +0x660 bit 4).
    blocked: bool,
    speed: f32,
    /// Stage 2: the action choice's state, the wait timer (+0xcd4), the target, bit 8 (the target
    /// changed this frame), the held velocity of Goofy's wait, the running attack, the wanted facing
    /// (+0x66c) and the actions chosen since the last per-second line.
    choosing: battle::Choosing,
    timer: f32,
    target: Option<Target>,
    target_changed: bool,
    held: Option<(Vec3, f32)>,
    attack: Option<Attacking>,
    want_yaw: Option<f32>,
    chosen: Vec<String>,
    /// The motion slot the member's script asked for last (FUN_0017e220 remapped), played while the
    /// member stands; `friend_ai` writes it.
    pub(crate) script_slot: Option<usize>,
    /// FUN_0017da30's state row now (1 battle, 2 field).
    pub(crate) motion_row: usize,
    pub(crate) free: bool,
}

impl PartyMember {
    /// Position (game units, the app's axes) and facing (app yaw).
    pub(crate) fn position(&self) -> Vec3 {
        self.motor.pos
    }

    pub(crate) fn yaw(&self) -> f32 {
        self.motor.yaw
    }
}

/// The player's objentry id in spawn records (`PLAYER`, FUN_0016e208).
const PLAYER_ENTITY: u32 = 0x236;

/// The bone the members' hand objects hang on, by model: found by looking at the hand on screen with
/// `KH2_PARTY_WEAPON_BONE` (as Sora's 178 was); the game's attach path (FUN_001751b0 -> FUN_00184190)
/// ends in resource handles, not a bone number, and objentry `WeaponJoint` (2 Donald, 3 Goofy) is not a
/// bone index of these models (docs/party.md section 11).
fn weapon_bone(model: &str) -> Option<usize> {
    if let Some(b) = std::env::var("KH2_PARTY_WEAPON_BONE").ok().and_then(|v| v.parse().ok()) {
        return Some(b);
    }
    match model {
        "P_EX020" => Some(DONALD_HAND),
        "P_EX030" => Some(GOOFY_HAND),
        _ => None,
    }
}

/// Donald's and Goofy's weapon bones (`weapon_bone`): the wrist bones with four finger children on the
/// bind pose's -X side, the side of Sora's 178 (bone dump of the models); on screen the staff and the
/// shield sit in the hand and face outward with these, and Goofy's shield turns edge-on into his leg on
/// the other wrist (27).
const DONALD_HAND: usize = 131;
const GOOFY_HAND: usize = 101;

/// Marks a member's weapon socket: the entity follows the bone of the member's body.
#[derive(Component)]
struct MemberHand {
    body: Entity,
    bone: usize,
}

/// The rows of the area's `map` script that place `FRIEND_1` / `FRIEND_2` (and the player's row of
/// the first friend's group): (objentry id, group, position in game units in the app's axes, app
/// yaw, entrance byte +0x1d), in script order, the first row of each object.
fn friend_records(disc: &Disc, area: &str) -> Vec<(u32, String, Vec3, f32, u8)> {
    let Some(file) = disc.read(&format!("ard/{area}.ard")) else { return Vec::new() };
    let Some((ard, _)) = kh2_ard::Area::parse(&file) else { return Vec::new() };
    let mut out: Vec<(u32, String, Vec3, f32, u8)> = Vec::new();
    for p in ard.scripts.get("map").into_iter().flatten() {
        for name in p.spawns() {
            let friends_here = ard.spawns.get(&name).into_iter().flatten().flat_map(|g| &g.entities).any(|e| e.object == rules::FRIEND_1 || e.object == rules::FRIEND_2);
            for e in ard.spawns.get(&name).into_iter().flatten().flat_map(|g| &g.entities) {
                let wanted = e.object == rules::FRIEND_1 || e.object == rules::FRIEND_2 || (e.object == PLAYER_ENTITY && friends_here);
                if wanted && !out.iter().any(|r| r.0 == e.object) {
                    let [x, y, z] = e.position;
                    out.push((e.object, name.clone(), Vec3::new(x, -y, -z), std::f32::consts::PI - e.rotation[1], e.raw[0x1d]));
                }
            }
        }
    }
    out
}

fn body_shape(rig: &Rig) -> (f32, f32) {
    rig.model.shapes.iter().find(|s| s.kind == 0 || s.kind == 4).map_or((35.0, 150.0), |s| (s.radius.abs(), s.height.abs()))
}

/// Spawns the members of a newly loaded area (they are area parts: the next area load removes them).
#[allow(clippy::too_many_arguments)]
fn spawn_party(
    mut commands: Commands,
    disc: Res<Disc>,
    area: Res<CurrentArea>,
    world: Option<Res<World>>,
    drive_world: Option<Res<crate::drive::DriveWorld>>,
    progress: Option<Res<crate::progress::Progress>>,
    (mut data, mut slots, mut mgr): (ResMut<PartyData>, ResMut<PartySlots>, ResMut<Manager>),
    mut sora: Query<(&mut Transform, &mut Sora), Without<PartyMember>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut images: ResMut<Assets<Image>>,
    mut bindposes: ResMut<Assets<SkinnedMeshInverseBindposes>>,
) {
    let Some(w) = world else { return };
    if area.loaded == mgr.area {
        return;
    }
    mgr.area = area.loaded.clone();
    mgr.trail = Trail::default();
    let Some(name) = area.loaded.clone() else { return };
    if std::env::var("KH2_NO_PARTY").is_ok_and(|v| v != "0") {
        return;
    }
    let world_id = drive_world.map_or(2, |d| d.id);
    // FUN_00112270: warp mode 1 in world 8 area 8 and world 13 area 3, else 2 (the area number is
    // the digits of the area's name).
    let area_no: u16 = name.trim_start_matches(|c: char| c.is_ascii_alphabetic()).parse().unwrap_or(0);
    mgr.warp_mode = if matches!((world_id, area_no), (8, 8) | (13, 3)) { 1 } else { 2 };
    let Some(memt) = data.memt.clone() else {
        warn!("party: memt not readable; nobody spawns");
        return;
    };
    let preset = memt.trailing.get(rules::DEFAULT_PRESET).map_or([0, rules::EMPTY, rules::EMPTY, rules::EMPTY], |w| w.to_le_bytes());
    slots.world = world_id;
    slots.bytes = rules::apply_preset([0, rules::EMPTY, rules::EMPTY, rules::EMPTY], preset);
    let merged = crate::drive::rules::memt_merge(&memt, world_id, |_| false);
    info!("party: {name} world {world_id} slot bytes {:02x?} (preset {}), warp mode {}", slots.bytes, rules::DEFAULT_PRESET, mgr.warp_mode);
    let Some(consts) = data.consts.clone() else { return };
    let (player_rec, records): (Vec<_>, Vec<_>) = friend_records(&disc, &name).into_iter().partition(|r| r.0 == PLAYER_ENTITY);
    if records.is_empty() {
        info!("party: {name}: the map script places no FRIEND_1 / FRIEND_2");
    }
    // Harness: `KH2_PARTY_AT_RECORD=1` puts Sora at the player's row of the friends' group (the app
    // starts him at its own spawn point; the game places the player at his row, docs/events.md). A
    // test aid, not the game's rule.
    if std::env::var("KH2_PARTY_AT_RECORD").is_ok_and(|v| v == "1")
        && let (Some(r), Ok((mut tf, mut s))) = (player_rec.first(), sora.single_mut())
    {
        let at = r.2.with_y(w.ground(r.2, r.2.y + 100.0, 400.0).map_or(r.2.y, |g| g.y));
        crate::sora::respawn(&mut s, at);
        s.motor.yaw = r.3;
        tf.translation = at * SCALE;
        info!("party: Sora placed at the player's row of {} at {at:?}", r.1);
    }
    let mut next_slot = 0u8;
    for (entity, group, at, yaw, entrance) in records {
        let byte = slots.bytes[if entity == rules::FRIEND_1 { 1 } else { 2 }];
        let Some(obj) = rules::slot_object(&merged, byte) else {
            info!("party: {group}: object {entity:#x}: slot byte {byte:#04x} names nobody");
            continue;
        };
        let Some(row) = data.objs.iter().find(|r| r.object_id == obj as u32).cloned() else { continue };
        let Some(class) = rules::ai_class(row.neo_status, row.neo_moveset) else { continue };
        let Some(rig) = data.rig(&disc, &row) else {
            warn!("party: {}: model or motion set {} not loadable", row.model_name, row.animation_name);
            continue;
        };
        let (radius, height) = body_shape(&rig);
        let prty = data.prty.get(row.neo_status as usize);
        let (walk, run, turn, jump) = prty.map_or((0.0, 0.0, 0.0, 0.0), |p| (p.walk_speed, p.run_speed, p.turn_speed, p.jump_height));
        let ground = w.ground(at, at.y + 100.0, 400.0).map_or(at.y, |g| g.y);
        let mut motor = Motor { pos: at.with_y(ground), yaw, grounded: true, ..Motor::default() };
        motor.capsule.radius = radius;
        let slot = next_slot;
        next_slot += 1;
        let root = commands.spawn((AreaPart, Transform::from_translation(motor.pos * SCALE).with_rotation(Quat::from_rotation_y(yaw)), Visibility::default())).id();
        let body = spawn_skinned(&mut commands, root, rig.clone(), &mut meshes, &mut materials, &mut images, &mut bindposes, SCALE);
        // FUN_0017c718: the hand objects. Hand 0 is the member's saved weapon word (FUN_0016ee98).
        // The app's new game leaves this word 0 (its writer in the original was not found): no hand
        // object then, as FUN_0016ee98 answers 0. `KH2_PARTY_WEAPONS=<Donald item>,<Goofy item>` is a
        // harness aid that hands in an item for a zero word.
        let saved = progress.as_ref().and_then(|p| p.member(row.neo_status)).map_or(0, |m| m.weapons[0]);
        let aid = std::env::var("KH2_PARTY_WEAPONS").ok().and_then(|v| v.split(',').nth(row.neo_status.saturating_sub(2) as usize).and_then(|s| s.trim().parse::<u16>().ok()));
        let item = if saved != 0 { saved } else { aid.unwrap_or(0) };
        let mut weapons = Vec::new();
        match (data.weapon_files(&row, item, 0), weapon_bone(&row.model_name)) {
            (Some(files), Some(bone)) => match crate::drive::weapon_rig(&disc, &files) {
                Some(wrig) => {
                    let socket = commands.spawn((Transform::default(), Visibility::default(), ChildOf(body), MemberHand { body, bone })).id();
                    weapons.push(spawn_skinned(&mut commands, socket, wrig, &mut meshes, &mut materials, &mut images, &mut bindposes, 1.0));
                    info!("party: {} weapon {} (item {item}, motion set {:?}) on bone {bone}", row.model_name, files.model, files.mset);
                }
                None => warn!("party: {}: weapon {} not loadable", row.model_name, files.model),
            },
            (Some(files), None) => info!("party: {}: weapon {} not attached (no hand bone known for this model)", row.model_name, files.model),
            (None, _) => info!("party: {}: no weapon (item {item}: no went entry for moveset {})", row.model_name, row.neo_moveset),
        }
        info!(
            "party: spawn {} slot {slot} ({} at {group} entrance {entrance}, NeoStatus {}, AI class {class}, walk {walk} run {run}, radius {radius}) at {:?}",
            row.model_name,
            if entity == rules::FRIEND_1 { "FRIEND_1" } else { "FRIEND_2" },
            row.neo_status,
            motor.pos
        );
        let mut attack_bodies = vec![body];
        attack_bodies.extend(weapons.iter().copied());
        commands.entity(root).insert((
            PartyMember {
                slot,
                neo: row.neo_status,
                class,
                model: row.model_name.clone(),
                body,
                motor,
                follow: Follow::default(),
                thresholds: consts.thresholds(class),
                walk,
                run,
                turn,
                jump,
                radius,
                height,
                far: 0,
                cooldown: 0.0,
                warps: 0,
                warping: false,
                forced: false,
                fade: None,
                alpha: 1.0,
                applied_alpha: 1.0,
                original_modes: HashMap::new(),
                blocked: false,
                speed: 0.0,
                choosing: Default::default(),
                timer: 0.0,
                target: None,
                target_changed: false,
                held: None,
                attack: None,
                want_yaw: None,
                chosen: Vec::new(),
                script_slot: None,
                motion_row: battle::motion_state(false, false),
                free: true,
            },
            // The members' attacks hit enemies (stage 2); they cannot be hit before stage 3.
            Fighter { team: Team::Player, attack_bodies, hurt_body: body, landed: Default::default(), hittable: false },
        ));
        if next_slot >= 2 {
            break;
        }
    }
}

/// Keeps every member's weapon socket on its hand bone (the joint entities are siblings under the
/// body, as Sora's `sora::follow_hand`), and plays the body's slot on the weapon (the weapons'
/// motion sets mirror the body's slots, as Sora's `sora::sync_weapon`).
///
/// The hand objects show only while the battle flag is set [inferred from the user's reference video
/// `10_menu_open_close.mp4` at 0.0 s: out of battle Donald and Goofy hold nothing; the game's switch
/// (FUN_001752a0 and +0x588 bit 4 select the weapon rows of FUN_0017da30) was not traced further].
fn member_hands(flag: Option<Res<crate::audio_bgm::BattleFlag>>, mut sockets: Query<(&MemberHand, &mut Transform, &mut Visibility, &Children)>, mut anims: Query<&mut Animator>) {
    let shown = if flag.is_some_and(|f| f.active()) { Visibility::Inherited } else { Visibility::Hidden };
    for (h, mut t, mut vis, kids) in &mut sockets {
        if *vis != shown {
            *vis = shown;
        }
        let Ok(a) = anims.get(h.body) else { continue };
        *t = Transform::from_matrix(a.pose.get(h.bone).copied().unwrap_or(Mat4::IDENTITY));
        let (slot, frame, prev, looping, speed) = (a.slot, a.frame, a.prev_frame, a.looping, a.speed);
        for k in kids.iter() {
            let Ok(mut w) = anims.get_mut(k) else { continue };
            let slot = slot.filter(|&x| w.rig.mset.has(x));
            if w.slot != slot {
                w.restart(slot, looping, 0.0);
            }
            w.frame = frame;
            w.prev_frame = prev;
            w.looping = looping;
            w.speed = speed;
        }
    }
}

/// Whether a box (game units, app axes: centre of the base, radius, height) shows on screen
/// ([inferred] stand-in for FUN_00146648 on the view 0x3b38d0 with the model's box +0x740..+0x754).
fn on_screen(cam: Option<(&Camera, &GlobalTransform)>, base: Vec3, r: f32, h: f32) -> bool {
    let Some((c, g)) = cam else { return false };
    [-r, r]
        .into_iter()
        .flat_map(|x| [-r, r].into_iter().flat_map(move |z| [0.0, h].map(|y| Vec3::new(x, y, z))))
        .filter_map(|o| c.world_to_ndc(g, (base + o) * SCALE))
        .any(|n| n.x.abs() <= 1.0 && n.y.abs() <= 1.0 && (0.0..=1.0).contains(&n.z))
}

/// A target's hurt shape now: centre (game units, app axes) and radius; `None` when the fighter is
/// gone, not hittable or has no such shape ([inferred] mapping of FUN_00112ac0's validity).
fn shape_of(t: Target, fighters: &[(Entity, Vec<(Vec3, f32)>)]) -> Option<(Vec3, f32)> {
    fighters.iter().find(|f| f.0 == t.entity).and_then(|f| f.1.get(t.shape).copied())
}

/// The relation FUN_0010b428 keeps (+0xb80 player, +0xbb0 target): the GAME's height difference
/// (target Y minus own Y with Y down), the ground direction and distance (the 3D parts are not used).
#[derive(Clone, Copy, Debug, Default)]
struct Rel {
    dy: f32,
    gdir: Vec3,
    gd: f32,
}

fn relation(from: Vec3, to: Vec3) -> Rel {
    let d = to - from;
    Rel { dy: -(to.y - from.y), gdir: d.with_y(0.0).normalize_or_zero(), gd: d.with_y(0.0).length() }
}

/// One tick of the party manager and of every member.
#[allow(clippy::too_many_arguments)]
fn party_tick(
    data: Res<PartyData>,
    game: Res<GameData>,
    mut mgr: ResMut<Manager>,
    world: Option<Res<World>>,
    flag: Option<Res<crate::audio_bgm::BattleFlag>>,
    progress: Option<Res<crate::progress::Progress>>,
    sora: Query<&Sora>,
    cam: Query<(&Camera, &GlobalTransform), With<Camera3d>>,
    fighters: Query<(Entity, &Fighter), Without<PartyMember>>,
    mut q: Query<(&mut Transform, &mut PartyMember)>,
    mut anims: Query<(&mut Animator, &GlobalTransform)>,
) {
    let (Some(c), Some(w), Ok(s)) = (data.consts.as_ref(), world, sora.single()) else { return };
    if q.is_empty() {
        return;
    }
    mgr.tick += 1;
    let log = mgr.log;
    let status = log && mgr.tick.is_multiple_of(60);
    let cam = cam.single().ok();
    let (player, player_yaw) = (s.motor.pos, s.motor.yaw);
    // FUN_001903d8: the battle flag.
    let in_battle = flag.as_ref().is_some_and(|f| f.active());
    if in_battle != mgr.in_battle {
        mgr.in_battle = in_battle;
        if log {
            info!("party: battle flag {}", if in_battle { "set: the members fight" } else { "clear: the members follow" });
        }
    }
    // The enemies' hurt shapes (centre in game units, radius): the members' possible targets.
    let enemies: Vec<(Entity, Vec<(Vec3, f32)>)> = fighters
        .iter()
        .filter(|(_, f)| f.team == Team::Enemy && f.hittable)
        .filter_map(|(e, f)| {
            let (a, g) = anims.get(f.hurt_body).ok()?;
            let shapes = a.rig.model.shapes.iter().filter(|s| s.kind == kh2_model::collision::kind::BATTLE || s.kind == kh2_model::collision::kind::TARGET);
            Some((e, shapes.map(|s| crate::combat::hurt_spheroid(s, a, g)).map(|h| (h.center / SCALE, h.radius / SCALE)).collect()))
        })
        .collect();

    // FUN_00110e08 by harness: at the given second every member asks for a forced warp.
    if !mgr.forced_done && let Some(at) = std::env::var("KH2_PARTY_WARP").ok().and_then(|v| v.parse::<f32>().ok()) && mgr.tick as f32 >= at * 60.0 {
        mgr.forced_done = true;
        for (_, mut m) in &mut q {
            m.far = 1000;
            m.cooldown = 0.0;
            m.forced = true;
            info!("party: {} forced warp requested (far counter 1000)", m.model);
        }
    }
    let warp_on_screen = std::env::var("KH2_PARTY_WARP_ONSCREEN").is_ok_and(|v| v == "1");

    // FUN_00111d18: the trail (a new point clears every member's warp count), then the follow points.
    if mgr.trail.push(player, c.trail_step2) {
        for (_, mut m) in &mut q {
            m.warps = 0;
        }
    }
    let mut inputs = [None; 2];
    for (_, m) in q.iter() {
        if let Some(i) = inputs.get_mut(m.slot as usize) {
            *i = Some((m.motor.pos, rules::follow_distance(&m.thresholds, m.radius, c)));
        }
    }
    let points = rules::follow_points(player, player_yaw, inputs, c);

    for (mut tf, mut m) in &mut q {
        let m = &mut *m;
        let pos = m.motor.pos;
        let to_player = player - pos;
        let ground_dir = to_player.with_y(0.0).normalize_or_zero();
        let before_mode = m.follow.mode;
        let facing = Vec3::new(m.motor.yaw.sin(), 0.0, m.motor.yaw.cos());
        m.motion_row = battle::motion_state(in_battle, false);
        let order = data.orders[m.motion_row];
        // FUN_001112e8: the timers count down by the frame unit.
        if m.cooldown > 0.0 {
            m.cooldown -= FRAME;
        }
        if m.timer > 0.0 {
            m.timer -= FRAME;
        }
        let mut delta = Vec3::ZERO;
        let mut start: Option<(usize, battle::Entry)> = None;
        m.free = m.attack.is_none();
        if m.free {
            // FUN_00111070: the far counter (a blocked step [inferred] and a height difference over far_dy).
            if m.blocked {
                m.far += 1;
            }
            if to_player.y.abs() > c.far_dy {
                m.far += 1;
            }
            let ai = data.ai.get(&m.class);
            // FUN_0010df08: bit 0x40 when the member is farther from the player than the battle
            // distance (style 0: the class's +0x80); then, in battle, the target (style 0, FUN_0010d0c0)
            // when the re-target test asks for one (FUN_0010cf90 -> FUN_0010cbe0).
            let rel_player = relation(pos, player);
            let too_far = ai.is_some_and(|a| battle::battle_distance(a.params.battle, 0, data.half) < rel_player.gd);
            m.target_changed = false;
            if in_battle {
                let valid = m.target.and_then(|t| shape_of(t, &enemies));
                let retarget = match (valid, ai) {
                    (Some((ct, _)), Some(a)) => battle::retarget_style0(false, (ct - pos).length(), a.params.retarget),
                    _ => true,
                };
                if retarget && let Some(a) = ai {
                    let all = enemies.iter().enumerate().flat_map(|(fi, (_, shapes))| shapes.iter().enumerate().map(move |(si, &(ct, r))| (fi * 256 + si, ct, r)));
                    let new = battle::nearest_target(pos, a.params.range, all).map(|k| Target { entity: enemies[k / 256].0, shape: k % 256 });
                    if new != m.target {
                        m.target_changed = true;
                        if log {
                            match new {
                                Some(t) => info!("party: {} target {:?} shape {} ({:.0} away)", m.model, t.entity, t.shape, shape_of(t, &enemies).map_or(0.0, |(ct, _)| (ct - pos).length())),
                                None => info!("party: {} no target within {:.0}", m.model, a.params.range),
                            }
                        }
                    }
                    m.target = new;
                }
            } else {
                m.target = None;
            }
            let tshape = m.target.and_then(|t| shape_of(t, &enemies));
            let rel_t = tshape.map_or(Rel::default(), |(ct, _)| relation(pos, ct));
            // FUN_0010ca88: a valid target and bit 0x40 clear.
            let target_ok = tshape.is_some() && !too_far;
            let (dir, speed);
            m.want_yaw = None;
            // FUN_001102c8 (the player relation) or FUN_001103e0 (the target), as FUN_001103f8 picks.
            let follow_player = |m: &mut PartyMember, row: usize| {
                let point = points.get(m.slot as usize).copied().flatten().unwrap_or(player);
                let input = FollowIn { to_point: point - pos, to_player: to_player.normalize_or_zero(), dist: to_player.length(), ground_dir, facing, walk: m.walk, run: m.run };
                rules::follow_step_row(&mut m.follow, &input, &m.thresholds, row, c)
            };
            let toward = |m: &mut PartyMember| {
                if target_ok {
                    battle::approach_step(&mut m.follow, rel_t.gdir, rel_t.gd, rel_t.dy, &m.thresholds, m.walk, m.run, data.stop)
                } else {
                    follow_player(m, rules::ROW_BATTLE_PLAYER)
                }
            };
            if !in_battle || ai.is_none_or(|a| a.records.is_empty()) {
                (dir, speed) = follow_player(m, if in_battle { rules::ROW_BATTLE_PLAYER } else { rules::ROW_FIELD });
            } else {
                let ai = ai.unwrap();
                // FUN_0010fcd0 (the emergency pass FUN_0010f8d8 is stage 4: no member is in danger here).
                let now = MemberNow {
                    standing: m.motor.grounded,
                    free: true,
                    status: false,
                    mp: 0,
                    mp_gate: false,
                    mp_charge: false,
                    target_ok,
                    dy: rel_t.dy,
                    gd: rel_t.gd,
                    facing_dot: facing.dot(rel_t.gdir),
                    radius: tshape.map_or(0.0, |s| s.1),
                };
                let member = progress.as_ref().and_then(|p| p.member(m.neo));
                // The setting bytes (party record +0xd4) are zero at a new game (`Progress::init_party`).
                let setting = |_: usize| Some(0u8);
                let equipped = |r: &battle::Record| member.as_ref().is_some_and(|mm| mm.abilities.iter().any(|&(id, on)| on && id as u32 == r.item));
                let skip = battle::repeated(m.choosing.last, m.class);
                let any_attack = |skip: Option<usize>| ai.records.iter().enumerate().any(|(i, r)| r.flags & battle::flag::ATTACK != 0 && Some(i) != skip && !matches!(r.handler, Handler::Other(0)) && r.handler == Handler::Attack && battle::feasible(r, &now, setting(i), equipped(r)));
                let (current, timer, changed) = (m.choosing.current, m.timer, m.target_changed);
                let can = |i: usize, cont: bool| {
                    let r = &ai.records[i];
                    let waited = !(changed || timer <= 0.0);
                    match r.handler {
                        Handler::Attack => battle::feasible(r, &now, setting(i), equipped(r)),
                        Handler::Approach => (if cont { waited } else { current != Some(i) }) && !any_attack(skip),
                        Handler::Wait => (!cont || waited) && !any_attack(skip),
                        Handler::GoofyWait => (!cont || waited) && target_ok,
                        // Items, specials and Cure need the member's status block and the MP gate
                        // (stages 3 and 4): not feasible yet.
                        _ => false,
                    }
                };
                let chain = m.motor.grounded && target_ok;
                let mut choosing = std::mem::take(&mut m.choosing);
                let choice = {
                    let mgr = &mut *mgr;
                    let rng = std::cell::RefCell::new(mgr);
                    battle::choose(&ai.records, &mut choosing, &ai.params, m.class, &now, false, chain, setting, can, || rng.borrow_mut().roll(), |n| rng.borrow_mut().random(n))
                };
                m.choosing = choosing;
                let record = match choice {
                    Choice::Keep(i) | Choice::Do(i) => Some(i),
                    _ => None,
                };
                if let Choice::Do(i) = choice {
                    let r = &ai.records[i];
                    m.chosen.push(format!("{:?}#{i}", r.handler));
                    if log {
                        info!(
                            "party: {} chose record {i} {:?} id {:#x}{} (target {})",
                            m.model,
                            r.handler,
                            r.id,
                            r.entry.map_or(String::new(), |e| format!(" motion {:#x}{}", e.motion, if e.air != 0.0 { " (air)" } else { "" })),
                            if target_ok { format!("{:.0} away, dy {:.0}", rel_t.gd, rel_t.dy) } else { "none".into() }
                        );
                    }
                }
                if let Choice::FollowUp(k) = choice
                    && log
                {
                    info!("party: {} combo follow-up {k} chosen: FUN_0010c800 / FUN_001b0758 are not ported, nothing starts", m.model);
                }
                let continuing = matches!(choice, Choice::Keep(_));
                match (choice, record.map(|i| &ai.records[i])) {
                    (_, Some(r)) => match r.handler {
                        Handler::Attack => {
                            // FUN_001137d8 phase 0: FUN_001103f8, then the command (FUN_0010c8f0).
                            (dir, speed) = toward(m);
                            if let Some(e) = r.entry {
                                start = Some((record.unwrap(), e));
                            }
                        }
                        Handler::Approach => {
                            // FUN_001135e0 phase 0: the wait, the facing toward the target, standing.
                            if !continuing {
                                let w = mgr.between(ai.params.wait_a);
                                m.timer = w;
                            }
                            if target_ok {
                                m.want_yaw = Some(rel_t.gdir.x.atan2(rel_t.gdir.z));
                            }
                            (dir, speed) = (facing, 0.0);
                        }
                        Handler::Wait => {
                            if !continuing {
                                let w = mgr.between(ai.params.wait_b);
                                m.timer = w;
                            }
                            (dir, speed) = toward(m);
                        }
                        Handler::GoofyWait => {
                            // FUN_00114738 phase 0: continuing, the held velocity; else the wait, the move,
                            // and the move's direction held at the run speed.
                            if continuing && let Some(h) = m.held {
                                (dir, speed) = h;
                            } else {
                                let w = mgr.between(ai.params.wait_a);
                                m.timer = w;
                                (dir, speed) = toward(m);
                                m.held = Some((dir, m.run));
                            }
                        }
                        _ => (dir, speed) = toward(m),
                    },
                    (Choice::Stand, _) => {
                        (dir, _) = toward(m);
                        speed = 0.0;
                        if dir != Vec3::ZERO {
                            m.want_yaw = Some(dir.x.atan2(dir.z));
                        }
                    }
                    _ => (dir, speed) = toward(m),
                }
            }
            // FUN_00110468: the steer.
            let (d, sp) = rules::steer(&mut m.follow, dir, speed, m.walk, m.run, c);
            if log && m.follow.mode != before_mode {
                info!("party: {} speed mode {before_mode} -> {} (distance to the player {:.0}, speed {sp:.1})", m.model, m.follow.mode, to_player.length());
            }
            m.speed = sp;
            delta = d.with_y(0.0).normalize_or_zero() * sp;
        }
        // The queued attack command (FUN_001112e8 -> FUN_00174cf8 -> FUN_00110790): the entry's motion
        // through the player-type remap in the battle row; an air entry jumps toward the target
        // (height `air` minus the height difference, held between 0 and the jump height) at the run
        // speed.
        if let Some((rec, e)) = start
            && let Ok((mut a, _)) = anims.get_mut(m.body)
        {
            let rig = a.rig.clone();
            match battle::remap(e.motion, order, |s| rig.mset.has(s)) {
                Some(slot) => {
                    a.restart(Some(slot), false, BLEND);
                    m.attack = Some(Attacking { record: rec, slot });
                    m.free = false;
                    m.script_slot = None;
                    m.speed = 0.0;
                    delta = Vec3::ZERO;
                    if e.air != 0.0 {
                        let to = m.target.and_then(|t| shape_of(t, &enemies)).map_or(Rel::default(), |(ct, _)| relation(m.motor.pos, ct));
                        let h = (e.air - to.dy).clamp(0.0, m.jump);
                        m.motor.start_jump(h);
                        m.held = Some((to.gdir, m.run));
                    } else {
                        m.held = None;
                    }
                    if log {
                        info!("party: {} attack motion {:#x} -> slot {slot}{}", m.model, e.motion, if e.air != 0.0 { " (jump)" } else { "" });
                    }
                }
                None => {
                    if log {
                        info!("party: {} attack motion {:#x} has no slot in the battle row; not started", m.model, e.motion);
                    }
                }
            }
        }
        // A running attack: root motion as Sora's (translation turned by the facing the motion started
        // from), the air entry's run toward the target, and free again once the motion ended on the ground.
        if let Some(at) = m.attack
            && !m.free
            && start.is_none()
            && let Ok((a, _)) = anims.get(m.body)
        {
            let (root_t, root_yaw) = a.root_delta;
            m.motor.yaw = crate::motor::wrap_angle(m.motor.yaw + root_yaw);
            let start_yaw = m.motor.yaw - a.root.yaw;
            delta = (Quat::from_rotation_y(start_yaw) * root_t).with_y(0.0);
            if !m.motor.grounded
                && let Some((d, sp)) = m.held
            {
                delta += d * sp;
            }
            if (a.finished || a.slot != Some(at.slot)) && m.motor.grounded {
                if mgr.log {
                    info!("party: {} attack of record {} ended", m.model, at.record);
                }
                m.attack = None;
                m.held = None;
                m.free = true;
            }
        }
        // Movement: the velocity's direction on the ground at the snapped speed (per 60 Hz frame).
        let airborne = (!m.motor.grounded).then(|| m.motor.jump_dy(game.sstm.fall_max));
        let before = m.motor.pos;
        m.motor.move_by(&w, delta, airborne);
        let moved = (m.motor.pos - before).with_y(0.0).length();
        m.blocked = m.speed > 0.0 && moved < 0.5 * m.speed;
        // Facing: a turning member turns toward the player, which ends the turn (dot above
        // `turn_off`); a wanted yaw (+0x66c) is turned to; otherwise toward its movement. All
        // [inferred]: the writer of the facing (+0x550) in the friend's motion states was not read.
        if m.attack.is_none() {
            if m.follow.turning {
                m.motor.turn_to(ground_dir, m.turn);
            } else if let Some(y) = m.want_yaw {
                m.motor.turn_to(Vec3::new(y.sin(), 0.0, y.cos()), m.turn);
            } else if delta != Vec3::ZERO {
                m.motor.turn_to(delta, m.turn);
            }
        }
        if m.attack.is_none()
            && let Ok((mut a, _)) = anims.get_mut(m.body)
        {
            let rig = a.rig.clone();
            let has = |s: usize| rig.mset.has(s);
            if m.speed <= 0.0
                && let Some(slot) = m.script_slot.filter(|&s| has(s))
            {
                // The script's request (its `idle` mode asks for motion 0) while the member stands.
                a.play(Some(slot), true, BLEND);
            } else {
                let motion = if m.speed <= 0.0 { id::IDLE } else if m.speed < m.run { id::WALK } else { id::RUN };
                let slot = battle::remap(motion as i32, order, has);
                a.play(slot, true, BLEND);
            }
        }
        // FUN_00110eb0: a running warp waits for its fade; else the warp check.
        if m.warping {
            if m.fade.is_none() {
                m.warping = false;
                m.forced = false;
            }
        } else if m.free
            && (mgr.warp_mode == 2 || (mgr.warp_mode == 1 && !in_battle))
            && mgr.trail.head >= 0
            && rules::warp_due(m.cooldown, m.far)
            && ((m.forced && warp_on_screen) || !on_screen(cam, m.motor.pos, m.radius, m.height))
        {
            let r = mgr.random(rules::WARP_RANDOM);
            let me = m.motor.pos;
            let ignore_screen = m.forced && warp_on_screen;
            let ok = |p: Vec3| {
                (ignore_screen || !on_screen(cam, p, m.radius, m.height))
                    && rules::warp_point_ok(p, player, me, ground_dir, c)
                    // [inferred] stand-in for the collision test of FUN_001108c8 (FUN_00143840 at the
                    // point, bit 34 clear): a floor under the point.
                    && w.ground(p, p.y + m.motor.capsule.step, 2.0 * m.motor.capsule.step).is_some()
            };
            let pick = if m.forced {
                // Flag 0x8000: every trail point; the accepted one nearest the player.
                (0..rules::TRAIL_LEN).filter(|&k| ok(mgr.trail.points[k])).min_by(|&a, &b| {
                    let d = |k: usize| (mgr.trail.points[k] - player).length_squared();
                    d(a).total_cmp(&d(b))
                })
            } else {
                rules::warp_order(m.warps, r).find(|&k| ok(mgr.trail.points[k]))
            };
            if let Some(k) = pick {
                let p = mgr.trail.points[k];
                // FUN_00110e28: put there, fade in, far counter 0, one more warp, the cooldown.
                m.motor.pos = p;
                m.motor.grounded = true;
                m.motor.jump = Default::default();
                m.warping = true;
                m.fade = Some(0.0);
                m.alpha = 0.0;
                m.far = 0;
                m.warps += 1;
                m.cooldown = c.warp_cooldown;
                info!(
                    "party: {} warp to trail point {k} at {p:?} (from {me:?}, {:.0} from the player{}{})",
                    m.model,
                    (p - player).length(),
                    if m.forced { ", forced" } else { "" },
                    if ignore_screen { ", on screen by KH2_PARTY_WARP_ONSCREEN" } else { "" }
                );
            }
        }
        if let Some(f) = m.fade.as_mut() {
            *f += 1.0;
            m.alpha = (*f / c.warp_fade).min(1.0);
            if *f >= c.warp_fade {
                m.fade = None;
            }
        }
        if status {
            info!(
                "party: status {} (NeoStatus {}, class {}) slot {} at {:?} distance {:.0} mode {} speed {:.1} far {} cooldown {:.0}{}",
                m.model,
                m.neo,
                m.class,
                m.slot,
                m.motor.pos,
                to_player.length(),
                m.follow.mode,
                m.speed,
                m.far,
                m.cooldown,
                if in_battle { format!(" target {:?} attacking {}", m.target.map(|t| t.entity), m.attack.is_some()) } else { String::new() }
            );
            if in_battle {
                let chosen = std::mem::take(&mut m.chosen);
                info!("party: {} actions in the last second: {} [{}]", m.model, chosen.len(), chosen.join(", "));
            }
        }
        tf.translation = m.motor.pos * SCALE;
        tf.rotation = Quat::from_rotation_y(m.motor.yaw);
    }
}

/// Keeps the members from overlapping each other, Sora and the enemies: a member inside another
/// body's radius moves out along the line between them on the ground (two members share the move).
/// [inferred] The game's object-to-object push (the movement's collision against other objects'
/// cylinders) was not read; the radii are the bodies' shapes.
fn party_push_apart(
    sora: Query<(&Sora, &Transform), Without<PartyMember>>,
    fighters: Query<(&Fighter, &Transform), (Without<PartyMember>, Without<Sora>)>,
    rigs: Query<&Animator>,
    mut q: Query<(&mut Transform, &mut PartyMember)>,
) {
    let mut others: Vec<(Vec3, f32)> = Vec::new();
    if let Ok((s, _)) = sora.single() {
        others.push((s.motor.pos, s.motor.capsule.radius.max(1.0)));
    }
    for (f, t) in &fighters {
        if f.team == Team::Enemy
            && f.hittable
            && let Ok(a) = rigs.get(f.hurt_body)
        {
            others.push((t.translation / SCALE, body_shape(&a.rig).0));
        }
    }
    let members: Vec<(Vec3, f32)> = q.iter().map(|(_, m)| (m.motor.pos, m.radius)).collect();
    for (i, (mut tf, mut m)) in q.iter_mut().enumerate() {
        let mut push = Vec3::ZERO;
        let mut add = |p: Vec3, r: f32, share: f32| {
            let d = (m.motor.pos - p).with_y(0.0);
            let l = d.length();
            let need = m.radius + r;
            if l < need {
                push += if l > 1e-3 { d / l } else { Vec3::X } * (need - l) * share;
            }
        };
        for &(p, r) in &others {
            add(p, r, 1.0);
        }
        for (j, &(p, r)) in members.iter().enumerate() {
            if i != j {
                add(p, r, 0.5);
            }
        }
        if push != Vec3::ZERO {
            m.motor.pos += push;
            tf.translation = m.motor.pos * SCALE;
        }
    }
}

/// `KH2_PARTYLOG`: the members' landed hits (the enemy's own line prints the damage).
fn party_hits(mut hits: MessageReader<Hit>, mgr: Res<Manager>, q: Query<&PartyMember>) {
    for h in hits.read() {
        if let Ok(m) = q.get(h.attacker)
            && mgr.log
        {
            info!("party: {} hit {:?} with atkp {} at {:.2?}", m.model, h.target, h.atkp, h.point);
        }
    }
}

/// The warp's fade-in on the member's materials (blended while it fades, as authored otherwise).
fn party_fade(
    mut q: Query<&mut PartyMember>,
    children: Query<&Children>,
    handles: Query<&MeshMaterial3d<StandardMaterial>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    for mut m in &mut q {
        let alpha = m.alpha.clamp(0.0, 1.0);
        if (alpha - m.applied_alpha).abs() < 1e-4 {
            continue;
        }
        m.applied_alpha = alpha;
        let Ok(kids) = children.get(m.body) else { continue };
        for k in kids.iter() {
            let Ok(h) = handles.get(k) else { continue };
            let Some(mut mat) = materials.get_mut(&h.0) else { continue };
            let original = *m.original_modes.entry(h.0.id()).or_insert_with(|| mat.alpha_mode);
            mat.base_color = mat.base_color.with_alpha(alpha);
            mat.alpha_mode = if alpha < 1.0 { AlphaMode::Blend } else { original };
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kh2_anim::PlayerState;

    /// tt02's map script places FRIEND_1 and FRIEND_2; with preset 1 in Twilight Town (world 2) they
    /// are Donald (`P_EX020`) and Goofy (`P_EX030`), friend-AI classes 0 and 1, with their models and
    /// motion sets on the disc. Needs `KH2_ISO`.
    #[test]
    fn party_tt02_friends_are_donald_and_goofy() {
        let Ok(iso) = crate::assets::iso_path() else { return };
        if !iso.exists() {
            return;
        }
        let disc = Disc::open(&iso);
        let mut data = PartyData::load(&disc);
        let c = data.consts.clone().expect("constants");
        let memt = data.memt.clone().expect("memt");
        let preset = memt.trailing[rules::DEFAULT_PRESET].to_le_bytes();
        let bytes = rules::apply_preset([0, rules::EMPTY, rules::EMPTY, rules::EMPTY], preset);
        let merged = crate::drive::rules::memt_merge(&memt, 2, |_| false);
        let recs = friend_records(&disc, "tt02");
        for r in &recs {
            eprintln!("tt02 record {:#x} group {} at {:?} yaw {:.2} entrance {}", r.0, r.1, r.2, r.3, r.4);
        }
        assert_eq!(recs.iter().filter(|r| r.0 != PLAYER_ENTITY).count(), 2);
        let mut models = Vec::new();
        for (k, byte) in [bytes[1], bytes[2]].into_iter().enumerate() {
            let obj = rules::slot_object(&merged, byte).expect("member");
            let row = data.objs.iter().find(|r| r.object_id == obj as u32).cloned().expect("objentry row");
            let class = rules::ai_class(row.neo_status, row.neo_moveset).expect("class");
            let rig = data.rig(&disc, &row).expect("rig");
            let t = c.thresholds(class);
            eprintln!(
                "FRIEND_{}: {} anim {} NeoStatus {} moveset {} class {class} slots {} idle/walk/run {:?} thresholds {:?} follow distance {}",
                k + 1,
                row.model_name,
                row.animation_name,
                row.neo_status,
                row.neo_moveset,
                rig.mset.slots.len(),
                [id::IDLE, id::WALK, id::RUN].map(|i| rig.mset.slot(i, PlayerState::FieldNoWeapon)),
                t[rules::ROW_FIELD],
                rules::follow_distance(&t, body_shape(&rig).0, &c)
            );
            eprintln!("  objentry WeaponJoint {} bones {}", row.weapon_joint, rig.model.bones.len());
            assert!(rig.mset.slot(id::IDLE, PlayerState::FieldNoWeapon).is_some());
            assert!(rig.mset.slot(id::RUN, PlayerState::FieldNoWeapon).is_some());
            models.push(row.model_name.clone());
        }
        assert_eq!(models, ["P_EX020", "P_EX030"]);
    }

    /// The members' hand objects: the new game's saved weapon word of Donald and Goofy, the `went`
    /// object of their movesets for it and the `wmst` motion set (printed); both resolve to a loadable
    /// weapon. Needs `KH2_ISO`.
    #[test]
    fn party_member_weapons_from_went() {
        let Ok(iso) = crate::assets::iso_path() else { return };
        if !iso.exists() {
            return;
        }
        let disc = Disc::open(&iso);
        let data = PartyData::load(&disc);
        let progress = crate::progress::Progress::start(&disc);
        for (neo, model) in [(2u16, "P_EX020"), (3, "P_EX030")] {
            let row = data.objs.iter().find(|r| r.model_name == model).cloned().expect("row");
            let saved = progress.as_ref().and_then(|p| p.member(neo)).map_or(0, |m| m.weapons[0]);
            // The app's new game leaves the members' weapon words 0 (no hand object, FUN_0016ee98);
            // the base weapon items (Mage's Staff 75, Knight's Shield 49 [inferred from the item ids])
            // are what `KH2_PARTY_WEAPONS` hands in.
            let item = if saved != 0 { saved } else if neo == 2 { 75 } else { 49 };
            let files = data.weapon_files(&row, item, 0);
            let rig = files.as_ref().and_then(|f| crate::drive::weapon_rig(&disc, f));
            eprintln!(
                "{model}: NeoStatus {neo} moveset {} WeaponJoint {} saved weapon word {saved}, item {item} -> {files:?} (bones {:?}, motion slots {:?})",
                row.neo_moveset,
                row.weapon_joint,
                rig.as_ref().map(|r| r.model.bones.len()),
                rig.as_ref().map(|r| r.mset.slots.len())
            );
            let w = |i: usize| data.weapons.went.get(i * 4..i * 4 + 4).map(|b| u32::from_le_bytes(b.try_into().unwrap())).unwrap_or(0);
            let off = w(row.neo_moveset as usize) as usize;
            for k in 0..64 {
                let id = if off == 0 { 0 } else { w(off + k) };
                if id != 0 {
                    let o = data.objs.iter().find(|o| o.object_id == id).map(|o| o.model_name.clone());
                    let items: Vec<u16> = data.weapons.items.iter().filter(|i| (i.flag1 as usize | (i.rank as usize) << 8) == k).map(|i| i.id).collect();
                    eprintln!("  went slot {k}: object {id} {o:?} items {items:?}");
                }
            }
            assert!(rig.is_some(), "{model}: weapon not loadable");
        }
    }

    /// The executable's follow constants are the ones docs/party.md names (printed, and checked for
    /// the shape the rules rely on: two mirrored side offsets, a positive forward part).
    #[test]
    fn party_consts_from_the_executable() {
        let Ok(iso) = crate::assets::iso_path() else { return };
        if !iso.exists() {
            return;
        }
        let disc = Disc::open(&iso);
        let c = PartyData::load(&disc).consts.expect("constants");
        eprintln!("{c:?}");
        assert_eq!(c.classes.len(), rules::CLASSES);
        assert_eq!(c.side[0], -c.side[1]);
        assert!(c.ahead > 0.0 && c.trail_step2 > 0.0 && c.warp_cooldown > 0.0);
        // Every class's field row is a hysteresis: each mode slows down below where it speeds up.
        for t in &c.classes {
            assert!(t[rules::ROW_FIELD].iter().all(|(down, up)| down <= up), "{t:?}");
        }
    }
}
