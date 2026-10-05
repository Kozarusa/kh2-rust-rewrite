//! HUD in the spirit of KH2's: HP (green) and MP (blue) gauges bottom-right, the locked or
//! auto-targeted enemy's HP top-right, floating damage numbers, and a lock-on marker.

use bevy::prelude::*;

use crate::assets::CurrentArea;
use crate::enemy::Enemy;
use crate::schedule::Frame;
use crate::skel::Animator;
use crate::sora::{Sora, State};

pub struct HudPlugin;

impl Plugin for HudPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<DamagePopup>()
            .add_systems(Startup, setup)
            .add_systems(Update, (update.in_set(Frame::Hud), popups.in_set(Frame::Popups)));
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
pub struct HpFill;
#[derive(Component)]
pub struct MpFill;
#[derive(Component)]
pub struct TargetFill;
#[derive(Component)]
pub struct TargetBar;
#[derive(Component)]
pub struct Popup {
    at: Vec3,
    t: f32,
}

fn bar(commands: &mut Commands, right: f32, top: Option<f32>, bottom: Option<f32>, width: f32, color: Color, marker: impl Component) -> Entity {
    let frame = commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                right: Val::Px(right),
                top: top.map_or(Val::Auto, Val::Px),
                bottom: bottom.map_or(Val::Auto, Val::Px),
                width: Val::Px(width),
                height: Val::Px(12.0),
                border: UiRect::all(Val::Px(2.0)),
                ..default()
            },
            BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.6)),
            BorderColor::all(Color::srgb(0.85, 0.85, 0.9)),
        ))
        .id();
    commands.spawn((Node { width: Val::Percent(100.0), height: Val::Percent(100.0), ..default() }, BackgroundColor(color), marker, ChildOf(frame)));
    frame
}

pub fn setup(mut commands: Commands) {
    commands.spawn((
        HudText,
        Text::new(""),
        TextFont { font_size: bevy::text::FontSize::Px(15.0), ..default() },
        TextColor(Color::WHITE),
        Node { position_type: PositionType::Absolute, left: Val::Px(12.0), top: Val::Px(10.0), ..default() },
    ));
    bar(&mut commands, 24.0, None, Some(44.0), 220.0, Color::srgb(0.25, 0.85, 0.2), HpFill);
    bar(&mut commands, 24.0, None, Some(26.0), 160.0, Color::srgb(0.2, 0.45, 1.0), MpFill);
    let t = bar(&mut commands, 24.0, Some(24.0), None, 200.0, Color::srgb(0.25, 0.85, 0.2), TargetFill);
    commands.entity(t).insert(TargetBar);
}

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
pub fn update(
    area: Res<CurrentArea>,
    sora: Query<(&Transform, &Sora)>,
    anims: Query<&Animator>,
    enemies: Query<(&GlobalTransform, &Enemy)>,
    mut text: Query<&mut Text, With<HudText>>,
    mut fills: ParamSet<(Query<&mut Node, With<HpFill>>, Query<&mut Node, With<MpFill>>, Query<&mut Node, With<TargetFill>>)>,
    mut target_bar: Query<&mut Visibility, With<TargetBar>>,
    mut gizmos: Gizmos,
) {
    let Ok((p, s)) = sora.single() else { return };
    let tag = anims.get(s.body).map(|a| format!("{} slot{:?} f{:.2} fin{} g{} jt{:.0}/{:.0}", a.tag(), a.slot, a.frame, a.finished, s.motor.grounded, s.motor.jump.t, s.motor.jump.rise)).unwrap_or_default();
    if let Ok(mut t) = text.single_mut() {
        let mode = if s.state == State::Viewer { "  [MOTION VIEWER: [ ] to step, F1 to exit]" } else { "" };
        t.0 = format!(
            "kh2-rs  |  {}  |  Sora Lv{}  HP {}/{}  MP {}  |  {:?}  {}{}\n\
             WASD move  Ctrl walk  Space jump  LMB/J attack  K/E guard  Q lock-on  RMB/arrows camera  1-9,0 areas  F1 motions  F3 enemies  F4 hitboxes",
            area.name, s.stats.level, s.hp, s.stats.max_hp, s.mp, s.state, tag, mode
        );
        for (g, e) in &enemies {
            t.0.push_str(&format!("\n{} {:?} hp {}/{} at {:.1} {:.1} {:.1} (d {:.1})", e.name, e.state, e.hp, e.stats.max_hp, g.translation().x, g.translation().y, g.translation().z, g.translation().distance(p.translation)));
        }
    }
    if let Ok(mut n) = fills.p0().single_mut() {
        n.width = Val::Percent(100.0 * s.hp as f32 / s.stats.max_hp.max(1) as f32);
    }
    if let Ok(mut n) = fills.p1().single_mut() {
        n.width = Val::Percent(s.mp.clamp(0, 100) as f32);
    }
    let target = s.lock.or(s.target).and_then(|t| enemies.get(t).ok());
    if let Ok(mut v) = target_bar.single_mut() {
        *v = if target.is_some() { Visibility::Inherited } else { Visibility::Hidden };
    }
    if let Some((g, e)) = target {
        if let Ok(mut n) = fills.p2().single_mut() {
            n.width = Val::Percent(100.0 * e.hp as f32 / e.stats.max_hp.max(1) as f32);
        }
        if s.lock.is_some() {
            let c = g.translation() + Vec3::Y * 0.9;
            gizmos.circle(Isometry3d::new(c, Quat::from_rotation_arc(Vec3::Z, (p.translation - c).normalize_or(Vec3::Z))), 0.45, Color::srgb(1.0, 0.85, 0.2));
        }
    }
}

pub fn popups(
    mut commands: Commands,
    time: Res<Time>,
    mut msgs: MessageReader<DamagePopup>,
    cam: Query<(&Camera, &GlobalTransform)>,
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
