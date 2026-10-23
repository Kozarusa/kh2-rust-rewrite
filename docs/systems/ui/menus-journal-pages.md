# Jiminy's Journal: character pages, text lists, counters, model pages, page turn (20.10 to 20.14)

[Contract](contract.md) | [Index](../../README.md)

### 20.10 A character's own page, Quests, Minigames, page 0xc (WP-MN14) [V unless marked]

Code: `menu_journal.rs` (`build_char_page`, `char_view`, `page_char`), `menu_journal/{games,games_page,root_page}.rs`; the minigame records in
`progress.rs` (slot key `minigames`, docs/progress.md).

- **A character's own page 0xf** (confirm on a face of page 6 / 0xb; `FUN_002ae790` first marks the character seen, `FUN_002b73b8(char)`,
  then `FUN_002aa358(list, page)` keeps the page's face list (the u16 character ids at +0x50, 8-byte stride) and the page to return to
  in `DAT_0036d49c`, stores the face as the page-0xe cursor (`FUN_00277dc8(0xe, min(face, 9), max(face - 9, 0))`) and builds with
  `FUN_002a98f0`). Objects (`FUN_002aadf0`, parent the page header unless said): the layer `DAT_0036d100` (the same object as page 6's
  0x51 layer, re-made) group 0x49; under it the face frame (group = the u16 table 0x382ea0 by the record's style, 10 .. 15) and, from
  `FUN_002ab8f0`, the face picture (the picture `char` +1 of `jmface_<family>.2ld`, groups 1 then 2 as on page 6), both offset by the
  layer group's x / y (`FUN_00138ed0(.., FUN_00237618, FUN_00237638)` [inferred: the group's x / y, as the face pages' bounds]); the name
  (group 0x4a, font 3, `+0x198 = 0x60000008`: the ink [8, 0, 0, 0x60] as page 3's text; the message `char` +6, with the world's name as its
  variable when `+0x10` has 0x400) and the sub-title (0x4b, `char` +0xa when not 0); the description message `char` +8 drawn by
  the list row's callback `FUN_002ab678` in font 3 with the page's pitch and size from the layer group (as page 3: `story_metrics`), a scissor
  of seven lines (`FUN_0014b6c0(x, y, 0x200 - x, pitch * 7)`) and the scroll position as a line offset; the list count is `lines - 6` when
  the text has more than seven lines, else 1 (`FUN_00298d58`); the two arrows 0x4f / 0x50 are shown by `FUN_002ab578` (none with one position,
  the up arrow hidden at the first, the down arrow at the last). Port: `chars::{desc_positions, arrows, desc_scroll}` (tested). Help line
  0x48f2 (`FUN_002ac450`, pages 0xf / 0x12 / 0x15). The list flags (`FUN_002a98f0`: 0x200, plus 0xc000 when more than one listed character
  exists) enable L2 / R2 (-11 / -10); with `FUN_00298fe0(list, 1)` and `FUN_00299010(list, 7)` Up / Down scroll one line and L1 / R1 a page
  of seven [inferred: clamped, as page 3's list].
  Handler `FUN_002a9b80`: -11 / -10 call `FUN_002aa600` (`chars::neighbour`: the next / previous listed character that exists, wrapping
  to the first / last one that does, else the same one; `FUN_002ac4c8` marks it seen) and rebuild the page; -2 (cancel) goes to page 0xe /
  0x11 / 0x14, whose handler `FUN_002a9590` re-enters the page `DAT_0036d49c` with `FUN_002ae3e8(stored cursor)`: the face page with the
  pointer on the character last shown (`first_pointer(.., Some(face))`); -4 closes; -6 opens page 0x10 (the 3D model viewer).
  Run (log lines `journal: page 0xf (FUN_002a98f0): character 116 of 6 (...)`, `journal: page 0xf R2 (-10) (FUN_002aa600) -> character 117`,
  `journal: page 0xf cancel -> page 0x6 with face 4 ...`): `local/x/state-c.json` = `{"story_flags": [2097, 4098, 5122, 6195, 7204, 8193,
  9217, 10243, 2197, 2200, 2201, 2204, 5161]}`, `KH2_KEYS=7:Enter,8:Down,8.2:Down,8.4:Down,8.6:Down,9.2:J,10.5:J,13:J`,
  `KH2_JOURNAL_HOLD=11.5:Down:0.6`, `KH2_SCREENSHOT=.. KH2_SHOT_AT=15`: Hayner's page (the frame group 12 with his picture at the top left
  of the page, the name on the plate, six lines of description and the down arrow; screenshot looked at). Tests:
  `chars::tests::{the_description_scrolls_by_lines_and_pages, the_character_pair_skips_those_that_do_not_exist_and_wraps}`,
  `menu_journal::tests::the_disc_character_page_data` (every character of every family has its name, description and sub-title messages; the
  frame table is 10 .. 15).
  (WP-JR2 ported the 3D model and page 0x10: 20.12.) Formerly not ported: the 3D model (`FUN_002abf60` loads the model `char` +0xc with the placement fields +0x12 .. +0x20 into the object at
  `DAT_0036d494` / `FUN_00295bc0`) and the page 0x10 view (-6, rotate -13 / -12 `FUN_002ac228`), the counter row (group 0x4c, message
  0x489b, `FUN_002a1f78`: the defeated-enemy count of a record with flag 0x800, from `FUN_00187e68(save +0x1660, enemy +0x4c)`), and the
  four drop rows of the enemy pages 0x12 / 0x15 (the enemy lists 0xd .. 0x16 are not ported).
- **Quests (9) and Minigames (10)** (`FUN_002b5e50` / `FUN_002b4b18`, handlers `FUN_002b6120` / `FUN_002b4dd8`). Layers 99 (0x63) and 0x6b
  (holds the scroll bar's track, `FUN_00299878(.., total = max(rows, 10), visible 10)`), ten row objects (`FUN_002b6650` / `FUN_002b52c0`: group 0x68
  or 0x69 for a quest, 0x6a for a game, font 3, the ink 0x60000008, a child 2 for the tag; a game row also has a text child at the row
  group's x / y with the value, right aligned), the list count `rows - 9` when there are more than ten (`FUN_00298d58`), L1 / R1 scroll a page of ten
  (`FUN_00299010(list, 10)`). Row texts and marks (`FUN_002b6920`, `FUN_002b5698`): a world header (group 100, `FUN_002a3070(world, 0)`
  or question marks while the world's cell does not exist, `FUN_002a3120`), a quest group header (group 0x66, the message with the group's
  number, question marks unless some quest of the group is known or done, `FUN_002b6fb8`), a quest row (0x68; 0x69 when done; the name
  `ques` +4, question marks for the all-worlds listing), a game row (the name `mini` +2, question marks below state 2, the value text);
  the tags are the words 0x382ff8 (quests) and 0x382fec (games), both 0x39 (new) / 0x3a (seen); the rows in view are marked seen when
  drawn (`FUN_002b6c20`: the quest's index in entry 8 into the seen set +0x80; `FUN_002b5aa8`: the game's index in entry 7, +0x78). Help
  lines 0x48f6 / 0x48f7. The page opens on the journal world's header row (`FUN_002b63c0` / `FUN_002b5078`: the first row with the world's header, kept inside
  the list count) except from page 0xc, where it opens at the top (pages 0x1b / 0x1c). Cancel goes to page 2 (page 0xc from 0x1b / 0x1c).
  - *The lists.* Quests (`FUN_002b6478`, `jiminy.bar` entry 8 "ques", 0x10-byte records: +0 world, +2 group, +4 name message, +6 test flags,
    +8 the flag / bit, +0xa the minigame record, +0xc the limit, +0xe the done flag / bit): a record is listed when `FUN_002b6d40` gives not 0;
    a header row for a world's first listed record and one for a record of another group than the last one. Minigames (`FUN_002b5130`, entry 7
    "mini", 8-byte records: +0 world, +2 name, +4 unit message, +6 the minigame record number): listed when the record's state byte
    (`FUN_001d36e8(n) = 0x341620 + 8 n`) is 2 or more, or with the all-worlds flag; a header per world. Page 2's rows 5 / 6 need a
    listed quest of the world (`FUN_002b6f00`) / a game with state above 1 (`FUN_002b5bb0`) or the all-worlds flag. Port: `games::{quest_rows,
    game_rows, quests_exist, games_exist, first_row, positions}` (tested).
  - *The quest test* `FUN_002b6d40` (`games::quest_state`): known when the flag +8 is set (a story flag with bit 1 of +6, else the record bit
    `FUN_001a5328`) and, with bit 4, the minigame record's state is not 0; not known: 3 with the all-worlds flag (the name shows as
    question marks), else 0. A known quest is done (2) by the first test of its flags: 0x10 the story flag +0xe, 0x20 the record bit +0xe,
    0x1000 the game's state not 0, 0x2000 state 2 or more, else with a state of 2 or more the score (state 4 is a time, whole seconds
    rounded up: `FUN_0014c7f0` splits the word, 0x34bc0 frames an hour and 0xe10 a minute) compared with +0xc: 0x100 done at or below,
    0x200 done at or above; otherwise 1 (open). The disc's quests use the flags 0x11, 0x21, 0x145, 0x245, 0x246, 0x1041 and 0x2042.
  - *The minigame value* (`FUN_002b5698`, the branch for `FUN_00105cd8() != 1`, which is the USA disc's [inferred: docs/hud.md takes
    value 1 for Japanese]): state 0 / 1 the message 0x4b71; state 2 the message 0x4b80 ("Round") then the score `%d`; state 4 the unit
    message and the time `%02d'%02d"%02d` (minutes, seconds, hundredths = `frames % 60 * 1.6667`); other states the score and then the unit
    message. `FUN_001918a0` maps the apostrophe and the quote to glyphs 55 and 65.
  - *New bits, aggregates, the world mark.* `FUN_002b7f58` (quests: the records whose state is 1 or 2), `FUN_002b8390` (games: state 2 or
    more) feed the seen sets (`Cat::Quest` 5, `Cat::Minigame` 6 in the slot's `journal_seen`); the completion buffers (`FUN_002ba2c0`,
    `FUN_002ba450`) take the quests that are done and the games with state 2 or more, `FUN_002ba3a8` / `FUN_002ba538` ask whether every one
    of a world is in them. Page 0's cell mark 0x9a (`FUN_002baf08`, world below 0xf): the album (`FUN_002ba080`), quests, games, treasures,
    maps, story and characters (`FUN_002baca8`, `FUN_002bad98(0, world)`) and the links (`FUN_002bad98(1, world)`) of the world are all
    complete (`menu_journal::world_complete`); world 0xf adds the limits (`FUN_002ba238`) and the Ansem reports, which have no app data and
    are not drawn. Page 2's rows 5 / 6 get their new tag and completion mark.
  - *Persistence.* `Progress::minigame(n)` / `set_minigame(n, state, score)` (slot key `minigames`, triples). Nothing in the app plays a
    minigame, so only a state file fills them.
  Run (screenshots looked at): `local/x/state-q.json` = the state-c flags plus 2224 and `"minigames": [[14,4,720],[12,1,0],[15,2,150],
  [11,4,3600],[13,3,77],[16,2,5]]`; `KH2_KEYS=7:Enter,8:Down,8.2:Down,8.4:Down,8.6:Down,9.2:J,10:Down,10.2:Down,10.4:Down,10.6:Down,10.8:Down,11.2:J`
  (quests; one more Down for the minigames): the quest page shows "Twilight Town" / "Mail Delivery" / "Try again" / "Complete within 14
  seconds" with the orange mouse marks and the scroll bar; the minigame page shows five rows with "Time: 00'12"00", "Round 150",
  "Time: 01'00"00", "77Time:" (the score of a game whose unit is the message "Time:", as the table has it) and "Round 5". With the all-worlds
  flag (18487) the lists hold 114 quest rows and every minigame, as question marks. Log lines: `journal: page 9 (FUN_002b5e50 / FUN_002b4b18):
  world 0, 21 rows [...]`, `journal: page 9 first row 10`. Tests: `games::tests::*`, `menu_journal::tests::the_disc_quest_and_minigame_data`.
- **Page 0xc** (`FUN_002a7220`, handler `FUN_002a7978`; world 0xf's page, reached from page 0 for an id above 0xe, and by L1 / R1 from
  pages 0 and 2: `hints`). Layers 0x85 and 0x8f, the two `jm_top_jm.2ld` pictures (both in their second group, `FUN_002a7ec0(0)`), the title (world 0xf's
  name), up to eleven rows of the categories listed by `FUN_002a80c0` (`root_page::listed`): 0 the Ansem reports (page 0x17: needs a report
  owned, `FUN_002ade10`), 1 / 2 / 3 the character lists (0xd, 0x11, 0x14), 4 / 5 treasures / maps (0x19, 0x1a), 6 quests (0x1b: listed
  quest), 7 minigames (0x1c: played game), 8 (0x1d: a limit used, `FUN_002b37d8`), 9 (0x1f: the moogle's first word or save word +0x3750,
  `FUN_002b4838`), 10 the character links (0x20); the all-worlds flag lists every category. The row texts are the words at 0x382d08
  (13945 .. 13948, 18650 .. 18656), the help lines (`FUN_002a81e8`, at the cursor) the words at 0x382d20, the new tag groups the words at
  0x382d00 (0x87 / 0x88), the completion mark 0x86 (`FUN_002a7558`). A confirm sets the tab's text from 0x382d08 and opens the row's page
  (`FUN_002a7978`'s switch: 0x17, 0xd, 0x11, 0x14, 0x19, 0x1a, 0x1b, 0x1c, 0x1d, 0x1f, 0x20); the pages the app has are run as
  7, 8, 9, 10 and 0xb with a cancel back to page 0xc (`Open::from_c`; they open at their first row: `FUN_002b1e18` zeroes the scroll of
  pages 0x19 / 0x1a, and `FUN_002b6120` / `FUN_002b4dd8` return to 0xc). The others (the Ansem reports, the heartless and nobody lists,
  categories 8 and 9) were refused with a log line until WP-MN15 (they open since, 20.11). Tests: `root_page::tests::*`, `menu_journal::tests::the_disc_character_page_data` (the
  table words, the groups).
  Run: `local/x/state-q.json`, `KH2_KEYS=7:Enter,8:Down,8.2:Down,8.4:Down,8.6:Down,9.2:J,10:Escape,11:Left,12:J` (screenshot looked at: the
  picture of Jiminy, "Character Files" with NEW, "The Heartless", "The Nobodies", "Treasures", "Maps", "Missions", "Minigames",
  "Character Links", the help line "See character information!", the hint buttons "Select World"); then `13:Down` x5, `14.2:J` the
  quests, `15.5:Escape` back.
- **Gaps (WP-MN14).** The 3D model and the model view page 0x10 of a character (-6); the counter row and the defeated-enemy counts of
  the enemy pages; the enemy lists 0xd .. 0x16 and their drop rows; the Ansem reports (0x17, 0x18, entry 4); categories 8 (0x1d, 0x1e: limits,
  entry 6) and 9 (0x1f: the moogle list, `FUN_002b3d80`); page 0x20 as its own page (it runs as page 0xb); Roxas's own pages 0x19 / 0x1a
  (they run as the world pages 7 / 8, whose world list is the same table [inferred]); the page-turn sound and wipe the handlers start on
  a direction (`FUN_002779f8`); the limits' part of page 0's completion mark for world 0xf (`FUN_002ba238`); the Ansem reports' new tag in
  page 0xc's row 0. Nothing in the app sets the opened-chest bits (WP-MN13) or plays a minigame, so only a state file shows found chests
  and minigame scores.
- **Decompile index (MN14).** `FUN_002aa358`, `FUN_002a98f0`, `FUN_002aadf0`, `FUN_002ab678`, `FUN_002ab7d8`, `FUN_002ab838`, `FUN_002ab8f0`,
  `FUN_002ab578`, `FUN_002ac380`, `FUN_002ac450`, `FUN_002ac4c8`, `FUN_002aa600`, `FUN_002a9b80`, `FUN_002a9590`, `FUN_002aa0b0`, `FUN_002abf60`,
  `FUN_002abdc8`, `FUN_002abef0`, `FUN_002ab540`, `FUN_002a1fc0`, `FUN_002a1f78`, `FUN_002a07c8` (only through its callers), `FUN_00298fe0`,
  `FUN_00299010`, `FUN_002977a8`, `FUN_00297148`, `FUN_002b73b8`, `FUN_002b5e50`, `FUN_002b4b18`, `FUN_002b6120`, `FUN_002b4dd8`, `FUN_002b6478`,
  `FUN_002b6650`, `FUN_002b6920`, `FUN_002b6c20`, `FUN_002b6c00`, `FUN_002b63c0`, `FUN_002b6d40`, `FUN_002b6618`, `FUN_002b6fb8`, `FUN_002b6f00`,
  `FUN_002b5130`, `FUN_002b52c0`, `FUN_002b5698`, `FUN_002b5078`, `FUN_002b5aa8`, `FUN_002b5a88`, `FUN_002b5288`, `FUN_002b5bb0`, `FUN_002b7f58`,
  `FUN_002b8048`, `FUN_002b80c8`, `FUN_002b81a0`, `FUN_002b8390`, `FUN_002b8470`, `FUN_002b84f0`, `FUN_002b85c8`, `FUN_002ba3a8`, `FUN_002ba538`,
  `FUN_002ba2c0`, `FUN_002ba450`, `FUN_002baf08`, `FUN_002a7220`, `FUN_002a7978`, `FUN_002a7558`, `FUN_002a7ec0`, `FUN_002a7d80`, `FUN_002a7e48`,
  `FUN_002a7190`, `FUN_002a80c0`, `FUN_002a81e8`, `FUN_002ade10`, `FUN_002b37d8`, `FUN_002b4838`, `FUN_002b4798`, `FUN_002b47c8`, `FUN_002ad6a0`,
  `FUN_00277740`, `FUN_00277860`, `FUN_00277b20`, `FUN_00277eb0`, `FUN_00277e18`, `FUN_00277dc8`, `FUN_001d36e8`, `FUN_001a5328`, `FUN_0014c7f0`,
  `FUN_00105cd8`, `FUN_001918a0`, `FUN_002b1e18`, `FUN_002b16a0`, `FUN_002b1650`.

### 20.11 The text lists, enemy pages, Ansem reports, limits and moogle (WP-MN15) [V unless marked]

Code: `menu_journal/{name_list,enemies,report_page,moogle_page,kills}.rs` (`enemies`, the data parts of the others and `kills` hold the pure
rules with tests; the rest is node glue), `chars.rs` (the record family, `enemies_present`, `family_list`), `marks.rs` (`Cat::Report`,
`Cat::Limit`, `Cat::Moogle`), `progress.rs` (the counters below). Page 0xc now opens every row it lists: nothing is refused any more.

- **Defeated-enemy counts (the writer).** The enemies' defeat method `FUN_00184a30` (reached by `FUN_001acfe0` and `FUN_001b02f0`) calls
  `FUN_00187e08(0x33fec0, id)` with `id = FUN_001699a8(actor)` = the NeoStatus (`objentry` `+0x4c`) of the defeated object. The function does
  nothing unless story flag 0x8d0 is set (`FUN_00180850` -> `FUN_001802e0`), finds the word with `FUN_00187dc8` (`save + 0x1660 + 0xff0 + 4 *
  (id - 1000)` for ids 1000 .. 1046, `+ 0x1170 + 4 * (id - 3000)` for 3000 .. 3011, nothing else) and adds 1 up to 9999. The pickup method
  `FUN_001c46a8` counts the ids 0x404 / 0x405 when the player absorbs a pickup-like object (24.4a) [the journal pages do not read those slots]. The reader is `FUN_002a1f78(record)`:
  for a `char` record with flag 0x800 it takes the objentry named by the record's `+0xc` (`FUN_0016e208`) and returns the counter of its NeoStatus
  (`FUN_00187e68`). Port: `Progress::{enemy_counter_id, enemy_defeats, count_defeat, set_enemy_defeats}` (slot key `enemy_kills`, pairs
  `[NeoStatus, count]`), `menu_journal::kills`: a system watches the `Enemy` components, counts each one whose HP has reached 0 once (the `Counted`
  marker) under the NeoStatus of its spawn record's object id (`NeoByObject`, read from `00objentry`; an enemy of the harness's own waves has
  object id 0 and counts nothing). The gate is the original's: in a plain run (no state file) the app's baseline leaves story flag 0x8d0 clear, so no count rises until a state file or an area script sets it. The app's defeat point is outside this work package's files; the hook is read-only. Tests
  `progress::tests::enemy_defeat_counters_count_and_are_saved`, `kills::tests::*`, `menu_journal::tests::the_disc_enemy_list_data` (every one of
  the 56 enemy records has a distinct objentry NeoStatus with a counter slot).
- **Existence of a record** (`FUN_002a1c30`, run by `FUN_002b7160` and by page 0xd's mask): the first loop sets the bit of every `char` record whose
  `FUN_002a1f78` is not 0, that is the enemy records (flag 0x800) of an enemy defeated at least once; the second loop (ported by MN12) the
  sub-records of every group that exists. `name_list::exist_set` is the union; it feeds the new bits, the lists and the completion marks. The record
  index is also its bit number (`+4`, checked on the disc).
- **Record families** (`char` `+0`): 0 .. 0xe the worlds, 0x10 the links, 0x11 the Heartless (57 records, 46 with the counter flag), 0x12 the
  Nobodies (11, 10 with it). `FUN_002ab7d8` names the face file by it: `ex` for 0x10, `heart` for 0x11, `no` for 0x12 (the table at 0x36d3f8).
- **Page 0xd** (`FUN_002a8a38`, handler `FUN_002a8e90`; page 0xc row 1 "Character Files"). Ten list objects (layers 0x58 and 0x5d, the second holds the
  scroll bar's track, pitch from group 0x58), one row per world: a mask built by `FUN_002aa430` (bit 0 always, bit `w + 1` for each world below
  0xf whose cell exists or with the all-worlds flag); bit 0 shows world 0x10's name ("Other"), bit `k` world `k - 1`. A world whose cell does not
  exist shows question marks. The tag is the aggregate of the family's records (`FUN_002b7408` any new, `FUN_002b74b8` all seen; the words at
  0x382e90 / 0x382e94 = groups 0x5a / 0x5b). A confirm keeps the row and the scroll (`FUN_00277dc8(0xd, ..)`), names the chosen world on a second
  plate (`FUN_0029d820(1, ..)`) and opens page 0xe at its top; a cancel goes to page 0xc. Coming from page 0xc the cursor starts on the first row
  (`FUN_002a8948` sets the flag that picks the journal's own world, and world 0xf has no row).
- **Pages 0xe, 0x11, 0x14** (`FUN_002a91a0`, `FUN_002aaae0`, handler `FUN_002a9590`; `FUN_002ac370(0x11 / 0x12)` selects the Heartless and Nobody family).
  `FUN_002aa528` lists the records of the family that exist (or all with the all-worlds flag: the unknown ones show question marks and no tag,
  `FUN_0029bb48`), each name from the record's `+6` (flag 0x400: the world's name is the message's variable). With no entry the single row reads the
  message 0x36b3, the hand is hidden and nothing can be chosen. The redraw (`FUN_002aa848`) also marks the entry under the cursor seen
  (`FUN_002ac4c8`: `FUN_002b73b8`, and its tag goes to the seen group at once). A confirm opens the character's page: 0xf from 0xe, 0x12 from 0x11,
  0x15 from 0x14 (`FUN_002aa358` keeps the list; `DAT_0036d49c = 0xff` makes the cancel return to the list page with the cursor on the character
  shown, L2 / R2 `FUN_002aa600` moving it). A cancel goes to 0xd from 0xe and to 0xc from the others. The widget (`FUN_002977a8` -> `FUN_002978f8`
  / `FUN_00297af0`, page steps `FUN_00298370` with `FUN_00298fe0(list, 1)`, `FUN_00299010(list, 10)`): a step inside the ten rows moves the
  cursor, at the edge the view scrolls by one, past the ends a new press wraps [inferred: the wrap byte of these lists, as the Album's]; L1 / R1 move the
  view by ten, to the end then to the last row, `enemies::step` (tested). The help lines are 0x48ee, 0x48ef, 0x48f0, 0x48f1 (`FUN_002ac450`).
- **An enemy's page (0x12, 0x15)** is the character page 0xf (`FUN_002a98f0`, `FUN_002aadf0`) plus the counter row and the reaction rows. The
  picture is group 1 then 2 whatever the family (`FUN_002ab8f0`; the links pages used 4 / 5 before this work package: fixed). *Counter row* (records with flag
  0x800, group 0x4c): the label is the message 0x489b ("Slain"), the value a child at the group's x / y (136, 0), right aligned, font size 15 (group `+0x18`),
  the group `FUN_00278418(2)` = 0x7f's colour, the text `FUN_001918a0(buf, 0x382de0, count)` with the format `%d`. *Reaction rows* (pages 0x12 and
  0x15 only): `FUN_001d0258(NeoStatus)` binary-searches the `rcct` table (five command ids per enemy); each command with a row in the command table
  (`FUN_001b6f48`) is listed when its use count is not 0 (`FUN_001b7168`: the u16 array at `0x3410b0` indexed by the command's `ReactionCount`, capped
  at 999) or with the all-worlds flag; group 0x4d, one pitch apart (the pitch is negative, -27: the rows stack upwards), the name from the command's
  `MessageId` (question marks while the count is 0) and the count in the same child layout. [inferred: a command row of `CmdAction` 2 whose id is not 2 is
  followed to the party member's own command by `FUN_001a4dd0` in `FUN_001b6f48`; the port lists the row as found.] Port: `enemies::reaction_rows`
  (tested), `menu_journal::enemy_rows`. The counts live in `reaction_cmd::ReactionUses`; an accepted reaction also writes the saved copy (`reaction_cmd::bump_use` ->
  `Progress::set_reaction_use`, slot key `reaction_uses`, pairs `[slot, count]`; WP-HK1) and the journal shows the larger of the two. Run (looked at): `local/x/state-e.json` = the character
  state's flags (docs 20.10) plus 2256 and `"enemy_kills": [[1000,12],[1001,3],[1002,7],[1034,1],[1013,2],[3000,4],[3003,9]]`,
  `"reaction_uses": [[2,5],[41,12]]`, `KH2_KEYS=7:Enter,8:Down,8.2:Down,8.4:Down,8.6:Down,9.2:J,10:Escape,11:Left,12:J,13:Down,14:J,15:Down,15.3:Down,16:J`
  (the Heartless list, Large Body's page: "Slain 7", "Guard 12", "Kickback 5"). Log lines `journal: page 0x11 (...)`, `journal: the counter row (group 0x4c,
  FUN_002a1f78): 7 defeated`, `journal: enemy page: NeoStatus 1002, 2 reaction rows [...]`. The 3D model and page 0x10 / 0x13 / 0x16 came in WP-JR2 (20.12).
### 20.11a The counter writers (WP-HK1)
- **Limit best counts.** `FUN_001b32d8` (and its twin `FUN_001b3178`), the limit task's end (`FUN_001b2850`, `FUN_001b3798`, `FUN_001b35e8`), calls `FUN_001d25b0(*task, task[0x28])`: the
  `limt` `Id` byte (`*task`, the constructor `FUN_001b2c10` stores the record pointer) indexes the u16 array `0x3414b0`, which rises to the word at task `+0xa0`. That word is
  `+0x38` of the gauge object at task `+0x68`: `FUN_001b2e08` sets the gauge's range 0 .. 999 (`FUN_001cf8a0`) and raises the marker to the stored best (`FUN_001d00c0`
  with `FUN_001d2590`); `FUN_001cffa8` adds to the gauge's value (`+0x2c`) and raises the marker to it (the only writers of `+0x38` besides the resets `FUN_001cfdc0` /
  `FUN_001cfe28`, found with `mips_scan.py --store-off 0x38`). [inferred] The value is the limit's hit count: `FUN_001cffa8` has no direct caller (virtual). Port:
  `limit::LimitHits` (`start`, `add` = `FUN_001cffa8`, `finish` = `FUN_001d25b0` through `Progress::raise_limit_best`), called from `limit::try_limit` when the duration ends.
  Gap: the limit task body is not ported, so nothing adds hits during a limit; `KH2_LIMIT_HITS=<n>` adds `n` at the start (a debug aid). Test `limit::tests::the_end_of_a_limit_raises_the_slots_best`.
- **Reaction use counts.** `reaction_cmd::accept` calls `bump_use` (live `ReactionUses` and `Progress::set_reaction_use`); test `reaction_cmd::tests::a_use_is_counted_live_and_saved`.
- **Defeats.** `FUN_00184a30` runs on each entry into the defeat mode (`FUN_001acfe0`, the only direct caller; others are virtual). `kills::count_generic` counts an object whose HP is 0 once and
  re-arms it when its HP is above 0 again [inferred: a revive re-enters the defeat mode]; the app has no reviving enemy yet. Tests `kills::tests::the_system_counts_a_defeat_once_and_only_with_the_flag`, `a_revived_enemy_counts_each_defeat`.
- **Page 0xc's marks** (`FUN_002a7558`): row 1 any new bit in the families 0 .. 0xe and 0x10, complete when `FUN_002b9d88` finds every record of those
  families existing; rows 2 and 3 the families 0x11 and 0x12 the same way; row 0 the reports (`FUN_002b7668(-1)`, complete `FUN_002b9f10`); row 8 the
  limits; row 9 the moogle (`FUN_002b8fd0(-1)`, `FUN_002b9058(-1)`, `FUN_002bab48(-1)`).
- **Ansem reports (pages 0x17, 0x18)**. `jiminy.bar` entry 4 ("JMAN", 13 records of 8 bytes: +0 the item, +2 the title message, +4 the text message). Page
  0x17 (`FUN_002ac900`, handler `FUN_002accb8`, rows `FUN_002ad760`, redraw `FUN_002adc18`): the reports whose item is owned (`FUN_002a1ff8`) or all with
  the all-worlds flag (question marks), the new tag from the bits at `+0x120` (words 0x382edc / 0x382ee0), the cursor's entry marked seen
  (`FUN_002ade80` -> `FUN_002b77c0`), help 0x48ec. Page 0x18 (`FUN_002acfb8`, handler `FUN_002ad3d0`, text `FUN_002adcb0`): layers 0x5f and 0x62
  (the scroll bar's track), the title centred in group 0x60 (font 3), the text nine lines at a time (the list count is the lines minus eight when
  there are more than nine), Up / Down one line, L1 / R1 nine [inferred: clamped], L2 / R2 (-11 / -10, `FUN_002ad998`, when more than one is owned) the
  previous / next owned report, a cancel back to 0x17 with the cursor on it, help 0x48ed. Page 0xc lists row 0 when a report is owned
  (`FUN_002ade10`). Port: `name_list::report_rows`, `report_page`. Run: the state above plus `"items": {"226":1,"227":1,"228":1,"233":1}`,
  `12:J,13:J,14:Down,14.5:J,16:E` (the second report, "Secret Ansem Report 2", scrolled a page).
- **Limits (pages 0x1d, 0x1e)**. `jiminy.bar` entry 6 (21 records of 8 bytes: +0 the limit's command id, +2 the name, +4 the text). The use count of a
  limit (`FUN_002a20e0`: above 0 = used) is `u16[0x3414b0 + 2 * slot]` where `slot` is byte 0 of the `limt` row whose `Command` is the record's id
  (`FUN_001d2518`, `FUN_001d2590`; the table has 21 slots, some commands twice). The writer is `FUN_001d25b0(row, value)` from the limit's end
  (`FUN_001b32d8`, `FUN_001b3178`): the slot is raised to `value = limit object +0xa0` when it is below. The meaning of that field is not traced
  [the page prints it with the unit message 0x4b06 ("Hits")]. WP-CT1 traced the field: it is the record marker of the gauge object at the limit task's `+0x68`, raised by `FUN_001cffa8`, whose
  direct caller `FUN_001b3dc8` adds 1 while a limit task exists; that is called from the enemies' receive function `FUN_001b21f0` (the enemy hit method
  `FUN_001849d8`) when the hit record is effective (`FUN_0018ba88`). So the count is the effective hits on enemies during the limit; `limit.rs` `count_limit_hits`
  counts the app's enemy hit messages and `LimitHits::finish` stores it with `Progress::raise_limit_best` (slot key `limit_best`, pairs `[slot, count]`).
  The other `FUN_001cffa8` callers (`FUN_001d6f28`, from `FUN_0019edb0`, `FUN_0019ee48`, `FUN_002e3fe8`) feed the gauge of `FUN_001b9d60` (not the limit task) and are not ported.
  Page 0x1d (`FUN_002b2130`, rows `FUN_002b30d8`, handler `FUN_002b25a8`): layers 0x6c and 0x6d, the used limits (or all: question marks and a count
  of 0), each with a right-aligned child at the layer group's x / y that reads the count (`FUN_001918a0(buf, 0x382f68, n)`, format `%d`) and the
  unit message 0x4b06, tags 0x39 / 0x3a (the words at 0x382f98), help 0x48f8. Page 0x1e (`FUN_002b2878`, handler `FUN_002b2d78`) is the report page
  with the title group 0x61 (left aligned: no alignment is set) and the count right-aligned at that group's x / y, help 0x4b00. Page 0xc lists row 8
  when some limit is used (`FUN_002b37d8`); the tab reads "Combo Attacks". Run: `"limit_best": [[5,14],[4,22],[13,3]]`, rows 6 of page 0xc, `14:J`.
- **The moogle page (0x1f)**. `FUN_002b3d80` (layer 99), rows `FUN_002b4170`, handler `FUN_002b3f10`, help 0x48f9. The synthesis record (`Moogle`):
  row 0 "Moogle level N" with the level's name (`level + 1` through `%d`; names at 0x382fc0, messages 0xbcd2 ..) at the right; rows 1 .. 3 the
  materials known (of 50), the collection rewards paid (of the 50 `cond` rows) and the creations made (bits 0 .. 44 of `made`, of the recipes' entries:
  one each, two with a higher rank), as `%d` or, with the all-worlds flag, `%d/%d` (0x382fa0, 0x382fa8). Rows 0 and 3 are listed when the moogle is
  available (unlock bit 0 or 10, `FUN_002b4798`), rows 1 and 2 when some material was handed over (`FUN_002b47c8`); with the all-worlds flag an unlisted
  row reads the message 0x36f8 (question marks). A row is complete (second group 0x69 instead of 0x68, `FUN_002ba978`) at level 8, 50 materials, 50
  rewards, 45 creations. The new bits of the listed rows are made at the scene's open (`FUN_002b8f18`) and every row is marked seen when the page is
  built (`FUN_002b9138`), so a row that was new shows the seen tag 0x3a. Page 0xc lists row 9 when `FUN_002b4838` holds (available, or a material, or
  the all-worlds flag). Port: `moogle_page`; the totals come from `mixdata.bar` (`FUN_002b48c8`). Run: `"moogle": {"unlocks":[0],"level":3,
  "known":[0..11],"rewards":[0,1,2],"made":[0,1,2,3,4]}` ("Moogle level 4 Skilled Moogle", 12, 3, 5), `12:J`, seven Downs, `J`.
- **World 0xf's mark 0x9a (item D).** Page 0's cell builder `FUN_002a2248` picks the mark: for every record below 0x10 whose cell exists
  it calls `FUN_002baf08(world)` (its only caller) and draws the literal group 0x9a on the cell's first child when it answers yes. World 0xf
  asks (`FUN_002baf08`, `menu_journal::journal_complete`): every character record exists (`FUN_002b9d88(-1)`), the 13 reports are owned (`FUN_002b9f10`), every
  limit is used (`FUN_002ba238`: the completion bits at `+0x74`), every quest, minigame, treasure and map of every world is complete, the moogle's
  four rows are complete (`FUN_002bab48(-1)`: the bits at `+0x124` set by `FUN_002ba978`) and the links are complete (`FUN_002bad98(1, 0xf)`).
- **Pages 0x19, 0x1a and 0x20 are not Roxas's.** They are page 0xc's own numbers for the Treasures, Maps and Character Links builders (`FUN_002afca0`,
  `FUN_002afe98`, `FUN_002ae3e8` / `FUN_002ae790`); the only places that read the page number are `FUN_002b1e18` (the scroll starts at 0 for 0x19 / 0x1a),
  `FUN_002b16a0` (the treasure test: page 7 or 0x19) and the cancel target (page 0xc for 0x19, 0x1a, 0x20, else page 2). The MN14 port already runs them as
  7, 8 and 0xb with `from_c` set, so nothing further is needed [V by reading the three handlers and builders].
- **Gaps (WP-MN15).** The writer of the limit counters (the limit object's `+0xa0` and where the app ends a limit); the counters of ids 0x404 / 0x405
  (`FUN_001c46a8`, a pickup-like object the player absorbs, not a defeat) have no reader here; the hook that calls `Progress::count_defeat` is a system in `kills.rs` reading the
  `Enemy` components, not the defeat method; the reaction use counts are not written by `reaction_cmd.rs`; the 3D model and the model pages (0x10,
  0x13, 0x16) of a character or an enemy; the page-turn sound and wipe (WP-JR1: `FUN_002779f8` has no sound, see above); the held-pad auto-repeat of the list cursors; the exact right edge of the
  right-aligned values (WP-JR1 moved the text off the limit row's tag).
- **Journal polish (WP-JR1).** (1) A limit row's NEW tag (`FUN_002b2130`: the tag object is made with group 2, hung from the value object and never
  moved; `FUN_002b30d8` only switches its group to 0x39 / 0x3a) is centred on the value's origin, which is the right end of the right-aligned text,
  so it covered the last 23 units of "Hits" [V: the quads of group 0x39 span x -23 .. 23]. The original's right edge of the text is not recoverable from
  the code [GAP]; the port ends the text at the tag's left edge (`name_list::tag_left_reach`, test `a_limit_value_ends_where_its_tag_begins`) [inferred]:
  the tag cannot sit right of the text because the scroll bar starts 12 units after its right edge. (2) The left sheet of every journal page that has one (worlds 0xd, characters, story, album, treasures, quests, minigames, the character's own page 0xf; group 88 / 94 / 99 / 81 / 73 ..., box -148 .. 501, or .. 383 on page 0xf) is carried to the window's left edge by `stretch_quads` like the frame's rail (`menu_journal::sheet_stretch`, run for every journal node after each page build by `carry_sheets`, test `the_sheet_box_is_carried_whole`) [inferred]; the rule is a quad set starting left of x -100 and at least 400 wide (the book's base starts at -80 and stays). Looked at 2560x1080 and 1280x720: pages 0xd, 3 (story), 6 (characters), 4 (album), 7 (treasures), 9 / 10 (quests, minigames), 0xf at 2560x1080 reach the window edge, 1280x720 unchanged. Pages 0, 1 / 2 and 0xc have no left sheet (the dark book), so nothing is open. (3) The page-turn sound and wipe: `FUN_002779f8(dir, mode)` only switches the header object's group (table 0x381fa8 / 0x381fb0,
  `FUN_00138e78` or `FUN_0029b600` with a group, then writes +0x17c); it plays no sound. Nothing was ported: the sound the handlers play on a direction (if any) is
  not in that function [GAP: callers' sound calls not traced], and the header's second / third group animation is not in the port.
- **Decompile index (MN15).** `FUN_00184a30`, `FUN_001acfe0`, `FUN_001b02f0`, `FUN_001c46a8`, `FUN_00187dc8`, `FUN_00187e08`, `FUN_00187e68`,
  `FUN_00180850`, `FUN_001802e0`, `FUN_001699a8`, `FUN_0016e208`, `FUN_002a1f78`, `FUN_002a1c30`, `FUN_002a1c10`, `FUN_002a1fc0`, `FUN_002a1ff8`,
  `FUN_002a20e0`, `FUN_002a8958`, `FUN_002a8a38`, `FUN_002a8e90`, `FUN_002a8948`, `FUN_002a8568`, `FUN_002a84d8`, `FUN_002a91a0`, `FUN_002a9590`,
  `FUN_002aa430`, `FUN_002aa4b0`, `FUN_002aa528`, `FUN_002aa600`, `FUN_002aa848`, `FUN_002aa8d0`, `FUN_002aaae0`, `FUN_002ab8f0`, `FUN_002ac370`,
  `FUN_002ac450`, `FUN_002ac4c8`, `FUN_002aadf0`, `FUN_001d0258`, `FUN_001b6f48`, `FUN_001b7168`, `FUN_001a4dd0`, `FUN_00278418`, `FUN_001918a0`,
  `FUN_0029bb48`, `FUN_0029d1d0`, `FUN_0029d820`, `FUN_0029d3e8`, `FUN_0029cf78`, `FUN_00297148`, `FUN_002977a8`, `FUN_002978f8`, `FUN_00297af0`,
  `FUN_00298370`, `FUN_00296fb8`, `FUN_00298d58`, `FUN_00298fe0`, `FUN_00299010`, `FUN_002ac900`, `FUN_002accb8`, `FUN_002acfb8`, `FUN_002ad3d0`,
  `FUN_002ad6a0`, `FUN_002ad760`, `FUN_002ad998`, `FUN_002adba0`, `FUN_002adc18`, `FUN_002adc78`, `FUN_002adcb0`, `FUN_002ade10`, `FUN_002ade50`,
  `FUN_002ade80`, `FUN_002b7668`, `FUN_002b76e8`, `FUN_002b77c0`, `FUN_002b2130`, `FUN_002b25a8`, `FUN_002b2878`, `FUN_002b3018`, `FUN_002b30d8`,
  `FUN_002b3640`, `FUN_002b37d8`, `FUN_002b3878`, `FUN_002b38a8`, `FUN_001d2518`, `FUN_001d2590`, `FUN_001d25b0`, `FUN_001b32d8`, `FUN_002b3d80`,
  `FUN_002b3f10`, `FUN_002b3bc0`, `FUN_002b4170`, `FUN_002b4778`, `FUN_002b48c8`, `FUN_002b4798`, `FUN_002b47c8`, `FUN_002b4838`, `FUN_002b8f18`,
  `FUN_002b8fd0`, `FUN_002b9058`, `FUN_002b9138`, `FUN_002b99d0`, `FUN_002b9b28`, `FUN_002b9bf0`, `FUN_002b9d88`, `FUN_002b9f10`, `FUN_002ba238`,
  `FUN_002ba978`, `FUN_002babb8`, `FUN_002bab48`, `FUN_002baf08`, `FUN_002a2248`, `FUN_002b1e18`, `FUN_002b16a0`, `FUN_002afe98`, `FUN_00237658`,
  `FUN_00237678`, `FUN_00237698`; data 0x382de0, 0x382e90, 0x382edc, 0x382f68, 0x382f98, 0x382fa0 .. 0x382fc4, 0x3410b0, 0x3414b0, 0x36d3f8.
- **Tests (WP-MN15).** `enemies::tests::*` (the widget's moves, the world rows, the reaction rows), `kills::tests::*`, `chars::tests::{an_enemy_record_exists_once_the_enemy_was_defeated,
  a_family_list_has_the_records_that_exist_or_all_with_the_all_worlds_flag}`, `name_list::tests::*`, `report_page::tests::*`, `moogle_page::tests::*`,
  `marks::tests::every_category_keeps_its_saved_number`, `menu_journal::tests::{the_disc_enemy_list_data, formats_are_written_with_the_fonts_glyphs}`,
  `progress::tests::{enemy_defeat_counters_count_and_are_saved, reaction_use_counts_are_saved, limit_best_counts_only_go_up_and_are_saved}`,
  `root_page::tests::*`.

### 20.12 The model on a character's page and the model pages 0x10 / 0x13 / 0x16 (WP-JR2) [V unless marked]

Code: `menu_journal/model_view.rs`, `chars::{CharRec, ModelPlace}`, `menu_journal::{build_model_page, shown_page, char_tabs}`.

- **Which pages.** Pages 0xf / 0x12 / 0x15 (character, Heartless, Nobody; the port runs all three as 0xf) show the small model of the shown record
  (`FUN_002a9b80` calls `FUN_002abf60(0)` while it has none). The list result -6 (the triangle: help "Use triangle for the full picture!") opens
  0x10 / 0x13 / 0x16 (`FUN_002aa0b0`), which show the large model of the same record (`FUN_002abf60(1)`) over the tab; -2 returns, -4 closes. The Album's
  picture page (5) shows a picture, not a model: not applicable. Port: key Y = -6 [inferred: the original uses the triangle button; Y is the app stand-in as on the other menus].
- **Record fields** (`jiminy.bar` entry 3, 0x24 bytes): +0xc object id, +0xe motion (133 in all 309 records), +0x10 flags (low ten bits a mesh-group mask),
  small model: x +0x12, y +0x14 (offsets from the screen middle), turn +0x16 (degrees), scale float +0x1c; large model: x +0x18, y +0x1a, scale +0x20, no turn.
  Depth `DAT_00382df8` = -100. Object 996 (list 0x382e68, halfwords) stands at the float + -100 with scale `DAT_00382dfc` = 1 [V disc: Lumiere].
- **Files.** `FUN_0016e208` maps the id to its `00objentry` row (every record of the disc has an ordinary row). Model `obj/<ModelName>.mdlx` (0x37a430); motion
  set `FUN_0016eda8` with the id's bit 0x10000000: `obj/<set stem>_MEMO.mset`, or `obj/<ModelName>_MEMO.mset` when the row names no set (0x37a468, 0x37a450 + 0x37a458).
  Motion 133 is entry 133 of an ordinary set and entry 133 * 4 + 2 of a player (P_*) set (`FUN_001a7a48`) [V disc test: all 309 records resolve].
- **Camera.** The preview object's slot camera of 16.8 (`FUN_0023a520`); the journal never calls `FUN_0023b448`, so no 0x60 tint (the camp's and Party's only).
- **Part mask.** `FUN_0023b640` calls `FUN_0011c5b0(model, bit)` for each set bit of the mask (flags & 0x3ff, shifted by 10 or 20 with flags 0x1000 / 0x2000), which
  clears the draw flag of the meshes whose group (bits 6..10 of the mesh header's flags) is that bit (`FUN_00121278`). Hayner's mask 2 hides his group-1 mesh.
- **Turn.** Small model: +0x16 degrees. Large: Left / Right (-12 / -13, list flags 0x30000) +-2 degrees a frame (`FUN_002ac228`, `DAT_00382e08`); object 2300 (list
  0x382e60) stops at +-16, others wrap at +-360. Positive turn faces the screen's left [inferred: with it the MEMO poses, which stand turned right, face the viewer].
- **Port.** One camera (layer 10) renders into a canvas-sized picture, shown by a node made after the layer (texts and rows stand in front [inferred]).
  Tests: `model_view::tests::*` (page to model, poses, mask, turn, projection, `the_disc_journal_models`), `chars::tests::a_record_carries_its_model_placements`,
  `menu_journal::tests::the_view_button_opens_the_model_page_and_the_model_page_returns`. Run (state-c flags as 20.10; `KH2_JOURNAL_HOLD=16:Left:0.5` holds Left):
  `KH2_KEYS=7:Enter,8:Down,8.2:Down,8.4:Down,8.6:Down,9.2:J,10.5:J,13:J,15:Y` -> log `journal: page 0x10 model (FUN_002abf60(1)): character 116, object 1309 ...`;
  screenshots looked at: Hayner frontal at the right of his page, the large model on the maroon page and turned, Large Body (Heartless) on 0x12 and 0x13 at 1280x720, Hayner at 2560x1080.
- **Gaps.** The viewer plate (`FUN_002aba90` groups 4 / 5, `FUN_002abc40` groups 7 / 8 on the layer) and its alpha: `FUN_0023b4e0` takes the model's alpha from the attached
  plate (fade in / out not ported; the port draws full alpha, and the existing backdrop shows behind). Draw order against the page's 2D is [inferred]. -6's sound
  [inferred: confirm]. Held rotation is the held direction, 2 degrees a 60 Hz tick [inferred]. The second sub-model (`FUN_0016ed50`; group bits 0..4) is not drawn.
  `FUN_002aa0b0` -2 passes 0x15 from both 0x13 and 0x16 (0x2aa2ec compares only 0x10), so the original's Heartless view returns to page number 0x15 (whose -2 goes to the
  Nobody list): ported in WP-JR3 (20.13). A wide model (Large Body) is cut at the 4:3 canvas edge, as the screen edge would. Lighting not read.
- **Decompile index (JR2).** `FUN_002aa0b0`, `FUN_002abf60`, `FUN_002abdc8`, `FUN_002abef0`, `FUN_002ac228`, `FUN_002ab540`, `FUN_002abc40`, `FUN_002aba90`, `FUN_002ac660`,
  `FUN_002ac6b8`, `FUN_0023a520`, `FUN_0023a920`, `FUN_0023aca8`, `FUN_0023b3c8`, `FUN_0023b408`, `FUN_0023b4b8`, `FUN_0023b4e0`, `FUN_0023b640`, `FUN_0011c5b0`,
  `FUN_00121278`, `FUN_001a78c8`, `FUN_001a7808`, `FUN_001a7a48`, `FUN_0016e208`, `FUN_0016eda8`, `FUN_0016ed08`; data 0x382df8 .. 0x382e14, 0x382e60, 0x382e68, 0x37a430 .. 0x37a468.

### 20.13 Counter row, model page return, page-turn sounds (WP-JR3) [V unless marked]

- **Counter row 0x4c.** Read again against `FUN_002aadf0` (label object +0x350 with message 0x489b, value object +0x4f8 in `FUN_00278418(2)`'s group, text
  `FUN_001918a0(.., 0x382de0, FUN_002a1f78(rec))`, the value object moved by the label group's x / y `FUN_00237618` / `FUN_00237638` and right aligned `FUN_00237658` ->
  +0x694, +0x69c = 1) and `FUN_002a1f78` (flag 0x800: `FUN_00187e68(save + 0x1660, objentry +0x4c)` = `FUN_00187dc8`, slot 1000 + id below 0x2f or 3000 + id below 12, the
  writer `FUN_00187e08` adds 1 up to 9999 under story flag 0x8d0). The port already had every detail (`menu_journal::enemy_rows`, `name_list::defeats`,
  `Progress::count_defeat` with the 9999 cap and the flag), so nothing was missing: no change. The row exists only in `FUN_002aadf0`, so the model pages 0x10 / 0x13 / 0x16 (`FUN_002aa0b0`) have none.
- **Return from the Heartless model page.** `FUN_002aa0b0` case -2 (0x2aa2ec .. 0x2aa300: `li 0x10; beq page, 0x10 -> 0xf; xori; movz 0x15`) returns page 0xf from 0x10 and 0x15 from
  0x13 and 0x16. Page 0x15 builds as the character page (`FUN_002a98f0`; the family, tab and face file come from the kind byte `DAT_0036d484 + 4`, which stays 0x11) but its cancel
  (`FUN_002a9b80` case -2: 0xe from 0xf, 0x11 from 0x12, else 0x14) goes to the Nobodies' list `FUN_002ac370(0x12)`, the cursor being the shown character's index (`FUN_00277dc8(0xe, ..)`),
  and its -6 opens 0x16 (case -6: 0x10 from 0xf, 0x13 from 0x12, else 0x16). Ported: `CharPage::nobody` (set by the model page's cancel when the original's number is 0x15,
  `model_view::char_page`), `CharPage::list_page`. Log: `journal: page 0xf cancel -> page 0x14 with face 2 ...`. Test `menu_journal::tests::a_heartless_model_page_returns_as_the_nobody_page`.
  Harness (state-e of 20.11, keys `7:Enter,8:Down,8.2:Down,8.4:Down,8.6:Down,9.2:J,10:Escape,11:Left,12:J,13:Down,14:J,15:Down,15.3:Down,16:J,18:Y,20:Escape,22:Escape`, shot at 24.5): the
  "The Nobodies" list with Dusk and Dragoon, the hand on the second row (screenshot looked at). [inferred: the cursor index is taken as the Nobodies list's row, clamped as every list's kept position.]
- **Close and reopen after a model page.** The journal loads once and keeps its nodes; `model_view::show` sees the closed menu, drops the camera and the model and hides the picture, and
  builds both again when a character page is next shown. Harness (same keys to `18:Y`, then `20:Enter` closes, `24:J` the camp's Journal row, `26:J,27:Down,28:J,29:Down,29.3:Down,30:J,32:Y`, shot at 35):
  the log shows `page 19 closed`, `open (FUN_002756f8)`, then `page 0x10 model (FUN_002abf60(1)): character 114 ...`; the screenshot shows Goofy's large model centred on the maroon page
  (looked at). Nothing was broken: no change.
- **Page-turn handlers' sound calls: none.** `FUN_002785a0` calls `FUN_0029b808(DAT_0036d100 + 0xe78, 0x7c)` = `FUN_0029b7b0`: when the object's group (+0x178) differs it stores 0x7c and restarts the
  sequence (`FUN_00138e78` -> `FUN_002368e8`), an animation group of the page-turn object, not a sound. `FUN_00277338` sets the groups 0x36, 0x2d, 0x30, 0x33 on the hint objects, `FUN_002779f8` the page-turn
  indicator's group (0x381fa8 / 0x381fb0 tables), `FUN_00277b70` tests group 0x82, `FUN_00277ad8` sets group 1 of `FUN_002779f8`. The only sound in these handlers is `FUN_00295bf0` -> `FUN_0023e738`
  (the cursor, confirm, cancel and refused ids the port already plays) and the explicit `FUN_0023e738(3, 1)` cancel in `FUN_002aa0b0`. Nothing to port.
- **Decompile index (JR3).** `FUN_002aadf0`, `FUN_002a1f78`, `FUN_00187e68`, `FUN_00187dc8`, `FUN_00187e08`, `FUN_002aa0b0` (+ instructions 0x2aa2b0 .. 0x2aa308), `FUN_002a9590`, `FUN_002a9b80`, `FUN_002a8958`,
  `FUN_002aa358`, `FUN_002aa600`, `FUN_002ac450`, `FUN_002ac370`, `FUN_002ab7d8`, `FUN_002785a0`, `FUN_0029b808`, `FUN_0029b7b0`, `FUN_00277b70`, `FUN_00277338`, `FUN_002779f8`, `FUN_00277ad8`,
  `FUN_002b6120`, `FUN_00295bf0`, `FUN_0023e738`, `FUN_0023e828`, `FUN_00138e78`.

### 20.14 Viewer plate, model alpha and the page turn (WP-JN3) [V unless marked]

- **Viewer plate.** `FUN_002aba90` (called when page 0x10 / 0x13 / 0x16 is built, `FUN_002a9b80`) makes object 0x7567 from `jmemo.2ld` sequence 5, groups 4 (12 frames, alpha 0 to 0x80) then 5 (hold), as a
  child of the page layer (`FUN_0029ba00`); `FUN_002abb20` sends it away with group 6 (12 frames, alpha to 0). Its group is a full-width light green band (x -40 .. 556, y 67 .. 376, dark green
  fades above and below), the "Jiminy's Journal" lettering (sprites 132 .. 135, alpha 0x40) and four corner brackets (sprite 70). Its animations translate by (-76, -180), cancelled by the layer
  group 0x49's anchor animation (76, 180), so the port hangs it from `cpage.layer` with that group. The teardown `FUN_002a8568` (next page 0x10 / 0x13 / 0x16) calls `FUN_0029d4e0(0 / 1)` on the name plates and `FUN_00277ec0` on the backdrop 0x755a; the port ends only the backdrop (the plate replaces it) and keeps the tab and help line, drawn in front of the plate (the plate is an early node) [inferred: the reviewer asked to keep them; the decomp does not prove them hidden] (`menu_journal::build_model_page`). The help line is
  sent away by -6 (`FUN_002785a0`, group 0x7c) and lies under the plate in the port's draw order [inferred: creation order, `FUN_001394e8` appends to the object list].
- **Model alpha (the fade).** `FUN_0023b4e0` gives each model the colour alpha of its alpha object (`FUN_00237518(object + 0x18) >> 24`); `FUN_002abc40` makes that object with the model
  (`+0x432c` small, `+0x44b8` large; sequence 5 groups 7 then 8, leave 9). Group 7: alpha 0 to 0x80 in 24 frames (linear); group 8: 0x80; group 9: 0x80 to 0 in 12 frames. Ported
  (`model_view::group_alpha`, `fade`): the picture's alpha follows the node, restarted by every new model. When the object dies the model is destroyed (`FUN_0023a920`).
- **Lighting.** `FUN_0023a520` sets the slot's light vectors `+0x660 / +0x670` to 0 and the light colour `+0x680` to `DAT_0037f478` on every slot, and the colour `+0x798` 0x80 per channel (`FUN_0023b448`
  is never called by the journal): the same slot setup as the camp's preview with the neutral colour 0x80 instead of 0x60. The port already draws the model at the neutral colour; the light
  vector numbers were not read from the executable [GAP: `DAT_0037f478`, the vectors' use in `FUN_0023aca8` through `FUN_001dbc70` / `FUN_001da2a0` not traced].
- **Page-turn gap.** A page-changing key makes the handler set the waiting state (`FUN_00295ac0(2)`, e.g. `FUN_002a27d8`, `FUN_002a9b80`): the header object 0x7559 plays group `table[1]` of 0x381fa8 =
  129 (12 frames, alpha 0x80 to 0; the table is 131 enter (24 frames), 129 leave, 130 hold), the help line group 0x7c, and each frame of waiting tears the old page down (`FUN_002a21b8`, `FUN_002a8568`);
  the next page is built only when the header (`FUN_00277b40`), the layer objects (`FUN_00278068`) and the viewer objects (`FUN_002abdc8`) are gone, and the pad is ignored meanwhile
  (`FUN_00295d90(0)`). The character page and its model page keep the header: they send the model alpha (group 9) and the plate (group 6) away instead. The header and alpha objects are one
  pixel sprites: the gap is the 12 frames of waiting. Ported (`menu_journal::begin_turn`, `Turn`): header leave, old page content hidden (tabs, title, backdrop, hints stay [inferred]), input
  ignored, next page built when the leave groups have finished, header re-entered. Not ported [GAP]: the rows' and list's own leave groups (`FUN_002989a8`, `FUN_00298808`), the help line's group 0x7c,
  the header enter wait (24 frames) before input.
- **Decompile index (JN3).** `FUN_002aba90`, `FUN_002aba68`, `FUN_002abb20`, `FUN_002abc40`, `FUN_002abd28`, `FUN_002abd68`, `FUN_002abdc8`, `FUN_002abef0`, `FUN_002abf60`, `FUN_0023b4e0`,
  `FUN_0023a520`, `FUN_0023a920`, `FUN_0023aca8`, `FUN_0023c1e8`, `FUN_00236cf8`, `FUN_00237518`, `FUN_0029b600`, `FUN_0029b7b0`, `FUN_0029b808`, `FUN_0029ba00`, `FUN_00138ed0`, `FUN_00139430`,
  `FUN_00139778`, `FUN_002779f8`, `FUN_00277b40`, `FUN_00277b70`, `FUN_00277ec0`, `FUN_00278068`, `FUN_00278608`, `FUN_00278658`, `FUN_002785a0`, `FUN_00276038`, `FUN_00276bb0`, `FUN_002a27d8`,
  `FUN_002a9b80`, `FUN_002aa0b0`, `FUN_002a8568`, `FUN_002a84d8`, `FUN_002a8718`, `FUN_0029d4e0`, `FUN_00295d90`. Tests: `model_view::tests::the_disc_model_alpha_and_turn_groups`.
