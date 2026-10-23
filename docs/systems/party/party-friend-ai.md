# Party: friend AI, host group 10, unknowns, index

[Contract](contract.md) | [Index](../../README.md)

## 4. The friend AI

### Classes [V, FUN_0010b780; oracle-printed thresholds]

| NeoStatus | AI class (index into 0x34ea00) | constructor | AI vtable | action records |
|---|---|---|---|---|
| 2 Donald (moveset 0x12 `P_LK020` -> 12) | 0 / 12 | FUN_00114550 | 0x353368 | 0x353090, 20 |
| 3 Goofy (moveset 0x15 `P_LK030` -> 13) | 1 / 13 | FUN_00114668 | 0x353630 | 0x3533a0, 18 |
| 5 Auron, 15 Ping, 6 Mulan, 7 Aladdin, 8 Sparrow, 9 Beast, 10 Jack, 11 Simba, 12 Tron, 13 Riku | 2, 3, 4, 5, 6, 7, 8, 9, 10, 11 | FUN_00114890, FUN_001149c8, FUN_00114ae0, FUN_00114c38, FUN_00114d90, FUN_00114f90, FUN_001150e8, FUN_00115260, FUN_001153b8, FUN_001154d0 | per class | per class |
| 200..204 summons | 14..18 | docs/systems/drive-limits-summons/contract.md | | |
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

### Targets (battle style = setting byte 0) [V; WP-PA1]

FUN_0010df08 asks FUN_0010cf90 whether to re-target and then FUN_0010dcf0 for a new record. Both index a
table by the style (vtable +0x98 = setting byte 0) for classes up to 0xd (0x352d28 choose, 0x352cd0
re-target, 8 bytes per row), and by the class for classes above 0xd (FUN_001124b8: +0xb6c > 0xd; choose
0x352ce8 + 8 x class, re-target 0x352c90 + 8 x class). While a limit runs (FUN_001b3c00) the choose
function is 0x352d48's FUN_0010d700 (empty: no target) and the re-target answer is always yes. The re-target
answer is also yes when the target is not valid (FUN_0010ca88: +0xb7c bit 0x40, FUN_00112b28, FUN_0018edd8).
FUN_0010dcf0 clears the record before the choice (a choice that finds nothing leaves no target), clears it
again when the result does not answer, and sets +0xb7c bit 8 when the record changed (FUN_0018f298).

| style | choose | re-target when (the target is valid) |
|---|---|---|
| 0 | FUN_0010d0c0: the remembered attacker's (+0xcec) nearest shape within +0x68 (FUN_00112b88); else among the targetable objects' enabled shapes the nearest surface below +0x68 | FUN_0010cbe0: an attacker is remembered and is not the target's object, or the target point is farther than +0x6c |
| 1 | FUN_0010d270: FUN_0010d848's pick when its shape is enabled; else, when the player's target passes FUN_00112b28, that object's nearest shape within +0x68 (FUN_00112b88) | FUN_0010cc80: the player's target record answers and names another object |
| 2 | FUN_0010d378: the nearest surface to the member below +0x68 whose surface distance to the player is below +0x84 | FUN_0010ccf8: the target point farther than +0x6c from the member, or farther than (+0x84 + +0x88) x `DAT_003784a4` (0.5) from the player |
| 3 | FUN_0010d538: among the shapes within 2 x +0x68 of the member, the one farthest from the player (start `DAT_003784a8` = -1) | FUN_0010ce40: the running record (+0xc60) exists and lacks flag 1; or the target point farther than +0x6c from the member; or closer than +0x88 to the player |
| 4 | FUN_0010d700: nothing | FUN_0010cf80: always |
| 5 | FUN_0010d708 = FUN_0010d0c0 | FUN_0010cf88: never |

Classes above 0xd (printed by `party_target_tables_from_the_executable`): 14 nothing / always; 15 and 17
FUN_0010d0c0 / never; 16 FUN_00119d60 / FUN_00119d58 (section 20); 18 FUN_0010d0c0 /
FUN_0010cbe0.

"Surface distance" = |shape centre - position| - shape radius (FUN_001894d0 gives centre and radius). The
walks visit the object list (FUN_0018f348 over FUN_001b7c88) in order, comparisons strict as written:

- FUN_00112b88(range, object, out, position): the object's enabled shapes; a shape is taken when the best so
  far is <= 0 or its distance is below the best, and becomes the best (starting at `range`). So a range of
  0 or less, or a taken distance of 0 or less, lets every later shape of that object win. With no position it
  takes the first enabled shape (not reached from the friend AI).
- FUN_0010d848 (style 1): needs `0x34e9c4`, the object of the player's latest attack start
  (FUN_00176050 -> FUN_00112850(1, record): written only when FUN_00112b28 accepts the record; zeroed by
  FUN_00112270 and by FUN_00111d18 when the object dies). When the player's target record (`0x34e9b0`, copied
  from the player's +0x9f4 by FUN_00111d18) answers and names that object, it is taken whole; else that
  object's shapes (no enabled test here) with the best starting at 0: the first is taken, then any nearer.
- FUN_00112b28(record) = FUN_0018ed58 (the record answers: +8 non-zero or FUN_0018ee20) and FUN_00112ac0 on
  the object (alive FUN_0019a3f8, +0x588 bit 0x10, +0x7f8 bit 4 clear, +0x108 bit 0x8000 clear).
- The second distance of FUN_0010ccf8 / FUN_0010ce40 is the target point's to the player. The decompilation
  shows a confusing vector copy there (`if (a != b) a = c`); the kh2-ee run of the four tests
  (`party_retarget_tests_match_the_executable`, 3840 cases) settles it.

Battle distance (FUN_0010df08): when the member's ground distance to the player (+0xbac) exceeds block +0x80
(x2 for styles 3 and 5, x0.5 for style 2), +0xb7c bit 0x40 is set, and a target with that bit set is not valid
(FUN_0010ca88): the member drops its target and follows (the follow step clears the bit). The target is
refreshed only in battle, when the member is not busy (+0x390), has no script-set target (+0xb7c bit 0x10000,
host 10.14) and the re-target test asks for it. Hits on the member remember the attacker in +0xcec
(FUN_001106b8: the AI vtable +0xc, FUN_0017cde0, then +0xcec = the hit's +0x10 when FUN_00112ac0 accepts it).
FUN_00110728 (the other hook beside it) is the AI vtable +0x10 then FUN_001767a8 (a limit's FUN_001b3d18, or a
forward to the ridden object's +0x74, then message 0xf to +0x484): it reads no attacker statistics and has
no part in the target choice. [GAP: the style names; who besides the setting menu writes setting byte 0.]

In the app (`party_battle.rs`): `choose_style0..3`, `player_hit_pick`, `nearest_shape_of`, `dispatch`,
`choose_target`, `needs_retarget`. WP-PA2 wired them into `party.rs`: `refresh_target` (the FUN_0010df08
refresh: re-target test, then the choice) runs `dispatch(class, style)` for every member in battle, and
`battle_distance` takes the member's style. Inputs:

- `style` = `PartyMember::style`, the setting byte 0 (WP-PA3: `Progress::member_setting(neo, 0)`, re-read every
  frame in `party_tick`; 0 at a new game; a `KH2_STATE` key `party_settings` sets it, docs/systems/core-runtime/progress.md; the
  harness `KH2_PARTY_STYLE=<n>` still overrides both members; read once per run, not per frame). A style byte above 5 (the
  table has six rows) reads as 0 [inferred; the original would read past the table]; `party_settings` in a state file rejects
  a style above 5 and any value above 255 instead of truncating. [GAP: no page writes it yet: the page that does is
  Customize page 0x1b for Donald / Goofy (`FUN_00271078` with another character, docs/systems/ui/menus-pause.md 2.4), which is not
  ported (`menu_customize.rs`, outside WP-PA3's ownership; the Party page 0x14/0x16 only swaps slots, WP-MN7/MN8).
  Searched: docs/systems/ui/contract.md and docs/systems/party/contract.md, `menu_party.rs`, `menu_config.rs` (the Config rows are the option
  word, not per-member). The Customize page's row builder for the others' table (0x381d48: kinds 3, 4, 2) was
  not read.] A style outside 0..5 makes no choice (the table has six rows).
- +0xcec, the remembered attacker: `PartyMember::attacker`, set in `party_receive_hits` for every enemy hit the
  member takes (all enemies in the app are targetable objects).
- `0x34e9b0`, the player's target record: Sora's lock-on, else its auto target, on shape 0 [inferred: the app
  keeps the object, not the shape]; dropped when that enemy is gone.
- `0x34e9c4`, the player's latest attacked object: `Manager::player_hit`, the object of the player's latest landed
  hit [inferred: the game writes it when the attack starts, FUN_00176050]; cleared when the object is gone.
- `current_counted`: the running action record (`Choosing::current`) lacks flag 1. Target validity (FUN_0010ca88)
  includes bit 0x40 (too far from the player by the style's battle distance).
- `limit` is false (no limits exist in the app yet); every enemy shape is enabled and targetable.

Log: `KH2_PARTYLOG=1` prints `party: <model> style <n> target ...` when the target changes. Tests:
`party_every_style_chooses_by_its_own_rule` (all six styles, the re-target tests, a class above 0xd, a style
outside the table) and `party_battle_distance_follows_the_style`.

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
| 10.50 | 0x2f7440 | 1 ret | status +0x180 (MP); no script on the disc calls it (WP-RV2) |
| 10.51 | 0x2f7450 | 4 | FUN_0011a6d8 |
| 10.52 | 0x2f7490 | 1 ret | FUN_00185a38 |
| 10.53, 10.54 | 0x2f74c0, 0x2f74f8 | 2 / 2 ret | FUN_00136388 / FUN_001364d0 on FUN_0010c5b8 ([I] HUD) |
| 10.55 | 0x2f7540 | 2 | FUN_002f0758(a, b != 0) |
| 10.56 | 0x2f7560 | 2 | FUN_00110dc8 |
| 10.57 | 0x2f7580 | 1 ret | push a position to the player trail (45-unit spacing) |
| 10.58 | 0x2f7598 | 3 | FUN_001131e8: friend object +0xb50 = a record, +0xb54 = a value, bit 2 of the record's object word +0x1c set; only the destructor FUN_0010c6e0 reads the pair; called by P_EH000 and P_EH000_LAST (WP-RV2, docs/systems/script-host/contract.md) |
| 10.59 | 0x2f75c0 | 2 | FUN_0011a6b8(AI, a) |

Use in the members' scripts (`kh2 bdx`): Donald 10.31 x2; Goofy 10.14, 10.31, 10.40, 10.55 (x2 each);
Auron 10.29, 10.31 x2, 10.55; Riku 10.14 x4, 10.17, 10.18, 10.29, 10.31, 10.35, 10.38, 10.39, 10.43, 10.53,
10.54, 10.58; Tron 10.31 x2, 10.35 x4, 10.40 x2, 10.55. The members' scripts use mostly groups 1 to 4
(69..92 distinct host functions each), so the scripts' own host needs are the larger cost of stage 2.

## 6. Combat

- **Class methods** [V]: the friend vtable 0x352d88 keeps the party class's combat methods (shared with
  Sora's vtable 0x35c9a8): +0x6c clash FUN_001754a0 (docs/systems/combat-effects/hit-reaction.md, ported as
  `reaction_rules::party_clash`), +0x70 FUN_00175668, HP change +0x78 FUN_00176c98 -> FUN_00186238. Overrides:
  +0x64 receive FUN_001106b8 (AI hook, FUN_0017cde0 = FUN_00175340 the `patn` receive path as for Sora, voice 9
  when HP enters danger, attacker remembered), +0x74 attacker callback FUN_00110728, +0x7c attack motion
  FUN_00110790, +0x84 FUN_001115a0 (FUN_00174c28 unless +0xb7c bit 0x400).
- **Attacks**: the member's motion set's attack volumes with `atkp` rows, as for any object (combat.rs);
  the attacker's stats from its status block. (FUN_00110728, read in WP-PA1: the AI vtable +0x10 then
  FUN_001767a8, no statistics; section 4 "Targets".)
- **Invulnerability**: +0x7f8 bit 0x10 (hidden), the party tick FUN_00174b68 timers (docs/systems/combat-effects/hit-reaction.md
  +0xb00) apply. [I] events: not traced.
- **Reaction commands**: the party code offers limits through FUN_0019d430 -> FUN_001d25e0 (docs/systems/combat-effects/player-data.md
  "Triggering"); members within `DAT_0037c414` with flag 2 qualify.
- **Limits**: FUN_001b28a0 marks participants (+0xb4c, +0x10c 0x10000) and puts both companions in the limit
  state (FUN_001c75d0) -> hidden list; target choice switches to FUN_0010d700 during a limit.
- **Drive forms**: FUN_001b4c28 -> FUN_001c76b0: each member the form takes (FUN_001c77a8: Valor Goofy,
  Wisdom Donald, Master/Final/Anti everybody, oracle-checked) is faded out and listed (15 frames,
  `DAT_0037c050`); the area spawn skips them while the form lasts (FUN_0017d1b0).
- **Summons**: docs/systems/drive-limits-summons/contract.md (fade 15 frames out and back, FUN_001c7740 / FUN_001c79c8).

## 7. HUD and audio

- HP / MP panel per member: registered at creation (FUN_0013dcd8(slot + 1, obj) on the table 0x355768); the
  panel reads the member's status block (HP +0, max +4, MP +0x180, max +0x184; the rules in docs/systems/ui/hud-notes.md
  "Party panels", 0x154818 / 0x154930). Summons get none.
- Voices: friend vtable +0x54 (line ids 2 KO, 9 danger); the bank by NeoStatus (docs/systems/audio/audio.md). Positional
  sound effects of the members' motions use their `.apdx` banks.

## 8. Where it goes in the app

| file | function / item | change |
|---|---|---|
| `crates/kh2-tables/src/player.rs` | `Memt` | name the record fields (world, flag, not-flag, budgets, 16 members) and the presets; `merged(world, flags)` and `slot_object(byte)` as in the tests |
| new `party.rs` (kh2-party) | `PartyPlugin` (third plugin tuple of `main.rs`) | `PartySlots` (the four slot bytes per world, hidden list), `PartyMember` component (slot, position 1/2, NeoStatus, AI class, follow mode +0xb78, far counter, warp cooldown, revive timer, last three actions) |
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
- The meaning of the settings' values 0..3 (WP-PA4: the menu offers 0 Free / Often, 1 Well-Balanced, 2 Rare / Last hope!, never 3; the page's
  messages say how it reads) and the styles 0..5 (Technic, Target, Huddle, Party, Sora, Relentless Attack: messages
  0x486..0x48b, docs/systems/ui/menus-customize.md 14b; what each does is the target table of section 4).
- Who writes the save slot bytes at new game; `lvup` table order beyond Sora.
- Per-class constructors of the world partners (FUN_00114890..FUN_001154d0) and their action records were not
  dumped; the oracle test prints only their thresholds.

## Decompile index

| address | what |
|---|---|
| 0x10dcf0, 0x10cf90, 0x352d28, 0x352cd0, 0x352ce8, 0x352c90, 0x352d48 | target choose / re-target dispatch tables (WP-PA1) |
| 0x10d0c0, 0x10d270, 0x10d848, 0x10d378, 0x10d538, 0x10d700, 0x10d708, 0x112b88, 0x112b28, 0x112ac0 | the choose functions and their walks |
| 0x10cbe0, 0x10cc80, 0x10ccf8, 0x10ce40, 0x10cf80, 0x10cf88, 0x114058, 0x114068, 0x114118, 0x114128, 0x3784a4, 0x3784a8 | the re-target tests and their block words |
| 0x112850, 0x112270, 0x111d18 (0x34e9b0, 0x34e9c4), 0x1106b8, 0x110728, 0x1767a8 | the player's target and attacked object; the hit hooks |
| 0x16e8c0, 0x16e208, 0x17cd20, 0x17cd78 | object factory, stand-in ids, slot -> object |
| 0x1a6f48, 0x1a6ff0, 0x1a6f70, 0x1802e0 | memt pointers, merge, record test, story flag |
| 0x1a71c0, 0x1a71e8, 0x1a7128, 0x1a7118, 0x1a7200, 0x1a7250, 0x1a72c0, 0x1a7330, 0x1a7500, 0x1a7558, 0x1a7608, 0x1a7268, 0x1a7698 | slot bytes, presets, member index, in-party test |
| 0x180b48 | area-script commands (0xf preset, 0x10) |
| 0x181148, 0x180a68, 0x180ad8, 0x180b48, 0x1a7330 | WP-P3: the area load runs map / btl / evt, opcode 0xf applies the preset |
| 0x1abfe8, 0x1ac130, 0x1ac170, 0x19fe48, 0x19ffb8 | opcode 0x1d party reset (WP-S8: removes nobody) |
| 0x19d9d8, 0x104e18, 0x104d58, 0x1a7330, 0x1abfe8, 0x1ac130, 0x1a0100, 0x1a0240, 0x1a0140, 0x1c75c0, 0x1c7be8, 0x24f300, 0x250450, 0x26ef80, 0x26e618, 0x26e970, 0x26e130, 0x26e158, 0x26e8d0, 0x1d73d0, 0x1b5868 | WP-PT1: the jump record's party words, the reset, page 0x15 |
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

- (WP-PA1, done in WP-PA2) `party.rs` now calls `battle::dispatch` / `needs_retarget` / `choose_target` with the
  remembered attacker, the player's target and attacked object. (WP-PA3 done: `Member::settings`, the
  `party_settings` state key, `party.rs` reads byte 0.) (WP-PA4 done: the Customize page 0x1b writes
  byte 0 and the use-setting bytes, `menu_customize.rs`, docs/systems/ui/menus-customize.md 14b.)

- `docs/systems/combat-effects/player-data.md`, `memt`: the 16 words at +0x10 are member objects by member index (0 player, 1 Donald,
  2 Goofy, 3 partner, 4..8 forms, 9 Mickey, 10..15 keyblades), merged per world; index 0..3 are not "world
  outfits". Sora's per-world object comes from the merge (world 14 `P_EX100_NM`, 17 `_TR`, 13 `_WI`).
- `docs/systems/combat-effects/player-data.md` / `docs/systems/drive-limits-summons/drive-forms.md`: FUN_001c77a8 takes **every** member in forms 3..5 (any
  NeoStatus), not "both".
- `docs/systems/combat-effects/player-data.md`, ability block +0x58: it also scales the party member's revive time (FUN_0017ce60).
- (WP-S4: `receive_hits` takes a member's strength / magic / defense from the rebuilt status too, `enemy::member_stats_rebuilt`; Donald's thunder no longer does 2.)

