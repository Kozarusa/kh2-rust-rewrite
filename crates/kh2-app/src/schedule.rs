//! Order between features. Each feature registers its own systems in its own plugin and puts
//! them in one of the sets below; the sets are chained here, so the whole cross-feature order
//! reads in one place.
//!
//! `Sim` is the 60 Hz logic tick (`FixedUpdate`). It is one total order, identical to the single
//! chain that `main.rs` used to hold: read the sets top to bottom. A set is a run of consecutive
//! systems of one feature, chained in the order the feature's plugin lists them.
//! `Frame` is the per-frame order in `Update` that follows `sora::read_pad`.

use bevy::prelude::*;

use crate::{assets, data};

/// `FixedUpdate`, in run order.
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Sim {
    /// Sora's weapon slot follows his state (sora).
    Weapon,
    /// Motion playback and bone poses for every character (skel).
    Pose,
    /// Weapon socket follows the hand bone, then Sora's controller tick (sora).
    Sora,
    /// Wave spawning, enemy AI, enemy separation (enemy).
    Enemy,
    /// Attack volumes against hurt volumes and walls (combat).
    Detect,
    /// Sora takes enemy hits and wall hits (sora).
    SoraHurt,
    /// Enemies take Sora's hits (enemy).
    EnemyHurt,
    /// Effect starts and their sounds: weapon trails, motion effects, start requests, hit
    /// sparks, the FXTEST hook, effect sounds (fx).
    Effects,
    /// Motion-trigger sounds and wall-hit sounds (audio).
    Sounds,
    /// Running effects advance (fx).
    EffectTick,
}

/// `Update`, in run order after the pad is read.
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Frame {
    /// Keyboard and mouse into `Pad` (sora). Runs after the area is loaded.
    Pad,
    Camera,
    Sky,
    Hud,
    Popups,
    /// Debug hit-volume overlay (combat).
    Volumes,
}

pub struct SchedulePlugin;

impl Plugin for SchedulePlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(Time::<Fixed>::from_hz(data::LOGIC_HZ as f64))
            .configure_sets(
                FixedUpdate,
                (
                    Sim::Weapon,
                    Sim::Pose,
                    Sim::Sora,
                    Sim::Enemy,
                    Sim::Detect,
                    Sim::SoraHurt,
                    Sim::EnemyHurt,
                    Sim::Effects,
                    Sim::Sounds,
                    Sim::EffectTick,
                )
                    .chain(),
            )
            .configure_sets(Update, Frame::Pad.after(assets::load_area))
            .configure_sets(
                Update,
                (Frame::Camera, Frame::Sky, Frame::Hud, Frame::Popups, Frame::Volumes).chain().after(Frame::Pad),
            );
    }
}
