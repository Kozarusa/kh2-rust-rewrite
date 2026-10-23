//! The pause menu's Customize pages, stage M3 (docs/systems/ui/menus-pause.md 2.4 and docs/systems/ui/menus-customize.md section 14): page 0x19 the character list
//! (`FUN_002707f8`), page 0x1a Sora's settings (`FUN_00271078`, built by `FUN_00270ae0`), the sub pages 0x1d (the
//! command a shortcut takes), 0x1e (a spell's MP Charge switch), 0x1f (an item's Auto-Reload) and 0x1c (the
//! question before Auto-Reload goes off) (`FUN_00272018`).
//!
//! This file is a child module of `menu` (`#[path] pub mod customize;` there). The pure part is the rules, so the
//! tests drive them:
//!
//! * `build_rows` is the table builder `FUN_002723c8` for the leader: four shortcut rows, then one row per owned
//!   spell, then one per distinct battle item,
//! * `candidates` is the shortcut list: the stock list of `FUN_00283a70` with the filter word that holds the
//!   consumables and the spells (`FUN_00283810`, argument 10), spells first (`FUN_002845a0`),
//! * `Machine::press` is the page handlers' result switches.
//!
//! The writers are `Progress::set_shortcut` (`FUN_00274dd0`), `toggle_spell_switch` / `set_spell_switch`
//! (`FUN_00274e50`) and `set_auto_reload` (`FUN_00274f70`).
//!
//! WP-PA4 adds the companions' page 0x1b (the same handler `FUN_00271078`, the row kinds of the table at 0x381d48:
//! 3, 4, 2) and its sub pages 0x20 (Battle Style, 6 rows, `FUN_00275398`), 0x21 (an ability's use setting, 3 rows,
//! `FUN_00275410`) and the four-row 0x1f (Auto-Reload and the item's use setting, `FUN_00274f70`): the character list
//! has a row per member of the party, the Style row writes setting byte 0 of the member's record (`+0xd4`), the
//! others byte `1 + FUN_00114348(id)` (`+0xd5 + ..`).

use bevy::prelude::*;
use kh2_image::text::{Style, Token};

use super::item_pages::{label, node, set_help, stock, tokens_of, ExeTables, ItemRow, ItemsMenu, Stack};
use super::{Menu, Phase};
use crate::assets::Disc;
use crate::audio::PlaySe;
use crate::menu_gfx::MenuScene;
use crate::menu_pages::{Entry, Exe};
use crate::progress::{Progress, SORA};

// --- pages --------------------------------------------------------------------------------------

pub const CHARACTERS: u16 = 0x19;
pub const ROWS: u16 = 0x1a;
pub const CANDIDATES: u16 = 0x1d;
pub const SPELL: u16 = 0x1e;
pub const RELOAD: u16 = 0x1f;
pub const QUESTION: u16 = 0x1c;
/// A companion's settings (`FUN_00271078` for the character list's rows after the first) and its sub pages.
pub const FRIEND: u16 = 0x1b;
pub const STYLE: u16 = 0x20;
pub const ABILITY: u16 = 0x21;
/// Battle styles (`FUN_00275398`: the byte wraps to 0 above 5) and the use settings of an ability (`FUN_00275410`).
pub const STYLES: usize = 6;
pub const USES: usize = 3;
/// The kind byte (`StatEntry` low byte, item `+6`) of the abilities a companion's page lists (`FUN_002723c8` case 4).
pub const ABILITY_KIND: u8 = 2;
/// Entry 2 of the top list (Customize) opens page 0x19.
pub const ENTRY: usize = 2;
/// Rows of the settings list shown at once (`FUN_00299010(list, 9)`).
pub const VISIBLE: usize = 9;
/// Spell slots (Fire, Blizzard, Thunder, Cure, Magnet, Reflect) and item type of a spell item.
pub const SPELLS: usize = 6;
pub const SPELL_TYPE: u8 = 0x12;

pub use super::item_pages::snd;

// --- the rows -------------------------------------------------------------------------------------

/// The row kinds of the leader (`DAT_00381d40`: 0, 1, 2) and of the companions (`DAT_00381d48`: 3, 4, 2).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// One of the four command shortcuts (save `+0x2600`).
    Shortcut,
    /// A spell's MP Charge switch (the bit of the word at save `+0x264c`).
    Spell,
    /// A battle item's Auto-Reload (its reserved ids, record `+0x44`).
    Item,
    /// A companion's Battle Style (kind 3; record `+0xd4`).
    Style,
    /// One of a companion's equipped abilities with its use setting (kind 4; record `+0xd5 + k`).
    Ability,
}

impl Kind {
    /// The kind byte of the tables (`FUN_00272af8`'s lists): 0 shortcut, 1 spell, 2 item, 3 style, 4 ability.
    pub fn index(self) -> usize {
        match self {
            Kind::Shortcut => 0,
            Kind::Spell => 1,
            Kind::Item => 2,
            Kind::Style => 3,
            Kind::Ability => 4,
        }
    }
}

/// One row of the working table `DAT_0036d030` (8 bytes: `+8` the item id, `+10` the flag).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Row {
    pub kind: Kind,
    /// The item the row is about: the one whose command the shortcut holds (0 when empty), the spell's item, the
    /// battle item.
    pub id: u16,
    /// The flag byte: a spell with MP Charge on, an item with Auto-Reload on (a shortcut row copies the flag of
    /// the row of the same item).
    pub on: bool,
    /// A spell row's slot, which is its bit in the switch word.
    pub slot: u8,
    /// A companion's setting byte of the row (`+0xb`): the battle style, an ability's use setting 0..2, an item's
    /// use setting (0 or 2).
    pub setting: u8,
}

impl Row {
    pub fn new(kind: Kind, id: u16, on: bool, slot: u8) -> Row {
        Row { kind, id, on, slot, setting: 0 }
    }
}

/// What `build_rows` reads of the game.
pub struct Inputs<'a> {
    /// The four command words (`Progress::shortcuts`).
    pub shortcuts: [u16; 4],
    /// The item of each spell slot (`ItemIndex::magic_items`).
    pub spell_items: [u16; SPELLS],
    pub count: &'a dyn Fn(u16) -> u8,
    /// The switch bit of a slot is set: "Not Used" (`FUN_001a4bb8` answers 1 for a clear bit).
    pub not_used: &'a dyn Fn(u8) -> bool,
    /// Sora's battle item slots as (item, reserved id).
    pub slots: Vec<(u16, u16)>,
    /// `FUN_00272fc8`: the first consumable or spell item (table order) whose command is the word, 0 for none.
    pub item_of_command: &'a dyn Fn(u16) -> u16,
}

/// `FUN_002723c8` for the leader (the character list's row 0).
pub fn build_rows(i: &Inputs) -> Vec<Row> {
    let mut rows = Vec::new();
    for &cmd in &i.shortcuts {
        let id = if cmd == 0 { 0 } else { (i.item_of_command)(cmd) };
        rows.push(Row::new(Kind::Shortcut, id, false, 0));
    }
    for slot in 0..SPELLS as u8 {
        let item = i.spell_items[slot as usize];
        if item != 0 && (i.count)(item) > 0 {
            rows.push(Row::new(Kind::Spell, item, !(i.not_used)(slot), slot));
        }
    }
    // The battle items: the id each slot carries, or reserves when empty, once; the flag is whether that slot's
    // reserved id is the item (the original compares them as it takes them).
    let mut seen: Vec<u16> = Vec::new();
    for &(item, reserved) in &i.slots {
        let id = if item != 0 { item } else { reserved };
        if id == 0 || seen.contains(&id) {
            continue;
        }
        seen.push(id);
        rows.push(Row::new(Kind::Item, id, reserved == id, 0));
    }
    // The shortcut rows show the state of the row with the same item (the loop at the end of the routine).
    let tail: Vec<Row> = rows[4..].to_vec();
    for r in rows.iter_mut().take(4) {
        if r.id != 0
            && let Some(t) = tail.iter().rfind(|t| t.id == r.id)
        {
            r.on = t.on;
        }
    }
    rows
}

/// What `build_friend_rows` reads of the game.
pub struct FriendInputs<'a> {
    /// Setting byte 0 of the member's record: the battle style.
    pub style: u8,
    /// The member's equipped abilities whose item row has kind 2, as (item id, position in the item table).
    pub abilities: Vec<(u16, usize)>,
    /// The member's battle item slots as (item, reserved id).
    pub slots: Vec<(u16, u16)>,
    /// The use setting byte of an item or ability (record `+0xd5 + FUN_00114348(id)`), 0 when the id has no byte.
    pub setting_of: &'a dyn Fn(u16) -> u8,
}

/// `FUN_002723c8` for a companion (the character list's rows after the first, kinds 3, 4, 2 of `DAT_00381d48`): the
/// Battle Style row (case 3 keeps the style in the table header; the list shows it as its first row), then the
/// equipped abilities of kind 2 in item-table order (case 4 sorts the records by their table position), then one row
/// per distinct battle item as the leader's (case 2) with the item's use setting.
pub fn build_friend_rows(i: &FriendInputs) -> Vec<Row> {
    let mut rows = vec![Row { setting: i.style, ..Row::new(Kind::Style, 0, false, 0) }];
    let mut abilities = i.abilities.clone();
    abilities.sort_by_key(|a| a.1);
    for (id, _) in abilities {
        rows.push(Row { setting: (i.setting_of)(id), ..Row::new(Kind::Ability, id, true, 0) });
    }
    let mut seen: Vec<u16> = Vec::new();
    for &(item, reserved) in &i.slots {
        let id = if item != 0 { item } else { reserved };
        if id == 0 || seen.contains(&id) {
            continue;
        }
        seen.push(id);
        rows.push(Row { setting: (i.setting_of)(id), ..Row::new(Kind::Item, id, reserved == id, 0) });
    }
    rows
}

/// The state index of a companion's item row on page 0x1f (`FUN_00272018`): the Auto-Reload flag, plus 2 when the use
/// setting byte is 2.
pub fn item_state(row: &Row) -> u8 {
    u8::from(row.on) + if row.setting == 2 { 2 } else { 0 }
}

/// The shortcut candidates after "Remove Shortcut settings" (`FUN_00273d30`: entry 0 is the removal, then the
/// stock list): the owned consumables and spells in table order with the spells first (`FUN_002845a0` keys a
/// spell by its table position and everything else by position + 1000). `word` is the filter word of
/// `FUN_00283810` for argument 10: bit 0 (consumables) and bit 18 (spells).
pub fn candidates(order: &[ItemRow], exe: &ExeTables, count: &dyn Fn(u16) -> u8) -> Vec<Stack> {
    let mut list = stock(order, 1 | 1 << SPELL_TYPE, exe, count, &|_| false);
    let kind = |id: u16| order.iter().find(|r| r.id == id).map_or(0, |r| r.kind);
    list.sort_by_key(|s| kind(s.id) != SPELL_TYPE);
    list
}

// --- the page machine ---------------------------------------------------------------------------------

/// The list results of docs/systems/ui/menus-pause.md 1.7 that the Customize pages use.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Key {
    Up { fresh: bool },
    Down { fresh: bool },
    Confirm,
    Cancel,
    Close,
    /// -10 / -11: the previous or next character.
    Prev,
    Next,
    /// -6: the quick switch of the highlighted row (`FUN_00271078`'s case -6).
    Toggle,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Effect {
    Sound(u32),
    Page(u16),
    /// Store the command of `item` (0: none) in the shortcut `slot` (`FUN_00274dd0`); the caller turns the item into
    /// its command (`FUN_001a1f78`).
    Shortcut { slot: usize, item: u16 },
    /// The spell slot's switch: usable ("MP Charge") or not (`FUN_00274e50`).
    Spell { slot: u8, usable: bool },
    /// The item's Auto-Reload on or off (`FUN_00274f70`).
    Reload { item: u16, on: bool },
    /// A companion's Battle Style (`FUN_00275398`): setting byte 0 of the member's record.
    Style(u8),
    /// A companion's ability use setting (`FUN_00275410`): byte `1 + FUN_00114348(id)`.
    Ability { id: u16, value: u8 },
    /// A companion's item row takes the state of page 0x1f (`FUN_00274f70`): bit 0 Auto-Reload, bit 1 the use setting
    /// 2 (byte `1 + FUN_00114348(id)`).
    FriendItem { item: u16, state: u8 },
    Leave,
    Close,
}

#[derive(Clone, Debug)]
pub struct Machine {
    pub page: u16,
    pub rows: Vec<Row>,
    /// The settings list's cursor and first visible row.
    pub cursor: usize,
    pub scroll: usize,
    /// The shortcut candidates (the removal is row 0 and not in the list) and the cursor on that page.
    pub candidates: Vec<Stack>,
    pub cand: usize,
    pub cand_scroll: usize,
    /// The cursor of the two-row sub pages 0x1e and 0x1f, and of the question (0 No, 1 Yes).
    pub sub: usize,
    pub question: usize,
    /// The party menu's characters (member ids, the leader first) and the one the pages show (`FUN_00252fb8(0x19)`).
    pub members: Vec<u16>,
    pub who: usize,
    /// The state a companion's item row takes when the question is answered Yes.
    pub pending: u8,
}

/// Rows the candidate list shows at once, the removal row among them (`FUN_00273d30` caps the list at 8).
pub const CAND_VISIBLE: usize = 8;

impl Machine {
    pub fn new(rows: Vec<Row>, candidates: Vec<Stack>) -> Machine {
        Machine { page: CHARACTERS, rows, cursor: 0, scroll: 0, candidates, cand: 0, cand_scroll: 0, sub: 0, question: 0, members: vec![SORA], who: 0, pending: 0 }
    }

    /// The settings page of the character shown: 0x1a for the leader, 0x1b for the others (`FUN_002707f8`).
    pub fn home(&self) -> u16 {
        if self.who == 0 { ROWS } else { FRIEND }
    }

    /// The member id the pages show.
    pub fn member(&self) -> u16 {
        self.members.get(self.who).copied().unwrap_or(SORA)
    }

    fn keep_visible(cursor: usize, scroll: &mut usize, rows: usize) {
        if cursor < *scroll {
            *scroll = cursor;
        } else if cursor >= *scroll + rows {
            *scroll = cursor + 1 - rows;
        }
    }

    /// The rows changed under the cursor (a write): the cursor stays where it is, held to the list.
    pub fn reload(&mut self, rows: Vec<Row>) {
        self.rows = rows;
        self.cursor = self.cursor.min(self.rows.len().saturating_sub(1));
        Self::keep_visible(self.cursor, &mut self.scroll, VISIBLE);
    }

    fn current(&self) -> Option<Row> {
        self.rows.get(self.cursor).copied()
    }

    /// The candidate row the shortcut's item stands on (0 when none): where the sub page starts [inferred: the
    /// original's start row of page 0x1d was not read].
    fn candidate_of(&self, item: u16) -> usize {
        self.candidates.iter().position(|s| s.id == item).map_or(0, |p| p + 1)
    }

    /// One key on the current page. `question` answers `Progress::reload_off_needs_question` for the character's
    /// battle item.
    pub fn press(&mut self, key: Key, question: &dyn Fn(u16) -> bool) -> Vec<Effect> {
        use Effect::{Page, Sound};
        let mut fx = Vec::new();
        if key == Key::Close {
            fx.push(Effect::Close);
            return fx;
        }
        let dir = |k: Key| match k {
            Key::Up { fresh } => Some((-1i8, fresh)),
            Key::Down { fresh } => Some((1, fresh)),
            _ => None,
        };
        let home = self.home();
        let friend = self.who != 0;
        match self.page {
            CHARACTERS => {
                // The list of the party menu's characters (`FUN_002707f8`): the cursor is the character.
                if let Some((d, fresh)) = dir(key) {
                    let new = super::step_cursor(self.who, self.members.len(), d, fresh);
                    if new != self.who {
                        self.who = new;
                        fx.push(Sound(snd::CURSOR));
                        fx.push(Page(CHARACTERS));
                    }
                    return fx;
                }
                match key {
                    Key::Confirm => {
                        // Row 0 opens page 0x1a, every other row page 0x1b.
                        self.page = self.home();
                        self.cursor = 0;
                        self.scroll = 0;
                        fx.push(Sound(snd::CONFIRM));
                        fx.push(Page(self.page));
                    }
                    Key::Cancel => {
                        fx.push(Sound(snd::CANCEL));
                        fx.push(Effect::Leave);
                    }
                    _ => {}
                }
            }
            ROWS | FRIEND => {
                if let Some((d, fresh)) = dir(key) {
                    let new = super::step_cursor(self.cursor, self.rows.len(), d, fresh);
                    if new != self.cursor {
                        self.cursor = new;
                        Self::keep_visible(new, &mut self.scroll, VISIBLE);
                        fx.push(Sound(snd::CURSOR));
                        fx.push(Page(home));
                    }
                    return fx;
                }
                let Some(row) = self.current() else {
                    if key == Key::Cancel {
                        self.page = CHARACTERS;
                        fx.push(Sound(snd::CANCEL));
                        fx.push(Page(CHARACTERS));
                    }
                    return fx;
                };
                match key {
                    Key::Confirm => match row.kind {
                        Kind::Shortcut => {
                            self.cand = self.candidate_of(row.id);
                            self.cand_scroll = 0;
                            Self::keep_visible(self.cand, &mut self.cand_scroll, CAND_VISIBLE);
                            self.page = CANDIDATES;
                            fx.push(Sound(snd::CONFIRM));
                            fx.push(Page(CANDIDATES));
                        }
                        Kind::Spell => {
                            self.sub = row.on as usize;
                            self.page = SPELL;
                            fx.push(Sound(snd::CONFIRM));
                            fx.push(Page(SPELL));
                        }
                        Kind::Item => {
                            // A companion's page lists four states: Auto-Reload off / on, each with the use setting
                            // 0 and 2 (`FUN_00274090`); the leader's lists two.
                            self.sub = if friend { usize::from(item_state(&row)) } else { row.on as usize };
                            self.page = RELOAD;
                            fx.push(Sound(snd::CONFIRM));
                            fx.push(Page(RELOAD));
                        }
                        Kind::Style => {
                            self.sub = usize::from(row.setting).min(STYLES - 1);
                            self.page = STYLE;
                            fx.push(Sound(snd::CONFIRM));
                            fx.push(Page(STYLE));
                        }
                        Kind::Ability => {
                            self.sub = usize::from(row.setting).min(USES - 1);
                            self.page = ABILITY;
                            fx.push(Sound(snd::CONFIRM));
                            fx.push(Page(ABILITY));
                        }
                    },
                    Key::Toggle => match row.kind {
                        Kind::Shortcut => {}
                        Kind::Spell => {
                            fx.push(Sound(snd::CONFIRM));
                            fx.push(Effect::Spell { slot: row.slot, usable: !row.on });
                        }
                        // `FUN_00275398(-1)`: the next style, back to 0 above 5.
                        Kind::Style => {
                            fx.push(Sound(snd::CONFIRM));
                            fx.push(Effect::Style(if usize::from(row.setting) + 1 >= STYLES { 0 } else { row.setting + 1 }));
                        }
                        // `FUN_00275410(-1)`: the entry after the one holding the current value, 0 after the last
                        // or when no entry holds it.
                        Kind::Ability => {
                            fx.push(Sound(snd::CONFIRM));
                            let next = if usize::from(row.setting) + 1 < USES { row.setting + 1 } else { 0 };
                            fx.push(Effect::Ability { id: row.id, value: next });
                        }
                        // `FUN_00274f70` for a companion: the four states in turn, the use setting counting as one
                        // bit when it is not 0; the question test `FUN_00275288` reads the parity of the settings
                        // list's cursor there (the "off" rows of page 0x1f are the even ones) [as read].
                        Kind::Item if friend => {
                            let state = (u8::from(row.setting != 0) << 1) | u8::from(row.on);
                            let next = if state + 1 > 3 { 0 } else { state + 1 };
                            fx.push(Sound(snd::CONFIRM));
                            if self.cursor.is_multiple_of(2) && question(row.id) {
                                self.pending = next;
                                self.question = 0;
                                self.page = QUESTION;
                                fx.push(Page(QUESTION));
                            } else {
                                fx.push(Effect::FriendItem { item: row.id, state: next });
                            }
                        }
                        Kind::Item if row.on && question(row.id) => {
                            self.question = 0;
                            self.page = QUESTION;
                            fx.push(Sound(snd::CONFIRM));
                            fx.push(Page(QUESTION));
                        }
                        Kind::Item => {
                            fx.push(Sound(snd::CONFIRM));
                            fx.push(Effect::Reload { item: row.id, on: !row.on });
                        }
                    },
                    Key::Cancel => {
                        self.page = CHARACTERS;
                        fx.push(Sound(snd::CANCEL));
                        fx.push(Page(CHARACTERS));
                    }
                    // `FUN_00275538`: -10 the next character, -11 the previous one (wrapping); with one character
                    // the switch has nowhere to go. The new character's page starts on its first row.
                    Key::Prev | Key::Next => {
                        let n = self.members.len();
                        if n < 2 {
                            fx.push(Sound(snd::REFUSED));
                        } else {
                            self.who = if key == Key::Next { (self.who + 1) % n } else { (self.who + n - 1) % n };
                            self.cursor = 0;
                            self.scroll = 0;
                            self.page = self.home();
                            fx.push(Sound(snd::CURSOR));
                            fx.push(Page(self.page));
                        }
                    }
                    _ => {}
                }
            }
            CANDIDATES => {
                let len = self.candidates.len() + 1;
                if let Some((d, fresh)) = dir(key) {
                    let new = super::step_cursor(self.cand, len, d, fresh);
                    if new != self.cand {
                        self.cand = new;
                        Self::keep_visible(new, &mut self.cand_scroll, CAND_VISIBLE);
                        fx.push(Sound(snd::CURSOR));
                        fx.push(Page(CANDIDATES));
                    }
                    return fx;
                }
                match key {
                    Key::Confirm => {
                        let item = if self.cand == 0 { 0 } else { self.candidates[self.cand - 1].id };
                        fx.push(Sound(snd::CONFIRM));
                        fx.push(Effect::Shortcut { slot: self.cursor, item });
                        self.page = home;
                        fx.push(Page(home));
                    }
                    Key::Cancel => {
                        self.page = home;
                        fx.push(Sound(snd::CANCEL));
                        fx.push(Page(home));
                    }
                    _ => {}
                }
            }
            SPELL | RELOAD | STYLE | ABILITY => {
                // The row counts: the leader's two-row pages, a companion's four states, the six styles, the three
                // use settings (`FUN_00274158`, `FUN_002741c8`, `FUN_00274090`).
                let len = match self.page {
                    STYLE => STYLES,
                    ABILITY => USES,
                    RELOAD if friend => 4,
                    _ => 2,
                };
                if let Some((d, fresh)) = dir(key) {
                    let new = super::step_cursor(self.sub, len, d, fresh);
                    if new != self.sub {
                        self.sub = new;
                        fx.push(Sound(snd::CURSOR));
                        fx.push(Page(self.page));
                    }
                    return fx;
                }
                let Some(row) = self.current() else { return fx };
                match key {
                    Key::Confirm => {
                        // `FUN_00272018` / `FUN_00274c88`: nothing changes when the row is the state it already has
                        // (kind 2, and kind 1 on the leader's page); the style and ability pages always write.
                        fx.push(Sound(snd::CONFIRM));
                        match self.page {
                            STYLE => fx.push(Effect::Style(self.sub as u8)),
                            ABILITY => fx.push(Effect::Ability { id: row.id, value: self.sub as u8 }),
                            RELOAD if friend => {
                                let want = self.sub as u8;
                                if want != item_state(&row) {
                                    // `FUN_00275288`: the "off" rows (even) ask when a slot waits for a refill.
                                    if self.sub.is_multiple_of(2) && question(row.id) {
                                        self.pending = want;
                                        self.question = 0;
                                        self.page = QUESTION;
                                        fx.push(Page(QUESTION));
                                        return fx;
                                    }
                                    fx.push(Effect::FriendItem { item: row.id, state: want });
                                }
                            }
                            _ => {
                                let want = self.sub == 1;
                                if want != row.on {
                                    if self.page == SPELL {
                                        fx.push(Effect::Spell { slot: row.slot, usable: want });
                                    } else if row.on && question(row.id) {
                                        self.question = 0;
                                        self.page = QUESTION;
                                        fx.push(Page(QUESTION));
                                        return fx;
                                    } else {
                                        fx.push(Effect::Reload { item: row.id, on: want });
                                    }
                                }
                            }
                        }
                        self.page = home;
                        fx.push(Page(home));
                    }
                    Key::Cancel => {
                        self.page = home;
                        fx.push(Sound(snd::CANCEL));
                        fx.push(Page(home));
                    }
                    _ => {}
                }
            }
            QUESTION => {
                if let Some((d, fresh)) = dir(key) {
                    let new = super::step_cursor(self.question, 2, d, fresh);
                    if new != self.question {
                        self.question = new;
                        fx.push(Sound(snd::CURSOR));
                        fx.push(Page(QUESTION));
                    }
                    return fx;
                }
                match key {
                    Key::Confirm if self.question == 1 => {
                        if let Some(row) = self.current() {
                            fx.push(Sound(snd::CONFIRM));
                            if friend {
                                fx.push(Effect::FriendItem { item: row.id, state: self.pending });
                            } else {
                                fx.push(Effect::Reload { item: row.id, on: false });
                            }
                        }
                        self.page = home;
                        fx.push(Page(home));
                    }
                    Key::Confirm | Key::Cancel => {
                        self.page = home;
                        fx.push(Sound(snd::CANCEL));
                        fx.push(Page(home));
                    }
                    _ => {}
                }
            }
            _ => {}
        }
        fx
    }
}

// ---------------------------------------------------------------------------------------------
// The screen.

/// Group ids of `camp.2ld` sequence 1 (`FUN_00270ae0`, the tables at 0x381ca0 .. 0x381d30): the page window
/// (enter 0xe3, hold 0xe4), a row plate and the tag of the first row of a section per kind, and the panel of each
/// kind's sub page (enter, hold: the triples at 0x381cd8, whose middle member is the leave group).
mod group {
    pub const TAB: [usize; 3] = [0x4b, 0x4d, 0x4c];
    pub const HAND: usize = 0x19;
    pub const GLINT: [usize; 3] = [0x1b, 0x1d, 0x1c];
    pub const WINDOW: [usize; 2] = [0xe3, 0xe4];
    /// By row kind (shortcut, spell, item, style, ability): the row plates at 0x381ca0, the section tags at 0x381cb0 and
    /// the sub pages' panels (enter, hold) from the triples at 0x381cd8.
    pub const ROW: [usize; 5] = [0x108, 0x107, 0x106, 0xfa, 0xf8];
    pub const TAG: [usize; 5] = [0x11a, 0x119, 0x118, 0x115, 0x114];
    pub const SUB: [[usize; 2]; 5] = [[0xc6, 0xc7], [0xbd, 0xbe], [0xb4, 0xb5], [0xa2, 0xa3], [0x99, 0x9a]];
    /// The row plate and title strip of the sub page lists (as the Items pages' dialogs).
    pub const LIST_ROW: usize = 0x123;
    pub const TITLE: usize = 0x124;
    /// The Yes / No information window (enter, leave, hold), as the Items pages' question.
    pub const INFO_ASK: [usize; 3] = [9, 0xb, 0xa];
}

mod at {
    pub const ROW_PITCH: f32 = 26.0;
    pub const HAND: [f32; 2] = [-13.0, 0.0];
    pub const GLINT: [f32; 2] = [94.0, -3.0];
    pub const TAB_LABEL: [f32; 2] = [0.0, 1.0];
    pub const TAB_PITCH: f32 = 120.0;
    pub const LABEL: [f32; 2] = [0.0, 1.0];
    pub const COLOR: [u8; 4] = [0xe5, 0xe5, 0xe5, 0x80];
    pub const SELECTED: [u8; 4] = [0xe5, 0xe5, 0xe5, 0x80];
    /// The tag text of a section and the sub page's rows from the window anchor [inferred from the groups' boxes].
    pub const TAG_LABEL: [f32; 2] = [-86.0, 1.0];
    pub const LIST_PITCH: f32 = 28.0;
    /// The setting icon at the right end of a companion's ability and item rows, and the extra text of page 0x1f's
    /// rows [inferred: `FUN_00272b20` and `FUN_002747d0` draw them into lists the port has no layout of].
    pub const MARK: [f32; 2] = [200.0, 1.0];
    pub const EXTRA: [f32; 2] = [128.0, 1.0];
    /// The character list's rows below the first [inferred].
    pub const CHAR_PITCH: f32 = 26.0;
    /// The information window's text and rows from its anchor (255, 126), by eye as on the Items pages.
    pub const INFO_TEXT: [f32; 2] = [-210.0, -10.0];
    pub const INFO_ROWS: [f32; 2] = [-30.0, 98.0];
    pub const INFO_PITCH: f32 = 24.0;
}

/// The executable's tables the pages read (docs/systems/ui/menus-customize.md section 14).
#[derive(Clone, Debug, Default)]
pub struct CustomizeTables {
    /// 0x381d40 and 0x381d48: the row kinds of the leader and of the companions.
    pub kinds: Vec<u8>,
    pub friend_kinds: Vec<u8>,
    /// 0x381cc0: the heading message of each kind (the tag of its first row and the title of its sub page); the
    /// four shortcut row labels are 0x381cd0.
    pub heading: [u32; 5],
    pub shortcut_labels: [u32; 4],
    /// 0x381d20 and 0x381d30: the two rows of the sub pages, index = the flag (0 off, 1 on).
    pub spell_rows: [u32; 2],
    pub item_rows: [u32; 2],
    /// 0x381d28 and 0x381d38: the help line of those rows (`FUN_002743e8`).
    pub spell_help: [u32; 2],
    pub item_help: [u32; 2],
    /// 0x381d58 (6), 0x381d88 (3) and 0x381da0 (2): the rows of the Battle Style page, of an ability's use setting
    /// and of an item's use setting (the extra text of page 0x1f's rows), index = the entry.
    pub styles: Vec<Choice>,
    pub uses: Vec<Choice>,
    pub item_uses: Vec<Choice>,
    /// 0x37ba90: the member names (messages), by member id.
    pub member_names: Vec<u32>,
    /// 0x352ec0 (29 entries of four words): (item or ability id, word 3) -- the index of the use setting byte past
    /// the seven consumables of 0x352eb0 (`FUN_00114348`, `FUN_001144d0`).
    pub setting_ids: Vec<(u32, u32)>,
    /// 0x3809d0: (item type, icon unequipped, icon equipped) until a negative type (`FUN_00255368`).
    pub icons: Vec<(i16, i16, i16)>,
}

/// One row of the choice tables (8 bytes: `+0` message, `+2` help message, `+4` icon, `+5` the value written).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Choice {
    pub msg: u32,
    pub help: u32,
    pub icon: u8,
    pub value: u8,
}

impl CustomizeTables {
    pub fn read(exe: &Exe) -> Option<CustomizeTables> {
        let msg = |va: u32| exe.u16(va).map(|v| u32::from(v & 0x7fff));
        let word = |va: u32| Some(u32::from(exe.u16(va)?) | u32::from(exe.u16(va + 2)?) << 16);
        let choices = |base: u32, n: u32| -> Option<Vec<Choice>> {
            (0..n)
                .map(|k| Some(Choice { msg: msg(base + 8 * k)?, help: msg(base + 8 * k + 2)?, icon: exe.u8(base + 8 * k + 4)?, value: exe.u8(base + 8 * k + 5)? }))
                .collect()
        };
        let mut t = CustomizeTables::default();
        for k in 0..3u32 {
            t.kinds.push(exe.u8(0x0038_1d40 + k)?);
            t.friend_kinds.push(exe.u8(0x0038_1d48 + k)?);
        }
        for k in 0..5u32 {
            t.heading[k as usize] = msg(0x0038_1cc0 + 2 * k)?;
        }
        t.styles = choices(0x0038_1d58, STYLES as u32)?;
        t.uses = choices(0x0038_1d88, USES as u32)?;
        t.item_uses = choices(0x0038_1da0, 2)?;
        t.member_names = (0..16u32).filter_map(|i| msg(0x0037_ba90 + 2 * i)).collect();
        for k in 0..29u32 {
            t.setting_ids.push((word(0x0035_2ec0 + 16 * k + 4)?, word(0x0035_2ec0 + 16 * k + 12)?));
        }
        for k in 0..4u32 {
            t.shortcut_labels[k as usize] = msg(0x0038_1cd0 + 2 * k)?;
        }
        t.spell_rows = [msg(0x0038_1d20)?, msg(0x0038_1d22)?];
        t.item_rows = [msg(0x0038_1d30)?, msg(0x0038_1d32)?];
        t.spell_help = [msg(0x0038_1d28)?, msg(0x0038_1d2a)?];
        t.item_help = [msg(0x0038_1d38)?, msg(0x0038_1d3a)?];
        let mut at = 0x0038_09d0u32;
        loop {
            let ty = exe.u16(at)? as i16;
            if ty < 0 || t.icons.len() > 64 {
                break;
            }
            t.icons.push((ty, exe.u16(at + 2)? as i16, exe.u16(at + 4)? as i16));
            at += 6;
        }
        Some(t)
    }

    /// `FUN_00114348`: the index of an item's or ability's use setting byte (the record's `+0xd5 + k`, so the settings
    /// block's byte `1 + k`): the consumables 1..=7 are 0..=6, the abilities of `FUN_001144d0` follow from 7. An id
    /// the table lacks makes `FUN_001144d0` return -1 and `FUN_00114348` add 7 to it: index 6, the byte of Megalixir
    /// (id 7). Many kind 2 abilities of the item table are not among the 29 entries (for example 82, 137, 195, 220,
    /// 246, 520 on the disc); they share that byte in the original.
    pub fn setting_index(&self, id: u16) -> usize {
        if (1..=7).contains(&id) {
            return usize::from(id) - 1;
        }
        self.setting_ids.iter().find(|e| e.0 == u32::from(id)).map_or(6, |e| 7 + e.1 as usize)
    }

    /// `FUN_00255368`: the icon of a row by item type and flag (0xb when the type is not listed).
    pub fn icon(&self, kind: u8, flag: bool) -> u8 {
        self.icons.iter().find(|t| t.0 == kind as i16).map_or(0xb, |t| if flag { t.2 as u8 } else { t.1 as u8 })
    }
}

struct Ui {
    tabs: [usize; 2],
    char_rows: Vec<usize>,
    /// The setting icon at the right end of a companion's rows, and the extra text of its page-0x1f rows.
    marks: Vec<usize>,
    extras: Vec<usize>,
    hand: usize,
    glint: usize,
    window: usize,
    rows: Vec<usize>,
    tags: Vec<usize>,
    sub_panel: usize,
    sub_title: usize,
    sub_rows: Vec<usize>,
    /// The information window of the question (`FUN_002532d0`) with its text and its two rows.
    info: usize,
    info_text: usize,
    info_rows: Vec<usize>,
    help: usize,
    all: Vec<usize>,
}

#[derive(Resource, Default)]
pub struct CustomizeMenu {
    ready: bool,
    tables: Option<std::sync::Arc<CustomizeTables>>,
    /// Entry 2 of the top list: its name and group sets.
    entry: Option<Entry>,
    machine: Option<Machine>,
    ui: Option<Ui>,
    hidden: Vec<usize>,
    held: (i8, f32),
}

pub struct CustomizePlugin;

impl Plugin for CustomizePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<CustomizeMenu>()
            .add_systems(Startup, load.after(super::item_pages::load))
            .add_systems(PreUpdate, input.after(bevy::input::InputSystems).after(crate::harness::Keys).before(super::input));
    }
}

fn load(disc: Res<Disc>, mut st: ResMut<CustomizeMenu>) {
    let Some(exe) = disc.read_root("SLUS_210.05").and_then(Exe::load) else {
        warn!("menu: the executable is not readable; the Customize pages are off");
        return;
    };
    st.tables = CustomizeTables::read(&exe).map(std::sync::Arc::new);
    st.entry = Entry::read(&exe, ENTRY);
    st.ready = st.tables.is_some() && st.entry.is_some();
    if !st.ready {
        warn!("menu: the executable's Customize tables are not readable; the pages are off");
    }
}

/// What the game gives the rules: the command of an item (`FUN_001a1f78`: a consumable's own, a spell's from the
/// loaded spell row) and the names of commands.
struct Cmds<'a> {
    p: &'a Progress,
    md: &'a crate::magic::MagicData,
}

impl Cmds<'_> {
    /// The spell command of a slot (`FUN_001a4dd0`): the row the slot's level picks.
    fn spell(&self, slot: u8) -> u16 {
        crate::magic_rules::pick_row(&self.md.magc, slot, self.p.spell_level(slot), self.md.world).map_or(0, |r| r.command)
    }

    /// `FUN_001a1f78`.
    fn of_item(&self, id: u16) -> u16 {
        let Some(it) = self.p.items().get(id) else { return 0 };
        match it.kind {
            0 => it.command,
            SPELL_TYPE => self.p.items().magic_items.iter().position(|&m| m == id).map_or(0, |s| self.spell(s as u8)),
            _ => 0,
        }
    }

    /// `FUN_00272fc8`: the first consumable or spell item (table order) whose command is `cmd`.
    fn item_of(&self, order: &[ItemRow], cmd: u16) -> u16 {
        if cmd == 0 {
            return 0;
        }
        order.iter().filter(|r| r.kind == SPELL_TYPE || r.kind == 0).find(|r| self.of_item(r.id) == cmd).map_or(0, |r| r.id)
    }

    /// The command's own name message (`FUN_00272e68`: the command row's `+8`).
    fn name(&self, cmd: u16) -> Option<u32> {
        self.md.cmds.iter().find(|c| c.id == cmd).map(|c| (c.message_id as u32) & 0x7fff)
    }
}

/// The rows of the character's page: the leader's (`rows_of`) or a companion's (`friend_rows_of`).
fn rows_for(st: &ItemsMenu, p: &Progress, c: &Cmds, t: &CustomizeTables, member: u16) -> Vec<Row> {
    if member == SORA { rows_of(st, p, c) } else { friend_rows_of(p, t, member) }
}

/// `FUN_002723c8` for a companion from the record: the style byte, the equipped abilities of kind 2 and the battle
/// items, each with its use setting byte.
fn friend_rows_of(p: &Progress, t: &CustomizeTables, member: u16) -> Vec<Row> {
    let Some(m) = p.member(member) else { return Vec::new() };
    let abilities = m
        .abilities
        .iter()
        .filter(|&&(_, equipped)| equipped)
        .filter_map(|&(id, _)| {
            let it = p.items().get(id)?;
            ((it.stat & 0xff) as u8 == ABILITY_KIND).then(|| (id, p.items().rows.iter().position(|r| r.id == id).unwrap_or(usize::MAX)))
        })
        .collect();
    let setting_of = |id: u16| p.member_setting(member, 1 + t.setting_index(id)).unwrap_or(0);
    build_friend_rows(&FriendInputs { style: m.settings[0], abilities, slots: p.battle_items(member), setting_of: &setting_of })
}

fn rows_of(st: &ItemsMenu, p: &Progress, c: &Cmds) -> Vec<Row> {
    let item_of = |cmd: u16| c.item_of(&st.order, cmd);
    let not_used = |bit: u8| p.spell_switch(bit);
    build_rows(&Inputs {
        shortcuts: p.shortcuts(),
        spell_items: p.items().magic_items,
        count: &|id| p.item_count(id),
        not_used: &not_used,
        slots: p.battle_items(SORA),
        item_of_command: &item_of,
    })
}

fn candidates_of(st: &ItemsMenu, p: &Progress) -> Vec<Stack> {
    let Some(exe) = st.exe.as_ref() else { return Vec::new() };
    candidates(&st.order, exe, &|id| p.item_count(id))
}

/// The writers of a companion's page: `FUN_00275398` (the style, setting byte 0), `FUN_00275410` (an ability's use
/// setting, byte `1 + FUN_00114348(id)`) and `FUN_00274f70` (an item's Auto-Reload and use setting). Returns the log
/// line; `None` for an effect that is not one of these.
pub fn write_friend(p: &mut Progress, t: &CustomizeTables, member: u16, e: &Effect) -> Option<String> {
    match *e {
        Effect::Style(value) => {
            p.set_member_setting(member, 0, value);
            Some(format!("member {member} battle style {value} (setting byte 0 = {:?})", p.member_setting(member, 0)))
        }
        Effect::Ability { id, value } => {
            let k = t.setting_index(id);
            if !p.set_member_setting(member, 1 + k, value) {
                warn!("menu: member {member} ability {id} use setting byte {} is not stored (past the settings block or unknown member)", 1 + k);
            }
            Some(format!("member {member} ability {id} use setting {value} (setting byte {} = {:?})", 1 + k, p.member_setting(member, 1 + k)))
        }
        Effect::FriendItem { item, state } => {
            let (on, setting) = (state & 1 != 0, if state & 2 != 0 { 2 } else { 0 });
            p.set_auto_reload(member, item, on);
            let k = t.setting_index(item);
            if !p.set_member_setting(member, 1 + k, setting) {
                warn!("menu: member {member} item {item} use setting byte {} is not stored (past the settings block or unknown member)", 1 + k);
            }
            Some(format!(
                "member {member} item {item} auto-reload {} use setting {setting} (byte {} = {:?}, slots {:?})",
                if on { "ON" } else { "OFF" },
                1 + k,
                p.member_setting(member, 1 + k),
                p.battle_items(member)
            ))
        }
        _ => None,
    }
}

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
        let (before, after) = (held.1, held.1 + dt);
        *held = (dir, after);
        let n = |t: f32| ((t - 0.4) / 0.1).floor();
        if after >= 0.4 && n(after) != n(before.max(0.4 - 1e-6)) {
            out.push(if dir < 0 { Key::Up { fresh: false } } else { Key::Down { fresh: false } });
        }
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
    if any(&[KeyCode::KeyY]) {
        out.push(Key::Toggle);
    }
    out
}

#[allow(clippy::too_many_arguments)]
fn input(
    mut st: ResMut<CustomizeMenu>,
    items: Res<ItemsMenu>,
    mut menu: ResMut<Menu>,
    mut scene: ResMut<MenuScene>,
    mut keys: ResMut<ButtonInput<KeyCode>>,
    mut se: MessageWriter<PlaySe>,
    mut progress: Option<ResMut<Progress>>,
    magic: Option<Res<crate::magic::MagicData>>,
    party: Query<&crate::party::PartyMember>,
    real: Res<Time<Real>>,
) {
    if !st.ready || !items.ready {
        return;
    }
    if st.machine.is_some() && matches!(menu.phase, Phase::Closing | Phase::Closed) {
        st.machine = None;
        st.hidden.clear();
        st.held = (0, 0.0);
        return;
    }
    let (Some(md), Some(p)) = (magic.as_deref(), progress.as_deref()) else {
        // Without the spell tables the page cannot name a spell command: the row refuses like the others.
        if st.machine.is_none() && menu.page == 0 && menu.current_entry() == Some(ENTRY) && keys.just_pressed(KeyCode::KeyJ) {
            crate::menu::menu_sound::play(&mut se, snd::REFUSED);
            keys.reset_all();
        }
        return;
    };
    if st.machine.is_none() {
        let on_row = menu.page == 0 && menu.current_entry() == Some(ENTRY);
        if !(on_row && keys.just_pressed(KeyCode::KeyJ)) {
            return;
        }
        let cmds = Cmds { p, md };
        let rows = rows_of(&items, p, &cmds);
        let cands = candidates_of(&items, p);
        let mut m = Machine::new(rows, cands);
        // The party menu's characters: the leader, then the members standing in the field in slot order.
        m.members = std::iter::once(SORA).chain(super::party_page::roster(party.iter()).iter().map(|r| r.id)).collect();
        st.machine = Some(m);
        st.held = (0, 0.0);
        enter(&mut st, &mut menu, &mut scene);
        crate::menu::menu_sound::play(&mut se, snd::CONFIRM);
        keys.reset_all();
        redraw(&mut st, &items, &mut scene, p, &cmds);
        return;
    }
    let mut held = st.held;
    let pressed = keys_of(&keys, &mut held, real.delta_secs());
    st.held = held;
    let mut close = false;
    let tables = st.tables.clone().unwrap_or_default(); // an Arc: the per-frame copy is a pointer bump
    for key in pressed {
        let Some(p) = progress.as_deref() else { break };
        let member = st.machine.as_ref().map_or(SORA, |m| m.member());
        let question = |id: u16| p.reload_off_needs_question(member, id);
        let fx = st.machine.as_mut().map(|m| m.press(key, &question)).unwrap_or_default();
        for e in fx {
            match e {
                Effect::Sound(n) => {
                    crate::menu::menu_sound::play(&mut se, n);
                }
                Effect::Page(page) => {
                    menu.page = page;
                    info!("menu: page {page:#x}");
                }
                Effect::Shortcut { slot, item } => {
                    if let Some(p) = progress.as_deref_mut() {
                        let cmds = Cmds { p, md };
                        let command = cmds.of_item(item);
                        let name = cmds.name(command).and_then(|m| scene.text.as_ref()?.messages.as_ref()?.text(m)).unwrap_or_default();
                        p.set_shortcut(slot, command);
                        info!("menu: shortcut {slot} = command {command} ({name}), word {:#06x}, shortcuts {:?}", p.shortcut(slot), p.shortcuts());
                    }
                }
                Effect::Spell { slot, usable } => {
                    if let Some(p) = progress.as_deref_mut() {
                        p.set_spell_switch(slot, !usable);
                        info!("menu: spell slot {slot} {} (switch word {:#x})", if usable { "MP Charge" } else { "Don't MP Charge" }, p.spell_switch_word());
                    }
                }
                Effect::Reload { item, on } => {
                    if let Some(p) = progress.as_deref_mut() {
                        p.set_auto_reload(SORA, item, on);
                        info!("menu: auto-reload item {item} {} (slots {:?})", if on { "ON" } else { "OFF" }, p.battle_items(SORA));
                    }
                }
                Effect::Style(_) | Effect::Ability { .. } | Effect::FriendItem { .. } => {
                    if let Some(p) = progress.as_deref_mut()
                        && let Some(line) = write_friend(p, &tables, member, &e)
                    {
                        info!("menu: {line}");
                    }
                }
                Effect::Leave => leave(&mut st, &mut menu, &mut scene),
                Effect::Close => {
                    leave(&mut st, &mut menu, &mut scene);
                    close = true;
                }
            }
            if st.machine.is_none() {
                break;
            }
        }
        // A write changed the rows: rebuild them under the cursor.
        if let (Some(p), true) = (progress.as_deref(), st.machine.is_some()) {
            let cmds = Cmds { p, md };
            let member = st.machine.as_ref().map_or(SORA, |m| m.member());
            let rows = rows_for(&items, p, &cmds, &tables, member);
            let cands = candidates_of(&items, p);
            if let Some(m) = st.machine.as_mut() {
                m.candidates = cands;
                m.reload(rows);
            }
        }
        if st.machine.is_none() {
            break;
        }
    }
    if close {
        return;
    }
    if let (Some(p), true) = (progress.as_deref(), st.machine.is_some()) {
        let cmds = Cmds { p, md };
        redraw(&mut st, &items, &mut scene, p, &cmds);
    }
    keys.reset_all();
}

fn ensure_ui(st: &mut CustomizeMenu, scene: &mut MenuScene) {
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
    let tabs = [mk(scene, S), mk(scene, S)];
    let char_rows: Vec<usize> = (0..3).map(|_| mk(scene, S)).collect();
    let window = mk(scene, S);
    let rows: Vec<usize> = (0..VISIBLE).map(|_| mk(scene, S)).collect();
    let tags: Vec<usize> = (0..VISIBLE).map(|_| mk(scene, S)).collect();
    let sub_panel = mk(scene, S);
    let sub_title = mk(scene, S);
    let sub_rows: Vec<usize> = (0..CAND_VISIBLE).map(|_| mk(scene, S)).collect();
    let marks: Vec<usize> = (0..VISIBLE).map(|_| mk(scene, None)).collect();
    let extras: Vec<usize> = (0..4).map(|_| mk(scene, None)).collect();
    let info = mk(scene, S);
    let info_text = mk(scene, None);
    let info_rows: Vec<usize> = (0..2).map(|_| mk(scene, S)).collect();
    // The hand and the glint draw over the window (later nodes draw on top).
    let hand = mk(scene, S);
    let glint = mk(scene, S);
    let help = mk(scene, None);
    for (&r, &t) in rows.iter().zip(&tags) {
        scene.nodes[r].parent = Some(window);
        scene.nodes[t].parent = Some(r);
    }
    for (&r, &m) in rows.iter().zip(&marks) {
        scene.nodes[m].parent = Some(r);
    }
    for (&r, &e) in sub_rows.iter().zip(&extras) {
        scene.nodes[e].parent = Some(r);
    }
    scene.nodes[sub_title].parent = Some(sub_panel);
    for &r in &sub_rows {
        scene.nodes[r].parent = Some(sub_panel);
    }
    scene.nodes[info_text].parent = Some(info);
    for &r in &info_rows {
        scene.nodes[r].parent = Some(info);
    }
    st.ui = Some(Ui { tabs, char_rows, marks, extras, hand, glint, window, rows, tags, sub_panel, sub_title, sub_rows, info, info_text, info_rows, help, all });
}

fn enter(st: &mut CustomizeMenu, menu: &mut Menu, scene: &mut MenuScene) {
    ensure_ui(st, scene);
    st.hidden = menu.page0_nodes().into_iter().filter(|&i| scene.nodes[i].visible).collect();
    for &i in &st.hidden {
        scene.nodes[i].visible = false;
    }
    menu.page = CHARACTERS;
    info!("menu: page {CHARACTERS:#x}");
}

fn leave(st: &mut CustomizeMenu, menu: &mut Menu, scene: &mut MenuScene) {
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

/// A row's text: the state-dependent icon (`FUN_00255478`: the item type and the flag through the table at
/// 0x3809d0) and the name (`FUN_00272e68`: the item's own name for items and abilities, the command's name for the
/// shortcut and spell rows); the Battle Style row shows the style's name (`FUN_00272f80`).
fn row_tokens(text: &crate::menu_text::MenuText, t: &CustomizeTables, items: &ItemsMenu, c: &Cmds, r: &Row) -> Vec<Token> {
    if r.kind == Kind::Style {
        return t.styles.iter().find(|s| s.value == r.setting).map_or_else(Vec::new, |s| tokens_of(text, s.msg));
    }
    if r.id == 0 {
        // The empty shortcut: "---" (message 0x8448).
        return tokens_of(text, 0x448);
    }
    let kind = items.rows.get(&r.id).map_or(0, |i| i.kind);
    // `FUN_00272b20` hands the icon lookup the flag for items and its negation for spells: a spell that is on shows
    // the unequipped icon (8, "MP Charge"), one that is off the equipped one (18, "Don't MP Charge").
    let flag = if kind == SPELL_TYPE { !r.on } else { r.on };
    let mut v = vec![Token::Icon(t.icon(kind, flag))];
    let name = match r.kind {
        Kind::Item | Kind::Ability => items.rows.get(&r.id).map(|i| u32::from(i.name)),
        _ => c.name(c.of_item(r.id)).or_else(|| items.rows.get(&r.id).map(|i| u32::from(i.name))),
    };
    if let Some(n) = name {
        v.extend(tokens_of(text, n));
    }
    v
}

/// The message `FUN_002743e8` appends to the description of a row of page 0x1b: the help message (`+2`) of the entry
/// of the kind's choice table that holds the row's setting byte (`FUN_002748d8(kind, byte)`). The Battle Style row
/// shows the style's message alone; a value no entry holds makes the original read a halfword at address 2, which the
/// port leaves out.
pub fn appended_help(t: &CustomizeTables, r: &Row) -> Option<u32> {
    let table = match r.kind {
        Kind::Ability => &t.uses,
        Kind::Item => &t.item_uses,
        _ => return None,
    };
    table.iter().find(|c| c.value == r.setting).map(|c| c.help)
}

/// The help of the character list (`FUN_002743e8`, page 0x19): message 0x381c6c (0x3b89) on the first row, 0x381c70
/// (0x3b8a) on the others.
pub fn characters_help(cursor: usize) -> u32 {
    if cursor == 0 { 0x3b89 } else { 0x3b8a }
}

/// The joined help buffer: the first message's tokens, the second's right after them, one end command.
fn joined_tokens(mut first: Vec<Token>, second: Vec<Token>) -> Vec<Token> {
    first.retain(|t| *t != Token::End);
    first.extend(second.into_iter().filter(|t| *t != Token::End));
    first.push(Token::End);
    first
}

/// `FUN_002743e8` ends by copying the first message without its end byte and the second after it into one buffer.
fn set_help_joined(scene: &mut MenuScene, text: &crate::menu_text::MenuText, help: usize, ids: (Option<u32>, Option<u32>)) {
    let (first, second) = match ids {
        (None, Some(b)) => (Some(b), None),
        other => other,
    };
    let Some(first) = first else { return set_help(scene, text, help, None) };
    let Some(second) = second else { return set_help(scene, text, help, Some(first)) };
    set_help(scene, text, help, Some(first));
    let tokens = joined_tokens(tokens_of(text, first), tokens_of(text, second));
    if let Some(l) = scene.nodes[help].label.as_mut() {
        l.tokens = tokens;
    }
}

/// The icon at the right end of a companion's ability or item row: the entry of the kind's table that holds the row's
/// setting byte (`FUN_00272b20`: `FUN_002748d8(kind, setting)` and its `+4`).
fn mark_of(t: &CustomizeTables, r: &Row) -> Option<u8> {
    let table = match r.kind {
        Kind::Ability => &t.uses,
        Kind::Item => &t.item_uses,
        _ => return None,
    };
    table.iter().find(|c| c.value == r.setting).map(|c| c.icon)
}

/// The name of a character in the list and on the header tab (the member names at 0x37ba90).
fn name_tokens(text: &crate::menu_text::MenuText, t: &CustomizeTables, id: u16) -> Vec<Token> {
    match t.member_names.get(usize::from(id)).copied() {
        Some(n) if n != 0 => tokens_of(text, n),
        _ => text.literal(["?", "Sora", "Donald", "Goofy"].get(usize::from(id)).copied().unwrap_or("?"), Style::default(), [0.0; 2]).tokens,
    }
}

fn redraw(st: &mut CustomizeMenu, items: &ItemsMenu, scene: &mut MenuScene, p: &Progress, c: &Cmds) {
    let Some(m) = st.machine.clone() else { return };
    let (Some(ui), Some(t)) = (st.ui.as_ref(), st.tables.clone()) else { return };
    let (tabs, char_rows, hand, glint, window, rows, tags, sub_panel, sub_title, sub_rows, help) =
        (ui.tabs, ui.char_rows.clone(), ui.hand, ui.glint, ui.window, ui.rows.clone(), ui.tags.clone(), ui.sub_panel, ui.sub_title, ui.sub_rows.clone(), ui.help);
    let (info, info_text, info_rows) = (ui.info, ui.info_text, ui.info_rows.clone());
    let (marks, extras) = (ui.marks.clone(), ui.extras.clone());
    let all = ui.all.clone();
    let entry = st.entry.clone().unwrap_or_else(|| Entry { idle: [0; 4], selected: [0; 4], name: 0, help: 0, need: vec![], deny: vec![] });
    let Some(text) = scene.text.take() else { return };
    let page = m.page;
    let home = m.home();
    let friend = m.who != 0;
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

    // The header tabs: Customize, then the character.
    for (k, &tab) in tabs.iter().enumerate() {
        let prev = was_of(tab);
        scene.nodes[tab].pos = [at::TAB_PITCH * k as f32, 0.0];
        set(scene, tab, group::TAB[0], Some(group::TAB[2]), prev);
        let tokens = if k == 0 { tokens_of(&text, entry.name) } else { name_tokens(&text, &t, m.member()) };
        scene.nodes[tab].label_anchor = Some(at::TAB_LABEL);
        scene.nodes[tab].label = Some(label(&text, tokens, 1, 15.0, at::COLOR, 0, [0.0; 2]));
    }

    if page == CHARACTERS {
        // The character list: a row per member of the party menu; the cursor's row is drawn as a selected row of
        // page 0 (the layout of the rows below the first is [inferred]).
        for (k, &id) in m.members.iter().enumerate().take(char_rows.len()) {
            let row = char_rows[k];
            let selected = k == m.who;
            let groups = if selected { &entry.selected } else { &entry.idle };
            scene.nodes[row].off = [0.0, at::CHAR_PITCH * k as f32];
            set(scene, row, usize::from(groups[0]), Some(usize::from(groups[2])), was_of(row));
            scene.nodes[row].label_anchor = Some([-7.0, 1.5]);
            scene.nodes[row].label = Some(label(&text, name_tokens(&text, &t, id), 1, 15.0, if selected { at::SELECTED } else { at::COLOR }, 0, [0.0; 2]));
        }
        let cur = char_rows[m.who.min(char_rows.len() - 1)];
        scene.nodes[hand].parent = Some(cur);
        scene.nodes[hand].off = at::HAND;
        set(scene, hand, group::HAND, None, was_of(hand));
        scene.nodes[glint].parent = Some(cur);
        scene.nodes[glint].off = at::GLINT;
        set(scene, glint, group::GLINT[0], Some(group::GLINT[2]), was_of(glint));
        set_help(scene, &text, help, Some(characters_help(m.who)));
        scene.text = Some(text);
        return;
    }

    // The settings window and its rows (`FUN_00270ae0`).
    set(scene, window, group::WINDOW[0], Some(group::WINDOW[1]), was_of(window));
    let mut prev_kind: Option<Kind> = (m.scroll > 0).then(|| m.rows[m.scroll - 1].kind);
    for k in 0..VISIBLE {
        let idx = m.scroll + k;
        let Some(r) = m.rows.get(idx) else { break };
        let ki = r.kind.index();
        let selected = idx == m.cursor && page == home;
        scene.nodes[rows[k]].off = [0.0, at::ROW_PITCH * k as f32];
        set(scene, rows[k], group::ROW[ki], None, was_of(rows[k]));
        scene.nodes[rows[k]].label_anchor = Some(at::LABEL);
        scene.nodes[rows[k]].label = Some(label(&text, row_tokens(&text, &t, items, c, r), 1, 15.0, if selected { at::SELECTED } else { at::COLOR }, 0, [0.0; 2]));
        // The tag of a section: each shortcut row has its own (a button), the other kinds only the first row.
        if r.kind == Kind::Shortcut || prev_kind != Some(r.kind) {
            set(scene, tags[k], group::TAG[ki], None, was_of(tags[k]));
            let msg = if r.kind == Kind::Shortcut { t.shortcut_labels[idx.min(3)] } else { t.heading[ki] };
            scene.nodes[tags[k]].label_anchor = Some(at::TAG_LABEL);
            scene.nodes[tags[k]].label = Some(label(&text, tokens_of(&text, msg), 1, 13.0, at::COLOR, 0, [0.0; 2]));
        }
        // A companion's ability and item rows carry the icon of their use setting (`FUN_00272b20`).
        if friend && let Some(icon) = mark_of(&t, r) {
            let n = &mut scene.nodes[marks[k]];
            n.visible = true;
            n.off = [0.0, 0.0];
            n.label_anchor = Some(at::MARK);
            n.label = Some(label(&text, vec![Token::Icon(icon)], 1, 15.0, at::COLOR, 0, [0.0; 2]));
        }
        prev_kind = Some(r.kind);
    }
    if let Some(&sel) = rows.get(m.cursor.saturating_sub(m.scroll)) {
        scene.nodes[hand].parent = Some(sel);
        scene.nodes[hand].off = at::HAND;
        set(scene, hand, group::HAND, None, page != home && was_of(hand));
    }
    let row = m.rows.get(m.cursor).copied();
    // The help line (`FUN_002743e8`): the highlighted row's item description (the style's own for the Battle Style
    // row), or the choice's; a companion's page appends the choice's message to the description.
    let help_id = match page {
        CANDIDATES => (m.cand.checked_sub(1).and_then(|i| m.candidates.get(i)).and_then(|s| items.rows.get(&s.id)).map(|r| u32::from(r.description)).or(Some(0x1df7)), None),
        STYLE => (t.styles.get(m.sub).map(|c| c.help), None),
        ABILITY => (t.uses.get(m.sub).map(|c| c.help), None),
        SPELL => (Some(t.spell_help[m.sub.min(1)]), None),
        RELOAD => (Some(t.item_help[m.sub % 2]), friend.then(|| t.item_uses.get(m.sub / 2).map(|c| c.help)).flatten()),
        pg if pg == home => row.map_or((None, None), |r| {
            let description = (r.kind != Kind::Style && r.id != 0).then(|| items.rows.get(&r.id).map(|i| u32::from(i.description))).flatten();
            (if r.kind == Kind::Style { t.styles.iter().find(|c| c.value == r.setting).map(|c| c.help) } else { description }, if pg == FRIEND { appended_help(&t, &r) } else { None })
        }),
        _ => (None, None),
    };
    set_help_joined(scene, &text, help, help_id);

    // The question: the information window with the message and the No and Yes rows (`FUN_00271688`).
    if page == QUESTION {
        set(scene, info, group::INFO_ASK[0], Some(group::INFO_ASK[2]), was_of(info));
        let n = &mut scene.nodes[info_text];
        n.visible = true;
        n.label_anchor = Some(at::INFO_TEXT);
        n.label = Some(label(&text, tokens_of(&text, 0x46a), 1, 15.0, at::COLOR, 0, [0.0; 2]));
        for (k, &msg) in [0x1fd1u32, 0x1fd0].iter().enumerate() {
            let r = info_rows[k];
            set(scene, r, group::LIST_ROW, None, was_of(r));
            scene.nodes[r].off = [at::INFO_ROWS[0], at::INFO_ROWS[1] + at::INFO_PITCH * k as f32];
            scene.nodes[r].label_anchor = Some(at::LABEL);
            scene.nodes[r].label = Some(label(&text, tokens_of(&text, msg), 1, 15.0, if k == m.question { at::SELECTED } else { at::COLOR }, 0, [0.0; 2]));
            if k == m.question {
                scene.nodes[hand].parent = Some(r);
                scene.nodes[hand].off = at::HAND;
                set(scene, hand, group::HAND, None, false);
            }
        }
        set_help(scene, &text, help, None);
        scene.text = Some(text);
        return;
    }

    // The sub pages: the kind's panel with its title and the rows (`FUN_00273d30`).
    if matches!(page, CANDIDATES | SPELL | RELOAD | STYLE | ABILITY) {
        let ki = match page {
            CANDIDATES => 0,
            SPELL => 1,
            RELOAD => 2,
            STYLE => 3,
            _ => 4,
        };
        set(scene, sub_panel, group::SUB[ki][0], Some(group::SUB[ki][1]), was_of(sub_panel));
        set(scene, sub_title, group::TITLE, None, was_of(sub_title));
        let title = if page == CANDIDATES { t.shortcut_labels[m.cursor.min(3)] } else { t.heading[ki] };
        scene.nodes[sub_title].label_anchor = Some(at::LABEL);
        scene.nodes[sub_title].label = Some(label(&text, tokens_of(&text, title), 1, 15.0, at::COLOR, 0, [0.0; 2]));
        // (tokens, selected, extra text at the row's right)
        let mut lines: Vec<(Vec<Token>, bool, Option<Vec<Token>>)> = Vec::new();
        let cursor;
        let mut first = 0;
        match page {
            CANDIDATES => {
                // Row 0 removes the shortcut, then the owned consumables and spells.
                lines.push((tokens_of(&text, 0x1df7), m.cand == 0, None));
                for (i, s) in m.candidates.iter().enumerate() {
                    let name = items.rows.get(&s.id).map_or(0, |r| r.name);
                    let icon = items.rows.get(&s.id).map_or(0, |r| r.icon);
                    let mut tk = vec![Token::Icon(icon)];
                    tk.extend(tokens_of(&text, c.name(c.of_item(s.id)).unwrap_or(u32::from(name))));
                    lines.push((tk, m.cand == i + 1, None));
                }
                cursor = m.cand;
                first = m.cand_scroll;
            }
            SPELL => {
                lines = t.spell_rows.iter().enumerate().map(|(i, &id)| (tokens_of(&text, id), i == m.sub, None)).collect();
                cursor = m.sub;
            }
            RELOAD => {
                // The leader's two rows; a companion's four: Auto-Reload off / on for the use setting 0, then for 2,
                // each followed by the setting's own text (`FUN_00274090`, `FUN_002747d0`).
                let n = if friend { 4 } else { 2 };
                lines = (0..n)
                    .map(|i| {
                        let extra = (friend).then(|| t.item_uses.get(i / 2).map(|c| tokens_of(&text, c.msg))).flatten();
                        (tokens_of(&text, t.item_rows[i % 2]), i == m.sub, extra)
                    })
                    .collect();
                cursor = m.sub;
            }
            STYLE => {
                lines = t.styles.iter().enumerate().map(|(i, c)| (tokens_of(&text, c.msg), i == m.sub, None)).collect();
                cursor = m.sub;
            }
            _ => {
                lines = t.uses.iter().enumerate().map(|(i, c)| (tokens_of(&text, c.msg), i == m.sub, None)).collect();
                cursor = m.sub;
            }
        }
        let shown: Vec<(Vec<Token>, bool, Option<Vec<Token>>)> = lines.into_iter().skip(first).take(CAND_VISIBLE).collect();
        for (k, (tokens, sel, extra)) in shown.iter().enumerate() {
            let r = sub_rows[k];
            // The title strip hangs above the panel's anchor, the rows start at it (`FUN_00273d30`, the group boxes).
            scene.nodes[r].off = [0.0, at::LIST_PITCH * k as f32];
            set(scene, r, group::LIST_ROW, None, was_of(r));
            scene.nodes[r].label_anchor = Some(at::LABEL);
            scene.nodes[r].label = Some(label(&text, tokens.clone(), 1, 15.0, if *sel { at::SELECTED } else { at::COLOR }, 0, [0.0; 2]));
            if let (Some(extra), Some(&e)) = (extra, extras.get(k)) {
                let n = &mut scene.nodes[e];
                n.visible = true;
                n.off = [0.0, 0.0];
                n.label_anchor = Some(at::EXTRA);
                n.label = Some(label(&text, extra.clone(), 1, 15.0, if *sel { at::SELECTED } else { at::COLOR }, 0, [0.0; 2]));
            }
        }
        if let Some(&sel) = sub_rows.get(cursor.saturating_sub(first)) {
            scene.nodes[hand].parent = Some(sel);
            scene.nodes[hand].off = at::HAND;
            set(scene, hand, group::HAND, None, false);
        }
    }
    let _ = p;
    scene.text = Some(text);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rows(shortcuts: [u16; 4], owned: &[u16], not_used: &[u8], slots: Vec<(u16, u16)>) -> Vec<Row> {
        // Item 700 + slot is the item of each spell slot; command 9xx maps back to item 7xx or 1.
        let item_of = |cmd: u16| match cmd {
            23 => 1,
            949 => 700,
            951 => 702,
            _ => 0,
        };
        build_rows(&Inputs {
            shortcuts,
            spell_items: [700, 701, 702, 703, 0, 705],
            count: &|id| owned.contains(&id) as u8,
            not_used: &|bit| not_used.contains(&bit),
            slots,
            item_of_command: &item_of,
        })
    }

    #[test]
    fn the_rows_are_four_shortcuts_the_owned_spells_in_slot_order_and_the_distinct_battle_items() {
        // Fire (700) and Thunder (702) are owned, Fire is switched off; Blizzard is not owned.
        let r = rows([23, 0, 951, 0], &[700, 702], &[0], vec![(1, 1), (0, 3), (1, 0), (4, 0), (0, 0)]);
        let kinds: Vec<Kind> = r.iter().map(|r| r.kind).collect();
        assert_eq!(kinds, vec![Kind::Shortcut; 4].into_iter().chain([Kind::Spell, Kind::Spell, Kind::Item, Kind::Item, Kind::Item]).collect::<Vec<_>>());
        // The shortcut rows hold the item of their command (0 for an empty slot).
        assert_eq!(r[..4].iter().map(|r| r.id).collect::<Vec<_>>(), vec![1, 0, 702, 0]);
        // Spell rows: slot order, the flag is "not switched off" (`FUN_001a4bb8`: a clear bit is 1).
        assert_eq!((r[4].id, r[4].slot, r[4].on), (700, 0, false));
        assert_eq!((r[5].id, r[5].slot, r[5].on), (702, 2, true));
        // Battle items: slot 0 carries item 1 and reserves it (on); slot 1 is empty and reserves 3 (on); slot 2
        // carries 1 again (skipped); slot 3 carries 4 without a reservation (off); slot 4 is free.
        assert_eq!(r[6..].iter().map(|r| (r.id, r.on)).collect::<Vec<_>>(), vec![(1, true), (3, true), (4, false)]);
    }

    #[test]
    fn a_shortcut_row_shows_the_state_of_the_row_with_its_item() {
        // Shortcut 0 holds item 1 (Auto-Reload on in the item rows); shortcut 2 holds Thunder (switched off).
        let r = rows([23, 0, 951, 0], &[702], &[2], vec![(1, 1)]);
        assert!(r[0].on, "the Auto-Reload flag of item 1");
        assert!(!r[2].on, "Thunder is switched off");
        assert!(!r[1].on && !r[3].on, "empty shortcuts keep no state");
        // A shortcut whose item has no other row keeps its own flag (off).
        let r = rows([23, 0, 0, 0], &[], &[], vec![]);
        assert!(!r[0].on);
    }

    fn machine() -> Machine {
        let r = rows([0, 0, 0, 0], &[700], &[], vec![(1, 1), (4, 0)]);
        let cands = vec![Stack { id: 700, count: 1 }, Stack { id: 1, count: 5 }];
        Machine::new(r, cands)
    }

    fn none(_: u16) -> bool {
        false
    }

    #[test]
    fn the_character_list_opens_the_settings_and_cancel_leaves() {
        let mut m = machine();
        assert_eq!(m.press(Key::Confirm, &none), vec![Effect::Sound(snd::CONFIRM), Effect::Page(ROWS)]);
        assert_eq!(m.press(Key::Cancel, &none), vec![Effect::Sound(snd::CANCEL), Effect::Page(CHARACTERS)]);
        assert_eq!(m.press(Key::Cancel, &none), vec![Effect::Sound(snd::CANCEL), Effect::Leave]);
        assert_eq!(m.press(Key::Close, &none), vec![Effect::Close]);
    }

    #[test]
    fn a_shortcut_row_opens_the_candidates_and_a_pick_stores_that_item() {
        let mut m = machine();
        m.press(Key::Confirm, &none);
        // Row 0 is a shortcut: the candidates start on the removal (the slot is empty).
        assert_eq!(m.press(Key::Confirm, &none), vec![Effect::Sound(snd::CONFIRM), Effect::Page(CANDIDATES)]);
        assert_eq!(m.cand, 0);
        m.press(Key::Down { fresh: true }, &none);
        m.press(Key::Down { fresh: true }, &none);
        assert_eq!(m.cand, 2);
        assert_eq!(m.press(Key::Confirm, &none), vec![Effect::Sound(snd::CONFIRM), Effect::Shortcut { slot: 0, item: 1 }, Effect::Page(ROWS)]);
        // The removal row writes 0; cancel changes nothing.
        m.press(Key::Confirm, &none);
        assert_eq!(m.cand, 0);
        assert_eq!(m.press(Key::Confirm, &none), vec![Effect::Sound(snd::CONFIRM), Effect::Shortcut { slot: 0, item: 0 }, Effect::Page(ROWS)]);
        m.press(Key::Confirm, &none);
        assert_eq!(m.press(Key::Cancel, &none), vec![Effect::Sound(snd::CANCEL), Effect::Page(ROWS)]);
    }

    /// Moves the settings cursor to the first row of a kind.
    fn goto(m: &mut Machine, kind: Kind) {
        m.page = ROWS;
        m.cursor = m.rows.iter().position(|r| r.kind == kind).unwrap();
    }

    #[test]
    fn a_spell_row_switches_with_the_sub_page_or_the_quick_key() {
        let mut m = machine();
        goto(&mut m, Kind::Spell);
        assert!(m.rows[m.cursor].on);
        // The quick key: usable -> not usable, at once.
        assert_eq!(m.press(Key::Toggle, &none), vec![Effect::Sound(snd::CONFIRM), Effect::Spell { slot: 0, usable: false }]);
        // The sub page starts on the row's state (index 1 = MP Charge); the other row writes the switch.
        assert_eq!(m.press(Key::Confirm, &none), vec![Effect::Sound(snd::CONFIRM), Effect::Page(SPELL)]);
        assert_eq!(m.sub, 1);
        m.press(Key::Up { fresh: true }, &none);
        assert_eq!(m.press(Key::Confirm, &none), vec![Effect::Sound(snd::CONFIRM), Effect::Spell { slot: 0, usable: false }, Effect::Page(ROWS)]);
        // The row it already has changes nothing.
        m.press(Key::Confirm, &none);
        assert_eq!(m.press(Key::Confirm, &none), vec![Effect::Sound(snd::CONFIRM), Effect::Page(ROWS)]);
        // Cancel on the sub page leaves the switch alone.
        m.press(Key::Confirm, &none);
        assert_eq!(m.press(Key::Cancel, &none), vec![Effect::Sound(snd::CANCEL), Effect::Page(ROWS)]);
    }

    #[test]
    fn auto_reload_asks_before_going_off_only_when_a_slot_waits_for_a_refill() {
        let mut m = machine();
        goto(&mut m, Kind::Item);
        let row = m.rows[m.cursor];
        assert_eq!((row.id, row.on), (1, true));
        // No waiting slot: the quick key switches it off at once.
        assert_eq!(m.press(Key::Toggle, &none), vec![Effect::Sound(snd::CONFIRM), Effect::Reload { item: 1, on: false }]);
        // A waiting slot: the question first; No and cancel leave it, Yes writes.
        let asks = |id: u16| id == 1;
        assert_eq!(m.press(Key::Toggle, &asks), vec![Effect::Sound(snd::CONFIRM), Effect::Page(QUESTION)]);
        assert_eq!(m.question, 0, "the question starts on No");
        assert_eq!(m.press(Key::Confirm, &asks), vec![Effect::Sound(snd::CANCEL), Effect::Page(ROWS)]);
        m.press(Key::Toggle, &asks);
        m.press(Key::Down { fresh: true }, &asks);
        assert_eq!(m.press(Key::Confirm, &asks), vec![Effect::Sound(snd::CONFIRM), Effect::Reload { item: 1, on: false }, Effect::Page(ROWS)]);
        // Through the sub page: row 0 (OFF) of an item that is on goes through the same question.
        goto(&mut m, Kind::Item);
        assert_eq!(m.press(Key::Confirm, &asks), vec![Effect::Sound(snd::CONFIRM), Effect::Page(RELOAD)]);
        assert_eq!(m.sub, 1);
        m.press(Key::Up { fresh: true }, &asks);
        assert_eq!(m.press(Key::Confirm, &asks), vec![Effect::Sound(snd::CONFIRM), Effect::Page(QUESTION)]);
        assert_eq!(m.press(Key::Cancel, &asks), vec![Effect::Sound(snd::CANCEL), Effect::Page(ROWS)]);
        // An item that is off turns on without a question.
        let off = vec![Row::new(Kind::Item, 4, false, 0)];
        let mut n = Machine::new(off, vec![]);
        n.page = ROWS;
        assert_eq!(n.press(Key::Toggle, &asks), vec![Effect::Sound(snd::CONFIRM), Effect::Reload { item: 4, on: true }]);
    }

    #[test]
    fn the_quick_key_does_nothing_on_a_shortcut_row_and_one_character_has_no_switch() {
        let mut m = machine();
        goto(&mut m, Kind::Shortcut);
        assert!(m.press(Key::Toggle, &none).is_empty());
        assert_eq!(m.press(Key::Next, &none), vec![Effect::Sound(snd::REFUSED)]);
        // Cursor moves wrap on a new press only and scroll the nine-row window.
        m.page = ROWS;
        m.cursor = 0;
        assert!(m.press(Key::Up { fresh: false }, &none).is_empty());
        m.press(Key::Up { fresh: true }, &none);
        assert_eq!(m.cursor, m.rows.len() - 1);
        let many: Vec<Row> = (0..20).map(|i| Row::new(Kind::Item, i + 1, false, 0)).collect();
        let mut big = Machine::new(many, vec![]);
        big.page = ROWS;
        for _ in 0..12 {
            big.press(Key::Down { fresh: true }, &none);
        }
        assert_eq!((big.cursor, big.scroll), (12, 4));
    }

    #[test]
    fn the_candidates_list_is_the_owned_spells_then_the_consumables() {
        let order = vec![
            ItemRow { id: 1, kind: 0, amount: 0, name: 0, description: 0, sell: 1, icon: 0 },
            ItemRow { id: 2, kind: 2, amount: 0, name: 0, description: 0, sell: 0, icon: 0 },
            ItemRow { id: 700, kind: SPELL_TYPE, amount: 0, name: 0, description: 0, sell: 0, icon: 0 },
            ItemRow { id: 3, kind: 0, amount: 0, name: 0, description: 0, sell: 1, icon: 0 },
            ItemRow { id: 701, kind: SPELL_TYPE, amount: 0, name: 0, description: 0, sell: 0, icon: 0 },
            ItemRow { id: 800, kind: 0x13, amount: 0, name: 0, description: 0, sell: 0, icon: 0 },
        ];
        let exe = ExeTables::default();
        let own = |id: u16| match id {
            1 | 3 => 2,
            2 | 800 => 1,
            700 | 701 => 1,
            _ => 0,
        };
        let c = candidates(&order, &exe, &own);
        assert_eq!(c.iter().map(|s| s.id).collect::<Vec<_>>(), vec![700, 701, 1, 3], "spells first, then items; no keyblade, no ability");
    }

    #[test]
    fn the_leaders_row_kinds_are_the_tables_and_the_icons_follow_the_flag() {
        let t = CustomizeTables { icons: vec![(0, 5, 12), (0x12, 18, 8)], ..Default::default() };
        assert_eq!((t.icon(0, false), t.icon(0, true)), (5, 12));
        assert_eq!((t.icon(0x12, false), t.icon(0x12, true)), (18, 8));
        assert_eq!(t.icon(77, true), 0xb, "an unlisted type shows icon 0xb");
    }

    // --- WP-PA4: the companions' page 0x1b ---------------------------------------------------------------

    /// The choice tables as the executable holds them (0x381d58, 0x381d88, 0x381da0) and two ability ids of the
    /// use-setting table (0x352ec0: ids 0xa5 and 0xa6, indexes 0 and 1).
    fn friend_tables() -> CustomizeTables {
        let c = |msg: u32, help: u32, icon: u8, value: u8| Choice { msg, help, icon, value };
        CustomizeTables {
            styles: (0..6u8).map(|v| c(0x486 + u32::from(v), 0x1e55 + u32::from(v), 0x1d, v)).collect(),
            uses: (0..3u8).map(|v| c(0x1e4e + u32::from(v), 0x1e5b + u32::from(v), 0x1a + v, v)).collect(),
            item_uses: vec![c(0x1e51, 0x1e5e, 0x1a, 0), c(0x1e52, 0x1e5f, 0x1c, 2)],
            setting_ids: vec![(0xa5, 0), (0xa6, 1)],
            ..Default::default()
        }
    }

    fn party_progress() -> Progress {
        Progress::from_block(&[], std::sync::Arc::new(Default::default()))
    }

    /// A machine for Sora, Donald and Goofy with the companions' rows read from `p` (no item table: the rows are the
    /// style row and the battle items).
    fn friend_machine(p: &Progress, t: &CustomizeTables, who: usize) -> Machine {
        let mut m = Machine::new(Vec::new(), vec![]);
        m.members = vec![1, 2, 3];
        m.who = who;
        m.rows = friend_rows_of(p, t, m.member());
        m
    }

    /// Presses a key and writes its effects as the page does; rebuilds the rows.
    fn press(m: &mut Machine, p: &mut Progress, t: &CustomizeTables, key: Key, asks: bool) -> Vec<Effect> {
        let member = m.member();
        let q = |_: u16| asks;
        let fx = m.press(key, &q);
        for e in &fx {
            write_friend(p, t, member, e);
        }
        m.reload(friend_rows_of(p, t, m.member()));
        fx
    }

    #[test]
    fn the_character_list_has_a_row_per_member_and_the_other_rows_open_page_0x1b() {
        let p = party_progress();
        let t = friend_tables();
        let mut m = friend_machine(&p, &t, 0);
        m.page = CHARACTERS;
        // The leader's row opens page 0x1a ...
        assert_eq!(m.press(Key::Confirm, &none), vec![Effect::Sound(snd::CONFIRM), Effect::Page(ROWS)]);
        m.press(Key::Cancel, &none);
        // ... Donald's (the second row) page 0x1b, Goofy's too; a held repeat stops at the end, a new press wraps.
        assert_eq!(m.press(Key::Down { fresh: true }, &none), vec![Effect::Sound(snd::CURSOR), Effect::Page(CHARACTERS)]);
        assert_eq!((m.who, m.member()), (1, 2));
        assert_eq!(m.press(Key::Confirm, &none), vec![Effect::Sound(snd::CONFIRM), Effect::Page(FRIEND)]);
        m.press(Key::Cancel, &none);
        m.press(Key::Down { fresh: true }, &none);
        assert_eq!(m.member(), 3);
        assert!(m.press(Key::Down { fresh: false }, &none).is_empty());
        m.press(Key::Down { fresh: true }, &none);
        assert_eq!(m.member(), 1, "a new press wraps to the leader");
    }

    #[test]
    fn the_companions_rows_are_the_style_the_kind_two_abilities_in_table_order_and_the_battle_items() {
        let setting = |id: u16| match id {
            0xa6 => 2,
            4 => 2,
            _ => 0,
        };
        // Abilities 0xa6 (table position 7) and 0xa5 (position 9) sort by position; the items repeat once.
        let r = build_friend_rows(&FriendInputs { style: 3, abilities: vec![(0xa5, 9), (0xa6, 7)], slots: vec![(1, 1), (0, 4), (1, 0), (4, 0)], setting_of: &setting });
        let shape: Vec<(Kind, u16, bool, u8)> = r.iter().map(|r| (r.kind, r.id, r.on, r.setting)).collect();
        assert_eq!(
            shape,
            vec![(Kind::Style, 0, false, 3), (Kind::Ability, 0xa6, true, 2), (Kind::Ability, 0xa5, true, 0), (Kind::Item, 1, true, 0), (Kind::Item, 4, true, 2)]
        );
        // Page 0x1f's state: the Auto-Reload flag, plus 2 for the use setting 2.
        assert_eq!(r[3..].iter().map(item_state).collect::<Vec<_>>(), vec![1, 3]);
    }

    #[test]
    fn the_style_row_writes_setting_byte_zero_of_the_members_record() {
        let t = friend_tables();
        let mut p = party_progress();
        let mut m = friend_machine(&p, &t, 1);
        m.page = FRIEND;
        assert_eq!(m.rows[0].kind, Kind::Style);
        // Confirm opens page 0x20 on the current style; the third row is style 2.
        assert_eq!(press(&mut m, &mut p, &t, Key::Confirm, false), vec![Effect::Sound(snd::CONFIRM), Effect::Page(STYLE)]);
        assert_eq!((m.page, m.sub), (STYLE, 0));
        press(&mut m, &mut p, &t, Key::Down { fresh: true }, false);
        press(&mut m, &mut p, &t, Key::Down { fresh: true }, false);
        assert_eq!(press(&mut m, &mut p, &t, Key::Confirm, false), vec![Effect::Sound(snd::CONFIRM), Effect::Style(2), Effect::Page(FRIEND)]);
        assert_eq!(p.member_setting(2, 0), Some(2));
        assert_eq!(p.member_setting(3, 0), Some(0), "Goofy's record is untouched");
        assert_eq!(p.member_setting(1, 0), Some(0), "Sora's is too");
        assert_eq!(m.rows[0].setting, 2, "the row shows the new style");
        // Cancel leaves the byte alone; the six rows wrap.
        press(&mut m, &mut p, &t, Key::Confirm, false);
        assert_eq!(m.sub, 2, "the page starts on the current style");
        press(&mut m, &mut p, &t, Key::Up { fresh: true }, false);
        press(&mut m, &mut p, &t, Key::Up { fresh: true }, false);
        press(&mut m, &mut p, &t, Key::Up { fresh: true }, false);
        assert_eq!(m.sub, 5, "the list wraps");
        assert_eq!(press(&mut m, &mut p, &t, Key::Cancel, false), vec![Effect::Sound(snd::CANCEL), Effect::Page(FRIEND)]);
        assert_eq!(p.member_setting(2, 0), Some(2));
        // The quick key (`FUN_00275398(-1)`) steps the style and wraps from 5 to 0.
        for want in [3, 4, 5, 0, 1] {
            press(&mut m, &mut p, &t, Key::Toggle, false);
            assert_eq!(p.member_setting(2, 0), Some(want));
        }
        // The other companion has its own page: Next from Donald lands on Goofy's, first row.
        assert_eq!(press(&mut m, &mut p, &t, Key::Next, false), vec![Effect::Sound(snd::CURSOR), Effect::Page(FRIEND)]);
        assert_eq!((m.member(), m.cursor), (3, 0));
        press(&mut m, &mut p, &t, Key::Toggle, false);
        assert_eq!((p.member_setting(3, 0), p.member_setting(2, 0)), (Some(1), Some(1)));
        // Next wraps through the leader (page 0x1a), Prev comes back.
        press(&mut m, &mut p, &t, Key::Next, false);
        assert_eq!((m.who, m.page), (0, ROWS));
        press(&mut m, &mut p, &t, Key::Prev, false);
        assert_eq!((m.who, m.page), (2, FRIEND));
    }

    #[test]
    fn an_abilitys_use_setting_is_written_to_its_byte_past_the_consumables() {
        let t = friend_tables();
        assert_eq!((t.setting_index(1), t.setting_index(7), t.setting_index(0xa5), t.setting_index(0xa6), t.setting_index(0x999)), (0, 6, 7, 8, 6));
        let mut p = party_progress();
        let mut m = Machine::new(Vec::new(), vec![]);
        m.members = vec![1, 2, 3];
        m.who = 1;
        m.page = FRIEND;
        m.rows = vec![Row::new(Kind::Style, 0, false, 0), Row { setting: 0, ..Row::new(Kind::Ability, 0xa6, true, 0) }];
        m.cursor = 1;
        // Page 0x21: three rows; row 2 writes the value 2 into byte 1 + 8 of Donald's block.
        press_rows(&mut m, &mut p, &t, Key::Confirm);
        assert_eq!(m.page, ABILITY);
        press_rows(&mut m, &mut p, &t, Key::Down { fresh: true });
        press_rows(&mut m, &mut p, &t, Key::Down { fresh: true });
        let fx = press_rows(&mut m, &mut p, &t, Key::Confirm);
        assert_eq!(fx, vec![Effect::Sound(snd::CONFIRM), Effect::Ability { id: 0xa6, value: 2 }, Effect::Page(FRIEND)]);
        assert_eq!(p.member_setting(2, 9), Some(2));
        assert_eq!(p.member(2).unwrap().settings[9], 2);
        assert_eq!(p.member_setting(3, 9), Some(0));
        // The quick key steps 0, 1, 2 and back to 0.
        m.rows[1].setting = 0;
        for (cur, want) in [(0u8, 1u8), (1, 2), (2, 0), (7, 0)] {
            m.rows[1].setting = cur;
            let fx = m.press(Key::Toggle, &none);
            assert_eq!(fx, vec![Effect::Sound(snd::CONFIRM), Effect::Ability { id: 0xa6, value: want }]);
        }
    }

    fn press_rows(m: &mut Machine, p: &mut Progress, t: &CustomizeTables, key: Key) -> Vec<Effect> {
        let member = m.member();
        let fx = m.press(key, &none);
        for e in &fx {
            write_friend(p, t, member, e);
        }
        fx
    }

    #[test]
    fn a_companions_item_row_has_four_states_and_asks_on_the_off_rows_when_a_slot_waits() {
        let t = friend_tables();
        let mut p = party_progress();
        let mut m = Machine::new(Vec::new(), vec![]);
        m.members = vec![1, 2, 3];
        m.who = 2;
        m.page = FRIEND;
        // Goofy: item 1 carried and reserved (on), setting 0.
        m.rows = vec![Row::new(Kind::Style, 0, false, 0), Row::new(Kind::Item, 1, true, 0)];
        m.cursor = 1;
        // Confirm starts page 0x1f on the row of the state (on, setting 0 = index 1); four rows.
        m.press(Key::Confirm, &none);
        assert_eq!((m.page, m.sub), (RELOAD, 1));
        for _ in 0..4 {
            m.press(Key::Down { fresh: true }, &none);
        }
        assert_eq!(m.sub, 1, "four rows wrap");
        // Row 3 = Auto-Reload on with the use setting 2: both are written; the byte is item 1's own (index 0 -> byte 1).
        m.press(Key::Down { fresh: true }, &none);
        m.press(Key::Down { fresh: true }, &none);
        let fx = press_rows(&mut m, &mut p, &t, Key::Confirm);
        assert_eq!(fx, vec![Effect::Sound(snd::CONFIRM), Effect::FriendItem { item: 1, state: 3 }, Effect::Page(FRIEND)]);
        assert_eq!((p.member_setting(3, 1), p.member_setting(2, 1)), (Some(2), Some(0)));
        // The row that is the state already changes nothing.
        m.rows[1].setting = 2;
        m.press(Key::Confirm, &none);
        assert_eq!(m.sub, 3);
        assert_eq!(m.press(Key::Confirm, &none), vec![Effect::Sound(snd::CONFIRM), Effect::Page(FRIEND)]);
        // An "off" row (even) of an item with a waiting slot asks first; Yes writes the state that was picked, No does not.
        let asks = |_: u16| true;
        m.press(Key::Confirm, &asks);
        assert_eq!(m.sub, 3);
        for _ in 0..3 {
            m.press(Key::Up { fresh: true }, &asks);
        }
        assert_eq!(m.sub, 0);
        assert_eq!(m.press(Key::Confirm, &asks), vec![Effect::Sound(snd::CONFIRM), Effect::Page(QUESTION)]);
        assert_eq!(m.press(Key::Confirm, &asks), vec![Effect::Sound(snd::CANCEL), Effect::Page(FRIEND)], "No leaves it");
        m.press(Key::Confirm, &asks);
        for _ in 0..3 {
            m.press(Key::Up { fresh: true }, &asks);
        }
        m.press(Key::Confirm, &asks);
        m.press(Key::Down { fresh: true }, &asks);
        assert_eq!(m.press(Key::Confirm, &asks), vec![Effect::Sound(snd::CONFIRM), Effect::FriendItem { item: 1, state: 0 }, Effect::Page(FRIEND)]);
        // An "on" row (odd) never asks.
        m.press(Key::Confirm, &asks);
        m.press(Key::Up { fresh: true }, &asks);
        m.press(Key::Up { fresh: true }, &asks);
        assert_eq!(m.sub, 1);
        assert_eq!(m.press(Key::Confirm, &asks), vec![Effect::Sound(snd::CONFIRM), Effect::FriendItem { item: 1, state: 1 }, Effect::Page(FRIEND)]);
        // The quick key cycles the four states: (setting != 0) * 2 + flag + 1, 0 after 3.
        m.rows[1] = Row { setting: 2, ..Row::new(Kind::Item, 1, true, 0) };
        assert_eq!(m.press(Key::Toggle, &none), vec![Effect::Sound(snd::CONFIRM), Effect::FriendItem { item: 1, state: 0 }]);
        m.rows[1] = Row::new(Kind::Item, 1, false, 0);
        assert_eq!(m.press(Key::Toggle, &none), vec![Effect::Sound(snd::CONFIRM), Effect::FriendItem { item: 1, state: 1 }]);
    }

    /// The rows a row of the table read from the executable (skipped without `KH2_ISO`): kinds 3, 4, 2, six styles,
    /// three use settings, two item settings, the consumables' setting indexes and a messages check.
    #[test]
    fn the_executables_tables_give_the_companions_pages() {
        let iso = std::env::var("KH2_ISO").ok().or_else(|| std::fs::read_to_string(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../local/iso.txt")).ok().map(|s| s.trim().to_string()));
        let Some(iso) = iso else {
            eprintln!("skipped: no disc (set KH2_ISO)");
            return;
        };
        let Ok(mut a) = kh2_archive::Archive::open(&iso) else { return };
        let exe = Exe::load(a.iso().read("SLUS_210.05").or_else(|_| a.iso().read("SLUS_210.05;1")).expect("the executable")).expect("an ELF");
        let t = CustomizeTables::read(&exe).expect("tables");
        println!("{t:?}");
        assert_eq!((t.kinds.as_slice(), t.friend_kinds.as_slice()), (&[0u8, 1, 2][..], &[3u8, 4, 2][..]));
        assert_eq!(t.styles.iter().map(|c| c.value).collect::<Vec<_>>(), vec![0, 1, 2, 3, 4, 5]);
        assert_eq!(t.uses.iter().map(|c| c.value).collect::<Vec<_>>(), vec![0, 1, 2]);
        assert_eq!(t.item_uses.iter().map(|c| c.value).collect::<Vec<_>>(), vec![0, 2]);
        assert_eq!(t.heading[3..], [0x483, 0x484]);
        assert_eq!(t.setting_ids.len(), 29);
        let largest = t.setting_ids.iter().map(|e| t.setting_index(e.0 as u16)).max().expect("entries");
        println!("largest use setting index {largest}, setting byte {}", 1 + largest);
        let mut p = Progress::from_block(&[], std::sync::Arc::new(Default::default()));
        for e in &t.setting_ids {
            let k = t.setting_index(e.0 as u16);
            assert!(p.set_member_setting(SORA, 1 + k, 1), "id {:#x} maps to setting byte {}, past the settings block", e.0, 1 + k);
        }
        assert_eq!((t.setting_index(1), t.setting_index(7), t.setting_index(0xa5), t.setting_index(0xbe)), (0, 6, 7, 7 + 2));
        assert_eq!((t.setting_index(246), t.setting_index(0)), (6, 6), "an id the table lacks shares Megalixir's byte");
        let m = kh2_image::text::Messages::from_bar(&a.read("msg/us/sys.bar").expect("sys.bar")).expect("messages");
        for c in t.styles.iter().chain(&t.uses).chain(&t.item_uses) {
            assert!(m.text(c.msg).is_some_and(|s| !s.is_empty()), "message {:#x}", c.msg);
            assert!(m.text(c.help).is_some_and(|s| !s.is_empty()), "help {:#x}", c.help);
        }
        for id in [2usize, 3] {
            assert!(m.text(t.member_names[id]).is_some_and(|s| !s.is_empty()), "name {id}");
        }
    }

    /// `FUN_002743e8`: on page 0x1b the help of a row is its description with the help message of the choice entry that
    /// holds the row's setting appended; the Battle Style row, the leader's page and the character list differ.
    #[test]
    fn a_companions_help_appends_the_choice_message() {
        let t = friend_tables();
        let ability = Row { setting: 2, ..Row::new(Kind::Ability, 0xa5, true, 0) };
        let item = Row { setting: 2, ..Row::new(Kind::Item, 3, false, 0) };
        let unset = Row { setting: 1, ..Row::new(Kind::Item, 3, false, 0) };
        assert_eq!(appended_help(&t, &ability), Some(0x1e5d), "uses entry with value 2");
        assert_eq!(appended_help(&t, &item), Some(0x1e5f), "item uses entry with value 2");
        assert_eq!(appended_help(&t, &unset), None, "no entry holds 1 in the item table");
        assert_eq!(appended_help(&t, &Row { setting: 3, ..Row::new(Kind::Style, 0, false, 0) }), None, "the style row shows its own message only");
        assert_eq!((characters_help(0), characters_help(1), characters_help(2)), (0x3b89, 0x3b8a, 0x3b8a));
    }

    /// The choice lookup (`FUN_002748d8`) matches on the value byte, not the position: ability settings 0 and 1 give the
    /// first two entries, item setting 0 the first item entry; the shortcut and spell rows append nothing.
    #[test]
    fn the_appended_help_follows_the_value_byte_of_each_kind() {
        let t = friend_tables();
        let row = |kind, setting| Row { setting, ..Row::new(kind, 3, false, 0) };
        assert_eq!(appended_help(&t, &row(Kind::Ability, 0)), Some(0x1e5b));
        assert_eq!(appended_help(&t, &row(Kind::Ability, 1)), Some(0x1e5c));
        assert_eq!(appended_help(&t, &row(Kind::Ability, 3)), None, "a value no entry holds");
        assert_eq!(appended_help(&t, &row(Kind::Item, 0)), Some(0x1e5e));
        assert_eq!(appended_help(&t, &row(Kind::Shortcut, 0)), None);
        assert_eq!(appended_help(&t, &row(Kind::Spell, 0)), None);
        assert_eq!(mark_of(&t, &row(Kind::Item, 2)), Some(0x1c));
    }

    #[test]
    fn the_joined_help_runs_the_description_into_the_choice_message() {
        let iso = std::env::var("KH2_ISO").ok().or_else(|| std::fs::read_to_string(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../local/iso.txt")).ok().map(|s| s.trim().to_string()));
        let Some(iso) = iso else {
            eprintln!("skipped: no disc (set KH2_ISO)");
            return;
        };
        let Ok(mut a) = kh2_archive::Archive::open(&iso) else { return };
        let m = kh2_image::text::Messages::from_bar(&a.read("msg/us/sys.bar").expect("sys.bar")).expect("messages");
        for id in [0x3b89u32, 0x3b8a, 0x1e5b, 0x1e5c, 0x1e5d, 0x1e5e, 0x1e5f] {
            assert!(m.text(id).is_some_and(|s| !s.is_empty()), "message {id:#x} exists");
        }
        // The buffer FUN_002743e8 builds: the description without its end byte, the choice message after it, one end.
        for (first, second) in [(0x3b89u32, 0x1e5bu32), (0x3b8a, 0x1e5c), (0x3b8a, 0x1e5f)] {
            let (a, b) = (m.tokens(first).expect("description"), m.tokens(second).expect("choice message"));
            let body = |v: &[Token]| v.iter().filter(|t| **t != Token::End).cloned().collect::<Vec<_>>();
            let (da, db) = (body(&a), body(&b));
            assert!(!da.is_empty() && !db.is_empty());
            let mut want = da.clone();
            want.extend(db.iter().cloned());
            want.push(Token::End);
            let got = joined_tokens(a, b);
            assert_eq!(got, want, "{first:#x} + {second:#x}");
            assert_eq!(got.iter().filter(|t| **t == Token::End).count(), 1, "one end command, at the end");
            assert_eq!(&got[..da.len()], &da[..], "the description comes first");
            assert_eq!(&got[da.len()..got.len() - 1], &db[..], "the choice message follows it directly");
        }
    }

    /// A companion's settings written by the pages come back through a slot file (`Progress::slot_json` /
    /// `apply_json`, the state file's own format), each in its member's record.
    #[test]
    fn the_pages_writes_survive_a_state_file_round_trip() {
        let t = friend_tables();
        let mut p = party_progress();
        for (member, e) in [
            (2, Effect::Style(4)),
            (2, Effect::Ability { id: 0xa6, value: 2 }),
            (3, Effect::Style(1)),
            (3, Effect::FriendItem { item: 3, state: 2 }),
        ] {
            write_friend(&mut p, &t, member, &e);
        }
        let text = p.slot_json(&crate::progress::SlotInfo { level: 1, munny: 0, play_seconds: 0, area: "tt04".into() });
        let mut q = party_progress();
        q.apply_json(&text).unwrap();
        // Donald: style 4, the ability byte 1 + (7 + 1); Goofy: style 1, item 3's byte 1 + 2 holds the use setting 2.
        assert_eq!((q.member_setting(2, 0), q.member_setting(2, 9)), (Some(4), Some(2)));
        assert_eq!((q.member_setting(3, 0), q.member_setting(3, 3)), (Some(1), Some(2)));
        assert_eq!((q.member_setting(1, 0), q.member_setting(2, 3)), (Some(0), Some(0)), "the other records are untouched");
        // The harness key sets the same bytes: `{"party_settings": {"2": 4}}`.
        let mut r = party_progress();
        r.apply_json("{\"party_settings\": {\"2\": 4}}").unwrap();
        assert_eq!(r.member_setting(2, 0), Some(4));
    }

    /// WP-PA4: the Customize page 0x1b writes what `party.rs` reads. Donald's Style row, driven through the page's machine
    /// (confirm, two rows down, confirm), stores style 2 in his record and `refresh_target` then picks the far enemy
    /// (PA3's scene); the ability row's use setting lands in the byte the AI record reads (`record_setting`), and Goofy's
    /// record is not touched.
    #[test]
    fn party_follows_what_the_customize_page_writes() {
        use crate::menu::customize::{write_friend, Choice, CustomizeTables, Effect, Key, Kind, Machine, Row, FRIEND};
        use crate::party::{record_setting, refresh_target, resolve_style, Target, TargetCtx};
        use crate::party_battle as battle;
        let mut p = crate::progress::Progress::from_block(&[], std::sync::Arc::new(Default::default()));
        let t = CustomizeTables { setting_ids: vec![(0xa5, 0), (0xa6, 1)], uses: vec![Choice::default(); 3], ..Default::default() };
        let mut m = Machine::new(vec![Row::new(Kind::Style, 0, false, 0)], vec![]);
        m.members = vec![1, 2, 3];
        m.who = 1;
        m.page = FRIEND;
        let no = |_: u16| false;
        for key in [Key::Confirm, Key::Down { fresh: true }, Key::Down { fresh: true }, Key::Confirm] {
            for e in m.press(key, &no) {
                write_friend(&mut p, &t, 2, &e);
            }
        }
        assert_eq!((p.member_setting(2, 0), p.member_setting(3, 0)), (Some(2), Some(0)));
        let mut w = bevy::ecs::world::World::new();
        let (a, b) = (w.spawn_empty().id(), w.spawn_empty().id());
        let enemies = vec![(a, vec![(Vec3::new(300.0, 0.0, 0.0), 0.0)]), (b, vec![(Vec3::new(-150.0, 0.0, 0.0), 0.0)])];
        let params = battle::Params { range: 400.0, retarget: 500.0, player_range: 220.0, player_near: 50.0, retarget_half: 0.5, farthest_start: -1.0, ..Default::default() };
        let ctx = TargetCtx { enemies: &enemies, player: Vec3::new(100.0, 0.0, 0.0), attacker: None, player_target: None, player_hit: None };
        let pick = |p: &crate::progress::Progress| {
            let style = resolve_style(None, p.member_setting(2, 0));
            refresh_target(2, style, &params, Vec3::ZERO, None, false, false, &ctx).expect("style in the table")
        };
        assert_eq!(pick(&p), Some(Target { entity: a, shape: 0 }), "style 2 picks the far enemy");
        // The default style 0 (a record that was never written) picks the other one in the same scene.
        let fresh = crate::progress::Progress::from_block(&[], std::sync::Arc::new(Default::default()));
        assert_eq!(pick(&fresh), Some(Target { entity: b, shape: 0 }), "style 0 picks the near enemy");
        // The quick key moves it on to 3 (the page rebuilds its rows from the record after a write).
        m.reload(vec![Row { setting: 2, ..Row::new(Kind::Style, 0, false, 0) }]);
        for e in m.press(Key::Toggle, &no) {
            write_friend(&mut p, &t, 2, &e);
        }
        assert_eq!(p.member_setting(2, 0), Some(3));
        // An ability row: id 0xa6 is entry 1 of the use-setting table, so its byte is 1 + (7 + 1) = 9.
        let fx = Effect::Ability { id: 0xa6, value: 3 };
        write_friend(&mut p, &t, 2, &fx);
        let member = p.member(2);
        assert_eq!(record_setting(member.as_ref(), 8), Some(3), "the record's setting index 8 reads byte 9");
        assert_eq!(record_setting(member.as_ref(), -1), Some(0));
        assert_eq!(record_setting(None, 8), Some(0));
        assert_eq!(p.member(3).unwrap().settings[9], 0);
    }
}
