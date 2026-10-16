//! The data side of the journal's character and enemy lists (docs/menus.md 20.11, WP-MN15): the list widget's cursor and scroll
//! rules, the world list of page 0xd, and the reaction-command rows of an enemy's page (0x12 / 0x15).
//!
//! The lists of pages 0xd (the worlds), 0xe (a world's characters), 0x11 (the Heartless) and 0x14 (the Nobodies) are ten list
//! objects over a list of entries (`FUN_002a8a38`, `FUN_002a91a0`); the widget (`FUN_00297148` -> `FUN_002977a8`, `FUN_00298370`)
//! keeps the cursor inside the view and the scroll position of the first row.

use std::collections::HashMap;

use kh2_tables::player::Rcct;

/// The list objects a page makes (`FUN_002a91a0`: `while (i < 10)`).
pub const VIEW: usize = 10;

/// The widget's position: the cursor row inside the view (`+0`) and the scroll (`+0x12`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Pos {
    pub cursor: usize,
    pub scroll: usize,
}

impl Pos {
    /// The cursor's index in the entry list (`FUN_00298e08`).
    pub fn at(self) -> usize {
        self.cursor + self.scroll
    }
}

/// A press the list widget answers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Move {
    Down,
    Up,
    PageDown,
    PageUp,
}

/// The widget's move (`FUN_002978f8` / `FUN_00297af0` for the single steps, `FUN_00298370` for the page steps of ten, `FUN_00299010(list, 10)`):
/// a step inside the view moves the cursor; at the view's edge the view scrolls; past the ends a new press wraps [inferred: the
/// widget's wrap byte `+0x24` is set for these lists, as for the Album's]. A page step moves the view by `step` rows, clamped to the
/// end where the cursor lands on the last row of the view. `count` is the number of entries. Returns the new position.
pub fn step(p: Pos, count: usize, mv: Move) -> Pos {
    if count == 0 {
        return Pos::default();
    }
    let view = VIEW.min(count);
    let last = count - view;
    match mv {
        Move::Down => {
            if p.cursor + 1 < view {
                Pos { cursor: p.cursor + 1, ..p }
            } else if p.scroll < last {
                Pos { scroll: p.scroll + 1, ..p }
            } else {
                Pos::default()
            }
        }
        Move::Up => {
            if p.cursor > 0 {
                Pos { cursor: p.cursor - 1, ..p }
            } else if p.scroll > 0 {
                Pos { scroll: p.scroll - 1, ..p }
            } else {
                Pos { cursor: view - 1, scroll: last }
            }
        }
        Move::PageDown => {
            let at = p.at();
            if at + 1 >= count {
                return Pos::default();
            }
            if at + VIEW >= count {
                // Past the end: the view goes to the end and the cursor to its last row.
                return Pos { cursor: view - 1, scroll: last };
            }
            let scroll = p.scroll + VIEW;
            if scroll + view >= count {
                let scroll = last;
                let cursor = if scroll + p.cursor < count { p.cursor } else { count - scroll - 1 };
                Pos { cursor, scroll }
            } else {
                Pos { scroll, ..p }
            }
        }
        Move::PageUp => {
            let at = p.at();
            if at == 0 {
                return Pos { cursor: view - 1, scroll: last };
            }
            if p.scroll >= VIEW {
                Pos { scroll: p.scroll - VIEW, ..p }
            } else if at >= VIEW {
                Pos { cursor: at - VIEW, scroll: 0 }
            } else {
                Pos::default()
            }
        }
    }
}

/// The families page 0xd lists (`FUN_002aa430` builds the mask: bit 0 always, bit `w + 1` for each world `w` below 0xf whose cell
/// exists or with the all-worlds flag; `FUN_002aa8d0` shows bit 0 as world 0x10 and bit `k` as world `k - 1`). Returns the world byte of each row.
pub fn world_rows(exists: &dyn Fn(u8) -> bool, all: bool) -> Vec<u8> {
    std::iter::once(0x10).chain((0..0xf).filter(|&w| exists(w) || all)).collect()
}

/// One reaction-command row of an enemy's page: the command's name message and its use count (0 for a command never used; the
/// name then shows as question marks).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ReactionRow {
    pub message: i32,
    pub count: u16,
}

/// A command row as the page reads it: the name message (`+8`) and the use-counter slot (`ReactionCount`, `+0x26`).
pub type CmdInfo = (i32, u16);

/// The most uses a row shows (`FUN_002aadf0`: `if (999 < n) n = 999`).
pub const MAX_USES: u16 = 999;

/// `FUN_002aadf0`, pages 0x12 / 0x15: the rows of the enemy with `neo` as NeoStatus. `rcct` is the table of five commands per enemy
/// (`FUN_001d0258`); a command id of 0 is empty; a command shows when it has been used (`FUN_001b7168`) or with the all-worlds flag.
/// `uses` gives a counter slot's count. A command with no row in the command table is skipped [inferred: the original would read the
/// counter through a null row].
pub fn reaction_rows(rcct: &[Rcct], neo: u32, cmds: &HashMap<u16, CmdInfo>, uses: &dyn Fn(u16) -> u16, all: bool) -> Vec<ReactionRow> {
    let Some(row) = rcct.iter().find(|r| u32::from(r.key) == neo) else { return Vec::new() };
    row.commands
        .iter()
        .filter(|&&c| c != 0)
        .filter_map(|c| cmds.get(c))
        .filter_map(|&(message, slot)| {
            let n = uses(slot).min(MAX_USES);
            (n != 0 || all).then_some(ReactionRow { message, count: n })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(cursor: usize, scroll: usize) -> Pos {
        Pos { cursor, scroll }
    }

    #[test]
    fn a_short_list_moves_inside_its_rows_and_wraps() {
        assert_eq!(step(p(0, 0), 3, Move::Down), p(1, 0));
        assert_eq!(step(p(2, 0), 3, Move::Down), p(0, 0), "past the last wraps to the first");
        assert_eq!(step(p(0, 0), 3, Move::Up), p(2, 0));
        assert_eq!(step(p(1, 0), 3, Move::Up), p(0, 0));
        assert_eq!(step(p(0, 0), 0, Move::Down), p(0, 0));
    }

    #[test]
    fn a_long_list_scrolls_at_the_edge_of_the_view() {
        assert_eq!(step(p(8, 0), 25, Move::Down), p(9, 0));
        assert_eq!(step(p(9, 0), 25, Move::Down), p(9, 1), "the view scrolls by one");
        assert_eq!(step(p(9, 15), 25, Move::Down), p(0, 0), "the last entry wraps");
        assert_eq!(step(p(0, 3), 25, Move::Up), p(0, 2));
        assert_eq!(step(p(0, 0), 25, Move::Up), p(9, 15), "above the first wraps to the last");
        assert_eq!(p(9, 15).at(), 24);
    }

    #[test]
    fn a_page_step_moves_the_view_by_ten_and_stops_at_the_ends() {
        assert_eq!(step(p(4, 0), 40, Move::PageDown), p(4, 10));
        assert_eq!(step(p(4, 10), 40, Move::PageDown), p(4, 20));
        assert_eq!(step(p(4, 20), 40, Move::PageDown), p(4, 30), "the last full view");
        assert_eq!(step(p(4, 30), 40, Move::PageDown), p(9, 30), "past the end: the cursor goes to the last row");
        assert_eq!(step(p(9, 30), 40, Move::PageDown), p(0, 0), "on the last entry a new press wraps");
        assert_eq!(step(p(4, 20), 40, Move::PageUp), p(4, 10));
        assert_eq!(step(p(4, 12), 40, Move::PageUp), p(4, 2));
        assert_eq!(step(p(7, 3), 40, Move::PageUp), p(0, 0), "the cursor lands on the first entry");
        assert_eq!(step(p(4, 5), 40, Move::PageUp), p(0, 0), "less than a page above: the first entry");
        assert_eq!(step(p(0, 0), 40, Move::PageUp), p(9, 30), "on the first entry a new press wraps to the end");
        assert_eq!(step(p(3, 0), 5, Move::PageDown), p(4, 0), "a list that fits goes to its last row");
    }

    #[test]
    fn the_world_list_starts_with_world_0x10_and_follows_the_open_worlds() {
        assert_eq!(world_rows(&|w| w == 0 || w == 4, false), vec![0x10, 0, 4]);
        assert_eq!(world_rows(&|_| false, false), vec![0x10]);
        assert_eq!(world_rows(&|_| false, true).len(), 16, "the all-worlds flag lists every world");
    }

    #[test]
    fn the_reaction_rows_show_the_commands_that_were_used() {
        let rcct = [Rcct { key: 1000, commands: [10, 11, 0, 12, 13] }, Rcct { key: 1001, commands: [20, 0, 0, 0, 0] }];
        let cmds: HashMap<u16, CmdInfo> = HashMap::from([(10, (100, 1)), (11, (101, 2)), (12, (102, 3)), (20, (200, 4))]);
        let uses = |slot: u16| match slot {
            1 => 7,
            2 => 0,
            3 => 5000,
            _ => 0,
        };
        let rows = reaction_rows(&rcct, 1000, &cmds, &uses, false);
        assert_eq!(rows, vec![ReactionRow { message: 100, count: 7 }, ReactionRow { message: 102, count: 999 }], "command 13 has no row, 11 was never used");
        let all = reaction_rows(&rcct, 1000, &cmds, &uses, true);
        assert_eq!(all.len(), 3);
        assert_eq!(all[1], ReactionRow { message: 101, count: 0 });
        assert!(reaction_rows(&rcct, 999, &cmds, &uses, true).is_empty());
        assert_eq!(reaction_rows(&rcct, 1001, &cmds, &uses, false), vec![]);
    }
}
