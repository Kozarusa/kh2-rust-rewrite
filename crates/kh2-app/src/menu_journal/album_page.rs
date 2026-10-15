//! The Album page (4) of Jiminy's Journal (docs/menus.md 20.9, WP-MN12): a list of the world's pictures, `FUN_002a5a68`.
//!
//! `jiminy.bar` entry 2 ("albu", 0xc-byte records) names each picture: +0 the journal world, +1 / +2 two ASCII digits that
//! end the picture file's name (`jm_photo/<world code><digits>.bin`, read by the viewer page 5, not ported), +6 the story flag
//! that makes it exist, +8 its title and +0xa its caption (messages of `msg/us/jm.bar`). The list shows the titles of the
//! world's pictures that exist (or all of them behind question marks with the all-worlds flag); a row has the new tag.

use super::*;

/// The page's literal groups (`FUN_002a5a68`, `FUN_002a6e78`): the layer, and the row tags (new, new and seen: the words at
/// 0x382bdc / 0x382be0).
pub const LAYER: usize = 0x58;
pub const TAG: [usize; 2] = [0x5a, 0x5b];
/// Rows the list has (`FUN_002a5a68` makes eight list objects).
pub const ROWS: usize = 8;
/// "There's nothing yet!" (`FUN_002a5a68` with no picture) and the help line (`FUN_002a6e48`).
const NOTHING: u16 = 0x32dd;
pub const HELP: u16 = 0x48d8;
const ROW_FONT: usize = 3;

/// One record of entry 2.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rec {
    pub world: i8,
    /// The two digits of the picture file's name.
    pub photo: [u8; 2],
    pub flag: u16,
    pub title: u16,
    pub caption: u16,
}

/// Parses entry 2.
pub fn parse(d: &[u8]) -> Vec<Rec> {
    records(d, 0xc)
        .iter()
        .map(|r| Rec {
            world: r[0] as i8,
            photo: [r[1], r[2]],
            flag: u16::from_le_bytes([r[6], r[7]]),
            title: u16::from_le_bytes([r[8], r[9]]),
            caption: u16::from_le_bytes([r[10], r[11]]),
        })
        .collect()
}

/// `FUN_002a5ef0`: the records the list shows for `world`: those whose flag is set, or all of them with the all-worlds flag.
pub fn entries(recs: &[Rec], world: i8, flag: &dyn Fn(u16) -> bool, all: bool) -> Vec<usize> {
    recs.iter().enumerate().filter(|(_, r)| r.world == world && (flag(r.flag) || all)).map(|(i, _)| i).collect()
}

/// `FUN_002b7818`: the records that exist now, for the new bits.
pub fn present(recs: &[Rec], flag: &dyn Fn(u16) -> bool) -> Vec<(Cat, usize)> {
    recs.iter().enumerate().filter(|(_, r)| flag(r.flag)).map(|(i, _)| (Cat::Album, i)).collect()
}

/// The keys of a world's records (`FUN_002b79d8` / `FUN_002b7af8` with the index -1); `None`: every world.
pub fn keys(recs: &[Rec], world: Option<i8>) -> Vec<(Cat, usize)> {
    recs.iter().enumerate().filter(|(_, r)| world.is_none_or(|w| r.world == w)).map(|(i, _)| (Cat::Album, i)).collect()
}

/// `FUN_002ba080`: the album of a world is complete when every picture of it exists.
pub fn complete(recs: &[Rec], world: i8, flag: &dyn Fn(u16) -> bool) -> bool {
    recs.iter().filter(|r| r.world == world).all(|r| flag(r.flag))
}

/// Builds page 4.
pub fn build(j: &mut Journal, scene: &mut MenuScene, world: i8, test: &dyn Fn(u16) -> bool) {
    category_frame(j, scene, 2);
    let list = j.seq_base + SEQ_LIST;
    let (layer, rows, tags) = (j.nodes.album.layer, j.nodes.album.rows, j.nodes.album.tags);
    scene.nodes[layer].visible = true;
    scene.nodes[layer].off = [0.0; 2];
    scene.set_group(layer, LAYER);
    let pitch = scene.seqs.get(list).and_then(|s| s.groups.get(LAYER)).map_or(26, |g| g.unk1c);
    let entries = entries(&j.album, world, test, test(ALL_WORLDS_FLAG));
    // The list has one row of "nothing yet" when no picture exists (`FUN_002a5a68`: `FUN_00190c40(0x32dd)`).
    let shown = entries.len().max(1);
    for k in 0..ROWS {
        let on = k < shown;
        scene.nodes[rows[k]].visible = on;
        scene.nodes[tags[k]].visible = on;
        if !on {
            continue;
        }
        scene.nodes[rows[k]].off = [0.0, (k as i32 * pitch) as f32];
        scene.set_group(rows[k], STORY_ROW);
        scene.set_group(tags[k], STORY_ROW);
        scene.nodes[rows[k]].label_anchor = Some([0.0; 2]);
        let toks = match entries.get(k) {
            None => tokens(j, scene, NOTHING),
            Some(&i) => {
                let r = j.album[i];
                tokens(j, scene, r.title).map(|t| {
                    let t: Vec<Token> = t.into_iter().filter(|t| *t != Token::End).collect();
                    // A picture that does not exist yet shows question marks (`FUN_0029bb48`).
                    if test(r.flag) { t } else { treasure_page::masked(&t) }
                })
            }
        };
        scene.nodes[rows[k]].label = toks.and_then(|t| {
            let t: Vec<Token> = t.into_iter().filter(|t| *t != Token::End).collect();
            let g = scene.seqs.get(list).and_then(|s| s.groups.get(STORY_ROW)).copied().unwrap_or_default();
            let size = if g.size > 0 { g.size as f32 } else { TEXT_SIZE };
            // Flag 8 is cleared on the rows (`FUN_00139860`): the group's colour.
            let color = group_color(scene, list, STORY_ROW);
            Some(Label { tokens: t, style: Style { font: ROW_FONT, size: Some(size), world: scene.text.as_ref()?.world, color, ..Style::default() }, offset: [0.0; 2] })
        });
        // The new tag (`FUN_002b79d8` / `FUN_002b7af8`): only for a picture that exists.
        if let Some(&i) = entries.get(k)
            && test(j.album[i].flag)
            && let Some(t) = j.marks.tag(Cat::Album, i)
        {
            scene.set_group(tags[k], TAG[usize::from(t == Tag::Seen)]);
        }
    }
    info!("journal: page 4 (FUN_002a5a68): world {world}, {} pictures {:?}", entries.len(), entries);
    let o = j.open.as_mut().unwrap();
    // The row the picture page kept for the way back (`FUN_00277de8(4)`, saved by `FUN_00277dc8` on page 5's cancel), else the
    // first. Page 2's own row (`back_row`) stays for the way back from here.
    let row = if o.back_album < entries.len() { o.back_album } else { 0 };
    (o.page, o.world, o.row, o.album_rows, o.back_album) = (4, world, row, entries, 0);
    seen_row(j, scene);
}

/// `FUN_002a6e78`: the picture under the cursor counts as seen (when it exists and has a new bit): its tag goes to the seen
/// group.
pub fn seen_row(j: &mut Journal, scene: &mut MenuScene) {
    let Some(o) = j.open.as_ref() else { return };
    let Some(&i) = o.album_rows.get(o.row) else { return };
    let (row, tags) = (o.row, j.nodes.album.tags);
    if j.marks.is_new(Cat::Album, i) {
        j.marks.see(Cat::Album, i);
        scene.set_group(tags[row], TAG[1]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rec(world: i8, flag: u16) -> Rec {
        Rec { world, photo: *b"11", flag, title: 1, caption: 2 }
    }

    #[test]
    fn the_list_shows_the_pictures_that_exist() {
        let r = [rec(0, 10), rec(0, 11), rec(1, 12), rec(0, 13)];
        let flag = |f: u16| f == 10 || f == 12;
        assert_eq!(entries(&r, 0, &flag, false), vec![0]);
        assert_eq!(entries(&r, 0, &flag, true), vec![0, 1, 3], "the all-worlds flag shows them all");
        assert_eq!(entries(&r, 2, &flag, false), Vec::<usize>::new());
        assert_eq!(present(&r, &flag), vec![(Cat::Album, 0), (Cat::Album, 2)]);
        assert_eq!(keys(&r, Some(0)), vec![(Cat::Album, 0), (Cat::Album, 1), (Cat::Album, 3)]);
        assert_eq!(keys(&r, None).len(), 4);
        assert!(!complete(&r, 0, &flag));
        assert!(complete(&r, 1, &flag));
        assert!(complete(&r, 5, &flag), "a world without pictures");
    }

    #[test]
    fn a_record_has_two_digits_and_two_texts() {
        let mut d = vec![0u8; 0x10 + 0xc];
        d[8] = 1;
        d[0x10..0x1c].copy_from_slice(&[4, b'2', b'3', 0, 0, 0, 0x14, 0x1c, 0x46, 0x2c, 0x47, 0x2c]);
        let p = parse(&d);
        assert_eq!(p, vec![Rec { world: 4, photo: *b"23", flag: 0x1c14, title: 0x2c46, caption: 0x2c47 }]);
    }
}
