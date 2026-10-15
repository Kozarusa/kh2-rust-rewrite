//! The Struggle's result menu (docs/struggle.md "The end", docs/menus.md section 21).
//!
//! What the original does (`SLUS_210.05`): the mission end's last step `FUN_001b96e0` (mission kinds 2, 0x19, 0x20, or
//! event 0x9c) calls `FUN_0024f330(FUN_001bef10(0x1a), 0)`. `FUN_001bef10` finds row 0x1a of the pause-menu record
//! table `DAT_01d5df08`, which `FUN_001d3408` points at the `slct` entry of `14mission.bar` (`FUN_00168510` with the tag
//! at 0x37c558; a u32 header word +4 = the row count, rows of 0x40 bytes from +8, sorted by the u16 key at +0 and found
//! by the binary search `FUN_0030ff40`). `FUN_0024f330` is the same opener as the field Save command: it stores the
//! row as the request block (`FUN_00254a18`), prepares the sound (`FUN_0023e860(FUN_0024f4f0(7))`) and requests menu
//! kind 7 (`FUN_0024f0b8(7, -1)`), whose page machine kind 8 shows the **pointer list** of the block
//! (`FUN_0026f698`): one row per option `i < byte +2` with a non-zero u16 action code at `+4 + 4i` and its message at
//! `+6 + 4i`; the cursor starts at byte `+3` (`FUN_002544b8` -> `FUN_0026f480`). Byte `+0x25` bit 0 clears the list's
//! flags 0x60 (`FUN_0026f508` -> `FUN_00298cf0`): the cancel (-2) and the -4 result of `FUN_00297148` can no longer
//! come, so the player has to pick a row; `FUN_0024f330` also clears word `+0xfc` of `FUN_0014a918`'s object then.
//!
//! Row 0x1a holds two options: code 4 (message 0x8ab2) and code 3 (message 0x8ab1). The page handler
//! `FUN_0026fda0` sends codes 3 and 4 to `FUN_001b9c58`: code 3 -> event 0x8b, code 4 -> event 0x8d (and the party
//! reset `FUN_001abfe8(9)`); the manager ends (`FUN_001b9518`) and the area trigger of the event (`FUN_00197a60` ->
//! `FUN_00197588(event, -1)`) gives the jump record that `FUN_0019d9d8(record, 0x40000000)` takes. The page closes
//! after the row (`FUN_00295ba0`), kind 7 ends and the field scene returns (`FUN_001038a0`).
//!
//! The port: the `slct` row is read from the disc at run time, the row texts are the disc's (`msg/us/sys.bar`), the
//! pointer list's pills and hand are `save.2ld`'s groups as the Save menu draws them (`menu_save.rs`) [inferred: kind
//! 8's resource ids 0x753c/0x7532 were not resolved to a file]; the header group `+0x16` and the frame are not drawn
//! (gap). Codes 3 and 4 end the manager (`struggle::Mission::menu_exit`) and start the trigger's jump when its type is 1
//! (`area_jump::JumpTask`), once the menu's close has finished [inferred: the order of the jump and the scene's end].

use bevy::prelude::*;
use kh2_image::layd;
use kh2_image::text::Style;

use crate::area::AreaRecord;
use crate::area_jump::{disc_localset, resolve_jump, Dest, JumpTask, LastExit};
use crate::assets::Disc;
use crate::audio::PlaySe;
use crate::menu::{close_brightness, dim_alpha, menu_sound, CLOSE_FRAMES, DIM_BRIGHTNESS};
use crate::menu_gfx::{self, MenuScene, Node};
use crate::schedule::Pause;
use crate::struggle::Mission;

/// The pause-menu record table's file and entry tag (`FUN_001d3408`: `PTR_s_14mission_bar_0036076c`, the tag at 0x37c558).
pub const FILE: &str = "14mission.bar";
pub const TAG: &str = "slct";

/// One row of the table (0x40 bytes; the fields the pointer list reads, `FUN_0026f698` / `FUN_002544b8` / `FUN_0026f508`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Block {
    /// +0: the key `FUN_001bef10` searches.
    pub key: u16,
    /// +2: the option count; +4 + 4i: (action code, message), the code 0 options give no row.
    pub options: Vec<(u16, u16)>,
    /// +3: the row the cursor starts on.
    pub cursor: u8,
    /// +0x14: kind 7's header group (`FUN_00254800`, -1 none); +0x16: kind 8's header group (`FUN_0026f8a8(0, ..)`).
    pub layer_group: i16,
    pub header_group: i16,
    /// +0x18: the information message (0 none).
    pub info: u32,
    /// +0x25 bit 0: no cancel (list flags 0x60 cleared).
    pub no_cancel: bool,
}

impl Block {
    /// The rows the list shows: the options with a non-zero code, in order (`FUN_0026f698`).
    pub fn rows(&self) -> Vec<(u16, u16)> {
        self.options.iter().copied().filter(|(c, _)| *c != 0).collect()
    }
}

/// The table of the `slct` entry: header (+0 a word, +4 the row count), rows of 0x40 bytes from +8.
pub fn parse_table(d: &[u8]) -> Vec<Block> {
    let u16_at = |r: &[u8], o: usize| u16::from_le_bytes([r[o], r[o + 1]]);
    let Some(n) = d.get(4..8).map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]) as usize) else { return Vec::new() };
    (0..n)
        .map_while(|i| d.get(8 + i * 0x40..8 + (i + 1) * 0x40))
        .map(|r| {
            let count = usize::from(r[2]).min(4);
            Block {
                key: u16_at(r, 0),
                options: (0..count).map(|i| (u16_at(r, 4 + 4 * i), u16_at(r, 6 + 4 * i))).collect(),
                cursor: r[3],
                layer_group: u16_at(r, 0x14) as i16,
                header_group: u16_at(r, 0x16) as i16,
                info: u32::from_le_bytes([r[0x18], r[0x19], r[0x1a], r[0x1b]]),
                no_cancel: r[0x25] & 1 != 0,
            }
        })
        .collect()
}

/// `FUN_001bef10(key)`: the binary search of `FUN_0030ff40` over the rows sorted by key.
pub fn find(table: &[Block], key: u16) -> Option<&Block> {
    table.binary_search_by_key(&key, |b| b.key).ok().map(|i| &table[i])
}

/// The stages of the menu.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Stage {
    #[default]
    Closed,
    /// The pointer list.
    List,
    /// The dim returns; then the chosen row's effect.
    Closing,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Press {
    Up,
    Down,
    Confirm,
    Cancel,
}

/// What a press does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    None,
    Cursor,
    /// A row was chosen: its action code. The page closes (every code but 0xc).
    Chosen(u16),
    /// The cancel closes the page (only without +0x25 bit 0).
    Cancelled,
}

/// The pointer list's state machine (tested without a disc).
#[derive(Clone, Debug, Default)]
pub struct Flow {
    pub stage: Stage,
    pub rows: Vec<(u16, u16)>,
    pub row: usize,
    pub no_cancel: bool,
    /// The code chosen, applied when the close ends.
    pub chosen: Option<u16>,
}

impl Flow {
    pub fn open(b: &Block) -> Flow {
        let rows = b.rows();
        let row = usize::from(b.cursor).min(rows.len().saturating_sub(1));
        Flow { stage: Stage::List, rows, row, no_cancel: b.no_cancel, chosen: None }
    }

    pub fn press(&mut self, p: Press) -> Action {
        if self.stage != Stage::List || self.rows.is_empty() {
            return Action::None;
        }
        let n = self.rows.len();
        match p {
            // [inferred] the list wraps, as the Save menu's port does (`FUN_002967b8(list, 8, 0)` was not read further).
            Press::Up | Press::Down if n > 1 => {
                self.row = if p == Press::Up { (self.row + n - 1) % n } else { (self.row + 1) % n };
                Action::Cursor
            }
            Press::Up | Press::Down => Action::None,
            Press::Confirm => {
                let code = self.rows[self.row].0;
                if code == 0xc {
                    // Code 0xc shows the picture page (`FUN_0026ff98`) and keeps the page: not ported.
                    return Action::None;
                }
                self.chosen = Some(code);
                self.stage = Stage::Closing;
                Action::Chosen(code)
            }
            Press::Cancel if self.no_cancel => Action::None,
            Press::Cancel => {
                self.stage = Stage::Closing;
                Action::Cancelled
            }
        }
    }
}

/// `save.2ld` sequence 0 groups (the Save menu's pointer list, docs/menus.md 18.2): the selected row (red pill, loops),
/// an idle row (hold), the hand; and the placements the Save menu's port uses [inferred there from the footage].
mod g {
    pub const ROW_SELECTED: usize = 77;
    pub const ROW_IDLE: usize = 79;
    pub const HAND: usize = 12;
    pub const ROW_LABEL: [f32; 2] = [70.0, 80.0];
    pub const ROW_PITCH: f32 = 26.0;
    pub const HAND_AT: [f32; 2] = [62.0, 80.0];
    pub const ROW_IDLE_COLOR: [u8; 4] = [0x9f, 0x9f, 0xaf, 0x80];
    pub const ROW_SELECTED_COLOR: [u8; 4] = [0xe5, 0xe5, 0xe5, 0x80];
}

const ROWS: usize = 4;

#[derive(Resource, Default)]
pub struct StruggleMenu {
    pub flow: Flow,
    tried: bool,
    loaded: bool,
    /// A code chosen while a jump was running; applied once the jump task is idle.
    pending: Option<u16>,
    table: Vec<Block>,
    tex: usize,
    seq: usize,
    rows: Vec<(usize, usize)>,
    hand: usize,
    frames: u32,
}

impl StruggleMenu {
    pub fn is_open(&self) -> bool {
        self.flow.stage != Stage::Closed
    }
}

pub struct StruggleMenuPlugin;

impl Plugin for StruggleMenuPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<StruggleMenu>()
            .add_systems(PreUpdate, input.after(crate::menu::input).after(bevy::input::InputSystems).after(crate::harness::Keys))
            .add_systems(Update, tick.before(crate::schedule::Frame::Popups));
    }
}

fn load(m: &mut StruggleMenu, disc: &Disc, scene: &mut MenuScene, images: &mut Assets<Image>) {
    m.tried = true;
    let Some(bar) = disc.read(FILE) else {
        warn!("struggle menu: {FILE} is not readable; the result menu is off");
        return;
    };
    let Ok(entries) = kh2_bar::parse(&bar) else { return };
    let Some(e) = entries.iter().find(|e| e.tag == TAG) else {
        warn!("struggle menu: {FILE} has no {TAG} entry; the result menu is off");
        return;
    };
    m.table = parse_table(e.data);
    let Some(two) = disc.read("menu/us/save.2ld").and_then(|b| layd::parse_2ld(&b)) else {
        warn!("struggle menu: menu/us/save.2ld is not readable; the result menu is off");
        return;
    };
    let (Some(img), Some(seq)) = (two.images.first(), two.layd.seqs.first()) else { return };
    if scene.text.is_none() {
        warn!("struggle menu: no menu text; the result menu is off");
        return;
    }
    m.tex = scene.texs.len();
    scene.texs.push(menu_gfx::Tex::new(img, images));
    m.seq = scene.seqs.len();
    scene.seqs.push(seq.clone());
    let new = |scene: &mut MenuScene, s: Option<usize>, t: usize| {
        let mut node = Node::new(s);
        node.tex = t;
        node.visible = false;
        scene.add(node)
    };
    m.rows = (0..ROWS).map(|_| (new(scene, Some(m.seq), m.tex), new(scene, None, m.tex))).collect();
    m.hand = new(scene, Some(m.seq), m.tex);
    m.loaded = true;
    info!("struggle menu: {FILE} {TAG}: {} rows", m.table.len());
}

fn hide_all(m: &StruggleMenu, scene: &mut MenuScene) {
    for &(p, t) in &m.rows {
        for i in [p, t] {
            scene.nodes[i].visible = false;
            scene.nodes[i].label = None;
        }
    }
    scene.nodes[m.hand].visible = false;
}

fn show(m: &StruggleMenu, scene: &mut MenuScene) {
    for (k, &(plate, text)) in m.rows.iter().enumerate() {
        let Some(&(_, msg)) = m.flow.rows.get(k) else {
            scene.nodes[plate].visible = false;
            scene.nodes[text].visible = false;
            continue;
        };
        let selected = k == m.flow.row;
        let dy = g::ROW_PITCH * k as f32;
        scene.nodes[plate].visible = true;
        scene.nodes[plate].pos = [0.0, dy];
        let want = if selected { g::ROW_SELECTED } else { g::ROW_IDLE };
        if scene.nodes[plate].player.group != Some(want) {
            scene.set_group(plate, want);
        }
        let color = if selected { g::ROW_SELECTED_COLOR } else { g::ROW_IDLE_COLOR };
        // Bit 15 of a stored id is ignored by the text lookup (`FUN_00190c40`).
        let label = scene.text.as_ref().and_then(|t| {
            t.message(u32::from(msg & 0x7fff), Style { font: 1, size: Some(15.0), color, ..Style::default() }, [g::ROW_LABEL[0], g::ROW_LABEL[1] + dy])
        });
        scene.nodes[text].label = label;
        scene.nodes[text].visible = true;
    }
    scene.nodes[m.hand].visible = true;
    scene.nodes[m.hand].pos = [g::HAND_AT[0], g::HAND_AT[1] + g::ROW_PITCH * m.flow.row as f32];
    if scene.nodes[m.hand].player.group != Some(g::HAND) {
        scene.set_group(m.hand, g::HAND);
    }
}

#[allow(clippy::too_many_arguments)]
fn input(
    mut m: ResMut<StruggleMenu>,
    mut mission: ResMut<Mission>,
    mut scene: ResMut<MenuScene>,
    mut keys: ResMut<ButtonInput<KeyCode>>,
    mut se: MessageWriter<PlaySe>,
    mut pause: ResMut<Pause>,
    disc: Res<Disc>,
    mut images: ResMut<Assets<Image>>,
) {
    if let Some(key) = mission.menu_request.take() {
        if !m.tried {
            load(&mut m, &disc, &mut scene, &mut images);
        }
        match find(&m.table, key).cloned() {
            Some(b) if m.loaded && !m.is_open() => {
                m.flow = Flow::open(&b);
                m.frames = 0;
                scene.hidden = false;
                scene.dim = dim_alpha(DIM_BRIGHTNESS);
                hide_all(&m, &mut scene);
                show(&m, &mut scene);
                // FUN_0026f0d0: the open sound kind 0xb (id 3).
                menu_sound::play(&mut se, menu_sound::OPEN);
                info!(
                    "struggle menu: open record {key:#x} (FUN_0024f330 -> menu kind 7): rows {:x?}, cursor {}, no cancel {}, header group {}, info {:#x}",
                    m.flow.rows, m.flow.row, b.no_cancel, b.header_group, b.info
                );
            }
            _ => {
                // Without the menu the manager ends as the menu's close would leave it [inferred], so the field returns.
                warn!("struggle menu: record {key:#x} cannot open (loaded {}); the manager ends", m.loaded);
                mission.menu_exit(0);
            }
        }
    }
    if !m.is_open() {
        return;
    }
    // A menu is a scene of its own: the field stops while it is up (docs/menus.md 1.1).
    pause.menu = true;
    let pressed = |k: KeyCode| keys.just_pressed(k);
    let press = if pressed(KeyCode::ArrowUp) || pressed(KeyCode::KeyW) {
        Some(Press::Up)
    } else if pressed(KeyCode::ArrowDown) || pressed(KeyCode::KeyS) {
        Some(Press::Down)
    } else if pressed(KeyCode::KeyJ) || pressed(KeyCode::Enter) || pressed(KeyCode::NumpadEnter) {
        Some(Press::Confirm)
    } else if pressed(KeyCode::KeyK) || pressed(KeyCode::Escape) || pressed(KeyCode::Backspace) {
        Some(Press::Cancel)
    } else {
        None
    };
    keys.reset_all();
    let Some(p) = press else { return };
    match m.flow.press(p) {
        Action::None => {
            if p == Press::Cancel {
                info!("struggle menu: cancel ignored (+0x25 bit 0: list flags 0x60 cleared)");
            }
        }
        Action::Cursor => {
            menu_sound::play(&mut se, menu_sound::CURSOR);
            info!("struggle menu: cursor {}", m.flow.row);
            show(&m, &mut scene);
        }
        Action::Chosen(code) => {
            menu_sound::play(&mut se, crate::menu_status::CONFIRM);
            info!("struggle menu: row {} chosen, code {code} (FUN_0026fda0)", m.flow.row);
            m.frames = 0;
            hide_all(&m, &mut scene);
        }
        Action::Cancelled => {
            menu_sound::play(&mut se, menu_sound::CANCEL);
            info!("struggle menu: cancelled");
            m.frames = 0;
            hide_all(&m, &mut scene);
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn tick(mut m: ResMut<StruggleMenu>, mut scene: ResMut<MenuScene>, real: Res<Time<Real>>, mut mission: ResMut<Mission>, mut task: ResMut<JumpTask>, rec: Res<AreaRecord>, last: Res<LastExit>, disc: Res<Disc>) {
    // A row chosen while a jump was running waits for the jump to finish (the choice is not dropped).
    if let Some(code) = m.pending
        && *task == JumpTask::Idle
    {
        m.pending = None;
        info!("struggle menu: the queued code {code} runs now");
        apply_code(code, &mut mission, &mut task, &rec, &last, &disc);
    }
    if !m.is_open() {
        return;
    }
    let ticks = scene.advance(real.delta_secs());
    m.frames += ticks;
    if m.flow.stage != Stage::Closing {
        return;
    }
    scene.dim = dim_alpha(close_brightness(m.frames));
    if m.frames < CLOSE_FRAMES {
        return;
    }
    m.flow.stage = Stage::Closed;
    scene.hidden = true;
    scene.dim = 0.0;
    hide_all(&m, &mut scene);
    info!("struggle menu: closed: back to the field (FUN_001038a0)");
    let chosen = m.flow.chosen.take();
    match after_close(chosen, *task == JumpTask::Idle) {
        // A cancel (a record without +0x25 bit 0): `FUN_0026fda0` only closes the page for it (page -2 -> `FUN_0023e8b0`),
        // so the original leaves the manager running. With the menu gone nothing could end it here (a soft lock), so the
        // port ends it as a failed open does [inferred]; no jump.
        Closing::EndManager => {
            info!("struggle menu: cancelled: the manager ends (a port choice, see menu_struggle.rs)");
            mission.menu_exit(0);
        }
        // Codes 1, 5, 6, 10, 0x11 and the party handler (FUN_0026fda0) are not on the result menu's row: not ported.
        Closing::NotPorted(code) => info!("struggle menu: code {code} is not ported; the manager stays"),
        Closing::Queue(code) => {
            info!("struggle menu: code {code} waits for the running jump [inferred: the original does not say]");
            m.pending = Some(code);
        }
        Closing::Apply(code) => apply_code(code, &mut mission, &mut task, &rec, &last, &disc),
    }
}

/// What the end of the close does with the chosen code.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Closing {
    EndManager,
    NotPorted(u16),
    Queue(u16),
    Apply(u16),
}

/// `chosen` is the code a row chose (`None`: the cancel); `idle` is whether no jump task is running.
fn after_close(chosen: Option<u16>, idle: bool) -> Closing {
    match chosen {
        None => Closing::EndManager,
        Some(code) if !matches!(code, 3 | 4) => Closing::NotPorted(code),
        Some(code) if !idle => Closing::Queue(code),
        Some(code) => Closing::Apply(code),
    }
}

/// Codes 3 and 4 (`FUN_001b9c58`): the manager ends and the trigger's jump starts. The caller has checked `task` is idle.
fn apply_code(code: u16, mission: &mut Mission, task: &mut JumpTask, rec: &AreaRecord, last: &LastExit, disc: &Disc) {
    let Some(exit) = mission.menu_exit(code) else { return };
    match exit.jump {
        Some(j) => match exit_dest(j, (rec.world, rec.area, rec.entrance), last.0, |w, k| disc_localset(disc, w, k)) {
            Ok((dest, kind)) => {
                info!("jump: type {} key {:#x} -> w{} a{} entrance {} programs {} (struggle menu event {:#x}: FUN_0019d9d8 with 0x40000000; fade kind {kind})", j[0], j[4], dest.world, dest.area, dest.entrance, dest.programs_text(), exit.event);
                *task = JumpTask::FadeOut { dest, kind, t: 0.0 };
            }
            Err(why) => info!("struggle menu: event {:#x}: trigger jump {j:?}: {why}; not taken", exit.event),
        },
        None => info!("struggle menu: event {:#x}: no trigger jump", exit.event),
    }
}

/// The result menu's jump (`FUN_001b9c58` -> `FUN_0019d9d8(record, 0x40000000)`): the trigger's jump record
/// `[type, world, area, entrance, key, flags]` resolved by the same rule as every other trigger jump
/// (`area_jump::resolve_jump`: type 1 the record's area, type 2 the `07localset.bin` area and the key as the
/// map, btl and evt programs, so "Retry" `[2, 2, 0, 0, 0xb6, 1]` reloads the Struggle with programs 0xb6),
/// with the fade kind from the flags (`flags & 0x3fff`).
fn exit_dest(j: [u16; 6], cur: (u8, u8, u8), last: Option<Dest>, localset: impl Fn(u8, u16) -> Option<u8>) -> Result<(Dest, u32), String> {
    let jump = kh2_ard::run::Jump { kind: j[0], world: j[1], area: j[2], entrance: j[3], key: j[4], flags: j[5] };
    resolve_jump(&jump, cur, last, localset).map(|d| (d, u32::from(j[5] & 0x3fff)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(key: u16, opts: &[(u16, u16)], cursor: u8, flag: u8) -> Vec<u8> {
        let mut r = vec![0u8; 0x40];
        r[0..2].copy_from_slice(&key.to_le_bytes());
        r[2] = opts.len() as u8;
        r[3] = cursor;
        for (i, (c, m)) in opts.iter().enumerate() {
            r[4 + 4 * i..6 + 4 * i].copy_from_slice(&c.to_le_bytes());
            r[6 + 4 * i..8 + 4 * i].copy_from_slice(&m.to_le_bytes());
        }
        r[0x14..0x16].copy_from_slice(&(-1i16).to_le_bytes());
        r[0x16..0x18].copy_from_slice(&13u16.to_le_bytes());
        r[0x25] = flag;
        r
    }

    fn table(rows: &[Vec<u8>]) -> Vec<u8> {
        let mut d = vec![2, 0, 0, 0];
        d.extend((rows.len() as u32).to_le_bytes());
        for r in rows {
            d.extend(r);
        }
        d
    }

    #[test]
    fn retry_takes_its_type_2_jump_with_the_programs_and_quit_its_type_1() {
        let cur = (2, 4, 7);
        // Retry: the record seen in tt04 (type 2, key 0xb6): the area the table gives, the key as all three programs.
        let (d, kind) = exit_dest([2, 2, 0, 0, 0xb6, 1], cur, None, |w, k| (w == 2 && k == 0xb6).then_some(4)).unwrap();
        assert_eq!((d.world, d.area, d.entrance, d.programs, kind), (2, 4, 0, [0xb6; 3], 1));
        // Quit: type 1 to tt04 entrance 55, no program bytes.
        let (d, _) = exit_dest([1, 2, 4, 55, 0, 0], cur, None, |_, _| None).unwrap();
        assert_eq!(d, Dest { world: 2, area: 4, entrance: 55, programs: [0xff; 3] });
        // A type not ported is refused, not guessed.
        assert!(exit_dest([5, 0, 0, 0, 0, 0], cur, None, |_, _| None).is_err());
    }

    #[test]
    fn the_table_rows_parse_and_the_search_finds_by_key() {
        let d = table(&[row(1, &[(2, 0x8ab0), (1, 0x8aaf)], 0, 1), row(0x1a, &[(4, 0x8ab2), (3, 0x8ab1)], 0, 1), row(0x1b, &[(0, 0), (3, 1)], 0, 0)]);
        let t = parse_table(&d);
        assert_eq!(t.len(), 3);
        let b = find(&t, 0x1a).unwrap();
        assert_eq!(b.rows(), vec![(4, 0x8ab2), (3, 0x8ab1)]);
        assert!(b.no_cancel);
        assert_eq!((b.layer_group, b.header_group, b.info), (-1, 13, 0));
        assert_eq!(find(&t, 0x1b).unwrap().rows(), vec![(3, 1)], "code 0 gives no row");
        assert!(find(&t, 0x19).is_none());
    }

    #[test]
    fn the_result_list_moves_wraps_refuses_the_cancel_and_takes_a_row() {
        let t = parse_table(&table(&[row(0x1a, &[(4, 0x8ab2), (3, 0x8ab1)], 0, 1)]));
        let mut f = Flow::open(&t[0]);
        assert_eq!((f.stage, f.row), (Stage::List, 0));
        assert_eq!(f.press(Press::Cancel), Action::None, "+0x25 bit 0: no cancel");
        assert_eq!(f.press(Press::Up), Action::Cursor);
        assert_eq!(f.row, 1);
        assert_eq!(f.press(Press::Down), Action::Cursor);
        assert_eq!(f.row, 0);
        assert_eq!(f.press(Press::Confirm), Action::Chosen(4));
        assert_eq!((f.stage, f.chosen), (Stage::Closing, Some(4)));
        assert_eq!(f.press(Press::Confirm), Action::None, "closing takes no press");
        // Without the flag the cancel closes the page.
        let t = parse_table(&table(&[row(9, &[(10, 1)], 0, 0)]));
        let mut f = Flow::open(&t[0]);
        assert_eq!(f.press(Press::Up), Action::None, "one row does not move");
        assert_eq!(f.press(Press::Cancel), Action::Cancelled);
        assert_eq!(f.chosen, None);
    }

    /// Row 0x1a of the disc's table, its texts and the pointer list's groups (skipped without `KH2_ISO`).
    /// Review findings 3 and 4: a cancel ends the manager, and a code chosen while a jump runs is queued, not dropped.
    #[test]
    fn a_cancel_ends_the_manager_and_a_choice_during_a_jump_is_queued() {
        let t = parse_table(&table(&[row(9, &[(3, 1), (4, 2)], 0, 0)]));
        let mut f = Flow::open(&t[0]);
        assert_eq!(f.press(Press::Cancel), Action::Cancelled);
        assert_eq!((f.stage, f.chosen), (Stage::Closing, None));
        assert_eq!(after_close(f.chosen, true), Closing::EndManager);
        assert_eq!(after_close(Some(3), true), Closing::Apply(3));
        assert_eq!(after_close(Some(4), false), Closing::Queue(4));
        assert_eq!(after_close(Some(5), true), Closing::NotPorted(5));
    }

    #[test]
    fn the_disc_result_menu_row_texts_and_groups() {
        let Ok(iso) = std::env::var("KH2_ISO") else {
            eprintln!("skipped: no disc (set KH2_ISO)");
            return;
        };
        let Ok(mut a) = kh2_archive::Archive::open(&iso) else { return };
        let bar = a.read(FILE).expect("14mission.bar");
        let entries = kh2_bar::parse(&bar).unwrap();
        let e = entries.iter().find(|e| e.tag == TAG).expect("the slct entry");
        let t = parse_table(e.data);
        assert!(t.windows(2).all(|w| w[0].key < w[1].key), "sorted by key for the binary search");
        let b = find(&t, crate::struggle::RESULT_MENU_RECORD).expect("row 0x1a");
        let rows = b.rows();
        assert_eq!(rows.iter().map(|r| r.0).collect::<Vec<_>>(), vec![4, 3], "codes 4 and 3: events 0x8d and 0x8b");
        assert!(b.no_cancel);
        assert_eq!(b.cursor, 0);
        let sys = kh2_image::text::Messages::from_bar(&a.read("msg/us/sys.bar").unwrap()).unwrap();
        for (code, msg) in &rows {
            let text = sys.text(u32::from(msg & 0x7fff)).unwrap_or_default();
            eprintln!("row code {code}: message {msg:#x}: {text:?}");
            assert!(!text.trim().is_empty(), "message {msg:#x}");
        }
        let two = layd::parse_2ld(&a.read("menu/us/save.2ld").unwrap()).unwrap();
        let s0 = &two.layd.seqs[0];
        for grp in [g::ROW_SELECTED, g::ROW_IDLE, g::HAND] {
            assert!(s0.groups.len() > grp && s0.groups[grp].count > 0, "group {grp}");
        }
    }
}
