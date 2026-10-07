//! The rules of Sora's magic, without Bevy: which `magc` row a spell uses, the command list, the
//! availability test, the MP cost and MP charge, the cast's "kind", the hit memory of a spell's
//! attack and the atkp row variants. Every rule names the function of the game's executable
//! (`SLUS_210.05`) it follows; `docs/magic.md` explains them. No game values are here: the rows
//! come from the user's disc.

use kh2_tables::tables::{Atkp, Magc};

/// Magic slots the game loads (FUN_001a45c0: six, the `magc` ids 0 Fire, 1 Blizzard, 2 Thunder,
/// 3 Cure, 4 Magnet, 5 Reflect).
pub const SLOTS: u8 = 6;
/// `cmd.Execute` of a magic command (FUN_00174cf8 sends it to FUN_00179400).
pub const EXECUTE_MAGIC: u16 = 2;
/// `cmd.Cost` 255: the command takes all MP (FUN_001a4bb8 lets it through with too little MP).
pub const COST_ALL: u8 = 255;

/// The `magc` row a magic slot uses (FUN_001a4c78): the first row of the slot's id is the base;
/// when the row has an item the slot needs that item (`owned` is its count, the spell's level;
/// 0 = not owned, no row); then the following rows of the same id whose level is not above the
/// count are walked and each one whose world is 0 or the current world replaces the pick.
pub fn pick_row(rows: &[Magc], id: u8, owned: u8, world: u8) -> Option<&Magc> {
    let first = rows.iter().position(|r| r.id == id)?;
    let level = if rows[first].item != 0 {
        if owned == 0 {
            return None;
        }
        owned
    } else {
        0
    };
    let mut pick = first;
    let mut i = first + 1;
    while i < rows.len() && rows[i].id == rows[first].id && rows[i].level <= level {
        if rows[i].world == 0 || rows[i].world == world {
            pick = i;
        }
        i += 1;
    }
    Some(&rows[pick])
}

/// The magic sub-menu list (FUN_001bbee8, the player's list at +0x9d4): for slots 0 to 5 in order,
/// the command of the slot's loaded row when there is one.
pub fn magic_list(rows: &[Magc], levels: &[u8; SLOTS as usize], world: u8) -> Vec<u16> {
    (0..SLOTS).filter_map(|id| pick_row(rows, id, levels[id as usize], world).map(|r| r.command).filter(|&c| c != 0)).collect()
}

/// A command's availability as FUN_001ba2c0 returns it for `Execute` 2: 0 usable, 2 usable with
/// too little MP (the spell takes what is left and starts the charge), 5 not usable. Callers treat
/// anything below 4 as usable (FUN_00178a78).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    Ready = 0,
    ShortMp = 2,
    Unavailable = 5,
}

impl Status {
    pub fn usable(self) -> bool {
        (self as u8) < 4
    }
}

/// FUN_001ba2c0 case 2: unavailable while MP charges (FUN_001a05d0: charge timer above 0); with
/// MP below the cost, a command that costs all MP (255) and any spell whose toggle bit at
/// 0x340eac is clear (FUN_001a4bb8; the bit is set and cleared only by the pause menu code
/// FUN_00274e50, so it is clear in a new game) give `ShortMp`, a set bit `Unavailable`.
/// FUN_001b9f70 (a special-battle flag) and the command locks of FUN_001b70b8 are not modelled;
/// `disabled` is the `DisableForm` test (FUN_001b7290) the caller makes.
pub fn availability(cost: u8, mp: i32, charging: bool, toggle_bit: bool, disabled: bool) -> Status {
    let s = if charging {
        Status::Unavailable
    } else if mp < cost as i32 {
        if cost == COST_ALL || !toggle_bit { Status::ShortMp } else { Status::Unavailable }
    } else {
        Status::Ready
    };
    if disabled { Status::Unavailable } else { s }
}

/// `DisableForm` (FUN_001b7290): bit `form` of the mask of the command whose id is the command's
/// `Execute` (for every magic command, the generic Magic command 2).
pub fn form_disables(mask: u16, form: u8) -> bool {
    form < 16 && mask & (1 << form) != 0
}

/// The cast's kind (FUN_001a4798): `2 * level - 2`, plus one for a ground finisher whose row has a
/// finisher motion, plus 10, 20 or 30 in Wisdom, Master or Final form (player only). It is the
/// `atkp` sub-row key of the spell's attacks and the `magi` row of its sizes.
pub fn cast_kind(level: u8, finisher_motion: bool, form: u8) -> u32 {
    let mut k = (level as u32 * 2).saturating_sub(2) + finisher_motion as u32;
    k += match form {
        2 => 10,
        3 => 20,
        4 => 30,
        _ => 0,
    };
    k
}

/// The kind a spell's script keeps after its event 20 (the `.mag` scripts, `kind / 2 * 2`): Fire,
/// Blizzard and Thunder clear the finisher bit in Master form (kind / 10 = 2), Reflect always; Cure
/// and Magnet keep it. The script passes this kind to `attack_create` and branches on it.
pub fn script_kind(slot: u8, kind: u32) -> u32 {
    match slot {
        0..=2 if kind / 10 == 2 => kind & !1,
        5 => kind & !1,
        _ => kind,
    }
}

/// Whether the cast is a finisher (FUN_00176908 with 0 extra): the combo counter (+0xb04, already
/// advanced by this press, FUN_00174cf8 calls FUN_00176b00 first) has reached the ground or air
/// combo length of the ability block (+0x28, +0x29). FUN_001768b8 and flag 0x20 of +0x168 (both
/// not identified) are not modelled; form 6 never finishes (FUN_00179400).
pub fn is_finisher(hits: u8, airborne: bool, max_ground: u8, max_air: u8) -> bool {
    hits >= if airborne { max_air } else { max_ground }
}

/// Which motion and blend the cast plays (FUN_001a4798): the air motion when airborne; on the ground
/// the finisher motion when it is a finisher and the row has one (`FinishMotion` is not -1), else
/// the ground motion. Returns (motion id, blend ticks, finisher motion used).
pub fn cast_motion(row: &Magc, airborne: bool, finisher: bool) -> (u16, f32, bool) {
    if airborne {
        (row.air_motion as u16, row.air_blend as f32, false)
    } else if finisher && (row.finish_motion as i16) >= 0 {
        (row.finish_motion, row.finish_blend as f32, true)
    } else {
        (row.ground_motion as u16, row.ground_blend as f32, false)
    }
}

/// The voice the cast asks for (FUN_001a4798: `Voice`, `VoiceFinisher` with the finisher motion,
/// `VoiceSelf` when the command's target is the caster); -1 is none.
pub fn cast_voice(row: &Magc, finisher_motion: bool, on_self: bool) -> i8 {
    if on_self {
        row.voice_self
    } else if finisher_motion {
        row.voice_finisher
    } else {
        row.voice
    }
}

/// Paying the spell (FUN_001769e0 -> FUN_00186368 with minus the cost byte, at the cast trigger):
/// when MP would go below zero the charge starts (FUN_00186440 -> FUN_001a0530: MP 0, charge left
/// and total = the player's `prty` MpDrive times 1.0); otherwise MP drops by the cost
/// (FUN_001a0180, clamped to 0..max). Paying exactly the MP left leaves 0 without a charge.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Paid {
    Mp(i32),
    Charge { total: f32 },
}

pub fn pay(mp: i32, max: i32, cost: u8, mp_drive: f32) -> Paid {
    let after = mp - cost as i32;
    if after < 0 { Paid::Charge { total: mp_drive * MP_CHARGE_SCALE } } else { Paid::Mp(after.min(max)) }
}

/// `DAT_0037a974`, the factor FUN_00186368 passes to the charge start (1.0 on the disc's executable).
pub const MP_CHARGE_SCALE: f32 = 1.0;

/// The MP charge (player status +0x1bc left, +0x1c0 total).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Charge {
    pub left: f32,
    pub total: f32,
}

impl Charge {
    pub fn active(&self) -> bool {
        self.total > 0.0
    }

    /// One frame (FUN_00175f48 -> FUN_001a0540 with minus the frame step times the ability block's
    /// MP regeneration multiplier +0x34): the time left drops, never below 0 nor above the total;
    /// at 0 the charge ends (FUN_001a05a8) and the caller refills MP. True when it ended.
    pub fn tick(&mut self, regen: f32) -> bool {
        if !self.active() {
            return false;
        }
        let v = self.left - 1.0 * regen;
        self.left = if v >= 0.0 { v.min(self.total) } else { 0.0 };
        if self.left == 0.0 {
            *self = Charge::default();
            return true;
        }
        false
    }
}

/// One spell attack's record in a victim's hit memory (the victim's block at +0xa50; FUN_001be108
/// tests, FUN_001be190 marks, FUN_001be300 counts down): the attack's group bit is set on a hit
/// and, when the row used has an `Interval`, cleared again after that many frames (four timer
/// slots). Consecutive hits by the same attack count up and pick the row variant.
#[derive(Clone, Debug, Default)]
pub struct HitMemory {
    /// Groups whose bit is set.
    pub groups: Vec<u32>,
    /// Up to four (group, frames left).
    pub slots: [(u32, f32); 4],
    /// The attack that hit last (+0x24) and how many times in a row (+0x28).
    pub last: Option<u32>,
    pub count: u32,
}

impl HitMemory {
    /// FUN_001be108: rows of type 5 and 6 always hit; otherwise not while the group bit is set.
    /// (The player-side invulnerability timer +0x2c is not this module's.)
    pub fn may_hit(&self, row_type: u8, group: u32) -> bool {
        matches!(row_type, 5 | 6) || !self.groups.contains(&group)
    }

    /// FUN_001be190: counts the hit (same attack as last: +1, else 0), returns the count for the
    /// variant lookup; `mark` then records the group and the interval of the row that was used.
    pub fn count_hit(&mut self, attack: u32) -> u32 {
        if self.last == Some(attack) {
            self.count += 1;
        } else {
            self.last = Some(attack);
            self.count = 0;
        }
        self.count
    }

    pub fn mark(&mut self, group: u32, interval: u16) {
        if interval != 0
            && let Some(s) = self.slots.iter_mut().find(|s| s.0 == 0)
        {
            *s = (group, interval as f32);
        }
        if !self.groups.contains(&group) {
            self.groups.push(group);
        }
    }

    /// FUN_001be300: every frame the slots count down; at 0 the group's bit is cleared.
    pub fn tick(&mut self) {
        for s in self.slots.iter_mut() {
            if s.0 != 0 {
                s.1 -= 1.0;
                if s.1 <= 0.0 {
                    let g = s.0;
                    self.groups.retain(|&x| x != g);
                    *s = (0, 0.0);
                }
            }
        }
    }
}

/// The index of the `atkp` row an attack object uses (FUN_001a2ba0 -> FUN_0018bd08: key `id`, sub
/// key `kind`, the high byte of `SubId`): the first row with that id and high byte. [inferred: the
/// constructor's lookup itself was not read; the rows of id 502 are grouped by that byte, 0 and 1
/// for a level-1 normal and finisher cast.]
pub fn attack_row(rows: &[Atkp], id: u16, kind: u32) -> Option<usize> {
    rows.iter().position(|r| r.id == id && (r.sub_id >> 8) as u32 == kind)
}

/// FUN_0019e7e0: the row variant for the `n`-th consecutive hit. Rows that follow the base row and
/// share its id and `SubId` high byte, whose low byte is at most `n`, replace it when their
/// `Switch` is 0 or `((n - low) & 1) + 1` equals it.
pub fn variant(rows: &[Atkp], base: usize, n: u32) -> usize {
    let b = &rows[base];
    let mut pick = base;
    let mut j = base + 1;
    while let Some(r) = rows.get(j) {
        if !(r.id == b.id && r.sub_id >> 8 == b.sub_id >> 8 && (r.sub_id & 0xff) as u32 <= n) {
            break;
        }
        let low = (r.sub_id & 0xff) as u32;
        if r.switch == 0 || ((n.wrapping_sub(low)) & 1) + 1 == r.switch as u32 {
            pick = j;
        }
        j += 1;
    }
    pick
}

/// One row of the `magi` pref entry of `03system.bin` (FUN_001cc1f0: row `kind`, row 0 when there is
/// none), read as floats by byte offset.
#[derive(Clone, Debug, Default)]
pub struct MagiRow(pub Vec<f32>);

impl MagiRow {
    pub fn f(&self, off: usize) -> f32 {
        self.0.get(off / 4).copied().unwrap_or(0.0)
    }
}

/// FUN_001cc140: `count` then `count` byte offsets from the start of the entry; a row index past
/// the count or with offset 0 falls back to row 0. Each row is read up to the next offset (or the end).
pub fn parse_magi(blob: &[u8]) -> Vec<MagiRow> {
    let u32_at = |o: usize| blob.get(o..o + 4).map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]) as usize);
    let Some(n) = u32_at(0) else { return Vec::new() };
    let offs: Vec<usize> = (0..n).filter_map(|i| u32_at(4 + 4 * i)).collect();
    let mut sorted: Vec<usize> = offs.iter().copied().filter(|&o| o != 0).collect();
    sorted.sort_unstable();
    offs.iter()
        .map(|&o| {
            if o == 0 {
                return MagiRow::default();
            }
            let end = sorted.iter().copied().find(|&x| x > o).unwrap_or(blob.len()).min(blob.len());
            MagiRow(blob.get(o..end).unwrap_or(&[]).as_chunks::<4>().0.iter().map(|c| f32::from_le_bytes(*c)).collect())
        })
        .collect()
}

pub fn magi_row(rows: &[MagiRow], kind: u32) -> Option<&MagiRow> {
    rows.get(kind as usize).filter(|r| !r.0.is_empty()).or_else(|| rows.first())
}

/// The heal of a Cure hit (FUN_001a0610 for an `atkp` row of type 6, FUN_00185a68 adds it for types 5
/// and 6): `(stat * Power + 99) / 100`, the stat being Strength (stat block +0x188) when `CriticalAdjust`
/// is 0, Magic (+0x18a) when it is 1 and 0 for any other value (the function loads nothing else).
/// Type 5 (a percentage of the target's maximum, `(max * stat * Power + 9999) / 10000`) is not used
/// by the spell rows and is not ported: it answers 0 here.
pub fn heal_amount(row: &Atkp, strength: i32, magic: i32) -> i32 {
    let stat = match row.critical_adjust {
        0 => strength,
        1 => magic,
        _ => 0,
    };
    match row.r#type {
        6 => (stat * row.power as i32 + 99) / 100,
        _ => 0,
    }
}

/// The HP after a heal (FUN_00186238 -> FUN_001a0100: the amount is added and the result clamped to
/// the maximum; a KO'd object (`hp` 0 here) takes no heal through this path).
pub fn healed(hp: i32, max: i32, heal: i32) -> i32 {
    if hp <= 0 { hp } else { (hp + heal).min(max) }
}

/// Ability bit tested by the cast start before FUN_00179470 (FUN_001a4798: ability block +0xc &
/// 0x80000, word 3 bit 19). The disc's ability row that owns it is item 403 (`magic_lock_on_bit_owner`
/// test).
pub const BIT_MAGIC_LOCK_ON: u16 = 3 * 32 + 19;

/// Whether FUN_001a4798 calls FUN_00179470 for a slot: with the Magic Lock-On bit, for Fire,
/// Blizzard, Thunder and Magnet (`magc` id below 3 or equal to 4).
pub fn magic_lock_on_applies(has_bit: bool, slot: u8) -> bool {
    has_bit && (slot < 3 || slot == 4)
}

/// FUN_0018fb98 as FUN_00179470 calls it (with a range of 0.0, which lets every target through: the
/// first test `range <= 0.0` holds, so no distance or height window applies): among the candidate
/// target points (each target shape of each targetable object, game units), the one with the lowest
/// `(d + |dy| * h_adjust) * ((1 - facing . dir) * 0.5 * (1 - inner) + inner)`, `d` the horizontal
/// distance from `pos` (the object's position, FUN_0016b960), `dir` the horizontal unit direction,
/// `dy` the height difference (FUN_00300188 [inferred: an absolute value]), `h_adjust` and `inner`
/// the player's row at +0x6c and +0x70 (`plyr` AutoLockOnHAdjust and AutoLockOnInnerAdjust). The
/// first of equal scores wins (`score < best`).
pub fn lock_on_pick<T: Copy>(pos: bevy::math::Vec3, facing: bevy::math::Vec3, h_adjust: f32, inner: f32, candidates: &[(T, bevy::math::Vec3)]) -> Option<T> {
    let mut best: Option<(T, f32)> = None;
    for &(t, at) in candidates {
        let d = at - pos;
        let flat = d.with_y(0.0);
        let dist = flat.length();
        let dir = if dist > 0.0 { flat / dist } else { flat };
        let score = (dist + d.y.abs() * h_adjust) * ((1.0 - facing.dot(dir)) * 0.5 * (1.0 - inner) + inner);
        if best.is_none_or(|(_, s)| score < s) {
            best = Some((t, score));
        }
    }
    best.map(|b| b.0)
}

/// Range triggers of a cast motion that turn the caster (FUN_00188030): 22 toward the command's target
/// record (+0x9f4, FUN_00186118), 28 toward the lock-on controller's target (+0xb40 + 4,
/// FUN_001861b8). Both only for an object with flag 2 of +0x588 (the player); the parameter is the
/// turn limit in degrees per frame (times `DAT_0037a988`, pi / 180).
pub const TRIGGER_TURN_TARGET: u8 = 22;
pub const TRIGGER_TURN_LOCK: u8 = 28;
/// FUN_00186018 turns only when the target is more than this far away horizontally (DAT_0037a970).
pub const TURN_MIN_DISTANCE: f32 = 50.0;

/// FUN_00186018 -> FUN_00183848 for one frame: the facing `yaw` turns toward the horizontal direction
/// from `pos` to the target point `at` (game units) by at most `deg_per_frame` degrees (times the frame
/// step, 1.0 per tick in the app), snapping once within that; nothing happens when the target is the
/// caster itself (`own`) or not more than `TURN_MIN_DISTANCE` away. Returns the new yaw. The yaw is
/// `atan2(x, z)` of the facing, as `Motor::forward`.
pub fn cast_turn(yaw: f32, pos: bevy::math::Vec3, at: bevy::math::Vec3, own: bool, deg_per_frame: f32) -> f32 {
    let d = (at - pos).with_y(0.0);
    if own || d.length() <= TURN_MIN_DISTANCE {
        return yaw;
    }
    let rate = deg_per_frame * std::f32::consts::PI / 180.0;
    let mut m = crate::motor::Motor { yaw, ..Default::default() };
    m.turn_to(d, rate);
    m.yaw
}

#[cfg(test)]
mod tests {
    use super::*;

    fn magc(id: u8, level: u8, world: u8, cmd: u16) -> Magc {
        Magc {
            id,
            level,
            world,
            padding: 0,
            file_name: String::new(),
            item: 21,
            command: cmd,
            ground_motion: 56,
            ground_blend: 8,
            finish_motion: 57,
            finish_blend: 8,
            air_motion: 58,
            air_blend: 8,
            voice: 7,
            voice_finisher: 11,
            voice_self: -1,
            padding2: 0,
        }
    }

    #[test]
    fn row_walk_follows_fun_001a4c78() {
        let rows = vec![magc(0, 1, 0, 49), magc(0, 1, 10, 49), magc(0, 2, 0, 119), magc(0, 2, 10, 119), magc(0, 3, 0, 120)];
        assert!(pick_row(&rows, 0, 0, 2).is_none(), "not owned");
        assert_eq!(pick_row(&rows, 0, 1, 2).map(|r| r.command), Some(49));
        assert_eq!(pick_row(&rows, 0, 1, 10).map(|r| (r.command, r.world)), Some((49, 10)));
        assert_eq!(pick_row(&rows, 0, 2, 2).map(|r| (r.command, r.world)), Some((119, 0)));
        assert_eq!(pick_row(&rows, 0, 9, 2).map(|r| r.command), Some(120));
        assert!(pick_row(&rows, 3, 1, 2).is_none(), "no row");
    }

    #[test]
    fn availability_follows_fun_001ba2c0() {
        assert_eq!(availability(10, 50, false, false, false), Status::Ready);
        assert_eq!(availability(10, 10, false, false, false), Status::Ready);
        assert_eq!(availability(10, 5, false, false, false), Status::ShortMp);
        assert_eq!(availability(10, 5, false, true, false), Status::Unavailable);
        assert_eq!(availability(COST_ALL, 99, false, true, false), Status::ShortMp);
        assert_eq!(availability(10, 50, true, false, false), Status::Unavailable);
        assert_eq!(availability(0, 0, true, false, false), Status::Unavailable, "the Magic command itself while charging");
        assert_eq!(availability(10, 50, false, false, true), Status::Unavailable);
        assert!(Status::ShortMp.usable() && !Status::Unavailable.usable());
    }

    #[test]
    fn paying_and_charging() {
        assert_eq!(pay(50, 100, 10, 3000.0), Paid::Mp(40));
        assert_eq!(pay(10, 100, 10, 3000.0), Paid::Mp(0), "exactly the MP left: no charge");
        assert_eq!(pay(5, 100, 10, 3000.0), Paid::Charge { total: 3000.0 });
        assert_eq!(pay(100, 100, COST_ALL, 3000.0), Paid::Charge { total: 3000.0 }, "Cure takes all MP");
        let mut c = Charge { left: 3.0, total: 3.0 };
        assert!(!c.tick(1.0));
        assert!(!c.tick(1.0));
        assert!(c.tick(1.0));
        assert!(!c.active());
        let mut c = Charge { left: 3.0, total: 3.0 };
        assert!(c.tick(4.0), "a regeneration step past the end ends it");
    }

    #[test]
    fn kinds_and_finishers() {
        assert_eq!(cast_kind(1, false, 0), 0);
        assert_eq!(cast_kind(1, true, 0), 1);
        assert_eq!(cast_kind(2, false, 0), 2);
        assert_eq!(cast_kind(1, false, 2), 10);
        assert_eq!(cast_kind(3, true, 4), 35);
        assert!(is_finisher(3, false, 3, 3));
        assert!(!is_finisher(2, false, 3, 3));
        let row = magc(0, 1, 0, 49);
        assert_eq!(cast_motion(&row, false, false), (56, 8.0, false));
        assert_eq!(cast_motion(&row, false, true), (57, 8.0, true));
        assert_eq!(cast_motion(&row, true, true), (58, 8.0, false));
        let cure = Magc { finish_motion: 65535, ..magc(3, 1, 0, 52) };
        assert_eq!(cast_motion(&cure, false, true), (56, 8.0, false), "no finisher motion: the ground motion");
    }

    #[test]
    fn hit_memory_rehits_after_the_interval() {
        let mut m = HitMemory::default();
        assert!(m.may_hit(1, 7));
        assert_eq!(m.count_hit(7), 0);
        m.mark(7, 3);
        assert!(!m.may_hit(1, 7));
        assert!(m.may_hit(6, 7), "heals always hit");
        m.tick();
        m.tick();
        assert!(!m.may_hit(1, 7));
        m.tick();
        assert!(m.may_hit(1, 7));
        assert_eq!(m.count_hit(7), 1);
        m.mark(7, 0);
        for _ in 0..100 {
            m.tick();
        }
        assert!(!m.may_hit(1, 7), "a row without an interval keeps the bit");
    }

    #[test]
    fn magi_rows_parse() {
        let mut b = Vec::new();
        b.extend(2u32.to_le_bytes());
        b.extend(12u32.to_le_bytes());
        b.extend(20u32.to_le_bytes());
        for f in [1.0f32, 2.0, 3.0, 4.0] {
            b.extend(f.to_le_bytes());
        }
        let rows = parse_magi(&b);
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].0, vec![1.0, 2.0]);
        assert_eq!(rows[1].f(4), 4.0);
        assert_eq!(magi_row(&rows, 9).map(|r| r.f(0)), Some(1.0), "past the count: row 0");
    }

    /// FUN_00179470's pick (FUN_0018fb98 with range 0): no distance limit, the facing weighs in by
    /// `inner`, the height by `h_adjust`.
    #[test]
    fn lock_on_pick_has_no_range_limit() {
        use bevy::math::Vec3;
        let pos = Vec3::ZERO;
        let facing = Vec3::Z;
        // Far ahead beats near behind when the facing weighs heavily (inner 0.1).
        let c = [(1, Vec3::new(0.0, 0.0, -300.0)), (2, Vec3::new(0.0, 0.0, 2000.0))];
        assert_eq!(lock_on_pick(pos, facing, 1.0, 0.1, &c), Some(2));
        // Inner 1.0: distance alone.
        assert_eq!(lock_on_pick(pos, facing, 1.0, 1.0, &c), Some(1));
        // The height difference adds |dy| * h_adjust to the distance.
        let h = [(1, Vec3::new(0.0, 500.0, 400.0)), (2, Vec3::new(0.0, 0.0, 600.0))];
        assert_eq!(lock_on_pick(pos, facing, 1.0, 1.0, &h), Some(2));
        assert_eq!(lock_on_pick(pos, facing, 0.0, 1.0, &h), Some(1));
        // Equal scores: the first.
        let e = [(1, Vec3::new(100.0, 0.0, 0.0)), (2, Vec3::new(-100.0, 0.0, 0.0))];
        assert_eq!(lock_on_pick(pos, facing, 1.0, 0.5, &e), Some(1));
        assert_eq!(lock_on_pick::<u8>(pos, facing, 1.0, 0.5, &[]), None);
    }

    /// FUN_00186018 -> FUN_00183848: at most the trigger's degrees per frame, a snap within that, no
    /// turn within 50 units or toward the caster itself.
    #[test]
    fn cast_turn_rule() {
        use bevy::math::Vec3;
        let step = 15f32.to_radians();
        // Facing +z (yaw 0), target to the right (+x): a quarter turn takes six frames of 15 degrees.
        let mut yaw = 0.0;
        let at = Vec3::new(1000.0, 0.0, 0.0);
        for k in 1..=5 {
            yaw = cast_turn(yaw, Vec3::ZERO, at, false, 15.0);
            assert!((yaw - step * k as f32).abs() < 1e-5, "frame {k}: {yaw}");
        }
        yaw = cast_turn(yaw, Vec3::ZERO, at, false, 15.0);
        assert!((yaw - std::f32::consts::FRAC_PI_2).abs() < 1e-5);
        // Within one step: snaps.
        assert!((cast_turn(1.5, Vec3::ZERO, at, false, 15.0) - std::f32::consts::FRAC_PI_2).abs() < 1e-6);
        // Turning the short way (to the left).
        assert!((cast_turn(0.0, Vec3::ZERO, Vec3::new(-1000.0, 0.0, 10.0), false, 15.0) + step).abs() < 1e-5);
        // Too close (horizontal distance 50 or less, height ignored), or the caster's own record.
        assert_eq!(cast_turn(0.3, Vec3::ZERO, Vec3::new(50.0, 900.0, 0.0), false, 15.0), 0.3);
        assert_ne!(cast_turn(0.3, Vec3::ZERO, Vec3::new(51.0, 0.0, 0.0), false, 15.0), 0.3);
        assert_eq!(cast_turn(0.3, Vec3::ZERO, at, true, 15.0), 0.3);
    }

    #[test]
    fn magic_lock_on_slots() {
        for (slot, want) in [(0, true), (1, true), (2, true), (3, false), (4, true), (5, false)] {
            assert_eq!(magic_lock_on_applies(true, slot), want, "slot {slot}");
            assert!(!magic_lock_on_applies(false, slot));
        }
    }
}
