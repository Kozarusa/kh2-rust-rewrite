//! Sora's magic, phase 1: the Magic sub-menu of the command menu, the cast motion, the MP cost and
//! the MP charge, and the spell objects of Fire and Cure.
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
//! Thunder's strike. Only the casts without a drive form are ported for Blizzard and Thunder (the form
//! variants send more shots and strikes). `KH2_MAGIC_VM=1` runs the scripts themselves in the BDX
//! interpreter instead (`VmCast`, `SpellHost`): Fire's matches the hand port; the other three stop at
//! host calls the app does not answer yet (docs/magic.md "Running the scripts in the interpreter").
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
use crate::fx::{self, Bind, LoadedPax, Placement, RunningEffect};
use crate::magic_rules::{self as rules, Charge, HitMemory, MagiRow, Paid, Status};
use crate::schedule::Sim;
use crate::skel::Animator;
use crate::sora::{Battle, Sora, State};
use crate::SCALE;

/// `atkp` id of Fire's attack objects (FIRE_*.mag event 21: `attack_create(ctx, 502, -1, kind)`).
pub const ATKP_FIRE: u16 = 502;
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
/// Ticks a cast request waits for Sora to accept it (app rule: the game keeps a pending command
/// and retries it while the playing motion has no window, the retry limit was not traced).
const REQUEST_TICKS: u32 = 20;
/// Sound of the menu? none: the game's menu sounds are not ported.
/// Size step of the swept attack test (FUN_001a2cc0: `min(radius, height) * DAT_0037b460`, 0.5).
const SWEEP_STEP: f32 = 0.5;
/// FUN_001a2fc8: a move longer than twice the frame step (`DAT_00356f40` = 1.0) sets the sweep flag.
const SWEEP_MIN: f32 = 2.0;

pub struct MagicPlugin;

impl Plugin for MagicPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<MagicState>()
            .add_systems(Startup, setup.after(crate::sora::setup))
            .add_systems(Update, harness_keys.after(crate::sora::read_pad))
            .add_systems(FixedUpdate, run.in_set(Sim::Sora).after(crate::sora::tick));
    }
}

fn log_on() -> bool {
    log_level() > 0
}

/// `KH2_MAGIC_LOG`: 1 the casts, MP and hits; 2 also every live attack each tick (the timeline the
/// hand port and the interpreter are compared by).
fn log_level() -> u32 {
    static ON: std::sync::OnceLock<u32> = std::sync::OnceLock::new();
    *ON.get_or_init(|| std::env::var("KH2_MAGIC_LOG").map_or(0, |v| v.parse().unwrap_or(1)))
}

/// `KH2_MAGIC_VM=1`: run the spell scripts in the BDX interpreter (see `VmCast`).
fn vm_on() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| std::env::var("KH2_MAGIC_VM").is_ok_and(|v| v == "1"))
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

#[derive(Resource, Default)]
pub struct MagicState {
    pub charge: Charge,
    /// A spell command waiting for Sora (command id, ticks left).
    request: Option<(u16, u32)>,
    casts: Vec<Cast>,
    attacks: Vec<SpellAttack>,
    memory: HashMap<Entity, HitMemory>,
    next_group: u32,
    shots: Vec<Shot>,
    strikes: Vec<Strike>,
    /// The cast's target records for the turn triggers of its motion (see `CastTurn`).
    turn: Option<CastTurn>,
    /// Ticks run, and the tick of the last cast (for the `KH2_MAGIC_LOG=2` attack timeline).
    tick: u64,
    cast_tick: u64,
    /// `KH2_MAGIC_HURT` has been applied.
    hurt_done: bool,
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

/// What the turn triggers of a cast motion read (FUN_00188030 cases 22 and 28): the command's target
/// record (+0x9f4; `own` when it names the caster) and the lock-on controller's mode-1 target that
/// FUN_00179470 set (+0xb40: FUN_0019b3f0 with 1). Kept while the cast motion plays.
#[derive(Clone, Copy, Debug)]
struct CastTurn {
    motion: Option<usize>,
    target: Option<Entity>,
    own: bool,
    lock_mode1: Option<Entity>,
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
    let host_table = disc.read_root("SLUS_210.05").and_then(|exe| kh2_bdx::exe::HostTable::from_exe(&exe));
    commands.insert_resource(MagicData { magc, cmds, magi, levels, world: world_id, slots, mp_drive: data.prty.mp_drive, scripts, host_table });
}

/// `KH2_KEYS` entries for the menu keys the harness does not map (T, G, B), pressed the same way.
pub(crate) fn harness_keys(time: Res<Time>, mut keys: ResMut<ButtonInput<KeyCode>>) {
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

impl MagicState {
    /// Menu glue (WP-7C): the command menu (`items.rs`, FUN_001bc0c8) issued a spell command; Sora
    /// takes it when a window lets him (see `run`). The menu only issues commands whose status is
    /// below 4 (FUN_001bb930).
    pub fn request_cast(&mut self, cmd: u16) {
        self.request = Some((cmd, REQUEST_TICKS));
        mlog!("magic: menu: cast command {cmd} selected");
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

/// The ability block, the drive state, the collision world and the game state, as optional resources.
type Context<'w> = (
    Option<Res<'w, crate::abilities::Abilities>>,
    Option<Res<'w, crate::drive::DriveState>>,
    Option<Res<'w, crate::world::World>>,
    Option<Res<'w, crate::progress::Progress>>,
);

type Victim =(Entity, Vec<(Volume, HurtSpheroid)>, Vec3);

/// The 60 Hz part: the MP charge, accepting a cast, the cast trigger, the spell scripts and their
/// attacks' hits.
#[allow(clippy::too_many_arguments)]
fn run(
    mut commands: Commands,
    data: Option<Res<MagicData>>,
    game: Res<GameData>,
    mut st: ResMut<MagicState>,
    mut battle: ResMut<Battle>,
    abilities: Res<crate::sora::Abilities>,
    (block, drive, world, progress): Context,
    mut sora_q: Query<(Entity, &mut Sora)>,
    mut anims: Query<&mut Animator>,
    locals: Query<(&Transform, Option<&ChildOf>)>,
    fighters: Query<(Entity, &Fighter, &GlobalTransform)>,
    gts: Query<&GlobalTransform>,
    mut running: Running,
    (mut hits, mut spell_hits, mut voices): (MessageWriter<Hit>, MessageWriter<fx::SpellHit>, MessageWriter<crate::audio::SoraVoice>),
    enemies: Query<&crate::enemy::Enemy>,
    mut recharge: ResMut<crate::hud::hud_gauge::MpRecharge>,
) {
    let Some(data) = data else { return };
    let Ok((me, mut s)) = sora_q.single_mut() else { return };
    let st = &mut *st;
    st.tick += 1;
    if !st.hurt_done
        && let Some(hp) = hurt_due(st.tick)
    {
        st.hurt_done = true;
        mlog!("magic: KH2_MAGIC_HURT: Sora's HP {} -> {hp}", s.hp);
        s.hp = hp.clamp(1, s.stats.max_hp);
    }
    let form = drive.as_ref().map_or(0, |d| d.form);

    // MP charge (FUN_00175f48 each frame; the regeneration multiplier is the block's +0x34). The
    // drive form start ends it through `refill_mp` (FUN_001a0240).
    if st.charge.active() {
        let regen = block.as_ref().map_or(1.0, |b| b.block().mp_regen);
        if st.charge.tick(regen) {
            s.mp = game.sora_max_mp;
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
    let alive = |e: Entity| enemies.get(e).is_ok_and(|x| x.alive());
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
            (Some((row, pax)), Some(press)) if status.usable() && !st.casts.iter().any(|c| c.step == Step::Waiting || (c.step == Step::Vm && !c.paid)) => {
                s.hits = chain.combo;
                s.finishers = chain.finishers;
                let finisher = form != 6 && rules::is_finisher(chain.combo, airborne, mg, ma);
                // The command's target record (+0x9f4, copied from the request by FUN_00174cf8): Cure's
                // names the caster (the app casts it on Sora), the others Sora's current target
                // [inferred: the menu code that fills the request's target was not traced].
                let own = row.id == 3;
                let mut target = if own { None } else { s.target.filter(|t| alive(*t)) };
                // FUN_00179470 (Magic Lock-On): without a valid record a target is picked (FUN_0018fb98
                // with range 0: every target shape of every enemy) and the lock-on controller goes to
                // mode 1 on it (FUN_0019b3f0: the target marker; FUN_0019bcb8 its timer).
                let mut lock_mode1 = None;
                let has_lock_on = block.as_ref().is_some_and(|b| b.has_bit(rules::BIT_MAGIC_LOCK_ON));
                if rules::magic_lock_on_applies(has_lock_on, row.id) && target.is_none() {
                    let cands: Vec<(Entity, Vec3)> = targets.iter().filter(|(e, _)| alive(*e)).map(|(e, p)| (*e, *p / SCALE)).collect();
                    target = rules::lock_on_pick(s.motor.pos, s.motor.forward(), game.plyr.auto_lock_on_h_adjust, game.plyr.auto_lock_on_inner_adjust, &cands);
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
                if airborne {
                    // The mover's method +0xc with sstm MagicJump (FUN_001a4798) [inferred: a jump start].
                    s.motor.start_jump(game.sstm.magic_jump);
                }
                // The voice (FUN_001a4798 -> the player's method +0x54, FUN_00175d18).
                let voice = rules::cast_voice(row, fin_motion, own);
                if voice >= 0 {
                    voices.write(crate::audio::SoraVoice { voice: voice as u16, why: format!("cast {}", row.file_name) });
                }
                st.turn = Some(CastTurn { motion: mslot, target, own, lock_mode1 });
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
                let vm = if vm_on() { pax.as_ref().zip(data.scripts.get(row.id as usize).cloned().flatten()).and_then(|(_, code)| VmCast::start(&code, data.host_table.as_ref(), kind)) } else { None };
                let charge_fx = if vm.is_none() { charge_element(row.id, kind) } else { None };
                if let (Some(id), Some(p)) = (charge_fx, pax) {
                    fx::start(&mut commands, p, id, Bind::Owner(body_e));
                }
                let step = if vm.is_some() { Step::Vm } else { Step::Waiting };
                let mut cast = Cast { slot: row.id, cmd, cost, kind, magi, pax: pax.clone(), motion: mslot, motion_id, charge_fx, step, vm, paid: false };
                st.cast_tick = st.tick;
                if cast.vm.is_some() {
                    mlog!("magic(vm): {} event 20 runs in the interpreter", row.file_name);
                    let mut ctx = VmCtx { commands: &mut commands, game: &game, frames: &frames, running: &mut running, attacks: &mut st.attacks, next: &mut st.next_group, body: body_e };
                    vm_slice(&mut cast, &mut ctx, airborne, true);
                }
                st.casts.push(cast);
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
    match st.turn {
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
                let (rec, own) = if id == rules::TRIGGER_TURN_TARGET { (t.target, t.own) } else { (s.lock.or(t.lock_mode1), false) };
                let Some(at) = rec.filter(|e| alive(*e)).and_then(|e| targets.iter().find(|x| x.0 == e)).map(|x| x.1 / SCALE) else { continue };
                let before = s.motor.yaw;
                s.motor.yaw = rules::cast_turn(before, s.motor.pos, at, own, deg);
                if before != s.motor.yaw {
                    mlog!("magic: turn trigger {id} at frame {g:.1}: toward {rec:?} by at most {deg} deg, yaw {before:.3} -> {:.3}", s.motor.yaw);
                }
            }
        }
        Some(_) if s.state != State::Magic => st.turn = None,
        _ => {}
    }

    // Frame trigger 17 of the motion this tick (FUN_00188498 case 0x11): pay, then event 21. A trigger
    // with a parameter fires only when it equals a word of the trigger context: the motion id the
    // cast asked for [inferred from the data: the cast slots are shared by spells (Fire's ground motion
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
    for c in st.casts.iter_mut() {
        if c.step != Step::Waiting && !(c.step == Step::Vm && !c.paid) {
            continue;
        }
        if fires(c) && in_motion(c) {
            // FUN_001a4a60 -> FUN_001769e0: the cost byte comes off MP (FUN_00186368).
            let before = s.mp;
            match rules::pay(s.mp, game.sora_max_mp, c.cost, data.mp_drive) {
                Paid::Mp(v) => s.mp = v,
                Paid::Charge { total } => {
                    s.mp = 0;
                    st.charge = Charge { left: total, total };
                }
            }
            if log_level() >= 2 {
                info!("magic: timeline +{} trigger 17", st.tick - st.cast_tick);
            }
            mlog!("magic: trigger 17 at frame {:.1}: MP spent {} -> {} (cost {}){}", body.gframe(), before, s.mp, c.cost, if st.charge.active() { format!(", MP charge {:.0} frames", st.charge.total) } else { String::new() });
            if let Some(vm) = c.vm.as_mut() {
                // FUN_0016c500(thread, 21, 0, 0): the script's own event 21 runs in this tick's slice.
                c.paid = true;
                vm.rt.restart_thread(21, &[]);
                mlog!("magic(vm): event 21");
                continue;
            }
            c.step = event21(&mut commands, c, &data, &game, &mut s, me, body_e, &frames, airborne_now, &mut running, &mut st.attacks, &mut st.next_group);
        } else if !in_motion(c) && c.vm.is_none() {
            // Event 20 ends when the caster's motion changes (host 1.15): no spell, no cost.
            mlog!("magic: cast of command {} ended before its trigger (Sora {:?})", c.cmd, s.state);
            c.step = Step::Done;
        }
    }

    // The scripts in the interpreter: one slice per tick (FUN_001a4428), except a cast's first, which
    // ran when it started.
    for c in st.casts.iter_mut().filter(|c| c.step == Step::Vm) {
        let im = in_motion(c);
        if c.vm.as_ref().is_some_and(|v| !v.ran) {
            let mut ctx = VmCtx { commands: &mut commands, game: &game, frames: &frames, running: &mut running, attacks: &mut st.attacks, next: &mut st.next_group, body: body_e };
            vm_slice(c, &mut ctx, airborne_now, im);
        }
        if let Some(v) = c.vm.as_mut() {
            v.ran = false;
        }
    }

    // The scripts after their first slice (Fire's attack loop).
    for c in st.casts.iter_mut() {
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
                    let a1 = new_attack(&mut st.attacks, &mut st.next_group, &game, &pax, ATKP_FIRE, c.kind, c.magi.f(MAGI_RADIUS), c.magi.f(MAGI_HEIGHT));
                    let mut a2 = None;
                    if c.kind / 10 == 3 {
                        // Host 2.31 copies the attack with id 502, sized 30 smaller; a finisher widens the first to 450.
                        a2 = new_attack(&mut st.attacks, &mut st.next_group, &game, &pax, ATKP_FIRE, c.kind, c.magi.f(MAGI_RADIUS) - 30.0, c.magi.f(MAGI_HEIGHT) - 30.0);
                        if c.kind & 1 == 1
                            && let Some(i) = a1
                        {
                            st.attacks[i].radius = 450.0;
                        }
                    }
                    let Some(a1) = a1 else {
                        warn!("magic: no atkp row {ATKP_FIRE} for kind {}", c.kind);
                        c.step = Step::Done;
                        continue;
                    };
                    mlog!("magic: Fire attack {} made: atkp {ATKP_FIRE} kind {} radius {} height {} for {} frames", st.attacks[a1].group, c.kind, st.attacks[a1].radius, st.attacks[a1].height, c.magi.f(MAGI_TIME));
                    (a1, a2)
                }
            };
            if t > 0.0 {
                let p = element_position(&pax, ball, &frames);
                move_attack(&mut st.attacks[a1], p);
                if let (Some(b2), Some(i2)) = (ball2, a2) {
                    move_attack(&mut st.attacks[i2], element_position(&pax, b2, &frames));
                }
                c.step = Step::Fire { ball, ball2, attacks: Some((a1, a2)), t: t - 1.0, yielded };
            } else {
                // Host 2.13: the attack objects are finished.
                st.attacks[a1].live = false;
                if let Some(i2) = a2 {
                    st.attacks[i2].live = false;
                }
                mlog!("magic: Fire attack {} over", st.attacks[a1].group);
                c.step = Step::Done;
            }
        }
    }
    // Blizzard's event 21 after its yield: the push of the caster (host 1.146 with `magi` +0x1c, not
    // ported) and the shot from element 1's position along the facing (R02b9 -> R02d7 -> R04d6).
    for c in st.casts.iter_mut() {
        if let Step::Blizzard { elem, yielded } = c.step {
            if !yielded {
                c.step = Step::Blizzard { elem, yielded: true };
                continue;
            }
            if let Some(pax) = c.pax.clone() {
                let pos = element_position(&pax, elem, &frames);
                let dir = s.motor.forward().with_y(0.0).normalize_or(Vec3::Z);
                mlog!("magic: Blizzard shot from element {elem} at {pos:?} along {dir:?}: speed {} life {} size {} turn {} (push {} not ported)", c.magi.f(MAGI_SPEED), c.magi.f(MAGI_LIFE), c.magi.f(MAGI_SHOT_SIZE), c.magi.f(MAGI_TURN), c.magi.f(MAGI_PUSH));
                st.shots.push(Shot { pax, kind: c.kind, magi: c.magi.clone(), pos, dir, target: s.target, attack: None, effect: None, life: c.magi.f(MAGI_LIFE) - SHOT_FADE as f32, fade: None, done: false });
            }
            c.step = Step::Done;
        }
    }
    // Thunder's event 21, the rest of its first slice: the strike's place is the target's position
    // (host 1.120) raised by 100, or 300 ahead of the caster raised by 80 and 330 without a target
    // (L003e / L0076 / L009c), then R02b5 starts the strike thread.
    for c in st.casts.iter_mut().filter(|c| c.step == Step::Thunder) {
        c.step = Step::Done;
        let Some(pax) = c.pax.clone() else { continue };
        let here = s.motor.pos * SCALE;
        let (pos, rise) = match s.target.and_then(|t| targets.iter().find(|x| x.0 == t)) {
            Some(t) => (t.1, 100.0),
            None => (here + s.motor.forward().with_y(0.0).normalize_or(Vec3::Z) * 300.0 * SCALE + Vec3::Y * 80.0 * SCALE, 330.0),
        };
        mlog!("magic: Thunder strike at {pos:?} (target {:?}, rise {rise})", s.target);
        st.strikes.push(Strike { pax, kind: c.kind, magi: c.magi.clone(), pos, rise, atkp: 0, started: false, wait: 0.0, attacks: None, done: false });
    }
    st.casts.retain(|c| c.step != Step::Done);
    for k in st.strikes.iter_mut() {
        step_strike(k, &mut commands, &game, &mut st.attacks, &mut st.next_group, world.as_deref());
    }
    st.strikes.retain(|x| !x.done);

    // The shots (request threads; they run after the spell threads in the same task pass [inferred]).
    let mut claimed: Vec<Entity> = st.shots.iter().filter_map(|x| x.effect).collect();
    for shot in st.shots.iter_mut() {
        step_shot(shot, &mut commands, &game, &mut st.attacks, &mut st.next_group, &mut running, &targets, &claimed);
        claimed.extend(shot.effect);
    }
    st.shots.retain(|x| !x.done);

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
    for a in st.attacks.iter_mut().filter(|a| a.live) {
        let Some(cur) = a.pos else { continue };
        if log_level() >= 2 {
            info!("magic: timeline +{} attack {} atkp {} at {:.1?} radius {} height {}", st.tick - st.cast_tick, a.group, game.atkp[a.base_row].id, cur / SCALE, a.radius, a.height);
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
        for (te, shapes, tpos) in &victims {
            let mem = st.memory.entry(*te).or_default();
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
            if let Some(pax) = &a.pax {
                spell_hits.write(fx::SpellHit { target: *te, atkp: hit.atkp, pax: pax.clone() });
            }
            a.hit_any = true;
        }
    }
    for m in st.memory.values_mut() {
        m.tick();
    }
    // Casts hold indices into the list: finished attacks go once no cast runs.
    if st.casts.is_empty() && st.shots.is_empty() && st.strikes.is_empty() {
        st.attacks.retain(|a| a.live);
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
    _attacks: &mut [SpellAttack],
    _next: &mut u32,
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
            // BLIZZARD (kind / 10 of 0 or 1 without a finisher bit): R0263(1) stops the charge effect
            // and starts element 1; Final finishers and Master use 7 (and 1 and 7), not ported.
            if c.kind / 10 != 0 {
                warn!("magic: Blizzard in a form (kind {}) is not ported", c.kind);
                return Step::Done;
            }
            if let Some(id) = c.charge_fx {
                stop_effect(running, pax, id, body);
            }
            start(commands, 1);
            Step::Blizzard { elem: 1, yielded: false }
        }
        2 => {
            // THUNDER (kind / 10 of 0 or 1 without a finisher): R0226(1) stops the charge effect and
            // starts element 1; the strike is placed in the same slice (`Step::Thunder`). Master, Final
            // and the Wisdom finisher strike more than once and are not ported.
            if c.kind / 10 != 0 {
                warn!("magic: Thunder in a form (kind {}) is not ported", c.kind);
                return Step::Done;
            }
            if let Some(id) = c.charge_fx {
                stop_effect(running, pax, id, body);
            }
            start(commands, 1);
            Step::Thunder
        }
        _ => Step::Done,
    }
}

/// The target points the shots steer toward: each enemy's first target shape (FUN_001894d0's spheroid
/// centre), by root entity [inferred: host 1.120 reads the target record's position].
fn victim_targets(fighters: &Query<(Entity, &Fighter, &GlobalTransform)>, anims: &Query<&mut Animator>, gts: &Query<&GlobalTransform>) -> Vec<(Entity, Vec3)> {
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
/// `magi` +0x10 - 12. Each frame (L039d): the shot ends when its effect is gone or its attack hit a
/// target or terrain; otherwise the effect and the attack move to the position; after the yield the
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
        move_attack(&mut attacks[ai], shot.pos);
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
    let rate = shot.magi.f(MAGI_TURN);
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
    if shot.fade == Some(0) || a.hit_any || a.wall {
        mlog!("magic: Blizzard shot over at {:?} (hit {}, wall {}, faded {})", shot.pos, a.hit_any, a.wall, shot.fade == Some(0));
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
    attacks.push(SpellAttack { group: *next, base_row, pos: None, prev: None, radius, height, live: true, hit_any: false, wall: false, pax: Some(pax.clone()) });
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

// ---- the spell scripts in the interpreter (`KH2_MAGIC_VM=1`) ---------------------------------------

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

/// A cast whose script runs in the interpreter: the thread of FUN_001a4798 (event 20 at the cast, event
/// 21 from the cast trigger), the effects (element ids, stopped) and attacks (index in
/// `MagicState::attacks`) it holds handles to, and the host functions it called that the app does not
/// answer.
struct VmCast {
    rt: kh2_bdx::Runtime,
    effects: Vec<(u16, bool)>,
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
        Some(Box::new(VmCast { rt, effects: Vec::new(), attacks: Vec::new(), missing: Default::default(), ran: false }))
    }
}

/// What a script asked for in its slice, carried out after it (they need the ECS).
#[derive(Debug)]
enum VmReq {
    Effect(u16),
    Stop(u16),
    Attack { h: usize, atkp: u16, kind: u32, copy_of: Option<usize> },
    Size { h: usize, a: f32, b: f32 },
    Move { h: usize, at: Vec3 },
    Finish(usize),
}

/// The app's game space for the scripts: the game's axes (Y and Z negated) in its units.
fn to_game(v: Vec3) -> [f32; 4] {
    let g = v / SCALE;
    [g.x, -g.y, -g.z, 1.0]
}

fn from_game(v: [f32; 4]) -> Vec3 {
    Vec3::new(v[0], -v[1], -v[2]) * SCALE
}

/// The host functions the four level-1 spell scripts call, answered from the tick's state. Calls that
/// change the game are queued (`VmReq`). Anything else is recorded as missing and answers 0.
struct SpellHost<'a> {
    airborne: bool,
    /// Host 1.15: the caster's motion word (+0x154): 1 while the cast motion plays, 0 after.
    motion_state: i32,
    kind: u32,
    magi: &'a MagiRow,
    pax: &'a LoadedPax,
    frames: &'a Frames,
    vm_effects: &'a mut Vec<(u16, bool)>,
    vm_attacks: &'a mut Vec<Option<usize>>,
    missing: &'a mut std::collections::BTreeSet<String>,
    reqs: Vec<VmReq>,
}

impl SpellHost<'_> {
    fn effect(&self, h: u32) -> Option<usize> {
        h.checked_sub(VM_EFFECT).map(|i| i as usize).filter(|&i| i < self.vm_effects.len())
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
            Call::MotionState { .. } => Value::Int(self.motion_state),
            // 1.139 (FUN_00173988 -> FUN_001db868): element `id` of the resource's effect file on the
            // context's object. The mode word (1 or 0) is not told apart: the element is bound to the
            // caster as in the hand port [inferred: FUN_001db868's third argument was not traced].
            Call::Unknown { group: 1, index: 139, args } if args.len() >= 2 => {
                let id = args[1] as u16;
                self.vm_effects.push((id, false));
                self.reqs.push(VmReq::Effect(id));
                Value::Handle(VM_EFFECT + self.vm_effects.len() as u32 - 1)
            }
            Call::Unknown { group: 2, index: 31, args } if args.len() >= 2 => match self.attack(args[0]) {
                Some(src) => self.new_attack(args[1] as u16, Some(src)),
                None => Value::Int(0),
            },
            Call::Func { group: 0, index: 50, args } => Value::Bool(self.effect(raw(args, 0)).is_some_and(|i| !self.vm_effects[i].1)),
            Call::Func { group: 0, index: 44, args } => {
                if let Some(i) = self.effect(raw(args, 0)) {
                    self.vm_effects[i].1 = true;
                    self.reqs.push(VmReq::Stop(self.vm_effects[i].0));
                }
                Value::None
            }
            // 0.53 (FUN_001d9d00): the effect's position, as the hand port takes it.
            Call::Func { group: 0, index: 53, args } => {
                let p = self.effect(raw(args, 0)).map_or(self.frames[0].w_axis.truncate(), |i| element_position(self.pax, self.vm_effects[i].0, self.frames));
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
}

/// One slice of a cast's script (FUN_0016c680 from the magic task), then its requests: effects started
/// on or stopped for the caster, attack objects made, sized (FUN_001a30e0: the height is the second
/// value when positive, else the radius), moved and finished. The cast is done once the thread has
/// ended; the host functions it called that the app does not answer are logged then.
fn vm_slice(c: &mut Cast, ctx: &mut VmCtx, airborne: bool, in_motion: bool) {
    let (Some(vm), Some(pax)) = (c.vm.as_mut(), c.pax.clone()) else { return };
    vm.ran = true;
    let mut host = SpellHost {
        airborne,
        motion_state: in_motion as i32,
        kind: c.kind,
        magi: &c.magi,
        pax: &pax,
        frames: ctx.frames,
        vm_effects: &mut vm.effects,
        vm_attacks: &mut vm.attacks,
        missing: &mut vm.missing,
        reqs: Vec::new(),
    };
    vm.rt.update(&mut host);
    let reqs = std::mem::take(&mut host.reqs);
    for e in std::mem::take(&mut vm.rt.core.errors) {
        warn!("magic(vm): script error at {:#x}: {:?}", e.0, e.1);
    }
    for r in reqs {
        match r {
            VmReq::Effect(id) => fx::start(ctx.commands, &pax, id, Bind::Owner(ctx.body)),
            VmReq::Stop(id) => stop_effect(ctx.running, &pax, id, ctx.body),
            VmReq::Attack { h, atkp, kind, copy_of } => {
                let i = new_attack(ctx.attacks, ctx.next, ctx.game, &pax, atkp, kind, 0.0, 0.0);
                if let (Some(i), Some(src)) = (i, copy_of.and_then(|s| vm.attacks.get(s).copied().flatten())) {
                    // FUN_001a2c58: the copy keeps the original's group and size [inferred, as the hand port].
                    let (g, r, ht) = (ctx.attacks[src].group, ctx.attacks[src].radius, ctx.attacks[src].height);
                    let a = &mut ctx.attacks[i];
                    (a.group, a.radius, a.height) = (g, r, ht);
                }
                vm.attacks[h] = i;
                mlog!("magic(vm): attack {:?} made: atkp {atkp} kind {kind}", i.map(|i| ctx.attacks[i].group));
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
            VmReq::Finish(h) => {
                if let Some(i) = vm.attacks[h] {
                    ctx.attacks[i].live = false;
                    mlog!("magic(vm): attack {} over", ctx.attacks[i].group);
                }
            }
        }
    }
    if vm.rt.thread_done() {
        mlog!("magic(vm): script of slot {} ended; host functions not answered: {:?}", c.slot, vm.missing);
        c.step = Step::Done;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sweep_steps_back_toward_the_previous_position() {
        let a = SpellAttack { group: 1, base_row: 0, pos: None, prev: Some(Vec3::new(0.0, 0.0, 1.0)), radius: 20.0, height: 40.0, live: true, hit_any: false, wall: false, pax: None };
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
        let pax = fx::load(&disc, "magic/FIRE_1.mag", 100, None).expect("pax");
        let f = disc.read("03system.bin").unwrap();
        let top = kh2_bar::parse(&f).unwrap();
        let inner = kh2_bar::parse(kh2_bar::find(&top, "pref").unwrap().data).unwrap();
        let magi = rules::parse_magi(kh2_bar::find(&inner, "magi").unwrap().data);
        let row = rules::magi_row(&magi, 0).unwrap().clone();
        let frames: Frames = [Mat4::IDENTITY; 10];
        let mut vm = VmCast::start(&code, table.as_ref(), 0).expect("event 20");
        let mut slice = |vm: &mut VmCast, in_motion: bool| {
            let mut host = SpellHost {
                airborne: false,
                motion_state: in_motion as i32,
                kind: 0,
                magi: &row,
                pax: &pax,
                frames: &frames,
                vm_effects: &mut vm.effects,
                vm_attacks: &mut vm.attacks,
                missing: &mut vm.missing,
                reqs: Vec::new(),
            };
            vm.rt.update(&mut host);
            assert!(vm.rt.core.errors.is_empty(), "{:?}", vm.rt.core.errors);
            host.reqs
        };
        let ev20: Vec<VmReq> = (0..3).flat_map(|_| slice(&mut vm, true)).collect();
        assert!(matches!(ev20[..], [VmReq::Effect(0)]), "event 20 starts element 0 and waits");
        assert!(!vm.rt.thread_done());
        vm.rt.restart_thread(21, &[]);
        let first = slice(&mut vm, true);
        // R017d (every kind but Final's): the charge effect stops, element 1 starts; then the ball.
        assert!(matches!(first[..], [VmReq::Stop(0), VmReq::Effect(1), VmReq::Effect(2)]), "event 21: {first:?}");
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
            let pax = fx::load(&disc, &name, 100, None).expect("pax");
            for form in [0u8, 2, 3, 4] {
                for fin in [false, true] {
                    for airborne in [false, true] {
                        let kind = rules::cast_kind(level, fin, form);
                        let row = rules::magi_row(&magi, kind).unwrap().clone();
                        let mut vm = VmCast::start(&code, table.as_ref(), kind).expect("event 20");
                        let mut slice = |vm: &mut VmCast| {
                            let mut host = SpellHost {
                                airborne,
                                motion_state: 1,
                                kind,
                                magi: &row,
                                pax: &pax,
                                frames: &frames,
                                vm_effects: &mut vm.effects,
                                vm_attacks: &mut vm.attacks,
                                missing: &mut vm.missing,
                                reqs: Vec::new(),
                            };
                            vm.rt.update(&mut host);
                            assert!(vm.rt.core.errors.is_empty(), "{:?}", vm.rt.core.errors);
                            host.reqs
                        };
                        let ev20: Vec<u16> = (0..3).flat_map(|_| slice(&mut vm)).filter_map(|r| if let VmReq::Effect(id) = r { Some(id) } else { None }).collect();
                        let charge = charge_element(0, kind);
                        assert_eq!(ev20, charge.into_iter().collect::<Vec<_>>(), "{name} kind {kind}: event 20");
                        vm.rt.restart_thread(21, &[]);
                        let first = slice(&mut vm);
                        let plan = fire_plan(kind, airborne);
                        let starts: Vec<u16> = first.iter().filter_map(|r| if let VmReq::Effect(id) = r { Some(*id) } else { None }).collect();
                        let stops = first.iter().any(|r| matches!(r, VmReq::Stop(0)));
                        assert_eq!(starts, plan.starts, "{name} kind {kind} airborne {airborne}: event 21 elements");
                        assert_eq!(stops, plan.stop_charge && charge.is_some(), "{name} kind {kind}: the charge effect stops");
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
}
