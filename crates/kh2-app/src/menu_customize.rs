//! The pause menu's Customize pages, stage M3 (docs/menus.md 2.4 and section 14): page 0x19 the character list
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
//! (`FUN_00274e50`) and `set_auto_reload` (`FUN_00274f70`). The party is Sora alone: the character list has one row
//! and the Donald and Goofy kinds (3 and 4: Battle Style, abilities) are not ported.

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
/// Entry 2 of the top list (Customize) opens page 0x19.
pub const ENTRY: usize = 2;
/// Rows of the settings list shown at once (`FUN_00299010(list, 9)`).
pub const VISIBLE: usize = 9;
/// Spell slots (Fire, Blizzard, Thunder, Cure, Magnet, Reflect) and item type of a spell item.
pub const SPELLS: usize = 6;
pub const SPELL_TYPE: u8 = 0x12;

pub use super::item_pages::snd;

// --- the rows -------------------------------------------------------------------------------------

/// The three row kinds of the leader (`DAT_00381d40`: 0, 1, 2).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// One of the four command shortcuts (save `+0x2600`).
    Shortcut,
    /// A spell's MP Charge switch (the bit of the word at save `+0x264c`).
    Spell,
    /// A battle item's Auto-Reload (its reserved ids, record `+0x44`).
    Item,
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
        rows.push(Row { kind: Kind::Shortcut, id, on: false, slot: 0 });
    }
    for slot in 0..SPELLS as u8 {
        let item = i.spell_items[slot as usize];
        if item != 0 && (i.count)(item) > 0 {
            rows.push(Row { kind: Kind::Spell, id: item, on: !(i.not_used)(slot), slot });
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
        rows.push(Row { kind: Kind::Item, id, on: reserved == id, slot: 0 });
    }
    // The shortcut rows show the state of the row with the same item (the loop at the end of the routine).
    let tail: Vec<Row> = rows[4..].to_vec();
    for r in rows.iter_mut().take(4) {
        if r.id != 0
            && let Some(t) = tail.iter().filter(|t| t.id == r.id).next_back()
        {
            r.on = t.on;
        }
    }
    rows
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

/// The list results of docs/menus.md 1.7 that the Customize pages use.
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
}

/// Rows the candidate list shows at once, the removal row among them (`FUN_00273d30` caps the list at 8).
pub const CAND_VISIBLE: usize = 8;

impl Machine {
    pub fn new(rows: Vec<Row>, candidates: Vec<Stack>) -> Machine {
        Machine { page: CHARACTERS, rows, cursor: 0, scroll: 0, candidates, cand: 0, cand_scroll: 0, sub: 0, question: 0 }
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

    /// One key on the current page. `question` answers `Progress::reload_off_needs_question`.
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
        match self.page {
            CHARACTERS => match key {
                Key::Confirm => {
                    self.page = ROWS;
                    self.cursor = 0;
                    self.scroll = 0;
                    fx.push(Sound(snd::CONFIRM));
                    fx.push(Page(ROWS));
                }
                Key::Cancel => {
                    fx.push(Sound(snd::CANCEL));
                    fx.push(Effect::Leave);
                }
                _ => {}
            },
            ROWS => {
                if let Some((d, fresh)) = dir(key) {
                    let new = super::step_cursor(self.cursor, self.rows.len(), d, fresh);
                    if new != self.cursor {
                        self.cursor = new;
                        Self::keep_visible(new, &mut self.scroll, VISIBLE);
                        fx.push(Sound(snd::CURSOR));
                        fx.push(Page(ROWS));
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
                            self.sub = row.on as usize;
                            self.page = RELOAD;
                            fx.push(Sound(snd::CONFIRM));
                            fx.push(Page(RELOAD));
                        }
                    },
                    Key::Toggle => match row.kind {
                        Kind::Shortcut => {}
                        Kind::Spell => {
                            fx.push(Sound(snd::CONFIRM));
                            fx.push(Effect::Spell { slot: row.slot, usable: !row.on });
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
                    // `FUN_00275538`: with one character the switch has nowhere to go.
                    Key::Prev | Key::Next => fx.push(Sound(snd::REFUSED)),
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
                        self.page = ROWS;
                        fx.push(Page(ROWS));
                    }
                    Key::Cancel => {
                        self.page = ROWS;
                        fx.push(Sound(snd::CANCEL));
                        fx.push(Page(ROWS));
                    }
                    _ => {}
                }
            }
            SPELL | RELOAD => {
                if let Some((d, fresh)) = dir(key) {
                    let new = super::step_cursor(self.sub, 2, d, fresh);
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
                        // `FUN_00272018` / `FUN_00274c88`: nothing changes when the row is the state it already has.
                        let want = self.sub == 1;
                        fx.push(Sound(snd::CONFIRM));
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
                        self.page = ROWS;
                        fx.push(Page(ROWS));
                    }
                    Key::Cancel => {
                        self.page = ROWS;
                        fx.push(Sound(snd::CANCEL));
                        fx.push(Page(ROWS));
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
                            fx.push(Effect::Reload { item: row.id, on: false });
                        }
                        self.page = ROWS;
                        fx.push(Page(ROWS));
                    }
                    Key::Confirm | Key::Cancel => {
                        self.page = ROWS;
                        fx.push(Sound(snd::CANCEL));
                        fx.push(Page(ROWS));
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
    pub const ROW: [usize; 3] = [0x108, 0x107, 0x106];
    pub const TAG: [usize; 3] = [0x11a, 0x119, 0x118];
    pub const SUB: [[usize; 2]; 3] = [[0xc6, 0xc7], [0xbd, 0xbe], [0xb4, 0xb5]];
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
    pub const COLOR: [u8; 4] = [0x80; 4];
    pub const SELECTED: [u8; 4] = [0xe5, 0xe5, 0xe5, 0x80];
    pub const IDLE: [u8; 4] = [0x50, 0x50, 0x58, 0x80];
    pub const HELP_COLOR: [u8; 4] = [0x30, 0x7c, 0x80, 0x80];
    /// The tag text of a section and the sub page's rows from the window anchor [inferred from the groups' boxes].
    pub const TAG_LABEL: [f32; 2] = [-86.0, 1.0];
    pub const LIST_PITCH: f32 = 28.0;
    /// The information window's text and rows from its anchor (255, 126), by eye as on the Items pages.
    pub const INFO_TEXT: [f32; 2] = [-210.0, -10.0];
    pub const INFO_ROWS: [f32; 2] = [-30.0, 98.0];
    pub const INFO_PITCH: f32 = 24.0;
}

/// The executable's tables the pages read (docs/menus.md section 14).
#[derive(Clone, Debug, Default)]
pub struct CustomizeTables {
    /// 0x381d40: the row kinds of the leader.
    pub kinds: Vec<u8>,
    /// 0x381cb0 is not read: the heading messages (kind 1 and 2) and the four shortcut row labels.
    pub heading: [u32; 3],
    pub shortcut_labels: [u32; 4],
    /// 0x381d20 and 0x381d30: the two rows of the sub pages, index = the flag (0 off, 1 on).
    pub spell_rows: [u32; 2],
    pub item_rows: [u32; 2],
    /// 0x3809d0: (item type, icon unequipped, icon equipped) until a negative type (`FUN_00255368`).
    pub icons: Vec<(i16, i16, i16)>,
}

impl CustomizeTables {
    pub fn read(exe: &Exe) -> Option<CustomizeTables> {
        let msg = |va: u32| exe.u16(va).map(|v| u32::from(v & 0x7fff));
        let mut t = CustomizeTables::default();
        for k in 0..3u32 {
            t.kinds.push(exe.u8(0x0038_1d40 + k)?);
        }
        for k in 0..3u32 {
            t.heading[k as usize] = msg(0x0038_1cc0 + 2 * k)?;
        }
        for k in 0..4u32 {
            t.shortcut_labels[k as usize] = msg(0x0038_1cd0 + 2 * k)?;
        }
        t.spell_rows = [msg(0x0038_1d20)?, msg(0x0038_1d22)?];
        t.item_rows = [msg(0x0038_1d30)?, msg(0x0038_1d32)?];
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

    /// `FUN_00255368`: the icon of a row by item type and flag (0xb when the type is not listed).
    pub fn icon(&self, kind: u8, flag: bool) -> u8 {
        self.icons.iter().find(|t| t.0 == kind as i16).map_or(0xb, |t| if flag { t.2 as u8 } else { t.1 as u8 })
    }
}

struct Ui {
    tabs: [usize; 2],
    char_row: usize,
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
    tables: Option<CustomizeTables>,
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
    st.tables = CustomizeTables::read(&exe);
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
            se.write(PlaySe::number(snd::REFUSED, None));
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
        st.machine = Some(Machine::new(rows, cands));
        st.held = (0, 0.0);
        enter(&mut st, &mut menu, &mut scene);
        se.write(PlaySe::number(snd::CONFIRM, None));
        keys.reset_all();
        redraw(&mut st, &items, &mut scene, p, &cmds);
        return;
    }
    let mut held = st.held;
    let pressed = keys_of(&keys, &mut held, real.delta_secs());
    st.held = held;
    let mut close = false;
    for key in pressed {
        let Some(p) = progress.as_deref() else { break };
        let question = |id: u16| p.reload_off_needs_question(SORA, id);
        let fx = st.machine.as_mut().map(|m| m.press(key, &question)).unwrap_or_default();
        for e in fx {
            match e {
                Effect::Sound(n) => {
                    se.write(PlaySe::number(n, None));
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
            let rows = rows_of(&items, p, &cmds);
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
    let char_row = mk(scene, S);
    let window = mk(scene, S);
    let rows: Vec<usize> = (0..VISIBLE).map(|_| mk(scene, S)).collect();
    let tags: Vec<usize> = (0..VISIBLE).map(|_| mk(scene, S)).collect();
    let sub_panel = mk(scene, S);
    let sub_title = mk(scene, S);
    let sub_rows: Vec<usize> = (0..CAND_VISIBLE).map(|_| mk(scene, S)).collect();
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
    scene.nodes[sub_title].parent = Some(sub_panel);
    for &r in &sub_rows {
        scene.nodes[r].parent = Some(sub_panel);
    }
    scene.nodes[info_text].parent = Some(info);
    for &r in &info_rows {
        scene.nodes[r].parent = Some(info);
    }
    st.ui = Some(Ui { tabs, char_row, hand, glint, window, rows, tags, sub_panel, sub_title, sub_rows, info, info_text, info_rows, help, all });
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
/// 0x3809d0) and the name (`FUN_00272e68`: the item's own name for items, the command's name for the shortcut and
/// spell rows).
fn row_tokens(text: &crate::menu_text::MenuText, t: &CustomizeTables, items: &ItemsMenu, c: &Cmds, r: &Row) -> Vec<Token> {
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
        Kind::Item => items.rows.get(&r.id).map(|i| u32::from(i.name)),
        _ => c.name(c.of_item(r.id)).or_else(|| items.rows.get(&r.id).map(|i| u32::from(i.name))),
    };
    if let Some(n) = name {
        v.extend(tokens_of(text, n));
    }
    v
}

fn redraw(st: &mut CustomizeMenu, items: &ItemsMenu, scene: &mut MenuScene, p: &Progress, c: &Cmds) {
    let Some(m) = st.machine.clone() else { return };
    let (Some(ui), Some(t)) = (st.ui.as_ref(), st.tables.clone()) else { return };
    let (tabs, char_row, hand, glint, window, rows, tags, sub_panel, sub_title, sub_rows, help) =
        (ui.tabs, ui.char_row, ui.hand, ui.glint, ui.window, ui.rows.clone(), ui.tags.clone(), ui.sub_panel, ui.sub_title, ui.sub_rows.clone(), ui.help);
    let (info, info_text, info_rows) = (ui.info, ui.info_text, ui.info_rows.clone());
    let all = ui.all.clone();
    let entry = st.entry.clone().unwrap_or_else(|| Entry { idle: [0; 4], selected: [0; 4], name: 0, help: 0, need: vec![], deny: vec![] });
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

    // The header tabs: Customize, then the character.
    for (k, &tab) in tabs.iter().enumerate() {
        let prev = was_of(tab);
        scene.nodes[tab].pos = [at::TAB_PITCH * k as f32, 0.0];
        set(scene, tab, group::TAB[0], Some(group::TAB[2]), prev);
        let tokens = if k == 0 { tokens_of(&text, entry.name) } else { text.literal("Sora", Style::default(), [0.0; 2]).tokens };
        scene.nodes[tab].label_anchor = Some(at::TAB_LABEL);
        scene.nodes[tab].label = Some(label(&text, tokens, 1, 15.0, at::COLOR, 0, [0.0; 2]));
    }

    if page == CHARACTERS {
        // The character list: Sora's row as a selected row of page 0.
        let prev = was_of(char_row);
        scene.nodes[char_row].off = [0.0, 0.0];
        set(scene, char_row, usize::from(entry.selected[0]), Some(usize::from(entry.selected[2])), prev);
        scene.nodes[char_row].label_anchor = Some([-7.0, 1.5]);
        scene.nodes[char_row].label = Some(label(&text, text.literal("Sora", Style::default(), [0.0; 2]).tokens, 1, 15.0, at::SELECTED, 0, [0.0; 2]));
        scene.nodes[hand].parent = Some(char_row);
        scene.nodes[hand].off = at::HAND;
        set(scene, hand, group::HAND, None, was_of(hand));
        scene.nodes[glint].parent = Some(char_row);
        scene.nodes[glint].off = at::GLINT;
        set(scene, glint, group::GLINT[0], Some(group::GLINT[2]), was_of(glint));
        set_help(scene, &text, help, Some(0x3b89));
        scene.text = Some(text);
        return;
    }

    // The settings window and its rows (`FUN_00270ae0`).
    set(scene, window, group::WINDOW[0], Some(group::WINDOW[1]), was_of(window));
    let kind_index = |k: Kind| match k {
        Kind::Shortcut => 0,
        Kind::Spell => 1,
        Kind::Item => 2,
    };
    let mut prev_kind: Option<Kind> = (m.scroll > 0).then(|| m.rows[m.scroll - 1].kind);
    for k in 0..VISIBLE {
        let idx = m.scroll + k;
        let Some(r) = m.rows.get(idx) else { break };
        let ki = kind_index(r.kind);
        let selected = idx == m.cursor && page == ROWS;
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
        prev_kind = Some(r.kind);
    }
    if let Some(&sel) = rows.get(m.cursor.saturating_sub(m.scroll)) {
        scene.nodes[hand].parent = Some(sel);
        scene.nodes[hand].off = at::HAND;
        set(scene, hand, group::HAND, None, page != ROWS && was_of(hand));
    }
    let row = m.rows.get(m.cursor).copied();
    let help_id = match (page, row) {
        (CANDIDATES, _) => m.cand.checked_sub(1).and_then(|i| m.candidates.get(i)).and_then(|s| items.rows.get(&s.id)).map(|r| u32::from(r.description)).or(Some(0x1df7)),
        _ => None,
    };
    set_help(scene, &text, help, help_id);

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
    if matches!(page, CANDIDATES | SPELL | RELOAD) {
        let ki = match page {
            CANDIDATES => 0,
            SPELL => 1,
            _ => 2,
        };
        set(scene, sub_panel, group::SUB[ki][0], Some(group::SUB[ki][1]), was_of(sub_panel));
        set(scene, sub_title, group::TITLE, None, was_of(sub_title));
        let title = match (page, row) {
            (CANDIDATES, _) => t.shortcut_labels[m.cursor.min(3)],
            (SPELL, _) => t.heading[1],
            _ => t.heading[2],
        };
        scene.nodes[sub_title].label_anchor = Some(at::LABEL);
        scene.nodes[sub_title].label = Some(label(&text, tokens_of(&text, title), 1, 15.0, at::COLOR, 0, [0.0; 2]));
        let mut lines: Vec<(Vec<Token>, bool)> = Vec::new();
        let cursor;
        let mut first = 0;
        match page {
            CANDIDATES => {
                // Row 0 removes the shortcut, then the owned consumables and spells.
                lines.push((tokens_of(&text, 0x1df7), m.cand == 0));
                for (i, s) in m.candidates.iter().enumerate() {
                    let name = items.rows.get(&s.id).map_or(0, |r| r.name);
                    let icon = items.rows.get(&s.id).map_or(0, |r| r.icon);
                    let mut tk = vec![Token::Icon(icon)];
                    tk.extend(tokens_of(&text, c.name(c.of_item(s.id)).unwrap_or(u32::from(name))));
                    lines.push((tk, m.cand == i + 1));
                }
                cursor = m.cand;
                first = m.cand_scroll;
            }
            SPELL => {
                lines = t.spell_rows.iter().enumerate().map(|(i, &id)| (tokens_of(&text, id), i == m.sub)).collect();
                cursor = m.sub;
            }
            RELOAD => {
                lines = t.item_rows.iter().enumerate().map(|(i, &id)| (tokens_of(&text, id), i == m.sub)).collect();
                cursor = m.sub;
            }
            _ => {
                lines = Vec::new();
                cursor = 0;
            }
        }
        let shown: Vec<(Vec<Token>, bool)> = lines.into_iter().skip(first).take(CAND_VISIBLE).collect();
        for (k, (tokens, sel)) in shown.iter().enumerate() {
            let r = sub_rows[k];
            // The title strip hangs above the panel's anchor, the rows start at it (`FUN_00273d30`, the group boxes).
            scene.nodes[r].off = [0.0, at::LIST_PITCH * k as f32];
            set(scene, r, group::LIST_ROW, None, was_of(r));
            scene.nodes[r].label_anchor = Some(at::LABEL);
            scene.nodes[r].label = Some(label(&text, tokens.clone(), 1, 15.0, if *sel { at::SELECTED } else { at::COLOR }, 0, [0.0; 2]));
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
        let off = vec![Row { kind: Kind::Item, id: 4, on: false, slot: 0 }];
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
        let many: Vec<Row> = (0..20).map(|i| Row { kind: Kind::Item, id: i + 1, on: false, slot: 0 }).collect();
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
}
