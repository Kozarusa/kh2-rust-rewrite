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
    /// Armor and accessory rows (the filter words of `FUN_00283810`: bits 0x4000 and 0x8000).
    pub const ARMOR: u8 = 14;
    pub const ACCESSORY: u8 = 15;
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
/// Donald's and Goofy's party member ids (`FUN_001b58d8`: ids 1..=13 are their own record index; the
/// spawn rows' NeoStatus 2 and 3).
pub const DONALD: u16 = 2;
pub const GOOFY: u16 = 3;
/// The start weapons the app baseline equips on them (`went` of their movesets: item 75 -> `W_EX020`,
/// item 49 -> `W_EX030`, docs/party.md "Weapons"; the original's writer of the words was not found).
pub const DONALD_STAFF: u16 = 75;
pub const GOOFY_SHIELD: u16 = 49;

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
    /// The story flags, the area program table and the exit-lock halfwords (`Story`, save block
    /// `0x33f6b0`, `0x33e86f`, `0x33f918`). Shared with the script hosts (`shared_story`): a clone of
    /// `Progress` shares it too.
    story: StoryHandle,
    /// The "seen" words of the NEW tags (save `+0x31b4`, docs/menus.md 16.1) as a save slot carries them: empty until a slot
    /// that holds them is loaded or the menu stores them (`set_new_seen`).
    new_seen: Vec<u32>,
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
        Progress { block, items, source: Source::NewGame, baseline: Vec::new(), dodge_roll: 0, wallet: 0, config: [OPTION_DEFAULT, GAME_MODE_DEFAULT], story: StoryHandle::default(), new_seen: Vec::new() }
    }

    // --- story flags ------------------------------------------------------------------------

    /// The story state (flags, program table, exit locks). Hold the guard briefly.
    pub fn story(&self) -> std::sync::MutexGuard<'_, Story> {
        self.story.lock()
    }

    /// The handle the script hosts share (`install_shared_story`).
    pub fn story_handle(&self) -> StoryHandle {
        self.story.clone()
    }

    /// The `KH2_STATE` keys that rebuild the story flags: `{"story_flags": [set ids]}` (applying it
    /// sets each flag through `FUN_0017ff60`, so its side effects run again in id order).
    pub fn story_json(&self) -> String {
        let ids: Vec<String> = self.story().set_flags().iter().map(|i| i.to_string()).collect();
        let points: Vec<String> = self.story().world_points().iter().map(|i| i.to_string()).collect();
        let points = if points.is_empty() { String::new() } else { format!(", \"world_points\": [{}]", points.join(", ")) };
        format!("{{\"story_flags\": [{}]{points}}}", ids.join(", "))
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

    /// Byte 6 of `0x340c88` (the summon level-up, `FUN_001d4ec8`).
    pub fn set_summon_level(&mut self, level: u8) {
        self.set_u8(off::SUMMON_LEVEL, level);
    }

    /// `0x340e4c`, the summon experience (`FUN_001d4ec8`).
    pub fn set_summon_exp(&mut self, exp: u32) {
        self.set_u32(off::SUMMON_EXP, exp);
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
    /// (the byte `(row - 1) * 6 + 2` of the page's list table): the list entry's count byte
    /// (`FUN_00283a70` stores the stock count there; the recent items `FUN_00283e50` adds with 0 are not carried).
    /// False when the member or the slot does not exist.
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

    /// `FUN_0026c420(item)` (pause menu, Items, Weapons): the selected member's main weapon (record `+0`). When it
    /// differs, the old one goes back to the stock (`FUN_001a18d8(old, 100, 0)`, called whether the old id is 0 or
    /// not), the record takes the new one and one comes off the stock (`FUN_001a1ba8(new, 100)`). The original then
    /// queues a live-weapon change (`FUN_002a1060(member, 0, item, 0)`); the app's caller refreshes `Abilities`.
    /// Answers whether anything changed (the original's return value).
    pub fn equip_weapon(&mut self, member: u16, item: u16) -> bool {
        let Some(r) = Self::member_off(member) else { return false };
        let old = self.u16_at(r + rec::WEAPONS);
        if old == item {
            return false;
        }
        self.give(old, INVENTORY, false);
        self.set_u16(r + rec::WEAPONS, item);
        self.take_item(item, INVENTORY);
        true
    }

    /// `FUN_0026c4d0(form, item)`: a drive form's second keyblade (form record `+0`), the same swap on the form
    /// record. The live weapon is refreshed by the caller when `form` is the current form (`FUN_00282df8`, the
    /// byte at save `+0x242c`: `current_form()`).
    pub fn equip_form_weapon(&mut self, form: u8, item: u16) -> bool {
        let Some(r) = Self::form_off(form) else { return false };
        let old = self.u16_at(r);
        if old == item {
            return false;
        }
        self.give(old, INVENTORY, false);
        self.set_u16(r, item);
        self.take_item(item, INVENTORY);
        true
    }

    /// `FUN_0026c588(slot, item)`: an armor slot (record `+0x14 + 2 * slot`). The old item goes back to the stock
    /// when there was one, the slot takes the new one and one comes off the stock when it is not 0. False when the
    /// slot already holds the item (the original's 0) or does not exist.
    pub fn equip_armor(&mut self, member: u16, slot: usize, item: u16) -> bool {
        self.equip_slot(member, rec::ARMOR_SLOTS, rec::ARMOR, slot, item)
    }

    /// `FUN_0026c610(slot, item)`: an accessory slot (record `+0x24 + 2 * slot`), as `equip_armor`.
    pub fn equip_accessory(&mut self, member: u16, slot: usize, item: u16) -> bool {
        self.equip_slot(member, rec::ACCESSORY_SLOTS, rec::ACCESSORIES, slot, item)
    }

    fn equip_slot(&mut self, member: u16, count_at: usize, list_at: usize, slot: usize, item: u16) -> bool {
        let Some(r) = Self::member_off(member) else { return false };
        // The original does not look at the slot count (the page only offers existing slots).
        if slot >= self.u8_at(r + count_at) as usize {
            return false;
        }
        let o = r + list_at + 2 * slot;
        let old = self.u16_at(o);
        if old == item {
            return false;
        }
        if old != 0 {
            self.give(old, INVENTORY, false);
        }
        self.set_u16(o, item);
        if item != 0 {
            self.take_item(item, INVENTORY);
        }
        true
    }

    /// What the pause menu runs when it leaves an equipment list page (`FUN_0026a528`, after the leave animation
    /// and on a close): `FUN_0026c770` then `FUN_001877c8`. The first re-reserves: the reserved ids of the
    /// member's slots are collected in slot order, and every slot that carries one of those items takes it as its
    /// reserved id (so an item moved between slots keeps its Auto-Reload); the second refills every empty slot
    /// that reserves an item from the stock (`refill_battle_items`).
    pub fn settle_battle_items(&mut self, member: u16) {
        let Some(r) = Self::member_off(member) else { return };
        let n = (self.u8_at(r + rec::ITEM_SLOTS) as usize).min(8);
        let reserved: Vec<u16> = (0..n).map(|i| self.u16_at(r + rec::RESERVED + 2 * i)).filter(|&v| v != 0).collect();
        for i in 0..n {
            let carried = self.u16_at(r + rec::ITEMS + 2 * i);
            if let Some(&v) = reserved.iter().find(|&&v| v == carried) {
                self.set_u16(r + rec::RESERVED + 2 * i, v);
            }
        }
        self.refill_battle_items(member);
    }

    /// `FUN_00274f70`, the slot loop (the Customize page's Auto-Reload switch): every battle item slot of the
    /// member that holds the item or reserves it gets its reserved id set to the item (on) or cleared (off).
    /// The items stay where they are.
    pub fn set_auto_reload(&mut self, member: u16, id: u16, on: bool) {
        let Some(r) = Self::member_off(member) else { return };
        let n = (self.u8_at(r + rec::ITEM_SLOTS) as usize).min(8);
        for i in 0..n {
            if self.u16_at(r + rec::ITEMS + 2 * i) == id || self.u16_at(r + rec::RESERVED + 2 * i) == id {
                self.set_u16(r + rec::RESERVED + 2 * i, if on { id } else { 0 });
            }
        }
    }

    /// `FUN_00275288` for the leader: whether turning Auto-Reload off for an item that is on needs the question
    /// first, which is when a slot reserves the item without carrying it (an empty slot waiting for a refill).
    pub fn reload_off_needs_question(&self, member: u16, id: u16) -> bool {
        self.battle_items(member).iter().any(|&(item, reserved)| item != id && reserved == id)
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

    /// `FUN_001d3f18(record + 0x54, 0x40, form, level)` on a member's 64-slot list: the first slot whose
    /// id (`& 0x7fff`) is `old` takes `new`, keeping its bit 15; at most one slot changes. `old` 0 matches
    /// the first empty slot, which has bit 15 clear, so a new growth ability arrives **unequipped**
    /// (`FUN_001d3da8` calls it for Sora at the form row's display level minus one; `old` / `new` are
    /// the growth chain's ids at 0x37c556, `prize::GrowthChains`). Returns the slot written, `None` when
    /// no slot matches (nothing changes) or the member does not exist.
    pub fn upgrade_ability(&mut self, member: u16, old: u16, new: u16) -> Option<usize> {
        let r = Self::member_off(member)?;
        let slot = (0..rec::ABILITY_SLOTS).find(|&i| self.u16_at(r + rec::ABILITIES + 2 * i) & 0x7fff == old)?;
        let o = r + rec::ABILITIES + 2 * slot;
        let v = self.u16_at(o);
        self.set_u16(o, v & 0x8000 | new & 0x7fff);
        Some(slot)
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
        let mut p = Progress {
            block: vec![0; off::LEN],
            items,
            source: Source::NewGame,
            baseline: Vec::new(),
            dodge_roll: 0,
            wallet: 0,
            config: [OPTION_DEFAULT, GAME_MODE_DEFAULT],
            story: StoryHandle::default(),
            new_seen: Vec::new(),
        };
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
        // WP-F1 item 6: Donald's staff (item 75) and Goofy's shield (item 49) in their weapon words
        // (record +0, hand 0: `FUN_0016ee98` reads it for the hand object), so the members fight with
        // their weapons shown. The original's new game (`FUN_00187b38`, `FUN_001c0cb0(0)`: docs/progress.md)
        // leaves both words 0 and the writer that fills them later was not found (docs/party.md: the item
        // immediates 0x4b / 0x31 were searched): an app baseline entry, like the spare Kingdom Keys.
        for (member, item) in [(DONALD, DONALD_STAFF), (GOOFY, GOOFY_SHIELD)] {
            if let Some(r) = Self::member_off(member)
                && self.u16_at(r + rec::WEAPONS) == 0
            {
                self.set_u16(r + rec::WEAPONS, item);
                added.push(format!("member {member} weapon item {item} equipped"));
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
                // Story flags: each id set through the game's setter `FUN_0017ff60` (its side effects
                // from `00progress.bin` run: area programs, exit locks, other flags), in file order.
                "story_flags" => {
                    let mut s = self.story();
                    for id in list(v, "story_flags")? {
                        s.set(id);
                    }
                }
                // Each id cleared through `FUN_00180298` (only the bit; no side effects).
                "story_clear" => {
                    let mut s = self.story();
                    for id in list(v, "story_clear")? {
                        s.clear(id);
                    }
                }
                // World-point bits (0x33f910), set through `FUN_00194fa8`.
                "world_points" => {
                    let mut s = self.story();
                    for id in list(v, "world_points")? {
                        s.set_world_point(id as usize);
                    }
                }
                "comment" | "_comment" => {}
                // The save slot keys (WP-M8, `slot_json`): the whole party block, the app fields beside it and the
                // row data the Save menu shows (ignored here).
                "format" | "slot" => {}
                "block" => {
                    let Json::Str(h) = v else { return Err("block: expected a hex string".into()) };
                    self.block = unhex(h, off::LEN).ok_or_else(|| format!("block: expected {} hex bytes", off::LEN))?;
                }
                "new_seen" => {
                    let Json::Arr(a) = v else { return Err("new_seen: expected an array".into()) };
                    self.new_seen = a.iter().map(|x| int(x, "new_seen").map(|n| n as u32)).collect::<Result<_, _>>()?;
                }
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
        // `FUN_0017fd10`: the 19 type-2 world entries of `00progress.bin` (the flag side effects).
        let effects: Arc<Vec<Vec<u8>>> = Arc::new(
            disc.read("00progress.bin")
                .and_then(|f| Some(kh2_bar::parse(&f).ok()?.into_iter().filter(|e| e.kind == 2).take(story::WORLDS).map(|e| e.data.to_vec()).collect()))
                .unwrap_or_default(),
        );
        if effects.is_empty() {
            warn!("progress: 00progress.bin not readable; story flags set without their side effects");
        }
        let mut p = Progress::new_game(index.clone(), &plrp);
        p.story().effects = effects.clone();
        match std::env::var("KH2_STATE") {
            Ok(path) if !path.is_empty() => match std::fs::read_to_string(&path).map_err(|e| e.to_string()).and_then(|s| p.apply_json(&s)) {
                Ok(()) => p.source = Source::File(path),
                Err(e) => {
                    // A file that failed half way is dropped as a whole.
                    warn!("progress: KH2_STATE={path}: {e}; using the new game and the app baseline");
                    p = Progress::new_game(index, &plrp);
                    p.story().effects = effects;
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
        ) + &format!("; story flags {:x?}", self.story().set_flags())
    }
}

// --- story flags, area programs, exit locks (save block) ------------------------------------------

/// EE addresses of the story state (docs/events.md sections 0, 2 and 5).
pub mod story {
    /// The flag array: 19 pages of 0x20 bytes, one per world (`FUN_001802e0`, `FUN_00180298`,
    /// `FUN_0017ff60`). It ends at 0x33f910, where the world-point bits of `FUN_00194fd8` start.
    pub const FLAGS: u32 = 0x0033_f6b0;
    pub const PAGE: usize = 0x20;
    pub const WORLDS: usize = 19;
    /// The area program table: `(map, btl, evt)` at `0x33e86f + world * 0xc0 + area * 3`
    /// (`FUN_00104bd0` copies it into the area record, flag op 1 writes it through `FUN_00104b90`).
    pub const PROGRAMS: u32 = 0x0033_e86f;
    pub const PROGRAMS_PER_WORLD: usize = 0xc0;
    /// The exit-lock halfwords: entry `i` of the `dsa` table owns `0x33f918 + 4i` (state) and
    /// `+2` (the locked prompt's message); `FUN_00195260` .. `FUN_00195300`, read by `FUN_00195358`.
    pub const LOCKS: u32 = 0x0033_f918;
}

/// What the save block keeps of the story: the flag bits, the area program table and the exit-lock
/// halfwords, plus the side-effect lists of `00progress.bin` that setting a flag runs. The new game
/// has every byte 0 (`FUN_00105638` clears the save block at boot) [inferred: no later new-game writer
/// of these three regions was found; docs/events.md section 10].
#[derive(Clone, Debug, Default)]
pub struct Story {
    /// The flag words (`story::FLAGS`), `WORLDS * PAGE / 4` of them.
    words: Vec<u32>,
    /// `story::PROGRAMS`, `WORLDS * PROGRAMS_PER_WORLD` bytes.
    programs: Vec<u8>,
    /// `story::LOCKS`: (state, message) per `dsa` entry; entries past the end read 0.
    locks: Vec<[u16; 2]>,
    /// The 19 type-2 world entries of `00progress.bin` (`FUN_0017fd10` keeps them at `0x35ccc0`).
    /// Empty: flags are set without side effects.
    pub effects: Arc<Vec<Vec<u8>>>,
    /// Side-effect ops met that the app does not act on (6, 8, 9, 0xd and the world-point bits of
    /// ops 4 and 5), with the flag that ran them; for the logs.
    pub unported: Vec<(u16, u8)>,
    /// The world-point bits at `0x33f910` (`FUN_00194fa8` sets bit `i` for record `i` of
    /// `00worldpoint.bin`, at most 0x36 of them; `FUN_00194fd8` clears, not ported). Two words.
    world_bits: [u32; 2],
}

/// `Story` behind a lock, shared by `Progress` and the script hosts.
#[derive(Clone, Debug, Default)]
pub struct StoryHandle(Arc<std::sync::Mutex<Story>>);

impl StoryHandle {
    pub fn new(s: Story) -> StoryHandle {
        StoryHandle(Arc::new(std::sync::Mutex::new(s)))
    }

    pub fn lock(&self) -> std::sync::MutexGuard<'_, Story> {
        self.0.lock().unwrap_or_else(|e| e.into_inner())
    }
}

static SHARED_STORY: std::sync::Mutex<Option<StoryHandle>> = std::sync::Mutex::new(None);

/// Makes `h` the story the script hosts read and write (hosts 0.14, 0.15, 0.70). The game's flags
/// are one fixed array in the save block; the hosts' `Env` is built in files outside the story code,
/// so they reach it through this one process-wide handle. `ensure` installs the app's.
pub fn install_shared_story(h: StoryHandle) {
    *SHARED_STORY.lock().unwrap_or_else(|e| e.into_inner()) = Some(h);
}

/// The installed story (`None` in unit tests and tools: the hosts then answer as before, all clear).
pub fn shared_story() -> Option<StoryHandle> {
    SHARED_STORY.lock().unwrap_or_else(|e| e.into_inner()).clone()
}

impl Story {
    /// A new-game story whose flags run the side effects of these world entries.
    pub fn with_effects(effects: Arc<Vec<Vec<u8>>>) -> Story {
        Story { effects, ..Story::default() }
    }

    fn ensure(&mut self) {
        if self.words.is_empty() {
            self.words = vec![0; story::WORLDS * story::PAGE / 4];
            self.programs = vec![0; story::WORLDS * story::PROGRAMS_PER_WORLD];
        }
    }

    /// The word index and bit of flag `id`: page `id >> 10` (0x20 bytes), word `(id & 0x3ff) >> 5`,
    /// bit `id & 0x1f` (`FUN_001802e0`). `None` past the array: a page beyond the 19 worlds, or a
    /// word past the end [inferred: the game would address memory after the array; no flag list
    /// names such an id, each world uses 256 flags].
    fn slot(id: u16) -> Option<(usize, u32)> {
        let w = (id as usize >> 10) * (story::PAGE / 4) + ((id as usize & 0x3ff) >> 5);
        (w < story::WORLDS * story::PAGE / 4).then_some((w, 1 << (id & 0x1f)))
    }

    /// `FUN_001802e0`: flag `id` is set (0xffff is always clear).
    pub fn test(&self, id: u16) -> bool {
        id != 0xffff && Self::slot(id).is_some_and(|(w, m)| self.words.get(w).is_some_and(|v| v & m != 0))
    }

    /// `FUN_00180298`: clears the bit (no side effects).
    pub fn clear(&mut self, id: u16) {
        self.ensure();
        if let Some((w, m)) = Self::slot(id) {
            self.words[w] &= !m;
        }
    }

    /// `FUN_0017ff60`: sets the bit, then runs the flag's side-effect list from the world entry
    /// `id >> 10` at the offset in halfword `id & 0x3ff` (`kh2_ard::progress::effects`):
    /// op 1 writes (map, btl, evt) of area `record[0]` of the flag's world (0xff keeps,
    /// `FUN_00104b90`); op 2 the lock entry's state and message (`FUN_00195260`); op 3 the state 0
    /// (`FUN_00195288`); op 4 the state 0xffff (`FUN_001952a0`); op 5 the state 0 (`FUN_00195300`);
    /// op 7 clears each flag (`FUN_00180298`); op 0xc sets each flag not yet set (recursive). The
    /// first halfword of ops 2..5 is the `dsa` index, each called once. Not acted on (recorded in
    /// `unported`): op 6 (`FUN_001a7730`, `0x33fe0c[world]`), 8 / 9 (the record bits 0x33fdb0,
    /// `FUN_001a52b0` / `FUN_001a52f0`), 0xd (`FUN_001b4e80`: `0x341858 + 4 * world` bytes 1, 2), the
    /// world-point bit ops 4 and 5 clear (`FUN_00194f68`, `FUN_00194fd8` on 0x33f910), and the calls
    /// after the list (`FUN_001803b0`, `FUN_00180450`, `FUN_001805d0`, `FUN_00180558`, `FUN_001d39d0`,
    /// `FUN_001d3238`, `FUN_001b6da0`, `FUN_001be9c8`, `FUN_001d5718`, `FUN_0019d890` for 0x8d0,
    /// `FUN_00195130`).
    pub fn set(&mut self, id: u16) {
        self.ensure();
        let Some((w, m)) = Self::slot(id) else { return };
        self.words[w] |= m;
        let world = id as usize >> 10;
        let effects = self.effects.clone();
        let Some(list) = effects.get(world).and_then(|e| kh2_ard::progress::effects(e, id & 0x3ff)) else { return };
        for e in list {
            let h = e.flags();
            let h0 = h.first().copied().unwrap_or(0) as usize;
            match e.op {
                1 => {
                    for [area, prog @ ..] in e.programs() {
                        let o = world * story::PROGRAMS_PER_WORLD + area as usize * 3;
                        for (k, b) in prog.iter().enumerate() {
                            if *b != 0xff
                                && let Some(slot) = self.programs.get_mut(o + k)
                            {
                                *slot = *b;
                            }
                        }
                    }
                }
                2 => {
                    let v = [h.get(1).copied().unwrap_or(0), h.get(2).copied().unwrap_or(0)];
                    self.lock_entry(h0)[..].copy_from_slice(&v);
                }
                3 => self.lock_entry(h0)[0] = 0,
                4 | 5 => {
                    self.lock_entry(h0)[0] = if e.op == 4 { 0xffff } else { 0 };
                    self.unported.push((id, e.op));
                }
                7 => h.iter().for_each(|&f| self.clear(f)),
                0xc => {
                    for f in h {
                        if !self.test(f) {
                            self.set(f);
                        }
                    }
                }
                op => self.unported.push((id, op)),
            }
        }
    }

    fn lock_entry(&mut self, i: usize) -> &mut [u16; 2] {
        if self.locks.len() <= i {
            self.locks.resize(i + 1, [0, 0]);
        }
        &mut self.locks[i]
    }

    /// The lock state halfword of `dsa` entry `i` (`0x33f918 + 4i`, read by `FUN_00195358`).
    pub fn lock_state(&self, i: usize) -> u16 {
        self.locks.get(i).map_or(0, |l| l[0])
    }

    /// The locked prompt's message halfword of entry `i` (`0x33f91a + 4i`, `FUN_001953e0`).
    pub fn lock_message(&self, i: usize) -> u16 {
        self.locks.get(i).map_or(0, |l| l[1])
    }

    /// The (map, btl, evt) bytes of the program table for (world, area) (`FUN_00104bd0` reads
    /// `0x33e86f`, `0x33e870`, `0x33e871` + `world * 0xc0 + area * 3`); 0 for each in a new game.
    pub fn programs(&self, world: u8, area: u8) -> [u8; 3] {
        let o = world as usize * story::PROGRAMS_PER_WORLD + area as usize * 3;
        let b = |k: usize| self.programs.get(o + k).copied().unwrap_or(0);
        [b(0), b(1), b(2)]
    }

    /// `FUN_00194fa8`: sets world-point bit `i` (`None` past the two words).
    pub fn set_world_point(&mut self, i: usize) {
        if let Some(w) = self.world_bits.get_mut(i >> 5) {
            *w |= 1 << (i & 31);
        }
    }

    /// Whether world-point bit `i` is set (the reader of the bits `FUN_00194fd8` tests).
    pub fn world_point(&self, i: usize) -> bool {
        self.world_bits.get(i >> 5).is_some_and(|w| w >> (i & 31) & 1 != 0)
    }

    /// Every set world-point bit, in order.
    pub fn world_points(&self) -> Vec<usize> {
        (0..self.world_bits.len() * 32).filter(|&i| self.world_point(i)).collect()
    }

    /// Every set flag id, in order.
    pub fn set_flags(&self) -> Vec<u16> {
        let mut out = Vec::new();
        for (w, &v) in self.words.iter().enumerate() {
            for b in 0..32 {
                if v >> b & 1 != 0 {
                    let page = w / (story::PAGE / 4);
                    out.push(((page << 10) | ((w % (story::PAGE / 4)) << 5) | b) as u16);
                }
            }
        }
        out
    }
}

// --- save slots (WP-M8, docs/menus.md 18) ------------------------------------------------------

/// What a slot's row on the Save menu shows besides the state itself: the leader's level, the munny, the play time and the
/// area the file was written in (the area's code, `tt03`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SlotInfo {
    pub level: u32,
    pub munny: u32,
    pub play_seconds: u64,
    pub area: String,
}

/// The save slots the Save menu lists (the footage shows four rows).
pub const SLOTS: usize = 4;

/// The folder the slots live in: `KH2_SAVE_DIR`, else `local/saves` below the working directory (git-ignored). The app never
/// reads or writes anywhere else (a PS2 memory card is not touched).
pub fn save_dir() -> std::path::PathBuf {
    std::env::var_os("KH2_SAVE_DIR").filter(|v| !v.is_empty()).map_or_else(|| std::path::PathBuf::from("local/saves"), std::path::PathBuf::from)
}

/// The file of slot `n` (0-based).
pub fn slot_path(dir: &std::path::Path, n: usize) -> std::path::PathBuf {
    dir.join(format!("slot{:02}.json", n + 1))
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

fn unhex(s: &str, len: usize) -> Option<Vec<u8>> {
    (s.len() == 2 * len && s.is_ascii()).then(|| (0..len).map(|i| u8::from_str_radix(&s[2 * i..2 * i + 2], 16).ok()).collect::<Option<Vec<u8>>>()).flatten()
}

impl Progress {
    /// The state as a slot file: a JSON object that `apply_json` (and so `KH2_STATE`) reads back. The party block goes as hex,
    /// the app fields kept beside it (munny, option word, game mode, Dodge Roll level, the NEW tags' seen words) as the keys
    /// of the state schema, and `slot` carries the row data. Rows are keys of the same schema, so a slot file is also a
    /// state file.
    pub fn slot_json(&self, info: &SlotInfo) -> String {
        let area: String = info.area.chars().filter(|c| c.is_ascii_alphanumeric() || *c == '_').collect();
        let seen: Vec<String> = self.new_seen.iter().map(u32::to_string).collect();
        format!(
            "{{\n  \"format\": 1,\n  \"slot\": {{\"level\": {}, \"munny\": {}, \"play_seconds\": {}, \"area\": \"{}\"}},\n  \"block\": \"{}\",\n  \"munny\": {},\n  \"option_word\": {},\n  \"game_mode\": {},\n  \"dodge_roll\": {},\n  \"new_seen\": [{}]\n}}\n",
            info.level,
            info.munny,
            info.play_seconds,
            area,
            hex(&self.block),
            self.wallet,
            self.config[0],
            self.config[1],
            self.dodge_roll,
            seen.join(", ")
        )
    }

    /// The row data of a slot file (`None` for text that is not a slot file).
    pub fn slot_info(text: &str) -> Option<SlotInfo> {
        let j = Json::parse(text).ok()?;
        let s = j.get("slot")?;
        let n = |k: &str| s.get(k).and_then(Json::int).map(|v| v.max(0));
        Some(SlotInfo {
            level: n("level")? as u32,
            munny: n("munny")? as u32,
            play_seconds: n("play_seconds")? as u64,
            area: match s.get("area") {
                Some(Json::Str(a)) => a.clone(),
                _ => return None,
            },
        })
    }

    /// The NEW tags' seen words (save `+0x31b4`) a loaded slot carried.
    pub fn new_seen(&self) -> &[u32] {
        &self.new_seen
    }

    pub fn set_new_seen(&mut self, words: &[u32]) {
        self.new_seen = words.to_vec();
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
    {
        let s = p.story();
        if !s.set_flags().is_empty() {
            let tt: Vec<String> = (0..64u8).filter(|&a| s.programs(2, a) != [0; 3]).map(|a| format!("tt{a:02} {:x?}", s.programs(2, a))).collect();
            let locks: Vec<String> = (0..64).filter(|&i| s.lock_state(i) != 0).map(|i| format!("{i}: {:#x}", s.lock_state(i))).collect();
            info!("progress: story flags {:x?}; Twilight Town program table (map, btl, evt) [{}]; exit locks [{}]; side effects not ported {:x?}", s.set_flags(), tt.join(", "), locks.join(", "), s.unported);
        }
    }
    install_shared_story(p.story_handle());
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

    /// `FUN_001d3f18` on Sora's list: the first slot holding the level below takes the new level and keeps
    /// its bit 15; the first growth level fills the first empty slot unequipped; no match changes nothing.
    /// The same as `prize::growth_upgrade` (the array form the form level-up log uses).
    #[test]
    fn growth_upgrade_keeps_the_equipped_bit_and_matches_the_array_form() {
        let mut p = Progress::from_block(&[], Arc::new(ItemIndex::default()));
        let r = Progress::member_off(SORA).unwrap();
        p.put_ability(r, 82, true);
        let chains = crate::prize::GrowthChains([[0; 5], [0, 94, 95, 96, 97], [0, 98, 99, 100, 101], [0; 5], [0; 5]]);
        let mut arr = [0u16; 64];
        arr[0] = 82 | 0x8000;
        let step = |p: &mut Progress, arr: &mut [u16; 64], form: u8, level: u8| {
            let old = if level == 0 { chains.at(form - 1, 4) } else { chains.at(form, level - 1) };
            let got = p.upgrade_ability(SORA, old, chains.at(form, level));
            crate::prize::growth_upgrade(arr, &chains, form, level);
            let words: Vec<u16> = (0..64).map(|i| p.u16_at(r + rec::ABILITIES + 2 * i)).collect();
            assert_eq!(words, arr.to_vec(), "form {form} level {level}");
            got
        };
        assert_eq!(step(&mut p, &mut arr, 1, 1), Some(1), "High Jump LV1 into the first empty slot");
        assert_eq!(p.ability_slots(SORA), vec![(0, 82, true), (1, 94, false)], "it arrives unequipped");
        p.set_ability_equipped(SORA, 1, true);
        arr[1] |= 0x8000;
        assert_eq!(step(&mut p, &mut arr, 1, 2), Some(1));
        assert_eq!(p.ability_slots(SORA), vec![(0, 82, true), (1, 95, true)], "LV2 keeps LV1's equipped bit");
        assert_eq!(step(&mut p, &mut arr, 2, 2), None, "no Quick Run LV1 to upgrade: nothing changes");
        assert_eq!(step(&mut p, &mut arr, 2, 1), Some(2));
        assert_eq!(step(&mut p, &mut arr, 2, 2), Some(2));
        assert_eq!(p.ability_slots(SORA), vec![(0, 82, true), (1, 95, true), (2, 99, false)]);
        assert_eq!(p.upgrade_ability(99, 0, 94), None, "no such member");
    }

    /// The oracle for `upgrade_ability`: the game's `FUN_001d3f18(list, 0x40, form, level)` (a leaf) runs in
    /// kh2-ee on the user's executable over a 64-slot list, with the chain table at 0x37c556 as the
    /// executable holds it; the port gets the chain's ids at `level - 1` (the routine reads the entry just before the
    /// chain's `level` entry, which for level 0 is the previous form's entry 4) and `level`. Every form 0..=4 and level 0..=4, twice
    /// round, with one slot taken off between the rounds; the 64 words must stay equal.
    #[test]
    fn growth_upgrade_matches_the_game() {
        use kh2_ee::Ee;
        const STACK: u32 = 0x01ff_0000;
        const LIST: u32 = 0x0180_0000;
        const CHAINS: u32 = crate::prize::GROWTH_CHAINS;
        let Some(mut tb) = tables() else { return };
        let elf = ["SLUS_210.05", "SLUS_210.05;1"].iter().find_map(|p| tb.archive.iso().read(p).ok()).expect("executable");
        let mut ee = Ee::new();
        ee.load_elf(&elf).expect("elf");
        let mut p = Progress::from_block(&[], Arc::new(ItemIndex::default()));
        let r = Progress::member_off(SORA).unwrap();
        p.put_ability(r, 82, true);
        p.put_ability(r, 162, false);
        let words = |p: &Progress| (0..64).map(|i| p.u16_at(r + rec::ABILITIES + 2 * i)).collect::<Vec<u16>>();
        let bytes: Vec<u8> = words(&p).iter().flat_map(|w| w.to_le_bytes()).collect();
        ee.write(LIST, &bytes);
        let (mut changed, mut cases) = (0, 0);
        for round in 0..2 {
            for form in 0..=4u32 {
                for level in 0..=4u32 {
                    let at = CHAINS + 10 * form + 2 * level;
                    let (old, new) = (ee.r16(at - 2), ee.r16(at));
                    ee.call(0x001d_3f18, &[LIST as u64, 0x40, form as u64, level as u64], &[], STACK).expect("FUN_001d3f18");
                    changed += p.upgrade_ability(SORA, old, new).is_some() as u32;
                    cases += 1;
                    let game: Vec<u16> = (0..64).map(|i| ee.r16(LIST + 2 * i)).collect();
                    assert_eq!(game, words(&p), "round {round} form {form} level {level} (old {old}, new {new})");
                }
            }
            // Take the first growth slot off (FUN_0027d8b8) in both, then go round again.
            if let Some(&(slot, _, _)) = p.ability_slots(SORA).iter().find(|s| s.1 != 82 && s.1 != 162) {
                p.set_ability_equipped(SORA, slot, false);
                let v = ee.r16(LIST + 2 * slot as u32);
                ee.write(LIST + 2 * slot as u32, &(v & 0x7fff).to_le_bytes());
            }
        }
        eprintln!("oracle: FUN_001d3f18 equal for {cases} calls ({changed} slots written); Sora's list {:?}", p.ability_slots(SORA));
        assert!(changed > 0);
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

    /// `FUN_00274f70`'s slot loop and `FUN_00275288`'s question test.
    #[test]
    fn auto_reload_reserves_and_releases_every_slot_of_the_item() {
        let mut p = Progress::from_block(&[], Arc::new(ItemIndex::default()));
        let r = Progress::member_off(SORA).unwrap();
        p.set_item_slots(r, 4);
        // Slot 0 carries item 1, slot 1 reserves 1 and is empty, slot 2 carries item 3, slot 3 is free.
        p.set_u16(r + rec::ITEMS, 1);
        p.set_u16(r + rec::RESERVED + 2, 1);
        p.set_u16(r + rec::ITEMS + 4, 3);
        assert_eq!(p.battle_items(SORA), vec![(1, 0), (0, 1), (3, 0), (0, 0)]);
        // Item 1 is on in slot 1 and not carried there: turning it off asks first (the empty slot waits for a refill).
        assert!(p.reload_off_needs_question(SORA, 1));
        assert!(!p.reload_off_needs_question(SORA, 3));
        p.set_auto_reload(SORA, 1, false);
        assert_eq!(p.battle_items(SORA), vec![(1, 0), (0, 0), (3, 0), (0, 0)], "the carried item stays, the reservation goes");
        assert!(!p.reload_off_needs_question(SORA, 1));
        // On: every slot that carries or reserves it reserves it; other items are untouched.
        p.set_auto_reload(SORA, 3, true);
        p.set_auto_reload(SORA, 1, true);
        assert_eq!(p.battle_items(SORA), vec![(1, 1), (0, 0), (3, 3), (0, 0)]);
        assert!(!p.reload_off_needs_question(SORA, 1), "reserved and carried: no question");
    }

    /// The Save menu's slot file (docs/menus.md 18): written from one state, read back through `apply_json` (the path
    /// `KH2_STATE` takes) into another, it reproduces the party block byte for byte and the app fields beside it.
    #[test]
    fn a_slot_file_restores_the_block_byte_for_byte() {
        let bytes: Vec<u8> = (0..off::LEN).map(|i| (i * 7 + 3) as u8).collect();
        let mut p = Progress::from_block(&bytes, Arc::new(ItemIndex::default()));
        p.wallet = 123_456;
        p.config = [0x41b, 2];
        p.dodge_roll = 3;
        p.set_new_seen(&[1, 0xffff_ffff, 7]);
        let info = SlotInfo { level: 7, munny: 123_456, play_seconds: 3725, area: "tt03".into() };
        let text = p.slot_json(&info);
        assert_eq!(Progress::slot_info(&text), Some(info));
        let mut q = Progress::from_block(&[], Arc::new(ItemIndex::default()));
        assert_ne!(q.block(), p.block());
        q.apply_json(&text).expect("a slot file is a state file");
        assert_eq!(q.block(), p.block(), "the party block, byte for byte");
        assert_eq!((q.wallet(), q.option_word(), q.game_mode(), q.dodge_roll), (123_456, 0x41b, 2, 3));
        assert_eq!(q.new_seen(), [1, 0xffff_ffff, 7]);
        // A block of the wrong length is refused, and the slot's row data is not a state key of its own.
        let short = text.replacen("\"block\": \"", "\"block\": \"00", 1);
        assert!(q.apply_json(&short).is_err());
        assert_eq!(Progress::slot_info("{\"munny\": 1}"), None);
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

    /// The oracle for the pause menu's equipment writers (WP-M4): the game's own `FUN_0026c420` (main weapon),
    /// `FUN_0026c4d0` (a form's keyblade), `FUN_0026c588` (armor slot) and `FUN_0026c610` (accessory slot) run in
    /// kh2-ee on the block the game built, with the page's party object faked (`DAT_0036ce90` points at a table
    /// whose row 0 holds a status object whose `+0x238` is Sora's record; the page's remembered cursor
    /// `FUN_00252fb8(1)` reads 0) and the live-weapon queue `FUN_002a1060` stubbed (it only queues a request for
    /// the field). The port must leave the same bytes and answer the same value after every swap.
    #[test]
    fn equipment_writers_match_the_game() {
        use kh2_ee::Ee;
        const STACK: u32 = 0x01ff_0000;
        const TABLE: u32 = 0x0197_0000;
        const STATUS: u32 = 0x0198_0000;
        let Some(mut tb) = tables() else { return };
        let ix = index(&tb);
        let elf = ["SLUS_210.05", "SLUS_210.05;1"].iter().find_map(|p| tb.archive.iso().read(p).ok()).expect("executable");
        let blob = |a: &mut kh2_archive::Archive, d: &kh2_tables::TableDef| kh2_tables::subfile(a, d).expect("table");
        let tables = [
            (0x01c9_5588u32, 0x0180_0000u32, blob(&mut tb.archive, &t::ITEM)),
            (0x01d5_df5c, 0x0181_0000, blob(&mut tb.archive, &t::PLRP)),
            (0x01d5_d798, 0x0182_0000, blob(&mut tb.archive, &t::CMD)),
            (0x01c9_5520, 0x0183_0000, blob(&mut tb.archive, &t::MAGC)),
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
        let mut port = Progress::new_game(ix.clone(), &tb.plrp);
        let r = Progress::member_off(SORA).unwrap();
        // The fake party object: one row, id 1, the status object pointer at row +0x14; the record at status +0x238.
        ee.w32(0x0036_ce90, TABLE);
        ee.w32(TABLE, 1);
        ee.write(TABLE + 4, &1u16.to_le_bytes());
        ee.write(TABLE + 6, &1u16.to_le_bytes());
        ee.w32(TABLE + 0x14, STATUS);
        ee.w32(STATUS + 0x238, party + r as u32);
        // `FUN_002a1060` returns at once.
        ee.write(0x002a_1060, &[0x08, 0x00, 0xe0, 0x03, 0, 0, 0, 0]);
        // Slots: two armor and two accessory slots (the game does not look at the counts), set in both blocks.
        for (o, n) in [(rec::ARMOR_SLOTS, 2u8), (rec::ACCESSORY_SLOTS, 2)] {
            ee.write(party + (r + o) as u32, &[n]);
            port.set_u8(r + o, n);
        }
        let read = |ee: &mut Ee| (0..off::LEN as u32).map(|o| ee.r8(party + o)).collect::<Vec<u8>>();
        let diff = |a: &[u8], b: &[u8]| a.iter().zip(b).enumerate().filter(|(_, (x, y))| x != y).map(|(o, (x, y))| format!("+{o:#x}: game {x:#x} port {y:#x}")).take(12).collect::<Vec<_>>();
        assert_eq!(diff(&read(&mut ee), port.block()), Vec::<String>::new(), "start");
        let of_kind = |k: u8, n: usize| ix.rows.iter().filter(|x| x.kind == k && !x.is_key() && x.id != 0).map(|x| x.id).take(n).collect::<Vec<u16>>();
        let (keys, armor, acc) = (of_kind(item_type::KEYBLADE, 3), of_kind(item_type::ARMOR, 3), of_kind(item_type::ACCESSORY, 3));
        assert!(keys.len() == 3 && armor.len() == 3 && acc.len() == 3, "the item table has keyblades, armor and accessories");
        // Own two of each (a second copy of the first, so a swap keeps a count above 0), through the game's own give.
        for &id in keys.iter().chain(&armor).chain(&acc).chain(&keys[..1]).chain(&armor[..1]).chain(&acc[..1]) {
            let ok = ee.call(0x001a_18d8, &[id as u64, INVENTORY as u64, 0], &[], STACK).expect("FUN_001a18d8") as u32 != 0;
            assert_eq!(port.give(id, INVENTORY, false), ok);
        }
        // The new game holds no weapon (0) in the main hand and the forms. The game's writers give the old id back
        // unconditionally, and its item lookup answers a null row for id 0 whose zero bytes bump count slot 0 (save
        // `+0xe28`) -- a junk side effect the port does not model (`give` refuses an unknown id) and the menu never
        // reaches (a keyblade is always held). So start from held keyblades, set the same way in both blocks.
        for o in std::iter::once(r).chain((1..=4u8).map(|f| Progress::form_off(f).unwrap())) {
            ee.write(party + o as u32, &keys[0].to_le_bytes());
            port.set_u16(o, keys[0]);
        }
        assert_eq!(diff(&read(&mut ee), port.block()), Vec::<String>::new(), "after the purchases");
        // The game's four writers answer 0 or 1; the port the same. A swap, the same item again, a removal, a swap back.
        let mut swaps = 0;
        let mut run = |ee: &mut Ee, port: &mut Progress, what: &str, game: u32, args: &[u64], f: &dyn Fn(&mut Progress) -> bool| {
            let g = ee.call(game, args, &[], STACK).expect("the game's writer") as u32 & 0xff;
            let p = f(port);
            assert_eq!(g != 0, p, "{what}: the answer");
            assert_eq!(diff(&read(ee), port.block()), Vec::<String>::new(), "{what}");
            swaps += 1;
        };
        for &w in &[keys[1], keys[1], keys[2], keys[0], keys[0]] {
            run(&mut ee, &mut port, &format!("weapon {w}"), 0x0026_c420, &[w as u64], &|p| p.equip_weapon(SORA, w));
        }
        for (slot, id) in [(0usize, armor[0]), (0, armor[0]), (1, armor[1]), (0, armor[2]), (1, 0), (0, 0), (0, armor[1])] {
            run(&mut ee, &mut port, &format!("armor slot {slot} item {id}"), 0x0026_c588, &[slot as u64, id as u64], &|p| p.equip_armor(SORA, slot, id));
        }
        for (slot, id) in [(0usize, acc[0]), (1, acc[0]), (1, acc[1]), (0, 0), (0, acc[2]), (1, 0)] {
            run(&mut ee, &mut port, &format!("accessory slot {slot} item {id}"), 0x0026_c610, &[slot as u64, id as u64], &|p| p.equip_accessory(SORA, slot, id));
        }
        // A form's keyblade: forms 1, 3 and 4 (the Valor, Master and Final hands).
        for (form, id) in [(1u8, keys[1]), (3, keys[2]), (1, keys[1]), (1, keys[0]), (4, keys[1])] {
            run(&mut ee, &mut port, &format!("form {form} weapon {id}"), 0x0026_c4d0, &[form as u64, id as u64], &|p| p.equip_form_weapon(form, id));
        }
        eprintln!("oracle: equipment writers equal after {swaps} swaps (weapon, armor, accessory, form weapon; stock counts and slots)");
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

    /// A world entry of `00progress.bin` with `n` flags whose lists are given (the others end at once):
    /// the offset table, then a shared end byte, then each list.
    fn flag_entry(n: usize, lists: &[(u16, Vec<u8>)]) -> Vec<u8> {
        let end = 2 * n;
        let mut d = vec![0u8; end + 1];
        for k in 0..n {
            d[2 * k..2 * k + 2].copy_from_slice(&(end as u16).to_le_bytes());
        }
        for (index, list) in lists {
            let at = d.len() as u16;
            d[2 * *index as usize..2 * *index as usize + 2].copy_from_slice(&at.to_le_bytes());
            d.extend_from_slice(list);
            d.push(0);
        }
        d
    }

    /// WP-E5 item 1: the flag array at 0x33f6b0 addressed by id (`FUN_001802e0` test, `FUN_00180298`
    /// clear, `FUN_0017ff60` set), without side effects.
    #[test]
    fn story_flags_set_test_and_clear_by_id() {
        let mut s = Story::default();
        assert!(!s.test(0x883), "a new game has every flag clear");
        s.set(0x883);
        // Page 2 (0x883 >> 10), word 4 ((0x83) >> 5), bit 3.
        assert_eq!(Story::slot(0x883), Some((2 * 8 + 4, 1 << 3)));
        assert!(s.test(0x883) && !s.test(0x882) && !s.test(0x483));
        s.set(0x8db);
        s.set(0x0);
        assert_eq!(s.set_flags(), vec![0x0, 0x883, 0x8db]);
        s.clear(0x883);
        assert!(!s.test(0x883) && s.test(0x8db));
        // 0xffff always tests clear; a page past the 19 worlds is not in the array.
        assert!(!s.test(0xffff));
        s.set(19 << 10);
        assert!(!s.test(19 << 10));
        assert_eq!(s.set_flags(), vec![0x0, 0x8db]);
        assert_eq!(s.programs(2, 4), [0, 0, 0]);
    }

    /// `FUN_0017ff60`'s side effects on a synthetic world-2 entry: op 1 writes the program bytes of
    /// the flag's world (0xff keeps), op 2 a lock entry, op 0xc sets flags not yet set (and their
    /// lists run), op 7 clears, op 4 / 3 write the lock state; op 6 is recorded as not ported.
    #[test]
    fn story_flag_side_effects_write_programs_locks_and_flags() {
        let lists = vec![
            // 0x883: area 4 btl 1; lock entry 3 = (5, 0x1234); set 0x805.
            (0x83, vec![1, 2, 4, 0xff, 1, 0xff, 2, 3, 3, 0, 5, 0, 0x34, 0x12, 0xc, 1, 0x05, 0x08]),
            // 0x805: clear 0x806; lock entry 3 state 0xffff; op 6.
            (0x05, vec![7, 1, 0x06, 0x08, 4, 1, 3, 0, 6, 1, 2, 0]),
            // 0x807: lock entry 3 state 0.
            (0x07, vec![3, 1, 3, 0]),
        ];
        let mut s = Story { effects: Arc::new(vec![Vec::new(), Vec::new(), flag_entry(0x100, &lists)]), ..Story::default() };
        s.set(0x806);
        s.set(0x883);
        assert_eq!(s.programs(2, 4), [0, 1, 0]);
        assert_eq!(s.programs(2, 3), [0, 0, 0]);
        assert!(s.test(0x805) && !s.test(0x806), "0xc set 0x805, whose op 7 cleared 0x806");
        assert_eq!((s.lock_state(3), s.lock_message(3)), (0xffff, 0x1234));
        assert_eq!(s.unported, vec![(0x805, 4), (0x805, 6)]);
        s.set(0x807);
        assert_eq!(s.lock_state(3), 0);
        assert_eq!(s.lock_state(40), 0, "entries no flag wrote read 0");
        // A flag of another world (no entry) only sets its bit.
        s.set(0x1883);
        assert!(s.test(0x1883) && s.programs(6, 4) == [0; 3]);
    }

    /// The world-point bits (0x33f910, `FUN_00194fa8`) are kept in the save block's story and ride the
    /// state file as `world_points`.
    #[test]
    fn world_point_bits_are_kept_and_round_trip() {
        let ix = Arc::new(ItemIndex::default());
        let p = Progress::from_block(&[], ix.clone());
        assert!(p.story().world_points().is_empty());
        assert_eq!(p.story_json(), r#"{"story_flags": []}"#, "a new game has none");
        p.story().set_world_point(1);
        p.story().set_world_point(33);
        p.story().set_world_point(0x36 + 100); // past the two words: ignored
        assert!(p.story().world_point(1) && p.story().world_point(33) && !p.story().world_point(2));
        let text = p.story_json();
        assert_eq!(text, r#"{"story_flags": [], "world_points": [1, 33]}"#);
        let mut q = Progress::from_block(&[], ix);
        q.apply_json(&text).unwrap();
        assert_eq!(q.story().world_points(), vec![1, 33]);
    }

    /// The `story_flags` / `story_clear` keys of the state file, and `story_json` read back.
    #[test]
    fn story_flags_round_trip_through_the_state_file() {
        let ix = Arc::new(ItemIndex::default());
        let mut p = Progress::from_block(&[], ix.clone());
        p.apply_json(r#"{"story_flags": [2179, 2267, 4096, 2050], "story_clear": [2050]}"#).unwrap();
        assert_eq!(p.story().set_flags(), vec![0x883, 0x8db, 0x1000]);
        let text = p.story_json();
        assert_eq!(text, r#"{"story_flags": [2179, 2267, 4096]}"#);
        let mut q = Progress::from_block(&[], ix);
        q.apply_json(&text).unwrap();
        assert_eq!(q.story().set_flags(), p.story().set_flags());
        assert!(q.apply_json(r#"{"story_flags": 3}"#).is_err());
        // A clone shares the story (the hosts' handle is the same store).
        let r = q.clone();
        q.story().clear(0x883);
        assert!(!r.story().test(0x883));
    }

    /// The oracle for `FUN_0017ff60` (WP-E5): for every flag of every world entry of `00progress.bin`,
    /// the game's setter runs in kh2-ee on a cleared save block and the port must leave the same flag
    /// words (0x33f6b0, 0x260 bytes), program table (0x33e86f, 19 * 0xc0 bytes) and lock halfwords
    /// (0x33f918, 4 per `dsa` entry). The calls after the list (`FUN_001803b0` .. `FUN_00195130`, which
    /// reach menus, tutorials and the world map) are `jr ra` stubs: they do not write these regions
    /// [inferred from their names in docs/events.md section 5; not read]. The world-point table
    /// `DAT_01c8a984` points at zeros, so ops 4 and 5 find no point (as the port, which leaves the bits).
    #[test]
    fn story_flag_setter_matches_the_game() {
        use kh2_ee::Ee;
        const STACK: u32 = 0x01ff_0000;
        let Some(mut tb) = tables() else { return };
        let file = tb.archive.read("00progress.bin").expect("00progress.bin");
        let bar = kh2_bar::parse(&file).expect("BAR");
        let entries: Vec<Vec<u8>> = bar.iter().filter(|e| e.kind == 2).take(story::WORLDS).map(|e| e.data.to_vec()).collect();
        let dsa = bar.iter().find(|e| e.tag == kh2_ard::progress::LOCK_TABLE).expect("dsa").data.to_vec();
        let locks = u16::from_le_bytes([dsa[0], dsa[1]]) as usize;
        let elf = ["SLUS_210.05", "SLUS_210.05;1"].iter().find_map(|p| tb.archive.iso().read(p).ok()).expect("executable");
        let mut ee = Ee::new();
        ee.load_elf(&elf).expect("elf");
        let mut at = 0x0180_0000u32;
        for (w, e) in entries.iter().enumerate() {
            ee.write(at, e);
            ee.w32(0x0035_ccc0 + 4 * w as u32, at);
            at = (at + e.len() as u32 + 0xf) & !0xf;
        }
        ee.write(at, &dsa);
        ee.w32(0x01c8_a988, at);
        at = (at + dsa.len() as u32 + 0xf) & !0xf;
        ee.write(at, &[0u8; 0x100]);
        ee.w32(0x01c8_a984, at);
        for stub in [0x001803b0u32, 0x00180450, 0x001805d0, 0x00180558, 0x001d39d0, 0x001d3238, 0x001b6da0, 0x001be9c8, 0x001d5718, 0x0019d890, 0x00195130] {
            ee.w32(stub, 0x03e0_0008);
            ee.w32(stub + 4, 0);
        }
        let effects = Arc::new(entries.clone());
        let flags_len = story::WORLDS * story::PAGE;
        let prog_len = story::WORLDS * story::PROGRAMS_PER_WORLD;
        let mut checked = 0;
        let mut with_effects = 0;
        for (w, e) in entries.iter().enumerate() {
            for f in 0..kh2_ard::progress::flag_count(e) as u16 {
                let id = (w as u16) << 10 | f;
                ee.write(story::FLAGS, &vec![0u8; flags_len]);
                ee.write(story::PROGRAMS, &vec![0u8; prog_len]);
                ee.write(story::LOCKS, &vec![0u8; 4 * locks]);
                ee.call(0x0017_ff60, &[id as u64], &[], STACK).unwrap_or_else(|e| panic!("FUN_0017ff60({id:#x}): {e:?}"));
                let mut s = Story { effects: effects.clone(), ..Story::default() };
                s.set(id);
                let words: Vec<u32> = (0..flags_len / 4).map(|k| ee.r32(story::FLAGS + 4 * k as u32)).collect();
                assert_eq!(s.words, words, "flag {id:#x}: flag words");
                let prog: Vec<u8> = (0..prog_len).map(|k| ee.r8(story::PROGRAMS + k as u32)).collect();
                assert_eq!(s.programs, prog, "flag {id:#x}: program table");
                for i in 0..locks {
                    let game = (ee.r16(story::LOCKS + 4 * i as u32), ee.r16(story::LOCKS + 4 * i as u32 + 2));
                    assert_eq!((s.lock_state(i), s.lock_message(i)), game, "flag {id:#x}: lock entry {i}");
                }
                checked += 1;
                with_effects += (s.set_flags().len() > 1 || s.programs != vec![0; prog_len] || s.locks.iter().any(|l| *l != [0, 0])) as usize;
            }
        }
        eprintln!("oracle: FUN_0017ff60 equal for {checked} flags of {} worlds ({with_effects} change more than their own bit)", entries.len());
        assert!(checked > 1000 && with_effects > 100, "{checked} {with_effects}");
    }

    /// WP-E5 item 3 data: on the disc, setting flag 0x883 gives tt04 btl program 1 (docs/events.md
    /// section 5), and 0x814 sets tt04 back to (0, 0, 0).
    #[test]
    fn flag_0x883_selects_tt04_btl_program_one() {
        let Some(mut tb) = tables() else { return };
        let file = tb.archive.read("00progress.bin").expect("00progress.bin");
        let entries: Vec<Vec<u8>> = kh2_bar::parse(&file).expect("BAR").iter().filter(|e| e.kind == 2).map(|e| e.data.to_vec()).collect();
        let mut s = Story { effects: Arc::new(entries), ..Story::default() };
        s.set(0x883);
        eprintln!("after 0x883: flags {:x?} tt04 {:?} unported {:x?}", s.set_flags(), s.programs(2, 4), s.unported);
        assert_eq!(s.programs(2, 4)[1], 1);
        s.set(0x814);
        assert_eq!(s.programs(2, 4), [0, 0, 0]);
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

    /// An item index without the disc: keyblades 10 and 11, armor 20, accessories 30 and 31, consumables 1 and 2
    /// (the count slot of each is its id).
    fn small_index() -> Arc<ItemIndex> {
        let row = |id: u16, kind: u8| ItemInfo { id, kind, flag0: 0, word4: 0, command: 0, slot: id };
        let rows = vec![row(1, 0), row(2, 0), row(10, 2), row(11, 2), row(20, item_type::ARMOR), row(30, item_type::ACCESSORY), row(31, item_type::ACCESSORY)];
        let by_id = rows.iter().enumerate().map(|(n, r)| (r.id, n)).collect();
        Arc::new(ItemIndex { rows, by_id, ..ItemIndex::default() })
    }

    /// The equipment writers keep the stock: the old item goes back, the new one comes out; the same item again
    /// and a slot that does not exist change nothing.
    #[test]
    fn equipment_writers_swap_with_the_stock_and_keep_the_counts() {
        let mut p = Progress::from_block(&[], small_index());
        let r = Progress::member_off(SORA).unwrap();
        p.set_u8(r + rec::ARMOR_SLOTS, 1);
        p.set_u8(r + rec::ACCESSORY_SLOTS, 2);
        p.add_item(10, 2);
        p.add_item(11, 1);
        p.add_item(20, 1);
        p.add_item(30, 1);
        // The weapon: 10 in, 10 out of two; then 11, which sends 10 back.
        assert!(p.equip_weapon(SORA, 10));
        assert_eq!((p.sora().weapons[0], p.item_count(10)), (10, 1));
        assert!(p.equip_weapon(SORA, 11));
        assert_eq!((p.sora().weapons[0], p.item_count(10), p.item_count(11)), (11, 2, 0));
        assert!(!p.equip_weapon(SORA, 11), "the same weapon changes nothing");
        assert_eq!((p.item_count(10), p.item_count(11)), (2, 0));
        // Armor: in, out (the slot empties, the item returns), no second slot.
        assert!(p.equip_armor(SORA, 0, 20));
        assert_eq!((p.sora().armor, p.item_count(20)), (vec![20], 0));
        assert!(!p.equip_armor(SORA, 0, 20));
        assert!(p.equip_armor(SORA, 0, 0));
        assert_eq!((p.sora().armor, p.item_count(20)), (vec![0], 1));
        assert!(!p.equip_armor(SORA, 0, 0), "an empty slot stays empty");
        assert!(!p.equip_armor(SORA, 1, 20), "the record has one armor slot");
        // Accessories: the two slots are separate words.
        assert!(p.equip_accessory(SORA, 1, 30));
        assert_eq!((p.sora().accessories, p.item_count(30)), (vec![0, 30], 0));
        assert!(p.equip_accessory(SORA, 0, 31) && p.item_count(31) == 0, "taking an item the stock does not have floors at 0");
        assert!(p.equip_accessory(SORA, 1, 31));
        assert_eq!((p.sora().accessories, p.item_count(30), p.item_count(31)), (vec![31, 31], 1, 0));
        // A form's keyblade: the form record's own word.
        assert!(p.equip_form_weapon(1, 10));
        assert_eq!((p.form(1).weapon, p.item_count(10)), (10, 1));
        assert!(p.equip_form_weapon(1, 11) && p.form(1).weapon == 11 && p.item_count(10) == 2);
        assert!(!p.equip_form_weapon(0, 10) && !p.equip_form_weapon(10, 10), "forms 1 to 9 only");
        // A full stock loses the old item (the menu refuses that swap before it gets here).
        p.set_count(10, 99);
        assert!(p.equip_weapon(SORA, 10));
        assert_eq!((p.sora().weapons[0], p.item_count(10), p.item_count(11)), (10, 98, 1));
        p.set_count(10, 99);
        assert!(p.equip_weapon(SORA, 11));
        assert_eq!((p.sora().weapons[0], p.item_count(10), p.item_count(11)), (11, 99, 0), "the old 10 had no room and is gone");
        assert_eq!(p.equipment(), vec![11, 31, 31]);
    }

    /// `FUN_0026c770` then `FUN_001877c8`: an item moved between slots keeps its reservation, and a slot that
    /// reserves an item the stock has is refilled.
    #[test]
    fn leaving_the_equipment_lists_settles_the_reservations_and_refills() {
        let mut p = Progress::from_block(&[], small_index());
        let r = Progress::member_off(SORA).unwrap();
        p.set_u8(r + rec::ITEM_SLOTS, 4);
        // Slot 0 reserves item 1 but is empty, slot 1 carries item 1 without a reservation, slot 2 carries 2 and
        // reserves it, slot 3 reserves 2 but is empty.
        for (i, (item, reserved)) in [(0u16, 1u16), (1, 0), (2, 2), (0, 2)].into_iter().enumerate() {
            p.set_u16(r + rec::ITEMS + 2 * i, item);
            p.set_u16(r + rec::RESERVED + 2 * i, reserved);
        }
        p.set_count(1, 1);
        p.settle_battle_items(SORA);
        // Slot 1 carries item 1, which another slot reserves: it takes the reservation; then the empty slots that
        // reserve something the stock has take it (item 1: one in stock, slot 0 gets it; item 2: none).
        assert_eq!(p.battle_items(SORA), vec![(1, 1), (1, 1), (2, 2), (0, 2)]);
        assert_eq!(p.item_count(1), 0);
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
