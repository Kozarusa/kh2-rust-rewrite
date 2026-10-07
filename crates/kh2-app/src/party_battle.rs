//! Pure rules of the party members in battle (stage 2 of docs/party.md section 9): the AI class's
//! action records and parameter entries as the executable holds them, the range test, the
//! feasibility test, the anti-repeat rule, the action choice and the style-0 target choice. `party.rs`
//! runs them; each function names the game routine it ports.
//!
//! Units and spaces as in `party_rules`: game units, the app's axes. Height differences handed to these
//! rules are the GAME's (`dy` = target Y minus own Y with the game's Y pointing down, so a target above
//! the member has a negative `dy`): the callers convert.
//!
//! Values. The records, the parameter blocks and the motion-state table are read from the executable
//! at run time (`ClassAi::from_exe`); the record tables' addresses and counts are instruction
//! immediates of the AI classes' constructors (named constants with the routine).

use crate::party_rules::{exe_bytes, AI_PARAMS};

/// The record tables of the AI classes stage 2 drives: (class, table, count). Donald's constructor
/// FUN_00114550 binds 0x353090 with 0x14 records, Goofy's FUN_00114668 0x3533a0 with 0x12; the Pride
/// Lands classes 12 and 13 use the same constructors (FUN_0010b780). The world partners' constructors
/// (FUN_00114890 .. FUN_001154d0) were not read: their members have no actions here.
pub const CLASS_RECORDS: [(u8, u32, usize); 4] = [(0, 0x353090, 0x14), (12, 0x353090, 0x14), (1, 0x3533a0, 0x12), (13, 0x3533a0, 0x12)];
/// The members' special abilities: 29 rows `{ability bit, ability item, NeoStatus, index}` (FUN_001143b8,
/// FUN_001144d0).
pub const ABILITY_TABLE: u32 = 0x352ec0;
pub const ABILITY_ROWS: usize = 0x1d;
/// The seven consumable items in setting order (FUN_00114348).
pub const ITEM_ORDER: u32 = 0x352eb0;
pub const ITEM_SETTINGS: usize = 7;
/// FUN_0017da30's table of slot orders: four bytes per motion state (0 battle with weapon, 1 battle,
/// 2 field, 3 field with weapon).
pub const MOTION_ORDER: u32 = 0x37a5c8;
/// Candidates the choice keeps (FUN_0010fcd0 at the store: `slti 0x20`).
pub const MAX_CANDIDATES: usize = 0x20;

/// Action record flags (+0).
pub mod flag {
    /// Not counted as a repeat (FUN_0010fcd0 resets +0xce8 instead).
    pub const NOT_COUNTED: u32 = 1;
    /// An attack: needs a valid target in range; a combo follow-up may chain after it.
    pub const ATTACK: u32 = 2;
    /// Continuous: re-tested and kept while it holds (FUN_0010fcd0's first step).
    pub const CONTINUOUS: u32 = 0x200;
}

/// Parameter entry flags (+0xc).
pub mod entry_flag {
    /// No band test on the ground distance (FUN_0010f118).
    pub const ANY_DISTANCE: u32 = 1;
    /// No height test.
    pub const ANY_HEIGHT: u32 = 2;
    /// The height test only inside the band +0x2c..+0x30.
    pub const HEIGHT_IN_BAND: u32 = 8;
    /// No facing test (FUN_0010f220).
    pub const ANY_FACING: u32 = 0x10;
}

/// The handler of a record (the pointer-to-member at +4), by the routine's address.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Handler {
    /// FUN_001135e0: stand facing the target and wait a random +0x70..+0x74 frames.
    Approach,
    /// FUN_00113728: move toward the target (or follow) and wait a random +0x78..+0x7c frames.
    Wait,
    /// FUN_00114738: Goofy's wait: keeps running the way it started for a random +0x70..+0x74 frames.
    GoofyWait,
    /// FUN_001137d8: a melee or air attack with the record's parameter entry.
    Attack,
    /// FUN_00113888 / FUN_00113b08: HP / MP items (stage 4).
    HpItem,
    MpItem,
    /// FUN_00113d48 (Goofy FUN_00114828 calls it): switch the member's script to the record's mode.
    Special,
    /// FUN_00113e68: Cure, a special wanted when someone's HP is low (stage 4).
    Cure,
    Other(u32),
}

impl Handler {
    pub fn from_addr(a: u32) -> Handler {
        match a {
            0x1135e0 => Handler::Approach,
            0x113728 => Handler::Wait,
            0x114738 => Handler::GoofyWait,
            0x1137d8 => Handler::Attack,
            0x113888 => Handler::HpItem,
            0x113b08 => Handler::MpItem,
            0x113d48 | 0x114828 => Handler::Special,
            0x113e68 => Handler::Cure,
            a => Handler::Other(a),
        }
    }
}

/// A 0x80-byte action parameter entry of the class's block (bound to records by id, FUN_00113450).
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct Entry {
    /// +0: matched to the record's +0xc.
    pub id: u32,
    /// +4: the motion requested (FUN_00110790 -> FUN_0017e220).
    pub motion: i32,
    /// +8: the MP cost (kept at +0xc35 by FUN_0010c8f0).
    pub mp: i32,
    /// +0xc: `entry_flag` bits.
    pub flags: u32,
    /// +0x10 / +0x14: the ground-distance band (FUN_0010f118).
    pub near: f32,
    pub far: f32,
    /// +0x18 / +0x1c: the height limits (FUN_0010f118: `-(+0x1c) - r <= dy <= r - (+0x18)`).
    pub up: f32,
    pub down: f32,
    /// +0x20: the height an air attack jumps to (FUN_00110790; 0 = a ground attack).
    pub air: f32,
    /// +0x2c / +0x30: the band of the height test (flag 8).
    pub band_near: f32,
    pub band_far: f32,
    /// +0x34 / +0x38: closer than +0x34 the height difference must be at most +0x38.
    pub close: f32,
    pub close_dy: f32,
    /// +0x3c: the HP fraction under which Cure is wanted (FUN_00113e68).
    pub cure: f32,
}

/// A 0x24-byte action record (static part from the executable, +0x10/+0x18/+0x1c as FUN_00113450
/// binds them).
#[derive(Clone, Debug, PartialEq)]
pub struct Record {
    pub flags: u32,
    pub handler: Handler,
    pub id: u32,
    pub entry: Option<Entry>,
    /// +0x14: the ability bit; +0x18 the ability item (FUN_001143b8); +0x1c the setting index
    /// (FUN_00114348: an item's place among the seven consumables, else 7 + its ability row's index).
    pub ability: u32,
    pub item: u32,
    pub setting: i32,
    /// +0x20: the BDX mode the special switches the script to.
    pub mode: String,
}

/// The class's parameter block (0x34ea00 + offset[class]) beyond the follow thresholds.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Params {
    /// +0x60 MP-use chance factor, +0x64 combo-continue chance.
    pub mp_chance: f32,
    pub combo: f32,
    /// +0x68: style 0's target range (FUN_00114058); +0x6c: its re-target range (FUN_00114068).
    pub range: f32,
    pub retarget: f32,
    /// +0x70..+0x74 and +0x78..+0x7c: the random waits of the two idle actions.
    pub wait_a: (f32, f32),
    pub wait_b: (f32, f32),
    /// +0x80: the battle distance from the player (FUN_00114108).
    pub battle: f32,
    /// +0xa8: count of combo follow-ups (FUN_00113588).
    pub followups: u32,
}

/// A class's actions.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct ClassAi {
    pub params: Params,
    pub records: Vec<Record>,
}

fn rw(elf: &[u8], va: u32) -> Option<u32> {
    exe_bytes(elf, va, 4).map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
}

fn rf(elf: &[u8], va: u32) -> Option<f32> {
    rw(elf, va).map(f32::from_bits)
}

fn rstr(elf: &[u8], va: u32) -> String {
    let mut s = String::new();
    for k in 0..32 {
        match exe_bytes(elf, va + k, 1).map(|b| b[0]) {
            Some(0) | None => break,
            Some(c) => s.push(c as char),
        }
    }
    s
}

impl ClassAi {
    /// The block and, for the classes of `CLASS_RECORDS`, the records bound to their entries as
    /// FUN_00113450 does. A class without records gets none (the default class FUN_001133e0).
    pub fn from_exe(elf: &[u8], class: u8) -> Option<ClassAi> {
        let block = AI_PARAMS + rw(elf, AI_PARAMS + 4 * class as u32)?;
        let params = Params {
            mp_chance: rf(elf, block + 0x60)?,
            combo: rf(elf, block + 0x64)?,
            range: rf(elf, block + 0x68)?,
            retarget: rf(elf, block + 0x6c)?,
            wait_a: (rf(elf, block + 0x70)?, rf(elf, block + 0x74)?),
            wait_b: (rf(elf, block + 0x78)?, rf(elf, block + 0x7c)?),
            battle: rf(elf, block + 0x80)?,
            followups: rw(elf, block + 0xa8)?,
        };
        let (entries_n, entries_at) = (rw(elf, block + 0xa0)? as usize, block + rw(elf, block + 0xa4)?);
        let mut records = Vec::new();
        if let Some(&(_, table, n)) = CLASS_RECORDS.iter().find(|r| r.0 == class) {
            for i in 0..n as u32 {
                let r = table + i * 0x24;
                let ability = rw(elf, r + 0x14)?;
                let id = rw(elf, r + 0xc)?;
                let entry = (0..entries_n as u32).map(|k| entries_at + k * 0x80).find(|&e| rw(elf, e) == Some(id)).and_then(|e| {
                    Some(Entry {
                        id,
                        motion: rw(elf, e + 4)? as i32,
                        mp: rw(elf, e + 8)? as i32,
                        flags: rw(elf, e + 0xc)?,
                        near: rf(elf, e + 0x10)?,
                        far: rf(elf, e + 0x14)?,
                        up: rf(elf, e + 0x18)?,
                        down: rf(elf, e + 0x1c)?,
                        air: rf(elf, e + 0x20)?,
                        band_near: rf(elf, e + 0x2c)?,
                        band_far: rf(elf, e + 0x30)?,
                        close: rf(elf, e + 0x34)?,
                        close_dy: rf(elf, e + 0x38)?,
                        cure: rf(elf, e + 0x3c)?,
                    })
                });
                // FUN_00113450: +0x18 = FUN_001143b8(+0x14) when +0x14 is set; +0x1c = FUN_00114348(+0x18)
                // when +0x18 is set (else the static word).
                let mut item = rw(elf, r + 0x18)?;
                let mut setting = rw(elf, r + 0x1c)? as i32;
                if ability != 0 {
                    item = (0..ABILITY_ROWS as u32).find(|&k| rw(elf, ABILITY_TABLE + k * 16) == Some(ability)).and_then(|k| rw(elf, ABILITY_TABLE + k * 16 + 4)).unwrap_or(0);
                }
                if item != 0 {
                    let consumable = (0..ITEM_SETTINGS as u32).find(|&k| exe_bytes(elf, ITEM_ORDER + 2 * k, 2).map(|b| u16::from_le_bytes([b[0], b[1]]) as u32) == Some(item & 0xffff));
                    setting = match consumable {
                        Some(k) => k as i32,
                        None => 7 + (0..ABILITY_ROWS as u32).find(|&k| rw(elf, ABILITY_TABLE + k * 16 + 4) == Some(item)).and_then(|k| rw(elf, ABILITY_TABLE + k * 16 + 12)).map_or(-1, |v| v as i32),
                    };
                }
                let mode_ptr = rw(elf, r + 0x20)?;
                records.push(Record {
                    flags: rw(elf, r)?,
                    handler: Handler::from_addr(rw(elf, r + 4)?),
                    id,
                    entry,
                    ability,
                    item,
                    setting,
                    mode: if mode_ptr != 0 { rstr(elf, mode_ptr) } else { String::new() },
                });
            }
        }
        Some(ClassAi { params, records })
    }
}

/// FUN_0017da30's slot orders (the table at 0x37a5c8, 4 bytes per motion state).
pub fn motion_orders(elf: &[u8]) -> Option<[[u8; 4]; 4]> {
    let b = exe_bytes(elf, MOTION_ORDER, 16)?;
    Some(std::array::from_fn(|s| std::array::from_fn(|k| b[s * 4 + k])))
}

/// FUN_0017da30: the motion state's row: 1 when the object is in battle (FUN_0019a898), else 2;
/// flipped to 0 / 3 when the object carries a weapon object (+0x588 bit 4 and FUN_001752a0).
pub fn motion_state(battle: bool, weapon_flag: bool) -> usize {
    let s = if battle { 1 } else { 2 };
    if weapon_flag { s ^ 1 } else { s }
}

/// FUN_0017da30: the slot of `motion` in the state's order: the first `motion * 4 + order[k]` the
/// motion set has; `None` when none.
pub fn remap(motion: i32, order: [u8; 4], has: impl Fn(usize) -> bool) -> Option<usize> {
    let m = usize::try_from(motion).ok()?;
    order.into_iter().map(|o| m * 4 + o as usize).find(|&s| has(s))
}

/// FUN_0010f118: whether a target at ground distance `gd` and game height difference `dy` is in the
/// entry's reach, `r` widening every limit (the caller passes +0xbf0, [inferred] the target shape's
/// radius).
pub fn in_range(e: &Entry, r: f32, dy: f32, gd: f32) -> bool {
    if e.flags & entry_flag::ANY_DISTANCE == 0 && (e.far + r < gd || gd < e.near - r) {
        return false;
    }
    if e.flags & entry_flag::ANY_HEIGHT == 0 {
        let test = e.flags & entry_flag::HEIGHT_IN_BAND == 0 || (gd <= e.band_far + r && gd >= e.band_near - r);
        if test && (dy < -e.down - r || r - e.up < dy) {
            return false;
        }
    }
    gd >= e.close || dy.abs() <= e.close_dy
}

/// What FUN_0010f220 reads of the member.
#[derive(Clone, Copy, Debug, Default)]
pub struct MemberNow {
    /// The motion controller is in state 0 or 0x1f (or the caller does not ask), and the member is
    /// free (+0x160 bit 4).
    pub standing: bool,
    pub free: bool,
    /// The member has a status block (+0x48c), its MP (+0x180), the MP gate bit (+0xb7c 0x20) and an
    /// MP charge (FUN_001a05d0).
    pub status: bool,
    pub mp: i32,
    pub mp_gate: bool,
    pub mp_charge: bool,
    /// FUN_0010ca88: the target is valid (and +0xb7c bit 0x40 clear).
    pub target_ok: bool,
    /// The target relation (+0xbb0): game height difference, ground distance, and the dot of the
    /// member's facing with the ground direction to the target; `radius` = +0xbf0.
    pub dy: f32,
    pub gd: f32,
    pub facing_dot: f32,
    pub radius: f32,
}

/// FUN_0010f220 with both tests asked (as the attack handler does): the ability gate (needs a setting
/// index, a setting other than 3 and the ability equipped), the MP gate for an entry that costs MP,
/// and for an attack a valid target in range and in front (unless the entry says any facing).
/// `setting` is the member's setting byte of the record (+0xd4 + 1 + index); `equipped` FUN_0010f578.
pub fn feasible(rec: &Record, m: &MemberNow, setting: Option<u8>, equipped: bool) -> bool {
    if !m.standing || !m.free {
        return false;
    }
    if rec.ability != 0 && (rec.setting < 0 || setting == Some(3) || !equipped) {
        return false;
    }
    let Some(e) = rec.entry else { return rec.flags & flag::ATTACK == 0 };
    if m.status && e.mp >= 1 && (!m.mp_gate || m.mp_charge) {
        return false;
    }
    if rec.flags & flag::ATTACK == 0 {
        return true;
    }
    m.target_ok && in_range(&e, m.radius, m.dy, m.gd) && (e.flags & entry_flag::ANY_FACING != 0 || m.facing_dot >= 0.0)
}

/// FUN_0010fc68: the record chosen the last three times in a row (+0xc54, +0xc58, +0xc5c), which the
/// choice skips once; never for the summon classes (FUN_001124b8: class above 13).
pub fn repeated(last: [Option<usize>; 3], class: u8) -> Option<usize> {
    if class > 0xd {
        return None;
    }
    (last[0].is_some() && last[0] == last[1] && last[1] == last[2]).then_some(last[0]).flatten()
}

/// FUN_0010ed00: a chosen counted record goes to the front of the last three.
pub fn push_last(last: &mut [Option<usize>; 3], rec: usize) {
    last[2] = last[1];
    last[1] = last[0];
    last[0] = Some(rec);
}

/// What the member is doing between frames (+0xc60 the current record, +0xb7c bit 4 continuing,
/// +0xc54.. the last three, +0xce8 the repeat count).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Choosing {
    pub current: Option<usize>,
    pub continuing: bool,
    pub last: [Option<usize>; 3],
    pub repeats: i32,
}

/// The outcome of one FUN_0010fcd0 run.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Choice {
    /// The running continuous record holds: its phase 0 runs again.
    Keep(usize),
    /// A combo follow-up of the class (index into the block's list; FUN_0010c800, not ported).
    FollowUp(u32),
    /// A record was chosen: its phase 0 runs.
    Do(usize),
    /// Nothing feasible: move toward the target (or follow) at speed 0 (FUN_001103f8, +0xc0c = 0).
    Stand,
    /// The last choice was repeated twice: move toward the target (or follow) (FUN_001103f8).
    Move,
}

/// FUN_0010fcd0 after its emergency and busy gates (the caller's). `setting(i)` is the member's
/// setting byte of record `i` (`None` for setting index -1); `can(i, continuing)` the record's phase 1;
/// `chain` FUN_0010f3d8 (standing, free and a valid target); `roll()` FUN_0010f438 (0..1); `rand(n)`
/// FUN_0010f420. `summon_any` is +0xb7c bit 0x800.
#[allow(clippy::too_many_arguments)]
pub fn choose(
    recs: &[Record],
    st: &mut Choosing,
    params: &Params,
    class: u8,
    m: &MemberNow,
    summon_any: bool,
    chain: bool,
    setting: impl Fn(usize) -> Option<u8>,
    mut can: impl FnMut(usize, bool) -> bool,
    mut roll: impl FnMut() -> f32,
    mut rand: impl FnMut(u32) -> u32,
) -> Choice {
    if let Some(cur) = st.current {
        if recs[cur].flags & flag::CONTINUOUS != 0 {
            st.continuing = true;
            if can(cur, true) {
                return Choice::Keep(cur);
            }
        }
        if recs[cur].flags & flag::ATTACK != 0 && chain && roll() < params.combo && params.followups > 0 {
            let k = rand(params.followups);
            st.repeats = 0;
            st.current = None;
            return Choice::FollowUp(k);
        }
    }
    if !(summon_any || st.repeats < 2) {
        st.repeats = 0;
        st.current = None;
        return Choice::Move;
    }
    st.continuing = false;
    let skip = repeated(st.last, class);
    let mut skipped = false;
    let mut pass = 0;
    loop {
        let mut cands: Vec<usize> = Vec::new();
        for (i, r) in recs.iter().enumerate() {
            if matches!(r.handler, Handler::Other(0)) {
                continue;
            }
            let s = if r.setting != -1 { setting(i).unwrap_or(0) } else { 0 };
            let excluded = if pass == 0 {
                s == 2 || r.entry.is_some_and(|e| e.mp > 0 && ((m.status && m.mp_charge) || (s == 1 && m.status && m.mp < e.mp)))
            } else {
                s != 2
            };
            if !excluded && can(i, false) && cands.len() < MAX_CANDIDATES {
                cands.push(i);
            }
        }
        let mut pick = None;
        if !cands.is_empty() {
            let k = cands[rand(cands.len() as u32) as usize];
            if Some(k) == skip {
                skipped = true;
            } else {
                pick = Some(k);
            }
        }
        if let Some(k) = pick {
            if recs[k].flags & flag::NOT_COUNTED == 0 {
                push_last(&mut st.last, k);
                st.repeats += 1;
            } else {
                st.repeats = 0;
            }
            st.current = Some(k);
            return Choice::Do(k);
        }
        pass += 1;
        if !skipped || pass > 1 {
            st.repeats = 0;
            st.current = None;
            return Choice::Stand;
        }
    }
}

/// FUN_0010d0c0's scan (style 0, no remembered attacker): among the valid enemies' hurt shapes, the one
/// whose surface (distance to its centre minus its radius) is nearest the member, and nearer than the
/// class's range. `shapes` are (index, centre, radius); returns the index.
pub fn nearest_target(member: bevy::math::Vec3, range: f32, shapes: impl Iterator<Item = (usize, bevy::math::Vec3, f32)>) -> Option<usize> {
    let mut best = range;
    let mut out = None;
    for (i, c, r) in shapes {
        let d = (c - member).length() - r;
        if d < best {
            best = d;
            out = Some(i);
        }
    }
    out
}

/// FUN_0010cbe0 (style 0's re-target test): when an attacker is remembered and it is not the target,
/// or the target lies farther than the re-target range (squared distance of the target point).
pub fn retarget_style0(attacker_differs: bool, target_dist: f32, retarget: f32) -> bool {
    attacker_differs || target_dist * target_dist > retarget * retarget
}

/// FUN_0010e5e8's body (motion state 0) with the target relation (+0xbb0, via FUN_001103e0): the
/// direction is the ground direction to the target; the speed mode climbs or drops by the class's
/// battle-target row against the ground distance; the speed is the mode's (FUN_0010e5b8). A target
/// closer than `stop.0` on the ground and more than `-stop.1` above (game `dy` below `stop.1`) stops
/// the member (0x378564 / 0x378568). The turning bit is cleared (bit 0x10, not the player relation).
/// The ground probe ahead (FUN_0010e4b8) and the wall/jump test are not ported: the movement's own
/// collision stops the member.
#[allow(clippy::too_many_arguments)]
pub fn approach_step(st: &mut crate::party_rules::Follow, gdir: bevy::math::Vec3, gd: f32, dy: f32, t: &crate::party_rules::Thresholds, walk: f32, run: f32, stop: (f32, f32)) -> (bevy::math::Vec3, f32) {
    use crate::party_rules::{mode_speed, next_mode, ROW_BATTLE_TARGET};
    st.turning = false;
    st.mode = next_mode(st.mode, gd, &t[ROW_BATTLE_TARGET]);
    let mut speed = mode_speed(st.mode, walk, run);
    if gd < stop.0 && dy < stop.1 {
        speed = 0.0;
    }
    (gdir, speed)
}

/// FUN_0010df08's battle distance: the class's +0x80, doubled for styles 3 and 5, halved (0x3784ac)
/// for style 2.
pub fn battle_distance(base: f32, style: u8, half: f32) -> f32 {
    match style {
        3 | 5 => base + base,
        2 => base * half,
        _ => base,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry() -> Entry {
        Entry { near: 0.0, far: 150.0, up: 0.0, down: 100.0, close: 30.0, close_dy: 50.0, ..Entry::default() }
    }

    /// The band, the height limits and the close-range height cap of FUN_0010f118.
    #[test]
    fn party_attack_range_rules() {
        let e = entry();
        assert!(in_range(&e, 0.0, 0.0, 100.0));
        assert!(!in_range(&e, 0.0, 0.0, 160.0));
        assert!(in_range(&e, 20.0, 0.0, 160.0), "the radius widens the band");
        assert!(!in_range(&e, 0.0, -101.0, 100.0), "too far above (game dy negative)");
        assert!(!in_range(&e, 0.0, 1.0, 100.0), "below: r - up < dy");
        assert!(!in_range(&e, 0.0, -60.0, 10.0), "close and more than close_dy apart");
        let any = Entry { flags: entry_flag::ANY_DISTANCE | entry_flag::ANY_HEIGHT, ..e };
        assert!(in_range(&any, 0.0, -90.0, 1000.0));
    }

    fn rec(handler: Handler, flags: u32) -> Record {
        Record { flags, handler, id: 0, entry: None, ability: 0, item: 0, setting: -1, mode: String::new() }
    }

    /// The third same choice in a row is skipped once; the repeat count stops the choice after two.
    #[test]
    fn party_choice_skips_a_triple_repeat() {
        assert_eq!(repeated([Some(1), Some(1), Some(1)], 0), Some(1));
        assert_eq!(repeated([Some(1), Some(1), Some(2)], 0), None);
        assert_eq!(repeated([Some(1), Some(1), Some(1)], 14), None);
        let recs = vec![rec(Handler::Attack, flag::ATTACK)];
        let m = MemberNow::default();
        let p = Params::default();
        let mut st = Choosing { last: [Some(0); 3], ..Choosing::default() };
        // Only record 0 is feasible and it is the repeated one: skipped, the second pass has nothing.
        let c = choose(&recs, &mut st, &p, 0, &m, false, false, |_| Some(0), |_, _| true, || 1.0, |_| 0);
        assert_eq!(c, Choice::Stand);
        let mut st = Choosing::default();
        assert_eq!(choose(&recs, &mut st, &p, 0, &m, false, false, |_| Some(0), |_, _| true, || 1.0, |_| 0), Choice::Do(0));
        assert_eq!((st.repeats, st.last[0]), (1, Some(0)));
        st.repeats = 2;
        assert_eq!(choose(&recs, &mut st, &p, 0, &m, false, false, |_| Some(0), |_, _| true, || 1.0, |_| 0), Choice::Move);
    }

    /// A continuous record that still holds is kept; an attack may chain into a follow-up.
    #[test]
    fn party_choice_keeps_a_continuous_record() {
        let recs = vec![rec(Handler::Wait, flag::CONTINUOUS | flag::NOT_COUNTED), rec(Handler::Attack, flag::ATTACK)];
        let m = MemberNow::default();
        let p = Params { combo: 0.5, followups: 3, ..Params::default() };
        let mut st = Choosing { current: Some(0), ..Choosing::default() };
        assert_eq!(choose(&recs, &mut st, &p, 0, &m, false, false, |_| Some(0), |_, _| true, || 1.0, |_| 0), Choice::Keep(0));
        let mut st = Choosing { current: Some(1), ..Choosing::default() };
        assert_eq!(choose(&recs, &mut st, &p, 0, &m, false, true, |_| Some(0), |_, _| true, || 0.1, |_| 2), Choice::FollowUp(2));
        // Setting 2 keeps a record out of the first pass.
        let mut st = Choosing::default();
        let recs = vec![Record { setting: 0, ..rec(Handler::Attack, flag::ATTACK) }];
        assert_eq!(choose(&recs, &mut st, &p, 0, &m, false, false, |_| Some(2), |_, _| true, || 1.0, |_| 0), Choice::Stand);
    }

    /// FUN_0017da30: the first slot of the state's order the set has.
    #[test]
    fn party_motion_remap_order() {
        assert_eq!(motion_state(true, false), 1);
        assert_eq!(motion_state(false, false), 2);
        assert_eq!(motion_state(true, true), 0);
        assert_eq!(remap(0x97, [1, 0, 2, 3], |s| s == 0x97 * 4 + 2), Some(0x97 * 4 + 2));
        assert_eq!(remap(5, [1, 0, 2, 3], |_| false), None);
        assert_eq!(remap(-1, [1, 0, 2, 3], |_| true), None);
    }

    /// The nearest surface within the range wins.
    #[test]
    fn party_style0_nearest_surface() {
        use bevy::math::Vec3;
        let shapes = [(0, Vec3::new(300.0, 0.0, 0.0), 10.0), (1, Vec3::new(0.0, 0.0, 200.0), 80.0), (2, Vec3::new(2000.0, 0.0, 0.0), 10.0)];
        assert_eq!(nearest_target(Vec3::ZERO, 1000.0, shapes.into_iter()), Some(1));
        assert_eq!(nearest_target(Vec3::ZERO, 100.0, shapes.into_iter()), None);
    }

    fn exe() -> Option<Vec<u8>> {
        let iso = crate::assets::iso_path().ok().filter(|p| p.exists())?;
        crate::assets::Disc::open(&iso).read_root("SLUS_210.05")
    }

    /// Donald's and Goofy's action records and parameter entries as the executable holds them
    /// (printed); every attack record has an entry with a motion; the slot-order table is the one
    /// `kh2_anim::PlayerState` uses. Needs `KH2_ISO`.
    #[test]
    fn party_action_records_from_the_executable() {
        let Some(elf) = exe() else { return };
        for class in [0u8, 1] {
            let ai = ClassAi::from_exe(&elf, class).expect("class");
            eprintln!("class {class}: {:?}", ai.params);
            for (i, r) in ai.records.iter().enumerate() {
                eprintln!("  {i:2} flags {:#06x} {:?} id {:#x} ability {} item {} setting {} mode {:?} entry {:?}", r.flags, r.handler, r.id, r.ability, r.item, r.setting, r.mode, r.entry);
            }
            assert_eq!(ai.records.len(), if class == 0 { 0x14 } else { 0x12 });
            assert!(ai.records.iter().filter(|r| r.handler == Handler::Attack).all(|r| r.entry.is_some_and(|e| e.motion > 0)));
            assert!(ai.records.iter().any(|r| r.handler == Handler::Attack));
        }
        let orders = motion_orders(&elf).expect("table");
        eprintln!("FUN_0017da30 slot orders {orders:?}");
        assert_eq!(orders, [[0, 1, 3, 2], [1, 0, 2, 3], [2, 3, 1, 0], [3, 2, 0, 1]]);
    }

    const STACK: u32 = 0x01ff_0000;
    const ENTRY: u32 = 0x0180_0000;
    const REL: u32 = 0x0181_0000;

    /// FUN_0010f118 in kh2-ee against `in_range` over a sweep of entries, radii, heights and distances.
    #[test]
    fn party_range_matches_fun_0010f118() {
        let Some(elf) = exe() else { return };
        let mut ee = kh2_ee::Ee::new();
        ee.load_elf(&elf).expect("elf");
        let mut n = 0;
        for flags in [0u32, 1, 2, 8, 9, 0x10] {
            for r in [0.0f32, 25.0] {
                for dy in [-300.0f32, -120.0, -40.0, 0.0, 30.0, 90.0] {
                    for gd in [0.0f32, 20.0, 70.0, 140.0, 200.0, 400.0] {
                        let e = Entry { flags, near: 40.0, far: 180.0, up: -20.0, down: 150.0, band_near: 50.0, band_far: 160.0, close: 60.0, close_dy: 80.0, ..Entry::default() };
                        for (o, v) in [(0x10, e.near), (0x14, e.far), (0x18, e.up), (0x1c, e.down), (0x2c, e.band_near), (0x30, e.band_far), (0x34, e.close), (0x38, e.close_dy)] {
                            ee.wf32(ENTRY + o, v);
                        }
                        ee.w32(ENTRY + 0xc, flags);
                        ee.wf32(REL, dy);
                        ee.wf32(REL + 0x2c, gd);
                        let game = ee.call(0x10f118, &[0, REL as u64, ENTRY as u64], &[r], STACK).expect("FUN_0010f118") & 0xff;
                        assert_eq!(game != 0, in_range(&e, r, dy, gd), "flags {flags:#x} r {r} dy {dy} gd {gd}");
                        n += 1;
                    }
                }
            }
        }
        eprintln!("FUN_0010f118: {n} cases equal");
    }
}
