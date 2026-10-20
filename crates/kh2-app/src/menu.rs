//! The pause (camp) menu, stage M1: open, close, the top page and the transition (docs/menus.md).
//!
//! What the original does (`SLUS_210.05`): Start opens menu kind 0 as a game scene (`FUN_00138758`
//! gate, `FUN_0024f0b8`, docs/menus.md 1.1), the field is stopped, the camp task (`FUN_00250590`)
//! plays the open sound (`FUN_0023e738(0xb, 1)`) and runs the page handler of the current page
//! every frame. Page 0 is the list of 2.1; its rows come from the executable's entry table and
//! its flags (`menu_pages`). A cancel on page 0 or Start closes the menu.
//!
//! The app has no scene change: while the menu is open the 60 Hz simulation sets do not run (a run
//! condition on every `Sim` set), no key or mouse press reaches the game (the input system takes them
//! before the game's own readers and clears them), and the menu draws on a camera of its own
//! (`menu_gfx`). The music keeps playing, as the original's does.
//!
//! From the footage (docs/menus.md 1.6, clip 10): the open dims the whole picture, field and HUD, to
//! about 30 % brightness in one frame; the close brings it back linearly over 14 frames.

use bevy::prelude::*;
use kh2_image::layd::Layd;
use kh2_image::text::Style;

use crate::assets::{CurrentArea, Disc};
use crate::audio::PlaySe;
use crate::menu_gfx::{self, MenuGfxPlugin, MenuScene, Node};
use crate::menu_pages::{self, Entry, Exe, Flags};
use crate::progress::Progress;
use crate::schedule::Pause;
use crate::sora::{Sora, State};

// The Config page (stage M6) lives in its own file as a child module, so it reads this file's state.
#[path = "menu_config.rs"]
pub mod config;
// The Items pages (stage M3), a child module for the same reason.
#[path = "menu_items.rs"]
pub mod item_pages;
// And the Customize pages.
#[path = "menu_customize.rs"]
pub mod customize;
// And the Equipment pages (stage M4).
#[path = "menu_equipment.rs"]
pub mod equipment;
// The NEW tags (stage M7).
#[path = "menu_new.rs"]
pub mod new_tags;
// The Help overlay (page 0x25, stage M7).
#[path = "menu_help.rs"]
pub mod help;
// The PAUSE band of the combat pause (docs/menus.md 1.8).
#[path = "menu_pause.rs"]
pub mod pause_band;
// The item pictures (stage M7).
#[path = "menu_pic.rs"]
pub mod pictures;
// The 3D party preview of the top page (WP-MN1).
#[path = "menu_preview.rs"]
pub mod preview;
// The Save menu (kinds 7 and 8, docs/menus.md 18).
#[path = "menu_save.rs"]
pub mod save;
// The Party page (pages 0x14 and 0x16, docs/menus.md 19, WP-MN7).
#[path = "menu_party.rs"]
pub mod party_page;

pub struct MenuPlugin;

impl Plugin for MenuPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((MenuGfxPlugin, crate::menu_status::StatusPlugin, config::ConfigPlugin, item_pages::ItemPagesPlugin, customize::CustomizePlugin, equipment::EquipmentPlugin, new_tags::NewTagsPlugin, pictures::PicturesPlugin, help::HelpPlugin, pause_band::PausePlugin, preview::PreviewPlugin, save::SavePlugin, party_page::PartyPlugin))
            .init_resource::<Menu>()
            .add_systems(Startup, load)
            // The game's own readers run in `Update`; the menu takes its keys first, in `PreUpdate`
            // after the keyboard is read and after the harness has pressed its scripted keys.
            .add_systems(PreUpdate, input.after(bevy::input::InputSystems).after(crate::harness::Keys))
            .add_systems(Update, (tick.before(crate::schedule::Frame::Popups), pause_log.after(crate::schedule::Frame::Camera)));
    }
}

/// `KH2_PAUSELOG=1`: one line every 30 frames with the frozen state and the positions that must stand still while
/// the simulation is paused: Sora, the first enemy (by name) and the camera (the proof of docs/menus.md 1.8).
fn pause_log(
    pause: Res<Pause>,
    menu: Res<Menu>,
    sora: Query<&Sora>,
    enemies: Query<(&crate::enemy::Enemy, &Transform)>,
    cam: Query<&Transform, With<crate::camera::OrbitCamera>>,
    mut frame: Local<u32>,
) {
    if std::env::var_os("KH2_PAUSELOG").is_none() {
        return;
    }
    *frame += 1;
    if !frame.is_multiple_of(30) {
        return;
    }
    let v = |p: Vec3| format!("({:.1} {:.1} {:.1})", p.x, p.y, p.z);
    let s = sora.single().map_or("-".into(), |s| v(s.motor.pos));
    let mut e: Vec<String> = enemies.iter().map(|(e, t)| format!("{} {}", e.name, v(t.translation))).collect();
    e.sort();
    let c = cam.single().map_or("-".into(), |t| v(t.translation));
    info!("pauselog f{} frozen {} phase {:?} sora {} camera {} enemies [{}]", *frame, pause.frozen(), menu.phase, s, c, e.first().cloned().unwrap_or_default());
}

/// Run condition: the menu is closed, so the field runs.
pub fn is_closed(menu: Res<Menu>) -> bool {
    menu.phase == Phase::Closed
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Phase {
    Closed,
    /// The enter animations play (the first 15-frame groups of the layout).
    Opening,
    Open,
    /// The leave animations play and the dim returns over `CLOSE_FRAMES`.
    Closing,
}

/// Frames the field takes to come back to full brightness on close (clip 10: frames 56..69).
pub const CLOSE_FRAMES: u32 = 14;
/// Brightness of the dimmed picture (clip 10 frame 22: the mean drops to about 30 %).
pub const DIM_BRIGHTNESS: f32 = 0.3;
/// The menu's sequences step one frame per 60 Hz tick (docs/menus.md 1.6.1): the frame step 0x2378a0 adds
/// `DAT_00356f44 * object +0x44`; +0x44 is 1.0 for every 2D object (`FUN_002341e8` / `FUN_00236650` write
/// 0x37f3c8 / 0x37f404 = 1.0, no menu code writes it), and `DAT_00356f44` is the vblank count of the frame
/// (`FUN_0014cdd0`), one per frame while a menu scene runs (`FUN_0024ef30` -> `FUN_00239e78` sets the
/// one-vblank mode). `KH2_MENU_SPEED=<f>` overrides it (harness aid; the old footage-measured pace was 2).
const PLAYER_SPEED: f32 = 1.0;

fn player_speed() -> f32 {
    std::env::var("KH2_MENU_SPEED").ok().and_then(|v| v.parse::<f32>().ok()).filter(|v| *v > 0.0).unwrap_or(PLAYER_SPEED)
}
/// Key repeat of the list (the repeat word of `FUN_002977a8`): frames before it starts and between
/// moves. The pad layer's repeat rate was not read [inferred].
const REPEAT_DELAY: u32 = 24;
const REPEAT_EVERY: u32 = 6;

/// Frames since the menu started, scaled to the 60 Hz tick.
type Tick = u32;

/// Layout of page 0 in menu units (before the HUD's 16 line offset), measured on clip 10 where the
/// code does not give it. The fit of footage pixels to menu units was done on the frame art, whose
/// coordinates are the layers' own: x_px = 190 + 1.133 x, y_px = 26.5 + 1.087 (y + 16) at 960x540.
mod at {
    /// Where a row's name starts from the row group's anchor point (34, 79 idle; 52, 79 selected):
    /// the clip's text starts 7 left of and 1.5 below it.
    pub const ROW_LABEL: [f32; 2] = [-7.0, 1.5];
    /// The hand's origin from the selected row's anchor (its fingertip, 1 right of the origin, is at
    /// about x 40 in the clip) and the glint's (it sits at the plate's top right corner).
    pub const HAND: [f32; 2] = [-13.0, 0.0];
    pub const GLINT: [f32; 2] = [94.0, -3.0];
    /// Unselected and selected row text colour (the clip: grey and white).
    pub const ROW_IDLE: [u8; 4] = [0x9f, 0x9f, 0xaf, 0x80];
    pub const ROW_SELECTED: [u8; 4] = [0xe5, 0xe5, 0xe5, 0x80];
    /// Row label size: `FUN_00139048` draws it with the group's size or 15 (`FUN_00138fc8`).
    pub const ROW_FONT: usize = 1;
    pub const ROW_SIZE: f32 = 15.0;
    /// Munny and Time digits: the right edge of the last digit's cell and the top of the digits.
    pub const DIGIT_RIGHT: f32 = 80.5;
    pub const MUNNY_TOP: f32 = 349.5;
    pub const TIME_TOP: f32 = 384.5;
    pub const DIGIT_PITCH: f32 = 16.0;
    pub const COLON_PITCH: f32 = 8.0;
    /// The area name: right edge and top.
    pub const AREA_RIGHT: f32 = 510.0;
    pub const AREA_TOP: f32 = 40.0;
    /// The help line.
    pub const HELP: [f32; 2] = [137.0, 337.0];
    /// Party cards: left edge of the card of slot `i` of `n` (`FUN_00250a00`: x = 0x2f, 0x91 or 0xf3
    /// minus (n * 98 - 98) / n, plus the fit constant of clip 10), top, width, pitch.
    pub const CARD_BASE: [i32; 3] = [0x2f, 0x91, 0xf3];
    pub const CARD_STEP: i32 = 98;
    pub const CARD_FIT: f32 = 232.0;
    pub const CARD_TOP: f32 = 259.0;
    /// Card width, height, and the distance between two cards' left edges (HD reference frame: 99.6, 71.3, 103.8 units at
    /// 2.6 pixels per unit) and the shift of the whole row that keeps the third card inside the canvas.
    pub const CARD_W: f32 = 100.0;
    pub const CARD_H: f32 = 71.3;
    pub const CARD_PITCH: f32 = 104.0;
    pub const CARD_SHIFT: f32 = -4.0;
    /// The card's text sizes: the HP and MP values, and the LV row with the width (percent) that keeps its advance.
    pub const CARD_VALUE_SIZE: f32 = 12.4;
    pub const CARD_LV_SIZE: f32 = 11.0;
    pub const CARD_LV_WIDTH: u8 = 113;
}

/// The groups of the objects page 0 builds with literal ids (`FUN_00250a00`, `FUN_002506b8`): the
/// panel outline (enter, hold, leave), the hand (`FUN_00253f98` gives 0x19 and 0x17 for page 0 to
/// `FUN_0029a9c0`; 0x19 is the 88-frame hand loop), the glint (`0x380830`: enter, leave, hold) and
/// the Munny and Time plates (`0x380818`: enter, leave, hold each), the digit bases and the colon (`FUN_0029d970`,
/// `FUN_0029de90`). The numbers are group indices of `camp.2ld`'s second sequence, read from the
/// code and the executable's tables; the disc test checks they exist.
mod group {
    pub const PANEL: [usize; 3] = [0x2e, 0x2f, 0x30];
    pub const HAND: usize = 0x19;
    pub const GLINT: [usize; 3] = [0x1b, 0x1d, 0x1c];
    pub const MUNNY: [usize; 3] = [0x6b, 0x6d, 0x6c];
    pub const TIME: [usize; 3] = [0x71, 0x73, 0x72];
    pub const MUNNY_DIGIT: usize = 0x33;
    pub const TIME_DIGIT: usize = 0x3d;
    pub const COLON: usize = 0x47;
}

/// `camp.2ld` sequence 0 holds the bars (unused by page 0), sequence 1 everything else.
const SEQ_MAIN: usize = 1;
/// Nodes per party card: twelve shapes (body, three rims, the four steps of the cut corner, the HP and MP gauges with
/// their tracks) and seven texts.
const CARD_SHAPES: usize = 12;
const CARD_NODES: usize = CARD_SHAPES + 7;

/// One party card's numbers.
#[derive(Clone, Debug, PartialEq)]
pub struct CardData {
    pub name: String,
    /// The member's name message (`0x37ba90[member id]`, `FUN_001b5890`); `name` when it is missing.
    pub name_msg: Option<u32>,
    pub level: i32,
    pub hp: i32,
    pub max_hp: i32,
    pub mp: i32,
    pub max_mp: i32,
}

#[derive(Default)]
struct Nodes {
    layers: Vec<usize>,
    panel: usize,
    /// The party preview picture (`menu_preview.rs`): drawn after the panel outline and before the rows and the cards,
    /// the order of the original's draw (the models stand in front of the panel's scan lines, behind the cards).
    preview: usize,
    rows: Vec<usize>,
    /// One NEW tag per row of the list, a child of its row (stage M7).
    tags: Vec<usize>,
    pointer: usize,
    glint: usize,
    munny: usize,
    time: usize,
    munny_digits: Vec<usize>,
    time_digits: Vec<usize>,
    area: usize,
    help: usize,
    cards: Vec<usize>,
}

/// A screen change that one page module asks another for: the list result -5 (docs/menus.md 1.7, 16.2) on the
/// Items pages and the Abilities pages. `second` is the second level (page 2 and page 0x10, the character's own
/// lists) as opposed to the first (page 1 and page 0xf, the character rows).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Switch {
    ToAbilities { second: bool },
    ToItems { second: bool },
}

#[derive(Resource)]
pub struct Menu {
    pub phase: Phase,
    /// The screen change a module has asked for and the receiving module has not taken yet.
    pub switch: Option<Switch>,
    /// Page 0's nodes that were hidden for the page that handed over: the receiving module keeps hiding them and
    /// puts them back when it leaves to page 0 (so the top list does not flash between two screens).
    pub carry: Vec<usize>,
    /// The Help overlay (page 0x25) that a page asked for with -7, taken by `menu_help.rs`.
    pub help: Option<help::Request>,
    /// The current page (`DAT_0036d1d4`); only page 0 exists in M1.
    pub page: u16,
    /// The cursor on page 0, an index into `rows`.
    pub cursor: usize,
    entries: Vec<Entry>,
    /// The table entries the list shows, in order.
    rows: Vec<usize>,
    pub flags: Flags,
    layd: Layd,
    nodes: Nodes,
    loaded: bool,
    /// 60 Hz ticks since the phase began.
    frames: Tick,
    /// Ticks the up or down key has been held (for the repeat), and which way.
    held: (i8, Tick),
    /// Real seconds the menu has existed: the play time the Time plate shows.
    play: f32,
    /// The members' name messages by member id (the u16 table at 0x37ba90, `FUN_001b5890`).
    member_names: Vec<u32>,
}

impl Default for Menu {
    fn default() -> Menu {
        Menu {
            phase: Phase::Closed,
            switch: None,
            carry: Vec::new(),
            help: None,
            page: 0,
            cursor: 0,
            entries: Vec::new(),
            rows: Vec::new(),
            flags: Flags::from_env(),
            layd: Layd::default(),
            nodes: Nodes::default(),
            loaded: false,
            frames: 0,
            held: (0, 0),
            play: 0.0,
            member_names: Vec::new(),
        }
    }
}

impl Menu {
    /// The nodes only page 0 draws (rows, hand, glint, panel outline, help line, cards): a sub-page hides
    /// them while it shows and puts their visibility back when it returns.
    pub fn page0_nodes(&self) -> Vec<usize> {
        let n = &self.nodes;
        n.rows.iter().copied().chain(n.tags.iter().copied()).chain([n.pointer, n.glint, n.panel, n.help]).chain(n.cards.iter().copied()).collect()
    }
}

/// The brightness-to-black alpha: the picture is multiplied by `b` in the game's gamma-space product
/// (the HUD's `gamma22` convention), which on the linear framebuffer is `b^2.2`.
pub fn dim_alpha(brightness: f32) -> f32 {
    1.0 - brightness.clamp(0.0, 1.0).powf(2.2)
}

/// The dim of the close fade at tick `t` (0..=CLOSE_FRAMES): brightness linear from the dimmed value
/// to 1 (clip 10: the field returns linearly over frames 56 to 69).
pub fn close_brightness(t: u32) -> f32 {
    let f = (t as f32 / CLOSE_FRAMES as f32).clamp(0.0, 1.0);
    DIM_BRIGHTNESS + (1.0 - DIM_BRIGHTNESS) * f
}

/// Moves a cursor in a list of `len` rows: a new press wraps, a held repeat stops at the ends
/// (`FUN_002977a8`: the repeat word wraps only with a new press).
pub fn step_cursor(cursor: usize, len: usize, dir: i8, wrap: bool) -> usize {
    if len == 0 {
        return 0;
    }
    match (dir, wrap) {
        (-1, true) => (cursor + len - 1) % len,
        (-1, false) => cursor.saturating_sub(1),
        (1, true) => (cursor + 1) % len,
        (1, false) => (cursor + 1).min(len - 1),
        _ => cursor,
    }
}

fn load(
    disc: Res<Disc>,
    area: Res<CurrentArea>,
    mut images: ResMut<Assets<Image>>,
    mut scene: ResMut<MenuScene>,
    mut menu: ResMut<Menu>,
) {
    let Some(two) = disc.read("menu/us/camp.2ld").and_then(|b| kh2_image::layd::parse_2ld(&b)) else {
        warn!("menu: menu/us/camp.2ld is not readable; the pause menu is off");
        return;
    };
    let Some(exe) = disc.read_root("SLUS_210.05").and_then(Exe::load) else {
        warn!("menu: the executable is not readable; the pause menu is off");
        return;
    };
    let Some(entries) = menu_pages::read_table(&exe) else {
        warn!("menu: the entry table is not readable; the pause menu is off");
        return;
    };
    let Some(image) = two.images.first() else { return };
    scene.texs.push(menu_gfx::Tex::new(image, &mut images));
    scene.texs.push(menu_gfx::white_tex(&mut images));
    let world = area.name.get(..2).unwrap_or("tt");
    let mut texs = std::mem::take(&mut scene.texs);
    scene.text = crate::menu_text::MenuText::load(&disc, world, &mut images, &mut texs);
    scene.texs = texs;
    scene.speed = player_speed();
    scene.hidden = true;
    let Layd { layers, layouts, seqs } = two.layd;
    scene.seqs = seqs;
    menu.layd = Layd { layers, layouts, seqs: Vec::new() };
    menu.entries = entries;
    menu.member_names = (0..16).filter_map(|i| exe.u16(MEMBER_NAMES + 2 * i).map(|v| u32::from(v & 0x7fff))).collect();
    menu.nodes = build_nodes(&mut scene);
    menu.loaded = true;
}

/// The close faded the row, area and help labels to nothing (`tick`, Closing): a reopen starts every label at full
/// strength again. (Play-test 2026-10-11: after one close the second open showed no row names, area name or help.)
fn restore_fades(scene: &mut MenuScene) {
    for n in &mut scene.nodes {
        n.label_fade = 1.0;
    }
}

/// Creates every node the page uses, hidden. Indices only grow: a parent comes before its child.
fn build_nodes(scene: &mut MenuScene) -> Nodes {
    let mut n = Nodes::default();
    let new = |scene: &mut MenuScene, seq: Option<usize>| {
        let mut node = Node::new(seq);
        node.visible = false;
        scene.add(node)
    };
    for _ in 0..4 {
        n.layers.push(new(scene, Some(SEQ_MAIN)));
    }
    n.panel = new(scene, Some(SEQ_MAIN));
    n.preview = new(scene, None);
    for _ in 0..menu_pages::ENTRIES {
        n.rows.push(new(scene, Some(SEQ_MAIN)));
    }
    for k in 0..menu_pages::ENTRIES {
        let t = new(scene, Some(SEQ_MAIN));
        scene.nodes[t].parent = Some(n.rows[k]);
        n.tags.push(t);
    }
    n.pointer = new(scene, Some(SEQ_MAIN));
    n.glint = new(scene, Some(SEQ_MAIN));
    n.munny = new(scene, Some(SEQ_MAIN));
    n.time = new(scene, Some(SEQ_MAIN));
    for _ in 0..6 {
        n.munny_digits.push(new(scene, Some(SEQ_MAIN)));
    }
    for _ in 0..5 {
        n.time_digits.push(new(scene, Some(SEQ_MAIN)));
    }
    n.area = new(scene, None);
    n.help = new(scene, None);
    for _ in 0..4 * CARD_NODES {
        n.cards.push(new(scene, None));
    }
    n
}

/// The members' name messages (`FUN_001b5890`: u16 per member id, bit 15 ignored).
const MEMBER_NAMES: u32 = 0x0037_ba90;

/// The party the cards show, in slot order (`FUN_00250a00` walks the party object's slots 0..2, `FUN_00282968`):
/// Sora, then the members in the field by their party slot (`party::PartyMember::slot`). The numbers are the live
/// ones (the status objects the original reads: HP and MP of the field objects), the level is the member's record
/// (`Progress::member(id).level`, record `+0xf`).
fn party(
    sora: Option<&Sora>,
    members: &[(u16, i32, i32, i32, i32)],
    progress: Option<&Progress>,
    names: &[u32],
) -> Vec<CardData> {
    let Some(s) = sora else { return Vec::new() };
    let mut out = vec![CardData {
        name: "Sora".into(),
        name_msg: names.get(usize::from(crate::progress::SORA)).copied(),
        level: s.stats.level,
        hp: s.hp,
        max_hp: s.stats.max_hp,
        mp: s.mp,
        max_mp: s.stats.max_mp,
    }];
    for &(id, hp, max_hp, mp, max_mp) in members.iter().take(2) {
        let level = progress.and_then(|p| p.member(id)).map_or(1, |m| i32::from(m.level));
        out.push(CardData { name: format!("member {id}"), name_msg: names.get(usize::from(id)).copied(), level, hp, max_hp, mp, max_mp });
    }
    out
}

impl Menu {
    /// Whether page 0's list shows the row of this table entry (the lists' switch mask, `FUN_00253f80`).
    pub fn rows_has(&self, entry: usize) -> bool {
        self.rows.contains(&entry)
    }

    /// The entry (index into the table of 2.1) of the highlighted row of page 0, while the menu is open.
    pub fn current_entry(&self) -> Option<usize> {
        (self.phase == Phase::Open).then(|| self.rows.get(self.cursor).copied()).flatten()
    }
}

/// The field task `FUN_00138758` does not open the camp (nor, in the app, the PAUSE band) while `DAT_00354f70` is non-zero.
/// Its writers: `FUN_00195898` sets bit 1 when an event is created with flag 0x80 (cleared by `FUN_00195968` /
/// `FUN_001959f0`), `FUN_001ad378` sets bit 2 for a running mission (cleared by `FUN_001ad790`, which also clears the
/// "fight stopped" bit of `DAT_0035d268`). The app's mapping: a playing event/layout (`event_active`, which holds the
/// intro's start event and its camera) or a stopped fight [inferred: the 0x80 flag is not parsed per event].
fn start_locked(mission: Option<&crate::struggle::Mission>) -> bool {
    mission.is_some_and(|m| m.event_active() || m.control_locked() || m.fight_stopped())
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn input(
    mut menu: ResMut<Menu>,
    mut scene: ResMut<MenuScene>,
    mut keys: ResMut<ButtonInput<KeyCode>>,
    mut mouse: ResMut<ButtonInput<MouseButton>>,
    mut se: MessageWriter<PlaySe>,
    sora: Query<&Sora>,
    progress: Option<Res<Progress>>,
    area: Res<CurrentArea>,
    real: Res<Time<Real>>,
    mut tags: ResMut<new_tags::NewTags>,
    items: Res<item_pages::ItemsMenu>,
    mut pause: ResMut<Pause>,
    band: Res<pause_band::Band>,
    (battle, mission, save): (Option<Res<crate::audio_bgm::BattleFlag>>, Option<Res<crate::struggle::Mission>>, Res<save::SaveMenu>),
    members: Query<&crate::party::PartyMember>,
) {
    if !menu.loaded {
        return;
    }
    menu.play = real.elapsed_secs();
    let before = menu.phase;
    let start = keys.just_pressed(KeyCode::Enter) || keys.just_pressed(KeyCode::NumpadEnter);
    let cancel = keys.just_pressed(KeyCode::Escape) || keys.just_pressed(KeyCode::Backspace) || keys.just_pressed(KeyCode::KeyK);
    let confirm = keys.just_pressed(KeyCode::KeyJ);
    let band_up = pause.band;
    match menu.phase {
        Phase::Closed if band_up => {
            // The PAUSE band: the pause button (or a cancel) takes it away again.
            if start || cancel {
                pause_band::close(&band, &mut scene, &mut pause);
                info!("menu: pause band closed");
            }
        }
        // The Save menu (menu_save.rs) owns the pad while it is up.
        Phase::Closed if save.is_open() => {}
        Phase::Closed => {
            if start && start_locked(mission.as_deref()) {
                info!("menu: start refused: a mission layout / event holds the field (FUN_00138758 gate, DAT_00354f70)");
            } else if start {
                // The gate of `FUN_00138758` that the app has: the player is in control of Sora (the
                // original also needs no scene change and no event running [inferred mapping]).
                let state = sora.single().ok().map(|s| s.state);
                let free = matches!(
                    state,
                    Some(State::Ground | State::Air | State::QuickRun | State::Glide | State::Attack { .. } | State::Guard | State::Counter)
                );
                // `KH2_NO_PAUSE_BAND=1` (harness aid): the menu opens even in combat.
                let in_combat = battle.as_ref().is_some_and(|b| b.active()) && std::env::var_os("KH2_NO_PAUSE_BAND").is_none();
                if free && in_combat && band.is_ready() {
                    // In combat the button shows the PAUSE band, not the menu (docs/menus.md 1.8 [inferred rule]).
                    pause_band::open(&band, &mut scene, &mut pause);
                    info!("menu: pause band (battle flag set)");
                } else if free {
                    let has_ability = progress.as_ref().is_some_and(|p| !p.sora().abilities.is_empty());
                    // The camp's start builds the NEW sets (`FUN_00293808`).
                    if let Some(p) = progress.as_deref() {
                        let abilities: Vec<u16> = p.sora().abilities.iter().map(|a| a.0).collect();
                        tags.load_once(p.new_seen());
                        tags.begin(&items.order, &|id| p.item_count(id), &abilities);
                        info!("menu: new tags: {}", tags.summary());
                    }
                    let mut field: Vec<&crate::party::PartyMember> = members.iter().collect();
                    field.sort_by_key(|m| m.slot);
                    let field: Vec<(u16, i32, i32, i32, i32)> = field.iter().map(|m| (m.neo, m.hp, m.hp_max, m.mp, m.mp_max)).collect();
                    let cards = party(sora.single().ok(), &field, progress.as_deref(), &menu.member_names);
                    open(&mut menu, &mut scene, &mut se, &cards, &area, has_ability);
                } else {
                    info!("menu: start refused ({state:?})");
                }
            }
        }
        // A sub-page (Status ...) owns its keys (its module runs before this system); only Start closes here.
        Phase::Opening | Phase::Open if menu.page != 0 => {
            if start {
                close(&mut menu, &mut scene, &mut se, true);
            }
        }
        Phase::Opening | Phase::Open => {
            let dir =if keys.pressed(KeyCode::ArrowUp) { -1 } else if keys.pressed(KeyCode::ArrowDown) { 1 } else { 0 };
            let fresh = keys.just_pressed(KeyCode::ArrowUp) || keys.just_pressed(KeyCode::ArrowDown);
            let mut step = None;
            if dir == 0 {
                menu.held = (0, 0);
            } else if fresh || menu.held.0 != dir {
                menu.held = (dir, 0);
                step = Some(true);
            }
            if start {
                close(&mut menu, &mut scene, &mut se, true);
            } else if cancel {
                // Page 0: a cancel (-2) closes (`FUN_00251848`).
                close(&mut menu, &mut scene, &mut se, false);
            } else if confirm {
                // M1 has no sub-page: the row refuses (sound kind 4).
                let target = menu.rows.get(menu.cursor).and_then(|&e| menu_pages::page_of(e));
                if let Some(p) = target.filter(|p| crate::menu_status::PAGES.contains(p) || party_page::PAGES.contains(p)) {
                    menu.page = p;
                    menu_sound::play(&mut se, crate::menu_status::CONFIRM);
                    info!("menu: page {p:#x}");
                } else {
                    info!("menu: page 0 row {} refused (no sub-page yet; the original opens page {:?})", menu.cursor, target);
                    menu_sound::play(&mut se, menu_sound::REFUSED);
                }
            } else if let Some(wrap) = step {
                move_cursor(&mut menu, &mut scene, &mut se, dir, wrap);
            }
        }
        Phase::Closing => {}
    }
    // The camp's end counts what the party wears as seen (`FUN_00293758`).
    if before != Phase::Closing
        && menu.phase == Phase::Closing
        && let Some(p) = progress.as_deref()
    {
        let s = p.sora();
        let worn: Vec<u16> = s.weapons.iter().chain(&s.armor).chain(&s.accessories).chain(&s.items).copied().collect();
        tags.end(&worn);
    }
    // Every press that arrives while the menu is up belongs to the menu.
    if menu.phase != Phase::Closed || band_up || pause.band || save.is_open() {
        keys.reset_all();
        mouse.reset_all();
    }
    // The simulation stops in the frame of the open input and runs again once the close has finished (`schedule::Pause`).
    let open = menu.phase != Phase::Closed || save.is_open();
    if pause.menu != open {
        pause.menu = open;
    }
}

/// System sounds of the menu: entries of the boot bank `se/se000.seb` (game bank 0), indices from
/// the executable's tables (docs/menus.md 1.7: kind -> id through 0x36cb48 and 0x37f654: cursor 1,
/// confirm 2, cancel 4, refused 5, close 2, open 3). `FUN_002be49c(2, id, ...)`: the 2 is the driver
/// SLOT that holds `se000.seb` (`FUN_002c0eb0` loads it, `FUN_002c0da8` names slot 2 bank 0), the
/// sound is entry `id` of it (docs/audio.md "System sounds"; WP-SE2 corrects the earlier "bank 2").
pub mod menu_sound {
    use bevy::prelude::*;

    use crate::audio::PlaySe;

    /// Game bank number of the system sounds (`kh2_audio::ssl::SYSTEM_BANK`).
    const BANK: u32 = 0;
    pub const CURSOR: u32 = BANK + 1;
    pub const CANCEL: u32 = BANK + 4;
    pub const REFUSED: u32 = BANK + 5;
    pub const CLOSE: u32 = BANK + 2;
    pub const OPEN: u32 = BANK + 3;

    /// The one switch for every menu sound effect (the pause menu and its pages: Items, Magic,
    /// Abilities, Customize, Equipment, Status, Config, Help). The user finds the app's menu sounds
    /// wrong (play-test report 2026-10-10, WP-F1 item 14), so they are muted (the system bank
    /// mapping was corrected in WP-SE2: the sounds are `se 0:<id>`, entries of `se000.seb`; the user
    /// has not asked for them back yet): with `false` no menu request reaches the mixer
    /// (`KH2_SOUNDLOG=1` logs `menu se 0:<id> muted` instead). Set it to `true` to hear them again.
    pub const MENU_SOUND_ENABLED: bool = false;

    /// Every menu sound request goes through here (`FUN_0023e738(kind, play)` -> `FUN_002be49c(2, id, ...)`).
    pub fn play(se: &mut MessageWriter<PlaySe>, n: u32) {
        let n = system_id(n);
        if MENU_SOUND_ENABLED {
            se.write(PlaySe::system(n));
        } else if std::env::var_os("KH2_SOUNDLOG").is_some() {
            info!("menu se 0:{n} muted (menu_sound::MENU_SOUND_ENABLED is false)");
        }
    }

    /// The pages' own constants still hold the old "bank 2" form `2000 + id` (menu_items.rs,
    /// menu_abilities.rs, menu_status.rs, menu_config.rs, menu_help.rs: other WPs' files); the
    /// system sound is the id alone.
    pub fn system_id(n: u32) -> u32 {
        if (2000..3000).contains(&n) { n - 2000 } else { n }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn menu_sounds_are_entries_of_the_boot_bank() {
            assert_eq!([CURSOR, CLOSE, OPEN, CANCEL, REFUSED], [1, 2, 3, 4, 5]);
            assert_eq!(system_id(2002), 2, "the pages' old 2000 + id form");
            assert_eq!(system_id(2037), 37);
            assert_eq!(system_id(4), 4);
        }
    }
}

fn move_cursor(menu: &mut Menu, scene: &mut MenuScene, se: &mut MessageWriter<PlaySe>, dir: i8, wrap: bool) {
    let old = menu.cursor;
    let new = step_cursor(old, menu.rows.len(), dir, wrap);
    if new == old {
        return;
    }
    menu.cursor = new;
    menu_sound::play(se, menu_sound::CURSOR);
    info!("menu: page {} cursor {}", menu.page, new);
    select_row(menu, scene, Some(old));
}

/// Puts the groups of the idle and the selected row set on the rows and the pointer on the cursor.
fn select_row(menu: &Menu, scene: &mut MenuScene, old: Option<usize>) {
    let rows = menu.nodes.rows.clone();
    if let Some(o) = old {
        // The row the cursor left goes back to its idle loop (`FUN_00296fb8` sets the selected row; the
        // previous row's group switch was not read [inferred]).
        let e = &menu.entries[menu.rows[o]];
        scene.set_group(rows[o], usize::from(e.idle[2]));
        scene.nodes[rows[o]].label_fade = 1.0;
    }
    let e = &menu.entries[menu.rows[menu.cursor]];
    // The selected row: its first group, then the loop (`FUN_00296fb8(list, cursor, 1, 0, 2)`).
    scene.set_group_then(rows[menu.cursor], usize::from(e.selected[0]), usize::from(e.selected[2]));
    let p = menu.nodes.pointer;
    scene.nodes[p].parent = Some(rows[menu.cursor]);
    scene.nodes[p].off = at::HAND;
    scene.set_group(p, group::HAND);
    scene.nodes[p].visible = true;
    let g = menu.nodes.glint;
    scene.nodes[g].parent = Some(rows[menu.cursor]);
    scene.nodes[g].off = at::GLINT;
    scene.set_group_then(g, group::GLINT[0], group::GLINT[2]);
    scene.nodes[g].visible = true;
    for (k, &r) in rows.iter().enumerate() {
        let selected = k == menu.cursor;
        if let Some(l) = scene.nodes[r].label.as_mut() {
            l.style.color = if selected { at::ROW_SELECTED } else { at::ROW_IDLE };
        }
    }
    update_help(menu, scene);
}

fn update_help(menu: &Menu, scene: &mut MenuScene) {
    let Some(text) = scene.text.as_ref() else { return };
    let id = menu.entries[menu.rows[menu.cursor]].help;
    let label = text.message(id, Style { font: 0, size: Some(15.0), color: [0x60, 0xf7, 0xff, 0x80], ..Style::default() }, at::HELP);
    scene.nodes[menu.nodes.help].label = label;
    scene.nodes[menu.nodes.help].visible = true;
}

fn open(
    menu: &mut Menu,
    scene: &mut MenuScene,
    se: &mut MessageWriter<PlaySe>,
    cards: &[CardData],
    area: &CurrentArea,
    has_ability: bool,
) {
    menu.rows = menu_pages::visible(&menu.entries, &menu.flags, has_ability);
    menu.page = 0;
    menu.cursor = menu.cursor.min(menu.rows.len().saturating_sub(1));
    menu.phase = Phase::Opening;
    menu.frames = 0;
    menu.held = (0, 0);
    scene.hidden = false;
    scene.dim = dim_alpha(DIM_BRIGHTNESS);
    let nodes = &menu.nodes;
    // The layout's layers play the enter groups (layout 0 of camp.2ld).
    set_layout(&menu.layd, nodes, scene, 0);
    scene.nodes[nodes.panel].visible = true;
    scene.set_group_then(nodes.panel, group::PANEL[0], group::PANEL[1]);
    for (k, &r) in nodes.rows.iter().enumerate() {
        let n = &mut scene.nodes[r];
        n.label = None;
        n.visible = k < menu.rows.len();
    }
    restore_fades(scene);
    let pitch = scene.seqs.get(SEQ_MAIN).and_then(|s| s.groups.get(0x86)).map_or(26, |g| g.unk20) as f32;
    for (k, &e) in menu.rows.iter().enumerate() {
        let entry = menu.entries[e].clone();
        let r = nodes.rows[k];
        scene.nodes[r].off = [0.0, pitch * k as f32];
        scene.set_group_then(r, usize::from(entry.idle[0]), usize::from(entry.idle[2]));
        let label = scene.text.as_ref().and_then(|t| {
            t.message(
                entry.name,
                Style { font: at::ROW_FONT, size: Some(at::ROW_SIZE), color: at::ROW_IDLE, ..Style::default() },
                [0.0, 0.0],
            )
        });
        scene.nodes[r].label = label;
        scene.nodes[r].label_anchor = Some(at::ROW_LABEL);
    }
    select_row(menu, scene, None);
    // The plates and the numbers.
    for (plate, g) in [(nodes.munny, group::MUNNY), (nodes.time, group::TIME)] {
        scene.nodes[plate].visible = true;
        scene.set_group_then(plate, g[0], g[2]);
    }
    // The area name, right-aligned above the orange line.
    let name = scene.text.as_ref().and_then(|t| {
        let (world, n) = (area.name.get(..2)?, area.name.get(2..)?.parse::<usize>().ok()?);
        let tokens = t.place_name(world, n)?;
        Some(crate::menu_text::Label {
            tokens,
            style: Style { font: 2, size: Some(16.0), color: [0xff, 0x8f, 0x10, 0x80], align: 1, world: t.world, ..Style::default() },
            offset: [at::AREA_RIGHT, at::AREA_TOP],
        })
    });
    scene.nodes[nodes.area].label = name;
    scene.nodes[nodes.area].visible = true;
    // The party cards.
    show_cards(menu, scene, cards);
    menu_sound::play(se, menu_sound::OPEN);
    info!("menu: open ({} rows, {:?})", menu.rows.len(), menu.rows);
}

/// The left edge of card `i` of `n`: the slot's x of `FUN_00250a00` (0x2f, 0x91, 0xf3 minus `(n * 98 - 98) / n`, the
/// centre of that slot's 3D preview, `menu_preview.rs`) plus the fit constant measured on clip 10 (the card's left edge
/// lies 24 units left of the preview's centre, which the reference frame 8 also shows: Sora's card centre is 24 units right
/// of his model's).
pub fn card_x(i: usize, n: usize) -> f32 {
    let n = n.clamp(1, 3);
    let base = at::CARD_BASE.get(i).copied().unwrap_or(0x10) as f32;
    let slot = at::CARD_FIT + base - ((n as i32 * at::CARD_STEP - at::CARD_STEP) / n as i32) as f32;
    // The cards of the HD reference stand 104 apart (the previews 98), the row's middle where it was.
    slot + (i as f32 - (n as f32 - 1.0) / 2.0) * (at::CARD_PITCH - at::CARD_STEP as f32) + at::CARD_SHIFT
}

/// The cards of the party, laid out as `FUN_00250a00` does (the code's x per slot minus the share of
/// the extra width, then the constant that fits clip 10).
fn show_cards(menu: &Menu, scene: &mut MenuScene, party: &[CardData]) {
    // At most three cards (`FUN_00250a00` caps the party count `FUN_00282940` at 3).
    let n = party.len().min(3);
    for (i, c) in party.iter().take(n).enumerate() {
        let x = card_x(i, n);
        let nodes = &menu.nodes.cards[i * CARD_NODES..(i + 1) * CARD_NODES];
        card(scene, nodes, x, c);
    }
    for (i, &id) in menu.nodes.cards.iter().enumerate() {
        if i / CARD_NODES >= n {
            scene.nodes[id].visible = false;
        }
    }
}

/// One party card, measured on the HD reference frame (8.webp, 1920 x 1080, 2.6 pixels per unit by the row pitch of 26): one
/// dark see-through body for plate and values alike, a light rim along the top (the plate's top corner at the left is cut
/// 45 degrees), a second light rim where the plate ends, a dim rim down the left edge, the name centred in the plate, then
/// the rows LV (yellow), HP (green) and MP (cyan) with their values right-aligned, and under HP and MP a coloured line that
/// is a gauge (its lit part is the share of the member's HP or MP; the rest is a dim track). The widget that draws it in the
/// original (`FUN_00295bc0`'s object, vtable +0x18, called by `FUN_00250a00`) was not decoded [GAP]; the shapes and
/// colours are measured, the numbers are the app's. Every number below is units from the card's left or top.
fn card(scene: &mut MenuScene, nodes: &[usize], x: f32, c: &CardData) {
    let top = at::CARD_TOP;
    let (w, h) = (at::CARD_W, at::CARD_H);
    let rim = [0x3a, 0x3a, 0x3a, 0x80];
    let rect = |l: f32, t: f32, r: f32, b: f32, color: [u8; 4]| ([x + l, top + t, x + r, top + b], [color; 4]);
    // The gauges' lit part: the line spans 4.2 .. w - 4.4; an empty gauge keeps a one-unit stub.
    let (gl, gr) = (4.2, w - 4.4);
    let share = |v: i32, max: i32| if max > 0 { (v.max(0) as f32 / max as f32).clamp(0.0, 1.0) } else { 0.0 };
    let lit = |v: i32, max: i32| gl + (gr - gl) * share(v, max);
    let mut shapes: Vec<([f32; 4], [[u8; 4]; 4])> = vec![
        rect(0.0, 0.0, w, h, [0x02, 0x02, 0x04, 0x6c]),
        rect(7.5, 0.0, w - 1.5, 2.5, rim),
        rect(0.0, 24.2, w - 1.5, 26.4, [0x32, 0x32, 0x32, 0x80]),
        rect(0.0, 8.5, 2.3, h, [0x16, 0x16, 0x16, 0x80]),
    ];
    // The cut corner: four steps from (0, 8.5) up to (7.5, 0).
    for k in 0..4 {
        let k = k as f32;
        shapes.push(rect(1.9 * k, 6.4 - 2.1 * k, 1.9 * k + 2.6, 8.8 - 2.1 * k, rim));
    }
    shapes.extend([
        rect(gl, 53.8, gr, 55.9, [0x01, 0x1c, 0x02, 0x80]),
        rect(gl, 53.8, lit(c.hp, c.max_hp), 55.9, [0x02, 0x64, 0x01, 0x80]),
        rect(gl, 69.0, gr, 71.0, [0x07, 0x17, 0x1f, 0x80]),
        rect(gl, 69.0, lit(c.mp, c.max_mp), 71.0, [0x09, 0x55, 0x74, 0x80]),
    ]);
    debug_assert_eq!(shapes.len(), CARD_SHAPES);
    for (k, (rect, color)) in shapes.into_iter().enumerate() {
        let n = &mut scene.nodes[nodes[k]];
        n.visible = true;
        n.label = None;
        n.fill = Some((rect, color));
    }
    let Some(text) = scene.text.as_ref() else { return };
    // Sizes and widths fitted to clip 10 (menu frame 0.5, 2.163 pixels per unit, the same pixel-box method on the app's
    // shot): the HP and MP digits are 8.3..8.8 high, 7.9..8.3 wide, 10.6 apart (the app's 12.0 gave 8.1, 7.5, 9.96: 6 % small), the
    // LV letters 8.8 high and 14.3 wide (12.5 gave 10.0 and 14.3: too tall, so a smaller size at a wider advance).
    let mk = |s: &str, size: f32, color: [u8; 4], align: u8, at: [f32; 2]| {
        let mut l = text.literal(s, Style { font: 0, size: Some(size), color, align, ..Style::default() }, at);
        if size == at::CARD_LV_SIZE {
            l.tokens.insert(0, kh2_image::text::Token::Width(at::CARD_LV_WIDTH));
        }
        l
    };
    let yellow = [0xf3, 0xd9, 0x28, 0x80];
    let green = [0x0c, 0xc7, 0x0c, 0x80];
    let cyan = [0x10, 0xc7, 0xe7, 0x80];
    let white = [0xe7, 0xe7, 0xe7, 0x80];
    let right = x + w - 3.6;
    let name_at = [x + w / 2.0 + 0.5, top + 7.3];
    let labels = [
        c.name_msg
            .and_then(|id| text.message(id, Style { font: 0, size: Some(15.0), color: white, align: 2, ..Style::default() }, name_at))
            .unwrap_or_else(|| mk(&c.name, 15.0, white, 2, name_at)),
        mk("LV", at::CARD_LV_SIZE, yellow, 0, [x + 4.4, top + 30.0]),
        mk(&c.level.to_string(), at::CARD_LV_SIZE, yellow, 1, [right, top + 30.0]),
        mk("HP", 12.5, green, 0, [x + 4.4, top + 44.5]),
        mk(&format!("{}/{}", c.hp.max(0), c.max_hp), at::CARD_VALUE_SIZE, green, 1, [right, top + 44.5]),
        mk("MP", 12.5, cyan, 0, [x + 4.4, top + 59.5]),
        mk(&format!("{}/{}", c.mp.max(0), c.max_mp), at::CARD_VALUE_SIZE, cyan, 1, [right, top + 59.5]),
    ];
    for (k, l) in labels.into_iter().enumerate() {
        let n = &mut scene.nodes[nodes[CARD_SHAPES + k]];
        n.visible = true;
        n.fill = None;
        n.label = Some(l);
    }
}

/// The box of a group's quads at its first frame drawn at `pos` (x0, y0, x1, y1), `None` when it draws nothing.
fn rest_box(scene: &MenuScene, seq: usize, group: usize, pos: [f32; 2], frame: Option<i32>) -> Option<[f32; 4]> {
    let s = scene.seqs.get(seq)?;
    let frame = frame.unwrap_or_else(|| (s.group_len(group) - 1).max(0));
    let qs = s.quads(group, frame, pos, &kh2_image::seqd::Ext::default());
    let mut b = [f32::MAX, f32::MAX, f32::MIN, f32::MIN];
    for p in qs.iter().flat_map(|q| q.pos) {
        b = [b[0].min(p[0]), b[1].min(p[1]), b[2].max(p[0]), b[3].max(p[1])];
    }
    (!qs.is_empty()).then_some(b)
}

fn set_layout(layd: &Layd, nodes: &Nodes, scene: &mut MenuScene, n: usize) {
    let layers = layd.layout(n);
    // The layout's layers are the frame (backdrop, header band, side rails, bottom panels): each is carried out to the
    // window's edges where it reaches the canvas's (`menu_gfx::stretch_quads`). Its place at rest: the enter group's
    // last frame (layout 0), the hold and leave groups' first (layouts 1 and 2).
    for (i, &id) in nodes.layers.iter().enumerate() {
        let frame = if n == 0 { None } else { Some(0) };
        let rest = layers.get(i).and_then(|l| rest_box(scene, l.seq, l.group, [l.x as f32, l.y as f32], frame));
        scene.nodes[id].stretch = if std::env::var_os("KH2_MENU_NO_STRETCH").is_some() { None } else { rest };
        if n == 0
            && let Some(r) = rest
        {
            info!("menu: frame layer {i} rest box {r:?}, window view {:?} (sides at the canvas edges are carried out)", scene.view());
        }
        match layers.get(i) {
            Some(l) => {
                scene.nodes[id].visible = true;
                scene.nodes[id].seq = Some(l.seq);
                scene.nodes[id].pos = [l.x as f32, l.y as f32];
                scene.set_group(id, l.group);
            }
            None => scene.nodes[id].visible = false,
        }
    }
}

fn close(menu: &mut Menu, scene: &mut MenuScene, se: &mut MessageWriter<PlaySe>, start: bool) {
    menu.phase = Phase::Closing;
    menu.frames = 0;
    menu_sound::play(se, if start { menu_sound::CLOSE } else { menu_sound::CANCEL });
    let nodes = &menu.nodes;
    set_layout(&menu.layd, nodes, scene, 2);
    scene.set_group(nodes.panel, group::PANEL[2]);
    for (k, &r) in nodes.rows.iter().enumerate() {
        if let Some(&e) = menu.rows.get(k) {
            let entry = &menu.entries[e];
            let g = if k == menu.cursor { entry.selected[1] } else { entry.idle[1] };
            scene.set_group(r, usize::from(g));
        }
    }
    scene.nodes[nodes.pointer].visible = false;
    scene.nodes[nodes.glint].visible = false;
    scene.set_group(nodes.munny, group::MUNNY[1]);
    scene.set_group(nodes.time, group::TIME[1]);
    info!("menu: close");
}

/// The per-frame work: step the players, run the phase clocks, keep the numbers current.
fn tick(
    mut menu: ResMut<Menu>,
    mut scene: ResMut<MenuScene>,
    real: Res<Time<Real>>,
    progress: Option<Res<Progress>>,
    tags: Res<new_tags::NewTags>,
) {
    if !menu.loaded || menu.phase == Phase::Closed {
        return;
    }
    if menu.page == 0 {
        update_tags(&menu, &mut scene, &tags);
    }
    let ticks = scene.advance(real.delta_secs());
    menu.frames += ticks;
    // Held up or down repeats without wrapping.
    if menu.held.0 != 0 && menu.phase != Phase::Closing {
        let (dir, t) = menu.held;
        menu.held.1 = t + ticks;
        if menu.held.1 >= REPEAT_DELAY && (menu.held.1 - REPEAT_DELAY) / REPEAT_EVERY != (t.max(REPEAT_DELAY) - REPEAT_DELAY) / REPEAT_EVERY.max(1) {
            let old = menu.cursor;
            let new = step_cursor(old, menu.rows.len(), dir, false);
            if new != old {
                menu.cursor = new;
                info!("menu: page {} cursor {} (repeat)", menu.page, new);
                let m = &*menu;
                select_row(m, &mut scene, Some(old));
            }
        }
    }
    let nodes_layers = menu.nodes.layers.clone();
    match menu.phase {
        Phase::Opening => {
            let done = nodes_layers.iter().filter(|&&l| scene.nodes[l].visible).all(|&l| scene.nodes[l].player.finished());
            if done {
                info!("menu: open animation done after {} ticks ({} sequence frames per 60 Hz tick)", menu.frames, scene.speed);
                if std::env::var_os("KH2_MENU_QUADS").is_some() {
                    scene.resolve();
                    for i in 0..scene.nodes.len() {
                        for q in scene.node_quads(i) {
                            let xs = q.pos.map(|p| p[0]);
                            let ys = q.pos.map(|p| p[1]);
                            let mn = |v: [f32; 4]| v.into_iter().fold(f32::MAX, f32::min);
                            let mx = |v: [f32; 4]| v.into_iter().fold(f32::MIN, f32::max);
                            info!(
                                "menu quad: node {i} seq {:?} group {:?} box ({:.0},{:.0})-({:.0},{:.0}) uv {:?} col {:?}",
                                scene.nodes[i].seq,
                                scene.nodes[i].player.group,
                                mn(xs),
                                mn(ys),
                                mx(xs),
                                mx(ys),
                                q.uv,
                                q.color[0]
                            );
                        }
                    }
                }
                menu.phase = Phase::Open;
                let m = &*menu;
                set_layout(&m.layd, &m.nodes, &mut scene, 1);
            }
        }
        Phase::Closing => {
            scene.dim = dim_alpha(close_brightness(menu.frames));
            let fade = 1.0 - (menu.frames as f32 / 8.0).min(1.0);
            let ids: Vec<usize> = menu.nodes.rows.iter().copied().chain([menu.nodes.area, menu.nodes.help]).collect();
            for id in ids {
                scene.nodes[id].label_fade = fade;
            }
            if menu.frames >= CLOSE_FRAMES {
                menu.phase = Phase::Closed;
                scene.hidden = true;
                scene.dim = 0.0;
                for n in &mut scene.nodes {
                    n.visible = false;
                }
            }
        }
        _ => {}
    }
    if menu.phase != Phase::Closed {
        refresh(&menu, &mut scene, progress.as_deref());
    }
}

/// The NEW tag of each row of the list (`FUN_00251100`): the group says unseen (0x32) or all seen (0x31); a row
/// with no fresh member has no tag.
fn update_tags(menu: &Menu, scene: &mut MenuScene, tags: &new_tags::NewTags) {
    for (k, &t) in menu.nodes.tags.iter().enumerate() {
        let want = menu.rows.get(k).and_then(|&e| new_tags::NewTags::set_of_entry(e)).and_then(|s| tags.row_group(s));
        match want {
            Some(g) => {
                // The tag sits at the row group's own x and y (the plate's right end) from the row's origin.
                let row_group = scene.nodes[menu.nodes.rows[k]].player.group.unwrap_or(0);
                scene.nodes[t].off = scene.extent(SEQ_MAIN, row_group);
                scene.nodes[t].visible = true;
                if scene.nodes[t].player.group != Some(g) {
                    scene.set_group(t, g);
                }
            }
            None => scene.nodes[t].visible = false,
        }
    }
}

/// The Munny and Time digits follow the game state.
fn refresh(menu: &Menu, scene: &mut MenuScene, progress: Option<&Progress>) {
    let munny = progress.map_or(0, |p| p.wallet());
    let digits = menu_pages::digits(munny, menu.nodes.munny_digits.len());
    for (i, &id) in menu.nodes.munny_digits.iter().enumerate() {
        let from_right = digits.len() as isize - 1 - i as isize;
        let n = &mut scene.nodes[id];
        match (from_right >= 0).then(|| digits[i]) {
            Some(d) => {
                let want = group::MUNNY_DIGIT + usize::from(d);
                n.pos = [at::DIGIT_RIGHT - at::DIGIT_PITCH * from_right as f32, at::MUNNY_TOP];
                // Visible on every refresh: the close hid every node while the group stayed the same.
                n.visible = true;
                if n.player.group != Some(want) {
                    scene.set_group(id, want);
                }
            }
            None => n.visible = false,
        }
    }
    let (h, m) = menu_pages::clock(menu.play as u64);
    // "H:MM": hours (one or more digits), the colon, two minute digits, from the right.
    let mut cells: Vec<(usize, f32)> = Vec::new();
    let mut x = at::DIGIT_RIGHT;
    for d in [m % 10, m / 10] {
        cells.push((group::TIME_DIGIT + d as usize, x));
        x -= at::DIGIT_PITCH;
    }
    x += at::DIGIT_PITCH - at::COLON_PITCH;
    cells.push((group::COLON, x));
    for d in menu_pages::digits(h, 3).into_iter().rev() {
        x -= at::DIGIT_PITCH;
        cells.push((group::TIME_DIGIT + usize::from(d), x));
    }
    for (i, &id) in menu.nodes.time_digits.iter().enumerate() {
        match cells.get(i) {
            Some(&(g, x)) => {
                scene.nodes[id].pos = [x, at::TIME_TOP];
                scene.nodes[id].visible = true;
                if scene.nodes[id].player.group != Some(g) {
                    scene.set_group(id, g);
                }
            }
            None => scene.nodes[id].visible = false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Without a mission layout, event or stop the Start button is free; the locked states are the
    /// mission's own predicates `event_active` / `control_locked` / `fight_stopped` (tests in `struggle.rs`).
    #[test]
    fn start_is_free_without_a_mission_event() {
        assert!(!start_locked(None));
        assert!(!start_locked(Some(&crate::struggle::Mission::default())));
    }

    /// The menu's sequence step (docs/menus.md 1.6.1): one frame per 60 Hz tick. Checked against the executable
    /// (skipped without `KH2_ISO`): the 2D objects' speed +0x44 starts at 0x37f3c8 / 0x37f404 = 1.0 and one vblank adds
    /// 0x379170 = 1.0 to `DAT_00356f44`.
    #[test]
    fn menu_sequences_step_one_frame_per_tick() {
        assert_eq!(PLAYER_SPEED, 1.0);
        let mut scene = MenuScene::default();
        scene.speed = PLAYER_SPEED;
        assert_eq!(scene.advance(15.0 / 60.0 + 0.001), 15, "a quarter second is 15 ticks");
        let Some(iso) = crate::assets::iso_path().ok().filter(|p| p.exists()) else {
            eprintln!("skipped the disc part: no disc (set KH2_ISO)");
            return;
        };
        let exe = Disc::open(&iso).read_root("SLUS_210.05").and_then(Exe::load).expect("executable");
        let f32_at = |va: u32| f32::from_bits(u32::from(exe.u16(va).unwrap()) | u32::from(exe.u16(va + 2).unwrap()) << 16);
        for va in [0x0037_f3c8, 0x0037_f404, 0x0037_9170] {
            println!("{va:#x} = {}", f32_at(va));
            assert_eq!(f32_at(va), PLAYER_SPEED, "{va:#x}");
        }
    }

    /// Three party cards stand side by side, 98 apart (`FUN_00250a00`), each 24 units right of its member's 3D preview.
    #[test]
    fn three_cards_stand_side_by_side_under_their_members() {
        assert_eq!([card_x(0, 3), card_x(1, 3), card_x(2, 3)], [204.0, 308.0, 412.0]);
        assert_eq!(card_x(0, 1), 275.0, "one card stands at slot 0's place");
        // The middle card keeps its place (centre 22 units right of its member's); the outer ones are 6 units further out
        // (the HD reference's cards stand 104 apart, the previews 98).
        assert_eq!(card_x(1, 3) + at::CARD_W / 2.0 - 22.0, 256.0 + preview::slot_x(1, 3));
        for i in [0, 2] {
            let d = (card_x(i, 3) + at::CARD_W / 2.0 - 22.0 - 256.0 - preview::slot_x(i, 3)).abs();
            assert_eq!(d, 6.0);
        }
        assert!(card_x(2, 3) + at::CARD_W <= 512.0, "the third card ends inside the canvas");
        assert!(card_x(1, 3) - (card_x(0, 3) + at::CARD_W) > 0.0, "a gap between two cards");
    }

    /// Regression (play-test 2026-10-11, "upon opening the menu most text vanishes"): a second open must show the
    /// labels the first close had faded out.
    #[test]
    fn a_reopen_restores_the_label_fades() {
        let mut scene = MenuScene::default();
        for fade in [0.0, 0.4, 1.0] {
            let mut n = Node::new(None);
            n.label_fade = fade;
            scene.add(n);
        }
        restore_fades(&mut scene);
        assert!(scene.nodes.iter().all(|n| n.label_fade == 1.0));
    }

    /// Regression: the close hid every node but left the digits' groups; the refresh only showed a digit whose group
    /// changed, so the Munny and Time numbers stayed invisible after a reopen.
    #[test]
    fn the_refresh_shows_a_digit_whose_group_did_not_change() {
        let mut scene = MenuScene::default();
        let mut menu = Menu::default();
        for _ in 0..6 {
            let mut n = Node::new(None);
            n.visible = false;
            n.player.group = Some(group::MUNNY_DIGIT);
            menu.nodes.munny_digits.push(scene.add(n));
        }
        for _ in 0..5 {
            let mut n = Node::new(None);
            n.visible = false;
            menu.nodes.time_digits.push(scene.add(n));
        }
        refresh(&menu, &mut scene, None);
        let first = menu.nodes.munny_digits[0];
        assert!(scene.nodes[first].visible, "the last Munny digit (a 0 with an empty wallet) is shown again");
        // The Time cells are placed too (their groups are unset in a node without a sequence, so they show).
        assert!(menu.nodes.time_digits.iter().any(|&t| scene.nodes[t].visible));
    }
}
