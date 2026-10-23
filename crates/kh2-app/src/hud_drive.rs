//! The drive gauge as the HUD will read it, and a minimal debug readout.
//!
//! `DriveGauge` is rewritten every tick by `drive` from its own state. A HUD that draws the game's
//! drive gauge reads this resource and never touches the drive state: `points` and `max_points`
//! are the orbs of the normal gauge, `fraction` the part of the next point (0..=100, 100 = full),
//! and while a form is active `time_left` / `time_total` give the form timer (the gauge that
//! empties) and `time_points` its whole drive points. The readout is text, shown with the debug
//! overlay (F9 or `KH2_DEBUGHUD=1`) or whenever `KH2_DRIVE` is set.

use bevy::prelude::*;

use crate::shared::hud::DebugHud;

/// What the HUD needs of the drive system.
#[derive(Resource, Clone, Debug, Default, PartialEq)]
pub struct DriveGauge {
    pub points: u8,
    pub max_points: u8,
    pub fraction: u8,
    /// Current form id (0 none, 1 Valor, 2 Wisdom, 3 Master, 4 Final, 5 Anti).
    pub form: u8,
    pub form_level: u8,
    /// A form is active (the gauge mode is the form timer).
    pub in_form: bool,
    /// A form change is playing (the command menu is locked).
    pub changing: bool,
    /// Form timer in gauge units, its full value, whole drive points left (the game's own
    /// `FUN_001a0420`) and the seconds left at the current drain.
    pub time_left: f32,
    pub time_total: f32,
    pub time_points: i32,
    pub seconds_left: f32,
}

#[derive(Component)]
pub struct DriveText;

pub struct HudDrivePlugin;

impl Plugin for HudDrivePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<DriveGauge>()
            .add_systems(Startup, setup)
            .add_systems(Update, readout.in_set(crate::schedule::Frame::Hud));
    }
}

fn setup(mut commands: Commands) {
    commands.spawn((
        DriveText,
        Text::new(""),
        TextFont { font_size: bevy::text::FontSize::Px(15.0), ..default() },
        TextColor(Color::srgb(1.0, 0.9, 0.5)),
        Node { position_type: PositionType::Absolute, left: Val::Px(12.0), bottom: Val::Px(10.0), ..default() },
        Visibility::Hidden,
    ));
}

fn readout(gauge: Res<DriveGauge>, debug: Res<DebugHud>, mut text: Query<(&mut Text, &mut Visibility), With<DriveText>>) {
    let Ok((mut t, mut v)) = text.single_mut() else { return };
    let on = debug.0 || std::env::var_os("KH2_DRIVE").is_some();
    *v = if on { Visibility::Inherited } else { Visibility::Hidden };
    if !on {
        return;
    }
    let f = kh2_tables::player::form::name(gauge.form);
    t.0 = if gauge.in_form {
        format!(
            "drive: {f} Lv{}{}  time {:.0}/{:.0} ({} points, {:.1}s)  gauge {}/{}",
            gauge.form_level,
            if gauge.changing { " (changing)" } else { "" },
            gauge.time_left,
            gauge.time_total,
            gauge.time_points,
            gauge.seconds_left,
            gauge.points,
            gauge.max_points
        )
    } else {
        format!("drive: {}/{} +{}%{}", gauge.points, gauge.max_points, gauge.fraction, if gauge.changing { " (changing)" } else { "" })
    };
}
