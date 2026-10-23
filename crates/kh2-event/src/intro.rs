//! The battle intro (WP-I1): the camera track of a mission's start event, played while the start layout runs.
//! See docs/systems/world-minigames/struggle.md "The intro camera" and docs/systems/player-camera/camera-event.md "Event camera".

// ---------------------------------------------------------------------------------------------------
// The intro in the app

use std::sync::Arc;

use bevy::prelude::*;

pub use crate::shared::event::{follow, Shot, Track};

/// The event player (WP-E6): a submodule so `main.rs` stays untouched; `IntroPlugin` adds its plugin.
#[path = "event_player.rs"]
pub mod event_player;

use crate::assets::Disc;
use crate::camera::CamEvent;
use crate::camera::rules::View;
use crate::struggle::{Mission, MissionStart};

pub struct IntroPlugin;

impl Plugin for IntroPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(event_player::EventPlayerPlugin)
            .init_resource::<Intro>()
            .add_systems(Update, standalone)
            .add_systems(FixedUpdate, tick.in_set(crate::schedule::Sim::EffectTick).run_if(crate::schedule::sim_running));
    }
}

/// The start event of a mission playing with a camera track.
struct Run {
    key: (u32, u8),
    name: String,
    track: Track,
    /// The player's place: the space the track's keys are in (`player_space`), refreshed every tick.
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

/// The space of an event's camera keys. `FUN_0022b0f8` multiplies the evaluated position and look-at by the matrix
/// `DAT_0036bce8`, the event object's +0x50 (`FUN_00195ae0` -> `FUN_002232c8`), which `FUN_00195898` fills from the
/// unit matrix at 0x354a60. [inferred] The keys are nevertheless the player's own space, as the data shows: the
/// last key of `TT04_MS500`'s record looks at the origin from 5.6 m behind it (z negative), its first looks 3.55 m
/// ahead (z) at head height (y 107), which is where Seifer stands from Roxas at the entry (3.46 m), and
/// `TR03_MS103`'s record circles (5, 92, 0) at 3.6 m, a head height above the origin. Read as map coordinates the
/// first shot looks at a wall and the second circles the arena's origin, 10 m from Sora, and under the floor
/// wherever the ground is above the key's height (the user's report, docs/systems/player-camera/camera-event.md "The intro camera"). The
/// fly-back of opcode 0x31 mode 1 names the player's matrix too (`FUN_0016aa08`, `DAT_0035c998`). The matrix is the
/// player's place: origin at his feet, z along his facing, y up, x to his left (the handedness of the app's
/// space, a proper turn of the game's).
pub fn player_space(pos: Vec3, forward: Vec3) -> Mat4 {
    let f = Vec3::new(forward.x, 0.0, forward.z).try_normalize().unwrap_or(Vec3::Z);
    let x = Vec3::Y.cross(f);
    Mat4::from_cols(x.extend(0.0), Vec3::Y.extend(0.0), f.extend(0.0), pos.extend(1.0))
}

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
fn tick(mut commands: Commands, disc: Res<Disc>, mission: Res<Mission>, mut intro: ResMut<Intro>, mut cam: ResMut<CamEvent>, sora: Query<&crate::sora::Sora>) {
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
        intro.run = Some(Run { key, name: a.name.clone(), track, origin: Mat4::IDENTITY, follow: None, last: None, started, pax: None });
    }
    let Some(r) = intro.run.as_mut() else { return };
    // The keys' space is the player's place, held while the event plays (the start layout locks the control).
    if let Ok(s) = sora.single() {
        r.origin = player_space(s.motor.pos, s.motor.forward());
    }
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
    use crate::shared::event::{channel, Cut, Key};
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


    #[test]
    fn the_keys_are_in_the_players_space() {
        // Sora at (1000, -60, 0) facing +x: a key 355 ahead lands 355 along +x, a key at head height is above his feet,
        // and the key's +x is his left (the app's Y-up space: up cross forward).
        let m = player_space(Vec3::new(1000.0, -60.0, 0.0), Vec3::X);
        let near = |a: Vec3, b: Vec3| (a - b).length() < 1e-3;
        assert!(near(m.transform_point3(Vec3::new(0.0, 107.0, 355.0)), Vec3::new(1355.0, 47.0, 0.0)));
        assert!(near(m.transform_point3(Vec3::new(10.0, 0.0, 0.0)), Vec3::new(1000.0, -60.0, -10.0)));
        // An orbit of radius 360 round (5, 92, 0) stays 360 from his head wherever he stands and turns.
        let m = player_space(Vec3::new(-20.0, 300.0, 40.0), Vec3::new(1.0, 0.0, 1.0));
        let c = m.transform_point3(Vec3::new(5.0, 92.0, 0.0));
        let p = m.transform_point3(Vec3::new(5.0 + 360.0 * 0.6, 92.0, 360.0 * 0.8));
        assert!(((p - c).length() - 360.0).abs() < 1e-2 && (c.y - 392.0).abs() < 1e-3);
        // No facing: straight along +z.
        assert!(near(player_space(Vec3::ZERO, Vec3::ZERO).transform_point3(Vec3::Z), Vec3::Z));
    }
}