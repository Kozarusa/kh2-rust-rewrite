//! Sora's player controller, run at the game's 60 Hz logic rate.
//!
//! Movement comes from `motor` (the game's ground, jump, air and fall model), the growth
//! abilities (High Jump, Quick Run, Aerial Dodge, Glide) use the `fmab` rows of their level, and
//! motions carry their own root motion (attack lunges, finisher spins). Motion IDs and blend
//! times are the game's (blends in ticks). Combo flow follows the motions' trigger windows:
//! range trigger 11 (allow combo) and 50 (allow finisher) open the next input; 3 suspends
//! gravity; 2 allows blending out early.

use std::sync::Arc;

use bevy::input::mouse::MouseButton;
use bevy::mesh::skinning::SkinnedMeshInverseBindposes;
use bevy::prelude::*;
use kh2_anim::{id, triggers::range, PlayerState};

use crate::camera::OrbitCamera;
use crate::combat::{Fighter, Hit, Team};
use crate::data::{GameData, Stats};
use crate::enemy::Enemy;
use crate::motor::{wrap_angle, Jump, Motor};
use crate::skel::{spawn_skinned, Animator, Rig};
use crate::world::World;
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

/// Motion IDs of the player motion set (state-independent part of the slot).
mod mid {
    pub const HIGH_JUMP: usize = 201;
    pub const AERIAL_DODGE: usize = 202;
    pub const QUICK_RUN: usize = 203;
    pub const QUICK_RUN_END: usize = 204;
    pub const GLIDE: usize = 205;
    pub const GLIDE_LEFT: usize = 206;
    pub const GLIDE_RIGHT: usize = 207;
    pub const GLIDE_HOVER: usize = 208;
}

/// Blend lengths in ticks.
const BLEND_LOCOMOTION: f32 = 8.0;
const BLEND_FALL: f32 = 8.0;
const BLEND_GLIDE: f32 = 16.0;
const BLEND_ATTACK: f32 = 4.0;
/// Stick magnitude below which Sora walks.
const WALK_STICK: f32 = 0.8;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum State {
    Ground,
    Air,
    QuickRun,
    Glide,
    Attack { air: bool, finisher: bool },
    Guard,
    Counter,
    Hurt,
    Blown,
    Down,
    Viewer,
}

/// Growth-ability levels (0 = not learned, 5 = MAX).
#[derive(Resource, Clone, Copy, Default, Debug)]
pub struct Growth {
    pub high_jump: u8,
    pub quick_run: u8,
    pub aerial_dodge: u8,
    pub glide: u8,
}

/// Controller input, sampled every rendered frame and consumed by the 60 Hz tick.
#[derive(Resource, Default, Debug)]
pub struct Pad {
    /// Camera-relative move direction (unit, or zero).
    pub dir: Vec3,
    /// Stick magnitude 0..1 (keyboard: 1, or 0.5 while walking).
    pub stick: f32,
    pub jump_held: bool,
    pub square_held: bool,
    attack: u8,
    jump: u8,
    square: u8,
    lock: u8,
    viewer: u8,
    next: u8,
    prev: u8,
}

impl Pad {
    fn take(n: &mut u8) -> bool {
        let p = *n > 0;
        *n = n.saturating_sub(1);
        p
    }
}

#[derive(Component)]
pub struct Sora {
    pub state: State,
    pub motor: Motor,
    pub combo: usize,
    pub air_combo: usize,
    pub queued_attack: bool,
    pub stats: Stats,
    pub hp: i32,
    pub mp: i32,
    /// Invincible ticks left.
    pub invuln: f32,
    pub lock: Option<Entity>,
    pub target: Option<Entity>,
    /// Battle mode ticks left (keyblade out).
    pub battle: f32,
    pub body: Entity,
    pub weapon: Entity,
    /// Ticks in the current state.
    timer: f32,
    /// Quick Run: ticks left while the button is held, and the per-tick brake.
    dash_time: f32,
    dash_brake: f32,
    /// One Aerial Dodge per airtime below MAX.
    aerial_dodge_ready: bool,
    lunge_left: f32,
    lunge_speed: f32,
    viewer_slot: usize,
}

impl Sora {
    pub fn grounded(&self) -> bool {
        self.motor.grounded
    }
    pub fn yaw(&self) -> f32 {
        self.motor.yaw
    }
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
            state: State::Ground,
            motor: Motor::default(),
            combo: 0,
            air_combo: 0,
            queued_attack: false,
            hp: stats.max_hp,
            mp: data.sora_max_mp,
            stats,
            invuln: 0.0,
            lock: None,
            target: None,
            battle: 0.0,
            body,
            weapon,
            timer: 0.0,
            dash_time: 0.0,
            dash_brake: 0.0,
            aerial_dodge_ready: false,
            lunge_left: 0.0,
            lunge_speed: 0.0,
            viewer_slot: 0,
        },
        Fighter { team: Team::Player, attack_bodies: vec![weapon, body], hurt_body: body, landed: Default::default(), hittable: true },
    ));
    let growth = std::env::var("KH2_GROWTH")
        .ok()
        .map(|v| {
            let l: Vec<u8> = v.split(',').filter_map(|x| x.parse().ok()).collect();
            Growth { high_jump: l.first().copied().unwrap_or(0), quick_run: l.get(1).copied().unwrap_or(0), aerial_dodge: l.get(2).copied().unwrap_or(0), glide: l.get(3).copied().unwrap_or(0) }
        })
        .unwrap_or_default();
    commands.insert_resource(growth);
}

/// Samples keyboard and mouse into the pad (every rendered frame). WASD move, Ctrl walk,
/// Space jump (Circle), J / left mouse attack (X), K / E guard and growth abilities (Square),
/// Q / Tab lock-on, F1 motion viewer, F6 cycles growth-ability levels.
pub fn read_pad(
    keys: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    cam: Query<&OrbitCamera>,
    mut pad: ResMut<Pad>,
    mut growth: ResMut<Growth>,
) {
    let mut v = Vec2::ZERO;
    for (k, d) in [(KeyCode::KeyW, Vec2::Y), (KeyCode::KeyS, -Vec2::Y), (KeyCode::KeyA, -Vec2::X), (KeyCode::KeyD, Vec2::X)] {
        if keys.pressed(k) {
            v += d;
        }
    }
    let yaw = cam.single().map_or(0.0, |c| c.yaw);
    let fwd = Vec3::new(-yaw.sin(), 0.0, -yaw.cos());
    let right = Vec3::new(-fwd.z, 0.0, fwd.x);
    pad.dir = (fwd * v.y + right * v.x).normalize_or_zero();
    pad.stick = if pad.dir == Vec3::ZERO { 0.0 } else if keys.pressed(KeyCode::ControlLeft) { 0.5 } else { 1.0 };
    pad.jump_held = keys.pressed(KeyCode::Space);
    pad.square_held = keys.pressed(KeyCode::KeyK) || keys.pressed(KeyCode::KeyE);
    let bump = |n: &mut u8, p: bool| {
        if p {
            *n = (*n + 1).min(2);
        }
    };
    bump(&mut pad.attack, keys.just_pressed(KeyCode::KeyJ) || mouse.just_pressed(MouseButton::Left));
    bump(&mut pad.jump, keys.just_pressed(KeyCode::Space));
    bump(&mut pad.square, keys.just_pressed(KeyCode::KeyK) || keys.just_pressed(KeyCode::KeyE));
    bump(&mut pad.lock, keys.just_pressed(KeyCode::KeyQ) || keys.just_pressed(KeyCode::Tab));
    bump(&mut pad.viewer, keys.just_pressed(KeyCode::F1));
    bump(&mut pad.next, keys.just_pressed(KeyCode::BracketRight));
    bump(&mut pad.prev, keys.just_pressed(KeyCode::BracketLeft));
    if keys.just_pressed(KeyCode::F6) {
        let l = (growth.high_jump + 1) % 6;
        *growth = Growth { high_jump: l, quick_run: l, aerial_dodge: l, glide: l };
        info!("growth abilities: level {l}");
    }
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
    weapon.speed = body.speed;
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

/// Height of the ground below Sora (0 when standing on it or nothing is below).
fn height_above_ground(world: &World, m: &Motor) -> f32 {
    world.ground(m.pos, m.pos.y + 1.0, 100_000.0).map_or(0.0, |g| (m.pos.y - g.y).max(0.0))
}

/// One 60 Hz tick of Sora's controller.
#[allow(clippy::too_many_arguments)]
pub fn tick(
    mut pad: ResMut<Pad>,
    data: Res<GameData>,
    growth: Res<Growth>,
    world: Option<Res<World>>,
    enemies: Query<(Entity, &GlobalTransform, &Enemy)>,
    mut sora_q: Query<(&mut Transform, &mut Sora, &mut Fighter)>,
    mut anims: Query<&mut Animator>,
    mut bodies: Query<&mut Transform, Without<Sora>>,
    mut vis: Query<&mut Visibility>,
) {
    let (Ok((mut tf, mut s, mut fighter)), Some(world)) = (sora_q.single_mut(), world) else { return };
    let s = &mut *s;
    let Ok([mut body, mut weapon]) = anims.get_many_mut([s.body, s.weapon]) else { return };
    let rig = body.rig.clone();
    let wrig = weapon.rig.clone();
    let pos_m = s.motor.pos * SCALE;
    let dir = pad.dir;
    let attack = Pad::take(&mut pad.attack);
    let jump = Pad::take(&mut pad.jump);
    let square = Pad::take(&mut pad.square);
    let lock_pressed = Pad::take(&mut pad.lock);
    s.invuln = (s.invuln - 1.0).max(0.0);
    s.timer += 1.0;
    fighter.hittable = s.invuln <= 0.0 && !matches!(s.state, State::Down | State::Viewer);
    let prty = &data.prty;
    let move_speed = if pad.stick <= 0.0 { 0.0 } else if pad.stick < WALK_STICK { prty.walk_speed } else { prty.run_speed };

    // Targets: lock-on (sstm LockOnRange), auto-target (plyr AutoLockOnRange).
    let alive: Vec<(Entity, Vec3)> = enemies.iter().filter(|(_, _, e)| e.alive()).map(|(e, g, _)| (e, g.translation())).collect();
    let lock_range = data.sstm.lock_on_range * SCALE;
    let nearest = |range: f32| {
        alive.iter().filter(|(_, p)| p.distance(pos_m) <= range).min_by(|a, b| a.1.distance(pos_m).total_cmp(&b.1.distance(pos_m))).map(|x| x.0)
    };
    if lock_pressed {
        s.lock = if s.lock.is_some() { None } else { nearest(lock_range) };
    }
    if let Some(l) = s.lock
        && !alive.iter().any(|(e, p)| *e == l && p.distance(pos_m) <= data.sstm.lock_on_release_range * SCALE)
    {
        s.lock = None;
    }
    s.target = s.lock.or_else(|| auto_target(&data.plyr, &s.motor, &alive));
    let target_pos = s.target.and_then(|t| alive.iter().find(|x| x.0 == t).map(|x| x.1 / SCALE));

    // Battle mode (keyblade out) while enemies are near or after attacking.
    if nearest(lock_range).is_some() {
        s.battle = 240.0;
    }
    s.battle = (s.battle - 1.0).max(0.0);
    let battle = s.battle > 0.0;
    let pstate = if battle { PlayerState::BattleWeapon } else { PlayerState::FieldNoWeapon };
    if let Ok(mut v) = vis.get_mut(s.weapon) {
        *v = if battle || s.state == State::Viewer { Visibility::Inherited } else { Visibility::Hidden };
    }
    let motion = |id: usize| rig.mset.slot(id, pstate);

    // Motion viewer: F1 toggles, [ and ] step through every motion slot.
    if Pad::take(&mut pad.viewer) {
        s.state = if s.state == State::Viewer { State::Ground } else { State::Viewer };
    }
    if s.state != State::Viewer
        && let Some(want) = std::env::var("KH2_VIEW_SLOT").ok().and_then(|v| v.parse::<usize>().ok())
    {
        s.state = State::Viewer;
        s.viewer_slot = (0..rig.mset.slots.len()).filter(|&i| rig.mset.has(i)).position(|i| i == want).unwrap_or(0);
    }
    if s.state == State::Viewer {
        let slots: Vec<usize> = (0..rig.mset.slots.len()).filter(|&i| rig.mset.has(i)).collect();
        if Pad::take(&mut pad.next) {
            s.viewer_slot = (s.viewer_slot + 1) % slots.len();
        }
        if Pad::take(&mut pad.prev) {
            s.viewer_slot = (s.viewer_slot + slots.len() - 1) % slots.len();
        }
        let slot = slots.get(s.viewer_slot).copied();
        body.play(slot, true, BLEND_LOCOMOTION);
        weapon.play(slot.filter(|&x| wrig.mset.has(x)), true, BLEND_LOCOMOTION);
        place(&mut tf, s, &body, &mut bodies);
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
        body.restart(slot, false, BLEND_ATTACK);
        weapon.restart(slot.filter(|&x| wrig.mset.has(x)), false, BLEND_ATTACK);
        if air {
            s.air_combo += 1;
        } else {
            s.combo += 1;
        }
        s.state = State::Attack { air, finisher };
        s.timer = 0.0;
        s.queued_attack = false;
        s.battle = 240.0;
        s.motor.vel = Vec3::ZERO;
        // Face the target; close distance beyond the swing's own root motion at plyr V0.
        s.lunge_left = 0.0;
        if let Some(tp) = target_pos {
            let to = (tp - s.motor.pos).with_y(0.0);
            s.motor.yaw = to.x.atan2(to.z);
            s.lunge_left = (to.length() - data.plyr.attack_radius.max(100.0)).max(0.0);
            s.lunge_speed = v0;
        } else if dir != Vec3::ZERO {
            s.motor.yaw = dir.x.atan2(dir.z);
        }
    };

    let start_jump = |s: &mut Sora, body: &mut Animator| {
        let high = data.fmab(growth.high_jump);
        let h = high.map_or(prty.jump_height, |f| f.high_jump_height);
        s.motor.vel = dir * move_speed;
        s.motor.start_jump(h);
        s.aerial_dodge_ready = true;
        s.state = State::Air;
        s.timer = 0.0;
        body.restart(if high.is_some() { motion(mid::HIGH_JUMP).or(motion(id::JUMP)) } else { motion(id::JUMP) }, false, 0.0);
    };

    // Vertical motion this tick: None = follow the floor.
    let mut airborne_dy: Option<f32> = None;
    let fall_max = data.sstm.fall_max;
    match s.state {
        State::Ground => {
            s.combo = 0;
            s.air_combo = 0;
            s.motor.ground_velocity(dir, move_speed);
            if move_speed > 0.0 {
                s.motor.turn_to(dir, prty.turn_speed);
            }
            let landing = body.slot == motion(id::LAND) && !body.finished;
            if move_speed > 0.0 || !landing {
                let id = if move_speed <= 0.0 { id::IDLE } else if move_speed < prty.run_speed { id::WALK } else { id::RUN };
                body.play(motion(id), true, BLEND_LOCOMOTION);
            }
            weapon.play(None, true, BLEND_LOCOMOTION);
            if jump {
                start_jump(s, &mut body);
            } else if attack {
                start_attack(s, &mut body, &mut weapon, false);
            } else if square {
                if let Some(f) = data.fmab(growth.quick_run).filter(|_| move_speed > 0.0) {
                    // Quick Run: dash along the input at AirSlideSpeed while the button is held.
                    s.state = State::QuickRun;
                    s.timer = 0.0;
                    s.motor.yaw = dir.x.atan2(dir.z);
                    s.motor.vel = dir * f.air_slide_speed;
                    s.dash_time = f.air_slide_time;
                    s.dash_brake = f.air_slide_brake;
                    s.invuln = f.air_slide_star_time;
                    body.restart(motion(mid::QUICK_RUN), false, 0.0);
                } else {
                    s.state = State::Guard;
                    s.timer = 0.0;
                    let slot = slot_by_tag(&rig, GUARD);
                    body.restart(slot, false, BLEND_ATTACK);
                    weapon.restart(slot.filter(|&x| wrig.mset.has(x)), false, BLEND_ATTACK);
                    s.motor.vel = Vec3::ZERO;
                    s.battle = 240.0;
                }
            }
        }
        State::Air => {
            s.motor.air_velocity(dir, move_speed);
            if move_speed > 0.0 {
                s.motor.turn_to(dir, prty.turn_speed);
            }
            if s.motor.jump_control(pad.jump_held) {
                body.play(motion(id::FALL), true, BLEND_FALL);
            }
            let can_air_act = s.motor.jump.t > s.motor.jump.rise.max(0.0) + 6.0 || (s.motor.jump.rise == 0.0 && s.motor.jump.t > 15.0);
            if jump && let Some(f) = data.fmab(growth.aerial_dodge).filter(|_| s.aerial_dodge_ready || growth.aerial_dodge >= 5) {
                // Aerial Dodge: a second jump of AirDodgeHeight, moving at AirDodgeSpeed.
                s.aerial_dodge_ready = false;
                s.motor.vel = if move_speed > 0.0 { dir * f.air_dodge_speed } else { s.motor.vel };
                s.motor.start_jump(f.air_dodge_height);
                body.restart(motion(mid::AERIAL_DODGE), false, 0.0);
            } else if square
                && let Some(f) = data.fmab(growth.glide)
                && can_air_act
                && height_above_ground(&world, &s.motor) > f.glide_start_height
            {
                s.state = State::Glide;
                s.timer = 0.0;
                body.play(motion(if move_speed > 0.0 { mid::GLIDE } else { mid::GLIDE_HOVER }), true, BLEND_GLIDE);
            } else if attack && s.air_combo <= AIR_COMBO.len() {
                start_attack(s, &mut body, &mut weapon, true);
            } else if square {
                s.state = State::Guard;
                s.timer = 0.0;
                body.restart(slot_by_tag(&rig, GUARD_AIR), false, BLEND_ATTACK);
            }
            if s.state == State::Air {
                airborne_dy = Some(s.motor.jump_dy(fall_max));
            }
        }
        State::QuickRun => {
            if pad.square_held {
                s.dash_time -= 1.0;
            } else {
                s.dash_time = 0.0;
            }
            if s.dash_time <= 0.0 && body.slot != motion(mid::QUICK_RUN_END) {
                let f = data.fmab(growth.quick_run.max(1)).unwrap();
                s.dash_brake = f.air_slide_stop_brake;
                body.restart(motion(mid::QUICK_RUN_END), false, 0.0);
            }
            s.motor.vel *= s.dash_brake;
            if body.slot == motion(mid::QUICK_RUN_END) && (body.finished || s.motor.vel.length() < prty.walk_speed) {
                s.state = State::Ground;
            }
        }
        State::Glide => {
            let f = data.fmab(growth.glide.max(1)).unwrap();
            let h = height_above_ground(&world, &s.motor);
            if !pad.square_held || h < f.glide_end_height {
                s.state = State::Air;
                s.motor.start_fall();
                body.play(motion(id::FALL), true, BLEND_GLIDE);
            } else {
                if move_speed > 0.0 {
                    // Glide: speed eases toward GlideSpeed, along facing, turning at GlideTurnSpeed.
                    s.motor.turn_to(dir, f.glide_turn_speed);
                    let speed = s.motor.vel.length() * f.glide_acceleration + f.glide_speed * (1.0 - f.glide_acceleration);
                    s.motor.vel = s.motor.forward() * speed;
                    let facing = s.motor.forward();
                    let id = if facing.dot(dir) < 0.98 { if facing.cross(dir).y > 0.0 { mid::GLIDE_LEFT } else { mid::GLIDE_RIGHT } } else { mid::GLIDE };
                    body.play(motion(id), true, BLEND_GLIDE);
                } else {
                    s.motor.vel *= 0.9;
                    body.play(motion(mid::GLIDE_HOVER), true, BLEND_GLIDE);
                }
                let sink = ((h - f.glide_fall_height).max(0.0) * f.glide_fall_ratio).min(f.glide_fall_max);
                airborne_dy = Some(-sink);
            }
        }
        State::Attack { air, .. } => {
            if attack {
                s.queued_attack = true;
            }
            let window = body.trigger(range::ALLOW_COMBO) || body.trigger(range::ALLOW_FINISHER);
            // Close the gap to the target at plyr V0, then let the motion's root motion carry.
            let step = s.lunge_speed.min(s.lunge_left);
            s.lunge_left -= step;
            s.motor.vel = s.motor.forward() * step;
            if let Some(tp) = target_pos
                && (body.trigger(range::TURN_TO_LOCK) || body.trigger(range::TURN_TO_TARGET))
            {
                s.motor.turn_to(tp - s.motor.pos, prty.turn_speed);
            }
            let hover = air || body.trigger(range::NO_GRAVITY) || body.trigger(range::NO_GRAVITY_KEEP_KINETICS);
            if !s.motor.grounded {
                airborne_dy = Some(if hover { 0.0 } else { s.motor.jump_dy(fall_max) });
                if hover {
                    s.motor.jump = Jump::fall(0.0, s.motor.pos.y);
                }
            }
            let can_continue = if air { s.air_combo <= AIR_COMBO.len() } else { s.combo <= GROUND_COMBO.len() };
            if s.queued_attack && window && can_continue {
                start_attack(s, &mut body, &mut weapon, air || !s.motor.grounded);
            } else if jump && window && s.motor.grounded {
                start_jump(s, &mut body);
            } else if body.finished || (body.trigger(range::BLEND_TO_IDLE) && dir != Vec3::ZERO) {
                s.state = if s.motor.grounded { State::Ground } else { State::Air };
                if s.state == State::Air {
                    s.motor.start_fall();
                    body.play(motion(id::FALL), true, BLEND_FALL);
                }
            }
        }
        State::Guard | State::Counter => {
            s.motor.vel = Vec3::ZERO;
            if !s.motor.grounded {
                airborne_dy = Some(s.motor.jump_dy(fall_max));
            }
            if body.finished || (body.trigger(range::ALLOW_COMBO) && (dir != Vec3::ZERO || attack)) {
                s.state = if s.motor.grounded { State::Ground } else { State::Air };
                if attack && s.motor.grounded {
                    start_attack(s, &mut body, &mut weapon, false);
                }
            }
        }
        State::Hurt => {
            s.motor.vel *= 0.85;
            if !s.motor.grounded {
                airborne_dy = Some(s.motor.jump_dy(fall_max));
            }
            if body.finished {
                s.state = if s.motor.grounded { State::Ground } else { State::Air };
            }
        }
        State::Blown => {
            airborne_dy = (!s.motor.grounded).then(|| s.motor.jump_dy(fall_max));
            if s.motor.grounded && s.timer > 12.0 {
                s.motor.vel = Vec3::ZERO;
                if s.hp <= 0 {
                    s.state = State::Down;
                    s.timer = 0.0;
                } else {
                    s.state = State::Hurt;
                    body.restart(motion(id::DAMAGE_BLOW_FRONT_LAND), false, 3.0);
                }
            }
        }
        State::Down => {
            s.motor.vel = Vec3::ZERO;
            if s.timer > 180.0 {
                // KH2 shows the game-over screen here; kh2-rs revives in place.
                s.hp = s.stats.max_hp;
                s.invuln = 120.0;
                s.state = State::Hurt;
                body.restart(motion(id::DAMAGE_BLOW_RECOV), false, 6.0);
            }
        }
        State::Viewer => {}
    }

    // Root motion: yaw turns the facing; translation is rotated by the facing the motion
    // started from.
    let (root_t, root_yaw) = body.root_delta;
    s.motor.yaw = wrap_angle(s.motor.yaw + root_yaw);
    let start_yaw = s.motor.yaw - body.root.yaw;
    let root_move = Quat::from_rotation_y(start_yaw) * root_t;
    let delta = s.motor.vel + root_move.with_y(0.0);
    let dy = airborne_dy.map(|d| d + root_move.y);
    let was_air = airborne_dy.is_some();
    let landed = s.motor.move_by(&world, delta, dy);
    if landed && was_air {
        land(s, &mut body, motion(id::LAND), dir != Vec3::ZERO);
    } else if !s.motor.grounded && s.state == State::Ground {
        // Walked off a ledge.
        s.state = State::Air;
        s.motor.start_fall();
        body.play(motion(id::FALL), true, BLEND_FALL);
    }
    if s.motor.pos.y < world.min.y - 5000.0 || !s.motor.pos.is_finite() {
        s.motor = Motor { pos: world.spawn_point(), ..Motor::default() };
    }
    place(&mut tf, s, &body, &mut bodies);
    if std::env::var("KH2_TRACE").is_ok() {
        // Pelvis (bone 0) in world units vs the character position: root motion check.
        let pelvis = body.pose.first().map_or(Vec3::ZERO, |m| m.w_axis.truncate());
        let world = s.motor.pos + Quat::from_rotation_y(s.motor.yaw) * pelvis;
        info!(
            "trace {:?} {} f{:.1} pos ({:.0} {:.0} {:.0}) yaw {:.2} pelvis ({:.0} {:.0} {:.0}) root ({:.0} {:.2})",
            s.state, body.tag(), body.frame, s.motor.pos.x, s.motor.pos.y, s.motor.pos.z, s.motor.yaw, world.x, world.y, world.z, body.root.translation.z, body.root.yaw
        );
    }
}

/// The game's automatic target choice: among targets within AutoLockOnRange and the height
/// window (plyr heights are Y-down: MinH above, MaxH below), the lowest
/// (distance + |height| × HAdjust) × ((1 − facing·direction) / 2 × (1 − Inner) + Inner).
fn auto_target(p: &kh2_tables::tables::PrefPlyr, m: &Motor, alive: &[(Entity, Vec3)]) -> Option<Entity> {
    let facing = m.forward();
    alive
        .iter()
        .filter_map(|&(e, at_m)| {
            let at = at_m / SCALE;
            let rel = at - m.pos;
            let up = rel.y;
            if up > -p.auto_lock_on_min_h || up < -p.auto_lock_on_max_h {
                return None;
            }
            let flat = rel.with_y(0.0);
            let dist = flat.length();
            if dist > p.auto_lock_on_range {
                return None;
            }
            let dir = flat.normalize_or_zero();
            let inner = p.auto_lock_on_inner_adjust;
            let score = (dist + up.abs() * p.auto_lock_on_h_adjust) * ((1.0 - facing.dot(dir)) * 0.5 * (1.0 - inner) + inner);
            Some((e, score))
        })
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map(|x| x.0)
}

fn land(s: &mut Sora, body: &mut Animator, land_slot: Option<usize>, moving: bool) {
    s.air_combo = 0;
    match s.state {
        State::Air | State::Glide | State::Blown => {
            if s.state != State::Blown {
                s.state = State::Ground;
                if !moving {
                    body.restart(land_slot, false, 0.0);
                }
            }
        }
        _ => {}
    }
}

/// Writes Sora's transform (poses are root-relative, so the body needs no offset).
fn place(tf: &mut Transform, s: &Sora, _body: &Animator, _bodies: &mut Query<&mut Transform, Without<Sora>>) {
    tf.translation = s.motor.pos * SCALE;
    tf.rotation = Quat::from_rotation_y(s.motor.yaw);
}

/// Puts Sora on an area's spawn point.
pub fn respawn(s: &mut Sora, at: Vec3) {
    s.motor = Motor { pos: at, ..Motor::default() };
    s.state = State::Ground;
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
        let facing = s.motor.forward();
        let from_front = facing.dot(-h.dir) > 0.0;
        // Guard: repelled while A320's guard window is open (before its combo window).
        if s.state == State::Guard && !body.trigger(range::ALLOW_COMBO) && from_front {
            let slot = slot_by_tag(&rig, GUARD_COUNTER);
            body.restart(slot, false, 2.0);
            let wslot = slot.filter(|&x| weapon.rig.mset.has(x));
            weapon.restart(wslot, false, 2.0);
            s.state = State::Counter;
            s.invuln = 18.0;
            continue;
        }
        let Some(atk) = data.atkp(h.atkp) else { continue };
        let from = enemies.get(h.attacker).map(|e| e.stats).unwrap_or(s.stats);
        let dmg = data.damage(atk, &from, &s.stats).max(1);
        s.hp = (s.hp - dmg).max(0);
        let pstate = PlayerState::BattleWeapon;
        let blow = atk.enemy_reaction >= 5 || s.hp == 0 || !s.motor.grounded;
        // Knockback per tick from the attack's strengths (approximation of the reaction code).
        let kb = (atk.knockback_strength1 as f32).clamp(0.0, 400.0) / 60.0;
        s.motor.vel = h.dir.with_y(0.0).normalize_or_zero() * kb.max(2.0);
        if blow {
            s.state = State::Blown;
            s.motor.start_jump((atk.knockback_strength2 as f32).clamp(40.0, 300.0));
            s.timer = 0.0;
            body.restart(rig.mset.slot(if from_front { id::DAMAGE_BLOW_FRONT } else { 18 }, pstate), false, 2.0);
        } else {
            s.state = State::Hurt;
            body.restart(rig.mset.slot(if from_front { id::DAMAGE_S_FRONT_HIGH } else { id::DAMAGE_S_BACK_HIGH }, pstate), false, 2.0);
        }
        weapon.restart(None, false, 2.0);
        s.invuln = 36.0;
        s.combo = 0;
        fighter.landed.clear();
    }
}
