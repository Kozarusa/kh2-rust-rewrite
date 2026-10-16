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
//! What the port draws: pages 0, 1 and 2 (docs/menus.md 20.7), and since WP-MN11 (20.8) the help line, the list
//! cursor, the L1 / R1 hints and switches between pages 0 and 2, and page 3 (the story). WP-MN12 (20.9) added the scroll bar
//! object, the NEW tags (`marks.rs`), the Characters and Character Links pages (6, 0xb: `chars.rs`), Treasures and Maps (7,
//! 8: `lists.rs`, `treasure_page.rs`) and the Album (4, 5: `album_page.rs`). The other pages (9, 10, 0xc and the Roxas pages) and
//! a character's own page (0xf) are not ported: a confirm on their rows, an L1 / R1 to page 0xc, or a confirm on a page 0
//! cell of id above 0xe, is refused with a log line.

mod album_page;
mod chars;
mod games;
mod games_page;
mod lists;
mod marks;
mod treasure_page;

use bevy::prelude::*;
use kh2_image::layd::{self, Layd};

use self::chars::{CharRec, Face, Group as FaceGroup};
use self::marks::{Bar, Cat, Marks, Tag};
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
    /// The scroll bar's part groups (u16 x 4, three equal sets; `FUN_00299878` via `FUN_00277e58`).
    pub const BAR: u32 = 0x0038_1fd0;
    /// Page 2's row tags (u32: new, new and seen; `FUN_002a39b8`), page 3's line tags (`FUN_002a5708`) and page 0's
    /// cell tags (`FUN_002a2248`: any new, all seen).
    pub const ROW_TAG: u32 = 0x0038_2b80;
    pub const LINE_TAG: u32 = 0x0038_2ba8;
    pub const CELL_TAG: u32 = 0x0038_2aec;
    /// The face page (`FUN_002ae3e8`): the hover groups (u32: page 6, other pages), the help messages (u32), the new / seen
    /// tag groups (u32 x 4: family 0 then links), the picture groups (u16 pairs at +0 / +4 with a 6-byte stride per
    /// family), the frame groups by style (u16 x 12) and the face file names by family (pointers).
    pub const FACE_HOVER: u32 = 0x0038_2efc;
    pub const FACE_HELP: u32 = 0x0038_2f04;
    pub const FACE_TAG: u32 = 0x0038_2f0c;
    pub const FACE_PIC: u32 = 0x0038_2f20;
    pub const FACE_STYLE: u32 = 0x0038_2f30;
    pub const FACE_NAMES: u32 = 0x0036_d3f8;
    /// The frame group of a character's own page by style (u16 x 6, `FUN_002aadf0`).
    pub const CHAR_FRAME: u32 = 0x0038_2ea0;
    pub const LIST_TEXT: u32 = 0x0038_1fe8;
    pub const QUEST_TAG: u32 = 0x0038_2ff8;
    pub const GAME_TAG: u32 = 0x0038_2fec;
}

/// The literal messages of `FUN_002a3d08` (the labels beside the two story texts).
mod msg {
    pub const LABEL_A: u32 = 0x3253;
    pub const LABEL_B: u32 = 0x3254;
    /// Page 1's help line (`FUN_002a4a68` when the page is 1).
    pub const HELP_ROXAS: u16 = 0x493f;
    /// Page 3's help line (`FUN_002a54a8`).
    pub const HELP_STORY: u16 = 0x48d7;
    /// Pages 7 and 8's help lines (`FUN_002b0c98`), "There's nothing yet!" for a world's empty section (`FUN_002b0678`) and
    /// the total's label (`FUN_002b0e78`).
    /// The help line of a character's own page (`FUN_002ac450`: pages 0xf, 0x12, 0x15).
    pub const HELP_CHAR: u16 = 0x48f2;
    pub const HELP_TREASURE: u16 = 0x48f4;
    pub const HELP_MAP: u16 = 0x48f5;
    pub const NOTHING_TREASURE: u16 = 0x4a0f;
    pub const NOTHING_MAP: u16 = 0x4a12;
    pub const TOTAL_TREASURE: u16 = 0x4a11;
    pub const TOTAL_MAP: u16 = 0x4a13;
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
/// The completion mark of page 2's rows (`FUN_002a39b8`: the first child on 0x8a).
const ROW_DONE: usize = 0x8a;
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
    /// A finished world's mark on its page-0 cell (`FUN_002baf08`).
    pub const CELL_DONE: usize = 0x9a;
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

/// The story records that exist now (`FUN_002b9198`: the world byte below 0xf and the flag +0xa set): their keys for the
/// new bits.
pub fn story_present(stor: &[u8], test: &dyn Fn(u16) -> bool) -> Vec<(Cat, usize)> {
    records(stor, 0xc)
        .iter()
        .enumerate()
        .filter(|(_, r)| (r[0] as i8) < 0xf && test(u16::from_le_bytes([r[10], r[11]])))
        .map(|(i, _)| (Cat::Story, i))
        .collect()
}

/// The keys of every story record of `world` (the argument of `FUN_002b9360` / `FUN_002b9480` with the index -1).
pub fn story_keys(stor: &[u8], world: Option<i8>) -> Vec<(Cat, usize)> {
    records(stor, 0xc).iter().enumerate().filter(|(_, r)| world.is_none_or(|w| r[0] as i8 == w)).map(|(i, _)| (Cat::Story, i)).collect()
}

/// The new tag of a world's cell on page 0 (`FUN_002b9658` / `FUN_002b9798`): the categories' aggregates joined: `None`
/// when nothing in the world is new, else whether everything new has been seen. World 0xf counts every world.
fn world_aggregate(j: &Journal, world: i8, test: &dyn Fn(u16) -> bool, items: &dyn Fn(u16) -> i32) -> Option<bool> {
    let all = world == 0xf;
    let chars_of = |family: u8| face_keys(j, family, test, items);
    let wi = |w: u8| j.world_index.get(usize::from(w)).copied().unwrap_or(-1);
    let is_map = |i: u16| j.tables_lists.items.get(&i).is_some_and(|r| r.kind == treasure_page::MAP_KIND);
    let parts = [
        j.marks.aggregate(story_keys(&j.stor, if all { None } else { Some(world) })),
        // `FUN_002af5e0(0, world)` and the links `FUN_002af5e0(1, -1)`; world 0xf takes every character (`FUN_002b7408(-1)`).
        j.marks.aggregate(if all { (0..j.chars.len()).map(|c| (Cat::Char, c)).collect() } else { chars_of(world as u8) }),
        j.marks.aggregate(chars_of(LINKS)),
        j.marks.aggregate(album_page::keys(&j.album, if all { None } else { Some(world) })),
        j.marks.aggregate(treasure_page::treasure_keys(&j.tables_lists.rows, &wi, if all { None } else { Some(world) })),
        j.marks.aggregate(treasure_page::map_keys(&j.tables_lists.rows, &wi, &is_map, if all { None } else { Some(world) })),
        j.marks.aggregate(games::quest_keys(&j.quests, if all { None } else { Some(world) })),
        j.marks.aggregate(games::game_keys(&j.minis, if all { None } else { Some(world) })),
    ];
    parts.iter().any(Option::is_some).then(|| parts.iter().all(|p| p.unwrap_or(true)))
}

/// `FUN_002baf08` for a world below 0xf: every category of the world is complete (the album, the quests, the minigames, the
/// treasures, the maps, the story, the characters and the character links).
fn world_complete(j: &Journal, world: i8, test: &dyn Fn(u16) -> bool, items: &dyn Fn(u16) -> i32) -> bool {
    if world as u8 > 0xe {
        return false;
    }
    let wi = |w: u8| j.world_index.get(usize::from(w)).copied().unwrap_or(-1);
    let is_map = |i: u16| j.tables_lists.items.get(&i).is_some_and(|r| r.kind == treasure_page::MAP_KIND);
    album_page::complete(&j.album, world, test)
        && games_page::with_game(j, test, |g| games::quests_complete(&j.quests, Some(world), g) && games::games_complete(&j.minis, Some(world), g))
        && treasure_page::treasures_complete(&j.tables_lists.rows, &wi, world, &|c| j.opened.contains(&c))
        && treasure_page::maps_complete(&j.tables_lists.rows, &wi, &is_map, &|i| items(i) > 0, world)
        && story_complete(&j.stor, world, test)
        && chars::family_complete(&j.groups, world as u8, test, items)
        && chars::family_complete(&j.groups, LINKS, test, items)
}

/// What page 2's row of category `cat` shows (`FUN_002a39b8`): the new tag state (`None`: no new entry, else whether all
/// have been seen) and whether the category is complete (the first child's 0x8a).
fn row_marks(j: &Journal, cat: usize, world: i8, test: &dyn Fn(u16) -> bool, items: &dyn Fn(u16) -> i32) -> (Option<bool>, bool) {
    match cat {
        0 => (j.marks.aggregate(face_keys(j, world as u8, test, items)), chars::family_complete(&j.groups, world as u8, test, items)),
        7 => (j.marks.aggregate(face_keys(j, LINKS, test, items)), chars::family_complete(&j.groups, LINKS, test, items)),
        2 => (j.marks.aggregate(album_page::keys(&j.album, Some(world))), album_page::complete(&j.album, world, test)),
        3 => {
            let wi = |w: u8| j.world_index.get(usize::from(w)).copied().unwrap_or(-1);
            (j.marks.aggregate(treasure_page::treasure_keys(&j.tables_lists.rows, &wi, Some(world))), treasure_page::treasures_complete(&j.tables_lists.rows, &wi, world, &|c| j.opened.contains(&c)))
        }
        4 => {
            let wi = |w: u8| j.world_index.get(usize::from(w)).copied().unwrap_or(-1);
            let is_map = |i: u16| j.tables_lists.items.get(&i).is_some_and(|r| r.kind == treasure_page::MAP_KIND);
            (j.marks.aggregate(treasure_page::map_keys(&j.tables_lists.rows, &wi, &is_map, Some(world))), treasure_page::maps_complete(&j.tables_lists.rows, &wi, &is_map, &|i| items(i) > 0, world))
        }
        1 => (j.marks.aggregate(story_keys(&j.stor, Some(world))), story_complete(&j.stor, world, test)),
        5 => (j.marks.aggregate(games::quest_keys(&j.quests, Some(world))), games_page::with_game(j, test, |g| games::quests_complete(&j.quests, Some(world), g))),
        6 => (j.marks.aggregate(games::game_keys(&j.minis, Some(world))), games_page::with_game(j, test, |g| games::games_complete(&j.minis, Some(world), g))),
        _ => (None, false),
    }
}

/// The family of the Character Links page (`FUN_002af398(1, -1)`).
const LINKS: u8 = 0x10;

/// The keys of the characters a family shows now (`FUN_002af5e0` walks the list `FUN_002af398` builds).
fn face_keys(j: &Journal, family: u8, test: &dyn Fn(u16) -> bool, items: &dyn Fn(u16) -> i32) -> Vec<(Cat, usize)> {
    chars::faces(&j.diag, &j.groups, family, test, items).iter().map(|f| (Cat::Char, f.ch)).collect()
}

/// The story category is complete for `world` when the last of its records exists (`FUN_002baca8`: the bit of the world's
/// last record in the completion buffer; a world without records counts as complete).
pub fn story_complete(stor: &[u8], world: i8, test: &dyn Fn(u16) -> bool) -> bool {
    records(stor, 0xc).iter().rfind(|r| r[0] as i8 == world).is_none_or(|r| test(u16::from_le_bytes([r[10], r[11]])))
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
    /// L2 / R2: the picture page's own pair (`FUN_002a62d0`: its help line shows the L2 and R2 icons, masks 0x100 / 0x200 of the
    /// table 0x380ab0; list results -11 / -10, `FUN_002a6578`) [inferred: the keyboard's `[` / `]` and Z / C].
    L2,
    R2,
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
    /// A confirm on the face page (6, 0xb): the face under the pointer (`FUN_002ae790`: sound 2 and page 0xf, or sound 4 with
    /// nothing under it).
    Face(Option<usize>),
    /// A cursor press on the Treasures / Maps pages (`FUN_002b1890`).
    Navigate(lists::Nav),
    /// A confirm on an Album row: the picture viewer, page 5 (`FUN_002a6010`).
    Photo(usize),
    /// L1 / R1 on the picture page: the previous (-1) or next (+1) picture (`FUN_002a6850`).
    Browse(i32),
    /// L2 / R2 on a character's own page: the previous (false) or next (true) character (`FUN_002aa600`).
    Neighbour(bool),
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
        Key::L1 | Key::R1 | Key::L2 | Key::R2 => Act::None,
    }
}

/// The face pages' state machine (`FUN_002ae790`, the button part): a cancel (-2: the triangle button, bit 8) goes back to
/// page 2, Start (bit 0x100) closes the scene, a confirm (bit 4) acts on the face under the pointer. The pointer itself
/// moves with the held direction (`chars::step_pointer`).
pub fn page_faces(key: Key, sel: Option<usize>) -> Act {
    match key {
        Key::Start => Act::Close,
        Key::Cancel => Act::Back,
        Key::Confirm => Act::Face(sel),
        _ => Act::None,
    }
}

/// A character's own page (`FUN_002a9b80`): Up / Down and L1 / R1 scroll the description, L2 / R2 (-11 / -10, enabled when more
/// than one character of the list exists) change the character, -2 goes back to the face page, -4 closes. The triangle-side
/// button -6 (page 0x10, the 3D model view) is not ported.
pub fn page_char(key: Key, top: usize, count: usize, many: bool) -> Act {
    match key {
        Key::Start => Act::Close,
        Key::Cancel => Act::Back,
        Key::Move(_, dy) if dy != 0 => scroll_to(top, chars::desc_scroll(top, count, dy, 0)),
        Key::L1 => scroll_to(top, chars::desc_scroll(top, count, 0, -1)),
        Key::R1 => scroll_to(top, chars::desc_scroll(top, count, 0, 1)),
        Key::L2 if many => Act::Neighbour(false),
        Key::R2 if many => Act::Neighbour(true),
        _ => Act::None,
    }
}

fn scroll_to(top: usize, to: usize) -> Act {
    if to == top { Act::None } else { Act::Cursor(to) }
}

/// The Album page's state machine (`FUN_002a6010`): the list widget's wrapping cursor, a confirm opens the picture (page 5),
/// -2 goes back to page 2, -4 closes.
pub fn page_album(key: Key, row: usize, n: usize) -> Act {
    match key {
        Key::Start => Act::Close,
        Key::Cancel => Act::Back,
        Key::Move(_, dy) if dy != 0 && n > 1 => Act::Cursor((row as i32 + dy.signum()).rem_euclid(n as i32) as usize),
        Key::Confirm if n > 0 => Act::Photo(row),
        _ => Act::None,
    }
}

/// The picture page's state machine (`FUN_002a6578`): L1 / R1 (-11 / -10) change the picture, -2 goes back to the list, -4 closes.
pub fn page_photo(key: Key) -> Act {
    match key {
        Key::Start => Act::Close,
        Key::Cancel => Act::Back,
        Key::L2 => Act::Browse(-1),
        Key::R2 => Act::Browse(1),
        _ => Act::None,
    }
}

/// The Treasures / Maps pages' state machine (`FUN_002afe98`): the four directions and L1 / R1 move the cursor, a cancel
/// goes back to page 2, Start closes the scene; a confirm does nothing.
pub fn page_lists(key: Key) -> Act {
    use lists::Nav;
    match key {
        Key::Start => Act::Close,
        Key::Cancel => Act::Back,
        Key::Move(_, dy) if dy < 0 => Act::Navigate(Nav::Up),
        Key::Move(_, dy) if dy > 0 => Act::Navigate(Nav::Down),
        Key::Move(dx, _) if dx < 0 => Act::Navigate(Nav::Left),
        Key::Move(dx, _) if dx > 0 => Act::Navigate(Nav::Right),
        Key::L1 => Act::Navigate(Nav::PageUp),
        Key::R1 => Act::Navigate(Nav::PageDown),
        _ => Act::None,
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
        Key::L1 | Key::R1 | Key::L2 | Key::R2 => Act::None,
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
    /// The scroll bar's four part groups, and the (new, new and seen) tag groups of page 2's rows, page 3's lines and
    /// page 0's cells.
    pub bar: [usize; 4],
    pub row_tag: [usize; 2],
    pub line_tag: [usize; 2],
    pub cell_tag: [usize; 2],
    pub face: FaceTables,
    /// The text objects' groups of the list pages (u16 at 0x381fe8: the info text's, then two more; the total's count is
    /// the third, `FUN_00278418(2)`).
    pub list_text: [usize; 3],
    /// The new / seen tag groups of the Quests page's rows (0x382ff8) and the Minigames page's (0x382fec).
    pub quest_tag: [usize; 2],
    pub game_tag: [usize; 2],
}

/// The executable's tables of the Characters / Character Links pages.
#[derive(Clone, Default, Debug, PartialEq, Eq)]
pub struct FaceTables {
    /// The frame group by style (u16 x 12: styles 0..5 of family 0, then of the links).
    pub style: [usize; 12],
    /// The face picture's (first, following) group for family 0 and for the links.
    pub pic: [[usize; 2]; 2],
    /// The new / seen tag groups: family 0, then the links.
    pub tag: [usize; 4],
    /// The hover frame's group on page 6 and on the others.
    pub hover: [usize; 2],
    /// The help line's message on page 6 and on the others.
    pub help: [u16; 2],
    /// The frame group of a character's own page by style 0..5.
    pub frame: [usize; 6],
    /// The face file name of each family (index 0..0x12; empty when the pointer is null).
    pub names: Vec<String>,
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
        bar: [0, 1, 2, 3].map(|k| exe.u16(addr::BAR + 2 * k).map_or(0, usize::from)),
        row_tag: [0, 1].map(|k| u32_at(exe, addr::ROW_TAG + 4 * k).map_or(0, |v| v as usize)),
        line_tag: [0, 1].map(|k| u32_at(exe, addr::LINE_TAG + 4 * k).map_or(0, |v| v as usize)),
        cell_tag: [0, 1].map(|k| u32_at(exe, addr::CELL_TAG + 4 * k).map_or(0, |v| v as usize)),
        list_text: [0, 1, 2].map(|k| exe.u16(addr::LIST_TEXT + 2 * k).map_or(0, usize::from)),
        quest_tag: [0, 1].map(|k| u32_at(exe, addr::QUEST_TAG + 4 * k).map_or(0, |v| v as usize)),
        game_tag: [0, 1].map(|k| u32_at(exe, addr::GAME_TAG + 4 * k).map_or(0, |v| v as usize)),
        face: FaceTables {
            style: std::array::from_fn(|k| exe.u16(addr::FACE_STYLE + 2 * k as u32).map_or(0, usize::from)),
            pic: [0u32, 6].map(|o| [exe.u16(addr::FACE_PIC + o).map_or(0, usize::from), exe.u16(addr::FACE_PIC + 4 + o).map_or(0, usize::from)]),
            tag: std::array::from_fn(|k| u32_at(exe, addr::FACE_TAG + 4 * k as u32).map_or(0, |v| v as usize)),
            hover: [0, 1].map(|k| u32_at(exe, addr::FACE_HOVER + 4 * k).map_or(0, |v| v as usize)),
            help: [0, 1].map(|k| u32_at(exe, addr::FACE_HELP + 4 * k).map_or(0, |v| v as u16)),
            frame: std::array::from_fn(|k| exe.u16(addr::CHAR_FRAME + 2 * k as u32).map_or(0, usize::from)),
            names: (0..19).map(|k| name_at(exe, addr::FACE_NAMES + 4 * k).unwrap_or_default()).collect(),
        },
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
    /// The scroll bar's four parts (`FUN_00299878`), hung from the layer that holds the track group.
    scroll: [usize; 4],
    /// The Characters / Character Links pages' own objects.
    chars: CharNodes,
    /// The Treasures / Maps pages' objects.
    lists: ListNodes,
    /// The Album page's objects.
    album: AlbumNodes,
    /// A character's own page (0xf).
    cpage: CharPageNodes,
    /// The Quests / Minigames pages' objects.
    games: games_page::Nodes,
}

/// The objects of a character's own page (`FUN_002a98f0`, `FUN_002aadf0`): the layer (group 0x49), the name (0x4a) and
/// sub-title (0x4b) texts, the two scroll arrows (0x4f, 0x50), the face frame and the face picture (children of the layer) and
/// the description's text row.
#[derive(Default, Clone, Copy)]
struct CharPageNodes {
    layer: usize,
    name: usize,
    sub: usize,
    arrows: [usize; 2],
    frame: usize,
    pic: usize,
    desc: usize,
}

impl CharPageNodes {
    fn all(&self) -> Vec<usize> {
        vec![self.layer, self.name, self.sub, self.arrows[0], self.arrows[1], self.frame, self.pic, self.desc]
    }
}

/// What a character's own page keeps: the characters of the page it was opened from (`FUN_002aa358`: the ids of the face
/// list), the one shown, the page to go back to, and the characters that exist now (`FUN_002a1fc0`).
#[derive(Default, Clone)]
struct CharPage {
    list: Vec<usize>,
    cur: usize,
    back: u16,
    present: std::collections::HashSet<usize>,
}

/// The objects of the Album page (`FUN_002a5a68`): the layer (group 0x58), eight rows and a new tag under each.
#[derive(Default, Clone, Copy)]
struct AlbumNodes {
    layer: usize,
    rows: [usize; album_page::ROWS],
    tags: [usize; album_page::ROWS],
    /// The picture page (5, `FUN_002a62d0`): its layer (group 0x54), the title (0x56), the caption (a list row of group 0x55)
    /// and the picture (sequence 1, a child of the layer).
    photo: [usize; 4],
}

impl AlbumNodes {
    fn all(&self) -> Vec<usize> {
        let mut v = vec![self.layer];
        v.extend(self.rows);
        v.extend(self.tags);
        v.extend(self.photo);
        v
    }
}

/// The objects of pages 7 and 8 (`FUN_002b0e78`): the two layers (0x3b and 0x43), the nine rows of eight cells, a label per
/// row (with its "nothing yet" text and its new tag), the total, the info text and the cursor frame.
#[derive(Default, Clone)]
struct ListNodes {
    layers: [usize; 2],
    cells: Vec<usize>,
    labels: Vec<[usize; 3]>,
    total: [usize; 2],
    info: usize,
    frame: usize,
}

impl ListNodes {
    fn all(&self) -> Vec<usize> {
        let mut v = self.layers.to_vec();
        v.extend(&self.cells);
        v.extend(self.labels.iter().flatten());
        v.extend(self.total);
        v.extend([self.info, self.frame]);
        v
    }
    fn clone_ids(&self) -> ListNodes {
        self.clone()
    }
}

/// The objects of pages 6 and 0xb (`FUN_002ae3e8`): the page's layer (group 0x51), the name plate (0x53), the pointer
/// (0x22), the hover frame, and a (frame, picture, tag) triple per face, made as the pages need them.
#[derive(Default)]
struct CharNodes {
    layer: usize,
    name: usize,
    pointer: usize,
    hover: usize,
    faces: Vec<[usize; 3]>,
}

impl CharNodes {
    fn all(&self) -> Vec<usize> {
        let mut v = vec![self.layer, self.name, self.pointer, self.hover];
        v.extend(self.faces.iter().flatten());
        v
    }
}

impl Nodes {
    fn page3(&self) -> Vec<usize> {
        let mut v = vec![self.tab, self.tab_follow, self.backdrop, self.story_row];
        v.extend(&self.story_lay);
        v.extend(&self.story_marks);
        v.extend(&self.scroll);
        v.extend(self.chars.all());
        v.extend(self.lists.all());
        v.extend(self.album.all());
        v.extend(self.cpage.all());
        v.extend(self.games.all());
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
    /// The journal's new / seen bits (`marks.rs`).
    marks: Marks,
    /// The `OverallChestIndex` numbers of the chests the player has opened (`Progress::chest_opened`), read when the journal opens.
    opened: std::collections::HashSet<i16>,
    /// `jiminy.bar` entries 5 ("diag", the face groups) and 3 ("char", the characters), parsed, and the face files already
    /// turned into textures by name.
    diag: Vec<u8>,
    groups: Vec<FaceGroup>,
    chars: Vec<CharRec>,
    face_tex: std::collections::HashMap<String, Vec<Option<usize>>>,
    /// The `trsr` and `item` rows of `03system.bin` (pages 7 and 8) and a cursor press the scene has not handled yet.
    tables_lists: treasure_page::Tables,
    nav: Option<lists::Nav>,
    /// The quest (entry 8) and minigame (entry 7) records, parsed, and what their tests read of the save.
    quests: Vec<games::Quest>,
    minis: Vec<games::Minigame>,
    snap: games_page::Snap,
    /// `jiminy.bar` entry 2 ("albu"), parsed, and the picture files already turned into textures by name.
    album: Vec<album_page::Rec>,
    photo_tex: std::collections::HashMap<String, Option<usize>>,
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
    /// Page 3: each entry's record (index in `jiminy.bar` entry 1) and line count, and the scroll bar.
    entries: Vec<(usize, usize)>,
    bar: Option<Bar>,
    /// Pages 6 and 0xb: the faces, the one under the pointer (kept for the way back), the pointer and its bounds.
    faces: Vec<Face>,
    sel: Option<usize>,
    pointer: [i32; 2],
    bounds: chars::Bounds,
    /// The half-width of a face's reach (`FUN_002af180`).
    half: i32,
    /// Pages 7 and 8: the layout and the cursor; the scroll bar's track group.
    lists: Option<(lists::Layout, lists::Cursor)>,
    bar_group: usize,
    /// Page 4: the album record behind each row, and the row the picture page (5) kept for the way back.
    album_rows: Vec<usize>,
    back_album: usize,
    /// Whether each Album row's picture exists (its flag is set; the rows of unset flags show only with the all-worlds flag).
    album_exist: Vec<bool>,
    /// A character's own page (0xf), and the face the face page it came from puts the pointer on again (`FUN_002ae3e8(face)`).
    cp: CharPage,
    back_face: Option<usize>,
    /// Pages 9 and 10: what each row of the list shows.
    gentries: Vec<games_page::Entry>,
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
    j.diag = entry(5);
    j.groups = chars::parse_groups(&j.diag);
    j.chars = chars::parse_chars(&entry(3));
    j.tables_lists = treasure_page::load_tables(disc);
    j.album = album_page::parse(&entry(2));
    j.quests = games::parse_quests(&j.ques);
    j.minis = games::parse_games(&j.mini);
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
    // The Treasures / Maps pages (`FUN_002b0e78`): before the cursor and the hints, which draw over them.
    {
        let layers = [add(scene, list, tex, Some(header)), add(scene, list, tex, Some(header))];
        let cells: Vec<usize> = (0..lists::VIEW_ROWS * lists::COLS).map(|_| add(scene, list, tex, Some(layers[0]))).collect();
        let labels: Vec<[usize; 3]> = (0..lists::VIEW_ROWS)
            .map(|_| {
                let label = add(scene, list, tex, Some(layers[0]));
                [label, add(scene, list, tex, Some(label)), add(scene, list, tex, Some(label))]
            })
            .collect();
        let total_label = add(scene, list, tex, Some(layers[0]));
        let total = [total_label, add(scene, list, tex, Some(total_label))];
        let info = add(scene, list, tex, Some(layers[0]));
        let frame = add(scene, list, tex, Some(layers[0]));
        nodes.lists = ListNodes { layers, cells, labels, total, info, frame };
        let layer = add(scene, list, tex, Some(header));
        let rows: Vec<usize> = (0..album_page::ROWS).map(|_| add(scene, list, tex, Some(layer))).collect();
        let tags: Vec<usize> = rows.iter().map(|&r| add(scene, list, tex, Some(r))).collect();
        let p_layer = add(scene, list, tex, Some(header));
        let p_title = add(scene, list, tex, Some(header));
        let p_caption = add(scene, list, tex, Some(p_layer));
        let p_pic = add(scene, j.seq_base + 1, tex, Some(p_layer));
        nodes.album = AlbumNodes {
            layer,
            rows: rows.try_into().unwrap_or_default(),
            tags: tags.try_into().unwrap_or_default(),
            photo: [p_layer, p_title, p_caption, p_pic],
        };
    }
    // The Quests / Minigames pages (`FUN_002b5e50`, `FUN_002b4b18`): two layers, ten rows with a tag and a value text each. Before the
    // scroll bar's parts, which draw over the second layer's track.
    {
        let layers = [add(scene, list, tex, Some(header)), add(scene, list, tex, Some(header))];
        let mut g = games_page::Nodes { layers, ..games_page::Nodes::default() };
        for k in 0..games_page::ROWS {
            g.rows[k] = add(scene, list, tex, Some(layers[0]));
            g.values[k] = add(scene, list, tex, Some(g.rows[k]));
            g.tags[k] = add(scene, list, tex, Some(g.rows[k]));
        }
        nodes.games = g;
    }
    // The help line (a child of the plate, `FUN_00278468`), the hints (roots), the cursor and its follower (moved
    // under the cursor row or cell when shown: those all have lower indices).
    nodes.help = add(scene, list, tex, Some(plate));
    nodes.hints = [add(scene, list, tex, None), add(scene, list, tex, None)];
    nodes.cursor = add(scene, list, tex, None);
    nodes.follow = add(scene, list, tex, None);
    // The scroll bar's parts: children of page 3's second layer (`FUN_00299878(.., parent 0x62 layer)`).
    for k in 0..4 {
        nodes.scroll[k] = add(scene, list, tex, Some(nodes.story_lay[1]));
    }
    // The face pages (`FUN_002ae3e8`): layer and name plate under the header, the pointer and the hover frame free.
    // The pool of faces (the most one page shows: the links' 36) goes before the pointer and the hover frame, which draw over
    // the faces.
    let (layer, name) = (add(scene, list, tex, Some(header)), add(scene, list, tex, Some(header)));
    let seq_face = j.seq_base + SEQ_FACE;
    let faces: Vec<[usize; 3]> = (0..FACE_POOL)
        .map(|_| {
            let frame = add(scene, list, tex, Some(layer));
            [frame, add(scene, seq_face, tex, Some(frame)), add(scene, list, tex, Some(frame))]
        })
        .collect();
    nodes.chars = CharNodes { layer, name, pointer: add(scene, list, tex, None), hover: add(scene, list, tex, None), faces };
    // A character's own page (`FUN_002a98f0`): the layer, the frame and the picture under it, the texts and arrows under the header.
    {
        let layer = add(scene, list, tex, Some(header));
        let frame = add(scene, list, tex, Some(layer));
        let pic = add(scene, seq_face, tex, Some(layer));
        let desc = add(scene, list, tex, Some(layer));
        let (name, sub) = (add(scene, list, tex, Some(header)), add(scene, list, tex, Some(header)));
        let arrows = [add(scene, list, tex, Some(header)), add(scene, list, tex, Some(header))];
        nodes.cpage = CharPageNodes { layer, name, sub, arrows, frame, pic, desc };
    }
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
    } else if keys.just_pressed(KeyCode::BracketLeft) || keys.just_pressed(KeyCode::KeyZ) {
        Some(Key::L2)
    } else if keys.just_pressed(KeyCode::BracketRight) || keys.just_pressed(KeyCode::KeyC) {
        Some(Key::R2)
    } else {
        None
    };
    for k in [KeyCode::KeyJ, KeyCode::Escape, KeyCode::Backspace, KeyCode::KeyK, KeyCode::Enter, KeyCode::NumpadEnter, KeyCode::ArrowLeft, KeyCode::ArrowRight, KeyCode::ArrowUp, KeyCode::ArrowDown, KeyCode::KeyQ, KeyCode::KeyE, KeyCode::PageUp, KeyCode::PageDown, KeyCode::BracketLeft, KeyCode::BracketRight, KeyCode::KeyZ, KeyCode::KeyC] {
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
        (6 | 0xb, _) => page_faces(key, o.sel),
        (0xf, _) => page_char(key, o.top, o.count, o.cp.list.iter().filter(|c| o.cp.present.contains(c)).count() > 1),
        (7 | 8, _) => page_lists(key),
        (9 | 10, _) => page3(key, o.top, o.count),
        (4, _) => page_album(key, o.row, o.album_rows.len()),
        (5, _) => page_photo(key),
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
            story_view(&mut j, &mut scene);
            menu_sound::play(&mut se, menu_sound::CURSOR);
            info!("journal: page 3 first line {t}");
        }
        Act::Cursor(t) if matches!(page, 9 | 10) => {
            j.open.as_mut().unwrap().top = t;
            games_page::view(&mut j, &mut scene);
            menu_sound::play(&mut se, menu_sound::CURSOR);
            info!("journal: page {page} first row {t}");
        }
        Act::Cursor(t) if page == 0xf => {
            j.open.as_mut().unwrap().top = t;
            char_view(&mut j, &mut scene);
            menu_sound::play(&mut se, menu_sound::CURSOR);
            info!("journal: page 0xf description position {t}");
        }
        Act::Neighbour(next) => {
            let o = j.open.as_mut().unwrap();
            let present = o.cp.present.clone();
            let to = chars::neighbour(&o.cp.list, o.cp.cur, &|c| present.contains(&c), next);
            if to != o.cp.cur {
                o.cp.cur = to;
                menu_sound::play(&mut se, menu_sound::CURSOR);
                info!("journal: page 0xf {} (FUN_002aa600) -> character {}", if next { "R2 (-10)" } else { "L2 (-11)" }, o.cp.list[to]);
                j.goto = Some((0xf, world_byte));
            }
        }
        Act::Cursor(r) if page == 2 => {
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
        Act::Navigate(nav) => j.nav = Some(nav),
        Act::Photo(r) => {
            menu_sound::play(&mut se, crate::menu_status::CONFIRM);
            info!("journal: page 4 confirm on row {r} -> page 5, the picture (FUN_002a6010 -> FUN_002a62d0)");
            j.open.as_mut().unwrap().row = r;
            j.goto = Some((5, world_byte));
        }
        Act::Browse(dir) => {
            let o = j.open.as_mut().unwrap();
            let to = album_page::browse(&o.album_exist, o.row, dir);
            if to != o.row {
                o.row = to;
                menu_sound::play(&mut se, menu_sound::CURSOR);
                info!("journal: page 5 picture {to} (FUN_002a6850)");
                j.goto = Some((5, world_byte));
            }
        }
        Act::Back if page == 5 => {
            menu_sound::play(&mut se, menu_sound::CANCEL);
            info!("journal: page 5 cancel -> page 4 (FUN_002a6578 case -2)");
            let o = j.open.as_mut().unwrap();
            o.back_album = o.row;
            j.goto = Some((4, world_byte));
        }
        Act::Cursor(r) if page == 4 => {
            j.open.as_mut().unwrap().row = r;
            place_cursor(&j, &mut scene);
            album_page::seen_row(&mut j, &mut scene);
            menu_sound::play(&mut se, menu_sound::CURSOR);
            info!("journal: page 4 row {r}");
        }
        Act::Cursor(_) => {}
        Act::Category { cat: 0 | 2..=7, page: to @ (4 | 6..=10 | 0xb) } => {
            menu_sound::play(&mut se, crate::menu_status::CONFIRM);
            info!("journal: page 2 row -> page {to}, the {} (FUN_002a3da0 -> FUN_002ae3e8 / FUN_002afca0)", match to { 4 => "album", 6 => "characters", 7 => "treasures", 8 => "maps", 9 => "quests", 10 => "minigames", _ => "character links" });
            let o = j.open.as_mut().unwrap();
            o.back_row = o.row;
            j.goto = Some((to, world_byte));
        }
        Act::Face(Some(s)) => {
            menu_sound::play(&mut se, crate::menu_status::CONFIRM);
            let o = j.open.as_mut().unwrap();
            let list: Vec<usize> = o.faces.iter().map(|f| f.ch).collect();
            // `FUN_002ae790` marks the character seen (`FUN_002b73b8`) and `FUN_002aa358` keeps the face list for the new page.
            let ch = list[s];
            o.cp = CharPage { list, cur: s, back: page, present: Default::default() };
            j.marks.see(Cat::Char, ch);
            info!("journal: page {page:#x} confirm on face {s} (character {ch}) -> page 0xf (FUN_002ae790 -> FUN_002aa358)");
            j.goto = Some((0xf, world_byte));
        }
        Act::Face(None) => {
            menu_sound::play(&mut se, menu_sound::REFUSED);
            info!("journal: page {page:#x} confirm with nothing under the pointer (sound 4, FUN_002ae790)");
        }
        Act::Back if page == 0xf => {
            menu_sound::play(&mut se, menu_sound::CANCEL);
            let o = j.open.as_mut().unwrap();
            let (back, cur) = (o.cp.back, o.cp.cur);
            o.back_face = Some(cur);
            info!("journal: page 0xf cancel -> page {back:#x} with face {cur} (FUN_002a9b80 case -2 -> FUN_002a9590 -> FUN_002ae3e8)");
            j.goto = Some((back, world_byte));
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
        Act::Back if matches!(page, 3 | 4 | 6..=10 | 0xb) => {
            menu_sound::play(&mut se, menu_sound::CANCEL);
            info!("journal: page {page:#x} cancel -> page 2 (FUN_002a5218 / FUN_002ae790 case -2)");
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
#[allow(clippy::too_many_arguments)]
fn show(j: &mut Journal, scene: &mut MenuScene, disc: &Disc, images: &mut Assets<Image>, mode: i32, world_number: Option<usize>, test: &dyn Fn(u16) -> bool, items: &dyn Fn(u16) -> i32) {
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
    // `FUN_002b7100` (task 0x276338, once per scene open): the new bits of every category the app has data for.
    let mut present = story_present(&j.stor, test);
    present.extend(chars::present(&j.diag, &j.groups, test, items).into_iter().map(|c| (Cat::Char, c)));
    {
        let wi = |w: u8| j.world_index.get(usize::from(w)).copied().unwrap_or(-1);
        let open = |w: usize| j.worlds.iter().find(|r| usize::try_from(r.id).is_ok_and(|id| id == w)).is_some_and(|r| test(r.flag));
        let is_map = |i: u16| j.tables_lists.items.get(&i).is_some_and(|r| r.kind == treasure_page::MAP_KIND);
        present.extend(album_page::present(&j.album, test));
        present.extend(treasure_page::treasures_present(&j.tables_lists.rows, &wi, test, &open));
        present.extend(treasure_page::maps_present(&j.tables_lists.rows, &is_map, &|i| items(i) > 0));
    }
    {
        let (quests, games) = games_page::with_game(j, test, |g| (games::quest_present(&j.quests, g), games::game_present(&j.minis, g)));
        present.extend(quests);
        present.extend(games);
    }
    j.marks.snapshot(present);
    let n = &j.nodes;
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
        entries: Vec::new(),
        bar: None,
        faces: Vec::new(),
        sel: None,
        pointer: [0; 2],
        bounds: [0; 4],
        half: 32,
        lists: None,
        bar_group: STORY_LAYERS[1],
        album_rows: Vec::new(),
        back_album: 0,
        album_exist: Vec::new(),
        cp: CharPage::default(),
        back_face: None,
        gentries: Vec::new(),
    });
    build(j, scene, disc, images, page, world, test, items);
}

/// Builds `page` (0, 1 or 2) after hiding the other page's nodes (`FUN_002a3da0`'s switch, `FUN_00276740`).
#[allow(clippy::too_many_arguments)]
fn build(j: &mut Journal, scene: &mut MenuScene, disc: &Disc, images: &mut Assets<Image>, page: u16, world: i8, test: &dyn Fn(u16) -> bool, items: &dyn Fn(u16) -> i32) {
    // Page 3 keeps the title of page 2 (the category teardown `FUN_00277ec0` frees the `DAT_0036d100` objects
    // only; the title is `DAT_0036d0f0 + 0x350`).
    let title = j.nodes.title;
    let n = &j.nodes;
    let ids: Vec<usize> = n.page0().into_iter().chain(n.page12()).chain(n.page3()).chain([n.help, n.cursor, n.follow]).collect();
    for id in ids {
        if !(matches!(page, 3..=11 | 0xf) && id == title) {
            scene.nodes[id].visible = false;
        }
    }
    let opened = j.opened.clone();
    match page {
        1 | 2 => build12(j, scene, disc, images, page, world, test, items),
        3 => build3(j, scene, world, test),
        6 | 0xb => build_faces(j, scene, disc, images, page, world, test, items),
        0xf => build_char_page(j, scene, disc, images, page, world, test, items),
        9 | 10 => games_page::build(j, scene, page, world, test),
        7 | 8 => treasure_page::build(j, scene, page, world, &treasure_page::Game { flag: test, items, opened: &|c| opened.contains(&c) }),
        4 => album_page::build(j, scene, world, test),
        5 => album_page::build_photo(j, scene, disc, images, world, test),
        _ => build0(j, scene, world, test, items),
    }
    chrome(j, scene, page);
}

fn build0(j: &mut Journal, scene: &mut MenuScene, world: i8, test: &dyn Fn(u16) -> bool, items: &dyn Fn(u16) -> i32) {
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
        // The two children start in group 1; an open world's second child shows the new tag of any category (0x98 until
        // everything new in it has been seen, then 0x99: `FUN_002b9658` / `FUN_002b9798`). The first child's 0x9a is the
        // completion mark (`FUN_002baf08`: every category complete; world 0xf also needs the limits list, a gap).
        scene.set_group(m[0], group::START);
        scene.set_group(m[1], group::START);
        if shown[k].1 != LOCKED_ICON
            && let Some(all_seen) = world_aggregate(j, j.worlds[shown[k].0].id, test, items)
        {
            scene.set_group(m[1], j.tables.cell_tag[usize::from(all_seen)]);
        }
        if shown[k].1 != LOCKED_ICON && world_complete(j, j.worlds[shown[k].0].id, test, items) {
            scene.set_group(m[0], group::CELL_DONE);
        }
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

#[allow(clippy::too_many_arguments)]
fn build12(j: &mut Journal, scene: &mut MenuScene, disc: &Disc, images: &mut Assets<Image>, page: u16, world: i8, test: &dyn Fn(u16) -> bool, items: &dyn Fn(u16) -> i32) {
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
        // The holders' texts start at the object's position (`FUN_00138f38`: the group's animated anchor) plus the
        // group's x / y (flag 0x10 kept, `FUN_00139048`): inside the pane, not at the screen's corner (WP-MN12).
        scene.nodes[id].label_anchor = (go_offset(k)).then_some([0.0; 2]);
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
        // The quest row needs a quest of the world that is listed (`FUN_002b6f00`), the minigame row a game of it that has been
        // played (`FUN_002b5bb0`); the all-worlds flag lists them all.
        let (quests, games) = games_page::with_game(j, test, |g| (games::quests_exist(&j.quests, Some(world), g), games::games_exist(&j.minis, Some(world), g)));
        categories(world, quests, games)
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
    // The rows' tags (`FUN_002a39b8`): the second child shows the new tag, the first the completion mark 0x8a.
    if page == 2 {
        for (k, &c) in cats.iter().enumerate() {
            let (new, done) = row_marks(j, c, world, test, items);
            if let Some(all_seen) = new {
                scene.set_group(kids[k][1], j.tables.row_tag[usize::from(all_seen)]);
            }
            if done {
                scene.set_group(kids[k][0], ROW_DONE);
            }
        }
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

/// The layer objects of pages 1 and 2 whose text takes the group's x / y (the two story-text holders, 2 and 4).
fn go_offset(k: usize) -> bool {
    k == 2 || k == 4
}

/// The colour of a text object whose flag 8 was cleared (`FUN_00139860`): `FUN_00139048` then takes the whole colour from its
/// sprite group (`FUN_00237518`) instead of replacing the colour's RGB with the object's own (0xe5e5e5, `FUN_00138fc8`). That
/// colour is the one `FUN_00236cf8` computes with the group's anchor, from the animation flagged 2 that covers the frame; the
/// port takes the steady state, the end colour of the last such animation (a fade-in is not followed). RGBA with 0x80 = 1.0, as
/// the text styles take them.
fn group_color(scene: &MenuScene, seq: usize, group: usize) -> [u8; 4] {
    let Some(s) = scene.seqs.get(seq) else { return DEFAULT_TEXT };
    let Some(g) = s.groups.get(group) else { return DEFAULT_TEXT };
    s.anims.iter().skip(g.first).take(g.count).filter(|a| a.flags & 2 != 0).max_by_key(|a| a.frame_end).map_or(DEFAULT_TEXT, |a| a.color_end)
}

/// The default window text colour (0xe5e5e5, alpha 0x80).
const DEFAULT_TEXT: [u8; 4] = [0xe5, 0xe5, 0xe5, 0x80];

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
        6 | 0xb => Some(j.tables.face.help[usize::from(page != 6)]),
        0xf => Some(msg::HELP_CHAR),
        9 => Some(games_page::HELP_QUEST),
        10 => Some(games_page::HELP_GAME),
        4 => Some(album_page::HELP),
        5 => Some(album_page::HELP_PHOTO),
        7 => Some(msg::HELP_TREASURE),
        8 => Some(msg::HELP_MAP),
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
        // The Treasures / Maps pages' hand sits on their cursor frame (`FUN_002b16e0`: `FUN_00277158(0, frame)`).
        7 | 8 => Some(n.lists.frame),
        4 => n.album.rows.get(o.row).copied().filter(|_| !o.album_rows.is_empty()),
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
/// What every category page of a world puts up before its own objects: the tab of plate 0 with the category's name (the
/// confirm on page 2's row put it there, `FUN_0029d820(0, 0x382b88[cat])`, then `FUN_0029d1d0(0, -1, 0)` made the tab) with
/// its follower (`FUN_0029af00(.., 0x381fc8)`), and the backdrop 0x755a (`FUN_00278658`: page family 1, set by page 2's
/// build with `FUN_00277d60(1)`).
fn category_frame(j: &Journal, scene: &mut MenuScene, cat: usize) {
    let n = &j.nodes;
    let (tab, tab_follow, backdrop) = (n.tab, n.tab_follow, n.backdrop);
    let (plate, follow) = (j.tables.plate, j.tables.follow);
    scene.nodes[tab].visible = true;
    scene.nodes[tab].pos = [0.0; 2];
    scene.set_group_then(tab, plate[0], plate[2]);
    let name = tokens(j, scene, j.tables.row_text[cat]);
    scene.nodes[tab].label = name.and_then(|t| label_of(scene, tab, plate[2], t, 0, false));
    scene.nodes[tab].label_anchor = Some([0.0; 2]);
    scene.nodes[tab_follow].visible = true;
    scene.set_group_then(tab_follow, follow[0], follow[2]);
    scene.nodes[backdrop].visible = true;
    scene.set_group(backdrop, j.tables.backdrop[1]);
}

fn build3(j: &mut Journal, scene: &mut MenuScene, world: i8, test: &dyn Fn(u16) -> bool) {
    let n = &j.nodes;
    let (lay, row, marks) = (n.story_lay.clone(), n.story_row, n.story_marks.clone());
    category_frame(j, scene, 1);
    for (k, &id) in lay.iter().enumerate() {
        scene.nodes[id].visible = true;
        scene.nodes[id].off = [0.0; 2];
        scene.set_group(id, STORY_LAYERS[k]);
    }
    let list = j.seq_base + SEQ_LIST;
    let g = scene.seqs.get(list).and_then(|s| s.groups.get(STORY_LAYERS[0])).copied().unwrap_or_default();
    let size = if g.size > 0 { g.size } else { TEXT_SIZE as i32 };
    let font_gap = kh2_image::font::Font::get(STORY_FONT).line_gap as i32;
    let (line_h, gap) = story_metrics(g.unk1c, size, font_gap);
    for (k, &id) in marks.iter().enumerate() {
        scene.nodes[id].visible = true;
        scene.nodes[id].off = [0.0, (k as i32 * g.unk1c) as f32];
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
    // The list opens on the last entry's first line (`FUN_002a4c38`: the scroll offset is set to the lines of every entry
    // but the last, `FUN_00298d98(list, lines)`; the scroll bar `FUN_00299c10(bar, lines)` follows).
    let before_last = counts.split_last().map_or(0, |(_, rest)| rest.iter().sum::<usize>());
    let last = counts.last().copied().unwrap_or(0);
    let bar_total = if last > STORY_LINES - 1 { before_last + last } else { before_last + STORY_LINES };
    scene.nodes[row].visible = true;
    scene.nodes[row].off = [0.0; 2];
    scene.set_group(row, STORY_ROW);
    info!(
        "journal: page 3 (FUN_002a4c38): world {world}, {} story entries {:x?}, {} lines, list count {count}, line height {line_h}, gap {gap}",
        entries.len(),
        entries.iter().map(|e| e.0).collect::<Vec<_>>(),
        lines.len()
    );
    let track = scene.seqs.get(list).and_then(|s| s.groups.get(STORY_LAYERS[1])).copied().unwrap_or_default();
    let body = scene.seqs.get(list).and_then(|s| s.groups.get(j.tables.bar[2])).map_or(1, |g| g.size.max(1));
    let bar = Bar::new(track.size, body, bar_total as i32, STORY_LINES as i32);
    let o = j.open.as_mut().unwrap();
    (o.page, o.world, o.top, o.count, o.lines, o.gap, o.size) = (3, world, before_last, count, lines, gap.min(255) as u8, size as f32);
    o.entries = entries.iter().map(|e| e.1).zip(counts).collect();
    o.bar = Some(bar);
    let bar_parent = j.nodes.story_lay[1];
    place_bar(j, scene, bar_parent, STORY_LAYERS[1]);
    story_view(j, scene);
}

/// The scroll bar's four parts (`FUN_00299878`): the groups of the bar table, hung from the layer that holds the track
/// group `track_group` (the parts sit at that group's x / y), the body's y scale from the share of the list shown.
fn place_bar(j: &mut Journal, scene: &mut MenuScene, parent: usize, track_group: usize) {
    let Some(bar) = j.open.as_ref().and_then(|o| o.bar) else { return };
    let g = scene.seqs.get(j.seq_base + SEQ_LIST).and_then(|s| s.groups.get(track_group)).copied().unwrap_or_default();
    if let Some(o) = j.open.as_mut() {
        o.bar_group = track_group;
    }
    for k in 0..4 {
        let id = j.nodes.scroll[k];
        scene.nodes[id].parent = Some(parent);
        scene.nodes[id].visible = true;
        scene.nodes[id].ext = kh2_image::seqd::Ext::default();
        if k == 2 {
            scene.nodes[id].ext.scale[2] = bar.body_scale();
        }
        // The hold group is the same group again (`FUN_0029c2d8` with the third set of the table): it loops.
        scene.set_group_then(id, j.tables.bar[k], j.tables.bar[k]);
        scene.nodes[id].off = [g.x as f32, g.y as f32];
    }
}

/// `FUN_00299c10(bar, pos)`: the thumb parts at list line `pos`.
fn bar_pos(j: &Journal, scene: &mut MenuScene, pos: usize) {
    let Some(o) = j.open.as_ref() else { return };
    let Some(bar) = o.bar else { return };
    let g = scene.seqs.get(j.seq_base + SEQ_LIST).and_then(|s| s.groups.get(o.bar_group)).copied().unwrap_or_default();
    let y = bar.offsets(pos as i32);
    for (k, dy) in y.into_iter().enumerate() {
        scene.nodes[j.nodes.scroll[k + 1]].off = [g.x as f32, (g.y + dy) as f32];
    }
}

/// Page 3's marks (`FUN_002a5708`): the new / seen tag of every entry that starts in view; an entry whose first line is
/// the top row is marked seen first (`FUN_002b95d8`).
fn story_marks(j: &mut Journal, scene: &mut MenuScene) {
    let Some(o) = j.open.as_ref() else { return };
    let (top, entries) = (o.top, o.entries.clone());
    for &id in &j.nodes.story_marks {
        scene.set_group(id, STORY_ROW);
    }
    let lines: Vec<usize> = entries.iter().map(|e| e.1).collect();
    for (row, e) in marks::story_mark_rows(&lines, top, STORY_LINES) {
        let key = entries[e].0;
        if row == 0 {
            j.marks.see(Cat::Story, key);
        }
        if let Some(tag) = j.marks.tag(Cat::Story, key) {
            let g = j.tables.line_tag[usize::from(tag == Tag::Seen)];
            scene.set_group(j.nodes.story_marks[row], g);
        }
    }
}

/// Everything that follows page 3's first line: the text, the thumb and the marks.
fn story_view(j: &mut Journal, scene: &mut MenuScene) {
    let top = j.open.as_ref().map_or(0, |o| o.top);
    story_text(j, scene);
    bar_pos(j, scene, top);
    story_marks(j, scene);
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

/// The face pages' literal groups of `jmemo.2ld` sequence 5 (`FUN_002ae3e8`): the layer 0x51, the name plate 0x53 (its text
/// is the face's name), the pointer 0x22 (`FUN_002aebb8`: object 0x7571) and the bounds object 0x52 (`FUN_002ae790`).
const FACE_LAYER: usize = 0x51;
const FACE_BOUNDS: usize = 0x52;
const FACE_NAME: usize = 0x53;
const FACE_POINTER: usize = 0x22;
/// The sequence of the face pictures (`FUN_002af800`: `FUN_00239020(.., 0)`).
const SEQ_FACE: usize = 0;
/// Faces a page can show: the Character Links family has 36 drawn sub-records (`jiminy.bar` entry 5).
const FACE_POOL: usize = 40;
/// A character's own page (0xf): the layer, name, sub-title and scroll-arrow groups of the list sequence (`FUN_002a98f0`,
/// `FUN_002aadf0`).
const CHAR_LAYER: usize = 0x49;
const CHAR_NAME: usize = 0x4a;
const CHAR_SUB: usize = 0x4b;
const CHAR_ARROWS: [usize; 2] = [0x4f, 0x50];
/// The name's font (`FUN_002ae3e8`: `+0x644 = 3`).
const NAME_FONT: usize = 3;

/// The text of a world's name for a message that holds a variable (`FUN_002a3070(world, 1)`: record +6, or +0xc once the
/// second flag is set).
fn world_name(j: &Journal, scene: &MenuScene, world: i8, test: &dyn Fn(u16) -> bool) -> Vec<Token> {
    let Some(w) = j.worlds.iter().find(|w| w.id == world).or(j.worlds.first()) else { return Vec::new() };
    let id = if w.flag2 != 0 && test(w.flag2) { w.names[3] } else { w.names[1] };
    tokens(j, scene, id).map(|t| t.into_iter().filter(|t| *t != Token::End).collect()).unwrap_or_default()
}

/// Characters (page 6) and Character Links (page 0xb): the faces of the family, the pointer, the name plate.
#[allow(clippy::too_many_arguments)]
fn build_faces(j: &mut Journal, scene: &mut MenuScene, disc: &Disc, images: &mut Assets<Image>, page: u16, world: i8, test: &dyn Fn(u16) -> bool, items: &dyn Fn(u16) -> i32) {
    let links = page != 6;
    let family: u8 = if links { LINKS } else { world as u8 };
    category_frame(j, scene, if links { 7 } else { 0 });
    let list = j.seq_base + SEQ_LIST;
    let (layer, plate, pointer, hover) = (j.nodes.chars.layer, j.nodes.chars.name, j.nodes.chars.pointer, j.nodes.chars.hover);
    for (id, g) in [(layer, FACE_LAYER), (plate, FACE_NAME)] {
        scene.nodes[id].visible = true;
        scene.nodes[id].off = [0.0; 2];
        scene.set_group(id, g);
    }
    scene.nodes[plate].label = None;
    // The faces (`FUN_002af398`) and their pictures (`FUN_002af6c0` / `FUN_002af800`, the file `jmface_<name>.2ld`).
    let faces = chars::faces(&j.diag, &j.groups, family, test, items);
    let file = j.tables.face.names.get(usize::from(family)).cloned().unwrap_or_default();
    if !file.is_empty() && !j.face_tex.contains_key(&file) {
        let t = bar_pictures(disc, &format!("menu/us/jmface_{file}.2ld"), scene, images);
        j.face_tex.insert(file.clone(), t);
    }
    let pics = j.face_tex.get(&file).cloned().unwrap_or_default();
    let faces: Vec<Face> = faces.into_iter().take(FACE_POOL).collect();
    let ft = j.tables.face.clone();
    let mut missing = 0;
    for (k, &[frame, pic, tag]) in j.nodes.chars.faces.clone().iter().enumerate() {
        let on = k < faces.len();
        for id in [frame, pic, tag] {
            scene.nodes[id].visible = on;
        }
        if !on {
            continue;
        }
        let f = faces[k];
        let rec = j.chars.get(f.ch).copied().unwrap_or_default();
        scene.nodes[frame].off = [f.x as f32, f.y as f32];
        scene.set_group(frame, ft.style[usize::from(links) * 6 + usize::from(rec.style).min(5)]);
        // The picture: `FUN_00138948(.., group, 0)` with the follow-up group in +0x17c.
        let [first, then] = ft.pic[usize::from(links)];
        match pics.get(usize::from(rec.face)).copied().flatten() {
            Some(t) => {
                scene.nodes[pic].tex = t;
                scene.nodes[pic].off = [0.0; 2];
                scene.set_group_then(pic, first, then);
            }
            None => {
                scene.nodes[pic].visible = false;
                missing += 1;
            }
        }
        // The tag: group 2 (blank) unless the character has a new bit (`FUN_002b7208` / `FUN_002b72c0`).
        scene.set_group(tag, STORY_ROW);
        if let Some(g) = chars::tag_group(ft.tag, links, j.marks.tag(Cat::Char, f.ch)) {
            scene.set_group(tag, g);
        }
    }
    // The pointer's bounds: the position of the bounds object's group 0x52 (its anchor) to the group's x / y
    // (`FUN_002ae790`: +0x48 / +0x4c from `FUN_00138f20`, +0x4a / +0x4e from `FUN_00237618` / `FUN_00237638`).
    let g52 = scene.seqs.get(list).and_then(|s| s.groups.get(FACE_BOUNDS)).copied().unwrap_or_default();
    let anchor = scene.seqs.get(list).and_then(|s| s.anchor(FACE_BOUNDS, 0, &kh2_image::seqd::Ext::default())).unwrap_or([0, 0]);
    let bounds = [anchor[0], g52.x, anchor[1], g52.y];
    let half = faces
        .first()
        .and_then(|f| j.chars.get(f.ch))
        .and_then(|r| scene.seqs.get(list).and_then(|s| s.groups.get(ft.style[usize::from(links) * 6 + usize::from(r.style).min(5)])))
        .map_or(32, |g| g.unk20 / 2);
    let want = j.open.as_mut().and_then(|o| o.back_face.take());
    let start = chars::first_pointer(&faces, want, bounds);
    scene.nodes[pointer].visible = true;
    scene.nodes[pointer].parent = None;
    scene.nodes[pointer].pos = [start[0] as f32, start[1] as f32];
    scene.set_group(pointer, FACE_POINTER);
    scene.nodes[hover].visible = false;
    info!(
        "journal: page {page:#x} (FUN_002ae3e8): family {family:#x}, {} faces, file jmface_{file}.2ld {} pictures ({missing} faces without one), bounds {bounds:?}, half {half}, pointer {start:?}",
        faces.len(),
        pics.len()
    );
    let o = j.open.as_mut().unwrap();
    (o.page, o.world, o.faces, o.sel, o.pointer, o.bounds, o.half) = (page, world, faces, None, start, bounds, half);
    faces_update(j, scene, test, true);
}

/// A character's own page (0xf: `FUN_002aa358` -> `FUN_002a98f0` -> `FUN_002aadf0`, and the picture `FUN_002ab8f0`): the layer
/// (group 0x49) with the face frame (the table 0x382ea0 by the record's style) and the face picture under it, the name (0x4a)
/// and sub-title (0x4b) texts, the description in a scrolled view of seven lines, and the two scroll arrows (0x4f, 0x50).
/// The counter row (group 0x4c, records with flag 0x800: how many of an enemy were defeated), the four drop rows of pages 0x12 /
/// 0x15 and the 3D model (`FUN_002abf60`) are not drawn.
#[allow(clippy::too_many_arguments)]
fn build_char_page(j: &mut Journal, scene: &mut MenuScene, disc: &Disc, images: &mut Assets<Image>, page: u16, world: i8, test: &dyn Fn(u16) -> bool, items: &dyn Fn(u16) -> i32) {
    let Some(o) = j.open.as_ref() else { return };
    let (back, cur) = (o.cp.back, o.cp.cur);
    let links = back != 6;
    let family: u8 = if links { LINKS } else { world as u8 };
    category_frame(j, scene, if links { 7 } else { 0 });
    let list = j.seq_base + SEQ_LIST;
    let present: std::collections::HashSet<usize> = chars::present(&j.diag, &j.groups, test, items).into_iter().collect();
    let ch = j.open.as_ref().and_then(|o| o.cp.list.get(cur).copied()).unwrap_or(0);
    let rec = j.chars.get(ch).copied().unwrap_or_default();
    // `FUN_002ac4c8`: a character that exists is marked seen when its page comes up.
    if present.contains(&ch) {
        j.marks.see(Cat::Char, ch);
    }
    let n = j.nodes.cpage;
    let size_of = |scene: &MenuScene, g: usize| scene.seqs.get(list).and_then(|s| s.groups.get(g)).map_or(0, |g| g.size);
    let g_layer = scene.seqs.get(list).and_then(|s| s.groups.get(CHAR_LAYER)).copied().unwrap_or_default();
    // `FUN_00237618` / `FUN_00237638` read the group's x / y (as for the face pages' bounds).
    let anchor = [g_layer.x, g_layer.y];
    let (name_size, sub_size) = (size_of(scene, CHAR_NAME), size_of(scene, CHAR_SUB));
    for (id, g) in [(n.layer, CHAR_LAYER), (n.name, CHAR_NAME)] {
        scene.nodes[id].visible = true;
        scene.nodes[id].off = [0.0; 2];
        scene.set_group(id, g);
    }
    // The face frame and the picture hang from the layer at its group's x / y (`FUN_00138ed0(.., FUN_00237618, FUN_00237638)`).
    let ft = j.tables.face.clone();
    scene.nodes[n.frame].visible = true;
    scene.nodes[n.frame].off = [anchor[0] as f32, anchor[1] as f32];
    scene.set_group(n.frame, ft.frame[usize::from(rec.style).min(5)]);
    let file = ft.names.get(usize::from(family)).cloned().unwrap_or_default();
    if !file.is_empty() && !j.face_tex.contains_key(&file) {
        let t = bar_pictures(disc, &format!("menu/us/jmface_{file}.2ld"), scene, images);
        j.face_tex.insert(file.clone(), t);
    }
    let tex = j.face_tex.get(&file).and_then(|p| p.get(usize::from(rec.face)).copied().flatten());
    let [first, then] = ft.pic[usize::from(links)];
    match tex {
        Some(t) => {
            scene.nodes[n.pic].tex = t;
            scene.nodes[n.pic].visible = true;
            scene.nodes[n.pic].off = [anchor[0] as f32, anchor[1] as f32];
            scene.set_group_then(n.pic, first, then);
        }
        None => scene.nodes[n.pic].visible = false,
    }
    // The name and the sub-title: font 3, the ink colour the page gives them (+0x198 = 0x60000008), drawn from the group's anchor
    // (`FUN_00139888` clears the group-offset flag).
    let world_tokens = world_name(j, scene, if links { LINKS as i8 } else { world }, test);
    let ink = [0x08, 0x00, 0x00, 0x60];
    let text_label = |msg: u16, group_size: i32, variable: Vec<Token>, scene: &MenuScene| -> Option<Label> {
        let t = tokens(j, scene, msg)?;
        let size = if group_size > 0 { group_size as f32 } else { TEXT_SIZE };
        let world = scene.text.as_ref()?.world;
        Some(Label { tokens: t, style: Style { font: NAME_FONT, size: Some(size), align: 0, world, variable, color: ink, ..Style::default() }, offset: [0.0; 2] })
    };
    let variable = if rec.flags & 0x400 != 0 { world_tokens } else { Vec::new() };
    let name_label = text_label(rec.name, name_size, variable, scene);
    scene.nodes[n.name].label = name_label;
    scene.nodes[n.name].label_anchor = Some([0.0; 2]);
    scene.nodes[n.sub].visible = rec.sub != 0;
    if rec.sub != 0 {
        scene.nodes[n.sub].off = [0.0; 2];
        scene.set_group(n.sub, CHAR_SUB);
        let sub_label = text_label(rec.sub, sub_size, Vec::new(), scene);
        scene.nodes[n.sub].label = sub_label;
        scene.nodes[n.sub].label_anchor = Some([0.0; 2]);
    }
    // The description: the message split into lines, `list count = lines - 6` scroll positions when it has more than seven
    // lines (`FUN_002aadf0`), laid out with the layer group's size and pitch as page 3's story is.
    let size = if g_layer.size > 0 { g_layer.size } else { TEXT_SIZE as i32 };
    let font_gap = kh2_image::font::Font::get(NAME_FONT).line_gap as i32;
    let (_, gap) = story_metrics(g_layer.unk1c, size, font_gap);
    let mut lines: Vec<Vec<Token>> = vec![Vec::new()];
    for tok in tokens(j, scene, rec.desc).unwrap_or_default().into_iter().filter(|t| *t != Token::End) {
        if tok == Token::Newline {
            lines.push(Vec::new());
        } else if let Some(l) = lines.last_mut() {
            l.push(tok);
        }
    }
    let count = chars::desc_positions(lines.len());
    scene.nodes[n.desc].visible = true;
    scene.nodes[n.desc].off = [0.0; 2];
    scene.set_group(n.desc, STORY_ROW);
    if rec.flags & 0x800 != 0 {
        info!("journal: page 0xf character {ch} has the counter row (flag 0x800, group 0x4c): not drawn, the app keeps no defeated-enemy counts");
    }
    info!(
        "journal: page 0xf (FUN_002a98f0): character {ch} of {} (face {}, style {}, name {:#x}, sub {:#x}, desc {:#x}, model {}), {} lines, {count} positions, file jmface_{file}.2ld picture {}, frame group {}, anchor {anchor:?}",
        j.open.as_ref().map_or(0, |o| o.cp.list.len()),
        rec.face,
        rec.style,
        rec.name,
        rec.sub,
        rec.desc,
        rec.model,
        lines.len(),
        if tex.is_some() { "found" } else { "missing" },
        ft.frame[usize::from(rec.style).min(5)]
    );
    let o = j.open.as_mut().unwrap();
    o.cp.present = present;
    (o.page, o.world, o.top, o.count, o.lines, o.gap, o.size) = (page, world, 0, count, lines, gap.min(255) as u8, size as f32);
    char_view(j, scene);
}

/// The description's seven lines from the scroll position and the two arrows (`FUN_002ab678`, `FUN_002ab578`).
fn char_view(j: &mut Journal, scene: &mut MenuScene) {
    let Some(o) = j.open.as_ref() else { return };
    let mut t = vec![Token::Gap(o.gap)];
    for (k, l) in o.lines.iter().skip(o.top).take(chars::DESC_VIEW).enumerate() {
        if k > 0 {
            t.push(Token::Newline);
        }
        t.extend(l.iter().cloned());
    }
    t.push(Token::End);
    let world = scene.text.as_ref().map_or(0, |x| x.world);
    let style = Style { font: NAME_FONT, size: Some(o.size), align: 0, world, color: [0x08, 0x00, 0x00, 0x60], ..Style::default() };
    let n = j.nodes.cpage;
    scene.nodes[n.desc].label = Some(Label { tokens: t, style, offset: [0.0; 2] });
    let (up, down) = chars::arrows(o.top, o.count);
    for (k, (shown, g)) in [(up, CHAR_ARROWS[0]), (down, CHAR_ARROWS[1])].into_iter().enumerate() {
        scene.nodes[n.arrows[k]].visible = shown;
        if shown {
            scene.set_group(n.arrows[k], g);
        }
    }
}

/// `FUN_002af180` / `FUN_002af2b0` / `FUN_002aecf0`: the face under the pointer, its name on the plate and the hover frame.
/// `force` redoes them when nothing moved (the build).
fn faces_update(j: &mut Journal, scene: &mut MenuScene, test: &dyn Fn(u16) -> bool, force: bool) {
    let Some(o) = j.open.as_ref() else { return };
    let sel = chars::hit(&o.faces, o.pointer, o.half);
    if sel == o.sel && !force {
        return;
    }
    let (page, world, face, was) = (o.page, o.world, sel.and_then(|s| o.faces.get(s)).copied(), o.sel.is_some());
    let (plate, hover) = (j.nodes.chars.name, j.nodes.chars.hover);
    let links = page != 6;
    j.open.as_mut().unwrap().sel = sel;
    // The name (`FUN_002af2b0`): the record's message; with flag 0x400 its variable is the world's name.
    let list = j.seq_base + SEQ_LIST;
    let rec = face.and_then(|f| j.chars.get(f.ch)).copied();
    let g = scene.seqs.get(list).and_then(|s| s.groups.get(FACE_NAME)).copied().unwrap_or_default();
    let size = if g.size > 0 { g.size as f32 } else { TEXT_SIZE };
    let label = rec.and_then(|r| {
        let t = tokens(j, scene, r.name)?;
        let text_world = scene.text.as_ref()?.world;
        let variable = if r.flags & 0x400 != 0 { world_name(j, scene, if links { LINKS as i8 } else { world }, test) } else { Vec::new() };
        let color = group_color(scene, list, FACE_NAME);
        Some(Label { tokens: t, style: Style { font: NAME_FONT, size: Some(size), align: 2, world: text_world, variable, color, ..Style::default() }, offset: [0.0; 2] })
    });
    scene.nodes[plate].label = label;
    scene.nodes[plate].label_anchor = Some([0.0; 2]);
    // The hover frame (`FUN_002aecf0`): made on the first face the pointer reaches, moved to the next one without
    // restarting, removed when the pointer leaves every face.
    match (face.is_some(), was) {
        (true, false) => {
            let hg = j.tables.face.hover[usize::from(links)];
            scene.nodes[hover].visible = true;
            scene.set_group(hover, hg);
        }
        (false, _) => scene.nodes[hover].visible = false,
        _ => {}
    }
    if let Some(s) = sel {
        scene.nodes[hover].parent = j.nodes.chars.faces.get(s).map(|f| f[1]);
        scene.nodes[hover].off = [0.0; 2];
    }
}

/// The pointer's frames: `steps` 60 Hz steps of the held direction.
fn move_pointer(j: &mut Journal, scene: &mut MenuScene, steps: u32, dir: (i32, i32), test: &dyn Fn(u16) -> bool) {
    let Some(o) = j.open.as_mut() else { return };
    let mut p = o.pointer;
    for _ in 0..steps {
        p = chars::step_pointer(p, o.bounds, dir.0, dir.1);
    }
    if p == o.pointer {
        return;
    }
    o.pointer = p;
    scene.nodes[j.nodes.chars.pointer].pos = [p[0] as f32, p[1] as f32];
    let before = j.open.as_ref().and_then(|o| o.sel);
    faces_update(j, scene, test, false);
    let after = j.open.as_ref().and_then(|o| o.sel);
    if after != before {
        info!("journal: pointer {p:?} -> face {after:?}");
    }
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
    mut progress: Option<ResMut<Progress>>,
    keys: Res<ButtonInput<KeyCode>>,
    time: Res<Time>,
    mut se: MessageWriter<PlaySe>,
    mut frames: Local<f32>,
    mut logged: Local<u32>,
) {
    // The entries the journal marked seen go into the save (key `journal_seen`).
    if let Some(pairs) = j.marks.take_dirty()
        && let Some(p) = progress.as_mut()
    {
        for (c, k) in pairs {
            p.set_journal_seen(c, k);
        }
    }
    if menu.phase == Phase::Closed {
        if j.open.is_some() {
            close(&mut j, &mut scene, &mut menu);
        }
        j.request = None;
        return;
    }
    let story = progress.as_ref().map(|p| p.story());
    let test = |id: u16| story.as_ref().is_some_and(|s| s.test(id));
    let items = |id: u16| progress.as_ref().map_or(0, |p| i32::from(p.item_count(id)));
    if let Some(mode) = j.request.take() {
        // The opened chests (`FUN_001a6a90`) and the saved seen set (save +0x38f0) the marks start from. Read once when
        // the journal opens: no chest opens and no save loads while it is up.
        if let Some(p) = progress.as_ref() {
            j.opened = j.tables_lists.rows.iter().map(|r| r.chest).filter(|&c| c > 0 && p.chest_opened(c as u16)).collect();
            if j.open.is_none() {
                j.marks.load_seen(p.journal_seen_all());
            }
        }
        if !j.tried && scene.texs.len() >= 2 {
            load(&mut j, &disc, &mut scene, &mut images);
        }
        if !j.loaded {
            menu.page = 0;
            return;
        }
        // The quest and minigame tests read the save once when the journal opens (nothing plays a minigame while it is up).
        j.snap = progress.as_ref().map(|p| games_page::Snap::read(p, &j.quests, &j.minis)).unwrap_or_default();
        let world = area.name.get(..2).and_then(|w| crate::audio::WORLDS.iter().position(|&n| n == w));
        show(&mut j, &mut scene, &disc, &mut images, mode, world, &test, &items);
    }
    if let Some((page, world)) = j.goto.take()
        && j.open.is_some()
    {
        build(&mut j, &mut scene, &disc, &mut images, page, world, &test, &items);
        *logged = 0;
    }
    // The Treasures / Maps pages' cursor presses (`FUN_002b1890`).
    if let Some(nav) = j.nav.take()
        && j.open.as_ref().is_some_and(|o| matches!(o.page, 7 | 8))
        && treasure_page::navigate(&mut j, &mut scene, nav, &test)
    {
        menu_sound::play(&mut se, menu_sound::CURSOR);
        let (page, at) = j.open.as_ref().map_or((0, None), |o| (o.page, o.lists.as_ref().map(|l| l.1)));
        info!("journal: page {page} cursor {at:?} after {nav:?}");
    }
    // The face pages' pointer: three units a 60 Hz frame while a direction is held (`FUN_002aeea8`).
    *frames = (*frames + time.delta_secs() * 60.0).min(6.0);
    let steps = frames.floor() as u32;
    *frames -= steps as f32;
    if j.open.as_ref().is_some_and(|o| matches!(o.page, 6 | 0xb)) {
        let (mut dx, mut dy) = (
            i32::from(keys.pressed(KeyCode::ArrowRight)) - i32::from(keys.pressed(KeyCode::ArrowLeft)),
            i32::from(keys.pressed(KeyCode::ArrowDown)) - i32::from(keys.pressed(KeyCode::ArrowUp)),
        );
        // `KH2_JOURNAL_HOLD=<at>:<Left|Right|Up|Down>:<seconds>,..` (harness aid: the scripted keys of `KH2_KEYS` are released
        // again every frame while the window has no focus, so a held direction cannot be scripted there).
        if let Ok(spec) = std::env::var("KH2_JOURNAL_HOLD") {
            let now = time.elapsed_secs();
            for item in spec.split(',') {
                let mut f = item.split(':');
                let (Some(at), Some(k), Some(len)) = (f.next().and_then(|v| v.parse::<f32>().ok()), f.next(), f.next().and_then(|v| v.parse::<f32>().ok())) else { continue };
                if (at..at + len).contains(&now) {
                    match k {
                        "Left" => dx = -1,
                        "Right" => dx = 1,
                        "Up" => dy = -1,
                        "Down" => dy = 1,
                        _ => {}
                    }
                }
            }
        }
        move_pointer(&mut j, &mut scene, steps, (dx, dy), &test);
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
    }

    #[test]
    fn story_records_exist_with_their_flag_and_complete_with_the_last() {
        let mut d = vec![0u8; 0x10 + 4 * 0xc];
        d[8] = 4;
        for (k, (w, f)) in [(0i8, 1u16), (0, 2), (1, 3), (0x13, 1)].into_iter().enumerate() {
            let r = 0x10 + k * 0xc;
            d[r] = w as u8;
            d[r + 10..r + 12].copy_from_slice(&f.to_le_bytes());
        }
        let has = |set: &'static [u16]| move |f: u16| set.contains(&f);
        // World 0x13 is above 0xe: never new. A record needs its flag.
        assert_eq!(story_present(&d, &has(&[1, 3])), vec![(Cat::Story, 0), (Cat::Story, 2)]);
        assert_eq!(story_keys(&d, Some(0)), vec![(Cat::Story, 0), (Cat::Story, 1)]);
        assert_eq!(story_keys(&d, None).len(), 4);
        // Complete: the world's last record exists (world 0: record 1, flag 2); a world without records is complete.
        assert!(!story_complete(&d, 0, &has(&[1])));
        assert!(story_complete(&d, 0, &has(&[2])));
        assert!(story_complete(&d, 5, &has(&[])));
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
        // WP-MN12: the scroll bar's parts and the tags' groups.
        assert_eq!((t.bar, t.row_tag, t.line_tag, t.cell_tag), ([1, 0x75, 0x74, 0x76], [0x8b, 0x8c], [0x39, 0x3a], [0x98, 0x99]));
        for g in t.bar.into_iter().chain(t.row_tag).chain(t.line_tag).chain(t.cell_tag).chain([ROW_DONE, STORY_LAYERS[1]]) {
            assert!(s5.groups.len() > g, "group {g:#x}");
        }
        assert!(s5.groups[t.bar[2]].size > 0 && s5.groups[STORY_LAYERS[1]].size > 0, "the thumb's body and the track have a length");
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

    /// The face pages' data on the disc (skipped without `KH2_ISO`): the executable's tables, every family's faces with every
    /// flag set, the character records and the face files (WP-MN12).
    #[test]
    fn the_disc_face_page_data() {
        let Ok(iso) = std::env::var("KH2_ISO") else { return };
        let Ok(mut a) = kh2_archive::Archive::open(&iso) else { return };
        let elf = ["SLUS_210.05", "SLUS_210.05;1"].iter().find_map(|p| a.iso().read(p).ok()).unwrap();
        let t = read_page_tables(&Exe::load(elf).unwrap()).expect("the page tables");
        let f = &t.face;
        eprintln!("style {:x?} pic {:x?} tag {:x?} hover {:x?} help {:x?} names {:?}", f.style, f.pic, f.tag, f.hover, f.help, f.names);
        assert_eq!(f.style, [0xa, 0xb, 0xc, 0xd, 0xe, 0xf, 0x10, 0x11, 0x12, 0x13, 0x14, 0x15]);
        assert_eq!((f.pic, f.tag, f.hover, f.help), ([[1, 2], [4, 5]], [0x18, 0x19, 0x1b, 0x1c], [0x16, 0x1a], [0x48d5, 0x48d6]));
        assert_eq!(&f.names[..3], ["tt", "hb", "bb"]);
        assert_eq!(f.names[0x10], "ex");
        let two = layd::parse_2ld(&a.read("menu/us/jmemo.2ld").unwrap()).unwrap();
        let s5 = &two.layd.seqs[SEQ_LIST];
        for g in f.style.into_iter().chain(f.tag).chain(f.hover).chain([FACE_LAYER, FACE_BOUNDS, FACE_NAME, FACE_POINTER]) {
            assert!(s5.groups.len() > g, "group {g:#x}");
        }
        let face_seq = &two.layd.seqs[SEQ_FACE];
        for g in f.pic.into_iter().flatten() {
            assert!(face_seq.groups.len() > g, "face picture group {g}");
        }
        assert!(s5.groups[f.style[0]].unk20 / 2 == 32, "the reach of a face is half its frame's pitch");
        let g52 = s5.groups[FACE_BOUNDS];
        let anchor = s5.anchor(FACE_BOUNDS, 0, &kh2_image::seqd::Ext::default()).expect("the bounds object has an anchor");
        assert!(anchor[0] < g52.x && anchor[1] < g52.y, "the pointer's bounds {anchor:?} .. ({}, {})", g52.x, g52.y);
        let bar = a.read("menu/us/jiminy.bar").unwrap();
        let e = kh2_bar::parse(&bar).unwrap();
        let chars = chars::parse_chars(e[3].data);
        let groups = chars::parse_groups(e[5].data);
        assert_eq!((chars.len(), groups.len()), (309, 181));
        let msgs = kh2_image::text::Messages::from_bar(&a.read("msg/us/jm.bar").unwrap()).unwrap();
        let mut most = 0;
        for family in (0..0xfu8).chain([LINKS]) {
            let faces = chars::faces(e[5].data, &groups, family, &|_| true, &|_| 1);
            assert!(!faces.is_empty(), "family {family}");
            let name = &f.names[usize::from(family)];
            let pics = kh2_bar::parse(&a.read(&format!("menu/us/jmface_{name}.2ld")).unwrap()).unwrap().len();
            for face in &faces {
                let r = chars.get(face.ch).unwrap_or_else(|| panic!("family {family}: character {}", face.ch));
                assert!(usize::from(r.face) < pics, "family {family}: face picture {} of {pics} ({name})", r.face);
                assert!(r.style < 6, "style {}", r.style);
                assert!(msgs.tokens(u32::from(r.name)).is_some(), "name message {:#x}", r.name);
                assert!((60..=490).contains(&face.x) && (80..=370).contains(&face.y), "face {face:?} inside the bounds");
            }
            most = most.max(faces.len());
        }
        assert!(most <= FACE_POOL, "the face pool holds {most}");
        eprintln!("most faces on a page: {most}");
    }

    /// A character's own page on the disc (skipped without `KH2_ISO`): the frame table, the page's groups, and for every character
    /// of every family that the name, sub-title and description messages exist (WP-MN14).
    #[test]
    fn the_disc_character_page_data() {
        let Ok(iso) = std::env::var("KH2_ISO") else { return };
        let Ok(mut a) = kh2_archive::Archive::open(&iso) else { return };
        let elf = ["SLUS_210.05", "SLUS_210.05;1"].iter().find_map(|p| a.iso().read(p).ok()).unwrap();
        let t = read_page_tables(&Exe::load(elf).unwrap()).expect("the page tables");
        assert_eq!(t.face.frame, [10, 11, 12, 13, 14, 15], "the frame group of each style");
        assert_eq!((t.quest_tag, t.game_tag), ([0x39, 0x3a], [0x39, 0x3a]));
        let two = layd::parse_2ld(&a.read("menu/us/jmemo.2ld").unwrap()).unwrap();
        let s5 = &two.layd.seqs[SEQ_LIST];
        for g in [CHAR_LAYER, CHAR_NAME, CHAR_SUB, CHAR_ARROWS[0], CHAR_ARROWS[1], games_page::LAYER, games_page::BAR_LAYER, games_page::WORLD_ROW, games_page::GROUP_ROW, games_page::QUEST_ROW[0], games_page::QUEST_ROW[1], games_page::GAME_ROW]
            .into_iter()
            .chain(t.face.frame)
            .chain(t.quest_tag)
        {
            assert!(s5.groups.len() > g, "group {g:#x}");
        }
        let bar = a.read("menu/us/jiminy.bar").unwrap();
        let e = kh2_bar::parse(&bar).unwrap();
        let chars = chars::parse_chars(e[3].data);
        let groups = chars::parse_groups(e[5].data);
        let msgs = kh2_image::text::Messages::from_bar(&a.read("msg/us/jm.bar").unwrap()).unwrap();
        let sys = a.read("msg/us/sys.bar").ok().and_then(|b| kh2_image::text::Messages::from_bar(&b));
        let has = |id: u16| msgs.tokens(u32::from(id)).is_some() || sys.as_ref().is_some_and(|s| s.tokens(u32::from(id)).is_some());
        let mut most = 0;
        for family in (0..0xfu8).chain([LINKS]) {
            let faces = chars::faces(e[5].data, &groups, family, &|_| true, &|_| 1);
            for f in &faces {
                let r = chars[f.ch];
                assert!(has(r.name), "family {family}: name {:#x}", r.name);
                assert!(has(r.desc), "family {family}: character {} description {:#x}", f.ch, r.desc);
                assert!(r.sub == 0 || has(r.sub), "family {family}: sub-title {:#x}", r.sub);
                let lines = msgs.tokens(u32::from(r.desc)).map_or(1, |t| 1 + t.iter().filter(|t| **t == Token::Newline).count());
                most = most.max(lines);
            }
        }
        eprintln!("most description lines: {most}");
        for g in groups.iter().filter(|g| g.family == 0) {
            eprintln!("family 0 group: on {} off {} kind {} count {}", g.on, g.off, g.kind, g.count);
        }
    }

    /// The Quests and Minigames pages' data on the disc (skipped without `KH2_ISO`): every record's messages exist, the world headers
    /// have a world record, and the all-worlds listing fits the list (WP-MN14).
    #[test]
    fn the_disc_quest_and_minigame_data() {
        let Ok(iso) = std::env::var("KH2_ISO") else { return };
        let Ok(mut a) = kh2_archive::Archive::open(&iso) else { return };
        let bar = a.read("menu/us/jiminy.bar").unwrap();
        let e = kh2_bar::parse(&bar).unwrap();
        let worlds = parse_worlds(e[0].data).unwrap();
        let (quests, minis) = (games::parse_quests(e[8].data), games::parse_games(e[7].data));
        let msgs = kh2_image::text::Messages::from_bar(&a.read("msg/us/jm.bar").unwrap()).unwrap();
        let sys = a.read("msg/us/sys.bar").ok().and_then(|b| kh2_image::text::Messages::from_bar(&b));
        let has = |id: u16| msgs.tokens(u32::from(id)).is_some() || sys.as_ref().is_some_and(|s| s.tokens(u32::from(id)).is_some());
        eprintln!("{} quests, {} minigames", quests.len(), minis.len());
        for id in [0x4b34u16, 0x4b35, 0x4b80, 0x4b71, 0x4c8f] {
            let t = msgs.tokens(u32::from(id)).or_else(|| sys.as_ref().and_then(|s| s.tokens(u32::from(id))));
            eprintln!("message {id:#x}: {:?}", t.map(|t| kh2_image::text::to_text(&t)));
        }
        assert!(!quests.is_empty() && !minis.is_empty());
        for q in &quests {
            assert!(has(q.name), "quest name {:#x}", q.name);
            assert!(q.group == 0 || has(q.group), "quest group {:#x}", q.group);
            assert!(worlds.iter().any(|w| w.id as i16 as u16 == q.world), "quest world {}", q.world);
        }
        for m in &minis {
            assert!(has(m.name), "minigame name {:#x}", m.name);
            assert!(has(m.unit) || m.unit == 0, "minigame unit {:#x}", m.unit);
            assert!(worlds.iter().any(|w| w.id as i16 as u16 == m.world), "minigame world {}", m.world);
        }
        assert!(has(games::MSG_PLAYED) && has(games::MSG_NO_SCORE) && has(games_page::HELP_QUEST) && has(games_page::HELP_GAME));
        let (story, bit, mini) = (|_: u16| false, |_: u16| false, |_: u16| (0u8, 0u32));
        let all = games::Game { story: &story, bit: &bit, mini: &mini, all: true };
        let rows = games::quest_rows(&quests, &all);
        assert_eq!(rows.iter().filter(|r| matches!(r, games::Row::Item(_))).count(), quests.len(), "every quest is listed with the all-worlds flag");
        assert_eq!(games::game_rows(&minis, &all).iter().filter(|r| matches!(r, games::Row::Item(_))).count(), minis.len());
        for (k, q) in quests.iter().take(8).enumerate() {
            eprintln!("quest {k}: world {} group {:#x} name {:#x} flags {:#x} id {} game {} limit {} done {}", q.world, q.group, q.name, q.flags, q.id, q.game, q.limit, q.done);
        }
        for (k, m) in minis.iter().enumerate() {
            eprintln!("minigame {k}: world {} name {:#x} unit {:#x} game {}", m.world, m.name, m.unit, m.game);
        }
        let mut flags: std::collections::BTreeMap<u16, usize> = Default::default();
        for q in &quests {
            *flags.entry(q.flags).or_default() += 1;
        }
        eprintln!("quest flags used: {flags:x?}");
    }

    /// The Treasures and Maps pages' data on the disc (skipped without `KH2_ISO`): the `trsr` and `item` rows, every row's
    /// world and item name, the groups the pages name, and the layout with every world listed (WP-MN12).
    #[test]
    fn the_disc_list_page_data() {
        let Ok(iso) = std::env::var("KH2_ISO") else { return };
        let Ok(mut a) = kh2_archive::Archive::open(&iso) else { return };
        let elf = ["SLUS_210.05", "SLUS_210.05;1"].iter().find_map(|p| a.iso().read(p).ok()).unwrap();
        let exe = Exe::load(elf).unwrap();
        let t = read_page_tables(&exe).expect("the page tables");
        assert_eq!(t.list_text[0], 0x7e);
        assert_eq!(t.list_text[2], 0x7f);
        let two = layd::parse_2ld(&a.read("menu/us/jmemo.2ld").unwrap()).unwrap();
        let s5 = &two.layd.seqs[SEQ_LIST];
        use treasure_page::{CELL_MAP, CELL_TREASURE, FRAME, LABEL, LABEL_TAG, LAYERS, TOTAL};
        for g in LAYERS.into_iter().chain(CELL_MAP).chain(CELL_TREASURE).chain(FRAME).chain(LABEL_TAG).chain([LABEL, TOTAL, t.list_text[0], t.list_text[2]]) {
            assert!(s5.groups.len() > g, "group {g:#x}");
        }
        assert_eq!((s5.groups[CELL_TREASURE[0]].unk1c, s5.groups[CELL_TREASURE[0]].unk20), (28, 28), "the cells' pitch");
        assert_eq!(s5.groups[LAYERS[1]].size, 200, "the track");
        let tb = treasure_page::tables_from(Some(a.read("03system.bin").unwrap()));
        assert_eq!(tb.rows.len(), 382, "the treasure table");
        assert!(tb.items.len() > 400, "the item table: {}", tb.items.len());
        let maps = tb.rows.iter().filter(|r| tb.items.get(&r.item).is_some_and(|i| i.kind == treasure_page::MAP_KIND)).count();
        assert!(maps > 0, "map rows");
        let idx = read_world_index(&exe).unwrap();
        let wi = |w: u8| idx.get(usize::from(w)).copied().unwrap_or(-1);
        let sysmsg = kh2_image::text::Messages::from_bar(&a.read("msg/us/sys.bar").unwrap()).unwrap();
        let chests = lists::treasures(&tb.rows, &wi, &|_| true, true, &|_| false);
        let owned = lists::maps(&tb.rows, &wi, &|i| tb.items.get(&i).is_some_and(|r| r.kind == treasure_page::MAP_KIND), &|_| true, true);
        eprintln!("chests {} maps {} of {} rows", chests.len(), owned.len(), tb.rows.len());
        assert!(chests.len() > 100 && !owned.is_empty());
        for e in chests.iter().chain(&owned) {
            let it = tb.items.get(&e.item).unwrap_or_else(|| panic!("item {} of row {}", e.item, e.idx));
            assert!(sysmsg.tokens(u32::from(it.name)).is_some(), "name message {:#x} of item {}", it.name, e.item);
        }
        // Every journal world but 8 has chests; the layout counts a section per world and the rows in all.
        let l = lists::layout(lists::sorted(chests), &|_| true, Some(8));
        assert!(l.sections.iter().enumerate().all(|(w, s)| w == 8 || w >= 0xf || s.rows > 0), "{:?}", l.sections);
        assert!(l.total > lists::VIEW_ROWS, "{} rows", l.total);
        assert_eq!(l.entries.len(), l.sections.iter().map(|s| s.count).sum::<usize>());
    }

    /// The face, list, album and picture pages' button handling (WP-MN12).
    #[test]
    fn the_new_pages_read_their_buttons() {
        // Faces: cancel back, start closes, confirm carries the face under the pointer; the arrows are the pointer's, not events.
        assert_eq!(page_faces(Key::Cancel, None), Act::Back);
        assert_eq!(page_faces(Key::Start, Some(2)), Act::Close);
        assert_eq!(page_faces(Key::Confirm, Some(2)), Act::Face(Some(2)));
        assert_eq!(page_faces(Key::Confirm, None), Act::Face(None));
        assert_eq!(page_faces(Key::Move(1, 0), None), Act::None);
        // Treasures and Maps: every direction and L1 / R1 are cursor presses, a confirm does nothing.
        use lists::Nav;
        assert_eq!(page_lists(Key::Move(0, -1)), Act::Navigate(Nav::Up));
        assert_eq!(page_lists(Key::Move(0, 1)), Act::Navigate(Nav::Down));
        assert_eq!(page_lists(Key::Move(-1, 0)), Act::Navigate(Nav::Left));
        assert_eq!(page_lists(Key::Move(1, 0)), Act::Navigate(Nav::Right));
        assert_eq!(page_lists(Key::L1), Act::Navigate(Nav::PageUp));
        assert_eq!(page_lists(Key::R1), Act::Navigate(Nav::PageDown));
        assert_eq!(page_lists(Key::Confirm), Act::None);
        assert_eq!((page_lists(Key::Cancel), page_lists(Key::Start)), (Act::Back, Act::Close));
        // Album: the list widget's wrapping cursor, a confirm on a row opens its picture, nothing to confirm on an empty list.
        assert_eq!(page_album(Key::Move(0, 1), 2, 8), Act::Cursor(3));
        assert_eq!(page_album(Key::Move(0, 1), 7, 8), Act::Cursor(0));
        assert_eq!(page_album(Key::Move(0, -1), 0, 8), Act::Cursor(7));
        assert_eq!(page_album(Key::Move(0, 1), 0, 1), Act::None, "one row: nowhere to go");
        assert_eq!(page_album(Key::Confirm, 3, 8), Act::Photo(3));
        assert_eq!(page_album(Key::Confirm, 0, 0), Act::None);
        assert_eq!(page_album(Key::Cancel, 0, 0), Act::Back);
        // The picture page switches with L2 / R2, not with L1 / R1 (its help line shows the L2 and R2 icons).
        assert_eq!((page_photo(Key::L2), page_photo(Key::R2)), (Act::Browse(-1), Act::Browse(1)));
        assert_eq!((page_photo(Key::L1), page_photo(Key::R1)), (Act::None, Act::None));
        assert_eq!((page_photo(Key::Cancel), page_photo(Key::Start)), (Act::Back, Act::Close));
        // Page 2's rows open the new pages.
        let cats = categories(0, false, false);
        assert_eq!(page2(Key::Confirm, 0, &cats), Act::Category { cat: 0, page: 6 });
        assert_eq!(page2(Key::Confirm, 2, &cats), Act::Category { cat: 2, page: 4 });
        assert_eq!(page2(Key::Confirm, 3, &cats), Act::Category { cat: 3, page: 7 });
        assert_eq!(page2(Key::Confirm, 4, &cats), Act::Category { cat: 4, page: 8 });
    }

    /// The text colours the pages whose flag 8 is cleared take from their groups (skipped without `KH2_ISO`).
    #[test]
    fn the_disc_group_colours_of_the_text_objects() {
        let Ok(iso) = std::env::var("KH2_ISO") else { return };
        let Ok(mut a) = kh2_archive::Archive::open(&iso) else { return };
        let two = layd::parse_2ld(&a.read("menu/us/jmemo.2ld").unwrap()).unwrap();
        let mut scene = MenuScene::default();
        scene.seqs = two.layd.seqs;
        // The dark ink of the rows (group 2), of the Treasures labels (0x46) and of the name plate (0x53), white on the info bar
        // (0x7e), yellow on the total's count (0x7f); a group with no flagged animation keeps the default.
        assert_eq!(group_color(&scene, SEQ_LIST, 2), [64, 70, 60, 0x80]);
        assert_eq!(group_color(&scene, SEQ_LIST, treasure_page::LABEL), [16, 24, 0, 0x80]);
        assert_eq!(group_color(&scene, SEQ_LIST, FACE_NAME), [16, 24, 0, 0x80]);
        assert_eq!(group_color(&scene, SEQ_LIST, 0x7e), [240, 240, 240, 0x80]);
        assert_eq!(group_color(&scene, SEQ_LIST, 0x7f), [250, 240, 0, 0x80]);
        assert_eq!(group_color(&scene, SEQ_LIST, treasure_page::TOTAL), [240, 240, 240, 0x80], "the fade-in's end");
        assert_eq!(group_color(&scene, SEQ_LIST, 9999), DEFAULT_TEXT);
        assert_eq!(group_color(&scene, 99, 2), DEFAULT_TEXT);
    }

    /// The Album's data on the disc (skipped without `KH2_ISO`): the records, their texts, the picture files and the page's groups.
    #[test]
    fn the_disc_album_data() {
        let Ok(iso) = std::env::var("KH2_ISO") else { return };
        let Ok(mut a) = kh2_archive::Archive::open(&iso) else { return };
        let elf = ["SLUS_210.05", "SLUS_210.05;1"].iter().find_map(|p| a.iso().read(p).ok()).unwrap();
        let t = read_page_tables(&Exe::load(elf).unwrap()).expect("the page tables");
        let two = layd::parse_2ld(&a.read("menu/us/jmemo.2ld").unwrap()).unwrap();
        let s5 = &two.layd.seqs[SEQ_LIST];
        for g in [album_page::LAYER, album_page::TAG[0], album_page::TAG[1], album_page::PHOTO_LAYER, album_page::PHOTO_TITLE, album_page::PHOTO_CAPTION] {
            assert!(s5.groups.len() > g, "group {g:#x}");
        }
        assert_eq!(s5.groups[album_page::LAYER].unk1c, 26, "the rows' pitch");
        assert!(two.layd.seqs[1].groups.len() > 2, "the picture's groups 1 and 2 in sequence 1");
        let bar = a.read("menu/us/jiminy.bar").unwrap();
        let e = kh2_bar::parse(&bar).unwrap();
        let recs = album_page::parse(e[2].data);
        assert_eq!(recs.len(), 100);
        let msgs = kh2_image::text::Messages::from_bar(&a.read("msg/us/jm.bar").unwrap()).unwrap();
        for m in [album_page::HELP, album_page::HELP_PHOTO, 0x32dd] {
            assert!(msgs.tokens(u32::from(m)).is_some(), "message {m:#x}");
        }
        let mut worlds = std::collections::BTreeSet::new();
        for r in &recs {
            assert!(msgs.tokens(u32::from(r.title)).is_some() && msgs.tokens(u32::from(r.caption)).is_some(), "texts of {r:?}");
            let code = &t.face.names[usize::try_from(r.world).unwrap()];
            let file = format!("menu/us/jm_photo/{code}{}{}.bin", r.photo[0] as char, r.photo[1] as char);
            let bytes = a.read(&file).unwrap_or_else(|_| panic!("{file}"));
            let pics = kh2_bar::parse(&bytes).unwrap();
            assert!(pics.first().is_some_and(|p| kh2_image::imgd::decode(p.data).is_some()), "{file}");
            worlds.insert(r.world);
        }
        assert_eq!(worlds.len(), 15, "a list for every world: {worlds:?}");
        // No world lists more rows than the list has.
        for w in 0..15 {
            assert!(recs.iter().filter(|r| r.world == w).count() <= album_page::ROWS, "world {w}");
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
