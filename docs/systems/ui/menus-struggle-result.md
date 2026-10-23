# Record table and the Struggle's result menu

[Contract](contract.md) | [Index](../../README.md)

## 21. The pause-menu record table and the Struggle's result menu (WP-STRM)

### 21.1 The table [V]

`DAT_01d5df08` (the "pause-menu records" of `FUN_001bef10` / `FUN_001bef48` / `FUN_001befc0`) points at entry `slct`
(tag at 0x37c558) of `14mission.bar`, set by `FUN_001d3408` through `FUN_001beed0`. The entry: +4 a u32 row count
(23 on the disc), rows of 0x40 bytes from +8, sorted by the u16 key at +0; `FUN_001bef10(key)` is the binary search
`FUN_0030ff40` (compare `0x333250`, no `.c`; the sorted keys fit a u16 compare [inferred]). A row is the request
block of menu kind 7 (section 18.1): +2 option count, +3 the first cursor row (`FUN_002544b8` -> `FUN_0026f480`),
+4 + 4i (u16 action code, u16 message) for up to four options, +0x14 kind 7's header group (`FUN_00254800`, -1 none),
+0x16 kind 8's header group (`FUN_0026f8a8(0, ..)`), +0x18 the information message, +0x25 bit 0: the list's flags
0x60 are cleared (`FUN_0026f508` -> `FUN_00298cf0`), so `FUN_00297148` never answers -2 (cancel, pad bit 8) or -4
(pad bit 0x100), and `FUN_0024f330` clears word +0xfc of `FUN_0014a918`'s object. Test
`menu_struggle::tests::the_disc_result_menu_row_texts_and_groups` checks the order and row 0x1a.

### 21.2 Row 0x1a: the Struggle's result menu [V]

Opened by the mission end's last step (`FUN_001b96e0`, docs/systems/world-minigames/struggle.md "The end"). Two rows: code 4 "Quit" (0x8ab2),
code 3 "Retry" (0x8ab1); `FUN_0026fda0` sends both to `FUN_001b9c58` (events 0x8d and 0x8b, the manager ends, the
trigger's jump). No help line is set by kind 8 for it [GAP: where the Save menu's help 0x3858 comes from was not read].

### 21.3 What the port does

`crates/kh2-menu/src/menu_struggle.rs` (`StruggleMenuPlugin`, the fourth plugin tuple of `main.rs`): reads the table at
the first request, dims the field (`DIM_BRIGHTNESS`), sets `Pause::menu`, draws the rows as the Save menu's pointer list
(`save.2ld` sequence 0 groups 77 selected, 79 idle, hand 12, placements of `menu_save.rs`) [inferred: kind 8's
resources 0x753c / 0x7532 were not resolved to a file], Up / Down move and wrap [inferred], J confirms, the cancel is
ignored for row 0x1a, the close fade is the camp's (`CLOSE_FRAMES`). Logs: `struggle menu: open record 0x1a ...`,
`struggle menu: cursor N`, `struggle menu: row N chosen, code C (FUN_0026fda0)`, `struggle menu: closed: back to the
field (FUN_001038a0)`, `struggle menu: event 0x8d: trigger jump type 1 to world 2 area 4 entrance 55`. Not drawn: kind
7's frame, the header group +0x16, the menu sounds' set-up `FUN_0023e860` (the sounds go through `menu_sound`, muted).
Code 0xc's picture page and the other rows' codes (1, 5, 6, 10, 0x11) are not reached by row 0x1a; the port takes
only codes 3 and 4 to the manager (`Mission::menu_exit`).
WP-PRG1: a record without +0x25 bit 0 can be cancelled; `FUN_0026fda0` only closes the page for the cancel (page -2 ->
`FUN_0023e8b0`) and never reaches `FUN_001b9c58`, so the original leaves the manager running. The port ends it
(`menu_exit(0)`, no jump) because with the menu gone nothing could [inferred, a port choice against a soft lock]. A code
chosen while a jump task runs is queued and applied when the task is idle [inferred]. Tests
`a_cancel_ends_the_manager_and_a_choice_during_a_jump_is_queued`.

