# Shops and synthesis

[Contract](contract.md) | [Index](../../README.md)

## 22. Shops: menu kind 4 (WP-SH1)

### 22.1 Who opens a shop [V]

Command 30 is **"Examine"** (cmd row 30: Execute 0x1e, message 0x81eb), not a shop. The shopkeepers offer the **"Shop"**
rows (cmd rows 195..212: message 0x84c7, Execute 0x7f; also 213, 453 and 622 with Execute 0x7f and a shop event). Their
Arguments (cmd +4) are the shop events: 195 0x66, 196 0x74, 197..200 0x6c..0x6f, 201..204 0x70..0x73, 205 0x75, 206..208
0x83..0x85, 209..212 0x67..0x6a, 213 0x6b, 453 0x13, 622 0x8a (`menu_shop::tests::the_disc_shop_file_rows_and_shopkeepers`).
Twilight Town's shopkeepers (townspeople, objentry type 15): tt06 `k01_` `N_EX650_TT_ITEM_RTN` (212, event 0x6a, "Wantz's
Item Shop"), tt06 `z70_` `N_EX700_TT_WEAPON_RTN` (209, 0x67), tt07 `170_` `N_EX640_SHOP_RTN` (213, 0x6b), `q70_`
`N_EX680_TT_PROTECT_RTN` (210, 0x68), `t70_` `N_EX690_TT_ACCE_RTN` (211, 0x69); their groups are loaded by tt06 evt
programs 0xe, 0x10..0x14 and tt07 evt programs 0x10, 0x12, 0x16.

- Offer: the availability switch `FUN_001ba2c0` treats Execute 0x7f as it treats 0x37 (the save point): status 5 while
  `FUN_001c6c08(0x35fec0, 0)` answers non-zero; then the row's flags and form mask (`FUN_001b70b8`, `FUN_001b7290`).
- Accept: Sora's class handler gives Execute 0x7f to `FUN_00177b18`, case 0x7f: `FUN_00197b70(halfword +6 of the pending
  command record at actor +0xb44, 0)`. [inferred: that halfword is the row's Argument; its writer was not traced. Every
  row with Execute 0x7f whose Argument is a shop event has a record in the shop file.]
- `FUN_00197b70` sends 0x13, 0x66..0x75, 0x83..0x85, 0x8a to `FUN_0024f0b8(4, event)`: menu kind 4, mode word = event.
- Examine (`FUN_001a54a8`): `FUN_001bef48(0xc)`, `FUN_001b7ce8(0)`, `FUN_001a53f8(target +0x500, player, record, task
  FUN_001a5510, callback FUN_001a5608)`. The task turns the player to the target, runs the class-5 script's
  "before_check" mode around the program (`FUN_00189eb8`, `FUN_00189f28` = `FUN_00189ff0(0)`), and unlocks unless an event
  runs. The callback's op 1 opens window class `FUN_001ab7d8` (`FUN_001aaf20(win, 3, 4, message, 0, 0x100, 0x20, 3)`:
  class 2 of `kh2_image::window::CLASSES`, at the top of the screen) and waits for state 3 or `FUN_001a5b18(0xe)` = 0.
  On the disc the Examine targets are mostly `F_EX000` gimmicks (objentry type 8, 262 entities over all worlds) and one
  townsperson (tt03 `a61_` `B_EX110_RTN`); every program is one message, none fires a shop event.
- Text command 0x12 (`FUN_001a8978`, a page's layout) also calls `FUN_00197b70(u16, 0)`; no shop message uses it.

### 22.2 The shop file [V]

`FUN_00193610` keeps `03system.bin`'s type-0x29 entry ("shop") through `FUN_00265ac0` (`DAT_0036cfe4`). `FUN_00265660`:
magic `TZSH` (u32 0x48535a54), u16 version 7 at +4. +6 i16 record count (21 on the disc), +0xc u32 offset of a list of up
to 0x80 item ids (`FUN_00265ad0`: an item's place in it is its bit in the save's array at +0x3714, the shop's "new" marks,
`FUN_00265280`), +0x10 the records, 0x18 bytes each: +0 event, +2 a record bit (array 0x33fdb0), +4 the header message
(the shop's name), +6..+0xc four words copied to the state [GAP: their readers], +0xe flags, +0x10 group count, +0x14 u32
group offset. A group is 8 bytes: i16 record bit (negative: always on), u16 count, u32 offset of `count` item ids.

`FUN_00265688(event, state)` (the state is 0x820 bytes at `DAT_0036cfbc`): the record whose +0 is the event; its record
bit is set when clear (`FUN_001a52b0`); state +0 = message, +2..+8 the words, +0xc bit 0 = flag 0x100, bit 1 = flag 0x200,
+0xd = (flag 0x80 clear and the first group has items). Each group that is on adds its items with the kind mask 0x1f;
with flag 0x80 the always-on groups of every other record whose record bit is set and whose own flag 0x80 is clear add
theirs, with the record's flags word as the mask. An item is added once (`FUN_00265970`) and only when its kind bit
(`FUN_00265a50`: item type 0, 1 -> 8; 2..0xd -> 1; 0xe -> 2; 0xf -> 4; 0x10 -> 0x10) is in the mask. Twilight Town's
weapon, armor, accessory and item shops have flags 0x181, 0x182, 0x184, 0x188 (one kind each, shared); the moogle
records 0x31f (every kind, the third row, gated groups 0x49..0x60 of synthesis materials).

### 22.3 The pages [V]

Init `FUN_0025f168`, task `FUN_0025fc18`: loads `itemshop.2ld` (`FUN_0024f738(4)`), builds the screen (`FUN_0025fcd8`,
`FUN_0025fef0`), opens with sound kind 0x10 when state +0xc bit 1 is set, else 0xb, and runs `FUN_00260540` each frame.
Layout triples (enter, leave, hold) at 0x381240: (0xf, 0x11, 0x10), (4, 6, 5), (0xc, 0xe, 0xd) (`FUN_00263f00`).

- **Page 0** (`FUN_00260748`): four rows from the table 0x381300 (0x24 bytes, the camp entry layout): Buy 0x8acc, Sell
  0x8acd, Synthesize 0xbbb5, Quit 0x8ace. `FUN_00264da8`: Buy and Sell need state +0xc bit 0 and a non-empty list, the
  third row bit 1, Quit always. Confirm: Buy -> page 1, Sell -> page 4, the third row -> page 8 (`FUN_00284c10` and the
  synthesis pages), Quit or a cancel -> the end. -4 (Start) ends it from any page.
- **Pages 1 / 4** (`FUN_00260ec8`): the buy list (state +0xa count, +0x10 ids) or the sell list (`FUN_00264be8`: every
  item row in table order with a type below 0x12, a count, and a sell price; +0x41a count, +0x420 ids, +0x620 counts).
  `FUN_00265550` refuses a row (the refused sound): buying when the player has more than 0x62 or the price (item +0xc)
  is above the munny (save +0x1600, `FUN_0023e5c0`); selling when the sell price (item +0xe) is 0. A confirm writes the
  selection (`DAT_0036cfc4`: item +2, count +4 = 1, price +6, total +8) and goes to page 2 / 5; a cancel to page 0.
- **Pages 2 / 5** (`FUN_002614e0`, `FUN_00261a28`): the pad bits 0x10, 0x20, 0x40, 0x80 (Up, Down, Left, Right) step the
  count by +1, -1, -10, +10. Buying, owned + count stops at 99 and a step the munny does not cover is cut to what it
  covers (none: refused); selling stops at the owned count; the count stays at least 1. Confirm -> page 3 / 6, cancel ->
  page 1 / 4.
- **Pages 3 / 6** (`FUN_00261d70`, rows by `FUN_00261bf8`): row 0 "No" (0x8ad0), row 1 "Yes" (0x8acf), the cursor on row 0.
  Row 0 or a cancel: sound kind 8, back to page 2 / 5. Row 1: sound kind 0xf; buying `FUN_0023e6b0(total)` (munny down)
  and `FUN_001a1a18(item, count)`; selling `FUN_0023e620(total)` and `FUN_001a1c30(item, count)` (`FUN_00283480` when the
  item is gone); then the munny digits, the lists again (`FUN_00264be8`) and page 1 / 4.
- **Page 7** (`FUN_00262140`, `FUN_002621a0`): when `FUN_00265498` (flags not 0x200 alone, and the save's byte +0x3724 bit
  `1 << (state +0xd != 0)` clear) the shop opens on a text (0x381128 or 0x38112c) and sets that bit [ported in WP-SH3:
  section 24.1].
- The end (`FUN_00260540` once closing): waits for the objects, `FUN_00284ae0`, `FUN_00262de0`, `FUN_001038a0` (back to
  the field).

### 22.4 What the port does (`crates/kh2-app/src/menu_shop.rs`)

`reaction_cmd::status` offers Execute 0x7f rows (blocked in a drive form, the part of 0x35fec0 the app has [approx]);
`reaction_cmd::accept` turns an accepted Shop row into `ShopRequest(event)`. `ShopPlugin` reads the shop file, the item
table and `itemshop.2ld` on the first request, builds the stock (`menu_shop::stock`, the record bit kept in
`Progress::record_bit`), dims the field and freezes it (`Pause::menu`), and runs pages 0..6 with the rules above
(`menu_shop::Flow`, `step_quantity`, `row_open`, `sell_list`, `deal`). The frame is the layout triple's enter layout
holding on its hold layout; the rows, names, prices, counts, the detail line, the question and the munny are text labels
the port places [inferred positions, not read from the list widget]. A close plays the leave layout and the camp's
close dim. Examine (command 30) runs the talk program in window class 2 (`npc.rs`).

Not ported (the first-visit text and the "new" marks came in WP-SH3, section 24): the list widget's own look (row plates, the pointer, scrolling), the help line, the munny plate's digits art, the open
sound's exact entry [inferred: kind k is the u16 at 0x37f654 + 2 k, the rule of kinds 0xc and 0xd].

Harness: `KH2_PROGRAMS=0,0,10 KH2_NPC_WATCH=2 KH2_NPC_WATCH_DIST=60 KH2_STATE=<{"munny": 500}> KH2_REACTION_PRESS=8
KH2_KEYS=10:J,10.6:J,11.2:Up,11.8:J,12.4:Down,13:J kh2-game tt06` buys two Potions at Wantz's. Log lines: `reaction:
accepted command 212 (Execute 0x7f, FUN_00177b18): event 0x6a`, `shop: open menu kind 4`, `shop: page 1 -> page 2`,
`shop: bought item 0x1 x2 for 80`, `shop: close`, `shop: menu kind 4 (0x6a) closed`.

### 22.5 Decompile index (WP-SH1)

| address | what |
|---|---|
| 0x177b18, 0x1778e8, 0x1ba2c0 | the Execute switch (case 0x7f), the pending command, the availability switch (case 0x7f) |
| 0x197b70, 0x24f0b8 | events to menu kind 4 |
| 0x1a54a8, 0x1a5510, 0x1a5608, 0x1a53f8, 0x1a53b8, 0x1ab7d8, 0x1aaf20, 0x1aacb0, 0x189eb8, 0x189f28 | Examine |
| 0x1a8978 | text command 0x12 fires an event |
| 0x193610, 0x265ac0, 0x265650, 0x265660, 0x265688, 0x265970, 0x265a50, 0x265ad0, 0x265280 | the shop file and the stock |
| 0x25f168, 0x25fc18, 0x25fcd8, 0x25fef0, 0x264da8, 0x264e20, 0x265498 | init, task, screen, top rows |
| 0x260540, 0x260748, 0x260ec8, 0x2614e0, 0x261910, 0x261a28, 0x261bf8, 0x261d70, 0x262140, 0x2621a0, 0x263f00 | the pages |
| 0x264b98, 0x264be8, 0x265550, 0x255588, 0x1a6730, 0x23e5c0, 0x23e6b0 | rows, the sell list, the refusals, munny |
| data 0x381240, 0x381300, 0x37f654, 0x381128 | layouts, top rows, sound kinds, first-visit texts |

Gaps: the writer of the pending command's +6 (searched: `FUN_001778e8`, `FUN_0019c670`); the readers of the record's words
+6..+0xc; `FUN_001c6c08`'s bits 2 and 3 of 0x35fec0; `FUN_001a5b18(0xe)`; `FUN_0023e680`; the list widget's positions
(`FUN_00296b48`, `FUN_0029b600`, `FUN_00296d10`); page 7. The third row (pages 8 and after): section 23.

## 23. The shop's third row: synthesis (WP-SH2)

### 23.1 The data [V]

`FUN_00284798` (the third row's setup) allocates the page state (`DAT_0036d194`, 0x1130 bytes), the screen objects
(`DAT_0036d198`), the data holder (`DAT_0036d19c`) and the session marks (`DAT_0036d1a0`), and lists the item rows of type
0x10 in table order into the state's +0x314 (the material list, at most 0x32; 50 on the disc). `FUN_00291ae0` loads
`menu/us/mixdata.bar` (name at `PTR_s_mixdata_bar_0036d190`); `FUN_0029e630(holder, k)` answers entry k:

| k | tag | layout (after a 0x10-byte header: magic, version 3, u32 count) | readers |
|---|---|---|---|
| 0 | `reci` | 0x20 per recipe: +0 the recipe's item id, +2 kind (0 taught by its item, 1..3 a development set), +3 rank, +4 the item made, +6 the higher-rank item (0 none), +8 six (u16 material, i16 count) lines ending at a non-positive id | `FUN_002906e0`, `FUN_00290058`, `FUN_00290ef8`, `FUN_00290468` |
| 1 | `cond` | 0xc per collection reward (+0 text, +2 reward, +4 type, +5..+7 condition, +8 count, +0xa record bit) | `FUN_00291140` (ported in WP-SH3: 24.3) |
| 2 | `leve` | 0xc per level: +0 name, +2 text, +4 i16 unlock bit, +8 u32 points | `FUN_00290bb8`, `FUN_00290c28` |
| 3 | `exp` | +0x10..+0x16 the points of a material by its rank (item +5), +0x18 f32 the first bonus' factor, +0x1c a story flag, +0x1e the unlock bit it grants | `FUN_00290468`, `FUN_00284c10` |

On the disc (`menu_shop::tests::the_disc_mixdata_recipes_levels_and_tables`): 23 recipes (10 of kind 0), 50 rewards, 9
levels with rising thresholds, factor 2.0, the story flag's bit is 10. The executable's tables: 0x3825e0 the seven row
messages of page 9, 0x382690 / 0x382688 / 0x382680 the bonus materials by rank (`FUN_002910f0(rank, k)` for k 0, 1, 2),
0x382578 a byte per rank (the unlock bit that halves that rank on the made list), 0x382548 the detail rows' help,
0x382628 the opening notices, 0x3826a0 the level names, 0x3825b8 the layout triple (8, 10, 9) of these screens.

### 23.2 The record [V]

`FUN_002906c0` answers save +0x3728. +0/+4 a bit per material once handed over; +8 the reward bits; +0x10 two bits per
recipe (bit 2r learned); +0x18 one bit per list entry made (entry 2r the item, 2r + 1 the higher rank); +0x20 the same
once seen; +0x28 the unlock bits (`FUN_002903b8`): 0..2 the development sets, 3 the points and halving bonuses, 4 the
higher rank, 5 two bonuses at once, 6..9 the made list's halving per rank, 10 the recipe row; +0x30 the points (at most
99,999); +0x34 the stock and +0xfc the totals, a word per material slot (at most 999); +0x1c4 the level. The port keeps
it as `progress::Moogle` (`Progress::moogle`; a slot file carries it; `KH2_STATE` key `moogle`: the bit sets, `points`,
`level`, `stock`, `totals`).

### 23.3 The pages [V unless marked]

- **Opening** (`FUN_00284c10`, page 8): the `exp` story flag set grants its bit (10). `FUN_00288758` / `FUN_002887c8`:
  every material held goes to the stock and the totals (999 at most each) and leaves the inventory; its known bit is
  set. With bit 10, `FUN_002888f8` / `FUN_002889e0`: a kind-0 recipe whose item is held and that is not learned is
  learned and its item taken (a learned one's item stays; an item of the other kinds is taken). `FUN_00291140` checks
  the rewards [ported in WP-SH3: 24.3]. The first page is 10 after a hand-over, else 0x10 after recipes, else 9; the notices of
  the state's +0x130 bits come first (`FUN_00287950`, page 0x13: bit 0 the hand-over, bit 5 the recipes, 1..4 rewards).
- **Page 9** (`FUN_00284ea8`, `FUN_00285388`): seven rows; row 0 shows when something was made, row 1 with a set bit
  0..2, row 2 with bit 10, rows 3..6 always. Row k leads (`FUN_0028a6f0`): 0 -> 0xc, 1 -> 0xe, 2 -> 0x10, 3 -> 0x17,
  4 -> 10, 5 -> 0xb, 6 -> page 0 (the shop's top rows, `FUN_0025fef0`); a cancel -> page 0, -4 the end.
- **Pages 0xc / 0xe / 0x10** (`FUN_00290058`, `FUN_00288fc0`, `FUN_00286580`): the made list (every entry made), the
  development list (learned recipes of sets 1..3 whose bit is set, not made), the recipe list (learned kind-0 recipes not
  made); at most 0x2d entries, an empty list shows 0xbe22. An entry is dimmed and refused (`FUN_00291b58`) unless some
  way of making it is in stock. A confirm opens its detail page with rows (+0x106b): make and quit always, the points
  bonus and the halving bonus with bit 3, the higher rank (a recipe with +6) with bit 4 (`FUN_002908c0`); the cursor
  starts on make when the item can be made as it is, else row 2 (3 for a higher-rank entry) [inferred: the widget
  keeps a cursor past the last row on the last].
- **Need** (`FUN_002906e0`, `FUN_00290618`): a line needs `count / div` rounded up, at least 1; div is 1, 2 with the
  rank's bit (0x382578) on the made list, doubled by the halving bonus. The higher-rank bonus and the halving bonus
  each need one of their material (k 2, k 1); both at once need bit 5.
- **Detail pages 0xd / 0xf / 0x11** (`FUN_00286bc8`, jump table 0x38249c): row kinds 1..3 toggle the bonus flags
  +0x106a bits 0..2 when `FUN_00291b58` allows (a flag already set always; else fewer set than allowed, 1 or 2 with bit
  5, and `FUN_002908c0` with the stock check; on 0xf and 0x11 not the higher rank; the flag tested is the shown row's
  minus one). Make (0x286f40): a higher-rank entry without that bonus -> 0xbef4, no way in stock -> 0xbef5, the item
  at 99 -> 0xbf06 (page 0x12, a text then back); else page 0x14 (`FUN_0028ee28`: 0xbef6 and the item's name). Quit and
  a cancel go back to the list.
- **Pages 0x14..0x16**: closing the result window makes it (`FUN_00287fb0` -> `FUN_00288170`: `FUN_001a1a18(item, 1)`,
  the item being +4 for a plain entry without the higher-rank bonus, else +6). `FUN_00290ef8` / `FUN_002909d0` take
  each line's need and one of each chosen bonus material from the stock and add the points (`FUN_00290468`: each line's
  full count times its rank's points, times the factor with the points bonus) [the original counts them down a frame
  at a time; the port at once]. `FUN_00288228` marks the entry made and seen (2r + 1 when the bonus raised a plain
  entry), `FUN_00290c28` grants the bits of the levels reached and the development sets they open (`FUN_00290cc0`),
  `FUN_00290bb8` sets the level; each level gained shows 0xbed1 with its name (page 0x16), then the list again.
- **Pages 10 and 0x17** (`FUN_002858e8`, `FUN_00287240`, `FUN_0028f9a8`): the materials with a total (10, titled 0xbcdf)
  or with stock (0x17, 0xbd90); a cancel goes to page 9 (the rows, their NEW tags and the story flag's extra rows: 24.9).

### 23.4 What the port does (`menu_shop::synth`)

`ShopMenu` loads `mixdata.bar` and the tables with the shop; the third row's confirm runs `synth::Session::start` (the
opening) and the pages above (`Session::key`, `entries`, `entry_open`, `detail_rows`, `make_check`, `make`), drawn as
text labels over the synthesis layout triple like the shop pages [inferred positions; the detail page also lists each
line as name and need / stock, an inferred layout: `FUN_002894f0` and `FUN_00289aa8` were not read]. Harness:
`KH2_PROGRAMS=0,0,16 KH2_NPC_WATCH=5 KH2_NPC_WATCH_DIST=60 KH2_REACTION_PRESS=8 KH2_STATE=<{"munny": 500, "items":
{"382": 1, "345": 6, "329": 2, "317": 2, "333": 2}, "moogle": {"unlocks": [10]}}> KH2_KEYS=10:J,11:J,11.6:J,12.2:K,12.8:J,
13.4:J,14:J,14.6:J,15.2:K,15.8:K,16.4:K kh2-game tt07` makes a Mega-Potion at the moogle (`N_EX640_SHOP_RTN`, event
0x6b): `synth: open the third row`, `synth: made item 0x5`, `shop: close`.

Not ported (page 0xb, the rewards and the NEW marks came in WP-SH3, section 24): the
moogle's animation (`FUN_00283520`, item +0x14), the window and list widgets' own look, the frame-by-frame count-down.

### 23.5 Decompile index (WP-SH2)

| address | what |
|---|---|
| 0x260540 | the page switch, pages 8..0x17 |
| 0x284798, 0x284908, 0x291ae0, 0x291b38, 0x29e4a8, 0x29e630 | setup, teardown, `mixdata.bar`, entries |
| 0x284c10, 0x288758, 0x2887c8, 0x2888f8, 0x2889e0, 0x291140 | the opening |
| 0x284ea8, 0x285388, 0x28a6f0, 0x28a748 | page 9 |
| 0x286378, 0x286580, 0x288fc0, 0x290058, 0x291b58, 0x2906e0, 0x290618, 0x290400, 0x2910b0, 0x2910f0 | the lists, the need |
| 0x286bc8, 0x38249c (0x286f40 .. 0x287228), 0x2908c0, 0x290978, 0x289bf8, 0x290e68 | the detail page |
| 0x287780, 0x28e980, 0x287950, 0x2879e8, 0x2877a0 | texts, the opening notices |
| 0x287ee0, 0x28ee28, 0x287fb0, 0x288170, 0x288228, 0x290ef8, 0x2909d0, 0x290468, 0x290c28, 0x290bb8, 0x290cc0, 0x288480, 0x2884e8 | the make, points, levels |
| 0x2858e8, 0x285b30, 0x287240, 0x287498, 0x28ffc8, 0x28f9a8, 0x28aa40 | the material pages |
| 0x2906c0, 0x2903b8, 0x2783f8 | the record, the unlock bits, story flag 0x4837 |

Gaps: (page 0xb and the rewards' conditions are closed in section 24); the result window's key handling (`FUN_0028e7f0`, `FUN_0028f2c0`: the port makes the item on any confirm or cancel);
the list widget (`FUN_00296b48`, `FUN_00299878`); the entries `FUN_002783f8` (story flag 0x4837) adds with mark 0x80
are ported in 24.9; `FUN_002909d0` with a bonus material outside the list writes the stock at slot -1 (the points word) [not
ported].

## 24. Page 7, the shop's NEW marks, the collection rewards and page 0xb (WP-SH3)

### 24.1 Page 7: the first-visit text [V]

`FUN_0025fcd8` builds the screen, runs `FUN_0025fef0` (the top rows) and then asks `FUN_00265498`; true opens page 7 and
runs `FUN_00262140`. `FUN_00265498`: the state's halfword at +0xc (bit 0 the Buy and Sell rows, bit 1 the third row, byte
+0xd the "first group" flag of `FUN_00265688`) must not be exactly 2 under the mask 0xff02, i.e. the shop is not the
third row alone, and the save's byte at +0x3724 must have the bit `1 << (+0xd != 0)` clear. So there are two classes of
shop: those whose record has no flag 0x80 and a first group with items (the moogle's records, flags 0x31f; bit 1) and
the shared ones (flag 0x80: Twilight Town's weapon, armor, accessory and item shops, flags 0x181..0x188; bit 0). A
moogle without wares of its own (the third row alone: tt07's `N_EX640_SHOP_RTN`, event 0x6b) never shows a text.
The text is the message id held in a word of the executable: 0x381128 when +0xd is clear, 0x38112c when it is set
(0xc401 and 0xc402 on the disc, read at run time; both say that items of earlier shops reach the shops of the bigger
towns). `FUN_00262140` opens a message window through `FUN_00262350` (window object 0x7546 and a list widget 0x7549, the
text by `FUN_00190c40`). `FUN_002621a0` waits: only a list result of 0 (confirm) or -2 (cancel) closes it
(`FUN_00262450`); a Start press just plays its sound. When the window has gone (`FUN_002624a0`) it sets the bit
(`|= 1 << (+0xd != 0)` in the save byte) and returns to page 0.

### 24.2 The shop's NEW marks [V]

The shop file's list of up to 0x80 item ids (`FUN_00265ad0`: an item's place in it, -1 past the end) has one bit per
place in the save at +0x3714 (4 words). `FUN_0025fef0` runs `FUN_00265280` at every open: each item of the buy list
(state +0xa count, +0x10 ids) whose save bit is clear sets its bit in a per-visit set (`DAT_0036cfd0`, 0x14 bytes): the
items that are NEW this visit. `FUN_00265348(item)` sets the item's save bit when it is NEW this visit and the bit is
still clear (`FUN_002653d8`: NEW this visit; `FUN_00265420`: NEW this visit and its save bit is set now). It is called
for the item under the cursor when page 1 is entered (`FUN_00260a78`) and when the cursor moves (`FUN_00264668`,
`FUN_00260ec8`). The tags are sprite groups of `itemshop.2ld`: NEW = group 0x78 (word 0x381238), VIEWED = group 0x77
(word 0x38123c).
- Buy list rows (`FUN_002626e8`, page 1 only): an item that is NEW this visit has the NEW tag until its save bit is
  set, then the VIEWED tag.
- Top page (`FUN_00262bd0`): the Buy row (its display index `FUN_00264e80(0)`) has a tag when any item is NEW this visit:
  NEW while one of them is unseen, else VIEWED. The third row has a NEW tag (group 0x78, literal) when `FUN_002919e8`
  answers non-zero and the record has the third row: a recipe learned (record +0x10 bit) and not seen (+0x20), else a
  made entry (+0x18) not seen, over 0x2d bits.
`itemshop.2ld`'s sequence 2 (257 groups) holds the groups: 0x77 (last frame 9) and 0x78 (last frame 0x77) are one 40 x
12 rectangle ending at the origin (quad x -39..1, y 6..18) over (0, 128)-(40, 140) of the file's second image, which is the
atlas with the NEW! art (the first image is the frame panel). The row plate group 0x62 has the extent 0x10e (the camp's
tags sit at the plate extent: `FUN_00138ed0(tag, FUN_00237618(row + 0x18), ..)`).

### 24.3 The collection rewards [V]

`FUN_00284c10` runs `FUN_00291140` after the hand-over (so the totals are the new ones). It clears the page's list (state
+0x184, 8 bytes per entry: the reward index and a byte "reached"; +0x60 the count) and walks the 50 `cond` rows:
`FUN_00291418(a, b, c, count)` with the row's bytes +5, +6, +7 and halfword +8 decides "reached"; a row that is not
reached is listed only when story flag 0x4837 is set (`FUN_002783f8`). For a reached row whose bit in the record's
reward word (+8) is clear: the bit is set, the session's reward mark (`DAT_0036d1a0` +0x18) too, and
- kind (+4) 0: the item (+2) is added with `FUN_001a1a18` when `FUN_001a18b8(item, 100, 1)` allows (count below 99; a
  unique item not yet owned), notice bit 2 (value 2 of the state's +0x130);
- kind 1, reward 0: unlock bit 0 (record +0x28), notice 4, then `FUN_00290cc0` learns the Free Development recipes of set
  1; reward 1: unlock bit 1, notice 8, the same; any other reward n: unlock bit n, notice 0x10, the record bit of
  halfword +0xa (`FUN_001a52b0`) and the first material of the row's category and rank is marked in the state's +0x134
  (`FUN_00291628`; the marks' reader is the notice page, 24.9).
The notices are the opening's texts 1..4 of the table at 0x382628.
The test `FUN_00291418` walks the 50 material slots (state +0x314: the item ids of type 0x10 in table order) with the
record's totals (+0xfc, the amount of each ever handed over): the slot counts when `FUN_00291560(item +4, a)` and
`FUN_002915f8(item +5, b)` hold: a in 0xd, 0xe any category; below 0xd the category itself; 0x10 one of three special
categories (the bytes at 0x382698: 9, 10, 11 on the disc); any other value one that is not special. b 4 and 5 any
rank; below 4 the rank itself. A matching slot with a total of 0 fails the whole test when a is 0xd or b is 4 (the
"collect all" rows); otherwise the totals add (at most 99,999) and the matching slots with a total are counted; c 0
compares the sum with the count, else the number of slots. The disc's 50 rows read as their texts say (test
`the_disc_shop_layout_has_the_tag_groups_and_the_rewards_read_as_conditions` prints them): 0..10 "Get n types of
Materials" (a 0xe, b 5, c 1; n = 1, 5 .. 45, all 50 with a 0xd, b 4), 11..15 "Obtain total amount of n" (c 0), 16..21 "all
materials of a category" (a 6..0xb, b 4, count 4), 22..25 "all of a rank" (a 0xd, b 0..3, count 12 or 13), 26..49
"n or more of one category and rank" (c 0; reward kind 1 with the record bits 0x49..0x60, the gated groups of the moogle's
shop, section 22.2). Rewards 0, 2, 3 and so on are items (24 of the 50), 26 are unlocks.

### 24.4 Page 0xb, the reward list [V]

Row 5 of page 9 leads to page 0xb (`FUN_00285388`, `FUN_0028a6f0`). `FUN_00285e10` builds it (the list widget
`FUN_00296b48`, 8 visible rows from state +0x60 entries; `FUN_0028aa40` -> `FUN_0028b470` the row widgets,
`FUN_0028bc70(0xbce1)` the title, message 0xbce1) and `FUN_0028a1b8` fills the rows (the same filler serves page 10's
twin): each listed reward shows its `cond` text (+0), a plate group 0x61 when reached (0x24 when not), the text style
of the table at 0x382608 (reached / not), and a session NEW tag (group 0x78 when the page's byte +0x112d is set, else
0x77; `DAT_003825ac` / `DAT_003825b0`) when the reward is among the ones paid at this opening. An empty list shows the text
0xc194. A reached reward's detail text is the item for kind 0, else the texts 0xc1c3 (reward 0), 0xcc7d (reward 1),
0xc1c4 (the rest); the exact form of the second column, reached and not (24.9). The handler `FUN_00286060`: Up and Down move; a confirm only plays the
refused sound (`FUN_00291b58` answers 0 for page 0xb); cancel (-2) goes to page 9 (the cursor on row 5);
**-5 goes to page 10** (state +0 = 4) and **-5 on page 10 comes back** (`FUN_00285b30`, state +0 = 5): the two pages are
tabs of each other; -4 ends the menu. The help text of the page is 0xbce0 (`FUN_00288f00`).

### 24.5 The synthesis marks [V]

`DAT_0036d1a0` (0x28 bytes, cleared by `FUN_00284c10`) holds the visit's marks: +0 the entries (bit 2r, 2r + 1) learned or
made and not seen (`FUN_002916d8` at the end of the opening, over 0x2d bits) plus the ones learned during the visit
(`FUN_002889e0`, `FUN_00290cc0`); +8 the materials handed over for the first time (`FUN_002887c8`); +0x18 the rewards
paid. The state's byte +0x112d is set by the opening and cleared by the handlers that finish leaving page 9
(`FUN_00285388`), page 10 (`FUN_00285b30`) and the Recipes list, page 0x10 (`FUN_00286580`); the stores of that byte are
only in these three (`mips_scan --store-off 0x112d`), so leaving page 0xb, 0x17 or the other lists keeps it.
- Page 9 (`FUN_00284ea8`): rows 3 and 4 have a tag while +8 has a bit, row 5 while +0x18 has one, NEW while +0x112d is
  set and VIEWED after; row 1 (Free Development sets) and row 2 (Recipes) when the session's entries hold one of their
  entries (`FUN_002917c8`): NEW while one of them is unseen (`FUN_00291880`, record +0x20), else VIEWED; row 0 (Creations)
  a VIEWED tag when `FUN_00291960` finds a made entry among the session's.
- The lists (`FUN_00288fc0`): the cursor's entry is marked seen in the record (+0x20) every frame; an entry of the
  session's marks has the NEW tag while its seen bit is clear, VIEWED after.

### 24.6 What the port does (WP-SH3)

`menu_shop::synth::pay_rewards` / `cond_met` (the rewards), `Session` (page 0xb as `Page::Rewards`, `Marks`, `see`,
`entry_mark`, `row_mark`, `has_new`), `menu_shop::{item_place, fresh_at_open, item_tag, buy_tag, first_visit_bit}` and
`Page::First` (page 7) in `crates/kh2-app/src/menu_shop.rs`; `Progress` keeps the save bytes (`shop_seen` +0x3714,
`shop_visit` +0x3724, `Moogle::rewards` +8; all three are in the slot files since WP-PRG1). The tags are the sprite groups 0x77 / 0x78 on
the file's NEW! art; their place (left of the hand) and the page 0xb rows' columns are the port's [inferred: the rows have
no plate]. `X` is -5.
Harness (a state file; `KH2_STATE` is a path, `local/sh3/state-moogle.json` is
`{"munny": 500, "items": {"317": 2, "329": 2, "333": 2, "345": 6, "349": 1}}`; keys `shop_seen`, `shop_visits` and
`moogle.rewards` set the new bytes):
- Page 7 and the tags at Wantz's: `KH2_PROGRAMS=0,0,10 KH2_NPC_WATCH=2 KH2_NPC_WATCH_DIST=60 KH2_REACTION_PRESS=8
  KH2_KEYS=10:J,11:J kh2-game tt06` (log `shop: first visit of this class (save +0x3724 bit 0 clear): page 7, text 0xc401`,
  `shop: page 7 dismissed: save +0x3724 bit 0 set`, `shop: 2 NEW items`, `shop: item place 4 seen (save +0x3714 bit set,
  FUN_00265348)`, `shop: tags Buy row Some(New), ...`).
- The rewards and page 0xb at the moogle: `KH2_PROGRAMS=0,0,16 KH2_NPC_WATCH=5 KH2_NPC_WATCH_DIST=60
  KH2_REACTION_PRESS=8 KH2_STATE=local/sh3/state-moogle.json KH2_KEYS=10:J,11:J,12:J,13:J,14.5:X kh2-game tt07` (the
  third row opens with three notices, page 10, then -5 to page 0xb; log `synth: open the third row ... notices 0x7`,
  `synth: page 0xa -> page 0xb`).

### 24.7 Decompile index (WP-SH3)

| address | what |
|---|---|
| 0x25fcd8, 0x265498, 0x262140, 0x262350, 0x2621a0, 0x262450, 0x2624a0, 0x262540, 0x262558 | page 7 |
| 0x265280, 0x265ad0, 0x265348, 0x2653d8, 0x265420, 0x262bd0, 0x2626e8, 0x260a78, 0x264668, 0x260ec8, 0x264e80 | the shop's NEW marks |
| 0x291140, 0x291418, 0x291560, 0x2915f8, 0x291628, 0x2783f8, 0x1a18b8, 0x1a1768, 0x1a52b0, 0x290cc0 | the rewards |
| 0x286060, 0x285e10, 0x28b470, 0x28aa40, 0x28a1b8, 0x28ba50, 0x285b30, 0x285388, 0x288f00, 0x288da8 | page 0xb and the neighbours |
| 0x2916d8, 0x2917c8, 0x291880, 0x291960, 0x2919e8, 0x284ea8, 0x288fc0, 0x2889e0, 0x2887c8, 0x28f490 | the synthesis marks |
| data 0x381128, 0x381238, 0x382698, 0x3825ac, 0x382608, 0x382628 | first-visit texts, tag groups, special categories, row mark groups, row styles, notices |

Gaps: the window origin of the list pages and the exact place of the tags on the shop pages (the widgets `FUN_00296b48`,
`FUN_0029b600`, `FUN_00299878`: see 24.9); the look of the message window of page 7 (the port's text in the synthesis
window style); the difference between the VIEWED group 0x77 and the NEW group 0x78 beyond the animation length (both use
the same art rectangle); the count-up animation of page 10 and the rank and category tabs it highlights (24.9);
`+0x3714`, `+0x3724` and the record `+8` are in the slot files since WP-PRG1 (a reload does not pay the rewards again).

### 24.8 The command menu's Genie "Drive!" (WP-FX12b)

The in-play command menu (`items.rs`, not the pause menu) lists "Drive!" (command 605) as the third entry of the second deck while Genie is
out: `FUN_001bc6e0` tests the summon state `DAT_01d5e368 == 2` (`FUN_001c6918`) and the NeoStatus 0xc9 of the summon object (`FUN_001c6940`), calls
`FUN_001bcf10` and puts 0x25d where Party sits. `FUN_001bcf10` rebuilds list 3 (the manager's +0x180) from `FUN_001c6a20` (forms 1..4 whose item
count, `FUN_001a1cc8`, is above 0) and the command table at 0x37c036 (`FUN_001c6b10`; the disc has 535..538). The commands' `Execute` is their id, so
the generic executor `FUN_00177c20` -> `FUN_0016b420` raises event 11 on Genie's script (summons.md); the app issues `SummonRequest::Form`. Key path in the
app: H swaps the deck (the other deck's cursor kept), T / G move, J confirms, B goes back. Tests `second_deck_shows_drive_with_genie_and_issues_the_form`,
`genie_drive_rows_and_table_come_from_the_disc`. Gap: the entry's target-record parameter (`FUN_0018f490(+0xa20, Genie, 0)`).

- **MN15 review notes.** [inferred] World 0xf's limits test uses "best count above 0" (`FUN_002a20e0`); the original's `FUN_002ba238` tests the completion bits at `DAT_0036d4f4 + 0x74`, and their equivalence is not shown. `Counted` is now removed when an enemy's HP is above 0 again, so a revive counts each defeat (WP-HK1, 20.11a); `count_defeats` has a system-level test through `count_generic`. Section 14's "0x19..0x1f" for Customize is a different use of the numbers than the journal pages 0x19, 0x1a and 0x20.
- **MN15 review notes.** [inferred] World 0xf's limits test uses "best count above 0" (`FUN_002a20e0`); the original's `FUN_002ba238` tests the completion bits at `DAT_0036d4f4 + 0x74`, and their equivalence is not shown. The once-only `Counted` marker counts an enemy that revives only on its first defeat; the original counts each defeat. `count_defeats` itself has no system-level test (only `count_one`). Section 14's "0x19..0x1f" for Customize is a different use of the numbers than the journal pages 0x19, 0x1a and 0x20.

### 24.9 Shop gaps closed: reward texts, notice marks, material tags, story flag 0x4837, list geometry (WP-SH4)

**The second column of a page 0xb row [V]** (`FUN_0028a1b8`; the text lands in the row's second text widget,
`FUN_0028b470`: state `+0x52a + 0x40 k` is its string). Reached (the "reached" byte of the list entry, `FUN_00291140`):
- an item reward: `FUN_001a1f40(item, icon)` builds the command 9 (an icon) followed by the item's name (item `+8`).
  The icon is the material's category icon (`0x382660[item +4]`, 0x26..0x32 on the disc) for an item of type 0x10, else
  `FUN_00255368(type, 0)`: the first icon of the triple table at 0x3809d0 (0xb for a type not in it).
- another reward: the message 0xc1c3 (set 0), 0xcc7d (set 1) or 0xc1c4 with its variable (command 0xd) replaced by the text
  of 0xc1c5 (`FUN_00191a30`; on the disc 0xc1c5 is one icon).

Not reached (listed only with story flag 0x4837, below; the entry's byte is 0): the text of message 0xc1c5 for an
unlock, or the item's kind for an item reward (`0x382640[item +4]` for a material, `FUN_002553c0(type)` = the pairs at
0x380a60 otherwise; on the disc every kind text is one icon), then `FUN_0029bb48(message)`: the string at 0x3828b8 (`?`)
once for each character of the message (`FUN_00191660`: glyphs and icons up to a line end; spaces are not counted; 5 in
other languages), so an unreached reward shows its icon and as many question marks as the reward has letters. The row
plate group is 0x61 reached, 0x24 not (`DAT_003825a8`, `DAT_003825a0`), the text style the table at 0x382608
(`FUN_0028f468`). Port: `synth::reward_text` (`RewardText::{Item, Hidden, Filled}`) with `reward_tokens`, `masked`, `fill`
and `shown_chars` in `menu_shop.rs`; test `a_reward_row_shows_the_item_behind_its_icon_or_its_kind_and_question_marks`
and the disc test `the_disc_reward_texts_notices_and_list_extents`, which prints every kind text and asserts that each of
the 50 rewards has a text.

**The state's marks +0x134 and +0x13c [V]** (`FUN_00291140` writes, `FUN_002879e8` reads). Both are bit arrays that
only name what a notice is about. An item reward sets the bit of its reward index in `+0x13c` (notice 2); an unlock
reward other than sets 0 and 1 sets, in `+0x134`, the bit of the place of the first material whose category (item `+4`)
and rank (item `+5`) equal the row's bytes +5 and +6 (`FUN_00291628`, `FUN_002910b0`; notice 0x10). The notice page
(`FUN_002879e8`, `FUN_00287950`) shows the lowest set bit of `+0x130` and, for notice 1 and notice 4, the lowest bit of
`+0x13c` / `+0x134` as well: it calls `FUN_0028e980(message, kind text, name)` with the item's kind text (the same as
above) and the item's name (`+8`), clears that one bit, and clears the notice bit only when the array is empty. So every
reward item and every marked material has its own notice. The texts 0xbed0 ("Obtained {}{}") and 0xc193 ("{}{} can now be
purchased at shops") each hold two variables, filled in order by `FUN_00191a30`. Port: `Rewards::materials` (the `+0x134`
bits, by place), `Rewards::items` (`+0x13c`), `Session::queue` and `queue_item` (one entry per bit), `text_item`,
`first_material`; test `the_item_and_unlock_notices_name_the_reward_item_and_the_material_the_reward_marks`; the
screenshot of the first notice reads "Obtained", the item's icon and "Elixir" (the name in yellow), "for completing the
collection.".

**NEW tags of pages 10 and 0x17 [V]** (`FUN_0028f9a8`, both pages): a material handed over for the first time at this
opening (session `+8`, `FUN_002887c8`) has the tag group 0x78 (NEW) while the page byte `+0x112d` is set, else 0x77
(VIEWED); the byte falls when page 10 is left (24.5). The row also has a plate whose group comes from the table at 0x382600
by rank, the category label (`0x382640`) and the number (page 10 the state's `+0x68`, the displayed total that counts
up to the record's `+0xfc`, `FUN_0028f490`; page 0x17 the stock). Port: `synth::material_mark`; test
`a_material_handed_over_for_the_first_time_carries_a_tag_on_the_material_pages`; the screenshot of page 10 shows five
materials each with a NEW tag. Not ported: the count-up and the session marks `+0x10`, `+0x20`, `+0x24` it writes
(`FUN_0028f490`, `FUN_0028f698`: a highlight of the rank and category tabs while a total counts up; the port's page shows
the final totals at once, like the make's count-down) and the grid of rank and category tabs (`FUN_0028aaa8`,
`FUN_0028f850`, `FUN_0028fea0`): the port has no tabs, so there is nothing to mark.

**Story flag 0x4837 (`FUN_002783f8`) [V]**: the app reads it from the story flag store (`Store::story`, `Session::all`,
read when the third row opens). The baseline leaves flags clear, so by default none of this shows: the lists are as
before. With it set:
- page 0xb lists the rewards not reached too (already ported, 24.3), now with the hidden texts above;
- page 10 lists every material, those never handed over with their name as question marks and a 0 (`FUN_0028f9a8`,
  count `FUN_0028ffc8(1)`); page 0x17 is unchanged;
- the lists of page 9's rows 0, 1 and 2 (`FUN_00290058`) hold entries with the flag 0x80 (`synth::HIDDEN`, `entries_all`):
  Creations every entry not made (the higher rank's, odd, only of a recipe that has one), Free Development a recipe of
  set 1..3 and Recipes a recipe item (kind 0) that is not learned and not made. `FUN_00288fc0` shows such an entry as its
  item's name in question marks, dimmed (`FUN_0028f468(1)`), with no number and no tag, and never marks it seen; a confirm
  is refused (`FUN_00291b58` finds no recipe for it).
Not ported: the category tabs of `FUN_0028aaa8`, which also test the flag (the port has no tab grid). Test
`story_flag_4837_lists_what_the_player_cannot_have_yet_and_never_opens_it`; the screenshot of page 0xb with the flag
(`KH2_STATE` with `"story_flags": [18487]`) shows nine rows, the unreached ones dim with an icon and question marks, and
the screenshot of page 10 shows the materials never handed over as question marks with a 0.

**List geometry [V in part]**. The row builder of page 0xb (`FUN_0028b470`; `FUN_0028aa40` calls it for the pages other than 10, and `FUN_0028b188` for page 10) places row k's plate widget (group 0x62)
at `(0, k * +0x1c)` of the window widget, and its second text and the NEW tag at the group's x (`FUN_00138ed0(widget, +0x10,
+0x14)`) from it, both texts at the size the group holds in +0x18 (0xf when 0). On the disc (read by
`the_disc_reward_texts_notices_and_list_extents`, which asserts them) group 0x62 has x 270, y 0, size 13, +0x1c 23: so the
rows are 23 apart (the port used 24), the second text starts 270 to the right of the first and the NEW tag ends there
(quad x -39..1), and the text is 13 pixels tall. Port: `reward_metrics` reads them from the file and page 0xb uses them.
Page 10's rows are rebuilt by `FUN_0028f9a8` from group 0x60 and page 0x17's by the same function from group 0x6e; group 0x60 has (x 177, size 15, +0x1c 23, +0x20 228: two
columns of nine, page 10) and group 0x6e (x 175, size 16, +0x1c 26, +0x20 28), so page 10 shares page 0xb's pitch 23 and
page 0x17 has 26; the port applies these pitches (`group_pitch`) to its single column and not the grid's second column. Not found: the window widget's own origin on the screen
(`FUN_0029b600(window, 0x7537, ..., 0x67, 0x68, 0)` with groups whose records hold x 460, y 123; `FUN_0029ba00` parents it;
no layout layer carries a position, all are at 0,0; the widget classes `FUN_00296aa8`, `FUN_00299e38`, `FUN_00299878` set
the rest at run time), so the port keeps its own first row (70, 120), the title and the hand. The shop pages 1 and 4 keep
the port's pitch and columns for the same reason (their rows come from the generic list widget, not a row builder with
readable offsets).

Decompile index (WP-SH4):

| address | what |
|---|---|
| 0x28a1b8, 0x29bb48, 0x191660, 0x191820, 0x191870, 0x191a30, 0x1a1f40, 0x1a1ee0, 0x2553c0, 0x255368 | the second column of a reward row |
| 0x291140, 0x291628, 0x2910b0, 0x2879e8, 0x287950, 0x28e980 | the marks +0x134 / +0x13c and the notices |
| 0x28f9a8, 0x28ffc8, 0x28f490, 0x28f698, 0x28fea0, 0x28f468, 0x2887c8 | pages 10 and 0x17 |
| 0x290058, 0x288fc0, 0x28aaa8, 0x2783f8 | story flag 0x4837 |
| 0x28b470, 0x285e10, 0x237618, 0x237638, 0x237658, 0x237678, 0x237698, 0x138ed0, 0x29b6c0, 0x29ba00 | the row geometry |
| data 0x382640, 0x382660, 0x380a60, 0x3809d0, 0x3828b8, 0x382628, 0x382600 | kind texts, icons, type tables, the mask string, notices, plate groups |

Gaps (WP-SH4): the window origin of the list pages (above); `FUN_0028e980`'s window class (0x7546) and its colour and
placement, which the port draws as a text window; the second text's clip width (`FUN_00296d10` sets +6..+0xc of the list);
`FUN_00191a30`'s handling of a text with more variables than texts (the port drops the extra variable).

### 24.10 Shop leftovers: findings of WP-SH5 (research only, nothing ported)

**Window origin [V from code, data not read]**: a child widget's screen position is `FUN_0029b060`: parent `+0x164/+0x168`, plus the parent layout's anchor offset `+0xe0/+0xe4` (`FUN_002375c0`, computed by `FUN_00236cf8` from the first animation of the current group with flag bit 2, at frame `+0xc8`: translate `[4..7]` plus pivot times scale), plus the parent's `+0x16c/+0x170` (`FUN_00138ed0`), plus the parent group's record x, y (`FUN_00237618`, `FUN_00237638`), then its own `+0x16c/+0x170`. The root window (`FUN_0029b600`, group 0x67 in `FUN_0028aaa8`) has no parent and `+0x16c/+0x170` 0, so its rows start at its anchor animation (flag bit 2, not named in `kh2_image::seqd::flag`) plus group 0x67's x, y. Earlier "layers all at 0,0" looked at the LAYD layers, not at this animation. Not done: reading those anims from itemshop.2ld and replacing the port's first row (70, 120). Next step: dump the anims of groups 0x67, 0x62, 0x24 with flag 2 and use their translate at the group's last frame.

**Count-up [V from code, not ported]**: `FUN_00285b30` (page 10's task) calls `FUN_0028f490(any pad bit held)` every frame while byte `+0x112d` is set. The displayed totals (state `+0x68`, copied from record `+0xfc` at the start of `FUN_00284c10`, before `FUN_002887c8` adds the hand-over) step by +1 per frame toward the record's totals (`FUN_0028f490(0)`), or jump at once while any button is held. Each change rebuilds the tab marks (`FUN_0028f698`: `+0x10` rank sums capped 99999, `+0x20` category sums capped 9999, `+0x54` kinds, `+0x58` total), the tab texts (`FUN_0028f850`), tab plates (`FUN_0028fea0`) and rows (`FUN_0028f9a8`). The list membership uses the record totals, the number shown the displayed ones. Port plan: `Session.shown` and `Opening.before`, one step per menu tick on page 10 while `marks.fresh`.

**Tab grid [partly read, not ported]**: `FUN_0028aaa8` builds 4 rank tabs (group 99, column, pitch from group +0x1c) and 13 category tabs (group 100.., 4 per row, x pitch group +0x20, y pitch +0x1c), number labels (`+0x3d2 + 8k`), and the words 0xbbc7 / 0xbbc8 at group 0x65 with the kind and total counts. Positions come from those group records; drawing needs the sprite-group path of `put_tag`.

**Not applicable**: `FUN_00296d10(list, a, b, c, d)` stores pad button masks (0x10, 0x20, 0x40, 0x80 = up, right, down, left) in the list's `+6..+0xc`, not a clip width: the "second text's clip width" gap does not exist. `FUN_00191a30` with more variables than texts reads the next stack argument slots (undefined values); the port leaves such variables empty.

### 24.11 Shop leftovers: ported by WP-SH6 (window origin, count-up, tab grid)

**Flag-2 animations (dumped to `local/sh6-anims.txt`, never the repo).** Sequence 2 of `itemshop.2ld`. Group 0x67 (the header window, records x 460, y 123): its first animation with flag bit 2 is group 28, frames 0..15, translate (24, 60), pivot x 200 to 0 (the slide-in), so the anchor is (224, 60) at frame 0 and (24, 60) at the last frame. The art animations of the same group sit at (484, 183) and (484, 153), (256, 212): those are art, not the anchor. Group 0x62 (the Material Log row plate) anchor (19, 102); group 0x60 (page 10's rows) (38, 102); group 0x6e (page 0x17's rows) (40, 7); group 99 (rank tabs) (-1, 2); group 100 (category tabs) (115, 2); group 0x65 (the kinds and total words) (215, 71); the empty-list texts' groups 0x66 and 0x6f (232, 188) and (232, 156). Group 0x24 has one animation at frames 999..1000 without the flag, so no anchor.

**Origin rule as ported.** A widget parented to the header window starts at the window's anchor at its last frame plus its own group's anchor (`menu_shop::plate_origin`). The group record's x, y of the window (460, 123) is not added although the decompile of `FUN_0029b060` adds the parent group's `FUN_00237618`/`FUN_00237638`: with it the rows would start at x 484 on a 512 wide screen, while the empty-list texts (anchors 232) land on x 256, the middle of the screen, only without it. [inferred] The addition probably reads a different field for this root; not matched to a record offset. Resulting first rows: page 10 (62, 162), page 0xb (43, 162), page 0x17 (64, 67); the rows go down by the plate group's `+0x1c` (23, 23, 26) and the second text sits at the group's x (177, 270, 175). The port's hand, tag and number positions follow the same shift [inferred]. Other pages (buy and sell lists, synthesis pages 9, 0xc, 0xd) keep the port's own first row: their widgets were not followed.

**Fixed on the way.** `ShopMenu` kept the shop sequence's group records in `layd.seqs`, which `load` empties after moving the sequences into the scene, so `reward_metrics` and `group_pitch` always fell back to the port's numbers. They now read a copy (`ShopMenu::seqd`); page 0xb's rows and the material pages use the file's pitch (23, 23, 26) and text size.

**Count-up (`FUN_0028f490`, run by page 10's task `FUN_00285b30` once a frame while `+0x112d` is set).** `Opening::before` holds the record's totals from before the hand-over; `Session::shown` starts there and `Session::count_up` raises every slot below the record by one per frame, or sets all to the record's at once while any key is held (`ShopMenu::held`, the skip flag `FUN_00299020(-1)`); a slot that differs first sets the session marks `+0x10` (row), `+0x20` bit 3 - rank (rank tab) and `+0x24` bit category (category tab); a displayed value above the record is left as it is. The list membership and the "?" masking use the record's totals, the number shown the displayed ones. The tick system (`menu_shop::count_up`) runs it per 60 Hz tick on page 10 and redraws.

**Tab grid (`FUN_0028aaa8`, numbers `FUN_0028f850`, sums `FUN_0028f698`, plates `FUN_0028fea0`).** Drawn on pages 10 and 0xb as text: four rank tabs (name `0x382638[3 - k]`, number the sum of rank 3 - k) at the window anchor plus the group's anchor, k times its `+0x1c` below, the number a child at the group record's x, y (100, 0); thirteen category tabs (name `0x382640[n]`) at `(n % 4)` times `+0x20` (90) and `(n / 4)` times `+0x1c` (23), the number at (75, 0). Sums (`synth::tab_sums`): rank sums at most 99,999, category sums at most 9,999, kinds, total at most 99,999. A category tab shows when the record's sum is above 0 or story flag 0x4837 is up. A marked tab is drawn bright [inferred: the original swaps the plate sprite group for `FUN_0028f468(0)`; the port draws no plates]. Harness screenshots (`local/sh6acc/moogle-rewards-all.png`, `p10-15.png`) show the S/A/B/C rows with their sums and the category numbers in the grid over the list.

**Gaps (WP-SH6).** The kinds and total words of group 0x65 (`0xbbc7`, `0xbbc8`; anchor (215, 71), children at x 160 and 240) are not drawn: alignment and clip unknown, the second would start at x 479. The row plate highlight (session `+0x10`) is computed, not drawn (no plates). The rows' two-column grid of page 10 (group `+0x20` 228) is still one column. Numbers are left aligned at the second text's x [inferred; the widget class `0x7539` was not read]. The frame art of group 0x67 itself (the window) is not drawn by the port; the layouts carry no layer for it (the widget draws it). Searched: `FUN_0029b060`, `FUN_00236cf8`, `FUN_0029b600`, `FUN_0028aaa8`, `FUN_0028f490`, `FUN_0028f698`, `FUN_0028f850`, `FUN_0028fea0`, `FUN_0028f9a8`, `FUN_0028b470`, `FUN_00285b30`, `FUN_00285e10`.

Tests: `the_count_up_steps_one_a_frame_or_jumps_and_marks_the_tabs`, `the_tab_sums_are_capped_per_rank_category_and_total`, `the_disc_window_origin_row_origins_and_tab_places` (disc), `the_disc_reward_texts_notices_and_list_extents` (now reads the copy).

### 24.12 Shop leftovers: group 0x65, page 10's two columns, row plates (WP-SH7)

**Group 0x65 [V code, I alignment].** `FUN_0028aaa8` builds, on pages 10 and 0xb, the heading 0xbbc6 at the group's anchor (window anchor (24, 60) plus (215, 71) = (239, 131)) and two children on that line at the group's `+0x20` (160) and x (240): the number of kinds (`+0x54`) and the total (`+0x58`) as text with the words 0xbbc7 and 0xbbc8 appended (`FUN_0028f850`, `FUN_001918a0`, `FUN_002554f0`), from the displayed totals. The port draws them (`draw_tabs`), right aligned [inferred from the widget flag `+0x1a4` = 1; left aligned the total would start at x 479]. Screenshot `local/p10.png` looked at: the heading, "Types" and "Total" words and the numbers 5 and 13 appear under the rank tabs, but I could not tell from the picture which number belongs to which word, so the exact order of number and word is unverified.

**Page 10's two columns [V].** `FUN_0028b188`: 18 rows of group 0x60, row i at `(i % 2) * +0x20` (228), `(i / 2) * +0x1c` (23). `FUN_002858e8` gives the list widget `(count + 1) / 2` rows with nine shown, so the cursor takes a pair; the port does this (`Session::key`, test `page_10_cursor_takes_a_pair_of_materials_and_the_stock_page_one`; disc test `the_disc_kinds_and_total_words_and_page_10_columns`). The NEW tag ends at the number's x in each cell, and a number the count-up marked (session `+0x10`) is drawn bright (`FUN_0028f9a8`, `FUN_0028f468(0)`). Screenshot looked at: two columns of nine rows, NEW tags left of the numbers of Blazing, Lucid and Power Shard.

**Marks +0x134 / +0x13c and the NEW tags of pages 10 and 0x17**: already ported in WP-SH4 (24.9); nothing more found.

**Row plates [superseded by 24.13].** The row plates are drawn; the gap text here was wrong (WP-SH8).

### 24.13 Kinds and total line proven, plates drawn (WP-SH8)

**Number before word [V code, V data].** `FUN_0028f850` and `FUN_0028aaa8` build each child text with `FUN_001918a0(buffer, "%d", value)` (the format at 0x382518 is `%d`), then `FUN_00191710` for its length and `FUN_002554f0(buffer + length, message)`, so the number comes first and the word is appended after it: kinds child (`+0x54`) is the number then message 0xbbc7, the total child (`+0x58`) the number then 0xbbc8. Disc data (test `the_disc_kinds_and_total_words_text`): 0xbbc6 is "Types" (the heading at the group's anchor), 0xbbc7 is one spacing glyph (glyph 83) and "Total", 0xbbc8 is empty. The line therefore reads "Types 5 Total 13": the word "Total" is the tail of the kinds child, and the total child is the bare number. The port's `number_then_word` does exactly this (harness screenshot `moogle-rewards-all` looked at: "0 Types  5 Total  13" on one line, the 0 being the last category tab's number).

**Alignment [V code].** The widget's `+0x1a4` (set to 1 at `0xf804` / `0xf9ac` of the menu object by `FUN_0028aaa8`) is the fifth argument of `FUN_001a7ed8` from the widget draw `FUN_00139048`; the text object keeps it at `+0x58` and `FUN_001a80d8` calls `FUN_001a8018(mode, ..)`, a jump table at 0x37b5e4. Mode 1 (0x1a803c) is `x = x - width`: right aligned at the child's x. So the kinds child ends at 239 + 160 = 399 and the total child at 239 + 240 = 479 (a position within the 512 wide screen). In the screenshot the right edges are at x 398 and 477 in game units (screen 868 and 995 px on the 1.607 scale with 228 px side margin). This replaces the `[inferred]` mark of 24.12.

**Row plates and tab plates [V code, V data].** The plate widgets (class tags 0x7538, 0x7539) are sprite-group instances: `FUN_00138fc8` hands its class argument to `FUN_00139430`, which ignores it (the tag only names the task `FUN_001394e8` registers); the group id decides what draws. Groups 0x60, 0x62, 0x6e (row plates), 0x63 (rank tab), 0x64 (category tab) and 0x65 (the line under the heading) hold real art in the shop's sequence: translucent green (0x1c, 0x7a, 0x00, alpha 0x40) or grey (group 0x6e) bands with gradient ends, and for 0x65 a black and a yellow 2 pixel line from x 201 to 456 (test `the_disc_plate_groups_hold_art`). Row plates were already placed (`put_plate`) and show in the harness screenshot as faint bands behind each row (visible when the picture's brightness is raised; they are alpha 0x40 of 0x80). This WP adds the tab plates: rank plate k from group 0x63 at the window anchor plus `(0, k * +0x1c)`, category plate n from group 0x64 at `((n % 4) * +0x20, (n / 4) * +0x1c)`, and group 0x65 at the window anchor, all drawn on pages 10 and 0xb (`Nodes::plates` entries after the rows'). A category plate exists for all 13 tabs, shown or not (`FUN_0028aaa8` builds it before it tests the count).

**The 0x7539 children [V data].** `FUN_0028f468(k)` reads the u16 table at 0x382608: 0xbb, 0xb7, 0xbc, 0xba, 0xb9, 0xb8, 0xb6 for k = 0..6. The plates' 0x7539 child widgets use these groups (0xb6 by default, 0xbb for a tab the count-up marked, `FUN_0028fea0`). Groups 0xb6 to 0xbc hold only a 1 by 1 pixel marker quad with alpha 0, so they draw nothing; the choice among them can only matter through the colour id the widget keeps (`FUN_00138e78` sets `+0x178`, then `FUN_002368e8`). [gap] What that colour id changes in the plate's colours was not read (searched `FUN_00138e78`, `FUN_002368e8`, `FUN_00139048`, `FUN_00237518`); the port keeps the art's own colours and the text colour it already derives from the mark. The table at 0x382600 by rank is not a plate table: its first four entries (0x5e, 0x5d, 0x5c, 0x5f) and the words after are message ids of the tab names, already ported.

Decompile index (WP-SH8): `FUN_0028aaa8`, `FUN_0028f850`, `FUN_0028fea0`, `FUN_0028f468`, `FUN_001918a0`, `FUN_00237618`, `FUN_00237698`, `FUN_00138fc8`, `FUN_00139430`, `FUN_00139048`, `FUN_001a7ed8`, `FUN_001a80d8`, `FUN_001a8018` (table 0x37b5e4).

