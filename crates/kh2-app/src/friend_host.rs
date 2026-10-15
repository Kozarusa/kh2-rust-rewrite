//! The party members' script host: the enemy host (`BevyHost`) plus what only a friend object answers
//! (docs/party.md section 5, host group 10 at table 0x371810) and the player-type motion remap of the
//! motion requests (FUN_0017e000 -> FUN_0017da30).
//!
//! Group 10 entries the members' scripts reach (`kh2 bdx P_EX020` / `P_EX030`: Donald 10.31; Goofy 10.14,
//! 10.31, 10.40; 10.55 is the runtime's) are answered here; the rest of group 10 goes to the enemy host,
//! which reports it as missing in the `KH2_BDXLOG` table. Everything else is the enemy host's.

use bevy::prelude::*;
use kh2_bdx::host::{Arg, Call, Host, Value};

use crate::enemy_ai::AiTrace;
use crate::enemy_host::{BevyHost, St};

/// What the group-10 entries and the remap read or write of the member (kept on `FriendBrain`).
#[derive(Clone, Debug, Default)]
pub struct FriendFields {
    /// +0xb7c bit 0x10000: the script set the member's target (host 10.14 with a mode other than 0).
    pub script_target: bool,
    /// +0x160 bits 4/8/0x10: the member is free (host 10.36).
    pub free: bool,
    /// FUN_0017da30's slot order for the member's motion state now.
    pub order: [u8; 4],
    /// +0xaf4: the MP the special's mode costs (FUN_00113d48 writes it); host 1.276 pays it.
    pub pending_mp: u8,
    /// What host 1.276 paid this tick (`friend_ai` hands it to the member's MP, FUN_00186368).
    pub paid: Option<u8>,
    /// The member's target record (+0x9f4) is valid: an enemy target, or the one a special or Cure was
    /// chosen for (FUN_00113d48 copies phase 1's record there). Host 1.140 reads it.
    pub target_valid: bool,
    /// The member's attack objects (host 2.10 / 2.31), kept from tick to tick until they are released
    /// and destroyed (FUN_0018c2d8), and the hits the script delivered on its target record (host
    /// 2.17); `friend_ai` steps the objects and hands the 2.17 hits to `party`.
    pub attacks: Vec<AttackObj>,
    pub target_hits: Vec<i16>,
    /// Attack objects made so far (their handles) and the hit groups taken (FUN_001bdf00 stand-in).
    pub made: u32,
    pub groups: u32,
    /// The point of the member's target record (host 1.120): the target's hurt shape centre, game
    /// units with the app's axes; `None` without a target. `party_tick` writes it.
    pub target_point: Option<Vec3>,
    /// The radius of the hurt shape the member targets (host 6.9).
    pub target_radius: f32,
    /// The effects the script holds handles to (hosts 1.22, 1.87, 1.149); `friend_ai` starts, moves,
    /// stops and destroys their instances and tells the script whether they still run.
    pub fx: Vec<FxLive>,
    pub effects_made: u32,
    /// The motion slot host 1.13 queued after the running motion (FUN_0017e170).
    pub queued: Option<usize>,
    /// The motion numbers the script asked for and the slots they play (FUN_0017e000 writes the number
    /// to +0x154, which host 1.15 reads): the last request and the queued one.
    pub motion: Option<(i32, usize)>,
    pub queued_motion: Option<(i32, usize)>,
    /// Host 1.91 (FUN_00175aa8): the invincibility timer +0xb00 the script set this tick (frames).
    pub invuln: Option<f32>,
    /// The object the member's target record (+0x9f4) names while a special runs (FUN_00113d48 copies
    /// phase 1's record): the record's word 0 (`TARGET_OTHER` / `TARGET_ENEMY` stand for the other
    /// member and the enemy).
    pub special: Option<crate::party::SpecialOn>,
    /// Host 1.247's state: the game's second generator (0x3716a0) and the bucket counter (0x35e9b0).
    /// [inferred] The game shares both with every other user; each member keeps its own here.
    pub scatter: crate::magic_rules::ScatterState,
    /// The member's hand-0 object as its script sees it (host 1.234 names it; Goofy's `blade` throws it).
    /// `None`: the member holds no hand object.
    pub weapon: Option<WeaponObj>,
    /// The enemy an attack object's callback (2.38) was called for (FUN_0018cf50), and the one host 1.160
    /// then wrote into the member's target record.
    pub callback_victim: Option<Entity>,
    pub retarget: Option<Entity>,
    /// What a target search (host 6.5, FUN_002f04a8) wrote into the member's target record this tick:
    /// `Some(None)` cleared it, `Some(Some((enemy, shape)))` names an enemy's part (the shape's index among
    /// its BATTLE / TARGET shapes, as `party` keeps it), with the parts the search walked (byte +5).
    pub searched: Option<(Option<(Entity, usize)>, u8)>,
}

/// One target part a member's target search (host 6.5) may pick: an enemy's TARGET shape (the collision
/// record of type 3 FUN_0016b4e0 walks) with its id (byte 0, `Shape::group`), its index among the
/// enemy's BATTLE / TARGET shapes (`party`'s target), its point (the hurt shape's centre, as
/// FUN_001894d0's bone frame times the offset moved up by the lift; game units, the app's axes) and radius.
#[derive(Clone, Debug, PartialEq)]
pub struct SearchPart {
    pub entity: Entity,
    pub shape: usize,
    pub part: u8,
    pub point: Vec3,
    pub radius: f32,
}

/// The object handle a search part's enemy has in the members' scripts (the history of the search block
/// keeps it from search to search): past the hand objects, the entity's index.
pub(crate) fn enemy_handle(e: Entity) -> u32 {
    ENEMY_HANDLE + (e.to_bits() as u32 & 0xff_ffff)
}

const ENEMY_HANDLE: u32 = 0x7e00_0000;

/// The member's hand object as a scripted object (the weapon list +0xaf8 holds it; FUN_0017c718 made it):
/// linked to the hand (1.18 / 1.102) or free, then placed by the script (1.148) and moved by its secondary
/// velocity (1.89, +0x880) each frame. Positions are game units with the app's axes.
#[derive(Clone, Debug, PartialEq)]
pub struct WeaponObj {
    pub handle: u32,
    /// Linked to the member's hand (+0x570 set): the hand moves it.
    pub linked: bool,
    /// The root's position and facing (app yaw) while free; the velocity per frame (1.89).
    pub pos: Vec3,
    pub yaw: f32,
    pub vel: Vec3,
    /// The motion the script asked for while it is free (1.11; [inferred] the number is the slot, the hand
    /// object being no player-type object).
    pub motion: Option<i32>,
    /// The bones' world matrices (Bevy space) and the root's, from the last drawn pose (host 1.83, 1.147).
    pub bones: Vec<Mat4>,
    pub root: Mat4,
}

/// Handles of the members' hand objects (host 1.234), past the attack and effect handles.
pub(crate) const WEAPON_HANDLE: u32 = 0x7d00_0000;

/// Object handles the members' target record names for the other member and an enemy.
pub(crate) const TARGET_OTHER: u32 = 0x7c00_0001;
pub(crate) const TARGET_ENEMY: u32 = 0x7c00_0002;

/// An effect handle of the member's script and what became of its instance.
#[derive(Clone, Debug, PartialEq)]
pub struct FxLive {
    pub handle: u32,
    pub req: FxReq,
    /// The instance (`None` before `friend_ai` started it, or when it could not start).
    pub entity: Option<Entity>,
    /// `friend_ai` has handled the start request.
    pub started: bool,
    /// The position FUN_001d9d00 answers (host 0.53): the element's bone frame times its translation
    /// for a bound element, the point given for a placed one; game units, the app's axes.
    pub pos: Vec3,
    /// The instance has ended (FUN_001da0d8 says so: host 0.49 false).
    pub gone: bool,
    /// Asked by the script: stop emitting (0.44 / 0.52), destroy at once (0.51), placed anew (0.38).
    pub stop: bool,
    pub destroy: bool,
    pub moved: bool,
    /// Host 0.41: the angles FUN_0017be00 made of a direction, written into the instance's second vector
    /// (FUN_001d9d50): the element's rotation, as the hit spark's (`fx::hit_rotation`).
    pub turn: Option<[f32; 3]>,
    /// Ticks since the instance ended (the record is dropped some time after).
    pub gone_ticks: u32,
}

/// An effect element of the member's own effect file (`obj/<model>.apdx`, the object's +0x80) the
/// script asked for.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum FxReq {
    /// 1.87 (FUN_0016a570 with owner 0 = the object) / 1.149 (FUN_0016a570 with another owner):
    /// `FUN_001db868(obj + 0x80, id, .., 1, .., owner)`: bound to the owner (`None`: the member; else
    /// the object handle the script named: the player, `TARGET_OTHER`, `TARGET_ENEMY`).
    Bound { id: u16, on: Option<u32> },
    /// 1.22 (FUN_0016a5b8): `FUN_0016a530` (no owner), then the effect's position (FUN_001d9d00) is
    /// overwritten with the vector: game units, the app's axes.
    At { id: u16, pos: Vec3 },
}

/// A member's attack object (FUN_001a2ba0 -> FUN_0018bd08, 0x140 bytes; the fields by game offset).
/// Positions are game units with the app's axes.
#[derive(Clone, Debug, PartialEq)]
pub struct AttackObj {
    pub handle: u32,
    /// +0x14 the `atkp` id (2.10 argument 1) and +0x18 the sub key (argument 3): FUN_0019e788 picks
    /// the row (`magic_rules::attack_row`).
    pub atkp: u16,
    pub kind: u32,
    /// +0x20: the hit group (the row's ComboGroup +0x2c when not 0, else argument 2, else a new one).
    pub group: u32,
    /// +0xe0 the position and +0xf0 the one before (FUN_001a2fc8); `None` before the first move.
    pub pos: Option<Vec3>,
    pub prev: Option<Vec3>,
    /// +0x100 the radius and +0x104 the half height (FUN_001a30e0: the radius when not positive).
    pub radius: f32,
    pub height: f32,
    /// +0xcc the frames of the growth (FUN_001a3480; 0 = none), +0xc8 the frames run, +0xd0 / +0xd4
    /// the size it starts from (FUN_001a2cc0 eases to +0x100 / +0x104 and releases the object after).
    pub grow: f32,
    pub t: f32,
    pub from: (f32, f32),
    /// +4 bit 0x800: released (FUN_0018dc98); FUN_0018c2d8 destroys it on its next pass.
    pub released: bool,
    /// Passes of FUN_001a2cc0 run on it (for the log).
    pub steps: u32,
    /// +4 bit 2: it hit a target (FUN_0018d768's hit); bits 8 / 0x10: it met a wall or a floor
    /// (FUN_0018c8e0 -> FUN_0018ca78). Hosts 2.14 / 2.29 read them (FUN_0018c890).
    pub hit: bool,
    pub wall: bool,
    /// +0x8c..+0x94 (host 2.38): the routine FUN_0018cf50 runs on the script's thread for every hit, with its
    /// argument (the routine's address, the argument).
    pub callback: Option<(u32, u32)>,
}

/// Host 0.45 (FUN_0016d098): `a + b / |b.xyz| x step x DAT_00356f40` on all four lanes (the frame step 1.0).
pub(crate) fn advance_vec(a: [f32; 4], b: [f32; 4], step: f32) -> [f32; 4] {
    let len = (b[0] * b[0] + b[1] * b[1] + b[2] * b[2]).sqrt();
    let q = if len > 0.0 { step / len } else { 0.0 };
    std::array::from_fn(|k| a[k] + b[k] * q)
}

/// Host 0.46 (FUN_0016d138 -> FUN_0017c0f8): `a` turned toward `b` by at most `rate` (times the frame step
/// 1.0). Both are normalised by their xyz length; the angle between them is FUN_0017bf10's (acos of the dot,
/// 0 at or above 1.0 `DAT_0037a53c`, pi at or below -1.0 `DAT_0037a540` / `DAT_0037a544`), held at `rate`;
/// FUN_0017bf88 builds the rotation by that angle about the unit `a x b` (rows `x^2 t + c, xy t + z s, xz t -
/// y s` .., `t = 1 - c`), and the result is `a` through it with w 0. A zero cross product gives a zero axis
/// (the vector unit's division by zero times 0): the result is `a x cos(angle)`.
pub(crate) fn turn_vec(a: [f32; 4], b: [f32; 4], rate: f32) -> [f32; 4] {
    let unit = |v: [f32; 4]| {
        let l = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
        if l > 0.0 { [v[0] / l, v[1] / l, v[2] / l] } else { [0.0; 3] }
    };
    let (u, w) = (unit(a), unit(b));
    let dot = u[0] * w[0] + u[1] * w[1] + u[2] * w[2];
    let angle = if dot >= 1.0 {
        0.0
    } else if dot <= -1.0 {
        std::f32::consts::PI
    } else {
        dot.acos()
    };
    let step = angle.min(rate);
    let k = unit([u[1] * w[2] - u[2] * w[1], u[2] * w[0] - u[0] * w[2], u[0] * w[1] - u[1] * w[0], 0.0]);
    let (s, c) = step.sin_cos();
    let t = 1.0 - c;
    let (x, y, z) = (k[0], k[1], k[2]);
    let rows = [[x * x * t + c, x * y * t + z * s, x * z * t - y * s], [x * y * t - z * s, y * y * t + c, y * z * t + x * s], [x * z * t + y * s, y * z * t - x * s, z * z * t + c]];
    let r: [f32; 3] = std::array::from_fn(|j| rows[0][j] * u[0] + rows[1][j] * u[1] + rows[2][j] * u[2]);
    [r[0], r[1], r[2], 0.0]
}

/// Handles of the members' attack objects (host 2.10), apart from the object handles.
const ATTACK_HANDLE: u32 = 0x7a00_0000;
/// Handles of the effects the members' scripts start (1.22, 1.87, 1.149).
const EFFECT_HANDLE: u32 = 0x7b00_0000;

/// The member's script host.
pub(crate) struct FriendHost<'a, 'b> {
    pub(crate) inner: BevyHost<'a>,
    pub(crate) fields: &'b mut FriendFields,
    /// The `atkp` rows (the attack objects' rows, FUN_0019e788).
    pub(crate) game: &'b crate::data::GameData,
    /// Host 1.247's numbers from the executable (`None`: not readable; the call is then missing).
    pub(crate) free: Option<&'b crate::party_rules::FreePointConsts>,
    /// The enemies' target parts host 6.5 walks (`friend_ai` lists them each tick).
    pub(crate) targets: &'b [SearchPart],
}

fn words(c: &Call) -> Option<(u16, u16, Vec<u32>)> {
    match c {
        Call::Unknown { group, index, args } => Some((*group, *index, args.clone())),
        Call::Func { group, index, args } => Some((
            *group,
            *index,
            args.iter()
                .map(|a| match a {
                    Arg::Float(f) => f.to_bits(),
                    a => a.int() as u32,
                })
                .collect(),
        )),
        _ => None,
    }
}

impl FriendHost<'_, '_> {
    fn trace(&mut self) -> &mut AiTrace {
        &mut *self.inner.trace
    }

    fn record(&mut self, c: &Call, v: &Value, st: St, why: &'static str) {
        let tag = self.inner.tag;
        self.trace().record(tag, c, v, st, why);
    }

    fn attack(&mut self, handle: u32) -> Option<&mut AttackObj> {
        self.fields.attacks.iter_mut().find(|a| a.handle == handle && !a.released)
    }

    /// FUN_0018bd08's part the app keeps: the row's sub key and the hit group (+0x20 = the row's
    /// ComboGroup when not 0, else the group given, else a new one from FUN_001bdf00 [the app's own
    /// counter: the victims' memory in `friend_ai` is the members' own]).
    fn new_attack(&mut self, atkp: u16, kind: u32, group: Option<u32>) -> Value {
        let combo = self.game.atkp.iter().find(|r| r.id == atkp && (r.sub_id >> 8) as u32 == kind).map_or(0, |r| r.combo_group as u32);
        let group = match (combo, group) {
            (c, _) if c != 0 => c,
            (_, Some(g)) => g,
            _ => {
                self.fields.groups += 1;
                0x100 + self.fields.groups
            }
        };
        self.fields.made += 1;
        let handle = ATTACK_HANDLE + self.fields.made;
        self.fields.attacks.push(AttackObj { handle, atkp, kind, group, pos: None, prev: None, radius: 0.0, height: 0.0, grow: 0.0, t: 0.0, from: (0.0, 0.0), released: false, steps: 0, hit: false, wall: false, callback: None });
        Value::Handle(handle)
    }

    fn new_effect(&mut self, req: FxReq) -> Value {
        self.fields.effects_made += 1;
        let handle = EFFECT_HANDLE + self.fields.effects_made;
        let pos = match req {
            FxReq::At { pos, .. } => pos,
            FxReq::Bound { .. } => self.inner.obj.motor.pos,
        };
        self.fields.fx.push(FxLive { handle, req, entity: None, started: false, pos, gone: false, stop: false, destroy: false, moved: false, turn: None, gone_ticks: 0 });
        Value::Handle(handle)
    }

    fn effect(&mut self, handle: u32) -> Option<&mut FxLive> {
        self.fields.fx.iter_mut().find(|f| f.handle == handle && !f.destroy)
    }

    /// The effect-handle functions of group 0 on the member's own effects. `None`: not one of them or
    /// not one of the member's handles.
    fn effect_calls(&mut self, c: &Call) -> Option<Value> {
        let Call::Func { group: 0, index, args } = c else { return None };
        let h = args.first().map(|a| a.int() as u32)?;
        if !(EFFECT_HANDLE..EFFECT_HANDLE + 0x100_0000).contains(&h) {
            return None;
        }
        let (v, st, why) = match index {
            // 0.49 effect_alive (0x16dbd0 -> FUN_001da0d8): the instance still runs.
            49 => (Value::Bool(self.effect(h).is_some_and(|f| !f.gone)), St::Approx, "the instance entity still exists"),
            // 0.50 effect_valid (0x16dc00): the handle has an instance (+0x1c).
            50 => (Value::Bool(self.effect(h).is_some_and(|f| !f.gone)), St::Approx, "the instance entity still exists"),
            // 0.53 effect_position (0x16dc88 -> FUN_001d9d00): the bind record's position.
            53 => {
                let own = self.inner.obj.motor.pos;
                let p = self.effect(h).map_or(own, |f| f.pos);
                (Value::Vec(crate::enemy_obj::to_game(p)), St::Approx, "the element's bone frame times its translation (bound) or the point given (placed), one tick old")
            }
            // 0.44 effect_stop (FUN_001e0298) / 0.85 (and the stop mark): the instance stops emitting.
            // 0.52 effect_fade: [inferred] taken as a stop (the alpha ramp is not modelled).
            44 | 85 | 52 => {
                if let Some(f) = self.effect(h) {
                    f.stop = true;
                }
                (Value::None, if *index == 52 { St::Approx } else { St::Impl }, "a fade is taken as a stop")
            }
            // 0.51 effect_destroy (0x16dc30): the instance is destroyed at once.
            51 => {
                if let Some(f) = self.effect(h) {
                    f.destroy = true;
                }
                (Value::None, St::Impl, "")
            }
            // 0.41 effect_set_vec2 (0x16db50): FUN_0017be00's angles of the direction (pitch atan2(-y, |xz|),
            // yaw atan2(x, z), 0) into the instance's second vector (FUN_001d9d50).
            41 => {
                let dir = match args.get(1) {
                    Some(Arg::Vec(v)) => crate::enemy_obj::from_game(*v),
                    _ => return None,
                };
                if let Some(f) = self.effect(h) {
                    f.turn = Some(crate::fx::hit_rotation(dir));
                }
                (Value::None, St::Approx, "the angles become the element's rotation (as the hit spark's placement); the instance's own use of the vector was not read")
            }
            // 0.38 effect_set_position (0x16da90): the effect's position.
            38 => {
                let p = match args.get(1) {
                    Some(Arg::Vec(v)) => crate::enemy_obj::from_game(*v),
                    _ => return None,
                };
                if let Some(f) = self.effect(h) {
                    f.pos = p;
                    f.moved = true;
                }
                (Value::None, St::Impl, "")
            }
            _ => return None,
        };
        self.record(c, &v, st, why);
        Some(v)
    }

    /// The hosts on the member's hand object (Goofy's `blade`, `kh2 bdx P_EX030` R0d94: 1.234 names it, the
    /// link calls take it off the hand and back, 1.83 / 1.148 place it, 1.89 / 1.79 / 1.11 fly it) and
    /// 1.160 / 2.38 / 2.40 / 2.67 of the throw's attack. `None`: not one of them.
    fn weapon_calls(&mut self, c: &Call) -> Option<Value> {
        let me = self.inner.obj.handle;
        let wh = self.fields.weapon.as_ref().map(|w| w.handle);
        let raw = |a: &Arg| match a {
            Arg::Float(f) => f.to_bits(),
            a => a.int() as u32,
        };
        let is_w = |o: u32| wh == Some(o);
        let (v, st, why): (Value, St, &'static str) = match c {
            // 1.234 obj_word_af8 (0x174280): the word at +0xaf8 + 4 i, the weapon list FUN_0017c718 fills.
            Call::Func { group: 1, index: 234, args } if args.first().is_some_and(|a| raw(a) == me) => {
                let i = args.get(1).map_or(0, |a| a.int());
                match (i, wh) {
                    (0, Some(h)) => (Value::Handle(h), St::Impl, ""),
                    _ => (Value::Int(0), St::Approx, "no hand object in that slot (the app keeps hand 0 only)"),
                }
            }
            Call::Position { obj } if is_w(*obj) => {
                let w = self.fields.weapon.as_ref().expect("weapon");
                let p = if w.linked { w.root.w_axis.truncate() / crate::SCALE } else { w.pos };
                (Value::Vec(crate::enemy_obj::to_game(p)), St::Approx, "the hand object's root: the drawn one while linked, the script's own while free")
            }
            Call::FacingVector { obj } if is_w(*obj) => {
                let w = self.fields.weapon.as_ref().expect("weapon");
                let f = if w.linked { w.root.z_axis.truncate().with_y(0.0).normalize_or_zero() } else { Vec3::new(w.yaw.sin(), 0.0, w.yaw.cos()) };
                let g = crate::enemy_obj::to_game(f);
                (Value::Vec([g[0], g[1], g[2], 0.0]), St::Approx, "the hand object's facing")
            }
            // 1.94 obj_valid: the hand object exists.
            Call::Func { group: 1, index: 94, args } if args.first().is_some_and(|a| is_w(raw(a))) => (Value::Bool(true), St::Impl, ""),
            // 1.161 has_link (0x1706c8): +0x570 set.
            Call::Func { group: 1, index: 161, args } if args.first().is_some_and(|a| is_w(raw(a))) => (Value::Bool(self.fields.weapon.as_ref().is_some_and(|w| w.linked)), St::Impl, ""),
            // 1.102 link_end (0x170e38 -> FUN_001becb8): off the hand; it stays where it was drawn.
            Call::Func { group: 1, index: 102, args } if args.first().is_some_and(|a| is_w(raw(a))) => {
                let w = self.fields.weapon.as_mut().expect("weapon");
                if w.linked {
                    w.linked = false;
                    w.pos = w.root.w_axis.truncate() / crate::SCALE;
                    let f = w.root.z_axis.truncate();
                    w.yaw = f.x.atan2(f.z);
                    w.vel = Vec3::ZERO;
                }
                (Value::None, St::Approx, "the link record and its routine are not kept: the hand object is free at its drawn place")
            }
            // 1.18 link_start (0x170dd8 -> FUN_001beb90): linked to the object named (the member's hand: 1.235
            // gives the joint), the movement vector cleared.
            Call::Func { group: 1, index: 18, args } if args.first().is_some_and(|a| is_w(raw(a))) => {
                let w = self.fields.weapon.as_mut().expect("weapon");
                w.linked = true;
                w.vel = Vec3::ZERO;
                w.motion = None;
                (Value::None, St::Approx, "linked back to the member's hand bone whatever joint is given")
            }
            // 1.83 bone_to_world (0x172e68 -> FUN_0016aa18): the vector through bone `i`'s matrix.
            Call::Func { group: 1, index: 83, args } if args.first().is_some_and(|a| is_w(raw(a))) => {
                let (Some(i), Some(Arg::Vec(v))) = (args.get(1).map(|a| a.int()), args.get(2)) else { return None };
                let w = self.fields.weapon.as_ref().expect("weapon");
                let m = usize::try_from(i).ok().and_then(|i| w.bones.get(i)).copied().unwrap_or(w.root);
                let p = m.transform_point3(crate::enemy_obj::from_game(*v)) / crate::SCALE;
                (Value::Vec(crate::enemy_obj::to_game(p)), St::Approx, "the bone's world matrix of the last drawn pose (one tick old)")
            }
            // 1.148 set_position (0x170658): the position (w 1).
            Call::Func { group: 1, index: 148, args } if args.first().is_some_and(|a| is_w(raw(a))) => {
                let Some(Arg::Vec(v)) = args.get(1) else { return None };
                let w = self.fields.weapon.as_mut().expect("weapon");
                w.pos = crate::enemy_obj::from_game(*v);
                (Value::None, St::Impl, "")
            }
            // 1.89 set_velocity (0x172f90): +0x880 = the direction made unit, the speed in w; [inferred] the free
            // hand object moves by it each frame (its mover's consumer was not read).
            Call::SetVelocity { obj, dir, speed } if is_w(*obj) => {
                let d = crate::enemy_obj::from_game(*dir).normalize_or_zero();
                self.fields.weapon.as_mut().expect("weapon").vel = d * *speed;
                (Value::None, St::Approx, "the free hand object moves by direction x speed each frame [inferred consumer]")
            }
            Call::FaceToward { obj, dir } | Call::FaceDirection { obj, dir } if is_w(*obj) => {
                let d = crate::enemy_obj::from_game(*dir);
                if d.x != 0.0 || d.z != 0.0 {
                    self.fields.weapon.as_mut().expect("weapon").yaw = d.x.atan2(d.z);
                }
                (Value::None, St::Approx, "the hand object turns at once (no lock)")
            }
            Call::PlayMotion { obj, motion, .. } | Call::PlayMotionChecked { obj, motion, .. } if is_w(*obj) => {
                self.fields.weapon.as_mut().expect("weapon").motion = Some(*motion);
                (Value::None, St::Approx, "the motion number taken as the hand object's slot; the blend is not kept")
            }
            // 1.126 / 1.236 (0x171670 / 0x1716a0) on the hand object's motion controller, 1.235 (0x1742a0) the
            // joint for 1.18: the app links no controllers.
            Call::Unknown { group: 1, index: 126 | 235 | 236, .. } if wh.is_some() => (Value::Int(0), St::Stub, "the app links no motion controllers; the joint is the member's hand bone"),
            // 1.160 (0x173770 -> FUN_0018f490): the target record names the object (position index 0). The
            // throw's callback (R10f7) points the member's target at the enemy hit.
            Call::Unknown { group: 1, index: 160, args } if args.get(1) == Some(&TARGET_ENEMY) => {
                self.fields.retarget = self.fields.callback_victim;
                (Value::None, St::Approx, "the member's target record names the enemy the attack object hit (shape 0)")
            }
            // 2.38 attack_callback_8c (0x18e4f0): +0x8c the running thread, +0x90 the routine, +0x94 its argument.
            Call::Func { group: 2, index: 38, args } if args.len() >= 3 && self.fields.attacks.iter().any(|x| x.handle == raw(&args[0])) => {
                let (h, pc, arg) = (raw(&args[0]), raw(&args[1]), raw(&args[2]));
                if let Some(o) = self.fields.attacks.iter_mut().find(|x| x.handle == h) {
                    o.callback = Some((pc, arg));
                }
                (Value::None, St::Approx, "the routine runs as a throw-away thread for every hit (FUN_0018cf50 -> FUN_0016c550), not on the registering thread")
            }
            // 2.40 attack_flag_8 (0x18e530): +4 bit 8, a wall (FUN_0018ca78 sets 8 for a polygon of another
            // kind than 1, 0x10 for kind 1).
            Call::Func { group: 2, index: 40, args } if args.first().is_some_and(|a| self.fields.attacks.iter().any(|x| x.handle == raw(a))) => {
                let h = raw(&args[0]);
                (Value::Bool(self.fields.attacks.iter().any(|x| x.handle == h && x.wall)), St::Approx, "8 and 0x10 (the polygon's kind) are not told apart")
            }
            // 2.67 new_hit_group (0x18e828 -> FUN_001bdf00): the next group id (the members' own counter).
            Call::Func { group: 2, index: 67, .. } => {
                self.fields.groups += 1;
                (Value::Int(0x100 + self.fields.groups as i32), St::Approx, "the members' own counter, not the game's 0x1d5dd4c")
            }
            _ => return None,
        };
        self.record(c, &v, st, why);
        Some(v)
    }

    /// The attack-object hosts of group 2 and the effect starts of group 1 the members' specials use
    /// (`kh2 bdx P_EX020` thunder R13c8, cure R1559). `None`: not one of them.
    fn specials(&mut self, c: &Call) -> Option<Value> {
        if let Some(v) = self.effect_calls(c) {
            return Some(v);
        }
        if let Some(v) = self.weapon_calls(c) {
            return Some(v);
        }
        let me = self.inner.obj.handle;
        let raw = |a: &Arg| match a {
            Arg::Float(f) => f.to_bits(),
            a => a.int() as u32,
        };
        match c {
            // 2.12 attack_move_to (0x18e3a0 -> FUN_001a2fc8): the previous position is kept for the swept
            // test once the object has a position (+4 bit 1), the new one stored; bit 4 (a move of more
            // than two frame units) is what the sweep of FUN_001a2cc0 reads, `friend_ai` measures it.
            Call::Func { group: 2, index: 12, args } => {
                let (h, at) = match (args.first(), args.get(1)) {
                    (Some(h), Some(Arg::Vec(v))) => (raw(h), crate::enemy_obj::from_game(*v)),
                    _ => return None,
                };
                if let Some(a) = self.attack(h) {
                    a.prev = a.pos.or(Some(at));
                    a.pos = Some(at);
                    self.record(c, &Value::None, St::Impl, "");
                } else {
                    self.record(c, &Value::None, St::Missing, "2.12 on a handle that is not a live attack object of the member");
                }
                Some(Value::None)
            }
            // 0.45 / 0.46 (the runtime hands the two vectors and the scalar; the answer goes back into the
            // first): `advance_vec` (FUN_0016d098) and `turn_vec` (FUN_0016d138 -> FUN_0017c0f8).
            Call::Func { group: 0, index: i @ (45 | 46), args } if args.len() >= 3 => {
                let (Arg::Vec(a), Arg::Vec(b)) = (&args[0], &args[1]) else { return None };
                let s = args[2].float();
                let v = Value::Vec(if *i == 45 { advance_vec(*a, *b, s) } else { turn_vec(*a, *b, s) });
                self.record(c, &v, St::Approx, "the platform's sqrt / acos / sin / cos, not the vector unit's and the game's kernels");
                Some(v)
            }
            // 1.130 obj_diff (0x170510): the difference of the two contexts' objects, 0 when either is 0 or
            // they are the same (Donald's `cure` R1570 tells healing itself from healing another).
            Call::Func { group: 1, index: 130, args } if args.len() >= 2 => {
                let (a, b) = (raw(&args[0]), raw(&args[1]));
                let v = Value::Int(if a == 0 || b == 0 || a == b { 0 } else { a.wrapping_sub(b) as i32 });
                self.record(c, &v, St::Impl, "");
                Some(v)
            }
            // 1.13 queue_motion (0x1703d8 -> FUN_0017e170): the motion goes into the controller's queue
            // (+0x3c, 4 entries) and follows the running one when it ends; `party` plays it then (one
            // entry kept; the blend is the member's own).
            Call::Func { group: 1, index: 13, args } if args.first().is_some_and(|a| raw(a) == me) => {
                let motion = args.get(1).map_or(-1, |a| a.int());
                let slot = usize::try_from(self.remap(motion)).ok().filter(|&s| self.inner.env.rig.mset.has(s));
                self.fields.queued = slot;
                self.fields.queued_motion = slot.map(|s| (motion, s));
                self.record(c, &Value::None, St::Approx, "one queued motion after the running one; its blend is not kept");
                Some(Value::None)
            }
            // 1.91 (0x172fe8 -> FUN_00175aa8): the invincibility timer +0xb00 = the frames given (Goofy's
            // `tornado` gives the length of its motion); FUN_00175ad8 refuses hits while it runs.
            Call::Unknown { group: 1, index: 91, .. } | Call::Func { group: 1, index: 91, .. } => {
                let frames = words(c).and_then(|(_, _, a)| a.get(1).copied()).map_or(0.0, f32::from_bits);
                self.fields.invuln = Some(frames);
                self.record(c, &Value::None, St::Impl, "");
                Some(Value::None)
            }
            // 6.3 (0x2ee718 -> FUN_002f01a0), 6.4 (0x2ee738 -> FUN_002f0748) and 6.5 (0x2ee750 ->
            // FUN_002f04a8) work on a block in the script's own memory: the runtime does them
            // (`kh2_bdx::search`, docs/party.md section 17) and asks `target_parts` below. They reach the host
            // only when the executable's numbers (0x383fb4..0x383fbc) could not be read.
            Call::Unknown { group: 6, index: 3..=5, .. } | Call::Func { group: 6, index: 3..=5, .. } => {
                self.record(c, &Value::None, St::Missing, "the executable's numbers of the target search are not readable: the member's target record stays");
                Some(Value::None)
            }
            // 1.247 random_free_point (0x1726d8 -> FUN_001a5ea8, mask 1): a free floor point between the two
            // radii around the vector (`party_rules::free_point`); the answer is the static buffer 0x1c88380.
            Call::Func { group: 1, index: 247, args } if args.len() >= 3 => {
                let Arg::Vec(at) = args[0] else { return None };
                let (lo, hi) = (args[1].float(), args[2].float());
                let centre = crate::enemy_obj::from_game(at);
                let Some(k) = self.free else {
                    let v = Value::Vec(at);
                    self.record(c, &v, St::Missing, "the executable's numbers for 1.247 are not readable: the centre is answered");
                    return Some(v);
                };
                let world = self.inner.env.world;
                let ground = |a: Vec3, b: Vec3| world.and_then(|w| w.ground(a, a.y, a.y - b.y)).map(|g| a.with_y(g.y));
                let clear = |a: Vec3, b: Vec3| world.is_none_or(|w| w.raycast(a, b).is_none());
                let p = crate::party_rules::free_point(centre, lo, hi, &mut self.fields.scatter, k, ground, clear);
                let v = Value::Vec(crate::enemy_obj::to_game(p));
                self.record(c, &v, St::Approx, "the floor cast (FUN_001d1648, mask 1) is the area's highest floor polygon in the span and the line test any polygon (World::raycast): the collision masks are not told apart");
                Some(v)
            }
            // 1.120 target_position (0x173688): the point of the member's target record.
            Call::TargetPosition { .. } => {
                let p = self.fields.target_point.unwrap_or(self.inner.obj.motor.pos);
                let v = Value::Vec(crate::enemy_obj::to_game(p));
                self.record(c, &v, St::Approx, "the centre of the hurt shape the member targets (its own position without a target) [inferred: the record's point]");
                Some(v)
            }
            // 1.141 probe_ground_down (FUN_00172538 -> FUN_001a5c30 with mask 4): from the point straight
            // down by the distance; the runtime writes the answer through the second argument.
            Call::Func { group: 1, index: 141, args } => {
                let from = match args.first() {
                    Some(Arg::Vec(v)) => *v,
                    _ => [0.0; 4],
                };
                let down = args.get(2).map_or(0.0, |a| a.float());
                let p = Vec3::new(from[0], -from[1], -from[2]);
                let found = self.inner.env.world.and_then(|w| w.ground(Vec3::new(p.x, 0.0, p.z), p.y, down.max(0.0)));
                let v = found.map_or(Value::Int(0), |g| Value::Vec([from[0], -g.y, from[2], 1.0]));
                self.record(c, &v, St::Approx, "the area's floor polygons under the point (World::ground), not FUN_001a5c30's mask-4 cast");
                Some(v)
            }
            // 1.22 spawn_effect_at (0x1717c0 -> FUN_0016a5b8): element `id` of the object's own effect file,
            // its position then the vector.
            Call::Func { group: 1, index: 22, args } if args.first().is_some_and(|a| raw(a) == me) => {
                let (id, pos) = match (args.get(1), args.get(2)) {
                    (Some(id), Some(Arg::Vec(v))) => (id.int().max(0) as u16, crate::enemy_obj::from_game(*v)),
                    _ => return None,
                };
                let v = self.new_effect(FxReq::At { id, pos });
                self.record(c, &v, St::Approx, "element `id` of the member's effect file placed at the point; the two last arguments are not modelled");
                Some(v)
            }
            // 1.21 spawn_effect (0x171780 -> FUN_0016a530): element `id` of the object's own effect file with no
            // owner (Donald's `blizzard` shard R0c82; the script places it with 0.38 at once).
            Call::Func { group: 1, index: 21, args } if args.first().is_some_and(|a| raw(a) == me) => {
                let id = args.get(1).map_or(0, |a| a.int().max(0) as u16);
                let v = self.new_effect(FxReq::At { id, pos: self.inner.obj.motor.pos });
                self.record(c, &v, St::Approx, "element `id` of the member's effect file, unbound, at the member's position until the script places it; the two last arguments are not modelled");
                Some(v)
            }
            // 6.9 (0x2eeb30 -> FUN_0018f308): the distance from the point to the surface of the record's shape
            // (FUN_0018eec0 builds it, FUN_00189628 measures: a sphere's is the centre distance minus the
            // radius). The member's target record names a hurt shape of the app's (a centre and a radius).
            Call::Func { group: 6, index: 9, args } if args.len() >= 2 => {
                let Arg::Vec(p) = args[1] else { return None };
                let at = crate::enemy_obj::from_game(p);
                let v = Value::Float(match self.fields.target_point {
                    Some(c) => at.distance(c) - self.fields.target_radius,
                    None => at.length(),
                });
                self.record(c, &v, St::Approx, "the member's target record whatever record is named; its shape taken as a sphere (kind 3) of the app's target part");
                Some(v)
            }
            // 1.87 spawn_attached (0x171800 -> FUN_0016a570, owner = the object).
            Call::SpawnAttached { obj, id, .. } if *obj == me => {
                let v = self.new_effect(FxReq::Bound { id: (*id).max(0) as u16, on: None });
                self.record(c, &v, St::Approx, "element `id` of the member's effect file bound to the member; the flag and extra arguments are not modelled");
                Some(v)
            }
            // 1.149 spawn_effect_for (0x171840 -> FUN_0016a570 with the third argument's object as owner):
            // element `id` of the first object's file bound to the other object. Donald's `cure` binds
            // element 12 to the object of its target record (R15cb).
            Call::Func { group: 1, index: 149, args } if args.first().is_some_and(|a| raw(a) == me) => {
                let id = args.get(1).map_or(0, |a| a.int().max(0) as u16);
                let on = args.get(2).map(raw).filter(|&o| o != me && o != 0);
                let v = self.new_effect(FxReq::Bound { id, on });
                self.record(c, &v, St::Approx, "element `id` of the member's effect file bound to the owner named (the member, the player, or the object of its target record)");
                Some(v)
            }
            _ => None,
        }
    }

    /// FUN_0017da30 on a motion number of the member's own player-type motion set: the slot the
    /// enemy host then plays. A number with no slot in the state's order stays as it is (the enemy
    /// host then reports it as not in the set).
    fn remap(&self, motion: i32) -> i32 {
        let rig = self.inner.env.rig;
        crate::party_battle::remap(motion, self.fields.order, |s| rig.mset.has(s)).map_or(motion, |s| s as i32)
    }

    fn group10(&mut self, index: u16, a: &[u32]) -> Option<(Value, St, &'static str)> {
        let arg = |k: usize| a.get(k).copied().unwrap_or(0);
        Some(match index {
            // 10.14 (0x2f6da0): mode 0 clears +0xb7c bit 0x10000, any other mode sets it after copying a
            // target record into +0x9f4 (1 the argument's, 2 the player's lock-on 0x34e9b0, 3 FUN_0010d720).
            // The app's member keeps its own target (party.rs); only the bit is carried.
            14 => {
                self.fields.script_target = arg(1) != 0;
                if arg(1) == 0 {
                    (Value::None, St::Impl, "")
                } else {
                    (Value::None, St::Approx, "the target bit is set; the record copy into the member's target is not carried out")
                }
            }
            // 10.19 (0x2f6e60): +0x10c bit 0x10000 (in a limit): no limits with partners yet (stage 5).
            19 => (Value::Bool(false), St::Approx, "no limit with partners in the app (stage 5): read clear"),
            // 10.31 (0x2f7118): +0x588 bit 0x400000, set only for summons (FUN_0010c5f8 path): a party
            // member is never one.
            31 => (Value::Bool(false), St::Impl, ""),
            // 10.36 (0x2f7230): free (+0x160 bits 4/8/0x10).
            36 => (Value::Bool(self.fields.free), St::Approx, "free = not in an attack motion (the controller's +0x160 bits are not modelled)"),
            // 10.40 (0x2f72e8 -> FUN_0017eb48): whether the second object's motion controller is linked
            // under the first's (+0x94 / +0x9c chain). The app links no controllers.
            40 => (Value::Bool(false), St::Approx, "the app links no motion controllers: read false"),
            // 10.55 (0x2f7540 -> FUN_002f0758) is the runtime's (`kh2_bdx::search`): bit 16 of the word at the
            // first argument + 4, on Goofy's target-search block in the script's own globals.
            _ => return None,
        })
    }
}

impl Host for FriendHost<'_, '_> {
    fn call(&mut self, c: &Call) -> Value {
        let me = self.inner.obj.handle;
        let played = match c {
            Call::PlayMotion { obj, motion, .. } | Call::PlayMotionChecked { obj, motion, .. } | Call::PlayMotionAt { obj, motion, .. } if *obj == me => Some((*motion, self.remap(*motion))),
            _ => None,
        };
        if let Some((motion, slot)) = played {
            if let Ok(s) = usize::try_from(slot) {
                self.fields.motion = Some((motion, s));
            }
            return match c {
                Call::PlayMotion { obj, blend, .. } => self.inner.call(&Call::PlayMotion { obj: *obj, motion: slot, blend: *blend }),
                Call::PlayMotionChecked { obj, blend, .. } => self.inner.call(&Call::PlayMotionChecked { obj: *obj, motion: slot, blend: *blend }),
                Call::PlayMotionAt { obj, start, blend, .. } => self.inner.call(&Call::PlayMotionAt { obj: *obj, motion: slot, start: *start, blend: *blend }),
                _ => Value::None,
            };
        }
        // 1.15 motion_state (0x170450): the word +0x154, the motion number FUN_0017e000 was asked for
        // (Goofy's `tornado` compares it with 191), not the slot the remap played.
        if let Call::MotionState { obj } = c
            && *obj == me
        {
            let slot = self.inner.obj.slot;
            let id = match (self.fields.motion, self.fields.queued_motion) {
                (Some((id, s)), _) if slot == Some(s) => id,
                (_, Some((id, s))) if slot == Some(s) => id,
                _ => slot.map_or(-1, |s| s as i32),
            };
            let v = Value::Int(id);
            self.record(c, &v, St::Approx, "the motion number of the script's last request (or the queued one) while its slot plays, else the slot");
            return v;
        }
        // 1.276 (0x170a00): for a party object (+0x588 bit 4) FUN_001769e0 pays the pending MP byte
        // +0xaf4 (FUN_00186368 with minus it) and clears it. [inferred] The object named is the member
        // itself (its script runs only here).
        // 1.140 (0x173740 -> FUN_0018ed58): whether a target record is valid. The members' scripts ask it
        // of their own target (2.23 then 1.140, `kh2 bdx P_EX020` R1559); [inferred] the app answers the
        // member's target record for any record the script names.
        // 2.10 (0x18e2f0, FUN_001a2ba0): an attack object of the object with `atkp` id arg 1; 2.17
        // (0x18e6e0): the attack hits the object named (FUN_0018c5f8 / FUN_0018c6d0, then the object's
        // receive method +0x64). The members' scripts aim 2.17 at their own target record (2.23, Donald's
        // `cure` R1559 with attack 1525), which `party` resolves (`party_script_hits`). 2.13 (arm) and 2.22
        // (anchor) need nothing more here: the hit is delivered at once.
        if let Some(v) = self.specials(c) {
            return v;
        }
        let float = |w: u32| f32::from_bits(w);
        match words(c) {
            // 2.10 attack_create (0x18e2f0 -> FUN_001a2ba0(obj, owner, a1, a2, a3, -1)): `atkp` id a1, hit
            // group a2 (-1: a new one), sub key a3.
            Some((2, 10, a)) => {
                let arg = |k: usize| a.get(k).copied().unwrap_or(0);
                let v = self.new_attack(arg(1) as u16, arg(3), (arg(2) as i32 >= 0).then_some(arg(2)));
                self.record(c, &v, St::Impl, "");
                return v;
            }
            // 2.31 (0x18e488 -> FUN_001a2c58): a new attack object with the `atkp` id given and the first
            // one's owner, hit group (+0x20), sub key (+0x18) and anchor (+0x24); its size and growth are
            // its own (FUN_001a2ba0 clears them).
            Some((2, 31, a)) => {
                let src = self.fields.attacks.iter().find(|x| Some(x.handle) == a.first().copied()).map(|x| (x.kind, x.group));
                let v = match src {
                    Some((kind, group)) => {
                        let v = self.new_attack(a.get(1).copied().unwrap_or(0) as u16, kind, Some(group));
                        self.record(c, &v, St::Impl, "");
                        v
                    }
                    None => {
                        self.record(c, &Value::Int(0), St::Missing, "2.31 on a handle that is not an attack object of the member");
                        Value::Int(0)
                    }
                };
                return v;
            }
            // 2.11 attack_set_size (0x18e348 -> FUN_001a30e0): +0x100 = a, +0x104 = b when b > 0 else a.
            Some((2, 11, a)) if a.len() >= 3 => {
                let (r, h) = (float(a[1]), float(a[2]));
                if let Some(o) = self.attack(a[0]) {
                    o.radius = r;
                    o.height = if h > 0.0 { h } else { r };
                }
                self.record(c, &Value::None, St::Impl, "");
                return Value::None;
            }
            // 2.62 attack_set_shape2 (0x18e680 -> FUN_001a3480): +0xc8 = 0, +0xcc = a (the growth's frames),
            // +0xd0 = b (the radius +0x100 unless b > 0), +0xd4 = c (the height +0x104 unless c > 0).
            Some((2, 62, a)) if a.len() >= 4 => {
                let (d, r, h) = (float(a[1]), float(a[2]), float(a[3]));
                if let Some(o) = self.attack(a[0]) {
                    o.t = 0.0;
                    o.grow = d;
                    o.from = (if r > 0.0 { r } else { o.radius }, if h > 0.0 { h } else { o.height });
                }
                self.record(c, &Value::None, St::Impl, "");
                return Value::None;
            }
            // 2.13 attack_arm (0x18e3c0 -> FUN_0018dc98): +4 |= 0x800, the object is released.
            Some((2, 13, a)) if a.first().is_some_and(|h| self.fields.attacks.iter().any(|x| x.handle == *h)) => {
                if let Some(o) = self.attack(a[0]) {
                    o.released = true;
                }
                self.record(c, &Value::None, St::Impl, "");
                return Value::None;
            }
            // 2.22 attack_anchor (0x18e7c8): +0x24 = the object's +0x80 (its effect file): the hit spark comes
            // from the member's own file (`friend_ai` sends it with the hit).
            Some((2, 22, a)) if a.first().is_some_and(|h| self.fields.attacks.iter().any(|x| x.handle == *h)) => {
                self.record(c, &Value::None, St::Approx, "the anchor is always the member's own effect file");
                return Value::None;
            }
            // 2.14 attack_hit_anything (0x18e3d8 -> FUN_0018c890): +4 has 2 (a target), 8 / 0x10 (a wall) or
            // 0x200 (a clash); 2.29 (0x18e420): 8 or 0x10; 2.39 (0x18e518): 0x200. The members' attack
            // objects are tested against no other attack (no clash), so 0x200 stays clear.
            Some((2, i @ (14 | 29 | 39), a)) if a.first().is_some_and(|h| self.fields.attacks.iter().any(|x| x.handle == *h)) => {
                let o = self.fields.attacks.iter().find(|x| x.handle == a[0]).expect("attack");
                let v = Value::Bool(match i {
                    14 => o.hit || o.wall,
                    29 => o.wall,
                    _ => false,
                });
                if i == 39 {
                    self.record(c, &v, St::Approx, "the members' attack objects meet no other attack: no clash (FUN_0018d430) is ever set");
                } else {
                    self.record(c, &v, St::Impl, "");
                }
                return v;
            }
            Some((2, 17, a)) => {
                let handle = a.first().copied().unwrap_or(0);
                if let Some(id) = self.fields.attacks.iter().find(|x| x.handle == handle).map(|x| x.atkp as i16) {
                    self.fields.target_hits.push(id);
                }
                self.record(c, &Value::None, St::Approx, "the hit lands on the member's target record (+0x9f4), resolved by party");
                return Value::None;
            }
            _ => {}
        }
        if let Some((1, 140, _)) = words(c) {
            let v = Value::Bool(self.fields.target_valid);
            let tag = self.inner.tag;
            self.trace().record(tag, c, &v, St::Approx, "the member's own target record (+0x9f4) whatever record is named");
            return v;
        }
        if let Some((1, 276, _)) = words(c) {
            let cost = std::mem::take(&mut self.fields.pending_mp);
            self.fields.paid = Some(self.fields.paid.unwrap_or(0).saturating_add(cost));
            let tag = self.inner.tag;
            self.trace().record(tag, c, &Value::None, St::Impl, "");
            return Value::None;
        }
        if let Some((10, index, a)) = words(c)
            && let Some((v, st, why)) = self.group10(index, &a)
        {
            let tag = self.inner.tag;
            self.trace().record(tag, c, &v, st, why);
            return v;
        }
        self.inner.call(c)
    }

    fn read_mem(&mut self, addr: u32, out: &mut [u8]) -> bool {
        // The member's target record (host 2.23): laid out as the enemy host's (FUN_0018ed40 +
        // FUN_0018f490: object, position index, 0, -1, 0), its object the one the special's record names
        // (the enemy host's record names the player).
        let t = addr.wrapping_sub(crate::enemy_obj::TARGET) as usize;
        if t + out.len() <= 20 {
            let mut r = crate::enemy_host::target_record();
            let obj = match self.fields.special {
                Some(crate::party::SpecialOn::Me) => self.inner.obj.handle,
                Some(crate::party::SpecialOn::Other) => TARGET_OTHER,
                Some(crate::party::SpecialOn::Enemy(_)) => TARGET_ENEMY,
                Some(crate::party::SpecialOn::Player) | None => crate::enemy_obj::PLAYER,
            };
            r[0..4].copy_from_slice(&obj.to_le_bytes());
            out.copy_from_slice(&r[t..t + out.len()]);
            return true;
        }
        self.inner.read_mem(addr, out)
    }

    fn write_mem(&mut self, addr: u32, data: &[u8]) -> bool {
        // The member's target record: only a target search (6.5) writes it; `target_found` carries what it
        // names, and `read_mem` keeps answering the record from the member's state.
        let t = addr.wrapping_sub(crate::enemy_obj::TARGET) as usize;
        if t + data.len() <= 20 {
            return true;
        }
        self.inner.write_mem(addr, data)
    }

    /// Host 6.5's objects (FUN_001b7c88's walk with FUN_002f0218 / FUN_002f03f8): for kind 0 (+0x588 bit
    /// 0x10, the enemy class FUN_00184820 sets) the hittable enemies' TARGET shapes; kinds 1 / 2 (+0x588 bit
    /// 4, the player and the members) are not listed.
    fn target_parts(&mut self, kind: i32, skip_flagged: bool) -> Option<Vec<kh2_bdx::search::TargetPart>> {
        let parts: Vec<kh2_bdx::search::TargetPart> = if matches!(kind, 1 | 2) {
            Vec::new()
        } else {
            self.targets.iter().map(|p| kh2_bdx::search::TargetPart { obj: enemy_handle(p.entity), part: p.part, point: crate::enemy_obj::to_game(p.point) }).collect()
        };
        let c = Call::Unknown { group: 6, index: 5, args: vec![kind as u32, skip_flagged as u32] };
        let v = Value::Int(parts.len() as i32);
        let why = match kind {
            1 | 2 => "kinds 1 / 2 (the player and the members, +0x588 bit 4) are not listed: nothing found",
            _ => "the hittable enemies' TARGET shapes in the app's order (FUN_0019a5d0's +0x7f8 bit 0x400 / +0x108 bit 0x20 read as `hittable`; the collision list's enable mask and +0x108 bit 0x8000 (10.55) not modelled)",
        };
        self.record(&c, &v, St::Approx, why);
        Some(parts)
    }

    fn target_found(&mut self, record: u32, found: Option<(u32, u8)>, walked: u8) {
        if record != crate::enemy_obj::TARGET {
            return;
        }
        let hit = found.and_then(|(o, part)| self.targets.iter().find(|p| enemy_handle(p.entity) == o && p.part == part));
        // The record names the part from now on: the member's target (1.140 / 1.120 / 6.9) follows at once.
        self.fields.target_valid = hit.is_some();
        self.fields.target_point = hit.map(|p| p.point);
        self.fields.target_radius = hit.map_or(0.0, |p| p.radius);
        self.fields.searched = Some((hit.map(|p| (p.entity, p.shape)), walked));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Host 0.46 (FUN_0017c0f8): the first vector, made unit, turned toward the second by at most the
    /// rate (the rotation FUN_0017bf88 builds about `a x b`), w 0; a smaller angle than the rate lands on
    /// the second. Goofy's `tornado` starts from (0, -1, 0) (up in the game's axes) and turns 10 degrees a
    /// frame toward its goal.
    #[test]
    fn member_turn_vec_follows_fun_0017c0f8() {
        let rate = 10f32.to_radians();
        let v = turn_vec([0.0, -1.0, 0.0, 1.0], [1.0, 0.0, 0.0, 0.0], rate);
        assert!((v[0] - rate.sin()).abs() < 1e-6 && (v[1] + rate.cos()).abs() < 1e-6 && v[2].abs() < 1e-6 && v[3] == 0.0, "{v:?}");
        let v = turn_vec([2.0, 0.0, 0.0, 5.0], [1.0, 0.0, 0.1, 0.0], rate);
        let w = 0.1f32.atan();
        assert!((v[0] - w.cos()).abs() < 1e-5 && (v[2] - w.sin()).abs() < 1e-5, "{v:?}");
        // Parallel: no turn, the unit vector.
        assert_eq!(turn_vec([0.0, 0.0, 3.0, 1.0], [0.0, 0.0, 1.0, 0.0], rate), [0.0, 0.0, 1.0, 0.0]);
    }

    /// Host 0.45 (FUN_0016d098): the second vector's direction times the scalar added on all four lanes.
    #[test]
    fn member_advance_vec_follows_fun_0016d098() {
        assert_eq!(advance_vec([1.0, 2.0, 3.0, 1.0], [0.0, 0.0, 2.0, 0.0], 5.0), [1.0, 2.0, 8.0, 1.0]);
        assert_eq!(advance_vec([1.0, 2.0, 3.0, 1.0], [0.0, 0.0, 0.0, 0.0], 5.0), [1.0, 2.0, 3.0, 1.0]);
    }
}
