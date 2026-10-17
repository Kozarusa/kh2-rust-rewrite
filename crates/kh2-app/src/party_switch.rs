//! The camp's notice page 0x15, "Party Members have been switched." (WP-PT1; docs/party.md section 21, docs/menus.md 19.4).
//!
//! What the original does (`SLUS_210.05`): a jump record whose word 4 is 0 or more arranges the destination world's party
//! (`FUN_001a7330`) after keeping the record as it stood in `DAT_01c8bda0`; a word 6 of 1 or 2 then calls
//! `FUN_0024f300(arg, 0x1c8bda0)`, which stores that address (`FUN_0026ef70`) and opens menu kind 5 (`FUN_0024f0b8(5, arg)`).
//! Kind 5's init `FUN_00250450` opens page 0x15 because the stored address is not 0. Its init `FUN_0026e130` builds the
//! notice window (`FUN_0026e618`): the window text 0x846d, the headers 0x846e "Party In" and 0x846f "Party Out" over two
//! comma lists built by `FUN_0026e970` (the members the new party has and the stored one lacks, and the other way, named by
//! `FUN_001b5868`, icon 10 before a locked slot's name), and a last line: message 0xcc69 when `DAT_0036cd30` is 2, else the
//! message of the table at 0x381b08 for the member of the last locked slot 1 to 3. A confirm or cancel leaves (`FUN_0026e8d0`,
//! `FUN_0026e158`).
//!
//! What the port does: `party::jump_party` arranges the bytes and sets [`PartySwitch::request`]; this module builds the
//! notice from the executable's tables and draws it over the frozen field until a confirm, cancel or Start press closes it
//! (the field is held, so the jump waits for the page as the original's task does [inferred]). The window's frame and
//! geometry are not read (the objects of `FUN_0029b6c0` with group ids from 0x381af8 and 0x381afe were not traced): the
//! panel is a plain translucent rectangle with the lines in the order the original builds them [inferred].

use bevy::prelude::*;
use kh2_image::text::{Style, Token};

use crate::assets::Disc;
use crate::menu_gfx::{MenuScene, Node};
use crate::menu_pages::Exe;
use crate::menu_text::Label;
use crate::party_rules::{Notice, NoticeSlot};
use crate::schedule::Pause;

/// The window text (`FUN_0026e618`: `FUN_002532d0(0x846d, 1)`) and the two headers (`FUN_0026e618` loads the groups of the
/// words at 0x381af8..; the messages are the ones the headers' text objects are given in `FUN_0026e970`'s caller). Bit 15 of
/// a message id is ignored.
pub const WINDOW_TEXT: u32 = 0x46d;
pub const PARTY_IN: u32 = 0x46e;
pub const PARTY_OUT: u32 = 0x46f;
/// The last line when `DAT_0036cd30` is 2 (`FUN_00190c40(0xcc69)`).
pub const LINE_MODE_2: u32 = 0xcc69;
/// Executable addresses: the separator format of the lists (`FUN_001918a0(.., 0x381ad8)`), the per-member last-line messages
/// (u16 each, by member id) and the names (`FUN_001b5868`: u16 each at 0x37ba90).
const SEPARATOR: u32 = 0x0038_1ad8;
const REASONS: u32 = 0x0038_1b08;
const NAMES: u32 = 0x0037_ba90;
/// The icon code `FUN_002554c0(10, ..)` writes before a locked slot's name.
const LOCK_ICON: u8 = 10;

/// What the executable gives the page.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Tables {
    /// The list separator: the C string at 0x381ad8, `FUN_001918a0`'s format (three spaces on the disc).
    pub separator: String,
    /// Message ids of the last line by member id, and of the names by member id.
    pub reasons: Vec<u32>,
    pub names: Vec<u32>,
}

impl Tables {
    pub fn read(exe: &Exe) -> Option<Tables> {
        let separator: String = (0..16).map_while(|k| exe.u8(SEPARATOR + k)).take_while(|b| *b != 0).map(char::from).collect();
        let ids = |base: u32| (0..16).filter_map(|i| exe.u16(base + 2 * i).map(|v| u32::from(v & 0x7fff))).collect::<Vec<_>>();
        let t = Tables { separator, reasons: ids(REASONS), names: ids(NAMES) };
        (!t.names.is_empty() && !t.reasons.is_empty()).then_some(t)
    }
}

/// A request for the page: the notice and the camp's argument (`FUN_0024f300` 's first parameter, 0 or 2).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Request {
    pub notice: Notice,
    pub arg: u32,
}

/// The party switch the last jump record made.
#[derive(Resource, Default, Debug)]
pub struct PartySwitch {
    /// Set by `party::jump_party`; the page takes it when no menu is up.
    pub request: Option<Request>,
    open: bool,
    nodes: Vec<usize>,
    tables: Option<Tables>,
    tried: bool,
}

pub struct PartySwitchPlugin;

impl Plugin for PartySwitchPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<PartySwitch>()
            .add_systems(PreUpdate, (input.before(crate::menu::input), hold.after(crate::menu::input)).after(bevy::input::InputSystems).after(crate::harness::Keys))
            .add_systems(Update, tick.before(crate::schedule::Frame::Popups));
    }
}

/// The text of a member list: names joined by the separator, icon 10 before a locked slot's name (`FUN_0026e970`).
pub fn list_tokens(scene: &MenuScene, t: &Tables, members: &[NoticeSlot]) -> Vec<Token> {
    let mut out = Vec::new();
    for (k, m) in members.iter().enumerate() {
        if k != 0 {
            out.extend(scene.text.as_ref().map(|m| m.literal(&t.separator, Style::default(), [0.0; 2]).tokens).unwrap_or_default());
        }
        if m.locked {
            out.push(Token::Icon(LOCK_ICON));
        }
        out.extend(t.names.get(usize::from(m.member)).map(|id| message(scene, *id)).unwrap_or_default());
    }
    out
}

/// The last line (`FUN_0026e618`): message 0xcc69 when `DAT_0036cd30` is 2 (`mode_two`), else the member's message.
pub fn reason_message(t: &Tables, notice: &Notice, mode_two: bool) -> Option<u32> {
    if mode_two {
        return Some(LINE_MODE_2);
    }
    notice.reason.and_then(|m| t.reasons.get(usize::from(m)).copied())
}

fn message(scene: &MenuScene, id: u32) -> Vec<Token> {
    let mut v = scene.text.as_ref().and_then(|t| t.messages.as_ref()).and_then(|m| m.tokens(id)).unwrap_or_default();
    v.retain(|t| *t != Token::End);
    v
}

/// The panel and the lines [inferred], in menu units.
mod at {
    pub const PANEL: [f32; 4] = [96.0, 112.0, 416.0, 336.0];
    pub const CENTER: f32 = 256.0;
    pub const TITLE_Y: f32 = 128.0;
    pub const IN_Y: f32 = 176.0;
    pub const OUT_Y: f32 = 240.0;
    pub const REASON_Y: f32 = 304.0;
    /// A list sits one line under its header.
    pub const DROP: f32 = 24.0;
    pub const SIZE: f32 = 15.0;
}

fn label(scene: &MenuScene, tokens: Vec<Token>, y: f32) -> Option<Label> {
    let t = scene.text.as_ref()?;
    Some(Label { tokens, style: Style { font: 0, size: Some(at::SIZE), color: [0xe5, 0xe5, 0xe5, 0x80], align: 2, world: t.world, ..Style::default() }, offset: [at::CENTER, y] })
}

/// Lays out the page: the lines of a notice with their y.
pub fn lines(scene: &MenuScene, t: &Tables, req: &Request, mode_two: bool) -> Vec<(f32, Vec<Token>)> {
    let mut v = vec![(at::TITLE_Y, message(scene, WINDOW_TEXT)), (at::IN_Y, message(scene, PARTY_IN))];
    v.push((at::IN_Y + at::DROP, list_tokens(scene, t, &req.notice.came_in)));
    v.push((at::OUT_Y, message(scene, PARTY_OUT)));
    v.push((at::OUT_Y + at::DROP, list_tokens(scene, t, &req.notice.went_out)));
    if let Some(id) = reason_message(t, &req.notice, mode_two) {
        v.push((at::REASON_Y, message(scene, id)));
    }
    v
}

fn show(ps: &mut PartySwitch, scene: &mut MenuScene, req: &Request) {
    let Some(t) = ps.tables.clone() else { return };
    let mut node = Node::new(None);
    node.fill = Some((at::PANEL, [[0, 0, 0, 0x60]; 4]));
    let panel = scene.add(node);
    ps.nodes.push(panel);
    for (y, tokens) in lines(scene, &t, req, false) {
        let mut n = Node::new(None);
        n.label = label(scene, tokens, y);
        ps.nodes.push(scene.add(n));
    }
    scene.hidden = false;
    scene.dim = 0.0;
}

fn clear(ps: &mut PartySwitch, scene: &mut MenuScene) {
    for i in ps.nodes.drain(..) {
        scene.nodes[i].visible = false;
        scene.nodes[i].label = None;
        scene.nodes[i].fill = None;
    }
}

/// Opens when a record asked for it; closes on a confirm, cancel or Start.
fn input(
    mut ps: ResMut<PartySwitch>,
    mut scene: ResMut<MenuScene>,
    disc: Res<Disc>,
    mut keys: ResMut<ButtonInput<KeyCode>>,
    menu: Res<crate::menu::Menu>,
) {
    if !ps.open {
        if ps.request.is_none() || menu.phase != crate::menu::Phase::Closed || scene.text.is_none() {
            return;
        }
        if !ps.tried {
            ps.tried = true;
            ps.tables = disc.read_root("SLUS_210.05").and_then(Exe::load).and_then(|e| Tables::read(&e));
            if ps.tables.is_none() {
                warn!("menu: kind 5 page 0x15: the executable's tables are not readable; the notice is off");
            }
        }
        let req = ps.request.take().unwrap();
        if ps.tables.is_none() {
            return;
        }
        show(&mut ps, &mut scene, &req);
        ps.open = true;
        info!(
            "menu: kind 5 page 0x15 open (FUN_0024f300({}, ..), FUN_0026e618): came in {:?}, went out {:?}, last line member {:?}",
            req.arg,
            req.notice.came_in.iter().map(|m| m.member).collect::<Vec<_>>(),
            req.notice.went_out.iter().map(|m| m.member).collect::<Vec<_>>(),
            req.notice.reason
        );
        keys.reset_all();
        return;
    }
    let close = [KeyCode::Enter, KeyCode::NumpadEnter, KeyCode::Escape, KeyCode::Backspace, KeyCode::KeyJ, KeyCode::KeyK].iter().any(|k| keys.just_pressed(*k));
    keys.reset_all();
    if close {
        clear(&mut ps, &mut scene);
        scene.hidden = true;
        ps.open = false;
        info!("menu: kind 5 page 0x15 closed (FUN_0026e8d0): back to the field");
    }
}

fn hold(ps: Res<PartySwitch>, mut pause: ResMut<Pause>, mut keys: ResMut<ButtonInput<KeyCode>>) {
    if ps.open {
        pause.menu = true;
        keys.reset_all();
    }
}

fn tick(ps: Res<PartySwitch>, mut scene: ResMut<MenuScene>, real: Res<Time<Real>>) {
    if ps.open {
        scene.advance(real.delta_secs());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tables() -> Tables {
        Tables { separator: "   ".to_string(), reasons: (0..16).map(|i| 0x100 + i).collect(), names: (0..16).map(|i| 0x200 + i).collect() }
    }

    /// The last line: mode 2 wins; otherwise the table entry of the notice's member; none without one.
    #[test]
    fn the_last_line_follows_the_mode_and_the_locked_member() {
        let t = tables();
        let n = Notice { reason: Some(3), ..Notice::default() };
        assert_eq!(reason_message(&t, &n, true), Some(0xcc69));
        assert_eq!(reason_message(&t, &n, false), Some(0x103));
        assert_eq!(reason_message(&t, &Notice::default(), false), None);
        assert_eq!(reason_message(&t, &Notice { reason: Some(40), ..Notice::default() }, false), None, "no entry");
    }

    /// The executable's tables exist and give the notice its names and separator (needs `KH2_ISO`).
    #[test]
    fn the_executable_has_the_tables_and_the_texts_exist() {
        let Ok(iso) = std::env::var("KH2_ISO") else { return };
        let mut a = kh2_archive::Archive::open(&iso).unwrap();
        let exe = Exe::load(a.iso().read("SLUS_210.05").or_else(|_| a.iso().read("SLUS_210.05;1")).expect("the executable")).expect("executable");
        let t = Tables::read(&exe).expect("tables");
        assert_eq!(t.names.len(), 16);
        assert_eq!(t.reasons.len(), 16);
        assert_eq!(t.separator, "   ", "the list separator is three spaces");
        let m = kh2_image::text::Messages::from_bar(&a.read("msg/us/sys.bar").unwrap()).unwrap();
        assert_eq!(m.text(WINDOW_TEXT).as_deref(), Some("Party Members have been switched."));
        assert_eq!(m.text(PARTY_IN).as_deref(), Some("Party In"));
        assert_eq!(m.text(PARTY_OUT).as_deref(), Some("Party Out"));
        // Entry 0 of the member tables is empty (member 0 is nobody).
        for id in t.names.iter().chain(&t.reasons).chain(&[LINE_MODE_2]).filter(|id| **id != 0) {
            assert!(m.text(*id).is_some(), "message {id:#x}");
        }
    }
}
