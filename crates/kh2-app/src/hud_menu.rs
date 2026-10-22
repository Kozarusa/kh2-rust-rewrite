//! The command menu (bottom left): header banner, one bar per command with the command's name,
//! the selected bar with its medallion and icon, and the reaction-command capsule above the list.
//! Art is `field2d/us/<world>0command.2dd` (one `comw` image/sequence pair); layout and group
//! numbers come from the menu code at 0x13b000..0x13d900 and the group tables it reads in the
//! executable (read at run time here). See docs/hud.md.
//!
//! The game state is not read here: `CommandMenu` is a plain resource that `items.rs` fills in (the
//! stacked command lists with their statuses and cursors, docs/items.md) and `mirror_reaction` adds
//! the offered reaction command to.

use bevy::prelude::*;
use kh2_image::text::Style;

use super::hud_gfx::{Edge, HudScene, Node};
use super::hud_text::Label;
use crate::assets::Disc;
use crate::reaction_cmd::{ReactionOffer, ReactionRules, Status};

/// Menu pages that can be open at once (a sub-menu opens one page to the right).
pub const PAGES: usize = 3;
/// Rows the node pool holds per page.
pub const ROWS: usize = 9;
/// Number of menu kinds in the game's group table.
const KINDS: usize = 18;

/// A command shown in a list.
#[derive(Clone, Debug, Default)]
pub struct MenuEntry {
    /// The command's message id (`cmd.MessageId`; bit 15 is ignored).
    pub message: u32,
    /// `cmd.CmdIcon`: picks the icon on the selected bar (0 none, 1 to 7 as the game's switch at 0x13bd48).
    pub icon: u8,
    /// Text colour style, 0 to 8: the entry's status byte (entry +5, list +0x11 for the first entry):
    /// 0 usable, 1 a sub-menu with nothing usable, 2 a spell short of MP (orange), 5 unavailable,
    /// 6 an item only reserved (docs/items.md).
    pub style: u8,
}

/// One open list.
#[derive(Clone, Debug, Default)]
pub struct MenuPage {
    /// Which menu: the command list's index (0 and 6 the decks, 1 magic, 2 items, 3 forms, 4 the targets,
    /// 5 shortcuts, 7 summons, 8 friends, 9 limits; the art of each is listed in docs/hud.md).
    pub kind: u8,
    pub entries: Vec<MenuEntry>,
    pub cursor: usize,
}

/// The reaction command offered above the list.
#[derive(Clone, Debug, Default)]
pub struct Reaction {
    pub message: u32,
    /// The command's `Flags` bit 3 (picks the capsule's alternate art).
    pub alt: bool,
    /// The list entry type 7 (a second alternate).
    pub kind7: bool,
}

#[derive(Resource, Default, Clone, Debug)]
pub struct CommandMenu {
    pub visible: bool,
    pub pages: Vec<MenuPage>,
    pub reaction: Option<Reaction>,
}

/// Scene file index of the command pair (`comw`).
#[derive(Resource, Default)]
pub struct MenuFile(pub Option<usize>);

/// Tables the menu code reads from the executable (`SLUS_210.05`), read at run time.
#[derive(Resource, Default)]
pub struct MenuTables(pub Option<Tables>);

pub struct Tables {
    /// Per kind: 3 header groups, 3 selected-bar groups, 3 bar groups, 3 last-bar groups (by page).
    kind: Vec<[u32; 12]>,
    /// Header groups that open (0x355530) and idle (0x355540), by page.
    open: [u32; 3],
    idle: [u32; 3],
    /// The medallion group behind the selected bar's icon, by page (0x355560).
    plate: [u32; 3],
    /// Text colours by style: normal, selected (0x355600, 0x355628), RGBA bytes.
    normal: [[u8; 4]; 9],
    selected: [[u8; 4]; 9],
    /// Reaction capsule groups: idle with a command [normal, kind 7, alt] and without [empty, text] (0x355678).
    reaction: [u32; 5],
}

fn word(exe: &[u8], va: usize) -> Option<u32> {
    let o = va.checked_sub(0xff000)?;
    Some(u32::from_le_bytes(exe.get(o..o + 4)?.try_into().ok()?))
}

impl Tables {
    pub fn from_exe(exe: &[u8]) -> Option<Tables> {
        let w = |va: usize| word(exe, va);
        let mut kind = Vec::new();
        for k in 0..KINDS {
            let mut row = [0u32; 12];
            for (i, r) in row.iter_mut().enumerate() {
                *r = w(0x3551c0 + (k * 12 + i) * 4)?;
            }
            kind.push(row);
        }
        let three = |va: usize| Some([w(va)?, w(va + 4)?, w(va + 8)?]);
        let colors = |va: usize| {
            let mut out = [[0u8; 4]; 9];
            for (i, c) in out.iter_mut().enumerate() {
                *c = w(va + 4 * i)?.to_le_bytes();
            }
            Some(out)
        };
        Some(Tables {
            kind,
            open: three(0x355530)?,
            idle: three(0x355540)?,
            plate: three(0x355560)?,
            normal: colors(0x355600)?,
            selected: colors(0x355628)?,
            reaction: [w(0x355678)?, w(0x35567c)?, w(0x355680)?, w(0x355684)?, w(0x355688)?],
        })
    }
}

/// Icon group of the selected bar by `CmdIcon` (the switch at 0x13bd48).
fn icon_group(icon: u8) -> Option<usize> {
    Some(match icon {
        1 => 37,
        2 => 41,
        3 => 39,
        4 => 44,
        5 => 43,
        6 => 38,
        7 => 40,
        _ => return None,
    })
}

/// Left edge of page `p`'s bars: 28 pixels, 16 more per page (0x13c4b0).
pub fn page_x(p: usize) -> f32 {
    28.0 + 16.0 * p as f32
}

/// Centre line of bar `i` in a list of `n`: the list ends at 410 and grows upward (0x13c4b0).
pub fn row_y(n: usize, i: usize) -> f32 {
    410.0 - 22.0 * n as f32 + 22.0 * i as f32
}

/// Origin of a list's header banner (0x13bc50).
pub fn header_y(n: usize) -> f32 {
    388.0 - 22.0 * n as f32
}

/// Origin of the reaction capsule above a list of `n` (0x13cef8).
pub fn capsule_y(n: usize) -> f32 {
    377.0 - 22.0 * n as f32
}

/// The bar group for row `r` of `n` on page `p`: the selected bar, the last bar, or any other (0x13c4b0,
/// 0x13c288), from a kind's 12 table words.
fn bar_group(row: &[u32; 12], p: usize, r: usize, n: usize, selected: bool) -> usize {
    (if selected { row[3 + p] } else if r + 1 == n { row[9 + p] } else { row[6 + p] }) as usize
}

/// The arrow beside the selected bar: group 20 for the first kind, 21 for the others (0x13bd48).
const ARROW: [usize; 2] = [20, 21];
/// Text offset inside a bar and the bar text's font and size (0x13abf0: +0x190 = 10, +0x194 = -7,
/// font 1, size 14).
const TEXT_OFFSET: [f32; 2] = [10.0, -7.0];
const TEXT_FONT: usize = 1;
const TEXT_SIZE: f32 = 14.0;

#[derive(Clone, Copy, Default)]
struct RowNodes {
    bar: usize,
    plate: usize,
    icon: usize,
    arrow: usize,
}

#[derive(Resource, Default)]
pub struct MenuNodes {
    built: bool,
    header: [usize; PAGES],
    rows: [[RowNodes; ROWS]; PAGES],
    /// The selected bar with its medallion, icon and arrow: the game draws the other bars first and the selected
    /// one last (0x13c4b0 loops the rows then calls 0x13c288), so these nodes come after every plain bar.
    selected: [RowNodes; PAGES],
    capsule: usize,
}

/// Only the first page draws the selected bar's medallion, icon and arrow (0x13bd38 returns `page == 0`).
fn shows_icons(page: usize) -> bool {
    page == 0
}

fn selected_nodes(r: RowNodes) -> [usize; 4] {
    [r.bar, r.plate, r.icon, r.arrow]
}

fn command_rows(disc: &Disc) -> Option<Vec<kh2_tables::tables::Cmd>> {
    use kh2_tables::tables as t;
    let mut a = disc.0.lock().ok()?;
    Some(kh2_tables::load::<t::Cmd>(&mut a, &t::CMD).ok()?.rows)
}

fn entry(c: &kh2_tables::tables::Cmd) -> MenuEntry {
    MenuEntry { message: c.message_id as u32, icon: c.cmd_icon, style: 0 }
}

/// The four basic commands (the rows whose Execute is 1 attack, 2 magic, 3 items, 4 drive, each
/// by its own id first). In play `items.rs` fills `CommandMenu` with the lists the original builds
/// (the command manager's lists, stacked at manager +0x974, drawn by 0x13ca30); this stand-in is
/// only shown before that system has run and by the `KH2_HUDMENU` demo.
fn base_entries(cmds: &[kh2_tables::tables::Cmd]) -> Vec<MenuEntry> {
    [1u16, 2, 3, 4]
        .iter()
        .filter_map(|&e| cmds.iter().find(|c| c.id == e && c.execute == e).or_else(|| cmds.iter().find(|c| c.execute == e)))
        .map(entry)
        .collect()
}

/// The menu shown in play: one page of the basic commands with the cursor on the first.
pub fn default_menu(disc: &Disc) -> Option<CommandMenu> {
    let cmds = command_rows(disc)?;
    Some(CommandMenu { visible: true, pages: vec![MenuPage { kind: 0, entries: base_entries(&cmds), cursor: 0 }], reaction: None })
}

/// Hook 1 of docs/limits-reactions.md: the offered reaction command (slot 1 of the command manager)
/// becomes the capsule above the list while it is available; `alt` is the command's Flags bit 3.
/// The entry-type-7 variant has no source in the port.
pub fn mirror_reaction(offer: Option<Res<ReactionOffer>>, rules: Option<Res<ReactionRules>>, mut menu: ResMut<CommandMenu>) {
    let active = offer.as_ref().and_then(|o| o.0).filter(|a| a.status == Status::Available);
    let next = active.map(|a| {
        let flags = rules.as_ref().and_then(|r| r.cmd(a.cmd)).map(|c| c.flags).unwrap_or(0);
        Reaction { message: a.message, alt: flags & 8 != 0, kind7: false }
    });
    let same = match (&menu.reaction, &next) {
        (None, None) => true,
        (Some(a), Some(b)) => a.message == b.message && a.alt == b.alt,
        _ => false,
    };
    if !same {
        menu.reaction = next;
    }
}

/// `KH2_HUDMENU=<kind>:<rows>:<cursor>[:r][;<kind>:<rows>:<cursor>...]` shows command lists (one
/// page per `;`) with names taken from the game's own command table; `r` adds a reaction command.
/// Display only.
pub fn demo_menu(spec: &str, disc: &Disc) -> Option<CommandMenu> {
    let cmds = command_rows(disc)?;
    let mut menu = CommandMenu { visible: true, ..default() };
    for (n, page) in spec.split(';').enumerate() {
        let p: Vec<&str> = page.split(':').collect();
        let kind: u8 = p.first()?.parse().ok()?;
        let rows: usize = p.get(1).and_then(|v| v.parse().ok()).unwrap_or(4).min(ROWS);
        let cursor: usize = p.get(2).and_then(|v| v.parse().ok()).unwrap_or(0);
        // The first page lists the four basic commands (1 attack, 2 magic, 3 items, 4 drive handlers),
        // later pages and longer lists continue with other named commands from `skip` on.
        let mut entries: Vec<MenuEntry> = if n == 0 { base_entries(&cmds) } else { Vec::new() };
        for c in cmds.iter().filter(|c| c.message_id > 0 && !matches!(c.execute, 1..=4)).skip(n * 12) {
            if entries.len() >= rows {
                break;
            }
            entries.push(entry(c));
        }
        entries.truncate(rows);
        if p.get(3) == Some(&"r") {
            menu.reaction = cmds.iter().find(|c| c.message_id > 0 && c.execute == 0x13c).or(cmds.get(20)).map(|c| Reaction { message: c.message_id as u32, alt: false, kind7: false });
        }
        menu.pages.push(MenuPage { kind, entries, cursor });
    }
    Some(menu)
}

/// The label of a bar: the command's message through the game's lookup (0x190c40: the system table,
/// then the world's, then message 0x8adc) at the bar's text offset, font and size (0x13abf0), in the
/// colour the status picks (0x13ba80). The colour goes to the text engine as the table has it: the
/// game's product is `texel (0x80) * colour >> 7`, which the exact atlas of `hud_text` reproduces
/// (docs/hud.md "Text colour"), so no per-bar compensation is needed.
fn name_label(text: &super::hud_text::TextSys, message: u32, color: [u8; 4]) -> Option<Label> {
    let tokens = text.lookup(message)?;
    Some(Label { tokens, style: Style { world: text.world, font: TEXT_FONT, size: Some(TEXT_SIZE), color, ..Style::default() }, offset: TEXT_OFFSET })
}

/// `KH2_HUDMENULOG=1`: one line per page whenever the drawn lists change (message id, status, name).
fn log_pages(menu: &CommandMenu, scene: &HudScene, last: &mut String) {
    let mut line = String::new();
    for p in menu.pages.iter().filter(|_| menu.visible) {
        let names: Vec<String> = p
            .entries
            .iter()
            .map(|e| {
                let t = scene.text.as_ref().and_then(|t| t.lookup(e.message)).map(|k| kh2_image::text::to_text(&k));
                format!("{:#x}/s{} {:?}", e.message, e.style, t.unwrap_or_default())
            })
            .collect();
        line += &format!("[kind {} cursor {}: {}] ", p.kind, p.cursor, names.join(", "));
    }
    if line != *last {
        info!("hudmenu: {line}");
        *last = line;
    }
}

fn set(scene: &mut HudScene, node: usize, group: Option<usize>) {
    let n = &mut scene.nodes[node];
    match group {
        None => n.visible = false,
        Some(g) => {
            n.visible = true;
            if n.player.group != Some(g) {
                scene.set_group(node, g);
            }
        }
    }
}

fn build(scene: &mut HudScene, nodes: &mut MenuNodes, file: usize) {
    let add = |scene: &mut HudScene| {
        let mut n = Node::new(file, None);
        n.visible = false;
        // The command deck keeps its 4:3 distance from the LEFT screen edge (WP-FIX-12).
        n.edge = Edge::Left;
        scene.add(n)
    };
    for p in 0..PAGES {
        nodes.header[p] = add(scene);
        // Draw order inside a page (0x13c4b0): the plain bars, then the selected bar, then its medallion, icon and
        // arrow (0x13c288, 0x13bd48), so the icon lies over the neighbouring bars and not under them.
        for r in 0..ROWS {
            nodes.rows[p][r] = RowNodes { bar: add(scene), ..default() };
        }
        nodes.selected[p] = RowNodes { bar: add(scene), plate: add(scene), icon: add(scene), arrow: add(scene) };
    }
    nodes.capsule = add(scene);
    nodes.built = true;
}

#[allow(clippy::too_many_arguments)]
pub fn update(
    mut scene: ResMut<HudScene>,
    mut menu: ResMut<CommandMenu>,
    file: Res<MenuFile>,
    tables: Res<MenuTables>,
    mut nodes: ResMut<MenuNodes>,
    disc: Res<Disc>,
    mut demo: Local<bool>,
    mut last_log: Local<String>,
) {
    let (Some(file), Some(tb)) = (file.0, tables.0.as_ref()) else { return };
    if scene.files.len() <= file {
        return;
    }
    if !*demo {
        *demo = true;
        if let Some(m) = std::env::var("KH2_HUDMENU").ok().and_then(|s| demo_menu(&s, &disc)) {
            *menu = m;
        } else if menu.pages.is_empty() {
            // Nothing filled the menu: show the basic list the game opens with.
            if let Some(m) = default_menu(&disc) {
                *menu = m;
            }
        }
    }
    if !nodes.built {
        build(&mut scene, &mut nodes, file);
    }
    let nodes = &*nodes;
    if std::env::var_os("KH2_HUDMENULOG").is_some() {
        log_pages(&menu, &scene, &mut last_log);
    }
    let show = menu.visible;
    let last_count = menu.pages.last().map(|p| p.entries.len()).unwrap_or(0);
    for p in 0..PAGES {
        let page = menu.pages.get(p).filter(|_| show);
        let Some(page) = page else {
            scene.nodes[nodes.header[p]].visible = false;
            for r in 0..ROWS {
                let rn = nodes.rows[p][r];
                scene.nodes[rn.bar].visible = false;
                scene.nodes[rn.bar].label = None;
            }
            for n in selected_nodes(nodes.selected[p]) {
                scene.nodes[n].visible = false;
                scene.nodes[n].label = None;
            }
            continue;
        };
        let k = (page.kind as usize).min(KINDS - 1);
        let row = &tb.kind[k];
        let n = page.entries.len().min(ROWS);
        let x = page_x(p);
        // Header: the commands banner opens (0x355530) then idles (0x355540); the other menus keep their own group.
        let h = nodes.header[p];
        let animated = matches!(page.kind, 0 | 6);
        let idle = row[p] as usize;
        let want = if animated { tb.idle[p] as usize } else { idle };
        if animated {
            let first = tb.open[p] as usize;
            let cur = scene.nodes[h].player.group;
            if cur != Some(first) && cur != Some(want) {
                set(&mut scene, h, Some(first));
            } else if cur == Some(first) && scene.nodes[h].player.finished() {
                set(&mut scene, h, Some(want));
            }
        } else {
            set(&mut scene, h, Some(want));
        }
        scene.nodes[h].pos = [x, header_y(n)];
        scene.nodes[h].visible = true;
        for r in 0..ROWS {
            if r >= n {
                scene.nodes[nodes.rows[p][r].bar].visible = false;
                scene.nodes[nodes.rows[p][r].bar].label = None;
                continue;
            }
            let e = &page.entries[r];
            let sel = r == page.cursor;
            let rn = if sel { nodes.selected[p] } else { nodes.rows[p][r] };
            if sel {
                scene.nodes[nodes.rows[p][r].bar].visible = false;
                scene.nodes[nodes.rows[p][r].bar].label = None;
            }
            let y = row_y(n, r);
            let g = bar_group(row, p, r, n, sel);
            let bx = if sel { x + 8.0 } else { x };
            set(&mut scene, rn.bar, Some(g));
            scene.nodes[rn.bar].pos = [bx, y];
            let color = if sel { tb.selected } else { tb.normal }[(e.style as usize).min(8)];
            let label = scene.text.as_ref().and_then(|t| name_label(t, e.message, color));
            scene.nodes[rn.bar].label = label;
            // The medallion, icon and arrow belong to the selected bar of the first page only (0x13bd38 is
            // `page == 0` and 0x13bd48 returns without drawing them otherwise).
            if sel {
                let first = shows_icons(p);
                set(&mut scene, rn.plate, first.then_some(tb.plate[p] as usize));
                set(&mut scene, rn.icon, icon_group(e.icon).filter(|_| first));
                set(&mut scene, rn.arrow, first.then_some(ARROW[(page.kind != 0) as usize]));
                for id in [rn.plate, rn.icon, rn.arrow] {
                    scene.nodes[id].pos = [bx, y];
                }
            }
        }
        if page.cursor >= n {
            for id in [nodes.selected[p].bar, nodes.selected[p].plate, nodes.selected[p].icon, nodes.selected[p].arrow] {
                scene.nodes[id].visible = false;
                scene.nodes[id].label = None;
            }
        }
    }
    // Reaction capsule above the list: at (36, 377 - 22 n) with the command's name.
    let c = nodes.capsule;
    let y = capsule_y(last_count);
    let group = match (&menu.reaction, show) {
        (Some(r), true) => Some(if r.kind7 { tb.reaction[1] } else if r.alt { tb.reaction[2] } else { tb.reaction[0] } as usize),
        (None, true) => Some(tb.reaction[3] as usize),
        _ => None,
    };
    set(&mut scene, c, group);
    scene.nodes[c].pos = [36.0, y];
    let label: Option<Label> = menu.reaction.as_ref().filter(|_| show).and_then(|r| {
        scene.text.as_ref().and_then(|t| t.message(r.message, Style { font: TEXT_FONT, size: Some(TEXT_SIZE), color: [0xff, 0xff, 0xff, 0x80], ..Style::default() }, TEXT_OFFSET))
    });
    scene.nodes[c].label = label;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lists_grow_upward_from_a_fixed_bottom() {
        // The last bar of any list sits at the same place; each extra row moves the rest up by 22.
        assert_eq!(row_y(4, 3), row_y(7, 6));
        assert_eq!(row_y(4, 0) - row_y(5, 0), 22.0);
        assert_eq!(row_y(4, 1) - row_y(4, 0), 22.0);
        // The header sits 22 above the first bar, the capsule 33 above it.
        assert_eq!(row_y(4, 0) - header_y(4), 22.0);
        assert_eq!(row_y(4, 0) - capsule_y(4), 33.0);
        assert_eq!(page_x(2) - page_x(0), 32.0);
    }

    /// Node order is draw order: the selected bar, its medallion, icon and arrow come after every plain bar of
    /// the page, and only the first page shows the medallion, icon and arrow.
    #[test]
    fn selected_bar_and_icon_draw_over_the_plain_bars() {
        let mut scene = HudScene::default();
        let mut nodes = MenuNodes::default();
        build(&mut scene, &mut nodes, 0);
        for p in 0..PAGES {
            let last_bar = nodes.rows[p].iter().map(|r| r.bar).max().unwrap();
            let s = nodes.selected[p];
            assert!(s.bar > last_bar && s.plate > s.bar && s.icon > s.plate && s.arrow > s.icon);
        }
        assert!(shows_icons(0) && !shows_icons(1) && !shows_icons(2));
    }

    #[test]
    fn bar_groups_pick_by_state_and_page() {
        let row = [18, 86, 31, 9, 77, 22, 16, 84, 29, 11, 79, 24];
        assert_eq!(bar_group(&row, 0, 1, 4, true), 9);
        assert_eq!(bar_group(&row, 1, 1, 4, true), 77);
        assert_eq!(bar_group(&row, 0, 0, 4, false), 16);
        assert_eq!(bar_group(&row, 2, 3, 4, false), 24);
    }

    #[test]
    fn tables_read_from_a_synthetic_executable() {
        // A fake image big enough to cover the highest table (0x3556b0), words counting up from their address.
        let mut exe = vec![0u8; 0x3556c0 - 0xff000];
        for (i, c) in exe.chunks_mut(4).enumerate() {
            c.copy_from_slice(&(i as u32).to_le_bytes());
        }
        let t = Tables::from_exe(&exe).expect("tables");
        assert_eq!(t.kind.len(), KINDS);
        assert_eq!(t.kind[0][0], ((0x3551c0 - 0xff000) / 4) as u32);
        assert_eq!(t.open[1] - t.open[0], 1);
        assert!(Tables::from_exe(&exe[..0x1000]).is_none());
    }

    /// The disc: the command table, the system messages and the executable's colour tables.
    fn disc() -> Option<(Vec<kh2_tables::tables::Cmd>, kh2_image::text::Messages, Tables)> {
        use kh2_tables::tables as t;
        let iso = std::env::var("KH2_ISO").ok()?;
        let mut a = kh2_archive::Archive::open(&iso).ok()?;
        let cmds = kh2_tables::load::<t::Cmd>(&mut a, &t::CMD).ok()?.rows;
        let sys = kh2_image::text::Messages::from_bar(&a.read("msg/us/sys.bar").ok()?)?;
        let exe = ["SLUS_210.05", "SLUS_210.05;1"].iter().find_map(|p| a.iso().read(p).ok())?;
        Some((cmds, sys, Tables::from_exe(&exe)?))
    }

    /// The "?" of the deck is the real name of the empty bar 0x68 (`Execute` 0, message 0x8503 in
    /// `sys.bar`, the clip 4.mp4 shows it in the same places); no other command row is named "?", and
    /// every command the lists can offer has a real name (docs/hud.md, WP-G2).
    #[test]
    fn question_mark_is_only_the_empty_bar() {
        let Some((cmds, sys, _)) = disc() else { return eprintln!("skipped: no disc (set KH2_ISO)") };
        let empty = cmds.iter().find(|c| c.id == 0x68).expect("command 0x68");
        assert_eq!(empty.execute, 0);
        assert_eq!(sys.text(empty.message_id as u32).as_deref(), Some("?"));
        for c in cmds.iter().filter(|c| c.id != 0x68 && c.message_id != 0) {
            assert_ne!(sys.text(c.message_id as u32).as_deref(), Some("?"), "command {} is named ?", c.id);
        }
        for id in [1u16, 2, 3, 4, 5, 0x48, 0x49, 0x51, 0x13c, 49, 50, 51, 52] {
            let c = cmds.iter().find(|c| c.id == id).expect("command");
            let name = sys.text(c.message_id as u32).unwrap_or_default();
            assert!(!name.is_empty() && name != "?", "command {id} has no name ({name:?})");
        }
    }

    /// The status byte picks the text colour (0x13ba80): grey (1, 5, 6) is darker than the normal
    /// light colour and orange (2) differs, for the normal and the selected bar.
    #[test]
    fn unavailable_commands_draw_darker() {
        let Some((_, _, tb)) = disc() else { return eprintln!("skipped: no disc (set KH2_ISO)") };
        for table in [&tb.normal, &tb.selected] {
            let light: u32 = table[0][..3].iter().map(|&v| v as u32).sum();
            for s in [1usize, 5, 6] {
                let grey: u32 = table[s][..3].iter().map(|&v| v as u32).sum();
                assert!(grey * 10 < light * 8, "style {s} is not clearly darker than style 0");
            }
            assert_ne!(table[2][..3], table[0][..3]);
        }
    }

    #[test]
    fn icon_groups_follow_the_game_switch() {
        assert_eq!(icon_group(1), Some(37));
        assert_eq!(icon_group(4), Some(44));
        assert_eq!(icon_group(0), None);
        assert_eq!(icon_group(8), None);
    }
}
