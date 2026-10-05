//! Third-person orbit camera. With a lock-on target it swings behind Sora to keep both in view.

use bevy::input::mouse::{AccumulatedMouseMotion, AccumulatedMouseScroll};
use bevy::prelude::*;

use crate::world::CameraWorld;
use crate::sora::Sora;

#[derive(Component)]
pub struct OrbitCamera {
    pub yaw: f32,
    pub pitch: f32,
    pub distance: f32,
}

pub struct CameraPlugin;

impl Plugin for CameraPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(ClearColor(Color::srgb(0.05, 0.05, 0.08)))
            .add_systems(Startup, setup)
            .add_systems(Update, follow.in_set(crate::schedule::Frame::Camera));
    }
}

pub fn setup(mut commands: Commands) {
    commands.spawn((
        Camera3d::default(),
        Transform::from_xyz(0.0, 3.0, 6.0).looking_at(Vec3::ZERO, Vec3::Y),
        OrbitCamera { yaw: 0.0, pitch: 0.3, distance: 5.0 },
    ));
}

#[allow(clippy::too_many_arguments)] // Bevy system parameters
pub fn follow(
    time: Res<Time>,
    keys: Res<ButtonInput<KeyCode>>,
    buttons: Res<ButtonInput<MouseButton>>,
    motion: Res<AccumulatedMouseMotion>,
    scroll: Res<AccumulatedMouseScroll>,
    collider: Option<Res<CameraWorld>>,
    sora: Query<(&Transform, &Sora), Without<OrbitCamera>>,
    targets: Query<&GlobalTransform>,
    mut cam: Query<(&mut Transform, &mut OrbitCamera)>,
) {
    let (Ok((p, s)), Ok((mut t, mut o))) = (sora.single(), cam.single_mut()) else { return };
    let dt = time.delta_secs();
    if buttons.pressed(MouseButton::Right) {
        o.yaw -= motion.delta.x * 0.005;
        o.pitch = (o.pitch + motion.delta.y * 0.004).clamp(-0.2, 1.3);
    }
    let turn = |a: KeyCode, b: KeyCode| (keys.pressed(a) as i32 - keys.pressed(b) as i32) as f32;
    o.yaw += turn(KeyCode::ArrowLeft, KeyCode::ArrowRight) * dt * 2.2;
    o.pitch = (o.pitch + turn(KeyCode::ArrowUp, KeyCode::ArrowDown) * dt * 1.2).clamp(-0.2, 1.3);
    o.distance = (o.distance - scroll.delta.y * 0.4 + turn(KeyCode::PageDown, KeyCode::PageUp) * dt * 6.0).clamp(1.5, 25.0);
    // Lock-on: ease the camera behind Sora, facing the target.
    if let Some(tp) = s.lock.and_then(|l| targets.get(l).ok()).map(|g| g.translation()) {
        let away = (p.translation - tp).with_y(0.0);
        if away.length() > 0.5 {
            let want = f32::atan2(away.x, away.z);
            let d = (want - o.yaw + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU) - std::f32::consts::PI;
            o.yaw += d * (dt * 3.0).min(1.0);
        }
    }
    let focus = p.translation + Vec3::Y * 1.2;
    let dir = Vec3::new(o.yaw.sin() * o.pitch.cos(), o.pitch.sin(), o.yaw.cos() * o.pitch.cos());
    let dist = collider.and_then(|c| c.0.raycast_m(focus, focus + dir * o.distance)).map_or(o.distance, |hit| (hit - 0.25).max(0.4));
    *t = Transform::from_translation(focus + dir * dist).looking_at(focus, Vec3::Y);
}
