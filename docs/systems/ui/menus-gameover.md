# Game over

[Contract](contract.md) | [Index](../../README.md)

## 26. Game over (WP-GO1)

### 26.1 The decision [V]

When the player's HP part reaches 0 (`FUN_00186238` -> `FUN_00186530`, unless the dead bit `+0x7f8 & 4` is already set) the
player class's method `+0x5c`, `FUN_001cec00` (the field player's class 0x360410), decides. In this order:

| test | result |
|---|---|
| `DAT_0035d268` bit 0x20000 (host 1.317, the Struggle script's event 3) | global event 0x5a (`FUN_00197b70`), no game over |
| `FUN_001c69b0`: summon state not 0, byte 5 = Peter Pan's row (`FUN_001d4ca0(0x132)`), byte 7 set (Tinker Bell ready) | game over type 2 (`FUN_001ad8a8`): Tinker Bell's revive (docs/systems/drive-limits-summons/contract.md), unless the form field `+0xb50` is 0xb |
| `FUN_001b9c00`: a mission manager of kind 2, 0x19 or 0x20 | type 3 (`FUN_001ad8e0(player)`) |
| `FUN_001793a8`: form field `+0xb50` in 1..5 (a drive form) or 0xb | `FUN_001b4d78` (`FUN_001b44c8`, the form's own end task), no game over |
| otherwise | type 0 (`FUN_001ad810(player)`); with the summon state 2 `FUN_001cf298` ends the summon first |

`FUN_001ced58` runs before every game-over call (`FUN_0017e220(0, 0, actor + 0x140, 0x36)`, `FUN_0016b698(actor, 0x29)`: the
fall motion). A manager that ends by itself (`FUN_001b9ce0`, the Struggle's failure with no trigger for event 8) starts type 3
for the mission kinds 2, 0x1e and 0x23 (`FUN_001ad8e0(0)`) and type 1 (`FUN_001ad868`, only while none runs) for the others.
The three constructors `FUN_001ad810`, `FUN_001ad8a8`, `FUN_001ad868` and `FUN_001ad8e0` are called only from `FUN_001cec00` and
`FUN_001b9ce0` (`mips_scan --jal`; the decomp text grep for the callers timed out, so this is the instruction scan's answer).

### 26.2 The object [V]

`FUN_001ad378(self, camera actor, type)` fills a 0x50-byte object: type at +0, step at +8, +0xc a byte ("alternate"), +0x10 the
record, +0x30 / +0x34 the list header words and the scene handle, +0x48 the callback of the list's code 2, +0x4c its task. It sets
`DAT_0035ee00` (one game over at a time), flag 0x35d268 bit 1, `0x35fec8` bit 1 (not for type 2), the music stop `FUN_00137520`
(`FUN_002c1a08`: both music slots fade, `FUN_00136a68` stream stop) and the system sound `FUN_002c14a8(0x13)` (type 1: `0x33`),
queues the camera/message object when an actor was passed (`FUN_00195898` / `FUN_00195a78` / `FUN_00195ae0`; WP-RV2: it is the event object of kind 0xf playing the first type-0x16 entry of `00battle.bin`, `re_gameover-0a`, a camera cut plus a 100-tick fade to black from event frame 0xb, with the pad locked; the actor is only stored as an unread word, docs/systems/drive-limits-summons/contract.md "The revive's message window"), sets `0x354f70`
bit 2 (`FUN_00138708`) and makes its task (`FUN_00102bc8(1, 15000, 0x1ad918)`, body `FUN_001ad960` -> `FUN_001ad9a0`).
`FUN_001ad640` (types other than 2) clears the manager (`FUN_001b9518`) when the alternate byte is 0, picks the record
(`FUN_001bef10(0x21)` for type 3, `FUN_001bef10(1)` for the others; the callback `+0x48` = `0x1adc70` for types other than 3) and
stores the list's header word at `+0x30` (`FUN_00169958(player) | 0x20000000` or `FUN_001cf2c8(0)`; the second word `+0x34` is 0,
so `FUN_0026f948` shows no information line). `FUN_001ad520` (type 0 with manager header bit 9 and no living ally, a random
chance from `DAT_003716a0`) sets the alternate byte: its list is record 10 ("I won't give up!" code 9, "It's all over..."
code 8), handled by `0x1adbb8`; reached only in the 11 missions of section 26.5 (header bit 9), which the app keeps off (WP-MK1).

The task `FUN_001ad9a0` (steps in `+8`): 1 sound gate `FUN_002c38b8`; 2 waits for `FUN_00196418` (an event / message), then for
the scene object `FUN_0014a978()+0xfc == 4`, ends the gate (`FUN_002c38c8`), puts the player on the floor (`FUN_00183fe0`, not
type 1) and (type 2) runs `FUN_001adeb8` (Tinker Bell), else `FUN_00196258` + `FUN_001c67b0` (the summon leaves) or
`FUN_001a3f68`; 4 waits for the word `0x35feb8` bit 0, `0x35fec0` bit 0 and the resource `FUN_0016ff50`, then `FUN_001ade58` loads
the scene `obj/GAMEOVER.mset` (resource 5000; `FUN_0016fac8`; motion 0x1e of the character's entry chosen by `FUN_001add40`: 0xfc
for type 0, 0xfd for types 1 and 3); 5 waits for the resource, `FUN_001adcf8` and opens the list: `FUN_0024f330(record,
object + 0x30)`; 6 waits for the list; 7 `FUN_00137558` (the music back) and `FUN_001cdf48` (the revive effect object at the
player, `FUN_001db868(.., 0x11, 1, 1, 0, player)`); 8 ends. The lengths of the waits and of the scene's motion were not read [GAP].

### 26.3 The list and its rows [V]

`14mission.bar` `slct` (section 21.1), read from the disc by `menu_gameover::tests::the_disc_rows_of_the_game_over`:

| record | rows (code: message) | header group | cancel |
|---|---|---|---|
| 1 (types 0, 1) | 2: "Continue" (0x8ab0), 1: "Load Game" (0x8aaf) | 3 | none (+0x25 bit 0) |
| 0x21 (type 3) | 4: "Quit" (0x8ab2), 3: "Retry" (0x8ab1) | -1 | none |
| 10 (alternate) | 9: "I won't give up!" (0x8ab5), 8: "It's all over..." (0x8ab6) | 2 | none |

`FUN_0026fda0` per code: 3 and 4 `FUN_001b9c58` (events 0x8b / 0x8d, the manager ends, the area trigger's jump, as the Struggle's
result menu); 1 `FUN_00254a08(2)` (the file screens, kind 2: the Load screen); 2 the default branch: the callback `+0x48`; 10 and
0x11 `FUN_00254a08(3)` / `(1)`; 5 `FUN_00222820` (`DAT_0036bcd8 = 1`, `FUN_001d3a48`; not on a game-over row); 6 closes.

"Continue" (callback `0x1adc70`, code 2) calls `FUN_00104e18(FUN_001cb3b8(), 0x80000001, 1, 1, 0)`: the jump task of the area
changes with the six bytes `DAT_01d69890` (world, area, entrance, three program bytes) and the byte `+8` = 1, so its fade-out step
(`FUN_00104d58`) runs `FUN_001cb208`: the snapshot of the save body (`0x33e860..0x349d20`, the words `0x35d278..0x35da98` and
`0x35daa0..0x35e2c0`) comes back from `0x1d5e3b0`, `0x1d69898`, `0x1d6a0c0`, then `FUN_001abfe8(8)` (party reset) and `FUN_001b81f8`
run and the jump enters the stored area record. `FUN_001047a8` (the start of every area, called by `FUN_001048e0`) takes the
snapshot (`FUN_001cafa0`: copies the same words and `DAT_01d69890 = DAT_0033e830`, the area record) when `FUN_001cb358` allows:
`FUN_001b9fb0` (a manager whose header word +4 has bit 12: allowed whatever the map program), else, with the byte
`DAT_0033e830 + 3` (the map program) below 0x33: refused when `DAT_0035e3a0` is not 0, allowed when no manager exists
(`FUN_001b9550`), refused when one does; with the program at 0x33 or above: refused. `FUN_001cb3e8(1, 0xffff)` clears the snapshot (all bytes 0xff = "keep the record's").

### 26.4 What the port does (`crates/kh2-app/src/menu_gameover.rs`, WP-GO1)

* `route` is the table of 26.1 (`KoInput` from `Mission::ko_view`, `SummonState` / `SummonData` for Tinker Bell, `DriveState` for the
  form; "in a drive form no game over" is [inferred] from `FUN_001793a8`'s test on `+0xb50` in 1..5, the form field mapping
  to `DriveState::form` was not traced): decided once when Sora's HP falls to 0 (`detect`, `FixedUpdate` between `Sim::SoraHurt` and `Sim::Sounds`, so Tinker
  Bell's byte 7 is still set the tick the revive begins). Only types 0 and 3 start the game over; event 0x5a stays in
  `struggle.rs`; the revive stays in `summon.rs`; a form's end is not changed. `Sora::game_over` holds him down (`sora.rs`'s
  in-place stand-up does not run). Without disc data the old in-place stand-up runs.
* The sound `0x13` (type 1 `0x33`) plays as `PlaySe::system` and the music stops (`audio_bgm::revive_stop`, the same `FUN_00137520`
  the revive uses); the list opens `open_after(kind)` after the fall: 122 ticks for types 0 and 3 (the knock-out event's fade, 26.8), 1.5 s for type 1 [inferred].
* The list is `menu_struggle`'s pointer list over the disc's row (record 1, or 0x21 for type 3): J / Enter confirms, W / S / arrows
  move [inferred: wrapping, as the Save menu's port does; `FUN_002967b8` was not read further], no cancel. The field is frozen
  (`Pause::menu`) while it is up.
* "Continue": the snapshot is the area record and Sora's HP and MP at the last area start that `FUN_001cb358`'s rule allows
  (`track_area`, `snapshot_allowed(map program, manager's header bit 12 or None)`: bit 12 allows it whatever the program, a
  manager without the bit forbids it, no manager needs program < 0x33; a harness area start counts; the Struggle's managers
  exist at the area start without bit 12 in the files read, so those areas take no snapshot). `DAT_0035e3a0` is taken as 0
  [inferred: nothing in the app writes it]. The jump is `JumpTask::FadeOut` with
  fade kind 1 (`0x80000001`); without a snapshot the record's own bytes stay (0xff) and the current area reloads. Sora's HP and MP are
  the maximum (WP-GO2): `FUN_001cb208` ends with the party refresher `FUN_001abfe8(8)`, which sets every member's HP and MP to the
  maximum after the body is back (section 26.7). This replaces WP-GO1b's reading that the area-start values come back, which was
  [inferred] without that function; the snapshot's HP and MP are now only logged.
  Gap (WP-LD1): `track_area` takes no snapshot when an area start reloads the same map name (it compares `CurrentArea.loaded`
  with the last name seen), while the original runs `FUN_001047a8` at every area start, so a re-entry of the same map in a
  new visit (a jump to itself) keeps the older snapshot in the port. Not changed: `menu_gameover.rs` only compares names.
  The save body IS restored (WP-GS1, next bullet). The party refresher `FUN_001abfe8(8)` is ported
  (WP-GO2, 26.7); `FUN_001b81f8` clears the Struggle's score block, which the app has no counterpart of (26.7).
* The save body (WP-GS1, `capture_body` / `restore_body` / `restore_snapshot`). `FUN_001cafa0` (read in full) copies `0x33e860..0x349d40`
  to `0x1d5e3b0`, `0x35d278..0x35daa0` to `0x1d69898` and `0x35daa0..0x35e2c8` to `0x1d6a0c0`, together with the area record
  (`DAT_01d69890 = DAT_0033e830`, `+4`, `+5`), only when `FUN_001cb358` allows; `FUN_001cb118` does the same without the test
  (and sets the record from the party record's bytes `+4..+6`, 0xff elsewhere) at the end of the Load screen. `FUN_001cb208` copies the three
  ranges back and then runs `FUN_001abfe8(8)`, `FUN_001b81f8`, `FUN_0023ea18`, `FUN_00168928` (refreshes: the first two are section 26.7, WP-GO2). The port
  holds the body as the slot file text (`Progress::slot_json`, the keys of the Save menu) in `GameOver::body`: taken in
  `track_area` with the snapshot (the NEW tags' live seen words go in as the Save menu stores them) and at the end of the
  Load screen; "Continue" applies it to a copy of the state before the jump (`apply_json`, which puts the shared story back on a failure, LD1b) and
  replaces `Progress` and the tags' seen words only on success; a body that does not apply (or none) leaves everything as it was.
  Lost on Continue, as in the original: items, key items, EXP, munny, party block, option word, shops' seen bits, the moogle record, record
  bits, journal seen set, kill/reaction/limit counters, BATTLE REPORT counters, the story flags, the area program table and
  the lock halfwords (every `Progress` field lies inside those ranges, save offsets up to `+0x38f0`). Not restored
  because outside the ranges: `DAT_0035d268` (the flag word with the KO and game-over bits), `DAT_0035e3a0`, `DAT_0033e830` (the area record,
  set by the jump), the defeated tables and the Struggle / mission state (managers are not part of the body). `dodge_roll` (the
  app's stand-in for a Final Mix growth ability) is restored with the body because the slot file carries it. Sora's HP and
  MP come from the area-start values kept in the snapshot (the app's party block does not follow the live HP, so its byte is not used:
  no double handling). Tests: `continue_restores_the_area_start_body_and_drops_later_gains`, `a_failed_restore_leaves_the_state_unchanged`.
* The scene `obj/GAMEOVER.mset` (WP-GS1 read it, WP-GO2 ported it): it is not a camera scene or a motion of the field actor but the motion of the
  model panel the list shows (the camp's 3D model object, slot 0 at the screen centre); `FUN_0016e638(.., -1, 0x1e)` only loads the
  player's own motion sets. See section 26.6 (entry `%03d0` of the BAR at index 0xfc / 0xfd, `model` in `menu_gameover.rs`).
* "Retry" / "Quit" (type 3): `Mission::menu_exit` (with the manager) or the jump records the manager carried when it ended
  (`GameOverRequest::exits`); resolved as every trigger jump. HP is full.
* "Load Game" (code 1): opens the Load screen of `menu_save.rs` (section 18.6, WP-LD1): the list waits (`Phase::Loading`, the
  field stays stopped), a read slot gives HP / MP, snapshots the loaded state (`FUN_001cb118`) and jumps to the slot's area with
  fade kind 1; leaving the screen brings the list back [inferred]. The list itself has no cancel (+0x25 bit 0).
* `route`'s simplifications [GAP]: with bit 0x20000 the original first tests `FUN_001b98a8` (a manager with a trigger for event 8 or a
  failure picture: then only the event 0x5a is sent) and otherwise sends a drive form or a form field of 0xb to `FUN_001b4d78`
  before the event; `route` and `struggle.rs` send the event whatever the form, and bit 0x20000 with no manager (the bit is
  set by a manager's script and the app clears nothing) also gives the event. The form field `+0xb50` = 0xb (a special non-drive
  mode with no counterpart in the app) is not modelled, so Tinker Bell's `+0xb50 == 0xb` exception to the revive is not either.
* Not ported: kind 7's frame and the
  header group `+0x14` (3), the camera object (`FUN_00195898`), type 1's difference (`FUN_00183fe0` skipped), the drive form's
  end (`FUN_001b4d78`). The alternate list (record 10) and Mickey's rescue (`FUN_001cdf48`) are section 26.5 (WP-MK1).
* Harness: `KH2_GAMEOVER_KO=<seconds>` knocks Sora out once (HP 0, `State::Down`). `KH2_NO_ENEMIES=1 KH2_GAMEOVER_KO=7 kh2-game
  tt00` logs `game over: Sora knocked out (FUN_001cec00): GameOver(0)` and `type 0 opens record 0x1 ... rows [(2, 8ab0), (1,
  8aaf)]`; with `KH2_KEYS=10:J` also `row chosen, code 2`, `Continue: jump to world 2 area 0 entrance 0` and `the jump is over:
  Sora is back`. `KH2_WAVE_GROUP=b_50` with the aid logs `MissionEvent` (host 1.317's bit) and no list; the summon run of
  docs/systems/drive-limits-summons/contract.md logs `TinkerRevive` and no list.
* Decompile index: `FUN_001ad378`, `001ad640`, `001ad520`, `001ad790`, `001ad810`, `001ad868`, `001ad8a8`, `001ad8e0`, `001ad960`,
  `001ad9a0`, `001adcf8`, `001add40`, `001ade58`, `001adeb8`, `001cec00`, `001ced58`, `001cf298`, `001b9ce0`, `001b9c00`,
  `001b9c58`, `001b9fb0`, `001b9aa0`, `001c69b0`, `001793a8`, `001cafa0`, `001cb118`, `001cb208`, `001cb358`, `001cb3b8`,
  `001cb3e8`, `001047a8`, `00104d58`, `00104e18`, `0026fda0`, `0026f480`, `0026f698`, `0026f948`, `00254a08`, `00254a18`,
  `0024f330`, `002c14a8`, and the raw instructions of `0x1adbb8` (the alternate list's callback) and `0x1adc70` (Continue), for
  which the decomp has no function.
* Tests: `detect_as_a_system_starts_type_0_once_and_holds_sora` and `drive_as_a_system_takes_the_manager_request_waits_and_ends_with_the_jump`
  (the ECS wiring through `run_system_once`; the second needs `KH2_ISO`), `the_knock_out_route_follows_fun_001cec00`, `the_record_and_the_manager_end_follow_the_type`,
  `death_to_list_to_choice_continue_closes_and_retry_needs_type_3`, `the_snapshot_rule_and_the_continue_destination`,
  `the_knock_out_edge_fires_once_per_fall`, `the_disc_rows_of_the_game_over`, and in `struggle.rs`
  `the_knock_out_view_carries_the_bit_the_kind_and_the_retry_and_quit_jumps`.
### 26.5 Mickey's rescue (WP-MK1: `crates/kh2-app/src/mickey.rs`, `menu_gameover.rs`)

**What Mickey is [V].** Not a summon and not a BDX script. `FUN_001cdb98` (and the identical copy `FUN_001cda78`) is the
**constructor of a player-class object**: it calls the player base constructor `FUN_00176e00(self, .., 10, ..)`, stores the class
table `0x360360` (`PTR_FUN_00360360`; the destructors `FUN_001cdcb8`, `FUN_001cdd50`, `FUN_001cdde8` store it back before
unwinding), sets object word `+0x588` bit 0x2000000, the status block
(`+0x48c`) bytes `+0x1b0 = 0`, `+0x1b1 = 0`, `+0x1b2 = 3` (a drive-gauge style value with maximum 3) and `+0x1af = 1`, hooks
`FUN_001d04c0(DAT_0037c37c, obj + 0xb74, 0xbbc2)` (a sound/effect slot, 0xbbc2) and sets word `0x35fec0` bit 4. It adds **1 to
the u16 at `0x34161c` and to the u16 at `0x34161e`**, each capped at 999 (save body `+0x2dbc` and `+0x2dbe`). An instruction
scan of loads and stores with those two offsets (`mips_scan --store-off 0x2dbc / 0x2dbe`, `--load-off`) finds only these two
constructors as writers, `FUN_001ad520` as the reader of `0x34161c` and the BATTLE REPORT `FUN_00293020` as the reader of
`0x34161e`; so the two words are always equal and the port keeps one number (`Progress::mickey_save_count`, report row 4).
The class table is the one docs/systems/drive-limits-summons/reaction-commands.md "Commands 452 to 457" describes (Mickey's `FUN_001ce240` tick offers the
reaction commands 455 to 457, his handler `FUN_001ce518` takes the Execute values 0x1c4, 0x1c7, 0x1c8 and 0x1c9, the command
menu table `0x37c450`, drive gauge maximum 3 here). The object entry is the **member 9** of the party object (`memt` u16 at
`DAT_01c95608 + 0x22`, `FUN_001ce080`; docs/systems/party/party-data.md section 1; `limt.Character` 4 = `P_EX200`), looked up with `FUN_0016e208`.

**Who makes him [V].** `FUN_001cdf48` is called from exactly one place, step 7 of the game-over task `FUN_001ad9a0`
(`mips_scan --jal 0x1cdf48`: `0x1adba8`; the decomp text grep for `FUN_001cdf48`, `FUN_001cda78`, `FUN_001cdb98` finds
`001ad9a0.c` and the constructors only). It: allocates 0xba0 bytes (`FUN_00104fa8`), takes the player (`FUN_00178cb8`) and its position with the second component lowered by 200.0 (`DAT_0037c32c`) and passed through `FUN_001a5ea8`, builds Mickey with `FUN_001cdb98(obj, memt object, player
model data, player +0xb38, position)`, then `FUN_0017f160(0, obj + 0x140, player, 0xfc)`: Sora's motion player takes **motion
0xfc from Mickey's motion table** (`FUN_0017da30` finds the id `0xfc * 4 + variant` in the object's two tables, `FUN_0017ef98`
starts it), `FUN_001db868(DAT_01c954d8 (FUN_001a21d0), 0x11, 1, 1, 0, player)`: system effect 0x11 on the player, and
`FUN_0018f188(player, Mickey)`: every object whose target `+0x9f4` was the player, and every entry of the lists around
`DAT_01c89bd0` / `DAT_01c8a5d0` that held the player's handle, now holds Mickey's.

**The decision [V].** The step is reached only through the **alternate list**:
* `FUN_001ad378` (type 0 only: `*self == 0`) asks `FUN_001b9aa0`: a mission manager exists (`DAT_01d5dd20 != 0`) and its header
  word `+4` has bit 9 (`*(u16 *)(*(manager + 8) + 4) >> 9 & 1`, port `mickey::HEADER_BIT` = 0x200 of `Header::flags`); if so it
  calls `FUN_001ad520(self)`.
* `FUN_001ad520` loops the two party slots (`FUN_0017cd08(i)`, the array at `DAT_01c893a8`): if a slot holds an actor with
  `FUN_0019a3f8` true **and** `FUN_0017d030 == 0` it returns at once (no offer). `FUN_0017d030(actor)` is 0 when `FUN_0019a3f8`
  is false, when `actor +0x160` has bit 0x4000, or when some object found by walking the actor list (`FUN_001b7c88`) has
  `+0x570 == actor` and `+0x588` bit 0x40 clear; it is 1 otherwise. Which of these a KO'd or absent friend produces was not
  traced; the port reads "ally up" as the party member that exists and is not knocked out (`PartyMember::ko`) **[inferred]**.
* Otherwise the chance is `max(sstm[0xdc], pow(sstm[0xd8], count))` (`DAT_00360130` is the `sstm` record, docs/systems/combat-effects/player-data.md; `FUN_00300448`
  is `powf` through `FUN_00301790`: it returns the constant 1.0 at `0x3852a8` for an exponent of 0) with `count` = the word
  `0x34161c` (rescues so far, over the whole save), and the shared generator `DAT_003716a0 = DAT_003716a0 * 0x10dcd` (32 bits)
  is rolled: `(float)(unsigned)seed * 2^-32` (the constant at `0x37b84c`; the negative branch at `0x1ad620` is the unsigned
  convert) below the chance sets the alternate byte `self +0xc` (instruction scan of `FUN_001ad520`, `0x1ad5d4` is the 3-operand
  `mult`). The seed is stepped only when the roll is made.
* With the byte set, `FUN_001ad640` stores the callback `LAB_001adbb8` at `+0x2c` (instead of clearing the manager) and keeps
  record 1 at `+0x10`; step 5 of `FUN_001ad9a0` waits for the player's scene (`FUN_001c7c30`) and opens **record 10** from
  `+0x14` (`FUN_001bef10(10)`), and step 4 runs `FUN_001c7740(0)` (each party member's `FUN_001c75d0(0)`) and `FUN_001cdf08`
  (`FUN_0016f8e0(5000)`, `FUN_0016e638(player model, 5000, 7)`: the GAMEOVER scene resource 5000 plays motion 7) instead of
  loading the scene with `FUN_001ade58`.
* The callback `0x1adbb8(code, ..)` (raw instructions): **code 8** ("It's all over...", message 0x8ab6): `FUN_0014a978`,
  `FUN_0014a7c8`, `FUN_0014de40`, `FUN_001ade58` (the GAMEOVER scene), `FUN_001adcf8`, then `FUN_0024f3a0` opens record 1 (the
  normal Continue / Load Game list) and the step is set to 8 (the task ends; the list belongs to the normal callback
  `0x1adc70`). **Code 9** ("I won't give up!", message 0x8ab5): `FUN_0016fb20(arg)` and the task step is set to 7: step 7 runs
  `FUN_00137558` (the music back) and `FUN_001cdf48` (Mickey), step 8 ends the task; `FUN_001ad378`'s flag `0x35d268` bit 1 is not
  cleared by anything seen, so the field resumes through the object's end [GAP: not followed].

**What the player sees.** The Sora lying down, the list "I won't give up! / It's all over...", on "I won't give up!" the music
returns, Mickey appears next to Sora with system effect 0x11, Sora plays motion 0xfc and every enemy that targeted Sora now
targets Mickey. What the effect and the motion do to Sora's HP (the heal), how long Mickey stays, what he does and how he leaves
are in his class table (`0x360360`: `FUN_001ce240` tick, `FUN_001ce458`, `FUN_001cde80` and others) and in Sora's motion 0xfc
event data; none of it was traced **[GAP: the heal amount; the leave; effect 0x11's script; motion 0xfc]**.

**What Mickey does once he is there (WP-MK1 follow-up, read from the class table 0x360360 [V], the heal itself not found).**
The rescue is an interactive reaction-command exchange, not a heal call. His tick `FUN_001ce240` (every frame, after the base
tick `FUN_00177788`) offers the player a command through `FUN_001bb3e0(.., 1, id, .., 1, 100)`: while his gauge byte
(status `+0x1b1`) is below the maximum (`+0x1b2` = 3) the charge command 455 (Execute 0x1c7), once it is full command 456
(0x1c8, "release"); the first full frame runs `FUN_001ce3f8` (turns on the object parts 10 to 0x15, `+0xb70` = 1) and starts the
slots `0xbbc3` / `0xbbc4` of `FUN_001d04c0` / `FUN_001d0508`. His command handler `FUN_001ce518` takes 0x1c7 (motion 0xc1, the charge timer
`+0xb68` from the float at `0x37c340`, end callback `LAB_001ce710`), 0x1c9 (the same timer from `0x37c344`), 0x1c4 (motion 0xc2,
queues motion 4, `+0xaf4` = the command's cost byte) and 0x1c8 (motion 0xc0, then `FUN_001ce680`: `FUN_001d0588` ends the
slot at `+0xb74`, `FUN_0019b580(+0xb40)`, `+0x10c |= 0x200000`). While charging, `+0xb6c` counts down with the frame unit
and each time it runs out `FUN_001a0290(gauge, 1)` adds one fraction step and the timer is re-armed with `sstm[0xd4]`
(docs/systems/drive-limits-summons/drive-forms.md "Mickey's charge"). `FUN_001ce458` (vtable +0x14) acts when the status word `+0` (HP) equals 1: it
looks up the ability row 0x22a (`FUN_001b6f48(0x22a)`, `FUN_0019abb0`) and starts it (`FUN_00175e20`) with the id `DAT_00360400`.
So what Mickey does for Sora is play his own motions 0xc0 to 0xc2 (their hit events and attack records live in his motion
and `atkp` data) while the player charges his gauge to 3 and releases. **No function of this class writes Sora's HP, calls
the item or magic heal (`FUN_001a11a0`) or names a Cure id**, so the amount Sora is healed to, how long Mickey stays and what
ends the task are not in the code read [GAP]: they are in motion 0xfc's and 0xc0's event data. `FUN_001ad9a0` step 7 ends at
once (step 8 returns 0), so the game-over object does not wait for Mickey.

**Where Sora's HP comes back (WP-MK2) [V code, trigger not found].** Three findings that change the picture above:
* **Mickey is the controlled player.** `FUN_001cdf48` first calls `FUN_00178cb8`, which stashes the current player (Sora) in
  `DAT_0035c99c` and clears `DAT_0035c998`; Mickey's base constructor `FUN_00176e00` then stores himself in `DAT_0035c998`.
  So while he exists the player's input, the command menu (455 charge, 456 release) and the camera belong to Mickey, and Sora
  (also kept in `DAT_00360400` by his own constructor `FUN_001ce718`) is the target of 456. His max HP is 60 and MP 100 (`plrp`
  row with `Character` 4; `kh2 table plrp`), NeoStatus 4 (`objentry` 91, `P_EX200`).
* **The heal is an item use with Mickey as the user.** `FUN_001ce458` (vtable `+0x14`, every frame, base `FUN_001778e8`) acts when
  Mickey's own HP is exactly 1: it starts command row 0x22a (`FUN_001b6f48`; `cmd` row 554 = Execute 3 (item), Argument 4, the
  Elixir of `item` row 4, message 33534) through `FUN_00175e20(.., take = 0)`, so `FUN_00175ec0` takes nothing from a stock.
  The queued item use `FUN_001b82b8` stores the user's NeoStatus 4 (`FUN_001699a8`); when its time is out `FUN_001b8658` sees user
  4 and calls **`FUN_001ce168(Mickey)`** instead of `FUN_001a11a0`. That function: Mickey's vtable `+0x20` (`FUN_001ce0a0`:
  `FUN_001973f8`, then `FUN_001772b0`, which clears `DAT_0035c998`), `FUN_00178d28` (the stashed Sora becomes the player again,
  `DAT_0035c998` = Sora, camera and input follow), `FUN_0018f188(Mickey, Sora)` (everything that targeted Mickey targets Sora),
  Sora plays **motion 0xfe** from Mickey's motion table (`FUN_0017f160(0, Mickey + 0x140, Sora, 0xfe)`), `FUN_00197230(Mickey,
  0x37c338, Sora)`, and finally Sora's vtable `+0x78` (`0x1792f0`, the same as the base class; `FUN_00186238` then `FUN_001a0100`)
  with the amount **`(int)(Sora max HP * sstm[0x54])`**. This is the answer to "how much": the share `sstm[0x54]` of the
  maximum, added to the current HP and clamped to the maximum (`mickey::heal_amount`, `mickey::HEAL_SSTM_OFFSET`, the disc's
  value is read by `the_heal_share_reads_the_disc_when_there_is_one`). So Mickey's leaving and Sora's heal are one event, and the
  control returns to Sora at that moment.
* **Motion 0xfc is not in the files.** `obj/P_EX200.mset` has 29 motions (slots up to 776) and `P_EX200_MEMO.mset` one (slot 133); neither
  has slot 1008 (0xfc * 4) or 1016 (0xfe * 4). `FUN_0017da30` / `FUN_0017db88` then return the null motion, so the two motion starts
  by `FUN_0017f160` look up an absent entry in the USA data [GAP: the second table (the second word of the first argument) of `FUN_0017db88` was not identified].

What is still not found, so the rescue stays off: (1) what makes Mickey's HP equal 1 (his max is 60; the status minimum is 0
and `FUN_00186238` clamps to it; the damage path of an enemy hit on a Mickey-class target was not traced); (2) what 456 does
besides motion 0xc0 (that motion holds range trigger 10 with the hit group 1197 and frame triggers 14 with the states 26 and 22,
none of which writes HP; `atkp` 1197 has Power 0, Type 3); (3) system effect 0x11 and the messages 0xbbc2 to 0xbbc4; (4) Mickey's
model state, the object parts 10 to 0x15 shown by `FUN_001ce3f8`, and the camera. Porting needs a second player class (control swap,
command menu, gauge) in `sora.rs` and the command code, outside this package's files.

**Missions that can offer it [V, scan].** The header word `+4` (BAR entry type 2) of every `msn/us/*.bar` was read; **11
missions have bit 9 (0x200)**: AL03_MS103, BB05_MS104A, BB05_MS104B, BB11_MS102, BB15_MS203, CA18_MS202, EH24_MS110,
HE07_MS103, HE18_MS108, MU08_MS204, MU09_MS108 (test `mickey::tests::the_disc_has_eleven_missions_with_header_bit_9`, needs
`KH2_ISO` and `local/kh2idx.txt`). In the original the offer exists there (when no friend is up and the roll passes).

**What the port does.** `mickey.rs` is the pure rule (`Chance::from_sstm`, `Chance::after`, `next_seed`, `roll`, `offered`;
tests in the module, `the_chance_reads_the_disc_when_there_is_one` reads `sstm[0xd8]` = 0.8 and `sstm[0xdc]` = 0.5 from the disc).
`menu_gameover.rs`: `detect` asks `alternate_offer` for a type 0 game over. **The offer is off in normal play**
(`mickey::RESCUE_PORTED` = false): the app has no Mickey object (a second player class with handler `FUN_001ce518`, tick
`FUN_001ce240`, command menu and gauge; `summon.rs` cannot carry it: it makes party-side *summons* from `sumn` rows and BDX
scripts, while this is a member-9 player-class object), no motion 0xfc, no effect 0x11, no retargeting `FUN_0018f188`, and
the heal is not known, so a rescue would invent Sora's HP. Missions with bit 9 therefore show the normal Continue / Load
Game list (log: `Mickey offer: header bit 9 is set ... the rescue is not ported`). The rest is in place for the day the rescue
exists: `Over::alternate` makes the Waiting phase open record 10, code 8 returns to the wait so the normal record 1 opens, code 9
closes the list and `rescue` counts the rescue (`Progress::count_mickey_save`, report row 4), plays the music back and
ends the game over with Sora up at full HP [harness placeholder, not the original's value]. `KH2_MICKEY_OFFER=1` bypasses the
gate to show that path (a generator that starts at 1 as `FUN_002f5fc8` resets it; allies from `PartyMember::ko`).
Not ported: everything listed above and `FUN_001c7740`'s effect on the friends.

**Harness.** `KH2_MICKEY_OFFER=1 KH2_NO_ENEMIES=1 KH2_GAMEOVER_KO=7 kh2-game tt00` logs `game over: Mickey offer: KH2_MICKEY_OFFER
forces the alternate list (record 10): true`, `type 0 opens record 0xa ... rows [(9, 8ab5), (8, 8ab6)]`; with `KH2_KEYS=10:J` the
`rescue` lines (`Mickey's rescue: counted (report row 4 now 1)`). Without the variable the same run logs `rows [(2, 8ab0), (1, 8aaf)]`
as before. Tests: `mickey::tests::*`, `menu_gameover::tests::the_alternate_list_routes_code_9_to_mickey_and_code_8_to_the_normal_list`,
`the_rescue_counts_once_and_releases_sora`, `the_plain_knock_out_has_no_alternate`.

**WP-MK3: why the control swap is not ported, and what was checked [V code, gate stays off].** The swap itself is one global: Mickey's base
constructor `FUN_00176e00` stores the object in `DAT_0035c998` (the current player), `FUN_001ce168` gives it back to the stashed Sora
through `FUN_00178d28`. In the port the controlled player is the `Sora` component and its systems (`sora.rs`, camera, HUD, the command
deck); a second player needs Mickey's own body (`P_EX200` model and motion set), his command deck and his tick, none of which exists,
and the motions the swap starts (0xfc on Sora, 0xfe on leaving) are absent from the USA motion sets (see above). The brief's rule
(E) applies: not ported faithfully, so `mickey::RESCUE_PORTED` stays false and normal play is unchanged. Checked and ruled out as the
writer of Mickey's HP 1: the record builder `FUN_0019f920` (HP and maximum both come from the `plrp` row of `Character` 4, 60; the
minimum `+8` is 0 and is raised to `DAT_01c954b4` only for a record with an owner block at `+0x250`, which `FUN_0019fdc0` creates
for handle types 1 and 0xe only, `FUN_001a0030` stores it back), `FUN_001792f0` / `FUN_00176c98` (a negative amount only charges the
drive gauge through `+0x1af`, no clamp) and `FUN_00186238` (clamps to the minimum, `FUN_00186530` on 0). The vtable at `0x360360`
holds only 0x00 to 0x9c; the entries from `0xb0` (`FUN_001ce928`, `001ceb88`, `001ceba8`, `001ceda8`) are the next class table [inferred: the zero words at `0xa0` to `0xac` end Mickey's],
Sora's (`FUN_001ce718`), not Mickey's. The `+0xb50` (class id) loads that were scanned all sit in player or Sora-class code, none
clamps a Mickey-class HP. Open (unchanged): the HP 1 writer, what 456 does beyond motion 0xc0, effect 0x11, the model parts 10 to
0x15, and which hit path ends a Mickey with exactly 1 HP [GAP: the enemy damage path against a class 10 target].

**Decompile index.** `FUN_001cdf48`, `001cdb98`, `001cda78`, `001cdcb8`, `001cdd50`, `001cdde8`, `001cdf08`, `001ce080`, `001ce240`,
`001ce458`, `001cde80`, `001ce0a0`, `001ce3f8`, `001cf330`, `001ad9a0`, `001ad520`, `001ad640`, `001ad378`, `001b9aa0`, `001b9550`,
`001cec00`, `001ced58`, `001c7740`, `001c75d0`, `001c7c30`, `001ade58`, `001adcf8`, `0017d030`, `0019a3f8`, `0017cd08`, `001b7c88`,
`0017f160`, `0017da30`, `0017db88`, `0017ef98`, `0018f188`, `001db868`, `00300448`, `00301790`, and the raw instructions of `0x1adbb8`
(alternate callback) and `0x1ad520` (the roll). Tables: `sstm` `0xd8`, `0xdc`; words `0x3716a0`, `0x37b84c`, `0x3852a8`.
**Gaps.** The heal amount is found (`FUN_001ce168`, see "Where Sora's HP comes back"); still open: what sets Mickey's HP to 1 (the leave trigger), what 456 does, the missing motions 0xfc / 0xfe, effect 0x11's script; WP-MK2 read also `FUN_001ce168`, `001b8658`, `001b82b8`, `001b84c8`, `001b8710`, `00175ec0`, `001a1ba8`, `001ce0a0`, `001772b0`, `00178cb8`, `00178d28`, `00176e00`, `00186238`, `001a0100`, `0019f920`, `00188030`, the frame-trigger jump table 0x37aa78 (raw), the class tables 0x360360 / 0x360410; `FUN_0017d030`'s exact meaning for a downed friend;
`FUN_001c7740`'s `FUN_001c75d0(0)` effect on the friends; whether `0x35d268` bit 1 is cleared by Mickey's end.

### 26.6 The scene `obj/GAMEOVER.mset` is the list's model panel (WP-GO2: `menu_gameover.rs`, `model`)

**What the original does [V from code, data read].**
* Resource and motion. `FUN_001ade58` (task step 4) frees resource slot 5000 (`FUN_0016f8e0`) and loads `obj/GAMEOVER.mset`
  into it (`FUN_0016fac8` -> `FUN_0016f9c0`, string at `PTR_s_obj_GAMEOVER_mset_0035edfc`); the call `FUN_0016e638(list header word,
  -1, 0x1e)` that follows only loads the player's own motion sets (it is the resource system's "load the sets of object X"
  entry: `FUN_0016e430` loads the object's `.mset` through the entry type at `+4`), it starts no motion on the actor [read: `FUN_0016e430` only requests the sets and the resource entries; no call into the motion player]. `FUN_001adcf8` (step 5)
  then stores two words in the game-over object: `+4` = the loaded file (`FUN_0016fd18`) and `+0x34` = `FUN_001add40(object, header word)`.
  `FUN_001add40` takes the **BAR entry at index 0xfc (type 0) or 0xfd (types 1 and 3)** of the file (`FUN_001684c0(file, -1, n)`:
  the n-th entry of any type; its data word `+8` must be non-zero), formats `%03d0` (string at `0x37b878`) with the **player
  object's `objentry` byte `+7`** (`FUN_0016e208(header word)` is the row; the byte is the sheet's `WeaponJoint`, 1 for every Sora
  row, `P_EX100` and its forms), takes the **third entry of that name** in the entry (`FUN_00168510(entry, -1, name, 2)`) and the **first type 9 entry**
  inside it (`FUN_001684c0(.., 9, 0)`): a motion in the usual ANB layout (motion, type 16 triggers). This is the same
  naming as the paired motions of the reaction commands (`enemy_ai`, `FUN_0017ef98`, string `%03d0`).
* The file (107952 bytes on the disc): 254 BAR entries, all `DUMM` and empty but index 252 (`AK10`, type 20, 71264 bytes) and 253
  (`AK12`, type 20, 32608 bytes). Each holds the motion-set layout of a player mset (four state variants per name, the
  third non-empty): `AK10` has the names `0010`, `0011`, `0012`, `0140`, `0141`, `0142`, `0200`, `0201`, `0270`, `0271`, `AK12`
  `0010`, `0011`, `0012`, `0200`, `0201`, ...; Sora's `0010` entry holds a 20912-byte motion in `AK10` and a 17728-byte one in `AK12`.
* Who shows it. The list's page build (`FUN_0026f698`, task `FUN_0026f4a8`) reads the two header words from `FUN_0026f480`
  (`DAT_0036d020`): the object id and the motion. When both are non-zero `FUN_0026f948` runs: a window `FUN_0029b600(DAT_0036d00c,
  ..., 0x10, 0x11)`, then the **camp's model object** (`FUN_00295bc0` = `DAT_0036d1f8`, the 3D party models of section 16.6): its vtable `+0x18`
  puts the object `id | 0x20000000` in **slot 0 at x 0, y 0** with no motion of its own (`motion -1`, variant 2), `FUN_0023b0f0(obj,
  0, motion)` gives it the GAMEOVER motion (frame 0, play flag), `FUN_0023b348(DAT_00381b28)` the translation z (the word is -100.0,
  the camp's value), `FUN_0023b3a8(0)` / `FUN_0023b3c8(0)` the rotation 0, and `FUN_0023b4b8(obj, 0, DAT_0036d00c)` makes the
  list window the alpha source. The per-frame step `FUN_0023a920` advances the frame by the frame step (`FUN_0014cbe8`) and **wraps
  it at the motion's end** (`vtable +8` = the frame count: frame minus count), so the motion loops; the idle-variant switch of the
  camp (`+0x6d8` / `+0x6dc`, a random 10 to 29 times `DAT_0037f4a8`) needs a second motion in those slots; `FUN_0023b0f0` writes only `+0x6d4` [inferred: nothing else fills them for this slot, so the one motion loops]. The alternate list (record 10) has no second
  header word (step 5 opens it before `FUN_001adcf8`; step 4 plays scene motion 7 through `FUN_001cdf08` instead), so it shows no model.
* The model stands where the camp's slot camera puts a slot at the screen centre (section 16.6: eye (0,-50,-800), look-at (0,0,55),
  horizontal field of view 40 degrees, model at (0,0,-100), colour 0x60 of 0x80).

**What the port does.** `model::Stage` reads the file and Sora's row (`P_EX100`'s `WeaponJoint`) from the disc when the list loads
(`Stage::read`), keeps the two motions as `Arc<kh2_anim::Slot>` (nothing is stored on disk or in the repo) and, while the list is
up (`Phase::List` and `Closing`, not the alternate list), `model::show` builds a private camera and Sora's rig (the camp preview's
recipe: `PreviewCamera`, an image of the menu canvas at twice the resolution, render layer 11, a node of the menu scene that shows the picture
right before the rows) at slot 0, x 0, y 0 (`menu::preview::origin_on_screen`, `model_drop`), `Animator::play_external(motion)` looping,
`model::animate` posing it on the real-time clock while the field is stopped and `model::layer_and_tint` giving the meshes the layer and the 0x60/0x80
colour. The picture fades with the close like the list. The model goes away when the list closes or the Load screen opens, and comes back with the
list. `KH2_NO_GAMEOVER_SCENE=1` turns it off. Type 3's list ("Quit" / "Retry") uses the 0xfd motion as types 1 and 3 do. Tests: `the_scene_set_and_entry_name_follow_fun_001add40`,
`the_scene_entry_is_the_third_of_its_name_in_the_asked_set`, `the_disc_scene_has_sora_motions_in_both_sets`.

**Harness.** `KH2_NO_SOUND=1 KH2_NO_MUSIC=1 KH2_NO_ENEMIES=1 KH2_GAMEOVER_KO=6 KH2_SCREENSHOT=<png> KH2_SHOT_AT=11 kh2-game tt00` logs `game over: obj/GAMEOVER.mset set 0xfc entry 0010 holds a motion of frames Some(0.0)..Some(150.0)`,
`set 0xfd ... Some(0.0)..Some(45.0)` and, when the list opens, `game over: the model stands in slot 0 at menu (256, 218), depth 702`; the picture shows Sora lying on his back above the screen's middle
(his own model, dark tint) with the list's rows at the upper left and the field dimmed. The harness knock-out leaves the field Sora standing (`KH2_GAMEOVER_KO` plays no fall motion), so
two Soras are visible in that picture; in play the field one lies in his fall pose. `KH2_NO_GAMEOVER_SCENE=1` turns the model off.

**Gaps.** Closed or narrowed by WP-GO2b, section 26.8: the wait before the list, the fallen Sora behind the model and the window's alpha. The model of another player (Roxas, a form) uses the entry named by its own `WeaponJoint` (`0140` for byte 14 ...), which the app does not need: the game over is Sora's.

### 26.7 The party refreshers (WP-GO2)

`FUN_001cb208` (the copy of the save body back, section 26.3) ends with `FUN_001abfe8(8)`, `FUN_001b81f8`, `FUN_0023ea18` and
`FUN_00168928`. Read in full now:
* **`FUN_001abfe8(8)`** (the switch has no case for 8; the default falls to the loop) runs for the member ids 1 to 15: `FUN_0019fe48(id)`
  gives the member's status object (id 1 and 0xe: `FUN_0019fdc0`, which rebuilds Sora's status from his record with `FUN_0019f920`; the
  others `FUN_0019fd30`, which finds or makes the status of that record), `FUN_001ac130(8, status)` and `FUN_0019ffb8(status)` (writes the status back into the record,
  `FUN_001a0030`: bytes `+4`..`+7`, and drops the reference). `FUN_001ac130` is `FUN_001a0240(status, 0)` + `FUN_001a0100(status, status +4, 0)`:
  `FUN_001a0240` ends a running MP charge (`FUN_001a05d0` tests the float `+0x1c0`, `FUN_001a05a8` zeroes `+0x1bc` / `+0x1c0`) and adds the MP maximum (`+0x184`)
  to MP (`FUN_001a0210` -> `FUN_001a0180`, clamped to 0..maximum); `FUN_001a0100` adds the HP maximum (`+4`) to HP (clamped to the minimum `+8` and the
  maximum). **Every member comes back with full HP and full MP, whatever the restored record held.** This is the same pair the form start uses
  (`magic::refill_mp`, docs/systems/drive-limits-summons/drive-forms.md).
* **`FUN_001b81f8`** zeroes 0x1c bytes at `0x1d5d7c0` (`FUN_001b8218`'s object). Its other readers and writers (`mips_scan`): `FUN_001a0968` (the damage scale of the
  Struggle bit `0x20000`, the word `+8` plus 1; the only writer of that word is this clear), `FUN_001d30d8` / `FUN_001d31b8` / `FUN_001d3238` (`+0` from the byte tables at
  `0x37c4f8` / `0x37c508`, global event 0xe1), `FUN_001d54d8` / `FUN_001d55d8` (`+4` is a sum of the values in the word table `0x37c5d0`, `+0x14` doubles a factor), `FUN_001d58e0` (clears `+0x18`, `+4`,
  `+0xc`, `+0x14` and runs `FUN_001abfe8(0xc)`; called by `FUN_001b9c58` for mission kind 0x1e and `FUN_00242090`). It is the **Struggle mission's score block**. The app has
  nothing that lives in it (the factor is 1, section 21 of docs/systems/world-minigames/struggle.md; the score is `struggle.rs`'s), and a Continue never happens inside a Struggle (its areas take no snapshot),
  so there is nothing to clear: not ported, no gap for this package.
* `FUN_0023ea18` (the sound mode of the card: `FUN_0023eb10` / `FUN_0023eb78` pick mono/stereo from the option word, `FUN_002bddf4`) and `FUN_00168928` (resets the resource slot table `DAT_00522178`
  and `FUN_0016fdf0(0)`, `FUN_00178a38`, `FUN_0019d8c0`, `FUN_001d3a30`) were not decoded further: they touch the resource system and the camera-side objects the app does not have.

**What the port does** (`refresh_party`, `refreshed`, run by `drive` when the list has closed on "Continue", right after the body is back): Sora's HP is his maximum, MP the data's
maximum (`GameData::sora_max_mp`) through `magic::refill_mp` (which ends a running MP charge), and each `PartyMember` gets HP / MP at his own maximum and his charge ended (a member's KO flag is
left: the jump builds the members again). **This replaces WP-GO1b's "Continue gives the area-start HP and MP"** (an [inferred] reading, 26.4): the refresher gives the maximum, so the snapshot's
HP / MP are only logged. The Load Game path does not run `FUN_001cb208` (its jump has the restore flag 0) and keeps the record's HP and MP. Test: `the_party_refresher_fills_hp_and_mp_and_ends_the_charge`.
`party.rs` was not touched: `PartyMember`'s `pub` fields and the `pub(crate)` charge are enough; nothing is needed there.
### 26.8 Game over follow-ups (WP-GO2b: `menu_gameover.rs`)

**B. The wait before the list [V, ticks inferred].** `FUN_001ad378` starts the knock-out event with `FUN_00195898(obj, 0xf, FUN_00193f08(0))`,
`FUN_00195a78(obj, player)` and `FUN_00195ae0` (types 0, 2, 3: the argument is the player; type 1 passes 0 and starts none). `FUN_00195ae0` stores the
event object in `DAT_0035e3a4`, which `FUN_00196418` tests. Step 1 of `FUN_001ad9a0` sets `DAT_0036f598 = 1` (`FUN_002c38b8`) and falls into step 2,
which returns "again" while `FUN_00196418` is true and while the screen fade object (`FUN_0014a978` = `0x356b30`) has `+0xfc != 4`; when both are over it
clears `DAT_0036f598` (`FUN_002c38c8`), lays the player on the floor (`FUN_00183fe0`: a ground probe `FUN_001a5c30` and the write-back `FUN_0016a618`, no motion
of its own), for the field types runs `FUN_00196258` (below) and `FUN_001c67b0` and goes on. The numbers come from the event of kind 0xf that docs/systems/drive-limits-summons/summons-objects-1.md
"WP-RV2" read (`00battle.bin` type 0x16 entry 0, `re_gameover-0a`): end frame 0x15 (the event object dies at tick 42), command 0x12 at event frame 0xb
(tick 22) with length 0x32 and mode 2 = `FUN_0014a7c8(0x356b30, 0x32, 1)`; `FUN_0014a710` doubles the length (counter 100), `FUN_0014a818` takes
`DAT_00356f44` (1.0) off it per tick, state 3 -> 4 at zero. So `+0xfc == 4` at tick 122, after the event object is gone, and the wait is 122 ticks.
Step 3 (no case) takes one tick. Step 4 waits for the words `0x35feb8` / `0x35fec0` to be zero and for the resource system (`FUN_0016ff50`: the stream queue
`FUN_00168240`, `FUN_002c13c0`, the list `DAT_0035bd88`), then sets bit 0 of `0x35feb8` (`FUN_001c6bd0`) and loads the scene (`FUN_001ade58`). Step 5 waits for
the resource again and opens the list. The two words are marks of the Mickey / revive objects: bit 4 of `0x35fec0` is set by their constructors (`FUN_001cda78`,
`FUN_001cdb98`) and both words are cleared by their destructors (`FUN_001cdcb8`, `FUN_001cdd50`, `FUN_001cdde8` call `FUN_001c6be8(0x35feb8, 0)` and `(0x35fec0, 4)`) and by
the reset `FUN_001c6c30`. With no such object alive both are 0, so the list opens at tick 122 plus one tick (step 3) plus the disc streaming time of
`obj/GAMEOVER.mset` (about 108 KB, which the app reads at once) [inferred: 60 ticks per second, as docs/systems/world-minigames/struggle.md]. Port: `open_after(kind)` = 122 / 60 s for types 0 and 3,
counted from the knock-out edge (the event starts in the same call); it was 1.5 s. Type 1 (`FUN_001ad868`, a manager that ended by itself) starts no event, so step 2 waits
only for a fade the mission's ending left in state 4; that was not traced, the old 1.5 s stays [inferred]. Test `the_wait_before_the_list_is_the_knock_out_events_fade`.
Searched: `mips_scan --addr 0x35e3a4` (28 hits, all in the event object's methods `FUN_00195968` to `FUN_001968d8`), `--addr 0x35feb8`, `--addr 0x35fec0`.

**A. Is the fallen field Sora hidden while the list shows? [V: no actor rule; the fade covers him]** Read: `FUN_001ad9a0` steps 1 to 8, `FUN_001ad378`, `FUN_00196258`
(`FUN_001b7e70(2000)`, `FUN_00196760`, `FUN_001abfe8(0xb)`), `FUN_001a3f68`, `FUN_00183fe0`, `FUN_00186e20` / `FUN_00186db0`. `FUN_001b7e70(2000)` walks the object
list `DAT_01d5d7a0` (next `+0x8b4`): an object whose method `+0x3c` (always `FUN_0016bc68`, the model resource tag, docs/systems/core-runtime/contract.md opcode 0x4d) answers 2000 or more gets its
method `+0x28` (the destructor: enemies, summons); the others (the player and the party members, tags below 2000) get `FUN_00186e20` (a status reset) and `FUN_0016a750` (the stored
placement `+0x890`). It does not hide the player: the hide bit `+0x10c & 0x80` is set only by the event opcode 0x4d (`mips_scan --jal 0x1b7fa8` finds no call). What covers him is the event's
fade: command 0x12 mode 2 is `FUN_0014a7c8(.., 0x32, 1)` and the flag byte 1 makes `FUN_0014a710` pick white (`FUN_0014af90(0xff, 0xff, 0xff, 0)` and blend `0x48`; flag 0 would be black,
`0, 0, 0` and `0x44`), alpha 0 to 0x80 (full) over 100 ticks, then held at state 4 while the list opens (nothing in steps 2 to 8 returns it to state 0; docs/systems/drive-limits-summons/summons-objects-1.md "WP-RV2"
found no clearing caller). So in the original the field, the fallen Sora included, is under a full white fade when the list shows, and the model panel Sora is the only one visible
[inferred: that the fade quad is drawn over the 3D field and under the menu]. The port does not draw that fade: it dims the field (`MenuScene::dim`, a black sprite in `menu_gfx.rs`,
not this package's), so the harness still shows the field Sora under the dim. Not ported, reason: drawing the white needs a coloured dim in `MenuScene` (outside ownership), and no
recording of the original screen was available to confirm white against the dim (the colour is read from the code, not seen). Outside-ownership edit: `menu_gfx::MenuScene` a dim colour
(white) and `menu_gameover::drive` to ramp it over ticks 22 to 122 and hold it while the list shows.
*WP-RV4 ported it.* Re-read: `FUN_00226830` case 2 passes flag 1 to `FUN_0014a7c8`, which stores it at `+0xf4`; `FUN_0014a710` tests that byte: non-zero is `FUN_0014af90(.., 0xff, 0xff, 0xff, 0)` with word 0x48, zero is black with 0x44; `FUN_0014a818` case 3 raises the alpha byte to 0x80. So the colour is read, not seen, from the code. `MenuScene::fade` (a white alpha, drawn even while the menu is
hidden, `menu_gfx::dim_paint`) is set by `menu_gameover::knockout_fade(kind, secs)`: 0 until tick 22, linear to 1 at tick 122 [linear is inferred], held 1 while the list shows and closes, 0 for type 1 (no event), the Load screen and the jump. The word 0x48 is not decoded: alpha blending is assumed [inferred]. The fade replaces the black dim while it is stronger, so under the list the field (the fallen Sora too) is white. Test `the_knock_out_fade_ramps_from_tick_22_to_122`.

**D. The list window and the model's alpha [partly read, gap].** `FUN_0026f948` builds the window `FUN_0029b600(DAT_0036d00c, parent, 0, 0, DAT_0036d004, 0x10, 0x11, 0)`:
`FUN_00138948` -> `FUN_00138a60` -> `FUN_00138e78(window, 0x10)` starts the animation id 0x10 (`+0x178`), and `FUN_0029b600` then stores 0x11 at `+0x17c`; `FUN_00138aa8` plays `+0x17c`
when the current animation ends and resets it to -1. The model's alpha is read every frame by `FUN_0023b4e0`: `FUN_0023b4b8(obj, 0, window)` stored the window in `+0x14c0`, and the alpha is the
top byte of the window layout's current colour (`FUN_00237518(window + 0x18) >> 0x18`, handed to `FUN_0023b480`). If `DAT_0036d004` is `save.2ld` sequence 0 (the rows' layout, section 26.3; what
`FUN_0029ada8(0x754a, -1)` in `FUN_0026f0d0` loads was not found), animation group 0x10 is one animation looping at the constant colour 128/128/128/128 (alpha 0x80, full) and group 0x11 one
play of 8 frames with alpha 128 to 0 (dumped from the disc; groups 15 to 17 are the fade-in, hold and fade-out of the 28 by 28 additive sprite group 11, which the hand cursor also uses), so
the model would be fully opaque while the list shows and fade out over 8 frames when the window switches to 0x11. *WP-GO3 closed it: the switch never happens.* `FUN_00138aa8` goes on to `+0x17c` only when the current group's play count (group record `+4`, `FUN_002369e0`) is not 0 and the animation has ended (`FUN_002369c0`); `FUN_00138e78(window, group)` passes the group on (`FUN_002368e8` -> `FUN_00236918` stores it at the layout's `+0x110` and the count at `+0x13c`). Group 0x10 of `save.2ld` sequence 0 has the play count 0 (forever) and one animation of 1500 frames at the constant colour 128/128/128/128, so it never ends. The window is `DAT_0036d00c`; the only code that touches it is `FUN_0026f0d0` (create), `FUN_0026f948` (the build above), `FUN_0026faa8` and `FUN_0026f2c8` (`mips_scan --addr 0x36d00c`: 8 hits, all four), and `FUN_0026faa8` calls only its vtable `+0x14` (`FUN_00139778`: sets bit 0 of the word `+0xc`), twice, then `FUN_0023a340(model object)`, which frees the model slots. `FUN_0026faa8` runs from `FUN_00270318` in `FUN_00270098`, in the frame where the closing flag (`FUN_00295bb0`, set by `FUN_00295ba0`) is first seen; the next frame `FUN_0026fd20` ends the task (`FUN_001038a0`). So the model is at full alpha (0x80 from group 0x10) for as long as the list shows and is gone at the first closing frame; group 0x11 (8 frames, alpha 128 to 0, play count 1) is never played. `FUN_0023b4e0` gives alpha 0 when the window is not alive. Searched: `mips_scan --store-off 0x178`, `--store-off 0x17c`, `--load-off 0x17c`, `--jal 0x138e78` (363 hits, none on this window outside the above). Port: `model::show` keeps `label_fade` at 1 and removes the model when the phase leaves `Phase::List` (it used to fade with the close, 14 frames [inferred]); tests `the_model_stands_only_while_the_list_shows`, `menu::save::tests::the_game_over_windows_first_animation_never_ends_so_group_0x11_never_plays` (disc).

**E. Type 1's wait (`FUN_001ad9a0` step 2) and the list build for type 1 [V from code, duration not derivable].** Type 1 is `FUN_001ad868` -> `FUN_001ad378(obj, 0, 1)`: no event object (`DAT_0035e3a4` stays 0, so `FUN_00196418` is false), and step 2 then waits only for the screen fade object `0x356b30` (`FUN_0014a978`) to reach state 4 (`+0xfc == 4`). That object's idle state is 0 (`FUN_001047a8`, the start of every area, runs `FUN_0014a7f0(.., 0, 0)` on it and the update `FUN_0014aa00` -> `FUN_0014a818` returns it to 0 after the fade-in). Its only other writers are the event player: `FUN_00226830` (event command 0x12, as the knock-out event's), `FUN_002226d0` (called by `FUN_00222728` while `DAT_0036bcd8` is 1: `FUN_0014a7c8(.., 0, 0)`, a black fade-out of two ticks) and `FUN_00223a88` (the reset at an event's start); `mips_scan --jal 0x14a978` lists exactly these nine callers (`FUN_001047a8`, `FUN_001ad9a0` twice, `FUN_002226d0`, `FUN_00223a88`, `FUN_00226830` four times). The mission scripts' own fade commands (`FUN_001b65e0`, cases 5 to 8) act on a different object, `0x3564d0` (`FUN_0014a918`), which `FUN_0024f330` clears (`+0xfc = 0`) when it opens a list whose record has bit 0 of `+0x25` (no cancel), as both game-over lists do. So a type 1 game over (the manager ends by itself: `FUN_001b9ce0` from `FUN_001b96e0` with global event 8, or from `FUN_001b9818`) waits for the fade that an event left in state 4, and the original would wait for ever with none; the length is that event's, which the app does not play for a mission ending. Kept: 1.5 s [inferred] (`OPEN_AFTER_TYPE_1`). The list build for type 1 is the same as type 0's: `FUN_001ad640` stores the same header words (the player's object id with `0x20000000`, record 1), `FUN_001adcf8` the motion of entry 0xfd, so `FUN_0026f948` stands the model in slot 0 (already ported: `set_index`). Open: which mission endings leave the fade in state 4 (the event scripts of the 11 + other missions were not read).

**F. The Load screen's header [closed by WP-GO2, checked by WP-GO3].** The header word is the `load` layer set's group 20 (section 18.6 gap 1); a screenshot of the Load screen opened from the game-over list (`KH2_GAMEOVER_KO=6 KH2_KEYS=9:Down,10:J`, shot at 13 s) shows the word LOAD in the header and the tab "Load" under it, not SAVE.

**C. Tests.** `the_whole_continue_restores_the_body_refreshes_the_party_and_starts_the_jump` runs `drive` from the closed list on "Continue": the body (munny and EXP of the area start), Sora's
maximum HP and ended MP charge, the jump to the snapshot's area with fade kind 1 and the phase `Jumping` (skipped without `KH2_ISO`). `an_allys_refresh_gives_full_hp_and_mp_and_ends_his_charge` tests
the per-member rule (`refresh_vitals`, which `refresh_party` calls for each `PartyMember`); a `PartyMember` cannot be built outside `party.rs` (private fields, no test constructor), so the query
loop itself is not exercised.

