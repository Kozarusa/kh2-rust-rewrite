//! The area record and the area-script run (WP-E1, docs/systems/core-runtime/areas.md sections 0 to 2).
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
use crate::shared::sora::Sora;
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
            dest_programs: [0xff; 3],
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
    /// Bytes 3..5 of the destination the running jump carries (`FUN_00104b90`: each byte that is not
    /// 0xff replaces the table's byte). A type-2 jump (`07localset.bin`) sets all three to its key.
    /// The next area start applies them and resets them to 0xff.
    pub dest_programs: [u8; 3],
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

/// Battle manager type of the map's `WORLDMAP` manager (opcode 0x15, argument 0).
pub const WORLDMAP_MANAGER: u32 = 0xc;

/// World byte of the world map (area `wm00`, docs/systems/worldmap-gummi/worldmap.md section 2).
pub const WORLD_MAP: u8 = 0xf;

/// The two-letter area-file prefix of each of the 19 worlds, in world-byte order (the game's
/// prefix table; the same order as the `ftst` colour columns, docs/systems/core-runtime/contract.md).
pub const WORLDS: [&str; 19] = [
    "zz", "es", "tt", "di", "hb", "bb", "he", "al", "mu", "po", "lk", "lm", "dc", "wi", "nm", "wm", "ca", "tr", "eh",
];

/// The world byte of an area name such as `tt04` or `wm00`.
pub fn world_of(name: &str) -> Option<u8> {
    let prefix = name.get(..2)?;
    WORLDS.iter().position(|w| *w == prefix).map(|i| i as u8)
}

/// The area name for (world, area) in the worlds this app has (Twilight Town and the world map).
pub fn area_name(world: u8, area: u8) -> Option<String> {
    matches!(world, TWILIGHT_TOWN | WORLD_MAP).then(|| format!("{}{area:02}", WORLDS[world as usize]))
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

/// Brings the world's collision groups in line with the game: the area word once per world, then the
/// switches of hosts 1.36 / 1.37 in call order. `KH2_AREALOG` logs the mask the world ends up with.
pub(crate) fn sync_groups(w: &mut World, word: u64, area: &str, edits: &[(u8, bool)]) {
    if w.apply_area_word(word) && arealog() {
        info!("area {area}: world collision mask {:#x} applied (script word {word:#x}; FUN_00141768)", w.enabled_groups);
    }
    for &(g, on) in edits {
        w.set_group(g as u32, on);
        if arealog() {
            info!("area {area}: collision group {g} {} (host {}); world mask now {:#x}", if on { "on" } else { "off" }, if on { "1.37" } else { "1.36" }, w.enabled_groups);
        }
    }
}

/// Runs the area's scripts when a new area has loaded and, when asked, places Sora at the
/// entrance's player entity (on the collision floor under the record's point).
pub fn run_area_script(
    disc: Res<Disc>,
    area: Res<CurrentArea>,
    mut rec: ResMut<AreaRecord>,
    mut world: Option<ResMut<World>>,
    progress: Option<Res<crate::progress::Progress>>,
    mut sora: Query<(&mut Transform, &mut Sora)>,
) {
    let Some(loaded) = area.loaded.clone() else { return };
    if rec.run.as_ref().is_none_or(|r| r.name != loaded) {
        rec.area = area_number(&loaded).unwrap_or(0);
        // The record's world follows the loaded file's prefix (`kh2-game wm00` starts in world 0xf).
        if let Some(w) = world_of(&loaded) {
            rec.world = w;
        }
        // FUN_00104bd0: the program bytes of the table for (world, area). The app's jumps carry no
        // program bytes of their own (exits and type-1 trigger jumps; FUN_00104b90 would let them
        // override with anything but 0xff).
        let from_table = progress.as_ref().map_or([0; 3], |p| p.story().programs(rec.world, rec.area));
        // FUN_00104bd0 then FUN_00104b90: the destination's program bytes (not 0xff) replace the table's.
        let from_jump = rec.dest_programs != [0xff; 3];
        let mut table = from_table;
        for (t, d) in table.iter_mut().zip(rec.dest_programs) {
            if d != 0xff {
                *t = d;
            }
        }
        rec.dest_programs = [0xff; 3];
        rec.programs = rec.programs_override.unwrap_or(table);
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
            let from = if rec.programs_override.is_some() { "KH2_PROGRAMS" } else if from_jump { "the jump's program bytes over the table (FUN_00104b90)" } else { "the program table 0x33e86f (story flags, FUN_00104bd0)" };
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
            if let Some((kind, _)) = &script.battle_manager {
                if *kind == WORLDMAP_MANAGER {
                    // FUN_001d5048 type 0xc: party reset code 1 (FUN_001abfe8) and area camera type 4
                    // (DAT_0035cd38). The camera has no type 4 behaviour decoded (docs/systems/player-camera/contract.md) and the
                    // party reset has no hook yet; neither is applied (docs/systems/worldmap-gummi/contract.md W4).
                    info!("area {loaded}: battle manager type {kind:#x} (WORLDMAP) wants camera type 4 and party reset 1; not applied (gap)");
                } else {
                    info!("area {loaded}: battle manager type {kind:#x} read but not created");
                }
            }
            for op in &script.unported {
                info!("area {loaded}: opcode {op:#x} {} read but not ported", kh2_ard::opcode_name(*op));
            }
        }
        rec.run = Some(Arc::new(AreaRun { name: loaded.clone(), ard, script }));
    }
    // The collision walker visits only meshes whose group bit is in the map visibility word
    // (FUN_00141768); `load_area` inserts a fresh world, which gets the word once. Host calls 1.36 / 1.37 then
    // switch single bits (queued by `enemy_host`, applied here in call order).
    if let (Some(w), Some(r)) = (world.as_mut(), rec.run.as_ref())
        && r.name == loaded
    {
        crate::enemy_host::set_area_mask(r.script.word_35cd18);
        sync_groups(w, r.script.map_visibility, &loaded, &crate::enemy_host::take_group_edits());
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
            crate::shared::sora::respawn(&mut s, at);
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

    /// The world's collision mask is the area word once, then only the host calls' switches change it: a later
    /// sync with the same word must not undo a group a script switched, and the switches apply in call order.
    #[test]
    fn the_world_gets_the_area_word_once_and_then_the_script_switches() {
        let mut w = World::default();
        sync_groups(&mut w, 0x407, "eh10", &[]);
        assert_eq!(w.enabled_groups, 0x407, "the applied mask is the script's word");
        // Host 1.37 on group 3, host 1.36 on group 0, then on again for 0 and off for 10 (order matters for 0).
        sync_groups(&mut w, 0x407, "eh10", &[(3, true), (0, false), (0, true), (10, false)]);
        assert_eq!(w.enabled_groups, 0xf, "bits 0 to 3 on, bit 10 off");
        // The frame-by-frame sync does not re-apply the word.
        sync_groups(&mut w, 0x407, "eh10", &[]);
        assert_eq!(w.enabled_groups, 0xf, "unchanged by a repeated sync");
        // A freshly loaded world is a new World: the word applies again.
        let mut fresh = World::default();
        assert!(fresh.apply_area_word(0x3) && !fresh.apply_area_word(0x7));
        assert_eq!(fresh.enabled_groups, 0x3);
    }

    /// WP-IW2 item 5: which object scripts of an area's spawn groups call host 1.36 / 1.37 (the collision group
    /// switches), as `(model, group clears, group sets, spawn group tags that create it)`.
    fn group_switchers(disc: &Disc, area: &str) -> Vec<(String, usize, usize, Vec<String>)> {
        use kh2_tables::tables as t;
        let (ard, _) = kh2_ard::Area::parse(&disc.read(&format!("ard/{area}.ard")).unwrap()).unwrap();
        let rows = {
            let mut a = disc.0.lock().unwrap();
            kh2_tables::load::<t::Objentry>(&mut a, &t::OBJENTRY).unwrap().rows
        };
        let objects: std::collections::BTreeSet<u32> = ard.spawns.values().flatten().flat_map(|g| g.entities.iter().map(|e| e.object)).collect();
        let mut out = Vec::new();
        for o in objects {
            let Some(row) = rows.iter().find(|r| r.object_id == o) else { continue };
            let Some(bar) = disc.read(&format!("obj/{}.mdlx", row.model_name)) else { continue };
            let Ok(entries) = kh2_bar::parse(&bar) else { continue };
            let Some(prog) = entries.iter().find(|e| e.kind == 3).and_then(|e| kh2_bdx::Program::parse(e.data).ok()) else { continue };
            let Ok(an) = kh2_bdx::analyze(&prog) else { continue };
            let count = |idx: u16| an.code.values().filter(|(i, _)| matches!(i, kh2_bdx::Insn::Host { group: 1, index } if *index == idx)).count();
            let tags = ard.spawns.iter().filter(|(_, gs)| gs.iter().any(|g| g.entities.iter().any(|e| e.object == o))).map(|(t, _)| t.clone()).collect();
            out.push((row.model_name.clone(), count(36), count(37), tags));
        }
        out
    }

    #[test]
    fn eh10_enemy_scripts_and_the_collision_group_switches() {
        let Some(iso) = crate::assets::iso_path().ok().filter(|p| p.exists()) else { return };
        let disc = Disc::open(&iso);
        let all = group_switchers(&disc, "eh10");
        eprintln!("iw2 eh10 objects with scripts: {all:?}");
        let (ard, _) = kh2_ard::Area::parse(&disc.read("ard/eh10.ard").unwrap()).unwrap();
        let op_b: Vec<_> = ard.scripts.iter().flat_map(|(s, ps)| ps.iter().flat_map(move |p| p.commands.iter().filter(|c| c.opcode == 0xb).map(move |c| (s.clone(), p.id, c.args.clone())))).collect();
        eprintln!("iw2 eh10 opcode 0xb commands: {op_b:?}");
        let switching: Vec<_> = all.iter().filter(|s| s.1 + s.2 > 0).collect();
        assert!(all.len() >= 3 && switching.len() == 1 && switching[0].0 == "B_EX140", "{switching:?}");
    }

    #[test]
    fn the_area_record_names_areas_and_turns_record_points_into_app_space() {
        assert_eq!(area_number("tt04"), Some(4));
        assert_eq!(area_number("tt"), None);
        assert_eq!(area_name(TWILIGHT_TOWN, 3).as_deref(), Some("tt03"));
        assert_eq!(area_name(0x12, 1), None);
        assert_eq!(area_name(WORLD_MAP, 0).as_deref(), Some("wm00"));
        assert_eq!(world_of("wm00"), Some(WORLD_MAP));
        assert_eq!(world_of("tt04"), Some(TWILIGHT_TOWN));
        assert_eq!(world_of("xx00"), None);
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
