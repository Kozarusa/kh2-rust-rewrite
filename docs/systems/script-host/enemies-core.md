# Enemies: which ones the app has, and why

[Contract](contract.md) | [Index](../../README.md)

Work package 6C. The app's enemies are Shadow (`M_EX020`), Soldier (`M_EX010`) and, since 6C, the Dusk (`M_EX990`).
All three run under their own BDX scripts (`docs/systems/script-host/contract.md`) and take hits through the reaction code
(`docs/systems/combat-effects/hit-reaction.md`). Nothing from the disc is stored in the repository: area files, models, motions,
scripts and tables are read at start-up or when an area loads.

## Choosing the next enemy from the data

The areas the app loads are tt00, tt02 and tt04. Their area files (`ard/<area>.ard`) hold the area scripts
(BAR type 13: `map`, `btl`, `evt`) that load spawn groups (BAR type 12). `crates/kh2-ard` reads both
(layout in its header); its disc test lists every object the three scripts can spawn:

```
cargo test -p kh2-ard --test disc -- --nocapture
```

Objects with `00objentry` ObjectType 4 (the enemies; Shadow, Soldier and the Dusk have it), counted over every
spawn group every program of every script loads (a group loaded by two programs counts twice):

| object | model | count | where |
| --- | --- | --- | --- |
| 0x13e | `M_EX990` (Dusk) | 13 | tt04 `btl`: b_00 (program 0x1, 3), b_44 (0x50, 7), b_43 (0x4f, 1), b_21 (0x7, 2) |
| 0x678 | `N_EX570_BTL` | 4 | tt04 `btl`: b_41, b_42, b_45, b_51 (Struggle opponents) |
| 0x138 | `M_EX930` | 1 | tt04 `btl`: b_21 |
| 0x4d7 | `N_EX500_BTL` | 1 | tt04 `btl`: b_50 |
| 0x82f | `N_EX600_BTL` | 1 | tt04 `btl`: b_52 |
| 0x139 | `M_EX940` (type 0x15) | 4 | tt04 `btl`: b_44 (0x50: serial 0x26 wave 1; 0x102..0x104 wave 2) |
| 0x131 | `M_EX760` | 15 areas | bb08 `btl`: b_20 (11 members), b_21 (7); also ca09, ca13, ca19, hb02, hb08 .. (survey, `docs/systems/script-host/area-survey.md`) |

tt00 spawns only the player; tt02 spawns the player, the two friends, save and world points, `F_EX000` and event
actors (ObjectType 15). Neither Shadow nor Soldier is in any of the three scripts: the app's Shadow/Soldier waves
are its own (see "Waves"). `M_EX940` (object 0x139, four in b_44) has ObjectType 21 (0x15, the second enemy class; spawned since WP-EN2, "M_EX940 and the object type 0x15").

The most common enemy is the Dusk. Its model, motion set (`obj/M_EX990.mset`, 46 motions), effect file
(`obj/M_EX990.apdx`) and BDX script (BAR type 3 of the model file, 6378 words) are on the disc; the test asserts that.

## Next kinds (WP-SV3, 2026-10-20)

`enemy::KINDS` has 38 kinds. `docs/systems/script-host/area-survey.md` "Ranked gaps" lists the absent ones from the disc (areas, groups, `enmp` row, missing host calls, boss or not) and a recipe for the next twelve: `M_EX900` (code exists; needs a soak, the event 17 sender was found by WP-DYN1), `M_EX130`, `M_EX710`, `M_EX890`, `M_EX680`, `M_EX570`, `M_EX580`, `M_EX780`, `M_EX740`, `M_EX550`, `M_EX500`, `M_EX590`. `M_EX950` is already in `KINDS` (its reaction-command modes are a gap of the kind); the cheapest boss is `B_LK110_PHANTOM`.

## The Dusk

- **Stats.** The script calls `set_obj_param` (host 2.9) with the constant 41 (BDX `M_EX990` at 0x073c), the
  `enmp` row the game builds the status block from (FUN_001b2148). `Kind::DUSK` uses row 41 with the app's
  world level 1 (as for Shadow and Soldier): 32 HP.
- **Model, motions, hit and attack shapes** come from `obj/M_EX990.mdlx`/`.mset` like the others. Attack windows
  are motion range triggers 10 and 33 (claw 201: atkp 16, kick 202: atkp 17, walk attack 203/204: atkp 602 and
  18); they need no new code.
- **Reactions** use the default slots (10/12 flinch, 16..19 blow and landing) through `enemy.rs` unchanged.
- **Death.** The `dead` mode starts element 2 of its own effect file (`spawn_attached(obj, 2, 1, 0)`), the same id
  as the Heartless, so `GameEffect::Death` plays the Dusk's own death effect; then fade and hide.
- **Spawn kind.** Host 1.110 reads byte +0x1c of the object's spawn record (FUN_0016b858). The Dusks of b_00 and b_21
  have 2, those of b_43 and part of b_44 have 3; the app passes the record's value (the app's own waves pass -1,
  no record). 2 and -1 take the same appear path in the script.

New host functions the Dusk needs are in the table of `docs/systems/script-host/contract.md` (0.15, 1.62/1.64, 1.63, 1.66, 1.69, 1.74,
1.98, 1.150).

## Waves

**What the game does (WP-EN1)** [V unless marked]. An area's enemies come from its scripts, not from a timer:

- `FUN_00181148` runs the `map`, `btl` and `evt` programs named by the area record (docs/systems/core-runtime/areas.md section 1);
  opcode 0 (`spawn`) loads every named type-12 group (`FUN_00181728`: one live group per record of the tag,
  `FUN_00198430`). Setting a group up (`FUN_00198750`) creates its entities by spawn kind (record +0x1c):
  0 and 3 at once (`FUN_00198920` -> `FUN_001982c8` -> `FUN_00198220`), 1 only for the current entrance
  (`FUN_001981a0`), 2 only counted (`FUN_001994c8`) for an enemy (object type 4 or 0x15); a disabled group
  (kind 5 or flag bit 0, `FUN_00198430`; `FUN_00198920` tests bit 0) and a kind-9 group create nothing.
- A **kind-2 group** creates its spawn-kind-2 members on demand: every frame `FUN_001989d8` runs the group's
  timer (+0x14) down by the frame step `DAT_00356f40` and, while the player is inside one of its volumes, calls
  `FUN_001991a0` (through `FUN_00198b30`). That walks the members in record order; once the timer runs it stops.
  A member that is absent (`FUN_0016b0d8` by the serial +0x1e), not in the table at 0x35d278, and whose wave byte
  (+0x30) is at most the group's wave (+0x20, 0 at set-up) is created (placement by +0x1d: 0 the record's point,
  1 near the player `FUN_00199970`, 2 within the radius +0x2e of the point `FUN_00199a80`; both below) and the timer becomes
  its delay (+0x2a). When the walk ends with the timer at 0, `FUN_00199618` moves to the next wave that has
  members, if the `00objentry` `SpawnLimiter` bytes (row +0x54) of the members in play (spawn kinds 0, 3, and 2 of
  the waves so far, except those defeated for good; `FUN_001996f0`, `FUN_001995c8`) add up to at most header
  +0x22; header +0x23 (not 0) is the wave at which it goes back to wave +0x24 (`FUN_001997f8`, `FUN_00199858`).
  On that wrap `FUN_00199858` first walks the group's entities: each spawn-kind-2 one whose wave byte (+0x30) is not below
  the new wave and that `FUN_001995c8` finds defeated (serial in the table and the object absent, `FUN_0016b0d8`) is taken
  out of the table (`FUN_001942c8` -> `FUN_001940d8`, which zeroes the serial's entry), so it comes again in the next lap
  (`KindTwo::run`, test `a_wave_wrap_takes_the_defeated_members_out_of_the_table`). [inferred] that `FUN_001942c8` takes the
  serial as its second argument: Ghidra's signature shows one parameter, the call site loads the serial halfword.
  The timer starts at header +0x1c (`FUN_00198430`).
- **Defeats.** `FUN_00198220` enters a created enemy (object types 4, 0xd, 0x15, 0x17, `FUN_00198150`) in the table
  at 0x35d278; when the object is removed (`FUN_001d5db8` -> `FUN_001993f8`) it leaves the table if it was alive
  (+0x7f8 bit 4 clear) or if its wave byte is above 99 (`FUN_00199378`), and the group's timer becomes header
  +0x20. So a defeated member of a wave up to 99 does not come back; the table is kept per area for four areas
  (`FUN_00194188`). The emptied group fires event 3 with its id, all groups emptied event 2 (`FUN_00199518`).
  WP-PL1 check of the classes: `FUN_00198150` reads the `ObjectType` byte (+4) of the objentry row `FUN_0016e208` resolves for the
  entity and answers 1 for exactly 4, 0xd, 0x15 and 0x17. The enemies the app keeps in kind-2 groups are the Heartless and
  Nobodies of type 4 and `M_EX940` of type 0x15 (the two enemy classes `FUN_001994c8` counts), both in that set, so the app's
  table needs no extra filter; types 0xd and 0x17 are not made by any group of the Twilight Town data [inferred from the
  objentry rows the groups name; no group record names a type 0xd or 0x17 entity].
  `FUN_00199e10` (the table's slot clear: it copies word 3 over word 2 of each of the `DAT_0035e468` records at 0x1c8afa0, stride
  0x38, then runs `FUN_00194300` on 0x35d278) is reached only through `FUN_001727d0`, a one-line wrapper. Neither has a `jal`
  caller (`mips_scan.py --jal 0x1727d0` and `--jal 0x199e10`: 0 hits); `FUN_001727d0` is an entry of a table of (function, flag)
  word pairs at 0x35bf00 (a callback table; its user was not found), so the clear is a hook the original registers, not
  something the area start calls. Not ported [GAP: nothing in the app corresponds to the 0x1c8afa0 records].
- **No refill timer.** Nothing in this path re-creates a group after its enemies are defeated: the app's former
  "a wave 4 s after the last enemy" in tt04 was the app's own and is gone there.
- **Unload.** The jump task's unload `FUN_00104ce0` calls `FUN_00168ca0` -> `FUN_001b8800` (frees every object of
  the list at 0x1d5d7e0) and `FUN_00168ce8` -> `FUN_00181af8` (frees the loaded groups) [inferred: these are the
  area's actors, among them the PRIZE manager with its orbs and the item objects (`prize::unload_prizes`); the objects' own
  teardown was not read].

tt04's groups (test `tt04_btl_programs_choose_their_groups`, disc): there is no `btl` program 0, so a new game (all
program bytes 0) has **no enemies in tt04**; program 1 (flag 0x883) loads b_00: a kind-2 group (0xe1, a box volume
over the Sandlot) with three spawn-kind-2 Dusks, none at load; 0x7 loads b_21 (kind 2: the Dancer and two Dusks, all
spawn kind 2); 0x50 loads b_44: kind-1 group 0x89 (the player and friends) and kind-2 group 0x8a (volume shape 2,
always inside) with three spawn-kind-3 Dusks (at load), one spawn-kind-2 Dusk of wave 0 and three of wave 1
(placement 1), plus four `M_EX940`; the Struggle groups b_41, b_42, b_45, b_50, b_52 (kind 1) have their opponent
at spawn kind 3 (b_51: 0), so it appears at load with the program's mission (opcode 0x15). All tt04 group headers
have +0x1c, +0x20 and +0x22..+0x24 = 0, and every member's delay is 0.

**The app (`enemy::spawn_waves`)**. When the area's scripts run (`area::run_area_script`: every area load, and a
type-2 jump into the loaded area), the groups the `btl` program loaded (`enemy::btl_groups`) are set up:
`created_at_load` members appear at once (on the floor under the record's point, with the record's facing: game
`(x, y, z)` is the app's `(x, -y, -z)`, game yaw `r` is the app's `PI - r`), and each kind-2 group keeps an
`enemy::KindTwo` (the walk above; its timer in 60 Hz frames [inferred: the frame step `DAT_00356f40`]; `killed`
for the table). An enemy counts as removed when its HP reaches 0 [inferred: the game removes the object after its
death motion]. The program's mission (opcode 0x15) starts with its group. F3 sets the groups up again. The jump's
unload (`area_jump::unload_area_actors`) despawns every `area_jump::AreaActor` (the enemies) when the task reaches
`Loading`, and the groups are set up again by the next run of the scripts, so the Struggle's Retry needs no
special clearing.

- `KH2_WAVE_GROUP=<tag>` (harness) sets up that group (from the first program that loads it) instead of the
  program's; when the group holds the player's entity (0x236) Sora is put there first (b_44: the centre of the
  Sandlot). With `KH2_WAVE_AT_GROUP=1` a forced group without a player entry (b_00, b_21) puts Sora at the centre
  of its members instead (a test aid, WP-7D; not the game's rule). b_00 and b_21 hold only spawn-kind-2 members,
  so their enemies appear once Sora is inside the group's box.
- Areas whose `btl` script loads no group with enemies the app has (tt00, tt02), and `KH2_AREA_WAVES=0`: the app's
  own waves of Shadows around Sora with a Soldier from the second wave on, one 4 s after the last enemy is gone or
  on F3. These are not from the disc (kept, `local/DEVIATIONS.md`).

Logs: `area tt04: btl program 0x50 group b_44 (KH2_WAVE_GROUP): 3 enemies at load (FUN_00198750), 4 on demand by
kind-2 group [8a]`, `area group b_44 kind-2 group 0x8a: Dusk (serial 0x24, wave 1) created; ...`, `... gone; timer
0 frames, defeated for good`, `jump: area unload (FUN_00104ce0): 1 area actors removed before tt04`, and without a
program that has enemies `area tt04: btl program 0x0 loads no group with enemies of the app (FUN_00181148); no
enemies`.

### Placements 1 and 2 of a kind-2 member (WP-EN2)

`FUN_001991a0` creates a member by the record byte +0x1d: 0 at the record's point (`FUN_001982c8`), 1 near the
player (`FUN_00199970`), 2 within a radius of the point (`FUN_00199a80`, radius = the record's halfword +0x2e,
centimetres) [V]. Both are a ring search on the collision, chosen by the object's `00objentry` flags (+0x48) bit 3:
clear searches the floor (`FUN_001a5ea8` -> `FUN_001a6198`), set searches in the air (`FUN_001a5ee0` ->
`FUN_001a63c8`) [V]. Their floating-point arguments (the decompiler drops them) were read from the instructions:

| placement | centre | radii (cm) | air heights (cm) |
| --- | --- | --- | --- |
| 1 (`FUN_00199970`) | the player's position (`FUN_0016b960` of `DAT_0035c998` [inferred: the object's position]) with y -200 (raised 2 m) | 150 .. 450 (`DAT_0037b2f8`, `0x37b2fc`) | -100 .. 100 (`0x37b300`, `0x37b304`) |
| 2 (`FUN_00199a80`) | the record's point | 0 .. the record's radius | 0 .. 0 |

The search (`FUN_001a6068` sets it up, `FUN_001a6198` / `FUN_001a63c8` run it) [V]:

1. Draw a radius `r` = min + u1 (max - min) and an octant: `(c & 4) >> 2 | c << 1` for the counter `c` = `DAT_0035e9b0`
   (which then counts 0..7 round) times 45 degrees (`DAT_0037b5a8`) plus u2 times 45 degrees. `u` is the generator
   `DAT_003716a0` (`seed * 0x10dcd`, unsigned, times 2^-32). The try `n` (1, 2, ..) is at the octant angle plus `n` steps of
   45 degrees (`DAT_0037b5b4`), the point at `r` round the centre (x = cos, game z = sin).
2. On the floor: the point's floor is the first hit of the line from 200 cm above the point to 1000 cm below it
   (`FUN_001a5c30(1000, 200, ..)`, `0x37b5bc` / `0x37b5c0`, the collision ray `FUN_001d1648`); it is taken when the line
   from the centre to a spot 20 cm further out (`0x37b5b8`) and 100 cm above that floor (`0x37b5b0`) is free
   (`FUN_001a5bf8`). In the air: a random height of the range is added to the point (a generator step per try) and the
   same line test, to 100 cm above the point, decides.
3. After 8 tries the radius shrinks by 100 cm (`DAT_0037b5b0`) until it is below the minimum. Nothing found: the floor
   under the centre (the same ray) or, in the air, the centre.

The app (`enemy::ring_place`, `member_ring`, `ring_place_in`; tests `ring_search_*`, `member_rings_follow_the_placement_byte`,
`place_rng_follows_the_games_generator`): the same loop in metres with its own generator (the game's one is shared with
every other caller, and the executable holds 0 as its seed, so the start value is the app's [inferred: nothing else
depends on the sequence]); the floor ray is `World::ground_m` from 2 m above the point (at most 12 m below) and the line test is
`World::blocked_m`, which meets walls only [inferred approximation: `FUN_001d1648` meets floors too]. tt04's b_44 has three
Dusks of wave 1 with placement 1 (serials 0x24, 0x25, 0x101; test `tt04_btl_programs_choose_their_groups`); no tt04 member
has placement 2. Run `KH2_WAVE_GROUP=b_44` (acceptance below): `kind-2 group 0x8a: Dusk (serial 0x24, wave 1) created near
the player (FUN_00199970) at Vec3(-2.98, -0.10, -6.12)`, and the same for 0x25 and 0x101 at other points round Sora.

### Group events 2 and 3 and kind-6 groups (WP-EN2)

- **The counter.** The group object's +0x18 counts the group's enemy-class entities (object types 4 and 0x15,
  `FUN_00198150`): `FUN_00198750` counts the spawn-kind-2 ones (`FUN_001994c8`, once per entity not in the table at
  0x35d278), and `FUN_00198920` the ones of kinds 0, 3 and 1 (current entrance) that it creates, in a group that is not
  disabled [V]. `counted_at_load` is the port, except the two filters of `FUN_00198750` it leaves out: the `DAT_0035e3a0` mode
  (`FUN_001981a0` fails when `FUN_0016ece0(type) >= 2000`) and the type-0x10 test (`FUN_002d6d28 == 0`); neither occurs in the
  Twilight Town data and no enemy-class entity is type 0x10; b_44 counts 3 + 8 = 11.
- **Removal.** `FUN_001993f8` (an object of the group is removed) calls `FUN_00199518` only for spawn kinds 0, 1 and 3 whose
  +0x588 bit 0x800 is set; the enemy classes' constructor `FUN_001b0270` (also the type-0x15 one, `FUN_001c1ec8`) sets that
  bit, so it holds for every enemy [V]. A spawn-kind-2 member's removal never lowers the counter [V: the compare with 2 skips it],
  so a group whose enemies are all kind 2 (tt04's b_00, b_21, b_44) never empties.
- **The events.** `FUN_00199518`: counter - 1; at 0 `FUN_00197b70(3, header +2)` (group emptied); then, when the counters of
  all live groups (`FUN_00199580`: the 0x38-byte records from 0x1c8af90) sum to 0, `FUN_00197b70(2, 0)` (all emptied) [V].
  `FUN_00197b70` broadcasts to every object (their method +0x34) and matches the area triggers (docs/systems/core-runtime/areas.md "Firing").
  The app (`removal_events`, test `emptied_groups_raise_events_3_and_2`) writes `struggle::GlobalEvent` (enemy scripts get
  it as event 10) and logs a matching trigger of the btl program: `area group b_42 group record 0x5d: member serial 0x14
  removed: global event 3 (93) (FUN_00199518)`. Removal is "its HP reached 0" [inferred, as for the kind-2 walk]. The
  entities the app cannot create still count in the game's counter and never leave it here, so such a group does not fire.
  What a matching trigger does (the story flag, the event, the jump) is not applied: no Twilight Town script has a trigger
  on event 2 or 3 (test `no_twilight_town_data_uses_group_events_or_kind_six_enemies`, tt00..tt41), so nothing in the data
  needs it (a gap for other worlds).
- **Kind-6 groups** (`FUN_00198b30` case 6 -> `FUN_00199e98`) [V]. `FUN_001989d8` visits a kind-6 group's volumes (only those
  whose activator halfword +0x2c is 1) and, while the player is inside one, creates every absent spawn-kind-2 entity at its record's
  point (`FUN_001982c8`: no timer, wave, table of the defeated or placement byte), and starts a 0 -> value ramp on the new
  object (`FUN_001abf40` on +0x82c: an appear fade, not ported). Port: `kind_six_creates` (test
  `a_kind_six_group_creates_its_absent_members_at_once`); the system calls it every tick. [inferred: an enemy counts as present until it is removed, so a corpse that is
  still lying there blocks the next creation; the game's check is the object table, `FUN_0016b0d8`.] The 231 kind-6
  groups of tt00..tt41 hold no enemy-class entity (same test), so the port has no user in the three areas the app loads.
  Kind 3, 4, 7, 8, 9 and 10 groups are not ported (kinds 3 and 4 hold no enemies in the data; 7 and 8 are other
  object kinds).

**Gaps.** Entities the app cannot create (other than those it has) are left out of the walk and of the `SpawnLimiter` sum [inferred
stand-in: counting them would hold a wave back for ever]; since WP-EN2 `M_EX940` is no longer one of them (below). The effect of a trigger that matches group event 2 or 3 is not applied (no data needs it). Prize orbs and items are
removed by the jump's unload since WP-EN2 (`docs/systems/combat-effects/prizes.md` "Unload"). The Dusk's drop row reads story flags 2267, 2256,
2122, 2091 through host 0.15, which answers from the story (all clear without a state file). Whether the game moves the
player to a battle group's player entry when the battle starts was not read (only the harness does it).

### Group gaps closed (WP-EN5)

- **`FUN_00190668`** [V] returns 1 when `DAT_0035d268 & 0x100`, or `& 2`, or `DAT_0035e3a4 != 0` (`FUN_00196418`: an event plays), or the
  word at 0x35fec8 is non-zero (`FUN_001c6c08(0x35fec8, 0)`: the latch of host 4.54), else 0. Readers: `FUN_001989d8` (the group walk
  of every kind except 4: when it holds, the group timer does not run down and nothing is created) and `FUN_00189ae8` (a native NPC's
  five-frame counter at +0xb0c, below). `FUN_00199e98` (kind 6) does not test it. Ported in `enemy::spawn_waves`: the kind-2 timer and
  creation stand while a mission event plays (`Mission::event_active`) or the fight is stopped (`Mission::fight_stopped`, bit 2 of
  `DAT_0035d268`, `FUN_001b9940`). Not modelled: the latch (private to `struggle::Active`; the stop sets it too in the missions of the
  app) and bit 0x100, for which no writer was found in the decompiled functions (a text search for `DAT_0035d268 |` finds bits 2, 4, 0x10,
  0x20, 0x10000 and 0x100000). WP-EN7: the instruction scan (`--addr 0x35d268`, 106 hits) lists only decompiled functions, and the
  writers found set bits 2, 4, 8, 0x10, 0x20, 0x80, 0x200, 0x400, 0x2000, 0x8000, 0x10000, 0x20000, 0x40000 and 0x100000; none sets
  0x100 (`FUN_001b8d80` tests bit 0x100 of a halfword of its input, not of the flag word). No writer exists in the code [V], so the bit
  stays clear and nothing is left to port. The kind-6 guard and group events 2 and 3 are unchanged from above (not applicable to the data).
- **Group events 2 and 3** [V that the senders are `FUN_00199518` only]: they go through `FUN_00197b70` like any event and run a matching
  trigger (`FUN_00197670`: set a flag, play an event, jump; docs/systems/core-runtime/areas.md "Firing"). The app loads only Twilight Town, and none of
  its 42 area scripts has a trigger on event 2 or 3 (`no_twilight_town_data_uses_group_events_or_kind_six_enemies`, 0 triggers), so there
  is nothing to execute: not applicable. The log line "area trigger ... matches" stays for other data.
- **The defeated table's lifetime** [V]: `FUN_00181148` calls `FUN_00194188(0x35d278, DAT_0035cd28)` at every area start (the area
  byte 0x33e831 is the key; `DAT_0035cd28` is 3, set by the area script opcode 6, and 0 when the map program byte is above 0x32).
  Four slots with ages and a counter: an area started again within `keep` area starts finds its slot with its entries; any other start
  clears the least recent slot. `FUN_00194330` (from `FUN_00168960`, an object-system reset) clears all slots and the counter. Ported
  as `enemy::DefeatedTables` (`BattleGroups::defeated`): a kind-2 group's `killed` list is seeded from the slot at set-up and written
  back each tick; F3 (harness) clears the tables. The second table at 0x35daa0 (`DAT_0035cd2c`, default 4) is not used by enemies.
  WP-EN8: the keep count is now read: `ScriptRun::keep` holds the opcode 6 argument (`FUN_00180b48` case 6 writes `DAT_0035cd28`; `FUN_00181148` presets 3 at its start and forces 0 when the map program byte is above 0x32, then calls `FUN_00194188`); `kh2_ard::run::keep_for_program` applies that rule and `enemy.rs` passes it to `DefeatedTables::enter` [V]. On the disc (test `twilight_town_area_scripts_keep_values`, 42 area files) only three programs write opcode 6, all with 0: tt07 btl 21 and 22, tt33 btl 1; every other Twilight Town program leaves 3. So those two areas keep nothing when those battle programs run (their defeated members come back at every visit) and the rest keep three area starts. Opcode 7 (`DAT_0035cd2c`) is not used by enemies.
  Gaps: the callers of
  `FUN_00168960` were traced in WP-EN7 (below); a serial of 0 is also the empty entry in the original and is not special-cased.
  WP-EN7: `FUN_00194330` has these callers [V, instruction scan `--jal 0x194330`, `0x194318`, `0x168960`]: `FUN_00168960` (through
  `FUN_00194318`; its one caller `FUN_001689c0` runs it when the world byte `0x33e830` differs from `DAT_00522178`, before the area is
  built) and three area-retry handlers `FUN_002d8b98`, `FUN_002df3d8`, `FUN_002e3c78` (they save the area key, clear, re-enter the same
  area). Ported: `DefeatedTables::enter_world` clears on a change of the world prefix (test
  `a_change_of_world_clears_the_tables_of_the_defeated`); the retry handlers have no counterpart (the app has no retry).
- **Kind-6 appear ramp guard** [V]: bit 0x10 of object +0x7f8 is set only right after a ramp is started on +0x82c, by `FUN_00170f78`,
  `FUN_001b3f78`, `FUN_001c7600` and `FUN_00189ae8` (other classes: each starts its own ramp, then sets the bit); the object
  constructor `FUN_00168d30` zeroes the word, and no decompiled function clears bit 0x10 alone. So a new enemy of kind 6 always has it
  clear and the ramp always starts, which is what `Appearing` does. Not applicable beyond that.
- **M_EX850 `idle` and `set_move(.., 15)`** [V]: the `idle` mode (routine at 0xb0d) only counts a timer (+0x70) down with host 0.3 and
  switches to `leave`; it does not move the object. The `set_move(.., 15)` is host 1.111 at 0x16f inside routine 0x121, which routine 0x118
  wraps and `R0060` (reached from the event-11 handler for reaction command 481, mode `exec_rc`) starts with host 1.112 `start_action`
  (code address 280) after 1.18 `link_start`. So it runs only for that reaction command, not in `idle`; the app has no object links, so
  that path is not entered. On screen the two are different objects: M_EX940 stays and fights, M_EX850 is the companion made by 1.0
  (above); the app's `KH2_WAVE_GROUP=b_44 KH2_BDX_HIT=M_EX940:7:25` run is the check, see the acceptance below.

### M_EX940 and the object type 0x15 (WP-EN2)

`FUN_0016e8c0` makes the object of an entity by the objentry `ObjectType`: type 4 `FUN_001b02b0` (vtable 0x35eea8) and type
0x15 `FUN_001c1ec8` (size 0xb00 like type 4, which it calls `FUN_001b0270` for, then vtable 0x35f580 and +0xabc bit 0x80) [V].
The two vtables are equal up to +0x78 (the same object methods) and differ in the second table (+0x84..+0x138: the link
interface, `FUN_001c26c0`, `FUN_001c2b70`, `FUN_001c2650`, `FUN_00169ee8` ..). So an object of type 0x15 is a scripted enemy
like the others; the objentry rows with type 0x15 are the large ones (weight 500: `M_EX570`, `M_EX640`, `M_EX670`, `M_EX680`,
`M_EX940`). It can be spawned with the existing machinery: `enemy::M_EX940` (object 0x139, model `M_EX940`; the name is the model's,
the data gives none). Its script (5159 words; modes `appear`, `btl_attack`, `atk_brush`, `atk_jump`, `atk_throw`,
`atk_fullpower`, `mode_revenge`, `leave`, `dead`) hands `enmp` row 49 to `set_obj_param` (constant pushed at 0x06e2 for `R124c`): 50 HP at world level 1;
`dead` (R08db) is the Dusk's (element 2 of its effect file, flags 3 and 6, fade, hide). New host calls: 1.97 `obj_part_110`
(the address of object +0x110, the init stores a float at its +0xc), 1.53 `bits_480_set` (+ 1.54 now clears in the same memory:
no reader of either bit array was traced), 1.108 / 1.109 `link_method_3c/40` (stubs: no link record in the app).

Run `KH2_BDXLOG=1 KH2_WAVE_GROUP=b_44 ... kh2-game tt04` (acceptance below): four M_EX940 appear (wave 1 and wave 2 of
group 0x8a) and enter `appear`, `mode_battle`, `btl_long`, `btl_short`, `atk_jump`, `atk_brush`, `atk_fullpower`,
`atk_throw`, `back_target`, `mode_revenge`, `rvg_short`, `idle`, `leave`, `dead`; the only unimplemented host call is 1.0.
**Blow-class hits (WP-EN3).** A blow-class hit (hit reaction 7, 9 or 11, R016a at 0x01a7) makes the script call `R0082`, which
creates a second object with host 1.0 `create_object` (0x170190 -> `FUN_0016e8c0(DAT_0037a484, id, 0x1c883a0)`)
for the objentry id 0x6bf = the `SpawnObject1` of M_EX940 = `M_EX850` (type 4, 3101 words, a `00objentry` row with NeoStatus 0,
Weight 0, flags 16) [V]. M_EX940 does **not** transform and does not hide itself: R0082 stores the handle at global +0x1084
(R011d), calls 1.98 `ground_position(self)` and 1.148 `set_position(created, point)`, then R0126 (effect 10 on itself, 1.109,
1.53 bits 7 and 5, global +0x1070 = 1). M_EX850 is a companion that appears at M_EX940's ground point and M_EX940 stays and keeps
fighting. A later R0082 first hides the previous companion with 1.28 and 1.70 flag 6 when host 1.94 calls it valid and it is not in
motion 241-243; R033b undoes it (1.108, bits cleared, global +0x1070 = 0). `FUN_0016e8c0` is given the place record 0x1c883a0, which
is zero: the object starts at the origin and facing 0, and nothing hands over HP, facing or serial; the script moves it [V].
M_EX850 has no `set_obj_param` (2.9), so no `enmp` row: the app gives it a stand-in status block ([inferred], HP 50 like M_EX940 in the
run). Its script has the modes `idle`, `appear` and `leave` among others; the string "burserk start" is only a debug print in its
event-11 handler for reaction command 481 (docs/systems/drive-limits-summons/contract.md). In the app (`KH2_WAVE_GROUP=b_44 KH2_BDX_HIT=M_EX940:7:25`)
M_EX850 is made a tick after the call at M_EX940's ground point, runs `idle` and its `leave` (about 3 s later), with no `missing`
host. What it does in the original beyond that (its attacks and what the `appear` mode is entered from) was not traced.

Ported (`enemy_obj.rs`, `enemy_host.rs`, `enemy.rs::spawn_created`, `enemy_ai.rs::ai_remote`): 1.0 for the objentry ids of the
app's enemy kinds (`ObjTable::kinds`), 1.148, and 1.28 / 1.70 / 1.71 / 1.15 / 1.94 / 1.98 on another object of the app, applied at the
end of the tick (calls on an object not yet built wait up to 3 ticks). 1.8 on an object an enemy made (WP-EN4): `Obj::ask_mode`
queues `Remote::Mode`, `ai_remote` hands it to the object's `Obj::modes_asked` and its script takes it at the start of its next tick
(the game switches it at once, FUN_00186db0); only unit-tested (`a_mode_asked_of_a_made_object_is_queued_for_it`), no enemy of the app
was seen making the call. Not ported: objects of kinds the app does not have, the host 6.58 queue for another object.

**Overlapping bodies.** `enemy::separate` pushed two bodies on one point both along +X, so they drifted together for ever
(M_EX850 made on M_EX940's point left the arena at 44 cm per tick and fell: the "moves 44 cm per tick against 15 in the script" of
the first WP-EN3 run was this, not `set_move`: the script's `set_move(.., 15)` is in a loop of its own mode and the mover's delta was 0 in every traced tick). The pair now takes opposite directions by index (`pair_dir`, a test).

**Kind-6 appear fade.** FUN_00199e98 calls `FUN_001abf40(0, DAT_0037b308, DAT_0037b30c, object + 0x82c)` on the object it makes and
`FUN_001abf78` steps the ramp each frame up to `DAT_0037b76c` (alpha = +0x82c * +0x834, `FUN_00193270`); `enemy_obj::Ramp` and
`AppearRamp` (read from the executable) do this for kind-6 group members. No tt04 group is of kind 6: tested by unit tests only.

### Acceptance runs (WP-EN2)

- `powershell -NoProfile -ExecutionPolicy Bypass -File localccept.ps1 -SkipAudio -Only tt04-dusk,tt04-seifer,combat-ground,reaction-34-forced -Out localccept\en2`:
  tt04-seifer, combat-ground and reaction-34-forced PASS; tt04-dusk WARN `soft-missing: death effect` (a soft line; the base
  checkout's exe gives the same WARN for that scenario, `localccept\en2-base`), its hard `Dusk` expectation met; the screenshot
  shows a Dusk in front of Sora in the Sandlot with its HP bar and the HUD.
- Placements: the b_44 run above (`KH2_BDXLOG=1 KH2_WAVE_GROUP=b_44 KH2_NO_SOUND=1 KH2_NO_MUSIC=1`, Sora attacking from 6.3 s): wave 1's
  Dusks (serials 0x24, 0x25, 0x101) are `created near the player (FUN_00199970)` at three different points, 3.5 to 9 m from Sora,
  the wave 0 member and the M_EX940 at the record's point (placement byte 0).
- Group events: `KH2_BDXLOG=1 KH2_WAVE_GROUP=b_42 ...` (Seifer defeated at about 21 s): `area group b_42 group record 0x5d (kind 1): 1
  enemies counted (FUN_001994c8)`, then `member serial 0x14 removed: global event 3 (93) (FUN_00199518)` and `... global event 2
  (0) (FUN_00199518)`; the Seifer's script runs its event-10 handler for both (`global event 3 (93); handler ran`).

## Acceptance run (6C)

`KH2_BDXLOG=1 KH2_FXLOG=1 KH2_WAVE_GROUP=b_44 KH2_KEYS="6:Q,<J every 0.45 s from 6.3 to 44 s, Q every ~7 s>"
KH2_SCREENSHOT=... KH2_SHOT_AT=45 kh2-game tt04`: seven Dusks appear around Sora. Modes entered: `appear`,
`mode_battle`, `btl_attack`, `btl_short`, `btl_long`, `idle_time`, `footwork`, `move_target`, `move_step`,
`move_front`, `move_down`, `escape`, `jump`, `atk_craw`, `atk_kick`, `atk_walk`, `mode_revenge`, `rvg_short`,
`rvg_long`, `leave`, `dead`. No script error. Four Dusks died with `death effect: element id 2 of
Some("obj/M_EX990.apdx")`, two left (mode `leave`, about 15 m from Sora), one still fought at the end; the only missing host function reached was 1.92 (the drop, as for the
Heartless). Screenshots were looked at: Dusks around Sora in the Sandlot, posed mid-motion (not a rest pose), one
locked on with its HP bar. Nobody listened to the sounds.

## Follow-ups (6G)

The Dusk's remaining host calls are wired (details in docs/systems/script-host/contract.md): its reaction commands 24 and 69 send
`dusk_surprise` (host 1.90) and start a request thread that plays its paired motion 241 / 242 on Sora (now read: the
paired-motion and collision-table lists iterate `enemy::KINDS`), which also reaches 0.42, 0.43 and 1.94; its death's
drop (1.92) is computed from `przt` (row 6: four prizes of kind 5, no item in the runs) and logged. Runs (`KH2_BDXLOG=1
KH2_HITLOG=1 KH2_REACTIONLOG=1 KH2_WAVE_GROUP=b_44 ... kh2-game tt04`, Sora attacking from 4.3 s): with
`KH2_REACTION_FAKE=24 KH2_REACTION_FAKE_ON=Dusk KH2_REACTION_PRESS=10..40`, 31 accepts, every Dusk got the message and
the near ones entered `dusk_surprise`; with `KH2_CLASH_FORCE_ENEMY=8`, four Dusks died with a drop line each and the
forced clash reached event 19 (handler absent). No `unimplemented host call` and no script error in either run. Only
logs were read: no screenshot was looked at for these runs.

## The Struggle (WP-7E)

The Struggle rules are in `docs/systems/world-minigames/struggle.md`. For the opponents: b_50, b_51 and b_52 (missions `TT04_MS00`,
`TT04_MS02`, `TT04_MS01`) are the Struggle matches; b_42 (`TT04_MS108`) is not. Host 4.29 now answers the
manager's counters (slot 0 the player's orbs, slot 1 the opponent's); every reaction an opponent takes knocks 3 or
15 orbs out of slot 1 (`FUN_001caeb0`); the opponents pick orbs up for slot 1; their prize drops make nothing
under the mission's header flag 0x40; global events reach their scripts as event 10, so the result 159 / 160 picks
`gameset_voice_lose` / `gameset_voice_win`.

WP-7E2: `player_finish_struggle` is a mode that the Struggle-tournament opponents `N_EX650_BTL10`, `N_EX670_BTL10`,
`N_EX680_BTL10` and `N_EX690_BTL10` register (host 1.6, next to `enemy_finish_struggle`); none of them is in tt04's
Struggle groups, so in tt04 the message finds no receiver (docs/systems/world-minigames/struggle.md, "player_finish_struggle"). Host 1.34
`event_active` now answers true while a mission layout event plays (the start, end and failure titles). An
opponent standing in a reaction motion (10..39) or in 140..150 does not pick orbs up (`FUN_001cadd8`).

### M_EX760 (WP-EN9b)

Object 0x131 = 305 (`00objentry` row: `ObjectType` 4, `NeoStatus` 1028, `NeoMoveset` 0, Weight 10, `SpawnLimiter` 7, flags 0, `ObjectTargetType` 1); its
BDX script is `m_ex760` (7982 words; modes `appear`, `btl_attack`, `atk_turn`, `atk_jump`, `atk_down`, `footwork`, `move_roll`, `move_back`,
`move_target`, `mode_revenge`, `leave`, `dead`). The data gives no name; the model is a grey armoured knight with a spear
(harness screenshot of bb08, idle: `M_EX760` standing in front of Sora); "Armored Knight" would be the community name [inferred]. Other rows
with the same model family: object 1845 `M_EX760_NM` (the same `.mset` and `NeoStatus`), `M_EX760_RAW` (1468, flags 4, `SpawnLimiter` 3, a
child of the `M_EX760_CROWD` group object 1366): not wired (their areas were not run).

`enmp` row 36: the init calls R1cc5 at 0x0c02 with 36 as the eighth word it pushes, R1cc5 hands it to host 2.9 at 0x1cec
(`set_obj_param(self, 36, 0)`); the run logs `SetObjParam { a: 36 }`. HP 10 at Sora level 1 in bb08, lvpm 4 x `enmp` Experience 20 = 1 point at level 1
("experience: enmp 36 defeated: +1"). Its `dead` mode (R0dbf at 0x07b2) is the Dusk's: 1.92 (the drop), 1.87 (element 2 of its effect file),
flags 3 and 6, fade and hide; `Kind::death_effect` is true.

New host call 2.21 `shared_block` (BDX R1ddd at 0x1ddd: key 12, size 20; the script reads the first word of the buffer and sets the block up
once): `enemy_host::shared_blocks` is one `kh2_bdx::shared::SharedBlocks` for all enemy scripts of the battle (the game's list at 0x1c95528),
emptied when the area's groups are set up (`enemy::spawn_waves`, FUN_001a5030 from FUN_00168ba8); `read_mem` / `write_mem` answer the buffer's
addresses. Before it, the call was `missing` and R1ddd's read of the answer died with `BadAddress(0)` at 0x1deb, then `BadAddress(16)` at 0x1209 and 0x1151
(the same pointer read later). What the block is for (the strings "attacker add" / "attacker sub" in the script suggest a count of attackers
shared among the knights) was not traced [inferred].

Run (bb08, group b_21 is kind 2, created while Sora stands in one of its volumes; the harness put him in the first volume of the group with a
temporary hunk, not committed): `KH2_BDXLOG=1 KH2_AREALOG=1 KH2_WAVE_GROUP=b_21` with `kh2-game bb08`: two knights appear (t2 `mode appear`), t92
`mode_battle` -> `move_roll`, t182 `atk_down`, later `atk_jump`, `rvg_short`, `atk_turn`, `move_back`, `jump`; no `unimplemented host call` and no
`script error`. Hit with `KH2_KEYS` J presses: `M_EX760 hit: atkp 127 damage 2 hp 8 reaction 3 ... FlinchPush`, a Blow (reaction 7) at hp 0, then
`hp 0: switching to mode dead`, `drop (host 1.92) asked row 4 ... 1 prize objects` and `experience: enmp 36 defeated: +1`.
Screenshots looked at (scratchpad, not committed): idle (two knights near Sora, labelled `M_EX760 Idle hp 10/10`), hit (damage numbers 3 and 2,
hit stars, label `Hurt hp 8/10`), after the death (the first knight gone, a pink HP orb, a green orb and a "1p" strip fading where it stood).

Gaps: 1.206 `GlobalFlag80` and 1.171 `GlobalFlag4` stay stubs (other scripts' state), 1.121 `SetTargetMode` is a stub (nothing in the app consumes the lock mode),
1.80 `TurnToward` uses the placeholder turn rate; the objects 1845 / 1468 above are not spawned; the attackers' count behind 2.21 is not modelled.

## Visual gaps (WP-VIS1, 2026-10-21)

Audit of every kind of `enemy::KINDS` (48) plus Sora's forms, Donald, Goofy, Mickey (`P_EX200`), Chicken Little (`P_EX350`) and the keyblade, 58 model files. Method: the developer asset viewer (`KH2_ASSETS=1 KH2_ASSETS_KIND=model KH2_ASSETS_START=obj/<name>.mdlx`, which draws through the game's own `skel::spawn_skinned`), one screenshot from the front and one from behind per model, each looked at; the in-game accept screenshots of 30 kinds for the colours seen in a fight; the decoded textures of every model (`kh2 model obj/<name>.mdlx --textures`) checked for flat or empty images. Nothing was copied into the repo; the contact sheets stayed in the scratch folder.

Result: no model with a missing, flat, wrongly coloured or stretched texture was found after WP-MD1's fix. Every drawn texture has at least 24 distinct colours and at least about half of its pixels off the dominant colour (`crates/kh2-model/tests/enemy_models.rs` asserts this and that every mesh names an existing texture and valid indices, for all 58 files). Looked at and fine: Shadow, Shadow RAW (yellow eyes visible), Soldier, Dusk, Dancer, Seifer (N_EX570_BTL), N_EX500_BTL, N_EX600_BTL, M_EX050, M_EX060, M_EX110, M_EX120, M_EX130, M_EX210, M_EX420, M_EX500, M_EX510, M_EX520, M_EX560, M_EX570, M_EX580, M_EX600, M_EX620, M_EX630, M_EX650, M_EX680, M_EX690, M_EX700, M_EX710, M_EX730, M_EX740, M_EX750, M_EX760, M_EX850, M_EX880, M_EX890, M_EX900, M_EX910, M_EX920, M_EX940, M_EX950, M_EX960, M_AL020_FIRE, M_AL020_ICEE, all Sora forms, Donald, Goofy, Mickey, Chicken Little, the keyblade.

Not defects (looked like one, checked):
- `M_EX870` draws as a flat light-grey triangle: the file has 4 triangles and no texture (`m_ex870_is_the_known_untextured_placeholder`); it is the script half of the eh17 fighter pair and the script hides it (see "The enemies M_EX870 and M_EX960"), the in-game eh17 screenshot shows no such triangle.
- `P_EX100_WI` is grey-scale: it is the Timeless River (Steamboat Willie) Sora; its texture has 29 colours, all grey. `P_EX100_NM` is the dark Halloween Town outfit [inferred from the file suffix and the look].
- Shadow, Shadow RAW and `M_EX420` are very dark: their textures are dark (mean grey 61 for Shadow, 12-36 for `M_EX420`) and the in-game shots show the same dark bodies; the yellow eyes show.
- The viewer frames the mesh's unposed bounds, so flyers (`M_EX660`, `M_EX780`) and large bosses (`M_EX760`, `M_EX770`) can fall partly or wholly outside its picture; `M_EX660` and `M_EX780` were looked at in the fight screenshots instead (al14, lk03): textured, orange and yellow with a dark body, and a red/orange winged Heartless.

Could not be seen well: the back and underside of `M_EX770` and `M_EX780` (viewer framing); the effect-only meshes (`M_EX680`'s three mostly transparent texture sheets: 62% opaque, 24-37 colours) were seen only as the neon trim on the body. Texture animation (TEXA / UVSC) of enemies was not examined in this audit.

Follow-up (not a model bug): the viewer would show posed flyers and bosses if its framing used the animated pose, or took a zoom and angle from the environment. A local, uncommitted `KH2_AV_ORBIT=<yaw>,<pitch>,<zoom>` switch in `crates/kh2-app/src/asset_viewer.rs` (a 6-line change to `AssetViewerPlugin::build`) made the front/back shots reproducible; it is outside this WP's ownership and was reverted.
