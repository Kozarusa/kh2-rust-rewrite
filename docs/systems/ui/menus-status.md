# Pause menu: Status page

[Contract](contract.md) | [Index](../../README.md)

## 10. Status as built (WP-M2)

Code: `menu_status.rs` (the number rules, the tables read from the executable, the two pages), `kh2-image/tests/menu_status.rs` (disc tests
that print the tables and the groups), hooks in `menu.rs` (the plugin, the Status row opens page 0x17, a sub-page's keys, `page0_nodes`),
`harness.rs` (keys `LB`, `RB`).

### 10.1 The pages and their input [V]

- **Tabs** (`FUN_002683c8` fills the u16 at `+0x18` of the page object): bit 0 the leader (when the party count `FUN_00282940` is not 0), bits 1..4
  the four forms (`FUN_00282b80`: the form's item is held and `FUN_00282b20` is false; that function tests the save byte at `+0xc` of
  the save block against a -1-terminated list at 0x3823a8 = {10, 11} (the byte is the world being entered, `FUN_00104bd0` stores it at 0x33e86c = save header +0xc; worlds 10 Pride Lands and 11 Atlantica report no forms, so those tabs vanish there; `menu_status::forms_blocked`, WP-ST2; the app takes the world from `AreaRecord::world` [inferred same value]), bit 5 the summons (`FUN_00268338`: any of the four items at
  0x381760 is held), bits 6.. the other party members. The list shows the set bits in ascending order (`FUN_00268828` list index -> tab, `FUN_00268880`
  the reverse).
- **Page 0x17** (`FUN_00265c48`, handler `FUN_00266000`): the tab rows use the group sets at 0x380860 (leader and members: idle 0x85, 0x87, 0x86; selected
  0x85, 0x87, 0x84, the same as page 0's list) and 0x380870 (forms and summons: 0x81, 0x83, 0x82 / 0x80); the label is the member's name
  (message `0x37ba90[member id]`, `FUN_001b5890`), a form's name (`0x381a38 + 2..`) or message 0xc5b1; the help line is the message of table 0x3817d0
  ("{}'s Status." with the member's name for the leader and the members, a sentence per form and for the summons). Cancel (-2): back to page 0
  (the tab cursor is remembered with `FUN_00252f98(0x17, tab)`); -4: close the menu; a confirm: page 0x18.
- **Page 0x18** (`FUN_00266328`, handler `FUN_00266760`): two list widgets (`DAT_0036ce9c`, `DAT_0036cea0`), one per column. Up/Down move in the column
  (the list widget of 1.7); Left/Right (-12, -13) `FUN_00268a08` toggle the byte `+0x1c` of the page object (the active column) and put the cursor on
  the other column at the same row, limited to its row count; -10 and -11 `FUN_00268c78` step the tab (-10 the previous, -11 the next, wrap at both
  ends; the page redraws for the new tab); -2 back to 0x17; -4 closes. A confirm has no case (nothing happens).
- The rows of the active column's cursor show their help message (`FUN_00267de0`: the table row's second u16).

### 10.2 Rows and number sources [V]

The row tables are at 0x3817e8 (leader and members), 0x381860 (forms), 0x3818d8 (summons): per table two blocks of 0x3c bytes (the columns), ten rows
of 6 bytes (name message, help message, row type; type 0xffff ends the block). `menu_status` reads them at run time; the disc test
`status_tables_have_the_documented_shape` prints every row with its text. A shown row is a bit `column * 10 + row` of the word at `+0x14` of the page object,
set per tab by the jump table 0x381768 (`FUN_00268568`): the leader and the members' case 0x2685a0 (bits 0..5 and 10..16; the leader alone also
bit 6 when form 1 is available, bit 7 always, bit 17 when a summon item is held, bits 8, 18, 9, 19 for forms 1, 2, 3, 4), forms 0x2686f8 (bits 0..3, 10..16),
summons 0x268758 (bits 0..3 and bit 10 + j for each summon j held). Shown rows are packed: a missing bit leaves no gap. The values are text
formatted by `FUN_001918a0` (a `sprintf` with the formats at 0x3816b8 `%d`, 0x3816c0 `%d/%d`, 0x3816c8 `%d%%`) into 0x20 byte buffers at `+0x520` of the page
object, filled by `FUN_002673c0` (jump table 0x3816d0; the leader's case is 0x267444, read from the instructions because the decompilation lacks it).

| row | label (message) | value | source |
|---|---|---|---|
| 0 | LV (0xc559) | `%d` | the member record `+0xf` (`Progress::sora().level`) |
| 1 | Experience (0xc55a) | `%d` | the word `0x340e48` (`Progress::experience`) |
| 2 | Next LV (0xc55b) | `%d` | `FUN_001878b0`: the Exp column of the `lvup` row for the current level (`FUN_001b50c0` clamps the level to the table length) minus the total; 0 at level 99 |
| 3 | HP (0xc55c) | `%d/%d` | status `+0` (the record's HP held at the maximum) and `+4` |
| 4 | MP (0xc55d) | `%d/%d` | status `+0x180` and `+0x184` |
| 5 | AP (0xc55e) | `%d/%d` | `FUN_002688d8`: the sum of the **equipped abilities' costs** (bit 15 of the record's ability slots; cost = the high byte of the item's `StatEntry`, item row `+7`) over status `+0x18e` = the `lvup` AP of the level + the record's AP byte `+8` + the `item_stat` AP of the equipment (`FUN_0019f920`, `FUN_001a0c08`) |
| 6 | Drive Gauge (0xc55f) | `%d/%d` | status `+0x1b1`, `+0x1b2` (the saved gauge bytes, party block `+0xdd1`, `+0xdd2`); shown with form 1 |
| 7 | Bonus LV (0xc567) | `%d` | the byte at party block `+0xfa8` (0x340e68) |
| 8, 9, 18, 19 | Valor LV, Master LV, Wisdom LV, Final LV (0xc5aa, 0xc5ac, 0xc5ab, 0xc5ad) | `%d` | the form record `+2` (`FUN_00187f60`) of forms 1, 3, 2, 4 |
| 10, 11, 12 | Strength, Magic, Defense (0xc560..2) | `%d` | status `+0x188`, `+0x18a`, `+0x18c` = the `lvup` row of the level + the record's bytes `+9`, `+0xa`, `+0xb` + the equipment's `item_stat` Attack, Magic, Defense |
| 13..16 | Fire, Blizzard, Thunder, Dark Resistance (0xc563..6) | `%d%%` | `100 - status[+0x1a9..+0x1ac]`; the seven bytes start at 100 and every equipped item multiplies each by its `item_stat` byte (+8 .. +0xe: physical, fire, ice, thunder, dark, light, general) `/ 100` rounded up (`FUN_001a0c08`) |
| 17 | Summon LV (0xc568) | `%d` | the byte at party block `+0xdce` (`Progress::summon_level`) |

"The equipment" is the record's two weapon words, the armor and accessory slots and, in a form, the form record's weapon (`FUN_0019f920`); the battle
item slots are not part of it. The forms' and summons' tabs print: the form level (record `+2`), its experience (`FUN_001d3c98`: the record's experience plus the
table entries below the level, the table from `FUN_001d3b90`), the experience to the next level (`FUN_001d3c38`: the table entry at the level minus the record's
experience) and the gauge row `level + 2`; the summons' tab the same from the summon level (`+0xdce`) and experience (`0x340e4c`, `FUN_001d3d48`,
`FUN_001d3cf8`). The second column of a form tab is the leader's strength, magic, defense and resistances. The party members' tabs print the leader's rows 0..5 and 10..16
from the member's own status object.

Checked on the disc (`the_leaders_numbers_follow_lvup_the_record_and_the_abilities_resource`): the new game with the app baseline gives LV 1, Next LV 40 (the Exp of `lvup` row 0), HP 20/20, MP 100/100,
AP 2/2 (Guard costs the 2 that `lvup` row 0 gives), Strength 5 (row 2 + Kingdom Key 3), Magic 7 (6 + 1), Defense 2, every resistance 0 %, Drive Gauge 3/3, and the same strength, magic, defense,
used AP and AP total as the `Abilities` resource computes (`equipment_stats`, `ap_used`, `ap_max` + the record's AP byte, which is 0 here).

**The AP rule** that 2.3 lists as a gap is therefore known for the display: the number the Abilities screen compares is the same `FUN_002688d8` sum over
`FUN_0019f920`'s total; the check that refuses an over-cost equip was still not found in the handlers read. `Abilities::ap_max()` (`abilities.rs`) leaves out the record's own AP byte `+8`.

### 10.3 What the port draws, and what is inferred

- Ported: the leader's tab (both pages, all rows of the table above with their conditions), the tab cursor, the column and row cursors with the wrap rules, the shoulder
  switch (`cycle`, a no-op with one tab), the help lines (the row's help message; "{}'s Status." with the name), the sounds (cursor, confirm, cancel).
- The forms' and summons' tabs: 16.5. The party members' tabs (Donald, Goofy): 19.1 (WP-MN7).
- **[I] Layout.** No footage shows Status (clips 5, 6, 7 and 10 do not). The groups come from `camp.2ld` sequence 1: a row plate of the wide rows (groups 0x143..0x147, one per row
  type) is 200 wide and 20 high above its origin with a row pitch of 24 (`group.unk1c`) and a column pitch of 232 (`group.unk20`), the value group of the page 0x17 preview
  (0x89, 0x8b, 0x8f, 0x90, 0x96) 164 wide with a pitch of 170; the value text is placed at the group's own x (180, resp. 160) and right aligned (the alignment is [I]). Where the list
  objects start (`FUN_00138ed0` on the lists, `FUN_0029b600`), the colours and the fonts of the labels and values were not read: they follow page 0's rows. Page 0x17 shows the tab list where
  page 0's list is and the preview plates at the right; page 0x18 the wide rows across the screen; page 0's own rows, cards and panel are hidden while the Status pages show (what the
  original does with them was not read). Munny and Time stay.
- **[I]** the exp-to-next number is held at 0 where the experience total passes the row (the original never shows that state: every gain runs the level-up check `FUN_001b52f8`;
  the app has no level-up yet).
- Forms are "available" by `Progress::owns_form` and not in worlds 10 and 11 (`FUN_00282b20`, WP-ST2; test `no_form_tabs_in_the_worlds_that_block_forms`). [GAP] The summons' extra rows (bit 10 + j) print nothing in `FUN_002673c0`'s case (16.5); still unread. WP-ST2 re-checked: forms (16.5), summons (16.5) and members (19.1) tabs, mask and L1/R1 cycling were already ported by WP-M7/MN7.

### 10.4 Harness

`KH2_KEYS` knows `LB` and `RB` (`[` and `]`, the shoulder buttons) besides Enter, Escape, Backspace, J (confirm), Up, Down, Left, Right. Log lines: `menu: page 0x17`, `menu: page 0x18`,
`menu: status tab N`, `menu: status row C R`, `menu: status column C row R`, `menu: page 0`. Example: `KH2_KEYS=2:Enter,3:Down,3.1:Down,3.2:Down,3.3:Down,3.6:J` then a screenshot opens Status (Abilities
is the second row with the baseline's Guard, so Status is the fifth).

