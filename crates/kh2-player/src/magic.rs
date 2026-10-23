//! Sora's magic: the cast motion, the MP cost and the MP charge, and the spell objects of the six
//! spells at levels 1 to 3, in and out of drive forms (phase 1 WP-6B / 7A, phase 2 WP-7I).
//!
//! The game keeps one magic resource per slot (FUN_001a45c0): the `magc` row the slot uses
//! (`magic_rules::pick_row`), the row's BAR (`magic/<SPELL>_<level>.mag`) with two BDX scripts and a
//! PAX effect file. A cast (FUN_00179400 -> FUN_001a4798) plays the row's motion, creates a script
//! thread on the spell's program and starts its event 20 with (caster, effect file, kind, `magi` row)
//! as the thread's code (FUN_0016c500). Frame trigger 17 of the cast motion (FUN_00188498 case 0x11 ->
//! FUN_001a4a60) pays the MP (FUN_001769e0) and restarts the thread at event 21, which makes the
//! spell. A cast whose motion is left before the trigger ends with event 20 (it waits for the
//! motion to change) and costs nothing.
//!
//! What the four spells' scripts do is ported by hand below, step by step, from their disassembly
//! (`kh2 bdx magic/FIRE_1.mag`), including the request threads (host 2.20) of Blizzard's shot and
//! Thunder's strike, with their drive-form branches (`fire_plan`, `blizzard_plan`, `thunder_plan`).
//! Magnet and Reflect always run their own scripts in the BDX interpreter (`VmCast`, `SpellHost`);
//! `KH2_MAGIC_VM=1` runs the other four there too: Fire's matches the hand port; the other three stop
//! at host calls the app does not answer yet (docs/systems/magic/magic-turn.md "Running the scripts in the interpreter").
//!
//! Keys: the command menu (`items.rs`, WP-7C: the original's lists and input) answers T (cursor
//! up), G (cursor down), B (back); J / left mouse executes the selected command (the game's X
//! button): Attack attacks, Magic opens the list, a spell is cast (`MagicState::request_cast`).
//! `KH2_KEYS` drives T, G and B too (`harness_keys` here; the harness maps J). `KH2_MAGIC=f,b,t,c[,m,r]`
//! sets the spell levels owned (default: the counts in `progress::Progress`, whose app baseline is
//! 1,1,1,1,0,0). `KH2_MAGIC_LOG=1`
//! logs the menu, the casts, MP, hits and damage; `=2` also every live attack per tick.

use std::collections::HashMap;
use std::sync::Arc;

use bevy::prelude::*;
use kh2_anim::moveset::{Chain, Windows};
use kh2_anim::triggers::{frame, range};
use kh2_anim::PlayerState;
use kh2_model::collision::kind as shape_kind;
use kh2_tables::tables::{self as t, Cmd, Magc};

use crate::assets::Disc;
use crate::combat::{self, Fighter, Hit, HurtSpheroid, Team, Volume};
use crate::data::GameData;
use crate::shared::hud::MagicMarker;
use crate::fx::{self, Bind, LoadedPax, Placement, RunningEffect};
use crate::magic_rules::{self as rules, Charge, HitMemory, MagiRow, Paid, Status};
use crate::schedule::Sim;
use crate::skel::Animator;
use crate::sora::{Battle, Sora, State};
use crate::SCALE;

use crate::shared::magic::{log_level, log_on};
pub use crate::shared::magic::{MagicState, SpellAttacks, ATKP_FIRE};

/// `atkp` ids of Cure's heal on its target and on another member (CURE_*.mag event 21).
pub const ATKP_CURE_SELF: u16 = 708;
pub const ATKP_CURE_OTHER: u16 = 709;
/// `magi` offsets the scripts read: attack radius and height, attack time (Fire), heal radius (Cure).
const MAGI_RADIUS: usize = 0x0;
const MAGI_HEIGHT: usize = 0x4;
const MAGI_TIME: usize = 0x8;
const MAGI_CURE_RANGE: usize = 0x38;
/// `atkp` id of Blizzard's shot (BLIZZARD_*.mag R035b: `attack_create(ctx, 248, -1, kind)`).
pub const ATKP_BLIZZARD: u16 = 248;
/// `magi` offsets Blizzard's shot reads: turn rate (+0xc, times the shot's scale), life (+0x10),
/// speed (+0x14, units per frame), size (+0x18), and the caster's push (+0x1c, host 1.146).
const MAGI_TURN: usize = 0xc;
const MAGI_LIFE: usize = 0x10;
const MAGI_SPEED: usize = 0x14;
const MAGI_SHOT_SIZE: usize = 0x18;
const MAGI_PUSH: usize = 0x1c;
/// The fade a shot ends with (host 0.52 with 12 frames).
const SHOT_FADE: u32 = 12;
/// Thunder's attack rows (THUNDER_*.mag R031c): on the ground, in the air, and the column copy.
pub const ATKP_THUNDER_GROUND: u16 = 250;
pub const ATKP_THUNDER_AIR: u16 = 251;
pub const ATKP_THUNDER_COLUMN: u16 = 249;
/// `magi` offsets of the strike: ground snap distance, radius, column height and radius, the wait
/// before the attacks and their time.
const MAGI_THUNDER_SNAP: usize = 0x20;
const MAGI_THUNDER_RADIUS: usize = 0x24;
const MAGI_THUNDER_COLUMN_H: usize = 0x28;
const MAGI_THUNDER_COLUMN_R: usize = 0x2c;
const MAGI_THUNDER_WAIT: usize = 0x30;
const MAGI_THUNDER_TIME: usize = 0x34;
/// Magnet's attack (MAGNET_*.mag R0017: `attack_create(ctx, 658, -1, kind)`); its pulling reaction is
/// the victim's (`EnemyReaction` 5, FUN_00185668 -> FUN_001bf920).
pub const ATKP_MAGNET: u16 = 658;
/// Reflect's guard (REFLECT_*.mag R0017: `attack_create(ctx, 573, -1, kind)`, a type-3 row).
pub const ATKP_REFLECT_GUARD: u16 = 573;
/// `atkp` Type of Reflect's guard row: an attack that clashes with enemy attacks and hits nothing.
const GUARD_TYPE: u8 = 3;
/// Sound of the menu? none: the game's menu sounds are not ported.
/// Size step of the swept attack test (FUN_001a2cc0: `min(radius, height) * DAT_0037b460`, 0.5).
const SWEEP_STEP: f32 = 0.5;
/// FUN_001a2fc8: a move longer than twice the frame step (`DAT_00356f40` = 1.0) sets the sweep flag.
const SWEEP_MIN: f32 = 2.0;
/// The shape type FUN_0016b7d0 asks FUN_0016b558 for: the victim's body centre for the pull.
const BODY_SHAPE: u8 = 1;

pub struct MagicPlugin;

impl Plugin for MagicPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<MagicState>()
            .init_resource::<MagicRun>()
            .init_resource::<rules::GlobalFlags>()
            .init_resource::<SpellAttacks>()
            .add_systems(Startup, setup.after(crate::sora::setup))
            .add_systems(Update, harness_keys.after(crate::sora::read_pad))
            .add_systems(FixedUpdate, run.in_set(Sim::Sora).after(crate::sora::tick));
    }
}

/// `KH2_MAGIC_VM=1`: run the spell scripts in the BDX interpreter (see `VmCast`).
fn vm_on() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| std::env::var("KH2_MAGIC_VM").is_ok_and(|v| v == "1"))
}

/// `KH2_MAGIC_NOREC=1` (harness aid): a cast starts as if Sora had no target record (nothing inside AutoLockOnRange), so
/// Magic Lock-On picks one and the lock-on controller goes to mode 1.
fn no_record() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| std::env::var("KH2_MAGIC_NOREC").is_ok_and(|v| v == "1"))
}

macro_rules! mlog {
    ($($t:tt)*) => {
        if log_on() {
            info!($($t)*);
        }
    };
}

/// What the game loads per slot (FUN_001a45c0) and the tables the rules read.
#[derive(Resource)]
pub struct MagicData {
    pub magc: Vec<Magc>,
    pub cmds: Vec<Cmd>,
    pub magi: Vec<MagiRow>,
    /// Spell levels owned per slot (the count of the slot's `magc` item).
    pub levels: [u8; rules::SLOTS as usize],
    pub world: u8,
    /// The row each slot uses, with its effect file.
    pub slots: Vec<Option<(Magc, Option<Arc<LoadedPax>>)>>,
    /// `prty` MpDrive of Sora's row (FUN_00186440 reads the player's `prty` row at +0x12c, +0x1c).
    pub mp_drive: f32,
    /// The slot's spell script (the first BDX of its `.mag`, the one FUN_001a45c0 keeps), for
    /// `KH2_MAGIC_VM=1`.
    pub scripts: Vec<Option<Arc<Vec<u8>>>>,
    /// Host function signatures read from the executable (for the scripts' unnamed calls).
    pub host_table: Option<kh2_bdx::exe::HostTable>,
    /// The numbers of host 1.248's scatter (Final Thunder), from the executable.
    pub scatter: Option<rules::ScatterConsts>,
}

impl MagicData {
    fn cmd(&self, id: u16) -> Option<&Cmd> {
        self.cmds.iter().find(|c| c.id == id)
    }

    fn slot_of_cmd(&self, cmd: u16) -> Option<u8> {
        self.slots.iter().position(|s| s.as_ref().is_some_and(|(r, _)| r.command == cmd)).map(|i| i as u8)
    }

    /// FUN_001ba2c0 for a magic command, with the `DisableForm` mask of command 2 (FUN_001b7290).
    /// The per-spell switch (FUN_001a4bb8: bit `cmd.Argument`, the command's magic slot, of the word
    /// at 0x340eac, `Progress::spell_switch`) decides whether a spell may be cast with too little MP;
    /// without `Progress` it reads as clear (a new game).
    pub fn status(&self, cmd: u16, mp: i32, charging: bool, form: u8, progress: Option<&crate::progress::Progress>) -> Status {
        let Some(c) = self.cmd(cmd) else { return Status::Unavailable };
        let mask = self.cmd(c.execute).map_or(c.disable_form, |e| e.disable_form);
        let switch = progress.is_some_and(|p| p.spell_switch(c.argument as u8));
        rules::availability(c.cost, mp, charging, switch, rules::form_disables(mask, form))
    }
}

/// FUN_001a0240 (the MP refill of the form start, FUN_001a04b0): a running MP charge ends first
/// (FUN_001a05d0 tests the total +0x1c0; FUN_001a05a8 clears both times and refills MP), then MP is
/// raised by its maximum (FUN_001a0210 -> FUN_001a0180, clamped), i.e. set to the maximum. Returns
/// whether a charge was ended. `charge` may be `None` when the magic system is not running.
pub fn refill_mp(charge: Option<&mut Charge>, mp: &mut i32, max_mp: i32) -> bool {
    let ended = match charge {
        Some(c) if c.total > 0.0 => {
            *c = Charge::default();
            true
        }
        _ => false,
    };
    *mp = max_mp;
    if ended {
        mlog!("magic: MP refill (FUN_001a0240): MP charge ended, MP {max_mp}");
    }
    ended
}

/// `KH2_MAGIC_HURT=<seconds>:<hp>`: once, that many seconds (60 Hz ticks) after the start, Sora's HP
/// is set to the value (a harness aid: the first levels start at full HP, so a Cure has nothing to
/// heal). Returns the HP when it is due.
fn hurt_due(tick: u64) -> Option<i32> {
    static SPEC: std::sync::OnceLock<Option<(f32, i32)>> = std::sync::OnceLock::new();
    let (at, hp) = (*SPEC.get_or_init(|| {
        let v = std::env::var("KH2_MAGIC_HURT").ok()?;
        let (a, b) = v.split_once(':')?;
        Some((a.parse().ok()?, b.parse().ok()?))
    }))?;
    (tick as f32 >= at * crate::data::LOGIC_HZ).then_some(hp)
}

/// The casting state of the spell system (`run`): the casts, their attacks, shots and strikes, the hit
/// memory and the interpreter's world. Split from `MagicState`, which keeps what other systems share.
#[derive(Resource, Default)]
struct MagicRun {
    casts: Vec<Cast>,
    attacks: Vec<SpellAttack>,
    memory: HashMap<Entity, HitMemory>,
    next_group: u32,
    shots: Vec<Shot>,
    strikes: Vec<Strike>,
    /// The cast's target records for the turn triggers of its motion (see `CastTurn`).
    turn: Option<CastTurn>,
    /// The (mode, shape byte) of the marker last handed to the HUD, for the log and the release.
    marker_log: Option<(u8, u8)>,
    /// Ticks run, and the tick of the last cast (for the `KH2_MAGIC_LOG=2` attack timeline).
    tick: u64,
    cast_tick: u64,
    /// `KH2_MAGIC_HURT` has been applied.
    hurt_done: bool,
    /// Effect handles and persistent blocks of the spell scripts in the interpreter.
    vm_world: VmWorld,
    /// Clashes already counted: (guard attack group, enemy, its attack body, its window start).
    clash_done: std::collections::HashSet<(u32, Entity, Entity, i16)>,
    /// Blizzard's global +0x2c: casts counted by its event 21 (Final finishers pick their elements by
    /// it). The program's globals live as long as the slot's resource [inferred], so it is kept here.
    bliz_counter: u32,
    /// Host 1.248's generator and bucket counter (Final Thunder's scatter).
    scatter: rules::ScatterState,
    /// Blizzard's push of the caster (host 1.146 -> FUN_001857b0), stepped each tick after Sora's own.
    push: Option<crate::reaction_rules::Push>,
    /// Horizontal distance the push has moved Sora so far (for the log).
    push_moved: f32,
}

/// What the turn triggers of a cast motion read (FUN_00188030 cases 22 and 28): the command's target
/// record (+0x9f4; `own` when it names the caster) and the lock-on controller's mode-1 target that
/// FUN_00179470 set (+0xb40: FUN_0019b3f0 with 1). Kept while the cast motion plays.
#[derive(Clone, Copy, Debug)]
struct CastTurn {
    motion: Option<usize>,
    target: Option<Entity>,
    own: bool,
    lock_mode1: Option<Entity>,
    /// The lock-on controller's mode-1 timer (+0x1c): `AutoLockOnTime` when the mode starts (FUN_0019bcb8),
    /// counted down each frame and refilled by trigger 28 (see `magic_rules::mode1_step`).
    lock_timer: f32,
    /// The controller is in mode 2 (FUN_0019b3f0 with 2, the lock button pressed in mode 1): the timer no longer
    /// runs and the record stays while it is targetable.
    lock_mode2: bool,
}

/// A Thunder strike: the request thread at word 0x2da of THUNDER_*.mag (block +0x20 the place, +0x48
/// how far above it the ground probe starts).
struct Strike {
    pax: Arc<LoadedPax>,
    kind: u32,
    magi: MagiRow,
    pos: Vec3,
    rise: f32,
    /// The ground or air attack row (250 or 251), chosen in the first slice.
    atkp: u16,
    started: bool,
    /// Frames left of the current wait (R029f).
    wait: f32,
    /// (ground attack, column attack) once made.
    attacks: Option<(usize, usize)>,
    done: bool,
}

/// A Blizzard projectile: the request thread at word 0x319 of BLIZZARD_*.mag (its block: +0x20
/// position, +0x50 direction, +0x60 turn rate, +0x64 flag; +0x40 the effect, +0x44 the attack).
struct Shot {
    pax: Arc<LoadedPax>,
    kind: u32,
    magi: MagiRow,
    pos: Vec3,
    /// The caster's lock-on point when the shot started (host 1.88): the attack's first place (R035b).
    origin: Vec3,
    dir: Vec3,
    /// The caster's target when the shot started (the block's copy of the target record, +0x30).
    target: Option<Entity>,
    /// The attack's index in `MagicState::attacks` (None before the first slice).
    attack: Option<usize>,
    /// The effect instance (element 2), once it exists.
    effect: Option<Entity>,
    /// Frames before the fade starts (`magi` +0x10 - 12), then the fade's frames left.
    life: f32,
    fade: Option<u32>,
    done: bool,
    /// The block's +0x60 is `magi` +0xc times this scale (R02d7).
    turn_scale: f32,
    /// The block's flag (+0x64, R02b9): a shot with 0 ends at its first target (host 2.14), one
    /// with 1 goes through targets and ends only at a wall or a clash (L039d).
    flag: u32,
}

/// One cast: the script instance of FUN_001a4798.
struct Cast {
    slot: u8,
    cmd: u16,
    cost: u8,
    kind: u32,
    magi: MagiRow,
    pax: Option<Arc<LoadedPax>>,
    /// The cast motion's slot: event 20 waits while it plays (host 1.15 `motion_state`).
    motion: Option<usize>,
    /// The motion id the cast asked for (the word frame trigger 17's parameter is compared with).
    motion_id: u16,
    /// Element started by event 20 ([g]+0x1c), for event 21 to stop.
    charge_fx: Option<u16>,
    step: Step,
    /// `KH2_MAGIC_VM=1`: the spell's script running in the interpreter instead of the hand port.
    vm: Option<Box<VmCast>>,
    /// The cast trigger has paid and restarted the script at event 21 (VM casts).
    paid: bool,
    /// The command's target record (host 2.23 for the interpreter): Sora's target or the Magic
    /// Lock-On pick; none for Cure (its record names the caster).
    target: Option<Entity>,
    /// Blizzard's shots for the slice after the yield.
    bliz: Option<BlizzardPlan>,
    /// Thunder's strikes still to start: (frames to wait after it, place, probe height, the place is
    /// host 1.248's scatter around it).
    thunder: Option<std::collections::VecDeque<(f32, Vec3, f32, bool)>>,
    thunder_wait: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Step {
    /// Event 20: waiting for frame trigger 17 or the end of the motion.
    Waiting,
    /// Event 21 ran its first slice; `ball` and `ball2` are the elements whose position the
    /// attacks follow, `t` the attack time left.
    Fire { ball: u16, ball2: Option<u16>, attacks: Option<(usize, Option<usize>)>, t: f32, yielded: bool },
    /// Blizzard's event 21 started element `elem` and yields once before it fires the shot.
    Blizzard { elem: u16, yielded: bool },
    /// Thunder's event 21 picks the strike's place (needs the targets; same frame).
    Thunder,
    /// The script runs in the interpreter (`KH2_MAGIC_VM=1`).
    Vm,
    /// The script has ended.
    Done,
}

/// An attack object of a spell (FUN_001a2ba0): its group, the base `atkp` row, its position this and
/// last frame, its size (radius +0x100, half height +0x104, centimetres) and whether it is live.
struct SpellAttack {
    group: u32,
    base_row: usize,
    pos: Option<Vec3>,
    prev: Option<Vec3>,
    radius: f32,
    height: f32,
    live: bool,
    /// It hit a target (flags 8 / 0x10 of the attack object [inferred]) or terrain (0x200 [inferred]).
    hit_any: bool,
    wall: bool,
    /// Byte +0xa4: the clashes the attack had with enemy attacks (FUN_0018d430; Reflect reads it).
    clashes: u8,
    /// The spell's effect file: the hit spark comes from it (`fx::SpellHit`).
    pax: Option<Arc<LoadedPax>>,
}

fn setup(
    mut commands: Commands,
    disc: Res<Disc>,
    data: Res<GameData>,
    world: Option<Res<crate::drive::DriveWorld>>,
    progress: Option<Res<crate::progress::Progress>>,
) {
    let (magc, cmds) = {
        let Ok(mut a) = disc.0.lock() else { return };
        let magc = kh2_tables::load::<Magc>(&mut a, &t::MAGC).map(|d| d.rows).unwrap_or_default();
        let cmds = kh2_tables::load::<Cmd>(&mut a, &t::CMD).map(|d| d.rows).unwrap_or_default();
        (magc, cmds)
    };
    let magi = disc
        .read("03system.bin")
        .and_then(|f| {
            let top = kh2_bar::parse(&f).ok()?;
            let pref = kh2_bar::find(&top, "pref")?;
            let inner = kh2_bar::parse(pref.data).ok()?;
            Some(rules::parse_magi(kh2_bar::find(&inner, "magi")?.data))
        })
        .unwrap_or_default();
    // The owned levels are the counts of the slots' items in `Progress` (which carries the app
    // baseline Fire/Blizzard/Thunder/Cure at level 1); without it the old default. `KH2_MAGIC` wins.
    let mut levels = progress.map_or([1u8, 1, 1, 1, 0, 0], |p| p.spell_levels());
    if let Ok(v) = std::env::var("KH2_MAGIC") {
        for (i, x) in v.split(',').enumerate().take(levels.len()) {
            levels[i] = x.trim().parse().unwrap_or(0);
        }
    }
    let world_id = world.map_or(0, |w| w.id as u8);
    let common = disc.read("effect/texcommon.dpd").and_then(|b| kh2_fx::Dpd::parse(&b).ok());
    let slots: Vec<_> = (0..rules::SLOTS)
        .map(|id| {
            let row = rules::pick_row(&magc, id, levels[id as usize], world_id)?.clone();
            let pax = fx::load(&disc, &row.file_name, 100 + id as usize, common.as_ref()).map(Arc::new);
            if pax.is_none() {
                warn!("magic: {} has no effect file", row.file_name);
            }
            Some((row, pax))
        })
        .collect();
    info!(
        "magic: slots {} (levels {levels:?}, world {world_id}); magi rows {}; MpDrive {}",
        slots.iter().flatten().map(|(r, _)| format!("{} cmd {}", r.file_name, r.command)).collect::<Vec<_>>().join(", "),
        magi.len(),
        data.prty.mp_drive
    );
    let scripts = slots
        .iter()
        .map(|s| {
            let (row, _) = s.as_ref()?;
            let file = disc.read(&row.file_name)?;
            let e = kh2_bar::parse(&file).ok()?.into_iter().find(|e| e.kind == 3)?;
            Some(Arc::new(e.data.to_vec()))
        })
        .collect();
    let exe = disc.read_root("SLUS_210.05");
    let host_table = exe.as_ref().and_then(|exe| kh2_bdx::exe::HostTable::from_exe(exe));
    let scatter = exe.as_ref().and_then(|exe| rules::ScatterConsts::from_exe(exe));
    commands.insert_resource(MagicData { magc, cmds, magi, levels, world: world_id, slots, mp_drive: data.prty.mp_drive, scripts, host_table, scatter });
}

/// `KH2_KEYS` entries for the menu keys the harness does not map (T, G, B), pressed the same way.
pub fn harness_keys(time: Res<Time>, mut keys: ResMut<ButtonInput<KeyCode>>) {
    let Ok(spec) = std::env::var("KH2_KEYS") else { return };
    let (t, dt) = (time.elapsed_secs(), time.delta_secs());
    for item in spec.split(',') {
        let mut parts = item.split(':');
        let (Some(at), Some(k)) = (parts.next(), parts.next()) else { continue };
        let hold: f32 = parts.next().and_then(|h| h.parse().ok()).unwrap_or(0.05);
        let Ok(at) = at.parse::<f32>() else { continue };
        let key = match k {
            "T" => KeyCode::KeyT,
            "G" => KeyCode::KeyG,
            "B" => KeyCode::KeyB,
            _ => continue,
        };
        if t >= at && t - dt < at {
            keys.press(key);
        } else if t >= at + hold && t - dt < at + hold {
            keys.release(key);
        }
    }
}

/// Menu glue (WP-7C): a menu line under `KH2_MAGIC_LOG` (the command menu lives in `items.rs`).
pub fn log_menu(line: &str) {
    if log_on() {
        info!("{line}");
    }
}

/// The caster's object frames 0x4000..=0x4009 this tick (FUN_0016aa18 through `fx::object_bone`), by
/// `bone - 0x4000`.
type Frames = [Mat4; 10];

fn object_frames(anims: &Query<&mut Animator>, locals: &Query<(&Transform, Option<&ChildOf>)>, body: Entity, weapons: &[Entity]) -> Frames {
    let owner_m = fx::world_now(body, locals);
    let Ok(anim) = anims.get(body) else { return [owner_m; 10] };
    let hands: Vec<Option<fx::ObjectFrame>> = weapons.iter().map(|w| anims.get(*w).ok().map(|a| fx::ObjectFrame::of(a, fx::world_now(*w, locals)))).collect();
    let owner = fx::ObjectFrame::of(anim, owner_m);
    std::array::from_fn(|k| fx::object_bone(0x4000 + k as u16, &owner, &hands, true))
}

/// The effect position of an element bound to the caster (host 0.53, FUN_001d9d00: the bind record's
/// position): the matrix the element's bone names (FUN_0016aa18, `fx::object_bone` for the numbers
/// with 0x4000: 0x4003 is the keyblade's blade moved out by its length) times the element's own
/// translation (game axes, Y and Z flipped). A plain bone number is taken as the object's matrix
/// [GAP: no spell element of the four files uses one].
fn element_position(pax: &LoadedPax, id: u16, frames: &Frames) -> Vec3 {
    let owner = frames[0];
    let Some(e) = pax.pax.elements_for(id).next() else { return owner.transform_point3(Vec3::ZERO) };
    let t = e.translation;
    let m = if e.bone & 0x4000 != 0 { frames.get((e.bone as usize) & 0xf).copied().unwrap_or(owner) } else { owner };
    m.transform_point3(Vec3::new(t[0], -t[1], -t[2]))
}

type Running<'w, 's> = Query<'w, 's, (Entity, &'static mut RunningEffect)>;

fn stop_effect(running: &mut Running, pax: &LoadedPax, id: u16, owner: Entity) {
    for (_, mut r) in running.iter_mut() {
        if r.lib.id == pax.id && r.game_id == id && matches!(r.bind, Bind::Owner(o) if o == owner) {
            r.effect.end();
        }
    }
}

/// The ability block, the drive state, the collision world and the game state, as optional resources,
/// and the attack objects published for the victims.
type Context<'w> = (
    Option<Res<'w, crate::abilities::Abilities>>,
    Option<Res<'w, crate::drive::DriveState>>,
    Option<Res<'w, crate::world::World>>,
    Option<Res<'w, crate::progress::Progress>>,
    Option<Res<'w, combat::ReactionData>>,
    ResMut<'w, SpellAttacks>,
);

type Victim =(Entity, Vec<(Volume, HurtSpheroid)>, Vec3);

/// The 60 Hz part: the MP charge, accepting a cast, the cast trigger, the spell scripts and their
/// attacks' hits.
#[allow(clippy::too_many_arguments)]
fn run(
    mut commands: Commands,
    data: Option<Res<MagicData>>,
    game: Res<GameData>,
    (mut st, mut mr): (ResMut<MagicState>, ResMut<MagicRun>),
    mut battle: ResMut<Battle>,
    abilities: Res<crate::sora::Abilities>,
    (block, drive, world, progress, reaction, mut published): Context,
    mut sora_q: Query<(Entity, &mut Sora)>,
    mut anims: Query<&mut Animator>,
    locals: Query<(&Transform, Option<&ChildOf>)>,
    mut fighters: Query<(Entity, &mut Fighter, &GlobalTransform)>,
    gts: Query<&GlobalTransform>,
    mut running: Running,
    (mut hits, mut spell_hits, mut voices, mut clash_out): (MessageWriter<Hit>, MessageWriter<fx::SpellHit>, MessageWriter<crate::audio::SoraVoice>, MessageWriter<crate::enemy::EnemyClash>),
    (enemies, mut recharge, pad, mut gflags): (Query<&crate::enemy::Enemy>, ResMut<crate::shared::hud::MpRecharge>, Res<crate::sora::Pad>, ResMut<rules::GlobalFlags>),
) {
    published.hits.clear();
    let Some(data) = data else { return };
    let Ok((me, mut s)) = sora_q.single_mut() else { return };
    let st = &mut *st;
    let mr = &mut *mr;
    gflags.merge(rules::GFLAG_COLLECT_AT_ONCE, mr.vm_world.gflags);
    mr.tick += 1;
    if !mr.hurt_done
        && let Some(hp) = hurt_due(mr.tick)
    {
        mr.hurt_done = true;
        mlog!("magic: KH2_MAGIC_HURT: Sora's HP {} -> {hp}", s.hp);
        s.hp = hp.clamp(1, s.stats.max_hp);
    }
    // The player object's form (+0xb50): the base object's until the body has been swapped.
    let form = drive.as_ref().map_or(0, |d| d.object_form());

    // MP charge (FUN_00175f48 each frame; the regeneration multiplier is the block's +0x34). The
    // drive form start ends it through `refill_mp` (FUN_001a0240).
    if st.charge.active() {
        let regen = block.as_ref().map_or(1.0, |b| b.block().mp_regen);
        if st.charge.tick(regen) {
            s.mp = s.stats.max_mp;
            mlog!("magic: MP charge over: MP {}", s.mp);
        }
    }
    recharge.progress = st.charge.left;
    recharge.total = st.charge.total;

    let body_e = s.body;
    // The caster's object frames (the pose sora::tick left for this tick).
    let frames = object_frames(&anims, &locals, body_e, &s.weapons);
    // The enemies' target points (each one's target shape centre, Bevy space).
    let targets = victim_targets(&fighters, &anims, &gts);
    // The shape byte of each enemy's target shape (byte +1 of the record, `parts`), for the lock-on marker.
    let shape_bytes: Vec<(Entity, u8)> = fighters.iter().filter(|(_, f, _)| f.team == Team::Enemy).filter_map(|(e, f, _)| Some((e, anims.get(f.hurt_body).ok()?.rig.model.shapes.iter().find(|sh| sh.kind == shape_kind::TARGET)?.parts))).collect();
    let alive = |e: Entity| enemies.get(e).is_ok_and(|x| x.alive());
    // A cast's target record as a point for the interpreter (host 1.120), while the target lives.
    let target_point = |t: Option<Entity>| t.filter(|e| alive(*e)).and_then(|e| targets.iter().find(|x| x.0 == e)).map(|x| x.1);
    mr.vm_world.refresh(&running);
    let Ok(mut body) = anims.get_mut(body_e) else { return };

    // Accept a pending cast (FUN_00174cf8: FUN_00176b00 with the hit latch forced, then FUN_00179400).
    if let Some((cmd, left)) = st.request.take() {
        let airborne = !s.motor.grounded || body.trigger(range::NO_GRAVITY);
        let (free, combo, fin) = match s.state {
            State::Ground | State::Air => (true, false, false),
            State::Attack { .. } | State::Magic => (
                body.trigger(range::GROUNDED) || body.trigger(range::AIR) || body.trigger(range::BLEND_TO_IDLE),
                body.trigger(range::ALLOW_COMBO),
                body.trigger(range::ALLOW_FINISHER),
            ),
            _ => (false, false, false),
        };
        let (mg, ma, mf) = abilities.combo_block();
        let mut chain = Chain { airborne, abilities: abilities.bits(), combo: s.hits, finishers: s.finishers, max_ground: mg, max_air: ma, max_finishers: mf, finisher_window: fin };
        let slot = data.slot_of_cmd(cmd);
        let row = slot.and_then(|i| data.slots[i as usize].as_ref());
        let status = data.status(cmd, s.mp, st.charge.active(), form, progress.as_deref());
        let pressed = (free || combo || fin).then(|| chain.press(Windows { free, combo }, true)).flatten();
        match (row, pressed) {
            (Some((row, pax)), Some(press)) if status.usable() && !mr.casts.iter().any(|c| c.step == Step::Waiting || (c.step == Step::Vm && !c.paid)) => {
                s.hits = chain.combo;
                s.finishers = chain.finishers;
                let finisher = form != 6 && rules::is_finisher(chain.combo, airborne, mg, ma);
                // The command's target record (+0x9f4, copied from the request by FUN_00174cf8): Cure's
                // names the caster (the app casts it on Sora), the others Sora's current target
                // [inferred: the menu code that fills the request's target was not traced].
                let own = row.id == 3;
                let mut target = if own || no_record() { None } else { s.target.filter(|t| alive(*t)) };
                // FUN_00179470 (Magic Lock-On): without a valid record a target is picked (FUN_0018fb98
                // with range 0: every target shape of every enemy) and the lock-on controller goes to
                // mode 1 on it (FUN_0019b3f0: the target marker; FUN_0019bcb8 its timer).
                let mut lock_mode1 = None;
                let has_lock_on = block.as_ref().is_some_and(|b| b.has_bit(rules::BIT_MAGIC_LOCK_ON));
                if rules::magic_lock_on_applies(has_lock_on, row.id) && target.is_none() {
                    let cands: Vec<(Entity, Vec3)> = targets.iter().filter(|(e, _)| alive(*e)).map(|(e, p)| (*e, *p / SCALE)).collect();
                    target = rules::lock_on_pick(s.motor.pos, s.motor.forward(), game.plyr.auto_lock_on_h_adjust, game.plyr.auto_lock_on_inner_adjust, None, &cands);
                    lock_mode1 = target;
                    mlog!("magic: Magic Lock-On (FUN_00179470): no target record, picked {target:?} of {} candidates; lock-on controller mode 1", cands.len());
                }
                let (motion_id, blend, fin_motion) = rules::cast_motion(row, airborne, finisher);
                let kind = rules::cast_kind(row.level, fin_motion, form);
                battle.ticks = battle.ticks.max(240.0);
                let pstate = PlayerState::BattleWeapon;
                let mslot = body.rig.mset.slot(motion_id as usize, pstate);
                body.restart(mslot, false, blend);
                s.enter(State::Magic, None);
                if own {
                    // Cure's script starts Leaf Bracer on the caster (host 1.343 bit 114, then 1.91; docs/systems/combat-effects/abilities.md).
                    s.start_leaf_bracer(&abilities, true);
                }
                if airborne {
                    // The mover's method +0xc with sstm MagicJump (FUN_001a4798) [inferred: a jump start].
                    s.motor.start_jump(game.sstm.magic_jump);
                }
                // The voice (FUN_001a4798 -> the player's method +0x54, FUN_00175d18).
                let voice = rules::cast_voice(row, fin_motion, own);
                if voice >= 0 {
                    voices.write(crate::audio::SoraVoice { voice: voice as u16, why: format!("cast {}", row.file_name) });
                }
                let lock_timer = if lock_mode1.is_some() { rules::mode1_timer_start(game.plyr.auto_lock_on_time) } else { 0.0 };
                mr.turn = Some(CastTurn { motion: mslot, target, own, lock_mode1, lock_timer, lock_mode2: false });
                let magi = rules::magi_row(&data.magi, kind).cloned().unwrap_or_default();
                let cost = data.cmd(cmd).map_or(0, |c| c.cost);
                mlog!(
                    "magic: cast {} (command {cmd}, slot {}, level {}): {press:?}, combo {} finisher {finisher}, kind {kind}, motion {motion_id} ({}) blend {blend}, airborne {airborne}, cost {cost}, MP {}, voice {voice}, target record {target:?} (own {own}), lock-on {:?}, magi {:?}",
                    row.file_name,
                    row.id,
                    row.level,
                    chain.combo,
                    body.tag(),
                    s.mp,
                    s.lock,
                    &magi.0[..magi.0.len().min(16)]
                );
                // Event 20 (FIRE/BLIZZARD/THUNDER/CURE): the charge effect of the cast. Wisdom and Final
                // (kind / 10 = 1, 3) start none in Fire; every other case starts element 0.
                // `KH2_MAGIC_VM=1`: the script itself runs event 20 instead.
                // Magnet and Reflect run their own scripts (WP-7I); the other four with `KH2_MAGIC_VM=1`.
                let vm = if vm_on() || row.id >= 4 { pax.as_ref().zip(data.scripts.get(row.id as usize).cloned().flatten()).and_then(|(_, code)| VmCast::start(&code, data.host_table.as_ref(), kind)) } else { None };
                let charge_fx = if vm.is_none() { charge_element(row.id, kind) } else { None };
                if let (Some(id), Some(p)) = (charge_fx, pax) {
                    fx::start(&mut commands, p, id, Bind::Owner(body_e));
                }
                let step = if vm.is_some() { Step::Vm } else { Step::Waiting };
                // The hand port keeps the kind the script keeps after event 20 (the VM's script makes it itself).
                let kind = if vm.is_some() { kind } else { rules::script_kind(row.id, kind) };
                let mut cast = Cast { slot: row.id, cmd, cost, kind, magi, pax: pax.clone(), motion: mslot, motion_id, charge_fx, step, vm, paid: false, target, bliz: None, thunder: None, thunder_wait: 0.0 };
                mr.cast_tick = mr.tick;
                if cast.vm.is_some() {
                    mlog!("magic(vm): {} event 20 runs in the interpreter", row.file_name);
                    let target = target_point(cast.target);
                    let mut ctx = VmCtx { commands: &mut commands, game: &game, frames: &frames, running: &mut running, attacks: &mut mr.attacks, next: &mut mr.next_group, body: body_e, world: &mut mr.vm_world, map: world.as_deref(), target };
                    vm_slice(&mut cast, &mut ctx, airborne, true);
                }
                mr.casts.push(cast);
            }
            _ => {
                if left > 1 && status.usable() {
                    st.request = Some((cmd, left - 1));
                } else {
                    mlog!("magic: cast of command {cmd} dropped (state {:?}, status {status:?}, windows free {free} combo {combo} finisher {fin})", s.state);
                }
            }
        }
    }

    // The turn triggers of the cast motion, each frame they cover (FUN_00188030 cases 22 and 28 ->
    // FUN_00186018): toward the command's target record, or toward the lock-on controller's target
    // (Sora's lock-on, else the mode-1 target of Magic Lock-On), by at most the trigger's degrees per
    // frame. The target point is the target's target shape (FUN_0018efa0); the caster's is its
    // position [inferred: the offset +0x560 added to it has no horizontal part].
    // The controller's frame (FUN_0019b7a8, mode 1): the lock button moves it to mode 2, else the timer counts down
    // while the player is up (FUN_0019a4f0); once it has run out the controller picks again with the player's
    // AutoLockOnRange and height window (FUN_0018f538 case 6 -> FUN_0018fb98) and a valid pick replaces the record
    // (FUN_0019b5e8, the timer stays out, so it picks again each frame); no pick releases the controller (mode 0).
    // [inferred: this runs before the trigger below in a frame.]
    let sora_up = rules::player_up(false, fighters.get(me).is_ok_and(|f| !f.1.hittable), true, s.hp);
    // The lock button is the input record's newly pressed bit 0x800 (`Pad::lock_newly_pressed`). In mode 2 a second press
    // releases the controller (FUN_0019b7a8: FUN_002c14a8(8), then FUN_0019b580, mode 0 with the record cleared; the system
    // sound 8 is not played).
    if let Some(t) = mr.turn.as_mut()
        && t.lock_mode1.is_some()
        && t.lock_mode2
        && !rules::mode2_keeps(true, pad.lock_newly_pressed())
    {
        mlog!("magic: lock button pressed again in mode 2 (FUN_0019b7a8): the lock-on controller is released (mode 0)");
        t.lock_mode1 = None;
        t.lock_mode2 = false;
    }
    if let Some(t) = mr.turn.as_mut()
        && t.lock_mode1.is_some()
        && !t.lock_mode2
    {
        let pressed = pad.lock_newly_pressed();
        match rules::mode1_frame(t.lock_timer, pressed, sora_up) {
            rules::Mode1Frame::Keep(left) => t.lock_timer = left,
            rules::Mode1Frame::ToMode2 => {
                mlog!("magic: lock button in mode 1 (FUN_0019b7a8): the lock-on controller goes to mode 2 on {:?}", t.lock_mode1);
                t.lock_mode2 = true;
            }
            rules::Mode1Frame::Repick => {
                let p = &game.plyr;
                let window = rules::LockWindow { range: p.auto_lock_on_range, min_h: p.auto_lock_on_min_h, max_h: p.auto_lock_on_max_h, h_scale: 0.0 };
                let cands: Vec<(Entity, Vec3)> = targets.iter().filter(|(e, _)| alive(*e)).map(|(e, p)| (*e, *p / SCALE)).collect();
                let pick = rules::lock_on_pick(s.motor.pos, s.motor.forward(), p.auto_lock_on_h_adjust, p.auto_lock_on_inner_adjust, Some(window), &cands);
                if pick != t.lock_mode1 {
                    mlog!("magic: lock-on controller mode 1 timer out (FUN_0018f538 case 6): {:?} -> {pick:?}", t.lock_mode1);
                }
                t.lock_mode1 = pick;
            }
        }
    }
    match mr.turn {
        Some(t) if s.state == State::Magic && body.slot == t.motion => {
            let g = body.gframe();
            let turns: Vec<(u8, f32)> = body
                .triggers()
                .map(|tr| {
                    tr.active(g)
                        .filter(|r| matches!(r.id, rules::TRIGGER_TURN_TARGET | rules::TRIGGER_TURN_LOCK))
                        .map(|r| (r.id, r.params.first().copied().unwrap_or(0) as u16 as f32))
                        .collect()
                })
                .unwrap_or_default();
            for (id, deg) in turns {
                if id == rules::TRIGGER_TURN_LOCK
                    && let Some(t) = mr.turn.as_mut()
                    && t.lock_mode1.is_some()
                {
                    // FUN_001861b8 refills the mode-1 timer (FUN_0019bcb8) before it turns.
                    t.lock_timer = rules::mode1_timer_start(game.plyr.auto_lock_on_time);
                }
                let (rec, own) = if id == rules::TRIGGER_TURN_TARGET { (t.target, t.own) } else { (s.lock.or(t.lock_mode1), false) };
                let Some(at) = rec.filter(|e| alive(*e)).and_then(|e| targets.iter().find(|x| x.0 == e)).map(|x| x.1 / SCALE) else { continue };
                let before = s.motor.yaw;
                s.motor.yaw = rules::cast_turn(before, s.motor.pos, at, own, deg);
                if before != s.motor.yaw {
                    mlog!("magic: turn trigger {id} at frame {g:.1}: toward {rec:?} by at most {deg} deg, yaw {before:.3} -> {:.3}", s.motor.yaw);
                }
            }
        }
        Some(_) if s.state != State::Magic => mr.turn = None,
        _ => {}
    }

    // The controller's marker (FUN_0019bbc0, run by FUN_0019b3f0 on every mode change and by FUN_0019b5e8 on every new record):
    // the icon exists while the controller is in mode 1 or 2 on a record and is released with it (FUN_0019b580). The mode, the
    // record's shape byte (FUN_0019b388: byte +1 of the picked target shape, `parts`) and the record's target point are
    // handed to the HUD each frame. The bubble flag is the target's +0x588 bit 1 with a positive +0xac8 / +0xacc, which an
    // enemy object here does not carry [GAP], so it stays off.
    let marker = mr.turn.and_then(|t| {
        let e = t.lock_mode1.filter(|e| alive(*e))?;
        let at = targets.iter().find(|x| x.0 == e)?.1;
        let shape = shape_bytes.iter().find(|x| x.0 == e).map_or(0, |x| x.1);
        Some(MagicMarker { mode: rules::marker_mode(t.lock_mode2), shape, at, alert: false })
    });
    match marker {
        Some(m) => {
            if mr.marker_log != Some((m.mode, m.shape)) {
                mlog!("magic: marker mode {} shape byte {} group {:?} at {:?}", m.mode, m.shape, crate::shared::hud::marker_group(m.mode, m.shape), m.at);
                mr.marker_log = Some((m.mode, m.shape));
            }
            commands.insert_resource(m);
        }
        None => {
            if mr.marker_log.take().is_some() {
                mlog!("magic: marker released");
                commands.remove_resource::<MagicMarker>();
            }
        }
    }

    // Frame trigger 17 of the motion this tick (FUN_00188498 case 0x11): pay, then event 21. A trigger
    // with a parameter fires only when it equals the motion controller's +0x14, the motion id the
    // cast asked for (FUN_0017e000 writes it on a successful request; WP-7I, ghidra) [the data agrees: the cast slots are shared by spells (Fire's ground motion
    // 56 and Reflect's 71 play one slot), and each slot carries one trigger 17 per motion id, Reflect's
    // at frame 0 and Fire's later].
    let fired: Vec<Option<i16>> = body
        .motion()
        .zip(body.triggers())
        .map(|(m, t)| {
            let g = |f: f32| f * crate::data::LOGIC_HZ / m.fps.max(1.0);
            let (from, to) = (g(body.prev_frame), g(body.frame));
            if from == to {
                return Vec::new();
            }
            t.crossed(from, to).filter(|f| f.id == frame::MAGIC_CAST).map(|f| f.params.first().copied()).collect()
        })
        .unwrap_or_default();
    let fires = |c: &Cast| fired.iter().any(|p| p.is_none_or(|p| p as u16 == c.motion_id));
    let airborne_now = !s.motor.grounded;
    let (magic_state, playing) = (s.state == State::Magic, body.slot);
    let in_motion = |c: &Cast| magic_state && playing == c.motion;
    for c in mr.casts.iter_mut() {
        if c.step != Step::Waiting && !(c.step == Step::Vm && !c.paid) {
            continue;
        }
        if fires(c) && in_motion(c) {
            // FUN_001a4a60 -> FUN_001769e0: the cost byte comes off MP (FUN_00186368).
            let before = s.mp;
            match rules::pay(s.mp, s.stats.max_mp, c.cost, data.mp_drive) {
                Paid::Mp(v) => s.mp = v,
                Paid::Charge { total } => {
                    s.mp = 0;
                    st.charge = Charge { left: total, total };
                }
            }
            if log_level() >= 2 {
                info!("magic: timeline +{} trigger 17", mr.tick - mr.cast_tick);
            }
            mlog!("magic: trigger 17 at frame {:.1}: MP spent {} -> {} (cost {}){}", body.gframe(), before, s.mp, c.cost, if st.charge.active() { format!(", MP charge {:.0} frames", st.charge.total) } else { String::new() });
            if let Some(vm) = c.vm.as_mut() {
                // FUN_0016c500(thread, 21, 0, 0): the script's own event 21 runs in this tick's slice.
                c.paid = true;
                vm.rt.restart_thread(21, &[]);
                mlog!("magic(vm): event 21");
                continue;
            }
            let mut bliz = None;
            c.step = event21(&mut commands, c, &data, &game, &mut s, me, body_e, &frames, airborne_now, &mut running, &mut mr.bliz_counter, &mut bliz);
            c.bliz = bliz;
        } else if !in_motion(c) && c.vm.is_none() {
            // Event 20 ends when the caster's motion changes (host 1.15): no spell, no cost.
            mlog!("magic: cast of command {} ended before its trigger (Sora {:?})", c.cmd, s.state);
            c.step = Step::Done;
        }
    }

    // The scripts in the interpreter: one slice per tick (FUN_001a4428), except a cast's first, which
    // ran when it started.
    for c in mr.casts.iter_mut().filter(|c| c.step == Step::Vm) {
        let im = in_motion(c);
        if c.vm.as_ref().is_some_and(|v| !v.ran) {
            let target = target_point(c.target);
            let mut ctx = VmCtx { commands: &mut commands, game: &game, frames: &frames, running: &mut running, attacks: &mut mr.attacks, next: &mut mr.next_group, body: body_e, world: &mut mr.vm_world, map: world.as_deref(), target };
            vm_slice(c, &mut ctx, airborne_now, im);
        }
        if let Some(v) = c.vm.as_mut() {
            v.ran = false;
        }
    }

    // The scripts after their first slice (Fire's attack loop).
    for c in mr.casts.iter_mut() {
        if let Step::Fire { ball, ball2, attacks, t, yielded } = c.step {
            if !yielded {
                // The script's yield after starting the effects: the attacks come next frame.
                c.step = Step::Fire { ball, ball2, attacks, t, yielded: true };
                continue;
            }
            let Some(pax) = c.pax.clone() else {
                c.step = Step::Done;
                continue;
            };
            let (a1, a2) = match attacks {
                Some(a) => a,
                None => {
                    // After the yield: the attack objects (FIRE event 21).
                    let a1 = new_attack(&mut mr.attacks, &mut mr.next_group, &game, &pax, ATKP_FIRE, c.kind, c.magi.f(MAGI_RADIUS), c.magi.f(MAGI_HEIGHT));
                    let mut a2 = None;
                    if c.kind / 10 == 3 {
                        // Host 2.31 copies the attack with id 502, sized 30 smaller; a finisher widens the first to 450.
                        a2 = new_attack(&mut mr.attacks, &mut mr.next_group, &game, &pax, ATKP_FIRE, c.kind, c.magi.f(MAGI_RADIUS) - 30.0, c.magi.f(MAGI_HEIGHT) - 30.0);
                        if c.kind & 1 == 1
                            && let Some(i) = a1
                        {
                            mr.attacks[i].radius = 450.0;
                        }
                    }
                    let Some(a1) = a1 else {
                        warn!("magic: no atkp row {ATKP_FIRE} for kind {}", c.kind);
                        c.step = Step::Done;
                        continue;
                    };
                    mlog!("magic: Fire attack {} made: atkp {ATKP_FIRE} kind {} radius {} height {} for {} frames", mr.attacks[a1].group, c.kind, mr.attacks[a1].radius, mr.attacks[a1].height, c.magi.f(MAGI_TIME));
                    (a1, a2)
                }
            };
            if t > 0.0 {
                let p = element_position(&pax, ball, &frames);
                move_attack(&mut mr.attacks[a1], p);
                if let (Some(b2), Some(i2)) = (ball2, a2) {
                    move_attack(&mut mr.attacks[i2], element_position(&pax, b2, &frames));
                }
                c.step = Step::Fire { ball, ball2, attacks: Some((a1, a2)), t: t - 1.0, yielded };
            } else {
                // Host 2.13: the attack objects are finished.
                mr.attacks[a1].live = false;
                if let Some(i2) = a2 {
                    mr.attacks[i2].live = false;
                }
                mlog!("magic: Fire attack {} over", mr.attacks[a1].group);
                c.step = Step::Done;
            }
        }
    }
    // Blizzard's event 21 after its yield: the push of the caster (host 1.146 with `magi` +0x1c) and
    // the shot from element 1's position along the facing (R02b9 -> R02d7 -> R04d6).
    for c in mr.casts.iter_mut() {
        if let Step::Blizzard { elem, yielded } = c.step {
            if !yielded {
                c.step = Step::Blizzard { elem, yielded: true };
                continue;
            }
            // L00ab, every kind: host 1.146 (FUN_001705e8 -> FUN_001857b0) pushes the caster along the
            // facing turned by pi (host 0.13) with the power `magi` +0x1c: the decaying ground push of
            // the wall recoil (initial speed (power + stat +0x1c8) * 0.1, times 0.9 per frame; the stat
            // term is 0 in the app as for the recoil).
            let back = -s.motor.forward().with_y(0.0).normalize_or(Vec3::Z);
            if let Some(rd) = reaction.as_deref() {
                mr.push = crate::reaction_rules::Push::new(back, c.magi.f(MAGI_PUSH), 0.0, &rd.consts);
                mr.push_moved = 0.0;
                mlog!("magic: Blizzard push of the caster (host 1.146 -> FUN_001857b0): power {} along {back:?}, speed {:?}", c.magi.f(MAGI_PUSH), mr.push.map(|p| p.speed));
            }
            if let (Some(pax), Some(plan)) = (c.pax.clone(), c.bliz.take()) {
                // The facing (host 1.201) in the game's space; each shot's direction is it with the
                // plan's height, turned about Y (host 0.13).
                let facing = s.motor.forward().with_y(0.0).normalize_or(Vec3::Z);
                for sh in &plan.shots {
                    let pos = element_position(&pax, sh.elem, &frames);
                    let origin = caster_lock_point(&body, gts.get(body_e).ok());
                    let dir = rotate_y_game(facing.with_y(-sh.y), sh.angle);
                    mlog!("magic: Blizzard shot from element {} at {pos:?} along {dir:?} (flag {}, turn scale {}): speed {} life {} size {} turn {}", sh.elem, sh.flag, sh.scale, c.magi.f(MAGI_SPEED), c.magi.f(MAGI_LIFE), c.magi.f(MAGI_SHOT_SIZE), c.magi.f(MAGI_TURN));
                    mr.shots.push(Shot { pax: pax.clone(), kind: c.kind, magi: c.magi.clone(), pos, origin, dir, target: s.target, attack: None, effect: None, life: c.magi.f(MAGI_LIFE) - SHOT_FADE as f32, fade: None, done: false, turn_scale: sh.scale, flag: sh.flag });
                }
            }
            let _ = elem;
            c.step = Step::Done;
        }
    }
    // The push of host 1.146 (FUN_001858b0 each frame: move by the speed, then decay; it ends below
    // the cutoff). Sora's own push field is his module's, so the app steps this one here, after his
    // tick; a hit reaction replaces it as it replaces the game's push [inferred for the air: the
    // script calls the ground push FUN_001857b0 also for an air cast].
    if matches!(s.state, State::Hurt | State::Blown | State::Down) {
        mr.push = None;
    }
    if let (Some(p), Some(rd), Some(w)) = (mr.push.as_mut(), reaction.as_deref(), world.as_deref()) {
        let d = p.step(&rd.consts);
        let dy = (!s.motor.grounded).then_some(0.0);
        let before = s.motor.pos;
        s.motor.move_by(w, d, dy);
        mr.push_moved += (s.motor.pos - before).with_y(0.0).length();
        if p.finished() {
            mr.push = None;
            mlog!("magic: Blizzard push over: Sora moved {:.1} units", mr.push_moved);
            mr.push_moved = 0.0;
        }
    }
    // Thunder's event 21, the rest of its first slice: the strike's place is the target's position
    // (host 1.120) raised by 100, or 300 ahead of the caster raised by 80 and 330 without a target
    // (L003e / L0076 / L009c), then R02b5 starts the strike thread.
    for c in mr.casts.iter_mut().filter(|c| c.step == Step::Thunder) {
        let Some(pax) = c.pax.clone() else {
            c.step = Step::Done;
            continue;
        };
        if c.thunder.is_none() {
            let here = s.motor.pos * SCALE;
            let target = s.target.and_then(|t| targets.iter().find(|x| x.0 == t)).map(|t| t.1);
            let facing = s.motor.forward().with_y(0.0).normalize_or(Vec3::Z);
            let plan = thunder_plan(c.kind, here, facing, target, c.magi.f(MAGI_THUNDER_RADIUS));
            mlog!("magic: Thunder event 21 (kind {}): {} strikes, target {:?}", c.kind, plan.len(), s.target);
            c.thunder = Some(plan.into());
        }
        // R029f between the strikes: the frames are counted down one per slice.
        if c.thunder_wait > 0.0 {
            c.thunder_wait -= 1.0;
            if c.thunder_wait > 0.0 {
                continue;
            }
        }
        match c.thunder.as_mut().and_then(|q| q.pop_front()) {
            Some((wait, mut pos, rise, scatter)) => {
                if scatter {
                    // Host 1.248 (L016f): R = 2.5 * `magi` +0x24, radius 0.4 R .. R, height -0.1 R ..
                    // 0.1 R; the line test against the collision world [inferred: mask 2's surfaces
                    // were not told apart, every triangle blocks].
                    let r = 2.5 * c.magi.f(MAGI_THUNDER_RADIUS);
                    match data.scatter.as_ref() {
                        Some(sc) => {
                            let clear = |a: Vec3, b: Vec3| world.as_deref().is_none_or(|w| w.raycast_m(a * SCALE, b * SCALE).is_none());
                            let centre = pos;
                            pos = rules::scatter_place(pos / SCALE, 0.4 * r, r, -0.1 * r, 0.1 * r, &mut mr.scatter, sc, clear) * SCALE;
                            mlog!("magic: Thunder in Final: host 1.248 around {centre:?} (radius {} .. {r}): {pos:?} (generator {:#x}, counter {})", 0.4 * r, mr.scatter.seed, mr.scatter.turn);
                        }
                        None => mlog!("magic: Thunder in Final: no scatter numbers from the executable; the strike stays at the place"),
                    }
                }
                mlog!("magic: Thunder strike at {pos:?} (rise {rise})");
                mr.strikes.push(Strike { pax, kind: c.kind, magi: c.magi.clone(), pos, rise, atkp: 0, started: false, wait: 0.0, attacks: None, done: false });
                c.thunder_wait = wait;
                if c.thunder.as_ref().is_some_and(|q| q.is_empty()) {
                    c.step = Step::Done;
                }
            }
            None => c.step = Step::Done,
        }
    }
    mr.casts.retain(|c| c.step != Step::Done);
    for k in mr.strikes.iter_mut() {
        step_strike(k, &mut commands, &game, &mut mr.attacks, &mut mr.next_group, world.as_deref());
    }
    mr.strikes.retain(|x| !x.done);

    // The shots (request threads; they run after the spell threads in the same task pass [inferred]).
    let mut claimed: Vec<Entity> = mr.shots.iter().filter_map(|x| x.effect).collect();
    for shot in mr.shots.iter_mut() {
        step_shot(shot, &mut commands, &game, &mut mr.attacks, &mut mr.next_group, &mut running, &targets, &claimed);
        claimed.extend(shot.effect);
    }
    mr.shots.retain(|x| !x.done);

    // The attacks against the enemies' hurt shapes, then the victims' hit memory counts down.
    let victims: Vec<Victim> = fighters
        .iter()
        .filter(|(_, f, _)| f.team == Team::Enemy && f.hittable)
        .filter_map(|(e, f, g)| {
            let (a, bg) = (anims.get(f.hurt_body).ok()?, gts.get(f.hurt_body).ok()?);
            let shapes = a
                .rig
                .model
                .shapes
                .iter()
                .filter(|sh| sh.kind == shape_kind::BATTLE || sh.kind == shape_kind::TARGET)
                .map(|sh| (combat::shape_volume(sh, a, bg), combat::hurt_spheroid(sh, a, bg)))
                .collect();
            Some((e, shapes, g.translation()))
        })
        .collect();
    let sora_pos = s.motor.pos * SCALE;
    let mut disable: Vec<(Entity, Entity, i16)> = Vec::new();
    let live_groups: Vec<u32> = mr.attacks.iter().filter(|a| a.live).map(|a| a.group).collect();
    mr.clash_done.retain(|k| live_groups.contains(&k.0));
    for a in mr.attacks.iter_mut().filter(|a| a.live) {
        let Some(cur) = a.pos else { continue };
        if log_level() >= 2 {
            info!("magic: timeline +{} attack {} atkp {} at {:.1?} radius {} height {}", mr.tick - mr.cast_tick, a.group, game.atkp[a.base_row].id, cur / SCALE, a.radius, a.height);
        }
        // FUN_0018c8e0 (from FUN_001a2cc0 when the attack moved more than two units): a row with BG Hit
        // (atkp Flags bit 0) whose attack has not hit yet casts the segment from the last to this
        // position against terrain that blocks attacks (mask 4); a hit goes to the wall handler
        // FUN_0018ca78 [inferred: it sets flag 0x200, which ends a Blizzard shot]. The second, lowered
        // segment for a non-zero `FloorCheck` is not ported.
        let moved = a.prev.map_or(0.0, |p| (p - cur).length() / SCALE);
        if game.atkp[a.base_row].flags & 1 != 0
            && moved > SWEEP_MIN
            && !a.hit_any
            && !a.wall
            && let (Some(w), Some(prev)) = (world.as_deref(), a.prev)
            && let Some((p, _)) = w.touching_m(prev, cur, 0.01, kh2_map::flags::ATTACK_COLLIDE)
        {
            a.wall = true;
            mlog!("magic: attack {} meets terrain at {p:?}", a.group);
        }
        // Reflect's guard (a type-3 row): FUN_0018d808 pairs it with enemy attack volumes the clash
        // table allows (`ExeConsts::types_clash`); each pair counts once in the guard's byte +0xa4
        // (FUN_0018d430), the enemy gets its clash reaction (0x1b2478, `reaction_rules::enemy_clash`) and
        // its volume is disabled unless its own row says Nothing (FUN_0018d6b0), so it cannot hit
        // Sora. The guard's own row has `RefactSelf` 2 and stays. The guard hits no object [inferred:
        // Power 0, reaction 0; the hit loop below is skipped for it].
        if game.atkp[a.base_row].r#type == GUARD_TYPE {
            let Some(rd) = reaction.as_deref() else { continue };
            let guard = attack_volume(cur, a.radius, a.height);
            for (fe, f, _) in fighters.iter() {
                if f.team != Team::Enemy {
                    continue;
                }
                for &ab in &f.attack_bodies {
                    let (Ok(an), Ok(g)) = (anims.get(ab), gts.get(ab)) else { continue };
                    for (atkp, start, vol) in combat::live_attacks(an, g) {
                        let key = (a.group, fe, ab, start);
                        let Some(er) = game.atkp(atkp) else { continue };
                        if mr.clash_done.contains(&key) || !rd.consts.types_clash(GUARD_TYPE, er.r#type) || !combat::overlap(&guard, &vol) {
                            continue;
                        }
                        mr.clash_done.insert(key);
                        a.clashes = a.clashes.wrapping_add(1);
                        let ef = crate::reaction_rules::ReflectFields::from_atkp(er);
                        let c = crate::reaction_rules::enemy_clash(&ef, false);
                        clash_out.write(crate::enemy::EnemyClash { enemy: fe, motion: c.and_then(|c| c.motion), power: c.map_or(0.0, |c| c.push_power), own_atkp: atkp, other_atkp: game.atkp[a.base_row].id as i16 });
                        if ef.refact_self != crate::reaction_rules::Refact::Nothing {
                            disable.push((fe, ab, start));
                        }
                        mlog!("magic: guard {} clashes with {fe:?} attack {atkp} (clashes {})", a.group, a.clashes);
                    }
                }
            }
            continue;
        }
        for (te, shapes, tpos) in &victims {
            let mem = mr.memory.entry(*te).or_default();
            let row = &game.atkp[a.base_row];
            if !mem.may_hit(row.r#type, a.group) {
                continue;
            }
            let Some((point_from, sph)) = sweep_points(&*a, cur).into_iter().find_map(|c| {
                let v = attack_volume(c, a.radius, a.height);
                shapes.iter().find(|(h, _)| combat::overlap(&v, h)).map(|(_, sph)| (c, *sph))
            }) else {
                continue;
            };
            let n = mem.count_hit(a.group);
            let vi = rules::variant(&game.atkp, a.base_row, n);
            let vrow = &game.atkp[vi];
            mem.mark(a.group, vrow.interval);
            let dir = (*tpos - sora_pos).with_y(0.0).normalize_or(Vec3::Z);
            let point = combat::victim_point(&sph, point_from);
            let hit = Hit { attacker: me, target: *te, atkp: vrow.id as i16, dir, point, row: Some(vrow.sub_id) };
            if log_on() {
                // The game's damage for this row (FUN_001a0610 before the boosts the receiver adds).
                let expected = game.hit_row(&hit).zip(enemies.get(*te).ok()).map(|(r, e)| game.damage(r, &s.stats, &e.stats));
                info!(
                    "magic: hit by attack {} on {te:?}: atkp {} row SubId {} (hit {n}, Power {}, Element {}, Interval {}), magic {}, damage {expected:?}",
                    a.group, vrow.id, vrow.sub_id, vrow.power, vrow.element, vrow.interval, s.stats.magic
                );
            }
            hits.write(hit);
            published.hits.push((*te, hit.atkp, a.group));
            if let Some(pax) = &a.pax {
                spell_hits.write(fx::SpellHit { target: *te, atkp: hit.atkp, pax: pax.clone() });
            }
            a.hit_any = true;
        }
    }
    // A disabled enemy volume cannot hit: its window counts as landed on the player's fighters (as
    // clash.rs does), so `combat::detect_hits` skips it until the window closes.
    if !disable.is_empty() {
        let players: Vec<Entity> = fighters.iter().filter(|(_, f, _)| f.team == Team::Player).map(|(e, _, _)| e).collect();
        for (fe, ab, start) in disable {
            if let Ok((_, mut f, _)) = fighters.get_mut(fe) {
                for p in &players {
                    f.landed.insert((*p, ab, start));
                }
            }
        }
    }
    for m in mr.memory.values_mut() {
        m.tick();
    }
    // What the victims read next (the pull of reaction class 5): the live attacks' positions and the
    // enemies' body centres (FUN_0016b7d0: the first shape of type 1, else the position).
    published.places = mr.attacks.iter().filter(|a| a.live).filter_map(|a| a.pos.map(|p| (a.group, p))).collect();
    published.centres = fighters
        .iter()
        .filter(|(_, f, _)| f.team == Team::Enemy)
        .filter_map(|(e, f, g)| {
            let (an, bg) = (anims.get(f.hurt_body).ok()?, gts.get(f.hurt_body).ok()?);
            let c = an.rig.model.shapes.iter().find(|sh| sh.kind == BODY_SHAPE).map(|sh| combat::hurt_spheroid(sh, an, bg).center);
            Some((e, c.map_or(Vec3::ZERO, |c| c - g.translation())))
        })
        .collect();
    // Casts hold indices into the list: finished attacks go once no cast runs.
    if mr.casts.is_empty() && mr.shots.is_empty() && mr.strikes.is_empty() {
        mr.attacks.retain(|a| a.live);
    }
}

/// What the first slice of Fire's event 21 does (FIRE_*.mag R001a -> R017d), by kind and airborne.
#[derive(Clone, Debug, PartialEq)]
struct FirePlan {
    /// Every cast but Final's stops the charge effect and starts element 1 (`call R01f9; i.iszero;
    /// jz`: the branch is taken when the kind is not Final's; Final starts no charge effect in event
    /// 20). Found by running the script in the interpreter (WP-7A).
    stop_charge: bool,
    /// The elements started, in order.
    starts: Vec<u16>,
    /// The element the first attack follows ([g]+0x24) and the one the Final copy follows.
    ball: u16,
    ball2: Option<u16>,
}

/// Fire's event 21, first slice: element 1 (not Final), the ball (5 airborne, else 2), its second
/// element (7 / 6) for Wisdom finishers, Master and Final finishers; Final adds element 10 (the copy
/// attack follows it) and 11, 12 (finisher) or 8, 9.
fn fire_plan(kind: u32, airborne: bool) -> FirePlan {
    let (k10, fin) = (kind / 10, kind & 1 == 1);
    let mut starts = Vec::new();
    if k10 != 3 {
        starts.push(1);
    }
    let (ball, extra) = if airborne { (5, 7) } else { (2, 6) };
    starts.push(ball);
    if (k10 == 1 && fin) || k10 == 2 || (k10 == 3 && fin) {
        starts.push(extra);
    }
    let mut ball2 = None;
    if k10 == 3 {
        starts.push(10);
        ball2 = Some(10);
        starts.extend(if fin { [11, 12] } else { [8, 9] });
    }
    FirePlan { stop_charge: k10 != 3, starts, ball, ball2 }
}

/// Fire's event 20: Wisdom and Final (kind / 10 = 1, 3) start no charge effect; every other kind
/// starts element 0 (the other spells always do).
fn charge_element(slot: u8, kind: u32) -> Option<u16> {
    (slot <= 3 && !(slot == 0 && matches!(kind / 10, 1 | 3))).then_some(0)
}

/// BLIZZARD_*.mag L039d, the end test of a shot each frame: host 2.29 (the attack's flags 8 / 0x10, a
/// wall: FUN_0018ca78), 2.39 (0x200, a clash: FUN_0018d430), or 2.14 (FUN_0018c890: flag 2, a target,
/// or any of the others) when the block's flag (+0x64) is 0. So a shot with flag 1 (every cast but the
/// Wisdom finisher's and Master's) goes on through the targets it hits. The app's attack has no clash
/// with enemy attacks, so `clash` is false for it.
fn shot_ends(wall: bool, clash: bool, target: bool, flag: u32) -> bool {
    wall || clash || ((target || wall || clash) && flag == 0)
}

/// One Blizzard shot of event 21 (R02b9(element, direction, flag, scale)): the element whose position
/// it starts from, the turn about Y of the facing (host 0.13, radians), the height the direction is
/// given first (the game's Y, down; Master only), the block's flag (+0x64) and the turn-rate scale.
#[derive(Clone, Debug, PartialEq)]
struct BlizzardShot {
    elem: u16,
    angle: f32,
    y: f32,
    flag: u32,
    scale: f32,
}

#[derive(Clone, Debug, PartialEq)]
struct BlizzardPlan {
    elements: Vec<u16>,
    shots: Vec<BlizzardShot>,
    /// The cast reaches L022b, which counts it in the global +0x2c.
    counts: bool,
}

/// BLIZZARD_*.mag R0017 by the script's kind and the global counter +0x2c. The elements: Final
/// non-finishers start 1 and 7, Final finishers 7 when the counter is 1 or 3, 1 and 7 when it is 4,
/// else 1; every other kind 1. The shots: the Wisdom finisher sends five from the first element (0.5
/// scale ahead, 1.0 at +-35 degrees, 2.0 at +-70 degrees, flag 0); Master seven with scale 1, flag 0
/// (ahead, then with the height -0.96 and 0.96, then 0.4 turned +-45 degrees, then -0.4 turned
/// +-45 degrees); every other kind one ahead with flag 1 and, when a second element started (7), one
/// more from it. Only the Wisdom finisher, Master and the one-element case reach the counter.
fn blizzard_plan(kind: u32, counter: u32) -> BlizzardPlan {
    let (k10, fin) = (kind / 10, kind & 1 == 1);
    let mode = if k10 == 3 {
        if fin {
            match counter {
                1 | 3 => 1,
                4 => 2,
                _ => 0,
            }
        } else {
            2
        }
    } else {
        0
    };
    let elements: Vec<u16> = match mode {
        0 => vec![1],
        1 => vec![7],
        _ => vec![1, 7],
    };
    let e = elements[0];
    let shot = |angle: f32, y: f32, flag: u32, scale: f32| BlizzardShot { elem: e, angle, y, flag, scale };
    let (a35, a70, a45) = (0.610_865_f32, 1.221_73_f32, std::f32::consts::FRAC_PI_4);
    if k10 == 1 && fin {
        let shots = vec![shot(0.0, 0.0, 0, 0.5), shot(a35, 0.0, 0, 1.0), shot(-a35, 0.0, 0, 1.0), shot(a70, 0.0, 0, 2.0), shot(-a70, 0.0, 0, 2.0)];
        return BlizzardPlan { elements, shots, counts: true };
    }
    if k10 == 2 {
        let shots = vec![shot(0.0, 0.0, 0, 1.0), shot(0.0, -0.96, 0, 1.0), shot(0.0, 0.96, 0, 1.0), shot(a45, 0.4, 0, 1.0), shot(-a45, 0.4, 0, 1.0), shot(a45, -0.4, 0, 1.0), shot(-a45, -0.4, 0, 1.0)];
        return BlizzardPlan { elements, shots, counts: true };
    }
    let mut shots = vec![shot(0.0, 0.0, 1, 1.0)];
    if let Some(&second) = elements.get(1) {
        shots.push(BlizzardShot { elem: second, ..shot(0.0, 0.0, 1, 1.0) });
    }
    let counts = elements.len() == 1;
    BlizzardPlan { elements, shots, counts }
}

/// Host 0.13 (FUN_0016cf08 -> FUN_0017c2e8 with the Y rotation of FUN_0011b4d8): a vector turned about
/// Y in the game's space, `x' = x cos + z sin`, `z' = z cos - x sin` [inferred: FUN_002fb628 read as
/// libvu0's Y rotation matrix]. Input and output in Bevy space (the game's Y and Z negated).
fn rotate_y_game(v: Vec3, angle: f32) -> Vec3 {
    let (sn, cs) = angle.sin_cos();
    let (x, z) = (v.x, -v.z);
    let (x2, z2) = (x * cs + z * sn, z * cs - x * sn);
    Vec3::new(x2, v.y, -z2)
}

/// THUNDER_*.mag R0017: the strikes of a cast, in order, as (frames to wait after it, place, probe
/// height). The main place is the target record's position with 100 (host 1.120) or, without a target,
/// 300 ahead of the caster (Master: 800 ahead turned 45 degrees) raised by 80, with 330. Master first
/// strikes at the caster's position and 0.47 and 0.78 of the way to the place, 16 frames apart, then
/// the place. The Wisdom finisher strikes three more times 8 frames apart around the place, `magi`
/// +0x24 * 1.5 away along the facing turned by 120, 240 and 360 degrees. Final strikes 2 (finisher 7)
/// more times 8 frames apart; those entries carry `true`: their place is host 1.248's scatter around
/// the place, drawn when the strike starts. Positions in Bevy space.
fn thunder_plan(kind: u32, here: Vec3, facing: Vec3, target: Option<Vec3>, radius: f32) -> Vec<(f32, Vec3, f32, bool)> {
    let (k10, fin) = (kind / 10, kind & 1 == 1);
    let (place, rise) = match target {
        Some(t) => (t, 100.0),
        None => {
            let ahead = if k10 == 2 { rotate_y_game(facing, std::f32::consts::FRAC_PI_4) * 800.0 } else { facing * 300.0 };
            (here + ahead * SCALE + Vec3::Y * 80.0 * SCALE, 330.0)
        }
    };
    let mut out = Vec::new();
    if k10 == 2 {
        for f in [0.0, 0.47, 0.78] {
            out.push((16.0, here + (place - here) * f, rise, false));
        }
    }
    out.push((0.0, place, rise, false));
    if k10 == 1 && fin {
        out.last_mut().unwrap().0 = 8.0;
        let mut dir = rotate_y_game(facing * radius * 1.5, 2.094_395);
        for i in 0..3 {
            out.push((if i < 2 { 8.0 } else { 0.0 }, place + dir * SCALE, 330.0, false));
            dir = rotate_y_game(dir, 2.094_395);
        }
    }
    // Final (R026a, L014c): 2 more strikes, 7 for a finisher (R0276), each after a wait of 8 frames,
    // at host 1.248's scatter around the place (`magic_rules::scatter_place`), probe height 330.
    if k10 == 3 {
        let n = if fin { 8 } else { 3 };
        out.last_mut().unwrap().0 = 8.0;
        for i in 1..n {
            out.push((if i < n - 1 { 8.0 } else { 0.0 }, place, 330.0, true));
        }
    }
    out
}

/// The first slice of event 21 of the spell's script (after the MP is paid).
#[allow(clippy::too_many_arguments)]
fn event21(
    commands: &mut Commands,
    c: &Cast,
    data: &MagicData,
    game: &GameData,
    s: &mut Sora,
    me: Entity,
    body: Entity,
    frames: &Frames,
    airborne: bool,
    running: &mut Running,
    bliz_counter: &mut u32,
    bliz_plan: &mut Option<BlizzardPlan>,
) -> Step {
    let Some(pax) = c.pax.as_ref() else { return Step::Done };
    let start = |commands: &mut Commands, id: u16| fx::start(commands, pax, id, Bind::Owner(body));
    match c.slot {
        0 => {
            let plan = fire_plan(c.kind, airborne);
            if plan.stop_charge
                && let Some(id) = c.charge_fx
            {
                stop_effect(running, pax, id, body);
            }
            for &id in &plan.starts {
                start(commands, id);
            }
            mlog!("magic: Fire event 21 (kind {}): elements {:?} (airborne {airborne}), attack follows {} for {} frames", c.kind, plan.starts, plan.ball, c.magi.f(MAGI_TIME));
            Step::Fire { ball: plan.ball, ball2: plan.ball2, attacks: None, t: c.magi.f(MAGI_TIME), yielded: false }
        }
        3 => {
            // CURE: stop the charge effect, element 1 on the caster, element 2 at the target's centre,
            // then every party member in range of the centre is healed (atkp 708 on the target).
            if let Some(id) = c.charge_fx {
                stop_effect(running, pax, id, body);
            }
            start(commands, 1);
            // The target is the caster (the app has no command target record); the centre is its
            // bone 0x4001 (host 1.83 -> FUN_0016aa18: the centre of its body shape).
            let centre = frames[1].w_axis.truncate();
            fx::start_placed(commands, pax, 2, Bind::Fixed(Mat4::from_translation(centre)), Placement { replace_translation: true, ..default() });
            let me_pos = s.motor.pos * SCALE;
            let dist = (me_pos - centre).with_y(0.0).length() / SCALE;
            let range = c.magi.f(MAGI_CURE_RANGE);
            if s.hp > 0 && dist - range < 0.0 {
                start(commands, 3);
                let row = rules::attack_row(&game.atkp, ATKP_CURE_SELF, c.kind).map(|i| &game.atkp[i]);
                match row {
                    Some(r) => {
                        let heal = rules::heal_amount(r, s.stats.strength, s.stats.magic);
                        let before = s.hp;
                        s.hp = rules::healed(s.hp, s.stats.max_hp, heal);
                        mlog!("magic: Cure heals Sora: atkp {} SubId {} Power {} x magic {} -> {heal}; HP {before} -> {} (range {range}, distance {dist:.0})", r.id, r.sub_id, r.power, s.stats.magic, s.hp);
                    }
                    None => warn!("magic: no atkp row {ATKP_CURE_SELF} for kind {}", c.kind),
                }
            }
            let _ = (me, data);
            mlog!("magic: Cure: no other party member to heal (atkp {ATKP_CURE_OTHER} for them)");
            Step::Done
        }
        1 => {
            // BLIZZARD (R0017): R0263 stops the charge effect and starts the plan's elements (1, or 7,
            // or 1 and 7 for Final); the shots follow after a yield (`Step::Blizzard`).
            if let Some(id) = c.charge_fx {
                stop_effect(running, pax, id, body);
            }
            let plan = blizzard_plan(c.kind, *bliz_counter);
            for &id in &plan.elements {
                start(commands, id);
            }
            if plan.counts {
                *bliz_counter += 1;
            }
            mlog!("magic: Blizzard event 21 (kind {}): elements {:?}, {} shots", c.kind, plan.elements, plan.shots.len());
            let first = plan.elements[0];
            *bliz_plan = Some(plan);
            Step::Blizzard { elem: first, yielded: false }
        }
        2 => {
            // THUNDER (R0017): R0226(1) stops the charge effect and starts element 1; the strikes are
            // placed in the same slice (`Step::Thunder`, `thunder_plan`).
            if let Some(id) = c.charge_fx {
                stop_effect(running, pax, id, body);
            }
            start(commands, 1);
            Step::Thunder
        }
        _ => Step::Done,
    }
}

/// Host 1.88 on the caster (FUN_00172f40 -> FUN_0016b6e0 -> FUN_0018eec0 on a target record of shape
/// 0): the centre of the object's first target shape (FUN_0018ee20 -> FUN_001894d0), else its position
/// (FUN_0016b960). Bevy space; `None` for the transform gives the origin.
fn caster_lock_point(body: &Animator, gt: Option<&GlobalTransform>) -> Vec3 {
    let Some(gt) = gt else { return Vec3::ZERO };
    body.rig.model.shapes.iter().find(|sh| sh.kind == shape_kind::TARGET).map_or(gt.translation(), |sh| combat::hurt_spheroid(sh, body, gt).center)
}

/// The target points the shots steer toward: each enemy's first target shape (FUN_001894d0's spheroid
/// centre), by root entity [inferred: host 1.120 reads the target record's position].
fn victim_targets(fighters: &Query<(Entity, &mut Fighter, &GlobalTransform)>, anims: &Query<&mut Animator>, gts: &Query<&GlobalTransform>) -> Vec<(Entity, Vec3)> {
    fighters
        .iter()
        .filter(|(_, f, _)| f.team == Team::Enemy)
        .filter_map(|(e, f, g)| {
            let (a, bg) = (anims.get(f.hurt_body).ok()?, gts.get(f.hurt_body).ok()?);
            let c = a.rig.model.shapes.iter().find(|sh| sh.kind == shape_kind::TARGET).map(|sh| combat::hurt_spheroid(sh, a, bg).center);
            Some((e, c.unwrap_or(g.translation())))
        })
        .collect()
}

/// Rotates `from` toward `to` by at most `rate` radians (FUN_0017c0f8: the angle between the unit
/// vectors, clamped to the rate times the frame step, about their cross product). Unit result.
pub fn turn_toward(from: Vec3, to: Vec3, rate: f32) -> Vec3 {
    let (a, b) = (from.normalize_or_zero(), to.normalize_or_zero());
    if a == Vec3::ZERO || b == Vec3::ZERO {
        return a;
    }
    let angle = a.dot(b).clamp(-1.0, 1.0).acos();
    let step = angle.clamp(-rate, rate);
    let axis = a.cross(b).normalize_or_zero();
    if axis == Vec3::ZERO {
        return a;
    }
    Quat::from_axis_angle(axis, step) * a
}

/// One frame of a Blizzard shot (BLIZZARD_*.mag routine 0x319). First slice (R035b): element 2 of the
/// spell's file, unbound; the attack `atkp` 248 of the cast's kind sized `magi` +0x18 both ways; life
/// `magi` +0x10 - 12. Each frame (L039d): the shot ends when its effect is gone or by `shot_ends` (a
/// wall; a target only for a shot with flag 0); otherwise the effect and the attack move to the position; after the yield the
/// direction turns toward the target by `magi` +0x60 of the block (`magi` +0xc times the shot's scale)
/// when there is one (host 0.46), the position moves `magi` +0x14 along it (host 0.45), and when the
/// life runs out the effect fades over 12 frames and goes (host 0.52). Cleanup (R0487): the effect is
/// destroyed and the attack finished.
#[allow(clippy::too_many_arguments)]
fn step_shot(
    shot: &mut Shot,
    commands: &mut Commands,
    game: &GameData,
    attacks: &mut Vec<SpellAttack>,
    next: &mut u32,
    running: &mut Running,
    targets: &[(Entity, Vec3)],
    claimed: &[Entity],
) {
    let place = |p: Vec3, d: Vec3| Mat4::from_rotation_translation(Quat::from_rotation_y(d.x.atan2(d.z)), p);
    let Some(ai) = shot.attack else {
        // R035b.
        fx::start_placed(commands, &shot.pax, 2, Bind::Fixed(place(shot.pos, shot.dir)), Placement { replace_translation: true, ..default() });
        let size = shot.magi.f(MAGI_SHOT_SIZE);
        shot.attack = new_attack(attacks, next, game, &shot.pax, ATKP_BLIZZARD, shot.kind, size, 0.0);
        let Some(ai) = shot.attack else {
            warn!("magic: no atkp row {ATKP_BLIZZARD} for kind {}", shot.kind);
            shot.done = true;
            return;
        };
        mlog!("magic: Blizzard attack {} made: atkp {ATKP_BLIZZARD} kind {} size {size}", attacks[ai].group, shot.kind);
        // R035b moves the attack to the caster's lock-on point first (host 1.88); the first loop pass moves
        // it to the shot's place, so the first swept segment runs from the caster to the shot.
        move_attack(&mut attacks[ai], shot.origin);
        return;
    };
    // The effect instance exists from the frame after its start request: the shot takes an unbound
    // element 2 of the file that no other shot holds [app bookkeeping for the handle].
    if shot.effect.is_none() {
        shot.effect = running
            .iter()
            .filter(|(e, r)| r.lib.id == shot.pax.id && r.game_id == 2 && matches!(r.bind, Bind::Fixed(_)) && !claimed.contains(e))
            .map(|(e, _)| e)
            .max();
    }
    // After the yield: steer, move, count the life down.
    let rate = shot.magi.f(MAGI_TURN) * shot.turn_scale;
    if rate > 0.0
        && let Some(t) = shot.target.and_then(|t| targets.iter().find(|x| x.0 == t))
    {
        shot.dir = turn_toward(shot.dir, t.1 - shot.pos, rate);
    }
    shot.pos += shot.dir.normalize_or_zero() * shot.magi.f(MAGI_SPEED) * SCALE;
    let mut effect = shot.effect.and_then(|e| running.get_mut(e).ok());
    match shot.fade.as_mut() {
        Some(f) => *f = f.saturating_sub(1),
        None => {
            shot.life -= 1.0;
            if shot.life <= 0.0 {
                // Host 0.52 (fade over 12 frames, then destroy): the app ends the effect's loops [inferred].
                shot.fade = Some(SHOT_FADE);
                if let Some((_, r)) = effect.as_mut() {
                    r.effect.end();
                }
            }
        }
    }
    // Loop top: the end conditions, then the effect and the attack follow the position.
    let a = &attacks[ai];
    if shot.fade == Some(0) || shot_ends(a.wall, false, a.hit_any, shot.flag) {
        mlog!("magic: Blizzard shot over at {:?} (hit {}, wall {}, flag {}, faded {})", shot.pos, a.hit_any, a.wall, shot.flag, shot.fade == Some(0));
        if let Some((_, r)) = effect.as_mut() {
            r.effect.end();
        }
        attacks[ai].live = false;
        shot.done = true;
        return;
    }
    if let Some((_, r)) = effect.as_mut() {
        r.bind = Bind::Fixed(place(shot.pos, shot.dir));
    }
    move_attack(&mut attacks[ai], shot.pos);
}

/// One frame of a Thunder strike (THUNDER_*.mag routine 0x2da, R031c). First slice: a ground probe
/// (host 1.141 -> FUN_001a5c30 with mask 4) from `rise` above the place, 10000 down; ground less than
/// `magi` +0x20 below the place moves the strike onto it with element 2 and atkp 250, otherwise it stays
/// with element 3 and atkp 251. The element is started there unbound. After `magi` +0x30 frames: the
/// attack (radius `magi` +0x24) at the place and its copy with atkp 249 (host 2.31; radius +0x2c,
/// height +0x28, raised by that height); both live `magi` +0x34 frames, then both are finished.
fn step_strike(k: &mut Strike, commands: &mut Commands, game: &GameData, attacks: &mut Vec<SpellAttack>, next: &mut u32, world: Option<&crate::world::World>) {
    if !k.started {
        k.started = true;
        let (mut elem, mut atkp) = (3u16, ATKP_THUNDER_AIR);
        if let Some(gy) = world.and_then(|w| w.ground_m(k.pos.x, k.pos.z, k.pos.y + k.rise * SCALE)) {
            let below = (k.pos.y - gy) / SCALE;
            if below - k.magi.f(MAGI_THUNDER_SNAP) < 0.0 {
                elem = 2;
                atkp = ATKP_THUNDER_GROUND;
                k.pos.y = gy;
            }
        }
        mlog!("magic: Thunder strike: element {elem} at {:?}, atkp {atkp} after {} frames", k.pos, k.magi.f(MAGI_THUNDER_WAIT));
        fx::start_placed(commands, &k.pax, elem, Bind::Fixed(Mat4::from_translation(k.pos)), Placement { replace_translation: true, ..default() });
        k.wait = k.magi.f(MAGI_THUNDER_WAIT);
        k.atkp = atkp;
    }
    if k.wait > 0.0 {
        k.wait -= 1.0;
        return;
    }
    match k.attacks {
        None => {
            let atkp = k.atkp;
            let Some(a1) = new_attack(attacks, next, game, &k.pax, atkp, k.kind, k.magi.f(MAGI_THUNDER_RADIUS), 0.0) else {
                warn!("magic: no atkp row {atkp} for kind {}", k.kind);
                k.done = true;
                return;
            };
            move_attack(&mut attacks[a1], k.pos);
            let group = attacks[a1].group;
            let Some(a2) = new_attack(attacks, next, game, &k.pax, ATKP_THUNDER_COLUMN, k.kind, k.magi.f(MAGI_THUNDER_COLUMN_R), k.magi.f(MAGI_THUNDER_COLUMN_H)) else {
                k.done = true;
                return;
            };
            // Host 2.31 copies the attack: the copy keeps the original's group [inferred from
            // FUN_001a2c58 passing the original's +0x20].
            attacks[a2].group = group;
            let h = attacks[a2].height;
            move_attack(&mut attacks[a2], k.pos + Vec3::Y * h * SCALE);
            mlog!("magic: Thunder attacks {group}: atkp {atkp} radius {} and atkp {ATKP_THUNDER_COLUMN} radius {} height {h}, for {} frames", attacks[a1].radius, attacks[a2].radius, k.magi.f(MAGI_THUNDER_TIME));
            k.attacks = Some((a1, a2));
            k.wait = k.magi.f(MAGI_THUNDER_TIME);
            if k.wait > 0.0 {
                k.wait -= 1.0;
            }
        }
        Some((a1, a2)) => {
            attacks[a1].live = false;
            attacks[a2].live = false;
            mlog!("magic: Thunder strike over");
            k.done = true;
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn new_attack(attacks: &mut Vec<SpellAttack>, next: &mut u32, game: &GameData, pax: &Arc<LoadedPax>, id: u16, kind: u32, radius: f32, height: f32) -> Option<usize> {
    let base_row = rules::attack_row(&game.atkp, id, kind)?;
    *next += 1;
    // FUN_001a30e0: the height is the radius unless it is positive.
    let height = if height > 0.0 { height } else { radius };
    attacks.push(SpellAttack { group: *next, base_row, pos: None, prev: None, radius, height, live: true, hit_any: false, wall: false, clashes: 0, pax: Some(pax.clone()) });
    Some(attacks.len() - 1)
}

/// Host 2.12 (FUN_001a2fc8): the previous position is kept for the swept test.
fn move_attack(a: &mut SpellAttack, p: Vec3) {
    a.prev = a.pos.or(Some(p));
    a.pos = Some(p);
}

/// The points FUN_001a2cc0 tests: the current position, then, after a move of more than two units,
/// steps of `min(radius, height) / 2`... toward the previous position.
fn sweep_points(a: &SpellAttack, cur: Vec3) -> Vec<Vec3> {
    let mut out = vec![cur];
    let Some(prev) = a.prev else { return out };
    let d = (prev - cur) / SCALE;
    let len = d.length();
    let step = a.radius.min(a.height);
    if len <= SWEEP_MIN || step <= 0.0 {
        return out;
    }
    let dir = d / len;
    let mut left = len;
    let mut p = cur / SCALE;
    while left > step * SWEEP_STEP {
        p += dir * step;
        left -= step;
        out.push(p * SCALE);
    }
    out
}

/// An attack volume (FUN_00189610): radius and half height around the centre; as the app's
/// capsules, a vertical segment of `height - radius` either side [inferred shape].
fn attack_volume(c: Vec3, radius: f32, height: f32) -> Volume {
    let half = (height - radius).max(0.0) * SCALE;
    Volume { a: c - Vec3::Y * half, b: c + Vec3::Y * half, r: radius * SCALE }
}

// ---- the spell scripts in the interpreter (Magnet, Reflect; the others with `KH2_MAGIC_VM=1`) ----------

/// What the spell script reads of the caster: a 16-byte block whose word +4 is the object (the script
/// copies the first 16 bytes of the object it is given into its context, R0283, and passes the context
/// to the host functions that take an object). The address is the app's own.
const VM_CASTER: u32 = 0x0a00_0000;
/// The caster's object handle in the interpreter.
const VM_OBJ: u32 = 1;
/// The slot's resource record (FUN_001a4798 passes `table + 0xc`; the script only hands it to 1.139).
const VM_PAX: u32 = 0x0a10_0000;
/// The `magi` row of the cast's kind (the script reads its floats with `ldf`).
const VM_MAGI: u32 = 0x0a20_0000;
/// Effect and attack handles given to the script.
const VM_EFFECT: u32 = 0x0b00_0000;
const VM_ATTACK: u32 = 0x0c00_0000;
/// The command's target record (host 2.23 answers it when the cast has a target).
const VM_TARGET: u32 = 0x0d00_0000;
/// The persistent blocks of host 2.21 (FUN_001a5040), in the order they were first asked for.
const VM_SHARED: u32 = 0x0e00_0000;
const VM_SHARED_STRIDE: u32 = 0x100;

/// An effect a spell script holds a handle to: an element of the spell's file bound to the caster
/// (host 1.139) or placed in the world (host 1.138, then 0.38 position and 0.39 scale).
#[derive(Clone)]
pub struct VmFx {
    id: u16,
    pax: Arc<LoadedPax>,
    fixed: bool,
    /// Bevy space; the scale of 0.39 (its first component).
    pos: Vec3,
    scale: f32,
    entity: Option<Entity>,
    /// The instance has been seen running; once it is gone again it has ended.
    seen: bool,
    gone: bool,
    stopped: bool,
}

impl VmFx {
    fn matrix(&self) -> Mat4 {
        // A zero scale (Magnet's growth starts at 0) would make the bind matrix singular.
        Mat4::from_scale_rotation_translation(Vec3::splat(self.scale.max(1e-3)), Quat::IDENTITY, self.pos)
    }
}

/// What the spell scripts share across casts: the effect handles (Reflect keeps one in its persistent
/// block from cast to cast) and the persistent blocks of host 2.21 by key.
#[derive(Default)]
pub struct VmWorld {
    fx: Vec<VmFx>,
    shared: Vec<(u32, Vec<u8>)>,
    /// The bits of the global flag word 0x35d268 the scripts hold (bit 3, hosts 1.187 / 1.188).
    gflags: u32,
}

impl VmWorld {
    /// Per tick, before the slices: an effect seen running and no longer there has ended (host 0.49).
    fn refresh(&mut self, running: &Running) {
        for f in self.fx.iter_mut() {
            if let Some(e) = f.entity {
                if running.get(e).is_ok() {
                    f.seen = true;
                } else if f.seen {
                    f.gone = true;
                }
            }
        }
    }
}

/// A cast whose script runs in the interpreter: the thread of FUN_001a4798 (event 20 at the cast, event
/// 21 from the cast trigger), the attacks (index in `MagicState::attacks`) it holds handles to, and the
/// host functions it called that the app does not answer.
struct VmCast {
    rt: kh2_bdx::Runtime,
    attacks: Vec<Option<usize>>,
    missing: std::collections::BTreeSet<String>,
    /// The slice of this tick has run.
    ran: bool,
}

impl VmCast {
    /// FUN_001a4798's thread: event 20 with (caster, resource record, kind, `magi` row).
    fn start(code: &[u8], table: Option<&kh2_bdx::exe::HostTable>, kind: u32) -> Option<Box<VmCast>> {
        let prog = kh2_bdx::Program::parse(code).ok()?;
        let mut rt = kh2_bdx::Runtime::new(prog);
        if let Some(t) = table {
            rt.set_host_table(t.clone());
        }
        if !rt.start_thread(VM_OBJ, 20, &[VM_CASTER, VM_PAX, kind, VM_MAGI]) {
            return None;
        }
        Some(Box::new(VmCast { rt, attacks: Vec::new(), missing: Default::default(), ran: false }))
    }
}

/// What a script asked for in its slice, carried out after it (they need the ECS). Effects are by
/// handle index in `VmWorld::fx`, with the element id for the logs and tests.
#[derive(Debug)]
enum VmReq {
    Effect { h: usize, id: u16 },
    Stop { h: usize, id: u16 },
    Destroy(usize),
    Place(usize),
    Attack { h: usize, atkp: u16, kind: u32, copy_of: Option<usize> },
    Size { h: usize, a: f32, b: f32 },
    Move { h: usize, at: Vec3 },
    Finish(usize),
    /// Hosts 1.187 / 1.188: bit 3 of the global flag word 0x35d268 set or cleared.
    GlobalFlag { on: bool },
}

/// The app's game space for the scripts: the game's axes (Y and Z negated) in its units.
fn to_game(v: Vec3) -> [f32; 4] {
    let g = v / SCALE;
    [g.x, -g.y, -g.z, 1.0]
}

fn from_game(v: [f32; 4]) -> Vec3 {
    Vec3::new(v[0], -v[1], -v[2]) * SCALE
}

/// The host functions of the six spell scripts, answered from the tick's state. Calls that change the
/// game are queued (`VmReq`). Anything else is recorded as missing and answers 0.
struct SpellHost<'a> {
    airborne: bool,
    /// Host 1.15: the caster's motion word (+0x154): 1 while the cast motion plays, 0 after.
    motion_state: i32,
    kind: u32,
    magi: &'a MagiRow,
    pax: &'a Arc<LoadedPax>,
    frames: &'a Frames,
    /// The command's target record's point (Bevy space), when the cast has a living target.
    target: Option<Vec3>,
    world: &'a mut VmWorld,
    /// The area's collision, for the ground probe of host 1.141 (`FUN_001a5c30` with mask 4).
    map: Option<&'a crate::world::World>,
    vm_attacks: &'a mut Vec<Option<usize>>,
    /// Byte +0xa4 of each attack handle (host 2.66): the clashes it had (FUN_0018d430).
    clashes: Vec<u8>,
    missing: &'a mut std::collections::BTreeSet<String>,
    reqs: Vec<VmReq>,
}

impl<'a> SpellHost<'a> {
    #[allow(clippy::too_many_arguments)]
    fn new(
        airborne: bool,
        in_motion: bool,
        kind: u32,
        magi: &'a MagiRow,
        pax: &'a Arc<LoadedPax>,
        frames: &'a Frames,
        target: Option<Vec3>,
        world: &'a mut VmWorld,
        map: Option<&'a crate::world::World>,
        vm: &'a mut VmCast,
        clashes: Vec<u8>,
    ) -> (SpellHost<'a>, &'a mut kh2_bdx::Runtime) {
        let host = SpellHost { airborne, motion_state: in_motion as i32, kind, magi, pax, frames, target, world, map, vm_attacks: &mut vm.attacks, clashes, missing: &mut vm.missing, reqs: Vec::new() };
        (host, &mut vm.rt)
    }

    /// Host 1.141 `probe_ground_down(point, out, distance)`: `FUN_001a5c30(distance, 0, point, out, 4)`, a
    /// collision probe from the point straight down by the distance against the polygons of mask 4
    /// (the floor the player falls onto). Game space in and out; `None` when nothing is below.
    fn probe_ground_down(&self, from: [f32; 4], down: f32) -> Option<[f32; 4]> {
        // Game space (cm, Y down, Z mirrored) to the collision world's (cm, Y up).
        let p = Vec3::new(from[0], -from[1], -from[2]);
        let g = self.map?.ground(Vec3::new(p.x, 0.0, p.z), p.y, down.max(0.0))?;
        Some([from[0], -g.y, from[2], 1.0])
    }

    fn effect(&self, h: u32) -> Option<usize> {
        h.checked_sub(VM_EFFECT).map(|i| i as usize).filter(|&i| i < self.world.fx.len())
    }

    fn attack(&self, h: u32) -> Option<usize> {
        h.checked_sub(VM_ATTACK).map(|i| i as usize).filter(|&i| i < self.vm_attacks.len())
    }

    fn new_attack(&mut self, atkp: u16, copy_of: Option<usize>) -> kh2_bdx::host::Value {
        let h = self.vm_attacks.len();
        self.vm_attacks.push(None);
        self.reqs.push(VmReq::Attack { h, atkp, kind: self.kind, copy_of });
        kh2_bdx::host::Value::Handle(VM_ATTACK + h as u32)
    }

    fn new_effect(&mut self, id: u16, fixed: bool) -> kh2_bdx::host::Value {
        let pos = self.frames[0].w_axis.truncate();
        let h = self.world.fx.len();
        self.world.fx.push(VmFx { id, pax: self.pax.clone(), fixed, pos, scale: 1.0, entity: None, seen: false, gone: false, stopped: false });
        self.reqs.push(VmReq::Effect { h, id });
        kh2_bdx::host::Value::Handle(VM_EFFECT + h as u32)
    }

    fn shared(&mut self, addr: u32) -> Option<(usize, usize)> {
        let o = addr.checked_sub(VM_SHARED)?;
        let (k, off) = ((o / VM_SHARED_STRIDE) as usize, (o % VM_SHARED_STRIDE) as usize);
        (k < self.world.shared.len()).then_some((k, off))
    }
}

impl kh2_bdx::host::Host for SpellHost<'_> {
    fn call(&mut self, c: &kh2_bdx::host::Call) -> kh2_bdx::host::Value {
        use kh2_bdx::host::{Arg, Call, Value};
        let raw = |a: &[Arg], k: usize| match a.get(k) {
            Some(Arg::Raw(v)) | Some(Arg::Obj(v)) => *v,
            Some(Arg::Int(v)) => *v as u32,
            _ => 0,
        };
        let float = |a: &[Arg], k: usize| match a.get(k) {
            Some(Arg::Float(f)) => *f,
            _ => 0.0,
        };
        match c {
            Call::Airborne { .. } => Value::Bool(self.airborne),
            // 1.60 (FUN_001729f8): the object is airborne (FUN_0019aa28), or its class flag 2 or flag
            // 0x40 of +0x160 is set; for Sora the app answers the first [inferred: the other two are
            // states the app's Sora does not have].
            Call::Busy { .. } => Value::Bool(self.airborne),
            Call::MotionState { .. } => Value::Int(self.motion_state),
            // 1.139 (FUN_00173988 -> FUN_001db868): element `id` of the resource's effect file on the
            // context's object. The mode word (1 or 0) is not told apart: the element is bound to the
            // caster as in the hand port [inferred: FUN_001db868's third argument was not traced].
            Call::Unknown { group: 1, index: 139, args } if args.len() >= 2 => self.new_effect(args[1] as u16, false),
            // 1.138 (FUN_00173948, not in the decompilation): element `id` started unbound; the scripts
            // place it next with 0.38 [inferred from the use: Thunder, Cure, Magnet].
            Call::Unknown { group: 1, index: 138, args } if args.len() >= 2 => self.new_effect(args[1] as u16, true),
            // 1.141 `probe_ground_down` (FUN_00172538 -> FUN_001a5c30 with mask 4): the runtime's own arm
            // decodes the point and the distance, writes the answer through the second argument (the
            // script's stack local, MAGNET_*.mag local+0x20) and returns 1 or 0; this host only answers
            // the point found (WP-F1 item 1; before it the call was answered "no ground").
            Call::Func { group: 1, index: 141, args } => {
                let from = match args.first() {
                    Some(Arg::Vec(v)) => *v,
                    _ => [0.0; 4],
                };
                match self.probe_ground_down(from, float(args, 2)) {
                    Some(p) => {
                        mlog!("magic(vm): 1.141 ground probe from ({:.0} {:.0} {:.0}) down {:.0}: floor at y {:.0} (game space)", from[0], from[1], from[2], float(args, 2), p[1]);
                        Value::Vec(p)
                    }
                    None => {
                        mlog!("magic(vm): 1.141 ground probe from ({:.0} {:.0} {:.0}) down {:.0}: no ground{}", from[0], from[1], from[2], float(args, 2), if self.map.is_none() { " (no collision world)" } else { "" });
                        Value::Int(0)
                    }
                }
            }
            // 1.187 / 1.188: bit 3 of the global flag word 0x35d268, set around Final's extra Magnet attack;
            // `GlobalFlags` carries it to the readers (FUN_001905e8: prizes and items go to the player at once).
            Call::Unknown { group: 1, index: idx @ (187 | 188), .. } | Call::Func { group: 1, index: idx @ (187 | 188), .. } => {
                self.reqs.push(VmReq::GlobalFlag { on: *idx == 187 });
                Value::None
            }
            Call::Unknown { group: 2, index: 31, args } if args.len() >= 2 => match self.attack(args[0]) {
                Some(src) => self.new_attack(args[1] as u16, Some(src)),
                None => Value::Int(0),
            },
            // 2.23 (FUN_0018ddb0): the command's target record; 1.140 its validity; 1.120 its position.
            Call::Target { .. } => Value::Handle(if self.target.is_some() { VM_TARGET } else { 0 }),
            Call::Func { group: 1, index: 140, args } => Value::Bool(raw(args, 0) == VM_TARGET && self.target.is_some()),
            Call::TargetPosition { .. } => Value::Vec(to_game(self.target.unwrap_or(self.frames[0].w_axis.truncate()))),
            Call::Func { group: 0, index: 50, args } => Value::Bool(self.effect(raw(args, 0)).is_some_and(|i| !self.world.fx[i].stopped && !self.world.fx[i].gone)),
            // 0.49 (FUN_001da0d8): the instance still runs.
            Call::Func { group: 0, index: 49, args } => Value::Bool(self.effect(raw(args, 0)).is_some_and(|i| !self.world.fx[i].gone)),
            Call::Func { group: 0, index: 44, args } => {
                if let Some(i) = self.effect(raw(args, 0)) {
                    self.world.fx[i].stopped = true;
                    self.reqs.push(VmReq::Stop { h: i, id: self.world.fx[i].id });
                }
                Value::None
            }
            // 0.51: the instance is destroyed at once.
            Call::Func { group: 0, index: 51, args } => {
                if let Some(i) = self.effect(raw(args, 0)) {
                    self.world.fx[i].gone = true;
                    self.reqs.push(VmReq::Destroy(i));
                }
                Value::None
            }
            // 0.38 (FUN_0016da90): the effect's position (FUN_001d9d00); 0.39 (FUN_0016dad0): the vector
            // after it (FUN_001d9d28), which Magnet sets to (s, s, s, 1): its scale [inferred].
            Call::Func { group: 0, index: 38 | 39, args } => {
                if let (Some(i), Some(Arg::Vec(v))) = (self.effect(raw(args, 0)), args.get(1)) {
                    match c {
                        Call::Func { index: 38, .. } => self.world.fx[i].pos = from_game(*v),
                        _ => self.world.fx[i].scale = v[0],
                    }
                    self.reqs.push(VmReq::Place(i));
                }
                Value::None
            }
            // 0.53 (FUN_001d9d00): the effect's position, as the hand port takes it.
            Call::Func { group: 0, index: 53, args } => {
                let p = match self.effect(raw(args, 0)).map(|i| &self.world.fx[i]) {
                    Some(f) if f.fixed => f.pos,
                    Some(f) => element_position(&f.pax, f.id, self.frames),
                    None => self.frames[0].w_axis.truncate(),
                };
                Value::Vec(to_game(p))
            }
            Call::Func { group: 0, index: 3, .. } => Value::Float(1.0),
            Call::Func { group: 2, index: 10, args } => self.new_attack(raw(args, 1) as u16, None),
            Call::Func { group: 2, index: 30, .. } => Value::None,
            Call::Func { group: 2, index: 11, args } => {
                if let Some(h) = self.attack(raw(args, 0)) {
                    self.reqs.push(VmReq::Size { h, a: float(args, 1), b: float(args, 2) });
                }
                Value::None
            }
            Call::Func { group: 2, index: 12, args } => {
                if let (Some(h), Some(Arg::Vec(v))) = (self.attack(raw(args, 0)), args.get(1)) {
                    self.reqs.push(VmReq::Move { h, at: from_game(*v) });
                }
                Value::None
            }
            Call::Func { group: 2, index: 13, args } => {
                if let Some(h) = self.attack(raw(args, 0)) {
                    self.reqs.push(VmReq::Finish(h));
                }
                Value::None
            }
            // 2.21 (FUN_0018e928 -> FUN_001a5040): the persistent block of a key; the first call makes
            // it zeroed, every call sets its first byte to 1.
            Call::Func { group: 2, index: 21, args } => {
                let (key, size) = (raw(args, 0), raw(args, 1) as usize);
                let k = match self.world.shared.iter().position(|(x, _)| *x == key) {
                    Some(k) => k,
                    None => {
                        self.world.shared.push((key, vec![0; (size + 1).clamp(16, VM_SHARED_STRIDE as usize)]));
                        self.world.shared.len() - 1
                    }
                };
                self.world.shared[k].1[0] = 1;
                Value::Handle(VM_SHARED + k as u32 * VM_SHARED_STRIDE)
            }
            // 2.66 (FUN_0018e818): byte +0xa4 of the attack, which FUN_0018d430 counts up per clash.
            Call::Func { group: 2, index: 66, args } => Value::Int(self.attack(raw(args, 0)).map_or(0, |h| self.clashes.get(h).copied().unwrap_or(0)) as i32),
            other => {
                let name = match other {
                    Call::Func { group, index, .. } | Call::Unknown { group, index, .. } => format!("{group}.{index}"),
                    c => format!("{c:?}").split([' ', '{', '(']).next().unwrap_or("?").to_string(),
                };
                self.missing.insert(name);
                Value::Int(0)
            }
        }
    }

    fn read_mem(&mut self, addr: u32, out: &mut [u8]) -> bool {
        if let Some((k, o)) = self.shared(addr) {
            return match self.world.shared[k].1.get(o..o + out.len()) {
                Some(b) => {
                    out.copy_from_slice(b);
                    true
                }
                None => false,
            };
        }
        let words: Vec<u32> = if (VM_CASTER..VM_CASTER + 16).contains(&addr) {
            vec![0, VM_OBJ, 0, 0]
        } else if (VM_MAGI..VM_MAGI + 4 * self.magi.0.len() as u32).contains(&addr) {
            self.magi.0.iter().map(|f| f.to_bits()).collect()
        } else {
            return false;
        };
        let base = if addr >= VM_MAGI { VM_MAGI } else { VM_CASTER };
        let bytes: Vec<u8> = words.iter().flat_map(|w| w.to_le_bytes()).collect();
        let o = (addr - base) as usize;
        match bytes.get(o..o + out.len()) {
            Some(b) => {
                out.copy_from_slice(b);
                true
            }
            None => false,
        }
    }

    fn write_mem(&mut self, addr: u32, data: &[u8]) -> bool {
        let Some((k, o)) = self.shared(addr) else { return false };
        match self.world.shared[k].1.get_mut(o..o + data.len()) {
            Some(b) => {
                b.copy_from_slice(data);
                true
            }
            None => false,
        }
    }
}

/// What carrying out a script's requests needs.
struct VmCtx<'a, 'w, 's, 'rw, 'rs> {
    commands: &'a mut Commands<'w, 's>,
    game: &'a GameData,
    frames: &'a Frames,
    running: &'a mut Running<'rw, 'rs>,
    attacks: &'a mut Vec<SpellAttack>,
    next: &'a mut u32,
    body: Entity,
    world: &'a mut VmWorld,
    /// The area's collision (host 1.141).
    map: Option<&'a crate::world::World>,
    /// The cast's target point (Bevy space).
    target: Option<Vec3>,
}

/// One slice of a cast's script (FUN_0016c680 from the magic task), then its requests: effects started
/// on the caster or in the world, moved, scaled, stopped or destroyed, attack objects made, sized
/// (FUN_001a30e0: the height is the second value when positive, else the radius), moved and finished.
/// The cast is done once the thread has ended; the host functions it called that the app does not
/// answer are logged then.
fn vm_slice(c: &mut Cast, ctx: &mut VmCtx, airborne: bool, in_motion: bool) {
    let (Some(vm), Some(pax)) = (c.vm.as_mut(), c.pax.clone()) else { return };
    vm.ran = true;
    let clashes: Vec<u8> = vm.attacks.iter().map(|a| a.map_or(0, |i| ctx.attacks[i].clashes)).collect();
    let (mut host, rt) = SpellHost::new(airborne, in_motion, c.kind, &c.magi, &pax, ctx.frames, ctx.target, ctx.world, ctx.map, vm, clashes);
    rt.update(&mut host);
    let reqs = std::mem::take(&mut host.reqs);
    for e in std::mem::take(&mut rt.core.errors) {
        warn!("magic(vm): script error at {:#x}: {:?}", e.0, e.1);
    }
    let mut pending: Vec<usize> = Vec::new();
    for r in reqs {
        match r {
            VmReq::Effect { h, id } => {
                let f = &mut ctx.world.fx[h];
                if f.fixed {
                    pending.push(h);
                } else {
                    f.entity = fx::start_placed(ctx.commands, &pax, id, Bind::Owner(ctx.body), Placement::default());
                }
                mlog!("magic(vm): effect {h}: element {id}{}", if f.fixed { " (in the world)" } else { " (on the caster)" });
            }
            VmReq::Stop { h, id } => {
                if let Some(mut r) = ctx.world.fx[h].entity.and_then(|e| ctx.running.get_mut(e).ok()) {
                    r.1.effect.end();
                }
                mlog!("magic(vm): effect {h} (element {id}) stopped");
            }
            VmReq::Destroy(h) => {
                pending.retain(|&p| p != h);
                if let Some(e) = ctx.world.fx[h].entity
                    && ctx.running.get(e).is_ok()
                {
                    ctx.commands.entity(e).despawn();
                }
                mlog!("magic(vm): effect {h} destroyed");
            }
            VmReq::Place(h) => {
                let f = &ctx.world.fx[h];
                if let Some(mut r) = f.entity.and_then(|e| ctx.running.get_mut(e).ok()) {
                    r.1.bind = Bind::Fixed(f.matrix());
                }
            }
            VmReq::Attack { h, atkp, kind, copy_of } => {
                let i = new_attack(ctx.attacks, ctx.next, ctx.game, &pax, atkp, kind, 0.0, 0.0);
                if let (Some(i), Some(src)) = (i, copy_of.and_then(|s| vm.attacks.get(s).copied().flatten())) {
                    // FUN_001a2c58: the copy keeps the original's group and size [inferred, as the hand port].
                    let (g, r, ht) = (ctx.attacks[src].group, ctx.attacks[src].radius, ctx.attacks[src].height);
                    let a = &mut ctx.attacks[i];
                    (a.group, a.radius, a.height) = (g, r, ht);
                }
                vm.attacks[h] = i;
                let what = match atkp {
                    ATKP_MAGNET => " (Magnet)",
                    ATKP_REFLECT_GUARD => " (Reflect's guard)",
                    _ => "",
                };
                mlog!("magic(vm): attack {:?} made: atkp {atkp}{what} kind {kind}", i.map(|i| ctx.attacks[i].group));
            }
            VmReq::Size { h, a, b } => {
                if let Some(i) = vm.attacks[h] {
                    ctx.attacks[i].radius = a;
                    ctx.attacks[i].height = if b > 0.0 { b } else { a };
                }
            }
            VmReq::Move { h, at } => {
                if let Some(i) = vm.attacks[h] {
                    move_attack(&mut ctx.attacks[i], at);
                }
            }
            VmReq::GlobalFlag { on } => {
                if on {
                    ctx.world.gflags |= rules::GFLAG_COLLECT_AT_ONCE;
                } else {
                    ctx.world.gflags &= !rules::GFLAG_COLLECT_AT_ONCE;
                }
                mlog!("magic(vm): global flag word bit 3 {}", if on { "set (1.187)" } else { "cleared (1.188)" });
            }
            VmReq::Finish(h) => {
                if let Some(i) = vm.attacks[h] {
                    ctx.attacks[i].live = false;
                    mlog!("magic(vm): attack {} over (clashes {})", ctx.attacks[i].group, ctx.attacks[i].clashes);
                }
            }
        }
    }
    // Elements placed in the world start where the slice left them (an instance only exists once the
    // frame's commands are applied, so later moves go to its bind).
    for h in pending {
        let f = &mut ctx.world.fx[h];
        f.entity = fx::start_placed(ctx.commands, &pax, f.id, Bind::Fixed(f.matrix()), Placement { replace_translation: true, ..default() });
    }
    if vm.rt.thread_done() {
        mlog!("magic(vm): script of slot {} ended; host functions not answered: {:?}", c.slot, vm.missing);
        c.step = Step::Done;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// One slice of a spell script with the app's host, outside the ECS (tests).
    #[allow(clippy::too_many_arguments)]
    fn test_slice(vm: &mut VmCast, world: &mut VmWorld, airborne: bool, in_motion: bool, kind: u32, magi: &MagiRow, pax: &Arc<LoadedPax>, frames: &Frames, target: Option<Vec3>, clashes: Vec<u8>) -> Vec<VmReq> {
        let (mut host, rt) = SpellHost::new(airborne, in_motion, kind, magi, pax, frames, target, world, None, vm, clashes);
        rt.update(&mut host);
        assert!(rt.core.errors.is_empty(), "{:?}", rt.core.errors);
        host.reqs
    }

    #[test]
    fn sweep_steps_back_toward_the_previous_position() {
        let a = SpellAttack { group: 1, base_row: 0, pos: None, prev: Some(Vec3::new(0.0, 0.0, 1.0)), radius: 20.0, height: 40.0, live: true, hit_any: false, wall: false, clashes: 0, pax: None };
        let pts = sweep_points(&a, Vec3::ZERO);
        // 100 units at steps of 20 while more than 10 is left: 5 extra points.
        assert_eq!(pts.len(), 6);
        assert!((pts[1].z - 0.2).abs() < 1e-5);
        let still = SpellAttack { prev: Some(Vec3::new(0.0, 0.0, 0.01)), ..a };
        assert_eq!(sweep_points(&still, Vec3::ZERO).len(), 1, "a move of 1 unit tests one place");
    }

    /// FUN_001a0240: a refill during the MP charge ends it and sets MP to the maximum; without a
    /// charge it only refills.
    #[test]
    fn refill_ends_the_mp_charge() {
        let mut c = Charge { left: 300.0, total: 600.0 };
        let mut mp = 0;
        assert!(refill_mp(Some(&mut c), &mut mp, 120));
        assert_eq!((mp, c.active(), c.left), (120, false, 0.0));
        let mut mp = 40;
        assert!(!refill_mp(Some(&mut Charge::default()), &mut mp, 120));
        assert_eq!(mp, 120);
        assert!(!refill_mp(None, &mut mp, 90));
        assert_eq!(mp, 90);
    }

    /// The disc's side of the magic turn (FUN_001a4798 -> FUN_00179470, FUN_00188030 cases 22 and 28):
    /// the Magic Lock-On bit belongs to an ability row; Thunder's ground cast motion turns toward the
    /// command's target (trigger 22), Blizzard's toward the lock-on target (28), each with a limit in
    /// degrees per frame; Fire's ground motion has no turn trigger.
    #[test]
    fn magic_turn_data_from_the_disc() {
        let Ok(iso) = crate::assets::iso_path() else { return };
        if !iso.exists() {
            return;
        }
        let disc = Disc::open(&iso);
        let (rules_ab, magc) = {
            let mut a = disc.0.lock().unwrap();
            (crate::abilities::AbilityRules::load(&mut a).expect("abilities"), kh2_tables::load::<Magc>(&mut a, &t::MAGC).unwrap().rows)
        };
        let owner = rules_ab.id_of_bit(rules::BIT_MAGIC_LOCK_ON);
        eprintln!("Magic Lock-On bit {} owned by item {owner:?}", rules::BIT_MAGIC_LOCK_ON);
        assert!(owner.is_some(), "an ability row sets the bit");
        let mset = kh2_anim::Mset::parse(&disc.read("obj/P_EX100.mset").unwrap()).unwrap();
        let turns = |id: u8| {
            let row = rules::pick_row(&magc, id, 1, 0).unwrap();
            let slot = mset.slot(row.ground_motion as usize, PlayerState::BattleWeapon).expect("motion");
            let m = mset.get(slot).unwrap();
            m.triggers.ranges.iter().filter(|r| matches!(r.id, rules::TRIGGER_TURN_TARGET | rules::TRIGGER_TURN_LOCK)).map(|r| (r.id, r.start, r.params.first().copied())).collect::<Vec<_>>()
        };
        // No cast motion of the four spells carries a range trigger that arms the caster against hits
        // (FUN_00188030: 36 sets a bit of the status mask at +0xac0, 37 one of +0xac4 that masks the
        // conversion of FUN_001bff00; 27 is the app's name for an invincible range): a hit that
        // flinches Sora during a cast ends it before its trigger (event 20 stops when the motion
        // changes), so the spell costs nothing and does nothing.
        for id in 0..4u8 {
            let row = rules::pick_row(&magc, id, 1, 0).unwrap();
            for mo in [row.ground_motion, row.air_motion] {
                let slot = mset.slot(mo as usize, PlayerState::BattleWeapon).expect("motion");
                let m = mset.get(slot).unwrap();
                assert!(!m.triggers.ranges.iter().any(|r| matches!(r.id, 27 | 36 | 37)), "spell {id} motion {mo} has an armor range");
            }
        }
        let (fire, blizzard, thunder) = (turns(0), turns(1), turns(2));
        eprintln!("turn triggers: Fire {fire:?}, Blizzard {blizzard:?}, Thunder {thunder:?}");
        assert!(fire.is_empty());
        assert!(blizzard.iter().any(|t| t.0 == rules::TRIGGER_TURN_LOCK && t.2.is_some_and(|p| p > 0)));
        assert!(thunder.iter().any(|t| t.0 == rules::TRIGGER_TURN_TARGET && t.2.is_some_and(|p| p > 0)));
    }

    /// Fire's own script (FIRE_1.mag) in the interpreter with the app's host (`KH2_MAGIC_VM=1`) asks for
    /// what the hand port does: event 20 starts element 0 and waits while the motion plays; event 21
    /// (after the cast trigger) starts element 2 (on the ground) and yields; then attack 502 of the
    /// cast's kind, sized `magi` +0 / +4, moved to the element's position for `magi` +8 frames, one per
    /// slice, then finished; the thread ends.
    #[test]
    fn fire_script_in_the_interpreter_matches_the_hand_port() {
        let Ok(iso) = crate::assets::iso_path() else { return };
        if !iso.exists() {
            return;
        }
        let disc = Disc::open(&iso);
        let file = disc.read("magic/FIRE_1.mag").expect("FIRE_1.mag");
        let code = kh2_bar::parse(&file).unwrap().into_iter().find(|e| e.kind == 3).expect("script").data.to_vec();
        let table = disc.read_root("SLUS_210.05").and_then(|exe| kh2_bdx::exe::HostTable::from_exe(&exe));
        let pax = Arc::new(fx::load(&disc, "magic/FIRE_1.mag", 100, None).expect("pax"));
        let f = disc.read("03system.bin").unwrap();
        let top = kh2_bar::parse(&f).unwrap();
        let inner = kh2_bar::parse(kh2_bar::find(&top, "pref").unwrap().data).unwrap();
        let magi = rules::parse_magi(kh2_bar::find(&inner, "magi").unwrap().data);
        let row = rules::magi_row(&magi, 0).unwrap().clone();
        let frames: Frames = [Mat4::IDENTITY; 10];
        let mut vm = VmCast::start(&code, table.as_ref(), 0).expect("event 20");
        let mut world = VmWorld::default();
        let mut slice = |vm: &mut VmCast, in_motion: bool| test_slice(vm, &mut world, false, in_motion, 0, &row, &pax, &frames, None, Vec::new());
        let ev20: Vec<VmReq> = (0..3).flat_map(|_| slice(&mut vm, true)).collect();
        assert!(matches!(ev20[..], [VmReq::Effect { id: 0, .. }]), "event 20 starts element 0 and waits");
        assert!(!vm.rt.thread_done());
        vm.rt.restart_thread(21, &[]);
        let first = slice(&mut vm, true);
        // R017d (every kind but Final's): the charge effect stops, element 1 starts; then the ball.
        assert!(matches!(first[..], [VmReq::Stop { id: 0, .. }, VmReq::Effect { id: 1, .. }, VmReq::Effect { id: 2, .. }]), "event 21: {first:?}");
        let second = slice(&mut vm, true);
        assert!(matches!(second[0], VmReq::Attack { atkp: ATKP_FIRE, kind: 0, copy_of: None, .. }));
        assert!(matches!(second[1], VmReq::Size { a, b, .. } if a == row.f(MAGI_RADIUS) && b == row.f(MAGI_HEIGHT)));
        let mut moves = second.iter().filter(|r| matches!(r, VmReq::Move { .. })).count();
        let mut finished = false;
        for _ in 0..200 {
            let r = slice(&mut vm, true);
            moves += r.iter().filter(|r| matches!(r, VmReq::Move { .. })).count();
            if r.iter().any(|r| matches!(r, VmReq::Finish(_))) {
                finished = true;
                break;
            }
        }
        assert!(finished);
        assert_eq!(moves as f32, row.f(MAGI_TIME), "one move per frame of the attack time");
        assert!(vm.missing.is_empty(), "every call answered: {:?}", vm.missing);
        let _ = slice(&mut vm, true);
        assert!(vm.rt.thread_done());
    }

    /// The disc's script, the `magi` rows and the effect file of a slot at level 1, for the VM tests.
    fn spell_setup(disc: &Disc, file: &str) -> (Vec<u8>, Option<kh2_bdx::exe::HostTable>, Arc<LoadedPax>, Vec<MagiRow>) {
        let code = kh2_bar::parse(&disc.read(file).unwrap()).unwrap().into_iter().find(|e| e.kind == 3).unwrap().data.to_vec();
        let table = disc.read_root("SLUS_210.05").and_then(|exe| kh2_bdx::exe::HostTable::from_exe(&exe));
        let pax = Arc::new(fx::load(disc, file, 100, None).expect("pax"));
        let f = disc.read("03system.bin").unwrap();
        let top = kh2_bar::parse(&f).unwrap();
        let inner = kh2_bar::parse(kh2_bar::find(&top, "pref").unwrap().data).unwrap();
        (code, table, pax, rules::parse_magi(kh2_bar::find(&inner, "magi").unwrap().data))
    }

    /// WP-7I: Magnet's own script (MAGNET_1.mag) with the app's host. Event 20 starts element 0 on the
    /// caster; event 21 stops it, starts element 1, and after a yield makes attack 658 of the cast's kind
    /// at the target (a sphere of `magi` +0x4c) with element 2 placed there; 16 frames later element 3
    /// grows over `magi` +0x40 frames (0.39 scale), holds `magi` +0x44 frames, then the attack is
    /// finished, element 4 starts, element 3 shrinks over `magi` +0x48 frames and is destroyed; the
    /// thread ends. Only the ground probe 1.141 is unanswered.
    #[test]
    fn magnet_script_runs_in_the_interpreter() {
        let Ok(iso) = crate::assets::iso_path() else { return };
        if !iso.exists() {
            return;
        }
        let disc = Disc::open(&iso);
        let (code, table, pax, magi) = spell_setup(&disc, "magic/MAGNET_1.mag");
        let row = rules::magi_row(&magi, 0).unwrap().clone();
        let frames: Frames = [Mat4::IDENTITY; 10];
        let target = Some(Vec3::new(3.0, 1.0, 0.0));
        let mut world = VmWorld::default();
        let mut vm = VmCast::start(&code, table.as_ref(), 0).expect("event 20");
        let ev20: Vec<VmReq> = (0..3).flat_map(|_| test_slice(&mut vm, &mut world, false, true, 0, &row, &pax, &frames, target, Vec::new())).collect();
        assert!(matches!(ev20[..], [VmReq::Effect { id: 0, .. }]), "{ev20:?}");
        vm.rt.restart_thread(21, &[]);
        let mut log: Vec<(usize, VmReq)> = Vec::new();
        // Element 3's scale (host 0.39, named in WP-7M2) after each slice.
        let mut scales: Vec<f32> = Vec::new();
        for n in 0..2000 {
            let clashes = vec![0; vm.attacks.len()];
            log.extend(test_slice(&mut vm, &mut world, false, true, 0, &row, &pax, &frames, target, clashes).into_iter().map(|r| (n, r)));
            if let Some(f) = world.fx.get(3)
                && !f.gone
            {
                scales.push(f.scale);
            }
            if vm.rt.thread_done() {
                break;
            }
        }
        assert!(vm.rt.thread_done(), "the script ends");
        let at = |f: &dyn Fn(&VmReq) -> bool| log.iter().find(|(_, r)| f(r)).map(|(n, _)| *n);
        assert_eq!(at(&|r| matches!(r, VmReq::Stop { id: 0, .. })), Some(0));
        assert_eq!(at(&|r| matches!(r, VmReq::Effect { id: 1, .. })), Some(0));
        let made = at(&|r| matches!(r, VmReq::Attack { atkp: ATKP_MAGNET, kind: 0, .. })).expect("attack 658");
        assert_eq!(made, 1, "after the yield");
        assert!(log.iter().any(|(_, r)| matches!(r, VmReq::Size { a, b, .. } if *a == row.f(0x4c) && *b == 0.0)));
        assert!(log.iter().any(|(_, r)| matches!(r, VmReq::Move { at, .. } if (*at - target.unwrap()).length() < 1e-3)), "the attack is at the target");
        let e2 = at(&|r| matches!(r, VmReq::Effect { id: 2, .. })).unwrap();
        let e3 = at(&|r| matches!(r, VmReq::Effect { id: 3, .. })).unwrap();
        assert!(world.fx[2].fixed && world.fx[3].fixed && (world.fx[2].pos - target.unwrap()).length() < 1e-3);
        assert_eq!(e3 - e2, 16, "16 frames between the attack and the sphere");
        let fin = at(&|r| matches!(r, VmReq::Finish(_))).unwrap();
        let grow = row.f(0x40).ceil() as usize;
        eprintln!("magnet: attack at {made}, sphere at {e3}, finished at {fin}; grow {grow} hold {} shrink {}", row.f(0x44), row.f(0x48));
        assert!(fin >= e3 + grow + row.f(0x44) as usize, "the pull lasts the growth and the hold");
        assert!(at(&|r| matches!(r, VmReq::Effect { id: 4, .. })).is_some_and(|n| n >= fin));
        assert!(at(&|r| matches!(r, VmReq::Destroy(3))).is_some());
        assert_eq!(vm.missing.iter().filter(|m| !m.starts_with("1.141")).count(), 0, "{:?}", vm.missing);
        // The sphere grows from 0 to 1 and shrinks back (`(sin((t - 0.5) pi) + 1) / 2`).
        let max = scales.iter().copied().fold(0.0f32, f32::max);
        eprintln!("magnet sphere scales: {} samples, first {:?}, max {max}, last {:?}", scales.len(), scales.first(), scales.last());
        assert!(scales.first().is_some_and(|s| *s < 0.2), "{scales:?}");
        assert!((max - 1.0).abs() < 0.02, "{scales:?}");
        assert!(scales.last().is_some_and(|s| *s < 0.2), "{scales:?}");
    }

    /// WP-7I: Reflect's own script (REFLECT_1.mag). Event 20 starts no charge effect; event 21 starts the
    /// barrier (element 0 on the ground, 2 in the air), keeps it in the persistent block of key 0x10002,
    /// and after a yield makes the guard (atkp 573, a sphere of `magi` +0x50) that follows the barrier
    /// while it lives; a clash (byte +0xa4 rising) starts element 1. When the barrier has ended the clash
    /// count picks the counter-burst: element 4 and atkp 574 then 667 (sizes `magi` +0x5c / +0x60, the
    /// second 20 larger) for one or two clashes below `magi` +0x64; no clash, no burst.
    #[test]
    fn reflect_script_runs_in_the_interpreter() {
        let Ok(iso) = crate::assets::iso_path() else { return };
        if !iso.exists() {
            return;
        }
        let disc = Disc::open(&iso);
        let (code, table, pax, magi) = spell_setup(&disc, "magic/REFLECT_1.mag");
        let row = rules::magi_row(&magi, 0).unwrap().clone();
        let frames: Frames = [Mat4::IDENTITY; 10];
        for (airborne, clash) in [(false, 1u8), (true, 0)] {
            let mut world = VmWorld::default();
            let mut vm = VmCast::start(&code, table.as_ref(), 0).expect("event 20");
            let ev20: Vec<VmReq> = (0..3).flat_map(|_| test_slice(&mut vm, &mut world, airborne, true, 0, &row, &pax, &frames, None, Vec::new())).collect();
            assert!(ev20.is_empty(), "no charge effect: {ev20:?}");
            vm.rt.restart_thread(21, &[]);
            let mut log: Vec<VmReq> = Vec::new();
            for n in 0..400 {
                // The barrier ends after 30 frames; the guard clashed `clash` times meanwhile.
                if n == 30 {
                    world.fx[0].gone = true;
                }
                let clashes = vec![if n > 5 { clash } else { 0 }; vm.attacks.len()];
                log.extend(test_slice(&mut vm, &mut world, airborne, true, 0, &row, &pax, &frames, None, clashes));
                if vm.rt.thread_done() {
                    break;
                }
            }
            assert!(vm.rt.thread_done());
            eprintln!("reflect airborne {airborne} clash {clash}: {:?}", log.iter().filter(|r| !matches!(r, VmReq::Move { .. })).collect::<Vec<_>>());
            assert!(matches!(log[0], VmReq::Effect { id, .. } if id == if airborne { 2 } else { 0 }), "the barrier");
            assert!(log.iter().any(|r| matches!(r, VmReq::Attack { atkp: ATKP_REFLECT_GUARD, .. })));
            assert!(log.iter().any(|r| matches!(r, VmReq::Size { a, .. } if *a == row.f(0x50))));
            let burst: Vec<u16> = log.iter().filter_map(|r| if let VmReq::Attack { atkp, .. } = r { Some(*atkp) } else { None }).collect();
            if clash > 0 {
                assert!(log.iter().any(|r| matches!(r, VmReq::Effect { id: 1, .. })), "the clash flash");
                assert!(log.iter().any(|r| matches!(r, VmReq::Effect { id: 4, .. })), "the burst effect");
                assert_eq!(burst, vec![ATKP_REFLECT_GUARD, 574, 667]);
                assert!(log.iter().any(|r| matches!(r, VmReq::Size { a, b, .. } if *a == row.f(0x5c) + 20.0 && *b == row.f(0x60) + 20.0)));
            } else {
                assert_eq!(burst, vec![ATKP_REFLECT_GUARD], "no clash, no burst");
            }
            assert!(vm.missing.is_empty(), "{:?}", vm.missing);
            // The persistent block is reset by the cast whose barrier it holds (R0181).
            assert_eq!(&world.shared[0].1[4..12], &[0; 8]);
        }
    }

    /// WP-7I forms: Blizzard's hand-ported element choice (`blizzard_plan`) against BLIZZARD_1.mag's own
    /// first slice of event 21 for every kind the game casts (a fresh program: the counter +0x2c is 0),
    /// and the plan's shot counts: one ahead (two from 1 and 7 for Final non-finishers), five for the
    /// Wisdom finisher, seven in Master.
    #[test]
    fn blizzard_forms_follow_the_script() {
        let Ok(iso) = crate::assets::iso_path() else { return };
        if !iso.exists() {
            return;
        }
        let disc = Disc::open(&iso);
        let (code, table, pax, magi) = spell_setup(&disc, "magic/BLIZZARD_1.mag");
        let frames: Frames = [Mat4::IDENTITY; 10];
        for form in [0u8, 2, 3, 4] {
            for fin in [false, true] {
                let kind = rules::cast_kind(1, fin, form);
                let row = rules::magi_row(&magi, kind).unwrap().clone();
                let mut world = VmWorld::default();
                let mut vm = VmCast::start(&code, table.as_ref(), kind).expect("event 20");
                let _ = test_slice(&mut vm, &mut world, false, true, kind, &row, &pax, &frames, None, Vec::new());
                vm.rt.restart_thread(21, &[]);
                let first = test_slice(&mut vm, &mut world, false, true, kind, &row, &pax, &frames, None, Vec::new());
                let starts: Vec<u16> = first.iter().filter_map(|r| if let VmReq::Effect { id, .. } = r { Some(*id) } else { None }).collect();
                let plan = blizzard_plan(rules::script_kind(1, kind), 0);
                assert_eq!(starts, plan.elements, "kind {kind}");
                let want = match (form, fin) {
                    (2, true) => 5,
                    (3, _) => 7,
                    (4, false) => 2,
                    _ => 1,
                };
                assert_eq!(plan.shots.len(), want, "kind {kind}");
            }
        }
        // Final finishers by the counter: 1 and 3 start element 7, 4 both, others 1.
        assert_eq!(blizzard_plan(31, 1).elements, vec![7]);
        assert_eq!(blizzard_plan(31, 4).elements, vec![1, 7]);
        assert_eq!(blizzard_plan(31, 2).elements, vec![1]);
    }

    /// WP-7I forms: Thunder's strikes per kind (`thunder_plan`, THUNDER_*.mag R0017): one; Master four
    /// (from the caster toward the place at 0, 0.47, 0.78, then the place, 16 frames apart); the Wisdom
    /// finisher four (the place, then three around it 8 frames apart, `magi` +0x24 * 1.5 away); Final
    /// three, a Final finisher eight (the place, then host 1.248's scatter).
    /// WP-7M2: Blizzard's end test L039d. A wall (2.29) or a clash (2.39) ends every shot; a target
    /// (2.14's flag 2) ends only a shot whose block flag is 0 (the Wisdom finisher's five, Master's
    /// seven); the plain shot (flag 1) goes on through targets.
    #[test]
    fn blizzard_shot_flag_follows_host_2_14() {
        assert!(shot_ends(true, false, false, 1) && shot_ends(false, true, false, 1));
        assert!(!shot_ends(false, false, true, 1), "flag 1: through targets");
        assert!(shot_ends(false, false, true, 0), "flag 0: ends at the first target");
        assert!(!shot_ends(false, false, false, 0));
        assert!(blizzard_plan(0, 0).shots.iter().all(|s| s.flag == 1));
        assert!(blizzard_plan(11, 0).shots.iter().all(|s| s.flag == 0));
        assert!(blizzard_plan(20, 0).shots.iter().all(|s| s.flag == 0));
    }

    #[test]
    fn thunder_forms_strike_counts() {
        let here = Vec3::ZERO;
        let facing = Vec3::Z;
        let t = Some(Vec3::new(0.0, 0.0, 5.0));
        assert_eq!(thunder_plan(0, here, facing, t, 100.0).len(), 1);
        let m = thunder_plan(20, here, facing, t, 100.0);
        assert_eq!(m.iter().map(|x| x.0).collect::<Vec<_>>(), vec![16.0, 16.0, 16.0, 0.0]);
        assert!((m[1].1.z - 5.0 * 0.47).abs() < 1e-5 && m[3].1 == t.unwrap() && m[3].2 == 100.0);
        let w = thunder_plan(11, here, facing, t, 100.0);
        assert_eq!(w.iter().map(|x| x.0).collect::<Vec<_>>(), vec![8.0, 8.0, 8.0, 0.0]);
        for x in &w[1..] {
            assert!(((x.1 - t.unwrap()).length() / SCALE - 150.0).abs() < 1e-2);
        }
        assert_eq!(thunder_plan(10, here, facing, t, 100.0).len(), 1, "Wisdom without a finisher");
        // Final (L014c): the place, then 2 (finisher 7) scatter strikes 8 frames apart, probe 330.
        let f = thunder_plan(30, here, facing, t, 100.0);
        assert_eq!(f.iter().map(|x| (x.0, x.3)).collect::<Vec<_>>(), vec![(8.0, false), (8.0, true), (0.0, true)]);
        assert!(f[1].1 == t.unwrap() && f[1].2 == 330.0, "scatter around the place");
        let ff = thunder_plan(31, here, facing, t, 100.0);
        assert_eq!(ff.len(), 8);
        assert_eq!(ff.iter().filter(|x| x.3).count(), 7);
        assert_eq!(ff.last().unwrap().0, 0.0);
        // No target: 300 ahead (Master: 800 at 45 degrees), raised by 80, probe height 330.
        let n = thunder_plan(0, here, facing, None, 100.0);
        assert!((n[0].1 - Vec3::new(0.0, 80.0, 300.0) * SCALE).length() < 1e-5 && n[0].2 == 330.0);
        let nm = thunder_plan(20, here, facing, None, 100.0);
        assert!(((nm[3].1.with_y(0.0)).length() / SCALE - 800.0).abs() < 1e-2);
        // Host 0.13 keeps the length and turns by the angle.
        let r = rotate_y_game(Vec3::Z, std::f32::consts::FRAC_PI_2);
        assert!((r.length() - 1.0).abs() < 1e-6 && r.z.abs() < 1e-6);
    }

    /// WP-7I follow-up of WP-7A: the HAND PORT of Fire's events 20 and 21 (`charge_element`,
    /// `fire_plan`) against Fire's own script in the interpreter, for every kind the game casts (levels
    /// 1 to 3, finisher or not, no form and Wisdom / Master / Final) on the ground and in the air:
    /// event 20 starts element 0 except for Wisdom and Final; event 21's first slice stops it (every
    /// kind but Final) and starts the same elements in the same order.
    #[test]
    fn fire_hand_port_matches_the_script_for_every_kind() {
        let Ok(iso) = crate::assets::iso_path() else { return };
        if !iso.exists() {
            return;
        }
        let disc = Disc::open(&iso);
        let table = disc.read_root("SLUS_210.05").and_then(|exe| kh2_bdx::exe::HostTable::from_exe(&exe));
        let f = disc.read("03system.bin").unwrap();
        let top = kh2_bar::parse(&f).unwrap();
        let inner = kh2_bar::parse(kh2_bar::find(&top, "pref").unwrap().data).unwrap();
        let magi = rules::parse_magi(kh2_bar::find(&inner, "magi").unwrap().data);
        let frames: Frames = [Mat4::IDENTITY; 10];
        for level in 1..=3u8 {
            let name = format!("magic/FIRE_{level}.mag");
            let file = disc.read(&name).expect("fire file");
            let code = kh2_bar::parse(&file).unwrap().into_iter().find(|e| e.kind == 3).expect("script").data.to_vec();
            let pax = Arc::new(fx::load(&disc, &name, 100, None).expect("pax"));
            for form in [0u8, 2, 3, 4] {
                for fin in [false, true] {
                    for airborne in [false, true] {
                        let kind = rules::cast_kind(level, fin, form);
                        let row = rules::magi_row(&magi, kind).unwrap().clone();
                        let mut vm = VmCast::start(&code, table.as_ref(), kind).expect("event 20");
                        let mut world = VmWorld::default();
                        let mut slice = |vm: &mut VmCast| test_slice(vm, &mut world, airborne, true, kind, &row, &pax, &frames, None, Vec::new());
                        let ev20: Vec<u16> = (0..3).flat_map(|_| slice(&mut vm)).filter_map(|r| if let VmReq::Effect { id, .. } = r { Some(id) } else { None }).collect();
                        let charge = charge_element(0, kind);
                        assert_eq!(ev20, charge.into_iter().collect::<Vec<_>>(), "{name} kind {kind}: event 20");
                        vm.rt.restart_thread(21, &[]);
                        let first = slice(&mut vm);
                        let plan = fire_plan(kind, airborne);
                        let starts: Vec<u16> = first.iter().filter_map(|r| if let VmReq::Effect { id, .. } = r { Some(*id) } else { None }).collect();
                        let stops = first.iter().any(|r| matches!(r, VmReq::Stop { id: 0, .. }));
                        assert_eq!(starts, plan.starts, "{name} kind {kind} airborne {airborne}: event 21 elements");
                        assert_eq!(stops, plan.stop_charge && charge.is_some(), "{name} kind {kind}: the charge effect stops");
                    }
                }
            }
        }
    }

    /// FUN_001ba2c0 -> FUN_001a4bb8 through `MagicData::status`: with too little MP a spell is usable
    /// (MP Charge) while its Customize switch bit (`cmd.Argument` of the word at 0x340eac) is clear,
    /// and not usable once the pause menu sets it ("Don't MP Charge"); Cure (cost 255) stays usable;
    /// with enough MP the bit does not matter.
    #[test]
    fn mp_charge_switch_follows_the_spell_slot() {
        let Ok(iso) = crate::assets::iso_path() else { return };
        if !iso.exists() {
            return;
        }
        let disc = Disc::open(&iso);
        let (magc, cmds) = {
            let mut a = disc.0.lock().unwrap();
            (kh2_tables::load::<Magc>(&mut a, &t::MAGC).unwrap().rows, kh2_tables::load::<Cmd>(&mut a, &t::CMD).unwrap().rows)
        };
        let list = rules::magic_list(&magc, &[1, 1, 1, 1, 0, 0], 2);
        let md = MagicData { magc, cmds, magi: Vec::new(), levels: [1, 1, 1, 1, 0, 0], world: 2, slots: Vec::new(), mp_drive: 0.0, scripts: Vec::new(), host_table: None, scatter: None };
        let mut p = crate::progress::Progress::from_block(&[], Arc::new(crate::progress::ItemIndex::new(&[], &[], &[])));
        let (fire, cure) = (list[0], list[3]);
        let fire_slot = md.cmd(fire).unwrap().argument as u8;
        assert_eq!(md.status(fire, 0, false, 0, Some(&p)), Status::ShortMp, "clear bit: MP Charge");
        p.set_spell_switch(fire_slot, true);
        assert!(p.spell_switch(fire_slot));
        assert_eq!(md.status(fire, 0, false, 0, Some(&p)), Status::Unavailable, "set bit: Don't MP Charge");
        assert_eq!(md.status(fire, 99, false, 0, Some(&p)), Status::Ready);
        assert_eq!(md.status(fire, 0, false, 0, None), Status::ShortMp, "no game state: a new game");
        let cure_slot = md.cmd(cure).unwrap().argument as u8;
        p.set_spell_switch(cure_slot, true);
        assert_eq!(md.status(cure, 0, false, 0, Some(&p)), Status::ShortMp, "cost 255 ignores the bit");
    }

    /// WP-7I, levels 2 and 3: each level's `magc` row names its own `.mag` (with an effect file), its own
    /// command with an MP cost, and the script of every level is the same program (the kind carries the
    /// level); every kind a level casts (`2 * level - 2`, + 1 with a finisher motion, + 10 / 20 / 30 in
    /// forms 2 / 3 / 4) has the attack rows its script asks for and a `magi` row.
    #[test]
    fn levels_two_and_three_have_their_rows() {
        let Ok(iso) = crate::assets::iso_path() else { return };
        if !iso.exists() {
            return;
        }
        let disc = Disc::open(&iso);
        let (magc, cmds, atkp) = {
            let mut a = disc.0.lock().unwrap();
            (
                kh2_tables::load::<Magc>(&mut a, &t::MAGC).unwrap().rows,
                kh2_tables::load::<Cmd>(&mut a, &t::CMD).unwrap().rows,
                kh2_tables::load::<t::Atkp>(&mut a, &t::ATKP).unwrap().rows,
            )
        };
        let f = disc.read("03system.bin").unwrap();
        let top = kh2_bar::parse(&f).unwrap();
        let inner = kh2_bar::parse(kh2_bar::find(&top, "pref").unwrap().data).unwrap();
        let magi = rules::parse_magi(kh2_bar::find(&inner, "magi").unwrap().data);
        let script = |name: &str| kh2_bar::parse(&disc.read(name).unwrap()).unwrap().into_iter().find(|e| e.kind == 3).unwrap().data.to_vec();
        // The attack rows each spell's script creates for a cast kind (hand port and VM alike).
        let rows: [&[u16]; 6] = [&[ATKP_FIRE], &[ATKP_BLIZZARD], &[ATKP_THUNDER_GROUND, ATKP_THUNDER_AIR, ATKP_THUNDER_COLUMN], &[ATKP_CURE_SELF], &[ATKP_MAGNET], &[ATKP_REFLECT_GUARD, 574, 667]];
        for id in 0..6u8 {
            let first = rules::pick_row(&magc, id, 1, 2).unwrap();
            let code1 = script(&first.file_name);
            for level in 1..=3u8 {
                let row = rules::pick_row(&magc, id, level, 2).unwrap();
                eprintln!("slot {id} level {level}: {} command {} motions {}/{}/{}", row.file_name, row.command, row.ground_motion, row.finish_motion as i16, row.air_motion);
                assert_eq!(row.level, level, "slot {id}: a row per level");
                assert!(fx::load(&disc, &row.file_name, 100, None).is_some(), "{} effect file", row.file_name);
                assert_eq!(script(&row.file_name), code1, "{}: the same program as level 1", row.file_name);
                let cmd = cmds.iter().find(|c| c.id == row.command).expect("command");
                assert_eq!((cmd.execute, cmd.argument as u8), (rules::EXECUTE_MAGIC, id));
                for form in [0u8, 2, 3, 4] {
                    for fin in [false, (row.finish_motion as i16) >= 0] {
                        // Reflect's script clears the finisher bit (event 20: kind / 2 * 2).
                        let kind = rules::script_kind(id, rules::cast_kind(level, fin, form));
                        assert!(!rules::magi_row(&magi, kind).unwrap().0.is_empty());
                        for &a in rows[id as usize] {
                            assert!(rules::attack_row(&atkp, a, kind).is_some(), "slot {id} level {level} kind {kind}: atkp {a}");
                        }
                    }
                }
            }
        }
    }

    /// The rules against the disc: Fire, Blizzard, Thunder and Cure at level 1 (`KH2_ISO` needed).
    #[test]
    fn magic_rows_from_the_disc() {
        let Ok(iso) = crate::assets::iso_path() else { return };
        if !iso.exists() {
            return;
        }
        let disc = Disc::open(&iso);
        let (magc, cmds, atkp) = {
            let mut a = disc.0.lock().unwrap();
            (
                kh2_tables::load::<Magc>(&mut a, &t::MAGC).unwrap().rows,
                kh2_tables::load::<Cmd>(&mut a, &t::CMD).unwrap().rows,
                kh2_tables::load::<t::Atkp>(&mut a, &t::ATKP).unwrap().rows,
            )
        };
        let list = rules::magic_list(&magc, &[1, 1, 1, 1, 0, 0], 2);
        eprintln!("magic list {list:?}");
        assert_eq!(list.len(), 4);
        for &c in &list {
            let cmd = cmds.iter().find(|x| x.id == c).unwrap();
            assert_eq!(cmd.execute, rules::EXECUTE_MAGIC);
            eprintln!("cmd {c}: cost {} argument {}", cmd.cost, cmd.argument);
        }
        // Cure costs all MP.
        let cure = cmds.iter().find(|x| x.id == list[3]).unwrap();
        assert_eq!(cure.cost, rules::COST_ALL);
        // Fire's level-1 attack rows: kind 0 and its variant for the third hit.
        let base = rules::attack_row(&atkp, ATKP_FIRE, 0).expect("502 kind 0");
        let v0 = rules::variant(&atkp, base, 0);
        let v2 = rules::variant(&atkp, base, 2);
        eprintln!("502: base SubId {} power {} interval {}; hit 2 SubId {} power {}", atkp[base].sub_id, atkp[base].power, atkp[base].interval, atkp[v2].sub_id, atkp[v2].power);
        assert_eq!(v0, base);
        assert_ne!(v2, base, "a later hit uses a later row");
        // Cure on self heals with type 6: (magic * Power + 99) / 100.
        let c = rules::attack_row(&atkp, ATKP_CURE_SELF, 0).expect("708 kind 0");
        assert_eq!(atkp[c].r#type, 6);
        let heal = rules::heal_amount(&atkp[c], 2, 6);
        assert_eq!(heal, (6 * atkp[c].power as i32 + 99) / 100);
        // magi rows and MpDrive.
        let f = disc.read("03system.bin").unwrap();
        let top = kh2_bar::parse(&f).unwrap();
        let inner = kh2_bar::parse(kh2_bar::find(&top, "pref").unwrap().data).unwrap();
        let magi = rules::parse_magi(kh2_bar::find(&inner, "magi").unwrap().data);
        eprintln!("magi rows {}; row 0 {:?}", magi.len(), &magi[0].0[..magi[0].0.len().min(16)]);
        assert!(magi[0].f(MAGI_TIME) > 0.0, "Fire's attack time");
    }

    /// WP-FIX-13: a Cure heals Sora for every kind the game can cast (level 1 to 3 normal casts, +10 / 20 /
    /// 30 in forms 2 / 3 / 4; Cure has no finisher motion, so the kind is always even). Each kind needs its
    /// `atkp` row 708 (type 6, Magic, Power from the disc: `(Magic * Power + 99) / 100`) and a `magi` range
    /// above 0 (`magi` +0x38: Sora counts as in range when his horizontal distance to the cast point is
    /// below it, so a zero range would skip the heal), and the heal must lift HP and stop at the maximum.
    #[test]
    fn cure_heals_for_every_kind_from_the_disc() {
        let Ok(iso) = crate::assets::iso_path() else { return };
        if !iso.exists() {
            return;
        }
        let disc = Disc::open(&iso);
        let (magc, atkp) = {
            let mut a = disc.0.lock().unwrap();
            (kh2_tables::load::<Magc>(&mut a, &t::MAGC).unwrap().rows, kh2_tables::load::<t::Atkp>(&mut a, &t::ATKP).unwrap().rows)
        };
        let f = disc.read("03system.bin").unwrap();
        let top = kh2_bar::parse(&f).unwrap();
        let inner = kh2_bar::parse(kh2_bar::find(&top, "pref").unwrap().data).unwrap();
        let magi = rules::parse_magi(kh2_bar::find(&inner, "magi").unwrap().data);
        for level in 1..=3u8 {
            let row = rules::pick_row(&magc, 3, level, 2).expect("Cure row");
            assert!((row.finish_motion as i16) < 0, "Cure has no finisher motion: its kind is never odd");
            for form in [0u8, 2, 3, 4] {
                let kind = rules::cast_kind(row.level, false, form);
                let r = rules::attack_row(&atkp, ATKP_CURE_SELF, kind).unwrap_or_else(|| panic!("no atkp 708 row for kind {kind}"));
                let (r, other) = (&atkp[r], rules::attack_row(&atkp, ATKP_CURE_OTHER, kind).expect("709 row"));
                assert_eq!((r.r#type, r.critical_adjust), (6, 1), "Cure's heal is type 6 on the Magic stat");
                assert!(atkp[other].power <= r.power, "the other member never gets more than the target");
                let magic = 7;
                let heal = rules::heal_amount(r, 99, magic);
                assert_eq!(heal, (magic * r.power as i32 + 99) / 100, "kind {kind}: the Strength argument is not used");
                assert!(heal > 0);
                let range = rules::magi_row(&magi, kind).expect("magi row").f(MAGI_CURE_RANGE);
                assert!(range > 0.0, "kind {kind}: a Cure range of 0 would heal nobody");
                // Damaged Sora: HP rises by the heal, never past the maximum.
                let max = 2 * heal + 10;
                assert_eq!(rules::healed(max - heal - 1, max, heal), max - 1);
                assert_eq!(rules::healed(max - 1, max, heal), max);
            }
        }
        // Level 1 with the starting Magic stat is the figure the harness log shows.
        let r = &atkp[rules::attack_row(&atkp, ATKP_CURE_SELF, 0).unwrap()];
        eprintln!("Cure level 1: Power {} -> heal {} at Magic 7", r.power, rules::heal_amount(r, 0, 7));
    }

    /// FUN_001a0610's stat choice for a heal row and FUN_00186238's rules (no heal for a KO'd object,
    /// the maximum caps it).
    #[test]
    fn heal_amount_and_clamp_rules() {
        let Ok(iso) = crate::assets::iso_path() else { return };
        if !iso.exists() {
            return;
        }
        let disc = Disc::open(&iso);
        let atkp = kh2_tables::load::<t::Atkp>(&mut disc.0.lock().unwrap(), &t::ATKP).unwrap().rows;
        let mut row = atkp[rules::attack_row(&atkp, ATKP_CURE_SELF, 0).unwrap()].clone();
        row.power = 400;
        assert_eq!((row.r#type, row.critical_adjust), (6, 1));
        assert_eq!(rules::heal_amount(&row, 3, 7), 28);
        assert_eq!(rules::heal_amount(&row, 3, 0), 0);
        row.critical_adjust = 0;
        assert_eq!(rules::heal_amount(&row, 3, 7), 12, "Strength when CriticalAdjust is 0");
        row.critical_adjust = 2;
        assert_eq!(rules::heal_amount(&row, 3, 7), 0, "no stat is loaded for other values");
        row.r#type = 0;
        row.critical_adjust = 1;
        assert_eq!(rules::heal_amount(&row, 3, 7), 0, "a damage row heals nothing");
        assert_eq!(rules::healed(6, 20, 28), 20);
        assert_eq!(rules::healed(6, 100, 28), 34);
        assert_eq!(rules::healed(0, 20, 28), 0, "a KO'd object takes no heal here");
    }

    /// A shot's attack starts at the caster's lock-on point (R035b, host 1.88) and moves to the shot's
    /// place in the first loop pass, so that pass sweeps the segment between them.
    #[test]
    fn shot_attack_first_segment_runs_from_the_caster() {
        let mut attacks = vec![SpellAttack { group: 1, base_row: 0, pos: None, prev: None, radius: 20.0, height: 20.0, live: true, hit_any: false, wall: false, clashes: 0, pax: None }];
        let (origin, place) = (Vec3::ZERO, Vec3::new(0.0, 0.0, 200.0));
        move_attack(&mut attacks[0], origin);
        assert_eq!(sweep_points(&attacks[0], origin).len(), 1, "created: nothing to sweep yet");
        move_attack(&mut attacks[0], place);
        assert_eq!(attacks[0].prev, Some(origin));
        let pts = sweep_points(&attacks[0], place);
        assert!(pts.len() > 2, "the segment from the caster to the shot is tested: {pts:?}");
        assert!(pts.iter().all(|p| p.z <= place.z && p.z >= origin.z));
    }

    /// The disc's executable against the rules: FUN_0019bcb8 sets the mode-1 timer from the player row's
    /// +0x68 only in mode 1, and FUN_001905e8 is bit 3 of 0x35d268 with the player object present.
    #[test]
    fn lock_on_timer_and_global_flag_8_match_the_executable() {
        use kh2_ee::Ee;
        const STACK: u32 = 0x01ff_0000;
        const CTRL: u32 = 0x0180_0000;
        const OBJ: u32 = 0x0181_0000;
        const ROW: u32 = 0x0182_0000;
        let Ok(iso) = crate::assets::iso_path() else { return };
        if !iso.exists() {
            return;
        }
        let disc = Disc::open(&iso);
        let elf = {
            let mut a = disc.0.lock().unwrap();
            ["SLUS_210.05", "SLUS_210.05;1"].iter().find_map(|p| a.iso().read(p).ok()).expect("executable")
        };
        let mut ee = Ee::new();
        ee.load_elf(&elf).expect("elf");
        ee.w32(OBJ + 0x130, ROW);
        ee.w32(ROW + 0x68, 7.5f32.to_bits());
        ee.w32(CTRL + 40, OBJ);
        for (mode, want) in [(1u32, 7.5f32), (0, 0.0), (2, 0.0)] {
            ee.w32(CTRL, mode);
            ee.w32(CTRL + 28, 0);
            ee.call(0x0019_bcb8, &[CTRL as u64], &[], STACK).expect("FUN_0019bcb8");
            let got = f32::from_bits(ee.r32(CTRL + 28));
            let ported = if mode == 1 { rules::mode1_timer_start(7.5) } else { 0.0 };
            assert_eq!((got, ported), (want, want), "mode {mode}");
        }
        for (flags, player) in [(8u32, 1u32), (8, 0), (0, 1), (0xffff_fff7, 1), (0x8, 1), (0xffff_ffff, 1)] {
            ee.w32(0x0035_d268, flags);
            ee.w32(0x0035_c998, if player != 0 { OBJ } else { 0 });
            let got = ee.call(0x0019_05e8, &[], &[], STACK).expect("FUN_001905e8") & 0xff != 0;
            assert_eq!(got, rules::global_flag_8_active(flags, player != 0), "flags {flags:#x} player {player}");
        }
    }

    /// FUN_0019a4f0 and the keep branch of FUN_0019b7a8 against the executable: every combination of the
    /// dead bit (+0x7f8 & 4), the cannot-be-hit bit (+0x108 & 8), a null status (+0x510) and hp gives the
    /// rule's answer, and a mode-1 controller with a timer above 0 and the lock button clear loses one step
    /// exactly when the object passes. The other branches of FUN_0019b7a8 call into the target marker pool
    /// and are not run.
    #[test]
    fn player_up_and_mode1_keep_match_the_executable() {
        use kh2_ee::Ee;
        const STACK: u32 = 0x01ff_0000;
        const CTRL: u32 = 0x0180_0000;
        const OBJ: u32 = 0x0181_0000;
        const STATUS: u32 = 0x0182_0000;
        const INPUT: u32 = 0x0183_0000;
        let Ok(iso) = crate::assets::iso_path() else { return };
        if !iso.exists() {
            return;
        }
        let disc = Disc::open(&iso);
        let elf = {
            let mut a = disc.0.lock().unwrap();
            ["SLUS_210.05", "SLUS_210.05;1"].iter().find_map(|p| a.iso().read(p).ok()).expect("executable")
        };
        let mut ee = Ee::new();
        ee.load_elf(&elf).expect("elf");
        let mut checked = 0;
        for dead in [0u32, 4] {
            for hit in [0u32, 8] {
                for status in [0u32, 1] {
                    for hp in [-5i32, 0, 1, 900] {
                        ee.w32(OBJ + 0x7f8, dead | 0x10);
                        ee.w32(OBJ + 0x108, hit | 0x1);
                        ee.w32(OBJ + 0x510, if status != 0 { STATUS } else { 0 });
                        ee.w32(STATUS, hp as u32);
                        let want = rules::player_up(dead != 0, hit != 0, status != 0, hp);
                        let got = ee.call(0x0019_a4f0, &[OBJ as u64], &[], STACK).expect("FUN_0019a4f0") & 0xff != 0;
                        assert_eq!(got, want, "dead {dead} hit {hit} status {status} hp {hp}");
                        if want {
                            // The controller: mode 1, owner OBJ (word 1), timer 4.0 (word 7), player OBJ (word 10).
                            ee.w32(OBJ + 0xb38, INPUT);
                            ee.w32(INPUT + 8, 0);
                            ee.w32(INPUT + 12, 0);
                            ee.w32(CTRL, 1);
                            ee.w32(CTRL + 4, OBJ);
                            ee.w32(CTRL + 28, 4.0f32.to_bits());
                            ee.w32(CTRL + 40, OBJ);
                            ee.call(0x0019_b7a8, &[CTRL as u64], &[], STACK).expect("FUN_0019b7a8");
                            let timer = f32::from_bits(ee.r32(CTRL + 28));
                            assert_eq!(Some(timer), rules::mode1_step(4.0, true), "the keep branch");
                            assert_eq!(rules::mode1_frame(4.0, false, true), rules::Mode1Frame::Keep(timer));
                        }
                        checked += 1;
                    }
                }
            }
        }
        assert_eq!(checked, 32);
    }
}
