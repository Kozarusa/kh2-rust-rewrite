//! Jiminy's Journal, menu kind 3 (docs/menus.md section 20): the camp's Journal rows leave the camp for this scene.
//!
//! What the original does (`SLUS_210.05`, read in WP-MN9):
//! - The camp's page-0 handler (`FUN_00251848`, jump table 0x380794 entries 5 and 6 at 0x251c38) writes the scene's
//!   mode word (`FUN_0024f4d0`: 0x36cd34) = 1 for entry 5 (Jiminy's Journal) and 2 for entry 6 (Roxas's), queues the
//!   scenes 2 then 0 (`FUN_00296458(2, 0)`: the journal, then the camp again), and closes the camp. The scene stack
//!   (`FUN_00296278`) runs kind 3 and, when its task ends, the camp's kind 0 again: the journal returns to the camp.
//! - Init `FUN_002756f8`: loads `jmemo.2ld` (menu file 3), the world picture list `jm_world.2ld` and the data file
//!   `jiminy.bar` (names at 0x36d3e0; `FUN_002a1a10`, `FUN_002a1bd0`), creates the layout object (`FUN_00276740`:
//!   type 0x7557, layouts from the table 0x381f98 = enter 0, leave 2, hold 1, or 0x381fa0 = 3, 5, 4 when the mode is
//!   2), and picks the start page from the mode and the current world (`start`).
//! - The task 0x276338 (no `.c`; read from the instructions) builds the start page (`FUN_00276740`) and then runs the
//!   page handler of `FUN_00276918` every frame (page 0 `FUN_002a27d8`, page 2 `FUN_002a3da0`, ...).
//! - Page 0 (`FUN_002a2248`) is the world grid: one cell per record of `jiminy.bar` entry 0 whose id is below 0x10
//!   and whose story flag (record +8, `FUN_001802e0`) is set, or every such record when flag 0x4837 is set
//!   (`FUN_002783f8`; such a world shows the locked picture 0x10). Up to 18 cells, six to a row (`FUN_00298e40(list,
//!   6, 3)`), centred on the layout's anchor (`FUN_002a25f0`). The cursor cell plays group 0x96, the others 0x97
//!   (`FUN_002a31e8`). A confirm opens the world's page (2, or 0xc for an id above 0xe); a cancel (-2) or -4 ends the
//!   scene (`FUN_00295ba0`).
//!
//! - Page 2 (`FUN_002a3540` / `FUN_002a3da0`, WP-MN10): the world's category rows with their texts, the two
//!   `jm_top_<world>.2ld` pictures, the story texts and the title; page 1 (`FUN_002a4188` / `FUN_002a44a8`) is
//!   Roxas's one-row page with the same layer objects in other groups.
//!
//! What the port draws: pages 0, 1 and 2 (docs/menus.md 20.7). The category pages (3..0x20) are not ported: a
//! confirm on a page 2 row, or on a page 0 cell of id above 0xe, is refused with a log line.

use bevy::prelude::*;
use kh2_image::layd::{self, Layd};

use crate::assets::{CurrentArea, Disc};
use crate::audio::PlaySe;
use crate::menu::{menu_sound, Menu, Phase};
use crate::menu_gfx::{self, MenuScene, Node};
use crate::menu_text::Label;
use kh2_image::text::{Style, Token};
use crate::menu_pages::Exe;
use crate::progress::Progress;

/// `menu.page` while the journal scene is up: not a camp page id (those are below 0x30), so every camp page module
/// leaves the keys alone and the camp's own input only watches Start, which this module takes first.
pub const PAGE: u16 = 0x0300;

/// The camp entries whose row leaves for the journal, and the mode word each writes (0x251c44: 1 for entry 5, else 2).
pub fn mode_of_entry(entry: usize) -> Option<i32> {
    match entry {
        5 => Some(1),
        6 => Some(2),
        _ => None,
    }
}

/// The executable's tables the scene reads.
mod addr {
    /// The layout triples (enter, leave, hold) of the layout object: Jiminy's, and Roxas's (mode 2) (`FUN_00278788`).
    pub const LAYOUTS: u32 = 0x0038_1f98;
    pub const LAYOUTS_ROXAS: u32 = 0x0038_1fa0;
    /// The header object's group triples (`FUN_002779f8`: the first when its argument is 0).
    pub const HEADER: u32 = 0x0038_1fa8;
    /// The jump table of `FUN_00277c88`: per game world, a stub that returns the world's journal index (or -1).
    pub const WORLD_STUBS: u32 = 0x0038_1f38;
    pub const WORLDS: u32 = 0x13;
    /// Page 2's row texts by category (u16 x 8, `FUN_002a39b8`; also the help line source of `FUN_002a3da0`).
    pub const ROW_TEXT: u32 = 0x0038_2b88;
    /// The sequences of the two `jm_top` pictures (u32: picture 1's, then picture 0's, `FUN_002a4750`).
    pub const PIC_SEQ: u32 = 0x0038_2b68;
    // The pictures' groups follow at 0x382b70 (u32: picture 1 enter, hold, picture 0 enter, hold): read as PIC_SEQ + 8..
    /// Pointers to the alternative `jm_top_` names (page 1's, and world 1's once its second flag is set; `FUN_002a45b0`).
    pub const TOP_NAME_ROXAS: u32 = 0x0036_d45c;
    pub const TOP_NAME_ALT: u32 = 0x0036_d460;
    /// The three page names of the L1 / R1 hints (u16: pages 0, 2 and 0xc, `FUN_00277860` / `FUN_00277928`).
    pub const HINTS: u32 = 0x0038_1fb8;
    /// The plate module's group triples (`FUN_0029cf78(0, 5, .., 0x381fc0, 0x381fc8)`): the plates (the category
    /// name tab, `FUN_0029d1d0`) and the follower object every plate and the list cursor carry (`FUN_0029af00`,
    /// `FUN_002771c0` object 0x7571).
    pub const PLATE_GROUPS: u32 = 0x0038_1fc0;
    pub const FOLLOW_GROUPS: u32 = 0x0038_1fc8;
    /// The category pages' backdrop object 0x755a (`FUN_00278658`): u16 group by page family (+2 for 0xf/0x12/0x15).
    pub const BACKDROP: u32 = 0x0038_1f88;
    /// Pairs (pad mask, icon) ending with -1 (`FUN_00255418`; icon 0xb when the mask is not listed).
    pub const PAD_ICONS: u32 = 0x0038_0ab0;
    /// Page 2's help lines by category (u16 x 8, `FUN_002a4a68`).
    pub const HELP: u32 = 0x0038_2b98;
}

/// The literal messages of `FUN_002a3d08` (the labels beside the two story texts).
mod msg {
    pub const LABEL_A: u32 = 0x3253;
    pub const LABEL_B: u32 = 0x3254;
    /// Page 1's help line (`FUN_002a4a68` when the page is 1).
    pub const HELP_ROXAS: u16 = 0x493f;
    /// Page 3's help line (`FUN_002a54a8`).
    pub const HELP_STORY: u16 = 0x48d7;
}

/// The pad masks of L1 and R1 the hints show (`FUN_00277740`: `FUN_00255418(0x400)` before the left name,
/// `FUN_00255418(0x800)` after the right one).
const PAD_L1: u16 = 0x400;
const PAD_R1: u16 = 0x800;
/// The help line object 0x755d (`FUN_00278468`: group 0x7b, a child of the plate 0x7558).
const HELP_GROUP: usize = 0x7b;
/// The list cursor (`FUN_00277158` -> `FUN_0029a9c0(0, seq 5, .., 0x21, 0x21)`).
const CURSOR_GROUP: usize = 0x21;
/// The L1 / R1 hint objects (`FUN_002774e8`: `DAT_0036d0f0` groups 0x2f then 0x30, `+0x1a8` groups 0x32 then 0x33,
/// both centred).
const HINT_L: [usize; 2] = [0x2f, 0x30];
const HINT_R: [usize; 2] = [0x32, 0x33];
/// Page 3 (the story, `FUN_002a4c38`): the two layer objects, the row's and the marks' group.
const STORY_LAYERS: [usize; 2] = [0x5e, 0x62];
const STORY_ROW: usize = 2;
/// Lines the story page shows (`FUN_00299010(list, 10)`, the draw `0x2a54c8` stops after ten line heights) and
/// the line mark objects (`FUN_002a4c38` creates ten at +0xa4a0).
pub const STORY_LINES: usize = 10;
/// The story text's font (`0x2a54c8`: `FUN_001a7ed8(ctx, 3, ..)`).
const STORY_FONT: usize = 3;

/// The layer objects of pages 2 (`FUN_002a3540`) and 1 (`FUN_002a4188`): six objects of sequence 5; 0 and 1 and the
/// two holders 2 and 4 hang from the header object (`DAT_0036d0e8`), 3 from 2 and 5 from 4.
const LAYERS_P2: [usize; 6] = [0x89, 0x93, 0x91, 3, 0x92, 3];
const LAYERS_P1: [usize; 6] = [0x8d, 0x90, 0x94, 3, 0x95, 3];
/// Every list row and its two children play group 3 (both group sets of `FUN_00296b48` are four 3s).
const ROW_GROUP: usize = 3;
/// Page 2's rows (`FUN_002a3540` creates 8 list objects).
pub const ROWS: usize = 8;
/// The title object (`DAT_0036d0f0 + 0x350`, `FUN_00277258`: enter 0x35, hold 0x36, centred text).
const TITLE: [usize; 2] = [0x35, 0x36];
/// The text size when a group gives none (`FUN_00138fc8`).
const TEXT_SIZE: f32 = 15.0;

/// Literal groups of `jmemo.2ld` sequence 5 the code names.
mod group {
    /// A cell: selected and idle (`FUN_002a31e8`), and the group every list object starts with (`DAT_0036d050` = 1).
    pub const CELL_SELECTED: usize = 0x96;
    pub const CELL_IDLE: usize = 0x97;
    pub const START: usize = 1;
    /// The plate object 0x7558 (`FUN_002772d0`: enter 0x2c, hold 0x2d).
    pub const PLATE: [usize; 2] = [0x2c, 0x2d];
    /// The world picture (sequence 2): enter 1, hold 2 (`FUN_002a2bc0`).
    pub const ICON: [usize; 2] = [1, 2];
}
/// Sequences of `jmemo.2ld`: the list objects use 5 (`DAT_0036d04c`), the world picture 2 (`FUN_00239020(.., 2)`).
const SEQ_LIST: usize = 5;
const SEQ_ICON: usize = 2;
/// The picture of a world the player has not opened (`FUN_002a2bc0`: id 0x10).
const LOCKED_ICON: u8 = 0x10;
/// The flag that shows every world (`FUN_002783f8`).
const ALL_WORLDS_FLAG: u16 = 0x4837;
/// Cells of the grid (`FUN_002a2248` creates 0x12 list objects) and columns (`FUN_00298e40(list, 6, 3)`).
pub const CELLS: usize = 0x12;
pub const COLUMNS: usize = 6;

/// The start page and the journal world byte (`*DAT_0036d0dc`) of `FUN_002756f8` for a mode word and the current
/// world's journal index (`FUN_00277c88`, -1 for a world without one).
pub fn start(mode: i32, journal_world: i8) -> (u16, i8) {
    let mut m = mode.max(0);
    let mut world = -1;
    if m != 2 {
        world = journal_world;
        if world < 0 {
            m = 0;
        }
    }
    match m {
        1 => (2, world),
        0 => (0, -1),
        2 => (1, 0x13),
        3 => (10, world),
        _ => (0, world),
    }
}

/// One record of `jiminy.bar` entry 0 ("worl": a 0x10-byte header with the count at +8, then 0x10-byte records).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct World {
    /// The journal world id (+0): the picture of `jm_world.2ld` and the byte the pages pass on.
    pub id: i8,
    /// The two-letter world code (+1).
    pub code: String,
    /// The story flag that opens the world's cell (+8, `FUN_002a1db8`).
    pub flag: u16,
    /// A second flag (+0xe, `FUN_002a1de8`, read by page 2 for id 1).
    pub flag2: u16,
    /// Message ids +4 and +6, and +0xa and +0xc used instead when `flag2` is set (`FUN_002a3070`: the first of
    /// each pair is page 2's title).
    pub names: [u16; 4],
}

impl World {
    /// Page 2's title (`FUN_002a3070(world, 0)`): +4, or +0xa when the second flag is set.
    pub fn title(&self, test: &dyn Fn(u16) -> bool) -> u16 {
        if test(self.flag2) { self.names[2] } else { self.names[0] }
    }
}

/// Parses `jiminy.bar` entry 0.
pub fn parse_worlds(d: &[u8]) -> Option<Vec<World>> {
    if d.get(..4)? != b"JMWO" {
        return None;
    }
    let n = u32::from_le_bytes(d.get(8..12)?.try_into().ok()?) as usize;
    (0..n)
        .map(|i| {
            let r = d.get(0x10 + 0x10 * i..0x20 + 0x10 * i)?;
            let u16_at = |o: usize| u16::from_le_bytes([r[o], r[o + 1]]);
            let code = r[1..4].iter().take_while(|&&b| b != 0).map(|&b| b as char).collect();
            Some(World { id: r[0] as i8, code, flag: u16_at(8), flag2: u16_at(0xe), names: [u16_at(4), u16_at(6), u16_at(0xa), u16_at(0xc)] })
        })
        .collect()
}

/// The records of a `jiminy.bar` entry (a 0x10-byte header with the count at +8) of `stride` bytes each.
pub fn records(d: &[u8], stride: usize) -> Vec<&[u8]> {
    let n = d.get(8..12).map_or(0, |b| u32::from_le_bytes(b.try_into().unwrap()) as usize);
    (0..n).map_while(|i| d.get(0x10 + stride * i..0x10 + stride * (i + 1))).collect()
}

/// The message used when a story record does not apply (`FUN_002a1e98` starts both outputs with it).
pub const NO_TEXT: u16 = 0x8adc;

/// The two texts of pages 1 and 2 (`FUN_002a1e98(world)`, entry 1 "stor", 0xc-byte records): every record of the
/// world whose flag (+0xa, `FUN_002a1e78`) is set overwrites them with its +4 and +6; the last one wins.
pub fn story_texts(stor: &[u8], world: i8, test: &dyn Fn(u16) -> bool) -> [u16; 2] {
    let mut out = [NO_TEXT; 2];
    for r in records(stor, 0xc) {
        let u16_at = |o: usize| u16::from_le_bytes([r[o], r[o + 1]]);
        if r[0] as i8 == world && test(u16_at(10)) {
            out = [u16_at(4), u16_at(6)];
        }
    }
    out
}

/// Whether an entry with `stride`-byte records starting with the u16 world has a record for `world`.
pub fn has_world(d: &[u8], stride: usize, world: i8) -> bool {
    records(d, stride).iter().any(|r| u16::from_le_bytes([r[0], r[1]]) == world as i16 as u16)
}

/// Page 2's rows (`FUN_002a4960` / `FUN_002a39b8`): the categories 0..8 in order, without 3 for journal world 8
/// (`FUN_002b1650`), 5 unless the quest list has an entry (`FUN_002b6f00`, entry 8) and 6 unless the minigame list
/// has one (`FUN_002b5bb0`, entry 7). `quests` / `games` are those tests' results.
pub fn categories(world: i8, quests: bool, games: bool) -> Vec<usize> {
    (0..8)
        .filter(|&c| match c {
            3 => world != 8,
            5 => quests,
            6 => games,
            _ => true,
        })
        .collect()
}

/// The page a category row opens (`FUN_002a3da0`).
pub fn category_page(cat: usize) -> u16 {
    match cat {
        0 => 6,
        1 => 3,
        2 => 4,
        3 => 7,
        4 => 8,
        5 => 9,
        6 => 10,
        _ => 0xb,
    }
}

/// The pages of the three hint names (`FUN_00277928`: the place of a name in the table 0x381fb8 gives 0, 2 or 0xc).
const HINT_PAGES: [u16; 3] = [0, 2, 0xc];

/// The names of the L1 and R1 hints on `page` (`FUN_00277860`): page 0 is name 0, pages 1 and 2 name 1, page 0xc
/// name 2; the left hint is the previous name, the right one the next (wrapping). When the journal world byte is
/// above 0xe (unsigned: `lbu`, so -1 too), a hint that would name 1 (page 2) names 0 (or 2 on page 0xc) instead.
pub fn hints(page: u16, world: i8, table: &[u16; 3]) -> [u16; 2] {
    let u: usize = match page {
        1 | 2 => 1,
        0xc => 2,
        _ => 0,
    };
    let l = if u == 0 { 2 } else { u - 1 };
    let r = if u < 2 { u + 1 } else { 0 };
    let mut out = [table[l], table[r]];
    if world as u8 > 0xe {
        let other = if u == 2 { 0 } else { 2 };
        for (k, i) in [l, r].into_iter().enumerate() {
            if i == 1 {
                out[k] = table[other];
            }
        }
    }
    out
}

/// The page a hint name leads to (`FUN_00277928`: the first place of the name in the table; -1 when absent).
pub fn hint_page(name: u16, table: &[u16; 3]) -> Option<u16> {
    table.iter().position(|&t| t == name).map(|i| HINT_PAGES[i])
}

/// The icon of a pad mask (`FUN_00255418`: the pairs at 0x380ab0 up to a -1 mask; 0xb when the mask is absent).
pub fn pad_icon(pairs: &[(u16, u16)], mask: u16) -> u8 {
    pairs.iter().find(|p| p.0 == mask).map_or(0xb, |p| p.1 as u8)
}

/// Page 3's entries (`FUN_002a5060`): every record of `jiminy.bar` entry 1 of the world (+0) whose story flag
/// (+0xa, `FUN_002a1e78`) is set, in file order: (the text, +8; the record's index).
pub fn story_entries(stor: &[u8], world: i8, test: &dyn Fn(u16) -> bool) -> Vec<(u16, usize)> {
    records(stor, 0xc)
        .iter()
        .enumerate()
        .filter(|(_, r)| r[0] as i8 == world && test(u16::from_le_bytes([r[10], r[11]])))
        .map(|(i, r)| (u16::from_le_bytes([r[8], r[9]]), i))
        .collect()
}

/// The list count of page 3 (`FUN_002a4c38`): the lines of every entry but the last, plus one, plus the last
/// entry's lines beyond ten. A cursor value is the first line shown.
pub fn story_count(lines: &[usize]) -> usize {
    let Some((&last, rest)) = lines.split_last() else { return 1 };
    rest.iter().sum::<usize>() + 1 + last.saturating_sub(STORY_LINES)
}

/// The line height and the extra gap of page 3 (`FUN_002a5060`, `0x2a54c8`): the line height is the layer group's
/// +0x1c, at least the text size plus the font's line gap; the gap the draw adds (`FUN_001a8cc0`) is what is
/// left of the line height over the size and the font's line gap (never negative).
pub fn story_metrics(pitch: i32, size: i32, font_gap: i32) -> (i32, i32) {
    let gap = (pitch - size - font_gap).max(0);
    (pitch.max(size + font_gap), gap)
}

/// Page 3's state machine (`FUN_002a5218`): Up/Down move the first line shown [inferred: one line, no wrap]; L1 /
/// R1 a page of ten (the list's page step +0x1c = 10, `FUN_00299010`; with +0x27 set by `FUN_00298fe0(list, 1)`
/// the list pages itself, `FUN_00298370`, instead of returning -9 / -8) [inferred: clamped at the ends]; -2 goes
/// back to page 2, -4 ends the scene.
pub fn page3(key: Key, top: usize, count: usize) -> Act {
    let last = count.saturating_sub(1);
    let to = match key {
        Key::Start => return Act::Close,
        Key::Cancel => return Act::Back,
        Key::Move(_, dy) if dy != 0 => {
            let to = top as i64 + i64::from(dy.signum());
            if to < 0 || to > last as i64 {
                return Act::None;
            }
            to as usize
        }
        Key::L1 => top.saturating_sub(STORY_LINES),
        Key::R1 => (top + STORY_LINES).min(last),
        _ => return Act::None,
    };
    if to == top { Act::None } else { Act::Cursor(to) }
}

/// The cells of page 0 in order: (record index, picture id). `FUN_002a2248` / `FUN_002a2bc0`: a record with an id
/// below 0x10 whose flag is set shows its own picture; with the all-worlds flag set, one whose flag is clear shows
/// the locked picture.
pub fn cells(worlds: &[World], test: &dyn Fn(u16) -> bool) -> Vec<(usize, u8)> {
    let all = test(ALL_WORLDS_FLAG);
    worlds
        .iter()
        .enumerate()
        .filter(|(_, w)| w.id >= 0 && w.id < 0x10)
        .filter_map(|(i, w)| {
            let open = test(w.flag);
            (open || all).then_some((i, if open { w.id as u8 } else { LOCKED_ICON }))
        })
        .take(CELLS)
        .collect()
}

/// The first cursor of page 0 (`FUN_002a2f38`): the index of the world `want` (0xf when the byte is negative) among
/// the shown records with an id below 0x14; `None` when it is not shown [inferred: the list then starts at 0].
///
/// The original counts the shown records with an id below 0x14, but records 0x10..0x13 get no cell, so the index is
/// taken within the cells actually shown (`shown`, from `cells`): the cursor always lands on the cell of that world.
pub fn first_cursor(worlds: &[World], shown: &[(usize, u8)], want: i8) -> Option<usize> {
    let want = if want < 0 { 0xf } else { want };
    shown.iter().position(|&(i, _)| worlds.get(i).is_some_and(|w| w.id == want))
}

/// The cell offsets from the layout's anchor (`FUN_002a25f0`): rows of six, each row centred, the rows centred; the
/// pitches are the idle cell group's +0x20 (x) and +0x1c (y). C integer division (towards zero).
pub fn grid(n: usize, pitch_x: i32, pitch_y: i32) -> Vec<[i32; 2]> {
    let n = n as i32;
    let cols = n.min(COLUMNS as i32);
    let rows = n / 6 + i32::from(n % 6 != 0);
    let mut out = Vec::new();
    let mut x0 = -((pitch_x * (cols - 1)) / 2);
    for r in 0..rows {
        if n < 6 * (r + 1) {
            x0 = -((pitch_x * (n % 6 - 1)) / 2);
        }
        let y = r * pitch_y - (pitch_y * (rows - 1)) / 2;
        for c in 0..cols {
            if r * 6 + c < n {
                out.push([x0 + c * pitch_x, y]);
            }
        }
    }
    out
}

/// One step of the grid cursor [inferred: the list widget with six columns; left and right step one cell, up and
/// down one row, no wrap].
pub fn step(cursor: usize, n: usize, dx: i32, dy: i32) -> usize {
    let to = cursor as i64 + dx as i64 + dy as i64 * COLUMNS as i64;
    if n == 0 || to < 0 || to >= n as i64 { cursor } else { to as usize }
}

/// What a key does on page 0 (`FUN_002a27d8`): the list's result.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Key {
    Confirm,
    Cancel,
    /// The -4 result (Start) [inferred: the camp's Start key].
    Start,
    Move(i32, i32),
    /// The list's -9 (L1) and -8 (R1) results (`FUN_00297148`: bits 0x8000 / 0x4000 of the pad word, enabled by the
    /// list flags 0x2000 / 0x1000 that `FUN_002a2248` / `FUN_002a3540` set; -8 takes the right hint's page,
    /// `FUN_002a27d8` / `FUN_002a3da0`) [inferred: -9 is L1 and -8 R1, the buttons the hints show on their side].
    L1,
    R1,
}

/// The outcome of a key on page 0.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Act {
    None,
    Cursor(usize),
    /// A confirm on a cell: the page the original opens (2, or 0xc for an id above 0xe) and the world id.
    Open { page: u16, world: i8 },
    /// -2 / -4: the scene ends; the scene stack runs the camp next.
    Close,
    /// Page 2's cancel (-2): page 0 (`FUN_002a3da0`: `FUN_00295a00(0)`).
    Back,
    /// A confirm on a page 2 row: the category and the page it opens.
    Category { cat: usize, page: u16 },
    /// An L1 / R1 page switch (`FUN_00277928`): the page the hint names (`None`: the name is not in the table).
    Switch(Option<u16>),
}

/// The L1 / R1 switch of pages 0 and 2.
fn switch(key: Key, page: u16, world: i8, table: &[u16; 3]) -> Act {
    let h = hints(page, world, table);
    Act::Switch(hint_page(if key == Key::R1 { h[1] } else { h[0] }, table))
}

/// Page 2's state machine (`FUN_002a3da0`): Up/Down move the row cursor (a new press past the ends wraps, the list
/// widget `FUN_002977a8`), a confirm opens the row's category page, -2 goes back to page 0, -4 ends the scene.
pub fn page2(key: Key, row: usize, cats: &[usize]) -> Act {
    match key {
        Key::Start => Act::Close,
        Key::Cancel => Act::Back,
        Key::Move(_, 0) => Act::None,
        Key::Move(_, dy) => {
            let n = cats.len() as i32;
            if n < 2 {
                return Act::None;
            }
            Act::Cursor((row as i32 + dy.signum()).rem_euclid(n) as usize)
        }
        Key::Confirm => cats.get(row).map_or(Act::None, |&cat| Act::Category { cat, page: category_page(cat) }),
        // L1 / R1: `switch` (it needs the hint table).
        Key::L1 | Key::R1 => Act::None,
    }
}

/// Page 1 (Roxas's Journal, `FUN_002a44a8`): one row; -2 and -4 end the scene.
pub fn page1(key: Key) -> Act {
    match key {
        Key::Cancel | Key::Start => Act::Close,
        _ => Act::None,
    }
}

/// Page 0's state machine.
pub fn page0(key: Key, cursor: usize, shown: &[(usize, u8)], worlds: &[World]) -> Act {
    match key {
        Key::Cancel | Key::Start => Act::Close,
        Key::Move(dx, dy) => {
            let c = step(cursor, shown.len(), dx, dy);
            if c == cursor { Act::None } else { Act::Cursor(c) }
        }
        Key::Confirm => match shown.get(cursor).and_then(|&(i, _)| worlds.get(i)) {
            Some(w) => Act::Open { page: if w.id > 0xe { 0xc } else { 2 }, world: w.id },
            None => Act::None,
        },
        Key::L1 | Key::R1 => Act::None,
    }
}

/// `FUN_00277c88`'s stubs: `jr ra` with `addiu v0, zero, n` in the delay slot; the journal index of each game world.
pub fn read_world_index(exe: &Exe) -> Option<Vec<i8>> {
    let u32_at = |va: u32| -> Option<u32> { Some(u32::from(exe.u16(va)?) | u32::from(exe.u16(va + 2)?) << 16) };
    (0..addr::WORLDS)
        .map(|w| {
            let stub = u32_at(addr::WORLD_STUBS + 4 * w)?;
            let (jr, slot) = (u32_at(stub)?, u32_at(stub + 4)?);
            if jr != 0x03e0_0008 {
                return None;
            }
            match slot {
                0x0000_102d => Some(0),
                s if s >> 16 == 0x2402 => Some((s & 0xffff) as i16 as i8),
                _ => None,
            }
        })
        .collect()
}

/// A u16 triple of the executable.
fn triple(exe: &Exe, va: u32) -> Option<[usize; 3]> {
    Some([usize::from(exe.u16(va)?), usize::from(exe.u16(va + 2)?), usize::from(exe.u16(va + 4)?)])
}

fn u32_at(exe: &Exe, va: u32) -> Option<u32> {
    Some(u32::from(exe.u16(va)?) | u32::from(exe.u16(va + 2)?) << 16)
}

/// The NUL-terminated name a pointer of the executable points at.
fn name_at(exe: &Exe, ptr: u32) -> Option<String> {
    let a = u32_at(exe, ptr)?;
    let s: String = (0..16).map_while(|k| exe.u8(a + k).filter(|&b| b != 0)).map(|b| b as char).collect();
    (!s.is_empty()).then_some(s)
}

/// The executable's page 1 / 2 tables.
#[derive(Clone, Default, Debug, PartialEq, Eq)]
pub struct PageTables {
    pub row_text: [u16; 8],
    /// Picture 0's and picture 1's sequence (`FUN_002a4750`: 0x382b6c for 0, 0x382b68 for 1).
    pub pic_seq: [usize; 2],
    /// Picture 0's and 1's (enter, hold) groups.
    pub pic_group: [[usize; 2]; 2],
    pub top_roxas: String,
    pub top_alt: String,
    /// The hint names (0x381fb8), page 2's help lines (0x382b98).
    pub hints: [u16; 3],
    pub help: [u16; 8],
    /// The plate and follower group triples (0x381fc0, 0x381fc8), the backdrop groups (0x381f88, four).
    pub plate: [usize; 3],
    pub follow: [usize; 3],
    pub backdrop: [usize; 4],
    /// The icons of L1 and R1 (`FUN_00255418`).
    pub icon_l1: u8,
    pub icon_r1: u8,
}

pub fn read_page_tables(exe: &Exe) -> Option<PageTables> {
    let u16s = |va: u32, out: &mut [u16]| -> Option<()> {
        for (k, t) in out.iter_mut().enumerate() {
            *t = exe.u16(va + 2 * k as u32)?;
        }
        Some(())
    };
    let (mut row_text, mut help, mut hints, mut back) = ([0; 8], [0; 8], [0; 3], [0u16; 4]);
    u16s(addr::ROW_TEXT, &mut row_text)?;
    u16s(addr::HELP, &mut help)?;
    u16s(addr::HINTS, &mut hints)?;
    u16s(addr::BACKDROP, &mut back)?;
    let mut pairs = Vec::new();
    for k in 0..64u32 {
        let m = exe.u16(addr::PAD_ICONS + 4 * k)?;
        if m == 0xffff {
            break;
        }
        pairs.push((m, exe.u16(addr::PAD_ICONS + 4 * k + 2)?));
    }
    let w = |k: u32| u32_at(exe, addr::PIC_SEQ + 4 * k).map(|v| v as usize);
    Some(PageTables {
        row_text,
        pic_seq: [w(1)?, w(0)?],
        pic_group: [[w(4)?, w(5)?], [w(2)?, w(3)?]],
        top_roxas: name_at(exe, addr::TOP_NAME_ROXAS)?,
        top_alt: name_at(exe, addr::TOP_NAME_ALT)?,
        hints,
        help,
        plate: triple(exe, addr::PLATE_GROUPS)?,
        follow: triple(exe, addr::FOLLOW_GROUPS)?,
        backdrop: back.map(usize::from),
        icon_l1: pad_icon(&pairs, PAD_L1),
        icon_r1: pad_icon(&pairs, PAD_R1),
    })
}

#[derive(Default)]
struct Nodes {
    layer: usize,
    header: usize,
    plate: usize,
    cells: Vec<usize>,
    marks: Vec<[usize; 2]>,
    icons: Vec<usize>,
    /// Pages 1 and 2: the six layer objects, the two `jm_top` pictures, the title, the rows and their children.
    lay: Vec<usize>,
    pics: Vec<usize>,
    title: usize,
    rows: Vec<usize>,
    row_kids: Vec<[usize; 2]>,
    /// The help line 0x755d, the list cursor and its follower 0x7571, the two hints.
    help: usize,
    cursor: usize,
    follow: usize,
    hints: [usize; 2],
    /// The category pages: the name tab (plate 0 of `FUN_0029d1d0`) and its follower, the backdrop 0x755a.
    tab: usize,
    tab_follow: usize,
    backdrop: usize,
    /// Page 3: the two layers, the text row and the ten line marks.
    story_lay: Vec<usize>,
    story_row: usize,
    story_marks: Vec<usize>,
}

impl Nodes {
    fn page3(&self) -> Vec<usize> {
        let mut v = vec![self.tab, self.tab_follow, self.backdrop, self.story_row];
        v.extend(&self.story_lay);
        v.extend(&self.story_marks);
        v
    }
    fn chrome(&self) -> Vec<usize> {
        vec![self.help, self.cursor, self.follow, self.hints[0], self.hints[1]]
    }
    fn page0(&self) -> Vec<usize> {
        let mut v = self.cells.clone();
        v.extend(self.marks.iter().flatten());
        v.extend(&self.icons);
        v
    }
    fn page12(&self) -> Vec<usize> {
        let mut v = self.lay.clone();
        v.extend(&self.pics);
        v.push(self.title);
        v.extend(&self.rows);
        v.extend(self.row_kids.iter().flatten());
        v
    }
    fn all(&self) -> Vec<usize> {
        let mut v = vec![self.layer, self.header, self.plate];
        v.extend(self.page0());
        v.extend(self.page12());
        v.extend(self.page3());
        v.extend(self.chrome());
        v
    }
}

#[derive(Resource, Default)]
pub struct Journal {
    tried: bool,
    loaded: bool,
    seq_base: usize,
    tex: usize,
    world_tex: Vec<Option<usize>>,
    layd: Layd,
    worlds: Vec<World>,
    /// `jiminy.bar` entries 1 (story texts), 7 (minigames) and 8 (quests).
    stor: Vec<u8>,
    /// `msg/us/jm.bar`, the scene's message file in message slot 0 (task 0x276338: `FUN_002765a0` ->
    /// `FUN_00190e70(0x65, 0)`; `FUN_00190dd0` names file 0x65 "jm"), searched before the system messages
    /// (`FUN_00190c40` walks the slots in order).
    msgs: Option<kh2_image::text::Messages>,
    mini: Vec<u8>,
    ques: Vec<u8>,
    world_index: Vec<i8>,
    layouts: [[usize; 3]; 2],
    header: [usize; 3],
    tables: PageTables,
    /// `jm_top_<name>.2ld` pictures already turned into textures, by name.
    top_tex: std::collections::HashMap<String, Vec<usize>>,
    nodes: Nodes,
    /// The mode a camp row asked for (taken by `tick`).
    request: Option<i32>,
    /// A page switch asked for by `input` (page, world byte), built by `tick`.
    goto: Option<(u16, i8)>,
    /// The scene is up: page, cursor, the shown cells, the scene nodes it hid and their visibility.
    open: Option<Open>,
}

struct Open {
    page: u16,
    /// The journal world byte (`*DAT_0036d0dc`).
    world: i8,
    cursor: usize,
    shown: Vec<(usize, u8)>,
    /// Page 2: the row cursor and the categories of the rows.
    row: usize,
    cats: Vec<usize>,
    saved: Vec<(usize, bool)>,
    roxas: bool,
    /// Page 2's row cursor kept for the way back (`FUN_00277dc8(page, row)` at the confirm, `FUN_00277de8` at the
    /// build).
    back_row: usize,
    /// Page 3: the first line shown, the list count, the text lines of all entries in order.
    top: usize,
    count: usize,
    lines: Vec<Vec<Token>>,
    /// Page 3's extra line gap (`FUN_001a8cc0`), its text size.
    gap: u8,
    size: f32,
}

pub struct JournalPlugin;

impl Plugin for JournalPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Journal>()
            .add_systems(PreUpdate, input.before(crate::menu::input).after(bevy::input::InputSystems).after(crate::harness::Keys))
            .add_systems(Update, tick.before(crate::schedule::Frame::Popups));
    }
}

fn load(j: &mut Journal, disc: &Disc, scene: &mut MenuScene, images: &mut Assets<Image>) {
    j.tried = true;
    let Some(two) = disc.read("menu/us/jmemo.2ld").and_then(|b| layd::parse_2ld(&b)) else {
        warn!("journal: menu/us/jmemo.2ld is not readable; the Journal rows do nothing");
        return;
    };
    let Some(bar) = disc.read("menu/us/jiminy.bar") else { return };
    let Ok(entries) = kh2_bar::parse(&bar) else { return };
    let Some(worlds) = entries.first().and_then(|e| parse_worlds(e.data)) else {
        warn!("journal: menu/us/jiminy.bar entry 0 is not readable; the Journal rows do nothing");
        return;
    };
    let entry = |k: usize| entries.get(k).map(|e| e.data.to_vec()).unwrap_or_default();
    (j.stor, j.mini, j.ques) = (entry(1), entry(7), entry(8));
    j.msgs = disc.read("msg/us/jm.bar").and_then(|b| kh2_image::text::Messages::from_bar(&b));
    let Some(exe) = disc.read_root("SLUS_210.05").and_then(Exe::load) else { return };
    let (Some(index), Some(l0), Some(l1), Some(h), Some(tables)) =
        (read_world_index(&exe), triple(&exe, addr::LAYOUTS), triple(&exe, addr::LAYOUTS_ROXAS), triple(&exe, addr::HEADER), read_page_tables(&exe))
    else {
        warn!("journal: the executable's journal tables are not readable");
        return;
    };
    let Some(image) = two.images.first() else { return };
    if two.layd.seqs.len() <= SEQ_LIST {
        return;
    }
    j.tex = scene.texs.len();
    scene.texs.push(menu_gfx::Tex::new(image, images));
    // `jm_world.2ld`: a BAR of IMGD pictures, one per journal world id (`FUN_0023c478`).
    j.world_tex = bar_pictures(disc, "menu/us/jm_world.2ld", scene, images);
    j.seq_base = scene.seqs.len();
    let Layd { layers, layouts, seqs } = two.layd;
    scene.seqs.extend(seqs);
    j.layd = Layd { layers, layouts, seqs: Vec::new() };
    j.worlds = worlds;
    j.world_index = index;
    j.layouts = [l0, l1];
    j.header = h;
    j.tables = tables;
    // The nodes, hidden; a parent always has a lower index.
    let (list, icon) = (j.seq_base + SEQ_LIST, j.seq_base + SEQ_ICON);
    let add = |scene: &mut MenuScene, seq: usize, tex: usize, parent: Option<usize>| {
        let mut n = Node::new(Some(seq));
        n.tex = tex;
        n.visible = false;
        n.parent = parent;
        scene.add(n)
    };
    let tex = j.tex;
    let layer = add(scene, list, tex, None);
    let header = add(scene, list, tex, None);
    let plate = add(scene, list, tex, None);
    let mut nodes = Nodes { layer, header, plate, ..Nodes::default() };
    for _ in 0..CELLS {
        let c = add(scene, list, tex, None);
        nodes.cells.push(c);
        nodes.marks.push([add(scene, list, tex, Some(c)), add(scene, list, tex, Some(c))]);
        nodes.icons.push(add(scene, icon, tex, Some(c)));
    }
    // Pages 1 and 2 (`FUN_002a3540` / `FUN_002a4188`).
    for k in 0..6 {
        let parent = match k {
            3 => nodes.lay[2],
            5 => nodes.lay[4],
            _ => header,
        };
        nodes.lay.push(add(scene, list, tex, Some(parent)));
    }
    for k in 0..2 {
        let seq = j.seq_base + j.tables.pic_seq[k];
        nodes.pics.push(add(scene, seq, tex, Some(nodes.lay[k])));
    }
    nodes.title = add(scene, list, tex, None);
    for _ in 0..ROWS {
        let r = add(scene, list, tex, Some(nodes.lay[0]));
        nodes.rows.push(r);
        nodes.row_kids.push([add(scene, list, tex, Some(r)), add(scene, list, tex, Some(r))]);
    }
    // The category pages (`FUN_0029d1d0`, `FUN_00278658`, page 3 `FUN_002a4c38`).
    nodes.tab = add(scene, list, tex, None);
    nodes.tab_follow = add(scene, list, tex, Some(nodes.tab));
    nodes.backdrop = add(scene, list, tex, Some(header));
    for _ in 0..2 {
        nodes.story_lay.push(add(scene, list, tex, Some(header)));
    }
    nodes.story_row = add(scene, list, tex, Some(nodes.story_lay[0]));
    for _ in 0..STORY_LINES {
        nodes.story_marks.push(add(scene, list, tex, Some(nodes.story_lay[0])));
    }
    // The help line (a child of the plate, `FUN_00278468`), the hints (roots), the cursor and its follower (moved
    // under the cursor row or cell when shown: those all have lower indices).
    nodes.help = add(scene, list, tex, Some(plate));
    nodes.hints = [add(scene, list, tex, None), add(scene, list, tex, None)];
    nodes.cursor = add(scene, list, tex, None);
    nodes.follow = add(scene, list, tex, None);
    j.nodes = nodes;
    j.loaded = true;
    info!("journal: loaded jmemo.2ld, {} world pictures, {} world records", j.world_tex.len(), j.worlds.len());
}

/// The IMGD pictures of a BAR file as textures (`FUN_0023c478`).
fn bar_pictures(disc: &Disc, path: &str, scene: &mut MenuScene, images: &mut Assets<Image>) -> Vec<Option<usize>> {
    let pics = disc.read(path).map(|b| kh2_bar::parse(&b).map(|e| e.iter().map(|e| kh2_image::imgd::decode(e.data)).collect::<Vec<_>>()).unwrap_or_default()).unwrap_or_default();
    pics.into_iter()
        .map(|p| {
            p.map(|img| {
                scene.texs.push(menu_gfx::Tex::new(&img, images));
                scene.texs.len() - 1
            })
        })
        .collect()
}

fn input(mut j: ResMut<Journal>, mut menu: ResMut<Menu>, mut keys: ResMut<ButtonInput<KeyCode>>, mut se: MessageWriter<PlaySe>, mut scene: ResMut<MenuScene>) {
    let confirm = keys.just_pressed(KeyCode::KeyJ);
    let cancel = keys.just_pressed(KeyCode::Escape) || keys.just_pressed(KeyCode::Backspace) || keys.just_pressed(KeyCode::KeyK);
    let start = keys.just_pressed(KeyCode::Enter) || keys.just_pressed(KeyCode::NumpadEnter);
    if j.open.is_none() && j.request.is_none() {
        // The camp's page 0: a confirm on a Journal row leaves for this scene.
        if menu.page == 0
            && confirm
            && let Some(mode) = menu.current_entry().and_then(mode_of_entry)
        {
            j.request = Some(mode);
            menu.page = PAGE;
            keys.clear_just_pressed(KeyCode::KeyJ);
            menu_sound::play(&mut se, crate::menu_status::CONFIRM);
            info!("journal: camp entry {:?} leaves for menu kind 3 (mode word {mode}, FUN_0024f4d0; scenes 2 then 0, FUN_00296458)", menu.current_entry());
        }
        return;
    }
    let key = if start {
        Some(Key::Start)
    } else if cancel {
        Some(Key::Cancel)
    } else if confirm {
        Some(Key::Confirm)
    } else if keys.just_pressed(KeyCode::ArrowLeft) {
        Some(Key::Move(-1, 0))
    } else if keys.just_pressed(KeyCode::ArrowRight) {
        Some(Key::Move(1, 0))
    } else if keys.just_pressed(KeyCode::ArrowUp) {
        Some(Key::Move(0, -1))
    } else if keys.just_pressed(KeyCode::ArrowDown) {
        Some(Key::Move(0, 1))
    } else if keys.just_pressed(KeyCode::KeyQ) || keys.just_pressed(KeyCode::PageUp) {
        // [inferred] the keyboard's L1 / R1: Q / E (and PageUp / PageDown).
        Some(Key::L1)
    } else if keys.just_pressed(KeyCode::KeyE) || keys.just_pressed(KeyCode::PageDown) {
        Some(Key::R1)
    } else {
        None
    };
    for k in [KeyCode::KeyJ, KeyCode::Escape, KeyCode::Backspace, KeyCode::KeyK, KeyCode::Enter, KeyCode::NumpadEnter, KeyCode::ArrowLeft, KeyCode::ArrowRight, KeyCode::ArrowUp, KeyCode::ArrowDown, KeyCode::KeyQ, KeyCode::KeyE, KeyCode::PageUp, KeyCode::PageDown] {
        keys.clear_just_pressed(k);
    }
    let (Some(key), true, None) = (key, j.loaded, j.goto) else { return };
    let Some(o) = j.open.as_ref() else { return };
    let (page, world_byte) = (o.page, o.world);
    let act = match (page, key) {
        (0 | 2, Key::L1 | Key::R1) => switch(key, page, world_byte, &j.tables.hints),
        (0, _) => page0(key, o.cursor, &o.shown, &j.worlds),
        (2, _) => page2(key, o.row, &o.cats),
        (3, _) => page3(key, o.top, o.count),
        _ => page1(key),
    };
    match act {
        Act::None => {}
        Act::Cursor(c) if page == 0 => {
            let old = o.cursor;
            j.open.as_mut().unwrap().cursor = c;
            select(&j, &mut scene, Some(old));
            place_cursor(&j, &mut scene);
            menu_sound::play(&mut se, menu_sound::CURSOR);
            info!("journal: page 0 cursor {c}");
        }
        Act::Cursor(t) if page == 3 => {
            j.open.as_mut().unwrap().top = t;
            story_text(&j, &mut scene);
            menu_sound::play(&mut se, menu_sound::CURSOR);
            info!("journal: page 3 first line {t}");
        }
        Act::Cursor(r) => {
            let o = j.open.as_mut().unwrap();
            o.row = r;
            info!("journal: page 2 row {r} (category {})", o.cats[r]);
            place_cursor(&j, &mut scene);
            menu_sound::play(&mut se, menu_sound::CURSOR);
        }
        Act::Switch(Some(to @ (0 | 2))) if !(to == 2 && world_byte as u8 > 0xe) => {
            menu_sound::play(&mut se, crate::menu_status::CONFIRM);
            info!("journal: page {page} {} -> page {to} (FUN_00277928)", if key == Key::R1 { "R1 (-8)" } else { "L1 (-9)" });
            j.goto = Some((to, world_byte));
        }
        Act::Switch(to) => {
            menu_sound::play(&mut se, menu_sound::REFUSED);
            info!("journal: page {page} {} -> page {to:x?} not ported (FUN_00277928); refused", if key == Key::R1 { "R1 (-8)" } else { "L1 (-9)" });
        }
        Act::Category { cat: 1, page: 3 } => {
            menu_sound::play(&mut se, crate::menu_status::CONFIRM);
            info!("journal: page 2 row category 1 -> page 3, the story (FUN_002a3da0 -> FUN_002a4c38)");
            let o = j.open.as_mut().unwrap();
            o.back_row = o.row;
            j.goto = Some((3, world_byte));
        }
        Act::Open { page: 2, world } => {
            menu_sound::play(&mut se, crate::menu_status::CONFIRM);
            info!("journal: page 0 world {world} -> page 2 (FUN_002a27d8)");
            j.goto = Some((2, world));
        }
        Act::Open { page, world } => {
            menu_sound::play(&mut se, menu_sound::REFUSED);
            info!("journal: page 0 world {world} -> page {page:#x} not ported; refused");
        }
        Act::Category { cat, page } => {
            menu_sound::play(&mut se, menu_sound::REFUSED);
            info!("journal: page 2 row category {cat} -> page {page:#x} not ported (FUN_002a3da0); refused");
        }
        Act::Back if page == 3 => {
            menu_sound::play(&mut se, menu_sound::CANCEL);
            info!("journal: page 3 cancel -> page 2 (FUN_002a5218 case -2)");
            j.goto = Some((2, world_byte));
        }
        Act::Back => {
            menu_sound::play(&mut se, menu_sound::CANCEL);
            info!("journal: page 2 cancel -> page 0 (FUN_002a3da0 case -2)");
            j.goto = Some((0, world_byte));
        }
        Act::Close => {
            menu_sound::play(&mut se, menu_sound::CANCEL);
            close(&mut j, &mut scene, &mut menu);
        }
    }
}

/// The cursor cell plays 0x96, the cell it left 0x97 (`FUN_002a31e8`).
fn select(j: &Journal, scene: &mut MenuScene, old: Option<usize>) {
    let Some(o) = j.open.as_ref() else { return };
    if let Some(c) = old.and_then(|i| j.nodes.cells.get(i)) {
        scene.set_group(*c, group::CELL_IDLE);
    }
    if let Some(&c) = j.nodes.cells.get(o.cursor) {
        scene.set_group(c, group::CELL_SELECTED);
    }
}

/// The scene's open: hides the camp, plays the layout, the header and the plate, then builds the start page.
fn show(j: &mut Journal, scene: &mut MenuScene, disc: &Disc, images: &mut Assets<Image>, mode: i32, world_number: Option<usize>, test: &dyn Fn(u16) -> bool) {
    let jw = world_number.and_then(|w| j.world_index.get(w).copied()).unwrap_or(-1);
    let (page, world) = start(mode, jw);
    let roxas = mode == 2;
    // Every node the scene had before this one's (the camp's frame, rows, plates, cards ...) is hidden while the journal is up.
    let first = j.nodes.layer;
    let saved: Vec<(usize, bool)> = (0..first).map(|i| (i, scene.nodes[i].visible)).collect();
    for i in 0..first {
        scene.nodes[i].visible = false;
    }
    info!("journal: open (FUN_002756f8): mode {mode}, journal world {jw}, start page {page} with world byte {world}");
    // The layout object: the enter layout, then the hold layout (`FUN_0029b530(.., t[0], t[2])`).
    let t = j.layouts[usize::from(roxas)];
    let layer = j.layd.layout(t[0]).first().cloned();
    let n = &j.nodes;
    if let Some(l) = layer {
        let id = n.layer;
        scene.nodes[id].seq = Some(j.seq_base + l.seq);
        scene.nodes[id].pos = [l.x as f32, l.y as f32];
        scene.nodes[id].visible = true;
        let hold = j.layd.layout(t[2]).first().map_or(l.group, |h| h.group);
        scene.set_group_then(id, l.group, hold);
        // The frame reaches the 4:3 canvas edges: carried out to the window's as the camp's frame is (docs/menus.md
        // 9.1) [inferred: the same rule for this scene's layout]. Its box at rest: the hold group's first frame.
        let qs = scene.seqs.get(j.seq_base + l.seq).map(|s| s.quads(hold, 0, [l.x as f32, l.y as f32], &scene.nodes[id].ext)).unwrap_or_default();
        let mut b = [f32::MAX, f32::MAX, f32::MIN, f32::MIN];
        for p in qs.iter().flat_map(|q| q.pos) {
            b = [b[0].min(p[0]), b[1].min(p[1]), b[2].max(p[0]), b[3].max(p[1])];
        }
        scene.nodes[id].stretch = (!qs.is_empty()).then_some(b);
    }
    scene.nodes[n.header].visible = true;
    scene.set_group_then(n.header, j.header[0], j.header[2]);
    scene.nodes[n.plate].visible = true;
    scene.set_group_then(n.plate, group::PLATE[0], group::PLATE[1]);
    j.open = Some(Open {
        page,
        world,
        cursor: 0,
        shown: Vec::new(),
        row: 0,
        cats: Vec::new(),
        saved,
        roxas,
        back_row: 0,
        top: 0,
        count: 0,
        lines: Vec::new(),
        gap: 0,
        size: TEXT_SIZE,
    });
    build(j, scene, disc, images, page, world, test);
}

/// Builds `page` (0, 1 or 2) after hiding the other page's nodes (`FUN_002a3da0`'s switch, `FUN_00276740`).
fn build(j: &mut Journal, scene: &mut MenuScene, disc: &Disc, images: &mut Assets<Image>, page: u16, world: i8, test: &dyn Fn(u16) -> bool) {
    // Page 3 keeps the title of page 2 (the category teardown `FUN_00277ec0` frees the `DAT_0036d100` objects
    // only; the title is `DAT_0036d0f0 + 0x350`).
    let title = j.nodes.title;
    let n = &j.nodes;
    let ids: Vec<usize> = n.page0().into_iter().chain(n.page12()).chain(n.page3()).chain([n.help, n.cursor, n.follow]).collect();
    for id in ids {
        if !(page == 3 && id == title) {
            scene.nodes[id].visible = false;
        }
    }
    match page {
        1 | 2 => build12(j, scene, disc, images, page, world, test),
        3 => build3(j, scene, world, test),
        _ => build0(j, scene, world, test),
    }
    chrome(j, scene, page);
}

fn build0(j: &mut Journal, scene: &mut MenuScene, world: i8, test: &dyn Fn(u16) -> bool) {
    let shown = cells(&j.worlds, test);
    let cursor = first_cursor(&j.worlds, &shown, world).unwrap_or(0);
    info!("journal: page 0: {} cells {:?}, cursor {cursor}", shown.len(), shown.iter().map(|&(i, p)| (j.worlds[i].code.clone(), p)).collect::<Vec<_>>());
    let n = &j.nodes;
    let list = j.seq_base + SEQ_LIST;
    let (px, py) = scene.seqs.get(list).and_then(|s| s.groups.get(group::CELL_IDLE)).map_or((80, 98), |g| (g.unk20, g.unk1c));
    let at = grid(shown.len(), px, py);
    for k in 0..CELLS {
        let (c, m, ic) = (n.cells[k], n.marks[k], n.icons[k]);
        let on = k < shown.len();
        for id in [c, m[0], m[1], ic] {
            scene.nodes[id].visible = on;
        }
        if !on {
            continue;
        }
        scene.nodes[c].pos = [at[k][0] as f32, at[k][1] as f32];
        scene.set_group(c, group::CELL_IDLE);
        // The two children start in group 1 (0x98 / 0x99 / 0x9a follow data the port does not have: docs/menus.md 20.5).
        scene.set_group(m[0], group::START);
        scene.set_group(m[1], group::START);
        let pic = shown[k].1;
        scene.nodes[ic].tex = j.world_tex.get(usize::from(pic)).copied().flatten().unwrap_or(j.tex);
        scene.set_group_then(ic, group::ICON[0], group::ICON[1]);
    }
    let o = j.open.as_mut().unwrap();
    (o.page, o.world, o.cursor, o.shown) = (0, world, cursor, shown);
    select(j, scene, None);
}

/// A text of font 1 (`FUN_00296b48` / `FUN_0029b6c0` write +0x1a0 = 1) in the colour `FUN_00138fc8` gives
/// (0xe5e5e5), the size of the node's group (+0x18, 15 when none), drawn from the group's x/y when the object keeps
/// flag 0x10 (`FUN_00139048`).
fn text(j: &Journal, scene: &MenuScene, node: usize, group: usize, id: u16, align: u8, group_offset: bool) -> Option<Label> {
    let g = scene.nodes[node].seq.and_then(|s| scene.seqs.get(s)).and_then(|s| s.groups.get(group)).copied().unwrap_or_default();
    let size = if g.size > 0 { g.size as f32 } else { TEXT_SIZE };
    let offset = if group_offset { [g.x as f32, g.y as f32] } else { [0.0; 2] };
    let t = scene.text.as_ref()?;
    let style = Style { font: 1, size: Some(size), align, ..Style::default() };
    match j.msgs.as_ref().and_then(|m| m.tokens(u32::from(id))) {
        Some(tokens) => Some(Label { tokens, style: Style { world: t.world, ..style }, offset }),
        None => t.message(u32::from(id), style, offset),
    }
}

fn build12(j: &mut Journal, scene: &mut MenuScene, disc: &Disc, images: &mut Assets<Image>, page: u16, world: i8, test: &dyn Fn(u16) -> bool) {
    let groups = if page == 2 { LAYERS_P2 } else { LAYERS_P1 };
    let story = story_texts(&j.stor, world, test);
    let record = j.worlds.iter().find(|w| w.id == world).cloned();
    // `FUN_002a3d08`: the holders 2 and 4 show the story texts, their children the two labels, centred (+0x1a4 = 2).
    let texts: [Option<(u16, u8, bool)>; 6] = [None, None, Some((story[0], 0, true)), Some((msg::LABEL_A as u16, 2, false)), Some((story[1], 0, true)), Some((msg::LABEL_B as u16, 2, false))];
    let lay = j.nodes.lay.clone();
    for k in 0..6 {
        let id = lay[k];
        scene.nodes[id].visible = true;
        scene.nodes[id].off = [0.0; 2];
        scene.set_group(id, groups[k]);
        scene.nodes[id].label = texts[k].and_then(|(m, a, go)| text(j, scene, id, groups[k], m, a, go));
    }
    // The `jm_top` pictures (`FUN_002a45b0`): page 2 the world's code (world 1's alternative once its second flag is
    // set), page 1 Roxas's name; picture 0 sits at layer 0's group x/y, picture 1 on layer 1.
    let name = match (&record, page) {
        (Some(w), 2) if w.id == 1 && test(w.flag2) => j.tables.top_alt.clone(),
        (Some(w), 2) => w.code.clone(),
        _ => j.tables.top_roxas.clone(),
    };
    if !j.top_tex.contains_key(&name) {
        let t = bar_pictures(disc, &format!("menu/us/jm_top_{name}.2ld"), scene, images).into_iter().flatten().collect();
        j.top_tex.insert(name.clone(), t);
    }
    let tops = j.top_tex.get(&name).cloned().unwrap_or_default();
    let ext0 = scene.extent(j.seq_base + SEQ_LIST, groups[0]);
    for k in 0..2 {
        let id = j.nodes.pics[k];
        let Some(&tex) = tops.get(k) else { continue };
        scene.nodes[id].tex = tex;
        scene.nodes[id].visible = true;
        scene.nodes[id].off = if k == 0 { ext0 } else { [0.0; 2] };
        // [inferred] the file arrives after the build: the per-frame call `FUN_002a4750(1)` plays enter then hold.
        let [e, h] = j.tables.pic_group[k];
        scene.set_group_then(id, e, h);
    }
    let (row_ids, kids) = (j.nodes.rows.clone(), j.nodes.row_kids.clone());
    let cats = if page == 2 {
        let all = test(ALL_WORLDS_FLAG);
        // [conservative] the quest / minigame rows also show without the all-worlds flag once a record's progress
        // test passes (`FUN_002b6d40`, the minigame bytes at 0x341620): the app has no such progress.
        categories(world, all && has_world(&j.ques, 0x10, world), all && has_world(&j.mini, 8, world))
    } else {
        Vec::new()
    };
    let rows = if page == 2 { cats.len() } else { 1 };
    let pitch = scene.seqs.get(j.seq_base + SEQ_LIST).and_then(|s| s.groups.get(groups[0])).map_or(0, |g| g.unk1c);
    for k in 0..ROWS {
        let (r, kid) = (row_ids[k], kids[k]);
        let on = k < rows;
        for id in [r, kid[0], kid[1]] {
            scene.nodes[id].visible = on;
        }
        if !on {
            continue;
        }
        // Rows hang from layer 0 one pitch (the 0x89 group's +0x1c) apart (`FUN_00138ed0(row, 0, k * pitch)`); page 1's
        // one row from layer 0 too, without text.
        scene.nodes[r].off = [0.0, (k as i32 * pitch) as f32];
        for id in [r, kid[0], kid[1]] {
            scene.set_group(id, ROW_GROUP);
        }
        scene.nodes[r].label = cats.get(k).and_then(|&c| text(j, scene, r, ROW_GROUP, j.tables.row_text[c], 0, true));
    }
    // Page 2's title on the object 0x350 (`FUN_00277258`, `FUN_00277640`).
    let title = j.nodes.title;
    if page == 2 {
        scene.nodes[title].visible = true;
        scene.set_group_then(title, TITLE[0], TITLE[1]);
        scene.nodes[title].label = record.as_ref().and_then(|w| text(j, scene, title, TITLE[1], w.title(test), 2, true));
        // The text starts at the object's position (`FUN_00138f38` -> `FUN_002375c0`): for this root object the
        // group's animated anchor (as `menu_gfx`'s `label_anchor` reads it for the camp's rows).
        scene.nodes[title].label_anchor = Some([0.0; 2]);
    }
    info!(
        "journal: page {page} (FUN_002a3540 / FUN_002a4188): world {world} '{}', jm_top_{name}.2ld {} pictures, story texts {:x?}, rows {:?}, text {}",
        record.as_ref().map_or("", |w| w.code.as_str()),
        tops.len(),
        story,
        cats,
        if scene.text.is_some() && scene.nodes[row_ids[0]].label.is_some() { "drawn" } else { "missing" }
    );
    let o = j.open.as_mut().unwrap();
    // The row cursor the confirm kept (`FUN_00277de8`), then cleared (`FUN_00277dc8(page, 0, 0)`).
    let row = if o.back_row < cats.len() { o.back_row } else { 0 };
    (o.page, o.world, o.row, o.cats, o.back_row) = (page, world, row, cats, 0);
}

/// The text tokens of a message: the journal's file first, then the system messages (`FUN_00190c40`).
fn tokens(j: &Journal, scene: &MenuScene, id: u16) -> Option<Vec<Token>> {
    j.msgs.as_ref().and_then(|m| m.tokens(u32::from(id))).or_else(|| scene.text.as_ref()?.messages.as_ref()?.tokens(u32::from(id)))
}

/// A label of font 1 in the default colour from `tokens`, with or without the group's x/y (`FUN_00139048`).
fn label_of(scene: &MenuScene, node: usize, group: usize, tokens: Vec<Token>, align: u8, group_offset: bool) -> Option<Label> {
    let g = scene.nodes[node].seq.and_then(|s| scene.seqs.get(s)).and_then(|s| s.groups.get(group)).copied().unwrap_or_default();
    let size = if g.size > 0 { g.size as f32 } else { TEXT_SIZE };
    let offset = if group_offset { [g.x as f32, g.y as f32] } else { [0.0; 2] };
    let world = scene.text.as_ref()?.world;
    Some(Label { tokens, style: Style { font: 1, size: Some(size), align, world, ..Style::default() }, offset })
}

/// The help line, the hints and the list cursor of `page` (built after the page's own objects).
fn chrome(j: &mut Journal, scene: &mut MenuScene, page: u16) {
    let Some(o) = j.open.as_ref() else { return };
    let n = &j.nodes;
    let (help, hint_nodes) = (n.help, n.hints);
    // The help line (`FUN_00278468`): page 2 the category of the cursor row at the build (`FUN_002a4a68`; nothing
    // else calls it), page 1 0x493f, page 3 0x48d7; page 0 sets none.
    let help_id = match page {
        2 => o.cats.get(o.row).map(|&c| j.tables.help[c]),
        1 => Some(msg::HELP_ROXAS),
        3 => Some(msg::HELP_STORY),
        _ => None,
    };
    scene.nodes[help].visible = help_id.is_some();
    if let Some(id) = help_id {
        scene.set_group(help, HELP_GROUP);
        let t = tokens(j, scene, id);
        scene.nodes[help].label = t.and_then(|t| label_of(scene, help, HELP_GROUP, t, 0, true));
    }
    // The L1 / R1 hints (`FUN_00277860` / `FUN_00277740` from the builds of pages 0 and 2; page 1 makes none; the
    // category pages keep them: their teardown `FUN_00277ec0` leaves the `DAT_0036d0f0` objects alone).
    match page {
        0 | 2 => {
            let names = hints(page, o.world, &j.tables.hints);
            let (il, ir) = (j.tables.icon_l1, j.tables.icon_r1);
            for (k, (id, groups)) in [(names[0], HINT_L), (names[1], HINT_R)].into_iter().enumerate() {
                let node = hint_nodes[k];
                scene.nodes[node].visible = true;
                scene.set_group_then(node, groups[0], groups[1]);
                let label = tokens(j, scene, id).map(|t| {
                    let mut v = Vec::new();
                    if k == 0 {
                        v.push(Token::Icon(il));
                    }
                    v.extend(t.into_iter().filter(|t| *t != Token::End));
                    if k == 1 {
                        v.push(Token::Icon(ir));
                    }
                    v
                });
                scene.nodes[node].label = label.and_then(|t| label_of(scene, node, groups[1], t, 2, true));
                scene.nodes[node].label_anchor = Some([0.0; 2]);
            }
        }
        1 => {
            for h in hint_nodes {
                scene.nodes[h].visible = false;
            }
        }
        _ => {}
    }
    place_cursor(j, scene);
}

/// The list cursor (`FUN_00276bb0`, each frame while the page's list has objects and the scene is idle): object
/// 0x21 of slot 0 and the follower 0x7571 (groups 0x381fc8) on the cursor's row or cell (`FUN_0029ac68`,
/// `FUN_0029b3a0`: the object follows its target, flag 0x800 [inferred: drawn at the target's position]). Pages
/// that set `DAT_0036d104` (`FUN_00277e70(1)`: 1 and the category pages) have none.
fn place_cursor(j: &Journal, scene: &mut MenuScene) {
    let Some(o) = j.open.as_ref() else { return };
    let n = &j.nodes;
    let target = match o.page {
        0 => n.cells.get(o.cursor).copied().filter(|_| o.cursor < o.shown.len()),
        2 => n.rows.get(o.row).copied().filter(|_| o.row < o.cats.len()),
        _ => None,
    };
    let follow = j.tables.follow;
    for (id, first) in [(n.cursor, true), (n.follow, false)] {
        let fresh = scene.nodes[id].parent != target || !scene.nodes[id].visible;
        scene.nodes[id].visible = target.is_some();
        scene.nodes[id].parent = target;
        scene.nodes[id].off = [0.0; 2];
        if target.is_some() && fresh {
            if first {
                scene.set_group(id, CURSOR_GROUP);
            } else {
                scene.set_group_then(id, follow[0], follow[2]);
            }
        }
    }
}

/// Page 3, the story (`FUN_002a4c38`): the category tab, the backdrop, two layers, the text row and the line marks.
fn build3(j: &mut Journal, scene: &mut MenuScene, world: i8, test: &dyn Fn(u16) -> bool) {
    let n = &j.nodes;
    let (tab, tab_follow, backdrop, lay, row, marks) = (n.tab, n.tab_follow, n.backdrop, n.story_lay.clone(), n.story_row, n.story_marks.clone());
    // The tab of plate 0 (`FUN_0029d1d0(0, -1, 0)`) with the category name the confirm put there (`FUN_0029d820(0,
    // 0x382b88[cat])`); the follower of the plate (`FUN_0029af00(.., 0x381fc8)`).
    let (plate, follow) = (j.tables.plate, j.tables.follow);
    scene.nodes[tab].visible = true;
    scene.nodes[tab].pos = [0.0; 2];
    scene.set_group_then(tab, plate[0], plate[2]);
    let name = tokens(j, scene, j.tables.row_text[1]);
    scene.nodes[tab].label = name.and_then(|t| label_of(scene, tab, plate[2], t, 0, false));
    scene.nodes[tab].label_anchor = Some([0.0; 2]);
    scene.nodes[tab_follow].visible = true;
    scene.set_group_then(tab_follow, follow[0], follow[2]);
    // The backdrop 0x755a (`FUN_00278658`): page family 1 (`FUN_00277d60(1)` of page 2's build).
    scene.nodes[backdrop].visible = true;
    scene.set_group(backdrop, j.tables.backdrop[1]);
    for (k, &id) in lay.iter().enumerate() {
        scene.nodes[id].visible = true;
        scene.nodes[id].off = [0.0; 2];
        scene.set_group(id, STORY_LAYERS[k]);
    }
    let list = j.seq_base + SEQ_LIST;
    let g = scene.seqs.get(list).and_then(|s| s.groups.get(STORY_LAYERS[0])).copied().unwrap_or_default();
    let size = if g.size > 0 { g.size as i32 } else { TEXT_SIZE as i32 };
    let font_gap = kh2_image::font::Font::get(STORY_FONT).line_gap as i32;
    let (line_h, gap) = story_metrics(g.unk1c, size, font_gap);
    for (k, &id) in marks.iter().enumerate() {
        scene.nodes[id].visible = true;
        scene.nodes[id].off = [0.0, (k as i32 * g.unk1c) as f32];
        // The marks' groups 0x382ba8 / 0x382bac follow the journal's read bits (`FUN_002b9360` / `FUN_002b9480`,
        // `DAT_0036d4e8`), which the app does not have: they stay on 2.
        scene.set_group(id, STORY_ROW);
    }
    // The entries and their lines (`FUN_002a5060`; `FUN_00191760` counts a text's lines).
    let entries = story_entries(&j.stor, world, test);
    let mut lines: Vec<Vec<Token>> = Vec::new();
    let mut counts = Vec::new();
    for &(id, _) in &entries {
        let t = tokens(j, scene, id).unwrap_or_default();
        let mut cur = Vec::new();
        let mut c = 0;
        for tok in t.into_iter().filter(|t| *t != Token::End) {
            if tok == Token::Newline {
                lines.push(std::mem::take(&mut cur));
                c += 1;
            } else {
                cur.push(tok);
            }
        }
        lines.push(cur);
        counts.push(c + 1);
    }
    let count = story_count(&counts);
    scene.nodes[row].visible = true;
    scene.nodes[row].off = [0.0; 2];
    scene.set_group(row, STORY_ROW);
    info!(
        "journal: page 3 (FUN_002a4c38): world {world}, {} story entries {:x?}, {} lines, list count {count}, line height {line_h}, gap {gap}",
        entries.len(),
        entries.iter().map(|e| e.0).collect::<Vec<_>>(),
        lines.len()
    );
    let o = j.open.as_mut().unwrap();
    (o.page, o.world, o.top, o.count, o.lines, o.gap, o.size) = (3, world, 0, count, lines, gap.min(255) as u8, size as f32);
    story_text(j, scene);
}

/// The story row's text: ten lines from the first one shown (`0x2a54c8`: the scissor of ten line heights, every
/// entry from the one that holds the first line), in font 3, with the extra gap; the colour is the row's +0x198
/// 0x60000008 [inferred: the value `FUN_002a4c38` writes there is the draw's colour argument].
fn story_text(j: &Journal, scene: &mut MenuScene) {
    let Some(o) = j.open.as_ref() else { return };
    let mut t = vec![Token::Gap(o.gap)];
    for (k, l) in o.lines.iter().skip(o.top).take(STORY_LINES).enumerate() {
        if k > 0 {
            t.push(Token::Newline);
        }
        t.extend(l.iter().cloned());
    }
    t.push(Token::End);
    let world = scene.text.as_ref().map_or(0, |x| x.world);
    let style = Style { font: STORY_FONT, size: Some(o.size), align: 0, world, color: [0x08, 0x00, 0x00, 0x60], ..Style::default() };
    scene.nodes[j.nodes.story_row].label = Some(Label { tokens: t, style, offset: [0.0; 2] });
}

fn close(j: &mut Journal, scene: &mut MenuScene, menu: &mut Menu) {
    let Some(o) = j.open.take() else { return };
    j.goto = None;
    for id in j.nodes.all() {
        scene.nodes[id].visible = false;
    }
    for (i, v) in o.saved {
        scene.nodes[i].visible = v;
    }
    menu.page = 0;
    info!("journal: page {} closed (FUN_00295ba0, roxas {}): the scene stack runs the camp again (FUN_00296278: scene 0 next)", o.page, o.roxas);
}

#[allow(clippy::too_many_arguments)]
fn tick(
    mut j: ResMut<Journal>,
    mut menu: ResMut<Menu>,
    mut scene: ResMut<MenuScene>,
    mut images: ResMut<Assets<Image>>,
    disc: Res<Disc>,
    area: Res<CurrentArea>,
    progress: Option<Res<Progress>>,
    mut logged: Local<u32>,
) {
    if menu.phase == Phase::Closed {
        if j.open.is_some() {
            close(&mut j, &mut scene, &mut menu);
        }
        j.request = None;
        return;
    }
    let story = progress.as_ref().map(|p| p.story());
    let test = |id: u16| story.as_ref().is_some_and(|s| s.test(id));
    if let Some(mode) = j.request.take() {
        if !j.tried && scene.texs.len() >= 2 {
            load(&mut j, &disc, &mut scene, &mut images);
        }
        if !j.loaded {
            menu.page = 0;
            return;
        }
        let world = area.name.get(..2).and_then(|w| crate::audio::WORLDS.iter().position(|&n| n == w));
        show(&mut j, &mut scene, &disc, &mut images, mode, world, &test);
    }
    if let Some((page, world)) = j.goto.take()
        && j.open.is_some()
    {
        build(&mut j, &mut scene, &disc, &mut images, page, world, &test);
        *logged = 0;
    }
    // The camp redraws its plates and digits every frame: keep its nodes hidden while the journal is up.
    if let Some(o) = j.open.as_ref() {
        for &(i, _) in &o.saved {
            scene.nodes[i].visible = false;
        }
        // `KH2_JOURNAL_QUADS=1`: the box each journal node draws, once, a second after the open (harness aid).
        *logged += 1;
        if *logged == 60 && std::env::var_os("KH2_JOURNAL_QUADS").is_some() {
            scene.resolve();
            for id in j.nodes.all() {
                let qs = scene.node_quads(id);
                if qs.is_empty() {
                    continue;
                }
                let (mut a, mut b) = ([f32::MAX; 2], [f32::MIN; 2]);
                for p in qs.iter().flat_map(|q| q.pos) {
                    a = [a[0].min(p[0]), a[1].min(p[1])];
                    b = [b[0].max(p[0]), b[1].max(p[1])];
                }
                info!("journal: node {id} group {:?} box ({:.0},{:.0})-({:.0},{:.0})", scene.nodes[id].player.group, a[0], a[1], b[0], b[1]);
            }
        }
    } else {
        *logged = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn worlds() -> Vec<World> {
        [(0x0f, "jm", 0x831), (0, "tt", 0x831), (1, "hb", 0x1002), (2, "bb", 0x1402), (0x10, "jm", 0x831)]
            .iter()
            .map(|&(id, c, f)| World { id, code: c.into(), flag: f, flag2: 0, names: [0; 4] })
            .collect()
    }

    #[test]
    fn the_start_page_follows_the_mode_and_the_world() {
        // Jiminy's Journal (mode 1) in a world with a journal index opens that world's page 2.
        assert_eq!(start(1, 0), (2, 0));
        // In a world without one (-1) the mode drops to 0: the world grid.
        assert_eq!(start(1, -1), (0, -1));
        // Roxas's Journal (mode 2) opens page 1 with the byte 0x13, whatever the world.
        assert_eq!(start(2, 5), (1, 0x13));
        assert_eq!(start(2, -1), (1, 0x13));
        assert_eq!(start(3, 4), (10, 4));
        assert_eq!(start(0, 4), (0, -1));
        assert_eq!(start(-5, 4), (0, -1));
    }

    #[test]
    fn camp_rows_five_and_six_write_modes_one_and_two() {
        assert_eq!(mode_of_entry(5), Some(1));
        assert_eq!(mode_of_entry(6), Some(2));
        assert_eq!(mode_of_entry(4), None);
    }

    #[test]
    fn cells_need_the_story_flag_or_the_all_worlds_flag() {
        let w = worlds();
        let tt_only = |id: u16| id == 0x831;
        // id 0x0f (jm) and 0 (tt) share the flag; 0x10 is never a cell.
        assert_eq!(cells(&w, &tt_only), vec![(0, 0x0f), (1, 0)]);
        let all = |id: u16| id == 0x831 || id == ALL_WORLDS_FLAG;
        assert_eq!(cells(&w, &all), vec![(0, 0x0f), (1, 0), (2, LOCKED_ICON), (3, LOCKED_ICON)]);
        assert!(cells(&w, &|_| false).is_empty());
        // The first cursor is the world's place among the shown records (0xf for a negative byte).
        let s = cells(&w, &tt_only);
        assert_eq!(first_cursor(&w, &s, 0), Some(1));
        assert_eq!(first_cursor(&w, &s, -1), Some(0));
        assert_eq!(first_cursor(&w, &s, 2), None);
    }

    #[test]
    fn the_first_cursor_indexes_the_shown_cells() {
        // A record with id 0x10..0x13 before the wanted world gets no cell: the cursor still lands on that world's cell.
        let mut w = worlds();
        w.insert(0, World { id: 0x11, code: "zz".into(), flag: 0x831, flag2: 0, names: [0; 4] });
        let s = cells(&w, &|id| id == 0x831);
        let c = first_cursor(&w, &s, 0).unwrap();
        assert_eq!(w[s[c].0].id, 0);
    }

    #[test]
    fn the_grid_centres_rows_of_six() {
        assert_eq!(grid(1, 80, 98), vec![[0, 0]]);
        assert_eq!(grid(2, 80, 98), vec![[-40, 0], [40, 0]]);
        let g = grid(8, 80, 98);
        assert_eq!(g.len(), 8);
        assert_eq!(g[0], [-200, -49]);
        assert_eq!(g[5], [200, -49]);
        // The partial second row is centred on its own.
        assert_eq!(g[6], [-40, 49]);
        assert_eq!(g[7], [40, 49]);
    }

    #[test]
    fn page_zero_state_machine() {
        let w = worlds();
        let shown = cells(&w, &|id| id == 0x831 || id == ALL_WORLDS_FLAG);
        assert_eq!(page0(Key::Move(1, 0), 0, &shown, &w), Act::Cursor(1));
        assert_eq!(page0(Key::Move(-1, 0), 0, &shown, &w), Act::None, "no wrap");
        assert_eq!(page0(Key::Move(0, 1), 0, &shown, &w), Act::None, "no second row");
        // A confirm asks for the world's page: 0xc above id 0xe, else 2.
        assert_eq!(page0(Key::Confirm, 0, &shown, &w), Act::Open { page: 0xc, world: 0x0f });
        assert_eq!(page0(Key::Confirm, 1, &shown, &w), Act::Open { page: 2, world: 0 });
        assert_eq!(page0(Key::Cancel, 2, &shown, &w), Act::Close);
        assert_eq!(page0(Key::Start, 2, &shown, &w), Act::Close);
        assert_eq!(page0(Key::Confirm, 0, &[], &w), Act::None);
        assert_eq!(step(2, 14, 0, 1), 8);
        assert_eq!(step(10, 14, 0, 1), 10);
    }

    #[test]
    fn page_two_rows_and_state_machine() {
        // Category 3 is left out for journal world 8, 5 and 6 need their lists.
        assert_eq!(categories(0, false, false), vec![0, 1, 2, 3, 4, 7]);
        assert_eq!(categories(8, true, true), vec![0, 1, 2, 4, 5, 6, 7]);
        let cats = categories(0, false, false);
        assert_eq!(page2(Key::Move(0, 1), 0, &cats), Act::Cursor(1));
        assert_eq!(page2(Key::Move(0, -1), 0, &cats), Act::Cursor(5), "a new press wraps");
        assert_eq!(page2(Key::Move(0, 1), 5, &cats), Act::Cursor(0));
        assert_eq!(page2(Key::Move(1, 0), 0, &cats), Act::None);
        assert_eq!(page2(Key::Confirm, 3, &cats), Act::Category { cat: 3, page: 7 });
        assert_eq!(page2(Key::Confirm, 5, &cats), Act::Category { cat: 7, page: 0xb });
        assert_eq!(page2(Key::Cancel, 0, &cats), Act::Back);
        assert_eq!(page2(Key::Start, 0, &cats), Act::Close);
        assert_eq!(page1(Key::Cancel), Act::Close);
        assert_eq!(page1(Key::Confirm), Act::None);
        assert_eq!((0..8).map(category_page).collect::<Vec<_>>(), vec![6, 3, 4, 7, 8, 9, 10, 0xb]);
    }

    #[test]
    fn story_texts_take_the_last_open_record() {
        let mut d = vec![0u8; 0x10 + 3 * 0xc];
        d[8] = 3;
        for (k, (w, a, b, f)) in [(0i8, 0x10u16, 0x11u16, 1u16), (0, 0x20, 0x21, 2), (1, 0x30, 0x31, 1)].into_iter().enumerate() {
            let r = 0x10 + k * 0xc;
            d[r] = w as u8;
            d[r + 4..r + 6].copy_from_slice(&a.to_le_bytes());
            d[r + 6..r + 8].copy_from_slice(&b.to_le_bytes());
            d[r + 10..r + 12].copy_from_slice(&f.to_le_bytes());
        }
        assert_eq!(story_texts(&d, 0, &|f| f == 1), [0x10, 0x11]);
        assert_eq!(story_texts(&d, 0, &|_| true), [0x20, 0x21]);
        assert_eq!(story_texts(&d, 2, &|_| true), [NO_TEXT; 2]);
        assert!(has_world(&d, 0xc, 1) && !has_world(&d, 0xc, 2));
    }

    #[test]
    fn hints_name_the_neighbour_pages() {
        let t = [0x10, 0x20, 0x30];
        // Page 0: left the third name (page 0xc), right the second (page 2).
        assert_eq!(hints(0, 0, &t), [0x30, 0x20]);
        assert_eq!(hints(2, 0, &t), [0x10, 0x30]);
        assert_eq!(hints(1, 0, &t), [0x10, 0x30]);
        assert_eq!(hints(0xc, 0, &t), [0x20, 0x10]);
        // A world byte above 0xe (or -1) never names page 2.
        assert_eq!(hints(0, -1, &t), [0x30, 0x30]);
        assert_eq!(hints(0xc, 0x10, &t), [0x10, 0x10]);
        assert_eq!(hint_page(0x20, &t), Some(2));
        assert_eq!(hint_page(0x30, &t), Some(0xc));
        assert_eq!(hint_page(0x99, &t), None);
        assert_eq!(switch(Key::R1, 0, 0, &t), Act::Switch(Some(2)));
        assert_eq!(switch(Key::L1, 2, 0, &t), Act::Switch(Some(0)));
        assert_eq!(switch(Key::R1, 2, 0, &t), Act::Switch(Some(0xc)));
        assert_eq!(pad_icon(&[(0x400, 0x2b), (0x800, 0x2c)], 0x800), 0x2c);
        assert_eq!(pad_icon(&[], 0x800), 0xb);
    }

    #[test]
    fn story_page_entries_lines_and_state_machine() {
        let mut d = vec![0u8; 0x10 + 3 * 0xc];
        d[8] = 3;
        for (k, (w, text, f)) in [(0i8, 0x100u16, 1u16), (1, 0x200, 1), (0, 0x300, 2)].into_iter().enumerate() {
            let r = 0x10 + k * 0xc;
            d[r] = w as u8;
            d[r + 8..r + 10].copy_from_slice(&text.to_le_bytes());
            d[r + 10..r + 12].copy_from_slice(&f.to_le_bytes());
        }
        assert_eq!(story_entries(&d, 0, &|_| true), vec![(0x100, 0), (0x300, 2)]);
        assert_eq!(story_entries(&d, 0, &|f| f == 2), vec![(0x300, 2)]);
        // The list count: all lines but the last entry's, plus one, plus the last entry's lines beyond ten.
        assert_eq!(story_count(&[3, 4]), 4);
        assert_eq!(story_count(&[3, 12]), 6);
        assert_eq!(story_count(&[]), 1);
        assert_eq!(story_metrics(28, 18, 2), (28, 8));
        assert_eq!(story_metrics(10, 18, 2), (20, 0));
        assert_eq!(page3(Key::Move(0, 1), 0, 4), Act::Cursor(1));
        assert_eq!(page3(Key::Move(0, 1), 3, 4), Act::None);
        assert_eq!(page3(Key::Move(0, -1), 0, 4), Act::None);
        assert_eq!(page3(Key::Cancel, 2, 4), Act::Back);
        assert_eq!(page3(Key::Start, 2, 4), Act::Close);
        assert_eq!(page3(Key::Confirm, 2, 4), Act::None);
        assert_eq!(page3(Key::R1, 0, 32), Act::Cursor(10));
        assert_eq!(page3(Key::R1, 25, 32), Act::Cursor(31));
        assert_eq!(page3(Key::L1, 5, 32), Act::Cursor(0));
        assert_eq!(page3(Key::L1, 0, 32), Act::None);
    }

    /// The executable's hint, help, plate and icon tables and the story page's groups (skipped without `KH2_ISO`).
    #[test]
    fn the_disc_category_page_tables() {
        let Ok(iso) = std::env::var("KH2_ISO") else { return };
        let Ok(mut a) = kh2_archive::Archive::open(&iso) else { return };
        let elf = ["SLUS_210.05", "SLUS_210.05;1"].iter().find_map(|p| a.iso().read(p).ok()).unwrap();
        let t = read_page_tables(&Exe::load(elf).unwrap()).expect("the page tables");
        eprintln!("hints {:x?} help {:x?} plate {:x?} follow {:x?} backdrop {:x?} icons {:x} {:x}", t.hints, t.help, t.plate, t.follow, t.backdrop, t.icon_l1, t.icon_r1);
        assert!(t.hints.iter().all(|&m| m != 0 && m != 0xffff));
        assert!(t.help.iter().all(|&m| m != 0));
        assert_ne!(t.icon_l1, t.icon_r1, "L1 and R1 have their own icons");
        let two = layd::parse_2ld(&a.read("menu/us/jmemo.2ld").unwrap()).unwrap();
        let s5 = &two.layd.seqs[SEQ_LIST];
        for g in [HELP_GROUP, CURSOR_GROUP, HINT_L[0], HINT_L[1], HINT_R[0], HINT_R[1], STORY_LAYERS[0], STORY_LAYERS[1], STORY_ROW, t.backdrop[1]]
            .into_iter()
            .chain(t.plate)
            .chain(t.follow)
        {
            assert!(s5.groups.len() > g, "group {g:#x}");
        }
        assert!(s5.groups[STORY_LAYERS[0]].unk1c > 0, "the story line height");
        let bar = a.read("menu/us/jiminy.bar").unwrap();
        let e = kh2_bar::parse(&bar).unwrap();
        // Twilight Town (journal world 0) has story records, all with a text.
        let all = story_entries(e[1].data, 0, &|_| true);
        assert!(!all.is_empty() && all.iter().all(|r| r.0 != 0));
        let flags: Vec<u16> = records(e[1].data, 0xc).iter().filter(|r| r[0] == 0).map(|r| u16::from_le_bytes([r[10], r[11]])).collect();
        eprintln!("Twilight Town story records: texts {:x?}, flags {flags:?}", all.iter().map(|r| r.0).collect::<Vec<_>>());
        let msgs = kh2_image::text::Messages::from_bar(&a.read("msg/us/jm.bar").unwrap()).unwrap();
        for id in t.hints.iter().chain(&t.help).copied().chain([msg::HELP_ROXAS, msg::HELP_STORY]).chain(all.iter().map(|r| r.0)) {
            assert!(msgs.tokens(u32::from(id)).is_some(), "message {id:#x} in jm.bar");
        }
    }

    /// Pages 1 and 2's data on the disc (skipped without `KH2_ISO`).
    #[test]
    fn the_disc_page_two_data() {
        let Ok(iso) = std::env::var("KH2_ISO") else { return };
        let Ok(mut a) = kh2_archive::Archive::open(&iso) else { return };
        let elf = ["SLUS_210.05", "SLUS_210.05;1"].iter().find_map(|p| a.iso().read(p).ok()).unwrap();
        let t = read_page_tables(&Exe::load(elf).unwrap()).expect("the page tables");
        assert_eq!(t.pic_seq, [4, 3]);
        assert_eq!(t.pic_group, [[1, 2], [1, 2]]);
        assert_eq!(t.top_roxas, "tt2");
        assert!(t.row_text.iter().all(|&m| m != 0));
        let two = layd::parse_2ld(&a.read("menu/us/jmemo.2ld").unwrap()).unwrap();
        let s5 = &two.layd.seqs[SEQ_LIST];
        for g in LAYERS_P2.iter().chain(&LAYERS_P1).chain(&TITLE) {
            assert!(s5.groups.len() > *g, "group {g:#x}");
        }
        assert!(s5.groups[LAYERS_P2[0]].unk1c > 0, "the row pitch");
        for g in [TITLE[1], 3, 0x91] {
            let x = s5.groups[g];
            eprintln!("group {g:#x}: x {} y {} size {} 1c {} 20 {}", x.x, x.y, x.size, x.unk1c, x.unk20);
        }
        for s in t.pic_seq {
            assert!(two.layd.seqs[s].groups.len() > 2);
        }
        let bar = a.read("menu/us/jiminy.bar").unwrap();
        let e = kh2_bar::parse(&bar).unwrap();
        let w = parse_worlds(e[0].data).unwrap();
        assert!(!records(e[1].data, 0xc).is_empty(), "story records");
        // Every world code with an id below 0xf, and Roxas's name, has a jm_top file of pictures.
        for name in w.iter().filter(|w| (0..0xf).contains(&w.id)).map(|w| w.code.clone()).chain([t.top_roxas.clone(), t.top_alt.clone()]) {
            let f = a.read(&format!("menu/us/jm_top_{name}.2ld")).unwrap_or_else(|_| panic!("jm_top_{name}.2ld"));
            let pics = kh2_bar::parse(&f).unwrap();
            assert!(pics.len() >= 2 && pics.iter().take(2).all(|p| kh2_image::imgd::decode(p.data).is_some()), "{name}");
        }
    }

    /// The disc's files and the executable's tables (skipped without `KH2_ISO`).
    #[test]
    fn the_disc_journal_files_and_tables() {
        let Ok(iso) = std::env::var("KH2_ISO") else { return };
        let Ok(mut a) = kh2_archive::Archive::open(&iso) else { return };
        let two = layd::parse_2ld(&a.read("menu/us/jmemo.2ld").unwrap()).expect("jmemo.2ld");
        assert_eq!(two.images.len(), 1);
        assert_eq!(two.layd.seqs.iter().map(|s| s.groups.len()).collect::<Vec<_>>(), vec![7, 4, 4, 4, 4, 155]);
        assert_eq!(two.layd.layouts.len(), 6);
        let s5 = &two.layd.seqs[SEQ_LIST];
        for g in [group::CELL_SELECTED, group::CELL_IDLE, group::PLATE[0], group::PLATE[1]] {
            assert!(s5.groups[g].count > 0, "group {g:#x} draws");
        }
        assert!(s5.groups[group::CELL_IDLE].unk20 > 0 && s5.groups[group::CELL_IDLE].unk1c > 0, "the grid pitches");
        let bar = a.read("menu/us/jiminy.bar").unwrap();
        let entries = kh2_bar::parse(&bar).unwrap();
        assert_eq!(entries.len(), 9);
        let w = parse_worlds(entries[0].data).expect("the world list");
        assert_eq!(w.len(), 17);
        assert!(w.iter().any(|w| w.code == "tt" && w.id == 0));
        let pic_file = a.read("menu/us/jm_world.2ld").unwrap();
        let pics = kh2_bar::parse(&pic_file).unwrap();
        assert!(pics.len() > usize::from(LOCKED_ICON), "a picture per world id and the locked one");
        assert!(pics.iter().all(|e| kh2_image::imgd::decode(e.data).is_some()));
        let elf = ["SLUS_210.05", "SLUS_210.05;1"].iter().find_map(|p| a.iso().read(p).ok()).unwrap();
        let exe = Exe::load(elf).unwrap();
        let idx = read_world_index(&exe).expect("the stubs of FUN_00277c88");
        assert_eq!(idx.len(), 19);
        // Twilight Town (game world 2) is journal world 0, the dark world (0) has none.
        assert_eq!(idx[2], 0);
        assert_eq!(idx[0], -1);
        assert_eq!(triple(&exe, addr::LAYOUTS), Some([0, 2, 1]));
        assert_eq!(triple(&exe, addr::LAYOUTS_ROXAS), Some([3, 5, 4]));
        // Every journal world id the stubs answer has a record.
        for &i in idx.iter().filter(|&&i| i >= 0) {
            assert!(w.iter().any(|r| r.id == i), "world {i}");
        }
    }
}
