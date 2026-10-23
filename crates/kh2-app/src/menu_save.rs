//! The Save menu (menu kinds 7 and 8, `save.2ld`; docs/systems/ui/menus-save.md section 18).
//!
//! What the original does (`SLUS_210.05`): the field task `FUN_00138758` (or a save point's prompt, `FUN_001ad9a0`) calls
//! `FUN_0024f330(request block, party)`, which opens menu kind 7 (`FUN_002540c8` / `FUN_00254210`). Kind 7 is only the
//! wrapper: its task `FUN_00254380` runs kind 8 (`FUN_0026f420` / `FUN_0026f468`, task `FUN_0026f4a8`) as the page
//! machine, and starts the sub-menu the page asked for (`FUN_00254a08`: 2 = kind 2, the file screens of `save.2ld`). Kind 8's
//! page 0 (`FUN_0026fda0`) is a pointer list built from the request block (`FUN_0026f698`: a header group, an
//! information message, one row per option with its message and an action code); the code 1 row ("Save") leaves for the
//! file screens, which are kind 2's page machine (`FUN_002589f0`, task `FUN_00258cf8`).
//!
//! The port keeps the screens and their order: the pointer list with the one row the disc's texts have (the footage's
//! "Title Screen" row is a remaster text; no such message is on this disc), the information box while the files are
//! checked, the four-row file list, the overwrite question on an occupied row, the information box while the file is
//! written, then the menu closes (`FUN_0025a158` ends the kind once the write is done). Memory card calls are replaced by a
//! folder of slot files (`progress::save_dir`, four slots); the texts are the disc's, with the memory card slot number
//! filled in as the original does.
//!
//! The menu keeps its own nodes in the shared `MenuScene` and does not touch the camp's page state: while it is open
//! `Menu.phase` stays `Closed`, `SaveMenu::is_open` holds the field frozen and the pad.
//!
//! Save points (WP-E7, docs/systems/ui/menus-save.md 18.5): the accept of command 55 (`FUN_001784b8`) opens menu kind 1 (scene 3, init
//! `FUN_00255598`, task 0x2556f0, page machine `FUN_00255a10`), the points' own copy of the file screens with the same
//! memory card driver and `save.2ld` groups. Its top page (`FUN_00255c10`) lists the row messages at 0x380d02 + 0x24 *
//! row ("Save", and a second row only while the kind's mode word 0x36cd28 is not 2: the world point's); after a write
//! the list shows again (`FUN_00257880` state 0x2b: page 0xb) instead of closing. `SaveMenu::request_point` opens it.

use bevy::prelude::*;
use kh2_image::layd::{self, Layd};
use kh2_image::text::Style;

use super::item_pages::{fill, label, tokens_of};
use super::{at, close_brightness, dim_alpha, menu_sound, Menu, Phase, CLOSE_FRAMES, DIM_BRIGHTNESS};
use crate::area_jump::{map_entrance, Dest, ExitConsts, JumpMenus, JumpTask, FADE_TICKS, WORLD_MAP};
use crate::assets::{CurrentArea, Disc};
use crate::audio::PlaySe;
use crate::menu_gfx::{self, MenuScene, Node};
use crate::progress::{self, Progress, SlotInfo, SLOTS};
use crate::schedule::Pause;
use crate::sora::{Sora, State};

/// Messages (`msg/us/sys.bar`; bit 15 of a stored id is ignored): the row "Save" (`0xa7e`), Yes and No (`0xa7c`, `0xa7d`),
/// the help of the pointer list (`0x3858` "Save data."), the check box (`0xa89`), "Select file to save." (`0xaa1`), "Save
/// new file." (`0xa9d`), the overwrite question (`0xaa4`, with its QUESTION header), the write boxes (`0xa90` saving,
/// `0xa92` overwriting), the memory card slot name (`0xa86`), LV (`0x39fc`) and Munny (`0x37db`).
mod msg {
    pub const SAVE: u32 = 0xa7e;
    pub const YES: u32 = 0xa7c;
    pub const NO: u32 = 0xa7d;
    pub const HELP_TOP: u32 = 0x3858;
    pub const CHECKING: u32 = 0xa89;
    pub const HELP_FILES: u32 = 0xaa1;
    pub const NEW_FILE: u32 = 0xa9d;
    pub const OVERWRITE_ASK: u32 = 0xaa4;
    /// The World Map question (`FUN_00257ee8`, page 0x10): "Jump to World Map?" (`0x3859`; its Yes and No are the same `0xa7c`, `0xa7d`).
    pub const MAP_ASK: u32 = 0x3859;
    pub const SAVING: u32 = 0xa90;
    pub const OVERWRITING: u32 = 0xa92;
    pub const LV: u32 = 0x39fc;
    pub const MUNNY: u32 = 0x37db;
    /// Kind 2's own texts (docs/systems/ui/menus-save.md 18.6): the header "Load" (`FUN_00258e80`: `FUN_0029d1d0(0, 0x8a7f, 0)`), the file
    /// list's help "Select file to load." (`FUN_0025a858`, page 0xb: `FUN_0025ed20(0x8aa2, 1)`), the loading box (page 0xb's
    /// row choice: `FUN_002bc510(0x8a91, ..)`, page 0xf), the failure box (`FUN_00259e88`: `0x8a8d`, page 6) and an empty
    /// row's "There is no data." (`0x8a9f` [inferred: the row's draw was not read]).
    pub const LOAD: u32 = 0xa7f;
    pub const HELP_LOAD: u32 = 0xaa2;
    pub const LOADING: u32 = 0xa91;
    pub const LOAD_FAILED: u32 = 0xa8d;
    pub const NO_DATA: u32 = 0xa9f;
}

/// Animation groups of `save.2ld` sequence 0 (every layer of the three layouts and every object the pages build; the
/// disc test `the_save_groups_exist` checks they exist). The pictures behind the numbers are in the dump of
/// `crates/kh2-image/tests/menu_save.rs` and the doc section 18.
mod g {
    /// The frame, layouts 0, 1, 2 (enter, hold, leave): backdrop 7/8/9, header with the SAVE word 20/21/22, area rule
    /// 69/70/71, side rail 18 (hold only).
    pub const MUNNY: [usize; 3] = [66, 67, 68];
    pub const TIME: [usize; 3] = [72, 73, 74];
    pub const MUNNY_DIGIT: usize = 27;
    pub const TIME_DIGIT: usize = 37;
    pub const COLON: usize = 47;
    /// The pointer list row, selected (red, outline and glint). Idle rows (enter 78, hold 79, leave 80) appear only on menu
    /// kind 1's two-row page (its row records at 0x380cf0 name 78/80/79 idle and 78/80/77 selected).
    pub const ROW_SELECTED: usize = 77;
    pub const ROW_IDLE: usize = 79;
    /// The hand.
    pub const HAND: usize = 12;
    /// The "Save" tab under the header while the files show: the red plate (loops).
    pub const TAB: usize = 0;
    /// The information / question box: enter, hold.
    pub const BOX: [usize; 2] = [4, 5];
    /// Yes / No pills: selected, idle.
    pub const PILL_SELECTED: usize = 0;
    pub const PILL_IDLE: usize = 1;
    /// The file panel: enter, hold.
    pub const PANEL: [usize; 2] = [81, 82];
    /// A file row: selected (loops), idle (enter, hold).
    pub const FILE_SELECTED: usize = 84;
    pub const FILE_IDLE: [usize; 2] = [85, 86];
    /// The scroll bar (hold) and its two arrows (hold).
    pub const BAR: usize = 55;
    pub const ARROW_UP: usize = 61;
    pub const ARROW_DOWN: usize = 64;
}

/// Placements in menu units (measured on the footage and the layout dump, [inferred] where the code gives none).
mod pos {
    /// The pointer list row (group 77 / 78 at its own origin) and its pitch (`unk20` of the group).
    pub const ROW: [f32; 2] = [0.0, 0.0];
    pub const ROW_LABEL: [f32; 2] = [70.0, 80.0];
    pub const ROW_PITCH: f32 = 26.0;
    pub const HAND: [f32; 2] = [62.0, 80.0];
    /// The tab under the header and its text.
    pub const TAB: [f32; 2] = [35.0, 41.0];
    pub const TAB_LABEL: [f32; 2] = [62.0, 41.0];
    /// The box (its group draws at the origin) and where its text starts.
    pub const BOX_TEXT: [f32; 2] = [99.0, 122.0];
    /// Yes and No pills, and the centre of their text.
    pub const PILLS: [[f32; 2]; 2] = [[193.0, 225.0], [324.0, 225.0]];
    pub const PILL_TEXT_DX: f32 = 46.0;
    /// File rows: pitch (`unk20` of group 84), the row's first top, ring centre, the text columns.
    pub const FILE_PITCH: f32 = 60.0;
    pub const FILE_TOP: f32 = 77.0;
    pub const FILE_NUMBER: f32 = 49.0;
    pub const FILE_LOGO: [f32; 2] = [58.0, 10.0];
    pub const FILE_LV: f32 = 168.0;
    pub const FILE_MUNNY_RIGHT: f32 = 340.0;
    pub const FILE_TIME_RIGHT: f32 = 468.0;
    pub const FILE_LINE1: f32 = 12.0;
    pub const FILE_LINE2: f32 = 40.0;
    /// Scroll bar arrows.
    pub const ARROWS: [[f32; 2]; 2] = [[486.5, 86.0], [486.5, 312.0]];
}

/// Ticks the check box shows before the list (the footage shows it for about a second [F]; the memory card check's own
/// time is not a number the code holds) and the write box before the file is written.
const CHECK_TICKS: u32 = 45;
const WRITE_TICKS: u32 = 45;

/// The logo group of `save.2ld` sequence 2 for a world code (the list rows show the world's logo). [inferred] from the
/// pictures: group 17 is Twilight Town; the others follow the world names of the sequence, by the common two-letter codes.
pub fn logo_group(world: &str) -> usize {
    match world {
        "al" => 1,
        "bb" => 2,
        "po" => 3,
        "dc" => 4,
        "nm" => 14,
        "hb" | "hb2" => 8,
        "he" => 10,
        "lk" => 11,
        "lm" => 12,
        "mu" => 13,
        "ca" => 3,
        "pp" => 16,
        "tr" => 18,
        "tt" | "eh" | "wi" => 17,
        _ => 17,
    }
}

/// The stages of the menu.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Stage {
    #[default]
    Closed,
    /// The frame's enter animations play.
    Opening,
    /// The pointer list (kind 8 page 0).
    Top,
    /// The information box while the files are checked (kind 2 page 2).
    Checking,
    /// The file list.
    Files,
    /// The overwrite question.
    Question,
    /// The information box while the file is written.
    Writing,
    /// Load mode: the information box while the file is read (kind 2 page 0xf, states 0x32..0x34).
    Reading,
    /// Load mode: the failure box (kind 2 page 6); a confirm or cancel returns to the file list.
    Failed,
    Closing,
    /// The World Map question was answered Yes: the screen fader goes to black over the open menu (`FUN_002580b8`, then page 0x14
    /// `FUN_002580e8` waits for the fader's state 4).
    MapFade,
}

/// One key press, as the flow takes it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Press {
    Up,
    Down,
    Confirm,
    Cancel,
}

/// What a press asks the scene to do.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    None,
    Cursor,
    /// Leave the pointer list for the file screens (confirm on the Save row).
    OpenFiles,
    /// Back from the file list to the pointer list.
    BackToTop,
    /// Ask the overwrite question for this slot.
    Ask(usize),
    /// Write this slot now.
    Write(usize),
    /// The question's No (or cancel): back to the list.
    Declined,
    /// Close the whole menu.
    Close,
    /// Menu kind 1's second row, "World Map" (`FUN_00255d38` row 1 -> `FUN_00257e70`): ask the question of page 0x10.
    SecondRow,
    /// The World Map question's Yes (`FUN_00257ee8` result 1): `FUN_002580b8` fades to black, page 0x14 waits for it.
    MapGo,
    /// Load mode: read this slot now (page 0xb's row choice goes straight to the loading box, no question).
    Read(usize),
    /// Load mode: the row holds no file; nothing happens (`FUN_0023ddb0` is 0).
    Empty,
    /// Load mode: leave the file list without loading (the list's cancel, which the original sends to the slot page and out).
    Leave,
    /// Load mode: the failure box is acknowledged.
    FailedOk,
}

/// The state machine apart from the scene (tested without a disc).
#[derive(Clone, Debug, Default)]
pub struct Flow {
    pub stage: Stage,
    /// The file list's cursor and the question's (0 Yes, 1 No).
    pub file: usize,
    pub ask: usize,
    /// The slot the question or the write is about.
    pub slot: usize,
    /// Rows of the top page (0 counts as 1: the field menu's one "Save" row) and its cursor.
    pub rows: usize,
    pub row: usize,
    /// Menu kind 1: after a write the file list shows again (`FUN_00257880` state 0x2b sets page 0xb).
    pub relist: bool,
    /// Menu kind 2, the Load screen (docs/systems/ui/menus-save.md 18.6): no pointer list, no question, a row reads its file.
    pub load: bool,
    /// The question up is the World Map one (page 0x10), not the overwrite question: its No and cancel return to the top list.
    pub map: bool,
}

impl Flow {
    /// A press in the current stage. `occupied[i]`: slot `i` holds a file.
    pub fn press(&mut self, p: Press, occupied: &[bool; SLOTS]) -> Action {
        let rows = self.rows.max(1);
        if self.load {
            return self.press_load(p, occupied);
        }
        match (self.stage, p) {
            // The top list moves only with more than one row; it wraps [inferred: the list widget `FUN_002967b8(list, 4, 1)`].
            (Stage::Top, Press::Up | Press::Down) if rows > 1 => {
                self.row = if p == Press::Up { (self.row + rows - 1) % rows } else { (self.row + 1) % rows };
                Action::Cursor
            }
            (Stage::Top, Press::Confirm) if self.row == 0 => Action::OpenFiles,
            (Stage::Top, Press::Confirm) => Action::SecondRow,
            (Stage::Top, Press::Cancel) => Action::Close,
            (Stage::Files, Press::Up) => {
                self.file = (self.file + SLOTS - 1) % SLOTS;
                Action::Cursor
            }
            (Stage::Files, Press::Down) => {
                self.file = (self.file + 1) % SLOTS;
                Action::Cursor
            }
            (Stage::Files, Press::Cancel) => Action::BackToTop,
            (Stage::Files, Press::Confirm) => {
                self.slot = self.file;
                if occupied[self.file] {
                    self.ask = 1;
                    Action::Ask(self.file)
                } else {
                    Action::Write(self.file)
                }
            }
            (Stage::Question, Press::Up | Press::Down) => {
                // Yes and No lie side by side: a vertical press does nothing (the pills answer Left and Right in the original's list).
                Action::None
            }
            (Stage::Question, Press::Cancel) => Action::Declined,
            (Stage::Question, Press::Confirm) => {
                if self.map {
                    if self.ask == 0 { Action::MapGo } else { Action::Declined }
                } else if self.ask == 0 {
                    Action::Write(self.slot)
                } else {
                    Action::Declined
                }
            }
            _ => Action::None,
        }
    }

    /// A press in the Load screen: the file list (kind 2 page 0xb, `FUN_00259b78`) and the failure box (page 6, `FUN_00259288`).
    fn press_load(&mut self, p: Press, occupied: &[bool; SLOTS]) -> Action {
        match (self.stage, p) {
            (Stage::Files, Press::Up) => {
                self.file = (self.file + SLOTS - 1) % SLOTS;
                Action::Cursor
            }
            (Stage::Files, Press::Down) => {
                self.file = (self.file + 1) % SLOTS;
                Action::Cursor
            }
            (Stage::Files, Press::Cancel) => Action::Leave,
            (Stage::Files, Press::Confirm) => {
                self.slot = self.file;
                if occupied[self.file] { Action::Read(self.file) } else { Action::Empty }
            }
            (Stage::Failed, Press::Confirm | Press::Cancel) => Action::FailedOk,
            _ => Action::None,
        }
    }

    /// Left and right on the question's pills.
    pub fn side(&mut self, left: bool) -> bool {
        if self.stage != Stage::Question {
            return false;
        }
        let new = if left { 0 } else { 1 };
        let moved = new != self.ask;
        self.ask = new;
        moved
    }
}

/// The nodes of the menu.
#[derive(Default)]
struct Nodes {
    layers: Vec<usize>,
    munny: usize,
    time: usize,
    munny_digits: Vec<usize>,
    time_digits: Vec<usize>,
    area: usize,
    help: usize,
    row: usize,
    row_text: usize,
    /// Menu kind 1's second row.
    row1: usize,
    row1_text: usize,
    hand: usize,
    tab: usize,
    tab_text: usize,
    boxn: usize,
    box_text: usize,
    pills: [usize; 2],
    pill_text: [usize; 2],
    pill_hand: usize,
    panel: usize,
    bar: usize,
    arrows: [usize; 2],
    files: Vec<FileNodes>,
    file_hand: usize,
}

#[derive(Default, Clone, Copy)]
struct FileNodes {
    plate: usize,
    number: usize,
    logo: usize,
    lv: usize,
    munny: usize,
    time: usize,
    area: usize,
}

#[derive(Resource, Default)]
pub struct SaveMenu {
    flow: Flow,
    loaded: bool,
    layd: Layd,
    /// The second layer set of `save.2ld` (the entry named `load`, which `FUN_00258bf0` picks with `FUN_0023c438(file, 1)`) and the
    /// scene index of its own sequence 0: the Load screen's header word and panels.
    load_layd: Option<Layd>,
    load_seq: usize,
    nodes: Nodes,
    /// Index of the first of `save.2ld`'s three textures and sequences in `MenuScene`.
    tex_base: usize,
    seq_base: usize,
    /// 60 Hz ticks since the stage began.
    frames: u32,
    /// The slots as the folder holds them, read when the list shows.
    slots: [Option<SlotInfo>; SLOTS],
    dir: std::path::PathBuf,
    /// Real seconds of play at the open (the Time plate and a slot's time).
    play: f32,
    /// Set by the input system for the `tick` of the same frame.
    area: String,
    /// Menu kind 1's top rows as the executable lists them (`point_rows`).
    point_rows: Vec<u32>,
    /// The rows of the open menu (the field menu: `[msg::SAVE]`).
    rows: Vec<u32>,
    /// A save point's request (the kind's mode word), opened by the next `input`.
    request: Option<i32>,
    /// The open menu is kind 1 with this mode.
    point: Option<i32>,
    /// The Load screen (menu kind 2) is requested; opened by the next `input`.
    load_request: bool,
    /// What the file read produced; moved to `outcome` when the menu has closed.
    pending: Option<Loaded>,
    /// How the last Load screen ended; taken by whoever requested it (`take_outcome`).
    outcome: Option<LoadOutcome>,
}

/// What a successful load gives the caller: the area the slot was written in and Sora's HP and MP from its party record.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Loaded {
    pub dest: Dest,
    /// HP = the smaller of record `+4` and `+5`, MP = the smaller of `+6` and `+7` (the rule `FUN_0019f920` follows when it
    /// rebuilds a status from the record, docs/systems/ui/menus-gameover.md 26.3); `None` when the record holds no maximum (an empty block).
    pub hp_mp: Option<(i32, i32)>,
}

/// The end of a Load screen.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LoadOutcome {
    /// The state was replaced; enter the area (`FUN_00103b38`'s mode 2).
    Loaded(Loaded),
    /// The file list was cancelled; nothing changed.
    Left,
}

/// The destination of a slot's area code: only Twilight Town exists in the app. The entrance is the one the slot file records
/// (the Save menu writes the area record's entrance; a file without it keeps entrance 0 [inferred]).
pub fn dest_of(area_code: &str, entrance: u8) -> Result<Dest, String> {
    let world = area_code.get(..2).ok_or_else(|| format!("area code {area_code:?} is too short"))?;
    if world != "tt" {
        return Err(format!("area {area_code}: the app has Twilight Town only"));
    }
    let area = crate::area::area_number(area_code).ok_or_else(|| format!("area code {area_code:?} has no number"))?;
    Ok(Dest::new(crate::area::TWILIGHT_TOWN, area, entrance))
}

/// Reads slot `n` of `dir` and applies it to a copy of `live`. Nothing of `live` changes: the caller replaces its state only
/// on `Ok`, so an empty, unreadable or malformed file can never corrupt the running game.
pub fn load_slot(dir: &std::path::Path, n: usize, live: &Progress) -> Result<(Progress, Loaded), String> {
    let text = std::fs::read_to_string(progress::slot_path(dir, n)).map_err(|e| format!("slot {} is not readable: {e}", n + 1))?;
    let info = Progress::slot_info(&text).ok_or_else(|| format!("slot {} has no row data (not a slot file)", n + 1))?;
    let dest = dest_of(&info.area, Progress::slot_entrance(&text).unwrap_or(0))?;
    let mut next = live.clone();
    next.apply_json(&text).map_err(|e| format!("slot {}: {e}", n + 1))?;
    let m = next.sora();
    let hp_mp = (m.hp_max > 0).then(|| (i32::from(m.hp.min(m.hp_max)), i32::from(m.mp.min(m.mp_max))));
    Ok((next, Loaded { dest, hp_mp }))
}

impl SaveMenu {
    /// The Load screen (menu kind 2, docs/systems/ui/menus-save.md 18.6) from the game-over list's "Load Game" row (`FUN_00254a08(2)`).
    /// False when the menu cannot open (no `save.2ld`, or a menu is up).
    pub fn request_load(&mut self) -> bool {
        if !self.loaded || self.is_open() || self.request.is_some() || self.load_request {
            return false;
        }
        self.load_request = true;
        true
    }

    /// How the Load screen ended, once (None while it is open or when none was requested).
    pub fn take_outcome(&mut self) -> Option<LoadOutcome> {
        self.outcome.take()
    }

    /// Test hook: the Load screen has ended with this outcome.
    #[cfg(test)]
    pub fn end_load_for_test(&mut self, o: LoadOutcome) {
        self.outcome = Some(o);
    }

    /// While the menu is up it owns the pad and the field is frozen.
    pub fn is_open(&self) -> bool {
        self.flow.stage != Stage::Closed
    }

    /// `FUN_0024f0b8(1, mode)` from a save point's accept (`FUN_001784b8`): mode 2 for the save point, 0 for the world
    /// point. False when the menu cannot open (no `save.2ld`, or a menu is up).
    pub fn request_point(&mut self, mode: i32) -> bool {
        if !self.loaded || self.is_open() || self.request.is_some() {
            return false;
        }
        self.request = Some(mode);
        true
    }
}

/// `FUN_00255c10`: the top page of menu kind 1 has two rows; row `r`'s message is the u16 at 0x380d02 + 0x24 * r (bit 15 is
/// ignored by the text lookup). Empty when the executable is not readable.
pub fn point_rows(exe: &[u8]) -> Vec<u32> {
    (0..2u32)
        .map_while(|r| {
            let va = 0x0038_0d02 + 0x24 * r;
            crate::struggle::exe_word(exe, va & !3).map(|w| if va & 2 != 0 { w >> 16 } else { w & 0xffff })
        })
        .collect()
}

/// The rows menu kind 1 shows for its mode: the second only while the mode is not 2 (`FUN_00255c10` stops at row 1 when
/// `FUN_0024f498()`, the mode word, is 2).
pub fn rows_for_mode(rows: &[u32], mode: i32, map: bool) -> Vec<u32> {
    rows.iter().take(if mode == 2 || !map { 1 } else { 2 }).copied().collect()
}

/// Before the fixed steps: whether `request_point` would take a save point's request now (`SavePointRequest::open`).
fn point_gate(save: Res<SaveMenu>, mut req: ResMut<crate::shared::ui::SavePointRequest>) {
    req.open = save.loaded && !save.is_open() && save.request.is_none();
}

/// After the fixed steps: the request a save point's accept made (`savepoint::point_use`) becomes the menu's own.
fn point_take(mut save: ResMut<SaveMenu>, mut req: ResMut<crate::shared::ui::SavePointRequest>) {
    if let Some(mode) = req.mode.take() {
        save.request_point(mode);
    }
}

pub struct SavePlugin;

impl Plugin for SavePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<SaveMenu>()
            .init_resource::<crate::shared::ui::SavePointRequest>()
            .add_systems(RunFixedMainLoop, (point_gate.in_set(RunFixedMainLoopSystems::BeforeFixedMainLoop), point_take.in_set(RunFixedMainLoopSystems::AfterFixedMainLoop)))
            .add_systems(Startup, load.after(super::load))
            .add_systems(PreUpdate, input.before(super::input).after(bevy::input::InputSystems).after(crate::harness::Keys))
            .add_systems(Update, tick.before(crate::schedule::Frame::Popups));
    }
}

fn load(disc: Res<Disc>, mut images: ResMut<Assets<Image>>, mut scene: ResMut<MenuScene>, mut save: ResMut<SaveMenu>) {
    let Some(two) = disc.read("menu/us/save.2ld").and_then(|b| layd::parse_2ld(&b)) else {
        warn!("menu: menu/us/save.2ld is not readable; the Save menu is off");
        return;
    };
    if scene.text.is_none() || two.images.len() < 3 || two.layd.seqs.len() < 3 {
        warn!("menu: the Save menu needs the menu text and the three images of save.2ld; it is off");
        return;
    }
    save.tex_base = scene.texs.len();
    for img in &two.images {
        scene.texs.push(menu_gfx::Tex::new(img, &mut images));
    }
    save.seq_base = scene.seqs.len();
    let Layd { layers, layouts, seqs } = two.layd;
    scene.seqs.extend(seqs);
    save.layd = Layd { layers, layouts, seqs: Vec::new() };
    if let Some(l) = disc.read("menu/us/save.2ld").and_then(|b| load_layd(&b)) {
        save.load_seq = scene.seqs.len();
        let Layd { layers, layouts, mut seqs } = l;
        scene.seqs.push(seqs.swap_remove(0));
        save.load_layd = Some(Layd { layers, layouts, seqs: Vec::new() });
    } else {
        warn!("menu: save.2ld has no second layer set (`load`); the Load screen uses the Save screen's header");
    }
    save.dir = progress::save_dir();
    save.nodes = build_nodes(&mut scene, save.tex_base, save.seq_base);
    save.point_rows = disc.read_root("SLUS_210.05").map(|e| point_rows(&e)).unwrap_or_default();
    if save.point_rows.is_empty() {
        warn!("menu: menu kind 1's rows are not readable from SLUS_210.05; a save point opens the field menu's one row");
    }
    save.loaded = true;
}

/// The second layer set of `save.2ld`: the BAR entries of type 0x1c are `save` and `load` (`FUN_0023c438(file, n)` takes the n-th of
/// them, `FUN_002557a8` builds with 0 and `FUN_00258bf0` with 1). `None` unless the entry parses and its sequence 0 exists.
pub(crate) fn load_layd(file: &[u8]) -> Option<Layd> {
    let entries = kh2_bar::parse(file).ok()?;
    let e = entries.iter().filter(|e| e.kind == 28).nth(1)?;
    let l = layd::parse_layd(e.data)?;
    (!l.seqs.is_empty()).then_some(l)
}

fn build_nodes(scene: &mut MenuScene, tex: usize, seq: usize) -> Nodes {
    let mut n = Nodes::default();
    let new = |scene: &mut MenuScene, s: Option<usize>, t: usize| {
        let mut node = Node::new(s);
        node.tex = t;
        node.visible = false;
        scene.add(node)
    };
    let main = |scene: &mut MenuScene| new(scene, Some(seq), tex);
    let text = |scene: &mut MenuScene| new(scene, None, tex);
    for _ in 0..4 {
        n.layers.push(main(scene));
    }
    n.munny = main(scene);
    n.time = main(scene);
    for _ in 0..6 {
        n.munny_digits.push(main(scene));
    }
    for _ in 0..5 {
        n.time_digits.push(main(scene));
    }
    n.area = text(scene);
    n.help = text(scene);
    n.row = main(scene);
    n.row_text = text(scene);
    n.row1 = main(scene);
    n.row1_text = text(scene);
    n.hand = main(scene);
    n.tab = main(scene);
    n.tab_text = text(scene);
    n.panel = main(scene);
    n.bar = main(scene);
    n.arrows = [main(scene), main(scene)];
    for _ in 0..SLOTS {
        let f = FileNodes {
            plate: main(scene),
            number: text(scene),
            logo: new(scene, Some(seq + 2), tex + 2),
            lv: text(scene),
            munny: text(scene),
            time: text(scene),
            area: text(scene),
        };
        n.files.push(f);
    }
    n.file_hand = main(scene);
    n.boxn = main(scene);
    n.box_text = text(scene);
    n.pills = [main(scene), main(scene)];
    n.pill_text = [text(scene), text(scene)];
    n.pill_hand = main(scene);
    n
}

/// Every node of the menu (to hide).
fn all_nodes(n: &Nodes) -> Vec<usize> {
    let mut v: Vec<usize> = n.layers.clone();
    v.extend(n.munny_digits.iter().chain(&n.time_digits));
    v.extend([n.munny, n.time, n.area, n.help, n.row, n.row_text, n.row1, n.row1_text, n.hand, n.tab, n.tab_text, n.boxn, n.box_text, n.pill_hand, n.panel, n.bar, n.file_hand]);
    v.extend(n.pills.iter().chain(&n.pill_text).chain(&n.arrows));
    for f in &n.files {
        v.extend([f.plate, f.number, f.logo, f.lv, f.munny, f.time, f.area]);
    }
    v
}

fn hide(scene: &mut MenuScene, ids: &[usize]) {
    for &i in ids {
        scene.nodes[i].visible = false;
        scene.nodes[i].label = None;
    }
}

fn read_slots(dir: &std::path::Path) -> [Option<SlotInfo>; SLOTS] {
    std::array::from_fn(|i| std::fs::read_to_string(progress::slot_path(dir, i)).ok().and_then(|t| Progress::slot_info(&t)))
}

/// The state the slot file records for the row (level, munny, time, area).
fn info_now(progress: &Progress, area: &str, play: f32) -> SlotInfo {
    SlotInfo { level: u32::from(progress.sora().level), munny: progress.wallet(), play_seconds: play as u64, area: area.to_string() }
}

/// Writes slot `n` into `dir`. The folder is created on the first write; nothing outside it is touched.
pub fn write_slot(dir: &std::path::Path, n: usize, progress: &Progress, info: &SlotInfo, entrance: Option<u8>) -> std::io::Result<std::path::PathBuf> {
    std::fs::create_dir_all(dir)?;
    let path = progress::slot_path(dir, n);
    std::fs::write(&path, progress.slot_json_at(info, entrance))?;
    Ok(path)
}

#[allow(clippy::too_many_arguments)]
fn input(
    mut save: ResMut<SaveMenu>,
    menu: Res<Menu>,
    mut scene: ResMut<MenuScene>,
    mut keys: ResMut<ButtonInput<KeyCode>>,
    mut mouse: ResMut<ButtonInput<MouseButton>>,
    mut se: MessageWriter<PlaySe>,
    sora: Query<&Sora>,
    area: Res<CurrentArea>,
    real: Res<Time<Real>>,
    pause: Res<Pause>,
    mission: Option<Res<crate::struggle::Mission>>,
) {
    if !save.loaded {
        return;
    }
    save.play = real.elapsed_secs();
    if save.flow.stage == Stage::Closed
        && let Some(mode) = save.request.take()
    {
        // A save point's accept: menu kind 1. The point's gate is its own (the offer); the menu opens when no other is up.
        if menu.phase == Phase::Closed && !pause.band {
            let rows = if save.point_rows.is_empty() { vec![msg::SAVE] } else { rows_for_mode(&save.point_rows, mode, crate::area_jump::worldmap_enabled()) };
            let area_name = area.name.clone();
            open(&mut save, &mut scene, &mut se, &area_name, rows, Some(mode), false);
        } else {
            info!("menu: save point menu refused (menu {:?}, band {})", menu.phase, pause.band);
        }
        keys.reset_all();
        return;
    }
    if save.flow.stage == Stage::Closed && save.load_request {
        // The game-over list's "Load Game": menu kind 2. The caller froze the field itself; only a menu already up refuses.
        save.load_request = false;
        if menu.phase == Phase::Closed && !pause.band {
            let area_name = area.name.clone();
            open(&mut save, &mut scene, &mut se, &area_name, Vec::new(), None, true);
        } else {
            info!("menu: load refused (menu {:?}, band {})", menu.phase, pause.band);
            save.outcome = Some(LoadOutcome::Left);
        }
        keys.reset_all();
        return;
    }
    if save.flow.stage == Stage::Closed {
        // F2 stands for the field Save command (the app has no save point object). The gate is the camp's: the player is in
        // control, no event or mission layout holds the field, no menu or PAUSE band is up.
        if keys.just_pressed(KeyCode::F2) {
            let free = matches!(
                sora.single().ok().map(|s| s.state),
                Some(State::Ground | State::Air | State::QuickRun | State::Glide | State::Attack { .. } | State::Guard | State::Counter)
            );
            if menu.phase == Phase::Closed && !pause.band && free && !super::start_locked(mission.as_deref()) {
                let area_name = area.name.clone();
                open(&mut save, &mut scene, &mut se, &area_name, vec![msg::SAVE], None, false);
            } else {
                info!("menu: save refused (menu {:?}, band {}, free {free})", menu.phase, pause.band);
            }
            keys.reset_all();
        }
        return;
    }
    let pressed = |k: KeyCode| keys.just_pressed(k);
    let up = pressed(KeyCode::ArrowUp);
    let down = pressed(KeyCode::ArrowDown);
    let left = pressed(KeyCode::ArrowLeft);
    let right = pressed(KeyCode::ArrowRight);
    let confirm = pressed(KeyCode::KeyJ);
    let cancel = pressed(KeyCode::Escape) || pressed(KeyCode::Backspace) || pressed(KeyCode::KeyK);
    let start = pressed(KeyCode::Enter) || pressed(KeyCode::NumpadEnter);
    keys.reset_all();
    mouse.reset_all();
    let occupied: [bool; SLOTS] = std::array::from_fn(|i| save.slots[i].is_some());
    let ready = matches!(save.flow.stage, Stage::Top | Stage::Files | Stage::Question | Stage::Failed);
    if !ready {
        return;
    }
    if left || right {
        if save.flow.side(left) {
            menu_sound::play(&mut se, menu_sound::CURSOR);
            info!("menu: save question {}", if save.flow.ask == 0 { "Yes" } else { "No" });
            show_question(&save, &mut scene);
        }
        return;
    }
    let press = if up {
        Press::Up
    } else if down {
        Press::Down
    } else if confirm {
        Press::Confirm
    } else if cancel {
        Press::Cancel
    } else if start {
        // Start closes the camp; here it leaves the menu as a cancel of the top page does.
        Press::Cancel
    } else {
        return;
    };
    let action = save.flow.press(press, &occupied);
    match action {
        Action::None => {}
        Action::Cursor if save.flow.stage == Stage::Top => {
            menu_sound::play(&mut se, menu_sound::CURSOR);
            info!("menu: save point row cursor {}", save.flow.row);
            show_top(&mut save, &mut scene);
        }
        Action::SecondRow => {
            // `FUN_00255d38` row 1 -> `FUN_00257e70` (list 0x7549, rows 0x380cb0) and page 0x10 (`FUN_00257ee8`): the question.
            // The cursor starts on No like the overwrite question's [inferred: the list widget's start row was not read].
            menu_sound::play(&mut se, crate::menu_status::CONFIRM);
            save.flow.map = true;
            save.flow.ask = 1;
            save.flow.stage = Stage::Question;
            info!("menu: save point row 1 ({:#x}): the World Map question (page 0x10, FUN_00257e70)", save.rows.get(1).copied().unwrap_or(0));
            show_question(&save, &mut scene);
        }
        Action::MapGo => {
            // `FUN_00257ee8` result 1: `FUN_002580b8` = `FUN_0014a7c8(fader, 0xf, 0)`, page 0x14.
            menu_sound::play(&mut se, crate::menu_status::CONFIRM);
            save.flow.stage = Stage::MapFade;
            save.frames = 0;
            info!("menu: save point World Map: Yes, the screen fades to black (FUN_002580b8, page 0x14 waits for the fader)");
        }
        Action::Cursor => {
            menu_sound::play(&mut se, menu_sound::CURSOR);
            info!("menu: save file cursor {}", save.flow.file);
            show_files(&save, &mut scene);
        }
        Action::OpenFiles => {
            menu_sound::play(&mut se, crate::menu_status::CONFIRM);
            start_checking(&mut save, &mut scene);
        }
        Action::BackToTop => {
            menu_sound::play(&mut se, menu_sound::CANCEL);
            show_top(&mut save, &mut scene);
        }
        Action::Ask(slot) => {
            menu_sound::play(&mut se, crate::menu_status::CONFIRM);
            save.flow.stage = Stage::Question;
            info!("menu: save slot {} is occupied: overwrite question", slot + 1);
            show_question(&save, &mut scene);
        }
        Action::Declined => {
            menu_sound::play(&mut se, menu_sound::CANCEL);
            save.frames = 0;
            let ids = [save.nodes.boxn, save.nodes.box_text, save.nodes.pills[0], save.nodes.pills[1], save.nodes.pill_text[0], save.nodes.pill_text[1], save.nodes.pill_hand];
            hide(&mut scene, &ids);
            if save.flow.map {
                // Page 0x10 result 0 or -2: `FUN_00295ac0(2)`, `FUN_00295a00(0)`: back to the top list.
                save.flow.map = false;
                save.flow.stage = Stage::Top;
                show_top(&mut save, &mut scene);
                info!("menu: save point World Map question declined: back to page 0");
            } else {
                save.flow.stage = Stage::Files;
                show_files(&save, &mut scene);
                info!("menu: save question declined");
            }
        }
        Action::Write(slot) => {
            menu_sound::play(&mut se, crate::menu_status::CONFIRM);
            save.flow.slot = slot;
            start_writing(&mut save, &mut scene, occupied[slot]);
        }
        Action::Close => {
            menu_sound::play(&mut se, menu_sound::CANCEL);
            close(&mut save, &mut scene);
        }
        Action::Read(slot) => {
            menu_sound::play(&mut se, crate::menu_status::CONFIRM);
            save.flow.slot = slot;
            start_reading(&mut save, &mut scene);
        }
        Action::Empty => {
            // `FUN_00259b78`: `FUN_0023ddb0(slot)` is 0, so no message and no page change.
            menu_sound::play(&mut se, menu_sound::REFUSED);
            info!("menu: load slot {} holds no file: nothing happens (FUN_0023ddb0)", save.flow.file + 1);
        }
        Action::Leave => {
            menu_sound::play(&mut se, menu_sound::CANCEL);
            info!("menu: load cancelled on the file list (page 0xb result -2)");
            save.pending = None;
            close(&mut save, &mut scene);
        }
        Action::FailedOk => {
            menu_sound::play(&mut se, menu_sound::CANCEL);
            save.flow.stage = Stage::Files;
            save.frames = 0;
            let ids = [save.nodes.boxn, save.nodes.box_text];
            hide(&mut scene, &ids);
            show_files(&save, &mut scene);
        }
    }
}

/// Opens the menu: the dim, the frame's enter animations, the pointer list.
fn open(save: &mut SaveMenu, scene: &mut MenuScene, se: &mut MessageWriter<PlaySe>, area: &str, rows: Vec<u32>, point: Option<i32>, load: bool) {
    save.flow = Flow { stage: Stage::Opening, rows: rows.len(), relist: point.is_some(), load, ..Flow::default() };
    save.pending = None;
    save.outcome = None;
    save.rows = rows;
    save.point = point;
    save.frames = 0;
    save.area = area.to_string();
    scene.hidden = false;
    scene.dim = dim_alpha(DIM_BRIGHTNESS);
    let ids = all_nodes(&save.nodes);
    hide(scene, &ids);
    for n in &mut scene.nodes {
        n.label_fade = 1.0;
    }
    // The panel and the plates play their enter groups again.
    scene.nodes[save.nodes.panel].player.group = None;
    for f in &save.nodes.files {
        scene.nodes[f.plate].player.group = None;
    }
    set_layout(save, scene, 0);
    let n = &save.nodes;
    for (plate, grp) in [(n.munny, g::MUNNY), (n.time, g::TIME)] {
        scene.nodes[plate].visible = true;
        scene.set_group_then(plate, grp[0], grp[1]);
    }
    // The area name, right-aligned above the orange rule (the camp's place).
    let label = scene.text.as_ref().and_then(|t| {
        let (world, num) = (area.get(..2)?, area.get(2..)?.parse::<usize>().ok()?);
        let tokens = t.place_name(world, num)?;
        Some(crate::menu_text::Label {
            tokens,
            style: Style { font: 2, size: Some(16.0), color: [0xff, 0x8f, 0x10, 0x80], align: 1, world: t.world, ..Style::default() },
            offset: [at::AREA_RIGHT, at::AREA_TOP],
        })
    });
    scene.nodes[n.area].label = label;
    // Kind 2's build (`FUN_00258bf0`) makes no area-name text bars and its layer set has no rule under them (`FUN_002557a8` makes both).
    scene.nodes[n.area].visible = !load;
    save.slots = read_slots(&save.dir);
    if !load {
        show_top(save, scene);
    }
    menu_sound::play(se, menu_sound::OPEN);
    if load {
        // Kind 2 starts on page 1, the memory card slot list (`FUN_00258bf0`); the port has no card, so it goes on to the check.
        info!("menu: load open (menu kind 2, dir {})", save.dir.display());
        return;
    }
    match point {
        Some(mode) => info!(
            "menu: save open (menu kind 1 mode {mode} from a save point, rows {:x?}, dir {})",
            save.rows,
            save.dir.display()
        ),
        None => info!("menu: save open (dir {})", save.dir.display()),
    }
}

/// The layer groups of layout `n` (0 enter, 1 hold, 2 leave) on the four layer nodes.
fn set_layout(save: &SaveMenu, scene: &mut MenuScene, n: usize) {
    // The Load screen's layers come from the `load` set, whose sequence 0 is its own (the header word); its sequences 1 and 2 are the Save set's.
    let own = save.flow.load.then_some(save.load_layd.as_ref()).flatten();
    let layers = own.unwrap_or(&save.layd).layout(n).to_vec();
    for (i, &id) in save.nodes.layers.iter().enumerate() {
        let frame = if n == 0 { None } else { Some(0) };
        match layers.get(i) {
            Some(l) => {
                let seq = if own.is_some() && l.seq == 0 { save.load_seq } else { save.seq_base + l.seq };
                let rest = super::rest_box(scene, seq, l.group, [l.x as f32, l.y as f32], frame);
                scene.nodes[id].stretch = if std::env::var_os("KH2_MENU_NO_STRETCH").is_some() { None } else { rest };
                scene.nodes[id].visible = true;
                scene.nodes[id].seq = Some(seq);
                scene.nodes[id].pos = [l.x as f32, l.y as f32];
                scene.set_group(id, l.group);
            }
            None => scene.nodes[id].visible = false,
        }
    }
}

fn text_label(scene: &MenuScene, id: u32, font: usize, size: f32, color: [u8; 4], align: u8, at: [f32; 2]) -> Option<crate::menu_text::Label> {
    let t = scene.text.as_ref()?;
    Some(label(t, tokens_of(t, id), font, size, color, align, at))
}

/// The message with the memory card slot filled in (the originals say "MEMORY CARD slot {}").
fn slot_message(scene: &MenuScene, id: u32, at: [f32; 2], size: f32) -> Option<crate::menu_text::Label> {
    let t = scene.text.as_ref()?;
    let one = t.literal("1", Style::default(), [0.0; 2]).tokens;
    Some(label(t, fill(tokens_of(t, id), &[one]), 0, size, [0xe5, 0xe5, 0xe5, 0x80], 0, at))
}

fn set_help(save: &SaveMenu, scene: &mut MenuScene, id: u32) {
    let l = text_label(scene, id, 0, 15.0, [0x8a, 0xef, 0x5c, 0x80], 0, at::HELP);
    scene.nodes[save.nodes.help].label = l;
    scene.nodes[save.nodes.help].visible = true;
}

/// The pointer list.
fn show_top(save: &mut SaveMenu, scene: &mut MenuScene) {
    save.flow.stage = if save.flow.stage == Stage::Opening { Stage::Opening } else { Stage::Top };
    save.frames = 0;
    let n = &save.nodes;
    let files = all_files(n);
    hide(scene, &files);
    hide(scene, &[n.tab, n.tab_text, n.boxn, n.box_text, n.pills[0], n.pills[1], n.pill_text[0], n.pill_text[1], n.pill_hand]);
    let rows = if save.rows.is_empty() { vec![msg::SAVE] } else { save.rows.clone() };
    let cursor = save.flow.row.min(rows.len() - 1);
    for (k, (row, text)) in [(n.row, n.row_text), (n.row1, n.row1_text)].into_iter().enumerate() {
        let Some(&id) = rows.get(k) else {
            hide(scene, &[row, text]);
            continue;
        };
        let selected = k == cursor;
        let dy = pos::ROW_PITCH * k as f32;
        scene.nodes[row].visible = true;
        scene.nodes[row].pos = [pos::ROW[0], pos::ROW[1] + dy];
        let want = if selected { g::ROW_SELECTED } else { g::ROW_IDLE };
        if scene.nodes[row].player.group != Some(want) {
            scene.set_group(row, want);
        }
        let color = if selected { at::ROW_SELECTED } else { at::ROW_IDLE };
        let l = text_label(scene, id & 0x7fff, at::ROW_FONT, at::ROW_SIZE, color, 0, [pos::ROW_LABEL[0], pos::ROW_LABEL[1] + dy]);
        scene.nodes[text].label = l;
        scene.nodes[text].visible = true;
    }
    scene.nodes[n.hand].visible = true;
    scene.nodes[n.hand].pos = [pos::HAND[0], pos::HAND[1] + pos::ROW_PITCH * cursor as f32];
    if scene.nodes[n.hand].player.group != Some(g::HAND) {
        scene.set_group(n.hand, g::HAND);
    }
    if save.point.is_some() {
        // Kind 1's top page sets no help of kind 8's (0x3858); which help it shows was not read [GAP]: none is drawn.
        let help = save.nodes.help;
        hide(scene, &[help]);
        info!("menu: save point page 0 (menu kind 1, {} row{}, cursor {cursor})", rows.len(), if rows.len() == 1 { "" } else { "s" });
    } else {
        set_help(save, scene, msg::HELP_TOP);
        info!("menu: save page 0 (pointer list, 1 row)");
    }
}

fn all_files(n: &Nodes) -> Vec<usize> {
    let mut v = vec![n.panel, n.bar, n.arrows[0], n.arrows[1], n.file_hand];
    for f in &n.files {
        v.extend([f.plate, f.number, f.logo, f.lv, f.munny, f.time, f.area]);
    }
    v
}

/// The information box with a message.
fn show_box(save: &SaveMenu, scene: &mut MenuScene, label: Option<crate::menu_text::Label>) {
    let n = &save.nodes;
    scene.nodes[n.boxn].visible = true;
    scene.nodes[n.boxn].pos = [0.0, 0.0];
    scene.set_group_then(n.boxn, g::BOX[0], g::BOX[1]);
    scene.nodes[n.box_text].label = label;
    scene.nodes[n.box_text].visible = true;
}

fn start_checking(save: &mut SaveMenu, scene: &mut MenuScene) {
    save.flow.stage = Stage::Checking;
    save.frames = 0;
    save.slots = read_slots(&save.dir);
    let n = &save.nodes;
    hide(scene, &[n.row, n.row_text, n.row1, n.row1_text, n.hand]);
    scene.nodes[n.tab].visible = true;
    scene.nodes[n.tab].pos = pos::TAB;
    scene.set_group(n.tab, g::TAB);
    let l = text_label(scene, if save.flow.load { msg::LOAD } else { msg::SAVE }, at::ROW_FONT, at::ROW_SIZE, at::ROW_SELECTED, 0, pos::TAB_LABEL);
    scene.nodes[n.tab_text].label = l;
    scene.nodes[n.tab_text].visible = true;
    let l = slot_message(scene, msg::CHECKING, pos::BOX_TEXT, 15.0);
    show_box(save, scene, l);
    let ids = [save.nodes.help];
    hide(scene, &ids);
    info!("menu: save page 2 (checking {})", save.dir.display());
}

/// The file list: four rows, the cursor's row selected with the hand.
fn show_files(save: &SaveMenu, scene: &mut MenuScene) {
    let Some(text) = scene.text.take() else { return };
    let n = &save.nodes;
    scene.nodes[n.panel].visible = true;
    scene.nodes[n.panel].pos = [0.0, 0.0];
    if scene.nodes[n.panel].player.group.is_none() {
        scene.set_group_then(n.panel, g::PANEL[0], g::PANEL[1]);
    }
    scene.nodes[n.bar].visible = true;
    scene.nodes[n.bar].pos = [0.0, 0.0];
    scene.set_group(n.bar, g::BAR);
    for (k, a) in n.arrows.iter().enumerate() {
        scene.nodes[*a].visible = true;
        scene.nodes[*a].pos = pos::ARROWS[k];
        scene.set_group(*a, [g::ARROW_UP, g::ARROW_DOWN][k]);
    }
    let orange = [0xff, 0x80, 0x20, 0x80];
    let grey = [0xe3, 0xe3, 0xe3, 0x80];
    for (k, f) in n.files.iter().enumerate() {
        let selected = k == save.flow.file;
        let y0 = pos::FILE_PITCH * k as f32;
        let fn_ = *f;
        scene.nodes[fn_.plate].visible = true;
        scene.nodes[fn_.plate].pos = [0.0, pos::FILE_PITCH * k as f32];
        let want = if selected { g::FILE_SELECTED } else { g::FILE_IDLE[1] };
        if scene.nodes[fn_.plate].player.group != Some(want) {
            if selected {
                scene.set_group(fn_.plate, g::FILE_SELECTED);
            } else {
                scene.set_group_then(fn_.plate, g::FILE_IDLE[0], g::FILE_IDLE[1]);
            }
        }
        let base_y = pos::FILE_TOP + y0;
        let color = if selected { [0xe5, 0xe5, 0xe5, 0x80] } else { grey };
        let number = text.literal(&format!("{:02}", k + 1), Style { font: 1, size: Some(16.0), color: orange, align: 2, ..Style::default() }, [pos::FILE_NUMBER, base_y + 22.0]);
        scene.nodes[fn_.number].label = Some(number);
        scene.nodes[fn_.number].visible = true;
        match &save.slots[k] {
            Some(info) => {
                let line1 = base_y + pos::FILE_LINE1;
                let lv = {
                    let mut t = tokens_of(&text, msg::LV);
                    t.push(kh2_image::text::Token::Space);
                    t.extend(text.literal(&info.level.to_string(), Style::default(), [0.0; 2]).tokens);
                    label(&text, t, 1, 14.0, color, 0, [pos::FILE_LV, line1])
                };
                let munny = {
                    let mut t = text.literal(&info.munny.to_string(), Style::default(), [0.0; 2]).tokens;
                    t.push(kh2_image::text::Token::Space);
                    t.extend(tokens_of(&text, msg::MUNNY));
                    label(&text, t, 1, 14.0, color, 1, [pos::FILE_MUNNY_RIGHT, line1])
                };
                let (h, m) = crate::menu_pages::clock(info.play_seconds);
                let time = text.literal(&format!("{h}:{m:02}"), Style { font: 1, size: Some(14.0), color, align: 1, ..Style::default() }, [pos::FILE_TIME_RIGHT, line1]);
                let area = info.area.get(..2).zip(info.area.get(2..).and_then(|s| s.parse::<usize>().ok())).and_then(|(w, a)| text.place_name(w, a)).map(|t| {
                    label(&text, t, 1, 14.0, color, 0, [pos::FILE_LV, base_y + pos::FILE_LINE2])
                });
                for (id, l) in [(fn_.lv, Some(lv)), (fn_.munny, Some(munny)), (fn_.time, Some(time)), (fn_.area, area)] {
                    scene.nodes[id].label = l;
                    scene.nodes[id].visible = true;
                }
                let world = info.area.get(..2).unwrap_or("tt");
                scene.nodes[fn_.logo].visible = true;
                scene.nodes[fn_.logo].pos = [pos::FILE_LOGO[0], base_y + pos::FILE_LOGO[1]];
                scene.nodes[fn_.logo].ext.scale = [0.7, 0.7, 1.0];
                if scene.nodes[fn_.logo].player.group != Some(logo_group(world)) {
                    scene.set_group(fn_.logo, logo_group(world));
                }
            }
            None => {
                let empty = if save.flow.load { msg::NO_DATA } else { msg::NEW_FILE };
                let new = label(&text, tokens_of(&text, empty), 1, 14.0, color, 2, [256.0, base_y + 22.0]);
                scene.nodes[fn_.area].label = Some(new);
                scene.nodes[fn_.area].visible = true;
                for id in [fn_.lv, fn_.munny, fn_.time, fn_.logo] {
                    scene.nodes[id].label = None;
                    scene.nodes[id].visible = false;
                }
            }
        }
    }
    // The hand beside the cursor's row.
    let h = n.file_hand;
    scene.nodes[h].visible = true;
    scene.nodes[h].pos = [26.0, pos::FILE_TOP + pos::FILE_PITCH * save.flow.file as f32 + 22.0];
    if scene.nodes[h].player.group != Some(g::HAND) {
        scene.set_group(h, g::HAND);
    }
    scene.text = Some(text);
    set_help(save, scene, if save.flow.load { msg::HELP_LOAD } else { msg::HELP_FILES });
}

/// Load mode: the loading box (`0x8a91`) while the file is read.
fn start_reading(save: &mut SaveMenu, scene: &mut MenuScene) {
    save.flow.stage = Stage::Reading;
    save.frames = 0;
    let l = slot_message(scene, msg::LOADING, pos::BOX_TEXT, 15.0);
    show_box(save, scene, l);
    info!("menu: load slot {} reading (page 0xf, FUN_00259e88 states 0x32..0x34)", save.flow.slot + 1);
}

/// Load mode: the failure box (`0x8a8d`).
fn start_failed(save: &mut SaveMenu, scene: &mut MenuScene) {
    save.flow.stage = Stage::Failed;
    save.frames = 0;
    let l = slot_message(scene, msg::LOAD_FAILED, pos::BOX_TEXT, 15.0);
    show_box(save, scene, l);
}

/// The overwrite question over the list.
fn show_question(save: &SaveMenu, scene: &mut MenuScene) {
    let n = &save.nodes;
    let l = text_label(scene, if save.flow.map { msg::MAP_ASK } else { msg::OVERWRITE_ASK }, 0, 15.0, [0xe5, 0xe5, 0xe5, 0x80], 0, pos::BOX_TEXT);
    show_box(save, scene, l);
    for k in 0..2 {
        let selected = k == save.flow.ask;
        scene.nodes[n.pills[k]].visible = true;
        scene.nodes[n.pills[k]].pos = pos::PILLS[k];
        scene.set_group(n.pills[k], if selected { g::PILL_SELECTED } else { g::PILL_IDLE });
        let id = [msg::YES, msg::NO][k];
        let at = [pos::PILLS[k][0] + pos::PILL_TEXT_DX, pos::PILLS[k][1] - 8.0];
        scene.nodes[n.pill_text[k]].label = text_label(scene, id, 1, 15.0, if selected { [0xe5, 0xe5, 0xe5, 0x80] } else { at::ROW_IDLE }, 2, at);
        scene.nodes[n.pill_text[k]].visible = true;
    }
    let h = n.pill_hand;
    scene.nodes[h].visible = true;
    scene.nodes[h].pos = [pos::PILLS[save.flow.ask][0] + 20.0, pos::PILLS[save.flow.ask][1] - 4.0];
    if scene.nodes[h].player.group != Some(g::HAND) {
        scene.set_group(h, g::HAND);
    }
}

fn start_writing(save: &mut SaveMenu, scene: &mut MenuScene, overwriting: bool) {
    save.flow.stage = Stage::Writing;
    save.frames = 0;
    let n = &save.nodes;
    hide(scene, &[n.pills[0], n.pills[1], n.pill_text[0], n.pill_text[1], n.pill_hand]);
    let l = slot_message(scene, if overwriting { msg::OVERWRITING } else { msg::SAVING }, pos::BOX_TEXT, 15.0);
    show_box(save, scene, l);
    info!("menu: save slot {} writing ({})", save.flow.slot + 1, if overwriting { "overwrite" } else { "new file" });
}

fn close(save: &mut SaveMenu, scene: &mut MenuScene) {
    save.flow.stage = Stage::Closing;
    save.frames = 0;
    set_layout(save, scene, 2);
    let n = &save.nodes;
    scene.set_group(n.munny, g::MUNNY[2]);
    scene.set_group(n.time, g::TIME[2]);
    let mut ids = all_files(n);
    ids.extend([n.row, n.row_text, n.row1, n.row1_text, n.hand, n.tab, n.tab_text, n.boxn, n.box_text, n.pill_hand, n.pills[0], n.pills[1], n.pill_text[0], n.pill_text[1]]);
    hide(scene, &ids);
    info!("menu: save close");
}

#[allow(clippy::too_many_arguments)]
fn tick(
    mut save: ResMut<SaveMenu>,
    mut scene: ResMut<MenuScene>,
    real: Res<Time<Real>>,
    (mut progress, mut tags): (Option<ResMut<Progress>>, ResMut<super::new_tags::NewTags>),
    mut se: MessageWriter<PlaySe>,
    rec: Option<Res<crate::area::AreaRecord>>,
    (task, jumps, consts): (Option<ResMut<JumpTask>>, Option<ResMut<JumpMenus>>, Option<Res<ExitConsts>>),
) {
    if !save.loaded || save.flow.stage == Stage::Closed {
        return;
    }
    let ticks = scene.advance(real.delta_secs());
    save.frames += ticks;
    match save.flow.stage {
        Stage::Opening => {
            let layers = save.nodes.layers.clone();
            if layers.iter().filter(|&&l| scene.nodes[l].visible).all(|&l| scene.nodes[l].player.finished()) {
                info!("menu: save open animation done after {} ticks", save.frames);
                save.flow.stage = Stage::Top;
                set_layout(&save, &mut scene, 1);
                if save.flow.load {
                    // Kind 2: page 1 (the card slots) is skipped, page 2 checks the card.
                    start_checking(&mut save, &mut scene);
                }
            }
        }
        Stage::Reading if save.frames >= WRITE_TICKS => {
            let slot = save.flow.slot;
            let result = match progress.as_deref() {
                Some(live) => load_slot(&save.dir, slot, live),
                None => Err("no game state to load into".to_string()),
            };
            match result {
                Ok((next, loaded)) => {
                    if let Some(p) = progress.as_deref_mut() {
                        *p = next;
                        tags.set_seen_words(p.new_seen());
                    }
                    info!(
                        "menu: load slot {} read: world {} area {} entrance {}, HP/MP {:?} (FUN_00259e88 state 0x34: sound kind 0xd, FUN_001cb118, page 0x14)",
                        slot + 1,
                        loaded.dest.world,
                        loaded.dest.area,
                        loaded.dest.entrance,
                        loaded.hp_mp
                    );
                    menu_sound::play(&mut se, WRITTEN);
                    save.pending = Some(loaded);
                    close(&mut save, &mut scene);
                }
                Err(why) => {
                    warn!("menu: load slot {} failed: {why} (FUN_00259e88 shows 0x8a8d on page 6)", slot + 1);
                    start_failed(&mut save, &mut scene);
                }
            }
        }
        Stage::Checking if save.frames >= CHECK_TICKS => {
            save.flow.stage = Stage::Files;
            let ids = [save.nodes.boxn, save.nodes.box_text];
            hide(&mut scene, &ids);
            info!("menu: save page 0xb (file list, slots {:?})", save.slots.iter().map(Option::is_some).collect::<Vec<_>>());
            show_files(&save, &mut scene);
        }
        Stage::Writing if save.frames >= WRITE_TICKS => {
            let slot = save.flow.slot;
            if let Some(p) = progress.as_deref() {
                let info = info_now(p, &save.area, save.play);
                let mut stored = p.clone();
                stored.set_new_seen(&tags.seen_words());
                match write_slot(&save.dir, slot, &stored, &info, rec.as_deref().map(|r| r.entrance)) {
                    Ok(path) => {
                        info!("menu: save slot {} written ({})", slot + 1, path.display());
                        menu_sound::play(&mut se, WRITTEN);
                    }
                    Err(e) => warn!("menu: save slot {} failed: {e}", slot + 1),
                }
            } else {
                warn!("menu: save slot {} not written: no game state", slot + 1);
            }
            save.slots = read_slots(&save.dir);
            if save.flow.relist {
                // Menu kind 1: `FUN_00257880` state 0x2b plays sound kind 0xc (id 0x24, the same entry as 0xd) and sets
                // page 0xb: the file list again.
                save.flow.stage = Stage::Files;
                let ids = [save.nodes.boxn, save.nodes.box_text];
                hide(&mut scene, &ids);
                info!("menu: save point page 0xb again after the write (FUN_00257880), slots {:?}", save.slots.iter().map(Option::is_some).collect::<Vec<_>>());
                show_files(&save, &mut scene);
            } else {
                close(&mut save, &mut scene);
            }
        }
        Stage::MapFade => {
            let mut jumps = jumps;
            if let Some(j) = jumps.as_mut() {
                j.map_fade = Some(save.frames);
            }
            if save.frames >= FADE_TICKS as u32 {
                // `FUN_002580e8`: the fader is black; `FUN_00103b18(1)` sets the scene mode word, the menu ends and the field
                // scene's entry (`FUN_00103b38`, mode 1) calls `FUN_00149df0(-1, 0x80000000)` (docs/systems/worldmap-gummi/worldmap.md section 1).
                let from = rec.as_deref().map_or(crate::area::TWILIGHT_TOWN, |r| r.world);
                let table = consts.as_deref().map(|c| c.map_entrances);
                let dest = Dest::new(WORLD_MAP, 0, table.map_or(0, |t| map_entrance(&t, from)));
                if let Some(mut t) = task
                    && *t == JumpTask::Idle
                {
                    // Bit 31 of the fade flags: the fader is already black, so the jump task goes straight to the load.
                    info!(
                        "menu: save point World Map: the fader is black; FUN_00103b18(1), FUN_00149df0(-1, 0x80000000): jump to world {} area {} entrance {} (the pair table 0x3790a8 for world {from})",
                        dest.world, dest.area, dest.entrance
                    );
                    *t = JumpTask::FadeOut { dest, kind: 1, t: FADE_TICKS - 1.0 };
                } else {
                    warn!("menu: save point World Map: a jump is already running or the jump task is missing; no jump");
                }
                if let Some(j) = jumps.as_mut() {
                    j.map_fade = None;
                }
                save.flow.stage = Stage::Closed;
                save.flow.map = false;
                save.point = None;
                scene.hidden = true;
                scene.dim = 0.0;
                let ids = all_nodes(&save.nodes);
                hide(&mut scene, &ids);
            }
        }
        Stage::Closing => {
            scene.dim = dim_alpha(close_brightness(save.frames));
            if save.frames >= CLOSE_FRAMES {
                save.flow.stage = Stage::Closed;
                if save.flow.load {
                    // The kind ends (`FUN_00258cf8`: `FUN_001038a0`); a read file leaves with mode 2 (`FUN_0025a158`:
                    // `FUN_00103b18(2)`), a cancel leaves with nothing changed.
                    save.outcome = Some(save.pending.take().map_or(LoadOutcome::Left, LoadOutcome::Loaded));
                    save.flow.load = false;
                    info!("menu: load closed ({:?})", save.outcome);
                }
                if let Some(mode) = save.point.take() {
                    // FUN_00255a10's end: FUN_00258838, then FUN_001038a0 returns to the field scene.
                    info!("menu: save point menu closed (menu kind 1 mode {mode}): back to the field (FUN_001038a0)");
                }
                scene.hidden = true;
                scene.dim = 0.0;
                let ids = all_nodes(&save.nodes);
                hide(&mut scene, &ids);
            }
        }
        _ => {}
    }
    if save.flow.stage != Stage::Closed {
        refresh(&save, &mut scene, progress.as_deref());
    }
}

/// The sound kind 0xd of `FUN_00259e88` (`FUN_0023e738(0xd, 1)`): kind 0xd is entry 0xd of the table at 0x37f660, id 0x24.
const WRITTEN: u32 = 0x24;

/// The Munny and Time digits of the plates (the camp's rule, this file's groups).
fn refresh(save: &SaveMenu, scene: &mut MenuScene, progress: Option<&Progress>) {
    let n = &save.nodes;
    let munny = progress.map_or(0, |p| p.wallet());
    let digits = crate::menu_pages::digits(munny, n.munny_digits.len());
    for (i, &id) in n.munny_digits.iter().enumerate() {
        let from_right = digits.len() as isize - 1 - i as isize;
        match (from_right >= 0).then(|| digits[i]) {
            Some(d) => {
                let want = g::MUNNY_DIGIT + usize::from(d);
                scene.nodes[id].pos = [at::DIGIT_RIGHT - at::DIGIT_PITCH * from_right as f32, at::MUNNY_TOP];
                scene.nodes[id].visible = true;
                if scene.nodes[id].player.group != Some(want) {
                    scene.set_group(id, want);
                }
            }
            None => scene.nodes[id].visible = false,
        }
    }
    let (h, m) = crate::menu_pages::clock(save.play as u64);
    let mut cells: Vec<(usize, f32)> = Vec::new();
    let mut x = at::DIGIT_RIGHT;
    for d in [m % 10, m / 10] {
        cells.push((g::TIME_DIGIT + d as usize, x));
        x -= at::DIGIT_PITCH;
    }
    x += at::DIGIT_PITCH - at::COLON_PITCH;
    cells.push((g::COLON, x));
    for d in crate::menu_pages::digits(h, 3).into_iter().rev() {
        x -= at::DIGIT_PITCH;
        cells.push((g::TIME_DIGIT + usize::from(d), x));
    }
    for (i, &id) in n.time_digits.iter().enumerate() {
        match cells.get(i) {
            Some(&(grp, x)) => {
                scene.nodes[id].pos = [x, at::TIME_TOP];
                scene.nodes[id].visible = true;
                if scene.nodes[id].player.group != Some(grp) {
                    scene.set_group(id, grp);
                }
            }
            None => scene.nodes[id].visible = false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn flow(stage: Stage) -> Flow {
        Flow { stage, ..Flow::default() }
    }

    #[test]
    fn the_point_menu_lists_its_second_row_only_outside_mode_2() {
        let rows = [0x8a7e, 0x8a80];
        assert_eq!(rows_for_mode(&rows, 2, true), vec![0x8a7e], "the save point (FUN_0024f0b8(1, 2))");
        assert_eq!(rows_for_mode(&rows, 0, true), vec![0x8a7e, 0x8a80], "the world point (FUN_0024f0b8(1, 0)) with KH2_WORLDMAP=1");
        assert_eq!(rows_for_mode(&rows, 0, false), vec![0x8a7e], "without KH2_WORLDMAP the World Map row is not listed");
        let none = [false; SLOTS];
        let mut f = Flow { stage: Stage::Top, rows: 2, ..Flow::default() };
        assert_eq!(f.press(Press::Down, &none), Action::Cursor);
        assert_eq!(f.row, 1);
        assert_eq!(f.press(Press::Confirm, &none), Action::SecondRow);
        // The World Map question: Yes goes, No and cancel return to the list.
        f.stage = Stage::Question;
        f.map = true;
        f.ask = 1;
        assert_eq!(f.press(Press::Confirm, &none), Action::Declined, "the cursor starts on No");
        assert!(f.side(true));
        assert_eq!(f.press(Press::Confirm, &none), Action::MapGo);
        assert_eq!(f.press(Press::Cancel, &none), Action::Declined);
        f.stage = Stage::MapFade;
        assert_eq!(f.press(Press::Confirm, &none), Action::None, "the fade takes no input");
        f.stage = Stage::Top;
        f.map = false;
        assert_eq!(f.press(Press::Down, &none), Action::Cursor);
        assert_eq!(f.row, 0, "wraps");
        assert_eq!(f.press(Press::Confirm, &none), Action::OpenFiles);
        let mut one = Flow { stage: Stage::Top, rows: 1, ..Flow::default() };
        assert_eq!(one.press(Press::Down, &none), Action::None, "one row does not move");
        assert_eq!(one.press(Press::Cancel, &none), Action::Close);
    }

    #[test]
    fn the_pointer_list_opens_the_files_and_a_cancel_closes() {
        let none = [false; SLOTS];
        assert_eq!(flow(Stage::Top).press(Press::Confirm, &none), Action::OpenFiles);
        assert_eq!(flow(Stage::Top).press(Press::Cancel, &none), Action::Close);
    }

    #[test]
    fn the_file_cursor_wraps_and_an_empty_slot_writes_at_once() {
        let none = [false; SLOTS];
        let mut f = flow(Stage::Files);
        assert_eq!(f.press(Press::Up, &none), Action::Cursor);
        assert_eq!(f.file, SLOTS - 1, "up from the first row wraps to the last");
        assert_eq!(f.press(Press::Down, &none), Action::Cursor);
        assert_eq!(f.file, 0);
        f.file = 2;
        assert_eq!(f.press(Press::Confirm, &none), Action::Write(2));
        assert_eq!(f.press(Press::Cancel, &none), Action::BackToTop);
    }

    #[test]
    fn an_occupied_slot_asks_and_only_yes_writes() {
        let mut occ = [false; SLOTS];
        occ[1] = true;
        let mut f = flow(Stage::Files);
        f.file = 1;
        assert_eq!(f.press(Press::Confirm, &occ), Action::Ask(1));
        f.stage = Stage::Question;
        assert_eq!(f.ask, 1, "the question starts on No");
        assert_eq!(f.press(Press::Confirm, &occ), Action::Declined);
        assert!(f.side(true));
        assert_eq!(f.ask, 0);
        assert!(!f.side(true), "already on Yes");
        assert_eq!(f.press(Press::Confirm, &occ), Action::Write(1));
        assert_eq!(f.press(Press::Cancel, &occ), Action::Declined);
    }

    #[test]
    fn other_stages_ignore_presses() {
        let none = [false; SLOTS];
        for s in [Stage::Checking, Stage::Writing, Stage::Opening, Stage::Closing, Stage::Closed] {
            assert_eq!(flow(s).press(Press::Confirm, &none), Action::None, "{s:?}");
        }
    }

    #[test]
    fn a_slot_is_written_into_its_own_folder_and_listed() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/test-saves-m8");
        let _ = std::fs::remove_dir_all(&dir);
        assert!(read_slots(&dir).iter().all(Option::is_none), "no folder, no files");
        let p = Progress::from_block(&[], std::sync::Arc::new(crate::progress::ItemIndex::default()));
        let info = SlotInfo { level: 5, munny: 150, play_seconds: 1560, area: "tt03".into() };
        let path = write_slot(&dir, 1, &p, &info, None).expect("the folder is writable");
        assert!(path.ends_with("slot02.json"));
        let slots = read_slots(&dir);
        assert_eq!(slots[1], Some(info));
        assert!(slots[0].is_none() && slots[2].is_none() && slots[3].is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }

    fn load_flow(stage: Stage) -> Flow {
        Flow { stage, load: true, ..Flow::default() }
    }

    #[test]
    fn the_load_screen_reads_an_occupied_row_at_once_and_ignores_an_empty_one() {
        let mut occ = [false; SLOTS];
        occ[2] = true;
        let mut f = load_flow(Stage::Files);
        f.file = 1;
        assert_eq!(f.press(Press::Confirm, &occ), Action::Empty, "FUN_0023ddb0 is 0: nothing happens");
        assert_eq!(f.press(Press::Down, &occ), Action::Cursor);
        assert_eq!(f.file, 2);
        // No question (page 0xb goes straight to the loading box, page 0xf has no Yes/No input).
        assert_eq!(f.press(Press::Confirm, &occ), Action::Read(2));
        assert_eq!(f.slot, 2);
        assert_eq!(f.press(Press::Up, &occ), Action::Cursor);
        assert_eq!(f.press(Press::Up, &occ), Action::Cursor);
        assert_eq!(f.press(Press::Up, &occ), Action::Cursor);
        assert_eq!(f.file, SLOTS - 1, "wraps");
        assert_eq!(f.press(Press::Cancel, &occ), Action::Leave);
        // The pointer list does not exist in load mode.
        assert_eq!(load_flow(Stage::Top).press(Press::Confirm, &occ), Action::None);
        // Reading ignores the pad; the failure box takes a confirm or a cancel.
        assert_eq!(load_flow(Stage::Reading).press(Press::Confirm, &occ), Action::None);
        assert_eq!(load_flow(Stage::Failed).press(Press::Confirm, &occ), Action::FailedOk);
        assert_eq!(load_flow(Stage::Failed).press(Press::Cancel, &occ), Action::FailedOk);
        assert_eq!(load_flow(Stage::Failed).press(Press::Down, &occ), Action::None);
    }

    /// Writes a slot with the Save menu's own code, loads it into a different state and compares.
    #[test]
    fn a_slot_written_by_the_save_menu_loads_back_and_a_bad_file_changes_nothing() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/test-saves-ld1");
        let _ = std::fs::remove_dir_all(&dir);
        let items = std::sync::Arc::new(crate::progress::ItemIndex::default());
        let len = Progress::from_block(&[], items.clone()).block().len();
        let bytes: Vec<u8> = (0..len).map(|i| (i * 7 + 3) as u8).collect();
        let mut saved = Progress::from_block(&bytes, items.clone());
        saved.apply_json("{\"munny\": 4321}").unwrap();
        let info = SlotInfo { level: 9, munny: 4321, play_seconds: 99, area: "tt03".into() };
        write_slot(&dir, 2, &saved, &info, Some(5)).expect("the folder is writable");

        let mut live = Progress::from_block(&[], items);
        live.apply_json("{\"munny\": 7}").unwrap();
        let before = live.block().to_vec();
        let (next, loaded) = load_slot(&dir, 2, &live).expect("the slot loads");
        assert_eq!(next.block(), saved.block(), "the block comes back byte for byte");
        assert_eq!(next.wallet(), 4321);
        assert_eq!(loaded.dest, Dest::new(crate::area::TWILIGHT_TOWN, 3, 5), "tt03, the entrance the slot recorded");
        let m = saved.sora();
        let want = (m.hp_max > 0).then(|| (i32::from(m.hp.min(m.hp_max)), i32::from(m.mp.min(m.mp_max))));
        assert_eq!(loaded.hp_mp, want, "HP and MP follow the record's +4/+5 and +6/+7");
        assert_eq!((live.wallet(), live.block()), (7, &before[..]), "the live state is untouched until the caller swaps");

        // An empty slot, a file that is not a slot file, a bad block and an area outside Twilight Town all fail and leave
        // the live state alone.
        assert!(load_slot(&dir, 0, &live).is_err(), "an empty slot");
        std::fs::write(progress::slot_path(&dir, 1), "not json").unwrap();
        assert!(load_slot(&dir, 1, &live).is_err(), "text that is not a slot file");
        let text = std::fs::read_to_string(progress::slot_path(&dir, 2)).unwrap();
        std::fs::write(progress::slot_path(&dir, 3), text.replacen("\"block\": \"", "\"block\": \"00", 1)).unwrap();
        assert!(load_slot(&dir, 3, &live).is_err(), "a block of the wrong length");
        assert!(dest_of("he03", 0).is_err() && dest_of("t", 0).is_err() && dest_of("tt", 0).is_err(), "unknown worlds and codes are refused");
        assert_eq!((live.wallet(), live.block()), (7, &before[..]));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// An old-format slot (a `block`, no `story_raw`) resets the shared story before its other keys are parsed; a bad block
    /// must restore it. A slot written without an entrance loads with entrance 0.
    #[test]
    fn a_failed_old_format_load_keeps_the_story_and_an_old_slot_enters_at_0() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/test-saves-ld1b");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let items = std::sync::Arc::new(crate::progress::ItemIndex::default());
        let mut live = Progress::from_block(&[], items.clone());
        live.apply_json("{\"story_flags\": [5, 70, 1030]}").unwrap();
        let flags = live.story().set_flags();
        assert!(!flags.is_empty(), "the story has flags to lose");
        let bad = "{\"slot\": {\"level\": 1, \"munny\": 0, \"play_seconds\": 0, \"area\": \"tt03\"}, \"block\": \"zz\"}";
        std::fs::write(progress::slot_path(&dir, 0), bad).unwrap();
        assert!(load_slot(&dir, 0, &live).is_err());
        assert_eq!(live.story().set_flags(), flags, "the shared story is as it was after the failed load");

        let old = Progress::from_block(&[], items);
        write_slot(&dir, 1, &old, &SlotInfo { level: 1, munny: 0, play_seconds: 0, area: "tt03".into() }, None).unwrap();
        let (_, loaded) = load_slot(&dir, 1, &live).expect("a slot without an entrance loads");
        assert_eq!(loaded.dest.entrance, 0);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn world_codes_pick_a_logo() {
        assert_eq!(logo_group("tt"), 17);
        assert_eq!(logo_group("he"), 10);
    }

    /// Every group the menu draws exists in the disc's `save.2ld` (skipped without `KH2_ISO`).
    #[test]
    fn the_save_groups_exist() {
        let Some(iso) = crate::assets::iso_path().ok().filter(|p| p.exists()) else {
            eprintln!("skipped: no disc (set KH2_ISO)");
            return;
        };
        let two = layd::parse_2ld(&Disc::open(&iso).read("menu/us/save.2ld").expect("save.2ld")).expect("a .2ld");
        let s0 = &two.layd.seqs[0];
        let groups = [
            g::MUNNY[0], g::MUNNY[1], g::MUNNY[2], g::TIME[0], g::TIME[1], g::TIME[2], g::MUNNY_DIGIT + 9, g::TIME_DIGIT + 9, g::COLON, g::ROW_SELECTED, g::ROW_IDLE,
            g::HAND, g::TAB, g::BOX[0], g::BOX[1], g::PILL_SELECTED, g::PILL_IDLE, g::PANEL[0], g::PANEL[1], g::FILE_SELECTED,
            g::FILE_IDLE[0], g::FILE_IDLE[1], g::BAR, g::ARROW_UP, g::ARROW_DOWN,
        ];
        for grp in groups {
            assert!(grp < s0.groups.len(), "group {grp}");
        }
        assert_eq!(two.layd.layouts, vec![(0, 3), (3, 4), (7, 3)], "enter, hold, leave");
        assert!(two.layd.seqs[2].groups.len() > logo_group("tt"));
        assert_eq!(s0.groups[g::FILE_SELECTED].unk20, pos::FILE_PITCH as i32, "the file rows' pitch");
        assert_eq!(s0.groups[g::ROW_SELECTED].unk20, 26, "the pointer rows' pitch");
    }

    /// The Load screen's layer set is the second BAR entry of type 0x1c of `save.2ld` (`FUN_0023c438(file, 1)`, named `load`):
    /// seven layers in the same three layouts but without the area rule's (groups 69 to 71), and a sequence 0 of its own whose
    /// header groups differ from the Save set's (skipped without `KH2_ISO`).
    #[test]
    fn the_load_screen_has_its_own_layer_set() {
        let Some(iso) = crate::assets::iso_path().ok().filter(|p| p.exists()) else {
            eprintln!("skipped: no disc (set KH2_ISO)");
            return;
        };
        let file = Disc::open(&iso).read("menu/us/save.2ld").expect("save.2ld");
        let save = layd::parse_2ld(&file).expect("a .2ld").layd;
        let load = load_layd(&file).expect("the second layer set");
        assert_eq!(save.layers.len(), 10);
        assert_eq!(load.layers.len(), 7);
        assert_eq!(load.layouts, vec![(0, 2), (2, 3), (5, 2)], "enter, hold, leave");
        let groups: Vec<usize> = load.layers.iter().map(|l| l.group).collect();
        assert_eq!(groups, vec![7, 20, 8, 18, 21, 9, 22]);
        assert!(!groups.iter().any(|g| (69..=71).contains(g)), "no area rule");
        assert_ne!(format!("{:?}", save.seqs[0]), format!("{:?}", load.seqs[0]), "the header sequence is its own");
        assert_eq!(format!("{:?}", save.seqs[1]), format!("{:?}", load.seqs[1]), "the other two are shared");
        assert!(load_layd(b"not a bar").is_none());
    }

    /// The game-over list's window (`FUN_0029b600(.., 0x10, 0x11)`, docs/systems/ui/menus-gameover.md 26.8 D) plays animation group 0x10 of `save.2ld`
    /// sequence 0 and would switch to group 0x11 only when group 0x10 ends (`FUN_00138aa8`: `FUN_002369e0` is true for a group whose
    /// play count, group record `+4`, is 0). Group 0x10 is one 1500-frame animation at the constant colour 128 and has the count 0,
    /// so it never ends and group 0x11 (8 frames, alpha 128 to 0) never plays (skipped without `KH2_ISO`).
    #[test]
    fn the_game_over_windows_first_animation_never_ends_so_group_0x11_never_plays() {
        let Some(iso) = crate::assets::iso_path().ok().filter(|p| p.exists()) else {
            eprintln!("skipped: no disc (set KH2_ISO)");
            return;
        };
        let file = Disc::open(&iso).read("menu/us/save.2ld").expect("save.2ld");
        let seq = &layd::parse_2ld(&file).expect("a .2ld").layd.seqs[0];
        let (g10, g11) = (seq.groups[0x10], seq.groups[0x11]);
        assert_eq!(g10.loops, 0, "play count 0: loops forever");
        assert_eq!(g10.count, 1);
        let a10 = &seq.anims[g10.first];
        assert_eq!((a10.frame_start, a10.frame_end), (0, 1500));
        assert_eq!((a10.color_start[3], a10.color_end[3]), (128, 128), "constant full alpha");
        assert_eq!(g11.loops, 1);
        let a11 = &seq.anims[g11.first];
        assert_eq!((a11.frame_start, a11.frame_end), (0, 8));
        assert_eq!((a11.color_start[3], a11.color_end[3]), (128, 0));
    }

    /// Menu kind 1 in the executable: its record (scene 3, init `FUN_00255598`, mode word 0x36cd28,
    /// the word `FUN_0024f498` returns) and the two rows of its top page (`FUN_00255c10`: message at
    /// 0x380d02 + 0x24 * row; row 1 only while the mode is not 2), printed with their texts.
    #[test]
    fn menu_kind_one_is_the_points_save_list() {
        let Some(iso) = crate::assets::iso_path().ok().filter(|p| p.exists()) else { return eprintln!("skipped: no disc (set KH2_ISO)") };
        let disc = Disc::open(&iso);
        let exe = disc.read_root("SLUS_210.05").expect("exe");
        let w = |va: u32| crate::struggle::exe_word(&exe, va).expect("in the executable");
        // The mode word (record +0xc; `FUN_0024f498` reads it).
        const MENU_KIND1_MODE: u32 = 0x0036_cd28;
        let rec = 0x0038_0560 + 0x10;
        assert_eq!(w(rec) & 0xffff, 1, "kind");
        assert_eq!(w(rec) >> 16, 3, "scene");
        assert_eq!((w(rec + 4), w(rec + 8), w(rec + 12)), (0x0025_5598, 0x0025_5608, MENU_KIND1_MODE));
        let msgs = kh2_image::text::Messages::from_bar(&disc.read("msg/us/sys.bar").expect("sys.bar")).expect("messages");
        let rows = point_rows(&exe);
        for (i, id) in rows.iter().enumerate() {
            eprintln!("kind 1 row {i}: message {id:#x} {:?}", msgs.text(id & 0x7fff));
        }
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0] & 0x7fff, 0xa7e, "row 0 is the Save row of the field menu");
        assert_eq!(rows[1] & 0x7fff, 0xa80, "row 1 is the world point's World Map row (message 0x8a80)");
        assert!(msgs.text(rows[1] & 0x7fff).is_some(), "the message exists in sys.bar");
        for id in [0x8a7b_u32, 0x8a7c, 0x8a7d] {
            eprintln!("row 1 question (0x380cb0): message {id:#x} {:?}", msgs.text(id & 0x7fff));
        }
    }
}
