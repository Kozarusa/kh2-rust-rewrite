//! Page 0xc of Jiminy's Journal (docs/menus.md 20.10, WP-MN14): the category list of the journal's own world (id 0xf, the cell
//! page 0 gives an id above 0xe). `FUN_002a7220` builds it, `FUN_002a7978` is its handler, `FUN_002a7558` fills the rows
//! (their texts are the words at 0x382d08, the help lines the words at 0x382d20, the new tags the words at 0x382d00) and
//! `FUN_002a80c0` maps a visible row to its category.
//!
//! The categories are 0 the Ansem reports (page 0x17), 1 (0xd), 2 (0x11), 3 (0x14) the character lists, 4 / 5 (0x19 / 0x1a) the
//! treasures and maps, 6 / 7 (0x1b / 0x1c) the quests and minigames, 8 (0x1d), 9 (0x1f) and 10 (0x20). The reports and the three character
//! lists open the text lists of `name_list` (WP-MN15), 4 to 7 and 10 run the pages the app has under their own numbers; categories
//! 9 and 10 (the moogle summary, the character links) run too: the whole list is ported.

use super::*;

/// The layers (`FUN_002a7220`: groups 0x85 and 0x8f) and the completion mark of a row (`FUN_002a7558`: group 0x86).
pub const LAYERS: [usize; 2] = [0x85, 0x8f];
pub const DONE: usize = 0x86;
/// The categories the page can list (`FUN_002a7220` makes eleven list objects).
pub const CATEGORIES: usize = 11;
/// The page each category opens (`FUN_002a7978`, the switch on `FUN_002a80c0`'s result).
pub const PAGES: [u16; CATEGORIES] = [0x17, 0xd, 0x11, 0x14, 0x19, 0x1a, 0x1b, 0x1c, 0x1d, 0x1f, 0x20];
/// The categories listed (`FUN_002a80c0`): all but the ones that need something the player does not have yet. The Ansem reports
/// need one report owned (`FUN_002ade10`), the quests a listed quest (`FUN_002b6f00`), the minigames a played one (`FUN_002b5bb0`),
/// category 8 a limit that has been used (`FUN_002b37d8`) and category 9 the moogle's
/// first record bit or the flags of the save word at +0x3750 (`FUN_002b4838`; the app has the first only). The all-worlds flag
/// lists every one of them.
pub fn listed(reports: bool, quests: bool, games: bool, limits: bool, moogle: bool, all: bool) -> Vec<usize> {
    (0..CATEGORIES)
        .filter(|&c| match c {
            0 => reports || all,
            6 => quests || all,
            7 => games || all,
            8 => limits || all,
            9 => moogle || all,
            _ => true,
        })
        .collect()
}

/// The page's state machine (`FUN_002a7978`): Up / Down move the row (the list wraps on a new press), a confirm opens the row's
/// page, -2 goes back to page 0, -4 ends the scene; the L1 / R1 page switches are `switch`.
pub fn page_c(key: Key, row: usize, cats: &[usize]) -> Act {
    match key {
        Key::Start => Act::Close,
        Key::Cancel => Act::Back,
        Key::Move(_, 0) => Act::None,
        Key::Move(_, dy) => {
            let n = cats.len() as i32;
            if n < 2 {
                return Act::None;
            }
            Act::Cursor((row as i32 + dy.signum()).rem_euclid(n) as usize)
        }
        Key::Confirm => cats.get(row).map_or(Act::None, |&cat| Act::Category { cat, page: PAGES[cat] }),
        Key::L1 | Key::R1 | Key::L2 | Key::R2 | Key::View => Act::None,
    }
}

/// What a row shows beyond its text: the new tag (`None`: no entry is new, else whether all have been seen) and whether the
/// category is complete (`FUN_002a7558`).
fn marks(j: &Journal, cat: usize, test: &dyn Fn(u16) -> bool, items: &dyn Fn(u16) -> i32) -> (Option<bool>, bool) {
    let exists = name_list::exist_set(j, test, items);
    // The keys of the character records of some families (`FUN_002b7408`) and whether every one of them exists (`FUN_002b9d88`).
    let families = |fs: &[u8]| -> Vec<(Cat, usize)> { j.chars.iter().enumerate().filter(|(_, r)| fs.contains(&r.family)).map(|(i, _)| (Cat::Char, i)).collect() };
    let done = |fs: &[u8]| j.chars.iter().enumerate().filter(|(_, r)| fs.contains(&r.family)).all(|(i, _)| exists.contains(&i));
    let every: Vec<u8> = (0..0xf).chain([LINKS]).collect();
    match cat {
        // The reports: complete when every one is owned (`FUN_002b9f10`).
        0 => (
            j.marks.aggregate((0..j.report_recs.len()).map(|i| (Cat::Report, i))),
            !j.report_recs.is_empty() && j.report_recs.iter().all(|r| items(r.item) > 0),
        ),
        1 => (j.marks.aggregate(families(&every)), done(&every)),
        // The enemy lists (`FUN_002a1c30`'s first loop makes a record with flag 0x800 exist once the enemy has been defeated).
        2 => (j.marks.aggregate(families(&[0x11])), done(&[0x11])),
        3 => (j.marks.aggregate(families(&[0x12])), done(&[0x12])),
        6 => games_page::with_game(j, test, |g| (j.marks.aggregate(games::quest_keys(&j.quests, None)), games::quests_complete(&j.quests, None, g))),
        7 => games_page::with_game(j, test, |g| (j.marks.aggregate(games::game_keys(&j.minis, None)), games::games_complete(&j.minis, None, g))),
        8 => (j.marks.aggregate((0..j.limit_recs.len()).map(|i| (Cat::Limit, i))), (0..j.limit_recs.len()).all(|i| j.limit_used(i))),
        // The moogle (`FUN_002b8fd0`, `FUN_002b9058`, `FUN_002bab48(-1)`): the rows' new bits and every row complete.
        9 => (j.marks.aggregate((0..moogle_page::ROWS).map(|i| (Cat::Moogle, i))), (0..moogle_page::ROWS).all(|i| moogle_page::complete(&j.snap.moogle_data, i))),
        10 => (j.marks.aggregate(face_keys(j, LINKS, test, items)), done(&[LINKS])),
        _ => (None, false),
    }
}

/// Builds page 0xc (`FUN_002a7220`): the two layers, the world's two top pictures, the title, the rows of the listed categories
/// with their marks, and the help line of the cursor row.
#[allow(clippy::too_many_arguments)]
pub fn build(j: &mut Journal, scene: &mut MenuScene, disc: &Disc, images: &mut Assets<Image>, test: &dyn Fn(u16) -> bool, items: &dyn Fn(u16) -> i32) {
    let list = j.seq_base + SEQ_LIST;
    let cats = {
        let (quests, games) = games_page::with_game(j, test, |g| (games::quests_exist(&j.quests, None, g), games::games_exist(&j.minis, None, g)));
        let reports = j.reports.iter().any(|&i| items(i) > 0);
        let limits = (0..j.limit_recs.len()).any(|i| j.limit_used(i));
        listed(reports, quests, games, limits, j.snap.moogle, test(ALL_WORLDS_FLAG))
    };
    let lay = j.nodes.lay.clone();
    for k in 0..2 {
        scene.nodes[lay[k]].visible = true;
        scene.nodes[lay[k]].off = [0.0; 2];
        scene.set_group(lay[k], LAYERS[k]);
        scene.nodes[lay[k]].label = None;
        scene.nodes[lay[k]].label_anchor = None;
    }
    let g0 = scene.seqs.get(list).and_then(|s| s.groups.get(LAYERS[0])).copied().unwrap_or_default();
    // The top pictures (`FUN_002a7d80`: the file of world 0xf, `FUN_002a7ec0(0)`: both in their second group at once).
    let name = j.worlds.iter().find(|w| w.id == 0xf).map_or_else(String::new, |w| w.code.clone());
    if !name.is_empty() && !j.top_tex.contains_key(&name) {
        let t = bar_pictures(disc, &format!("menu/us/jm_top_{name}.2ld"), scene, images).into_iter().flatten().collect();
        j.top_tex.insert(name.clone(), t);
    }
    let tops = j.top_tex.get(&name).cloned().unwrap_or_default();
    for k in 0..2 {
        let id = j.nodes.pics[k];
        let Some(&tex) = tops.get(k) else { continue };
        scene.nodes[id].tex = tex;
        scene.nodes[id].visible = true;
        scene.nodes[id].off = if k == 0 { [g0.x as f32, g0.y as f32] } else { [0.0; 2] };
        scene.set_group(id, j.tables.pic_group[k][1]);
    }
    // The rows hang from layer 0, one pitch apart; both children start in group 3.
    let (rows, kids) = (j.nodes.rows.clone(), j.nodes.row_kids.clone());
    let pitch = g0.unk1c;
    let marks: Vec<(Option<bool>, bool)> = cats.iter().map(|&c| marks(j, c, test, items)).collect();
    for k in 0..ROWS {
        let (r, kid) = (rows[k], kids[k]);
        let on = k < cats.len();
        for id in [r, kid[0], kid[1]] {
            scene.nodes[id].visible = on;
        }
        if !on {
            continue;
        }
        scene.nodes[r].off = [0.0, (k as i32 * pitch) as f32];
        for id in [r, kid[0], kid[1]] {
            scene.set_group(id, ROW_GROUP);
        }
        scene.nodes[r].label = text(j, scene, r, ROW_GROUP, j.tables.c_text[cats[k]], 0, true);
        let (new, done) = marks[k];
        if let Some(all_seen) = new {
            scene.set_group(kid[1], j.tables.c_tag[usize::from(all_seen)]);
        }
        if done {
            scene.set_group(kid[0], DONE);
        }
    }
    // The title is world 0xf's name (`FUN_00277640(FUN_002a3070(0xf, 0))`).
    let title = j.nodes.title;
    let record = j.worlds.iter().find(|w| w.id == 0xf).cloned();
    scene.nodes[title].visible = true;
    scene.set_group_then(title, TITLE[0], TITLE[1]);
    scene.nodes[title].label = record.as_ref().and_then(|w| text(j, scene, title, TITLE[1], w.title(test), 2, true));
    scene.nodes[title].label_anchor = Some([0.0; 2]);
    info!("journal: page 0xc (FUN_002a7220): categories {cats:?}, jm_top_{name}.2ld {} pictures, title world {}", tops.len(), record.map_or(-1, |w| i32::from(w.id)));
    let o = j.open.as_mut().unwrap();
    // The row the confirm kept (`FUN_00277dc8(page, row)`), else the first.
    let row = if o.back_row < cats.len() { o.back_row } else { 0 };
    (o.page, o.world, o.row, o.cats, o.back_row, o.from_c) = (0xc, 0xf, row, cats, 0, false);
}

/// The help line of the cursor row (`FUN_002a81e8`: the word of the row's category at 0x382d20).
pub fn help(j: &Journal) -> Option<u16> {
    let o = j.open.as_ref()?;
    o.cats.get(o.row).map(|&c| j.tables.c_help[c])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_categories_listed_follow_what_the_player_has() {
        // Nothing yet: the character, treasure, map and link rows only.
        assert_eq!(listed(false, false, false, false, false, false), vec![1, 2, 3, 4, 5, 10]);
        assert_eq!(listed(true, true, false, false, false, false), vec![0, 1, 2, 3, 4, 5, 6, 10]);
        assert_eq!(listed(false, false, true, false, true, false), vec![1, 2, 3, 4, 5, 7, 9, 10]);
        assert_eq!(listed(false, false, false, true, false, false), vec![1, 2, 3, 4, 5, 8, 10]);
        assert_eq!(listed(false, false, false, false, false, true), (0..CATEGORIES).collect::<Vec<_>>(), "the all-worlds flag lists them all");
    }

    #[test]
    fn page_c_moves_with_a_wrap_and_opens_the_rows_page() {
        let cats = [1, 6, 7];
        assert_eq!(page_c(Key::Move(0, 1), 2, &cats), Act::Cursor(0));
        assert_eq!(page_c(Key::Move(0, -1), 0, &cats), Act::Cursor(2));
        assert_eq!(page_c(Key::Move(1, 0), 0, &cats), Act::None);
        assert_eq!(page_c(Key::Move(0, 1), 0, &cats[..1]), Act::None, "one row does not move");
        assert_eq!(page_c(Key::Confirm, 1, &cats), Act::Category { cat: 6, page: 0x1b });
        assert_eq!(page_c(Key::Confirm, 2, &cats), Act::Category { cat: 7, page: 0x1c });
        assert_eq!(page_c(Key::Confirm, 0, &cats), Act::Category { cat: 1, page: 0xd });
        assert_eq!(page_c(Key::Cancel, 0, &cats), Act::Back);
        assert_eq!(page_c(Key::Start, 0, &cats), Act::Close);
        assert_eq!(page_c(Key::Confirm, 0, &[]), Act::None);
    }
}
