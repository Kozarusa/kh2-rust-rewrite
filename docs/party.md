# Party members (Donald, Goofy, the world partners): implementation spec

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
  (FUN_0010fcd0, FUN_0010f220). [I] these are the party menu's Customize settings.
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

## 4. The friend AI

### Classes [V, FUN_0010b780; oracle-printed thresholds]

| NeoStatus | AI class (index into 0x34ea00) | constructor | AI vtable | action records |
|---|---|---|---|---|
| 2 Donald (moveset 0x12 `P_LK020` -> 12) | 0 / 12 | FUN_00114550 | 0x353368 | 0x353090, 20 |
| 3 Goofy (moveset 0x15 `P_LK030` -> 13) | 1 / 13 | FUN_00114668 | 0x353630 | 0x3533a0, 18 |
| 5 Auron, 15 Ping, 6 Mulan, 7 Aladdin, 8 Sparrow, 9 Beast, 10 Jack, 11 Simba, 12 Tron, 13 Riku | 2, 3, 4, 5, 6, 7, 8, 9, 10, 11 | FUN_00114890, FUN_001149c8, FUN_00114ae0, FUN_00114c38, FUN_00114d90, FUN_00114f90, FUN_001150e8, FUN_00115260, FUN_001153b8, FUN_001154d0 | per class | per class |
| 200..204 summons | 14..18 | docs/summons.md | | |
| 1, 4, 14 | none | | | |
| anything else | class with the table's first block | FUN_001133e0 (no actions) | 0x352e78 | none |

The AI object (0x14 bytes, Goofy 0x30): +4 owner, +8 parameter block, +0xc action count, +0x10 action
records. In the base AI vtable (0x352e78, kept by Donald's and Goofy's) the hooks +0xc (on hit), +0x14
(after create), +0x18, +0x2c and +0x30 (on warp) are empty functions (0x114138, 0x114148, 0x114150, 0x1141c0,
0x1141c8, decompiled); +0x28 (FUN_00114170, host 10.3) ends a link to a carrier object (+0x570) and puts
the member back on the ground. +0x10, +0x1c..+0x24 (0x114140, 0x114158..0x114168) were not read.

### Parameter block (0x34ea00 + offset[class]) [V for the readers; values printed by the oracle test]

| offset | reader | use |
|---|---|---|
| +0x00..+0x5c | FUN_00114078 (down), FUN_001140c0 (up) | follow-speed thresholds: 3 rows (field; battle following the player; battle other target) x 3 speed modes x {slow-down distance, speed-up distance}. Donald: field `0/200, 160/240, 200/1000`, battle-other `0/0, 160/240, 200/1000`. Beast `0/300, 260/340, 300/1000`; Genie `0/300, 270/360, 330/1000`; Chicken Little `0/160, 120/200, 160/1000`. Oracle-checked |
| +0x60 | FUN_00114038 | MP-use chance factor (FUN_00110f90) |
| +0x64 | FUN_00114048 | combo-continue chance (Donald 0.1, Goofy 0.5) |
| +0x70, +0x74 / +0x78, +0x7c | FUN_0010f648 from actions 0x1135e0 / 0x113728 | random wait range in frames of the two idle actions (Donald 6..60 and 6..180; Goofy 6..60 and 600..900) [I: FUN_00106408 = random in range] |
| +0x80 | FUN_00114108 | battle distance from the player (FUN_0010df08), x2 for styles 3 and 5, x`DAT_003784ac` (0.5) for style 2 |
| +0xa0, +0xa4 | FUN_00113450 | count and offset of the 0x80-byte action parameter entries |
| +0xa8, +0xac | FUN_00113588, FUN_001135a0 | count and offset of 0x20-byte combo follow-ups (FUN_0010c800) |

Action parameter entry (0x80 bytes): +0 id (matched to the action record's +0xc), +4 motion request
(FUN_00110790 -> FUN_0017e220; Donald 0x97, 0x98 ground, 0xb5, 0xb6 air), +8 MP cost (Donald's spells 10, Cure
0xff = all; FUN_0010c8f0 keeps it at +0xc35), +0xc flags (0x10 = no facing test, FUN_0010f220), +0x10..+0x1c
[I] range and height limits (FUN_0010f118, not read), +0x20 height of an air attack (FUN_00110790), +0x3c
the HP fraction under which Cure is wanted (FUN_00113e68; 0.4 for Donald).

Action record (0x24 bytes): +0 flags (0x200 continuous, 0x10 emergency pass 1, 0x20 emergency pass 2, 0x8,
0x2 attack, 0x1 "not counted as repeat"), +4/+8 handler (pointer-to-member), +0xc id, +0x10 parameter entry
(bound at creation), +0x14 ability bit, +0x18 ability item (from 0x352ec0), +0x1c setting index, +0x20 BDX mode
name. Handlers are called with phase 1 = "can I?" (fills the target) and phase 0 = "do it".

| handler | records (Donald) | what it does |
|---|---|---|
| FUN_001135e0 | 0x201 | stand near the target / move to it, then wait a random +0x70..+0x74 frames |
| FUN_00113728 | 0x201 | wait a random +0x78..+0x7c frames ([I] idle) |
| FUN_00113888 | 0x18 | HP item: targets with HP < `DAT_003785c8` (0.45) x max among self, the player, the other member; item from 0x352e28 (one target) or 0x352e3c (several), each needing setting < 2 (or an emergency) and stock (FUN_0010f510 -> FUN_001a1d50); executes FUN_0010c9d0 (item motion state 9) |
| FUN_00113b08 | 0x28 | MP item: the same with MP < 0.45 x max (`DAT_003785d4`) and lists 0x352e50 / 0x352e60 |
| FUN_001137d8 | 0x2 (ids 0..3, 0x31..0x34) | melee or air attack with its parameter entry; feasibility FUN_0010f220; executes FUN_001103f8 + FUN_0010c8f0 |
| FUN_00113d48 | 0x2 (Fire, Blizzard, Thunder) | special: switches the member's BDX mode to the record's name (FUN_00186e70 on +0x300) with +0xaf4 = the entry's MP byte |
| FUN_00113e68 | 0x10 (Cure) | as FUN_00113d48, only when someone's HP < entry +0x3c x max |
| FUN_00114738, FUN_00114828 | Goofy | Goofy's variants of the idle and special handlers |

Goofy's specials are `tornado`, `blade`, `turbo` (ids 0x5c..0x5e, abilities 423, 429, 425); Donald's `fire`,
`blizzard`, `thunder`, `cure` (0x39..0x3c, abilities 165..168). The same modes are registered by the members'
scripts (`kh2 bdx P_EX020`: `fire, blizzard, thunder, cure`; `P_EX030`: `tornado, turbo, blade`).

### The decision loop [V]

Per frame the friend object's update (vtable +0x14, FUN_001112e8) calls the state function
`+0xb64` (FUN_00111070 for party members; Stitch 0x1155e0, Chicken Little 0x11a2e8), then FUN_0016a288 and, when
the member is free, the queued command (`+0xc30`: kind 1 attack/item -> FUN_00174cf8; a jump request
0x200000000 -> FUN_0010c748; else FUN_00110eb0 = warp check).

FUN_00111070, while the member is free (+0x160 bits 4/8/0x10) or forced (bit 0x40000000):

1. A KO'd member of class 12 (Pride Lands Donald) in motion controller state 0x1f..0x20 is reset
   (FUN_0016b698(obj, 0)).
2. FUN_0010df08: refresh the relation to the player at `+0xb80` (FUN_0010b428: +0 `dy` = target Y minus own
   Y, game Y pointing down; +0x10 unit direction and +0x1c distance in 3D; +0x20 unit direction and +0x2c
   distance on the ground plane) and, in battle, the target (+0x9f4) by the battle style (section "Targets").
3. Far counter `+0xcd8` += 1 for each of: object +0x660 bit 4, the player riding (+0x810), a vertical
   difference to the player above 400 (`|dy|` > `DAT_003785a0`).
4. Out of battle (battle flag `DAT_01c8a7d8` clear) or forced: **follow** (FUN_001102c8 -> FUN_0010e5e8).
   In battle: FUN_00110f90 (MP gate: +0xb7c bit 0x20 when random(0..1) <= block+0x60 x (0.75 x MP/maxMP +
   0.25)), FUN_0010f830 (the member in danger, `+0xce4`: the player, else self, else the other member, by
   FUN_001a0be0), then **emergency** FUN_0010f8d8 and **choose** FUN_0010fcd0.
5. Unless +0xb7c bit 0x80: **steer** FUN_00110468; FUN_00140cd8; velocity +0xc00 copied to +0x10.

"HP in danger" (FUN_001a0be0): `HP <= maxHP / 4` (integer division). "Below fraction" (FUN_0010f490 /
FUN_0010f4d0): `value < int(max x fraction)`. Both oracle-checked.

**Follow** (FUN_0010e5e8 with the follow vector): direction to `DAT_0034e920 + position x 16`, normalised into
+0xc00. Speed mode `+0xb78` 0..2 with hysteresis: distance above the mode's speed-up distance -> mode + 1 (max
2); below its slow-down distance -> mode - 1 (min 0); thresholds from the block (above), doubled in battle
for style 3. The distance compared is the ground-plane one (+0xbac). Speed = 0, walk (+0x110) or run
(+0x114) (FUN_0010e5b8). Then a ground probe ahead (FUN_0010e4b8 -> FUN_001834f8 at 0.1, 0.9 and 0.5 x the
object's +0x118, raised by `dy` when positive) and a wall test (FUN_001439f8); a blocked path asks for a jump
(FUN_0010c8d0 kind 3). Ground distance < 8 with `dy` < -250 (the player far above) stops and counts as stuck.
When the member rides (+0x810) and the player does not, and `dy` > 80: jump.

**Steer** (FUN_00110468): the wanted velocity is averaged over the last six frames (ring at +0xc70), then the
speed snaps: below 0.5 x walk -> 0, below 0.5 x (walk + run) -> walk, else run.

**Warp (catch-up)** (FUN_00110eb0 -> FUN_00110a58 / FUN_00110e28): allowed when `DAT_0034e980` is 2, or 1
and no battle. When the warp cooldown +0xcdc <= 0 and the far counter +0xcd8 > 39, a point of the player's
trail is chosen (FUN_00110a58: a random start, candidates tested by FUN_001108c8: FUN_00146648 on the view data at 0x3b38d0
returns 0 ([I] not on screen), the player not +0x160 bit 0x4000, the candidate's height offset of the same
sign as `dy`, an absolute value below 1000 (`DAT_00378580`) and a value above -0.5 (`DAT_00378584`) [I: a
distance and a direction test; FUN_001108c8 was read only in outline]); the member is put there (FUN_0016a700), fades in over 30 frames (FUN_00175aa8,
`DAT_00378590`), counter 0, warp count +0xce0 + 1, cooldown 480 frames (`DAT_00378594`). Host function at
0x2eec20 (FUN_00110e08) forces it (counter 1000, flag 0x8000). [I] the far counter's decay; only its
increments and the reset after a warp were read.

**Choose** (FUN_0010fcd0), not while the script is busy (+0x390):

1. The emergency pass (FUN_0010f8d8) runs first when someone is in danger: records with flag 0x10, then
   0x20, each tested; a random feasible one (FUN_0010f420 = random below n) is executed.
2. A running continuous action (flag 0x200) is re-tested and kept.
3. After an attack (flag 2) that can chain (FUN_0010f3d8): with chance block+0x64 a random combo follow-up.
4. Otherwise, if the last choice was not repeated twice (+0xce8 < 2) or +0xb7c bit 0x800 (summons): pass 0
   tests every record whose setting is not 2 (and not MP-charging when it costs MP, and with setting 1 only
   while MP >= cost); pass 1 only setting-2 records. One feasible record is picked at random; the action
   chosen three times in a row (+0xc54/+0xc58/+0xc5c, FUN_0010fc68) is skipped once. Nothing feasible: stop
   moving (FUN_001103f8), clear the speed.

Feasibility (FUN_0010f220): the member stands (motion controller state 0 or 0x1f) and is free; an
ability-gated action needs the ability equipped (FUN_0010f578) and a setting other than 3; an MP action needs
the MP gate bit 0x20 and no MP charge; an attack needs a valid target (FUN_0010ca88) in range (FUN_0010f118)
and in front (dot >= 0) unless entry flag 0x10.

### Targets (battle style = setting byte 0) [V]

FUN_0010dcf0 picks the target by style through the table at 0x352d28 (re-target test 0x352cd0):

| style | choose | re-target when (FUN_0010cf90, also always when the target is not valid) |
|---|---|---|
| 0 | FUN_0010d0c0 [GAP: not read] | FUN_0010cbe0: an attacker is remembered (+0xcec) and is not the target, or the target is farther than FUN_00114068's range |
| 1 | FUN_0010d270: FUN_0010d848's pick, else the player's target (`0x34e9b0`) | FUN_0010cc80 [GAP] |
| 2 | FUN_0010d378: the nearest enemy within FUN_00114058's range of the member and within FUN_00114118's range of the player | FUN_0010ccf8 [GAP] |
| 3 | FUN_0010d538: among enemies within 2x range, the one farthest from the player | FUN_0010ce40 [GAP] |
| 4, 5 | FUN_0010d700 / FUN_0010d708 (short) | FUN_0010cf80 (always) / FUN_0010cf88 (never) |

Battle distance (FUN_0010df08): when the member's ground distance to the player (+0xbac) exceeds block +0x80
(x2 for styles 3 and 5, x0.5 for style 2), +0xb7c bit 0x40 is set, and a target with that bit set is not valid
(FUN_0010ca88): the member drops its target and follows (the follow step clears the bit). The target is
refreshed only in battle, when the member is not busy (+0x390), has no script-set target (+0xb7c bit 0x10000,
host 10.14) and the re-target test asks for it. During a limit (FUN_001b3c00) FUN_0010d700 is used. Hits on
the member remember the attacker in +0xcec
(FUN_001106b8, when FUN_00112ac0 accepts it). [GAP: FUN_0010d0c0, FUN_0010d848, FUN_00112b28, FUN_00112b88
not read; the style names are not known]

## 5. Host group 10 (table 0x371810, 60 entries) [V for the wrappers]

`obj` = the calling script's object; "AI" = its friend AI object (FUN_0010c5f0 = obj +0xb60).

| id | native | args/ret | what it does |
|---|---|---|---|
| 10.0 | 0x2f6a90 | 0 | nothing |
| 10.1 | 0x2f6a98 | 2 | FUN_001175a0(AI, a) (Stitch class) |
| 10.2 | 0x2f6ad0 | 1 ret | vector FUN_00117650(AI) (Stitch) |
| 10.3, 10.4 | 0x2f6b28, 0x2f6b58 | 1 | AI vtable +0x28 / +0x2c |
| 10.5 | 0x2f6b88 | 1 ret | vector FUN_0011a6a0(AI) |
| 10.6, 10.7 | 0x2f6be0, 0x2f6c00 | 2 / 1 | FUN_001972e8(obj, a), FUN_001973f8(obj) |
| 10.8 | 0x2f6c20 | 1 ret | vector FUN_00112538(obj) |
| 10.9 | 0x2f6c68 | 2 | Genie: FUN_00119c18 (form byte) |
| 10.10 | 0x2f6ca0 | 1 ret | FUN_00119ca8(AI) (Genie) |
| 10.11 | 0x2f6cd8 | 3 | FUN_0016bea0(c, obj, b) |
| 10.12 | 0x2f6d00 | 8 ret | FUN_00117708: add an attack slot to the AI (flags 1/0x21 self, 2 the player, 8) |
| 10.13 | 0x2f6d68 | 2 | FUN_001195e8(AI, a) |
| 10.14 | 0x2f6da0 | 3 | set the target record (+0x9f4): 1 from an argument, 2 the player's target, 3 FUN_0010d720; +0xb7c bit 0x10000 set unless 0 |
| 10.15 | 0x2f6dc8 | 3 | FUN_0017e390 motion request on the controller +0x140 |
| 10.16 | 0x2f6df8 | 2 | FUN_00117840(AI, a) |
| 10.17, 10.18 | 0x2f6e30, 0x2f6e48 | 1 ret | +0x588 bit 0x800 / 0x400 |
| 10.19 | 0x2f6e60 | 1 ret | +0x10c bit 0x10000 (in a limit) |
| 10.20 | 0x2f6e80 | 1 | follow update now (FUN_001102c8) and copy velocity |
| 10.21 | 0x2f6ef0 | 1 | FUN_001103e0 and copy velocity |
| 10.22 | 0x2f6f60 | 1 ret | motion controller flag 0x40 |
| 10.23 | 0x2f6f88 | 2 ret | AI's +0xae4 == other object |
| 10.24 | 0x2f6fb0 | 2 ret | ability equipped (FUN_0010f578 by bit) |
| 10.25, 10.26 | 0x2f6fe8, 0x2f7020 | 1 | Tinker Bell / summon helpers FUN_0011a1a8, FUN_0011a1d0 |
| 10.27 | 0x2f7070 | 1 | FUN_0010f6e0 |
| 10.28 | 0x2f7090 | 1 | leave motion state 0x1f (swap controller) or set +0xb7c 0x4000 |
| 10.29 | 0x2f70b0 | 4 | use an item on a target (as FUN_0010c9d0) |
| 10.30 | 0x2f70e0 | 1 ret | controller flag 0x40 and +0x10c bit 0x20000 |
| 10.31 | 0x2f7118 | 1 ret | +0x588 bit 0x400000 (is a summon) |
| 10.32 | 0x2f7138 | 2 | FUN_00118aa8(AI, a) |
| 10.33 | 0x2f7170 | 1 ret | FUN_001127f0 and remove it from the list at 0x34e988 |
| 10.34 | 0x2f71d0 | 2 | FUN_00112668(a, b != 0) |
| 10.35 | 0x2f71f0 | 2 ret | FUN_001141d0(AI, a) |
| 10.36 | 0x2f7230 | 1 ret | free (+0x160 bits 4/8/0x10) |
| 10.37 | 0x2f7270 | 1 ret | +0x160 bit 0x40000000 |
| 10.38 | 0x2f7290 | 1 ret | FUN_0018cdf8 |
| 10.39 | 0x2f72c0 | 1 | clear the current action (FUN_0010ecb0(0, 0)) |
| 10.40 | 0x2f72e8 | 2 ret | FUN_0017eb48 on both controllers |
| 10.41 | 0x2f7328 | 1 | FUN_001129f0 |
| 10.42 | 0x2f7348 | 1 ret | FUN_00112a08 |
| 10.43 | 0x2f7378 | 2 | +0xaf4 byte = a |
| 10.44 | 0x2f7390 | 1 ret | controller flag 0x10 |
| 10.45 | 0x2f73b0 | 1 ret | status +0x1b4 (gauge timer) |
| 10.46 | 0x2f73c0 | 1 | FUN_00113190 (Chicken Little setup) |
| 10.47, 10.48, 10.49 | 0x2f73e0.. | 1 | +0xb7c clear 0x1000 / set 0x20000 / clear 0x20000 |
| 10.50 | 0x2f7440 | 1 ret | status +0x180 (MP) |
| 10.51 | 0x2f7450 | 4 | FUN_0011a6d8 |
| 10.52 | 0x2f7490 | 1 ret | FUN_00185a38 |
| 10.53, 10.54 | 0x2f74c0, 0x2f74f8 | 2 / 2 ret | FUN_00136388 / FUN_001364d0 on FUN_0010c5b8 ([I] HUD) |
| 10.55 | 0x2f7540 | 2 | FUN_002f0758(a, b != 0) |
| 10.56 | 0x2f7560 | 2 | FUN_00110dc8 |
| 10.57 | 0x2f7580 | 1 ret | push a position to the player trail (45-unit spacing) |
| 10.58 | 0x2f7598 | 3 | FUN_001131e8 |
| 10.59 | 0x2f75c0 | 2 | FUN_0011a6b8(AI, a) |

Use in the members' scripts (`kh2 bdx`): Donald 10.31 x2; Goofy 10.14, 10.31, 10.40, 10.55 (x2 each);
Auron 10.29, 10.31 x2, 10.55; Riku 10.14 x4, 10.17, 10.18, 10.29, 10.31, 10.35, 10.38, 10.39, 10.43, 10.53,
10.54, 10.58; Tron 10.31 x2, 10.35 x4, 10.40 x2, 10.55. The members' scripts use mostly groups 1 to 4
(69..92 distinct host functions each), so the scripts' own host needs are the larger cost of stage 2.

## 6. Combat

- **Class methods** [V]: the friend vtable 0x352d88 keeps the party class's combat methods (shared with
  Sora's vtable 0x35c9a8): +0x6c clash FUN_001754a0 (docs/hit-reaction.md, ported as
  `reaction_rules::party_clash`), +0x70 FUN_00175668, HP change +0x78 FUN_00176c98 -> FUN_00186238. Overrides:
  +0x64 receive FUN_001106b8 (AI hook, FUN_0017cde0 = FUN_00175340 the `patn` receive path as for Sora, voice 9
  when HP enters danger, attacker remembered), +0x74 attacker callback FUN_00110728, +0x7c attack motion
  FUN_00110790, +0x84 FUN_001115a0 (FUN_00174c28 unless +0xb7c bit 0x400).
- **Attacks**: the member's motion set's attack volumes with `atkp` rows, as for any object (combat.rs);
  the attacker's stats from its status block. [GAP: FUN_00110728 not read]
- **Invulnerability**: +0x7f8 bit 0x10 (hidden), the party tick FUN_00174b68 timers (docs/hit-reaction.md
  +0xb00) apply. [I] events: not traced.
- **Reaction commands**: the party code offers limits through FUN_0019d430 -> FUN_001d25e0 (docs/player-data.md
  "Triggering"); members within `DAT_0037c414` with flag 2 qualify.
- **Limits**: FUN_001b28a0 marks participants (+0xb4c, +0x10c 0x10000) and puts both companions in the limit
  state (FUN_001c75d0) -> hidden list; target choice switches to FUN_0010d700 during a limit.
- **Drive forms**: FUN_001b4c28 -> FUN_001c76b0: each member the form takes (FUN_001c77a8: Valor Goofy,
  Wisdom Donald, Master/Final/Anti everybody, oracle-checked) is faded out and listed (15 frames,
  `DAT_0037c050`); the area spawn skips them while the form lasts (FUN_0017d1b0).
- **Summons**: docs/summons.md (fade 15 frames out and back, FUN_001c7740 / FUN_001c79c8).

## 7. HUD and audio

- HP / MP panel per member: registered at creation (FUN_0013dcd8(slot + 1, obj) on the table 0x355768); the
  panel reads the member's status block (HP +0, max +4, MP +0x180, max +0x184; the rules in docs/hud.md
  "Party panels", 0x154818 / 0x154930). Summons get none.
- Voices: friend vtable +0x54 (line ids 2 KO, 9 danger); the bank by NeoStatus (docs/audio.md). Positional
  sound effects of the members' motions use their `.apdx` banks.

## 8. Where it goes in the app

| file | function / item | change |
|---|---|---|
| `crates/kh2-tables/src/player.rs` | `Memt` | name the record fields (world, flag, not-flag, budgets, 16 members) and the presets; `merged(world, flags)` and `slot_object(byte)` as in the tests |
| new `party.rs` (kh2-app) | `PartyPlugin` (third plugin tuple of `main.rs`) | `PartySlots` (the four slot bytes per world, hidden list), `PartyMember` component (slot, position 1/2, NeoStatus, AI class, follow mode +0xb78, far counter, warp cooldown, revive timer, last three actions) |
| `party.rs` | spawn | resolve FRIEND_1/2 (FUN_0017cd20), refuse in forms/summons, load model/mset/apdx/BDX, weapons |
| `party.rs` | `party_ai_tick` | the decision loop of section 4, one function per FUN (follow, steer, warp, choose, emergency) as pure rules plus a system |
| new `party_rules.rs` | pure rules | `in_danger`, `below_fraction`, `form_takes`, `apply_preset`, follow-mode hysteresis, steer snapping, the action-choice passes; unit tests against the oracle routines |
| `enemy_host.rs` | group 10 | the 60 entries of section 5 (most are one field each) |
| `hud_party.rs` | `PartyMembers` | fill from the members' status blocks |
| `limit.rs` | `LimitParty` | replace the stand-in with the real party (positions, distance `DAT_0037c414`, flag 2) |
| `drive.rs` | form start/end | hide/show members (FUN_001c76b0), fade 15 frames |
| `combat.rs` / `player_reaction.rs` | receive path | members take hits through the same party handlers (FUN_001754a0, FUN_00175340) |

## 9. Staged plan

Each stage ends with `verify.ps1` passing and the named check.

1. **Spawn, idle, follow, warp (no combat).** memt merge and slot bytes (default preset 1); spawn
   `FRIEND_1`/`FRIEND_2` in tt02 at their records with model, motion set, weapons; the member's BDX attached
   and put in `idle`; follow (section 4 Follow + Steer) with the class's thresholds and the walk/run speeds;
   the player trail and the warp (cooldown 480, counter > 39). Accept: `KH2_PARTYLOG=1` logs "spawn
   P_EX020 slot 0" / "P_EX030 slot 1", the speed mode changes, and "warp" after Sora runs away; a screenshot
   with Donald and Goofy behind Sora (look at it).
2. **Attacks.** Target choice (style 0 needs FUN_0010d0c0, read it first), the action records and parameter
   entries, FUN_0010f220 feasibility, the attack command (+0xc30) and motion requests, the anti-repeat rule,
   combo follow-ups. Group 10 entries the scripts reach. Accept: log of chosen actions per second in tt04
   b_44; enemies take hits from members (KH2_HITLOG).
3. **Get hit, KO, revive, HUD.** Receive path, voice 2/9, KO timer and revive heal, effect 0xe while down,
   `PartyMembers` filled. Accept: log "KO", "revive"; screenshot of the two party panels with HP arcs.
4. **Healing and items, MP.** Emergency pass, HP/MP items (stock and settings), Donald's Cure mode, the MP
   gate. Accept: a run with Sora at low HP logs the item/Cure choice.
5. **Limits with partners.** `LimitParty` from the real party; members hidden during the limit.
6. **Drive forms and summons.** Hide/show with fades (FUN_001c76b0, FUN_001c7600, FUN_001c79c8); spawn refused
   while a form lasts; summon fade.

## 10. Unknowns (with what was searched)

- (Resolved by WP-7P1: the follow-point geometry, see section 3 and section 11.)
- Style 0's target choice FUN_0010d0c0, FUN_0010d848, the range test FUN_0010f118, FUN_00112b28/b88.
- (Resolved by WP-P3b: the record at +0x12c is the `prty` row of the objentry NeoMoveset, section 3; the
  earlier search read the rows by NeoStatus, whose StunTime is 0.)
- (Resolved by WP-P3b: the walk/run speeds +0x110/+0x114 are written by FUN_00194438 from the same row.)
- Frame-trigger frames [inferred, WP-P3b]: in the members' motion sets every frame trigger lies inside the
  motion's length counted in 60 Hz ticks from the motion's FIRST frame (Donald's motion 9 runs frames
  15..55 at 30 fps with trigger 6 at tick 14, Goofy's 12..48 with trigger 6 at tick 19, Goofy's 0x37 88..142
  with triggers 0 and 76; printed by `party_ko_data_from_the_disc`). The app's trigger readers (`audio`,
  `items`, `fx`, `enemy_ai`, `magic`) count from frame 0, so for such motions they never fire; `party`'s
  item use and `friend_ai`'s trigger bytes for the members' scripts (WP-P3c) count from the start; this is kept
  party-local (the other readers are outside the party's files and are not changed). Not checked against the
  game's motion controller.
- (Resolved by WP-P3c) Cure's heal and Donald's specials, section 14.
- The members' ability block (+0x1cc) is not built: Hyper Healing (+0x58), MP Rage (+0x54), the revive
  scale bit (+0x10 bit 8), Item Boost and MP regeneration read their defaults.
- Members are hit only by chance (the enemies aim at Sora); the attacker memory +0xcec (FUN_001106b8) is not
  kept, and the party panel's hit flashes (FUN_0013dd20 / 0013dd70 / 0013ddc0 from FUN_00175340) are not
  drawn.
- The far counter +0xcd8: increments (FUN_00111070, FUN_0010e5e8) and resets (FUN_00110eb0, FUN_00110e28)
  were read; no decrement was found in FUN_00110a58, FUN_00110e08, FUN_00110e28, FUN_00111070. WP-7P1 scanned
  every immediate access (`mips_scan --store-off 0xcd8`): the only other writer is the small routine at
  0x111ae0 (not decompiled), which clears the counter, sets the steer ring index +0xcd0 to -1 and puts the member
  on its follow point (FUN_0016a700); its callers were not traced. The warp cooldown +0xcdc counts down by the
  frame unit `DAT_00356f40` in the friend update (0x1113dc..0x111400, inside FUN_001112e8).
- What +0x660 bit 4 is (FUN_00111070 counts it toward the warp): +0x660 is a copy of the high word of the
  movement's collision result +0x600 (FUN_00182690 line `+0x660 = +0x604`); bit 4 of it is bit 34 of that result,
  the same bit the warp's spot test rejects (FUN_001108c8 after FUN_00143840). Which collision sets it was not
  read; the app counts a tick whose step was blocked [I].
- Attacker callback FUN_00110728, the combo follow-up FUN_001b0758, item use FUN_001a1fc0 fields.
- The meaning of the settings' values 0..3 and styles 0..5 in the party menu (FUN_00112308 readers only).
- Who writes the save slot bytes at new game; `lvup` table order beyond Sora.
- Per-class constructors of the world partners (FUN_00114890..FUN_001154d0) and their action records were not
  dumped; the oracle test prints only their thresholds.

## 11. Status: stage 1 in the app (WP-7P1)

Files: `crates/kh2-app/src/party_rules.rs` (pure rules, constants read from the executable at run time),
`party.rs` (spawn and tick), `friend_ai.rs` (the members' scripts). Log `KH2_PARTYLOG=1`; harness
`KH2_NO_PARTY=1`, `KH2_PARTY_WARP=<s>` (the forced warp of FUN_00110e08), `KH2_PARTY_AT_RECORD=1` (Sora starts at
the player's row of the friends' map group: the app otherwise starts him at its own spawn point, outside the
walls of tt02's Usual Spot).

Done:

- Slot bytes: preset 1 applied to an empty party (FUN_001a7330), `FRIEND_1` = slot byte 1, `FRIEND_2` = byte 2,
  resolved through the world's memt merge (FUN_0017cd20). In Twilight Town: `P_EX020` and `P_EX030`, AI classes 0
  and 1 (`party_tt02_friends_are_donald_and_goofy`). The rows are the first `FRIEND_1` / `FRIEND_2` rows of the
  area's `map` script (tt02 `m_00`); the entrance byte is not tested (the app has no entrances).
- Model and motion set (objentry `AnimationName`); player-type motion sets (4 slots per motion id).
- The member's script attached, init run, mode `idle` set and run each tick (`friend_ai.rs`). The idle mode
  (`P_EX020` R0489 -> R0499) faces the target and asks for motion 0; that request is logged and not carried out
  (the player-type remap FUN_0017da30 is not ported), so the mode ends after a frame.
- Follow: FUN_0010e140 in the friend's motion state 0x1f (FUN_00111070 keeps a free member there), which is the
  path the field follow takes; FUN_0010e5e8's own body (state 0) is not ported. Corrections to section 4: in this
  path the distance the speed modes compare is the 3D one (+0xb9c), and while the turning bit is set the
  direction is the one to the player (+0xb90), not to the follow point (oracle-checked,
  `party_follow_step_matches_fun_0010e140`). Steer FUN_00110468 (`party_steer_matches_fun_00110468`), trail
  FUN_0010b270 (`party_trail_matches_fun_0010b270`), follow points FUN_00111670 (section 3).
- Warp: FUN_00110eb0 / FUN_00110a58 / FUN_001108c8 / FUN_00110e28: due when the cooldown is out and the far
  counter is above 39; the member must be off screen; trail points tried from `(warps + 0x1c + random(4)) & 31`
  (absolute ring indices, 0x110ccc), eleven, three apart; forced (flag 0x8000) takes the accepted point nearest
  the player. Fade-in 30 frames, cooldown 480, counter 0, warps + 1; a new trail point clears the warps.

Inferred or approximated in the app: the walk/run/turn speeds from the `prty` row of the NeoStatus; the facing
turns toward the movement (toward the player while turning); the motion is the motion id of the speed (0, 1, 2)
in the field state; the screen test (FUN_00146648) is the camera frustum on a box of the body shape; the spot
test of FUN_001108c8 is "a floor under the point"; +0x660 bit 4 is "the last step was blocked"; the random
generator is not the game's.

Not done in stage 1 (stage 2 below did the weapons): the weapons. FUN_0017c718 attaches the hand objects through FUN_0016ef28 (the member's
saved weapon word, `Progress::member`) -> FUN_001845b0 (`went` of the moveset) -> FUN_001751b0 -> FUN_00184190,
which ends in resource handles, not a bone number; the attach bone of Donald's and Goofy's models is not known
(objentry `WeaponJoint` is 2 and 3, not a bone). Sora's bone 178 was set by looking, which this WP did not do.
Also not done: members do not push each other or Sora apart (nothing in the paths read does it); the spawn
refusals during a form or a summon (stage 6); battle (the members follow in battle too until stage 2).

## 12. Status: stage 2 in the app (WP-7P2)

Files: `crates/kh2-app/src/party_battle.rs` (pure rules of the battle AI), `party.rs` (the battle branch of the
tick, weapons, push-apart), `friend_host.rs` (the members' script host: group 10 and the motion remap),
`friend_ai.rs` (runs the scripts through it). Log `KH2_PARTYLOG=1`: "battle flag set", "target", "chose record",
"attack motion", "actions in the last second", "hit". Harness `KH2_PARTY_WARP_ONSCREEN=1` (with
`KH2_PARTY_WARP=<s>`) lets the forced warp happen in view, `KH2_PARTY_WEAPON_BONE=<n>` overrides the weapon bone, `KH2_PARTY_WEAPONS=<Donald item>,<Goofy item>` hands an item in for
a zero weapon word.

Done:

- **Battle gate** (FUN_001903d8): the tick reads the game's battle flag (`audio_bgm::BattleFlag`, word
  0x1c8a7d8). Clear: the stage-1 follow. Set: the battle branch of FUN_00111070.
- **Action records** (`ClassAi::from_exe`): the class's block (+0x60..+0x80, +0xa0/+0xa4 entries, +0xa8 follow-ups)
  and the records of Donald (0x353090, 20, from FUN_00114550) and Goofy (0x3533a0, 18, FUN_00114668), bound to
  their 0x80-byte entries by id and to the ability table 0x352ec0 / item order 0x352eb0 as FUN_00113450 does.
  Printed by `party_action_records_from_the_executable`.
- **Range** FUN_0010f118 (oracle-checked over 432 cases, `party_range_matches_fun_0010f118`), **feasibility**
  FUN_0010f220, **anti-repeat** FUN_0010fc68 / FUN_0010ed00, **choice** FUN_0010fcd0 (continuous records kept,
  setting-2 records only in the second pass, the triple repeat skipped once, nothing feasible = stand).
  Handlers read for phase 0/1: FUN_001137d8 (attack: FUN_001103f8 then the command FUN_0010c8f0), FUN_001135e0
  (stand facing the target, wait +0x70..+0x74), FUN_00113728 (move, wait +0x78..+0x7c), FUN_00114738 (Goofy:
  run the way it started); the waits count down in FUN_001112e8 (+0xcd4). The idle handlers refuse while any
  attack is feasible (FUN_0010ed20).
- **Target, style 0** (FUN_0010d0c0, read): the remembered attacker's nearest shape, else the enemy hurt shape
  whose surface (centre distance minus radius) is nearest and within the block's +0x68; re-target
  (FUN_0010cbe0) when the target is invalid, the attacker differs, or the target is beyond +0x6c. Battle
  distance (FUN_0010df08): farther than +0x80 from the player makes the target invalid (bit 0x40).
- **Moving in battle**: toward the target FUN_0010e5e8's body with the relation +0xbb0 (row 2 thresholds,
  ground distance; the stop when the target is right above, 0x378564 / 0x378568); following the player with
  row 1.
- **Attack command** (FUN_001112e8 -> FUN_00174cf8 -> FUN_00110790): the entry's motion through the player-type
  remap FUN_0017da30 (battle row 1 of the table 0x37a5c8, checked equal to `kh2_anim::PlayerState`'s orders);
  an air entry (+0x20 != 0) jumps `+0x20 - dy` (held between 0 and the jump height) toward the target at the
  run speed. The member is busy until the motion ends on the ground. Its attack volumes hit enemies through
  `combat` (a `Fighter` of the player team; not hittable before stage 3).
- **Motion remap for the scripts** (follow-up 2): `friend_host` remaps the scripts' motion requests
  (FUN_0017e000 -> FUN_0017da30) before the enemy host plays them; the requested slot plays while the member
  stands (the `idle` mode's motion 0).
- **Group 10** (`friend_host`): 10.14 (target bit; record copy not carried out), 10.19, 10.31 (false: not a
  summon), 10.36, 10.40 (false: no linked controllers), 10.55 (bit 16 of the argument's word +4, read from
  FUN_002f0758). Wrappers 0x2f6da0, 0x2f72e8, 0x2f7118, 0x2f7230, 0x2f6e60 decompiled with ghidra.
- **Push-apart** (follow-up 4) [inferred]: members move out of each other, Sora and the enemies by their body
  radii; the game's object-to-object push was not read.
- **Weapons** (follow-up 1): FUN_0016ef28 -> FUN_0016ee98 reads the member's saved weapon word (`Progress::member`),
  FUN_001845b0 the `went` object of the member's moveset for the item's `Flag1 | Rank << 8` slot, FUN_00184640 the
  `wmst` motion set of hand 0 (item 75 -> `W_EX020`, item 49 -> `W_EX030`, `party_member_weapons_from_went`). The
  bones: Donald 131, Goofy 101, the wrist bones with four finger children on the bind pose's -X side, the side of
  Sora's 178; on screen the staff and the shield sit in the hand facing outward (the other wrist puts Goofy's shield
  edge-on into his leg). Shown only while the battle flag is set [inferred from the reference video
  `10_menu_open_close.mp4` at 0.0 s, where neither member holds anything out of battle]. **Gap**: the app's new
  game leaves both members' weapon words 0 (the original's writer of those words was not found; searched: item-id
  immediates 0x4b / 0x31 in the decompilation), so no weapon appears without `KH2_PARTY_WEAPONS=75,49` (harness
  aid) or a state that sets the words.

Not done or approximated in stage 2:

- (Done in WP-P3b, section 13: the combo follow-ups, items, specials, Cure and the emergency pass.)
- (Closed in WP-F1 item 5.) A member's hit now uses the member's own record: `enemy::receive_hits` builds the
  attacker's `Stats` from `Progress::member(neo)` (strength, magic, defense, level, max HP: the status block
  +0x188 / +0x18a that `FUN_001a0610` reads is rebuilt from the saved record, `FUN_0019fd30` -> `FUN_0019f920`),
  without the hand-0 weapon's attack and magic (`enemy::member_stats`; [inferred] gap: the rebuild's equipment
  pass is not applied for the members, unlike Sora's keyblade in `GameData::sora_stats`); the combo count is
  Sora's only for Sora's hits. The hit sparks come from
  the member's weapon effect file (`fx::hit_sparks`: the first hand object's rig; `party::spawn_party` registers
  `obj/W_EX020.apdx` / `obj/W_EX030.apdx` with `drive::register_effects`), the keyblade's when the weapon has
  none. Log: `<enemy> hit: atkp N damage D ... (attacker Donald ...)` lines carry the damage.
- (WP-F1 item 6.) The app baseline equips item 75 on Donald and item 49 on Goofy (`Progress::apply_baseline`,
  constants `DONALD_STAFF` / `GOOFY_SHIELD`) when their weapon words are 0, so the weapons show without
  `KH2_PARTY_WEAPONS`; the original's writer of those words is still not found (above).
- The settings bytes (record +0xd4) are read as 0 (their new-game value).
- (WP-P3b) The walk/run/turn/jump speeds now come from the `prty` row of the NeoMoveset (FUN_00194438; Donald
  row 17 and Goofy row 20: walk 2, run 8), no longer the row of the NeoStatus (run 12) that stage 1 inferred.

## 13. Status: stages 3 and 4 in the app (WP-P3b)

Files: `party.rs` (status block, hits, KO, revive, items, the battle branch's stage-4 steps, the panels),
`party_battle.rs` (pure rules: `hp_change`, `revive_time`, `revive_step`, `revive_heal`, `pref_row`, and
WP-P3's `mp_gate`, `danger_member`, `emergency`, `item_pick`, `cure_target`, `followup_dir`), `friend_ai.rs`
(the special's mode switch, the MP payment), `friend_host.rs` (hosts 1.140 and 1.276), `hud_party.rs` (doc).
Log `KH2_PARTYLOG=1`; harness `KH2_PARTY_HP=<n>` (members start with n HP), `KH2_PARTY_HIT=<s>:<atkp>,...`
(the nearest enemy lands that `atkp` row on every member at those seconds).

Done:

- **Status block**: HP/MP from the saved record (`Progress::member`: max HP and MP; [inferred] a new block
  starts full), the MP charge (+0x1bc/+0x1c0) counting down per frame (FUN_00175f48 -> FUN_001a0540, regen 1.0)
  and refilling MP at its end (FUN_001a05a8).
- **Hits** (`party_receive_hits`, FUN_001106b8 -> FUN_0017cde0 -> FUN_00175340 -> FUN_00185a68): the `atkp`
  row's damage on `enemy::member_stats` by the enemy's stats, at least 1 unless ineffective, through the HP
  method port `hp_change` (FUN_00176c98 -> FUN_00186238 / FUN_001a0100); voice 9 when the hit takes the HP
  into danger (FUN_001a0be0 before and after); the reaction from `reaction_rules::victim_outcome` played as a
  motion of the member's set through the player-type remap (pushes and the blow's launch not carried out).
  A KO'd member is not hit [inferred]. Log: `party: P_EX020 hit: atkp 17 damage 3 HP 2 -> 0 of 18 ...`.
- **KO** (FUN_00186530 -> FUN_0017ce60): voice 2 (FUN_00175f10), the revive timer = StunTime of the +0x12c
  row x Hyper Healing (1.0): Donald 2400, Goofy 2100 frames. While KO'd and on the ground: motion 0x36 and
  system effect 0xe bound to the body (FUN_0017ca70); the effect is told to end when the member gets up.
- **Revive** (FUN_0017ca70 -> FUN_0017cc20 -> FUN_0017cee8): the timer counts down by a frame in battle and by
  `sstm +0x5c` (8) frames out of battle; at 0 the heal `int(maxHP x sstm +0x58 (0.5))` goes through the HP
  method, which on a KO'd member clears the KO bit and scales the heal by `sstm +0x54` (0.5); any heal (an
  item) revives the same way; motion 0x37 (blend `DAT_0037a55c` = 8) when the member lay in 0x36.
- **HUD** (`party_hud`): `hud_party::PartyMembers` per slot from the members' HP and MP.
- **Battle branch** of FUN_00111070: the MP gate each frame (FUN_00110f90 with 0.75 / 0.25 from 0x378598 /
  0x37859c), the member in danger (FUN_0010f830), the emergency pass (FUN_0010f8d8, which sets the gate) before
  the choice, the status block in the feasibility (MP cost, MP charge).
- **Combo follow-ups** (FUN_0010c800): direction = yaw + angle (`DAT_003784a0` degrees to radians), motion 3
  (blend 0), a jump of the entry's height moving the entry's speed per frame until the member lands
  ([inferred] the app's motor gravity instead of FUN_001b0758's `2h / t^2`).
- **Items** (FUN_00113888 / FUN_00113b08 phase 1 `item_pick`, phase 0 FUN_0010c9d0 -> FUN_00175e20): motion 9
  (blend 8); at frame trigger 6 (counted from the motion's start, section 10) `Progress::take_item(item,
  NeoStatus)` (FUN_00175ec0); after 30 frames (FUN_001b82b8) `items::item_effect` on the target (Sora, the
  member, the other member; everyone for the group items, FUN_001a1fe0); a member's HP part goes through its
  HP method (so a Potion revives). The use effects are not started.
- **Specials and Cure** (FUN_00113d48 / FUN_00113e68 / FUN_00114828): phase 1 needs the record's mode, the
  script not busy with a special ([inferred] stand-in for +0x390) and FUN_0010f220 (ability equipped, MP gate);
  Cure also `cure_target` below the entry's +0x3c. Phase 0 sets +0xaf4 and switches the script's mode
  (`friend_ai`). Host **1.276** (0x170a00 -> FUN_001769e0 -> FUN_00186368) pays +0xaf4: MP drops, or the MP
  charge starts (MP 0, length = the row's MpDrive 1200 x `DAT_0037a974` 1.0) when MP is short (Goofy's `blade`
  pays 10 per use in a b_44 run). Host 1.140 (0x173740 -> FUN_0018ed58) answers the member's target record.

Not done: (Cure's heal and the specials' payment: section 14; Donald's thunder, fire, cure and Goofy's tornado
attack objects and effects: section 15; Donald's blizzard, Goofy's blade and turbo: section 16) the members'
ability block, the attacker memory +0xcec, the panel flashes, limits and forms (stages 5, 6).
- FUN_00111040 forces motion state 0x1f only for classes 12, 15 and 16, so Donald and Goofy (classes 0, 1)
  may follow through FUN_0010e5e8's body (state 0) rather than FUN_0010e140 as stage 1 assumed; which state a
  free class-0 member stands in was not determined. The field follow keeps stage 1's port.
- The random generator is not the game's.

## 14. Status: Cure and the specials' payment (WP-P3c)

- **Donald's specials pay.** While the script runs a special's mode (+0x390 busy, FUN_0010f698) FUN_0010fcd0 and
  FUN_0010f8d8 return at once, so the member no longer chooses or moves (Choice::Busy; [inferred] the steer's
  speed is held at 0 so the cast motion is not replaced). riend_ai fills the motion controller's trigger
  bytes +0x238 / +0x239 (hosts 1.150 / 1.151, as enemy_ai does for enemies, frames counted from the motion's
  start, section 10): the cast's wait R089a (P_EX020) waits for bit 0 of +0x239, set by frame trigger 9 of
  motion 192 (slot 768, tick 26). Log: party: P_EX020 paid 10 MP (host 1.276): MP 100 -> 90 (thunder),
  paid 255 MP (host 1.276): MP 100 -> 0, MP charge 1200 (cure).
- **The scripts' attack objects** (riend_host): 2.10 (0x18e2f0) keeps the tkp id by a handle; 2.17
  (0x18e6e0: FUN_0018c5f8 / FUN_0018c6d0, then the target's receive method +0x64) delivers it on the member's
  target record; 2.13 / 2.22 need nothing. party_script_hits carries the hits out: on an enemy, a
  combat::Hit; on a party object, attack types 5 and 6 are a heal (FUN_00185a68 adds the amount; FUN_00175340
  lets these types through even on a KO'd member), through the HP method, which revives a KO'd member
  (FUN_00186238 -> FUN_0017cee8, scaled by sstm +0x54). Cure's attack 1525 is type 6, Power 200, so the
  amount is (magic x 200 + 99) / 100 of Donald (GameData::damage).
- The record FUN_00113d48 copies into +0x9f4 is phase 1's: Cure's is cure_target (the member in danger, else
  the first below +0x3c), the others' the member's enemy target.
- Log: party: P_EX020 heals P_EX030 with atkp 1525 (type 6, FUN_00185a68): +2, HP 0 -> 1 (was KO'd: revived),
  party: P_EX020 heals Sora with atkp 1525 (type 6, FUN_00185a68): +2, HP 6 -> 8.
- [inferred] The caster's magic is the saved record's plus the hand-0 weapon's item_stat magic (the status
  rebuild's equipment pass, FUN_0019f920, as GameData::sora_stats adds the keyblade); with the new game's record
  Donald's magic is 1, so Cure heals 2.
- Harness: KH2_PARTY_HIT=<s>:<atkp>[:<model>] (the model limits the hit to one member).
- (Done in WP-S3, section 15: thunder's / fire's attack objects and the specials' effects; the caster's
  magic is the rebuilt status, so Cure heals 12, not 2.)

## 15. Status: the specials' attack objects and effects (WP-S3)

Files: `friend_host.rs` (the hosts), `friend_ai.rs` (the attack-object pass, the effects), `party.rs` (the
status numbers, the script motion, the target point). Log `KH2_PARTYLOG=1`: `attack object 0x7a000001: atkp
1165 group 0x101 at .. radius 160 height 160 for 20 frames`, `attack object .. hits <enemy>: atkp .. SubId ..
(hit n, Power .., Element .., Interval ..)`, `released after its 20 frames`, `destroyed (FUN_0018c2d8)`, `effect
element 8 of obj/P_EX020.apdx (At { .. }): started`; `KH2_HITLOG=1` prints the damage (`Dusk hit: atkp 1165
damage 2 ..`). Harness `KH2_PARTY_ABILITIES=<item>,..` counts ability items as equipped on the members (the
new game holds Donald Thunder 167 and Cure 168 only; Fire 165, Blizzard 166, Goofy Tornado 423, Turbo 425,
Bash 429 from the table 0x352ec0).

**Attack objects** [V unless marked]. Host 2.10 (0x18e2f0 -> FUN_001a2ba0 -> FUN_0018bd08): an object of
0x140 bytes in the global list `DAT_01c89b58` (+0xc0 the next): +0x14 the `atkp` id (argument 1), +0x18 the
sub key (argument 3; FUN_0019e788 picks the row), +0x20 the hit group (the row's ComboGroup +0x2c when not 0,
else argument 2, else FUN_001bdf00's next), +0x1c a serial (FUN_001bde48). 2.31 (0x18e488 -> FUN_001a2c58):
a new object with the given `atkp` id and the first one's owner, sub key, group, serial and anchor +0x24 (its
size and growth start cleared). 2.11 (FUN_001a30e0): radius +0x100, half height +0x104 (the radius when not
positive). 2.12 (FUN_001a2fc8): +0xf0 = the old position once the object has one (+4 bit 1), +0xe0 = the new
one, +4 bit 4 when it moved more than two frame units. 2.62 (0x18e680 -> FUN_001a3480): +0xc8 = 0, +0xcc =
frames, +0xd0 / +0xd4 the start size (the final one when not positive). 2.13 (FUN_0018dc98): +4 |= 0x800.
Each frame the task FUN_0018c2d8 (registered by FUN_0018c3f0, priority 21000) destroys every object with
0x800 (vtable +4) and runs every other one's +0x10 = FUN_001a2cc0 (vtable 0x35e788): with +0xcc > 0 the clock
+0xc8 gains a frame (`DAT_00356f40`, not while the pause bit 0x20 of 0x35d268 is set) and past +0xcc the
object is released (no test that frame); the tested size is `start x (1 - s) + final x s`, `s =
sin(t / frames x pi x 0.5)` (FUN_00300278 = sinf, `DAT_0037b45c` pi, `DAT_0037b460` 0.5); the volume
(FUN_00189610, a sphere when radius = height) is tested at +0xe0 and, with bit 4, in steps of `min(radius,
height)` toward +0xf0 while more than half a step is left (`friend_ai::sweep_points`, test
`member_attack_sweeps_toward_its_previous_position`; growth: `member_attack_grows_from_its_start_size`). An
object with +0xc4 (2.61, an effect it follows) takes the effect's position first; no member script on the
disc uses it in the specials done here. [inferred] The victims are the enemies' hurt shapes (the member's
team, +0x3b0, is the player's); FUN_0018d768's own test and its early stop were not read: as `magic.rs`, every
victim is tested at every sweep point and the first overlapping point is taken. The victim's hit memory and
the row variant are `magic_rules::HitMemory` / `variant` (FUN_001be108 / FUN_001be190 / FUN_001be300,
FUN_0019e7e0), kept per (member, victim) [inferred]. A hit is a `combat::Hit` with the variant's SubId
(`Hit::row`), so `enemy::receive_hits` does the damage and reaction, and an `fx::SpellHit` with the member's
own effect file (2.22 anchors the attack to the object's +0x80, the file FUN_0018d090 takes the spark from).

**Effects.** 1.87 (0x171800 -> FUN_0016a570(obj, id, a, b, 0)) and 1.149 (0x171840 -> FUN_0016a570 with the
third argument's object as the owner) start the first element `id` of the object's own file
(`FUN_001db868(obj + 0x80, id, a, 1, b, owner)`) bound to the owner; 1.22 (0x1717c0 -> FUN_0016a5b8:
FUN_0016a530, no owner) starts it and overwrites its position (FUN_001d9d00) with the vector. The member's
file is `obj/<model>.apdx` (`spawn_party` registers it). The handles answer 0.49 / 0.50 (the instance runs),
0.53 (FUN_001d9d00: for a bound element its bone frame times its translation, `fx::object_bone`; one tick
old), 0.38 (placed anew), 0.44 / 0.85 (stop), 0.51 (destroyed); [inferred] 0.52 (fade) is taken as a stop.

**The target record.** 1.120 answers the centre of the hurt shape the member targets (`party_tick` keeps it in
`target_point`) [inferred: the record's point]; 1.141 is the floor under the point (`World::ground`, not
FUN_001a5c30's mask-4 cast). The record's word 0 (the object) names what FUN_00113d48 copied: the member itself,
the player, the other member or the enemy (`friend_host::read_mem`), so Cure's 1.149 binds element 12 to the one
healed and 1.130 (0x170510, the objects' difference) tells healing itself from another.

**Donald's specials** (`kh2 bdx P_EX020`; modes registered at R0206..R0296: main / exit / enter routines).
- `thunder` (R13c1 -> R13c8): voice 6 (1.262), motion 192 then 0 queued, the cast trigger (R089a: bit 0 of
  +0x239), element 7 on Donald, 10 frames, MP (1.276), the target's point (else 300 ahead), a ground probe from 80
  above: ground less than 160 below -> element 8 at the point, else element 9; attack 1165 at the point, radius
  160 for 20 frames (2.62 (20, 160)), and its copy (2.31) radius 25, half height 320, 320 above, for 20 frames;
  the mode waits 20 frames. Log (b_44, tt04): `attack object 0x7a000001 hits 2605v0: atkp 1165 SubId 0 (hit 0, Power
  100, Element 3, Interval 8)` and `Dusk hit: atkp 1165 damage 2 hp 30 ..`; every Dusk in the radius is hit, again
  after the row's Interval (8 frames): three hits in 20 frames.
- `fire` (R07d3 -> R07e1): voice 6, motion 192, the trigger, MP, element 0, 4 frames, element 1 kept at the
  context +0x18, attack 1163 at +0x1c anchored, radius 220 half height 70; for 60 frames while element 1 runs the
  attack follows its position (0.53 -> 2.12); then released (R0967), and the mode waits for the element's end.
  Log: `hits 2731v0: atkp 1163 SubId 0 (hit 0, Power 50, Element 1, Interval 12)`, then `SubId 2 (hit 2, Power
  125 ..)` (FUN_0019e7e0's variant) with a blow.
- `cure` (R1552 -> R1559): as in section 14; its elements 11 (on Donald) and 12 (on the one healed) start.
- `blizzard` (R0a0b -> R0a19): voice 6, motion 191, the trigger, MP, element 3, then a request thread (2.20) at
  word 0xaf8 (a homing projectile library, listed with `cargo run -p kh2-bdx --example listing -- P_EX020 af8`)
  that needs hosts 1.21, 1.88, 1.160, 2.19, 2.29, 2.38, 2.39, 2.43, 2.44, 2.57, 6.9, 6.51, 6.54, 0.41, 0.45, 0.46.
  (Done in WP-S5, section 16.)

**The caster's numbers** [V]. FUN_001a0610 reads the attacker's status +0x188 / +0x18a, which FUN_0019f920 rebuilds
from the saved record: the `lvup` row (byte +4 strength, +5 magic, +6 defense) of the record's level (+0xf) in the
table FUN_001b50c0 picks (FUN_001b58d8(NeoStatus): 1..15, 14 -> 1, 15 -> 6, else 0 -> 1; the first table when the
index is past the count or its offset is 0; the level held at the table's count), plus the record's bytes +9 / +0xa /
+0xb, plus FUN_001a0c08 for the two weapon words, the armor and the accessory slots (the `item_stat` Attack / Magic /
Defense). `PartyData::member_status`; test `party_member_status_from_lvup_record_and_equipment` (Sora's come out as
the status page's 5 / 7 / 2). Donald at the app's start: magic 6, so Cure (atkp 1525, type 6, Power 200) heals
`(6 x 200 + 99) / 100` = 12 (log `party: P_EX020 heals Sora with atkp 1525 (type 6, FUN_00185a68): +12, HP 4 -> 16`).
[inferred] A level of 0 is read as 1. A member's defense when an enemy hits it (`party_receive_hits`) is the same
rebuilt +0x18c (Donald 2 at the app's start instead of the record's 0). **Gap**: the members' motion-trigger and attack-object hits on enemies are
damaged by `enemy::receive_hits` with `enemy::member_stats` (the record's bytes only, no `lvup` row and no
equipment): thunder does 2 on a Dusk where the status would give more (outside-ownership change, report).

**The script's motion** (fixes a member stuck in a special): a motion request of the script (1.11, FUN_0017e220)
starts the motion again even when it plays, and loops only when its data has a loop range (as `enemy_ai`); 1.13
(0x1703d8 -> FUN_0017e170, the controller's queue +0x3c) plays the queued motion once the requested one has ended
[inferred: one queue entry, the member's blend]. Before, the cast motion 192 kept looping after a special, and the
next special's request of the same slot did not restart it: the cast trigger never came and the mode waited forever.

**Member handles.** The members' script handles were `SELF_BASE + (0x1000 + slot) x OBJ_SIZE`: slot 0 (Donald) is
0x0200_0000 = `PLAYER`, so every host arm that tests for the player took Donald's own calls as the player's (his
script's contexts named the player; 1.130 and 1.149 showed it). They start at 0x1001 now.

**Goofy's specials** (`kh2 bdx P_EX030`: `tornado` main R07af, `turbo` R0b4e, `blade` R0d30).
- `tornado` (R07af -> R07bd) [V for the script, the hosts as marked]: motion 191 (1.12), invincible for the motion's
  length (1.67 then 1.91 -> FUN_00175aa8: +0xb00 = it; FUN_00175ab0 counts it down in the party tick, FUN_00175ad8
  refuses hits: `party_receive_hits` logs `not hit by atkp ..: invincible`), motion 4 queued; once range trigger 16
  is on (1.150 bit 1) and the motion is still 191 (1.15 answers the motion number the script asked for, not the
  remapped slot): attack 1188 radius 200; each frame while the motion runs (1.14): a goal (the target's point at
  Goofy's height, or the player when the target is nearer than 150, or 100 ahead without a target), `h = sin(a) x
  35 + 65` above it (a ground probe from 80 above when far), the direction (start (0, -1, 0, 1)) turned toward the
  goal by 10 degrees (0.46 = FUN_0016d138 -> FUN_0017c0f8, `friend_host::turn_vec`, test
  `member_turn_vec_follows_fun_0017c0f8`), `a` += 5 degrees, the attack 100 above Goofy (2.12); while trigger 16 is
  on 1.111 moves him along the direction at 3 / 10 / 2 a frame, else the attack is released and he stops; at the
  end motion 4 and 1.74. 6.3 / 6.5 (the target search FUN_002f01a0 / FUN_002f04a8, used only without a valid target)
  are not ported. Atkp 1188 has Power 0 and reaction class 5 (a pull toward the attack object, which
  `enemy::receive_hits` decodes only for spells: `NotDecoded { class: 5 }`); the damage comes from the spin's own
  motion volumes (atkp 1160, damage 1 with a blow on a Dusk). Log (tt04 b_44, `KH2_PARTY_ABILITIES=423,-429`):
  `P_EX030#1 invincible for 215 frames (1.91, +0xb00)`, `attack object 0x7a000001: atkp 1188 group 0x101 at ..
  radius 200 ..`, `script mode "tornado" -> "-"` 7.3 s later. [inferred] The movement: the enemy host's 1.111 keeps
  the ground part of direction x speed and the member's motor its gravity (the goal's height is not reached:
  Goofy spins on the ground); the motion's length (motion frames) is taken as frames of +0xb00 (one per 60 Hz tick),
  so the invincibility ends halfway through the app's motion; the units of the two were not compared.
- The runtime (`kh2-bdx`, `runtime.rs`) hands 0.45 / 0.46 the two vectors and the scalar and writes the host's answer
  back into the first (their pointer is the script's memory, which the host cannot reach); the enemy and spell hosts
  do not answer them (unchanged). `friend_host::advance_vec` is 0.45 (FUN_0016d098, test
  `member_advance_vec_follows_fun_0016d098`).
- `blade` (R0d30) (done in WP-S5, section 16; as found by WP-S3): a child thread (1.82) throws the shield object (1.234 the weapon list +0xaf8, 1.83,
  1.148 / 1.89 / 1.79 / 1.11 on the weapon, attack 1161 with a new group (2.67) anchored to it and a callback 2.38,
  2.40) and the mode waits for it (context +0x20). The app's hand objects are not objects a script can move, so the
  child thread fails (a script error reading through the null weapon object at 0x0e14 in the base run, then the mode
  waits forever: Goofy stops choosing actions after his first `blade`; this was so before WP-S3).
- `turbo` (R0b4e -> R0b5c) (done in WP-S5, section 16; as found by WP-S3): motion, 1.91 for its length (186), motion queued, then while the motion runs
  a dash steered with 0.46 toward the target or toward points of 1.247 `random_free_point` (FUN_001726d8 ->
  FUN_001a6068 / FUN_001a6198, a spiral search with the game's second generator) and 6.4 / 6.5 (target search),
  moved with 1.111 while range trigger 16 is on; no attack object (the motion's volumes hit). In the app 1.247 is not
  answered: the script reads through its 0 answer (`script error BadAddress(0) at 0x0beb`) and the mode ends after
  ~1.7 s (log `turbo.log` with `KH2_PARTY_ABILITIES=425,-429`).
- 10.55 on the script's own memory (Goofy's search blocks) is classed a stub: the host cannot reach the script's
  globals, and only 6.5 reads the bit. (WP-S6: the runtime does it, section 17.)

**The script's motion, again**: a script motion that ended stays on its last frame (`Animator::play` would start a
finished motion again, so the script's 1.14 never saw it end and Goofy's `tornado` never left its loop); the queued
motion (1.13) is put on by `friend_ai` after the slice that saw the end.

Not verified: the spark element each hit starts (only the effect file is chosen as the game does); voices 6 / 21 / 3
the scripts ask for (1.262 on a member goes to the enemy voice path, which finds no enemy of that name).

## 16. Status: Goofy's turbo and blade, Donald's blizzard (WP-S5)

Files: `party_rules.rs` (`free_point`), `friend_host.rs` (the hosts), `friend_ai.rs` (the hand object, the
callbacks, the wall test, `MemberAttacks`), `party.rs` (`member_hands` draws a thrown hand object,
`retarget`), `enemy.rs` (the class-5 pull), `kh2-bdx` (`Runtime::call_routine`, the empty copy, name 6.9).

**Turbo** (`P_EX030` R0b5c) [V for the script and 1.247]. Host 1.247 (0x1726d8 -> FUN_001a5ea8 with mask 1):
FUN_001a6068 builds the search block: the centre, the radius `lo + r x (hi - lo)` and a start angle of
`((t & 4) >> 2 | t << 1) x 0x37b5a8 + r x 0x37b5a8` (`r` the second generator 0x3716a0 times 0x37b5ac, `t`
the counter 0x35e9b0, then `t + 1` modulo 8), as host 1.248's. FUN_001a6198: one step of 0x37b5b4 (pi / 4)
further each try, the candidate `centre + (sin a, 0, cos a) x radius` is cast for a floor from 0x37b5c0 (200)
above to 0x37b5bc (1000) below (FUN_001a5c30 -> FUN_001d1648); with a floor the line from the centre to the
point 0x37b5b8 (20) further out and 0x37b5b0 (100) above the floor must be clear (FUN_001a5bf8), and the floor
point is the answer; after eight tries the radius shrinks by 100; below `lo` the floor under the centre
(else the centre). Test `party_free_point_matches_fun_001a5ea8` (kh2-ee, the two collision queries stubbed:
the point, 0x3716a0 and 0x35e9b0 agree in 7 cases) and `party_free_point_by_hand`. [inferred] The floor cast
is `World::ground` and the line test `World::raycast` (the masks of FUN_001d1648 were not read); each member
keeps its own generator and counter (the game shares them). Log (tt04 b_44, `KH2_PARTY_ABILITIES=425,-429,-423`):
`script mode "-" -> "turbo"` at t 8.5, `"turbo" -> "-"` at t 14.5 (the whole motion; before: the script error
at 0x0beb after ~1.7 s); the host table shows `1.247 random_free_point Approx x10` and no missing call but
6.4 before it was classed (below).

**The target search 6.3 / 6.4 / 6.5** [V for what was read, **not ported**]. 6.3 (FUN_002f01a0): block word 0 =
the argument (5, the history length [inferred]), +0x14 = 0x383fb4 (1000), +0x18 = 0x383fb8 (pi), the rest 0; the
script then writes +0x10 / +0x14 (0 / 500) and +0x18 (2 pi) itself. 6.4 (FUN_002f0748): byte +4 and word +8 = 0.
6.5 (FUN_002f04a8): clears the record (FUN_0018ed40), walks the object list (FUN_001b7c88), keeps objects that pass
FUN_002f0218 (the kind +0xc: 0 = +0x588 bit 0x10, 1 / 2 = bit 4) and FUN_002f03f8 (FUN_0019a5d0 unless kind 2 or
bit 4), skips +0x108 bit 0x8000 when block +4 bit 0x10000 is set (10.55), takes each target part
(FUN_0016b4e0(obj, 3, ..), FUN_001894d0 its point), with history on skips parts in the history (FUN_002f01d8),
measures the (flat when asked) distance to the point given, keeps those within +0x10..+0x14 and, when +0x18 is
below 0x383fbc (pi), within that angle of the direction given; the nearest (or farthest) goes into the record
(FUN_0018f490) and, with history on, into the history (FUN_002f0438 -> FUN_002f02f8, not read). The block lives
in the script's own memory, which the host cannot reach, and FUN_002f02f8 / FUN_0019a5d0 were not read: all three
are a stub, the member keeps its own target (Goofy's `tornado` and `turbo` dash at it). (Ported in WP-S6: section 17.)

**Blade** (`P_EX030` R0d30 / R0d94, exit R13f5) [V for the script; the hosts as marked]. The member's hand-0 object
is a scripted object (`friend_host::WeaponObj`, handle `WEAPON_HANDLE + slot`): 1.234 (0x174280, +0xaf8) names it;
the child thread R0d94 waits for the throw trigger (R0f55: 1.151 bit 0), takes it off the hand (1.161 has_link ->
1.102 link_end), places it at its bone 3 (1.83 -> FUN_0016aa18 on the last drawn pose, 1.148), makes attack 1161
with a new group (2.67) anchored to it (2.22), radius 60, and registers R10f7 as its hit routine (2.38: +0x8c the
thread, +0x90 the routine, +0x94 the argument; disassembly of 0x18e4f0), plays motion 201 on it and flies it for at
most 180 frames: each frame the homing step R110b, 0.45, 1.89 (the secondary velocity +0x880; [inferred] the free
object moves by it once a frame, `friend_ai`), 2.12 to its position, 1.79; it stops on a wall (2.40) or a hit. A
hit runs the routine (FUN_0018cf50: after the spark and the owner's +0x74, FUN_0018dbd8 -> FUN_0016c550 with
(argument, victim, attack); `Runtime::call_routine`): R10f7 sets the context's +0x40 and points the target record at
the victim (1.160 -> FUN_0018f490; `PartyMember::retarget`). The object then holds still on the target (1.89 speed
0) while hits keep coming, ending 16 frames after the last one; the main routine links it back (R13ac: 1.235 the
joint, 1.18 link_start), plays motion 193 and queues 0; the mode's exit (R13f5 -> R0a96 -> R0ad9) releases the
attack (2.13). `party::member_hands` draws a free hand object at its own root and facing (one tick behind) with
the motion asked for, [inferred] the motion number as the hand object's slot (not a player-type object) and a
yaw-only facing. Test `goofy_blade_throws_and_takes_back_the_hand_object` (kh2-bdx disc test, the mock world
with a hand object; no script error). Log (tt04 b_44, `KH2_PARTY_ABILITIES=-423,-425 KH2_PARTY_WEAPONS=75,49`):
`hand object off the hand (1.102)`, `attack object 0x7a000001: atkp 1161 ..`, `.. hits 2985v0: atkp 1161`,
`its routine R10f7 runs (2.38, FUN_0018cf50)`, `target record now names 2985v0 (1.160 ..)`, `hand object back on
the hand (1.18)`, `script mode "blade" -> "-"`, then `P_EX030 chose record 5 Attack ..`: Goofy goes on choosing
(before: BadAddress(0) at 0x0e14 and the mode waited forever). (The members held items 75 / 49 in these runs without
`KH2_PARTY_WEAPONS` too.) A member without a hand object gets 0 from 1.234, and the script fails as before.

**Blizzard** (`P_EX020` R0a19 -> R0acf, request thread R0af8) [V for the script]. The cast (motion 191, the
trigger, MP, element 3 on Donald) starts a request (2.20) of 288 bytes at R0af8 with the facing: element 4 of
Donald's file unbound (1.21 -> FUN_0016a530) and attack 1164 (2.10, radius 80 by 2.11); each frame the homing
step R0f2b (1.140 / 1.120 / 6.9 on the target record copied by 1.131, 0.21 / 0.13 / 0.46 turns), 0.45 along the
direction, 0.38 the shard's place, 2.12 the attack there, 0.41 its direction (FUN_0017be00's angles: the element's
rotation, as the hit spark's); it ends when the attack meets a wall (2.29) or clashes (2.39) or after its 30-frame
life (then 0.52 fade 8 and 2.13). Hits on enemies are `combat::Hit`s with the rebuilt status (WP-S4). Log (tt04
b_44, `KH2_PARTY_ABILITIES=166,-167,-168,-165`): `attack object 0x7a000001 hits 2733v0: atkp 1164 ..` and `Dusk
hit: atkp 1164 damage 10 hp 22 reaction 4 ..`; no missing call. It needed one interpreter fix: R1312 copies 0 bytes
from address 0 (`copyto [local0]+0x118, 0`), which the game's interpreter does with memcpy (FUN_001d7cb0 case 2 ->
FUN_00310af4) and so reads nothing; the runtime raised BadAddress(0) at 0x137f (test `copy_of_no_bytes_reads_nothing`).
Not reached by blizzard (the request's +0x100, +0x108, +0xfc, +0x118 stay 0 from R1312 / R1385, and the bounce
branch tests `-1 >= 0`): 1.88, 1.160 (R0d3a), 2.19, 2.43, 2.44, 2.57, 6.51, 6.54; they stay unanswered by the
members' host (the enemy host's `missing` if a script reached them). Not seen in a run: a shard meeting a wall. (WP-S6, section 17: none of them is reachable.)

**Wall test of the members' attack objects** (FUN_001a2cc0 -> FUN_0018c8e0): after a move of more than two
units (+4 & 4) a row with BG Hit (+0x12 bit 0) whose object has hit nothing and met no wall casts the segment from
the previous position to the new one, then down by FloorCheck (`combat::wall_cast`, mask 4); a hit sets 8 or 0x10
(FUN_0018ca78). [inferred] +4 bit 0x100 (testing after a hit) is not modelled; the wall spark is not shown.

**Tornado's pull** (atkp 1188, reaction class 5). FUN_00185668 -> FUN_001bf920 keeps the attack object in the victim
(+0xaac) for any attack object, so a member's object pulls like a spell's: `friend_ai::MemberAttacks` holds the
members' live objects (keys with bit 31, the slot and the handle) and this tick's hits; `enemy::receive_hits` and
`think` read it after `magic::SpellAttacks`. Log (tt04 b_44, `KH2_PARTY_ABILITIES=423,-425,-429 KH2_REACTIONLOG=1`):
`Dusk hit: atkp 1188 damage 0 .. reaction 5 effective 5`, `Dusk pulled (reaction class 5, FUN_001bf920): attack
2164260865 ..`, `Dusk pull: 201 units from attack ..`; the spin's own motion hit (atkp 1160) then replaces the pull
(FUN_0016b698), as for the spells.

## 17. Status: the target search, blizzard's later hosts, the hand object's velocity (WP-S6)

Files: `kh2-bdx` (`search.rs`, the runtime's arms for 6.3 / 6.4 / 6.5 / 10.55 / 6.54, `Host::target_parts` /
`Host::target_found`, the executable's numbers in `HostTable::search`, the name 2.19), `friend_host.rs`,
`friend_ai.rs`, `party.rs` (`search_target`).

**The target search** [V: test `search_matches_fun_002f04a8`]. Goofy's scripts keep the block in their mode
context: `tornado` at +0x24 (R0a15: 6.3 with a history of 1; 10.55 with 1; R0a1f: +0x10 = 0, +0x14 = 10000;
R0a2e: +0x18 = 2 pi), searched (R091a) only when 1.140 says his target record is not valid; `turbo` at +0x20
(R0cfb: a history of 5; 10.55; +0x10 / +0x14 = 0 / 500; +0x18 = 2 pi), searched every 30 frames of the dash
(R0c1b), and after five searches 6.4 empties the history (R0c12). Both pass the record of 2.23 (the member's
+0x9f4), the member's position and facing, nearest 1, flat 1, history 1; neither writes the kind +0xc, so it
stays 0. The block (FUN_002f01a0): +0 the history's capacity, byte +4 its next slot, byte +5 the parts the last
search walked, +4 bit 0x10000 (10.55, FUN_002f0758: skip objects with +0x108 bit 0x8000), +8 the history count,
+0xc the kind, +0x10 / +0x14 the distances, +0x18 the angle, +0x1c + 8 i the history (object, part id);
6.3 leaves the history entries as they are, and +0x14 / +0x18 start at 0x383fb4 / 0x383fb8 (1000, pi).
FUN_002f04a8: the record cleared (FUN_0018ed40: 0, 0, 0, -1), byte +5 = 0; the object list from 0x1d5d7a0
(+0x8b4 the next; FUN_001b7c88 skips objects with +0x108 bit 0x80000 or 0x10000000, FUN_0019a440); an object
passes FUN_002f0218 (one linked under a parent with +0x588 bit 0x80 never; kind 0: +0x588 bit 0x10, set by the
enemy class FUN_00184820; kinds 1 / 2: bit 4, the player-type classes FUN_001746e0 / FUN_00174800; other kinds:
any), not +0x108 bit 0x8000 with the skip bit, and FUN_002f03f8 (kind 2 or bit 4, else FUN_0019a5d0: class kind
byte (+8)+4 not 0 / 1 / 5, not 8 / 0xd in battle with 0x35d268 bit 0x10000, +0x7f8 bit 0x400, +0x108 bit 0x20
clear, a target part); then each target part (FUN_0016b4e0: the collision list +0x8d8, records of type 3, the
part enabled in the list's word +4 by its id byte 0 unless the bit arrays +0x474 / +0x47c say otherwise,
FUN_0019a6a0): byte +5 counts it; with history on a part in the first +8 entries is skipped (FUN_002f01d8); its
point is FUN_001894d0's (the bone's matrix times the record's offset, moved up by the lift +0xe); the distance
(the height difference dropped when flat) must lie in +0x10..+0x14; below 0x383fbc (pi) the ground direction of
the part must differ from the direction given by less than +0x18 (atan2 of x and z, FUN_003003e8, the difference
wrapped by FUN_0017bbc0 with 0x37a528 / 0x37a52c); the nearest (or the farthest) wins, the first of equals
(FUN_0018f490: object, part id, 0, -1). With history on, the winner goes into the next slot (FUN_002f0438 ->
FUN_002f02f8: unless already there; the count grows up to the capacity, the slot advances modulo it,
FUN_002f02b0). The script's block is out of the host's reach, so the runtime (`kh2_bdx::search`) does 6.3,
6.4, 6.5 and 10.55 itself and asks the host only for the objects' target parts (`Host::target_parts`), then
writes the record and tells the host (`Host::target_found`); the oracle test lays random objects (flags, class
kinds, collision lists, matrices) into kh2-ee memory, runs the game's FUN_002f04a8 with every callee its own,
and compares the record and the whole block over 400 scenes (144 with a target). Test
`goofy_turbo_searches_its_targets` (kh2-bdx disc test, the mock world): kind 0, the skip bit set, the nearest part
within 500 first, the other next (the history), then none, and after five searches the nearest again; no script
error.

In the app (`FriendHost::target_parts`): kind 0 lists the hittable enemies' TARGET shapes (the collision records of
type 3, with their id byte and the hurt shape's centre: `combat::hurt_spheroid` is the bone frame times the offset
plus the lift, as FUN_001894d0) in the fighters' order; their handles are `0x7e000000 + the entity's index`, so the
history keeps them from search to search. `target_found` on the member's target record points the member's target
at the part found (`party::search_target`: the special's record too) and answers 1.140 / 1.120 / 6.9 from it at
once; a cleared record makes the target invalid. [inferred] `hittable` stands for FUN_0019a5d0's tests; kinds 1 / 2
(the player and the members) are answered with no parts (no member script asks them); the enable mask of the
collision list (its word +4, which `kh2_model::collision` does not keep) and +0x108 bit 0x8000 (10.55's skip) are
not modelled; the list's order is the fighters' query order. The app's own re-target pass (`party_tick`) still runs
while a special does (as before). 10.14 (0x2f6da0 -> FUN_00112370) in Goofy's script passes mode 0 (R0a7b) or mode 1 with his own
record of 2.23 (R0b14): the record is copied onto itself (and cleared when FUN_0018ed58 finds it invalid), so no other
record reaches +0x9f4 that way. Log (tt04 b_44, `KH2_PARTY_ABILITIES=425,-429,-423 KH2_PARTYLOG=1 KH2_BDXLOG=1`):
`script mode "-" -> "turbo"`, then every half second `party: P_EX030#1 target search (6.5, FUN_002f04a8): 7 parts
walked, the record names 2985v0 shape 0`, then `.. names 2859v0 ..`, `.. names 2796v0 ..` (the history moves the
choice on), 23 searches in the run, no script error; the host table shows `6.5 ? Approx` and no missing call.

**Blizzard's later hosts** [V from the script]. In P_EX020 the request fields +0x100, +0x108, +0xfc, +0x10c, +0x114,
+0x118 are written only by R0b1b, R1312 and R1385, each with 0 (no indirect writer: no `push.i` of their offsets in
the script). So 1.88 (R0ca9, behind +0x100), 2.38 at R0c9d (+0x10c), 2.57 (R0c71 / R0d02, +0x118), 6.51 / 6.54
(R1145 / R114b, +0x108) and 2.19 (R1191, +0xfc > 0) are never reached; the bounce branch R0cce ands with the
constant test `-1 - 0 >= 0`, so R0d3a (1.132, 2.44, 2.23, 1.131, 1.140, 1.160, 2.43, 2.13) and the 2.44 at R0cdb
are dead code. What is ported anyway, because the code is plain: 6.54 (0x2ef110: the vector into the static
buffer 0x1e35680, the answer its address) in the runtime for every script; 2.19 (0x18e7a0 -> FUN_001a30a8: +0xe0
and +0xf0 set, +4 bit 0x20) named `attack_set_positions` and answered for the members' attack objects (bit 0x20 not
kept: its reader was not read); 2.44 (0x18e5d8: the word +0x84) answers the member, the attack object's owner
(FUN_001a3280 reads the owner's +0x9f4 through it) [inferred: FUN_0018bd08's write of +0x84 not read]. 1.88, 1.160
(other than the blade callback's), 2.43 (FUN_001a3280), 2.57 (FUN_001b3c48 -> FUN_001db868 on the object +0x18) and
6.51 (FUN_001d1648, a collision cast with mask 0x10004) are stubs naming the reason. Log (tt04 b_44,
`KH2_PARTY_ABILITIES=166,-167,-168,-165`): `script mode "-" -> "blizzard"`, `attack object 0x7a000001 hits 2733v0:
atkp 1164 ..`, `Dusk hit: atkp 1164 damage 10 hp 22 reaction 4 ..`, `"blizzard" -> "-"`, no script error, none of
the stubs in the host table.

**The hand object's velocity and facing** [V]. 1.89 (0x172f90 -> FUN_00183418): +0x880 = the direction made unit,
+0x88c = the speed. Its only reader is FUN_00181e00 (the object's movement, from FUN_001b74e0; the `lqc2` loads
mips_scan does not list, found in the decompilation): the frame's displacement gets `+0x880 x +0x88c x
DAT_00356f40` and +0x880 / +0x870 are cleared at the end, so the object moves only in frames the script set it.
`friend_ai` now clears the velocity after the move (before, it kept moving by the last one). 1.79 on the hand object
(0x172d98 -> FUN_0016a828): without the lock bit (+0x7f8 bit 8, FUN_0019ad98) only a yaw, atan2 of x and z
(FUN_003003e8), into +0x66c (FUN_0016a7f0): the flat facing `member_hands` draws is the game's. [inferred] The
turn from +0x550 to +0x66c (who applies it) and the mover's own displacement (+0xc vtable +0x1c, FUN_00182370) were
not read. Blade still runs (log: `hand object off the hand (1.102)`, `atkp 1161 ..`, `back on the hand (1.18)`,
`"blade" -> "-"`, no script error).

**Gaps**: FUN_0019a6a0's bit arrays +0x474 / +0x47c (who sets them) and the collision list's enable word; the
writer of +0x108 bit 0x8000 on enemies (10.55 skips them); FUN_001d1648's masks (6.51); FUN_0018bd08's +0x84.

## 18. Status: the slot bytes across area loads, member rebuild, the third slot (WP-MN8)

Files: `party.rs` (`PartySlots::enter_world`, `wanted_members`, `member_key`, `spawn_party`).

- **Carry-over.** Before, every area load re-applied preset 1 to an empty party, so a Party-page swap was lost at the
  next area. The original keeps one four-byte record per world (`0x340c9c + world x 4`, FUN_001a71c0) in the save data;
  only the area-script command 0xf (FUN_001a7330, `rules::apply_preset`) rewrites it. The app's `PartySlots` now keeps a
  record per world it has entered: `enter_world(world, fresh)` stores the bytes of the world being left, restores the
  record of the world entered, and calls `fresh` (preset 1) only for a world with no record. [inferred] The app has no
  new-game writer and does not run command 0xf for these bytes (gap, section 10), so preset 1 stays the start value.
  Test `slot_bytes_survive_area_loads`.
- **Rebuild.** `spawn_party` also runs when the bytes change inside an area. `member_key(bytes, merged)` is the sorted
  list of objects the slot bytes 1..3 name through the merged `memt` (FUN_0017cd20, `rules::slot_object`); when it differs
  from the key the standing members were built for, their entities are despawned and the named members spawned again,
  each at the place of a standing member of the same NeoStatus, else at Sora's [inferred]. A swap of two slots (the Party
  page) keeps the key, so nothing is rebuilt: the entities already trade `slot` (`menu_party::swap_members`). Test
  `slot_changes_change_the_member_set`. Nothing in the app changes the set yet (Party In / Out, page 0x15, is a gap),
  so the rebuild path is covered by the key test only; it was not run in the game.
- **Slot 3.** A filled byte 3 spawns a third member (`PARTNER`) on an area load, beside Sora [inferred: no map script row
  places a third member; FRIEND_1 / FRIEND_2 are the only stand-ins, FUN_0016e208]. It takes the next free party slot
  (FUN_0017cd08, `slot` 2). `follow_points` (FUN_00111670) is built for two followers, so the third member's follow point
  falls back to the player [inferred]. The preset 1 start has an empty byte 3, so nobody extra spawns in the default game.

**Gaps** (not read): page 0x15 and its caller; the writers of the hidden-member list FUN_001c7be8 (so the "locked by the
hidden list" rule of `menu_party::rows` stays byte-bit-0x80 only); the members' leave / enter animations; the model tweaks
of FUN_0023b5d0 / FUN_0023b5b0; a third follow point in FUN_00111670.

## 19. Status: no members in the Struggle (WP-S8)

Files: `party_rules.rs` (`mission_presets`), `party.rs` (`mission_party`).

- **The rule [V].** Nothing removes members at a mission's start: the members are never made. tt04's Struggle `btl`
  programs (0xb6, 0xb7, 0xb8; read from the disc) are `0xf [0]; 0x15 [mission]; 0x1d [1]; spawn b_5x; spawn b_60`,
  and their `map` programs spawn no group. Opcode 0xf (FUN_00180b48 -> FUN_001a7330) applies preset 0 `[0,-,-,-]`
  (section 1) to the world's slot bytes, which empties slots 1..3. The groups b_50 / b_51 / b_52 do hold FRIEND_1 /
  FRIEND_2 rows (0x237 / 0x238, spawn kind 0) beside the player's, but the factory resolves them through the slot
  bytes (FUN_0016e208 -> FUN_0017cd20: FUN_001a71c0(-1) the current world's record, FUN_001a71e8 the byte & 0x1f,
  0x10 -> no object), so nobody is created and Sora fights alone. The Seifer programs 0x4c, 0x4d, 0x4e and 0xb5
  also apply preset 0 (test output). Opcode 0x1d [1] (FUN_001abfe8 case 1) ends a drive form if one is active, else
  calls FUN_001ac130 on the status blocks 1..15 (FUN_001a0240, FUN_001a0100) [inferred: a heal back to full]; it
  removes nobody and is not ported here.
- **The return [V].** The slot bytes stay as preset 0 left them (the record survives area loads, section 18) until
  the next opcode 0xf or the Party page rewrites them; applying preset 1 again gives Donald and Goofy back
  (`apply_preset`).
- **The port.** The app does not run a `btl` program at area load: the wave spawner loads the group and sends
  `struggle::MissionStart`. `mission_party` reads that message, finds the area's `btl` program whose opcode 0x15
  names the mission (`rules::mission_presets`) and applies its 0xf presets to `PartySlots`; `spawn_party`'s
  rebuild (section 18) then despawns the members, since the bytes name nobody. Log: `party: mission 0x128
  TT04_MS00: its btl program applies preset 0 (opcode 0xf, FUN_001a7330): slot bytes [00, 01, 02, 10] -> [00, 10,
  10, 10]`, then `party: the slot bytes ... name other members than the 2 standing: rebuilding`. Tests
  `a_struggle_program_leaves_sora_alone`, `party_the_struggle_programs_leave_sora_alone`.
- **Gaps.** [inferred] The app's rebuild removes the standing members at once (the original never created them,
  since the program runs before the area's objects). The app does not apply opcode 0xf at area load (the
  area-script run, `ScriptRun::world_records`, is collected but no system applies it), so after a Struggle the
  members stay out in world 2 for the session unless the Party page changes the bytes; in the original the next
  program with an 0xf preset would restore them. Opcode 0x1d's status reset is not ported.

## Decompile index

| address | what |
|---|---|
| 0x16e8c0, 0x16e208, 0x17cd20, 0x17cd78 | object factory, stand-in ids, slot -> object |
| 0x1a6f48, 0x1a6ff0, 0x1a6f70, 0x1802e0 | memt pointers, merge, record test, story flag |
| 0x1a71c0, 0x1a71e8, 0x1a7128, 0x1a7118, 0x1a7200, 0x1a7250, 0x1a72c0, 0x1a7330, 0x1a7500, 0x1a7558, 0x1a7608, 0x1a7268, 0x1a7698 | slot bytes, presets, member index, in-party test |
| 0x180b48 | area-script commands (0xf preset, 0x10) |
| 0x1abfe8, 0x1ac130, 0x1ac170, 0x19fe48, 0x19ffb8 | opcode 0x1d party reset (WP-S8: removes nobody) |
| 0x17d1b0, 0x17d290, 0x10b780, 0x10be80, 0x17c718, 0x19fd30, 0x19fdc0 | spawn, re-create, friend class, friend base, status blocks |
| 0x17c968, 0x17c9b8, 0x17ca48, 0x17ca70, 0x17cc20, 0x17cc70, 0x17cc98, 0x17cde0, 0x17ce60, 0x17cee8, 0x17cfc0, 0x17d030, 0x17d0a8, 0x17d0f0, 0x17d150 | friend base methods (panel, tick, KO, revive, in-party) |
| 0x10c5f8, 0x10c630, 0x10c658, 0x10c6e0, 0x1106b8, 0x110790, 0x1112e8, 0x1115a0 | friend vtable overrides |
| 0x111070, 0x111040, 0x10df08, 0x10dcf0, 0x10cf90, 0x110f90, 0x10f830, 0x10f8d8, 0x10fcd0, 0x10fc68, 0x10ed00, 0x10ed20, 0x10f220, 0x10f578 | decision loop |
| 0x1102c8, 0x10e5e8, 0x10e140, 0x10e4b8, 0x10e5b8, 0x110468, 0x10c8d0, 0x10c748 | follow, steer, jump |
| 0x110eb0, 0x110a58, 0x1108c8, 0x110e08, 0x110e28 | warp |
| 0x111ae0, 0x1113dc..0x111400, 0x1115d0, 0x0016b960, 0x001beb00, 0x00182690 (+0x660 copy), 0x0016ef28, 0x0016ee98, 0x001845b0, 0x001751b0 | WP-7P1: snap to follow point, cooldown count-down, follow distance, position getter, collision word, hand objects |
| 0x113450, 0x1133c0, 0x1133e0, 0x114550, 0x114668, 0x114078, 0x1140c0, 0x114038, 0x114048, 0x114108, 0x113578..0x1135a0, 0x1143b8, 0x114348, 0x1144d0 | AI classes and parameter readers |
| 0x1135e0, 0x113728, 0x113888, 0x113b08, 0x1137d8, 0x113d48, 0x113e68, 0x114738, 0x114828, 0x10c8f0, 0x10c9d0, 0x10c800 | action handlers and command starts |
| 0x10cbe0, 0x10d270, 0x10d378, 0x10d538, 0x10cf80, 0x10cf88 | target styles |
| 0x10f490, 0x10f4d0, 0x1a0be0, 0x1a05d0, 0x10f420, 0x10f438, 0x112308, 0x112350 | thresholds, random, settings |
| 0x111c18, 0x111d18, 0x111670, 0x112168, 0x112260, 0x112270, 0x113208, 0x17d5a0 | party manager |
| 0x1c76b0, 0x1c77a8, 0x1c77f0, 0x1c75c0, 0x1c75d0, 0x1c7600, 0x1c7ab0, 0x1c7be8, 0x1b3f78 | hide / return members, forms |
| 0x2f6a90..0x2f75c0 | host group 10 (ghidra decompiled the 31 the decompilation lacked) |
| 0x1903d8, 0x17da30, 0x17d960, 0x19a898, 0x1752a0, 0x17e000, 0x17e220, 0x37a5c8 | WP-7P2: battle flag, motion remap and its table |
| 0x1137d8, 0x10c8f0, 0x110790, 0x10cbe0, 0x1112e8, 0x114738, 0x114828, 0x10c748, 0x110728 (ghidra) | WP-7P2: attack handler, command, attack motion, re-target, friend update, Goofy's handlers |
| 0x10fcd0, 0x10f220, 0x10f118, 0x10fc68, 0x10ed00, 0x10ed20, 0x10ecb0, 0x10f3d8, 0x10c800, 0x10f698, 0x10f688, 0x10f6a8, 0x10f670, 0x10f7a8, 0x10f7b8, 0x10ca78, 0x10ca88, 0x10d0c0, 0x10d848, 0x10dcf0, 0x10cf90, 0x112b28, 0x112b88, 0x112ac0, 0x10d720, 0x10e5e8, 0x1102c8, 0x1103e0, 0x1103f8, 0x111040, 0x110f90, 0x10f8d8, 0x113450, 0x1135e0, 0x113728, 0x1143b8, 0x114348, 0x1144d0, 0x10f578, 0x174cf8, 0x176b00, 0x1b6f48, 0x114058, 0x114068, 0x114108, 0x114048, 0x2f0758, 0x17eb48 | WP-7P2: choice, feasibility, range, targets, movement, records |

| 0x196b58 (tail 0x196cd8..0x196cf8), 0x194438, 0x1943c8, 0x1cc1c8, 0x1cc140, 0x1cc040, 0x37c290 | WP-P3b: the record at +0x12c / the movement block +0x110 = `prty` row of the NeoMoveset |
| 0x17ce60, 0x17ca70, 0x17cc20, 0x17cee8, 0x186238, 0x176c98, 0x186530, 0x186580, 0x1a0100, 0x1903c8, 0x1b7e00, 0x1d9f20, 0x17cde0, 0x1106b8, 0x175340, 0x185a68, 0x175f10, 0x37a55c, 0x37a560 | WP-P3b: hits, KO, revive |
| 0x111070, 0x110f90, 0x10f830, 0x10f8d8, 0x10c800, 0x1b0758, 0x113888, 0x10c9d0, 0x175e20, 0x1b8710, 0x10f510, 0x1a1d50, 0x113d48, 0x113e68, 0x114828, 0x1870d8, 0x187458, 0x186e70 | WP-P3b: battle branch, follow-up, items, specials |
| 0x170a00 (1.276), 0x1769e0, 0x186368, 0x186440, 0x1a0180, 0x1a0540, 0x1a05a8, 0x1a05d0, 0x37a974, 0x173740 (1.140), 0x18ed58 | WP-P3b: the MP-cost host and the MP charge |
| 0x18e2f0 (2.10), 0x1a2ba0, 0x18bd08, 0x18e488 (2.31), 0x1a2c58, 0x18e348 (2.11), 0x1a30e0, 0x18e3a0 (2.12), 0x1a2fc8, 0x18e680 (2.62), 0x1a3480, 0x18e3c0 (2.13), 0x18dc98, 0x18dce0, 0x18c2d8, 0x18c3f0, 0x18c0d8, 0x1a2cc0, 0x300278, vtables 0x35e788 / 0x35cf58, 0x37b45c, 0x37b460, 0x356f40 | WP-S3: the attack objects |
| 0x171800 (1.87), 0x171840 (1.149), 0x1717c0 (1.22), 0x16a570, 0x16a530, 0x16a5b8, 0x170510 (1.130), 0x16d098 (0.45), 0x16d138 (0.46), 0x172fe8 (1.91), 0x175aa8, 0x2ee718 (6.3), 0x2f01a0, 0x2ee750 (6.5), 0x2f04a8 | WP-S3: the effect starts and Goofy's tornado hosts |
| 0x19f920, 0x1b50c0, 0x1b58d8, 0x1a0c08 (named), 0x352ec0 | WP-S3: the rebuilt status numbers, the special ability items |
| 0x1726d8 (1.247), 0x1a5ea8, 0x1a6068, 0x1a6198, 0x1a5c30, 0x1a5bf8, 0x37b5a8..0x37b5c0, 0x2ee718 (6.3), 0x2f01a0, 0x2ee738 (6.4), 0x2f0748, 0x2ee750 (6.5), 0x2f04a8, 0x2f0218, 0x2f03f8, 0x2f01d8, 0x2f0438, 0x16b4e0, 0x1894d0, 0x18ed40, 0x383fb4..0x383fbc | WP-S5: turbo, the target search |
| 0x174280 (1.234), 0x172e68 (1.83), 0x170658 (1.148), 0x172f90 (1.89), 0x1706c8 (1.161), 0x170e38 (1.102), 0x170dd8 (1.18), 0x18e4f0 (2.38, disassembled), 0x18cf50, 0x18dbd8, 0x16c550, 0x173770 (1.160), 0x18f490, 0x18e530 (2.40), 0x18e828 (2.67), 0x18c890, 0x18c8e0, 0x18ca78, 0x18d430, 0x1a2cc0 | WP-S5: blade, the attack flags and the wall test |
| 0x171780 (1.21), 0x16a530, 0x16db50 (0.41), 0x17be00, 0x1d9d50, 0x2eeb30 (6.9), 0x18f308, 0x18eec0, 0x189628, 0x18e420 (2.29), 0x18e518 (2.39), 0x18e7a0 (2.19), 0x18e570 (2.43), 0x18ea60 (2.57), 0x2ef040 (6.51), 0x2ef110 (6.54), 0x1d7cb0, 0x310af4 | WP-S5: blizzard and the interpreter's copy |
| 0x2ee718, 0x2ee738, 0x2ee750, 0x2f7540, 0x2f01a0, 0x2f0748, 0x2f0758, 0x2f04a8, 0x2f0218, 0x2f03f8, 0x2f01d8, 0x2f0438, 0x2f02f8, 0x2f02b0, 0x19a5d0, 0x19a3f8, 0x19a440, 0x1b7c88, 0x1b80e8, 0x16b4e0, 0x188db8, 0x19a6a0, 0x19de20, 0x19de48, 0x1894d0, 0x16aa18, 0x18ed40, 0x18f490, 0x17bbc0, 0x3003e8, 0x300f18, 0x169978, 0x1beb00, 0x1beb40, 0x1903d8, 0x184820, 0x1746e0, 0x383fb4..0x383fbc, 0x37a528, 0x37a52c, 0x1d5d7a0 | WP-S6: the target search |
| 0x172f40 (1.88), 0x16b6e0, 0x173770 (1.160), 0x18e7a0 (2.19), 0x1a30a8, 0x18e570 (2.43), 0x1a3280, 0x18e5d8 (2.44), 0x18ea60 (2.57), 0x1b3c48, 0x2ef040 (6.51), 0x2ef110 (6.54), 0x18efa0, 0x112370 (10.14), 0x10d720, 0x18ed58, 0x18ee20 | WP-S6: blizzard's later hosts |
| 0x172f90 (1.89), 0x183418, 0x181e00, 0x1b74e0, 0x172d98 (1.79), 0x16a828, 0x16a7f0, 0x19ad98, 0x16a9a8 | WP-S6: the hand object's velocity and facing |

## Corrections to other docs (for the orchestrator)

- `docs/player-data.md`, `memt`: the 16 words at +0x10 are member objects by member index (0 player, 1 Donald,
  2 Goofy, 3 partner, 4..8 forms, 9 Mickey, 10..15 keyblades), merged per world; index 0..3 are not "world
  outfits". Sora's per-world object comes from the merge (world 14 `P_EX100_NM`, 17 `_TR`, 13 `_WI`).
- `docs/player-data.md` / `docs/drive-forms.md`: FUN_001c77a8 takes **every** member in forms 3..5 (any
  NeoStatus), not "both".
- `docs/player-data.md`, ability block +0x58: it also scales the party member's revive time (FUN_0017ce60).
- (WP-S4: `receive_hits` takes a member's strength / magic / defense from the rebuilt status too, `enemy::member_stats_rebuilt`; Donald's thunder no longer does 2.)
