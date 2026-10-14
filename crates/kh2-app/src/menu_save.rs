//! The Save menu (menu kinds 7 and 8, `save.2ld`; docs/menus.md section 18).
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

use bevy::prelude::*;
use kh2_image::layd::{self, Layd};
use kh2_image::text::Style;

use super::item_pages::{fill, label, tokens_of};
use super::{at, close_brightness, dim_alpha, menu_sound, Menu, Phase, CLOSE_FRAMES, DIM_BRIGHTNESS};
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
    pub const SAVING: u32 = 0xa90;
    pub const OVERWRITING: u32 = 0xa92;
    pub const LV: u32 = 0x39fc;
    pub const MUNNY: u32 = 0x37db;
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
    /// The pointer list row, selected (red, outline and glint; the idle groups 78, 79, 80 are not used: one row).
    pub const ROW_SELECTED: usize = 77;
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
    Closing,
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
}

impl Flow {
    /// A press in the current stage. `occupied[i]`: slot `i` holds a file.
    pub fn press(&mut self, p: Press, occupied: &[bool; SLOTS]) -> Action {
        match (self.stage, p) {
            (Stage::Top, Press::Confirm) => Action::OpenFiles,
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
                if self.ask == 0 {
                    Action::Write(self.slot)
                } else {
                    Action::Declined
                }
            }
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
}

impl SaveMenu {
    /// While the menu is up it owns the pad and the field is frozen.
    pub fn is_open(&self) -> bool {
        self.flow.stage != Stage::Closed
    }
}

pub struct SavePlugin;

impl Plugin for SavePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<SaveMenu>()
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
    save.dir = progress::save_dir();
    save.nodes = build_nodes(&mut scene, save.tex_base, save.seq_base);
    save.loaded = true;
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
    v.extend([n.munny, n.time, n.area, n.help, n.row, n.row_text, n.hand, n.tab, n.tab_text, n.boxn, n.box_text, n.pill_hand, n.panel, n.bar, n.file_hand]);
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
pub fn write_slot(dir: &std::path::Path, n: usize, progress: &Progress, info: &SlotInfo) -> std::io::Result<std::path::PathBuf> {
    std::fs::create_dir_all(dir)?;
    let path = progress::slot_path(dir, n);
    std::fs::write(&path, progress.slot_json(info))?;
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
                open(&mut save, &mut scene, &mut se, &area_name);
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
    let ready = matches!(save.flow.stage, Stage::Top | Stage::Files | Stage::Question);
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
            save.flow.stage = Stage::Files;
            save.frames = 0;
            let ids = [save.nodes.boxn, save.nodes.box_text, save.nodes.pills[0], save.nodes.pills[1], save.nodes.pill_text[0], save.nodes.pill_text[1], save.nodes.pill_hand];
            hide(&mut scene, &ids);
            show_files(&save, &mut scene);
            info!("menu: save question declined");
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
    }
}

/// Opens the menu: the dim, the frame's enter animations, the pointer list.
fn open(save: &mut SaveMenu, scene: &mut MenuScene, se: &mut MessageWriter<PlaySe>, area: &str) {
    save.flow = Flow { stage: Stage::Opening, ..Flow::default() };
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
            style: Style { font: 2, size: Some(16.0), color: [0x80, 0x48, 0x08, 0x80], align: 1, world: t.world, ..Style::default() },
            offset: [at::AREA_RIGHT, at::AREA_TOP],
        })
    });
    scene.nodes[n.area].label = label;
    scene.nodes[n.area].visible = true;
    save.slots = read_slots(&save.dir);
    show_top(save, scene);
    menu_sound::play(se, menu_sound::OPEN);
    info!("menu: save open (dir {})", save.dir.display());
}

/// The layer groups of layout `n` (0 enter, 1 hold, 2 leave) on the four layer nodes.
fn set_layout(save: &SaveMenu, scene: &mut MenuScene, n: usize) {
    let layers = save.layd.layout(n).to_vec();
    for (i, &id) in save.nodes.layers.iter().enumerate() {
        let frame = if n == 0 { None } else { Some(0) };
        match layers.get(i) {
            Some(l) => {
                let rest = super::rest_box(scene, save.seq_base + l.seq, l.group, [l.x as f32, l.y as f32], frame);
                scene.nodes[id].stretch = if std::env::var_os("KH2_MENU_NO_STRETCH").is_some() { None } else { rest };
                scene.nodes[id].visible = true;
                scene.nodes[id].seq = Some(save.seq_base + l.seq);
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
    Some(label(t, fill(tokens_of(t, id), &[one]), 0, size, [0x80; 4], 0, at))
}

fn set_help(save: &SaveMenu, scene: &mut MenuScene, id: u32) {
    let l = text_label(scene, id, 0, 15.0, [0x45, 0x78, 0x2e, 0x80], 0, at::HELP);
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
    scene.nodes[n.row].visible = true;
    scene.nodes[n.row].pos = pos::ROW;
    scene.set_group(n.row, g::ROW_SELECTED);
    let l = text_label(scene, msg::SAVE, at::ROW_FONT, at::ROW_SIZE, at::ROW_SELECTED, 0, pos::ROW_LABEL);
    scene.nodes[n.row_text].label = l;
    scene.nodes[n.row_text].visible = true;
    scene.nodes[n.hand].visible = true;
    scene.nodes[n.hand].pos = pos::HAND;
    scene.set_group(n.hand, g::HAND);
    set_help(save, scene, msg::HELP_TOP);
    info!("menu: save page 0 (pointer list, 1 row)");
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
    hide(scene, &[n.row, n.row_text, n.hand]);
    scene.nodes[n.tab].visible = true;
    scene.nodes[n.tab].pos = pos::TAB;
    scene.set_group(n.tab, g::TAB);
    let l = text_label(scene, msg::SAVE, at::ROW_FONT, at::ROW_SIZE, at::ROW_SELECTED, 0, pos::TAB_LABEL);
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
    let orange = [0x80, 0x40, 0x10, 0x80];
    let grey = [0x72, 0x72, 0x72, 0x80];
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
        let color = if selected { [0x80; 4] } else { grey };
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
                let new = label(&text, tokens_of(&text, msg::NEW_FILE), 1, 14.0, color, 2, [256.0, base_y + 22.0]);
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
    set_help(save, scene, msg::HELP_FILES);
}

/// The overwrite question over the list.
fn show_question(save: &SaveMenu, scene: &mut MenuScene) {
    let n = &save.nodes;
    let l = text_label(scene, msg::OVERWRITE_ASK, 0, 15.0, [0x80; 4], 0, pos::BOX_TEXT);
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
    ids.extend([n.row, n.row_text, n.hand, n.tab, n.tab_text, n.boxn, n.box_text, n.pill_hand, n.pills[0], n.pills[1], n.pill_text[0], n.pill_text[1]]);
    hide(scene, &ids);
    info!("menu: save close");
}

#[allow(clippy::too_many_arguments)]
fn tick(
    mut save: ResMut<SaveMenu>,
    mut scene: ResMut<MenuScene>,
    real: Res<Time<Real>>,
    (progress, tags): (Option<Res<Progress>>, Res<super::new_tags::NewTags>),
    mut se: MessageWriter<PlaySe>,
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
                match write_slot(&save.dir, slot, &stored, &info) {
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
            close(&mut save, &mut scene);
        }
        Stage::Closing => {
            scene.dim = dim_alpha(close_brightness(save.frames));
            if save.frames >= CLOSE_FRAMES {
                save.flow.stage = Stage::Closed;
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
        Flow { stage, file: 0, ask: 0, slot: 0 }
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
        let path = write_slot(&dir, 1, &p, &info).expect("the folder is writable");
        assert!(path.ends_with("slot02.json"));
        let slots = read_slots(&dir);
        assert_eq!(slots[1], Some(info));
        assert!(slots[0].is_none() && slots[2].is_none() && slots[3].is_none());
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
            g::MUNNY[0], g::MUNNY[1], g::MUNNY[2], g::TIME[0], g::TIME[1], g::TIME[2], g::MUNNY_DIGIT + 9, g::TIME_DIGIT + 9, g::COLON, g::ROW_SELECTED,
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
}
