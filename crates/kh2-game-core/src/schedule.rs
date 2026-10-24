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
    /// The field's frame limit: 1 = the original's 30 Hz field, 0 = a logic frame per vblank (60 Hz).
    field_limit: u32,
    /// The clock the logic sees as `Time` (see [`vblank_step`]).
    logic_time: Time,
    /// The app frame in which the logic ran last (see [`hold_messages`]).
    logic_frame: Option<u32>,
}

impl FrameClock {
    /// The clock after the frame init `FUN_0014cd38` (field cadence).
    pub fn new() -> FrameClock {
        let mut vsync = crate::frame::Vsync::default();
        vsync.init();
        FrameClock { vsync, runs: 0, saved: None, field_limit: 1, logic_time: Time::default(), logic_frame: None }
    }

    /// The clock with the field at `hz` (30 = the original's limit 1, 60 = limit 0, one logic tick per vblank, step 1.0).
    pub fn with_field_hz(hz: u32) -> FrameClock {
        let mut c = FrameClock::new();
        c.set_field_hz(hz);
        c
    }

    /// Switch the field cadence (30 or 60). While a menu is up the menu cadence stays and the field's is restored on close.
    pub fn set_field_hz(&mut self, hz: u32) {
        self.field_limit = if hz >= 60 { 0 } else { 1 };
        match self.saved {
            Some((t, _)) => self.saved = Some((t, self.field_limit)),
            None => self.vsync.set_frame_limit(self.field_limit),
        }
    }

    /// The field's rate in Hz (30 or 60).
    pub fn field_hz(&self) -> u32 {
        if self.field_limit == 0 { 60 } else { 30 }
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

/// The button edges of the app frames since the last logic tick: the logic of a frame sees every press and release made since the
/// frame before it, once, as the original's pad task reads the pad once per game frame (`0x167038`, priority 110000). Without it a
/// press in a frame whose vblank runs no logic would be lost, and one in a logic frame would be seen by both of its ticks.
#[derive(Clone, Debug)]
struct Edges<T: Clone + Eq + std::hash::Hash + Send + Sync + 'static> {
    pressed: Vec<T>,
    released: Vec<T>,
    /// The app frame whose edges were taken last (a frame can hold more than one vblank).
    frame: Option<u32>,
}

impl<T: Copy + Eq + std::hash::Hash + Send + Sync + 'static> Edges<T> {
    fn new() -> Self {
        Edges { pressed: Vec::new(), released: Vec::new(), frame: None }
    }

    /// Adds the edges of `input` unless this frame's were taken already.
    fn gather(&mut self, input: &ButtonInput<T>, frame: Option<u32>) {
        if frame.is_some() && frame == self.frame {
            return;
        }
        self.frame = frame;
        for k in input.get_just_pressed() {
            if !self.pressed.contains(k) {
                self.pressed.push(*k);
            }
        }
        for k in input.get_just_released() {
            if !self.released.contains(k) {
                self.released.push(*k);
            }
        }
    }

    /// The input the logic sees: the keys held now, with the gathered edges.
    fn input(&self, now: &ButtonInput<T>) -> ButtonInput<T> {
        let mut b = ButtonInput::default();
        for k in now.get_pressed().chain(&self.pressed).chain(&self.released) {
            b.press(*k);
        }
        b.clear();
        for k in &self.pressed {
            b.release(*k);
            b.press(*k);
            if !self.released.contains(k) {
                b.clear_just_released(*k);
            }
        }
        for k in &self.released {
            if !now.pressed(*k) {
                b.release(*k);
            }
        }
        b
    }

    fn take(&mut self) {
        self.pressed.clear();
        self.released.clear();
    }
}

/// Runs `FixedUpdate` `runs` times with the logic's clock and the gathered button edges in place of the frame's (see [`vblank_step`]).
fn run_logic(world: &mut World, runs: u32) {
    let frame = world.get_resource::<bevy::diagnostic::FrameCount>().map(|f| f.0);
    let keys = world.remove_resource::<ButtonInput<KeyCode>>();
    let mouse = world.remove_resource::<ButtonInput<MouseButton>>();
    if runs > 0 {
        world.resource_mut::<FrameClock>().logic_frame = frame;
    }
    let (logic_keys, logic_mouse) = {
        let mut latch = world.get_resource_or_insert_with(InputLatch::default);
        if let Some(k) = &keys {
            latch.keys.gather(k, frame);
        }
        if let Some(m) = &mouse {
            latch.mouse.gather(m, frame);
        }
        if runs == 0 {
            (None, None)
        } else {
            let out = (keys.as_ref().map(|k| latch.keys.input(k)), mouse.as_ref().map(|m| latch.mouse.input(m)));
            latch.keys.take();
            latch.mouse.take();
            out
        }
    };
    if let Some(k) = logic_keys {
        world.insert_resource(k);
    }
    if let Some(m) = logic_mouse {
        world.insert_resource(m);
    }
    // The logic sees its own clock as `Time`: one fixed step per run, so the systems that fire when the elapsed time crosses a
    // scripted second (`elapsed - delta < at <= elapsed`) see every step once, also the vblanks that ran no logic.
    let vblank_time = world.get_resource::<Time>().cloned();
    for i in 0..runs {
        if let Some(v) = &vblank_time {
            let mut clock = world.resource_mut::<FrameClock>();
            clock.logic_time.advance_by(v.delta());
            let t = clock.logic_time;
            *world.resource_mut::<Time>() = t;
        }
        if i == 1 {
            // The edges belong to the frame's first tick.
            if let Some(mut k) = world.get_resource_mut::<ButtonInput<KeyCode>>() {
                k.clear();
            }
            if let Some(mut m) = world.get_resource_mut::<ButtonInput<MouseButton>>() {
                m.clear();
            }
        }
        world.run_schedule(FixedUpdate);
    }
    if let Some(v) = vblank_time {
        *world.resource_mut::<Time>() = v;
    }
    match keys {
        Some(k) => world.insert_resource(k),
        None => {
            world.remove_resource::<ButtonInput<KeyCode>>();
        }
    }
    match mouse {
        Some(m) => world.insert_resource(m),
        None => {
            world.remove_resource::<ButtonInput<MouseButton>>();
        }
    }
}

/// `FixedLast`: Bevy ages the message buffers once in each frame after a fixed step (the signal of `FixedPostUpdate`), and a message
/// lives for two agings. A message the logic writes for its next tick would be gone after the two frames without logic between two
/// logic frames, so the aging waits for frames in which the logic ran.
pub fn hold_messages(clock: Res<FrameClock>, frame: Option<Res<bevy::diagnostic::FrameCount>>, registry: Option<ResMut<bevy::ecs::message::MessageRegistry>>) {
    use bevy::ecs::message::ShouldUpdateMessages;
    let Some(mut registry) = registry else { return };
    if matches!(registry.should_update, ShouldUpdateMessages::Ready) && clock.logic_frame != frame.map(|f| f.0) {
        registry.should_update = ShouldUpdateMessages::Waiting;
    }
}

#[derive(Resource)]
struct InputLatch {
    keys: Edges<KeyCode>,
    mouse: Edges<MouseButton>,
}

impl Default for InputLatch {
    fn default() -> Self {
        InputLatch { keys: Edges::new(), mouse: Edges::new() }
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
    run_logic(world, runs);
}

/// The field rate: `KH2_FIELD_HZ=30|60` wins; else 30 for rig and turbo runs (they compare against the original) and 60 for play.
pub fn field_hz_for(var: Option<&str>, rig_or_turbo: bool) -> u32 {
    match var.map(str::trim) {
        Some("30") => 30,
        Some("60") => 60,
        _ if rig_or_turbo => 30,
        _ => 60,
    }
}

/// [`field_hz_for`] on the process environment.
pub fn field_hz_from_env() -> u32 {
    let rig = ["KH2_TURBO", "KH2_PAD", "KH2_TRACE", "KH2_STATE"].iter().any(|k| std::env::var_os(k).is_some_and(|v| v != "0"));
    field_hz_for(std::env::var("KH2_FIELD_HZ").ok().as_deref(), rig)
}

/// Numpad + switches the field between 30 and 60 Hz (not in turbo runs).
pub fn toggle_field_hz(keys: Res<ButtonInput<KeyCode>>, mut clock: ResMut<FrameClock>, mut shown: ResMut<FieldHzShown>) {
    if keys.just_pressed(KeyCode::NumpadAdd) && !crate::harness::turbo() {
        let hz = if clock.field_hz() == 60 { 30 } else { 60 };
        clock.set_field_hz(hz);
        shown.0 = 120;
    }
    shown.0 = shown.0.saturating_sub(1);
}

/// Update frames left to show the field rate (about 2 seconds after Numpad +).
#[derive(Resource, Default)]
pub struct FieldHzShown(pub u32);

/// Gathers the button edges of every rendered frame into the latch, also of a frame whose vblank runs no `FixedMain` (at an uncapped
/// frame rate most frames run none): the next logic tick takes them, so a tap is never lost. A frame already gathered is skipped.
fn latch_input(
    keys: Option<Res<ButtonInput<KeyCode>>>,
    mouse: Option<Res<ButtonInput<MouseButton>>>,
    frame: Option<Res<bevy::diagnostic::FrameCount>>,
    mut latch: ResMut<InputLatch>,
) {
    let frame = frame.map(|f| f.0);
    if let Some(k) = keys {
        latch.keys.gather(&k, frame);
    }
    if let Some(m) = mouse {
        latch.mouse.gather(&m, frame);
    }
}

pub struct SchedulePlugin;

impl Plugin for SchedulePlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(Time::<Fixed>::from_hz(data::LOGIC_HZ as f64))
            .init_resource::<Pause>()
            .insert_resource(FrameClock::with_field_hz(field_hz_from_env()))
            .init_resource::<FieldHzShown>()
            .init_resource::<InputLatch>()
            .add_systems(PreUpdate, latch_input.after(bevy::input::InputSystems))
            .add_systems(Update, toggle_field_hz)
            .add_systems(Vblank, vblank_step)
            .add_systems(FixedLast, hold_messages);
        // `FixedUpdate` leaves the fixed order: `Vblank` runs it (absent in a bare test app without the main schedules).
        if let Some(mut order) = app.world_mut().get_resource_mut::<bevy::app::FixedMainScheduleOrder>()
            && let Some(i) = order.labels.iter().position(|l| (**l).eq(&FixedUpdate))
        {
            order.labels[i] = Vblank.intern();
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

    /// A tap in a rendered frame that runs no logic is still seen by the next logic tick (uncapped rendering).
    #[test]
    fn a_tap_in_a_frame_without_logic_reaches_the_next_logic_tick() {
        #[derive(Resource, Default)]
        struct Seen(Vec<bool>);
        let mut app = App::new();
        app.add_plugins(SchedulePlugin).init_resource::<Seen>().init_resource::<ButtonInput<KeyCode>>();
        app.add_systems(FixedUpdate, |k: Res<ButtonInput<KeyCode>>, mut s: ResMut<Seen>| s.0.push(k.just_pressed(KeyCode::KeyJ)));
        app.world_mut().resource_mut::<ButtonInput<KeyCode>>().press(KeyCode::KeyJ);
        app.update(); // a frame with no logic: the latch takes the edge
        {
            let mut k = app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
            k.clear();
            k.release(KeyCode::KeyJ);
            k.clear();
        }
        app.update(); // the key is up and the edge is gone from the input
        run_logic(app.world_mut(), 1);
        assert_eq!(app.world().resource::<Seen>().0, [true], "the tap survived the frames without logic");
        run_logic(app.world_mut(), 1);
        assert_eq!(app.world().resource::<Seen>().0, [true, false], "and is seen once");
    }

    /// A latched tap in a frame with two ticks (field at 30 Hz) reaches the first tick only.
    #[test]
    fn a_latched_tap_is_seen_by_the_first_of_two_ticks_only() {
        #[derive(Resource, Default)]
        struct Seen(Vec<bool>);
        let mut app = App::new();
        app.add_plugins(SchedulePlugin).init_resource::<Seen>().init_resource::<ButtonInput<KeyCode>>();
        app.add_systems(FixedUpdate, |k: Res<ButtonInput<KeyCode>>, mut s: ResMut<Seen>| s.0.push(k.just_pressed(KeyCode::KeyJ)));
        app.world_mut().resource_mut::<ButtonInput<KeyCode>>().press(KeyCode::KeyJ);
        app.update();
        {
            let mut k = app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
            k.clear();
            k.release(KeyCode::KeyJ);
            k.clear();
        }
        app.update();
        run_logic(app.world_mut(), 2);
        assert_eq!(app.world().resource::<Seen>().0, [true, false]);
    }

    #[test]
    fn the_field_rate_is_selected_by_the_variable_and_the_environment() {
        assert_eq!(field_hz_for(None, false), 60, "play defaults to the smooth field");
        assert_eq!(field_hz_for(None, true), 30, "rig and turbo runs keep the original's cadence");
        assert_eq!(field_hz_for(Some("30"), false), 30);
        assert_eq!(field_hz_for(Some("60"), true), 60);
        assert_eq!(field_hz_for(Some("x"), false), 60);
    }

    /// Limit 0 in the field: one tick per vblank, step 1.0, and it survives a menu.
    #[test]
    fn a_sixty_hertz_field_runs_one_tick_per_vblank() {
        let mut c = FrameClock::with_field_hz(60);
        assert_eq!((0..4).map(|_| c.vblank()).collect::<Vec<_>>(), [1, 1, 1, 1]);
        assert_eq!(c.step(), 1.0);
        c.set_menu(true);
        c.set_field_hz(30);
        c.set_menu(false);
        assert_eq!(c.field_hz(), 30);
        let runs: Vec<u32> = (0..4).map(|_| c.vblank()).collect();
        assert!(runs.iter().sum::<u32>() == 4 && runs.contains(&0), "30 Hz again: {runs:?}");
    }

    /// In the app the fixed order runs `Vblank` instead of `FixedUpdate`, and `FixedUpdate` runs as often as the step says.
    #[test]
    fn the_fixed_order_runs_the_logic_through_the_vblank() {
        let mut app = App::new();
        app.add_plugins(SchedulePlugin).init_resource::<Ran>();
        app.world_mut().resource_mut::<FrameClock>().set_field_hz(30);
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
