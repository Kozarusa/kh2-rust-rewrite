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
//! docs/enemy-ai.md for the mapping and the gaps.
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
use crate::enemy_obj::{Obj, FLAG_UNHITTABLE, HIT, OBJ_SIZE, PLAYER, SELF_BASE};
use crate::fx::{GameEffect, StartEffect, EFFECT_DEATH};
use crate::motor::wrap_angle;
use crate::reaction_cmd::{PlayerReaction, PlayerReactionCmd, ReactionAccepted};
use crate::schedule::Sim;
use crate::skel::Animator;
use crate::sora::Sora;
use crate::world::World;
use crate::SCALE;

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

/// The BDX runtime of one enemy. Its presence hands the enemy's behaviour to the script.
#[derive(Component)]
pub struct AiBrain {
    rt: Runtime,
    obj: Obj,
    tag: String,
    enmp: u16,
    flags124: bool,
    paired: Arc<HashMap<usize, Vec<(String, f32)>>>,
    player_model: u8,
    started: bool,
    dead_sent: bool,
    errors_seen: usize,
    /// Animator frame after the previous tick (a looping motion wrapped when it goes down).
    last_frame: f32,
    in_air: bool,
    visible: bool,
    applied_alpha: f32,
    original_modes: HashMap<AssetId<StandardMaterial>, AlphaMode>,
}

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
            .add_message::<AiHit>()
            .add_systems(Startup, load_programs)
            .add_systems(
                FixedUpdate,
                (
                    ai_attach.after(crate::enemy::spawn_waves),
                    ai_update.after(ai_attach).before(crate::enemy::think),
                    ai_fade.after(crate::enemy::think),
                )
                    .in_set(Sim::Enemy),
            );
    }
}

/// Reads the BDX entry (BAR type 3) of each scripted enemy's object file and its `objentry` flags.
fn load_programs(disc: Res<Disc>, mut programs: ResMut<AiPrograms>) {
    let entries: Vec<(&'static str, &str)> = crate::enemy::KINDS.iter().map(|k| (k.name, k.model)).collect();
    let rows = {
        let Ok(mut a) = disc.0.lock() else { return };
        kh2_tables::load::<t::Objentry>(&mut a, &t::OBJENTRY).map(|d| d.rows).unwrap_or_default()
    };
    // The player's objentry byte +7 (`WeaponJoint`): the paired motions of reaction commands hold one
    // sub-motion per player model, named by `%03d0` of it (FUN_0017ef98 formats the key with the format
    // string at 0x37a598, FUN_0017ee40 looks the name up). Sora's row is P_EX100.
    programs.player_model = rows.iter().find(|r| r.model_name == "P_EX100").map_or(1, |r| r.weapon_joint);
    programs.prizes = disc
        .read("00battle.bin")
        .and_then(|b| kh2_bar::parse(&b).ok().and_then(|e| e.iter().find(|x| x.tag == "przt").map(|x| parse_przt(x.data))))
        .unwrap_or_default();
    if programs.prizes.is_empty() {
        warn!("bdx: 00battle.bin przt not readable; drops are not computed");
    }
    for (kind, model) in entries {
        let paired = disc.read(&format!("obj/{model}.mset")).map(|m| paired_motions(&m)).unwrap_or_default();
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

/// Gives every new enemy its script runtime. Runs right after the wave spawner, so the
/// script's first frame is the frame the enemy appears in.
fn ai_attach(mut commands: Commands, programs: Res<AiPrograms>, q: Query<(Entity, &Enemy), Without<AiBrain>>, mut serial: Local<u32>) {
    for (ent, e) in &q {
        let Some(script) = programs.by_kind.get(e.kind.name) else { continue };
        *serial += 1;
        let mut rt = Runtime::new(script.prog.clone());
        rt.shared.rng = 0x9E37_79B9u32 ^ serial.wrapping_mul(0x85EB_CA6B);
        rt.core.mode_log = Some(Vec::new());
        commands.entity(ent).insert(AiBrain {
            rt,
            obj: Obj::new(SELF_BASE + (*serial % 0x1000) * OBJ_SIZE as u32),
            tag: format!("{}#{}", e.name, *serial),
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

/// One tick of every scripted enemy: deliver events, run the script, carry out what it asked for.
#[allow(clippy::too_many_arguments)]
fn ai_update(
    mut commands: Commands,
    data: Res<GameData>,
    world: Option<Res<World>>,
    cam: Query<(&Camera, &GlobalTransform), With<Camera3d>>,
    sora: Query<&Transform, (With<Sora>, Without<Enemy>)>,
    mut q: Query<(Entity, &mut Transform, &mut Enemy, &mut Fighter, &mut AiBrain)>,
    mut anims: Query<&mut Animator>,
    (mut hit_msgs, mut accepted, mut clash_msgs): (MessageReader<AiHit>, MessageReader<ReactionAccepted>, MessageReader<EnemyClash>),
    mut fx: MessageWriter<StartEffect>,
    mut game: MessageWriter<GameEffect>,
    mut player_cmds: MessageWriter<PlayerReactionCmd>,
    mut player_reaction: ResMut<PlayerReaction>,
    (mut trace, mut mailbox): (ResMut<AiTrace>, ResMut<Mailbox>),
    (programs, abilities): (Res<AiPrograms>, Option<Res<crate::abilities::Abilities>>),
) {
    trace.tick += 1;
    if trace.tick.is_multiple_of(SUMMARY_EVERY) {
        trace.summary();
    }
    let mut hits: HashMap<Entity, Vec<bool>> = HashMap::new();
    for h in hit_msgs.read() {
        hits.entry(h.target).or_default().push(h.real);
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
    let Ok(sp) = sora.single() else { return };
    let player = sp.translation / SCALE;
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
        obj.blend_left = (obj.blend_left - 1.0).max(0.0);
        obj.spawn_kind = e.spawn_kind;
        obj.spawn_params = e.spawn_params;
        obj.trigger_bits = trigger_bits(&anim);
        obj.hp_max = e.stats.max_hp;
        obj.hp_min = e.min_hp;
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
        };
        let handle = obj.handle;
        let mut host = BevyHost { obj, env: &env, trace: &mut trace, tag: tag.as_str() };

        if !*started {
            *started = true;
            rt.spawn(&mut host, handle);
        }
        // Death comes before the damage event: the dead mode refuses later mode switches.
        if e.hp <= 0 && !*dead_sent {
            *dead_sent = true;
            host.trace.line(host.tag, "hp 0: switching to mode dead");
            rt.set_mode_named(&mut host, "dead");
        }
        for real in hits.remove(&ent).unwrap_or_default() {
            host.obj.next_hit += 1;
            let h = HIT + host.obj.next_hit;
            host.obj.hits.insert(h, real);
            host.trace.line(host.tag, &format!("event 6: hit, real {real}"));
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
        // Messages (host 1.90 of any object, this one included): the mode of that name gets them.
        for (sender, name, delay) in mail.remove(&ent).unwrap_or_default() {
            host.trace.line(host.tag, &format!("message {name:?} from {sender:#x}, delay {delay:.2}"));
            rt.receive_message(&mut host, &name, sender, delay);
        }
        rt.update(&mut host);
        // The game's queue holds 0x40 entries; FUN_00187170 drops a message when it is full.
        for m in obj.outbox.drain(..) {
            if mailbox.queue.len() < MAILBOX_LEN {
                mailbox.queue.push(m);
            }
        }
        // Drops (host 1.92): the party is Sora alone, his ability block gives Jackpot and Lucky Lucky.
        for id in obj.drops.drain(..) {
            let (jackpot, lucky) = abilities.as_deref().map_or((0.0, 0.0), |a| (a.block().jackpot, a.block().lucky_lucky));
            match prize_row(&programs.prizes, id) {
                Some(row) => {
                    let d = compute_drop(row, 1.0 + jackpot, 1.0 + lucky, &mut mailbox.seed);
                    info!("bdx {tag}: drop (host 1.92) asked row {id}, used row {}: prizes per kind {:?}, items {:?} (no prize objects in the app)", d.row, d.prizes, d.items);
                }
                None => trace.note(tag, format!("drop row {id}: przt has neither it nor row 0x48")),
            }
        }
        // The status block the script rebuilt (host 2.9) and the HP changes it made (1.231, 1.155).
        if let Some(row) = obj.param_row.take() {
            e.stats = data.enemy_stats(row, 1);
            e.hp = e.stats.max_hp;
            *enmp = row;
            trace.line(tag, &format!("status block rebuilt from enmp row {row}: {} HP", e.hp));
        }
        e.min_hp = obj.hp_min;
        if obj.hp_add != 0 {
            e.hp = (e.hp + std::mem::take(&mut obj.hp_add)).max(e.min_hp).min(e.stats.max_hp);
            trace.line(tag, &format!("hp changed by the script: {}", e.hp));
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
        for (_, err) in &rt.core.errors[*errors_seen..] {
            warn!("bdx {tag}: script error {err:?}");
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
        *last_frame = anim.frame;
        e.grounded = obj.motor.grounded;
        e.yaw = obj.motor.yaw;
        e.vel = Vec3::ZERO;
        tf.translation = obj.motor.pos * SCALE;
        tf.rotation = Quat::from_rotation_y(e.yaw);
        f.hittable = obj.flags & (1 << FLAG_UNHITTABLE) == 0 && e.hp > 0;
        for id in obj.effects.drain(..) {
            if id == EFFECT_DEATH {
                game.write(GameEffect::Death { victim: ent });
            } else {
                fx.write(StartEffect { owner: e.body, id });
            }
        }
        obj.advance_fade();
        let visible = !obj.hidden && obj.alpha > 0.0;
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
        let alpha = brain.obj.alpha.clamp(0.0, 1.0);
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

}
