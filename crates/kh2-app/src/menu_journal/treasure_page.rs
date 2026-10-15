//! The Treasures (7) and Maps (8) pages' objects and redraw (docs/menus.md 20.9, WP-MN12): `FUN_002afca0` builds them,
//! `FUN_002b0e78` makes the objects, `FUN_002b0678` / `FUN_002b09e8` / `FUN_002b0cc8` redraw rows, cells and the info text,
//! `FUN_002b1890` moves the cursor. The layout and the cursor rules are in `lists.rs`.

use super::lists::{self, Layout, Nav, Row};
use super::*;

/// The page's literal groups of `jmemo.2ld` sequence 5 (`FUN_002afca0`, `FUN_002b0e78`, `FUN_002b09e8`, `FUN_002b0678`,
/// `FUN_002b1890`): the two layers, the row label, the cells (treasures found / not found, maps found / not found), the
/// total's label, the cursor frames (0x41, and 0x42 on a row with no entry).
pub const LAYERS: [usize; 2] = [0x3b, 0x43];
pub const LABEL: usize = 0x46;
pub const CELL_TREASURE: [usize; 2] = [0x48, 0x47];
pub const CELL_MAP: [usize; 2] = [0x3e, 0x3d];
pub const TOTAL: usize = 0x3c;
pub const FRAME: [usize; 2] = [0x41, 0x42];
/// The tag groups of a world's label row (`FUN_002b0678`: the words at 0x382f58 / 0x382f5c: new, new and seen).
pub const LABEL_TAG: [usize; 2] = [0x3f, 0x40];
/// The total's count text group (`FUN_00278418(2)`: the table at 0x381fe8) and the label font (`+0x1a0 = 3`).
const LABEL_FONT: usize = 3;

/// The `trsr` rows and the item rows the pages read.
#[derive(Default)]
pub struct Tables {
    pub rows: Vec<Row>,
    pub items: std::collections::HashMap<u16, ItemRow>,
}

/// The fields of an item row the pages read: its type (maps are 0x16), its name message and its second icon.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ItemRow {
    pub kind: u8,
    pub name: u16,
    pub icon: u8,
}

/// The item type of a map (`FUN_002b0228`: `item + 2 == 0x16`).
pub const MAP_KIND: u8 = 0x16;

/// Decodes the `trsr` and `item` tables of `03system.bin`.
pub fn load_tables(disc: &Disc) -> Tables {
    tables_from(disc.read("03system.bin"))
}

/// The same from the bytes of `03system.bin`.
pub fn tables_from(file: Option<Vec<u8>>) -> Tables {
    use kh2_tables::tables as t;
    let Some(file) = file else { return Tables::default() };
    let Ok(entries) = kh2_bar::parse(&file) else { return Tables::default() };
    let blob = |tag: &str| kh2_bar::find(&entries, tag).map(|e| e.data.to_vec());
    let rows = blob("trsr")
        .and_then(|b| kh2_tables::rt::decode::<t::Trsr>(&t::TRSR, &b).ok())
        .map(|d| d.rows.iter().map(|r| Row { kind: r.r#type, world: r.world, flag: r.event_id, item: r.item_id, chest: r.overall_chest_index }).collect())
        .unwrap_or_default();
    let items = blob("item")
        .and_then(|b| kh2_tables::rt::decode::<t::Item>(&t::ITEM, &b).ok())
        .map(|d| d.rows.iter().map(|r| (r.id, ItemRow { kind: r.r#type, name: r.name, icon: r.icon2 })).collect())
        .unwrap_or_default();
    Tables { rows, items }
}

/// The treasure rows that exist for the new bits (`FUN_002b87b8`): a chest with a flag (not 0, not 0x800) that is set, in a
/// world whose cell exists. Keys are the table ordinals.
pub fn treasures_present(rows: &[Row], world_index: &dyn Fn(u8) -> i8, flag: &dyn Fn(u16) -> bool, open: &dyn Fn(usize) -> bool) -> Vec<(Cat, usize)> {
    rows.iter()
        .enumerate()
        .filter(|(_, r)| r.kind == 0 && r.flag != 0 && r.flag != 0x800 && flag(r.flag as u16))
        .filter(|(_, r)| {
            let w = world_index(r.world);
            w >= 0 && open(w as usize)
        })
        .map(|(i, _)| (Cat::Treasure, i))
        .collect()
}

/// The owned maps for the new bits (`FUN_002b8b88`).
pub fn maps_present(rows: &[Row], is_map: &dyn Fn(u16) -> bool, owned: &dyn Fn(u16) -> bool) -> Vec<(Cat, usize)> {
    rows.iter().enumerate().filter(|(_, r)| is_map(r.item) && owned(r.item)).map(|(i, _)| (Cat::Map, i)).collect()
}

/// What the page needs of the game state.
pub struct Game<'a> {
    pub flag: &'a dyn Fn(u16) -> bool,
    pub items: &'a dyn Fn(u16) -> i32,
}

fn cat_of(page: u16) -> Cat {
    if page == 7 { Cat::Treasure } else { Cat::Map }
}

/// A world's cell exists (`FUN_002a3120`: the world record's flag, or always for an id from 0x10).
fn world_open(j: &Journal, w: usize, test: &dyn Fn(u16) -> bool) -> bool {
    j.worlds.iter().find(|r| usize::try_from(r.id).is_ok_and(|id| id == w)).is_some_and(|r| test(r.flag))
}

/// The entries of the page (`FUN_002b00e0` / `FUN_002b0228` and the sort of `FUN_002b0358`) and their layout.
fn make_layout(j: &Journal, page: u16, g: &Game) -> Layout {
    let wi = |w: u8| j.world_index.get(usize::from(w)).copied().unwrap_or(-1);
    let all = (g.flag)(ALL_WORLDS_FLAG);
    let entries = if page == 7 {
        // [no chest data] the app records no opened chest: every listed chest is not found.
        lists::treasures(&j.tables_lists.rows, &wi, g.flag, all, &|_| false)
    } else {
        let is_map = |i: u16| j.tables_lists.items.get(&i).is_some_and(|r| r.kind == MAP_KIND);
        lists::maps(&j.tables_lists.rows, &wi, &is_map, &|i| (g.items)(i) > 0, all)
    };
    let open = |w: usize| world_open(j, w, g.flag);
    lists::layout(lists::sorted(entries), &open, (page == 7).then_some(8))
}

/// Builds page 7 or 8.
pub fn build(j: &mut Journal, scene: &mut MenuScene, page: u16, world: i8, g: &Game) {
    category_frame(j, scene, if page == 7 { 3 } else { 4 });
    let list = j.seq_base + SEQ_LIST;
    let ln = j.nodes.lists.clone_ids();
    for (id, grp) in [(ln.layers[0], LAYERS[0]), (ln.layers[1], LAYERS[1])] {
        scene.nodes[id].visible = true;
        scene.nodes[id].off = [0.0; 2];
        scene.set_group(id, grp);
    }
    // The static objects: the info text hangs from the first layer at the layer group's x / y (`FUN_002b0e78`:
    // `FUN_00138ed0(text, FUN_00237618(base + 0x18), FUN_00237638(..))`), the total's count at its label group's x / y.
    let g3b = scene.extent(list, LAYERS[0]);
    let g3c = scene.extent(list, TOTAL);
    scene.nodes[ln.info].visible = true;
    scene.nodes[ln.info].off = g3b;
    scene.set_group(ln.info, j.tables.list_text[0]);
    scene.nodes[ln.total[1]].off = g3c;
    let layout = make_layout(j, page, g);
    let cur = lists::start(&layout, usize::try_from(world).unwrap_or(0).min(lists::WORLDS - 1));
    let track = scene.seqs.get(list).and_then(|s| s.groups.get(LAYERS[1])).map_or(0, |gr| gr.size);
    let body = scene.seqs.get(list).and_then(|s| s.groups.get(j.tables.bar[2])).map_or(1, |gr| gr.size.max(1));
    let bar = Bar::new(track, body, layout.total as i32, lists::VIEW_ROWS as i32);
    info!(
        "journal: page {page} (FUN_002afca0): {} entries ({} new), {} found, {} rows, cursor {cur:?}, section rows {:?}",
        layout.entries.len(),
        layout.entries.iter().filter(|e| j.marks.is_new(cat_of(page), usize::from(e.idx))).count(),
        layout.entries.iter().filter(|e| e.found).count(),
        layout.total,
        layout.sections.iter().map(|s| s.rows).collect::<Vec<_>>()
    );
    let o = j.open.as_mut().unwrap();
    (o.page, o.world, o.bar, o.bar_group) = (page, world, Some(bar), LAYERS[1]);
    o.lists = Some((layout, cur));
    place_bar(j, scene, ln.layers[1], LAYERS[1]);
    seen_world(j, page);
    redraw(j, scene, g.flag, true);
}

/// `FUN_002b0df8` / `FUN_002b8a98`: the entries of the world under the cursor count as seen (the ones with a new bit).
fn seen_world(j: &mut Journal, page: u16) {
    let Some((l, c)) = j.open.as_ref().and_then(|o| o.lists.as_ref()) else { return };
    let w = l.world_of_row(c.top + c.at / lists::COLS);
    let Some(s) = l.sections.get(w) else { return };
    let cat = cat_of(page);
    let keys: Vec<usize> = l.entries[s.first..s.first + s.count].iter().map(|e| usize::from(e.idx)).collect();
    for k in keys {
        if j.marks.is_new(cat, k) {
            j.marks.see(cat, k);
        }
    }
}

/// The text of a row's label (`FUN_002b0678`): the world's name, or question marks while the world is closed.
fn label_text(j: &Journal, scene: &MenuScene, w: usize, open: bool, test: &dyn Fn(u16) -> bool) -> Option<Vec<Token>> {
    let rec = j.worlds.iter().find(|r| usize::try_from(r.id).is_ok_and(|id| id == w))?;
    let id = if rec.flag2 != 0 && test(rec.flag2) { rec.names[3] } else { rec.names[1] };
    let t: Vec<Token> = tokens(j, scene, id)?.into_iter().filter(|t| *t != Token::End).collect();
    Some(if open { t } else { masked(&t) })
}

/// `FUN_0029bb48`: a question mark for each glyph or icon of a text (up to its first line break).
pub fn masked(t: &[Token]) -> Vec<Token> {
    let n = t.iter().take_while(|t| **t != Token::Newline).filter(|t| matches!(t, Token::Glyph(_) | Token::Icon(_))).count();
    // The question mark is glyph 41 (the sprintf of `FUN_001918a0` turns the ASCII question mark into the byte 0x49).
    (0..n).map(|_| Token::Glyph(41)).collect()
}

/// The group colour of a node's sequence.
fn node_group_color(scene: &MenuScene, node: usize, group: usize) -> [u8; 4] {
    scene.nodes[node].seq.map_or(DEFAULT_TEXT, |s| group_color(scene, s, group))
}

/// A label of `font` from `tokens` (the page's text objects: group offset off, `FUN_00139888`).
fn plain_label(scene: &MenuScene, node: usize, group: usize, tokens: Vec<Token>, font: usize, align: u8) -> Option<Label> {
    let g = scene.nodes[node].seq.and_then(|s| scene.seqs.get(s)).and_then(|s| s.groups.get(group)).copied().unwrap_or_default();
    let size = if g.size > 0 { g.size as f32 } else { TEXT_SIZE };
    let world = scene.text.as_ref()?.world;
    // Flag 8 is cleared on these objects (`FUN_00139860`): the colour is the group's.
    let color = node_group_color(scene, node, group);
    Some(Label { tokens, style: Style { font, size: Some(size), align, world, color, ..Style::default() }, offset: [0.0; 2] })
}

/// `FUN_002b0678` + `FUN_002b09e8` + `FUN_002b0cc8`: the rows in view, the cells, the labels with their tags, the thumb, the
/// cursor frame and the info text. `build` is whether the page was just made.
pub fn redraw(j: &mut Journal, scene: &mut MenuScene, test: &dyn Fn(u16) -> bool, _build: bool) {
    let Some(o) = j.open.as_ref() else { return };
    let (page, Some((layout, cur))) = (o.page, o.lists.clone()) else { return };
    let cat = cat_of(page);
    let ln = j.nodes.lists.clone_ids();
    let list = j.seq_base + SEQ_LIST;
    let cell_groups = if page == 7 { CELL_TREASURE } else { CELL_MAP };
    let (pitch_x, pitch_y) = scene.seqs.get(list).and_then(|s| s.groups.get(cell_groups[0])).map_or((28, 28), |gr| (gr.unk20, gr.unk1c));
    let (label_dy, label_group) = scene.seqs.get(list).and_then(|s| s.groups.get(LABEL)).map_or((28, [200, 0]), |gr| (gr.unk1c, [gr.x, gr.y]));
    let rows = lists::view(&layout, cur.top);
    for r in 0..lists::VIEW_ROWS {
        let row = rows.get(r);
        for c in 0..lists::COLS {
            let id = ln.cells[r * lists::COLS + c];
            scene.nodes[id].visible = true;
            scene.nodes[id].off = [(c as i32 * pitch_x) as f32, (r as i32 * pitch_y) as f32];
            let grp = match row.and_then(|v| v.cells.get(c)) {
                Some(true) => cell_groups[0],
                Some(false) => cell_groups[1],
                None => STORY_ROW,
            };
            scene.set_group(id, grp);
        }
        let [label, nothing, tag] = ln.labels[r];
        scene.nodes[label].visible = true;
        scene.nodes[label].off = [0.0, (r as i32 * label_dy) as f32];
        scene.nodes[label].label = None;
        scene.set_group(label, STORY_ROW);
        scene.nodes[nothing].visible = true;
        scene.nodes[nothing].off = [label_group[0] as f32, label_group[1] as f32];
        scene.nodes[nothing].label = None;
        scene.set_group(nothing, STORY_ROW);
        scene.nodes[tag].visible = true;
        scene.set_group(tag, STORY_ROW);
        let Some(w) = row.and_then(|v| v.label) else { continue };
        let open = world_open(j, w, test);
        scene.set_group(label, LABEL);
        let t = label_text(j, scene, w, open, test);
        scene.nodes[label].label = t.and_then(|t| plain_label(scene, label, LABEL, t, LABEL_FONT, 0));
        scene.nodes[label].label_anchor = Some([0.0; 2]);
        let s = layout.sections[w];
        if s.count == 0 {
            // The empty section: "There's nothing yet!" at the label's group offset (`+0x8658`: 0x4a0f / 0x4a12).
            let id = if page == 7 { msg::NOTHING_TREASURE } else { msg::NOTHING_MAP };
            let t = tokens(j, scene, id).map(|t| t.into_iter().filter(|t| *t != Token::End).collect::<Vec<_>>());
            scene.nodes[nothing].label = t.and_then(|t| plain_label(scene, nothing, STORY_ROW, t, LABEL_FONT, 0));
            scene.nodes[nothing].label_anchor = Some([0.0; 2]);
        }
        // The new tag of the section's first row (`FUN_002b0678`: world open and something in it is new).
        if open {
            let keys = layout.entries[s.first..s.first + s.count].iter().map(|e| (cat, usize::from(e.idx)));
            let agg = j.marks.aggregate(keys);
            if let Some(all_seen) = agg {
                scene.set_group(tag, LABEL_TAG[usize::from(all_seen)]);
            }
        }
    }
    // The thumb (`FUN_00299c10`) and the cursor frame (`FUN_002b1890`: group 0x41, 0x42 on a world's empty row).
    bar_pos(j, scene, cur.top);
    let (cr, cc) = (cur.at / lists::COLS, cur.at % lists::COLS);
    let frame = ln.frame;
    scene.nodes[frame].visible = true;
    scene.nodes[frame].off = [(cc as i32 * pitch_x) as f32, (cr as i32 * pitch_y) as f32];
    let empty = layout.entry_at(cur.top, cur.at).is_none();
    if scene.nodes[frame].player.group != Some(FRAME[usize::from(empty)]) {
        scene.set_group(frame, FRAME[usize::from(empty)]);
    }
    // The info text under the cursor (`FUN_002b0cc8`): the item's name, with its icon on page 7, or question marks.
    let label = layout.entry_at(cur.top, cur.at).and_then(|i| {
        let e = layout.entries[i];
        let it = j.tables_lists.items.get(&e.item)?;
        let name: Vec<Token> = tokens(j, scene, it.name)?.into_iter().filter(|t| *t != Token::End).collect();
        let t = if !e.found {
            masked(&name)
        } else if page == 7 {
            let mut v = vec![Token::Icon(it.icon)];
            v.extend(name);
            v
        } else {
            name
        };
        plain_label(scene, ln.info, j.tables.list_text[0], t, 1, 2)
    });
    scene.nodes[ln.info].label = label;
    scene.nodes[ln.info].label_anchor = Some([0.0; 2]);
    // The total (`FUN_002b0e78`, only with the all-worlds flag): found / listed.
    let on = test(ALL_WORLDS_FLAG);
    for id in ln.total {
        scene.nodes[id].visible = on;
        scene.nodes[id].label = None;
    }
    if on {
        let found = layout.entries.iter().filter(|e| e.found).count();
        let id = if page == 7 { msg::TOTAL_TREASURE } else { msg::TOTAL_MAP };
        scene.set_group(ln.total[0], TOTAL);
        scene.nodes[ln.total[0]].label = tokens(j, scene, id).and_then(|t| plain_label(scene, ln.total[0], TOTAL, t, 1, 0));
        scene.nodes[ln.total[0]].label_anchor = Some([0.0; 2]);
        let text = format!("{found}/{}", layout.entries.len());
        // The digits and the slash as `FUN_001918a0` writes them: the digits are glyphs 1 to 10, the slash glyph 43.
        let toks: Vec<Token> = text.bytes().map(|b| if b == b'/' { Token::Glyph(43) } else { Token::Glyph(u16::from(b - b'0') + 1) }).collect();
        scene.set_group(ln.total[1], j.tables.list_text[2]);
        scene.nodes[ln.total[1]].label = plain_label(scene, ln.total[1], j.tables.list_text[2], toks, 1, 1);
        scene.nodes[ln.total[1]].label_anchor = Some([0.0; 2]);
    }
}

/// One press on the pages: `FUN_002b1890`.
pub fn navigate(j: &mut Journal, scene: &mut MenuScene, nav: Nav, test: &dyn Fn(u16) -> bool) -> bool {
    let Some(o) = j.open.as_mut() else { return false };
    let page = o.page;
    let Some((layout, cur)) = o.lists.as_mut() else { return false };
    let to = lists::navigate(layout, *cur, nav, true);
    if to == *cur {
        return false;
    }
    *cur = to;
    seen_world(j, page);
    redraw(j, scene, test, false);
    true
}

/// The keys of the chests of a world (the argument of `FUN_002b88b0` / `FUN_002b8990`): every chest row with a flag, of the
/// world (`None`: every world).
pub fn treasure_keys(rows: &[Row], world_index: &dyn Fn(u8) -> i8, world: Option<i8>) -> Vec<(Cat, usize)> {
    rows.iter()
        .enumerate()
        .filter(|(_, r)| r.kind == 0 && r.flag != 0 && r.flag != 0x800 && world.is_none_or(|w| world_index(r.world) == w))
        .map(|(i, _)| (Cat::Treasure, i))
        .collect()
}

/// The keys of a world's map rows (`FUN_002b8c58` / `FUN_002b8d38`).
pub fn map_keys(rows: &[Row], world_index: &dyn Fn(u8) -> i8, is_map: &dyn Fn(u16) -> bool, world: Option<i8>) -> Vec<(Cat, usize)> {
    rows.iter().enumerate().filter(|(_, r)| is_map(r.item) && world.is_none_or(|w| world_index(r.world) == w)).map(|(i, _)| (Cat::Map, i)).collect()
}

/// `FUN_002ba6d8`: the treasures of a world are complete when every chest of it has been opened [inferred from the bit set
/// the completion buffer takes at +0xd0: the app has no opened chests, so only a world without chests is complete].
pub fn treasures_complete(rows: &[Row], world_index: &dyn Fn(u8) -> i8, world: i8) -> bool {
    treasure_keys(rows, world_index, Some(world)).is_empty()
}

/// `FUN_002ba898`: the maps of a world are complete when it has all of them (the bits at +0x110).
pub fn maps_complete(rows: &[Row], world_index: &dyn Fn(u8) -> i8, is_map: &dyn Fn(u16) -> bool, owned: &dyn Fn(u16) -> bool, world: i8) -> bool {
    rows.iter().filter(|r| is_map(r.item) && world_index(r.world) == world).all(|r| owned(r.item))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_mask_is_a_question_mark_per_glyph() {
        let t = vec![Token::Glyph(14), Token::Space, Token::Icon(3), Token::Glyph(15), Token::Newline, Token::Glyph(16)];
        assert_eq!(masked(&t), vec![Token::Glyph(41); 3]);
        assert!(masked(&[]).is_empty());
    }

    #[test]
    fn the_sets_of_a_world() {
        let rows = [
            Row { kind: 0, world: 2, flag: 5, item: 10, chest: 1 },
            Row { kind: 0, world: 2, flag: 0, item: 11, chest: 2 },
            Row { kind: 1, world: 2, flag: 5, item: 12, chest: 3 },
            Row { kind: 0, world: 4, flag: 6, item: 13, chest: 4 },
        ];
        let wi = |w: u8| if w == 2 { 0 } else { 1 };
        assert_eq!(treasure_keys(&rows, &wi, Some(0)), vec![(Cat::Treasure, 0)]);
        assert_eq!(treasure_keys(&rows, &wi, None).len(), 2);
        assert!(!treasures_complete(&rows, &wi, 0), "a chest that cannot be opened in the app");
        assert!(treasures_complete(&rows, &wi, 5), "a world without chests");
        let is_map = |i: u16| i == 12 || i == 13;
        assert_eq!(map_keys(&rows, &wi, &is_map, Some(0)), vec![(Cat::Map, 2)]);
        assert!(maps_complete(&rows, &wi, &is_map, &|i| i == 12, 0));
        assert!(!maps_complete(&rows, &wi, &is_map, &|i| i == 12, 1));
        let pres = treasures_present(&rows, &wi, &|f| f == 5, &|w| w == 0);
        assert_eq!(pres, vec![(Cat::Treasure, 0)]);
        assert_eq!(maps_present(&rows, &is_map, &|i| i == 13), vec![(Cat::Map, 3)]);
    }
}
