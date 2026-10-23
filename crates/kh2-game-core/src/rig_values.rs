//! The named values of the diff rig, read from the port (the names and units are `scenarios/values.toml`; the original side reads the
//! same names from PCSX2 memory). Each getter answers in the original's units and axes: centimetres, the game's axes (y up is
//! negative), radians, raw integers, with the items of a vector joined by `;` and floats in the shortest form that reads back
//! (`kh2_rig::scenario::Ty::format`). `None` means the port has no such value now (no object, or no field for it yet).
//!
//! Names of `values.toml` the port has no field for, and why:
//! * `sora.object`: the original's player object pointer; the port has no address space.
//! * `place.battle`: the original's battle mode word; the port's `Battle` is a tick count, not the same word [no mapping known].
//! * `cam.look`: the camera's look-at point is private to the camera system (`camera::CamState`), not exposed.
//! * `menu.word_354f70`: 0 at every stop of the original's menu runs (RIG-4); not a menu value, kept for the old traces.
//! * `sora.motion`, `sora.motion_frame` (actor +0x154, +0x170, traced in RIG-4): `Sora` keeps no general motion id or motion time
//!   (only `damage_motion` inside hit reactions); the playing clip lives in the player crate's animation state, outside `Ctx`.
//! * `menu.*` (traced in RIG-4: scene state 0x33e764, scene 0x3b10d0, page 0x36d1d4, next page 0x36d1dc, phase 0x36d1e0, closing
//!   0x36d1f0, the list widget's cursor and scroll through 0x36ce9c): the port's page, phase and cursors are the menu crate's `Menu`
//!   resource, which `Ctx` does not carry (the shared `MenuPhase` has only closed/opening/open/closing, no page or cursor).

use bevy::prelude::*;

use crate::area::AreaRecord;
use crate::progress::Progress;
use crate::shared::sora::Sora;
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
}

type Getter = fn(&Ctx) -> Option<String>;

/// Every name the port answers, with its getter.
pub const GETTERS: &[(&str, Getter)] = &[
    ("clock.vsync", vsync),
    ("frame.step", frame_step),
    ("place.world", |c| c.area.map(|a| a.world.to_string())),
    ("place.area", |c| c.area.map(|a| a.area.to_string())),
    ("place.entrance", |c| c.area.map(|a| a.entrance.to_string())),
    ("sora.pos", sora_pos),
    ("sora.facing", sora_facing),
    ("save.sora_hp", |c| c.progress.map(|p| p.sora().hp.to_string())),
    ("save.sora_hp_max", |c| c.progress.map(|p| p.sora().hp_max.to_string())),
    ("cam.pos", cam_pos),
];

/// Names of `values.toml` the port has no getter for (see the module comment).
pub const MISSING: &[&str] = &[
    "sora.object",
    "place.battle",
    "cam.look",
    "menu.word_354f70",
    "sora.motion",
    "sora.motion_frame",
    "menu.scene_state",
    "menu.scene",
    "menu.page",
    "menu.next_page",
    "menu.phase",
    "menu.closing",
    "menu.cursor",
    "menu.scroll",
];

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

/// The app steps one game frame per 60 Hz tick: its frame step is 1.0 (the original's field runs 2.0; ported in L1-P5).
fn frame_step(_: &Ctx) -> Option<String> {
    Some(f(1.0))
}

fn sora_pos(c: &Ctx) -> Option<String> {
    Some(game_vec(c.sora?.motor.pos))
}

/// A unit vector in the game's axes: the app's forward `(sin yaw, 0, cos yaw)` with z mirrored.
fn sora_facing(c: &Ctx) -> Option<String> {
    let yaw = c.sora?.motor.yaw;
    Some([yaw.sin(), 0.0, -yaw.cos()].map(f).join(";"))
}

fn cam_pos(c: &Ctx) -> Option<String> {
    Some(game_vec(c.cam?.translation / SCALE))
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
        let c = Ctx { k: 42, sora: Some(&s), area: Some(&area), progress: Some(&p), cam: Some(&cam) };
        let r = |n: &str| read(n, &c).unwrap_or_else(|| panic!("{n}"));
        assert_eq!(r("sora.pos"), "650;0;250", "the app's z is mirrored and a negative zero is printed as 0");
        assert_eq!(r("sora.facing"), "0;0;-1", "yaw 0 is app +z, game -z");
        assert_eq!(r("clock.vsync"), "495177");
        assert_eq!((r("place.world"), r("place.area"), r("place.entrance")), ("2".into(), "21".into(), "99".into()));
        assert_eq!(r("frame.step"), "1");
        let cam_pos: Vec<f32> = r("cam.pos").split(';').map(|v| v.parse().unwrap()).collect();
        assert!((cam_pos[0] - 520.26).abs() < 0.01 && (cam_pos[1] + 220.0).abs() < 0.01 && (cam_pos[2] - 56.76).abs() < 0.01, "{cam_pos:?}");
        let mut turned = Sora::for_test(80, 80);
        turned.motor.yaw = std::f32::consts::FRAC_PI_2;
        let c = Ctx { sora: Some(&turned), ..c };
        let facing: Vec<f32> = read("sora.facing", &c).unwrap().split(';').map(|v| v.parse().unwrap()).collect();
        assert!((facing[0] - 1.0).abs() < 1e-6 && facing[2].abs() < 1e-6, "{facing:?}");
        // No objects: no values; a name the port lacks stays None.
        let none = Ctx { k: 0, sora: None, area: None, progress: None, cam: None };
        assert!(GETTERS.iter().filter(|(n, _)| *n != "frame.step").all(|(_, g)| g(&none).is_none()));
        assert_eq!(read("menu.word_354f70", &c), None);
    }
}
