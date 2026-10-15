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
    /// The emergency pass's first and second lists (FUN_0010f8d8).
    pub const EMERGENCY_1: u32 = 0x10;
    pub const EMERGENCY_2: u32 = 0x20;
    /// Continuous: re-tested and kept while it holds (FUN_0010fcd0's first step).
    pub const CONTINUOUS: u32 = 0x200;
}

/// The item choice orders of the HP item handler FUN_00113888 (one target, several targets) and of
/// the MP item handler FUN_00113b08, u32 item ids in the executable.
pub const HP_ITEMS_ONE: u32 = 0x352e28;
pub const HP_ITEMS_MANY: u32 = 0x352e3c;
pub const HP_ITEM_CHOICES: usize = 5;
pub const MP_ITEMS_ONE: u32 = 0x352e50;
pub const MP_ITEMS_MANY: u32 = 0x352e60;
pub const MP_ITEM_CHOICES: usize = 4;
/// The HP fraction of FUN_00113888 (`DAT_003785c8`) and the MP fraction of FUN_00113b08 (`DAT_003785d4`).
pub const HP_ITEM_FRACTION: u32 = 0x3785c8;
pub const MP_ITEM_FRACTION: u32 = 0x3785d4;
/// FUN_00110f90's MP-gate weights (`DAT_00378598`, `DAT_0037859c`).
pub const MP_GATE_SLOPE: u32 = 0x378598;
pub const MP_GATE_BASE: u32 = 0x37859c;
/// FUN_0010c800: degrees to radians (`DAT_003784a0`).
pub const DEG_TO_RAD: u32 = 0x3784a0;
/// FUN_0010ccf8's factor (`DAT_003784a4`) and FUN_0010d538's start value (`DAT_003784a8`).
pub const RETARGET_HALF: u32 = 0x3784a4;
pub const FARTHEST_START: u32 = 0x3784a8;

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
    /// +0x84 (FUN_00114118): style 2's range around the player; +0x88 (FUN_00114128): the second
    /// distance of the style 2 and 3 re-target tests.
    pub player_range: f32,
    pub player_near: f32,
    /// `DAT_003784a4` (style 2's re-target factor on +0x84 + +0x88) and `DAT_003784a8` (style 3's start
    /// value of the farthest distance, a "none yet" mark), executable constants.
    pub retarget_half: f32,
    pub farthest_start: f32,
}

/// A 0x20-byte combo follow-up of the class's block (+0xac, FUN_001135a0), as FUN_0010c800 reads its
/// first five words (the instructions at 0x10c82c..0x10c898: the decompilation's argument order is
/// not the register order): a jump away from the target after an attack.
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct FollowUp {
    /// +0: the jump direction's angle in degrees, added to the member's yaw (+0x55c) (Donald 180: a
    /// hop backwards).
    pub angle: f32,
    /// +4: the jump height (FUN_001b0758 reads it from the vector's Y and zeroes it).
    pub height: f32,
    /// +8: the horizontal speed per frame (the vector's W, multiplied into the velocity).
    pub speed: f32,
    /// +0xc: the motion speed of the jump motion (FUN_0017e220's second argument).
    pub motion_speed: f32,
    /// +0x10: frames to the apex (FUN_001b0758: gravity `2h / t^2`, rise `g x t`).
    pub frames: f32,
}

/// A class's actions.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct ClassAi {
    pub params: Params,
    pub records: Vec<Record>,
    pub followups: Vec<FollowUp>,
}

/// FUN_0010c800's jump direction: the member's yaw plus the entry's angle, as (sin, 0, cos) in the
/// game's axes; the app's facing has the same form (`Vec3::new(yaw.sin(), 0, yaw.cos())`).
pub fn followup_dir(yaw: f32, f: &FollowUp, deg_to_rad: f32) -> bevy::math::Vec3 {
    let a = yaw + f.angle * deg_to_rad;
    bevy::math::Vec3::new(a.sin(), 0.0, a.cos())
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
            player_range: rf(elf, block + 0x84)?,
            player_near: rf(elf, block + 0x88)?,
            retarget_half: rf(elf, RETARGET_HALF)?,
            farthest_start: rf(elf, FARTHEST_START)?,
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
        // FUN_001135a0: the follow-ups at block + (+0xac), 0x20 bytes each.
        let fu_at = block + rw(elf, block + 0xac)?;
        let followups = (0..params.followups.min(16))
            .filter_map(|k| {
                let e = fu_at + k * 0x20;
                Some(FollowUp { angle: rf(elf, e)?, height: rf(elf, e + 4)?, speed: rf(elf, e + 8)?, motion_speed: rf(elf, e + 0xc)?, frames: rf(elf, e + 0x10)? })
            })
            .collect();
        Some(ClassAi { params, records, followups })
    }
}

/// The item ids of an item choice order of the executable (`HP_ITEMS_ONE` and the others).
pub fn item_order(elf: &[u8], at: u32, n: usize) -> Vec<u16> {
    (0..n as u32).filter_map(|k| rw(elf, at + 4 * k).map(|v| v as u16)).collect()
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
    /// The caller skipped the choice: the script is busy (+0x390, FUN_0010f698); the member stands.
    Busy,
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

// ---------------------------------------------------------------------------------------------------
// The target choice and re-target tests of every battle style (FUN_0010dcf0, FUN_0010cf90)

/// One hurt shape the game's object walk (FUN_0018f348 over FUN_001b7c88's list) visits, in walk order.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Cand {
    /// The object (the target record's +0) and the shape's place in it.
    pub fighter: usize,
    pub shape: usize,
    /// FUN_001894d0: the shape's centre and radius (the surface distance is `|centre - p| - radius`).
    pub centre: bevy::math::Vec3,
    pub radius: f32,
    /// FUN_0018edd8: the shape answers (FUN_0019a5d0 on the record).
    pub enabled: bool,
    /// FUN_00112ac0 on the object: alive, +0x588 bit 0x10, +0x7f8 bit 4 clear, +0x108 bit 0x8000 clear.
    pub targetable: bool,
}

/// A target record (+0x9f4, `0x34e9b0`): the object and the shape.
pub type Pick = (usize, usize);

/// What the choose and re-target functions read besides the class's block.
#[derive(Clone, Copy, Debug, Default)]
pub struct TargetScene<'a> {
    /// FUN_0016b960 of the member and of the player (`DAT_0034e908`).
    pub member: bevy::math::Vec3,
    pub player: bevy::math::Vec3,
    /// +0xcec: the last object whose hit FUN_00112ac0 accepted (FUN_001106b8).
    pub attacker: Option<usize>,
    /// `0x34e9b0`: the player's target record, copied from the player's +0x9f4 by FUN_00111d18 and
    /// cleared there when it no longer answers (FUN_0018ed58 and a non-zero object).
    pub player_target: Option<Pick>,
    /// `0x34e9c4`: the object of the player's attack start (FUN_00176050 -> FUN_00112850(1, record),
    /// kept only when FUN_00112b28 accepts it; zeroed by FUN_00112270 and when the object dies).
    pub player_hit: Option<usize>,
    /// The object walk, in order.
    pub cands: &'a [Cand],
}

fn surface(c: &Cand, p: bevy::math::Vec3) -> f32 {
    (c.centre - p).length() - c.radius
}

/// FUN_00112b88(range, object, out, position): the object's enabled shapes in walk order; a shape is
/// taken when the best distance so far is at most 0 or its surface distance is below it, and then
/// becomes the best. The best starts at `range`: so with a range of 0 or less (or once a taken shape's
/// surface distance is 0 or less) every later enabled shape replaces the pick, as the code reads.
pub fn nearest_shape_of(range: f32, fighter: usize, pos: bevy::math::Vec3, cands: &[Cand]) -> Option<Pick> {
    let mut best = range;
    let mut out = None;
    for c in cands.iter().filter(|c| c.fighter == fighter && c.enabled) {
        let d = surface(c, pos);
        if best <= 0.0 || d < best {
            out = Some((c.fighter, c.shape));
            best = d;
        }
    }
    out
}

fn find(cands: &[Cand], p: Pick) -> Option<&Cand> {
    cands.iter().find(|c| (c.fighter, c.shape) == p)
}

/// FUN_00112b28 on a record: it answers and its object is targetable.
fn record_ok(cands: &[Cand], p: Pick) -> bool {
    find(cands, p).is_some_and(|c| c.targetable)
}

/// Style 0 (FUN_0010d0c0; style 5 FUN_0010d708 calls it): the remembered attacker's nearest shape
/// within the range (FUN_00112b88 with +0x68), else among the targetable objects' enabled shapes the
/// nearest surface below the range.
pub fn choose_style0(p: &Params, s: &TargetScene) -> Option<Pick> {
    if let Some(a) = s.attacker
        && let Some(t) = nearest_shape_of(p.range, a, s.member, s.cands)
    {
        return Some(t);
    }
    let mut best = p.range;
    let mut out = None;
    for c in s.cands.iter().filter(|c| c.targetable && c.enabled) {
        let d = surface(c, s.member);
        if d < best {
            best = d;
            out = Some((c.fighter, c.shape));
        }
    }
    out
}

/// FUN_0010d848 (style 1's pick): nothing without `0x34e9c4`; the player's target record itself when it
/// answers and is on that object; else that object's shapes in walk order (no enabled test here),
/// the first taken while the best is still 0, then any with a smaller surface distance.
pub fn player_hit_pick(s: &TargetScene) -> Option<Pick> {
    let hit = s.player_hit?;
    if let Some(t) = s.player_target
        && t.0 == hit
    {
        return Some(t);
    }
    let mut best = 0.0f32;
    let mut out = None;
    for c in s.cands.iter().filter(|c| c.fighter == hit) {
        let d = surface(c, s.member);
        if best == 0.0 || d < best {
            out = Some((c.fighter, c.shape));
            best = d;
        }
    }
    out
}

/// Style 1 (FUN_0010d270): FUN_0010d848's pick when it answers and its shape is enabled; else, when
/// the player's target passes FUN_00112b28, that object's nearest shape within the range
/// (FUN_00112b88 with +0x68).
pub fn choose_style1(p: &Params, s: &TargetScene) -> Option<Pick> {
    if let Some(t) = player_hit_pick(s)
        && find(s.cands, t).is_some_and(|c| c.enabled)
    {
        return Some(t);
    }
    match s.player_target {
        Some(t) if record_ok(s.cands, t) => nearest_shape_of(p.range, t.0, s.member, s.cands),
        _ => None,
    }
}

/// Style 2 (FUN_0010d378): among the targetable objects' enabled shapes, the nearest surface to the
/// member below +0x68 whose surface distance to the player is below +0x84.
pub fn choose_style2(p: &Params, s: &TargetScene) -> Option<Pick> {
    let mut best = p.range;
    let mut out = None;
    for c in s.cands.iter().filter(|c| c.targetable && c.enabled) {
        let d = surface(c, s.member);
        if d < best && surface(c, s.player) < p.player_range {
            best = d;
            out = Some((c.fighter, c.shape));
        }
    }
    out
}

/// Style 3 (FUN_0010d538): among the targetable objects' enabled shapes within twice +0x68 of the
/// member (surface distance), the one whose surface lies farthest from the player. The best starts at
/// `DAT_003784a8` (-1): any first candidate is taken while it is negative.
pub fn choose_style3(p: &Params, s: &TargetScene) -> Option<Pick> {
    let mut best = p.farthest_start;
    let mut out = None;
    for c in s.cands.iter().filter(|c| c.targetable && c.enabled) {
        if surface(c, s.member) < p.range + p.range {
            let dp = surface(c, s.player);
            if best < 0.0 || best < dp {
                best = dp;
                out = Some((c.fighter, c.shape));
            }
        }
    }
    out
}

/// The choose function of the table at 0x352d28 (classes up to 0xd) or 0x352ce8 + 8 x class (classes
/// above 0xd, FUN_001124b8), and the re-target test of 0x352cd0 / 0x352c90 + 8 x class, by name.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Chooser {
    /// FUN_0010d0c0 (style 0; style 5's FUN_0010d708 calls it).
    Nearest,
    /// FUN_0010d270.
    PlayerTarget,
    /// FUN_0010d378.
    NearPlayer,
    /// FUN_0010d538.
    FarFromPlayer,
    /// FUN_0010d700: an empty function, no target.
    None,
    /// A routine not read (class 16's 0x119d60).
    Unread(u32),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Retest {
    /// FUN_0010cbe0.
    Attacker,
    /// FUN_0010cc80.
    PlayerTarget,
    /// FUN_0010ccf8.
    NearPlayer,
    /// FUN_0010ce40.
    FarFromPlayer,
    /// FUN_0010cf80 (1) and FUN_0010cf88 (0).
    Always,
    Never,
    /// A routine not read (class 16's 0x119d58).
    Unread(u32),
}

/// FUN_0010dcf0's and FUN_0010cf90's dispatch (the tables as the executable holds them: printed by
/// `party_target_tables_from_the_executable`). `None` for a style outside 0..5 (the table has six rows).
pub fn dispatch(class: u8, style: u8) -> Option<(Chooser, Retest)> {
    if class > 0xd {
        return Some(match class {
            14 => (Chooser::None, Retest::Always),
            15 | 17 => (Chooser::Nearest, Retest::Never),
            16 => (Chooser::Unread(0x119d60), Retest::Unread(0x119d58)),
            18 => (Chooser::Nearest, Retest::Attacker),
            _ => return None,
        });
    }
    Some(match style {
        0 => (Chooser::Nearest, Retest::Attacker),
        1 => (Chooser::PlayerTarget, Retest::PlayerTarget),
        2 => (Chooser::NearPlayer, Retest::NearPlayer),
        3 => (Chooser::FarFromPlayer, Retest::FarFromPlayer),
        4 => (Chooser::None, Retest::Always),
        5 => (Chooser::Nearest, Retest::Never),
        _ => return None,
    })
}

/// Runs a choose function. `limit`: FUN_001b3c00 (a limit runs): 0x352d48's FUN_0010d700, no target.
/// An unread routine keeps no target [the routine was not read].
pub fn choose_target(c: Chooser, limit: bool, p: &Params, s: &TargetScene) -> Option<Pick> {
    if limit {
        return None;
    }
    match c {
        Chooser::Nearest => choose_style0(p, s),
        Chooser::PlayerTarget => choose_style1(p, s),
        Chooser::NearPlayer => choose_style2(p, s),
        Chooser::FarFromPlayer => choose_style3(p, s),
        Chooser::None | Chooser::Unread(_) => None,
    }
}

/// FUN_0010cf90: re-target when the target is not valid (FUN_0010ca88: none, bit 0x40, FUN_00112b28 or
/// FUN_0018edd8 refuse), when a limit runs, else by the style's test. `target`: the current record
/// when FUN_0010ca88 accepts it. `current_counted`: the running action record (+0xc60) exists and
/// lacks flag 1 (FUN_0010ce40 then always re-targets). An unread test re-targets.
pub fn needs_retarget(t: Retest, limit: bool, target: Option<(Pick, bevy::math::Vec3)>, current_counted: bool, p: &Params, s: &TargetScene) -> bool {
    let Some((pick, point)) = target else { return true };
    if limit {
        return true;
    }
    // FUN_0018efa0 (the target point) minus the member's position, squared length.
    let d2 = (point - s.member).length_squared();
    let sq = |x: f32| x * x;
    match t {
        Retest::Attacker => s.attacker.is_some_and(|a| a != pick.0) || d2 > sq(p.retarget),
        Retest::PlayerTarget => s.player_target.is_some_and(|pt| pt.0 != pick.0),
        // The second distance is the target point's to the player (FUN_0016b960 of `DAT_0034e908`), as
        // the oracle test shows (the decompilation's vector copy reads ambiguously).
        Retest::NearPlayer => d2 > sq(p.retarget) || (point - s.player).length_squared() > sq((p.player_range + p.player_near) * p.retarget_half),
        // A target closer to the player than +0x88 re-targets.
        Retest::FarFromPlayer => current_counted || d2 > sq(p.retarget) || (point - s.player).length_squared() < sq(p.player_near),
        Retest::Always | Retest::Unread(_) => true,
        Retest::Never => false,
    }
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

// ---------------------------------------------------------------------------------------------------
// Stages 3 and 4: the status block, danger, the MP gate, the emergency pass, items and Cure

/// FUN_001a0be0 ("HP in danger"): `HP <= maxHP / 4` with the division toward zero.
pub fn in_danger(hp: i32, max: i32) -> bool {
    hp <= max / 4
}

/// FUN_0010f490 / FUN_0010f4d0 ("below a fraction"): `value < int(max x fraction)`.
pub fn below_fraction(value: i32, max: i32, fraction: f32) -> bool {
    value < (max as f32 * fraction) as i32
}

/// FUN_00110f90: the MP gate bit (+0xb7c 0x20) of this frame: with a status block, set when a roll
/// in 0..1 is at most the class's MP-use chance factor (block +0x60) times
/// `MP / maxMP x slope + base` (`DAT_00378598` = 0.75, `DAT_0037859c` = 0.25 as the executable holds them).
pub fn mp_gate(roll: f32, mp_chance: f32, mp: i32, max_mp: i32, slope: f32, base: f32) -> bool {
    let ratio = mp as f32 / max_mp as f32;
    let threshold = mp_chance * (ratio * slope + base);
    // A zero maximum divides by zero in the game too (inf or NaN); the comparison then decides as
    // written: a NaN threshold refuses.
    roll <= threshold
}

/// One of the three party objects FUN_0010f830 and the item handlers look at: the player
/// (`DAT_0034e908[0]`), the member itself, the other member (FUN_00111be8).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Who {
    Player,
    Me,
    Other,
}

/// HP and MP of a status block as the handlers read them (+0, +4, +0x180, +0x184).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct Vital {
    pub hp: i32,
    pub hp_max: i32,
    pub mp: i32,
    pub mp_max: i32,
}

/// FUN_0010f830: the member in danger (+0xce4): the player when his HP is in danger; else, for a
/// party member (not a summon class, FUN_001124b8), itself, else the other member; else nobody.
pub fn danger_member(player: Vital, me: Vital, other: Option<Vital>, summon_class: bool) -> Option<Who> {
    if in_danger(player.hp, player.hp_max) {
        return Some(Who::Player);
    }
    if summon_class {
        return None;
    }
    if in_danger(me.hp, me.hp_max) {
        return Some(Who::Me);
    }
    other.filter(|o| in_danger(o.hp, o.hp_max)).map(|_| Who::Other)
}

/// The target and item of an item handler's phase 1 (FUN_00113888 for HP, FUN_00113b08 for MP):
/// the target starts as the member in danger (+0xce4); each of the member itself, the player and
/// the other member whose value is below `fraction` of its maximum (FUN_0010f490 / FUN_0010f4d0)
/// becomes the target when none is set yet and is counted. With a target, the choice order is
/// `one` for fewer than two in need, else `many`; the first item whose setting byte is below 2 (or,
/// when someone is in danger, any setting) and which the member has in stock (FUN_0010f510 ->
/// FUN_001a1d50: in its own battle item slots) is taken. `None`: nothing to do.
#[allow(clippy::too_many_arguments)]
pub fn item_pick(
    danger: Option<Who>,
    me: Vital,
    player: Vital,
    other: Option<Vital>,
    fraction: f32,
    value: impl Fn(&Vital) -> (i32, i32),
    one: &[u16],
    many: &[u16],
    setting: impl Fn(u16) -> u8,
    stock: impl Fn(u16) -> bool,
) -> Option<(Who, u16)> {
    let mut target = danger;
    let mut count = 0;
    let candidates = [Some((Who::Me, me)), Some((Who::Player, player)), other.map(|o| (Who::Other, o))];
    for (who, v) in candidates.into_iter().flatten() {
        let (val, max) = value(&v);
        if below_fraction(val, max, fraction) {
            if target.is_none() {
                target = Some(who);
            }
            count += 1;
        }
    }
    let target = target?;
    let order = if count < 2 { one } else { many };
    let item = order.iter().copied().find(|&item| (setting(item) < 2 || danger.is_some()) && stock(item))?;
    Some((target, item))
}

/// FUN_00113e68's target (Cure): the member in danger, else the first of the member, the player and
/// the other member whose HP is below the entry's fraction (+0x3c); `None` when nobody needs it.
pub fn cure_target(danger: Option<Who>, me: Vital, player: Vital, other: Option<Vital>, fraction: f32) -> Option<Who> {
    if danger.is_some() {
        return danger;
    }
    let low = |v: &Vital| below_fraction(v.hp, v.hp_max, fraction);
    if low(&me) {
        return Some(Who::Me);
    }
    if low(&player) {
        return Some(Who::Player);
    }
    other.filter(low).map(|_| Who::Other)
}

/// FUN_0010f8d8, the emergency pass, run when someone is in danger and the script is not busy: the
/// MP gate is set, the last three choices are cleared (FUN_0010ecc0) and the repeat count reset;
/// then the records with flag 0x10, and if none is feasible the records with flag 0x20, are tested
/// (`can(i)`, phase 1; a record whose entry costs MP is skipped while the MP charge runs); one of the
/// feasible ones is picked at random and becomes the current record (counted unless flag 1).
/// `None`: nothing was feasible (the previous current record is kept, as the game restores it).
pub fn emergency(recs: &[Record], st: &mut Choosing, status: bool, mp_charge: bool, mut can: impl FnMut(usize) -> bool, mut rand: impl FnMut(u32) -> u32) -> Option<usize> {
    let saved = st.current;
    st.last = [None; 3];
    st.current = None;
    st.repeats = 0;
    for pass_flag in [flag::EMERGENCY_1, flag::EMERGENCY_2] {
        let mut cands: Vec<usize> = Vec::new();
        for (i, r) in recs.iter().enumerate() {
            if matches!(r.handler, Handler::Other(0)) || r.flags & pass_flag == 0 {
                continue;
            }
            if r.entry.is_some_and(|e| e.mp >= 1) && status && mp_charge {
                continue;
            }
            if can(i) && cands.len() < MAX_CANDIDATES {
                cands.push(i);
            }
        }
        if !cands.is_empty() {
            let k = cands[rand(cands.len() as u32) as usize];
            if recs[k].flags & flag::NOT_COUNTED == 0 {
                push_last(&mut st.last, k);
                st.repeats += 1;
            } else {
                st.repeats = 0;
            }
            st.current = Some(k);
            return Some(k);
        }
    }
    st.current = saved;
    None
}

// ---------------------------------------------------------------------------------------------------
// Stage 3: hits, KO and revive of a party object

/// What `FUN_00186238` (the HP method of a party object, +0x588 bit 0x4000 set, through FUN_00176c98)
/// did with a change.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct HpChange {
    /// The KO bit was cleared by a heal (FUN_0017cee8): motion 0x37 when the member lay in 0x36.
    pub revived: bool,
    /// HP reached 0 by this change (FUN_00186530 -> the KO method +0x5c, FUN_0017ce60).
    pub knocked_out: bool,
    /// The amount applied (after FUN_0017cee8's scale); 0 when the change was refused.
    pub applied: i32,
}

/// `FUN_00186238` for a party object on its HP (member index 0): no change at a zero maximum; a heal
/// (`delta >= 1`) on a KO'd object first revives it (FUN_0017cee8: the KO bit cleared, the amount
/// scaled by `revive_scale`, which is 1.0 (`DAT_0037a560`) with the ability block's +0x10 bit 8 and
/// `sstm` +0x54 without); while KO'd nothing else changes; otherwise FUN_001a0100 adds the amount held
/// between the minimum (+8) and the maximum (+4), and an HP of 0 knocks the object out (FUN_00186530,
/// only when the KO bit is clear).
pub fn hp_change(hp: &mut i32, max: i32, min: i32, ko: &mut bool, delta: i32, revive_scale: f32) -> HpChange {
    let mut out = HpChange::default();
    if max == 0 {
        return out;
    }
    let mut delta = delta;
    if delta >= 1 && *ko {
        *ko = false;
        out.revived = true;
        delta = (delta as f32 * revive_scale) as i32;
    }
    if *ko {
        return out;
    }
    let sum = *hp + delta;
    let new = if sum < min { min } else if sum > max { max } else { sum };
    out.applied = new - *hp;
    *hp = new;
    if new == 0 {
        *ko = true;
        out.knocked_out = true;
    }
    out
}

/// FUN_0017ce60: the revive timer (+0xb40) is the StunTime (+0x18) of the record at +0x12c, the `prty`
/// row of the object's `objentry` NeoMoveset (FUN_00196b58 -> FUN_00194438 -> FUN_001cc1c8), times the
/// ability block's +0x58 (Hyper Healing).
pub fn revive_time(stun: f32, hyper_healing: f32) -> f32 {
    stun * hyper_healing
}

/// FUN_0017ca70's count-down of a KO'd member: the frame unit, times `sstm` +0x5c while the battle
/// flag is clear (FUN_001903c8 = 0). True when the timer reached 0 or below: FUN_0017cc20 then heals
/// `maxHP x sstm +0x58` through the HP method.
pub fn revive_step(timer: &mut f32, frame: f32, battle: bool, field_scale: f32) -> bool {
    let step = if battle { frame } else { frame * field_scale };
    *timer -= step;
    *timer <= 0.0
}

/// FUN_0017cc20: the heal of a revive, `int(maxHP x sstm +0x58)`.
pub fn revive_heal(max: i32, scale: f32) -> i32 {
    (max as f32 * scale) as i32
}

/// FUN_001cc140: the row a `pref` lookup answers: `index` when it is below the count and its offset
/// is not 0, else `fallback` (FUN_001cc1c8 passes 1 for `prty`); `None` when that one is empty too.
pub fn pref_row(offsets: &[u32], index: usize, fallback: usize) -> Option<usize> {
    let k = if index < offsets.len() && offsets[index] != 0 { index } else { fallback };
    offsets.get(k).filter(|&&o| o != 0).map(|_| k)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// FUN_00186238 / FUN_0017cee8: damage to 0 knocks out, nothing changes while KO'd, a heal revives
    /// scaled; FUN_0017ce60 / FUN_0017ca70 / FUN_0017cc20: the timer, its count-down and the heal.
    #[test]
    fn party_hp_method_ko_and_revive() {
        let (mut hp, mut ko) = (10, false);
        assert_eq!(hp_change(&mut hp, 20, 0, &mut ko, -4, 0.5), HpChange { applied: -4, ..HpChange::default() });
        let c = hp_change(&mut hp, 20, 0, &mut ko, -30, 0.5);
        assert!(c.knocked_out && ko && hp == 0);
        assert_eq!(hp_change(&mut hp, 20, 0, &mut ko, -3, 0.5), HpChange::default(), "no damage while KO'd");
        let c = hp_change(&mut hp, 20, 0, &mut ko, 10, 0.5);
        assert!(c.revived && !ko && hp == 5 && !c.knocked_out, "the revive heal is scaled");
        assert_eq!(hp_change(&mut hp, 20, 0, &mut ko, 100, 0.5).applied, 15, "held at the maximum");
        assert_eq!(hp_change(&mut hp, 0, 0, &mut ko, -100, 0.5), HpChange::default(), "no block maximum: refused");
        assert_eq!(revive_time(600.0, 0.5), 300.0);
        let mut t = 2.0;
        assert!(!revive_step(&mut t, 1.0, true, 4.0) && revive_step(&mut t, 1.0, true, 4.0));
        let mut t = 2.0;
        assert!(revive_step(&mut t, 1.0, false, 4.0), "out of battle the step is scaled");
        assert_eq!(revive_heal(37, 0.5), 18);
        assert_eq!(pref_row(&[8, 0, 16, 0], 3, 1), None);
        assert_eq!(pref_row(&[8, 12, 16, 0], 3, 1), Some(1));
        assert_eq!(pref_row(&[8, 12, 16], 2, 1), Some(2));
        assert_eq!(pref_row(&[8, 12, 16], 9, 1), Some(1));
    }

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

    /// FUN_001a0be0, FUN_0010f490, FUN_00110f90, FUN_0010f830: danger, the fraction test, the MP gate
    /// and the member in danger.
    #[test]
    fn party_danger_and_mp_gate_rules() {
        assert!(in_danger(5, 20) && !in_danger(6, 20) && in_danger(0, 3));
        assert!(below_fraction(8, 20, 0.45) && !below_fraction(9, 20, 0.45));
        // Full MP: the factor itself; empty MP: a quarter of it.
        assert!(mp_gate(0.5, 0.5, 100, 100, 0.75, 0.25) && !mp_gate(0.51, 0.5, 100, 100, 0.75, 0.25));
        assert!(mp_gate(0.125, 0.5, 0, 100, 0.75, 0.25) && !mp_gate(0.13, 0.5, 0, 100, 0.75, 0.25));
        let v = |hp: i32| Vital { hp, hp_max: 20, mp: 0, mp_max: 10 };
        assert_eq!(danger_member(v(5), v(20), None, false), Some(Who::Player));
        assert_eq!(danger_member(v(20), v(5), None, false), Some(Who::Me));
        assert_eq!(danger_member(v(20), v(5), None, true), None, "a summon class looks only at the player");
        assert_eq!(danger_member(v(20), v(20), Some(v(2)), false), Some(Who::Other));
        assert_eq!(danger_member(v(20), v(20), Some(v(20)), false), None);
    }

    /// FUN_00113888's phase 1: the target order, the one/many lists, the setting and stock gates.
    #[test]
    fn party_item_pick_follows_fun_00113888() {
        let v = |hp: i32| Vital { hp, hp_max: 100, mp: 0, mp_max: 10 };
        let hp = |x: &Vital| (x.hp, x.hp_max);
        let (one, many) = ([1u16, 2, 5, 4, 7], [5u16, 1, 2, 4, 7]);
        // Only the player is low: the single-target list, the first item in stock.
        let pick = item_pick(None, v(100), v(40), None, 0.45, hp, &one, &many, |_| 0, |i| i == 2);
        assert_eq!(pick, Some((Who::Player, 2)));
        // Two in need: the group list.
        let pick = item_pick(None, v(10), v(40), None, 0.45, hp, &one, &many, |_| 0, |_| true);
        assert_eq!(pick, Some((Who::Me, 5)));
        // Setting 2 keeps an item out unless someone is in danger.
        assert_eq!(item_pick(None, v(100), v(40), None, 0.45, hp, &one, &many, |_| 2, |_| true), None);
        assert_eq!(item_pick(Some(Who::Player), v(100), v(40), None, 0.45, hp, &one, &many, |_| 2, |_| true), Some((Who::Player, 1)));
        // Nobody low and nobody in danger: nothing.
        assert_eq!(item_pick(None, v(100), v(100), Some(v(100)), 0.45, hp, &one, &many, |_| 0, |_| true), None);
        assert_eq!(cure_target(None, v(100), v(30), None, 0.4), Some(Who::Player));
        assert_eq!(cure_target(None, v(100), v(100), Some(v(10)), 0.4), Some(Who::Other));
        assert_eq!(cure_target(Some(Who::Me), v(100), v(100), None, 0.4), Some(Who::Me));
    }

    /// FUN_0010f8d8: the first list before the second; the current record is restored when nothing fits.
    #[test]
    fn party_emergency_pass_order() {
        let recs = vec![rec(Handler::HpItem, flag::EMERGENCY_2), rec(Handler::Cure, flag::EMERGENCY_1 | flag::NOT_COUNTED), rec(Handler::Attack, flag::ATTACK)];
        let mut st = Choosing { current: Some(2), last: [Some(2); 3], ..Choosing::default() };
        assert_eq!(emergency(&recs, &mut st, false, false, |_| true, |_| 0), Some(1));
        assert_eq!((st.current, st.last, st.repeats), (Some(1), [None; 3], 0));
        let mut st = Choosing { current: Some(2), ..Choosing::default() };
        assert_eq!(emergency(&recs, &mut st, false, false, |i| i == 0, |_| 0), Some(0));
        assert_eq!((st.current, st.repeats), (Some(0), 1));
        let mut st = Choosing { current: Some(2), ..Choosing::default() };
        assert_eq!(emergency(&recs, &mut st, false, false, |_| false, |_| 0), None);
        assert_eq!(st.current, Some(2));
        let f = FollowUp { angle: 180.0, height: 15.0, speed: 12.0, motion_speed: 0.98, frames: 12.0 };
        let d = followup_dir(0.0, &f, std::f32::consts::PI / 180.0);
        assert!(d.z < -0.999 && d.x.abs() < 1e-3, "a 180 degree entry hops backwards: {d:?}");
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
            eprintln!("  follow-ups (FUN_0010c800 entries): {:?}", ai.followups);
            assert_eq!(ai.followups.len(), ai.params.followups as usize);
            assert!(ai.followups.iter().all(|f| f.frames > 0.0 && f.height > 0.0));
            for (i, r) in ai.records.iter().enumerate() {
                eprintln!("  {i:2} flags {:#06x} {:?} id {:#x} ability {} item {} setting {} mode {:?} entry {:?}", r.flags, r.handler, r.id, r.ability, r.item, r.setting, r.mode, r.entry);
            }
            assert_eq!(ai.records.len(), if class == 0 { 0x14 } else { 0x12 });
            assert!(ai.records.iter().filter(|r| r.handler == Handler::Attack).filter_map(|r| r.entry).all(|e| e.motion > 0));
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

    fn cand(fighter: usize, shape: usize, x: f32, z: f32, r: f32) -> Cand {
        Cand { fighter, shape, centre: bevy::math::Vec3::new(x, 0.0, z), radius: r, enabled: true, targetable: true }
    }

    fn tparams() -> Params {
        Params { range: 1000.0, retarget: 1200.0, player_range: 500.0, player_near: 300.0, retarget_half: 0.5, farthest_start: -1.0, ..Params::default() }
    }

    /// FUN_0010d0c0: the remembered attacker first (its nearest shape within the range), else the nearest
    /// targetable enabled surface; FUN_00112b88's "best <= 0" rule.
    #[test]
    fn party_target_style0_attacker_then_nearest() {
        use bevy::math::Vec3;
        let p = tparams();
        let cands = [cand(1, 0, 600.0, 0.0, 10.0), cand(1, 1, 400.0, 0.0, 10.0), cand(2, 0, 100.0, 0.0, 10.0), Cand { targetable: false, ..cand(3, 0, 50.0, 0.0, 10.0) }, Cand { enabled: false, ..cand(4, 0, 20.0, 0.0, 10.0) }];
        let mut s = TargetScene { cands: &cands, ..TargetScene::default() };
        assert_eq!(choose_style0(&p, &s), Some((2, 0)), "nearest targetable and enabled");
        s.attacker = Some(1);
        assert_eq!(choose_style0(&p, &s), Some((1, 1)), "the attacker's nearest shape");
        s.attacker = Some(9);
        assert_eq!(choose_style0(&p, &s), Some((2, 0)), "an attacker without shapes falls through");
        // A shape the member stands in (surface below 0) makes every later shape of that object win.
        let inside = [cand(1, 0, 0.0, 0.0, 50.0), cand(1, 1, 900.0, 0.0, 10.0)];
        assert_eq!(nearest_shape_of(1000.0, 1, Vec3::ZERO, &inside), Some((1, 1)));
        assert_eq!(nearest_shape_of(0.0, 1, Vec3::ZERO, &cands), Some((1, 1)), "range 0 takes the last shape");
        assert_eq!(nearest_shape_of(100.0, 1, Vec3::ZERO, &cands), None);
    }

    /// FUN_0010d270 / FUN_0010d848: the player's attacked object, its exact record when it is the player's
    /// target, else its nearest shape; then the player's target object's nearest shape within the range.
    #[test]
    fn party_target_style1_follows_the_player() {
        let p = tparams();
        let cands = [cand(1, 0, 600.0, 0.0, 10.0), cand(1, 1, 400.0, 0.0, 10.0), cand(2, 0, 100.0, 0.0, 10.0), cand(2, 1, 1500.0, 0.0, 10.0)];
        let mut s = TargetScene { cands: &cands, ..TargetScene::default() };
        assert_eq!(choose_style1(&p, &s), None, "no player target, no attack: nothing");
        s.player_target = Some((2, 1));
        assert_eq!(choose_style1(&p, &s), Some((2, 0)), "the player's target object's nearest shape");
        s.player_hit = Some(2);
        assert_eq!(choose_style1(&p, &s), Some((2, 1)), "the player's exact record when it is on the attacked object");
        s.player_hit = Some(1);
        assert_eq!(choose_style1(&p, &s), Some((1, 1)));
        // A disabled pick falls back to the player's target.
        let off = [Cand { enabled: false, ..cands[0] }, Cand { enabled: false, ..cands[1] }, cands[2], cands[3]];
        s.cands = &off;
        assert_eq!(choose_style1(&p, &s), Some((2, 0)));
        // An untargetable player target gives nothing.
        let untarget = [cands[0], cands[1], Cand { targetable: false, ..cands[2] }, Cand { targetable: false, ..cands[3] }];
        s.cands = &untarget;
        s.player_hit = None;
        assert_eq!(choose_style1(&p, &s), None);
    }

    /// FUN_0010d378 and FUN_0010d538: near the member and the player; within twice the range and farthest
    /// from the player.
    #[test]
    fn party_target_styles_2_and_3() {
        use bevy::math::Vec3;
        let p = tparams();
        // The player stands at x = 1000.
        let cands = [cand(1, 0, 200.0, 0.0, 10.0), cand(2, 0, 700.0, 0.0, 10.0), cand(3, 0, 1900.0, 0.0, 10.0), cand(4, 0, -1500.0, 0.0, 10.0)];
        let s = TargetScene { player: Vec3::new(1000.0, 0.0, 0.0), cands: &cands, ..TargetScene::default() };
        assert_eq!(choose_style2(&p, &s), Some((2, 0)), "object 1 is nearer the member but too far from the player");
        assert_eq!(choose_style3(&p, &s), Some((4, 0)), "within 2000 of the member, farthest from the player");
        let none = TargetScene { cands: &[], ..s };
        assert_eq!((choose_style2(&p, &none), choose_style3(&p, &none)), (None, None));
    }

    /// FUN_0010cf90's dispatch through 0x352d28 / 0x352cd0, read from the executable: every row names
    /// the routine `dispatch` maps it to. Needs `KH2_ISO`.
    #[test]
    fn party_target_tables_from_the_executable() {
        let Some(elf) = exe() else { return };
        let ch = |c: Chooser| match c {
            Chooser::Nearest => vec![0x10d0c0, 0x10d708],
            Chooser::PlayerTarget => vec![0x10d270],
            Chooser::NearPlayer => vec![0x10d378],
            Chooser::FarFromPlayer => vec![0x10d538],
            Chooser::None => vec![0x10d700],
            Chooser::Unread(a) => vec![a],
        };
        let rt = |t: Retest| match t {
            Retest::Attacker => 0x10cbe0,
            Retest::PlayerTarget => 0x10cc80,
            Retest::NearPlayer => 0x10ccf8,
            Retest::FarFromPlayer => 0x10ce40,
            Retest::Always => 0x10cf80,
            Retest::Never => 0x10cf88,
            Retest::Unread(a) => a,
        };
        for style in 0..6u8 {
            let (c, t) = dispatch(0, style).expect("style");
            let (cv, tv) = (rw(&elf, 0x352d28 + 8 * style as u32).unwrap(), rw(&elf, 0x352cd0 + 8 * style as u32).unwrap());
            eprintln!("style {style}: choose {cv:#x} re-target {tv:#x}");
            assert!(ch(c).contains(&cv) && rt(t) == tv, "style {style}");
        }
        for class in 14..=18u8 {
            let (c, t) = dispatch(class, 0).expect("class");
            let (cv, tv) = (rw(&elf, 0x352ce8 + 8 * class as u32).unwrap(), rw(&elf, 0x352c90 + 8 * class as u32).unwrap());
            eprintln!("class {class}: choose {cv:#x} re-target {tv:#x}");
            assert!(ch(c).contains(&cv) && rt(t) == tv, "class {class}");
        }
        assert_eq!(rw(&elf, 0x352d48), Some(0x10d700), "a limit chooses nothing");
        let p = ClassAi::from_exe(&elf, 0).expect("class 0").params;
        eprintln!("class 0 target params: range {} retarget {} player {} near {} half {} start {}", p.range, p.retarget, p.player_range, p.player_near, p.retarget_half, p.farthest_start);
        assert!(p.farthest_start < 0.0);
    }

    const MEMBER: u32 = 0x0182_0000;
    const PLAYER: u32 = 0x0182_8000;
    const AI: u32 = 0x0183_0000;
    const BLOCK: u32 = 0x0184_0000;
    const CUR: u32 = 0x0185_0000;
    const POINT: u32 = 0x0186_0000;

    /// The four re-target tests FUN_0010cbe0, FUN_0010cc80, FUN_0010ccf8, FUN_0010ce40 in kh2-ee against
    /// `needs_retarget` over a sweep. Their callees that only hand over inputs are replaced by stubs:
    /// FUN_0016b960 (the position getter) answers `obj + 0x540`, FUN_0018efa0 (the target point) copies
    /// the vector at `POINT`, FUN_0018ee20 answers 0 (so FUN_0018ed58 is "record +8 non-zero").
    #[test]
    fn party_retarget_tests_match_the_executable() {
        use bevy::math::Vec3;
        let Some(elf) = exe() else { return };
        let mut ee = kh2_ee::Ee::new();
        ee.load_elf(&elf).expect("elf");
        let code = |ee: &mut kh2_ee::Ee, at: u32, words: &[u32]| words.iter().enumerate().for_each(|(i, &w)| ee.w32(at + 4 * i as u32, w));
        code(&mut ee, 0x16b960, &[0x03e0_0008, 0x2482_0540]);
        code(&mut ee, 0x18ee20, &[0x03e0_0008, 0x0000_1021]);
        let hi = 0x3c08_0000 | (POINT >> 16);
        code(&mut ee, 0x18efa0, &[hi, 0x8d09_0000, 0xac89_0000, 0x8d09_0004, 0xac89_0004, 0x8d09_0008, 0xac89_0008, 0x8d09_000c, 0xac89_000c, 0x03e0_0008, 0x0080_1021]);
        let mut p = tparams();
        let mut n = 0;
        let wv = |ee: &mut kh2_ee::Ee, at: u32, v: Vec3| {
            ee.wf32(at, v.x);
            ee.wf32(at + 4, v.y);
            ee.wf32(at + 8, v.z);
            ee.wf32(at + 12, 1.0);
        };
        ee.w32(MEMBER + 0xb60, AI);
        ee.w32(AI + 8, BLOCK);
        ee.w32(0x34e908, PLAYER);
        for (near, player_range) in [(300.0f32, 500.0f32), (900.0, 1500.0)] {
            p.player_near = near;
            p.player_range = player_range;
            ee.wf32(BLOCK + 0x6c, p.retarget);
            ee.wf32(BLOCK + 0x84, p.player_range);
            ee.wf32(BLOCK + 0x88, p.player_near);
            p.retarget_half = rf(&elf, RETARGET_HALF).unwrap();
            for member in [Vec3::ZERO, Vec3::new(100.0, -50.0, 30.0)] {
                for dist in [0.0f32, 100.0, 299.0, 301.0, 399.0, 401.0, 700.0, 1199.0, 1201.0, 3000.0] {
                    for player in [Vec3::ZERO, Vec3::new(800.0, 0.0, 0.0)] {
                        for (attacker, target, ptarget) in [(0usize, 7usize, 0usize), (7, 7, 7), (8, 7, 8), (0, 7, 7)] {
                            for cur in [None, Some(0u32), Some(1)] {
                                let point = member + Vec3::new(dist * 0.6, 0.0, dist * 0.8);
                                wv(&mut ee, MEMBER + 0x540, member);
                                wv(&mut ee, PLAYER + 0x540, player);
                                wv(&mut ee, POINT, point);
                                ee.w32(MEMBER + 0x9f4, target as u32);
                                ee.w32(MEMBER + 0xcec, attacker as u32);
                                ee.w32(0x34e9b0, ptarget as u32);
                                ee.w32(0x34e9b8, 1);
                                match cur {
                                    None => ee.w32(MEMBER + 0xc60, 0),
                                    Some(f) => {
                                        ee.w32(MEMBER + 0xc60, CUR);
                                        ee.w32(CUR, f);
                                    }
                                }
                                let s = TargetScene { member, player, attacker: (attacker != 0).then_some(attacker), player_target: (ptarget != 0).then_some((ptarget, 0)), cands: &[], ..TargetScene::default() };
                                let tgt = Some(((target, 0), point));
                                for (addr, t) in [(0x10cbe0, Retest::Attacker), (0x10cc80, Retest::PlayerTarget), (0x10ccf8, Retest::NearPlayer), (0x10ce40, Retest::FarFromPlayer)] {
                                    let game = ee.call(addr, &[MEMBER as u64], &[], STACK).expect("re-target test") & 0xff;
                                    assert_eq!(game != 0, needs_retarget(t, false, tgt, cur == Some(0), &p, &s), "{addr:#x} member {member} dist {dist} player {player} a/t/p {attacker}/{target}/{ptarget} cur {cur:?} near {near}");
                                    n += 1;
                                }
                            }
                        }
                    }
                }
            }
        }
        eprintln!("re-target tests: {n} cases equal");
    }
}
