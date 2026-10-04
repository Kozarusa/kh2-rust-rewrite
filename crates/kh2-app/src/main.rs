//! kh2-rs game. Everything on screen is read from the user's own disc at startup.
//!
//! Controls: WASD move (camera-relative), Ctrl walk, Space jump, LMB/J attack (combo), K/E guard
//! and growth abilities (Quick Run, Glide), Q/Tab lock-on, RMB drag / arrows orbit the camera,
//! wheel zoom, 1-9/0 switch Twilight Town areas, F1 motion viewer ([ ] to step), F3 spawn
//! enemies, F4 show hit volumes, F6 cycle growth-ability levels.
//!
//! Gameplay runs in `FixedUpdate` at the game's 60 Hz logic rate.

mod assets;
mod audio;
mod camera;
mod combat;
mod data;
mod enemy;
mod fx;
mod hud;
mod motor;
mod skel;
mod sora;
mod world;

use std::sync::Arc;

use bevy::prelude::*;

/// KH2 units are centimetres; Bevy scene units are metres.
pub const SCALE: f32 = 0.01;

fn rig(disc: &assets::Disc, model: &str, mset: &str) -> Arc<skel::Rig> {
    let m = disc.read(model).and_then(|d| kh2_model::mdlx(&d)).unwrap_or_else(|| panic!("{model}: cannot load"));
    let s = disc.read(mset).and_then(|d| kh2_anim::Mset::parse(&d)).unwrap_or_else(|| panic!("{mset}: cannot load"));
    let apdx = model.strip_suffix(".mdlx").map(|b| format!("{b}.apdx")).filter(|a| disc.read(a).is_some());
    Arc::new(skel::Rig { model: m, mset: s, apdx })
}

fn main() {
    let iso = assets::iso_path().unwrap_or_else(|e| {
        eprintln!("{e}");
        std::process::exit(1);
    });
    let start = std::env::args().nth(1).unwrap_or_else(|| "tt04".into());
    let clock = std::time::Instant::now();
    let disc = assets::Disc::open(&iso);
    eprintln!("[startup] disc {:?}", clock.elapsed());
    let game_data = data::GameData::load(&disc);
    eprintln!("[startup] tables {:?}", clock.elapsed());
    let rigs = sora::Rigs {
        sora: rig(&disc, "obj/P_EX100.mdlx", "obj/P_EX100.mset"),
        keyblade: rig(&disc, "obj/W_EX010.mdlx", "obj/W_EX010.mset"),
        shadow: rig(&disc, "obj/M_EX020.mdlx", "obj/M_EX020.mset"),
        soldier: rig(&disc, "obj/M_EX010.mdlx", "obj/M_EX010.mset"),
    };
    eprintln!("[startup] rigs {:?}", clock.elapsed());
    App::new()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window { title: "kh2-rs".into(), ..default() }),
            ..default()
        }))
        .add_message::<combat::Hit>()
        .add_message::<combat::WallHit>()
        .add_message::<hud::DamagePopup>()
        .add_message::<fx::StartEffect>()
        .insert_resource(ClearColor(Color::srgb(0.05, 0.05, 0.08)))
        .insert_resource(disc)
        .insert_resource(game_data)
        .insert_resource(rigs)
        .insert_resource(enemy::Waves::default())
        .insert_resource(assets::CurrentArea { name: start, loaded: None })
        .insert_resource(Time::<Fixed>::from_hz(data::LOGIC_HZ as f64))
        .init_resource::<sora::Pad>()
        .add_systems(Startup, (sora::setup, camera::setup, hud::setup, fx::setup))
        .add_systems(Update, (assets::switch_area, assets::load_area, sora::read_pad).chain())
        .add_systems(
            FixedUpdate,
            (
                sora::sync_weapon,
                skel::animate,
                sora::follow_hand,
                sora::tick,
                enemy::spawn_waves,
                enemy::think,
                enemy::separate,
                combat::detect_hits,
                sora::receive_hits,
                sora::receive_wall_hits,
                enemy::receive_hits,
                fx::weapon_trails,
                fx::motion_effects,
                fx::start_requests,
                fx::hit_sparks,
                fx::fx_test,
                fx::effect_sounds,
                audio::motion_sounds,
                audio::wall_sounds,
                fx::tick,
            )
                .chain(),
        )
        .add_systems(
            Update,
            (camera::follow, assets::sky_follow, hud::update, hud::popups, combat::draw_volumes).chain().after(sora::read_pad),
        )
        .add_systems(Update, auto_screenshot)
        .add_plugins(FpsLog)
        .add_plugins(audio::AudioPlugin)
        .add_systems(Update, audio::area_bgm.after(assets::load_area))
        .run();
}

/// `KH2_FPS=1`: log frame rate every 2 s.
struct FpsLog;

impl Plugin for FpsLog {
    fn build(&self, app: &mut App) {
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
        info!("t={t:.1} state={}", *state);
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
        info!("taking screenshot at t={t:.2}");
        commands.spawn(Screenshot::primary_window()).observe(save_to_disk(path));
        *state = 2;
    } else if *state == 2 && t > shot_at + 3.0 {
        std::process::exit(0);
    }
}
