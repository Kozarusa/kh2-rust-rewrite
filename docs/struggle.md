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
  loses (the player's knock-out, `FUN_001cec00`, section "The player's knock-out"). An empty counter (0xe) decides nothing. Win: host 4.42 tallies wins (`FUN_001d7560`, held at 99), at 10
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

**Gaps** (after WP-7E2). Story flags, record bits and tallies are kept for the session only (no save data). The
global frame counter and the generator 0x3716a0 are the pool's own (the game shares them). Sora's guard repel is
taken as the whole guard state facing the attacker.

## The start and the end (WP-7E2, verified unless marked)

**The header** (BAR type 2): +2 the id, +4 the flags, +6 a message id for `FUN_001b9a08` (0 here), +8 the mission
kind (2 in all three Struggle files), +9 and +0xc (0x13: `FUN_001d3688` / `FUN_001d36b8` / `FUN_001b9ae0` at the
start, not ported), +0xd an unlock byte for `FUN_001be6b8` (0), +0x10 / +0x14 / +0x18 the sounds of the start, the
end and the failure (0xbcd, 0xbce, 0xbcf; interface sounds, `FUN_001b9ec8` -> `FUN_002c3318`, played as
`PlaySe::number`). Test `the_struggle_layouts_and_sounds_come_from_the_header`.

**Layout records** (opcodes 1, 2, 3, 0xd; `FUN_001b9378` -> `FUN_001d3490`): byte +4 is the BAR index of an event
of the mission file (`re_m`, type 0x16), +5 the 1-based index of a picture (IMGD; the SEQD follows,
`FUN_001d3390`), +6 an object the event binds (0 here). `FUN_001d3538` plays the record as an event (a 0xb0-byte
event object, `FUN_00195898`, kind = the manager's default from `FUN_001b8b10`: record 1 0xc, 2 and 0xd 0xe, 3 0xf;
record 1 with a picture becomes 0xd) whose picture pair goes to event +0x94 / +0x98. The Struggle's records: 1 =
event 2 with picture 4 (`st_t`: "STRUGGLE!"), 2 = event 5 with picture 7 (`ed_t`), 3 = event 8 with picture 10
(`ed_t`); no record 0xd. The `re_m` event: opcode 8 sets the end frame (500, `FUN_002245d0` case 8 ->
`FUN_00226fb0`), opcode 0x50 at frame 11 starts a 2D object that plays one group of the event's picture at draw
priority 26000 (`FUN_00225af0` -> `FUN_00225998`; the group is `DAT_0036bd30` 1..5, set from event +0x9c by
`FUN_002232c8`, else 1). The event frame is the game frame count times 0.5 (`FUN_00222728`, 0x37ec50); the event
ends when its frame reaches the end frame (`FUN_00222668`): 500 event frames are 1000 game frames, **or earlier,
when its picture has played through** (WP-G1, verified chain): the 2D object (vtable 0x36bed8; update method
`FUN_00225aa8` -> the sequence step `FUN_00138aa8`, which at the end of a non-looping group with no follow-up calls the
destroy method, vtable +0x14) has a destructor `FUN_00225a50` that, when the object's byte +0x350 is 1 (set by
`FUN_00225998`'s third argument, which `FUN_00225af0` passes as 1), starts a thread of `FUN_00225a20`; that calls
`FUN_00223658(0)`, which, while an event plays (`FUN_00196418`, `DAT_0035e3a4`), ends it (`FUN_002235c0`). The group
plays once (`loops` 1) from event frame 11 (tick 22) for its length: `st_t` group 1 is 90 ticks (last frame 89),
`ed_t` group 1 ("YOU WIN!") 300, group 2 ("YOU LOSE") 270, so the start event ends at tick 112, the win at 322, the
loss at 292, not at the end frame [inferred: the destroy thread's own start takes a tick or two]. Port:
`Layout::group_ticks`, `Play::picture_end`, `Play::ended`; tests
`the_start_event_ends_when_its_picture_group_has_played_through`, `an_event_without_a_finishing_picture_ends_at_its_end_frame`.
The picture goes with its object: `hud_mission::picture_act` removes the node when its group has finished (it used to
hold the last frame, so "STRUGGLE!" stayed on screen until the event's end frame), test
`a_layout_picture_goes_when_it_has_played_through_or_its_event_is_over`.
[inferred] the event object's own startup and fade states (`FUN_00195c08`) take no time in the port.

**Event kinds** (`03system.bin` entry `evtp`, read by `FUN_001d1128`; byte 0 the kind, +2 flags): flag bit 3 clear
-> players 0 and 1 lose control for the event (`FUN_00195ae0` -> `FUN_00196280` -> `FUN_001b7ce8`; `FUN_001b7d50`
reads it; `FUN_00196170` -> `FUN_001962a0` gives it back); bit 2 clear -> the field HUD's draw tags 3 to 6 are hidden
(`FUN_00196760` -> `FUN_0014ef48` -> `FUN_0013da78`; back at the end through `FUN_00196790`). Every event also
pauses the clock with bit 4 (`FUN_0014c748(4)`, cleared by `FUN_001959f0`). Kind 0xd (the start) has 0x60: holds
the player and hides the field HUD; 0xe (the end) 0x44 and 0xf (the failure) 0x64: hold the player, the field HUD
stays.

**The start** (`FUN_001b8ee8`'s tail): sound +0x10, record 1 plays at once. `FUN_001b9140` state 0 waits while an
event plays and the player cannot act (`FUN_001794f0` -> `FUN_00176a18` -> `FUN_001b7d50`), then shows the
elements (`FUN_001d5f00`; the clock starts with its record's show method) and goes to state 1. So the counters and
the clock appear when "STRUGGLE!" ends, 112 ticks (about 1.9 s) after the mission starts (it was 1000 before WP-G1: the
event waited for its end frame); control returns then and the match clock runs from there. Before WP-G1 the log showed
the layout start at 13:11:41.33 and `layout start ended at event frame 500` 16.6 s later (the whole time the player was
held). Test
`the_start_layout_holds_the_hud_until_it_ends`. [inferred] The show is skipped when manager +4 bit 1 is set; its
writer was not found.

**The end.** Host 4.3 (`FUN_001b9780`) ends with event 7 (0x9c only for mission kind 0x1e with a set story flag on
the trigger of (7, argument)); host 4.16 (`FUN_001b97f0`) with 0x10. Both go through `FUN_001b95a0`: header +0xd
(none), the end sound (+0x14), the stop `FUN_001b9940` (manager +4 bits 4 and 0x10, clock stop, bit 2 of
`DAT_0035d268`, `FUN_001d4bf8`, pause-menu records: clear 8, apply 5, `FUN_001adf78`), then record 2: with event
0x10 or 0x9c, or a trigger for the event while the header lacks flag 0x40, state 2 (next frame 3: wait until the
event has 4 or fewer frames left, `FUN_00196668` against 4.0 at 0x37bbc8, then record 0xd or a `00battle.bin` event,
state 4); otherwise state 4 at once. State 4 waits for the event's end and runs the last step `FUN_001b96e0`: mission
kinds 2, 0x19, 0x20 (`FUN_001b9c00`) or event 0x9c open the menu of pause-menu record 0x1a (`FUN_0024f330`) and the
manager stays; otherwise the event goes to the area (`FUN_00197b70`): a trigger that takes it ends the manager
(`FUN_001b9518`), event 8 with no trigger ends it with the game-over path (`FUN_001b9ce0`). Host 4.11
(`FUN_001b9818`, the loss): the failure sound (+0x18), +0xb4 = 2 (record 3's select word: its picture shows group 2,
"YOU LOSE"; record 2 shows group 1, "YOU WIN!"), record 3, the stop; with a trigger for event 8 or a picture on
record 3 (`FUN_001b98a8`) the last step waits with event 8 (state 4), else the manager ends at once
(`FUN_001b9ce0`). Host 4.22 (`FUN_001b9908`): the stop, the manager ends, global event 0xf. Tests
`the_struggle_end_plays_its_layout_then_opens_the_menu`, `with_a_trigger_and_no_flag_0x40_the_end_waits_for_the_last_frames`,
`area_triggers_match_by_event_and_argument`.

In tt04 the Struggle header has flag 0x40 and kind 2: a win plays "YOU WIN!" (record 2, 322 ticks, the player held,
the field HUD and the mission HUD with the final score stay), a loss "YOU LOSE" (record 3); then the menu 0x1a would
open. The area triggers of the `btl` program and of the `evt` program with the same id (0xb6: events 0x8b and 0x8d,
jumps) do not take event 7 or 8, and the mission kind sends neither to the area anyway. So the HUD does not come
down at the end: it stays until the menu ends the manager. **Gaps:** the menu of record 0x1a (`FUN_0024f330`, the
pause-menu record table of `DAT_01d5df08`) is not in the app. Since WP-F1 (item 16, user report: the Struggle UI
lingered for ever) the last step ends the manager at once in its place (log `struggle: last step (FUN_001b96e0):
mission kind 2 opens the menu of record 0x1a (FUN_0024f330), which the app does not have; the manager ends as if
it had closed`, then `struggle: mission manager ended`): the counters, the frozen clock, the result picture, the
orbs and the player lock go 322 ticks (292 for "YOU LOSE") after the result layout started, as `FUN_001b90d0` removes them with the
manager [inferred: what the game does when that menu closes was not read]. Since WP-F1 (item 2) the player
controller reads the layout's control lock and the stop (`Mission::control_locked`, `Mission::fight_stopped`):
`sora::tick` drops every press and the stick (`Pad::lock`) while either holds, and Sora does not revive from
`State::Down` meanwhile; log (`KH2_MOVELOG=1`) `move: input ignored: the mission's layout locks control or the
fight stopped (FUN_001b7d50 / FUN_001b9940)`; test `a_locked_pad_reads_as_untouched`. `FUN_001be6b8`,
`FUN_001d4bf8`, `FUN_001adf78`, the pause-menu records 5 and 8, the start's `FUN_001d3688` / `FUN_001b9ae0`,
`FUN_001d5048` (equips item 0x180 on 0x33fec0 for missions 0x128..0x12a) and `FUN_001d5320` (the reverse at the
end) are not ported. Event skipping was not read.

What the opponents do at the end: they get global event 159 / 160 as event 10 (`gameset_voice_*` modes); host
1.34 `event_active` now answers true while a layout plays (the app's only events).

**The fight stops** (`FUN_001b9940`, bit 2 of `DAT_0035d268`): readers `FUN_00175ad8` (the player's method +0x60,
`FUN_001789d8`: busy), `FUN_00190668` (spawn groups `FUN_001989d8` and code at 0x189b64: no new spawns),
`FUN_001b70b8` (every command answers 5, refused: `FUN_001ba2c0`). In the app the counters no longer move after the
stop (`FUN_001b9560`) and the clock is stopped; the rest needs the player controller and the command menu
(outside-ownership).

## The player's knock-out (verified)

Header flag 0x100 -> bit 0x8000 of `DAT_0035d268` (`FUN_001b8d80`) has one reader, `FUN_001908b0`, called only by
`FUN_0015b798`, a draw method (vtable 0x359008, created by `FUN_0015bc10` from area opcode 0xe): the bit hides that
screen element [inferred: an area map marker]. It has nothing to do with the knock-out (the Struggle header has no
0x100 anyway). The rule is bit 0x20000 of `DAT_0035d268`, set by host 1.317 in the Struggle script's event 3: the
field player's class (vtable 0x360410, made by `FUN_001ce820`) has the knock-out method +0x5c = `FUN_001cec00`,
called when HP part 0 reaches 0 (`FUN_00186238` -> `FUN_00186530`). With the bit it sends global event 0x5a (90)
instead of the game-over paths (`FUN_001ad8e0` / `FUN_001ad810` / `FUN_001ad8a8`); the Struggle script takes event
90 as a loss (host 4.11). Sora's HP does go to 0 (his lowest HP is the status record +8, 0 unless a script sets it,
`FUN_0019f920` / `FUN_001a0030`). The same bit makes `FUN_001a0968` scale damage by the word at 0x1d5d7c8 plus 1
(the player takes it divided, others multiplied); no writer of that word was found except the clear `FUN_001b81f8`,
so the factor is 1. Port: `mission_tick` sends event 0x5a when Sora's HP falls to 0 with the bit set; test
`a_knock_out_with_host_1_317_loses_the_struggle`; log `struggle: Sora knocked out: global event 0x5a`. The app still
revives Sora after 3 s (`sora.rs`); in the original he stays down through the result [inferred from the absence of a
revive in `FUN_001cec00`].

## Sora fights alone (WP-S8, verified)

The Struggle `btl` programs (0xb6..0xb8) run opcode 0xf with preset 0 (`FUN_001a7330`) before they spawn the
group, so the slot bytes name no companion and the group's FRIEND_1 / FRIEND_2 rows create nothing
(`FUN_0016e208` -> `FUN_0017cd20`). The app applies the program's presets when the mission starts
(`party::mission_party`) and the party rebuild removes Donald and Goofy. Details and gaps: docs/party.md section 19.

## The pickup skips (verified)

`FUN_001cadd8` passes a fighter by while `FUN_00185a08` (controller +0x140 present and the last requested motion
+0x154 in 10..39: the reaction motions) or `FUN_00185a38` (140..150) holds, or while +0x168 bit 0x40 is set. Port:
`pickup_skipped` with the motion id from the body's animator (enemies: the slot; Sora: slot / 4, `Mset::slot`). The
writer of +0x168 bit 0x40 was not found; it reads clear. Test `the_pickup_passes_fighters_in_reaction_motions`.

## `player_finish_struggle` (verified)

Searched every entry of the disc for the bytes (26,464 entries: `KH2.IDX`, the world sub-indexes named by the name
list, and `OVL.IMG`, decompressed; a scratch test, not kept): they are in nine opponent files. `N_EX500_BTL`,
`N_EX570_BTL`, `N_EX600_BTL`, `N_EX610_BTL` and `N_EX610_BTL2` hold it as a send argument (host 1.90, checked in the
listings of `N_EX500_BTL` and `N_EX610_BTL`); `N_EX650_BTL10`, `N_EX670_BTL10`, `N_EX680_BTL10` and `N_EX690_BTL10`
register a mode of that name (host 1.6, beside `enemy_finish_struggle`). Messages go to the mode machines of every
object (`FUN_00187250`), so the receivers are those `_BTL10` opponents; none of them is in tt04's Struggle groups,
so in the tt04 matches the message reaches no one, as in the app.

## The target gauge over the TIME plate (inferred fix)

The game binds the target gauge only when the player's ability bit 19 (ability 138, Scan) is set
(`FUN_0019b4a8`: status +0x1cc bit 0x80000) and the target's +0x108 bit 0x4000 is clear (the opponents' scripts set
only +0x108 flags 3 and 6, host 1.70). No mission rule that hides it was found (searched `FUN_0019b4a8`,
`FUN_0019b3f0`, `FUN_00154498`, `FUN_00153f98`, `FUN_0013da78`, `FUN_0014f8b0`, `FUN_001b9378`, `FUN_001d6110`).
[inferred] `hud_mission` hides the gauge (field gauge file group 38 and its children) while a counter stands on
screen side 1, where the gauge would cover it and the TIME plate.
## HUD (verified unless marked)

See `hud_mission.rs`'s header. Counter: frame group by side (0x24, 0x25, 0x23), group 3 (the ORBS plate and the
orb ring), three digits (`FUN_0017c6e0` of 200) of 32 units each (sequence group +0x20), digits through the table at
0x3585f0, leading zeros group 0x16 (`FUN_0014ff98`). Clock: frame group 0x19 (side 2), group 3 (the TIME plate),
minutes, separator 0x17, seconds, separator 0x18, hundredths (digit width 18, separators 8; table 0x358680),
hidden from one hour on; the last seconds of a countdown start groups 0x1f, 0x1e, 0x1d, then 0x1c at 0
(`FUN_00150d40`; its sound 0x2b is not played). [inferred] The children are placed at the translation of group 3's
anchor animation (the pivots of -256 and -100 would put them off the screen); the stacking y of `FUN_0014f8b0` is
taken as 0; the target gauge of the app is hidden while a side-1 counter shows ([inferred], section "The target
gauge over the TIME plate"). Widescreen: the user's edge rule (left counter left, right counter right).
Test `counter_digits_blank_their_leading_zeros`, `the_struggle_pictures_have_the_groups_the_code_uses`.

## The battle intro (WP-I1): camera, effect, INFORMATION bar

The reference clip `3.mp4` (Roxas against Seifer in the Sandlot, prologue) is mission `TT04_MS500` (tt04 `btl`
program 0xb5, group b_45, header +6 message 0x48c1 "Defeat Seifer!"; test
`the_seifer_missions_name_their_objective_in_the_header`). The b_50 Struggle (`TT04_MS00`) has no intro camera
and no objective message: its start event (`re_m`, entry 2) holds only the picture (opcodes 0, 0xf, 0x10, 8, 0x14,
0x50), its header +6 is 0 (test `the_seifer_intro_is_the_start_event_camera_and_the_struggle_has_none`). The other
"Defeat Seifer!" missions: `TT04_MS108` (b_42, message 0x375a, start event `1b` with a camera), `TT03_MS400` (tt03
b_41, 0x48bf, the same `1a` event as MS500).

**What the clip shows** [measured, 60 fps frames at 0.05 to 0.1 s steps, `local/videoframes/3_*.png`]: 0.0 to 0.3 s
the picture fades in from black; 0.1 to 1.4 s a close view over Roxas's shoulder toward the board, 1.5 to 2.8 s
Seifer alone in the middle of the plaza (trees and a street lamp behind him), 2.9 to 3.9 s a wide shot from far
with Seifer left, Roxas right, townspeople and the arched door behind; 4.0 to 4.5 s the camera rushes back behind
Roxas while the picture smears (the previous view stays as a fading, stretched ghost over the new one; the
command deck and HP ring appear doubled at 4.3 to 4.35 s), then play with the deck and the HP ring. The
INFORMATION bar is up from the first visible frame; its text comes in left to right at 0.6 to 1.0 s, stands
still to about 3.55 s, then runs left (about 27 capture pixels a second) and is gone at 7.2 s; the bar fades out
by 7.8 s.

**The data** (mission file `msn/us/TT04_MS500.bar`): layout record 1 (word 0x02360002: event entry 2, no picture,
object 0x236 the player) plays event `re_msstart-1a` (kind 0xc, `evtp` flags 0x70: control locked, field HUD
hidden). Its commands: opcode 8 end frame 130 (4.33 s), opcode 0x13 one camera record (eight channels, 191 keys),
opcode 6 cut camera 0 from frame 0 to 130, opcode 0x46 (+6 = 0: pulled in against the map, no blockers), opcode
0x31 at frame 103 with mode 1 (the fly-back to the field camera), opcode 0x3c at frame 120 (effect 0 of the
mission's effect file, mode 0 = start). How each is read and run: docs/camera.md "The intro camera".

**In the app** (`intro.rs`): `intro::tick` follows the mission's start layout (`Mission.m.play`, `what ==
"start"`), parses its event (`Track::parse`), and every tick hands the camera the track's view at the event frame
(game ticks times 0.5, `FUN_00222728`), then the fly-back from frame 103, through `CamEvent::track`; opcode 0x3c
starts the effect (`fx::load` of the mission file's BAR type 0x12 entry, the manager's +0xc group that
`FUN_001b8ee8` loads with `FUN_001db4d0`). The control lock and the field-HUD hide are the start layout's, as
before. The INFORMATION bar is `hud_info.rs` (docs/hud.md "The INFORMATION bar"). Logs: `intro: TT04_MS500 start
event (entry 2): 1 camera record(s), cuts [(0, 0, 130)], end frame 130, fly-back from frame Some(103), ...`,
`camera: intro camera on`, `intro: frame 103: the camera flies to the field camera behind the player`, `intro:
frame 120: effect 0 of TT04_MS500's effect file started`, `camera: intro camera off: the field camera takes over`.

Harness: `KH2_WAVE_GROUP=b_45 ... kh2-game tt04` (the mission starts about 4 s in); `KH2_INTRO=1` (or a mission
name) starts `TT04_MS500`'s manager three seconds in wherever Sora stands (`intro::standalone`, sends
`MissionStart`), but on tt04 the area's default mission (`TT04_MS107`) starts about a second later and ends that intro, so
`KH2_INTRO=1` alone does not reproduce the intro's log lines there: use `KH2_WAVE_GROUP=b_45` for the intro on tt04; `KH2_INTRO_HOLD=<event frame>` holds the camera at a frame for screenshots; `KH2_INFO=<hex id>`
shows the bar with any message.

**Gaps.** (1) Where the fighters stand (WP-I2, not resolved). The data: tt04 `btl` program 0xb5 loads only group
b_45 (opcodes 0x19 flag 0x8db, 0xf, 0x15 the mission, 0x1d 3, 0 `b_45`); its `evt` program 0xb5 sets the music and
two triggers (events 7 and 0xf with argument -1: jumps) and loads the groups `e55_` and `f55_`; its `map` program only map words.
b_45 (kind 1) holds the player entity 0x236 at game (155.7, 10, 3.5) facing -90 degrees, the friends 0x237 / 0x238
at x 255.7, and Seifer (0x678, spawn kind 3) at (-194.3, 10, 3.5) facing +90: the app places them exactly so (log
`wave 1: Sora placed at the group's player entry`). With those places and the event matrix taken as the unit
matrix, the wide shot (frames 65 to 95) frames them as the clip does: Seifer left, Sora right, the arched door and
the notice post behind (`local/i1_compare.png`, row 3). The first two shots (frames 0 to 60: camera at about
(-9, 113, 164) cm looking at (1, 106, 355), then (-37, 117, 143) looking at (-76, 124, 252)) look toward +z, away
from both b_45 places, where the clip shows Roxas's back and then Seifer; a player-space reading (the keys in the
player's frame, +z his facing) frames Seifer at the first look-at (355 ahead = the 350 between the two entries)
but puts the camera in front of Roxas and misses Seifer in the second shot by about 43 degrees, and breaks the wide
shot. So neither reading frames all three shots with the b_45 places: in the original something else stands the
actors elsewhere for frames 0 to 60, or those keys use another space. Not found: the event has no actor opcodes (1,
2, 0x47); the record's object 0x236 only reaches event +0x40 (`FUN_00195a78` -> `FUN_001d1160`); no tt04 group puts
the player near (0, 0, 250). The board the clip shows behind Seifer is not in the app's tt04 at all (the app's view
past Seifer shows the three trees and the house the clip shows behind the board), so it is an object of the story
state, not map geometry [inferred]. Exact next step: run the tt04 story state of the prologue Seifer fight
(`KH2_STATE` with the story flags the program's opcode 0x19 names, 0x8db) to see which `evt` groups and entrance
the area script picks, and read Seifer's script (`N_EX570_BTL` with spawn parameter 4) for position hosts in its
start mode. Nothing was changed in the placement. WP-I3 followed those steps; still not resolved, and these are
ruled out [measured]: (a) the `evt` groups `e55_` and `f55_` (kind 6) hold one object each, 0x523 at (205.8, 0,
-1254.2) and 0x524 at (-106.0, 0, -1257.1): nothing near the fighters or the camera's +z view; (b) the story
flag: the `btl` program 0xb5 sets flag 0x8db (opcode 0x19) and its other commands are the world record (0xf), the
battle manager with `TT04_MS500` (0x15), party reset 3 (0x1d) and the spawn of `b_45`; `KH2_STATE` has no
story-flag key (docs/progress.md schema), so a story-state run is not available in the app, and nothing in the
three 0xb5 programs reads the flag to choose other groups; (c) Seifer's script `N_EX570_BTL` (`cargo run -p kh2 --
bdx N_EX570_BTL`, 3369 lines) has no position setter: its movement hosts are 1.111 `set_move` (3), 1.58
`set_dash` (2), 1.1 `set_yaw` (2), 1.4 `face_direction` (4); 1.147 `position` and 1.120 `target_position` only
read; (d) the camera record (opcode 0x13) holds its index and the eight channel pairs and keys, every byte
accounted for: no target actor. Remaining hypothesis [inferred, not checked]: the actors keep the places the
story event before the mission left them (the prologue scene at the notice board) until the battle layout takes
over; that event and its hand-over to the mission are not decoded. (2) The smear: drawn since WP-I3 as the
display-buffer feedback lens, docs/camera.md "The intro camera".

## Harness

```
$env:KH2_WAVE_GROUP='b_50'; $env:KH2_NO_SOUND='1'; $env:KH2_NO_MUSIC='1'
# won: Q at 6 s, J every 0.45 s from 6.3 s to 75 s; lost: no keys
$env:KH2_SHOT_AT='70'; $env:KH2_SCREENSHOT='<png>'; target\debug\kh2-game.exe tt04
```
`KH2_STRUGGLELOG=1` logs every orb taken. WP-7E2 runs: `KH2_SHOT_AT=7` shows "STRUGGLE!" (the mission starts about
4 s in); standing still, the opponent knocks Sora out and the log has `struggle: end knock-out`, then
`layout failure (record 3)` and "YOU LOSE" on screen (shot at 58 s); attacking (the keys above, to 82 s) ends by
time with `struggle: end time` and "YOU WIN!" (shot at 90 s); both end with `struggle: last step (FUN_001b96e0)`.

WP-G1 runs (user report: the start pictures stayed on screen and control came back far too late): same harness with
`KH2_KEYS=3:D:60 KH2_MOVELOG=1 KH2_STRUGGLELOG=1`. The start picture (`st_t` group 1, 90 ticks) shows "READY" and then
"STRUGGLE!" zooming in over it (shots at 5.3 s and 5.6 s); before the fix the last frame, "STRUGGLE!", stayed up for
16.6 s (shot at 8 s and 14 s) with the player held (log: layout start to `layout start ended at event frame 500`
16.64 s, 990 `input ignored` lines); after the fix the picture is gone and the counters and clock are up at 5.9 s,
control returned 1.85 s after the layout began (`layout start ended at event frame 56`, 106 lines). The final
countdown numerals (clock groups 0x1f, 0x1e, 0x1d, 0x1c) were looked at in shots at 78.6 to 82 s (before) and 63 to
65 s (after): each shows alone for about a second and is gone (`FUN_00150d40` plays each once and its object ends);
the app already removed them when their group finished.

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
| 0x1b8ee8, 0x1b9140, 0x1b95a0, 0x1b96e0, 0x1b9780, 0x1b97f0, 0x1b9818, 0x1b98a8, 0x1b9908, 0x1b9940, 0x1b9c00, 0x1b9ce0, 0x1b9518, 0x1b90d0, 0x1b8b10, 0x1ba100, 0x1ba140, 0x1ba150, 0x1b9ec8, 0x1d3490, 0x1d3538, 0x1d3390, 0x1d5320, 0x1d5048, 0x1d51b8, 0x1b9a08 | the start and the end (WP-7E2) |
| 0x195898, 0x195ae0, 0x1959f0, 0x195c08, 0x196170, 0x1962a0, 0x196280, 0x196418, 0x196668, 0x196760, 0x1964d0, 0x1968c8, 0x1d1118, 0x1d1128, 0x193610, 0x1b7ce8, 0x1b7d18, 0x1b7d50, 0x2232c8, 0x2245d0, 0x226fb0, 0x225af0, 0x225998, 0x222668, 0x222728, 0x2236d8, 0x224b98, 0x1bef10, 0x1bef48, 0x1befc0, 0x1beff8, 0x1bf030, 0x138758 | events, event kinds, pause-menu records |
| 0x1b8d80, 0x1908b0, 0x15b798, 0x15bc10, 0x1cec00, 0x1ce718, 0x1ce820, 0x178c88, 0x175f10, 0x186238, 0x186530, 0x1a0100, 0x19f920, 0x1a0030, 0x1a0968, 0x1b8218, 0x175ad8, 0x1789d8, 0x190668, 0x1b70b8, 0x1ba2c0 | flags of `DAT_0035d268`, the knock-out |
| 0x185a08, 0x185a38, 0x1cadd8, 0x19b4a8, 0x19b3f0, 0x154498, 0x153f98, 0x13da78, 0x14f8b0, 0x187250 | pickup skips, target gauge, messages |
| 0x1b9a08, 0x1b8ee8, 0x1d3538, 0x195898, 0x195a78, 0x1d1160, 0x195ae0, 0x2232c8, 0x11b460, 0x1db4d0 | the battle intro: the objective message, the start event and its matrix, the effect group (WP-I1) |
