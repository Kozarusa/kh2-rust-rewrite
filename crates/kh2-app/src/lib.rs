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

pub mod abilities;
mod plugins;
mod area_jump;
mod asset_viewer;
mod clash;
mod drive;
mod enemy;
mod enemy_ai;
mod enemy_host;
mod enemy_obj;
mod friend_ai;
mod friend_host;
mod hud;
mod intro;
mod items;
mod limit;
mod limit_camera;
mod limit_script;
mod limit_visual;
mod magic;
mod menu;
mod menu_abilities;
mod menu_ending;
mod menu_gfx;
mod menu_journal;
mod menu_pages;
mod menu_shop;
mod menu_status;
mod menu_gameover;
mod mickey;
mod menu_struggle;
mod menu_text;
mod npc;
mod party;
mod party_battle;
mod party_switch;
mod player_reaction;
mod prize;
mod reaction_cmd;
mod savepoint;
mod sora;
mod sound_test;
mod struggle;
mod summon;
mod worldmap;
mod worldmap_ui;

use bevy::prelude::*;

/// Builds the Bevy application and runs it: the disc comes from `KH2_ISO`, the first command-line
/// argument names the start area (default `tt04`).
pub fn run() {
    let iso = assets::iso_path().unwrap_or_else(|e| {
        eprintln!("{e}");
        std::process::exit(1);
    });
    let start = std::env::args().nth(1).unwrap_or_else(|| "tt04".into());
    kh2_game_core::start_clock();
    // The window opens unfocused and below every other window, so background runs never cover the
    // user's screen. KH2_FRONT=1 (set by tools/playtest.bat) restores a normal window.
    let front = std::env::var_os("KH2_FRONT").is_some();
    let level = if front { bevy::window::WindowLevel::Normal } else { bevy::window::WindowLevel::AlwaysOnBottom };
    App::new()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window { title: "kh2-rs".into(), focused: front, window_level: level, ..default() }),
            ..default()
        }))
        .add_plugins(plugins::GamePlugins { iso, start })
        .run();
}
