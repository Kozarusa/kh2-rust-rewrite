//! The pause menu's Equipment pages, stage M4 (docs/menus.md 2.2 and section 15): page 2 the character's slots
//! (`FUN_00269580`, built by `FUN_00269028` over the equipment list `FUN_00283be0`), pages 5 to 9 the candidate
//! list of one category (`FUN_0026a528`, built by `FUN_00269580`'s page switch), page 3 the question before
//! Auto-Reload goes off (`FUN_0026a960`) and page 4 the notice that an accessory would leave too little AP
//! (`FUN_0026ab10`).
//!
//! This file is a child module of `menu` (`#[path] pub mod equipment;` there). The pure part is the rules, so the
//! tests drive them:
//!
//! * `build_rows` is the equipment list of `FUN_00283be0` for the leader: the weapon, one keyblade per drive form
//!   that has a second hand, the armor slots, the accessory slots and the battle item slots,
//! * `candidates` is the list of a pick page: `FUN_00283a70` with the category's filter (`FUN_00283810`), and for
//!   the Items category the received items `FUN_00283e50` adds with a count of 0, sorted by `FUN_002845a0`,
//! * `ap_short` is `FUN_0026ca28`, the refusal of an accessory swap that would leave the abilities without AP,
//! * `Machine::press` is the page handlers' result switches.
//!
//! The writers are `Progress::equip_weapon` (`FUN_0026c420`), `equip_form_weapon` (`FUN_0026c4d0`), `equip_armor`
//! (`FUN_0026c588`), `equip_accessory` (`FUN_0026c610`), `equip_battle_item` (`FUN_0026c698`), `set_auto_reload`
//! (`FUN_0026c860`) and `settle_battle_items` (`FUN_0026c770` + `FUN_001877c8`). Every one takes the member whose row opened
//! page 2 (`ItemsMenu::member`: Sora, Donald or Goofy); the forms' rows and the live `Abilities` refresh are Sora's.

use bevy::prelude::*;
use kh2_image::text::{Style, Token};

use super::item_pages::{count_text, fill, label, node, set_help, stock, tokens_of, ExeTables, ItemRow, ItemsMenu, Stack};
use super::new_tags::NewTags;
use super::pictures::Pictures;
use super::{help, Menu, Phase, Switch};
use crate::abilities::Abilities;
use crate::assets::Disc;
use crate::audio::PlaySe;
use crate::menu_gfx::MenuScene;
use crate::menu_pages::Exe;
use crate::progress::{item_type, Progress, INVENTORY, SORA};
use kh2_tables::tables as t;

// --- pages --------------------------------------------------------------------------------------

/// Page 2: the slots of the character (the categories' rows).
pub const LIST: u16 = 2;
/// Page 3: "Turn auto-reload function Off?" (No, Yes).
pub const QUESTION: u16 = 3;
/// Page 4: the notice that the swap would leave less AP than the abilities use.
pub const NOTICE: u16 = 4;
/// The pick pages are `PICK + category` (5 Weapons, 6 forms' weapons, 7 Armor, 8 Accessories, 9 Items; the table
/// at 0x381970).
pub const PICK: u16 = 5;
/// Rows of the slot list shown at once (`FUN_00299010(list, 9)`) and of a pick list (`FUN_00299010(list, 8)`).
pub const VISIBLE: usize = 9;
pub const PICK_VISIBLE: usize = 8;
/// The drive forms that hold a second keyblade (`FUN_00282bc8`: 1 Valor, 3 Master, 4 Final).
pub const HAND_FORMS: [u8; 3] = [1, 3, 4];

pub use super::item_pages::snd;

// --- the rows -------------------------------------------------------------------------------------

/// The five categories of `FUN_00283f28` (the forms' weapons are category 1 and are listed with the weapon).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cat {
    Weapon,
    Form,
    Armor,
    Accessory,
    Item,
}

impl Cat {
    pub const ALL: [Cat; 5] = [Cat::Weapon, Cat::Form, Cat::Armor, Cat::Accessory, Cat::Item];

    pub fn index(self) -> usize {
        self as usize
    }

    /// The pick page of the category (table 0x381970).
    pub fn page(self) -> u16 {
        PICK + self.index() as u16
    }

    pub fn of_page(page: u16) -> Option<Cat> {
        Cat::ALL.get(page.checked_sub(PICK)? as usize).copied()
    }

    /// Armor, accessory and item pages have a first row that empties the slot (`FUN_00269580`: the row count is
    /// the list's plus one unless the page is 5 or 6).
    pub fn has_remove(self) -> bool {
        matches!(self, Cat::Armor | Cat::Accessory | Cat::Item)
    }

    /// The item type the category lists (the filter argument of table 0x3819a8: 1 the character's keyblade type,
    /// 2 armor, 3 accessories, 4 consumables).
    pub fn item_type(self) -> u8 {
        match self {
            Cat::Weapon | Cat::Form => item_type::KEYBLADE,
            Cat::Armor => item_type::ARMOR,
            Cat::Accessory => item_type::ACCESSORY,
            Cat::Item => item_type::CONSUMABLE,
        }
    }
}

/// One row of the equipment list (`FUN_00283be0`, 6 bytes: the id, `+2` carried, `+3` the Auto-Reload flag).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Row {
    pub cat: Cat,
    /// The item in the slot (a battle item slot that only reserves an item shows the reserved one); 0 for empty.
    pub id: u16,
    /// The flag byte `+2`: the slot holds an item (1 for any non-empty row but a reserved empty battle item slot).
    pub carried: bool,
    /// The flag byte `+3`: for a battle item slot, its reserved id equals the shown id (Auto-Reload is on).
    pub reload: bool,
    /// The slot in the record's list for armor, accessories and items, the form number for a form's weapon, 0 for
    /// the main weapon.
    pub slot: usize,
}

/// What `build_rows` reads of the game.
pub struct Inputs {
    pub weapon: u16,
    /// The forms with a second hand that the player has (`FUN_00282bf8 & FUN_00282c60`) and their keyblades
    /// (`FUN_00282cf0`), in form order.
    pub forms: Vec<(u8, u16)>,
    pub armor: Vec<u16>,
    pub accessories: Vec<u16>,
    /// The battle item slots as (item, reserved id).
    pub items: Vec<(u16, u16)>,
}

/// `FUN_00283be0`: the rows in the order the page lists them (the weapon category holds its own row and then the
/// forms' rows, category 1 is skipped as a block of its own).
pub fn build_rows(i: &Inputs) -> Vec<Row> {
    let mut rows = vec![Row { cat: Cat::Weapon, id: i.weapon, carried: i.weapon != 0, reload: false, slot: 0 }];
    for &(form, id) in &i.forms {
        rows.push(Row { cat: Cat::Form, id, carried: id != 0, reload: false, slot: form as usize });
    }
    for (cat, list) in [(Cat::Armor, &i.armor), (Cat::Accessory, &i.accessories)] {
        for (slot, &id) in list.iter().enumerate() {
            rows.push(Row { cat, id, carried: id != 0, reload: false, slot });
        }
    }
    for (slot, &(item, reserved)) in i.items.iter().enumerate() {
        // An empty slot that reserves an item shows the reserved one, not carried; the flag compares the reserved
        // id with the shown id (so an empty slot with nothing reserved has the flag set too, as the original).
        let (id, carried) = if item == 0 && reserved != 0 { (reserved, false) } else { (item, item != 0) };
        rows.push(Row { cat: Cat::Item, id, carried, reload: reserved == id, slot });
    }
    rows
}

/// The rows from the game state: the member's record and, for Sora, the forms he can use (`FUN_00282bf8` and
/// `FUN_00282c60` ask the drive forms, which only the leader has).
pub fn rows_of(p: &Progress, member: u16) -> Vec<Row> {
    let m = p.member(member).unwrap_or_default();
    build_rows(&Inputs {
        weapon: m.weapons[0],
        forms: if member == SORA { HAND_FORMS.iter().filter(|&&f| p.owns_form(f)).map(|&f| (f, p.form(f).weapon)).collect() } else { Vec::new() },
        armor: m.armor,
        accessories: m.accessories,
        items: p.battle_items(member),
    })
}

/// The list of a pick page (`FUN_00283a70(list, member + 1, filter)` and, for the Items page, `FUN_00283e50` and
/// the sort `FUN_002845a0`): every row of the item table, in table order, of the category's type that the player
/// owns at least one of; the Items page also lists the received consumables (the 8 ids of the "recent" list) that
/// the stock has run out of, with a count of 0.
pub fn candidates(cat: Cat, member: u16, order: &[ItemRow], exe: &ExeTables, count: &dyn Fn(u16) -> u8, received: &[u16]) -> Vec<Stack> {
    // `FUN_00283810`: the weapon pages take the bit the character's pair names (table 0x3823d0: Sora is 2), the
    // others a fixed bit of the type.
    let word = match cat {
        Cat::Weapon | Cat::Form => exe.filter_bits.iter().find(|p| u16::from(p.0) == member).map_or(1 << item_type::KEYBLADE, |p| 1u32 << (p.1 & 0x1f)),
        other => 1 << other.item_type(),
    };
    let mut list = stock(order, word, exe, count, &|_| false);
    if cat == Cat::Item {
        for &id in received {
            if id != 0 && !list.iter().any(|s| s.id == id) {
                list.push(Stack { id, count: 0 });
            }
        }
        let place = |id: u16| order.iter().position(|r| r.id == id).unwrap_or(usize::MAX);
        list.sort_by_key(|s| place(s.id));
    }
    list
}

/// A member's AP maximum, status `+0x18e` as `FUN_0019f920` builds it: the AP of his `lvup` row (`FUN_001b50c0`: the
/// table by member id, 14 -> 1 and 15 -> 6, the first table when the member's own is missing, the row of the level held
/// at the table's length), plus the record's AP byte (`+8`), plus the AP of the equipped items (`FUN_001a0c08`,
/// passed in as `gear`). A level of 0 counts as 1 [inferred, as `PartyData::member_status`].
pub fn member_ap_max(lvup: &[Vec<t::Lvup>], member: u16, level: u8, record_ap: u8, gear: i32) -> i32 {
    let k = match member {
        14 => 1,
        15 => 6,
        n @ 1..=13 => n as usize,
        _ => 1,
    };
    let table = lvup.get(k - 1).filter(|t| !t.is_empty()).or_else(|| lvup.first());
    let base = table.and_then(|t| t.get((level.max(1) as usize).min(t.len()) - 1)).map_or(0, |r| i32::from(r.ap));
    base + i32::from(record_ap) + gear
}

/// `FUN_0026ca28` (page 8 only): would swapping the accessory `current` for `new` leave the abilities more AP than
/// the character has? `max` is the AP total (`status +0x18e`), `used` the sum of the equipped abilities' costs and
/// the two numbers are the AP each item adds (`item_stat +7`). An empty slot (`current` 0) is never refused.
pub fn ap_short(current: u16, max: i32, used: i32, current_ap: i32, new_ap: i32) -> bool {
    current != 0 && (max - used) - (current_ap - new_ap) < 0
}

// --- the page machine ---------------------------------------------------------------------------------

/// The list results of docs/menus.md 1.7 that the Equipment pages use.
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
    /// -6: the Auto-Reload switch of a battle item row (`FUN_00269580`'s case -6).
    Toggle,
    /// -5: to the Abilities pages (page 2 goes to page 0x10).
    Switch,
    /// -7: the Help overlay (page 2 on an Items row or a form's row, pages 6 and 9).
    Help,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Effect {
    Sound(u32),
    Page(u16),
    /// Write the item into the slot (`FUN_0026c280`: the writer the category's jump table names); the caller then
    /// rebuilds the rows.
    Equip { cat: Cat, slot: usize, item: u16 },
    /// The item's Auto-Reload on or off (`FUN_0026c860`).
    Reload { item: u16, on: bool },
    /// The pick page was left: `FUN_0026c770` and `FUN_001877c8`.
    Settle,
    /// Back to page 1 (the character list of Items).
    Leave,
    /// -5: hand over to the Abilities pages (page 0x10).
    Switch,
    /// -10 / -11: the page shows the next or previous party member; the caller rebuilds the rows.
    Step { forward: bool },
    /// -7: open the Help overlay with a topic, to come back to the page.
    Help { topic: u8, ret: u16 },
    Close,
}

/// What the machine asks of the game.
pub trait Game {
    /// The list of a pick page.
    fn candidates(&self, cat: Cat) -> Vec<Stack>;
    /// `FUN_0026c260`: would the stock take the item (`FUN_001a18b8(id, 100, 1)`: room below 99).
    fn can_return(&self, id: u16) -> bool;
    /// `FUN_0026ca28`.
    fn ap_short(&self, current: u16, new: u16) -> bool;
    /// `FUN_00275288`: turning Auto-Reload off needs the question first.
    fn reload_question(&self, id: u16) -> bool;
    /// The party's size (`FUN_00282940`).
    fn members(&self) -> usize {
        1
    }
}

#[derive(Clone, Debug)]
pub struct Machine {
    pub page: u16,
    pub rows: Vec<Row>,
    /// Page 2's cursor and first visible row (remembered across the pick pages, `FUN_00252f98(2, ..)`).
    pub cursor: usize,
    pub scroll: usize,
    /// The pick list, its cursor (row 0 is the removal row of the Armor, Accessory and Items pages) and scroll.
    pub cands: Vec<Stack>,
    pub pick: usize,
    pub pick_scroll: usize,
    /// The question's cursor (0 No, 1 Yes).
    pub question: usize,
}

fn keep_visible(cursor: usize, scroll: &mut usize, rows: usize) {
    if cursor < *scroll {
        *scroll = cursor;
    } else if cursor >= *scroll + rows {
        *scroll = cursor + 1 - rows;
    }
}

impl Machine {
    pub fn new(rows: Vec<Row>) -> Machine {
        Machine { page: LIST, rows, cursor: 0, scroll: 0, cands: Vec::new(), pick: 0, pick_scroll: 0, question: 0 }
    }

    /// The rows changed under the cursor (a write): the cursor stays where it is, held to the list.
    pub fn reload(&mut self, rows: Vec<Row>) {
        self.rows = rows;
        self.cursor = self.cursor.min(self.rows.len().saturating_sub(1));
        keep_visible(self.cursor, &mut self.scroll, VISIBLE);
    }

    pub fn current(&self) -> Option<Row> {
        self.rows.get(self.cursor).copied()
    }

    /// The category the open pick page lists.
    pub fn cat(&self) -> Option<Cat> {
        Cat::of_page(self.page)
    }

    /// The rows of the pick page: the removal row first where the category has one.
    pub fn pick_len(&self) -> usize {
        match self.cat() {
            Some(c) => self.cands.len() + usize::from(c.has_remove()),
            None => 0,
        }
    }

    /// The item of a pick row (0 for the removal row, `None` past the end).
    pub fn pick_item(&self, row: usize) -> Option<u16> {
        let c = self.cat()?;
        if c.has_remove() {
            if row == 0 {
                return Some(0);
            }
            self.cands.get(row - 1).map(|s| s.id)
        } else {
            self.cands.get(row).map(|s| s.id)
        }
    }

    /// One key on the current page.
    pub fn press(&mut self, key: Key, game: &dyn Game) -> Vec<Effect> {
        use Effect::{Page, Sound};
        let mut fx = Vec::new();
        if key == Key::Close {
            if self.cat().is_some() {
                fx.push(Effect::Settle);
            }
            fx.push(Effect::Close);
            return fx;
        }
        let dir = |k: Key| match k {
            Key::Up { fresh } => Some((-1i8, fresh)),
            Key::Down { fresh } => Some((1, fresh)),
            _ => None,
        };
        match self.page {
            LIST => {
                if let Some((d, fresh)) = dir(key) {
                    let new = super::step_cursor(self.cursor, self.rows.len(), d, fresh);
                    if new != self.cursor {
                        self.cursor = new;
                        keep_visible(new, &mut self.scroll, VISIBLE);
                        fx.push(Sound(snd::CURSOR));
                        fx.push(Page(LIST));
                    }
                    return fx;
                }
                // `FUN_00269580`: -5 sets the target page 0x10 (sound kind 2).
                if key == Key::Switch {
                    fx.push(Sound(snd::CONFIRM));
                    fx.push(Effect::Switch);
                    return fx;
                }
                let Some(row) = self.current() else {
                    if key == Key::Cancel {
                        fx.push(Sound(snd::CANCEL));
                        fx.push(Effect::Leave);
                    }
                    return fx;
                };
                match key {
                    Key::Confirm => {
                        self.cands = game.candidates(row.cat);
                        self.pick = 0;
                        self.pick_scroll = 0;
                        self.page = row.cat.page();
                        fx.push(Sound(snd::CONFIRM));
                        fx.push(Page(self.page));
                    }
                    Key::Cancel => {
                        fx.push(Sound(snd::CANCEL));
                        fx.push(Effect::Leave);
                    }
                    // `FUN_00269580` cases -10 and -11 -> `FUN_0026cfc8`: step the character (-10 forward, -11 back, both
                    // wrap) and rebuild the page from the first row [inferred: the cursor sound and the reset].
                    // With one character the switch has nowhere to go.
                    Key::Prev | Key::Next => {
                        if game.members() < 2 {
                            fx.push(Sound(snd::REFUSED));
                        } else {
                            self.cursor = 0;
                            self.scroll = 0;
                            fx.push(Sound(snd::CURSOR));
                            fx.push(Effect::Step { forward: key == Key::Prev });
                        }
                    }
                    // `FUN_00269580` case -7: topic 0 on an Items row, topic 3 on a form's row; nothing elsewhere.
                    Key::Help if matches!(row.cat, Cat::Item | Cat::Form) => {
                        fx.push(Sound(snd::CONFIRM));
                        fx.push(Effect::Help { topic: if row.cat == Cat::Item { 0 } else { 3 }, ret: LIST });
                    }
                    // Only the Items rows allow the key (the mask's 0x200); the validity callback refuses an
                    // empty row.
                    Key::Toggle if row.cat == Cat::Item => {
                        if row.id == 0 {
                            fx.push(Sound(snd::REFUSED));
                        } else if row.reload && game.reload_question(row.id) {
                            self.question = 0;
                            self.page = QUESTION;
                            fx.push(Sound(snd::CONFIRM));
                            fx.push(Page(QUESTION));
                        } else {
                            fx.push(Sound(snd::CONFIRM));
                            fx.push(Effect::Reload { item: row.id, on: !row.reload });
                        }
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
                        self.page = LIST;
                        fx.push(Page(LIST));
                    }
                    Key::Confirm | Key::Cancel => {
                        self.page = LIST;
                        fx.push(Sound(snd::CANCEL));
                        fx.push(Page(LIST));
                    }
                    _ => {}
                }
            }
            NOTICE => {
                // `FUN_0026ab10`: the confirm and the cancel close the notice; the page below comes back.
                if matches!(key, Key::Confirm | Key::Cancel) {
                    // The notice only opens from the Accessory page.
                    self.page = Cat::Accessory.page();
                    fx.push(Sound(if key == Key::Confirm { snd::CONFIRM } else { snd::CANCEL }));
                    fx.push(Page(self.page));
                }
            }
            page if Cat::of_page(page).is_some() => {
                let Some(cat) = Cat::of_page(page) else { return fx };
                if let Some((d, fresh)) = dir(key) {
                    let new = super::step_cursor(self.pick, self.pick_len(), d, fresh);
                    if new != self.pick {
                        self.pick = new;
                        keep_visible(new, &mut self.pick_scroll, PICK_VISIBLE);
                        fx.push(Sound(snd::CURSOR));
                        fx.push(Page(page));
                    }
                    return fx;
                }
                match key {
                    Key::Confirm => {
                        let Some(item) = self.pick_item(self.pick) else { return fx };
                        let Some(row) = self.current() else { return fx };
                        // `FUN_0026d058`: the old item must fit back into the stock (pages 5 and 6 only when the
                        // list is not empty, the others when the slot holds something).
                        let must_fit = if cat.has_remove() { row.id != 0 } else { !self.cands.is_empty() };
                        if must_fit && !game.can_return(row.id) {
                            fx.push(Sound(snd::REFUSED));
                            return fx;
                        }
                        fx.push(Sound(snd::CONFIRM));
                        if cat == Cat::Accessory && game.ap_short(row.id, item) {
                            self.page = NOTICE;
                            fx.push(Page(NOTICE));
                            return fx;
                        }
                        // `FUN_0026c280`: nothing is written when the row already holds the item.
                        if row.id != item {
                            fx.push(Effect::Equip { cat, slot: row.slot, item });
                        }
                        fx.push(Effect::Settle);
                        self.page = LIST;
                        fx.push(Page(LIST));
                    }
                    Key::Cancel => {
                        fx.push(Sound(snd::CANCEL));
                        fx.push(Effect::Settle);
                        self.page = LIST;
                        fx.push(Page(LIST));
                    }
                    // `FUN_0026a528` case -7: page 9 (the Items list) topic 0, page 6 (the forms' weapons) topic 3.
                    Key::Help if matches!(cat, Cat::Item | Cat::Form) => {
                        fx.push(Sound(snd::CONFIRM));
                        fx.push(Effect::Help { topic: if cat == Cat::Item { 0 } else { 3 }, ret: page });
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

/// Group ids of `camp.2ld` sequence 1 (the tables at 0x3819e8 .. 0x381ab0).
mod group {
    pub const TAB: [usize; 3] = [0x4b, 0x4d, 0x4c];
    pub const HAND: usize = 0x19;
    pub const GLINT: [usize; 3] = [0x1b, 0x1d, 0x1c];
    pub const WINDOW: [usize; 2] = [0xe3, 0xe4];
    /// The item window of page 2 (enter, hold).
    pub const ITEM_WINDOW: [usize; 2] = [0x11f, 0x120];
    pub const INFO_OK: [usize; 3] = [6, 8, 7];
    pub const INFO_ASK: [usize; 3] = [9, 0xb, 0xa];
    pub const LIST_ROW: usize = 0x123;
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
    pub const TAG_LABEL: [f32; 2] = [-86.0, 1.0];
    pub const INFO_OK_TEXT: [f32; 2] = [-170.0, -4.0];
    pub const INFO_ASK_TEXT: [f32; 2] = [-210.0, -10.0];
    pub const INFO_OK_ROWS: [f32; 2] = [-30.0, 125.0];
    pub const INFO_ASK_ROWS: [f32; 2] = [-30.0, 98.0];
    pub const INFO_PITCH: f32 = 24.0;
    /// The right edge of the count of a pick row from the row's origin: the row plates' right edge (201 for
    /// the armor, accessory and item plates, 185 for the weapon plates) less a margin.
    pub const COUNT_RIGHT: f32 = 190.0;
}

/// The question's message (0x846a: bit 15 ignored) and the notice's (0xc22e).
const QUESTION_MESSAGE: u32 = 0x46a;
const NOTICE_MESSAGE: u32 = 0x422e;
/// The removal row's text (message 0x8448, "---").
const EMPTY: u32 = 0x448;

/// The executable's tables the pages read (docs/menus.md section 15).
#[derive(Clone, Debug, Default)]
pub struct EquipTables {
    /// 0x3819e8: the row plate of each category; 0x3819f8: of each form bit (0..5, 0xffff for none).
    pub plates: [u16; 5],
    pub form_plates: [u16; 6],
    /// 0x381a08 / 0x381a28: the tag plate of the first row of a category / of each form's row.
    pub tags: [u16; 5],
    pub form_tags: [u16; 6],
    /// 0x381a18 / 0x381a38: the category labels and the form labels (messages, bit 15 ignored).
    pub labels: [u32; 5],
    pub form_labels: [u32; 6],
    /// 0x381a48: the pick page's list window per category (enter, leave, hold).
    pub windows: [[u16; 3]; 5],
    /// 0x381a78: the item window of each pick page, with the picture (enter, leave, hold).
    pub pic_windows: [[u16; 3]; 5],
    /// 0x381a98: the pick row plate per category, idle then selected.
    pub pick_plates: [[u16; 2]; 5],
    /// 0x382460: the item type whose icon an empty row of the category shows (0xffff for the weapon).
    pub empty_types: [u16; 5],
}

impl EquipTables {
    pub fn read(exe: &Exe) -> Option<EquipTables> {
        let w = |va: u32| exe.u16(va);
        let msg = |va: u32| w(va).map(|v| u32::from(v & 0x7fff));
        let mut t = EquipTables::default();
        for k in 0..5u32 {
            let i = k as usize;
            t.plates[i] = w(0x0038_19e8 + 2 * k)?;
            t.tags[i] = w(0x0038_1a08 + 2 * k)?;
            t.labels[i] = msg(0x0038_1a18 + 2 * k)?;
            for e in 0..3u32 {
                t.windows[i][e as usize] = w(0x0038_1a48 + 6 * k + 2 * e)?;
                t.pic_windows[i][e as usize] = w(0x0038_1a78 + 6 * k + 2 * e)?;
            }
            t.pick_plates[i] = [w(0x0038_1a98 + 2 * k)?, w(0x0038_1a98 + 2 * (k + 5))?];
            t.empty_types[i] = w(0x0038_2460 + 2 * k)?;
        }
        for k in 0..6u32 {
            let i = k as usize;
            t.form_plates[i] = w(0x0038_19f8 + 2 * k)?;
            t.form_tags[i] = w(0x0038_1a28 + 2 * k)?;
        }
        for k in 0..5u32 {
            t.form_labels[k as usize] = msg(0x0038_1a38 + 2 * k)?;
        }
        Some(t)
    }

    /// The row plate of a row.
    fn plate(&self, r: &Row) -> usize {
        usize::from(if r.cat == Cat::Form { self.form_plates[(r.slot).min(5)] } else { self.plates[r.cat.index()] })
    }

    fn tag(&self, r: &Row) -> usize {
        usize::from(if r.cat == Cat::Form { self.form_tags[(r.slot).min(5)] } else { self.tags[r.cat.index()] })
    }

    fn label(&self, r: &Row) -> u32 {
        if r.cat == Cat::Form { self.form_labels[(r.slot).min(4)] } else { self.labels[r.cat.index()] }
    }
}

struct Ui {
    tabs: [usize; 3],
    window: usize,
    rows: Vec<usize>,
    tags: Vec<usize>,
    pick_window: usize,
    /// The item window with the picture (page 2: groups 0x11f, 0x120; the pick pages: `pic_windows`).
    item_window: usize,
    pick_rows: Vec<usize>,
    pick_counts: Vec<usize>,
    /// The NEW tag of each pick row (a child of the row; stage M7).
    pick_new: Vec<usize>,
    info: usize,
    info_text: usize,
    info_rows: Vec<usize>,
    hand: usize,
    glint: usize,
    help: usize,
    all: Vec<usize>,
}

#[derive(Resource, Default)]
pub struct EquipmentMenu {
    ready: bool,
    tables: Option<EquipTables>,
    machine: Option<Machine>,
    ui: Option<Ui>,
    held: (i8, f32),
    /// `lvup` by character (`FUN_001b50c0`'s tables), for a member's AP maximum.
    lvup: std::sync::Arc<Vec<Vec<t::Lvup>>>,
}

pub struct EquipmentPlugin;

impl Plugin for EquipmentPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<EquipmentMenu>()
            .add_systems(Startup, load.after(super::item_pages::load))
            .add_systems(PreUpdate, input.after(bevy::input::InputSystems).after(crate::harness::Keys).after(super::item_pages::input).before(super::input));
    }
}

fn load(disc: Res<Disc>, mut st: ResMut<EquipmentMenu>) {
    let Some(exe) = disc.read_root("SLUS_210.05").and_then(Exe::load) else {
        warn!("menu: the executable is not readable; the Equipment pages are off");
        return;
    };
    st.tables = EquipTables::read(&exe);
    if let Ok(mut a) = disc.0.lock() {
        st.lvup = std::sync::Arc::new(kh2_tables::subfile(&mut a, &t::LVUP).ok().and_then(|b| kh2_tables::rt::decode_lvup::<t::Lvup>(&b)).unwrap_or_default());
    }
    st.ready = st.tables.is_some();
    if !st.ready {
        warn!("menu: the executable's Equipment tables are not readable; the pages are off");
    }
}

/// What the game gives the machine: the stock, the AP numbers and the record.
struct Snapshot<'a> {
    p: &'a Progress,
    /// The member whose page is open.
    member: u16,
    /// The party's size.
    members: usize,
    lvup: &'a [Vec<t::Lvup>],
    items: &'a ItemsMenu,
    ab: Option<&'a Abilities>,
}

impl Snapshot<'_> {
    /// The AP an item adds (`item_stat +7`, through the ability rules' equipment table).
    fn item_ap(&self, id: u16) -> i32 {
        let Some(ab) = self.ab else { return 0 };
        let r = ab.rules();
        r.items.get(&id).and_then(|e| r.equip.get(e)).map_or(0, |e| i32::from(e.ap))
    }

    /// The AP the equipped abilities use (the sum of the costs of the slots with bit 15 set, `FUN_0026ca28`).
    fn ap_used(&self) -> i32 {
        let Some(ab) = self.ab else { return 0 };
        self.p.ability_slots(self.member).iter().filter(|s| s.2).filter_map(|s| ab.rules().abilities.get(&s.1)).map(|i| i32::from(i.ap)).sum()
    }

    fn ap_max(&self) -> i32 {
        if self.member == SORA {
            return self.ab.map_or(0, |ab| i32::from(crate::menu_abilities::ap_max(ab, self.p)));
        }
        let m = self.p.member(self.member).unwrap_or_default();
        let gear: i32 = m.weapons.iter().chain(&m.armor).chain(&m.accessories).filter(|&&i| i != 0).map(|&i| self.item_ap(i)).sum();
        member_ap_max(self.lvup, self.member, m.level, m.ap, gear)
    }
}

impl Game for Snapshot<'_> {
    fn candidates(&self, cat: Cat) -> Vec<Stack> {
        let Some(exe) = self.items.exe.as_ref() else { return Vec::new() };
        candidates(cat, self.member, &self.items.order, exe, &|id| self.p.item_count(id), &self.p.recent_items())
    }
    fn can_return(&self, id: u16) -> bool {
        self.p.room(id, INVENTORY, 1) != 0
    }
    fn ap_short(&self, current: u16, new: u16) -> bool {
        ap_short(current, self.ap_max(), self.ap_used(), self.item_ap(current), self.item_ap(new))
    }
    fn reload_question(&self, id: u16) -> bool {
        self.p.reload_off_needs_question(self.member, id)
    }
    fn members(&self) -> usize {
        self.members
    }
}

/// The party member id of the row whose equipment page is open.
fn member_of(items: &ItemsMenu) -> u16 {
    items.party.get(items.member).copied().unwrap_or(SORA)
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
    if any(&[KeyCode::KeyX]) {
        out.push(Key::Switch);
    }
    if any(&[KeyCode::Tab]) {
        out.push(Key::Help);
    }
    out
}

fn name_of(items: &ItemsMenu, scene: &MenuScene, id: u16) -> String {
    if id == 0 {
        return "---".into();
    }
    items.rows.get(&id).and_then(|r| scene.text.as_ref()?.messages.as_ref()?.text(u32::from(r.name))).unwrap_or_else(|| format!("item {id}"))
}

/// The live weapon and the numbers the equipment feeds (`FUN_002a1060`'s queue and `FUN_00282898`'s status
/// refresh in the original): the app's `Abilities` rebuilds its block from the equipment.
fn refresh(p: &Progress, abilities: Option<&mut Abilities>) {
    let Some(ab) = abilities else { return };
    ab.equipment = p.equipment();
    ab.recompute();
    let r = ab.rules();
    let grants: Vec<String> = ab
        .equipment
        .iter()
        .filter_map(|i| r.items.get(i).and_then(|e| r.equip.get(e)).map(|e| (*i, e.ability)))
        .filter(|&(_, a)| a != 0)
        .map(|(i, a)| format!("item {i} grants ability {a} ({})", if ab.has(a) { "applied" } else { "not applied" }))
        .collect();
    info!("menu: abilities refreshed: equipment {:?}, AP {}/{}, {}", ab.equipment, ab.ap_used(), crate::menu_abilities::ap_max(ab, p), grants.join(", "));
}

#[allow(clippy::too_many_arguments)]
fn input(
    mut st: ResMut<EquipmentMenu>,
    mut items: ResMut<ItemsMenu>,
    mut menu: ResMut<Menu>,
    mut scene: ResMut<MenuScene>,
    mut keys: ResMut<ButtonInput<KeyCode>>,
    mut se: MessageWriter<PlaySe>,
    mut progress: Option<ResMut<Progress>>,
    mut abilities: Option<ResMut<Abilities>>,
    real: Res<Time<Real>>,
    mut ntags: ResMut<NewTags>,
    mut pics: ResMut<Pictures>,
) {
    if !st.ready {
        return;
    }
    if st.machine.is_some() && matches!(menu.phase, Phase::Closing | Phase::Closed) {
        st.machine = None;
        st.held = (0, 0.0);
        return;
    }
    // The Help overlay has the keys.
    if menu.page == help::PAGE {
        return;
    }
    let Some(p) = progress.as_deref() else { return };
    if st.machine.is_none() {
        // Page 1's confirm on a member's row (`FUN_0025ae18`): the Items module asks for page 2.
        if !items.take_equipment_request() {
            return;
        }
        let rows = rows_of(p, member_of(&items));
        st.machine = Some(Machine::new(rows));
        st.held = (0, 0.0);
        ensure_ui(&mut st, &mut scene);
        menu.page = LIST;
        info!("menu: page {LIST:#x}");
        keys.reset_all();
        redraw(&mut st, &items, &mut scene, p, &mut ntags, &mut pics);
        return;
    }
    let mut held = st.held;
    let pressed = keys_of(&keys, &mut held, real.delta_secs());
    st.held = held;
    let mut close = false;
    let lvup = st.lvup.clone();
    for key in pressed {
        let Some(p) = progress.as_deref() else { break };
        let member = member_of(&items);
        let game = Snapshot { p, member, members: items.party.len(), lvup: &lvup, items: &items, ab: abilities.as_deref() };
        let fx = st.machine.as_mut().map(|m| m.press(key, &game)).unwrap_or_default();
        let mut rebuild = false;
        for e in fx {
            match e {
                Effect::Sound(n) => {
                    crate::menu::menu_sound::play(&mut se, n);
                }
                Effect::Page(page) => {
                    menu.page = page;
                    let extra = st.machine.as_ref().filter(|_| Cat::of_page(page).is_some()).map(|m| format!(" ({} candidates)", m.cands.len())).unwrap_or_default();
                    info!("menu: page {page:#x}{extra}");
                }
                Effect::Equip { cat, slot, item } => {
                    if let Some(p) = progress.as_deref_mut() {
                        let member = member_of(&items);
                        let before = p.member(member).unwrap_or_default();
                        let old = match cat {
                            Cat::Weapon => before.weapons[0],
                            Cat::Form => p.form(slot as u8).weapon,
                            Cat::Armor => before.armor.get(slot).copied().unwrap_or(0),
                            Cat::Accessory => before.accessories.get(slot).copied().unwrap_or(0),
                            Cat::Item => before.items.get(slot).copied().unwrap_or(0),
                        };
                        let (old_n, new_n) = (p.item_count(old), p.item_count(item));
                        let changed = match cat {
                            Cat::Weapon => p.equip_weapon(member, item),
                            Cat::Form => p.equip_form_weapon(slot as u8, item),
                            Cat::Armor => p.equip_armor(member, slot, item),
                            Cat::Accessory => p.equip_accessory(member, slot, item),
                            // The row's flag byte is the candidate's count (`FUN_0026c698`: carried when the stock
                            // has one).
                            Cat::Item => p.equip_battle_item(member, slot, item, item == 0 || new_n > 0),
                        };
                        info!(
                            "menu: equipped {} slot {slot}: {} (stock {old_n} -> {}) replaced by {} (stock {new_n} -> {}), changed {changed}",
                            ["weapon", "form weapon", "armor", "accessory", "item"][cat.index()],
                            name_of(&items, &scene, old),
                            p.item_count(old),
                            name_of(&items, &scene, item),
                            p.item_count(item),
                        );
                        // The live Abilities block is Sora's.
                        if member == SORA {
                            refresh(p, abilities.as_deref_mut());
                        }
                        rebuild = true;
                    }
                }
                Effect::Reload { item, on } => {
                    if let Some(p) = progress.as_deref_mut() {
                        let member = member_of(&items);
                        p.set_auto_reload(member, item, on);
                        info!("menu: auto-reload item {item} {} (slots {:?})", if on { "ON" } else { "OFF" }, p.battle_items(member));
                        rebuild = true;
                    }
                }
                Effect::Settle => {
                    if let Some(p) = progress.as_deref_mut() {
                        let member = member_of(&items);
                        p.settle_battle_items(member);
                        info!("menu: battle item slots {:?}", p.battle_items(member));
                        rebuild = true;
                    }
                }
                Effect::Leave => {
                    leave(&mut st, &mut items, &mut menu, &mut scene);
                    menu.page = super::item_pages::MEMBERS;
                    info!("menu: page {:#x}", super::item_pages::MEMBERS);
                }
                Effect::Switch => {
                    // The list mask has 0x100 only while the Abilities row is on page 0.
                    if menu.rows_has(1) {
                        if let Some(ui) = &st.ui {
                            for &i in &ui.all {
                                scene.nodes[i].visible = false;
                            }
                        }
                        st.machine = None;
                        super::item_pages::hand_over(&mut items, &mut menu, &mut scene);
                        menu.switch = Some(Switch::ToAbilities { second: true });
                        info!("menu: switch to Abilities (page 0x10)");
                    }
                }
                Effect::Step { forward } => {
                    let n = items.party.len().max(1);
                    let row = if forward { (items.member + 1) % n } else { (items.member + n - 1) % n };
                    items.set_member(row);
                    info!("menu: equipment of member {}", member_of(&items));
                    rebuild = true;
                }
                Effect::Help { topic, ret } => {
                    menu.help = Some(help::Request { topic, ret });
                }
                Effect::Close => {
                    leave(&mut st, &mut items, &mut menu, &mut scene);
                    super::item_pages::leave(&mut items, &mut menu, &mut scene);
                    close = true;
                }
            }
            if st.machine.is_none() {
                break;
            }
        }
        if rebuild
            && let (Some(p), Some(m)) = (progress.as_deref(), st.machine.as_mut())
        {
            m.reload(rows_of(p, member_of(&items)));
        }
        if st.machine.is_none() {
            break;
        }
    }
    if close {
        return;
    }
    if st.machine.is_some()
        && let Some(p) = progress.as_deref()
    {
        redraw(&mut st, &items, &mut scene, p, &mut ntags, &mut pics);
    }
    keys.reset_all();
}

fn ensure_ui(st: &mut EquipmentMenu, scene: &mut MenuScene) {
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
    let window = mk(scene, S);
    let rows: Vec<usize> = (0..VISIBLE).map(|_| mk(scene, S)).collect();
    let tags: Vec<usize> = (0..VISIBLE).map(|_| mk(scene, S)).collect();
    let pick_window = mk(scene, S);
    let item_window = mk(scene, S);
    let pick_rows: Vec<usize> = (0..PICK_VISIBLE).map(|_| mk(scene, S)).collect();
    let pick_counts: Vec<usize> = (0..PICK_VISIBLE).map(|_| mk(scene, None)).collect();
    let pick_new: Vec<usize> = (0..PICK_VISIBLE).map(|_| mk(scene, S)).collect();
    let info = mk(scene, S);
    let info_text = mk(scene, None);
    let info_rows: Vec<usize> = (0..2).map(|_| mk(scene, S)).collect();
    let hand = mk(scene, S);
    let glint = mk(scene, S);
    let help = mk(scene, None);
    for (&r, &t) in rows.iter().zip(&tags) {
        scene.nodes[r].parent = Some(window);
        scene.nodes[t].parent = Some(r);
    }
    for ((&r, &c), &n) in pick_rows.iter().zip(&pick_counts).zip(&pick_new) {
        scene.nodes[r].parent = Some(pick_window);
        scene.nodes[c].parent = Some(r);
        scene.nodes[n].parent = Some(r);
    }
    scene.nodes[info_text].parent = Some(info);
    for &r in &info_rows {
        scene.nodes[r].parent = Some(info);
    }
    st.ui = Some(Ui { tabs, window, rows, tags, pick_window, item_window, pick_rows, pick_counts, pick_new, info, info_text, info_rows, hand, glint, help, all });
}

/// Hides this module's nodes and gives the keys back to the Items module (page 1 redraws itself).
fn leave(st: &mut EquipmentMenu, items: &mut ItemsMenu, _menu: &mut Menu, scene: &mut MenuScene) {
    if let Some(ui) = &st.ui {
        for &i in &ui.all {
            scene.nodes[i].visible = false;
        }
    }
    st.machine = None;
    items.suspended = false;
}

/// A row's text: its icon and name (`FUN_00284330`: an empty row shows the category's empty icon and "---"; a
/// battle item with Auto-Reload on shows the equipped icon of its type, `FUN_00255520`).
fn row_tokens(text: &crate::menu_text::MenuText, t: &EquipTables, items: &ItemsMenu, cat: Cat, id: u16, reload: bool) -> Vec<Token> {
    let exe = items.exe.clone().unwrap_or_default();
    match items.rows.get(&id).filter(|_| id != 0) {
        Some(r) => {
            let icon = if reload { exe.icon(r.kind, true) } else { r.icon };
            let mut v = vec![Token::Icon(icon)];
            v.extend(tokens_of(text, u32::from(r.name)));
            v
        }
        None => {
            let ty = t.empty_types[cat.index()];
            let mut v = vec![Token::Icon(exe.icon(if ty == 0xffff { 0xff } else { ty as u8 }, false))];
            v.extend(tokens_of(text, EMPTY));
            v
        }
    }
}

/// Where the item picture goes in a window group: the picture is 128 by 128 (`itempic/item-NNN.imd`, drawn at 100 % by
/// `FUN_002830f8` when the window has no frame group), centred across the window's last-frame box and 10 below its top
/// [inferred: the sprite's anchor was not read; clip 6 shows it near the top of the window].
fn picture_rect(scene: &MenuScene, group: usize) -> [f32; 4] {
    let Some(seq) = scene.seqs.get(1) else { return [0.0; 4] };
    let quads = seq.quads(group, seq.last_frame(group), [0.0, 0.0], &Default::default());
    let (mut x0, mut x1, mut y0) = (f32::MAX, f32::MIN, f32::MAX);
    for p in quads.iter().flat_map(|q| q.pos) {
        x0 = x0.min(p[0]);
        x1 = x1.max(p[0]);
        y0 = y0.min(p[1]);
    }
    let cx = (x0 + x1) / 2.0;
    [cx - 64.0, y0 + 10.0, cx + 64.0, y0 + 138.0]
}

fn redraw(st: &mut EquipmentMenu, items: &ItemsMenu, scene: &mut MenuScene, p: &Progress, ntags: &mut NewTags, pics: &mut Pictures) {
    let Some(m) = st.machine.clone() else { return };
    let (Some(ui), Some(t)) = (st.ui.as_ref(), st.tables.clone()) else { return };
    let (tabs, window, rows, tags, pick_window, pick_rows, pick_counts, hand, glint, help) =
        (ui.tabs, ui.window, ui.rows.clone(), ui.tags.clone(), ui.pick_window, ui.pick_rows.clone(), ui.pick_counts.clone(), ui.hand, ui.glint, ui.help);
    let (info, info_text, info_rows) = (ui.info, ui.info_text, ui.info_rows.clone());
    let pick_new = ui.pick_new.clone();
    let item_window = ui.item_window;
    let all = ui.all.clone();
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
    let item_help = |id: u16| items.rows.get(&id).filter(|_| id != 0).map(|r| u32::from(r.description));

    // The header tabs: Items, the character, and on a pick page the category's name [inferred: as the Abilities
    // pages' second level].
    let pick_cat = Cat::of_page(page).or(if page == NOTICE { Some(Cat::Accessory) } else { None });
    let entry_name = items.entry_name();
    let tab_count = if pick_cat.is_some() { 3 } else { 2 };
    for (k, &tab) in tabs.iter().enumerate().take(tab_count) {
        let prev = was_of(tab);
        scene.nodes[tab].pos = [at::TAB_PITCH * k as f32, 0.0];
        set(scene, tab, group::TAB[0], Some(group::TAB[2]), prev);
        let tokens = match k {
            0 => tokens_of(&text, entry_name),
            1 => super::item_pages::member_tokens(&text, &items.member_names, member_of(items)),
            _ => {
                let row = m.current();
                let msg = match (pick_cat, row) {
                    (Some(Cat::Form), Some(r)) => t.form_labels[r.slot.min(4)],
                    (Some(c), _) => t.labels[c.index()],
                    _ => 0,
                };
                tokens_of(&text, msg)
            }
        };
        scene.nodes[tab].label_anchor = Some(at::TAB_LABEL);
        scene.nodes[tab].label = Some(label(&text, tokens, 1, 15.0, at::COLOR, 0, [0.0; 2]));
    }

    let on_pick = pick_cat.is_some() && page != NOTICE;
    if matches!(page, LIST | QUESTION) {
        // The slot list (`FUN_00269028`): the window and nine rows with the tag of a category's first row.
        set(scene, window, group::WINDOW[0], Some(group::WINDOW[1]), was_of(window));
        // The item window on the right (`FUN_00283018` + `FUN_0026b1c8`): groups 0x11f then 0x120, with the picture of
        // the highlighted row's item (an empty row has none).
        set(scene, item_window, group::ITEM_WINDOW[0], Some(group::ITEM_WINDOW[1]), was_of(item_window));
        match m.current().and_then(|r| items.pictures.get(&r.id)).copied().filter(|&p| p >= 0 && m.current().is_some_and(|r| r.id != 0)) {
            Some(pic) => pics.show(item_window, pic as u16, picture_rect(scene, group::ITEM_WINDOW[0])),
            None => pics.clear(item_window),
        }
        let mut prev_cat: Option<(Cat, usize)> = None;
        if m.scroll > 0 {
            let r = &m.rows[m.scroll - 1];
            prev_cat = Some((r.cat, r.slot));
        }
        for k in 0..VISIBLE {
            let idx = m.scroll + k;
            let Some(r) = m.rows.get(idx) else { break };
            let selected = idx == m.cursor && page == LIST;
            scene.nodes[rows[k]].off = [0.0, at::ROW_PITCH * k as f32];
            set(scene, rows[k], t.plate(r), None, was_of(rows[k]));
            scene.nodes[rows[k]].label_anchor = Some(at::LABEL);
            scene.nodes[rows[k]].label = Some(label(&text, row_tokens(&text, &t, items, r.cat, r.id, r.reload && r.cat == Cat::Item), 1, 15.0, if selected { at::SELECTED } else { at::COLOR }, 0, [0.0; 2]));
            // The tag: the first row of a category, and every form's row (`FUN_00269028` / `FUN_0026adc0`).
            let first = r.cat == Cat::Form || prev_cat.map(|p| p.0) != Some(r.cat);
            if first && t.tag(r) != 0xffff {
                set(scene, tags[k], t.tag(r), None, was_of(tags[k]));
                scene.nodes[tags[k]].label_anchor = Some(at::TAG_LABEL);
                scene.nodes[tags[k]].label = Some(label(&text, tokens_of(&text, t.label(r)), 1, 13.0, at::COLOR, 0, [0.0; 2]));
            }
            prev_cat = Some((r.cat, r.slot));
        }
        if let Some(&sel) = rows.get(m.cursor.saturating_sub(m.scroll)) {
            scene.nodes[hand].parent = Some(sel);
            scene.nodes[hand].off = at::HAND;
            set(scene, hand, group::HAND, None, page != LIST && was_of(hand));
            if page == LIST {
                scene.nodes[glint].parent = Some(sel);
                scene.nodes[glint].off = at::GLINT;
                set(scene, glint, group::GLINT[0], Some(group::GLINT[2]), was_of(glint));
            }
        }
        // The help of a slot row is its category's: the message after the label ("Change Armor." after "Armor",
        // "Change Sub-Weapon used in Valor Form." after "Valor"; the system messages hold them as consecutive
        // pairs and nothing else uses them) [inferred: the list's help table builder was not read].
        set_help(scene, &text, help, if page == LIST { m.current().map(|r| t.label(&r) + 1) } else { None });
    }

    if on_pick || page == NOTICE {
        let cat = pick_cat.unwrap_or(Cat::Accessory);
        let w = t.windows[cat.index()];
        set(scene, pick_window, usize::from(w[0]), Some(usize::from(w[2])), was_of(pick_window));
        let pw = t.pic_windows[cat.index()];
        set(scene, item_window, usize::from(pw[0]), Some(usize::from(pw[2])), was_of(item_window));
        match m.pick_item(m.pick).and_then(|id| items.pictures.get(&id)).copied().filter(|&p| p >= 0 && m.pick_item(m.pick) != Some(0)) {
            Some(pic) => pics.show(item_window, pic as u16, picture_rect(scene, usize::from(pw[0]))),
            None => pics.clear(item_window),
        }
        let len = m.pick_len();
        let remove = cat.has_remove();
        let [idle, selected_plate] = t.pick_plates[cat.index()];
        for k in 0..PICK_VISIBLE {
            let idx = m.pick_scroll + k;
            if idx >= len {
                break;
            }
            let sel = idx == m.pick && on_pick;
            scene.nodes[pick_rows[k]].off = [0.0, at::ROW_PITCH * k as f32];
            set(scene, pick_rows[k], usize::from(if sel { selected_plate } else { idle }), None, false);
            scene.nodes[pick_rows[k]].label_anchor = Some(at::LABEL);
            let (tokens, count) = if remove && idx == 0 {
                (row_tokens(&text, &t, items, cat, 0, false), None)
            } else {
                let s = m.cands[idx - usize::from(remove)];
                // `FUN_0026bd08`: a fresh candidate carries the tag; `FUN_0026c068`: the cursor on it sets its seen bit.
                if sel {
                    ntags.view(s.id);
                }
                if let Some(g) = ntags.cell_group(s.id) {
                    scene.nodes[pick_new[k]].off = scene.extent(1, usize::from(idle));
                    set(scene, pick_new[k], g, None, false);
                }
                (row_tokens(&text, &t, items, cat, s.id, false), count_text(s.count))
            };
            scene.nodes[pick_rows[k]].label = Some(label(&text, tokens, 1, 15.0, if sel { at::SELECTED } else { at::COLOR }, 0, [0.0; 2]));
            if let Some(c) = count {
                let id = pick_counts[k];
                scene.nodes[id].visible = true;
                let right = if matches!(cat, Cat::Weapon | Cat::Form) { at::COUNT_RIGHT - 16.0 } else { at::COUNT_RIGHT };
                scene.nodes[id].label = Some(label(&text, text.literal(&c, Style::default(), [0.0; 2]).tokens, 1, 15.0, if sel { at::SELECTED } else { at::COLOR }, 1, [right, 1.0]));
            }
        }
        if let Some(&sel) = pick_rows.get(m.pick.saturating_sub(m.pick_scroll)) {
            scene.nodes[hand].parent = Some(sel);
            scene.nodes[hand].off = at::HAND;
            set(scene, hand, group::HAND, None, false);
        }
        let desc = m.pick_item(m.pick).and_then(item_help);
        set_help(scene, &text, help, if on_pick { desc } else { None });
    }

    // The question and the notice: the information window (`FUN_002532d0`, `FUN_00253128`).
    if matches!(page, QUESTION | NOTICE) {
        let ask = page == QUESTION;
        let g = if ask { group::INFO_ASK } else { group::INFO_OK };
        set(scene, info, g[0], Some(g[2]), was_of(info));
        let n = &mut scene.nodes[info_text];
        n.visible = true;
        n.label_anchor = Some(if ask { at::INFO_ASK_TEXT } else { at::INFO_OK_TEXT });
        let msg = tokens_of(&text, if ask { QUESTION_MESSAGE } else { NOTICE_MESSAGE });
        n.label = Some(label(&text, fill(msg, &[]), 1, 15.0, at::COLOR, 0, [0.0; 2]));
        let exe = items.exe.clone().unwrap_or_default();
        let lines: Vec<(u32, bool)> = if ask { vec![(exe.no, m.question == 0), (exe.yes, m.question == 1)] } else { vec![(exe.ok, true)] };
        let [px, py] = if ask { at::INFO_ASK_ROWS } else { at::INFO_OK_ROWS };
        for (k, &(msg, sel)) in lines.iter().enumerate() {
            let r = info_rows[k];
            set(scene, r, group::LIST_ROW, None, was_of(r));
            scene.nodes[r].off = [px, py + at::INFO_PITCH * k as f32];
            scene.nodes[r].label_anchor = Some(at::LABEL);
            scene.nodes[r].label = Some(label(&text, tokens_of(&text, msg), 1, 15.0, if sel { at::SELECTED } else { at::COLOR }, 0, [0.0; 2]));
            if sel {
                scene.nodes[hand].parent = Some(r);
                scene.nodes[hand].off = at::HAND;
                set(scene, hand, group::HAND, None, false);
            }
        }
        set_help(scene, &text, help, None);
    }
    let _ = (p, pick_counts);
    scene.text = Some(text);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(id: u16, kind: u8) -> ItemRow {
        ItemRow { id, kind, amount: 0, name: 0, description: 0, sell: 1, icon: 0 }
    }

    fn exe() -> ExeTables {
        ExeTables { filter_bits: vec![(1, 2), (2, 3), (3, 4)], ..Default::default() }
    }

    /// Item table: keyblades 10, 11, 12; armor 20, 21; accessories 30, 31; consumables 1, 2, 3.
    fn table() -> Vec<ItemRow> {
        vec![row(1, 0), row(2, 0), row(3, 0), row(10, 2), row(11, 2), row(12, 2), row(20, 14), row(21, 14), row(30, 15), row(31, 15)]
    }

    fn inputs() -> Inputs {
        Inputs { weapon: 10, forms: vec![(1, 11), (3, 0), (4, 12)], armor: vec![20, 0], accessories: vec![30], items: vec![(1, 1), (0, 2), (3, 0), (0, 0)] }
    }

    #[test]
    fn the_rows_are_the_weapon_the_forms_the_armor_the_accessories_and_the_battle_items() {
        let rows = build_rows(&inputs());
        let cats: Vec<Cat> = rows.iter().map(|r| r.cat).collect();
        assert_eq!(
            cats,
            [Cat::Weapon, Cat::Form, Cat::Form, Cat::Form, Cat::Armor, Cat::Armor, Cat::Accessory, Cat::Item, Cat::Item, Cat::Item, Cat::Item]
        );
        // The forms' rows carry the form number, an empty hand is a row too.
        assert_eq!(rows[1..4].iter().map(|r| (r.slot, r.id, r.carried)).collect::<Vec<_>>(), [(1, 11, true), (3, 0, false), (4, 12, true)]);
        assert_eq!(rows[5], Row { cat: Cat::Armor, id: 0, carried: false, reload: false, slot: 1 });
        // Battle items: carried with its reservation (on), reserved but empty (shows the reserved item, not carried,
        // on), carried without a reservation (off), empty with nothing reserved (the flag compares 0 with 0: set).
        let items: Vec<(u16, bool, bool)> = rows[7..].iter().map(|r| (r.id, r.carried, r.reload)).collect();
        assert_eq!(items, [(1, true, true), (2, false, true), (3, true, false), (0, false, true)]);
    }

    #[test]
    fn the_real_pair_table_gives_each_member_a_weapon_type() {
        // Table 0x3823d0 of the user's executable (skipped without KH2_ISO): Sora 2 (keyblade), Donald 3, Goofy 4.
        let Ok(iso) = std::env::var("KH2_ISO") else { return };
        let Ok(mut a) = kh2_archive::Archive::open(&iso) else { return };
        let Some(elf) = ["SLUS_210.05", "SLUS_210.05;1"].iter().find_map(|p| a.iso().read(p).ok()) else { return };
        let exe = ExeTables::read(&Exe::load(elf).expect("an ELF")).expect("the Items tables");
        let pair = |m: u16| exe.filter_bits.iter().find(|p| u16::from(p.0) == m).map(|p| p.1);
        assert_eq!(pair(SORA), Some(2));
        assert_eq!(pair(crate::progress::DONALD), Some(3));
        assert_eq!(pair(crate::progress::GOOFY), Some(4));
        // And the pick list takes the page's bit: a staff row (type 3) is Donald's, a shield row (type 4) Goofy's.
        let table = vec![row(10, 2), row(40, 3), row(50, 4)];
        let ids = |m| candidates(Cat::Weapon, m, &table, &exe, &|_| 1, &[]).iter().map(|s| s.id).collect::<Vec<_>>();
        assert_eq!((ids(SORA), ids(crate::progress::DONALD), ids(crate::progress::GOOFY)), (vec![10], vec![40], vec![50]));
    }

    #[test]
    fn a_members_ap_maximum_is_his_lvup_row_plus_the_record_byte_plus_the_gear() {
        let row = |ap: u8| t::Lvup { exp: 0, strength: 0, magic: 0, defense: 0, ap, sword_ability: 0, shield_ability: 0, staff_ability: 0, padding: 0 };
        let lvup = vec![vec![row(3), row(5)], vec![row(7), row(9), row(11)], Vec::new()];
        assert_eq!(member_ap_max(&lvup, 2, 2, 1, 4), 9 + 1 + 4, "Donald: table 1, level 2");
        assert_eq!(member_ap_max(&lvup, 2, 9, 0, 0), 11, "past the end: the last row");
        assert_eq!(member_ap_max(&lvup, 3, 1, 0, 0), 3, "an empty table falls back to the first");
        assert_eq!(member_ap_max(&lvup, 1, 0, 0, 0), 3, "level 0 counts as 1");
    }

    #[test]
    fn a_members_weapon_page_lists_the_type_the_pair_table_names() {
        // Table 0x3823d0 pairs the character with a weapon type: Sora 2 (keyblades), Donald 3 (staffs), Goofy 4 (shields).
        let table = vec![row(10, 2), row(40, 3), row(41, 3), row(50, 4)];
        let own = |_: u16| 1;
        let ids = |l: Vec<Stack>| l.iter().map(|s| s.id).collect::<Vec<_>>();
        let e = exe();
        assert_eq!(ids(candidates(Cat::Weapon, SORA, &table, &e, &own, &[])), [10]);
        assert_eq!(ids(candidates(Cat::Weapon, crate::progress::DONALD, &table, &e, &own, &[])), [40, 41]);
        assert_eq!(ids(candidates(Cat::Weapon, crate::progress::GOOFY, &table, &e, &own, &[])), [50]);
    }

    #[test]
    fn the_character_keys_step_the_member_and_restart_the_list_when_there_are_several() {
        let mut g = game(&[]);
        g.members = 3;
        let mut m = machine();
        m.cursor = 4;
        let fx = m.press(Key::Prev, &g);
        assert_eq!(fx, vec![Effect::Sound(snd::CURSOR), Effect::Step { forward: true }], "-10 steps forward (FUN_0026cfc8(1))");
        assert_eq!((m.cursor, m.scroll), (0, 0));
        assert_eq!(m.press(Key::Next, &g), vec![Effect::Sound(snd::CURSOR), Effect::Step { forward: false }]);
    }

    #[test]
    fn the_pick_lists_hold_the_owned_items_of_the_type_with_the_received_ones_at_zero() {
        let own = |id: u16| match id {
            10 | 12 | 20 | 30 | 31 | 1 | 3 => 1,
            21 => 2,
            _ => 0,
        };
        let ids = |l: Vec<Stack>| l.iter().map(|s| (s.id, s.count)).collect::<Vec<_>>();
        let (t, e) = (table(), exe());
        assert_eq!(ids(candidates(Cat::Weapon, SORA, &t, &e, &own, &[])), [(10, 1), (12, 1)]);
        assert_eq!(ids(candidates(Cat::Form, SORA, &t, &e, &own, &[])), [(10, 1), (12, 1)]);
        assert_eq!(ids(candidates(Cat::Armor, SORA, &t, &e, &own, &[])), [(20, 1), (21, 2)]);
        assert_eq!(ids(candidates(Cat::Accessory, SORA, &t, &e, &own, &[])), [(30, 1), (31, 1)]);
        // The consumables, then a received item the stock ran out of, in table order; a held one is not added twice.
        assert_eq!(ids(candidates(Cat::Item, SORA, &t, &e, &own, &[2, 3, 0])), [(1, 1), (2, 0), (3, 1)]);
    }

    #[test]
    fn an_accessory_swap_is_refused_only_when_the_ap_would_fall_below_the_use() {
        // 5 AP, 4 used, the held accessory gives 2: swapping for one that gives 0 leaves 3 < 4.
        assert!(ap_short(30, 5, 4, 2, 0));
        assert!(!ap_short(30, 5, 4, 2, 1), "1 AP less leaves 4, which is enough");
        assert!(!ap_short(30, 5, 3, 2, 0));
        assert!(!ap_short(0, 5, 9, 0, 0), "an empty slot is never refused");
        assert!(!ap_short(30, 5, 4, 0, 0));
    }

    struct G {
        cands: Vec<Stack>,
        fits: bool,
        short: bool,
        question: bool,
        members: usize,
    }

    impl Game for G {
        fn candidates(&self, _cat: Cat) -> Vec<Stack> {
            self.cands.clone()
        }
        fn can_return(&self, _id: u16) -> bool {
            self.fits
        }
        fn ap_short(&self, _c: u16, _n: u16) -> bool {
            self.short
        }
        fn reload_question(&self, _id: u16) -> bool {
            self.question
        }
        fn members(&self) -> usize {
            self.members
        }
    }

    fn game(cands: &[(u16, u8)]) -> G {
        G { cands: cands.iter().map(|&(id, count)| Stack { id, count }).collect(), fits: true, short: false, question: false, members: 1 }
    }

    fn machine() -> Machine {
        Machine::new(build_rows(&inputs()))
    }

    fn goto(m: &mut Machine, row: usize) {
        m.cursor = row;
    }

    #[test]
    fn a_confirm_opens_the_categorys_page_and_a_cancel_comes_back_to_the_list() {
        let g = game(&[(20, 1), (21, 2)]);
        let mut m = machine();
        goto(&mut m, 4);
        let fx = m.press(Key::Confirm, &g);
        assert_eq!(m.page, 7, "the Armor page is 7");
        assert!(fx.contains(&Effect::Page(7)));
        assert_eq!(m.pick_len(), 3, "the removal row and two items");
        assert_eq!(m.pick_item(0), Some(0));
        assert_eq!(m.pick_item(2), Some(21));
        let fx = m.press(Key::Cancel, &g);
        assert_eq!(m.page, LIST);
        assert!(fx.contains(&Effect::Settle) && fx.contains(&Effect::Page(LIST)));
        // The weapon and the forms have no removal row.
        goto(&mut m, 0);
        m.press(Key::Confirm, &g);
        assert_eq!((m.page, m.pick_len(), m.pick_item(0)), (5, 2, Some(20)));
        m.press(Key::Cancel, &g);
        goto(&mut m, 2);
        m.press(Key::Confirm, &g);
        assert_eq!(m.page, 6);
    }

    #[test]
    fn picking_a_row_equips_it_in_the_slot_of_the_cursor_and_returns_to_the_list() {
        let g = game(&[(20, 1), (21, 2)]);
        let mut m = machine();
        goto(&mut m, 5);
        m.press(Key::Confirm, &g);
        m.press(Key::Down { fresh: true }, &g);
        m.press(Key::Down { fresh: true }, &g);
        let fx = m.press(Key::Confirm, &g);
        assert!(fx.contains(&Effect::Equip { cat: Cat::Armor, slot: 1, item: 21 }), "armor slot 1 (row 5) takes item 21: {fx:?}");
        assert_eq!(m.page, LIST);
        // The removal row empties the slot.
        goto(&mut m, 4);
        m.press(Key::Confirm, &g);
        let fx = m.press(Key::Confirm, &g);
        assert!(fx.contains(&Effect::Equip { cat: Cat::Armor, slot: 0, item: 0 }));
        // The form's keyblade names the form.
        goto(&mut m, 3);
        m.press(Key::Confirm, &game(&[(10, 1)]));
        let fx = m.press(Key::Confirm, &game(&[(10, 1)]));
        assert!(fx.contains(&Effect::Equip { cat: Cat::Form, slot: 4, item: 10 }));
    }

    #[test]
    fn choosing_what_the_slot_holds_changes_nothing() {
        let mut m = machine();
        goto(&mut m, 4);
        let g = game(&[(20, 1)]);
        m.press(Key::Confirm, &g);
        m.press(Key::Down { fresh: true }, &g);
        let fx = m.press(Key::Confirm, &g);
        assert!(!fx.iter().any(|e| matches!(e, Effect::Equip { .. })), "armor slot 0 already holds 20: {fx:?}");
        assert_eq!(m.page, LIST);
    }

    #[test]
    fn a_swap_is_refused_when_the_old_item_does_not_fit_back_in_the_stock() {
        let mut g = game(&[(20, 1)]);
        g.fits = false;
        let mut m = machine();
        goto(&mut m, 4);
        m.press(Key::Confirm, &g);
        m.press(Key::Down { fresh: true }, &g);
        let fx = m.press(Key::Confirm, &g);
        assert_eq!(fx, vec![Effect::Sound(snd::REFUSED)]);
        assert_eq!(m.page, 7);
        // An empty slot has nothing to return: the pick goes through.
        goto(&mut m, 5);
        m.press(Key::Cancel, &g);
        goto(&mut m, 5);
        m.press(Key::Confirm, &g);
        m.press(Key::Down { fresh: true }, &g);
        let fx = m.press(Key::Confirm, &g);
        assert!(fx.iter().any(|e| matches!(e, Effect::Equip { slot: 1, item: 20, .. })), "{fx:?}");
        // The weapon page with candidates is refused the same way; with no candidates it is not.
        goto(&mut m, 0);
        m.press(Key::Confirm, &g);
        assert_eq!(m.press(Key::Confirm, &g), vec![Effect::Sound(snd::REFUSED)]);
        let mut empty = game(&[]);
        empty.fits = false;
        let mut m2 = machine();
        m2.press(Key::Confirm, &empty);
        assert!(!m2.press(Key::Confirm, &empty).contains(&Effect::Sound(snd::REFUSED)));
    }

    #[test]
    fn an_accessory_that_would_leave_too_little_ap_opens_the_notice_and_writes_nothing() {
        let mut g = game(&[(30, 1), (31, 1)]);
        g.short = true;
        let mut m = machine();
        goto(&mut m, 6);
        m.press(Key::Confirm, &g);
        assert_eq!(m.page, 8);
        m.press(Key::Down { fresh: true }, &g);
        m.press(Key::Down { fresh: true }, &g);
        let fx = m.press(Key::Confirm, &g);
        assert_eq!(m.page, NOTICE);
        assert!(!fx.iter().any(|e| matches!(e, Effect::Equip { .. })));
        m.press(Key::Confirm, &g);
        assert_eq!(m.page, 8, "the notice closes onto the accessory list");
    }

    #[test]
    fn the_quick_key_switches_auto_reload_on_item_rows_and_asks_before_a_waiting_slot_goes_off() {
        let mut g = game(&[]);
        let mut m = machine();
        // Row 7 carries item 1 with Auto-Reload on: off with no question.
        goto(&mut m, 7);
        let fx = m.press(Key::Toggle, &g);
        assert!(fx.contains(&Effect::Reload { item: 1, on: false }));
        // Row 9 (item 3, off): on.
        goto(&mut m, 9);
        assert!(m.press(Key::Toggle, &g).contains(&Effect::Reload { item: 3, on: true }));
        // Row 8 reserves item 2 in an empty slot: off asks first when the slot waits for a refill.
        goto(&mut m, 8);
        g.question = false;
        assert!(m.press(Key::Toggle, &g).iter().any(|e| matches!(e, Effect::Reload { .. })));
        let mut rows = build_rows(&inputs());
        rows[8].reload = true;
        m.reload(rows);
        g.question = true;
        let fx = m.press(Key::Toggle, &g);
        assert_eq!(m.page, QUESTION);
        assert!(fx.contains(&Effect::Page(QUESTION)));
        m.press(Key::Down { fresh: true }, &g);
        let fx = m.press(Key::Confirm, &g);
        assert!(fx.contains(&Effect::Reload { item: 2, on: false }));
        assert_eq!(m.page, LIST);
        // The other categories ignore the key; an empty item row refuses it.
        goto(&mut m, 0);
        assert!(m.press(Key::Toggle, &g).is_empty());
        goto(&mut m, 10);
        assert_eq!(m.press(Key::Toggle, &g), vec![Effect::Sound(snd::REFUSED)]);
    }

    #[test]
    fn the_list_wraps_on_a_new_press_only_and_one_character_has_no_switch() {
        let g = game(&[]);
        let mut m = machine();
        m.press(Key::Up { fresh: false }, &g);
        assert_eq!(m.cursor, 0);
        m.press(Key::Up { fresh: true }, &g);
        assert_eq!(m.cursor, 10, "a new press at the top wraps to the last row");
        assert_eq!(m.scroll, 2, "nine rows show: the last row scrolls into view");
        assert_eq!(m.press(Key::Prev, &g), vec![Effect::Sound(snd::REFUSED)]);
        assert_eq!(m.press(Key::Cancel, &g), vec![Effect::Sound(snd::CANCEL), Effect::Leave]);
        assert_eq!(m.press(Key::Close, &g), vec![Effect::Close]);
        // Closing from a pick page settles the battle items first.
        m.cursor = 0;
        m.press(Key::Confirm, &game(&[(10, 1)]));
        assert_eq!(m.press(Key::Close, &g), vec![Effect::Settle, Effect::Close]);
    }
}
