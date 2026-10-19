//! The world map's panels, prompts and landing (WP-WM5, stages W6 and W7 of docs/worldmap.md).
//!
//! What the original does (`SLUS_210.05`): on contact the cursor's handler passes the touched symbol to `FUN_002cca98`,
//! which makes the world info panel (`FUN_002cf618` -> `FUN_002ce808`) and removes it when the cursor leaves
//! (`FUN_002cf698`). The confirm button on a symbol makes the window object (`FUN_002cc1e0` -> `FUN_002cbee8`): it asks
//! `FUN_002d1d20` for the destination rows of the world; none gives mode 0, the land prompt (`FUN_002cbcf8`: row 0 the
//! "Land in <world>" text from the u16 table at 0x383520, row 1 the cancel text 0x4884; the result handler
//! `FUN_002cacd8`), some give mode 1, the area list (handler `FUN_002ca6a0`: a locked row shows message
//! 0x4c0d). Its teardown (`FUN_002ca2e8`) starts the landing, `FUN_00104e18(destination, 1, 0, 0, 0)` (`FUN_002cc4b0`),
//! with the destination of `FUN_001c1cf8` (mode 0) or of the chosen row's `00worldpoint.bin` record (mode 1).
//!
//! The port keeps that flow and the data behind it (the `wlds` lookup, the `land` rows of `70landing.bar`, the seen bits
//! of the world points, the exit locks, the texts of `msg/us/wm.bar`). What is not ported, each a gap in docs/worldmap.md:
//! the art of the panels (`wm0command.2dd` holds the camp command deck's art, not these windows, and the layout file
//! `wm_ws.2ld` is unread), so the panels are text in the message window frame of the HUD (the frame kind and the
//! positions are [inferred]); the destination pictures; the episode list (modes 2 and 3); the two forced events of
//! `FUN_002ccd88`; the sounds (`FUN_002d1760`; every menu sound is muted in the app anyway). Cursor input is [inferred]: the attack key (J or the left mouse button) confirms, K, Escape or
//! Backspace cancels, the arrow keys (and W, S) move the row cursor while a window is open. While it is open the pad
//! is held, as the menus do.

use bevy::prelude::*;
use kh2_ard::worldmap as wm;
use kh2_image::text::{Messages, Style, Token};
use kh2_image::window::{self, Class, Phase};

use crate::area::{self, AreaRecord, WORLDS, WORLD_MAP};
use crate::area_jump::{Dest, ExitLocks, JumpTask, KEEP};
use crate::assets::Disc;
use crate::hud::{HudNode, HudScene};
use crate::progress::Progress;
use crate::worldmap::{symbol_world, MapCursor, MapPiece};
use crate::SCALE;

/// Bytes between the executable's addresses and its file offsets.
const EXE_BIAS: usize = 0xff000;
/// The u16 table of the "Land in <world>" texts (`FUN_002cbcf8`), 19 worlds.
const LAND_TEXTS: usize = 0x383520;
/// The u32 table of the world names (`FUN_00105de0`), read for the panel's first line.
const WORLD_NAMES: usize = 0x378040;
/// Worlds the two tables hold.
const WORLD_COUNT: usize = 19;
/// Messages of `msg/us/wm.bar`: the cancel row of the land prompt (`FUN_002cbcf8`) and the locked row (`FUN_002ca6a0`). The
/// caption of the area list is 0x4c15, "Select destination" [inferred]: the disc holds 0x4c10 / 0x4c11 with a button icon and
/// the plain 0x4c15 / 0x4c16 for the destination and the episode list, and `FUN_002cb508` (which uses 0x4c16) therefore builds the
/// episode list, not the area list as docs/worldmap.md section 4 read it.
const MAYBE_LATER: u32 = 0x4884;
const NOT_ACCESSIBLE: u32 = 0x4c0d;
const AREA_CAPTION: u32 = 0x4c15;
/// A message of the system table the label template is made from (`TextSys::message` needs an id that exists).
const TEMPLATE: u32 = 0xa7e;
/// How near the cursor must be to a symbol for contact, in centimetres on the ground plane. The original's test is the
/// actors' collision (`FUN_002cc9d0`, unread): [inferred] a distance.
pub const CONTACT_RADIUS: f32 = 1500.0;
/// The row the cursor is on is drawn in this colour (the original uses a pointer group of the unread layout) [inferred].
const SELECTED: [u8; 4] = [0xff, 0xd8, 0x50, 0x80];

/// The info panel's and the window's frame classes (font and colour as the event windows use, class 0 of `kh2_image::window`;
/// kind 3 and 0 frames, the positions are [inferred]).
const INFO_CLASS: Class = Class { ctor: 0, font: 3, size: None, color: 5, x: 256, y: 70, kind: 3, align: 3, width: None, y_from_top: false };
const WINDOW_CLASS: Class = Class { ctor: 0, font: 3, size: None, color: 5, x: 256, y: 240, kind: 0, align: 3, width: None, y_from_top: false };

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
    Confirm,
    Cancel,
}

/// The window's two modes that the app has (`FUN_002cbee8`'s mode word +0x1f68; 2 and 3 are the episode list, not ported).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    /// The land prompt: row 0 lands, row 1 is "Maybe later".
    Land,
    /// The area list of a world that has seen destinations.
    Areas,
}

/// One row of the window: its text and where it lands (`None`: the cancel row).
#[derive(Clone, Debug, PartialEq)]
pub struct Row {
    pub tokens: Vec<Token>,
    pub dest: Option<Dest>,
    /// The row's area is locked (`FUN_00195010` result 2): a confirm shows message 0x4c0d.
    pub locked: bool,
}

/// What a press did.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    None,
    /// The row cursor moved.
    Moved,
    /// The window closes without a landing (row 1 of the land prompt, or a cancel: results 0 and -2 of `FUN_002cacd8`).
    Close,
    /// Land here (result 1; the teardown calls `FUN_00104e18(dest, 1, 0, 0, 0)`).
    Land(Dest),
    /// A locked row was confirmed: the "not accessible" box.
    Locked,
}

/// The window's state, apart from the scene (tested without a disc).
#[derive(Clone, Debug, PartialEq)]
pub struct Window {
    pub world: u8,
    pub mode: Mode,
    pub rows: Vec<Row>,
    pub cursor: usize,
}

impl Window {
    pub fn press(&mut self, p: Press) -> Outcome {
        let n = self.rows.len();
        match p {
            Press::Up | Press::Down if n > 1 => {
                self.cursor = if p == Press::Up { (self.cursor + n - 1) % n } else { (self.cursor + 1) % n };
                Outcome::Moved
            }
            Press::Up | Press::Down => Outcome::None,
            Press::Cancel => Outcome::Close,
            Press::Confirm => match self.rows.get(self.cursor) {
                Some(Row { dest: Some(_), locked: true, .. }) => Outcome::Locked,
                Some(Row { dest: Some(d), .. }) => Outcome::Land(*d),
                _ => Outcome::Close,
            },
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
/// (docs/progress.md), so a world's landing key (record byte 1) is 0, a state no `wlds` record has. Then the world's first
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
        rows.push((r.row, AreaRow { dest, locked: locked(p.world, p.area) }));
    }
    rows.sort_by_key(|(row, _)| *row);
    rows.into_iter().map(|(_, r)| r).collect()
}

/// The first episode of `world` whose bit is set (`FUN_001c14e8(world, 0)` with `FUN_001c15f0`): its title and hint text ids.
/// The info panel shows these two lines [inferred: `FUN_002c9548` / `FUN_002c9580` read a table whose owner is unread].
pub fn objective(epsd: &[wm::Episode], world: u8, seen: impl Fn(u32) -> bool) -> Option<(u16, u16)> {
    epsd.iter().find(|e| e.world == world && seen(u32::from(e.id))).map(|e| (e.title, e.hint))
}

/// The symbol the cursor touches: the nearest within `radius` (ground plane), with its world.
pub fn touched(cursor: Vec2, symbols: &[(u8, Vec2)], radius: f32) -> Option<u8> {
    symbols.iter().map(|(w, p)| (*w, p.distance(cursor))).filter(|(_, d)| *d <= radius).min_by(|a, b| a.1.total_cmp(&b.1)).map(|(w, _)| w)
}

/// The tokens of a list: the cursor's row in the selected colour, one row per line.
pub fn list_tokens(rows: &[&[Token]], cursor: usize, head: Option<&[Token]>) -> Vec<Token> {
    let mut out = Vec::new();
    if let Some(h) = head {
        out.extend_from_slice(h);
        out.push(Token::Newline);
    }
    for (i, r) in rows.iter().enumerate() {
        if i == cursor {
            out.push(Token::Rgba(SELECTED));
        }
        out.extend_from_slice(r);
        if i == cursor {
            out.push(Token::Reset);
        }
        if i + 1 < rows.len() {
            out.push(Token::Newline);
        }
    }
    out
}

/// The tables and texts the map reads from the disc.
struct Data {
    messages: Messages,
    wlds: Vec<wm::Landing>,
    land: Vec<wm::LandRow>,
    points: Vec<wm::WorldPoint>,
    epsd: Vec<wm::Episode>,
    exe: Vec<u8>,
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
        let land = wm::parse_land(kh2_bar::parse(&landing).ok()?.iter().find(|e| e.tag == wm::LAND)?.data);
        let points = wm::parse_worldpoints(&disc.read("00worldpoint.bin")?);
        let exe = disc.read_root("SLUS_210.05")?;
        Some(Data { messages, wlds, land, points, epsd, exe })
    }

    fn word(&self, va: usize, size: usize) -> Option<u32> {
        let at = va.checked_sub(EXE_BIAS)?;
        let b = self.exe.get(at..at + size)?;
        Some(b.iter().rev().fold(0u32, |a, x| (a << 8) | u32::from(*x)))
    }

    /// A message of `wm.bar`; its end token is dropped so lines can be joined (the layout stops at it).
    fn text(&self, id: u32) -> Option<Vec<Token>> {
        self.messages.tokens(id & 0x7fff).map(unended)
    }

    /// The "Land in <world>" text of a world (`FUN_002cbcf8`).
    fn land_text(&self, world: u8) -> Option<Vec<Token>> {
        if world as usize >= WORLD_COUNT {
            return None;
        }
        self.text(self.word(LAND_TEXTS + 2 * world as usize, 2)?)
    }

    /// The world's name (`FUN_00105de0`): the id is in the wm table or else the system table.
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

/// What the map reads of the story, copied out so its lock is not held while other code (the exit locks) takes it again.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Snap {
    /// The world points seen (`FUN_00195010`).
    pub points: Vec<usize>,
    /// The 19 world records (`FUN_001b4e80`).
    pub records: Vec<[u8; 4]>,
    /// The ids of the episodes whose bit is set (`FUN_001c15f0`).
    pub episodes: Vec<u32>,
}

impl Snap {
    pub fn of(story: &crate::progress::Story, epsd: &[wm::Episode]) -> Snap {
        Snap {
            points: story.world_points(),
            records: (0..WORLD_COUNT).map(|w| story.world_record(w)).collect(),
            episodes: epsd.iter().map(|e| u32::from(e.id)).filter(|id| story.episode_seen(*id)).collect(),
        }
    }

    fn state(&self, world: u8) -> u8 {
        self.records.get(world as usize).map_or(0, |r| r[1])
    }
}

/// The scene nodes the panels use.
#[derive(Clone, Copy)]
struct Nodes {
    info: usize,
    window: usize,
}

#[derive(Resource, Default)]
pub struct MapUi {
    data: Option<Data>,
    failed: bool,
    near: Option<u8>,
    window: Option<Window>,
    /// The "not accessible" box is up over a locked row.
    notice: bool,
    nodes: Option<Nodes>,
    /// The area the state belongs to; a new area starts clean.
    area: Option<String>,
}

fn uilog() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| std::env::var("KH2_WMLOG").is_ok_and(|v| !v.is_empty() && v != "0"))
}

/// Draws `tokens` in a message window frame on `node` (or hides it with `None`). Without a frame file in the HUD text system the
/// panel is text only.
fn put(scene: &mut HudScene, node: usize, class: &Class, tokens: Option<Vec<Token>>) {
    let Some(tokens) = tokens else {
        let n = &mut scene.nodes[node];
        n.visible = false;
        n.label = None;
        return;
    };
    let Some(t) = scene.text.as_ref() else { return };
    let style = Style { font: class.font, size: class.size, color: t.color(class.color), align: class.align, world: t.world, ..Style::default() };
    let Some(mut label) = t.message(TEMPLATE, style, [0.0; 2]) else { return };
    label.tokens = tokens;
    let bx = window::text_box(t.measure(&label));
    let at = window::place(class, bx);
    label.offset = [(at.text[0] - at.frame[0]) as f32, (at.text[1] - at.frame[1]) as f32];
    let file = t.frame_file;
    let group = window::frame_group(class.kind, Phase::Idle);
    if let Some(f) = file.filter(|f| *f < scene.files.len()) {
        scene.files[f].seq.resize_frame(group, at.size[0], at.size[1]);
        scene.nodes[node].file = f;
        scene.set_group(node, group);
    }
    let n = &mut scene.nodes[node];
    n.pos = [at.frame[0] as f32, at.frame[1] as f32];
    n.label = Some(label);
    n.label_time = 0.0;
    n.visible = true;
}

/// The window's tokens for its current state.
fn window_tokens(w: &Window, data: &Data, notice: bool) -> Vec<Token> {
    if notice {
        return data.text(NOT_ACCESSIBLE).unwrap_or_default();
    }
    let rows: Vec<&[Token]> = w.rows.iter().map(|r| r.tokens.as_slice()).collect();
    let head = (w.mode == Mode::Areas).then(|| data.text(AREA_CAPTION)).flatten();
    list_tokens(&rows, w.cursor, head.as_deref())
}

/// The info panel's tokens for a world: its name, then the objective and hint of its first episode [inferred].
fn info_tokens(data: &Data, world: u8, sys: Option<&Messages>, snap: Option<&Snap>) -> Vec<Token> {
    let mut out = data.world_name(world, sys).unwrap_or_default();
    if let Some((title, hint)) = snap.and_then(|s| objective(&data.epsd, world, |id| s.episodes.contains(&id))) {
        for id in [title, hint] {
            if let Some(t) = data.text(u32::from(id)) {
                out.push(Token::Newline);
                out.extend(t);
            }
        }
    }
    out
}

/// The window a confirm on `world` opens: the area list when the world has seen rows (`FUN_002d1d20` > 0), else the land prompt.
fn open_window(data: &Data, world: u8, snap: &Snap, locks: &ExitLocks, place_name: impl Fn(u8, u8) -> Option<Vec<Token>>, localset: impl Fn(u8, u16) -> Option<u8>) -> Option<Window> {
    let rows = area_rows(&data.land, &data.points, world, |i| snap.points.contains(&i), |w, a| locks.locked(w, a), app_has);
    if !rows.is_empty() {
        let rows = rows.iter().map(|r| Row { tokens: place_name(r.dest.world, r.dest.area).map(unended).unwrap_or_default(), dest: Some(r.dest), locked: r.locked }).collect();
        return Some(Window { world, mode: Mode::Areas, rows, cursor: 0 });
    }
    let state = snap.state(world);
    let dest = land_destination_or_first(&data.wlds, world, state, localset).map(|(d, fallback)| {
        if fallback {
            info!("worldmap: world {world}'s landing key is 0 (no new-game side effects in the app): the first wlds record is used [inferred]");
        }
        d
    });
    let rows = vec![Row { tokens: data.land_text(world)?, dest, locked: false }, Row { tokens: data.text(MAYBE_LATER)?, dest: None, locked: false }];
    Some(Window { world, mode: Mode::Land, rows, cursor: 0 })
}

/// Whether the app has the destination's area: the map itself is not a landing place, and only Twilight Town is loaded.
pub fn app_has(d: &Dest) -> bool {
    d.world != WORLD_MAP && d.world != KEEP && area::area_name(d.world, d.area).is_some()
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
    pieces: Query<(&MapPiece, &Transform, &Visibility)>,
    cursor: Query<&Transform, With<MapCursor>>,
) {
    let on_map = rec.world == WORLD_MAP && rec.run.is_some();
    let name = rec.run.as_ref().map(|r| r.name.clone());
    let Some(scene) = scene.as_deref_mut() else { return };
    if ui.area != name || !on_map {
        // A new area (or the map left): nothing of the old panels stays.
        if let Some(n) = ui.nodes {
            put(scene, n.info, &INFO_CLASS, None);
            put(scene, n.window, &WINDOW_CLASS, None);
        }
        ui.near = None;
        ui.window = None;
        ui.notice = false;
        ui.area = name;
        if !on_map {
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
    if task.as_deref().is_some_and(|t| *t != JumpTask::Idle) {
        // A jump is running: the window is gone (the original's teardown has started it).
        return;
    }
    let nodes = *ui.nodes.get_or_insert_with(|| {
        let file = scene.text.as_ref().and_then(|t| t.frame_file).unwrap_or(0);
        let mut mk = || {
            let mut n = HudNode::new(file, None);
            n.visible = false;
            scene.add(n)
        };
        Nodes { info: mk(), window: mk() }
    });
    let Some(sora) = cursor.single().ok() else { return };
    let confirm = keys.just_pressed(KeyCode::KeyJ) || mouse.just_pressed(MouseButton::Left);
    let cancel = keys.just_pressed(KeyCode::Escape) || keys.just_pressed(KeyCode::Backspace) || keys.just_pressed(KeyCode::KeyK);
    let up = keys.just_pressed(KeyCode::ArrowUp) || keys.just_pressed(KeyCode::KeyW);
    let down = keys.just_pressed(KeyCode::ArrowDown) || keys.just_pressed(KeyCode::KeyS);
    let ui = &mut *ui;
    let Some(data) = ui.data.as_ref() else { return };
    // The story's lock is taken only to copy what is read; `ExitLocks::locked` takes it again.
    let snap = progress.as_ref().map(|p| Snap::of(&p.story(), &data.epsd));
    // Contact: the nearest visible symbol within the radius (docs/worldmap.md section 4).
    let at = |t: &Transform| Vec2::new(t.translation.x, t.translation.z);
    let symbols: Vec<(u8, Vec2)> = pieces.iter().filter(|(_, _, v)| **v != Visibility::Hidden).filter_map(|(p, t, _)| Some((symbol_world(&data.exe, p.object)?, at(t)))).collect();
    let near = touched(at(sora), &symbols, CONTACT_RADIUS * SCALE);
    if let Some(w) = ui.window.as_mut() {
        // The window holds the pad: its presses are its own.
        let press = if up {
            Some(Press::Up)
        } else if down {
            Some(Press::Down)
        } else if confirm {
            Some(Press::Confirm)
        } else if cancel {
            Some(Press::Cancel)
        } else {
            None
        };
        keys.reset_all();
        mouse.reset_all();
        let Some(press) = press else { return };
        if ui.notice {
            // The box closes on any confirm or cancel (`FUN_002ca6a0`).
            if matches!(press, Press::Confirm | Press::Cancel) {
                ui.notice = false;
                put(scene, nodes.window, &WINDOW_CLASS, Some(window_tokens(w, data, false)));
            }
            return;
        }
        let out = w.press(press);
        match out {
            Outcome::None => {}
            Outcome::Moved => {
                if uilog() {
                    info!("worldmap: window cursor row {}", w.cursor);
                }
                put(scene, nodes.window, &WINDOW_CLASS, Some(window_tokens(w, data, false)));
            }
            Outcome::Locked => {
                info!("worldmap: row {} is locked (FUN_00195358): message {NOT_ACCESSIBLE:#x}", w.cursor);
                ui.notice = true;
                put(scene, nodes.window, &WINDOW_CLASS, Some(window_tokens(w, data, true)));
            }
            Outcome::Close => {
                info!("worldmap: window closed without a landing (result 0)");
                ui.window = None;
                put(scene, nodes.window, &WINDOW_CLASS, None);
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
                    ui.window = None;
                    put(scene, nodes.window, &WINDOW_CLASS, None);
                    put(scene, nodes.info, &INFO_CLASS, None);
                }
            }
        }
        return;
    }
    // The info panel follows contact (`FUN_002cf618` on contact, `FUN_002cf698` on leaving).
    if near != ui.near {
        ui.near = near;
        match near {
            Some(w) => {
                info!("worldmap: the cursor touches world {w}: info panel (FUN_002cf618)");
                let tokens = info_tokens(data, w, scene.text.as_ref().and_then(|t| t.messages.as_ref()), snap.as_ref());
                put(scene, nodes.info, &INFO_CLASS, Some(tokens));
            }
            None => {
                info!("worldmap: the cursor left the symbol: info panel removed (FUN_002cf698)");
                put(scene, nodes.info, &INFO_CLASS, None);
            }
        }
    }
    // Confirm on a symbol opens the window (`FUN_002cc1e0`); the key press is the window's, not an attack.
    if let (Some(world), true, Some(snap)) = (near, confirm, snap.as_ref()) {
        keys.reset_all();
        mouse.reset_all();
        let place = |w: u8, a: u8| sys_place(scene, w, a);
        let localset = |w: u8, k: u16| crate::area_jump::disc_localset(&disc, w, k);
        match open_window(data, world, snap, &locks, place, localset) {
            Some(w) => {
                info!("worldmap: confirm on world {world}: window mode {:?}, {} rows (FUN_002cbee8)", w.mode, w.rows.len());
                if uilog() {
                    for (i, r) in w.rows.iter().enumerate() {
                        info!("worldmap: row {i}: {:?} {:?} -> {:?}", kh2_image::text::to_text(&r.tokens), r.tokens, r.dest);
                    }
                }
                put(scene, nodes.window, &WINDOW_CLASS, Some(window_tokens(&w, data, false)));
                ui.window = Some(w);
            }
            None => info!("worldmap: confirm on world {world}: no window (its texts are missing)"),
        }
    }
}

/// The area's name through the HUD text system's place table.
fn sys_place(scene: &HudScene, world: u8, area: u8) -> Option<Vec<Token>> {
    scene.text.as_ref()?.place_name(WORLDS.get(world as usize)?, area as usize)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(dest: Option<Dest>, locked: bool) -> Row {
        Row { tokens: vec![Token::Glyph(1)], dest, locked }
    }

    #[test]
    fn the_land_prompt_lands_on_row_zero_and_closes_on_row_one() {
        let dest = Dest::new(2, 1, 3);
        let mut w = Window { world: 2, mode: Mode::Land, rows: vec![row(Some(dest), false), row(None, false)], cursor: 0 };
        assert_eq!(w.press(Press::Confirm), Outcome::Land(dest), "row 0 is result 1 (FUN_002cacd8)");
        assert_eq!(w.press(Press::Down), Outcome::Moved);
        assert_eq!(w.cursor, 1);
        assert_eq!(w.press(Press::Confirm), Outcome::Close, "row 1 is result 0");
        assert_eq!(w.press(Press::Down), Outcome::Moved);
        assert_eq!(w.cursor, 0, "the cursor wraps");
        assert_eq!(w.press(Press::Cancel), Outcome::Close, "the cancel result -2 closes");
    }

    #[test]
    fn a_locked_row_shows_the_box_and_an_unlocked_one_lands() {
        let (a, b) = (Dest::new(2, 4, 0), Dest::new(2, 5, 1));
        let mut w = Window { world: 2, mode: Mode::Areas, rows: vec![row(Some(a), true), row(Some(b), false)], cursor: 0 };
        assert_eq!(w.press(Press::Confirm), Outcome::Locked);
        w.press(Press::Down);
        assert_eq!(w.press(Press::Confirm), Outcome::Land(b));
        let mut one = Window { world: 2, mode: Mode::Land, rows: vec![row(None, false)], cursor: 0 };
        assert_eq!(one.press(Press::Down), Outcome::None, "a single row does not move");
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
        // Point 2 is another world's area the app lacks, point 9 has no record; the list is in row order.
        assert_eq!(rows, vec![AreaRow { dest: Dest::new(2, 5, 2), locked: false }, AreaRow { dest: Dest::new(2, 4, 1), locked: true }]);
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
        assert_eq!(land_destination_or_first(&records, 2, 0, none), Some((Dest::new(2, 1, 7), true)));
        assert_eq!(land_destination_or_first(&records, 2, 1, |_, _| Some(3)), Some((Dest { world: 2, area: 3, entrance: 0, programs: [0x40; 3] }, false)));
        assert_eq!(land_destination_or_first(&records, 2, 5, none), None);
        assert_eq!(land_destination_or_first(&records, 3, 0, none), None);
    }

    #[test]
    fn contact_takes_the_nearest_symbol_in_reach_and_the_objective_the_first_seen_episode() {
        let symbols = [(2u8, Vec2::new(0.0, 0.0)), (4, Vec2::new(5.0, 0.0)), (6, Vec2::new(100.0, 0.0))];
        assert_eq!(touched(Vec2::new(4.0, 0.0), &symbols, 10.0), Some(4));
        assert_eq!(touched(Vec2::new(50.0, 0.0), &symbols, 10.0), None, "out of reach of every symbol");
        assert_eq!(touched(Vec2::new(0.0, 0.0), &[], 10.0), None);
        let eps = [
            wm::Episode { id: 3, world: 2, order: 0, title: 0x1e02, hint: 0x1e03 },
            wm::Episode { id: 4, world: 2, order: 1, title: 0x1e04, hint: 0x1e05 },
            wm::Episode { id: 5, world: 4, order: 0, title: 0x1e06, hint: 0x1e07 },
        ];
        assert_eq!(objective(&eps, 2, |id| id == 4), Some((0x1e04, 0x1e05)));
        assert_eq!(objective(&eps, 2, |_| false), None, "a fresh game has no episode");
        assert_eq!(objective(&eps, 4, |_| true), Some((0x1e06, 0x1e07)));
    }

    #[test]
    fn the_list_marks_the_cursor_row_and_puts_the_caption_first() {
        let (a, b, head) = ([Token::Glyph(1)], [Token::Glyph(2)], [Token::Glyph(9)]);
        let rows: [&[Token]; 2] = [&a, &b];
        let t = list_tokens(&rows, 1, Some(&head));
        assert_eq!(t, vec![Token::Glyph(9), Token::Newline, Token::Glyph(1), Token::Newline, Token::Rgba(SELECTED), Token::Glyph(2), Token::Reset]);
        assert_eq!(list_tokens(&rows, 0, None).first(), Some(&Token::Rgba(SELECTED)));
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
        let snap = Snap::of(&story, &data.epsd);
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
        let snap = Snap::of(&seen, &data.epsd);
        let point = data.points[47];
        assert_eq!((point.world, point.area), (2, 2), "point 47 is the Usual Spot");
        let list = open_window(&data, world, &snap, &locks, |_, _| Some(vec![Token::Glyph(7)]), |_, _| None).expect("the list opens");
        assert_eq!(list.mode, Mode::Areas);
        assert!(list.rows.iter().any(|r| r.dest == Some(Dest::new(2, 2, point.entrance)) && !r.locked && r.tokens == vec![Token::Glyph(7)]), "{:?}", list.rows);
    }

    /// The window copies the story out before the exit locks read it: both lock the same mutex, so holding the first while
    /// calling the second would hang the game (it did, once).
    #[test]
    fn the_story_lock_is_free_when_the_exit_locks_are_read() {
        let handle = crate::progress::StoryHandle::default();
        let locks = ExitLocks { table: Vec::new(), story: Some(handle.clone()) };
        let snap = Snap::of(&handle.lock(), &[]);
        assert!(!locks.locked(2, 4), "returns instead of waiting on the lock");
        assert_eq!(snap.records.len(), WORLD_COUNT);
    }
}
