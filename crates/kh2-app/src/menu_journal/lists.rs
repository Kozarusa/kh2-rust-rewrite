//! The Treasures (page 7) and Maps (page 8) pages of Jiminy's Journal (docs/menus.md 20.9, WP-MN12): the layout and the cursor.
//!
//! Both pages list the rows of the treasure table (`trsr`, `DAT_01c95590`, 0xc-byte rows) in a grid of eight cells a row,
//! grouped by journal world (`FUN_002b00e0` for treasures, `FUN_002b0228` for maps): a section of ceil(count / 8) rows per
//! world, in world order, nine rows in view, a scroll bar and a cursor on a cell. The page struct `DAT_0036d4bc` holds the
//! sorted entries, per world the count, the row count and the first entry (+0x1000, +0x1001, +0x1004), the entry count
//! (+0x1078), the found count (+0x107c), the total rows (+0x1080), the first row in view (+0x1084) and the cursor (+0x1088:
//! view row * 8 + column).

/// A world's sections: the journal worlds 0..0xe.
pub const WORLDS: usize = 15;
/// Cells in a row and rows in view (`FUN_002afca0`: the scroll bar is made for 9).
pub const COLS: usize = 8;
pub const VIEW_ROWS: usize = 9;

/// One entry (8 bytes in the original: world, found, table ordinal, item).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Entry {
    /// The journal world index of the row's game world (`FUN_00277c88`).
    pub world: u8,
    /// Treasures: the chest has been opened (`FUN_001a6a90`); maps: the map is owned (`FUN_001a1cc8 > 0`).
    pub found: bool,
    /// The row's ordinal in the table: the sort key inside a world.
    pub idx: u16,
    pub item: u16,
}

/// The parts of a `trsr` row the pages read (`Trsr`: kind, game world, story flag, item, chest bit).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Row {
    pub kind: u8,
    pub world: u8,
    pub flag: i16,
    pub item: u16,
    pub chest: i16,
}

/// `FUN_002b00e0` (page 7): the chests (kind 0) with a flag other than 0 and 0x800, that exist: their flag is set or the
/// all-worlds flag is; `found` is the chest's opened bit. A row whose game world has no journal world (index below 0 or
/// above 0xe) is left out [the original would index past its world table].
pub fn treasures(rows: &[Row], world_index: &dyn Fn(u8) -> i8, flag: &dyn Fn(u16) -> bool, all: bool, opened: &dyn Fn(i16) -> bool) -> Vec<Entry> {
    rows.iter()
        .enumerate()
        .filter(|(_, r)| r.kind == 0 && r.flag != 0 && r.flag != 0x800 && (flag(r.flag as u16) || all))
        .filter_map(|(i, r)| {
            let w = world_index(r.world);
            (0..WORLDS as i8).contains(&w).then(|| Entry { world: w as u8, found: r.chest != 0 && opened(r.chest), idx: i as u16, item: r.item })
        })
        .collect()
}

/// `FUN_002b0228` (page 8): the rows whose item is a map (item type 0x16) and owned, or any such row with the all-worlds
/// flag set; `found` is the map being owned.
pub fn maps(rows: &[Row], world_index: &dyn Fn(u8) -> i8, is_map: &dyn Fn(u16) -> bool, owned: &dyn Fn(u16) -> bool, all: bool) -> Vec<Entry> {
    rows.iter()
        .enumerate()
        .filter(|(_, r)| is_map(r.item) && (owned(r.item) || all))
        .filter_map(|(i, r)| {
            let w = world_index(r.world);
            (0..WORLDS as i8).contains(&w).then(|| Entry { world: w as u8, found: owned(r.item), idx: i as u16, item: r.item })
        })
        .collect()
}

/// `FUN_002b0358`: the entries sorted by world, then by table ordinal.
pub fn sorted(mut v: Vec<Entry>) -> Vec<Entry> {
    v.sort_by_key(|e| (e.world, e.idx));
    v
}

/// One world's section.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Section {
    /// Entries of the world (+0x1000).
    pub count: usize,
    /// Rows of the section (+0x1001): ceil(count / 8), or 1 for an open world without entries.
    pub rows: usize,
    /// Index of the first entry in the sorted list.
    pub first: usize,
}

/// The page's layout: a section per world and the rows in all.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Layout {
    pub sections: [Section; WORLDS],
    pub total: usize,
    pub entries: Vec<Entry>,
}

/// `FUN_002b0358` (its tail): the sections of the sorted entries; an open world without entries still gets a row (an empty
/// one), except the world `skip` (treasures leave out world 8: `FUN_002b1650`). `open` is whether a world's cell exists
/// (`FUN_002a3120`).
pub fn layout(entries: Vec<Entry>, open: &dyn Fn(usize) -> bool, skip: Option<usize>) -> Layout {
    let mut sections = [Section::default(); WORLDS];
    for (i, e) in entries.iter().enumerate() {
        let s = &mut sections[usize::from(e.world)];
        if s.count == 0 {
            s.first = i;
        }
        s.count += 1;
    }
    let mut total = 0;
    for (w, s) in sections.iter_mut().enumerate() {
        s.rows = s.count.div_ceil(COLS);
        if skip != Some(w) {
            if s.rows == 0 && open(w) {
                s.rows = 1;
            }
            total += s.rows;
        } else {
            // The skipped world adds nothing to the total, but its rows (if any) stay in the table.
        }
    }
    Layout { sections, total, entries }
}

impl Layout {
    /// `FUN_002b1c30`: the world whose section holds global row `row` (the first world with rows that reaches it; 15 when
    /// the row is past the last section).
    pub fn world_of_row(&self, row: usize) -> usize {
        let mut acc = 0;
        for (w, s) in self.sections.iter().enumerate() {
            if s.rows == 0 {
                continue;
            }
            acc += s.rows;
            if acc > row {
                return w;
            }
        }
        WORLDS
    }

    /// The global row where a world's section starts.
    pub fn first_row(&self, world: usize) -> usize {
        self.sections.iter().take(world).map(|s| s.rows).sum()
    }

    /// `FUN_002b1ca0`: the cells in global row `row`: 8, or the rest of the world's count on its last row, or 1 for an
    /// empty section's one row.
    pub fn cells_in_row(&self, row: usize) -> usize {
        let w = self.world_of_row(row);
        let Some(s) = self.sections.get(w) else { return 1 };
        if row + 1 >= self.first_row(w) + s.rows {
            match s.count % COLS {
                0 if s.count == 0 => 1,
                0 => COLS,
                n => n,
            }
        } else {
            COLS
        }
    }

    /// `FUN_002b1d78`: the entry (index in the sorted list) a cursor (`top`, view row * 8 + column) is on; `None` on the
    /// empty row of a world without entries.
    pub fn entry_at(&self, top: usize, cursor: usize) -> Option<usize> {
        let row = top + cursor / COLS;
        let w = self.world_of_row(row);
        let s = self.sections.get(w)?;
        let i = (row - self.first_row(w)) * COLS + cursor % COLS;
        (i < s.count).then_some(s.first + i)
    }
}

/// The cursor's state: the first row in view and the cursor (view row * 8 + column).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Cursor {
    pub top: usize,
    pub at: usize,
}

/// `FUN_002b1e18`: where the page opens: scrolled to the section of the journal world the page was entered from, the cursor
/// on the view's first cell.
pub fn start(l: &Layout, world: usize) -> Cursor {
    let visible = l.total.min(VIEW_ROWS);
    let before = l.first_row(world.min(WORLDS));
    let max = l.total - visible;
    if max < before { Cursor { top: max, at: (before - max) * COLS } } else { Cursor { top: before, at: 0 } }
}

/// The pad's presses the page reads.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Nav {
    Up,
    Down,
    Left,
    Right,
    /// L1 and R1: a page up and a page down.
    PageUp,
    PageDown,
}

/// `FUN_002b1890`: one press. Down and up scroll after the view's last / first row and wrap at the ends, L1 / R1 move by a
/// page (and wrap), left and right wrap inside the row; the column is kept where the new row has a cell for it. `fresh` is
/// a press that just began (the wraps need it; the original also repeats a held pad, which the app does not).
pub fn navigate(l: &Layout, c: Cursor, nav: Nav, fresh: bool) -> Cursor {
    let vis = l.total.min(VIEW_ROWS);
    if vis == 0 {
        return c;
    }
    let (mut top, mut row, mut col) = (c.top, c.at / COLS, c.at % COLS);
    let mut global = row + top;
    let max_top = l.total - vis;
    match nav {
        Nav::Up => {
            if row != 0 {
                row -= 1;
            } else if top == 0 {
                if fresh {
                    top = max_top;
                    row = vis - 1;
                }
            } else {
                top -= 1;
            }
            global = row + top;
        }
        Nav::Down => {
            if row + 1 < vis {
                row += 1;
            } else if top + VIEW_ROWS < l.total {
                top += 1;
            } else if fresh {
                (top, row) = (0, 0);
            }
            global = row + top;
        }
        Nav::PageUp => {
            if top <= vis {
                if top == 0 {
                    if col == 0 {
                        if fresh {
                            top = max_top;
                            row = vis - 1;
                            global = row + top;
                            col = l.cells_in_row(global) - 1;
                        }
                    } else {
                        col = 0;
                    }
                } else {
                    (top, row, global) = (0, 0, 0);
                }
            } else {
                top -= vis;
                global = row + top;
            }
        }
        Nav::PageDown => {
            if top + vis < max_top {
                top += vis;
                global = row + top;
            } else if top < max_top {
                top = max_top;
                row = vis - 1;
                global = row + top;
            } else {
                let last = l.cells_in_row(global) - 1;
                if col < last {
                    col = last;
                } else if fresh {
                    (top, row, col, global) = (0, 0, 0, 0);
                }
            }
        }
        Nav::Left | Nav::Right => {}
    }
    let n = l.cells_in_row(global);
    if n <= col {
        col = n - 1;
    }
    match nav {
        Nav::Left => col = if col != 0 { col - 1 } else { n - 1 },
        Nav::Right => col = if col < n - 1 { col + 1 } else { 0 },
        _ => {}
    }
    Cursor { top, at: row * COLS + col }
}

/// One row in view: its world when a section starts on it (the label shows there), and the found flags of its cells.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ViewRow {
    pub label: Option<usize>,
    pub cells: Vec<bool>,
}

/// `FUN_002b0678` / `FUN_002b09e8`: the rows in view from `top`: a label on every row where a section starts (a section that
/// began above the view has none), a cell per entry.
pub fn view(l: &Layout, top: usize) -> Vec<ViewRow> {
    (top..(top + VIEW_ROWS).min(l.total))
        .map(|row| {
            let w = l.world_of_row(row);
            let Some(s) = l.sections.get(w) else { return ViewRow { label: None, cells: Vec::new() } };
            let start = l.first_row(w);
            let i = (row - start) * COLS;
            let n = s.count.saturating_sub(i).min(COLS);
            let cells = (0..n).map(|k| l.entries[s.first + i + k].found).collect();
            ViewRow { label: (row == start).then_some(w), cells }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn e(world: u8, idx: u16, found: bool) -> Entry {
        Entry { world, found, idx, item: idx }
    }

    /// World 0: 10 entries (2 rows), world 1: none but open (1 row), world 2: 8 entries (1 row), world 3: 3 entries.
    fn sample() -> Layout {
        let mut v: Vec<Entry> = (0..10).map(|i| e(0, i, i % 2 == 0)).collect();
        v.extend((20..28).map(|i| e(2, i, false)));
        v.extend((30..33).map(|i| e(3, i, true)));
        layout(sorted(v), &|w| w <= 3, None)
    }

    #[test]
    fn treasures_and_maps_pick_their_rows() {
        let rows = [
            Row { kind: 0, world: 2, flag: 0x10, item: 1, chest: 5 },
            Row { kind: 0, world: 2, flag: 0, item: 2, chest: 6 },
            Row { kind: 0, world: 2, flag: 0x800, item: 3, chest: 7 },
            Row { kind: 1, world: 2, flag: 0x10, item: 4, chest: 8 },
            Row { kind: 0, world: 4, flag: 0x11, item: 5, chest: 9 },
            Row { kind: 0, world: 0, flag: 0x10, item: 6, chest: 10 },
        ];
        let wi = |w: u8| match w {
            2 => 0,
            4 => 1,
            _ => -1,
        };
        let flag = |f: u16| f == 0x10;
        let t = treasures(&rows, &wi, &flag, false, &|c| c == 5);
        // Only the first chest has its flag; the world-0 row has no journal world.
        assert_eq!(t, vec![Entry { world: 0, found: true, idx: 0, item: 1 }]);
        let all = treasures(&rows, &wi, &flag, true, &|_| false);
        assert_eq!(all.iter().map(|x| x.item).collect::<Vec<_>>(), vec![1, 5], "the all-worlds flag shows the rest");
        assert!(all.iter().all(|x| !x.found));
        let is_map = |i: u16| i == 4 || i == 5;
        let m = maps(&rows, &wi, &is_map, &|i| i == 4, false);
        assert_eq!(m, vec![Entry { world: 0, found: true, idx: 3, item: 4 }], "only owned maps without the flag");
        assert_eq!(maps(&rows, &wi, &is_map, &|i| i == 4, true).len(), 2);
    }

    #[test]
    fn sections_count_rows_per_world() {
        let l = sample();
        assert_eq!(l.sections[0], Section { count: 10, rows: 2, first: 0 });
        assert_eq!(l.sections[1], Section { count: 0, rows: 1, first: 0 }, "an open world without entries keeps one row");
        assert_eq!((l.sections[2].count, l.sections[2].rows, l.sections[2].first), (8, 1, 10));
        assert_eq!((l.sections[3].count, l.sections[3].rows, l.sections[3].first), (3, 1, 18));
        assert_eq!(l.sections[4].rows, 0, "a closed world has none");
        assert_eq!(l.total, 5);
        assert_eq!((0..5).map(|r| l.world_of_row(r)).collect::<Vec<_>>(), vec![0, 0, 1, 2, 3]);
        assert_eq!(l.world_of_row(5), WORLDS);
        assert_eq!((0..5).map(|r| l.cells_in_row(r)).collect::<Vec<_>>(), vec![8, 2, 1, 8, 3]);
        assert_eq!((l.first_row(0), l.first_row(2), l.first_row(3)), (0, 3, 4));
        // The skipped world adds no row to the total.
        let l2 = layout(vec![e(8, 1, false)], &|_| true, Some(8));
        assert_eq!(l2.sections[8].rows, 1);
        assert_eq!(l2.total, 14);
    }

    #[test]
    fn the_cursor_names_its_entry() {
        let l = sample();
        assert_eq!(l.entry_at(0, 0), Some(0));
        assert_eq!(l.entry_at(0, 9), Some(9), "row 1, column 1 is the tenth entry");
        assert_eq!(l.entry_at(0, 2 * COLS), None, "the empty row");
        assert_eq!(l.entry_at(0, 3 * COLS + 7), Some(17));
        assert_eq!(l.entry_at(2, 2 * COLS + 1), Some(19), "scrolled by two rows");
    }

    #[test]
    fn the_page_opens_on_the_worlds_section() {
        let l = sample();
        // Five rows fit: nothing to scroll, the section's row is not scrolled to.
        assert_eq!(start(&l, 3), Cursor { top: 0, at: 4 * COLS });
        assert_eq!(start(&l, 0), Cursor { top: 0, at: 0 });
        let mut v: Vec<Entry> = Vec::new();
        for w in 0..6u8 {
            v.extend((0..16).map(|i| e(w, u16::from(w) * 100 + i, false)));
        }
        let big = layout(sorted(v), &|_| true, None);
        assert_eq!(big.total, 12 + 9, "6 worlds of 2 rows and 9 empty ones");
        // World 4 starts at row 8: the view can go no further than total - 9.
        assert_eq!(big.first_row(4), 8);
        assert_eq!(start(&big, 4), Cursor { top: 8, at: 0 });
        assert_eq!(start(&big, 12), Cursor { top: big.total - 9, at: (big.first_row(12) - (big.total - 9)) * COLS });
    }

    #[test]
    fn moving_scrolls_wraps_and_keeps_the_column() {
        let mut v: Vec<Entry> = (0..80).map(|i| e(0, i, false)).collect();
        v.extend((100..103).map(|i| e(1, i, false)));
        let l = layout(sorted(v), &|w| w <= 1, None);
        // 10 rows of world 0, then one of world 1: 11 rows, nine in view.
        assert_eq!(l.total, 11);
        let c0 = Cursor { top: 0, at: 0 };
        assert_eq!(navigate(&l, c0, Nav::Right, true), Cursor { top: 0, at: 1 });
        assert_eq!(navigate(&l, c0, Nav::Left, true), Cursor { top: 0, at: 7 }, "left wraps to the row's last cell");
        assert_eq!(navigate(&l, Cursor { top: 0, at: 7 }, Nav::Right, true), c0);
        assert_eq!(navigate(&l, c0, Nav::Down, true), Cursor { top: 0, at: 8 });
        // At the view's last row a down scrolls; at the very end a new press wraps to the top.
        let last_view = Cursor { top: 0, at: 8 * COLS + 5 };
        assert_eq!(navigate(&l, last_view, Nav::Down, true), Cursor { top: 1, at: 8 * COLS + 5 });
        let end = Cursor { top: 2, at: 8 * COLS + 2 };
        assert_eq!(l.cells_in_row(10), 3);
        assert_eq!(navigate(&l, end, Nav::Down, true), Cursor { top: 0, at: 2 }, "a new press wraps to the top row");
        assert_eq!(navigate(&l, end, Nav::Down, false), end, "a held pad stops at the end");
        // The column is cut to the new row's cells.
        assert_eq!(navigate(&l, Cursor { top: 2, at: 7 * COLS + 7 }, Nav::Down, true), Cursor { top: 2, at: 8 * COLS + 2 }, "row 10 has three cells");
        // Up from the first row wraps to the last, a held pad does not.
        assert_eq!(navigate(&l, c0, Nav::Up, true), Cursor { top: 2, at: 8 * COLS }, "the last row, column 0");
        assert_eq!(navigate(&l, c0, Nav::Up, false), c0);
        assert_eq!(navigate(&l, Cursor { top: 1, at: 3 }, Nav::Up, true), Cursor { top: 0, at: 3 }, "scroll up by one");
    }

    #[test]
    fn pages_move_by_the_view() {
        let v: Vec<Entry> = (0..8 * 20).map(|i| e(0, i, false)).collect();
        let l = layout(sorted(v), &|w| w == 0, None);
        assert_eq!(l.total, 20);
        let c = Cursor { top: 0, at: 4 * COLS + 3 };
        // R1: a view further down, the cursor row kept.
        assert_eq!(navigate(&l, c, Nav::PageDown, true), Cursor { top: 9, at: 4 * COLS + 3 });
        // Then to the end (the last view), the cursor on its last row.
        assert_eq!(navigate(&l, Cursor { top: 9, at: 4 * COLS + 3 }, Nav::PageDown, true), Cursor { top: 11, at: 8 * COLS + 3 });
        assert_eq!(navigate(&l, Cursor { top: 11, at: 8 * COLS + 3 }, Nav::PageDown, true), Cursor { top: 11, at: 8 * COLS + 7 }, "then to the row's last cell");
        assert_eq!(navigate(&l, Cursor { top: 11, at: 8 * COLS + 7 }, Nav::PageDown, true), Cursor { top: 0, at: 0 }, "and a new press wraps");
        assert_eq!(navigate(&l, Cursor { top: 11, at: 8 * COLS + 7 }, Nav::PageDown, false), Cursor { top: 11, at: 8 * COLS + 7 });
        // L1: a view up; near the top it jumps to the top, at the top it goes to column 0, then wraps.
        assert_eq!(navigate(&l, Cursor { top: 11, at: 3 }, Nav::PageUp, true), Cursor { top: 2, at: 3 });
        assert_eq!(navigate(&l, Cursor { top: 2, at: 3 }, Nav::PageUp, true), Cursor { top: 0, at: 3 }, "the column is kept");
        assert_eq!(navigate(&l, Cursor { top: 0, at: 3 }, Nav::PageUp, true), Cursor { top: 0, at: 0 });
        assert_eq!(navigate(&l, Cursor { top: 0, at: 0 }, Nav::PageUp, true), Cursor { top: 11, at: 8 * COLS + 7 });
    }

    #[test]
    fn the_view_labels_the_sections_that_start_in_it() {
        let l = sample();
        let v = view(&l, 0);
        assert_eq!(v.len(), 5);
        assert_eq!(v.iter().map(|r| r.label).collect::<Vec<_>>(), vec![Some(0), None, Some(1), Some(2), Some(3)]);
        assert_eq!(v[0].cells.len(), 8);
        assert_eq!(v[0].cells[..3], [true, false, true]);
        assert_eq!(v[1].cells, vec![true, false], "the rest of world 0's ten entries");
        assert!(v[2].cells.is_empty(), "the empty row");
        assert_eq!(v[4].cells, vec![true; 3]);
        // Scrolled by one: the section that began above has no label.
        let v1 = view(&l, 1);
        assert_eq!(v1[0].label, None);
        assert_eq!(v1[1].label, Some(1));
    }
}
