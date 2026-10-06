//! The host of the enemy scripts: `BevyHost` answers each `kh2_bdx::Call` a script makes from the scripted
//! object (`enemy_obj::Obj`) and the app's world (`Env`). Every call is classified (`St`) by the helpers
//! `imp`, `approx`, `stub` and `missing`; `AiTrace` (`enemy_ai.rs`) records the classes. The dispatch splits by
//! the call's group: `group0`, `group1`, `group2`, `other` (the order of the arms inside a group is the order
//! they are tried in). The drop computation of host 1.92 (prize table rows, `compute_drop`) lives here too.
//! See docs/enemy-ai.md for the mapping and the gaps.

use std::collections::HashMap;

use bevy::prelude::*;
use kh2_bdx::{Call, Host, Value, Vec4};

use crate::enemy_ai::AiTrace;
use crate::enemy_obj::{
    from_game, to_game, yaw_of, Fade, MotionReq, Obj, EFFECT, FLAG_BIT_UNHITTABLE, GAUGES, MOVE_VECTOR, PLAYER, RECORD, RECORD_SIZE, TARGET,
};
use crate::motor::wrap_angle;
use crate::reaction_cmd::PlayerReactionCmd;
use crate::skel::Rig;
use crate::world::World;
use crate::SCALE;

/// Turn rate in radians per tick when a script passes speed 0 ("the object's own rate"). The game reads
/// it from the object (+0x11c); where that value comes from was not found, this is the rate the
/// placeholder enemies used (5 rad/s).
const TURN_DEFAULT: f32 = 5.0 / 60.0;
/// Ground height tolerance of `probe_ground`, centimetres (the game: halfword at +0xe of a data record
/// of the object times 0.4; the record was not decoded, this is the step height of `Motor`).
const PROBE_TOLERANCE: f32 = 40.0;

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

/// Classification of a host call, for the trace and the summary.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum St {
    /// Does what the game's function does (read from the game's code).
    Impl,
    /// Does the visible part; some of what the game does is not reproduced.
    Approx,
    /// Accepted and answered with a default; nothing in the app consumes it.
    Stub,
    /// Not implemented: the call fails visibly.
    Missing,
}

/// `group.index` of the host function behind a call (the mode manager's hooks are not host functions).
pub(crate) fn call_id(c: &Call) -> (u16, u16) {
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
pub(crate) struct Env<'a> {
    pub(crate) player: Vec3,
    pub(crate) player_yaw: f32,
    pub(crate) world: Option<&'a World>,
    pub(crate) rig: &'a Rig,
    pub(crate) flags124: bool,
    pub(crate) enmp: u16,
    /// The player's +0x158 (the serial of the paired motion last started on him) and whether that
    /// motion has ended (bit 1 of his +0x160), from `reaction_cmd::PlayerReaction`.
    pub(crate) player_serial: i32,
    pub(crate) player_motion_done: bool,
    pub(crate) paired: &'a HashMap<usize, Vec<(String, f32)>>,
    pub(crate) player_model: u8,
}

pub(crate) struct BevyHost<'a> {
    pub(crate) obj: &'a mut Obj,
    pub(crate) env: &'a Env<'a>,
    pub(crate) trace: &'a mut AiTrace,
    pub(crate) tag: &'a str,
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
        match call_id(c).0 {
            0 => self.group0(c),
            1 => self.group1(c),
            2 => self.group2(c),
            _ => self.other(c),
        }
    }

    /// Host group 0 (global functions).
    fn group0(&mut self, c: &Call) -> (Value, St, &'static str) {
        match c {
            Call::ReleaseEffect { .. } => stub(Value::None, "effects run to their end"),
            // 0.15 progress_flag_test (FUN_0016d5b8 -> FUN_001802e0): a bit of the story progress array
            // at 0x33f6b0 (0xffff is false). The app has no story progress: every flag reads clear.
            Call::Func { group: 0, index: 15, .. } => stub(Value::Bool(false), "story progress flags: the app has none, all read clear (a new game)"),
            // 0.42 angle_of (FUN_0016d060): atan2(v.x, v.z) through the game's FUN_003003e8.
            Call::Func { group: 0, index: 42, args } => match args.first() {
                Some(kh2_bdx::Arg::Vec(v)) => approx(Value::Float(v[0].atan2(v[2])), "the platform's atan2, not the game's FUN_003003e8 (same function, last-bit differences possible)"),
                _ => missing(Value::Float(0.0), "angle_of without a vector"),
            },
            // 0.43 wrap_angle (FUN_0016de78 -> FUN_0017bbc0).
            Call::Func { group: 0, index: 43, args } => imp(Value::Float(game_wrap_angle(args.first().map_or(0.0, |a| a.float())))),
            _ => missing(Value::Int(0), "function not mapped"),
        }
    }

    /// Host group 1 (object functions).
    fn group1(&mut self, c: &Call) -> (Value, St, &'static str) {
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
            Call::TargetPosition { .. } => approx(Value::Vec(to_game(self.env.player)), "the player's origin, not the lock-on part's point"),
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
            Call::Busy { .. } => approx(Value::Bool(self.obj.reacting), "the game's flags (+0x160) are read as 'a hit reaction plays'"),
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
            Call::ObjMethod44 { .. } | Call::ObjMethod48 { .. } => missing(Value::None, "virtual method of the object, role unknown"),
            Call::MoverMethod { .. } => missing(Value::None, "virtual call on the object's mover, role unknown"),
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
            // 1.132: the target record returns to a pool; the app's records are never reused.
            Call::Func { group: 1, index: 132, .. } => stub(Value::None, "target records are not pooled"),
            _ => missing(Value::Int(0), "function not mapped"),
        }
    }

    /// Host group 2 (enemy and battle functions).
    fn group2(&mut self, c: &Call) -> (Value, St, &'static str) {
        match c {
            Call::Target { .. } => imp(Value::Handle(TARGET)),
            Call::LastAttacker { .. } => imp(Value::Handle(PLAYER)),
            Call::HitStun { .. } => approx(Value::Bool(self.obj.reacting), "the game's flags (+0x160) are read as 'a hit reaction plays'"),
            Call::HitIsReal { hit } => imp(Value::Bool(self.obj.hits.get(hit).copied().unwrap_or(true))),
            // FUN_001b2148 builds the status block from `enmp` row `a`; the app built it at spawn from
            // the kind's row and rebuilds it (full HP) when the script asks for another one.
            Call::SetObjParam { a, .. } => {
                if *a != self.env.enmp as i32 {
                    self.trace.note(self.tag, format!("set_obj_param asks for enmp row {a}, the enemy was built from {}: rebuilt", self.env.enmp));
                    self.obj.param_row = u16::try_from(*a).ok();
                }
                imp(Value::None)
            }
            // 2.76 (0x18e068): a float stored at object +0xaf4 (N_EX570_BTL: 50). The readers of +0xaf4
            // found (FUN_001769e0, FUN_0022e0a0, ...) read it as a byte or an int of other object
            // classes; no reader for this one was found.
            Call::Func { group: 2, index: 76, .. } => stub(Value::None, "float at +0xaf4: no reader found"),
            _ => missing(Value::Int(0), "function not mapped"),
        }
    }

    /// Every other call: the mode manager's hooks, group 6 and the functions no group knows.
    fn other(&mut self, c: &Call) -> (Value, St, &'static str) {
        match c {
            Call::ModeFrozen { .. } => stub(Value::Bool(false), "hit stop is not implemented"),
            Call::ModeInterrupted { .. } => approx(Value::Bool(self.obj.reacting), "FUN_00186b70: the object is in a hit reaction"),
            Call::ModeMayStart { .. } => approx(Value::Bool(!self.obj.reacting), "FUN_001873d0: the object is not in hit stun"),
            Call::RemoteSetMode { .. } => missing(Value::None, "a mode queued for another object (host 6.58): not wired"),
            _ => missing(Value::Int(0), "function not mapped"),
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assets::Disc;

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
