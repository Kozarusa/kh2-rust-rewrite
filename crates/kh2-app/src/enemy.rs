//! Heartless (Shadow M_EX020, Soldier M_EX010): models, motions, hitboxes and stats from the
//! disc. What an enemy does runs under the game's own BDX script (`enemy_ai.rs`); this module
//! spawns the waves and holds the hit reactions of the victim side (damage, flinch, push, blow).
//!
//! Enemy motion sets use the default mapping (slot = motion id): 0 idle, 10/12 small damage
//! front/back, 16/17 blow + land. Attack motions carry attack triggers (atkp + collision group).
//! An enemy without a script (its BDX entry did not load) only stands and reacts to hits.

use std::sync::Arc;

use bevy::mesh::skinning::SkinnedMeshInverseBindposes;
use bevy::prelude::*;

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
}

pub const SHADOW: Kind = Kind { name: "Shadow", enmp: 34 };

pub const SOLDIER: Kind = Kind { name: "Soldier", enmp: 33 };

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
        app.insert_resource(Waves::default()).add_message::<EnemyClash>().add_systems(
            FixedUpdate,
            (
                (spawn_waves, think, separate).chain().in_set(Sim::Enemy),
                (receive_clashes, receive_hits).chain().in_set(Sim::EnemyHurt),
            ),
        );
    }
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
    rigs: Res<Rigs>,
    data: Res<GameData>,
    collider: Option<Res<World>>,
    sora: Query<&Transform, With<Sora>>,
    enemies: Query<&Enemy>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut images: ResMut<Assets<Image>>,
    mut bindposes: ResMut<Assets<SkinnedMeshInverseBindposes>>,
) {
    let (Some(col), Ok(p)) = (collider, sora.single()) else { return };
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
        let (kind, rig): (&'static Kind, Arc<Rig>) =
            if waves.count > 1 && i == 0 { (&SOLDIER, rigs.soldier.clone()) } else { (&SHADOW, rigs.shadow.clone()) };
        let stats = data.enemy_stats(kind.enmp, 1);
        let root = commands.spawn((Transform::from_xyz(x, y, z), Visibility::default())).id();
        let body = spawn_skinned(&mut commands, root, rig, &mut meshes, &mut materials, &mut images, &mut bindposes, SCALE);
        commands.entity(root).insert((
            Enemy {
                kind,
                name: kind.name,
                state: EState::Idle,
                hp: stats.max_hp,
                stats,
                vel: Vec3::ZERO,
                grounded: true,
                yaw: { let d = p.translation - Vec3::new(x, y, z); f32::atan2(d.x, d.z) },
                timer: 0.0,
                body,
                push: None,
                blow: None,
                blown_back: false,
                hit_sounds: 0,
            },
            Fighter { team: Team::Enemy, attack_bodies: vec![body], hurt_body: body, landed: Default::default(), hittable: false },
        ));
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
) {
    let from = sora.single().map(|s| s.stats).ok();
    let Some(reaction) = reaction else { return };
    for h in hits.read() {
        let (Ok((mut e, mut f, status)), Some(from)) = (q.get_mut(h.target), from) else { continue };
        if !e.alive() {
            continue;
        }
        let Some(atk) = data.atkp(h.atkp) else { continue };
        let useless = hit_is_ineffective(atk, &e.stats);
        let mut dmg = data.damage(atk, &from, &e.stats);
        // Anti form multiplies the damage Sora deals (FUN_001a0610).
        if let (Some(rules), Some(state)) = (&drive.0, &drive.1) {
            dmg = crate::drive::rules::outgoing_damage(&rules.tuning, state.form, atk.r#type, dmg);
        }
        e.hp = (e.hp - dmg).max(0);
        if e.hp == 0 {
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
        ai_hits.write(crate::enemy_ai::AiHit { target: h.target, real: rules::reaction_class(out.effective) >= 2 });
        // Dying is not part of the reaction rules: the last hit always launches.
        let d = &reaction.defaults;
        let action = match out.action {
            x @ VictimAction::Blow { .. } => x,
            // An ineffective hit has reaction 0 (flag 4): no flinch, push or blow.
            _ if e.hp == 0 && !useless => VictimAction::Blow { motion: rules::blow_motion(from_front), up: d.blow_up, horiz: d.blow_speed, face_attacker: from_front },
            x => x,
        };
        info!(
            "{} hit: atkp {} damage {dmg} hp {} reaction {} effective {} ineffective {useless} {} -> {action:?}",
            e.name,
            h.atkp,
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
/// the table behind FUN_0019e870 is not decoded.
#[derive(Message, Clone, Copy, Debug)]
pub struct EnemyClash {
    pub enemy: Entity,
    pub motion: Option<u8>,
    pub power: f32,
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
