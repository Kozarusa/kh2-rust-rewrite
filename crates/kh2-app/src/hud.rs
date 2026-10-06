//! HUD: the game's own 2D elements (Sora's HP/MP gauges, the target gauge, the lock-on reticle),
//! drawn from the field sequence file on the disc; see `hud_gfx`, `hud_gauge`, `hud_target` and
//! docs/hud.md. The developer text overlay and the floating damage numbers are not part of the
//! original; they show only with `KH2_DEBUGHUD=1` or after F9.

use bevy::prelude::*;

use crate::assets::{CurrentArea, Disc};
use crate::enemy::Enemy;
use crate::schedule::Frame;
use crate::skel::Animator;
use crate::sora::{Sora, State};

// The HUD's own files sit beside this one; they are submodules so `main.rs` stays untouched.
#[path = "hud_gauge.rs"]
mod hud_gauge;
#[path = "hud_gfx.rs"]
mod hud_gfx;
#[path = "hud_target.rs"]
mod hud_target;
#[path = "hud_banner.rs"]
mod hud_banner;
#[path = "hud_menu.rs"]
pub(crate) mod hud_menu;
#[path = "hud_text.rs"]
mod hud_text;
#[path = "hud_card.rs"]
mod hud_card;
#[path = "hud_party.rs"]
pub(crate) mod hud_party;
#[path = "hud_msgwin.rs"]
pub(crate) mod hud_msgwin;

use hud_gauge::{DriveGauge, DriveTables, FaceFile, MpRecharge, SoraPanel};
use hud_gfx::{HudGfxPlugin, HudScene};
use hud_banner::{BannerNodes, HudBanner, MunnyPopups, NumberStrips, StripNodes};
use hud_card::{AreaCard, CardNodes};
use hud_menu::{CommandMenu, MenuFile, MenuNodes, MenuTables};
use hud_msgwin::{MsgWindow, WindowNodes};
use hud_party::{PartyMembers, PartyNodes};
use hud_target::{TargetBubble, TargetHud};

pub struct HudPlugin;

impl Plugin for HudPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(HudGfxPlugin)
            .add_message::<DamagePopup>()
            .insert_resource(DebugHud(std::env::var("KH2_DEBUGHUD").is_ok_and(|v| v != "0")))
            .init_resource::<SoraPanel>()
            .init_resource::<TargetHud>()
            .init_resource::<TargetBubble>()
            .init_resource::<FaceFile>()
            .init_resource::<DriveGauge>()
            .init_resource::<MpRecharge>()
            .init_resource::<DriveTables>()
            .init_resource::<HudBanner>()
            .init_resource::<MunnyPopups>()
            .init_resource::<BannerNodes>()
            .init_resource::<CommandMenu>()
            .init_resource::<MenuFile>()
            .init_resource::<MenuTables>()
            .init_resource::<MenuNodes>()
            .init_resource::<NumberStrips>()
            .init_resource::<StripNodes>()
            .init_resource::<AreaCard>()
            .init_resource::<CardNodes>()
            .init_resource::<PartyMembers>()
            .init_resource::<PartyNodes>()
            .init_resource::<MsgWindow>()
            .init_resource::<WindowNodes>()
            .add_systems(Startup, (setup, load_files).chain())
            .add_systems(
                Update,
                (
                    toggle_debug,
                    (
                        hud_gauge::update,
                        hud_party::update,
                        hud_target::update,
                        hud_menu::mirror_reaction,
                        hud_menu::update,
                        hud_banner::update,
                        hud_banner::update_strips,
                        hud_card::update,
                        hud_msgwin::update,
                        hud_text::demo,
                    )
                        .chain(),
                    debug_text.run_if(debug_on),
                )
                    .in_set(Frame::Hud),
            )
            .add_systems(Update, popups.in_set(Frame::Popups).run_if(debug_on));
    }
}

/// Developer overlay switch.
#[derive(Resource)]
pub struct DebugHud(pub bool);

fn debug_on(d: Res<DebugHud>) -> bool {
    d.0
}

fn toggle_debug(keys: Res<ButtonInput<KeyCode>>, mut d: ResMut<DebugHud>, mut text: Query<&mut Visibility, With<HudText>>) {
    if keys.just_pressed(KeyCode::F9) {
        d.0 = !d.0;
    }
    if let Ok(mut v) = text.single_mut() {
        *v = if d.0 { Visibility::Inherited } else { Visibility::Hidden };
    }
}

#[derive(Message, Clone, Copy)]
pub struct DamagePopup {
    pub at: Vec3,
    pub amount: i32,
}

#[derive(Component)]
pub struct HudText;
#[derive(Component)]
pub struct Popup {
    at: Vec3,
    t: f32,
}

fn setup(mut commands: Commands, debug: Res<DebugHud>) {
    commands.spawn((
        HudText,
        Text::new(""),
        TextFont { font_size: bevy::text::FontSize::Px(15.0), ..default() },
        TextColor(Color::WHITE),
        Node { position_type: PositionType::Absolute, left: Val::Px(12.0), top: Val::Px(10.0), ..default() },
        if debug.0 { Visibility::Inherited } else { Visibility::Hidden },
    ));
}

/// Reads the world's field sequence file and Sora's face from the disc.
#[allow(clippy::too_many_arguments)]
fn load_files(
    disc: Res<Disc>,
    area: Res<CurrentArea>,
    mut images: ResMut<Assets<Image>>,
    mut scene: ResMut<HudScene>,
    mut face: ResMut<FaceFile>,
    mut menu_file: ResMut<MenuFile>,
    mut menu_tables: ResMut<MenuTables>,
    mut drive_tables: ResMut<DriveTables>,
) {
    let world = area.name.get(..2).unwrap_or("tt");
    let mut files = hud_gfx::load_files(&disc, &format!("field2d/us/{world}0field.2dd"), &mut images);
    if files.is_empty() {
        files = hud_gfx::load_files(&disc, "field2d/us/zz0field.2dd", &mut images);
    }
    if files.is_empty() {
        warn!("hud: no field2d file for {world}; the HUD is not drawn");
        return;
    }
    // Sora's face is the `face` image/sequence pair in his object's APDX.
    if let Some(f) = hud_gfx::load_files(&disc, "obj/P_EX100.apdx", &mut images).pop() {
        face.0 = Some(files.len());
        files.push(f);
    }
    // The command menu's art (one `comw` pair) and the group tables its code reads from the executable.
    let mut cmd = hud_gfx::load_files(&disc, &format!("field2d/us/{world}0command.2dd"), &mut images);
    if cmd.is_empty() {
        cmd = hud_gfx::load_files(&disc, "field2d/us/zz0command.2dd", &mut images);
    }
    if !cmd.is_empty() {
        menu_file.0 = Some(files.len());
        files.extend(cmd);
    }
    if let Some(exe) = disc.read_root("SLUS_210.05") {
        menu_tables.0 = hud_menu::Tables::from_exe(&exe);
        *drive_tables = DriveTables::from_exe(&exe);
    }
    scene.text = hud_text::TextSys::load(&disc, world, &mut images, &mut files);
    scene.files = files;
}

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn debug_text(
    area: Res<CurrentArea>,
    sora: Query<(&Transform, &Sora)>,
    anims: Query<&Animator>,
    enemies: Query<(&GlobalTransform, &Enemy)>,
    mut text: Query<&mut Text, With<HudText>>,
) {
    let Ok((p, s)) = sora.single() else { return };
    let tag = anims.get(s.body).map(|a| format!("{} slot{:?} f{:.2} fin{} g{} jt{:.0}/{:.0}", a.tag(), a.slot, a.frame, a.finished, s.motor.grounded, s.motor.jump.t, s.motor.jump.rise)).unwrap_or_default();
    if let Ok(mut t) = text.single_mut() {
        let mode = if s.state == State::Viewer { "  [MOTION VIEWER: [ ] to step, F1 to exit]" } else { "" };
        t.0 = format!(
            "kh2-rs  |  {}  |  Sora Lv{}  HP {}/{}  MP {}  |  {:?}  {}{}\n\
             WASD move  Ctrl walk  Space jump  LMB/J attack  K/E guard  Q lock-on  RMB/arrows camera  1-9,0 areas  F1 motions  F3 enemies  F4 hitboxes  F9 this text",
            area.name, s.stats.level, s.hp, s.stats.max_hp, s.mp, s.state, tag, mode
        );
        for (g, e) in &enemies {
            t.0.push_str(&format!("\n{} {:?} hp {}/{} at {:.1} {:.1} {:.1} (d {:.1})", e.name, e.state, e.hp, e.stats.max_hp, g.translation().x, g.translation().y, g.translation().z, g.translation().distance(p.translation)));
        }
    }
}

/// Developer damage numbers. The original game draws none: its only floating number is the
/// munny popup (docs/hud.md), so these are off unless the debug overlay is on.
pub fn popups(
    mut commands: Commands,
    time: Res<Time>,
    mut msgs: MessageReader<DamagePopup>,
    cam: Query<(&Camera, &GlobalTransform), With<Camera3d>>,
    mut q: Query<(Entity, &mut Popup, &mut Node, &mut TextColor)>,
) {
    for m in msgs.read() {
        commands.spawn((
            Popup { at: m.at + Vec3::Y * 0.6, t: 0.0 },
            Text::new(m.amount.to_string()),
            TextFont { font_size: bevy::text::FontSize::Px(22.0), ..default() },
            TextColor(Color::WHITE),
            Node { position_type: PositionType::Absolute, ..default() },
        ));
    }
    let Ok((camera, cg)) = cam.single() else { return };
    for (e, mut p, mut n, mut c) in &mut q {
        p.t += time.delta_secs();
        if p.t > 0.8 {
            commands.entity(e).despawn();
            continue;
        }
        if let Ok(v) = camera.world_to_viewport(cg, p.at + Vec3::Y * p.t * 0.6) {
            n.left = Val::Px(v.x - 10.0);
            n.top = Val::Px(v.y - 12.0);
        }
        c.0 = Color::srgba(1.0, 1.0, 1.0, 1.0 - (p.t / 0.8).powi(3));
    }
}
