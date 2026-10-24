//! Automated-run harness: the frame-rate log and the scripted screenshot run. Its log lines keep
//! the `kh2_game` target they had when this code lived in `main.rs`.

use bevy::prelude::*;

use crate::camera;

pub struct HarnessPlugin;

/// `KH2_TURBO=1`: the app runs as fast as the CPU allows with one fixed tick per frame (offscreen-style
/// runs of the acceptance sweep). The screenshot and key scripts then count whole ticks.
pub fn turbo() -> bool {
    std::env::var_os("KH2_TURBO").is_some_and(|v| v != "0")
}

/// Fixed steps (vblanks) run so far (counted in `FixedFirst`, only when turbo is on).
#[derive(Resource, Default, Debug, Clone, Copy, PartialEq, Eq)]
pub struct TurboTicks(pub u64);

/// The turbo settings: every frame advances `Time` by exactly one fixed step (so `FixedUpdate` runs
/// once per frame and the scripted seconds of `KH2_KEYS` / `KH2_SHOT_AT` become exact tick counts),
/// winit keeps updating without focus, and the tick counter runs.
pub fn configure_turbo(app: &mut App) {
    app.insert_resource(bevy::time::TimeUpdateStrategy::FixedTimesteps(1))
        .insert_resource(bevy::winit::WinitSettings::continuous())
        .init_resource::<TurboTicks>()
        .add_systems(FixedFirst, count_turbo_ticks);
}

fn count_turbo_ticks(mut ticks: ResMut<TurboTicks>) {
    ticks.0 += 1;
}

/// `KH2_YAW` / `KH2_DIST` written into the camera component (the camera's tick takes a changed value as a new setting).
fn pin_camera(cam: &mut Query<&mut camera::OrbitCamera>) {
    if let (Ok(yaw), Ok(mut o)) = (std::env::var("KH2_YAW").unwrap_or_default().parse::<f32>(), cam.single_mut()) {
        o.yaw = yaw;
    }
    if let (Ok(d), Ok(mut o)) = (std::env::var("KH2_DIST").unwrap_or_default().parse::<f32>(), cam.single_mut()) {
        o.distance = d;
    }
}

/// The pin runs before every logic tick (`FixedUpdate`, first), as it ran before every 60 Hz tick when it lived in `PreUpdate` (one
/// tick per turbo frame). Since WP-L1-P4 a field frame is two ticks every second vblank: pinned in `PreUpdate`, the second tick
/// would turn the camera freely, and the frames without logic would hand the pad read (`Update`) the pinned yaw instead of the
/// camera's, so the scripted runs would take another path.
fn pin_camera_tick(time: Res<Time>, mut cam: Query<&mut camera::OrbitCamera>) {
    let shot_at: f32 = std::env::var("KH2_SHOT_AT").ok().and_then(|v| v.parse().ok()).unwrap_or(8.0);
    if std::env::var_os("KH2_SCREENSHOT").is_some() && time.elapsed_secs() < shot_at {
        pin_camera(&mut cam);
    }
}

/// The whole tick of the scripted clock (`Time` in `PreUpdate` is the virtual clock).
fn clock_tick(time: &Time) -> u64 {
    (time.elapsed().as_secs_f64() * f64::from(crate::data::LOGIC_HZ)).round() as u64
}

/// `KH2_SHOT_TICKS=k1,k2,...`: writes `shot-<k>.png` (next to `KH2_SCREENSHOT`, else in the working
/// directory) when the clock reaches tick k, and exits 60 ticks after the last one.
fn tick_shots(mut commands: Commands, time: Res<Time>, mut taken: Local<usize>) {
    let Ok(spec) = std::env::var("KH2_SHOT_TICKS") else { return };
    let mut ticks: Vec<u64> = spec.split(',').filter_map(|v| v.trim().parse().ok()).collect();
    ticks.sort_unstable();
    let Some(&last) = ticks.last() else { return };
    let now = clock_tick(&time);
    if let Some(&k) = ticks.get(*taken) {
        if now >= k {
            use bevy::render::view::screenshot::{save_to_disk, Screenshot};
            let dir = std::env::var_os("KH2_SCREENSHOT")
                .and_then(|p| std::path::Path::new(&p).parent().map(|d| d.to_path_buf()))
                .unwrap_or_default();
            info!(target: "kh2_game", "taking screenshot at tick {now}");
            commands.spawn(Screenshot::primary_window()).observe(save_to_disk(dir.join(format!("shot-{k}.png"))));
            *taken += 1;
        }
    } else if now >= last + 60 {
        std::process::exit(0);
    }
}

/// The scripted key presses of `KH2_KEYS` (the menu's input runs after this set).
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Keys;

impl Plugin for HarnessPlugin {
    fn build(&self, app: &mut App) {
        // The scripted keys are pressed in `PreUpdate`, after the keyboard is read, so the pause menu
        // (which takes its keys there) sees them like real ones.
        app.add_systems(PreUpdate, auto_screenshot.in_set(Keys).after(bevy::input::InputSystems));
        app.add_systems(PreUpdate, tick_shots.after(bevy::input::InputSystems));
        app.add_systems(FixedUpdate, pin_camera_tick.before(crate::schedule::Sim::Weapon));
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
                // The Save menu (docs/systems/ui/menus-save.md 18): the app has no save point object, so F2 stands for the field Save command.
                "F2" => KeyCode::F2,
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
                // The camp's list results -5 (switch) and -7 (help): the face button X and Select.
                "X" => KeyCode::KeyX,
                "Select" | "Tab" => KeyCode::Tab,
                "[" => KeyCode::BracketLeft,
                "]" => KeyCode::BracketRight,
                // The sound viewer (F10) and the asset viewer (F11) and their paging keys.
                "F10" => KeyCode::F10,
                "F11" => KeyCode::F11,
                "PageUp" => KeyCode::PageUp,
                "PageDown" => KeyCode::PageDown,
                "Shift" => KeyCode::ShiftLeft,
                _ => continue,
            };
            if turbo() {
                // Whole ticks: the press frame is the tick nearest `at`, the release `hold` later (at least one tick).
                let hz = f64::from(crate::data::LOGIC_HZ);
                let press = (f64::from(at) * hz).round() as u64;
                let release = press + ((f64::from(hold) * hz).round() as u64).max(1);
                let now = clock_tick(&time);
                if now == press {
                    keys.press(key);
                } else if now == release {
                    keys.release(key);
                }
            } else if t >= at && t - dt < at {
                keys.press(key);
            } else if t >= at + hold && t - dt < at + hold {
                keys.release(key);
            }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn turbo_runs_exactly_one_fixed_update_per_frame() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins).insert_resource(Time::<Fixed>::from_hz(f64::from(crate::data::LOGIC_HZ)));
        configure_turbo(&mut app);
        app.finish();
        app.cleanup();
        for _ in 0..3 {
            app.update();
        }
        let base = app.world().resource::<TurboTicks>().0;
        for _ in 0..500 {
            app.update();
        }
        assert_eq!(app.world().resource::<TurboTicks>().0 - base, 500);
    }

    #[test]
    fn clock_tick_counts_whole_fixed_steps() {
        let mut time = Time::<()>::default();
        for n in 1..=7u64 {
            time.advance_by(std::time::Duration::from_secs_f64(1.0 / 60.0));
            assert_eq!(clock_tick(&time), n);
        }
    }
}
