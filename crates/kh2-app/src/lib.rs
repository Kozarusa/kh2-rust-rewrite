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
    let present_mode = if turbo { bevy::window::PresentMode::AutoNoVsync } else { bevy::window::PresentMode::default() };
    let mut app = App::new();
    app.add_plugins(DefaultPlugins.set(WindowPlugin {
        primary_window: Some(Window { title: "kh2-rs".into(), focused: front, window_level: level, present_mode, ..default() }),
        ..default()
    }));
    if turbo {
        kh2_game_core::harness::configure_turbo(&mut app);
    }
    app.add_plugins(plugins::GamePlugins { iso, start }).run();
}
