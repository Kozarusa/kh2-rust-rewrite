//! kh2-rs game. Everything on screen is read from the user's own disc at startup.
//!
//! Controls: WASD move (camera-relative), Space jump, Shift walk, mouse drag (right button) or
//! arrow keys orbit the camera, wheel / PageUp/PageDown zoom, 1-9 and 0 switch Twilight Town areas.

mod assets;
mod collision;
mod player;

use bevy::prelude::*;

/// KH2 units are centimetres; Bevy scene units are metres.
pub const SCALE: f32 = 0.01;

fn main() {
    let iso = assets::iso_path().unwrap_or_else(|e| {
        eprintln!("{e}");
        std::process::exit(1);
    });
    let start = std::env::args().nth(1).unwrap_or_else(|| "tt04".into());
    App::new()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window { title: "kh2-rs".into(), ..default() }),
            ..default()
        }))
        .insert_resource(ClearColor(Color::srgb(0.05, 0.05, 0.08)))
        .insert_resource(assets::Disc::open(&iso))
        .insert_resource(assets::CurrentArea { name: start, loaded: None })
        .add_systems(Startup, (player::setup, hud_setup))
        .add_systems(Update, auto_screenshot)
        .add_systems(
            Update,
            (assets::switch_area, assets::load_area, player::control, player::camera_follow, assets::sky_follow, hud_update)
                .chain(),
        )
        .run();
}

/// `KH2_SCREENSHOT=<file.png>`: save one frame shortly after start, then quit (for automated checks).
/// `KH2_YAW=<radians>` turns the camera first.
fn auto_screenshot(
    mut commands: Commands,
    time: Res<Time>,
    mut state: Local<u8>,
    mut cam: Query<&mut player::OrbitCamera>,
    mut keys: ResMut<ButtonInput<KeyCode>>,
) {
    let Ok(path) = std::env::var("KH2_SCREENSHOT") else { return };
    let t = time.elapsed_secs();
    // `KH2_WALK=1`: hold W from 3 s to 6 s, to exercise movement and collision.
    if std::env::var("KH2_WALK").is_ok() {
        if (3.0..6.0).contains(&t) {
            keys.press(KeyCode::KeyW);
        } else {
            keys.release(KeyCode::KeyW);
        }
    }
    if *state == 0 {
        if let (Ok(yaw), Ok(mut o)) = (std::env::var("KH2_YAW").unwrap_or_default().parse::<f32>(), cam.single_mut()) {
            o.yaw = yaw;
        }
        *state = 1;
    } else if *state == 1 && t > 8.0 {
        // Pipelines compile asynchronously; wait until the scene is actually drawn.
        use bevy::render::view::screenshot::{save_to_disk, Screenshot};
        commands.spawn(Screenshot::primary_window()).observe(save_to_disk(path));
        *state = 2;
    } else if *state == 2 && t > 11.0 {
        std::process::exit(0);
    }
}

#[derive(Component)]
struct Hud;

fn hud_setup(mut commands: Commands) {
    commands.spawn((
        Hud,
        Text::new(""),
        TextFont { font_size: bevy::text::FontSize::Px(16.0), ..default() },
        TextColor(Color::WHITE),
        Node { position_type: PositionType::Absolute, left: Val::Px(12.0), top: Val::Px(10.0), ..default() },
    ));
}

fn hud_update(area: Res<assets::CurrentArea>, player: Query<&Transform, With<player::Player>>, mut hud: Query<&mut Text, With<Hud>>) {
    let (Ok(p), Ok(mut text)) = (player.single(), hud.single_mut()) else { return };
    text.0 = format!(
        "kh2-rs  |  area {}  |  pos {:.1} {:.1} {:.1}\nWASD move  Space jump  Shift walk  RMB/arrows camera  wheel zoom  1-9,0 areas",
        area.name, p.translation.x, p.translation.y, p.translation.z
    );
}
