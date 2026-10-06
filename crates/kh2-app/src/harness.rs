//! Automated-run harness: the frame-rate log and the scripted screenshot run. Its log lines keep
//! the `kh2_game` target they had when this code lived in `main.rs`.

use bevy::prelude::*;

use crate::camera;

pub struct HarnessPlugin;

/// The scripted key presses of `KH2_KEYS` (the menu's input runs after this set).
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Keys;

impl Plugin for HarnessPlugin {
    fn build(&self, app: &mut App) {
        // The scripted keys are pressed in `PreUpdate`, after the keyboard is read, so the pause menu
        // (which takes its keys there) sees them like real ones.
        app.add_systems(PreUpdate, auto_screenshot.in_set(Keys).after(bevy::input::InputSystems));
        // `KH2_FPS=1`: log frame rate every 2 s.
        if std::env::var("KH2_FPS").is_ok() {
            app.add_plugins((
                bevy::diagnostic::FrameTimeDiagnosticsPlugin::default(),
                bevy::diagnostic::LogDiagnosticsPlugin { wait_duration: std::time::Duration::from_secs(2), ..default() },
            ));
        }
    }
}

/// `KH2_SCREENSHOT=<file.png>`: save one frame after 8 s, then quit (for automated checks).
/// `KH2_YAW=<radians>` turns the camera; `KH2_WALK=1` holds W from 3-6 s;
/// `KH2_KEYS=<t:key[:hold],...>` presses keys at times for `hold` seconds (default 0.05),
/// e.g. `4:J,4.3:J,4:Space:0.6`.
fn auto_screenshot(
    mut commands: Commands,
    time: Res<Time>,
    mut state: Local<u8>,
    mut cam: Query<&mut camera::OrbitCamera>,
    mut keys: ResMut<ButtonInput<KeyCode>>,
) {
    let Ok(path) = std::env::var("KH2_SCREENSHOT") else { return };
    let t = time.elapsed_secs();
    if (t * 2.0).floor() != ((t - time.delta_secs()) * 2.0).floor() {
        info!(target: "kh2_game", "t={t:.1} state={}", *state);
    }
    let shot_at: f32 = std::env::var("KH2_SHOT_AT").ok().and_then(|v| v.parse().ok()).unwrap_or(8.0);
    if std::env::var("KH2_WALK").is_ok() {
        if (3.0..6.0).contains(&t) {
            keys.press(KeyCode::KeyW);
        } else {
            keys.release(KeyCode::KeyW);
        }
    }
    if let Ok(spec) = std::env::var("KH2_KEYS") {
        let dt = time.delta_secs();
        for item in spec.split(',') {
            let mut parts = item.split(':');
            let (Some(at), Some(k)) = (parts.next(), parts.next()) else { continue };
            let hold: f32 = parts.next().and_then(|h| h.parse().ok()).unwrap_or(0.05);
            let Ok(at) = at.parse::<f32>() else { continue };
            let key = match k {
                "J" => KeyCode::KeyJ,
                "K" => KeyCode::KeyK,
                "Q" => KeyCode::KeyQ,
                "Space" => KeyCode::Space,
                "F3" => KeyCode::F3,
                "F4" => KeyCode::F4,
                "W" => KeyCode::KeyW,
                "E" => KeyCode::KeyE,
                "F6" => KeyCode::F6,
                "A" => KeyCode::KeyA,
                "S" => KeyCode::KeyS,
                "D" => KeyCode::KeyD,
                "C" => KeyCode::KeyC,
                "Left" => KeyCode::ArrowLeft,
                "Right" => KeyCode::ArrowRight,
                "Up" => KeyCode::ArrowUp,
                "Down" => KeyCode::ArrowDown,
                "Enter" => KeyCode::Enter,
                "Escape" => KeyCode::Escape,
                "Backspace" => KeyCode::Backspace,
                "LB" => KeyCode::BracketLeft,
                "RB" => KeyCode::BracketRight,
                "Y" => KeyCode::KeyY,
                "[" => KeyCode::BracketLeft,
                "]" => KeyCode::BracketRight,
                _ => continue,
            };
            if t >= at && t - dt < at {
                keys.press(key);
            } else if t >= at + hold && t - dt < at + hold {
                keys.release(key);
            }
        }
    }
    if t < shot_at {
        if let (Ok(yaw), Ok(mut o)) = (std::env::var("KH2_YAW").unwrap_or_default().parse::<f32>(), cam.single_mut()) {
            o.yaw = yaw;
        }
        if let (Ok(d), Ok(mut o)) = (std::env::var("KH2_DIST").unwrap_or_default().parse::<f32>(), cam.single_mut()) {
            o.distance = d;
        }
    }
    if *state == 0 {
        *state = 1;
    } else if *state == 1 && t > shot_at {
        use bevy::render::view::screenshot::{save_to_disk, Screenshot};
        info!(target: "kh2_game", "taking screenshot at t={t:.2}");
        commands.spawn(Screenshot::primary_window()).observe(save_to_disk(path));
        *state = 2;
    } else if *state == 2 && t > shot_at + 3.0 {
        std::process::exit(0);
    }
}
