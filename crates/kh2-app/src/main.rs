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

mod abilities;
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
mod fx;
mod harness;
mod hud;
mod items;
mod limit;
mod magic;
mod magic_rules;
mod menu;
mod menu_abilities;
mod menu_gfx;
mod menu_pages;
mod menu_status;
mod menu_text;
mod motor;
mod party;
mod party_rules;
mod player_reaction;
mod prize;
mod progress;
mod reaction_cmd;
mod reaction_parts;
mod reaction_rules;
mod schedule;
mod skel;
mod sora;
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

fn main() {
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
        // Order matters only where noted: `AssetsPlugin` first (it provides `Disc`, which
        // `SoraPlugin` reads when it builds); the rest follow the order systems were first
        // registered in, which keeps the schedules' tie-breaking between unordered systems.
        .add_plugins((
            schedule::SchedulePlugin,
            assets::AssetsPlugin { iso, start },
            sora::SoraPlugin,
            camera::CameraPlugin,
            skel::SkelPlugin,
            combat::CombatPlugin,
            enemy::EnemyPlugin,
            enemy_ai::EnemyAiPlugin,
        ))
        .add_plugins((
            hud::HudPlugin,
            fx::FxPlugin,
            harness::HarnessPlugin,
            audio::AudioPlugin,
            abilities::AbilitiesPlugin,
            drive::DrivePlugin,
            limit::LimitPlugin,
            reaction_cmd::ReactionPlugin,
            clash::ClashPlugin,
            player_reaction::PlayerReactionPlugin,
            magic::MagicPlugin,
            texanim::TexAnimPlugin,
        ))
        // The third tuple: later plugins go here (the second is near the 15-entry limit).
        // `progress::ensure` lets the plugins above that read `Progress` while they build insert it
        // first, so its place here does not matter.
        .add_plugins((progress::ProgressPlugin, prize::PrizePlugin, menu::MenuPlugin, menu_abilities::MenuAbilitiesPlugin, items::ItemsPlugin, party::PartyPlugin, struggle::StrugglePlugin, summon::SummonPlugin))
        .run();
}
