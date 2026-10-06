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
use crate::data::GameData;
use crate::menu_gfx::{self, MenuGfxPlugin, MenuScene, Node};
use crate::menu_pages::{self, Entry, Exe, Flags};
use crate::progress::Progress;
use crate::schedule::Sim;
use crate::sora::{Sora, State};

pub struct MenuPlugin;

impl Plugin for MenuPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(MenuGfxPlugin)
            .init_resource::<Menu>()
            // The field stands still while the menu is up: every 60 Hz set waits for it.
            .configure_sets(
                FixedUpdate,
                (
                    Sim::Weapon,
                    Sim::Pose,
                    Sim::Sora,
                    Sim::Enemy,
                    Sim::Detect,
                    Sim::SoraHurt,
                    Sim::EnemyHurt,
                    Sim::Effects,
                    Sim::Sounds,
                    Sim::EffectTick,
                )
                    .run_if(is_closed),
            )
            .add_systems(Startup, load)
            // The game's own readers run in `Update`; the menu takes its keys first, in `PreUpdate`
            // after the keyboard is read and after the harness has pressed its scripted keys.
            .add_systems(PreUpdate, input.after(bevy::input::InputSystems).after(crate::harness::Keys))
            .add_systems(Update, tick.before(crate::schedule::Frame::Popups));
    }
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
/// The menu's sequences step two frames per 60 Hz tick: the clip's 15-frame groups settle in about
/// eight clip frames (docs/menus.md 1.6 [F]). How the game steps a menu sequence was not read.
const PLAYER_SPEED: f32 = 2.0;
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
    pub const ROW_IDLE: [u8; 4] = [0x50, 0x50, 0x58, 0x80];
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
    pub const CARD_W: f32 = 96.0;
    pub const CARD_H: f32 = 76.0;
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
/// Nodes per party card: five shapes and seven texts.
const CARD_NODES: usize = 12;

/// One party card's numbers.
#[derive(Clone, Debug, PartialEq)]
pub struct CardData {
    pub name: String,
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
    rows: Vec<usize>,
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

#[derive(Resource)]
pub struct Menu {
    pub phase: Phase,
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
}

impl Default for Menu {
    fn default() -> Menu {
        Menu {
            phase: Phase::Closed,
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
        }
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
    scene.speed = PLAYER_SPEED;
    scene.hidden = true;
    let Layd { layers, layouts, seqs } = two.layd;
    scene.seqs = seqs;
    menu.layd = Layd { layers, layouts, seqs: Vec::new() };
    menu.entries = entries;
    menu.nodes = build_nodes(&mut scene);
    menu.loaded = true;
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
    for _ in 0..menu_pages::ENTRIES {
        n.rows.push(new(scene, Some(SEQ_MAIN)));
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

/// The party the cards show: the app's party is Sora alone (no companions, docs/party.md).
fn party(sora: Option<&Sora>, data: Option<&GameData>) -> Vec<CardData> {
    let Some(s) = sora else { return Vec::new() };
    vec![CardData {
        name: "Sora".into(),
        level: s.stats.level,
        hp: s.hp,
        max_hp: s.stats.max_hp,
        mp: s.mp,
        max_mp: data.map_or(100, |d| d.sora_max_mp),
    }]
}

#[allow(clippy::too_many_arguments)]
fn input(
    mut menu: ResMut<Menu>,
    mut scene: ResMut<MenuScene>,
    mut keys: ResMut<ButtonInput<KeyCode>>,
    mut mouse: ResMut<ButtonInput<MouseButton>>,
    mut se: MessageWriter<PlaySe>,
    sora: Query<&Sora>,
    progress: Option<Res<Progress>>,
    data: Option<Res<GameData>>,
    area: Res<CurrentArea>,
    real: Res<Time<Real>>,
) {
    if !menu.loaded {
        return;
    }
    menu.play = real.elapsed_secs();
    let start = keys.just_pressed(KeyCode::Enter) || keys.just_pressed(KeyCode::NumpadEnter);
    let cancel = keys.just_pressed(KeyCode::Escape) || keys.just_pressed(KeyCode::Backspace) || keys.just_pressed(KeyCode::KeyK);
    let confirm = keys.just_pressed(KeyCode::KeyJ);
    match menu.phase {
        Phase::Closed => {
            if start {
                // The gate of `FUN_00138758` that the app has: the player is in control of Sora (the
                // original also needs no scene change and no event running [inferred mapping]).
                let state = sora.single().ok().map(|s| s.state);
                let free = matches!(
                    state,
                    Some(State::Ground | State::Air | State::QuickRun | State::Glide | State::Attack { .. } | State::Guard | State::Counter)
                );
                if free {
                    let has_ability = progress.as_ref().is_some_and(|p| !p.sora().abilities.is_empty());
                    open(&mut menu, &mut scene, &mut se, sora.single().ok(), data.as_deref(), &area, has_ability);
                } else {
                    info!("menu: start refused ({state:?})");
                }
            }
        }
        Phase::Opening | Phase::Open => {
            let dir = if keys.pressed(KeyCode::ArrowUp) { -1 } else if keys.pressed(KeyCode::ArrowDown) { 1 } else { 0 };
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
                info!("menu: page 0 row {} refused (no sub-page yet; the original opens page {:?})", menu.cursor, target);
                se.write(PlaySe::number(menu_sound::REFUSED, None));
            } else if let Some(wrap) = step {
                move_cursor(&mut menu, &mut scene, &mut se, dir, wrap);
            }
        }
        Phase::Closing => {}
    }
    // Every press that arrives while the menu is up belongs to the menu.
    if menu.phase != Phase::Closed {
        keys.reset_all();
        mouse.reset_all();
    }
}

/// System sounds of the menu: bank 2 of the sound driver, indices from the executable's tables
/// (docs/menus.md 1.7: kind -> id through 0x36cb48 and 0x37f654: cursor 1, confirm 2, cancel 4,
/// refused 5, close 2, open 3). `FUN_002be49c(2, id, ...)` is bank 2, index `id`
/// (`FUN_002c2b70` calls the same function with the slot's bank at +0x18 and index at +0x10).
mod menu_sound {
    const BANK: u32 = 2000;
    pub const CURSOR: u32 = BANK + 1;
    pub const CANCEL: u32 = BANK + 4;
    pub const REFUSED: u32 = BANK + 5;
    pub const CLOSE: u32 = BANK + 2;
    pub const OPEN: u32 = BANK + 3;
}

fn move_cursor(menu: &mut Menu, scene: &mut MenuScene, se: &mut MessageWriter<PlaySe>, dir: i8, wrap: bool) {
    let old = menu.cursor;
    let new = step_cursor(old, menu.rows.len(), dir, wrap);
    if new == old {
        return;
    }
    menu.cursor = new;
    se.write(PlaySe::number(menu_sound::CURSOR, None));
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
    let label = text.message(id, Style { font: 0, size: Some(15.0), color: [0x30, 0x7c, 0x80, 0x80], ..Style::default() }, at::HELP);
    scene.nodes[menu.nodes.help].label = label;
    scene.nodes[menu.nodes.help].visible = true;
}

fn open(
    menu: &mut Menu,
    scene: &mut MenuScene,
    se: &mut MessageWriter<PlaySe>,
    sora: Option<&Sora>,
    data: Option<&GameData>,
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
            style: Style { font: 2, size: Some(16.0), color: [0x80, 0x48, 0x08, 0x80], align: 1, world: t.world, ..Style::default() },
            offset: [at::AREA_RIGHT, at::AREA_TOP],
        })
    });
    scene.nodes[nodes.area].label = name;
    scene.nodes[nodes.area].visible = true;
    // The party cards.
    show_cards(menu, scene, &party(sora, data));
    se.write(PlaySe::number(menu_sound::OPEN, None));
    info!("menu: open ({} rows, {:?})", menu.rows.len(), menu.rows);
}

/// The cards of the party, laid out as `FUN_00250a00` does (the code's x per slot minus the share of
/// the extra width, then the constant that fits clip 10).
fn show_cards(menu: &Menu, scene: &mut MenuScene, party: &[CardData]) {
    let n = party.len().min(4);
    for (i, c) in party.iter().take(n).enumerate() {
        let base = at::CARD_BASE.get(i).copied().unwrap_or(0x10) as f32;
        let x = at::CARD_FIT + base - ((n as i32 * at::CARD_STEP - at::CARD_STEP) / n as i32) as f32;
        let nodes = &menu.nodes.cards[i * CARD_NODES..(i + 1) * CARD_NODES];
        card(scene, nodes, x, c);
    }
    for (i, &id) in menu.nodes.cards.iter().enumerate() {
        if i / CARD_NODES >= n {
            scene.nodes[id].visible = false;
        }
    }
}

/// One party card from footage (clip 10): a black plate with a light edge, the name above a rule, then
/// the rows LV (yellow), HP (green) and MP (cyan) with their numbers on the right. The widget that draws
/// it in the original (`FUN_00295bc0`'s object, vtable +0x18, called by `FUN_00250a00`) was not decoded
/// [GAP]; the shapes and colours here are measured from the footage, the numbers are the app's.
fn card(scene: &mut MenuScene, nodes: &[usize], x: f32, c: &CardData) {
    let top = at::CARD_TOP;
    let shapes: [([f32; 4], [u8; 4]); 5] = [
        ([x, top, x + at::CARD_W, top + at::CARD_H], [0xb0, 0xb0, 0xb8, 0x80]),
        ([x + 1.0, top + 1.0, x + at::CARD_W - 1.0, top + at::CARD_H - 1.0], [0x06, 0x06, 0x0a, 0x80]),
        ([x + 1.0, top + 24.0, x + at::CARD_W - 1.0, top + 25.0], [0x60, 0x60, 0x68, 0x80]),
        ([x + 4.0, top + 57.0, x + at::CARD_W - 4.0, top + 58.0], [0x18, 0x80, 0x10, 0x80]),
        ([x + 4.0, top + 72.0, x + at::CARD_W - 4.0, top + 73.0], [0x10, 0x70, 0x80, 0x80]),
    ];
    for (k, (rect, color)) in shapes.into_iter().enumerate() {
        let n = &mut scene.nodes[nodes[k]];
        n.visible = true;
        n.label = None;
        n.fill = Some((rect, [color; 4]));
    }
    let Some(text) = scene.text.as_ref() else { return };
    let mk = |s: &str, size: f32, color: [u8; 4], align: u8, at: [f32; 2]| {
        text.literal(s, Style { font: 0, size: Some(size), color, align, ..Style::default() }, at)
    };
    let yellow = [0x80, 0x80, 0x10, 0x80];
    let green = [0x30, 0x80, 0x10, 0x80];
    let cyan = [0x10, 0x80, 0x80, 0x80];
    let right = x + at::CARD_W - 6.0;
    let labels = [
        mk(&c.name, 15.0, [0x80, 0x80, 0x80, 0x80], 2, [x + at::CARD_W / 2.0, top + 5.0]),
        mk("LV", 12.0, yellow, 0, [x + 5.0, top + 28.0]),
        mk(&c.level.to_string(), 13.0, yellow, 1, [right, top + 27.0]),
        mk("HP", 12.0, green, 0, [x + 5.0, top + 43.0]),
        mk(&format!("{}/{}", c.hp.max(0), c.max_hp), 12.0, green, 1, [right, top + 42.5]),
        mk("MP", 12.0, cyan, 0, [x + 5.0, top + 58.0]),
        mk(&format!("{}/{}", c.mp.max(0), c.max_mp), 12.0, cyan, 1, [right, top + 57.5]),
    ];
    for (k, l) in labels.into_iter().enumerate() {
        let n = &mut scene.nodes[nodes[5 + k]];
        n.visible = true;
        n.fill = None;
        n.label = Some(l);
    }
}

fn set_layout(layd: &Layd, nodes: &Nodes, scene: &mut MenuScene, n: usize) {
    let layers = layd.layout(n);
    for (i, &id) in nodes.layers.iter().enumerate() {
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
    se.write(PlaySe::number(if start { menu_sound::CLOSE } else { menu_sound::CANCEL }, None));
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
) {
    if !menu.loaded || menu.phase == Phase::Closed {
        return;
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
                if n.player.group != Some(want) {
                    n.visible = true;
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
                if scene.nodes[id].player.group != Some(g) {
                    scene.nodes[id].visible = true;
                    scene.set_group(id, g);
                }
            }
            None => scene.nodes[id].visible = false,
        }
    }
}
