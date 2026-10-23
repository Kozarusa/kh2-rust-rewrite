# Party members (Donald, Goofy, the world partners): implementation spec

[Contract](contract.md) | [Index](../../README.md)

Work package 7H (research); stage 1 of section 9 is in the app since WP-7P1 (section 11). The note is meant to let later
work packages build the party stage by stage without reading the game code again. Structure and standard
follow `docs/summons.md`.

Marks: **[V]** read in the game's code (address given) or checked against the disc or a kh2-ee run;
**[I]** inferred (fits the code but no line states it); **[GAP]** not found, with the addresses searched.
Addresses are functions (`FUN_xxxxxxxx`) or data in `SLUS_210.05`. No game values are copied here except
where a test prints them from the user's disc or the executable; the code reads them at run time.

Evidence that runs:

- `cargo test -p kh2-tables --test party -- --nocapture` prints `memt` (every record, the seven party
  presets, the member objects per world), the three stand-in objects (`PLAYER`, `FRIEND_1`, `FRIEND_2`),
  every party-member object with its friend-AI class, BAR contents (BDX present or not), motion set and
  effect file sizes, and the `plrp`, `prty` and `lvup` rows.
- `cargo test -p kh2 --test party_oracle -- --nocapture` runs the game's own FUN_001a6ff0 (memt merge),
  FUN_0017cd20 (slot to object), FUN_001a7330 (party preset), FUN_001c77a8 (members a form takes),
  FUN_001a0be0 (HP in danger), FUN_0010f490 / FUN_0010f4d0 (HP / MP below a fraction), FUN_00114078 /
  FUN_001140c0 (follow-speed thresholds) in kh2-ee and compares them with the rules written below. It also
  prints the follow thresholds of all 19 AI classes.
- `kh2 bdx P_EX020` (Donald), `P_EX030` (Goofy), `P_HE000`, `P_EH000`, `P_TR000` ... disassemble the members'
  scripts.

Earlier notes: `docs/summons.md` (summons share the friend class; the fade-out of the companions),
`docs/drive-forms.md` and `docs/player-data.md` (forms take companions), `docs/limits-reactions.md`
(limits need partners), `docs/hud.md` ("Party panels"), `docs/hit-reaction.md` (the party class's clash and
receive handlers), `docs/audio.md` (voices by NeoStatus).

## 1. The data

### Who is who [V, from the disc test]

- **Party member id = NeoStatus** (`objentry` +0x4c, FUN_001699a8), the byte `plrp.Character` and
  `limt.Character` use: 1 Sora, 2 Donald, 3 Goofy, 4 Mickey (`P_EX200`, object type 0, a player class),
  5 Auron, 6 Mulan, 7 Aladdin, 8 Jack Sparrow, 9 Beast, 10 Jack Skellington, 11 Simba, 12 Tron, 13 Riku,
  14 Roxas (`P_EX110`, type 0), 15 Ping (`P_MU010`). 200..204 are the summons (docs/summons.md).
- **Party-member objects** are `objentry` type 1: `P_EX020` Donald, `P_EX030` Goofy and their world
  variants (`_NM`, `_TR`, `P_WI020`/`P_WI030`, `P_LK020`/`P_LK030`, `N_LM010_PLY`/`N_LM020_PLY`), `P_HE000`,
  `P_MU000`, `P_MU010`, `P_AL000`, `P_CA000` (+`_HUMAN`, `_LOW` variants), `P_BB000`, `P_NM000` (+`_SANTA`),
  `P_LK000`, `P_TR000`, `P_EH000` (+`_SIDECAR`, `_LAST`). Every one of them except the Atlantica pair has a
  BDX script (BAR type 3) in its `.mdlx`, a motion set and an `.apdx`.
- **Stand-ins**: objentry 0x236 `PLAYER`, 0x237 `FRIEND_1` (type 1), 0x238 `FRIEND_2` (type 1), NeoStatus 0;
  0x319/0x31a/0x3ee `MENU_FRIEND_1..3` (type 14, menu objects). Area spawn lists name the stand-ins;
  section 2 resolves them.

### `memt` (00battle.bin): member objects per world [V]

`u32 version (5), u32 count (34)`, `count` records of 0x30 bytes, then seven 4-byte party presets.
FUN_001a6f48 stores the file (`DAT_01c95600`) and the preset list (`DAT_01c95604` = file + 8 + count x 0x30).

| record field | offset | meaning |
|---|---|---|
| world | +0 (u16) | 0 = any world, else the world id (`DAT_0033e830`) |
| flag | +2 (u16) | story flag that must be set (0 = none) (FUN_001a6f70 -> FUN_001802e0) |
| not-flag | +4 (u16) | story flag that must be clear (0 = none) |
| two words | +8, +0xc (i32) | passed to FUN_0016f278 (`DAT_01c88360..68`): [I] the object memory budget of the world |
| members | +0x10, 16 x u16 | object ids by **member index**: 0 the player object, 1 Donald's, 2 Goofy's, 3 the world's partner, 4..8 Sora's form objects (Valor..Anti, FUN_001cf2c8), 9 Mickey, 10..15 the keyblade objects (FUN_00196ad0) |

Merge (FUN_001a6ff0, on area load from FUN_001689c0): for every record in file order whose world is 0 or the
current world and whose flags pass, every non-zero member word replaces the merged word. The merged list
is the 0x30-byte block at `0x01c95608` (FUN_001a7118), members at +0x10. **Checked by the oracle test for
worlds 0..18, with no flag set and with every flag memt names set.** Story flag rule (FUN_001802e0): bit
`f & 31` of the word at `0x33f6b0 + (f >> 10) x 0x20 + ((f & 0x3ff) >> 5) x 4`.

With no flag set (printed by the disc test): Donald and Goofy are `P_EX020`/`P_EX030` everywhere except
world 10 (Pride Lands, `P_LK020`/`P_LK030`), 11 (Atlantica, `N_LM0x0_PLY`), 13 (Timeless River,
`P_WI020`/`P_WI030`), 14 (Halloween Town, `_NM`), 17 (Space Paranoids, `_TR`). The partner (index 3):
5 Beast, 6 Auron, 7 Aladdin, 8 Ping (Mulan with flag 8216), 10 Simba, 14 Jack (Santa with flag 14394), 16
Jack Sparrow (`P_CA000_HUMAN_LOW`, `P_CA000_LOW` by flags), 17 Tron, 18 Riku (`P_EH000_LAST` with flag
18501), and Riku (`P_EH000`) as the default in the other worlds.

### Party presets (end of `memt`) and the slot bytes [V]

The current party of a world is four **slot bytes** in the save data at `0x340c9c + world x 4`
(FUN_001a71c0): byte 0 the player's member index, bytes 1, 2 the two companions, byte 3 the third
(partner) slot. Low five bits = member index, 0x10 = empty (FUN_001a71e8); bit 0x80 / 0x40 are flags
(below). The disc's presets: 0 `[0,-,-,-]`, 1 `[0,1,2,-]`, 2 `[0,3,2,1]`, 3 `[0,3*,2,1]`,
4 `[0,3*,2,1]`, 5 `[0,3,-,-]`, 6 `[0,1,-,-]` (`*` = flag: 0x43 in preset 3, 0x83 in preset 4).

Applying a preset (FUN_001a7330, area-script command 0xf of FUN_00180b48; checked by the oracle test for all
seven presets): the result is the preset's player index; then the preset's members flagged 0x80/0x40; then
the current members (slots 1..3 in order) that the preset also lists; then the preset's remaining
members; the rest 0x10. Stored bytes are masked with 0x9f. FUN_001a7558 (from FUN_001b4260, in the member
swap code) puts a member (FUN_001a7500: NeoStatus 1 and 14 -> index 0, 2 -> 1, 3 -> 2, others 3) into slot 3
and the former slot-3 byte where that member was in slots 1..2.

A member counts as "in the party" (FUN_001a7698 via FUN_001a7608 / FUN_001a7268) when it sits in slot 0, or
its slot byte has 0x80, or its index is in the hidden list (below). FUN_0017d150 / FUN_0017d0f0 use it.

### Stats, equipment, abilities [V for the tables, I for their use at new game]

- `plrp`: one row per party member id and set (`Id` 0 and 1): HP, MP, AP, armor/accessory/item slot counts and
  32 item words (bit 15 = equipped). Donald: HP 18, items Potion, Ether, Donald Thunder, Donald Cure (set 0)
  and Fantasia, Flare Force, Donald Fire, Donald Blizzard, Auto Limit, ... (set 1). Goofy: HP 25, Goofy Bash,
  Goofy Tornado, Goofy Turbo, Tornado Fusion, Teamwork, Auto Change, ... The rows feed the saved party
  records (`0x33fec0` + the per-member record, FUN_00187d40); the battle state is rebuilt from the saved
  record (FUN_0019fd30 -> FUN_0019f920, docs/player-data.md "How abilities apply").
- `lvup`: 13 character tables of 99 levels (table 0 Sora, then [I] 1 Donald, 2 Goofy, ... in party-member
  order); printed by the disc test.
- `prty` (03system.bin pref): rows by character index (row 1 is Sora's, as kh2-app reads it). Rows 2..8 and
  12..15 hold walk 2, run 7..18 per character; StunTime is 0 in those rows. (WP-P3b) The game indexes `prty`
  by the objentry **NeoMoveset**, not the NeoStatus: FUN_00194438 copies the row into +0x110.. and keeps it
  at +0x12c (section 3); Donald's row 17 and Goofy's row 20 hold walk 2, run 8, StunTime 2400 / 2100,
  MpDrive 1200.
- The member's own ability list: 64 slots at party record +0x54 (FUN_0010f578 searches it for an equipped
  ability item); the ability bit set at status +0x1cc (FUN_001768a8) as for Sora.
- **AI settings per member**: bytes at party record `+0xd4` (FUN_00112308: config block FUN_001057a8 + 0x1660,
  record by NeoStatus through FUN_00187d40). Byte 0 = the member's battle style 0..5 (friend vtable slot
  +0x98, FUN_00112350); byte `1 + k` = the use setting of item / ability `k`: k 0..6 are the items 1..7
  (Potion, Hi-Potion, Ether, Elixir, Mega-Potion, Mega-Ether, Megalixir; table 0x352eb0), k 7 and up the
  member's special abilities in the order of the table at 0x352ec0 (FUN_00114348 / FUN_001144d0). Values
  used by the code: 1 = only while MP >= the action's cost, 2 = only as a fallback (second pass), 3 = never
  (FUN_0010fcd0, FUN_0010f220). [V, WP-PA4] these are the Customize page 0x1b settings: the Battle Style row
  writes byte 0 (`FUN_00275398`), an ability row byte `1 + FUN_00114348(id)` with the values 0, 1, 2 (`FUN_00275410`),
  an item row Auto-Reload and the byte 0 or 2 (`FUN_00274f70`); docs/menus.md 14b. Value 3 ("never") has no menu row.
- `ptya`: the moveset table (docs/moveset-selection.md); party members have their own `NeoMoveset`
  (Donald 17, Goofy 20, ... printed). `patn` is the hit-state conversion (docs/hit-reaction.md), shared.
  `vbrt`: [I] the controller vibration table (frame trigger 31, docs/hit-reaction.md); nothing party-specific
  reads it in the paths read.

### ELF-embedded friend tables (not on the disc) [V]

| address | content |
|---|---|
| 0x34ea00 | AI parameter table: 19 u32 offsets from the table start, one block per AI class (section 4) |
| 0x352ec0 | 29 rows `{ability bit, ability item, NeoStatus, index}`: the members' special abilities (Donald Fire 165 .. Cure 168, Goofy Tornado 423, Bash 429, Turbo 425, Beast 430, ...) |
| 0x352eb0 | the seven consumable items 1..7 in setting order |
| 0x352e28, 0x352e3c | HP-item choice order: single target `1, 2, 5, 4, 7`, several `5, 1, 2, 4, 7` |
| 0x352e50, 0x352e60 | MP-item choice order: `3, 6, 4, 7` and `6, 3, 7, 4` |
| 0x353090 (Donald, 20), 0x3533a0 (Goofy, 18), ... | the AI class's action records (section 4) |
| 0x352d28, 0x352cd0 | target choice / re-target test per battle style 0..5 |

## 2. Who the friends are

1. Area scripts spawn `FRIEND_1` / `FRIEND_2` (tt02 does; docs/enemies.md). The object factory FUN_0016e8c0
   resolves the objentry id through FUN_0016e208: 0x236 = the player object, **0x237 = FUN_0017cd20(1),
   0x238 = FUN_0017cd20(2)**, 0x319/0x31a/0x3ee = FUN_0017cd78(1/2/3) (the same, plus Jack Sparrow (NeoStatus 8)
   becomes 0x81d when FUN_0016f038 < 2 [I: a memory-light variant]). FUN_0017cd20(slot) = the merged member
   object of the slot byte's index, or none for 0x10 (oracle-checked).
2. The factory refuses the menu stand-ins 0x319/0x31a/0x3ee and ids with bit 28 or 29 set (FUN_0016efd8,
   FUN_0016f008), and any object whose `SpawnLimiter` (+0x54) exceeds what FUN_00181458 allows
   (FUN_00181408; [I] a memory budget).
3. Type 1 -> FUN_0017d1b0 (section 3).
4. The friend's files are the object's own: `obj/<model>.mdlx` (model + BDX), the motion set named by
   `AnimationName`, `obj/<model>.apdx` (effects and sound banks). Sizes per object are printed by the disc
   test. The base constructor attaches up to two hand objects (FUN_0017c718: FUN_0016ef28(objentry, 0/1) ->
   FUN_001751b0) [I: the weapons; objentry has `DONALD_WEAPON` 575, `GOOFY_WEAPON` 576, `AURON_WEAPON` 704,
   type 6; FUN_0016ef28 was not read].
5. Voices: by NeoStatus through the voice table (docs/audio.md "other party"); the friend's voice method is
   vtable slot +0x54, the party class's FUN_00175d18 (summons override it with FUN_001c6998). Lines asked
   for: 2 on KO (FUN_00175f10), 9 when its HP falls into danger (FUN_0017cde0).

## 3. Spawn and despawn

### Construction [V]

FUN_0017d1b0(rotation, objentry, spawn record):

- a summon is active (`DAT_00340c88` byte 5): the member is listed as hidden (FUN_001c7600) and the summon is
  created again instead (docs/summons.md);
- the current drive form takes this member (FUN_001c77a8(form byte 4 of `DAT_00340c88`, NeoStatus)): listed
  as hidden, no object;
- else a 0xd00-byte object, FUN_0010be80 (= FUN_0010b780, the friend class).

FUN_0017d290 (re-create beside the player, used by the summon return FUN_001c7ab0 and the member swap
FUN_001b3f78): a free spot `[min, max]` units from the player, searched from a point lifted by the first
argument (FUN_001a5bf8 / FUN_001a5ea8), facing the player.

Constructor chain:

1. **Actor base** FUN_001746e0(obj, objentry, 1, record).
2. **Friend base** FUN_0017c718 (vtable 0x35cc18): first free party slot `DAT_01c893a8[0..1]` (FUN_0017cd08),
   slot index at +0xb48, +0x588 |= 0x4000, status block FUN_0019fd30(NeoStatus) at +0x48c (an existing block
   is rebuilt with FUN_0019f920; a new one (FUN_0019f708) gets +0x244 = NeoStatus, is built from a saved
   party record (FUN_00187d40 on 0x33fec0) and gets its HP through FUN_001a0140 [I: set to the maximum]),
   its owner pointer +0x24c; side angle +0xb3c = `slot x 2a - a` with a = `DAT_01c893a0` (set from 0x37a568, 20 degrees, by
   FUN_0017d5a0) [I: used for the follow points]; both hand weapons; +0x768 |= 0x10.
3. **Friend AI class** FUN_0010b780 (vtable 0x352d88): AI state fields cleared, state function
   `+0xb64/+0xb68` = 0x111070 (pointer-to-member pair), flags +0xb7c = 0x10000000, +0xcd0 = -1; party
   position +0xb70 = 1 or 2 by slot; the AI object at +0xb60 by NeoStatus (section 4); +0xabc |= 2 for
   summons; FUN_00113450 binds the AI class's action records to the parameter block.
4. After creation (vtable +8, FUN_0010c5f8 -> FUN_0017c968): the HUD party panel gets the object
   (FUN_0013dcd8(slot + 1, obj): table 0x355768) unless it is a summon (+0x588 & 0x400000); FUN_00174920.

Where it stands: the spawn record's position and rotation (FUN_0016e8c0 passes the record). The tt02 records
are listed by `kh2-ard`'s disc test.

### Party manager [V]

FUN_00112168 (called from FUN_001b72f0) registers three callbacks through FUN_00102778: 0x111d18 and 0x112018
(arguments 4 and 6, [I] priorities) and 0x112100; it also sets the warp mode to 2 and clears the party list.
[I] Each frame FUN_00111d18 runs:

- FUN_00111c18: `DAT_0034e908[3]` = {player, slot 0 member, slot 1 member}; each member's AI object owner is
  set (FUN_00113448) and +0xb70 = 1-based position; changes go to the HUD (FUN_00135a48).
- copies the player's lock-on target record (+0x9f4..+0xa04) into `0x34e9b0` (style 1 reads it);
- records the player's trail (FUN_0010b270 on 0x3b36c0): a 32-entry ring of positions at 0x3b36d0, a new
  point when the player moved more than 45 units (`DAT_00378480` = 2025 is the squared distance; host 10.57,
  FUN_002f7580, does the same for scripts);
- FUN_00111670 computes the follow points `DAT_0034e920 + slot x 16` (player, member 0, member 1) from the
  player's position and the members' positions. [V, WP-7P1, oracle-checked by
  `party_follow_points_match_fun_00111670`] Two offsets `(0x3785b4 or 0x3785b8, 0, 0x3785ac)` (0.86 / -0.86
  sideways, 0.51 forward) times the member's follow distance (FUN_001115d0: the class's field slow-down
  distance of mode 1 minus `0x3785a8` (0.5) times the body radius +0x5b8, at least `0x3785a4`; doubled in battle
  for style 3) go through the player's matrix: axis rows from +0x6b0, position row from +0x70. Each member
  takes the nearer offset; when both take the same, the one farther from it (the second on a tie) takes the
  other. In the app's drawing convention (local +Z is the facing) the points lie ahead of the player, about 59
  degrees to each side [I: that +0x6b0 matches the app's draw turn].

Area load (FUN_00112270): warp mode `DAT_0034e980` = 1 in world 8 area 8 and world 13 area 3, else 2; host
function at 0x2eec70 (FUN_00112260) sets it from scripts.

### Despawn [V]

- Area change: FUN_00168ba8 calls FUN_0017cc70 (both slots cleared, hidden list count `DAT_0035ff30` = 0).
- Removal (vtable +0x90, FUN_0010c6e0): AI object deleted (its slot +8), weapons released, slot freed
  (FUN_0017c9b8).
- **Hidden members** (summon, drive form, limit): FUN_001c7600(fade frames, NeoStatus) fades the object out
  (FUN_001abf40(1.0, 0, frames, +0x82c); the callers pass 15 frames: `DAT_0037c050` forms, `DAT_0037a564`
  area spawn, `DAT_0037bff0` summons; the member swap FUN_001b3f78 fades over 8, `DAT_0037ba60`), sets +0x7f8
  bit 0x10 and appends its member index (FUN_001a7500: Sora/Roxas 0, Donald 1, Goofy 2, others 3) to the list
  at 0x1d5e380. They come back through FUN_001c79c8 -> FUN_001c7ab0 -> FUN_0017d290 with a fade-in.
- **Hide and show in the app (WP-PY4, `party.rs` `party_hide`, `PartyHidden`, `PartyMember::hide` / `show`).**
  Decompile index: FUN_001c76b0 (per party slot 0 and 1: object exists = FUN_0019a3f8, FUN_001c77a8 on the
  NeoStatus FUN_001699a8, then FUN_001c75d0 with `DAT_0037c050`), FUN_001c75d0 (FUN_001c7600 with the object's
  NeoStatus), FUN_001c7600 (looks the object up by NeoStatus, FUN_0017d400; with an object and bit 0x10 of
  +0x7f8 clear: FUN_001abf40(`DAT_0037c04c` = 1.0, 0, frames, +0x82c); bit 0x10 set; ALWAYS appends
  FUN_001a7500(NeoStatus) to the list, also with no object), FUN_001a7500 (1 and 14 -> 0, 2 -> 1, 3 -> 2, else
  3), FUN_001c79c8 (FUN_001c7a38, FUN_001c7ab0 for every entry, count 0), FUN_001c7ab0 (finds the entry's slot
  1 or 2 through FUN_001a71e8, creates the member with FUN_0017d290(`DAT_0037c058`, `DAT_0037c05c`,
  `DAT_0037c060`, the slot byte at +0x10 of FUN_001a7118's record, 0), then FUN_001abf40(0, `DAT_0037c068`,
  `DAT_0037c064`, +0x82c) when bit 0x10 is clear), FUN_001c7be8 (list test), FUN_001c75c0 (count 0),
  FUN_001c7a38/FUN_001c7bc0, FUN_0017d1b0 (area spawn: `DAT_00340c88` byte 5 summon or FUN_001c77a8 of byte 4
  -> FUN_001c7600 with `DAT_0037a564`, no object), FUN_0017d470. Callers (mips_scan --jal): FUN_001c76b0 only
  from FUN_001b4c28 (the form start); FUN_001c7600 from FUN_001c75d0 and FUN_0017d1b0; FUN_001c79c8 from
  FUN_001b37f8, FUN_001b4630, FUN_001b46b8, FUN_001cdcb8, FUN_001cdd50, FUN_001cdde8. Executable floats
  (read at load): 0x37c04c 1.0, 0x37c050 15, 0x37a564 15, 0x37bff0 15, 0x37ba60 8, 0x37c058 100, 0x37c05c 200,
  0x37c060 300, 0x37c064 15, 0x37c068 1.0.
  What the app does: when `DriveState::form` leaves 0 every member the form takes (the oracle-checked
  FUN_001c77a8 table, `form_takes`) is hidden (`hide`: alpha ramp 1 -> 0 over 15 ticks, not ticked, not hit, its
  fighter not hittable, hidden once the ramp ends) and listed; when it returns to 0 the listed members come back
  (`show`: placed, alpha ramp 0 -> 1 over 15 ticks, ticking again at once) and the list is emptied. An area load
  empties the list (FUN_00168ba8 -> FUN_0017cc70); a member the running form takes is created hidden at an area
  spawn (FUN_0017d1b0; the original makes no object and creates it on the return, the app keeps one hidden object
  so the return is one rule). The summon keeps its own copy of this (`summon::Away`, `CompanionsOut` /
  `CompanionsBack`); `party_hide` skips members that have `Away`.
  Gaps and [inferred]: (1) FUN_0017d290's free-spot search (FUN_001a5ea8: sector search with collision) is not
  ported here; the return point is a random angle with a distance between `DAT_0037c05c` and `DAT_0037c060` of
  the player, a floor looked up under it (8 tries, else the player's place), facing the player [inferred]. (2)
  The consumer of the +0x82c ramp was not read; the ramp is stepped once per tick and clamped at its end
  [inferred]. (3) WP-PY5 wired the limit hide: FUN_001b28a0 (after the MP spend) loops FUN_0017cd08(0..1); for
  each party object (FUN_0019a3f8) with +0x588 bit 0x400000 clear and the byte +0xb4c zero (the byte FUN_001b28a0
  and FUN_001b2c10 store at 0x1b2b4c / 0x1b2d44, FUN_001b3178 / FUN_001b32d8 clear it at the end; the +0x588 bit has no
  `li 0x400000` hit in mips_scan and is taken as clear [inferred]) it calls FUN_001c75d0 with 0 frames (alpha
  to 0 at once, listed). The end (FUN_001b37f8, reached from FUN_001b2850 / FUN_001b35e8 / FUN_001b3798) calls
  FUN_001c79c8 when the thread flag +0x40 bit 0x10 is clear (else FUN_001c75c0 clears the list; the app takes the bit as clear [inferred]). The app:
  `limit.rs` `try_limit` sends `party::LimitMembers { away }` at the start and at the end of the stand-in's
  duration; `party_hide` hides every member without `Away` (`hide(0, ..)`) and, on the end, runs `bring_back`
  (the FUN_001c79c8 path, shared with the drive form's return). FUN_001c7740 (hide both, from FUN_001ad9a0 and
  FUN_001c6168) was not traced further. The stand-in has no limit task, so nobody takes the members' place
  while they are hidden. **The member swap fade (WP-PY6).** FUN_001b3f78 (mode 0, from FUN_001b41e8 <- the task
  FUN_001b4170 that FUN_0017ce60 starts): unless +0x7f8 bit 0x10 is set it runs FUN_001abf40(`DAT_0037ba64`, 0,
  `DAT_0037ba60` = 8 frames, +0x82c) and sets the bit (it does NOT call FUN_001c7600, so the member is not put on the
  hidden list), then yields while FUN_001b80e8 finds the object in the busy list (the fade running) and once more;
  then FUN_0016f968 x3 (releases for the NeoStatus, +1, +2), FUN_001ae100 (party data: the reserve takes the leaving
  member's place), FUN_0016e638, FUN_0016fb20, FUN_001c7c88 (waits for FUN_00196418 / FUN_00179538) and FUN_0017d290 with
  `DAT_0037ba68..70` creating the reserve member. The new member gets no fade-in from this function. FUN_001b4260 ends the
  task (FUN_001a7558 on the party, FUN_001ae060, clears 0x35fec0 bit 2, `DAT_0035f190` = 0). FUN_002a15d0 (called by
  FUN_002514d0 on the menu path) does not call FUN_001b3f78: once per menu (`DAT_0036d3d0`) it rearranges the slot
  bytes with FUN_001a75e8 or spawns a small task (FUN_00102808, 0x2a1790) for the member in slot 3; it was not ported
  (no fade involved). The app: `party_receive_hits` hides the KO'd member (`PartyMember::hide`, 8 frames
  from `HideFades::swap_from`, no list entry, not hittable) and starts `SwapFade`; `party_swap` waits the fade plus one
  tick [inferred: the yield after FUN_001b80e8] and then writes the slot bytes, so `spawn_party` rebuilds the set; a second
  KO's swap waits while one runs (`DAT_0035f190`). Placement: FUN_001b3f78 passes the leaving member's position (FUN_0016b960 of its object) to FUN_0017d290, which uses it when FUN_001a5bf8 accepts the spot, else searches around the player (FUN_001a5ea8) [inferred: accepted]; the yaw comes from the player's direction. The app's rebuild gives the new member that spot (`SwapFade::spot`, taken by `spawn_party`) facing Sora. A swap in progress and its spot are dropped by an area load (`spawn_party`'s non-rebuild branch: jump, game over, Continue), test `the_auto_change_swap_waits_out_the_fade`. Not modelled (gaps): the bit 0x100 of the member's +0x108 and `0x35fec0` bit 2 (readers not traced); the call FUN_001b41e8 makes after the swap, vtable +0x54 of the task's result (`(*(vtable + 0x54))(obj, 0x11, 0)`, whose target was not resolved), so any pad or command effect it has is unknown; a second KO'd member (not the leaver) is rebuilt at full HP by the rebuild, an existing behaviour of `spawn_party`, not of the original. Test: `the_auto_change_swap_waits_out_the_fade`. (4) WP-PY5: `friend_ai::friend_update` skips a hidden member (`PartyMember::runs_script`, test
  `a_hidden_member_does_not_run_its_script`). (5) WP-PY5: `menu_party::rows` locks a slot whose byte's low five
  bits are in the hidden list: FUN_001a7268 returns locked for slot 0, byte bit 0x80, or FUN_001c7be8(byte & 0x1f)
  (the byte is compared with the list's entries as it stands, `hidden_index` values 0..3; test
  `a_member_in_the_hidden_list_is_locked`). (6) A drive form that starts while a summon is
  active, and the FUN_0017d1b0 summon branch (byte 5), are not modelled.
- **Hyper Healing on a revive (WP-PY4).** `PartyMember::change_hp` takes the saved record (`Progress`) and asks
  `PartyData::revive_scale_of(abilities)` = `party_rules::revive_scale` (the word at `DAT_0037a560` when ability
  419 is equipped, else `sstm` +0x54); every field path (revive timer, hits, item uses, script heals) passes it.
  The menu's use (`menu_items::use_item`) passes one scale per field member (`revive_scales`, from each member's
  own saved abilities); a group item (Mega potions) revives member `k` with scale `k` (`revive_at`, WP-PY5, test
  `group_items_revive_each_member_with_its_own_scale`). The revive timer's own Hyper Healing multiplier (+0x58 of
  the block, `sstm` +0x154) stays 1.0 as before.
- **Auto swap** on KO [I: the Auto Change ability]: FUN_0017ce60 calls FUN_001b4170 when the KO'd member's
  ability block word +0x10 has bit 4, FUN_001b42f0 holds and FUN_0017d0f0 says the member is out of the
  party; FUN_001b41e8 -> FUN_001b3f78 fades it out and creates the replacement with FUN_0017d290.

### KO and revive [V]

- KO (friend vtable +0x5c, FUN_0017ce60, reached from the HP method FUN_00186238 -> FUN_00186530 when HP
  becomes 0): FUN_00175f10 (voice line 2, then FUN_00186568 -> FUN_00186580 sets +0x7f8 bit 4); revive
  timer `+0xb40 = (record at +0x12c)+0x18 x ability block +0x58` (Hyper Healing multiplier). **The record
  at +0x12c is the `prty` row of the object's `objentry` NeoMoveset** (WP-P3b): the base constructor
  FUN_00196b58 ends with a tail call `FUN_00194438(obj + 0x110, objentry +0x4e)` (instructions
  0x196cd8..0x196cf8), which stores `FUN_001cc1c8(NeoMoveset)` (= FUN_001cc140 on the `prty` table
  `DAT_01d6a8f8`, loaded by FUN_001cc040 from the name at 0x37c290 "prty", fallback row 1) at +0x12c and
  copies its walk, run, jump, turn (+0x110..+0x11c) and words 8 and 10. +0x18 of the row is StunTime; +0x1c
  (MpDrive) is the MP-charge length FUN_00186440 reads through the same pointer.
- Each frame while +0x7f8 bit 4 (KO) is set (FUN_0017ca70, from vtable +0xc): the timer counts down by the
  frame unit, times `sstm[0x5c]` while the battle flag `DAT_01c8a7d8` is clear; at 0 FUN_0017cc20 heals
  `maxHP x sstm[0x58]` through the HP method (vtable +0x78). While down (motion state 0x36, played with
  FUN_0017e2d8 when the member is free) system effect 0xe follows bone 0x10 (FUN_001db868 handle at +0xb44).
- Revive (FUN_0017cee8 from the HP routine FUN_00186238): KO bit cleared, motion 0x37 if in 0x36, the healed
  amount scaled by 1.0 (ability block +0x10 bit 8) or `sstm[0x54]`.

