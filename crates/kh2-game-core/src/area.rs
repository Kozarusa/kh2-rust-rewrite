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
pub fn sync_groups(w: &mut World, word: u64, area: &str, edits: &[(u8, bool)]) {
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
        ctl::merge_programs(&mut table, &rec.dest_programs);
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
        crate::shared::enemy::set_area_mask(r.script.word_35cd18);
        sync_groups(w, r.script.map_visibility, &loaded, &crate::shared::enemy::take_group_edits());
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

/// The area control unit on the original's byte layouts (WP-L1-P6; docs/systems/core-runtime/areas.md section 2a).
///
/// The game keeps the place it is in as 6-byte records (world, area, entrance, map program, btl program, evt program) in a row of
/// globals from 0x33e828: the current place (0x33e830), the destination of the jump in progress (0x33e838), the last exit walked into
/// (0x33e840), the previous world (0x33e848), plus the gate byte and the saved destination of the area thread (0x3b2180, 0x3b2188).
/// [`AreaCtl`] holds exactly those bytes, so the oracle compares them one by one. The jump record that triggers and exits carry (0x20
/// bytes) is [`JumpRec`]. Everything the area change asks of other units (fades, task creation, the party reset, the story flags,
/// memory of the loaded area files) goes through [`AreaHost`], whose calls the oracle test records on both sides.
///
/// Not here: the area thread `FUN_001048e0` (it yields in two loops, so it is a coroutine over the task manager), the object
/// constructors (`FUN_001692f8`, `FUN_001694f4`), the group exit and spawn callbacks that belong to the enemy unit, and the
/// shorter loops of the camp/pause poll task `FUN_0014a080`. The app does not call this module yet (see the `area_jump` notes).
pub mod ctl {
    /// A place record: world, area, entrance, map, btl and evt program ids. 0xff in a destination keeps the current byte.
    pub type Dest6 = [u8; 6];

    /// What the area unit asks of the rest of the game. The oracle logs every call (callee address and arguments, low 32 bits of the
    /// argument registers) on both sides; memory of other units is read and written through the `read*` and `write*` calls.
    pub trait AreaHost {
        /// A call of the original function at `callee`; returns v0.
        fn call(&mut self, callee: u32, args: &[u64]) -> u64;
        /// A virtual call: method `slot` (a word index) of the object's vtable, with the object as first argument.
        fn vcall(&mut self, obj: u32, slot: u32, args: &[u64]) -> u64;
        fn read8(&mut self, addr: u32) -> u8;
        fn read16(&mut self, addr: u32) -> u16;
        fn read32(&mut self, addr: u32) -> u32;
        fn write8(&mut self, addr: u32, v: u8);
        fn write32(&mut self, addr: u32, v: u32);
    }

    /// Sign-extends a word the way a load into a 64-bit register does.
    pub fn sx(v: u32) -> u64 {
        v as i32 as i64 as u64
    }

    /// Offsets in the save block (0x33e860) that the area change reads and writes.
    pub const SAVE_WORLD: usize = 0xc;
    pub const SAVE_AREA: usize = 0xd;
    pub const SAVE_ENTRANCE: usize = 0xe;
    /// The per-area program ids: three bytes per area, 0xc0 bytes per world, from save +0xf (0x33e86f).
    pub const SAVE_PROGRAMS: usize = 0xf;

    /// Addresses of the globals (documentation; the oracle test lays its memory at the same addresses).
    pub const G_ACTIVE: u32 = 0x33e828;
    pub const G_CUR: u32 = 0x33e830;
    pub const G_PENDING: u32 = 0x33e838;
    pub const G_LAST_EXIT: u32 = 0x33e840;
    pub const G_PREV: u32 = 0x33e848;
    pub const G_ALLOC: u32 = 0x33e850;
    pub const G_TASK: u32 = 0x33e854;
    pub const G_TASK_MGR: u32 = 0x33e6e4;
    pub const G_GATE: u32 = 0x3b2180;
    pub const G_SAVED: u32 = 0x3b2188;

    /// The wait, flag, fade and reset words of the jump task block (the block `FUN_00102778` makes, 0x48 bytes).
    pub const TASK_FUNC: u32 = 0;
    pub const TASK_WAIT: u32 = 4;
    pub const TASK_FLAG: u32 = 8;
    pub const TASK_FADE: u32 = 0xc;
    pub const TASK_RESET: u32 = 0x10;

    /// The globals of the area change.
    #[derive(Clone, PartialEq, Eq, Debug, Default)]
    pub struct AreaCtl {
        /// 0x33e828: 1 while an area is loaded (set by `FUN_001047a8`, cleared by `FUN_00104ce0`).
        pub active: u8,
        /// 0x33e830: the current place.
        pub cur: Dest6,
        /// 0x33e838: the destination of the jump in progress.
        pub pending: Dest6,
        /// 0x33e840: the last exit walked into (the target of a type-3 jump record).
        pub last_exit: Dest6,
        /// 0x33e848: the previous place (its world byte is cleared on a world change).
        pub prev: Dest6,
        /// 0x33e850: the area allocator object (0 for none).
        pub allocator: u32,
        /// 0x33e854: the jump task (0 for none).
        pub task: u32,
        /// 0x33e6e4: the field task manager (owned by the task unit; read here).
        pub task_mgr: u32,
        /// 0x3b2180: set while the area thread plays the jump record at the end of an area (`FUN_001048e0`).
        pub gate: u8,
        /// 0x3b2188: the destination saved by that thread.
        pub saved: Dest6,
        /// 0x356398 and 0x356399: the pause-poll task's two flags.
        pub poll_stop: u8,
        pub poll_paused: u8,
        /// 0x359368: the area allocator as the layout code sees it.
        pub layout_alloc: u32,
        /// 0x1c8bd90: the world byte a type-4 jump remembers (-1 for none).
        pub type4_memory: u32,
        /// 0x1c8bd98: set once the jump party word is initialised.
        pub party_ready: u8,
        /// 0x1c8bda0: the jump party word.
        pub party_word: u32,
        /// 0x1c8af88: the loaded `07localset.bin` (a pointer the file unit uses).
        pub localset_file: u32,
        /// 0x35e468: how many group records are in use.
        pub group_slots: u32,
        /// 0x1c8a970: the loaded area info table (a pointer the file unit owns).
        pub areainfo: u32,
        /// 0x1c8a974: set when the current area's entry names an ensemble that exists.
        pub areainfo_has: u8,
        /// 0x1c8a975: set to hold the ensemble back.
        pub areainfo_hold: u8,
        /// 0x1c8a978: the file the area info was loaded from.
        pub areainfo_file: u32,
    }

    /// The jump record (0x20 bytes) a trigger, an exit or the area thread hands to `FUN_0019d9d8`: +0 type, +4 the destination (six
    /// bytes), +0xc the fade flags, +0x10 the party arrangement (-1 for none), +0x14 the party reset code, +0x18 the "menu before" code.
    #[derive(Clone, PartialEq, Eq, Debug, Default)]
    pub struct JumpRec(pub [u8; 0x20]);

    impl JumpRec {
        fn w(&self, o: usize) -> u32 {
            u32::from_le_bytes(self.0[o..o + 4].try_into().unwrap())
        }
        fn set_w(&mut self, o: usize, v: u32) {
            self.0[o..o + 4].copy_from_slice(&v.to_le_bytes());
        }
        pub fn kind(&self) -> u32 {
            self.w(0)
        }
        pub fn set_kind(&mut self, v: u32) {
            self.set_w(0, v)
        }
        pub fn dest(&self) -> Dest6 {
            self.0[4..10].try_into().unwrap()
        }
        pub fn set_dest(&mut self, d: &Dest6) {
            self.0[4..10].copy_from_slice(d);
        }
        pub fn fade(&self) -> u32 {
            self.w(0xc)
        }
        pub fn set_fade(&mut self, v: u32) {
            self.set_w(0xc, v)
        }
        pub fn party(&self) -> i32 {
            self.w(0x10) as i32
        }
        pub fn set_party(&mut self, v: i32) {
            self.set_w(0x10, v as u32)
        }
        pub fn reset(&self) -> i32 {
            self.w(0x14) as i32
        }
        pub fn set_reset(&mut self, v: i32) {
            self.set_w(0x14, v as u32)
        }
        pub fn menu(&self) -> i32 {
            self.w(0x18) as i32
        }
        pub fn set_menu(&mut self, v: i32) {
            self.set_w(0x18, v as u32)
        }

        /// A fresh record before the parser fills it.
        ///
        /// orig 0x19d8d0
        pub fn init(&mut self) {
            self.set_w(0x1c, 0);
            self.set_w(0x10, 0xffff_ffff);
            self.set_w(0, 0);
            self.set_w(0x14, 0);
            self.set_w(0x18, 0);
        }

        /// Whether the record carries a jump (its type is not 0).
        ///
        /// orig 0x19d9c8
        pub fn is_jump(&self) -> bool {
            self.kind() != 0
        }
    }

    /// The destination bytes of the three places a [`JumpRec`] may point `FUN_0019d9d8` at.
    #[derive(Clone, Copy, PartialEq, Eq, Debug)]
    enum Target {
        Rec,
        LastExit,
        Local,
    }

    /// Keeps each program byte of `dst` that `src` leaves at 0xff.
    ///
    /// orig 0x104b90
    pub fn merge_programs(dst: &mut [u8], src: &[u8]) {
        for k in 0..3 {
            if src[k] != 0xff {
                dst[k] = src[k];
            }
        }
    }

    /// Lays `dest` over the place `cur` (0xff keeps a byte), then takes the three program ids from the save block's table for the
    /// resulting world and area and lets the destination's program bytes override them.
    ///
    /// orig 0x104bd0
    pub fn apply_destination(cur: &mut Dest6, dest: &Dest6, save: &[u8]) {
        for k in 0..3 {
            if dest[k] != 0xff {
                cur[k] = dest[k];
            }
        }
        let i = SAVE_PROGRAMS + cur[0] as usize * 0xc0 + cur[1] as usize * 3;
        let at = |k: usize| save.get(i + k).copied().unwrap_or(0);
        cur[3] = at(0);
        cur[4] = at(1);
        cur[5] = at(2);
        merge_programs(&mut cur[3..], &dest[3..]);
    }

    /// Whether two places have the same world and area.
    ///
    /// orig 0x105070
    pub fn same_area(a: &Dest6, b: &Dest6) -> bool {
        a[0] == b[0] && a[1] == b[1]
    }

    /// The place's file stem: the world's two-letter prefix and the area as two digits.
    ///
    /// orig 0x105010
    pub fn area_name(world_prefix: &str, area: u8) -> String {
        format!("{world_prefix}{area:02}")
    }

    /// The debug text for a place: the name, the entrance in parentheses, then the three program ids as letters when all are below
    /// 0x19 (each plus 0x61) and as numbers otherwise.
    ///
    /// orig 0x105098
    pub fn area_text(world_prefix: &str, rec: &Dest6) -> String {
        let mut s = format!("{}({}) ", area_name(world_prefix, rec[1]), rec[2]);
        if rec[3] < 0x19 && rec[4] < 0x19 && rec[5] < 0x19 {
            s += &format!("{}/{}/{}", (rec[3] + 0x61) as char, (rec[4] + 0x61) as char, (rec[5] + 0x61) as char);
        } else {
            s += &format!("{}/{}/{}", rec[3], rec[4], rec[5]);
        }
        s
    }

    /// The world whose two-letter prefix `name` is (0 when `name` is not two characters long or matches none of the 19 prefixes).
    ///
    /// orig 0x105d60
    pub fn world_of_prefix(name: &str, prefixes: &[&str]) -> u32 {
        if name.len() != 2 {
            return 0;
        }
        for (i, p) in prefixes.iter().take(19).enumerate() {
            if name.as_bytes() == &p.as_bytes()[..2.min(p.len())] {
                return i as u32;
            }
        }
        0
    }

    /// The entrance of the world map at which a world is entered from `world` (0 when not in the 15-row table of `(entrance, world)`).
    ///
    /// orig 0x149db8
    pub fn world_map_entrance(table: &[(u32, u32); 15], world: u32) -> u32 {
        table.iter().find(|(_, w)| *w == world).map_or(0, |(e, _)| *e)
    }

    /// Whether the region word is neither 1 nor 2 (`1 < region - 1` as an unsigned compare).
    ///
    /// orig 0x105ce8
    pub fn region_is_not_pal_or_ntsc_j(region: u32) -> bool {
        1 < region.wrapping_sub(1)
    }

    /// The address of the region's row (three words) in the table at 0x349d5c.
    ///
    /// orig 0x105d10
    pub fn region_row(region: u32) -> u32 {
        0x349d5c + region.wrapping_mul(12)
    }

    /// The word at `base + index * 4` of a table the caller read from the executable.
    ///
    /// orig 0x105de0
    pub fn table_word(table: &[u32], index: i32) -> u32 {
        table.get(index as u32 as usize).copied().unwrap_or(0)
    }

    /// Fills the allocator object at `obj` (its virtual table, the end `end`, the begin `begin` and the cursor words).
    ///
    /// orig 0x105f68
    pub fn allocator_reset(host: &mut impl AreaHost, obj: u32) {
        host.write32(obj + 0xc, 0);
        let b = host.read32(obj + 8);
        host.write32(obj + 0x14, b);
        let e = host.read32(obj + 4);
        host.write32(obj + 0x1c, e.wrapping_sub(obj.wrapping_add(0x14)));
        host.write32(obj + 0x18, 0);
        host.write32(obj + 0x20, obj);
        host.write32(obj + 0x10, 0);
    }

    /// Constructs the allocator object in place (its virtual table pointer, the two bounds) and resets it.
    ///
    /// orig 0x105f18
    /// orig 0x105f40
    pub fn allocator_construct(host: &mut impl AreaHost, obj: u32, a: u32, b: u32) {
        host.write32(obj, ALLOCATOR_VTABLE);
        host.write32(obj + 8, b);
        host.write32(obj + 4, a);
        allocator_reset(host, obj);
    }

    /// The virtual table of the allocator class (an address in the executable's data).
    pub const ALLOCATOR_VTABLE: u32 = 0x349e20;

    /// Sets only the virtual table pointer of the allocator object.
    ///
    /// orig 0x105f98
    /// orig 0x105fa8
    pub fn allocator_set_vtable(host: &mut impl AreaHost, obj: u32) {
        host.write32(obj, ALLOCATOR_VTABLE);
    }

    /// The destructor: sets the vtable pointer, then frees the load data.
    ///
    /// orig 0x105fb8
    pub fn allocator_destroy(host: &mut impl AreaHost, obj: u32) {
        host.write32(obj, ALLOCATOR_VTABLE);
        host.call(0x106280, &[]);
    }

    /// Makes an allocator over `size` bytes from `base`: the object sits at `base` rounded up to 16 and takes the first 0x30 bytes;
    /// 0 when the room is not larger than 0x50 bytes.
    ///
    /// orig 0x105fd8
    pub fn create_memory_allocator(host: &mut impl AreaHost, base: u32, size: u32) -> u32 {
        let obj = base.wrapping_add(0xf) & 0xffff_fff0;
        if obj.wrapping_add(0x50) < base.wrapping_add(size) {
            allocator_construct(host, obj, obj.wrapping_add(0x30), base.wrapping_add(size));
            obj
        } else {
            0
        }
    }

    /// Mirrors `FUN_001688a0`: the game's one-time init, a fixed run of calls.
    ///
    /// orig 0x1688a0
    pub fn game_init(host: &mut impl AreaHost) {
        host.call(0x1681a0, &[u64::MAX]);
        for c in [0x166fd0, 0x1af928, 0x16e1e0, 0x17fd10, 0x193610, 0x194d50, 0x194ee0, 0x198058, 0x1ad100, 0x1d3408, 0x1a8ff8, 0x19e088, 0x190f00, 0x1af940] {
            host.call(c, &[]);
        }
    }

    /// The title/boot reset of the area-side state.
    ///
    /// orig 0x168928
    pub fn boot_reset(host: &mut impl AreaHost, ctl: &mut AreaCtl) {
        host.write32(0x522178, 0xffff_ffff);
        host.call(0x16fdf0, &[0]);
        host.call(0x178a38, &[]);
        clear_type4_memory(ctl);
        host.call(0x1d3a30, &[]);
    }

    /// Mirrors `FUN_00168960`.
    ///
    /// orig 0x168960
    pub fn mode_exit(host: &mut impl AreaHost, mode: i64) {
        host.call(0x194318, &[]);
        host.call(0x1adfd0, &[]);
        host.call(0x1b81f8, &[]);
        if mode != 0xd && host.read32(0x522178) != 0xd {
            return;
        }
        host.call(0x16fdf0, &[0]);
    }

    /// Reads for the area: the field files, the mission prepare and the voice read, each with the thread argument.
    ///
    /// orig 0x168ae0
    pub fn area_read(host: &mut impl AreaHost, thread: u64) {
        host.call(0x16fb20, &[]);
        host.call(0x1b8e88, &[]);
        host.call(0x1ae070, &[thread]);
    }

    /// Mirrors `FUN_00168ca0`: the exits of the story-side systems when an area is left.
    ///
    /// orig 0x168ca0
    pub fn area_exit(host: &mut impl AreaHost) {
        host.call(0x1a44a8, &[]);
        host.call(0x1b8800, &[]);
        host.call(0x1b0a10, &[]);
        host.call(0x1b9518, &[]);
        host.call(0x190ea8, &[2]);
        host.call(0x1b6490, &[]);
        host.call(0x1966b8, &[]);
    }

    /// Mirrors `FUN_00168ce8`: the finalisers of the story-side systems when an area unloads.
    ///
    /// orig 0x168ce8
    pub fn area_finalize(host: &mut impl AreaHost) {
        for c in [0x1b3798, 0x1a2260, 0x18c448, 0x16c0a8, 0x181af8, 0x1d1bd0, 0x1a5108] {
            host.call(c, &[]);
        }
    }

    /// Whether the area's script variables `flags` (0x35d268) let the magic and mission reads through.
    ///
    /// orig 0x168b10
    pub fn area_data_read(host: &mut impl AreaHost, ctl: &AreaCtl, flags: u32) {
        host.call(0x190e40, &[ctl.cur[0] as u64, 0x410]);
        host.call(0x1b63d8, &[]);
        host.call(0x1a21a8, &[]);
        if flags & 0x80000 == 0 {
            host.call(0x1a4ae0, &[]);
        }
        host.call(0x1a3fe0, &[]);
        if flags & 0x200 == 0 && flags & 0x400 == 0 {
            host.call(0x1bda48, &[]);
        }
        host.call(0x181828, &[]);
        host.call(0x1c66a0, &[1]);
        host.call(0x1b8eb8, &[]);
    }

    /// The init of the story-side systems when an area starts.
    ///
    /// orig 0x168ba8
    pub fn area_systems_init(host: &mut impl AreaHost, ctl: &AreaCtl) {
        host.call(0x190e70, &[ctl.cur[0] as u64, 2]);
        host.call(0x1aad28, &[]);
        host.call(0x1b6408, &[]);
        host.call(0x16c088, &[sx(ctl.allocator)]);
        for c in [0x18ece8, 0x1b72f0, 0x17cc70, 0x184ab8, 0x1ad060, 0x195630, 0x19f450, 0x18c3f0, 0x1a45c0, 0x197f18, 0x1a5030, 0x1afa28, 0x19d580, 0x1b2770, 0x178a58, 0x1c68f8, 0x1d1bc8, 0x1d3a58, 0x1a21e0, 0x1b87d8, 0x1a3f98, 0x1b0a08, 0x181890] {
            host.call(c, &[]);
        }
    }

    /// The area-start step of the display side: reads the area's map data.
    ///
    /// orig 0x149b08
    pub fn display_read_data(host: &mut impl AreaHost) {
        host.call(0x13a7e8, &[]);
        host.call(0x13a6d0, &[]);
        host.call(0x1492b8, &[0x355e98]);
        host.call(0x149328, &[0x355e98]);
    }

    /// Hands the map visibility word to the display side.
    ///
    /// orig 0x149b40
    pub fn display_visibility(host: &mut impl AreaHost, ctl: &AreaCtl, word: u64) {
        host.call(0x149348, &[0x355e98]);
        let u = host.call(0x1491e0, &[0x355e98]);
        host.call(0x15bd40, &[u]);
        host.call(0x15e6c0, &[0x355e98, sx(ctl.allocator), 1, word]);
    }

    /// orig 0x149ba0
    pub fn display_init(host: &mut impl AreaHost) {
        host.call(0x13dc40, &[]);
    }

    /// Starts the display side of an area: layers, camera, menus, and the fade-in when nothing else takes the screen.
    ///
    /// orig 0x149bb8
    pub fn display_start(host: &mut impl AreaHost, ctl: &mut AreaCtl) {
        host.call(0x13a8a8, &[]);
        host.call(0x103a48, &[u64::MAX]);
        host.call(0x13a140, &[sx(ctl.allocator)]);
        ctl.layout_alloc = ctl.allocator;
        for c in [0x1537e0, 0x138828, 0x109c20, 0x15da80, 0x140c00, 0x1619b0, 0x1613b8, 0x161380, 0x15a358] {
            host.call(c, &[]);
        }
        host.call(0x14eed8, &[0x355e98]);
        host.call(0x14ee38, &[0x355e98]);
        host.call(0x14ee78, &[]);
        host.call(0x14ed88, &[]);
        host.call(0x13d6e8, &[]);
        if !same_area(&ctl.cur, &ctl.prev) && host.call(0x1b9550, &[]) == 0 {
            host.call(0x15ade8, &[]);
            host.call(0x13d6d8, &[]);
        }
    }

    /// orig 0x149cb8
    pub fn display_exit_a(host: &mut impl AreaHost, ctl: &mut AreaCtl) {
        host.call(0x15daf0, &[]);
        ctl.layout_alloc = 0;
    }

    /// The display side's teardown when an area is left.
    ///
    /// orig 0x149ce0
    pub fn display_exit_b(host: &mut impl AreaHost) {
        for c in [0x15a398, 0x153800, 0x14edb8, 0x14ee90, 0x14ee50, 0x14eef8, 0x15e8d0, 0x165388] {
            host.call(c, &[]);
        }
        host.call(0x149ad0, &[0x355e98]);
    }

    /// Runs the reset of the display side when called with (1, 0xffff).
    ///
    /// orig 0x149d40
    pub fn display_reset(host: &mut impl AreaHost, a: i32, b: i64) {
        if b == 0xffff && a == 1 {
            host.call(0x1490d8, &[0x355e98]);
        }
    }

    /// orig 0x149d78
    pub fn display_reset_all(host: &mut impl AreaHost) {
        display_reset(host, 1, 0xffff);
    }

    /// Opens the field's pause poll: stop flag cleared, the poll task made.
    ///
    /// orig 0x14a1e0
    pub fn poll_start(host: &mut impl AreaHost, ctl: &mut AreaCtl) {
        ctl.poll_paused = 0;
        poll_clear_stop(ctl);
        let m = host.read32(0x33e69c);
        host.call(0x102778, &[sx(m), 0, 0x1c908, 0x14a080]);
    }

    /// orig 0x149fa8
    pub fn poll_set_stop(ctl: &mut AreaCtl) {
        ctl.poll_stop = 1;
    }

    /// orig 0x149fb8
    pub fn poll_clear_stop(ctl: &mut AreaCtl) {
        ctl.poll_stop = 0;
    }

    /// orig 0x149fc8
    pub fn poll_stop_set(ctl: &AreaCtl) -> u8 {
        ctl.poll_stop
    }

    /// Whether the camp menu state answers 1.
    ///
    /// orig 0x149fd8
    pub fn camp_open(host: &mut impl AreaHost) -> bool {
        host.call(0x2c0bd0, &[]) == 1
    }

    /// orig 0x14a000
    pub fn camp_idle(host: &mut impl AreaHost) -> bool {
        if host.call(0x2c0bd0, &[]) == 1 {
            return false;
        }
        host.call(0x1acf10, &[]) == 0
    }

    /// orig 0x14a038
    pub fn pause_released(host: &mut impl AreaHost) -> bool {
        host.call(0x2fee00, &[1]) == 2
    }

    /// orig 0x14a060
    pub fn pause_step(host: &mut impl AreaHost) {
        host.call(0x1674f8, &[0]);
        host.call(0x14cdd0, &[]);
    }

    /// Makes the field's second task and starts the screen effect.
    ///
    /// orig 0x149f70
    pub fn field_effect_start(host: &mut impl AreaHost, ctl: &AreaCtl) {
        host.call(0x13a878, &[]);
        let t = host.call(0x102bc8, &[0, 75000, 0x14a5a0]);
        let _ = ctl;
        host.write8((t as u32).wrapping_add(4), 0);
        host.call(0x14ab80, &[]);
    }

    /// Ends the task block `task` (its function word is cleared).
    ///
    /// orig 0x102498
    pub fn end_task(host: &mut impl AreaHost, task: u32) {
        host.write32(task + TASK_FUNC, 0);
    }

    /// The area thread: the coroutine that owns the area while it is loaded, as a straight run of its steps. Each round it reads the
    /// area's files (`0x1689c0`), asks whether the story side wants a jump (`0x1051f0` on the current place and the jump record),
    /// and when it does makes the area allocator, waits (yielding to the task manager with `0x102438(thread, 0)`) until the jump record is
    /// ready (`0x1a6b30`, filled by `0x1a6b50`), saves the current place as the saved place (`G_SAVED`, six bytes), sets the gate byte, plays
    /// the jump, yields once, clears the gate and tears the area down (`0x102678`, `0x1dbfc8`, `0x1052a0`, `0x181af8`, `0x105188` on the saved
    /// place). When no round is wanted any more it makes the allocator for the final area, runs the area's start-up calls and yields until
    /// the byte at 0x355830 (`0x13ee98`) is set, then creates the next task (`0x102bc8`). `rec` is the address of the jump record on the
    /// thread's stack: the destination bytes (+4..+9) start as 0xff and the word at +0x1c as 0 before the record's init `0x19d8d0`.
    /// Every callee goes through the host, so the call log is the thread's sequence; a yield is the host call of `0x102438` returning
    /// (the thread switch itself is not ported: `not-needed:hw`).
    ///
    /// orig 0x1048e0
    pub fn area_thread(host: &mut impl AreaHost, thread: u64, rec: u32) {
        for o in 4..10 {
            host.write8(rec + o, 0xff);
        }
        host.write32(rec + 0x1c, 0);
        host.call(0x19d8d0, &[rec as u64]);
        host.call(0x14cbd0, &[]);
        host.call(0x108f68, &[]);
        loop {
            host.call(0x1689c0, &[thread]);
            if host.call(0x1051f0, &[G_CUR as u64, rec as u64]) == 0 {
                break;
            }
            host.call(0x16f260, &[6, 0x8c0000]);
            host.call(0x105250, &[0x380000, 0x100000]);
            let alloc = host.read32(G_ALLOC);
            host.call(0x1dbe90, &[sx(alloc), 0x80000, 0x20000]);
            host.call(0x1dbe78, &[]);
            while host.call(0x1a6b30, &[]) == 0 {
                let v = host.call(0x1a6b50, &[rec as u64]);
                host.call(0x138540, &[v]);
                host.call(0x102438, &[thread, 0]);
            }
            for o in 0..6 {
                let b = host.read8(G_CUR + o);
                host.write8(G_SAVED + o, b);
            }
            host.write8(G_GATE, 1);
            host.call(0x19d9d8, &[rec as u64, 0]);
            host.call(0x102438, &[thread, 0]);
            let mgr = host.read32(G_TASK_MGR);
            host.write8(G_GATE, 0);
            host.call(0x102678, &[sx(mgr), 8, 8]);
            host.call(0x1dbfc8, &[]);
            host.call(0x1052a0, &[]);
            host.call(0x16f260, &[6, 0xc0000]);
            host.call(0x181af8, &[]);
            host.call(0x105188, &[G_SAVED as u64]);
        }
        let (size, heap) = (host.read32(0x35cd20), host.read32(0x35cd24));
        host.call(0x105250, &[sx(size.wrapping_add(heap)), sx(heap)]);
        host.call(0x16f260, &[6, 0xc0000]);
        let v = host.call(0x224b50, &[]);
        host.call(0x2c1158, &[v]);
        host.call(0x149ba0, &[]);
        host.call(0x1368e8, &[thread]);
        host.call(0x137150, &[]);
        host.call(0x168ae0, &[thread]);
        host.call(0x13aaa8, &[]);
        host.call(0x149b08, &[]);
        host.call(0x1dbe78, &[]);
        host.call(0x168b10, &[]);
        host.call(0x16fb20, &[thread]);
        host.call(0x136aa0, &[]);
        host.call(0x136ec0, &[thread]);
        while crate::system_tables::byte_355830(&[host.read8(0x355830)]) == 0 {
            host.call(0x102438, &[thread, 0]);
        }
        host.call(0x102bc8, &[0, 1000, 0x104b60]);
    }

    /// Whether the current region word is the PAL one (`FUN_00105cd8` returns it).
    pub const REGION_WORD: u32 = 0x349d44;

    impl AreaCtl {
        /// Writes the destination into the current place and the save block's world, area and entrance bytes.
        ///
        /// orig 0x105188
        pub fn commit(&mut self, dest: &Dest6, save: &mut [u8]) {
            apply_destination(&mut self.cur, dest, save);
            save[SAVE_ENTRANCE] = self.cur[2];
            save[SAVE_WORLD] = self.cur[0];
            save[SAVE_AREA] = self.cur[1];
        }

        /// Applies the pending destination, ends the gate and starts the area thread.
        ///
        /// orig 0x104c68
        pub fn start_thread(&mut self, host: &mut impl AreaHost, dest: &Dest6, save: &mut [u8]) {
            self.gate = 0;
            self.commit(dest, save);
            host.call(0x102808, &[sx(self.task_mgr), 1, 5000, 0x1048e0, 0x2000]);
        }

        /// The area is entered: display, systems and the fade-in start, in that order.
        ///
        /// orig 0x1047a8
        pub fn area_start(&mut self, host: &mut impl AreaHost, fx_heap: (u32, u32), map_visibility: u64) {
            self.active = 1;
            host.call(0x1021e0, &[]);
            host.call(0x1cafa0, &[]);
            for c in [0x14a978, 0x14a928, 0x14a938, 0x14a958, 0x14a968, 0x14a948, 0x14a918] {
                let u = host.call(c, &[]);
                host.call(0x14a7f0, &[u, 0, 0]);
            }
            host.call(0x1dbe90, &[sx(self.allocator), sx(fx_heap.0), sx(fx_heap.1)]);
            host.call(0x1ddd10, &[]);
            display_visibility(host, self, map_visibility);
            area_systems_init(host, self);
            host.call(0x107ec8, &[]);
            display_start(host, self);
            host.call(0x223280, &[]);
            host.call(0x137128, &[]);
            host.call(0x13efe8, &[]);
            host.call(0x1b94e8, &[]);
        }

        /// The task that runs the area start, then ends itself.
        ///
        /// orig 0x104b60
        pub fn start_task(&mut self, host: &mut impl AreaHost, task: u32, fx_heap: (u32, u32), map_visibility: u64) {
            self.area_start(host, fx_heap, map_visibility);
            end_task(host, task);
        }

        /// Tears the area down.
        ///
        /// orig 0x104ce0
        pub fn unload(&mut self, host: &mut impl AreaHost) {
            self.active = 0;
            host.call(0x167878, &[]);
            area_exit(host);
            host.call(0x107ee0, &[]);
            display_exit_a(host, self);
            host.call(0x102678, &[sx(self.task_mgr), 1, 1]);
            area_finalize(host);
            display_exit_b(host);
            host.call(0x1dbfc8, &[]);
            host.call(0x13ab08, &[G_PENDING as u64]);
            host.call(0x180948, &[]);
            self.destroy_allocator(host);
        }

        /// Frees the area allocator.
        ///
        /// orig 0x1052a0
        pub fn destroy_allocator(&mut self, host: &mut impl AreaHost) {
            host.call(0x1090a8, &[sx(self.allocator)]);
            host.call(0x16ff18, &[sx(self.allocator)]);
            host.call(0x16f8e0, &[0]);
            self.allocator = 0;
        }

        /// Makes the area allocator over `size` bytes at `base` (the base comes from the cache unit; the size is the first argument).
        ///
        /// orig 0x105250
        pub fn create_allocator(&mut self, host: &mut impl AreaHost, size: u32, extra: u32) {
            let base = host.call(0x16fe70, &[]);
            let obj = create_memory_allocator(host, base as u32, size);
            self.allocator = obj;
            host.call(0x108ff0, &[obj as u64, 0x100, extra as u64]);
        }

        /// Allocates `size` bytes from the area allocator (its virtual method 0).
        ///
        /// orig 0x104fa8
        pub fn alloc(&mut self, host: &mut impl AreaHost, size: u64) -> u64 {
            host.vcall(self.allocator, 0, &[size])
        }

        /// Frees a block of the area allocator (its virtual method 1).
        ///
        /// orig 0x104fe0
        pub fn free(&mut self, host: &mut impl AreaHost, p: u64) -> u64 {
            host.vcall(self.allocator, 1, &[p])
        }

        /// The allocator object.
        ///
        /// orig 0x104f98
        pub fn get_allocator(&self) -> u32 {
            self.allocator
        }

        /// Clears the jump task word.
        ///
        /// orig 0x104f88
        pub fn clear_task(&mut self) {
            self.task = 0;
        }

        /// Whether a jump is running.
        ///
        /// orig 0x1051e0
        pub fn jump_running(&self) -> bool {
            self.task != 0
        }

        /// Whether the area thread has to run another round: the story side asks for a jump record (two checks, the first wins).
        ///
        /// orig 0x1051f0
        pub fn more_to_play(&mut self, host: &mut impl AreaHost, cur_addr: u32, rec_addr: u32) -> bool {
            if host.call(0x1d30d8, &[cur_addr as u64, rec_addr as u64]) == 0 {
                host.call(0x1d3a68, &[cur_addr as u64, rec_addr as u64]) != 0
            } else {
                true
            }
        }

        /// Sets all five place records to 0xff when called with (1, 0xffff): current, pending, last exit, previous and saved.
        ///
        /// orig 0x1052e0
        pub fn reset(&mut self, a: i32, b: i64) {
            if b == 0xffff && a == 1 {
                self.cur = [0xff; 6];
                self.pending = [0xff; 6];
                self.last_exit = [0xff; 6];
                self.prev = [0xff; 6];
                self.saved = [0xff; 6];
            }
        }

        /// orig 0x105420
        pub fn reset_all(&mut self) {
            self.reset(1, 0xffff);
        }

        /// Starts a jump to `dest`. With the gate set (the area thread is playing its end) only the fade kind and the saved destination
        /// are recorded; otherwise a jump task is made that waits `wait` ticks, fades with `fade` and applies the destination.
        ///
        /// orig 0x104e18
        pub fn begin_jump(&mut self, host: &mut impl AreaHost, dest: &Dest6, fade: u32, wait: i32, flag: u8, reset: i32) {
            if self.gate == 0 {
                host.call(0x167878, &[]);
                let t = host.call(0x102778, &[sx(self.task_mgr), 0, 300000, 0x104d58]) as u32;
                self.task = t;
                host.write32(t + TASK_WAIT, wait as u32);
                host.write8(t + TASK_FLAG, flag);
                host.write32(t + TASK_FADE, fade);
                if wait == 0 {
                    host.write32(t + TASK_RESET, reset as u32);
                    host.call(0x13ef68, &[fade as u64]);
                } else {
                    host.call(0x1abfe8, &[sx(reset as u32)]);
                    host.write32(t + TASK_RESET, 0);
                }
                host.call(0x1c6bd0, &[0x35fec8, 2]);
                self.pending = *dest;
            } else {
                host.call(0x13ef68, &[(fade | 0x8000_0000) as u64]);
                self.saved = *dest;
                if reset != 0 {
                    host.call(0x1abfe8, &[sx(reset as u32)]);
                }
            }
        }

        /// The jump task's body, run each tick: counts the wait down, fades, and once no menu window is up unloads the area,
        /// clears the previous world on a world change, applies the destination and starts the area thread.
        ///
        /// orig 0x104d58
        pub fn jump_task(&mut self, host: &mut impl AreaHost, save: &mut [u8], task: u32) {
            let wait = host.read32(task + TASK_WAIT) as i32;
            if wait > 0 {
                let w = wait - 1;
                host.write32(task + TASK_WAIT, w as u32);
                if w > 0 {
                    return;
                }
                let fade = host.read32(task + TASK_FADE);
                host.call(0x13ef68, &[fade as u64]);
            }
            if host.call(0x1039d0, &[]) == 0 && host.call(0x2f4c40, &[]) == 0 {
                let reset = host.read32(task + TASK_RESET);
                if reset != 0 {
                    host.call(0x1abfe8, &[sx(reset)]);
                }
                self.unload(host);
                if host.read8(task + TASK_FLAG) != 0 {
                    host.call(0x1cb208, &[]);
                    self.prev[0] = 0xff;
                }
                let dest = self.pending;
                self.start_thread(host, &dest, save);
                self.clear_task();
                end_task(host, task);
            }
        }

        /// Walks into an exit: remembers its destination, asks the story side whether a trigger takes the exit and otherwise starts
        /// the jump (a fade of kind 1 and a party reset when the world changes). `flags` is the exit group's flag word (bit 0 set).
        ///
        /// orig 0x198fa0
        pub fn exit_walk_in(&mut self, host: &mut impl AreaHost, flags: &mut u32, group_id: u16, dest: &Dest6) {
            self.last_exit = *dest;
            if host.call(0x197b70, &[1, group_id as u64]) == 0 {
                let changed = (self.cur[0] != dest[0]) as u32;
                let d = self.last_exit;
                self.begin_jump(host, &d, changed, 0, 0, changed as i32);
            }
            *flags |= 1;
        }

        /// Jumps to the world map (world 0xf) from `world` (negative: the save block's world); `fade` gets bit 0 added.
        ///
        /// orig 0x149df0
        pub fn to_world_map(&mut self, host: &mut impl AreaHost, save: &mut [u8], table: &[(u32, u32); 15], world: i64, fade: u32) {
            host.call(0x197b70, &[0x21, 0]);
            let w = if world < 0 { save[SAVE_WORLD] as u32 } else { world as u32 };
            let mut dest = [0xff; 6];
            save[SAVE_WORLD] = 0xf;
            dest[0] = 0xf;
            save[SAVE_AREA] = 0;
            dest[1] = 0;
            let e = world_map_entrance(table, w) as u8;
            dest[2] = e;
            save[SAVE_ENTRANCE] = e;
            self.begin_jump(host, &dest, fade | 1, 0, 0, 0);
        }

        /// Starts the title-screen task.
        ///
        /// orig 0x149f38
        pub fn title_start(&mut self, host: &mut impl AreaHost) {
            host.call(0x102808, &[sx(self.task_mgr), 0, 300000, 0x149eb0, 0x1000]);
        }

        /// The title-screen task: freezes the frame, fades to black, waits until the fade reports black, then starts the title mode.
        /// `yield_until_black` is the yield loop (the fader's state word is polled between yields).
        ///
        /// orig 0x149eb0
        pub fn title_task(&mut self, host: &mut impl AreaHost, task: u64) {
            host.call(0x14dcb0, &[]);
            host.call(0x14de00, &[]);
            let f = host.call(0x14a918, &[]);
            host.call(0x14a7c8, &[f, 0xf, 0]);
            loop {
                let fader = host.call(0x14a918, &[]) as u32;
                if host.read32(fader.wrapping_add(0xfc)) == 4 {
                    break;
                }
                host.call(0x102438, &[task, 0]);
            }
            let mode = host.call(0x102f18, &[]) as u32;
            host.vcall(mode, 0, &[]);
        }

        /// Sets the world map as the place and enters the field mode of the save-point return.
        ///
        /// orig 0x105e28
        pub fn leave_to_map_entrance(&mut self, host: &mut impl AreaHost, save: &mut [u8], entrance_table: &[u32]) {
            save[SAVE_WORLD] = 0xf;
            save[SAVE_AREA] = 0;
            let i = host.call(0x104058, &[]) as i32;
            save[SAVE_ENTRANCE] = table_word(entrance_table, i) as u8;
            let m = host.call(0x102c30, &[]) as u32;
            host.vcall(m, 0, &[]);
        }

        /// The same with entrance 0.
        ///
        /// orig 0x105ea8
        pub fn leave_to_map(&mut self, host: &mut impl AreaHost, save: &mut [u8]) {
            save[SAVE_WORLD] = 0xf;
            save[SAVE_AREA] = 0;
            save[SAVE_ENTRANCE] = 0;
            let m = host.call(0x102c30, &[]) as u32;
            host.vcall(m, 0, &[]);
        }

        /// The word of the entrance table the current mode selects.
        ///
        /// orig 0x105df8
        pub fn entrance_word(&mut self, host: &mut impl AreaHost, table: &[u32]) -> u32 {
            let i = host.call(0x104058, &[]) as i32;
            table_word(table, i)
        }
    }

    /// Looks up a story program's area in `07localset.bin`: `out` gets (world, area, 0, key, key, key) for the first pair whose
    /// program byte equals `key`, or stays 0xff when none does. A negative `world` means the current one. The file's world entry is
    /// found through the file unit (`FUN_001684c0`) and read from memory (u16 count, then a u16 per pair: program byte, area byte).
    ///
    /// orig 0x198080
    pub fn localset(host: &mut impl AreaHost, ctl: &AreaCtl, key: u64, world: i64) -> Dest6 {
        let mut out = [0xff; 6];
        let w = if world < 0 { ctl.cur[0] as u64 } else { world as u64 };
        let e = host.call(0x1684c0, &[sx(ctl.localset_file), 2, w]) as u32;
        let data = host.read32(e.wrapping_add(8));
        let count = host.read16(data) as u32;
        let mut p = data;
        for _ in 0..count {
            if host.read8(p.wrapping_add(2)) as u64 == key {
                out[0] = w as u8;
                out[1] = host.read8(p.wrapping_add(3));
                out[2] = 0;
                out[3] = key as u8;
                out[4] = key as u8;
                out[5] = key as u8;
                return out;
            }
            p = p.wrapping_add(2);
        }
        out
    }

    /// Fills a jump record from the six shorts of a trigger's jump sub-command.
    ///
    /// orig 0x19d8f0
    pub fn parse_jump(host: &mut impl AreaHost, ctl: &AreaCtl, rec: &mut JumpRec, p: &[i16; 6]) {
        rec.set_kind(p[0] as i32 as u32);
        rec.set_fade(p[5] as i32 as u32);
        match p[0] {
            1 => {
                rec.0[4] = p[1] as u8;
                rec.0[5] = p[2] as u8;
                rec.0[7] = 0xff;
                rec.0[6] = p[3] as u8;
                rec.0[9] = 0xff;
                rec.0[8] = 0xff;
            }
            2 => {
                let out = localset(host, ctl, p[4] as i64 as u64, p[1] as i64);
                rec.0[4..8].copy_from_slice(&out[..4]);
                rec.0[8] = out[4];
                rec.0[9] = out[5];
                rec.0[6] = p[3] as u8;
            }
            4 => rec.0[4] = p[1] as u8,
            _ => {}
        }
    }

    /// The story flag test (`FUN_001802e0`) through the host.
    fn flag(host: &mut impl AreaHost, f: u32) -> bool {
        host.call(0x1802e0, &[f as u64]) != 0
    }

    /// Plays a jump record: picks the destination by the record's type (1 and 2 its own bytes, 3 the last exit, 4 a world-map case
    /// decided by story flags, 5 the report menu, 6 the title fade), applies the party arrangement and the "menu before" code, and
    /// starts the jump. `extra` is OR-ed into the fade word. `fade_wait` is how the jump waits: 2 ticks after a menu.
    ///
    /// orig 0x19d9d8
    pub fn play_jump(ctl: &mut AreaCtl, host: &mut impl AreaHost, save: &mut [u8], map_table: &[(u32, u32); 15], rec: &mut JumpRec, extra: u32) {
        let mut local: Dest6 = [0xff; 6];
        let mut target: Option<Target> = None;
        let mut wait: u32 = 0;
        if ctl.party_ready == 0 {
            host.call(0x1a71a8, &[0x1c8bda0]);
            ctl.party_ready = 1;
        }
        let f = rec.fade();
        let mut fade: u32 = 0;
        if (f as i32) >= 0 {
            fade = f;
            if f & 0x4000 != 0 {
                fade = (f & 0xffff_bfff) | 0x8000_0000;
            }
        }
        fade |= extra;
        let party;
        match rec.kind() {
            1 | 2 => {
                target = Some(Target::Rec);
                party = rec.party();
            }
            3 => {
                target = Some(Target::LastExit);
                party = rec.party();
            }
            4 => {
                fade = 1;
                let mut chosen = false;
                if flag(host, 0x1071) && !flag(host, 0x1072) && (flag(host, 0x140f) || flag(host, 0x1822) || flag(host, 0x200b)) {
                    ctl.type4_memory = rec.0[4] as u32;
                    target = Some(Target::Rec);
                    rec.0[5] = 0;
                    rec.0[6] = 0;
                    rec.0[4] = 4;
                    rec.0[7] = 0xff;
                    rec.0[9] = 0xff;
                    rec.0[8] = 0xff;
                    chosen = true;
                }
                if !chosen {
                    if !flag(host, 0x8cd) && (flag(host, 0x3834) || flag(host, 0x1c1c)) {
                        ctl.type4_memory = rec.0[4] as u32;
                        target = Some(Target::Rec);
                        let out = localset(host, ctl, 0x40, 0x12);
                        rec.0[4..8].copy_from_slice(&out[..4]);
                        rec.0[8] = out[4];
                        rec.0[9] = out[5];
                    } else {
                        if (ctl.type4_memory as i32) >= 0 {
                            rec.0[4] = ctl.type4_memory as u8;
                            ctl.type4_memory = 0xffff_ffff;
                        }
                        ctl.to_world_map(host, save, map_table, rec.0[4] as i64, 0);
                    }
                }
                party = rec.party();
            }
            5 => {
                host.call(0x24f0b8, &[0xe, u64::MAX]);
                party = rec.party();
            }
            6 => {
                ctl.title_start(host);
                party = rec.party();
            }
            _ => {
                party = rec.party();
            }
        }
        let target_byte0 = |ctl: &AreaCtl, rec: &JumpRec, local: &Dest6, t: Option<Target>| -> Option<u8> {
            t.map(|t| match t {
                Target::Rec => rec.0[4],
                Target::LastExit => ctl.last_exit[0],
                Target::Local => local[0],
            })
        };
        if party >= 0 {
            let who = match target_byte0(ctl, rec, &local, target) {
                Some(b) => b as u64,
                None => u64::MAX,
            };
            let p = host.call(0x1a71c0, &[who]) as u32;
            ctl.party_word = host.read32(p);
            host.call(0x1a7330, &[p as u64, sx(rec.party() as u32)]);
            let m = rec.menu();
            if m != 0 {
                wait = 0;
                if m != 1 {
                    wait = 2;
                    if m == 2 {
                        host.call(0x1d73d0, &[ctl.party_word as u64, p as u64]);
                    }
                }
                if m == 1 || m == 2 {
                    host.call(0x24f300, &[wait as u64, 0x1c8bda0]);
                    wait = 2;
                }
            }
        }
        match target {
            None => {
                if rec.reset() != 0 {
                    host.call(0x1abfe8, &[sx(rec.reset() as u32)]);
                }
            }
            Some(t) => {
                if !flag(host, 0x872) && flag(host, 0x86e) && flag(host, 0x86f) && flag(host, 0x870) && flag(host, 0x871) {
                    local = localset(host, ctl, 0x7a, 2);
                    target = Some(Target::Local);
                } else {
                    target = Some(t);
                }
                let dest: Dest6 = match target.unwrap() {
                    Target::Rec => rec.dest(),
                    Target::LastExit => ctl.last_exit,
                    Target::Local => local,
                };
                ctl.begin_jump(host, &dest, fade, wait as i32, 0, rec.reset());
            }
        }
    }

    /// Group records: the 64-slot table at 0x1c8af90, 0x38 bytes each.
    pub const GROUP_BASE: u32 = 0x1c8af90;
    pub const GROUP_COUNT: usize = 64;
    pub const GROUP_SIZE: usize = 0x38;

    /// One group record. +0 kind, +4 flags, +8 the group header's address, +0xc head and +0x10 tail of the created objects,
    /// +0x14 a float for kind 2, +0x20 a byte, +0x24..+0x34 pointers into the header.
    #[derive(Clone, PartialEq, Eq, Debug)]
    pub struct GroupRec(pub [u8; GROUP_SIZE]);

    impl Default for GroupRec {
        fn default() -> Self {
            GroupRec([0; GROUP_SIZE])
        }
    }

    impl GroupRec {
        pub fn w(&self, o: usize) -> u32 {
            u32::from_le_bytes(self.0[o..o + 4].try_into().unwrap())
        }
        pub fn set_w(&mut self, o: usize, v: u32) {
            self.0[o..o + 4].copy_from_slice(&v.to_le_bytes());
        }

        /// Fills the record from the group header at `hdr` in the area file: the sections' addresses, the start-disabled flag and the
        /// signals (events that enable or disable the group).
        ///
        /// orig 0x198340
        /// orig 0x198430
        pub fn init(&mut self, host: &mut impl AreaHost, kind: u32, hdr: u32) {
            self.set_w(8, hdr);
            self.set_w(0, kind);
            self.set_w(4, 0);
            self.set_w(0x24, hdr.wrapping_add(0x2c));
            self.set_w(0x10, 0);
            self.set_w(0xc, 0);
            self.set_w(0x14, 0);
            self.0[0x20] = 0;
            let n4 = host.read16(hdr.wrapping_add(4)) as u32;
            let p10 = hdr.wrapping_add(0x2c).wrapping_add(n4.wrapping_mul(0x40));
            self.set_w(0x28, p10);
            let n6 = host.read16(hdr.wrapping_add(6)) as u32;
            let p11 = p10.wrapping_add(n6.wrapping_mul(0x40));
            self.set_w(0x2c, p11);
            let mut n8 = host.read16(hdr.wrapping_add(8)) as u32;
            let mut p = p11;
            if n8 == 0 {
                self.set_w(0x30, p);
            } else {
                loop {
                    n8 -= 1;
                    let c = host.read16(p.wrapping_add(2)) as u32;
                    p = p.wrapping_add(c.wrapping_mul(0xc)).wrapping_add(8);
                    if n8 == 0 {
                        break;
                    }
                }
                self.set_w(0x30, p);
            }
            let n10 = host.read16(hdr.wrapping_add(10)) as u32;
            let signals = p.wrapping_add(n10.wrapping_mul(0x10));
            self.set_w(0x34, signals);
            let k = host.read8(hdr) as i8;
            if k == 2 {
                let v = host.read16(hdr.wrapping_add(0x1c)) as f32;
                self.set_w(0x14, v.to_bits());
            } else if k == 5 {
                self.set_w(4, self.w(4) | 1);
            }
            if host.read8(hdr.wrapping_add(1)) & 1 != 0 {
                self.set_w(4, self.w(4) | 1);
            }
            let count = host.read16(hdr.wrapping_add(0xc)) as i16 as i32;
            let mut i = 0i32;
            if count != 0 {
                loop {
                    let sig = self.w(0x34).wrapping_add((i as u32).wrapping_mul(8));
                    let ev = host.read16(sig) as i16;
                    let arg: u64 = if ev != 0x4a { host.read16(sig.wrapping_add(2)) as u64 } else { u64::MAX };
                    let t = host.call(0x1975e0, &[ev as u16 as u64, arg]);
                    let act = host.read8(self.w(0x34).wrapping_add((i as u32).wrapping_mul(8)).wrapping_add(4));
                    let id = host.read16(hdr.wrapping_add(2)) as u64;
                    if act == 0 {
                        host.call(0x197838, &[t, id]);
                    } else if act == 1 {
                        host.call(0x197870, &[t, id]);
                    }
                    i += 1;
                    if (host.read16(hdr.wrapping_add(0xc)) as i32) <= i {
                        break;
                    }
                }
            }
        }

        /// Marks the group disabled and dismisses its objects (virtual method 1 of each, following the link at +0x60).
        ///
        /// orig 0x1985f8
        /// orig 0x198660
        pub fn dismiss_all(&mut self, host: &mut impl AreaHost) {
            self.set_w(4, self.w(4) | 1);
            let mut p = self.w(0xc);
            while p != 0 {
                let next = host.read32(p.wrapping_add(0x60));
                host.vcall(p, 1, &[]);
                p = next;
            }
            self.set_w(0xc, 0);
            self.set_w(0x10, 0);
        }

        /// Whether the group has an entity of spawn kind 2 whose byte at +0x1c is `id`.
        ///
        /// orig 0x199698
        pub fn has_kind2_entity(&self, host: &mut impl AreaHost, id: u64) -> bool {
            let hdr = self.w(8);
            let n = host.read16(hdr.wrapping_add(4)) as i16 as i32;
            let mut i = 0i32;
            if n != 0 {
                let mut off = 0u32;
                loop {
                    i += 1;
                    let e = hdr.wrapping_add(off);
                    if host.read8(e.wrapping_add(0x48)) == 2 && host.read8(e.wrapping_add(0x5c)) as u64 == id {
                        return true;
                    }
                    off = (i as u32).wrapping_mul(0x40);
                    if i >= host.read16(hdr.wrapping_add(4)) as i32 {
                        break;
                    }
                }
            }
            false
        }
    }

    /// The group record table and the use counter.
    #[derive(Clone, PartialEq, Eq, Debug)]
    pub struct GroupTable {
        pub recs: Vec<GroupRec>,
    }

    impl Default for GroupTable {
        fn default() -> Self {
            GroupTable { recs: vec![GroupRec::default(); GROUP_COUNT] }
        }
    }

    impl GroupTable {
        /// Takes the next record (its address); the counter advances.
        ///
        /// orig 0x198300
        pub fn take(ctl: &mut AreaCtl) -> u32 {
            let a = (ctl.group_slots as i32).wrapping_mul(0x38) as u32;
            ctl.group_slots = ctl.group_slots.wrapping_add(1);
            a.wrapping_add(GROUP_BASE)
        }

        /// Gives the last record back.
        ///
        /// orig 0x198328
        pub fn give_back(ctl: &mut AreaCtl) {
            ctl.group_slots = ctl.group_slots.wrapping_sub(1);
        }

        /// Called with (1, 0xffff) it clears every record's flags and list words; called with (0, 0xffff) it dismisses every record's
        /// objects, last record first.
        ///
        /// orig 0x19a308
        pub fn reset(&mut self, host: &mut impl AreaHost, a: i32, b: i64) {
            if b == 0xffff {
                if a == 1 {
                    for r in self.recs.iter_mut() {
                        r.set_w(4, 0);
                        r.set_w(0x10, 0);
                        r.set_w(0xc, 0);
                    }
                }
                if a == 0 {
                    for r in self.recs.iter_mut().rev() {
                        r.dismiss_all(host);
                    }
                }
            }
        }

        /// orig 0x19a3b8
        pub fn clear_all(&mut self, host: &mut impl AreaHost) {
            self.reset(host, 1, 0xffff);
        }

        /// orig 0x19a3d8
        pub fn dismiss_every_group(&mut self, host: &mut impl AreaHost) {
            self.reset(host, 0, 0xffff);
        }
    }

    /// A unit appears at its record's point: the x, y, z of the unit record and a 0 are handed to the placement routine.
    ///
    /// orig 0x1982c8
    pub fn unit_appear(host: &mut impl AreaHost, unit: u32, arg: u64) -> u64 {
        let x = host.read32(unit.wrapping_add(4));
        let y = host.read32(unit.wrapping_add(8));
        let z = host.read32(unit.wrapping_add(0xc));
        host.call(0x198220, &[unit as u64, arg, x as u64, y as u64, z as u64, 0])
    }

    /// The world name pointer at index `world` of the prefix table at 0x349dc0 (the caller reads the table from the executable).
    ///
    /// orig 0x105d48
    pub fn world_name_address(table: &[u32], world: i32) -> u32 {
        table_word(table, world)
    }

    /// The pair of words cleared (a two-word record).
    ///
    /// orig 0x19dd78
    pub fn clear_pair(host: &mut impl AreaHost, p: u32) {
        host.write32(p + 4, 0);
        host.write32(p, 0);
    }

    /// Whether the group's entity record `unit` (0x40 bytes: object id, the spawn kind at +0x1c, the entrance at +0x1d) is wanted in
    /// the current entrance: the object exists, passes the character-data limit when that check is on, and a kind-1 entity only
    /// appears for its own entrance.
    ///
    /// orig 0x1981a0
    pub fn entity_wanted(host: &mut impl AreaHost, ctl: &AreaCtl, unit: u32) -> bool {
        let id = host.read32(unit);
        let chara = host.call(0x16e208, &[id as u64]);
        if chara == 0 {
            return false;
        }
        if host.read8(0x35e3a0) != 0 {
            let c4 = host.read8((chara as u32).wrapping_add(4));
            let r = host.call(0x16ece0, &[c4 as u64]);
            if 1999 < r as i64 {
                return false;
            }
        }
        let kind = host.read8(unit.wrapping_add(0x1c));
        if kind == 1 { host.read8(unit.wrapping_add(0x1d)) == ctl.cur[2] } else { true }
    }

    /// Whether the unit's spawn record says it is pinned (its byte +0x30 is above 99).
    ///
    /// orig 0x199378
    pub fn unit_pinned(host: &mut impl AreaHost, obj: u32) -> bool {
        let rec = host.read32(obj.wrapping_add(0x81c));
        rec != 0 && host.read8(rec.wrapping_add(0x30)) > 99
    }

    /// A story-side check the group asks with the serial table at 0x35d278 and a lookup of an object by serial.
    ///
    /// orig 0x1995c8
    pub fn serial_free(host: &mut impl AreaHost, serial: u64) -> bool {
        host.call(0x1942e0, &[0x35d278]) != 0 && host.call(0x16b0d8, &[serial]) == 0
    }

    impl GroupTable {
        fn addr(idx: usize) -> u32 {
            GROUP_BASE + (idx * GROUP_SIZE) as u32
        }

        /// Asks the object model to load each wanted entity of the group (`FUN_0016e638` with the object id, -1 and 0x1e).
        ///
        /// orig 0x1986c8
        pub fn preload(&mut self, host: &mut impl AreaHost, ctl: &AreaCtl, idx: usize) {
            let n = host.read16(self.recs[idx].w(8).wrapping_add(4)) as i16;
            if n == 0 {
                return;
            }
            let mut i = 0i32;
            let mut off = 0u32;
            loop {
                i += 1;
                let hdr = self.recs[idx].w(8);
                let unit = hdr.wrapping_add(off).wrapping_add(0x2c);
                if entity_wanted(host, ctl, unit) {
                    let id = host.read32(unit);
                    host.call(0x16e638, &[id as u64, u64::MAX, 0x1e]);
                }
                let hdr = self.recs[idx].w(8);
                off = (i as u32).wrapping_mul(0x40);
                if i >= host.read16(hdr.wrapping_add(4)) as i32 {
                    break;
                }
            }
        }

        /// Makes one entity of an enabled group appear if it is wanted; a character of type 4 or 0x15 only when the group's check
        /// on its serial (`FUN_001994c8`) passes.
        ///
        /// orig 0x198920
        pub fn appear_entity(&mut self, host: &mut impl AreaHost, ctl: &AreaCtl, idx: usize, unit: u32) {
            if self.recs[idx].w(4) & 1 != 0 {
                return;
            }
            let rec = Self::addr(idx);
            let id = host.read32(unit);
            let chara = host.call(0x16e208, &[id as u64]) as u32;
            if entity_wanted(host, ctl, unit) {
                let t = host.read8(chara.wrapping_add(4));
                if t == 4 || t == 0x15 {
                    let serial = host.read16(unit.wrapping_add(0x1e));
                    if host.call(0x1994c8, &[rec as u64, serial as u64]) != 0 {
                        unit_appear(host, unit, rec as u64);
                    }
                } else {
                    unit_appear(host, unit, rec as u64);
                }
            }
        }

        /// Fires the group's event once (`FUN_00197b70` with the word at header +0x1c and the group id) and disables the group.
        ///
        /// orig 0x199930
        pub fn fire_once(&mut self, host: &mut impl AreaHost, idx: usize) {
            let hdr = self.recs[idx].w(8);
            let ev = host.read32(hdr.wrapping_add(0x1c));
            let id = host.read16(hdr.wrapping_add(2));
            host.call(0x197b70, &[ev as u64, id as u64]);
            let f = self.recs[idx].w(4);
            self.recs[idx].set_w(4, f | 1);
        }

        /// Whether a unit is pinned: a pinned unit's spawn record is never dropped when its group empties.
        ///
        /// orig 0x199398
        pub fn empty_check(&mut self, host: &mut impl AreaHost, idx: usize, obj: u32) {
            let rec = Self::addr(idx);
            if self.recs[idx].w(4) & 1 == 0 && host.read32(obj.wrapping_add(0x588)) & 0x800 != 0 && !unit_pinned(host, obj) {
                host.call(0x199518, &[rec as u64]);
            }
        }

        /// A unit leaves the group: its serial is noted, and a unit that appeared by way 0, 1 or 3 and is a class-0x800 object
        /// empties its place in the group. Ends by loading the group's repeat delay as a float.
        ///
        /// orig 0x1993f8
        pub fn unit_left(&mut self, host: &mut impl AreaHost, idx: usize, obj: u32) {
            let rec = Self::addr(idx);
            let state = host.read32(obj.wrapping_add(0x7f8));
            if state & 4 == 0 {
                let serial = host.call(0x16b928, &[obj as u64]);
                host.call(0x1942c8, &[0x35d278, serial]);
                let way = host.call(0x16b858, &[obj as u64]) as u32;
                if (way as i32) >= 0 {
                    // Ways 0, 1 and 3 empty the unit's place; 2 and the rest do not.
                    let empties = (way as i32) < 2 || way == 3;
                    if empties && host.read32(obj.wrapping_add(0x588)) & 0x800 != 0 {
                        host.call(0x199518, &[rec as u64]);
                    }
                }
            } else if unit_pinned(host, obj) {
                let serial = host.call(0x16b928, &[obj as u64]);
                host.call(0x1942c8, &[0x35d278, serial]);
            }
            let hdr = self.recs[idx].w(8);
            let v = host.read16(hdr.wrapping_add(0x20)) as f32;
            self.recs[idx].set_w(0x14, v.to_bits());
        }

        /// The sum of the field at +0x18 over the records in use.
        ///
        /// orig 0x199580
        pub fn used_total(&self, ctl: &AreaCtl) -> i32 {
            let mut t = 0i32;
            let mut n = ctl.group_slots as i32;
            let mut i = 0;
            if 0 < n {
                while n != 0 {
                    n -= 1;
                    t = t.wrapping_add(self.recs[i].w(0x18) as i32);
                    i += 1;
                }
            }
            t
        }

        /// The cost the group's entities add up to: each wanted-kind entity not yet alive adds its character data's byte +0x54.
        ///
        /// orig 0x1996f0
        pub fn entity_cost(&mut self, host: &mut impl AreaHost, idx: usize) -> i32 {
            let mut i = 0i32;
            let mut sum = 0i32;
            let hdr = self.recs[idx].w(8);
            let n = host.read16(hdr.wrapping_add(4)) as i16;
            if n == 0 {
                return 0;
            }
            loop {
                let hdr = self.recs[idx].w(8);
                let off = (i as u32).wrapping_mul(0x40);
                let b = host.read8(off.wrapping_add(hdr).wrapping_add(0x48));
                let mut add = false;
                if b == 2 {
                    if host.read8(off.wrapping_add(hdr).wrapping_add(0x5c)) <= self.recs[idx].0[0x20] {
                        add = true;
                    }
                } else if b < 3 {
                    if b == 0 {
                        add = true;
                    }
                } else if b == 3 {
                    add = true;
                }
                if add {
                    let serial = host.read16(off.wrapping_add(hdr).wrapping_add(0x4a));
                    if !serial_free(host, serial as u64) {
                        let id = host.read32(off.wrapping_add(self.recs[idx].w(8)).wrapping_add(0x2c));
                        let c = host.call(0x16e208, &[id as u64]) as u32;
                        sum = sum.wrapping_add(host.read8(c.wrapping_add(0x54)) as i32);
                    }
                }
                i += 1;
                let hdr = self.recs[idx].w(8);
                if i >= host.read16(hdr.wrapping_add(4)) as i32 {
                    break;
                }
            }
            sum
        }

        /// The address of the record whose header has the group id `id`, among the records in use, or 0.
        ///
        /// orig 0x199bf8
        pub fn find(&self, host: &mut impl AreaHost, ctl: &AreaCtl, id: u64) -> u32 {
            let n = ctl.group_slots as i32;
            if n < 1 {
                return 0;
            }
            let mut i = 0usize;
            let mut hdr = self.recs[0].w(8);
            while host.read16(hdr.wrapping_add(2)) as u64 != id {
                i += 1;
                if n <= i as i32 {
                    return 0;
                }
                hdr = self.recs.get(i).map_or(0, |r| r.w(8));
            }
            Self::addr(i)
        }

        fn or_flag(&mut self, host: &mut impl AreaHost, ctl: &AreaCtl, id: u64, set: bool) {
            let a = self.find(host, ctl, id);
            if a == 0 {
                // The original writes through address 0 + 4 when no record has the id.
                let v = host.read32(4);
                host.write32(4, if set { v | 1 } else { v & !1 });
            } else {
                let idx = ((a - GROUP_BASE) / GROUP_SIZE as u32) as usize;
                let f = self.recs[idx].w(4);
                self.recs[idx].set_w(4, if set { f | 1 } else { f & !1 });
            }
        }

        /// Disables the group with the id.
        ///
        /// orig 0x199c48
        pub fn disable(&mut self, host: &mut impl AreaHost, ctl: &AreaCtl, id: u64) {
            self.or_flag(host, ctl, id, true);
        }

        /// Enables the group with the id.
        ///
        /// orig 0x199c70
        pub fn enable(&mut self, host: &mut impl AreaHost, ctl: &AreaCtl, id: u64) {
            self.or_flag(host, ctl, id, false);
        }

        /// The routine parameter of a kind-6 group (the word at +0x30), else 0.
        ///
        /// orig 0x199ca0
        pub fn routine_param(&self, host: &mut impl AreaHost, idx: usize) -> u32 {
            let hdr = self.recs[idx].w(8);
            if host.read8(hdr) as i8 == 6 { self.recs[idx].w(0x30) } else { 0 }
        }

        /// The signal block with the id `id` in the group (its first halfword; a negative id gives the first block), or 0.
        ///
        /// orig 0x199cc0
        pub fn find_signal_block(&self, host: &mut impl AreaHost, idx: usize, id: i64) -> u32 {
            let hdr = self.recs[idx].w(8);
            let count = host.read16(hdr.wrapping_add(8)) as u32;
            let mut p = self.recs[idx].w(0x2c);
            if count != 0 {
                if id < 0 {
                    return p;
                }
                let mut i = 0u32;
                loop {
                    if host.read16(p) as i64 == id {
                        return p;
                    }
                    i += 1;
                    let c = host.read16(p.wrapping_add(2)) as u32;
                    p = p.wrapping_add(c.wrapping_mul(12)).wrapping_add(8);
                    if i >= count {
                        break;
                    }
                }
            }
            0
        }

        /// The block of section 3 whose byte +5 is `id`, or 0.
        ///
        /// orig 0x199d18
        pub fn find_section_block(&self, host: &mut impl AreaHost, idx: usize, id: u64) -> u32 {
            let hdr = self.recs[idx].w(8);
            let count = host.read16(hdr.wrapping_add(8)) as u32;
            let mut p = self.recs[idx].w(0x2c);
            let mut i = 0u32;
            if count != 0 {
                loop {
                    if host.read8(p.wrapping_add(5)) as u64 == id {
                        return p;
                    }
                    i += 1;
                    let c = host.read16(p.wrapping_add(2)) as u32;
                    p = p.wrapping_add(c.wrapping_mul(12)).wrapping_add(8);
                    if i >= count {
                        break;
                    }
                }
            }
            0
        }

        /// The first created object of a kind-6 group whose spawn record has kind 0 and whose method +8 accepts `arg`, or 0.
        ///
        /// orig 0x199d68
        pub fn first_accepting(&self, host: &mut impl AreaHost, idx: usize, arg: u64) -> u32 {
            let hdr = self.recs[idx].w(8);
            if host.read8(hdr) as i8 != 6 {
                return 0;
            }
            let mut prev = 0u32;
            loop {
                let p = if prev == 0 { self.recs[idx].w(0xc) } else { host.read32(prev.wrapping_add(0x60)) };
                if p == 0 {
                    return 0;
                }
                prev = p;
                let block = host.read32(p.wrapping_add(0x6c));
                if host.read16(block.wrapping_add(0x2c)) as i16 != 0 {
                    continue;
                }
                if host.vcall(p, 2, &[arg]) == 0 {
                    continue;
                }
                return p;
            }
        }

        /// At a group-pass start: each record in use gets its +0x1c word into +0x18, then the story side resets its serial table.
        ///
        /// orig 0x199e10
        pub fn roll_over(&mut self, host: &mut impl AreaHost, ctl: &AreaCtl) {
            let mut n = ctl.group_slots as i32;
            let mut i = 0;
            if 0 < n {
                while n != 0 {
                    n -= 1;
                    let v = self.recs[i].w(0x1c);
                    self.recs[i].set_w(0x18, v);
                    i += 1;
                }
            }
            host.call(0x194300, &[0x35d278]);
        }

        /// A kind-6 group's entities of spawn kind 2 appear on demand when their serial is free; a new one that is not in
        /// state 0x10 gets a move order with the two constants at 0x37b308 and 0x37b30c.
        ///
        /// orig 0x199e98
        pub fn appear_on_demand(&mut self, host: &mut impl AreaHost, idx: usize) {
            let rec = Self::addr(idx);
            let mut i = 0u32;
            let hdr = self.recs[idx].w(8);
            if host.read16(hdr.wrapping_add(4)) as i16 == 0 {
                return;
            }
            loop {
                let hdr = self.recs[idx].w(8);
                let e = i.wrapping_mul(0x40).wrapping_add(hdr);
                let n = if host.read8(e.wrapping_add(0x48)) == 2 {
                    let serial = host.read16(e.wrapping_add(0x4a));
                    if host.call(0x16b0d8, &[serial as u64]) == 0 {
                        let unit = self.recs[idx].w(8).wrapping_add(i.wrapping_mul(0x40)).wrapping_add(0x2c);
                        let o = unit_appear(host, unit, rec as u64) as u32;
                        if host.read32(o.wrapping_add(0x7f8)) & 0x10 == 0 {
                            let a = host.read32(0x37b308);
                            let b = host.read32(0x37b30c);
                            // 0.0 and the two constants travel in float registers 12 to 14.
                            host.call(0x1abf40, &[o.wrapping_add(0x82c) as u64, 0, a as u64, b as u64]);
                        }
                    }
                    host.read16(self.recs[idx].w(8).wrapping_add(4))
                } else {
                    host.read16(hdr.wrapping_add(4))
                };
                i += 1;
                if i as i32 >= n as i32 {
                    break;
                }
            }
        }

        /// A map-part switch group shows and hides map parts by the two bytes of the activator at +0x2e and +0x2f, when the map
        /// asks (`FUN_0015f1e0`) the part is in the other state.
        ///
        /// orig 0x199f70
        pub fn switch_parts(host: &mut impl AreaHost, obj: u32) {
            let mut block = host.read32(obj.wrapping_add(0x6c));
            let show = host.read8(block.wrapping_add(0x2e));
            if show != 0 {
                let l = host.call(0x15f1e0, &[(show as u64).wrapping_sub(1)]);
                if l == 0 {
                    let blk = host.read32(obj.wrapping_add(0x6c));
                    let b = host.read8(blk.wrapping_add(0x2e));
                    host.call(0x15e9a8, &[(b as u64).wrapping_sub(1)]);
                }
                block = host.read32(obj.wrapping_add(0x6c));
            }
            let hide = host.read8(block.wrapping_add(0x2f));
            if hide != 0 {
                let l = host.call(0x15f1e0, &[(hide as u64).wrapping_sub(1)]);
                if l != 0 {
                    let blk = host.read32(obj.wrapping_add(0x6c));
                    let b = host.read8(blk.wrapping_add(0x2f));
                    host.call(0x15e928, &[(b as u64).wrapping_sub(1)]);
                }
            }
        }

        /// What a touched exit volume does: the exit group's destination is built from the header (area 100 is world 0x12 area 1,
        /// 101 is world 2 area 0x28, else the current world and the header's area), then by the volume's option the walk-in jump
        /// (option 0) or the prompt of a larger volume (options 1 and 2) is handled. Always answers 0.
        ///
        /// orig 0x198e10
        pub fn exit_check(&mut self, host: &mut impl AreaHost, ctl: &mut AreaCtl, save: &mut [u8], idx: usize, obj: u32) -> u64 {
            let rec = Self::addr(idx);
            let mut d: Dest6 = [0xff; 6];
            let mode = host.call(0x1903c8, &[]);
            let hdr = self.recs[idx].w(8);
            if mode == 2 && host.read8(hdr.wrapping_add(0x1e)) != 0 {
                return 0;
            }
            let kind = host.read8(hdr.wrapping_add(0x1c));
            d[0] = 0x12;
            if kind == 0x64 {
                d[1] = 1;
            } else if kind == 0x65 {
                d[0] = 2;
                d[1] = 0x28;
            } else {
                d[0] = ctl.cur[0];
                d[1] = host.read8(hdr.wrapping_add(0x1c));
            }
            d[2] = host.read8(hdr.wrapping_add(0x1d));
            let lock = host.call(0x195358, &[d[0] as u64, d[1] as u64]);
            let activator = host.read32(obj.wrapping_add(0x6c));
            let opt = host.read16(activator.wrapping_add(2));
            let mut wide = 0u64;
            if opt != 1 {
                if opt < 2 {
                    if opt != 0 {
                        return 0;
                    }
                    if self.recs[idx].w(4) & 1 != 0 {
                        return 0;
                    }
                    if lock != 0 {
                        return 0;
                    }
                    let gate = host.read8(self.recs[idx].w(8).wrapping_add(0x1e));
                    if host.call(0x1d88a0, &[obj as u64, gate as u64]) == 0 {
                        return 0;
                    }
                    let id = host.read16(self.recs[idx].w(8).wrapping_add(2));
                    let mut flags = self.recs[idx].w(4);
                    ctl.exit_walk_in(host, &mut flags, id, &d);
                    self.recs[idx].set_w(4, flags | 1);
                    return 0;
                }
                if opt != 2 {
                    return 0;
                }
                wide = 1;
            }
            if self.recs[idx].w(4) & 0x10 != 0 {
                return 0;
            }
            let act = host.read32(obj.wrapping_add(0x6c));
            let ok = host.call(0x199058, &[rec as u64, d[0] as u64, d[1] as u64, d[2] as u64, act as u64, (lock == 0) as u64, wide]);
            if ok == 0 {
                return 0;
            }
            let f = self.recs[idx].w(4);
            self.recs[idx].set_w(4, f | 0x10);
            let _ = save;
            0
        }

        /// What happens to a group when `obj` touches one of its volumes, by the group's kind.
        ///
        /// orig 0x198b30
        pub fn dispatch(&mut self, host: &mut impl AreaHost, ctl: &mut AreaCtl, save: &mut [u8], idx: usize, obj: u64) -> bool {
            let rec = Self::addr(idx);
            let hdr = self.recs[idx].w(8);
            match host.read8(hdr) {
                1 => obj != 0 && {
                    self.exit_check(host, ctl, save, idx, obj as u32);
                    false
                },
                2 => {
                    host.call(0x1991a0, &[rec as u64, obj]);
                    true
                }
                3 => {
                    self.fire_once(host, idx);
                    true
                }
                6 => {
                    self.appear_on_demand(host, idx);
                    true
                }
                7 => {
                    if obj == 0 {
                        true
                    } else {
                        host.call(0x199e68, &[rec as u64, obj]);
                        false
                    }
                }
                8 => {
                    if obj != 0 {
                        Self::switch_parts(host, obj as u32);
                    }
                    true
                }
                9 => {
                    host.call(0x19a000, &[rec as u64, obj]);
                    true
                }
                10 => {
                    let hdr = self.recs[idx].w(8);
                    let id = host.read16(hdr.wrapping_add(2));
                    let player = host.read32(0x35c998);
                    host.write32(player.wrapping_add(0xb4c), id as u32);
                    true
                }
                _ => true,
            }
        }

        /// The per-frame pass over one group: clears the transient flag bits, counts the repeat delay down, and offers `who` to
        /// each created object's method +8; the first that accepts is handled by [`GroupTable::dispatch`], which decides whether
        /// the walk goes on. A kind-4 group is handled by `FUN_00199b18`.
        ///
        /// orig 0x1989d8
        pub fn frame_pass(&mut self, host: &mut impl AreaHost, ctl: &mut AreaCtl, save: &mut [u8], idx: usize, who: u64) {
            let rec = Self::addr(idx);
            let f = self.recs[idx].w(4);
            self.recs[idx].set_w(4, f & 0xffff_ffe7);
            if f & 1 != 0 {
                return;
            }
            let hdr = self.recs[idx].w(8);
            if host.read8(hdr) == 4 {
                host.call(0x199b18, &[rec as u64]);
                return;
            }
            if host.call(0x190668, &[]) != 0 {
                return;
            }
            let t = f32::from_bits(self.recs[idx].w(0x14));
            if 0.0 < t {
                let step = f32::from_bits(host.read32(0x356f40));
                let t = f32::from_bits(self.recs[idx].w(0x14));
                self.recs[idx].set_w(0x14, (t - step).to_bits());
            }
            let mut prev = 0u32;
            loop {
                let p = if prev == 0 { self.recs[idx].w(0xc) } else { host.read32(prev.wrapping_add(0x60)) };
                if p == 0 {
                    return;
                }
                prev = p;
                let hdr = self.recs[idx].w(8);
                if host.read8(hdr) == 6 {
                    let block = host.read32(p.wrapping_add(0x6c));
                    if host.read16(block.wrapping_add(0x2c)) as i16 != 1 {
                        continue;
                    }
                }
                if host.vcall(p, 2, &[who]) == 0 {
                    continue;
                }
                let f = self.recs[idx].w(4);
                self.recs[idx].set_w(4, f | 8);
                if self.dispatch(host, ctl, save, idx, p as u64) {
                    return;
                }
            }
        }
    }

    /// The area info table: a header with the world count at +4, a row per world at +8 + 4 * world (area count at +0 and the
    /// offset of its entries at +2), 0x40-byte entries. Returns the entry address for (area, world) (negative: the current
    /// one), or 0.
    ///
    /// orig 0x1933a0
    pub fn areainfo_entry(host: &mut impl AreaHost, ctl: &AreaCtl, area: i64, world: i32) -> u32 {
        let area = if area < 0 { ctl.cur[1] as i64 } else { area };
        let world = if world < 0 { ctl.cur[0] as u32 } else { world as u32 };
        let t = ctl.areainfo;
        if (world as i32) < host.read32(t.wrapping_add(4)) as i32 && area >= 0 {
            let row = world.wrapping_mul(4).wrapping_add(t);
            if area < host.read16(row.wrapping_add(8)) as i64 {
                return t.wrapping_add(host.read16(row.wrapping_add(10)) as u32).wrapping_add((area as u32).wrapping_mul(0x40));
            }
        }
        0
    }

    /// Tests bit `bit` of the first word of an area's entry.
    ///
    /// orig 0x193410
    pub fn areainfo_bit(host: &mut impl AreaHost, ctl: &AreaCtl, area: i64, world: i32, bit: u32) -> bool {
        let e = areainfo_entry(host, ctl, area, world);
        e != 0 && host.read32(e) & (1u32 << (bit & 31)) != 0
    }

    /// orig 0x193458
    pub fn areainfo_bit0(host: &mut impl AreaHost, ctl: &AreaCtl, area: i64, world: i32) -> bool {
        areainfo_bit(host, ctl, area, world, 0)
    }

    /// orig 0x193478
    pub fn areainfo_bit1(host: &mut impl AreaHost, ctl: &AreaCtl) -> bool {
        areainfo_bit(host, ctl, -1, -1, 1)
    }

    /// orig 0x1934a0
    pub fn areainfo_bit2(host: &mut impl AreaHost, ctl: &AreaCtl, area: i64, world: i32) -> bool {
        areainfo_bit(host, ctl, area, world, 2)
    }

    /// Whether the current area casts no shadows.
    ///
    /// orig 0x1934c0
    pub fn areainfo_no_shadow(host: &mut impl AreaHost, ctl: &AreaCtl) -> bool {
        areainfo_bit(host, ctl, -1, -1, 3)
    }

    /// orig 0x1934e8
    pub fn areainfo_bit4(host: &mut impl AreaHost, ctl: &AreaCtl) -> bool {
        areainfo_bit(host, ctl, -1, -1, 4)
    }

    /// The byte at +0x34 of the current area's entry.
    ///
    /// orig 0x1935e8
    pub fn areainfo_byte34(host: &mut impl AreaHost, ctl: &AreaCtl) -> u8 {
        let e = areainfo_entry(host, ctl, -1, -1);
        host.read8(e.wrapping_add(0x34))
    }

    /// Hands the current area's record bits to the story side (`FUN_001a7700`).
    ///
    /// orig 0x193510
    pub fn areainfo_apply(host: &mut impl AreaHost, ctl: &AreaCtl, save: &[u8]) {
        let e = areainfo_entry(host, ctl, ctl.cur[1] as i64, ctl.cur[0] as i32);
        let b = save.get(0x1550 + ctl.cur[0] as usize + 0x5c).copied().unwrap_or(0);
        host.call(0x1a7700, &[e.wrapping_add((b as u32).wrapping_mul(4)).wrapping_add(0x10) as u64]);
    }

    /// Sets the "has ensemble" byte from the current area's entry (its halfword +0x32 names an ensemble entry that must exist).
    ///
    /// orig 0x193560
    pub fn areainfo_scan(host: &mut impl AreaHost, ctl: &mut AreaCtl) {
        let mut v = 0u8;
        let e = areainfo_entry(host, ctl, -1, -1);
        let h = host.read16(e.wrapping_add(0x32));
        if h != 0 && 0 < host.call(0x1a1cc8, &[h as u64]) as i64 {
            v = 1;
        }
        ctl.areainfo_has = v;
    }

    /// The area info init: clears the hold byte, scans and applies.
    ///
    /// orig 0x193380
    pub fn areainfo_init(host: &mut impl AreaHost, ctl: &mut AreaCtl, save: &[u8]) {
        ctl.areainfo_hold = 0;
        areainfo_scan(host, ctl);
        areainfo_apply(host, ctl, save);
    }

    /// Whether the area has an ensemble and nobody has held it.
    ///
    /// orig 0x1935b0
    pub fn areainfo_ready(ctl: &AreaCtl) -> bool {
        ctl.areainfo_has != 0 && ctl.areainfo_hold == 0
    }

    /// orig 0x1935d8
    pub fn areainfo_hold(ctl: &mut AreaCtl) {
        ctl.areainfo_hold = 1;
    }

    /// The word the area info file was loaded to.
    ///
    /// orig 0x193ef8
    pub fn areainfo_file(ctl: &AreaCtl) -> u32 {
        ctl.areainfo_file
    }

    /// Resets the type-4 memory to none.
    ///
    /// orig 0x19d8c0
    pub fn clear_type4_memory(ctl: &mut AreaCtl) {
        ctl.type4_memory = 0xffff_ffff;
    }
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
