//! Enemy AI runtime: Shadow, Soldier and Dusk run under the game's own BDX scripts
//! (`obj/M_EX020.mdlx`, `obj/M_EX010.mdlx`, `obj/M_EX990.mdlx`, read from the disc).
//!
//! One `kh2_bdx::Runtime` per enemy runs once per 60 Hz tick (`Runtime::update`). The script talks to
//! the world through `kh2_bdx::Host::call`; `BevyHost` below answers each `Call` from a small model of
//! the scripted object (`Obj`: position and facing, the motion it asked for, flags, fade, movement
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
//! (the app is the game rotated 180 degrees about X). `to_game` / `from_game` convert; yaw is
//! measured in the app's space inside `Obj`.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::Arc;

use bevy::color::Alpha;
use bevy::prelude::*;
use kh2_bdx::{Call, Host, Program, Runtime, Value, Vec4};
use kh2_tables::tables as t;

use crate::assets::Disc;
use crate::combat::Fighter;
use crate::data::GameData;
use crate::enemy::{EState, Enemy, EnemyClash};
use crate::enemy_obj::*;
use crate::fx::{GameEffect, StartEffect, EFFECT_DEATH};
use crate::motor::wrap_angle;
use crate::reaction_cmd::{PlayerReaction, PlayerReactionCmd, ReactionAccepted};
use crate::schedule::Sim;
use crate::skel::{Animator, Rig};
use crate::sora::Sora;
use crate::world::World;
use crate::SCALE;

/// Turn rate in radians per tick when a script passes speed 0 ("the object's own rate"). The game reads
/// it from the object (+0x11c); where that value comes from was not found, this is the rate the
/// placeholder enemies used (5 rad/s).
const TURN_DEFAULT: f32 = 5.0 / 60.0;
/// Ground height tolerance of `probe_ground`, centimetres (the game: halfword at +0xe of a data record
/// of the object times 0.4; the record was not decoded, this is the step height of `Motor`).
const PROBE_TOLERANCE: f32 = 40.0;
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

// ---------------------------------------------------------------------------------------------------
// Drops (host 1.92)

/// One row of the prize table `przt` (`00battle.bin`; FUN_00193610 hands it to FUN_001a41c8): 0x18
/// bytes, sorted by the halfword id at +0 (the bsearch of FUN_001a41d8 compares it, 0x331c48).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PrizeRow {
    pub id: u16,
    /// Bytes +2..+0xb: how many prizes of each of the nine kinds (FUN_001a4240 reads byte +2 + kind).
    pub counts: [u8; 9],
    /// +0xc: three (item id, chance) halfword pairs.
    pub items: [(u16, u16); 3],
}

/// The rows of a `przt` entry: count at +4, rows from +8.
pub fn parse_przt(d: &[u8]) -> Vec<PrizeRow> {
    let u16_at = |o: usize| d.get(o..o + 2).map(|b| u16::from_le_bytes([b[0], b[1]]));
    let Some(n) = d.get(4..8).map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]) as usize) else { return Vec::new() };
    (0..n)
        .map_while(|k| {
            let o = 8 + k * 0x18;
            let r = d.get(o..o + 0x18)?;
            let mut counts = [0u8; 9];
            counts.copy_from_slice(&r[2..11]);
            let item = |j: usize| Some((u16_at(o + 0xc + 4 * j)?, u16_at(o + 0xe + 4 * j)?));
            Some(PrizeRow { id: u16_at(o)?, counts, items: [item(0)?, item(1)?, item(2)?] })
        })
        .collect()
}

/// FUN_001a41d8: the row of `id`, else the row of 0x48.
pub fn prize_row(rows: &[PrizeRow], id: i32) -> Option<&PrizeRow> {
    let find = |k: i32| rows.iter().find(|r| r.id as i32 == k);
    find(id).or_else(|| find(0x48))
}

/// What a drop is made of: prizes per kind and the items that won their roll.
#[derive(Clone, Debug, PartialEq)]
pub struct Drop {
    pub row: u16,
    pub prizes: [u32; 9],
    pub items: Vec<u16>,
}

/// FUN_001a3cb0 up to the prize objects: kind `k` drops `ceil(counts[k] * orb)` prizes (FUN_001a4240,
/// FUN_00300008 is a ceiling), `orb` being 1.0 (DAT_01c8a7e4, reset by FUN_00190370) plus each party
/// member's Jackpot float (ability block +0x60, FUN_00190788). Each prize object takes three steps of
/// the generator (FUN_001a3b28: seed * 0x10dcd, * 0x10dcd^2, then seed *= 0x10dcd^3). Then each item
/// pair in turn: one step, and the item drops when `seed * 2^-32 * 100 < chance * item` (`item` 1.0
/// plus each member's Lucky Lucky float, +0x64; the seed read unsigned). The flags of 0x35d268 (0x200
/// no prizes, 0x400 no items) and the player state of FUN_001ce6c0 are taken as clear.
pub fn compute_drop(row: &PrizeRow, orb: f32, item: f32, seed: &mut u32) -> Drop {
    let mut prizes = [0u32; 9];
    for (k, p) in prizes.iter_mut().enumerate() {
        *p = (row.counts[k] as f32 * orb).ceil().max(0.0) as u32;
        for _ in 0..*p {
            *seed = seed.wrapping_mul(0x10dcd).wrapping_mul(0x10dcd).wrapping_mul(0x10dcd);
        }
    }
    let mut items = Vec::new();
    for &(id, chance) in &row.items {
        *seed = seed.wrapping_mul(0x10dcd);
        if (*seed as f32) * 2.328_306_4e-10 * 100.0 < chance as f32 * item {
            items.push(id);
        }
    }
    Drop { row: row.id, prizes, items }
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

/// Classification of a host call, for the trace and the summary.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum St {
    /// Does what the game's function does (read from the game's code).
    Impl,
    /// Does the visible part; some of what the game does is not reproduced.
    Approx,
    /// Accepted and answered with a default; nothing in the app consumes it.
    Stub,
    /// Not implemented: the call fails visibly.
    Missing,
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
    fn line(&self, tag: &str, msg: &str) {
        if self.level > 0 {
            info!("bdx {tag} t{}: {msg}", self.tick);
        }
    }

    /// One log line per distinct note (data gaps, approximations hit).
    fn note(&mut self, tag: &str, msg: String) {
        if self.notes.insert(msg.clone()) {
            warn!("bdx {tag}: {msg}");
        }
    }

    fn record(&mut self, tag: &str, c: &Call, v: &Value, st: St, why: &str) {
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

/// `group.index` of the host function behind a call (the mode manager's hooks are not host functions).
fn call_id(c: &Call) -> (u16, u16) {
    match c {
        Call::Player => (1, 3),
        Call::Position { .. } => (1, 147),
        Call::FacingVector { .. } => (1, 201),
        Call::ObjVec860 { .. } => (1, 62),
        Call::ObjVec850 { .. } => (1, 64),
        Call::Target { .. } => (2, 23),
        Call::TargetPosition { .. } => (1, 120),
        Call::LastAttacker { .. } => (2, 88),
        Call::SetTargetMode { .. } => (1, 121),
        Call::MotionDone { .. } => (1, 14),
        Call::MotionState { .. } => (1, 15),
        Call::MotionLength { .. } => (1, 67),
        Call::MotionEnd { .. } => (1, 68),
        Call::OnScreen { .. } => (1, 56),
        Call::Airborne { .. } => (1, 57),
        Call::Busy { .. } => (1, 60),
        Call::HitStun { .. } => (2, 5),
        Call::SpawnParam { .. } => (1, 39),
        Call::SpawnKind { .. } => (1, 110),
        Call::GlobalByte33e830 => (1, 23),
        Call::GlobalFlag4 => (1, 171),
        Call::GlobalFlag80 => (1, 206),
        Call::EventActive => (1, 34),
        Call::GlobalByte301 => (1, 301),
        Call::HitIsReal { .. } => (2, 8),
        Call::Flag124 { .. } => (1, 124),
        Call::Flag309 { .. } => (1, 309),
        Call::ProbeGround { .. } => (1, 119),
        Call::PlayMotion { .. } => (1, 11),
        Call::PlayMotionChecked { .. } => (1, 12),
        Call::PlayMotionAt { .. } => (1, 61),
        Call::FaceDirection { .. } => (1, 4),
        Call::FaceToward { .. } => (1, 79),
        Call::TurnToward { .. } => (1, 80),
        Call::SetMove { .. } => (1, 111),
        Call::SetVelocity { .. } => (1, 89),
        Call::StartMove { .. } => (1, 55),
        Call::StartMoveAlt { .. } => (1, 168),
        Call::Hide { .. } => (1, 28),
        Call::FadeOut { .. } => (1, 19),
        Call::FadeIn { .. } => (1, 20),
        Call::GroundMarker { .. } => (1, 92),
        Call::SetFlag { .. } => (1, 70),
        Call::ClearFlag { .. } => (1, 71),
        Call::SetFlag26 { .. } => (1, 65),
        Call::SetFlag768 { .. } => (1, 104),
        Call::ClearFlag768 { .. } => (1, 105),
        Call::SpawnAttached { .. } => (1, 87),
        Call::ReleaseEffect { .. } => (0, 79),
        Call::ObjMethod44 { .. } => (1, 133),
        Call::ObjMethod48 { .. } => (1, 134),
        Call::MoverMethod { .. } => (1, 128),
        Call::SetObjParam { .. } => (2, 9),
        Call::FreeTarget { .. } => (1, 131),
        Call::SendMessage { .. } => (1, 90),
        Call::RemoteSetMode { .. } => (6, 58),
        Call::FindObject { .. } => (1, 114),
        Call::ModeFrozen { .. } => (0xffff, 0),
        Call::ModeInterrupted { .. } => (0xffff, 1),
        Call::ModeMayStart { .. } => (0xffff, 2),
        Call::Func { group, index, .. } | Call::Unknown { group, index, .. } => (*group, *index),
    }
}

// ---------------------------------------------------------------------------------------------------
// Game space

/// FUN_0017bbc0 (host 0.43): an angle into -pi..pi with fmod by 2 pi, from above for a positive angle
/// and from below otherwise (pi and 2 pi are the floats at 0x37a528 / 0x37a52c, 0x40490fdb / 0x40c90fdb).
fn game_wrap_angle(x: f32) -> f32 {
    let (pi, two_pi) = (f32::from_bits(0x4049_0fdb), f32::from_bits(0x40c9_0fdb));
    if x > 0.0 { (x + pi) % two_pi - pi } else { pi - (pi - x) % two_pi }
}

/// What the host reads from the app, per enemy and tick.
struct Env<'a> {
    player: Vec3,
    player_yaw: f32,
    world: Option<&'a World>,
    rig: &'a Rig,
    flags124: bool,
    enmp: u16,
    /// The player's +0x158 (the serial of the paired motion last started on him) and whether that
    /// motion has ended (bit 1 of his +0x160), from `reaction_cmd::PlayerReaction`.
    player_serial: i32,
    player_motion_done: bool,
    paired: &'a HashMap<usize, Vec<(String, f32)>>,
    player_model: u8,
}

struct BevyHost<'a> {
    obj: &'a mut Obj,
    env: &'a Env<'a>,
    trace: &'a mut AiTrace,
    tag: &'a str,
}

fn imp(v: Value) -> (Value, St, &'static str) {
    (v, St::Impl, "")
}

fn approx(v: Value, why: &'static str) -> (Value, St, &'static str) {
    (v, St::Approx, why)
}

fn stub(v: Value, why: &'static str) -> (Value, St, &'static str) {
    (v, St::Stub, why)
}

fn missing(v: Value, why: &'static str) -> (Value, St, &'static str) {
    (v, St::Missing, why)
}

impl BevyHost<'_> {
    fn pos_of(&self, obj: u32) -> Option<Vec3> {
        if obj == self.obj.handle {
            Some(self.obj.motor.pos)
        } else if obj == PLAYER || obj == TARGET {
            Some(self.env.player)
        } else {
            None
        }
    }

    /// Starts a motion (`checked`: not when the slot already plays). Slots are the script's motion
    /// numbers (the object's own motion sets are not remapped: FUN_0017da30 returns the number itself
    /// for them).
    fn play(&mut self, motion: i32, blend: f32, start: Option<f32>, checked: bool) {
        let o = &mut *self.obj;
        let Ok(slot) = usize::try_from(motion) else { return };
        if !self.env.rig.mset.has(slot) {
            self.trace.note(self.tag, format!("motion {motion} is not in the object's motion set; request ignored"));
            return;
        }
        if checked && o.slot == Some(slot) {
            return;
        }
        self.trace.line(self.tag, &format!("motion request slot {slot} blend {blend}{}", if checked { " (checked)" } else { "" }));
        o.slot = Some(slot);
        o.done = false;
        o.frame = start.unwrap_or_else(|| self.env.rig.mset.get(slot).and_then(|s| s.motion.as_ref()).map_or(0.0, |m| m.frame_start));
        o.blend_left = blend.max(0.0);
        o.motion_req = Some(MotionReq { slot, blend, start });
    }

    /// Host 1.63 (FUN_00172b00 -> FUN_0017f400): the controller has a motion, no blend left (+0x34 <= 0)
    /// and its time (+0x30) has reached the value of the motion's virtual method +0x1c. That value is
    /// the return frame [inferred: FUN_0017e3b8 wraps a looping motion to time - end + that value, and
    /// FUN_0017f558 (host 1.68) answers the same frame].
    fn motion_near_end(&self) -> bool {
        let o = &*self.obj;
        let Some(m) = o.slot.and_then(|s| self.env.rig.mset.get(s)).and_then(|s| s.motion.as_ref()) else { return false };
        o.blend_left <= 0.0 && m.frame_return <= o.frame
    }

    /// Host 1.98 (FUN_00173088): the object's position with the height of the floor under it
    /// (FUN_00183808 -> FUN_00183750: a line from 200 above the object down 100000 below it,
    /// DAT_0037a894 and DAT_0037a890; no floor keeps the object's own height).
    fn ground_position(&self, obj: u32) -> Option<Vec3> {
        let p = self.pos_of(obj)?;
        let g = self.env.world.and_then(|w| w.ground(Vec3::new(p.x, 0.0, p.z), p.y + 200.0, 100_200.0));
        Some(g.map_or(p, |g| p.with_y(g.y)))
    }

    /// Record `i` of the HP records of the status block: (current, maximum, lowest); parts other than
    /// 0 have no HP in the app (an `enmp` row with one health value gives them 0, FUN_0019f770).
    fn hp_record(&self, i: i32) -> [i32; 3] {
        if i == 0 {
            [self.obj.hp, self.obj.hp_max, self.obj.hp_min]
        } else {
            [0; 3]
        }
    }

    fn turn_toward(&mut self, dir: Vec4, speed: f32) -> bool {
        let target = yaw_of(dir);
        let step = if speed == 0.0 { TURN_DEFAULT } else { speed };
        let m = &mut self.obj.motor;
        let d = wrap_angle(target - m.yaw);
        if d > step {
            m.yaw = wrap_angle(m.yaw + step);
            true
        } else if d < -step {
            m.yaw = wrap_angle(m.yaw - step);
            true
        } else {
            m.yaw = target;
            false
        }
    }

    /// Host 1.119 (FUN_001834f8): a line from the point `a` above the object, `dir.w` ahead; a wall in the
    /// way gives 3; then the floor under the far point is looked up (down to `b` below the object's
    /// height) and compared with the object's own: none gives 3, higher than the tolerance 1, lower 2,
    /// else 0. The game's two collision line tests are the wall test and the floor lookup of `World`.
    fn probe(&self, dir: Vec4, a: f32, b: f32) -> i32 {
        let Some(w) = self.env.world else { return 0 };
        let d = from_game(dir).normalize_or_zero();
        let start = self.obj.motor.pos + Vec3::Y * a;
        let far = start + d * dir[3];
        if w.blocked_m(start * SCALE, far * SCALE) {
            return 3;
        }
        let Some(g) = w.ground(Vec3::new(far.x, 0.0, far.z), far.y, a + b) else { return 3 };
        let here = self.obj.motor.pos.y;
        if g.y > here + PROBE_TOLERANCE {
            1
        } else if g.y < here - PROBE_TOLERANCE {
            2
        } else {
            0
        }
    }

    fn start_move(&mut self, dir: Vec4, motion: i32, blend: f32, commit: bool) {
        // FUN_0017b278 passes the y of the direction to FUN_0017b1e0, which reads a negative y as a rise
        // of that many units (Y is down) and y >= 0 as a fall that starts that far above the object.
        // The scripts (Shadow's pounce 100, Soldier's jump 260) pass positive values, then wait for
        // `airborne`, so the app jumps |y|; the rest of the direction, normalised and scaled by w, is the
        // horizontal speed.
        // The jump installs its own mover (FUN_0017b278 -> mover 1), which ends a dash.
        self.obj.dash = None;
        let h = dir[1].abs();
        let d = from_game([dir[0], 0.0, dir[2], 0.0]).normalize_or_zero();
        self.obj.air_vel = Vec3::new(d.x, 0.0, d.z) * dir[3];
        if h > 0.0 {
            self.obj.motor.start_jump(h);
            self.obj.fall_pending = true;
        }
        self.play(motion, blend, None, false);
        // FUN_0017b1b0 resets the follow-up motions (none for falling, 5 for landing); FUN_001712d0 with
        // the commit flag sets them to motion + 1 and motion + 2. The landing and falling code reads
        // them (FUN_0017b9e0, FUN_0017b8f8).
        self.obj.fall_motion = if commit { motion + 1 } else { -1 };
        self.obj.land_motion = if commit { motion + 2 } else { 5 };
    }

    fn exec(&mut self, c: &Call) -> (Value, St, &'static str) {
        let me = self.obj.handle;
        match c {
            Call::Player => imp(Value::Handle(PLAYER)),
            Call::Position { obj } => match self.pos_of(*obj) {
                Some(p) => imp(Value::Vec(to_game(p))),
                None => missing(Value::Vec([0.0; 4]), "position of an object other than self and the player"),
            },
            Call::FacingVector { obj } => {
                let yaw = if *obj == me { self.obj.motor.yaw } else { self.env.player_yaw };
                imp(Value::Vec([yaw.sin(), 0.0, -yaw.cos(), 0.0]))
            }
            // FUN_00181e00, the object's movement step, writes both every tick: +0x850 the displacement
            // the object asked for (mover, movement vector, push), +0x860 the position after collision
            // minus the position before, each times DAT_00356f48 (a frame-step factor, 1.0 at start;
            // taken as one 60 Hz tick).
            Call::ObjVec850 { obj } | Call::ObjVec860 { obj } if *obj == me => {
                let v = if matches!(c, Call::ObjVec850 { .. }) { self.obj.want_vel } else { self.obj.moved };
                approx(Value::Vec([v.x, -v.y, -v.z, 0.0]), "the asked-for displacement of a grounded object is taken to press down by one tick of gravity [inferred]; the frame-step factor is 1")
            }
            Call::ObjVec860 { .. } | Call::ObjVec850 { .. } => missing(Value::Vec([0.0; 4]), "movement vectors of an object other than self"),
            Call::Target { .. } => imp(Value::Handle(TARGET)),
            Call::TargetPosition { .. } => approx(Value::Vec(to_game(self.env.player)), "the player's origin, not the lock-on part's point"),
            Call::LastAttacker { .. } => imp(Value::Handle(PLAYER)),
            Call::SetTargetMode { .. } => stub(Value::None, "lock mode of a target record: nothing in the app consumes it"),
            Call::FreeTarget { .. } => stub(Value::Bool(true), "target records are not pooled"),
            Call::MotionDone { obj } if *obj == PLAYER => {
                // A paired motion this object started on the player in this tick is a new motion
                // that has not ended (the game's bit is read live; the player side's is a tick old).
                let done = self.env.player_motion_done && self.obj.player_serial.is_none();
                approx(Value::Bool(done), "bit 1 of the player's +0x160 from reaction_cmd::PlayerReaction (player_reaction.rs sets it from the real end of his motion, one tick old)")
            }
            Call::MotionDone { .. } => imp(Value::Bool(self.obj.done)),
            Call::MotionState { .. } => imp(Value::Int(self.obj.slot.map_or(-1, |s| s as i32))),
            Call::MotionLength { motion, .. } | Call::MotionEnd { motion, .. } => {
                let slot = if *motion < 0 { self.obj.slot } else { Some(*motion as usize) };
                let m = slot.and_then(|s| self.env.rig.mset.get(s)).and_then(|s| s.motion.as_ref());
                let end = matches!(c, Call::MotionEnd { .. });
                imp(Value::Float(m.map_or(0.0, |m| if end { m.frame_return } else { m.frame_end })))
            }
            Call::OnScreen { .. } => approx(Value::Bool(self.obj.on_screen), "the origin lifted one metre against the camera's view"),
            Call::Airborne { .. } => imp(Value::Bool(!self.obj.motor.grounded)),
            Call::Busy { .. } | Call::HitStun { .. } => approx(Value::Bool(self.obj.reacting), "the game's flags (+0x160) are read as 'a hit reaction plays'"),
            // FUN_0016b940: word `index` of the array at object +0x81c; words 1 and 2 are the record's
            // +0x20 and +0x24 (FUN_0016b870). Without a record (the app's own waves) 99 stays.
            Call::SpawnParam { index, .. } => match (self.obj.spawn_params, index) {
                (Some(p), 1 | 2) => imp(Value::Int(p[*index as usize - 1])),
                _ => stub(Value::Int(99), "no spawn record (the app's own waves) or word 0 (the record pointer)"),
            },
            // FUN_0016b858: byte +0x1c of the spawn record, -1 without one (the app's own waves).
            Call::SpawnKind { .. } => imp(Value::Int(self.obj.spawn_kind)),
            Call::GlobalByte33e830 => approx(Value::Int(2), "world number of Twilight Town"),
            Call::GlobalFlag4 | Call::GlobalFlag80 | Call::EventActive => stub(Value::Bool(false), "global state set by other scripts"),
            Call::GlobalByte301 => stub(Value::Int(0), "global byte at 0x35ea48 + 0x1e, role unknown"),
            Call::HitIsReal { hit } => imp(Value::Bool(self.obj.hits.get(hit).copied().unwrap_or(true))),
            Call::Flag124 { .. } => imp(Value::Bool(self.env.flags124)),
            Call::Flag309 { obj } => approx(Value::Bool(*obj == PLAYER), "status bit 0x80: set for the party class"),
            Call::ProbeGround { dir, a, b, .. } => imp(Value::Int(self.probe(*dir, *a, *b))),
            Call::PlayMotion { motion, blend, .. } => {
                self.play(*motion, *blend, None, false);
                approx(Value::None, "the game's motion controller (blend queue, state slots) is not ported")
            }
            Call::PlayMotionChecked { motion, blend, .. } => {
                self.play(*motion, *blend, None, true);
                approx(Value::None, "the game's motion controller (blend queue, state slots) is not ported")
            }
            Call::PlayMotionAt { motion, start, blend, .. } => {
                self.play(*motion, *blend, Some(*start), false);
                approx(Value::None, "the game's motion controller (blend queue, state slots) is not ported")
            }
            Call::FaceDirection { dir, .. } | Call::FaceToward { dir, .. } => {
                self.obj.motor.yaw = yaw_of(*dir);
                imp(Value::None)
            }
            Call::TurnToward { dir, speed, .. } => {
                let turning = self.turn_toward(*dir, *speed);
                if *speed == 0.0 {
                    approx(Value::Bool(turning), "turn rate of speed 0 is the placeholder's, the object's own rate was not found")
                } else {
                    imp(Value::Bool(turning))
                }
            }
            Call::SetMove { dir, speed, .. } => {
                self.obj.set_move(*dir, *speed);
                approx(Value::None, "applied for one tick (inferred), vertical part ignored")
            }
            Call::SetVelocity { obj, dir, speed } if *obj == PLAYER => {
                let d = from_game(*dir).normalize_or_zero();
                self.obj.player_cmds.push(PlayerReactionCmd::Velocity { dir: d, speed: *speed });
                approx(Value::None, "handed to the player side as PlayerReactionCmd::Velocity (player_reaction.rs, applied one tick later)")
            }
            Call::SetVelocity { .. } => missing(Value::None, "secondary velocity vector at +0x880: consumer not found"),
            Call::StartMove { dir, motion, blend, commit, .. } => {
                self.start_move(*dir, *motion, *blend, *commit);
                approx(Value::None, "jump height is |y| (positive y reads as a fall in FUN_0017b1e0, the scripts mean a jump); falling and landing motions follow FUN_0017b9e0 / FUN_0017b8f8, the apex trigger is inferred")
            }
            Call::StartMoveAlt { dir, motion, blend, commit, .. } => {
                self.start_move(*dir, *motion, *blend, *commit);
                approx(Value::None, "as start_move; the alternate setter FUN_001b4ee0 was not read")
            }
            Call::Hide { .. } => {
                self.obj.hidden = true;
                self.obj.alpha = 0.0;
                self.obj.fade = None;
                imp(Value::None)
            }
            Call::FadeOut { frames, .. } => {
                self.obj.alpha = 1.0;
                self.obj.fade = Some(Fade { from: 1.0, to: 0.0, frames: *frames, t: 0.0 });
                imp(Value::None)
            }
            Call::FadeIn { frames, .. } => {
                self.obj.alpha = 0.0;
                self.obj.fade = Some(Fade { from: 0.0, to: 1.0, frames: *frames, t: 0.0 });
                imp(Value::None)
            }
            // 1.92 (FUN_0016b738 -> FUN_001a3cb0): the drop of prize row `id`; computed after the tick.
            Call::GroundMarker { id, .. } => {
                self.obj.drops.push(*id);
                approx(Value::None, "the drop is computed from `przt` (FUN_001a3cb0: prize counts, item rolls) and logged; the app has no prize objects, nothing appears")
            }
            Call::SetFlag { obj, bit } | Call::ClearFlag { obj, bit } if *obj == PLAYER => {
                let set = matches!(c, Call::SetFlag { .. });
                if *bit == FLAG_BIT_UNHITTABLE {
                    self.obj.player_cmds.push(PlayerReactionCmd::Unhittable(set));
                    approx(Value::None, "bit 3 of the player's flags handed to the player side as PlayerReactionCmd::Unhittable (player_reaction.rs)")
                } else {
                    stub(Value::None, "a flag bit of the player other than 3: no consumer in the app")
                }
            }
            Call::SetFlag { bit, .. } => {
                if (0..64).contains(bit) {
                    self.obj.flags |= 1 << bit;
                }
                imp(Value::None)
            }
            Call::ClearFlag { bit, .. } => {
                if (0..64).contains(bit) {
                    self.obj.flags &= !(1 << bit);
                }
                imp(Value::None)
            }
            Call::SetFlag26 { .. } => {
                self.obj.flags |= 1 << 26;
                stub(Value::None, "flag 26 of +0x108: no consumer in the app")
            }
            Call::SetFlag768 { .. } => {
                self.obj.flags768 = true;
                stub(Value::None, "bit 6 of +0x768: no consumer in the app")
            }
            Call::ClearFlag768 { .. } => {
                self.obj.flags768 = false;
                stub(Value::None, "bit 6 of +0x768: no consumer in the app")
            }
            Call::SpawnAttached { id, .. } => {
                self.obj.effects.push((*id).max(0) as u16);
                self.obj.next_effect += 1;
                approx(Value::Handle(EFFECT + self.obj.next_effect), "element `id` of the object's effect file; the flag and extra arguments are ignored")
            }
            Call::ReleaseEffect { .. } => stub(Value::None, "effects run to their end"),
            Call::ObjMethod44 { .. } | Call::ObjMethod48 { .. } => missing(Value::None, "virtual method of the object, role unknown"),
            Call::MoverMethod { .. } => missing(Value::None, "virtual call on the object's mover, role unknown"),
            // FUN_001b2148 builds the status block from `enmp` row `a`; the app built it at spawn from
            // the kind's row and rebuilds it (full HP) when the script asks for another one.
            Call::SetObjParam { a, .. } => {
                if *a != self.env.enmp as i32 {
                    self.trace.note(self.tag, format!("set_obj_param asks for enmp row {a}, the enemy was built from {}: rebuilt", self.env.enmp));
                    self.obj.param_row = u16::try_from(*a).ok();
                }
                imp(Value::None)
            }
            Call::ModeFrozen { .. } => stub(Value::Bool(false), "hit stop is not implemented"),
            Call::ModeInterrupted { .. } => approx(Value::Bool(self.obj.reacting), "FUN_00186b70: the object is in a hit reaction"),
            Call::ModeMayStart { .. } => approx(Value::Bool(!self.obj.reacting), "FUN_001873d0: the object is not in hit stun"),
            Call::FindObject { value, .. } => {
                if *value == 0x236 {
                    imp(Value::Handle(PLAYER))
                } else {
                    missing(Value::Handle(0), "object lookup other than the player")
                }
            }
            // 1.90 send_message (0x172fb8 -> FUN_00187170) / 1.241: an entry of the global 64-entry queue
            // (name, sender's context, delay) that the per-frame task FUN_00187250 hands to every object.
            Call::SendMessage { sender, name, delay } => {
                self.trace.line(self.tag, &format!("send_message {name:?} delay {delay} from {sender:#x}"));
                self.obj.outbox.push((*sender, name.clone(), *delay));
                approx(Value::None, "delivered to every scripted enemy at the start of the next tick (Mailbox); objects without a script (the player) draw no delay from the generator")
            }
            Call::RemoteSetMode { .. } => missing(Value::None, "a mode queued for another object (host 6.58): not wired"),
            // 1.29 play_motion_with(from, to, entry, blend): entry `motion` of the first object's motion
            // set played on the second object (FUN_0017f160). Only the player as `to` is supported.
            Call::Func { group: 1, index: 29, args } => {
                let (to, slot, blend) = (args[1].int() as u32, args[2].int(), args[3].float());
                if to != PLAYER {
                    return missing(Value::Int(0), "play_motion_with on an object other than the player");
                }
                let name = format!("{:03}0", self.env.player_model);
                let ticks = usize::try_from(slot).ok().and_then(|s| self.env.paired.get(&s)).and_then(|l| l.iter().find(|(n, _)| *n == name)).map(|(_, t)| *t);
                // FUN_0017ef98: the serial is the counter at 0x35ccb0 minus one, never above -1; its
                // start value was not read, so it is taken as 0 [inferred; scripts only compare it].
                let serial = self.obj.player_serial.unwrap_or(self.env.player_serial).min(0) - 1;
                self.obj.player_serial = Some(serial);
                self.obj.player_cmds.push(PlayerReactionCmd::PairedMotion {
                    from: Entity::PLACEHOLDER,
                    slot: slot.max(0) as usize,
                    blend,
                    serial,
                    ticks: ticks.unwrap_or(0.0),
                });
                if ticks.is_none() {
                    self.trace.note(self.tag, format!("paired motion entry {slot} has no sub-motion named {name}"));
                }
                approx(Value::Int(serial), "the player side plays the sub-motion one tick later (player_reaction.rs) and reports its end through motion_done")
            }
            // 1.238: the player's +0x158, the serial of his last paired motion.
            Call::Func { group: 1, index: 238, args } if args[0].int() as u32 == PLAYER => {
                imp(Value::Int(self.obj.player_serial.unwrap_or(self.env.player_serial)))
            }
            Call::Func { group: 1, index: 13, args } if args[0].int() as u32 == PLAYER => {
                self.obj.player_cmds.push(PlayerReactionCmd::QueueMotion { slot: args[1].int().max(0) as usize, blend: args[2].float() });
                approx(Value::None, "handed to the player side as PlayerReactionCmd::QueueMotion (player_reaction.rs)")
            }
            // 0.15 progress_flag_test (FUN_0016d5b8 -> FUN_001802e0): a bit of the story progress array
            // at 0x33f6b0 (0xffff is false). The app has no story progress: every flag reads clear.
            Call::Func { group: 0, index: 15, .. } => stub(Value::Bool(false), "story progress flags: the app has none, all read clear (a new game)"),
            Call::Func { group: 1, index: 63, args } if args[0].int() as u32 == me => imp(Value::Bool(self.motion_near_end())),
            // 1.66 (0x172bc0): bit 27 of the flag word at +0x108.
            Call::Func { group: 1, index: 66, .. } => {
                self.obj.flags |= 1 << 27;
                stub(Value::None, "flag 27 of +0x108: no consumer in the app")
            }
            // 1.69 (0x172c50): the float at object +0x170, the motion controller's (+0x140) time at +0x30.
            Call::Func { group: 1, index: 69, args } if args[0].int() as u32 == me => imp(Value::Float(self.obj.frame)),
            // 1.74 (FUN_00171548): ends the object's mover (its virtual method +4) and installs mover
            // type 0 (FUN_0016b698 -> FUN_0016b648 -> FUN_00179868(0)).
            Call::Func { group: 1, index: 74, .. } => {
                self.obj.move_vel = Vec3::ZERO;
                self.obj.air_vel = Vec3::ZERO;
                self.obj.dash = None;
                approx(Value::None, "the mover's own state is not modelled: the movement and jump speeds are cleared")
            }
            Call::Func { group: 1, index: 98, args } => match self.ground_position(args[0].int() as u32) {
                Some(p) => imp(Value::Vec(to_game(p))),
                None => missing(Value::Vec([0.0; 4]), "ground position of an object other than self and the player"),
            },
            // 0.42 angle_of (FUN_0016d060): atan2(v.x, v.z) through the game's FUN_003003e8.
            Call::Func { group: 0, index: 42, args } => match args.first() {
                Some(kh2_bdx::Arg::Vec(v)) => approx(Value::Float(v[0].atan2(v[2])), "the platform's atan2, not the game's FUN_003003e8 (same function, last-bit differences possible)"),
                _ => missing(Value::Float(0.0), "angle_of without a vector"),
            },
            // 0.43 wrap_angle (FUN_0016de78 -> FUN_0017bbc0).
            Call::Func { group: 0, index: 43, args } => imp(Value::Float(game_wrap_angle(args.first().map_or(0.0, |a| a.float())))),
            // 1.94 obj_valid (0x170150): 0 is false, else FUN_0019a3f8: in the object list (FUN_001b80e8)
            // and FUN_0019a440 clear (+0x108 bits 0x80000 / 0x10000000, not traced: read clear).
            Call::Func { group: 1, index: 94, args } => {
                let o = args.first().map_or(0, |a| a.int() as u32);
                if o == me || o == PLAYER || o == TARGET {
                    approx(Value::Bool(true), "self and the player are in the object list; the flags of FUN_0019a440 are read clear")
                } else {
                    approx(Value::Bool(false), "an object other than self and the player: not known to this host, read as absent")
                }
            }
            // 1.150 / 1.151 (0x171600, 0x171628): bit n-1 of the byte at object +0x238 / +0x239, the
            // motion controller's trigger bytes (see Obj::trigger_bits).
            Call::Func { group: 1, index: i @ (150 | 151), args } if args[0].int() as u32 == me => {
                let byte = self.obj.trigger_bits[*i as usize - 150];
                imp(Value::Bool((byte as u32).wrapping_shr((args[1].int() - 1) as u32) & 1 != 0))
            }
            // 1.84 (0x172ee8): the pointer at object +0x48c, the status block FUN_001b2148 builds. Its
            // first 32 records of 12 bytes are the HP of each part (FUN_0019f770: current, maximum,
            // lowest); the app has part 0 only.
            Call::Func { group: 1, index: 84, args } if args[0].int() as u32 == me => imp(Value::Handle(GAUGES)),
            // 1.228 / 1.229 / 1.232 (0x1740e0, 0x174100, 0x174120): word 0, 1 or 2 of record i.
            Call::Func { group: 1, index: i @ (228 | 229 | 232), args } if args[0].int() as u32 == GAUGES => {
                let rec = self.hp_record(args[1].int());
                imp(Value::Int(rec[match i { 228 => 0, 229 => 1, _ => 2 }]))
            }
            // 1.230 (0x174140): float(word 0) / float(word 1) of record i.
            Call::Func { group: 1, index: 230, args } if args[0].int() as u32 == GAUGES => {
                let rec = self.hp_record(args[1].int());
                imp(Value::Float(rec[0] as f32 / rec[1] as f32))
            }
            // 1.231 (0x1741c0): word 2 of record `index` = `value` (arguments: base, value, index); the
            // HP change FUN_001a0100 never goes below it.
            Call::Func { group: 1, index: 231, args } if args[0].int() as u32 == GAUGES => {
                if args[2].int() == 0 {
                    self.obj.hp_min = args[1].int();
                    imp(Value::None)
                } else {
                    stub(Value::None, "lowest HP of a part other than 0: the app's enemies have one part")
                }
            }
            // 1.155 (0x171c48): the object's virtual method +0x78 with (amount, part, flag != 0). The slot
            // holds FUN_00186238 in 27 object tables of the executable [the N_EX570 object's table was
            // not traced]: HP of the part plus `amount`, held between its lowest and its maximum
            // (FUN_001a0100); the death method runs when part 0 reaches 0. It returns that HP.
            Call::Func { group: 1, index: 155, args } if args[0].int() as u32 == me => {
                if args[2].int() != 0 {
                    return stub(Value::Int(0), "HP change of a part other than 0: the app's enemies have one part");
                }
                self.obj.hp_add += args[1].int();
                let hp = (self.obj.hp + self.obj.hp_add).max(self.obj.hp_min).min(self.obj.hp_max);
                approx(Value::Int(hp), "the change is applied after the tick; the HUD side of FUN_00186238 (FUN_0019bd18 / FUN_0019bd58) is not reproduced")
            }
            // 1.58 (0x172988 -> FUN_0019e6b8): +0x20 = the direction normalised times its w, +0xf0 = the
            // factor, then mover type 5 (FUN_0016b698(obj, 5); the mover object at 0x1c88f00, table
            // 0x35e4d8). Its velocity method FUN_0019e740 -> FUN_001832b8: with a movement vector
            // (+0x10, speed +0x1c above 0) v = v * k + direction * speed * (1 - k), else v = v * k; the
            // step is v times the frame-step factor (1). N_EX570_BTL passes (0, 0, 0, 1): v starts at 0.
            Call::Func { group: 1, index: 58, args } if args[0].int() as u32 == me => {
                let v = match &args[1] {
                    kh2_bdx::host::Arg::Vec(d) => from_game([d[0], d[1], d[2], 0.0]).normalize_or_zero() * d[3],
                    _ => Vec3::ZERO,
                };
                self.obj.dash = Some((v, args[2].float()));
                approx(Value::None, "mover 5 as the velocity rule of FUN_001832b8; the threshold test at its start (DAT_0037a880) only picks between two equal paths")
            }
            // 1.262 (0x1709c8): the object's virtual method +0x54 with (line, priority), FUN_001970b8 in
            // the enemy tables: the voice slot at +0x9b8 takes the line when its priority is at least
            // the queued one's (FUN_001ae798). N_EX570_BTL asks for lines 17, 26 and 37 with priority 2.
            Call::Func { group: 1, index: 262, .. } => stub(Value::None, "enemy voice lines: no voice slot for enemies in the app, and no voice/us/battle bank names this speaker"),
            // 1.211 / 1.212 (0x170760, 0x170788): set / clear bit n of the word at object +0xabc. Its
            // readers (FUN_00185be0, FUN_0018c6d0) test bits 9, 10, 11 and 13 (with +0xac0 and +0xac4);
            // N_EX570_BTL only uses bits 1 and 12, for which no reader was found.
            Call::Func { group: 1, index: i @ (211 | 212), args } if args[0].int() as u32 == me => {
                let bit = 1u32.checked_shl(args[1].int() as u32).unwrap_or(0);
                if *i == 211 {
                    self.obj.flags_abc |= bit;
                } else {
                    self.obj.flags_abc &= !bit;
                }
                stub(Value::None, "word +0xabc stored; bits 1 and 12 have no reader in the executable that was found")
            }
            // 2.76 (0x18e068): a float stored at object +0xaf4 (N_EX570_BTL: 50). The readers of +0xaf4
            // found (FUN_001769e0, FUN_0022e0a0, ...) read it as a byte or an int of other object
            // classes; no reader for this one was found.
            Call::Func { group: 2, index: 76, .. } => stub(Value::None, "float at +0xaf4: no reader found"),
            // 1.132: the target record returns to a pool; the app's records are never reused.
            Call::Func { group: 1, index: 132, .. } => stub(Value::None, "target records are not pooled"),
            Call::Func { .. } | Call::Unknown { .. } => missing(Value::Int(0), "function not mapped"),
        }
    }
}

impl Host for BevyHost<'_> {
    fn call(&mut self, c: &Call) -> Value {
        let (v, st, why) = self.exec(c);
        self.trace.record(self.tag, c, &v, st, why);
        v
    }

    fn read_mem(&mut self, addr: u32, out: &mut [u8]) -> bool {
        let o = addr.wrapping_sub(self.obj.handle) as usize;
        if let Some(b) = self.obj.mem.get(o..o + out.len()) {
            out.copy_from_slice(b);
            return true;
        }
        // A target record of event 11.
        let r = addr.wrapping_sub(RECORD);
        if let Some(rec) = self.obj.records.get((r / RECORD_SIZE) as usize) {
            let k = (r % RECORD_SIZE) as usize;
            if let Some(b) = rec.get(k..k + out.len()) {
                out.copy_from_slice(b);
                return true;
            }
        }
        if self.obj.volume_word(addr, out) {
            return true;
        }
        // The first 16 bytes of the player object: the Soldier's reaction thread copies them into its
        // request block and then uses the copy as a context, so the word at +4 must read as the player
        // object [inferred from that use; what the game keeps at object +4 was not read].
        let p = addr.wrapping_sub(PLAYER) as usize;
        if p + out.len() <= 16 {
            let mut b = [0u8; 16];
            b[4..8].copy_from_slice(&PLAYER.to_le_bytes());
            out.copy_from_slice(&b[p..p + out.len()]);
            return true;
        }
        false
    }

    fn write_mem(&mut self, addr: u32, data: &[u8]) -> bool {
        let o = addr.wrapping_sub(self.obj.handle) as usize;
        let Some(b) = self.obj.mem.get_mut(o..o + data.len()) else { return false };
        b.copy_from_slice(data);
        // A write that reaches the movement vector steers the object like `set_move` does.
        if o < MOVE_VECTOR + 16 && o + data.len() > MOVE_VECTOR {
            let f = |i: usize| f32::from_le_bytes(self.obj.mem[MOVE_VECTOR + 4 * i..MOVE_VECTOR + 4 * i + 4].try_into().unwrap());
            self.obj.set_move([f(0), f(1), f(2), 0.0], f(3));
        } else {
            self.trace.note(self.tag, format!("script wrote object memory at +{o:#x} ({} bytes): stored, nothing reads it", data.len()));
        }
        true
    }
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

    #[test]
    fn wrap_angle_follows_fun_0017bbc0() {
        let pi = f32::from_bits(0x4049_0fdb);
        assert_eq!(game_wrap_angle(0.0), 0.0);
        assert!((game_wrap_angle(3.0 * pi / 2.0) + pi / 2.0).abs() < 1e-5);
        assert!((game_wrap_angle(-3.0 * pi / 2.0) - pi / 2.0).abs() < 1e-5);
        assert!((game_wrap_angle(0.5) - 0.5).abs() < 1e-6);
        assert!((game_wrap_angle(-0.5) + 0.5).abs() < 1e-6);
    }

    fn row(id: u16, counts: [u8; 9], items: [(u16, u16); 3]) -> Vec<u8> {
        let mut r = vec![0u8; 0x18];
        r[0..2].copy_from_slice(&id.to_le_bytes());
        r[2..11].copy_from_slice(&counts);
        for (j, (i, c)) in items.iter().enumerate() {
            r[0xc + 4 * j..0xe + 4 * j].copy_from_slice(&i.to_le_bytes());
            r[0xe + 4 * j..0x10 + 4 * j].copy_from_slice(&c.to_le_bytes());
        }
        r
    }

    #[test]
    fn prize_rows_parse_and_fall_back_to_row_0x48() {
        let mut d = vec![0u8; 8];
        d[4..8].copy_from_slice(&2u32.to_le_bytes());
        d.extend(row(0x48, [1; 9], [(0, 0); 3]));
        d.extend(row(0x60, [0, 2, 0, 0, 3, 0, 0, 0, 1], [(10, 5), (0, 0), (11, 100)]));
        let rows = parse_przt(&d);
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[1].counts[4], 3);
        assert_eq!(rows[1].items[2], (11, 100));
        assert_eq!(prize_row(&rows, 0x60).map(|r| r.id), Some(0x60));
        assert_eq!(prize_row(&rows, 7).map(|r| r.id), Some(0x48));
    }

    /// FUN_001a3cb0 by hand: ceiling of count times the orb factor, three generator steps per prize,
    /// then one step per item pair, compared against chance times the item factor.
    #[test]
    fn a_drop_follows_fun_001a3cb0() {
        let r = PrizeRow { id: 1, counts: [0, 2, 0, 0, 3, 0, 0, 0, 1], items: [(10, 0), (11, 100), (12, 50)] };
        let mut seed = 1u32;
        let d = compute_drop(&r, 1.5, 1.0, &mut seed);
        assert_eq!(d.prizes, [0, 3, 0, 0, 5, 0, 0, 0, 2]);
        // 10 prizes, 3 steps each, then 3 item steps.
        let mut s = 1u32;
        for _ in 0..30 {
            s = s.wrapping_mul(0x10dcd);
        }
        let mut want = Vec::new();
        for (id, chance) in r.items {
            s = s.wrapping_mul(0x10dcd);
            if (s as f32) * 2.328_306_4e-10 * 100.0 < chance as f32 {
                want.push(id);
            }
        }
        assert_eq!(seed, s);
        assert_eq!(d.items, want);
        assert!(!d.items.contains(&10), "a chance of 0 never drops");
        assert!(d.items.contains(&11), "a chance of 100 always drops");
    }

    /// The disc's prize table: sorted by id and holding the fallback row 0x48 (skips without an ISO).
    #[test]
    fn real_przt() {
        let Some(iso) = crate::assets::iso_path().ok().filter(|p| p.exists()) else { return eprintln!("skipped: no disc (set KH2_ISO)") };
        let disc = Disc::open(&iso);
        let bin = disc.read("00battle.bin").expect("00battle.bin");
        let e = kh2_bar::parse(&bin).expect("bar");
        let rows = parse_przt(e.iter().find(|x| x.tag == "przt").expect("przt").data);
        assert!(rows.len() > 10, "{} rows", rows.len());
        assert!(rows.windows(2).all(|w| w[0].id < w[1].id), "sorted by id (FUN_001a41d8 bisects)");
        assert!(rows.iter().any(|r| r.id == 0x48));
    }

}
