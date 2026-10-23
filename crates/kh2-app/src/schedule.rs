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

/// What freezes the simulation. The pause menu sets `menu` in the same frame as its open input (`menu::input`, in
/// `PreUpdate`, before the 60 Hz sets and the `Update` systems of that frame) and clears it when the close has
/// finished; the combat PAUSE band (`menu_pause`) sets `band`. Music, the HUD and the menu draw on.
///
/// The original stops the field by changing scene to the menu scene (docs/systems/ui/menus-pause.md 1.1: `FUN_00138758` ->
/// `FUN_0024f0b8` -> `FUN_001033a8`), so the field tasks do not run while a menu is open [V for the camp menu];
/// the sound driver (the BGM) is not a field task. The app has no scene change: every simulation system waits for
/// `sim_running` instead.
#[derive(Resource, Default, Clone, Copy, Debug, PartialEq, Eq)]
pub struct Pause {
    pub menu: bool,
    pub band: bool,
}

impl Pause {
    pub fn frozen(self) -> bool {
        self.menu || self.band
    }
}

/// Run condition of every simulation system: nothing is paused.
pub fn sim_running(pause: Res<Pause>) -> bool {
    !pause.frozen()
}

pub struct SchedulePlugin;

impl Plugin for SchedulePlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(Time::<Fixed>::from_hz(data::LOGIC_HZ as f64))
            .init_resource::<Pause>()
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
                    .chain()
                    // The one run condition of the whole simulation: a pause stops every set at once.
                    .run_if(sim_running),
            )
            .configure_sets(Update, Frame::Pad.after(assets::load_area))
            .configure_sets(
                Update,
                (Frame::Camera, Frame::Sky, Frame::Hud, Frame::Popups, Frame::Volumes).chain().after(Frame::Pad),
            );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Resource, Default)]
    struct Ran(Vec<&'static str>);

    fn app() -> App {
        let mut app = App::new();
        app.add_plugins(SchedulePlugin).init_resource::<Ran>();
        app.add_systems(FixedUpdate, (|mut r: ResMut<Ran>| r.0.push("sora")).in_set(Sim::Sora));
        app.add_systems(FixedUpdate, (|mut r: ResMut<Ran>| r.0.push("fx")).in_set(Sim::EffectTick));
        // A system outside the sets carries the condition itself (the camera, the area jump).
        app.add_systems(FixedUpdate, (|mut r: ResMut<Ran>| r.0.push("camera")).after(Sim::EffectTick).run_if(sim_running));
        app
    }

    #[test]
    fn a_pause_stops_every_set_and_a_resume_runs_them_again() {
        let mut app = app();
        app.world_mut().run_schedule(FixedUpdate);
        assert_eq!(app.world().resource::<Ran>().0, ["sora", "fx", "camera"], "nothing paused: the sets run in order");
        app.world_mut().resource_mut::<Ran>().0.clear();
        for pause in [Pause { menu: true, band: false }, Pause { menu: false, band: true }, Pause { menu: true, band: true }] {
            *app.world_mut().resource_mut::<Pause>() = pause;
            app.world_mut().run_schedule(FixedUpdate);
            assert!(app.world().resource::<Ran>().0.is_empty(), "{pause:?} freezes the simulation");
        }
        *app.world_mut().resource_mut::<Pause>() = Pause::default();
        app.world_mut().run_schedule(FixedUpdate);
        assert_eq!(app.world().resource::<Ran>().0.len(), 3, "resumed");
    }

    #[test]
    fn frozen_means_menu_or_band() {
        assert!(!Pause::default().frozen());
        assert!(Pause { menu: true, band: false }.frozen());
        assert!(Pause { menu: false, band: true }.frozen());
    }
}
