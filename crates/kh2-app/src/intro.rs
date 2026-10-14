//! The battle intro (WP-I1): the camera track of a mission's start event, played while the start layout runs.
//! See docs/struggle.md "The intro camera" and docs/camera.md "Event camera".

/// One key of an event camera channel (16 bytes, `FUN_0022ab08` / `FUN_0022a9d8`): word 0 is the key type in its
/// top three bits and the time in its low 29 (signed, 1/512 s), then the value, the incoming and the outgoing
/// tangent.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Key {
    pub kind: u32,
    pub time: i32,
    pub value: f32,
    pub tin: f32,
    pub tout: f32,
}

impl Key {
    pub fn parse(b: &[u8]) -> Option<Key> {
        let w = |i: usize| b.get(4 * i..4 * i + 4).map(|s| u32::from_le_bytes([s[0], s[1], s[2], s[3]]));
        let w0 = w(0)?;
        Some(Key { kind: w0 >> 29, time: ((w0 << 3) as i32) >> 3, value: f32::from_bits(w(1)?), tin: f32::from_bits(w(2)?), tout: f32::from_bits(w(3)?) })
    }
}

/// `FUN_0022a9d8`: the value between key `i` and key `i + 1` at `s` seconds: type 0 holds key `i`, 1 is linear,
/// 2..4 a cubic Hermite with key `i`'s outgoing and key `i + 1`'s incoming tangent (per second), other types 0.
fn segment(keys: &[Key], i: usize, s: f32) -> f32 {
    const UNIT: f32 = 1.0 / 512.0; // 0x37f12c
    let (k, n) = (keys[i], keys[i + 1]);
    let (a, b) = (k.time as f32 * UNIT, n.time as f32 * UNIT);
    match k.kind {
        0 => k.value,
        1 => k.value + (n.value - k.value) * (s - a) / (b - a),
        2..=4 => {
            let u = s - a;
            let inv = 1.0 / (b - a);
            let u2 = u * u;
            let f6 = u2 * inv;
            let f5 = u2 * u * inv * inv;
            let f3 = u2 * 3.0 * inv * inv;
            let f4 = (f5 + f5) * inv;
            n.tin * (f5 - f6) + k.tout * ((f5 - (f6 + f6)) + u) + n.value * (f3 - f4) + k.value * ((f4 - f3) + 1.0)
        }
        _ => 0.0,
    }
}

/// `FUN_0022ab08`: one channel at `t` event frames (30 a second, 0x37f138; key times are 1/512 s, 0x37f13c).
/// Before the first key and after the last the value runs on along a straight line: the slope to the
/// neighbouring key for a linear key, else the key's own tangent, taken per key time unit (as the code does).
pub fn channel(keys: &[Key], t: f32) -> f32 {
    let s = t / 30.0;
    match keys.len() {
        0 => return 0.0,
        1 => return keys[0].value,
        _ => {}
    }
    let ti = (s * 512.0) as i32;
    let first = keys[0];
    if ti <= first.time {
        let dt = (keys[1].time - first.time) as f32;
        let slope = if first.kind == 1 && dt != 0.0 { (keys[1].value - first.value) / dt } else { first.tin };
        return first.value - (first.time - ti) as f32 * slope;
    }
    let n = keys.len();
    let last = keys[n - 1];
    if last.time <= ti {
        let dt = (last.time - keys[n - 2].time) as f32;
        let slope = if last.kind == 1 && dt != 0.0 { (last.value - keys[n - 2].value) / dt } else { last.tout };
        return last.value + (ti - last.time) as f32 * slope;
    }
    // The key with the exact time gives its value; otherwise the segment around the time.
    if let Some(k) = keys.iter().find(|k| k.time == ti) {
        return k.value;
    }
    let i = keys.iter().rposition(|k| k.time < ti).unwrap_or(0).min(n - 2);
    segment(keys, i, s)
}

/// The event camera's channels in the order the code reads them (`FUN_0022adb8`): position x, y, z, look-at x,
/// y, z, roll (degrees) and field of view (degrees). The code negates y and z into the game's Y-down space; the
/// app's space turns the game's half a turn about X, so the keys are the app's coordinates as they stand (cm).
pub type Channels = [Vec<Key>; 8];

/// What a camera record gives at a time: position and look-at (app space, cm), roll and horizontal field of view
/// (radians).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Shot {
    pub pos: bevy::math::Vec3,
    pub look: bevy::math::Vec3,
    pub roll: f32,
    pub fov: f32,
}

/// `FUN_0022adb8` then `FUN_0022b0f8`'s hand-off: a channel without keys keeps the start value (0, the field of
/// view 1.0 at 0x37f154); the roll goes through pi / 180 (0x37f144 / 0x37f148), the field of view through
/// 0x37f15c (pi / 180).
pub fn shot(ch: &Channels, t: f32) -> Shot {
    let v = |i: usize, default: f32| if ch[i].is_empty() { default } else { channel(&ch[i], t) };
    use bevy::math::Vec3;
    Shot {
        pos: Vec3::new(v(0, 0.0), v(1, 0.0), v(2, 0.0)),
        look: Vec3::new(v(3, 0.0), v(4, 0.0), v(5, 0.0)),
        roll: v(6, 0.0).to_radians(),
        fov: v(7, 1.0).to_radians(),
    }
}

/// One entry of the cut table (event opcode 6, `FUN_0022a970` -> the 128 entries at `DAT_0036bbb4`): camera
/// record `cam` is drawn from event frame `start` to `end` (both included, `FUN_0022b0f8`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Cut {
    pub cam: i16,
    pub start: i16,
    pub end: i16,
}

/// The camera part of an event (the setup reader `FUN_002245d0` and the timeline thread `FUN_00223a88`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Track {
    /// Opcode 0x13 (`FUN_0022b4f8`): camera records by their index (+4).
    pub cams: Vec<(i16, Channels)>,
    pub cuts: Vec<Cut>,
    /// Opcode 8 (`FUN_00226fb0`): the end frame.
    pub end: i32,
    /// Opcode 0x31 with mode 1 (`FUN_0022bc40` -> `FUN_0022b590`): from this frame the camera flies to the field
    /// camera's place behind the player, reached at the end frame.
    pub follow_at: Option<i32>,
    /// Opcode 0x46 (`FUN_0022bce0`): the camera is pulled in against the map (`DAT_0036bce4`), with the blockers
    /// when its +6 is 1 (`DAT_0036bce5`).
    pub collide: Option<bool>,
    /// Opcode 0x4c (`FUN_0022bcd0`): the springs run on the camera (`DAT_0036bcdb`).
    pub smooth: bool,
    /// Opcode 0x3c (`FUN_00226188`): (frame, effect number, mode) of the event's effect group (mode 0 starts it).
    pub effects: Vec<(i32, u16, i16)>,
}

impl Track {
    pub fn parse(cmds: &[kh2_ard::EventCommand]) -> Track {
        let mut t = Track { end: -1, ..Track::default() };
        let h = |d: &[u8], o: usize| d.get(o..o + 2).map_or(0, |b| i16::from_le_bytes([b[0], b[1]]));
        for c in cmds {
            let d = &c.data;
            match c.opcode {
                8 => t.end = h(d, 0) as i32,
                6 => t.cuts.push(Cut { cam: h(d, 0), start: h(d, 2), end: h(d, 4) }),
                0x13 => {
                    let ch: Channels = std::array::from_fn(|k| {
                        let (first, n) = (h(d, 2 + 4 * k) as u16 as usize, h(d, 4 + 4 * k).max(0) as usize);
                        (0..n).filter_map(|i| d.get(0x24 + 16 * (first + i)..).and_then(Key::parse)).collect()
                    });
                    t.cams.push((h(d, 0), ch));
                }
                0x31 if h(d, 2) == 1 => t.follow_at = Some(h(d, 0) as i32),
                0x46 => t.collide = Some(h(d, 2) == 1),
                0x4c => t.smooth = true,
                0x3c => t.effects.push((h(d, 0) as i32, h(d, 2) as u16, h(d, 4))),
                _ => {}
            }
        }
        t
    }

    pub fn has_camera(&self) -> bool {
        !self.cams.is_empty() && !self.cuts.is_empty()
    }

    /// `FUN_0022b0f8` at event frame `f` (fractional): the last cut whose range holds the frame, evaluated at the
    /// frame minus the cut's start.
    pub fn at(&self, f: f32) -> Option<Shot> {
        let fi = f.floor() as i32;
        let cut = self.cuts.iter().rfind(|c| c.start as i32 <= fi && fi <= c.end as i32)?;
        let (_, ch) = self.cams.iter().find(|(i, _)| *i == cut.cam)?;
        Some(shot(ch, f - cut.start as f32))
    }
}

/// `FUN_0022b590` / `FUN_0022af88`: the fly to the field camera: two keys of type 2 with zero tangents, at the
/// frame of opcode 0x31 (the view drawn then) and at the end frame (the field camera placed behind the player,
/// `FUN_00140e90`), kept in the player's space; between them a cubic ease (Hermite with zero tangents). Key times
/// are whole 1/512 s (`(frame << 9) / 30`).
pub fn follow(from: Shot, to: Shot, start: i32, end: i32, f: f32) -> Shot {
    let (a, b) = (((start << 9) / 30) as f32 / 512.0, ((end << 9) / 30) as f32 / 512.0);
    let s = f / 30.0;
    let tau = if b > a { ((s - a) / (b - a)).clamp(0.0, 1.0) } else { 1.0 };
    let w = tau * tau * (3.0 - 2.0 * tau);
    Shot { pos: from.pos.lerp(to.pos, w), look: from.look.lerp(to.look, w), roll: from.roll + (to.roll - from.roll) * w, fov: from.fov + (to.fov - from.fov) * w }
}

// ---------------------------------------------------------------------------------------------------
// The intro in the app

use std::sync::Arc;

use bevy::prelude::*;

use crate::assets::Disc;
use crate::camera::CamEvent;
use crate::camera::rules::View;
use crate::struggle::{Mission, MissionStart};

pub struct IntroPlugin;

impl Plugin for IntroPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Intro>()
            .add_systems(Update, standalone)
            .add_systems(FixedUpdate, tick.in_set(crate::schedule::Sim::EffectTick).run_if(crate::schedule::sim_running));
    }
}

/// The start event of a mission playing with a camera track.
struct Run {
    key: (u32, u8),
    name: String,
    track: Track,
    /// The player's place when the event started: the space the track's keys are in (see `Frame`).
    origin: Mat4,
    /// The fly-back's two ends (opcode 0x31), fixed at its first frame.
    follow: Option<(Shot, Shot)>,
    last: Option<Shot>,
    started: Vec<bool>,
    pax: Option<Arc<crate::fx::LoadedPax>>,
}

#[derive(Resource, Default)]
pub struct Intro {
    run: Option<Run>,
    /// `KH2_INTRO`: the mission already sent.
    sent: bool,
}

/// The space of an event's camera keys (`FUN_0022b0f8`'s else branch): the event's matrix `DAT_0036bce8`, the
/// event object's +0x50 that `FUN_00195898` fills with `FUN_0011b460` from 0x354a60 (zero in the file, filled at
/// run time) [inferred: the unit matrix, so the keys are map coordinates; the reference clip's backgrounds agree,
/// docs/camera.md "The intro camera"]. Only the fly-back of opcode 0x31 works in the player's space
/// (`FUN_0016aa08`, `DAT_0035c998`).
const KEY_SPACE: Mat4 = Mat4::IDENTITY;
fn place(m: &Mat4, s: Shot) -> Shot {
    Shot { pos: m.transform_point3(s.pos), look: m.transform_point3(s.look), ..s }
}

fn view(s: Shot) -> View {
    View { look: s.look, pos: s.pos, fov: s.fov, roll: s.roll }
}

fn shot_of(v: View) -> Shot {
    Shot { pos: v.pos, look: v.look, roll: v.roll, fov: v.fov }
}

/// The intro of the mission's start layout (record 1): its event's camera track drives the camera
/// (`CamEvent::track`), its opcode-0x3c effects start from the mission's effect file (BAR type 0x12, the
/// manager's +0xc group, `FUN_001db4d0` in `FUN_001b8ee8`) at their frames. The event's frame is the game frame
/// count times 0.5 (`FUN_00222728`).
fn tick(mut commands: Commands, disc: Res<Disc>, mission: Res<Mission>, mut intro: ResMut<Intro>, mut cam: ResMut<CamEvent>) {
    let play = mission.m.as_deref().and_then(|a| a.play.as_ref().filter(|p| p.what == "start").map(|p| (a, p)));
    let Some((a, p)) = play else {
        if let Some(r) = intro.run.take() {
            info!("intro: {} start event over: the camera goes back to the field camera", r.name);
        }
        return;
    };
    let key = (a.id, p.layout.entry);
    if intro.run.as_ref().is_none_or(|r| r.key != key) {
        let track = kh2_bar::parse(&a.file).ok().and_then(|e| e.get(p.layout.entry as usize).and_then(|e| kh2_ard::parse_event(e.data))).map(|c| Track::parse(&c)).unwrap_or_default();
        info!(
            "intro: {} start event (entry {}): {} camera record(s), cuts {:?}, end frame {}, fly-back from frame {:?}, map pull-in {:?}, effects {:?}",
            a.name,
            p.layout.entry,
            track.cams.len(),
            track.cuts.iter().map(|c| (c.cam, c.start, c.end)).collect::<Vec<_>>(),
            track.end,
            track.follow_at,
            track.collide,
            track.effects
        );
        let started = vec![false; track.effects.len()];
        intro.run = Some(Run { key, name: a.name.clone(), track, origin: KEY_SPACE, follow: None, last: None, started, pax: None });
    }
    let Some(r) = intro.run.as_mut() else { return };
    // `KH2_INTRO_HOLD=<event frame>` holds the camera at that frame (a harness aid for screenshots).
    let hold = std::env::var("KH2_INTRO_HOLD").ok().and_then(|v| v.parse::<f32>().ok()).unwrap_or(f32::MAX);
    let f_real = p.ticks as f32 * 0.5;
    let f = f_real.min(hold);
    // `KH2_INTRO_SHOTS=<path prefix>` with `KH2_INTRO_SHOTS_AT=<event frame>,...`: a screenshot at each of those
    // event frames, `<prefix>_<frame>.png` (a harness aid for comparing the intro with the reference clip).
    if let (Ok(prefix), Ok(at)) = (std::env::var("KH2_INTRO_SHOTS"), std::env::var("KH2_INTRO_SHOTS_AT"))
        && f_real.fract() == 0.0
        && at.split(',').any(|x| x.trim().parse::<f32>().ok() == Some(f_real))
    {
        use bevy::render::view::screenshot::{Screenshot, save_to_disk};
        info!("intro: screenshot at event frame {f_real}");
        commands.spawn(Screenshot::primary_window()).observe(save_to_disk(format!("{prefix}_{f_real}.png")));
    }
    if r.track.has_camera() {
        let shot = match (r.track.follow_at, cam.behind) {
            (Some(start), Some(behind)) if f >= start as f32 && r.track.end > start => {
                let (from, to) = *r.follow.get_or_insert_with(|| {
                    let from = r.last.unwrap_or_else(|| shot_of(behind));
                    info!("intro: frame {start}: the camera flies to the field camera behind the player, reached at frame {} (FUN_0022b590)", r.track.end);
                    (from, shot_of(behind))
                });
                Some(follow(from, to, start, r.track.end, f))
            }
            _ => r.track.at(f).map(|s| place(&r.origin, s)),
        };
        if let Some(s) = shot {
            r.last = Some(s);
            cam.track = Some((view(s), r.track.collide.is_some()));
        }
    }
    for (i, &(frame, id, mode)) in r.track.effects.iter().enumerate() {
        if r.started[i] || f_real < frame as f32 {
            continue;
        }
        r.started[i] = true;
        if mode != 0 {
            info!("intro: frame {frame}: effect {id} mode {mode} (FUN_00226188) is not ported");
            continue;
        }
        if r.pax.is_none() {
            let common = disc.read("effect/texcommon.dpd").and_then(|b| kh2_fx::Dpd::parse(&b).ok());
            r.pax = crate::fx::load(&disc, &format!("msn/us/{}.bar", r.name), 0x1000 + a.id as usize, common.as_ref()).map(Arc::new);
        }
        match &r.pax {
            Some(lib) => {
                // FUN_001db868(group, id, 1, 3, 0x9c41, 0): no owner object; the effect stands at the world origin
                // [inferred] and draws itself in front of the camera (pppDrawMatrixFront).
                crate::fx::start(&mut commands, lib, id, crate::fx::Bind::Fixed(Mat4::IDENTITY));
                info!("intro: frame {frame}: effect {id} of {}'s effect file started (event opcode 0x3c, FUN_00226188 -> FUN_001db868)", r.name);
            }
            None => warn!("intro: {} has no effect file for effect {id}", r.name),
        }
    }
}

/// `KH2_INTRO=<mission name>` (`1`: `TT04_MS500`, the prologue's "Defeat Seifer!"): starts that mission's
/// manager three seconds in, wherever Sora stands, so its start layout plays (the intro camera, its effect, the
/// INFORMATION bar) without the area's battle group. A harness aid.
fn standalone(time: Res<Time>, disc: Res<Disc>, mut intro: ResMut<Intro>, mut out: MessageWriter<MissionStart>) {
    if intro.sent || time.elapsed_secs() < 3.0 {
        return;
    }
    let Ok(v) = std::env::var("KH2_INTRO") else { return };
    intro.sent = true;
    let name = if v == "1" { "TT04_MS500".to_string() } else { v };
    let id = disc
        .read(&format!("msn/us/{name}.bar"))
        .and_then(|f| kh2_bar::parse(&f).ok().and_then(|e| e.iter().find(|e| e.kind == 2).and_then(|e| crate::struggle::parse_header(e.data))).map(|h| h.id as u32));
    match id {
        Some(id) => {
            info!("intro: KH2_INTRO starts mission {name} ({id:#x})");
            out.write(MissionStart { id, name, orb_pool: false, triggers: Vec::new() });
        }
        None => warn!("intro: KH2_INTRO: no mission file msn/us/{name}.bar"),
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use bevy::math::Vec3;

    /// The start event of a mission file (layout record 1: byte 0 of its word is the BAR entry).
    fn start_track(disc: &Disc, name: &str) -> Option<Track> {
        let file = disc.read(&format!("msn/us/{name}.bar"))?;
        let entries = kh2_bar::parse(&file).ok()?;
        let header = crate::struggle::parse_header(entries.iter().find(|e| e.kind == 2)?.data)?;
        let r = header.records.iter().find(|r| r.op == 1)?;
        let e = entries.get((r.args[0] & 0xff) as usize)?;
        Some(Track::parse(&kh2_ard::parse_event(e.data)?))
    }

    fn key(kind: u32, time: i32, value: f32, tin: f32, tout: f32) -> Key {
        Key { kind, time, value, tin, tout }
    }

    #[test]
    fn an_event_camera_channel_steps_lines_and_curves_like_the_game() {
        // Times are 1/512 s; the channel is read at event frames (30 a second).
        let lin = [key(1, 0, 0.0, 0.0, 0.0), key(1, 512, 30.0, 0.0, 0.0)];
        assert!((channel(&lin, 15.0) - 15.0).abs() < 1e-3, "half a second into a 1 s line");
        // Before the first key a linear key runs on along the line to its neighbour (per 1/512 s).
        assert!((channel(&lin, -30.0) + 30.0).abs() < 0.1);
        // A Hermite segment with zero tangents is a smooth step: 0.5 at the middle, the ends exact.
        let h = [key(2, 0, 10.0, 0.0, 0.0), key(2, 512, 20.0, 0.0, 0.0)];
        assert!((channel(&h, 15.0) - 15.0).abs() < 1e-3);
        assert!((channel(&h, 7.5) - (10.0 + 10.0 * 0.15625)).abs() < 1e-3, "3t^2 - 2t^3 at a quarter");
        // A step key holds its value; past the last key the outgoing tangent runs on.
        let s = [key(0, 0, 4.0, 0.0, 0.0), key(0, 512, 8.0, 0.0, 0.5)];
        assert_eq!(channel(&s, 20.0), 4.0);
        assert!((channel(&s, 60.0) - (8.0 + 512.0 * 0.5)).abs() < 1e-2);
        assert_eq!(channel(&[key(1, 0, 3.0, 0.0, 0.0)], 99.0), 3.0, "one key is a constant");
    }

    /// The port against the game's own `FUN_0022ab08` (and `FUN_0022a9d8`) run in kh2-ee on the user's executable,
    /// over every channel of the Seifer intro's camera record and a sweep of frames (the event is not being skipped:
    /// `FUN_00224b18` reads the event object `DAT_0035e3a4`, zero here).
    #[test]
    fn the_camera_keys_evaluate_as_fun_0022ab08_does() {
        let Some(iso) = crate::assets::iso_path().ok().filter(|p| p.exists()) else {
            eprintln!("skipped: no disc (set KH2_ISO)");
            return;
        };
        let disc = Disc::open(&iso);
        let elf = disc.read_root("SLUS_210.05").expect("executable");
        let mut ee = kh2_ee::Ee::new();
        ee.load_elf(&elf).expect("elf");
        let track = start_track(&disc, "TT04_MS500").expect("TT04_MS500 start event");
        let (_, ch) = &track.cams[0];
        const KEYS: u32 = 0x0180_0000;
        const CACHE: u32 = 0x0181_0000;
        let mut worst = 0.0f32;
        for keys in ch.iter().filter(|k| !k.is_empty()) {
            for (i, k) in keys.iter().enumerate() {
                let a = KEYS + 16 * i as u32;
                ee.w32(a, (k.kind << 29) | (k.time as u32 & 0x1fff_ffff));
                ee.wf32(a + 4, k.value);
                ee.wf32(a + 8, k.tin);
                ee.wf32(a + 12, k.tout);
            }
            for f in (-10..150).map(|f| f as f32 * 1.0 + 0.5) {
                ee.call(0x0022_ab08, &[KEYS as u64, keys.len() as u64, CACHE as u64], &[f], 0x01ff_0000).expect("FUN_0022ab08");
                let game = ee.fpr[0];
                let port = channel(keys, f);
                worst = worst.max((game - port).abs() / game.abs().max(1.0));
                assert!((game - port).abs() <= 1e-3 * game.abs().max(1.0), "frame {f}: game {game}, port {port}");
            }
        }
        eprintln!("FUN_0022ab08 against the port: worst relative difference {worst:e}");
    }

    #[test]
    fn the_seifer_intro_is_the_start_event_camera_and_the_struggle_has_none() {
        let Some(iso) = crate::assets::iso_path().ok().filter(|p| p.exists()) else {
            eprintln!("skipped: no disc (set KH2_ISO)");
            return;
        };
        let disc = Disc::open(&iso);
        let t = start_track(&disc, "TT04_MS500").expect("TT04_MS500");
        assert_eq!(t.cuts, vec![Cut { cam: 0, start: 0, end: 130 }]);
        assert_eq!((t.end, t.follow_at, t.collide, t.smooth), (130, Some(103), Some(false), false));
        assert_eq!(t.effects, vec![(120, 0, 0)]);
        for f in [0, 20, 50, 75, 103, 130] {
            eprintln!("frame {f}: {:?}", t.at(f as f32));
        }
        // The last key looks at the map origin from behind it, 5.6 m back.
        let end = t.at(130.0).unwrap();
        assert!(end.look.x.abs() < 1.0 && end.look.z.abs() < 1.0 && end.pos.z < -500.0, "{end:?}");
        // The prologue's other "Defeat Seifer!" (b_42) has a camera too; the Struggle's start event only its picture.
        assert!(start_track(&disc, "TT04_MS108").expect("TT04_MS108").has_camera());
        let s = start_track(&disc, "TT04_MS00").expect("TT04_MS00");
        assert!(!s.has_camera() && s.effects.is_empty(), "{s:?}");
    }

    #[test]
    fn the_fly_back_eases_from_the_event_view_to_the_field_camera() {
        let a = Shot { pos: Vec3::ZERO, look: Vec3::X, roll: 0.0, fov: 1.0 };
        let b = Shot { pos: Vec3::new(100.0, 0.0, 0.0), look: Vec3::Y, roll: 0.0, fov: 2.0 };
        // The first key's time is truncated to 1/512 s, so frame 103 is already a hair past it.
        assert!(follow(a, b, 103, 130, 103.0).pos.x < 0.01);
        assert_eq!(follow(a, b, 103, 130, 100.0).pos, a.pos);
        assert!((follow(a, b, 103, 130, 130.0).pos - b.pos).length() < 1e-3);
        // Key times are whole 1/512 s: (103 << 9) / 30 = 1757, (130 << 9) / 30 = 2218.
        let mid = (1757.0 + 2218.0) / 2.0 / 512.0 * 30.0;
        assert!((follow(a, b, 103, 130, mid).pos.x - 50.0).abs() < 1e-2);
        assert!((follow(a, b, 103, 130, mid).fov - 1.5).abs() < 1e-4);
    }
}