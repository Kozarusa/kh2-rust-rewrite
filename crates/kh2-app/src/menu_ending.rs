//! The battle report: menu kind 0xe (`ending.2ld`; docs/menus.md section 24), opened by a jump record of type 5.
//!
//! What the original does (`SLUS_210.05`, read in WP-JT4): `FUN_0019d9d8` case 5 calls `FUN_0024f0b8(0xe, -1)`, which
//! requests the kind's scene (record 0x380650, scene 9). Its init `FUN_00292f18` loads `ending.2ld` (the file name
//! `PTR_s_ending_2ld_0036d1b0`, `FUN_00293648` / `FUN_002935e8`), makes the page object `FUN_00293020` and starts the task
//! `FUN_00292fc8`, which loops without reading the pad. The page is the "BATTLE REPORT": a backdrop layout chosen by the
//! game mode (`FUN_001d77d0`), a title (message 0xcc24) and eight rows, each a heading (0xcc18..0xcc1f) and a value built
//! with `%d` / `%d%%` formats and the suffix message 0xcc20 ("Times").
//!
//! What the port does: the layout, title and rows are drawn from the file; the values come from [`Report`]. The counters
//! behind rows 1 to 7 (`FUN_002787b0`, `FUN_001de018`, save `+0x2fdc`, `+0x2dbe`, `+0x2fe2`, `+0x2fea`, `+0x2fec`) have no
//! writer in the app, so they stay 0 (a gap, docs/menus.md 24.4). The screen holds the field frozen until a confirm,
//! cancel or Start press closes it [inferred: the original's task never reads the pad and leaves with a scene change from
//! an event, which the app does not have].

use bevy::prelude::*;
use kh2_image::layd::{self, Layd};
use kh2_image::text::{Style, Token};

use crate::area_jump::JumpMenus;
use crate::assets::Disc;
use crate::menu_gfx::{self, MenuScene, Node};
use crate::menu_text::Label;
use crate::progress::Progress;
use crate::schedule::Pause;

/// Heading messages of the rows (`DAT_003827d0`, u16 each; `FUN_00190c40` looks them up in `sys.bar`).
pub const HEADINGS: [u32; 8] = [0xcc18, 0xcc19, 0xcc1a, 0xcc1b, 0xcc1c, 0xcc1d, 0xcc1e, 0xcc1f];
/// The title (`FUN_00293020`: the group 1 object's text 0xcc24) and the suffix after a count (0xcc20, "Times").
pub const TITLE: u32 = 0xcc24;
pub const SUFFIX: u32 = 0xcc20;
/// The game mode names (0xc331 + mode, `FUN_00293020` row 0 from the mode word at save `+0x1658`).
pub const MODE_NAMES: [u32; 3] = [0xc331, 0xc332, 0xc333];
/// The form names (`DAT_003827e8`: entry 1 to 5 for the u16 counters at save `+0x2fe2`..) and the summon names
/// (`DAT_00382800` by the index list `DAT_003827f8` = 0 1 2 3 for the counters at save `+0x2fec + 2 * index`).
pub const FORM_NAMES: [u32; 5] = [0x81e5, 0x81e6, 0x81e7, 0x81e8, 0x81e9];
pub const SUMMON_NAMES: [u32; 4] = [0xc594, 0xc592, 0xc591, 0xc593];

/// The numbers the page reads from the save.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Report {
    /// Game mode (save `+0x1658`): 0 Beginner, 1 Standard, 2 Proud.
    pub mode: u32,
    /// Row 1: `FUN_002787b0`, the journal's completion percentage.
    pub journal_pct: u32,
    /// Row 2: `FUN_001de018(save + 0xacc0)`, the Gummi routes' percentage.
    pub gummi_pct: u32,
    /// Row 3 (save `+0x2fdc`) and row 4 (save `+0x2dbe`).
    pub healed: u16,
    pub saved_by_mickey: u16,
    /// Row 5: uses per form (save `+0x2fe2`, five u16), row 6: the Antiform count (`+0x2fea`).
    pub forms: [u16; 5],
    pub antiform: u16,
    /// Row 7: uses per summon (save `+0x2fec`, indexed 0 to 3).
    pub summons: [u16; 4],
}

/// One row's value as the page prints it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Value {
    /// A message (row 0: the mode name).
    Message(u32),
    /// `%d%%`.
    Percent(u32),
    /// `%d` and the suffix message.
    Count(u16),
    /// A name message, then ` %d` and the suffix (rows 5 and 7).
    Named(u32, u16),
}

/// The backdrop layout of `ending.2ld` for a mode (`FUN_001d77d0`): mode 1 -> 1, mode 2 -> 3 when the completion
/// percentage (`FUN_002787b0`) is 100 and 2 otherwise, anything else -> 0.
pub fn layout_for(mode: u32, completion: u32) -> usize {
    match mode {
        1 => 1,
        2 if completion == 100 => 3,
        2 => 2,
        _ => 0,
    }
}

/// The first largest counter (`FUN_00293020`: the running maximum starts at -1 and is replaced only by a larger value,
/// so an equal later counter never wins and all zeros give the first entry). Returns (index, count).
pub fn first_max(counts: &[u16]) -> (usize, u16) {
    let mut best = (0, counts.first().copied().unwrap_or(0));
    for (i, &c) in counts.iter().enumerate().skip(1) {
        if c > best.1 {
            best = (i, c);
        }
    }
    best
}

/// The eight rows' values.
pub fn rows(r: &Report) -> [Value; 8] {
    let (form, form_n) = first_max(&r.forms);
    let (summon, summon_n) = first_max(&r.summons);
    [
        Value::Message(MODE_NAMES[(r.mode as usize).min(2)]),
        Value::Percent(r.journal_pct),
        Value::Percent(r.gummi_pct),
        Value::Count(r.healed),
        Value::Count(r.saved_by_mickey),
        Value::Named(FORM_NAMES[form], form_n),
        Value::Count(r.antiform),
        Value::Named(SUMMON_NAMES[summon], summon_n),
    ]
}

/// Group numbers of `ending.2ld` (sequence 0 holds the page's objects, sequence 1 the backdrops; the layout's layer `kind`
/// is the index of the image the layer's sequence draws from).
mod g {
    pub const TITLE: usize = 1;
    pub const ROW: usize = 2;
}

/// Row geometry in menu units: the row pitch (`unk1c` of group 2 = 40, the offset `FUN_00138ed0(row, 0, 40 * i)` of
/// `FUN_00293020`) and the value's offset from its row (204, 20 = the group's x and y, `FUN_00237618` / `FUN_00237638`); the
/// text anchors are the empty animations of the groups (title (106, 18), row (8, 46)). The rows' own origin is [inferred]
/// (8, 20): the layouts leave the left 224 units black and put the picture right of that, and the heading and value
/// have to fit in it.
mod at {
    pub const ROW_ORIGIN: [f32; 2] = [8.0, 20.0];
    pub const PITCH: f32 = 40.0;
    pub const TITLE: [f32; 2] = [106.0, 18.0];
    pub const LABEL: [f32; 2] = [8.0, 46.0];
    pub const VALUE_RIGHT: f32 = 204.0;
    /// The value's line under its heading [inferred: on the heading's line the two overlap].
    pub const VALUE_DROP: f32 = 18.0;
    pub const SIZE: f32 = 15.0;
}

#[derive(Default)]
struct Nodes {
    layers: Vec<usize>,
    title: usize,
    rows: Vec<usize>,
    values: Vec<usize>,
}

#[derive(Resource, Default)]
pub struct EndingMenu {
    layd: Layd,
    nodes: Nodes,
    tex_base: usize,
    seq_base: usize,
    tried: bool,
    loaded: bool,
    open: bool,
}

pub struct EndingPlugin;

impl Plugin for EndingPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<EndingMenu>()
            .add_systems(PreUpdate, (input.before(crate::menu::input), hold.after(crate::menu::input)).after(bevy::input::InputSystems).after(crate::harness::Keys))
            .add_systems(Update, tick.before(crate::schedule::Frame::Popups));
    }
}

fn load(m: &mut EndingMenu, disc: &Disc, scene: &mut MenuScene, images: &mut Assets<Image>) {
    m.tried = true;
    let Some(two) = disc.read("menu/us/ending.2ld").and_then(|b| layd::parse_2ld(&b)) else {
        warn!("menu: menu/us/ending.2ld is not readable; menu kind 0xe is off");
        return;
    };
    if scene.text.is_none() || two.images.len() < 5 || two.layd.seqs.len() < 2 {
        warn!("menu: menu kind 0xe needs the menu text and the images of ending.2ld; it is off");
        return;
    }
    m.tex_base = scene.texs.len();
    for img in &two.images {
        scene.texs.push(menu_gfx::Tex::new(img, images));
    }
    m.seq_base = scene.seqs.len();
    let Layd { layers, layouts, seqs } = two.layd;
    scene.seqs.extend(seqs);
    m.layd = Layd { layers, layouts, seqs: Vec::new() };
    let new = |scene: &mut MenuScene, seq: Option<usize>, tex: usize| {
        let mut n = Node::new(seq);
        n.tex = tex;
        n.visible = false;
        scene.add(n)
    };
    let (seq0, tex0) = (m.seq_base, m.tex_base);
    for _ in 0..3 {
        m.nodes.layers.push(new(scene, Some(m.seq_base + 1), m.tex_base + 1));
    }
    m.nodes.title = new(scene, Some(seq0), tex0);
    for _ in 0..8 {
        m.nodes.rows.push(new(scene, Some(seq0), tex0));
        m.nodes.values.push(new(scene, None, tex0));
    }
    m.loaded = true;
}

fn all_nodes(n: &Nodes) -> Vec<usize> {
    let mut v = n.layers.clone();
    v.push(n.title);
    v.extend(&n.rows);
    v.extend(&n.values);
    v
}

fn hide(m: &EndingMenu, scene: &mut MenuScene) {
    for i in all_nodes(&m.nodes) {
        scene.nodes[i].visible = false;
        scene.nodes[i].label = None;
    }
}

fn set_layout(m: &EndingMenu, scene: &mut MenuScene, n: usize) {
    let layers = m.layd.layout(n).to_vec();
    for (i, &id) in m.nodes.layers.iter().enumerate() {
        match layers.get(i) {
            Some(l) => {
                let node = &mut scene.nodes[id];
                node.visible = true;
                node.seq = Some(m.seq_base + l.seq);
                node.tex = m.tex_base + l.kind as usize;
                node.pos = [l.x as f32, l.y as f32];
                scene.set_group(id, l.group);
            }
            None => scene.nodes[id].visible = false,
        }
    }
}

fn message(scene: &MenuScene, id: u32) -> Vec<Token> {
    scene.text.as_ref().and_then(|t| t.messages.as_ref()).and_then(|m| m.tokens(id)).unwrap_or_default()
}

fn literal(scene: &MenuScene, text: &str) -> Vec<Token> {
    scene.text.as_ref().map(|t| t.literal(text, Style::default(), [0.0; 2]).tokens).unwrap_or_default()
}

/// The tokens of a value (`%d%%`, `%d` + suffix, name + ` %d` + suffix).
fn value_tokens(scene: &MenuScene, v: &Value) -> Vec<Token> {
    let mut out = match v {
        Value::Message(id) => message(scene, *id),
        Value::Percent(p) => literal(scene, &format!("{p}%")),
        Value::Count(n) => {
            let mut o = literal(scene, &n.to_string());
            o.extend(message(scene, SUFFIX));
            o
        }
        Value::Named(name, n) => {
            let mut o = message(scene, *name);
            o.extend(literal(scene, &format!(" {n}")));
            o.extend(message(scene, SUFFIX));
            o
        }
    };
    out.retain(|t| *t != Token::End);
    out
}

fn text(scene: &MenuScene, tokens: Vec<Token>, align: u8, offset: [f32; 2]) -> Option<Label> {
    let t = scene.text.as_ref()?;
    Some(Label { tokens, style: Style { font: 0, size: Some(at::SIZE), color: [0xe5, 0xe5, 0xe5, 0x80], align, world: t.world, ..Style::default() }, offset })
}

/// Builds the page for a report.
fn show(m: &EndingMenu, scene: &mut MenuScene, report: &Report) {
    hide(m, scene);
    set_layout(m, scene, layout_for(report.mode, report.journal_pct));
    let n = &m.nodes;
    scene.nodes[n.title].visible = true;
    scene.nodes[n.title].pos = [0.0, 0.0];
    scene.set_group(n.title, g::TITLE);
    let title = message(scene, TITLE);
    scene.nodes[n.title].label = text(scene, title, 2, at::TITLE);
    for (i, v) in rows(report).iter().enumerate() {
        let origin = [at::ROW_ORIGIN[0], at::ROW_ORIGIN[1] + at::PITCH * i as f32];
        let (row, value) = (n.rows[i], n.values[i]);
        scene.nodes[row].visible = true;
        scene.nodes[row].pos = origin;
        scene.set_group(row, g::ROW);
        let head = message(scene, HEADINGS[i]);
        scene.nodes[row].label = text(scene, head, 0, at::LABEL);
        scene.nodes[value].visible = true;
        scene.nodes[value].pos = origin;
        let tokens = value_tokens(scene, v);
        scene.nodes[value].label = text(scene, tokens, 1, [at::VALUE_RIGHT, at::LABEL[1] + at::VALUE_DROP]);
    }
    scene.hidden = false;
    scene.dim = 0.0;
}

/// Opens when a type 5 record asked for it; closes on a confirm, cancel or Start (see the module doc).
#[allow(clippy::too_many_arguments)]
fn input(
    mut m: ResMut<EndingMenu>,
    mut jump: ResMut<JumpMenus>,
    mut scene: ResMut<MenuScene>,
    mut images: ResMut<Assets<Image>>,
    disc: Res<Disc>,
    mut keys: ResMut<ButtonInput<KeyCode>>,
    progress: Option<Res<Progress>>,
    menu: Res<crate::menu::Menu>,
) {
    if std::mem::take(&mut jump.ending) {
        if !m.tried && scene.text.is_some() {
            load(&mut m, &disc, &mut scene, &mut images);
        }
        if !m.loaded || m.open || menu.phase != crate::menu::Phase::Closed {
            info!("menu: kind 0xe refused (loaded {}, open {}, a menu is up {})", m.loaded, m.open, menu.phase != crate::menu::Phase::Closed);
            return;
        }
        let report = Report { mode: progress.as_deref().map_or(crate::progress::GAME_MODE_DEFAULT, |p| p.game_mode()), ..Report::default() };
        show(&m, &mut scene, &report);
        m.open = true;
        info!(
            "menu: kind 0xe open (FUN_0024f0b8(0xe, -1), FUN_00292f18, page FUN_00293020): layout {}, rows {:?}",
            layout_for(report.mode, report.journal_pct),
            rows(&report)
        );
        keys.reset_all();
        return;
    }
    if !m.open {
        return;
    }
    let close = closes(&keys);
    keys.reset_all();
    if close {
        hide(&m, &mut scene);
        scene.hidden = true;
        m.open = false;
        info!("menu: kind 0xe closed: back to the field (FUN_001038a0) [inferred: the original's task never reads the pad]");
    }
}

/// The keys that close the screen (confirm, cancel, Start: an app stand-in [inferred]).
fn closes(keys: &ButtonInput<KeyCode>) -> bool {
    [KeyCode::Enter, KeyCode::NumpadEnter, KeyCode::Escape, KeyCode::Backspace, KeyCode::KeyJ, KeyCode::KeyK].iter().any(|k| keys.just_pressed(*k))
}

fn hold(m: Res<EndingMenu>, mut pause: ResMut<Pause>, mut keys: ResMut<ButtonInput<KeyCode>>) {
    if m.open {
        pause.menu = true;
        keys.reset_all();
    }
}

fn tick(m: Res<EndingMenu>, mut scene: ResMut<MenuScene>, real: Res<Time<Real>>) {
    if m.open {
        scene.advance(real.delta_secs());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Enter, NumpadEnter, Esc, Backspace, J and K each close the screen; other keys and a held (not just pressed) key do not.
    #[test]
    fn the_ending_screen_closes_on_its_keys() {
        for k in [KeyCode::Enter, KeyCode::NumpadEnter, KeyCode::Escape, KeyCode::Backspace, KeyCode::KeyJ, KeyCode::KeyK] {
            let mut keys = ButtonInput::<KeyCode>::default();
            assert!(!closes(&keys));
            keys.press(k);
            assert!(closes(&keys), "{k:?} closes");
            keys.clear();
            assert!(!closes(&keys), "{k:?} held is not a new press");
        }
        let mut keys = ButtonInput::<KeyCode>::default();
        keys.press(KeyCode::KeyW);
        keys.press(KeyCode::Space);
        assert!(!closes(&keys));
    }

    #[test]
    fn the_layout_follows_the_mode_and_completion() {
        assert_eq!(layout_for(0, 0), 0);
        assert_eq!(layout_for(1, 100), 1);
        assert_eq!(layout_for(2, 99), 2);
        assert_eq!(layout_for(2, 100), 3);
        assert_eq!(layout_for(7, 100), 0, "an unknown mode takes layout 0");
    }

    #[test]
    fn the_most_used_entry_is_the_first_maximum() {
        assert_eq!(first_max(&[0, 0, 0, 0, 0]), (0, 0), "all zeros give the first entry");
        assert_eq!(first_max(&[3, 9, 9, 1, 0]), (1, 9), "a later equal count never wins");
        assert_eq!(first_max(&[1, 2, 3, 4]), (3, 4));
    }

    #[test]
    fn the_rows_use_the_tables_of_the_executable() {
        let r = Report { mode: 2, journal_pct: 40, healed: 5, saved_by_mickey: 2, forms: [1, 0, 7, 0, 7], antiform: 3, summons: [0, 0, 4, 4], ..Report::default() };
        let v = rows(&r);
        assert_eq!(v[0], Value::Message(0xc333));
        assert_eq!(v[1], Value::Percent(40));
        assert_eq!(v[3], Value::Count(5));
        assert_eq!(v[5], Value::Named(0x81e7, 7), "Master Form, the first of the two 7s");
        assert_eq!(v[6], Value::Count(3));
        assert_eq!(v[7], Value::Named(0xc591, 4), "Stitch is summon index 2");
    }

    /// The disc's file has the four layouts of three layers, and the page's texts exist (needs `KH2_ISO`).
    #[test]
    fn the_file_and_texts_exist() {
        let Ok(iso) = std::env::var("KH2_ISO") else { return };
        let mut a = kh2_archive::Archive::open(&iso).unwrap();
        let two = a.read("menu/us/ending.2ld").ok().and_then(|b| layd::parse_2ld(&b)).expect("ending.2ld");
        assert_eq!(two.images.len(), 5);
        assert_eq!(two.layd.layouts.len(), 4);
        for n in 0..4 {
            let l = two.layd.layout(n);
            assert_eq!(l.len(), 3);
            assert_eq!(l[0].kind as usize, n + 1, "the backdrop of layout {n} is image {}", n + 1);
        }
        assert!(two.layd.seqs[0].groups.len() > g::ROW);
        assert_eq!(two.layd.seqs[0].groups[g::ROW].unk1c, 40, "the row pitch");
        let m = kh2_image::text::Messages::from_bar(&a.read("msg/us/sys.bar").unwrap()).unwrap();
        assert_eq!(m.text(TITLE).as_deref(), Some("BATTLE REPORT"));
        for id in HEADINGS.iter().chain(&MODE_NAMES).chain(&FORM_NAMES).chain(&SUMMON_NAMES) {
            assert!(m.text(*id).is_some(), "message {id:#x}");
        }
    }
}
