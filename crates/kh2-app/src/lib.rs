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

pub mod abilities;
mod plugins;
mod area;
mod area_jump;
mod asset_viewer;
mod assets;
mod audio;
mod audio_bgm;
mod camera;
mod clash;
mod combat;
mod data;
mod drive;
mod enemy;
mod enemy_ai;
mod enemy_host;
mod enemy_obj;
mod friend_ai;
mod friend_host;
mod fx;
mod harness;
mod hud;
mod intro;
mod items;
mod limit;
mod magic;
mod magic_rules;
mod menu;
mod menu_abilities;
mod menu_ending;
mod menu_gfx;
mod menu_journal;
mod menu_pages;
mod menu_shop;
mod menu_status;
mod menu_struggle;
mod menu_text;
mod motor;
mod npc;
mod party;
mod party_battle;
mod party_rules;
mod player_reaction;
mod prize;
mod progress;
mod reaction_cmd;
mod reaction_parts;
mod reaction_rules;
mod savepoint;
mod schedule;
mod skel;
mod sora;
mod sound_test;
mod struggle;
mod summon;
mod texanim;
mod world;

use std::sync::OnceLock;
use std::time::Instant;

use bevy::prelude::*;

/// KH2 units are centimetres; Bevy scene units are metres.
pub const SCALE: f32 = 0.01;

static CLOCK: OnceLock<Instant> = OnceLock::new();

/// Prints a startup timing line: time since the clock started in `main`.
pub fn startup_log(stage: &str) {
    eprintln!("[startup] {stage} {:?}", CLOCK.get_or_init(Instant::now).elapsed());
}

/// Builds the Bevy application and runs it: the disc comes from `KH2_ISO`, the first command-line
/// argument names the start area (default `tt04`).
pub fn run() {
    let iso = assets::iso_path().unwrap_or_else(|e| {
        eprintln!("{e}");
        std::process::exit(1);
    });
    let start = std::env::args().nth(1).unwrap_or_else(|| "tt04".into());
    CLOCK.get_or_init(Instant::now);
    App::new()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window { title: "kh2-rs".into(), ..default() }),
            ..default()
        }))
        .add_plugins(plugins::GamePlugins { iso, start })
        .run();
}
