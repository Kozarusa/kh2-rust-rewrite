//! Heartless (Shadow M_EX020, Soldier M_EX010): models, motions, hitboxes and stats from the
//! disc; behaviour is a compact state machine standing in for the game's BDX AI scripts (not
//! parsed yet).
//!
//! Enemy motion sets use the default mapping (slot = motion id): 0 idle, 1 walk,
//! 10/12 small damage front/back, 16/17 blow + land; appear, sink/rise and attacks are found by
//! tag. Attack motions carry attack triggers (atkp + collision group).

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
const WALK: usize = 1;
const HURT_FRONT: usize = 10;
const BLOW: usize = 16;
const SUNK_SPEED: f32 = 5.5;

pub struct Kind {
    pub name: &'static str,
    /// Row in `enmp`.
    pub enmp: u16,
    pub appear: &'static str,
    pub attacks: &'static [&'static str],
    /// (sink, resurface) motion tags, for enemies that travel underground.
    pub sink: Option<(&'static str, &'static str)>,
    pub walk_speed: f32,
    pub reach: f32,
}

pub const SHADOW: Kind = Kind {
    name: "Shadow",
    enmp: 34,
    appear: "81A",
    attacks: &["52A", "50A"],
    sink: Some(("10A", "11A")),
    walk_speed: 3.0,
    reach: 1.7,
};

pub const SOLDIER: Kind = Kind { name: "Soldier", enmp: 33, appear: "80A", attacks: &["52A", "53A"], sink: None, walk_speed: 2.4, reach: 2.0 };

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum EState {
    Appear,
    Idle,
    Chase,
    Sink,
    Sunk,
    Rise,
    Attack,
    Hurt,
    Blown,
    Dying,
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
    pub cooldown: f32,
    pub body: Entity,
    /// Decaying push of a push reaction, added to the movement every tick.
    push: Option<Push>,
    /// Flight of the blow that put the enemy in `EState::Blown`, and whether it came from behind.
    blow: Option<Blow>,
    blown_back: bool,
    /// Hit sounds played so far; the sound alternates between two entries (actor byte +0x7fc).
    hit_sounds: u8,
    seed: u32,
}

impl Enemy {
    pub fn alive(&self) -> bool {
        !matches!(self.state, EState::Dying) && self.hp > 0
    }
    fn rand(&mut self) -> f32 {
        self.seed ^= self.seed << 13;
        self.seed ^= self.seed >> 17;
        self.seed ^= self.seed << 5;
        (self.seed % 10_000) as f32 / 10_000.0
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
        app.insert_resource(Waves::default()).add_systems(
            FixedUpdate,
            (
                (spawn_waves, think, separate).chain().in_set(Sim::Enemy),
                receive_hits.in_set(Sim::EnemyHurt),
            ),
        );
    }
}

fn slot(rig: &Rig, tag: &str) -> Option<usize> {
    rig.mset.by_tag(tag).next()
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
                state: EState::Appear,
                hp: stats.max_hp,
                stats,
                vel: Vec3::ZERO,
                grounded: true,
                yaw: { let d = p.translation - Vec3::new(x, y, z); f32::atan2(d.x, d.z) },
                timer: 0.0,
                cooldown: 1.0,
                body,
                push: None,
                blow: None,
                blown_back: false,
                hit_sounds: 0,
                seed: 0x9E37_79B9 ^ (waves.count * 31 + i),
            },
            Fighter { team: Team::Enemy, attack_bodies: vec![body], hurt_body: body, landed: Default::default(), hittable: false },
        ));
    }
}

#[allow(clippy::too_many_arguments)]
pub fn think(
    mut commands: Commands,
    time: Res<Time>,
    data: Res<GameData>,
    collider: Option<Res<World>>,
    reaction: Option<Res<ReactionData>>,
    sora: Query<&Transform, (With<Sora>, Without<Enemy>)>,
    mut q: Query<(Entity, &mut Transform, &mut Enemy, &mut Fighter, Has<crate::enemy_ai::AiBrain>)>,
    mut anims: Query<&mut Animator>,
    mut fx: MessageWriter<crate::fx::StartEffect>,
) {
    let (Some(col), Ok(sp)) = (collider, sora.single()) else { return };
    let dt = time.delta_secs().min(0.05);
    for (ent, mut tf, mut e, mut f, scripted) in &mut q {
        let e = &mut *e;
        // A BDX-driven enemy (enemy_ai.rs) moves and animates itself; only its hit reactions run here.
        if scripted && !matches!(e.state, EState::Hurt | EState::Blown) {
            continue;
        }
        let Ok(mut a) = anims.get_mut(e.body) else { continue };
        let rig = a.rig.clone();
        let kind = e.kind;
        e.timer += dt;
        e.cooldown -= dt;
        let to = (sp.translation - tf.translation).with_y(0.0);
        let dist = to.length();
        let face = |e: &mut Enemy, rate: f32| {
            if dist > 0.01 {
                let target = f32::atan2(to.x, to.z);
                let d = (target - e.yaw + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU) - std::f32::consts::PI;
                e.yaw += d.clamp(-rate * dt, rate * dt);
            }
        };
        let mut gravity = GRAVITY;
        f.hittable = !matches!(e.state, EState::Sunk | EState::Dying | EState::Appear);
        match e.state {
            EState::Appear => {
                a.play(slot(&rig, kind.appear), false, 0.0);
                if a.finished || e.timer > 2.5 {
                    e.state = EState::Idle;
                    e.timer = 0.0;
                }
            }
            EState::Idle => {
                e.vel = Vec3::new(0.0, e.vel.y, 0.0);
                a.play(has(&rig, IDLE), true, 0.2);
                face(e, 4.0);
                if e.timer > 0.4 + e.rand() * 0.6 {
                    e.timer = 0.0;
                    e.state = if dist < kind.reach + 0.2 && e.cooldown <= 0.0 {
                        EState::Attack
                    } else if kind.sink.is_some() && dist > 5.0 && e.rand() < 0.5 {
                        // Shadow effect 2: the pool of darkness it sinks into.
                        fx.write(crate::fx::StartEffect { owner: e.body, id: 2 });
                        EState::Sink
                    } else {
                        EState::Chase
                    };
                    if e.state == EState::Attack {
                        let pick = (e.rand() * kind.attacks.len() as f32) as usize % kind.attacks.len();
                        a.restart(slot(&rig, kind.attacks[pick]), false, 0.1);
                    }
                }
            }
            EState::Chase => {
                face(e, 5.0);
                let fwd = Vec3::new(e.yaw.sin(), 0.0, e.yaw.cos());
                e.vel = (fwd * kind.walk_speed).with_y(e.vel.y);
                a.play(has(&rig, WALK), true, 0.2);
                if dist < kind.reach || e.timer > 3.0 {
                    e.state = EState::Idle;
                    e.timer = 0.0;
                }
            }
            EState::Sink => {
                e.vel = Vec3::new(0.0, e.vel.y, 0.0);
                a.play(kind.sink.and_then(|s| slot(&rig, s.0)), false, 0.1);
                if a.finished || e.timer > 1.5 {
                    e.state = EState::Sunk;
                    e.timer = 0.0;
                }
            }
            EState::Sunk => {
                face(e, 8.0);
                let fwd = Vec3::new(e.yaw.sin(), 0.0, e.yaw.cos());
                e.vel = (fwd * SUNK_SPEED).with_y(e.vel.y);
                if dist < 2.0 || e.timer > 3.0 {
                    e.state = EState::Rise;
                    e.timer = 0.0;
                    fx.write(crate::fx::StartEffect { owner: e.body, id: 2 });
                    a.restart(kind.sink.and_then(|s| slot(&rig, s.1)), false, 0.05);
                }
            }
            EState::Rise => {
                e.vel = Vec3::new(0.0, e.vel.y, 0.0);
                if a.finished || e.timer > 1.5 {
                    e.state = EState::Attack;
                    e.timer = 0.0;
                    face(e, 100.0);
                    a.restart(slot(&rig, kind.attacks[0]), false, 0.05);
                }
            }
            EState::Attack => {
                // Lunge forward while the claw swipe's hitbox window is open.
                let fwd = Vec3::new(e.yaw.sin(), 0.0, e.yaw.cos());
                let live = a.trigger(kh2_anim::triggers::range::ATTACK);
                e.vel = (fwd * if live && dist > 0.8 { 3.0 } else { 0.0 }).with_y(e.vel.y);
                if a.finished || e.timer > 4.0 {
                    e.state = EState::Idle;
                    e.timer = 0.0;
                    e.cooldown = 1.2 + e.rand() * 1.5;
                }
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
                    e.state = if e.hp <= 0 { EState::Dying } else { EState::Hurt };
                    e.timer = 0.0;
                    a.restart(has(&rig, BLOW + if e.blown_back { 3 } else { 1 }).or(has(&rig, BLOW + 1)), false, 0.05);
                }
            }
            EState::Dying => {
                e.vel = Vec3::new(0.0, e.vel.y, 0.0);
                // Heartless dissolve: sink into the ground, then despawn.
                if e.timer > 0.6 {
                    tf.translation.y -= dt * 1.5;
                    gravity = 0.0;
                    e.vel.y = 0.0;
                }
                if e.timer > 1.6 {
                    commands.entity(ent).despawn();
                    continue;
                }
            }
        }
        if !matches!(e.state, EState::Dying) || e.timer <= 0.6 {
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
        }
        tf.rotation = Quat::from_rotation_y(e.yaw);
        // Sunk Shadows are only a puddle: hide the body.
        let sunk = e.state == EState::Sunk;
        if let Ok(mut ec) = commands.get_entity(e.body) {
            ec.insert(if sunk { Visibility::Hidden } else { Visibility::Inherited });
        }
    }
}

/// Keeps bodies from overlapping: enemies push apart from each other and from Sora.
pub fn separate(sora: Query<&Transform, (With<Sora>, Without<Enemy>)>, mut q: Query<(&mut Transform, &Enemy)>) {
    const ENEMY_R: f32 = 0.45;
    const SORA_R: f32 = 0.4;
    let sp = sora.single().map(|t| t.translation).ok();
    let pos: Vec<Vec3> = q.iter().map(|(t, _)| t.translation).collect();
    for (i, (mut t, e)) in q.iter_mut().enumerate() {
        if !e.alive() || e.state == EState::Sunk {
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
        let dmg = data.damage(atk, &from, &e.stats);
        e.hp = (e.hp - dmg).max(0);
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
            _ if e.hp == 0 => VictimAction::Blow { motion: rules::blow_motion(from_front), up: d.blow_up, horiz: d.blow_speed, face_attacker: from_front },
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
