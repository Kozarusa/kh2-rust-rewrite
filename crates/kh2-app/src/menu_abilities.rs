//! The pause menu's Abilities pages, stage M5 (docs/menus.md 2.3 and section 10).
//!
//! Pages (ids of `FUN_002514d0`'s switch): 0xf the character list (`FUN_002790e0`, entry
//! `FUN_00278aa8`), 0x10 the character's abilities in four groups (`FUN_00279ac0`, entry
//! `FUN_002794c8`), 0x11 to 0x13 the Equip / Don't Equip prompt of a row of group 0, 1 or 2
//! (`FUN_0027a578`, entry `FUN_0027a068`). The rows are one table per character (`DAT_0036d110`,
//! built by `FUN_0027a7e8`): a header (+2 the AP maximum, +4 the AP in use, +6 the number of
//! characters, +7 the row count, +8..+0xb the rows per group) and then 8 bytes per row (+0 item id,
//! +2 equipped, +3 slot of the character's ability list, +4 group, +5 cost, +6 the command slot
//! of a Donald or Goofy action ability). The writer of the equipped flag is `FUN_0027d8b8`: bit 15 of
//! the slot's u16 in the character record (`record + 0x54 + 2 * slot`).
//!
//! The whole screen keeps the original's rules; what the app lacks is stated in docs/menus.md
//! section 10: the party is Sora alone, so the character list has one row and the drive form and
//! summon rows of `FUN_00278eb0` are not ported; Donald's and Goofy's action-ability command slots
//! (the four-row prompt) are not ported either.

use std::collections::BTreeMap;

use bevy::prelude::*;
use kh2_image::text::{Style, Token};
use kh2_tables::tables as t;

use crate::abilities::Abilities;
use crate::assets::Disc;
use crate::audio::PlaySe;
use crate::menu::new_tags::{self, NewTags};
use crate::menu::{help, Menu, Phase, Switch};
use crate::menu_gfx::{MenuScene, Node};
use crate::menu_pages::{Entry, Exe};
use crate::menu_text::{Label, MenuText};
use crate::progress::Progress;

/// The four groups, in the order of `FUN_0027a7e8`'s group byte and of the label table at 0x382070:
/// Action, Growth, Support, Weapons.
pub const GROUPS: usize = 4;
/// Group byte of the keyblade's own ability row (`FUN_0027adc0`).
pub const WEAPON_GROUP: u8 = 3;

/// What the screen reads of one ability item (`item` row type 0x13): `StatEntry` low byte is the kind
/// and the high byte the AP cost (`FUN_0027a7e8` reads item `+6` and `+7`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Info {
    pub id: u16,
    /// 0 support, 1 party only, 2 action, 3 growth.
    pub kind: u8,
    pub cost: u8,
    /// Message ids: item `+8` (name) and `+0xa` (help text, `FUN_0027ce58`).
    pub name: u16,
    pub description: u16,
    /// Item `+0x17`, the icon an unequipped row shows (`FUN_001a1ee0`).
    pub icon: u8,
    /// Position in the item table: the order of the rows inside a group (`FUN_0027b1b0`).
    pub order: usize,
}

/// The group of an ability kind (`FUN_0027a7e8`: kind 0 -> 2, 1 and 2 -> 0, 3 -> 1). Other kinds
/// leave the byte unset in the original; none exists on the disc, so they land in group 0 [inferred].
pub fn group_of_kind(kind: u8) -> u8 {
    match kind {
        0 => 2,
        3 => 1,
        _ => 0,
    }
}

/// One row of the table.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Row {
    pub id: u16,
    pub equipped: bool,
    /// Slot of the character's ability list; `None` for the keyblade's own ability (slot 0xff).
    pub slot: Option<usize>,
    pub group: u8,
    pub cost: u8,
}

/// The table `DAT_0036d110` of one character.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Table {
    pub rows: Vec<Row>,
    /// Header `+2`: the character's AP maximum (status `+0x18e`).
    pub ap_max: u16,
}

impl Table {
    /// `FUN_0027a7e8`, `FUN_0027adc0`, `FUN_0027b1b0`: a row for every slot of the character's list
    /// whose id has an item row, then the keyblade's own ability as a Weapons row (equipped, no
    /// cost, no slot), sorted by group and, inside a group other than Weapons, by the item table's
    /// order. `slots` are (slot, id, equipped) as `Progress::ability_slots` gives them.
    pub fn build(slots: &[(usize, u16, bool)], weapon_ability: Option<u16>, info: &BTreeMap<u16, Info>, ap_max: u16) -> Table {
        let mut rows: Vec<Row> = slots
            .iter()
            .filter_map(|&(slot, id, equipped)| {
                let i = info.get(&id)?;
                Some(Row { id, equipped, slot: Some(slot), group: group_of_kind(i.kind), cost: i.cost })
            })
            .collect();
        if let Some(id) = weapon_ability.filter(|&a| a != 0) {
            rows.push(Row { id, equipped: true, slot: None, group: WEAPON_GROUP, cost: 0 });
        }
        let order = |r: &Row| info.get(&r.id).map_or(usize::MAX, |i| i.order);
        // The original orders by group first, then by the table position inside a group; the sort
        // is by key here, which is the same order wherever the keys differ.
        rows.sort_by_key(|r| (r.group, if r.group == WEAPON_GROUP { 0 } else { order(r) }));
        Table { rows, ap_max }
    }

    /// Header `+4` (`FUN_0027b150`): the cost of every equipped row added up.
    pub fn ap_used(&self) -> u16 {
        self.rows.iter().filter(|r| r.equipped).map(|r| r.cost as u16).sum()
    }

    /// Header `+8..+0xb`: the rows of each group.
    pub fn counts(&self) -> [u8; GROUPS] {
        let mut c = [0u8; GROUPS];
        for r in &self.rows {
            if let Some(n) = c.get_mut(r.group as usize) {
                *n += 1;
            }
        }
        c
    }

    /// `FUN_0027c6f8`: the group the row at `index` belongs to (`None` past the last row).
    pub fn group_at(&self, index: usize) -> Option<usize> {
        let mut total = 0usize;
        for (g, n) in self.counts().iter().enumerate() {
            total += *n as usize;
            if index < total {
                return Some(g);
            }
        }
        None
    }

    /// `FUN_0027d6c0`: an equipped row is always usable (it can be taken off); an unequipped one
    /// needs its cost not to exceed the AP left, `max(0, max - used)`.
    pub fn affordable(&self, index: usize) -> bool {
        let Some(r) = self.rows.get(index) else { return false };
        if r.equipped {
            return true;
        }
        let left = (self.ap_max as i32 - self.ap_used() as i32).max(0);
        r.cost as i32 <= left
    }

    /// `FUN_0027dc30` for a confirm on a row of a character's own list (not a form's): a row without
    /// cost (the keyblade's ability) is refused, any other by `affordable`.
    pub fn can_toggle(&self, index: usize) -> bool {
        match self.rows.get(index) {
            Some(r) if r.cost != 0 => self.affordable(index),
            _ => false,
        }
    }

    /// `FUN_0027d8b8`, the table half: the row's equipped flag.
    pub fn set_equipped(&mut self, index: usize, on: bool) {
        if let Some(r) = self.rows.get_mut(index) {
            r.equipped = on;
        }
    }

    /// `FUN_0027c9e8` on page 0x10: the AP in use after the highlighted row changes, as the bar
    /// shows it: the row's cost is taken off when it is equipped and added when it is not, and the
    /// result is held to the maximum.
    pub fn preview(&self, index: usize) -> i32 {
        let used = self.ap_used() as i32;
        let delta = match self.rows.get(index) {
            Some(r) if r.equipped => -(r.cost as i32),
            Some(r) => r.cost as i32,
            None => 0,
        };
        let delta = if (self.ap_max as i32) < used + delta { self.ap_max as i32 - used } else { delta };
        used + delta
    }
}

/// The two fractions of the AP bar (`FUN_0027c9e8`): the one in use now and the one after the
/// change, each used / max held to 1 (`DAT_00382038`, read as 1.0 [inferred: the bar cannot pass full]).
pub fn bar_fractions(used: i32, after: i32, max: u16) -> (f32, f32) {
    if max == 0 {
        return (0.0, 0.0);
    }
    let f = |v: i32| (v.max(0) as f32 / max as f32).min(1.0);
    (f(used), f(after))
}

/// The page a row of group `g` opens: 0x11, 0x12 or 0x13 (`FUN_00279ac0`).
pub fn prompt_page(group: usize) -> u16 {
    0x11 + group.min(2) as u16
}

/// The icon of an ability row (`FUN_00255520`): an equipped row takes the equipped icon of its item
/// type from the table at 0x3809d0 (triples of type, unequipped icon, equipped icon; 0xb when the
/// type is not listed), an unequipped one the item's own icon byte.
pub fn row_icon(equipped: bool, own: u8, item_type: u8, table: &[(i16, i16, i16)]) -> u8 {
    if !equipped {
        return own;
    }
    table.iter().find(|t| t.0 == item_type as i16).map_or(0xb, |t| t.2 as u8)
}

/// The tables the page reads from the executable (addresses in the module doc and docs/menus.md 10).
#[derive(Clone, Debug, Default)]
pub struct ExeTables {
    /// 0x382070: the group label messages (bit 15 set; the id is the low 15 bits).
    pub group_label: [u32; GROUPS],
    /// 0x382078: per group the tab's groups (three u16 each: the enter group, the leave group, the loop).
    pub tab: [[u16; 3]; GROUPS],
    /// 0x382060: per group the row object's group (`FUN_0027b9a8`).
    pub row: [u16; GROUPS],
    /// 0x382058: per group the equipped marker's group (`FUN_0027b668`).
    pub marker: [u16; GROUPS],
    /// 0x382090: per group the list plate's enter group (`FUN_0027a068`), 0x382098 the next one.
    pub plate: [u16; GROUPS],
    pub plate2: [u16; GROUPS],
    /// 0x3820a8: the two prompt messages, index 0 (Don't Equip) and 1 (Equip); 0x3820b0 their help lines.
    pub prompt: [u32; 2],
    pub help_prompt: [u32; 2],
    /// 0x38204c: the help line of the character list.
    pub help_party: u32,
    /// 0x3809d0: (item type, unequipped icon, equipped icon) until a negative type.
    pub icons: Vec<(i16, i16, i16)>,
}

impl ExeTables {
    pub fn read(exe: &Exe) -> Option<ExeTables> {
        let w = |va: u32| exe.u16(va);
        let mut t = ExeTables::default();
        for g in 0..GROUPS {
            let g32 = g as u32;
            t.group_label[g] = u32::from(w(0x0038_2070 + 2 * g32)? & 0x7fff);
            for k in 0..3 {
                t.tab[g][k] = w(0x0038_2078 + 6 * g32 + 2 * k as u32)?;
            }
            t.row[g] = w(0x0038_2060 + 2 * g32)?;
            t.marker[g] = w(0x0038_2058 + 2 * g32)?;
            t.plate[g] = w(0x0038_2090 + 2 * g32)?;
            t.plate2[g] = w(0x0038_2098 + 2 * g32)?;
        }
        t.prompt = [u32::from(w(0x0038_20a8)? & 0x7fff), u32::from(w(0x0038_20aa)? & 0x7fff)];
        t.help_prompt = [u32::from(w(0x0038_20b0)? & 0x7fff), u32::from(w(0x0038_20b2)? & 0x7fff)];
        t.help_party = u32::from(w(0x0038_204c)? & 0x7fff);
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
}


// ---------------------------------------------------------------------------------------------
// The page machine: which page is up, the cursors, and what a key does there. No Bevy here, so the
// tests drive it.

/// Sound ids of the menu's system bank (docs/menus.md 9: `PlaySe::number(2000 + id)`): cursor 1,
/// confirm 2, cancel 4, refused 5.
pub mod snd {
    pub const CURSOR: u32 = 2001;
    pub const CONFIRM: u32 = 2002;
    pub const CANCEL: u32 = 2004;
    pub const REFUSED: u32 = 2005;
}

/// Rows the list shows at once (`FUN_002794c8` builds at most nine).
pub const VISIBLE: usize = 9;

/// The list results of docs/menus.md 1.7 that the Abilities pages use.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Key {
    /// Up / Down: `fresh` is a new press (wraps), not a held repeat (stops at the ends).
    Up { fresh: bool },
    Down { fresh: bool },
    /// -> an index >= 0.
    Confirm,
    /// -2.
    Cancel,
    /// -4: leave the whole menu.
    Close,
    /// -10 / -11: the previous or next character.
    Prev,
    Next,
    /// -6: the quick toggle of the highlighted row (`FUN_0027d708`).
    Toggle,
    /// -5: to the Items pages (page 0xf goes to page 1, page 0x10 to page 2).
    Switch,
    /// -7: the Help overlay (page 0x10 only, topic 2).
    Help,
}

/// What the screen has to do after a key.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Effect {
    Sound(u32),
    /// The page changed (the screen redraws).
    Page(u16),
    /// The equipped flag of `row` is written (`FUN_0027d8b8`): the caller writes the slot.
    Toggle { row: usize, on: bool },
    /// The character changed: the caller builds that character's table.
    Member(usize),
    /// Back to the top list (page 0).
    Leave,
    /// -5: hand over to the Items pages (`second`: from page 0x10 to page 2).
    Switch { second: bool },
    /// -7: open the Help overlay with a topic, to come back to the page.
    Help { topic: u8, ret: u16 },
    /// Close the whole menu.
    Close,
}

/// Pages 0xf, 0x10 and 0x11..=0x13.
#[derive(Clone, Debug)]
pub struct Machine {
    pub page: u16,
    pub table: Table,
    /// The characters the list shows (the app's party is Sora alone) and the highlighted one.
    pub members: usize,
    pub member: usize,
    /// Page 0x10's row and the first row shown.
    pub cursor: usize,
    pub scroll: usize,
    /// The prompt's row: 0 Don't Equip, 1 Equip (`FUN_0027c780` starts on the current state).
    pub prompt: usize,
}

impl Machine {
    pub fn new(table: Table, members: usize) -> Machine {
        Machine { page: 0xf, table, members: members.max(1), member: 0, cursor: 0, scroll: 0, prompt: 0 }
    }

    /// `FUN_0027da98`: the scroll keeps the highlighted row on the screen.
    fn keep_visible(&mut self) {
        if self.cursor < self.scroll {
            self.scroll = self.cursor;
        } else if self.cursor >= self.scroll + VISIBLE {
            self.scroll = self.cursor + 1 - VISIBLE;
        }
    }

    fn step(&mut self, dir: i8, fresh: bool, fx: &mut Vec<Effect>) {
        let len = match self.page {
            0xf => self.members,
            0x10 => self.table.rows.len(),
            _ => 2,
        };
        let cur = match self.page {
            0xf => &mut self.member,
            0x10 => &mut self.cursor,
            _ => &mut self.prompt,
        };
        let new = crate::menu::step_cursor(*cur, len, dir, fresh);
        if new != *cur {
            *cur = new;
            fx.push(Effect::Sound(snd::CURSOR));
            if self.page == 0x10 {
                self.keep_visible();
            }
            fx.push(Effect::Page(self.page));
        }
    }

    /// One key on the current page. The rules are `FUN_002790e0` (0xf), `FUN_00279ac0` (0x10) and
    /// `FUN_0027a578` (0x11..=0x13), each with the validity callback `FUN_0027dc30`.
    pub fn press(&mut self, key: Key) -> Vec<Effect> {
        let mut fx = Vec::new();
        match key {
            Key::Close => fx.push(Effect::Close),
            Key::Up { fresh } => self.step(-1, fresh, &mut fx),
            Key::Down { fresh } => self.step(1, fresh, &mut fx),
            Key::Cancel => match self.page {
                0xf => {
                    fx.push(Effect::Sound(snd::CANCEL));
                    fx.push(Effect::Leave);
                }
                0x10 => {
                    self.page = 0xf;
                    fx.push(Effect::Sound(snd::CANCEL));
                    fx.push(Effect::Page(0xf));
                }
                _ => {
                    self.page = 0x10;
                    fx.push(Effect::Sound(snd::CANCEL));
                    fx.push(Effect::Page(0x10));
                }
            },
            Key::Confirm => match self.page {
                0xf => {
                    self.page = 0x10;
                    self.cursor = 0;
                    self.scroll = 0;
                    fx.push(Effect::Sound(snd::CONFIRM));
                    fx.push(Effect::Page(0x10));
                }
                0x10 => match self.table.group_at(self.cursor) {
                    // The callback `FUN_0027dc30` refuses a row without cost and an unequipped row
                    // that does not fit: the refused sound, nothing else happens.
                    Some(g) if self.table.can_toggle(self.cursor) => {
                        self.page = prompt_page(g);
                        self.prompt = self.table.rows[self.cursor].equipped as usize;
                        fx.push(Effect::Sound(snd::CONFIRM));
                        fx.push(Effect::Page(self.page));
                    }
                    _ => fx.push(Effect::Sound(snd::REFUSED)),
                },
                _ => {
                    // `FUN_0027d960`: the equipped flag becomes "the chosen row is Equip".
                    let (row, on) = (self.cursor, self.prompt == 1);
                    self.table.set_equipped(row, on);
                    self.page = 0x10;
                    fx.push(Effect::Sound(snd::CONFIRM));
                    fx.push(Effect::Toggle { row, on });
                    fx.push(Effect::Page(0x10));
                }
            },
            Key::Prev | Key::Next if self.page == 0x10 => {
                // `FUN_0027dc30`: refused unless there is more than one character.
                if self.members > 1 {
                    let n = self.members;
                    self.member = if key == Key::Next { (self.member + 1) % n } else { (self.member + n - 1) % n };
                    self.cursor = 0;
                    self.scroll = 0;
                    fx.push(Effect::Sound(snd::CURSOR));
                    fx.push(Effect::Member(self.member));
                } else {
                    fx.push(Effect::Sound(snd::REFUSED));
                }
            }
            // `FUN_002790e0` / `FUN_00279ac0`: -5 sets the target page 1 (from 0xf) or 2 (from 0x10), sound kind 2.
            Key::Switch if self.page == 0xf || self.page == 0x10 => {
                fx.push(Effect::Sound(snd::CONFIRM));
                fx.push(Effect::Switch { second: self.page == 0x10 });
            }
            // `FUN_00279ac0`: -7 opens page 0x25 with topic 2 and the return page 0x10.
            Key::Help if self.page == 0x10 => {
                fx.push(Effect::Sound(snd::CONFIRM));
                fx.push(Effect::Help { topic: 2, ret: 0x10 });
            }
            Key::Toggle if self.page == 0x10 => {
                // The shortcut is offered (mask 0x200) only for a row with a cost; `FUN_0027d708` then
                // flips the flag when the callback allows it.
                if self.table.rows.get(self.cursor).is_some_and(|r| r.cost != 0) {
                    if self.table.can_toggle(self.cursor) {
                        let on = !self.table.rows[self.cursor].equipped;
                        self.table.set_equipped(self.cursor, on);
                        fx.push(Effect::Sound(snd::CONFIRM));
                        fx.push(Effect::Toggle { row: self.cursor, on });
                        fx.push(Effect::Page(0x10));
                    } else {
                        fx.push(Effect::Sound(snd::REFUSED));
                    }
                }
            }
            _ => {}
        }
        fx
    }
}

// ---------------------------------------------------------------------------------------------
// The screen.

/// Group ids of `camp.2ld` sequence 1 that the pages name with literals (the others are the tables
/// of `ExeTables`): the list window (enter, leave, hold; `FUN_002794c8`), the cost plate of a row
/// (`FUN_0027b9a8`), the AP panel (`FUN_0027c898`) and its two gauges (`FUN_0027c9e8`, the bar
/// groups 0x382038..0x382040 hold the colour pair), the header tab (`FUN_002506b8` through
/// `FUN_0029cf78(.., 0x380828, ..)`), the hand and the glint (as page 0).
mod group {
    pub const LIST_WINDOW: [usize; 3] = [0xe6, 0xe7, 0xe8];
    pub const COST_PLATE: usize = 0xfb;
    pub const AP_PANEL: [usize; 3] = [0x110, 0x111, 0x112];
    /// The gauge that previews a gain, the one that shows a loss, the solid one for what is in use.
    pub const GAUGE_ADD: usize = 0x10d;
    pub const GAUGE_NOW: usize = 0x10e;
    pub const GAUGE_LOSE: usize = 0x10f;
    pub const TAB: [usize; 3] = [0x4b, 0x4d, 0x4c];
    pub const HAND: usize = 0x19;
    pub const GLINT: [usize; 3] = [0x1b, 0x1d, 0x1c];
    /// The red outline of a highlighted row (the group beside the prompt's row plates).
    pub const OUTLINE: usize = 0x9c;
}

/// Measured or derived layout, in menu units (before the 16 line offset). The list rows hang on the
/// window's anchor `(111, 84)`; their pitch is 26 (the unit of `FUN_00237678`, as page 0's).
mod at {
    pub const ROW_PITCH: f32 = 26.0;
    pub const HAND: [f32; 2] = [-13.0, 0.0];
    pub const GLINT: [f32; 2] = [94.0, -3.0];
    /// Text offsets from a group's anchor point.
    pub const ROW_LABEL: [f32; 2] = [0.0, 1.0];
    pub const COST_LABEL: [f32; 2] = [3.0, 1.0];
    pub const TAG_LABEL: [f32; 2] = [3.0, 1.0];
    pub const TAB_LABEL: [f32; 2] = [0.0, 1.0];
    pub const TAB_PITCH: f32 = 120.0;
    /// The AP number: its right edge and top.
    pub const AP_TEXT: [f32; 2] = [497.0, 283.0];
    pub const HELP: [f32; 2] = [137.0, 337.0];
    pub const COLOR: [u8; 4] = [0x80; 4];
    pub const GREY: [u8; 4] = [0x48, 0x48, 0x50, 0x80];
    pub const HELP_COLOR: [u8; 4] = [0x30, 0x7c, 0x80, 0x80];
}

/// The message that says a character has no abilities (`FUN_0027b9a8`: 0xb372, bit 15 ignored).
const NO_ABILITIES: u32 = 0x3372;

/// Indices of the nodes the pages draw, created on first use. They come after page 0's, so they
/// draw on top; a parent comes before its children.
struct Ui {
    tabs: [usize; 3],
    window: usize,
    /// The highlighted character's row on page 0xf.
    char_row: usize,
    /// Page 0xf's list of equipped abilities, page 0x10's rows (nine each).
    rows: Vec<usize>,
    costs: Vec<usize>,
    tags: Vec<usize>,
    /// The NEW tag of each row of page 0x10 and of the character row of page 0xf (stage M7).
    new_tags: Vec<usize>,
    char_new: usize,
    outline: usize,
    hand: usize,
    glint: usize,
    /// The prompt: the strip with the ability's name and the two rows.
    prompt_head: usize,
    prompt_rows: [usize; 2],
    ap_panel: usize,
    gauge_a: usize,
    gauge_b: usize,
    ap_text: usize,
    help: usize,
    all: Vec<usize>,
}

/// What the plugin keeps between frames.
#[derive(Resource, Default)]
pub struct AbilitiesMenu {
    machine: Option<Machine>,
    ready: bool,
    info: BTreeMap<u16, Info>,
    item_types: BTreeMap<u16, u8>,
    exe: Option<ExeTables>,
    /// Entry 1 of the top list (Abilities): its name message and group sets (`menu_pages`).
    entry: Option<Entry>,
    ui: Option<Ui>,
    /// Page 0's nodes that were visible when this screen took over (it hides them).
    hidden: Vec<usize>,
    /// Real seconds the up or down key has been held, and which way.
    held: (i8, f32),
}

pub struct MenuAbilitiesPlugin;

impl Plugin for MenuAbilitiesPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<AbilitiesMenu>()
            .add_systems(Startup, load)
            // The menu's own input system clears every key after it has looked at them; this one runs
            // first so it sees them, and when a page of this module is up it clears them itself.
            .add_systems(
                PreUpdate,
                input.after(bevy::input::InputSystems).after(crate::harness::Keys).before(crate::menu::input),
            );
    }
}

/// Reads the item rows and the executable tables once.
fn load(disc: Res<Disc>, mut st: ResMut<AbilitiesMenu>) {
    let items = {
        let mut a = disc.0.lock().unwrap();
        kh2_tables::load::<t::Item>(&mut a, &t::ITEM).ok().map(|d| d.rows)
    };
    let Some(items) = items else {
        warn!("abilities: the item table is not readable; the Abilities pages are off");
        return;
    };
    for (order, i) in items.iter().enumerate() {
        st.item_types.entry(i.id).or_insert(i.r#type);
        if i.r#type == crate::progress::item_type::ABILITY {
            st.info.entry(i.id).or_insert(Info {
                id: i.id,
                kind: (i.stat_entry & 0xff) as u8,
                cost: (i.stat_entry >> 8) as u8,
                name: i.name,
                description: i.description,
                icon: i.icon2,
                order,
            });
        }
    }
    let Some(exe) = disc.read_root("SLUS_210.05").and_then(Exe::load) else {
        warn!("abilities: the executable is not readable; the Abilities pages are off");
        return;
    };
    st.exe = ExeTables::read(&exe);
    st.entry = Entry::read(&exe, 1);
    st.ready = st.exe.is_some() && st.entry.is_some();
    if !st.ready {
        warn!("abilities: the executable's tables are not readable; the Abilities pages are off");
    }
}

/// Sora's AP maximum (`FUN_0019f920`: the `lvup` AP of the level, the record's AP bonus byte, and the
/// equipment's AP; the app's `Abilities::ap_max` has the first and third with its own level).
pub fn ap_max(ab: &Abilities, p: &Progress) -> u16 {
    let r = ab.rules();
    let m = p.sora();
    let base = r.ap_by_level.get((m.level as usize).saturating_sub(1)).copied().unwrap_or(0);
    let gear: u16 = p.equipment().iter().filter_map(|i| r.items.get(i).and_then(|e| r.equip.get(e))).map(|e| e.ap as u16).sum();
    base + m.ap as u16 + gear
}

/// The table of Sora's abilities from the game state.
fn table_of(st: &AbilitiesMenu, ab: &Abilities, p: &Progress) -> Table {
    let m = p.sora();
    let r = ab.rules();
    // `FUN_0027adc0`: the keyblade's own ability (its `item_stat` row's Ability).
    let weapon = m.weapons.first().and_then(|w| r.items.get(w)).and_then(|e| r.equip.get(e)).map(|e| e.ability);
    Table::build(&p.ability_slots(crate::progress::SORA), weapon, &st.info, ap_max(ab, p))
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

#[allow(clippy::too_many_arguments)]
fn input(
    mut st: ResMut<AbilitiesMenu>,
    mut menu: ResMut<Menu>,
    mut scene: ResMut<MenuScene>,
    mut keys: ResMut<ButtonInput<KeyCode>>,
    mut se: MessageWriter<PlaySe>,
    mut progress: Option<ResMut<Progress>>,
    mut abilities: Option<ResMut<Abilities>>,
    real: Res<Time<Real>>,
    mut ntags: ResMut<NewTags>,
) {
    if !st.ready {
        return;
    }
    // The menu left for another reason (closed): forget the pages.
    if st.machine.is_some() && matches!(menu.phase, Phase::Closing | Phase::Closed) {
        st.machine = None;
        st.hidden.clear();
        st.held = (0, 0.0);
        return;
    }
    // The Help overlay has the keys.
    if menu.page == help::PAGE {
        return;
    }
    if st.machine.is_none() {
        // Page 0's confirm on the Abilities row opens page 0xf (`FUN_00251848`'s jump table); the Items pages' -5 asks
        // for page 0xf too (or, from page 2, for page 0x10).
        let on_row = menu.page == 0 && menu.current_entry() == Some(1);
        let asked = match menu.switch {
            Some(Switch::ToAbilities { second }) => Some(second),
            _ => None,
        };
        if asked.is_none() && !(on_row && keys.just_pressed(KeyCode::KeyJ)) {
            // A Help overlay we came back from asks for its page again (`menu_help.rs`).
            return;
        }
        menu.switch = None;
        let (Some(ab), Some(p)) = (abilities.as_deref(), progress.as_deref()) else {
            crate::menu::menu_sound::play(&mut se, snd::REFUSED);
            keys.reset_all();
            return;
        };
        let table = table_of(&st, ab, p);
        if table.rows.is_empty() {
            // `FUN_0027b9a8`: "There are no Abilities." (the row is not even offered on page 0).
            crate::menu::menu_sound::play(&mut se, snd::REFUSED);
            keys.reset_all();
            return;
        }
        let mut machine = Machine::new(table, 1);
        if asked == Some(true) {
            machine.page = 0x10;
        }
        st.machine = Some(machine);
        st.held = (0, 0.0);
        enter(&mut st, &mut menu, &mut scene);
        if asked == Some(true) {
            menu.page = 0x10;
            info!("menu: page 0x10");
        }
        if asked.is_none() {
            crate::menu::menu_sound::play(&mut se, snd::CONFIRM);
        }
        keys.reset_all();
        redraw(&mut st, &mut scene, &mut ntags);
        return;
    }
    let mut held = st.held;
    let pressed = keys_of(&keys, &mut held, real.delta_secs());
    st.held = held;
    let mut close = false;
    for key in pressed {
        let fx = st.machine.as_mut().map(|m| m.press(key)).unwrap_or_default();
        for e in fx {
            match e {
                Effect::Sound(n) => {
                    crate::menu::menu_sound::play(&mut se, n);
                }
                Effect::Page(page) => {
                    menu.page = page;
                    info!("menu: page {page:#x}");
                }
                Effect::Toggle { row, on } => {
                    let name = ability_name(&st, &scene, row);
                    apply_toggle(&mut st, row, on, &name, progress.as_deref_mut(), abilities.as_deref_mut());
                }
                Effect::Member(_) => {}
                Effect::Leave => leave(&mut st, &mut menu, &mut scene),
                Effect::Switch { second } => {
                    // The list mask has 0x100 only while the Items row is on page 0.
                    if menu.rows_has(0) {
                        hand_over(&mut st, &mut menu, &mut scene);
                        menu.switch = Some(Switch::ToItems { second });
                        info!("menu: switch to Items{}", if second { " (page 2)" } else { "" });
                    }
                }
                Effect::Help { topic, ret } => {
                    menu.help = Some(help::Request { topic, ret });
                }
                Effect::Close => {
                    // Back to page 0, then the menu's own input closes it from there (the key is
                    // left unconsumed).
                    leave(&mut st, &mut menu, &mut scene);
                    close = true;
                }
            }
            if st.machine.is_none() {
                break;
            }
        }
        if st.machine.is_none() {
            break;
        }
    }
    if close {
        return;
    }
    if st.machine.is_some() {
        redraw(&mut st, &mut scene, &mut ntags);
        keys.reset_all();
    } else {
        // Leaving for page 0 consumed this frame's key.
        keys.reset_all();
    }
}

/// The readable name of the ability of a row (for the log).
fn ability_name(st: &AbilitiesMenu, scene: &MenuScene, row: usize) -> String {
    let id = st.machine.as_ref().and_then(|m| m.table.rows.get(row)).map(|r| r.id);
    id.and_then(|id| scene.text.as_ref()?.messages.as_ref()?.text(u32::from(st.info.get(&id)?.name))).unwrap_or_default()
}

/// `FUN_0027d8b8` on the character record and the app's `Abilities`: bit 15 of the row's slot.
fn apply_toggle(st: &mut AbilitiesMenu, row: usize, on: bool, name: &str, progress: Option<&mut Progress>, abilities: Option<&mut Abilities>) {
    let Some(r) = st.machine.as_ref().and_then(|m| m.table.rows.get(row).copied()) else { return };
    let Some(slot) = r.slot else { return };
    let (Some(p), Some(ab)) = (progress, abilities) else { return };
    if !p.set_ability_equipped(crate::progress::SORA, slot, on) {
        return;
    }
    // The app's `Abilities` list is the same list without the empty slots: the toggle goes to the same
    // entry, and the block (the controller's ability bits, the combo lengths) is rebuilt.
    let k = p.ability_slots(crate::progress::SORA).iter().position(|s| s.0 == slot);
    match k.and_then(|k| ab.learned.get_mut(k)).filter(|s| s.id == r.id) {
        Some(s) => s.equipped = on,
        None => {
            if let Some(s) = ab.learned.iter_mut().find(|s| s.id == r.id && s.equipped != on) {
                s.equipped = on;
            }
        }
    }
    ab.recompute();
    let (used, max) = st.machine.as_ref().map_or((0, 0), |m| (m.table.ap_used(), m.table.ap_max));
    // The word "equipped" is in both lines on purpose: the harness greps it.
    if on {
        info!("abilities: {name} (id {}, slot {slot}) equipped; AP {used}/{max}", r.id);
    } else {
        info!("abilities: {name} (id {}, slot {slot}) taken off, no longer equipped; AP {used}/{max}", r.id);
    }
}

// --- drawing -------------------------------------------------------------------------------

fn node(scene: &mut MenuScene, seq: Option<usize>) -> usize {
    let mut n = Node::new(seq);
    n.visible = false;
    scene.add(n)
}

fn ensure_ui(st: &mut AbilitiesMenu, scene: &mut MenuScene) {
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
    let char_row = mk(scene, S);
    let rows: Vec<usize> = (0..VISIBLE).map(|_| mk(scene, S)).collect();
    let costs: Vec<usize> = (0..VISIBLE).map(|_| mk(scene, S)).collect();
    let tags: Vec<usize> = (0..VISIBLE).map(|_| mk(scene, S)).collect();
    let new_tags: Vec<usize> = (0..VISIBLE).map(|_| mk(scene, S)).collect();
    let char_new = mk(scene, S);
    let outline = mk(scene, S);
    let hand = mk(scene, S);
    let glint = mk(scene, S);
    let prompt_head = mk(scene, S);
    let prompt_rows = [mk(scene, S), mk(scene, S)];
    let ap_panel = mk(scene, S);
    let gauge_a = mk(scene, S);
    let gauge_b = mk(scene, S);
    let ap_text = mk(scene, None);
    let help = mk(scene, None);
    // Parents (the children's positions follow the parent's anchor point).
    for (&r, &c) in rows.iter().zip(&costs) {
        scene.nodes[r].parent = Some(window);
        scene.nodes[c].parent = Some(r);
    }
    for (&r, &g) in rows.iter().zip(&tags) {
        scene.nodes[g].parent = Some(r);
    }
    for (&r, &g) in rows.iter().zip(&new_tags) {
        scene.nodes[g].parent = Some(r);
    }
    scene.nodes[char_new].parent = Some(char_row);
    scene.nodes[outline].parent = Some(window);
    scene.nodes[prompt_head].parent = Some(window);
    for &r in &prompt_rows {
        scene.nodes[r].parent = Some(window);
    }
    scene.nodes[gauge_a].parent = Some(ap_panel);
    scene.nodes[gauge_b].parent = Some(ap_panel);
    st.ui = Some(Ui { tabs, window, char_row, rows, costs, tags, new_tags, char_new, outline, hand, glint, prompt_head, prompt_rows, ap_panel, gauge_a, gauge_b, ap_text, help, all });
}

/// Gives the screen to the Items pages: this module's nodes go and page 0's stay hidden (handed on through
/// `Menu::carry`).
fn hand_over(st: &mut AbilitiesMenu, menu: &mut Menu, scene: &mut MenuScene) {
    if let Some(ui) = &st.ui {
        for &i in &ui.all {
            scene.nodes[i].visible = false;
        }
    }
    menu.carry = std::mem::take(&mut st.hidden);
    st.machine = None;
    menu.page = 0;
}

/// Takes over from page 0: hides its rows, hand, help line and cards, and brings in the header tabs
/// and the AP panel.
fn enter(st: &mut AbilitiesMenu, menu: &mut Menu, scene: &mut MenuScene) {
    ensure_ui(st, scene);
    st.hidden = if menu.carry.is_empty() { menu.page0_nodes().into_iter().filter(|&i| scene.nodes[i].visible).collect() } else { std::mem::take(&mut menu.carry) };
    for &i in &st.hidden {
        scene.nodes[i].visible = false;
    }
    menu.page = 0xf;
    info!("menu: page 0xf");
}

/// Back to page 0: this module's nodes go, page 0's come back.
fn leave(st: &mut AbilitiesMenu, menu: &mut Menu, scene: &mut MenuScene) {
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

fn tokens_of(text: &MenuText, id: u32) -> Vec<Token> {
    text.messages.as_ref().and_then(|m| m.tokens(id)).unwrap_or_default()
}

fn label(text: &MenuText, tokens: Vec<Token>, font: usize, size: f32, color: [u8; 4], align: u8, offset: [f32; 2]) -> Label {
    Label { tokens, style: Style { font, size: Some(size), color, align, world: text.world, ..Style::default() }, offset }
}

/// One row's text: the icon, then the name (`FUN_00255520` through `FUN_001a1ee0`).
fn row_label(text: &MenuText, st: &AbilitiesMenu, r: &Row, color: [u8; 4]) -> Option<Label> {
    let i = st.info.get(&r.id)?;
    let ty = st.item_types.get(&r.id).copied().unwrap_or(crate::progress::item_type::ABILITY);
    let icons = st.exe.as_ref().map(|e| e.icons.as_slice()).unwrap_or(&[]);
    let mut tokens = vec![Token::Icon(row_icon(r.equipped, i.icon, ty, icons))];
    tokens.extend(tokens_of(text, u32::from(i.name)));
    Some(label(text, tokens, 1, 15.0, color, 0, [0.0, 0.0]))
}

/// Sets what every node shows from the machine's state.
fn redraw(st: &mut AbilitiesMenu, scene: &mut MenuScene, ntags: &mut NewTags) {
    let Some(m) = st.machine.clone() else { return };
    let (Some(ui_ref), Some(exe)) = (st.ui.as_ref(), st.exe.clone()) else { return };
    let (tabs, window, char_row, rows, costs, tags, outline, hand, glint, prompt_head, prompt_rows, ap_panel, gauge_a, gauge_b, ap_text, help) = (
        ui_ref.tabs,
        ui_ref.window,
        ui_ref.char_row,
        ui_ref.rows.clone(),
        ui_ref.costs.clone(),
        ui_ref.tags.clone(),
        ui_ref.outline,
        ui_ref.hand,
        ui_ref.glint,
        ui_ref.prompt_head,
        ui_ref.prompt_rows,
        ui_ref.ap_panel,
        ui_ref.gauge_a,
        ui_ref.gauge_b,
        ui_ref.ap_text,
        ui_ref.help,
    );
    let (new_tags, char_new) = (ui_ref.new_tags.clone(), ui_ref.char_new);
    let all = ui_ref.all.clone();
    let entry = st.entry.clone().unwrap_or_else(|| Entry { idle: [0; 4], selected: [0; 4], name: 0, help: 0, need: vec![], deny: vec![] });
    let Some(text) = scene.text.take() else { return };
    let page = m.page;
    // Start from nothing shown, then show what the page has. A node keeps its group while it stays
    // visible, so a row that did not change does not restart its animation.
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

    // The header tabs: Abilities, the character, and on the prompt the group's name.
    let tab_names = [entry.name];
    let tab_count = if page >= 0x11 { 3 } else if page == 0x10 { 2 } else { 1 };
    for (k, &t) in tabs.iter().enumerate().take(tab_count) {
        let prev = was_of(t);
        {
            let n = &mut scene.nodes[t];
            n.pos = [at::TAB_PITCH * k as f32, 0.0];
        }
        set(scene, t, group::TAB[0], Some(group::TAB[2]), prev);
        let tokens = match k {
            0 => tokens_of(&text, tab_names[0]),
            1 => text.literal("Sora", Style::default(), [0.0, 0.0]).tokens,
            _ => m.table.group_at(m.cursor).map(|g| tokens_of(&text, exe.group_label[g])).unwrap_or_default(),
        };
        scene.nodes[t].label_anchor = Some(at::TAB_LABEL);
        scene.nodes[t].label = Some(label(&text, tokens, 1, 15.0, at::COLOR, 0, [0.0, 0.0]));
    }

    let ap_now = m.table.ap_used() as i32;
    let ap_after = if page == 0x10 { m.table.preview(m.cursor) } else { ap_now };

    match page {
        0xf => {
            // The character row, as page 0's selected row, and the character's equipped abilities in the
            // list window (`FUN_0027b668`).
            let prev = was_of(char_row);
            scene.nodes[char_row].off = [0.0, 0.0];
            set(scene, char_row, usize::from(entry.selected[0]), Some(usize::from(entry.selected[2])), prev);
            scene.nodes[char_row].label_anchor = Some([-7.0, 1.5]);
            scene.nodes[char_row].label = Some(label(&text, text.literal("Sora", Style::default(), [0.0, 0.0]).tokens, 1, 15.0, [0xe5, 0xe5, 0xe5, 0x80], 0, [0.0, 0.0]));
            // `FUN_00278aa8`: the character's row has the tag while any of his abilities is fresh.
            if let Some(g) = ntags.row_group(new_tags::Set::Abilities) {
                scene.nodes[char_new].off = scene.extent(1, usize::from(entry.selected[2]));
                set(scene, char_new, g, None, was_of(char_new));
            }
            set(scene, window, group::LIST_WINDOW[0], None, was_of(window));
            for (k, r) in m.table.rows.iter().filter(|r| r.equipped).take(VISIBLE).enumerate() {
                let id = rows[k];
                // The marker plates hang from their bottom edge (their box is y -20..0).
                scene.nodes[id].off = [0.0, 20.0 + at::ROW_PITCH * k as f32];
                set(scene, id, exe.marker[r.group as usize % 4] as usize, None, was_of(id));
                scene.nodes[id].label_anchor = Some([4.0, 1.0]);
                scene.nodes[id].label = row_label(&text, st, r, at::COLOR);
            }
            let p = &mut scene.nodes[hand];
            p.parent = Some(char_row);
            p.off = at::HAND;
            set(scene, hand, group::HAND, None, was_of(hand));
            let g = &mut scene.nodes[glint];
            g.parent = Some(char_row);
            g.off = at::GLINT;
            set(scene, glint, group::GLINT[0], Some(group::GLINT[2]), was_of(glint));
            set_help(scene, &text, help, Some(exe.help_party));
        }
        0x10 => {
            set(scene, window, group::LIST_WINDOW[0], None, was_of(window));
            if m.table.rows.is_empty() {
                set_help(scene, &text, help, Some(NO_ABILITIES));
            }
            let mut prev_group: Option<u8> = (m.scroll > 0).then(|| m.table.rows[m.scroll - 1].group);
            for k in 0..VISIBLE {
                let idx = m.scroll + k;
                let Some(r) = m.table.rows.get(idx) else { break };
                let g = r.group as usize % 4;
                scene.nodes[rows[k]].off = [0.0, at::ROW_PITCH * k as f32];
                set(scene, rows[k], exe.row[g] as usize, None, was_of(rows[k]));
                let color = if m.table.affordable(idx) { at::COLOR } else { at::GREY };
                scene.nodes[rows[k]].label_anchor = Some(at::ROW_LABEL);
                scene.nodes[rows[k]].label = row_label(&text, st, r, color);
                if r.cost != 0 {
                    set(scene, costs[k], group::COST_PLATE, None, was_of(costs[k]));
                    scene.nodes[costs[k]].label_anchor = Some(at::COST_LABEL);
                    scene.nodes[costs[k]].label = Some(label(&text, text.literal(&r.cost.to_string(), Style::default(), [0.0; 2]).tokens, 1, 15.0, [0x80, 0x80, 0x10, 0x80], 1, [0.0, 0.0]));
                }
                if prev_group != Some(r.group) {
                    let tag = [0x114usize, 0x116, 0x118, 0x11b][g];
                    set(scene, tags[k], tag, None, was_of(tags[k]));
                    scene.nodes[tags[k]].label_anchor = Some(at::TAG_LABEL);
                    scene.nodes[tags[k]].label = Some(label(&text, tokens_of(&text, exe.group_label[g]), 1, 13.0, at::COLOR, 0, [0.0, 0.0]));
                }
                // `FUN_0027b9a8`: a fresh ability's row has the tag (the keyblade's own row, cost 0 and slot 0xff, is
                // not in the list); `FUN_0027be78`: the cursor on it sets its seen bit.
                if idx == m.cursor && r.slot.is_some() {
                    ntags.view_ability(r.id);
                }
                if r.slot.is_some()
                    && let Some(g) = ntags.ability_group(r.id)
                {
                    scene.nodes[new_tags[k]].off = scene.extent(1, exe.row[r.group as usize % 4] as usize);
                    set(scene, new_tags[k], g, None, false);
                }
                prev_group = Some(r.group);
            }
            let k = m.cursor - m.scroll;
            if let Some(r) = m.table.rows.get(m.cursor) {
                scene.nodes[outline].off = [9.0, at::ROW_PITCH * k as f32];
                set(scene, outline, group::OUTLINE, None, was_of(outline));
                let p = &mut scene.nodes[hand];
                p.parent = Some(rows[k]);
                p.off = at::HAND;
                set(scene, hand, group::HAND, None, false);
                let desc = st.info.get(&r.id).map(|i| u32::from(i.description));
                set_help(scene, &text, help, desc);
            }
        }
        _ => {
            let g = m.table.group_at(m.cursor).unwrap_or(0) % 4;
            set(scene, window, exe.tab[g][0] as usize, None, was_of(window));
            set(scene, prompt_head, exe.plate[g] as usize, None, was_of(prompt_head));
            scene.nodes[prompt_head].label_anchor = Some([6.0, 0.0]);
            let row = m.table.rows.get(m.cursor).copied();
            scene.nodes[prompt_head].label = row.and_then(|r| row_label(&text, st, &r, at::COLOR));
            for (k, &pr) in prompt_rows.iter().enumerate() {
                scene.nodes[pr].off = [0.0, at::ROW_PITCH * k as f32];
                set(scene, pr, exe.plate2[g] as usize, None, was_of(pr));
                scene.nodes[pr].label_anchor = Some([14.0, 0.0]);
                let color = if k == m.prompt { [0xe5, 0xe5, 0xe5, 0x80] } else { at::GREY };
                scene.nodes[pr].label = Some(label(&text, tokens_of(&text, exe.prompt[k]), 1, 15.0, color, 0, [0.0, 0.0]));
            }
            let p = &mut scene.nodes[hand];
            p.parent = Some(prompt_rows[m.prompt]);
            p.off = at::HAND;
            set(scene, hand, group::HAND, None, false);
            // The help of the chosen option (`FUN_0027ce58`: messages 0x362a and 0x3628).
            set_help(scene, &text, help, Some(exe.help_prompt[m.prompt]));
        }
    }

    // The AP panel and its two gauges (`FUN_0027c898`, `FUN_0027c9e8`).
    set(scene, ap_panel, group::AP_PANEL[0], None, was_of(ap_panel));
    let (now, after) = bar_fractions(ap_now, ap_after, m.table.ap_max);
    let (big, small) = (now.max(after), now.min(after));
    // The wider gauge is the preview (blue for a gain, yellow for a loss), the narrower the AP in use.
    let preview_group = if after < now { group::GAUGE_LOSE } else { group::GAUGE_ADD };
    set(scene, gauge_a, preview_group, None, false);
    scene.nodes[gauge_a].ext.scale[1] = big;
    set(scene, gauge_b, group::GAUGE_NOW, None, was_of(gauge_b));
    scene.nodes[gauge_b].ext.scale[1] = small;
    scene.nodes[ap_text].pos = at::AP_TEXT;
    scene.nodes[ap_text].visible = true;
    scene.nodes[ap_text].label = Some(label(&text, text.literal(&format!("{}/{}", ap_now, m.table.ap_max), Style::default(), [0.0; 2]).tokens, 1, 15.0, at::COLOR, 1, [0.0, 0.0]));
    scene.text = Some(text);
}

fn set_help(scene: &mut MenuScene, text: &MenuText, help: usize, id: Option<u32>) {
    let n = &mut scene.nodes[help];
    n.pos = [0.0, 0.0];
    n.label = id.and_then(|id| text.message(id, Style { font: 0, size: Some(15.0), color: at::HELP_COLOR, ..Style::default() }, at::HELP));
    n.visible = n.label.is_some();
}

#[cfg(test)]
mod tests {
    use super::*;

    fn info() -> BTreeMap<u16, Info> {
        // Item ids in table order: a growth ability, an action one, two support ones.
        let mk = |id: u16, kind: u8, cost: u8, order: usize| (id, Info { id, kind, cost, name: 0, description: 0, icon: 0, order });
        BTreeMap::from([mk(94, 3, 1, 4), mk(82, 2, 0, 1), mk(162, 0, 3, 7), mk(160, 0, 2, 5), mk(500, 2, 0, 9)])
    }

    #[test]
    fn rows_group_by_kind_and_sort_by_the_item_table() {
        // Slots 0..: Combo Plus (162), Guard (82, an action ability), High Jump (94), 160; slot 4 empty.
        let slots = [(0, 162, true), (1, 82, true), (2, 94, false), (3, 160, false)];
        let t = Table::build(&slots, Some(500), &info(), 5);
        let ids: Vec<u16> = t.rows.iter().map(|r| r.id).collect();
        // Action (82), Growth (94), Support by table order (160 then 162), the keyblade's ability last.
        assert_eq!(ids, vec![82, 94, 160, 162, 500]);
        assert_eq!(t.rows.iter().map(|r| r.group).collect::<Vec<_>>(), vec![0, 1, 2, 2, 3]);
        assert_eq!(t.counts(), [1, 1, 2, 1]);
        assert_eq!(t.group_at(0), Some(0));
        assert_eq!(t.group_at(1), Some(1));
        assert_eq!(t.group_at(3), Some(2));
        assert_eq!(t.group_at(4), Some(3));
        assert_eq!(t.group_at(5), None);
        // The keyblade's row has no slot and no cost; the others keep their slot.
        assert_eq!(t.rows[4].slot, None);
        assert_eq!(t.rows[3].slot, Some(0));
    }

    #[test]
    fn ap_in_use_is_the_sum_of_the_equipped_costs() {
        let slots = [(0, 162, true), (1, 160, false), (2, 94, true)];
        let mut t = Table::build(&slots, Some(500), &info(), 6);
        assert_eq!(t.ap_used(), 3 + 1, "162 costs 3, 94 costs 1, the keyblade's row costs nothing");
        t.set_equipped(t.rows.iter().position(|r| r.id == 160).unwrap(), true);
        assert_eq!(t.ap_used(), 6);
    }

    #[test]
    fn over_cost_equipping_is_refused_and_taking_off_never_is() {
        let slots = [(0, 162, true), (1, 160, false)];
        let mut t = Table::build(&slots, None, &info(), 4);
        let i160 = t.rows.iter().position(|r| r.id == 160).unwrap();
        let i162 = t.rows.iter().position(|r| r.id == 162).unwrap();
        // 3 of 4 AP are in use; 160 costs 2 and does not fit, 162 is equipped and can come off.
        assert!(!t.affordable(i160) && !t.can_toggle(i160));
        assert!(t.affordable(i162) && t.can_toggle(i162));
        t.set_equipped(i162, false);
        assert!(t.can_toggle(i160), "taking 162 off leaves 4 AP, enough for 160");
        // A row without cost (the keyblade's ability) never toggles.
        let w = Table::build(&[], Some(500), &info(), 4);
        assert!(!w.can_toggle(0));
        // The AP left is held at 0 when more is in use than the maximum (an over-full list).
        let full = Table::build(&[(0, 162, true), (1, 160, false)], None, &info(), 1);
        assert!(!full.affordable(full.rows.iter().position(|r| r.id == 160).unwrap()));
    }

    #[test]
    fn the_bar_previews_the_change_and_holds_to_the_maximum() {
        let slots = [(0, 162, true), (1, 160, false)];
        let t = Table::build(&slots, None, &info(), 4);
        let i160 = t.rows.iter().position(|r| r.id == 160).unwrap();
        let i162 = t.rows.iter().position(|r| r.id == 162).unwrap();
        // 160 would add 2: 3 + 2 = 5 is held to the maximum 4; 162 would give 3 back.
        assert_eq!(t.preview(i160), 4);
        assert_eq!(t.preview(i162), 0);
        let (now, after) = bar_fractions(t.ap_used() as i32, t.preview(i162), t.ap_max);
        assert!((now - 0.75).abs() < 1e-6 && after == 0.0);
        assert_eq!(bar_fractions(1, 1, 0), (0.0, 0.0));
    }

    #[test]
    fn the_prompt_page_follows_the_group_and_the_icon_the_equipped_flag() {
        assert_eq!((prompt_page(0), prompt_page(1), prompt_page(2)), (0x11, 0x12, 0x13));
        let table = [(19, 3, 7), (2, 4, 8)];
        assert_eq!(row_icon(false, 5, 19, &table), 5, "an unequipped row shows the item's own icon");
        assert_eq!(row_icon(true, 5, 19, &table), 7);
        assert_eq!(row_icon(true, 5, 77, &table), 0xb, "an unlisted type shows icon 0xb");
    }

    /// A machine over: 162 equipped (3 AP), 160 not (2 AP), High Jump 94 not (1 AP), maximum 4 AP.
    fn machine() -> Machine {
        let slots = [(0, 162, true), (1, 160, false), (2, 94, false)];
        Machine::new(Table::build(&slots, Some(500), &info(), 4), 1)
    }

    fn row_of(m: &Machine, id: u16) -> usize {
        m.table.rows.iter().position(|r| r.id == id).unwrap()
    }

    #[test]
    fn the_character_list_opens_the_abilities_and_cancel_goes_back() {
        let mut m = machine();
        assert_eq!(m.page, 0xf);
        assert_eq!(m.press(Key::Confirm), vec![Effect::Sound(snd::CONFIRM), Effect::Page(0x10)]);
        assert_eq!((m.page, m.cursor), (0x10, 0));
        assert_eq!(m.press(Key::Cancel), vec![Effect::Sound(snd::CANCEL), Effect::Page(0xf)]);
        assert_eq!(m.press(Key::Cancel), vec![Effect::Sound(snd::CANCEL), Effect::Leave]);
        assert_eq!(m.press(Key::Close), vec![Effect::Close]);
    }

    #[test]
    fn confirm_on_an_affordable_row_opens_the_prompt_of_its_group_and_equip_writes_it() {
        let mut m = machine();
        m.press(Key::Confirm);
        // Rows: High Jump (Growth, 1 AP), 160 and 162 (Support), the keyblade's ability (Weapons).
        let hj = row_of(&m, 94);
        m.cursor = hj;
        assert_eq!(m.press(Key::Confirm), vec![Effect::Sound(snd::CONFIRM), Effect::Page(0x12)], "the Growth group opens page 0x12");
        assert_eq!(m.prompt, 0, "the prompt starts on the row's current state: not equipped");
        m.press(Key::Down { fresh: true });
        assert_eq!(m.prompt, 1);
        let fx = m.press(Key::Confirm);
        assert_eq!(fx, vec![Effect::Sound(snd::CONFIRM), Effect::Toggle { row: hj, on: true }, Effect::Page(0x10)]);
        assert!(m.table.rows[hj].equipped);
        assert_eq!(m.table.ap_used(), 4);
        assert_eq!(m.page, 0x10);
    }

    #[test]
    fn equipping_over_the_cost_is_refused_on_the_list() {
        let mut m = machine();
        m.press(Key::Confirm);
        // 3 of 4 AP are in use: 160 (2 AP) does not fit, so the confirm is refused and nothing changes.
        m.cursor = row_of(&m, 160);
        assert_eq!(m.press(Key::Confirm), vec![Effect::Sound(snd::REFUSED)]);
        assert_eq!(m.page, 0x10);
        assert!(!m.table.rows[m.cursor].equipped);
        // The quick toggle is refused the same way and writes nothing.
        assert_eq!(m.press(Key::Toggle), vec![Effect::Sound(snd::REFUSED)]);
        // The keyblade's row has no cost: no prompt, and the quick toggle does not even answer.
        m.cursor = row_of(&m, 500);
        assert_eq!(m.press(Key::Confirm), vec![Effect::Sound(snd::REFUSED)]);
        assert!(m.press(Key::Toggle).is_empty());
    }

    #[test]
    fn taking_an_ability_off_frees_its_ap_and_dont_equip_clears_the_flag() {
        let mut m = machine();
        m.press(Key::Confirm);
        let s = row_of(&m, 162);
        m.cursor = s;
        // An equipped row opens the prompt on Equip; moving up chooses Don't Equip.
        m.press(Key::Confirm);
        assert_eq!((m.page, m.prompt), (0x13, 1), "Support is group 2: page 0x13");
        m.press(Key::Up { fresh: true });
        assert_eq!(m.prompt, 0);
        assert_eq!(m.press(Key::Confirm), vec![Effect::Sound(snd::CONFIRM), Effect::Toggle { row: s, on: false }, Effect::Page(0x10)]);
        assert_eq!(m.table.ap_used(), 0);
        // Now 160 fits.
        m.cursor = row_of(&m, 160);
        assert!(matches!(m.press(Key::Confirm)[..], [Effect::Sound(snd::CONFIRM), Effect::Page(0x13)]));
        // Cancel on the prompt leaves the row as it was.
        m.press(Key::Cancel);
        assert!(!m.table.rows[m.cursor].equipped);
    }

    #[test]
    fn the_cursor_wraps_on_a_new_press_only_and_the_list_scrolls() {
        let mut m = machine();
        m.press(Key::Confirm);
        let n = m.table.rows.len();
        assert_eq!(m.press(Key::Up { fresh: false }), vec![], "a held repeat stops at the top");
        assert_eq!(m.cursor, 0);
        m.press(Key::Up { fresh: true });
        assert_eq!(m.cursor, n - 1, "a new press wraps");
        // One member: the character switch is refused.
        assert_eq!(m.press(Key::Next), vec![Effect::Sound(snd::REFUSED)]);
        // A long list scrolls so that the highlighted row stays among the nine shown.
        let rows: Vec<Row> = (0..20).map(|i| Row { id: 82, equipped: false, slot: Some(i), group: 2, cost: 1 }).collect();
        let mut big = Machine::new(Table { rows, ap_max: 99 }, 1);
        big.press(Key::Confirm);
        for _ in 0..12 {
            big.press(Key::Down { fresh: true });
        }
        assert_eq!((big.cursor, big.scroll), (12, 4));
        for _ in 0..12 {
            big.press(Key::Up { fresh: true });
        }
        assert_eq!((big.cursor, big.scroll), (0, 0));
    }
}
