# Battle report

[Contract](contract.md) | [Index](../../README.md)

## 25. The battle report: menu kind 0xe (WP-JT4)

**Entry [V].** A trigger's jump record of type 5 (`FUN_0019d9d8` case 5) calls `FUN_0024f0b8(0xe, -1)`: record 0x380650 of the
table (1.2), scene 9, init `FUN_00292f18`, task `FUN_00292fc8`, page `FUN_00293020`. The init loads `ending.2ld` (`FUN_00293648`,
`FUN_002935e8`: the name `PTR_s_ending_2ld_0036d1b0`), and the task builds the page once, then loops (`FUN_00233cd8`,
`FUN_00102438`) without reading the pad. No jump follows.

**The page [V].** The backdrop is a layout of `ending.2ld` chosen by `FUN_001d77d0`: game mode (save `+0x1658`) 1 -> layout 1, mode 0
-> 0, mode 2 -> 3 when the completion percentage `FUN_002787b0` is 100, else 2. Each layout has three layers (seq 1 groups 1 and 2,
then seq 0 group 4, "The End"); a layer's `kind` (1 to 4 for the backdrops, 0) is the image of the file it draws from. The page
object has a title (message 0xcc24 "BATTLE REPORT", group 1 of sequence 0, centred, `+0x1a4 = 2`) and eight rows, each a group 2
object (pitch 40 = `unk1c` of the group, `FUN_00138ed0(row, 0, 40 * i)`) with a heading (message 0xcc18 .. 0xcc1f) and a value
object (group 3, offset 204, 20, right-aligned, `+0x108c = 1`). The values are strings built by `FUN_001918a0` (`%d`, `%d%%`,
` %d`) plus message 0xcc20 ("Times"): row 0 the mode name (0xc331 + mode: Beginner, Standard, Proud Mode); row 1 `FUN_002787b0`
% ("Completion of Jiminy's Journal"); row 2 `FUN_001de018(save + 0xacc0)` % ("Completion of Gummi Routes", the mean of 27 floats at
`+0x20` of the records `save + 0xacc0 + 0x40 * (3 * a + b - 1)`); row 3 u16 save `+0x2fdc` ("Healed Party Members"); row 4 u16 `+0x2dbe`
("Saved by Mickey", written by `FUN_001cda78` / `FUN_001cdb98`); row 5 the form with the largest u16 of the five at `+0x2fe2` ("Most
used Form": the name from 0x3827e8, entry k + 1 for counter k) and its count; row 6 u16 `+0x2fea` ("Used AntiForm"); row 7 the summon
with the largest u16 of `+0x2fec + 2 * i`, i = 0 to 3 (names 0xc594 Chicken Little, 0xc592 Genie, 0xc591 Stitch, 0xc593 Peter Pan, table 0x382800). The
running maximum starts at -1 and only a larger value replaces it, so all zeros give the first entry.

**What the port does.** `menu_ending.rs` (`EndingPlugin`, `Report`, `rows`, `layout_for`, `first_max`): `area_jump::JumpMenus.ending` (set by
`take_record` on `Outcome::OpenMenu { kind: 0xe }`) opens it; the layout, the title and the rows are drawn from the file with the
disc's texts; the field is frozen. Tests `the_layout_follows_the_mode_and_completion`, `the_most_used_entry_is_the_first_maximum`,
`the_rows_use_the_tables_of_the_executable`, `the_file_and_texts_exist`. Harness: `KH2_JUMP=5:0:0 KH2_JUMP_AT=4 KH2_SHOT_AT=8 kh2-game tt02`
logs "jump record type 5: opens menu kind 0xe ..." and "menu: kind 0xe open ... layout 1, rows [...]"; the screenshot shows the
black left side with the title, eight headings with their values on the line below, "The End" at the bottom left, and the artwork on the right.

**[inferred], gaps.** The rows' origin (8, 20), the value on the line under its heading (the value offset 204, 20 is the group's x and y;
on the heading's line the two overlap) and the colour (white) are not read from the code; no original frame of this screen was available.
Closing on a confirm, cancel or Start is the app's own (the task never reads the pad; the original leaves by a scene change from an
event). Rows 1 to 7 are 0: no counter exists in the app (`FUN_002787b0` needs the journal's completion aggregate, 20.9; the counters
at save `+0x2fdc`, `+0x2fe2`, `+0x2fea`, `+0x2fec` are written through absolute addresses, not a computed base, which is why `--store-off` missed them: see 24.4a).
Row 0 uses the app's game mode.

**24.4a Report counter writers (WP-CT1) [V by reading the functions; no event exists in the app for any of them].** Save base `0x33e860`, so `+0x2fdc` = `0x34183c`,
`+0x2dbe` = `0x34161e`, `+0x2fe2` = `0x341842`, `+0x2fea` = `0x34184a`, `+0x2fec` = `0x34184c`. Each counter is a u16 that stops at 999.
- Row 3 (`0x34183c`): `FUN_001d76a0` (also `FUN_0016da78`, a bare wrapper). Caller: `FUN_001b8658`, the heal task's end: after `FUN_001a11a0(.., member, ..)` succeeds on member 1 (Sora). Other members count nothing.
- Row 4 (`0x34161e`): `FUN_001cda78` and `FUN_001cdb98` (the init of the same object class, made by `FUN_001cdf48`) add 1 to `0x34161c` and `0x34161e`; `FUN_001ad520` reads `0x34161c`.
- Row 5 (`0x341840 + 2 * i`): `FUN_001d76c8(i)` from `FUN_001cf030` message 4, `i` = the result of `FUN_001d4180` (the drive cost is taken there). The report reads i = 1 to 5.
- Rows 6 and 7 (`0x34184a + 2 * i`): `FUN_001d76f8(i)` from `FUN_001c6168` (reached by `FUN_001cf210`, message 0x48), `i = FUN_001d4ca0(command)` = the command's 1-based slot in the object table at `DAT_01d6a9b8`
  (0 if absent). i = 1 to 4 are the summon counters of row 7; i = 0 is the word `0x34184a`, the same word as form index 5 and as the Antiform row 6, so the Antiform count is not a separate
  counter [the writer that makes it the Anti form is not separate: unresolved which index the Anti form call passes].
- Chest opening: `FUN_001a6db8` (the chest-open sequence) calls `FUN_001a6a58` twice, which calls `FUN_001a6910` (sets the chest bit, `Progress::set_chest_opened`); `FUN_001a6db8` has no direct caller (a virtual method of the chest object), and the app has no chest object, so nothing calls the port.
- The ids 0x404 / 0x405 (1028 / 1029, both inside the counter range 1000 .. 1046): `FUN_001c46a8` is not a party member's defeat method. `FUN_001c3d38` (called by `FUN_001c4208` and itself) tests the player's distance to an object (`FUN_001c3a08`) and then runs `FUN_001c46a8`,
  which adds experience (`FUN_001b51e0`, popup `FUN_0015b198`) and calls `FUN_00187e08(0x33fec0, id)` with id 0x405 if the object's `+0x5c` record id (`FUN_001c3d20`) is 0x5bd, else 0x404. So they count a pickup-like object of two kinds [which kind is which not identified]. `Progress::count_defeat(0x404 / 0x405)` already does the counting (story flag 0x8d0 gate included); the app event is the player absorbing that object (`prize.rs`, not this work package's file).
Port: `Progress::{count_healed, count_mickey_save, count_form_use, count_summon_use}` and the readers `healed_count`, `mickey_save_count`, `form_uses`, `antiform_uses`, `summon_uses`
(slot key `report_counters`).

**24.4b Hooks (WP-CT2).** `menu_ending.rs` builds rows 3 to 7 from the readers (`Report::from_progress`; rows 1 and 2 still have no source). Hooks:
- Form change, done: `drive.rs` (`drive_tick`, where the entered form is decided) calls `Progress::count_form_use(form)` with the form entered (1 Valor, 2 Wisdom, 3 Master, 4 Final, 5 Anti), the same value
  `FUN_001d4180` answers to `FUN_001cf030` message 4. Anti is index 5, the word the Antiform row (row 6) reads, so a roll that gives Anti counts there and in the fifth entry of row 5.
- Row 3 is NOT the Cure spell (correction of the earlier note). `FUN_001b8658` is the end of the item-use task (`FUN_001b84c8`, queued by `FUN_001b82b8`; docs/systems/ui/items.md): its task fields are the item (`+4`), the user's member id (`+0xc`)
  and the target (`+0x10`), and it calls `FUN_001a11a0(item, target, user, mult)`, which returns `target != user` (a group item, `FUN_001a1fe0`: any living slot other than the user). The counter is added when the user is member 1
  (Sora) and that holds, so row 3 ("Healed Party Members") counts Sora using an item on another party member. The Cure spell never reaches `FUN_001b8658`. Done (WP-IT1): `items.rs` `run` calls `Progress::count_healed()` once when a queued use finishes with Sora as the user and
  another field member as the target (`items::counts_as_healed`), the field command's target list offering Donald and Goofy (docs/systems/ui/items.md section 6).
  The pause menu's item use calls `FUN_001a11a0` directly (`FUN_0025ea40`), not `FUN_001b8658`, so it counts nothing.
- Ids 0x404 / 0x405: `prize.rs` ports the prize pool (`FUN_001a3f98`, pickups through `FUN_001a39b0`, 9 kinds at 0x37b4b0); the object of `FUN_001c3d38` / `FUN_001c46a8` is a different class (its own absorb path
  `FUN_001c45f8`, a limit of 1000 - `DAT_01d5e2b0` < 300, experience through `FUN_001b51e0`, then `FUN_001be6b8`) and neither it nor its record id 0x5bd is spawned by the app, so there is nothing to hook [object not identified]. `Progress::count_defeat(0x404 / 0x405)` stays ready.
- Summon use, for a later work package (`summon.rs`): in the `SummonRequest::Summon(c)` arm of `summon.rs`'s request loop (inside `tick`), right after `sm.index = index;` (the 1-based `sumn` row, the original's `FUN_001d4ca0(command)`), call
  `Progress::count_summon_use(index as usize)`; the app's `index` is 1 to 4, so row 7's four counters are filled and the 0 index is never used.
- Mickey's rescue: no such object in the app, skipped.

**Kind 5, the camp before a jump (`FUN_0024f300`).** `FUN_0024f300(a, b)` is `FUN_0026ef70(b)` (`DAT_0036cffc = b`) then
`FUN_0024f0b8(5, a)`; `FUN_0019d9d8` passes b = 0x1c8bda0, an address, so `FUN_0026ef80()` is never 0 and kind 5's init
`FUN_00250450` always opens page 0x15 (the "Party Members have been switched." notice, 19.4), not page 0x14. That page lists
the members that came in and went out of the party (word 4's arrangement). Ported in WP-PT1 (`party_switch.rs`, docs/systems/party/party-built.md section 21):
the arrangement is applied (`party::jump_party`) and the notice opens over the frozen field; its frame geometry is [inferred] (addresses
`FUN_0024f300`, `FUN_00250450`, `FUN_0026ef80`, `FUN_0026e618`, `FUN_0026e970`). The wait is JT4's (docs/systems/core-runtime/contract.md).


