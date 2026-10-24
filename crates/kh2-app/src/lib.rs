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
mod config;
mod launcher;

use bevy::prelude::*;

/// The window settings that are remembered between runs, tracked while the game runs and saved when it exits.
#[derive(Resource)]
struct WindowMemory {
    dir: std::path::PathBuf,
    config: config::Config,
}

/// Writes the remembered window settings when the game exits.
fn save_window_settings(mut exits: MessageReader<AppExit>, memory: Res<WindowMemory>) {
    if exits.read().next().is_some() {
        let mut cfg = config::Config::load(&memory.dir);
        cfg.width = memory.config.width;
        cfg.height = memory.config.height;
        cfg.fullscreen = memory.config.fullscreen;
        if let Err(e) = cfg.save(&memory.dir) {
            eprintln!("could not save {}: {e}", memory.dir.display());
        }
    }
}

/// Builds the Bevy application and runs it. The disc is found by [`launcher::discover`]; the first argument that is
/// not a flag names the start area (default `tt04`). See [`launcher::HELP`] for the flags.
pub fn run() {
    let opts = launcher::parse_args(std::env::args().skip(1));
    // The release build is a windows-subsystem program: it has no console unless asked for.
    let release = cfg!(not(debug_assertions));
    let dir = config::config_dir();
    if opts.help {
        if !release || launcher::attach_console(false) {
            print!("{}", launcher::HELP);
        } else {
            launcher::info_box("kh2-rs help", launcher::HELP);
        }
        return;
    }
    if release {
        if opts.console {
            launcher::attach_console(true);
        } else {
            launcher::redirect_log(&dir);
        }
    }
    let env_iso = std::env::var_os("KH2_ISO").map(std::path::PathBuf::from);
    let local_txt = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../local/iso.txt");
    let iso = launcher::discover(env_iso, &dir, &local_txt, opts.choose_disc, &mut |m| launcher::message_box("kh2-rs", m), &mut launcher::pick_disc_dialog).unwrap_or_else(|e| launcher::fail(&e));
    if release && assets::names_file().is_none() {
        launcher::info_box(
            "kh2-rs: name list missing",
            "The file kh2idx.txt (a list of file names used to find the maps) was not found, so some maps will not load.

Put the file in the same folder as kh2-game.exe, or in %APPDATA%\\kh2-rs. The game starts without it.",
        );
    }
    let start = opts.area.unwrap_or_else(|| "tt04".into());
    // A rig state file (`place` key) names the start area and the entrance.
    let start = kh2_game_core::rig::start_area(start);
    kh2_game_core::start_clock();
    // Harness runs (the debug exe) open the window unfocused and below every other window, so background runs never
    // cover the user's screen. KH2_FRONT=1 (set by tools/playtest.bat) restores a normal window; the release build,
    // which a player starts, and the window flags always do.
    let front = std::env::var_os("KH2_FRONT").is_some() || release || opts.windowed || opts.fullscreen;
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
    // The remembered window size and fullscreen apply to the release build and the window flags, never to harness runs.
    let remember = release || opts.windowed || opts.fullscreen;
    let saved = if remember { config::Config::load(&dir) } else { config::Config::default() };
    let fullscreen = if opts.fullscreen { true } else if opts.windowed { false } else { saved.fullscreen.unwrap_or(false) };
    let mut window = Window { title: "kh2-rs".into(), focused: front, window_level: level, present_mode, ..default() };
    if let (Some(w), Some(h)) = (saved.width, saved.height) {
        window.resolution = bevy::window::WindowResolution::new(w.clamp(320, 7680), h.clamp(240, 4320));
    }
    if fullscreen {
        window.mode = bevy::window::WindowMode::BorderlessFullscreen(bevy::window::MonitorSelection::Primary);
    }
    let mut app = App::new();
    app.add_plugins(DefaultPlugins.set(WindowPlugin { primary_window: Some(window), ..default() }));
    if remember {
        app.insert_resource(WindowMemory { dir, config: saved }).add_systems(Last, save_window_settings);
    }
    if turbo {
        kh2_game_core::harness::configure_turbo(&mut app);
    }
    if !turbo {
        app.add_systems(Update, window_modes);
    }
    app.add_plugins(plugins::GamePlugins { iso, start }).run();
}

/// The one window system: F11 and Alt+Enter toggle borderless fullscreen and the size is noted for the config file (when
/// the settings are remembered); Numpad - toggles uncapped rendering (`KH2_VSYNC=0` starts it). One title state machine: the title names the vsync mode while it is off
/// and the field rate for 2 seconds after Numpad + (Numpad + is `schedule::toggle_field_hz`; no other handler reads either key).
fn window_modes(
    keys: Res<ButtonInput<KeyCode>>,
    clock: Res<kh2_game_core::schedule::FrameClock>,
    shown: Res<kh2_game_core::schedule::FieldHzShown>,
    mut memory: Option<ResMut<WindowMemory>>,
    mut windows: Query<&mut Window, With<bevy::window::PrimaryWindow>>,
) {
    use bevy::window::PresentMode::{AutoNoVsync, AutoVsync};
    let Ok(mut w) = windows.single_mut() else { return };
    let alt_enter = keys.just_pressed(KeyCode::Enter) && (keys.pressed(KeyCode::AltLeft) || keys.pressed(KeyCode::AltRight));
    if keys.just_pressed(KeyCode::F11) || alt_enter {
        w.mode = if matches!(w.mode, bevy::window::WindowMode::Windowed) {
            bevy::window::WindowMode::BorderlessFullscreen(bevy::window::MonitorSelection::Current)
        } else {
            bevy::window::WindowMode::Windowed
        };
    }
    if let Some(memory) = memory.as_mut() {
        let full = !matches!(w.mode, bevy::window::WindowMode::Windowed);
        memory.config.fullscreen = Some(full);
        if !full {
            memory.config.width = Some(w.width().round() as u32);
            memory.config.height = Some(w.height().round() as u32);
        }
    }
    if keys.just_pressed(KeyCode::NumpadSubtract) {
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
