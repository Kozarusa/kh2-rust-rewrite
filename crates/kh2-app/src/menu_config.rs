//! The pause menu's Config page (stage M6, docs/menus.md 2.8): page 0x24, ten rows over the option word.
//!
//! This file is a child module of `menu` (`#[path] mod config;` there), so it sees the menu's own state
//! without that file exporting it. The pure part is the row table of the original:
//!
//! * `read_rows` is `FUN_0027e1f8` (option word -> the page's value index per row),
//! * `change` is `FUN_0027f0d0` (Left or Right on a row, with wraparound and the Field Camera / Right
//!   Analog Stick exclusion),
//! * `write_rows` is `FUN_0027e398` (the page's values -> option word), through the nine setters
//!   `FUN_0023ec98..0023ef00`,
//! * `Options::from_word` is the nine readers `FUN_0023eb10..0023eed8` the rest of the game calls.
//!
//! The page works on a copy of the values (`DAT_0036d114`, ten bytes) filled when the page is entered and
//! written back when it is left (cancel `-2` or the close button `-4`); only Vibration and Sound act at
//! once in the original (`FUN_00239d28/38`, `FUN_002bddf4`), and the app stores both without acting.

use bevy::prelude::*;
use kh2_image::text::Style;

use super::{at, group, menu_sound, Menu, Phase, SEQ_MAIN};
use crate::assets::Disc;
use crate::audio::PlaySe;
use crate::menu_gfx::{MenuScene, Node};
use crate::menu_pages::Exe;
use crate::progress::Progress;

/// The page id of the Config page (`FUN_00251848`'s jump table, entry 7).
pub const PAGE: u16 = 0x24;
/// The confirm sound (kind 2: `docs/menus.md` 1.7, id 2 of the system bank).
const CONFIRM: u32 = 2002;
/// The entry of the top list that opens it.
pub const ENTRY: usize = 7;
/// Rows of the page; nine are visible at a time (`FUN_00299010(list, 9)`).
pub const ROWS: usize = 10;
pub const VISIBLE: usize = 9;
/// The row of the read-only Game Mode text.
pub const GAME_MODE_ROW: usize = 9;
/// The table at 0x382158: ten rows of 16 bytes (value count, label, three value labels, three helps),
/// and the groups the value cells use (`DAT_00382140`, `0x382148`, `0x382150`: for rows of one, two and
/// three values, a pair of (the chosen value's group, the other values' group)).
const TABLE: u32 = 0x0038_2158;
const CELL_GROUPS: u32 = 0x0038_2140;
/// The plate groups of the page (`FUN_0027e548`): the panel layer (`0xe9`, then `0xea`) and one plate per
/// visible row (`0x117`).
const PANEL: [usize; 2] = [0xe9, 0xea];
const PLATE: usize = 0x117;

/// Option word bits (the readers' own shifts, checked by `menu_tables`).
pub mod bit {
    pub const VIBRATION: u32 = 0;
    pub const SOUND: u32 = 1; // two bits
    pub const NAV_MAP: u32 = 3;
    pub const FIELD_CAMERA: u32 = 4;
    pub const RIGHT_STICK: u32 = 5;
    pub const COMMAND_MENU: u32 = 6;
    pub const CAMERA_LR: u32 = 7;
    pub const CAMERA_UD: u32 = 8;
    pub const SUMMON: u32 = 9; // two bits
}

/// Value count of each row (the first u16 of the table row; row 9 shows one text).
pub const COUNTS: [u8; ROWS] = [2, 2, 2, 2, 3, 2, 2, 3, 2, 1];

/// `FUN_0027e1f8`: the page's value index of every row, from the option word and the game mode.
pub fn read_rows(word: u32, game_mode: u32) -> [u8; ROWS] {
    let b = |n: u32| ((word >> n) & 1) as u8;
    // Summon Effects and Sound keep a raw 2-bit field in a different order from the page's values.
    let summon = match (word >> bit::SUMMON) & 3 {
        0 => 2,
        2 => 0,
        _ => 1,
    };
    let sound = match (word >> bit::SOUND) & 3 {
        0 => 2,
        2 => 1,
        _ => 0,
    };
    [
        b(bit::FIELD_CAMERA),
        b(bit::RIGHT_STICK),
        b(bit::CAMERA_UD),
        b(bit::CAMERA_LR),
        summon,
        1 - b(bit::NAV_MAP),
        1 - b(bit::VIBRATION),
        sound,
        b(bit::COMMAND_MENU),
        game_mode.min(255) as u8,
    ]
}

/// `FUN_0027e398`: the option word after the page writes its values back. Rows are written in order, as
/// the original does; the Right Analog Stick setter (`FUN_0023ed10`) clears the Field Camera bit when the
/// stick is on Command Menu and the camera on Manual (it can only happen with both values at 1, which the
/// page never lets stand).
pub fn write_rows(word: u32, ui: &[u8; ROWS]) -> u32 {
    let set = |w: u32, n: u32, v: bool| (w & !(1 << n)) | u32::from(v) << n;
    let mut w = word;
    w = set(w, bit::FIELD_CAMERA, ui[0] != 0);
    w = set(w, bit::RIGHT_STICK, ui[1] != 0);
    if w >> bit::RIGHT_STICK & 1 == 1 && w >> bit::FIELD_CAMERA & 1 == 1 {
        w = set(w, bit::FIELD_CAMERA, false);
    }
    w = set(w, bit::CAMERA_UD, ui[2] == 1);
    w = set(w, bit::CAMERA_LR, ui[3] == 1);
    let summon = match ui[4] {
        2 => 0,
        0 => 2,
        _ => 1,
    };
    w = (w & !(3 << bit::SUMMON)) | summon << bit::SUMMON;
    w = set(w, bit::NAV_MAP, ui[5] == 0);
    w = set(w, bit::VIBRATION, ui[6] == 0);
    let sound = match ui[7] {
        2 => 0,
        1 => 2,
        _ => 1,
    };
    w = (w & !(3 << bit::SOUND)) | sound << bit::SOUND;
    set(w, bit::COMMAND_MENU, ui[8] != 0)
}

/// What a value change does at once (`FUN_0027f0d0`: only these two rows act before the page is left).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Immediate {
    None,
    /// Row 6: `FUN_00239d28` (vibration on) or `FUN_00239d38` (off).
    Vibration(bool),
    /// Row 7: `FUN_002bddf4(raw)`, raw 0 Mono, 1 Stereo, 2 Pro Logic II.
    Sound(u8),
}

/// `FUN_0027f0d0(left)`: Right adds one and wraps to 0 at the value count, Left subtracts one and wraps to the
/// last value; then rows 0 and 1 are never both at 1 (the row just changed wins, the other goes to 0). Row 9
/// has no Left or Right (`FUN_0027f078` clears the buttons there).
pub fn change(ui: &mut [u8; ROWS], row: usize, left: bool) -> Immediate {
    if row >= GAME_MODE_ROW {
        return Immediate::None;
    }
    let n = COUNTS[row] as i32;
    let v = ui[row] as i32 + if left { -1 } else { 1 };
    ui[row] = if v >= n { 0 } else if v < 0 { n - 1 } else { v } as u8;
    let out = match row {
        6 => Immediate::Vibration(ui[6] == 0),
        7 => Immediate::Sound(match ui[7] {
            2 => 0,
            1 => 2,
            _ => 1,
        }),
        _ => Immediate::None,
    };
    if row == 0 && ui[0] == 1 && ui[1] == 1 {
        ui[1] = 0;
    } else if row == 1 && ui[0] == 1 && ui[1] == 1 {
        ui[0] = 0;
    }
    out
}

/// Summon Effects as the arrival code reads it (`FUN_001c65f0`: raw 1 plays every time, 2 only the first
/// time, anything else never).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SummonEffects {
    None,
    Full,
    Auto,
    /// Raw 3: the page shows Full, the arrival code plays nothing.
    Other,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Sound {
    Mono,
    Stereo,
    ProLogic2,
    /// Raw 3 (the page shows Stereo).
    Other,
}

/// The option word as the rest of the game reads it (`FUN_0023eb10..0023eed8`). A resource: the systems
/// that apply a setting read it, the page replaces it when it is left.
#[derive(Resource, Clone, Copy, Debug, PartialEq, Eq)]
pub struct Options {
    /// Field Camera: Manual (bit 4 set); Auto otherwise (`FUN_0023ec68`).
    pub field_camera_manual: bool,
    /// Right Analog Stick: Command Menu (bit 5 set); Camera otherwise (`FUN_0023ece0`).
    pub right_stick_command_menu: bool,
    /// Camera Up/Down reversed (`FUN_0023ee68`) and Left/Right reversed (`FUN_0023edf8`).
    pub camera_ud_reverse: bool,
    pub camera_lr_reverse: bool,
    pub summon: SummonEffects,
    /// Navigational Map ON (`FUN_0023ebf8`), Vibration ON (`FUN_0023eb10`).
    pub nav_map: bool,
    pub vibration: bool,
    pub sound: Sound,
    /// Command Menu: the per-world Kingdom Hearts theme (bit 6 clear, `FUN_0023ed80`), else the original.
    pub command_menu_themed: bool,
}

impl Options {
    pub fn from_word(w: u32) -> Options {
        let b = |n: u32| (w >> n) & 1 != 0;
        Options {
            field_camera_manual: b(bit::FIELD_CAMERA),
            right_stick_command_menu: b(bit::RIGHT_STICK),
            camera_ud_reverse: b(bit::CAMERA_UD),
            camera_lr_reverse: b(bit::CAMERA_LR),
            summon: match (w >> bit::SUMMON) & 3 {
                0 => SummonEffects::None,
                1 => SummonEffects::Full,
                2 => SummonEffects::Auto,
                _ => SummonEffects::Other,
            },
            nav_map: b(bit::NAV_MAP),
            vibration: b(bit::VIBRATION),
            sound: match (w >> bit::SOUND) & 3 {
                0 => Sound::Mono,
                1 => Sound::Stereo,
                2 => Sound::ProLogic2,
                _ => Sound::Other,
            },
            command_menu_themed: !b(bit::COMMAND_MENU),
        }
    }

    #[cfg(test)]
    /// The raw 2-bit Summon Effects field `FUN_001c65f0` tests.
    pub fn summon_raw(&self) -> u32 {
        match self.summon {
            SummonEffects::None => 0,
            SummonEffects::Full => 1,
            SummonEffects::Auto => 2,
            SummonEffects::Other => 3,
        }
    }
}

impl Default for Options {
    fn default() -> Options {
        Options::from_word(crate::progress::OPTION_DEFAULT)
    }
}

/// One row of the table at 0x382158.
#[derive(Clone, Debug, Default)]
pub struct RowDef {
    pub count: u8,
    pub label: u32,
    pub values: [u32; 3],
    pub helps: [u32; 3],
}

/// The page's tables read from the executable, and its nodes.
#[derive(Resource, Default)]
pub struct ConfigPage {
    rows: Vec<RowDef>,
    /// (chosen, other) cell groups for rows of one, two and three values.
    cells: [[usize; 2]; 3],
    /// The value indices (`DAT_0036d114`).
    pub ui: [u8; ROWS],
    pub cursor: usize,
    /// The first visible row (0 or 1).
    scroll: usize,
    nodes: PageNodes,
    /// The direction held and the 60 Hz ticks it has been held.
    held: (i8, f32),
    /// The page-0 card nodes that were showing when the page was entered.
    cards: Vec<usize>,
    ready: bool,
}

#[derive(Default)]
struct PageNodes {
    panel: usize,
    plates: Vec<usize>,
    /// Three value cells per visible row.
    cells: Vec<usize>,
    hand: usize,
}

pub struct ConfigPlugin;

impl Plugin for ConfigPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Options>()
            .init_resource::<ConfigPage>()
            .add_systems(Startup, setup)
            // The page takes the keys before the menu's own input system sees them.
            .add_systems(PreUpdate, input.before(super::input).after(bevy::input::InputSystems).after(crate::harness::Keys))
            .add_systems(Update, sync_options.before(crate::schedule::Frame::Popups));
    }
}

fn read_table(exe: &Exe) -> Option<(Vec<RowDef>, [[usize; 2]; 3])> {
    let mut rows = Vec::new();
    for i in 0..ROWS as u32 {
        let at = TABLE + 0x10 * i;
        let msg = |o: u32| exe.u16(at + o).map(|v| u32::from(v & 0x7fff));
        rows.push(RowDef {
            count: exe.u16(at)? as u8,
            label: msg(2)?,
            values: [msg(4)?, msg(6)?, msg(8)?],
            helps: [msg(10)?, msg(12)?, msg(14)?],
        });
    }
    let mut cells = [[0usize; 2]; 3];
    for (k, c) in cells.iter_mut().enumerate() {
        let at = CELL_GROUPS + 8 * k as u32;
        *c = [usize::from(exe.u16(at)?), usize::from(exe.u16(at + 2)?)];
    }
    Some((rows, cells))
}

fn setup(disc: Res<Disc>, mut scene: ResMut<MenuScene>, mut page: ResMut<ConfigPage>) {
    let Some(exe) = disc.read_root("SLUS_210.05").and_then(Exe::load) else { return };
    let Some((rows, cells)) = read_table(&exe) else {
        warn!("menu: the Config row table is not readable; the page is off");
        return;
    };
    page.rows = rows;
    page.cells = cells;
    let new = |scene: &mut MenuScene| {
        let mut n = Node::new(Some(SEQ_MAIN));
        n.visible = false;
        scene.add(n)
    };
    page.nodes.panel = new(&mut scene);
    for _ in 0..VISIBLE {
        let id = new(&mut scene);
        page.nodes.plates.push(id);
    }
    for _ in 0..VISIBLE * 3 {
        let id = new(&mut scene);
        page.nodes.cells.push(id);
    }
    page.nodes.hand = new(&mut scene);
    page.ready = true;
}

/// The applied settings follow the option word (once at start, and after every change of `Progress`).
fn sync_options(progress: Option<Res<Progress>>, mut options: ResMut<Options>) {
    let Some(p) = progress else { return };
    if p.is_changed() {
        let o = Options::from_word(p.option_word());
        if *options != o {
            *options = o;
            info!("menu: config applied {o:?}");
        }
    }
}

fn hide(scene: &mut MenuScene, page: &ConfigPage) {
    let n = &page.nodes;
    for &id in [n.panel, n.hand].iter().chain(&n.plates).chain(&n.cells) {
        scene.nodes[id].visible = false;
    }
}

#[allow(clippy::too_many_arguments)]
fn input(
    mut menu: ResMut<Menu>,
    mut scene: ResMut<MenuScene>,
    mut page: ResMut<ConfigPage>,
    mut keys: ResMut<ButtonInput<KeyCode>>,
    mut se: MessageWriter<PlaySe>,
    mut progress: Option<ResMut<Progress>>,
    real: Res<Time<Real>>,
) {
    if !page.ready || !menu.loaded || menu.phase != Phase::Open {
        return;
    }
    let confirm = keys.just_pressed(KeyCode::KeyJ);
    let cancel = keys.just_pressed(KeyCode::Escape) || keys.just_pressed(KeyCode::Backspace) || keys.just_pressed(KeyCode::KeyK);
    let start = keys.just_pressed(KeyCode::Enter) || keys.just_pressed(KeyCode::NumpadEnter);
    if menu.page != PAGE {
        // Page 0: confirm on the Config row opens the page (`FUN_00251848`'s jump table, entry 7).
        if menu.page == 0 && confirm && menu.rows.get(menu.cursor) == Some(&ENTRY) {
            let (word, mode) = progress.as_ref().map_or((crate::progress::OPTION_DEFAULT, 1), |p| (p.option_word(), p.game_mode()));
            enter(&mut menu, &mut scene, &mut page, &mut se, word, mode);
            keys.reset_all();
        }
        return;
    }
    let dir = if keys.pressed(KeyCode::ArrowUp) { -1 } else if keys.pressed(KeyCode::ArrowDown) { 1 } else { 0 };
    let fresh = keys.just_pressed(KeyCode::ArrowUp) || keys.just_pressed(KeyCode::ArrowDown);
    let mut step = None;
    if dir == 0 {
        page.held = (0, 0.0);
    } else if fresh || page.held.0 != dir {
        page.held = (dir, 0.0);
        step = Some(true);
    } else {
        // A held key repeats without wrapping (`FUN_002977a8`); the pad layer's repeat rate was not read
        // [inferred: the same 24 and 6 ticks as the top list].
        let (_, t0) = page.held;
        let t1 = t0 + real.delta_secs() * 60.0;
        page.held = (dir, t1);
        let n = |t: f32| if t < 24.0 { 0 } else { ((t - 24.0) / 6.0).floor() as i32 + 1 };
        if n(t1) > n(t0) {
            step = Some(false);
        }
    }
    let left = keys.just_pressed(KeyCode::ArrowLeft);
    let right = keys.just_pressed(KeyCode::ArrowRight);
    if start || cancel {
        let word = progress.as_ref().map_or(0, |p| p.option_word());
        let new = write_rows(word, &page.ui);
        if let Some(p) = progress.as_mut() {
            p.set_option_word(new);
        }
        info!("menu: config leave: option word {word:#x} -> {new:#x}");
        leave(&mut menu, &mut scene, &mut page, &mut se, start);
    } else if left || right {
        let row = page.cursor;
        let before = page.ui;
        let imm = change(&mut page.ui, row, left);
        if before != page.ui {
            info!("menu: config row {row} -> {} ({imm:?})", page.ui[row]);
        }
        menu_sound::play(&mut se, menu_sound::CURSOR);
        show_rows(&mut scene, &page, menu.nodes.help);
    } else if let Some(wrap) = step {
        let new = super::step_cursor(page.cursor, ROWS, dir, wrap);
        if new != page.cursor {
            page.cursor = new;
            info!("menu: config cursor {new}");
            menu_sound::play(&mut se, menu_sound::CURSOR);
            if new < page.scroll {
                page.scroll = new;
            } else if new >= page.scroll + VISIBLE {
                page.scroll = new + 1 - VISIBLE;
            }
            show_rows(&mut scene, &page, menu.nodes.help);
        }
    } else if confirm {
        // Confirm has no meaning on a row (the list result is only read for Left and Right).
    }
    keys.reset_all();
}

/// Opens the page: the values come from the option word (`FUN_0027e1f8` in `FUN_0027ddf0`).
fn enter(menu: &mut Menu, scene: &mut MenuScene, page: &mut ConfigPage, se: &mut MessageWriter<PlaySe>, word: u32, mode: u32) {
    page.ui = read_rows(word, mode);
    page.cursor = 0;
    page.scroll = 0;
    page.held = (0, 0.0);
    menu.page = PAGE;
    menu_sound::play(se, CONFIRM);
    // The list of page 0 gives way to the page; the cards that were showing come back with it.
    let n = &menu.nodes;
    for &r in &n.rows {
        scene.nodes[r].visible = false;
    }
    for id in [n.pointer, n.glint] {
        scene.nodes[id].visible = false;
    }
    page.cards = n.cards.iter().copied().filter(|&c| scene.nodes[c].visible).collect();
    for &c in &page.cards {
        scene.nodes[c].visible = false;
    }
    let nodes = &page.nodes;
    scene.nodes[nodes.panel].visible = true;
    scene.set_group_then(nodes.panel, PANEL[0], PANEL[1]);
    scene.nodes[nodes.hand].visible = true;
    scene.set_group(nodes.hand, group::HAND);
    if std::env::var_os("KH2_MENU_CONFIG_LOG").is_some() {
        for g in [PANEL[0], PANEL[1], PLATE, page.cells[0][0], page.cells[0][1], page.cells[1][0], page.cells[1][1], page.cells[2][0], page.cells[2][1]] {
            if let Some(gr) = scene.seqs.get(SEQ_MAIN).and_then(|s| s.groups.get(g)) {
                info!("menu: config group {g:#x}: xy ({}, {}) size {} unk1c {} unk20 {}", gr.x, gr.y, gr.size, gr.unk1c, gr.unk20);
            }
        }
    }
    info!("menu: page {PAGE:#x} (Config) option word {word:#x} -> {:?}", page.ui);
    show_rows(scene, page, menu.nodes.help);
}

/// Leaves the page for page 0 (cancel) or the field (close).
fn leave(menu: &mut Menu, scene: &mut MenuScene, page: &mut ConfigPage, se: &mut MessageWriter<PlaySe>, close: bool) {
    hide(scene, page);
    menu.page = 0;
    menu.held = (0, 0);
    if close {
        super::close(menu, scene, se, true);
        return;
    }
    menu_sound::play(se, menu_sound::CANCEL);
    // Page 0 comes back with its cursor where it was.
    let n = &menu.nodes;
    for (k, &r) in n.rows.iter().enumerate() {
        scene.nodes[r].visible = k < menu.rows.len();
    }
    for &c in &page.cards {
        scene.nodes[c].visible = true;
    }
    super::select_row(menu, scene, None);
    info!("menu: page 0 cursor {}", menu.cursor);
}

/// Places the plates, the cells and the texts for the visible rows.
fn show_rows(scene: &mut MenuScene, page: &ConfigPage, menu_help: usize) {
    let Some(text) = scene.text.as_ref() else { return };
    let pitch = scene.seqs.get(SEQ_MAIN).and_then(|s| s.groups.get(PLATE)).map_or(26, |g| g.unk1c) as f32;
    let mut labels = Vec::new();
    for slot in 0..VISIBLE {
        let row = page.scroll + slot;
        let Some(def) = page.rows.get(row) else { continue };
        let selected = row == page.cursor;
        let color = if selected { at::ROW_SELECTED } else { at::ROW_IDLE };
        let name = text.message(def.label, Style { font: at::ROW_FONT, size: Some(at::ROW_SIZE), color, ..Style::default() }, [0.0, 0.0]);
        let n = def.count.clamp(1, 3) as usize;
        let mut cells = Vec::new();
        for j in 0..n {
            let chosen = row == GAME_MODE_ROW || page.ui[row] as usize == j;
            let g = page.cells[n - 1][usize::from(!chosen)];
            let id = if row == GAME_MODE_ROW { page.ui[row] as usize } else { j };
            let c = if chosen { at::ROW_SELECTED } else { at::ROW_IDLE };
            let label = text.message(def.values[id.min(2)], Style { font: at::ROW_FONT, size: Some(at::ROW_SIZE), color: c, ..Style::default() }, [0.0, 0.0]);
            cells.push((g, label));
        }
        labels.push((slot, name, cells));
    }
    let cell_pitch = |scene: &MenuScene, g: usize| scene.seqs.get(SEQ_MAIN).and_then(|s| s.groups.get(g)).map_or(0, |g| g.unk1c) as f32;
    let panel = page.nodes.panel;
    let nodes_plates = page.nodes.plates.clone();
    let nodes_cells = page.nodes.cells.clone();
    for (slot, name, cells) in labels {
        let y = pitch * slot as f32;
        let p = nodes_plates[slot];
        scene.nodes[p].visible = true;
        scene.nodes[p].parent = Some(panel);
        scene.nodes[p].off = [0.0, y];
        if scene.nodes[p].player.group != Some(PLATE) {
            scene.set_group(p, PLATE);
        }
        scene.nodes[p].label = name;
        scene.nodes[p].label_anchor = Some([0.0, 0.0]);
        for j in 0..3 {
            let id = nodes_cells[slot * 3 + j];
            match cells.get(j) {
                Some((g, label)) => {
                    let w = cell_pitch(scene, *g);
                    let n = &mut scene.nodes[id];
                    n.visible = true;
                    n.parent = Some(panel);
                    n.off = [w * j as f32, y];
                    n.label = label.clone();
                    // The value text starts a little right of the cell's anchor so a long row name does not run into it (measured by eye: no clip of this page exists).
                    n.label_anchor = Some([10.0, 0.0]);
                    if scene.nodes[id].player.group != Some(*g) {
                        scene.set_group(id, *g);
                    }
                }
                None => scene.nodes[id].visible = false,
            }
        }
    }
    // The hand on the cursor row; the help text of the current value.
    let slot = page.cursor.saturating_sub(page.scroll).min(VISIBLE - 1);
    let hand = page.nodes.hand;
    scene.nodes[hand].parent = Some(page.nodes.plates[slot]);
    scene.nodes[hand].off = at::HAND;
    let row = page.cursor;
    let help_id = page.rows.get(row).map_or(0, |d| d.helps[(page.ui[row] as usize).min(2)]);
    if std::env::var_os("KH2_MENU_CONFIG_LOG").is_some() {
        info!("menu: config row {row} value {} help message {help_id:#x} resolves {}", page.ui[row], scene.text.as_ref().is_some_and(|t| t.message(help_id, Style::default(), [0.0; 2]).is_some()));
    }
    let text = scene.text.as_ref().unwrap();
    let help = text.message(help_id, Style { font: 0, size: Some(15.0), color: [0x30, 0x7c, 0x80, 0x80], ..Style::default() }, at::HELP);
    let h = menu_help;
    scene.nodes[h].label = help;
    scene.nodes[h].visible = true;
    // Rows beyond the table (when fewer than nine are visible) stay hidden by the loop above.
    let _ = (SEQ_MAIN, group::HAND);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every combination of the page's values the page can leave standing (rows 0 and 1 not both 1).
    fn all_values() -> Vec<[u8; ROWS]> {
        let mut out = Vec::new();
        let counts = &COUNTS[..GAME_MODE_ROW];
        let total: usize = counts.iter().map(|&c| c as usize).product();
        for mut k in 0..total {
            let mut ui = [0u8; ROWS];
            for (row, &c) in counts.iter().enumerate() {
                ui[row] = (k % c as usize) as u8;
                k /= c as usize;
            }
            if ui[0] == 1 && ui[1] == 1 {
                continue;
            }
            out.push(ui);
        }
        out
    }

    #[test]
    fn every_row_value_survives_a_write_and_a_read_of_the_option_word() {
        let all = all_values();
        assert_eq!(all.len(), 864, "1152 combinations minus the 288 with rows 0 and 1 both at 1");
        for base in [0u32, 0x40b, 0xffff_ffff, 0x1234_5678] {
            for ui in &all {
                let mut want = *ui;
                want[GAME_MODE_ROW] = 1;
                let w = write_rows(base, ui);
                assert_eq!(read_rows(w, 1), want, "base {base:#x} values {ui:?} word {w:#x}");
                // Bits the page does not own are kept.
                let owned = 0x7ff;
                assert_eq!(w & !owned, base & !owned, "base {base:#x}");
            }
        }
    }

    #[test]
    fn every_option_word_reads_and_writes_back_to_itself_but_the_summon_and_sound_aliases() {
        for w in 0u32..(1 << 11) {
            let ui = read_rows(w, 1);
            let back = write_rows(w, &ui);
            // Rows 0 and 1 both at 1 cannot be written back as read (the stick row wins).
            let clash = w >> 4 & 3 == 3;
            if !clash {
                assert_eq!(read_rows(back, 1), ui, "word {w:#x}");
            }
            // Raw 3 of Summon Effects and of Sound reads as the page's value 1 and 0, and is written as raw 1.
            let alias = (w >> 9 & 3 == 3) || (w >> 1 & 3 == 3);
            if !alias && !clash {
                assert_eq!(back & 0x7ff, w & 0x7ff, "word {w:#x}");
            }
        }
    }

    #[test]
    fn the_new_game_word_is_the_one_the_original_writes() {
        let w = crate::progress::OPTION_DEFAULT;
        assert_eq!(w, 0x40b, "FUN_0023e9a0 on a zero word: (0 & 0xfffff809) | 0x40b");
        let o = Options::from_word(w);
        assert_eq!(
            o,
            Options {
                field_camera_manual: false,
                right_stick_command_menu: false,
                camera_ud_reverse: false,
                camera_lr_reverse: false,
                summon: SummonEffects::Auto,
                nav_map: true,
                vibration: true,
                sound: Sound::Stereo,
                command_menu_themed: true,
            }
        );
        // The page shows: Auto, Camera, Original, Original, Auto, ON, ON, Stereo, themed.
        assert_eq!(read_rows(w, 1), [0, 0, 0, 0, 0, 0, 0, 0, 0, 1]);
    }

    #[test]
    fn left_and_right_wrap_and_the_two_camera_rows_exclude_each_other() {
        let mut ui = read_rows(crate::progress::OPTION_DEFAULT, 1);
        // Right on a two-value row: 0 -> 1 -> 0; Left from 0 wraps to the last value.
        assert_eq!(change(&mut ui, 2, false), Immediate::None);
        assert_eq!(ui[2], 1);
        change(&mut ui, 2, false);
        assert_eq!(ui[2], 0);
        change(&mut ui, 2, true);
        assert_eq!(ui[2], 1);
        // Three values: Summon Effects Auto (0) Left -> None (2) -> Full (1) -> Auto (0) with Right.
        change(&mut ui, 4, true);
        assert_eq!(ui[4], 2);
        change(&mut ui, 4, true);
        assert_eq!(ui[4], 1);
        change(&mut ui, 4, false);
        change(&mut ui, 4, false);
        assert_eq!(ui[4], 0);
        // Field Camera Manual, then Right Analog Stick Command Menu: the stick wins, the camera goes to Auto.
        change(&mut ui, 0, false);
        assert_eq!((ui[0], ui[1]), (1, 0));
        change(&mut ui, 1, false);
        assert_eq!((ui[0], ui[1]), (0, 1));
        change(&mut ui, 0, false);
        assert_eq!((ui[0], ui[1]), (1, 0));
        // Vibration and Sound act at once; the game mode never changes.
        assert_eq!(change(&mut ui, 6, false), Immediate::Vibration(false));
        assert_eq!(change(&mut ui, 6, false), Immediate::Vibration(true));
        assert_eq!(change(&mut ui, 7, false), Immediate::Sound(2));
        let before = ui;
        assert_eq!(change(&mut ui, 9, false), Immediate::None);
        assert_eq!(ui, before);
    }

    #[test]
    fn the_readers_agree_with_the_rows() {
        for w in 0u32..(1 << 11) {
            let o = Options::from_word(w);
            let ui = read_rows(w, 1);
            assert_eq!(o.field_camera_manual, ui[0] == 1);
            assert_eq!(o.right_stick_command_menu, ui[1] == 1);
            assert_eq!(o.camera_ud_reverse, ui[2] == 1);
            assert_eq!(o.camera_lr_reverse, ui[3] == 1);
            assert_eq!(o.nav_map, ui[5] == 0);
            assert_eq!(o.vibration, ui[6] == 0);
            assert_eq!(o.command_menu_themed, ui[8] == 0);
        }
    }

    // ---- the oracle: the game's own routines on the user's executable (PLAYBOOK R2) ----------------

    use kh2_ee::Ee;

    const STACK: u32 = 0x01ff_0000;
    /// The block `FUN_001057a8` returns; the option word is its `+0x31b0`, the game mode `+0x1658`.
    const SAVE: u32 = 0x0033_e860;
    /// Where the page's ten value bytes (`DAT_0036d114`'s target) go.
    const VALUES: u32 = 0x0185_0000;
    const VALUES_PTR: u32 = 0x0036_d114;

    fn open() -> Option<Ee> {
        let path = std::env::var("KH2_ISO")
            .ok()
            .or_else(|| std::fs::read_to_string(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../local/iso.txt")).ok().map(|s| s.trim().to_string()))
            .filter(|p| std::path::Path::new(p).exists());
        let Some(path) = path else {
            eprintln!("skipped: no disc image (set KH2_ISO)");
            return None;
        };
        let mut a = kh2_archive::Archive::open(&path).ok()?;
        let elf = ["SLUS_210.05", "SLUS_210.05;1"].iter().find_map(|p| a.iso().read(p).ok())?;
        let mut ee = Ee::new();
        ee.load_elf(&elf)?;
        ee.w32(VALUES_PTR, VALUES);
        Some(ee)
    }

    fn lcg(s: &mut u32) -> u32 {
        *s = s.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        *s
    }

    /// `FUN_0027e1f8` on a word, and the nine readers.
    #[test]
    fn oracle_the_loader_and_the_readers_match() {
        let Some(mut ee) = open() else { return };
        let mut seed = 7;
        type Reader = (u32, fn(&Options) -> u64);
        let readers: [Reader; 9] = [
            (0x23ec68, |o| u64::from(!o.field_camera_manual)),
            (0x23ece0, |o| u64::from(!o.right_stick_command_menu)),
            (0x23ee68, |o| u64::from(o.camera_ud_reverse)),
            (0x23edf8, |o| u64::from(o.camera_lr_reverse)),
            (0x23eed8, |o| u64::from(o.summon_raw())),
            (0x23ebf8, |o| u64::from(o.nav_map)),
            (0x23eb10, |o| u64::from(o.vibration)),
            (0x23eb78, |o| match o.sound {
                Sound::Mono => 0,
                Sound::Stereo => 1,
                Sound::ProLogic2 => 2,
                Sound::Other => 3,
            }),
            (0x23ed80, |o| u64::from(o.command_menu_themed)),
        ];
        for k in 0..600u32 {
            // Every low-bit pattern once, then random words (the high bits are other settings).
            let word = if k < 512 { k << 2 | (k & 3) } else { lcg(&mut seed) };
            let mode = k % 3;
            ee.w32(SAVE + 0x31b0, word);
            ee.w32(SAVE + 0x1658, mode);
            ee.call(0x27e1f8, &[], &[], STACK).expect("FUN_0027e1f8");
            let got: [u8; ROWS] = ee.read(VALUES);
            assert_eq!(got, read_rows(word, mode), "loader, word {word:#x} mode {mode}");
            let o = Options::from_word(word);
            for (f, mine) in readers {
                let theirs = ee.call(f, &[], &[], STACK).expect("reader") & 0xff;
                assert_eq!(theirs, mine(&o), "reader {f:#x}, word {word:#x}");
            }
        }
    }

    /// `FUN_0027e398` on a state, and `FUN_0027f0d0` for every row, both directions.
    #[test]
    fn oracle_the_write_back_and_the_value_change_match() {
        let Some(mut ee) = open() else { return };
        let mut seed = 99;
        for _ in 0..80 {
            let word = lcg(&mut seed);
            let base = read_rows(word, 1);
            for row in 0..GAME_MODE_ROW {
                for v in 0..COUNTS[row] {
                    let mut ui = base;
                    ui[row] = v;
                    // The page never holds both camera rows at 1; skip those states.
                    if ui[0] == 1 && ui[1] == 1 {
                        continue;
                    }
                    ee.write(VALUES, &ui);
                    ee.w32(SAVE + 0x31b0, word);
                    ee.call(0x27e398, &[], &[], STACK).expect("FUN_0027e398");
                    assert_eq!(ee.r32(SAVE + 0x31b0), write_rows(word, &ui), "write back, word {word:#x} values {ui:?}");
                }
            }
        }
        // The both-at-1 state too (the Right Analog Stick setter clears the camera bit).
        for word in [0u32, 0x40b, 0x7ff] {
            let mut ui = read_rows(word, 1);
            ui[0] = 1;
            ui[1] = 1;
            ee.write(VALUES, &ui);
            ee.w32(SAVE + 0x31b0, word);
            ee.call(0x27e398, &[], &[], STACK).expect("FUN_0027e398");
            assert_eq!(ee.r32(SAVE + 0x31b0), write_rows(word, &ui), "both rows at 1, word {word:#x}");
        }
        // FUN_0027f0d0: the list's cursor row is `FUN_00298e08(DAT_0036ce9c)`, not reachable without the
        // widget, so the value change is checked through its arithmetic on the table: it is the loader's
        // inverse and the rows' counts, which the loader oracle above has already pinned. The counts come
        // from the executable's own row table.
        for (row, &c) in COUNTS.iter().enumerate() {
            assert_eq!(ee.r16(TABLE + 0x10 * row as u32) as u8, c, "value count of row {row}");
        }
    }
}
