//! The moogle page (0x1f) of Jiminy's Journal (docs/menus.md 20.11, WP-MN15): `FUN_002b3d80` builds it, `FUN_002b4170` fills the (up to)
//! four rows, `FUN_002b3f10` is its handler. It summarises the synthesis record of the save (`Moogle`, save `+0x3728`): the moogle's level and
//! how many materials, collection rewards and creations the player has.
//!
//! Row 0 "Level N" with the level's name at its right, rows 1 to 3 a count (`%d`, or `%d/%d` with the all-worlds flag). A row is listed when
//! the moogle is available (row 0 and 3: unlock bit 0 or 10, `FUN_002b4798`) or a material has been handed over (rows 1 and 2:
//! `FUN_002b47c8`); with the all-worlds flag an unlisted row shows question marks. A complete row (`FUN_002ba978`) takes the second group.

use super::*;
use crate::progress::Moogle;

/// The page's layer (`FUN_002b3d80`: group 99), the help line (`FUN_002b4778`) and the message of an unlisted row (`FUN_002b4170`).
pub const LAYER: usize = 0x63;
pub const HELP: u16 = 0x48f9;
pub const HIDDEN: u16 = 0x36f8;
/// The rows of the page.
pub const ROWS: usize = 4;
/// The materials the record tracks (`FUN_002b4170`: 0x32 bits), the creations it counts (0x2d bits) and the rewards the completion test needs.
pub const MATERIALS: u32 = 0x32;
pub const CREATIONS: u32 = 0x2d;
/// The unlock bits that make the moogle available (`FUN_002b4798`: bit 0 and bit 10 of the word at `+0x3750`).
const AVAILABLE_BITS: [u32; 2] = [0, 10];
/// The rank names of the highest level (the table at 0x382fc0 has nine; the last level is 8).
pub const TOP_LEVEL: u8 = 8;

/// What the page reads: the synthesis record and the totals the mix file gives (`DAT_0036d4d4` the collection rewards, `DAT_0036d4d5` the
/// creations: each recipe once, twice with a higher rank).
#[derive(Clone, Debug, Default)]
pub struct Data {
    pub rec: Moogle,
    pub rewards_total: u32,
    pub creations_total: u32,
}

/// `FUN_002b4798`: the moogle is available.
pub fn available(m: &Moogle) -> bool {
    AVAILABLE_BITS.iter().any(|&b| Moogle::bit(m.unlocks, b))
}

/// `FUN_002b47c8`: some material has been handed over.
pub fn has_material(m: &Moogle) -> bool {
    m.known != 0
}

/// `FUN_002b4838`: page 0xc lists the moogle row.
pub fn listed(m: &Moogle, all: bool) -> bool {
    available(m) || has_material(m) || all
}

/// Whether row `i` is available (not counting the all-worlds flag): rows 0 and 3 need the moogle, 1 and 2 a material.
pub fn row_on(m: &Moogle, i: usize) -> bool {
    if matches!(i, 0 | 3) { available(m) } else { has_material(m) }
}

/// The bits set among the first `n` of a set.
pub fn count(set: u64, n: u32) -> u32 {
    (0..n).filter(|&b| Moogle::bit(set, b)).count() as u32
}

/// The count and the maximum a row shows (rows 1 to 3).
pub fn counts(d: &Data, i: usize) -> (u32, u32) {
    match i {
        1 => (count(d.rec.known, MATERIALS), MATERIALS),
        2 => (count(d.rec.rewards, MATERIALS), d.rewards_total),
        _ => (count(d.rec.made, CREATIONS), d.creations_total),
    }
}

/// `FUN_002ba978`: row `i` is complete: listed and its count is full (the level 8, 50 materials, 50 rewards, 45 creations).
pub fn complete(d: &Data, i: usize) -> bool {
    row_on(&d.rec, i)
        && match i {
            0 => d.rec.level >= TOP_LEVEL,
            1 => count(d.rec.known, MATERIALS) >= MATERIALS,
            2 => count(d.rec.rewards, MATERIALS) >= MATERIALS,
            _ => count(d.rec.made, 0x40) >= CREATIONS,
        }
}

/// The recipes' creations (`FUN_002b48c8`: one per recipe, two when the recipe has a higher rank).
pub fn creations_total(recipes: &[(u16, u16)]) -> u32 {
    recipes.iter().map(|&(_, upgraded)| 1 + u32::from(upgraded != 0)).sum()
}

/// What a row shows: its group, its label, its value (rows with the moogle available only) and whether its new tag (the seen one) shows.
#[derive(Clone, Debug, Default)]
pub struct Row {
    pub row: usize,
    pub group: usize,
    pub label: Vec<Token>,
    pub value: Option<Vec<Token>>,
}

/// The rows of the page (`FUN_002b4170`).
pub fn rows(j: &Journal, scene: &MenuScene, all: bool) -> Vec<Row> {
    let d = &j.snap.moogle_data;
    let t = &j.tables.moogle;
    let msg = |id: u16| -> Vec<Token> { tokens(j, scene, id).map(|t| t.into_iter().filter(|t| *t != Token::End).collect()).unwrap_or_default() };
    (0..ROWS)
        .filter(|&i| row_on(&d.rec, i) || all)
        .enumerate()
        .map(|(_, i)| {
            let group = t.groups[usize::from(complete(d, i))];
            if !row_on(&d.rec, i) {
                // An unlisted row with the all-worlds flag: question marks and nothing else (`uVar13 = 0x36f8`).
                return Row { row: i, group, label: msg(HIDDEN), value: None };
            }
            if i == 0 {
                let mut label = msg(t.msgs[0]);
                label.extend(fmt_tokens(&t.fmt, &[i64::from(d.rec.level) + 1]));
                let level = msg(t.levels.get(usize::from(d.rec.level)).copied().unwrap_or(0));
                return Row { row: 0, group, label, value: Some(level) };
            }
            let (n, max) = counts(d, i);
            let value = if all { fmt_tokens(&t.fmt_max, &[i64::from(n), i64::from(max)]) } else { fmt_tokens(&t.fmt, &[i64::from(n)]) };
            Row { row: i, group, label: msg(t.msgs[i]), value: Some(value) }
        })
        .collect()
}

/// Builds page 0x1f.
pub fn build(j: &mut Journal, scene: &mut MenuScene, world: i8, test: &dyn Fn(u16) -> bool) {
    category_tab(j, scene, j.tables.c_text[9]);
    let list = j.seq_base + SEQ_LIST;
    let n = j.nodes.games;
    scene.nodes[n.layers[0]].visible = true;
    scene.nodes[n.layers[0]].off = [0.0; 2];
    scene.set_group(n.layers[0], LAYER);
    let all = test(ALL_WORLDS_FLAG);
    let rows = rows(j, scene, all);
    // The rows come out of `FUN_002b4170` one pitch apart (`FUN_00138ed0(row, 0, pitch * k)`), the value at the row group's x / y.
    let gs: Vec<_> = rows.iter().map(|r| scene.seqs.get(list).and_then(|s| s.groups.get(r.group)).copied().unwrap_or_default()).collect();
    for k in 0..games_page::ROWS {
        let r = rows.get(k);
        for id in [n.rows[k], n.tags[k], n.values[k]] {
            scene.nodes[id].visible = r.is_some();
        }
        scene.nodes[n.rows[k]].label = None;
        scene.nodes[n.values[k]].label = None;
        let Some(r) = r else { continue };
        let g = gs[k];
        scene.nodes[n.rows[k]].off = [0.0, (k as i32 * g.unk1c) as f32];
        scene.nodes[n.rows[k]].label_anchor = Some([0.0; 2]);
        scene.set_group(n.rows[k], r.group);
        scene.nodes[n.rows[k]].label = games_page::ink(scene, n.rows[k], r.group, r.label.clone(), 0);
        scene.nodes[n.values[k]].visible = r.value.is_some();
        scene.nodes[n.values[k]].parent = Some(n.rows[k]);
        scene.nodes[n.values[k]].off = [g.x as f32, g.y as f32];
        scene.nodes[n.values[k]].label_anchor = Some([0.0; 2]);
        scene.set_group(n.values[k], STORY_ROW);
        if let Some(v) = &r.value {
            scene.nodes[n.values[k]].label = games_page::ink(scene, n.values[k], STORY_ROW, v.clone(), 1);
        }
        // The tag (`FUN_002b8fd0`): the seen group for a row that was new when the journal opened (`FUN_002b9138` marks them all seen).
        scene.nodes[n.tags[k]].parent = Some(n.values[k]);
        scene.nodes[n.tags[k]].off = [0.0; 2];
        scene.set_group(n.tags[k], STORY_ROW);
        if r.value.is_some() && j.marks.is_new(Cat::Moogle, r.row) {
            scene.set_group(n.tags[k], j.tables.limit_tag[1]);
        }
    }
    // `FUN_002b9138`: every row counts as seen once the page is up.
    for i in 0..ROWS {
        j.marks.see(Cat::Moogle, i);
    }
    info!("journal: page 0x1f (FUN_002b3d80 / FUN_002b4170): {} rows {:?}", rows.len(), rows.iter().map(|r| (r.row, r.group)).collect::<Vec<_>>());
    let o = j.open.as_mut().unwrap();
    (o.page, o.world, o.lrows, o.lpos) = (0x1f, world, Vec::new(), enemies::Pos::default());
}

/// A key on page 0x1f (`FUN_002b3f10`): -2 goes back to page 0xc, -4 ends the scene; nothing else does anything.
pub fn input(j: &mut Journal, scene: &mut MenuScene, menu: &mut Menu, se: &mut MessageWriter<PlaySe>, key: Key) {
    let Some(o) = j.open.as_ref() else { return };
    let wb = o.world;
    match key {
        Key::Start => {
            menu_sound::play(se, menu_sound::CANCEL);
            close(j, scene, menu);
        }
        Key::Cancel => {
            menu_sound::play(se, menu_sound::CANCEL);
            info!("journal: page 0x1f cancel -> page 0xc (FUN_002b3f10 case -2)");
            j.goto = Some((0xc, wb));
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn data(f: impl FnOnce(&mut Moogle)) -> Data {
        let mut d = Data { rewards_total: 50, creations_total: 30, ..Data::default() };
        f(&mut d.rec);
        d
    }

    #[test]
    fn the_rows_follow_the_moogle_and_the_materials() {
        let none = Moogle::default();
        assert!(!listed(&none, false) && listed(&none, true));
        let m = Moogle { unlocks: 1, ..Moogle::default() };
        assert!(available(&m) && listed(&m, false) && !has_material(&m));
        assert_eq!((row_on(&m, 0), row_on(&m, 1), row_on(&m, 2), row_on(&m, 3)), (true, false, false, true));
        let k = Moogle { known: 1, ..Moogle::default() };
        assert_eq!((row_on(&k, 0), row_on(&k, 1), row_on(&k, 2), row_on(&k, 3)), (false, true, true, false));
        let r = Moogle { unlocks: 1 << 10, ..Moogle::default() };
        assert!(available(&r), "the Recipes row's bit makes the moogle available too");
    }

    #[test]
    fn counts_and_completion() {
        let d = data(|m| {
            m.known = (1 << 50) - 1;
            m.rewards = 0b1011;
            m.made = (1 << 45) - 1;
            m.unlocks = 1;
            m.level = 8;
        });
        assert_eq!((counts(&d, 1), counts(&d, 2), counts(&d, 3)), ((50, 50), (3, 50), (45, 30)));
        assert!(complete(&d, 0) && complete(&d, 1) && !complete(&d, 2) && complete(&d, 3));
        let low = data(|m| {
            m.known = 5;
            m.unlocks = 1;
        });
        assert!(!complete(&low, 0) && !complete(&low, 1));
        let off = data(|m| m.level = 8);
        assert!(!complete(&off, 0), "a row that is not listed is not complete");
        assert_eq!(count(u64::MAX, 50), 50);
        assert_eq!(creations_total(&[(1, 0), (2, 7), (3, 0)]), 4);
    }
}
