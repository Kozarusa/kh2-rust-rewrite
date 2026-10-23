//! The camera of a limit (WP-LS2b): the type 0x16 events of the limit BAR (`limit/us/<file>`, entries "00" and "10"), started by the
//! limit script's host 2.50. See docs/systems/drive-limits-summons/limits.md "Limit camera (WP-LS2b)".
//!
//! Host 2.50 (`0x18e9f8` -> `FUN_001b3468`) is not an effect start. It looks the event up in the limit file
//! (`FUN_00168510(bar, 0x16, name, 0)`, name = the script's four byte literal) and plays it as an event object of kind 0xb
//! (`FUN_00195898(obj, 0xb, data)`, `FUN_00195a78(obj, player)`, `FUN_00195ae0(obj)`): the same event player an area's events use, so
//! the camera track is `intro::Track` (opcodes 0x13 keys, 6 cuts, 8 end frame, 0x46 pull-in) read at the event frame, which is the game
//! tick count times 0.5 (`FUN_00222728`).
//!
//! Where the keys are [inferred]: the original evaluates them in the event matrix `DAT_0036bce8` (identity: `FUN_0011b460` copies the
//! constant `0x354a60`, nothing else writes the event object's `+0x50`), which would put the camera at the area's origin. The keys of every
//! limit event frame a figure standing at the origin (look-at heights 56 to 130 cm, positions within a few metres), so the app puts them in the
//! player's frame at the moment the event starts: the origin at Sora's feet, the key axis +z along his facing (`Motor::forward`).
//! A swing from behind his shoulder to the front (Trinity "00": z from -125 to +129) reads as a cinematic only that way.
//!
//! Off by default (`KH2_LIMIT_CAMERA=1` turns it on): the shots of Sparrow, Mulan and others frame the participants, which the app
//! cannot show yet (no models), so the view would cut to empty ground; and the frame above is an inference. The data path (script
//! host 2.50, the event lookup, the track, the hand-back) is complete and tested; the switch goes on with the participants' models.
//!
//! Not ported: the held player and the world stop of the event kind (`FUN_00196280`, `FUN_00184b40`), the HUD hide (`FUN_00196760`), the
//! actors of the event (the limit events have none), the spring flag of opcode 0x31 mode 0 (`DAT_0036bcdc`; the field camera takes the last
//! view over, as for the intro).
//!
//! Logs: `limit camera: ...`.

use bevy::prelude::*;
use kh2_ard::parse_event;

use crate::assets::Disc;
use crate::camera::CamEvent;
use crate::camera::rules::View;
use crate::intro::{Shot, Track};
use crate::limit_script::LimitEvent;
use crate::schedule::Sim;
use crate::sora::Sora;

/// BAR entry kind of an event.
pub const EVENT_KIND: u16 = 0x16;

/// Event frames per game tick (`FUN_00222728`: the counter grows by `DAT_00356f40` = 1.0 a frame, times `DAT_0037ec50` = 0.5).
pub const FRAMES_PER_TICK: f32 = 0.5;

/// The track of the event `name` in the limit file's bytes (`FUN_00168510(bar, 0x16, name, 0)`: the first entry of that kind and name).
pub fn event_track(bar: &[u8], name: &str) -> Option<Track> {
    let entries = kh2_bar::parse(bar).ok()?;
    let e = entries.iter().find(|e| e.kind == EVENT_KIND && e.tag == name)?;
    Some(Track::parse(&parse_event(e.data)?))
}

/// The space the keys are in [inferred, see the module doc]: the player's feet and facing when the event starts.
pub fn key_frame(feet: Vec3, yaw: f32) -> Mat4 {
    Mat4::from_rotation_translation(Quat::from_rotation_y(yaw), feet)
}

/// The last event frame: opcode 8's end frame (`FUN_00226fb0`, `FUN_00222668`), else the last frame a cut shows.
pub fn end_frame(track: &Track) -> i32 {
    if track.end >= 0 { track.end } else { track.cuts.iter().map(|c| i32::from(c.end)).max().unwrap_or(0) }
}

/// A limit event playing.
struct Run {
    file: String,
    name: String,
    track: Track,
    frame: Mat4,
    ticks: u32,
    last: Option<Shot>,
}

impl Run {
    /// The view at the current tick, or `None` once the event is over (the frame counter reached the end frame).
    fn step(&mut self, hold: Option<f32>) -> Option<Shot> {
        let f = (self.ticks as f32 * FRAMES_PER_TICK).min(hold.unwrap_or(f32::MAX));
        self.ticks += 1;
        if f >= end_frame(&self.track) as f32 {
            return None;
        }
        // Between cuts no camera call is made (`FUN_0022b0f8`): the view stays where it was.
        if let Some(s) = self.track.at(f) {
            self.last = Some(Shot { pos: self.frame.transform_point3(s.pos), look: self.frame.transform_point3(s.look), ..s });
        }
        self.last
    }
}

#[derive(Resource, Default)]
pub struct LimitCamera {
    run: Option<Run>,
}

pub struct LimitCameraPlugin;

impl Plugin for LimitCameraPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<LimitCamera>().add_systems(FixedUpdate, run.in_set(Sim::EffectTick).run_if(crate::schedule::sim_running));
    }
}

fn enabled() -> bool {
    std::env::var("KH2_LIMIT_CAMERA").is_ok_and(|v| v != "0")
}

fn run(disc: Option<Res<Disc>>, mut events: MessageReader<LimitEvent>, sora: Query<&Sora>, mut lc: ResMut<LimitCamera>, mut cam: ResMut<CamEvent>) {
    for e in events.read() {
        if !enabled() {
            continue;
        }
        let Some(disc) = disc.as_deref() else { continue };
        let Some(track) = disc.read(&format!("limit/us/{}", e.file)).and_then(|b| event_track(&b, &e.name)) else {
            info!("limit camera: {} has no readable event {:?}", e.file, e.name);
            continue;
        };
        if !track.has_camera() {
            info!("limit camera: event {:?} of {} has no camera track", e.name, e.file);
            continue;
        }
        let Ok(s) = sora.single() else { continue };
        // A running event ends when another starts (`FUN_002232c8` calls `FUN_00223698` while `FUN_00196418` holds).
        info!(
            "limit camera: {} event {:?}: {} camera record(s), cuts {:?}, end frame {}, map pull-in {:?}; keys in the player's frame at {:.0} facing {:.2} [inferred]",
            e.file,
            e.name,
            track.cams.len(),
            track.cuts.iter().map(|c| (c.cam, c.start, c.end)).collect::<Vec<_>>(),
            end_frame(&track),
            track.collide,
            s.motor.pos,
            s.motor.yaw
        );
        lc.run = Some(Run { file: e.file.clone(), name: e.name.clone(), track, frame: key_frame(s.motor.pos, s.motor.yaw), ticks: 0, last: None });
    }
    let Some(r) = lc.run.as_mut() else { return };
    // `KH2_LIMIT_CAMERA_HOLD=<event frame>` holds the camera at that frame (a harness aid for screenshots).
    let hold = std::env::var("KH2_LIMIT_CAMERA_HOLD").ok().and_then(|v| v.parse::<f32>().ok());
    match r.step(hold) {
        Some(s) => cam.track = Some((View { look: s.look, pos: s.pos, fov: s.fov, roll: s.roll }, r.track.collide.is_some())),
        None => {
            info!("limit camera: {} event {:?} over after {} ticks: the field camera takes over", r.file, r.name, r.ticks);
            lc.run = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::intro::{Channels, Cut, Key};

    fn constant(v: f32) -> Vec<Key> {
        vec![Key { kind: 0, time: 0, value: v, tin: 0.0, tout: 0.0 }]
    }

    /// A camera at (0, 24, -125) looking at (0, 126, -50), as Trinity's "00" starts.
    fn track(end: i32) -> Track {
        let ch: Channels = [constant(0.0), constant(24.0), constant(-125.0), constant(0.0), constant(126.0), constant(-50.0), constant(0.0), constant(45.0)];
        Track { cams: vec![(0, ch)], cuts: vec![Cut { cam: 0, start: 0, end: end as i16 }], end, ..Track::default() }
    }

    /// The keys are in the player's frame: a camera at z = -125 stands 125 cm behind a player who faces +z, whichever way he turns, and
    /// the origin is his feet.
    #[test]
    fn the_keys_are_in_the_players_frame() {
        let feet = Vec3::new(300.0, 40.0, -80.0);
        for yaw in [0.0f32, 1.0, 2.5, -1.7] {
            let m = key_frame(feet, yaw);
            let cam = m.transform_point3(Vec3::new(0.0, 24.0, -125.0));
            let fwd = Vec3::new(yaw.sin(), 0.0, yaw.cos());
            let rel = cam - feet;
            assert!((rel.dot(fwd) + 125.0).abs() < 1e-3, "yaw {yaw}: {rel}");
            assert!((rel.y - 24.0).abs() < 1e-4);
            assert!((m.transform_point3(Vec3::ZERO) - feet).length() < 1e-4);
        }
    }

    /// The event frame is half the tick count; the camera holds its view for the event's length and gives up at the end frame.
    #[test]
    fn the_event_runs_to_its_end_frame_and_hands_the_camera_back() {
        let mut r = Run { file: "x.bar".into(), name: "00".into(), track: track(10), frame: Mat4::IDENTITY, ticks: 0, last: None };
        let first = r.step(None).expect("frame 0 shows");
        assert!((first.pos - Vec3::new(0.0, 24.0, -125.0)).length() < 1e-3 && (first.fov - 45f32.to_radians()).abs() < 1e-5);
        // Ticks 1..19 are event frames 0.5..9.5; tick 20 reaches frame 10.
        for _ in 1..20 {
            assert!(r.step(None).is_some());
        }
        assert!(r.step(None).is_none(), "frame 10 is the end frame");
    }

    /// `KH2_LIMIT_CAMERA_HOLD` keeps the event on one frame however many ticks pass.
    #[test]
    fn a_held_frame_does_not_advance() {
        let mut r = Run { file: "x.bar".into(), name: "00".into(), track: track(200), frame: Mat4::IDENTITY, ticks: 0, last: None };
        for _ in 0..50 {
            assert!(r.step(Some(3.0)).is_some());
        }
    }

    /// The limit files' events (disc): "00" and "10" are camera tracks with a cut table and an end frame; the files without an event
    /// give none. Auron's "00" ends at event frame 115, Trinity's at 333 (opcode 8).
    #[test]
    fn the_limit_files_hold_camera_events() {
        let Some(iso) = crate::assets::iso_path().ok().filter(|p| p.exists()) else {
            eprintln!("skipped: no disc (set KH2_ISO)");
            return;
        };
        let disc = Disc::open(&iso);
        let bar = |f: &str| disc.read(&format!("limit/us/{f}")).unwrap_or_else(|| panic!("limit/us/{f}"));
        let auron = bar("auron.bar");
        let a0 = event_track(&auron, "00").expect("auron event 00");
        assert!(a0.has_camera() && end_frame(&a0) == 115, "{:?} {}", a0.cuts, end_frame(&a0));
        let a10 = event_track(&auron, "10").expect("auron event 10");
        assert!(a10.has_camera() && end_frame(&a10) == 190);
        let t0 = event_track(&bar("trinity.bar"), "00").expect("trinity event 00");
        assert!(t0.has_camera() && end_frame(&t0) == 333);
        assert!(event_track(&bar("donald.bar"), "00").is_none(), "Donald's file has no events");
        assert!(event_track(&auron, "99").is_none());
    }
}
