//! The Ansem report page (0x18) and the limit page (0x1e) of Jiminy's Journal (docs/systems/ui/menus-journal-pages.md 20.11, WP-MN15): `FUN_002acfb8` builds it, `FUN_002ad3d0` is its
//! handler, `FUN_002adcb0` draws the text (nine lines from the scroll position, as a character's description shows seven).
//!
//! The report is a record of `jiminy.bar` entry 4 (+0 the item that makes it exist, +2 the title message, +4 the text message). The
//! page shows the title centred and the text with a scroll bar; L2 / R2 go to the previous / next report that is owned.

use super::*;

/// The page's literal groups of the list sequence (`FUN_002acfb8`): the layers 0x5f and 0x62 (the second holds the scroll bar's
/// track) and the title's 0x60.
pub const LAYERS: [usize; 2] = [0x5f, 0x62];
pub const TITLE: usize = 0x60;
/// The limit page's title group (`FUN_002b2878`: 0x61, with the count at its right) and help line (`FUN_002b3878`).
pub const LIMIT_TITLE: usize = 0x61;
pub const HELP_LIMIT: u16 = 0x4b00;
/// The lines in view (the scissor of `FUN_002adcb0`: nine line heights) and the help line (`FUN_002ade50`).
pub const VIEW: usize = 9;
pub const HELP: u16 = 0x48ed;
/// The text's font and ink (`+0x1a0 = 3`, `+0x198 = 0x60000008`), as the character page's description has them.
const FONT: usize = 3;
const INK: [u8; 4] = [0x08, 0x00, 0x00, 0x60];

/// The list count of the text (`FUN_002acfb8`: the lines minus eight when there are more than nine, else one).
pub fn positions(lines: usize) -> usize {
    if lines > VIEW { lines - (VIEW - 1) } else { 1 }
}

/// The text's scroll after a press: Up / Down one line, L1 / R1 a page of nine (`FUN_00299010(list, 9)`), clamped to the positions
/// [inferred: as the character page's description].
pub fn scroll(top: usize, count: usize, dy: i32, page: i32) -> usize {
    let last = count.saturating_sub(1) as i64;
    (top as i64 + i64::from(dy.signum()) + i64::from(page.signum()) * VIEW as i64).clamp(0, last) as usize
}

/// Builds page 0x18 (an Ansem report) or 0x1e (a limit).
pub fn build(j: &mut Journal, scene: &mut MenuScene, page: u16, world: i8) {
    let limit = page == 0x1e;
    let Some(o) = j.open.as_ref() else { return };
    let (list_ids, cur) = (o.rlist.clone(), o.rcur);
    let recs = if limit { &j.limit_recs } else { &j.report_recs };
    let Some(rec) = list_ids.get(cur).and_then(|&i| recs.get(i)).copied() else { return };
    category_tab(j, scene, j.tables.c_text[if limit { 8 } else { 0 }]);
    let list = j.seq_base + SEQ_LIST;
    let (lay, story_row) = (j.nodes.story_lay.clone(), j.nodes.story_row);
    for (k, &id) in lay.iter().enumerate() {
        scene.nodes[id].visible = true;
        scene.nodes[id].off = [0.0; 2];
        scene.set_group(id, LAYERS[k]);
    }
    let g = scene.seqs.get(list).and_then(|s| s.groups.get(LAYERS[0])).copied().unwrap_or_default();
    let size = if g.size > 0 { g.size } else { TEXT_SIZE as i32 };
    let font_gap = kh2_image::font::Font::get(FONT).line_gap as i32;
    let (_, gap) = story_metrics(g.unk1c, size, font_gap);
    let mut lines: Vec<Vec<Token>> = vec![Vec::new()];
    for tok in tokens(j, scene, rec.text).unwrap_or_default().into_iter().filter(|t| *t != Token::End) {
        if tok == Token::Newline {
            lines.push(Vec::new());
        } else if let Some(l) = lines.last_mut() {
            l.push(tok);
        }
    }
    let count = positions(lines.len());
    scene.nodes[story_row].visible = true;
    scene.nodes[story_row].off = [0.0; 2];
    scene.set_group(story_row, STORY_ROW);
    // The title (`FUN_002acfb8`: group 0x60, font 3, centred; the limit page's group 0x61 sets no alignment: left).
    let title = j.nodes.report_title;
    let title_group = if limit { LIMIT_TITLE } else { TITLE };
    scene.nodes[title].visible = true;
    scene.nodes[title].off = [0.0; 2];
    scene.set_group(title, title_group);
    let tg = scene.seqs.get(list).and_then(|s| s.groups.get(title_group)).copied().unwrap_or_default();
    let tsize = if tg.size > 0 { tg.size as f32 } else { TEXT_SIZE };
    scene.nodes[title].label = tokens(j, scene, rec.title).and_then(|t| {
        let world = scene.text.as_ref()?.world;
        Some(Label { tokens: t.into_iter().filter(|t| *t != Token::End).collect(), style: Style { font: FONT, size: Some(tsize), align: if limit { 0 } else { 2 }, world, color: INK, ..Style::default() }, offset: [0.0; 2] })
    });
    scene.nodes[title].label_anchor = Some([0.0; 2]);
    // The limit page's count: a child of the title at its group's x / y, group 2, font 3, right aligned (`FUN_002b2878`).
    let value = j.nodes.report_value;
    scene.nodes[value].visible = limit;
    if limit {
        let unit: Vec<Token> = tokens(j, scene, name_list::LIMIT_UNIT).map(|t| t.into_iter().filter(|t| *t != Token::End).collect()).unwrap_or_default();
        let mut v = count_tokens(&j.tables.limit_fmt, u32::from(j.limit_best(list_ids[cur])));
        v.extend(unit);
        scene.nodes[value].off = [tg.x as f32, tg.y as f32];
        scene.set_group(value, STORY_ROW);
        scene.nodes[value].label = games_page::ink(scene, value, STORY_ROW, v, 1);
        scene.nodes[value].label_anchor = Some([0.0; 2]);
    }
    // The scroll bar: total = the lines, visible nine, the track on the second layer.
    let track = scene.seqs.get(list).and_then(|s| s.groups.get(LAYERS[1])).copied().unwrap_or_default();
    let body = scene.seqs.get(list).and_then(|s| s.groups.get(j.tables.bar[2])).map_or(1, |g| g.size.max(1));
    let bar = Bar::new(track.size, body, lines.len() as i32, VIEW as i32);
    info!(
        "journal: page {page:#x} (FUN_002acfb8 / FUN_002b2878): entry {} of {} (item {:#x}, title {:#x}, text {:#x}), {} lines, {count} positions, line height {}, gap {gap}",
        list_ids[cur],
        list_ids.len(),
        rec.item,
        rec.title,
        rec.text,
        lines.len(),
        g.unk1c
    );
    // The entry counts as seen (`FUN_002ade80`, `FUN_002b38a8`: the entry under the cursor of the list page is the one shown).
    let cat = if limit { Cat::Limit } else { Cat::Report };
    if j.marks.is_new(cat, list_ids[cur]) {
        j.marks.see(cat, list_ids[cur]);
    }
    let o = j.open.as_mut().unwrap();
    (o.page, o.world, o.top, o.count, o.lines, o.gap, o.size, o.bar) = (page, world, 0, count, lines, gap.min(255) as u8, size as f32, Some(bar));
    place_bar(j, scene, lay[1], LAYERS[1]);
    view(j, scene);
}

/// The nine lines from the scroll position and the thumb (`FUN_002adcb0`, `FUN_00299c10`).
pub fn view(j: &mut Journal, scene: &mut MenuScene) {
    let Some(o) = j.open.as_ref() else { return };
    let mut t = vec![Token::Gap(o.gap)];
    for (k, l) in o.lines.iter().skip(o.top).take(VIEW).enumerate() {
        if k > 0 {
            t.push(Token::Newline);
        }
        t.extend(l.iter().cloned());
    }
    t.push(Token::End);
    let world = scene.text.as_ref().map_or(0, |x| x.world);
    let style = Style { font: FONT, size: Some(o.size), align: 0, world, color: INK, ..Style::default() };
    let (row, top) = (j.nodes.story_row, o.top);
    scene.nodes[row].label = Some(Label { tokens: t, style, offset: [0.0; 2] });
    bar_pos(j, scene, top);
}

/// A key on page 0x18 (`FUN_002ad3d0`): Up / Down and L1 / R1 scroll the text, L2 / R2 (-11 / -10, when more than one report is owned)
/// change the report, -2 goes back to the list, -4 ends the scene.
pub fn input(j: &mut Journal, scene: &mut MenuScene, menu: &mut Menu, se: &mut MessageWriter<PlaySe>, key: Key, items: &dyn Fn(u16) -> i32) {
    let Some(o) = j.open.as_ref() else { return };
    let (top, count, wb, page) = (o.top, o.count, o.world, o.page);
    let (list_page, limit) = (if page == 0x1e { 0x1d } else { 0x17 }, page == 0x1e);
    let owned = |id: usize| if limit { j.limit_used(id) } else { j.report_recs.get(id).is_some_and(|r| items(r.item) > 0) };
    let owned_ids: std::collections::HashSet<usize> = o.rlist.iter().copied().filter(|&i| owned(i)).collect();
    let many = owned_ids.len() > 1;
    let scrolled = |to: usize, j: &mut Journal, scene: &mut MenuScene, se: &mut MessageWriter<PlaySe>| {
        if to != top {
            j.open.as_mut().unwrap().top = to;
            view(j, scene);
            menu_sound::play(se, menu_sound::CURSOR);
            info!("journal: page {page:#x} first line {to}");
        }
    };
    match key {
        Key::Start => {
            menu_sound::play(se, menu_sound::CANCEL);
            close(j, scene, menu);
        }
        Key::Cancel => {
            menu_sound::play(se, menu_sound::CANCEL);
            info!("journal: page {page:#x} cancel -> page {list_page:#x} (FUN_002ad3d0 / FUN_002b2d78 case -2)");
            j.goto = Some((list_page, wb));
        }
        Key::Move(_, dy) if dy != 0 => scrolled(scroll(top, count, dy, 0), j, scene, se),
        Key::L1 => scrolled(scroll(top, count, 0, -1), j, scene, se),
        Key::R1 => scrolled(scroll(top, count, 0, 1), j, scene, se),
        Key::L2 | Key::R2 if many => {
            let o = j.open.as_mut().unwrap();
            let to = chars::neighbour(&o.rlist, o.rcur, &|i| owned_ids.contains(&i), key == Key::R2);
            if to != o.rcur {
                o.rcur = to;
                // `FUN_002ad998` stores the new report as the list page's cursor.
                o.kept.retain(|k| k.0 != list_page);
                o.kept.push((list_page, enemies::Pos { cursor: to.min(9), scroll: to.saturating_sub(9) }));
                menu_sound::play(se, menu_sound::CURSOR);
                info!("journal: page {page:#x} {} (FUN_002ad998) -> entry {}", if key == Key::R2 { "R2 (-10)" } else { "L2 (-11)" }, o.rlist[to]);
                j.goto = Some((page, wb));
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_text_scrolls_by_lines_and_pages_inside_its_positions() {
        assert_eq!((positions(5), positions(9), positions(10), positions(30)), (1, 1, 2, 22));
        assert_eq!(scroll(0, 22, 1, 0), 1);
        assert_eq!(scroll(0, 22, -1, 0), 0, "no wrap above the first");
        assert_eq!(scroll(0, 22, 0, 1), 9);
        assert_eq!(scroll(20, 22, 0, 1), 21, "the last position");
        assert_eq!(scroll(5, 22, 0, -1), 0);
        assert_eq!(scroll(0, 1, 1, 0), 0, "a text that fits does not scroll");
    }
}
