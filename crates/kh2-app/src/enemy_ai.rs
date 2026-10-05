//! Enemy AI runtime: Shadow and Soldier run under the game's own BDX scripts (`obj/M_EX020.mdlx`,
//! `obj/M_EX010.mdlx`, read from the disc) instead of the placeholder state machine in `enemy.rs`.
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

use bevy::color::Alpha;
use bevy::prelude::*;
use kh2_bdx::{Call, Host, Program, Runtime, Value, Vec4};
use kh2_tables::tables as t;

use crate::assets::Disc;
use crate::combat::Fighter;
use crate::data::GameData;
use crate::enemy::{EState, Enemy};
use crate::fx::{GameEffect, StartEffect, EFFECT_DEATH};
use crate::motor::{wrap_angle, Motor};
use crate::schedule::Sim;
use crate::skel::{Animator, Rig};
use crate::sora::Sora;
use crate::world::World;
use crate::SCALE;

/// Object handles the script sees. The scripted object's memory is `[handle, handle + OBJ_SIZE)`.
const SELF_BASE: u32 = 0x0100_0000;
const PLAYER: u32 = 0x0200_0000;
const TARGET: u32 = 0x0300_0000;
const EFFECT: u32 = 0x0400_0000;
const HIT: u32 = 0x0500_0000;
const OBJ_SIZE: usize = 0x1000;
/// Offset of the movement vector (direction and speed in w) in a game object.
const MOVE_VECTOR: usize = 0x10;
/// Flag bit 3 of the object's flag array: the object cannot be hit (FUN_0019a4f0 tests it).
const FLAG_UNHITTABLE: u32 = 3;
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
}

/// The scripts of the enemies that run under BDX, by `Kind::name`.
#[derive(Resource, Default)]
struct AiPrograms {
    by_kind: HashMap<&'static str, Script>,
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
            | Call::Request2_20 { .. }
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
        Call::Request2_20 { .. } => (2, 20),
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

/// A position in the app (centimetres, Y up) as the script sees it.
fn to_game(p: Vec3) -> Vec4 {
    [p.x, -p.y, -p.z, 1.0]
}

/// A direction from the script as an app direction.
fn from_game(d: Vec4) -> Vec3 {
    Vec3::new(d[0], -d[1], -d[2])
}

/// App yaw of a script direction: atan2(x, z) in the game's space (atan2 of nothing is 0), then
/// mirrored into the app's.
fn yaw_of(d: Vec4) -> f32 {
    let g = if d[0] == 0.0 && d[2] == 0.0 { 0.0 } else { d[0].atan2(d[2]) };
    (g.sin()).atan2(-g.cos())
}

// ---------------------------------------------------------------------------------------------------
// The scripted object

struct Fade {
    from: f32,
    to: f32,
    frames: f32,
    t: f32,
}

struct MotionReq {
    slot: usize,
    blend: f32,
    start: Option<f32>,
}

/// What the script sees of its object, plus what it has asked for during this tick.
struct Obj {
    handle: u32,
    /// Position (centimetres, Y up), facing, ground contact and the jump model.
    motor: Motor,
    hp: i32,
    /// A hit reaction is playing (`EState::Hurt` or `Blown`).
    reacting: bool,
    on_screen: bool,
    /// The motion slot playing, as the script left it or the reaction code changed it (the game's +0x154).
    slot: Option<usize>,
    /// The motion reached its end during the last tick (looping motions: each time they wrap).
    done: bool,
    flags: u64,
    flags768: bool,
    mem: Vec<u8>,
    /// Horizontal movement set by `set_move` / the object's memory, centimetres per tick; used for one tick.
    move_vel: Vec3,
    /// Horizontal speed given to `start_move`, used while airborne.
    air_vel: Vec3,
    alpha: f32,
    fade: Option<Fade>,
    hidden: bool,
    motion_req: Option<MotionReq>,
    /// Motions the jump started by `start_move` (with the commit flag) plays: falling, landing.
    fall_motion: i32,
    land_motion: i32,
    fall_pending: bool,
    effects: Vec<u16>,
    next_effect: u32,
    hits: HashMap<u32, bool>,
    next_hit: u32,
}

impl Obj {
    fn new(handle: u32) -> Obj {
        Obj {
            handle,
            motor: Motor { grounded: true, ..Default::default() },
            hp: 1,
            reacting: false,
            on_screen: true,
            slot: None,
            done: false,
            flags: 0,
            flags768: false,
            mem: vec![0; OBJ_SIZE],
            move_vel: Vec3::ZERO,
            air_vel: Vec3::ZERO,
            alpha: 1.0,
            fade: None,
            hidden: false,
            motion_req: None,
            fall_motion: -1,
            land_motion: -1,
            fall_pending: false,
            effects: Vec::new(),
            next_effect: 0,
            hits: HashMap::new(),
            next_hit: 0,
        }
    }

    fn advance_fade(&mut self) {
        if let Some(f) = &mut self.fade {
            f.t += 1.0;
            let k = if f.frames <= 0.0 { 1.0 } else { (f.t / f.frames).min(1.0) };
            self.alpha = f.from + (f.to - f.from) * k;
            if k >= 1.0 {
                self.fade = None;
            }
        }
    }

    fn set_move(&mut self, dir: Vec4, speed: f32) {
        let d = from_game(dir).normalize_or_zero();
        self.move_vel = Vec3::new(d.x, 0.0, d.z) * speed;
    }
}

/// What the host reads from the app, per enemy and tick.
struct Env<'a> {
    player: Vec3,
    player_yaw: f32,
    world: Option<&'a World>,
    rig: &'a Rig,
    flags124: bool,
    enmp: u16,
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
        o.motion_req = Some(MotionReq { slot, blend, start });
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
            Call::ObjVec860 { .. } | Call::ObjVec850 { .. } => {
                missing(Value::Vec([0.0; 4]), "vector in the object at +0x860 / +0x850: no writer found")
            }
            Call::Target { .. } => imp(Value::Handle(TARGET)),
            Call::TargetPosition { .. } => approx(Value::Vec(to_game(self.env.player)), "the player's origin, not the lock-on part's point"),
            Call::LastAttacker { .. } => imp(Value::Handle(PLAYER)),
            Call::SetTargetMode { .. } => stub(Value::None, "lock mode of a target record: nothing in the app consumes it"),
            Call::FreeTarget { .. } => stub(Value::Bool(true), "target records are not pooled"),
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
            Call::SpawnParam { .. } => stub(Value::Int(99), "no spawn record: the area's event data is not read"),
            Call::SpawnKind { .. } => imp(Value::Int(-1)),
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
            Call::GroundMarker { .. } => missing(Value::None, "ground marker effect"),
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
            Call::SetObjParam { a, .. } => {
                if *a != self.env.enmp as i32 {
                    self.trace.note(self.tag, format!("set_obj_param asks for enmp row {a}, the enemy was built from {}", self.env.enmp));
                }
                imp(Value::None)
            }
            Call::Request2_20 { .. } => missing(Value::Int(0), "sound or effect request"),
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
            Call::SendMessage { .. } | Call::RemoteSetMode { .. } => missing(Value::None, "messages between objects"),
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
        match self.obj.mem.get(o..o + out.len()) {
            Some(b) => {
                out.copy_from_slice(b);
                true
            }
            None => false,
        }
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
    let entries: Vec<(&'static str, &str)> = vec![("Shadow", "M_EX020"), ("Soldier", "M_EX010")];
    let rows = {
        let Ok(mut a) = disc.0.lock() else { return };
        kh2_tables::load::<t::Objentry>(&mut a, &t::OBJENTRY).map(|d| d.rows).unwrap_or_default()
    };
    for (kind, model) in entries {
        let Some(file) = disc.read(&format!("obj/{model}.mdlx")) else { continue };
        let Some(entry) = kh2_bar::parse(&file).ok().and_then(|b| b.into_iter().find(|e| e.kind == 3).map(|e| e.data.to_vec())) else { continue };
        match Program::parse(&entry) {
            Ok(prog) => {
                let flags124 = rows.iter().find(|r| r.model_name == model).is_some_and(|r| r.flags & 8 != 0);
                info!("bdx: {kind} script {} ({} words), objentry flag 124 = {flags124}", prog.name, prog.words.len());
                programs.by_kind.insert(kind, Script { prog, flags124 });
            }
            Err(e) => warn!("bdx: {model}: {e}; {kind} keeps the placeholder AI"),
        }
    }
}

/// Gives every new Shadow and Soldier its script runtime. Runs right after the wave spawner, so the
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
    mut hit_msgs: MessageReader<AiHit>,
    mut fx: MessageWriter<StartEffect>,
    mut game: MessageWriter<GameEffect>,
    mut trace: ResMut<AiTrace>,
) {
    trace.tick += 1;
    if trace.tick.is_multiple_of(SUMMARY_EVERY) {
        trace.summary();
    }
    let mut hits: HashMap<Entity, Vec<bool>> = HashMap::new();
    for h in hit_msgs.read() {
        hits.entry(h.target).or_default().push(h.real);
    }
    let Ok(sp) = sora.single() else { return };
    let player = sp.translation / SCALE;
    let player_yaw = sp.rotation.to_euler(EulerRot::YXZ).0;
    let cam = cam.single().ok();
    for (ent, mut tf, mut e, mut f, mut brain) in &mut q {
        let Ok(mut anim) = anims.get_mut(e.body) else { continue };
        let rig = anim.rig.clone();
        let e = &mut *e;
        let AiBrain { rt, obj, tag, enmp, flags124, started, dead_sent, errors_seen, last_frame, in_air, .. } = &mut *brain;

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
        let env = Env { player, player_yaw, world: world.as_deref(), rig: &rig, flags124: *flags124, enmp: *enmp };
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
        rt.update(&mut host);
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
        if !matches!(e.state, EState::Hurt | EState::Blown | EState::Dying) {
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
        let mut delta = obj.move_vel + root_move.with_y(0.0);
        if was_air {
            delta += obj.air_vel;
        }
        obj.move_vel = Vec3::ZERO;
        let mut landed = false;
        if let Some(w) = world.as_deref() {
            let dy = was_air.then(|| obj.motor.jump_dy(data.sstm.fall_max) + root_move.y);
            landed = obj.motor.move_by(w, delta, dy);
        }
        // Poses are kept relative to the motion's root, so a motion whose root rises (Soldier's appear
        // motion) is followed by the origin: on the ground the root's height is an offset above the floor.
        if !was_air && obj.motor.grounded {
            obj.motor.pos.y += anim.root.translation.y;
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

    #[test]
    fn game_space_is_the_apps_rotated_half_a_turn_about_x() {
        let p = Vec3::new(1.0, 2.0, 3.0);
        assert_eq!(to_game(p), [1.0, -2.0, -3.0, 1.0]);
        assert_eq!(from_game([1.0, -2.0, -3.0, 0.0]), p);
    }

    #[test]
    fn yaw_of_a_script_direction_is_the_apps_facing() {
        // The app's facing for yaw a is (sin a, 0, cos a); the script sees it as (sin a, 0, -cos a).
        for k in -7..8 {
            let a = k as f32 * 0.4;
            let d = to_game(Vec3::new(a.sin(), 0.0, a.cos()) * 300.0);
            assert!(wrap_angle(yaw_of(d) - a).abs() < 1e-4, "{a}");
        }
        // atan2 of nothing is 0 in the game's space.
        assert!(wrap_angle(yaw_of([0.0; 4]) - std::f32::consts::PI).abs() < 1e-5);
    }

    #[test]
    fn a_fade_runs_its_length_and_stops() {
        let mut o = Obj::new(SELF_BASE);
        o.alpha = 0.0;
        o.fade = Some(Fade { from: 0.0, to: 1.0, frames: 30.0, t: 0.0 });
        for _ in 0..15 {
            o.advance_fade();
        }
        assert!((o.alpha - 0.5).abs() < 1e-5);
        for _ in 0..15 {
            o.advance_fade();
        }
        assert_eq!(o.alpha, 1.0);
        assert!(o.fade.is_none());
    }

    #[test]
    fn the_movement_vector_is_horizontal_and_in_the_apps_axes() {
        let mut o = Obj::new(SELF_BASE);
        o.set_move([3.0, 0.0, -3.0, 0.0], 8.0);
        let want = Vec3::new(1.0, 0.0, 1.0).normalize() * 8.0;
        assert!((o.move_vel - want).length() < 1e-4, "{:?}", o.move_vel);
    }
}
