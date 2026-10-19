# Summons (backlog B5): implementation spec

Work package 6F (research); stages 1 and 2 implemented by WP-7J, stage 3 and parts of stages 5 and 6 by WP-7J2
(see "Status" at the end of section 6).
This note is meant to let a later work package build summons without reading the game code again.

Marks: **[V]** read in the game's code (address given) or checked against the disc or a kh2-ee run;
**[I]** inferred (fits the code but no line states it); **[GAP]** not found, with the addresses searched.
Addresses are functions (`FUN_xxxxxxxx`) or data in `SLUS_210.05`. No game values are copied here except
where a test prints them from the user's disc; the code reads them at run time.

Evidence that runs:

- `cargo test -p kh2-tables --test summons -- --nocapture` prints the `sumn` rows joined with their
  charms, commands, objects and limits, and the summon command family (names from `msg/us/sys.bar`).
- `cargo test -p kh2 --test summon_oracle -- --nocapture` runs the game's own FUN_001a0480, FUN_001a0420,
  FUN_00155bf0 and FUN_001c65f0 in kh2-ee and compares them with the rules written below (gauge mode set,
  whole segments, HUD digit and fill, the arrival event choice).
- `kh2 bdx N_HB040_BTL` (and `P_EX350`, `P_AL010`, `P_EX330`, `P_EX340`) disassembles the summons' scripts.

Earlier notes: `docs/player-data.md` ("Summons", the drive gauge and form timer), `docs/limits-reactions.md`
(limits), `docs/drive-forms.md` (the form timer as ported), `docs/hud.md` (drive gauge modes),
`docs/hit-stop.md` (actor stop ids), `docs/bdx.md` (script runtime and modes).

## 1. The data

### `sumn` (00battle.bin, 4 rows) [V]

64-byte rows; FUN_001d4c68 walks them (`+8` from the table start, step 0x40), FUN_001d4ca0 finds the
1-based row index of a command, FUN_001d4cf8 turns an index back into a row, FUN_001d4d28 returns the row of
the active summon (index in `DAT_00340c88` byte 5, written by the summon start, 0 = none).

| field | offset | used by | meaning |
|---|---|---|---|
| `Command` | +0 | FUN_001d4ca0, FUN_001d4d48 | the summon command (cmd `Execute` 72) |
| `Item` | +2 | FUN_001d4d48, FUN_001d4e68 | the charm item (type 20); a summon is offered only while its charm's count is above 0 (FUN_001a1cc8) |
| `Entity1` | +4 | FUN_001c6260, FUN_001c6710, FUN_001d4da8 | objentry id of the summon object that is spawned |
| `Entity2` | +8 | FUN_001d4da8 only | a second object whose files are preloaded with the summon (Tinker Bell for Peter Pan); the game never spawns it from this field: Peter Pan's script and the revive code create it (section 4) |
| `LimitCommand` | +0xc | FUN_001c66a0 -> FUN_001d20d8 | the summon's limit command, used to preload the limit's resources. The value 609 (Genie) is a special case: FUN_001d20d8 then preloads the four commands in the table at 0x360738 (542, 544, 546, 548, Genie's per-form limits) |

The rows on the USA disc (printed by the test): Chicken Little (cmd 447, charm 383, object 1736
`P_EX350`, limit 529 "FPS Mode"), Genie (89, 159, 2037 `P_AL010`, 609), Stitch (88, 25, 778 `N_HB040_BTL`,
519 "Ohana,"), Peter Pan (306, 160, 1115 `P_EX330` and 1268 `P_EX340` Tinker Bell, 468 "Never Land"). The
1-based indices are 1 Chicken Little, 2 Genie, 3 Stitch, 4 Peter Pan.

### Related rows [V, from the test]

- The four summon commands have `Execute` 72, `Cost` 3 (drive points), `Flags` 0x112, menu icon 5.
- The summon objects are objentry type 19 with `NeoStatus` 200 Stitch, 201 Genie, 202 Peter Pan,
  203 Tinker Bell, 204 Chicken Little. The same numbers are the `limt` `Summon` byte of the summon limits
  (FUN_001d1fe0: byte - 200 < 5) and are what the friend-AI constructor switches on (section 3).
  Chicken Little's objentry names spawn object 2098 (his script creates it, `kh2 bdx P_EX350`).
- Summon limits (`limt` rows 14 to 20): `Character` 1, `Summon` 200..204, `Limit` = the charm. Command
  costs: Never Land, Ohana, FPS Mode 100; Genie's Sonic, Strike, Arcana, Infinity 50. `Time` 600 (24 or 60 for
  Genie's).
- Sub-commands a summon limit uses (Execute 81 rows without a `limt` row): 469 (Never Land's follow-up),
  521, 525, 526, 527 (Stitch: `Require` 600), 530, 531, 532, 534 (Chicken Little), 539, 540, and Genie's
  finishers 543, 545, 547, 549 (`Require` = the per-form command 535..538, `Mark` 4).
- Genie's per-form commands 535..538 ("Valor Genie" .. "Final Genie", `CmdReceiver` 1) and the menu entry
  605 "Drive!" that lists them. The table at 0x37c036 maps form id 1..4 to 535..538 (FUN_001c6b10).
- Menu entries: 72 "Summon" (`SubMenu` 7, the SUMMONS list), 316 "Dismiss" (`Execute` 316), 104 the blank
  entry, 73 "Party", 81 "Limit".

### Tuning (`sstm`, printed by `kh2 table sstm_full`) [V]

| offset | use |
|---|---|
| 0x68 | size of one gauge segment (600 units on this disc). Summon timer = this x (summon level + 3). Also the timer loss for damage equal to the summon's maximum HP |
| 0x6c | drain scale per frame while the battle flag (FUN_001903d8) is clear |
| 0x70 | multiplier on every gauge change made through FUN_001788d8 in modes 2 and 3 (the summon limit's cost is `Cost` x this) |
| 0x84, 0x88 | the summon start wait in frames: 0x84 when the arrival plays as an event, 0x88 otherwise (FUN_001c6260) |
| 0x16c | Summon Boost: multiplies the ability block's summon drain factor (+0x68 of the block) |

## 2. The gauge, the command and who may summon

### Gauge mode 3 [V]

The player's status block (object +0x48c) carries one gauge used three ways, selected by the mode byte
+0x1af: 1 normal drive points, 2 form timer, 3 summon timer, 4 locked. Mode 3 uses the same fields as the
form timer: +0x1b3 segment count, +0x1b4 timer, +0x1b8 its maximum. The drive points (+0x1b1) and fraction
(+0x1b0) are left alone while the summon runs, apart from the cost paid at the start.

- Start (FUN_001c6260, after the arrival wait): `FUN_001a0480(status, 3, summon level)` sets mode 3,
  segments = level + 3, timer = maximum = `sstm[0x68]` x (level + 3). The summon level is `DAT_00340c88`
  byte 6 (save data). Checked by the oracle test for levels 0 to 6.
- Drain (FUN_00177788, every frame, modes 2 and 3, not while an event runs): the frame unit, times
  `sstm[0x6c]` while the battle flag is clear, times the ability block's +0x68 (Summon Boost) in mode 3;
  applied by FUN_001a0358 with a negative sign, clamped to 0..maximum. The app's
  `drive::rules::drain_per_tick` already does this with the form factor; mode 3 swaps in the summon factor.
- Damage to the summon drains the timer instead of the summon's HP: the summon class overrides the HP
  change method (vtable 0x35fe18, slot +0x78 -> FUN_001c60f8). For a negative change `d` it changes the
  player's timer by `sstm[0x68] x d / (summon's max HP, its status +4)` and does not call the base HP
  routine (the party class's slot is FUN_00176c98, which ends in FUN_00186238). So damage equal to the
  summon's maximum HP costs one segment. [V for the code; I that the summon's HP therefore never drops]
- Gauge changes from hits and summon limits go through FUN_001788d8: in mode 3 the amount is multiplied
  by `sstm[0x70]` and added to the timer (FUN_001a0358).
- Summon experience: in mode 3, FUN_001a0358 counts whole segments before and after the change
  (FUN_001a0420: floor(timer / segment), minus one when the timer is full; checked by the oracle) and
  passes the drop to FUN_001d4ec8. That adds it to the summon experience (`DAT_00340e4c`) and levels up
  while the `fmlv` form-0 row of the current level is reached (FUN_001d3cf8 -> FUN_001d3b90(0, level)).
  No row is returned once the level reaches the cap, and the experience is then set to 0. Nothing is gained
  while the minigame flag of FUN_001b9bd8 is set, and a gain (negative drop) is ignored.
- Summon level cap (FUN_001d4e68): 2 + the number of summon charms owned, and 7 when that sum is 6 (with all
  four charms). Level-up (FUN_001d4fa0): a notice with message 0xc5bd through FUN_001b5578..FUN_001b5828; the
  row's ability (none on this disc for form 0) would be added and equipped; FUN_0019fb98 refreshes the
  status; FUN_001d5718 is called [I: journal/notice bookkeeping].
- End: the timer reaching 0 (below), or Dismiss. Both set the mode back to 1 at once (FUN_001cf298 and the
  Execute 316 case of FUN_001cf030).

### The HUD for mode 3 [V]

The drive gauge panel already switches groups by mode (docs/hud.md: mode 3 uses bar intro 140, idle 141,
fill 139 and the digit table at 0x358998, groups 144..153). For modes 2 and 3 the digit and fill come from
FUN_00155bf0: segment size = int(max / count); digit = int(timer) / size, and when that equals the count it
shows count - 1 with a full fill; otherwise the fill is int(timer) mod size. Checked by the oracle test.

When the mode changes into 3 the panel's per-frame routine (FUN_00157a50, predicate FUN_00156c48) calls
FUN_00156d08, which starts a small task (FUN_00156ca8) that waits until party slot 0 holds a summon object
(objects with flag 0x400000 at +0x588), then plays group 0x9a then 0x9b at panel +0x69c with the summon's
face (FUN_00156e30: FUN_0016b998 on the party slot 0 object, drawn by FUN_00153b98 at +0x1300). When the
mode leaves 3 (FUN_00156c78) FUN_00157020 plays group 0x9c. Summon objects get no party HP panel
(FUN_0017c968 skips FUN_0013dcd8 for them). [V for the calls and groups; I that 0x9a/0x9b/0x9c are the
summon panel's in, idle and out animations]

### The command menu [V]

- The list built by FUN_001bc6e0 (menu +0x2dc, four entries, unless a map overrides it through
  FUN_001bb538 slot 3): entry 0 the attack command (FUN_001bc4c8); entry 1 "Dismiss" (316) while a summon
  is active (state below non-zero), else "Summon" (72) when the summon list (+0x350) has entries
  (+0x358 > 0), else the blank entry 104; entry 2 "Drive!" (605) while Genie is out (state 2 and the summon
  object's NeoStatus is 201), else "Party" (73) or blank by story flags (FUN_001807a0); entry 3 "Limit" (81)
  or blank (FUN_001807f8). Which on-screen page this list is was not checked [I].
- The SUMMONS list (+0x350) is filled from FUN_001d4d48: every `sumn` command whose charm is owned
  (FUN_001bbe58 at menu creation, FUN_001bc8b0 on refresh).
- Genie's "Drive!" sub-list (FUN_001bcf10): for each drive form the player owns (FUN_001c6a20: forms 1..4
  whose form item count is above 0) the command from the table at 0x37c036, with Genie (found by
  NeoStatus 201, FUN_0016b138) as the command's target record.

### Availability of a summon command [V]

FUN_001ba2c0, case Execute 72, gives status 5 (unavailable) when any of these holds, else 0:

1. The current battle record forbids summons: `FUN_001ba0c0` is true (bit 14 of the u16 at +4 of the
   record that `DAT_01d5dd20` + 8 points to; no record means allowed).
2. `FUN_001ba568` is false. That routine passes at once for a menu entry (FUN_001b7098: `SubMenu` set and not
   4), else needs: the words `0x35feb8` and `0x35fec0` both zero (bit 0 of the first is set by a summon or limit
   sequence, bit 3 of the second during the summon arrival),
   the player valid with flag 0x1000000 at +0x588 (the controlled player object), `FUN_001c7970(cmd)` = 0,
   the form field (+0xb50) not 6 or 0xb, `DAT_0035d268` bit 0x40000 clear, and `Cost` <= drive points
   (+0x1b1).
3. `FUN_001c7970` for Execute 72 is FUN_001c78f0: both party slots (`FUN_0017cd08(0)`, `(1)`) must hold valid
   objects for which FUN_0017d150 holds, unless bit 13 of the same battle-record word is set
   (FUN_001ba080). [I: FUN_0017d150 tests that the member is in the current party (FUN_001a7698); this is
   what keeps Summon out of drive forms, where the companions are absorbed]
4. The command flags (FUN_001b70b8) or `DisableForm` (FUN_001b7290; 0 for command 72) refuse it.

Dismiss (316) is available only while the summon state is 2 and no limit task exists (FUN_001b3c00).

## 3. Spawn, the arrival, the player during it, and despawn

### Execution (Execute 72, inlined into FUN_001cf030; FUN_001cf210 + FUN_001c6168 are the same code) [V]

1. The player plays motion request 0x8a (blend `DAT_0037c358` = 8) through FUN_0017e220.
2. System effect 0x13 (`00effect.bar`, the same library as the clash spark) starts bound to the player
   (FUN_001db868 on `DAT_01c954d8`); its handle is kept at player +0xb68.
3. `Cost` drive points are spent (FUN_001a0320; when the points were full or short the fraction is cleared).
4. `DAT_00340c88` byte 5 = the sumn index; `0x35feb8` bit 0 set; summon state (`DAT_01d5e368`) = 2.
5. Byte 7 of `DAT_00340c88`: for Genie (89) a random owned drive form (FUN_001c6a88: LCG `DAT_003716a0`
   times 0x10dcd, then the form list of FUN_001c6a20 indexed by the new word divided by `(2^32 + n - 1) / n`,
   FUN_00325b38 being an unsigned 64-bit division; checked by the oracle in `summon.rs`); for Peter Pan (306) 1
   (Tinker Bell available, below).
6. Player flags +0x108 |= 0x104 [I: 0x100 makes the player unhittable (FUN_00186638 reads it), 0x4 makes
   FUN_0019a530 report "busy"].
7. A thread (FUN_00102808 at 0x1c6260, stack 0x2000, handle `DAT_0035fe08`, cleanup 0x1c6550) runs the
   arrival.
8. The two companions fade out over 15 frames (`DAT_0037bff0`) and are listed for return
   (FUN_001c7740 -> FUN_001c75d0 -> FUN_001c7600: fade record at member +0x82c from 1 to 0, flag 0x10 at
   +0x7f8, member id stored in the list at 0x1d5e380).
9. FUN_001c6578: `0x35fec0` bit 3 set; `DAT_00356f1c` |= 0x10 [GAP: meaning]; the battle record's +0x528 |= 1;
   actor stop id 2 is put on every actor except object classes 0, 1, 5 and 19 (FUN_0016bef0(2, 0xfff7ffdc);
   docs/hit-stop.md lists this caller as a "command cut-in": it is the summon start). Undone by
   FUN_001c65b8 when the thread ends.
10. The summon command's use counter (+0x34184a + 2 x index, capped at 999) goes up.

### The arrival thread (0x1c6260, decompiled on request) [V]

1. Choose event or skip: FUN_001c65f0(index, player) is true when the player's +0x810 is 0 and the config
   option (bits 9..10 of `DAT_0033e860` +0x31b0, FUN_0023eed8) is 1 (always), or 2 and the summon's bit in
   `DAT_01d5e370` is clear (first time; the bit is then set; FUN_001c6930 / FUN_001c6bb0 clear the word,
   the first from overlay 1). Other values mean no event. Checked by the oracle test for every case. [I: this is
   the summon-effect option of the game's config menu]
2. Wait while the screen fade list is busy (FUN_0016ff50), counting the wait down.
3. FUN_0016f8e0(5000) and FUN_0016fb20(0) around the preload [GAP: both walk the fade/screen list at
   `DAT_0035bd88`; their effect was not worked out], FUN_001c66a0(0) (preload: FUN_001d4da8 loads the files of Entity1 and Entity2 with
   FUN_0016e638(id, -1, 0x1e), which picks load class 7 for objentry type 19; FUN_001d20d8 loads the limit
   resources of `LimitCommand`).
4. Wait until the fades are done, the wait (`sstm[0x84]` with event, `sstm[0x88]` without) has run out, and
   FUN_0017d470 says both party slots are empty (the companions have gone).
5. Player +0x108 |= 1; gauge mode 3 starts (above).
6. The summon object is created at the player's position (FUN_0016b960): 0xd00 bytes, objentry of Entity1
   (FUN_0016e208), constructor FUN_001c5e20.
7. Without event: the effect 0x13 is told to end (FUN_001d9f48); the player plays motion 0x82 and FUN_0017e378;
   a free spot 200 to 300 units from the player is picked (FUN_001a5ea8 with `DAT_0037c00c`, `DAT_0037c010`)
   and the summon is put there (FUN_0016a618); the summon's script switches to mode `"appear_skip"`
   (FUN_00186e20 -> FUN_00186db0, mode by name, at summon +0x300).
   With event: the effect is killed at once (FUN_001da028); the summon's mode becomes `"appear"`; summon
   state 1; HUD layers 7..10 hidden (FUN_0014eec0 -> FUN_0014f198); wait while an event runs
   (`FUN_00196418`, `DAT_0035e3a4`); HUD layers shown again (FUN_0014eea8).
8. Summon state 2; player +0x108 &= ~0x105; the summon objects' +0x108 bit 0 cleared.
9. Thread cleanup (0x1c6550): FUN_001c65b8 (undo step 9 of the execution) and `DAT_0035fe08` = 0.

### The summon object [V]

- Class: the party-member ("friend") object, 0xd00 bytes, base constructor FUN_0010b780 (via
  FUN_001c5e20 / FUN_001c5d78), vtable 0x35fe18. It overrides the destructors (FUN_001c5f80,
  FUN_001c6038: remove from the summon slots and set the state to 0 when both are empty), slot +0x54
  (FUN_001c6998 -> FUN_001970b8), slot +0x78 (FUN_001c60f8, damage drains the gauge) and slot +0x98
  (0x1c6928, returns 0). All other slots are the party class's (vtable 0x352d88).
- Registration: object flag +0x588 |= 0x400000 (the "summon" flag several systems test: no party HUD panel,
  not put into the limit state, not offered in the party menu (FUN_001bcb98), not counted for item sharing
  (FUN_001b84c8)); +0x768 bit 4 cleared; stored in the first free of two slots at 0x1d5e360/0x1d5e364; a
  second object is linked to the first (FUN_001dd938 on +0x80).
- Friend AI: FUN_0010b780 picks an AI object by NeoStatus (stored at +0xb60, parameters from the table at
  0x34ea00): 200 Stitch FUN_001179c0 (0x790 bytes, AI vtable 0x354870), 201 Genie FUN_001199f8 (0x354910),
  202 Peter Pan FUN_0011a048 (0x354978), 203 Tinker Bell FUN_0011a230 (0x3549b8), 204 Chicken Little
  FUN_0011a450 (0x354a20). Genie and Peter Pan also get flag 0x800 at +0xb7c ([I] flying: FUN_002f6320 picks
  their motions by NeoStatus 201/202), Tinker Bell 0x802, Stitch 2. The friend AI code is 0x10b000..0x11b000 (288
  functions in the decompilation): the same engine runs Donald, Goofy and the world partners.
- Script: each summon model has its own BDX (`kh2 bdx N_HB040_BTL` etc.). Modes registered by all:
  `appear`, `appear_skip` (not Tinker Bell), `leave`, `dead`, `event`, `freeze`, `idle`, the
  `btl_*`/`rvg_*`/`rst_*` battle modes and `footwork`. Peter Pan also has `appear_area_start`; Tinker Bell has
  `reraise`. Entry events: 0 init and 1 main for all; 22 and 23 for the four summons (Tinker Bell's script
  has only 0 and 1); 15 for Chicken Little and Peter Pan,
  11 for Genie and Peter Pan, 17 for Stitch and Peter Pan, 26 for Peter Pan.
- Host functions the summon scripts use that the app's BDX host lacks: group 10 (the friend library,
  natives 0x2f6a90..0x2f75c0, table at 0x371810; for example 10.45 reads the gauge timer +0x1b4 of a status
  block, 10.19 tests +0x10c bit 0x10000 "taking part in a limit", 10.17/10.18 test +0x588 bits 0x800/0x400,
  10.9 sets Genie's form byte through FUN_00119c18 -> FUN_001c6b48, 10.12 starts an attack through
  FUN_00117708), and 1.318 (FUN_00170b30: play sound `id` at the object, `id / 1000 == 500` remapped to the
  object's own bank). Counts per script (unnamed distinct / total calls): Chicken Little 29/294, Genie
  17/172, Stitch 24/189, Peter Pan 40/399, Tinker Bell 1/90 (`kh2 bdx` listing, this work package).

### What happens in `appear` (example: Stitch, Peter Pan) [V from the scripts]

The mode's enter routine links the player to the summon (host 1.18 link_start, flags 0x4002), plays
motion 231 on the summon and 252 on the player (1.29 play_motion_with), starts the summon's effect "00"
(1.157). The mode's main routine waits while an event runs and has passed 20 frames (1.274 reads the event's
frame, FUN_00196668), plays sound 598050 (1.318), waits until the event ends, ends
the link (1.102), places the summon and starts its battle behaviour. A routine of Peter Pan's script that does the
same link and motions (most likely his `appear` enter routine [I]) also creates Tinker Bell (host 1.0 with
objentry 1268) and puts her in her own `appear` mode. `appear_skip`'s main routine
ends at once and its enter routine starts the battle behaviour. What starts the event that the thread and
the mode wait on is not identified [GAP: searched FUN_0014f198, the appear routines of N_HB040_BTL and
P_EX330, host 1.318; the link (1.18 -> FUN_001becb8) is the best candidate, unverified].

### Leaving [V]

`FUN_001c67b0` (only when the state is non-zero): state 3, `DAT_00340c88` byte 5 = 0, a thread at 0x1c6810
(cleanup 0x1c68d8 clears `0x35feb8` bit 0):

1. If the first summon object is valid, its mode becomes `"leave"` (FUN_00186db0) and the thread waits
   until both summon slots are empty (the script ends the object itself).
2. One more yield (status 2), then FUN_001c7c88 waits while an event runs or the player has +0x160 bit 0x4000.
3. FUN_001c79c8 brings the companions back: each listed member is re-created (FUN_001c7ab0 ->
   FUN_0017d290) at a free spot 200..300 units from the player (`DAT_0037c05c`, `DAT_0037c060`), searched
   from a point 100 units above the player (`DAT_0037c058` subtracted from the game's Y, which points down),
   and fades in over 15 frames (`DAT_0037c064`, `DAT_0037c068` = 1).

Callers of FUN_001c67b0:

- Timer empty: the player's update FUN_001ceda8 sees the timer <= 0 (and the player not in a form, not
  dead, no action thread, not frozen) with state 2, checks Dismiss (cmd 316) through FUN_00178a78 and calls
  FUN_001cf298 (leave + gauge mode 1).
- Dismiss chosen: FUN_001cf030 case 316 (leave + gauge mode 1).
- The player is knocked out: FUN_001cec00 ([I] the player's death handler): unless Tinker Bell is ready
  (below), with state 2 it calls FUN_001cf298 before the game-over / revive path (FUN_001ad810).
- The revive sequence FUN_001ad9a0 (step 2) calls it in the branch where its byte +0xc is clear.

On an area change FUN_00168ba8 clears the slots and the state (FUN_001c68f8) and the next area's load
(FUN_00168b10) preloads the summon again (FUN_001c66a0(1), which also sets `0x35feb8` bit 0). When the
area spawns its party members while a summon is active (FUN_0017d1b0), the member is listed as hidden
instead and the summon is created again (FUN_001c6710) in mode `"appear_area_start"` (only Peter Pan's
script has it). [V; I that the other summons then simply keep their default mode]

### Tinker Bell's revive (Peter Pan) [V decomp; the app's trigger condition (player Down with HP 0) is [inferred]; run in a harness with `KH2_SUMMON_KO`, WP-SM1]

While Peter Pan is out and byte 7 is set (FUN_001c69b0; a host-function wrapper FUN_0018eba8 returns it to scripts), a knock-out
goes to FUN_001ad8a8 (revive sequence of type 2, FUN_001ad378) instead of ending the summon. At its end
FUN_001adeb8 clears byte 7 (once per summon), creates Tinker Bell (objentry 0x4f4 = 1268) at the player and
sets her mode `"reraise"` (string at 0x37b880).

### The player during the summon [V unless marked]

- Motions: 0x8a (138) when the command runs, 0x82 (130) when the summon appears without the event; in the
  event the summon's script drives the player through the link (Stitch and Peter Pan use 252).
- Flags: +0x108 bits 0x104 during the arrival, 0x1 from the start of mode 3 to the end of the arrival
  (cleared together). [I for the meanings, see step 6]
- Commands: Limit commands of normal limits are refused while a summon is active (FUN_001d1ce0 returns 0
  for a non-summon row when the state is non-zero); limit prompts are refused while the word `0x35feb8` is
  non-zero unless the state is 2 (FUN_001d2460). Drive and Summon are refused while `0x35feb8`/`0x35fec0` are set.
- Camera: `FUN_0019bae8` (camera/target update) skips its target logic while the state is 1, and
  `FUN_0019af28` is true while the state is 1 for objects without +0x588 bits 4 and 0x40 [I: the event
  camera owns the view].
- Effects: FUN_001da368 hides effects without flag 0x800000 while the state is 1 [I].

## 4. What the summons do

- Fighting, healing, following: the friend AI (section 3) plus the summon's script. Neither is read in
  detail here; the scripts' battle modes have the same names as the party members' and the enemy library.
  [GAP: the per-summon behaviour; needs the friend AI engine and the group 10 hosts]
- Summon limits are offered by the summon's own script, not by the Limit menu: the Limit list (FUN_001d2320)
  skips summon rows (FUN_001d1fe0), and the party-limit offer (FUN_001d25e0) skips rows whose
  `Character` is 1. Stitch's main thread (event 1) calls, every frame, a routine that offers command 519
  through host 1.165 `offer_command` (slot 2, show flag 1) when the summon is valid, not in a limit (10.19),
  and the gauge timer of the object its helper R0544 returns is above 0 (1.84 + 10.45) [I: that object is
  the player]. The other summons do the same with their command
  [V for Stitch; I for the others: their scripts compare the same command ids].
- Running a summon limit (FUN_001b28a0, Execute 81): the `limt` row must be usable, which for a summon row
  needs the summon object present (FUN_001d1ce0 -> FUN_00175ba0 finds the object whose NeoStatus is the
  `Summon` byte); the cost is `FUN_001788d8(-Cost)`, so the timer loses `Cost x sstm[0x70]`; the limit task
  (FUN_001b2c10) marks each participant byte above 1, which includes the summon object (+0xb4c = 1,
  +0x10c |= 0x10000), loads `limit/us/<FileName>` and runs it. The limit task's script gets event 23 with
  the command id when a command without a usable `limt` row is executed (FUN_001b28a0 first branch): that is
  how the sub-commands (Stitch's 525..527 and so on) reach the running limit [V for the send; the
  receiver is the limit BAR's script thread at task +0x34, I].
- The summon scripts' event 23 handlers compare the same sub-command ids (Stitch: 519, 525, 526, 527; they
  call 7.26 and offer 527 again). Who delivers event 22/23 to the summon object itself was not found
  [GAP: the only direct sender of event 0x17 is FUN_001b28a0; senders of 0x16 none found by searching the
  decompilation for FUN_0016c620 calls with 0x16/0x17].
- Genie: "Drive!" lists the forms the player owns; choosing one (Execute 535..538, receiver 1) hands the
  command to Genie (the generic executor FUN_00177c20 -> FUN_0016b420 as event 11), whose event 11 handler
  plays motion 241 for any of 535..538 and continues per command; host 10.9 (FUN_00119c18) sets byte 7 of
  `DAT_00340c88` to the form, which FUN_001c6b28 reads back (host wrapper FUN_00119a40). Genie's limit per
  form (542, 544, 546, 548) and its finisher (543, 545, 547, 549, `Require` the per-form command) follow.
  [V for the menu and the event 11 compare; I for the order of the rest]
- Damage dealt: summon attacks are ordinary attack volumes of the summon's motion set, with `atkp` rows
  looked up as for any object (combat.rs). The attacker's stats come from the summon's status block,
  which the party-member setup fills [GAP: where a summon's strength comes from; there is no `plrp` row for
  NeoStatus 200..204 (checked: `kh2 table plrp` lists characters 1..13, 100 and 129..137)].
- MP: nothing summon-specific was found in the paths read.

## 5. Where it goes in the app

| file | function / item | change |
|---|---|---|
| `drive.rs` | `rules` | add the summon rules as pure functions: `summon_timer(t, level)` (= `FormTimer::start` with form 0), the mode-3 drain factor (ability block +0x68), `segments_drained(before, after)` -> experience, `summon_level_cap(charms)`, the experience add and level-up over `FormLevels` form 0, the damage-to-timer rule of FUN_001c60f8, the summon-limit cost `Cost x sstm[0x70]`. Each tested against the disc rows and, where possible, the oracle routines in `crates/kh2/tests/summon_oracle.rs` |
| `drive.rs` | `DriveState`, `drive_tick` | a gauge mode (1, 2, 3) instead of "in a form or not"; in mode 3 drain with the summon factor, feed the experience, end at 0 with Dismiss; never apply the Anti sign flip in mode 3 |
| `drive.rs` (or a new `summon.rs`) | new `SummonState` resource | index (byte 5), level and experience (byte 6, `DAT_00340e4c`), Genie/Tinker Bell byte (byte 7), state 0..3, the two object slots, the seen bits; a `SummonQueue` like `DriveQueue` for Summon / Dismiss requests |
| `sora.rs` | the command availability used by the menu (Drive's `check_enter` path) | Summon availability (section 2), Dismiss availability; refuse Drive and normal limits while a summon is active |
| `sora.rs` | motion requests | play 0x8a on Summon, 0x82 on an appear without event; the unhittable flag during the arrival (+0x108 0x100) |
| `hud_menu.rs` | `default_menu` / `base_entries` | Summon / Dismiss / blank in the list of FUN_001bc6e0, the SUMMONS sub-list (kind 7) from the owned charms, Genie's "Drive!" list |
| `hud_gauge.rs` | `DriveGauge`, `update_drive` | mode 3 is `DriveMode::FormB`; feed `points` = digit and the fill from the FUN_00155bf0 rule; the form-time fill (hud.md "Not ported") applies to modes 2 and 3 alike |
| `hud_gauge.rs` or `hud_party.rs` | new | the summon face panel (groups 0x9a, 0x9b, 0x9c at panel +0x69c, face at +0x1300) on mode changes into and out of 3 |
| `fx.rs` | new message like `StartSystemEffect` but bound to an owner | system effect 0x13 bound to the player, with "end" (FUN_001d9f48) and "kill" (FUN_001da028) |
| `hit-stop` code (`combat.rs`) | actor stops | stop id 2 on all actors except classes 0, 1, 5, 19 during the arrival |
| `enemy_ai.rs` (BDX host) | object spawn, `set_mode_named` | spawn the summon object with its script, switch modes by name (`appear`, `appear_skip`, `leave`, `reraise`); the group 10 hosts and 1.318 |
| `limit.rs` | `Party.summon_gauge`, `try_limit` | the summon gauge test reads the mode-3 timer; the cost goes to the timer; summon rows need the summon object present (NeoStatus) instead of a party member |
| `audio.rs` | positional SE | host 1.318 sound ids (bank `id / 1000`, sound `id mod 1000`) |

## 6. Staged plan

Each stage ends with `verify.ps1` passing and the named check.

1. **Rules only.** The pure functions of section 5 (`drive.rs` rules) with unit tests on the real rows and
   an oracle run of FUN_001a0480 / FUN_001a0420 / FUN_00155bf0 / FUN_001d4e68 (the last needs a fake
   inventory). Accept: tests green; `KH2_SUMMONLOG=1`-style log not needed yet.
2. **Gauge and command without the object.** Summon and Dismiss in the menu and as harness keys; cost,
   mode 3 start after the `sstm[0x88]` wait, drain, experience, end at 0 and Dismiss; HUD mode 3 (digit and
   bar groups 139/140/141, digits 144..153). Accept: a harness run logs "summon start", the timer per
   second, "summon end (timer)"; a screenshot shows the mode-3 gauge (look at it before claiming it).
3. **The object, skip arrival.** Spawn Entity1 with its model, motion set and effect file at the
   200..300 spot; actor stop id 2 during the wait; effect 0x13 on the player; the summon's BDX attached and
   switched to `appear_skip`, then `leave` at the end (object removed when the script ends it). Needs the
   party-member class basics. Accept: log of mode switches, the object present for the timer's length,
   no crash when the script calls an unknown host (logged).
4. **Friend AI and group 10.** Port the friend AI engine (0x10b000..0x11b000) and the group 10 hosts the
   summon scripts call, one summon at a time (Stitch has the fewest unknown hosts after Tinker Bell).
   Accept per summon: its script's host calls all answered (trace shows none `missing`), it follows the
   player and attacks.
5. **Summon limits.** Script offers (host 1.165 slot 2), the limit task with the summon as participant,
   cost from the timer, the limit BAR script and the sub-commands as event 23. Depends on the limit task
   work (limit.rs gaps). Accept: the prompt appears with the gauge above 0 and disappears in a limit.
6. **Event arrival and the rest.** The config option and seen bits; `appear` with the link and HUD layers
   hidden; Genie's "Drive!"; Tinker Bell's reraise; area re-entry with `appear_area_start`; the summon face
   panel. Accept: logs per path; visuals only after someone looks at them.

### Status (WP-7J: stages 1 and 2)

Stage 1 is done. The rules are pure functions in `drive.rs` `rules`: `GaugeMode`, `SummonTuning`
(`sstm` 0x70, 0x84, 0x88), `summon_timer`, `mode_drain_factor`, `segments_drained` and
`FormTimer::change` (FUN_001a0358 with the segment count in mode 3), `summon_level_cap`, `SummonExp::add`
(FUN_001d4ec8), `summon_damage_to_timer` (FUN_001c60f8), `summon_gauge_change` and `summon_limit_cost`
(FUN_001788d8), `segment_display` (FUN_00155bf0), `summon_available` and `dismiss_available`. Tests:
`summon_rules_on_the_disc_rows`, `summon_availability_and_dismiss`, and the oracle
`summon_gauge_matches_the_game`, which runs the game's FUN_001a0480, FUN_001a0358 (with the experience
add and its level-up loop), FUN_001d4e68 (with the charms set in a fake inventory), FUN_00155bf0 and
FUN_001c60f8 in kh2-ee and compares every result. The level-up notice routines FUN_001d4fa0 and
FUN_001b5528 are stubbed in that run.

Stage 2 is done without the summon object. `summon.rs` keeps `SummonState` (bytes 5..7, the experience,
the state, the sequence bit, the arrival wait, the object slots, the use counters, the SUMMONS list, the
cap) and `SummonQueue`. The command menu (`items.rs`) lists the owned charms' commands under Summon,
shows Dismiss (0x13c) on the second deck while the state is set (the earlier "map command" reading of
0x13c in `items.rs` was wrong: FUN_001ceda8 looks Dismiss up by 0x13c), and issues both to the queue.
A summon pays its cost, sets the state to 2, waits `sstm[0x88]` frames, then sets gauge mode 3. Mode 3
drains with the Summon Boost factor, feeds the experience, takes hits Sora lands as `DriveDrain x
sstm[0x70]`, and ends at 0 or on Dismiss (gauge mode 1). The HUD's drive gauge shows mode 3 with the
FormB groups, the digit and the in-segment fill of FUN_00155bf0. Drive is refused while a summon runs.

Inferred in stage 2: the arrival wait counts down one frame unit per tick; the end-at-0 test runs only in
mode 3 (the game tests +0x1b4 in any mode, and during the arrival the player is busy through +0x108 bit
4, which is not modelled); the state goes back to 0 when the leave finds both object slots empty (the
game does it in the object's destructor). Left for later stages (WP-7J2 below took up most of them): the companions' fade out and return
(the arrival does not wait for FUN_0017d470), the player's motions and flags, effect 0x13, actor stop
id 2, the arrival event and its config option, Genie's form byte (FUN_001c6a88 divides a sign-extended
random word; not ported, the generator is not stepped), the summon face panel, the limit task test of
Dismiss (FUN_001b3c00 reads no task), and summon limits (limit.rs refuses summon rows until a summon
object exists, because FUN_00175ba0 needs it). The summon level and experience are kept in
`SummonState`, not written back to `Progress` (it has no setter for them).

### Status (WP-7J2: stage 3, with parts of stages 5 and 6)

The arrival thread was decompiled (0x1c6260, ghidra-cli `DecompileAt.java`) together with the Summon case of
FUN_001cf030; section 3 matches it, with these details: the wait loop runs while the fade list is busy, while
`0 < wait` (the wait loses the frame unit `DAT_00356f40` per frame) or while `FUN_0017d470` finds a party slot
filled; motion 0x8a takes its blend from `DAT_0037c358`, motion 0x82 has blend 0; the mode names are the strings
at 0x37c000 (`appear_skip`) and 0x37bff8 (`appear`); FUN_001a5ea8 gets flags 0x10000 and the player's position.

What `summon.rs` now does, in the order of the game:

- Summon: motion 0x8a on Sora (`State::Reaction` until it ends), system effect 0x13 bound to his body
  (`fx::start_placed`, the handle kept as +0xb68), the cost, byte 5, the sequence bit, state 2, byte 7 (Genie:
  `rules::genie_form`, the port of FUN_001c6a88; Peter Pan 1), the player's +0x108 |= 0x104 (0x100: no hit
  reaches Sora while it is set), the arrival choice (`rules::arrival_event`, FUN_001c65f0, with the option
  word's bits 9..10 and the seen bits), the companions' fade-out over `DAT_0037bff0` frames (component `Away`),
  actor stop id 2 (below) and the use counter.
- The arrival: the wait (`sstm[0x84]` with the event, `sstm[0x88]` without) and the companions gone, then +0x108
  |= 1, gauge mode 3 and the summon object (`sumn` `Entity1`: objentry row, model, motion set, script; component
  `SummonObject` in the first free of the two slots). Without the event: effect 0x13 told to end, motion 0x82,
  the free spot (`rules::spot_start` = FUN_001a6068, oracle-checked; `rules::spot_find` = FUN_001a6198 with a
  floor under the point standing in for FUN_001a5c30 and no test for FUN_001a5bf8), script mode `appear_skip`.
  With the event: the effect killed, mode `appear`, state 1. Then state 2, +0x108 &= ~0x105, the stop lifted.
- The object: its script runs every tick through the enemy host (as the party's in `friend_ai.rs`); its motion
  requests are logged (`KH2_SUMMONLOG=1`) and not played (the friend AI's motion mapping is stage 4); the body
  loops motion 0. A hide by the script removes it (the destructor empties its slot; with both slots empty the
  state is 0). `SummonState::present` lists the objects' NeoStatus.
- Leave: the first object's script switches to `leave`; once both slots are empty the companions fade back in
  over `DAT_0037c064` frames to `DAT_0037c068`, and the sequence bit is cleared.
- Summon limits (stage 5, the rule side): limit.rs refuses a summon row unless its summon object is present
  (FUN_00175ba0 by NeoStatus) and the timer is above 0, and charges `Cost x sstm[0x70]` to the timer
  (`SummonRequest::Gauge`, FUN_001788d8 with the experience). The limit task is the resource `LimitTask`
  (`DAT_01d5d6b0`); Dismiss is refused while it exists (FUN_001b3c00).
- The summon level and experience are written back to `Progress` (`set_summon_level`, `set_summon_exp`).
- Actor stop id 2 during the arrival: the enemies (object types in the mask 0xfff7ffdc) stop their motion
  (`Animator::speed` 0) and no hit reaches them.

Tests: `summon_arrival_choice_follows_the_option_and_the_seen_bits`, `summon_genie_form_picks_an_owned_form`,
`summon_consts_and_spot_on_a_flat_floor`, the oracles `summon_spot_start_and_genie_form_match_the_game`
(FUN_001a6068 and FUN_001c6a88 with the form items in a fake inventory), `summon_spot_trig_is_sin_and_cos`
(FUN_00300278, FUN_003000d8) and `summon_arrival_choice_matches_the_game` (FUN_001c65f0), and limit.rs's
`summon_limits_need_the_gauge_not_a_partner`.

Not done, and why:

- (Closed in WP-F1 item 3.) The enemies' scripts now get no tick while `SummonState::stop` is set: `enemy_ai::ai_update`
  skips `Runtime::update` for every scripted enemy (the game's actor stop id 2 leaves the actor's update out, so its
  script thread does not advance), logging `bdx: enemy scripts paused (summon arrival actor stop id 2, 0x35fec0 bit 3)`
  once and `... resume` when the stop lifts; the trace of each enemy gets `summon arrival stop: no script tick`.
- (Closed in WP-F1 item 4.) The companions come back at FUN_0017d290's spot: `PartyMember::place` (party.rs) puts
  the member at the free spot the summon's own search finds (`spot_start` / `spot_find`, FUN_001a5ea8, with
  `ret_spot` 200..300 units from a point `ret_lift` above the player; the collision stand-ins of the summon's
  spot apply) facing the player (FUN_003003e8 -> FUN_0010be80), then fades in. Log: `summon: companions: <model>
  comes back at .. (N units from the player, FUN_0017d290: ..), facing him, fading in over 15 frames`. Seen in
  tt02 (`KH2_STATE` with item 25, `KH2_DRIVE_POINTS=5 KH2_SUMMON=3 KH2_SUMMON_AT=4 KH2_DISMISS_AT=12`): both members
  come back at the same spot 200 units from the player: the search runs once per member in the same frame and
  the first member placed is not an obstacle to the second [inferred; whether the game's FUN_001a5ea8 sees the
  just-created member was not read].
- The event that the `appear` mode and the thread wait for is never started (section 7), so the wait ends at once;
  the HUD layers 7..10 are not hidden. The fade list (FUN_0016ff50, FUN_0016f8e0, FUN_0016fb20) is not modelled.
- The free-spot collision tests FUN_001a5c30 and FUN_001a5bf8 were not read: a floor under the point (`World::ground`)
  stands in for the first, the second always passes [inferred].
- No friend AI, no motion requests (stage 4; WP-SUM4 below answers Stitch's group 10 hosts); summon limits are not offered by the scripts
  (host 1.165) and the limit task body is not run (stage 5); the summon face panel, Genie's "Drive!", Tinker
  Bell's reraise and the area re-entry (stage 6).
- Byte 5 and byte 7 are kept in `SummonState`, not written to `Progress` (no setters for them).
- An area change during a summon (FUN_00168ba8 -> FUN_001c68f8 clears the slots and the state, FUN_001c6710
  creates the summon again in the new area) is not handled: the object is not an area part and stays where it was.
- The game steps one LCG (`DAT_003716a0`) for many systems; the app keeps a copy for the summon only, started
  from the executable's initial word, which is 0 in the file. A multiplicative LCG at 0 stays 0, so the game
  must seed it somewhere [GAP: none of the 34 functions `mips_scan.py --addr 0x3716a0` lists stores anything but
  the next step; a store through a computed base or an overlay was not searched]. With 0 the port gives Genie
  the first owned form and the free spot its minimum radius.
- System effect 0x13 is element 19 of `00effect.bar` (`kh2 pax 00effect.bar`: effect 18, bone 67, flags 0x805,
  sound 180). WP-FX13 status: it starts and draws (`KH2_FXLOG=1`: `summon: system effect 0x13 (00effect.bar) bound
  to the player: started`, then `fx 19 objs 9 batches 7 verts 1176` per frame); the earlier "Effect::new refuses
  its particle program" was the app's DPX place lookup, fixed by WP-7Bc (docs/effects-damage.md "The summon
  arrival effect 0x13"). The `why` string in `summon.rs` that names `Effect::new` is stale.
- The free spot's direction is (sin, 0, cos) of the angle and the LCG word is read unsigned (both oracle-checked).

Harness (the runs behind the status, tt02, Donald and Goofy spawned by the map script):
`KH2_STATE=<file with "items": {"25": 1}, "summon": {"level": 1, "exp": 0}, "option_word": 11>` (11 = the default
option word with Summon Effects 0, so the arrival skips the event) `KH2_DRIVE_POINTS=5 KH2_SUMMON=3 KH2_SUMMON_AT=4`,
plus `KH2_DISMISS_AT=12` (Dismiss) or `KH2_SUMMON_AT=2 KH2_LIMIT=519 KH2_DISMISS_AT=4.5` (Ohana with Stitch out:
`limit: summon limit cost 100 x 12 = -1200 to the summon timer`, `summon: Dismiss refused (summon state 2, limit
task true)`). Without `option_word` the default Auto plays the event path on the first summon.

### Status (WP-SUM4: stage 4 for Stitch, the group 10 hosts)

Stitch's script (`N_HB040_BTL`, `kh2 bdx N_HB040_BTL`) calls 88 distinct host functions; its group 10 entries are
10.1, 10.3, 10.4, 10.12, 10.13, 10.16, 10.19, 10.32, 10.39, 10.41 and 10.45. All eleven now have a name and a
signature in `kh2-bdx` `names.rs` and docs/bdx.md (checked against the executable's table at 0x371810 by
`signatures_match_the_executable`), so the runtime decodes their arguments (the AI calls take the script's
context, whose word +4 is the object; 10.1 a vector). Every wrapper that names the AI reads object +0xb60
(FUN_0010c5f0), the AI object FUN_0010b780 made.

The summon object's script now runs through `summon::SummonHost`: the party members' host (`FriendHost`:
attack objects 2.10 / 2.11 / 2.12 / 2.13 / 2.22, effects, the target record) with the summon's own entries in
front of it. The AI object's fields the hosts reach are `summon::ai::SummonAi` (one field per game field, each
with its offset). Motion requests still go to the enemy host unremapped and are logged, not played.

Host calls of Stitch's script, as the harness run reaches them (`KH2_BDXLOG=1 KH2_SUMMONLOG=1 KH2_STATE=<items
{"25": 1}, summon level 1, option_word 11> KH2_DRIVE_POINTS=5 KH2_SUMMON=3 KH2_SUMMON_AT=4 kh2-game tt02`; the
`appear_skip` mode, the main thread and the battle start). No call of `N_HB040_BTL#summon0` is `missing` (before
this work package: 1.84, 1.272, 2.10, 2.22, 2.74, 10.1, 10.12, 10.19, 10.32, 10.45 were):

| host | class | what the app does |
|---|---|---|
| 1.84 gauges on the player | Impl | the player's status block handle (his +0x48c) |
| 10.45 status_f_1b4 (0x2f73b0) | Impl | the float +0x1b4 of that block: `DriveState::timer.left` (the mode-3 timer) |
| 10.19 in_limit (0x2f6e60) | Approx | object +0x10c bit 0x10000 = a limit task runs (`LimitTask`) [inferred: while a summon is out only summon limits can run, FUN_001d1ce0, and the limit task marks its participants, FUN_001b2c10] |
| 1.272 flag_7f8_4 (0x171cb0) | Impl | (object +0x7f8 & 4) != 0; set only by FUN_00186580 (callers FUN_001865a8 = host wrapper 0x171cd0, FUN_00175f10, FUN_001b25d0), never for the summon in the app |
| 1.165 offer_command | Stub | Ohana (519, slot 2) is offered every frame while the timer is above 0; summon limit offers are stage 5, the menu does not show it |
| 10.12 ai_slot_start (0x2f6d00 -> FUN_00117708) | Impl | the AI's 0x18-slot list at +0x20 (FUN_00117668 takes the first slot with kind < 0 and no effect) |
| 10.1 ai_walk_to (0x2f6a98 -> FUN_001175a0) | Approx | AI +0x630 bit 0 and the goal +0x640 kept; FUN_00117320, FUN_00193228, FUN_0017ecf0 (the class's motion toward it) not ported |
| 10.32 ai_stitch_32 (0x2f7138 -> FUN_00118aa8) | Stub | a goal (FUN_00116780), motion request 0xa1 and a record's action (FUN_00114280 -> FUN_0010ecb0): the class update is not ported |
| 2.10, 2.11, 2.12 | Impl | (the party host) Stitch's body attack object: `atkp` row, size, position each frame |
| 2.22 | Approx | (the party host) the anchor is the object's own effect file |
| 2.74 attack_callback_98 | Impl | the routine and argument kept; FUN_0018d430 (a clash of two attacks) is its only caller and never happens for the summon |
| 1.13, 1.11, 1.94, 1.262 | Approx | as for enemies (enemy host) |

Not reached in the harness (classed as the host would answer them): 10.3 / 10.4 (Stitch's vtable +0x28
FUN_001191a8 / +0x2c FUN_00119360: the player and his hand objects linked to Stitch for Ohana, stage 5) Stub;
10.13 (FUN_001195e8, event 17 code 1000: AI slots of kind 1 and attack objects of `atkp` 0x373 around the target,
stage 5) Stub; 10.16 (FUN_00117840: slot flags |= 0x40000000) Impl; 10.39 (FUN_0010ecb0(object, 0, 0)) Impl;
10.41 (FUN_001129f0: object +0xb7c |= 0x40000000) Impl. The battle modes (`mode_battle`, `btl_*`) never run:
the native AI starts them.

Tests: `stitch_summon_script_offers_ohana_while_the_timer_runs` (kh2-bdx disc: the script in `appear_skip` on the
mock world decodes every group 10 call, asks for AI slots, offers Ohana only with the timer above 0 and no
limit), `summon_ai_slots_take_the_first_free_one` and the oracle `summon_ai_slot_start_matches_the_game`
(FUN_00117708 with FUN_00117668 in kh2-ee over 200 random slot lists), the interpreter oracle with
`N_HB040_BTL` added (`cargo test -p kh2-bdx --test oracle`: 0 failures).

Not done: Stitch neither follows the player nor attacks. Both are native: Stitch's AI class FUN_001179c0 (vtable
0x354870) binds 7 action records at 0x354708 (stride 0x24) whose handlers are Stitch's own, FUN_00117cf8 (stand:
the wait +0x70..+0x74, motion request `DAT_00378870`), FUN_00117ea0 (a goal from FUN_00116780 and motion request
`DAT_00378874` / 0xa1, facing FUN_00116658), FUN_00118348 and FUN_00118730 (AI slots through FUN_00117708,
motions through FUN_0017e220 / FUN_0017e378, FUN_00118730 also an effect FUN_0016a570), FUN_00118db8 (motion 0xc9
and the player linked with 0xfe), chosen by the friend engine's record choice (FUN_0010fcd0, ported for Donald
and Goofy in `party_battle.rs`); the class update FUN_00116b48 (531 lines: the slot list, effects, FUN_0016aa18)
runs the slots and the goal. None of these were ported [GAP: read only as far as the calls listed here; the
summon object has no locomotion of its own in the app (party.rs owns the members' movement)]. So no hit by
Stitch's attacks is logged. (WP-SUM4b below ports them.)

### Status (WP-SUM4b: Stitch moves and attacks)

**What drives Stitch.** FUN_0010b780 gives NeoStatus 200 the class 0xe (+0xb6c), the AI object FUN_001179c0 and,
at object +0xb64, the update FUN_001155e0 in place of the members' FUN_00111070. FUN_001155e0 never calls the
members' follow (FUN_00110468, FUN_00110f90, FUN_0010f8d8): Stitch does not follow on the ground. Each frame,
while the object is free (+0x160 bits 4/8/0x10 or 0x40000000), it

1. places the AI once at the zero vector if +0x630 bit 0 is clear (FUN_001175a0; the script's 10.1 does it first),
2. wraps the position and the goal across the plane's edges (FUN_00116530) and stops the direction (+0x660 = 0),
3. in battle (FUN_001903d8), without a limit task (FUN_001b3c00), when the choices counted at +0x788 exceed the
   entry's +0x10, resets the count and with the entry's chance (+0x14) forces a record: the MP refill
   (FUN_00118730) when the player's MP is the lower fraction (`mp x maxhp < hp x maxmp`, the status block's +0x180,
   +4, +0, +0x184), else the blast (FUN_00118348); the record runs its phase 0 at once (`_PTR_FUN_003787a0` /
   `_PTR_FUN_003787a8` found by FUN_00114280);
4. otherwise runs the friend engine's choice FUN_0010fcd0 over the seven records and counts a change of record
   (or a record that is not continuing) at +0x788;
5. turns the yaw +0x64c toward +0x65c by +0x62c (FUN_0017bc30) and adds direction x speed (+0x660 x +0x66c) to
   the position +0x640;
6. every entry +4 frames (+0x674) calls the player's vtable +0x78 with (1, 0, 0) while his HP is below its maximum
   [GAP: the player's method at vtable +0x78 was not read; not ported].

The entry is the class block's (0x34ea00 + offset[0xe]) parameter entry with id 0x80 (FUN_00114228 over the list at
block +0xb0 / +0xb4), kept at AI +0x14. FUN_001179c0 sets +0x628 (`DAT_0037885c`, speed cap and arrival distance)
and +0x62c (`DAT_00378860`, turn per frame and arrival angle).

**The plane.** The position +0x640 is (x, y, depth, yaw): FUN_001175a0 and FUN_00116780 hold the depth at
`DAT_00378840` / `DAT_003787d0`; FUN_00116530 moves x by `DAT_003787bc` past `DAT_003787b8` / `DAT_003787c0` and y
by `DAT_003787c8` past `DAT_003787c4` / `DAT_003787cc`, position and goal together. FUN_00117320 makes a matrix of
it (rotation by the yaw less the motion's root rotation from FUN_0017ecf0, translation `+0x640 x DAT_0037882c`) at
AI +0x6c0, and FUN_00193228 gives the object that matrix as its parent (+0x790, +0x10c |= 0x40, +0x768 |= 0x1000);
with +0x10c bit 0x40 the object's movement FUN_00181e00 adds no gravity and no collision. [inferred] The parent is
the camera: the constant depth, the screen-sized wrap, the class update's placement type 3 through the camera's
inverse view (FUN_00116b48 with FUN_001068c0(1)) and the blast aimed from the camera's position all point there;
who reads object +0x790 was not found [GAP: `mips_scan.py --load-off 0x790` finds no load, the decompilation has
only the writers FUN_00193228 / FUN_00193248; +0x768 bit 0x1000 has no reader by the patterns `0x768) & 0x1000`,
`0x768) >> 0xc`]. The app puts the object at the camera-space point (x, y, depth) x `DAT_0037882c` (game camera
axes: x right, y down, z forward) through the game camera's transform, turned by the yaw about the camera's
vertical axis; the root rotation is taken as 0. So Stitch moves after Sora because the camera does.

**The records** (0x354708, stride 0x24; handler at +4, flags at +0):

| # | flags | handler | phase 1 (test) | phase 0 |
|---|---|---|---|---|
| 0 | 0x201 | FUN_00117cf8 stand | no limit task, +0x10c bit 0x800 clear, no flag-2 record feasible (FUN_0010ed20); continuing: the wait +0xcd4 above 0 | new: the wait from block +0x70..+0x74 (FUN_0010f648; in battle `2w + DAT_00378864`; yaw beyond `DAT_00378868`: times `DAT_0037886c`); motion 0 (FUN_0017e2d8 with `DAT_00378870`), stopped |
| 1 | 0x201 | FUN_00117ea0 move | as 0; new: not the current record (FUN_0010f7a8); continuing: not arrived (FUN_00116aa0) | new: a random goal (FUN_00116780(-1)) and motion 0xa1; the step toward the goal (direction, speed = distance x `DAT_00356f48` capped at +0x628), the facing (FUN_00116658) |
| 2 | 0x212 | FUN_00118348 blast | gates, +0x160 bit 0x40000000 clear; new: FUN_001180c0 (player HP below entry +8 of its maximum, FUN_0010f490, and an enemy in view, FUN_00118068); continuing: phase +0x634 not 4 | new: phase 0, goal case 6; phase 0: step and face, nearer than `DAT_00378878` -> wanted yaw 0, phase 1; phase 1: step, arrived -> phase 2; phase 2: snap to the goal, motion 0xcb, AI slots 0xc and 0xd (argument 0x13, flag 0x10), phase 3; phase 3: while motion 0xcb plays, its end -> phase 4; phases 2 and 3 run FUN_00118110 |
| 3 | 0x220 | FUN_00118730 MP refill | gates; new: player MP below entry +0xc of its maximum (FUN_0010f4d0); continuing: phase not 3 | as the blast with goal case 7 and `DAT_0037887c`; phase 2: motion 0xcc, FUN_001a0240 on the player's status (MP to its maximum, a charge ended), effect 10 on the player (FUN_0016a570), AI slot 0xb |
| 4 | 0x212 | FUN_00118db8 | never new; continuing while motion 0xc9 plays | motion 0xc9, the player linked with motion 0xfe (FUN_0017f160, FUN_0017e390) [not reached] |
| 5, 6 | 0x212 | FUN_00118ec0, FUN_00119068 | not read [GAP] | not read (both call FUN_001175a0 with the vectors at 0x3b4050 / 0x3b4060) |

FUN_00118110 (the blast): an attack object (FUN_0018bef0 with `atkp` 0x4ba, owner Stitch) kept at +0x620 with the
motion at +0x624; every object the target walk (FUN_0018f348) gives that is a valid enemy (FUN_00112b28 ->
FUN_00112ac0), not out of view (FUN_0019ada8) and has a hurt shape (FUN_00112d20, the nearest) is hit once
(FUN_0018c5f8, FUN_0018c7f0, the target's vtable +0x64) with the direction from the camera's position
(FUN_001071c8). The class update FUN_00116b48 frees the attack object (FUN_0018dc98) when the motion is no longer
+0x624 or it ended (+0x160 bit 2).

**Motions.** FUN_0017da30 uses the four-slot order only for a motion set whose header +0xc has bit 0 (FUN_0017d960 ->
FUN_001a7d70); otherwise the number is the slot. N_HB040_BTL's set has slots 0, 161, 201..206, 231, 241, 242: the
requests 0, 0xa1, 0xcb, 0xcc play at their own numbers. [inferred: the header bit is not read; a slot at the number
wins, the order is the fallback.]

**In the app** (`summon.rs` module `stitch`, run from `objects` for NeoStatus 200 while the script is not in
`leave`): `Consts::from_exe` (every address above), `records`, `goal` (FUN_00116780), `wrap` (FUN_00116530),
`toward`, `arrived` (FUN_00116aa0), `face` (FUN_00116658), `place` (FUN_001175a0), `can` / `run` (the records'
phases), `update` (FUN_001155e0 with `party_battle::choose` for FUN_0010fcd0). Hosts: 10.1 Approx (the root rotation
taken as 0), 10.32 Approx (FUN_00118aa8: its FUN_00116780 case is a register the wrapper leaves, taken as the random
branch), 10.39 also clears the current record. The blast writes `combat::Hit { atkp: 0x4ba }` for each enemy whose
nearest hurt volume projects inside the screen; the refill calls `magic::refill_mp`.

Inferred or left out:

- [inferred] The generator FUN_00106308 behind FUN_0010f420 / FUN_0010f438 / FUN_00106408 is party.rs's xorshift
  stand-in (one per summon object), not the game's.
- [inferred] The stand wait +0xcd4 counts down one frame unit per frame (as party.rs).
- [inferred] When FUN_0010fcd0 chooses nothing, the members' FUN_001103f8 runs; for Stitch it was not read: he
  stands.
- [inferred] The two-argument calls of FUN_001166d0 / FUN_00116728 in FUN_00116780's random branch write y too (the
  third argument register still holds +0x654).
- [inferred] "Out of view" (FUN_001a5db0 -> FUN_00146648, a frustum test of the object's +0x740 block) is the hurt
  volume's centre projecting inside the screen; "free" (+0x160) is always true; +0x10c bit 0x800 and +0x160 bit
  0x40000000 are never set.
- The AI slots' effects (FUN_00116b48 starts `FUN_0016a530(object, kind, ..)` and places them by type 1..3), the
  refill's effect 10 on the player and the 0x674 call are logged, not done [GAP].
- The attacker's strength for the blast: `enemy.rs` takes Sora's stats for an attacker that is not a party member
  (the summon's status block is the open gap above); the harness shows damage 0 with `atkp` 1210 and a blow.

Harness (tt04, the Dusk group `b_44`, `KH2_STATE` with item 25 / summon level 1 / option word 11,
`KH2_DRIVE_POINTS=5 KH2_SUMMON=3 KH2_SUMMON_AT=7 KH2_SUMMON_PARTY=1 KH2_SUMMONLOG=1 KH2_HITLOG=1`): `summon:
N_HB040_BTL#summon0 record 2 (handler 0x118348, FUN_001155e0's forced pick) starts`, `... blast (FUN_00118110,
atkp 0x4ba) hits <enemy>`, then `enemy: Dusk hit: atkp 1210 damage 0 ... Blow`; the once-a-second `plane (x, y,
1024) ... -> world ... (N units from the player)` line shows the plane position and the world point following the
camera. (`KH2_SUMMON_AT=4` there is refused: the player is airborne at 4 s.)

Tests: `stitch_follow_decision_and_blast` (the choice out of battle is stand or move, a move ends at its goal, the
plane wraps, the blast is the only choice with low HP and an enemy in view and fires from its spot until its motion
ends, the forced pick takes the refill when MP is the lower fraction), `stitch_class_from_the_executable`, and the
oracle `stitch_plane_rules_match_the_game` (FUN_00116530, FUN_00116aa0, FUN_00116658 and FUN_00116780 cases 0..7
in kh2-ee over 300 random rounds).

### Status (WP-SUM5: the next summon, Genie)

**A. The measure.** The SUM4b harness (tt04, `KH2_WAVE_GROUP=b_44`, `KH2_STATE` with the summon's charm / summon level 1 /
option word 11, `KH2_DRIVE_POINTS=5 KH2_SUMMON=<row> KH2_SUMMON_AT=7 KH2_SUMMON_PARTY=1 KH2_BDXLOG=1 KH2_SUMMONLOG=1`, 22 s;
charms: row 1 item 383, row 2 item 159, row 4 item 160) on the base of this work package. "Reached" is what each summon
object's script asked of the host in that run (the `first host call` lines of its tag, the `unimplemented host call` lines
for `missing`); "listing" counts the distinct host functions of the whole script (`kh2 bdx <model>`) and those without a
name in `names.rs`.

| summon | script | reached in the run (class) | listing: distinct / unnamed | unnamed group 10 |
|---|---|---|---|---|
| Chicken Little (1) | P_EX350 | 1.13 Approx, 1.87 Approx, 2.10 Impl, **2.21 missing**; then `script error BadAddress(4)` at 0x1638 and 0x1268 (the thread stops) | 107 / 24 | 10.5, 10.8, 10.17, 10.18, 10.20, 10.29, 10.31, 10.33, 10.34, 10.36, 10.37, 10.51, 10.59 |
| Genie (2) | P_AL010 | 1.13 Approx, 1.84 Impl, 1.94 Approx, 1.165 Stub, 1.272 Impl, 10.19 Approx, 10.45 Impl, **10.10 missing** (x2040, twice a frame) | 81 / 12 | 10.9, 10.10 |
| Peter Pan (4) | P_EX330 | 0.49 Approx, 0.51 Impl, 1.13 Approx, 1.21 Approx, 1.84 Impl, 1.87 Approx, 1.94 Approx, 1.165 Stub, 1.272 Impl, 10.19 Approx, 10.45 Impl, **2.75 missing** | 133 / 34 (Tinker Bell P_EX340: 37 / 1) | 10.17, 10.18, 10.20, 10.23, 10.25, 10.26, 10.27, 10.28, 10.30, 10.31, 10.36, 10.37 |

Chosen: **Genie**. It reaches one missing host (as the others), its whole script has the fewest distinct and unnamed host
functions (81 / 12 against 107 / 24 and 133 / 34) and only two unnamed group 10 entries (10.9, 10.10), and its script
runs without an error (Chicken Little's stops on a bad address after the missing 2.21).

**B. Genie's class** (`summon.rs` module `genie`, run from `objects` for NeoStatus 201 while the script is not in
`leave`).

- FUN_0010b780 gives NeoStatus 201 the class 0xf, the AI object FUN_001199f8 (0x18 bytes: FUN_001133c0, then +0x10 =
  0x3548a8, vtable 0x354910, +0xc = 2 records) and +0xb7c |= 0x800, but keeps the members' update (+0xb64 =
  `PTR_FUN_00378488` = FUN_00111070). So Genie runs the same decision loop as Donald and Goofy: out of battle
  FUN_001102c8 (the player relation), in battle FUN_0010f830 (summon class: only the player can be in danger),
  FUN_0010f8d8 (the emergency pass over flag 0x10 records) or FUN_0010fcd0, then the steer FUN_00110468. The app
  runs party_rules' follow point (FUN_00111670, party slot 0), follow step (row of the battle state) and steer with
  the class 0xf thresholds and the `prty` row of Genie's NeoMoveset (walk +0x110, run +0x114, turn +0x11c).
- The records (0x3548a8, stride 0x24): 0 = flags 0x201, FUN_00119af8 (always feasible; phase 0 FUN_001102c8, whose
  class 0xf branch raises a speed above the walk speed to the player's movement +0x860 plus `DAT_00378574` when
  that is faster); 1 = flags 0x10, FUN_00119b28 (feasible while the player's HP is below `DAT_003788d0` of its
  maximum, FUN_0010f490, which also targets the player; phase 0 FUN_0010c9d0(object, 1, player, 0): speed mode 0,
  object +0xb2c = the command of item 1 (FUN_001a1fc0(1): FUN_001a66e8 finds item 1 in 03system.bin `item`,
  loaded by FUN_00193610 -> FUN_001a66b8, its +0x10 is the command), +0xb30 = the target, motion request 9 through
  FUN_00175e20).
- The form: the first update (FUN_001112e8, +0xb7c bit 0x20000000 clear) calls the object's vtable +0x94
  (FUN_0010c630, at 0x352e1c and 0x35feac), which calls the AI's +0x18: Genie's FUN_00119a40 = FUN_00119c18 with
  FUN_001c6b28's command (byte 7 through the table at 0x37c036). FUN_00119c18 (also host 10.9 from the event 11
  handler with the "Drive!" command): AI +0x14 = the command, parts 2..6 hidden and part row + 2 shown (FUN_00192ce8
  / FUN_00192cd0 -> object +0x670's vtable +0x40 / +0x3c), byte 7 = row + 1 (6 when the command is not in the table).
  Host 10.10 (FUN_00119ca8) gives the limit command paired with AI +0x14; the main thread (event 1) offers it every
  frame in slot 2 through 1.165 while the timer is above 0 (as Stitch's Ohana).

Hosts reached by `P_AL010#summon0` in the harness (`KH2_STATE` with item 159 / level 1 / option word 11,
`KH2_DRIVE_POINTS=5 KH2_SUMMON=2 KH2_SUMMON_AT=7 KH2_SUMMON_PARTY=1 KH2_BDXLOG=1 KH2_SUMMONLOG=1`, tt04 `b_44`):
1.13 Approx, 1.84 Impl, 1.94 Approx, 1.165 Stub (summon limit offers: stage 5), 1.272 Impl, 10.10 Impl, 10.19 Approx,
10.45 Impl: none `missing`. 10.9 (event 11, Genie's "Drive!" commands) was not reached in that run (the menu entry came with WP-FX12b,
see "Genie's Drive!" below); it is Approx (the parts are not drawn).

Log lines of that run: `summon: P_AL010#summon0 first update (FUN_00119a40): byte 7 = 0 -> form command 0`, `... form
command 0 (FUN_00119c18): parts [2, 3, 4, 5, 6] hidden, part None shown (not drawn), byte 7 = 6, limit command 0`,
`... record 0 (handler 0x119af8) starts`, and once a second `... follow: N units from the player, speed mode m speed
s (walk 2.0 run 8.0)`: with Sora walking (`KH2_KEYS=12:W:3`) the distance rose to 356 with mode 2 (run 8.0), then
fell back to 270 and Genie stopped (mode 0). With no drive form owned (the harness state) byte 7 is 0, so the first
update shows no form part and 10.10 gives 0 (the script offers command 0).

Inferred or left out:

- [inferred] Genie flies (FUN_002f6320 picks his motions by NeoStatus): the app keeps him at the player's
  height and does no ground probe. WP-SM6 found the reader of +0xb7c bit 0x800: it is the chooser FUN_0010fcd0, not a flight
  test (see "WP-SM6" below); the flight itself rests on the flying motion table only [inferred].
- His motions: only record 1's motion 9 is played; the follow's motions (FUN_002f6320's flying set) are not ported,
  so the body keeps the idle slot the spawn chose (slot 1 in the harness; the screenshot shows him with his arms
  spread) [GAP].
- The model parts of FUN_00119c18 (object +0x670, vtable +0x40 / +0x3c with +0x104) are logged, not drawn [GAP: the
  part object at +0x670 was not read].
- Record 1's command (item 1's) is not carried out: the item use of the members (motion 9's trigger, party.rs) is not
  run for a summon; the log says `the command's effect is not carried out` [GAP]. Genie is not free until motion 9
  ends [inferred, as the members' item use].
- [inferred] When FUN_0010fcd0 chooses nothing the follow runs (FUN_001103f8's player relation).
- [inferred] The player's movement +0x860 is the length of Sora's horizontal velocity per tick (`Motor::vel`).
- The generator is party.rs's xorshift stand-in (`stitch::Rng`), as for Stitch.
- The fifth table pair (607, 608) has no byte 7 command in the table at 0x37c036 (it reads 0 there).

**C. Tests.** `genie_class_from_the_executable_and_its_decision` (the records, the form table both ways, the decision
out of battle, healthy in battle, in danger through the emergency pass, the class 0xf speed rule); the oracle
`genie_form_and_limit_match_the_game` (FUN_00119ca8, FUN_00119c18 with its part calls logged by stubs at FUN_00192ce8
/ FUN_00192cd0 and the game's FUN_001c6b48 writing byte 7, FUN_001c6b10, and FUN_0010f490 over 300 random rounds, in
kh2-ee); kh2-bdx `genie_summon_script_offers_the_form_limit_while_the_timer_runs` (P_AL010 in `appear_skip` on the
mock host: no group 10 call without a signature, 10.10 asked every frame, the limit 544 of the form command 536
offered only with the timer above 0 and no limit task); `P_AL010` in the interpreter oracle (0 failures).

### Status (WP-SUM6 / WP-SUM7: Chicken Little and Peter Pan run without a missing host)

Measured with the SUM5 harness (`KH2_STATE` = items 160 and 383, summon level 1, option word 11; `KH2_WAVE_GROUP=b_44 KH2_DRIVE_POINTS=5
KH2_SUMMON=<1|4> KH2_SUMMON_AT=7 KH2_SUMMON_PARTY=1 KH2_BDXLOG=1 KH2_SUMMONLOG=1 kh2-game tt04`, 20 s): no `P_EX350` or `P_EX330` line
among the `unimplemented host call` / `script error` warnings (the end-of-run table's only `Missing` rows, 1.161 and 1.83, are Goofy's
`P_EX030`).

- **Chicken Little**: the `BadAddress(4)` at 0x1638 / 0x1268 was the missing answer of host 2.21 (SUM6, FUN_001a5040 keyed buffers,
  `kh2_bdx::shared`: the script stores through the returned pointer). Test `disc.rs` (the with / without buffer pair, SUM6) reproduces
  it; the run now reaches 2.21 Impl, 2.10, 1.87, 1.13, 1.272, 10.19, 1.84, 10.45, 1.165 and stays running.
- **Peter Pan** (`P_EX330#summon0`, `appear_skip`): the two hosts left were **1.155** and **0.40**.
  - 1.155 (`obj_method_78`, wrapper 0x171c48: the receiver's virtual method +0x78 with (amount, part, flag != 0), result into word 0). The
    receiver here is the player (Obj 0x2000000), the arguments (5, 0, 0) follow the effect Tinker Bell's script starts on him (1.149). The
    method of the player is FUN_00186238 (docs/magic.md): with the +0x7f8 bit 4 clear it adds the amount to part 0's HP and holds it between
    the lowest and the maximum (FUN_001a0100), posts the panel's heal event (FUN_0013de10) and runs the death method only at 0 HP. App:
    `SummonHost::own` adds the amount with `magic_rules::healed` to `SummonAi::player_hp`, `objects` writes Sora's HP back after the
    script ran (log `summon: P_EX330#summon0 healed the player (host 1.155): HP 17 -> 20`, Sora HP 20/20 in the screenshot) and the call returns the new HP. Approx: the
    revive branch (bit 4 set, FUN_0017cee8), the panel event and the face are not run; a negative amount is a Stub (no summon script
    asks for it [inferred: only this call site was reached]).
  - 0.40 (wrapper 0x16db10, no `names.rs` row before): copies the vector argument raw into the 16 bytes behind FUN_001d9d50(handle), the
    effect instance's second vector, the same place 0.41 (0x16db50) fills with the angles of a direction (FUN_0017be00). The script calls
    it every tick after 0.38 on the effect it started with 1.21. App: the vector is taken as those angles (pitch, yaw) and handed to the
    friend host's 0.41 as the matching direction, which sets the element's rotation [inferred; the roll is dropped; what the instance
    reads the vector for was not traced].
  - Tests: kh2-bdx `peter_pan_script_heals_the_player_and_sets_the_effect_vector` (both calls decode, 1.155 gets (object, 5, 0, 0),
    0.40 a vector, no `Unknown`, no script error), `peter_pan_script_asks_whether_tinker_bell_is_ready` (SUM6); the interpreter oracle
    lists `P_EX350` and `P_EX330` and ends `0 failures`.
- Gaps: whether the 1.155 call sits behind the 2.75 test (the log shows 2.75 asked just before it in the same tick [inferred
  order]; the script's branch structure was not read further); Tinker Bell's own object (`P_EX340`) was not looked for in this run; the
  two summons' friend-AI classes (follow, attack choice) are still not ported: they stand where they were put.

### Status (WP-SUM8: the classes of Chicken Little, Peter Pan and Tinker Bell, the face panel, Genie's "Drive!")

**The classes** (`summon.rs` module `scripted`; FUN_0010b780 -> FUN_0011a450 / FUN_0011a048 / FUN_0011a230). Class 0x12 (Chicken Little,
NeoStatus 204), 0x10 (Peter Pan, 202) and 0x11 (Tinker Bell, 203) keep the members' update FUN_00111070 (Chicken Little's own update
FUN_0011a2e8 runs it first, then keeps the aim point AI +0x40: the target's position, else 200 units ahead, read by 10.5; +0xb7c |= 0x1000
by FUN_00113190). Chicken Little and Peter Pan have one record each (0x3549f0, 0x354948: flags 3, handler FUN_0011a590 / FUN_0011a118,
mode string `"battle"` at +0x20); Tinker Bell none. The handler's phase 0 is FUN_00186e70 (`Runtime::set_mode_priority`): it starts the
script's `battle` mode, and that is all the class decides. `battle` (R2be0 in P_EX350) tests `1.244` (FUN_001903d8), 10.36, 10.37 and no
event, picks `follow_player`, `ball` (the shot), `whistle`, `surprise` or `use_potion` by its own tests (the enemy walk 2.4, distances to
the player, the player's HP fraction 1.230 below 0.25, the player's lock-on 10.33) and queues `battle` again. FUN_00111070 runs the record
only while the script is idle (FUN_0010fcd0 returns at once while object +0x390, the running mode record, is set: `current_mode()` is
`Some`); out of battle it runs the follow (FUN_001102c8, whose class speed rule is for the classes 0xc, 0xf and 0x10 only), and the script's
`follow_player` mode calls host 10.20 (FUN_001102c8 and the vector copies) every frame. The steer FUN_00110468 follows. In the app:
`scripted::decide` / `follow_speed` / `aim_point` / `join_ready`, and the motion table of FUN_002f6320 per class (`scripted::Data::loco`: the
default {1, 2, -1, -1, -1} for 0x12 and 0x11, Peter Pan's {0xa5, 0xa1, 0xa2, 0xa3, 0xa4}) for the walk and run motions while motion 0 or
one of the table's plays.

**Hosts** (docs/bdx.md, rows added): 10.5, 10.8, 10.17, 10.18, 10.20, 10.22, 10.23, 10.25, 10.26, 10.27, 10.28, 10.29, 10.30, 10.31,
10.33, 10.34, 10.36, 10.37, 10.44, 10.51, 10.59, 7.19, 7.24, 7.25, 6.23, 2.4, 1.244, 0.37 (`kh2-bdx` `math.rs`). The summons' object
walk (2.4: the alive hittable enemies, handles `friend_host::enemy_handle`) answers the enemies' position, reaction and boss flags; the
object's own target record is `SummonAi::scripted.target` (the script names it with 1.160, clears it with 6.16; the runtime's dispatch hands
1.160 the context's object as a third word); 1.131 clones a record into the object's pool (`Obj::new_record`); attack objects (2.10 ..),
2.17 (hits the named enemy: `combat::Hit` with the atkp row), 1.83 bone_to_world and effects (1.87, 1.21, 1.22, 1.149 on the object's own
`.apdx`, registered with the effect library at the spawn) run as for the members (`friend_ai::step_attacks`). The script's own motion
requests (1.11, 1.12, 1.13 queue) are played on the body for every summon but Stitch. The bodies carry a `Fighter` (player team, not
hittable): the summons' motion triggers hit the enemies as any object's (Peter Pan's atkp 691 and 1135 in the harness).

**Run** (harness of SUM5, `KH2_SUMMON=1|4`, tt04, 30 s): Chicken Little goes `appear_skip` -> `battle` -> `surprise` / `follow_player` /
`ball` / `whistle` / `use_potion`; Peter Pan `follow_player` / `attack_empty` / `attack_small` / `attack_blow`; no `P_EX350` or `P_EX330`
line among the `unimplemented host call` / `script error` warnings except Chicken Little's `1.0 create_object` / `1.148` in `surprise`
(Gaps). Peter Pan heals the player through 1.155 while he follows. Chicken Little's `ball` shot made an attack object (atkp 1441) that met
the terrain between his muzzle and the Dusk (the wall cast in `friend_ai::step_attacks`), so no enemy was hit by it in tt04; the `whistle`
threads (2.20, one per enemy in range) run clean (`chicken_little_whistle_threads_run`).

**The summon face panel** (`hud_party.rs` `SummonFace`, `summon_panel`; `summon.rs` `face_feed`). FUN_00156d08 (the gauge mode into 3) starts
the task FUN_00156ca8 that waits for a summon object in party slot 0 (+0x588 bit 0x400000); FUN_00156fe8 then calls FUN_00156e30(panel, 0x9a,
0x9b): a node of the field file's group 154 (plays once, then 155 looping) whose parent is Sora's HP circle (the panel's +0x1980; the groups'
own offsets are (-71, -17)), and a child node with the summon's own `face` picture (`obj/<model>.apdx`, BAR types 0x18 / 0x19 fetched by
FUN_0016b998; group 3 of its SEQD). When the mode leaves 3 (FUN_00156c78) FUN_00157020 plays group 156 (0x9c, 17 frames). The party panels
give way while the summon holds the slot. Screenshot `local/sum8/face2.png` (Chicken Little, tt04, 14 s) [looked at]: his face in a circle
left of Sora's portrait, Donald's and Goofy's circles gone. Gaps: the HUD layer order of the nodes (Z 7000 / 13000) is the scene's add
order; FUN_00156d08's other objects (group 0x82 at +0xccc and the two text objects at +0x2c40 / +0x2dd0) are not built.

**Genie's "Drive!"**: the choice of a form command (535..538, receiver 1) reaches his script as event 11 with (player, command, record)
through FUN_00177c20 -> FUN_0016b420; `SummonRequest::Form(cmd)` (harness `KH2_GENIE_FORM=<cmd> KH2_GENIE_FORM_AT=<s>`) delivers it while Genie
is out (`summon: event 11 (Drive!: form command 536): handler ran`, then motion 241 and `form command 536 (FUN_00119c18) ... byte 7 = 2, limit
command 544` in the harness). Test `genie_drive_event_plays_the_form_motion_and_sets_the_form`. **The menu entry (WP-FX12b)**: `items.rs` follows FUN_001bc6e0. With the summon state 2 and Genie's NeoStatus 201 (`FUN_001c6918() == 2`, `FUN_001c6940() == 0xc9`; `SummonState::genie_out`) the second deck's third entry is command 605 (`Execute` 605, `SubMenu` 3 = the forms list, the disc row) in place of Party, whatever the story flags say, and FUN_001bcf10 refills list 3 (the manager's +0x180) with the commands of the table at 0x37c036 (read at run time: 535..538 for forms 1..4) for each of forms 1..4 whose item count is above 0 (FUN_001c6a20; the fifth form item is not in the loop). The refill runs after the first deck was built (FUN_001bc2b8's order: FUN_001bca70, FUN_001bc578, FUN_001bc6e0), so the deck's own Drive entry sees the same list while Genie is out. Choosing a form (`Execute` = its id, no sub-menu) issues `SummonRequest::Form(cmd)` (`items::summon_request`); `summon.rs` takes it only while `genie_out` and raises event 11. Status of the entry: the generic `FUN_001ba748` rule (empty list: unavailable); the entries are ready [inferred: FUN_001ba2c0 has no case for 535..538, the port's default]. Tests `second_deck_shows_drive_with_genie_and_issues_the_form`, `genie_drive_rows_and_table_come_from_the_disc`. Not looked at in a screenshot or played: Genie needs the summon harness, and the menu key path (H for the deck swap) was only run in the unit test. Gap: the entry's target record (`FUN_0018f490(+0xa20, Genie, 0)`, whose pointer goes with each entry as its parameter) is not modelled; the event's record is made by `summon.rs` as before.

**Not done / gaps**: (1) Chicken Little's `surprise` creates objentry 2098 (`P_EX360`, a scripted bouncing object, type 9) with host 1.0 and
places it with 1.148: the app creates no objects of its own (missing, logged once); Peter Pan's `appear` creates Tinker Bell the same way
(1.0 with objentry 1268) and sets her mode through 1.8 on her context, which the runtime applies to the calling script (cross-object mode
switches are not routed: 1.8 on another object's context needs `Call::RemoteSetMode`), so Tinker Bell does not appear and her class and
script (`reraise`, 1.277 = FUN_00178df0, the player's revive with full HP) are untested beyond the constants
(`scripted_classes_from_the_executable_and_their_decision`). (2) The FPS limit of Chicken Little (hosts 10.3 / 10.4 = FUN_0011ae60 /
FUN_0011b008: player flags, camera, `0x35d268`, the screen box target search 10.51 = FUN_0011a6d8, aim 10.59) is stage 5 and not ported (WP-SM2 below ports the hosts, not the limit task).
(3) 10.29's item command is not carried out (motion 9 only; WP-SM2 below carries it out); 10.22 / 10.30 read the blow mover only; 10.44 (mover flag 0x10) reads false;
7.19 stands the object the player attacked last for FUN_00119d60's list (+0x10c bit 0x20000 not modelled); 7.24 / 7.25 set +0xb7c bit 0x400
with no reader found [GAP]; 1.18 / 1.102 / 1.157 / 1.220 (the arrival event's link and its effect from the object's own BAR entry `"00"` of
type 0x16) are not answered; 2.57 / 6.51 answer 0. (4) Summon limits (stage 5) are untouched. (5) The summons' damage strength is still
the open gap above.

## 7. Unknowns (with what was searched)

- What starts the arrival event that FUN_001c6260 and the `appear` mode wait on: FUN_0014f198 only hides
  HUD layers; the appear routines of `N_HB040_BTL` and `P_EX330` call link_start (1.18), play_motion_with
  (1.29) and 1.318 (a sound). Candidate: the link (FUN_001becb8), not read.
- Who delivers events 22 and 23 to a summon's own script: FUN_0016c620 calls with 0x16 or 0x17 in the
  decompilation: only FUN_001b28a0 (to the limit task's thread).
- Summon stats (strength, max HP) and where the party setup reads them: no `plrp` row for 200..204;
  FUN_0010b780 and FUN_0010be80 not read past the AI selection.
- The friend AI engine (288 functions) and the per-summon AI classes: not ported. Stitch's (FUN_001179c0) was read
  as far as WP-SUM4's status lists (its records at 0x354708 and their handlers, the slot list FUN_00117708).
- Meanings of `DAT_00356f1c` bit 0x10, battle record +0x528 bit 1, the player's +0x108 bits 0x1 and 0x4,
  the summon's slot +0x54 override (FUN_001970b8 -> FUN_001ae798 with +0x9b8).
- The initial summon level (byte 6) of a new game: save initialisation not traced.
- Whether summons change the music or the camera mode apart from the event: not searched.
- The free-spot search (FUN_001a5ea8 -> FUN_001a6068, FUN_001a6198): not read.
- Which on-screen menu page the FUN_001bc6e0 list is.

## Corrections to other docs (for the orchestrator)

- `docs/hit-stop.md`, the list of stop callers: `FUN_001c6168` is the summon start (command 89 Genie and
  306 Peter Pan are its special cases), not a general "command cut-in", and the state it sets
  (`DAT_01d5e368 = 2`) is the summon state.
- `docs/player-data.md`, "Summons": `Entity2` is only preloaded (FUN_001d4da8); Tinker Bell is created by
  Peter Pan's script and by FUN_001adeb8. The summon start sequence is in section 3 here.

### Objects the summon scripts make (WP-EN3)

Host 1.0 `create_object` (0x170190 -> FUN_0016e8c0) in a summon script makes the objects the summon's rows name: Peter Pan's
`appear` routine (R16b9 of P_EX330) makes Tinker Bell (objentry 1268 = `Entity2` of his `sumn` row) and sets her mode `appear` with
host 1.8 on her context; Chicken Little's `surprise` thread (R1033 at 0x2171 of P_EX350) makes objentry 2098 `P_EX360` (the
`SpawnObject1` of his row; type 9, a script and effects but no model entry and no motion set), flags it (1.70 bit 1) and puts
it 60 units off its maker with 1.148. `summon.rs` (`SummonHost::own`, `act`'s `Act::Kid`, `objects`) builds them a tick later in no
summon slot (handles `KID_BASE` + serial), at the origin (the game's place record is zero) until a 1.148 moves them, carries out
the calls made on them (position, mode, hide, flags) once they exist, and removes them with the summon. Seen in `KH2_SUMMON=4`
(Tinker Bell: `appear` then `Hide` about 5.5 s later) and `KH2_SUMMON=1` (nine P_EX360 over the run). The three gaps of that
work package are closed by WP-EN4 below.

**WP-EN4 (the leftovers).**

- *Tinker Bell at the player (host 1.18).* Her `appear` routine (R0679 of P_EX340) calls `link_start(me, player, 0x4002, 0, 0, 0)`
  (BDX trace line `Func { group: 1, index: 18, args: [Obj(<her>), Obj(<player>), Int(16386), Int(0), Raw(0), Raw(0)] }`), then plays motion 231 and,
  once it is done, hides. FUN_001beb90 (0x170dd8) stores the player as her parent (+0x570), the halfwords +0x574 = 0x4002 and +0x576 = 0, clears the
  offset vector +0x540 (0, 0, 0, 1) and, as bit 1 of +0x576 is clear, the facing +0x550; FUN_0016b698(.., 3) gives her the link mover and
  FUN_0016b960 then reads her pose from +0x70 instead of +0x540. The link mover's own update was not read: [inferred] she stands at the parent's
  origin with his facing. `ai::Link::pose` places her there each tick after the class's steering (`objects`). Run `KH2_SUMMON=4`
  (state with item 160 and no option word, so the arrival event path makes Tinker Bell): `summon: P_EX340#kid class 0x11: 2338 units from the
  player` (she was made at the origin) and then `summon: P_EX340#kid linked to the player (host 1.18, kind 0x4002, flags 0): placed at Vec3(-2015,
  40, 1190)`, the player's point; she is gone with `hidden by its script` at the end of motion 231. Peter Pan's
  own `appear` link of the player to him stays a stub (`enemy-ai.md`). The screenshot of that run (7 s) shows neither she nor Peter Pan
  (she is invisible between her motions and Peter Pan stands off screen), so her position is the log's.
- *Chicken Little's child (P_EX360).* 1.114 `bind_object(ctx, 1736)` finds the object whose id is 1736 (FUN_0016b078): the `sumn` row's Entity1,
  Chicken Little's own object, the first of the summon's slots (`SummonEnv::main`, `main_object`; test
  `host_1_114_finds_the_summons_own_object_by_the_rows_entity`). 10.53 / 10.54 are the writer and reader of the effect list at an object's
  +0xb5c (FUN_0010c5b8 makes it, FUN_00136388 / FUN_001364d0 enter and find an (instance, generation) pair, FUN_00136430 drops one); the child
  spawns its effect with 1.21 on itself, enters it in Chicken Little's list, asks 10.54 and ends it with 0.51 (test
  `effect_list_registration_follows_fun_00136388`). The app keeps the list with the calling script [inferred: the list's readers, the
  object's end, were not read]. 6.49 (FUN_0019b088) is asked of each enemy the child's 2.4 walk hands out: true when the object's actor stop bit (+0x518),
  FUN_0019af28 (+0x588 bits 4 and 0x40 clear and the summon state 1), +0x108 bit 0x400 or the manager at 0x36f6a4 holds; the app answers true in
  the arrival only (test `host_6_49_is_occupied_in_the_arrival_only`). Run `KH2_BDXLOG=1 KH2_WAVE_GROUP=b_44 KH2_STATE=<item 383, option word 11>
  KH2_DRIVE_POINTS=5 KH2_SUMMON=1 KH2_SUMMON_AT=4 kh2-game tt04`: `bdx P_EX360#kid t369: first host call 1.114 FindObject [Impl]`, `... 10.53 effect_list_add
  [Approx]`, `... 6.49 object_occupied [Approx]`, `Func { group: 10, index: 54, args: [Obj(41943040), Raw(2063597569)] } -> Bool(true)`, then
  `first host call 0.51 effect_destroy [Impl]` and `1.28 Hide`: the child's script runs to its end. Still unmapped in that script (warnings in the
  run): 1.100 (0x173178, FUN_00173178: `sqrt(2 x / DAT_0037a538)` of a float, a fall time from a height) and 1.128 (the mover's virtual call,
  role unknown).
- *The revive's Tinker Bell.* The knock-out path is FUN_001cec00 -> the revive sequence FUN_001ad9a0 / FUN_001ad8a8 -> FUN_001adeb8 (clears byte 7,
  makes objentry 0x4f4 at the player with FUN_001c5e20, sets the mode `reraise`, 0x37b880). In the app the knock-out is `State::Down` of `sora.rs`
  (no game-over screen); `objects` makes Tinker Bell the tick the player is down with Peter Pan out and byte 7 set (`revive_due`), clears byte 7, puts her at
  the player in mode `reraise`, and her script's host 1.277 (0x172030 -> FUN_00178df0: FUN_00178d80 clears the dead bit and resets the mover,
  FUN_001d4a50, then the HP method with the status word +4 as the amount) gives him full HP and the app's stand-up
  (`sora.rs`'s own `State::Down` revival). The type-2 revive sequence is not run (WP-SM1 below says what it does). Peter Pan's own
  healing (1.155) kept Sora up in every 125 s harness run (waves of `b_44`); WP-SM1 knocks him out with `KH2_SUMMON_KO` instead.

**WP-SM1 (summon gaps, part 1).**

- *The link's break and routine (host 1.18, FUN_00169ee8, FUN_001becb8).* 0x170dd8 stores the 5th and 6th arguments on the link record when the routine is not 0 (record +0xc a context word, +0x10 the
  routine, +0x14 its argument: object +0x57c, +0x580, +0x584); `ai::Link` keeps them (`routine`, `arg`). The break FUN_001becb8 (called
  by FUN_001beb90 over a link already there, by host 1.102 `link_end`, 0x170e38, and by FUN_00169ee8) takes the object out of the parent's list (+0x760 /
  +0x764), clears the parent (+0x570), and, when the context word (object +0x57c) is set, runs the routine (+0x580) with the argument (+0x584) (FUN_0016c550 with one argument). The app
  does the same through `SummonAi::link_start` / `link_end`: the link is dropped, the object stays where the link last put it (FUN_0016a618 at its
  pose), and `objects` runs the routine after the tick with `Runtime::call_routine` (the game runs it synchronously inside the break; the app queues it, in order, for the end of the script's update, because the host cannot re-enter the runtime that calls it: an [inferred] deviation; log `summon: <tag> link to the player broken (FUN_001becb8): routine
  R<pc> runs with argument <n>`). Tests `a_link_break_hands_back_its_routine_and_argument` and `a_broken_links_routine_runs_with_its_argument` (a real routine run through `call_routine`). Not carried out: the facing a break gives an
  object whose flags bit 1 is clear (FUN_003003e8 of -(+0x60), -(+0x68), then FUN_0016a7f0) and the clearing of its +0x7f8 value 8 (set by the link start), because
  the app's summon objects have neither field; neither shows without a moving linked object. The automatic break of FUN_00169ee8 (flags bit 0 set and the
  parent either gone or without bit 0x8000 in its +0x160) is not applicable here: the only link a summon script makes in the app is Tinker Bell's, with flags 0,
  and a link with bit 0 clear never takes that path; a parent's +0x160 is not modelled for the player. Whether Tinker Bell's `appear` or any other
  summon script calls 1.102 or passes a routine was not found in the app's runs (her trace line shows routine 0, argument 0).
- *Host 1.277 (FUN_00178df0).* FUN_00178d80 clears the dead bit (+0x7f8 value 4), FUN_00183fe0 puts the player on the floor point under him
  (FUN_001a5c30, then FUN_0016a618), and FUN_0016b698(.., 0) gives back the normal mover; FUN_001d4a50 sets +0x9a4 bit 0 and +0xa70 bit 0 and clears
  +0x20 bit 0 of the object at his +0xb3c (the bits FUN_001d49c8 changes at his death, FUN_00178c88 -> FUN_001d4a20), and the HP method (FUN_00186238) adds
  the maximum. That method refuses while the dead bit (value 4) is set, which FUN_00178d80 has just cleared, so the heal fills a player with HP left the same
  way (`SummonAi::revive_player`, test `host_1_277_fills_the_players_hp_and_stands_up_only_a_downed_player`). In the app the dead bit and the mover are
  `State::Down` and the stand-up of `sora.rs`, and the +0xb3c object's bits are what `sora.rs` derives from the state (`hittable`): the stand-up is done only
  for a player who is `State::Down` (log `raised the player (host 1.277, FUN_00178df0): HP 0 -> <max>, he was down and stands up`), a player not down only has his
  HP filled (`he was not down`). The floor snap is `sora.rs`'s own ground contact.
- *The revive sequence (FUN_001cec00 -> FUN_001ad8a8 -> FUN_001ad378 type 2 -> FUN_001ad9a0 -> FUN_001adeb8).* What the sequence does besides making Tinker Bell:
  FUN_001ad378 sets 0x35d268 bit 1, stops the music (FUN_00137520: the BGM stream stop FUN_00136a68 with 0x1e0 and the channel ramps FUN_002c1a08 (it takes no
  argument; the 0x2d0 FUN_00137520 passes is ignored), the music state kept in 0x354ebc / 0x354ebe), opens a message window object (0xb0 bytes, FUN_00195898 / FUN_00195a78 / FUN_00195ae0, which
  becomes 0x35e3a4; step 1 of the thread sets 0x36f598 through FUN_002c38b8) and starts a thread (FUN_00102bc8 priority 1, 15000). The thread's step 2
  waits while that window exists (FUN_00196418) and until the fade object at 0x356b30 has its +0xfc at 4 (idle), clears 0x36f598 (FUN_002c38c8), puts the player on
  the floor (FUN_00183fe0), and FUN_001adeb8 makes Tinker Bell, clears 0x35d268 bit 1 and starts the music again (FUN_00137558: the saved music state back,
  FUN_00136aa0 starts the area's music record). (Corrected by WP-RV2, below: state 4 is the finished fade to black, started by the event the "window" really is.) The "fade" is therefore not a picture fade started here: the thread only waits for the fade object at
  0x356b30 to be idle (+0xfc == 4; its writers are FUN_0014a6f8 / FUN_0014a7c8 / FUN_0014a7f0 / FUN_0014a818, the screen fades of other code), so a fade
  already running (the player's death) finishes before Tinker Bell appears. What the player sees and hears, in order: system sound 0x13 (FUN_002c14a8), the
  music ramping out, a message window if the argument is a text, (no pad lock comes from 0x36f598, see below) the wait until the window is gone and the screen is not fading, then the player on the
  floor, Tinker Bell made at him and the music back.
  *Ported (WP-SM4, `summon.rs` `Revive` / `ReviveAct`, `audio_bgm.rs` `revive_stop` / `revive_restart`):* the thread as a state machine with the game's order
  (begin: flag 0x35d268 bit 1, music stop, sound 0x13, window, 0x354f70 bit 2; thread: step 0 moves on, step 1 sets the sound gate 0x36f598 and falls into step 2, which waits
  on window and fade, then clears it, puts the player on the floor, makes Tinker Bell, clears the flag, restarts the music and, at the thread's end,
  clears 0x354f70 bit 2). The music state is held in `AreaMusic::held` (0x354ebc kept in 0x354ebe, so the battle switch does nothing meanwhile) and both BGM
  slots fade to 0 over 0x4b0 driver frames (WP-SM4b: FUN_002c1a08 calls FUN_002be2e8(slot, 0, 0x4b0) for slots 0 and 1, which is IOP command 0x11 as `Driver::bgm_fade`), restarting the audible slot at full volume where it stopped. Sound 0x13 plays as a system sound. Each act is
  logged (`summon: revive: ...`), with `KH2_NO_MUSIC` / `KH2_NO_SOUND` too. Tests: `the_revive_sequence_runs_in_the_games_order`,
  `the_revive_waits_for_the_window_and_the_fade_with_sounds_held`, `a_held_music_switches_nothing_until_released`, `revive_effects_hold_and_release_the_area_music` (the Bevy glue).
  *0x36f598 is not a pad lock (WP-SM4b).* `mips_scan --addr 0x36f598` finds one reader, FUN_002c3228 (0x2c322c); no load with offset 0xf598 or a gp-relative form turned up. FUN_002c3228
  returns a free 0x40-byte request slot of the sound engine (the pool of docs/audio.md) and returns 0 while 0x36f598 is set; its callers are FUN_002c36b8 and FUN_002c37b8, which start
  positional sounds. So during step 2 no new positional sound is started; the player's pad is not touched by it, and the app's `Pad::lock` is not called for it (WP-SM4 did, wrongly; removed). The
  pad lock of a message window is FUN_00196280 in FUN_00195ae0 (when the message's flag bit 3 is clear), which the app does not open here. The sound gate is logged only (the app's PlaySe
  is outside this work package). Its other setters are FUN_00195d28, FUN_00196138 and FUN_001b9140 (clears: FUN_00195968, 001959f0, 00195e68, 00196348, 001b9140, 001b99a0, 002c1158, 002c11e0).
  *The music fade through the wait (WP-SM4b).* The original's ramp is 0x4b0 frames (5 s at 240 Hz) and the wait ends when the window is gone and the screen fade is idle, neither of which the app has,
  so the app's wait is 2 ticks and the music is back before the ramp has lowered it audibly (the restart path of `bgm_start` just sets full volume). No minimum hold is added: the code gives no number
  for the wait except those two conditions. Not ported, unknown: IOP commands 0x16 (FUN_002be5dc, args -100, -1, 0, 0, 0x4b0) and 0x27 (FUN_002bf5e4), FUN_002c1c18 -> FUN_002c0b98, and the stream stop
  FUN_00136a68 (handle 0x354e94, 0x1e0).
  *Not ported:* the message window (its text: FUN_001ad8a8 hands its own argument, which FUN_001cec00 uses as the player object, to FUN_00195a78 as the text;
  FUN_001d1160 only stores it, so no message id was found: searched FUN_001ad378, FUN_001ad8a8, FUN_001ad8e0, FUN_001ad810, FUN_00195898, FUN_00195a78,
  FUN_00195ae0; the instruction at 0x1ad8b4..0x1ad8d0 confirms a1 = a0), so no window opens and the window wait is always over; the 0x356b30 fade is not
  modelled by the app (the wait is always over). Both waits are parameters of `Revive::tick`, tested. The thread's per-tick function (FUN_001ad918, created
  by FUN_00102bc8) has no decompile; that it calls FUN_001ad9a0 each tick is [inferred] from FUN_001ad9a0's shape. [Resolved by WP-RV2, next bullet: there is no text, and the fade is the event's own.]
- *The revive's "message window" is a camera event, and the 0x356b30 fade is its fade (WP-RV2, research; nothing ported, the files are not this package's).*
  FUN_001ad378 builds the "window" with FUN_00195898(object, 0xf, FUN_00193f08(0)), FUN_00195a78(object, a1) and FUN_00195ae0(object): this is the event
  object of docs/events.md "Start and end" (the same constructor FUN_00196428 uses for a trigger's event), not a text window. Its three arguments:
  0xf is the event kind (`evtp` row of kind 0xf in `03system.bin`, bytes `0f 00 64 00 ..`: no start fade (byte +1 is 0), flags 0x64 = bits 2, 5 and 6; bit 3 clear, so
  FUN_00195ae0 calls FUN_00196280 and the two players lose control for the event, and FUN_00196170 gives it back; bit 2 set, so the field HUD stays; bit 4 clear, so no
  fade-in at the end); FUN_00193f08(0) is the event record: `FUN_001684c0(DAT_01c8a97c, 0x16, 0)`, the first BAR entry of type 0x16 of `00battle.bin` (DAT_01c8a97c is
  the second file FUN_00193610 loads), field +8 of its row; and the player object FUN_001cec00 passes is stored as the "text" word (FUN_00195a78 -> FUN_001d1160
  stores a mode 2 or 3 and the word at +0x40 / +0x44) and handed on as the fourth parameter of FUN_002232c8 into DAT_0036bd50, whose only accesses are the clear in FUN_00223180
  and that store (`mips_scan --addr 0x36bd50`: two hits, both writes): nothing reads it, so no message id exists to find [V]. On the disc `00battle.bin` has
  three type 0x16 entries, all tagged `0a`: `re_gameover-0a` (612 bytes), `re_mscomp2-0a` (396) and `re_deadboss2-0a` (2760); entry 0 is the one used. Its commands
  (size, opcode, operands): 0 the name, 0xf / 0x10 setup, **8 end frame = 0x15 (21 event frames, FUN_00226fb0 stores it at DAT_0036bcf8; FUN_00222668 ends the event
  when the event frame reaches it)**, 0x14, one camera record (0x13, FUN_0022b4f8, eight channels) with one cut (6: cut 0, frames 0 to 0x1e), 0x17 / 0x18 (FUN_00226940 /
  FUN_00226a98 at frames 0 and 0x15, not read further), 0x46 (the camera flags), 0x27 (FUN_00226b58 starts a thread, not read), two 0x20 (FUN_00226fc0, a colour change
  FUN_0014cb50 at frames 4 and 0x14 [inferred]) and **0x12 at frame 0xb with length 0x32 and mode 2**. FUN_00226830 mode 2 is `FUN_0014a7c8(FUN_0014a978(), 0x32, 1)`: FUN_0014a978 returns
  the object at **0x356b30**, so the "0x356b30 fade" is this event's screen fade to black. FUN_0014a7c8 (disassembled: it stores state 3 at +0xfc, the flag at +0xf4 and jumps
  to FUN_0014a710 with the length untouched in a1) makes the counter 2 x length = 100 and FUN_0014a818 takes DAT_00356f44 (1.0, read from the executable) off it each
  frame while the alpha goes from 0 to 0x80; at zero the state becomes 4 and stays (black). FUN_001ad9a0 step 2 waits `+0xfc != 4` with `return 1`, i.e. **until the
  screen has faded to black** (the earlier reading "waits for the fade to be idle" had the sense reversed: state 4 is the finished fade-out, state 0 the idle one;
  a run without the event's fade would wait forever). The window wait (FUN_00196418: DAT_0035e3a4 != 0) is the event object's life: it ends when the event does (end frame 21
  event frames = 42 game ticks, the event frame being half the tick counter, FUN_00222728 and 0x37ec50 = 0.5) and the object's end states run (FUN_00195c08
  states 5 to 9 -> FUN_00196170, which also stops the event's own sound gate by FUN_001959f0). Event frame 0xb is game tick 22, so the fade ends at about tick 122
  (100 ticks after tick 22); at 60 ticks a second that is 2.0 s after the revive begins [inferred: that FUN_0014a818 runs once per game tick; the event frame to
  tick ratio is the one docs/struggle.md already uses]. So **Tinker Bell appears after about 2 s on a black screen with the pad locked**, not two ticks later. What lifts the black
  afterwards was not found: the callers of the fade-in FUN_0014a7f0 are FUN_001047a8 (every area start), FUN_00223a88 (the next event's start, which fades 0x356b30 and
  0x3565e0 in at once, only while an event runs and FUN_002c17e0 is set), the event end FUN_00196170 / FUN_00195ff8 (other faders, 0x3564d0 and the two of FUN_0014a958 /
  FUN_0014a968), the movie player FUN_00225860, FUN_00227280, FUN_001b65e0, FUN_0015d1a8 and FUN_002de1c8; Tinker Bell's script host 0.31 / 0.32 / 0.88 move 0x3564d0, not 0x356b30.
  [GAP: which of these returns 0x356b30 to clear in the revive; searched `mips_scan --jal 0x14a7f0`, `--jal 0x14a978`, `--jal 0x14a6f8` (only FUN_0014ab80 resets it).]
  For the port (code outside this package: `summon.rs` `Revive::tick`, `sora.rs`, `event_player.rs`, `audio_bgm.rs`): the window wait is the 42 ticks of the event, the fade wait ends at tick
  122, the pad is locked from the start to the event's end, and the camera plays the first 30 event frames of `re_gameover-0a`'s cut; the screen goes black from tick 22 over 100 ticks.
  Tests: none (nothing ported); the claim rests on the disc dump of the three events (a scratch BAR reader) and the addresses above.
- *The pad lock and the fade are wired, and the fade is WHITE (WP-RV4).* `sora::tick` locks the pad (`Pad::lock`) while `SummonState::revive_pad_locked()` (= `Revive::pad_locked`, ticks 0 to 42), and
  `summon.rs` draws a full-screen quad (`ReviveFade`, `paint_revive_fade`) with `Revive::fade_alpha()` (0 to 0x80 over ticks 22 to 122). The colour corrects the "fade to black" wording above: FUN_00226830 mode 2 passes the flag 1 to
  FUN_0014a7c8 (case 2 -> label 0x2268cc sets a local to 1), which stores it at +0xf4, and FUN_0014a710 reads +0xf4: non-zero gives `FUN_0014af90(0xff, 0xff, 0xff, 0)` and word 0x48, zero gives black and 0x44. So the revive's
  fade, the same object 0x356b30, is white. The blend word is not decoded, so plain alpha blending is assumed [inferred]. The quad is clear again once the thread ends (the gap above: what lifts the fade is unknown). Tests
  `the_revive_lock_and_fade_reach_the_screen`, `the_revive_waits_for_the_window_and_the_fade_with_the_sounds_held`.
- *The fade's blend, its alpha curve, what does not lift it, and the message text (WP-RV5, `revive_fade.rs`, `summon.rs`).*
  *Blend words.* The word FUN_0014a710 stores at +0x90 of the fader object (FUN_0014b390) is the GS register ALPHA_1: the draw packet FUN_0014b408 copies the object's words from +0x20, and FUN_0014b000 puts the register id 0x42 (ALPHA_1) at +0x98, right behind the +0x90 data word. The register's fields are A, B, C, D (two bits each from bit 0; A, B and D pick 0 source colour, 1 destination colour, 2 zero; C picks 0 source alpha, 1 destination alpha, 2 the fixed value) and the result is (A - B) * C / 128 + D. So 0x44 (A 0, B 1, C 0, D 1) is the plain alpha blend `Cd + (Cs - Cd) * As` and 0x48 (A 0, B 2, C 0, D 1) is the additive blend `Cs * As + Cd`. With the flag 1 the fade is white and additive: it adds white to the whole picture, the HUD included, and clamps, so it reaches full white before a plain alpha blend of the same alpha would. The flag 0 (black) uses the alpha blend. The port draws the revive's fade as a white quad with the additive blend (`ReviveFadeMaterial`, the blend of `hud_gfx::HudMaterial`, the HUD shader) on the keyhole wipe's layer (above the HUD and the menus [inferred from the task priorities: the fader thread FUN_0014aa00 runs at 0x19640 = 104000, the field HUD's tasks at 8000, docs/hud.md; the keyhole wipe and the 0x3564d0 fader come after, FUN_0014aa90 at 330000]). Test `the_blend_words_decode_to_alpha_and_additive`; the screenshots below.
  *Alpha curve.* The ramp is exact now, not linear [V]: FUN_0014a710 makes the counter 2 x 0x32 = 100 and the ratio 128 / 100 (DAT_00379130 = 128.0); each FUN_0014a818 run in state 3 takes DAT_00356f44 (1.0) off the counter and stores `0x80 - (char)(int)(ratio * counter)`, and at counter 0 or less it stores 0x80 and sets state 4. In single precision that is 2 after the first run, 0x40 after 50, 0x7f after 99, 0x80 after 100 (`revive_fade::alpha_after`, test `the_fade_alpha_follows_the_games_counter`). State 4 has no case in FUN_0014a818, so the fader keeps drawing at alpha 0x80: full white.
  *What lifts the fade: nothing in the revive.* The fader is at state 4 when the thread ends (FUN_001ad9a0 case 2 returns once the event object is gone and +0xfc is 4, then FUN_001adeb8 ends it). The only code that can take 0x356b30 out of state 4 is a fade-in call on it (state 1 through FUN_0014a7f0, or a new state 3 through FUN_0014a7c8 with the flag 0). The complete list of callers that reach 0x356b30 (`mips_scan --jal 0x14a978` and `--jal 0x14a7f0`, all hits read): FUN_001047a8 (every area start, fade-in length 0, flag 0); FUN_00223a88 (the event command thread: before its first command it fades 0x356b30 and the object of FUN_0014a948 in, length 0, flag 0, only when an event object exists and FUN_002c17e0 is set; DAT_01e33ca0 is set by FUN_00195e68 when an event starts and cleared by FUN_00195968 / FUN_001959f0 only when the event kind row's flag bit 0 is set, which the revive event's row 0x64 does not have); FUN_00226830 (the event command 0x12, modes 0 to 2 and 5); FUN_002226d0 (the event skip, FUN_00222820: a fade out to black); FUN_001ad9a0's own case 6, FUN_001adc18 (a fade out to black for the game-over types 0 and 1, never the type 2 revive). Everything else in the earlier list (FUN_001b65e0, FUN_002de1c8, FUN_0015d1a8, FUN_00195ff8, FUN_00196170, FUN_002233d8, FUN_00227280, FUN_0016d880 / 0016d958, FUN_0013f078 / 0013f1a0 / 0013f2e8) works on other faders (0x3564d0, 0x3565e0 and the FUN_0014a928 / 938 / 948 / 958 / 968 ones); the stores to +0xfc elsewhere are other objects' fields. The revive's own event (`re_gameover-0a`, entry 0 of the type 0x16 entries of `00battle.bin`, dumped from the disc) has exactly one 0x12 command (frame 0xb, length 0x32, mode 2) and its commands after the end frame 0x15 never run; its other commands are 0x20 at frames 4 and 0x14 (FUN_00226fc0 -> FUN_0014cb50, a colour parameter 0.1 and 1.0 handed to a thread, not read), 0x27 at frame 0 (FUN_00226b58, a small effect record and a thread at 0x226b10, not read) and 0x17 / 0x18 at frames 0 and 0x15 (FUN_00226940 / FUN_00226a98, the camera filter and a camera object, not the fader). So by the code the screen stays full white after Tinker Bell appears, until the next area start, the next event's start (its prelude fade-in, which re-colours the object black for the two hold frames and the two fade frames it runs) or an event's 0x12 command. That reading is [inferred from the code only, not observed]: a PCSX2 watch of 0x356b30 + 0xfc after a revive would settle it. The app does NOT keep the white: `Revive::fade_alpha` is 0 once the thread ends, because the app has no later fade-in call to lift it and a permanent white screen would be worse than a missing lift [conservative; the observed original, which the user's play-test reports outrank, may differ]. [GAP: the observed lift; searched every caller above and the stores to +0xfc and to the +0xf4 / +0xf8 fields through `mips_scan --store-off 0xfc`.]
  *The message window text: there is none.* The "window" is the kind 0xf camera event (the WP-RV2 bullet): FUN_001ad378 hands FUN_00195a78 the player object as its second argument (the second argument of the call; FUN_001ad8a8 passes its own argument through, FUN_001cec00 uses the player object there), FUN_00195a78 -> FUN_001d1160 stores it at +0x40 / +0x44 of the event and FUN_002232c8 hands it on into DAT_0036bd50, which nothing reads (`mips_scan --addr 0x36bd50`: two writes, no reader). No message id and no text bar entry is read anywhere on the path, and the three constructors take no message argument. "It has no text" is the result; the act `ReviveAct::WindowSkipped` only logs it (`the "window" ... is the kind 0xf camera event; it has no message text`).
  Run (Peter Pan charm is item 160, `{"items": {"160": 1}, "summon": {"level": 1, "exp": 0}}` as `KH2_STATE`): `KH2_STATE=<file> KH2_WAVE_GROUP=b_44 KH2_DRIVE_POINTS=5 KH2_SUMMON=4 KH2_SUMMON_AT=5 KH2_SUMMON_PARTY=1 KH2_SUMMON_KO=10 KH2_NO_SOUND=1 KH2_NO_MUSIC=1 KH2_SCREENSHOT=<png> KH2_SHOT_AT=11.4 kh2-game tt04` logs `revive: tick 60, pad locked false, white fade alpha 0x31`, `tick 80 ... alpha 0x4b`, `tick 100 ... 0x64`, `tick 120 ... 0x7e`. The screenshot at 11.4 s (about tick 55, looked at) shows the town, Sora, the Dusks and the HUD washed with a faint white; the one at 11.9 s (about tick 100, looked at) shows everything, the command deck and the Peter Pan / Sora faces included, almost white. Nobody has listened or watched.
- *Ported (WP-RV3, `summon.rs` `Revive`).* `Revive::tick` keeps a clock from the start of the revive: the camera event (`window_open`, `pad_locked`) lasts 42 ticks, the black fade (`fade_alpha`, 0 to 0x80, linear [inferred]) runs from tick 22 for 100 ticks, and the thread makes Tinker Bell at tick 122 (`fade_black`), as the previous bullet found. Tests `the_revive_sequence_runs_in_the_games_order`, `the_revive_waits_for_the_window_and_the_fade_with_the_sounds_held`. Not wired: `sora.rs` / the HUD do not yet read `pad_locked` / `fade_alpha` (outside this package), so in the app nothing is drawn black and the pad is not held. The gap stays: what lifts the black afterwards (the fade alpha returns to 0 when the thread ends). No harness PNG of the revive was made.
- *The revive's Tinker Bell in a run.* `KH2_SUMMON_KO=<seconds>` (new, a harness aid) knocks the player out (HP 0, `State::Down`) once while a summon is out.
  Run: `KH2_STATE=localcc1\state-sum.json KH2_WAVE_GROUP=b_44 KH2_DRIVE_POINTS=5 KH2_SUMMON=4 KH2_SUMMON_AT=5 KH2_SUMMON_PARTY=1 KH2_SUMMONLOG=1
  KH2_SUMMON_KO=10 kh2-game tt04` (WP-SM4, with `KH2_NO_SOUND=1 KH2_NO_MUSIC=1 KH2_SCREENSHOT=<png> KH2_SHOT_AT=14`) logs `KH2_SUMMON_KO: the player is knocked out (HP 20 -> 0)`,
  `the revive sequence starts (FUN_001ad378 type 2)`, then in this order `revive: flag 0x35d268 bit 1 set`, `music stops`, `system sound 0x13`, `the message window ... is
  not opened: its text is not found`, `0x354f70 bit 2 set`, 38 ms later (two ticks) `Tinker Bell (objentry 1268) made at the player in mode "reraise" (FUN_001adeb8)` with
  `pad locked`, `pad released`, `put on the floor`, `flag cleared`, `music restarts`, `bit 2 cleared`, and 21 ms after that `P_EX340#kid raised the player (host 1.277):
  HP 0 -> 20, he was down and stands up`. The screenshot 4 s later (looked at) shows Sora standing with HP 20/20, Peter Pan beside him, Dusks around and no message
  window; Tinker Bell is not in it. The music itself and the sound were not heard (nobody has listened; the run had audio off, the stop and restart are logged).

**WP-SM2 (summon gaps, part 2).** Every item of the package is answered below; what was not carried out says why. Addresses are the executable's.

- *Hosts 1.100 and 1.128 (Chicken Little's child P_EX360).* Both are reached only inside a limit (the child's main thread asks 10.19 first), so the run is
  `KH2_LIMIT=529 KH2_SUMMON=1 KH2_SUMMON_AT=2 KH2_SUMMON_EVENT=22:limit@5` (below). 1.100 (0x173178 -> FUN_0017bee0) is `sqrt(2 x / DAT_0037a538)`, the time a body takes to fall
  `x`, `DAT_0037a538` = 20/49 (0x3ed0fac6) read from the executable; the table gives it no name or signature, so it reaches the host as raw words
  (`Call::Unknown`), and `ai::fall_time` answers it [Impl]. The EE's `sqrt.s` of a negative operand is the root of its absolute value in the emulator;
  the oracle `air_launch_and_fall_time_match_the_game` compares FUN_0017bee0 with the port over 400 heights of both signs. 1.128 (0x171298, `MoverMethod`)
  calls the object's mover (+0xc) at vtable +0xc with the float: for the mover FUN_00179868 gives every new object (kind 0, vtable 0x35cbb0, a real
  entry at 0x35cbbc = FUN_001796e8) that is FUN_0017b1e0(speed) and then FUN_0016b698(object, 1), the airborne mover. FUN_0017b1e0 leaves +0xd0..+0xdc
  (`ai::launch`: an upward speed gives +0xd8 = 0, +0xdc = -speed, +0xd0 = 0, +0xd4 = the fall time of it; a negative one +0xd8 = speed, +0xd4 = +0xdc = 0,
  +0xd0 = `sqrt(speed x DAT_0037a4f0 / DAT_0037a4f4)`) and FUN_0017b1b0 the mover fields (+0x514 = 1, +0x100 = -1, +0x10c bit 0 cleared, +0x104 = 5,
  +0xcc = 0): the same oracle runs FUN_0017b1e0 and compares all of them. The app records the launch (`SummonAi::air`) [Approx]. The mover's update
  (vtable 0x35cbe0 +0 FUN_0017b328, +4 FUN_0017b410, +0x18 FUN_0017b440: they integrate +0xd0..+0xdc, FUN_0017b328 adds the object's height to +0xd8) is not
  ported, so the child does not fly [GAP: the flight is the engine's]. Run lines: `bdx P_EX360#kid t271: first host call 1.100 ? [Impl]`, `Unknown { group: 1,
  index: 100, args: [1119281056] } -> Float(21.16696)`, `MoverMethod { obj: 42012672, value: 91.43677 } -> None`.
- *The limit task, and what stands in for it.* FUN_001b28a0 (Execute 81) makes the limit task (FUN_001b2c10) for a command with a `limt` row; the task loads
  `limit/us/<file>` (FUN_001b2e08) and that BAR's own script sends the participants their events: 22 (the limit starts, with the limit's attack object) and 23
  (a sub-command). Neither the task body nor that script is ported, so no summon limit ran in the app before. Since WP-SM5 the sender is `limit.rs` (`LimitScriptEvent`: event 22 with the task at the start, event 23 with the command id for a sub-command, WP-LM1) and the receiver is `summon::objects`: an event with `Participant(n)` goes to the summon object with neo id `n` (200 Stitch .. 204 Chicken Little) by `Runtime::event`, 22 with `LIMIT_ATTACK` [inferred] and 23 with the command id. An object with no script (the `rt` none branch) never receives it, matching `FUN_001b3118` skipping a participant whose `+0x484` is zero. `LimitScript` receivers are the BAR's own script and are not delivered. The harness aid
  `KH2_SUMMON_EVENT=<event>:<argument>[@<seconds>],...` (a time carries on to the events after it) delivers events to the summon's own script by
  `Runtime::event`: `22:limit` starts the limit the command names (the handlers call their own start routine: Chicken Little's R1058 calls the FPS mode,
  Peter Pan's R09cb his command 468), `23:519`, `23:525` .. `23:527` are Stitch's. `limit` is the constant `LIMIT_ATTACK`: the scripts read two things of the
  attack object (hosts 2.48 `hit_owner`: its word +0xc, 2.49 `attack_word_10`: the words from +0x10) and store them as the limit's player and
  summon records, so the harness answers the player and the summon for that one handle; 2.60 (the follow-up window, `> 0` gates Stitch's calls of 7.26)
  reads 1. All three are [inferred] from the handlers and exist only for this aid. With `KH2_LIMIT=<command>` (limit.rs) the limit task flag is on, so
  `10.19` is true while it lasts (`time` 600 frames: events must arrive within about 10 s of it).
- *The class hosts 10.3 / 10.4 (0x2f6b28 / 0x2f6b58).* They call the AI object's vtable +0x28 / +0x2c, which differs by class (vtables read from the executable):
  Genie 0x354910, Peter Pan 0x354978 and Tinker Bell 0x3549b8 hold FUN_00114170 (FUN_0010ecb0(object, 0, 0): the action cleared; a link the object has broken,
  FUN_001beb00 -> FUN_001becb8) and FUN_001141c0 (a return) [Impl, `SummonAi::base_enter`]. Chicken Little (NeoStatus 204, class FUN_0011a450, vtable 0x354a20) holds
  FUN_0011ae60 and FUN_0011b008, the FPS mode: the base part, AI +0x20 bit 0 set, AI +0x24 = `DAT_00378904` (100), AI +0x28 = 0, the object linked to
  the player with the halfwords 0x4000 and 4 (FUN_001beb90(object, player, 0x4000, 4), offset zero: at his origin with his facing, `ai::Link::pose`), and the way out breaks the
  link and clears the bit and the target record [Approx, `fps_enter` / `fps_leave`]; not carried out: the player's +0x108 bit 0x100 and +0x10c bit 0x4000, his mover
  0x3e (FUN_0016b698), the camera object (FUN_0013e528(3), mode 9 through FUN_0014e870 with the aim vector), the HUD interpolators (FUN_001abf40) and `DAT_0035d268`
  bit 0x4000: the camera and the player's movement are not this module's files. Stitch (NeoStatus 200, vtable 0x354870) holds FUN_001191a8 / FUN_00119360: Stitch placed with
  FUN_001175a0 at the vector 0x3b4050 (x, y = `DAT_003788b8`, `DAT_003788bc`, built by FUN_001198c8; the second vector 0x3b4060 is `DAT_003788c4` / `DAT_003788c8`), motion 0,
  the player linked to him with motion 0xfe (FUN_0017f160), his +0x10c bit 0x200 and the hands' parent matrices (FUN_00193228), the player's mover 0x3c and the callback
  FUN_00119488 stored at object +0xcfc (FUN_00112510); the app does the placement, the motion and the callback (below) [Approx, `stitch_limit_enter`]. 10.59 (0x2f75c0 -> FUN_0011a6b8): AI +0x40 = the vector [Impl].
  10.51 (0x2f7450 -> FUN_0011a6d8(half width, half height, AI, record)): the AI's aim point (+0x40) and each valid, living enemy not out of view are projected
  (the point through the camera's view matrix at FUN_001068c0(0) + 0x10, the vector then multiplied by the projection block's +0x40 over its depth, FUN_00106f00), and the enemy
  within the half sizes (120 x 60 in Chicken Little's script) of the aim point on both axes and nearest to it by squared distance goes into the record; the record starts empty, so the first
  enemy in the box is taken whatever its distance (`fps::screen_target`, tested by `the_screen_box_search_takes_the_nearest_enemy_inside_the_box`) [Approx: [inferred] that +0x40 is the camera
  builder's focal length `(512 / 0.8) x 0.5 / tan(hfov / 2)` (docs/camera.md), the app's own projection stands for the frustum test]. The game writes the found enemy into the script's record
  (the last argument), which a host answer cannot do (that is kh2-bdx's runtime, as for 6.5), so the answer is kept in `ScriptedAi::fps_found` and the script's record is not filled
  [outside this work package's files: `kh2-bdx` `runtime.rs` needs a (10, 51) arm that writes the object, part 0, 0, -1 words as the 6.5 arm does]. Run lines
  (`KH2_LIMIT=529 KH2_SUMMON=1 KH2_SUMMON_AT=2 KH2_SUMMON_EVENT=22:limit@5`): `P_EX350#summon0 ... 10.59 ai_set_aim_vec [Impl]`, `script mode "surprise" -> "fps"`, `linked to the player (host 1.18, kind 0x4002, flags 0)` (the script's own link of the FPS mode, host 1.18 on himself), and with the events `22:limit,23:529`
  also `10.4 ai_vcall_2c [Approx] FUN_0011b008: FPS mode off ...` (the start routine resets the mode before the second event enters it). 10.3 of Chicken Little (R01b4, in a mode) and 10.51 (R0248, inside
  the `fps` mode's shot loop) were not reached in a run: the tests (`hosts_10_3_and_10_4_act_by_class`, the screen box test) are their evidence. Screenshot `fps2.png` (16 s) looked at: Sora
  with the summon panel showing Chicken Little's face and four hurt Dusks; a small cream shape at Sora's left boot is, I take it, Chicken Little at the player's origin (not certain).
- *The item command of 10.29 (0x2f70b0 -> FUN_0010c9d0(object, item, target, argument)).* Chicken Little's `use_potion` (R2ba7) calls it with item 1 and the player: `host 10.29`
  arguments (object, 1, the record of host 1.3, 0). The chain: FUN_001a1fc0(item) finds the item row (FUN_001a66e8), takes its command (+0x10) and the command row (FUN_001b6f48);
  FUN_00175e20 keeps the command's argument (the item) at object +0xb2c, the target's NeoStatus at +0xb30 and requests motion 9. At frame trigger 6 of that motion (FUN_00188498 case 6) FUN_001b8710 calls
  FUN_00175ec0, which answers true without taking anything for objects with +0x588 bit 0x400000 (the summon objects, host 10.31), then FUN_001b82b8 queues the use with the wait `DAT_0037bba4` (30 frames);
  FUN_001b84c8 counts it down and FUN_001b8658 -> FUN_001a11a0 applies the item (`items::item_effect`) to the target. The app does the same (`scripted::ItemUse`, `ItemUse::step`, the common section in `objects`):
  Genie's record 1 uses the same path. The user's Item Boost (FUN_001768a8 +0x4c) is not known for a summon: the multiplier is 100 [inferred]; the player's HP method refuses a player who is down, so he keeps his HP. Not carried out: the use
  effects (system effect 0 and the item's own effect), a target that is not the player. Test `an_item_use_waits_for_its_trigger_and_then_for_the_queue`. Genie's run (`KH2_SUMMON=2 KH2_SUMMON_AT=7 KH2_MAGIC_HURT=19:3`):
  `P_AL010#summon0 record 1 (FUN_00119b28): the command of item 1 on the player ...`, `trigger 6: item 1 is used (not taken from any slot), the effect lands in 30 frames`, `item 1 (amount 40) lands on the player: HP 3 -> 11, MP 100 -> 100`.
  Chicken Little's own run printed `item 1 on 0x2000000 (host 10.29, FUN_0010c9d0): motion 9 plays ...`, `trigger 6 ...` and the landing.
- *Stitch's strength.* A summon's status block is built by the object constructor FUN_0017c718: FUN_0019fd30(NeoStatus). FUN_0019fbc8 finds an existing block only for 1..15, so for 200..204 a new block (FUN_0019f708) gets +0x244 = the NeoStatus
  and FUN_0019f920 builds it from `FUN_00187d40(0x33fec0, 1)`, the party record of member 1: Sora's. Strength (+0x188), Magic (+0x18a) and Defense (+0x18c) are the `lvup` row at the record's level (FUN_001b50c0: FUN_001b58d8(200) is 0, counted as 1, Sora's
  table) plus the record's bytes +9 .. +0xb plus the equipment (FUN_001a0c08). So a summon's strength is Sora's, which is what `enemy::receive_hits` already takes for an attacker that is not a party member: nothing to change [V by the decompile]. The block's +0x250
  (the saved record) stays 0, so the ability bytes at +0xdd0.. are not copied (FUN_0019f920 copies them only from a saved record). The damage 0 seen with `atkp` 0x4ba is the table's: FUN_001a0610 multiplies by the row's `Power`, and the row (`kh2 table atkp`: Id 1210, Type 1, CriticalAdjust 1 = Magic, Power 0) has none; the harness
  screenshot of record 6 shows `0` popups.
- *Stitch's records 4, 5 and 6 and the limit sub-commands.* FUN_00119488 (set at object +0xcfc by FUN_001191a8's FUN_00112510, called by FUN_00112518 = host 7.26 `(object, command)`, 0x2ef888) switches on the command: 525 -> FUN_00118db8 (record 4), 526 -> FUN_00118ec0 (record 5), 527 -> FUN_00119068 (record 6), the pointers at
  0x378898 / 0x3788a0 / 0x3788a8; it runs the handler's phase 0 with +0xb7c bit 2 as it stands, makes the record the action (FUN_0010ecb0) and clears +0xb7c bits 0 and 2. Any other command was not read past its prologue (the action cleared). Record 4 (525): Stitch at the first spot (FUN_001175a0 with 0x3b4050), motion 0xc9 and queued motion 0
  (FUN_0017e378), the player linked with 0x101 / 0xfe; record 5 (526): the first spot, motion 0xcd, queued motion 0, the player's motion 0xff, AI slots 3 (all), 4 (on Stitch, argument 0x13) and 4 (on the player, 0x41); record 6 (527): FUN_00118b38 (the target pick: the blast hits every enemy in view, so not needed), the second spot (0x3b4060), motion 0xf2 with nothing queued, the player's 0x100, slots 0xc and 0xd (the player, 0x41).
  Records 5 and 6 run FUN_00118110 with their motion every frame (the attack object is made when +0x620 is 0 and every valid enemy in view is hit once per object); their test (phase 1) holds while +0xb7c bit 2 is set and the object's motion (+0x154) is theirs, and FUN_00118ec0 releases the enemies' actor stop 0x11 (FUN_00184b40) when it is not [not carried out: the app sets no stop id but 2]. Ported in `stitch::limit_command` / `limit_tick` / `run` / `can`, host 7.26 (raw words) sets `SummonAi::limit_cmd` and `objects` runs it after the script's tick. Records 4 and 5 queue motion 0, so when their motion ended the object's motion is 0 and
  the record is over; record 6 queues nothing, so it goes on (and fires again every frame, the attack object being freed with the motion) until the script clears the action (10.39) [the port follows the decompile; the loop was seen in a run]. Tests `stitch_limit_sub_commands_start_records_4_5_and_6`, `stitch_class_from_the_executable` (the seven handlers, the two spots and the three pointers). Run (`KH2_SUMMON=3 KH2_LIMIT=519 KH2_SUMMON_EVENT=22:limit@5,23:519@5.5,23:527@7.5`, a state with item 25): `limit sub-command 527 (host 7.26, FUN_00119488): record 6 (handler 0x119068) starts`, `record 6 ... starts: position (185, 125)`, `motion request 0xf2 -> slot Some(242)`, four `blast (FUN_00118110, atkp 0x4ba) hits`; with 525 and 526 the same lines for records 4 (motion 0xc9, slot 201) and 5 (0xcd, slot 205). Screenshot `stitch3.png` (8.2 s) looked at: Stitch (small, blue) above and behind Sora, four hurt Dusks with `0` and star hit effects.
  Ohana's own start (R00a9, event 23 with 519) also needed two hosts the app lacked and failed with `BadAddress` at 0xc6 without them: 1.305 (0x173e80: the camera's position, FUN_001071c8 of FUN_001068c0(1), returned as a pointer) answers the camera's world position [Approx] and 7.0 (0x2ef2f0 -> FUN_00184b40(id): the actor stop id released on every enemy; 17 here) does nothing since only stop 2 is ever set [Impl]; the start then reaches its 10.3. The hosts still unmapped in these runs (warnings) are the limit task's and the effects': 2.54, 2.63, 6.19, 6.36, 6.40, 0.32, 1.52, 1.157, 1.220, 1.227, 1.115, 1.285, 1.286, 1.318.
- *Summon stage 4 for Stitch: the heal (FUN_001155e0's tail).* While the player's HP (status word 0) is below his maximum (word 1), +0x674 loses a frame unit per frame; at 0 the player's vtable +0x78 is called with (1, 0, 0) and the timer restarts at the entry's +4 (`Consts::period`; the constructor starts it there). The app adds one HP through `magic_rules::healed` (the same HP method as 1.155)
  [`stitch::update`, `StitchAi::heal_timer`]. Test `stitch_heals_the_player_one_hp_each_period_while_hurt`. Run line: `N_HB040_BTL#summon0 heals the player (FUN_001155e0's tail, the player's method +0x78 with 1): HP 11 -> 12`.
- *Genie's follow motions.* FUN_002f6320 chooses the locomotion motion from a table by NeoStatus: 0xc9 (Genie) 0x3716d8, 0xca (Peter Pan) 0x3716c0, the rest 0x3716a8. Genie's holds [1, 0x98, 0x99, -1, -1]: the walk 1, the run 0x98, the stop motion 0x99. The WP-SUM8 choice (`scripted`'s block) is now the function `locomotion_want`, which Genie's follow uses too (test
  `locomotion_want_follows_fun_002f6320`, the table from the executable in `genie_class_from_the_executable_and_its_decision`). Run (`KH2_SUMMON=2 KH2_KEYS=12:W:3,17:W:2`): `P_AL010#summon0 locomotion motion 1 -> slot 1 (speed 2.0, walk 2.0 run 8.0)`, `motion 152 -> slot 152 (speed 8.0 ...)`, `motion 153 -> slot 153 (speed 0.0 ...)`, `motion 0 -> slot 0`; screenshot `genie.png` (26 s) looked at: Genie (blue, yellow vest, grey sphere) hovering beside Sora with two Dusks.
- *Peter Pan's own link of the player.* Two calls of host 1.18 in `P_EX330`: his `appear` (R1660) makes (object, player, 0x4002, 0, 0, 0), the shape of Tinker Bell's, and his limit (event 22 -> R09cb -> R00ee(468) -> R021f) makes (object, the limit attack's owner = the player, 0x4002, flags 1, routine word 580 = 0x244, argument the context): the SM1 arm takes both
  (object = himself, parent = the player). The routine R0244 (a code address no entry names, so the disassembly shows it as data) plays motion 0 on him again; it runs when the link breaks, through `Runtime::call_routine`. Flags bit 0 asks the automatic break of FUN_00169ee8 (the player's +0x160 bit 0x8000 lost), not modelled as in SM1. Run
  (`KH2_SUMMON=4 KH2_SUMMON_AT=2 KH2_LIMIT=468 KH2_SUMMON_EVENT=22:limit@5`): `P_EX330#summon0 ... 10.3 ai_vcall_28 [Impl]`, `1.18 link_start [Approx]`, `linked to the player (host 1.18, kind 0x4002, flags 1): placed at ...`; with a second event (`23:468`, which starts the command again) the first link breaks and `link to the player broken (FUN_001becb8): routine R0244 runs with argument 536870912`.
- *Not done.* The limit task and the limit BARs' scripts (the events come from the aid only); the airborne mover's flight; the camera's FPS mode and the player's FPS mover; the player-side motions and parent matrices of Stitch's limit (sora.rs, the camera); the items' use effects. (The 10.51 record write and the `docs/bdx.md` rows 1.100, 1.128, 7.26, 1.305, 7.0 and 10.3 / 10.4 were done in WP-SM3, below.)

**WP-SM3 follow-ups.**
- *Host 10.51's record.* `kh2-bdx/src/runtime.rs` answers (10, 51) itself: the host call (decoded `Opff`: the AI object, the record, the two half sizes) answers the object found as a handle (anything else is none) and the runtime writes the script's record, the second argument, as the object, 0, 0, -1; with none found the record is the emptied one (FUN_0018ed40: object 0, -1 in word 3), as 6.5 writes its record (FUN_0018ed40 then FUN_0018f490). The found record's part word is 0 [inferred: FUN_0011a6d8 copies the walker's record, which the app's walk gives no part]. `summon.rs` answers the enemy's handle from `fps::screen_target`. Test `screen_target_writes_the_found_object_into_the_scripts_record` (kh2-bdx `tests/runtime.rs`) asserts the record both ways.
- *The summon-use counter.* FUN_001c6168 (the summon start, inlined into FUN_001cf030 case 0x13c, section 4) ends with FUN_001d76f8(FUN_001d4ca0(command)): the BATTLE REPORT word of the command's 1-based row index (1 to 4). `tick` calls `count_summon_call` (`Progress::count_summon_use`) once the call is accepted, next to `sm.index`. Test `a_summon_call_counts_one_use_of_its_row`. The accepted-call path itself has no unit test (it needs the disc's rows and the full resource set).
- *The class vtables.* Read from the executable (the LOAD segment holding 0x354910): the words at 0x354910, 0x354978 and 0x3549b8, slots 10 and 11 (+0x28 / +0x2c, hosts 10.3 / 10.4) are FUN_00114170 and FUN_001141c0 in all three; slot 0 is FUN_00119a98 (Genie), FUN_0011a0b8 (Peter Pan) and FUN_0011a288 (Tinker Bell), and slot 15 holds FUN_0011a118 (Genie) and FUN_0011a590 (Tinker Bell). The addresses and the roles in the "class hosts 10.3 / 10.4" paragraph are right.
- *Peter Pan's 1.18 links of the player.* Confirmed from FUN_00170dd8 and FUN_001beb90: the call's object and parent come from the two contexts, the halfwords from words 2 and 3, and when a routine is given the record (object +0x570 .. +0x584) holds the context word `DAT_0035b9a0`, the routine and the argument; a link the object already has is broken first (FUN_001becb8). The app does the same with `SummonAi::link_start` / `link_end`: the `appear` link (0x4002, 0, no routine) queues nothing when the limit's link (0x4002, 1, routine 0x244) replaces it, and breaking the limit's link queues the routine with its argument. Test `peter_pans_links_of_the_player_break_with_their_routine`. Not done: the facing set from +0x60 / +0x68 and the +0x7f8 bit 8 on the break (FUN_003003e8, FUN_0016a7f0), as for 1.102.

**WP-FX12b follow-ups.** (1) 1.155's app side: `ai::SummonAi::heal_player` (the host arm) and `apply_heal` (`objects`, after the tick) are unit-tested
(`host_1_155_heal_reaches_the_players_hp`: 10/100 + 5 -> 15 and the HP written, the maximum clamps, a KO'd player (HP 0) stays); the `SummonHost::call`
arm itself is not run by a test (it needs a whole `BevyHost`), it only calls `heal_player`. (2) `scripted::decide`'s stand-ins for `FUN_0010fcd0`'s reads
of the member (`MemberNow`, `Params`, the setting byte, the record's phase 1) are tied to the record: only a record whose handler is one of `START_MODE`
(phase 1 answers 1 in `FUN_0011a590` / `FUN_0011a118`), with setting -1 and no MP-costing entry, is offered [inferred from the one record per class];
anything else is never chosen (`scripted_decide_offers_only_records_its_stand_ins_fit`). (3) `hud_party::summon_panel` no longer builds the circle at the
screen origin when Sora's HP circle is not in the scene: it logs a `warn!` once and retries each frame (`summon_panel_waits_for_soras_circle`).

**WP-SM6 (summon follow-ups, hosts of the limit starts and the bit readers).** Addresses are the executable's. Runs: `KH2_ISO` set, `KH2_NO_SOUND=1 KH2_NO_MUSIC=1 KH2_BDXLOG=1 KH2_SUMMONLOG=1 KH2_WAVE_GROUP=b_44 KH2_DRIVE_POINTS=5 KH2_STATE=<items 25, 159, 160, 383, 26 / summon level 1 / option word 11> kh2-game tt04`.
- *Stitch's Ohana start (R00a9) is answered end to end.* The run `KH2_SUMMON=3 KH2_SUMMON_AT=2 KH2_LIMIT=519 KH2_SUMMON_EVENT=22:limit@5,23:519@5.5,23:527@7.5` printed six `unimplemented host call` warnings before this package (6.36, 1.220, 1.157, 1.318, 1.295, 0.32); it prints none now. The unnamed ones reach the host as raw words (`Call::Unknown`) whose first word is a script context; `SummonHost::call_with_mem` reads the object the context names (the word at context +4, which is what the interpreter's own `O` decode reads: `ctx_object`).
  - **1.318 (0x170b30 -> FUN_00197008(object, id) -> FUN_002c32b8(id, FUN_0016b960(object)))** [Impl]: the sound `id` starts at the object's position. FUN_00197008 first swaps a `500xxx` id for the bank number of an owner's record (+0x9b4, halfword +2): the object itself when its +0x588 bit 0x40 is set, else the object at +0xaf8 when bit 4 is set (no swap without an owner or with a negative bank); the request carries the object's own `.apdx` (`PlaySe::apdx`, which is that private bank in the app). `SummonAi::play_sound` queues (object, id); `objects` writes `PlaySe::number(id, apdx).at(position)` after the script's tick (`sound_request`). Stitch plays 598050 after the arrival event (pc 0x16a), Peter Pan's limit routine R02fc 595042 (pc 0x30a), and a second call at pc 0x927. Run line: `bdx N_HB040_BTL#summon0 t240: first host call 1.318 ? [Impl]`, `summon: N_HB040_BTL#summon0 sound 598050 at object 0x2800000 (host 1.318, FUN_00197008)`. Test `host_1_318_queues_the_sound_for_the_object_the_context_names`. Nobody has listened: bank 598 / 595 are the summons' own banks and the app plays them as it plays any other number.
  - **6.36 (0x2eee10 -> FUN_00193248(object))** [Impl on the summon's own object]: +0x790 = 0, +0x10c &= ~0x40, +0x768 &= ~0x1000, the exact inverse of FUN_00193228 (the parent matrix `stitch::place` sets). This is the first writer of the clear that the gap at "who reads object +0x790" (above) listed as found only in name. `StitchAi` now keeps the parent matrix (`parent`) apart from +0x630 bit 0 (`attached`): the old single flag made FUN_001155e0's "place at the zero vector while bit 0 is clear" and the camera pin one thing, but FUN_00193248 does not touch +0x630. After 6.36 the pin to the camera plane (`objects`) is off until `stitch::place` runs again (the start's 10.3). The 1.18 link the start makes right after already won over the pin (`ai::Link::pose` runs later in the tick), so no screenshot differs. Test `host_6_36_releases_the_parent_matrix_but_not_the_placed_bit`. [GAP: the reader of +0x790 is still unknown.]
  - **0.32 `fader_out` (0x16d880 -> FUN_0014a7f0(FUN_0014a918() = 0x3564d0, seconds x DAT_0037a250, 1))** [Approx]: state 1 (the fade-in) of the screen fader at 0x3564d0 over the length; the start calls it with 0 after its event, so the fader ends clear. The app never fades 0x3564d0 (the revive's white quad is another fader, 0x356b30, `revive_fade.rs`), so there is nothing to clear.
  - **1.295 (0x173cf8 -> FUN_00140e38(FUN_00140d08() = 0x355840))** [Stub]: byte +0x39 of the field camera, the reset-behind-the-player flag (`docs/camera.md` 0x38 / 0x39: FUN_00140288 clears it and runs FUN_0013fdd8). The app's camera takes that reset only from its own input path (`camera.rs` `gather_input`, a private `CamState`).
  - **1.220 (0x170838 -> FUN_00183eb0(length, DAT_0037a490, DAT_0037a490, object, flag != 0))** [Stub]: sweeps the object forward by `length` in steps (FUN_00143840 moves, FUN_0016a670 applies); Ohana's start asks it of the player with 700.0 and flag 0 [GAP: FUN_00143840 not read].
  - **1.157 (0x171f40 -> FUN_00197230(object, name, second object))** [Stub]: `name` is a 4-character entry name of type 0x16 in the object's BAR (FUN_001971c0, `FUN_00168510(BAR, 0x16, name)`), its data is an event record (type 0xb, FUN_00195898) placed at +0x80 of the object, bound to the second object (FUN_00195a78) and started (FUN_00195ae0). Ohana plays entry "10" (pc 0x150) and waits for it with 1.34 and 1.274 > 26. The app has no player for object-owned events, so the cutscene shows nothing [GAP: the record layout was not read].
- *Peter Pan's limit hosts, answered or documented.* The run `KH2_SUMMON=4 KH2_SUMMON_AT=2 KH2_LIMIT=468 KH2_SUMMON_EVENT=22:limit@5,23:468@7` now prints one `unimplemented host call`, 1.247, with 6.19, 6.40, 1.115, 1.227, 1.286, 1.285 and 1.221 answered: 6.19 (0x2eecb8) and 1.286 (0x173b10) set bits 0x800 and 0x1000 of the global flag word 0x35d268 [Approx, `SummonAi::gflags`]; the readers are FUN_00190610 (`& 0x800` with the player object, called by FUN_001a3848 and FUN_001bd358) and FUN_0019afe8 (`& 0x1000`, the object update gate of FUN_00184498 and the shading of FUN_00191c18). 6.40 (0x2eee70 -> FUN_0016b698(object, 0x34)) gives the player the mover kind 0x34 and 1.115 (0x171388) / 1.227 (0x171350) are the end of the limit's mover words and mover method on the player [Stub: sora.rs's movement, kind 0x34's update was not read, the movers are objects built at run time at 0x1c88eb0.. so a table read does not give it]. 1.285 (0x170a50 -> FUN_001972b0(object, n)) starts entry type 0x2e of the object's BAR as a 2D sequence (FUN_001538f0 -> FUN_00153848, 0xa4 bytes) and returns it [Stub]. 1.221 (0x170878 -> FUN_001b6fd0(FUN_001b6f48(command), object, a target record)) is the generic "command row usable by object" test (FUN_0019abb0: the row's Flags bits 1, 2, 4, 7, 8 and the object's +0x160 / +0x168 / +0x50c / +0x9b0 words); Peter Pan's limit thread asks it of the player for commands 470, 471 and 482 every frame and offers the usable one with 1.165 [Stub, answers false: the player's words are not modelled and 1.165 only logs]. Chicken Little's 1.52 (0x1728c0, bit 1 of himself) sets bit n of the array at +0x47c (`SummonAi::parts_47c`) [Approx: the summon bodies have no collision part table to test it]. 1.247 `random_free_point` (0x1726d8 -> FUN_001a5ea8 -> FUN_001a6068 builds the search record: the second generator `DAT_003716a0 x 0x10dcd` twice, the radius between the two radii, a start angle from the counter `DAT_0035e9b0`; FUN_001a6198 walks a spiral of 8 samples per ring with two collision probes FUN_001a5c30 and FUN_001a5bf8 and returns the first free point) stays unmapped [GAP: the two probes were not read; the app has no probe with that signature]. Tests `host_bits_of_1_52_6_19_and_1_286_are_kept`.
- *The reader of +0xb7c bit 0x800 (the old Genie gap) and a bug it exposed.* FUN_0010fcd0 (the friend chooser, `party_battle::choose`) tests `(b7c & 0x800) != 0 || +0xce8 < 2` before it chooses: with the bit set the choice is never turned into the "chosen twice: move" branch. FUN_0010b780 ors 0x800 into +0xb7c for both Genie (NeoStatus 0xc9, the path through `LAB_0010bc7c` after FUN_001199f8) and Peter Pan (0xca). `scripted::decide` already passed it for Peter Pan (`class == PAN.class`); `genie::decide` passed `false` (the doc above had taken 0x800 for a flight bit, which no reader supports). It passes `genie::SUMMON_ANY` (true) now. The other decompiled readers of +0xb7c test bits 4, 0x10, 0x20, 0x40, 0x8000 and the sign bit (FUN_0010ca88, FUN_0010e140, FUN_0010ed20, FUN_0010f220, FUN_00110468, FUN_00110a58, FUN_00111070, FUN_001155e0); no store of 0x800 besides FUN_0010b780's and FUN_001124c8's whole-word store (called only by FUN_00119360) turned up (`grep 0xb7c` over the decompilation, `mips_scan.py --store-off 0xb7c`). Test `genie_chooses_even_after_two_repeats_because_of_bit_0x800` (the same state with the bit clear is `Choice::Move`).
- *The reader of +0xb7c bit 0x400 (hosts 7.24 / 7.25).* The one reader found is FUN_001115a0 (`if (b7c & 0x400) == 0` then FUN_00174c28 else 0), whose address sits at +0x30 of the tables at 0x352ddc and 0x35fe6c (words 0x352e0c and 0x35fe9c in the executable); FUN_00174c28 is the controller-ready test of the player (the first test of FUN_00179108, the player's "may move" method). So bit 0x400 turns that object virtual off. The callers of the +0x30 slot of those tables were not identified [GAP: dozens of `+ 0x30` virtual calls in 0x101000..0x124000 and 0x183080 were not matched to the table].
- *Not done, and why.* The limit offers (1.165 slot 2; needs `reaction_cmd` / `hud_menu`, outside this package's files, and the slot-2 capsule has no HUD); the camera reset 1.295 (needs a request in `camera.rs`); the event of 1.157 and the 2D sequence of 1.285 (no object-event player in the app); the sweep 1.220; the player's mover kinds (sora.rs). Screenshot `stitch3.png` (8.2 s, the Ohana run above, scratchpad, not committed) looked at: Sora with Stitch (small, blue) just behind him, three Dusks hurt with `0` popups and star hit effects, the SUMMON gauge and Stitch's portrait at the lower right; the changed behaviour (sound queue, released pin, hosts answered) is not visible in a picture.

**WP-SM7 (summon follow-ups 2: host 1.247 answered, the readers of the gate bits, the sweep 1.220 read).** Addresses are the executable's. Runs: `KH2_ISO` set, `KH2_NO_SOUND=1 KH2_NO_MUSIC=1 KH2_BDXLOG=1 KH2_SUMMONLOG=1 KH2_WAVE_GROUP=b_44 KH2_DRIVE_POINTS=5 KH2_STATE=<items 25, 159, 160, 383, 26 / summon level 1 / option word 11> kh2-game tt04`.
- *Host 1.247 `random_free_point` is answered for the summons.* The summon host built the party host with `free: None`, so the call returned its centre and printed `unimplemented host call 1.247`. `SummonData` now reads the numbers of FUN_001a6068 / FUN_001a6198 from the executable (`party_rules::FreePointConsts`, 0x37b5a8..0x37b5c0) and hands them to the host; the search is the members' `party_rules::free_point`, which already matched the game in the oracle `party_free_point_matches_fun_001a5ea8`. The two probes were read here: FUN_001a5c30 casts a segment from the candidate point lowered by the second float to the point raised by the sum of both floats through the floor cast FUN_001d1648 and answers its hit (else it copies the point), and FUN_001a5bf8 is `FUN_001d1648(..) ^ 1` (the line test is clear when the cast finds nothing); `free_point` takes the same two as `ground` and `clear`. Run `KH2_SUMMON=4 KH2_SUMMON_AT=2 KH2_LIMIT=468 KH2_SUMMON_EVENT=22:limit@5,23:468@7`, 9 s: `bdx P_EX330#summon0 t241: first host call 1.247 random_free_point [Approx]`, no `unimplemented host call` and no script error. [Approx: the app's floor cast is the highest floor polygon in the span and the line test any polygon, the collision masks are not told apart.] Test `summon_free_point_numbers_come_from_the_executable`.
- *The readers of the global flag bits 0x800 and 0x1000 of 0x35d268 (the SM6 claim checked).* FUN_00190610 takes no argument: it is `(0x35d268 & 0x800) != 0 && DAT_0035c998 != 0` (a player object exists), not a test with the player object as SM6 wrote. Its two callers are FUN_001a3848 (an enemy's target decision: with the bit set and a player it ors 8 into the target record's +0x28 and passes the player to FUN_001a3a88 as the target) and FUN_001bd358 (after FUN_001a3700 on the player's position, in the enemy's attack check). So Peter Pan's 6.19 makes the enemies choose the player. FUN_0019afe8 is as SM6 said (`& 0x1000` among +0x10c bits 0x80 / 0x200, FUN_0019af28, FUN_0019b100, FUN_002c56c0); its callers are FUN_00184498 and FUN_00191c18 only (`mips_scan.py --jal 0x190610 / 0x19afe8`). The app has none of these gates [GAP, enemy files].
- *The caller of the +0x30 slot of the tables at 0x352ddc and 0x35fe6c (FUN_001115a0).* Those tables are the tails of the vtables at 0x352d88 (the friend class, set by FUN_0010b780 at the start of its constructor) and 0x35fe18 (a second class that uses the same slots; its constructors are around 0x1c5dac): the word at 0x352e0c is vtable +0x84, not a +0x30 slot. The one virtual call of vtable +0x84 on a base read from the object's first word is FUN_00174bd8 (itself vtable +0x18 at 0x352da0): when FUN_001115a0 says the object may move (bit 0x400 of +0xb7c clear and FUN_00174c28) it copies the vector at +0x10 and sets the facing +0x66c = FUN_003003e8(x, z) of it (FUN_0016a948), then FUN_00197118 refreshes the two shape points at +0x9dc. So bit 0x400 (hosts 7.24 / 7.25) freezes the object's facing from its +0x10 vector. The app turns bodies from their own motion and does not model the bit [GAP: sora.rs / the summon body orientation].
- *The reader of +0x790 (the parent matrix FUN_00193228 / FUN_00193248).* `mips_scan.py --load-off 0x790` finds no load; the stores are FUN_00193228 / FUN_00193248 and, in another class (the table object FUN_00145748 / FUN_001459d0, where +0x790 is a vtable word), the unrelated constructor stores. The reader, if there is one, goes through a computed base [GAP: not found]. The companion bit +0x768 & 0x1000 (set and cleared by the same pair) has no decompiled reader either: the +0x768 loads are FUN_00193048 (bits 0x200 / 0x400), FUN_00191c18 (bits 4 / 8) and the rest test other bits.
- *1.220 (FUN_00183eb0) read, not ported.* FUN_00183eb0(length, step, object, flag) copies the object's move block (+0x590, FUN_0032ca50) with its position FUN_0016b960, then repeats up to `length` in increments of max(step, length-so-far): FUN_00143840 (the sphere sweep) tests the block against the actors (FUN_00165430 / FUN_00164138), the area's wall list (FUN_001654f8, entries from FUN_00165910) and the floor (FUN_001423d8), and writes the result (new point at +0x80, flags +0x70 / +0x6c); with flag 0 a hit (+0x70 bit 32) stops the sweep, else the object takes the point (FUN_0016a670 = FUN_0016a618, the position setter of host 1.148, then +0x7f8 |= 0x1000). Ohana's start asks it of the player with 700.0 and flag 0: Sora is pushed forward up to 700 units until a wall. The app has no request that moves the player from a summon [GAP, outside ownership: `sora.rs` would take a "move by" request; the collision test would be `World::raycast` on the swept segment, the block's radius at +0x6c was not traced].
- *The 1.318 call count of the SM6 run.* The three `sound 598050` lines of `KH2_SUMMON_EVENT=22:limit@5,23:519@5.5,23:527@7.5` are: the limit task's own event 22 (pc 0x16a), the harness's duplicate event 22 (the same routine, 0x16a again) and event 23 (pc 0xbb8 [inferred: 0x3b8 pushes 598051 and was not reached; the pc is not logged]). With no harness events the run has one such line (`KH2_LIMIT=519` alone), with only `23:519@5.5` two. Peter Pan's 595042 lines are the same duplicates: pc 0x30a is the only 595042 call (R0311); the call at pc 0x927 pushes 595043 and was not reached (SM6 had it as a second 595042 call).
- *Peter Pan's 6.40 / 1.115 / 1.227 / 1.285 / 1.221 stay stubs, with the reason.* The mover kind table is `PTR_DAT_0035ca48[kind]` (FUN_00179868; FUN_0016b648 sets `object +0xc` to the entry and calls its vtable +0, FUN_0016b698 first calls the old mover's vtable +4): the entries are objects built at run time in memory the executable does not hold (the table's words are zero in the file), so kind 0x34's methods cannot be read without a trace of the running game [GAP: needs a `kh2-ee` run of the mover init, outside this package]. 1.285 and 1.221 need `reaction_cmd` / `hud_menu` and a 2D sequence player (not mine).
- Not done: the camera reset 1.295 (camera.rs), the limit offer 1.165 (reaction_cmd / hud_menu), 1.157 (no object-event player).
- Screenshots looked at: `st3.png` (Stitch Ohana run, `KH2_SUMMON=3 KH2_SUMMON_AT=2 KH2_LIMIT=519 KH2_SUMMON_EVENT=23:519@5.5`, 12 s): Sora with the keyblade in Sandlot, a small blue Stitch to the left of the Dusks, three Dusks with damage numbers, Stitch's portrait and the SUMMON gauge at the lower right; `pp.png` (Peter Pan, 9 s): Sora with arms spread and green sparkles among Dusks, Peter Pan's portrait and the SUMMON gauge at the lower right. The new behaviour (the 1.247 answer) is not visible in a picture.

**WP-FL1 (the readers of 0x800 and 0x1000, corrected).** SM7's reading of FUN_001a3848 and FUN_001bd358 as an enemy's target decision was wrong: FUN_001a3848 is the prize manager's pickup dispatch and FUN_001bd358 the item object's pickup check (prize.rs). With bit 0x800 (host 6.19) and a player object, every prize gets +0x28 bit 8 and, from age 3 (+0x44), goes to the player through FUN_001a3a88 (`magic_rules::prize_dispatch`), and an item out of reach is still given to the player (`item_taken_by_player`); enemies are not affected. Host 1.286's bit 0x1000 is `update_gate_global` (FUN_0019afe8: the weapon update FUN_00184498 is held, FUN_00191c18 clears +0x768 bit 8). `summon::publish_gflags` carries both bits to `magic_rules::GlobalFlags`; a removed summon object takes its bits along [inferred]. The wiring into prize.rs and the weapon update is outside this package [GAP].
