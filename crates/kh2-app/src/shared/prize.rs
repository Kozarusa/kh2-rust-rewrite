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
#[cfg(test)]
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
