//! Enemy AI runtime: Shadow, Soldier and Dusk run under the game's own BDX scripts
//! (`obj/M_EX020.mdlx`, `obj/M_EX010.mdlx`, `obj/M_EX990.mdlx`, read from the disc).
//!
//! One `kh2_bdx::Runtime` per enemy runs once per 60 Hz tick (`Runtime::update`). The script talks to
//! the world through `kh2_bdx::Host::call`; `BevyHost` (`enemy_host.rs`) answers each `Call` from a small model of
//! the scripted object (`Obj` in `enemy_obj.rs`: position and facing, the motion it asked for, flags, fade, movement
//! vector) and the app's world (the player, the collision, the camera). After the script has run the
//! system carries the requests out: motion on the `Animator`, movement through `Motor`, effects,
//! fade, hide. Hits still go through the reaction code of `enemy.rs` (`receive_hits`, states `Hurt`
//! and `Blown`); they are reported to the script as event 6, and the script sees the reaction through
//! `busy`, `hit_stun` and the mode manager's interrupt hook.
//!
//! Every call is classified (`St`): implemented, approximated, stubbed (accepted, nothing in the app
//! reads it) or missing (a logged `unimplemented host call group.index`, once per function).
//! `KH2_BDXLOG=1` prints mode transitions, state-changing host calls, motion requests, events and
//! the first use of every function; `KH2_BDXLOG=all` also prints the per-frame queries. See
//! docs/systems/script-host/contract.md for the mapping and the gaps.
//!
//! Coordinates. Scripts work in the game's space: centimetres, Y down, Z mirrored against the app's
//! (the app is the game rotated 180 degrees about X). `enemy_obj::to_game` / `from_game` convert; yaw is
//! measured in the app's space inside `Obj`.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::Arc;

use bevy::color::Alpha;
use bevy::prelude::*;
use kh2_bdx::{Call, Program, Runtime, Value};
use kh2_tables::tables as t;

use crate::assets::Disc;
use crate::combat::Fighter;
use crate::data::GameData;
use crate::enemy::{EState, Enemy, EnemyClash};
use crate::enemy_host::{call_id, compute_drop, parse_przt, prize_row, BevyHost, Env, PrizeRow, St};
use crate::enemy_obj::{handle_of_serial, AppearRamp, Appearing, CreatedHandle, Obj, ObjTable, Other, Remote, FLAG_UNHITTABLE, HIT, PLAYER, REMOTE_PATIENCE};
use crate::fx::{GameEffect, StartEffect, EFFECT_DEATH};
use crate::motor::wrap_angle;
use crate::shared::reaction::{PlayerReaction, PlayerReactionCmd, ReactionAccepted};
use crate::schedule::Sim;
use crate::skel::Animator;
use crate::shared::sora::Sora;
use crate::world::World;
use crate::SCALE;
pub use crate::shared::enemy::AiBrain;

/// Ticks between two printouts of the host call table (with `KH2_BDXLOG`).
const SUMMARY_EVERY: u64 = 900;

// ---------------------------------------------------------------------------------------------------
// Messages and resources

/// Written by `enemy::receive_hits` for every hit on an enemy: `real` is the "hit is real" test of the
/// script (the effective reaction's class is 2 or more, FUN_0018b700).
#[derive(Message, Clone, Copy, Debug)]
pub struct AiHit {
    pub target: Entity,
    pub real: bool,
    /// The effective reaction (the hit record's byte +0x26, host 2.7).
    pub reaction: u8,
    /// The id of the attack row that hit (host 2.45 answers it).
    pub atkp: i16,
}

struct Script {
    prog: Program,
    /// Bit 3 of the halfword at +0x48 of the object's `objentry` row (host 1.124, FUN_0019aaf0).
    flags124: bool,
    /// Paired player motions of the object's motion set, see `paired_motions`.
    paired: Arc<HashMap<usize, Vec<(String, f32)>>>,
}

/// The scripts of the enemies that run under BDX, by `Kind::name`.
#[derive(Resource, Default)]
struct AiPrograms {
    by_kind: HashMap<&'static str, Script>,
    /// The player's objentry byte +7, which names the sub-motion of a paired motion.
    player_model: u8,
    /// The prize table `przt` of `00battle.bin` (drops, host 1.92).
    prizes: Vec<PrizeRow>,
    /// The host function table of the executable (0x35bce8): argument counts and result flags of the
    /// functions `kh2_bdx::names` does not carry (the Struggle opponents call 4.29, for example).
    table: Option<kh2_bdx::HostTable>,
}

/// The global message queue of host 1.90 / 1.241 (64 entries at 0x1c89738, FUN_00187170) and the
/// game's second generator (0x3716a0, `seed * 0x10dcd`), which draws the message delays and the
/// drops. FUN_002f5fc8 sets it to 1; many other users advance it, so its state at a given moment is
/// not the game's.
#[derive(Resource)]
struct Mailbox {
    queue: Vec<(u32, String, f32)>,
    seed: u32,
}

impl Default for Mailbox {
    fn default() -> Self {
        Mailbox { queue: Vec::new(), seed: 1 }
    }
}

/// `KH2_BDX_HIT=<enemy name>:<reaction>:<seconds>` (harness): from `<seconds>` on, once, the first living enemy of that name
/// gets the event-6 hit of a real hit with the given effective reaction (7, 9 and 11 are the blow class), as
/// `enemy::receive_hits` would report it. Nothing else happens to the enemy (no damage, no reaction motion).
fn forced_hit() -> Option<&'static (String, u8, u64)> {
    static HIT: std::sync::OnceLock<Option<(String, u8, u64)>> = std::sync::OnceLock::new();
    HIT.get_or_init(|| {
        let v = std::env::var("KH2_BDX_HIT").ok()?;
        let mut it = v.split(':');
        let (name, reaction, secs) = (it.next()?.to_string(), it.next()?.trim().parse().ok()?, it.next()?.trim().parse::<f32>().ok()?);
        Some((name, reaction, (secs * 60.0) as u64))
    })
    .as_ref()
}

/// Size of the game's message queue (FUN_00187170 drops a message when all 0x40 entries are taken).
const MAILBOX_LEN: usize = 0x40;

/// FUN_00187250 for the app's objects: every queued message goes to every receiver in `order` (the
/// object list, FUN_001b7c88), each drawing its own delay ([`kh2_bdx::message_delay`]); the queue
/// empties. Returns the deliveries per receiver: (sender, name, delay).
fn dispatch_messages<K: Copy + Eq + std::hash::Hash>(mailbox: &mut Mailbox, order: &[K]) -> HashMap<K, Vec<(u32, String, f32)>> {
    let mut out: HashMap<K, Vec<(u32, String, f32)>> = HashMap::new();
    for (sender, name, delay) in std::mem::take(&mut mailbox.queue) {
        for &k in order {
            let d = kh2_bdx::message_delay(&mut mailbox.seed, delay);
            out.entry(k).or_default().push((sender, name.clone(), d));
        }
    }
    out
}

#[derive(Resource)]
pub struct AiTrace {
    /// 0 off, 1 modes and state-changing calls, 2 everything.
    level: u8,
    tick: u64,
    calls: BTreeMap<String, (St, u64)>,
    notes: HashSet<String>,
}

impl AiTrace {
    pub(crate) fn line(&self, tag: &str, msg: &str) {
        if self.level > 0 {
            info!("bdx {tag} t{}: {msg}", self.tick);
        }
    }

    /// One log line per distinct note (data gaps, approximations hit).
    pub(crate) fn note(&mut self, tag: &str, msg: String) {
        if self.notes.insert(msg.clone()) {
            warn!("bdx {tag}: {msg}");
        }
    }

    pub(crate) fn record(&mut self, tag: &str, c: &Call, v: &Value, st: St, why: &str) {
        let (g, i) = call_id(c);
        let name = match c {
            Call::Func { group, index, .. } | Call::Unknown { group, index, .. } => {
                kh2_bdx::names::lookup(*group, *index).map_or("?", |f| f.name).to_string()
            }
            _ => variant_name(c),
        };
        let key = format!("{g}.{i} {name}");
        let e = self.calls.entry(key.clone()).or_insert((st, 0));
        e.1 += 1;
        if e.1 == 1 {
            if st == St::Missing {
                warn!("bdx {tag}: unimplemented host call {g}.{i} ({name}) {why}");
            } else if self.level > 0 {
                info!("bdx {tag} t{}: first host call {g}.{i} {name} [{st:?}] {why}", self.tick);
            }
        }
        if self.level >= 2 || (self.level == 1 && !is_query(c)) {
            info!("bdx {tag} t{}: {c:?} -> {v:?}", self.tick);
        }
    }

    fn summary(&self) {
        if self.level == 0 {
            return;
        }
        info!("bdx host calls so far ({} distinct):", self.calls.len());
        for (k, (st, n)) in &self.calls {
            info!("bdx   {k:34} {st:?} x{n}");
        }
    }
}

fn variant_name(c: &Call) -> String {
    let s = format!("{c:?}");
    s.split(|ch: char| !ch.is_alphanumeric()).next().unwrap_or("").to_string()
}

/// Pure queries: not logged at level 1 (they run every frame).
fn is_query(c: &Call) -> bool {
    !matches!(
        c,
        Call::PlayMotion { .. }
            | Call::PlayMotionChecked { .. }
            | Call::PlayMotionAt { .. }
            | Call::FaceDirection { .. }
            | Call::FaceToward { .. }
            | Call::SetMove { .. }
            | Call::SetVelocity { .. }
            | Call::StartMove { .. }
            | Call::StartMoveAlt { .. }
            | Call::Hide { .. }
            | Call::FadeOut { .. }
            | Call::FadeIn { .. }
            | Call::GroundMarker { .. }
            | Call::SetFlag { .. }
            | Call::ClearFlag { .. }
            | Call::SetFlag26 { .. }
            | Call::SetFlag768 { .. }
            | Call::ClearFlag768 { .. }
            | Call::SpawnAttached { .. }
            | Call::ReleaseEffect { .. }
            | Call::SetObjParam { .. }
            | Call::SetTargetMode { .. }
            | Call::FreeTarget { .. }
            | Call::ObjMethod44 { .. }
            | Call::ObjMethod48 { .. }
            | Call::MoverMethod { .. }
            | Call::Func { .. }
            | Call::Unknown { .. }
    )
}

// ---------------------------------------------------------------------------------------------------
// Bevy side

pub struct EnemyAiPlugin;

impl Plugin for EnemyAiPlugin {
    fn build(&self, app: &mut App) {
        let level = match std::env::var("KH2_BDXLOG").as_deref() {
            Err(_) | Ok("") | Ok("0") => 0,
            Ok("all") | Ok("2") => 2,
            Ok(_) => 1,
        };
        app.insert_resource(AiTrace { level, tick: 0, calls: BTreeMap::new(), notes: HashSet::new() })
            .init_resource::<AiPrograms>()
            .init_resource::<Mailbox>()
            .init_resource::<ObjTable>()
            .add_message::<AiHit>()
            .add_systems(Startup, load_programs)
            .add_systems(
                FixedUpdate,
                (
                    ai_attach.after(crate::enemy::spawn_waves),
                    ai_remote.after(ai_attach),
                    ai_update.after(ai_remote).before(crate::enemy::think),
                    crate::enemy::spawn_created.after(ai_update),
                    ai_fade.after(crate::enemy::think),
                )
                    .in_set(Sim::Enemy),
            );
    }
}

/// Depth below the player (game units, 30 m) at which an airborne enemy counts as fallen out of the map.
const FALL_OUT_UNITS: f32 = 3000.0;

/// Reads the BDX entry (BAR type 3) of each scripted enemy's object file and its `objentry` flags.
fn load_programs(disc: Res<Disc>, mut programs: ResMut<AiPrograms>, mut table: ResMut<ObjTable>) {
    let entries: Vec<(&'static str, &str)> = crate::enemy::KINDS.iter().map(|k| (k.name, k.model)).collect();
    let rows = {
        let Ok(mut a) = disc.0.lock() else { return };
        kh2_tables::load::<t::Objentry>(&mut a, &t::OBJENTRY).map(|d| d.rows).unwrap_or_default()
    };
    // The player's objentry byte +7 (`WeaponJoint`): the paired motions of reaction commands hold one
    // sub-motion per player model, named by `%03d0` of it (FUN_0017ef98 formats the key with the format
    // string at 0x37a598, FUN_0017ee40 looks the name up). Sora's row is P_EX100.
    programs.player_model = rows.iter().find(|r| r.model_name == "P_EX100").map_or(1, |r| r.weapon_joint);
    // The objects host 1.0 can make: the `00objentry` rows whose model is one of the app's enemy kinds.
    for r in &rows {
        if let Some(k) = crate::enemy::KINDS.iter().find(|k| k.model == r.model_name) {
            table.kinds.insert(r.object_id, k);
        }
    }
    table.creatable = Arc::new(table.kinds.keys().copied().collect());
    programs.prizes = disc
        .read("00battle.bin")
        .and_then(|b| kh2_bar::parse(&b).ok().and_then(|e| e.iter().find(|x| x.tag == "przt").map(|x| parse_przt(x.data))))
        .unwrap_or_default();
    if programs.prizes.is_empty() {
        warn!("bdx: 00battle.bin przt not readable; drops are not computed");
    }
    programs.table = disc.read_root("SLUS_210.05").and_then(|exe| kh2_bdx::HostTable::from_exe(&exe));
    if programs.table.is_none() {
        warn!("bdx: the host table of SLUS_210.05 is not readable; calls to functions kh2_bdx does not name stop their script");
    }
    for (kind, model) in entries {
        // The motion set is the one the objentry row names (`M_EX610_RAW` names `M_EX610.mset` and has no file of its own).
        let mset = crate::enemy::mset_name(rows.iter().find(|r| r.model_name == model).map(|r| r.animation_name.as_str()), model);
        let paired = disc.read(&format!("obj/{mset}.mset")).map(|m| paired_motions(&m)).unwrap_or_default();
        let Some(file) = disc.read(&format!("obj/{model}.mdlx")) else { continue };
        let Some(entry) = kh2_bar::parse(&file).ok().and_then(|b| b.into_iter().find(|e| e.kind == 3).map(|e| e.data.to_vec())) else { continue };
        match Program::parse(&entry) {
            Ok(prog) => {
                let flags124 = rows.iter().find(|r| r.model_name == model).is_some_and(|r| r.flags & 8 != 0);
                info!("bdx: {kind} script {} ({} words), objentry flag 124 = {flags124}", prog.name, prog.words.len());
                if !paired.is_empty() {
                    info!("bdx: {kind} motion set has {} paired player motion entries", paired.len());
                }
                programs.by_kind.insert(kind, Script { prog, flags124, paired: Arc::new(paired) });
            }
            Err(e) => warn!("bdx: {model}: {e}; {kind} has no script and only stands"),
        }
    }
}

/// The paired player motions of an enemy's motion set: the entries of BAR type 20, each a BAR of
/// sub-motions named `%03d0` of a player model byte (FUN_0017ee40). Maps entry slot to
/// (sub-entry name, length in 60 Hz ticks); an empty sub-entry (the fallbacks) is left out.
fn paired_motions(mset: &[u8]) -> HashMap<usize, Vec<(String, f32)>> {
    let mut out = HashMap::new();
    let Ok(entries) = kh2_bar::parse(mset) else { return out };
    for (slot, e) in entries.iter().enumerate().filter(|(_, e)| e.kind == 20) {
        let Ok(subs) = kh2_bar::parse(e.data) else { continue };
        let list: Vec<(String, f32)> = subs
            .iter()
            .filter(|s| !s.data.is_empty())
            .filter_map(|s| {
                // Each sub-entry is an animation BAR like a slot of the set: motion (type 9), triggers (16).
                let anb = kh2_bar::parse(s.data).ok()?;
                let m = anb.iter().find(|x| x.kind == 9).and_then(|x| kh2_anim::Motion::parse(x.data))?;
                Some((s.tag.to_string(), (m.frame_end - m.frame_start).max(0.0) * 60.0 / m.fps.max(1.0)))
            })
            .collect();
        out.insert(slot, list);
    }
    out
}

/// An enemy with no script runtime yet: its handle if host 1.0 made it, and whether it fades in.
type Unattached = (Entity, &'static Enemy, Option<&'static CreatedHandle>, Option<&'static Appearing>);

/// Gives every new enemy its script runtime. Runs right after the wave spawner, so the
/// script's first frame is the frame the enemy appears in.
fn ai_attach(
    mut commands: Commands,
    programs: Res<AiPrograms>,
    mut table: ResMut<ObjTable>,
    appear: Option<Res<AppearRamp>>,
    q: Query<Unattached, Without<AiBrain>>,
) {
    for (ent, e, made, appearing) in &q {
        let Some(script) = programs.by_kind.get(e.kind.name) else { continue };
        // An object host 1.0 made has the handle that call gave (its serial was taken then).
        let (serial, handle) = match made {
            Some(h) => (h.0.wrapping_sub(handle_of_serial(0)) / crate::enemy_obj::OBJ_SIZE as u32, h.0),
            None => {
                table.serial += 1;
                (table.serial, handle_of_serial(table.serial))
            }
        };
        let mut rt = Runtime::new(script.prog.clone());
        if let Some(t) = &programs.table {
            rt.set_host_table(t.clone());
        }
        rt.shared.rng = 0x9E37_79B9u32 ^ serial.wrapping_mul(0x85EB_CA6B);
        rt.core.mode_log = Some(Vec::new());
        let mut obj = Obj::new(handle);
        // A new object of a kind-6 group fades in (FUN_00199e98: FUN_001abf40 on +0x82c).
        if let (Some(_), Some(a)) = (appearing, appear.as_deref()) {
            obj.appear = a.start();
        }
        commands.entity(ent).insert(AiBrain {
            rt,
            obj,
            tag: format!("{}#{}", e.name, serial),
            enmp: e.kind.enmp,
            flags124: script.flags124,
            paired: script.paired.clone(),
            player_model: programs.player_model,
            started: false,
            dead_sent: false,
            errors_seen: 0,
            last_frame: 0.0,
            in_air: false,
            visible: true,
            applied_alpha: 1.0,
            original_modes: HashMap::new(),
        });
    }
}

/// The scripts, Sora's abilities, the mission manager (WP-7E: host 4.29, no-prize flag) and the global events.
type ScriptWorld<'w, 's> = (
    Res<'w, AiPrograms>,
    Option<Res<'w, crate::shared::abilities::Abilities>>,
    Option<Res<'w, crate::shared::struggle::Mission>>,
    MessageReader<'w, 's, crate::shared::struggle::GlobalEvent>,
);

/// One tick of every scripted enemy: deliver events, run the script, carry out what it asked for.
#[allow(clippy::too_many_arguments)]
fn ai_update(
    mut commands: Commands,
    data: Res<GameData>,
    world: Option<Res<World>>,
    cam: Query<(&Camera, &GlobalTransform), crate::menu::preview::GameCamera>,
    sora: Query<&Transform, (With<Sora>, Without<Enemy>)>,
    mut q: Query<(Entity, &mut Transform, &mut Enemy, &mut Fighter, &mut AiBrain)>,
    mut anims: Query<&mut Animator>,
    (mut hit_msgs, mut accepted, mut clash_msgs): (MessageReader<AiHit>, MessageReader<ReactionAccepted>, MessageReader<EnemyClash>),
    mut fx: MessageWriter<StartEffect>,
    mut game: MessageWriter<GameEffect>,
    mut player_cmds: MessageWriter<PlayerReactionCmd>,
    mut player_reaction: ResMut<PlayerReaction>,
    (mut trace, mut mailbox, mut table, appear): (ResMut<AiTrace>, ResMut<Mailbox>, ResMut<ObjTable>, Option<Res<AppearRamp>>),
    (programs, abilities, mission, mut global_events): ScriptWorld,
    (prize_pool, mut prize_drops, mut item_drops, mut collect_all): (
        Option<Res<crate::shared::prize::PrizePool>>,
        MessageWriter<crate::shared::prize::PrizeDrop>,
        MessageWriter<crate::shared::prize::ItemDrop>,
        MessageWriter<crate::shared::prize::CollectAll>,
    ),
    (summon, mut summon_stop_logged, mut forced_done, mut defeated): (
        Option<Res<crate::summon::SummonState>>,
        Local<bool>,
        Local<bool>,
        MessageWriter<crate::progress::EnemyDefeated>,
    ),
) {
    trace.tick += 1;
    // The summon arrival puts actor stop id 2 on every enemy (FUN_0016bef0(2, 0xfff7ffdc), `SummonState::stop`):
    // a stopped actor's update does not run, so its script gets no tick (WP-F1 item 3).
    let summon_stop = summon.as_deref().is_some_and(|s| s.stop);
    if summon_stop != *summon_stop_logged {
        *summon_stop_logged = summon_stop;
        info!("bdx: enemy scripts {} (summon arrival actor stop id 2, 0x35fec0 bit 3)", if summon_stop { "paused" } else { "resume" });
    }
    if trace.tick.is_multiple_of(SUMMARY_EVERY) {
        trace.summary();
    }
    let mut hits: HashMap<Entity, Vec<(bool, u8, i16)>> = HashMap::new();
    for h in hit_msgs.read() {
        hits.entry(h.target).or_default().push((h.real, h.reaction, h.atkp));
    }
    if let Some((name, reaction, at)) = forced_hit().filter(|(_, _, at)| !*forced_done && trace.tick >= *at)
        && let Some((ent, ..)) = q.iter().find(|(_, _, e, _, _)| e.alive() && e.name == name)
    {
        hits.entry(ent).or_default().push((true, *reaction, 0));
        *forced_done = true;
        info!("bdx: KH2_BDX_HIT: {name} gets a real hit with reaction {reaction} (tick {at})");
    }
    let mut reactions: HashMap<Entity, Vec<(u16, i16)>> = HashMap::new();
    for r in accepted.read() {
        reactions.entry(r.target).or_default().push((r.cmd, r.pos));
    }
    let mut clashes: HashMap<Entity, Vec<EnemyClash>> = HashMap::new();
    for c in clash_msgs.read() {
        clashes.entry(c.enemy).or_default().push(*c);
    }
    // Last tick's messages go to every scripted object (FUN_00187250), each with its own delay.
    let order: Vec<Entity> = q.iter().map(|(e, ..)| e).collect();
    let mut mail = dispatch_messages(&mut mailbox, &order);
    // Global events (FUN_00197b70 -> FUN_0016b470, object method +0x34): every scripted enemy gets event 10
    // with (event, argument) (WP-7E: the Struggle's result 159 / 160 picks the opponents' `gameset_voice_*`).
    let globals: Vec<crate::shared::struggle::GlobalEvent> = global_events.read().copied().collect();
    let mission_counters = mission.as_deref().and_then(|m| m.counters());
    let no_prizes = mission.as_deref().is_some_and(|m| m.no_prizes());
    let event_active = mission.as_deref().is_some_and(|m| m.event_active());
    let Ok(sp) = sora.single() else { return };
    let player = sp.translation / SCALE;
    // The other objects as they stand now, for the calls one object makes on another (1.15, 1.94, 1.98, 1.147).
    let others: Arc<HashMap<u32, Other>> = Arc::new(q.iter().map(|(_, tf, _, _, b)| (b.obj.handle, Other { pos: tf.translation / SCALE, slot: b.obj.slot })).collect());
    let appear_limit = appear.as_deref().map_or(1.0, |a| a.limit);
    let player_yaw = sp.rotation.to_euler(EulerRot::YXZ).0;
    let cam = cam.single().ok();
    for (ent, mut tf, mut e, mut f, mut brain) in &mut q {
        let Ok(mut anim) = anims.get_mut(e.body) else { continue };
        let rig = anim.rig.clone();
        let e = &mut *e;
        let AiBrain { rt, obj, tag, enmp, flags124, paired, player_model, started, dead_sent, errors_seen, last_frame, in_air, .. } = &mut *brain;

        // The world as the script finds it this tick.
        obj.motor.pos = tf.translation / SCALE;
        obj.motor.yaw = e.yaw;
        obj.motor.grounded = e.grounded;
        if !e.grounded && !*in_air {
            obj.motor.start_fall();
        }
        obj.hp = e.hp;
        obj.reacting = matches!(e.state, EState::Hurt | EState::Blown);
        obj.on_screen = cam
            .and_then(|(c, g)| c.world_to_ndc(g, tf.translation + Vec3::Y))
            .is_none_or(|n| n.x.abs() <= 1.0 && n.y.abs() <= 1.0 && (0.0..=1.0).contains(&n.z));
        let wrapped = anim.looping && anim.frame < *last_frame;
        obj.done = anim.slot.is_some() && (anim.finished || wrapped);
        obj.slot = anim.slot;
        obj.frame = anim.frame;
        // Host 1.13: the next queued motion starts when the playing one has ended.
        if obj.done && obj.motion_req.is_none() && !obj.motion_queue.is_empty() {
            let (slot, blend) = obj.motion_queue.remove(0);
            obj.motion_req = Some(crate::enemy_obj::MotionReq { slot, blend, start: None });
            obj.slot = Some(slot);
            obj.done = false;
            obj.blend_left = blend.max(0.0);
        }
        obj.blend_left = (obj.blend_left - 1.0).max(0.0);
        obj.spawn_kind = e.spawn_kind;
        obj.spawn_params = e.spawn_params;
        obj.trigger_bits = trigger_bits(&anim);
        let motion_events = motion_events(&anim);
        obj.hp_max = e.stats.max_hp;
        obj.hp_min = e.min_hp;
        obj.others = others.clone();
        obj.creatable = table.creatable.clone();
        obj.next_serial = table.serial;
        let env = Env {
            player,
            player_yaw,
            world: world.as_deref(),
            rig: &rig,
            flags124: *flags124,
            enmp: *enmp,
            player_serial: player_reaction.serial,
            player_motion_done: player_reaction.paired_done,
            paired: paired.as_ref(),
            player_model: *player_model,
            mission_counters,
            event_active,
        };
        let handle = obj.handle;
        let mut host = BevyHost { obj, env: &env, trace: &mut trace, tag: tag.as_str() };

        if !*started {
            *started = true;
            rt.spawn(&mut host, handle);
        }
        // Host 1.8 on this object by the script that made it (FUN_00186db0 on its mode machine).
        for m in std::mem::take(&mut host.obj.modes_asked) {
            host.trace.line(host.tag, &format!("mode {m:?} asked by the script that made this object (host 1.8)"));
            rt.set_mode_named(&mut host, &m);
        }
        // Death comes before the damage event: the dead mode refuses later mode switches.
        if e.hp <= 0 && !*dead_sent {
            *dead_sent = true;
            host.trace.line(host.tag, "hp 0: switching to mode dead");
            rt.set_mode_named(&mut host, "dead");
        }
        for (real, reaction, atkp) in hits.remove(&ent).unwrap_or_default() {
            host.obj.next_hit += 1;
            let h = HIT + host.obj.next_hit;
            host.obj.hits.insert(h, (real, reaction, atkp));
            host.trace.line(host.tag, &format!("event 6: hit, real {real}, reaction {reaction}"));
            rt.event(&mut host, 6, &[h]);
        }
        // An accepted reaction command: event 0xb with (player, command id, target record), the call
        // FUN_00177c20 makes through FUN_0016b420 on the target's persistent thread.
        for (cmd, pos) in reactions.remove(&ent).unwrap_or_default() {
            let rec = host.obj.new_record(handle, pos);
            let handled = rt.event(&mut host, 11, &[PLAYER, cmd as u32, rec]);
            host.trace.line(host.tag, &format!("event 11: reaction command {cmd} (position index {pos}, record {rec:#x}); handler {}", if handled { "ran" } else { "absent" }));
            if host.trace.level == 0 {
                info!("bdx {}: event 11 (reaction command {cmd}) delivered, handler {}", host.tag, if handled { "ran" } else { "absent" });
            }
        }
        // A clash of one of its attack volumes (0x1b2478): the running mode's clash hook, then event
        // 0x13 with (own volume, other volume).
        for c in clashes.remove(&ent).unwrap_or_default() {
            let (own, other) = host.obj.new_volumes(c.own_atkp, c.other_atkp);
            let handled = rt.clash(&mut host, own, other);
            host.trace.line(host.tag, &format!("event 19: clash, own attack {} against {} (volumes {own:#x}, {other:#x}); handler {}", c.own_atkp, c.other_atkp, if handled { "ran" } else { "absent" }));
            if host.trace.level == 0 && std::env::var("KH2_HITLOG").is_ok_and(|v| v != "0") {
                info!("bdx {}: event 19 (clash, attack {} against {}) delivered, handler {}", host.tag, c.own_atkp, c.other_atkp, if handled { "ran" } else { "absent" });
            }
        }
        for g in &globals {
            let handled = rt.event(&mut host, 10, &[g.id, g.arg as u32]);
            host.trace.line(host.tag, &format!("event 10: global event {} ({}); handler {}", g.id, g.arg, if handled { "ran" } else { "absent" }));
            if handled && matches!(g.id, crate::shared::struggle::EV_PLAYER_WON | crate::shared::struggle::EV_PLAYER_LOST) {
                info!("bdx {}: event 10 (global event {}) delivered", host.tag, g.id);
            }
        }
        // Frame trigger 7 of the motion (dispatcher 0x188498, case 7 at 0x188604): event 0x11 with (the object, the record's
        // halfwords +4 and +6) to the object's script (`FUN_0016c620`, thread at +0x484), the moment the motion's frame is crossed.
        for (code, arg) in motion_events {
            let handled = rt.event(&mut host, 17, &[handle, u32::from(code), u32::from(arg)]);
            host.trace.line(host.tag, &format!("event 17: motion frame trigger 7 (code {code}, argument {arg}); handler {}", if handled { "ran" } else { "absent" }));
        }
        // Messages (host 1.90 of any object, this one included): the mode of that name gets them.
        for (sender, name, delay) in mail.remove(&ent).unwrap_or_default() {
            host.trace.line(host.tag, &format!("message {name:?} from {sender:#x}, delay {delay:.2}"));
            rt.receive_message(&mut host, &name, sender, delay);
        }
        if summon_stop {
            host.trace.line(host.tag, "summon arrival stop: no script tick");
        } else {
            rt.update(&mut host);
        }
        // The objects the script made with host 1.0 and its calls on other objects wait for their owners.
        table.serial = obj.next_serial;
        table.creates.append(&mut obj.creates);
        table.remote.extend(obj.remote.drain(..).map(|(h, r)| (h, r, 0)));
        // The game's queue holds 0x40 entries; FUN_00187170 drops a message when it is full.
        for m in obj.outbox.drain(..) {
            if mailbox.queue.len() < MAILBOX_LEN {
                mailbox.queue.push(m);
            }
        }
        // Drops (host 1.92): the party is Sora alone, his ability block gives Jackpot and Lucky Lucky.
        // The prize objects and the dropped items' objects (FUN_001bd490) are made by `prize` (no pool
        // without its plugin: no prize objects then).
        for id in obj.drops.drain(..) {
            // A mission whose header has flag 0x40 (the Struggle's): FUN_001a3cb0 makes no prizes and no items.
            if no_prizes {
                info!("bdx {tag}: drop (host 1.92) of row {id}: none, the mission forbids prizes (header flag 0x40, DAT_0035d268 bit 0x200)");
                continue;
            }
            let (jackpot, lucky) = abilities.as_deref().map_or((0.0, 0.0), |a| (a.block().jackpot, a.block().lucky_lucky));
            match prize_row(&programs.prizes, id) {
                Some(row) => {
                    let free = prize_pool.as_deref().map_or(0, |p| p.free());
                    let d = compute_drop(row, 1.0 + jackpot, 1.0 + lucky, &mut mailbox.seed, free);
                    info!(
                        "bdx {tag}: drop (host 1.92) asked row {id}, used row {}: prizes per kind {:?}, items {:?} ({} prize objects, {} item objects)",
                        d.row,
                        d.prizes,
                        d.items,
                        d.launches.len(),
                        d.item_launches.len()
                    );
                    if !d.launches.is_empty() {
                        prize_drops.write(crate::shared::prize::PrizeDrop { from: ent, row: d.row, launches: d.launches });
                    }
                    if !d.item_launches.is_empty() {
                        item_drops.write(crate::shared::prize::ItemDrop { from: ent, launches: d.item_launches });
                    }
                }
                None => trace.note(tag, format!("drop row {id}: przt has neither it nor row 0x48")),
            }
        }
        // Host 6.31 (FUN_002eece8): every prize and item goes to the player.
        if std::mem::take(&mut obj.collect_all) {
            info!("bdx {tag}: host 6.31 collects every prize and item for the player");
            collect_all.write(crate::shared::prize::CollectAll);
        }
        // The status block the script rebuilt (host 2.9) and the HP changes it made (1.231, 1.155).
        if let Some(row) = obj.param_row.take() {
            e.stats = data.enemy_stats(row, 1);
            e.hp = e.stats.max_hp;
            *enmp = row;
            trace.line(tag, &format!("status block rebuilt from enmp row {row}: {} HP", e.hp));
        }
        // Host 1.311: byte i of the status block's damage scales = the row's weakness i times the percent, rounded up.
        for (i, pct) in std::mem::take(&mut obj.weak_scale) {
            let base = data.enemy_stats(*enmp, 1).weakness[i];
            e.stats.weakness[i] = (base * pct + 99) / 100;
        }
        e.min_hp = obj.hp_min;
        if obj.hp_add != 0 {
            let was_alive = e.hp > 0;
            e.hp = (e.hp + std::mem::take(&mut obj.hp_add)).max(e.min_hp).min(e.stats.max_hp);
            trace.line(tag, &format!("hp changed by the script: {}", e.hp));
            // Host 1.155 runs the object's HP method FUN_00186238, whose death method runs when part 0 reaches 0
            // (FUN_001b25d0 pays the experience); no player hit is behind it.
            if was_alive && e.hp <= 0 {
                defeated.write(crate::progress::EnemyDefeated { victim: ent, enmp: *enmp, by_player: false });
            }
        }
        for c in obj.player_cmds.drain(..) {
            let c = match c {
                PlayerReactionCmd::PairedMotion { slot, blend, serial, ticks, .. } => {
                    // A new motion on the player has not ended (bit 1 of his +0x160, host 1.14);
                    // `player_reaction.rs` plays it and sets the bit when it really ends.
                    player_reaction.paired_done = false;
                    PlayerReactionCmd::PairedMotion { from: ent, slot, blend, serial, ticks }
                }
                other => other,
            };
            trace.line(tag, &format!("player side: {c:?}"));
            player_cmds.write(c);
        }
        if let Some(s) = obj.player_serial.take() {
            player_reaction.serial = s;
        }
        for m in rt.core.mode_log.as_mut().map(std::mem::take).unwrap_or_default() {
            trace.line(tag, &format!("mode {m}"));
        }
        if trace.level > 0 && trace.tick.is_multiple_of(60) {
            let mode = rt.current_mode().unwrap_or_default();
            let dist = (obj.motor.pos - player).with_y(0.0).length();
            trace.line(tag, &format!("status: mode {mode} slot {:?} distance {dist:.0} cm hp {} grounded {}", obj.slot, e.hp, obj.motor.grounded));
        }
        for (pc, err) in &rt.core.errors[*errors_seen..] {
            warn!("bdx {tag}: script error {err:?} at {pc:#06x}");
        }
        *errors_seen = rt.core.errors.len();

        // ---- carry out the requests
        if !matches!(e.state, EState::Hurt | EState::Blown) {
            e.state = EState::Idle;
        }
        if let Some(req) = obj.motion_req.take() {
            let looping = rig.mset.get(req.slot).and_then(|s| s.motion.as_ref()).is_some_and(|m| m.frame_return < m.frame_end);
            anim.restart(Some(req.slot), looping, req.blend);
            if let Some(s) = req.start {
                anim.frame = s;
                anim.prev_frame = s;
            }
        }
        // Root motion of the playing motion, rotated by the facing the motion started with (as Sora's).
        let (root_t, root_yaw) = anim.root_delta;
        if trace.level >= 2 && (root_t != Vec3::ZERO || root_yaw != 0.0) {
            trace.line(tag, &format!("root motion slot {:?} frame {:.1} delta {root_t:?} yaw {root_yaw:.3} root {:?}", anim.slot, anim.frame, anim.root.translation));
        }
        obj.motor.yaw = wrap_angle(obj.motor.yaw + root_yaw);
        let root_move = Quat::from_rotation_y(obj.motor.yaw - anim.root.yaw) * root_t;
        let was_air = !obj.motor.grounded;
        // Mover 5 (`set_dash`) turns the movement vector into its smoothed velocity.
        let step = match obj.dash.as_mut() {
            Some((v, k)) => {
                *v = if obj.move_vel != Vec3::ZERO { *v * *k + obj.move_vel * (1.0 - *k) } else { *v * *k };
                *v
            }
            None => obj.move_vel,
        };
        let mut delta = step + root_move.with_y(0.0);
        if was_air {
            delta += obj.air_vel;
        }
        obj.move_vel = Vec3::ZERO;
        let mut landed = false;
        let before = obj.motor.pos;
        let mut asked = delta;
        if let Some(w) = world.as_deref() {
            let dy = was_air.then(|| obj.motor.jump_dy(data.sstm.fall_max) + root_move.y);
            asked.y = dy.unwrap_or(root_move.y - crate::motor::G);
            landed = obj.motor.move_by(w, delta, dy);
        }
        // Poses are kept relative to the motion's root, so a motion whose root rises (Soldier's appear
        // motion) is followed by the origin: on the ground the root's height is an offset above the floor.
        // A root below the floor (the Dusk's sinking motion 181) is stopped by it, as the game's
        // movement step (FUN_00181e00) collides the whole displacement.
        if !was_air && obj.motor.grounded {
            obj.motor.pos.y += anim.root.translation.y.max(0.0);
        }
        obj.want_vel = asked;
        obj.moved = obj.motor.pos - before;
        if trace.level >= 2 && obj.moved != Vec3::ZERO {
            trace.line(tag, &format!("moved {:?} to {:?} (mover delta {delta:?}, grounded {})", obj.moved, obj.motor.pos, obj.motor.grounded));
        }
        // Falling and landing motions of a jump started with the commit flag.
        let mut follow = None;
        if landed {
            obj.air_vel = Vec3::ZERO;
            obj.fall_pending = false;
            follow = (obj.land_motion >= 0).then_some(obj.land_motion);
        } else if !obj.motor.grounded && obj.fall_pending && !obj.motor.jump.rising() {
            obj.fall_pending = false;
            follow = (obj.fall_motion >= 0).then_some(obj.fall_motion);
        }
        if let Some(m) = follow {
            let looping = rig.mset.get(m as usize).and_then(|s| s.motion.as_ref()).is_some_and(|x| x.frame_return < x.frame_end);
            if rig.mset.has(m as usize) {
                trace.line(tag, &format!("motion request slot {m} (jump follow-up)"));
                anim.restart(Some(m as usize), looping, 4.0);
                obj.slot = Some(m as usize);
            }
        }
        *in_air = !obj.motor.grounded;
        // An object that jumped out of the map has no floor to land on and would stay "busy" for ever (EN16a: an `M_AL020_FIRE_RAW` of al10 b_40 was still falling 330 m from Sora 78 s later,
        // its script waiting in R0731 for the jump to end). The app removes it like a leaver once it is more than 30 m below Sora [inferred: the rule of the original was not found].
        if !obj.motor.grounded && obj.motor.pos.y < sp.translation.y / SCALE - FALL_OUT_UNITS && !obj.hidden {
            trace.line(tag, "fell out of the world: removed");
            obj.hidden = true;
        }
        *last_frame = anim.frame;
        e.grounded = obj.motor.grounded;
        e.yaw = obj.motor.yaw;
        e.vel = Vec3::ZERO;
        tf.translation = obj.motor.pos * SCALE;
        tf.rotation = Quat::from_rotation_y(e.yaw);
        f.hittable = obj.flags & (1 << FLAG_UNHITTABLE) == 0 && e.hp > 0 && e.kind.status;
        for id in obj.effects.drain(..) {
            if id == EFFECT_DEATH {
                game.write(GameEffect::Death { victim: ent });
            } else {
                fx.write(StartEffect { owner: e.body, id });
            }
        }
        obj.advance_fade();
        obj.appear.tick(appear_limit);
        let visible = !obj.hidden && obj.shown_alpha() > 0.0;
        if visible != brain.visible {
            brain.visible = visible;
            if let Ok(mut b) = commands.get_entity(e.body) {
                b.insert(if visible { Visibility::Inherited } else { Visibility::Hidden });
            }
        }
        // A hidden object is gone: the game's rule for deleting it was not found; the app removes it.
        if brain.obj.hidden {
            trace.line(&brain.tag, "hidden: despawned");
            commands.entity(ent).despawn();
        }
    }
}

/// The calls scripted objects made on other objects (`Obj::remote`: 1.148, 1.28, 1.70, 1.71), carried out on the owner of the
/// handle. A call on an object that is not there yet (one made in this tick) waits a few ticks.
fn ai_remote(mut table: ResMut<ObjTable>, mut q: Query<(&mut Transform, &mut AiBrain)>) {
    let ops = std::mem::take(&mut table.remote);
    for (handle, op, age) in ops {
        let Some((mut tf, mut brain)) = q.iter_mut().find(|(_, b)| b.obj.handle == handle) else {
            if age + 1 < REMOTE_PATIENCE {
                table.remote.push((handle, op, age + 1));
            }
            continue;
        };
        let o = &mut brain.obj;
        match op {
            Remote::Position(p) => {
                o.motor.pos = p;
                tf.translation = p * SCALE;
            }
            Remote::Mode(m) => o.modes_asked.push(m),
            Remote::Hide => {
                o.hidden = true;
                o.alpha = 0.0;
                o.fade = None;
            }
            Remote::SetFlag(bit) if (0..64).contains(&bit) => o.flags |= 1 << bit,
            Remote::ClearFlag(bit) if (0..64).contains(&bit) => o.flags &= !(1 << bit),
            Remote::SetFlag(_) | Remote::ClearFlag(_) => {}
        }
    }
}

/// The frame triggers of type 7 the last tick crossed, as (code, argument) = the record's halfwords `+4` and `+6` (dispatcher
/// `0x188498` case 7: event 0x11 with them). The motions of an enemy carry one at their first frame (code 2, or 3 on a
/// motion that should leave the object in the other state), docs/systems/script-host/enemies-m-ex8-9.md "Event 17".
fn motion_events(anim: &Animator) -> Vec<(u16, u16)> {
    let (Some(t), Some(m)) = (anim.triggers(), anim.motion()) else { return Vec::new() };
    if anim.frame == anim.prev_frame {
        return Vec::new();
    }
    let g = |f: f32| f * crate::data::LOGIC_HZ / m.fps.max(1.0);
    t.crossed(g(anim.prev_frame), g(anim.frame)).filter(|f| f.id == 7).map(|f| (f.params.first().copied().unwrap_or(0) as u16, f.params.get(1).copied().unwrap_or(0) as u16)).collect()
}

/// The motion controller's trigger bytes after the last advance (Obj::trigger_bits): range triggers
/// 16..19 active at the current frame, frame triggers 9..12 crossed by the last tick.
fn trigger_bits(anim: &Animator) -> [u8; 2] {
    let (Some(t), Some(m)) = (anim.triggers(), anim.motion()) else { return [0; 2] };
    let g = |f: f32| f * crate::data::LOGIC_HZ / m.fps.max(1.0);
    let mut bits = [0u8; 2];
    for k in 0..4u8 {
        if t.is_active(anim.gframe(), 16 + k) {
            bits[0] |= 1 << k;
        }
    }
    if anim.frame != anim.prev_frame {
        for ft in t.crossed(g(anim.prev_frame), g(anim.frame)).filter(|f| (9..=12).contains(&f.id)) {
            bits[1] |= 1 << (ft.id - 9);
        }
    }
    bits
}

/// Applies the script's fade to the enemy's materials (blended while it fades, as authored otherwise).
fn ai_fade(
    mut q: Query<(&Enemy, &mut AiBrain)>,
    children: Query<&Children>,
    handles: Query<&MeshMaterial3d<StandardMaterial>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    trace: Res<AiTrace>,
) {
    for (e, mut brain) in &mut q {
        let alpha = brain.obj.shown_alpha().clamp(0.0, 1.0);
        if (alpha - brain.applied_alpha).abs() < 1e-4 {
            continue;
        }
        brain.applied_alpha = alpha;
        let Ok(kids) = children.get(e.body) else { continue };
        if trace.level >= 2 {
            trace.line(&brain.tag, &format!("fade: alpha {alpha:.2} on {} parts", kids.iter().filter(|k| handles.contains(*k)).count()));
        }
        for k in kids.iter() {
            let Ok(h) = handles.get(k) else { continue };
            let Some(mut m) = materials.get_mut(&h.0) else { continue };
            let original = *brain.original_modes.entry(h.0.id()).or_insert_with(|| m.alpha_mode);
            m.base_color = m.base_color.with_alpha(alpha);
            m.alpha_mode = if alpha < 1.0 { AlphaMode::Blend } else { original };
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::enemy_obj::SELF_BASE;

    /// FUN_00187250: every message goes to every object of the list, each drawing its own delay from
    /// the generator in list order; a delay of 0 draws nothing.
    #[test]
    fn messages_go_to_every_object_with_their_own_delay() {
        let mut m = Mailbox::default();
        m.queue.push((SELF_BASE, "dusk_surprise".into(), 30.0));
        m.queue.push((SELF_BASE, "now".into(), 0.0));
        let out = dispatch_messages(&mut m, &[1u8, 2, 3]);
        assert!(m.queue.is_empty());
        let mut seed = 1u32;
        for k in [1u8, 2, 3] {
            let d = kh2_bdx::message_delay(&mut seed, 30.0);
            assert_eq!(out[&k][0], (SELF_BASE, "dusk_surprise".to_string(), d));
            assert_eq!(out[&k][1].2, 0.0);
        }
        assert_eq!(m.seed, seed);
    }


    /// The sender of event 17 (WP-DYN1): the motion's frame trigger 7 (dispatcher 0x188498, jump-table entry 7 at 0x188604
    /// sends event 0x11 with the object and the record's halfwords +4 and +6; PCSX2 read the table at 0x37aa78 live). M_EX900's
    /// motions carry them (`kh2 mset obj/M_EX900.mset` prints `7@0[2, 0]` on most slots and `7@0[3, 0]` on the dive motions) and its
    /// script has the entry for event 17 (code 2 clears +0x18 and puts the weakness byte back to 100, code 3 sets +0x18 and
    /// the byte to 0). Also lists which kinds of `KINDS` have an event 17 entry. Needs `KH2_ISO`.
    #[test]
    fn m_ex900_motions_send_event_17_to_its_script() {
        let Ok(iso) = crate::assets::iso_path() else { return };
        if !iso.exists() {
            return;
        }
        let disc = Disc::open(&iso);
        let program = |model: &str| {
            let file = disc.read(&format!("obj/{model}.mdlx"))?;
            let entry = kh2_bar::parse(&file).ok()?.into_iter().find(|e| e.kind == 3).map(|e| e.data.to_vec())?;
            Program::parse(&entry).ok()
        };
        let prog = program("M_EX900").expect("M_EX900 script");
        assert!(prog.entries.iter().any(|e| e.id == 17), "the script has an entry for event 17");
        let mset = kh2_anim::Mset::parse(&disc.read("obj/M_EX900.mset").expect("M_EX900.mset")).expect("mset parses");
        let mut codes = std::collections::BTreeMap::<i16, usize>::new();
        for slot in mset.slots.iter().flatten() {
            for f in slot.triggers.frames.iter().filter(|f| f.id == 7) {
                *codes.entry(f.params.first().copied().unwrap_or(0)).or_default() += 1;
            }
        }
        assert!(codes.get(&2).is_some_and(|n| *n > 10), "code 2 on most motions: {codes:?}");
        assert!(codes.get(&3).is_some_and(|n| *n > 3), "code 3 on the dive motions: {codes:?}");
        let with: Vec<&str> = crate::enemy::KINDS.iter().filter(|k| program(k.model).is_some_and(|p| p.entries.iter().any(|e| e.id == 17))).map(|k| k.name).collect();
        eprintln!("m_ex900_motions_send_event_17_to_its_script: codes {codes:?}; kinds with an event 17 entry: {with:?}");
    }

    /// `motion_events` reads the records the way the dispatcher does: type 7, code = first halfword, argument = second, and only
    /// when the frame moved over them.
    #[test]
    fn motion_events_report_crossed_type_7_records() {
        let t = kh2_anim::Triggers { ranges: Vec::new(), frames: vec![
            kh2_anim::FrameTrigger { frame: 0, id: 7, params: vec![2, 0] },
            kh2_anim::FrameTrigger { frame: 10, id: 7, params: vec![3, 9] },
            kh2_anim::FrameTrigger { frame: 10, id: 9, params: vec![] },
        ] };
        let crossed = |a: f32, b: f32| t.crossed(a, b).filter(|f| f.id == 7).map(|f| (f.params[0], f.params[1])).collect::<Vec<_>>();
        assert_eq!(crossed(-1.0, 1.0), vec![(2, 0)], "the first tick covers frame 0");
        assert_eq!(crossed(1.0, 12.0), vec![(3, 9)], "type 9 is not an event");
        assert!(crossed(10.0, 12.0).is_empty(), "a trigger is crossed once");
    }
}
