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
  12..15 hold walk 2, run 7..18 per character; StunTime is 0 in every row. [I] that the friend's walk and run
  speeds at object +0x110 / +0x114 (FUN_0010e5b8) come from these rows; the writer was not traced.
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

- KO (friend vtable +0x5c, FUN_0017ce60): FUN_00175f10 (voice line 2, FUN_00186568); revive timer
  `+0xb40 = (record at +0x12c)+0x18 x ability block +0x58` (Hyper Healing multiplier) [GAP: which record
  +0x12c points to; `prty` StunTime is 0 for every row, so it is not that].
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
- The record at +0x12c whose +0x18 is the revive time (searched: `prty` StunTime is 0 in all rows).
- Writers of the walk/run speeds +0x110/+0x114 (FUN_0010e5b8 reads them).
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

- The combo follow-ups (FUN_0010c800 -> FUN_001b0758 not read): a chosen follow-up starts nothing.
- Items, specials, Cure and the emergency pass (stage 4: they need the member's status block and the MP gate
  FUN_00110f90); they are never feasible here.
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
- FUN_00111040 forces motion state 0x1f only for classes 12, 15 and 16, so Donald and Goofy (classes 0, 1)
  may follow through FUN_0010e5e8's body (state 0) rather than FUN_0010e140 as stage 1 assumed; which state a
  free class-0 member stands in was not determined. The field follow keeps stage 1's port.
- The random generator is not the game's.

## Decompile index

| address | what |
|---|---|
| 0x16e8c0, 0x16e208, 0x17cd20, 0x17cd78 | object factory, stand-in ids, slot -> object |
| 0x1a6f48, 0x1a6ff0, 0x1a6f70, 0x1802e0 | memt pointers, merge, record test, story flag |
| 0x1a71c0, 0x1a71e8, 0x1a7128, 0x1a7118, 0x1a7200, 0x1a7250, 0x1a72c0, 0x1a7330, 0x1a7500, 0x1a7558, 0x1a7608, 0x1a7268, 0x1a7698 | slot bytes, presets, member index, in-party test |
| 0x180b48 | area-script commands (0xf preset, 0x10) |
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

## Corrections to other docs (for the orchestrator)

- `docs/player-data.md`, `memt`: the 16 words at +0x10 are member objects by member index (0 player, 1 Donald,
  2 Goofy, 3 partner, 4..8 forms, 9 Mickey, 10..15 keyblades), merged per world; index 0..3 are not "world
  outfits". Sora's per-world object comes from the merge (world 14 `P_EX100_NM`, 17 `_TR`, 13 `_WI`).
- `docs/player-data.md` / `docs/drive-forms.md`: FUN_001c77a8 takes **every** member in forms 3..5 (any
  NeoStatus), not "both".
- `docs/player-data.md`, ability block +0x58: it also scales the party member's revive time (FUN_0017ce60).
