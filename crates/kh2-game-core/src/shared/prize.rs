//! The prize and obtained-log data the lower crates read (moved out of `prize.rs`, which re-exports it).

use bevy::prelude::*;

use crate::party_rules::exe_bytes;

/// Pool size: `FUN_001a3f98` -> `FUN_001a34f8(.., 0x100, 0x60)`.
pub const POOL: usize = 0x100;

/// Address of the growth chains `FUN_001d3f18` reads: `0x37c556 + 10 * form + 2 * level`, five ability
/// ids per form for the display levels 0..4 (0: none). Rows 1..4 are Valor (High Jump), Wisdom (Quick
/// Run), Master (Aerial Dodge) and Final (Glide).
pub const GROWTH_CHAINS: u32 = 0x37c556;

/// The growth chains of the drive forms (`GROWTH_CHAINS`), forms 0..=4, read from the executable.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct GrowthChains(pub [[u16; 5]; 5]);

impl GrowthChains {
    pub fn from_exe(elf: &[u8]) -> Option<GrowthChains> {
        let b = exe_bytes(elf, GROWTH_CHAINS, 50)?;
        let mut t = [[0u16; 5]; 5];
        for i in 0..25 {
            t[i / 5][i % 5] = u16::from_le_bytes([b[2 * i], b[2 * i + 1]]);
        }
        Some(GrowthChains(t))
    }

    /// The id at `(form, level)` (0 outside the table).
    pub fn at(&self, form: u8, level: u8) -> u16 {
        self.0.get(form as usize).and_then(|r| r.get(level as usize)).copied().unwrap_or(0)
    }
}

/// `FUN_001d3f18(list, n, form, level)`: in the ability list (bit 15 = equipped) the first slot whose id
/// is the chain's id one level below `level` (an empty slot when that is 0) takes the chain's id at
/// `level`, keeping its bit 15; at most one slot changes. Returns the chain's id at `level` whether or not
/// a slot matched (the disassembly loads it into v0 before the search; the decompiler shows no return).
pub fn growth_upgrade(list: &mut [u16], chains: &GrowthChains, form: u8, level: u8) -> u16 {
    let new = chains.at(form, level);
    let old = if level == 0 { chains.at(form.wrapping_sub(1), 4) } else { chains.at(form, level - 1) };
    if let Some(slot) = list.iter_mut().find(|s| **s & 0x7fff == old) {
        *slot = (*slot & 0x8000) | (new & 0x7fff);
    }
    new
}

/// `FUN_001d4078` (from the enemies' hit method at 0x1849d8, after the hit's +0x2a byte is added to the
/// victim's +0xaf0): a hit whose attacker is the player in Valor Form (+0xb50 == 1) gives Valor 1
/// experience (`FUN_001d3f88(player, 1)`).
pub fn valor_hit_exp(player_form: u8) -> i32 {
    (player_form == kh2_tables::player::form::VALOR) as i32
}

/// `FUN_001d40e0(id, player)` (from the enemies' defeat method `FUN_00184a30`, when the object that
/// defeated it, +0xae4, is the player): in Wisdom Form an object id (`FUN_001699a8`) in 1000..=2011, in
/// Final Form one in 3000..=4008, gives the form 1 experience.
pub fn defeat_exp(player_form: u8, object_id: u32) -> i32 {
    use kh2_tables::player::form;
    let hit = match player_form {
        form::WISDOM => object_id.wrapping_sub(1000) < 0x3f4,
        form::FINAL => object_id.wrapping_sub(3000) < 0x3f1,
        _ => false,
    };
    hit as i32
}

/// One record of the obtained log (`FUN_001d0688`, 6 bytes in the ring of 32 at 0x341518 + 4).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Obtained {
    /// +0, +1: the world and area bytes (0x33e830, 0x33e831).
    pub world: u8,
    pub area: u8,
    /// +2 low 15 bits.
    pub item: u16,
    /// +2 bit 15: set for a key item (`Flag0` bit 0), else set only when the item's "obtained once" bit
    /// (0x3414f0 by its `Slot`) is clear.
    pub new: bool,
    /// +4: who got it (the `FUN_001a1768` room, 1 for the form level-up gifts).
    pub to: u8,
    /// +5: the source: 0 an item object (`FUN_001bd1d8`), 3 a level-up (`FUN_001b53d0`), 4 (`FUN_001be500`),
    /// 5 a form level-up (`FUN_001d3da8`), 6 a summon level-up (`FUN_001d4fa0`).
    pub source: u8,
}

/// Ring size of the obtained log (`FUN_001d0760`: index + 1 modulo 32).
pub const OBTAINED_LOG: usize = 32;

/// The obtained log (`FUN_001d0828` -> `FUN_001d0760(0x341518, ..)`): the index word, then 32 records;
/// each new record overwrites the one at the index, which then moves on modulo 32. It lives in the save
/// (0x341518 = save + 0x16b8); here it is kept beside `Progress` (docs/systems/combat-effects/prizes.md).
#[derive(Resource, Clone, Debug, Default, PartialEq)]
pub struct ObtainedLog {
    pub next: usize,
    pub records: [Obtained; OBTAINED_LOG],
}

impl ObtainedLog {
    /// `FUN_001d0760`: writes `r` at the index and moves the index on.
    pub fn push(&mut self, r: Obtained) {
        let i = self.next % OBTAINED_LOG;
        self.records[i] = r;
        self.next = (i + 1) % OBTAINED_LOG;
    }
}

/// `FUN_001d0688`'s record. `key` is the item row's `Flag0` bit 0, `obtained_bit` its "obtained once" bit
/// at the time of the call.
pub fn obtained_record(place: (u8, u8), item: u16, key: bool, obtained_bit: bool, to: u8, source: u8) -> Obtained {
    Obtained { world: place.0, area: place.1, item: item & 0x7fff, new: key || !obtained_bit, to, source }
}

/// The world and area bytes (0x33e830, 0x33e831) of an area name such as `tt04` [inferred: the world is the
/// code's place in the game's world table, the area the number].
pub fn place_bytes(area: &str) -> (u8, u8) {
    let world = area.get(..2).and_then(|w| kh2_image::text::WORLDS.iter().position(|x| *x == w)).unwrap_or(0) as u8;
    let n = area.get(2..).and_then(|n| n.parse::<u8>().ok()).unwrap_or(0);
    (world, n)
}

/// Experience for the player's current form from outside the prizes (`FUN_001d4078` Valor hits,
/// `FUN_001d40e0` Wisdom / Final defeats): applied by `FUN_001d3f88` when the player is in `form`. The
/// rules are `valor_hit_exp` and `defeat_exp`; the sender is the hit / defeat code.
#[derive(Message, Clone, Copy, Debug, PartialEq, Eq)]
pub struct FormExpGain {
    pub form: u8,
    pub amount: i32,
}

/// A drop's prize objects, written by `enemy_ai` when host 1.92 runs.
#[derive(Message, Clone, Debug)]
pub struct PrizeDrop {
    /// The enemy that dropped them (its lock-on point is the drop point).
    pub from: Entity,
    pub row: u16,
    pub launches: Vec<(u8, [u32; 3])>,
}

/// A drop's item objects (`FUN_001bd490` per item that won its roll), written by `enemy_ai`.
#[derive(Message, Clone, Debug)]
pub struct ItemDrop {
    /// The enemy that dropped them (its lock-on point is the drop point, as for the prizes).
    pub from: Entity,
    /// Item id and the four generator values of its object (`enemy_host::Drop::item_launches`).
    pub launches: Vec<(u16, [u32; 4])>,
}

/// Host 6.31 (`FUN_002eece8`): every pickable prize is drawn into the player in mode 1 and paid
/// (`FUN_001a40b8`), every item object is handed to the player (`FUN_001bda98`).
#[derive(Message, Clone, Copy, Debug)]
pub struct CollectAll;

/// The live prizes (the pool's used slots).
#[derive(Resource, Default)]
pub struct PrizePool {
    pub live: usize,
}

impl PrizePool {
    pub fn free(&self) -> usize {
        POOL.saturating_sub(self.live)
    }
}

/// A kind's entry at 0x37b4b0 + 16 * kind.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct KindEntry {
    /// +0: the kind number the pool entry keeps (+0x14) and the pickup row it uses.
    pub id: u8,
    /// +4: added to the position's Y (down) for the floor sweep and the pickup test.
    pub radius: f32,
    /// +8: sound of the first two bounces (`FUN_001c5060`).
    pub bounce_se: u32,
    /// +0xc: sound at the pickup, placed at the player's centre (`FUN_001c5808`).
    pub pickup_se: u32,
}

/// The floats the prize code reads (addresses in the field docs).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Consts {
    /// 0x37b480: the generator value's scale (2^-32).
    pub unit: f32,
    /// 0x37b484 / 0x37b488: X and Z launch speed = base + value * unit * span.
    pub xz_span: f32,
    pub xz_base: f32,
    /// 0x37b48c / 0x37b490: Y launch speed (Y down).
    pub y_span: f32,
    pub y_base: f32,
    /// 0x37b478 / 0x37b47c: pickup reach (horizontal) and height (`FUN_001a3700`).
    pub reach: f32,
    pub height: f32,
    pub kinds: [KindEntry; KINDS],
    pub pickups: [Pickup; KINDS],
    /// 0x37bf34: life in frames (`FUN_001c4eb0` -> +0x40).
    pub life: f32,
    /// 0x37bf44: slowest landing speed that still bounces.
    pub bounce_min: f32,
    /// 0x37bf48: how far the mover's sideways result may differ from the velocity.
    pub wall_eps: f32,
    /// 0x37bf4c / 0x37bf50: gravity per frame and the fall speed cap.
    pub gravity: f32,
    pub fall_max: f32,
    /// 0x37bf54: sideways velocity factor per frame.
    pub friction: f32,
    /// 0x37bf58: the bounce factor `FUN_001c5248` passes.
    pub bounce: f32,
    /// 0x37bf5c / 0x37bf60: frames of the closing shrink and the size scale.
    pub fade: f32,
    pub fade_scale: f32,
    /// 0x37bf64 / 0x37bf68 / 0x37bf6c: the draw-in length, turn numerator and size scale.
    pub absorb_len: f32,
    pub absorb_turn: f32,
    pub absorb_scale: f32,
    /// 0x37bf7c: frames of the draw-in (`FUN_001c5808` mode 0).
    pub absorb_time: f32,
    /// 0x37bf88: largest turn per frame (copied to 0x1d5e2c4 by `FUN_001c59b0`).
    pub turn_max: f32,
    /// The draw-in of mode 1 (`FUN_001a40b8` -> `FUN_001a3ad8` -> `FUN_001c5808(.., 1)`, update
    /// `LAB_001c5748` -> `FUN_001c5588`): 0x37bf70 length, 0x37bf74 turn numerator, 0x37bf78 size
    /// scale, 0x37bf80 frames, 0x37bf84 largest turn (copied to 0x1d5e2c0 by `FUN_001c59b0`).
    pub absorb1_len: f32,
    pub absorb1_turn: f32,
    pub absorb1_scale: f32,
    pub absorb1_time: f32,
    pub turn_max1: f32,
    /// 0x37bc04: the drive multiplier `FUN_001ba040` returns outside its special case.
    pub drive_scale: f32,
}

impl Consts {
    pub fn from_exe(elf: &[u8]) -> Option<Consts> {
        let w = |va: u32| exe_bytes(elf, va, 4).map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]));
        let f = |va: u32| w(va).map(f32::from_bits);
        let b = |va: u32| exe_bytes(elf, va, 1).map(|x| x[0]);
        let mut kinds = [KindEntry::default(); KINDS];
        let mut pickups = [Pickup::default(); KINDS];
        for k in 0..KINDS as u32 {
            let e = 0x37b4b0 + 16 * k;
            kinds[k as usize] = KindEntry { id: w(e)? as u8, radius: f(e + 4)?, bounce_se: w(e + 8)?, pickup_se: w(e + 12)? };
            let p = 0x37b540 + 8 * k;
            pickups[k as usize] = Pickup { glow: f(p)?, hp: b(p + 4)?, mp: b(p + 5)?, drive: b(p + 6)?, munny: b(p + 7)? };
        }
        Some(Consts {
            unit: f(0x37b480)?,
            xz_span: f(0x37b484)?,
            xz_base: f(0x37b488)?,
            y_span: f(0x37b48c)?,
            y_base: f(0x37b490)?,
            reach: f(0x37b478)?,
            height: f(0x37b47c)?,
            kinds,
            pickups,
            life: f(0x37bf34)?,
            bounce_min: f(0x37bf44)?,
            wall_eps: f(0x37bf48)?,
            gravity: f(0x37bf4c)?,
            fall_max: f(0x37bf50)?,
            friction: f(0x37bf54)?,
            bounce: f(0x37bf58)?,
            fade: f(0x37bf5c)?,
            fade_scale: f(0x37bf60)?,
            absorb_len: f(0x37bf64)?,
            absorb_turn: f(0x37bf68)?,
            absorb_scale: f(0x37bf6c)?,
            absorb_time: f(0x37bf7c)?,
            turn_max: f(0x37bf88)?,
            absorb1_len: f(0x37bf70)?,
            absorb1_turn: f(0x37bf74)?,
            absorb1_scale: f(0x37bf78)?,
            absorb1_time: f(0x37bf80)?,
            turn_max1: f(0x37bf84)?,
            drive_scale: f(0x37bc04)?,
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    /// Update `FUN_001c5288` (physics, then the check).
    Fall,
    /// Update `FUN_001c5388` (the check only).
    Rest,
    /// Update `LAB_001c5568` -> `FUN_001c53a8` (drawn into the player).
    Absorb,
}

/// One pool entry (the fields of the 0x60-byte record the rules use).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Prize {
    /// +0x14: the kind (its entry's first byte).
    pub kind: u8,
    /// +0x00: position.
    pub pos: Vec3,
    /// +0x30..+0x38: velocity; while drawn in, the unit direction from the player's centre.
    pub vel: Vec3,
    /// +0x3c: while drawn in, the distance at the pickup.
    pub dist: f32,
    /// +0x40: frames left.
    pub life: f32,
    /// +0x44: bounces so far.
    pub bounces: u8,
    /// +0x45: frames to the next floor check at rest.
    pub recheck: u8,
    /// +0x28 bit 0: may be picked up (set once it falls).
    pub pickable: bool,
    /// +0x16: draw size 0..255 (the sprite is scaled by size / 255).
    pub size: u8,
    /// +0x17: shape frame (0xff: not started).
    pub frame: u8,
    pub phase: Phase,
    /// The draw-in's mode (`FUN_001c5808` third argument): 0 the pickup, 1 collect-all.
    pub mode: u8,
    /// +0x28 bit 8: set by the pickup dispatch `FUN_001a3848` while global flag 0x800 is up (`magic_rules::prize_dispatch`),
    /// never cleared; with it the prize goes to the player once +0x44 reaches 3.
    pub flag_8: bool,
}

impl Prize {
    pub fn radius(&self, c: &Consts) -> f32 {
        c.kinds.get(self.kind as usize).map_or(0.0, |k| k.radius)
    }

    /// The point the sweep and the pickup test use: the position lowered by the radius (Y down).
    pub fn low_point(&self, c: &Consts) -> Vec3 {
        self.pos + Vec3::Y * self.radius(c)
    }
}

/// Prize kinds (the 16-byte entries at 0x37b4b0 and the pickup rows at 0x37b540).
pub const KINDS: usize = 9;

/// A pickup row at 0x37b540 + 8 * kind (`FUN_001a39b0`).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Pickup {
    /// +0: above 0, the player's glow effect for this many frames (`FUN_00175c28`; not ported).
    pub glow: f32,
    /// +4: HP added through the player's method +0x78.
    pub hp: u8,
    /// +5: MP added (`FUN_00186368` -> `FUN_001a0180`).
    pub mp: u8,
    /// +6: drive gauge units (`FUN_001788d8`, `FUN_001d4148`).
    pub drive: u8,
    /// +7: munny (`FUN_0014f2d8` the strip, `FUN_0023e620` the counter).
    pub munny: u8,
}
