//! The pause menu's Status pages, stage M2 (docs/menus.md 2.6 and section 10).
//!
//! What the original does (`SLUS_210.05`): Status is camp page 0x17 (a list of the party's tabs) and page
//! 0x18 (the numbers of the chosen tab, two columns of ten rows). `FUN_00265c48` builds page 0x17,
//! `FUN_00266328` page 0x18, `FUN_00266000` and `FUN_00266760` are their handlers (cancel steps back,
//! `-4` closes, the shoulder results `-10` and `-11` switch the tab with wrap in `FUN_00268c78`, Left and
//! Right toggle the column in `FUN_00268a08`). Which tabs exist is `FUN_002683c8`; which rows of a tab are
//! shown is its jump table at 0x381768 (the bit `column * 10 + row` of the word at `+0x14` of the page
//! object); the strings are fixed rows of three tables (0x3817e8, 0x381860, 0x3818d8) and the numbers
//! are filled by `FUN_002673c0` (jump table 0x3816d0) with `FUN_001918a0` (a `sprintf` into the row's
//! 0x20 byte buffer).
//!
//! Port: the tab list and the Sora tab. The forms' tabs, the summons' tab and the party members' tabs
//! are [GAP] (their number sources were read, see docs/menus.md 10.2, but the form experience table
//! `FUN_001d3b90` and the members' stat blocks have no counterpart in the app yet); the app's party is
//! Sora alone. The numbers are computed from `Progress` (the member record), the `lvup` table and the
//! `item_stat` rows of the equipment, exactly as `FUN_0019f920` and `FUN_001a0c08` build the status
//! block the page prints.

use std::collections::BTreeMap;

use bevy::prelude::*;
use kh2_image::text::Style;
use kh2_tables::player::FormLevels;
use kh2_tables::tables::{self as t, Fmlv, Item, ItemStat, Lvup};

use crate::assets::Disc;
use crate::audio::PlaySe;
use crate::data::GameData;
use crate::menu::{Menu, Phase};
use crate::menu_gfx::{MenuScene, Node};
use crate::menu_pages::Exe;
use crate::progress::Progress;
use crate::sora::Sora;

/// Camp page ids of the Status screen (`FUN_00251848` jump table, docs/menus.md 1.4).
pub const PAGE_TABS: u16 = 0x17;
pub const PAGE_NUMBERS: u16 = 0x18;

/// Offset in the party block (`0x33fec0`) of the byte row 7 prints (`FUN_002673c0` case 0, `lbu +0xfa8`).
const BONUS_OFFSET: usize = 0xfa8;

/// The grid's row bits: `column * 10 + row` of `FUN_00268568`'s word.
pub mod bit {
    pub const LEVEL: u32 = 0;
    pub const EXP: u32 = 1;
    pub const NEXT: u32 = 2;
    pub const HP: u32 = 3;
    pub const MP: u32 = 4;
    pub const AP: u32 = 5;
    pub const DRIVE: u32 = 6;
    /// Row 7 of the first column: "Bonus LV" (message 0x4567), the byte at party block `+0xfa8` (0x340e68).
    pub const BONUS: u32 = 7;
    /// Form levels: Valor 8, Master 9 (first column), Wisdom 18, Final 19 (second column).
    pub const VALOR: u32 = 8;
    pub const MASTER: u32 = 9;
    pub const STRENGTH: u32 = 10;
    pub const MAGIC: u32 = 11;
    pub const DEFENSE: u32 = 12;
    pub const FIRE: u32 = 13;
    pub const BLIZZARD: u32 = 14;
    pub const THUNDER: u32 = 15;
    pub const DARK: u32 = 16;
    pub const SUMMON: u32 = 17;
    pub const WISDOM: u32 = 18;
    pub const FINAL: u32 = 19;
}

/// A tab of the page-0x17 list, in the order of `FUN_002683c8`'s bit mask: the character, the four forms,
/// the summons, then the other party members.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tab {
    Sora,
    /// Form 1..=4 (Valor, Wisdom, Master, Final).
    Form(u8),
    Summons,
    /// The party's `n`-th member after the leader (1 or 2).
    Member(u8),
}

impl Tab {
    /// The tab index of the original (0 Sora, 1..=4 forms, 5 summons, 6..=8 members).
    pub fn index(self) -> usize {
        match self {
            Tab::Sora => 0,
            Tab::Form(f) => usize::from(f),
            Tab::Summons => 5,
            Tab::Member(m) => 5 + usize::from(m),
        }
    }

    /// The tabs the port draws: Sora's, the forms' and the summons' (the party members' tabs need the other
    /// members' status blocks, which the app does not have; see the module doc).
    pub fn ported(self) -> bool {
        !matches!(self, Tab::Member(_))
    }

    /// Which row table the tab reads (`FUN_00267de0`: 0 the character and the members, 1 the forms, 2 the summons)
    /// and which group set its row in the tab list uses (the forms' and summons' are the second one).
    pub fn table(self) -> usize {
        match self {
            Tab::Sora | Tab::Member(_) => 0,
            Tab::Form(_) => 1,
            Tab::Summons => 2,
        }
    }
}

/// The numbers of a form's or the summons' tab (`FUN_002673c0` at 0x267788 and 0x2678dc): the level (record `+2`,
/// the summon level byte), its experience in total (`FUN_001d3c98`, `FUN_001d3d48`: the record's experience plus
/// the `fmlv` rows below the level), the experience to the next level (`FUN_001d3c38`, `FUN_001d3cf8`) and the
/// gauge row, `level + 2`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LevelNumbers {
    pub level: i32,
    pub exp: i32,
    pub next: i32,
    pub gauge: i32,
}

/// `FUN_001d3bd0` and `FUN_001d4e68`: the level cap of the rows, 2 plus the number of owned form items (or summon
/// charms), 7 instead of 6.
pub fn level_cap(owned: usize) -> i32 {
    match 2 + owned as i32 {
        6 => 7,
        n => n,
    }
}

/// The tab numbers for `form` (0: the summons, whose `fmlv` rows are the form-0 rows) at `level` with the record's
/// own experience `rec_exp`.
pub fn level_numbers(levels: &FormLevels, form: u8, level: i32, rec_exp: i32, cap: i32) -> LevelNumbers {
    let below: i32 = (1..level.max(1)).map(|l| levels.get(form, l as u8).map_or(0, |r| r.exp)).sum();
    // `FUN_001d3c38`: a row exists below the cap; the next level needs the row's Exp minus the record's.
    let next = if level < cap { levels.get(form, level.clamp(0, 255) as u8).map_or(0, |r| r.exp - rec_exp) } else { 0 };
    LevelNumbers { level, exp: rec_exp + below, next, gauge: level + 2 }
}

/// `FUN_002683c8`: the mask of present tabs (u16 at `+0x18` of the page object). `party` is the number of
/// party members (`FUN_00282940`), `forms[i]` whether form `i + 1` is available (`FUN_00282b80`), `summons`
/// whether any summon charm is held (`FUN_00268338`).
pub fn tab_mask(party: u32, forms: [bool; 4], summons: bool) -> u16 {
    let mut m = 0u16;
    if party != 0 {
        m |= 1;
    }
    for k in 1..party.min(4) {
        m |= 1 << (k + 5);
    }
    for (i, &has) in forms.iter().enumerate() {
        if has {
            m |= 1 << (i + 1);
        }
    }
    if summons {
        m |= 0x20;
    }
    m
}

/// The tabs of a mask in list order (ascending bit; `FUN_00268828` maps a list index to a tab).
pub fn tabs(mask: u16) -> Vec<Tab> {
    (0..9)
        .filter(|&b| mask >> b & 1 != 0)
        .map(|b| match b {
            0 => Tab::Sora,
            1..=4 => Tab::Form(b as u8),
            5 => Tab::Summons,
            _ => Tab::Member(b as u8 - 5),
        })
        .collect()
}

/// `FUN_00268c78`: the shoulder buttons step the tab with wrap (`next` false is the previous one).
pub fn cycle(index: usize, count: usize, next: bool) -> usize {
    if count == 0 {
        0
    } else if next {
        if index + 1 >= count { 0 } else { index + 1 }
    } else if index == 0 {
        count - 1
    } else {
        index - 1
    }
}

/// `FUN_00268a08`: Left and Right toggle the column; the cursor keeps its row, limited to the new
/// column's row count.
pub fn toggle_column(row: usize, rows_in_new_column: usize) -> usize {
    row.min(rows_in_new_column.saturating_sub(1))
}

/// What one equipped item adds to the status block (`FUN_001a0c08`, the `item_stat` row of the item).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Gear {
    pub strength: i32,
    pub magic: i32,
    pub defense: i32,
    pub ap: i32,
    /// The seven resistance bytes of the row (physical, fire, blizzard, thunder, dark, light, general):
    /// the status keeps `100` and multiplies each by `byte / 100`, rounding up.
    pub resist: [i32; 7],
}

/// The `lvup` columns the page reads: row `level - 1` of the character's table (`FUN_001b50c0`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LevelRow {
    pub exp: i32,
    pub strength: i32,
    pub magic: i32,
    pub defense: i32,
    pub ap: i32,
}

impl From<&Lvup> for LevelRow {
    fn from(l: &Lvup) -> LevelRow {
        LevelRow { exp: l.exp, strength: i32::from(l.strength), magic: i32::from(l.magic), defense: i32::from(l.defense), ap: i32::from(l.ap) }
    }
}

/// Everything the numbers need besides `Progress`, read from the disc.
#[derive(Clone, Debug, Default)]
pub struct StatRules {
    /// Sora's level table (`lvup` character 0).
    pub levels: Vec<LevelRow>,
    /// Item id -> what the item adds.
    pub gear: BTreeMap<u16, Gear>,
    /// Item id -> the ability's AP cost (the high byte of the item's `StatEntry`, `item + 7`;
    /// `FUN_002688d8`).
    pub ap_cost: BTreeMap<u16, u8>,
    /// The `fmlv` rows (`FUN_001d3b90`: key `form << 4 | level`, the summons are form 0).
    pub form_levels: FormLevels,
}

impl StatRules {
    /// Builds the tables from the disc rows (`item`, `item_stat`) and Sora's `lvup` rows.
    pub fn from_tables(items: &[Item], stats: &[ItemStat], levels: &[Lvup]) -> StatRules {
        let mut gear = BTreeMap::new();
        let mut ap_cost = BTreeMap::new();
        for i in items {
            ap_cost.insert(i.id, (i.stat_entry >> 8) as u8);
            if let Some(s) = stats.iter().find(|s| s.id == i.stat_entry) {
                gear.insert(
                    i.id,
                    Gear {
                        strength: i32::from(s.attack),
                        magic: i32::from(s.magic),
                        defense: i32::from(s.defense),
                        ap: i32::from(s.ability_points),
                        resist: [
                            i32::from(s.unknown08),
                            i32::from(s.fire_resistance),
                            i32::from(s.ice_resistance),
                            i32::from(s.lightning_resistance),
                            i32::from(s.dark_resistance),
                            i32::from(s.unknown0d),
                            i32::from(s.general_resistance),
                        ],
                    },
                );
            }
        }
        StatRules { levels: levels.iter().map(LevelRow::from).collect(), gear, ap_cost, form_levels: FormLevels::default() }
    }

    /// `FUN_001b50c0(character, level)`: the row of `level` (the table is clamped to its length).
    pub fn level_row(&self, level: i32) -> LevelRow {
        let n = self.levels.len() as i32;
        if n == 0 {
            return LevelRow::default();
        }
        self.levels[(level.min(n) - 1).clamp(0, n - 1) as usize]
    }
}

/// The numbers of the Sora tab, in the units the page prints.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Numbers {
    pub level: i32,
    /// The experience total (`0x340e48`).
    pub exp: i32,
    /// `FUN_001878b0`: the next level's `lvup` Exp minus the total, 0 at level 99.
    pub next: i32,
    pub hp: (i32, i32),
    pub mp: (i32, i32),
    /// AP used by the equipped abilities (`FUN_002688d8`) over the total (`status + 0x18e`).
    pub ap: (i32, i32),
    pub strength: i32,
    pub magic: i32,
    pub defense: i32,
    /// Fire, blizzard, thunder, dark as `100 - byte` (`status + 0x1a9..0x1ac`).
    pub resist: [i32; 4],
    /// The drive gauge's level and maximum (`status + 0x1b1`, `+0x1b2`), shown when form 1 is available.
    pub drive: Option<(i32, i32)>,
    /// Level of the form (Valor, Wisdom, Master, Final) when it is available (record `+2`).
    pub forms: [Option<i32>; 4],
    /// The summon level (`0x340c88 + 6`) when a summon charm is held.
    pub summon: Option<i32>,
    /// "Bonus LV": the byte at party block `+0xfa8` (row 7), always shown on the Sora tab.
    pub bonus: i32,
    /// The rows of each form's tab (Valor, Wisdom, Master, Final) and of the summons' tab.
    pub form_tabs: [Option<LevelNumbers>; 4],
    pub summon_tab: Option<LevelNumbers>,
    /// Which of the four summon charms (`SUMMON_ITEMS`) are held: the summons' tab lists those (`FUN_00268758`).
    pub summons_held: [bool; 4],
}

/// The state the numbers read besides `Progress`: the live HP and MP (the app keeps them on Sora, not in
/// the member record) and which of the optional rows exist.
#[derive(Clone, Copy, Debug, Default)]
pub struct Live {
    pub hp: i32,
    pub max_hp: i32,
    pub mp: i32,
    pub max_mp: i32,
    /// `FUN_00282b20` and `FUN_00282b80`: the form is available. `Progress::owns_form` here.
    pub forms: [bool; 4],
    pub summons: bool,
    /// Which summon charms are held.
    pub summons_held: [bool; 4],
}

/// The Sora tab's numbers as `FUN_0019f920` builds the status block and `FUN_002673c0` prints it:
/// the record's level, the `lvup` row of the level plus the record's own bonus bytes (`+9`, `+0xa`, `+0xb`,
/// AP `+8`) plus every equipped item's `item_stat` row. The equipped items are the record's weapon words,
/// armor and accessory slots and, in a form, the form record's weapon (`FUN_0019f920`).
pub fn numbers(p: &Progress, rules: &StatRules, live: &Live) -> Numbers {
    let m = p.sora();
    let level = i32::from(m.level);
    let row = rules.level_row(level);
    let mut items: Vec<u16> = p.equipment();
    if p.current_form() != 0 {
        let w = p.form(p.current_form()).weapon;
        if w != 0 {
            items.push(w);
        }
    }
    let (mut st, mut mg, mut df, mut ap) = (0, 0, 0, 0);
    let mut res = [100i32; 7];
    for g in items.iter().filter_map(|i| rules.gear.get(i)) {
        st += g.strength;
        mg += g.magic;
        df += g.defense;
        ap += g.ap;
        for (r, &b) in res.iter_mut().zip(&g.resist) {
            *r = (*r * b + 99) / 100;
        }
    }
    let used: i32 = m.abilities.iter().filter(|&&(_, on)| on).map(|&(id, _)| i32::from(rules.ap_cost.get(&id).copied().unwrap_or(0))).sum();
    let exp = p.experience();
    // FUN_001878b0: the row of the current level gives the Exp the next level needs. The original never
    // shows a negative number because every experience gain runs the level-up check (`FUN_001b52f8`);
    // the app has no level-up yet, so the number is held at 0 [inferred].
    let next = if level < 99 { (rules.level_row(level).exp - exp).max(0) } else { 0 };
    let drive = p.drive_bytes();
    // The forms' tabs (`FUN_001d3bd0` caps the level rows by the number of owned forms), the summons' (`FUN_001d4e68`).
    let form_cap = level_cap(live.forms.iter().filter(|&&f| f).count());
    let form_tabs = [0, 1, 2, 3].map(|i| {
        live.forms[i].then(|| {
            let f = i as u8 + 1;
            level_numbers(&rules.form_levels, f, i32::from(p.form_level(f)), p.form_exp(f) as i32, form_cap)
        })
    });
    let summon_cap = level_cap(live.summons_held.iter().filter(|&&h| h).count());
    let summon_tab = live.summons.then(|| level_numbers(&rules.form_levels, 0, i32::from(p.summon_level()), p.summon_exp() as i32, summon_cap));
    Numbers {
        level,
        exp,
        next,
        hp: (live.hp.min(live.max_hp).max(0), live.max_hp),
        mp: (live.mp.min(live.max_mp).max(0), live.max_mp),
        ap: (used, row.ap + i32::from(m.ap) + ap),
        strength: row.strength + i32::from(m.strength) + st,
        magic: row.magic + i32::from(m.magic) + mg,
        defense: row.defense + i32::from(m.defense) + df,
        resist: [100 - res[1], 100 - res[2], 100 - res[3], 100 - res[4]],
        drive: live.forms[0].then(|| (i32::from(drive[1]), i32::from(drive[2]))),
        forms: [0, 1, 2, 3].map(|i| live.forms[i].then(|| i32::from(p.form_level(i as u8 + 1)))),
        summon: live.summons.then(|| i32::from(p.summon_level())),
        bonus: p.block().get(BONUS_OFFSET).copied().map_or(0, i32::from),
        form_tabs,
        summon_tab,
        summons_held: live.summons_held,
    }
}

/// The rows a tab shows, as bits of the page object's word (`FUN_00268568`'s jump table 0x381768): the forms' tab
/// has bits 0..3 and 10..16 (`FUN_002686f8`), the summons' tab bits 0..3 and `10 + j` for each summon j held
/// (`FUN_00268758`).
pub fn tab_bits(tab: Tab, n: &Numbers) -> Vec<u32> {
    match tab {
        Tab::Sora | Tab::Member(_) => sora_bits(n),
        Tab::Form(_) => (0..4).chain(10..17).collect(),
        Tab::Summons => (0..4).chain(n.summons_held.iter().enumerate().filter(|&(_, &h)| h).map(|(j, _)| 10 + j as u32)).collect(),
    }
}

/// The value text of a row of a tab: the forms' and summons' tabs print the level, experience, next level and the
/// gauge row for bits 0..3 (the formats at 0x3816b8, `%d`), the forms' second column the leader's strength, magic,
/// defense and resistances (`FUN_002673c0` at 0x267788: the status block `+0x188..` and `100 - +0x1a9..`).
pub fn tab_value(tab: Tab, n: &Numbers, b: u32) -> String {
    let rows = match tab {
        Tab::Form(f) => n.form_tabs.get(usize::from(f).wrapping_sub(1)).copied().flatten(),
        Tab::Summons => n.summon_tab,
        _ => return value_text(n, b),
    };
    match (b, rows) {
        (0, Some(r)) => r.level.to_string(),
        (1, Some(r)) => r.exp.to_string(),
        (2, Some(r)) => r.next.to_string(),
        (3, Some(r)) => r.gauge.to_string(),
        (10..=16, _) if matches!(tab, Tab::Form(_)) => value_text(n, b),
        _ => String::new(),
    }
}

/// The rows the Sora tab shows (`FUN_00268568`'s case 0 at 0x2685a0: bits 0..5 and 10..16 always, 6 with
/// the first form, 7 on this tab, 17 with a summon, 8, 18, 9 and 19 with forms 1 to 4), in table order.
pub fn sora_bits(n: &Numbers) -> Vec<u32> {
    let mut v: Vec<u32> = (0..6).collect();
    if n.drive.is_some() {
        v.push(bit::DRIVE);
    }
    v.push(bit::BONUS);
    if n.forms[0].is_some() {
        v.push(bit::VALOR);
    }
    if n.forms[2].is_some() {
        v.push(bit::MASTER);
    }
    v.extend(10..17);
    if n.summon.is_some() {
        v.push(bit::SUMMON);
    }
    if n.forms[1].is_some() {
        v.push(bit::WISDOM);
    }
    if n.forms[3].is_some() {
        v.push(bit::FINAL);
    }
    v
}

/// The value text of a row (`FUN_001918a0` with the formats at 0x3816b8 `%d`, 0x3816c0 `%d/%d`,
/// 0x3816c8 `%d%%`).
pub fn value_text(n: &Numbers, b: u32) -> String {
    let pair = |(a, m): (i32, i32)| format!("{a}/{m}");
    match b {
        bit::LEVEL => n.level.to_string(),
        bit::EXP => n.exp.to_string(),
        bit::NEXT => n.next.to_string(),
        bit::HP => pair(n.hp),
        bit::MP => pair(n.mp),
        bit::AP => pair(n.ap),
        bit::DRIVE => n.drive.map_or(String::new(), pair),
        bit::BONUS => n.bonus.to_string(),
        bit::VALOR => n.forms[0].map_or(String::new(), |v| v.to_string()),
        bit::WISDOM => n.forms[1].map_or(String::new(), |v| v.to_string()),
        bit::MASTER => n.forms[2].map_or(String::new(), |v| v.to_string()),
        bit::FINAL => n.forms[3].map_or(String::new(), |v| v.to_string()),
        bit::STRENGTH => n.strength.to_string(),
        bit::MAGIC => n.magic.to_string(),
        bit::DEFENSE => n.defense.to_string(),
        bit::FIRE => format!("{}%", n.resist[0]),
        bit::BLIZZARD => format!("{}%", n.resist[1]),
        bit::THUNDER => format!("{}%", n.resist[2]),
        bit::DARK => format!("{}%", n.resist[3]),
        bit::SUMMON => n.summon.map_or(String::new(), |v| v.to_string()),
        _ => String::new(),
    }
}

// --- the tables the screen reads from the executable ------------------------------------------------

/// Address of the three row tables: Sora and party members, forms, summons (two columns of ten rows of
/// six bytes each, `0x3c` bytes per column; `FUN_00267de0` indexes them by the tab and the column byte).
const ROW_TABLES: [u32; 3] = [0x0038_17e8, 0x0038_1860, 0x0038_18d8];
/// Row type -> (value plate group, row group): 0x3817b8, four bytes per type (`FUN_00266ae8`,
/// `FUN_00267980`).
const TYPE_TABLE: u32 = 0x0038_17b8;
/// Tab row group sets: `FUN_00253048(n)` = 0x380860 + 8n (Sora and members), `FUN_00253060(n)` =
/// 0x380870 + 8n (forms and summons); n 0 is the idle set, n 1 the selected set.
const TAB_SETS: [u32; 2] = [0x0038_0860, 0x0038_0870];
/// The name messages of the tab list (`FUN_00268b90`): members through the table at 0x37ba90 (`FUN_001b5890`,
/// indexed by member id), forms through `0x381a38 + 2` ..., the summons 0xc5b1.
const MEMBER_NAMES: u32 = 0x0037_ba90;
/// The help line of page 0x17 per tab (`FUN_00267de0`: the table at 0x3817d0).
const TAB_HELP: u32 = 0x0038_17d0;
/// The four summon charm items whose possession adds the summons tab (`FUN_00268338`, 0x381760).
const SUMMON_ITEMS: u32 = 0x0038_1760;
/// The forms' label messages (`FUN_00268b90`: `0x381a38 + 2 * form`) and the summons' tab name (message 0xc5b1).
const FORM_NAMES: u32 = 0x0038_1a38;
const SUMMONS_NAME: u32 = 0x45b1;

/// One row of a row table.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RowDef {
    pub name: u32,
    pub help: u32,
    /// Row type 0..4: selects the row's group and the value plate through `TYPE_TABLE`.
    pub kind: usize,
}

/// What the screen reads from the executable.
#[derive(Clone, Debug, Default)]
pub struct Tables {
    /// `rows[table][column][row]`.
    pub rows: [[[Option<RowDef>; 10]; 2]; 3],
    /// Per row type: (value plate group, row group).
    pub types: [(usize, usize); 5],
    /// `tab_sets[table][n]`: four group ids (enter, leave, loop, spare).
    pub tab_sets: [[[usize; 4]; 2]; 2],
    pub tab_help: [u32; 9],
    pub summon_items: [u16; 4],
    pub member_names: Vec<u32>,
    /// The forms' names (`0x381a38 + 2 * form`, form 1..=4; index 0 is the main weapon's label).
    pub form_names: Vec<u32>,
}

impl Tables {
    pub fn read(exe: &Exe) -> Option<Tables> {
        let mut t = Tables::default();
        for (k, &base) in ROW_TABLES.iter().enumerate() {
            for c in 0..2u32 {
                for r in 0..10u32 {
                    let a = base + 0x3c * c + 6 * r;
                    let (name, help, kind) = (exe.u16(a)?, exe.u16(a + 2)?, exe.u16(a + 4)?);
                    t.rows[k][c as usize][r as usize] =
                        (name != 0 && kind != 0xffff).then(|| RowDef { name: u32::from(name & 0x7fff), help: u32::from(help & 0x7fff), kind: usize::from(kind) });
                }
            }
        }
        for (k, ty) in t.types.iter_mut().enumerate() {
            *ty = (usize::from(exe.u16(TYPE_TABLE + 4 * k as u32)?), usize::from(exe.u16(TYPE_TABLE + 4 * k as u32 + 2)?));
        }
        for (i, &base) in TAB_SETS.iter().enumerate() {
            for n in 0..2u32 {
                for k in 0..4u32 {
                    t.tab_sets[i][n as usize][k as usize] = usize::from(exe.u16(base + 8 * n + 2 * k)?);
                }
            }
        }
        for (i, h) in t.tab_help.iter_mut().enumerate() {
            *h = u32::from(exe.u16(TAB_HELP + 2 * i as u32)? & 0x7fff);
        }
        for (i, s) in t.summon_items.iter_mut().enumerate() {
            *s = exe.u16(SUMMON_ITEMS + 2 * i as u32)?;
        }
        t.form_names = (0..5).filter_map(|i| exe.u16(FORM_NAMES + 2 * i).map(|v| u32::from(v & 0x7fff))).collect();
        t.member_names = (0..16).filter_map(|i| exe.u16(MEMBER_NAMES + 2 * i).map(|v| u32::from(v & 0x7fff))).collect();
        Some(t)
    }
}

// --- the screen -------------------------------------------------------------------------------------

/// System sounds (docs/menus.md 9: bank 2 of the sound driver, `PlaySe::number(2000 + id)`).
pub const CURSOR: u32 = 2001;
pub const CONFIRM: u32 = 2002;
pub const CANCEL: u32 = 2004;

/// The pages this module draws; `menu.rs` opens them from the Status row.
pub const PAGES: [u16; 2] = [PAGE_TABS, PAGE_NUMBERS];

/// `camp.2ld` sequence 1 holds the rows and plates.
const SEQ_MAIN: usize = 1;
/// The hand's group (`FUN_00253f98` gives 0x19 for the lists).
const HAND_GROUP: usize = 0x19;
/// Key repeat of the lists (as `menu.rs`: frames before it starts and between moves) [inferred].
const REPEAT_DELAY: u32 = 24;
const REPEAT_EVERY: u32 = 6;

/// Where things sit, in menu units (the 512 x 448 canvas, before the 16 line offset). The groups'
/// own sizes come from the sequence (rows 200 wide and 20 high above their origin, a row pitch of 24, a
/// column pitch of 232; the value plates 164 wide with a pitch of 170); the list objects' origins are set
/// by code that was not read (`FUN_00138ed0` on the lists, `FUN_0029b600`), so these are chosen to fit the
/// canvas [inferred] (docs/menus.md 10.3): page 0x17 shows the tab list at the left and the plates of the
/// highlighted tab at the right, page 0x18 the wide rows across the screen.
mod at {
    /// The tab list: the origin of the first row, as page 0's list (`menu.rs`), and the row pitch.
    pub const TAB_X: f32 = 0.0;
    pub const TAB_Y: f32 = 0.0;
    pub const TAB_PITCH: f32 = 26.0;
    /// Page 0x17: the preview plates' left edge and the first row's origin (the baseline of the plate).
    pub const PREVIEW: [f32; 2] = [164.0, 104.0];
    /// Page 0x18: the wide rows.
    pub const WIDE: [f32; 2] = [40.0, 104.0];
    /// Row pitch of both pages (`group.unk1c`, 24 for every row group of the page).
    pub const ROW_PITCH: f32 = 24.0;
    /// The label from the row's group anchor, and the value's top from the row origin.
    pub const LABEL: [f32; 2] = [-7.0, 1.5];
    pub const VALUE_TOP: f32 = -17.5;
    pub const HAND: [f32; 2] = [-13.0, 0.0];
    pub const IDLE: [u8; 4] = [0x50, 0x50, 0x58, 0x80];
    pub const SELECTED: [u8; 4] = [0xe5, 0xe5, 0xe5, 0x80];
    pub const VALUE: [u8; 4] = [0xe5, 0xe5, 0xe5, 0x80];
    pub const HELP: [f32; 2] = [137.0, 337.0];
    pub const SIZE: f32 = 15.0;
    pub const FONT: usize = 1;
}

/// Tab row nodes: Sora, the forms, the summons and the other members never exceed this many tabs.
const MAX_TAB_NODES: usize = 10;

/// The nodes the screen draws with, created once in the scene.
struct Nodes {
    tabs: Vec<usize>,
    /// Per column and row: the row node (a group) and its value node (text only, at the row's origin).
    rows: [Vec<usize>; 2],
    values: [Vec<usize>; 2],
    hand: usize,
    help: usize,
}

/// The state of the Status pages.
#[derive(Resource, Default)]
pub struct StatusUi {
    ready: bool,
    tables: Tables,
    rules: StatRules,
    nodes: Option<Nodes>,
    /// The page drawn now (0: none).
    page: u16,
    /// Visibility the page-0 nodes had when this screen hid them.
    saved: Vec<(usize, bool)>,
    /// The tab list as the mask gives it (only the ported ones) and the cursor in it.
    tabs: Vec<Tab>,
    pub tab: usize,
    /// The cursor of the numbers page: column and the row index in that column's shown rows.
    pub col: usize,
    pub row: [usize; 2],
    /// The numbers last drawn (a change redraws the values).
    drawn: Option<(Numbers, u16, usize, usize, [usize; 2])>,
    /// Frames the up or down key has been held and which way (the repeat does not wrap).
    held: (i8, u32),
    /// Real seconds since the menu started to close.
    closing: f32,
}

impl StatusUi {
    /// The rows of column `c` in the order they are drawn: (table row, bit).
    fn column(tab: Tab, numbers: &Numbers, c: usize) -> Vec<(usize, u32)> {
        tab_bits(tab, numbers).into_iter().filter(|&b| b as usize / 10 == c).map(|b| (b as usize % 10, b)).collect()
    }

    fn rows_in(&self, numbers: &Numbers, c: usize) -> usize {
        Self::column(self.tabs.get(self.tab).copied().unwrap_or(Tab::Sora), numbers, c).len()
    }
}

pub struct StatusPlugin;

impl Plugin for StatusPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<StatusUi>()
            // Before the menu's own input, which would read these presses as page 0's.
            .add_systems(
                PreUpdate,
                input.before(crate::menu::input).after(bevy::input::InputSystems).after(crate::harness::Keys),
            )
            .add_systems(Update, tick.before(crate::schedule::Frame::Popups));
    }
}

/// The numbers of Sora's tab from the app's state.
fn gather(p: &Progress, ui: &StatusUi, sora: Option<&Sora>, data: Option<&GameData>) -> Numbers {
    let live = Live {
        hp: sora.map_or(i32::from(p.sora().hp), |s| s.hp),
        max_hp: sora.map_or(i32::from(p.sora().hp_max), |s| s.stats.max_hp),
        mp: sora.map_or(i32::from(p.sora().mp), |s| s.mp),
        max_mp: data.map_or(i32::from(p.sora().mp_max), |d| d.sora_max_mp),
        forms: [1, 2, 3, 4].map(|f| p.owns_form(f)),
        summons: ui.tables.summon_items.iter().any(|&i| i != 0 && p.item_count(i) > 0),
        summons_held: ui.tables.summon_items.map(|i| i != 0 && p.item_count(i) > 0),
    };
    numbers(p, &ui.rules, &live)
}

/// Reads the tables the screen needs, once.
fn load(ui: &mut StatusUi, disc: &Disc, data: Option<&GameData>) {
    ui.ready = true;
    let Some(exe) = disc.read_root("SLUS_210.05").and_then(Exe::load) else {
        warn!("menu: status: the executable is not readable; the Status pages draw nothing");
        return;
    };
    match Tables::read(&exe) {
        Some(t) => ui.tables = t,
        None => warn!("menu: status: the row tables are not readable"),
    }
    let rows = {
        let Ok(mut a) = disc.0.lock() else { return };
        (kh2_tables::load::<Item>(&mut a, &t::ITEM), kh2_tables::load::<ItemStat>(&mut a, &t::ITEM_STAT))
    };
    let fmlv = {
        let Ok(mut a) = disc.0.lock() else { return };
        kh2_tables::load::<Fmlv>(&mut a, &t::FMLV).ok()
    };
    if let (Ok(items), Ok(stats)) = rows {
        let levels = data.map(|d| d.sora_levels.as_slice()).unwrap_or_default();
        ui.rules = StatRules::from_tables(&items.rows, &stats.rows, levels);
        if let Some(f) = fmlv {
            ui.rules.form_levels = FormLevels::from_rows(&f.rows);
        }
    } else {
        warn!("menu: status: the item tables are not readable");
    }
}

/// The tabs of the list: the mask of the party, forms and summons, reduced to the tabs the port draws.
fn tab_list(p: &Progress, ui: &StatusUi) -> Vec<Tab> {
    let forms = [1, 2, 3, 4].map(|f| p.owns_form(f));
    let summons = ui.tables.summon_items.iter().any(|&i| i != 0 && p.item_count(i) > 0);
    // The app's party is Sora alone (docs/party.md).
    tabs(tab_mask(1, forms, summons)).into_iter().filter(|t| t.ported()).collect()
}

#[allow(clippy::too_many_arguments)]
fn input(
    mut menu: ResMut<Menu>,
    mut ui: ResMut<StatusUi>,
    mut keys: ResMut<ButtonInput<KeyCode>>,
    mut se: MessageWriter<PlaySe>,
    progress: Option<Res<Progress>>,
    sora: Query<&Sora>,
    data: Option<Res<GameData>>,
) {
    if !matches!(menu.phase, Phase::Opening | Phase::Open) || !PAGES.contains(&menu.page) {
        ui.held = (0, 0);
        return;
    }
    let Some(p) = progress else { return };
    let confirm = keys.just_pressed(KeyCode::KeyJ);
    let cancel = keys.just_pressed(KeyCode::Escape) || keys.just_pressed(KeyCode::Backspace) || keys.just_pressed(KeyCode::KeyK);
    let (left, right) = (keys.just_pressed(KeyCode::ArrowLeft), keys.just_pressed(KeyCode::ArrowRight));
    let (prev, next) = (keys.just_pressed(KeyCode::BracketLeft), keys.just_pressed(KeyCode::BracketRight));
    let dir = if keys.pressed(KeyCode::ArrowUp) { -1i8 } else if keys.pressed(KeyCode::ArrowDown) { 1 } else { 0 };
    let fresh = keys.just_pressed(KeyCode::ArrowUp) || keys.just_pressed(KeyCode::ArrowDown);
    // One move per new press (wrapping) and, while held, one per `REPEAT_EVERY` frames after the delay
    // (not wrapping): `FUN_002977a8`, docs/menus.md 1.7.
    let mut step = None;
    if dir == 0 {
        ui.held = (0, 0);
    } else if fresh || ui.held.0 != dir {
        ui.held = (dir, 0);
        step = Some(true);
    } else {
        ui.held.1 += 1;
        if ui.held.1 >= REPEAT_DELAY && (ui.held.1 - REPEAT_DELAY).is_multiple_of(REPEAT_EVERY) {
            step = Some(false);
        }
    }
    let count = ui.tabs.len();
    let numbers = gather(&p, &ui, sora.single().ok(), data.as_deref());
    match menu.page {
        PAGE_TABS => {
            if let Some(wrap) = step {
                let to = crate::menu::step_cursor(ui.tab, count, dir, wrap);
                if to != ui.tab {
                    ui.tab = to;
                    se.write(PlaySe::number(CURSOR, None));
                    info!("menu: status tab {to}");
                }
            }
            if confirm && count > 0 {
                // Page 0x18 with the cursor where this tab last left it (`FUN_00252fb8(0x17)`).
                menu.page = PAGE_NUMBERS;
                se.write(PlaySe::number(CONFIRM, None));
                info!("menu: page {PAGE_NUMBERS:#x}");
            } else if cancel {
                menu.page = 0;
                se.write(PlaySe::number(CANCEL, None));
                info!("menu: page 0");
            }
        }
        _ => {
            let rows = ui.rows_in(&numbers, ui.col);
            if let Some(wrap) = step {
                let c = ui.col;
                let to = crate::menu::step_cursor(ui.row[c], rows, dir, wrap);
                if to != ui.row[c] {
                    ui.row[c] = to;
                    se.write(PlaySe::number(CURSOR, None));
                    info!("menu: status row {} {}", c, to);
                }
            }
            if left || right {
                // `FUN_00268a08`: the other column, the row limited to its row count.
                let other = 1 - ui.col;
                let n = ui.rows_in(&numbers, other);
                if n > 0 {
                    ui.row[other] = toggle_column(ui.row[ui.col], n);
                    ui.col = other;
                    se.write(PlaySe::number(CURSOR, None));
                    info!("menu: status column {} row {}", other, ui.row[other]);
                }
            }
            if prev || next {
                let to = cycle(ui.tab, count, next);
                if to != ui.tab {
                    ui.tab = to;
                    se.write(PlaySe::number(CURSOR, None));
                    info!("menu: status tab {to}");
                }
            }
            if cancel {
                menu.page = PAGE_TABS;
                se.write(PlaySe::number(CANCEL, None));
                info!("menu: page {PAGE_TABS:#x}");
            }
        }
    }
    // The keys this screen took do not reach the menu's own input.
    for k in [
        KeyCode::KeyJ,
        KeyCode::Escape,
        KeyCode::Backspace,
        KeyCode::KeyK,
        KeyCode::ArrowLeft,
        KeyCode::ArrowRight,
        KeyCode::ArrowUp,
        KeyCode::ArrowDown,
        KeyCode::BracketLeft,
        KeyCode::BracketRight,
    ] {
        keys.clear_just_pressed(k);
    }
}

/// Creates the nodes (hidden), once the menu's sequences are loaded.
fn build(scene: &mut MenuScene) -> Nodes {
    let new = |scene: &mut MenuScene, seq: Option<usize>| {
        let mut n = Node::new(seq);
        n.visible = false;
        scene.add(n)
    };
    let tabs = (0..MAX_TAB_NODES).map(|_| new(scene, Some(SEQ_MAIN))).collect();
    let mut rows = [Vec::new(), Vec::new()];
    let mut values = [Vec::new(), Vec::new()];
    for c in 0..2 {
        for _ in 0..10 {
            rows[c].push(new(scene, Some(SEQ_MAIN)));
        }
        for _ in 0..10 {
            values[c].push(new(scene, None));
        }
    }
    let hand = new(scene, Some(SEQ_MAIN));
    let help = new(scene, None);
    Nodes { tabs, rows, values, hand, help }
}

/// One frame of the Status pages: follow the page the menu is on, draw the rows and the numbers.
#[allow(clippy::too_many_arguments)]
fn tick(
    menu: Res<Menu>,
    mut ui: ResMut<StatusUi>,
    mut scene: ResMut<MenuScene>,
    disc: Res<Disc>,
    progress: Option<Res<Progress>>,
    data: Option<Res<GameData>>,
    sora: Query<&Sora>,
    real: Res<Time<Real>>,
) {
    let want = if matches!(menu.phase, Phase::Opening | Phase::Open | Phase::Closing) && PAGES.contains(&menu.page) { menu.page } else { 0 };
    if menu.phase == Phase::Closed {
        // The menu hid every node itself; forget what this screen drew.
        ui.page = 0;
        ui.saved.clear();
        ui.drawn = None;
        ui.closing = 0.0;
        return;
    }
    if want == 0 && ui.page == 0 {
        return;
    }
    let Some(p) = progress else { return };
    if !ui.ready {
        load(&mut ui, &disc, data.as_deref());
    }
    if scene.seqs.get(SEQ_MAIN).is_none() {
        return;
    }
    if ui.nodes.is_none() {
        ui.nodes = Some(build(&mut scene));
    }
    if want != ui.page {
        let was = ui.page;
        if was == 0 {
            // Entering from page 0: hide what page 0 draws (the numbers are drawn below).
            let ids = menu.page0_nodes();
            ui.saved = ids.iter().map(|&i| (i, scene.nodes[i].visible)).collect();
            for i in ids {
                scene.nodes[i].visible = false;
            }
            ui.tabs = tab_list(&p, &ui);
            ui.tab = ui.tab.min(ui.tabs.len().saturating_sub(1));
        } else if want == 0 {
            // Back on page 0: hide this screen and give page 0 its nodes back.
            let nodes = ui.nodes.take().expect("built");
            hide(&mut scene, &nodes);
            ui.nodes = Some(nodes);
            for (i, v) in std::mem::take(&mut ui.saved) {
                scene.nodes[i].visible = v;
            }
        }
        ui.page = want;
        ui.drawn = None;
    }
    if ui.page == 0 {
        return;
    }
    let nodes = ui.nodes.take().expect("built");
    if menu.phase == Phase::Closing {
        ui.closing += real.delta_secs();
        let fade = 1.0 - (ui.closing * 60.0 / 8.0).min(1.0);
        for id in nodes.tabs.iter().chain(nodes.rows.iter().flatten()).chain(nodes.values.iter().flatten()).chain([&nodes.help]) {
            scene.nodes[*id].label_fade = fade;
        }
    }
    let numbers = gather(&p, &ui, sora.single().ok(), data.as_deref());
    let key = (numbers.clone(), ui.page, ui.tab, ui.col, ui.row);
    if ui.drawn.as_ref() != Some(&key) {
        draw(&ui, &nodes, &mut scene, &numbers);
        ui.drawn = Some(key);
    }
    ui.nodes = Some(nodes);
}

fn hide(scene: &mut MenuScene, n: &Nodes) {
    for id in n.tabs.iter().chain(n.rows.iter().flatten()).chain(n.values.iter().flatten()).chain([&n.hand, &n.help]) {
        scene.nodes[*id].visible = false;
    }
}

/// Lays out the current page (rows, labels, values, hand, help line).
fn draw(ui: &StatusUi, n: &Nodes, scene: &mut MenuScene, numbers: &Numbers) {
    let wide = ui.page == PAGE_NUMBERS;
    hide(scene, n);
    let Some(text) = scene.text.as_ref() else { return };
    let label = |id: u32, selected: bool| {
        text.message(id, Style { font: at::FONT, size: Some(at::SIZE), color: if selected { at::SELECTED } else { at::IDLE }, ..Style::default() }, [0.0, 0.0])
    };
    let value = |s: &str, x: f32| {
        text.literal(s, Style { font: at::FONT, size: Some(at::SIZE), color: at::VALUE, align: 1, ..Style::default() }, [x, at::VALUE_TOP])
    };
    // The tab list (page 0x17 only): the member's name on the row plates of page 0's list.
    let mut tab_labels = Vec::new();
    for (k, tab) in ui.tabs.iter().enumerate() {
        let name = match tab {
            Tab::Sora => ui.tables.member_names.get(usize::from(crate::progress::SORA)).copied(),
            // `FUN_00268b90`: a form's name is the label at 0x381a38 + 2 * form, the summons' tab 0xc5b1.
            Tab::Form(f) => ui.tables.form_names.get(usize::from(*f)).copied(),
            Tab::Summons => Some(SUMMONS_NAME),
            _ => None,
        };
        tab_labels.push((k, name.and_then(|id| label(id, !wide && k == ui.tab))));
    }
    // The rows: one group per row type, packed per column.
    let mut cells = Vec::new();
    let tab_now = ui.tabs.get(ui.tab).copied().unwrap_or(Tab::Sora);
    for c in 0..2 {
        for (i, (r, b)) in StatusUi::column(tab_now, numbers, c).into_iter().enumerate() {
            let Some(def) = ui.tables.rows[tab_now.table()][c][r] else { continue };
            let selected = wide && c == ui.col && i == ui.row[c];
            cells.push((c, i, b, def, label(def.name, selected), value(&tab_value(tab_now, numbers, b), if wide { 180.0 } else { 160.0 })));
        }
    }
    let help = if wide {
        cells.iter().find(|(c, i, ..)| *c == ui.col && *i == ui.row[ui.col]).map(|(_, _, _, def, ..)| def.help)
    } else {
        ui.tabs.get(ui.tab).map(|t| ui.tables.tab_help[t.index()])
    };
    let help_variable = ui.tables.member_names.get(usize::from(crate::progress::SORA)).copied();
    let pitch = scene.seqs.get(SEQ_MAIN).and_then(|s| s.groups.get(0x86)).map_or(at::TAB_PITCH, |g| g.unk20 as f32);
    let origin = if wide { at::WIDE } else { at::PREVIEW };
    let mut hand_on = None;
    if !wide {
        for (k, label) in tab_labels {
            let Some(&id) = n.tabs.get(k) else { break };
            let set = ui.tables.tab_sets[usize::from(ui.tabs[k].table() != 0)][usize::from(k == ui.tab)];
            scene.nodes[id].visible = true;
            scene.nodes[id].off = [at::TAB_X, at::TAB_Y + pitch * k as f32];
            scene.nodes[id].label = label;
            scene.nodes[id].label_anchor = Some(at::LABEL);
            scene.nodes[id].label_fade = 1.0;
            scene.set_group_then(id, set[0], set[2]);
            if k == ui.tab {
                hand_on = Some(id);
            }
        }
    }
    for (c, i, _b, def, lab, val) in cells {
        let (plate, row_group) = ui.tables.types.get(def.kind).copied().unwrap_or((0, 0));
        let group = if wide { row_group } else { plate };
        let (rid, vid) = (n.rows[c][i], n.values[c][i]);
        let pitch_x = scene.seqs.get(SEQ_MAIN).and_then(|s| s.groups.get(group)).map_or(0.0, |g| g.unk20 as f32);
        scene.nodes[rid].visible = true;
        scene.nodes[rid].pos = [origin[0] + pitch_x * c as f32, origin[1] + at::ROW_PITCH * i as f32];
        scene.nodes[rid].label = lab;
        scene.nodes[rid].label_anchor = Some(at::LABEL);
        scene.nodes[rid].label_fade = 1.0;
        scene.set_group(rid, group);
        scene.nodes[vid].visible = true;
        // The value sits at the row group's own x from the row's origin, not from its anchor.
        scene.nodes[vid].pos = scene.nodes[rid].pos;
        scene.nodes[vid].label = val.into();
        scene.nodes[vid].label_fade = 1.0;
        if wide && c == ui.col && i == ui.row[c] {
            hand_on = Some(rid);
        }
    }
    if let Some(id) = hand_on {
        scene.nodes[n.hand].parent = Some(id);
        scene.nodes[n.hand].off = at::HAND;
        scene.nodes[n.hand].visible = true;
        scene.set_group(n.hand, HAND_GROUP);
    }
    if let (Some(id), Some(text)) = (help, scene.text.as_ref()) {
        let variable = help_variable.and_then(|v| text.messages.as_ref()?.tokens(v)).unwrap_or_default();
        let l = text.message(id, Style { font: 0, size: Some(15.0), color: [0x30, 0x7c, 0x80, 0x80], variable, ..Style::default() }, at::HELP);
        scene.nodes[n.help].label = l;
        scene.nodes[n.help].visible = true;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::progress::{off, ItemIndex};
    use std::sync::Arc;

    /// A party block with Sora's record filled by hand: level, the record's bonus bytes, equipment,
    /// abilities, the experience word and the form levels.
    fn block(f: impl FnOnce(&mut Vec<u8>)) -> Progress {
        let mut b = vec![0u8; off::LEN];
        f(&mut b);
        Progress::from_block(&b, Arc::new(ItemIndex::default()))
    }

    fn put16(b: &mut [u8], o: usize, v: u16) {
        b[o..o + 2].copy_from_slice(&v.to_le_bytes());
    }

    fn rules() -> StatRules {
        let lv = |exp, strength, magic, defense, ap| LevelRow { exp, strength, magic, defense, ap };
        StatRules {
            // Row n is level n + 1; the exp column is what the level needs to be left.
            levels: vec![lv(100, 5, 6, 2, 3), lv(250, 6, 7, 3, 4), lv(500, 7, 8, 4, 5)],
            gear: BTreeMap::from([
                (41, Gear { strength: 3, magic: 2, defense: 0, ap: 0, resist: [100; 7] }),
                (60, Gear { strength: 0, magic: 0, defense: 4, ap: 1, resist: [100, 50, 100, 100, 80, 100, 100] }),
                (61, Gear { strength: 1, magic: 0, defense: 1, ap: 2, resist: [100, 50, 100, 100, 100, 100, 100] }),
            ]),
            ap_cost: BTreeMap::from([(82, 1), (90, 3), (7, 0)]),
            form_levels: FormLevels::default(),
        }
    }

    fn live() -> Live {
        Live { hp: 90, max_hp: 80, mp: 40, max_mp: 100, ..Live::default() }
    }

    #[test]
    fn numbers_come_from_the_record_the_lvup_row_and_the_equipment() {
        let p = block(|b| {
            b[0xf] = 2; // level 2: lvup row 1
            b[0x8] = 10; // the record's AP bonus byte
            b[0x9] = 1; // strength bonus
            b[0xa] = 2; // magic bonus
            b[0xb] = 3; // defense bonus
            put16(b, 0, 41); // weapon hand 0
            b[0x10] = 1;
            put16(b, 0x14, 60); // armor slot
            b[0x11] = 1;
            put16(b, 0x24, 61); // accessory slot
            put16(b, 0x54, 82 | 0x8000); // equipped ability
            put16(b, 0x56, 90); // learned, not equipped
            put16(b, 0x58, 7 | 0x8000); // equipped, free
            b[off::EXPERIENCE..off::EXPERIENCE + 4].copy_from_slice(&180i32.to_le_bytes());
        });
        let n = numbers(&p, &rules(), &live());
        assert_eq!(n.level, 2);
        assert_eq!(n.exp, 180);
        // The next level's threshold is the row of the current level (FUN_001878b0 via FUN_001b50c0):
        // row index level - 1 = 250, minus the 180 gained.
        assert_eq!(n.next, 70);
        assert_eq!(n.strength, 6 + 1 + 3 + 0 + 1, "lvup + record + weapon + armor + accessory");
        assert_eq!(n.magic, 7 + 2 + 2);
        assert_eq!(n.defense, 3 + 3 + 4 + 1);
        assert_eq!(n.ap, (1, 4 + 10 + 1 + 2), "equipped costs 1 + 0 over the lvup, record and equipment AP");
        // Fire: 100 * 50 / 100 = 50, then 50 * 50 / 100 rounded up = 25 -> 75 % shown; dark 80 -> 20 %.
        assert_eq!(n.resist, [75, 0, 0, 20]);
        assert_eq!(n.hp, (80, 80), "the current HP is held at the maximum");
        assert_eq!(n.mp, (40, 100));
        assert_eq!((n.drive, n.summon), (None, None));
    }

    #[test]
    fn exp_to_next_clamps_at_level_99_and_below_zero() {
        let rules = rules();
        let p = block(|b| {
            b[0xf] = 99;
        });
        assert_eq!(numbers(&p, &rules, &live()).next, 0);
        // Past the last row of the table the last row serves (FUN_001b50c0 clamps the level).
        assert_eq!(rules.level_row(50).exp, 500);
        // More experience than the row asks for (no level-up in the app yet) is shown as 0, never negative.
        let p = block(|b| {
            b[0xf] = 1;
            b[off::EXPERIENCE..off::EXPERIENCE + 4].copy_from_slice(&900i32.to_le_bytes());
        });
        assert_eq!(numbers(&p, &rules, &live()).next, 0);
    }

    #[test]
    fn optional_rows_follow_forms_summons_and_the_gauge() {
        let p = block(|b| {
            b[off::FORM_BASE + 40 + 2] = 3; // Valor level
            b[off::FORM_BASE + 3 * 40 + 2] = 5; // Master level
            b[off::SUMMON_LEVEL] = 2;
            b[off::DRIVE + 1] = 4;
            b[off::DRIVE + 2] = 6;
            b[BONUS_OFFSET] = 7;
        });
        let l = Live { forms: [true, false, true, false], summons: true, ..live() };
        let n = numbers(&p, &rules(), &l);
        assert_eq!(n.forms, [Some(3), None, Some(5), None]);
        assert_eq!((n.drive, n.summon, n.bonus), (Some((4, 6)), Some(2), 7));
        // Table order of the shown rows: first column 0..5, the drive 6, the bonus level 7, Valor 8, Master 9;
        // the second column 10..16, the summon 17.
        assert_eq!(sora_bits(&n), vec![0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17]);
        let bare = numbers(&p, &rules(), &live());
        assert_eq!(sora_bits(&bare), vec![0, 1, 2, 3, 4, 5, 7, 10, 11, 12, 13, 14, 15, 16]);
    }

    #[test]
    fn value_text_uses_the_games_three_formats() {
        let p = block(|b| b[0xf] = 2);
        let n = numbers(&p, &rules(), &live());
        assert_eq!(value_text(&n, bit::LEVEL), "2", "%d");
        assert_eq!(value_text(&n, bit::HP), "80/80", "%d/%d");
        assert_eq!(value_text(&n, bit::AP), "0/4", "%d/%d");
        assert_eq!(value_text(&n, bit::FIRE), "0%", "%d%%");
    }

    /// The real tables (skipped without `KH2_ISO`): the leader's numbers on a new game with the app baseline
    /// are the `lvup` row of level 1 plus the record plus the Kingdom Key's `item_stat` row, and agree with the
    /// Abilities resource's own readings (`equipment_stats`, `ap_used`, and `ap_max` plus the record's AP byte).
    #[test]
    fn the_leaders_numbers_follow_lvup_the_record_and_the_abilities_resource() {
        use crate::abilities::{AbilityRules, Abilities};
        use kh2_tables::tables::{Cmd, Magc, Plrp};
        let iso = std::env::var("KH2_ISO").ok().or_else(|| {
            let f = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../local/iso.txt");
            std::fs::read_to_string(f).ok().map(|s| s.trim().to_string())
        });
        let Some(iso) = iso else {
            eprintln!("skipped: no disc (set KH2_ISO)");
            return;
        };
        let Ok(mut a) = kh2_archive::Archive::open(&iso) else { return };
        let items = kh2_tables::load::<Item>(&mut a, &t::ITEM).expect("item").rows;
        let stats = kh2_tables::load::<ItemStat>(&mut a, &t::ITEM_STAT).expect("item_stat").rows;
        let cmds = kh2_tables::load::<Cmd>(&mut a, &t::CMD).expect("cmd").rows;
        let magc = kh2_tables::load::<Magc>(&mut a, &t::MAGC).expect("magc").rows;
        let plrp = kh2_tables::load::<Plrp>(&mut a, &t::PLRP).expect("plrp").rows;
        let blob = kh2_tables::subfile(&mut a, &t::LVUP).expect("lvup");
        let lvup = kh2_tables::rt::decode_lvup::<Lvup>(&blob).and_then(|c| c.into_iter().next()).expect("Sora's lvup table");
        let index = Arc::new(ItemIndex::new(&items, &cmds, &magc));
        let mut p = Progress::new_game(index, &plrp);
        p.apply_baseline(true);
        let rules = StatRules::from_tables(&items, &stats, &lvup);
        let live = Live { hp: 20, max_hp: 20, mp: 100, max_mp: 100, forms: [1, 2, 3, 4].map(|f| p.owns_form(f)), summons: false, summons_held: [false; 4] };
        let n = numbers(&p, &rules, &live);
        let (rec, row, kk) = (p.sora(), &lvup[0], rules.gear[&crate::data::KINGDOM_KEY]);
        println!("leader: {n:?}\n  record {rec:?}\n  lvup row 0 {:?}\n  Kingdom Key {kk:?}", rules.level_row(1));
        assert_eq!(rules.level_row(1), LevelRow::from(row), "level 1 is row 0 of the table");
        assert_eq!(n.level, 1);
        assert_eq!(n.strength, i32::from(row.strength) + i32::from(rec.strength) + kk.strength);
        assert_eq!(n.magic, i32::from(row.magic) + i32::from(rec.magic) + kk.magic);
        assert_eq!(n.defense, i32::from(row.defense) + i32::from(rec.defense) + kk.defense);
        // Against the Abilities resource (the equipment's stats, the used AP, the AP total).
        let ab = Abilities::with_defaults(Arc::new(AbilityRules::load(&mut a).expect("rules")), Some(&p));
        let (es, em, ed) = ab.equipment_stats();
        assert_eq!((n.strength, n.magic, n.defense), (i32::from(row.strength) + i32::from(rec.strength) + es, i32::from(row.magic) + i32::from(rec.magic) + em, i32::from(row.defense) + i32::from(rec.defense) + ed));
        assert_eq!(n.ap.0, i32::from(ab.ap_used()), "the used AP is the equipped abilities' cost sum");
        assert_eq!(n.ap.1, i32::from(ab.ap_max()) + i32::from(rec.ap), "the AP total adds the record's own AP byte, which ap_max leaves out");
        // The experience to the next level is the row's Exp minus the total.
        p.add_experience(30);
        let n = numbers(&p, &rules, &live);
        assert_eq!((n.exp, n.next), (30, (row.exp - 30).max(0)));
    }

    #[test]
    fn tab_mask_lists_the_character_forms_summons_then_members() {
        // FUN_002683c8: bit 0 the leader, 1..4 forms, 5 summons, 6.. the other members.
        assert_eq!(tab_mask(1, [false; 4], false), 0b1);
        assert_eq!(tabs(tab_mask(1, [false; 4], false)), vec![Tab::Sora]);
        assert_eq!(tab_mask(3, [true, false, false, true], true), 0b1100_0000 | 0b10_0000 | 0b1_0010 | 1);
        assert_eq!(
            tabs(tab_mask(3, [true, false, false, true], true)),
            vec![Tab::Sora, Tab::Form(1), Tab::Form(4), Tab::Summons, Tab::Member(1), Tab::Member(2)]
        );
        assert_eq!(tab_mask(0, [false; 4], false), 0);
        assert_eq!(Tab::Member(2).index(), 7);
        assert!(Tab::Sora.ported() && Tab::Form(1).ported() && Tab::Summons.ported() && !Tab::Member(1).ported());
        assert_eq!((Tab::Sora.table(), Tab::Form(2).table(), Tab::Summons.table()), (0, 1, 2));
    }

    fn fmlv() -> FormLevels {
        // Form 1 (Valor): levels 1..=4 need 10, 20, 40, 80; the summons (form 0) 5, 15.
        let row = |form: u8, level: u8, exp: i32| kh2_tables::player::FormLevel { form, level, display_level: level, ability: 0, exp };
        FormLevels(vec![row(1, 1, 10), row(1, 2, 20), row(1, 3, 40), row(1, 4, 80), row(0, 1, 5), row(0, 2, 15)])
    }

    #[test]
    fn a_forms_experience_is_the_record_plus_the_rows_below_its_level() {
        // `FUN_001d3c98`: record exp + the rows of levels 1..level-1; `FUN_001d3c38`: the row at the level minus the record.
        let f = fmlv();
        let n = level_numbers(&f, 1, 3, 7, 5);
        assert_eq!((n.level, n.exp, n.next, n.gauge), (3, 7 + 10 + 20, 40 - 7, 5));
        // At level 1 nothing is below.
        assert_eq!(level_numbers(&f, 1, 1, 4, 5).exp, 4);
        // At or above the cap there is no next level.
        assert_eq!(level_numbers(&f, 1, 5, 0, 5).next, 0);
        assert_eq!(level_numbers(&f, 1, 4, 0, 4).next, 0, "the cap is exclusive");
        // A missing row (the table ends) shows 0.
        assert_eq!(level_numbers(&f, 1, 6, 0, 7).next, 0);
    }

    #[test]
    fn the_cap_is_two_plus_the_owned_items_and_seven_instead_of_six() {
        assert_eq!([0, 1, 2, 3, 4].map(level_cap), [2, 3, 4, 5, 7]);
    }

    #[test]
    fn the_forms_and_summons_tabs_have_their_own_rows_and_values() {
        let p = block(|b| {
            b[0xf] = 2;
            b[off::FORM_BASE + 3 * 40 + 2] = 3; // form 3 (Master) level 3
            b[off::SUMMON_LEVEL] = 2;
        });
        let l = Live { forms: [true, false, true, false], summons: true, summons_held: [true, false, true, false], ..live() };
        let mut r = rules();
        r.form_levels = fmlv();
        let n = numbers(&p, &r, &l);
        // Forms: bits 0..3 and 10..16; the values of 0..3 are the tab's own, 10..16 the leader's.
        assert_eq!(tab_bits(Tab::Form(1), &n), vec![0, 1, 2, 3, 10, 11, 12, 13, 14, 15, 16]);
        assert_eq!(tab_value(Tab::Form(1), &n, 0), "0", "the record of form 1 is empty in this block");
        assert_eq!(tab_value(Tab::Form(1), &n, 3), "2", "level + 2");
        assert_eq!(tab_value(Tab::Form(3), &n, 3), "5", "level + 2");
        assert_eq!(tab_value(Tab::Form(1), &n, 10), n.strength.to_string());
        assert_eq!(tab_value(Tab::Form(3), &n, 0), "3");
        // Summons: bits 0..3 and 10 + j for each held charm.
        assert_eq!(tab_bits(Tab::Summons, &n), vec![0, 1, 2, 3, 10, 12]);
        assert_eq!(tab_value(Tab::Summons, &n, 0), "2");
        assert_eq!(tab_value(Tab::Summons, &n, 1), "5", "the summon's form-0 row of level 1 below the record exp 0");
        assert_eq!(tab_value(Tab::Summons, &n, 10), "", "the summon rows of the second column print no number");
        // The Sora tab keeps its own rows.
        assert_eq!(tab_value(Tab::Sora, &n, 0), "2");
    }

    #[test]
    fn shoulders_switch_the_tab_with_wrap_and_left_right_keep_the_row() {
        // FUN_00268c78.
        assert_eq!(cycle(0, 3, true), 1);
        assert_eq!(cycle(2, 3, true), 0);
        assert_eq!(cycle(0, 3, false), 2);
        assert_eq!(cycle(1, 3, false), 0);
        assert_eq!(cycle(0, 1, true), 0);
        assert_eq!(cycle(0, 0, false), 0);
        // FUN_00268a08: the cursor row is limited to the other column's row count.
        assert_eq!(toggle_column(5, 7), 5);
        assert_eq!(toggle_column(8, 7), 6);
        assert_eq!(toggle_column(3, 0), 0);
    }
}
