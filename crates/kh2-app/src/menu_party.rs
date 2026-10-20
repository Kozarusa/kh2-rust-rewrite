//! The pause menu's Party page, WP-MN7 (docs/menus.md 2.5 and section 19).
//!
//! What the original does (`SLUS_210.05`): the camp's Party row opens page 0x14 (`FUN_0026dbd8`), a **horizontal**
//! list of the party's members (`FUN_0026d378` gives both lists the previous / next masks 0x40 and 0x80, Left and
//! Right, and lays the rows out with `FUN_00296bd0(list, i, -1, 0)`: x = the row group's x pitch times the index, y
//! 0), one row per entry of the menu party object (`FUN_00282738`: a slot of the party arrangement that has a member
//! and a model), each row a plate with the member's name and a 3D model standing over it (`FUN_0026d8a8`). The leader's
//! row and every slot that is locked (`FUN_001a7268`: slot 0, a slot byte with bit 0x80, a member in the hidden list at
//! 0x1d5e380) is **disabled** (`FUN_00298f78` mask), and the cursor skips disabled rows (`FUN_00296fb8`,
//! `FUN_002978f8`, `FUN_00297af0`). The page's help line is message 0x46b ("Select Party Member."), or 0x3b88 when the
//! party cannot be switched (`FUN_0026e2a0`); the switch needs at least three members and at least three that are not
//! locked (the tail of `FUN_0026d378` clears the list's confirm, move and cancel bits otherwise).
//!
//! A confirm on row `a` opens page 0x16 (`FUN_0026dec8`): a second list over the same plates, with row `a` and every
//! disabled row disabled, its cursor on the next row (or the previous one at the end or when the next is disabled), the
//! help message 0x46c ("Switch Party Member."). A confirm on row `b` there swaps the two slot bytes of the party
//! arrangement (`FUN_0026eeb0`: `FUN_001a75e8(party, a, b)`, the menu party object rebuilt by `FUN_00282908`, the names
//! of the two plates exchanged, the two 3D models swapped by `FUN_0023b6d0`) and ends the page; a cancel ends it
//! without the swap. Both go back to page 0x14 (`FUN_0026df80`). A cancel on page 0x14 returns to the camp's list.
//!
//! The port keeps the arrangement as the order of the field's `PartyMember` entities (their `slot`) and the slot bytes
//! of `PartySlots`; a swap exchanges both [inferred: the original swaps the bytes and re-reads the party when the
//! camp ends, `FUN_0026ef90`]. Page 0x15 (`FUN_0026e158`, the "Party In / Party Out" notice an event opens through menu
//! kind 5, `FUN_0024f300`) is not ported: nothing in the app opens the camp that way [GAP].

use bevy::prelude::*;
use kh2_image::text::Style;

use crate::assets::Disc;
use crate::audio::PlaySe;
use crate::menu::{Menu, Phase};
use crate::menu_gfx::{MenuScene, Node};
use crate::menu_pages::Exe;
use crate::party::{PartyHidden, PartyMember, PartySlots};
use crate::progress::SORA;

/// Camp page ids of the Party screen (`FUN_00251848` jump table, docs/menus.md 1.4).
pub const PAGE_PICK: u16 = 0x14;
pub const PAGE_SWITCH: u16 = 0x16;
pub const PAGES: [u16; 2] = [PAGE_PICK, PAGE_SWITCH];

/// `camp.2ld` sequence 1 holds every group the page draws.
const SEQ_MAIN: usize = 1;
/// The most rows the list has (the four party slots).
const MAX_ROWS: usize = 4;

/// The groups the page builds with literal ids (`FUN_0026d378`, `FUN_0026e2f8`, `FUN_0026ed88`, `FUN_00253f98`): the
/// frame layer (enter, hold, leave: `FUN_0029b600(layer, 0x753d, .., 0x138, 0x139, 0)` and `FUN_0029b808(layer, 0x13a)`),
/// the column plate of a row (`(slot == 3) + 0x13b`), the marker `FUN_0026ed88` puts under a hand and the hand's groups
/// (`FUN_00253f98` answers 0x19 and 0x17 for a page that is not in the table at 0x3807d8).
mod group {
    pub const FRAME: [usize; 3] = [0x138, 0x139, 0x13a];
    pub const COLUMN: [usize; 2] = [0x13b, 0x13c];
    pub const MARKER: usize = 0x13d;
    pub const HAND: usize = 0x19;
    pub const HAND_WAITING: usize = 0x17;
}

/// Where things sit, in menu units. The columns, the plates and the marker are children of the frame layer and sit at
/// `FUN_00296bd0(list, i, -1, 0)`: the row group's x pitch (`unk20`, 118) times the row, y 0, from the layer's anchor.
/// The text and the hand are [inferred] like the other pages' (the plate object's text field `+0x18c` is drawn by code that
/// was not read).
mod at {
    /// The name from the plate group's anchor: centred on it, the top a little under the plate's top edge.
    pub const NAME: [f32; 2] = [0.0, 20.0];
    pub const FONT: usize = 1;
    pub const SIZE: f32 = 15.0;
    pub const IDLE: [u8; 4] = [0x9f, 0x9f, 0xaf, 0x80];
    pub const SELECTED: [u8; 4] = [0xe5, 0xe5, 0xe5, 0x80];
    /// The hand from the row's anchor, as the camp's other lists (`menu::at::HAND`).
    pub const HAND: [f32; 2] = [-13.0, 0.0];
    /// The help line, as the camp's (`menu::at::HELP`).
    pub const HELP: [f32; 2] = [137.0, 337.0];
}

/// The executable's words the page reads (`FUN_0026d378`, `FUN_0026e2a0`): the plate groups of a free row (0x381adc) and
/// of the leader or a locked row (0x381ae0), the help messages of page 0x14 (0x381ae4) and of the page that cannot switch
/// (0x381ae8), and the names of the members (the table at 0x37ba90, as the Status tabs read it).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Tables {
    pub plate_free: usize,
    pub plate_locked: usize,
    pub help_pick: u32,
    pub help_blocked: u32,
    pub member_names: Vec<u32>,
}

/// The message of page 0x16's help line (`FUN_0026e2a0`: the literal 0x846c; bit 15 of a message id is ignored).
pub const HELP_SWITCH: u32 = 0x46c;

impl Tables {
    pub fn read(exe: &Exe) -> Option<Tables> {
        let w = |a: u32| Some(u32::from(exe.u16(a)?) | u32::from(exe.u16(a + 2)?) << 16);
        Some(Tables {
            plate_free: w(0x0038_1adc)? as usize,
            plate_locked: w(0x0038_1ae0)? as usize,
            help_pick: w(0x0038_1ae4)? & 0x7fff,
            help_blocked: w(0x0038_1ae8)? & 0x7fff,
            member_names: (0..16).filter_map(|i| exe.u16(0x0037_ba90 + 2 * i).map(|v| u32::from(v & 0x7fff))).collect(),
        })
    }
}

/// Where the 3D models stand while the page shows: the origin of each row's model on the canvas
/// (`FUN_0026d8a8`: the models are attached to the plates, `FUN_0023b4b8`). Empty on every other page.
#[derive(Resource, Default, Debug)]
pub struct Stage {
    pub origins: Vec<Vec2>,
}

/// One member standing in the field after the leader, with the live numbers the cards and the Status tabs show.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rostered {
    /// The party member id (`PartyMember::neo`, the NeoStatus: 2 Donald, 3 Goofy).
    pub id: u16,
    /// The party slot of the arrangement: 1 or 2 (slot 0 is the leader).
    pub slot: usize,
    pub hp: i32,
    pub hp_max: i32,
    pub mp: i32,
    pub mp_max: i32,
}

/// The members standing in the field after the leader, in party-slot order (`PartyMember::slot` 0 or 1 is party slot 1
/// or 2, `party.rs`).
pub fn roster<'a>(members: impl Iterator<Item = &'a PartyMember>) -> Vec<Rostered> {
    let mut v: Vec<Rostered> =
        members.map(|m| Rostered { id: m.neo, slot: usize::from(m.slot) + 1, hp: m.hp, hp_max: m.hp_max, mp: m.mp, mp_max: m.mp_max }).collect();
    v.sort_by_key(|r| r.slot);
    v
}

/// A row of the Party list (an entry of `FUN_00282738`): the member and whether its slot is locked.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Row {
    pub id: u16,
    pub slot: usize,
    pub locked: bool,
}

/// The rows: the leader (slot 0, always locked) and the members in slot order. A companion is locked when its slot
/// byte has bit 0x80, or when the byte's low five bits are in the hidden-member list (`FUN_001a7268`: slot 0, `b & 0x80`,
/// then `FUN_001c7be8(b & 0x1f)`, which compares the number with the list's entries as it stands; WP-PY5).
pub fn rows(roster: &[Rostered], bytes: [u8; 4], hidden: &PartyHidden) -> Vec<Row> {
    let mut v = vec![Row { id: SORA, slot: 0, locked: true }];
    v.extend(roster.iter().map(|r| Row { id: r.id, slot: r.slot, locked: bytes.get(r.slot).is_some_and(|b| b & 0x80 != 0 || hidden.contains(u32::from(b & 0x1f))) }));
    v
}

/// The list's disabled-row mask (`FUN_00298f78`): a bit per locked row.
pub fn disabled_mask(rows: &[Row]) -> u32 {
    rows.iter().enumerate().filter(|(_, r)| r.locked).fold(0, |m, (i, _)| m | 1 << i)
}

/// Whether the list lets the player move and confirm (the tail of `FUN_0026d378`): at least three rows, and at least
/// three that are not among the locked companions (`rows - locked >= 3`, the leader counting as unlocked there).
pub fn can_switch(rows: &[Row]) -> bool {
    let locked_companions = rows.iter().skip(1).filter(|r| r.locked).count();
    rows.len() >= 3 && rows.len() - locked_companions >= 3
}

/// The row the cursor starts on (`FUN_0026d378`): the first row from the second one on that is not locked, the second
/// row when all are free or none is, row 0 for a one-row party.
pub fn first_cursor(rows: &[Row]) -> usize {
    let mut cursor = usize::from(rows.len() >= 2);
    for i in 1..rows.len() {
        if !rows[i].locked && rows[cursor].locked {
            cursor = i;
        }
    }
    cursor
}

/// A step of the cursor over the enabled rows (`FUN_002977a8` with `FUN_002978f8` and `FUN_00297af0`): one row in the
/// direction, wrapping when `wrap` (a new press; a held repeat stops at the ends), skipping rows the mask disables;
/// when no other row is enabled the cursor stays.
pub fn step(cursor: usize, count: usize, dir: i8, disabled: u32, wrap: bool) -> usize {
    if count == 0 || dir == 0 {
        return cursor;
    }
    let mut at = cursor;
    for _ in 0..count {
        let next = if dir > 0 {
            if at + 1 < count {
                at + 1
            } else if wrap {
                0
            } else {
                return cursor;
            }
        } else if at > 0 {
            at - 1
        } else if wrap {
            count - 1
        } else {
            return cursor;
        };
        at = next;
        if disabled >> at & 1 == 0 {
            return at;
        }
    }
    cursor
}

/// The second list's disabled rows and starting cursor (`FUN_0026dec8`): the first list's mask plus the chosen row; the
/// cursor on the next row, or the previous one when there is no next or the next is disabled.
pub fn partner_list(rows: &[Row], chosen: usize) -> (u32, usize) {
    let mask = disabled_mask(rows) | 1 << chosen;
    let next = chosen + 1;
    let start = if next >= rows.len() || mask >> next & 1 != 0 { chosen.saturating_sub(1) } else { next };
    (mask, start)
}

/// `FUN_001a75e8(party, a, b)`: exchanges two slot bytes.
pub fn swap_bytes(bytes: &mut [u8; 4], a: usize, b: usize) {
    if a < 4 && b < 4 {
        bytes.swap(a, b);
    }
}

// --- the screen -------------------------------------------------------------------------------------

/// The nodes the page draws with, created once in the scene. The order is the draw order: the frame, the column
/// plates, the 3D models (a copy of the preview's picture node: the models stand on the columns and behind the name
/// plates), the name plates, the marker and the hands.
struct Nodes {
    frame: usize,
    columns: Vec<usize>,
    preview: usize,
    plates: Vec<usize>,
    marker: usize,
    hands: [usize; 2],
    help: usize,
}

impl Nodes {
    fn all(&self) -> Vec<usize> {
        [self.frame, self.preview, self.marker, self.help].into_iter().chain(self.columns.iter().copied()).chain(self.plates.iter().copied()).chain(self.hands).collect()
    }
}

/// The state of the Party pages.
#[derive(Resource, Default)]
pub struct PartyUi {
    ready: bool,
    tables: Tables,
    nodes: Option<Nodes>,
    /// The page drawn now (0: none).
    page: u16,
    /// Visibility the page-0 nodes had when this screen hid them.
    saved: Vec<(usize, bool)>,
    /// The members after the leader and the slot bytes, as the last frame saw them.
    roster: Vec<Rostered>,
    bytes: [u8; 4],
    /// The list's cursor (a row), the row page 0x14 chose, and the disabled rows of the list that is active.
    pub cursor: usize,
    pub chosen: usize,
    disabled: u32,
    /// What was last drawn (a change redraws).
    drawn: Option<(Vec<Row>, u16, usize, usize)>,
    /// Frames the Left or Right key has been held and which way (the repeat does not wrap).
    held: (i8, u32),
    /// Real seconds since the menu started to close.
    closing: f32,
}

pub struct PartyPlugin;

impl Plugin for PartyPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<PartyUi>()
            .init_resource::<Stage>()
            // Before the menu's own input, which would read these presses as page 0's.
            .add_systems(PreUpdate, input.before(crate::menu::input).after(bevy::input::InputSystems).after(crate::harness::Keys))
            .add_systems(Update, tick.after(crate::menu::tick).before(crate::menu::preview::show).before(crate::schedule::Frame::Popups));
    }
}

/// Key repeat of the lists, as the other pages [inferred].
const REPEAT_DELAY: u32 = 24;
const REPEAT_EVERY: u32 = 6;

#[allow(clippy::too_many_arguments)]
fn input(
    mut menu: ResMut<Menu>,
    mut ui: ResMut<PartyUi>,
    mut keys: ResMut<ButtonInput<KeyCode>>,
    mut se: MessageWriter<PlaySe>,
    mut slots: ResMut<PartySlots>,
    mut members: Query<&mut PartyMember>,
    hidden: Res<PartyHidden>,
) {
    // The page's own state follows the menu's one frame later (`tick`): the first frame of a page only syncs.
    if !matches!(menu.phase, Phase::Opening | Phase::Open) || !PAGES.contains(&menu.page) || ui.page != menu.page {
        ui.held = (0, 0);
        return;
    }
    ui.roster = roster(members.iter());
    ui.bytes = slots.bytes;
    let rows = rows(&ui.roster, ui.bytes, &hidden);
    let confirm = keys.just_pressed(KeyCode::KeyJ);
    let cancel = keys.just_pressed(KeyCode::Escape) || keys.just_pressed(KeyCode::Backspace) || keys.just_pressed(KeyCode::KeyK);
    // The lists' previous and next masks are Left and Right (`FUN_00296d10(list, 0x40, 0x80, 0, 0)`).
    let dir = if keys.pressed(KeyCode::ArrowLeft) { -1i8 } else if keys.pressed(KeyCode::ArrowRight) { 1 } else { 0 };
    let fresh = keys.just_pressed(KeyCode::ArrowLeft) || keys.just_pressed(KeyCode::ArrowRight);
    let mut step_now = None;
    if dir == 0 {
        ui.held = (0, 0);
    } else if fresh || ui.held.0 != dir {
        ui.held = (dir, 0);
        step_now = Some(true);
    } else {
        ui.held.1 += 1;
        if ui.held.1 >= REPEAT_DELAY && (ui.held.1 - REPEAT_DELAY).is_multiple_of(REPEAT_EVERY) {
            step_now = Some(false);
        }
    }
    let switchable = can_switch(&rows);
    // `FUN_0026d378`: a party that cannot switch has no move or confirm in the list; the cancel stays.
    if let Some(wrap) = step_now.filter(|_| switchable) {
        let to = step(ui.cursor, rows.len(), dir, ui.disabled, wrap);
        if to != ui.cursor {
            ui.cursor = to;
            crate::menu::menu_sound::play(&mut se, crate::menu_status::CURSOR);
            info!("menu: party cursor {to} (page {:#x})", menu.page);
        }
    }
    match menu.page {
        PAGE_PICK => {
            if confirm && switchable && rows.get(ui.cursor).is_some_and(|r| !r.locked) {
                // `FUN_0026dec8`: the second list starts beside the chosen row, the chosen row disabled.
                let chosen = ui.cursor;
                let (mask, start) = partner_list(&rows, chosen);
                ui.chosen = chosen;
                ui.disabled = mask;
                ui.cursor = start;
                menu.page = PAGE_SWITCH;
                crate::menu::menu_sound::play(&mut se, crate::menu_status::CONFIRM);
                info!("menu: party row {chosen} chosen, page {PAGE_SWITCH:#x} cursor {start}");
            } else if cancel {
                menu.page = 0;
                crate::menu::menu_sound::play(&mut se, crate::menu_status::CANCEL);
                info!("menu: page 0");
            }
        }
        _ => {
            let pair = rows.get(ui.chosen).zip(rows.get(ui.cursor)).filter(|_| confirm && switchable && ui.disabled >> ui.cursor & 1 == 0);
            if let Some((&a, &b)) = pair {
                swap_members(&mut slots.bytes, &mut members, a.slot, b.slot);
                info!("menu: party switched {} (slot {}) with {} (slot {}), slot bytes {:02x?}", a.id, a.slot, b.id, b.slot, slots.bytes);
                ui.cursor = ui.chosen;
                ui.disabled = disabled_mask(&rows);
                menu.page = PAGE_PICK;
                crate::menu::menu_sound::play(&mut se, crate::menu_status::CONFIRM);
            } else if cancel {
                ui.cursor = ui.chosen;
                ui.disabled = disabled_mask(&rows);
                menu.page = PAGE_PICK;
                crate::menu::menu_sound::play(&mut se, crate::menu_status::CANCEL);
                info!("menu: page {PAGE_PICK:#x}");
            }
        }
    }
    // The keys this screen took do not reach the menu's own input.
    for k in [KeyCode::KeyJ, KeyCode::Escape, KeyCode::Backspace, KeyCode::KeyK, KeyCode::ArrowLeft, KeyCode::ArrowRight, KeyCode::ArrowUp, KeyCode::ArrowDown] {
        keys.clear_just_pressed(k);
    }
}

/// `FUN_0026eeb0`: the two slots of the arrangement trade places (the bytes, and the members standing in the field,
/// whose `slot` is the follow position) [inferred: the field's objects; the original re-reads the arrangement later].
fn swap_members(bytes: &mut [u8; 4], members: &mut Query<&mut PartyMember>, a: usize, b: usize) {
    swap_bytes(bytes, a, b);
    for mut m in members.iter_mut() {
        m.slot = (traded_slot(usize::from(m.slot) + 1, a, b) - 1) as u8;
    }
}

/// The party slot a member standing in `slot` has after slots `a` and `b` trade places.
pub fn traded_slot(slot: usize, a: usize, b: usize) -> usize {
    if slot == a {
        b
    } else if slot == b {
        a
    } else {
        slot
    }
}

/// Where a row's model stands (`FUN_0026d8a8`, `FUN_0023aca8`): the frame layer's anchor, the plate group's anchor and the
/// row's offset along the layer (`pitch * row`), the origin of the model a little under the anchor (`drop`).
pub fn model_origin(frame: [f32; 2], plate: [f32; 2], pitch: f32, row: usize, drop: f32) -> Vec2 {
    Vec2::new(frame[0] + plate[0] + pitch * row as f32, frame[1] + plate[1] + drop)
}

/// Creates the nodes (hidden), once the menu's sequences are loaded. `preview` is the page-0 picture node of the 3D models.
fn build(scene: &mut MenuScene) -> Nodes {
    let new = |scene: &mut MenuScene, seq: Option<usize>| {
        let mut n = Node::new(seq);
        n.visible = false;
        scene.add(n)
    };
    let frame = new(scene, Some(SEQ_MAIN));
    let columns: Vec<usize> = (0..MAX_ROWS).map(|_| new(scene, Some(SEQ_MAIN))).collect();
    let preview = new(scene, None);
    let plates: Vec<usize> = (0..MAX_ROWS).map(|_| new(scene, Some(SEQ_MAIN))).collect();
    let marker = new(scene, Some(SEQ_MAIN));
    let hands = [new(scene, Some(SEQ_MAIN)), new(scene, Some(SEQ_MAIN))];
    let help = new(scene, None);
    for &c in columns.iter().chain(&plates).chain(&[marker]) {
        scene.nodes[c].parent = Some(frame);
    }
    Nodes { frame, columns, preview, plates, marker, hands, help }
}

/// Reads what the page needs from the executable, once.
fn load(ui: &mut PartyUi, disc: &Disc) {
    ui.ready = true;
    match disc.read_root("SLUS_210.05").and_then(Exe::load).and_then(|e| Tables::read(&e)) {
        Some(t) => ui.tables = t,
        None => warn!("menu: party: the executable's tables are not readable; the Party page draws nothing"),
    }
}

/// One frame of the Party pages: follow the page the menu is on, draw the columns, the names, the hands and the models.
#[allow(clippy::too_many_arguments)]
fn tick(
    menu: Res<Menu>,
    mut ui: ResMut<PartyUi>,
    mut stage: ResMut<Stage>,
    mut scene: ResMut<MenuScene>,
    disc: Res<Disc>,
    slots: Res<PartySlots>,
    members: Query<&PartyMember>,
    sora: Query<&crate::sora::Sora>,
    progress: Option<Res<crate::progress::Progress>>,
    real: Res<Time<Real>>,
    hidden: Res<PartyHidden>,
) {
    let want = if matches!(menu.phase, Phase::Opening | Phase::Open | Phase::Closing) && PAGES.contains(&menu.page) { menu.page } else { 0 };
    if menu.phase == Phase::Closed {
        // The menu hid every node itself; forget what this screen drew.
        ui.page = 0;
        ui.saved.clear();
        ui.drawn = None;
        ui.closing = 0.0;
        stage.origins.clear();
        return;
    }
    if want == 0 && ui.page == 0 {
        return;
    }
    if !ui.ready {
        load(&mut ui, &disc);
    }
    if scene.seqs.get(SEQ_MAIN).is_none() {
        return;
    }
    if ui.nodes.is_none() {
        ui.nodes = Some(build(&mut scene));
    }
    ui.roster = roster(members.iter());
    ui.bytes = slots.bytes;
    let rows = rows(&ui.roster, ui.bytes, &hidden);
    if want != ui.page {
        let nodes = ui.nodes.take().expect("built");
        if ui.page == 0 {
            // Entering from page 0: hide what page 0 draws and its 3D models' picture (the page has its own).
            let ids = menu.page0_nodes();
            ui.saved = ids.iter().map(|&i| (i, scene.nodes[i].visible)).collect();
            for i in ids {
                scene.nodes[i].visible = false;
            }
            ui.cursor = first_cursor(&rows);
            ui.disabled = disabled_mask(&rows);
            // The frame plays its enter group, then its hold group (`FUN_0029b600(layer, 0x753d, .., 0x138, 0x139, 0)`).
            scene.set_group_then(nodes.frame, group::FRAME[0], group::FRAME[1]);
            scene.nodes[nodes.frame].visible = true;
            info!("menu: party page {want:#x}: {} rows, cursor {}, disabled {:#06b}, switchable {}", rows.len(), ui.cursor, ui.disabled, can_switch(&rows));
        } else if want == 0 {
            // Back on page 0: hide this screen and give page 0 its nodes back.
            for id in nodes.all() {
                scene.nodes[id].visible = false;
            }
            for (i, v) in std::mem::take(&mut ui.saved) {
                scene.nodes[i].visible = v;
            }
            stage.origins.clear();
            // The cards of page 0 follow the order the members now stand in (`FUN_00250a00` builds them from the party object,
            // which `FUN_00282908` rebuilt after the swap).
            let field: Vec<(u16, i32, i32, i32, i32)> = ui.roster.iter().map(|r| (r.id, r.hp, r.hp_max, r.mp, r.mp_max)).collect();
            let cards = crate::menu::party(sora.single().ok(), &field, progress.as_deref(), &ui.tables.member_names);
            crate::menu::show_cards(&menu, &mut scene, &cards);
        }
        ui.page = want;
        ui.drawn = None;
        ui.nodes = Some(nodes);
    }
    if ui.page == 0 {
        return;
    }
    let nodes = ui.nodes.take().expect("built");
    if menu.phase == Phase::Closing {
        ui.closing += real.delta_secs();
        let fade = 1.0 - (ui.closing * 60.0 / 8.0).min(1.0);
        for &id in nodes.plates.iter().chain([&nodes.help, &nodes.preview]) {
            scene.nodes[id].label_fade = fade;
        }
    }
    // The 3D models' picture: the preview's own node is hidden on this page; this one stands in the page's draw order.
    if let Some(pic) = scene.nodes[menu.nodes.preview].pic {
        scene.nodes[nodes.preview].pic = Some(pic);
        scene.nodes[nodes.preview].visible = !stage.origins.is_empty();
        scene.nodes[menu.nodes.preview].visible = false;
    }
    if ui.drawn.as_ref() != Some(&(rows.clone(), ui.page, ui.cursor, ui.chosen)) {
        draw(&ui, &nodes, &mut scene, &rows, &mut stage);
        ui.drawn = Some((rows, ui.page, ui.cursor, ui.chosen));
    }
    ui.nodes = Some(nodes);
}

/// Lays out the current page: the column of each row, its name plate, the marker and the hand under the cursor (and, on
/// page 0x16, the waiting hand on the chosen row), the help line, and the places of the models.
fn draw(ui: &PartyUi, n: &Nodes, scene: &mut MenuScene, rows: &[Row], stage: &mut Stage) {
    let pitch = scene.seqs.get(SEQ_MAIN).and_then(|s| s.groups.get(group::COLUMN[0])).map_or(118.0, |g| g.unk20 as f32);
    let anchor_of = |scene: &MenuScene, g: usize| scene.seqs[SEQ_MAIN].anchor(g, 0, &Default::default()).map_or([0.0; 2], |a| [a[0] as f32, a[1] as f32]);
    let frame = anchor_of(scene, group::FRAME[0]);
    let plate_anchor = [anchor_of(scene, ui.tables.plate_free), anchor_of(scene, ui.tables.plate_locked)];
    let drop = crate::menu::preview::model_drop();
    stage.origins.clear();
    for k in 0..MAX_ROWS {
        let (c, p) = (n.columns[k], n.plates[k]);
        let Some(row) = rows.get(k) else {
            scene.nodes[c].visible = false;
            scene.nodes[p].visible = false;
            continue;
        };
        // `FUN_0026d378`: `(slot == 3) + 0x13b` for the column, the leader's and a locked row's plate in the other colour.
        let col_group = group::COLUMN[usize::from(k == 3)];
        let plate_group = if row.locked { ui.tables.plate_locked } else { ui.tables.plate_free };
        for (id, g) in [(c, col_group), (p, plate_group)] {
            scene.nodes[id].visible = true;
            scene.nodes[id].off = [pitch * k as f32, 0.0];
            if scene.nodes[id].player.group != Some(g) {
                scene.set_group(id, g);
            }
        }
        let selected = k == ui.cursor;
        let name = ui.tables.member_names.get(usize::from(row.id)).copied();
        let label = scene.text.as_ref().and_then(|t| {
            t.message(name?, Style { font: at::FONT, size: Some(at::SIZE), color: if selected { at::SELECTED } else { at::IDLE }, align: 2, ..Style::default() }, [0.0, 0.0])
        });
        scene.nodes[p].label = label;
        scene.nodes[p].label_anchor = Some(at::NAME);
        scene.nodes[p].label_fade = 1.0;
        // The model stands at the plate's anchor, a little lower (`FUN_0023b4b8`, the draw of `FUN_0023aca8`).
        let a = plate_anchor[usize::from(row.locked)];
        stage.origins.push(model_origin(frame, a, pitch, k, drop));
    }
    // The marker pulses on the row the active list's hand is on (`FUN_0026ed88`); the hands: the active one on the cursor,
    // the first list's on the chosen row while page 0x16 runs (`FUN_00251d38`).
    scene.nodes[n.marker].visible = true;
    scene.nodes[n.marker].off = [pitch * ui.cursor as f32, 0.0];
    if scene.nodes[n.marker].player.group != Some(group::MARKER) {
        scene.set_group(n.marker, group::MARKER);
    }
    let waiting = ui.page == PAGE_SWITCH;
    for (k, (row, g)) in [(ui.chosen, group::HAND_WAITING), (ui.cursor, group::HAND)].into_iter().enumerate() {
        let h = n.hands[k];
        scene.nodes[h].visible = k == 1 || waiting;
        scene.nodes[h].parent = Some(n.columns[row.min(MAX_ROWS - 1)]);
        scene.nodes[h].off = at::HAND;
        if scene.nodes[h].player.group != Some(g) {
            scene.set_group(h, g);
        }
    }
    // The help line (`FUN_0026e2a0`): page 0x14 asks to select a member, or says why it cannot, page 0x16 to pick the partner.
    let help = match (ui.page, can_switch(rows)) {
        (PAGE_PICK, true) => ui.tables.help_pick,
        (PAGE_PICK, false) => ui.tables.help_blocked,
        _ => HELP_SWITCH,
    };
    if let Some(text) = scene.text.as_ref() {
        let l = text.message(help, Style { font: 0, size: Some(15.0), color: [0x60, 0xf7, 0xff, 0x80], ..Style::default() }, at::HELP);
        scene.nodes[n.help].label = l;
        scene.nodes[n.help].visible = true;
        scene.nodes[n.help].label_fade = 1.0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn member(id: u16, slot: usize) -> Rostered {
        Rostered { id, slot, hp: 10, hp_max: 20, mp: 5, mp_max: 100 }
    }

    fn rows(r: &[Rostered], b: [u8; 4]) -> Vec<Row> {
        super::rows(r, b, &PartyHidden::default())
    }

    #[test]
    fn a_member_in_the_hidden_list_is_locked() {
        // Slot bytes 1 and 2 name list entries 1 and 2 (`FUN_001c7be8(b & 0x1f)`); Donald's NeoStatus 2 is entry 1.
        let mut h = PartyHidden::default();
        h.push(2);
        let r = super::rows(&[member(2, 1), member(3, 2)], [0, 1, 2, 0x10], &h);
        assert_eq!(r.iter().map(|r| r.locked).collect::<Vec<_>>(), vec![true, true, false]);
        let none = super::rows(&[member(2, 1), member(3, 2)], [0, 1, 2, 0x10], &PartyHidden::default());
        assert_eq!(none.iter().map(|r| r.locked).collect::<Vec<_>>(), vec![true, false, false]);
    }

    fn party3() -> Vec<Row> {
        rows(&[member(2, 1), member(3, 2)], [0, 1, 2, 0x10])
    }

    #[test]
    fn rows_are_the_leader_then_the_members_and_the_leader_is_locked() {
        let r = party3();
        assert_eq!(r.iter().map(|r| r.id).collect::<Vec<_>>(), vec![SORA, 2, 3]);
        assert_eq!(r.iter().map(|r| r.locked).collect::<Vec<_>>(), vec![true, false, false]);
        assert_eq!(disabled_mask(&r), 0b001);
        // A slot byte with bit 0x80 locks its member (`FUN_001a7268`).
        let locked = rows(&[member(2, 1), member(3, 2)], [0, 0x81, 2, 0x10]);
        assert_eq!(disabled_mask(&locked), 0b011);
    }

    #[test]
    fn the_party_switches_only_with_three_rows_and_three_unlocked() {
        assert!(can_switch(&party3()));
        assert!(!can_switch(&rows(&[member(2, 1)], [0, 1, 0x10, 0x10])), "two rows");
        assert!(!can_switch(&rows(&[], [0; 4])), "the leader alone");
        // Three rows with one locked companion: rows - locked = 2 < 3.
        assert!(!can_switch(&rows(&[member(2, 1), member(3, 2)], [0, 0x81, 2, 0x10])));
        // Four rows with one locked: 3 left, allowed.
        assert!(can_switch(&rows(&[member(2, 1), member(3, 2), member(5, 3)], [0, 0x81, 2, 5])));
    }

    #[test]
    fn the_cursor_starts_on_the_first_free_row_after_the_leader() {
        assert_eq!(first_cursor(&party3()), 1);
        assert_eq!(first_cursor(&rows(&[member(2, 1), member(3, 2)], [0, 0x81, 2, 0x10])), 2, "Donald is locked");
        assert_eq!(first_cursor(&rows(&[], [0; 4])), 0, "one row");
        assert_eq!(first_cursor(&rows(&[member(2, 1)], [0, 0x81, 0x10, 0x10])), 1, "every companion locked: the second row stays");
    }

    #[test]
    fn the_cursor_skips_disabled_rows_and_wraps_on_a_new_press_only() {
        let d = 0b0001; // the leader
        assert_eq!(step(1, 3, 1, d, true), 2);
        assert_eq!(step(2, 3, 1, d, true), 1, "wraps past the leader to the next enabled row");
        assert_eq!(step(2, 3, 1, d, false), 2, "a held repeat stops at the end");
        assert_eq!(step(1, 3, -1, d, true), 2, "wraps from the first enabled row to the last");
        assert_eq!(step(1, 3, -1, d, false), 1);
        assert_eq!(step(1, 3, 0, d, true), 1);
        // Only one enabled row: the cursor stays on it.
        assert_eq!(step(1, 3, 1, 0b101, true), 1);
        assert_eq!(step(0, 0, 1, 0, true), 0);
    }

    #[test]
    fn the_partner_list_disables_the_chosen_row_and_starts_beside_it() {
        let r = party3();
        // Donald chosen: Goofy (the next row) is where the cursor starts; the leader and Donald are disabled.
        assert_eq!(partner_list(&r, 1), (0b011, 2));
        // Goofy chosen: there is no next row, the previous one is Donald.
        assert_eq!(partner_list(&r, 2), (0b101, 1));
        // The next row disabled: back to the previous one.
        let locked = rows(&[member(2, 1), member(3, 2)], [0, 1, 0x82, 0x10]);
        assert_eq!(partner_list(&locked, 1), (0b111, 0));
    }

    fn disc() -> Option<kh2_archive::Archive> {
        let iso = std::env::var("KH2_ISO").ok().or_else(|| {
            let f = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../local/iso.txt");
            std::fs::read_to_string(f).ok().map(|s| s.trim().to_string())
        });
        let Some(iso) = iso else {
            eprintln!("skipped: no disc (set KH2_ISO)");
            return None;
        };
        kh2_archive::Archive::open(&iso).ok()
    }

    /// The groups the page draws, read from `camp.2ld` (skipped without `KH2_ISO`): the frame is the 492 x 254 panel with
    /// an enter, a hold and a leave group, the columns and plates repeat every 118 units, the leader's plate and the fourth
    /// column have colours of their own, the marker pulses (docs/menus.md 19.2). Printed with their extents.
    #[test]
    fn the_party_groups_exist_in_camp() {
        let Some(mut a) = disc() else { return };
        let bar = a.read("menu/us/camp.2ld").expect("camp.2ld");
        let two = kh2_image::layd::parse_2ld(&bar).expect("a .2ld");
        let seq = &two.layd.seqs[1];
        let bbox = |g: usize, f: i32| {
            let q = seq.quads(g, f, [0.0, 0.0], &Default::default());
            let (mut x0, mut y0, mut x1, mut y1) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
            for p in q.iter().flat_map(|q| q.pos) {
                x0 = x0.min(p[0]);
                x1 = x1.max(p[0]);
                y0 = y0.min(p[1]);
                y1 = y1.max(p[1]);
            }
            ([x0, y0, x1, y1], q.len(), q.first().map(|q| q.color[0]))
        };
        for g in [0x138usize, 0x139, 0x13a, 0x13b, 0x13c, 0x13d, 0x13e, 0x13f, 0x19, 0x17] {
            let gr = &seq.groups[g];
            println!("group {g:#x}: loops {} len {} xy ({}, {}) pitch {} {} anchor {:?} bbox {:?}", gr.loops, seq.group_len(g), gr.x, gr.y, gr.unk1c, gr.unk20, seq.anchor(g, 0, &Default::default()), bbox(g, 0));
        }
        let (frame, _, _) = bbox(group::FRAME[0], seq.last_frame(group::FRAME[0]));
        assert_eq!(frame, [12.0, 72.0, 504.0, 326.0]);
        assert_eq!(seq.anchor(group::FRAME[0], 0, &Default::default()), Some([24, 83]));
        // The three frame groups: a 16-frame enter that plays once, a looping hold, a leave that plays once.
        assert_eq!([seq.group_len(group::FRAME[0]), seq.group_len(group::FRAME[1]), seq.group_len(group::FRAME[2])], [16, 8, 8]);
        assert_eq!([seq.groups[group::FRAME[0]].loops, seq.groups[group::FRAME[1]].loops, seq.groups[group::FRAME[2]].loops], [1, 0, 1]);
        // Rows and plates are laid out by the group's x pitch `unk20`: 118 for the column, the marker and both plates.
        for g in [0x13b, 0x13c, 0x13d, 0x13e, 0x13f] {
            assert_eq!(seq.groups[g].unk20, 118, "group {g:#x}");
        }
        // The column is a 116 x 210 plate, the plate a strip under it; the leader's plate and the fourth column differ in colour.
        assert_eq!(bbox(0x13b, 0).0, [-5.0, -4.0, 111.0, 206.0]);
        assert_eq!(bbox(0x13e, 0).0[1], 189.0);
        assert_ne!(bbox(0x13e, 0).2, bbox(0x13f, 0).2);
        assert_ne!(bbox(0x13b, 0).2, bbox(0x13c, 0).2);
        assert_eq!(seq.anchor(0x13e, 0, &Default::default()), Some([51, 189]));
        // The marker's colour changes over its loop (the pulse).
        assert_ne!(bbox(0x13d, 0).2, bbox(0x13d, 30).2);
        // The first model stands under the plate's anchor on the first column: x 24 + 51, y 83 + 189 + the projection's drop.
        let o = model_origin([24.0, 83.0], [51.0, 189.0], 118.0, 0, crate::menu::preview::model_drop());
        assert_eq!(o.x, 75.0);
        assert!((o.y - 281.83).abs() < 0.05, "{o:?}");
        assert_eq!(model_origin([24.0, 83.0], [51.0, 189.0], 118.0, 2, 0.0).x, 311.0);
    }

    /// The executable's words (skipped without `KH2_ISO`): the plate groups, the two help messages and the member names.
    #[test]
    fn the_executables_tables_name_the_plates_and_the_help_lines() {
        let Some(mut a) = disc() else { return };
        let exe = Exe::load(a.iso().read("SLUS_210.05").or_else(|_| a.iso().read("SLUS_210.05;1")).expect("the executable")).expect("an ELF");
        let t = Tables::read(&exe).expect("tables");
        println!("{t:?}");
        assert_eq!((t.plate_free, t.plate_locked), (0x13e, 0x13f));
        assert_eq!((t.help_pick, t.help_blocked), (0x46b, 0x3b88));
        assert_eq!(t.member_names.len(), 16);
        // Every message the page shows exists in the system table.
        let m = kh2_image::text::Messages::from_bar(&a.read("msg/us/sys.bar").expect("sys.bar")).expect("messages");
        for id in [t.help_pick, t.help_blocked, HELP_SWITCH, t.member_names[2], t.member_names[3], t.member_names[1]] {
            assert!(m.text(id).is_some_and(|s| !s.is_empty()), "message {id:#x}");
        }
    }

    #[test]
    fn two_members_trade_their_party_slots() {
        // Slots 1 and 2 trade: a member in slot 1 goes to 2, in 2 to 1, anything else stays.
        assert_eq!((traded_slot(1, 1, 2), traded_slot(2, 1, 2), traded_slot(3, 1, 2), traded_slot(0, 1, 2)), (2, 1, 3, 0));
        assert_eq!(traded_slot(3, 3, 1), 1);
    }

    #[test]
    fn a_swap_exchanges_two_slot_bytes() {
        let mut b = [0u8, 1, 2, 0x10];
        swap_bytes(&mut b, 1, 2);
        assert_eq!(b, [0, 2, 1, 0x10]);
        swap_bytes(&mut b, 1, 9);
        assert_eq!(b, [0, 2, 1, 0x10], "a slot out of range is ignored");
    }
}
