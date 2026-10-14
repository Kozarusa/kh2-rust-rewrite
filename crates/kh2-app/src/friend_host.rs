//! The party members' script host: the enemy host (`BevyHost`) plus what only a friend object answers
//! (docs/party.md section 5, host group 10 at table 0x371810) and the player-type motion remap of the
//! motion requests (FUN_0017e000 -> FUN_0017da30).
//!
//! Group 10 entries the members' scripts reach (`kh2 bdx P_EX020` / `P_EX030`: Donald 10.31; Goofy 10.14,
//! 10.31, 10.40, 10.55) are answered here; the rest of group 10 goes to the enemy host, which reports it
//! as missing in the `KH2_BDXLOG` table. Everything else is the enemy host's.

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
    /// The effects the script started this tick (hosts 1.22, 1.87, 1.149); `friend_ai` starts them.
    pub fx: Vec<FxReq>,
    pub effects_made: u32,
}

/// An effect element of the member's own effect file (`obj/<model>.apdx`, the object's +0x80) the
/// script asked for.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum FxReq {
    /// 1.87 (FUN_0016a570 with owner 0 = the object) / 1.149 (FUN_0016a570 with another owner):
    /// `FUN_001db868(obj + 0x80, id, .., 1, .., owner)`: bound to the owner.
    Bound { id: u16, on_target: bool },
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
        self.fields.attacks.push(AttackObj { handle, atkp, kind, group, pos: None, prev: None, radius: 0.0, height: 0.0, grow: 0.0, t: 0.0, from: (0.0, 0.0), released: false });
        Value::Handle(handle)
    }

    fn new_effect(&mut self, req: FxReq) -> Value {
        self.fields.fx.push(req);
        self.fields.effects_made += 1;
        Value::Handle(EFFECT_HANDLE + self.fields.effects_made)
    }

    /// The attack-object hosts of group 2 and the effect starts of group 1 the members' specials use
    /// (`kh2 bdx P_EX020` thunder R13c8, cure R1559). `None`: not one of them.
    fn specials(&mut self, c: &Call) -> Option<Value> {
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
            // 1.87 spawn_attached (0x171800 -> FUN_0016a570, owner = the object).
            Call::SpawnAttached { obj, id, .. } if *obj == me => {
                let v = self.new_effect(FxReq::Bound { id: (*id).max(0) as u16, on_target: false });
                self.record(c, &v, St::Approx, "element `id` of the member's effect file bound to the member; the flag and extra arguments are not modelled");
                Some(v)
            }
            // 1.149 spawn_effect_for (0x171840 -> FUN_0016a570 with the third argument's object as owner):
            // element `id` of the first object's file bound to the other object. Donald's `cure` binds
            // element 12 to the object of its target record (R15cb).
            Call::Func { group: 1, index: 149, args } if args.first().is_some_and(|a| raw(a) == me) => {
                let id = args.get(1).map_or(0, |a| a.int().max(0) as u16);
                let on_target = args.get(2).is_none_or(|a| raw(a) != me);
                let v = self.new_effect(FxReq::Bound { id, on_target });
                self.record(c, &v, St::Approx, "element `id` of the member's effect file bound to the owner named; an owner other than the member is taken to be the object of its target record [inferred]");
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
            // 10.55 (0x2f7540 -> FUN_002f0758): bit 16 of the word at the first argument + 4 = (second != 0).
            55 => {
                let at = arg(0).wrapping_add(4);
                let mut b = [0u8; 4];
                if !self.inner.read_mem(at, &mut b) {
                    return Some((Value::None, St::Missing, "10.55 on a word outside the member's memory"));
                }
                let w = u32::from_le_bytes(b) & !0x1_0000 | u32::from(arg(1) != 0) << 16;
                self.inner.write_mem(at, &w.to_le_bytes());
                (Value::None, St::Impl, "")
            }
            _ => return None,
        })
    }
}

impl Host for FriendHost<'_, '_> {
    fn call(&mut self, c: &Call) -> Value {
        let me = self.inner.obj.handle;
        match c {
            Call::PlayMotion { obj, motion, blend } if *obj == me => {
                return self.inner.call(&Call::PlayMotion { obj: *obj, motion: self.remap(*motion), blend: *blend });
            }
            Call::PlayMotionChecked { obj, motion, blend } if *obj == me => {
                return self.inner.call(&Call::PlayMotionChecked { obj: *obj, motion: self.remap(*motion), blend: *blend });
            }
            Call::PlayMotionAt { obj, motion, start, blend } if *obj == me => {
                return self.inner.call(&Call::PlayMotionAt { obj: *obj, motion: self.remap(*motion), start: *start, blend: *blend });
            }
            _ => {}
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
        self.inner.read_mem(addr, out)
    }

    fn write_mem(&mut self, addr: u32, data: &[u8]) -> bool {
        self.inner.write_mem(addr, data)
    }
}
