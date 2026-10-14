//! The area record and the area-script run (WP-E1, docs/events.md sections 0 to 2).
//!
//! The game keeps the current place in six bytes at `0x33e830`: world, area, entrance and the
//! three program ids of the `map`, `btl` and `evt` scripts (`FUN_00104bd0` writes them when an
//! area is entered). `AreaRecord` is that record. When `CurrentArea` finishes loading, the area
//! file's three scripts run through `kh2_ard::run::run_area` (the port of `FUN_00181148`) and the
//! result is kept here for the exit volumes (`area_jump`), the log and later stages (the groups,
//! triggers and music; E6 plays events). The program bytes come from the story's program table
//! (`progress::Story::programs`, written by the story flags' op 1; WP-E5) and opcode 0x19 of the
//! run sets its story flags.
//!
//! Placement: an entity of spawn kind 1 is created only for its entrance (`FUN_001981a0`), and
//! the player stands at his entity of the `map` program's groups. After a jump (or with
//! `KH2_ENTRANCE=<n>` at startup) Sora is put there, facing `PI - rotation y` (`FUN_00198220`,
//! the same rule as the enemies' records). Otherwise the startup placement of `assets::load_area`
//! stays, so the existing scenarios keep their start point.
//!
//! `KH2_AREALOG=1` logs the run: `area ttNN entrance N programs (m, b, e)`, the groups, the music
//! and the opcodes the interpreter reads but does not act on.

use std::sync::Arc;

use bevy::prelude::*;
use kh2_ard::run::{self, ScriptRun};

use crate::assets::{CurrentArea, Disc};
use crate::sora::Sora;
use crate::world::World;
use crate::SCALE;

pub struct AreaPlugin;

impl Plugin for AreaPlugin {
    fn build(&self, app: &mut App) {
        let entrance: Option<u8> = std::env::var("KH2_ENTRANCE").ok().and_then(|v| v.parse().ok());
        // Harness (WP-E2): `KH2_PROGRAMS=<map>,<btl>,<evt>` (hex) sets the program bytes 3..5 of the
        // record for every area, over the program table (a townspeople group of tt02 is loaded by
        // map programs other than 0). Without it the bytes come from the story's program table.
        let programs_override = std::env::var("KH2_PROGRAMS").ok().map(|v| {
            let mut p = [0u8; 3];
            for (k, s) in v.split(',').take(3).enumerate() {
                p[k] = u8::from_str_radix(s.trim().trim_start_matches("0x"), 16).unwrap_or(0);
            }
            p
        });
        app.insert_resource(AreaRecord {
            world: TWILIGHT_TOWN,
            area: 0,
            entrance: entrance.unwrap_or(0),
            programs: programs_override.unwrap_or([0; 3]),
            programs_override,
            place_on_load: entrance.is_some(),
            run: None,
            placed: None,
        })
            .add_systems(Update, run_area_script.after(crate::assets::load_area).before(crate::schedule::Frame::Pad));
    }
}

/// World byte of Twilight Town (the prefix table at 0x349dc0: index 2 is `tt`).
pub const TWILIGHT_TOWN: u8 = 2;

/// The six bytes at `0x33e830` plus what this app keeps next to them.
#[derive(Resource)]
pub struct AreaRecord {
    /// Byte 0.
    pub world: u8,
    /// Byte 1: the `NN` of `ttNN` (set from `CurrentArea` when it loads).
    pub area: u8,
    /// Byte 2: read by opcode 0x1c and the entity filter `FUN_001981a0`.
    pub entrance: u8,
    /// Bytes 3..5: the `map`, `btl`, `evt` program ids. `FUN_00104bd0` copies them from the program
    /// table (`progress::Story::programs`, `0x33e86f + world * 0xc0 + area * 3`) when an area is
    /// entered; the story flags' op 1 writes that table (all 0 in a new game).
    pub programs: [u8; 3],
    /// `KH2_PROGRAMS` (harness): used instead of the table for every area.
    pub programs_override: Option<[u8; 3]>,
    /// Put Sora at the entrance's player entity once the area has loaded (a jump, or
    /// `KH2_ENTRANCE`).
    pub place_on_load: bool,
    /// The run for `CurrentArea.loaded`.
    pub run: Option<Arc<AreaRun>>,
    /// The area Sora was last placed in by `place_on_load`.
    pub placed: Option<String>,
}

/// An area file and what its scripts produced.
pub struct AreaRun {
    pub name: String,
    pub ard: kh2_ard::Area,
    pub script: ScriptRun,
}

/// The area number of a name such as `tt04`.
pub fn area_number(name: &str) -> Option<u8> {
    name.get(2..)?.parse().ok()
}

/// The area name for (world, area) in the worlds this app has (Twilight Town only).
pub fn area_name(world: u8, area: u8) -> Option<String> {
    (world == TWILIGHT_TOWN).then(|| format!("tt{area:02}"))
}

/// A point of the game's space (centimetres, Y down, Z mirrored) in the app's axes (still
/// centimetres; `SCALE` makes metres).
pub fn game_to_app(p: [f32; 3]) -> Vec3 {
    Vec3::new(p[0], -p[1], -p[2])
}

/// The app's yaw for a record rotation y (`FUN_00198220`: the object faces `DAT_0037b270 - ry`
/// in the game's space, `(sin, 0, cos)`; the axis flip makes that `PI - ry` here).
pub fn app_yaw(ry: f32) -> f32 {
    std::f32::consts::PI - ry
}

pub fn arealog() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| std::env::var("KH2_AREALOG").is_ok_and(|v| v != "0"))
}

/// Runs the area's scripts when a new area has loaded and, when asked, places Sora at the
/// entrance's player entity (on the collision floor under the record's point).
pub fn run_area_script(
    disc: Res<Disc>,
    area: Res<CurrentArea>,
    mut rec: ResMut<AreaRecord>,
    world: Option<Res<World>>,
    progress: Option<Res<crate::progress::Progress>>,
    mut sora: Query<(&mut Transform, &mut Sora)>,
) {
    let Some(loaded) = area.loaded.clone() else { return };
    if rec.run.as_ref().is_none_or(|r| r.name != loaded) {
        rec.area = area_number(&loaded).unwrap_or(0);
        // FUN_00104bd0: the program bytes of the table for (world, area). The app's jumps carry no
        // program bytes of their own (exits and type-1 trigger jumps; FUN_00104b90 would let them
        // override with anything but 0xff).
        let from_table = progress.as_ref().map_or([0; 3], |p| p.story().programs(rec.world, rec.area));
        rec.programs = rec.programs_override.unwrap_or(from_table);
        let ard = disc.read(&format!("ard/{loaded}.ard")).and_then(|d| kh2_ard::Area::parse(&d)).map(|(a, _)| a);
        let Some(ard) = ard else {
            warn!("area {loaded}: ard/{loaded}.ard not readable; no area script runs");
            rec.run = Some(Arc::new(AreaRun { name: loaded, ard: kh2_ard::Area::default(), script: ScriptRun::default() }));
            return;
        };
        let script = run::run_area(&ard, rec.programs, rec.entrance);
        // Opcode 0x19 (FUN_00180b48): each flag not yet set goes through FUN_0017ff60, side effects
        // included (they change the program table for the next entry, not this run's bytes).
        let mut newly = Vec::new();
        if let Some(p) = progress.as_ref() {
            let mut s = p.story();
            for &f in &script.story_flags {
                if !s.test(f as u16) {
                    s.set(f as u16);
                    newly.push(f);
                }
            }
        }
        if arealog() {
            let [m, b, e] = rec.programs;
            info!("area {loaded} entrance {} programs ({m}, {b}, {e})", rec.entrance);
            let from = if rec.programs_override.is_some() { "KH2_PROGRAMS" } else { "the program table 0x33e86f (story flags, FUN_00104bd0)" };
            info!("area {loaded}: map program {m}, btl program {b}, evt program {e} from {from}");
            if !script.story_flags.is_empty() {
                info!("area {loaded}: opcode 0x19 story flags {:x?}, newly set {newly:x?}", script.story_flags);
            }
            for g in &script.groups {
                let n: usize = ard.spawns.get(&g.tag).map_or(0, |gs| gs.iter().map(|g| g.entities.len()).sum());
                info!("area {loaded}: {} program {:#x} loads group {}{} ({n} entities)", g.script, g.program, g.tag, if g.alt { " (alt)" } else { "" });
            }
            match script.music {
                Some([field, battle]) => info!("area {loaded}: music field {field} battle {battle} (opcode 0x10; audio::area_bgm applies it)"),
                None => info!("area {loaded}: no music command (the previous pair stays, FUN_00137150)"),
            }
            info!("area {loaded}: {} triggers, map visibility {:#x}, capacity {}", script.triggers.len(), script.map_visibility, script.capacity);
            for op in &script.unported {
                info!("area {loaded}: opcode {op:#x} {} read but not ported", kh2_ard::opcode_name(*op));
            }
        }
        rec.run = Some(Arc::new(AreaRun { name: loaded.clone(), ard, script }));
    }
    if !rec.place_on_load || rec.placed.as_deref() == Some(loaded.as_str()) {
        return;
    }
    // The collision world is inserted by `load_area` (deferred: present from the frame it loads).
    let Some(w) = world else { return };
    let Some(r) = rec.run.clone() else { return };
    let Ok((mut tf, mut s)) = sora.single_mut() else { return };
    match run::player_start(&r.ard, &r.script, rec.entrance) {
        Some((tag, e)) => {
            let at = game_to_app(e.position);
            let at = w.ground(at, at.y + 100.0, 400.0).map_or(at, |g| at.with_y(g.y));
            crate::sora::respawn(&mut s, at);
            s.motor.yaw = app_yaw(e.rotation[1]);
            tf.translation = at * SCALE;
            tf.rotation = Quat::from_rotation_y(s.motor.yaw);
            info!("area {loaded}: player start {tag} entrance {} at ({:.0}, {:.0}, {:.0}) yaw {:.3}", rec.entrance, at.x, at.y, at.z, s.motor.yaw);
        }
        None => info!("area {loaded}: no player entity for entrance {} in the loaded groups; the startup placement stays", rec.entrance),
    }
    rec.placed = Some(loaded);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_area_record_names_areas_and_turns_record_points_into_app_space() {
        assert_eq!(area_number("tt04"), Some(4));
        assert_eq!(area_number("tt"), None);
        assert_eq!(area_name(TWILIGHT_TOWN, 3).as_deref(), Some("tt03"));
        assert_eq!(area_name(0x12, 1), None);
        // tt04's entrance-1 player record (printed by the kh2-ard disc test): Y down and Z
        // mirrored become the app's Y up, Z forward; the yaw is PI - ry.
        let at = game_to_app([-1064.645, -0.0001, 1544.645]);
        assert!((at - Vec3::new(-1064.645, 0.0001, -1544.645)).length() < 1e-3);
        assert!((app_yaw(0.785) - 2.3566).abs() < 1e-3);
        // A record facing +Z in the game faces -Z here.
        let d = app_yaw(0.0);
        assert!((Vec3::new(d.sin(), 0.0, d.cos()) - Vec3::NEG_Z).length() < 1e-6);
    }
}
