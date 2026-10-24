//! kh2-rs game. Everything on screen is read from the user's own disc at startup.
//!
//! Controls: WASD move (camera-relative), Ctrl walk, Space jump, LMB/J attack (combo), K/E guard
//! and growth abilities (Quick Run, Glide), Q/Tab lock-on, RMB drag / arrows orbit the camera,
//! wheel zoom, 1-9/0 switch Twilight Town areas, F1 motion viewer ([ ] to step), F3 spawn
//! enemies, F4 show hit volumes, F6 cycle growth-ability levels. Command menu: T / G move the
//! cursor, B goes back, J executes the selected command (Magic opens the spell list).
//!
//! Gameplay runs in `FixedUpdate` at the game's 60 Hz logic rate. Each feature module has a
//! plugin that registers its own resources, messages and systems; the order between features is
//! in `schedule`.

pub use kh2_game_core::*;
pub use kh2_sound::*;
pub use kh2_enemy::*;
pub use kh2_player::*;
pub use kh2_party::*;
pub use kh2_event::*;
pub use kh2_hud::*;
pub use kh2_menu::*;
pub use kh2_worldmap::*;

mod plugins;
mod asset_viewer;
mod sound_test;

use bevy::prelude::*;

/// Builds the Bevy application and runs it: the disc comes from `KH2_ISO`, the first command-line
/// argument names the start area (default `tt04`).
pub fn run() {
    let iso = assets::iso_path().unwrap_or_else(|e| {
        eprintln!("{e}");
        std::process::exit(1);
    });
    let start = std::env::args().nth(1).unwrap_or_else(|| "tt04".into());
    // A rig state file (`place` key) names the start area and the entrance.
    let start = kh2_game_core::rig::start_area(start);
    kh2_game_core::start_clock();
    // The window opens unfocused and below every other window, so background runs never cover the
    // user's screen. KH2_FRONT=1 (set by tools/playtest.bat) restores a normal window.
    let front = std::env::var_os("KH2_FRONT").is_some();
    let level = if front { bevy::window::WindowLevel::Normal } else { bevy::window::WindowLevel::AlwaysOnBottom };
    let turbo = kh2_game_core::harness::turbo();
    if turbo {
        // Turbo runs are silent: no sound device and no music (the audio scenario stays real time).
        // SAFETY: still single-threaded here, before the app (and any thread that reads the environment) starts.
        unsafe {
            std::env::set_var("KH2_NO_SOUND", "1");
            std::env::set_var("KH2_NO_MUSIC", "1");
        }
    }
    let no_vsync = turbo || std::env::var("KH2_VSYNC").is_ok_and(|v| v.trim() == "0");
    let present_mode = if no_vsync { bevy::window::PresentMode::AutoNoVsync } else { bevy::window::PresentMode::AutoVsync };
    let mut app = App::new();
    app.add_plugins(DefaultPlugins.set(WindowPlugin {
        primary_window: Some(Window { title: "kh2-rs".into(), focused: front, window_level: level, present_mode, ..default() }),
        ..default()
    }));
    if turbo {
        kh2_game_core::harness::configure_turbo(&mut app);
    }
    if !turbo {
        app.add_systems(Update, window_modes);
    }
    app.add_plugins(plugins::GamePlugins { iso, start }).run();
}

/// End toggles uncapped rendering (`KH2_VSYNC=0` starts it). One title state machine: the title names the vsync mode while it is off
/// and the field rate for 2 seconds after Home (Home is `schedule::toggle_field_hz`; no other handler reads either key).
fn window_modes(
    keys: Res<ButtonInput<KeyCode>>,
    clock: Res<kh2_game_core::schedule::FrameClock>,
    shown: Res<kh2_game_core::schedule::FieldHzShown>,
    mut windows: Query<&mut Window>,
) {
    use bevy::window::PresentMode::{AutoNoVsync, AutoVsync};
    let Ok(mut w) = windows.single_mut() else { return };
    if keys.just_pressed(KeyCode::End) {
        w.present_mode = if matches!(w.present_mode, AutoNoVsync) { AutoVsync } else { AutoNoVsync };
    }
    let mut title = String::from("kh2-rs");
    if matches!(w.present_mode, AutoNoVsync) {
        title.push_str(" [vsync off]");
    }
    if shown.0 > 0 {
        title.push_str(&format!(" [field {} Hz]", clock.field_hz()));
    }
    if w.title != title {
        w.title = title;
    }
}
