//! Enemies (Heartless Shadow M_EX020 and Soldier M_EX010, the Nobody Dusk M_EX990): models,
//! motions, hitboxes and stats from the disc. What an enemy does runs under the game's own BDX
//! script (`enemy_ai.rs`); this module spawns the waves and holds the hit reactions of the victim
//! side (damage, flinch, push, blow).
//!
//! Waves. In an area whose battle script (`ard/<area>.ard`, entry `btl`) loads spawn groups with
//! enemies the app has, a wave is the next such group: its enemies at the places and with the
//! spawn records the group gives (tt04: the Dusks, `docs/enemies.md`). Which of the script's
//! programs the game runs depends on story progress, which the app does not have, so the groups
//! take turns in script order. Elsewhere (and with `KH2_AREA_WAVES=0`) a wave is the app's own:
//! Shadows around Sora, with a Soldier from the second wave on (not from the disc).
//!
//! Enemy motion sets use the default mapping (slot = motion id): 0 idle, 10/12 small damage
//! front/back, 16/17 blow + land. Attack motions carry attack triggers (atkp + collision group).
//! An enemy without a script (its BDX entry did not load) only stands and reacts to hits.

use std::collections::HashMap;
use std::sync::Arc;

use bevy::mesh::skinning::SkinnedMeshInverseBindposes;
use bevy::prelude::*;

use crate::assets::{CurrentArea, Disc};
use crate::world::World;
use crate::combat::{hit_is_ineffective, Fighter, Hit, IneffectiveHit, ReactionData, ReactionStatus, Team};
use crate::data::{GameData, Stats, GRAVITY, LOGIC_HZ};
use crate::reaction_rules::{self as rules, Blow, Push, VictimAction, VictimCtx};
use crate::skel::{spawn_skinned, Animator, Rig};
use crate::sora::{Rigs, Sora};
use crate::SCALE;

const IDLE: usize = 0;
const HURT_FRONT: usize = 10;
const BLOW: usize = 16;

pub struct Kind {
    pub name: &'static str,
    /// Row in `enmp`.
    pub enmp: u16,
    /// The object's model name (`obj/<model>.mdlx`, `.mset`, `.apdx`; its `00objentry` row).
    pub model: &'static str,
    /// The script's `dead` mode starts element 2 of the object's effect file (the death effect), so
    /// the damage code starts it when HP reaches 0 (`GameEffect::Death`).
    pub death_effect: bool,
}

pub const SHADOW: Kind = Kind { name: "Shadow", enmp: 34, model: "M_EX020", death_effect: true };

pub const SOLDIER: Kind = Kind { name: "Soldier", enmp: 33, model: "M_EX010", death_effect: true };

/// The Dusk. Its `enmp` row is the constant its script hands to `set_obj_param` (host 2.9, the
/// call that builds the status block from the row, FUN_001b2148): 41 (BDX `M_EX990` at 0x073c).
pub const DUSK: Kind = Kind { name: "Dusk", enmp: 41, model: "M_EX990", death_effect: true };

/// The Struggle opponent `N_EX570_BTL` (object 0x678; Seifer by the community's model names), in
/// tt04's battle groups b_41, b_42, b_45 and b_51. Its script picks the `enmp` row it hands to
/// `set_obj_param` from spawn parameter 1 (BDX `N_EX570_BTL` R1864 at 0x077d): 155 for 3 and 4, else
/// 135. The kind starts with 135; host 2.9 rebuilds the stats from the row the script asks for. Its
/// `dead` mode (BDX 0x174d) plays motion 16 or 10 and waits for ever: no death effect, no fade.
pub const SEIFER: Kind = Kind { name: "Seifer", enmp: 135, model: "N_EX570_BTL", death_effect: false };

/// The Nobody `M_EX930` (object 0x138), with two Dusks in tt04's battle group b_21; "Dancer" is
/// inferred from its mode names (`atk_force_dance`, `atk_spin_*`, `atk_kick`). Its script hands
/// `enmp` row 48 to `set_obj_param` (BDX `M_EX930` R0f13 at 0x0f3a, the constant pushed at 0x04ce). Its `dead` mode (R06bd) starts element 2 of its effect file, fades out and hides, like
/// the Dusk's.
pub const DANCER: Kind = Kind { name: "Dancer", enmp: 48, model: "M_EX930", death_effect: true };

/// The Struggle opponent `N_EX500_BTL` (object 0x4d7) of tt04's battle group b_50, called by its model
/// (its name is not in the data the app reads). Its script hands `enmp` row 150 to `set_obj_param`
/// (BDX `N_EX500_BTL` 0x0736). Unlike `N_EX570_BTL` its `dead` mode (R0900) starts element 2 of its
/// effect file, fades out and hides, as the Dusk's does.
pub const N_EX500: Kind = Kind { name: "N_EX500", enmp: 150, model: "N_EX500_BTL", death_effect: true };

/// The Struggle opponent `N_EX600_BTL` (object 0x82f) of tt04's battle group b_52, called by its model.
/// Its script hands `enmp` row 158 to `set_obj_param` (BDX `N_EX600_BTL` 0x07fd); its `dead` mode
/// (R09c8) starts element 2 of its effect file, fades out and hides.
pub const N_EX600: Kind = Kind { name: "N_EX600", enmp: 158, model: "N_EX600_BTL", death_effect: true };

/// Every enemy kind the app has.
pub const KINDS: [&Kind; 7] = [&SHADOW, &SOLDIER, &DUSK, &SEIFER, &DANCER, &N_EX500, &N_EX600];

/// The rigs of the enemy kinds, by `Kind::name` (Shadow and Soldier share `sora::Rigs`' copies).
#[derive(Resource, Default)]
pub struct EnemyRigs(pub HashMap<&'static str, Arc<Rig>>);

impl EnemyRigs {
    pub fn get(&self, kind: &Kind) -> Option<Arc<Rig>> {
        self.0.get(kind.name).cloned()
    }
}

fn load_rig(disc: &Disc, model: &str) -> Option<Arc<Rig>> {
    let m = disc.read(&format!("obj/{model}.mdlx")).and_then(|d| kh2_model::mdlx(&d))?;
    let s = disc.read(&format!("obj/{model}.mset")).and_then(|d| kh2_anim::Mset::parse(&d))?;
    let apdx = Some(format!("obj/{model}.apdx")).filter(|a| disc.read(a).is_some());
    Some(Arc::new(Rig { model: m, mset: s, apdx }))
}

/// Adds the effect files of the enemy rigs that `fx::setup` does not know (it loads Sora's,
/// the keyblade's, the Shadow's and the Soldier's), so motion effects and the death effect of
/// the Dusk find its `obj/M_EX990.apdx`.
fn register_effects(disc: Res<Disc>, rigs: Res<EnemyRigs>, lib: Option<ResMut<crate::fx::EffectLib>>) {
    let Some(mut lib) = lib else { return };
    let common = disc.read("effect/texcommon.dpd").and_then(|b| kh2_fx::Dpd::parse(&b).ok());
    for rig in rigs.0.values() {
        let Some(name) = &rig.apdx else { continue };
        if lib.objects.contains_key(name) {
            continue;
        }
        let id = 2 + lib.objects.len();
        match crate::fx::load(&disc, name, id, common.as_ref()) {
            Some(p) => {
                lib.objects.insert(name.clone(), Arc::new(p));
            }
            None => warn!("{name}: no effects"),
        }
    }
}

/// One enemy of an area's spawn group: the kind, where it stands (metres, the app's space), its
/// facing and the spawn kind byte of its record (host 1.110).
#[derive(Clone)]
pub struct Member {
    pub kind: &'static Kind,
    pub at: Vec3,
    pub yaw: f32,
    pub spawn_kind: i32,
    /// The record's words at +0x20 and +0x24, which the game copies to the object's spawn
    /// parameters 1 and 2 (FUN_0016b870; host 1.39).
    pub spawn_params: [i32; 2],
}

/// A spawn group of the area's battle script that holds enemies the app has.
#[derive(Clone)]
pub struct AreaGroup {
    pub program: u16,
    pub name: String,
    pub members: Vec<Member>,
    /// Enemies (`ObjectType` 4) of the group that the app does not have, by model name.
    pub skipped: Vec<String>,
    /// Where the group puts the player (its entity 0x236, the player's id, FUN_0016b078), metres.
    pub player: Option<Vec3>,
}

/// The current area's enemy spawn groups (see the module header).
#[derive(Resource, Default)]
pub struct AreaWaves {
    area: Option<String>,
    pub groups: Vec<AreaGroup>,
}

/// `ObjectType` of enemies in `00objentry`.
const OBJECT_ENEMY: u8 = 4;
/// The player's entity id in spawn records (`00objentry` row `PLAYER`; FUN_0016b078 finds the player by it).
const PLAYER_ENTITY: u32 = 0x236;

/// The spawn groups that the area's battle script loads and that hold enemies of `KINDS`, in
/// script order. The positions are the game's (centimetres, Y down, Z mirrored: the app's space is
/// the game's turned half a turn about X); a game facing of yaw `r` (direction (sin r, 0, cos r),
/// FUN_0016a7f0) is the app's yaw `PI - r`.
pub fn area_groups(disc: &Disc, area: &str) -> Vec<AreaGroup> {
    use kh2_tables::tables as t;
    let Some(file) = disc.read(&format!("ard/{area}.ard")) else { return Vec::new() };
    let Some((ard, _)) = kh2_ard::Area::parse(&file) else { return Vec::new() };
    let rows = {
        let Ok(mut a) = disc.0.lock() else { return Vec::new() };
        kh2_tables::load::<t::Objentry>(&mut a, &t::OBJENTRY).map(|d| d.rows).unwrap_or_default()
    };
    let mut out: Vec<AreaGroup> = Vec::new();
    for p in ard.scripts.get("btl").into_iter().flatten() {
        for name in p.spawns() {
            if out.iter().any(|g| g.name == name) {
                continue;
            }
            let mut g = AreaGroup { program: p.id, name: name.clone(), members: Vec::new(), skipped: Vec::new(), player: None };
            for e in ard.spawns.get(&name).into_iter().flatten().flat_map(|g| &g.entities) {
                if e.object == PLAYER_ENTITY && g.player.is_none() {
                    let [x, y, z] = e.position;
                    g.player = Some(Vec3::new(x, -y, -z) * SCALE);
                }
                let Some(row) = rows.iter().find(|r| r.object_id == e.object) else { continue };
                match KINDS.iter().find(|k| k.model == row.model_name) {
                    Some(kind) => {
                        let [x, y, z] = e.position;
                        g.members.push(Member {
                            kind,
                            at: Vec3::new(x, -y, -z) * SCALE,
                            yaw: std::f32::consts::PI - e.rotation[1],
                            spawn_kind: e.spawn_kind as i32,
                            spawn_params: [0x20, 0x24].map(|o| i32::from_le_bytes(e.raw[o..o + 4].try_into().unwrap_or_default())),
                        });
                    }
                    None if row.object_type == OBJECT_ENEMY => g.skipped.push(row.model_name.clone()),
                    None => {}
                }
            }
            if !g.members.is_empty() {
                out.push(g);
            }
        }
    }
    out
}

/// What the reaction code tracks: a hit reaction in progress, or nothing (the script is in charge).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum EState {
    Idle,
    Hurt,
    Blown,
}

#[derive(Component)]
pub struct Enemy {
    pub kind: &'static Kind,
    pub name: &'static str,
    pub state: EState,
    pub hp: i32,
    pub stats: Stats,
    pub vel: Vec3,
    pub grounded: bool,
    pub yaw: f32,
    pub timer: f32,
    pub body: Entity,
    /// Decaying push of a push reaction, added to the movement every tick.
    push: Option<Push>,
    /// Flight of the blow that put the enemy in `EState::Blown`, and whether it came from behind.
    blow: Option<Blow>,
    blown_back: bool,
    /// Hit sounds played so far; the sound alternates between two entries (actor byte +0x7fc).
    hit_sounds: u8,
    /// The spawn kind byte of the enemy's spawn record (host 1.110), -1 without one.
    pub spawn_kind: i32,
    /// Spawn parameters 1 and 2 from the record (host 1.39), `None` without a record.
    pub spawn_params: Option<[i32; 2]>,
    /// The lowest HP a hit can leave (word 2 of the HP record of the status block, which the game's
    /// HP change FUN_001a0100 clamps to; scripts set it with host 1.231). 0 unless a script sets it.
    pub min_hp: i32,
}

impl Enemy {
    pub fn alive(&self) -> bool {
        self.hp > 0
    }
}

#[derive(Resource, Default)]
pub struct Waves {
    pub timer: f32,
    pub count: u32,
}

pub struct EnemyPlugin;

impl Plugin for EnemyPlugin {
    fn build(&self, app: &mut App) {
        use crate::schedule::Sim;
        let rigs = {
            let (disc, shared) = (app.world().resource::<Disc>(), app.world().resource::<Rigs>());
            let mut map = HashMap::new();
            map.insert(SHADOW.name, shared.shadow.clone());
            map.insert(SOLDIER.name, shared.soldier.clone());
            for kind in KINDS.iter().filter(|k| ![SHADOW.name, SOLDIER.name].contains(&k.name)) {
                match load_rig(disc, kind.model) {
                    Some(r) => {
                        map.insert(kind.name, r);
                    }
                    None => warn!("{}: obj/{}.mdlx or .mset not loadable; no {} spawns", kind.name, kind.model, kind.name),
                }
            }
            EnemyRigs(map)
        };
        app.insert_resource(Waves::default())
            .insert_resource(rigs)
            .init_resource::<AreaWaves>()
            .add_message::<EnemyClash>()
            .add_systems(Startup, register_effects.after(crate::fx::setup))
            .add_systems(
                FixedUpdate,
                (
                    (spawn_waves, think, separate).chain().in_set(Sim::Enemy),
                    (receive_clashes, receive_hits).chain().in_set(Sim::EnemyHurt),
                ),
            );
    }
}

/// The assets and the area a wave is made from.
type WaveSource<'a> = (Res<'a, Disc>, Res<'a, CurrentArea>, ResMut<'a, AreaWaves>, Res<'a, EnemyRigs>);

/// Spawns one enemy: its model under a new root at `at` (metres), stats from its `enmp` row.
#[allow(clippy::too_many_arguments)]
fn spawn_enemy(
    commands: &mut Commands,
    kind: &'static Kind,
    rig: Arc<Rig>,
    at: Vec3,
    yaw: f32,
    (spawn_kind, spawn_params): (i32, Option<[i32; 2]>),
    data: &GameData,
    assets: (&mut Assets<Mesh>, &mut Assets<StandardMaterial>, &mut Assets<Image>, &mut Assets<SkinnedMeshInverseBindposes>),
) {
    let stats = data.enemy_stats(kind.enmp, 1);
    let root = commands.spawn((Transform::from_translation(at), Visibility::default())).id();
    let (meshes, materials, images, bindposes) = assets;
    let body = spawn_skinned(commands, root, rig, meshes, materials, images, bindposes, SCALE);
    commands.entity(root).insert((
        Enemy {
            kind,
            name: kind.name,
            state: EState::Idle,
            hp: stats.max_hp,
            stats,
            vel: Vec3::ZERO,
            grounded: true,
            yaw,
            timer: 0.0,
            body,
            push: None,
            blow: None,
            blown_back: false,
            hit_sounds: 0,
            spawn_kind,
            spawn_params,
            min_hp: 0,
        },
        Fighter { team: Team::Enemy, attack_bodies: vec![body], hurt_body: body, landed: Default::default(), hittable: false },
    ));
}

fn has(rig: &Rig, s: usize) -> Option<usize> {
    rig.mset.has(s).then_some(s)
}

#[allow(clippy::too_many_arguments)]
pub fn spawn_waves(
    mut commands: Commands,
    time: Res<Time>,
    keys: Res<ButtonInput<KeyCode>>,
    mut waves: ResMut<Waves>,
    (disc, area, mut area_waves, rigs): WaveSource,
    data: Res<GameData>,
    collider: Option<Res<World>>,
    mut sora: Query<(&mut Transform, &mut Sora)>,
    enemies: Query<&Enemy>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut images: ResMut<Assets<Image>>,
    mut bindposes: ResMut<Assets<SkinnedMeshInverseBindposes>>,
) {
    let (Some(col), Ok((mut sora_tf, mut sora))) = (collider, sora.single_mut()) else { return };
    let p = *sora_tf;
    if std::env::var("KH2_NO_ENEMIES").is_ok() {
        return;
    }
    if enemies.iter().any(|e| e.alive()) && !keys.just_pressed(KeyCode::F3) {
        waves.timer = 0.0;
        return;
    }
    waves.timer += time.delta_secs();
    if waves.timer < 4.0 && !keys.just_pressed(KeyCode::F3) {
        return;
    }
    waves.timer = 0.0;
    waves.count += 1;
    // The area's own enemy groups, read once per loaded area.
    if area_waves.area != area.loaded {
        area_waves.area = area.loaded.clone();
        area_waves.groups = area.loaded.as_deref().map(|a| area_groups(&disc, a)).unwrap_or_default();
        for g in &area_waves.groups {
            info!(
                "area waves: {} btl program {:#x} group {}: {} (not in the app: {:?})",
                area.loaded.as_deref().unwrap_or("-"),
                g.program,
                g.name,
                g.members.iter().map(|m| m.kind.name).collect::<Vec<_>>().join(", "),
                g.skipped
            );
        }
    }
    let area_on = !std::env::var("KH2_AREA_WAVES").is_ok_and(|v| v == "0");
    // Harness: `KH2_WAVE_GROUP=<tag>` makes every wave that group and, when the group holds the
    // player's entity, puts Sora there first (the record's place for the player; that the game
    // moves the player there when such a battle starts is [inferred]).
    let forced = std::env::var("KH2_WAVE_GROUP").ok();
    if area_on && !area_waves.groups.is_empty() {
        let k = (waves.count as usize - 1) % area_waves.groups.len();
        let g = forced.as_deref().and_then(|f| area_waves.groups.iter().find(|g| g.name == f)).unwrap_or(&area_waves.groups[k]);
        info!("wave {}: area group {} (btl program {:#x}), {} enemies", waves.count, g.name, g.program, g.members.len());
        // Harness: `KH2_WAVE_AT_GROUP=1` puts Sora at the centre of a forced group that has no player
        // entry (b_00, b_21: the group stands about 22 m from his start point, out of the scripts'
        // battle radius). A test aid, not the game's rule.
        let centre = (forced.is_some() && g.player.is_none() && std::env::var("KH2_WAVE_AT_GROUP").is_ok_and(|v| v == "1") && !g.members.is_empty())
            .then(|| g.members.iter().map(|m| m.at).sum::<Vec3>() / g.members.len() as f32);
        if let (Some(_), Some(at)) = (&forced, g.player.or(centre)) {
            let y = col.ground_m(at.x, at.z, at.y + 1.0).unwrap_or(at.y);
            crate::sora::respawn(&mut sora, at.with_y(y) / SCALE);
            sora_tf.translation = at.with_y(y);
            info!("wave {}: Sora placed at the group's player entry {:?}", waves.count, sora_tf.translation);
        }
        for m in &g.members {
            let Some(rig) = rigs.get(m.kind) else { continue };
            // On the floor under the record's point (the record's height is the game's own floor).
            let y = col.ground_m(m.at.x, m.at.z, m.at.y + 1.0).unwrap_or(m.at.y);
            let at = m.at.with_y(y);
            spawn_enemy(&mut commands, m.kind, rig, at, m.yaw, (m.spawn_kind, Some(m.spawn_params)), &data, (&mut meshes, &mut materials, &mut images, &mut bindposes));
        }
        return;
    }
    let n = 3 + (waves.count % 3) as usize;
    // Candidate points around Sora with floor near his height and a clear path to him.
    let knee = Vec3::Y * 0.5;
    let spots: Vec<Vec3> = (0..24)
        .flat_map(|k| [4.5f32, 6.5].map(move |r| (k, r)))
        .filter_map(|(k, r)| {
            let ang = k as f32 / 24.0 * std::f32::consts::TAU + waves.count as f32 * 0.7;
            let (x, z) = (p.translation.x + ang.cos() * r, p.translation.z + ang.sin() * r);
            let y = col.ground_m(x, z, p.translation.y + 1.5)?;
            let at = Vec3::new(x, y, z);
            ((y - p.translation.y).abs() < 1.5 && !col.blocked_m(p.translation + knee, at + knee)).then_some(at)
        })
        .collect();
    let step = (spots.len() / n.max(1)).max(1);
    for (i, at) in spots.iter().step_by(step).take(n).enumerate() {
        let (x, y, z, i) = (at.x, at.y, at.z, i as u32);
        // Mostly Shadows, with a Soldier in every wave after the first.
        let kind: &'static Kind = if waves.count > 1 && i == 0 { &SOLDIER } else { &SHADOW };
        let Some(rig) = rigs.get(kind) else { continue };
        let yaw = { let d = p.translation - Vec3::new(x, y, z); f32::atan2(d.x, d.z) };
        spawn_enemy(&mut commands, kind, rig, Vec3::new(x, y, z), yaw, (-1, None), &data, (&mut meshes, &mut materials, &mut images, &mut bindposes));
    }
}

/// The hit reactions of the enemies (`Hurt`, `Blown`) and the movement they need. A scripted enemy
/// (`AiBrain`) moves and animates itself in `enemy_ai.rs` whenever no reaction plays.
#[allow(clippy::too_many_arguments)]
pub fn think(
    time: Res<Time>,
    data: Res<GameData>,
    collider: Option<Res<World>>,
    reaction: Option<Res<ReactionData>>,
    sora: Query<&Transform, (With<Sora>, Without<Enemy>)>,
    mut q: Query<(&mut Transform, &mut Enemy, Has<crate::enemy_ai::AiBrain>)>,
    mut anims: Query<&mut Animator>,
) {
    let (Some(col), Ok(_)) = (collider, sora.single()) else { return };
    let dt = time.delta_secs().min(0.05);
    for (mut tf, mut e, scripted) in &mut q {
        let e = &mut *e;
        if scripted && !matches!(e.state, EState::Hurt | EState::Blown) {
            continue;
        }
        let Ok(mut a) = anims.get_mut(e.body) else { continue };
        let rig = a.rig.clone();
        e.timer += dt;
        let mut gravity = GRAVITY;
        match e.state {
            EState::Idle => {
                e.vel = Vec3::new(0.0, e.vel.y, 0.0);
                a.play(has(&rig, IDLE), true, 0.2);
            }
            EState::Hurt => {
                e.vel *= Vec3::new(0.85, 1.0, 0.85);
                if a.finished || e.timer > 1.2 {
                    e.state = EState::Idle;
                    e.timer = 0.0;
                }
            }
            EState::Blown => {
                // The flight of a blow (FUN_0019e4d8) is in units per tick; the mover takes m/s and
                // gets the vertical speed directly, so it adds no gravity of its own.
                if let (Some(b), Some(r)) = (e.blow.as_mut(), reaction.as_ref()) {
                    let (h, vy) = b.step(&r.defaults, &r.consts);
                    let k = SCALE * LOGIC_HZ;
                    e.vel = (b.dir * h * k).with_y(vy * k);
                    gravity = 0.0;
                }
                if e.grounded && e.timer > 0.25 {
                    info!("{} lands from a blow after {:.2} s at {:?}", e.name, e.timer, tf.translation);
                    e.vel = Vec3::ZERO;
                    e.blow = None;
                    e.state = EState::Hurt;
                    e.timer = 0.0;
                    a.restart(has(&rig, BLOW + if e.blown_back { 3 } else { 1 }).or(has(&rig, BLOW + 1)), false, 0.05);
                }
            }
        }
        // A push (hit reaction) moves the enemy on top of whatever else it does this tick.
        let mut push_v = Vec3::ZERO;
        if let (Some(p), Some(r)) = (e.push.as_mut(), reaction.as_ref()) {
            push_v = p.step(&r.consts) * (SCALE * LOGIC_HZ);
            if p.finished() {
                e.push = None;
            }
        }
        e.vel += push_v;
        let mut g = e.grounded;
        col.step_m(&mut tf.translation, &mut e.vel, dt, gravity, data.fall_max(), &mut g);
        e.grounded = g;
        e.vel -= push_v;
        tf.rotation = Quat::from_rotation_y(e.yaw);
    }
}

/// Keeps bodies from overlapping: enemies push apart from each other and from Sora.
pub fn separate(sora: Query<&Transform, (With<Sora>, Without<Enemy>)>, mut q: Query<(&mut Transform, &Enemy)>) {
    const ENEMY_R: f32 = 0.45;
    const SORA_R: f32 = 0.4;
    let sp = sora.single().map(|t| t.translation).ok();
    let pos: Vec<Vec3> = q.iter().map(|(t, _)| t.translation).collect();
    for (i, (mut t, e)) in q.iter_mut().enumerate() {
        if !e.alive() {
            continue;
        }
        let mut push = Vec3::ZERO;
        for (j, o) in pos.iter().enumerate() {
            let d = (t.translation - *o).with_y(0.0);
            let l = d.length();
            if i != j && l < 2.0 * ENEMY_R {
                push += if l > 1e-4 { d / l } else { Vec3::X } * (2.0 * ENEMY_R - l) * 0.5;
            }
        }
        if let Some(sp) = sp {
            let d = (t.translation - sp).with_y(0.0);
            let l = d.length();
            if l < ENEMY_R + SORA_R {
                push += if l > 1e-4 { d / l } else { Vec3::Z } * (ENEMY_R + SORA_R - l);
            }
        }
        t.translation += push;
    }
}

/// Sora's hits on enemies: damage, then the victim reaction of the attack row
/// (`reaction_rules::victim_outcome`): nothing, a push, a flinch, a flinch with a push, or a blow.
#[allow(clippy::too_many_arguments)]
pub fn receive_hits(
    mut hits: MessageReader<Hit>,
    data: Res<GameData>,
    reaction: Option<Res<ReactionData>>,
    sora: Query<&Sora>,
    mut q: Query<(&mut Enemy, &mut Fighter, Option<&ReactionStatus>)>,
    mut anims: Query<&mut Animator>,
    mut popups: MessageWriter<crate::hud::DamagePopup>,
    mut sounds: MessageWriter<crate::audio::PlaySe>,
    mut ineffective: MessageWriter<IneffectiveHit>,
    mut ai_hits: MessageWriter<crate::enemy_ai::AiHit>,
    mut game: MessageWriter<crate::fx::GameEffect>,
    drive: (Option<Res<crate::drive::DriveRules>>, Option<Res<crate::drive::DriveState>>),
    abilities: Option<Res<crate::abilities::Abilities>>,
) {
    let from = sora.single().map(|s| (s.stats, s.hits)).ok();
    let Some(reaction) = reaction else { return };
    for h in hits.read() {
        let (Ok((mut e, mut f, status)), Some((from, combo_hits))) = (q.get_mut(h.target), from) else { continue };
        if !e.alive() {
            continue;
        }
        let Some(atk) = data.hit_row(h) else { continue };
        let useless = hit_is_ineffective(atk, &e.stats);
        let mut dmg = data.damage(atk, &from, &e.stats);
        // Anti form multiplies the damage Sora deals (FUN_001a0610), then the attacker's ability
        // block adds its element and combo/reaction boosts (FUN_001a0a18, FUN_001a0aa8).
        if let (Some(rules), Some(state)) = (&drive.0, &drive.1) {
            dmg = crate::drive::rules::outgoing_damage(&rules.tuning, state.form, atk.r#type, dmg);
        }
        if let Some(ab) = &abilities {
            dmg = crate::abilities::attacker_bonuses(ab.block(), combo_hits, atk, dmg);
        }
        // FUN_001a0100: the new HP is held between word 2 (`min_hp`) and the maximum.
        e.hp = (e.hp - dmg).max(e.min_hp);
        if e.hp == 0 && e.kind.death_effect {
            // The damage method starts the death at the moment HP reaches 0 (FUN_00186530 -> the
            // `dead` mode); a second request for the same victim is ignored by `fx`.
            game.write(crate::fx::GameEffect::Death { victim: h.target });
        }
        popups.write(crate::hud::DamagePopup { at: h.point, amount: dmg });
        if useless {
            ineffective.write(IneffectiveHit { attacker: h.attacker, target: h.target, atkp: h.atkp, point: h.point });
        }
        let Ok(mut a) = anims.get_mut(e.body) else { continue };
        let rig = a.rig.clone();
        // The victim's hit sound (FUN_001b2378): not for ineffective hits; the sound alternates
        // between two entries per victim (the actor byte at +0x7fc).
        if !useless && let Some(se) = crate::audio::PlaySe::hit(atk.enemy_reaction, e.hit_sounds, rig.apdx.clone()) {
            sounds.write(se);
            e.hit_sounds = e.hit_sounds.wrapping_add(1);
        }
        let facing = Vec3::new(e.yaw.sin(), 0.0, e.yaw.cos());
        let from_front = facing.dot(h.dir) < 0.0;
        let ctx = VictimCtx {
            status: status.map_or(0, |x| x.0),
            grounded: e.grounded,
            attacker_flag_4000: false,
            from_front,
            hit_part_plain: true,
        };
        let out = rules::victim_outcome(&rules::HitRow::from_atkp(atk), useless, &ctx, &reaction.patn, &reaction.defaults, &reaction.consts);
        ai_hits.write(crate::enemy_ai::AiHit { target: h.target, real: rules::reaction_class(out.effective) >= 2, reaction: out.effective });
        // Dying is not part of the reaction rules: the last hit always launches.
        let d = &reaction.defaults;
        let action = match out.action {
            x @ VictimAction::Blow { .. } => x,
            // An ineffective hit has reaction 0 (flag 4): no flinch, push or blow.
            _ if e.hp == 0 && !useless => VictimAction::Blow { motion: rules::blow_motion(from_front), up: d.blow_up, horiz: d.blow_speed, face_attacker: from_front },
            x => x,
        };
        info!(
            "{} hit: atkp {} row SubId {} damage {dmg} hp {} reaction {} effective {} ineffective {useless} {} -> {action:?}",
            e.name,
            h.atkp,
            atk.sub_id,
            e.hp,
            atk.enemy_reaction,
            out.effective,
            if from_front { "front" } else { "back" }
        );
        let away = h.dir.with_y(0.0).normalize_or_zero();
        let push = |strength: f32| Push::new(away, strength, 0.0, &reaction.consts);
        let first = |ids: &[usize]| ids.iter().find_map(|&i| has(&rig, i));
        match action {
            VictimAction::None | VictimAction::NotDecoded { .. } => {}
            VictimAction::Push { strength } => e.push = push(strength),
            VictimAction::Flinch { motion, fallback } | VictimAction::FlinchPush { motion, fallback, .. } => {
                if let VictimAction::FlinchPush { strength, .. } = action {
                    e.push = push(strength);
                }
                e.vel = Vec3::new(0.0, e.vel.y, 0.0);
                e.timer = 0.0;
                e.state = EState::Hurt;
                a.restart(first(&[motion as usize, fallback as usize]), false, 0.03);
            }
            VictimAction::Class14 { motion, strength, face_attacker } => {
                let to = if face_attacker { -away } else { away };
                e.yaw = to.x.atan2(to.z);
                e.push = push(strength);
                e.vel = Vec3::new(0.0, e.vel.y, 0.0);
                e.timer = 0.0;
                e.state = EState::Hurt;
                a.restart(first(&[motion as usize, HURT_FRONT]), false, 0.03);
            }
            VictimAction::Blow { motion, up, horiz, face_attacker } => {
                // Faces the attacker when hit from the front, launched along the hit direction.
                let to = if face_attacker { -away } else { away };
                e.yaw = to.x.atan2(to.z);
                e.blow = Some(Blow::new(away, up, horiz, d));
                e.push = None;
                e.blown_back = !from_front;
                e.vel = Vec3::ZERO;
                e.grounded = false;
                e.timer = 0.0;
                e.state = EState::Blown;
                a.restart(first(&[motion as usize, BLOW, HURT_FRONT]), false, 0.03);
            }
        }
        if !matches!(action, VictimAction::None | VictimAction::NotDecoded { .. } | VictimAction::Push { .. }) {
            f.landed.clear();
        }
    }
}

/// An enemy attack volume met a player attack volume (`clash.rs`) and the enemy-class handler of the
/// game (0x1b2478) reacts: the row's `ReflectedMotion` with a recoil push (FUN_00185d30, power
/// `ReflectHitBack`, negative for the default). `ReflectAction` (a script action id) is not carried:
/// the table behind FUN_0019e870 is not decoded. Written for every clash of an enemy volume, also when
/// the handler's own part does nothing (`motion` `None`): the script part of 0x1b2478 (clash hook and
/// event 0x13, `enemy_ai.rs`) runs before its flag 0x40 / `RefactSelf` 2 tests.
#[derive(Message, Clone, Copy, Debug)]
pub struct EnemyClash {
    pub enemy: Entity,
    pub motion: Option<u8>,
    pub power: f32,
    /// The attack rows of the enemy's own volume and of the other one.
    pub own_atkp: i16,
    pub other_atkp: i16,
}

/// Plays the clash recoil: the motion, then a push backwards along the enemy's own facing
/// (FUN_001857b0). A motion the enemy's set does not have is ignored.
pub fn receive_clashes(
    mut clashes: MessageReader<EnemyClash>,
    reaction: Option<Res<ReactionData>>,
    mut q: Query<&mut Enemy>,
    mut anims: Query<&mut Animator>,
) {
    let Some(reaction) = reaction else { return };
    for c in clashes.read() {
        let (Ok(mut e), Some(m)) = (q.get_mut(c.enemy), c.motion) else { continue };
        if !e.alive() {
            continue;
        }
        let Ok(mut a) = anims.get_mut(e.body) else { continue };
        let rig = a.rig.clone();
        let Some(slot) = has(&rig, m as usize) else { continue };
        let facing = Vec3::new(e.yaw.sin(), 0.0, e.yaw.cos());
        e.push = Push::new(-facing, rules::recoil_power(c.power, &reaction.consts), 0.0, &reaction.consts);
        e.vel = Vec3::new(0.0, e.vel.y, 0.0);
        e.timer = 0.0;
        e.state = EState::Hurt;
        a.restart(Some(slot), false, 0.0);
        info!("{} clash: recoil motion {m}", e.name);
    }
}
