//! Pure rules of the party members' field behaviour (docs/systems/party/party-friend-ai.md, stage 1 of section 9): the slot
//! bytes and the objects `FRIEND_1` / `FRIEND_2` stand for, the friend-AI class and its follow
//! thresholds, the follow points around the player, the follow step, the steer, the player trail and
//! the warp (catch-up). Each function names the game routine it ports; `party.rs` runs them.
//!
//! Spaces. The rules work in game units (centimetres) in the app's axes (Y up, the game's space turned
//! half a turn about X). Every test below is invariant under that turn (distances, dot products of two
//! vectors that both turn, products of two height differences), except the player's local axes, which
//! `follow_points` takes in the app's space (see there).
//!
//! Values. Every number of the game's data is read from the executable at run time (`Consts::from_exe`,
//! the addresses are in each field's doc comment). Numbers that are instruction immediates (loop counts,
//! a `slti` bound) are named constants with the instruction address.

use bevy::math::{Vec3, Vec4};

/// `00objentry` ids the object factory resolves at run time (FUN_0016e208): the two friends.
pub const FRIEND_1: u32 = 0x237;
pub const FRIEND_2: u32 = 0x238;
/// "No member" in a slot byte (FUN_001a71e8 masks the byte with 0x1f; 0x10 is empty).
pub const EMPTY: u8 = 0x10;
/// The party preset stage 1 starts from (docs/systems/party/party-friend-ai.md section 9: "default preset 1").
pub const DEFAULT_PRESET: usize = 1;
/// Points in the player's trail (FUN_0010b270: 32 entries after a 16-byte header at 0x3b36c0).
pub const TRAIL_LEN: usize = 32;
/// Wanted velocities the steer averages (FUN_00110468: a ring of six at +0xc70).
pub const STEER_LEN: usize = 6;
/// The warp needs the far counter above 39 (FUN_00110a58 at 0x110ab8: `slti t7, 0x28`).
pub const WARP_FAR_MIN: i32 = 0x28;
/// The first trail point a warp tries is `warps + 0x1c + random(4)` (FUN_00110a58 at 0x110ccc / 0x110cbc).
pub const WARP_BASE: i32 = 0x1c;
pub const WARP_RANDOM: u32 = 4;
/// Trail points tried per warp check and the step between them (FUN_00110a58: 11 tries, +3 modulo 32).
pub const WARP_TRIES: usize = 11;
pub const WARP_STEP: usize = 3;
/// Rows of the follow thresholds (FUN_00114078 / FUN_001140c0): field, battle while following the
/// player, battle with another target.
pub const ROW_FIELD: usize = 0;
/// In battle, following the player and moving to another target (FUN_00114078 / FUN_001140c0 +0x20, +0x40).
pub const ROW_BATTLE_PLAYER: usize = 1;
pub const ROW_BATTLE_TARGET: usize = 2;
/// Speed modes (+0xb78): 0 stand, 1 walk, 2 run (FUN_0010e5b8).
pub const MODES: usize = 3;
/// AI classes in the parameter table at 0x34ea00 (docs/systems/party/party-friend-ai.md section 4).
pub const CLASSES: usize = 19;

/// The follow thresholds of one AI class: `[row][mode]` = (slow-down distance, speed-up distance),
/// from the class's block in the table at 0x34ea00 (`block + row * 0x20 + mode * 8`, +4).
pub type Thresholds = [[(f32, f32); MODES]; 3];

/// The executable's numbers the stage-1 rules use.
#[derive(Clone, Debug, PartialEq)]
pub struct Consts {
    /// Per AI class, the follow thresholds (0x34ea00 table).
    pub classes: Vec<Thresholds>,
    /// 0x378480: squared spacing of the player's trail points (FUN_0010b270).
    pub trail_step2: f32,
    /// 0x3785a0: a height difference to the player above this counts toward the warp (FUN_00111070).
    pub far_dy: f32,
    /// 0x3785b4 / 0x3785b8: the sideways components of the two follow offsets, 0x3785ac their
    /// forward component (FUN_00111670 builds them once into 0x3b4030 / 0x3b4040).
    pub side: [f32; 2],
    pub ahead: f32,
    /// 0x3785a8 / 0x3785a4: the follow distance is the class's field slow-down distance of mode 1
    /// minus this times the body radius (+0x5b8), at least the second (FUN_001115d0).
    pub radius_k: f32,
    pub follow_min: f32,
    /// 0x378578: the steer divides the summed speeds by this; 0x37857c: it mixes and snaps by this
    /// (FUN_00110468).
    pub steer_div: f32,
    pub steer_mix: f32,
    /// 0x3784c0: standing, a member whose facing points this little toward the player starts turning;
    /// 0x3784b4 the facing's weight in the turn direction; 0x3784b8 the walk-speed factor of the turn;
    /// 0x3784bc the facing dot above which the turn ends (FUN_0010e140, motion state 0x1f).
    pub turn_on: f32,
    pub turn_mix: f32,
    pub turn_speed: f32,
    pub turn_off: f32,
    /// 0x378580 / 0x378584: the trail-point direction test of the warp (FUN_001108c8).
    pub warp_dy: f32,
    pub warp_dot: f32,
    /// 0x378590: fade-in frames after a warp; 0x378594: the warp cooldown (FUN_00110e28).
    pub warp_fade: f32,
    pub warp_cooldown: f32,
}

/// Bytes at a virtual address of the executable, through its program headers.
pub(crate) fn exe_bytes(elf: &[u8], va: u32, len: usize) -> Option<&[u8]> {
    let u16_at = |o: usize| elf.get(o..o + 2).map(|b| u16::from_le_bytes([b[0], b[1]]) as usize);
    let u32_at = |o: usize| elf.get(o..o + 4).map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]));
    if elf.get(0..4)? != b"\x7fELF" {
        return None;
    }
    let (phoff, phentsize, phnum) = (u32_at(0x1c)? as usize, u16_at(0x2a)?, u16_at(0x2c)?);
    for i in 0..phnum {
        let p = phoff + i * phentsize;
        let (kind, off, vaddr, filesz) = (u32_at(p)?, u32_at(p + 4)?, u32_at(p + 8)?, u32_at(p + 16)?);
        if kind == 1 && va >= vaddr && (va - vaddr) as usize + len <= filesz as usize {
            let start = off as usize + (va - vaddr) as usize;
            return elf.get(start..start + len);
        }
    }
    None
}

/// The friend-AI parameter table (FUN_0010b780 binds `0x34ea00 + offset[class]` to the AI object).
pub(crate) const AI_PARAMS: u32 = 0x34ea00;

impl Consts {
    pub fn from_exe(elf: &[u8]) -> Option<Consts> {
        let w = |va: u32| exe_bytes(elf, va, 4).map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]));
        let f = |va: u32| w(va).map(f32::from_bits);
        let mut classes = Vec::with_capacity(CLASSES);
        for class in 0..CLASSES as u32 {
            let block = AI_PARAMS + w(AI_PARAMS + 4 * class)?;
            let mut t = [[(0.0, 0.0); MODES]; 3];
            for (row, r) in t.iter_mut().enumerate() {
                for (mode, m) in r.iter_mut().enumerate() {
                    let at = block + row as u32 * 0x20 + mode as u32 * 8;
                    *m = (f(at)?, f(at + 4)?);
                }
            }
            classes.push(t);
        }
        Some(Consts {
            classes,
            trail_step2: f(0x378480)?,
            far_dy: f(0x3785a0)?,
            side: [f(0x3785b4)?, f(0x3785b8)?],
            ahead: f(0x3785ac)?,
            radius_k: f(0x3785a8)?,
            follow_min: f(0x3785a4)?,
            steer_div: f(0x378578)?,
            steer_mix: f(0x37857c)?,
            turn_on: f(0x3784c0)?,
            turn_mix: f(0x3784b4)?,
            turn_speed: f(0x3784b8)?,
            turn_off: f(0x3784bc)?,
            warp_dy: f(0x378580)?,
            warp_dot: f(0x378584)?,
            warp_fade: f(0x378590)?,
            warp_cooldown: f(0x378594)?,
        })
    }

    /// The thresholds of a class (the first block for an unknown class, as FUN_001133e0 binds it).
    pub fn thresholds(&self, class: u8) -> Thresholds {
        self.classes.get(class as usize).or(self.classes.first()).copied().unwrap_or_default()
    }
}

// ---------------------------------------------------------------------------------------------------
// Who the friends are

/// FUN_0017cd20: the object a slot byte names in the world's merged `memt` record (24 words, members
/// at words 8..24 by member index); `None` for an empty slot (0x10) or a zero word.
pub fn slot_object(merged: &[u16; 24], slot_byte: u8) -> Option<u16> {
    let i = (slot_byte & 0x1f) as usize;
    if i == EMPTY as usize {
        return None;
    }
    merged.get(8 + i).copied().filter(|&o| o != 0)
}

/// FUN_001a7500: the member index of a NeoStatus (1 and 14 the player 0, 2 Donald 1, 3 Goofy 2, others 3).
pub fn member_index(neo: u16) -> u8 {
    match neo {
        1 | 14 => 0,
        2 => 1,
        3 => 2,
        _ => 3,
    }
}

/// Auto Change (FUN_0017ce60 -> FUN_001b42f0 / FUN_0017d0f0 -> FUN_001b4170, the swap FUN_001b4260 ->
/// FUN_001a7558): the slot bytes after a KO'd member of `neo` that has the ability is swapped for the
/// reserve member, or `None` when the rule does not fire. It needs a filled byte 3 (FUN_001b42f0), and the
/// member's slot (FUN_001a7608 over the four slots) must not be slot 0 and must not carry the 0x80 flag
/// (FUN_001a7268; the hidden-member list FUN_001c7be8 is not modelled). The swap: slot 1 or 2 holding the
/// member's index takes byte 3's member index, and byte 3 takes the member's index (bytes masked with 0x9f,
/// FUN_001a7250). Not modelled [inferred satisfied]: no limit running (FUN_001b3c00), no swap task running
/// (`0x35fec0`), FUN_0017d030 (no object with +0x570 set to the member).
pub fn auto_change_swap(bytes: [u8; 4], neo: u16) -> Option<[u8; 4]> {
    if bytes[3] & 0x1f == EMPTY {
        return None;
    }
    let index = member_index(neo);
    let slot = (0..4).find(|&i| bytes[i] & 0x1f == index)?;
    if slot == 0 || bytes[slot] & 0x80 != 0 {
        return None;
    }
    let mut out = bytes;
    if let Some(i) = (1..3).find(|&i| bytes[i] & 0x1f == index) {
        out[i] = bytes[3] & 0x1f & 0x9f;
    }
    out[3] = index & 0x9f;
    Some(out)
}

/// `FUN_001a0100(block, delta, 0)`: the HP method of a status block without an owning actor, `FUN_001a0140`'s
/// fallback. The sum `current + delta` is held between the minimum (block word 2, 0 for a party member) and the
/// maximum (word 1); a sum below the minimum gives the minimum.
pub fn clamped_add(current: i32, delta: i32, min: i32, max: i32) -> i32 {
    let sum = current + delta;
    if min <= sum { sum.min(max) } else { min }
}

/// The HP and MP a member starts with when his block is made (`FUN_0019f920`): `min(maximum, current)` of the record
/// for each. A stored HP of 0 starts at the maximum: the original builds the block with HP 0 and whatever the actor does
/// with it was not traced [gap, conservative: the app's earlier start]. No record: the maxima.
pub fn spawn_vitals(record: Option<(u8, u8)>, hp_max: i32, mp_max: i32) -> (i32, i32) {
    match record {
        Some((hp, mp)) => (if hp == 0 { hp_max } else { i32::from(hp).min(hp_max) }, i32::from(mp).min(mp_max)),
        None => (hp_max, mp_max),
    }
}

/// `FUN_0017d4b8`'s timer (`DAT_0035cc08`): the frame step comes off it; when it is at or below zero the pass runs and
/// the timer takes the interval (`sstm` +0x168). The timer starts at 0 (the variable is cleared at boot), so the first
/// pass runs on the first tick. Returns whether the pass runs.
pub fn auto_heal_due(timer: &mut f32, step: f32, interval: f32) -> bool {
    *timer -= step;
    if *timer <= 0.0 {
        *timer = interval;
        return true;
    }
    false
}

/// The members `FUN_0017d4b8` visits: party slots 1 to 3 (`FUN_001a7200(record, 1..3)`: the member the slot byte names
/// through the world's `memt`, `member_of` giving the NeoStatus of an object) whose member is not one of the field
/// actors (`FUN_0017d400` answers 0 for each of the two companion entries `FUN_0017cd08(0, 1)`). `field` holds the
/// NeoStatus of the members standing on the field.
pub fn reserve_members(bytes: [u8; 4], merged: &[u16; 24], member_of: impl Fn(u16) -> Option<u16>, field: &[u16]) -> Vec<u16> {
    (1..4).filter_map(|i| slot_object(merged, bytes[i]).and_then(&member_of)).filter(|m| !field.contains(m)).collect()
}

/// FUN_001a7330 (area-script command 0xf): the preset's player index; then the preset's members
/// flagged 0x80/0x40; then the current members (slots 1..3 in order) the preset also lists; then the
/// preset's remaining members; the rest empty. Stored bytes are masked with 0x9f.
pub fn apply_preset(cur: [u8; 4], preset: [u8; 4]) -> [u8; 4] {
    let mut tmp = preset;
    let mut out = [tmp[0] & 0x1f, EMPTY, EMPTY, EMPTY];
    let mut k = 1;
    let mut put = |out: &mut [u8; 4], v: u8| {
        if k < 4 {
            out[k] = v & 0x9f;
        }
        k += 1;
    };
    for t in tmp.iter_mut().skip(1) {
        if *t & 0xc0 != 0 {
            put(&mut out, *t);
            *t = EMPTY;
        }
    }
    for &c in &cur[1..] {
        let v = c & 0x1f;
        if v != EMPTY
            && let Some(j) = (0..4).find(|&j| tmp[j] & 0x1f == v)
        {
            put(&mut out, v);
            tmp[j] = EMPTY;
        }
    }
    for &t in &tmp[1..] {
        if t & 0x1f != EMPTY {
            put(&mut out, t);
        }
    }
    out
}

/// A jump record's party arrangement (record word 4, `FUN_0019d9d8`): when the word is 0 or more, `FUN_001a7330` is
/// applied to the destination world's record (`FUN_001a71c0(world)`, the current world when the record names none)
/// with the preset of that index in the end of `memt`; a negative word applies nothing. Returns the new bytes.
pub fn arrange(cur: [u8; 4], word: i16, presets: &[u32]) -> Option<[u8; 4]> {
    if word < 0 {
        return None;
    }
    let preset = presets.get(word as usize).map(|w| w.to_le_bytes())?;
    Some(apply_preset(cur, preset))
}

/// What the party reset `FUN_001abfe8(code)` does, as far as the app has the state.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ResetEffect {
    /// A drive form ends: `FUN_001c75c0` gives the members the form took back and `FUN_001ac170` zeroes the form
    /// bytes (`DAT_00340c88` +4, +5) and resets the player's status block.
    pub end_form: bool,
    /// The loop over the status blocks 1 to 15 (`FUN_001ac130`): HP raised by its maximum and clamped (`FUN_001a0100`)
    /// and MP set to its maximum with a running MP charge ended (`FUN_001a0240`), the same pair a form start runs
    /// (docs/systems/drive-limits-summons/drive-forms.md).
    pub restore: bool,
    /// Code 5 raises bit 0x40000 of `DAT_0035d268` (the HUD's locked drive command, docs/systems/ui/contract.md); the app does not model it.
    pub lock_drive_command: bool,
}

/// `FUN_001abfe8(code)`. `form_active` is the low half of `DAT_00340c88` +4 (a form other than base), `anti` its byte 4 being 5.
/// Code 0 returns at once. Codes 4, 5, 6 and 0xb return after ending the form (4 and 5 only when a form is active, 0xb only for
/// byte 4 = 5); every other code (also negative ones, 3, 7, 8, 9 and above 0xc) ends the form for 1 (when active), 2, 0xc and 10 and
/// then restores every block.
pub fn reset_effect(code: i64, form_active: bool, anti: bool) -> ResetEffect {
    let mut e = ResetEffect::default();
    match code {
        0 => {}
        4..=6 => {
            e.lock_drive_command = code == 5;
            e.end_form = code == 6 || form_active;
        }
        0xb => e.end_form = anti,
        1 => {
            e.end_form = form_active;
            e.restore = true;
        }
        2 | 0xc | 10 => {
            e.end_form = true;
            e.restore = true;
        }
        _ => e.restore = true,
    }
    e
}

/// A party slot as page 0x15 sees it: the member (`FUN_001a7200`: the `objentry` member id of the slot's object, 0 for an
/// empty slot) and whether the slot is locked (`FUN_001a7268`: the leader's slot 0, a slot byte with bit 0x80, or a
/// member in the hidden list the app does not keep).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct NoticeSlot {
    pub member: u16,
    pub locked: bool,
}

/// `FUN_001a7200` and `FUN_001a7268` for the four bytes of a record; `member_of` gives the member id of an object.
pub fn notice_slots(bytes: [u8; 4], merged: &[u16; 24], member_of: impl Fn(u16) -> Option<u16>) -> [NoticeSlot; 4] {
    std::array::from_fn(|i| NoticeSlot {
        member: slot_object(merged, bytes[i]).and_then(&member_of).unwrap_or(0),
        locked: i == 0 || bytes[i] & 0x80 != 0,
    })
}

/// The contents of page 0x15 (`FUN_0026e970`, `FUN_0026e618`): the members that came in and went out, each with the lock
/// flag that puts icon code 10 before the name, and the member whose line closes the notice.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Notice {
    pub came_in: Vec<NoticeSlot>,
    pub went_out: Vec<NoticeSlot>,
    /// The line under the lists: the member of the last locked slot 1 to 3 of the new party (`FUN_0026e618`; its message
    /// is the entry of the table at 0x381b08).
    pub reason: Option<u16>,
}

/// `FUN_0026e970` over the new party (`FUN_002a1848`) and the stored one (`DAT_0036cffc`, the record as it stood before the
/// arrangement): a non-empty member of one that the other lacks, in slot order, locked by its own party's slot.
pub fn switch_notice(prev: [NoticeSlot; 4], now: [NoticeSlot; 4]) -> Notice {
    let diff = |a: &[NoticeSlot; 4], b: &[NoticeSlot; 4]| a.iter().filter(|s| s.member != 0 && !b.iter().any(|t| t.member == s.member)).copied().collect();
    let reason = (1..4).rev().find(|&i| now[i].locked).map(|i| now[i].member);
    Notice { came_in: diff(&now, &prev), went_out: diff(&prev, &now), reason }
}

/// The party presets the `btl` program that starts `mission` applies (WP-S8): the area-script interpreter
/// FUN_00180b48 runs a program's commands in order; opcode 0x15 (FUN_001b9338) creates the mission manager with
/// argument 0 the mission id, opcode 0xf (FUN_001a7330) applies preset argument 0 to the current world's slot
/// bytes. The program's FRIEND_1 / FRIEND_2 entities then resolve through those bytes (FUN_0016e208 ->
/// FUN_0017cd20, none for an empty byte). Returns the 0xf arguments of the first program whose 0x15 names the
/// mission, in program order (tt04 0xb6..0xb8, the Struggle: `[0]`, Sora alone).
pub fn mission_presets(btl: &[kh2_ard::Program], mission: u32) -> Vec<u32> {
    btl.iter()
        .find(|p| p.commands.iter().any(|c| c.opcode == 0x15 && c.args.first() == Some(&mission)))
        .map(|p| p.commands.iter().filter(|c| c.opcode == 0xf).filter_map(|c| c.args.first().copied()).collect())
        .unwrap_or_default()
}

/// The area's own opcode 0xf commands at area load (FUN_00181148 runs the `map`, `btl` and `evt` programs, FUN_00180b48
/// case 0xf -> FUN_001a7330 on the current world's record): each argument names a preset of the end of `memt`
/// (`presets`, the trailing words), applied in program order to `cur`. A preset the table lacks is skipped.
pub fn apply_world_records(mut cur: [u8; 4], records: &[u32], presets: &[u32]) -> [u8; 4] {
    for &r in records {
        if let Some(p) = presets.get(r as usize) {
            cur = apply_preset(cur, p.to_le_bytes());
        }
    }
    cur
}

/// FUN_0010b780: the friend-AI class (index into the table at 0x34ea00) for an object's NeoStatus
/// and NeoMoveset; `None` = no AI object (Sora 1, Mickey 4, Roxas 14). An unlisted NeoStatus gets
/// the default class (FUN_001133e0) bound to the table's first block.
pub fn ai_class(neo: u16, moveset: u16) -> Option<u8> {
    Some(match neo {
        1 | 4 | 14 => return None,
        2 if moveset == 0x12 => 12,
        2 => 0,
        3 if moveset == 0x15 => 13,
        3 => 1,
        5 => 2,
        15 => 3,
        6 => 4,
        7 => 5,
        8 => 6,
        9 => 7,
        10 => 8,
        11 => 9,
        12 => 10,
        13 => 11,
        200..=204 => (neo - 200 + 14) as u8,
        _ => 0,
    })
}

// ---------------------------------------------------------------------------------------------------
// The party manager: trail and follow points

/// The player's trail (FUN_0010b270 on 0x3b36c0): 32 positions, a new one when the player is at
/// least `sqrt(trail_step2)` from the newest.
#[derive(Clone, Debug, PartialEq)]
pub struct Trail {
    pub points: [Vec3; TRAIL_LEN],
    /// Index of the newest point (+4 of the header), -1 before the first.
    pub head: i32,
    /// Byte 0 of the header: set once a point has been added after the first fill.
    pub moved: bool,
}

impl Default for Trail {
    fn default() -> Self {
        Trail { points: [Vec3::ZERO; TRAIL_LEN], head: -1, moved: false }
    }
}

impl Trail {
    /// One frame: the first call fills every entry with `p`; later calls add `p` after the newest when
    /// it is far enough. Returns whether a point was written this frame (header byte 1).
    pub fn push(&mut self, p: Vec3, step2: f32) -> bool {
        if self.head < 0 {
            self.points = [p; TRAIL_LEN];
            self.head = 0;
            return true;
        }
        if (p - self.points[self.head as usize]).length_squared() < step2 {
            return false;
        }
        self.head = (self.head + 1).rem_euclid(TRAIL_LEN as i32);
        self.points[self.head as usize] = p;
        self.moved = true;
        true
    }
}

/// FUN_001115d0: how far ahead of the player a member's follow points lie: the class's field
/// slow-down distance of mode 1, minus `radius_k` times the body radius (+0x5b8, the radius of the
/// body shape, FUN_00169af0), at least `follow_min`. (Doubled in battle for style 3; no battle here.)
pub fn follow_distance(t: &Thresholds, radius: f32, c: &Consts) -> f32 {
    (t[ROW_FIELD][1].0 - radius * c.radius_k).max(c.follow_min)
}

/// FUN_00111670: the follow point of each member. The two offsets `(side[k], 0, ahead)` times the
/// member's follow distance are carried through the player's model matrix (axis rows from +0x6b0,
/// position row from +0x70: [inferred] to be the player's position); each member takes
/// the nearer of the two, and when both take the same one the member that is farther from it (the
/// second one on a tie) takes the other.
///
/// `player_yaw` is the player's facing in the app's space (0 = +Z): the app draws the model with
/// exactly that turn about its origin, so the matrix's local axes are the app's local axes (local +Z
/// is the facing). That the game's +0x6b0 matches the app's draw transform up to the half turn of the
/// space is [inferred] from the app's models facing their direction of travel; the points therefore
/// lie ahead of the player, `atan(side / ahead)` off the facing to either side.
pub fn follow_points(player: Vec3, player_yaw: f32, members: [Option<(Vec3, f32)>; 2], c: &Consts) -> [Option<Vec3>; 2] {
    let (s, co) = player_yaw.sin_cos();
    let local = |k: usize, d: f32| {
        let (x, z) = (c.side[k] * d, c.ahead * d);
        player + Vec3::new(x * co + z * s, 0.0, -x * s + z * co)
    };
    let mut out = [None; 2];
    let mut pick = [usize::MAX; 2];
    let mut best = [0.0f32; 2];
    for (i, m) in members.iter().enumerate() {
        let Some((pos, d)) = *m else { continue };
        for k in 0..2 {
            let p = local(k, d);
            let d2 = (p - pos).length_squared();
            if pick[i] == usize::MAX || d2 < best[i] {
                best[i] = d2;
                pick[i] = k;
                out[i] = Some(p);
            }
        }
    }
    if let (Some((_, d0)), Some((_, d1))) = (members[0], members[1])
        && pick[0] == pick[1]
    {
        if best[0] <= best[1] {
            out[1] = Some(local(pick[1] ^ 1, d1));
        } else {
            out[0] = Some(local(pick[0] ^ 1, d0));
        }
    }
    out
}

// ---------------------------------------------------------------------------------------------------
// One member: follow, steer, warp

/// The member's state the follow and the steer keep.
#[derive(Clone, Debug, PartialEq)]
pub struct Follow {
    /// +0xb78: speed mode 0..2.
    pub mode: usize,
    /// +0xb7c bit 0x10: standing and turning to face the player.
    pub turning: bool,
    /// +0xc70 / +0xcd0: the steer's ring of wanted velocities (xyz = direction times speed, w = the
    /// speed squared) and its write index (-1 before the first).
    pub ring: [Vec4; STEER_LEN],
    pub ring_at: i32,
}

impl Default for Follow {
    fn default() -> Self {
        Follow { mode: 0, turning: false, ring: [Vec4::ZERO; STEER_LEN], ring_at: -1 }
    }
}

/// What the follow step reads (FUN_0010df08's relation to the player at +0xb80, the member's facing
/// +0x550, its walk and run speeds +0x110 / +0x114).
#[derive(Clone, Copy, Debug)]
pub struct FollowIn {
    /// The follow point minus the member's position.
    pub to_point: Vec3,
    /// +0xb90 / +0xb9c: the unit direction and the 3D distance to the player.
    pub to_player: Vec3,
    pub dist: f32,
    /// +0xba0: the unit direction to the player on the ground plane.
    pub ground_dir: Vec3,
    /// +0x550: the member's facing.
    pub facing: Vec3,
    pub walk: f32,
    pub run: f32,
}

/// FUN_0010e5b8: the speed of a mode (0 stands, 1 walks, 2 runs).
pub fn mode_speed(mode: usize, walk: f32, run: f32) -> f32 {
    match mode {
        1 => walk,
        2 => run,
        _ => 0.0,
    }
}

/// The speed-mode hysteresis of the follow step: above the mode's speed-up distance one mode faster
/// (at most 2), else below its slow-down distance one mode slower (at least 0).
pub fn next_mode(mode: usize, dist: f32, t: &[(f32, f32); MODES]) -> usize {
    let (down, up) = t[mode.min(MODES - 1)];
    if up < dist {
        (mode + 1).min(MODES - 1)
    } else if dist < down {
        mode.saturating_sub(1)
    } else {
        mode
    }
}

/// FUN_0010e140, motion state 0x1f/0x20 (the friend's free state, FUN_00111070 puts it there), with
/// the follow vector (+0xb80): the direction to the follow point (to the player himself while the
/// turning bit is set), the speed of the mode the distance to the player asks for (field row);
/// standing, a member not facing the player turns toward him at a fraction of the walk speed.
/// Returns the wanted direction (+0xc00) and speed (+0xc0c).
#[cfg(test)]
pub fn follow_step(st: &mut Follow, i: &FollowIn, t: &Thresholds, c: &Consts) -> (Vec3, f32) {
    follow_step_row(st, i, t, ROW_FIELD, c)
}

/// `follow_step` with the thresholds row FUN_001140c0 / FUN_00114078 pick: `ROW_FIELD` out of battle,
/// `ROW_BATTLE_PLAYER` in battle (the player relation).
pub fn follow_step_row(st: &mut Follow, i: &FollowIn, t: &Thresholds, row: usize, c: &Consts) -> (Vec3, f32) {
    let mut dir = if st.turning { i.to_player } else { i.to_point.normalize_or_zero() };
    st.mode = next_mode(st.mode, i.dist, &t[row.min(2)]);
    let mut speed = mode_speed(st.mode, i.walk, i.run);
    if speed != 0.0 {
        st.turning = false;
    } else if !st.turning {
        if i.facing.dot(i.ground_dir) <= c.turn_on {
            st.turning = true;
        }
    } else {
        dir = (dir + i.facing * c.turn_mix).normalize_or_zero();
        speed = i.walk * c.turn_speed;
        if i.facing.dot(i.ground_dir) > c.turn_off {
            st.turning = false;
        }
    }
    (dir, speed)
}

/// FUN_00110468: the wanted velocity is averaged with the last six (a turning member stores zero),
/// mixed with this frame's, then the speed snaps to stand / walk / run. Returns the direction (not
/// normalised: the game keeps the mixed vector) and the speed.
pub fn steer(st: &mut Follow, dir: Vec3, speed: f32, walk: f32, run: f32, c: &Consts) -> (Vec3, f32) {
    let v = if st.turning { Vec4::ZERO } else { (dir * speed).extend(speed * speed) };
    if st.ring_at < 0 {
        st.ring = [v; STEER_LEN];
        st.ring_at = STEER_LEN as i32 - 1;
    }
    st.ring[st.ring_at as usize] = v;
    st.ring_at += 1;
    if st.ring_at >= STEER_LEN as i32 {
        st.ring_at = 0;
    }
    let sum: Vec4 = st.ring.iter().copied().sum();
    let len = sum.truncate().length();
    let avg = if len > 0.0 { (sum.truncate() / len).extend(len / c.steer_div) } else { Vec4::ZERO };
    let mixed = (dir.extend(speed) + avg) * c.steer_mix;
    let mut s = mixed.w;
    if s < run {
        s = if s < walk * c.steer_mix {
            0.0
        } else if s < (walk + run) * c.steer_mix {
            walk
        } else {
            run
        };
    }
    if st.turning {
        s = walk;
    }
    (mixed.truncate(), s)
}

/// FUN_00110a58's first gate: no cooldown left and the far counter above 39.
pub fn warp_due(cooldown: f32, far: i32) -> bool {
    cooldown <= 0.0 && far >= WARP_FAR_MIN
}

/// FUN_00110a58: the trail indices a warp tries, in order: from `(warps + 0x1c + r) & 31` (`r` a
/// random number below 4, FUN_0010f420), eleven tries three apart modulo 32.
pub fn warp_order(warps: i32, r: u32) -> impl Iterator<Item = usize> {
    let start = (warps + WARP_BASE + r as i32) as usize & (TRAIL_LEN - 1);
    (0..WARP_TRIES).map(move |k| (start + k * WARP_STEP) % TRAIL_LEN)
}

/// FUN_001108c8, the direction part (the screen and collision parts are the caller's): `d` is the
/// trail point minus the player. Its height must not lie on the player's side of the member's height
/// difference (`d.y * dy <= 0`, `dy` the player's height minus the member's); then, with the member's
/// ground direction to the player, the point must not lie past the player: dot <= 0 when the height
/// difference is below `warp_dy`, dot <= `warp_dot` otherwise.
pub fn warp_point_ok(point: Vec3, player: Vec3, member: Vec3, ground_dir: Vec3, c: &Consts) -> bool {
    let d = point - player;
    let dy = player.y - member.y;
    if d.y * dy > 0.0 {
        return false;
    }
    let dot = ground_dir.dot(d.with_y(0.0));
    if dy.abs() < c.warp_dy { dot <= 0.0 } else { dot <= c.warp_dot }
}

/// The numbers host 1.247 (`random_free_point`, FUN_001726d8 -> FUN_001a5ea8) reads, from the executable.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FreePointConsts {
    /// One bucket of the start angle and the angle step between tries (0x37b5a8, 0x37b5b4).
    pub bucket_angle: f32,
    pub turn_angle: f32,
    /// The generator's state to 0..1 (0x37b5ac).
    pub rand_scale: f32,
    /// The radius step after eight tries, and how far above the floor found the line test ends (0x37b5b0).
    pub step: f32,
    /// The line test reaches this much beyond the candidate (0x37b5b8).
    pub reach: f32,
    /// The floor cast FUN_001a5c30 runs from `cast_up` above the candidate to `cast_down` below it
    /// (0x37b5bc, 0x37b5c0).
    pub cast_down: f32,
    pub cast_up: f32,
}

impl FreePointConsts {
    pub fn from_exe(elf: &[u8]) -> Option<FreePointConsts> {
        let f = |va| crate::reaction_rules::exe_f32(elf, va);
        Some(FreePointConsts { bucket_angle: f(0x37b5a8)?, rand_scale: f(0x37b5ac)?, step: f(0x37b5b0)?, turn_angle: f(0x37b5b4)?, reach: f(0x37b5b8)?, cast_down: f(0x37b5bc)?, cast_up: f(0x37b5c0)? })
    }
}

/// Host 1.247 (FUN_001726d8 -> FUN_001a5ea8 with collision mask 1): a free floor point around `centre`
/// (Goofy's `turbo` dashes to it). FUN_001a6068 draws the radius between `r_min` and `r_max` and the
/// start angle (a bucket of eighths taken by the counter 0x35e9b0 plus a random part of one) from the
/// game's second generator, as host 1.248 (`magic_rules::scatter_place`). FUN_001a6198: one eighth further
/// each try, the candidate `centre + (sin a, 0, cos a) x radius` (the game's axes) is cast for a floor from
/// `cast_up` above to `cast_down` below it (FUN_001a5c30 -> FUN_001d1648: `ground`, the first hit from the
/// top); with a floor, the line from the centre to the point `reach` further out and `step` above that
/// floor must be clear (FUN_001a5bf8: `clear`) and the floor point is the answer; after eight tries the
/// radius shrinks by `step`; below `r_min` the floor under the centre (else the centre) is the answer.
/// Game units in the app's axes.
pub fn free_point(
    centre: Vec3,
    r_min: f32,
    r_max: f32,
    st: &mut crate::magic_rules::ScatterState,
    c: &FreePointConsts,
    mut ground: impl FnMut(Vec3, Vec3) -> Option<Vec3>,
    mut clear: impl FnMut(Vec3, Vec3) -> bool,
) -> Vec3 {
    let mut rand = || {
        st.seed = st.seed.wrapping_mul(0x10dcd);
        st.seed as f32 * c.rand_scale
    };
    let mut r = r_min + rand() * (r_max - r_min);
    let t = st.turn;
    let bucket = ((t & 4) >> 2) | (t << 1);
    let start = bucket as f32 * c.bucket_angle + rand() * c.bucket_angle;
    st.turn = (t + 1).rem_euclid(8);
    let mut cast = |p: Vec3| ground(p + Vec3::Y * c.cast_up, p - Vec3::Y * (c.cast_down + c.cast_up));
    if r_min <= r {
        let mut k = 0;
        loop {
            k += 1;
            // FUN_0017bbc0 wraps the angle into -pi..pi.
            let x = start + k as f32 * c.turn_angle;
            let (pi, tau) = (std::f32::consts::PI, std::f32::consts::TAU);
            let a = if x > 0.0 { (x + pi) % tau - pi } else { pi - (pi - x) % tau };
            let dir = Vec3::new(a.sin(), 0.0, -a.cos());
            if let Some(hit) = cast(centre + dir * r) {
                let q = (centre + dir * (r + c.reach)).with_y(hit.y + c.step);
                if clear(centre, q) {
                    return hit;
                }
            }
            if k >= 8 {
                k = 0;
                r -= c.step;
            }
            if r < r_min {
                break;
            }
        }
    }
    cast(centre).unwrap_or(centre)
}

/// The ability item whose bit is 131 (word 4 mask 8 of a status block's ability block, `+0x1dc & 8`), the one flag
/// `FUN_0017cee8` reads. Hyper Healing: `FUN_001c0f40` multiplies the block's `+0x58` by `sstm +0x154` for the same
/// ability (id 0x1a3 = 419), which `revive_time` already takes; the bit comes from the disc's item row (test
/// `hyper_healing_is_ability_419_bit_131`).
pub const ABILITY_HYPER_HEALING: u16 = 419;
/// `DAT_0037a560`: the revive scale of a member whose ability block has bit 131 (1.0 in the executable).
pub const REVIVE_FULL_SCALE_VA: u32 = 0x37a560;

/// `DAT_0037a560`, read from the executable.
pub fn revive_full_scale(elf: &[u8]) -> Option<f32> {
    exe_bytes(elf, REVIVE_FULL_SCALE_VA, 4).map(|b| f32::from_bits(u32::from_le_bytes([b[0], b[1], b[2], b[3]])))
}

/// The factor `FUN_0017cee8` scales a heal on a KO'd member by: `DAT_0037a560` when the member's ability block has
/// bit 131 (`FUN_001768a8` = status block `+0x1cc`, word at `+0x10`, mask 8), else `sstm +0x54`. `abilities` is the
/// member's record list (ability id, equipped); the block is built from the equipped ones (`FUN_0019f920`).
pub fn revive_scale(abilities: &[(u16, bool)], full_scale: f32, sstm_scale: f32) -> f32 {
    if abilities.contains(&(ABILITY_HYPER_HEALING, true)) { full_scale } else { sstm_scale }
}

#[cfg(test)]
mod tests {
    /// A member's block starts from his record (`FUN_0019f920`): `min(maximum, current)`; a stored 0 HP is a gap and
    /// starts at the maximum.
    #[test]
    fn spawn_vitals_come_from_the_record() {
        assert_eq!(spawn_vitals(Some((12, 34)), 30, 50), (12, 34), "a member entering the field with his stored HP and MP");
        assert_eq!(spawn_vitals(Some((90, 90)), 30, 50), (30, 50), "held to the maxima");
        assert_eq!(spawn_vitals(Some((0, 0)), 30, 50), (30, 0), "HP 0 starts at the maximum, MP is kept");
        assert_eq!(spawn_vitals(None, 30, 50), (30, 50));
    }

    /// Auto Healing's pieces: the HP method clamps to [min, max], the timer runs from 0 and refills with the
    /// interval, and only the slots 1..3 members outside the field are visited.
    #[test]
    fn auto_healing_timer_clamp_and_reserve_set() {
        assert_eq!((clamped_add(10, 1, 0, 100), clamped_add(100, 1, 0, 100), clamped_add(0, 1, 0, 100), clamped_add(5, -9, 0, 100)), (11, 100, 1, 0));
        let mut timer = 0.0;
        let mut fired = 0;
        for _ in 0..250 {
            fired += auto_heal_due(&mut timer, 1.0, 100.0) as u32;
        }
        assert_eq!(fired, 3, "the first tick at once, then every 100 frames (ticks 1, 101, 201)");
        // Slots: Sora 0, Donald in 1, the partner in 2, Goofy in the reserve slot 3; the memt words 8.. name the objects.
        let mut merged = [0u16; 24];
        merged[8 + 1] = 101;
        merged[8 + 2] = 102;
        merged[8 + 3] = 103;
        let member_of = |o: u16| Some(o - 99);
        assert_eq!(reserve_members([0, 1, 2, 3], &merged, member_of, &[2, 3]), vec![4], "Donald and the partner stand; the byte 3 member is reserve");
        assert_eq!(reserve_members([0, 1, 2, 3], &merged, member_of, &[]), vec![2, 3, 4], "nobody standing: slots 1 to 3 all heal");
        assert_eq!(reserve_members([0, 1, EMPTY, EMPTY], &merged, member_of, &[2]), Vec::<u16>::new(), "empty slots visit nobody");
    }

    /// Auto Change's swap (FUN_001a7558): the KO'd member goes to the reserve slot and the reserve member
    /// takes its place; a flagged, player, absent or reserve-less case does not fire.
    #[test]
    fn auto_change_swaps_the_reserve_in() {
        // Preset 2: the partner (index 3) in slot 1, Goofy (2) in slot 2, Donald (1) in the reserve slot.
        assert_eq!(auto_change_swap([0, 3, 2, 1], 7), Some([0, 1, 2, 3]), "a partner (NeoStatus 7) is index 3");
        assert_eq!(auto_change_swap([0, 3, 2, 1], 3), Some([0, 3, 1, 2]), "NeoStatus 3 is Goofy, index 2");
        assert_eq!(auto_change_swap([0, 1, 2, 3], 3), Some([0, 1, 3, 2]), "Goofy out, the partner in");
        assert_eq!(auto_change_swap([0, 1, 2, EMPTY], 3), None, "no reserve member");
        assert_eq!(auto_change_swap([0, 1, 0x82, 3], 3), None, "the 0x80 flag keeps the member");
        assert_eq!(auto_change_swap([0, 1, 2, 3], 1), None, "NeoStatus 1 is the player's slot 0 member");
        assert_eq!(auto_change_swap([0, 1, EMPTY, 3], 3), None, "Goofy is in no slot");
        assert_eq!((member_index(1), member_index(14), member_index(2), member_index(3), member_index(5)), (0, 0, 1, 2, 3));
    }

    use super::*;

    fn consts() -> Consts {
        let t = [[(0.0, 200.0), (160.0, 240.0), (200.0, 1000.0)]; 3];
        Consts {
            classes: vec![t; CLASSES],
            trail_step2: 2025.0,
            far_dy: 400.0,
            side: [0.86, -0.86],
            ahead: 0.51,
            radius_k: 0.5,
            follow_min: 1.0,
            steer_div: 6.0,
            steer_mix: 0.5,
            turn_on: 0.5,
            turn_mix: 0.8,
            turn_speed: 0.2,
            turn_off: 0.97,
            warp_dy: 1000.0,
            warp_dot: -0.5,
            warp_fade: 30.0,
            warp_cooldown: 480.0,
        }
    }

    /// The speed mode climbs one step per frame above the speed-up distance and drops one below the
    /// slow-down distance; in between it holds (hysteresis).
    #[test]
    fn party_speed_mode_hysteresis() {
        let t = consts().classes[0][ROW_FIELD];
        assert_eq!(next_mode(0, 150.0, &t), 0);
        assert_eq!(next_mode(0, 201.0, &t), 1);
        assert_eq!(next_mode(1, 201.0, &t), 1);
        assert_eq!(next_mode(1, 241.0, &t), 2);
        assert_eq!(next_mode(2, 999.0, &t), 2);
        assert_eq!(next_mode(2, 199.0, &t), 1);
        assert_eq!(next_mode(1, 170.0, &t), 1);
        assert_eq!(next_mode(1, 159.0, &t), 0);
    }

    /// The trail fills on the first frame, then adds a point only 45 units (sqrt 2025) from the newest.
    #[test]
    fn party_trail_spacing() {
        let c = consts();
        let mut tr = Trail::default();
        assert!(tr.push(Vec3::ZERO, c.trail_step2));
        assert_eq!(tr.head, 0);
        assert!(!tr.moved);
        assert!(!tr.push(Vec3::new(44.0, 0.0, 0.0), c.trail_step2));
        assert!(tr.push(Vec3::new(45.0, 0.0, 0.0), c.trail_step2));
        assert_eq!((tr.head, tr.moved), (1, true));
        for k in 2..40 {
            tr.push(Vec3::new(45.0 * k as f32, 0.0, 0.0), c.trail_step2);
        }
        assert_eq!(tr.head, 39 % 32);
    }

    /// The steer snaps the mixed speed to stand, walk or run, and a turning member walks.
    #[test]
    fn party_steer_snaps_speed() {
        let c = consts();
        let mut st = Follow::default();
        let (_, s) = steer(&mut st, Vec3::Z, 8.0, 2.0, 8.0, &c);
        assert_eq!(s, 8.0);
        let mut st = Follow::default();
        let (_, s) = steer(&mut st, Vec3::Z, 0.0, 2.0, 8.0, &c);
        assert_eq!(s, 0.0);
        // From running, a stop decays over the ring: (0 + avg) / 2 with avg falling as zeros come in.
        let mut st = Follow::default();
        steer(&mut st, Vec3::Z, 8.0, 2.0, 8.0, &c);
        let speeds: Vec<f32> = (0..6).map(|_| steer(&mut st, Vec3::Z, 0.0, 2.0, 8.0, &c).1).collect();
        assert_eq!(speeds[0], 2.0, "{speeds:?}");
        assert_eq!(*speeds.last().unwrap(), 0.0, "{speeds:?}");
        st.turning = true;
        assert_eq!(steer(&mut st, Vec3::Z, 0.0, 2.0, 8.0, &c).1, 2.0);
    }

    /// The follow points lie ahead of the player to both sides; each member takes the nearer, and two
    /// members never share one.
    #[test]
    fn party_follow_points_split() {
        let c = consts();
        let d = 160.0;
        let left_back = Vec3::new(100.0, 0.0, -50.0);
        let right_back = Vec3::new(-100.0, 0.0, -50.0);
        let p = follow_points(Vec3::ZERO, 0.0, [Some((left_back, d)), Some((right_back, d))], &c);
        assert!((p[0].unwrap() - Vec3::new(0.86 * d, 0.0, 0.51 * d)).length() < 1e-3);
        assert!((p[1].unwrap() - Vec3::new(-0.86 * d, 0.0, 0.51 * d)).length() < 1e-3);
        // Both on the same side: the nearer keeps it, the other gets the other point.
        let p = follow_points(Vec3::ZERO, 0.0, [Some((left_back, d)), Some((left_back * 2.0, d))], &c);
        assert!(p[0].unwrap().x > 0.0 && p[1].unwrap().x < 0.0);
        // Turned a quarter: ahead is +X.
        let p = follow_points(Vec3::ZERO, std::f32::consts::FRAC_PI_2, [Some((Vec3::ZERO, d)), None], &c);
        assert!(p[0].unwrap().x > 0.0 && p[1].is_none());
    }

    /// Standing members facing away from the player start turning (and walk slowly toward the point);
    /// the turn ends once they face him.
    #[test]
    fn party_standing_member_turns_to_the_player() {
        let c = consts();
        let t = c.classes[0];
        let mut st = Follow::default();
        let i = FollowIn { to_point: Vec3::Z * 50.0, to_player: Vec3::Z, dist: 100.0, ground_dir: Vec3::Z, facing: -Vec3::Z, walk: 2.0, run: 8.0 };
        let (_, s) = follow_step(&mut st, &i, &t, &c);
        assert!(st.turning && s == 0.0);
        let (_, s) = follow_step(&mut st, &i, &t, &c);
        assert!((s - 0.4).abs() < 1e-6);
        let i = FollowIn { facing: Vec3::Z, ..i };
        follow_step(&mut st, &i, &t, &c);
        assert!(!st.turning);
    }

    /// The warp tries eleven trail points three apart from `warps + 0x1c + r`, and only points not past
    /// the player as the member sees him.
    #[test]
    fn party_warp_order_and_point_test() {
        let o: Vec<usize> = warp_order(0, 0).collect();
        assert_eq!(o.len(), 11);
        assert_eq!(&o[..4], &[28, 31, 2, 5]);
        assert!(!warp_due(0.0, 39) && warp_due(0.0, 40) && !warp_due(1.0, 100));
        let c = consts();
        let (player, member) = (Vec3::ZERO, Vec3::new(0.0, 0.0, -1000.0));
        let gdir = Vec3::Z;
        assert!(warp_point_ok(Vec3::new(0.0, 0.0, -100.0), player, member, gdir, &c));
        assert!(!warp_point_ok(Vec3::new(0.0, 0.0, 100.0), player, member, gdir, &c));
    }

    // ---- Oracle checks (PLAYBOOK R2): the game's own routines in kh2-ee on the user's executable.

    const STACK: u32 = 0x01ff_0000;
    const OBJ: u32 = 0x0182_0000;
    const CTRL: u32 = 0x0183_0000;
    const AI: u32 = 0x0184_0000;
    const RING: u32 = 0x0180_0000;
    const POS: u32 = 0x0181_0000;
    /// The follow points (FUN_00111670 writes them, FUN_0010e140 reads `+ position x 16`).
    const FOLLOW_POINTS: u32 = 0x0034_e920;
    /// The battle flag FUN_001903d8 reads.
    const BATTLE: u32 = 0x01c8_a7d8;

    fn oracle() -> Option<(kh2_ee::Ee, Consts)> {
        let iso = crate::assets::iso_path().ok().filter(|p| p.exists())?;
        let elf = crate::assets::Disc::open(&iso).read_root("SLUS_210.05")?;
        let mut ee = kh2_ee::Ee::new();
        ee.load_elf(&elf)?;
        Some((ee, Consts::from_exe(&elf)?))
    }

    fn clear(ee: &mut kh2_ee::Ee, at: u32, len: u32) {
        for o in (0..len).step_by(4) {
            ee.w32(at + o, 0);
        }
    }

    fn wv(ee: &mut kh2_ee::Ee, at: u32, v: Vec4) {
        for (i, x) in v.to_array().into_iter().enumerate() {
            ee.wf32(at + 4 * i as u32, x);
        }
    }

    fn rv(ee: &mut kh2_ee::Ee, at: u32) -> Vec4 {
        Vec4::new(ee.f32_at(at), ee.f32_at(at + 4), ee.f32_at(at + 8), ee.f32_at(at + 12))
    }

    struct Lcg(u32);
    impl Lcg {
        fn f(&mut self) -> f32 {
            self.0 = self.0.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            (self.0 >> 8) as f32 / (1u32 << 24) as f32
        }
        fn unit(&mut self) -> Vec3 {
            let a = self.f() * std::f32::consts::TAU;
            Vec3::new(a.sin(), 0.0, a.cos())
        }
    }

    fn close(a: Vec3, b: Vec3) -> bool {
        (a - b).length() <= 1e-4 * (1.0 + a.length().max(b.length()))
    }

    /// FUN_0010b270 (the player's trail) against `Trail::push` over a random walk with jumps: the
    /// return byte, the head index, the newest point and the header's byte 0 agree every frame.
    #[test]
    fn party_trail_matches_fun_0010b270() {
        let Some((mut ee, c)) = oracle() else { return };
        clear(&mut ee, RING, 0x220);
        ee.w32(RING + 4, u32::MAX);
        let mut tr = Trail::default();
        let mut rng = Lcg(7);
        let mut p = Vec3::new(100.0, 0.0, -50.0);
        for step in 0..400 {
            p += rng.unit() * if rng.f() < 0.05 { 300.0 } else { rng.f() * 30.0 } + Vec3::Y * (rng.f() - 0.5) * 4.0;
            wv(&mut ee, POS, p.extend(1.0));
            let game = ee.call(0x10b270, &[RING as u64, POS as u64], &[], STACK).expect("FUN_0010b270") & 0xff;
            let port = tr.push(p, c.trail_step2);
            assert_eq!(game, port as u64, "step {step}: point added");
            let head = ee.r32(RING + 4) as i32;
            assert_eq!(head, tr.head, "step {step}: head");
            assert_eq!(ee.r32(RING) & 0xff != 0, tr.moved, "step {step}: header byte 0");
            assert!(close(rv(&mut ee, RING + 16 * (1 + head as u32)).truncate(), tr.points[head as usize]), "step {step}: newest point");
        }
        eprintln!("FUN_0010b270 agrees with Trail::push over 400 frames (head {})", tr.head);
    }

    /// FUN_00110468 (the steer) against `steer` over random wanted velocities, speeds (stand, walk, run
    /// and in between) and turning frames: the mixed direction, the snapped speed and the ring index.
    #[test]
    fn party_steer_matches_fun_00110468() {
        let Some((mut ee, c)) = oracle() else { return };
        let (walk, run) = (2.0, 12.0);
        clear(&mut ee, OBJ, 0xd00);
        clear(&mut ee, CTRL, 0x10);
        ee.w32(OBJ + 0xc, CTRL);
        ee.w32(CTRL + 4, 0x1f);
        ee.wf32(OBJ + 0x110, walk);
        ee.wf32(OBJ + 0x114, run);
        ee.w32(OBJ + 0xcd0, u32::MAX);
        let mut st = Follow::default();
        let mut rng = Lcg(11);
        for step in 0..300 {
            let dir = rng.unit();
            let speed = match (rng.f() * 5.0) as u32 {
                0 => 0.0,
                1 => walk,
                2 => run,
                _ => rng.f() * run,
            };
            st.turning = rng.f() < 0.1;
            wv(&mut ee, OBJ + 0xc00, dir.extend(speed));
            ee.w32(OBJ + 0xb7c, if st.turning { 0x10 } else { 0 });
            ee.call(0x110468, &[OBJ as u64], &[], STACK).expect("FUN_00110468");
            let game = rv(&mut ee, OBJ + 0xc00);
            let (d, s) = steer(&mut st, dir, speed, walk, run, &c);
            assert!(close(game.truncate(), d), "step {step}: direction {game:?} vs {d:?}");
            assert!((game.w - s).abs() <= 1e-4, "step {step}: speed {} vs {s}", game.w);
            assert_eq!(ee.r32(OBJ + 0xcd0) as i32, st.ring_at, "step {step}: ring index");
        }
        eprintln!("FUN_00110468 agrees with steer over 300 frames");
    }

    /// FUN_0010e140 in the friend's motion state 0x1f with the follow vector, out of battle, against
    /// `follow_step` with the executable's thresholds of class 0 (Donald): the speed mode, the turning
    /// bit, the wanted direction and speed, over random positions, facings and starting modes.
    #[test]
    fn party_follow_step_matches_fun_0010e140() {
        let Some((mut ee, c)) = oracle() else { return };
        let (walk, run) = (2.0, 12.0);
        let class = 0u8;
        let block = 0x34ea00 + ee.r32(0x34ea00 + 4 * class as u32);
        let t = c.thresholds(class);
        let mut rng = Lcg(23);
        for step in 0..400 {
            clear(&mut ee, OBJ, 0xd00);
            clear(&mut ee, CTRL, 0x10);
            clear(&mut ee, AI, 0x20);
            ee.w32(OBJ + 0xc, CTRL);
            ee.w32(CTRL + 4, 0x1f);
            ee.w32(AI + 8, block);
            ee.w32(OBJ + 0xb60, AI);
            ee.w32(OBJ + 0xb70, 1);
            ee.w32(BATTLE, 0);
            ee.wf32(OBJ + 0x110, walk);
            ee.wf32(OBJ + 0x114, run);
            let mode = (rng.f() * 3.0) as usize;
            let turning = rng.f() < 0.3;
            ee.w32(OBJ + 0xb78, mode as u32);
            ee.w32(OBJ + 0xb7c, if turning { 0x10 } else { 0 });
            let pos = Vec3::new(rng.f() * 400.0, 0.0, rng.f() * 400.0);
            let player = pos + rng.unit() * rng.f() * 600.0 + Vec3::Y * (rng.f() - 0.5) * 50.0;
            let point = player + rng.unit() * 115.0;
            let facing = rng.unit();
            wv(&mut ee, OBJ + 0x540, pos.extend(1.0));
            wv(&mut ee, FOLLOW_POINTS + 16, point.extend(1.0));
            wv(&mut ee, OBJ + 0x550, facing.extend(0.0));
            let rel = player - pos;
            let gdir = rel.with_y(0.0).normalize_or_zero();
            ee.wf32(OBJ + 0xb80, rel.y);
            wv(&mut ee, OBJ + 0xb90, rel.normalize_or_zero().extend(rel.length()));
            wv(&mut ee, OBJ + 0xba0, gdir.extend(rel.with_y(0.0).length()));
            let r = ee.call(0x10e140, &[OBJ as u64, OBJ as u64 + 0xb80], &[], STACK).expect("FUN_0010e140");
            assert_eq!(r & 0xffff_ffff, 1);
            let mut st = Follow { mode, turning, ..Follow::default() };
            let i = FollowIn { to_point: point - pos, to_player: rel.normalize_or_zero(), dist: rel.length(), ground_dir: gdir, facing, walk, run };
            let (d, s) = follow_step(&mut st, &i, &t, &c);
            let game = rv(&mut ee, OBJ + 0xc00);
            assert_eq!(ee.r32(OBJ + 0xb78) as usize, st.mode, "step {step}: mode");
            assert_eq!(ee.r32(OBJ + 0xb7c) & 0x10 != 0, st.turning, "step {step}: turning bit");
            assert!(
                close(game.truncate(), d),
                "step {step}: direction {game:?} vs {d:?}; mode {mode} turning {turning} pos {pos} player {player} point {point} facing {facing} to point {:?}",
                (point - pos).normalize()
            );
            assert!((game.w - s).abs() <= 1e-4, "step {step}: speed {} vs {s}", game.w);
        }
        eprintln!("FUN_0010e140 (state 0x1f, field) agrees with follow_step over 400 cases");
    }

    /// FUN_00111670 (the follow points) against `follow_points`: the player's model matrix (+0x6b0)
    /// is the turn the app draws him with (rows: local X, Y, Z, position), the two members stand at
    /// random places with the body radius of the disc's Donald; the points written to 0x34e930 and
    /// 0x34e940 agree, the shared-point rule included.
    #[test]
    fn party_follow_points_match_fun_00111670() {
        let Some((mut ee, c)) = oracle() else { return };
        const PLAYER: u32 = 0x0185_0000;
        const MEMBERS: [u32; 2] = [0x0186_0000, 0x0187_0000];
        let class = 0u8;
        let block = 0x34ea00 + ee.r32(0x34ea00 + 4 * class as u32);
        let t = c.thresholds(class);
        let radius = 90.0;
        let mut rng = Lcg(31);
        let mut shared = 0;
        for step in 0..300 {
            for o in [PLAYER, MEMBERS[0], MEMBERS[1]] {
                clear(&mut ee, o, 0xd00);
            }
            clear(&mut ee, AI, 0x20);
            ee.w32(AI + 8, block);
            ee.w32(BATTLE, 0);
            ee.w32(0x34e908, PLAYER);
            ee.w32(0x34e90c, MEMBERS[0]);
            ee.w32(0x34e910, MEMBERS[1]);
            let player = Vec3::new(rng.f() * 1000.0, 0.0, rng.f() * 1000.0);
            let yaw = (rng.f() - 0.5) * std::f32::consts::TAU;
            let (s, co) = yaw.sin_cos();
            wv(&mut ee, PLAYER + 0x540, player.extend(1.0));
            wv(&mut ee, PLAYER + 0x6b0, Vec4::new(co, 0.0, -s, 0.0));
            wv(&mut ee, PLAYER + 0x6c0, Vec4::new(0.0, 1.0, 0.0, 0.0));
            wv(&mut ee, PLAYER + 0x6d0, Vec4::new(s, 0.0, co, 0.0));
            // The routine takes the matrix's three axis rows from +0x6b0 and the position row from +0x70
            // (the position FUN_0016b960 returns for a linked object).
            wv(&mut ee, PLAYER + 0x70, player.extend(1.0));
            let mut pos = [Vec3::ZERO; 2];
            for (k, &m) in MEMBERS.iter().enumerate() {
                // Every third case both members stand on the same side, so they want the same point.
                pos[k] = if step % 3 == 0 && k == 1 { pos[0] + rng.unit() * 40.0 } else { player + rng.unit() * rng.f() * 500.0 };
                wv(&mut ee, m + 0x540, pos[k].extend(1.0));
                ee.w32(m + 0xb60, AI);
                ee.wf32(m + 0x5b8, radius);
            }
            ee.call(0x111670, &[], &[], STACK).expect("FUN_00111670");
            let d = follow_distance(&t, radius, &c);
            let port = follow_points(player, yaw, [Some((pos[0], d)), Some((pos[1], d))], &c);
            for (k, p) in port.iter().enumerate() {
                let game = rv(&mut ee, FOLLOW_POINTS + 16 * (1 + k as u32)).truncate();
                assert!(close(game, p.unwrap()), "step {step} member {k}: {game} vs {p:?}");
            }
            let local = |p: Vec3| ((p - player).x * co - (p - player).z * s).signum();
            if local(pos[0]) == local(pos[1]) {
                shared += 1;
            }
        }
        eprintln!("FUN_00111670 agrees with follow_points over 300 cases ({shared} with both members on one side)");
    }

    /// FUN_001a7330 on the empty party with preset 1 gives Donald and Goofy in slots 1 and 2, and the
    /// slot bytes name merged words 9 and 10.
    #[test]
    fn party_default_preset_names_donald_and_goofy() {
        let s = apply_preset([0, EMPTY, EMPTY, EMPTY], [0, 1, 2, EMPTY]);
        assert_eq!(s, [0, 1, 2, EMPTY]);
        let mut rec = [0u16; 24];
        rec[9] = 0x5a;
        rec[10] = 0x5b;
        assert_eq!(slot_object(&rec, s[1]), Some(0x5a));
        assert_eq!(slot_object(&rec, s[2]), Some(0x5b));
        assert_eq!(slot_object(&rec, s[3]), None);
        assert_eq!(ai_class(2, 17), Some(0));
        assert_eq!(ai_class(3, 0x15), Some(13));
        assert_eq!(ai_class(1, 0), None);
    }

    /// WP-S8: the program that starts a mission names its preset (opcode 0xf); preset 0 `[0,-,-,-]` empties the
    /// companions' slots whatever stood there, so FRIEND_1 / FRIEND_2 name nobody. Another mission's program
    /// is not read, and a mission no program starts applies nothing.
    #[test]
    fn a_struggle_program_leaves_sora_alone() {
        let cmd = |opcode: u16, args: &[u32]| kh2_ard::Command { opcode, args: args.to_vec() };
        let progs = vec![
            kh2_ard::Program { id: 0x4c, commands: vec![cmd(0xf, &[1]), cmd(0x15, &[0xac, 0])] },
            kh2_ard::Program { id: 0xb6, commands: vec![cmd(0xf, &[0]), cmd(0x15, &[0x128, 0]), cmd(0x1d, &[1]), cmd(0, &[0x30355f62])] },
        ];
        assert_eq!(mission_presets(&progs, 0x128), vec![0]);
        assert_eq!(mission_presets(&progs, 0xac), vec![1]);
        assert!(mission_presets(&progs, 0x999).is_empty());
        let after = apply_preset([0, 1, 2, EMPTY], [0, EMPTY, EMPTY, EMPTY]);
        assert_eq!(after, [0, EMPTY, EMPTY, EMPTY]);
        let mut rec = [0u16; 24];
        rec[9] = 0x5a;
        rec[10] = 0x5b;
        assert!((1..4).all(|i| slot_object(&rec, after[i]).is_none()));
        // The next program that applies preset 1 brings Donald and Goofy back.
        assert_eq!(apply_preset(after, [0, 1, 2, EMPTY]), [0, 1, 2, EMPTY]);
    }

    /// WP-P3: a mission program applies preset 0 (Donald and Goofy go); the next area load, whose programs apply
    /// preset 1 (opcode 0xf), brings them back; an area whose programs have no 0xf leaves the bytes alone.
    #[test]
    fn party_return_after_the_mission_program() {
        let presets = [0x1010_1000u32, 0x1002_0100];
        let start = apply_preset([0, EMPTY, EMPTY, EMPTY], presets[1].to_le_bytes());
        assert_eq!(start, [0, 1, 2, EMPTY]);
        let mut rec = [0u16; 24];
        rec[9] = 0x5a;
        rec[10] = 0x5b;
        let wanted = |b: [u8; 4]| (1..4).filter(|&i| slot_object(&rec, b[i]).is_some()).count();
        let mission = apply_world_records(start, &[0], &presets);
        assert_eq!(mission, [0, EMPTY, EMPTY, EMPTY]);
        assert_eq!(wanted(mission), 0, "Sora alone");
        assert_eq!(apply_world_records(mission, &[], &presets), mission, "no 0xf: unchanged");
        assert_eq!(apply_world_records(mission, &[7], &presets), mission, "a preset the table lacks is skipped");
        let back = apply_world_records(mission, &[1], &presets);
        assert_eq!(back, [0, 1, 2, EMPTY]);
        assert_eq!(wanted(back), 2, "the members are wanted again");
        assert_eq!(apply_world_records(start, &[0, 1], &presets), [0, 1, 2, EMPTY], "in program order: the last wins");
    }

    fn toy_free() -> FreePointConsts {
        let q = std::f32::consts::FRAC_PI_4;
        FreePointConsts { bucket_angle: q, turn_angle: q, rand_scale: 1.0 / 4_294_967_296.0, step: 100.0, reach: 20.0, cast_down: 1000.0, cast_up: 200.0 }
    }

    /// Host 1.247 by hand: with a floor 30 below everywhere and a clear line the answer is that floor at a
    /// radius between the two; the floor cast spans 200 above to 1200 below the candidate; the line test
    /// ends 20 further out and 100 above the floor found. Without a clear line: eight tries per radius,
    /// shrinking by 100, then the floor under the centre; without any floor, the centre.
    #[test]
    fn party_free_point_by_hand() {
        let c = toy_free();
        let centre = Vec3::new(10.0, 50.0, -20.0);
        let mut st = crate::magic_rules::ScatterState::default();
        for _ in 0..10 {
            let p = free_point(
                centre,
                300.0,
                500.0,
                &mut st,
                &c,
                |a, b| {
                    assert!((a.y - b.y - 1400.0).abs() < 1e-3 && a.x == b.x && a.z == b.z);
                    Some(a - Vec3::Y * 230.0)
                },
                |from, to| {
                    assert_eq!(from, centre);
                    assert!((to.y - (centre.y - 30.0 + 100.0)).abs() < 1e-3);
                    true
                },
            );
            let flat = (p - centre).with_y(0.0).length();
            assert!((300.0..=500.0).contains(&flat), "{flat}");
            assert!((p.y - (centre.y - 30.0)).abs() < 1e-3);
        }
        let mut tries = 0;
        let p = free_point(centre, 300.0, 500.0, &mut st, &c, |a, _| Some(a - Vec3::Y * 230.0), |_, _| {
            tries += 1;
            false
        });
        assert_eq!(p, centre - Vec3::Y * 30.0);
        assert!(tries % 8 == 0 && (8..=24).contains(&tries), "{tries}");
        assert_eq!(free_point(centre, 300.0, 500.0, &mut st, &c, |_, _| None, |_, _| true), centre);
    }

    /// Oracle (PLAYBOOK R2): FUN_001a5ea8 (host 1.247's body, Goofy's `turbo` arguments 300 / 500) in kh2-ee
    /// against `free_point`. The collision queries are stubbed: FUN_001d1648 (the floor cast) answers a hit
    /// at the segment's start or none, FUN_001a5bf8 (the line test) clear or not. The point, the generator
    /// 0x3716a0 and the counter 0x35e9b0 after the call must agree.
    #[test]
    fn party_free_point_matches_fun_001a5ea8() {
        let Some(iso) = crate::assets::iso_path().ok().filter(|p| p.exists()) else {
            eprintln!("skipped: no disc image (set KH2_ISO)");
            return;
        };
        let elf = crate::assets::Disc::open(&iso).read_root("SLUS_210.05").expect("exe");
        let c = FreePointConsts::from_exe(&elf).expect("free point constants");
        let mut ee = kh2_ee::Ee::new();
        ee.load_elf(&elf).expect("elf");
        const OUT: u32 = 0x0180_0000;
        const CENTRE: u32 = 0x0180_0100;
        let game = Vec4::new(120.0, -35.0, 260.0, 1.0);
        wv(&mut ee, CENTRE, game);
        let centre = Vec3::new(game.x, -game.y, -game.z);
        let mut n = 0;
        for (floor, clear, seed, turn) in [(true, true, 1u32, 0i32), (true, true, 0x0123_4567, 5), (true, false, 99, 3), (false, true, 7, 7), (true, true, 0xdead_beef, 6), (false, false, 0x8000_0001, 4), (true, true, 0x51ed_270b, 2)] {
            // FUN_001d1648 (out a0, from a1, to a2): the 16 bytes of `from` into `out`, then v0 = floor.
            let stub = [0x8ca8_0000u32, 0xac88_0000, 0x8ca8_0004, 0xac88_0004, 0x8ca8_0008, 0xac88_0008, 0x8ca8_000c, 0xac88_000c, 0x03e0_0008, 0x2402_0000 | floor as u32];
            for (i, w) in stub.iter().enumerate() {
                ee.w32(0x1d1648 + 4 * i as u32, *w);
            }
            ee.w32(0x1a5bf8, 0x03e0_0008);
            ee.w32(0x1a5bfc, 0x2402_0000 | clear as u32);
            ee.w32(0x3716a0, seed);
            ee.w32(0x35e9b0, turn as u32);
            ee.call(0x1a5ea8, &[OUT as u64, CENTRE as u64, 1], &[300.0, 500.0], STACK).expect("FUN_001a5ea8");
            let got = rv(&mut ee, OUT).truncate();
            let mut st = crate::magic_rules::ScatterState { seed, turn };
            let p = free_point(centre, 300.0, 500.0, &mut st, &c, |a, _| floor.then_some(a), |_, _| clear);
            let mine = Vec3::new(p.x, -p.y, -p.z);
            eprintln!("floor {floor} clear {clear} seed {seed:#x} turn {turn}: game {got} port {mine}");
            assert!((got - mine).abs().max_element() < 0.05, "game {got} port {mine}");
            assert_eq!(ee.r32(0x3716a0), st.seed, "generator state");
            assert_eq!(ee.r32(0x35e9b0) as i32, st.turn, "bucket counter");
            n += 1;
        }
        assert_eq!(n, 7);
    }
}

/// WP-PT1: the jump record's party words.
#[cfg(test)]
mod jump_tests {
    use super::*;

    /// Word 4 names a preset of the end of `memt`; a negative word applies nothing; the result is `apply_preset`'s.
    #[test]
    fn a_record_word_four_arranges_the_party() {
        let presets = [u32::from_le_bytes([0, EMPTY, EMPTY, EMPTY]), u32::from_le_bytes([0, 1, 2, EMPTY]), u32::from_le_bytes([0, 2, 0x40 | 1, EMPTY])];
        let cur = [0, 1, 2, EMPTY];
        assert_eq!(arrange(cur, -1, &presets), None);
        assert_eq!(arrange(cur, 0, &presets), Some([0, EMPTY, EMPTY, EMPTY]), "preset 0 empties the companions");
        assert_eq!(arrange(cur, 1, &presets), Some([0, 1, 2, EMPTY]));
        // Preset 2 flags member 1 (0x40): it goes first, then the standing member 2 the preset lists.
        assert_eq!(arrange(cur, 2, &presets), Some([0, 1, 2, EMPTY]));
        assert_eq!(arrange([0, EMPTY, EMPTY, EMPTY], 2, &presets), Some([0, 1, 2, EMPTY]), "an empty party takes the flagged member first");
        assert_eq!(arrange(cur, 9, &presets), None, "no such preset");
    }

    /// `FUN_001abfe8`: code 0 does nothing; 4, 5 and 6 end the form and return; 1, 2, 0xc, 10 and the rest restore.
    #[test]
    fn the_party_reset_codes_follow_the_switch() {
        let r = |c, f, a| reset_effect(c, f, a);
        assert_eq!(r(0, true, true), ResetEffect::default());
        assert_eq!(r(1, false, false), ResetEffect { end_form: false, restore: true, lock_drive_command: false });
        assert_eq!(r(1, true, false), ResetEffect { end_form: true, restore: true, lock_drive_command: false });
        for c in [2, 0xc, 10] {
            assert_eq!(r(c, false, false), ResetEffect { end_form: true, restore: true, lock_drive_command: false }, "code {c}");
        }
        assert_eq!(r(4, false, false), ResetEffect::default(), "4 returns when no form is active");
        assert_eq!(r(4, true, false), ResetEffect { end_form: true, restore: false, lock_drive_command: false });
        assert_eq!(r(5, false, false), ResetEffect { end_form: false, restore: false, lock_drive_command: true }, "5 locks the command even with no form");
        assert_eq!(r(6, false, false), ResetEffect { end_form: true, restore: false, lock_drive_command: false });
        assert_eq!(r(0xb, true, false), ResetEffect::default());
        assert_eq!(r(0xb, false, true), ResetEffect { end_form: true, restore: false, lock_drive_command: false });
        for c in [-1, 3, 7, 8, 9, 0xd, 99] {
            assert_eq!(r(c, true, false), ResetEffect { end_form: false, restore: true, lock_drive_command: false }, "code {c}");
        }
    }

    /// Page 0x15: Donald and Goofy leave (Struggle preset), the leader stays; a member in both lists is not named.
    #[test]
    fn the_notice_lists_who_came_in_and_who_went_out() {
        let slot = |member, locked| NoticeSlot { member, locked };
        let sora = slot(1, true);
        let prev = [sora, slot(2, false), slot(3, false), slot(0, false)];
        let now = [sora, slot(0, false), slot(0, false), slot(0, false)];
        let n = switch_notice(prev, now);
        assert!(n.came_in.is_empty());
        assert_eq!(n.went_out, vec![slot(2, false), slot(3, false)]);
        assert_eq!(n.reason, None);
        let back = switch_notice(now, prev);
        assert_eq!(back.came_in, vec![slot(2, false), slot(3, false)]);
        assert!(back.went_out.is_empty());
        // A swap of two slots changes nobody; a locked new member closes the notice with its line.
        let swapped = [sora, slot(3, false), slot(2, false), slot(0, false)];
        assert_eq!(switch_notice(prev, swapped), Notice::default());
        let locked = [sora, slot(2, false), slot(7, true), slot(0, false)];
        let n = switch_notice(prev, locked);
        assert_eq!((n.came_in, n.went_out, n.reason), (vec![slot(7, true)], vec![slot(3, false)], Some(7)));
    }

    /// Slots from bytes: the leader's slot is locked, bit 0x80 locks a companion, 0x10 is empty.
    #[test]
    fn the_notice_slots_read_the_bytes() {
        let mut merged = [0u16; 24];
        merged[8] = 100;
        merged[9] = 101;
        merged[10] = 102;
        let member_of = |o: u16| Some(o - 98);
        let s = notice_slots([0, 1, 0x80 | 2, EMPTY], &merged, member_of);
        assert_eq!(s[0], NoticeSlot { member: 2, locked: true });
        assert_eq!(s[1], NoticeSlot { member: 3, locked: false });
        assert_eq!(s[2], NoticeSlot { member: 4, locked: true });
        assert_eq!(s[3], NoticeSlot { member: 0, locked: false });
    }

    /// `FUN_0017cee8`'s two scales: the executable's word at 0x37a560 is 1.0 and replaces `sstm +0x54` only for a
    /// member whose block has bit 131, i.e. an equipped Hyper Healing.
    #[test]
    fn revive_scale_takes_the_full_word_with_hyper_healing() {
        assert_eq!(revive_scale(&[], 1.0, 0.25), 0.25);
        assert_eq!(revive_scale(&[(420, true)], 1.0, 0.25), 0.25, "another ability does not set bit 131");
        assert_eq!(revive_scale(&[(ABILITY_HYPER_HEALING, false)], 1.0, 0.25), 0.25, "learned but not equipped");
        assert_eq!(revive_scale(&[(ABILITY_HYPER_HEALING, true)], 1.0, 0.25), 1.0);
    }

    /// The word at 0x37a560 is 1.0 in the executable, and ability 419 (the one `FUN_001c0f40` handles as Hyper
    /// Healing, id 0x1a3) is the item row with bit 131.
    #[test]
    fn hyper_healing_is_ability_419_bit_131() {
        let Some(iso) = crate::assets::iso_path().ok().filter(|p| p.exists()) else {
            eprintln!("skipped: no disc image (set KH2_ISO)");
            return;
        };
        let elf = crate::assets::Disc::open(&iso).read_root("SLUS_210.05").expect("exe");
        assert_eq!(revive_full_scale(&elf), Some(1.0));
        let mut a = kh2_archive::Archive::open(&iso).expect("disc");
        let r = crate::shared::abilities::AbilityRules::load(&mut a).expect("rules");
        assert_eq!(r.abilities[&ABILITY_HYPER_HEALING].bit, 131);
    }
}
