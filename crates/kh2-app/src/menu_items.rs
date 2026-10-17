//! The pause menu's Items pages, stage M3 (docs/menus.md 2.2 and section 13): page 1 (the members and
//! Stock, `FUN_0025ae18`), page 0xa (the Stock grid, `FUN_0025b6e0`, built by `FUN_0025b2a0`), page 0xb (the
//! Use / Throw away prompt of one stock item, `FUN_0025bfa8`, built by `FUN_0025bad0`), page 0xc (the target
//! list of Use, `FUN_0025c590`, built by `FUN_0025c2c8`), page 0xd (the "no effect" notice, `FUN_0025c930`) and
//! page 0xe (the Throw away question, `FUN_0025cba0`, built by `FUN_0025cae8`).
//!
//! This file is a child module of `menu` (`#[path] pub mod item_pages;` there), so it sees the menu's state
//! without that file exporting it. The pure part (no Bevy) is the rules, so the tests drive them:
//!
//! * `stock` is `FUN_00283a70` over the item table (a filter word from `FUN_00283810`, one entry per owned item),
//! * `can_use` is `FUN_001a15a0` (does the item change anything on that member),
//! * `prompt_choice` is `FUN_0025eab0` and the Use rows' availability (`FUN_0025e8a0`, `FUN_0025e928`),
//! * `Machine::press` is the page handlers' result switches.
//!
//! The party is Sora alone (docs/party.md): page 1 lists Sora and Stock, and Sora's row opens page 2
//! (equipment, stage M4), which is not ported: the row refuses with the refused sound.

use std::collections::BTreeMap;

use bevy::prelude::*;
use kh2_image::text::{Style, Token};
use kh2_tables::tables as t;

use super::{Menu, Phase, Switch};
use crate::assets::Disc;
use crate::audio::PlaySe;
use crate::items::{item_effect, Vitals};
use super::new_tags::{self, NewTags};
use crate::menu_gfx::{MenuScene, Node};
use super::pictures::Pictures;
use crate::menu_pages::{Entry, Exe};
use crate::menu_text::{Label, MenuText};
use crate::progress::Progress;

// --- pages --------------------------------------------------------------------------------------

pub const MEMBERS: u16 = 1;
pub const STOCK: u16 = 0xa;
pub const PROMPT: u16 = 0xb;
pub const TARGET: u16 = 0xc;
pub const NOTICE: u16 = 0xd;
pub const DISCARD: u16 = 0xe;
/// Entry 0 of the top list (Items) opens page 1.
pub const ENTRY: usize = 0;

/// Cells of the Stock grid: two columns, nine rows visible (`FUN_00298e40(list, 2, 9)`, 0x12 cells).
pub const COLUMNS: usize = 2;
pub const ROWS: usize = 9;
/// The Stock page cycles five filters (`FUN_0025e838`: -10 adds one, -11 subtracts, wrapping at 5).
pub const FILTERS: u8 = 5;

/// Sounds of the system bank (`PlaySe::number(2000 + id)`, docs/menus.md 9): cursor 1, confirm 2, cancel 4,
/// refused 5; the two kinds this module adds are `FUN_0023e738(8, 1)` (an item that would do nothing: kind 8 is
/// not below 6, so its id is the u16 at 0x37f654 + 2 * 8 = 2, the confirm sound) and `FUN_0023e738(0x11, 1)` (an
/// item used: the u16 at 0x37f654 + 2 * 0x11 = 37).
pub mod snd {
    pub const CURSOR: u32 = 2001;
    pub const CONFIRM: u32 = 2002;
    pub const CANCEL: u32 = 2004;
    pub const REFUSED: u32 = 2005;
    pub const USED: u32 = 2037;
}

// --- the item rows and the executable's tables ----------------------------------------------------

/// What the pages read of one item row (`item`, 0x18 bytes): `Type` (+2), `Flag1 | Rank << 8` (+4), `Name`
/// (+8), `Description` (+0xa), `ShopSell` (+0xe), `Icon2` (+0x17).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ItemRow {
    pub id: u16,
    pub kind: u8,
    pub amount: u16,
    pub name: u16,
    pub description: u16,
    /// `FUN_0025e928`: the item can be thrown away when this is not 0.
    pub sell: u16,
    pub icon: u8,
}

impl ItemRow {
    pub fn of(i: &t::Item) -> ItemRow {
        ItemRow {
            id: i.id,
            kind: i.r#type,
            amount: i.flag1 as u16 | (i.rank as u16) << 8,
            name: i.name,
            description: i.description,
            sell: i.shop_sell,
            icon: i.icon2,
        }
    }

    /// `FUN_0025e928`.
    pub fn can_discard(&self) -> bool {
        self.sell != 0
    }
}

/// The tables the pages read from the executable.
#[derive(Clone, Debug, Default)]
pub struct ExeTables {
    /// 0x381060: the item types a Use row is offered for (signed bytes, a negative one ends): `FUN_0025e8a0`.
    pub usable_types: Vec<u8>,
    /// 0x381068: items that go to one member only (signed words, a negative one ends): `FUN_0025e978`.
    pub single: Vec<u16>,
    /// 0x381090: the labels of the prompt's rows, Use then Throw away.
    pub prompt: [u32; 2],
    /// 0x3823d0: (character, bit) pairs; the filter word `FUN_00283810` ORs the bits of the pairs for filters
    /// 0 and 2 and 4.
    pub filter_bits: Vec<(u8, u8)>,
    /// 0x382450: (story flag, item) pairs: an item is not listed while its flag is set (`FUN_002846f0`).
    pub hidden: Vec<(u16, u16)>,
    /// 0x380a60: (item type, message of the type's name), a negative type ends (`FUN_002553c0`).
    pub type_names: Vec<(i16, u16)>,
    /// 0x36cf28, 0x36cf2a, 0x36cf2c: the messages of the notice's OK and of the question's Yes and No.
    pub ok: u32,
    pub yes: u32,
    pub no: u32,
    /// 0x3809d0: (item type, icon unequipped, icon equipped) until a negative type (`FUN_00255368`).
    pub icons: Vec<(i16, i16, i16)>,
}

impl ExeTables {
    /// `FUN_00255368`: the icon of an item type for a flag (0xb when the type is not listed).
    pub fn icon(&self, kind: u8, flag: bool) -> u8 {
        self.icons.iter().find(|t| t.0 == kind as i16).map_or(0xb, |t| if flag { t.2 as u8 } else { t.1 as u8 })
    }

    pub fn read(exe: &Exe) -> Option<ExeTables> {
        let w = |va: u32| exe.u16(va);
        let msg = |va: u32| w(va).map(|v| u32::from(v & 0x7fff));
        let mut t = ExeTables::default();
        for k in 0..16u32 {
            let b = exe.u8(0x0038_1060 + k)? as i8;
            if b < 0 {
                break;
            }
            t.usable_types.push(b as u8);
        }
        for k in 0..16u32 {
            let v = w(0x0038_1068 + 2 * k)? as i16;
            if v < 0 {
                break;
            }
            t.single.push(v as u16);
        }
        t.prompt = [msg(0x0038_1090)?, msg(0x0038_1092)?];
        for k in 0..32u32 {
            let a = exe.u8(0x0038_23d0 + 2 * k)? as i8;
            if a < 0 {
                break;
            }
            t.filter_bits.push((a as u8, exe.u8(0x0038_23d0 + 2 * k + 1)?));
        }
        for k in 0..32u32 {
            let flag = w(0x0038_2450 + 4 * k)?;
            if flag == 0 {
                break;
            }
            t.hidden.push((flag, w(0x0038_2450 + 4 * k + 2)?));
        }
        for k in 0..32u32 {
            let ty = w(0x0038_0a60 + 4 * k)? as i16;
            if ty < 0 {
                break;
            }
            t.type_names.push((ty, w(0x0038_0a60 + 4 * k + 2)? & 0x7fff));
        }
        t.ok = msg(0x0036_cf28)?;
        t.yes = msg(0x0036_cf2a)?;
        t.no = msg(0x0036_cf2c)?;
        let mut at = 0x0038_09d0u32;
        loop {
            let ty = w(at)? as i16;
            if ty < 0 || t.icons.len() > 64 {
                break;
            }
            t.icons.push((ty, w(at + 2)? as i16, w(at + 4)? as i16));
            at += 6;
        }
        Some(t)
    }

    /// `FUN_0025e8a0`: is a Use row offered for the item's type.
    pub fn usable(&self, kind: u8) -> bool {
        self.usable_types.contains(&kind)
    }

    /// `FUN_0025e978`: does the item go to one member only.
    pub fn single_target(&self, id: u16) -> bool {
        self.single.contains(&id)
    }

    /// `FUN_002553c0`: the message of an item type's name.
    pub fn type_name(&self, kind: u8) -> Option<u32> {
        self.type_names.iter().find(|p| p.0 == kind as i16).map(|p| u32::from(p.1))
    }

    /// `FUN_00283810(list, 0, arg)`: the filter word of the third argument the five stubs at 0x25e7e0..0x25e830
    /// pass to `FUN_00283a70`: filter 0 -> 0 (everything), 1 -> 0xd, 2 -> 0xb, 3 -> 0xc, 4 -> 0xe. An item is
    /// listed when bit `Type` of the word is set.
    pub fn filter_word(&self, filter: u8) -> u32 {
        let arg = match filter {
            0 => 0u32,
            1 => 0xd,
            2 => 0xb,
            3 => 0xc,
            _ => 0xe,
        };
        let mut w = 0u32;
        if matches!(arg, 0 | 1 | 0xb) {
            for &(_, bit) in &self.filter_bits {
                w |= 1 << (bit & 0x1f);
            }
        }
        for (cond, bits) in [
            (matches!(arg, 2 | 0), 0x4000u32),
            (matches!(arg, 3 | 0), 0x8000),
            (matches!(arg, 4 | 0 | 10), 1),
            (matches!(arg, 5 | 0), 2),
            (matches!(arg, 6 | 0), 0x1_0000),
            (matches!(arg, 7 | 0), 0x2_0000),
            (matches!(arg, 8 | 0), 0x10_0000),
            (matches!(arg, 9 | 10), 0x4_0000),
            (matches!(arg, 0xc | 0), 0xc000),
            (matches!(arg, 0xd | 0), 3),
            (matches!(arg, 0xe | 0), 0x13_0000),
        ] {
            if cond {
                w |= bits;
            }
        }
        // `arg` 1 (a character filter) never occurs here: the stubs pass 0, 0xd, 0xb, 0xc and 0xe, and the
        // first branch only matters for 0 and 0xb.
        if arg == 0xd {
            w &= 3;
        }
        w
    }
}

// --- the Stock list --------------------------------------------------------------------------------

/// One entry of the Stock list: the item and how many are owned (`FUN_00283a70`: the count's low byte).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Stack {
    pub id: u16,
    pub count: u8,
}

/// `FUN_00283a70` followed by the sort `FUN_002845a0`: walk the item table in order and list every row whose
/// type is in the filter word, that the player owns at least one of (`FUN_001a1cc8`) and that no set story flag
/// hides. The sort key of an entry is its table position (plus 1000 unless it is a spell, which never lists),
/// so the order is the table's.
pub fn stock(rows: &[ItemRow], word: u32, exe: &ExeTables, count: &dyn Fn(u16) -> u8, flag: &dyn Fn(u16) -> bool) -> Vec<Stack> {
    rows.iter()
        .filter(|r| word >> (r.kind & 0x1f) & 1 != 0)
        .filter(|r| !exe.hidden.iter().any(|&(f, item)| item == r.id && flag(f)))
        .filter_map(|r| {
            let n = count(r.id);
            (n != 0).then_some(Stack { id: r.id, count: n })
        })
        .collect()
}

// --- Use -------------------------------------------------------------------------------------------

/// What `FUN_001a15a0` reads of the target: its status record and its character record.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Target {
    pub vitals: Vitals,
    /// The record bytes the boosts raise (`+8` AP, `+9` strength, `+0xa` magic, `+0xb` defense); `None` when the
    /// status has no record (`+0x238` is 0), which answers "usable".
    pub record: Option<[u8; 4]>,
}

/// `FUN_001a15a0(item row, target status)`: whether using the item on the target changes anything (the Use row's
/// target check, `FUN_0025e9e8` through `FUN_001a14c8`). HP items (1, 2, 5) need HP below the maximum; MP items
/// (3, 6) need MP below the maximum or the MP charge running (`FUN_001a05d0`: the charge total above 0); the
/// combined items (4, 7, 0x83) are usable unless HP is full and neither a charge runs nor MP is full; 0x112 and
/// 0x113 need drive points below the maximum; the boosts (0x114 .. 0x117) need the record byte not above 99.
pub fn can_use(id: u16, t: &Target) -> bool {
    let v = &t.vitals;
    let charging = v.charge_total > 0.0;
    let hp = v.hp < v.hp_max;
    let both = !(v.hp_max <= v.hp && !charging && v.mp_max <= v.mp);
    let boost = |byte: usize| t.record.is_none_or(|r| r[byte] <= 99);
    match id {
        1 | 2 | 5 => hp,
        3 | 6 => charging || v.mp < v.mp_max,
        4 | 7 | 0x83 => both,
        0x112 | 0x113 => v.drive_points < v.drive_max,
        0x114 => boost(1),
        0x115 => boost(2),
        0x116 => boost(3),
        0x117 => boost(0),
        _ => false,
    }
}

/// `FUN_001a1fe0`: the items used on everyone at once (5, 6, 7 and 0x83).
pub fn group_item(id: u16) -> bool {
    (5..8).contains(&id) || id == 0x83
}

/// What a confirm on the prompt's row does (`FUN_0025eab0`): the Use row of a usable type goes to the target
/// list; any other row asks to throw the item away, unless the item has no sell price, which refuses
/// (`FUN_0025eba8` as the validity callback).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Choice {
    Use,
    Discard,
    Refused,
}

/// `FUN_0025eab0(row)` for the item at the cursor.
pub fn prompt_choice(usable: bool, row: usize, can_discard: bool) -> Choice {
    if row == 0 && usable {
        Choice::Use
    } else if can_discard {
        Choice::Discard
    } else {
        Choice::Refused
    }
}

/// The rows of the prompt: Use and Throw away for a usable type, Throw away alone otherwise
/// (`FUN_0025bad0`: two rows or one; the one row's label is the second message).
pub fn prompt_rows(usable: bool) -> Vec<usize> {
    if usable { vec![0, 1] } else { vec![1] }
}

/// The rows of the target list (`FUN_0025c2c8`): one row ("All") for a group item, one row for the items that go
/// to one member, else a row per party member. Answers the number of member rows and whether the first row is
/// "All".
pub fn target_rows(id: u16, single: bool, members: usize) -> (usize, bool) {
    if group_item(id) {
        (1, true)
    } else if single {
        (1, false)
    } else {
        (members, false)
    }
}

// --- the page machine ---------------------------------------------------------------------------------

/// What the machine asks of the game: whether the item would do something on a target row.
pub trait Game {
    /// The party's size (`FUN_00282940`).
    fn members(&self) -> usize;
    /// `FUN_0025e9e8(row)`: would the item change anything on the member of that row ("All": any member).
    fn would_help(&self, id: u16, row: usize) -> bool;
}

/// The list results of docs/menus.md 1.7 that the Items pages use.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Key {
    Up { fresh: bool },
    Down { fresh: bool },
    /// Left and Right inside the Stock grid (the grid's own movement, `FUN_00297cc0`).
    Left,
    Right,
    /// -> an index >= 0.
    Confirm,
    /// -2.
    Cancel,
    /// -4: leave the whole menu.
    Close,
    /// -10 / -11: the previous or next Stock filter.
    Prev,
    Next,
    /// -5: to the Abilities pages (page 1 only, `FUN_0025ae18`).
    Switch,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Effect {
    Sound(u32),
    /// The page changed (the screen redraws).
    Page(u16),
    /// Use the item on the target row: the caller applies it and takes one from the stock
    /// (`FUN_0025ea40`: `FUN_001a11a0` then `FUN_001a1ba8(item, 100)`), then calls `Machine::reload`.
    Use { item: u16, row: usize },
    /// Throw one away (`FUN_001a1ba8(item, 100)`), then `Machine::reload`.
    Discard { item: u16 },
    /// The Stock filter changed: the caller rebuilds the list and calls `Machine::reload`.
    Filter(u8),
    /// A member's row was confirmed: his equipment pages (page 2) take over (`menu_equipment.rs`).
    Member(usize),
    /// Back to the top list (page 0).
    Leave,
    /// -5: hand over to the Abilities pages.
    Switch,
    /// Close the whole menu.
    Close,
}

/// Pages 1, 0xa, 0xb, 0xc, 0xd, 0xe.
#[derive(Clone, Debug)]
pub struct Machine {
    pub page: u16,
    /// Page 1's rows are the members and Stock; its cursor.
    pub members: usize,
    pub cursor1: usize,
    pub stock: Vec<Stack>,
    pub filter: u8,
    /// Page 0xa's cursor (an index into `stock`) and the first visible row.
    pub cursor: usize,
    pub scroll: usize,
    /// Pages 0xb, 0xc, 0xe: the cursor of their list.
    pub prompt: usize,
    pub target: usize,
    pub question: usize,
    /// The item the prompt pages work on (the Stock cursor's item when the prompt opened).
    pub item: Option<ItemRow>,
    pub usable: bool,
    pub single: bool,
}

/// The two Left/Right-wrapping and Up/Down-moving rules of the Stock grid (`FUN_00297cc0`): the cursor of a grid
/// of `n` cells, two columns. Left and Right wrap inside the row (a short last row has one cell); Up and Down
/// move one row and wrap only on a new press, landing on the same column or, when the row is short, its last
/// cell [inferred: the clamp].
pub fn grid_move(cursor: usize, n: usize, key: Key) -> usize {
    if n == 0 {
        return 0;
    }
    let (row, col) = (cursor / COLUMNS, cursor % COLUMNS);
    let rows = n.div_ceil(COLUMNS);
    let in_row = |r: usize| if r + 1 == rows && !n.is_multiple_of(COLUMNS) { n % COLUMNS } else { COLUMNS };
    let land = |r: usize| r * COLUMNS + col.min(in_row(r) - 1);
    match key {
        Key::Left => row * COLUMNS + (col + in_row(row) - 1) % in_row(row),
        Key::Right => row * COLUMNS + (col + 1) % in_row(row),
        Key::Up { fresh } if row > 0 => {
            let _ = fresh;
            land(row - 1)
        }
        Key::Up { fresh: true } => land(rows - 1),
        Key::Down { .. } if row + 1 < rows => land(row + 1),
        Key::Down { fresh: true } => land(0),
        _ => cursor,
    }
}

impl Machine {
    pub fn new(members: usize, stock: Vec<Stack>) -> Machine {
        Machine {
            page: MEMBERS,
            members: members.max(1),
            cursor1: 0,
            stock,
            filter: 0,
            cursor: 0,
            scroll: 0,
            prompt: 0,
            target: 0,
            question: 0,
            item: None,
            usable: false,
            single: false,
        }
    }

    /// The first visible row keeps the highlighted one on the screen (nine rows of two cells).
    fn keep_visible(&mut self) {
        let row = self.cursor / COLUMNS;
        if row < self.scroll {
            self.scroll = row;
        } else if row >= self.scroll + ROWS {
            self.scroll = row + 1 - ROWS;
        }
    }

    /// The Stock list changed under the cursor (a use or a throw-away took one, a filter changed): `FUN_0025eb20`
    /// keeps the cursor on the list; the rule for a list that became shorter than the cursor is read as "the
    /// last entry" [inferred: the original steps the scroll and the cursor back by a row or a cell].
    pub fn reload(&mut self, stock: Vec<Stack>, reset: bool) {
        self.stock = stock;
        if reset {
            self.cursor = 0;
            self.scroll = 0;
        } else if self.cursor >= self.stock.len() {
            self.cursor = self.stock.len().saturating_sub(1);
        }
        self.keep_visible();
    }

    fn current(&self) -> Option<Stack> {
        self.stock.get(self.cursor).copied()
    }

    /// One key on the current page, with the item rows and tables to decide by.
    pub fn press(&mut self, key: Key, rows: &BTreeMap<u16, ItemRow>, exe: &ExeTables, game: &dyn Game) -> Vec<Effect> {
        use Effect::{Page, Sound};
        let mut fx = Vec::new();
        if key == Key::Close {
            fx.push(Effect::Close);
            return fx;
        }
        match self.page {
            MEMBERS => {
                let n = self.members + 1;
                match key {
                    Key::Up { fresh } | Key::Down { fresh } => {
                        let dir = if matches!(key, Key::Up { .. }) { -1 } else { 1 };
                        let new = super::step_cursor(self.cursor1, n, dir, fresh);
                        if new != self.cursor1 {
                            self.cursor1 = new;
                            fx.push(Sound(snd::CURSOR));
                            fx.push(Page(MEMBERS));
                        }
                    }
                    Key::Confirm if self.cursor1 == self.members => {
                        self.page = STOCK;
                        self.filter = 0;
                        fx.push(Sound(snd::CONFIRM));
                        fx.push(Effect::Filter(0));
                        fx.push(Page(STOCK));
                    }
                    // A member's row opens page 2, his equipment (stage M4, `menu_equipment.rs`).
                    Key::Confirm => {
                        fx.push(Sound(snd::CONFIRM));
                        fx.push(Effect::Member(self.cursor1));
                    }
                    Key::Cancel => {
                        fx.push(Sound(snd::CANCEL));
                        fx.push(Effect::Leave);
                    }
                    // `FUN_0025ae18`: -5 sets the target page 0xf (the list result's sound kind is 2, the confirm).
                    Key::Switch => {
                        fx.push(Sound(snd::CONFIRM));
                        fx.push(Effect::Switch);
                    }
                    _ => {}
                }
            }
            STOCK => match key {
                Key::Up { .. } | Key::Down { .. } | Key::Left | Key::Right => {
                    let new = grid_move(self.cursor, self.stock.len(), key);
                    if new != self.cursor {
                        self.cursor = new;
                        self.keep_visible();
                        fx.push(Sound(snd::CURSOR));
                        fx.push(Page(STOCK));
                    }
                }
                // With an empty list the confirm is not offered (`FUN_0025b2a0` clears the mask's 0x10).
                Key::Confirm => {
                    if let Some(s) = self.current() {
                        self.item = rows.get(&s.id).copied();
                        self.usable = self.item.is_some_and(|r| exe.usable(r.kind));
                        self.single = exe.single_target(s.id);
                        self.prompt = 0;
                        self.page = PROMPT;
                        fx.push(Sound(snd::CONFIRM));
                        fx.push(Page(PROMPT));
                    }
                }
                Key::Cancel => {
                    self.page = MEMBERS;
                    self.cursor1 = self.members;
                    fx.push(Sound(snd::CANCEL));
                    fx.push(Page(MEMBERS));
                }
                Key::Prev | Key::Next => {
                    self.filter = if key == Key::Next { (self.filter + 1) % FILTERS } else { (self.filter + FILTERS - 1) % FILTERS };
                    // The filter step is list result -10 / -11 (`FUN_0025b6e0` -> `FUN_0025e838`); `FUN_00295bf0`
                    // maps those to sound kind 2, the confirm sound, not the cursor's (WP-F1 item 9).
                    fx.push(Sound(snd::CONFIRM));
                    fx.push(Effect::Filter(self.filter));
                    fx.push(Page(STOCK));
                }
                _ => {}
            },
            PROMPT => {
                let n = prompt_rows(self.usable).len();
                match key {
                    Key::Up { fresh } | Key::Down { fresh } => {
                        let dir = if matches!(key, Key::Up { .. }) { -1 } else { 1 };
                        let new = super::step_cursor(self.prompt, n, dir, fresh);
                        if new != self.prompt {
                            self.prompt = new;
                            fx.push(Sound(snd::CURSOR));
                            fx.push(Page(PROMPT));
                        }
                    }
                    Key::Confirm => {
                        let can = self.item.is_some_and(|r| r.can_discard());
                        match prompt_choice(self.usable, self.prompt, can) {
                            Choice::Use => {
                                self.target = 0;
                                self.page = TARGET;
                                fx.push(Sound(snd::CONFIRM));
                                fx.push(Page(TARGET));
                            }
                            Choice::Discard => {
                                self.question = 0;
                                self.page = DISCARD;
                                fx.push(Sound(snd::CONFIRM));
                                fx.push(Page(DISCARD));
                            }
                            Choice::Refused => fx.push(Sound(snd::REFUSED)),
                        }
                    }
                    Key::Cancel => {
                        self.page = STOCK;
                        fx.push(Sound(snd::CANCEL));
                        fx.push(Page(STOCK));
                    }
                    _ => {}
                }
            }
            TARGET => {
                let id = self.item.map_or(0, |r| r.id);
                let (n, _) = target_rows(id, self.single, game.members());
                match key {
                    Key::Up { fresh } | Key::Down { fresh } => {
                        let dir = if matches!(key, Key::Up { .. }) { -1 } else { 1 };
                        let new = super::step_cursor(self.target, n, dir, fresh);
                        if new != self.target {
                            self.target = new;
                            fx.push(Sound(snd::CURSOR));
                            fx.push(Page(TARGET));
                        }
                    }
                    Key::Confirm => {
                        if game.would_help(id, self.target) {
                            // `FUN_0025c590`: sound kind 0x11, the use, then back to the Stock list.
                            fx.push(Sound(snd::USED));
                            fx.push(Effect::Use { item: id, row: self.target });
                            self.page = STOCK;
                            fx.push(Page(STOCK));
                        } else {
                            // Sound kind 8, then the notice.
                            fx.push(Sound(snd::CONFIRM));
                            self.page = NOTICE;
                            fx.push(Page(NOTICE));
                        }
                    }
                    Key::Cancel => {
                        self.page = PROMPT;
                        fx.push(Sound(snd::CANCEL));
                        fx.push(Page(PROMPT));
                    }
                    _ => {}
                }
            }
            NOTICE => {
                // `FUN_0025c930`: the cancel and every row go back to the target list.
                if matches!(key, Key::Confirm | Key::Cancel) {
                    self.page = TARGET;
                    fx.push(Sound(snd::CANCEL));
                    fx.push(Page(TARGET));
                }
            }
            DISCARD => match key {
                Key::Up { fresh } | Key::Down { fresh } => {
                    let dir = if matches!(key, Key::Up { .. }) { -1 } else { 1 };
                    let new = super::step_cursor(self.question, 2, dir, fresh);
                    if new != self.question {
                        self.question = new;
                        fx.push(Sound(snd::CURSOR));
                        fx.push(Page(DISCARD));
                    }
                }
                Key::Confirm if self.question == 1 => {
                    let id = self.item.map_or(0, |r| r.id);
                    fx.push(Sound(snd::CONFIRM));
                    fx.push(Effect::Discard { item: id });
                    self.page = STOCK;
                    fx.push(Page(STOCK));
                }
                Key::Confirm | Key::Cancel => {
                    self.page = PROMPT;
                    fx.push(Sound(snd::CANCEL));
                    fx.push(Page(PROMPT));
                }
                _ => {}
            },
            _ => {}
        }
        fx
    }
}

// ---------------------------------------------------------------------------------------------
// The screen.

/// Group ids of `camp.2ld` sequence 1 the pages name with literals (`FUN_0025b2a0`, `FUN_0025bad0`,
/// `FUN_0025c2c8`, `FUN_0025cae8`; the tab, hand and glint are the same objects as on page 0 and the Abilities
/// pages).
mod group {
    pub const TAB: [usize; 3] = [0x4b, 0x4d, 0x4c];
    pub const HAND: usize = 0x19;
    pub const GLINT: [usize; 3] = [0x1b, 0x1d, 0x1c];
    /// The preview window of page 1 (`FUN_002794c8`'s list window, 0xe6).
    pub const WINDOW: usize = 0xe6;
    /// The Stock panel (enter, loop: `FUN_0025b2a0`), its idle and selected cell groups (`0x130`, `0x12f`) and the
    /// group of the "no items" cell (`FUN_0025dae8`).
    pub const STOCK_PANEL: [usize; 2] = [0x125, 0x126];
    pub const CELL: usize = 0x130;
    pub const CELL_SELECTED: usize = 0x12f;
    pub const CELL_EMPTY: usize = 0x12e;
    /// The prompt panel (`FUN_0025bad0`: 0x128 enter, 0x129 loop), the item window beside it (0x131, 0x132), the
    /// target panel under it (`FUN_0025c2c8`: 0x12b, 0x12c), the title strip of both (0x124) and the row plate
    /// (0x123, every set of `FUN_00296b48` in both builders).
    pub const PROMPT_PANEL: [usize; 2] = [0x128, 0x129];
    pub const ITEM_WINDOW: [usize; 2] = [0x131, 0x132];
    pub const TARGET_PANEL: [usize; 2] = [0x12b, 0x12c];
    pub const TITLE: usize = 0x124;
    pub const ROW: usize = 0x123;
    /// The information window (`FUN_00253398`, the groups at 0x380880 and 0x380888, in the order of every group
    /// table of the camp: enter, leave, hold): the OK window of the notice and the larger Yes / No window of the
    /// question.
    pub const INFO_OK: [usize; 3] = [6, 8, 7];
    pub const INFO_ASK: [usize; 3] = [9, 0xb, 0xa];
}

/// Layout in menu units (before the 16 line offset). The positions come from the groups' own anchors; what
/// the code does not give is placed by eye and marked in docs/menus.md section 13.
mod at {
    pub const ROW_PITCH: f32 = 26.0;
    pub const HAND: [f32; 2] = [-13.0, 0.0];
    pub const GLINT: [f32; 2] = [94.0, -3.0];
    pub const TAB_LABEL: [f32; 2] = [0.0, 1.0];
    pub const TAB_PITCH: f32 = 120.0;
    pub const HELP: [f32; 2] = [137.0, 337.0];
    pub const COLOR: [u8; 4] = [0xe5, 0xe5, 0xe5, 0x80];
    pub const GREY: [u8; 4] = [0x8f, 0x8f, 0x9f, 0x80];
    pub const SELECTED: [u8; 4] = [0xe5, 0xe5, 0xe5, 0x80];
    pub const HELP_COLOR: [u8; 4] = [0x60, 0xf7, 0xff, 0x80];
    pub const LABEL: [f32; 2] = [0.0, 1.0];
    /// The right edge of a stock cell's count from the cell's anchor (the selected cell group is 216 wide).
    pub const COUNT_RIGHT: f32 = 204.0;
    /// The item picture inside the item window (absolute menu units; compared with clip 6 at 12.5 s).
    pub const PICTURE: [f32; 4] = [318.0, 82.0, 446.0, 210.0];
    /// Where the information window's text and rows start from its anchor (255, 118) / (255, 126): by eye, no clip
    /// shows the window.
    pub const INFO_OK_TEXT: [f32; 2] = [-170.0, -4.0];
    pub const INFO_ASK_TEXT: [f32; 2] = [-210.0, -10.0];
    /// The first row of the window's list and the pitch between rows (the Yes / No window's group has 24).
    pub const INFO_OK_ROWS: [f32; 2] = [-30.0, 125.0];
    pub const INFO_ASK_ROWS: [f32; 2] = [-30.0, 98.0];
    pub const INFO_ASK_PITCH: f32 = 24.0;
}

/// "There are no items." (`FUN_0025dae8`: `0x845a`, bit 15 ignored).
const NO_ITEMS: u32 = 0x45a;
/// The Stock row of page 1 (`FUN_0025ae18`: the last row, message 0x455).
const STOCK_LABEL: u32 = 0x455;

struct Ui {
    tabs: [usize; 3],
    rows: Vec<usize>,
    hand: usize,
    glint: usize,
    window: usize,
    preview: Vec<usize>,
    panel: usize,
    cells: Vec<usize>,
    counts: Vec<usize>,
    /// The NEW tag of each Stock cell (a child of the cell) and of page 1's Stock row (stage M7).
    cell_tags: Vec<usize>,
    stock_tag: usize,
    prompt_panel: usize,
    prompt_title: usize,
    prompt_rows: Vec<usize>,
    item_window: usize,
    target_panel: usize,
    target_title: usize,
    target_rows: Vec<usize>,
    info: usize,
    info_text: usize,
    info_rows: Vec<usize>,
    /// The "no items" text (its own group 0x12e, placed by the group's absolute anchor).
    empty: usize,
    help: usize,
    all: Vec<usize>,
}

#[derive(Resource, Default)]
pub struct ItemsMenu {
    pub(super) ready: bool,
    /// The item rows in table order, and by id.
    pub(super) order: Vec<ItemRow>,
    /// Item id -> picture id (`item +0x14`, -1 none): `itempic/item-NNN.imd`.
    pub(super) pictures: BTreeMap<u16, i16>,
    pub(super) rows: BTreeMap<u16, ItemRow>,
    pub(super) exe: Option<ExeTables>,
    /// Entry 0 of the top list (Items): its name message and group sets.
    entry: Option<Entry>,
    machine: Option<Machine>,
    ui: Option<Ui>,
    /// Page 0's nodes that were visible when the pages took over.
    hidden: Vec<usize>,
    held: (i8, f32),
    /// The equipment pages (page 2 and below, `menu_equipment.rs`) own the keys and the screen: this module
    /// neither reads keys nor draws while it is set, and keeps its machine (page 1's cursor) for the way back.
    pub(super) suspended: bool,
    /// Set by the confirm on a member's row, taken by the equipment module.
    equipment_requested: bool,
    /// The party's member ids in row order (Sora, then the field members by party slot; `FUN_00282968`'s slots), set when
    /// page 1 opens.
    pub(super) party: Vec<u16>,
    /// The row of page 1 whose equipment page is open (`FUN_00252fb8(1)`), an index into `party`.
    pub(super) member: usize,
    /// The member name messages (the u16 table at 0x37ba90, by member id; bit 15 ignored).
    pub(super) member_names: Vec<u32>,
}

impl ItemsMenu {
    /// The name message of the Items entry of page 0 (the first header tab).
    pub(super) fn entry_name(&self) -> u32 {
        self.entry.as_ref().map_or(0, |e| e.name)
    }

    /// `FUN_0026cfc8`: the equipment pages stepped to another member; page 1's remembered cursor follows
    /// (`FUN_00252f98(1, row, 0)`).
    pub(super) fn set_member(&mut self, row: usize) {
        self.member = row;
        if let Some(m) = self.machine.as_mut() {
            m.cursor1 = row.min(m.members);
        }
    }

    /// Whether page 2 was just asked for (and clears the request).
    pub(super) fn take_equipment_request(&mut self) -> bool {
        std::mem::take(&mut self.equipment_requested)
    }
}

pub struct ItemPagesPlugin;

impl Plugin for ItemPagesPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ItemsMenu>()
            .add_systems(Startup, load)
            // The menu's own input clears every key after looking at them: this system runs first and clears
            // them itself while one of its pages is up.
            .add_systems(PreUpdate, input.after(bevy::input::InputSystems).after(crate::harness::Keys).before(super::input));
    }
}

pub(super) fn load(disc: Res<Disc>, mut st: ResMut<ItemsMenu>) {
    let items = {
        let Ok(mut a) = disc.0.lock() else { return };
        kh2_tables::load::<t::Item>(&mut a, &t::ITEM).ok().map(|d| d.rows)
    };
    let Some(items) = items else {
        warn!("menu: the item table is not readable; the Items pages are off");
        return;
    };
    st.order = items.iter().map(ItemRow::of).collect();
    st.pictures = items.iter().map(|i| (i.id, i.picture)).collect();
    for r in st.order.clone() {
        st.rows.entry(r.id).or_insert(r);
    }
    let Some(exe) = disc.read_root("SLUS_210.05").and_then(Exe::load) else {
        warn!("menu: the executable is not readable; the Items pages are off");
        return;
    };
    st.exe = ExeTables::read(&exe);
    st.entry = Entry::read(&exe, ENTRY);
    st.member_names = (0..16).filter_map(|i| exe.u16(0x0037_ba90 + 2 * i).map(|v| u32::from(v & 0x7fff))).collect();
    st.ready = st.exe.is_some() && st.entry.is_some();
    if !st.ready {
        warn!("menu: the executable's Items tables are not readable; the Items pages are off");
    }
}

/// The Stock list for a filter from the game state (no story flag is set in the app, so nothing is hidden).
pub fn stock_of(st: &ItemsMenu, p: &Progress, filter: u8) -> Vec<Stack> {
    let Some(exe) = st.exe.as_ref() else { return Vec::new() };
    stock(&st.order, exe.filter_word(filter), exe, &|id| p.item_count(id), &|_| false)
}

/// Sora's status as the use check and the effect read it.
fn target_of(s: &crate::sora::Sora, max_mp: i32, magic: &crate::magic::MagicState, drive: Option<&crate::drive::DriveState>, p: &Progress) -> Target {
    let m = p.sora();
    Target {
        vitals: Vitals {
            hp: s.hp,
            hp_max: s.stats.max_hp,
            hp_min: 0,
            mp: s.mp,
            mp_max: max_mp,
            charge_left: magic.charge.left,
            charge_total: magic.charge.total,
            drive_fraction: drive.map_or(0, |d| d.points.fraction),
            drive_points: drive.map_or(0, |d| d.points.whole),
            drive_max: drive.map_or(0, |d| d.points.max),
        },
        record: Some([m.ap, m.strength, m.magic, m.defense]),
    }
}

/// The party as the target page tests it: one `Target` per member, Sora first.
struct Snapshot {
    targets: Vec<Target>,
}

impl Snapshot {
    /// `FUN_001a15a0` has no test for a down member: HP 0 is below the maximum, so a heal is offered and the member's HP
    /// method revives him (`FUN_001a0140` -> vtable +0x78 of the party class at 0x35cc18 = `FUN_00176c98` ->
    /// `FUN_00186238` -> `FUN_0017cee8`, WP-PL2). The Drive Recovery pair (0x112, 0x113) reaches `FUN_001a0290` on the gauge
    /// bytes with no HP method and no KO test either; it has the single "Sora" row, who is never down in the menu.
    fn usable_on(&self, id: u16, row: usize) -> bool {
        self.targets.get(row).is_some_and(|t| can_use(id, t))
    }
}

impl Game for Snapshot {
    fn members(&self) -> usize {
        self.targets.len().max(1)
    }
    /// `FUN_0025e9e8(row)` -> `FUN_001a14c8(item, member of the row)`: a group item ("All", row 0) is usable when any
    /// member can use it, else the row's member decides (`FUN_002829f0`: the party object's row -> member id).
    fn would_help(&self, id: u16, row: usize) -> bool {
        if group_item(id) {
            (0..self.targets.len()).any(|k| self.usable_on(id, k))
        } else {
            self.usable_on(id, row)
        }
    }
}

/// A field member's status as the use check and the effect read it (`FUN_001a15a0`'s status block: HP and MP of the
/// field object, the record bytes of the party record). Drive points do not exist for a member (`drive_max` 0).
fn member_target(m: &crate::party::PartyMember, p: &Progress) -> Target {
    let r = p.member(m.neo).unwrap_or_default();
    Target {
        vitals: Vitals {
            hp: m.hp,
            hp_max: m.hp_max,
            hp_min: 0,
            mp: m.mp,
            mp_max: m.mp_max,
            charge_left: m.charge.left,
            charge_total: m.charge.total,
            ..Default::default()
        },
        record: Some([r.ap, r.strength, r.magic, r.defense]),
    }
}

/// What using an item on a field member does (`FUN_001a11a0` -> `FUN_001a12a0` on the member's status block): the
/// boosts raise his record byte, everything else is `items::item_effect` on his live HP, MP and MP charge. A KO'd
/// member's HP gain goes through the HP method (`FUN_00186238`, `battle::hp_change`), so a heal revives a KO'd member
/// (`FUN_0017cee8`: the KO bit cleared, the amount scaled by `revive`, `sstm` +0x54); WP-PL2. Answers a log note.
fn use_on_member(p: &mut Progress, m: &mut crate::party::PartyMember, item: u16, amount: u16, feed: f32, revive: f32) -> String {
    if (0x114..=0x117).contains(&item) {
        let added = p.apply_boost(m.neo, item, amount.min(255) as u8);
        return format!("boost +{added}");
    }
    let mut v = Vitals { hp: m.hp, hp_max: m.hp_max, hp_min: 0, mp: m.mp, mp_max: m.mp_max, charge_left: m.charge.left, charge_total: m.charge.total, ..Default::default() };
    let (hp0, mp0) = (v.hp, v.mp);
    item_effect(item, amount, 100, &mut v, feed);
    let revived = m.ko && v.hp > hp0;
    crate::party_battle::hp_change(&mut m.hp, m.hp_max, 0, &mut m.ko, v.hp - hp0, revive);
    m.mp = v.mp;
    m.charge.left = v.charge_left;
    m.charge.total = v.charge_total;
    format!("HP {hp0} -> {}, MP {mp0} -> {}{}", m.hp, m.mp, if revived { ", revived (FUN_0017cee8)" } else { "" })
}

/// What the keys of this frame mean to the open page.
fn keys_of(keys: &ButtonInput<KeyCode>, held: &mut (i8, f32), dt: f32) -> Vec<Key> {
    let mut out = Vec::new();
    let any = |c: &[KeyCode]| c.iter().any(|&k| keys.just_pressed(k));
    let dir = if keys.pressed(KeyCode::ArrowUp) { -1 } else if keys.pressed(KeyCode::ArrowDown) { 1 } else { 0 };
    let fresh = keys.just_pressed(KeyCode::ArrowUp) || keys.just_pressed(KeyCode::ArrowDown);
    if dir == 0 {
        *held = (0, 0.0);
    } else if fresh || held.0 != dir {
        *held = (dir, 0.0);
        out.push(if dir < 0 { Key::Up { fresh: true } } else { Key::Down { fresh: true } });
    } else {
        // The repeat word: after 24 ticks, every 6 (the pad layer's rate was not read; as page 0's).
        let (before, after) = (held.1, held.1 + dt);
        *held = (dir, after);
        let n = |t: f32| ((t - 0.4) / 0.1).floor();
        if after >= 0.4 && n(after) != n(before.max(0.4 - 1e-6)) {
            out.push(if dir < 0 { Key::Up { fresh: false } } else { Key::Down { fresh: false } });
        }
    }
    if any(&[KeyCode::ArrowLeft]) {
        out.push(Key::Left);
    }
    if any(&[KeyCode::ArrowRight]) {
        out.push(Key::Right);
    }
    if any(&[KeyCode::KeyJ]) {
        out.push(Key::Confirm);
    }
    if any(&[KeyCode::Escape, KeyCode::Backspace, KeyCode::KeyK]) {
        out.push(Key::Cancel);
    }
    if any(&[KeyCode::Enter, KeyCode::NumpadEnter]) {
        out.push(Key::Close);
    }
    if any(&[KeyCode::BracketLeft]) {
        out.push(Key::Prev);
    }
    if any(&[KeyCode::BracketRight]) {
        out.push(Key::Next);
    }
    if any(&[KeyCode::KeyX]) {
        out.push(Key::Switch);
    }
    out
}

type Vitality<'w> = (
    Option<ResMut<'w, crate::magic::MagicState>>,
    Option<ResMut<'w, crate::drive::DriveState>>,
    Option<Res<'w, crate::data::GameData>>,
    Option<Res<'w, crate::items::ItemsData>>,
    Option<Res<'w, crate::party::PartyData>>,
);

#[allow(clippy::too_many_arguments)]
pub(super) fn input(
    mut st: ResMut<ItemsMenu>,
    mut menu: ResMut<Menu>,
    mut scene: ResMut<MenuScene>,
    mut keys: ResMut<ButtonInput<KeyCode>>,
    mut se: MessageWriter<PlaySe>,
    mut progress: Option<ResMut<Progress>>,
    mut sora: Query<&mut crate::sora::Sora>,
    mut party: Query<&mut crate::party::PartyMember>,
    (mut magic, mut drive, game, idata, pdata): Vitality,
    real: Res<Time<Real>>,
    mut tags: ResMut<NewTags>,
    mut pics: ResMut<Pictures>,
) {
    if !st.ready {
        return;
    }
    // The menu left for another reason (closed): forget the pages.
    if st.machine.is_some() && matches!(menu.phase, Phase::Closing | Phase::Closed) {
        st.machine = None;
        st.hidden.clear();
        st.held = (0, 0.0);
        st.suspended = false;
        return;
    }
    // The Help overlay has the keys.
    if menu.page == super::help::PAGE {
        return;
    }
    // The equipment pages have the keys and the screen.
    if st.suspended {
        return;
    }
    if st.machine.is_none() {
        // Page 0's confirm on the Items row opens page 1 (`FUN_00251848`'s jump table, entry 0); the Abilities pages'
        // -5 asks for the same page (or, from page 0x10, for page 2).
        let on_row = menu.page == 0 && menu.current_entry() == Some(ENTRY);
        let asked = match menu.switch {
            Some(Switch::ToItems { second }) => Some(second),
            _ => None,
        };
        if asked.is_none() && !(on_row && keys.just_pressed(KeyCode::KeyJ)) {
            return;
        }
        menu.switch = None;
        let Some(p) = progress.as_deref() else {
            crate::menu::menu_sound::play(&mut se, snd::REFUSED);
            keys.reset_all();
            return;
        };
        let stock = stock_of(&st, p, 0);
        // The party object's rows (`FUN_00282968`): Sora, then the field members by party slot.
        let mut field: Vec<(u8, u16)> = party.iter().map(|m| (m.slot, m.neo)).collect();
        field.sort();
        st.party = std::iter::once(crate::progress::SORA).chain(field.into_iter().take(2).map(|f| f.1)).collect();
        st.machine = Some(Machine::new(st.party.len(), stock));
        st.held = (0, 0.0);
        enter(&mut st, &mut menu, &mut scene);
        if asked.is_none() {
            crate::menu::menu_sound::play(&mut se, snd::CONFIRM);
        }
        keys.reset_all();
        if asked == Some(true) {
            // Page 2 directly (`FUN_0026xxxx`'s -5 from page 0x10): the equipment module takes over this frame.
            st.suspended = true;
            st.equipment_requested = true;
            if let Some(ui) = &st.ui {
                for &i in &ui.all {
                    scene.nodes[i].visible = false;
                }
            }
            return;
        }
        redraw(&mut st, &mut scene, progress.as_deref(), &mut tags, &mut pics);
        return;
    }
    let mut held = st.held;
    let pressed = keys_of(&keys, &mut held, real.delta_secs());
    st.held = held;
    let mut close = false;
    for key in pressed {
        let (Some(p), Ok(s)) = (progress.as_deref(), sora.single()) else { break };
        let max_mp = game.as_ref().map_or(100, |g| g.sora_max_mp);
        let mut field: Vec<&crate::party::PartyMember> = party.iter().collect();
        field.sort_by_key(|m| m.slot);
        let mut targets = vec![match magic.as_deref() {
            Some(m) => target_of(s, max_mp, m, drive.as_deref(), p),
            None => Target::default(),
        }];
        targets.extend(field.iter().take(2).map(|m| member_target(m, p)));
        let snap = Snapshot { targets };
        let (rows, exe) = (st.rows.clone(), st.exe.clone().unwrap_or_default());
        let fx = st.machine.as_mut().map(|m| m.press(key, &rows, &exe, &snap)).unwrap_or_default();
        for e in fx {
            match e {
                Effect::Sound(n) => {
                    crate::menu::menu_sound::play(&mut se, n);
                }
                Effect::Page(page) => {
                    menu.page = page;
                    // The Stock page names the highlighted item; the others only their id.
                    let extra = match st.machine.as_ref() {
                        Some(m) if page == STOCK => m.stock.get(m.cursor).map(|s| format!(" cursor {} {} x{}", m.cursor, item_name(&st, &scene, s.id), s.count)).unwrap_or_else(|| " (empty)".into()),
                        _ => String::new(),
                    };
                    info!("menu: page {page:#x}{extra}");
                }
                Effect::Filter(f) => {
                    if let Some(p) = progress.as_deref() {
                        let stock = stock_of(&st, p, f);
                        let n = stock.len();
                        if let Some(m) = st.machine.as_mut() {
                            m.reload(stock, true);
                        }
                        info!("menu: stock filter {f}: {n} items");
                    }
                }
                Effect::Use { item, row } => {
                    if let (Some(p), Ok(mut s)) = (progress.as_deref_mut(), sora.single_mut())
                        && let (Some(m), Some(d)) = (magic.as_deref_mut(), game.as_deref())
                    {
                        let feed = idata.as_deref().map_or(0.0, |i| i.charge_feed);
                        let name = item_name(&st, &scene, item);
                        let mut field: Vec<_> = party.iter_mut().collect();
                        field.sort_by_key(|m| m.slot);
                        field.truncate(2);
                        use_item(&st, p, &mut s, m, drive.as_deref_mut(), d.sora_max_mp, (feed, pdata.as_deref().map_or(1.0, |d| d.revive_scale())), (item, &name), row, &mut field);
                        let stock = stock_of(&st, p, st.machine.as_ref().map_or(0, |m| m.filter));
                        if let Some(m) = st.machine.as_mut() {
                            m.reload(stock, false);
                        }
                    }
                }
                Effect::Discard { item } => {
                    if let Some(p) = progress.as_deref_mut() {
                        let name = item_name(&st, &scene, item);
                        let before = p.item_count(item);
                        p.take_item(item, crate::progress::INVENTORY);
                        info!("menu: item thrown away: {name} (id {item}), stock {before} -> {}", p.item_count(item));
                        let stock = stock_of(&st, p, st.machine.as_ref().map_or(0, |m| m.filter));
                        if let Some(m) = st.machine.as_mut() {
                            m.reload(stock, false);
                        }
                    }
                }
                Effect::Member(row) => {
                    st.member = row;
                    // Page 2: the equipment module takes over this frame (it runs after this system); the
                    // screen of page 1 goes.
                    st.suspended = true;
                    st.equipment_requested = true;
                    if let Some(ui) = &st.ui {
                        for &i in &ui.all {
                            scene.nodes[i].visible = false;
                        }
                    }
                }
                Effect::Leave => leave(&mut st, &mut menu, &mut scene),
                Effect::Switch => {
                    // -5 needs the Abilities row on page 0 (the list mask's 0x100 is set only then).
                    if menu.rows_has(1) {
                        hand_over(&mut st, &mut menu, &mut scene);
                        menu.switch = Some(Switch::ToAbilities { second: false });
                        info!("menu: switch to Abilities");
                    }
                }
                Effect::Close => {
                    // Back to page 0, then the menu's own input closes it from there (the key is left
                    // unconsumed).
                    leave(&mut st, &mut menu, &mut scene);
                    close = true;
                }
            }
            if st.machine.is_none() || st.suspended {
                break;
            }
        }
        if st.machine.is_none() || st.suspended {
            break;
        }
    }
    if close {
        return;
    }
    if st.machine.is_some() && !st.suspended {
        redraw(&mut st, &mut scene, progress.as_deref(), &mut tags, &mut pics);
    }
    keys.reset_all();
}

fn item_name(st: &ItemsMenu, scene: &MenuScene, id: u16) -> String {
    st.rows.get(&id).and_then(|r| scene.text.as_ref()?.messages.as_ref()?.text(u32::from(r.name))).unwrap_or_else(|| format!("item {id}"))
}

/// `FUN_0025ea40`: the item's effect on the member (`FUN_001a11a0(item, member, 0, 100)`: no Item Boost in the
/// menu), then one comes off the stock (`FUN_001a1ba8(item, 100)`). The boosts (0x114 .. 0x117) raise the
/// record byte (`FUN_001a0ce0` .. `0dd0`) by the item's amount; everything else is `items::item_effect`. A group
/// item ("All") reaches every member (`FUN_001a11a0`'s loop over the four party slots); `members` are the field
/// members after Sora in row order. The menu's use never counts "Healed Party Members": `FUN_0025ea40` drops
/// `FUN_001a11a0`'s answer and `FUN_001d76a0` has no caller here (only `FUN_001b8658`, the field command's finish, calls it).
#[allow(clippy::too_many_arguments)]
fn use_item(
    st: &ItemsMenu,
    p: &mut Progress,
    s: &mut crate::sora::Sora,
    magic: &mut crate::magic::MagicState,
    mut drive: Option<&mut crate::drive::DriveState>,
    max_mp: i32,
    (feed, revive): (f32, f32),
    (item, name): (u16, &str),
    row: usize,
    members: &mut [Mut<crate::party::PartyMember>],
) {
    let Some(r) = st.rows.get(&item).copied() else { return };
    let before = p.item_count(item);
    let rows: Vec<usize> = if group_item(item) { (0..=members.len()).collect() } else { vec![row] };
    let mut sora_line = None;
    for &k in &rows {
        if k == 0 {
            let mut v = target_of(s, max_mp, magic, drive.as_deref(), p).vitals;
            let (hp0, mp0) = (v.hp, v.mp);
            let mut note = String::new();
            if (0x114..=0x117).contains(&item) {
                let m0 = p.sora();
                let was = [m0.ap, m0.strength, m0.magic, m0.defense];
                let added = p.apply_boost(crate::progress::SORA, item, r.amount.min(255) as u8);
                let m1 = p.sora();
                let now = [m1.ap, m1.strength, m1.magic, m1.defense];
                let b = match item {
                    0x114 => 1,
                    0x115 => 2,
                    0x116 => 3,
                    _ => 0,
                };
                note = format!(", boost +{added} (record byte {} -> {})", was[b], now[b]);
            } else {
                item_effect(item, r.amount, 100, &mut v, feed);
                s.hp = v.hp;
                s.mp = v.mp;
                magic.charge.left = v.charge_left;
                magic.charge.total = v.charge_total;
                if let Some(d) = drive.as_deref_mut() {
                    d.points.fraction = v.drive_fraction;
                    d.points.whole = v.drive_points;
                }
            }
            sora_line = Some((hp0, mp0, note));
        } else if let Some(m) = members.get_mut(k - 1) {
            let note = use_on_member(p, m, item, r.amount, feed, revive);
            info!("menu: item used: {name} (id {item}, row {k}) on member {} ({}): {note}", m.neo, m.model);
        }
    }
    p.take_item(item, crate::progress::INVENTORY);
    if let Some((hp0, mp0, note)) = sora_line {
        info!("menu: item used: {name} (id {item}, row {row}) on Sora: HP {hp0} -> {}, MP {mp0} -> {}{note}, stock {before} -> {}", s.hp, s.mp, p.item_count(item));
    }
}

// --- drawing -------------------------------------------------------------------------------

pub(super) fn node(scene: &mut MenuScene, seq: Option<usize>) -> usize {
    let mut n = Node::new(seq);
    n.visible = false;
    scene.add(n)
}

fn ensure_ui(st: &mut ItemsMenu, scene: &mut MenuScene) {
    if st.ui.is_some() {
        return;
    }
    const S: Option<usize> = Some(1);
    let mut all = Vec::new();
    let mut mk = |scene: &mut MenuScene, seq: Option<usize>| {
        let id = node(scene, seq);
        all.push(id);
        id
    };
    let tabs = [mk(scene, S), mk(scene, S), mk(scene, S)];
    let rows: Vec<usize> = (0..4).map(|_| mk(scene, S)).collect();
    let window = mk(scene, S);
    let preview: Vec<usize> = (0..9).map(|_| mk(scene, None)).collect();
    let panel = mk(scene, S);
    let cells: Vec<usize> = (0..COLUMNS * ROWS).map(|_| mk(scene, S)).collect();
    let counts: Vec<usize> = (0..COLUMNS * ROWS).map(|_| mk(scene, None)).collect();
    let cell_tags: Vec<usize> = (0..COLUMNS * ROWS).map(|_| mk(scene, S)).collect();
    let stock_tag = mk(scene, S);
    let item_window = mk(scene, S);
    let prompt_panel = mk(scene, S);
    let prompt_title = mk(scene, S);
    let prompt_rows: Vec<usize> = (0..2).map(|_| mk(scene, S)).collect();
    let target_panel = mk(scene, S);
    let target_title = mk(scene, S);
    let target_rows: Vec<usize> = (0..4).map(|_| mk(scene, S)).collect();
    let info = mk(scene, S);
    let info_text = mk(scene, None);
    let info_rows: Vec<usize> = (0..2).map(|_| mk(scene, S)).collect();
    let empty = mk(scene, S);
    // The hand and the glint draw over everything else (later nodes draw on top).
    let hand = mk(scene, S);
    let glint = mk(scene, S);
    let help = mk(scene, None);
    for ((&c, &n), &t) in cells.iter().zip(&counts).zip(&cell_tags) {
        scene.nodes[c].parent = Some(panel);
        scene.nodes[n].parent = Some(c);
        scene.nodes[t].parent = Some(c);
    }
    scene.nodes[stock_tag].parent = Some(rows[1]);
    scene.nodes[prompt_title].parent = Some(prompt_panel);
    for &r in &prompt_rows {
        scene.nodes[r].parent = Some(prompt_panel);
    }
    scene.nodes[target_title].parent = Some(target_panel);
    for &r in &target_rows {
        scene.nodes[r].parent = Some(target_panel);
    }
    scene.nodes[info_text].parent = Some(info);
    for &r in &info_rows {
        scene.nodes[r].parent = Some(info);
    }
    for &r in &preview {
        scene.nodes[r].parent = Some(window);
    }
    st.ui = Some(Ui {
        tabs,
        rows,
        hand,
        glint,
        window,
        preview,
        panel,
        cells,
        counts,
        cell_tags,
        stock_tag,
        prompt_panel,
        prompt_title,
        prompt_rows,
        item_window,
        target_panel,
        target_title,
        target_rows,
        info,
        info_text,
        info_rows,
        empty,
        help,
        all,
    });
}

/// Gives the screen to another module (the Abilities pages): this module's nodes go, page 0's stay hidden and are
/// handed on through `Menu::carry`.
pub(super) fn hand_over(st: &mut ItemsMenu, menu: &mut Menu, scene: &mut MenuScene) {
    if let Some(ui) = &st.ui {
        for &i in &ui.all {
            scene.nodes[i].visible = false;
        }
    }
    menu.carry = std::mem::take(&mut st.hidden);
    st.machine = None;
    st.suspended = false;
    menu.page = 0;
}

/// Takes over from page 0: hides its rows, hand, help line and cards.
fn enter(st: &mut ItemsMenu, menu: &mut Menu, scene: &mut MenuScene) {
    ensure_ui(st, scene);
    st.hidden = if menu.carry.is_empty() { menu.page0_nodes().into_iter().filter(|&i| scene.nodes[i].visible).collect() } else { std::mem::take(&mut menu.carry) };
    for &i in &st.hidden {
        scene.nodes[i].visible = false;
    }
    menu.page = MEMBERS;
    info!("menu: page {MEMBERS:#x}");
}

/// Back to page 0: this module's nodes go, page 0's come back.
pub(super) fn leave(st: &mut ItemsMenu, menu: &mut Menu, scene: &mut MenuScene) {
    if let Some(ui) = &st.ui {
        for &i in &ui.all {
            scene.nodes[i].visible = false;
        }
    }
    for &i in &st.hidden {
        scene.nodes[i].visible = true;
    }
    st.hidden.clear();
    st.machine = None;
    menu.page = 0;
    info!("menu: page 0");
}

/// A message's tokens without its end command, so runs can be joined (the layout stops at an end command).
pub(super) fn tokens_of(text: &MenuText, id: u32) -> Vec<Token> {
    text.messages.as_ref().and_then(|m| m.tokens(id)).unwrap_or_default().into_iter().filter(|t| *t != Token::End).collect()
}

/// A party member's name as the pages print it: Sora's stays the literal the pages always used; Donald and Goofy take
/// their message from the name table at 0x37ba90 (`names`, by member id) and a literal when the table has no text.
pub(super) fn member_tokens(text: &MenuText, names: &[u32], id: u16) -> Vec<Token> {
    if id != crate::progress::SORA
        && let Some(&msg) = names.get(usize::from(id))
    {
        let t = tokens_of(text, msg);
        if !t.is_empty() {
            return t;
        }
    }
    let literal = match id {
        crate::progress::DONALD => "Donald",
        crate::progress::GOOFY => "Goofy",
        _ => "Sora",
    };
    text.literal(literal, Style::default(), [0.0; 2]).tokens
}

pub(super) fn label(text: &MenuText, tokens: Vec<Token>, font: usize, size: f32, color: [u8; 4], align: u8, offset: [f32; 2]) -> Label {
    Label { tokens, style: Style { font, size: Some(size), color, align, world: text.world, ..Style::default() }, offset }
}

/// `FUN_00191a30`: a message whose variables (token 0x0d) are replaced in order by the given token runs.
pub fn fill(tokens: Vec<Token>, args: &[Vec<Token>]) -> Vec<Token> {
    let mut out = Vec::new();
    let mut next = args.iter();
    for t in tokens {
        match t {
            // A substituted run ends with its own end command, which would stop the whole message.
            Token::Variable => out.extend(next.next().into_iter().flatten().filter(|t| **t != Token::End).cloned()),
            other => out.push(other),
        }
    }
    out
}

/// The tokens of the count beside a cell.
fn count_tokens(text: &MenuText, n: u8) -> Option<Vec<Token>> {
    Some(text.literal(&count_text(n)?, Style::default(), [0.0; 2]).tokens)
}

/// One stock cell's text: the item's icon and name (`FUN_00255520`).
fn item_tokens(text: &MenuText, r: &ItemRow) -> Vec<Token> {
    let mut v = vec![Token::Icon(r.icon)];
    v.extend(tokens_of(text, u32::from(r.name)));
    v
}

/// The count text beside a cell (`FUN_00284088`: nothing for one, else the format at 0x382408, `*%d`). The game's
/// glyph table has no asterisk in the font's character set, and its multiplication sign (glyph 13) is what the
/// font draws for it [inferred: the format's ASCII `*` goes through the text conversion of `FUN_00191710`].
pub fn count_text(n: u8) -> Option<String> {
    (n != 1).then(|| format!("\u{d7}{n}"))
}

pub(super) fn set_help(scene: &mut MenuScene, text: &MenuText, help: usize, id: Option<u32>) {
    let n = &mut scene.nodes[help];
    n.pos = [0.0, 0.0];
    n.label = id.and_then(|id| text.message(id, Style { font: 0, size: Some(15.0), color: at::HELP_COLOR, ..Style::default() }, at::HELP));
    n.visible = n.label.is_some();
}

/// The pitch of the Stock cells: the idle cell group's `unk20` across and `unk1c` down (`FUN_0025b2a0` places
/// cell `i` at `(i % 2 * FUN_00237698, i / 2 * FUN_00237678)`).
fn cell_pitch(scene: &MenuScene) -> (f32, f32) {
    scene.seqs.get(1).and_then(|s| s.groups.get(group::CELL)).map_or((200.0, 26.0), |g| (g.unk20 as f32, g.unk1c as f32))
}

/// Sets what every node shows from the machine's state.
fn redraw(st: &mut ItemsMenu, scene: &mut MenuScene, progress: Option<&Progress>, tags: &mut NewTags, pics: &mut Pictures) {
    let Some(m) = st.machine.clone() else { return };
    let Some(ui_ref) = st.ui.as_ref() else { return };
    let (tabs, rows, hand, glint, window, preview, panel, cells, counts, help) = (
        ui_ref.tabs,
        ui_ref.rows.clone(),
        ui_ref.hand,
        ui_ref.glint,
        ui_ref.window,
        ui_ref.preview.clone(),
        ui_ref.panel,
        ui_ref.cells.clone(),
        ui_ref.counts.clone(),
        ui_ref.help,
    );
    let (cell_tags, stock_tag) = (ui_ref.cell_tags.clone(), ui_ref.stock_tag);
    let empty = ui_ref.empty;
    let (prompt_panel, prompt_title, prompt_rows, item_window, target_panel, target_title, target_nodes, info, info_text, info_rows) = (
        ui_ref.prompt_panel,
        ui_ref.prompt_title,
        ui_ref.prompt_rows.clone(),
        ui_ref.item_window,
        ui_ref.target_panel,
        ui_ref.target_title,
        ui_ref.target_rows.clone(),
        ui_ref.info,
        ui_ref.info_text,
        ui_ref.info_rows.clone(),
    );
    let all = ui_ref.all.clone();
    let entry = st.entry.clone().unwrap_or_else(|| Entry { idle: [0; 4], selected: [0; 4], name: 0, help: 0, need: vec![], deny: vec![] });
    let exe = st.exe.clone().unwrap_or_default();
    let (party, names) = (st.party.clone(), st.member_names.clone());
    let member_id = |k: usize| party.get(k).copied().unwrap_or(crate::progress::SORA);
    let Some(text) = scene.text.take() else { return };
    let page = m.page;
    let was: Vec<bool> = all.iter().map(|&i| scene.nodes[i].visible).collect();
    for &i in &all {
        scene.nodes[i].visible = false;
    }
    let set = |scene: &mut MenuScene, id: usize, group: usize, next: Option<usize>, was_vis: bool| {
        let n = &mut scene.nodes[id];
        n.visible = true;
        let cur = n.player.group;
        if !was_vis || !(cur == Some(group) || (next.is_some() && cur == next)) {
            match next {
                Some(nx) => scene.set_group_then(id, group, nx),
                None => scene.set_group(id, group),
            }
        }
    };
    let was_of = |id: usize| all.iter().position(|&a| a == id).map(|k| was[k]).unwrap_or(false);

    // The header tabs: "Items" alone on page 1 (clip 6 at 10.2 s), then "Items" and "Stock" on the Stock and its
    // dialogs [inferred: the second tab as the Abilities pages have it on their second level].
    let on_stock_row = page != MEMBERS || m.cursor1 == m.members;
    let tab_count = if page == MEMBERS { 1 } else { 2 };
    for (k, &t) in tabs.iter().enumerate().take(tab_count) {
        let prev = was_of(t);
        scene.nodes[t].pos = [at::TAB_PITCH * k as f32, 0.0];
        set(scene, t, group::TAB[0], Some(group::TAB[2]), prev);
        let tokens = match k {
            0 => tokens_of(&text, entry.name),
            _ if on_stock_row => tokens_of(&text, STOCK_LABEL),
            _ => text.literal("Sora", Style::default(), [0.0, 0.0]).tokens,
        };
        scene.nodes[t].label_anchor = Some(at::TAB_LABEL);
        scene.nodes[t].label = Some(label(&text, tokens, 1, 15.0, at::COLOR, 0, [0.0, 0.0]));
    }

    let item_help = |id: u16, st: &ItemsMenu| st.rows.get(&id).map(|r| u32::from(r.description));
    match page {
        MEMBERS => {
            for (k, &r) in rows.iter().enumerate().take(m.members + 1) {
                let selected = k == m.cursor1;
                let prev = was_of(r);
                scene.nodes[r].off = [0.0, at::ROW_PITCH * k as f32];
                let g = if selected { entry.selected } else { entry.idle };
                set(scene, r, usize::from(g[0]), Some(usize::from(g[2])), prev);
                scene.nodes[r].label_anchor = Some([-7.0, 1.5]);
                let tokens = if k == m.members { tokens_of(&text, STOCK_LABEL) } else { member_tokens(&text, &names, member_id(k)) };
                scene.nodes[r].label = Some(label(&text, tokens, 1, 15.0, if selected { at::SELECTED } else { [0x9f, 0x9f, 0xaf, 0x80] }, 0, [0.0, 0.0]));
            }
            let sel = rows[m.cursor1.min(rows.len() - 1)];
            // The Stock row is the one after the members.
            scene.nodes[stock_tag].parent = Some(rows[m.members.min(rows.len() - 1)]);
            scene.nodes[hand].parent = Some(sel);
            scene.nodes[hand].off = at::HAND;
            set(scene, hand, group::HAND, None, was_of(hand));
            scene.nodes[glint].parent = Some(sel);
            scene.nodes[glint].off = at::GLINT;
            set(scene, glint, group::GLINT[0], Some(group::GLINT[2]), was_of(glint));
            // The preview of the highlighted member: his equipment (`FUN_00269028`) [inferred layout].
            set(scene, window, group::WINDOW, None, was_of(window));
            if m.cursor1 < m.members
                && let Some(p) = progress
            {
                // One line per slot with its category: the weapon (type 2), the armor slots (3), the accessory
                // slots (4) and the battle item slots (0); an empty slot shows its category's icon and `---`
                // (message 0x448), as clip 6 at 10.2 s does.
                let s = p.member(member_id(m.cursor1)).unwrap_or_default();
                let mut lines: Vec<(Option<u8>, Option<u16>)> = vec![(Some(2), Some(s.weapons[0]).filter(|&w| w != 0))];
                // The armor and accessory item types are not among the ones with an icon in the table at 0x3809d0
                // (types 5 to 13 share the unlisted icon): their empty rows show no icon [GAP: the footage's
                // category icons come from the preview builder, which was not read].
                lines.extend(s.armor.iter().map(|&a| (None, Some(a).filter(|&a| a != 0))));
                lines.extend(s.accessories.iter().map(|&a| (None, Some(a).filter(|&a| a != 0))));
                lines.extend(s.items.iter().map(|&a| (Some(0), Some(a).filter(|&a| a != 0))));
                for (k, &(kind, line)) in lines.iter().take(preview.len()).enumerate() {
                    let id = preview[k];
                    scene.nodes[id].visible = true;
                    scene.nodes[id].off = [0.0, at::ROW_PITCH * k as f32];
                    scene.nodes[id].label_anchor = None;
                    let tokens = match line.and_then(|i| st.rows.get(&i)) {
                        Some(r) => item_tokens(&text, r),
                        None => {
                            let mut t: Vec<Token> = kind.map(|k| Token::Icon(exe.icon(k, false))).into_iter().collect();
                            t.extend(tokens_of(&text, 0x448));
                            t
                        }
                    };
                    scene.nodes[id].label = Some(label(&text, tokens, 1, 15.0, at::COLOR, 0, at::LABEL));
                }
            }
            // `FUN_0025aac0`: the Stock row carries the tag when any listed item is fresh (the set of page 0's Items row).
            if let Some(g) = tags.row_group(new_tags::Set::Items) {
                scene.nodes[stock_tag].off = scene.extent(1, usize::from(entry.idle[2]));
                set(scene, stock_tag, g, None, was_of(stock_tag));
            }
            // `FUN_0025ddc0`: the member's help, or the Stock's (messages 0xbb86, 0xbb87; bit 15 ignored).
            set_help(scene, &text, help, Some(if m.cursor1 < m.members { 0x3b86 } else { 0x3b87 }));
        }
        STOCK => {
            set(scene, panel, group::STOCK_PANEL[0], Some(group::STOCK_PANEL[1]), was_of(panel));
            if m.stock.is_empty() {
                // `FUN_0025dae8`: the first cell takes group 0x12e and the message 0x845a; the group's anchor
                // (216, 115) is where the text starts.
                set(scene, empty, group::CELL_EMPTY, None, was_of(empty));
                scene.nodes[empty].label_anchor = Some(at::LABEL);
                scene.nodes[empty].label = Some(label(&text, tokens_of(&text, NO_ITEMS), 1, 15.0, at::COLOR, 0, [0.0, 0.0]));
            }
            let (dx, dy) = cell_pitch(scene);
            for slot in 0..COLUMNS * ROWS {
                let idx = m.scroll * COLUMNS + slot;
                let Some(s) = m.stock.get(idx) else { break };
                let c = cells[slot];
                let selected = idx == m.cursor;
                scene.nodes[c].off = [dx * (slot % COLUMNS) as f32, dy * (slot / COLUMNS) as f32];
                set(scene, c, if selected { group::CELL_SELECTED } else { group::CELL }, None, false);
                scene.nodes[c].label_anchor = None;
                scene.nodes[c].label = st.rows.get(&s.id).map(|r| label(&text, item_tokens(&text, r), 1, 15.0, if selected { at::SELECTED } else { at::COLOR }, 0, at::LABEL));
                // `FUN_0025dae8`: a fresh item's cell has the tag; `FUN_0025dcf8`: the cursor reaching it sets its
                // seen bit, which turns the tag to the viewed form.
                if selected {
                    tags.view(s.id);
                }
                if let Some(g) = tags.cell_group(s.id) {
                    scene.nodes[cell_tags[slot]].off = scene.extent(1, group::CELL);
                    set(scene, cell_tags[slot], g, None, false);
                }
                if let Some(tokens) = count_tokens(&text, s.count) {
                    let id = counts[slot];
                    scene.nodes[id].visible = true;
                    scene.nodes[id].label = Some(label(&text, tokens, 1, 15.0, if selected { at::SELECTED } else { at::COLOR }, 1, [at::COUNT_RIGHT, 1.0]));
                }
            }
            if let Some(s) = m.stock.get(m.cursor) {
                scene.nodes[hand].parent = Some(cells[m.cursor - m.scroll * COLUMNS]);
                scene.nodes[hand].off = at::HAND;
                set(scene, hand, group::HAND, None, false);
                set_help(scene, &text, help, item_help(s.id, st));
            } else {
                set_help(scene, &text, help, None);
            }
        }
        PROMPT | DISCARD | TARGET | NOTICE => {
            let item = m.item;
            let id = item.map_or(0, |r| r.id);
            // The prompt panel and the item window stay up on every one of these pages (`FUN_0025bad0` builds them,
            // the later pages add their own objects); the target panel hangs under the prompt panel.
            set(scene, prompt_panel, group::PROMPT_PANEL[0], Some(group::PROMPT_PANEL[1]), was_of(prompt_panel));
            set(scene, item_window, group::ITEM_WINDOW[0], Some(group::ITEM_WINDOW[1]), was_of(item_window));
            // `FUN_0025bad0` -> `FUN_0029ed50`: the item's picture in the window.
            match item.and_then(|r| st.pictures.get(&r.id)).copied().filter(|&p| p >= 0) {
                Some(p) => pics.show(item_window, p as u16, at::PICTURE),
                None => pics.clear(item_window),
            }
            // The title strip of the prompt panel is the item's own list text (icon, name, count: `FUN_0025bad0` points
            // the strip's text at the cell's buffer).
            set(scene, prompt_title, group::TITLE, None, was_of(prompt_title));
            let count = m.stock.iter().find(|s| s.id == id).map_or(0, |s| s.count);
            let mut title = item.map(|r| item_tokens(&text, &r)).unwrap_or_default();
            if let Some(c) = count_tokens(&text, count) {
                title.push(Token::Space);
                title.extend(c);
            }
            scene.nodes[prompt_title].label_anchor = Some(at::LABEL);
            scene.nodes[prompt_title].label = Some(label(&text, title, 1, 15.0, at::COLOR, 0, [0.0, 0.0]));
            let can = item.is_some_and(|r| r.can_discard());
            let pitch = scene.seqs.get(1).and_then(|s| s.groups.get(group::ROW)).map_or(28.0, |g| g.unk1c as f32);
            for (k, &r) in prompt_rows.iter().enumerate() {
                let Some(&row) = prompt_rows_of(m.usable).get(k) else { continue };
                scene.nodes[r].off = [0.0, pitch * k as f32];
                set(scene, r, group::ROW, None, was_of(r));
                scene.nodes[r].label_anchor = Some(at::LABEL);
                let selected = page == PROMPT && k == m.prompt;
                let color = if selected { at::SELECTED } else if row == 1 && !can { at::GREY } else { at::COLOR };
                scene.nodes[r].label = Some(label(&text, tokens_of(&text, exe.prompt[row]), 1, 15.0, color, 0, [0.0, 0.0]));
            }
            match page {
                PROMPT => {
                    scene.nodes[hand].parent = Some(prompt_rows[m.prompt.min(prompt_rows.len() - 1)]);
                    scene.nodes[hand].off = at::HAND;
                    set(scene, hand, group::HAND, None, false);
                }
                TARGET => {
                    set(scene, target_panel, group::TARGET_PANEL[0], Some(group::TARGET_PANEL[1]), was_of(target_panel));
                    set(scene, target_title, group::TITLE, None, was_of(target_title));
                    scene.nodes[target_title].label_anchor = Some(at::LABEL);
                    scene.nodes[target_title].label = Some(label(&text, tokens_of(&text, 0x3943), 1, 15.0, at::COLOR, 0, [0.0, 0.0]));
                    let (n, all_row) = target_rows(id, m.single, m.members);
                    for (k, &r) in target_nodes.iter().enumerate().take(n) {
                        scene.nodes[r].off = [0.0, pitch * k as f32];
                        set(scene, r, group::ROW, None, was_of(r));
                        scene.nodes[r].label_anchor = Some(at::LABEL);
                        let tokens = if all_row { tokens_of(&text, 0x3944) } else { member_tokens(&text, &names, member_id(k)) };
                        scene.nodes[r].label = Some(label(&text, tokens, 1, 15.0, if k == m.target { at::SELECTED } else { at::COLOR }, 0, [0.0, 0.0]));
                    }
                    scene.nodes[hand].parent = Some(target_nodes[m.target.min(n.saturating_sub(1))]);
                    scene.nodes[hand].off = at::HAND;
                    set(scene, hand, group::HAND, None, false);
                }
                NOTICE | DISCARD => {
                    // The information window (`FUN_00253398`): "{type} {name}" goes into the message (`FUN_00191a30`);
                    // the OK of the notice or the No and Yes of the question are its rows.
                    let ask = page == DISCARD;
                    let g = if ask { group::INFO_ASK } else { group::INFO_OK };
                    set(scene, info, g[0], Some(g[2]), was_of(info));
                    let type_tokens = item.and_then(|r| exe.type_name(r.kind)).map(|t| tokens_of(&text, t)).unwrap_or_default();
                    let name = item.map(|r| tokens_of(&text, u32::from(r.name))).unwrap_or_default();
                    let tokens = fill(tokens_of(&text, if ask { 0x394f } else { 0x3b90 }), &[type_tokens, name]);
                    let n = &mut scene.nodes[info_text];
                    n.visible = true;
                    n.label_anchor = Some(if ask { at::INFO_ASK_TEXT } else { at::INFO_OK_TEXT });
                    n.label = Some(label(&text, tokens, 1, 15.0, at::COLOR, 0, [0.0, 0.0]));
                    let rows: Vec<(u32, bool)> = if ask { vec![(exe.no, m.question == 0), (exe.yes, m.question == 1)] } else { vec![(exe.ok, true)] };
                    let [px, py] = if ask { at::INFO_ASK_ROWS } else { at::INFO_OK_ROWS };
                    for (k, &(msg, sel)) in rows.iter().enumerate() {
                        let r = info_rows[k];
                        set(scene, r, group::ROW, None, was_of(r));
                        scene.nodes[r].off = [px, py + at::INFO_ASK_PITCH * k as f32];
                        scene.nodes[r].label_anchor = Some(at::LABEL);
                        scene.nodes[r].label = Some(label(&text, tokens_of(&text, msg), 1, 15.0, if sel { at::SELECTED } else { at::COLOR }, 0, [0.0, 0.0]));
                        if sel {
                            scene.nodes[hand].parent = Some(r);
                            scene.nodes[hand].off = at::HAND;
                            set(scene, hand, group::HAND, None, false);
                        }
                    }
                }
                _ => {}
            }
            // `FUN_0025ddc0` on pages 0xb and 0xc: the item's description (`item +0xa`).
            let desc = if matches!(page, PROMPT | TARGET) { item.map(|r| u32::from(r.description)) } else { None };
            set_help(scene, &text, help, desc);
        }
        _ => {}
    }
    scene.text = Some(text);
}

fn prompt_rows_of(usable: bool) -> Vec<usize> {
    prompt_rows(usable)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(id: u16, kind: u8, sell: u16) -> ItemRow {
        ItemRow { id, kind, amount: 0, name: 0, description: 0, sell, icon: 0 }
    }

    /// The tables as the disc test reads them: types 0 and 1 are usable, 0x112 and 0x113 single target.
    fn exe() -> ExeTables {
        ExeTables {
            usable_types: vec![0, 1],
            single: vec![0x112, 0x113],
            prompt: [0x456, 0x457],
            filter_bits: vec![(1, 2), (2, 3), (3, 4), (7, 5), (5, 6), (9, 7), (10, 8), (6, 9), (13, 10), (11, 11), (8, 12), (12, 13)],
            hidden: vec![(0x10, 7)],
            type_names: vec![(0, 0x39c7), (2, 0x39c9)],
            ok: 1,
            yes: 2,
            no: 3,
            icons: vec![(0, 0, 12), (2, 4, 14)],
        }
    }

    fn rows() -> BTreeMap<u16, ItemRow> {
        [row(1, 0, 10), row(2, 0, 20), row(7, 0, 0), row(0x112, 0, 5), row(100, 2, 0), row(200, 16, 1)].into_iter().map(|r| (r.id, r)).collect()
    }

    struct G {
        members: usize,
        helps: bool,
    }
    impl Game for G {
        fn members(&self) -> usize {
            self.members
        }
        fn would_help(&self, _id: u16, _row: usize) -> bool {
            self.helps
        }
    }

    fn stacks(ids: &[(u16, u8)]) -> Vec<Stack> {
        ids.iter().map(|&(id, count)| Stack { id, count }).collect()
    }

    #[test]
    fn the_filter_words_are_the_ones_the_stubs_pass() {
        let e = exe();
        // Filter 0 (everything): types 0..=13, 14, 15, 16, 17 and 20; never spells (18), abilities (19), forms (21).
        let all = e.filter_word(0);
        for ty in [0u32, 1, 2, 7, 13, 14, 15, 16, 17, 20] {
            assert!(all >> ty & 1 != 0, "type {ty}");
        }
        for ty in [18u32, 19, 21] {
            assert!(all >> ty & 1 == 0, "type {ty} is never listed");
        }
        assert_eq!(e.filter_word(1), 3, "Items: types 0 and 1");
        assert_eq!(e.filter_word(2), 0x3ffc, "Equipment: types 2..=13");
        assert_eq!(e.filter_word(3), 0xc000, "types 14 and 15");
        assert_eq!(e.filter_word(4), 0x13_0000, "types 16, 17 and 20");
    }

    #[test]
    fn the_stock_lists_owned_items_in_table_order_by_filter() {
        let e = exe();
        let table = vec![row(1, 0, 10), row(100, 2, 0), row(2, 0, 20), row(7, 0, 0), row(200, 16, 1), row(300, 0, 1)];
        let own = |id: u16| match id {
            1 => 3,
            100 => 1,
            2 => 0,
            7 => 99,
            200 => 2,
            _ => 0,
        };
        let none = |_f: u16| false;
        let all = stock(&table, e.filter_word(0), &e, &own, &none);
        assert_eq!(all, stacks(&[(1, 3), (100, 1), (7, 99), (200, 2)]), "owned only, in table order");
        assert_eq!(stock(&table, e.filter_word(1), &e, &own, &none), stacks(&[(1, 3), (7, 99)]));
        assert_eq!(stock(&table, e.filter_word(2), &e, &own, &none), stacks(&[(100, 1)]));
        assert_eq!(stock(&table, e.filter_word(4), &e, &own, &none), stacks(&[(200, 2)]));
        // An item whose story flag is set is not listed (`FUN_002846f0`).
        let set = |f: u16| f == 0x10;
        assert_eq!(stock(&table, e.filter_word(0), &e, &own, &set), stacks(&[(1, 3), (100, 1), (200, 2)]));
    }

    #[test]
    fn use_follows_fun_001a15a0() {
        let base = Vitals { hp: 50, hp_max: 100, mp: 40, mp_max: 100, drive_points: 1, drive_max: 3, ..Default::default() };
        let t = |v: Vitals| Target { vitals: v, record: Some([0, 99, 100, 7]) };
        // HP items need HP below the maximum; MP items MP below it; the MP charge makes an MP item usable.
        assert!(can_use(1, &t(base)) && can_use(5, &t(base)));
        assert!(!can_use(1, &t(Vitals { hp: 100, ..base })));
        assert!(can_use(3, &t(base)) && !can_use(3, &t(Vitals { mp: 100, ..base })));
        assert!(can_use(3, &t(Vitals { mp: 100, charge_total: 300.0, ..base })), "while MP charges");
        // The combined items: usable unless HP is full and MP is full (or no charge) too.
        let full = Vitals { hp: 100, mp: 100, ..base };
        assert!(!can_use(4, &t(full)) && !can_use(7, &t(full)) && !can_use(0x83, &t(full)));
        assert!(can_use(4, &t(Vitals { mp: 99, ..full })) && can_use(4, &t(Vitals { hp: 99, ..full })));
        assert!(can_use(4, &t(Vitals { charge_total: 1.0, ..full })), "a running charge counts as room");
        // The drive recovery items need a point below the maximum; the boosts a byte not above 99.
        assert!(can_use(0x112, &t(base)) && !can_use(0x113, &t(Vitals { drive_points: 3, ..base })));
        assert!(can_use(0x114, &t(base)), "strength byte 99 can still take one");
        assert!(!can_use(0x115, &t(base)), "magic byte 100 is full");
        assert!(can_use(0x116, &t(base)) && can_use(0x117, &t(base)));
        assert!(can_use(0x115, &Target { record: None, ..t(base) }), "no record: usable");
        // Item 0 and any other id do nothing.
        assert!(!can_use(0, &t(base)) && !can_use(0x50, &t(base)) && !can_use(0x118, &t(base)));
    }

    #[test]
    fn group_items_show_one_all_row_and_single_items_one_member() {
        assert_eq!(target_rows(5, false, 3), (1, true));
        assert_eq!(target_rows(0x83, false, 3), (1, true));
        assert_eq!(target_rows(0x112, true, 3), (1, false));
        assert_eq!(target_rows(1, false, 3), (3, false));
        assert!(group_item(6) && group_item(7) && !group_item(4) && !group_item(8));
    }

    #[test]
    fn the_prompt_offers_use_for_usable_types_and_refuses_an_item_with_no_sell_price() {
        assert_eq!(prompt_rows(true), vec![0, 1]);
        assert_eq!(prompt_rows(false), vec![1]);
        assert_eq!(prompt_choice(true, 0, true), Choice::Use);
        assert_eq!(prompt_choice(true, 0, false), Choice::Use, "Use needs no sell price");
        assert_eq!(prompt_choice(true, 1, true), Choice::Discard);
        assert_eq!(prompt_choice(true, 1, false), Choice::Refused);
        assert_eq!(prompt_choice(false, 0, true), Choice::Discard, "a type without Use: the one row throws away");
        assert_eq!(prompt_choice(false, 0, false), Choice::Refused);
    }

    #[test]
    fn the_grid_moves_by_cell_and_row_and_wraps_inside_a_row() {
        // Five cells: rows (0 1) (2 3) (4).
        let n = 5;
        let up = |fresh| Key::Up { fresh };
        let down = |fresh| Key::Down { fresh };
        assert_eq!(grid_move(0, n, Key::Right), 1);
        assert_eq!(grid_move(1, n, Key::Right), 0, "Right wraps inside the row");
        assert_eq!(grid_move(0, n, Key::Left), 1);
        assert_eq!(grid_move(4, n, Key::Left), 4, "a one-cell row stays");
        assert_eq!(grid_move(0, n, down(true)), 2);
        assert_eq!(grid_move(3, n, down(true)), 4, "the short last row: the last cell");
        assert_eq!(grid_move(4, n, down(true)), 0, "a new press wraps to the top");
        assert_eq!(grid_move(4, n, down(false)), 4, "a held repeat stops at the end");
        assert_eq!(grid_move(1, n, up(true)), 4);
        assert_eq!(grid_move(1, n, up(false)), 1);
        assert_eq!(grid_move(2, n, up(true)), 0);
        assert_eq!(grid_move(0, 0, down(true)), 0);
    }

    fn start(stock: Vec<Stack>) -> (Machine, BTreeMap<u16, ItemRow>, ExeTables) {
        (Machine::new(1, stock), rows(), exe())
    }

    #[test]
    fn stock_row_of_page_1_opens_the_stock_and_cancel_comes_back() {
        let (mut m, r, e) = start(stacks(&[(1, 3)]));
        let g = G { members: 1, helps: true };
        // Sora's row opens his equipment (stage M4, `menu_equipment.rs`).
        assert_eq!(m.press(Key::Confirm, &r, &e, &g), vec![Effect::Sound(snd::CONFIRM), Effect::Member(0)]);
        assert_eq!(m.press(Key::Down { fresh: true }, &r, &e, &g), vec![Effect::Sound(snd::CURSOR), Effect::Page(MEMBERS)]);
        assert_eq!(m.cursor1, 1);
        assert_eq!(m.press(Key::Down { fresh: true }, &r, &e, &g)[0], Effect::Sound(snd::CURSOR), "wraps to Sora");
        m.press(Key::Up { fresh: true }, &r, &e, &g);
        let fx = m.press(Key::Confirm, &r, &e, &g);
        assert_eq!(fx, vec![Effect::Sound(snd::CONFIRM), Effect::Filter(0), Effect::Page(STOCK)]);
        assert_eq!(m.press(Key::Cancel, &r, &e, &g), vec![Effect::Sound(snd::CANCEL), Effect::Page(MEMBERS)]);
        assert_eq!(m.cursor1, 1, "page 1's cursor is on Stock again");
        assert_eq!(m.press(Key::Cancel, &r, &e, &g), vec![Effect::Sound(snd::CANCEL), Effect::Leave]);
        assert_eq!(m.press(Key::Close, &r, &e, &g), vec![Effect::Close]);
    }

    #[test]
    fn three_members_get_three_rows_and_the_use_names_the_row_chosen() {
        let (mut m, r, e) = start(stacks(&[(1, 3)]));
        m.members = 3;
        let g = G { members: 3, helps: true };
        // Page 1: Sora, Donald, Goofy and Stock; the third member's row opens his equipment.
        m.press(Key::Down { fresh: true }, &r, &e, &g);
        m.press(Key::Down { fresh: true }, &r, &e, &g);
        assert_eq!(m.cursor1, 2);
        let fx = m.press(Key::Confirm, &r, &e, &g);
        assert_eq!(fx, vec![Effect::Sound(snd::CONFIRM), Effect::Member(2)]);
        // The Stock is the row after the members.
        m.press(Key::Down { fresh: true }, &r, &e, &g);
        assert_eq!(m.cursor1, 3);
        m.press(Key::Confirm, &r, &e, &g);
        m.press(Key::Confirm, &r, &e, &g);
        assert_eq!(m.page, PROMPT);
        m.press(Key::Confirm, &r, &e, &g);
        assert_eq!(m.page, TARGET);
        m.press(Key::Down { fresh: true }, &r, &e, &g);
        m.press(Key::Down { fresh: true }, &r, &e, &g);
        assert_eq!(m.target, 2, "three rows to choose from");
        let fx = m.press(Key::Confirm, &r, &e, &g);
        assert!(fx.contains(&Effect::Use { item: 1, row: 2 }));
    }

    #[test]
    fn the_target_check_is_per_member_and_a_group_item_needs_any_member() {
        let hurt = Target { vitals: Vitals { hp: 10, hp_max: 50, mp: 5, mp_max: 5, ..Default::default() }, record: None };
        let well = Target { vitals: Vitals { hp: 50, hp_max: 50, mp: 5, mp_max: 5, ..Default::default() }, record: None };
        let snap = Snapshot { targets: vec![well, hurt, well] };
        assert_eq!(snap.members(), 3);
        assert!(!snap.would_help(1, 0) && snap.would_help(1, 1) && !snap.would_help(1, 2), "Potion: only Donald is hurt");
        // Elixir-like group items ("All", row 0) help when any member can use them.
        let group = (0..0x200).find(|&i| group_item(i) && can_use(i, &hurt)).expect("a group item the check knows");
        assert!(snap.would_help(group, 0));
        assert!(!Snapshot { targets: vec![well, well] }.would_help(group, 0));
    }

    /// `FUN_001a15a0` has no KO test: a down member (HP 0, below the maximum) is offered the heal and the Elixir-like group
    /// item counts him (WP-PL2; replaces WP-PM1b's refusal, which assumed a revive rule nobody had read).
    #[test]
    fn a_down_member_is_offered_the_heals_as_the_use_check_has_no_ko_test() {
        let down = Target { vitals: Vitals { hp: 0, hp_max: 50, mp: 5, mp_max: 5, ..Default::default() }, record: Some([0, 1, 1, 1]) };
        let well = Target { vitals: Vitals { hp: 50, hp_max: 50, mp: 5, mp_max: 5, ..Default::default() }, record: Some([0, 1, 1, 1]) };
        let snap = Snapshot { targets: vec![down, well] };
        assert!(snap.would_help(1, 0), "a Potion on a down member revives him");
        assert!(!snap.would_help(1, 1));
        assert!(snap.would_help(0x115, 0), "a boost writes the record whether he is up or not");
        let group = (0..0x200).find(|&i| group_item(i) && can_use(i, &down)).expect("a group item");
        assert!(Snapshot { targets: vec![down] }.would_help(group, 0), "All with only a down member helps");
    }

    /// The heal goes through the HP method: a KO'd member gets up with the scaled amount (`battle::hp_change`), a
    /// standing one is healed by the item's amount, held at the maximum.
    #[test]
    fn an_item_revives_a_ko_member_through_the_hp_method() {
        let mut v = Vitals { hp: 0, hp_max: 50, ..Default::default() };
        item_effect(1, 40, 100, &mut v, 12.0);
        assert_eq!(v.hp, 20, "Potion: 50 * 40 %");
        let (mut hp, mut ko) = (0, true);
        let c = crate::party_battle::hp_change(&mut hp, 50, 0, &mut ko, v.hp, 0.5);
        assert!(c.revived && !ko, "the KO bit is cleared");
        assert_eq!(hp, 10, "the amount is scaled by sstm +0x54 (FUN_0017cee8)");
        let mut v = Vitals { hp: 10, hp_max: 50, ..Default::default() };
        item_effect(1, 40, 100, &mut v, 12.0);
        assert_eq!(v.hp, 30, "10 + 50 * 40 %");
        item_effect(1, 40, 100, &mut v, 12.0);
        assert_eq!(v.hp, 50, "capped at the maximum");
    }

    #[test]
    fn using_an_item_asks_for_a_member_then_takes_one_and_returns_to_the_stock() {
        let (mut m, r, e) = start(stacks(&[(1, 3), (100, 1)]));
        let g = G { members: 1, helps: true };
        m.page = STOCK;
        assert_eq!(m.press(Key::Confirm, &r, &e, &g), vec![Effect::Sound(snd::CONFIRM), Effect::Page(PROMPT)]);
        assert!(m.usable && m.prompt == 0);
        assert_eq!(m.press(Key::Confirm, &r, &e, &g), vec![Effect::Sound(snd::CONFIRM), Effect::Page(TARGET)]);
        let fx = m.press(Key::Confirm, &r, &e, &g);
        assert_eq!(fx, vec![Effect::Sound(snd::USED), Effect::Use { item: 1, row: 0 }, Effect::Page(STOCK)]);
        assert_eq!(m.page, STOCK);
        // The list is rebuilt by the caller; the cursor stays on the list.
        m.reload(stacks(&[(1, 2), (100, 1)]), false);
        assert_eq!(m.cursor, 0);
        // Cancel on the target list goes back to the prompt, and from there to the stock.
        m.press(Key::Confirm, &r, &e, &g);
        m.press(Key::Confirm, &r, &e, &g);
        assert_eq!(m.press(Key::Cancel, &r, &e, &g), vec![Effect::Sound(snd::CANCEL), Effect::Page(PROMPT)]);
        assert_eq!(m.press(Key::Cancel, &r, &e, &g), vec![Effect::Sound(snd::CANCEL), Effect::Page(STOCK)]);
    }

    #[test]
    fn an_item_that_would_do_nothing_shows_the_notice_and_keeps_the_stock() {
        let (mut m, r, e) = start(stacks(&[(1, 3)]));
        let g = G { members: 1, helps: false };
        m.page = STOCK;
        m.press(Key::Confirm, &r, &e, &g);
        m.press(Key::Confirm, &r, &e, &g);
        let fx = m.press(Key::Confirm, &r, &e, &g);
        assert_eq!(fx, vec![Effect::Sound(snd::CONFIRM), Effect::Page(NOTICE)], "no Use effect: kind 8 plays the confirm sound");
        assert_eq!(m.page, NOTICE);
        assert_eq!(m.press(Key::Confirm, &r, &e, &g), vec![Effect::Sound(snd::CANCEL), Effect::Page(TARGET)]);
    }

    #[test]
    fn throwing_away_asks_first_and_only_yes_discards() {
        let (mut m, r, e) = start(stacks(&[(1, 3), (100, 1), (7, 2)]));
        let g = G { members: 1, helps: true };
        m.page = STOCK;
        // Item 1: Use and Throw away; the second row asks.
        m.press(Key::Confirm, &r, &e, &g);
        m.press(Key::Down { fresh: true }, &r, &e, &g);
        assert_eq!(m.press(Key::Confirm, &r, &e, &g), vec![Effect::Sound(snd::CONFIRM), Effect::Page(DISCARD)]);
        // The question starts on No (row 0); cancel and No go back to the prompt.
        assert_eq!(m.press(Key::Cancel, &r, &e, &g), vec![Effect::Sound(snd::CANCEL), Effect::Page(PROMPT)]);
        m.press(Key::Confirm, &r, &e, &g);
        assert_eq!(m.press(Key::Confirm, &r, &e, &g), vec![Effect::Sound(snd::CANCEL), Effect::Page(PROMPT)], "No");
        m.press(Key::Confirm, &r, &e, &g);
        m.press(Key::Down { fresh: true }, &r, &e, &g);
        let fx = m.press(Key::Confirm, &r, &e, &g);
        assert_eq!(fx, vec![Effect::Sound(snd::CONFIRM), Effect::Discard { item: 1 }, Effect::Page(STOCK)], "Yes");
        // Item 7 has no sell price: the second row refuses.
        m.cursor = 2;
        m.page = STOCK;
        m.press(Key::Confirm, &r, &e, &g);
        m.press(Key::Down { fresh: true }, &r, &e, &g);
        assert_eq!(m.press(Key::Confirm, &r, &e, &g), vec![Effect::Sound(snd::REFUSED)]);
        assert_eq!(m.page, PROMPT);
        // The Keyblade (type 2) has only the Throw away row, which it can do not (no price): refused.
        m.cursor = 1;
        m.page = STOCK;
        m.press(Key::Confirm, &r, &e, &g);
        assert!(!m.usable);
        assert_eq!(m.press(Key::Confirm, &r, &e, &g), vec![Effect::Sound(snd::REFUSED)]);
    }

    #[test]
    fn the_filter_keys_cycle_five_modes_and_a_shorter_list_keeps_the_cursor_on_it() {
        let (mut m, r, e) = start(stacks(&[(1, 1)]));
        let g = G { members: 1, helps: true };
        m.page = STOCK;
        assert_eq!(m.press(Key::Next, &r, &e, &g), vec![Effect::Sound(snd::CONFIRM), Effect::Filter(1), Effect::Page(STOCK)]);
        assert_eq!(m.press(Key::Prev, &r, &e, &g)[1], Effect::Filter(0));
        assert_eq!(m.press(Key::Prev, &r, &e, &g)[1], Effect::Filter(4), "wraps below 0");
        m.filter = 4;
        assert_eq!(m.press(Key::Next, &r, &e, &g)[1], Effect::Filter(0), "wraps at 5");
        // A list that lost its last entry puts the cursor on the new last one.
        m.reload(stacks(&[(1, 1), (2, 1), (7, 1)]), true);
        m.cursor = 2;
        m.reload(stacks(&[(1, 1), (2, 1)]), false);
        assert_eq!(m.cursor, 1);
        m.reload(Vec::new(), false);
        assert_eq!(m.cursor, 0);
        // With an empty list the confirm does nothing.
        assert!(m.press(Key::Confirm, &r, &e, &g).is_empty());
    }

    #[test]
    fn the_grid_scrolls_a_row_at_a_time_keeping_the_cursor_in_view() {
        let stock: Vec<Stack> = (0..50).map(|i| Stack { id: 1, count: i as u8 + 1 }).collect();
        let (mut m, r, e) = start(stock);
        let g = G { members: 1, helps: true };
        m.page = STOCK;
        for _ in 0..12 {
            m.press(Key::Down { fresh: true }, &r, &e, &g);
        }
        assert_eq!((m.cursor, m.scroll), (24, 4), "row 12 needs the first visible row at 4");
        for _ in 0..12 {
            m.press(Key::Up { fresh: true }, &r, &e, &g);
        }
        assert_eq!((m.cursor, m.scroll), (0, 0));
    }

    /// The disc and the executable's item table loaded into a kh2-ee machine (the items oracle's setup).
    struct Oracle {
        ee: kh2_ee::Ee,
        items: Vec<t::Item>,
        exe: Exe,
    }

    const STACK: u32 = 0x01ff_0000;
    const ITEM_PTR: u32 = 0x01c9_5588;
    const ITEM_AT: u32 = 0x0180_0000;
    const STATUS: u32 = 0x0190_0000;
    const RECORD: u32 = 0x0192_0000;

    fn oracle() -> Option<Oracle> {
        let iso = std::env::var("KH2_ISO").ok().or_else(|| {
            let f = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../local/iso.txt");
            std::fs::read_to_string(f).ok().map(|s| s.trim().to_string())
        });
        let Some(iso) = iso else {
            eprintln!("skipped: no disc (set KH2_ISO)");
            return None;
        };
        let mut a = kh2_archive::Archive::open(&iso).map_err(|e| eprintln!("skipped: {iso}: {e}")).ok()?;
        let items = kh2_tables::load::<t::Item>(&mut a, &t::ITEM).ok()?.rows;
        let blob = kh2_tables::subfile(&mut a, &t::ITEM).ok()?;
        let elf = ["SLUS_210.05", "SLUS_210.05;1"].iter().find_map(|p| a.iso().read(p).ok())?;
        let mut ee = kh2_ee::Ee::new();
        ee.load_elf(&elf)?;
        ee.write(ITEM_AT, &blob);
        ee.w32(ITEM_PTR, ITEM_AT);
        let exe = Exe::load(elf)?;
        Some(Oracle { ee, items, exe })
    }

    /// The oracle for the Use row's target check: the game's own `FUN_001a15a0` in kh2-ee, for every item row,
    /// over HP, MP, MP charge and drive states and record bytes, against `can_use`. `FUN_0019fe48` (a member's
    /// status record) is a stub that returns a record laid out in free memory, whose `+0x238` points at a
    /// character record holding the four boost bytes (as `docs/items.md`'s item oracle does for `FUN_001a12a0`).
    #[test]
    fn can_use_matches_the_game() {
        let Some(mut o) = oracle() else { return };
        let ee = &mut o.ee;
        // FUN_0019fe48 -> `lui v0, hi; jr ra; ori v0, v0, lo`.
        ee.w32(0x0019_fe48, 0x3c02_0000 | (STATUS >> 16));
        ee.w32(0x0019_fe4c, 0x03e0_0008);
        ee.w32(0x0019_fe50, 0x3442_0000 | (STATUS & 0xffff));
        let states = [
            Vitals { hp: 1, hp_max: 120, hp_min: 0, mp: 0, mp_max: 100, charge_left: 0.0, charge_total: 0.0, drive_fraction: 0, drive_points: 0, drive_max: 3 },
            Vitals { hp: 120, hp_max: 120, hp_min: 0, mp: 100, mp_max: 100, charge_left: 0.0, charge_total: 0.0, drive_fraction: 70, drive_points: 3, drive_max: 3 },
            Vitals { hp: 120, hp_max: 120, hp_min: 0, mp: 100, mp_max: 100, charge_left: 500.0, charge_total: 600.0, drive_fraction: 99, drive_points: 3, drive_max: 3 },
            Vitals { hp: 120, hp_max: 120, hp_min: 0, mp: 3, mp_max: 100, charge_left: 0.0, charge_total: 0.0, drive_fraction: 10, drive_points: 2, drive_max: 3 },
            Vitals { hp: 7, hp_max: 120, hp_min: 0, mp: 100, mp_max: 100, charge_left: 0.0, charge_total: 0.0, drive_fraction: 10, drive_points: 2, drive_max: 3 },
        ];
        let records: [Option<[u8; 4]>; 5] = [None, Some([0, 0, 0, 0]), Some([100, 100, 100, 100]), Some([99, 100, 99, 100]), Some([100, 99, 100, 99])];
        let mut checked = 0;
        let mut usable = 0;
        for row in &o.items {
            let ptr = ee.call(0x001a_66e8, &[row.id as u64], &[], STACK).expect("FUN_001a66e8") as u32;
            if ptr == 0 {
                continue;
            }
            for st in &states {
                for rec in &records {
                    ee.write(STATUS, &[0u8; 0x260]);
                    ee.w32(STATUS, st.hp as u32);
                    ee.w32(STATUS + 4, st.hp_max as u32);
                    ee.w32(STATUS + 0x180, st.mp as u32);
                    ee.w32(STATUS + 0x184, st.mp_max as u32);
                    ee.write(STATUS + 0x1b0, &[st.drive_fraction, st.drive_points, st.drive_max]);
                    ee.write(STATUS + 0x1bc, &st.charge_left.to_le_bytes());
                    ee.write(STATUS + 0x1c0, &st.charge_total.to_le_bytes());
                    ee.w32(STATUS + 0x248, 100);
                    match rec {
                        Some(r) => {
                            ee.write(RECORD, &[0u8; 0x40]);
                            ee.write(RECORD + 8, r);
                            ee.w32(STATUS + 0x238, RECORD);
                        }
                        None => ee.w32(STATUS + 0x238, 0),
                    }
                    let game = ee.call(0x001a_15a0, &[ptr as u64, 1], &[], STACK).expect("FUN_001a15a0") as u8 != 0;
                    let port = can_use(row.id, &Target { vitals: *st, record: *rec });
                    assert_eq!(port, game, "item {} state {st:?} record {rec:?}", row.id);
                    checked += 1;
                    usable += game as u32;
                }
            }
        }
        assert!(checked > 1000 && usable > 100, "checked {checked}, usable {usable}");
        eprintln!("oracle: FUN_001a15a0 equal for {checked} cases ({usable} usable) over {} item rows", o.items.len());
    }

    /// The oracle for the Stock list: the game's own `FUN_00283a70` (with the five stubs' filter arguments 0, 0xd,
    /// 0xb, 0xc, 0xe) and the sort `FUN_002845a0` in kh2-ee over a party block with counts, against `stock`. The
    /// type names of `item_page_tables_have_the_documented_shape` (kh2-image) are read from the same executable.
    #[test]
    fn stock_list_matches_the_game() {
        const LIST: u32 = 0x0194_0000;
        let Some(mut o) = oracle() else { return };
        let exe = ExeTables::read(&o.exe).expect("the executable's Items tables");
        let rows: Vec<ItemRow> = o.items.iter().map(ItemRow::of).collect();
        let mut seen = std::collections::BTreeSet::new();
        let mut total = 0;
        for variant in 0..3u32 {
            // Counts for every row with a slot: a few owned, the rest 0 (the party block's count array).
            let mut counts = vec![0u8; 0x140];
            for (n, r) in o.items.iter().enumerate() {
                if r.r#type == 0x13 || (r.flag0 & 1) != 0 {
                    continue;
                }
                let k = (n as u32 + variant) % 4;
                if k != 0 && (r.slot as usize) < counts.len() {
                    counts[r.slot as usize] = (k * 3 + variant) as u8;
                }
            }
            o.ee.write(crate::progress::addr::PARTY + crate::progress::off::COUNTS as u32, &counts);
            let game_count = |ee: &mut kh2_ee::Ee, id: u16| ee.call(0x001a_1cc8, &[id as u64], &[], STACK).expect("FUN_001a1cc8") as u8;
            let mut ids = Vec::new();
            for r in &o.items {
                ids.push((r.id, game_count(&mut o.ee, r.id)));
            }
            let by_id: std::collections::BTreeMap<u16, u8> = ids.into_iter().collect();
            let count = |id: u16| by_id.get(&id).copied().unwrap_or(0);
            // The five filters of the Stock page, then argument 10 (the consumables and the spells), which the
            // Customize page's shortcut list builds (`crate::menu::customize::candidates`).
            for (filter, arg) in [(0u8, 0u64), (1, 0xd), (2, 0xb), (3, 0xc), (4, 0xe), (5, 10)] {
                o.ee.write(LIST, &vec![0u8; 0x1100]);
                o.ee.call(0x0028_3a70, &[LIST as u64, 0, arg], &[], STACK).expect("FUN_00283a70");
                o.ee.call(0x0028_45a0, &[LIST as u64], &[], STACK).expect("FUN_002845a0");
                let n = o.ee.r32(LIST) as usize;
                let game: Vec<Stack> = (0..n.min(400))
                    .map(|k| {
                        let at = LIST + 4 + 6 * k as u32;
                        Stack { id: o.ee.r16(at), count: o.ee.r8(at + 2) }
                    })
                    .collect();
                let port = if filter == 5 { crate::menu::customize::candidates(&rows, &exe, &count) } else { stock(&rows, exe.filter_word(filter), &exe, &count, &|_| false) };
                assert_eq!(port, game, "filter {filter} (argument {arg:#x}), counts variant {variant}");
                seen.insert((filter, game.len()));
                total += game.len();
            }
        }
        assert!(total > 50, "the lists were not trivially empty: {total}");
        eprintln!("oracle: FUN_00283a70 + FUN_002845a0 equal for 18 lists ({total} entries; (filter, length) {seen:?}; filter 5 is argument 10, Customize's shortcut candidates)");
    }

    #[test]
    fn an_icon_follows_the_type_and_the_flag_and_an_unlisted_type_shows_0xb() {
        let e = exe();
        assert_eq!((e.icon(0, false), e.icon(0, true)), (0, 12));
        assert_eq!((e.icon(2, false), e.icon(2, true)), (4, 14));
        assert_eq!(e.icon(9, true), 0xb);
    }

    #[test]
    fn messages_take_the_type_and_the_name_in_place_of_their_variables() {
        let msg = vec![Token::Glyph(1), Token::Variable, Token::Space, Token::Variable, Token::Glyph(2)];
        let out = fill(msg, &[vec![Token::Glyph(10)], vec![Token::Glyph(20), Token::Glyph(21)]]);
        assert_eq!(out, vec![Token::Glyph(1), Token::Glyph(10), Token::Space, Token::Glyph(20), Token::Glyph(21), Token::Glyph(2)]);
        assert_eq!(count_text(1), None, "a single item shows no count");
        assert_eq!(count_text(12).as_deref(), Some("\u{d7}12"));
    }
}
