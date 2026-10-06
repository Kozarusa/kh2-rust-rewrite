# The Struggle (WP-7E)

Modules: `crates/kh2-app/src/struggle.rs` (the rules), `crates/kh2-app/src/hud_mission.rs` (the counters and the
clock on screen). Everything comes from the user's disc and executable at run time: the mission files
(`msn/us/<name>.bar`), the orb effect file (`obj/PRIZE_TT.mdlx`), the orb kinds, digit tables and the hundredths
factor (read from `SLUS_210.05`). Status words: **verified** = read in the decompilation or the disassembly;
**inferred** = follows from what was read; **gap** = not found or not ported.

## Which battles are Struggle matches (verified)

The tt04 battle script (`ard/tt04.ard`, `btl`) starts a mission with area-script opcode 0x15 (`FUN_00180b48`
case 0x15 -> `FUN_001b9338(id, name)`, the manager `DAT_01d5dd20`). The programs that load the brief's groups:

| program | mission (id, file) | groups | opponent | match |
|---|---|---|---|---|
| 0xb6 | 0x128 `TT04_MS00` | b_50 + b_60 | `N_EX500_BTL` | Struggle: two orb counters 100 of 200, clock 60 s |
| 0xb7 | 0x129 `TT04_MS01` | b_52 + b_60 | `N_EX600_BTL` | Struggle (same header) |
| 0xb8 | 0x12a `TT04_MS02` | b_51 + b_60 | `N_EX570_BTL` (spawn parameter 5) | Struggle (same header) |
| 0x4e | 0xae `TT04_MS108` | b_42 | `N_EX570_BTL` (parameter 2) | not a Struggle: one opcode-6 gauge (30 of 100), no counters, no clock |
| 0x4d | 0xad `TT04_MS107` | b_41 | `N_EX570_BTL` (parameter 1) | one opcode-5 counter (start 0, maximum 3), no pictures, no clock |
| 0xb5 | 0x127 `TT04_MS500` | b_45 | `N_EX570_BTL` (parameter 4) | no HUD records |

b_60 holds one object, `PRIZE_TT` (object 0x5f4, `00objentry` type 22, subtype 8): `FUN_001c0400` case 8 makes
the Struggle orb pool (`FUN_001cac70`, `DAT_003600c0`). So the Struggle matches are b_50, b_51 and b_52; b_42 is
not one (the brief's guess). The harness forces a group with `KH2_WAVE_GROUP=b_50` (or b_51, b_52); the wave
spawner then sends `struggle::MissionStart` with the program's mission and whether it has the orb pool.

## The mission file (verified)

BAR entries: type 2 the header, type 3 the mission script (BDX), type 22 (`re_m`) layouts, then (image type 24,
sequence type 25) pairs. Header: +2 u16 id, +4 u16 flags, then from +0x1c records `u16 opcode, u16 count, words`
until opcode 0 (`FUN_001d8d18` with `FUN_001b9378`).

- Flags (`FUN_001b8d80`): 1 -> `FUN_001905a8`; 0x20, 0x40, 0x80, 0x100 set 4, 0x200, 0x400, 0x8000 of
  `DAT_0035d268`. 0x200 means no prizes and no items: `FUN_001a3cb0` makes none unless the player's +0x588 bit
  0x2000000 is set. The Struggle header has 0x0d61 (0x40 included), so the opponents' drops (host 1.92 row 122 or
  132) make nothing; the app logs `drop (host 1.92) of row N: none, the mission forbids prizes`.
- Records 1, 2, 3, 0xd (`FUN_001d3490`): result layouts (manager +0x4c, +0x68, +0xa0, +0x84). 4 to 9
  (`FUN_001d6110`): HUD elements; word 1..3 to the element's init method (vtable +0x14), +0x10 the 1-based BAR
  index of its picture (image = index - 1, sequence = index, `FUN_001d3390`), +0x12 the screen side, +0x13 the
  slot. 4 the clock record (+0xbc), 5 a counter (+0xe8 + 0x38 * slot), 6 a gauge (+0x190, float, events 0x88 /
  0x89), 7 a counter with flash timer (+0x260), 8 the kill score (+0x2e4, experience through `FUN_001b51e0`), 9
  (+0x22c).
- The Struggle's records: counter slot 0 (100, 200, side 0, picture 13), counter slot 1 (100, 200, side 1,
  picture 15), clock (60, 60, side 2, picture 17). Test `the_struggle_missions_hold_two_orb_counters_and_a_minute`.

## Counters (verified)

Class with vtable 0x360588: +0xc flags (bit 0 exists, 2 shown, 3 hidden, 4 off), +0x10 slot, +0x14 start, +0x18
maximum (bit 0x40000000 of the init word: show "value / maximum"), +0x2c value. `FUN_001cf9e8` sets the value held
in 0..maximum; dropping to 0 from above sends global event 0xe, reaching the maximum from below 0x7b, both with the
slot and only while shown (`FUN_001cf5b0` -> `FUN_00197b70`). Script hosts: 4.5 set (`FUN_001d6598`), 4.41 add
(`FUN_001d6620` -> `FUN_001d6698`), 4.6 / 4.20 one up / down (0x19eb68 / 0x19eb88), 4.48 one decimal digit
(0x19ec58), 4.29 read (`FUN_001d6738`), 4.46 the word +0x110 (0x19ec38), 4.18 init (0x19eba8). Every change asks
`FUN_001b9560` (manager exists and +4 bit 2 clear): a stopped mission's counters do not move. Port:
`struggle::Counter`; tests `a_counter_holds_its_range_and_reports_empty_and_full`, `a_counter_digit_is_replaced_in_place`,
`counter_init_reads_the_maximum_word`.

## The clock (verified)

The match clock is the global timer at 0x356f18..0x356f30: frames (counted by `FUN_0014c7b8` while no pause bit
of 0x356f1c is set), start frame, length, shown time, state (0 stopped, 1 started, 2 running), down flag.
`FUN_0014c6b0(s)` sets a countdown of s * 60 frames, `FUN_0014c6e0(s)` counting up to s * 60, `FUN_0014c728`
starts, `FUN_0014c790` stops. The tick `FUN_0014c560` (disassembled; the decompiler loses its end): counting down,
shown = length - (frames - start); at 0 or less it stops and sends global event 0x4a with 0; counting up, shown =
elapsed (capped at 0x34bbf) and with a length it stops at it and sends 0x4a. The clock record's show method
`FUN_001cf748` calls `FUN_001cf7d8(start)`: a positive start counts down from it, otherwise up to the maximum, then
starts and clears pause bit 2. Hosts: 4.7 restart (`FUN_001d6290`), 4.13 / 4.24 pause bits (`FUN_0014c748` /
`FUN_0014c760`), 4.26 the shown frames (`FUN_001d6340` -> `FUN_0014c7a8`). `FUN_0014c7f0` splits frames into hours,
minutes, seconds and hundredths (the frame remainder times the float at 0x379168, 100/60). Port: `struggle::Clock`;
tests `the_countdown_runs_out_after_its_length`, `the_count_up_clock_ends_at_its_limit`. [inferred] the frame
count steps before the tick reads it, both once per frame.

## Orbs (verified)

- **Knocked out.** Every reaction ends in `FUN_001caeb0` (the reaction dispatcher `FUN_00184e80`, which the
  player's vtable 0x35c9a8 method +0x68 `0x179078` also reaches unless +0x7f8 bit 4 is set): with the orb pool
  present, the victim's (hit record +0x14) side is 0 for the player (+0x588 bit 0x80), 1 for any other object; the
  reaction (+0x26) through `FUN_0018b700` gives 3 orbs for classes 0, 2, 3 and 15 for 4, 5, 14 (none for class 1),
  never more than the side's counter (4.29); the counter loses them (`FUN_001d6620(-n, side)`) and `FUN_001cad50`
  launches n orbs of kind 0 (player) or 2 (opponent) at the victim's centre (`FUN_0016b7d0`), each with flag bit 4
  (its life does not run down). The launch is the prize launch `FUN_001a3b28` (docs/prizes.md). Port:
  `struggle::orbs_knocked_out`, `side_of`, `orb_kind`; test `reactions_knock_out_3_or_15_orbs`. Sora's reaction is
  worked out as `sora::receive_hits` does (`victim_outcome`), the opponents' comes with `AiHit`.
- **Kinds** (table 0x37c238, 16 bytes each as the prize kinds): kind number, radius 27.9, bounce sound 0x29,
  pickup sound 0x2a; read at start (`struggle::orb_consts`).
- **Picked up.** The pool's pickup check (vtable 0x3600d0 slot +0x58, `FUN_001cadd8`) runs where the prize pool's
  does (at rest, after the recheck): it walks the objects with +0x588 bit 2 (`FUN_00184d98`), skips those with
  `FUN_00185a08` / `FUN_00185a38` true or +0x168 bit 0x40, and the first one in reach (`FUN_001c57b8`, the prize
  reach) draws the orb in (`FUN_001c5808` mode 0) and adds 1 to its side's counter (`FUN_001d6620(1, side)`), then
  the check returns. So the opponent picks orbs up too. Port: `struggle::orb_tick`; test
  `an_orb_rests_and_goes_to_the_first_fighter_in_reach`.
- **Look.** `obj/PRIZE_TT.mdlx` is one effect file with the prize program (four shapes); the orbs are drawn as the
  prizes (`prize::kind_draws`) [inferred: the pool's draw slot was not read]. Screenshot below: red orbs.

## The mission script and the end (verified)

`FUN_001b8ee8` starts the mission's BDX (`FUN_001954f8`: a system script, event 2 now, event 3 as its thread);
every global event reaches it as event 10 with (event, argument) (`FUN_00197b70` -> `FUN_00195718` ->
`FUN_00195690`, the eight system scripts at 0x35e364). `TT04_MS00` (MS01 and MS02 differ in the record bonus
after the handler, below; the result branches were checked on all three by the test):

- Event 3: host 1.317 (bit 0x20000 of `DAT_0035d268`), then a loop that waits for its global word 0 to change:
  1 -> host 4.3 (`FUN_001b9780`: the mission ends with event 7, or 0x9c), 2 -> 4.16 (event 0x10), 3 -> 4.11
  (`FUN_001b9818`, the failure path), 4 -> 4.22 (global event 0xf).
- Event 10: event 0x7b (a counter full) with slot 0 -> the player wins, with slot 1 -> he loses; event 0x4a (the
  clock ran out) -> he wins if his counter is above the opponent's, else he loses (a tie loses); event 90 -> he
  loses (read in MS00; its sender was not traced). An empty counter (0xe) decides nothing. Win: host 4.42 tallies wins (`FUN_001d7560`, held at 99), at 10
  wins sets story flag 2281 if clear; global event 159 (host 1.41); word 0 = 1 unless the latch 0x35fec8 (4.55)
  is set, then the latch (4.54). Lose: 4.42 tallies losses, global event 160, word 0 = 3. After the handler, when
  a record bit is clear (0.89) and the win flag (global +0x10) is set, a condition sets that bit (0.69): MS00 bit
  28 for a lead of 100 orbs or more, MS01 bit 67 for an opponent with no orbs, MS02 bit 98 for 150 orbs or more.
- The opponents' event 10 (`N_EX500_BTL` 0x22, the same in `N_EX570_BTL`, `N_EX600_BTL`): 159 -> mode
  `gameset_voice_lose`, 160 -> `gameset_voice_win`.

The app runs the script itself (`struggle::MissionHost` maps 0.14, 0.15, 0.69, 0.89, 1.41, 1.317, 4.3, 4.5, 4.6,
4.11, 4.16, 4.20, 4.22, 4.26, 4.29, 4.41, 4.42, 4.48, 4.54, 4.55; any other call is logged once). Test
`the_struggle_script_decides_by_a_full_counter_or_by_time` (all three files, every branch). Log lines:
`struggle: end <win|lose|time>: player <n> orbs, opponent <m>` (the deciding event: a full counter or the clock)
and `struggle: result <win|lose> (global event 159|160)`; the enemies log `event 10 (global event 159) delivered`.

**Gaps.** The end after the result is not in the app: the result layouts (records 1 to 3, `re_m`, `st_t`, `ed_t`),
the mission end states of `FUN_001b9140` (states 2 to 4), the area event 7 / 8 the end fires, sounds 0xbcd..0xbcf
of the header. The HUD stays up and the fighters go on (`FUN_001b9940`'s other calls, `FUN_001d4bf8`,
`FUN_001befc0(8)`, `FUN_001bef48(5)`, `FUN_001adf78`, were not read). Story flags, record bits and tallies are kept
for the session only (no save data). The match starts at once: the start layout (record 1) and the event that may
play before (`FUN_001b9140` state 0 waits for no event) are not in the app. The global frame counter and the
generator 0x3716a0 are the pool's own (the game shares them). `player_finish_struggle` (host 1.90 from the
opponents' event 6): the name occurs only in the senders (nine `obj/N_EX5..6.._BTL*` files) among every named file
under `obj/`, `msn/`, `ard/`, `limit/`, `magic/`, `event/`, `libretto*` and `00*` (decompressed and searched for
the bytes), and the executable has no such string, so its receiver is unknown; the app delivers it to
every scripted enemy and none has the mode. The pickup skips (`FUN_00185a08` / `FUN_00185a38` on +0x140 / +0x154,
+0x168 bit 0x40) are not ported: any fighter in reach takes an orb. Sora's guard repel is taken as the whole guard
state facing the attacker.

## HUD (verified unless marked)

See `hud_mission.rs`'s header. Counter: frame group by side (0x24, 0x25, 0x23), group 3 (the ORBS plate and the
orb ring), three digits (`FUN_0017c6e0` of 200) of 32 units each (sequence group +0x20), digits through the table at
0x3585f0, leading zeros group 0x16 (`FUN_0014ff98`). Clock: frame group 0x19 (side 2), group 3 (the TIME plate),
minutes, separator 0x17, seconds, separator 0x18, hundredths (digit width 18, separators 8; table 0x358680),
hidden from one hour on; the last seconds of a countdown start groups 0x1f, 0x1e, 0x1d, then 0x1c at 0
(`FUN_00150d40`; its sound 0x2b is not played). [inferred] The children are placed at the translation of group 3's
anchor animation (the pivots of -256 and -100 would put them off the screen); the stacking y of `FUN_0014f8b0` is
taken as 0; the target gauge of the app (`hud_target`) still shows and overlaps the TIME plate (whether the game
hides it in a mission was not traced). Widescreen: the user's edge rule (left counter left, right counter right).
Test `counter_digits_blank_their_leading_zeros`, `the_struggle_pictures_have_the_groups_the_code_uses`.

## Harness

```
$env:KH2_WAVE_GROUP='b_50'; $env:KH2_NO_SOUND='1'; $env:KH2_NO_MUSIC='1'
# won: Q at 6 s, J every 0.45 s from 6.3 s to 75 s; lost: no keys
$env:KH2_SHOT_AT='70'; $env:KH2_SCREENSHOT='<png>'; target\debug\kh2-game.exe tt04
```
`KH2_STRUGGLELOG=1` logs every orb taken.

## Decompile index

| address | what |
|---|---|
| 0x180b48 | area-script interpreter, case 0x15 |
| 0x1b9338, 0x1b8b10, 0x1b8d80, 0x1b8ee8, 0x1b90d0, 0x1b9140, 0x1b95a0, 0x1b96e0, 0x1b9378 | manager create, load, start, destroy, update, end, header records |
| 0x1b9550, 0x1b9560, 0x1b9d60, 0x1b9780, 0x1b97f0, 0x1b9818, 0x1b98a8, 0x1b9940, 0x1b99a0, 0x1b9ce0, 0x1ba100, 0x1ba140 | manager helpers |
| 0x1d6110, 0x1d61c8, 0x1d64d0, 0x1d6ac0, 0x1d6f00, 0x1d72c8, 0x1d3390, 0x1d3490, 0x1d3538 | HUD element records |
| 0x1cf508, 0x1cf528, 0x1cf578, 0x1cf588, 0x1cf598, 0x1cf5b0, 0x1cf640, 0x1cf700, 0x1cf748, 0x1cf798, 0x1cf7d8, 0x1cf838, 0x1cf870, 0x1cf8a0, 0x1cf8d8, 0x1cf970, 0x1cf9e8, 0x1cfa78, 0x1cfab0, 0x1cfb60..0x1cfd10, 0x1cfc28, 0x1cfe28..0x1d0038, 0x1d2d50..0x1d3010 | element classes (vtables 0x360560, 0x360588, 0x3605a8, 0x3605e8, 0x360750, 0x360858) |
| 0x1d6290, 0x1d6340, 0x1d6598, 0x1d6620, 0x1d6698, 0x1d6738, 0x1d67a0..0x1d6a58, 0x1d7598, 0x1d7560, 0x19e910..0x19ed28 | group-4 hosts (table read from 0x35bce8) |
| 0x14c560, 0x14c650..0x14c7f0 | the global clock |
| 0x14f540, 0x14fa70, 0x14f8b0, 0x14f868, 0x14fed0, 0x14ff98, 0x150a30, 0x150a70, 0x150858, 0x150d40, 0x237550, 0x237698, 0x17c6e0 | counter and clock HUD |
| 0x184e80, 0x179078, 0x18b700, 0x1caeb0, 0x1cad50, 0x1cadd8, 0x1cac70, 0x1c0400, 0x184d98, 0x185a08, 0x185a38 | orbs |
| 0x1a3cb0, 0x1c5b10, 0x1c5bf8, 0x1d54d8, 0x1d55d8, 0x1ce6c0 | the drop and the other two mini-game pools (subtypes 4 and 0xd, not in tt04) |
| 0x197b70, 0x195718, 0x195690, 0x1954f8, 0x16b470, 0x1727b0, 0x173b70, 0x16e088, 0x16e0b8, 0x1a5328 | global events and the script's other hosts |
