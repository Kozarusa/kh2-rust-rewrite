//! The journal's NEW tags and the scroll bar object (docs/menus.md 20.9, WP-MN12).
//!
//! The original keeps two bit sets. The "seen" set lives in the save (`FUN_002b70e0` = save block + 0x38f0: one bit per
//! story record, album record, treasure, map, character and so on). The "new" buffer `DAT_0036d4e8` (0x128 bytes,
//! `FUN_002b7060`) is rebuilt by `FUN_002b7100` every time the scene opens: a bit is set for every entry that exists now
//! (its story flag is set, its item is owned, ...) and is not yet in the seen set. A tag is drawn only for entries whose
//! new bit is set: group 0x39 until the entry has been seen, 0x3a after (`FUN_002b9480`: new and seen). An entry is
//! marked seen when its first line reaches the top row of the list (`FUN_002a5708` -> `FUN_002b95d8`).
//!
//! The app has no journal save data: the seen set lives for the run only (`Marks::seen`) and starts empty, so a game
//! that has never opened the journal shows every existing entry as new [inferred: the original's new-game save has the
//! seen bits clear].

use std::collections::HashSet;

/// The journal's categories that have their own bit sets (the offsets inside the buffers: characters +0, album +0x34, limits
/// +0x74, minigames +0x78, quests +0x80, story +0x90, treasures +0xd0, maps +0x110, the Ansem reports +0x120).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Cat {
    Story,
    Char,
    Album,
    Treasure,
    Map,
    Quest,
    Minigame,
}

impl Cat {
    /// The category's number in the save's seen set (`Progress::journal_seen`).
    pub fn code(self) -> u8 {
        match self {
            Cat::Story => 0,
            Cat::Char => 1,
            Cat::Album => 2,
            Cat::Treasure => 3,
            Cat::Map => 4,
            Cat::Quest => 5,
            Cat::Minigame => 6,
        }
    }

    /// The category of a saved number. The numbers are part of the save format: never renumber.
    pub fn from_code(c: u8) -> Option<Cat> {
        Some(match c {
            0 => Cat::Story,
            1 => Cat::Char,
            2 => Cat::Album,
            3 => Cat::Treasure,
            4 => Cat::Map,
            5 => Cat::Quest,
            6 => Cat::Minigame,
            _ => return None,
        })
    }
}

/// The new / seen bit sets. Keys are the record's own number (story: the record's index in `jiminy.bar` entry 1; character:
/// the index in entry 3; album: the record index of entry 2; treasure and map: the row of the `trsr` table).
#[derive(Default)]
pub struct Marks {
    seen: HashSet<(Cat, usize)>,
    dirty: bool,
    new: HashSet<(Cat, usize)>,
}

impl Marks {
    /// `FUN_002b7100`: the new buffer after a scene open: every entry of `present` that is not yet seen.
    pub fn snapshot(&mut self, present: impl IntoIterator<Item = (Cat, usize)>) {
        self.new = present.into_iter().filter(|k| !self.seen.contains(k)).collect();
    }

    /// The entry has a new bit (`FUN_002b9360` and the like with an index).
    pub fn is_new(&self, cat: Cat, key: usize) -> bool {
        self.new.contains(&(cat, key))
    }

    /// The entry has a new bit and has been seen since (`FUN_002b9480` with an index).
    pub fn is_seen_new(&self, cat: Cat, key: usize) -> bool {
        self.new.contains(&(cat, key)) && self.seen.contains(&(cat, key))
    }

    /// `FUN_002b95d8` (story), `FUN_002b73b8` (characters) and the like: the entry's seen bit.
    pub fn see(&mut self, cat: Cat, key: usize) {
        self.dirty |= self.seen.insert((cat, key));
    }

    /// Replaces the seen set by the one the save carries (`Progress::journal_seen_all`: category numbers and entries).
    pub fn load_seen(&mut self, pairs: impl IntoIterator<Item = (u8, u32)>) {
        let loaded: HashSet<(Cat, usize)> = pairs.into_iter().filter_map(|(c, k)| Some((Cat::from_code(c)?, k as usize))).collect();
        // What was marked this run and not yet saved stays.
        if !self.dirty {
            self.seen = loaded;
        } else {
            self.seen.extend(loaded);
        }
    }

    /// The pairs marked since the last call (for `Progress::set_journal_seen`), `None` when there are none.
    pub fn take_dirty(&mut self) -> Option<Vec<(u8, u32)>> {
        std::mem::take(&mut self.dirty).then(|| self.seen.iter().map(|&(c, k)| (c.code(), k as u32)).collect())
    }

    /// The tag group of an entry on a list page (`FUN_002a5708`): `None` for an entry without a new bit.
    pub fn tag(&self, cat: Cat, key: usize) -> Option<Tag> {
        self.is_new(cat, key).then(|| if self.is_seen_new(cat, key) { Tag::Seen } else { Tag::New })
    }

    /// The aggregate of a whole category of a world (`FUN_002b9360` / `FUN_002b9480` with the index -1): `None` when no
    /// entry has a new bit, else `Some(all those entries have been seen)`.
    pub fn aggregate(&self, keys: impl IntoIterator<Item = (Cat, usize)>) -> Option<bool> {
        let mut any = false;
        let mut all_seen = true;
        for k in keys {
            if self.new.contains(&k) {
                any = true;
                all_seen &= self.seen.contains(&k);
            }
        }
        any.then_some(all_seen)
    }
}

/// The two states of a NEW tag.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tag {
    New,
    Seen,
}

/// The rows of page 3's line marks (`FUN_002a5708`): for the list's first line `top`, the entries that start inside the ten
/// rows, as (row, entry index). The entry that holds the first line starts above the rows unless `top` is its first line.
/// `lines` are the entries' line counts.
pub fn story_mark_rows(lines: &[usize], top: usize, rows: usize) -> Vec<(usize, usize)> {
    let mut start = 0;
    let mut found = None;
    for (i, &l) in lines.iter().enumerate() {
        if top == start || top < start + l {
            found = Some((i, start as i64 - top as i64));
            break;
        }
        start += l;
    }
    let Some((mut e, mut row)) = found else { return Vec::new() };
    let mut out = Vec::new();
    while e < lines.len() && row < rows as i64 {
        if row >= 0 {
            out.push((row as usize, e));
        }
        row += lines[e] as i64;
        e += 1;
    }
    out
}

/// The scroll bar object `FUN_00299878` (page 3's `FUN_002a4c38`, and the lists of 7 and 8): four parts (groups 1,
/// 0x75, 0x74, 0x76 of the list sequence), placed from the track group of the layer it hangs from. `track` is that
/// group's +0x18 (the track length), `body` the +0x18 of group 0x74 (the thumb's body), `total` the list length the bar
/// was made for and `visible` the rows shown.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Bar {
    pub track: i32,
    pub body: i32,
    pub total: i32,
    pub visible: i32,
    pub made_for: i32,
}

impl Bar {
    /// `FUN_00299878(.., total, visible, ..)`: the larger of the two is the list length the thumb is scaled to
    /// (the bar's +0x20 word), `visible` the rows shown (+0x24).
    pub fn new(track: i32, body: i32, total: i32, visible: i32) -> Bar {
        Bar { track, body, total: total.max(visible), visible, made_for: total }
    }

    /// The y scale of the thumb's body (part 2): the track against the body's own length, times the share of the list that
    /// is shown (at most 1), `(h0 / h2) * min(visible / total, 1)` with the unclamped `total` the bar was made for.
    pub fn body_scale(&self) -> f32 {
        let ratio = self.visible as f32 / self.made_for as f32;
        let ratio = if ratio >= 1.0 { 1.0 } else { ratio };
        (self.track as f32 / self.body as f32) * ratio
    }

    /// `FUN_00299c10(bar, pos)`: the y offsets of parts 1, 2 and 3 for the list at line `pos`.
    pub fn offsets(&self, pos: i32) -> [i32; 3] {
        let (vis, total) = (self.visible, self.total);
        let ratio = vis as f32 / total as f32;
        let pos = if total <= pos + vis { total - vis } else { pos };
        let full = ratio >= 1.0;
        let share = if full { 1.0 } else { ratio };
        let top = ((self.track as f32 / total as f32) * pos as f32) as i32;
        let len = self.body as f32 * (self.track as f32 / self.body as f32) * share;
        let mut end = top + len as i32;
        if full && len - end as f32 == 0.0 {
            end -= 1;
        }
        [top, top, end]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The numbers are the save format (`journal_seen` pairs): each category keeps its number and nothing else maps.
    #[test]
    fn every_category_keeps_its_saved_number() {
        let all = [(Cat::Story, 0u8), (Cat::Char, 1), (Cat::Album, 2), (Cat::Treasure, 3), (Cat::Map, 4), (Cat::Quest, 5), (Cat::Minigame, 6)];
        for (cat, n) in all {
            assert_eq!(cat.code(), n, "{cat:?}");
            assert_eq!(Cat::from_code(n), Some(cat));
        }
        assert_eq!(Cat::from_code(7), None);
        assert_eq!(Cat::from_code(255), None);
    }

    #[test]
    fn the_seen_set_goes_to_and_from_the_save() {
        let mut m = Marks::default();
        assert_eq!(m.take_dirty(), None);
        m.see(Cat::Album, 4);
        m.see(Cat::Map, 2);
        let mut saved = m.take_dirty().expect("two entries were marked");
        saved.sort_unstable();
        assert_eq!(saved, [(Cat::Album.code(), 4), (Cat::Map.code(), 2)]);
        assert_eq!(m.take_dirty(), None, "nothing new since");
        let mut n = Marks::default();
        n.load_seen(saved.iter().copied().chain([(99, 1)]));
        n.snapshot([(Cat::Album, 4), (Cat::Album, 5)]);
        assert_eq!(n.tag(Cat::Album, 4), None, "seen in the save before the scene opened");
        assert_eq!(n.tag(Cat::Album, 5), Some(Tag::New));
        assert_eq!(n.take_dirty(), None, "loading marks nothing new");
        assert_eq!(Cat::from_code(Cat::Treasure.code()), Some(Cat::Treasure));
    }

    #[test]
    fn a_tag_needs_a_new_bit_and_turns_on_seeing() {
        let mut m = Marks::default();
        m.see(Cat::Story, 1);
        m.snapshot([(Cat::Story, 0), (Cat::Story, 1), (Cat::Story, 2)]);
        // Entry 1 was seen before the scene opened: no new bit, no tag.
        assert_eq!(m.tag(Cat::Story, 1), None);
        assert_eq!(m.tag(Cat::Story, 0), Some(Tag::New));
        m.see(Cat::Story, 0);
        assert_eq!(m.tag(Cat::Story, 0), Some(Tag::Seen));
        assert_eq!(m.tag(Cat::Story, 2), Some(Tag::New));
        // A new snapshot drops what was seen.
        m.snapshot([(Cat::Story, 0), (Cat::Story, 2)]);
        assert_eq!(m.tag(Cat::Story, 0), None);
        assert_eq!(m.tag(Cat::Story, 2), Some(Tag::New));
        assert_eq!(m.tag(Cat::Album, 2), None, "another category has its own bits");
    }

    #[test]
    fn the_category_aggregate_is_none_new_or_all_seen() {
        let mut m = Marks::default();
        m.snapshot([(Cat::Story, 0), (Cat::Story, 1)]);
        let keys = || [Cat::Story].into_iter().flat_map(|c| (0..3).map(move |k| (c, k)));
        assert_eq!(m.aggregate(keys()), Some(false));
        m.see(Cat::Story, 0);
        assert_eq!(m.aggregate(keys()), Some(false), "entry 1 is still unseen");
        m.see(Cat::Story, 1);
        assert_eq!(m.aggregate(keys()), Some(true));
        assert_eq!(m.aggregate([(Cat::Album, 0)]), None);
    }

    #[test]
    fn the_mark_rows_follow_the_entries_in_view() {
        // Entries of 3, 4 and 12 lines.
        let l = [3, 4, 12];
        assert_eq!(story_mark_rows(&l, 0, 10), vec![(0, 0), (3, 1), (7, 2)]);
        // The first line of entry 1: it is on row 0; entry 2 follows four rows down.
        assert_eq!(story_mark_rows(&l, 3, 10), vec![(0, 1), (4, 2)]);
        // Inside entry 1: its first line is above the rows, no mark for it.
        assert_eq!(story_mark_rows(&l, 5, 10), vec![(2, 2)]);
        // Inside the last entry: nothing starts in view.
        assert_eq!(story_mark_rows(&l, 9, 10), vec![]);
        assert_eq!(story_mark_rows(&l, 7, 10), vec![(0, 2)]);
        assert_eq!(story_mark_rows(&[], 0, 10), vec![]);
    }

    #[test]
    fn the_scroll_thumb_scales_with_the_share_shown() {
        // Track 200 long, body 20 long; a list of 40 lines with 10 shown.
        let b = Bar::new(200, 20, 40, 10);
        assert_eq!(b.body_scale(), 200.0 / 20.0 * 0.25);
        assert_eq!(b.offsets(0), [0, 0, 50]);
        assert_eq!(b.offsets(20), [100, 100, 150]);
        // Past the end the position is clamped to total - visible.
        assert_eq!(b.offsets(39), [150, 150, 200]);
        // A list that fits: the thumb fills the track; its end is pulled in by one pixel.
        let full = Bar::new(200, 20, 8, 10);
        assert_eq!(full.body_scale(), 10.0);
        assert_eq!(full.offsets(0), [0, 0, 199]);
        assert_eq!(Bar::new(200, 20, 8, 10).total, 10, "the larger of the two");
    }
}
