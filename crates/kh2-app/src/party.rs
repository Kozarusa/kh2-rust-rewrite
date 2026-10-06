//! Party members, stage 1 of docs/party.md section 9: Donald and Goofy (whoever the world's party
//! names) stand where the area's `map` script puts `FRIEND_1` / `FRIEND_2`, with their models and
//! motion sets, and follow the player under the friend AI's follow rules: the follow points, the
//! speed modes, the steer, the player's trail and the warp (catch-up). No combat yet.
//!
//! Who: the world's party slot bytes start from preset 1 (`party_rules::apply_preset`, the writer of
//! the new-game bytes is a gap, docs/party.md section 10); `FRIEND_1` is slot byte 1 and `FRIEND_2`
//! slot byte 2 (FUN_0016e208 -> FUN_0017cd20), resolved through the world's merged `memt` record.
//! The member takes the first free party slot (FUN_0017cd08) and the follow position slot + 1.
//!
//! Each 60 Hz tick (`party_tick`) runs the party manager (FUN_00111d18: the trail, the follow points)
//! and then, per member, the decision loop of a free member out of battle (FUN_00111070: far counter,
//! follow, steer), the movement, and the warp check (FUN_00110eb0). The motion played is the motion
//! id of the speed (0 stand, 1 walk, 2 run) [inferred: the controller's state 0x1f, which picks the
//! member's motion, was not read]; the member turns toward its movement at its `prty` turn speed
//! [inferred]. Battle is not looked at: the members follow in battle too until stage 2.
//!
//! Log: `KH2_PARTYLOG=1` prints speed-mode changes, warps and a status line per second; the spawn
//! lines always print. Harness: `KH2_NO_PARTY=1` spawns nobody; `KH2_PARTY_WARP=<s>` at that time asks
//! every member for a warp the way host 10.? at 0x2eec20 does (FUN_00110e08: far counter 1000, no
//! cooldown, nearest trail point).

use std::collections::HashMap;
use std::sync::Arc;

use bevy::color::Alpha;
use bevy::mesh::skinning::SkinnedMeshInverseBindposes;
use bevy::prelude::*;
use kh2_anim::{id, PlayerState};
use kh2_tables::tables as t;

use crate::assets::{AreaPart, CurrentArea, Disc};
use crate::data::GameData;
use crate::motor::Motor;
use crate::party_rules::{self as rules, Consts, Follow, FollowIn, Thresholds, Trail};
use crate::schedule::Sim;
use crate::skel::{spawn_skinned, Animator, Rig};
use crate::sora::Sora;
use crate::world::World;
use crate::SCALE;

/// Blend of the locomotion motions, as Sora's (`sora::BLEND_LOCOMOTION`).
const BLEND: f32 = 8.0;
/// The frame unit the warp cooldown counts down by (`DAT_00356f40`, one 60 Hz frame).
const FRAME: f32 = 1.0;

pub struct PartyPlugin;

impl Plugin for PartyPlugin {
    fn build(&self, app: &mut App) {
        let data = PartyData::load(app.world().resource::<Disc>());
        app.insert_resource(data)
            .init_resource::<PartySlots>()
            .insert_resource(Manager { log: std::env::var("KH2_PARTYLOG").is_ok_and(|v| !v.is_empty() && v != "0"), rng: 0x2545_f491, ..Default::default() })
            .add_systems(FixedUpdate, (spawn_party, party_tick, party_fade).chain().in_set(PartyTick).in_set(Sim::Enemy));
        crate::friend_ai::register(app);
    }
}

/// The party's own systems (spawn, tick, fade); the members' scripts run after them.
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct PartyTick;

/// The disc and executable data the party needs.
#[derive(Resource)]
pub struct PartyData {
    consts: Option<Consts>,
    objs: Vec<t::Objentry>,
    memt: Option<kh2_tables::player::Memt>,
    prty: Vec<t::PrefPrty>,
    /// Loaded rigs by model name (`None`: not loadable).
    rigs: HashMap<String, Option<Arc<Rig>>>,
}

impl PartyData {
    fn load(disc: &Disc) -> PartyData {
        let consts = disc.read_root("SLUS_210.05").and_then(|exe| Consts::from_exe(&exe));
        if consts.is_none() {
            warn!("party: the friend AI's constants are not readable from SLUS_210.05; nobody follows");
        }
        let (objs, memt, prty) = match disc.0.lock() {
            Ok(mut a) => (
                kh2_tables::load::<t::Objentry>(&mut a, &t::OBJENTRY).map(|d| d.rows).unwrap_or_default(),
                kh2_tables::player::load_memt(&mut a).ok(),
                kh2_tables::load::<t::PrefPrty>(&mut a, &t::PREF_PRTY).map(|d| d.rows).unwrap_or_default(),
            ),
            Err(_) => (Vec::new(), None, Vec::new()),
        };
        PartyData { consts, objs, memt, prty, rigs: HashMap::new() }
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
    /// +0x110 / +0x114 (the `prty` row of the NeoStatus, [inferred]) and the turn rate.
    walk: f32,
    run: f32,
    turn: f32,
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
        let (walk, run, turn) = prty.map_or((0.0, 0.0, 0.0), |p| (p.walk_speed, p.run_speed, p.turn_speed));
        let ground = w.ground(at, at.y + 100.0, 400.0).map_or(at.y, |g| g.y);
        let mut motor = Motor { pos: at.with_y(ground), yaw, grounded: true, ..Motor::default() };
        motor.capsule.radius = radius;
        let slot = next_slot;
        next_slot += 1;
        let root = commands.spawn((AreaPart, Transform::from_translation(motor.pos * SCALE).with_rotation(Quat::from_rotation_y(yaw)), Visibility::default())).id();
        let body = spawn_skinned(&mut commands, root, rig.clone(), &mut meshes, &mut materials, &mut images, &mut bindposes, SCALE);
        info!(
            "party: spawn {} slot {slot} ({} at {group} entrance {entrance}, NeoStatus {}, AI class {class}, walk {walk} run {run}, radius {radius}) at {:?}",
            row.model_name,
            if entity == rules::FRIEND_1 { "FRIEND_1" } else { "FRIEND_2" },
            row.neo_status,
            motor.pos
        );
        commands.entity(root).insert(PartyMember {
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
        });
        if next_slot >= 2 {
            break;
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

/// One tick of the party manager and of every member.
#[allow(clippy::too_many_arguments)]
fn party_tick(
    data: Res<PartyData>,
    game: Res<GameData>,
    mut mgr: ResMut<Manager>,
    world: Option<Res<World>>,
    sora: Query<&Sora>,
    cam: Query<(&Camera, &GlobalTransform), With<Camera3d>>,
    mut q: Query<(&mut Transform, &mut PartyMember)>,
    mut anims: Query<&mut Animator>,
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
        // FUN_00111070: the far counter (a blocked step [inferred] and a height difference over far_dy).
        if m.blocked {
            m.far += 1;
        }
        if to_player.y.abs() > c.far_dy {
            m.far += 1;
        }
        // Follow (FUN_0010e140, state 0x1f) and steer (FUN_00110468).
        let before_mode = m.follow.mode;
        let point = points.get(m.slot as usize).copied().flatten().unwrap_or(player);
        let facing = Vec3::new(m.motor.yaw.sin(), 0.0, m.motor.yaw.cos());
        let input = FollowIn { to_point: point - pos, to_player: to_player.normalize_or_zero(), dist: to_player.length(), ground_dir, facing, walk: m.walk, run: m.run };
        let (dir, speed) = rules::follow_step(&mut m.follow, &input, &m.thresholds, c);
        let (dir, speed) = rules::steer(&mut m.follow, dir, speed, m.walk, m.run, c);
        if log && m.follow.mode != before_mode {
            info!(
                "party: {} speed mode {before_mode} -> {} (distance to the player {:.0}, speed {speed:.1})",
                m.model,
                m.follow.mode,
                to_player.length()
            );
        }
        m.speed = speed;
        // Movement: the velocity's direction on the ground at the snapped speed (per 60 Hz frame).
        let delta = dir.with_y(0.0).normalize_or_zero() * speed;
        let airborne = (!m.motor.grounded).then(|| m.motor.jump_dy(game.sstm.fall_max));
        let before = m.motor.pos;
        m.motor.move_by(&w, delta, airborne);
        let moved = (m.motor.pos - before).with_y(0.0).length();
        m.blocked = speed > 0.0 && moved < 0.5 * speed;
        // Facing: a turning member turns toward the player, which ends the turn (dot above
        // `turn_off`); otherwise toward its movement. Both [inferred]: the writer of the facing
        // (+0x550) during the friend's motion state 0x1f was not read.
        if m.follow.turning {
            m.motor.turn_to(ground_dir, m.turn);
        } else if delta != Vec3::ZERO {
            m.motor.turn_to(delta, m.turn);
        }
        if let Ok(mut a) = anims.get_mut(m.body) {
            let motion = if speed <= 0.0 { id::IDLE } else if speed < m.run { id::WALK } else { id::RUN };
            let rig = a.rig.clone();
            let slot = rig.mset.slot(motion, PlayerState::FieldNoWeapon).or_else(|| rig.mset.has(motion).then_some(motion));
            a.play(slot, true, BLEND);
        }
        // FUN_001112e8: the cooldown counts down by the frame unit.
        if m.cooldown > 0.0 {
            m.cooldown -= FRAME;
        }
        // FUN_00110eb0: a running warp waits for its fade; else the warp check.
        if m.warping {
            if m.fade.is_none() {
                m.warping = false;
                m.forced = false;
            }
        } else if mgr.warp_mode == 2 && mgr.trail.head >= 0 && rules::warp_due(m.cooldown, m.far) && !on_screen(cam, m.motor.pos, m.radius, m.height) {
            let r = mgr.random(rules::WARP_RANDOM);
            let me = m.motor.pos;
            let ok = |p: Vec3| {
                !on_screen(cam, p, m.radius, m.height)
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
                    "party: {} warp to trail point {k} at {p:?} (from {me:?}, {:.0} from the player{})",
                    m.model,
                    (p - player).length(),
                    if m.forced { ", forced" } else { "" }
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
                "party: status {} (NeoStatus {}, class {}) slot {} at {:?} distance {:.0} mode {} speed {:.1} far {} cooldown {:.0}",
                m.model,
                m.neo,
                m.class,
                m.slot,
                m.motor.pos,
                to_player.length(),
                m.follow.mode,
                m.speed,
                m.far,
                m.cooldown
            );
        }
        tf.translation = m.motor.pos * SCALE;
        tf.rotation = Quat::from_rotation_y(m.motor.yaw);
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
