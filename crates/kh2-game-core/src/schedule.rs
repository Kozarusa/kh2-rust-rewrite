//! Order between features. Each feature registers its own systems in its own plugin and puts
//! them in one of the sets below; the sets are chained here, so the whole cross-feature order
//! reads in one place.
//!
//! `Sim` is the 60 Hz logic tick (`FixedUpdate`). It is one total order, identical to the single
//! chain that `main.rs` used to hold: read the sets top to bottom. A set is a run of consecutive
//! systems of one feature, chained in the order the feature's plugin lists them.
//! `Frame` is the per-frame order in `Update` that follows `sora::read_pad`.

use bevy::ecs::schedule::ScheduleLabel;
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
    /// The HUD's gauge and party-panel updates (inside Hud): the summon feeds order themselves before them.
    HudGauges,
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

/// The vblank clock and the frame step that drive the logic (WP-L1-P4; docs/systems/core-runtime/runtime.md "Frame step" and
/// "Frame order in the app").
///
/// One fixed step of the app (`FixedMain`, 60 Hz) is one vblank of the original: `FixedFirst` and `FixedLast` run once per vblank, and
/// [`Vblank`] (put where `FixedUpdate` stood in the fixed order) adds the vblank to the ported `frame::Vsync` block and runs the game
/// logic, `FixedUpdate`, as many times as the completed frame's step says. The original's field runs limit 1 (`FUN_0014cd38`): a game
/// frame every second vblank with step 2.0, so `FixedUpdate` runs twice on every second vblank and not at all in between; the menus run
/// limit 0 (`FUN_00239e78`, restored by `FUN_00239ec0`): one frame per vblank, step 1.0.
///
/// Decided (RULES A20): every system of the app moves by a 60 Hz unit per `FixedUpdate`, so a step of 2.0 is two `FixedUpdate`
/// runs back to back instead of one run with every speed doubled. Linear motion (a run at constant speed) is the same to the bit at
/// every frame boundary; motion that integrates (gravity, a turn toward a target) takes two half steps where the original takes one
/// [inferred: no oracle compares a double step]. Nothing that counts ticks changes its rate: still 60 logic ticks a second.
#[derive(Resource, Clone, Debug, Default)]
pub struct FrameClock {
    pub vsync: crate::frame::Vsync,
    /// `FixedUpdate` runs of the vblank that ran last.
    pub runs: u32,
    /// The frame type and limit the menu saved when it opened (`DAT_0036ca40`, `DAT_0036ca44`); `None` while no menu is up.
    saved: Option<(u8, u32)>,
}

impl FrameClock {
    /// The clock after the frame init `FUN_0014cd38` (field cadence).
    pub fn new() -> FrameClock {
        let mut vsync = crate::frame::Vsync::default();
        vsync.init();
        FrameClock { vsync, runs: 0, saved: None }
    }

    /// The step of the last completed frame (`0x356f40`).
    pub fn step(&self) -> f32 {
        self.vsync.frame_step()
    }

    /// The menu cadence: on open, save type and limit and run type 1, limit 0 (`FUN_00239e78`); on close, restore them
    /// (`FUN_00239ec0`).
    pub fn set_menu(&mut self, open: bool) {
        match (open, self.saved) {
            (true, None) => {
                self.saved = Some((self.vsync.frame_type(), self.vsync.limit()));
                self.vsync.set_frame_type(1);
                self.vsync.set_frame_limit(0);
            }
            (false, Some((t, l))) => {
                self.saved = None;
                self.vsync.set_frame_type(t);
                self.vsync.set_frame_limit(l);
            }
            _ => {}
        }
    }

    /// One vblank: the handler's count (`FUN_0014cc88`), then, when the frame is complete (the counter has reached `limit + 1`), the
    /// frame step `FUN_0014cdd0` with no wait left to do. Returns how many logic ticks the vblank runs: the step rounded (0 while the
    /// frame is still waiting).
    pub fn vblank(&mut self) -> u32 {
        self.vsync.vblank();
        self.runs = if self.vsync.since_step() >= self.vsync.limit().wrapping_add(1) {
            self.vsync.step_frame(|_| {});
            self.vsync.frame_step().round() as u32
        } else {
            0
        };
        self.runs
    }
}

/// The schedule that stands in `FixedMain` where `FixedUpdate` stood: one run per vblank (see [`FrameClock`]).
#[derive(bevy::ecs::schedule::ScheduleLabel, Debug, Clone, PartialEq, Eq, Hash)]
pub struct Vblank;

/// The one system of [`Vblank`]: the menu cadence, the vblank, then the logic ticks of a completed frame.
///
/// In a rig run the frame boundaries are put on the rig clock: the state was saved at a frame boundary (the original's field stops
/// at even `k`), so the counter starts at 0 with `k = 0`. The logic of the frame that starts at an even `k` then runs at the odd vblank
/// after it, which is the vblank whose pad the app has read by then (the pad is read in `Update`, one fixed step behind the script),
/// and the frame's motion stands at the next even `k`, as in the original [inferred: the original reads the pad at the frame start].
pub fn vblank_step(world: &mut World) {
    let menu = world.get_resource::<Pause>().is_some_and(|p| p.menu);
    let rig_start = world.get_resource::<crate::rig::RigClock>().is_some_and(|c| c.started && c.ticks == 0);
    let runs = {
        let mut clock = world.get_resource_or_init::<FrameClock>();
        clock.set_menu(menu);
        if rig_start {
            clock.vsync.set_since_step(0);
        }
        clock.vblank()
    };
    for _ in 0..runs {
        world.run_schedule(FixedUpdate);
    }
}

pub struct SchedulePlugin;

impl Plugin for SchedulePlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(Time::<Fixed>::from_hz(data::LOGIC_HZ as f64))
            .init_resource::<Pause>()
            .insert_resource(FrameClock::new())
            .add_systems(Vblank, vblank_step);
        // `FixedUpdate` leaves the fixed order: `Vblank` runs it (absent in a bare test app without the main schedules).
        if let Some(mut order) = app.world_mut().get_resource_mut::<bevy::app::FixedMainScheduleOrder>() {
            if let Some(i) = order.labels.iter().position(|l| (**l).eq(&FixedUpdate)) {
                order.labels[i] = Vblank.intern();
            }
        }
        app
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

    /// The field runs the logic twice on every second vblank (step 2.0), the menu once per vblank (step 1.0), and the field's
    /// cadence comes back when the menu closes.
    #[test]
    fn the_field_runs_two_ticks_every_second_vblank_and_the_menu_one_per_vblank() {
        let mut c = FrameClock::new();
        let runs: Vec<u32> = (0..6).map(|_| c.vblank()).collect();
        assert_eq!(runs, [0, 2, 0, 2, 0, 2]);
        assert_eq!(c.step(), 2.0);
        c.set_menu(true);
        let runs: Vec<u32> = (0..4).map(|_| c.vblank()).collect();
        assert_eq!((runs, c.step()), (vec![1, 1, 1, 1], 1.0));
        c.set_menu(false);
        let runs: Vec<u32> = (0..4).map(|_| c.vblank()).collect();
        assert_eq!((runs, c.step()), (vec![0, 2, 0, 2], 2.0));
    }

    /// In the app the fixed order runs `Vblank` instead of `FixedUpdate`, and `FixedUpdate` runs as often as the step says.
    #[test]
    fn the_fixed_order_runs_the_logic_through_the_vblank() {
        let mut app = App::new();
        app.add_plugins(SchedulePlugin).init_resource::<Ran>();
        app.add_systems(FixedUpdate, (|mut r: ResMut<Ran>| r.0.push("sora")).in_set(Sim::Sora));
        let order = app.world().resource::<bevy::app::FixedMainScheduleOrder>();
        assert!(order.labels.iter().any(|l| (**l).eq(&Vblank)) && !order.labels.iter().any(|l| (**l).eq(&FixedUpdate)));
        let mut counts = Vec::new();
        for _ in 0..4 {
            app.world_mut().resource_mut::<Ran>().0.clear();
            app.world_mut().run_schedule(bevy::app::FixedMain);
            counts.push(app.world().resource::<Ran>().0.len());
        }
        assert_eq!(counts, [0, 2, 0, 2]);
    }

    #[test]
    fn frozen_means_menu_or_band() {
        assert!(!Pause::default().frozen());
        assert!(Pause { menu: true, band: false }.frozen());
        assert!(Pause { menu: false, band: true }.frozen());
    }
}
