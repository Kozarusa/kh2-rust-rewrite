//! Sora: model from `obj/P_EX100.mdlx`, third-person movement and an orbit camera.

use bevy::input::mouse::{AccumulatedMouseMotion, AccumulatedMouseScroll};
use bevy::prelude::*;

use crate::assets::{spawn_model, Disc};
use crate::collision::Collider;
use crate::SCALE;

const RUN: f32 = 7.0; // m/s
const WALK: f32 = 2.5;
const JUMP: f32 = 7.5;
const GRAVITY: f32 = 24.0;
const STEP: f32 = 0.45; // how high a ledge Sora walks up without jumping
const KNEE: f32 = 0.5;

#[derive(Component, Default)]
pub struct Player {
    pub velocity: Vec3,
    pub grounded: bool,
}

#[derive(Component)]
pub struct OrbitCamera {
    pub yaw: f32,
    pub pitch: f32,
    pub distance: f32,
}

pub fn setup(
    mut commands: Commands,
    disc: Res<Disc>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut images: ResMut<Assets<Image>>,
) {
    let player = commands.spawn((Player::default(), Transform::default(), Visibility::default())).id();
    match disc.read("obj/P_EX100.mdlx").and_then(|d| kh2_model::mdlx(&d)) {
        Some(model) => {
            let body = commands.spawn((Transform::from_scale(Vec3::splat(SCALE)), Visibility::default(), ChildOf(player))).id();
            spawn_model(&mut commands, body, &model, &mut meshes, &mut materials, &mut images);
        }
        None => warn!("obj/P_EX100.mdlx: could not load Sora"),
    }
    commands.spawn((
        Camera3d::default(),
        Transform::from_xyz(0.0, 3.0, 6.0).looking_at(Vec3::ZERO, Vec3::Y),
        OrbitCamera { yaw: 0.0, pitch: 0.35, distance: 5.5 },
    ));
}

pub fn control(
    time: Res<Time>,
    keys: Res<ButtonInput<KeyCode>>,
    collider: Option<Res<Collider>>,
    cam: Query<&OrbitCamera>,
    mut q: Query<(&mut Transform, &mut Player)>,
) {
    let (Ok(cam), Ok((mut t, mut p)), Some(col)) = (cam.single(), q.single_mut(), collider) else { return };
    let dt = time.delta_secs().min(0.05);
    let mut input = Vec2::ZERO;
    for (k, v) in [(KeyCode::KeyW, Vec2::Y), (KeyCode::KeyS, -Vec2::Y), (KeyCode::KeyA, -Vec2::X), (KeyCode::KeyD, Vec2::X)] {
        if keys.pressed(k) {
            input += v;
        }
    }
    // Camera-relative: forward is away from the camera.
    let fwd = Vec3::new(-cam.yaw.sin(), 0.0, -cam.yaw.cos());
    let right = Vec3::new(-fwd.z, 0.0, fwd.x);
    let wish = (fwd * input.y + right * input.x).normalize_or_zero();
    let speed = if keys.pressed(KeyCode::ShiftLeft) { WALK } else { RUN };

    if wish != Vec3::ZERO {
        let target = Quat::from_rotation_y(f32::atan2(wish.x, wish.z));
        t.rotation = t.rotation.slerp(target, (dt * 14.0).min(1.0));
    }
    // Horizontal move, blocked by walls at knee height.
    let step = wish * speed * dt;
    let here = t.translation + Vec3::Y * KNEE;
    for axis in [step, Vec3::new(step.x, 0.0, 0.0), Vec3::new(0.0, 0.0, step.z)] {
        if axis != Vec3::ZERO && !col.blocked(here, here + axis + axis.normalize() * 0.3) {
            t.translation += axis;
            break;
        }
    }
    // Vertical: gravity, jumping, ground snapping with a step-up allowance.
    if p.grounded && keys.just_pressed(KeyCode::Space) {
        p.velocity.y = JUMP;
        p.grounded = false;
    }
    p.velocity.y -= GRAVITY * dt;
    let next_y = t.translation.y + p.velocity.y * dt;
    match col.ground(t.translation.x, t.translation.z, t.translation.y + STEP) {
        Some(g) if next_y <= g => {
            t.translation.y = g;
            p.velocity.y = 0.0;
            p.grounded = true;
        }
        Some(g) if p.grounded && p.velocity.y <= 0.0 && t.translation.y - g < STEP => {
            t.translation.y = g; // walking down a slope or step
            p.velocity.y = 0.0;
        }
        _ => {
            t.translation.y = next_y;
            p.grounded = false;
        }
    }
    if t.translation.y < -200.0 {
        t.translation = Vec3::new(t.translation.x, 50.0, t.translation.z); // fell out of the world
        p.velocity = Vec3::ZERO;
    }
}

#[allow(clippy::too_many_arguments)] // Bevy system parameters
pub fn camera_follow(
    time: Res<Time>,
    keys: Res<ButtonInput<KeyCode>>,
    buttons: Res<ButtonInput<MouseButton>>,
    motion: Res<AccumulatedMouseMotion>,
    scroll: Res<AccumulatedMouseScroll>,
    collider: Option<Res<Collider>>,
    player: Query<&Transform, (With<Player>, Without<OrbitCamera>)>,
    mut cam: Query<(&mut Transform, &mut OrbitCamera)>,
) {
    let (Ok(p), Ok((mut t, mut o))) = (player.single(), cam.single_mut()) else { return };
    let dt = time.delta_secs();
    if buttons.pressed(MouseButton::Right) {
        o.yaw -= motion.delta.x * 0.005;
        o.pitch = (o.pitch + motion.delta.y * 0.004).clamp(-0.2, 1.3);
    }
    let turn = |a: KeyCode, b: KeyCode| (keys.pressed(a) as i32 - keys.pressed(b) as i32) as f32;
    o.yaw += turn(KeyCode::ArrowLeft, KeyCode::ArrowRight) * dt * 2.2;
    o.pitch = (o.pitch + turn(KeyCode::ArrowUp, KeyCode::ArrowDown) * dt * 1.2).clamp(-0.2, 1.3);
    o.distance = (o.distance - scroll.delta.y * 0.4 + turn(KeyCode::PageDown, KeyCode::PageUp) * dt * 6.0).clamp(1.5, 25.0);
    let focus = p.translation + Vec3::Y * 1.2;
    let dir = Vec3::new(o.yaw.sin() * o.pitch.cos(), o.pitch.sin(), o.yaw.cos() * o.pitch.cos());
    // Keep the camera in front of walls between it and Sora.
    let dist = collider
        .and_then(|c| c.first_hit(focus, focus + dir * o.distance))
        .map_or(o.distance, |hit| (hit - 0.25).max(0.4));
    *t = Transform::from_translation(focus + dir * dist).looking_at(focus, Vec3::Y);
}
