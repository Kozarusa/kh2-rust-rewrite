//! The Quests page (9) and the Minigames page (10) of Jiminy's Journal (docs/menus.md 20.10, WP-MN14).
//!
//! Both pages are a list of up to ten rows with a scroll bar. The quest list (`FUN_002b6478`) is built from `jiminy.bar`
//! entry 8 ("ques", 0x10-byte records), the minigame list (`FUN_002b5130`) from entry 7 ("mini", 8-byte records). Records are
//! grouped under a header row per world (and, for quests, per group of the world); a record shows when its state
//! (`FUN_002b6d40` for a quest, the minigame record's state byte for a game) is not 0, or with the all-worlds flag.

use super::marks::Cat;

/// A quest record (entry 8, 0x10 bytes).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Quest {
    /// +0: the journal world.
    pub world: u16,
    /// +2: the group within the world; also the message of the group's header row (`FUN_002b6920`).
    pub group: u16,
    /// +4: the quest's name message.
    pub name: u16,
    /// +6: the test flags (see `quest_state`).
    pub flags: u16,
    /// +8: the number of the story flag or record bit that makes the quest known.
    pub id: u16,
    /// +0xa: the minigame record the quest reads (flag 4 and the score tests).
    pub game: u16,
    /// +0xc: the score or time limit.
    pub limit: u16,
    /// +0xe: the number of the story flag or record bit that marks the quest done (flags 0x10 / 0x20).
    pub done: u16,
}

/// Parses `jiminy.bar` entry 8.
pub fn parse_quests(d: &[u8]) -> Vec<Quest> {
    super::records(d, 0x10)
        .iter()
        .map(|r| {
            let u = |o: usize| u16::from_le_bytes([r[o], r[o + 1]]);
            Quest { world: u(0), group: u(2), name: u(4), flags: u(6), id: u(8), game: u(0xa), limit: u(0xc), done: u(0xe) }
        })
        .collect()
}

/// A minigame record (entry 7, 8 bytes).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Minigame {
    /// +0: the journal world.
    pub world: u16,
    /// +2: the name message.
    pub name: u16,
    /// +4: the message of the unit shown after the score.
    pub unit: u16,
    /// +6: the minigame record number (`FUN_001d36e8`).
    pub game: u16,
}

/// Parses `jiminy.bar` entry 7.
pub fn parse_games(d: &[u8]) -> Vec<Minigame> {
    super::records(d, 8)
        .iter()
        .map(|r| {
            let u = |o: usize| u16::from_le_bytes([r[o], r[o + 1]]);
            Minigame { world: u(0), name: u(2), unit: u(4), game: u(6) }
        })
        .collect()
}

/// What the quest tests read of the game: the story flags (`FUN_001802e0`), the record bits (`FUN_001a5328`), the minigame
/// records (`FUN_001d36e8`: state byte and score word) and the all-worlds flag (`FUN_002783f8`).
pub struct Game<'a> {
    pub story: &'a dyn Fn(u16) -> bool,
    pub bit: &'a dyn Fn(u16) -> bool,
    pub mini: &'a dyn Fn(u16) -> (u8, u32),
    pub all: bool,
}

/// The result of `FUN_002b6d40`: not shown.
pub const HIDDEN: u8 = 0;
/// Known, not done.
pub const OPEN: u8 = 1;
/// Done.
pub const DONE: u8 = 2;
/// Not known, listed only because of the all-worlds flag (its name shows as question marks).
pub const LISTED: u8 = 3;

/// A minigame time word as `FUN_0014c7f0` splits it: (hours, minutes, seconds, hundredths), 60 frames to the second; the last is
/// `trunc(frames * 1.6667f)` (`DAT_00379168`).
pub fn split_time(word: u32) -> (u32, u32, u32, u32) {
    let w = word as i32;
    let rest = w % 216_000;
    let (h, m, s, f) = (w / 216_000, rest / 3600, (rest % 3600) / 60, rest % 60);
    (h as u32, m as u32, s as u32, (f as f32 * 1.666_666_6_f32) as u32)
}

/// `FUN_002b6d40`: the state of a quest. It is known when its flag (+8: a story flag with bit 1 of +6, else a record bit) is
/// set and, with bit 4, its minigame has been seen (state byte not 0). A known quest is done by (checked in this order): bit
/// 0x10 the story flag +0xe, bit 0x20 the record bit +0xe, bit 0x1000 the minigame played (state not 0), bit 0x2000 the
/// minigame state at least 2, else the minigame's score (a state 4 word is a time, taken as whole seconds rounded up) against
/// +0xc: bit 0x100 done when it is at most the limit, bit 0x200 done when it is at least the limit; with none of them the
/// quest is never done (and a minigame state below 2 is not done either). An unknown quest is `LISTED` with the all-worlds flag.
pub fn quest_state(q: &Quest, g: &Game) -> u8 {
    let known = if q.flags & 1 == 0 { (g.bit)(q.id) } else { (g.story)(q.id) };
    let known = known && (q.flags & 4 == 0 || (g.mini)(q.game).0 != 0);
    if !known {
        return if g.all { LISTED } else { HIDDEN };
    }
    let (state, word) = (g.mini)(q.game);
    let done = if q.flags & 0x10 != 0 {
        (g.story)(q.done)
    } else if q.flags & 0x20 != 0 {
        (g.bit)(q.done)
    } else if q.flags & 0x1000 != 0 {
        state != 0
    } else if q.flags & 0x2000 != 0 {
        state >= 2
    } else if state < 2 {
        false
    } else {
        let score = if state == 4 {
            let (h, m, s, f) = split_time(word);
            h * 3600 + m * 60 + s + u32::from(f != 0)
        } else {
            word
        };
        if q.flags & 0x100 != 0 {
            score <= u32::from(q.limit)
        } else {
            q.flags & 0x200 != 0 && score >= u32::from(q.limit)
        }
    };
    if done { DONE } else { OPEN }
}

/// A row of the list.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Row {
    /// A world's header (`FUN_002b6920`: group 100, the world's name).
    World(u16),
    /// A quest group's header inside a world (group 0x66, the message with the group's number).
    Group(u16),
    /// A record, by its index in the entry.
    Item(usize),
}

/// `FUN_002b6478`: the quest list. Every quest whose state is not 0 gets a row; a world's first row is preceded by the world's
/// header, and a row of another group than the last one by the group's header (a group number 0 has none).
pub fn quest_rows(quests: &[Quest], g: &Game) -> Vec<Row> {
    let mut out: Vec<Row> = Vec::new();
    let (mut world, mut group) = (None, 0u16);
    for (k, q) in quests.iter().enumerate() {
        if quest_state(q, g) == HIDDEN {
            continue;
        }
        if world != Some(q.world) {
            out.push(Row::World(q.world));
            (world, group) = (Some(q.world), 0);
        }
        if group != q.group {
            out.push(Row::Group(q.group));
            group = q.group;
        }
        out.push(Row::Item(k));
    }
    out
}

/// `FUN_002b5130`: the minigame list: every record whose state byte is 2 or more (any with the all-worlds flag), a header per
/// world.
pub fn game_rows(games: &[Minigame], g: &Game) -> Vec<Row> {
    let mut out = Vec::new();
    let mut world = None;
    for (k, m) in games.iter().enumerate() {
        if (g.mini)(m.game).0 < 2 && !g.all {
            continue;
        }
        if world != Some(m.world) {
            out.push(Row::World(m.world));
            world = Some(m.world);
        }
        out.push(Row::Item(k));
    }
    out
}

/// `FUN_002b6f00(world)`: some quest of the world (any world with `None`) is listed, or the all-worlds flag is set.
pub fn quests_exist(quests: &[Quest], world: Option<i8>, g: &Game) -> bool {
    quests.iter().any(|q| world.is_none_or(|w| q.world == w as i16 as u16) && (g.all || quest_state(q, g) != HIDDEN))
}

/// `FUN_002b5bb0(world)`: some minigame of the world (any with `None`) has been played (state 2 or more), or the all-worlds
/// flag is set.
pub fn games_exist(games: &[Minigame], world: Option<i8>, g: &Game) -> bool {
    games.iter().any(|m| world.is_none_or(|w| m.world == w as i16 as u16) && (g.all || (g.mini)(m.game).0 > 1))
}

/// The quests that exist now, for the new bits (`FUN_002b7f58`): those whose state is known or done (not 0, not the all-worlds
/// listing).
pub fn quest_present(quests: &[Quest], g: &Game) -> Vec<(Cat, usize)> {
    quests.iter().enumerate().filter(|(_, q)| matches!(quest_state(q, g), OPEN | DONE)).map(|(i, _)| (Cat::Quest, i)).collect()
}

/// The minigames that exist now (`FUN_002b8390`): those whose state byte is 2 or more.
pub fn game_present(games: &[Minigame], g: &Game) -> Vec<(Cat, usize)> {
    games.iter().enumerate().filter(|(_, m)| (g.mini)(m.game).0 >= 2).map(|(i, _)| (Cat::Minigame, i)).collect()
}

/// The keys of every quest of a world (`None`: every world), for the aggregates of the new tags.
pub fn quest_keys(quests: &[Quest], world: Option<i8>) -> Vec<(Cat, usize)> {
    quests.iter().enumerate().filter(|(_, q)| world.is_none_or(|w| q.world == w as i16 as u16)).map(|(i, _)| (Cat::Quest, i)).collect()
}

/// The keys of every minigame of a world (`None`: every world).
pub fn game_keys(games: &[Minigame], world: Option<i8>) -> Vec<(Cat, usize)> {
    games.iter().enumerate().filter(|(_, m)| world.is_none_or(|w| m.world == w as i16 as u16)).map(|(i, _)| (Cat::Minigame, i)).collect()
}

/// `FUN_002ba3a8` (with `FUN_002ba2c0` filling its set): every quest of the world is done.
pub fn quests_complete(quests: &[Quest], world: Option<i8>, g: &Game) -> bool {
    quests.iter().filter(|q| world.is_none_or(|w| q.world == w as i16 as u16)).all(|q| quest_state(q, g) == DONE)
}

/// `FUN_002ba538` (with `FUN_002ba450`): every minigame of the world has been played (state 2 or more).
pub fn games_complete(games: &[Minigame], world: Option<i8>, g: &Game) -> bool {
    games.iter().filter(|m| world.is_none_or(|w| m.world == w as i16 as u16)).all(|m| (g.mini)(m.game).0 >= 2)
}

/// The first row shown (`FUN_002b63c0`, `FUN_002b5078`): the header of the world the journal is on, else 0; kept below the list's
/// last position.
pub fn first_row(rows: &[Row], world: i8, positions: usize) -> usize {
    rows.iter().position(|r| *r == Row::World(world as i16 as u16)).unwrap_or(0).min(positions.saturating_sub(1))
}

/// The scroll positions of a list of `n` rows with ten shown (`FUN_002b5e50`: `n - 9` when `n` is above ten, else one).
pub fn positions(n: usize) -> usize {
    if n > 10 { n - 9 } else { 1 }
}

/// A minigame row's value text as the pieces `FUN_002b5698` joins for the US language (`FUN_00105cd8() == 1`): state 0 and 1 the
/// message 0x4b71; state 4 the unit message and the time `%02d'%02d"%02d` of minutes, seconds and hundredths; otherwise
/// the score as `%d` followed by the message 0x4b80 (state 2) or the unit message.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Value {
    NoScore,
    Time { unit: u16, m: u32, s: u32, f: u32 },
    Score { n: u32, msg: u16 },
}

/// The value of a minigame row from its record.
pub fn value(m: &Minigame, state: u8, word: u32) -> Value {
    match state {
        0 | 1 => Value::NoScore,
        4 => {
            let (_, mm, s, f) = split_time(word);
            Value::Time { unit: m.unit, m: mm, s, f }
        }
        2 => Value::Score { n: word, msg: MSG_PLAYED },
        _ => Value::Score { n: word, msg: m.unit },
    }
}

/// The message after a played minigame's score (`FUN_002b5698`: 0x4b80) and the one of a minigame without a score (0x4b71).
pub const MSG_PLAYED: u16 = 0x4b80;
pub const MSG_NO_SCORE: u16 = 0x4b71;

#[cfg(test)]
mod tests {
    use super::*;

    fn game<'a>(
        story: &'a dyn Fn(u16) -> bool,
        bit: &'a dyn Fn(u16) -> bool,
        mini: &'a dyn Fn(u16) -> (u8, u32),
        all: bool,
    ) -> Game<'a> {
        Game { story, bit, mini, all }
    }

    fn q(world: u16, group: u16, flags: u16, id: u16) -> Quest {
        Quest { world, group, name: 1, flags, id, game: 7, limit: 100, done: 50 }
    }

    #[test]
    fn a_quest_is_known_by_its_flag_and_done_by_the_test_its_flags_pick() {
        let no_bit = |_: u16| false;
        let mini = |_: u16| (0u8, 0u32);
        // Story flag 5 known (bit 1), record bit 6 not.
        let story = |f: u16| f == 5;
        let g = game(&story, &no_bit, &mini, false);
        assert_eq!(quest_state(&q(0, 0, 1, 5), &g), OPEN);
        assert_eq!(quest_state(&q(0, 0, 0, 5), &g), HIDDEN, "bit 1 clear reads a record bit, not a story flag");
        assert_eq!(quest_state(&q(0, 0, 1, 9), &g), HIDDEN);
        // Done by the story flag +0xe (bit 0x10).
        let done_story = |f: u16| f == 5 || f == 50;
        assert_eq!(quest_state(&q(0, 0, 1 | 0x10, 5), &game(&done_story, &no_bit, &mini, false)), DONE);
        assert_eq!(quest_state(&q(0, 0, 1 | 0x10, 5), &g), OPEN);
        // Done by the record bit +0xe (bit 0x20).
        let bit50 = |b: u16| b == 50;
        assert_eq!(quest_state(&q(0, 0, 1 | 0x20, 5), &game(&story, &bit50, &mini, false)), DONE);
    }

    #[test]
    fn the_minigame_tests_read_the_record_state_and_the_score() {
        let (story, bit) = (|f: u16| f == 5, |_: u16| false);
        let state = |s: u8, w: u32| move |_: u16| (s, w);
        let m0 = state(0, 0);
        // Bit 4: the minigame must have been seen to know the quest.
        assert_eq!(quest_state(&q(0, 0, 1 | 4, 5), &game(&story, &bit, &m0, false)), HIDDEN);
        let m1 = state(1, 0);
        assert_eq!(quest_state(&q(0, 0, 1 | 4, 5), &game(&story, &bit, &m1, false)), OPEN);
        // 0x1000: done once played at all; 0x2000: done from state 2.
        assert_eq!(quest_state(&q(0, 0, 1 | 0x1000, 5), &game(&story, &bit, &m1, false)), DONE);
        assert_eq!(quest_state(&q(0, 0, 1 | 0x1000, 5), &game(&story, &bit, &m0, false)), OPEN);
        assert_eq!(quest_state(&q(0, 0, 1 | 0x2000, 5), &game(&story, &bit, &m1, false)), OPEN);
        assert_eq!(quest_state(&q(0, 0, 1 | 0x2000, 5), &game(&story, &bit, &state(2, 0), false)), DONE);
        // The score against +0xc = 100: 0x200 done at or above, 0x100 done at or below; a state below 2 is never done.
        assert_eq!(quest_state(&q(0, 0, 1 | 0x200, 5), &game(&story, &bit, &state(2, 99), false)), OPEN);
        assert_eq!(quest_state(&q(0, 0, 1 | 0x200, 5), &game(&story, &bit, &state(2, 100), false)), DONE);
        assert_eq!(quest_state(&q(0, 0, 1 | 0x200, 5), &game(&story, &bit, &state(1, 500), false)), OPEN);
        assert_eq!(quest_state(&q(0, 0, 1 | 0x100, 5), &game(&story, &bit, &state(2, 100), false)), DONE);
        assert_eq!(quest_state(&q(0, 0, 1 | 0x100, 5), &game(&story, &bit, &state(2, 101), false)), OPEN);
        assert_eq!(quest_state(&q(0, 0, 1, 5), &game(&story, &bit, &state(3, 5000), false)), OPEN, "no score flag: never done");
        // A time word (state 4) is whole seconds rounded up: 100 s = 6000 frames; one frame more counts as 101.
        assert_eq!(quest_state(&q(0, 0, 1 | 0x100, 5), &game(&story, &bit, &state(4, 6000), false)), DONE);
        assert_eq!(quest_state(&q(0, 0, 1 | 0x100, 5), &game(&story, &bit, &state(4, 6001), false)), OPEN);
        // Unknown quests: listed only with the all-worlds flag.
        assert_eq!(quest_state(&q(0, 0, 1, 9), &game(&story, &bit, &m0, true)), LISTED);
    }

    #[test]
    fn a_time_word_splits_into_hours_minutes_seconds_and_hundredths() {
        assert_eq!(split_time(0), (0, 0, 0, 0));
        assert_eq!(split_time(60), (0, 0, 1, 0));
        assert_eq!(split_time(3600 + 120 + 30), (0, 1, 2, 50), "30 frames are 50 hundredths");
        assert_eq!(split_time(216_000 + 59), (1, 0, 0, 98), "59 frames: 98.3 truncated");
    }

    #[test]
    fn the_quest_list_has_a_header_per_world_and_per_group() {
        let story = |_: u16| true;
        let (bit, mini) = (|b: u16| b != 3, |_: u16| (0u8, 0u32));
        let g = game(&story, &bit, &mini, false);
        // The quest at index 2 has record bit 3, which is clear: not listed. Groups 0, 7, 7 in world 1; world 2 starts with group 0.
        let quests = [q(1, 0, 0, 1), q(1, 7, 0, 2), q(1, 7, 0, 3), q(1, 7, 0, 4), q(2, 0, 0, 5), q(2, 9, 0, 6)];
        assert_eq!(
            quest_rows(&quests, &g),
            vec![Row::World(1), Row::Item(0), Row::Group(7), Row::Item(1), Row::Item(3), Row::World(2), Row::Item(4), Row::Group(9), Row::Item(5)]
        );
        assert!(quests_exist(&quests, Some(2), &g) && !quests_exist(&quests, Some(5), &g));
        let none = |_: u16| false;
        let g2 = game(&none, &none, &mini, false);
        assert!(!quests_exist(&quests, None, &g2) && quest_rows(&quests, &g2).is_empty());
        assert!(quests_exist(&quests, Some(2), &game(&none, &none, &mini, true)), "the all-worlds flag lists the world's quests");
        assert!(!quests_exist(&quests, Some(5), &game(&none, &none, &mini, true)), "a world without any record has none");
    }

    #[test]
    fn the_minigame_list_lists_played_games_under_a_world_header() {
        let (story, bit) = (|_: u16| true, |_: u16| true);
        let mini = |n: u16| match n {
            1 => (2u8, 10u32),
            2 => (1, 0),
            3 => (4, 600),
            _ => (0, 0),
        };
        let games = [
            Minigame { world: 1, name: 1, unit: 2, game: 1 },
            Minigame { world: 1, name: 1, unit: 2, game: 2 },
            Minigame { world: 2, name: 1, unit: 2, game: 3 },
            Minigame { world: 2, name: 1, unit: 2, game: 4 },
        ];
        let g = game(&story, &bit, &mini, false);
        assert_eq!(game_rows(&games, &g), vec![Row::World(1), Row::Item(0), Row::World(2), Row::Item(2)]);
        assert!(games_exist(&games, Some(1), &g) && !games_exist(&games, Some(5), &g));
        let all = game(&story, &bit, &mini, true);
        assert_eq!(game_rows(&games, &all).len(), 6);
        assert_eq!(first_row(&game_rows(&games, &g), 2, 3), 2, "the world's header");
        assert_eq!(first_row(&game_rows(&games, &g), 9, 3), 0, "no header: the top");
        assert_eq!(first_row(&game_rows(&games, &all), 2, 1), 0, "kept inside the list's positions");
        assert_eq!((positions(4), positions(10), positions(11), positions(25)), (1, 1, 2, 16));
    }

    #[test]
    fn present_keys_and_completion_follow_the_states() {
        let (story, bit) = (|_: u16| true, |_: u16| true);
        let mini = |n: u16| if n == 1 { (2u8, 9u32) } else { (0, 0) };
        let g = game(&story, &bit, &mini, false);
        // Quest 0 is done by the minigame (state 2, flag 0x2000); quest 1 needs a story flag the closure grants but a done flag
        // that is also granted (0x10): done too; quest 2 reads a minigame at state 0 with flag 0x2000: open.
        let mut a = q(1, 0, 1 | 0x2000, 5);
        a.game = 1;
        let b = q(1, 0, 1 | 0x10, 5);
        let mut c = q(2, 0, 1 | 0x2000, 5);
        c.game = 0;
        let quests = [a, b, c];
        assert_eq!(quest_present(&quests, &g), vec![(Cat::Quest, 0), (Cat::Quest, 1), (Cat::Quest, 2)]);
        assert!(quests_complete(&quests, Some(1), &g) && !quests_complete(&quests, Some(2), &g) && !quests_complete(&quests, None, &g));
        assert!(quests_complete(&quests, Some(7), &g), "a world without quests is complete");
        assert_eq!(quest_keys(&quests, Some(2)), vec![(Cat::Quest, 2)]);
        let games = [Minigame { world: 1, name: 1, unit: 1, game: 1 }, Minigame { world: 1, name: 1, unit: 1, game: 2 }];
        assert_eq!(game_present(&games, &g), vec![(Cat::Minigame, 0)]);
        assert!(!games_complete(&games, Some(1), &g) && games_complete(&games, Some(3), &g));
        assert_eq!(game_keys(&games, None).len(), 2);
    }

    #[test]
    fn a_minigame_value_follows_the_record_state() {
        let m = Minigame { world: 1, name: 1, unit: 0x4c00, game: 1 };
        assert_eq!(value(&m, 0, 0), Value::NoScore);
        assert_eq!(value(&m, 1, 5), Value::NoScore);
        assert_eq!(value(&m, 2, 77), Value::Score { n: 77, msg: MSG_PLAYED });
        assert_eq!(value(&m, 3, 9), Value::Score { n: 9, msg: 0x4c00 });
        assert_eq!(value(&m, 4, 3600 + 61 * 60 + 5), Value::Time { unit: 0x4c00, m: 2, s: 1, f: 8 });
    }
}
