//! The command menu (bottom left): header banner, one bar per command with the command's name,
//! the selected bar with its medallion and icon, and the reaction-command capsule above the list.
//! Art is `field2d/us/<world>0command.2dd` (one `comw` image/sequence pair); layout and group
//! numbers come from the menu code at 0x13b000..0x13d900 and the group tables it reads in the
//! executable (read at run time here). See docs/hud.md.
//!
//! The game state is not read here: `CommandMenu` is a plain resource that the player module
//! fills in (which commands exist, which is selected, whether a reaction command is offered).

use bevy::prelude::*;
use kh2_image::text::Style;

use super::hud_gfx::{HudScene, Node};
use super::hud_text::Label;
use crate::assets::Disc;

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
    /// Text colour style, 0 to 8 (the byte at +0x11 of the list entry; 0 normal, 1 unavailable, 2 highlighted).
    pub style: u8,
}

/// One open list.
#[derive(Clone, Debug, Default)]
pub struct MenuPage {
    /// Which menu: 0 and 6 commands, 1 magic, 2 items, 3 forms, 4 party, 7 summons, 8 friends, 9 shortcuts
    /// (the art of each is listed in docs/hud.md).
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
    capsule: usize,
}

/// `KH2_HUDMENU=<kind>:<rows>:<cursor>[:r][;<kind>:<rows>:<cursor>...]` shows command lists (one
/// page per `;`) with names taken from the game's own command table; `r` adds a reaction command.
/// Display only.
pub fn demo_menu(spec: &str, disc: &Disc) -> Option<CommandMenu> {
    use kh2_tables::tables as t;
    let cmds: Vec<t::Cmd> = {
        let mut a = disc.0.lock().ok()?;
        kh2_tables::load::<t::Cmd>(&mut a, &t::CMD).ok()?.rows
    };
    let entry = |c: &t::Cmd| MenuEntry { message: c.message_id as u32, icon: c.cmd_icon, style: 0 };
    let mut menu = CommandMenu { visible: true, ..default() };
    for (n, page) in spec.split(';').enumerate() {
        let p: Vec<&str> = page.split(':').collect();
        let kind: u8 = p.first()?.parse().ok()?;
        let rows: usize = p.get(1).and_then(|v| v.parse().ok()).unwrap_or(4).min(ROWS);
        let cursor: usize = p.get(2).and_then(|v| v.parse().ok()).unwrap_or(0);
        // The first page lists the four basic commands (1 attack, 2 magic, 3 items, 4 drive handlers),
        // later pages and longer lists continue with other named commands from `skip` on.
        let mut entries: Vec<MenuEntry> = Vec::new();
        if n == 0 {
            for e in [1u16, 2, 3, 4] {
                if let Some(c) = cmds.iter().find(|c| c.id == e && c.execute == e).or_else(|| cmds.iter().find(|c| c.execute == e)) {
                    entries.push(entry(c));
                }
            }
        }
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
    let mut add = |scene: &mut HudScene| {
        let mut n = Node::new(file, None);
        n.visible = false;
        scene.add(n)
    };
    for p in 0..PAGES {
        nodes.header[p] = add(scene);
        for r in 0..ROWS {
            nodes.rows[p][r] = RowNodes { bar: add(scene), plate: add(scene), icon: add(scene), arrow: add(scene) };
        }
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
) {
    let (Some(file), Some(tb)) = (file.0, tables.0.as_ref()) else { return };
    if scene.files.len() <= file {
        return;
    }
    if !*demo {
        *demo = true;
        if let Some(m) = std::env::var("KH2_HUDMENU").ok().and_then(|s| demo_menu(&s, &disc)) {
            *menu = m;
        }
    }
    if !nodes.built {
        build(&mut scene, &mut nodes, file);
    }
    let nodes = &*nodes;
    let show = menu.visible;
    let last_count = menu.pages.last().map(|p| p.entries.len()).unwrap_or(0);
    for p in 0..PAGES {
        let page = menu.pages.get(p).filter(|_| show);
        let Some(page) = page else {
            scene.nodes[nodes.header[p]].visible = false;
            for r in 0..ROWS {
                let rn = nodes.rows[p][r];
                for n in [rn.bar, rn.plate, rn.icon, rn.arrow] {
                    scene.nodes[n].visible = false;
                    scene.nodes[n].label = None;
                }
            }
            continue;
        };
        let k = (page.kind as usize).min(KINDS - 1);
        let row = &tb.kind[k];
        let n = page.entries.len().min(ROWS);
        let x = 28.0 + 16.0 * p as f32;
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
        scene.nodes[h].pos = [x, 388.0 - 22.0 * n as f32];
        scene.nodes[h].visible = true;
        for r in 0..ROWS {
            let rn = nodes.rows[p][r];
            if r >= n {
                for id in [rn.bar, rn.plate, rn.icon, rn.arrow] {
                    scene.nodes[id].visible = false;
                    scene.nodes[id].label = None;
                }
                continue;
            }
            let e = &page.entries[r];
            let sel = r == page.cursor;
            let y = 410.0 - 22.0 * n as f32 + 22.0 * r as f32;
            let g = if sel { row[3 + p] } else if r + 1 == n { row[9 + p] } else { row[6 + p] } as usize;
            let bx = if sel { x + 8.0 } else { x };
            set(&mut scene, rn.bar, Some(g));
            scene.nodes[rn.bar].pos = [bx, y];
            let color = if sel { tb.selected } else { tb.normal }[(e.style as usize).min(8)];
            let label = scene.text.as_ref().and_then(|t| t.message(e.message, Style { font: TEXT_FONT, size: Some(TEXT_SIZE), color, ..Style::default() }, TEXT_OFFSET));
            scene.nodes[rn.bar].label = label;
            // The medallion, icon and arrow belong to the selected bar only.
            let icon = icon_group(e.icon).filter(|_| sel);
            set(&mut scene, rn.plate, sel.then_some(tb.plate[p] as usize));
            set(&mut scene, rn.icon, icon);
            set(&mut scene, rn.arrow, sel.then_some(ARROW[(page.kind != 0) as usize]));
            for id in [rn.plate, rn.icon, rn.arrow] {
                scene.nodes[id].pos = [bx, y];
            }
        }
    }
    // Reaction capsule above the list: at (36, 377 - 22 n) with the command's name.
    let c = nodes.capsule;
    let y = 377.0 - 22.0 * last_count as f32;
    let group = match (&menu.reaction, show) {
        (Some(r), true) => Some(if r.kind7 { tb.reaction[1] } else if r.alt { tb.reaction[2] } else { tb.reaction[0] } as usize),
        (None, true) => Some(tb.reaction[3] as usize),
        _ => None,
    };
    set(&mut scene, c, group);
    scene.nodes[c].pos = [36.0, y];
    let label: Option<Label> = menu.reaction.as_ref().filter(|_| show).and_then(|r| {
        scene.text.as_ref().and_then(|t| t.message(r.message, Style { font: TEXT_FONT, size: Some(TEXT_SIZE), ..Style::default() }, TEXT_OFFSET))
    });
    scene.nodes[c].label = label;
}
