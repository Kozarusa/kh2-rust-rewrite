//! The Characters page (6) and the Character Links page (0xb) of Jiminy's Journal (docs/menus.md 20.9, WP-MN12).
//!
//! Both pages are one builder in the original (`FUN_002ae3e8`; the page id tells them apart: `FUN_002af398(page != 6,
//! -1)`): a map of faces over the book page, a free pointer the player moves with the pad, the name of the face under the
//! pointer, and a help line. The faces are the sub-records of `jiminy.bar` entry 5 ("diag", 0xc-byte headers) for the
//! journal world (page 6) or for the links family 0x10 (page 0xb); each names a character record of entry 3 ("char", 0x24
//! bytes) and the face's x / y on the page. The pictures come from `jmface_<name>.2ld` (the name table at 0x36d3f8).

/// One face of the page: the character record index (entry 3) and the face's place on the page.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Face {
    pub ch: usize,
    pub x: i32,
    pub y: i32,
}

/// The fields of a character record (entry 3, 0x24 bytes) the page reads.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct CharRec {
    /// +0: the family the record belongs to: a journal world 0..0xe, 0x10 the Character Links, 0x11 the Heartless, 0x12 the Nobodies
    /// (`FUN_002aa528` lists the records of the page's family; `FUN_002ab7d8` names the face file by it).
    pub family: u8,
    /// +1: the picture in the face file (`FUN_002af800`: `FUN_0023c478(list, +1)`).
    pub face: u8,
    /// +2: the frame style 0..5 (`FUN_002ae3e8`: the group table at 0x382f30 by style).
    pub style: u8,
    /// +4: the bit number of the journal's new / seen sets.
    pub bit: u16,
    /// +6: the name's message (`FUN_002af2b0`).
    pub name: u16,
    /// +0x10: flags; 0x400 puts the world's name into the message (`FUN_00191a30`).
    pub flags: u16,
    /// +8: the description's message (`FUN_002aadf0`: the page's scrolled text).
    pub desc: u16,
    /// +0xa: the sub-title's message (0 for none).
    pub sub: u16,
    /// +0xc: the 3D model of the character's page (`FUN_002abf60`); for a record with flag 0x800 it is also the `00objentry` id of the
    /// enemy whose NeoStatus keys the defeated-enemy counter (`FUN_002a1f78`: `FUN_0016e208(+0xc)` -> `+0x4c`).
    pub model: u16,
    /// +0xe: the motion the model plays (`FUN_002abf60` passes it to the preview object's vtable +0x18 with the variant 2).
    pub motion: u16,
    /// The two placements of the model: [0] the small one on the character's own page (offset +0x12, +0x14, turn +0x16, scale +0x1c),
    /// [1] the large one on the model page 0x10 / 0x13 / 0x16 (offset +0x18, +0x1a, scale +0x20, no turn).
    pub place: [ModelPlace; 2],
}

/// Where and how large a record's model stands (`FUN_002abf60`).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ModelPlace {
    /// The slot offsets passed as the preview object's x / y (the small model's +0x12 / +0x14, the large one's +0x18 / +0x1a).
    pub x: i16,
    pub y: i16,
    /// The turn about the vertical axis in degrees (`FUN_0023b3c8`): +0x16 for the small model, 0 for the large one.
    pub turn: i16,
    /// The float at +0x1c / +0x20: the model's scale (`FUN_0023b408`), or for the models of table 0x382e68 its extra depth.
    pub scale: f32,
}

/// The flag of the enemy records: the page shows the counter row and the record exists once the enemy has been defeated.
pub const ENEMY_FLAG: u16 = 0x800;

/// Parses `jiminy.bar` entry 3 ("char").
pub fn parse_chars(d: &[u8]) -> Vec<CharRec> {
    super::records(d, 0x24)
        .iter()
        .map(|r| {
            let u16_at = |o: usize| u16::from_le_bytes([r[o], r[o + 1]]);
            let i16_at = |o: usize| u16_at(o) as i16;
            let f32_at = |o: usize| f32::from_le_bytes([r[o], r[o + 1], r[o + 2], r[o + 3]]);
            CharRec {
                family: r[0],
                face: r[1],
                style: r[2],
                bit: u16_at(4),
                name: u16_at(6),
                flags: u16_at(0x10),
                desc: u16_at(8),
                sub: u16_at(0xa),
                model: u16_at(0xc),
                motion: u16_at(0xe),
                place: [
                    ModelPlace { x: i16_at(0x12), y: i16_at(0x14), turn: i16_at(0x16), scale: f32_at(0x1c) },
                    ModelPlace { x: i16_at(0x18), y: i16_at(0x1a), turn: 0, scale: f32_at(0x20) },
                ],
            }
        })
        .collect()
}

/// One header of entry 5 ("diag"): the flags that switch a group of faces on and off, the family it belongs to and where
/// its sub-records are.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Group {
    /// +0: the story flag (or, with kind 2, the item) that switches the group on.
    pub on: u16,
    /// +2: a story flag that switches it off again.
    pub off: u16,
    /// +4: the family: a journal world 0..0xe, or 0x10 for the Character Links page.
    pub family: u8,
    /// +5: the number of sub-records.
    pub count: u8,
    /// +6: 0 the base group of the family, 1 a group added by a flag, 2 a group added by an item count.
    pub kind: u8,
    /// +8: where the sub-records start in the entry.
    pub at: usize,
}

/// Parses the headers of entry 5.
pub fn parse_groups(d: &[u8]) -> Vec<Group> {
    super::records(d, 0xc)
        .iter()
        .map(|r| Group {
            on: u16::from_le_bytes([r[0], r[1]]),
            off: u16::from_le_bytes([r[2], r[3]]),
            family: r[4],
            count: r[5],
            kind: r[6],
            at: u32::from_le_bytes([r[8], r[9], r[10], r[11]]) as usize,
        })
        .collect()
}

/// `FUN_002a2018`: whether a group is switched on: its flag (kind 2: its item is owned) and not its off flag.
pub fn group_on(g: &Group, flag: &dyn Fn(u16) -> bool, item: &dyn Fn(u16) -> i32) -> bool {
    let on = if g.kind == 2 { item(g.on) > 0 } else { flag(g.on) };
    on && !(g.off != 0 && flag(g.off))
}

/// The sub-records of a group that are drawn (`FUN_002af398`: those whose byte +6 is not 0): (character, x, y).
fn faces_of(d: &[u8], g: &Group) -> Vec<Face> {
    (0..usize::from(g.count))
        .filter_map(|k| d.get(g.at + 8 * k..g.at + 8 * k + 8))
        .filter(|r| r[6] != 0)
        .map(|r| Face { ch: usize::from(u16::from_le_bytes([r[0], r[1]])), x: i32::from(i16::from_le_bytes([r[2], r[3]])), y: i32::from(i16::from_le_bytes([r[4], r[5]])) })
        .collect()
}

/// `FUN_002af398`: the faces of a family: the base group's (when it is on), then every group of the family that is added by
/// a flag or an item and is on, in file order. `d` is the whole entry (the sub-records' offsets are from its start).
pub fn faces(d: &[u8], groups: &[Group], family: u8, flag: &dyn Fn(u16) -> bool, item: &dyn Fn(u16) -> i32) -> Vec<Face> {
    let mut out = Vec::new();
    // The original walks the family's run of headers and stops at the first one that is not of the family after it began.
    let run: Vec<&Group> = {
        let mut began = false;
        groups
            .iter()
            .take_while(|g| {
                if g.family == family {
                    began = true;
                    true
                } else {
                    !began
                }
            })
            .filter(|g| g.family == family)
            .collect()
    };
    // The base group: kind 0. When it is off the walk stops there and the list has nothing from it.
    if let Some(base) = run.iter().find(|g| g.kind == 0)
        && group_on(base, flag, item)
    {
        out.extend(faces_of(d, base));
    }
    for g in run.iter().filter(|g| g.kind != 0) {
        if group_on(g, flag, item) {
            out.extend(faces_of(d, g));
        }
    }
    out
}

/// `FUN_002a2090`: whether a group exists for the journal's bit sets: its flag is set (kind 2: its item is owned). The off
/// flag is not looked at here.
pub fn group_exists(g: &Group, flag: &dyn Fn(u16) -> bool, item: &dyn Fn(u16) -> i32) -> bool {
    if g.kind == 2 { item(g.on) > 0 } else { flag(g.on) }
}

/// The characters that exist now through a face group (`FUN_002a1c30`, its second loop: every sub-record of every group that exists, drawn or
/// not; the first loop's enemy records are `enemies_present`): the character indices.
pub fn present(d: &[u8], groups: &[Group], flag: &dyn Fn(u16) -> bool, item: &dyn Fn(u16) -> i32) -> Vec<usize> {
    let mut out = Vec::new();
    for g in groups.iter().filter(|g| group_exists(g, flag, item)) {
        for k in 0..usize::from(g.count) {
            if let Some(r) = d.get(g.at + 8 * k..g.at + 8 * k + 2) {
                out.push(usize::from(u16::from_le_bytes([r[0], r[1]])));
            }
        }
    }
    out
}

/// The enemy records that exist (`FUN_002a1c30`, its first loop): those with flag 0x800 whose defeated-enemy count (`count` of the
/// record) is not 0. Returns the record indices.
pub fn enemies_present(recs: &[CharRec], count: &dyn Fn(&CharRec) -> u32) -> Vec<usize> {
    recs.iter().enumerate().filter(|(_, r)| r.flags & ENEMY_FLAG != 0 && count(r) > 0).map(|(i, _)| i).collect()
}

/// `FUN_002aa528`: the records of `family` the list page shows: those that exist, or all of them with the all-worlds flag.
pub fn family_list(recs: &[CharRec], family: u8, exists: &dyn Fn(usize) -> bool, all: bool) -> Vec<usize> {
    recs.iter().enumerate().filter(|(i, r)| r.family == family && (exists(*i) || all)).map(|(i, _)| i).collect()
}

/// `FUN_002bad98`: the family is complete when every one of its groups exists (a family without groups counts as complete).
pub fn family_complete(groups: &[Group], family: u8, flag: &dyn Fn(u16) -> bool, item: &dyn Fn(u16) -> i32) -> bool {
    groups.iter().filter(|g| g.family == family).all(|g| group_exists(g, flag, item))
}

/// The face under the pointer (`FUN_002af180`): the first face whose centre is within `half` of the pointer on both axes
/// (inclusive); `half` is half the pitch of the first frame's group (+0x20).
pub fn hit(faces: &[Face], pointer: [i32; 2], half: i32) -> Option<usize> {
    faces.iter().position(|f| (f.x - half..=f.x + half).contains(&pointer[0]) && (f.y - half..=f.y + half).contains(&pointer[1]))
}

/// The pointer's bounds on the page (`FUN_002ae790`, set on the first frame): x0, x1, y0, y1.
pub type Bounds = [i32; 4];

/// A step of the pointer with the pad's direction held, three units a frame (`FUN_002aeea8`: the digital branch; the stick
/// adds `stick * 6 + 0.5`, not taken here). Left beats right and up beats down; each axis stops at its bounds.
pub fn step_pointer(p: [i32; 2], b: Bounds, dx: i32, dy: i32) -> [i32; 2] {
    let mut x = p[0];
    if dx < 0 {
        if b[0] < x {
            x -= 3;
        }
        x = x.max(b[0]);
    } else if dx > 0 {
        if x < b[1] {
            x += 3;
        }
        x = x.min(b[1]);
    }
    let mut y = p[1];
    if dy < 0 {
        if b[2] < y {
            y -= 3;
        }
        y = y.max(b[2]);
    } else if dy > 0 {
        if y < b[3] {
            y += 3;
        }
        y = y.min(b[3]);
    }
    [x, y]
}

/// The pointer's first place (`FUN_002aebb8`): the face the page was left on, else the middle of the bounds (integer
/// division, C style).
pub fn first_pointer(faces: &[Face], selected: Option<usize>, b: Bounds) -> [i32; 2] {
    match selected.and_then(|s| faces.get(s)) {
        Some(f) => [f.x, f.y],
        None => [b[0] + (b[1] - b[0]) / 2, b[2] + (b[3] - b[2]) / 2],
    }
}

/// The tag group of a face (`FUN_002ae3e8`): none (2) for a character without a new bit; else the table's new / seen group
/// for the family (`tags` = [family-0 new, family-0 seen, links new, links seen]).
pub fn tag_group(tags: [usize; 4], links: bool, state: Option<super::marks::Tag>) -> Option<usize> {
    state.map(|t| tags[usize::from(links) * 2 + usize::from(t == super::marks::Tag::Seen)])
}

/// The visible lines of the character page's description (`FUN_002ab678`: a scissor of seven lines).
pub const DESC_VIEW: usize = 7;

/// The number of scroll positions of the description (`FUN_002aadf0`: the list count is the text's lines minus six when it
/// has more than seven, else one).
pub fn desc_positions(lines: usize) -> usize {
    if lines > DESC_VIEW { lines - 6 } else { 1 }
}

/// The scroll arrows of the character page (`FUN_002ab578`): (up shown, down shown). Neither with one position; the up arrow
/// is hidden on the first, the down arrow on the last.
pub fn arrows(top: usize, count: usize) -> (bool, bool) {
    if count < 2 {
        return (false, false);
    }
    (top != 0, top + 1 < count)
}

/// The description's scroll (`FUN_002977a8` with the list flags of `FUN_002a98f0`: step 1, page 7 [inferred: clamped at the ends,
/// as page 3's list]): `dy` is the direction of an Up / Down press, `page` -1 / 1 the L1 / R1 press.
pub fn desc_scroll(top: usize, count: usize, dy: i32, page: i32) -> usize {
    let last = count.saturating_sub(1) as i64;
    let to = top as i64 + i64::from(dy.signum()) + i64::from(page.signum()) * DESC_VIEW as i64;
    to.clamp(0, last) as usize
}

/// `FUN_002aa600`: the character the L2 / R2 pair moves to in the list the page was opened from. Next: the first one after
/// `cur` that exists, else the first one before it, else `cur`; previous: the last one before `cur`, else the last one after
/// it, else `cur`.
pub fn neighbour(list: &[usize], cur: usize, exists: &dyn Fn(usize) -> bool, next: bool) -> usize {
    let n = list.len();
    if next {
        if let Some(i) = (cur + 1..n).find(|&i| exists(list[i])) {
            return i;
        }
        (0..cur.min(n)).find(|&i| exists(list[i])).unwrap_or(cur)
    } else {
        if let Some(i) = (0..cur.min(n)).rev().find(|&i| exists(list[i])) {
            return i;
        }
        (cur + 1..n).rev().find(|&i| exists(list[i])).unwrap_or(cur)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::menu_journal::marks::Tag;

    /// A diag entry: headers of 0xc bytes from +0x10, then 8-byte sub-records.
    fn entry(groups: &[(u16, u16, u8, u8, &[(u16, i16, i16, u8)])]) -> Vec<u8> {
        let mut head = vec![0u8; 0x10];
        head[..4].copy_from_slice(b"JMDI");
        head[8..12].copy_from_slice(&(groups.len() as u32).to_le_bytes());
        let mut subs = Vec::new();
        let base = 0x10 + 0xc * groups.len();
        for &(on, off, family, kind, list) in groups {
            let mut r = vec![0u8; 0xc];
            r[..2].copy_from_slice(&on.to_le_bytes());
            r[2..4].copy_from_slice(&off.to_le_bytes());
            r[4] = family;
            r[5] = list.len() as u8;
            r[6] = kind;
            r[8..12].copy_from_slice(&((base + subs.len()) as u32).to_le_bytes());
            head.extend(r);
            for &(c, x, y, f) in list {
                subs.extend(c.to_le_bytes());
                subs.extend(x.to_le_bytes());
                subs.extend(y.to_le_bytes());
                subs.push(f);
                subs.push(0);
            }
        }
        head.extend(subs);
        head
    }

    #[test]
    fn a_family_shows_its_base_group_then_the_groups_that_are_on() {
        let d = entry(&[
            (10, 0, 4, 0, &[(1, 10, 20, 1), (2, 30, 40, 1), (3, 50, 60, 0)]),
            (11, 12, 4, 1, &[(4, 70, 80, 1)]),
            (13, 0, 4, 1, &[(5, 90, 100, 1)]),
            (20, 0, 5, 0, &[(9, 1, 1, 1)]),
        ]);
        let g = parse_groups(&d);
        assert_eq!(g.len(), 4);
        let flags = |on: &'static [u16]| move |f: u16| on.contains(&f);
        let none = |_: u16| 0;
        // Only the base flag: its two drawn faces (the third has byte +6 clear).
        let f = faces(&d, &g, 4, &flags(&[10]), &none);
        assert_eq!(f, vec![Face { ch: 1, x: 10, y: 20 }, Face { ch: 2, x: 30, y: 40 }]);
        // A later group joins when its flag is set, and leaves when its off flag is set.
        assert_eq!(faces(&d, &g, 4, &flags(&[10, 11]), &none).len(), 3);
        assert_eq!(faces(&d, &g, 4, &flags(&[10, 11, 12]), &none).len(), 2);
        assert_eq!(faces(&d, &g, 4, &flags(&[10, 11, 13]), &none).last(), Some(&Face { ch: 5, x: 90, y: 100 }));
        // Without the base flag nothing of the family is shown from it.
        assert_eq!(faces(&d, &g, 4, &flags(&[]), &none), vec![]);
        // Another family.
        assert_eq!(faces(&d, &g, 5, &flags(&[20]), &none), vec![Face { ch: 9, x: 1, y: 1 }]);
        assert_eq!(faces(&d, &g, 6, &flags(&[20]), &none), vec![]);
    }

    #[test]
    fn an_item_group_needs_the_item() {
        let d = entry(&[(1, 0, 16, 0, &[(1, 1, 1, 1)]), (0x19, 0, 16, 2, &[(7, 5, 5, 1)])]);
        let g = parse_groups(&d);
        let flag = |f: u16| f == 1;
        assert_eq!(faces(&d, &g, 16, &flag, &|_| 0).len(), 1);
        assert_eq!(faces(&d, &g, 16, &flag, &|i| i32::from(i == 0x19)).len(), 2, "kind 2 reads the item count, not a flag");
        assert!(!group_on(&g[1], &|f| f == 0x19, &|_| 0), "a flag with the same number does not count");
    }

    #[test]
    fn the_journal_counts_every_sub_record_of_a_group_that_exists() {
        let d = entry(&[(1, 0, 4, 0, &[(1, 0, 0, 1), (2, 0, 0, 0)]), (2, 1, 4, 1, &[(3, 0, 0, 1)]), (0x19, 0, 16, 2, &[(7, 0, 0, 1)])]);
        let g = parse_groups(&d);
        let flag = |f: u16| f == 1 || f == 2;
        // The off flag of group 2 (flag 1) hides its face but the character still exists; a face without its draw byte counts.
        assert_eq!(present(&d, &g, &flag, &|_| 0), vec![1, 2, 3]);
        assert_eq!(present(&d, &g, &flag, &|i| i32::from(i == 0x19)), vec![1, 2, 3, 7]);
        assert!(!family_complete(&g, 4, &|f| f == 1, &|_| 0));
        assert!(family_complete(&g, 4, &flag, &|_| 0));
        assert!(!family_complete(&g, 16, &flag, &|_| 0));
        assert!(family_complete(&g, 9, &flag, &|_| 0));
    }

    #[test]
    fn the_pointer_moves_three_a_frame_inside_its_bounds() {
        let b = [60, 490, 80, 370];
        assert_eq!(step_pointer([100, 100], b, 1, 0), [103, 100]);
        assert_eq!(step_pointer([100, 100], b, -1, 1), [97, 103]);
        assert_eq!(step_pointer([100, 100], b, 0, -1), [100, 97]);
        assert_eq!(step_pointer([489, 369], b, 1, 1), [490, 370], "a step past the edge stops on it");
        assert_eq!(step_pointer([490, 370], b, 1, 1), [490, 370]);
        assert_eq!(step_pointer([61, 81], b, -1, -1), [60, 80]);
        assert_eq!(step_pointer([60, 80], b, -1, -1), [60, 80]);
        assert_eq!(step_pointer([100, 100], b, 0, 0), [100, 100]);
    }

    #[test]
    fn the_face_under_the_pointer_is_the_first_within_half_a_pitch() {
        let f = [Face { ch: 0, x: 100, y: 100 }, Face { ch: 1, x: 120, y: 100 }, Face { ch: 2, x: 300, y: 300 }];
        assert_eq!(hit(&f, [100, 100], 32), Some(0));
        assert_eq!(hit(&f, [150, 100], 32), Some(1), "past the first face's reach");
        assert_eq!(hit(&f, [132, 132], 32), Some(0), "the edge counts");
        assert_eq!(hit(&f, [133, 100], 32), Some(1));
        assert_eq!(hit(&f, [200, 200], 32), None);
        assert_eq!(first_pointer(&f, Some(2), [60, 490, 80, 370]), [300, 300]);
        assert_eq!(first_pointer(&f, None, [60, 490, 80, 370]), [275, 225]);
        assert_eq!(first_pointer(&f, Some(9), [60, 490, 80, 370]), [275, 225]);
    }

    #[test]
    fn the_tag_group_follows_the_family_and_the_state() {
        let t = [0x18, 0x19, 0x1b, 0x1c];
        assert_eq!(tag_group(t, false, Some(Tag::New)), Some(0x18));
        assert_eq!(tag_group(t, false, Some(Tag::Seen)), Some(0x19));
        assert_eq!(tag_group(t, true, Some(Tag::New)), Some(0x1b));
        assert_eq!(tag_group(t, true, Some(Tag::Seen)), Some(0x1c));
        assert_eq!(tag_group(t, true, None), None);
    }

    #[test]
    fn the_description_scrolls_by_lines_and_pages() {
        assert_eq!(desc_positions(3), 1);
        assert_eq!(desc_positions(7), 1);
        assert_eq!(desc_positions(8), 2);
        assert_eq!(desc_positions(20), 14);
        assert_eq!(arrows(0, 1), (false, false));
        assert_eq!(arrows(0, 3), (false, true));
        assert_eq!(arrows(1, 3), (true, true));
        assert_eq!(arrows(2, 3), (true, false));
        assert_eq!(desc_scroll(0, 14, 1, 0), 1);
        assert_eq!(desc_scroll(0, 14, -1, 0), 0, "no wrap at the top");
        assert_eq!(desc_scroll(13, 14, 1, 0), 13);
        assert_eq!(desc_scroll(2, 14, 0, 1), 9);
        assert_eq!(desc_scroll(9, 14, 0, 1), 13, "a page stops at the last position");
        assert_eq!(desc_scroll(3, 14, 0, -1), 0);
    }

    #[test]
    fn the_character_pair_skips_those_that_do_not_exist_and_wraps() {
        let list = [10, 11, 12, 13];
        let all = |_: usize| true;
        assert_eq!(neighbour(&list, 1, &all, true), 2);
        assert_eq!(neighbour(&list, 1, &all, false), 0);
        assert_eq!(neighbour(&list, 3, &all, true), 0, "past the end it wraps to the first");
        assert_eq!(neighbour(&list, 0, &all, false), 3, "before the start it wraps to the last");
        let some = |c: usize| c == 10 || c == 12;
        assert_eq!(neighbour(&list, 0, &some, true), 2);
        assert_eq!(neighbour(&list, 2, &some, true), 0);
        assert_eq!(neighbour(&list, 2, &some, false), 0);
        assert_eq!(neighbour(&list, 0, &some, false), 2);
        let lone = |c: usize| c == 11;
        assert_eq!(neighbour(&list, 1, &lone, true), 1, "the only one stays");
        assert_eq!(neighbour(&list, 1, &lone, false), 1);
        assert_eq!(neighbour(&list, 0, &|_| false, true), 0);
    }

    /// A record's bytes give the model fields (`FUN_002abf60`): the motion, the small model's x, y, turn and scale and the large one's x, y and scale.
    #[test]
    fn a_record_carries_its_model_placements() {
        let mut d = vec![0u8; 0x10 + 0x24];
        d[8] = 1;
        let r = &mut d[0x10..];
        r[0xc..0xe].copy_from_slice(&1309u16.to_le_bytes());
        r[0xe..0x10].copy_from_slice(&133u16.to_le_bytes());
        r[0x10..0x12].copy_from_slice(&0x802u16.to_le_bytes());
        r[0x12..0x14].copy_from_slice(&183i16.to_le_bytes());
        r[0x14..0x16].copy_from_slice(&(-20i16).to_le_bytes());
        r[0x16..0x18].copy_from_slice(&40i16.to_le_bytes());
        r[0x18..0x1a].copy_from_slice(&(-3i16).to_le_bytes());
        r[0x1a..0x1c].copy_from_slice(&145i16.to_le_bytes());
        r[0x1c..0x20].copy_from_slice(&1.36f32.to_le_bytes());
        r[0x20..0x24].copy_from_slice(&1.52f32.to_le_bytes());
        let c = parse_chars(&d);
        assert_eq!(c.len(), 1);
        assert_eq!((c[0].model, c[0].motion, c[0].flags), (1309, 133, 0x802));
        assert_eq!(c[0].place[0], ModelPlace { x: 183, y: -20, turn: 40, scale: 1.36 });
        assert_eq!(c[0].place[1], ModelPlace { x: -3, y: 145, turn: 0, scale: 1.52 });
    }

    fn rec(family: u8, flags: u16, model: u16) -> CharRec {
        CharRec { family, flags, model, ..CharRec::default() }
    }

    #[test]
    fn an_enemy_record_exists_once_the_enemy_was_defeated() {
        let recs = [rec(0x11, ENEMY_FLAG, 5), rec(0x11, ENEMY_FLAG, 6), rec(0x11, 0, 7), rec(0, 0, 8), rec(0x12, ENEMY_FLAG | 4, 9)];
        let count = |r: &CharRec| u32::from(r.model == 6 || r.model == 7 || r.model == 9);
        // Model 7 has a count but no counter flag: only records with flag 0x800 are made to exist by their count.
        assert_eq!(enemies_present(&recs, &count), vec![1, 4]);
        assert_eq!(enemies_present(&recs, &|_| 0), Vec::<usize>::new());
    }

    #[test]
    fn a_family_list_has_the_records_that_exist_or_all_with_the_all_worlds_flag() {
        let recs = [rec(0x11, 0, 0), rec(0x12, 0, 0), rec(0x11, 0, 0), rec(0x11, 0, 0), rec(0, 0, 0)];
        let there = |i: usize| i == 0 || i == 3 || i == 4;
        assert_eq!(family_list(&recs, 0x11, &there, false), vec![0, 3]);
        assert_eq!(family_list(&recs, 0x11, &there, true), vec![0, 2, 3]);
        assert_eq!(family_list(&recs, 0x12, &there, false), Vec::<usize>::new());
        assert_eq!(family_list(&recs, 0, &there, false), vec![4]);
    }
}
