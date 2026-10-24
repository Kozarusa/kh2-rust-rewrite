//! The world map's panels, prompts and landing (WP-WM5 and WP-WM6, stages W6 and W7 of docs/systems/worldmap-gummi/contract.md).
//!
//! What the original does (`SLUS_210.05`): on contact the cursor's handler passes the touched symbol to `FUN_002cca98`,
//! which makes the world info panel (`FUN_002cf618` -> `FUN_002ce808`) and removes it when the cursor leaves
//! (`FUN_002cf698`). The confirm button on a symbol makes the window object (`FUN_002cc1e0` -> `FUN_002cbee8`): it asks
//! `FUN_002d1d20` for the destination rows of the world; none gives mode 0, the land prompt (`FUN_002cbcf8`: row 0 the
//! "Land in <world>" text from the u16 table at 0x383520, row 1 the cancel text 0x4884; the result handler
//! `FUN_002cacd8`), some give mode 1, the area list (`FUN_002cb508`, handler `FUN_002ca6a0`: a locked row shows message
//! 0x4c0d). The button 0x100 switches to mode 2, the episode list (`FUN_002ca3d0`), and 0x208 from there to mode 3, the
//! area list again (`FUN_002ca538`). The window's teardown (`FUN_002ca2e8`) starts the landing,
//! `FUN_00104e18(destination, 1, 0, 0, 0)` (`FUN_002cc4b0`), with the destination of `FUN_001c1cf8` (mode 0) or of the chosen
//! row's `00worldpoint.bin` record (mode 1).
//!
//! The art (WP-WM6): both panels draw from `file/us/wm_ws.2ld` (a `.2ld`: one LAYD of 72 layers in 11 layouts, two images
//! and two sequences; the panels use sequence 1 with image 1, `FUN_00239020(layd, 1)` / `FUN_00233c20(imgz, 1)`). The
//! u16 table at 0x383458 gives each mode's layouts (open, close, idle, 0xffff): mode 0 layouts 0, 2, 1, mode 1 3, 5, 4,
//! mode 2 6, 9, 8 and mode 3 10, 5, 4. The info panel uses the layouts of mode 0 (`FUN_002cf3f8`). The text objects of the
//! screens (`FUN_0023bcd0`, nine records of 0x20 bytes at 0x383548: three triples of group ids per mode, a message id) sit
//! on the layer whose group they name in the current layout (`FUN_00238f48`: the id counts the groups of all sequences, so
//! it is the sequence-1 group plus the groups of sequence 0). Rows of the land prompt are bars (groups from the table at
//! 0x3834e8), the area list is a grid of picture cards (`FUN_002cb508`: card group 0x37, locked marker 0x38, the picture
//! `ws_i` group 0xb of `70landing.bar` with the area's picture, two columns, steps `unk20` across and `unk1c` down, the
//! grid starting at the frame layer's anchor plus the frame group's x and y), and the episode list a panel (groups 0x5e..0x60)
//! with up to three rows of group 0x34.
//!
//! What stays inferred or not ported, each a gap in docs/systems/worldmap-gummi/contract.md: the exact text positions (the original anchors text
//! objects on layers; the offsets here are read off the art), the app keys of the window buttons, the jmemo-driven
//! plate texts' message table (`FUN_00190c40`: the map's table, the system table and the journal's are tried in that order),
//! the info panel's level digits and key boxes (`FUN_001bea40`, `FUN_0014c9b0`, the `wmky` table), the two forced events
//! of `FUN_002ccd88`, the cursor sounds (`FUN_002d1760`; every menu sound is muted in the app anyway), and the open
//! animation of the rows (they appear in their idle group).

use std::collections::HashMap;

use bevy::prelude::*;
use kh2_ard::worldmap as wm;
use kh2_image::imgd;
use kh2_image::layd::{self, Layd};
use kh2_image::seqd::{Ext, Player, Seqd};
use kh2_image::text::{Messages, Style, Token};

use crate::area::{self, AreaRecord, WORLDS, WORLD_MAP};
use crate::area_jump::{Dest, ExitLocks, JumpTask, KEEP};
use crate::audio::PlaySe;
use crate::assets::Disc;
use crate::hud::{HudNode, HudScene, Label, SeqFile};
use crate::menu::menu_sound;
use crate::progress::Progress;
use crate::worldmap::{forced_event, symbol_object, symbol_world, MapCursor, MapPiece, WmEvent};
use crate::SCALE;

/// Bytes between the executable's addresses and its file offsets.
const EXE_BIAS: usize = 0xff000;
/// The u16 table of the "Land in <world>" texts (`FUN_002cbcf8`), 19 worlds.
const LAND_TEXTS: usize = 0x383520;
/// The u32 table of the world names (`FUN_00105de0`), read for the panel's first line.
const WORLD_NAMES: usize = 0x378040;
/// Worlds the two tables hold.
const WORLD_COUNT: usize = 19;
/// Messages of `msg/us/wm.bar`: the cancel row of the land prompt (`FUN_002cbcf8`) and the locked row (`FUN_002ca6a0`).
const MAYBE_LATER: u32 = 0x4884;
const NOT_ACCESSIBLE: u32 = 0x4c0d;
/// A message of the system table the label template is made from (`TextSys::message` needs an id that exists).
const TEMPLATE: u32 = 0xa7e;
/// How near the cursor must be to a symbol for contact, in centimetres on the ground plane. The original's test is the
/// actors' collision (`FUN_002cc9d0`, unread): [inferred] a distance.
pub const CONTACT_RADIUS: f32 = 1500.0;

/// The u16 table of the window modes: four entries per mode, the layout numbers (open, close, idle, 0xffff) (`FUN_002ca2a8`).
const MODE_TABLE: usize = 0x383458;
/// The nine text object descriptors (`FUN_002cbee8` loop over 0x383548 in steps of 0x20): triples of group ids for the modes 1, 2 and 3
/// at +0, +8 and +0x10, the message id at +0x18 (0xffffffff: none, the text is set by the window).
const DESCRIPTORS: usize = 0x383548;
const DESCRIPTOR_COUNT: usize = 9;
const DESCRIPTOR_SIZE: usize = 0x20;
/// The group sets of the land prompt's rows (`FUN_00296aa8` in `FUN_002cbcf8`): idle at +0 (0x3834e8) and selected at +8; local groups.
const LAND_ROWS: usize = 0x3834e8;
/// The anchor ids of an area card (`FUN_002992b8` with 0x3834b8 in `FUN_002cb508`): groups of the frame layer.
const CARD_ANCHORS: usize = 0x3834b8;
/// The group set of an episode row (`FUN_00296aa8` in `FUN_002cb508`): all three the same group.
const EPISODE_ROWS: usize = 0x3834f8;
/// The scroll bar's groups (`FUN_00299878`): none, up arrow, bar, down arrow.
const SCROLL_GROUPS: usize = 0x383508;
/// The group of an active episode's marker and the empty one (`DAT_003834a4`, `DAT_003834a8`).
const MARKER_GROUPS: usize = 0x3834a4;
/// The sequence and image of the panels in `wm_ws.2ld` (`FUN_00239020(layd, 1)`, `FUN_00233c20(imgz, 1)`).
const PANEL: usize = 1;
/// Local groups used by name in `FUN_002cb508`: the card (0x37), its locked marker (0x38), the picture group of `ws_i` (0xb),
/// the episode panel (open 0x5e, idle 0x5f, close 0x60) and a row of it (0x34).
const CARD_GROUP: usize = 0x37;
const LOCKED_GROUP: usize = 0x38;
const THUMBNAIL: usize = 0xb;
const EPISODE_PANEL: [usize; 3] = [0x5e, 0x5f, 0x60];
/// The "not accessible" box (`FUN_002ca6a0`: a text plate of group 0x5b that goes on to 0x5c, closed with 0x5d, message 0x4c0d).
const NOTICE_GROUPS: [usize; 3] = [0x5b, 0x5c, 0x5d];
/// The red outline of the selected card [inferred: group 54 of the sequence is a red frame of the card's size; the original's
/// list widget draws its own cursor object (`FUN_00296aa8`), which is unread].
const CARD_CURSOR: usize = 54;
/// The info panel's name plate groups (open, idle): `FUN_002ce808` makes the plate of group 0x97 (and 0x98 next).
const NAME_PLATE: [usize; 2] = [0x97, 0x98];
/// The info panel's level and time labels and digits, all local groups of sequence 1 (`FUN_002ce808`): the plates 0x41 (then 0x42) and
/// 0x48 (then 0x49), the level digits 0x12 + d, the time digits 0x1c + d and the colon 0x26 as children of the plates.
const LEVEL_LABEL: [usize; 2] = [0x41, 0x42];
const TIME_LABEL: [usize; 2] = [0x48, 0x49];
const LEVEL_DIGITS: usize = 0x12;
const TIME_DIGITS: usize = 0x1c;
const COLON: usize = 0x26;
/// The key boxes (`FUN_002cf6d0`): an empty one (0x94) and a filled one (0x95), children of the name plate.
const KEY_BOXES: [usize; 2] = [0x94, 0x95];
/// Rows of the episode list shown at once (`FUN_002cb508`: three).
const EPISODE_VISIBLE: usize = 3;
/// Layer slots of a panel and node slots of its other parts.
const LAYER_SLOTS: usize = 12;
const INFO_SLOTS: usize = 16;
const WINDOW_SLOTS: usize = 80;
/// Logic ticks a close animation plays (the groups are 17 frames long).
const CLOSE_TICKS: f32 = 17.0 / 60.0;
/// The typewriter budget given to the panel texts: none of them uses the typewriter, so the budget is never the limit.
const NO_TYPEWRITER: f32 = 1.0e6;
/// Half the height of a line of the map's text (font 3), to centre a left-aligned line on its bar [inferred].
const LINE_HALF: f32 = 9.0;

pub struct WorldMapUiPlugin;

impl Plugin for WorldMapUiPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<MapUi>().add_systems(Update, map_ui.before(crate::sora::read_pad));
    }
}

/// What a press in the window asks for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Press {
    Up,
    Down,
    Left,
    Right,
    Confirm,
    Cancel,
    /// The window's switch button (mask 0x100 of `FUN_002ca6a0`, and 0x208 from the episode list).
    Switch,
    /// The journal button, the triangle (mask 0x400 of `FUN_002ca6a0`, area list only): `FUN_002c9510`.
    Journal,
}

/// The window's modes (`FUN_002cbee8`'s mode word +0x1f68): 0 the land prompt, 1 and 3 the area list, 2 the episode list.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    /// The land prompt: row 0 lands, row 1 is "Maybe later".
    Land,
    /// The area list of a world that has seen destinations (mode 1, or 3 once the episode list was left).
    Areas,
    /// The world's episode list (mode 2).
    Episodes,
}

/// One row of the window: its text and where it lands (`None`: the cancel row).
#[derive(Clone, Debug, PartialEq)]
pub struct Row {
    pub tokens: Vec<Token>,
    pub dest: Option<Dest>,
    /// The row's area is locked (`FUN_00195010` result 2): a confirm shows message 0x4c0d.
    pub locked: bool,
    /// The picture of the area (`land` record byte 3, `FUN_002f7cc0`).
    pub picture: Option<u8>,
}

/// One row of the episode list (`FUN_002c9600` title, `FUN_002c9678` hint, `FUN_002c96f0` marker).
#[derive(Clone, Debug, PartialEq)]
pub struct EpisodeRow {
    pub title: Vec<Token>,
    pub hint: Vec<Token>,
    /// The episode's bit in the second array is set (`FUN_001c1628`): the row gets the marker group.
    pub active: bool,
}

/// What a press did.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    None,
    /// A row cursor moved.
    Moved,
    /// The window changed mode (the layouts of the new mode start).
    Switched,
    /// The window closes without a landing (row 1 of the land prompt, or a cancel: results 0 and -2 of `FUN_002cacd8`).
    Close,
    /// Land here (result 1; the teardown calls `FUN_00104e18(dest, 1, 0, 0, 0)`).
    Land(Dest),
    /// A locked row was confirmed: the "not accessible" box.
    Locked,
    /// The journal button on the area list: `FUN_002c9510(world)` opens the journal at the world's topic (`FUN_0024f3c0(0, FUN_00277c88(world))`).
    Journal,
}

/// The window's state, apart from the scene (tested without a disc).
#[derive(Clone, Debug, PartialEq)]
pub struct Window {
    pub world: u8,
    pub mode: Mode,
    pub rows: Vec<Row>,
    pub cursor: usize,
    pub episodes: Vec<EpisodeRow>,
    /// The episode list's cursor and its first visible row.
    pub episode_cursor: usize,
    pub scroll: usize,
    /// The area list is mode 3 (the episode list was left), not mode 1.
    pub returned: bool,
}

impl Window {
    pub fn new(world: u8, mode: Mode, rows: Vec<Row>, episodes: Vec<EpisodeRow>) -> Window {
        Window { world, mode, rows, cursor: 0, episodes, episode_cursor: 0, scroll: 0, returned: false }
    }

    /// The number the original calls the mode (0, 1, 2 or 3).
    pub fn mode_number(&self) -> usize {
        match self.mode {
            Mode::Land => 0,
            Mode::Areas if self.returned => 3,
            Mode::Areas => 1,
            Mode::Episodes => 2,
        }
    }

    pub fn press(&mut self, p: Press) -> Outcome {
        match self.mode {
            Mode::Land => self.press_land(p),
            Mode::Areas => self.press_areas(p),
            Mode::Episodes => self.press_episodes(p),
        }
    }

    fn press_land(&mut self, p: Press) -> Outcome {
        let n = self.rows.len();
        match p {
            Press::Up | Press::Down if n > 1 => {
                self.cursor = if p == Press::Up { (self.cursor + n - 1) % n } else { (self.cursor + 1) % n };
                Outcome::Moved
            }
            Press::Cancel => Outcome::Close,
            Press::Confirm => self.confirm(),
            _ => Outcome::None,
        }
    }

    fn confirm(&self) -> Outcome {
        match self.rows.get(self.cursor) {
            Some(Row { dest: Some(_), locked: true, .. }) => Outcome::Locked,
            Some(Row { dest: Some(d), .. }) => Outcome::Land(*d),
            _ => Outcome::Close,
        }
    }

    /// The area list is a grid of two columns (`FUN_00298e40(list, 2, rows)`; the buttons 0x10, 0x20, 0x40, 0x80 of
    /// `FUN_00296d10` are up, down, left, right): left and right move one card, up and down one row [inferred: wrapping like
    /// the other lists, a new press at an end wraps].
    fn press_areas(&mut self, p: Press) -> Outcome {
        let n = self.rows.len();
        let old = self.cursor;
        match p {
            Press::Left | Press::Right if n > 1 => {
                self.cursor = if p == Press::Left { (self.cursor + n - 1) % n } else { (self.cursor + 1) % n };
            }
            Press::Up | Press::Down if n > 2 => {
                let col = self.cursor % 2;
                let last = (0..n).rev().find(|i| i % 2 == col).unwrap_or(0);
                self.cursor = if p == Press::Down {
                    if self.cursor + 2 < n { self.cursor + 2 } else { col }
                } else if self.cursor >= 2 {
                    self.cursor - 2
                } else {
                    last
                };
            }
            Press::Cancel => return Outcome::Close,
            Press::Confirm => return self.confirm(),
            Press::Switch => {
                self.mode = Mode::Episodes;
                return Outcome::Switched;
            }
            Press::Journal => return Outcome::Journal,
            _ => {}
        }
        if self.cursor == old { Outcome::None } else { Outcome::Moved }
    }

    /// The episode list scrolls three rows at a time; the switch button and cancel go back to the area list (mode 3).
    fn press_episodes(&mut self, p: Press) -> Outcome {
        let n = self.episodes.len();
        match p {
            Press::Up | Press::Down if n > 1 => {
                self.episode_cursor = if p == Press::Up { (self.episode_cursor + n - 1) % n } else { (self.episode_cursor + 1) % n };
                if self.episode_cursor < self.scroll {
                    self.scroll = self.episode_cursor;
                } else if self.episode_cursor >= self.scroll + EPISODE_VISIBLE {
                    self.scroll = self.episode_cursor + 1 - EPISODE_VISIBLE;
                }
                Outcome::Moved
            }
            Press::Cancel | Press::Switch => {
                self.mode = Mode::Areas;
                self.returned = true;
                Outcome::Switched
            }
            _ => Outcome::None,
        }
    }
}

/// `FUN_001c1cf8`: the destination of a landing in `world` whose record's byte 1 is `state`. A record whose area is a story
/// program key resolves it through `07localset` (`localset(world, key)`, `FUN_00198080`) and carries the key as the three
/// program bytes (as the type-4 jump does) with entrance 0 [inferred: `FUN_00198080`'s own entrance]. No record: `None`
/// (the original then returns the previous contents of its result buffer; [inferred] that every reachable pair has a record).
pub fn land_destination(records: &[wm::Landing], world: u8, state: u8, localset: impl Fn(u8, u16) -> Option<u8>) -> Option<Dest> {
    Some(match wm::landing(records, world, state)? {
        wm::Destination::Area { area, entrance } => Dest::new(world, area, entrance),
        wm::Destination::Program { key, entrance: _ } => Dest { world, area: localset(world, u16::from(key))?, entrance: 0, programs: [key; 3] },
    })
}

/// `land_destination` for the app's own start: the app's baseline story does not run the new game's flag side effects
/// (docs/systems/core-runtime/progress.md), so a world's landing key (record byte 1) is 0, a state no `wlds` record has. Then the world's first
/// record is used [inferred: the original's new game has the key of its first record; with no record at all the original
/// returns whatever its result buffer held]. Returns the destination and whether the fallback was taken.
pub fn land_destination_or_first(records: &[wm::Landing], world: u8, state: u8, localset: impl Fn(u8, u16) -> Option<u8>) -> Option<(Dest, bool)> {
    if let Some(d) = land_destination(records, world, state, &localset) {
        return Some((d, false));
    }
    let first = records.iter().find(|r| r.world == world)?;
    (state == 0).then(|| land_destination(records, world, first.state, localset)).flatten().map(|d| (d, true))
}

/// A destination of the area list.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AreaRow {
    pub dest: Dest,
    pub locked: bool,
    pub picture: u8,
}

/// `FUN_002d1d20`: the rows of the world's area list. For each `land` record of the world whose world point was seen
/// (`seen`, `FUN_00195010`), the row `record.row` gets the point's record of `00worldpoint.bin` as destination (`FUN_00194f08`,
/// the three program bytes keep) and is locked when `locked(world, area)` (`FUN_00195358`). `in_app` drops the rows whose
/// area the app does not have (they are absent, not locked) [inferred]. Rows come out in list order.
pub fn area_rows(
    land: &[wm::LandRow],
    points: &[wm::WorldPoint],
    world: u8,
    seen: impl Fn(usize) -> bool,
    locked: impl Fn(u8, u8) -> bool,
    in_app: impl Fn(&Dest) -> bool,
) -> Vec<AreaRow> {
    let mut rows: Vec<(u8, AreaRow)> = Vec::new();
    for r in land.iter().filter(|r| r.world == world) {
        let Some(p) = points.get(r.world_point as usize) else { continue };
        if !seen(r.world_point as usize) {
            continue;
        }
        let dest = Dest::new(p.world, p.area, p.entrance);
        if !in_app(&dest) {
            continue;
        }
        rows.push((r.row, AreaRow { dest, locked: locked(p.world, p.area), picture: r.picture }));
    }
    rows.sort_by_key(|(row, _)| *row);
    rows.into_iter().map(|(_, r)| r).collect()
}

/// The episodes the world's list shows, in table order (`FUN_001c14e8` yields the records of the world whose bit in the first
/// array is set, `FUN_002c95b8` counts them).
pub fn episodes_of(epsd: &[wm::Episode], world: u8, seen: impl Fn(u32) -> bool) -> Vec<wm::Episode> {
    epsd.iter().filter(|e| e.world == world && seen(u32::from(e.id))).copied().collect()
}

/// The symbol the cursor touches: the nearest within `radius` (ground plane), with its world.
pub fn touched(cursor: Vec2, symbols: &[(u8, Vec2)], radius: f32) -> Option<u8> {
    symbols.iter().map(|(w, p)| (*w, p.distance(cursor))).filter(|(_, d)| *d <= radius).min_by(|a, b| a.1.total_cmp(&b.1)).map(|(w, _)| w)
}

/// The tables and texts the map reads from the disc.
struct Data {
    messages: Messages,
    wlds: Vec<wm::Landing>,
    land: Vec<wm::LandRow>,
    points: Vec<wm::WorldPoint>,
    epsd: Vec<wm::Episode>,
    exe: Vec<u8>,
    art: Option<Art>,
    /// `jiminy.bar` entry 1 (the journal's story records), the journal index of each world (`FUN_00277c88`) and the journal's
    /// messages: the sources of the two plate texts (`FUN_002c9548` / `FUN_002c9580` -> `FUN_002a1e98`).
    stor: Vec<u8>,
    world_index: Vec<i8>,
    journal_messages: Option<Messages>,
    /// `wmky` (the key boxes of the info panel) and `btlv` (the level sums), WP-WM7.
    keys: Vec<wm::KeyBoxes>,
    levels: Vec<wm::LevelRow>,
    /// Every story flag `Snap` copies: the journal's, the key boxes' and the level rows'.
    watch: Vec<u16>,
}

impl Data {
    fn load(disc: &Disc) -> Option<Data> {
        let messages = Messages::from_bar(&disc.read("msg/us/wm.bar")?)?;
        let progress = disc.read("00progress.bin")?;
        let entries = kh2_bar::parse(&progress).ok()?;
        let entry = |tag: &str| entries.iter().find(|e| e.tag == tag).map(|e| e.data);
        let wlds = wm::parse_wlds(entry(wm::WLDS)?);
        let epsd = wm::parse_epsd(entry(wm::EPSD)?);
        let landing = disc.read("70landing.bar")?;
        let lbar = kh2_bar::parse(&landing).ok()?;
        let land = wm::parse_land(lbar.iter().find(|e| e.tag == wm::LAND)?.data);
        let points = wm::parse_worldpoints(&disc.read("00worldpoint.bin")?);
        let exe = disc.read_root("SLUS_210.05")?;
        let stor = disc.read("menu/us/jiminy.bar").and_then(|b| Some(kh2_bar::parse(&b).ok()?.get(1)?.data.to_vec())).unwrap_or_default();
        let world_index = crate::menu_pages::Exe::load(exe.clone()).and_then(|e| crate::menu_journal::read_world_index(&e)).unwrap_or_default();
        let journal_messages = disc.read("msg/us/jm.bar").and_then(|b| Messages::from_bar(&b));
        let stor_flags: Vec<u16> = crate::menu_journal::records(&stor, 0xc).iter().map(|r| u16::from_le_bytes([r[10], r[11]])).collect();
        let keys = entry(wm::WMKY).map(wm::parse_wmky).unwrap_or_default();
        let levels = disc.read("00battle.bin").and_then(|b| Some(wm::parse_btlv(kh2_bar::parse(&b).ok()?.iter().find(|e| e.tag == wm::BTLV)?.data))).unwrap_or_default();
        let mut watch = stor_flags;
        watch.extend(keys.iter().flat_map(|k| k.pairs.iter().flat_map(|(a, b)| [*a, *b])).filter(|f| *f != 0));
        watch.extend(levels.iter().filter_map(|r| u16::try_from(r.flag).ok()));
        watch.sort_unstable();
        watch.dedup();
        let mut data = Data { messages, wlds, land, points, epsd, exe, art: None, stor, world_index, journal_messages, keys, levels, watch };
        data.art = Art::load(disc, &data, &lbar);
        Some(data)
    }

    fn word(&self, va: usize, size: usize) -> Option<u32> {
        let at = va.checked_sub(EXE_BIAS)?;
        let b = self.exe.get(at..at + size)?;
        Some(b.iter().rev().fold(0u32, |a, x| (a << 8) | u32::from(*x)))
    }

    /// Three u16 starting at `va`.
    fn triple(&self, va: usize) -> Option<[usize; 3]> {
        Some([self.word(va, 2)? as usize, self.word(va + 2, 2)? as usize, self.word(va + 4, 2)? as usize])
    }

    /// A message of `wm.bar`; its end token is dropped so lines can be joined (the layout stops at it).
    fn text(&self, id: u32) -> Option<Vec<Token>> {
        self.messages.tokens(id & 0x7fff).map(unended)
    }

    /// The two plate texts of a world's window (`FUN_002c9548`, `FUN_002c9580`): `FUN_002a1e98(FUN_00277c88(world))` gives the ids of the
    /// journal's last story record of the world whose flag is set (or the fallback message 0x8adc). The ids are looked up in the map's
    /// own table, the system table and the journal's, in that order [inferred: `FUN_00190c40` searches the tables bound at the time].
    fn plate_texts(&self, world: u8, sys: Option<&Messages>, test: &dyn Fn(u16) -> bool) -> [Vec<Token>; 2] {
        let index = self.world_index.get(world as usize).copied().unwrap_or(-1);
        let ids = crate::menu_journal::story_texts(&self.stor, index, test);
        if uilog() {
            info!("worldmap: world {world} journal index {index}: plate message ids {ids:#x?}");
        }
        ids.map(|id| {
            // The fallback message 0x8adc is a placeholder text of the system file (docs/systems/ui/contract.md); the baseline story has no journal flag set, so
            // the plate stays empty instead of showing it [inferred: the original's story always has a record for a world].
            if id == crate::menu_journal::NO_TEXT {
                return Vec::new();
            }
            let id = u32::from(id) & 0x7fff;
            self.messages.tokens(id).or_else(|| sys?.tokens(id)).or_else(|| self.journal_messages.as_ref()?.tokens(id)).map(unended).unwrap_or_default()
        })
    }

    /// The "Land in <world>" text of a world (`FUN_002cbcf8`).
    fn land_text(&self, world: u8) -> Option<Vec<Token>> {
        if world as usize >= WORLD_COUNT {
            return None;
        }
        self.text(self.word(LAND_TEXTS + 2 * world as usize, 2)?)
    }

    /// The world's name (`FUN_00105de0`): the id is in the wm table or else the system table.
    /// orig 0x105de0
    fn world_name(&self, world: u8, sys: Option<&Messages>) -> Option<Vec<Token>> {
        if world as usize >= WORLD_COUNT {
            return None;
        }
        let id = self.word(WORLD_NAMES + 4 * world as usize, 4)? & 0x7fff;
        self.messages.tokens(id).or_else(|| sys?.tokens(id)).map(unended)
    }
}

/// The tokens without the message's end marker.
fn unended(mut t: Vec<Token>) -> Vec<Token> {
    t.retain(|x| *x != Token::End);
    t
}

/// What the info panel shows of a world besides its name (`FUN_002ce808`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct InfoFacts {
    /// The world's level sum (`FUN_001bea40`'s table, `btlv`).
    pub level: u8,
    /// The time spent in the world (`FUN_0014c9b0`): hours and minutes. The app keeps no per-world clock, so both are 0 [gap].
    pub hours: u32,
    pub minutes: u32,
    /// Key boxes drawn and filled (`FUN_002c9358`, `FUN_002c93c0`, `wmky`).
    pub shown: usize,
    pub filled: usize,
}

/// The facts of `world` for the story flags `set` (the info panel).
fn info_facts(keys: &[wm::KeyBoxes], levels: &[wm::LevelRow], world: u8, set: impl Fn(u16) -> bool) -> InfoFacts {
    let sums = wm::level_sums(levels, |f| u16::try_from(f).is_ok_and(&set));
    let (shown, filled) = wm::find_keys(keys, world).map_or((0, 0), |k| k.counts(&set));
    InfoFacts { level: sums.get(world as usize).copied().unwrap_or(0), hours: 0, minutes: 0, shown, filled }
}

/// `FUN_002cc288(value, count, out)`: the last `count` decimal digits of `value`, zeros in front.
fn fixed_digits(value: u32, count: usize) -> Vec<usize> {
    let s = format!("{value:0>count$}");
    s.bytes().skip(s.len() - count).map(|b| usize::from(b - b'0')).collect()
}

/// The digits of `value` without leading zeros, at least one (the hours: the function answers how many digits the value has and the panel
/// draws that many of the last).
fn plain_digits(value: u32) -> Vec<usize> {
    value.to_string().bytes().map(|b| usize::from(b - b'0')).collect()
}

/// One text object descriptor (`0x383548`): the group ids it follows in the modes 1, 2 and 3 (open, close, idle) and its message.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Desc {
    sets: [[usize; 3]; 3],
    text: Option<u32>,
}

/// The panels' art and the tables that drive it (read from `wm_ws.2ld`, `70landing.bar` and the executable).
struct Art {
    layd: Layd,
    image: imgd::Image,
    /// `ws_i`: the sequence that draws a destination picture.
    ws: Seqd,
    pictures: Vec<Vec<u8>>,
    /// Per mode (0..4): the layouts (open, close, idle).
    modes: [[usize; 3]; 4],
    descs: Vec<Desc>,
    land_rows: [[usize; 3]; 2],
    card_anchor: [usize; 3],
    episode_row: usize,
    scroll_groups: [usize; 4],
    marker: [usize; 2],
}

impl Art {
    fn load(disc: &Disc, data: &Data, landing: &[kh2_bar::Entry]) -> Option<Art> {
        let two = layd::parse_2ld(&disc.read("file/us/wm_ws.2ld")?)?;
        let image = two.images.get(PANEL)?.clone();
        let ws = kh2_image::seqd::parse(landing.iter().find(|e| e.tag == wm::WS_I)?.data)?;
        let pictures = wm::pictures(landing).into_iter().map(<[u8]>::to_vec).collect();
        let mut modes = [[0; 3]; 4];
        for (m, row) in modes.iter_mut().enumerate() {
            *row = data.triple(MODE_TABLE + 8 * m)?;
        }
        let mut descs = Vec::new();
        for k in 0..DESCRIPTOR_COUNT {
            let at = DESCRIPTORS + DESCRIPTOR_SIZE * k;
            let sets = [data.triple(at)?, data.triple(at + 8)?, data.triple(at + 16)?];
            let id = data.word(at + 0x18, 4)?;
            descs.push(Desc { sets, text: (id != u32::MAX).then_some(id) });
        }
        let land_rows = [data.triple(LAND_ROWS)?, data.triple(LAND_ROWS + 8)?];
        let episode_row = data.word(EPISODE_ROWS, 2)? as usize;
        let scroll_groups = [0, 2, 4, 6].map(|o| data.word(SCROLL_GROUPS + o, 2).map(|v| v as usize));
        let scroll_groups = [scroll_groups[0]?, scroll_groups[1]?, scroll_groups[2]?, scroll_groups[3]?];
        let marker = [data.word(MARKER_GROUPS, 4)? as usize, data.word(MARKER_GROUPS + 4, 4)? as usize];
        Some(Art { layd: two.layd, image, ws, pictures, modes, descs, land_rows, card_anchor: data.triple(CARD_ANCHORS)?, episode_row, scroll_groups, marker })
    }

    fn seq(&self) -> Option<&Seqd> {
        self.layd.seqs.get(PANEL)
    }

    /// The (sequence, group) a group id names: the ids count the groups of all sequences in order (`FUN_00238f48`).
    fn locate(&self, id: usize) -> Option<(usize, usize)> {
        let mut first = 0;
        for (s, seq) in self.layd.seqs.iter().enumerate() {
            if id < first + seq.groups.len() {
                return Some((s, id - first));
            }
            first += seq.groups.len();
        }
        None
    }

    /// The index within `layout` of the layer that shows group id `id` (`FUN_00238f48`).
    fn layer_slot(&self, layout: usize, id: usize) -> Option<usize> {
        let (seq, group) = self.locate(id)?;
        self.layd.layout(layout).iter().position(|l| l.seq == seq && l.group == group)
    }

    /// The drawn rectangle of a layer's group at its last frame, in screen units.
    fn layer_rect(&self, layer: &layd::Layer) -> Option<[f32; 4]> {
        let seq = self.layd.seqs.get(layer.seq)?;
        let frame = seq.last_frame(layer.group);
        let qs = seq.quads(layer.group, frame, [layer.x as f32, layer.y as f32], &Ext::default());
        let mut r = [f32::MAX, f32::MAX, f32::MIN, f32::MIN];
        for p in qs.iter().flat_map(|q| q.pos) {
            r = [r[0].min(p[0]), r[1].min(p[1]), r[2].max(p[0]), r[3].max(p[1])];
        }
        (r[0] <= r[2]).then_some(r)
    }

    fn group(&self, g: usize) -> kh2_image::seqd::AnimGroup {
        self.seq().and_then(|s| s.groups.get(g)).copied().unwrap_or_default()
    }

    /// The window's layouts for the mode number (0..=3): (open, close, idle).
    fn layouts(&self, mode: usize) -> [usize; 3] {
        self.modes[mode.min(3)]
    }
}

/// What the scene needs to draw one object of a panel.
#[derive(Clone, Debug, PartialEq)]
struct Item {
    file: usize,
    group: Option<usize>,
    /// The group that follows `group` when it has played (the open group's idle group).
    next: Option<usize>,
    /// A root item's position, or a child's offset from its parent's anchor.
    at: [f32; 2],
    parent: Parent,
    text: Option<ItemText>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Parent {
    Root,
    /// A layer of the panel's layout.
    Layer(usize),
    /// An earlier item of the same list.
    Item(usize),
}

#[derive(Clone, Debug, PartialEq)]
struct ItemText {
    tokens: Vec<Token>,
    /// Text alignment (`kh2_image::text::Style::align`).
    align: u8,
    /// Glyph size in pixels (the group's size field, `FUN_00138fc8`); `None`: the font's own.
    size: Option<f32>,
}

impl Item {
    fn plate(file: usize, group: usize, at: [f32; 2], parent: Parent) -> Item {
        Item { file, group: Some(group), next: None, at, parent, text: None }
    }

    fn text(file: usize, at: [f32; 2], parent: Parent, tokens: Vec<Token>, align: u8) -> Item {
        Item { file, group: None, next: None, at, parent, text: Some(ItemText { tokens, align, size: None }) }
    }

    fn sized(mut self, size: Option<f32>) -> Item {
        if let Some(t) = self.text.as_mut() {
            t.size = size;
        }
        self
    }
}

/// What the item builders read besides the art.
struct Ctx<'a> {
    panel: usize,
    /// The scene file that draws picture `n` (`ws_i` with the picture).
    picture: &'a dyn Fn(u8) -> Option<usize>,
    text: &'a dyn Fn(u32) -> Option<Vec<Token>>,
    /// The two plates' bodies (`Data::plate_texts`).
    plates: Option<[Vec<Token>; 2]>,
}

/// The phase of a layout pair a panel is in: its entry of the open, close, idle triples (`Art::layouts`, a descriptor's sets).
const OPEN: usize = 0;
const IDLE: usize = 2;

/// The text size a group asks for (`FUN_00138fc8` takes its +0x18, 15 when it is 0); a value that is not a text size (the episode panel's
/// 234) leaves the font's own [inferred].
fn text_size(g: &kh2_image::seqd::AnimGroup) -> Option<f32> {
    (1..=32).contains(&g.size).then_some(g.size as f32)
}

impl Art {
    /// The anchor point of a layer's group at its last frame (where its children sit once it has played).
    fn layer_anchor(&self, layer: &layd::Layer) -> [f32; 2] {
        let seq = self.layd.seqs.get(layer.seq);
        let a = seq.and_then(|s| s.anchor(layer.group, s.last_frame(layer.group), &Ext::default())).unwrap_or([0, 0]);
        [layer.x as f32 + a[0] as f32, layer.y as f32 + a[1] as f32]
    }

    /// The text objects of a mode: each descriptor whose message is set (or that the window fills) hangs on the layer its group triple names
    /// in the layout of `phase`, at the place its art gives. A descriptor whose layer is an empty group (no drawn rectangle) shows nothing.
    fn text_items(&self, w: &Window, c: &Ctx, phase: usize) -> Vec<Item> {
        let mode = w.mode_number();
        if mode == 0 {
            return Vec::new();
        }
        let set = mode - 1;
        let layout = self.layouts(mode)[phase];
        let mut out = Vec::new();
        for (k, d) in self.descs.iter().enumerate() {
            let Some(slot) = self.layer_slot(layout, d.sets[set][phase]) else { continue };
            let layer = self.layd.layout(layout)[slot];
            let Some(r) = self.layer_rect(&layer) else { continue };
            let size = text_size(&self.group(layer.group));
            let anchor = self.layer_anchor(&layer);
            let (w_, h_) = (r[2] - r[0], r[3] - r[1]);
            let tokens = d.text.and_then(|id| (c.text)(id));
            // The text's place on the screen, its alignment and its tokens.
            let (at, align, tokens) = match (k, tokens) {
                // The caption pills and the frame's caption sit centred on their art; the blue pill's arrow tip takes the right
                // end of its rectangle, so its text centres on the body [inferred].
                (1 | 2 | 5, Some(t)) => ([r[0] + w_ / 2.0 - if k == 1 { 8.0 } else { 0.0 }, r[1] + h_ / 2.0], 3, t),
                (6, Some(t)) => ([r[0] + w_ / 2.0, r[1] + 13.0], 3, t),
                // The plates' caption pills are the top-left corner of the plate.
                (3 | 4, Some(t)) => ([r[0] + 95.0, r[1] + 11.0], 3, t),
                // The selected area's name stands on the gold line.
                (0, _) => match w.rows.get(w.cursor).filter(|row| !row.tokens.is_empty()) {
                    Some(row) => ([r[0] + 40.0, r[1] - 2.0], 0, row.tokens.clone()),
                    None => continue,
                },
                // The two plates' bodies: the journal's two story texts of the world.
                (7, _) => match &c.plates {
                    Some([first, _]) => ([r[0] + 15.0, r[1] + 26.0], 0, first.clone()),
                    None => continue,
                },
                (8, _) => match &c.plates {
                    Some([_, second]) => ([r[0] + 15.0, r[1] + 26.0], 0, second.clone()),
                    None => continue,
                },
                _ => continue,
            };
            out.push(Item::text(c.panel, [at[0] - anchor[0], at[1] - anchor[1]], Parent::Layer(slot), tokens, align).sized(size));
        }
        out
    }

    /// The land prompt's rows (`FUN_002cbcf8`): bars one `unk20` apart, the cursor's row in the selected group.
    fn land_items(&self, w: &Window, c: &Ctx) -> Vec<Item> {
        let mut out = Vec::new();
        let step = self.group(self.land_rows[0][0]).unk20 as f32;
        for (i, row) in w.rows.iter().enumerate() {
            let set = self.land_rows[usize::from(i == w.cursor)];
            let g = self.group(set[2]);
            out.push(Item::plate(c.panel, set[2], [0.0, step * i as f32], Parent::Root));
            // The text starts at the group's x (148, past the pointer) and is centred on the bar's height [inferred].
            let r = self.group_rect(set[2]).unwrap_or([0.0; 4]);
            let at = [g.x as f32, step * i as f32 + (r[1] + r[3]) / 2.0 - LINE_HALF];
            out.push(Item::text(c.panel, at, Parent::Root, row.tokens.clone(), 0).sized(text_size(&g)));
        }
        out
    }

    /// The area list (`FUN_002cb508`): a card per row.
    fn area_items(&self, w: &Window, c: &Ctx, phase: usize) -> Vec<Item> {
        let mut out = Vec::new();
        let layout = self.layouts(w.mode_number())[phase];
        if let Some(frame) = self.layer_slot(layout, self.card_anchor[phase]) {
            let fg = self.group(self.layd.layout(layout)[frame].group);
            let card = self.group(CARD_GROUP);
            for (i, row) in w.rows.iter().enumerate() {
                let (col, line) = ((i % 2) as f32, (i / 2) as f32);
                let at = [col * card.unk20 as f32 + fg.x as f32, line * card.unk1c as f32 + fg.y as f32];
                let base = out.len();
                out.push(Item::plate(c.panel, CARD_GROUP, at, Parent::Layer(frame)));
                if let Some(file) = row.picture.and_then(c.picture) {
                    out.push(Item { file, group: Some(THUMBNAIL), next: None, at: [2.0, 2.0], parent: Parent::Item(base), text: None });
                }
                if row.locked {
                    out.push(Item::plate(c.panel, LOCKED_GROUP, at, Parent::Layer(frame)));
                }
                if i == w.cursor {
                    out.push(Item::plate(c.panel, CARD_CURSOR, at, Parent::Layer(frame)));
                }
            }
        }
        out.extend(self.text_items(w, c, phase));
        out
    }

    /// The episode list (mode 2): the panel, the visible rows with their title, hint and marker, the scroll bar when there is more.
    fn episode_items(&self, w: &Window, c: &Ctx, phase: usize) -> Vec<Item> {
        let mut out = Vec::new();
        let panel = out.len();
        out.push(Item { file: c.panel, group: Some(EPISODE_PANEL[0]), next: Some(EPISODE_PANEL[1]), at: [0.0, 0.0], parent: Parent::Root, text: None });
        let anchor_of = |g: usize| {
            let a = self.seq().and_then(|s| s.anchor(g, s.last_frame(g), &Ext::default())).unwrap_or([0, 0]);
            [a[0] as f32, a[1] as f32]
        };
        let anchor = anchor_of(EPISODE_PANEL[1]);
        if let Some(t) = (c.text)(0x4c16) {
            // The caption sits on the panel's tab.
            let r = self.group_rect(EPISODE_PANEL[1]).unwrap_or([141.0, 73.0, 740.0, 395.0]);
            out.push(Item::text(c.panel, [r[0] + 265.0 - anchor[0], r[1] + 8.0 - anchor[1]], Parent::Item(panel), t, 3).sized(text_size(&self.group(EPISODE_PANEL[1]))));
        }
        let step = self.group(self.episode_row).unk20 as f32;
        let (ra, rr) = (anchor_of(self.episode_row), self.group_rect(self.episode_row).unwrap_or([0.0; 4]));
        let size = text_size(&self.group(self.episode_row));
        for (j, e) in w.episodes.iter().enumerate().skip(w.scroll).take(EPISODE_VISIBLE) {
            let line = (j - w.scroll) as f32;
            let row = out.len();
            out.push(Item::plate(c.panel, self.episode_row, [0.0, step * line], Parent::Item(panel)));
            // The title in the row's top band, the hint below it [inferred offsets inside the row's rectangle].
            let at = |dy: f32| [rr[0] + 16.0 - ra[0], rr[1] + dy - ra[1]];
            out.push(Item::text(c.panel, at(5.0), Parent::Item(row), e.title.clone(), 0).sized(size));
            out.push(Item::text(c.panel, at(33.0), Parent::Item(row), e.hint.clone(), 0).sized(size));
            if e.active {
                out.push(Item::plate(c.panel, self.marker[0], [0.0, 0.0], Parent::Item(row)));
            }
        }
        if w.episodes.len() > EPISODE_VISIBLE {
            // The scroll bar: the bar group between the arrows, at the panel's right side [inferred position].
            let [_, up, bar, down] = self.scroll_groups;
            let x = anchor[0] + 91.0;
            out.push(Item::plate(c.panel, bar, [x, anchor[1] + 20.0], Parent::Root));
            out.push(Item::plate(c.panel, up, [x, anchor[1] + 14.0], Parent::Root));
            out.push(Item::plate(c.panel, down, [x, anchor[1] + 20.0 + 100.0], Parent::Root));
        }
        out.extend(self.text_items(w, c, phase));
        out
    }

    /// The box over a locked row: the plate with its message centred on it.
    fn notice_items(&self, c: &Ctx) -> Vec<Item> {
        let mut out = vec![Item { file: c.panel, group: Some(NOTICE_GROUPS[0]), next: Some(NOTICE_GROUPS[1]), at: [0.0, 0.0], parent: Parent::Root, text: None }];
        if let Some(t) = (c.text)(NOT_ACCESSIBLE) {
            let r = self.group_rect(NOTICE_GROUPS[1]).unwrap_or([128.0, 200.0, 384.0, 253.0]);
            out.push(Item::text(c.panel, [(r[0] + r[2]) / 2.0, (r[1] + r[3]) / 2.0], Parent::Root, t, 3).sized(text_size(&self.group(NOTICE_GROUPS[1]))));
        }
        out
    }

    fn group_rect(&self, g: usize) -> Option<[f32; 4]> {
        self.layer_rect(&layd::Layer { kind: 1, seq: PANEL, group: g, x: 0, y: 0 })
    }

    /// The info panel's level and time lines and key boxes (`FUN_002ce808`, `FUN_002cf6d0`), after the `base` items it already has (the
    /// name plate is item 0): a label plate with the digits hung on it one `unk20` apart (`FUN_00237698` is the group's `unk20`), the level
    /// as two digits, the hours without leading zeros, a colon and the minutes as two digits; then one box per key pair whose first flag is
    /// set, the filled ones first, `x - size * i` from the name plate (`FUN_00237618` is the group's x, `FUN_00237658` its size), the
    /// whole row moved by half its width [inferred: the sign and the parent of that shift (`+0x254`) were not traced].
    fn info_extras(&self, panel: usize, base: usize, f: &InfoFacts) -> Vec<Item> {
        let mut out: Vec<Item> = Vec::new();
        let label = |out: &mut Vec<Item>, g: [usize; 2]| {
            out.push(Item { file: panel, group: Some(g[0]), next: Some(g[1]), at: [0.0, 0.0], parent: Parent::Root, text: None });
            base + out.len() - 1
        };
        let digit = |out: &mut Vec<Item>, parent: usize, x: &mut f32, g: usize| {
            out.push(Item::plate(panel, g, [*x, 0.0], Parent::Item(parent)));
            *x += self.group(g).unk20 as f32;
        };
        let l = label(&mut out, LEVEL_LABEL);
        let mut x = 0.0;
        for d in fixed_digits(u32::from(f.level), 2) {
            digit(&mut out, l, &mut x, LEVEL_DIGITS + d);
        }
        let t = label(&mut out, TIME_LABEL);
        let mut x = 0.0;
        for d in plain_digits(f.hours.min(999)) {
            digit(&mut out, t, &mut x, TIME_DIGITS + d);
        }
        digit(&mut out, t, &mut x, COLON);
        for d in fixed_digits(f.minutes, 2) {
            digit(&mut out, t, &mut x, TIME_DIGITS + d);
        }
        for i in 0..f.shown.min(3) {
            let g = self.group(KEY_BOXES[usize::from(i < f.filled)]);
            let (w, n) = (g.size as f32, f.shown.min(3) as f32);
            out.push(Item::plate(panel, KEY_BOXES[usize::from(i < f.filled)], [g.x as f32 - w * i as f32 - w * n / 2.0, 0.0], Parent::Item(0)));
        }
        out
    }

    /// The items of the window in its state; `phase` is `OPEN` while its open layout plays and `IDLE` after.
    fn items(&self, w: &Window, c: &Ctx, phase: usize) -> Vec<Item> {
        match w.mode {
            Mode::Land => self.land_items(w, c),
            Mode::Areas => self.area_items(w, c, phase),
            Mode::Episodes => self.episode_items(w, c, phase),
        }
    }
}

/// A set of layer nodes: the layers of the panel's current layout. The open layout plays, then the idle one replaces it (the original's
/// layout object switches to the next layout when the current one has played, `FUN_0023bbb8`).
#[derive(Default)]
struct LayerSet {
    slots: Vec<usize>,
    /// What is shown: the layout it opened (so a mode change restarts, a cursor move does not).
    shown: Option<usize>,
    /// Seconds until the idle layout takes over, and which one.
    swap: Option<(f32, usize)>,
    /// Seconds left of the close animation.
    closing: Option<f32>,
}

impl LayerSet {
    /// The phase the layers are in.
    fn phase(&self) -> usize {
        if self.swap.is_some() { OPEN } else { IDLE }
    }

    /// Seconds the layers of `layout` need to play (a group that plays a limited number of times counts for its whole play).
    fn play_time(art: &Art, layout: usize) -> f32 {
        let frames = art
            .layd
            .layout(layout)
            .iter()
            .filter(|l| l.seq == PANEL)
            .filter_map(|l| art.seq().and_then(|s| s.groups.get(l.group).map(|g| (g.loops, s.group_len(l.group)))))
            .filter(|(loops, _)| *loops > 0)
            .map(|(loops, len)| i32::from(loops) * len)
            .max()
            .unwrap_or(0);
        frames as f32 / 60.0
    }

    fn start(&mut self, scene: &mut HudScene, art: &Art, panel: usize, layout: usize) {
        for (i, slot) in self.slots.clone().into_iter().enumerate() {
            match art.layd.layout(layout).get(i).filter(|l| l.seq == PANEL) {
                Some(l) => {
                    let n = &mut scene.nodes[slot];
                    n.file = panel;
                    n.pos = [l.x as f32, l.y as f32];
                    n.off = [0.0; 2];
                    n.next = None;
                    n.visible = true;
                    scene.set_group(slot, l.group);
                }
                None => scene.nodes[slot].visible = false,
            }
        }
    }

    /// Starts the layers of `open`; the layers of `idle` take over when they have played.
    fn show(&mut self, scene: &mut HudScene, art: &Art, panel: usize, open: usize, idle: usize) {
        if self.shown == Some(open) && self.closing.is_none() {
            return;
        }
        self.closing = None;
        self.shown = Some(open);
        self.swap = Some((Self::play_time(art, open), idle));
        self.start(scene, art, panel, open);
    }

    /// Plays the close layout; the nodes go when it has played.
    fn close(&mut self, scene: &mut HudScene, art: &Art, panel: usize, close: usize) {
        if self.shown.is_none() || self.closing.is_some() {
            return;
        }
        self.swap = None;
        self.closing = Some(CLOSE_TICKS);
        self.start(scene, art, panel, close);
    }

    fn hide(&mut self, scene: &mut HudScene) {
        for slot in &self.slots {
            let n = &mut scene.nodes[*slot];
            n.visible = false;
            n.label = None;
        }
        self.shown = None;
        self.closing = None;
        self.swap = None;
    }

    /// Advances the open and close timers; true when the idle layout has just taken over (the panel's items must be laid again).
    fn tick(&mut self, scene: &mut HudScene, art: &Art, panel: usize, dt: f32) -> bool {
        if let Some(t) = self.closing {
            if t - dt <= 0.0 {
                self.hide(scene);
            } else {
                self.closing = Some(t - dt);
            }
        }
        match self.swap {
            Some((t, idle)) if t - dt <= 0.0 => {
                self.swap = None;
                self.start(scene, art, panel, idle);
                true
            }
            Some((t, idle)) => {
                self.swap = Some((t - dt, idle));
                false
            }
            None => false,
        }
    }
}

/// The scene nodes a panel uses.
struct Pool {
    info: LayerSet,
    info_items: Vec<usize>,
    window: LayerSet,
    window_items: Vec<usize>,
}

/// A label for `tokens` drawn in the map's text style: the light grey of the original's text plates (`FUN_00138fc8` stores 0x80e5e5e5).
fn label(scene: &HudScene, t: &ItemText) -> Option<Label> {
    let sys = scene.text.as_ref()?;
    let style = Style { font: 3, size: t.size, align: t.align, world: sys.world, ..Style::default() };
    let mut l = sys.message(TEMPLATE, style, [0.0; 2])?;
    l.tokens = t.tokens.clone();
    Some(l)
}

/// Lays `items` on the nodes `slots` (the rest are hidden); `layers` are the panel's layer nodes for `Parent::Layer`. An item whose group is
/// already playing (or whose follow-up is) keeps its animation.
fn apply(scene: &mut HudScene, slots: &[usize], layers: &[usize], items: &[Item]) {
    let labels: Vec<Option<Label>> = items.iter().map(|i| i.text.as_ref().and_then(|t| label(scene, t))).collect();
    for (k, slot) in slots.iter().copied().enumerate() {
        let Some(item) = items.get(k) else {
            let n = &mut scene.nodes[slot];
            n.visible = false;
            n.label = None;
            continue;
        };
        let parent = match item.parent {
            Parent::Root => None,
            Parent::Layer(i) => layers.get(i).copied(),
            Parent::Item(i) => slots.get(i).copied(),
        };
        let same = {
            let n = &scene.nodes[slot];
            n.file == item.file && (n.player.group == item.group || (item.next.is_some() && n.player.group == item.next))
        };
        {
            let n = &mut scene.nodes[slot];
            n.file = item.file;
            n.parent = parent;
            if parent.is_some() {
                n.pos = [0.0; 2];
                n.off = item.at;
            } else {
                n.pos = item.at;
                n.off = [0.0; 2];
            }
            n.visible = true;
            n.label = labels[k].clone();
            n.label_time = NO_TYPEWRITER;
        }
        if !same {
            match item.group {
                Some(g) => {
                    scene.set_group(slot, g);
                    scene.nodes[slot].next = item.next;
                }
                None => {
                    let n = &mut scene.nodes[slot];
                    n.player = Player::default();
                    n.next = None;
                }
            }
        }
    }
}

/// What the map reads of the story, copied out so its lock is not held while other code (the exit locks) takes it again.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Snap {
    /// The world points seen (`FUN_00195010`).
    pub points: Vec<usize>,
    /// The 19 world records (`FUN_001b4e80`).
    pub records: Vec<[u8; 4]>,
    /// The ids of the episodes whose bit is set (`FUN_001c15f0`).
    pub episodes: Vec<u32>,
    /// The ids of the episodes whose second bit is set (`FUN_001c1628`).
    pub active: Vec<u32>,
    /// Which of the journal's story flags are set (`FUN_002a1e78`), for the plate texts.
    pub set_flags: Vec<u16>,
}

impl Snap {
    pub fn of(story: &crate::progress::Story, epsd: &[wm::Episode], flags: &[u16]) -> Snap {
        Snap {
            points: story.world_points(),
            records: (0..WORLD_COUNT).map(|w| story.world_record(w)).collect(),
            episodes: epsd.iter().map(|e| u32::from(e.id)).filter(|id| story.episode_seen(*id)).collect(),
            active: epsd.iter().map(|e| u32::from(e.id)).filter(|id| story.episode_active(*id)).collect(),
            set_flags: flags.iter().copied().filter(|f| story.test(*f)).collect(),
        }
    }

    fn state(&self, world: u8) -> u8 {
        self.records.get(world as usize).map_or(0, |r| r[1])
    }
}

#[derive(Resource, Default)]
pub struct MapUi {
    data: Option<Data>,
    failed: bool,
    near: Option<u8>,
    window: Option<Window>,
    /// The "not accessible" box is up over a locked row.
    notice: bool,
    pool: Option<Pool>,
    /// The scene file of the panel art, and the scene files of the pictures made so far.
    panel: Option<usize>,
    pictures: HashMap<u8, usize>,
    /// The mode number and cursor the window's items were last laid for.
    laid: Option<(usize, usize, usize, usize, bool)>,
    /// The area the state belongs to; a new area starts clean.
    area: Option<String>,
    /// Where the player stood before the map (app-only escape).
    from: Option<Dest>,
}

impl MapUi {
    /// The scene file of the panel art: the panel's own nodes use it (the forced events leave them alone when they hide the field HUD).
    pub fn panel_file(&self) -> Option<usize> {
        self.panel
    }
}

fn uilog() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| std::env::var("KH2_WMLOG").is_ok_and(|v| !v.is_empty() && v != "0"))
}

/// Harness aid: `KH2_WMLOCK=<area>` shows the rows of that area of the world as locked (the exit locks are story state the harness cannot reach easily).
fn forced_lock() -> Option<u8> {
    std::env::var("KH2_WMLOCK").ok()?.parse().ok()
}

/// The window's text lines as plain strings, for the log.
fn plain(t: &[Token]) -> String {
    kh2_image::text::to_text(t)
}

/// The info panel's name line for a world (`FUN_002ce808`: the plate with `FUN_00105de0(world)`'s text).
fn info_name(data: &Data, world: u8, sys: Option<&Messages>) -> Vec<Token> {
    data.world_name(world, sys).unwrap_or_default()
}

/// The episode rows of a world's list (`FUN_002cb508` rows).
fn episode_rows(data: &Data, world: u8, snap: &Snap) -> Vec<EpisodeRow> {
    episodes_of(&data.epsd, world, |id| snap.episodes.contains(&id))
        .into_iter()
        .map(|e| EpisodeRow { title: data.text(u32::from(e.title)).unwrap_or_default(), hint: data.text(u32::from(e.hint)).unwrap_or_default(), active: snap.active.contains(&u32::from(e.id)) })
        .collect()
}

/// The window a confirm on `world` opens: the area list when the world has seen rows (`FUN_002d1d20` > 0), else the land prompt.
fn open_window(data: &Data, world: u8, snap: &Snap, locks: &ExitLocks, place_name: impl Fn(u8, u8) -> Option<Vec<Token>>, localset: impl Fn(u8, u16) -> Option<u8>) -> Option<Window> {
    let episodes = episode_rows(data, world, snap);
    let forced = forced_lock();
    let rows = area_rows(&data.land, &data.points, world, |i| snap.points.contains(&i), |w, a| locks.locked(w, a) || forced == Some(a), app_has);
    if !rows.is_empty() {
        let rows = rows
            .iter()
            .map(|r| Row { tokens: place_name(r.dest.world, r.dest.area).map(unended).unwrap_or_default(), dest: Some(r.dest), locked: r.locked, picture: Some(r.picture) })
            .collect();
        return Some(Window::new(world, Mode::Areas, rows, episodes));
    }
    let state = snap.state(world);
    let dest = land_destination_or_first(&data.wlds, world, state, localset).map(|(d, fallback)| {
        if fallback {
            info!("worldmap: world {world}'s landing key is 0 (no new-game side effects in the app): the first wlds record is used [inferred]");
        }
        d
    });
    let rows = vec![
        Row { tokens: data.land_text(world)?, dest, locked: false, picture: None },
        Row { tokens: data.text(MAYBE_LATER)?, dest: None, locked: false, picture: None },
    ];
    Some(Window::new(world, Mode::Land, rows, episodes))
}

/// Whether the app has the destination's area: the map itself is not a landing place, and only Twilight Town is loaded.
pub fn app_has(d: &Dest) -> bool {
    d.world != WORLD_MAP && d.world != KEEP && area::area_name(d.world, d.area).is_some()
}

/// Makes the scene file of the panel art and the pool of nodes once the HUD text system exists.
fn ensure_pool(ui: &mut MapUi, scene: &mut HudScene, images: &mut Assets<Image>) -> Option<()> {
    if ui.pool.is_some() {
        return Some(());
    }
    let art = ui.data.as_ref()?.art.as_ref()?;
    let seq = art.seq()?.clone();
    scene.files.push(SeqFile::new(&art.image, seq, images));
    let panel = scene.files.len() - 1;
    ui.panel = Some(panel);
    let mut make = |count: usize| -> Vec<usize> {
        (0..count)
            .map(|_| {
                let mut n = HudNode::new(panel, None);
                n.visible = false;
                scene.add(n)
            })
            .collect()
    };
    let info = LayerSet { slots: make(LAYER_SLOTS), ..LayerSet::default() };
    let info_items = make(INFO_SLOTS);
    let window = LayerSet { slots: make(LAYER_SLOTS), ..LayerSet::default() };
    let window_items = make(WINDOW_SLOTS);
    ui.pool = Some(Pool { info, info_items, window, window_items });
    Some(())
}

/// Makes the scene file of each picture the rows name (`ws_i` drawing the picture).
fn ensure_pictures(ui: &mut MapUi, scene: &mut HudScene, images: &mut Assets<Image>, w: &Window) {
    let Some(art) = ui.data.as_ref().and_then(|d| d.art.as_ref()) else { return };
    for p in w.rows.iter().filter_map(|r| r.picture) {
        if ui.pictures.contains_key(&p) {
            continue;
        }
        let Some(img) = art.pictures.get(p as usize).and_then(|b| imgd::decode(b)) else { continue };
        scene.files.push(SeqFile::new(&img, art.ws.clone(), images));
        ui.pictures.insert(p, scene.files.len() - 1);
    }
}

#[allow(clippy::too_many_arguments)]
fn map_ui(
    mut ui: ResMut<MapUi>,
    rec: Res<AreaRecord>,
    disc: Res<Disc>,
    progress: Option<Res<Progress>>,
    locks: Res<ExitLocks>,
    mut keys: ResMut<ButtonInput<KeyCode>>,
    mut mouse: ResMut<ButtonInput<MouseButton>>,
    mut task: Option<ResMut<JumpTask>>,
    mut scene: Option<ResMut<HudScene>>,
    mut images: ResMut<Assets<Image>>,
    time: Res<Time>,
    pieces: Query<(&MapPiece, &Transform, &Visibility)>,
    cursor: Query<&Transform, With<MapCursor>>,
    mut event: ResMut<WmEvent>,
    mut se: MessageWriter<PlaySe>,
    (menu, mut open_journal): (Res<crate::menu::Menu>, MessageWriter<crate::menu_journal::OpenJournal>),
) {
    let on_map = rec.world == WORLD_MAP && rec.run.is_some();
    let name = rec.run.as_ref().map(|r| r.name.clone());
    let Some(scene) = scene.as_deref_mut() else { return };
    if ui.area != name || !on_map {
        // A new area (or the map left): nothing of the old panels stays.
        if let Some(p) = ui.pool.as_mut() {
            p.info.hide(scene);
            p.window.hide(scene);
            for s in p.info_items.iter().chain(&p.window_items) {
                scene.nodes[*s].visible = false;
                scene.nodes[*s].label = None;
            }
        }
        ui.near = None;
        ui.window = None;
        ui.notice = false;
        ui.laid = None;
        ui.area = name;
        if !on_map {
            ui.from = Some(Dest::new(rec.world, rec.area, rec.entrance));
            return;
        }
    }
    if ui.data.is_none() && !ui.failed {
        match Data::load(&disc) {
            Some(d) => ui.data = Some(d),
            None => {
                warn!("worldmap: the tables or msg/us/wm.bar are not readable; no panels");
                ui.failed = true;
            }
        }
    }
    if ui.data.is_none() || scene.text.is_none() {
        return;
    }
    if ui.data.as_ref().is_some_and(|d| d.art.is_none()) && !ui.failed {
        warn!("worldmap: file/us/wm_ws.2ld or 70landing.bar's pictures are not readable; no panel art");
        ui.failed = true;
    }
    if ui.failed || ensure_pool(&mut ui, scene, &mut images).is_none() {
        return;
    }
    if task.as_deref().is_some_and(|t| *t != JumpTask::Idle) {
        // A jump is running: the window is gone (the original's teardown has started it).
        return;
    }
    if menu.phase != crate::menu::Phase::Closed {
        // The journal (opened by the window's triangle) holds the pad until it closes.
        return;
    }
    let Some(sora) = cursor.single().ok() else { return };
    // App-only escape (not in the original): Backspace with no window up returns to the area the map was entered from (Twilight Town
    // at its first area when that is not known), so the player is never stuck on the map.
    if ui.window.is_none() && keys.just_pressed(KeyCode::Backspace) {
        let dest = ui.from.filter(app_has).unwrap_or(Dest::new(area::TWILIGHT_TOWN, 1, 0));
        if let Some(t) = task.as_deref_mut().filter(|t| **t == JumpTask::Idle) {
            warn!("worldmap: escape key (app only): back to world {} area {} entrance {}", dest.world, dest.area, dest.entrance);
            *t = JumpTask::FadeOut { dest, kind: 1, t: 0.0 };
            keys.reset_all();
            return;
        }
    }
    let confirm = keys.just_pressed(KeyCode::KeyJ) || mouse.just_pressed(MouseButton::Left);
    let cancel = keys.just_pressed(KeyCode::Escape) || keys.just_pressed(KeyCode::Backspace) || keys.just_pressed(KeyCode::KeyK);
    let up = keys.just_pressed(KeyCode::ArrowUp) || keys.just_pressed(KeyCode::KeyW);
    let down = keys.just_pressed(KeyCode::ArrowDown) || keys.just_pressed(KeyCode::KeyS);
    let left = keys.just_pressed(KeyCode::ArrowLeft) || keys.just_pressed(KeyCode::KeyA);
    let right = keys.just_pressed(KeyCode::ArrowRight) || keys.just_pressed(KeyCode::KeyD);
    // The switch button [inferred keys: E, or Tab].
    let switch = keys.just_pressed(KeyCode::KeyE) || keys.just_pressed(KeyCode::Tab);
    // The journal button, the triangle: Y, the key the menu pages use for it.
    let journal = keys.just_pressed(KeyCode::KeyY);
    let ui = &mut *ui;
    let dt = time.delta_secs();
    // The layouts' open timers: when the window's idle layout takes over its items are laid again for that phase.
    let mut redraw = false;
    if let (Some(p), Some(art), Some(panel)) = (ui.pool.as_mut(), ui.data.as_ref().and_then(|d| d.art.as_ref()), ui.panel) {
        p.info.tick(scene, art, panel, dt);
        redraw = p.window.tick(scene, art, panel, dt);
    }
    let Some(data) = ui.data.as_ref() else { return };
    // The story's lock is taken only to copy what is read; `ExitLocks::locked` takes it again.
    let snap = progress.as_ref().map(|p| Snap::of(&p.story(), &data.epsd, &data.watch));
    // Contact: the nearest visible symbol within the radius (docs/systems/worldmap-gummi/worldmap.md section 4).
    let at = |t: &Transform| Vec2::new(t.translation.x, t.translation.z);
    let symbols: Vec<(u8, Vec2)> = pieces.iter().filter(|(_, _, v)| **v != Visibility::Hidden).filter_map(|(p, t, _)| Some((symbol_world(&data.exe, p.object)?, at(t)))).collect();
    let near = touched(at(sora), &symbols, CONTACT_RADIUS * SCALE);
    if event.busy() {
        // A forced event plays (or has just ended): the map's buttons are not read (`FUN_002c6a40` takes the control and the pause menu).
        if redraw {
            draw_panels(ui, scene, &mut images, snap.as_ref());
        }
        return;
    }
    if let Some(w) = ui.window.as_mut() {
        // The window holds the pad: its presses are its own.
        let press = if up {
            Some(Press::Up)
        } else if down {
            Some(Press::Down)
        } else if left {
            Some(Press::Left)
        } else if right {
            Some(Press::Right)
        } else if switch {
            Some(Press::Switch)
        } else if journal {
            Some(Press::Journal)
        } else if confirm {
            Some(Press::Confirm)
        } else if cancel {
            Some(Press::Cancel)
        } else {
            None
        };
        keys.reset_all();
        mouse.reset_all();
        if let Some(press) = press {
            if ui.notice {
                // The box closes on any confirm or cancel (`FUN_002ca6a0`, sound 4).
                if matches!(press, Press::Confirm | Press::Cancel) {
                    ui.notice = false;
                    redraw = true;
                    sound(&mut se, SOUND_CANCEL);
                }
            } else {
                match w.press(press) {
                    Outcome::None => {}
                    Outcome::Moved => {
                        if uilog() {
                            info!("worldmap: window {:?} cursor row {} episode row {} scroll {}", w.mode, w.cursor, w.episode_cursor, w.scroll);
                        }
                        redraw = true;
                        sound(&mut se, SOUND_CURSOR);
                    }
                    Outcome::Switched => {
                        info!("worldmap: window mode {} ({:?}, FUN_002ca3d0 / FUN_002ca538)", w.mode_number(), w.mode);
                        redraw = true;
                        sound(&mut se, SOUND_CONFIRM);
                    }
                    Outcome::Journal => {
                        let index = data.world_index.get(w.world as usize).copied();
                        info!(
                            "worldmap: the journal button (FUN_002c9510): world {} journal index {index:?}, FUN_0024f3c0(0, FUN_00277c88(world)) opens the journal scene",
                            w.world
                        );
                        open_journal.write(crate::menu_journal::OpenJournal { index: index.unwrap_or(-1) });
                    }
                    Outcome::Locked => {
                        info!("worldmap: row {} is locked (FUN_00195358): message {NOT_ACCESSIBLE:#x}", w.cursor);
                        ui.notice = true;
                        redraw = true;
                        sound(&mut se, SOUND_DENIED);
                    }
                    Outcome::Close => {
                        info!("worldmap: window closed without a landing (result 0)");
                        // The cancel result plays 4, the "Maybe later" row 2 (`FUN_002cacd8`).
                        sound(&mut se, if press == Press::Cancel { SOUND_CANCEL } else { SOUND_CONFIRM });
                        let mode = w.mode_number();
                        ui.window = None;
                        close_window(ui, scene, mode);
                        return;
                    }
                    Outcome::Land(dest) => {
                        if !app_has(&dest) {
                            info!("worldmap: landing in world {} area {} entrance {}: not in the app (only Twilight Town is); the window stays", dest.world, dest.area, dest.entrance);
                        } else if let Some(t) = task.as_deref_mut().filter(|t| **t == JumpTask::Idle) {
                            info!(
                                "worldmap: landing (result 1, FUN_002cc4b0): FUN_00104e18(world {} area {} entrance {}, 1, 0, 0, 0)",
                                dest.world, dest.area, dest.entrance
                            );
                            *t = JumpTask::FadeOut { dest, kind: 1, t: 0.0 };
                            sound(&mut se, SOUND_CONFIRM);
                            ui.window = None;
                            hide_all(ui, scene);
                            return;
                        }
                    }
                }
            }
        }
    } else {
        // The info panel follows contact (`FUN_002cf618` on contact, `FUN_002cf698` on leaving).
        if near != ui.near {
            ui.near = near;
            redraw = true;
            match near {
                Some(w) => info!("worldmap: the cursor touches world {w}: info panel (FUN_002cf618)"),
                None => info!("worldmap: the cursor left the symbol: info panel removed (FUN_002cf698)"),
            }
        }
        // Confirm on a symbol opens the window (`FUN_002cc1e0`); the key press is the window's, not an attack.
        if let (Some(world), true, Some(snap)) = (near, confirm, snap.as_ref()) {
            keys.reset_all();
            mouse.reset_all();
            // `FUN_002ccd88` first: a forced event starts and no prompt follows.
            let flag = |i: u32| progress.as_ref().is_some_and(|p| p.story().world_flag(i));
            if let Some(id) = symbol_object(&data.exe, world).and_then(|o| forced_event(o, flag)) {
                info!("worldmap: confirm on world {world}: forced event {id:#x} (FUN_002ccd88 -> FUN_002c6a40), no prompt; the info panel goes (FUN_002cf698)");
                event.request(id);
                ui.near = None;
                draw_panels(ui, scene, &mut images, Some(snap));
                return;
            }
            let place = |w: u8, a: u8| sys_place(scene, w, a);
            let localset = |w: u8, k: u16| crate::area_jump::disc_localset(&disc, w, k);
            match open_window(data, world, snap, &locks, place, localset) {
                Some(w) => {
                    info!("worldmap: confirm on world {world}: window mode {:?}, {} rows, {} episodes (FUN_002cbee8)", w.mode, w.rows.len(), w.episodes.len());
                    if uilog() {
                        for (i, r) in w.rows.iter().enumerate() {
                            info!("worldmap: row {i}: {:?} {:?} -> {:?} picture {:?}", plain(&r.tokens), r.tokens, r.dest, r.picture);
                        }
                    }
                    ui.window = Some(w);
                    ui.laid = None;
                    redraw = true;
                }
                None => info!("worldmap: confirm on world {world}: no window (its texts are missing)"),
            }
        }
    }
    if redraw || ui.laid.is_none() {
        draw_panels(ui, scene, &mut images, snap.as_ref());
    }
}

/// The map's menu sounds (`FUN_002d1760(k)` -> `FUN_002c3318(k)`: bank `k / 1000` = 0, entry `k % 1000` = k, `FUN_002c2258`): entries 1 to 5
/// of the system bank, the same sounds as the pause menu's cursor, confirm, open, cancel and refused (docs/systems/ui/menus-pause.md 1.7). They go through the
/// menu's own switch, `menu_sound::play`, which the user has muted (play-test report 2026-10-10): `KH2_SOUNDLOG=1` logs them as muted.
const SOUND_CURSOR: u32 = 1;
const SOUND_CONFIRM: u32 = 2;
const SOUND_CANCEL: u32 = 4;
const SOUND_DENIED: u32 = 5;

fn sound(se: &mut MessageWriter<PlaySe>, k: u32) {
    menu_sound::play(se, k);
}

/// Plays the window's close layout and clears its items.
fn close_window(ui: &mut MapUi, scene: &mut HudScene, mode: usize) {
    ui.notice = false;
    ui.laid = None;
    let (Some(p), Some(art), Some(panel)) = (ui.pool.as_mut(), ui.data.as_ref().and_then(|d| d.art.as_ref()), ui.panel) else { return };
    for s in &p.window_items {
        scene.nodes[*s].visible = false;
        scene.nodes[*s].label = None;
    }
    let close = art.layouts(mode)[1];
    p.window.close(scene, art, panel, close);
}

/// Hides every panel node (a landing starts: the original's teardown removes both objects).
fn hide_all(ui: &mut MapUi, scene: &mut HudScene) {
    ui.near = None;
    ui.laid = None;
    ui.notice = false;
    if let Some(p) = ui.pool.as_mut() {
        p.info.hide(scene);
        p.window.hide(scene);
        for s in p.info_items.iter().chain(&p.window_items) {
            scene.nodes[*s].visible = false;
            scene.nodes[*s].label = None;
        }
    }
}

/// Lays the info panel and the window on the scene for the current state.
fn draw_panels(ui: &mut MapUi, scene: &mut HudScene, images: &mut Assets<Image>, snap: Option<&Snap>) {
    if let Some(w) = ui.window.clone() {
        ensure_pictures(ui, scene, images, &w);
    }
    let (Some(data), Some(panel)) = (ui.data.as_ref(), ui.panel) else { return };
    let Some(art) = data.art.as_ref() else { return };
    let Some(pool) = ui.pool.as_mut() else { return };
    // The info panel (layouts of mode 0): the veil and title, the name plate and the lines.
    let [info_open, info_close, info_idle] = art.layouts(0);
    match ui.near {
        Some(world) => {
            let name = info_name(data, world, scene.text.as_ref().and_then(|t| t.messages.as_ref()));
            pool.info.show(scene, art, panel, info_open, info_idle);
            let mut items = vec![Item { file: panel, group: Some(NAME_PLATE[0]), next: Some(NAME_PLATE[1]), at: [0.0, 0.0], parent: Parent::Root, text: None }];
            let r = art.group_rect(NAME_PLATE[1]).unwrap_or([173.0, 11.0, 598.0, 72.0]);
            items.push(Item::text(panel, [r[0] + 40.0, r[1] + 22.0], Parent::Root, name, 0).sized(text_size(&art.group(NAME_PLATE[1]))));
            let facts = info_facts(&data.keys, &data.levels, world, |f| snap.is_some_and(|s| s.set_flags.contains(&f)));
            let extras = art.info_extras(panel, items.len(), &facts);
            items.extend(extras);
            apply(scene, &pool.info_items, &pool.info.slots, &items);
        }
        None => {
            for s in &pool.info_items {
                scene.nodes[*s].visible = false;
                scene.nodes[*s].label = None;
            }
            pool.info.close(scene, art, panel, info_close);
        }
    }
    let Some(w) = ui.window.as_ref() else {
        // Nothing more to lay until the contact or the window changes.
        ui.laid = Some((usize::MAX, 0, 0, 0, false));
        return;
    };
    let [open, _close, idle] = art.layouts(w.mode_number());
    pool.window.show(scene, art, panel, open, idle);
    let pics = ui.pictures.clone();
    let picture = move |n: u8| pics.get(&n).copied();
    let text = |id: u32| data.text(id);
    let set = snap.map(|s| s.set_flags.clone()).unwrap_or_default();
    let plates = Some(data.plate_texts(w.world, scene.text.as_ref().and_then(|t| t.messages.as_ref()), &|id| set.contains(&id)));
    let ctx = Ctx { panel, picture: &picture, text: &text, plates };
    let mut items = art.items(w, &ctx, pool.window.phase());
    if ui.notice {
        // The box stands over the list (`FUN_002ca6a0` makes it as another object).
        items.extend(art.notice_items(&ctx));
    }
    apply(scene, &pool.window_items, &pool.window.slots, &items);
    ui.laid = Some((w.mode_number(), w.cursor, w.episode_cursor, w.scroll, ui.notice));
}

/// The area's name through the HUD text system's place table.
fn sys_place(scene: &HudScene, world: u8, area: u8) -> Option<Vec<Token>> {
    scene.text.as_ref()?.place_name(WORLDS.get(world as usize)?, area as usize)
}

#[cfg(test)]
mod tests {
    use super::*;
    use kh2_image::seqd::AnimGroup;

    fn row(dest: Option<Dest>, locked: bool) -> Row {
        Row { tokens: vec![Token::Glyph(1)], dest, locked, picture: None }
    }

    fn window(mode: Mode, rows: Vec<Row>) -> Window {
        Window::new(2, mode, rows, Vec::new())
    }

    #[test]
    fn the_land_prompt_lands_on_row_zero_and_closes_on_row_one() {
        let dest = Dest::new(2, 1, 3);
        let mut w = window(Mode::Land, vec![row(Some(dest), false), row(None, false)]);
        assert_eq!(w.press(Press::Confirm), Outcome::Land(dest), "row 0 is result 1 (FUN_002cacd8)");
        assert_eq!(w.press(Press::Down), Outcome::Moved);
        assert_eq!(w.cursor, 1);
        assert_eq!(w.press(Press::Confirm), Outcome::Close, "row 1 is result 0");
        assert_eq!(w.press(Press::Down), Outcome::Moved);
        assert_eq!(w.cursor, 0, "the cursor wraps");
        assert_eq!(w.press(Press::Cancel), Outcome::Close, "the cancel result -2 closes");
        assert_eq!(w.press(Press::Switch), Outcome::None, "the land prompt has no switch button");
        assert_eq!(w.mode_number(), 0);
    }

    #[test]
    fn a_locked_row_shows_the_box_and_an_unlocked_one_lands() {
        let (a, b) = (Dest::new(2, 4, 0), Dest::new(2, 5, 1));
        let mut w = window(Mode::Areas, vec![row(Some(a), true), row(Some(b), false)]);
        assert_eq!(w.press(Press::Confirm), Outcome::Locked);
        assert_eq!(w.press(Press::Right), Outcome::Moved);
        assert_eq!(w.press(Press::Confirm), Outcome::Land(b));
        let mut one = window(Mode::Land, vec![row(None, false)]);
        assert_eq!(one.press(Press::Down), Outcome::None, "a single row does not move");
    }

    #[test]
    fn the_area_grid_has_two_columns_and_wraps() {
        let rows: Vec<Row> = (0..5).map(|i| row(Some(Dest::new(2, i, 0)), false)).collect();
        let mut w = window(Mode::Areas, rows);
        assert_eq!(w.press(Press::Down), Outcome::Moved);
        assert_eq!(w.cursor, 2, "down is a whole row");
        assert_eq!(w.press(Press::Down), Outcome::Moved);
        assert_eq!(w.cursor, 4);
        assert_eq!(w.press(Press::Down), Outcome::Moved);
        assert_eq!(w.cursor, 0, "past the last row it wraps to the top of the column");
        assert_eq!(w.press(Press::Up), Outcome::Moved);
        assert_eq!(w.cursor, 4, "up from the top wraps to the last card of the column");
        w.cursor = 1;
        assert_eq!(w.press(Press::Up), Outcome::Moved);
        assert_eq!(w.cursor, 3, "the odd column has its own last card");
        assert_eq!(w.press(Press::Left), Outcome::Moved);
        assert_eq!(w.cursor, 2);
        w.cursor = 4;
        assert_eq!(w.press(Press::Right), Outcome::Moved);
        assert_eq!(w.cursor, 0, "right wraps over the end of the list");
        assert_eq!(window(Mode::Areas, vec![row(None, false)]).press(Press::Left), Outcome::None, "one card does not move");
    }

    #[test]
    fn the_switch_button_enters_the_episode_list_and_back_is_mode_three() {
        let mut w = window(Mode::Areas, vec![row(Some(Dest::new(2, 1, 0)), false)]);
        assert_eq!(w.mode_number(), 1);
        assert_eq!(w.press(Press::Switch), Outcome::Switched, "mask 0x100 (FUN_002ca6a0)");
        assert_eq!((w.mode, w.mode_number()), (Mode::Episodes, 2));
        assert_eq!(w.press(Press::Confirm), Outcome::None, "the episode list does not confirm");
        assert_eq!(w.press(Press::Switch), Outcome::Switched, "mask 0x208 (FUN_002ca538)");
        assert_eq!((w.mode, w.mode_number()), (Mode::Areas, 3));
        assert_eq!(w.press(Press::Switch), Outcome::Switched, "mode 3 switches to the episode list again");
        assert_eq!(w.press(Press::Cancel), Outcome::Switched, "cancel from the episode list is also 0x208");
        assert_eq!(w.press(Press::Cancel), Outcome::Close, "cancel from the area list closes the window");
    }

    fn episodes(n: usize) -> Vec<EpisodeRow> {
        (0..n).map(|i| EpisodeRow { title: vec![Token::Glyph(i as u16)], hint: vec![], active: i == 0 }).collect()
    }

    #[test]
    fn the_episode_list_scrolls_three_rows_at_a_time() {
        let mut w = Window::new(2, Mode::Episodes, Vec::new(), episodes(5));
        let mut seen = Vec::new();
        for _ in 0..4 {
            assert_eq!(w.press(Press::Down), Outcome::Moved);
            seen.push((w.episode_cursor, w.scroll));
        }
        assert_eq!(seen, vec![(1, 0), (2, 0), (3, 1), (4, 2)], "the window follows the cursor");
        assert_eq!(w.press(Press::Down), Outcome::Moved);
        assert_eq!((w.episode_cursor, w.scroll), (0, 0), "past the end it wraps to the top");
        assert_eq!(w.press(Press::Up), Outcome::Moved);
        assert_eq!((w.episode_cursor, w.scroll), (4, 2), "and back to the end");
        let mut none = Window::new(2, Mode::Episodes, Vec::new(), Vec::new());
        assert_eq!(none.press(Press::Down), Outcome::None, "an empty list has nothing to move");
    }

    #[test]
    fn area_rows_keep_only_seen_points_of_the_world_and_drop_absent_areas() {
        let land = [
            wm::LandRow { world_point: 0, world: 2, row: 1, picture: 0 },
            wm::LandRow { world_point: 1, world: 2, row: 0, picture: 1 },
            wm::LandRow { world_point: 2, world: 2, row: 2, picture: 2 },
            wm::LandRow { world_point: 3, world: 4, row: 0, picture: 3 },
            wm::LandRow { world_point: 9, world: 2, row: 3, picture: 4 },
        ];
        let points = [
            wm::WorldPoint { world: 2, area: 4, entrance: 1 },
            wm::WorldPoint { world: 2, area: 5, entrance: 2 },
            wm::WorldPoint { world: 5, area: 1, entrance: 0 },
            wm::WorldPoint { world: 4, area: 0, entrance: 0 },
        ];
        let rows = area_rows(&land, &points, 2, |_| true, |_, a| a == 4, |d| d.world == 2);
        // Point 2 is another world's area the app lacks, point 9 has no record; the list is in row order and keeps each row's picture.
        assert_eq!(
            rows,
            vec![AreaRow { dest: Dest::new(2, 5, 2), locked: false, picture: 1 }, AreaRow { dest: Dest::new(2, 4, 1), locked: true, picture: 0 }]
        );
        assert!(area_rows(&land, &points, 2, |_| false, |_, _| false, |_| true).is_empty(), "no point seen: no list, the land prompt shows");
        assert!(app_has(&Dest::new(2, 1, 0)) && !app_has(&Dest::new(WORLD_MAP, 0, 0)) && !app_has(&Dest::new(4, 0, 0)));
    }

    #[test]
    fn the_landing_follows_the_state_byte_and_resolves_program_keys() {
        let records = [
            wm::Landing { world: 2, state: 0, area: 1, entrance: 7 },
            wm::Landing { world: 2, state: 1, area: 0x40, entrance: 9 },
        ];
        let none = |_: u8, _: u16| None;
        assert_eq!(land_destination(&records, 2, 0, none), Some(Dest::new(2, 1, 7)));
        let program = land_destination(&records, 2, 1, |w, k| (w == 2 && k == 0x40).then_some(11));
        assert_eq!(program, Some(Dest { world: 2, area: 11, entrance: 0, programs: [0x40; 3] }));
        assert_eq!(land_destination(&records, 2, 1, none), None, "a key the localset lacks has no destination");
        assert_eq!(land_destination(&records, 2, 5, none), None, "no record for the state");
        assert_eq!(land_destination(&records, 3, 0, none), None, "no record for the world");
        // The app's baseline has landing key 0: the first record stands in; a nonzero key without a record still has none.
        assert_eq!(land_destination_or_first(&records, 2, 0, none), Some((Dest::new(2, 1, 7), false)), "an exact record is not the fallback");
        let no_zero = [wm::Landing { world: 2, state: 4, area: 6, entrance: 8 }];
        assert_eq!(land_destination_or_first(&no_zero, 2, 0, none), Some((Dest::new(2, 6, 8), true)), "key 0 without a record takes the first");
        assert_eq!(land_destination_or_first(&no_zero, 2, 3, none), None);
        assert_eq!(land_destination_or_first(&records, 2, 1, |_, _| Some(3)), Some((Dest { world: 2, area: 3, entrance: 0, programs: [0x40; 3] }, false)));
        assert_eq!(land_destination_or_first(&records, 2, 5, none), None);
        assert_eq!(land_destination_or_first(&records, 3, 0, none), None);
    }

    #[test]
    fn contact_takes_the_nearest_symbol_in_reach_and_the_list_the_seen_episodes_in_table_order() {
        let symbols = [(2u8, Vec2::new(0.0, 0.0)), (4, Vec2::new(5.0, 0.0)), (6, Vec2::new(100.0, 0.0))];
        assert_eq!(touched(Vec2::new(4.0, 0.0), &symbols, 10.0), Some(4));
        assert_eq!(touched(Vec2::new(50.0, 0.0), &symbols, 10.0), None, "out of reach of every symbol");
        assert_eq!(touched(Vec2::new(0.0, 0.0), &[], 10.0), None);
        let eps = [
            wm::Episode { id: 3, world: 2, order: 0, title: 0x1e02, hint: 0x1e03 },
            wm::Episode { id: 4, world: 2, order: 1, title: 0x1e04, hint: 0x1e05 },
            wm::Episode { id: 5, world: 4, order: 0, title: 0x1e06, hint: 0x1e07 },
        ];
        assert_eq!(episodes_of(&eps, 2, |id| id == 4), vec![eps[1]]);
        assert!(episodes_of(&eps, 2, |_| false).is_empty(), "a fresh game has no episode");
        assert_eq!(episodes_of(&eps, 4, |_| true), vec![eps[2]]);
        let list = episodes_of(&eps, 2, |_| true);
        assert_eq!(list.iter().map(|e| e.id).collect::<Vec<_>>(), vec![3, 4], "the world's episodes in table order");
        assert!(episodes_of(&eps, 2, |id| id == 9).is_empty());
    }

    /// A synthetic `wm_ws.2ld`: sequence 0 has two groups, sequence 1 has 200 (card 0x37 steps 84 and 66, frame group 103 at
    /// (-82, 24), row group 0x34 steps 90).
    fn art() -> Art {
        let mut s1 = Seqd { groups: vec![AnimGroup::default(); 200], ..Seqd::default() };
        s1.groups[CARD_GROUP] = AnimGroup { unk20: 84, unk1c: 66, ..AnimGroup::default() };
        s1.groups[103] = AnimGroup { x: -82, y: 24, ..AnimGroup::default() };
        s1.groups[104] = AnimGroup { x: -82, y: 24, ..AnimGroup::default() };
        s1.groups[0x34] = AnimGroup { unk20: 90, ..AnimGroup::default() };
        s1.groups[0x3a] = AnimGroup { unk20: 36, ..AnimGroup::default() };
        let s0 = Seqd { groups: vec![AnimGroup::default(); 2], ..Seqd::default() };
        let layer = |group: usize| layd::Layer { kind: 1, seq: 1, group, x: 0, y: 0 };
        // Layout 0: open, 1: idle (mode 0); layout 3: open, 4: idle (mode 1); frame group 103 open and 104 idle.
        let layd = Layd {
            layers: vec![layer(3), layer(88), layer(4), layer(89), layer(103), layer(78), layer(104), layer(79)],
            layouts: vec![(0, 2), (2, 2), (0, 0), (4, 2), (6, 2)],
            seqs: vec![s0, s1],
        };
        Art {
            layd,
            image: imgd::Image { width: 1, height: 1, rgba: vec![0; 4], swizzled: false },
            ws: Seqd::default(),
            pictures: Vec::new(),
            modes: [[0, 1, 1], [3, 2, 4], [3, 2, 4], [3, 2, 4]],
            descs: Vec::new(),
            land_rows: [[0x3a, 0x3c, 0x3b], [0x3a, 0x3c, 0x39]],
            card_anchor: [2 + 103, 2 + 105, 2 + 104],
            episode_row: 0x34,
            scroll_groups: [0xffff, 0x3e, 0x3d, 0x3f],
            marker: [0x11, 1],
        }
    }

    fn ctx<'a>(picture: &'a dyn Fn(u8) -> Option<usize>, text: &'a dyn Fn(u32) -> Option<Vec<Token>>) -> Ctx<'a> {
        Ctx { panel: 7, picture, text, plates: None }
    }

    #[test]
    fn group_ids_count_the_groups_of_all_sequences() {
        let a = art();
        assert_eq!(a.locate(0), Some((0, 0)));
        assert_eq!(a.locate(1), Some((0, 1)));
        assert_eq!(a.locate(2), Some((1, 0)), "sequence 1 starts after sequence 0's two groups");
        assert_eq!(a.locate(2 + 103), Some((1, 103)));
        assert_eq!(a.locate(202), None, "past the last group");
        assert_eq!(a.layer_slot(4, 2 + 104), Some(0), "the idle frame is the first layer of layout 4");
        assert_eq!(a.layer_slot(3, 2 + 104), None, "the open layout shows the open group instead");
        assert_eq!(a.layer_slot(3, 2 + 103), Some(0));
        assert_eq!(a.layouts(2), [3, 2, 4]);
    }

    #[test]
    fn cards_sit_on_the_frame_layer_in_a_two_column_grid() {
        let a = art();
        let mut rows: Vec<Row> = (0..3).map(|i| Row { picture: Some(i), ..row(Some(Dest::new(2, i, 0)), i == 1) }).collect();
        rows[2].picture = Some(9);
        let mut w = window(Mode::Areas, rows);
        w.cursor = 2;
        let pic = |n: u8| (n < 5).then_some(100 + n as usize);
        let txt = |_: u32| None;
        let items = a.items(&w, &ctx(&pic, &txt), IDLE);
        let cards: Vec<&Item> = items.iter().filter(|i| i.group == Some(CARD_GROUP)).collect();
        assert_eq!(cards.len(), 3);
        assert!(cards.iter().all(|c| c.parent == Parent::Layer(0) && c.file == 7), "children of the frame layer, drawn from the panel file");
        assert_eq!(cards[0].at, [-82.0, 24.0], "the first card is at the frame group's x and y");
        assert_eq!(cards[1].at, [2.0, 24.0], "the second column is `unk20` further");
        assert_eq!(cards[2].at, [-82.0, 90.0], "the second row is `unk1c` lower");
        // Pictures: row 0 and 1 have one (files 100 and 101), row 2's picture number has no file.
        let thumbs: Vec<usize> = items.iter().filter(|i| i.group == Some(THUMBNAIL)).map(|i| i.file).collect();
        assert_eq!(thumbs, vec![100, 101]);
        assert_eq!(items.iter().filter(|i| i.group == Some(LOCKED_GROUP)).count(), 1, "one locked row, one marker");
        let outline = items.iter().filter(|i| i.group == Some(CARD_CURSOR)).collect::<Vec<_>>();
        assert_eq!(outline.len(), 1);
        assert_eq!(outline[0].at, cards[2].at, "the outline is on the cursor's card");
    }

    #[test]
    fn the_land_prompt_stacks_its_bars_and_selects_the_cursor_row() {
        let a = art();
        let mut w = window(Mode::Land, vec![row(Some(Dest::new(2, 1, 0)), false), row(None, false)]);
        w.cursor = 1;
        let (pic, txt) = (|_: u8| None, |_: u32| None);
        let items = a.items(&w, &ctx(&pic, &txt), IDLE);
        let plates: Vec<(Option<usize>, [f32; 2])> = items.iter().filter(|i| i.text.is_none()).map(|i| (i.group, i.at)).collect();
        assert_eq!(plates, vec![(Some(0x3b), [0.0, 0.0]), (Some(0x39), [0.0, 36.0])], "idle bar, then the selected bar one `unk20` lower");
        let texts: Vec<i32> = items.iter().filter(|i| i.text.is_some()).map(|i| i.at[1] as i32).collect();
        assert_eq!(texts, vec![-9, 27], "a line per row, one step apart (the synthetic bars have no drawn rectangle, so the centre is 0)");
    }

    #[test]
    fn the_episode_panel_shows_three_rows_with_markers_and_a_scroll_bar_for_more() {
        let a = art();
        let mut w = Window::new(2, Mode::Episodes, Vec::new(), episodes(4));
        let (pic, txt) = (|_: u8| None, |_: u32| Some(vec![Token::Glyph(5)]));
        let items = a.items(&w, &ctx(&pic, &txt), IDLE);
        assert_eq!((items[0].group, items[0].next), (Some(EPISODE_PANEL[0]), Some(EPISODE_PANEL[1])), "the panel opens, then idles");
        let rows: Vec<&Item> = items.iter().filter(|i| i.group == Some(0x34)).collect();
        assert_eq!(rows.iter().map(|r| r.at).collect::<Vec<_>>(), vec![[0.0, 0.0], [0.0, 90.0], [0.0, 180.0]], "three rows `unk20` apart");
        assert_eq!(items.iter().filter(|i| i.group == Some(0x11)).count(), 1, "only the active episode has the marker");
        assert!(items.iter().any(|i| i.group == Some(0x3d)), "four episodes need a scroll bar");
        w.scroll = 1;
        let items = a.items(&w, &ctx(&pic, &txt), IDLE);
        assert_eq!(items.iter().filter(|i| i.group == Some(0x11)).count(), 0, "scrolled past the active one");
        let three = Window::new(2, Mode::Episodes, Vec::new(), episodes(3));
        assert!(!a.items(&three, &ctx(&pic, &txt), IDLE).iter().any(|i| i.group == Some(0x3d)), "three rows fit without a bar");
    }

    #[test]
    fn a_layer_set_plays_the_open_layout_then_the_idle_one_and_closes_into_the_close_layout() {
        let mut a = art();
        // Group 103 plays once over 17 frames: the open layout takes 17 ticks.
        a.layd.seqs[1].anims = vec![kh2_image::seqd::Anim { frame_end: 16, ..Default::default() }];
        a.layd.seqs[1].groups[103] = AnimGroup { first: 0, count: 1, loops: 1, x: -82, y: 24, ..AnimGroup::default() };
        let mut scene = HudScene::default();
        scene.files.push(SeqFile { seq: a.layd.seqs[1].clone(), width: 1, height: 1, linear: Default::default(), nearest: Default::default() });
        let slots: Vec<usize> = (0..4).map(|_| scene.add(HudNode::new(0, None))).collect();
        let mut set = LayerSet { slots: slots.clone(), ..LayerSet::default() };
        assert!((LayerSet::play_time(&a, 3) - 17.0 / 60.0).abs() < 1e-6, "the longest limited group sets the time");
        set.show(&mut scene, &a, 0, 3, 4);
        assert_eq!(slots.iter().map(|s| scene.nodes[*s].player.group).collect::<Vec<_>>(), vec![Some(103), Some(78), None, None], "layout 3's layers");
        assert_eq!(set.phase(), OPEN);
        assert!(!scene.nodes[slots[2]].visible, "unused slots are hidden");
        let first = scene.nodes[slots[0]].player.group;
        set.show(&mut scene, &a, 0, 3, 4);
        assert_eq!(scene.nodes[slots[0]].player.group, first, "showing the same layout again does not restart it");
        assert!(!set.tick(&mut scene, &a, 0, 0.1), "still opening");
        assert_eq!(scene.nodes[slots[0]].player.group, Some(103));
        assert!(set.tick(&mut scene, &a, 0, 0.2), "the idle layout takes over and says so");
        assert_eq!(set.phase(), IDLE);
        assert_eq!(slots.iter().map(|s| scene.nodes[*s].player.group).collect::<Vec<_>>(), vec![Some(104), Some(79), None, None], "layout 4's layers replace layout 3's");
        set.close(&mut scene, &a, 0, 3);
        assert_eq!(scene.nodes[slots[0]].player.group, Some(103), "the close layout's groups play");
        assert!(!set.tick(&mut scene, &a, 0, 0.01));
        assert!(scene.nodes[slots[0]].visible, "still closing");
        set.tick(&mut scene, &a, 0, 1.0);
        assert!(!scene.nodes[slots[0]].visible && set.shown.is_none(), "gone when the close has played");
    }

    #[test]
    fn text_objects_hang_on_the_layer_their_group_names_in_the_layout_of_the_phase() {
        let mut a = art();
        // A descriptor (mode 1 set 0) on the frame: open id of group 103, idle id of group 104; message 0x4c15, and one with no layer in the layout.
        a.descs = vec![
            Desc { sets: [[2 + 103, 2 + 105, 2 + 104], [0, 0, 0], [0, 0, 0]], text: Some(0x4c15) },
            Desc { sets: [[2 + 150, 2 + 150, 2 + 150], [0; 3], [0; 3]], text: Some(0x4c11) },
        ];
        // Give the frame groups a drawn rectangle: one animation with one sprite group would be needed; the synthetic sequence draws nothing, so
        // no rectangle exists and no text is placed.
        let w = window(Mode::Areas, vec![row(Some(Dest::new(2, 1, 0)), false)]);
        let (pic, txt) = (|_: u8| None, |_: u32| Some(vec![Token::Glyph(1)]));
        assert!(a.text_items(&w, &ctx(&pic, &txt), IDLE).is_empty(), "a layer that draws nothing has no text");
        assert!(a.text_items(&window(Mode::Land, Vec::new()), &ctx(&pic, &txt), IDLE).is_empty(), "the land prompt has no descriptor texts");
        assert_eq!(a.layer_slot(a.layouts(1)[OPEN], a.descs[0].sets[0][OPEN]), Some(0), "open: the layer of group 103");
        assert_eq!(a.layer_slot(a.layouts(1)[IDLE], a.descs[0].sets[0][IDLE]), Some(0), "idle: the layer of group 104");
        assert_eq!(a.layer_slot(a.layouts(1)[IDLE], a.descs[1].sets[0][IDLE]), None, "a group that no layer shows");
    }

    #[test]
    fn apply_keeps_a_playing_group_and_parents_children_to_layers() {
        let mut scene = HudScene::default();
        scene.files.push(SeqFile { seq: Seqd { groups: vec![AnimGroup::default(); 10], ..Seqd::default() }, width: 1, height: 1, linear: Default::default(), nearest: Default::default() });
        let layer = scene.add(HudNode::new(0, None));
        let slots: Vec<usize> = (0..3).map(|_| scene.add(HudNode::new(0, None))).collect();
        let items = vec![
            Item { file: 0, group: Some(1), next: Some(2), at: [0.0, 0.0], parent: Parent::Root, text: None },
            Item::plate(0, 3, [5.0, 6.0], Parent::Layer(0)),
            Item::plate(0, 4, [1.0, 2.0], Parent::Item(0)),
        ];
        apply(&mut scene, &slots, &[layer], &items);
        assert_eq!(scene.nodes[slots[1]].parent, Some(layer));
        assert_eq!(scene.nodes[slots[1]].off, [5.0, 6.0]);
        assert_eq!(scene.nodes[slots[2]].parent, Some(slots[0]));
        // The open group has handed over to its follow-up: laying the same items again must not restart it.
        scene.nodes[slots[0]].player.set_group(&scene.files[0].seq, 2);
        apply(&mut scene, &slots, &[layer], &items);
        assert_eq!(scene.nodes[slots[0]].player.group, Some(2), "the follow-up group is still playing");
        apply(&mut scene, &slots, &[layer], &items[..1]);
        assert!(scene.nodes[slots[0]].visible && !scene.nodes[slots[1]].visible && !scene.nodes[slots[2]].visible, "items past the list are hidden");
    }

    /// A fresh game on the disc: no world point is seen, so a confirm on the Twilight Town symbol opens the land prompt
    /// (mode 0), whose row 0 returns the destination the `wlds` table gives (the world's first record, as the baseline's landing key is 0).
    #[test]
    fn a_fresh_games_twilight_town_land_prompt_returns_the_tables_entrance() {
        let Ok(iso) = std::env::var("KH2_ISO") else { return };
        let disc = Disc::open(std::path::Path::new(&iso));
        let data = Data::load(&disc).expect("the map's tables are on the disc");
        assert_eq!((data.wlds.len(), data.land.len(), data.points.len(), data.epsd.len()), (34, 53, 54, 43), "the documented counts");
        let story = crate::progress::Story::default();
        let snap = Snap::of(&story, &data.epsd, &data.watch);
        let world = area::TWILIGHT_TOWN;
        let state = snap.state(world);
        // The independent read: the first record of the world with this state byte (`FUN_001c1cf8`), straight from the bytes.
        let progress = disc.read("00progress.bin").expect("00progress.bin");
        let entry = kh2_bar::parse(&progress).unwrap().into_iter().find(|e| e.tag == wm::WLDS).unwrap().data.to_vec();
        let count = i32::from_le_bytes(entry[4..8].try_into().unwrap()) as usize;
        // A fresh state has key 0 and no record: the first record of the world stands in (`land_destination_or_first`).
        let raw = entry[8..].chunks_exact(4).take(count).find(|r| r[0] == world).expect("a Twilight Town record");
        assert!(raw[2] >= 0x33, "the first record names a story program, resolved through 07localset");
        let area = crate::area_jump::disc_localset(&disc, world, u16::from(raw[2])).expect("the program is in 07localset");
        assert_eq!(state, 0);
        let locks = ExitLocks::default();
        let w = open_window(&data, world, &snap, &locks, |_, _| None, |w, k| crate::area_jump::disc_localset(&disc, w, k)).expect("the window opens");
        assert_eq!(w.mode, Mode::Land, "no point is seen in a fresh game");
        assert_eq!(w.rows.len(), 2);
        assert_eq!(w.rows[0].dest, Some(Dest { world, area, entrance: 0, programs: [raw[2]; 3] }), "area from the localset, entrance 0 and the key as the program bytes (FUN_00198080)");
        assert!(!w.rows[0].tokens.is_empty() && w.rows[1].tokens == data.text(MAYBE_LATER).unwrap(), "row 0 is the land text, row 1 the cancel text");
        assert!(app_has(&w.rows[0].dest.unwrap()), "the app has the area, so the landing goes ahead");

        // The save point at the Usual Spot (world point 47, world 2 area 2) marks that point seen: Twilight Town now has a
        // destination row, so the confirm opens the area list (mode 1) with the point's own record as destination.
        let mut seen = story.clone();
        seen.set_world_point(47);
        let snap = Snap::of(&seen, &data.epsd, &data.watch);
        let point = data.points[47];
        assert_eq!((point.world, point.area), (2, 2), "point 47 is the Usual Spot");
        let list = open_window(&data, world, &snap, &locks, |_, _| Some(vec![Token::Glyph(7)]), |_, _| None).expect("the list opens");
        assert_eq!(list.mode, Mode::Areas);
        assert!(list.rows.iter().any(|r| r.dest == Some(Dest::new(2, 2, point.entrance)) && !r.locked && r.tokens == vec![Token::Glyph(7)] && r.picture.is_some()), "{:?}", list.rows);
    }

    /// The panel art on the disc (docs/systems/worldmap-gummi/worldmap.md section 5): `wm_ws.2ld` has the 11 layouts the mode table names, the sequence the panels use
    /// is the second one, every layer of the used layouts shows a group of it, the text descriptors name groups that exist, `ws_i` draws every
    /// one of the 53 pictures, and the layer a card hangs on is where the grid puts the cards inside the frame.
    #[test]
    fn the_panel_art_and_tables_on_the_disc_fit_together() {
        let Ok(iso) = std::env::var("KH2_ISO") else { return };
        let disc = Disc::open(std::path::Path::new(&iso));
        let data = Data::load(&disc).expect("the map's tables are on the disc");
        let art = data.art.as_ref().expect("the panel art loads");
        assert_eq!((art.layd.layouts.len(), art.layd.layers.len(), art.layd.seqs.len()), (11, 72, 2));
        assert_eq!(art.modes, [[0, 2, 1], [3, 5, 4], [6, 9, 8], [10, 5, 4]], "the mode table: open, close, idle");
        let seq = art.seq().expect("sequence 1");
        for layout in art.modes.iter().flatten() {
            assert!(!art.layd.layout(*layout).is_empty(), "layout {layout} has layers");
            assert!(art.layd.layout(*layout).iter().all(|l| l.seq == PANEL && l.group < seq.groups.len()), "layout {layout} draws sequence 1");
        }
        assert_eq!(art.episode_row, 0x34, "the episode row group of FUN_002cb508");
        assert_eq!(art.descs.len(), DESCRIPTOR_COUNT);
        assert_eq!(art.descs.iter().filter_map(|d| d.text).collect::<Vec<_>>(), vec![0x4c11, 0x4c14, 0x4c12, 0x4c13, 0x4c10, 0x4c15], "the six captions, in descriptor order");
        for d in &art.descs {
            for id in d.sets.iter().flatten() {
                assert!(art.locate(*id).is_some(), "group id {id:#x} exists");
            }
        }
        assert_eq!(art.pictures.len(), 53);
        assert_eq!(art.ws.groups.len(), 13, "ws_i");
        let pic = imgd::decode(&art.pictures[0]).expect("a picture decodes");
        assert_eq!((pic.width, pic.height), (128, 128));
        // The card grid: the frame layer of mode 1's idle layout is the card anchor group; two columns of `unk20` and rows of `unk1c`.
        let idle = art.layouts(1)[2];
        let frame = art.layer_slot(idle, art.card_anchor[2]).expect("the frame layer is in the idle layout");
        let g = art.group(art.layd.layout(idle)[frame].group);
        let card = art.group(CARD_GROUP);
        assert!(g.x < 0 && g.y > 0 && card.unk20 > 0 && card.unk1c > 0, "frame offset {} {}, card step {} {}", g.x, g.y, card.unk20, card.unk1c);
        let anchor = seq.anchor(art.layd.layout(idle)[frame].group, 0, &Ext::default()).expect("the frame has an anchor");
        let fr = art.layer_rect(&art.layd.layout(idle)[frame]).expect("the frame is drawn");
        let first = [anchor[0] as f32 + g.x as f32, anchor[1] as f32 + g.y as f32];
        let last_x = first[0] + card.unk20 as f32 + 80.0;
        assert!(first[0] >= fr[0] && last_x <= fr[2], "the two card columns ({} to {last_x}) fit in the frame ({} to {})", first[0], fr[0], fr[2]);
        assert!(first[1] >= fr[1] && first[1] + 4.0 * card.unk1c as f32 <= fr[3], "four card rows fit in the frame");
    }

    /// `FUN_002ca6a0`: the triangle (mask 0x400) opens the journal only from the area list; the land prompt (`FUN_002cacd8`) and the episode
    /// list do not read it.
    #[test]
    fn the_journal_button_answers_on_the_area_list_only() {
        let mut areas = window(Mode::Areas, vec![row(Some(Dest::new(2, 1, 0)), false)]);
        assert_eq!(areas.press(Press::Journal), Outcome::Journal);
        assert_eq!(areas.press(Press::Journal), Outcome::Journal, "it does not close or move anything");
        assert_eq!((areas.mode, areas.cursor), (Mode::Areas, 0));
        let mut land = window(Mode::Land, vec![row(Some(Dest::new(2, 1, 0)), false), row(None, false)]);
        assert_eq!(land.press(Press::Journal), Outcome::None);
        let mut eps = window(Mode::Episodes, Vec::new());
        assert_eq!(eps.press(Press::Journal), Outcome::None);
        assert_eq!(eps.mode, Mode::Episodes);
    }

    /// The info panel's extras (WP-WM7): the level label with two digits, the time label with the hours, a colon and two minute digits
    /// each one `unk20` apart on their label, and one box per key pair, the filled ones first.
    #[test]
    fn the_info_panel_hangs_digits_on_its_labels_and_draws_the_key_boxes() {
        let mut a = art();
        let seq = &mut a.layd.seqs[1];
        for d in 0..10 {
            seq.groups[LEVEL_DIGITS + d].unk20 = 16;
            seq.groups[TIME_DIGITS + d].unk20 = 16;
        }
        seq.groups[COLON].unk20 = 8;
        seq.groups[KEY_BOXES[0]] = AnimGroup { x: 140, size: 20, ..AnimGroup::default() };
        seq.groups[KEY_BOXES[1]] = AnimGroup { x: 140, size: 20, ..AnimGroup::default() };
        let facts = InfoFacts { level: 7, hours: 12, minutes: 5, shown: 3, filled: 1 };
        let items = a.info_extras(7, 2, &facts);
        let groups: Vec<Option<usize>> = items.iter().map(|i| i.group).collect();
        let g = |v: &[usize]| v.iter().map(|x| Some(*x)).collect::<Vec<_>>();
        assert_eq!(
            groups,
            [
                g(&[LEVEL_LABEL[0], LEVEL_DIGITS, LEVEL_DIGITS + 7, TIME_LABEL[0], TIME_DIGITS + 1, TIME_DIGITS + 2, COLON, TIME_DIGITS, TIME_DIGITS + 5]),
                g(&[KEY_BOXES[1], KEY_BOXES[0], KEY_BOXES[0]]),
            ]
            .concat(),
            "level 07, time 12:05, three boxes with the first filled"
        );
        assert_eq!(items[1].parent, Parent::Item(2), "the digits hang on the label (item 2 after the two the panel already has)");
        assert_eq!(items[2].at, [16.0, 0.0], "the next digit is one unk20 along");
        assert_eq!(items[3].parent, Parent::Root, "the label itself is a plate of its own");
        assert_eq!((items[5].at[0], items[6].at[0], items[7].at[0]), (16.0, 32.0, 40.0), "hours, colon after them, minutes after the colon's 8");
        assert_eq!(items[9].parent, Parent::Item(0), "the boxes hang on the name plate");
        assert_eq!((items[9].at[0], items[10].at[0], items[11].at[0]), (140.0 - 30.0, 140.0 - 20.0 - 30.0, 140.0 - 40.0 - 30.0), "x - size * i, the row shifted by half its width");
        // No key pair set: no box. Nothing special about a single digit of hours.
        let plain = a.info_extras(7, 2, &InfoFacts::default());
        assert_eq!(plain.len(), 8, "labels, 00, 0:00 and no box");
    }

    #[test]
    fn digits_are_the_last_places_with_zeros_in_front_and_the_hours_have_none() {
        assert_eq!(fixed_digits(7, 2), [0, 7]);
        assert_eq!(fixed_digits(255, 2), [5, 5], "only the last two places (FUN_002cc288 writes two)");
        assert_eq!(plain_digits(0), [0]);
        assert_eq!(plain_digits(105), [1, 0, 5]);
    }

    #[test]
    fn the_facts_come_from_the_story_flags_the_tables_name() {
        let keys = [wm::KeyBoxes { world: 2, pairs: [(10, 11), (12, 0), (0, 0)] }];
        let mut levels = vec![wm::LevelRow { bit: 0, flag: 20, levels: [0; wm::LEVEL_WORLDS] }, wm::LevelRow { bit: 1, flag: 21, levels: [0; wm::LEVEL_WORLDS] }];
        levels[0].levels[2] = 5;
        levels[1].levels[2] = 3;
        let f = info_facts(&keys, &levels, 2, |f| matches!(f, 10 | 11 | 12 | 20 | 21));
        assert_eq!((f.level, f.shown, f.filled), (8, 2, 1), "5 + 3, two boxes, one filled");
        let f = info_facts(&keys, &levels, 2, |f| f == 20);
        assert_eq!((f.level, f.shown, f.filled), (5, 0, 0));
        let f = info_facts(&keys, &levels, 4, |_| true);
        assert_eq!((f.level, f.shown), (0, 0), "a world with no row has no box and no level");
        assert_eq!((f.hours, f.minutes), (0, 0), "the app keeps no per-world clock");
    }

    /// The window copies the story out before the exit locks read it: both lock the same mutex, so holding the first while
    /// calling the second would hang the game (it did, once).
    #[test]
    fn the_story_lock_is_free_when_the_exit_locks_are_read() {
        let handle = crate::progress::StoryHandle::default();
        let locks = ExitLocks { table: Vec::new(), story: Some(handle.clone()) };
        let snap = Snap::of(&handle.lock(), &[], &[]);
        assert!(!locks.locked(2, 4), "returns instead of waiting on the lock");
        assert_eq!(snap.records.len(), WORLD_COUNT);
    }
}
