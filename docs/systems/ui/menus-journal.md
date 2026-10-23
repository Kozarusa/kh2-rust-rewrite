# Jiminy's Journal: entry, data, pages 0 to 3, scroll bar, NEW tags, lists (20.1 to 20.9)

[Contract](contract.md) | [Index](../../README.md)

## 20. Jiminy's Journal: menu kind 3 (WP-MN9)

### 20.1 Entry, scene and exit [V unless marked]

- **From the camp.** Page 0's handler (`FUN_00251848`, jump table 0x380794, entries 5 and 6 both at 0x251c38): `FUN_00253ec0`
  gives the row's entry; the mode word 0x36cd34 (`FUN_0024f4d0`) becomes 1 for entry 5 (Jiminy's Journal) and 2 for entry 6
  (Roxas's); `FUN_00296458(2, 0)` queues the menu scenes 2 (kind 3) and then 0 (the camp); `FUN_00296470(0x250ec8)` starts the
  wipe with the camp's teardown as its callback; `FUN_00295ba0` closes the camp. The scene runner `FUN_00296278` runs the
  queued scene's init, its task until it ends, its term, then shifts the queue (`DAT_0036d206` <- `0x208` <- `0x20a`): **the
  journal returns to the camp**. (The other opener, `FUN_0024f3c0` from 0x2c9510 (the world map's triangle), stores the world's journal index in 0x36d038 and calls `FUN_0024f0b8(3, 0)`: kind 3's record (0x380590) names the mode word 0x36cd34, so the mode is 0 and `FUN_002756f8` starts on page 0 with the world byte -1. Ported by WP-JN4: `menu_journal::OpenJournal` (a message) makes `tick` open the scene with `Journal::external` set and `Menu::phase` open on `PAGE`; `close` then ends the scene instead of returning to the camp, which was never up.)
- **Init `FUN_002756f8`.** Loads menu file 3 (`jmemo.2ld`, `FUN_0024f738(3)`, `FUN_00255258(3)`), creates two list widgets
  (`FUN_002969b0(0x38)`, `FUN_002967b8(.., 0x12, 1 / 0)`), the data holder `DAT_0036d0dc` (bytes: +0 journal world, +1 page
  family, +2 Roxas flag), the file list `FUN_002a1a10` with `jiminy.bar` (`FUN_002a1bd0`: `FUN_002a19c8(0, 0)`, names at
  0x36d3e0: `jiminy.bar`, `jm_world.2ld`, `jm_top_` + world + `.2ld`, `jm_photo/`, `jmface_`; suffixes `.2ld` 0x36d3d8 and
  `.bin` 0x36d3dc, built by `FUN_002a18e8`) and the fifteen page modules' inits (`FUN_002afa48` .. `FUN_002b5c60`). Then the
  **start page**: `m` = the mode word (negative -> 0); the Roxas flag `+2` = (m == 2); when m != 2, `+0` = the current world's
  journal index (`FUN_00277c88(DAT_0036d038)`; `DAT_0036d038` = -1 from the camp, so the world byte 0x33e830) and a negative
  index makes m = 0. Page: m 1 -> **2**; m 0 -> **0** (`+0` = -1); m 2 -> **1** (`+0` = 0x13); m 3 -> **10**; others 0
  (`menu_journal::start`).
- **World index `FUN_00277c88`.** A jump table at 0x381f38, one stub per game world (19), each `jr ra` with the index in the
  delay slot: -1 for worlds 0, 3 and 15, 0 for Twilight Town (2), and so on (read at run time: `read_world_index`).
- **Task 0x276338** (no `.c`; read from the instructions): `FUN_002552c0(3, task)` (waits for the files), `FUN_00277be8`,
  `FUN_002a1858(0, 0)` (the `jiminy.bar` request), `FUN_00276550` / `FUN_002765a0` (the message file 0x65 = `msg/us/jm.bar`, WP-MN10: 20.7), `FUN_002b4890`,
  yields, `FUN_002a1bd0` / `FUN_002a1c30(0)` unless `DAT_0036d040 & 4`, `FUN_002b48c8`, `FUN_00276488(page)` (the page's own
  `jiminy.bar` sub-files), waits while `DAT_0036d106`, `FUN_002b7100`, `FUN_002b9b28`, **the build `FUN_00276740`**, the list
  widget `FUN_002a04f8`, the open sound (`FUN_0023e738(0xb, 1)`), then each frame `FUN_00233cd8` and **`FUN_00276918`** until
  it returns 1, with `FUN_00276bb0` (the draw) between; the end `FUN_002765e8`.
- **Build `FUN_00276740`.** The layout object (`FUN_0023b920(0xa0)`, `FUN_0029b530(.., 0x7557, 0, 0, t[0], t[2])` with
  `t` = 0x381f98 (layouts 0, 2, 1: enter, leave, hold) or 0x381fa0 (3, 5, 4) when the Roxas flag is set, `FUN_00278788`), the
  plates `FUN_0029cf78(0, 5, layout, 0x381fc0, 0x381fc8)`, the header object 0x7559 (`FUN_002779f8(0, 0)`: groups from
  0x381fa8, the first then the third), the plate object 0x7558 (`FUN_002772d0`: groups 0x2c then 0x2d), then the start page's
  builder: 0 `FUN_002a2248`, 1 `FUN_002a4188`, 2 `FUN_002a3540`, 10 `FUN_002b4b18` (after `FUN_0029d820(0, 0x3251)`).
- **Page handlers `FUN_00276918`** (page = `DAT_0036d1d4`): 0 `FUN_002a27d8`, 1 `FUN_002a44a8`, 2 `FUN_002a3da0`, 3
  `FUN_002a5218`, 4 `FUN_002a6010`, 5 `FUN_002a6578`, 6/0xb/0x20 `FUN_002ae790`, 7/8/0x19/0x1a `FUN_002afe98`, 9/0x1b
  `FUN_002b6120`, 10/0x1c `FUN_002b4dd8`, 0xc `FUN_002a7978`, 0xd `FUN_002a8e90`, 0xe/0x11/0x14 `FUN_002a9590`, 0xf/0x12/0x15
  `FUN_002a9b80`, 0x10/0x13/0x16 `FUN_002aa0b0`, 0x17 `FUN_002accb8`, 0x18 `FUN_002ad3d0`, 0x1d `FUN_002b25a8`, 0x1e
  `FUN_002b2d78`, 0x1f `FUN_002b3f10`. Once closing (`FUN_00295bb0`), it waits for the objects, tears down (`FUN_002761e8`) and
  returns 1 (`FUN_001038a0`): the scene ends.

### 20.2 The data: `jiminy.bar` [V layout of entry 0; I the others' use]

`menu/us/jiminy.bar` is a BAR of nine type-43 entries, each with a 4-byte magic, 0x10 at +4 and a record count at +8: "worl"
(JMWO, 17), "stor" (JMST), "albu" (JMAL), "char" (JMCH), "anse" (JMAN), "diag" (JMDI), "limi" (JMLI), "mini" (JMMG), "ques"
(JMQU). Entry 0 (the worlds) has 0x10-byte records from +0x10: +0 the journal world id (i8), +1 the two-letter world code, +4
and +6 two message ids, **+8 the story flag that opens the world** (`FUN_002a1db8` -> `FUN_001802e0`), +0xe a second flag
(`FUN_002a1de8`, read by `FUN_00276488` for id 1). Entry 3 is read by `FUN_002a1c30` (0x24-byte records, +4 a bit number) and
entry 5 (0xc-byte records with a sub-list at +8): the bits of the 12-word table `DAT_0036d450` [the meaning: GAP]. The
other entries belong to the pages that are not read. `jm_world.2ld` is a BAR of 17 IMGD pictures ("wmic"), one per journal
world id, 0x10 the locked world.

### 20.3 Page 0: the world grid [V]

- **Build `FUN_002a2248`.** 0x12 list objects (`FUN_00296b48`, every group of the record set = `DAT_0036d050` = 1) in
  sequence 5 (`DAT_0036d04c`), each with two children (types 0x755f and 0x7560, group 1). Walking `jiminy.bar` entry 0, a
  record with an id below 0x10 gets a cell when its flag is set or when flag 0x4837 is (`FUN_002783f8`); the cell plays 0x97
  and, for an open world, `FUN_002b9658(id)` / `FUN_002b9798(id)` put the first child on group 0x98 or 0x99 (the words at
  0x382aec / 0x382af0) and `FUN_002baf08(id)` the second on 0x9a. The world picture (`FUN_002a2bc0`, once `jm_world.2ld` has
  arrived): an object of sequence 2 drawing picture `id` (0x10 when the flag is clear) of the `jm_world.2ld` list
  (`FUN_0023c478`, `FUN_002332e0`), group 1 then 2 (2 alone when built before the pictures arrived), child of the cell.
- **Place `FUN_002a25f0`.** Rows of six; each full row centred (`x = -(pitch_x * (cols - 1)) / 2 + col * pitch_x`), the
  last partial row centred on its own count, the rows centred (`y = row * pitch_y - (pitch_y * (rows - 1)) / 2`), C integer
  division. `pitch_x` is the cell group's +0x20 (`FUN_00237698`), `pitch_y` its +0x1c (`FUN_00237678`): 80 and 98 for
  group 0x97. The offsets are the objects' own positions (`FUN_00138ed0`); the cell group itself is drawn around the screen
  centre (port log: the 0x97 box sits at x 215..295 with offset 0).
- **Cursor.** `FUN_002a2f38(+0 or 0xf when negative)`: the place of that world among the shown records with an id below 0x14;
  the widget gets 6 columns, 3 rows (`FUN_00298e40(list, 6, 3)`), the cursor cell plays 0x96 and the others 0x97
  (`FUN_002a31e8`).
- **Handler `FUN_002a27d8`.** List result >= 0: `FUN_002a2e68` gives the world id, `+0` = it, page 2 (0xc when the id is
  above 0xe); -8 / -9 page switches through `FUN_00277928`; **-2 and -4 close the scene** (`FUN_00295ba0`).
- **Page 2 `FUN_002a3540` / `FUN_002a3da0`** (read for the exits only): an 8-row list of the world's categories (loads
  `jm_top_<world>.2ld` through `FUN_002a45b0`); a row leads to page 6, 3, 4, 7, 8, 9, 10 or 0xb (the message at 0x382b88 +
  2 * row goes to the help line); **-2 goes back to page 0**, -4 closes.

### 20.4 What the port does (`crates/kh2-app/src/menu_journal.rs`)

A confirm on camp entry 5 or 6 sets the mode (1 / 2), takes `menu.page` (0x300, not a camp page) and opens the scene on the
next frame: it loads `jmemo.2ld`, `jm_world.2ld` and `jiminy.bar` once, hides every node the camp had (restored on the way
back), plays layout `t[0]` then `t[2]` (the frame, carried out to the window's edges as the camp's frame is [inferred]), the
header object (0x381fa8 groups) and the plate 0x7558 (0x2c, 0x2d), and the cells of 20.3 with the app's story flags
(`Progress::story`). (WP-MN10: start pages 1 and 2 are now built, 20.7; page 0 follows a cancel on page 2, with the cursor on the
world.) Left/Right/Up/Down move the cursor [inferred: one cell, one row of six, no
wrap], a confirm opens page 2 (ids above 0xe: refused with a log line), K / Esc / Enter close the scene and the camp's
page 0 comes back. Tests: `menu_journal::tests::{the_start_page_follows_the_mode_and_the_world,
camp_rows_five_and_six_write_modes_one_and_two, cells_need_the_story_flag_or_the_all_worlds_flag,
the_grid_centres_rows_of_six, page_zero_state_machine, the_disc_journal_files_and_tables}`. Run (rows of a `KH2_STATE` with the
flags of nine worlds; the camp has six rows there, Journal is the fifth):
`KH2_STATE=<{"story_flags": [2097, 4098, 5122, 6195, 7204, 8193, 9217, 10243]}> KH2_NO_ENEMIES=1 KH2_KEYS=7:Enter,8:Down,8.2:Down,8.4:Down,8.6:Down,9.2:J,10:Right kh2-game tt02`
(`journal: open (FUN_002756f8): mode 1, journal world 0, start page 2 ...; 9 cells ...`); `KH2_JOURNAL_QUADS=1` logs each
node's box once.

### 20.5 Gaps

- **Pages 4 to 0x20** (WP-MN10 ported 1 and 2: 20.7; WP-MN11 page 3, the help line, the cursor and L1/R1: 20.8; WP-MN12 pages 4, 5, 6, 7, 8 and 0xb: 20.9) (the world's categories, characters, enemies, the album, Ansem's reports, limits, minigames, quests,
  the Roxas pages, the photo and face files): not read beyond the exits of page 2.
- **The children's groups 0x98 / 0x99 / 0x9a** (`FUN_002b9658`, `FUN_002b9798`, `FUN_002baf08`, the words at 0x382aec /
  0x382af0): WP-MN12 ported 0x98 / 0x99 (20.9); 0x9a (every category complete) is still open.
- **The plates `FUN_0029cf78(0, 5, .., 0x381fc0, 0x381fc8)`** (groups 0x71/0x73/0x72 and 0x23/0x25/0x24): not drawn; what
  they show on this scene was not read.
- **Text:** the world names (record +4 / +6) and any help line of page 0 (`FUN_0029d820` is called for page 10 only) were
  not traced to a drawer: the port draws no text on the page.
- **Sounds:** none of the scene's own were read (`FUN_00276550` / `FUN_002765a0` are the message file, 20.7); the menu sounds are muted anyway.
- **The wipe** (`FUN_00296470`: `pause.2ld` group 5 as object 0x7554) is not played; the camp's cursor on the way back
  stays where it was [inferred].
- **Footage:** no reference clip shows the journal (`agents/RECIPES.md` R6); the screen was compared with nothing but the code.

### 20.6 Decompile index (WP-MN9)

`FUN_002756f8`, `FUN_00275bd8`, task 0x276338 (instructions), `FUN_00276038`, `FUN_002761e8`, `FUN_00276488`,
`FUN_00276550`, `FUN_002765a0`, `FUN_002765e8`, `FUN_00276740`, `FUN_00276918`, `FUN_00276bb0`, `FUN_00277c88` (+ stubs
0x277cbc..0x277d3c), `FUN_002779f8`, `FUN_002772d0`, `FUN_00278788`, `FUN_0029cf78`, `FUN_0029b530`, `FUN_0029b600`,
`FUN_00296b48`, `FUN_002a1a10`, `FUN_002a1bd0`, `FUN_002a1858`, `FUN_002a18e8`, `FUN_002a19c8`, `FUN_002a1c10`,
`FUN_002a1c30`, `FUN_002a1db8`, `FUN_002a1de8`, `FUN_002a2108`, `FUN_002a21b8`, `FUN_002a2248`, `FUN_002a25f0`,
`FUN_002a27d8`, `FUN_002a2a90`, `FUN_002a2af0`, `FUN_002a2b48`, `FUN_002a2bc0`, `FUN_002a2d98`, `FUN_002a2e68`,
`FUN_002a2f38`, `FUN_002a3008`, `FUN_002a31e8`, `FUN_002a3540`, `FUN_002a3da0`, `FUN_002783f8`, `FUN_00237678`,
`FUN_00237698`, `FUN_0024f0b8`, `FUN_0024f3c0`, `FUN_0024f4d0`, `FUN_0024f4e0`, `FUN_0024f4f0`, `FUN_00250ec8`,
`FUN_00296278`, `FUN_00296458`, `FUN_00296470`, `FUN_001038a0`, `FUN_00103d58`, camp handler 0x251c38 (instructions).

### 20.7 Pages 2 and 1 (WP-MN10) [V unless marked]

- **Message file.** Task 0x276338's `FUN_002765a0` calls `FUN_00190e70(0x65, 0)`: `FUN_00190dd0(0x65)` builds
  `msg/%s/%s.bar` with "jm" (0x37ad38), and `FUN_00190b70(0, file)` puts it in message slot 0; `FUN_00190c40` looks an id
  up slot by slot (0x1c8a7f4, three slots), so the journal's ids come from `msg/us/jm.bar` before the system file.
  (`FUN_00276550` asks the same file with 0x13ec through `FUN_0016fac8`; what that second argument is was not read.)
- **Objects** (`FUN_002a3540` page 2, `FUN_002a4188` page 1). Six layer objects of sequence 5, groups 0x89, 0x93, 0x91,
  3, 0x92, 3 (page 1: 0x8d, 0x90, 0x94, 3, 0x95, 3); 0, 1, 2 and 4 hang from the header object `DAT_0036d0e8` (the 0x7559
  of `FUN_002779f8`), 3 from 2 and 5 from 4. `FUN_002a3d08` gives 2 and 4 the two story texts of the world
  (`FUN_002a1e98`: `jiminy.bar` entry 1, 0xc-byte records, +0 world, +4 / +6 the ids, used when the flag +0xa is set,
  the last such record wins, 0x8adc otherwise) and 3 and 5 the labels 0x3253 / 0x3254, centred (+0x1a4 = 2).
- **Text placement** (`FUN_00139048`): from the object's position (`FUN_00138f38`), plus the group's x/y (+0x10/+0x14)
  when flag 0x10 is set, in font +0x1a0 (1 for these objects), size the group's +0x18 (15 when 0), colour +0x198
  (0xe5e5e5, `FUN_00138fc8`) when flag 8 is set. `FUN_00138fc8` sets flags 4, 8, 0x10; `FUN_00139888` clears 0x10,
  `FUN_00139860` clears 8. The port: rows and the holders 2/4 use the group offset, 3/5 do not; holders 2/4 lose flag 8
  (colour from the group: the port keeps 0xe5e5e5 [inferred]).
- **Rows** (page 2): 8 list objects (`FUN_00296b48`, both group sets four 3s: the row's group does not change with the
  cursor) as children of layer 0, `k *` the 0x89 group's +0x1c apart (`FUN_00138ed0`), each with two children on group 3
  (the marks: `FUN_002a39b8` puts the first on 0x8a and the second on 0x8b / 0x8c (0x382b80/84) from the categories'
  progress tests: not ported, GAP). The rows are categories 0..7 (`FUN_002a4960`) without 3 for journal world 8
  (`FUN_002b1650`), 5 unless `FUN_002b6f00` (entry 8, 0x10-byte records, u16 world) and 6 unless `FUN_002b5bb0` (entry 7,
  8-byte records) finds a record that the all-worlds flag 0x4837 or a progress test opens. **The port opens 5 and 6
  only with the all-worlds flag** [conservative: the progress tests `FUN_002b6d40` and the minigame bytes 0x341620 have no
  app data]. A row's text is the u16 at 0x382b88 + 2 * category; its help line the one at 0x382b98 (`FUN_002a4a68` ->
  `FUN_00278430` -> `FUN_00278468`, object 0x755d: not drawn, GAP).
- **Title** (page 2): object `DAT_0036d0f0 + 0x350` (`FUN_00277258`: groups 0x35 then 0x36, centred), text
  `FUN_002a3070(world, 0)`: the world record's +4, or +0xa when its flag +0xe is set (`FUN_00277640`). A root object: the
  port draws its text from the group's anchor (`label_anchor`) [inferred: the reading `menu_gfx` uses for the camp rows].
- **Pictures** (`FUN_002a45b0`, `FUN_002a4750`): `jm_top_<name>.2ld` (`FUN_002a19c8(2, name)`), name = the world
  record's code on page 2 (world 1 with its second flag: the string at 0x36d460), the string at 0x36d45c on page 1; picture
  0 in sequence 0x382b6c at layer 0's group x/y, picture 1 in sequence 0x382b68 on layer 1; groups 0x382b78 / 0x382b7c
  and 0x382b70 / 0x382b74 (enter, hold) when the file arrives after the build (the per-frame `FUN_002a4750(1)`), the hold
  alone when it was already there [inferred: the port plays enter then hold].
- **Handlers.** Page 2 (`FUN_002a3da0`): Up/Down move the row cursor (the list widget, wrap on a new press), a confirm
  opens the category page 6, 3, 4, 7, 8, 9, 10 or 0xb (**not ported: refused with a log line**), -2 goes to page 0
  (`FUN_002a2248`, the cursor on the world), -4 closes the scene. Page 1 (`FUN_002a44a8`): -2 and -4 close the scene.
  Page 0's confirm on a world up to id 0xe now builds page 2.
- **First cursor of page 0** (review fix): the index is taken within the shown cells (`first_cursor`), since records
  0x10..0x13 that `FUN_002a2f38` counts have no cell.
- **Port.** `menu_journal::{build12, page2, page1, categories, story_texts, read_page_tables}`. Tests:
  `menu_journal::tests::{page_two_rows_and_state_machine, story_texts_take_the_last_open_record, the_disc_page_two_data,
  the_first_cursor_indexes_the_shown_cells}`. Run as in 20.4; the log: `journal: page 2 (FUN_002a3540 / FUN_002a4188):
  world 0 'tt', jm_top_tt.2ld 2 pictures, story texts [8adc, 8adc], rows [0, 1, 2, 3, 4, 7], text drawn`.
- **Gaps.** The row cursor's highlight (no group change; the widget's cursor object was not found), the help line, the
  L/R page hints (`FUN_00277860` / `FUN_00277740`: ids from 0x381fb8 with a button prefix, `FUN_002774e8` objects 0x2f/0x30
  and 0x32/0x33), the marks, the page switches -8/-9 (`FUN_00277928`), the category pages, the children groups of page 0
  (`FUN_002b9658` / `FUN_002b9798` / `FUN_002baf08` aggregate the same per-category progress tests). Roxas's page 1 is
  ported but was not reached in a run (the test state's camp has no Roxas row). No footage of the journal exists.
- **Decompile index (MN10).** `FUN_002a3540`, `FUN_002a3da0`, `FUN_002a4188`, `FUN_002a44a8`, `FUN_002a4750`,
  `FUN_002a3d08`, `FUN_002a39b8`, `FUN_002a4a68`, `FUN_002a45b0`, `FUN_002a4960`, `FUN_002a46d8`, `FUN_002a34a8`,
  `FUN_002a3070`, `FUN_002a1e98`, `FUN_002a1e78`, `FUN_002a3008`, `FUN_002b1650`, `FUN_002b6f00`, `FUN_002b6d40`,
  `FUN_002b5bb0`, `FUN_001d36e8`, `FUN_00277640`, `FUN_00277740`, `FUN_00277860`, `FUN_00277258`, `FUN_002774e8`,
  `FUN_002772d0`, `FUN_00278430`, `FUN_00278468`, `FUN_002779f8`, `FUN_00277b20`, `FUN_00277eb0`, `FUN_00277d40`,
  `FUN_00277d60`, `FUN_00277dc8`, `FUN_00277de8`, `FUN_002a19c8`, `FUN_002a4b18`, `FUN_002a4b28`, `FUN_0029b600`,
  `FUN_0029b6c0`, `FUN_0029ba00`, `FUN_00296b48`, `FUN_00296aa8`, `FUN_00299f30`, `FUN_00299e38`, `FUN_00138fc8`,
  `FUN_00138948`, `FUN_00139430`, `FUN_001389b0`, `FUN_00138a60`, `FUN_00138ed0`, `FUN_00138f38`, `FUN_00139048`,
  `FUN_00139850`, `FUN_00139860`, `FUN_00139878`, `FUN_00139888`, `FUN_001398a0`, `FUN_00237518`, `FUN_00237618`,
  `FUN_00237638`, `FUN_00237658`, `FUN_00237678`, `FUN_001a7ed8`, `FUN_00190c40`, `FUN_00190dd0`, `FUN_00190e40`,
  `FUN_00190e70`, `FUN_00190b70`, `FUN_00276550`, `FUN_002765a0`, `FUN_002b9658`, `FUN_002b9798`, `FUN_002baf08`,
  `FUN_002a04f8`; data 0x382b68..0x382ba6, 0x381fb8, 0x36d45c, 0x36d460, 0x37ad38.

### 20.8 Help line, cursor, L1/R1, page 3 the story (WP-MN11) [V unless marked]

- **Help line** (`FUN_00278468`, instructions 0x278468..0x278554): object `DAT_0036d100 + 0xe78`, type 0x755d,
  sequence 5, **group 0x7b**, a child of the plate 0x7558 (`DAT_0036d0f0 + 0x4f8`) when that one is alive; the text
  goes to +0x18c; flag 8 cleared (`FUN_00139860`); a second call on a live object plays 0x7b again; `FUN_002785a0`
  plays 0x7c (the leave) at every page switch. Callers: page 2's build through `FUN_002a4a68` (page 1: 0x493f, else
  the u16 at 0x382b98 + 2 * the cursor row's category), page 3's build through `FUN_002a54a8` (0x48d7), the other
  category pages (`FUN_002afa00`: 0x382f04 / 0x382f08). **Nothing calls `FUN_002a4a68` on a cursor move** (`--jal
  0x2a4a68`: the build only; a tail `j` was not scanned): the line keeps the category of the row the build put the
  cursor on. Page 0 sets none (the bar stays empty). Port: `chrome`.
- **List cursor** (`FUN_00276bb0`, the per-frame draw): while the list `DAT_0036d0e0` has a live row object
  (`FUN_00298a90`), +0xe is 0, the scene is idle (`FUN_00295ad0() == 1`) and `DAT_0036d104` is 0, it creates cursor
  slot 0 (`FUN_00277158` -> `FUN_0029a9c0(0, seq 5, .., 0x21, 0x21)`) and hands it the cursor row
  (`FUN_0029ac68`: target +0x180, flag 0x800 `FUN_00139918`), and object 0x7571 (`FUN_002771c0` ->
  `FUN_0029af00(DAT_0036d0fc, 0x7571, .., 0x381fc8)`: the follower groups, enter [0] then [2]) on the same row
  unless `DAT_0036d105` (no writer found with `--addr 0x36d105`: taken as 0 [inferred]). `DAT_0036d104` is set by
  `FUN_00277e70(1)` in the builds of page 1 (`FUN_002a4188`) and page 3 (`FUN_002a4c38`): those pages have none.
  Port: the cursor (group 0x21) and the follower drawn at the cursor row of page 2 or the cursor cell of page 0
  [inferred: an object with a target is drawn at the target's position]; `place_cursor`. This is the row
  highlight MN10 looked for (the rows' own groups never change).
- **L1 / R1** (`FUN_00277860`, `FUN_00277740`, `FUN_002774e8`, `FUN_00277928`): the names at 0x381fb8 belong to
  pages 0, 2 and 0xc (`FUN_00277928` maps a name's place in the table to 0, 2, 0xc). On page 0 the left hint is name 2
  and the right name 1; on pages 1 and 2 left 0, right 2; on 0xc left 1, right 0. When the journal world byte read
  with `lbu` is above 0xe (so -1 too) a hint that would name page 2 names 0 (page 0xc: 2) instead. The texts:
  left = icon `FUN_00255418(0x400)` (pairs at 0x380ab0, 0xb when absent) + the name, right = the name + icon
  `FUN_00255418(0x800)` (control code 9, `FUN_002554c0`). Objects `DAT_0036d0f0` (groups 0x2f then 0x30) and
  `DAT_0036d0f0 + 0x1a8` (0x32 then 0x33), centred (+0x1a4 = 2), made by `FUN_00277258` -> `FUN_002774e8`; only the
  builds of pages 0 and 2 write the texts; the category teardown `FUN_00277ec0` frees the `DAT_0036d100` objects
  only, so the hints and the title stay on page 3 [inferred from the teardown]. The list results -9 and -8 come from
  bits 0x8000 / 0x4000 of the pad word when the list flags 0x2000 / 0x1000 are set (`FUN_00297148`; the builds OR
  0x3000 in); -8 takes the right hint's page (`FUN_002a27d8`, `FUN_002a3da0`). Port: -9 = L1 = Q / PageUp, -8 = R1 =
  E / PageDown [inferred: the side each icon is drawn on]; pages 0 and 2 switch to each other, page 0xc is refused.
- **Page 3, the story** (category 1; build `FUN_002a4c38`, data `FUN_002a5060`, draw callback 0x2a54c8 (no `.c`,
  read from the instructions), marks `FUN_002a5708`, handler `FUN_002a5218`). Entries: every record of `jiminy.bar`
  entry 1 of the journal world whose flag +0xa is set (`FUN_002a1e78`), in file order, text +8; `FUN_00191760`
  counts each text's lines. Objects: the tab of plate 0 (`FUN_0029d1d0(0, -1, 0)`: groups 0x381fc0 [0] then [2],
  the category's row text put there by page 2's confirm `FUN_0029d820(0, ..)`, font 1, its follower 0x381fc8), the
  backdrop 0x755a (`FUN_00278658`: group 0x381f88[family], family 1 from `FUN_00277d60(1)` of page 2), layers 0x5e
  and 0x62 under the header, one list row (groups 2) under 0x5e carrying the draw callback, ten mark objects (group
  2) one line height apart. The callback clips to ten line heights (`FUN_0014b6c0`), starts with the entry that
  holds the first shown line, draws each text in **font 3** at the 0x5e group's size (15 when 0) with an extra gap
  of `pitch - size - font gap` (`FUN_001a8cc0`, never below 0) so a line is the group's +0x1c high, and moves down
  an entry's lines times the line height. The list count is the lines of all entries but the last, plus one, plus
  the last's lines beyond ten (`story_count`); Up/Down step a line; L1/R1 a page of ten (+0x1c = 10,
  `FUN_00299010`; +0x27 set: the list pages itself in `FUN_00298370` and returns no -9/-8) [inferred: clamped];
  -2 goes back to page 2 with the row the confirm kept (`FUN_00277dc8` / `FUN_00277de8`), -4 closes. The colour is
  the row's +0x198 = 0x60000008 [inferred: that word is the callback's colour argument]. Port: `build3`,
  `story_text`; the scroll bar `FUN_00299878(DAT_0036d0f8, ..)` is not drawn as its own object (the 0x62 layer
  draws its arrows); the marks stay on group 2 (their 0x382ba8 / 0x382bac follow the read bits of `DAT_0036d4e8`,
  which the app has no data for).
- **Run.** The MN10 command with a `KH2_STATE` holding Twilight Town's first story flags:
  `KH2_STATE=<{"story_flags": [2097, 4098, 5122, 6195, 7204, 8193, 9217, 10243, 2197, 2200, 2201, 2204]}>
  KH2_NO_ENEMIES=1 KH2_KEYS=7:Enter,8:Down,8.2:Down,8.4:Down,8.6:Down,9.2:J,10.5:Down,11:J,12:E,12.5:Down kh2-game tt02`
  (`journal: page 3 (FUN_002a4c38): world 0, 4 story entries [2afa, 2afb, 2afc, 2afd], 41 lines, list count 32,
  line height 26, gap 6`, then `first line 10`, `first line 11`). Tests: `menu_journal::tests::{hints_name_the_neighbour_pages,
  story_page_entries_lines_and_state_machine, the_disc_category_page_tables}`.
- **Gaps** (WP-MN12 ported Characters, Character Links, Treasures, Maps and the Album: 20.9; what is still open is listed there). Characters (page 6 / 0xb, `FUN_002ae3e8`, handler `FUN_002ae790`): the faces come from entry 5
  (`FUN_002af398`: records of the world opened by `FUN_002a2018`, story flags or an item count) placed at the
  record's x/y in groups from 0x382f30, but their marks and the "met" state are the bits of `DAT_0036d4e8`
  (`FUN_002b7208` / `FUN_002b72c0`, the journal's own save data) and the face file `jmface_` (`FUN_002af7a0`,
  `FUN_002a19f8`) with `FUN_002af6c0` / `FUN_002af800` were not read: not ported. Album (page 4, `FUN_002a5a68`,
  entry 2 and `FUN_002b79d8`), Treasures / Maps (7 / 8, `FUN_002afca0`), quests (9, `FUN_002b5e50`), minigames (10,
  `FUN_002b4b18`), Character Links (0xb, `FUN_002ae3e8` with the second family) and page 0xc (`FUN_002a7220`): not
  read beyond their first calls; whether they draw an empty frame without data was not established, so they stay
  refused. The help line's colour (flag 8 cleared: the group's colour) is kept at 0xe5e5e5 [inferred]. Page 0's
  cursor and follower are drawn on the cell (the same draw code runs there) on top of the 0x96 group. No footage of
  the journal exists (`agents/RECIPES.md` R6).
- **Decompile index (MN11).** `FUN_00278468` (instructions), `FUN_002785a0`, `FUN_002785d8`, `FUN_00277740`,
  `FUN_00277860`, `FUN_002774e8`, `FUN_00277928`, `FUN_00277258`, `FUN_00277338`, `FUN_002776e0`, `FUN_002779f8`,
  `FUN_00277b20`, `FUN_00277b40`, `FUN_00277e70`, `FUN_00277eb0`, `FUN_00277ec0`, `FUN_00278658`, `FUN_00276bb0`,
  `FUN_00277158`, `FUN_002771c0`, `FUN_0029a9c0`, `FUN_0029ac68`, `FUN_0029ad48`, `FUN_0029aaf8`, `FUN_0029af00`,
  `FUN_0029b3a0`, `FUN_0029cf78`, `FUN_0029d1d0`, `FUN_0029d3e8`, `FUN_0029d820`, `FUN_0029d860`, `FUN_00297148`,
  `FUN_002977a8`, `FUN_002978f8`, `FUN_00298370`, `FUN_00298a80`, `FUN_00298a90`, `FUN_00298ce0`, `FUN_00298cf0`,
  `FUN_00298d58`, `FUN_00298fb0`, `FUN_00298fe0`, `FUN_00299010`, `FUN_002967b8`, `FUN_00296c50`, `FUN_00296f98`,
  `FUN_00296fb8`, `FUN_00255418`, `FUN_002554c0`, `FUN_002554e8`, `FUN_002554f0`, `FUN_00295bf0`, 0x2a4ac0
  (instructions), `FUN_002a46d8`, `FUN_002a34a8`, `FUN_002a4680`, `FUN_002a4c38`, `FUN_002a5060`, 0x2a54c8
  (instructions), `FUN_002a5708`, `FUN_002a54a8`, `FUN_002a5218`, `FUN_002a4ba8`, `FUN_002a39b8`, `FUN_002a5a68`,
  `FUN_002ae3e8`, `FUN_002af398`, `FUN_002af2b0`, `FUN_002afa00`, `FUN_002af7a0`, `FUN_002ae790`, `FUN_002afca0`,
  `FUN_002b5e50`, `FUN_002b4b18`, `FUN_002b7208`, `FUN_002b72c0`, `FUN_002b9360`, `FUN_002b9480`, `FUN_002a2018`,
  `FUN_002af688`, `FUN_002a1e78`, `FUN_00139918`; data 0x381f88, 0x381fb8, 0x381fc0, 0x381fc8, 0x380ab0,
  0x382b98, 0x382ba8, 0x382bac, 0x382f04..0x382f30.

### 20.9 Scroll bar, NEW tags, Characters, Character Links, Treasures, Maps, Album (WP-MN12) [V unless marked]

Code: `menu_journal.rs` and its submodules `menu_journal/{marks,chars,lists,treasure_page,album_page}.rs` (`marks`, `lists` and the
data parts of `chars` / `album_page` are pure functions with tests; the rest is node glue).

- **Text colour (the rule behind several pages).** `FUN_00139048` draws an object's text with the colour `FUN_00237518`
  gives (`+0xdc` of the sprite player, computed by `FUN_00236cf8` together with the group's anchor: the animation flagged 2 that
  covers the frame, its colour blended from start to end). Flag 8 (set by `FUN_00138fc8`, cleared by `FUN_00139860`) replaces the
  RGB with the object's own word `+0x198` (0xe5e5e5), keeping the group's alpha. Objects that clear flag 8 therefore take the
  group's whole colour: group 2 (64, 70, 60, 0x80: the dark ink of the album rows), 0x46 (16, 24, 0: the Treasures labels), 0x53
  (16, 24, 0: the name plate), 0x7e (240, 240, 240: the info text on its black bar), 0x7f (250, 240, 0: the total's count), 0x55 /
  0x56 (16, 8, 0 / 16, 16, 0 with alpha 100 / 96: the picture page's caption and title). The port reads the end colour of the group's
  last flagged animation (`group_color`; a fade-in such as the total label's is not followed). The holders of pages 1 and 2
  (flag 8 kept) and the rows of page 2 (WP-MN10, group 3: 0x80 white, the same look) are left as they were.
- **Holder texts of pages 1 and 2** (fix): `FUN_00139048` starts at the object's position `FUN_00138f38` (the group's animated
  anchor) and adds the group's x / y when flag 0x10 is kept; the port drew them from the node's origin, at the screen's corner.
  They now use `label_anchor` (the Summary and Objective texts sit in their panes).
- **Scroll bar** (`FUN_00299878`, called by the builds of page 3 with the 0x62 layer and of pages 7 / 8 with the 0x43 layer;
  `FUN_00299c10` moves it): a four-part object (`FUN_0029bdd0`: a part is a layout object with a group, a follow-up group in +0x150
  and an offset / scale; `FUN_0029cae8` places them from the parent's position plus the object's x / y) whose groups come from the
  table 0x381fd0 (three equal sets 1, 0x75, 0x74, 0x76: track [empty], top cap, body, bottom cap; a part's follow-up is the same
  group again, so it loops). x / y = the parent layer's current group x / y; the track is that group's +0x18 (224 on page 3, 200
  on pages 7 / 8), the body's own length the +0x18 of group 0x74 (100). Body scale y = `track / body * min(visible / total, 1)`
  (`total` as given, `visible` 10 or 9). `FUN_00299c10(pos)`: `pos` is clamped to `total - visible` (`total` here is the larger of
  the two numbers); parts 1 and 2 sit at `(int)(track / total * pos)`, part 3 at that plus `(int)(track * min(visible / total, 1))`,
  one less when the list fits and the sum is exact. Port: `marks::Bar` (tested), `place_bar`, `bar_pos`.
- **Page 3 opens on the last entry** (fix): `FUN_002a4c38` ends with `FUN_00298d98(list, n)` and `FUN_00299c10(bar, n)`
  where `n` is the sum of the lines of every entry but the last; the first line shown is the last entry's first line (the
  port started at line 0). The scroll bar is made for `n + max(10, last entry's lines)`.
- **New / seen bit sets** (`marks.rs`). `DAT_0036d4e8` (0x128 bytes, allocated by `FUN_002b7060` at the scene's init;
  `FUN_002b7100` fills it at every open: `FUN_002b7160` characters (+0), `7590` the Ansem reports (entry 4, +0x120), `7818` album
  (entry 2, +0x34 + 4 * world), `7cd0` limits (entry 6, +0x74), `7f58` quests (entry 8, +0x80), `8390` minigames (entry 7,
  +0x78), `87b8` treasures (+0xd0), `8b88` maps (+0x110), `8f18` (+0x124), `9198` story (entry 1, +0x90 + 4 * world)) holds a bit for
  every entry that exists now and is not in the seen set. The seen set is in the save at `FUN_002b70e0` = `0x33e860 + 0x38f0`
  (the same offsets). A tag is drawn for an entry with a new bit: new until the seen bit is set, then the seen group (`FUN_002b9360`
  / `FUN_002b9480` and siblings: story 0x39 / 0x3a at 0x382ba8 / 0x382bac, rows 0x8b / 0x8c at 0x382b80 / 0x382b84, cells 0x98 / 0x99
  at 0x382aec / 0x382af0, characters 0x18 / 0x19 and links 0x1b / 0x1c at 0x382f0c.., treasure and map labels 0x3f / 0x40 at
  0x382f58 / 0x382f5c, album 0x5a / 0x5b at 0x382bdc / 0x382be0). `DAT_0036d4f4` is a second set (the completion percentage
  `FUN_002b9bf0`: every entry that exists, no seen filter); its aggregate (`FUN_002baca8` story, `ba080` album, `ba6d8` treasures,
  `ba898` maps, `bad98` characters / links) draws the completion mark 0x8a on page 2's rows. Port: the app has no journal save
  data, so the seen set lives in the `Journal` resource for the run and starts empty (a state that never opened the journal shows
  everything as new [inferred: the new-game save has the bits clear]); the new set is rebuilt in `show` from the story flags, the
  item counts and the tables below.
  - *Page 3's marks* (`FUN_002a5708`, every cursor move through the list callback `FUN_002a5450`): the entries that start in the
    ten rows get a tag on their first row; the entry whose first line is on row 0 is marked seen first (`FUN_002b95d8`).
    `marks::story_mark_rows`.
  - *Page 2's rows* (`FUN_002a39b8`): per category the second child shows the aggregate (any entry new: 0x8b, all of them seen:
    0x8c) and the first the completion mark 0x8a. Characters: `FUN_002af5e0(0, world)`, links `(1, -1)`.
  - *Page 0's cells* (`FUN_002a2248`): an open world's second child shows `FUN_002b9658` / `FUN_002b9798` (any category new /
    all seen; world 0xf takes every world); the first child's 0x9a (`FUN_002baf08`: every category complete) needs the quest,
    minigame and limit lists the app has no data for: GAP.
- **Characters (6) and Character Links (0xb)** (one builder `FUN_002ae3e8`, handler `FUN_002ae790`; page 6 is category 0, 0xb
  category 7; page 0x20, which shares the builder and is reached from page 0xc, is not ported). The faces are the sub-records (8 bytes: character u16, x, y i16, +6 drawn when not 0)
  of the groups of `jiminy.bar` entry 5 (0xc-byte headers: +0 flag, +2 off flag, +4 family = the journal world or 0x10 for the
  links, +5 count, +6 kind: 0 base, 1 flag, 2 item count, +8 offset), `FUN_002af398`: the base group when it is on
  (`FUN_002a2018`: kind 2 reads `FUN_001a1cc8(item) > 0`, else the story flag, and not the off flag), then every group of the
  family that is on, in file order. A face is a frame object (group from 0x382f30 by the character record's style byte +2: 0xa..0xf,
  0x10..0x15 for the links), its picture (a child in sequence 0 of `jmemo.2ld`: groups 1 then 2, links 4 then 5, at
  0x382f20 / 0x382f24 / +6; the texture is picture `+1` of `jmface_<name>.2ld`, `<name>` from the pointer table 0x36d3f8 by
  family: tt hb bb he al mu po lk lm dc wi nm ca tr eh, "", ex, heart, no) and a tag child. Entry 3 ("char", 0x24-byte records): +1 picture, +2 style,
  +4 bit, +6 name message, +0x10 flags (0x400: the world's name is the message's variable, `FUN_00191a30`). The pointer is object
  0x7571 group 0x22 moved three units a frame while a direction is held inside the bounds of group 0x52 (its anchor to its x /
  y: (60, 80) to (490, 370); `FUN_002aeea8`, digital branch; left beats right, up beats down), first at the middle of the bounds
  or on the face the page was left on (`FUN_002aebb8`). The face under it is the first whose centre is within half the frame
  group's +0x20 (32) on both axes (`FUN_002af180`, inclusive); its name goes on the plate (group 0x53, font 3, centred, the
  group's colour) and the hover frame (0x16, 0x1a on the links, a child of the face's picture) appears. The tag of a face:
  0x18 / 0x19 (links 0x1b / 0x1c) for a character with a new bit. Cancel goes back to page 2 (row kept), Start closes; a confirm
  opens the character's own page 0xf (`FUN_002aa358`, with its list pages 0xe / 0x10): **not ported, refused with a log line**,
  so a character's seen bit (`FUN_002b73b8`, set there) never gets set in the app. Help line 0x48d5 (page 6), 0x48d6.
  Run (the state of an area that has Sora, Donald, Goofy and the friends): `local\x\state-c.json`-like
  `{"story_flags": [..., 2197, 2201, 5161]}`, `KH2_KEYS=7:Enter,8:Down,8.2:Down,8.4:Down,8.6:Down,9.2:J,10.5:J` (page 6),
  `KH2_JOURNAL_HOLD=11.5:Down:0.6` holds a direction for the pointer (the scripted keys are released every frame while the
  window has no focus), links: five Downs on page 2, then J. Log: `journal: page 0x6 (FUN_002ae3e8): family 0x0, 6 faces ...`,
  `journal: pointer [275, 303] -> face Some(2)`.
- **Treasures (7) and Maps (8)** (one builder `FUN_002afca0`; objects `FUN_002b0e78`; redraw `FUN_002b0678`, `FUN_002b09e8`,
  `FUN_002b0cc8`; handler `FUN_002afe98`; cursor `FUN_002b1890`; pages 0x19 / 0x1a, the same builder reached from page 0xc [inferred: Roxas's], are not ported). The rows of the table `trsr`
  (`03system.bin`, `DAT_01c95590`, 0xc-byte rows, iterated by `FUN_001a68d0`): treasures (`FUN_002b00e0`) are the chests (type 0)
  whose flag (+8) is neither 0 nor 0x800 and is set (or the all-worlds flag 0x4837 `FUN_002783f8`); maps (`FUN_002b0228`) are
  the rows whose item is of type 0x16, owned (`FUN_001a1cc8`) or listed by the all-worlds flag. An entry: the journal world
  index of the row's game world (`FUN_00277c88`), found (a treasure: the chest's bit `FUN_001a6a90` at `DAT_0033fdcc`; a map: owned),
  the row's ordinal and item. Sorted by world then ordinal (`FUN_002b0358`); a section per world of ceil(n / 8) rows, an open
  world without entries keeps one (empty) row, treasures leave out world 8 (`FUN_002b1650`). Page objects: layers 0x3b and 0x43,
  nine rows of eight cells (group pitch 28 x 28 from 0x48; found / not found 0x48 / 0x47, maps 0x3e / 0x3d, blank 2), a label per
  row (font 3, group 0x46, shown where a section starts; the world's name, or one question mark per glyph while the world's cell
  does not exist `FUN_0029bb48`: glyph 41, the sprintf of `FUN_001918a0` turns the ASCII `?` into it) with "There's nothing yet!"
  (0x4a0f / 0x4a12) at the label's offset for an empty section and the new tag 0x3f / 0x40, the info text under the cursor
  (`FUN_002b0cc8`: found: the item's name, with its second icon on page 7 (`FUN_001a1ee0`: control 9, item +0x17); not found:
  question marks; group 0x7e, centred, offset = the layer group's x / y), the total `found/listed` (only with the all-worlds
  flag; label 0x4a11 / 0x4a13 group 0x3c, count group 0x7f), the scroll bar (parent 0x43) and the cursor frame 0x41 (0x42 on
  an empty row) with the list hand (0x21, `FUN_00277158`) on it. The page opens scrolled to the journal world's section
  (`FUN_002b1e18`). Cursor (`FUN_002b1890`, `lists::navigate`, tested): Down / Up step a row and scroll at the view's edge, a new
  press wraps at the ends; L1 / R1 move a view (to the end, then to the row's last cell, then wrap); Left / Right wrap inside
  the row; the column is cut to the new row's cells. Each move marks the world under the cursor seen (`FUN_002b0df8` ->
  `FUN_002b8a98` / `FUN_002b8e38`). Help 0x48f4 / 0x48f5. **The app has no opened-chest data, so every listed chest is "not
  found"** and a world's treasure completion mark needs a world without chests. Run: `local\x\state-t.json`-like flags of the
  chests (`2256 2210 2286` for Twilight Town), `KH2_KEYS=...,10:Down,10.2:Down,10.4:Down,11:J` (treasures), four Downs
  (maps; the all-worlds flag 18487 lists unowned maps). Log: `journal: page 7 (FUN_002afca0): 164 entries (164 new), 0 found,
  24 rows, cursor ..., section rows [...]`, `journal: page 7 cursor Some(Cursor { top: 9, at: 1 }) after Right`.
- **Album (4)** (`FUN_002a5a68`, handler `FUN_002a6010`, helpers `FUN_002a5ef0`, `FUN_002a6e48`, `FUN_002a6e78`). Entry 2 ("albu",
  0xc bytes): +0 world, +1 / +2 two ASCII digits, +6 flag, +8 title, +0xa caption (messages of `jm.bar`). The list: the
  records of the world whose flag is set (or all of them with the all-worlds flag: those show question marks), eight rows from
  layer 0x58 (pitch 26), font 3, group 2's colour, a tag child (0x5a new / 0x5b seen), or one row "There's nothing yet!"
  (0x32dd). The row under the cursor counts as seen. Up / Down wrap (the list widget), cancel goes to page 2 (row kept), a confirm
  opens page 5; page 4's own first row is the one page 5 kept (`FUN_00277de8(4)`), else 0. Help 0x48d8.
  - *Picture page 5* (`FUN_002a62d0`, `FUN_002a6850`, `FUN_002a6c80`, `FUN_002a6bd8`, `FUN_002a6ad0`): layer 0x54, the picture
    (a sequence-1 object, groups 1 then 2, at the layer group's x / y (160, -113)) from `jm_photo/<world code><digits>.bin` (a BAR
    of one image), the title (text object group 0x56) and the caption (list row 0, group 0x55). L1 / R1 go to the next / previous
    existing picture (`FUN_002a6850`, wrapping), cancel goes back to page 4 with the row kept (`FUN_00277dc8`), help 0x48d9.
- **Gaps (WP-MN12).** The characters' own page 0xf and its list pages (`FUN_002aa358`, `FUN_002ab7d8`, `FUN_002a98f0`; confirm on a
  face is refused); the album's and the characters' seen bits of the entries only those pages mark; the minigames and quests
  pages (9, 10: `FUN_002b5e50`, `FUN_002b4b18`) and Roxas's pages 0xc, 0x19 .. 0x20; page 0's completion mark 0x9a and the quest /
  minigame / limit aggregates; opened chests (treasures never show as found); the wipe, sounds and the `FUN_002779f8(0, 2)` header
  hide the handlers do when a direction is pressed; the stick-driven pointer (`FUN_002aeea8` analog branch: `stick * 6 + 0.5`);
  the held-pad auto-repeat of the cursor moves (the port takes one step per press, wraps on every press).
  Text colour of page 2's rows (see the rule above) was left at the WP-MN10 reading.
- **WP-MN14 status.** The character page 0xf, the Quests and Minigames pages, page 0xc and page 0's completion mark 0x9a are ported (20.10); what stays open is the 3D model, the enemy pages, the Ansem reports, limits and the moogle list.
- **WP-MN15 status.** The text lists of page 0xc (the Character Files worlds and characters, the Heartless, the Nobodies, the Ansem reports, the limits) and the pages behind them, the enemy pages with the defeated-enemy count and the reaction rows, the moogle summary and world 0xf's mark 0x9a are ported (20.11); the defeated-enemy counters, the reaction use counts and the limit counts are saved in the slot (`docs/systems/core-runtime/progress.md`). The 3D model pages were ported in WP-JR2 (20.12). WP-HK1 added the writers: see 20.11a.
- **WP-MN13 status.** Nothing in the app opens chests yet (gap, WP-FX12c): the original sets a chest's bit in `FUN_001a6910` (reached by `FUN_001a6a58` from the chest-open sequence `FUN_001a6db8`, and by `FUN_001a6a78` from the reward list of an area trigger in `FUN_001978b0`, whose only callers are `FUN_00197670` and `FUN_00197a60`), and the app has no chest object and does not run a trigger's reward list, so a chest is "found" only through a state file. Done: persistence. `Progress` keeps the opened chests (`chest_opened`, record bits from 224 on) and the journal seen set (`journal_seen`, slot key `journal_seen`); `Marks` loads it when the journal opens (`load_seen`) and the tick writes new marks back (`take_dirty`); the Treasures page marks chests found from the chest bit and a world's treasure completion needs every chest of it opened (`treasures_complete`). Not started (no decompile work done beyond reading): the character page 0xf (`FUN_002aa358` builds layer group 0x49, name 0x4a, sub-title 0x4b from `char` +0xa, a counter row 0x4c when `char` +0x10 has 0x800, scroll arrows 0x4f / 0x50, the face frame by style from 0x382ea0, the description message `char` +8 in a 6-line scissored view (`FUN_002ab678`); pages 0x12 / 0x15 add the four drop rows, so those are the enemy pages; the face is loaded through the slot cache `FUN_002a07c8`), the Quests / Minigames pages (page 9 reads entry 8 via `FUN_002b6478` / `FUN_002b6650`, page 10 entry 7 via `FUN_002b5130` / `FUN_002b52c0`; ten rows of group 0x68 / 0x6a with a child 2, world header rows group 100, values from the minigame records at `0x341620 + 8 * n` (`FUN_001d36e8`: byte 0 state, +4 score) and `FUN_002b6d40` for quests; both lists are empty without the all-worlds flag since the app keeps neither record), page 0xc, Roxas's pages, page 0's mark 0x9a.

- **Decompile index (MN12).** `FUN_00299878`, `FUN_00299bf0`, `FUN_0029bdd0`, `FUN_0029bfa0`, `FUN_0029be88`, `FUN_0029c0b0`,
  `FUN_0029c130`, `FUN_0029c208`, `FUN_0029c2d8`, `FUN_0029c408`, `FUN_0029c440`, `FUN_0029c490`, `FUN_0029c568`, `FUN_0029c5b0`,
  `FUN_0029c880`, `FUN_0029c8f0`, `FUN_0029cae8`, `FUN_00299c10`, `FUN_00277e58`, `FUN_00298d98`, `FUN_00298d58`, `FUN_00298e08`,
  `FUN_00296fb8`, `FUN_002a4c38`, `FUN_002a5450`, `FUN_002a5708`, `FUN_002a5060`, `FUN_002b7060`, `FUN_002b70a8`, `FUN_002b70e0`,
  `FUN_002b7100`, `FUN_002b7160`, `FUN_002b7208`, `FUN_002b72c0`, `FUN_002b73b8`, `FUN_002b7408`, `FUN_002b74b8`, `FUN_002b7590`,
  `FUN_002b7668`, `FUN_002b76e8`, `FUN_002b7818`, `FUN_002b7928`, `FUN_002b79d8`, `FUN_002b7af8`, `FUN_002b7c50`, `FUN_002b7e28`,
  `FUN_002b7f58`, `FUN_002b8048`, `FUN_002b8390`, `FUN_002b87b8`, `FUN_002b88b0`, `FUN_002b8990`, `FUN_002b8a98`, `FUN_002b8c58`,
  `FUN_002b8d38`, `FUN_002b8e38`, `FUN_002b8fd0`, `FUN_002b9058`, `FUN_002b9198`, `FUN_002b92b0`, `FUN_002b9360`, `FUN_002b9480`,
  `FUN_002b95d8`, `FUN_002b9658`, `FUN_002b9798`, `FUN_002b99d0`, `FUN_002b9ab0`, `FUN_002b9b28`, `FUN_002b9bf0`, `FUN_002b9ce8`,
  `FUN_002b9e30`, `FUN_002b9f68`, `FUN_002baca8`, `FUN_002ba080`, `FUN_002ba6d8`, `FUN_002ba898`, `FUN_002bad98`, `FUN_002baf08`,
  `FUN_002a1c30`, `FUN_002a1f58`, `FUN_002a1f78`, `FUN_002a1ff8`, `FUN_002a2018`, `FUN_002a2090`, `FUN_002a39b8`, `FUN_002af5e0`,
  `FUN_002ae3e8`, `FUN_002ae790`, `FUN_002ae230`, `FUN_002ae2a0`, `FUN_002af180`, `FUN_002af2b0`, `FUN_002af398`, `FUN_002af688`,
  `FUN_002af6c0`, `FUN_002af730`, `FUN_002af7a0`, `FUN_002af800`, `FUN_002aebb8`, `FUN_002aecf0`, `FUN_002aee60`, `FUN_002aeea8`,
  `FUN_002afa00`, `FUN_002aa358`, `FUN_002a19f8`, `FUN_002a18e8`, `FUN_002a19c8`, `FUN_002a1858`, `FUN_00278658`, `FUN_00277d60`,
  `FUN_0029d1d0`, `FUN_0029d820`, `FUN_002afca0`, `FUN_002afe98`, `FUN_002b0228`, `FUN_002b00e0`, `FUN_002b0358`, `FUN_002b0e78`,
  `FUN_002b1e18`, `FUN_002b0df8`, `FUN_002b0c98`, `FUN_002b0678`, `FUN_002b09e8`, `FUN_002b0cc8`, `FUN_002b1c30`, `FUN_002b1ca0`,
  `FUN_002b1d78`, `FUN_002b1398`, `FUN_002b1890`, `FUN_002b16e0`, `FUN_002b16a0`, `FUN_002b1650`, `FUN_002b1660`, `FUN_002b1680`,
  `FUN_001a68d0`, `FUN_001a66e8`, `FUN_001a6a90`, `FUN_001a1cc8`, `FUN_001a1ee0`, `FUN_0029bb48`, `FUN_00191660`, `FUN_00191020`,
  `FUN_00190f68`, `FUN_001918a0`, `FUN_00277158`, `FUN_002771c0`, `FUN_0029a9c0`, `FUN_002783f8`, `FUN_002a3120`, `FUN_002a3008`,
  `FUN_002a5a68`, `FUN_002a6010`, `FUN_002a6578`, `FUN_002a5ef0`, `FUN_002a5fd8`, `FUN_002a6e48`, `FUN_002a6e78`, `FUN_002a59d8`,
  `FUN_002a62d0`, `FUN_002a6850`, `FUN_002a6c80`, `FUN_002a6b80`, `FUN_002a6bd8`, `FUN_002a6a38`, `FUN_002a6ad0`, `FUN_00139048`,
  `FUN_00138fc8`, `FUN_00139860`, `FUN_00139888`, `FUN_00237518`, `FUN_00237618`, `FUN_00237638`, `FUN_00237658`, `FUN_00237678`,
  `FUN_00237698`; data 0x381fd0, 0x381fe8, 0x382aec..0x382af0, 0x382b80, 0x382ba8..0x382bac, 0x382bdc..0x382be0, 0x382ef4..0x382f64,
  0x382f50, 0x3828b8, 0x36d3f8..0x36d460, 0x33fdcc.
- **Tests (WP-MN12).** `menu_journal::marks::tests::*` (tags, aggregates, the mark rows, the thumb), `chars::tests::*` (groups of a
  family, items, pointer, hit test, tags), `lists::tests::*` (rows and sections, the start, the cursor moves, pages, the view),
  `treasure_page::tests::*`, `album_page::tests::*`, `menu_journal::tests::{story_records_exist_with_their_flag_and_complete_with_the_last,
  the_disc_face_page_data, the_disc_list_page_data, the_disc_category_page_tables}`.

