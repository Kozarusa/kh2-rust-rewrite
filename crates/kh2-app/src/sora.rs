//! Sora: state machine over his real motion set, with movement constants from `pref`/`prty`
//! and `plyr`, hitboxes from the Kingdom Key's motion triggers, and damage from `atkp`.
//!
//! Combo flow follows the motions' own trigger windows: range trigger 11 (allow combo) and
//! 50 (allow finisher) open the next input; 3 suspends gravity; 2 allows blending out early.

use std::sync::Arc;

use bevy::mesh::skinning::SkinnedMeshInverseBindposes;
use bevy::prelude::*;
use kh2_anim::{id, triggers::range, PlayerState};

use crate::camera::OrbitCamera;
use crate::collision::Collider;
use crate::combat::{Fighter, Hit, Team};
use crate::data::{GameData, Stats, GRAVITY, LOGIC_HZ};
use crate::enemy::Enemy;
use crate::skel::{spawn_skinned, Animator, Rig};
use crate::SCALE;

/// Sora's right-hand bone (mirror of the left hand, bone 86).
const RIGHT_HAND: usize = 178;
const GROUND_COMBO: [&str; 3] = ["A300", "A301", "A302"];
const GROUND_FINISH: [&str; 2] = ["A303", "A304"];
const AIR_COMBO: [&str; 4] = ["A310", "A311", "A312", "A313"];
const AIR_FINISH: &str = "A314";
const GUARD: &str = "A320";
const GUARD_COUNTER: &str = "A321";
const GUARD_AIR: &str = "A350";
/// Dodge Roll: the tucked roll loop. Speed/length tuned to the game's ~3 m roll.
const ROLL: &str = "A160";
const ROLL_SPEED: f32 = 8.0;
const ROLL_TIME: f32 = 0.45;
const ROLL_INVULN: f32 = 0.3;
/// Frames of A320 during which a hit is repelled (until its allow-combo window opens).
const BLEND: f32 = 0.12;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum State {
    Move,
    Air,
    Land,
    Attack { air: bool, finisher: bool },
    Guard,
    Counter,
    Roll,
    Hurt,
    Blown,
    Down,
    Viewer,
}

#[derive(Component)]
pub struct Sora {
    pub state: State,
    pub vel: Vec3,
    pub grounded: bool,
    pub yaw: f32,
    pub combo: usize,
    pub air_combo: usize,
    pub queued_attack: bool,
    pub stats: Stats,
    pub hp: i32,
    pub mp: i32,
    pub invuln: f32,
    pub lock: Option<Entity>,
    pub target: Option<Entity>,
    pub battle_timer: f32,
    pub body: Entity,
    pub weapon: Entity,
    lunge_left: f32,
    lunge_speed: f32,
    timer: f32,
    viewer_slot: usize,
}

#[derive(Resource)]
pub struct Rigs {
    pub sora: Arc<Rig>,
    pub keyblade: Arc<Rig>,
    pub shadow: Arc<Rig>,
    pub soldier: Arc<Rig>,
}

#[allow(clippy::too_many_arguments)]
pub fn setup(
    mut commands: Commands,
    rigs: Res<Rigs>,
    data: Res<GameData>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut images: ResMut<Assets<Image>>,
    mut bindposes: ResMut<Assets<SkinnedMeshInverseBindposes>>,
) {
    let root = commands.spawn((Transform::default(), Visibility::default())).id();
    let body = spawn_skinned(&mut commands, root, rigs.sora.clone(), &mut meshes, &mut materials, &mut images, &mut bindposes, SCALE);
    // The keyblade is its own skinned model parented to Sora's right-hand joint (already in model units).
    let hand = commands.spawn((Transform::default(), Visibility::default(), ChildOf(body))).id();
    let weapon = spawn_skinned(&mut commands, hand, rigs.keyblade.clone(), &mut meshes, &mut materials, &mut images, &mut bindposes, 1.0);
    commands.entity(hand).insert(HandSocket);
    let stats = data.sora_stats(1);
    commands.entity(root).insert((
        Sora {
            state: State::Move,
            vel: Vec3::ZERO,
            grounded: false,
            yaw: 0.0,
            combo: 0,
            air_combo: 0,
            queued_attack: false,
            hp: stats.max_hp,
            mp: data.sora_max_mp,
            stats,
            invuln: 0.0,
            lock: None,
            target: None,
            battle_timer: 0.0,
            body,
            weapon,
            lunge_left: 0.0,
            lunge_speed: 0.0,
            timer: 0.0,
            viewer_slot: 0,
        },
        Fighter { team: Team::Player, attack_bodies: vec![weapon, body], hurt_body: body, landed: Default::default(), hittable: true },
    ));
}

/// The keyblade's motion set mirrors Sora's slot for slot: lock it to Sora's slot and frame.
pub fn sync_weapon(sora: Query<&Sora>, mut anims: Query<&mut Animator>) {
    let Ok(s) = sora.single() else { return };
    let Ok([body, mut weapon]) = anims.get_many_mut([s.body, s.weapon]) else { return };
    let slot = body.slot.filter(|&x| weapon.rig.mset.has(x));
    if weapon.slot != slot {
        weapon.restart(slot, body.looping, 0.0);
    }
    weapon.frame = body.frame;
    weapon.prev_frame = body.prev_frame;
    weapon.looping = body.looping;
    weapon.speed = body.speed; // advances in lockstep within `animate`
}

/// Marks the entity that follows Sora's right-hand joint.
#[derive(Component)]
pub struct HandSocket;

/// Keeps the keyblade socket on the hand bone (the joint entities are siblings under the body).
pub fn follow_hand(sora: Query<&Sora>, anims: Query<&Animator>, mut socket: Query<&mut Transform, With<HandSocket>>) {
    let (Ok(s), Ok(mut t)) = (sora.single(), socket.single_mut()) else { return };
    if let Ok(a) = anims.get(s.body) {
        *t = Transform::from_matrix(a.pose.get(RIGHT_HAND).copied().unwrap_or(Mat4::IDENTITY));
    }
}

fn slot_by_tag(rig: &Rig, tag: &str) -> Option<usize> {
    rig.mset.by_tag(tag).next()
}

fn input_dir(keys: &ButtonInput<KeyCode>, cam: &OrbitCamera) -> Vec3 {
    let mut v = Vec2::ZERO;
    for (k, d) in [(KeyCode::KeyW, Vec2::Y), (KeyCode::KeyS, -Vec2::Y), (KeyCode::KeyA, -Vec2::X), (KeyCode::KeyD, Vec2::X)] {
        if keys.pressed(k) {
            v += d;
        }
    }
    let fwd = Vec3::new(-cam.yaw.sin(), 0.0, -cam.yaw.cos());
    let right = Vec3::new(-fwd.z, 0.0, fwd.x);
    (fwd * v.y + right * v.x).normalize_or_zero()
}

fn turn_toward(yaw: &mut f32, dir: Vec3, max_step: f32) {
    if dir.length_squared() < 1e-6 {
        return;
    }
    let target = f32::atan2(dir.x, dir.z);
    let mut d = (target - *yaw + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU) - std::f32::consts::PI;
    d = d.clamp(-max_step, max_step);
    *yaw += d;
}

#[allow(clippy::too_many_arguments)]
pub fn control(
    time: Res<Time>,
    keys: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    data: Res<GameData>,
    collider: Option<Res<Collider>>,
    cam: Query<&OrbitCamera>,
    enemies: Query<(Entity, &GlobalTransform, &Enemy)>,
    mut sora_q: Query<(&mut Transform, &mut Sora, &mut Fighter)>,
    mut anims: Query<&mut Animator>,
    mut vis: Query<&mut Visibility>,
) {
    let (Ok(cam), Ok((mut tf, mut s, mut fighter)), Some(col)) = (cam.single(), sora_q.single_mut(), collider) else { return };
    let dt = time.delta_secs().min(0.05);
    let s = &mut *s;
    let Ok([mut body, mut weapon]) = anims.get_many_mut([s.body, s.weapon]) else { return };
    let rig = body.rig.clone();
    let wrig = weapon.rig.clone();
    let pos = tf.translation;
    let dir = input_dir(&keys, cam);
    let attack = keys.just_pressed(KeyCode::KeyJ) || mouse.just_pressed(MouseButton::Left);
    let jump = keys.just_pressed(KeyCode::Space);
    let guard = keys.just_pressed(KeyCode::KeyK) || keys.just_pressed(KeyCode::KeyE);
    s.invuln = (s.invuln - dt).max(0.0);
    s.timer += dt;
    fighter.hittable = s.invuln <= 0.0 && !matches!(s.state, State::Down | State::Viewer);

    // Targets: lock-on (Q) within the system lock range; auto-target within the attack range.
    let alive: Vec<(Entity, Vec3)> = enemies.iter().filter(|(_, _, e)| e.alive()).map(|(e, g, _)| (e, g.translation())).collect();
    let lock_range = data.sstm.lock_on_range * SCALE;
    let nearest = |range: f32| {
        alive.iter().filter(|(_, p)| p.distance(pos) <= range).min_by(|a, b| a.1.distance(pos).total_cmp(&b.1.distance(pos))).map(|x| x.0)
    };
    if keys.just_pressed(KeyCode::KeyQ) || keys.just_pressed(KeyCode::Tab) {
        s.lock = if s.lock.is_some() { None } else { nearest(lock_range) };
    }
    if let Some(l) = s.lock
        && !alive.iter().any(|(e, p)| *e == l && p.distance(pos) <= data.sstm.lock_on_release_range * SCALE)
    {
        s.lock = None;
    }
    let auto_range = (data.plyr.auto_lock_on_range * SCALE).max(3.0);
    s.target = s.lock.or_else(|| nearest(auto_range));
    let target_pos = s.target.and_then(|t| alive.iter().find(|x| x.0 == t).map(|x| x.1));

    // Battle mode (keyblade out) while enemies are near or after attacking.
    if nearest(lock_range).is_some() {
        s.battle_timer = 4.0;
    }
    s.battle_timer = (s.battle_timer - dt).max(0.0);
    let battle = s.battle_timer > 0.0;
    let pstate = if battle { PlayerState::BattleWeapon } else { PlayerState::FieldNoWeapon };
    if let Ok(mut v) = vis.get_mut(s.weapon) {
        *v = if battle { Visibility::Inherited } else { Visibility::Hidden };
    }
    let motion = |id: usize| rig.mset.slot(id, pstate);

    // Motion viewer: F1 toggles, [ and ] step through every motion slot.
    if keys.just_pressed(KeyCode::F1) {
        s.state = if s.state == State::Viewer { State::Move } else { State::Viewer };
    }
    // Test hook: KH2_VIEW_SLOT=<slot> opens the viewer on that motion slot.
    if s.state != State::Viewer
        && let Some(want) = std::env::var("KH2_VIEW_SLOT").ok().and_then(|v| v.parse::<usize>().ok())
    {
        s.state = State::Viewer;
        s.viewer_slot = (0..rig.mset.slots.len()).filter(|&i| rig.mset.has(i)).position(|i| i == want).unwrap_or(0);
    }
    if s.state == State::Viewer {
        let slots: Vec<usize> = (0..rig.mset.slots.len()).filter(|&i| rig.mset.has(i)).collect();
        if keys.just_pressed(KeyCode::BracketRight) {
            s.viewer_slot = (s.viewer_slot + 1) % slots.len();
        }
        if keys.just_pressed(KeyCode::BracketLeft) {
            s.viewer_slot = (s.viewer_slot + slots.len() - 1) % slots.len();
        }
        let slot = slots.get(s.viewer_slot).copied();
        body.play(slot, true, BLEND);
        weapon.play(slot.filter(|&x| wrig.mset.has(x)), true, BLEND);
        if let Ok(mut v) = vis.get_mut(s.weapon) {
            *v = Visibility::Inherited;
        }
        tf.rotation = Quat::from_rotation_y(s.yaw);
        return;
    }

    let start_attack = |s: &mut Sora, body: &mut Animator, weapon: &mut Animator, air: bool| {
        let (tag, finisher, v0) = if air {
            if s.air_combo < AIR_COMBO.len() {
                (AIR_COMBO[s.air_combo], false, if s.air_combo == 0 { data.plyr.attack_first_v0 } else { data.plyr.attack_combo_v0 })
            } else {
                (AIR_FINISH, true, data.plyr.attack_finish_v0)
            }
        } else if s.combo < GROUND_COMBO.len() {
            (GROUND_COMBO[s.combo], false, if s.combo == 0 { data.plyr.attack_first_v0 } else { data.plyr.attack_combo_v0 })
        } else {
            (GROUND_FINISH[s.combo % 2], true, data.plyr.attack_finish_v0)
        };
        let slot = slot_by_tag(&rig, tag);
        body.restart(slot, false, 0.06);
        weapon.restart(slot.filter(|&x| wrig.mset.has(x)), false, 0.06);
        if air {
            s.air_combo += 1;
            s.vel.y = s.vel.y.max(0.0);
        } else {
            s.combo += 1;
        }
        s.state = State::Attack { air, finisher };
        s.queued_attack = false;
        s.battle_timer = 4.0;
        // Lunge toward the target: plyr V0 (units/frame), stopping at weapon reach.
        s.lunge_left = 0.0;
        if let Some(tp) = target_pos {
            let to = (tp - pos).with_y(0.0);
            s.yaw = f32::atan2(to.x, to.z);
            s.lunge_left = (to.length() - 1.1).max(0.0);
            s.lunge_speed = v0 * LOGIC_HZ * SCALE;
        } else if dir != Vec3::ZERO {
            s.yaw = f32::atan2(dir.x, dir.z);
        }
    };

    let mut gravity = GRAVITY;
    match s.state {
        State::Move | State::Land => {
            s.combo = 0;
            s.air_combo = 0;
            let walk = keys.pressed(KeyCode::ControlLeft);
            if dir != Vec3::ZERO {
                let speed = if walk { data.walk_speed() } else { data.run_speed() };
                s.vel = (dir * speed).with_y(s.vel.y);
                turn_toward(&mut s.yaw, dir, data.turn_rate() * dt);
                body.play(motion(if walk { id::WALK } else { id::RUN }), true, BLEND);
                s.state = State::Move;
            } else {
                s.vel = Vec3::new(0.0, s.vel.y, 0.0);
                if s.state == State::Land && !body.finished {
                    // finish the landing motion
                } else {
                    s.state = State::Move;
                    body.play(motion(id::IDLE), true, 0.2);
                }
            }
            weapon.play(None, true, BLEND);
            if !s.grounded {
                s.state = State::Air;
                body.play(motion(id::FALL), true, 0.15);
            } else if jump {
                s.vel.y = data.jump_velocity();
                s.grounded = false;
                s.state = State::Air;
                body.restart(motion(id::JUMP), false, 0.06);
            } else if attack {
                start_attack(s, &mut body, &mut weapon, false);
            } else if guard && dir != Vec3::ZERO {
                // Guard button + direction = Dodge Roll (KH2 ability), invincible early on.
                s.state = State::Roll;
                s.timer = 0.0;
                s.yaw = f32::atan2(dir.x, dir.z);
                s.vel = (dir * ROLL_SPEED).with_y(s.vel.y);
                s.invuln = ROLL_INVULN;
                body.restart(slot_by_tag(&rig, ROLL), true, 0.04);
                s.battle_timer = 4.0;
            } else if guard {
                s.state = State::Guard;
                let slot = slot_by_tag(&rig, GUARD);
                body.restart(slot, false, 0.05);
                weapon.restart(slot.filter(|&x| wrig.mset.has(x)), false, 0.05);
                s.vel = Vec3::new(0.0, s.vel.y, 0.0);
                s.battle_timer = 4.0;
            }
        }
        State::Air => {
            if dir != Vec3::ZERO {
                s.vel = (dir * data.run_speed()).with_y(s.vel.y);
                turn_toward(&mut s.yaw, dir, data.turn_rate() * dt);
            } else {
                s.vel = Vec3::new(s.vel.x * 0.9, s.vel.y, s.vel.z * 0.9);
            }
            if s.vel.y < 0.0 && (body.finished || body.slot != motion(id::JUMP)) {
                body.play(motion(id::FALL), true, 0.15);
            }
            if attack && s.air_combo <= AIR_COMBO.len() {
                start_attack(s, &mut body, &mut weapon, true);
            } else if guard {
                s.state = State::Guard;
                body.restart(slot_by_tag(&rig, GUARD_AIR), false, 0.05);
            } else if s.grounded {
                s.air_combo = 0;
                s.state = State::Land;
                if dir == Vec3::ZERO {
                    body.restart(motion(id::LAND), false, 0.05);
                }
            }
        }
        State::Attack { air, .. } => {
            if attack {
                s.queued_attack = true;
            }
            let window = body.trigger(range::ALLOW_COMBO) || body.trigger(range::ALLOW_FINISHER);
            // Aerial attacks hold Sora in the air for the whole swing (KH2 air combos hover);
            // ground motions can also suspend gravity through their triggers.
            if air || body.trigger(range::NO_GRAVITY) || body.trigger(range::NO_GRAVITY_KEEP_KINETICS) {
                gravity = 0.0;
                s.vel.y = 0.0;
            }
            // Lunge, then hold position.
            let step = (s.lunge_speed * dt).min(s.lunge_left);
            s.lunge_left -= step;
            let fwd = Vec3::new(s.yaw.sin(), 0.0, s.yaw.cos());
            s.vel = (fwd * if step > 0.0 { s.lunge_speed } else { 0.0 }).with_y(s.vel.y);
            if let Some(tp) = target_pos
                && (body.trigger(range::TURN_TO_LOCK) || body.trigger(range::TURN_TO_TARGET))
            {
                turn_toward(&mut s.yaw, tp - pos, data.turn_rate() * dt);
            }
            let can_continue = if air { s.air_combo <= AIR_COMBO.len() } else { s.combo <= GROUND_COMBO.len() };
            if s.queued_attack && window && can_continue {
                start_attack(s, &mut body, &mut weapon, air || !s.grounded);
            } else if jump && window && s.grounded {
                s.vel.y = data.jump_velocity();
                s.grounded = false;
                s.state = State::Air;
                body.restart(motion(id::JUMP), false, 0.06);
            } else if body.finished || (body.trigger(range::BLEND_TO_IDLE) && dir != Vec3::ZERO) {
                s.state = if s.grounded { State::Move } else { State::Air };
                if s.grounded {
                    s.combo = 0;
                }
            }
        }
        State::Roll => {
            let fwd = Vec3::new(s.yaw.sin(), 0.0, s.yaw.cos());
            s.vel = (fwd * ROLL_SPEED * (1.0 - (s.timer / ROLL_TIME).powi(2)).max(0.0)).with_y(s.vel.y);
            if s.timer >= ROLL_TIME {
                s.state = if s.grounded { State::Move } else { State::Air };
            } else if attack && s.timer > ROLL_TIME * 0.6 {
                start_attack(s, &mut body, &mut weapon, false);
            }
        }
        State::Guard | State::Counter => {
            s.vel = Vec3::new(0.0, s.vel.y, 0.0);
            if body.finished || (body.trigger(range::ALLOW_COMBO) && (dir != Vec3::ZERO || attack)) {
                s.state = if s.grounded { State::Move } else { State::Air };
                if attack && s.grounded {
                    start_attack(s, &mut body, &mut weapon, false);
                }
            }
        }
        State::Hurt => {
            s.vel *= Vec3::new(0.85, 1.0, 0.85);
            if body.finished {
                s.state = if s.grounded { State::Move } else { State::Air };
            }
        }
        State::Blown => {
            if s.grounded && s.timer > 0.2 {
                s.vel = Vec3::ZERO;
                if s.hp <= 0 {
                    s.state = State::Down;
                    s.timer = 0.0;
                } else {
                    s.state = State::Hurt;
                    body.restart(motion(id::DAMAGE_BLOW_FRONT_LAND), false, 0.05);
                }
            }
        }
        State::Down => {
            s.vel = Vec3::ZERO;
            if s.timer > 3.0 {
                // KH2 shows the game-over screen here; kh2-rs revives in place.
                s.hp = s.stats.max_hp;
                s.invuln = 2.0;
                s.state = State::Hurt;
                body.restart(motion(id::DAMAGE_BLOW_RECOV), false, 0.1);
            }
        }
        State::Viewer => {}
    }

    let mut grounded = s.grounded;
    col.step(&mut tf.translation, &mut s.vel, dt, gravity, data.fall_max(), &mut grounded);
    s.grounded = grounded;
    tf.rotation = Quat::from_rotation_y(s.yaw);
}

/// Applies enemy hits to Sora: guard repels from the front, otherwise damage and reactions.
pub fn receive_hits(
    mut hits: MessageReader<Hit>,
    data: Res<GameData>,
    enemies: Query<&Enemy>,
    mut sora_q: Query<(Entity, &mut Sora, &mut Fighter)>,
    mut anims: Query<&mut Animator>,
) {
    let Ok((me, mut s, mut fighter)) = sora_q.single_mut() else { return };
    for h in hits.read().filter(|h| h.target == me) {
        if s.invuln > 0.0 || matches!(s.state, State::Down | State::Viewer) {
            continue;
        }
        let Ok([mut body, mut weapon]) = anims.get_many_mut([s.body, s.weapon]) else { return };
        let rig = body.rig.clone();
        let facing = Vec3::new(s.yaw.sin(), 0.0, s.yaw.cos());
        let from_front = facing.dot(-h.dir) > 0.0;
        // Guard: repelled while A320's guard window is open (before its combo window).
        if s.state == State::Guard && !body.trigger(range::ALLOW_COMBO) && from_front {
            let slot = slot_by_tag(&rig, GUARD_COUNTER);
            body.restart(slot, false, 0.03);
            let wslot = slot.filter(|&x| weapon.rig.mset.has(x));
            weapon.restart(wslot, false, 0.03);
            s.state = State::Counter;
            s.invuln = 0.3;
            continue;
        }
        let Some(atk) = data.atkp(h.atkp) else { continue };
        let from = enemies.get(h.attacker).map(|e| e.stats).unwrap_or(s.stats);
        let dmg = data.damage(atk, &from, &s.stats).max(1);
        s.hp = (s.hp - dmg).max(0);
        let pstate = PlayerState::BattleWeapon;
        let blow = atk.enemy_reaction >= 5 || s.hp == 0 || !s.grounded;
        let kb = (atk.knockback_strength1 as f32).clamp(0.0, 400.0) * SCALE * 4.0;
        s.vel = h.dir * kb + Vec3::Y * if blow { (2.0 * GRAVITY * (atk.knockback_strength2 as f32 * SCALE).max(0.6)).sqrt() } else { 0.0 };
        if blow {
            s.state = State::Blown;
            s.grounded = false;
            s.timer = 0.0;
            body.restart(rig.mset.slot(if from_front { id::DAMAGE_BLOW_FRONT } else { 18 }, pstate), false, 0.03);
        } else {
            s.state = State::Hurt;
            body.restart(rig.mset.slot(if from_front { id::DAMAGE_S_FRONT_HIGH } else { id::DAMAGE_S_BACK_HIGH }, pstate), false, 0.03);
        }
        weapon.restart(None, false, 0.03);
        s.invuln = 0.6;
        s.combo = 0;
        fighter.landed.clear();
    }
}
