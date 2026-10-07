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

### Tinker Bell's revive (Peter Pan) [V]

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

- The enemies' scripts keep running during the stop: `enemy_ai.rs` (not owned here) would have to skip an
  enemy's tick while `SummonState::stop` is set. Their movement from the script therefore continues.
- The companions come back where the party code left them (it keeps moving them while they are hidden):
  `party.rs` has no way to place a member; FUN_0017d290's spot (200..300 units, searched from 100 units above
  the player) needs a placement call there.
- The event that the `appear` mode and the thread wait for is never started (section 7), so the wait ends at once;
  the HUD layers 7..10 are not hidden. The fade list (FUN_0016ff50, FUN_0016f8e0, FUN_0016fb20) is not modelled.
- The free-spot collision tests FUN_001a5c30 and FUN_001a5bf8 were not read: a floor under the point (`World::ground`)
  stands in for the first, the second always passes [inferred].
- No friend AI, no group 10 hosts, no motion requests (stage 4); summon limits are not offered by the scripts
  (host 1.165) and the limit task body is not run (stage 5); the summon face panel, Genie's "Drive!", Tinker
  Bell's reraise and the area re-entry (stage 6).
- Byte 5 and byte 7 are kept in `SummonState`, not written to `Progress` (no setters for them).
- The game steps one LCG (`DAT_003716a0`) for many systems; the app keeps a copy for the summon only, started
  from the executable's initial word, which is 0 in the file. A multiplicative LCG at 0 stays 0, so the game
  must seed it somewhere [GAP: none of the 34 functions `mips_scan.py --addr 0x3716a0` lists stores anything but
  the next step; a store through a computed base or an overlay was not searched]. With 0 the port gives Genie
  the first owned form and the free spot its minimum radius.
- System effect 0x13 is element 19 of `00effect.bar` (`kh2 pax 00effect.bar`: effect 18, bone 67, flags 0x805,
  sound 180), but `kh2_fx::Effect::new` refuses its particle program, so nothing is drawn (logged `not started:
  its particle program did not start`); fx.rs / kh2-fx are not owned here.
- The free spot's direction is (sin, 0, cos) of the angle and the LCG word is read unsigned (both oracle-checked).

Harness (the runs behind the status, tt02, Donald and Goofy spawned by the map script):
`KH2_STATE=<file with "items": {"25": 1}, "summon": {"level": 1, "exp": 0}, "option_word": 11>` (11 = the default
option word with Summon Effects 0, so the arrival skips the event) `KH2_DRIVE_POINTS=5 KH2_SUMMON=3 KH2_SUMMON_AT=4`,
plus `KH2_DISMISS_AT=12` (Dismiss) or `KH2_SUMMON_AT=2 KH2_LIMIT=519 KH2_DISMISS_AT=4.5` (Ohana with Stitch out:
`limit: summon limit cost 100 x 12 = -1200 to the summon timer`, `summon: Dismiss refused (summon state 2, limit
task true)`). Without `option_word` the default Auto plays the event path on the first summon.

## 7. Unknowns (with what was searched)

- What starts the arrival event that FUN_001c6260 and the `appear` mode wait on: FUN_0014f198 only hides
  HUD layers; the appear routines of `N_HB040_BTL` and `P_EX330` call link_start (1.18), play_motion_with
  (1.29) and 1.318 (a sound). Candidate: the link (FUN_001becb8), not read.
- Who delivers events 22 and 23 to a summon's own script: FUN_0016c620 calls with 0x16 or 0x17 in the
  decompilation: only FUN_001b28a0 (to the limit task's thread).
- Summon stats (strength, max HP) and where the party setup reads them: no `plrp` row for 200..204;
  FUN_0010b780 and FUN_0010be80 not read past the AI selection.
- The friend AI engine (288 functions) and the per-summon AI classes (FUN_001179c0 etc.): not read.
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
