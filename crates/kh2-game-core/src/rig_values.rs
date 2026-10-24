//! The named values of the diff rig, read from the port (the names and units are `scenarios/values.toml`; the original side reads the
//! same names from PCSX2 memory). Each getter answers in the original's units and axes: centimetres, the game's axes (y up is
//! negative), radians, raw integers, with the items of a vector joined by `;` and floats in the shortest form that reads back
//! (`kh2_rig::scenario::Ty::format`). `None` means the port has no such value now (no object, or no field for it yet).
//!
//! Names of `values.toml` the port has no field for, and why:
//! * `sora.object`: the original's player object pointer; the port has no address space.
//! * `place.battle`: the original's battle mode word; the port's `Battle` is a tick count, not the same word [no mapping known].
//! * `menu.word_354f70`: 0 at every stop of the original's menu runs (RIG-4); not a menu value, kept for the old traces.
//!
//! `menu.scene` (7 while a menu is up, -1 otherwise) and `menu.next_page` (-1) are constants the port derives from its phase [inferred]:
//! the port has no scene request or page leave animation, so the scenarios show them but never count them.
//!
//! The menu and motion names are answered from three places `Ctx` carries: the shared `MenuPhase` and `RigMenu` (the menu crate writes
//! the page and the list cursor into the mirror), and Sora's body animator (`motion`: the motion set slot divided by four, the slot being
//! `id * 4 + variant`, and the animator's frame). The port has one phase for the whole camp, so the page's own phase (the original's
//! `menu.phase`) is the camp's: entering while the open animation plays, selecting when it is done, leaving while it closes; a page
//! change does not replay the entering phase in the port, and the page words of the original that stay stale while no menu runs
//! (`menu.page`, `menu.next_page`, `menu.phase`, `menu.cursor`, `menu.scroll`) are empty here while the menu is closed.

use bevy::prelude::*;

use crate::area::AreaRecord;
use crate::progress::Progress;
use crate::shared::sora::Sora;
use crate::shared::ui::{MenuPhase, Phase, RigMenu};
use crate::SCALE;

/// What a getter may look at: the tick's `k` and the objects of the app.
#[derive(Clone, Copy)]
pub struct Ctx<'a> {
    pub k: u32,
    pub sora: Option<&'a Sora>,
    pub area: Option<&'a AreaRecord>,
    pub progress: Option<&'a Progress>,
    /// The camera's transform (metres, the app's axes).
    pub cam: Option<&'a Transform>,
    /// The camera object's wanted position and look-at point (`0x355840 + 0x60` / `+ 0x80`, game units, map-model axes).
    pub cam_wanted: Option<(Vec3, Vec3)>,
    /// The camp's phase and the page and cursor mirror (`shared::ui`).
    pub menu: Option<(&'a MenuPhase, &'a RigMenu)>,
    /// Sora's playing motion: the id (slot / 4) and the animator's frame.
    pub motion: Option<(i32, f32)>,
    /// The vsync block of the frame clock (`schedule::FrameClock`): the frame step, the counters and the speed.
    pub vsync: Option<&'a crate::frame::Vsync>,
}

type Getter = fn(&Ctx) -> Option<String>;

/// Every name the port answers, with its getter.
pub const GETTERS: &[(&str, Getter)] = &[
    ("clock.vsync", vsync),
    ("frame.step", frame_step),
    ("frame.vblanks", |c| c.vsync.map(|v| v.vblanks().to_string())),
    ("frame.since_step", |c| c.vsync.map(|v| v.since_step().to_string())),
    ("frame.speed", |c| c.vsync.map(|v| f(v.speed()))),
    ("place.programs", |c| c.area.map(|a| a.programs.map(|b| b.to_string()).join(";"))),
    ("place.world", |c| c.area.map(|a| a.world.to_string())),
    ("place.area", |c| c.area.map(|a| a.area.to_string())),
    ("place.entrance", |c| c.area.map(|a| a.entrance.to_string())),
    ("sora.pos", sora_pos),
    ("sora.facing", sora_facing),
    ("save.sora_hp", |c| c.progress.map(|p| p.sora().hp.to_string())),
    ("save.sora_hp_max", |c| c.progress.map(|p| p.sora().hp_max.to_string())),
    ("cam.pos", cam_pos),
    ("cam.look", cam_look),
    ("rng.lo", |_| Some((crate::rng::state() as u32).to_string())),
    ("rng.hi", |_| Some(((crate::rng::state() >> 32) as u32).to_string())),
    ("sora.motion", |c| c.motion.map(|m| m.0.to_string())),
    ("sora.motion_frame", |c| c.motion.map(|m| f(m.1))),
    ("menu.scene_state", |c| c.menu.map(|(p, _)| if p.0 == Phase::Closed { "0" } else { "1" }.to_string())),
    ("menu.scene", |c| c.menu.map(|(p, _)| if p.0 == Phase::Closed { "-1" } else { "7" }.to_string())),
    ("menu.page", |c| open_menu(c).map(|(_, m)| m.page.to_string())),
    ("menu.next_page", |c| open_menu(c).map(|_| "-1".to_string())),
    ("menu.phase", |c| open_menu(c).map(|(p, _)| if p == Phase::Opening { "0" } else if p == Phase::Open { "1" } else { "2" }.to_string())),
    ("menu.closing", |c| c.menu.map(|(p, _)| if p.0 == Phase::Closing { "1" } else { "0" }.to_string())),
    ("menu.cursor", |c| open_menu(c).and_then(|(_, m)| m.cursor).map(|v| v.to_string())),
    ("menu.scroll", |c| open_menu(c).map(|(_, m)| m.scroll.to_string())),
];

/// Names of `values.toml` the port has no getter for (see the module comment).
pub const MISSING: &[&str] = &[
    "sora.object",
    "place.battle",
    "menu.word_354f70",
];

/// The camp's phase and mirror while a menu is up (the original's page words are stale otherwise).
fn open_menu<'a>(c: &Ctx<'a>) -> Option<(Phase, RigMenu)> {
    c.menu.filter(|(p, _)| p.0 != Phase::Closed).map(|(p, m)| (p.0, *m))
}

pub fn read(name: &str, ctx: &Ctx) -> Option<String> {
    GETTERS.iter().find(|(n, _)| *n == name).and_then(|(_, g)| g(ctx))
}

/// A float as the original's trace prints it (a negative zero is a zero: the axis flip makes zeros negative).
fn f(v: f32) -> String {
    (if v == 0.0 { 0.0 } else { v }).to_string()
}

/// A vector of the app's axes (centimetres) in the game's: `(x, -y, -z)`.
fn game_vec(v: Vec3) -> String {
    [v.x, -v.y, -v.z].map(f).join(";")
}

/// The state's vsync counter plus `k` (the rig clock is the vsync count since the state).
fn vsync(c: &Ctx) -> Option<String> {
    Some((c.progress?.rig.vsync? + c.k).to_string())
}

/// The frame step `0x356f40` of the ported `frame::Vsync` that drives the app (`schedule::FrameClock`): 2.0 in the field (limit 1),
/// 1.0 while a menu is up (limit 0).
fn frame_step(c: &Ctx) -> Option<String> {
    c.vsync.map(|v| f(v.frame_step()))
}

fn sora_pos(c: &Ctx) -> Option<String> {
    Some(game_vec(c.sora?.motor.pos))
}

/// A unit vector in the game's axes: the app's forward `(sin yaw, 0, cos yaw)` with z mirrored.
fn sora_facing(c: &Ctx) -> Option<String> {
    let yaw = c.sora?.motor.yaw;
    Some([yaw.sin(), 0.0, -yaw.cos()].map(f).join(";"))
}

/// The camera object's wanted position (`+0x60`, the value `values.toml` names); the drawn transform when the camera has not run yet.
fn cam_pos(c: &Ctx) -> Option<String> {
    match c.cam_wanted {
        Some((pos, _)) => Some(game_vec(pos)),
        None => Some(game_vec(c.cam?.translation / SCALE)),
    }
}

/// The camera object's wanted look-at point (`+0x80`).
fn cam_look(c: &Ctx) -> Option<String> {
    c.cam_wanted.map(|(_, look)| game_vec(look))
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::progress::{ItemIndex, Progress};

    #[test]
    fn every_value_of_the_list_has_a_getter_or_is_listed_missing() {
        // The rig's value list (scenarios/values.toml) is the contract: every table with a `path` is a name.
        let text = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../../scenarios/values.toml")).unwrap();
        let values = kh2_rig::scenario::Values::parse(&text).unwrap();
        let names: Vec<String> = values.defs.keys().cloned().collect();
        assert!(names.len() >= 12, "{names:?}");
        for n in &names {
            let have = GETTERS.iter().any(|(g, _)| g == n);
            let missing = MISSING.contains(&n.as_str());
            assert!(have != missing, "'{n}' must be a getter or listed missing, exactly one: have {have}, missing {missing}");
        }
        for (g, _) in GETTERS {
            assert!(names.iter().any(|n| n == g), "getter '{g}' is not a name of values.toml");
        }
    }

    #[test]
    fn getters_answer_in_the_games_units_and_axes() {
        let mut s = Sora::for_test(80, 80);
        s.motor.pos = Vec3::new(650.0, 0.0, -250.0);
        s.motor.yaw = 0.0;
        let mut p = Progress::from_block(&[], Arc::new(ItemIndex::default()));
        p.rig.vsync = Some(495_135);
        let cam = Transform::from_translation(Vec3::new(5.2026, 2.2, -0.5676));
        let area = AreaRecord { world: 2, area: 21, entrance: 99, programs: [0; 3], programs_override: None, dest_programs: [0xff; 3], place_on_load: false, run: None, placed: None };
        let mut vs = crate::frame::Vsync::default();
        vs.init();
        vs.set_step(2.0);
        vs.set_vblanks(7);
        vs.set_since_step(1);
        let c = Ctx { k: 42, sora: Some(&s), area: Some(&area), progress: Some(&p), cam: Some(&cam), cam_wanted: None, menu: None, motion: None, vsync: Some(&vs) };
        let r = |n: &str| read(n, &c).unwrap_or_else(|| panic!("{n}"));
        assert_eq!(r("sora.pos"), "650;0;250", "the app's z is mirrored and a negative zero is printed as 0");
        assert_eq!(r("sora.facing"), "0;0;-1", "yaw 0 is app +z, game -z");
        assert_eq!(r("clock.vsync"), "495177");
        assert_eq!((r("place.world"), r("place.area"), r("place.entrance")), ("2".into(), "21".into(), "99".into()));
        assert_eq!(r("frame.step"), "2");
        assert_eq!((r("frame.vblanks"), r("frame.since_step"), r("frame.speed")), ("7".into(), "1".into(), "1".into()));
        assert_eq!(r("place.programs"), "0;0;0", "the three program bytes of the area record, joined by ;");
        let cam_pos: Vec<f32> = r("cam.pos").split(';').map(|v| v.parse().unwrap()).collect();
        assert!((cam_pos[0] - 520.26).abs() < 0.01 && (cam_pos[1] + 220.0).abs() < 0.01 && (cam_pos[2] - 56.76).abs() < 0.01, "{cam_pos:?}");
        let mut turned = Sora::for_test(80, 80);
        turned.motor.yaw = std::f32::consts::FRAC_PI_2;
        let c = Ctx { sora: Some(&turned), ..c };
        let facing: Vec<f32> = read("sora.facing", &c).unwrap().split(';').map(|v| v.parse().unwrap()).collect();
        assert!((facing[0] - 1.0).abs() < 1e-6 && facing[2].abs() < 1e-6, "{facing:?}");
        // No objects: no values; a name the port lacks stays None.
        let none = Ctx { k: 0, sora: None, area: None, progress: None, cam: None, cam_wanted: None, menu: None, motion: None, vsync: None };
        assert!(GETTERS.iter().filter(|(n, _)| !matches!(*n, "rng.lo" | "rng.hi")).all(|(_, g)| g(&none).is_none()));
        assert_eq!(read("menu.word_354f70", &c), None);
    }

    #[test]
    fn menu_and_motion_values_follow_the_phase_and_the_mirror() {
        let rig = |page, cursor| RigMenu { page, cursor, scroll: 0 };
        let at = |phase: Phase, mirror: RigMenu, names: &[&str]| -> Vec<Option<String>> {
            let phase = MenuPhase(phase);
            let c = Ctx { k: 0, sora: None, area: None, progress: None, cam: None, cam_wanted: None, menu: Some((&phase, &mirror)), motion: None, vsync: None };
            names.iter().map(|n| read(n, &c)).collect()
        };
        let s = |v: &str| Some(v.to_string());
        let none = Ctx { k: 0, sora: None, area: None, progress: None, cam: None, cam_wanted: None, menu: None, motion: Some((2, 38.0)), vsync: None };
        assert_eq!((read("sora.motion", &none), read("sora.motion_frame", &none)), (s("2"), s("38")));
        assert_eq!(read("menu.scene_state", &none), None, "no menu resources: no value");
        let names = ["menu.scene_state", "menu.scene", "menu.closing", "menu.page", "menu.cursor"];
        assert_eq!(at(Phase::Closed, rig(0, Some(2)), &names), [s("0"), s("-1"), s("0"), None, None], "the stale page words are empty while closed");
        assert_eq!(at(Phase::Opening, rig(0, Some(2)), &["menu.scene_state", "menu.scene", "menu.phase"]), [s("1"), s("7"), s("0")]);
        assert_eq!(at(Phase::Open, rig(0xa, Some(2)), &["menu.phase", "menu.page", "menu.cursor", "menu.next_page"]), [s("1"), s("10"), s("2"), s("-1")]);
        assert_eq!(at(Phase::Open, rig(0x14, None), &["menu.cursor"]), [None], "a page the menu crate does not expose has no cursor");
        assert_eq!(at(Phase::Closing, rig(0, Some(1)), &["menu.closing", "menu.phase", "menu.scroll"]), [s("1"), s("2"), s("0")]);
    }
}
