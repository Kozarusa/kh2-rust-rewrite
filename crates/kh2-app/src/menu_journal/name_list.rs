//! The text lists of Jiminy's Journal that hang from page 0xc (docs/menus.md 20.11, WP-MN15): the worlds of the Character Files
//! (page 0xd, `FUN_002a8a38`), the characters of one world (0xe), the Heartless (0x11) and the Nobodies (0x14) (all three
//! `FUN_002a91a0`) and the Ansem reports (0x17, `FUN_002ac900`).
//!
//! All of them are ten list objects over a list of entries, the backdrop and the two layers 0x58 and 0x5d (the second holds the
//! scroll bar's track), a new tag child per row (`FUN_002aa8d0`, `FUN_002aaae0`, `FUN_002ad760` fill them; the redraw also marks the
//! entry under the cursor seen: `FUN_002ac4c8`, `FUN_002ade80`). The rows are the nodes of the Quests page (`games_page::Nodes`).

use super::chars::CharRec;
use super::enemies::{self, Move, Pos};
use super::*;

/// The literal groups of the list sequence (`FUN_002a91a0`: 0x58 and 0x5d).
pub const LAYER: usize = 0x58;
pub const BAR_LAYER: usize = 0x5d;
/// The message the list shows when it has no entry (`FUN_002aaae0`: `FUN_00190c40(0x36b3)`).
pub const NOTHING: u16 = 0x36b3;
/// The help lines (`FUN_002ac450`: pages 0xd, 0xe, 0x11, 0x14; `FUN_002ade50`: page 0x17).
pub const HELP: [(u16, u16); 6] = [(0xd, 0x48ee), (0xe, 0x48ef), (0x11, 0x48f0), (0x14, 0x48f1), (0x17, 0x48ec), (0x1d, 0x48f8)];
/// The limit page's layers (`FUN_002b2130`: 0x6c and 0x6d) and the message that follows a limit's count (`FUN_002b30d8`).
pub const LIMIT_LAYERS: [usize; 2] = [0x6c, 0x6d];
pub const LIMIT_UNIT: u16 = 0x4b06;

/// Whether `page` is one of the text lists.
pub fn is_list(page: u16) -> bool {
    matches!(page, 0xd | 0xe | 0x11 | 0x14 | 0x17 | 0x1d)
}

/// The help line of a list page.
pub fn help(page: u16) -> Option<u16> {
    HELP.iter().find(|h| h.0 == page).map(|h| h.1)
}

/// The character family a list page shows (`FUN_002ac370`: 0x11 for page 0x11, 0x12 for page 0x14; page 0xe the chosen world).
pub fn family_of(page: u16, world: u8) -> u8 {
    match page {
        0x11 => 0x11,
        0x14 => 0x12,
        _ => world,
    }
}

/// One row of a list: its text, the new-bit keys its tag summarises, and the entry it stands for (a world byte, a character record
/// or a report).
#[derive(Clone, Debug, Default)]
pub struct Row {
    pub label: Vec<Token>,
    pub keys: Vec<(Cat, usize)>,
    pub id: usize,
    /// The right-aligned value of a limit row (its count and unit).
    pub value: Option<Vec<Token>>,
}

/// The tokens with `Token::Variable` replaced (`FUN_00191a30`).
pub fn substitute(t: Vec<Token>, var: &[Token]) -> Vec<Token> {
    t.into_iter().flat_map(|k| if k == Token::Variable { var.to_vec() } else { vec![k] }).collect()
}

fn plain(t: Vec<Token>) -> Vec<Token> {
    t.into_iter().filter(|t| *t != Token::End).collect()
}

/// The name of a character record, with the world's name for a record that has a variable (`flags` 0x400).
pub fn char_name(j: &Journal, scene: &MenuScene, rec: &CharRec, family: u8, test: &dyn Fn(u16) -> bool) -> Vec<Token> {
    let t = plain(tokens(j, scene, rec.name).unwrap_or_default());
    if rec.flags & 0x400 != 0 { substitute(t, &world_name(j, scene, family as i8, test)) } else { t }
}

/// Whether the world `w`'s cell exists (`FUN_002a3120`).
fn world_exists(j: &Journal, w: u8, test: &dyn Fn(u16) -> bool) -> bool {
    j.worlds.iter().find(|r| r.id as u8 == w).is_some_and(|r| test(r.flag))
}

/// The rows of page 0xd (`FUN_002aa8d0`).
pub fn world_rows(j: &Journal, scene: &MenuScene, exists: &dyn Fn(usize) -> bool, test: &dyn Fn(u16) -> bool) -> Vec<Row> {
    let all = test(ALL_WORLDS_FLAG);
    enemies::world_rows(&|w| world_exists(j, w, test), all)
        .into_iter()
        .map(|w| {
            let name = world_name(j, scene, w as i8, test);
            let there = world_exists(j, w, test);
            let keys = if there { j.chars.iter().enumerate().filter(|(i, r)| r.family == w && exists(*i)).map(|(i, _)| (Cat::Char, i)).collect() } else { Vec::new() };
            Row { label: if there { name } else { treasure_page::masked(&name) }, keys, id: usize::from(w), ..Row::default() }
        })
        .collect()
}

/// The rows of pages 0xe, 0x11 and 0x14 (`FUN_002aa528`, `FUN_002aaae0`): the records of the family that exist, or all with the
/// all-worlds flag (a record that does not exist shows question marks and no tag).
pub fn family_rows(j: &Journal, scene: &MenuScene, family: u8, exists: &dyn Fn(usize) -> bool, test: &dyn Fn(u16) -> bool) -> Vec<Row> {
    chars::family_list(&j.chars, family, exists, test(ALL_WORLDS_FLAG))
        .into_iter()
        .map(|i| {
            let name = char_name(j, scene, &j.chars[i], family, test);
            let there = exists(i);
            Row { label: if there { name } else { treasure_page::masked(&name) }, keys: if there { vec![(Cat::Char, i)] } else { Vec::new() }, id: i, ..Row::default() }
        })
        .collect()
}

/// The rows of page 0x17 (`FUN_002ad6a0`, `FUN_002ad760`): the reports that are owned, or all with the all-worlds flag.
pub fn report_rows(j: &Journal, scene: &MenuScene, items: &dyn Fn(u16) -> i32, test: &dyn Fn(u16) -> bool) -> Vec<Row> {
    j.report_recs
        .iter()
        .enumerate()
        .filter(|(_, r)| items(r.item) > 0 || test(ALL_WORLDS_FLAG))
        .map(|(i, r)| {
            let name = plain(tokens(j, scene, r.title).unwrap_or_default());
            let there = items(r.item) > 0;
            Row { label: if there { name } else { treasure_page::masked(&name) }, keys: if there { vec![(Cat::Report, i)] } else { Vec::new() }, id: i, ..Row::default() }
        })
        .collect()
}

/// The rows of page 0x1d (`FUN_002b3018`, `FUN_002b30d8`): the limits that have been used, or all with the all-worlds flag; each with its
/// best count and the unit message at the right (a limit never used shows question marks and a count of 0).
pub fn limit_rows(j: &Journal, scene: &MenuScene, test: &dyn Fn(u16) -> bool) -> Vec<Row> {
    let unit = plain(tokens(j, scene, LIMIT_UNIT).unwrap_or_default());
    (0..j.limit_recs.len())
        .filter(|&i| j.limit_used(i) || test(ALL_WORLDS_FLAG))
        .map(|i| {
            let name = plain(tokens(j, scene, j.limit_recs[i].title).unwrap_or_default());
            let used = j.limit_used(i);
            let mut value = count_tokens(&j.tables.limit_fmt, u32::from(j.limit_best(i)));
            value.extend(unit.iter().cloned());
            Row { label: if used { name } else { treasure_page::masked(&name) }, keys: if used { vec![(Cat::Limit, i)] } else { Vec::new() }, id: i, value: Some(value) }
        })
        .collect()
}

/// The tag groups of a list page (the words at 0x382e90, 0x382edc).
fn tag_groups(j: &Journal, page: u16) -> [usize; 2] {
    match page {
        0x17 => j.tables.report_tag,
        0x1d => j.tables.limit_tag,
        _ => j.tables.name_tag,
    }
}

/// Builds a list page.
#[allow(clippy::too_many_arguments)]
pub fn build(j: &mut Journal, scene: &mut MenuScene, page: u16, test: &dyn Fn(u16) -> bool, items: &dyn Fn(u16) -> i32) {
    let (world, kept) = j.open.as_ref().map_or((0, Pos::default()), |o| (o.lworld, o.kept.iter().find(|k| k.0 == page).map_or(Pos::default(), |k| k.1)));
    let exists = exist_set(j, test, items);
    let rows = match page {
        0xd => world_rows(j, scene, &|i| exists.contains(&i), test),
        0x17 => report_rows(j, scene, items, test),
        0x1d => limit_rows(j, scene, test),
        _ => family_rows(j, scene, family_of(page, world), &|i| exists.contains(&i), test),
    };
    // The tab: the category's name (`FUN_002a7978`: `FUN_0029d820(0, 0x382d08[row])`), and on page 0xe the world's name on a second
    // plate (`FUN_0029d1d0(1, -1, 1)`, its text set by page 0xd's confirm).
    let cat = match page {
        0x17 => 0,
        0xd | 0xe => 1,
        0x11 => 2,
        0x1d => 8,
        _ => 3,
    };
    category_tab(j, scene, j.tables.c_text[cat]);
    if page == 0xe {
        plate_two(j, scene, world, test);
    }
    let list = j.seq_base + SEQ_LIST;
    let n = j.nodes.games;
    let layers = if page == 0x1d { LIMIT_LAYERS } else { [LAYER, BAR_LAYER] };
    for (k, g) in layers.into_iter().enumerate() {
        scene.nodes[n.layers[k]].visible = true;
        scene.nodes[n.layers[k]].off = [0.0; 2];
        scene.set_group(n.layers[k], g);
    }
    let group = |g: usize| scene.seqs.get(list).and_then(|s| s.groups.get(g)).copied().unwrap_or_default();
    let pitch = group(layers[0]).unk1c;
    // A limit row's value hangs from the row at the layer group's x / y, and its tag from the value (`FUN_002b2130`).
    let value_at = [group(layers[0]).x as f32, group(layers[0]).y as f32];
    for k in 0..games_page::ROWS {
        scene.nodes[n.rows[k]].off = [0.0, (k as i32 * pitch) as f32];
        scene.nodes[n.rows[k]].label_anchor = Some([0.0; 2]);
        scene.nodes[n.values[k]].visible = false;
        scene.nodes[n.values[k]].off = value_at;
        scene.nodes[n.values[k]].label_anchor = Some([0.0; 2]);
        scene.nodes[n.tags[k]].parent = Some(if page == 0x1d { n.values[k] } else { n.rows[k] });
        scene.nodes[n.tags[k]].off = [0.0; 2];
    }
    let count = rows.len();
    // The scroll bar (`FUN_00299878(.., max(count, 10), 10, parent = the second layer)`).
    let track = group(layers[1]);
    let body = scene.seqs.get(list).and_then(|s| s.groups.get(j.tables.bar[2])).map_or(1, |g| g.size.max(1));
    let bar = Bar::new(track.size, body, count.max(games_page::ROWS) as i32, games_page::ROWS as i32);
    // The position the page kept (`FUN_00277de8` / `FUN_00277e00`), kept inside the list.
    let pos = if count == 0 { Pos::default() } else { Pos { cursor: kept.cursor.min(count.min(enemies::VIEW) - 1), scroll: kept.scroll.min(count.saturating_sub(enemies::VIEW)) } };
    info!(
        "journal: page {page:#x} (FUN_002a91a0 / FUN_002a8a38 / FUN_002ac900): world {world:#x}, {count} rows {:?}, cursor {} scroll {}",
        rows.iter().take(12).map(|r| r.id).collect::<Vec<_>>(),
        pos.cursor,
        pos.scroll
    );
    let o = j.open.as_mut().unwrap();
    (o.page, o.lrows, o.lpos, o.bar) = (page, rows, pos, Some(bar));
    let layer = j.nodes.games.layers[1];
    place_bar(j, scene, layer, layers[1]);
    view(j, scene);
}

/// The second plate of page 0xe (`FUN_0029d1d0(1, -1, 1)`): the tab shifted by the plate group's x pitch, with the world's name.
fn plate_two(j: &Journal, scene: &mut MenuScene, world: u8, test: &dyn Fn(u16) -> bool) {
    let (plate, follow) = (j.tables.plate, j.tables.follow);
    let list = j.seq_base + SEQ_LIST;
    let dx = scene.seqs.get(list).and_then(|s| s.groups.get(plate[0])).map_or(0, |g| g.unk20);
    let (tab, tab_follow) = (j.nodes.tab2, j.nodes.tab2_follow);
    scene.nodes[tab].visible = true;
    scene.nodes[tab].pos = [dx as f32, 0.0];
    scene.set_group_then(tab, plate[0], plate[2]);
    let name = world_name(j, scene, world as i8, test);
    scene.nodes[tab].label = label_of(scene, tab, plate[2], name, 0, false);
    scene.nodes[tab].label_anchor = Some([0.0; 2]);
    scene.nodes[tab_follow].visible = true;
    scene.set_group_then(tab_follow, follow[0], follow[2]);
}

/// The rows in view, the tags and the thumb (`FUN_002aa848`: the rows, then the cursor entry is marked seen).
pub fn view(j: &mut Journal, scene: &mut MenuScene) {
    let Some(o) = j.open.as_ref() else { return };
    let (page, pos, count) = (o.page, o.lpos, o.lrows.len());
    // The entry under the cursor counts as seen first (a character `FUN_002ac4c8`, a report `FUN_002ade80`), so its tag reads "seen".
    if matches!(page, 0xe | 0x11 | 0x14 | 0x17 | 0x1d)
        && let Some(r) = o.lrows.get(pos.at()).cloned()
    {
        for k in r.keys {
            if j.marks.is_new(k.0, k.1) {
                j.marks.see(k.0, k.1);
            }
        }
    }
    let o = j.open.as_ref().unwrap();
    let tags = tag_groups(j, page);
    let n = j.nodes.games;
    let shown: Vec<Row> = o.lrows.iter().skip(pos.scroll).take(games_page::ROWS).cloned().collect();
    let empty = count == 0;
    for k in 0..games_page::ROWS {
        let on = k < shown.len() || (empty && k == 0);
        scene.nodes[n.rows[k]].visible = on;
        scene.nodes[n.tags[k]].visible = on;
        scene.nodes[n.values[k]].visible = false;
        scene.nodes[n.rows[k]].label = None;
        if !on {
            continue;
        }
        scene.set_group(n.rows[k], STORY_ROW);
        scene.set_group(n.tags[k], STORY_ROW);
        let label = match shown.get(k) {
            Some(r) => r.label.clone(),
            None => plain(tokens(j, scene, NOTHING).unwrap_or_default()),
        };
        scene.nodes[n.rows[k]].label = games_page::ink(scene, n.rows[k], STORY_ROW, label, 0);
        let value = shown.get(k).and_then(|r| r.value.clone());
        scene.nodes[n.values[k]].visible = value.is_some();
        if let Some(v) = value {
            scene.set_group(n.values[k], STORY_ROW);
            scene.nodes[n.values[k]].label = games_page::ink(scene, n.values[k], STORY_ROW, v, 1);
        }
        if let Some(r) = shown.get(k)
            && let Some(all_seen) = j.marks.aggregate(r.keys.iter().copied())
        {
            scene.set_group(n.tags[k], tags[usize::from(all_seen)]);
        }
    }
    bar_pos(j, scene, pos.scroll);
}

/// Moves the cursor of a list page; returns whether it moved.
pub fn step(j: &mut Journal, scene: &mut MenuScene, mv: Move) -> bool {
    let Some(o) = j.open.as_mut() else { return false };
    let to = enemies::step(o.lpos, o.lrows.len(), mv);
    if to == o.lpos {
        return false;
    }
    o.lpos = to;
    view(j, scene);
    true
}

/// A key on a list page (the handlers `FUN_002a8e90`, `FUN_002a9590`, `FUN_002accb8`): Up / Down and L1 / R1 move the widget's cursor,
/// a confirm opens the entry (a world's list, a character's page, a report), -2 goes back, -4 ends the scene.
pub fn input(j: &mut Journal, scene: &mut MenuScene, menu: &mut Menu, se: &mut MessageWriter<PlaySe>, key: Key) {
    let Some(o) = j.open.as_ref() else { return };
    let (page, wb, pos) = (o.page, o.world, o.lpos);
    match key {
        Key::Start => {
            menu_sound::play(se, menu_sound::CANCEL);
            close(j, scene, menu);
        }
        Key::Cancel => {
            menu_sound::play(se, menu_sound::CANCEL);
            let back = if page == 0xe { 0xd } else { 0xc };
            info!("journal: page {page:#x} cancel -> page {back:#x} (FUN_002a8e90 / FUN_002a9590 / FUN_002accb8 case -2)");
            j.goto = Some((back, wb));
        }
        Key::Move(_, dy) if dy != 0 => moved(j, scene, se, if dy > 0 { Move::Down } else { Move::Up }),
        Key::L1 => moved(j, scene, se, Move::PageUp),
        Key::R1 => moved(j, scene, se, Move::PageDown),
        Key::Confirm => {
            let Some(row) = j.open.as_ref().and_then(|o| o.lrows.get(pos.at()).cloned()) else { return };
            menu_sound::play(se, crate::menu_status::CONFIRM);
            let o = j.open.as_mut().unwrap();
            o.kept.retain(|k| k.0 != page);
            o.kept.push((page, pos));
            match page {
                0xd => {
                    // `FUN_002aa4b0`: the world of the cursor row; page 0xe starts at the top of its list.
                    o.lworld = row.id as u8;
                    o.kept.retain(|k| k.0 != 0xe);
                    info!("journal: page 0xd confirm on world {:#x} -> page 0xe (FUN_002a8e90)", row.id);
                    j.goto = Some((0xe, wb));
                }
                0x17 | 0x1d => {
                    o.rlist = o.lrows.iter().map(|r| r.id).collect();
                    o.rcur = pos.at();
                    let to = if page == 0x17 { 0x18 } else { 0x1e };
                    info!("journal: page {page:#x} confirm on entry {} -> page {to:#x} (FUN_002accb8 / FUN_002b25a8)", row.id);
                    j.goto = Some((to, wb));
                }
                _ => {
                    let list: Vec<usize> = o.lrows.iter().map(|r| r.id).collect();
                    o.cp = CharPage { list, cur: pos.at(), back: page, present: Default::default() };
                    info!("journal: page {page:#x} confirm on character {} -> page {:#x} (FUN_002a9590)", row.id, match page { 0xe => 0xf, 0x11 => 0x12, _ => 0x15 });
                    j.goto = Some((0xf, wb));
                }
            }
        }
        _ => {}
    }
}

fn moved(j: &mut Journal, scene: &mut MenuScene, se: &mut MessageWriter<PlaySe>, mv: Move) {
    if step(j, scene, mv) {
        place_cursor(j, scene);
        menu_sound::play(se, menu_sound::CURSOR);
        if let Some(o) = j.open.as_ref() {
            info!("journal: page {:#x} cursor {} scroll {} (entry {})", o.page, o.lpos.cursor, o.lpos.scroll, o.lpos.at());
        }
    }
}

/// The first row of the world list is the cursor row of the journal's own world (`FUN_002a8a38`); from page 0xc that is row 0.
/// The row of the entry `id` in a page's rows, if any.
pub fn row_of(rows: &[Row], id: usize) -> Option<usize> {
    rows.iter().position(|r| r.id == id)
}

/// `FUN_002a1c30`: the characters that exist: those of a face group that is on, and the enemy records whose defeat count is not 0.
pub fn exist_set(j: &Journal, test: &dyn Fn(u16) -> bool, items: &dyn Fn(u16) -> i32) -> std::collections::HashSet<usize> {
    let mut set: std::collections::HashSet<usize> = chars::present(&j.diag, &j.groups, test, items).into_iter().collect();
    set.extend(chars::enemies_present(&j.chars, &|r| defeats(j, r)));
    set
}

/// The defeated-enemy count of a record with the counter flag (`FUN_002a1f78`); 0 for any other record.
pub fn defeats(j: &Journal, rec: &CharRec) -> u32 {
    if rec.flags & chars::ENEMY_FLAG == 0 {
        return 0;
    }
    j.neo.get(&u32::from(rec.model)).map_or(0, |&n| j.snap.kills.get(&u32::from(n)).copied().unwrap_or(0))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_variable_of_a_name_is_replaced() {
        let t = vec![Token::Glyph(1), Token::Variable, Token::Glyph(2)];
        assert_eq!(substitute(t, &[Token::Glyph(7), Token::Glyph(8)]), vec![Token::Glyph(1), Token::Glyph(7), Token::Glyph(8), Token::Glyph(2)]);
    }

    #[test]
    fn the_list_pages_and_their_helps() {
        for p in [0xd, 0xe, 0x11, 0x14, 0x17] {
            assert!(is_list(p) && help(p).is_some());
        }
        assert!(!is_list(0xf) && !is_list(0xc));
        assert_eq!((family_of(0x11, 3), family_of(0x14, 3), family_of(0xe, 3)), (0x11, 0x12, 3));
        assert_eq!(row_of(&[Row { id: 4, ..Row::default() }, Row { id: 9, ..Row::default() }], 9), Some(1));
    }
}
