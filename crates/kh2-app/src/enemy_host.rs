//! The host of the enemy scripts: `BevyHost` answers each `kh2_bdx::Call` a script makes from the scripted
//! object (`enemy_obj::Obj`) and the app's world (`Env`). Every call is classified (`St`) by the helpers
//! `imp`, `approx`, `stub` and `missing`; `AiTrace` (`enemy_ai.rs`) records the classes. The dispatch splits by
//! the call's group: `group0`, `group1`, `group2`, `other` (the order of the arms inside a group is the order
//! they are tried in). The drop computation of host 1.92 (prize table rows, `compute_drop`) lives here too.
//! See docs/enemy-ai.md for the mapping and the gaps.

use std::collections::HashMap;
use std::sync::{LazyLock, Mutex, MutexGuard};

use bevy::prelude::*;
use kh2_bdx::{Arg, Call, Host, Value, Vec4};

use crate::enemy_ai::AiTrace;
use crate::enemy_obj::{
    from_game, to_game, yaw_of, Fade, MotionReq, Obj, Remote, EFFECT, FLAG_BIT_UNHITTABLE, GAUGES, MOVE_VECTOR, PLAYER, RECORD, RECORD_SIZE, TARGET,
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

/// What a drop is made of: prizes per kind, the prize objects made (kind and the three generator
/// values of their launch, in order) and the items that won their roll.
#[derive(Clone, Debug, PartialEq)]
pub struct Drop {
    pub row: u16,
    pub prizes: [u32; 9],
    /// One entry per prize object the pool had room for (`crate::prize` turns the values into the
    /// launch velocity, FUN_001a3b28).
    pub launches: Vec<(u8, [u32; 3])>,
    pub items: Vec<u16>,
    /// One entry per dropped item: its id and the four generator values its object takes when it is
    /// made (`crate::prize::item_launch`): the yaw (FUN_001bd0d0), then X, Z and Y of the launch (the
    /// enter of state 12, FUN_001bd580, which FUN_001bd0d0's `FUN_0016b698(obj, 0xc)` runs at once).
    pub item_launches: Vec<(u16, [u32; 4])>,
}

/// FUN_001a3cb0 up to the prize objects: kind `k` drops `ceil(counts[k] * orb)` prizes (FUN_001a4240,
/// FUN_00300008 is a ceiling), `orb` being 1.0 (DAT_01c8a7e4, reset by FUN_00190370) plus each party
/// member's Jackpot float (ability block +0x60, FUN_00190788). Each prize object takes three steps of
/// the generator (FUN_001a3b28: seed * 0x10dcd, * 0x10dcd^2, then seed *= 0x10dcd^3), but only when
/// the pool gives it a slot (FUN_001c4d78 returns 0 when none is free; `free` slots are left). Then
/// each item pair in turn: one step, and the item drops when `seed * 2^-32 * 100 < chance * item`
/// (`item` 1.0 plus each member's Lucky Lucky float, +0x64; the seed read unsigned); a dropping item
/// takes four more steps before the next roll (FUN_001bd490 -> FUN_001bd0d0: one for the object's yaw,
/// then `FUN_0016b698(obj, 0xc)` enters state 12 whose enter FUN_001bd580 takes three for the launch).
/// The flags of 0x35d268 (0x200 no prizes, 0x400 no items) and the player state of FUN_001ce6c0 are
/// taken as clear.
pub fn compute_drop(row: &PrizeRow, orb: f32, item: f32, seed: &mut u32, mut free: usize) -> Drop {
    let mut prizes = [0u32; 9];
    let mut launches = Vec::new();
    for (k, p) in prizes.iter_mut().enumerate() {
        *p = (row.counts[k] as f32 * orb).ceil().max(0.0) as u32;
        for _ in 0..*p {
            if free == 0 {
                continue;
            }
            free -= 1;
            let mut v = [0u32; 3];
            for x in &mut v {
                *seed = seed.wrapping_mul(0x10dcd);
                *x = *seed;
            }
            launches.push((k as u8, v));
        }
    }
    let mut items = Vec::new();
    let mut item_launches = Vec::new();
    for &(id, chance) in &row.items {
        *seed = seed.wrapping_mul(0x10dcd);
        if (*seed as f32) * 2.328_306_4e-10 * 100.0 < chance as f32 * item {
            items.push(id);
            let mut v = [0u32; 4];
            for x in &mut v {
                *seed = seed.wrapping_mul(0x10dcd);
                *x = *seed;
            }
            item_launches.push((id, v));
        }
    }
    Drop { row: row.id, prizes, launches, items, item_launches }
}

/// The bytes of the object's own target record (host 2.23, `TARGET`): the player, position index 0
/// [inferred], 0, -1, 0 (the layout of FUN_0018ed40 + FUN_0018f490).
pub(crate) fn target_record() -> [u8; 20] {
    let mut r = [0u8; 20];
    r[0..4].copy_from_slice(&PLAYER.to_le_bytes());
    r[12..16].copy_from_slice(&(-1i32).to_le_bytes());
    r
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
pub(crate) fn game_wrap_angle(x: f32) -> f32 {
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
    /// The mission manager's counters (`struggle::Mission`), None without a manager (host 4.29).
    pub(crate) mission_counters: Option<[i32; 3]>,
    /// A mission layout event is playing (`struggle::Mission::event_active`, host 1.34) (WP-7E2).
    pub(crate) event_active: bool,
}

/// The keyed persistent buffers of host 2.21 (`kh2_bdx::shared`), one list for every enemy script of the battle like the
/// game's list at 0x1c95528. Emptied when the area's groups are set up (FUN_001a5030, called by the battle set-up
/// FUN_00168ba8); nothing else clears it, the game also frees it at the battle tear-down (FUN_001a5108).
static SHARED_BLOCKS: LazyLock<Mutex<kh2_bdx::shared::SharedBlocks>> = LazyLock::new(|| Mutex::new(kh2_bdx::shared::SharedBlocks::default()));

fn shared_blocks() -> MutexGuard<'static, kh2_bdx::shared::SharedBlocks> {
    SHARED_BLOCKS.lock().unwrap_or_else(|e| e.into_inner())
}

/// FUN_001a5030: the list of keyed buffers is emptied (the area's groups are set up again).
pub(crate) fn clear_shared_blocks() {
    shared_blocks().clear();
}

pub(crate) struct BevyHost<'a> {
    pub(crate) obj: &'a mut Obj,
    pub(crate) env: &'a Env<'a>,
    pub(crate) trace: &'a mut AiTrace,
    pub(crate) tag: &'a str,
}

/// The self-targeted motion queue (host 1.13) is enabled for M_EX900, M_EX110 and M_EX210 only until it is verified for the other kinds whose
/// scripts call it (M_EX930, M_EX940, M_EX850, M_EX760, M_EX420): for them the call stays unmapped as before. M_EX110's
/// `atk_foot_stamp` (R162c) queues motion 0 behind motion 202 and then waits (R0f0a) for the motion state to leave 202. M_EX210's
/// `move_target` run (0x11e6) plays motion 153, queues motion 0 and waits (0x1205) for the motion state to leave 153: without the queue it waited forever.
fn queues_motions(tag: &str) -> bool {
    tag.starts_with("M_EX900#") || tag.starts_with("M_EX110#") || tag.starts_with("M_EX210#")
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
        } else if self.obj.creates.iter().any(|c| c.0 == obj) {
            Some(self.obj.made_at(obj))
        } else {
            self.obj.others.get(&obj).map(|o| o.pos)
        }
    }

    /// Whether a call names another object of the app's (one that existed at the start of the tick or that this tick made).
    fn names_other(&self, obj: u32) -> bool {
        obj != self.obj.handle && self.obj.knows(obj)
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
        // [inferred] a motion started by the script replaces what was queued behind the old one (M_EX900 only).
        o.motion_queue.clear();
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
            // 0.14 progress_flag_set (FUN_0016d5a0 -> FUN_0017ff60): set the flag and run its side
            // effects (`progress::Story::set`). 0.15 progress_flag_test (FUN_0016d5b8 -> FUN_001802e0):
            // the bit of the array at 0x33f6b0 (0xffff is false). 0.70 (FUN_0016d5e8 -> FUN_00180330):
            // flag 0x102b. All on the app's story (`progress::shared_story`); with none installed (unit
            // tests, tools) every flag reads clear, as a new game.
            Call::Func { group: 0, index: 14, args } => {
                let id = args.first().map_or(0xffff, |a| a.int()) as u16;
                match crate::progress::shared_story() {
                    Some(s) => {
                        s.lock().set(id);
                        imp(Value::None)
                    }
                    None => stub(Value::None, "no story installed: the flag is not kept"),
                }
            }
            Call::Func { group: 0, index: 15 | 70, args } => {
                let id = if matches!(c, Call::Func { index: 70, .. }) { 0x102b } else { args.first().map_or(0xffff, |a| a.int()) as u16 };
                match crate::progress::shared_story() {
                    Some(s) => imp(Value::Bool(s.lock().test(id))),
                    None => stub(Value::Bool(false), "no story installed: every flag reads clear (a new game)"),
                }
            }
            // 0.21 angle_between (0x16cfd0): the wrapped turn from direction a to direction b,
            // atan2(b.x, b.z) - atan2(a.x, a.z) through FUN_0017bbc0 (moved here from npc.rs, WP-E2b).
            Call::Func { group: 0, index: 21, args } => match (args.first(), args.get(1)) {
                (Some(kh2_bdx::Arg::Vec(a)), Some(kh2_bdx::Arg::Vec(b))) => imp(Value::Float(game_wrap_angle(b[0].atan2(b[2]) - a[0].atan2(a[2])))),
                _ => missing(Value::Float(0.0), "angle_between without two vectors"),
            },
            // 0.42 angle_of (FUN_0016d060): atan2(v.x, v.z) through the game's FUN_003003e8.
            Call::Func { group: 0, index: 42, args } => match args.first() {
                Some(kh2_bdx::Arg::Vec(v)) => approx(Value::Float(v[0].atan2(v[2])), "the platform's atan2, not the game's FUN_003003e8 (same function, last-bit differences possible)"),
                _ => missing(Value::Float(0.0), "angle_of without a vector"),
            },
            // 0.45 / 0.46 (the runtime hands the two vectors and the scalar; the answer goes back into the first): the members'
            // `advance_vec` (FUN_0016d098) and `turn_vec` (FUN_0016d138 -> FUN_0017c0f8). M_EX110 steers its rocket's direction
            // with 0.46 (R05bc's caller, call at 0x05ac).
            Call::Func { group: 0, index: i @ (45 | 46), args } if args.len() >= 3 => match (&args[0], &args[1]) {
                (Arg::Vec(a), Arg::Vec(b)) => {
                    let st = args[2].float();
                    let v = if *i == 45 { crate::friend_host::advance_vec(*a, *b, st) } else { crate::friend_host::turn_vec(*a, *b, st) };
                    approx(Value::Vec(v), "the platform's sqrt / acos / sin / cos, not the vector unit's and the game's kernels")
                }
                _ => missing(Value::None, "0.45 / 0.46 without two vectors"),
            },
            // 0.43 wrap_angle (FUN_0016de78 -> FUN_0017bbc0).
            Call::Func { group: 0, index: 43, args } => imp(Value::Float(game_wrap_angle(args.first().map_or(0.0, |a| a.float())))),
            // 0.52 effect_fade (0x16dc48): fade the effect of a handle out over `frames`, destroying it at the end when the last argument is non-zero. The
            // handles of 1.22 name no drawn effect in the app (see there), so there is nothing to fade.
            Call::Func { group: 0, index: 52, .. } => stub(Value::None, "the effect of 1.22 is not drawn, so nothing fades"),
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
            Call::FreeTarget { .. } => stub(Value::Bool(true), "1.131 allocates: FUN_0018f3f8 files the record into the target pool; the app's records are not pooled"),
            Call::MotionDone { obj } if *obj == PLAYER => {
                // A paired motion this object started on the player in this tick is a new motion
                // that has not ended (the game's bit is read live; the player side's is a tick old).
                let done = self.env.player_motion_done && self.obj.player_serial.is_none();
                approx(Value::Bool(done), "bit 1 of the player's +0x160 from reaction_cmd::PlayerReaction (player_reaction.rs sets it from the real end of his motion, one tick old)")
            }
            Call::MotionDone { .. } => imp(Value::Bool(self.obj.done)),
            Call::MotionState { obj } if self.names_other(*obj) => {
                let slot = self.obj.others.get(obj).and_then(|o| o.slot);
                imp(Value::Int(slot.map_or(-1, |s| s as i32)))
            }
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
            // 1.34 (0x173888): DAT_0035e3a4 non-zero. The app's only events are the mission's layouts (WP-7E2).
            Call::EventActive => approx(Value::Bool(self.env.event_active), "only the mission's layout events exist in the app"),
            Call::GlobalFlag4 | Call::GlobalFlag80 => stub(Value::Bool(false), "global state set by other scripts"),
            // 1.301 (0x1744f8 -> FUN_001abee8): the byte at 0x35ea48 + 0x1e is the NUL ending the frame-group string and nothing stores to it, so the answer is 0.
            Call::GlobalByte301 => imp(Value::Int(0)),
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
            // 1.28 on an object of the app other than the caller (M_EX940 hides the M_EX850 it made before): the call is
            // carried out on that object at the end of the tick (`enemy_ai::ai_remote`).
            Call::Hide { obj } if self.names_other(*obj) => {
                self.obj.remote.push((*obj, Remote::Hide));
                approx(Value::None, "carried out on the other object at the end of the tick")
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
                approx(Value::None, "the drop is computed from `przt` (FUN_001a3cb0) and its prize objects run in prize.rs; dropped items are logged only (no item objects, FUN_001bd490), the three mini-game pools are absent")
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
            Call::SetFlag { obj, bit } | Call::ClearFlag { obj, bit } if self.names_other(*obj) => {
                self.obj.remote.push((*obj, if matches!(c, Call::SetFlag { .. }) { Remote::SetFlag(*bit) } else { Remote::ClearFlag(*bit) }));
                approx(Value::None, "carried out on the other object at the end of the tick")
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
                stub(Value::None, "bit 6 of +0x768 has no reader in the executable (mips_scan --load-off 0x768), so no behaviour is exact")
            }
            Call::ClearFlag768 { .. } => {
                self.obj.flags768 = false;
                stub(Value::None, "bit 6 of +0x768 has no reader in the executable (mips_scan --load-off 0x768), so no behaviour is exact")
            }
            Call::SpawnAttached { id, .. } => {
                self.obj.effects.push((*id).max(0) as u16);
                self.obj.next_effect += 1;
                approx(Value::Handle(EFFECT + self.obj.next_effect), "element `id` of the object's effect file; the flag and extra arguments are ignored")
            }
            Call::ObjMethod44 { .. } | Call::ObjMethod48 { .. } => missing(Value::None, "virtual method of the object, role unknown"),
            Call::MoverMethod { .. } => stub(Value::None, "virtual call (method +0xc, FUN_00171298) on the object's mover with one float: the role of the method was not traced, nothing in the app consumes it"),
            // 1.143 (FUN_0016b0d8): the object whose spawn record byte +0x1e equals the argument. The app keeps no such
            // byte per object, so nothing is found (M_EX650's shared-block set-up asks for 92 once and never reads the answer).
            Call::FindObject { by: kh2_bdx::host::ObjectKey::SpawnGroup, .. } => {
                stub(Value::Handle(0), "lookup by the spawn record's byte +0x1e: the app's objects do not carry it, so no object is found [inferred: the answer is 0]")
            }
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
            // 1.0 create_object (0x170190 -> FUN_0016e8c0(DAT_0037a484, id, place 0x1c883a0)): the object of `00objentry`
            // id `id`, built by its `ObjectType` constructor with the place at the origin (the place record is zero
            // but its last word) and no facing. The app makes the objects of its enemy kinds (`Obj::creatable`).
            Call::Func { group: 1, index: 0, args } => {
                let id = args.first().map_or(0, |a| a.int() as u32);
                if self.obj.creatable.contains(&id) {
                    let h = self.obj.create(id);
                    self.trace.line(self.tag, &format!("create_object: objentry {id:#x} made as {h:#x} at the origin"));
                    approx(Value::Handle(h), "the object appears at the next tick (enemy::spawn_created) and runs its own script from then; only the objentry ids of the app's enemy kinds are made")
                } else {
                    missing(Value::Handle(0), "create_object of an `00objentry` id the app has no object for")
                }
            }
            // 1.148 set_position (0x170658 -> FUN_0016a618): the point (w forced to 1.0) becomes the object's position and
            // the mover is synced.
            Call::Func { group: 1, index: 148, args } if args.len() >= 2 && (args[0].int() as u32 == me || self.names_other(args[0].int() as u32)) => {
                let (o, Arg::Vec(v)) = (args[0].int() as u32, &args[1]) else { return missing(Value::None, "set_position without a vector") };
                let p = from_game(*v);
                if o == me {
                    self.obj.motor.pos = p;
                    imp(Value::None)
                } else {
                    self.obj.remote.push((o, Remote::Position(p)));
                    approx(Value::None, "carried out on the other object at the end of the tick (the object a script made this tick takes the point where it appears); the mover's sync (FUN_0016a618) is the position itself")
                }
            }
            Call::Func { group: 1, index: 98, args } => match self.ground_position(args[0].int() as u32) {
                Some(p) => imp(Value::Vec(to_game(p))),
                None => missing(Value::Vec([0.0; 4]), "ground position of an object other than self and the player"),
            },
            // 1.99 ground_depth (0x1730e8, FUN_001730e8): the floor height of FUN_00183808 (the one host 1.98 answers) minus the
            // object's own height, in the game's space where Y points down; only when it is positive, and the answer is its negative. In
            // the app's space that is minus the height above the floor, 0 on or below it (or with no floor, which keeps the height).
            Call::Func { group: 1, index: 99, args } => match self.ground_position(args[0].int() as u32).zip(self.pos_of(args[0].int() as u32)) {
                Some((g, p)) => imp(Value::Float(-(p.y - g.y).max(0.0))),
                None => missing(Value::Float(0.0), "ground depth of an object other than self and the player"),
            },
            // 1.59 mover_bit1_or_busy (0x1729c0): the test the movement step FUN_00181e00 makes before it adds the ground's pull
            // (FUN_00183928) to a grounded object: bit 1 of the mover's word +8 (mover at +0xc of the object's +4 record) or bit 0x40 of
            // its +0x160 is set, i.e. the object is exempt from the ground. Their writers were not found, so a grounded object is read.
            Call::Func { group: 1, index: 59, .. } => stub(Value::Bool(false), "bit 1 of the mover's +8 / bit 0x40 of +0x160 (no gravity pull): writers not found, read clear"),
            // 1.94 obj_valid (0x170150): 0 is false, else FUN_0019a3f8: in the object list (FUN_001b80e8)
            // and FUN_0019a440 clear (+0x108 bits 0x80000 / 0x10000000, not traced: read clear).
            Call::Func { group: 1, index: 94, args } => {
                let o = args.first().map_or(0, |a| a.int() as u32);
                if o == me || o == PLAYER || o == TARGET {
                    approx(Value::Bool(true), "self and the player are in the object list; the flags of FUN_0019a440 are read clear")
                } else if self.obj.knows(o) {
                    approx(Value::Bool(true), "an object of the app (made by host 1.0 or one of the scripted objects) is in the object list; the flags of FUN_0019a440 are read clear")
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
            // The line is a line number of the object's own voice bank, the type 0x22 entry of its
            // `.apdx` (FUN_00196b58 builds the slot only when the entry exists; the speaker is not
            // looked up in `voice/<lang>/battle`). `audio::enemy_voice_calls` plays it (the priority
            // slot is `kh2_audio::voice::VoiceSlot`, one per enemy).
            Call::Func { group: 1, index: 262, args } if args[0].int() as u32 == me => {
                crate::audio::queue_enemy_voice(crate::audio::EnemyVoiceCall {
                    tag: self.tag.to_string(),
                    line: args[1].int(),
                    priority: args[2].int(),
                    at: self.obj.motor.pos * SCALE,
                });
                approx(Value::None, "the line is played by the nearest enemy of the script's kind (the host has no entity); the bank, priority slot and minimum volume are the game's")
            }
            // The same call on another object (the player's method +0x54 is FUN_00175d18, which takes a
            // voice id for the voice table, not a line): no script of the app does this.
            Call::Func { group: 1, index: 262, .. } => stub(Value::None, "voice request on an object other than the caller"),
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
            // 1.95 (0x173038 -> FUN_00183470): mover 1 (the jump mover, FUN_0016b698(obj, 1)) continuing the
            // last tick's real movement (+0x860, see 1.62): its horizontal part is the velocity (+0x20), its
            // vertical speed vy gives the apex time vy / g (+0xd0) and height y - (vy / g)^2 * g * 0.5 (+0xd8;
            // g = DAT_0037a884 = 20/49, 0.5 = DAT_0037a888). N_EX600_BTL's atk_air_counter hook (R13e7). Not
            // in `kh2_bdx::names`, it arrives undecoded with the raw context word; the only caller passes
            // its own context, so it is taken as this object.
            Call::Unknown { group: 1, index: 95, .. } | Call::Func { group: 1, index: 95, .. } => {
                let v = self.obj.moved;
                let h = v.y * v.y / (2.0 * crate::motor::G);
                self.obj.dash = None;
                self.obj.air_vel = Vec3::new(v.x, 0.0, v.z);
                if v.y > 0.0 {
                    self.obj.motor.start_jump(h);
                } else {
                    self.obj.motor.start_fall();
                    self.obj.motor.jump = crate::motor::Jump::fall(h, self.obj.motor.pos.y);
                }
                approx(Value::None, "the parabola of mover 1 is the app's Motor jump (eased rise to the same apex); the fall-motion follow-ups are not set")
            }
            // 1.54 bits_47c_480_clear (0x172910 -> FUN_0019dde0): bit n cleared in the bit arrays at object
            // +0x47c and +0x480 (set by 1.52 / 1.53, which none of the app's scripts call). No reader of
            // either array was traced. N_EX500_BTL clears bit 2 (R1447).
            Call::Func { group: 1, index: 54, args } if args[0].int() as u32 == me => {
                let n = args[1].int() as u32;
                let o = 0x47c + (n >> 5) as usize * 4;
                for base in [o, o + 4] {
                    if let Some(w) = self.obj.mem.get_mut(base..base + 4) {
                        let v = u32::from_le_bytes([w[0], w[1], w[2], w[3]]) & !(1 << (n & 31));
                        w.copy_from_slice(&v.to_le_bytes());
                    }
                }
                stub(Value::None, "bit arrays +0x47c / +0x480 cleared in the object memory; their reader FUN_0019a6a0 (per-part collision switch) has no counterpart in the app")
            }
            // 1.53 bits_480_set (0x1728e8 -> FUN_0019ddb0 on object +0x47c): bit n of the word at +0x480 + 4 * (n >> 5).
            // M_EX940 sets bits 5 and 7 in its init. No reader was traced (see 1.54).
            Call::Func { group: 1, index: 53, args } if args[0].int() as u32 == me => {
                let n = args[1].int() as u32;
                let o = 0x480 + (n >> 5) as usize * 4;
                if let Some(w) = self.obj.mem.get_mut(o..o + 4) {
                    let v = u32::from_le_bytes([w[0], w[1], w[2], w[3]]) | 1 << (n & 31);
                    w.copy_from_slice(&v.to_le_bytes());
                }
                stub(Value::None, "bit array +0x480 set in the object memory; its reader FUN_0019a6a0 (per-part collision switch) has no counterpart in the app")
            }
            // 1.97 obj_part_110 (0x173070): the address of the object's block at +0x110 (M_EX940 stores a float at
            // its +0xc in the init).
            Call::Func { group: 1, index: 97, args } if args[0].int() as u32 == me => imp(Value::Handle(me + 0x110)),
            // 1.108 / 1.109 (0x173278 / 0x173298 -> FUN_00192cd0 / FUN_00192ce8 -> FUN_0011c580 / FUN_0011c5b0): virtual
            // method +0x3c / +0x40 of the object behind the link record at object +0x670 (record +0x14), with the record's
            // word +0x104 (a joint) and the argument. The app has no link records (the Dancer's grab is not
            // carried either), so there is no object to call.
            Call::Func { group: 1, index: 108 | 109, .. } => stub(Value::None, "no link record in the app: the linked object's method is not called"),
            // 1.132: the target record returns to a pool; the app's records are never reused.
            Call::Func { group: 1, index: 132, .. } => stub(Value::None, "target records are not pooled"),
            // The Dancer's (M_EX930) force dance asks these of the player before it grabs him (BDX R0a48,
            // R0b0a). 1.101 (0x1716d0): bit 0x4000 of the player's +0x160 (the same test as FUN_00179538);
            // its writer was not found (the word is written through the motion controller's base).
            Call::Func { group: 1, index: 101, args } if args[0].int() as u32 == PLAYER => {
                approx(Value::Bool(false), "bit 0x4000 of the player's +0x160: writer not found, read clear")
            }
            // 1.225 (FUN_001708f8): 0 unless bit 2 of the player's +0x588 is set, then the result of his
            // mover's (+4) virtual method +0x60. The bit's writer was not traced.
            Call::Func { group: 1, index: 225, args } if args[0].int() as u32 == PLAYER => {
                approx(Value::Int(0), "bit 2 of the player's +0x588: writer not traced, read clear (the mover is not asked)")
            }
            // 1.161 (FUN_001706c8 -> FUN_001beb00): the player's link (+0x570) is set. Links are not in the
            // app (1.18 below is not carried out), so the player is never linked.
            Call::Func { group: 1, index: 161, args } if args[0].int() as u32 == PLAYER => {
                approx(Value::Bool(false), "the app has no object links (1.18 is not carried out): the player is never linked")
            }
            // 1.162 (0x173798 -> FUN_0018ffb0 -> FUN_001cb530(table 0x1c89b90, arg1, word +4 of arg0)): enters the object that 1.143 stored at
            // +4 of the block into the 8-slot table at 0x1c89b90 (halfword tag arg1), but only when FUN_0019a3f8 finds it in the object list.
            // 1.143 answers no object in the app, so the word is 0 and the call changes nothing. M_EX210 makes it once from its
            // shared-block set-up (R1a25 at 0x1a69). Who reads the table was not traced.
            Call::Unknown { group: 1, index: 162, .. } | Call::Func { group: 1, index: 162, .. } => {
                stub(Value::None, "enters the object bound by 1.143 into the table at 0x1c89b90; 1.143 finds none, so nothing is entered")
            }
            // 1.161 on the script's own object (M_EX770: BDX R145b at 0x1479 and R1659 at 0x165f ask it of local+0). Same function: the
            // object's +0x570 is the link parent that link_start (1.18) writes; the app writes none, so the enemy is never linked and
            // its wait loop ends at once.
            Call::Func { group: 1, index: 161, args } if args[0].int() as u32 == me => {
                approx(Value::Bool(false), "the app has no object links (1.18 is not carried out): the enemy is never linked")
            }
            // 1.18 link_start (0x170dd8 -> FUN_001beb90): the first object (the player) gets the second as
            // its parent (+0x570), the two halfwords (+0x574: 0x4002, +0x576: 1), its facing (+0x550) cleared
            // and 8 or-ed into +0x7f8 (when bit 1 of the flags is clear), mover 3; FUN_00169ee8 breaks the link when bit 0 of
            // +0x576 is set and the parent's +0x160 loses bit 0x8000. The player side has no link mover.
            Call::Func { group: 1, index: 18, args } if args[0].int() as u32 == PLAYER => stub(
                Value::None,
                "the player-side link (FUN_001beb90: parent +0x570, mover 3) is not in the app: Sora is not attached to the Dancer during its force dance",
            ),
            // 1.13 queue_motion on this object (0x1703d8 -> FUN_0017e170): appends (slot, blend) to the 4-entry ring
            // at the motion controller +0x3c; the consumer of that ring was not traced.
            Call::Func { group: 1, index: 13, args } if args[0].int() as u32 == me && queues_motions(self.tag) => {
                if let Ok(slot) = usize::try_from(args[1].int()) {
                    if self.obj.motion_queue.len() == 4 {
                        self.obj.motion_queue.remove(0);
                    }
                    self.obj.motion_queue.push((slot, args[2].float()));
                }
                approx(Value::None, "the queue's consumer was not traced: the first entry starts when the playing motion ends, a new play_motion drops the queue [inferred]")
            }
            // 1.311 set_scaled_byte (0x174238 -> FUN_001a0ba0): byte +0x1a8 + i of the status block = (halfword i of the
            // enmp row's table (+0x48) * percent + 99) / 100. FUN_001a0610 multiplies the damage by byte (attack
            // element) and byte +0x1ae (i = 6) and FUN_0018b618 makes a hit ineffective when their product is 0:
            // `Stats::weakness`, which `enemy_ai` rescales from the row.
            Call::Func { group: 1, index: 311, args } if args[0].int() as u32 == GAUGES => match usize::try_from(args[1].int()) {
                Ok(i) if i < 7 => {
                    self.obj.weak_scale.push((i, args[2].int()));
                    imp(Value::None)
                }
                _ => missing(Value::None, "set_scaled_byte with an index outside the seven damage scales"),
            },
            // 1.213 (0x1707d8): FUN_001699a8 of the object at +4 of the context the argument points at: the halfword at +0x4c
            // of the object's `00objentry` row, which is `NeoStatus` (1 for every P_EX100 row, Sora's forms; the script
            // compares it with 1 to ask "is the target Sora"). The pointer is into the script's own frame (the host cannot
            // read it); the scripts build that context from host 2.23's target record, whose object is always the player
            // in the app, so the answer is Sora's.
            Call::Unknown { group: 1, index: 213, .. } => {
                approx(Value::Int(1), "the context lives in the script's frame; its object is the target, which is always the player here [inferred]")
            }
            // 1.130 obj_diff (0x170510): the difference of the two contexts' objects, 0 when either is 0 or they are
            // the same (M_EX950 R1fe7 asks it of the sender of an event and itself before taking `mode_revenge`).
            Call::Func { group: 1, index: 130, args } if args.len() >= 2 => {
                let (a, b) = (args[0].int() as u32, args[1].int() as u32);
                imp(Value::Int(if a == 0 || b == 0 || a == b { 0 } else { a.wrapping_sub(b) as i32 }))
            }
            // 1.140 target_valid (0x173740 -> FUN_0018ed58): the target record names an object, or the lookup FUN_0018ee20
            // finds one. The record (host 2.23) is always the player's, who always exists in the app, so any non-zero record
            // is valid.
            Call::Func { group: 1, index: 140, args } => approx(Value::Bool(args.first().is_some_and(|a| a.int() != 0)), "the record always names the player, who always exists; FUN_0018ee20's lookup is not ported"),
            // 1.179 set_word_100 (0x1713a8): stores the word at object +0x100 (M_EX950 R2316 stores 154, M_EX880 too).
            // No reader of +0x100 was traced for the enemy objects, so nothing in the app consumes it.
            Call::Func { group: 1, index: 179, .. } => stub(Value::None, "word at object +0x100: reader not traced, nothing in the app consumes it"),
            // 1.13 queue_motion (0x1703d8 -> FUN_0017e170) on the object itself: the motion follows the running one in the
            // controller's queue. The app has no queue: M_EX950 queues motion 0 (idle) at the end of its attacks and its next
            // mode starts its own motion.
            Call::Func { group: 1, index: 13, args } if args[0].int() as u32 == me => {
                stub(Value::None, "the motion controller's queue (+0x3c) is not in the app: the queued motion does not follow the running one")
            }
            // 1.75 (0x172d18 -> FUN_0017d9c8) and 1.76 (0x172d40 -> FUN_0017da18): the motion controller (+0x140) takes the
            // entry that FUN_001a7da8 finds for the id (251 in M_EX950 R230c) into its +4, and 1.76 clears that word. Its reader
            // was not traced, so nothing in the app consumes it.
            Call::Func { group: 1, index: 75 | 76, .. } | Call::Unknown { group: 1, index: 75 | 76, .. } => stub(Value::None, "motion controller word +4 (set from a lookup, cleared by 1.76): reader not traced"),
            // 1.22 spawn_effect_at (0x1717c0 -> FUN_0016a5b8): element `id` of the object's effect file started with no owner and moved to the vector
            // (FUN_001d9d00); the answer is the effect's handle. The app has no placed, ownerless effect for an enemy (`Obj::effects` start on the
            // owner), so no effect is drawn and the handle only stands for the effect (M_EX730 calls it at 0x00e9 with element 12 at a bone and at 0x0c8c with element 9; R0d4f fades a held handle with 0.52 at 0x0d63).
            Call::Func { group: 1, index: 22, .. } => {
                self.obj.next_effect += 1;
                stub(Value::Handle(EFFECT + self.obj.next_effect), "the element placed at the script's vector is not drawn: the app only starts effects on their owner")
            }
            _ => missing(Value::Int(0), "function not mapped"),
        }
    }

    /// Host group 2 (enemy and battle functions).
    fn group2(&mut self, c: &Call) -> (Value, St, &'static str) {
        match c {
            Call::Target { .. } => imp(Value::Handle(TARGET)),
            Call::LastAttacker { .. } => imp(Value::Handle(PLAYER)),
            // 2.6 in_reaction (FUN_0018dfa0 -> FUN_00185a08): the object has a controller (+0x140) and its motion state (+0x154)
            // is 10 to 39. The app's slot is that state (as host 1.15 answers it).
            Call::Func { group: 2, index: 6, args } => {
                let o = args.first().map_or(0, |a| a.int() as u32);
                let slot = if self.names_other(o) { self.obj.others.get(&o).and_then(|x| x.slot) } else { self.obj.slot };
                approx(Value::Bool(slot.is_some_and(|s| (10..40).contains(&s))), "the app's motion slot stands for the game's +0x154 [inferred, as host 1.15]")
            }
            // 2.67 new_hit_group (FUN_001bdf00): the next id of the counter at 0x1d5dd4c, 0x100 after 0x10000000.
            Call::Func { group: 2, index: 67, .. } => {
                static HIT_GROUP: Mutex<i32> = Mutex::new(0);
                let mut counter = HIT_GROUP.lock().unwrap_or_else(|e| e.into_inner());
                *counter = if *counter + 1 > 0x1000_0000 { 0x100 } else { *counter + 1 };
                let next = *counter;
                approx(Value::Int(next), "the counter starts at 0 here; the game's is shared with every other user of FUN_001bdf00")
            }
            Call::HitStun { .. } => approx(Value::Bool(self.obj.reacting), "the game's flags (+0x160) are read as 'a hit reaction plays'"),
            Call::HitIsReal { hit } => imp(Value::Bool(self.obj.hits.get(hit).is_none_or(|h| h.0))),
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
            // 2.21 shared_block (0x18e928 -> FUN_001a5040): the buffer registered under the key (M_EX760 asks for key 12, size 20
            // in its R1ddd and tests the first word of it to set the block up once).
            Call::Func { group: 2, index: 21, args } => imp(Value::Handle(shared_blocks().get(args[0].int(), args[1].int()))),
            Call::Func { group: 2, index: 76, .. } => stub(Value::None, "float at +0xaf4: no reader found"),
            // 2.7 hit_reaction (0x18e0d0): byte +0x26 of the hit record, the effective reaction; the app's
            // is `reaction_rules::victim_outcome`'s, the same byte 2.8 classifies.
            Call::Func { group: 2, index: 7, args } => match self.obj.hits.get(&(args[0].int() as u32)) {
                Some(&(_, r, _)) => imp(Value::Int(r as i32)),
                None => missing(Value::Int(0), "hit_reaction of a handle that is not one of this object's hit records"),
            },
            // 2.45 hit_atkp_id (0x18e1c0): the halfword at +2 of the hit record, the id of the attack row.
            Call::Func { group: 2, index: 45, args } => match self.obj.hits.get(&(args[0].int() as u32)) {
                Some(&(_, _, atkp)) => imp(Value::Int(atkp as i32)),
                None => missing(Value::Int(0), "hit_atkp_id of a handle that is not one of this object's hit records"),
            },
            // 2.64 hit_reaction_original (0x18e0e0): byte +0x27 of the hit record, the attack row's own EnemyReaction (byte +10; FUN_0018b618 stores
            // it at +0x26 and +0x27, only +0x26 is changed afterwards).
            // 2.85 hit_atkp_element (0x18e218): the Element byte (+9) of the row [inferred from the name table: the function is not in the decomp].
            Call::Func { group: 2, index: 64 | 85, args } => {
                let atkp = self.obj.hits.get(&(args[0].int() as u32)).map(|h| h.2);
                match atkp.and_then(crate::enemy_obj::hit_row_bytes) {
                    Some((reaction, element)) => imp(Value::Int(if matches!(c, Call::Func { index: 64, .. }) { reaction as i32 } else { element as i32 })),
                    None => missing(Value::Int(0), "attack row of a hit that is not one of this object's hit records"),
                }
            }
            _ => missing(Value::Int(0), "function not mapped"),
        }
    }

    /// Every other call: the mode manager's hooks, group 6 and the functions no group knows.
    fn other(&mut self, c: &Call) -> (Value, St, &'static str) {
        match c {
            Call::ModeFrozen { .. } => stub(Value::Bool(false), "hit stop is not implemented"),
            Call::ModeInterrupted { .. } => approx(Value::Bool(self.obj.reacting), "FUN_00186b70: the object is in a hit reaction"),
            Call::ModeMayStart { .. } => approx(Value::Bool(!self.obj.reacting), "FUN_001873d0: the object is not in hit stun"),
            // Host 1.8 on the context of an object this script made (host 1.0): FUN_00186db0 on that object's mode machine.
            // Carried out when the object runs its next tick (`enemy_ai::ai_remote` -> `Obj::modes_asked`).
            Call::RemoteSetMode { obj, name } if self.obj.ask_mode(*obj, name) => {
                approx(Value::None, "the mode is switched by the made object's own script at the start of its next tick (the game switches it at once)")
            }
            Call::RemoteSetMode { .. } => missing(Value::None, "a mode queued for another object (host 6.58) or host 1.8 on an object that is not one this script made: not wired"),
            // 4.29 (FUN_0019ebd0 -> FUN_001d6738(i)): 0 unless the manager at DAT_01d5dd20 exists (set by
            // FUN_001b9338, which the event command FUN_00180b48 runs), then word +0x114 of its entry i
            // (0x38 bytes from +0xf4, used when bit 0 of the entry's first word is set). The Struggle
            // opponents compare entries 0 and 1 (the two sides' score [inferred from gameset_voice_win /
            // _lose / _draw]). Since WP-7E the manager is `struggle::Mission`: entry i is its counter i
            // (slot 0 the player's orbs, slot 1 the opponent's), 0 when it does not exist.
            Call::Unknown { group: 4, index: 29, .. } | Call::Func { group: 4, index: 29, .. } => {
                let i = match c {
                    Call::Unknown { args, .. } => args.first().map_or(0, |&a| a as i32),
                    Call::Func { args, .. } => args.first().map_or(0, |a| a.int()),
                    _ => 0,
                };
                let v = self.env.mission_counters.and_then(|cs| usize::try_from(i).ok().and_then(|i| cs.get(i).copied())).unwrap_or(0);
                imp(Value::Int(v))
            }
            // 6.31 (wrapper FUN_002eece8, table entry 0x370f58, no arguments): FUN_001a40b8 draws every
            // pickable prize into the player (FUN_001a3ad8, mode 1) and FUN_001bda98 hands every item
            // object to the player (FUN_001bd1d8). No BDX script on the disc calls it (`kh2 bdx --scan`).
            // 6.9 target_distance (0x2eeb30 -> FUN_0018f308): the distance from the point to the surface of the target record's
            // shape. The app's record is always the player's (host 1.120); his shape is taken as the point at his origin, so
            // the distance is to that point.
            Call::Func { group: 6, index: 9, args } => match args.get(1) {
                Some(Arg::Vec(p)) => approx(Value::Float(from_game(*p).distance(self.env.player)), "the player's hurt shape (a sphere or spheroid, FUN_00189628) is taken as a point at his origin: the radius is not subtracted"),
                _ => missing(Value::Float(0.0), "target_distance without a point"),
            },
            Call::Unknown { group: 6, index: 31, .. } | Call::Func { group: 6, index: 31, .. } => {
                self.obj.collect_all = true;
                imp(Value::None)
            }
            // 6.24 (0x2ee960, `kh2_bdx::names` obj_vec_8a0): the 16 bytes at +0x8a0 of the object's +4 record, copied to the static buffer
            // at 0x1e25680. FUN_00182690 writes them each tick as (position +0x5c0 - previous position +0x540 - the offset +0x5a0) times
            // DAT_00356f48: the displacement the object really made, the same quantity as +0x860 (1.62) up to the +0x5a0 offset (zero unless
            // the object rides something; its writers were not traced). M_EX210's wait loop (0x105f) compares its length with half of 1.64's.
            Call::Func { group: 6, index: 24, args } if args.first().map(|a| a.int() as u32) == Some(self.obj.handle) => {
                let v = self.obj.moved;
                approx(Value::Vec([v.x, -v.y, -v.z, 0.0]), "the displacement made this tick as for 1.62; the +0x5a0 offset FUN_00182690 subtracts is taken as zero [inferred]")
            }
            Call::Func { group: 6, index: 24, .. } => missing(Value::Vec([0.0; 4]), "obj_vec_8a0 of an object other than self"),
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
        if shared_blocks().read(addr, out) {
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
        // The object's own target record (host 2.23: object +0x9f4), laid out like the records of
        // event 11 (FUN_0018ed40 + FUN_0018f490: object, position index, 0, -1, 0). Its object is the
        // player; the position index is taken as 0 [inferred: the app has no lock-on parts]. The
        // Dusk's 0x136d reads word 0 when range trigger 16 is on (host 1.150 bit 1).
        let t = addr.wrapping_sub(TARGET) as usize;
        if t + out.len() <= 20 {
            out.copy_from_slice(&target_record()[t..t + out.len()]);
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
        let Some(b) = self.obj.mem.get_mut(o..o + data.len()) else { return shared_blocks().write(addr, data) };
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

    /// Host 1.13 on the object itself queues motions for M_EX900 only; every other kind keeps the old unmapped answer.
    #[test]
    fn the_motion_queue_is_enabled_for_m_ex900_only() {
        assert!(queues_motions("M_EX900#3"));
        for t in ["M_EX930#1", "M_EX940#2", "M_EX850#1", "M_EX760#4", "M_EX420#5", "Dusk#1"] {
            assert!(!queues_motions(t), "{t}");
        }
    }

    /// The Dusk reads word 0 of its target record (BDX `M_EX990` 0x136d -> R0217 at 0x021b): the player.
    #[test]
    fn the_own_target_record_names_the_player() {
        let r = target_record();
        assert_eq!(u32::from_le_bytes(r[0..4].try_into().unwrap()), PLAYER);
        assert_eq!(i32::from_le_bytes(r[12..16].try_into().unwrap()), -1);
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
        let d = compute_drop(&r, 1.5, 1.0, &mut seed, 256);
        assert_eq!(d.prizes, [0, 3, 0, 0, 5, 0, 0, 0, 2]);
        // 10 prizes, 3 steps each (the launch values, in order), then 3 item steps, four more per drop
        // (the item object's yaw and its three launch values).
        let mut s = 1u32;
        let mut values = Vec::new();
        for _ in 0..30 {
            s = s.wrapping_mul(0x10dcd);
            values.push(s);
        }
        assert_eq!(d.launches.len(), 10);
        assert_eq!(d.launches[0], (1, [values[0], values[1], values[2]]));
        assert_eq!(d.launches[3].0, 4);
        assert_eq!(d.launches[9], (8, [values[27], values[28], values[29]]));
        let mut want = Vec::new();
        let mut want_launch = Vec::new();
        for (id, chance) in r.items {
            s = s.wrapping_mul(0x10dcd);
            if (s as f32) * 2.328_306_4e-10 * 100.0 < chance as f32 {
                want.push(id);
                let mut v = [0u32; 4];
                for x in &mut v {
                    s = s.wrapping_mul(0x10dcd);
                    *x = s;
                }
                want_launch.push((id, v));
            }
        }
        assert_eq!(seed, s);
        assert_eq!(d.items, want);
        assert_eq!(d.item_launches, want_launch);
        assert!(!d.items.contains(&10), "a chance of 0 never drops");
        assert!(d.items.contains(&11), "a chance of 100 always drops");
        // A full pool: no prize object, no launch steps (FUN_001c4d78 returns 0).
        let mut seed2 = 1u32;
        let full = compute_drop(&r, 1.5, 1.0, &mut seed2, 0);
        assert!(full.launches.is_empty());
        assert_eq!(full.prizes, d.prizes);
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
