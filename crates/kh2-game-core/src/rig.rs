//! The app side of the diff rig (local/specs/RIG.md section 2; the original side is `crates/kh2-rig`).
//!
//! * [`PadState`]: the controller as the original's pad buffer holds it (sixteen buttons in the buffer's bit order, four sticks, centre
//!   0x7f). `KH2_PAD=<file>` plays a pad script (`kh2_rig::pad`, keyed on the rig clock `k`) into it every frame. The buttons are
//!   mirrored onto the keys the menus and the field already read ([`BUTTON_KEYS`]), so a script opens the pause menu like the
//!   keyboard does; the stick drives Sora through the original's dead zone and scale ([`stick_vector`]).
//! * [`RigClock`]: `k` counts `FixedUpdate` ticks from the first tick after the area and Sora stand (and, with a rig `sora` key,
//!   Sora was put where the original had him). Frozen ticks (a menu open) count too, like the original's vsync counter.
//! * `KH2_STATE` keys `save_block`, `place`, `sora`, `rig` are read by `Progress::apply_json`; [`start_area`] takes the place over
//!   the command-line area, [`place_sora`] puts Sora at the state's position once the area has spawned.
//! * `KH2_TRACE=<names>` with `KH2_TRACE_AT=<k list | every>` writes `trace-port.csv` (`KH2_TRACE_FILE` names another file), the
//!   `k,name,value,stop` rows of `kh2_rig::trace`, values by name from `rig_values`. The state at `k` is the state before tick `k` runs
//!   (what the original shows at its pad read of vsync `k`: the previous frames' effects settled, the input of `k` not yet applied).
//!   The run ends (`AppExit`) after the last k of the pad script (`end`), or of `KH2_TRACE_END`, or of the `KH2_TRACE_AT` list.
//!
//! Environment: `KH2_PAD`, `KH2_TRACE`, `KH2_TRACE_AT`, `KH2_TRACE_END`, `KH2_TRACE_FILE`, `KH2_STATE`. Without them nothing here runs.

use std::collections::BTreeSet;
use std::io::Write;

use bevy::app::AppExit;
use bevy::prelude::*;
use kh2_rig::pad::{Frame, PadScript, BUTTONS, CENTRE};

use crate::area::{game_to_app, AreaRecord};
use crate::assets::Disc;
use crate::progress::{Json, Progress};
use crate::rig_values::{self, Ctx};
use crate::shared::sora::Sora;
use crate::shared::ui::{MenuPhase, RigMenu};
use crate::SCALE;

/// The controller as one view. Bit i of `buttons` is `BUTTONS[i]` of `kh2_rig::pad` (Select, L3, R3, Start, Up, Right, Down, Left,
/// L2, R2, L1, R1, Triangle, Circle, Cross, Square: the original's pad buffer order, pressed = 1).
#[derive(Resource, Clone, Copy, Debug, PartialEq, Eq)]
pub struct PadState {
    pub buttons: u16,
    pub lx: u8,
    pub ly: u8,
    pub rx: u8,
    pub ry: u8,
    /// A pad script (or a test) fills the state: `read_pad` takes the stick from it instead of from WASD.
    pub scripted: bool,
}

impl Default for PadState {
    fn default() -> PadState {
        PadState { buttons: 0, lx: CENTRE, ly: CENTRE, rx: CENTRE, ry: CENTRE, scripted: false }
    }
}

impl PadState {
    pub fn from_frame(f: Frame) -> PadState {
        PadState { buttons: f.buttons, lx: f.lx, ly: f.ly, rx: f.rx, ry: f.ry, scripted: true }
    }

    /// The keyboard as a pad: WASD is the left stick at full tilt, J, Space, K or E, and Q or Tab are Cross, Circle, Square and L1.
    pub fn from_keyboard(keys: &ButtonInput<KeyCode>) -> PadState {
        let axis = |neg: KeyCode, pos: KeyCode| match (keys.pressed(neg), keys.pressed(pos)) {
            (true, false) => 0,
            (false, true) => 255,
            _ => CENTRE,
        };
        let held = [
            ("cross", keys.pressed(KeyCode::KeyJ)),
            ("circle", keys.pressed(KeyCode::Space)),
            ("square", keys.pressed(KeyCode::KeyK) || keys.pressed(KeyCode::KeyE)),
            ("l1", keys.pressed(KeyCode::KeyQ) || keys.pressed(KeyCode::Tab)),
        ];
        let mut buttons = 0u16;
        for (name, on) in held {
            if let (true, Some(i)) = (on, BUTTONS.iter().position(|b| *b == name)) {
                buttons |= 1 << i;
            }
        }
        PadState { buttons, lx: axis(KeyCode::KeyA, KeyCode::KeyD), ly: axis(KeyCode::KeyW, KeyCode::KeyS), ..PadState::default() }
    }

    pub fn pressed(&self, name: &str) -> bool {
        BUTTONS.iter().position(|b| *b == name).is_some_and(|i| self.buttons >> i & 1 != 0)
    }
}

/// The key each pad button stands for in the menus and the field (the app reads keys directly in 35 files). A button may press several
/// keys: the menus cancel with Escape, Backspace or K, the field's Circle is Space.
pub const BUTTON_KEYS: &[(&str, &[KeyCode])] = &[
    ("cross", &[KeyCode::KeyJ]),
    ("circle", &[KeyCode::Space, KeyCode::Backspace]),
    ("square", &[KeyCode::KeyK]),
    ("triangle", &[KeyCode::KeyX]),
    ("l1", &[KeyCode::KeyQ]),
    ("start", &[KeyCode::Enter]),
    ("select", &[KeyCode::Tab]),
    ("up", &[KeyCode::ArrowUp]),
    ("down", &[KeyCode::ArrowDown]),
    ("left", &[KeyCode::ArrowLeft]),
    ("right", &[KeyCode::ArrowRight]),
];

/// The keys that stand for the pressed buttons of `pad`.
pub fn keys_for(pad: &PadState) -> BTreeSet<KeyCode> {
    BUTTON_KEYS.iter().filter(|(name, _)| pad.pressed(name)).flat_map(|(_, keys)| keys.iter().copied()).collect()
}

/// The original's stick conversion (`FUN_00167198`): per axis the byte minus the centre (`DAT_0037968c`), the dead zone taken off
/// toward zero, divided by `centre - dead zone`; the vector's length is capped at `DAT_00379690`. The dead zone is `DAT_00522168`,
/// which is BSS: `FUN_00166fd0` fills it at start through `FUN_00167058(DAT_00379680, ..)`, so it is read from `DAT_00379680` in the
/// executable image. Each constant is read on its own; one that cannot be read falls back alone (centre 127, dead zone 0, cap 1) and
/// [`StickScale::from_exe`] names it.
#[derive(Resource, Clone, Copy, Debug, PartialEq)]
pub struct StickScale {
    pub centre: f32,
    pub dead_zone: f32,
    pub cap: f32,
}

impl Default for StickScale {
    fn default() -> StickScale {
        StickScale { centre: f32::from(CENTRE), dead_zone: 0.0, cap: 1.0 }
    }
}

impl StickScale {
    /// The constants from the executable image, and the names of the ones that fell back to the default.
    pub fn from_exe(exe: &[u8]) -> (StickScale, Vec<&'static str>) {
        let f = |va: u32| crate::reaction_rules::exe_f32(exe, va);
        let d = StickScale::default();
        let mut missing = Vec::new();
        let mut pick = |name: &'static str, v: Option<f32>, default: f32| {
            v.unwrap_or_else(|| {
                missing.push(name);
                default
            })
        };
        let s = StickScale { centre: pick("centre 0x37968c", f(0x37968c), d.centre), dead_zone: pick("dead zone 0x379680", f(0x379680), d.dead_zone), cap: pick("cap 0x379690", f(0x379690), d.cap) };
        (s, missing)
    }
}

/// One stick as (unit direction with y up, magnitude): `FUN_00167198` leaves the dead-zone-corrected axes normalised by
/// `centre - dead zone` and the vector length capped. The direction is the corrected vector over its length [inferred from the
/// routine's VU tail, which divides by the length].
pub fn stick_vector(x: u8, y: u8, s: &StickScale) -> (Vec2, f32) {
    let axis = |b: u8| {
        let a = f32::from(b) - s.centre;
        let r = if a > 0.0 { (a - s.dead_zone).max(0.0) } else { (a + s.dead_zone).min(0.0) };
        r / (s.centre - s.dead_zone)
    };
    // A pad's y grows downward: pushed up is a small byte.
    let v = Vec2::new(axis(x), -axis(y));
    let len = v.length();
    if len > 0.0 { (v / len, len.min(s.cap)) } else { (Vec2::ZERO, 0.0) }
}

/// The pad script of `KH2_PAD`.
#[derive(Resource, Clone, Debug)]
pub struct RigScript(pub PadScript);

/// The rig clock. `ticks` is `k` of the tick that is running (or ran last).
#[derive(Resource, Default, Clone, Copy, Debug, PartialEq, Eq)]
pub struct RigClock {
    pub started: bool,
    /// Sora stands where the state had him (set at once when the state has no rig `sora`).
    pub placed: bool,
    pub ticks: u32,
}

impl RigClock {
    /// The `k` of the next tick to run: what the pad of the coming `FixedUpdate` is keyed on.
    pub fn next_k(&self) -> u32 {
        if self.started { self.ticks + 1 } else { 0 }
    }
}

/// `KH2_PAD`: reads the script at plugin build, as `PadScript::parse` does; a bad file stops the run with the line named.
pub fn load_script(path: &str) -> Result<PadScript, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("KH2_PAD={path}: {e}"))?;
    PadScript::parse(&text).map_err(|e| format!("KH2_PAD={path}: {e}"))
}

/// Pad script into `PadState`, then the mirrored keys (PreUpdate, before the menu reads its keys).
fn drive_pad(script: Res<RigScript>, clock: Res<RigClock>, mut pad: ResMut<PadState>, mut keys: ResMut<ButtonInput<KeyCode>>, mut held: Local<BTreeSet<KeyCode>>) {
    // Before the script's first line the pad is idle (no keyboard in a rig run).
    *pad = PadState::from_frame(script.0.at(clock.next_k()).unwrap_or_default());
    let want = keys_for(&pad);
    for k in held.difference(&want).copied().collect::<Vec<_>>() {
        keys.release(k);
        held.remove(&k);
    }
    for k in &want {
        if held.insert(*k) {
            keys.press(*k);
        }
    }
}

/// Everything that must stand before `k = 0`: the area has run its scripts (and Sora was put at the entrance when it asks), Sora exists.
fn area_ready(rec: Option<&AreaRecord>, sora: bool) -> bool {
    sora && rec.is_some_and(|r| r.run.is_some() && (!r.place_on_load || r.placed.is_some()))
}

/// Puts Sora where the state's `sora` key says (game units and axes) once the area has spawned and placed him at its entrance.
fn place_sora(progress: Option<Res<Progress>>, rec: Option<Res<AreaRecord>>, mut sora: Query<(&mut Transform, &mut Sora)>, mut clock: ResMut<RigClock>) {
    if clock.placed {
        return;
    }
    let want = progress.as_ref().and_then(|p| p.rig.sora);
    let Some(want) = want else {
        clock.placed = true;
        return;
    };
    if !area_ready(rec.as_deref(), sora.single().is_ok()) {
        return;
    }
    let Ok((mut tf, mut s)) = sora.single_mut() else { return };
    let at = game_to_app(want.pos);
    crate::shared::sora::respawn(&mut s, at);
    // The facing is a unit vector in the game's axes; the app's forward is (sin yaw, 0, cos yaw) with z mirrored.
    s.motor.yaw = want.facing[0].atan2(-want.facing[2]);
    tf.translation = at * SCALE;
    tf.rotation = Quat::from_rotation_y(s.motor.yaw);
    info!("rig: Sora put at game ({:.2}, {:.2}, {:.2}) facing ({:.3}, {:.3}) from the state", want.pos[0], want.pos[1], want.pos[2], want.facing[0], want.facing[2]);
    clock.placed = true;
}

/// FixedFirst (once per vblank, before the logic of `schedule::Vblank`): starts the clock on the first vblank after the area stands,
/// then counts every vblank.
fn rig_clock(mut clock: ResMut<RigClock>, rec: Option<Res<AreaRecord>>, sora: Query<(), With<Sora>>) {
    if clock.started {
        clock.ticks += 1;
    } else if clock.placed && area_ready(rec.as_deref(), !sora.is_empty()) {
        clock.started = true;
        clock.ticks = 0;
    }
}

/// Which `k` the trace records.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TraceAt {
    Every,
    List(BTreeSet<u32>),
}

impl TraceAt {
    pub fn parse(spec: &str) -> TraceAt {
        if spec.trim() == "every" {
            TraceAt::Every
        } else {
            TraceAt::List(spec.split(',').filter_map(|v| v.trim().parse().ok()).collect())
        }
    }

    pub fn wants(&self, k: u32) -> bool {
        match self {
            TraceAt::Every => true,
            TraceAt::List(l) => l.contains(&k),
        }
    }

    pub fn last(&self) -> Option<u32> {
        match self {
            TraceAt::Every => None,
            TraceAt::List(l) => l.last().copied(),
        }
    }
}

/// `KH2_TRACE`: the names, the checkpoints, the file, and the k the run ends at.
#[derive(Resource)]
pub struct RigTrace {
    pub names: Vec<String>,
    pub at: TraceAt,
    pub end: Option<u32>,
    out: std::io::BufWriter<std::fs::File>,
    warned: BTreeSet<String>,
}

impl RigTrace {
    pub fn create(path: &str, names: Vec<String>, at: TraceAt, end: Option<u32>) -> Result<RigTrace, String> {
        let mut out = std::io::BufWriter::new(std::fs::File::create(path).map_err(|e| format!("trace file {path}: {e}"))?);
        writeln!(out, "{}", kh2_rig::trace::HEADER).map_err(|e| e.to_string())?;
        Ok(RigTrace { names, at, end, out, warned: BTreeSet::new() })
    }

    /// The rows of checkpoint `k`, one per name (a value the port has no field for is an empty row, like a null pointer).
    pub fn record(&mut self, k: u32, ctx: &Ctx) {
        for name in &self.names {
            let v = rig_values::read(name, ctx);
            if v.is_none() && self.warned.insert(name.clone()) {
                warn!("rig: trace value '{name}' has no value in the port (unknown name, or no object yet); its rows stay empty");
            }
            let _ = writeln!(self.out, "{k},{name},{},{k}", v.unwrap_or_default());
        }
        let _ = self.out.flush();
    }
}

#[allow(clippy::too_many_arguments)]
fn rig_trace(
    clock: Res<RigClock>,
    mut trace: ResMut<RigTrace>,
    sora: Query<&Sora>,
    area: Option<Res<AreaRecord>>,
    progress: Option<Res<Progress>>,
    cam: Query<(&Transform, &crate::camera::OrbitCamera)>,
    (phase, menu, anims): (Option<Res<MenuPhase>>, Option<Res<RigMenu>>, Query<&crate::skel::Animator>),
    frame: Option<Res<crate::schedule::FrameClock>>,
    mut exit: MessageWriter<AppExit>,
) {
    if !clock.started {
        return;
    }
    let k = clock.ticks;
    if trace.at.wants(k) {
        let sora = sora.single().ok();
        // The playing motion: the slot is `id * 4 + variant` (contract.md, the controller's +0x14 holds the id), the frame is the animator's.
        let motion = sora.and_then(|s| anims.get(s.body).ok()).and_then(|a| Some((i32::try_from(a.slot? / 4).ok()?, a.frame)));
        let menu = phase.as_deref().zip(menu.as_deref());
        let ctx = Ctx { k, sora, area: area.as_deref(), progress: progress.as_deref(), cam: cam.single().ok().map(|c| c.0), cam_wanted: cam.single().ok().and_then(|c| c.1.wanted), menu, motion, vsync: frame.as_deref().map(|f| &f.vsync) };
        trace.record(k, &ctx);
    }
    if trace.end.is_some_and(|e| k >= e) {
        info!("rig: trace finished at k {k}");
        exit.write(AppExit::Success);
    }
}

/// The start area for a `KH2_STATE` file with a `place` key: the state's world and area name the area file (Twilight Town and the
/// world map only), taken over the command line's; the entrance goes to `KH2_ENTRANCE` unless that is set. Without the key `start`
/// is returned as it is.
pub fn start_area(start: String) -> String {
    let Some(place) = std::env::var("KH2_STATE").ok().and_then(|p| place_of_file(&p)) else { return start };
    match crate::area::area_name(place[0], place[1]) {
        Some(name) => {
            if std::env::var_os("KH2_ENTRANCE").is_none() {
                // SAFETY: called from `run` before the app (and any thread that reads the environment) starts.
                unsafe { std::env::set_var("KH2_ENTRANCE", place[2].to_string()) };
            }
            info!("rig: the state's place (world {}, area {}, entrance {}) starts the area {name}", place[0], place[1], place[2]);
            name
        }
        None => {
            warn!("rig: the state's place (world {}, area {}) is not in this app; the area stays {start}", place[0], place[1]);
            start
        }
    }
}

fn place_of_file(path: &str) -> Option<[u8; 3]> {
    let j = Json::parse(&std::fs::read_to_string(path).ok()?).ok()?;
    let p = j.get("place")?;
    let b = |k: &str| p.get(k).and_then(Json::int).and_then(|n| u8::try_from(n).ok());
    Some([b("world")?, b("area")?, b("entrance")?])
}

pub struct RigPlugin;

impl Plugin for RigPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<PadState>().init_resource::<RigClock>().init_resource::<StickScale>();
        match app.world().get_resource::<Disc>().and_then(|d| d.read_root("SLUS_210.05")) {
            Some(exe) => {
                let (scale, missing) = StickScale::from_exe(&exe);
                if !missing.is_empty() {
                    error!("rig: stick constants not readable from SLUS_210.05, defaults used for: {missing:?}; scripted sticks will differ from the original");
                }
                info!("rig: stick scale centre {} dead zone {} cap {}", scale.centre, scale.dead_zone, scale.cap);
                app.insert_resource(scale);
            }
            None => error!("rig: SLUS_210.05 not readable; the stick has the default scale (centre 127, no dead zone, cap 1) and differs from the original"),
        }
        app.add_systems(Update, place_sora.after(crate::area::run_area_script).before(crate::schedule::Frame::Pad));
        let script = std::env::var("KH2_PAD").ok().filter(|p| !p.is_empty());
        let names = std::env::var("KH2_TRACE").ok().map(|v| v.split(',').map(|n| n.trim().to_string()).filter(|n| !n.is_empty()).collect::<Vec<_>>()).filter(|n| !n.is_empty());
        if script.is_none() && names.is_none() {
            return;
        }
        let script = script.map(|p| load_script(&p).unwrap_or_else(|e| fail(&e)));
        app.add_systems(FixedFirst, rig_clock);
        if let Some(s) = &script {
            if !s.odd_changes().is_empty() {
                warn!("rig: the pad script changes at odd k {:?}; the original's field reads the pad every second vsync", s.odd_changes());
            }
            app.insert_resource(RigScript(s.clone()))
                .add_systems(PreUpdate, drive_pad.in_set(crate::harness::Keys).after(bevy::input::InputSystems));
        }
        if let Some(names) = names {
            let at = TraceAt::parse(&std::env::var("KH2_TRACE_AT").unwrap_or_else(|_| "every".into()));
            let end = std::env::var("KH2_TRACE_END").ok().and_then(|v| v.parse().ok()).or_else(|| script.as_ref().map(PadScript::last_k)).or_else(|| at.last());
            let file = std::env::var("KH2_TRACE_FILE").unwrap_or_else(|_| "trace-port.csv".into());
            let trace = RigTrace::create(&file, names, at, end).unwrap_or_else(|e| fail(&e));
            app.insert_resource(trace).add_systems(FixedFirst, rig_trace.after(rig_clock));
        }
    }
}

fn fail(msg: &str) -> ! {
    eprintln!("{msg}");
    std::process::exit(2);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::progress::ItemIndex;
    use std::sync::Arc;

    fn script() -> PadScript {
        PadScript::parse("0 - 127 127 127 127\n40 - 127 0 127 127\n120 cross+circle 255 127 127 127\n200 start\n204 -\nend 330\n").unwrap()
    }

    #[test]
    fn a_pad_script_is_applied_per_tick_and_mirrored_onto_keys() {
        let mut app = App::new();
        app.init_resource::<PadState>().init_resource::<ButtonInput<KeyCode>>().insert_resource(RigScript(script()));
        app.insert_resource(RigClock { started: true, placed: true, ticks: 0 }).add_systems(Update, drive_pad);
        // `next_k` = ticks + 1: the pad of the coming tick.
        let at = |app: &mut App, ticks: u32| {
            app.world_mut().resource_mut::<RigClock>().ticks = ticks;
            app.world_mut().resource_mut::<ButtonInput<KeyCode>>().clear();
            app.update();
            (*app.world().resource::<PadState>(), app.world().resource::<ButtonInput<KeyCode>>().clone())
        };
        let (p, _) = at(&mut app, 38);
        assert_eq!((p.ly, p.buttons, p.scripted), (127, 0, true), "k 39 is still");
        let (p, _) = at(&mut app, 39);
        assert_eq!(p.ly, 0, "the line of k 40 takes effect for the tick k = 40");
        let (p, keys) = at(&mut app, 119);
        assert_eq!((p.lx, p.pressed("cross"), p.pressed("circle")), (255, true, true));
        assert!(keys.just_pressed(KeyCode::KeyJ) && keys.just_pressed(KeyCode::Space) && keys.pressed(KeyCode::Backspace));
        let (_, keys) = at(&mut app, 150);
        assert!(keys.pressed(KeyCode::KeyJ) && !keys.just_pressed(KeyCode::KeyJ), "a held button is pressed once");
        let (p, keys) = at(&mut app, 199);
        assert!(p.pressed("start") && keys.just_pressed(KeyCode::Enter) && !keys.pressed(KeyCode::KeyJ), "buttons of the old line are released");
        let (p, keys) = at(&mut app, 203);
        assert_eq!(p.buttons, 0);
        assert!(keys.just_released(KeyCode::Enter) && !keys.pressed(KeyCode::Enter));
        // Before the clock starts the first tick is k 0.
        assert_eq!(RigClock::default().next_k(), 0);
    }

    #[test]
    fn the_stick_goes_through_the_dead_zone_and_the_cap() {
        let s = StickScale { centre: 127.5, dead_zone: 20.0, cap: 1.0 };
        let (d, m) = stick_vector(127, 127, &s);
        assert_eq!((d, m), (Vec2::ZERO, 0.0), "the centre is inside the dead zone");
        let (d, m) = stick_vector(127, 0, &s);
        assert!(d.x.abs() < 1e-6 && (d.y - 1.0).abs() < 1e-6, "stick up is +y: {d:?}");
        assert!((m - 1.0).abs() < 1e-6, "full push is the cap: {m}");
        let (d, _) = stick_vector(255, 127, &s);
        assert!((d.x - 1.0).abs() < 1e-6 && d.y.abs() < 1e-6, "stick right is +x: {d:?}");
        let (_, m) = stick_vector(127 + 40, 127, &s);
        assert!((m - (39.5 - 20.0) / 107.5).abs() < 1e-5, "a half push is corrected by the dead zone: {m}");
        let (_, m) = stick_vector(150, 127, &s);
        assert_eq!(m, (150.0f32 - 127.5 - 20.0).max(0.0) / 107.5, "inside the dead zone plus a little");
        assert_eq!(stick_vector(140, 127, &s).1, 0.0, "12.5 off the centre is dead");
        let capped = StickScale { cap: 0.8, ..s };
        assert_eq!(stick_vector(127, 0, &capped).1, 0.8);
    }

    #[test]
    fn the_trace_places_rows_in_the_port_format() {
        let path = std::env::temp_dir().join(format!("kh2-rig-trace-{}.csv", std::process::id()));
        let mut t = RigTrace::create(path.to_str().unwrap(), vec!["clock.vsync".into(), "place.world".into(), "nope".into()], TraceAt::parse("3,5"), Some(5)).unwrap();
        let mut p = Progress::from_block(&[], Arc::new(ItemIndex::default()));
        p.rig.vsync = Some(1000);
        let ctx = Ctx { k: 3, sora: None, area: None, progress: Some(&p), cam: None, cam_wanted: None, menu: None, motion: None, vsync: None };
        for k in 0..=5 {
            if t.at.wants(k) {
                t.record(k, &Ctx { k, ..ctx });
            }
        }
        drop(t);
        let text = std::fs::read_to_string(&path).unwrap();
        let _ = std::fs::remove_file(&path);
        assert_eq!(text, "k,name,value,stop\n3,clock.vsync,1003,3\n3,place.world,,3\n3,nope,,3\n5,clock.vsync,1005,5\n5,place.world,,5\n5,nope,,5\n");
        let parsed = kh2_rig::trace::Trace::parse(&text).unwrap();
        assert_eq!(parsed.get(5, "clock.vsync").unwrap().value, "1005");
        assert_eq!(TraceAt::parse("every"), TraceAt::Every);
        assert_eq!(TraceAt::parse("4, 9,x").last(), Some(9));
    }

    #[test]
    fn the_clock_starts_after_the_area_and_counts_every_tick() {
        let mut app = App::new();
        app.init_resource::<RigClock>().add_systems(Update, rig_clock);
        app.update();
        assert!(!app.world().resource::<RigClock>().started, "no area, no clock");
        assert!(!area_ready(None, true));
        app.world_mut().resource_mut::<RigClock>().placed = true;
        app.update();
        assert!(!app.world().resource::<RigClock>().started, "placed but the area has not run");
        // The area has run its scripts and Sora stands: the next tick is k 0, and every tick after counts.
        app.insert_resource(AreaRecord { world: 2, area: 21, entrance: 0, programs: [0; 3], programs_override: None, dest_programs: [0xff; 3], place_on_load: false, run: Some(std::sync::Arc::new(crate::area::AreaRun { name: "tt21".into(), ard: Default::default(), script: Default::default() })), placed: None });
        app.world_mut().spawn(Sora::for_test(80, 80));
        app.update();
        let c = *app.world().resource::<RigClock>();
        assert!(c.started && c.ticks == 0 && c.next_k() == 1, "{c:?}");
        for _ in 0..5 {
            app.update();
        }
        assert_eq!(app.world().resource::<RigClock>().ticks, 5, "five ticks after k 0");
    }
}
