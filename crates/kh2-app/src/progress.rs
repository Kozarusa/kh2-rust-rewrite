//! The player's game state: what the original keeps in its save block for the party. One resource,
//! `Progress`, holds the inventory (spells, form items, summon charms and keyblades are items), the
//! party records (equipment, abilities, stats), the drive form records (level, experience, second
//! keyblade), the summon level and experience, munny, the per-world outfit bytes and the per-spell
//! switch bits. `docs/progress.md` has the layout with the addresses.
//!
//! `Progress` is the original's party block itself (`0x33fec0`, 0x1998 bytes; the save block is
//! `0x33e860`), with typed accessors. Keeping the bytes means every rule here is a port of the
//! routine that reads or writes them, and the kh2-ee oracle test compares the whole block with
//! the game's own code.
//!
//! Start state, in this order:
//! 1. The new game: the save block is cleared at boot (`FUN_00105638(0x33e860)`), the party block
//!    is set up (`FUN_00187b38`) and every `plrp` row with `Id` 0 is applied (`FUN_00187d20` ->
//!    `FUN_001c0cb0(0)`). Ported here and read from the user's disc (`plrp`, `item`, `cmd`).
//! 2. `KH2_STATE=<path>`: a JSON file applied on top of the new game (schema in docs/progress.md).
//!    Nothing else is added then.
//! 3. Without `KH2_STATE`: the app baseline, which is NOT the original's new game: the spells
//!    Fire, Blizzard, Thunder and Cure at level 1, the four form items (with the spare Kingdom Keys
//!    the forms take), the Kingdom Key equipped and Guard (`KH2_ABILITY_START=data` drops Guard).
//!    The app had these defaults before `Progress`; they stay until the story gives them.
//!
//! The systems read `Progress` and still apply their own environment variables on top
//! (`KH2_MAGIC`, `KH2_DRIVE_*`, `KH2_ABILITIES`, ...). `KH2_PROGRESSLOG=1` logs the state at start.

// The API serves later work packages (items, prizes, magic, summons); not all of it has a reader yet.
#![allow(dead_code)]

use std::collections::BTreeMap;
use std::sync::Arc;

use bevy::prelude::*;
use kh2_tables::tables::{self as t, Cmd, Item, Magc, Plrp};

use crate::assets::Disc;
use crate::data::KINGDOM_KEY;

/// EE addresses of the blocks (for the docs and the oracle test).
pub mod addr {
    /// The save block: `FUN_00105638(0x33e860)` clears 0xb4e0 bytes from here at boot.
    pub const SAVE: u32 = 0x0033_e860;
    pub const SAVE_LEN: u32 = 0xb4e0;
    /// The party block (save + 0x1660), set up by `FUN_00187b38`.
    pub const PARTY: u32 = 0x0033_fec0;
}

/// Byte offsets in the party block.
pub mod off {
    /// Party member records: `FUN_00187d40(0x33fec0, member)` = `0x33fec0 + (index - 1) * 0xf4`.
    pub const MEMBER_SIZE: usize = 0xf4;
    pub const MEMBERS: usize = 13;
    /// Form records: `FUN_00187f60(0x33fec0, f)` = `0x33fec0 + 0xc3c + f * 0x28`, `f` 1..=9.
    pub const FORM_BASE: usize = 0xc3c;
    pub const FORM_SIZE: usize = 0x28;
    pub const FORMS: usize = 9;
    /// `0x340c88` + 4: the current form (`FUN_001b4c28`).
    pub const CURRENT_FORM: usize = 0xdcc;
    /// `0x340c88` + 5: the active summon (1-based `sumn` index), + 6 the summon level, + 7 the
    /// Genie form / Tinker Bell byte.
    pub const SUMMON: usize = 0xdcd;
    pub const SUMMON_LEVEL: usize = 0xdce;
    pub const SUMMON_EXTRA: usize = 0xdcf;
    /// Saved drive gauge bytes `+0xdd0..+0xdd3` and floats `+0xdd4`, `+0xdd8` (`FUN_0019f920`
    /// copies them to status `+0x1b0..+0x1b8`).
    pub const DRIVE: usize = 0xdd0;
    /// World records `0x340c9c + 4 * world` (`FUN_001a71c0`), 19 of them.
    pub const WORLDS: usize = 0xddc;
    pub const WORLD_COUNT: usize = 19;
    /// Item counts `0x340ce8`, one byte per item `Slot` (`FUN_001a1cc8`).
    pub const COUNTS: usize = 0xe28;
    pub const COUNT_SLOTS: usize = 0x140;
    /// Key-item bits `0x340e28` (items with `Flag0` bit 0), by `Slot`.
    pub const KEYS: usize = 0xf68;
    pub const KEY_WORDS: usize = 8;
    /// Experience total `0x340e48` (`FUN_001b51e0`, followed by the level-up check `FUN_001b52f8`).
    pub const EXPERIENCE: usize = 0xf88;
    /// Summon experience `0x340e4c` (`FUN_001d4ec8`).
    pub const SUMMON_EXP: usize = 0xf8c;
    /// Consumables Sora received, 8 ids (`FUN_001a2120`).
    pub const RECENT: usize = 0xf90;
    /// Four command shortcuts (`0x340e60`; `FUN_001a2120` and `FUN_001a1a18` fill empty ones).
    pub const SHORTCUTS: usize = 0xfa0;
    /// Per-spell switch bits `0x340eac` (`FUN_001a4b30`, `FUN_001a4b68`, `FUN_001a4bb8`).
    pub const SPELL_SWITCH: usize = 0xfec;
    /// "Obtained once" bits `0x3414f0`, by item `Slot` (`FUN_001a18d8`).
    pub const OBTAINED: usize = 0x1630;
    pub const OBTAINED_WORDS: usize = 10;
    /// Length of the party block as `FUN_00187b38` touches it.
    pub const LEN: usize = 0x1998;
}

/// Offsets in a party member record.
mod rec {
    pub const WEAPONS: usize = 0x0;
    pub const HP: usize = 0x4;
    pub const HP_MAX: usize = 0x5;
    pub const MP: usize = 0x6;
    pub const MP_MAX: usize = 0x7;
    pub const AP: usize = 0x8;
    pub const STRENGTH: usize = 0x9;
    pub const MAGIC: usize = 0xa;
    pub const DEFENSE: usize = 0xb;
    pub const WEAPON_SET: usize = 0xe;
    pub const LEVEL: usize = 0xf;
    pub const ARMOR_SLOTS: usize = 0x10;
    pub const ACCESSORY_SLOTS: usize = 0x11;
    pub const ITEM_SLOTS: usize = 0x12;
    pub const ARMOR: usize = 0x14;
    pub const ACCESSORIES: usize = 0x24;
    pub const ITEMS: usize = 0x34;
    pub const RESERVED: usize = 0x44;
    pub const ABILITIES: usize = 0x54;
    pub const ABILITY_SLOTS: usize = 64;
}

/// Item types the rules switch on (`item` +2).
pub mod item_type {
    pub const CONSUMABLE: u8 = 0;
    pub const KEYBLADE: u8 = 2;
    pub const MAGIC: u8 = 0x12;
    pub const ABILITY: u8 = 0x13;
    pub const DRIVE_FORM: u8 = 0x15;
}

/// The "inventory" target id (`FUN_001a1768` / `FUN_001a18d8` with 100: no party member).
pub const INVENTORY: u16 = 100;
/// Count limit of an item (`FUN_001a1768`: a count above 99 refuses the item).
pub const MAX_COUNT: u8 = 99;
/// The command shortcuts of the Customize page (save `+0x2600`, four u16).
pub const SHORTCUTS: usize = 4;
/// Experience limit (`FUN_001b51e0`).
pub const MAX_EXPERIENCE: i32 = 9_999_999;
/// Sora's party member id.
pub const SORA: u16 = 1;

/// What the rules read of one item row (`item`, 0x18 bytes, found by `FUN_001a66e8`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ItemInfo {
    pub id: u16,
    pub kind: u8,
    pub flag0: u8,
    /// `Flag1 | Rank << 8` (item +4).
    pub word4: u16,
    pub command: u16,
    /// Index into the count array and the bit sets (item +0x12).
    pub slot: u16,
}

impl ItemInfo {
    /// `Flag0` bit 0: a key item, owned or not (one bit, no count).
    pub fn is_key(&self) -> bool {
        self.flag0 & 1 != 0
    }
}

/// The tables the rules need, from the user's disc.
#[derive(Debug, Default)]
pub struct ItemIndex {
    /// In table order (`FUN_001a6730` walks them in this order).
    pub rows: Vec<ItemInfo>,
    by_id: BTreeMap<u16, usize>,
    /// `cmd.Argument` by command id (`FUN_001b6f48` + 4).
    cmd_arg: BTreeMap<u16, i16>,
    /// The item of each magic slot: the first `magc` row of the slot (`FUN_001a4c78` reads its
    /// `Item` at +0x24); 0 when the slot needs none.
    pub magic_items: [u16; 6],
    /// The "X Form" item (type 21) of each form id 1..=5 (its command's `Execute` is 4 and its
    /// `Argument` the form).
    pub form_items: [u16; 6],
}

impl ItemIndex {
    pub fn new(items: &[Item], cmds: &[Cmd], magc: &[Magc]) -> ItemIndex {
        let rows: Vec<ItemInfo> = items
            .iter()
            .map(|i| ItemInfo {
                id: i.id,
                kind: i.r#type,
                flag0: i.flag0,
                word4: i.flag1 as u16 | (i.rank as u16) << 8,
                command: i.command,
                slot: i.slot,
            })
            .collect();
        let mut by_id = BTreeMap::new();
        for (n, r) in rows.iter().enumerate() {
            by_id.entry(r.id).or_insert(n);
        }
        let cmd_arg: BTreeMap<u16, i16> = cmds.iter().map(|c| (c.id, c.argument)).collect();
        let mut magic_items = [0u16; 6];
        for (slot, it) in magic_items.iter_mut().enumerate() {
            *it = magc.iter().find(|r| r.id as usize == slot).map_or(0, |r| r.item);
        }
        let mut form_items = [0u16; 6];
        for r in rows.iter().filter(|r| r.kind == item_type::DRIVE_FORM) {
            if let Some(c) = cmds.iter().find(|c| c.id == r.command && c.execute == 4)
                && (1..=5).contains(&c.argument)
                && form_items[c.argument as usize] == 0
            {
                form_items[c.argument as usize] = r.id;
            }
        }
        ItemIndex { rows, by_id, cmd_arg, magic_items, form_items }
    }

    /// `FUN_001a66e8`: the row of an item id.
    pub fn get(&self, id: u16) -> Option<&ItemInfo> {
        self.by_id.get(&id).map(|&n| &self.rows[n])
    }
}

/// A party member record, decoded.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct Member {
    /// Hand 0 and hand 1 (`FUN_0016ee98`: record `+2 * hand`); hand 0 is the keyblade.
    pub weapons: [u16; 2],
    pub hp: u8,
    pub hp_max: u8,
    pub mp: u8,
    pub mp_max: u8,
    pub ap: u8,
    pub strength: u8,
    pub magic: u8,
    pub defense: u8,
    /// Sword, Shield or Staff column of `lvup` (`FUN_001b52f8`).
    pub weapon_set: u8,
    pub level: u8,
    /// Armor slots (the count is `+0x10`); 0 is an empty slot.
    pub armor: Vec<u16>,
    /// Accessory slots (count `+0x11`).
    pub accessories: Vec<u16>,
    /// Battle item slots (count `+0x12`).
    pub items: Vec<u16>,
    /// The ability list (64 slots, bit 15 = equipped), non-empty slots only.
    pub abilities: Vec<(u16, bool)>,
}

/// A drive form record (`FUN_00187f60`), decoded.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct FormRecord {
    /// The form's second keyblade (`FUN_001a2010` writes it, `FUN_0016ee98` hand 1 reads it).
    pub weapon: u16,
    /// Internal level (`+2`, `FUN_001d3da8`).
    pub level: u8,
    /// `+3`: set to 1 with the level by `FUN_00187ed0`; no reader was traced.
    pub byte3: u8,
    /// Experience (`+4`, `FUN_001d3f88`); for form 5 the Anti counter (`FUN_001d4278`).
    pub exp: u32,
    /// The form's own ability list (16 slots, `FUN_00187f18`), non-empty slots only.
    pub abilities: Vec<(u16, bool)>,
}

/// Where the state came from (the log's "state from ...").
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Source {
    NewGame,
    File(String),
}

/// The game state. See the module doc.
#[derive(Resource, Clone)]
pub struct Progress {
    block: Vec<u8>,
    items: Arc<ItemIndex>,
    pub source: Source,
    /// What the app baseline added on top of the new game (empty with `KH2_STATE`).
    pub baseline: Vec<String>,
    /// Sora's Dodge Roll growth level (0 = not learned). The USA disc has no Dodge Roll ability item
    /// (docs/final-mix.md: the Final Mix game has it as growth slot 4), so the level is an app field
    /// beside the save block. 0 in the new game; the app baseline sets 1 (docs/progress.md).
    pub dodge_roll: u8,
    /// The munny the prizes pay: save block `+0x1600` (`0x33fe60`, before the party block, so it is
    /// kept apart from `block`). `FUN_0023e620` adds to it and `FUN_0023e5e0` lets a sum through only
    /// below 1,000,000 (else 999,999). The word at `0x340e48` that `experience()` reads is raised by
    /// `FUN_001b51e0`, which then runs the level-up check of every party member (`FUN_001b52f8`
    /// compares it with the level table), so that word is the experience total (docs/prizes.md).
    wallet: u32,
    /// The option word (save `+0x31b0`, `0x33fa10`: the ten Config rows of docs/menus.md 2.8) and the game
    /// mode (save `+0x1658`, read-only on that page). Outside the party block, so kept apart like `wallet`.
    config: [u32; 2],
}

/// The option word `FUN_0023e9a0` writes on a new game: it keeps bits 0, 3 and 11 up and sets 0x40b, so
/// vibration on, Stereo, Navigational Map on, Summon Effects Auto, and every other row at its first value.
pub const OPTION_DEFAULT: u32 = 0x40b;
/// The game mode the app starts in: Standard (1; Beginner 0, Proud 2). The title screen sets the original's;
/// the app has none, so this is [inferred].
pub const GAME_MODE_DEFAULT: u32 = 1;

/// The prize munny limit (`FUN_0023e5e0`: a sum below 1,000,000 is kept, else 999,999).
pub const MAX_WALLET: u32 = 999_999;

impl Progress {
    // --- raw block access -------------------------------------------------------------------

    fn u8_at(&self, o: usize) -> u8 {
        self.block[o]
    }
    fn set_u8(&mut self, o: usize, v: u8) {
        self.block[o] = v;
    }
    fn u16_at(&self, o: usize) -> u16 {
        u16::from_le_bytes([self.block[o], self.block[o + 1]])
    }
    fn set_u16(&mut self, o: usize, v: u16) {
        self.block[o..o + 2].copy_from_slice(&v.to_le_bytes());
    }
    fn u32_at(&self, o: usize) -> u32 {
        u32::from_le_bytes(self.block[o..o + 4].try_into().unwrap())
    }
    fn set_u32(&mut self, o: usize, v: u32) {
        self.block[o..o + 4].copy_from_slice(&v.to_le_bytes());
    }
    fn zero(&mut self, o: usize, len: usize) {
        self.block[o..o + len].fill(0);
    }
    fn bit(&self, base: usize, words: usize, n: u16) -> bool {
        let w = n as usize >> 5;
        w < words && self.u32_at(base + 4 * w) >> (n & 31) & 1 != 0
    }
    fn set_bit(&mut self, base: usize, words: usize, n: u16, on: bool) {
        let w = n as usize >> 5;
        if w < words {
            let v = self.u32_at(base + 4 * w);
            let m = 1 << (n & 31);
            self.set_u32(base + 4 * w, if on { v | m } else { v & !m });
        }
    }

    /// The party block bytes (`0x33fec0`, `off::LEN` bytes).
    pub fn block(&self) -> &[u8] {
        &self.block
    }

    pub fn items(&self) -> &ItemIndex {
        &self.items
    }

    /// A state from party block bytes (for the oracle test: the block the game's code built).
    pub fn from_block(bytes: &[u8], items: Arc<ItemIndex>) -> Progress {
        let mut block = vec![0u8; off::LEN];
        let n = bytes.len().min(off::LEN);
        block[..n].copy_from_slice(&bytes[..n]);
        Progress { block, items, source: Source::NewGame, baseline: Vec::new(), dodge_roll: 0, wallet: 0, config: [OPTION_DEFAULT, GAME_MODE_DEFAULT] }
    }

    // --- records ----------------------------------------------------------------------------

    /// `FUN_001b58d8`: member id -> 1-based record index (14 is Sora's record, 15 the sixth).
    fn member_index(member: u16) -> usize {
        match member {
            14 => 1,
            15 => 6,
            m if (1..=15).contains(&m) => m as usize,
            _ => 0,
        }
    }

    /// `FUN_00187d40`: offset of a member's record.
    fn member_off(member: u16) -> Option<usize> {
        let i = Self::member_index(member);
        (i != 0 && i <= off::MEMBERS).then(|| (i - 1) * off::MEMBER_SIZE)
    }

    /// `FUN_00187f60`: offset of a form record, `form` 1..=9.
    fn form_off(form: u8) -> Option<usize> {
        (1..=off::FORMS as u8).contains(&form).then(|| off::FORM_BASE + form as usize * off::FORM_SIZE)
    }

    /// A party member's record (`member` is the party id: 1 Sora, 2 Donald, 3 Goofy, ...).
    pub fn member(&self, member: u16) -> Option<Member> {
        let r = Self::member_off(member)?;
        let list = |o: usize, n: usize| (0..n.min(8)).map(|i| self.u16_at(r + o + 2 * i)).collect::<Vec<_>>();
        Some(Member {
            weapons: [self.u16_at(r + rec::WEAPONS), self.u16_at(r + rec::WEAPONS + 2)],
            hp: self.u8_at(r + rec::HP),
            hp_max: self.u8_at(r + rec::HP_MAX),
            mp: self.u8_at(r + rec::MP),
            mp_max: self.u8_at(r + rec::MP_MAX),
            ap: self.u8_at(r + rec::AP),
            strength: self.u8_at(r + rec::STRENGTH),
            magic: self.u8_at(r + rec::MAGIC),
            defense: self.u8_at(r + rec::DEFENSE),
            weapon_set: self.u8_at(r + rec::WEAPON_SET),
            level: self.u8_at(r + rec::LEVEL),
            armor: list(rec::ARMOR, self.u8_at(r + rec::ARMOR_SLOTS) as usize),
            accessories: list(rec::ACCESSORIES, self.u8_at(r + rec::ACCESSORY_SLOTS) as usize),
            items: list(rec::ITEMS, self.u8_at(r + rec::ITEM_SLOTS) as usize),
            abilities: (0..rec::ABILITY_SLOTS)
                .map(|i| self.u16_at(r + rec::ABILITIES + 2 * i))
                .filter(|v| v & 0x7fff != 0)
                .map(|v| (v & 0x7fff, v & 0x8000 != 0))
                .collect(),
        })
    }

    pub fn sora(&self) -> Member {
        self.member(SORA).unwrap_or_default()
    }

    /// A drive form record (`form` 1..=9; 1..=5 are Valor, Wisdom, Master, Final, Anti).
    pub fn form(&self, form: u8) -> FormRecord {
        let Some(r) = Self::form_off(form) else { return FormRecord::default() };
        FormRecord {
            weapon: self.u16_at(r),
            level: self.u8_at(r + 2),
            byte3: self.u8_at(r + 3),
            exp: self.u32_at(r + 4),
            abilities: (0..16).map(|i| self.u16_at(r + 8 + 2 * i)).filter(|v| v & 0x7fff != 0).map(|v| (v & 0x7fff, v & 0x8000 != 0)).collect(),
        }
    }

    pub fn form_level(&self, form: u8) -> u8 {
        self.form(form).level
    }

    pub fn form_exp(&self, form: u8) -> u32 {
        self.form(form).exp
    }

    pub fn set_form_level(&mut self, form: u8, level: u8) {
        if let Some(r) = Self::form_off(form) {
            self.set_u8(r + 2, level);
        }
    }

    pub fn set_form_exp(&mut self, form: u8, exp: u32) {
        if let Some(r) = Self::form_off(form) {
            self.set_u32(r + 4, exp);
        }
    }

    pub fn set_form_weapon(&mut self, form: u8, item: u16) {
        if let Some(r) = Self::form_off(form) {
            self.set_u16(r, item);
        }
    }

    /// The form is unlocked: its "X Form" item is owned.
    pub fn owns_form(&self, form: u8) -> bool {
        self.items.form_items.get(form as usize).is_some_and(|&i| i != 0 && self.item_count(i) > 0)
    }

    pub fn current_form(&self) -> u8 {
        self.u8_at(off::CURRENT_FORM)
    }

    /// The active summon (1-based `sumn` index, 0 none).
    pub fn summon(&self) -> u8 {
        self.u8_at(off::SUMMON)
    }

    pub fn summon_level(&self) -> u8 {
        self.u8_at(off::SUMMON_LEVEL)
    }

    pub fn summon_exp(&self) -> u32 {
        self.u32_at(off::SUMMON_EXP)
    }

    /// The Genie form / Tinker Bell byte (`0x340c8f`).
    pub fn summon_extra(&self) -> u8 {
        self.u8_at(off::SUMMON_EXTRA)
    }

    /// The saved drive gauge bytes (`+0xdd0..+0xdd3`, status `+0x1b0..+0x1b3`). Not wired: the
    /// drive system keeps its own start values (docs/progress.md, Gaps).
    pub fn drive_bytes(&self) -> [u8; 4] {
        [self.u8_at(off::DRIVE), self.u8_at(off::DRIVE + 1), self.u8_at(off::DRIVE + 2), self.u8_at(off::DRIVE + 3)]
    }

    /// The four bytes of a world's record (`0x340c9c + 4 * world`).
    pub fn world_record(&self, world: usize) -> [u8; 4] {
        if world >= off::WORLD_COUNT {
            return [0; 4];
        }
        let o = off::WORLDS + 4 * world;
        [self.u8_at(o), self.u8_at(o + 1), self.u8_at(o + 2), self.u8_at(o + 3)]
    }

    /// Sora's outfit in a world: the low 5 bits of byte 0 of the world's record (`FUN_001a71e8`).
    pub fn outfit(&self, world: usize) -> u8 {
        self.world_record(world)[0] & 0x1f
    }

    pub fn set_outfit(&mut self, world: usize, outfit: u8) {
        if world < off::WORLD_COUNT {
            let o = off::WORLDS + 4 * world;
            let b = self.u8_at(o);
            self.set_u8(o, (b & !0x1f) | (outfit & 0x1f));
        }
    }

    /// The per-spell switch bit of a magic slot (`FUN_001a4bb8`; the pause menu toggles it).
    pub fn spell_switch(&self, slot: u8) -> bool {
        self.bit(off::SPELL_SWITCH, 1, slot as u16)
    }

    pub fn spell_switch_word(&self) -> u32 {
        self.u32_at(off::SPELL_SWITCH)
    }

    /// `FUN_001a4b68` (set the bit) and `FUN_001a4b30` (clear it): the per-spell switch bit `bit` of the
    /// word at save `+0x264c` (party block `+0xfec`). A SET bit means "Not Used": `FUN_001a4bb8` answers
    /// 1 (usable) when the bit is clear. A bit past the word (`bit >= 32`) is ignored here; the original
    /// indexes a second word it never has.
    pub fn set_spell_switch(&mut self, bit: u8, set: bool) {
        self.set_bit(off::SPELL_SWITCH, 1, bit as u16, set);
    }

    /// `FUN_00274e50`, the Customize page's switch: flips the bit and answers whether the spell is now
    /// usable ("Use", the bit clear). The original reads the state with `FUN_001a4bb8` and writes the
    /// opposite (clear when it read 0, set otherwise), which is a plain flip for every spell whose command
    /// `Cost` is not 255 (`docs/magic.md`).
    pub fn toggle_spell_switch(&mut self, bit: u8) -> bool {
        let now = self.spell_switch(bit);
        self.set_spell_switch(bit, !now);
        now
    }

    /// One of the four command shortcuts (`FUN_001a1f78` gives an item's command, `FUN_00274dd0` writes
    /// it): the u16 at save `+0x2600 + 2 * slot` (party block `+0xfa0`).
    pub fn shortcut(&self, slot: usize) -> u16 {
        if slot < SHORTCUTS { self.u16_at(off::SHORTCUTS + 2 * slot) } else { 0 }
    }

    pub fn shortcuts(&self) -> [u16; SHORTCUTS] {
        std::array::from_fn(|i| self.shortcut(i))
    }

    /// `FUN_00274dd0`: store a command (0 empties the slot). False for a slot past the four.
    pub fn set_shortcut(&mut self, slot: usize, command: u16) -> bool {
        if slot >= SHORTCUTS {
            return false;
        }
        self.set_u16(off::SHORTCUTS + 2 * slot, command);
        true
    }

    /// `FUN_001a0ec0`, `0ee0`, `0f00`, `0f20`: whether the stat boost item `id` (0x114 strength, 0x115 magic,
    /// 0x116 defense, 0x117 AP) has reached its limit on the member's record (the byte is above 99).
    pub fn boost_maxed(&self, member: u16, id: u16) -> bool {
        let (Some(r), Some(b)) = (Self::member_off(member), Self::boost_byte(id)) else { return false };
        self.u8_at(r + b) > 99
    }

    fn boost_byte(id: u16) -> Option<usize> {
        match id {
            0x114 => Some(rec::STRENGTH),
            0x115 => Some(rec::MAGIC),
            0x116 => Some(rec::DEFENSE),
            0x117 => Some(rec::AP),
            _ => None,
        }
    }

    /// `FUN_001a0ce0`, `0d30`, `0d80`, `0dd0`: add `n` to the record byte the boost item raises, held at 100
    /// (when the sum would pass 100 only the room is added). Answers the amount added; the original adds
    /// the same amount to the live status block, which the app recomputes from the record.
    pub fn apply_boost(&mut self, member: u16, id: u16, n: u8) -> u8 {
        let (Some(r), Some(b)) = (Self::member_off(member), Self::boost_byte(id)) else { return 0 };
        let old = self.u8_at(r + b);
        let add = if (old as u32 + n as u32) < 0x65 { n } else { 100u8.saturating_sub(old) };
        self.set_u8(r + b, old.wrapping_add(add));
        add
    }

    /// The experience total (`0x340e48`, see the field `wallet` for the munny).
    pub fn experience(&self) -> i32 {
        self.u32_at(off::EXPERIENCE) as i32
    }

    /// `FUN_001b51e0`: add (or, negative, remove) experience; a sum of 10,000,000 or more becomes
    /// 9,999,999. The routine does not floor at 0 (its callers check first).
    pub fn add_experience(&mut self, delta: i32) {
        let sum = self.experience().wrapping_add(delta);
        self.set_u32(off::EXPERIENCE, (if sum < 10_000_000 { sum } else { MAX_EXPERIENCE }) as u32);
    }

    fn set_experience(&mut self, v: i32) {
        self.set_u32(off::EXPERIENCE, v.min(MAX_EXPERIENCE) as u32);
    }

    /// `KH2_STATE` key `munny`: set the wallet, held at the prize limit.
    fn set_wallet(&mut self, v: u32) {
        self.wallet = v.min(MAX_WALLET);
    }

    /// The option word (save `+0x31b0`).
    pub fn option_word(&self) -> u32 {
        self.config[0]
    }

    pub fn set_option_word(&mut self, w: u32) {
        self.config[0] = w;
    }

    /// The game mode (save `+0x1658`).
    pub fn game_mode(&self) -> u32 {
        self.config[1]
    }

    /// The prize munny (save `+0x1600`, see the field).
    pub fn wallet(&self) -> u32 {
        self.wallet
    }

    /// `FUN_0023e620`: add `n`; when the unsigned sum is not below 1,000,000 (`FUN_0023e5e0`) the
    /// counter becomes 999,999. Returns the new value.
    pub fn add_to_wallet(&mut self, n: u32) -> u32 {
        let sum = self.wallet.wrapping_add(n);
        self.wallet = if sum < MAX_WALLET + 1 { sum } else { MAX_WALLET };
        self.wallet
    }

    // --- inventory ----------------------------------------------------------------------------

    /// `FUN_001a1cc8`: how many of an item the player owns. A key item answers 0 or 1 (its bit);
    /// an ability (type 0x13) is not counted here (0); everything else is the count byte of the
    /// item's slot. A spell's count is its level.
    pub fn item_count(&self, id: u16) -> u8 {
        let Some(it) = self.items.get(id) else { return 0 };
        if it.is_key() {
            self.bit(off::KEYS, off::KEY_WORDS, it.slot) as u8
        } else if it.kind != item_type::ABILITY {
            self.count_at(it.slot)
        } else {
            0
        }
    }

    fn count_at(&self, slot: u16) -> u8 {
        if (slot as usize) < off::COUNT_SLOTS { self.u8_at(off::COUNTS + slot as usize) } else { 0 }
    }

    fn set_count_at(&mut self, slot: u16, v: u8) {
        if (slot as usize) < off::COUNT_SLOTS {
            self.set_u8(off::COUNTS + slot as usize, v);
        }
    }

    /// The level of a magic slot (0 Fire, 1 Blizzard, 2 Thunder, 3 Cure, 4 Magnet, 5 Reflect): the
    /// count of the slot's `magc` item (`FUN_001a4c78`; 0 = not owned). A slot without an item
    /// answers 0 (its base row is then used regardless, `magic_rules::pick_row`).
    pub fn spell_level(&self, slot: u8) -> u8 {
        match self.items.magic_items.get(slot as usize) {
            Some(&i) if i != 0 => self.item_count(i),
            _ => 0,
        }
    }

    pub fn spell_levels(&self) -> [u8; 6] {
        std::array::from_fn(|s| self.spell_level(s as u8))
    }

    /// `FUN_001a1768(id, to, n)`: where `n` copies of an item would go. `to` is a party member id
    /// or `INVENTORY`. Answers `to` when the member's record takes it (a consumable into a free
    /// battle item slot, an ability into the ability list), `INVENTORY` when the count can grow
    /// (to 99 at most; a key item only once), 0 when the item is refused.
    pub fn room(&self, id: u16, to: u16, n: u8) -> u16 {
        let Some(it) = self.items.get(id).copied() else { return 0 };
        if it.is_key() {
            return if self.bit(off::KEYS, off::KEY_WORDS, it.slot) || n != 1 { 0 } else { INVENTORY };
        }
        if to != INVENTORY
            && n == 1
            && let Some(r) = Self::member_off(to)
        {
            if it.kind == item_type::CONSUMABLE {
                let slots = self.u8_at(r + rec::ITEM_SLOTS) as usize;
                for i in 0..slots {
                    if self.u16_at(r + rec::ITEMS + 2 * i) == 0 {
                        let reserved = self.u16_at(r + rec::RESERVED + 2 * i);
                        if reserved == id || reserved == 0 {
                            return to;
                        }
                    }
                }
            } else if it.kind == item_type::ABILITY {
                return to;
            }
        }
        if self.count_at(it.slot) as u32 + n as u32 > MAX_COUNT as u32 { 0 } else { INVENTORY }
    }

    /// `FUN_001a18d8(id, to, equipped)`: give one item to a party member or the inventory.
    /// Consumables fill the member's battle item slots first, abilities go into the member's
    /// list (`equipped` sets bit 15), everything else is counted in the inventory. Sets the
    /// "obtained" bit of a non-key item. False when refused.
    pub fn give(&mut self, id: u16, to: u16, equipped: bool) -> bool {
        let Some(it) = self.items.get(id).copied() else { return false };
        let target = self.room(id, to, 1);
        if target == 0 {
            return false;
        }
        if !it.is_key() && target != INVENTORY {
            if let Some(r) = Self::member_off(target) {
                if it.kind == item_type::CONSUMABLE {
                    self.put_battle_item(r, id);
                    if target == 1 || target == 0xe {
                        self.note_recent(id);
                    }
                } else if it.kind == item_type::ABILITY {
                    self.put_ability(r, id, equipped);
                }
            }
        } else {
            self.add_count(id, 1);
        }
        if !it.is_key() {
            self.set_bit(off::OBTAINED, off::OBTAINED_WORDS, it.slot, true);
        }
        true
    }

    /// Add `n` copies to the inventory, one `give` each (the original's own entry point); returns
    /// how many were taken (a full count or an owned key item stops it).
    pub fn add_item(&mut self, id: u16, n: u8) -> u8 {
        (0..n).take_while(|_| self.give(id, INVENTORY, false)).count() as u8
    }

    /// `FUN_001a1c30(id, n)`: remove `n` copies (to 0 at least); a key item loses its bit.
    pub fn use_item(&mut self, id: u16, n: u8) {
        let Some(it) = self.items.get(id).copied() else { return };
        if it.is_key() {
            self.set_bit(off::KEYS, off::KEY_WORDS, it.slot, false);
        } else {
            let c = self.count_at(it.slot);
            self.set_count_at(it.slot, c.saturating_sub(n));
        }
    }

    /// Set a count directly (the state file; no side effects). Key items: set or clear the bit.
    pub fn set_count(&mut self, id: u16, n: u8) {
        let Some(it) = self.items.get(id).copied() else { return };
        if it.is_key() {
            self.set_bit(off::KEYS, off::KEY_WORDS, it.slot, n > 0);
        } else if it.kind != item_type::ABILITY {
            self.set_count_at(it.slot, n.min(MAX_COUNT));
        }
    }

    /// `FUN_001a1a18(id, n)`: count the item (or set its key bit), then the type's side effect:
    /// a consumable is noted (`FUN_001a2120`); a form item makes its form take a keyblade
    /// (`FUN_001a2010`); a spell obtained for the first time puts its command into the first
    /// empty shortcut 1..3. Not ported: item 0x20's `FUN_001803b0` and the spell's tutorial
    /// prompts `FUN_00180450` (both act outside this block).
    fn add_count(&mut self, id: u16, n: u8) {
        let Some(it) = self.items.get(id).copied() else { return };
        let mut before = 0;
        if it.is_key() {
            self.set_bit(off::KEYS, off::KEY_WORDS, it.slot, true);
        } else {
            before = self.count_at(it.slot);
            self.set_count_at(it.slot, before.wrapping_add(n));
        }
        match it.kind {
            item_type::CONSUMABLE => self.note_recent(id),
            item_type::DRIVE_FORM => self.form_takes_keyblade(&it),
            item_type::MAGIC if before == 0 => {
                for k in 1..4 {
                    if self.u16_at(off::SHORTCUTS + 2 * k) == 0 {
                        self.set_u16(off::SHORTCUTS + 2 * k, it.command);
                        break;
                    }
                }
            }
            _ => {}
        }
    }

    /// `FUN_001a2120`: the first consumable also becomes shortcut 0 (its command,
    /// `FUN_001a1f78`); the id joins the 8-entry list if not there.
    fn note_recent(&mut self, id: u16) {
        if self.u16_at(off::RECENT) == 0 && self.u16_at(off::SHORTCUTS) == 0 {
            let cmd = self.shortcut_command(id);
            self.set_u16(off::SHORTCUTS, cmd);
        }
        for i in 0..8 {
            let v = self.u16_at(off::RECENT + 2 * i);
            if v == 0 {
                self.set_u16(off::RECENT + 2 * i, id);
                return;
            }
            if v == id {
                return;
            }
        }
    }

    /// `FUN_001a1f78`: a consumable's command; a spell's command comes from the loaded magic
    /// resources (`FUN_001a4dd0`, empty until an area loads them), answered 0 here.
    fn shortcut_command(&self, id: u16) -> u16 {
        match self.items.get(id) {
            Some(it) if it.kind == item_type::CONSUMABLE => it.command,
            _ => 0,
        }
    }

    /// `FUN_001a2010`: obtaining a form item resets the Anti counter (form record 5 `+4`) and,
    /// for Valor, Master and Final with no second keyblade yet, takes the first keyblade the
    /// inventory holds (`FUN_001a20b0(2)`, table order) for the form's left hand.
    fn form_takes_keyblade(&mut self, it: &ItemInfo) {
        self.set_form_exp(5, 0);
        let form = self.items.cmd_arg.get(&it.command).copied().unwrap_or(0);
        if matches!(form, 1 | 3 | 4) && self.form(form as u8).weapon == 0 {
            let taken = self.take_first_of_type(item_type::KEYBLADE);
            self.set_form_weapon(form as u8, taken);
        }
    }

    /// `FUN_001a20b0(type)`: the first item of the type (table order) with a count, minus one.
    fn take_first_of_type(&mut self, kind: u8) -> u16 {
        let found = self.items.rows.iter().find(|r| r.kind == kind && self.item_count(r.id) >= 1).map(|r| r.id);
        match found {
            Some(id) => {
                self.use_item(id, 1);
                id
            }
            None => 0,
        }
    }

    /// `FUN_00187728`: a consumable into the member's first free battle item slot whose reserved
    /// id is this item or empty (an empty reserved slot takes the id when another slot reserves it).
    fn put_battle_item(&mut self, r: usize, id: u16) {
        let n = self.u8_at(r + rec::ITEM_SLOTS) as usize;
        for i in 0..n {
            if self.u16_at(r + rec::ITEMS + 2 * i) != 0 {
                continue;
            }
            let reserved = self.u16_at(r + rec::RESERVED + 2 * i);
            if reserved == id {
                self.set_u16(r + rec::ITEMS + 2 * i, id);
                return;
            }
            if reserved == 0 {
                if (0..n).any(|j| self.u16_at(r + rec::RESERVED + 2 * j) == id) {
                    self.set_u16(r + rec::RESERVED + 2 * i, id);
                }
                self.set_u16(r + rec::ITEMS + 2 * i, id);
                return;
            }
        }
    }

    /// A member's battle item slots as (item, reserved id) pairs, `+0x34` and `+0x44`, for the first
    /// `+0x12` slots (the field Items list `FUN_001bc910` reads both).
    pub fn battle_items(&self, member: u16) -> Vec<(u16, u16)> {
        let Some(r) = Self::member_off(member) else { return Vec::new() };
        let n = (self.u8_at(r + rec::ITEM_SLOTS) as usize).min(8);
        (0..n).map(|i| (self.u16_at(r + rec::ITEMS + 2 * i), self.u16_at(r + rec::RESERVED + 2 * i))).collect()
    }

    /// `FUN_001a1ba8(id, to)`: take one item away. `to == INVENTORY`: one off the stock count
    /// (`FUN_001a1c30(id, 1)`), always true. A member: the LAST of its battle item slots that holds
    /// the item is emptied (the reserved id stays); false when none holds it. Using an item from the
    /// field menu takes it this way from the user's slots (`FUN_00175ec0` -> here with the user).
    pub fn take_item(&mut self, id: u16, to: u16) -> bool {
        if to == INVENTORY {
            self.use_item(id, 1);
            return true;
        }
        let Some(r) = Self::member_off(to) else { return false };
        let n = (self.u8_at(r + rec::ITEM_SLOTS) as usize).min(8);
        for i in (0..n).rev() {
            if self.u16_at(r + rec::ITEMS + 2 * i) == id {
                self.set_u16(r + rec::ITEMS + 2 * i, 0);
                return true;
            }
        }
        false
    }

    /// `FUN_001877c8`: every empty battle item slot that reserves an item takes one from the stock
    /// when the stock has it (`FUN_001a1cc8` >= 1, then `FUN_001a1c30(id, 1)`).
    pub fn refill_battle_items(&mut self, member: u16) {
        let Some(r) = Self::member_off(member) else { return };
        let n = (self.u8_at(r + rec::ITEM_SLOTS) as usize).min(8);
        for i in 0..n {
            let reserved = self.u16_at(r + rec::RESERVED + 2 * i);
            if reserved != 0 && self.u16_at(r + rec::ITEMS + 2 * i) == 0 && self.item_count(reserved) >= 1 {
                self.use_item(reserved, 1);
                self.set_u16(r + rec::ITEMS + 2 * i, reserved);
            }
        }
    }

    /// `FUN_0026c698(slot, item)` (pause menu, Items category): the slot's item goes back to the stock
    /// (`FUN_001a18d8(old, 100, 0)`); then, when `item` is 0 or `carried`, the reserved id is cleared, the
    /// slot takes `item` and one comes off the stock (`FUN_001a1ba8(item, 100)`); otherwise the item
    /// only becomes the slot's reserved id and the slot stays empty. `carried` is the menu row's flag
    /// (the byte `(row - 1) * 6 + 2` of the page's list table); which rows set it was not traced
    /// [inferred: a stock count above 0]. False when the member or the slot does not exist.
    pub fn equip_battle_item(&mut self, member: u16, slot: usize, item: u16, carried: bool) -> bool {
        let Some(r) = Self::member_off(member) else { return false };
        if slot >= 8 {
            return false;
        }
        let o = r + rec::ITEMS + 2 * slot;
        let old = self.u16_at(o);
        if old != 0 {
            self.give(old, INVENTORY, false);
        }
        if item == 0 || carried {
            self.set_u16(r + rec::RESERVED + 2 * slot, 0);
            self.set_u16(o, item);
            if item != 0 {
                self.take_item(item, INVENTORY);
            }
        } else {
            self.set_u16(r + rec::RESERVED + 2 * slot, item);
            self.set_u16(o, 0);
        }
        true
    }

    /// The 8 consumable ids Sora received (`0x340e50`, `FUN_001a2120` fills it); the field menu shows
    /// its Items entry only once the first is set (`FUN_001bbca0`).
    pub fn recent_items(&self) -> [u16; 8] {
        std::array::from_fn(|i| self.u16_at(off::RECENT + 2 * i))
    }

    /// `FUN_00187868`: an ability into the first empty slot of the member's list.
    fn put_ability(&mut self, r: usize, id: u16, equipped: bool) {
        for i in 0..rec::ABILITY_SLOTS {
            let o = r + rec::ABILITIES + 2 * i;
            if self.u16_at(o) & 0x7fff == 0 {
                self.set_u16(o, id & 0x7fff | (equipped as u16) << 15);
                return;
            }
        }
    }

    /// The member's ability list with the raw slot index (0..64) of each non-empty slot: the menu's
    /// rows keep the slot (`FUN_0027a7e8` stores the loop index in the row) because the toggle
    /// writes that slot (`FUN_0027d8b8`).
    pub fn ability_slots(&self, member: u16) -> Vec<(usize, u16, bool)> {
        let Some(r) = Self::member_off(member) else { return Vec::new() };
        (0..rec::ABILITY_SLOTS)
            .map(|i| (i, self.u16_at(r + rec::ABILITIES + 2 * i)))
            .filter(|&(_, v)| v & 0x7fff != 0)
            .map(|(i, v)| (i, v & 0x7fff, v & 0x8000 != 0))
            .collect()
    }

    /// `FUN_0027d8b8`: the equipped flag (bit 15) of one slot of a member's ability list is written
    /// as `(old & 0x7fff) | on << 15`. False when the member or the slot does not exist.
    pub fn set_ability_equipped(&mut self, member: u16, slot: usize, on: bool) -> bool {
        let Some(r) = Self::member_off(member) else { return false };
        if slot >= rec::ABILITY_SLOTS {
            return false;
        }
        let o = r + rec::ABILITIES + 2 * slot;
        let old = self.u16_at(o);
        self.set_u16(o, old & 0x7fff | (on as u16) << 15);
        true
    }

    /// `FUN_00187f18`: an ability into the first empty slot of a form's list, always equipped.
    fn put_form_ability(&mut self, r: usize, v: u16) {
        for i in 0..16 {
            let o = r + 8 + 2 * i;
            let old = self.u16_at(o);
            if old & 0x7fff == 0 {
                self.set_u16(o, old & 0x8000 | v & 0x7fff | 0x8000);
                return;
            }
        }
    }

    /// `FUN_00187658`: change a member's number of battle item slots; slots that disappear give
    /// their item back to the inventory.
    fn set_item_slots(&mut self, r: usize, n: u8) {
        let cur = self.u8_at(r + rec::ITEM_SLOTS);
        if cur < n {
            for i in cur as usize..n as usize {
                self.set_u16(r + rec::ITEMS + 2 * i, 0);
                self.set_u16(r + rec::RESERVED + 2 * i, 0);
            }
        } else if n < cur {
            for i in n as usize..cur as usize {
                let v = self.u16_at(r + rec::ITEMS + 2 * i);
                if v != 0 {
                    self.give(v, INVENTORY, false);
                    self.set_u16(r + rec::ITEMS + 2 * i, 0);
                }
            }
        }
        self.set_u8(r + rec::ITEM_SLOTS, n);
    }

    /// Keyblades in the inventory, one id per copy, in table order.
    pub fn keyblades(&self) -> Vec<u16> {
        self.items
            .rows
            .iter()
            .filter(|r| r.kind == item_type::KEYBLADE)
            .flat_map(|r| std::iter::repeat_n(r.id, self.item_count(r.id) as usize))
            .collect()
    }

    /// Sora's equipped items: the weapons, then the armor and accessory slots (non-empty).
    pub fn equipment(&self) -> Vec<u16> {
        let s = self.sora();
        s.weapons.iter().chain(&s.armor).chain(&s.accessories).copied().filter(|&i| i != 0).collect()
    }

    /// Everything owned in the inventory as (item id, count), in table order.
    pub fn inventory(&self) -> Vec<(u16, u8)> {
        self.items.rows.iter().map(|r| (r.id, self.item_count(r.id))).filter(|&(_, c)| c > 0).collect()
    }

    // --- the new game -------------------------------------------------------------------------

    /// The new-game state: the party block after `FUN_00187b38` on the cleared save block and the
    /// `plrp` rows with `Id` 0 (`FUN_001c0cb0(0)`).
    pub fn new_game(items: Arc<ItemIndex>, plrp: &[Plrp]) -> Progress {
        let mut p = Progress { block: vec![0; off::LEN], items, source: Source::NewGame, baseline: Vec::new(), dodge_roll: 0, wallet: 0, config: [OPTION_DEFAULT, GAME_MODE_DEFAULT] };
        p.init_party();
        p.apply_plrp(plrp, 0);
        p
    }

    /// `FUN_00187b38(0x33fec0)`. The stores of zero are kept so the port reads like the routine.
    fn init_party(&mut self) {
        for m in 0..off::MEMBERS {
            self.init_member(m * off::MEMBER_SIZE);
        }
        for f in 1..=off::FORMS {
            // FUN_00187ed0: weapon 0, level 1, +3 = 1, experience 0, 16 empty ability slots.
            let r = off::FORM_BASE + f * off::FORM_SIZE;
            self.set_u16(r, 0);
            self.set_u8(r + 3, 1);
            self.set_u8(r + 2, 1);
            self.set_u32(r + 4, 0);
            self.zero(r + 8, 32);
        }
        self.set_u8(off::CURRENT_FORM, 0);
        self.set_u8(off::DRIVE + 2, 3);
        self.set_u8(off::SUMMON_LEVEL, 1);
        self.set_u8(off::DRIVE, 100);
        self.set_u8(off::SUMMON, 0);
        self.set_u8(off::DRIVE + 1, 3);
        for w in 0..off::WORLD_COUNT {
            // FUN_001a71a8: {0, 0x10, 0x10, 0x10}.
            let o = off::WORLDS + 4 * w;
            self.block[o..o + 4].copy_from_slice(&[0, 0x10, 0x10, 0x10]);
        }
        self.zero(off::KEYS, 4 * off::KEY_WORDS);
        self.set_u32(off::EXPERIENCE, 0);
        self.zero(0xfac, 32);
        self.zero(0xfcc, 32);
        self.set_u32(off::SPELL_SWITCH, 0);
        self.zero(off::OBTAINED, 4 * off::OBTAINED_WORDS);
        self.set_u32(0x1658, 0);
        for k in 0..32 {
            let o = 0x165c + 8 * k + 2;
            let v = self.u16_at(o) & 0x8000;
            self.set_u16(o, v);
        }
        for k in 0..64 {
            let o = 0x1760 + 8 * k;
            self.set_u8(o, 0);
            self.set_u32(o + 4, 0);
        }
        self.zero(0x1960, 16);
        self.zero(0x197a, 26);
        self.set_u16(0x1994, 0);
        self.zero(off::SHORTCUTS, 8);
    }

    /// `FUN_001875c8`: a member record's defaults.
    fn init_member(&mut self, r: usize) {
        self.set_u8(r + rec::STRENGTH, 0);
        self.set_u8(r + rec::MP_MAX, 100);
        self.set_u8(r + rec::ACCESSORY_SLOTS, 1);
        self.set_u8(r + rec::HP, 100);
        self.set_u8(r + rec::HP_MAX, 100);
        self.set_u8(r + rec::MP, 100);
        for o in [rec::MAGIC, rec::DEFENSE, 0xc, 0xd, rec::WEAPON_SET] {
            self.set_u8(r + o, 0);
        }
        self.set_u8(r + rec::LEVEL, 1);
        self.set_u8(r + rec::ARMOR_SLOTS, 1);
        self.set_u8(r + rec::ITEM_SLOTS, 0);
        self.zero(r + rec::ABILITIES, 2 * rec::ABILITY_SLOTS);
        self.zero(r + 0xd4, 32);
    }

    /// `FUN_001c0cb0(id)`: the first `plrp` row with this `Id` and the rows after it with the same
    /// `Id`, each through `FUN_001c0b80`.
    fn apply_plrp(&mut self, plrp: &[Plrp], id: u16) {
        let Some(first) = plrp.iter().position(|r| r.id == id) else { return };
        for row in plrp[first..].iter().take_while(|r| r.id == id) {
            self.apply_plrp_row(row);
        }
    }

    /// `FUN_001c0b80`: a row for a party member (`Character` < 0x80) sets the record's HP, MP, AP,
    /// strength, magic, defense and slot counts, then gives each item (bit 15 = equipped); a row
    /// for a form (`Character` 0x80 | form) adds each item to the form's ability list.
    fn apply_plrp_row(&mut self, row: &Plrp) {
        let c = row.character;
        let (member, form) = if c & 0x80 == 0 { (Self::member_off(c as u16), None) } else { (None, Self::form_off(c & 0x7f)) };
        if let Some(r) = member {
            self.set_u8(r + rec::HP_MAX, row.hp);
            self.set_u8(r + rec::HP, row.hp);
            self.set_u8(r + rec::MP_MAX, row.mp);
            self.set_u8(r + rec::MP, row.mp);
            self.set_u8(r + rec::AP, row.ap);
            self.set_u8(r + rec::STRENGTH, row.strength);
            self.set_u8(r + rec::MAGIC, row.magic);
            self.set_u8(r + rec::DEFENSE, row.defense);
            self.set_u8(r + rec::ARMOR_SLOTS, row.armor_slot_max);
            self.set_u8(r + rec::ACCESSORY_SLOTS, row.accessory_slot_max);
            self.set_item_slots(r, row.item_slot_max);
        }
        for &v in row.items.iter().filter(|&&v| v != 0) {
            match form {
                None => {
                    self.give(v & 0x7fff, c as u16, v >> 15 != 0);
                }
                Some(f) => self.put_form_ability(f, v),
            }
        }
    }

    // --- start state --------------------------------------------------------------------------

    /// The app baseline on top of the new game (see the module doc). Records what it adds.
    pub fn apply_baseline(&mut self, guard: bool) {
        let mut added = Vec::new();
        let missing: Vec<u8> = (1..=4u8).filter(|&f| !self.owns_form(f) && self.items.form_items[f as usize] != 0).collect();
        // The forms that take a keyblade when their item is obtained (FUN_001a2010) find one
        // spare Kingdom Key each, as the app always had.
        let takers = missing.iter().filter(|&&f| matches!(f, 1 | 3 | 4) && self.form(f).weapon == 0).count() as u8;
        if takers > 0 {
            let n = self.add_item(KINGDOM_KEY, takers);
            added.push(format!("{n} spare Kingdom Key (item {KINGDOM_KEY})"));
        }
        for f in missing {
            let item = self.items.form_items[f as usize];
            if self.add_item(item, 1) == 1 {
                added.push(format!("form {f} item {item}"));
            }
        }
        for slot in 0..4u8 {
            let item = self.items.magic_items[slot as usize];
            if item != 0 && self.spell_level(slot) == 0 && self.add_item(item, 1) == 1 {
                added.push(format!("spell slot {slot} item {item} level 1"));
            }
        }
        if let Some(r) = Self::member_off(SORA) {
            if self.u16_at(r + rec::WEAPONS) == 0 {
                self.set_u16(r + rec::WEAPONS, KINGDOM_KEY);
                added.push(format!("Kingdom Key (item {KINGDOM_KEY}) equipped"));
            }
            let guard_id = crate::abilities::id::GUARD;
            if guard && !self.sora().abilities.iter().any(|&(id, _)| id == guard_id) {
                self.put_ability(r, guard_id, true);
                added.push(format!("Guard (item {guard_id}) equipped"));
            }
        }
        // The user's wish (2026-10-06): Sora knows the Dodge Roll. It goes with the Guard switch:
        // `KH2_ABILITY_START=data` (no Guard) starts without it too, so the original's rules are reachable.
        if guard && self.dodge_roll == 0 {
            self.dodge_roll = 1;
            added.push("Dodge Roll growth level 1".to_string());
        }
        self.baseline = added;
    }

    /// Applies a state file (schema in docs/progress.md) on top of this state.
    pub fn apply_json(&mut self, text: &str) -> Result<(), String> {
        let j = Json::parse(text)?;
        let Json::Obj(fields) = &j else { return Err("the state must be a JSON object".into()) };
        let int = |v: &Json, what: &str| v.int().ok_or_else(|| format!("{what}: expected an integer"));
        let id_of = |k: &str, what: &str| k.trim().parse::<u16>().map_err(|_| format!("{what}: '{k}' is not an id"));
        let list = |v: &Json, what: &str| -> Result<Vec<u16>, String> {
            match v {
                Json::Arr(a) => a.iter().map(|x| int(x, what).map(|n| n as u16)).collect(),
                _ => Err(format!("{what}: expected an array")),
            }
        };
        for (key, v) in fields {
            match key.as_str() {
                "experience" => self.set_experience(int(v, "experience")? as i32),
                "munny" => self.set_wallet(int(v, "munny")?.max(0) as u32),
                "items" => {
                    let Json::Obj(m) = v else { return Err("items: expected an object of id: count".into()) };
                    for (k, n) in m {
                        self.set_count(id_of(k, "items")?, int(n, "items")?.clamp(0, 255) as u8);
                    }
                }
                "magic" => {
                    for (slot, n) in list(v, "magic")?.into_iter().enumerate().take(6) {
                        let item = self.items.magic_items[slot];
                        self.set_count(item, n.min(255) as u8);
                    }
                }
                "weapon" => {
                    let r = Self::member_off(SORA).unwrap();
                    self.set_u16(r + rec::WEAPONS, int(v, "weapon")? as u16);
                }
                "armor" | "accessories" => {
                    let (count, base) = if key == "armor" { (rec::ARMOR_SLOTS, rec::ARMOR) } else { (rec::ACCESSORY_SLOTS, rec::ACCESSORIES) };
                    let ids = list(v, key)?;
                    let r = Self::member_off(SORA).unwrap();
                    let n = (ids.len().min(8) as u8).max(self.u8_at(r + count));
                    self.set_u8(r + count, n);
                    self.zero(r + base, 16);
                    for (i, id) in ids.iter().take(8).enumerate() {
                        self.set_u16(r + base + 2 * i, *id);
                    }
                }
                "abilities" | "abilities_off" => {}
                // Each id is given to Sora through the original's give (`FUN_001a18d8(id, 1, 0)`, what a
                // pickup does): a consumable fills his first free battle item slot (else the stock) and
                // joins the received list.
                "sora_items" => {
                    for id in list(v, "sora_items")? {
                        self.give(id, SORA, false);
                    }
                }
                "dodge_roll" => self.dodge_roll = int(v, "dodge_roll")?.clamp(0, 5) as u8,
                "forms" => {
                    let Json::Obj(m) = v else { return Err("forms: expected an object of form id: {...}".into()) };
                    for (k, f) in m {
                        let form = id_of(k, "forms")? as u8;
                        if Self::form_off(form).is_none() {
                            return Err(format!("forms: no form record {form}"));
                        }
                        if f.get("owned").is_some_and(|o| *o == Json::Bool(true)) && !self.owns_form(form) {
                            let item = self.items.form_items.get(form as usize).copied().unwrap_or(0);
                            self.add_item(item, 1);
                        }
                        if let Some(l) = f.get("level") {
                            self.set_form_level(form, int(l, "forms.level")? as u8);
                        }
                        if let Some(e) = f.get("exp") {
                            self.set_form_exp(form, int(e, "forms.exp")? as u32);
                        }
                        if let Some(w) = f.get("weapon") {
                            self.set_form_weapon(form, int(w, "forms.weapon")? as u16);
                        }
                    }
                }
                "summon" => {
                    if let Some(l) = v.get("level") {
                        self.set_u8(off::SUMMON_LEVEL, int(l, "summon.level")? as u8);
                    }
                    if let Some(e) = v.get("exp") {
                        self.set_u32(off::SUMMON_EXP, int(e, "summon.exp")? as u32);
                    }
                }
                "outfits" => {
                    let Json::Obj(m) = v else { return Err("outfits: expected an object of world: outfit".into()) };
                    for (k, o) in m {
                        self.set_outfit(id_of(k, "outfits")? as usize, int(o, "outfits")? as u8);
                    }
                }
                "spell_switch" => self.set_u32(off::SPELL_SWITCH, int(v, "spell_switch")? as u32),
                "option_word" => self.config[0] = int(v, "option_word")? as u32,
                "game_mode" => self.config[1] = int(v, "game_mode")?.clamp(0, 2) as u32,
                "comment" | "_comment" => {}
                other => return Err(format!("unknown field '{other}'")),
            }
        }
        // Sora's ability list: replaced as a whole when either list is given.
        if j.get("abilities").is_some() || j.get("abilities_off").is_some() {
            let on = j.get("abilities").map(|v| list(v, "abilities")).transpose()?.unwrap_or_default();
            let off_list = j.get("abilities_off").map(|v| list(v, "abilities_off")).transpose()?.unwrap_or_default();
            let r = Self::member_off(SORA).unwrap();
            self.zero(r + rec::ABILITIES, 2 * rec::ABILITY_SLOTS);
            for id in on {
                self.put_ability(r, id, true);
            }
            for id in off_list {
                self.put_ability(r, id, false);
            }
        }
        Ok(())
    }

    /// Reads the tables from the disc and builds the start state (new game, then `KH2_STATE` or
    /// the app baseline).
    pub fn start(disc: &Disc) -> Option<Progress> {
        let (items, cmds, magc, plrp) = {
            let mut a = disc.0.lock().ok()?;
            (
                kh2_tables::load::<Item>(&mut a, &t::ITEM).ok()?.rows,
                kh2_tables::load::<Cmd>(&mut a, &t::CMD).ok()?.rows,
                kh2_tables::load::<Magc>(&mut a, &t::MAGC).ok()?.rows,
                kh2_tables::load::<Plrp>(&mut a, &t::PLRP).ok()?.rows,
            )
        };
        let index = Arc::new(ItemIndex::new(&items, &cmds, &magc));
        let mut p = Progress::new_game(index.clone(), &plrp);
        match std::env::var("KH2_STATE") {
            Ok(path) if !path.is_empty() => match std::fs::read_to_string(&path).map_err(|e| e.to_string()).and_then(|s| p.apply_json(&s)) {
                Ok(()) => p.source = Source::File(path),
                Err(e) => {
                    // A file that failed half way is dropped as a whole.
                    warn!("progress: KH2_STATE={path}: {e}; using the new game and the app baseline");
                    p = Progress::new_game(index, &plrp);
                    p.apply_baseline(std::env::var("KH2_ABILITY_START").as_deref() != Ok("data"));
                }
            },
            _ => p.apply_baseline(std::env::var("KH2_ABILITY_START").as_deref() != Ok("data")),
        }
        Some(p)
    }

    /// The `KH2_PROGRESSLOG` line.
    pub fn report(&self) -> String {
        let from = match &self.source {
            Source::NewGame if self.baseline.is_empty() => "new game".to_string(),
            Source::NewGame => "new game + app baseline".to_string(),
            Source::File(p) => p.clone(),
        };
        let s = self.sora();
        let owned: Vec<u8> = (1..=5).filter(|&f| self.owns_form(f)).collect();
        let levels: Vec<u8> = (1..=5).map(|f| self.form_level(f)).collect();
        let fw: Vec<u16> = (1..=4).map(|f| self.form(f).weapon).collect();
        let abil: Vec<String> = s.abilities.iter().map(|&(id, on)| if on { id.to_string() } else { format!("{id} off") }).collect();
        let inv: Vec<String> = self.inventory().iter().map(|&(id, n)| format!("{id}x{n}")).collect();
        format!(
            "progress: state from {from}: experience {}; munny {}; spells {:?}; forms owned {owned:?} levels {levels:?} keyblades {fw:?}; \
             weapon {:?} armor {:?} accessories {:?}; abilities [{}]; HP {}/{} MP {}/{}; summon level {} exp {}; \
             outfit (world 2) {}; spell switch {:#x}; option word {:#x}; inventory [{}]",
            self.experience(),
            self.wallet(),
            self.spell_levels(),
            s.weapons,
            s.armor,
            s.accessories,
            abil.join(", "),
            s.hp,
            s.hp_max,
            s.mp,
            s.mp_max,
            self.summon_level(),
            self.summon_exp(),
            self.outfit(2),
            self.spell_switch_word(),
            self.option_word(),
            inv.join(" "),
        )
    }
}

fn log_on() -> bool {
    std::env::var("KH2_PROGRESSLOG").is_ok_and(|v| v != "0")
}

/// Inserts `Progress` if it is not there yet (needs `Disc`). The plugins that read it while they
/// build (abilities, drive) call this, so the order of the plugin tuples does not matter.
pub fn ensure(app: &mut App) -> bool {
    if app.world().contains_resource::<Progress>() {
        return true;
    }
    let Some(p) = app.world().get_resource::<Disc>().and_then(Progress::start) else {
        warn!("progress: tables not readable; the systems use their own defaults");
        return false;
    };
    if log_on() {
        info!("{}", p.report());
        if !p.baseline.is_empty() {
            info!("progress: app baseline (not the original's new game, docs/progress.md): {}", p.baseline.join("; "));
        }
    }
    app.insert_resource(p);
    true
}

pub struct ProgressPlugin;

impl Plugin for ProgressPlugin {
    fn build(&self, app: &mut App) {
        ensure(app);
    }
}

// --- a small JSON reader (the state file) ----------------------------------------------------

/// A JSON value. Objects keep their order.
#[derive(Clone, Debug, PartialEq)]
pub enum Json {
    Null,
    Bool(bool),
    Num(f64),
    Str(String),
    Arr(Vec<Json>),
    Obj(Vec<(String, Json)>),
}

impl Json {
    pub fn parse(s: &str) -> Result<Json, String> {
        let mut p = JsonParser { b: s.as_bytes(), i: 0 };
        let v = p.value()?;
        p.ws();
        if p.i != p.b.len() {
            return p.err("trailing characters");
        }
        Ok(v)
    }

    pub fn get(&self, k: &str) -> Option<&Json> {
        match self {
            Json::Obj(m) => m.iter().find(|(n, _)| n == k).map(|(_, v)| v),
            _ => None,
        }
    }

    pub fn int(&self) -> Option<i64> {
        match self {
            Json::Num(n) if n.fract() == 0.0 => Some(*n as i64),
            _ => None,
        }
    }
}

struct JsonParser<'a> {
    b: &'a [u8],
    i: usize,
}

impl JsonParser<'_> {
    fn err<T>(&self, what: &str) -> Result<T, String> {
        Err(format!("JSON: {what} at byte {}", self.i))
    }

    fn ws(&mut self) {
        while self.b.get(self.i).is_some_and(|c| c.is_ascii_whitespace()) {
            self.i += 1;
        }
    }

    fn eat(&mut self, c: u8) -> bool {
        self.ws();
        let hit = self.b.get(self.i) == Some(&c);
        if hit {
            self.i += 1;
        }
        hit
    }

    fn value(&mut self) -> Result<Json, String> {
        self.ws();
        match self.b.get(self.i) {
            None => self.err("unexpected end"),
            Some(b'{') => {
                self.i += 1;
                let mut m = Vec::new();
                if self.eat(b'}') {
                    return Ok(Json::Obj(m));
                }
                loop {
                    self.ws();
                    let k = self.string()?;
                    if !self.eat(b':') {
                        return self.err("expected ':'");
                    }
                    let v = self.value()?;
                    m.push((k, v));
                    if self.eat(b',') {
                        continue;
                    }
                    if self.eat(b'}') {
                        return Ok(Json::Obj(m));
                    }
                    return self.err("expected ',' or '}'");
                }
            }
            Some(b'[') => {
                self.i += 1;
                let mut a = Vec::new();
                if self.eat(b']') {
                    return Ok(Json::Arr(a));
                }
                loop {
                    a.push(self.value()?);
                    if self.eat(b',') {
                        continue;
                    }
                    if self.eat(b']') {
                        return Ok(Json::Arr(a));
                    }
                    return self.err("expected ',' or ']'");
                }
            }
            Some(b'"') => Ok(Json::Str(self.string()?)),
            Some(b't') => self.word("true", Json::Bool(true)),
            Some(b'f') => self.word("false", Json::Bool(false)),
            Some(b'n') => self.word("null", Json::Null),
            Some(_) => self.number(),
        }
    }

    fn word(&mut self, w: &str, v: Json) -> Result<Json, String> {
        if self.b[self.i..].starts_with(w.as_bytes()) {
            self.i += w.len();
            Ok(v)
        } else {
            self.err("unknown word")
        }
    }

    fn number(&mut self) -> Result<Json, String> {
        let start = self.i;
        while self.b.get(self.i).is_some_and(|c| c.is_ascii_digit() || matches!(c, b'-' | b'+' | b'.' | b'e' | b'E')) {
            self.i += 1;
        }
        std::str::from_utf8(&self.b[start..self.i]).ok().and_then(|s| s.parse::<f64>().ok()).map(Json::Num).map_or_else(|| self.err("bad number"), Ok)
    }

    fn string(&mut self) -> Result<String, String> {
        if self.b.get(self.i) != Some(&b'"') {
            return self.err("expected a string");
        }
        self.i += 1;
        let mut out: Vec<u8> = Vec::new();
        loop {
            let Some(&c) = self.b.get(self.i) else { return self.err("unterminated string") };
            self.i += 1;
            match c {
                b'"' => return Ok(String::from_utf8_lossy(&out).into_owned()),
                b'\\' => {
                    let Some(&e) = self.b.get(self.i) else { return self.err("unterminated escape") };
                    self.i += 1;
                    let ch = match e {
                        b'"' => '"',
                        b'\\' => '\\',
                        b'/' => '/',
                        b'n' => '\n',
                        b't' => '\t',
                        b'r' => '\r',
                        b'b' => '\u{8}',
                        b'f' => '\u{c}',
                        b'u' => {
                            let hex = self.b.get(self.i..self.i + 4).and_then(|h| std::str::from_utf8(h).ok()).and_then(|h| u32::from_str_radix(h, 16).ok());
                            let Some(h) = hex else { return self.err("bad \\u escape") };
                            self.i += 4;
                            char::from_u32(h).unwrap_or('\u{fffd}')
                        }
                        _ => return self.err("bad escape"),
                    };
                    let mut buf = [0u8; 4];
                    out.extend_from_slice(ch.encode_utf8(&mut buf).as_bytes());
                }
                _ => out.push(c),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `FUN_0027d8b8`: the toggle writes bit 15 of exactly the named slot, keeps the id bits, and
    /// leaves the other slots (and the empty slots between them) alone.
    #[test]
    fn ability_toggle_sets_and_clears_bit_15_of_the_right_slot() {
        let mut p = Progress::from_block(&[], Arc::new(ItemIndex::default()));
        let r = Progress::member_off(SORA).unwrap();
        p.put_ability(r, 82, true);
        p.put_ability(r, 162, false);
        p.put_ability(r, 394, false);
        // A gap: slot 1 emptied, so the next row's slot index differs from its position in the list.
        p.set_u16(r + rec::ABILITIES + 2, 0);
        assert_eq!(p.ability_slots(SORA), vec![(0, 82, true), (2, 394, false)]);
        assert!(p.set_ability_equipped(SORA, 2, true));
        assert_eq!(p.ability_slots(SORA), vec![(0, 82, true), (2, 394, true)]);
        assert_eq!(p.u16_at(r + rec::ABILITIES + 4), 394 | 0x8000);
        assert!(p.set_ability_equipped(SORA, 0, false));
        assert_eq!(p.ability_slots(SORA), vec![(0, 82, false), (2, 394, true)]);
        assert_eq!(p.u16_at(r + rec::ABILITIES), 82, "the id bits are kept");
        assert_eq!(p.sora().abilities, vec![(82, false), (394, true)]);
        assert!(!p.set_ability_equipped(SORA, 64, true), "slot 64 does not exist");
        assert!(!p.set_ability_equipped(99, 0, true), "no such member");
    }

    /// The Customize writers: the shortcut words and the spell switch bits, and the stat boost bytes.
    #[test]
    fn shortcuts_spell_switches_and_boosts_write_their_words() {
        let mut p = Progress::from_block(&[], Arc::new(ItemIndex::default()));
        assert_eq!(p.shortcuts(), [0; 4]);
        assert!(p.set_shortcut(2, 0x1234));
        assert_eq!((p.shortcut(2), p.shortcut(0), p.shortcut(4)), (0x1234, 0, 0));
        assert_eq!(p.u16_at(off::SHORTCUTS + 4), 0x1234, "slot 2 is the u16 at party block +0xfa0 + 4 (save +0x2604)");
        assert!(!p.set_shortcut(4, 1), "only four slots");
        // The switch: a set bit is "Not Used"; the toggle answers the state it left (true = it was set).
        assert!(!p.toggle_spell_switch(3), "was clear");
        assert!(p.spell_switch(3) && p.spell_switch_word() == 8);
        assert!(p.toggle_spell_switch(3), "was set");
        assert_eq!(p.spell_switch_word(), 0);
        p.set_spell_switch(5, true);
        p.set_spell_switch(0, true);
        p.set_spell_switch(5, false);
        assert_eq!(p.spell_switch_word(), 1);
        // The boosts add to the record byte and stop at 100.
        let r = Progress::member_off(SORA).unwrap();
        p.set_u8(r + rec::STRENGTH, 98);
        assert!(!p.boost_maxed(SORA, 0x114));
        assert_eq!(p.apply_boost(SORA, 0x114, 1), 1);
        assert_eq!(p.apply_boost(SORA, 0x114, 5), 1, "only the room up to 100 is added");
        assert_eq!(p.u8_at(r + rec::STRENGTH), 100);
        assert!(p.boost_maxed(SORA, 0x114));
        assert_eq!(p.apply_boost(SORA, 0x114, 1), 0);
        assert_eq!(p.apply_boost(SORA, 0x117, 2), 2);
        assert_eq!(p.u8_at(r + rec::AP), 2);
        assert_eq!(p.apply_boost(SORA, 0x118, 1), 0, "not a boost");
    }

    struct Tables {
        items: Vec<Item>,
        cmds: Vec<Cmd>,
        magc: Vec<Magc>,
        plrp: Vec<Plrp>,
        archive: kh2_archive::Archive,
    }

    fn tables() -> Option<Tables> {
        let iso = std::env::var("KH2_ISO").ok().or_else(|| {
            let f = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../local/iso.txt");
            std::fs::read_to_string(f).ok().map(|s| s.trim().to_string())
        });
        let Some(iso) = iso else {
            eprintln!("skipped: no disc (set KH2_ISO)");
            return None;
        };
        let mut a = kh2_archive::Archive::open(&iso).map_err(|e| eprintln!("skipped: {iso}: {e}")).ok()?;
        Some(Tables {
            items: kh2_tables::load::<Item>(&mut a, &t::ITEM).ok()?.rows,
            cmds: kh2_tables::load::<Cmd>(&mut a, &t::CMD).ok()?.rows,
            magc: kh2_tables::load::<Magc>(&mut a, &t::MAGC).ok()?.rows,
            plrp: kh2_tables::load::<Plrp>(&mut a, &t::PLRP).ok()?.rows,
            archive: a,
        })
    }

    fn index(tb: &Tables) -> Arc<ItemIndex> {
        Arc::new(ItemIndex::new(&tb.items, &tb.cmds, &tb.magc))
    }

    /// The oracle for the pause menu's Items and Customize writers (WP-M3): the game's own `FUN_001a4b68` /
    /// `FUN_001a4b30` / `FUN_001a4bb8` (the spell switch bit and the flip `FUN_00274e50` makes of it) and
    /// `FUN_001a1ba8(id, 100)` (the Stock page's "Throw away" and the take after "Use") run in kh2-ee on the
    /// block the game built; the port must leave the same bytes. The shortcut write `FUN_00274dd0` is one u16
    /// store at `+0x2600 + 2 * slot` of the save block, checked at that address.
    #[test]
    fn menu_writers_match_the_game() {
        use kh2_ee::Ee;
        const STACK: u32 = 0x01ff_0000;
        const ITEM_PTR: u32 = 0x01c9_5588;
        const PLRP_PTR: u32 = 0x01d5_df5c;
        const CMD_PTR: u32 = 0x01d5_d798;
        const MAGC_PTR: u32 = 0x01c9_5520;
        let Some(mut tb) = tables() else { return };
        let ix = index(&tb);
        let elf = ["SLUS_210.05", "SLUS_210.05;1"].iter().find_map(|p| tb.archive.iso().read(p).ok()).expect("executable");
        let blob = |a: &mut kh2_archive::Archive, d: &kh2_tables::TableDef| kh2_tables::subfile(a, d).expect("table");
        let tables = [
            (ITEM_PTR, 0x0180_0000, blob(&mut tb.archive, &t::ITEM)),
            (PLRP_PTR, 0x0181_0000, blob(&mut tb.archive, &t::PLRP)),
            (CMD_PTR, 0x0182_0000, blob(&mut tb.archive, &t::CMD)),
            (MAGC_PTR, 0x0183_0000, blob(&mut tb.archive, &t::MAGC)),
        ];
        let mut ee = Ee::new();
        ee.load_elf(&elf).expect("elf");
        for (ptr, at, data) in &tables {
            ee.write(*at, data);
            ee.w32(*ptr, *at);
        }
        let party = addr::PARTY;
        ee.write(addr::SAVE, &vec![0u8; addr::SAVE_LEN as usize]);
        ee.call(0x0018_7b38, &[party as u64], &[], STACK).expect("FUN_00187b38");
        ee.call(0x001c_0cb0, &[0], &[], STACK).expect("FUN_001c0cb0");
        let read = |ee: &mut Ee| (0..off::LEN as u32).map(|o| ee.r8(party + o)).collect::<Vec<u8>>();
        let mut port = Progress::new_game(ix.clone(), &tb.plrp);
        let diff = |a: &[u8], b: &[u8]| a.iter().zip(b).enumerate().filter(|(_, (x, y))| x != y).map(|(o, (x, y))| format!("+{o:#x}: game {x:#x} port {y:#x}")).take(12).collect::<Vec<_>>();
        assert_eq!(diff(&read(&mut ee), port.block()), Vec::<String>::new(), "start");

        // The spell switch: set, clear, and the page's flip (read FUN_001a4bb8, write the opposite).
        for bit in [0u8, 3, 5, 31, 2, 3, 0, 17] {
            ee.call(0x001a_4b68, &[bit as u64], &[], STACK).expect("FUN_001a4b68");
            port.set_spell_switch(bit, true);
            assert_eq!(diff(&read(&mut ee), port.block()), Vec::<String>::new(), "set bit {bit}");
            ee.call(0x001a_4b30, &[bit as u64], &[], STACK).expect("FUN_001a4b30");
            port.set_spell_switch(bit, false);
            assert_eq!(diff(&read(&mut ee), port.block()), Vec::<String>::new(), "clear bit {bit}");
        }
        let mut flips = Vec::new();
        for bit in [1u8, 4, 1, 1, 9, 4] {
            let state = ee.call(0x001a_4bb8, &[bit as u64], &[], STACK).expect("FUN_001a4bb8") as u8;
            // FUN_00274e50: state 0 (the bit is set) clears it, anything else sets it.
            ee.call(if state == 0 { 0x001a_4b30 } else { 0x001a_4b68 }, &[bit as u64], &[], STACK).expect("set or clear");
            let was_set = port.toggle_spell_switch(bit);
            assert_eq!(state == 0, was_set, "bit {bit}: the game read {state}");
            flips.push(!was_set);
            assert_eq!(diff(&read(&mut ee), port.block()), Vec::<String>::new(), "flip bit {bit}");
        }

        // The shortcut word: save +0x2600 is party block +0xfa0 (0x340e60); one u16 per slot.
        for (slot, cmd) in [(0usize, 23u16), (3, 0x1234), (1, 0)] {
            ee.write(0x0034_0e60 + 2 * slot as u32, &cmd.to_le_bytes());
            port.set_shortcut(slot, cmd);
        }
        assert_eq!(diff(&read(&mut ee), port.block()), Vec::<String>::new(), "shortcuts");

        // Throw away / the take after Use: FUN_001a1ba8(id, 100) over the owned items and some not owned.
        let owned: Vec<u16> = ix.rows.iter().filter(|r| r.kind == item_type::CONSUMABLE && !r.is_key() && r.id != 0).map(|r| r.id).take(6).collect();
        for (k, &id) in owned.iter().enumerate() {
            for _ in 0..=(k % 3) {
                let ok = ee.call(0x001a_18d8, &[id as u64, INVENTORY as u64, 0], &[], STACK).expect("FUN_001a18d8") as u32 != 0;
                assert_eq!(port.give(id, INVENTORY, false), ok);
            }
        }
        let mut taken = 0;
        for &id in owned.iter().chain(&owned).chain(&owned) {
            ee.call(0x001a_1ba8, &[id as u64, INVENTORY as u64], &[], STACK).expect("FUN_001a1ba8");
            assert!(port.take_item(id, INVENTORY));
            taken += 1;
            assert_eq!(diff(&read(&mut ee), port.block()), Vec::<String>::new(), "after taking item {id} from the stock");
        }
        eprintln!("oracle: menu writers equal (spell switch set/clear/flip {flips:?}, 4 shortcut words, {taken} stock takes; counts floor at 0)");
    }

    #[test]
    fn json_reader_handles_the_schema_shapes() {
        let j = Json::parse(r#" { "a": [1, 2.5, -3e2], "b": {"x": true, "y": null}, "s": "q\"A\n" } "#).unwrap();
        assert_eq!(j.get("a"), Some(&Json::Arr(vec![Json::Num(1.0), Json::Num(2.5), Json::Num(-300.0)])));
        assert_eq!(j.get("b").and_then(|b| b.get("x")), Some(&Json::Bool(true)));
        assert_eq!(j.get("s"), Some(&Json::Str("q\"A\n".into())));
        assert!(Json::parse("{\"a\": 1,}").is_err());
        assert!(Json::parse("[1 2]").is_err());
        assert!(Json::parse("{} x").is_err());
    }

    #[test]
    fn state_file_sets_the_fields() {
        let Some(tb) = tables() else { return };
        let ix = index(&tb);
        let mut p = Progress::new_game(ix.clone(), &tb.plrp);
        let fire = ix.magic_items[0];
        let valor = ix.form_items[1];
        let json = format!(
            r#"{{ "experience": 1234, "munny": 4321, "magic": [3, 0, 2], "items": {{ "{fire}": 2, "1": 7 }},
                 "weapon": {KINGDOM_KEY}, "abilities": [82, 162], "abilities_off": [394],
                 "forms": {{ "1": {{ "owned": true, "level": 3, "exp": 12 }}, "2": {{ "level": 2 }} }},
                 "summon": {{ "level": 4, "exp": 99 }}, "outfits": {{ "2": 1 }}, "spell_switch": 5 }}"#
        );
        p.apply_json(&json).unwrap();
        assert_eq!(p.experience(), 1234);
        assert_eq!(p.wallet(), 4321, "the munny key sets the wallet, not the experience");
        // "items" comes after "magic" in the file, so Fire ends at 2.
        assert_eq!(p.spell_levels()[..3], [2, 0, 2]);
        assert_eq!(p.item_count(1), 7);
        assert_eq!(p.sora().weapons[0], KINGDOM_KEY);
        assert_eq!(p.sora().abilities, vec![(82, true), (162, true), (394, false)]);
        assert!(p.owns_form(1) && !p.owns_form(2));
        assert_eq!(p.item_count(valor), 1);
        assert_eq!((p.form_level(1), p.form_exp(1), p.form_level(2)), (3, 12, 2));
        assert_eq!((p.summon_level(), p.summon_exp()), (4, 99));
        assert_eq!(p.outfit(2), 1);
        assert!(p.spell_switch(0) && !p.spell_switch(1) && p.spell_switch(2));
        assert!(p.apply_json(r#"{"munnny": 1}"#).is_err(), "typos are reported");
    }

    /// `FUN_0026c698` (equip a battle item, the pause menu writer) and the `sora_items` state key
    /// (`FUN_001a18d8` to Sora, what a pickup does).
    #[test]
    fn battle_item_equip_swaps_with_the_stock() {
        let Some(tb) = tables() else { return };
        let ix = index(&tb);
        let mut p = Progress::new_game(ix.clone(), &tb.plrp);
        let r = Progress::member_off(SORA).unwrap();
        p.set_item_slots(r, 3);
        let (potion, ether) = (1u16, 3u16);
        p.add_item(potion, 2);
        assert!(p.equip_battle_item(SORA, 0, potion, p.item_count(potion) > 0));
        assert_eq!((p.battle_items(SORA)[0], p.item_count(potion)), ((potion, 0), 1), "one off the stock into the slot");
        // Not carried: the item is only reserved and the old one goes back.
        assert!(p.equip_battle_item(SORA, 0, ether, p.item_count(ether) > 0));
        assert_eq!((p.battle_items(SORA)[0], p.item_count(potion)), ((0, ether), 2));
        assert!(p.equip_battle_item(SORA, 0, 0, false));
        assert_eq!(p.battle_items(SORA)[0], (0, 0));
        assert!(!p.equip_battle_item(SORA, 8, potion, true));
        // The state file: two Potions to Sora fill two slots and the received list.
        let mut q = Progress::new_game(ix, &tb.plrp);
        q.set_item_slots(Progress::member_off(SORA).unwrap(), 3);
        q.apply_json(r#"{"sora_items": [1, 1]}"#).unwrap();
        assert_eq!(q.battle_items(SORA).iter().filter(|s| s.0 == potion).count(), 2);
        assert_eq!(q.recent_items()[0], potion);
        assert!(q.take_item(potion, SORA) && q.take_item(potion, SORA) && !q.take_item(potion, SORA));
    }

    #[test]
    fn baseline_gives_the_old_app_defaults() {
        let Some(tb) = tables() else { return };
        let mut p = Progress::new_game(index(&tb), &tb.plrp);
        p.apply_baseline(true);
        assert_eq!(p.spell_levels(), [1, 1, 1, 1, 0, 0]);
        assert_eq!((1..=5).map(|f| p.owns_form(f)).collect::<Vec<_>>(), [true, true, true, true, false]);
        assert_eq!(p.sora().weapons[0], KINGDOM_KEY);
        assert!(p.sora().abilities.contains(&(crate::abilities::id::GUARD, true)));
        assert_eq!(p.dodge_roll, 1, "the baseline gives Sora the Dodge Roll");
        // Valor, Master and Final took the three spare Kingdom Keys (FUN_001a2010).
        assert_eq!((1..=4).map(|f| p.form(f).weapon).collect::<Vec<_>>(), [KINGDOM_KEY, 0, KINGDOM_KEY, KINGDOM_KEY]);
        assert!(!p.keyblades().contains(&KINGDOM_KEY));
        let mut q = Progress::new_game(p.items.clone(), &tb.plrp);
        q.apply_baseline(false);
        assert!(q.sora().abilities.is_empty(), "KH2_ABILITY_START=data: no Guard");
        assert_eq!(q.dodge_roll, 0, "KH2_ABILITY_START=data: no Dodge Roll either");
        q.apply_json(r#"{"dodge_roll": 3}"#).unwrap();
        assert_eq!(q.dodge_roll, 3);
    }

    #[test]
    fn counts_experience_and_spell_rows_follow_the_rules() {
        let Some(tb) = tables() else { return };
        let ix = index(&tb);
        let mut p = Progress::new_game(ix.clone(), &tb.plrp);
        // Experience: capped at 9,999,999 (FUN_001b51e0).
        p.add_experience(9_999_000);
        p.add_experience(5_000);
        assert_eq!(p.experience(), MAX_EXPERIENCE);
        p.add_experience(-999);
        assert_eq!(p.experience(), MAX_EXPERIENCE - 999);
        // Counts stop at 99 (FUN_001a1768), use floors at 0 (FUN_001a1c30).
        let potion = ix.rows.iter().find(|r| r.kind == item_type::CONSUMABLE && !r.is_key()).unwrap().id;
        let before = p.item_count(potion);
        assert_eq!(p.add_item(potion, 120), 99 - before);
        assert_eq!(p.item_count(potion), 99);
        p.use_item(potion, 200);
        assert_eq!(p.item_count(potion), 0);
        // A spell's level is its item's count; the magc row follows it (FUN_001a4c78).
        for slot in 0..6u8 {
            let item = ix.magic_items[slot as usize];
            assert_ne!(item, 0, "every magic slot has an item");
            for level in 0..=3u8 {
                p.set_count(item, level);
                assert_eq!(p.spell_level(slot), level);
                let row = crate::magic_rules::pick_row(&tb.magc, slot, p.spell_level(slot), 0);
                match level {
                    0 => assert!(row.is_none(), "slot {slot}: not owned, no row"),
                    l => {
                        let r = row.expect("a row");
                        assert!(r.level <= l && r.id == slot, "slot {slot} level {l}: row {} level {}", r.file_name, r.level);
                    }
                }
            }
        }
    }

    /// The oracle: the game's own `FUN_00187b38` and `FUN_001c0cb0(0)` run in kh2-ee on the user's
    /// executable build the party block; it must equal the port byte for byte. Then the game's
    /// `FUN_001a1cc8` (item count) and `FUN_001a4c78` (spell row) answer as `Progress` does, and
    /// `FUN_001a18d8` / `FUN_001a1c30` (give / take) change the block as the port does over a
    /// sweep of items. Item 0x20 and spell items are left out of the give sweep: their side
    /// calls (`FUN_001803b0`, `FUN_00180450`) act on state outside this block.
    #[test]
    fn new_game_block_matches_the_game() {
        use kh2_ee::Ee;
        const STACK: u32 = 0x01ff_0000;
        const ITEM_PTR: u32 = 0x01c9_5588;
        const PLRP_PTR: u32 = 0x01d5_df5c;
        const CMD_PTR: u32 = 0x01d5_d798;
        const MAGC_PTR: u32 = 0x01c9_5520;
        const WORLD: u32 = 0x0033_e830;
        let Some(mut tb) = tables() else { return };
        let ix = index(&tb);
        let elf = ["SLUS_210.05", "SLUS_210.05;1"].iter().find_map(|p| tb.archive.iso().read(p).ok()).expect("executable");
        let blob = |a: &mut kh2_archive::Archive, d: &kh2_tables::TableDef| kh2_tables::subfile(a, d).expect("table");
        let tables = [
            (ITEM_PTR, 0x0180_0000, blob(&mut tb.archive, &t::ITEM)),
            (PLRP_PTR, 0x0181_0000, blob(&mut tb.archive, &t::PLRP)),
            (CMD_PTR, 0x0182_0000, blob(&mut tb.archive, &t::CMD)),
            (MAGC_PTR, 0x0183_0000, blob(&mut tb.archive, &t::MAGC)),
        ];
        let mut ee = Ee::new();
        ee.load_elf(&elf).expect("elf");
        for (ptr, at, data) in &tables {
            ee.write(*at, data);
            ee.w32(*ptr, *at);
        }
        let party = addr::PARTY;
        ee.write(addr::SAVE, &vec![0u8; addr::SAVE_LEN as usize]);
        ee.call(0x0018_7b38, &[party as u64], &[], STACK).expect("FUN_00187b38");
        ee.call(0x001c_0cb0, &[0], &[], STACK).expect("FUN_001c0cb0");
        let read = |ee: &mut Ee| (0..off::LEN as u32).map(|o| ee.r8(party + o)).collect::<Vec<u8>>();
        let game = read(&mut ee);
        let mut port = Progress::new_game(ix.clone(), &tb.plrp);
        let diff = |a: &[u8], b: &[u8]| a.iter().zip(b).enumerate().filter(|(_, (x, y))| x != y).map(|(o, (x, y))| format!("+{o:#x}: game {x:#x} port {y:#x}")).take(12).collect::<Vec<_>>();
        assert_eq!(diff(&game, port.block()), Vec::<String>::new(), "new-game party block");

        // The decoded fields of the game's block.
        let g = Progress::from_block(&game, ix.clone());
        let sora_row = tb.plrp.iter().find(|r| r.id == 0 && r.character == 1).expect("Sora's new-game row");
        let s = g.sora();
        assert_eq!((s.hp, s.hp_max, s.mp, s.mp_max), (sora_row.hp, sora_row.hp, sora_row.mp, sora_row.mp));
        assert_eq!(s.items.len(), sora_row.item_slot_max as usize);
        assert_eq!((g.experience(), g.summon(), g.summon_level(), g.summon_exp(), g.spell_switch_word()), (0, 0, 1, 0, 0));
        assert!((1..=5).all(|f| g.form_level(f) == 1 && g.form(f).weapon == 0));
        assert!((0..off::WORLD_COUNT).all(|w| g.outfit(w) == 0));
        for r in &ix.rows {
            let n = ee.call(0x001a_1cc8, &[r.id as u64], &[], STACK).expect("FUN_001a1cc8") as u8;
            assert_eq!(g.item_count(r.id), n, "item {} count", r.id);
        }

        // Give and take a sweep of items in both and compare the blocks again.
        let world = 2u8;
        ee.write(WORLD, &[world]);
        let mut gives = 0;
        for (n, r) in ix.rows.iter().enumerate().filter(|(_, r)| r.id != 0x20 && r.kind != item_type::MAGIC && r.id != 0) {
            let (to, eq) = match n % 3 {
                0 => (INVENTORY, false),
                1 => (SORA, true),
                _ => (2, false),
            };
            let reps = 1 + n % 2;
            for _ in 0..reps {
                let ok = ee.call(0x001a_18d8, &[r.id as u64, to as u64, eq as u64], &[], STACK).expect("FUN_001a18d8") as u32 != 0;
                assert_eq!(port.give(r.id, to, eq), ok, "give item {} to {to}", r.id);
                gives += ok as u32;
            }
            if n % 5 == 0 {
                ee.call(0x001a_1c30, &[r.id as u64, 1], &[], STACK).expect("FUN_001a1c30");
                port.use_item(r.id, 1);
            }
        }
        // Spells through the counts: FUN_001a1a18 (no FUN_00180450 when the count was not 0) is
        // avoided by writing the counts the same way into both.
        let magc_base = 0x0183_0008u32;
        for slot in 0..6u8 {
            let item = ix.magic_items[slot as usize];
            let it = *ix.get(item).unwrap();
            for level in 0..=3u8 {
                ee.write(party + (off::COUNTS + it.slot as usize) as u32, &[level]);
                port.set_count(item, level);
                let ptr = ee.call(0x001a_4c78, &[slot as u64], &[], STACK).expect("FUN_001a4c78") as u32;
                let game_row = (ptr != 0).then(|| ((ptr - magc_base) / 0x38) as usize);
                let port_row = crate::magic_rules::pick_row(&tb.magc, slot, port.spell_level(slot), world).map(|r| tb.magc.iter().position(|x| std::ptr::eq(x, r)).unwrap());
                assert_eq!(game_row, port_row, "spell slot {slot} level {level}");
            }
        }
        let game = read(&mut ee);
        assert_eq!(diff(&game, port.block()), Vec::<String>::new(), "party block after the give/take sweep");

        // The field item use (WP-7C): FUN_001a1ba8 (take from a member's slots or the stock) and
        // FUN_001877c8 (refill reserved slots from the stock), on Sora with three battle item slots.
        let sora = Progress::member_off(SORA).unwrap();
        ee.write(party + (sora + rec::ITEM_SLOTS) as u32, &[3]);
        port.set_u8(sora + rec::ITEM_SLOTS, 3);
        for i in 0..3 {
            ee.write(party + (sora + rec::ITEMS + 2 * i) as u32, &[0, 0]);
            port.set_u16(sora + rec::ITEMS + 2 * i, 0);
        }
        let consumables: Vec<u16> = ix.rows.iter().filter(|r| r.kind == item_type::CONSUMABLE && !r.is_key() && r.id != 0).map(|r| r.id).take(2).collect();
        let (potion, other) = (consumables[0], consumables[1]);
        for (id, to) in [(potion, SORA), (other, SORA), (potion, SORA), (potion, INVENTORY), (potion, INVENTORY)] {
            let ok = ee.call(0x001a_18d8, &[id as u64, to as u64, 0], &[], STACK).expect("FUN_001a18d8") as u32 != 0;
            assert_eq!(port.give(id, to, false), ok, "give {id} to {to}");
        }
        assert_eq!(diff(&read(&mut ee), port.block()), Vec::<String>::new(), "after the gives to Sora's slots");
        let mut takes = Vec::new();
        for (id, to) in [(potion, SORA), (potion, SORA), (potion, SORA), (other, SORA), (potion, INVENTORY)] {
            let ok = ee.call(0x001a_1ba8, &[id as u64, to as u64], &[], STACK).expect("FUN_001a1ba8") as u32 != 0;
            assert_eq!(port.take_item(id, to), ok, "take {id} from {to}");
            takes.push(ok);
            assert_eq!(diff(&read(&mut ee), port.block()), Vec::<String>::new(), "after taking {id} from {to}");
        }
        // Reserve the potion in slots 0 and 2 and refill from the stock (one potion is left there).
        for i in [0usize, 2] {
            ee.write(party + (sora + rec::RESERVED + 2 * i) as u32, &potion.to_le_bytes());
            port.set_u16(sora + rec::RESERVED + 2 * i, potion);
        }
        ee.call(0x0018_77c8, &[(party + sora as u32) as u64], &[], STACK).expect("FUN_001877c8");
        port.refill_battle_items(SORA);
        assert_eq!(diff(&read(&mut ee), port.block()), Vec::<String>::new(), "after the refill");
        eprintln!("oracle: battle item take/refill equal (takes {takes:?}; Sora's slots {:?}; stock {})", port.battle_items(SORA), port.item_count(potion));
        let inv = port.inventory().len();
        eprintln!(
            "oracle: new-game party block equal ({} bytes, {} plrp rows with Id 0); {} item counts equal; \
             give/take sweep equal ({gives} gives accepted, {inv} items owned after); 24 spell rows equal",
            off::LEN,
            tb.plrp.iter().filter(|r| r.id == 0).count(),
            ix.rows.len(),
        );
    }
}
