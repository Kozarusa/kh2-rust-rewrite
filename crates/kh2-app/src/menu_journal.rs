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
//! What the port draws: page 0 only (the layout, the header 0x7559 and the plate 0x7558, the cells with their world
//! pictures). The other pages (1, 2, 3..0x20: the world's lists, characters, enemies, treasures, reports ...) are not
//! ported: a start page of 1 or 2 shows page 0, as one cancel on page 2 does in the original (`FUN_002a3da0` case
//! -2 -> page 0), and a confirm on a cell is refused with a log line.

use bevy::prelude::*;
use kh2_image::layd::{self, Layd};

use crate::assets::{CurrentArea, Disc};
use crate::audio::PlaySe;
use crate::menu::{menu_sound, Menu, Phase};
use crate::menu_gfx::{self, MenuScene, Node};
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
}

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
            Some(World { id: r[0] as i8, code, flag: u16_at(8), flag2: u16_at(0xe) })
        })
        .collect()
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
pub fn first_cursor(worlds: &[World], test: &dyn Fn(u16) -> bool, want: i8) -> Option<usize> {
    let want = if want < 0 { 0xf } else { want };
    let all = test(ALL_WORLDS_FLAG);
    worlds.iter().filter(|w| w.id < 0x14 && (w.id >= 0x10 || test(w.flag) || all)).position(|w| w.id == want)
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

#[derive(Default)]
struct Nodes {
    layer: usize,
    header: usize,
    plate: usize,
    cells: Vec<usize>,
    marks: Vec<[usize; 2]>,
    icons: Vec<usize>,
}

impl Nodes {
    fn all(&self) -> Vec<usize> {
        let mut v = vec![self.layer, self.header, self.plate];
        v.extend(&self.cells);
        v.extend(self.marks.iter().flatten());
        v.extend(&self.icons);
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
    world_index: Vec<i8>,
    layouts: [[usize; 3]; 2],
    header: [usize; 3],
    nodes: Nodes,
    /// The mode a camp row asked for (taken by `tick`).
    request: Option<i32>,
    /// The scene is up: page, cursor, the shown cells, the scene nodes it hid and their visibility.
    open: Option<Open>,
}

struct Open {
    page: u16,
    cursor: usize,
    shown: Vec<(usize, u8)>,
    saved: Vec<(usize, bool)>,
    roxas: bool,
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
    let Some(worlds) = disc.read("menu/us/jiminy.bar").and_then(|b| kh2_bar::parse(&b).ok().and_then(|e| e.first().and_then(|e| parse_worlds(e.data)))) else {
        warn!("journal: menu/us/jiminy.bar entry 0 is not readable; the Journal rows do nothing");
        return;
    };
    let Some(exe) = disc.read_root("SLUS_210.05").and_then(Exe::load) else { return };
    let (Some(index), Some(l0), Some(l1), Some(h)) = (read_world_index(&exe), triple(&exe, addr::LAYOUTS), triple(&exe, addr::LAYOUTS_ROXAS), triple(&exe, addr::HEADER)) else {
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
    let pics = disc.read("menu/us/jm_world.2ld").map(|b| kh2_bar::parse(&b).map(|e| e.iter().map(|e| kh2_image::imgd::decode(e.data)).collect::<Vec<_>>()).unwrap_or_default()).unwrap_or_default();
    j.world_tex = pics
        .into_iter()
        .map(|p| {
            p.map(|img| {
                scene.texs.push(menu_gfx::Tex::new(&img, images));
                scene.texs.len() - 1
            })
        })
        .collect();
    j.seq_base = scene.seqs.len();
    let Layd { layers, layouts, seqs } = two.layd;
    scene.seqs.extend(seqs);
    j.layd = Layd { layers, layouts, seqs: Vec::new() };
    j.worlds = worlds;
    j.world_index = index;
    j.layouts = [l0, l1];
    j.header = h;
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
    j.nodes = nodes;
    j.loaded = true;
    info!("journal: loaded jmemo.2ld, {} world pictures, {} world records", j.world_tex.len(), j.worlds.len());
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
    } else {
        None
    };
    for k in [KeyCode::KeyJ, KeyCode::Escape, KeyCode::Backspace, KeyCode::KeyK, KeyCode::Enter, KeyCode::NumpadEnter, KeyCode::ArrowLeft, KeyCode::ArrowRight, KeyCode::ArrowUp, KeyCode::ArrowDown] {
        keys.clear_just_pressed(k);
    }
    let (Some(key), true) = (key, j.loaded) else { return };
    let Some(o) = j.open.as_ref() else { return };
    match page0(key, o.cursor, &o.shown, &j.worlds) {
        Act::None => {}
        Act::Cursor(c) => {
            let old = o.cursor;
            j.open.as_mut().unwrap().cursor = c;
            select(&j, &mut scene, Some(old));
            menu_sound::play(&mut se, menu_sound::CURSOR);
            info!("journal: page 0 cursor {c}");
        }
        Act::Open { page, world } => {
            menu_sound::play(&mut se, menu_sound::REFUSED);
            info!("journal: page 0 world {world} -> page {page:#x} not ported (FUN_002a3540 and the pages after it); refused");
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

fn show(j: &mut Journal, scene: &mut MenuScene, mode: i32, world_number: Option<usize>, test: &dyn Fn(u16) -> bool) {
    let jw = world_number.and_then(|w| j.world_index.get(w).copied()).unwrap_or(-1);
    let (page, world) = start(mode, jw);
    let roxas = mode == 2;
    // Every node the scene had before this one's (the camp's frame, rows, plates, cards ...) is hidden while the journal is up.
    let first = j.nodes.layer;
    let saved: Vec<(usize, bool)> = (0..first).map(|i| (i, scene.nodes[i].visible)).collect();
    for i in 0..first {
        scene.nodes[i].visible = false;
    }
    let shown = cells(&j.worlds, test);
    let cursor = first_cursor(&j.worlds, test, world).filter(|&c| c < shown.len()).unwrap_or(0);
    info!(
        "journal: open (FUN_002756f8): mode {mode}, journal world {jw}, start page {page} with world byte {world}; {} cells {:?}{}",
        shown.len(),
        shown.iter().map(|&(i, p)| (j.worlds[i].code.clone(), p)).collect::<Vec<_>>(),
        if page == 0 { String::new() } else { format!("; page {page} is not ported: page 0 (the page its cancel shows)") }
    );
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
        // The two children start in group 1 (0x98 / 0x99 / 0x9a follow data the port does not have: docs/menus.md 20.4).
        scene.set_group(m[0], group::START);
        scene.set_group(m[1], group::START);
        let pic = shown[k].1;
        scene.nodes[ic].tex = j.world_tex.get(usize::from(pic)).copied().flatten().unwrap_or(j.tex);
        scene.set_group_then(ic, group::ICON[0], group::ICON[1]);
    }
    j.open = Some(Open { page: 0, cursor, shown, saved, roxas });
    select(j, scene, None);
}

fn close(j: &mut Journal, scene: &mut MenuScene, menu: &mut Menu) {
    let Some(o) = j.open.take() else { return };
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
    if let Some(mode) = j.request.take() {
        if !j.tried && scene.texs.len() >= 2 {
            load(&mut j, &disc, &mut scene, &mut images);
        }
        if !j.loaded {
            menu.page = 0;
            return;
        }
        let world = area.name.get(..2).and_then(|w| crate::audio::WORLDS.iter().position(|&n| n == w));
        let story = progress.as_ref().map(|p| p.story());
        let test = |id: u16| story.as_ref().is_some_and(|s| s.test(id));
        show(&mut j, &mut scene, mode, world, &test);
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
            .map(|&(id, c, f)| World { id, code: c.into(), flag: f, flag2: 0 })
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
        assert_eq!(first_cursor(&w, &tt_only, 0), Some(1));
        assert_eq!(first_cursor(&w, &tt_only, -1), Some(0));
        assert_eq!(first_cursor(&w, &tt_only, 2), None);
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
