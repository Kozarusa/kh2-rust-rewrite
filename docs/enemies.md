# Enemies: which ones the app has, and why

Work package 6C. The app's enemies are Shadow (`M_EX020`), Soldier (`M_EX010`) and, since 6C, the Dusk (`M_EX990`).
All three run under their own BDX scripts (`docs/enemy-ai.md`) and take hits through the reaction code
(`docs/hit-reaction.md`). Nothing from the disc is stored in the repository: area files, models, motions,
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
| 0x131 | `M_EX760` | 15 areas | bb08 `btl`: b_20 (11 members), b_21 (7); also ca09, ca13, ca19, hb02, hb08 .. (survey, `docs/area-survey.md`) |

tt00 spawns only the player; tt02 spawns the player, the two friends, save and world points, `F_EX000` and event
actors (ObjectType 15). Neither Shadow nor Soldier is in any of the three scripts: the app's Shadow/Soldier waves
are its own (see "Waves"). `M_EX940` (object 0x139, four in b_44) has ObjectType 21 (0x15, the second enemy class; spawned since WP-EN2, "M_EX940 and the object type 0x15").

The most common enemy is the Dusk. Its model, motion set (`obj/M_EX990.mset`, 46 motions), effect file
(`obj/M_EX990.apdx`) and BDX script (BAR type 3 of the model file, 6378 words) are on the disc; the test asserts that.

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

New host functions the Dusk needs are in the table of `docs/enemy-ai.md` (0.15, 1.62/1.64, 1.63, 1.66, 1.69, 1.74,
1.98, 1.150).

## Waves

**What the game does (WP-EN1)** [V unless marked]. An area's enemies come from its scripts, not from a timer:

- `FUN_00181148` runs the `map`, `btl` and `evt` programs named by the area record (docs/events.md section 1);
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
  `FUN_00197b70` broadcasts to every object (their method +0x34) and matches the area triggers (docs/events.md "Firing").
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
removed by the jump's unload since WP-EN2 (`docs/prizes.md` "Unload"). The Dusk's drop row reads story flags 2267, 2256,
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
  trigger (`FUN_00197670`: set a flag, play an event, jump; docs/events.md "Firing"). The app loads only Twilight Town, and none of
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
event-11 handler for reaction command 481 (docs/limits-reactions.md). In the app (`KH2_WAVE_GROUP=b_44 KH2_BDX_HIT=M_EX940:7:25`)
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

The Dusk's remaining host calls are wired (details in docs/enemy-ai.md): its reaction commands 24 and 69 send
`dusk_surprise` (host 1.90) and start a request thread that plays its paired motion 241 / 242 on Sora (now read: the
paired-motion and collision-table lists iterate `enemy::KINDS`), which also reaches 0.42, 0.43 and 1.94; its death's
drop (1.92) is computed from `przt` (row 6: four prizes of kind 5, no item in the runs) and logged. Runs (`KH2_BDXLOG=1
KH2_HITLOG=1 KH2_REACTIONLOG=1 KH2_WAVE_GROUP=b_44 ... kh2-game tt04`, Sora attacking from 4.3 s): with
`KH2_REACTION_FAKE=24 KH2_REACTION_FAKE_ON=Dusk KH2_REACTION_PRESS=10..40`, 31 accepts, every Dusk got the message and
the near ones entered `dusk_surprise`; with `KH2_CLASH_FORCE_ENEMY=8`, four Dusks died with a drop line each and the
forced clash reached event 19 (handler absent). No `unimplemented host call` and no script error in either run. Only
logs were read: no screenshot was looked at for these runs.

## The second enemy: N_EX570_BTL (WP-6H)

`N_EX570_BTL` (object 0x678, ObjectType 4; Seifer by the community's model names, `enemy::SEIFER`) is the next
most spawned enemy (4 groups). It is a combat opponent, not an NPC: its script registers attack modes
(`atk_swing_down`, `atk_cut_off`, `atk_cut_off_short`, `atk_jump`). Model, motion set (206 slots, 31 with motions),
effect file and BDX script (6619 words) are on the disc (`cargo test -p kh2-ard --test disc second`).

- **Spawn parameters.** FUN_0016b870 copies the record words at +0x20 and +0x24 to object +0x820/+0x824, which
  host 1.39 (FUN_0016b940) reads as spawn parameters 1 and 2. Parameter 1 per group: b_41 1, b_42 2, b_45 4, b_51 5.
  The script branches on it: 1 never attacks (`btl_attack` R13ab: walk, idle, footwork, backstep only), 3 or 4
  take `enmp` row 155 (R1864), others row 135; 5 (b_51) sets the lowest HP to 1 (host 1.231) and sends
  `player_finish_struggle` / goes to `faint` when hit at HP 2 or less (event 6 handler at 0x84).
- **Stats.** Kind row 135 (60 HP at world level 1); host 2.9 rebuilds the status block (FUN_001b2148) when the script
  asks for another row (b_45: row 155, 100 HP).
- **HP record.** The status block holds 32 records (current, maximum, lowest) (FUN_0019f770); every HP change
  goes through FUN_001a0100, which holds the result between lowest and maximum. `Enemy::min_hp` is that lowest.
- **Death.** Its `dead` mode (0x174d) asks for voice line 37, plays motion 16 (or 10), stops its dash and waits for
  ever: no death effect and no fade (`Kind::death_effect` false), so the body stays.
- **Gaps.** The Struggle rules (orbs, timer, the win/lose event 10 with 159/160, `player_finish_struggle`) are not in
  the app; messages (1.90) stay missing. Voice lines (1.262 -> FUN_001970b8 -> FUN_001ae798, lines 17, 26, 37)
  are stubbed: no `voice/us/battle` bank names this speaker and the app has no enemy voice slot. Its motions' voice
  triggers (13) are only played for Sora by `audio.rs`. Script motions 48..50 (R0b83, which also calls host 4.0, an
  empty table entry) are not in the set; that path was not reached.
- **Next enemies.** `M_EX930` (b_21), `N_EX500_BTL` (b_50), `N_EX600_BTL` (b_52): WP-7D, below.

Acceptance runs (WP-6H, `KH2_BDXLOG=1 KH2_WAVE_GROUP=<g>`, Sora idle until 25 s then J every 0.45 s, Q lock):
b_42 modes `start_voice`, `mode_battle`, `check_arg_mode`, `btl_long`, `btl_attack`, `footwork`, `walk_forward`,
`run_forward`, `atk_swing_down`, `atk_cut_off`, `atk_jump`, `backstep`, `mode_revenge`, `dead`; b_45 also `btl_short`,
stats rebuilt to row 155. No script error, no unimplemented host call. Its claw 1042 flinched Sora (push 200).

## The third enemy: M_EX930, the Dancer (WP-7D)

`M_EX930` (object 0x138, ObjectType 4, `enemy::DANCER`) stands with two Dusks in b_21 (btl program 0x7). "Dancer"
is inferred from its mode names (`atk_force_dance`, `atk_spin_u/m/d`, `atk_kick`) and its pink and cream textures
with the Nobody emblem; the code calls it by its model. Model (4 meshes, 60 bones, 2 textures), motion set
(253 slots, 32 with motions, entry 252 a paired player motion), effect file and BDX script (4383 words) are on the
disc (`cargo test -p kh2-bdx --test disc m_ex930`).

- **Stats.** R0f13 hands `enmp` row 48 to `set_obj_param` (the constant pushed at 0x04ce, the call at 0x0f3a): 32 HP
  at world level 1. The same routine stores the attack ranges (400 / 800 cm at context +0x30 / +0x34), the chase
  radii (2000 / 1200 cm, R0f99: world byte 0x33e830 is not 10, spawn kind not 5 or 6), the motion blend (8) and
  the prize row (25, read by `dead`).
- **Appear.** Spawn kind 2 (b_21) or -1 (the app's waves) faces the player and plays motion 44 with a fade-in
  (R058a); other kinds start motion 0 at a random frame (R062d).
- **Attacks.** Spins 206 / 207 / 208 (atkp 438, 439, 440 through range trigger 10), the kick 205 (atkp 441; three in
  a row, and `atk_kick_call` sends `atk_kick` to every object with a delay of 60), the force dance (below).
  Attack shapes and windows come from the motions' triggers like the other enemies'.
- **Force dance.** `atk_force_dance_near` (R0a48) closes in with motion 202 for at most 600 frames; within 100 cm
  it switches to `atk_force_dance` (R0b0a) when the player's 1.101 and 1.225 read clear, else to
  `atk_force_dance_finish` (motion 204). `atk_force_dance` tests 1.225, 1.161 and 1.101 on the player again, then links the player to the Dancer (1.18 `link_start`, joint word 0x4002, flags 1), sets
  his flag bit 1, plays motion 203 on itself and paired entry 252 on him (1.29: sub-motion `0010`, 116 ticks),
  queues his motion 0 and waits while motion 203 plays and the link holds. **Gap:** the player-side link
  (FUN_001beb90: parent at +0x570, mover 3; FUN_00169ee8 breaks it when the parent's +0x160 loses bit 0x8000) is
  not in the app, so Sora plays the paired motion where he stands instead of being carried by the Dancer; 1.161
  answers "not linked", which only ends the script's wait earlier (it then waits for motion 203's end anyway).
- **Reactions** use the default slots (10/12 flinch, 16..19 blow) of its set. **Death.** `dead` (R06bd) drops
  prize row 25 (1.92), starts element 2 of its effect file (`Kind::death_effect`), sets flags 3 and 6, waits half of
  +0x20 (15 of 30 frames), fades out over the other half and hides.
- **Not reached.** Event 13 (`event_begin`: 1.128, motion 4, 1.65) is not delivered by the app; motion 47
  (`footwork`) is not in its set (the request is logged and ignored).

Also fixed for the Dusks of b_00 / b_21 (WP-7D): when range trigger 16 is on (1.150 bit 1) the Dusk reads word 0
of its own target record (0x136d -> R0217); the host now serves that record (the player, index 0 [inferred], 0,
-1, 0), which removes the `BadAddress(0x3000000)` script error those Dusks hit in mode `escape`.

Acceptance run (WP-7D): `KH2_BDXLOG=1 KH2_WAVE_GROUP=b_21 KH2_WAVE_AT_GROUP=1 KH2_NO_SOUND=1 KH2_NO_MUSIC=1
KH2_KEYS="6:Q,<J every 0.45 s from 6.3 to 36 s>" kh2-game tt04`: modes `appear`, `mode_battle`, `btl_attack`,
`btl_short`, `btl_long`, `move_back`, `move_target`, `mode_revenge`, `rvg_short`, `atk_spin_random`, `atk_spin_d`,
`atk_kick_call`, `atk_kick`, `atk_force_dance_near`, `atk_force_dance`, `dead`; no script error, no unimplemented
host call; its spin 440 flinched Sora; the paired motion 252 played on Sora (`player: paired motion started: entry
252 sub-motion 0010 of Dancer`).

## The fourth enemy: N_EX500_BTL (WP-7D)

`N_EX500_BTL` (object 0x4d7, ObjectType 4, `enemy::N_EX500`; the app calls it by its model, its name is not in the
data it reads) is the Struggle opponent of b_50 (btl program 0xb6, with a player entry: Sora is put there). Model
(4 meshes, 3200 triangles, 166 bones, 3 textures), motion set (206 slots, 29 with motions), effect file and BDX
script (6638 words) are on the disc (`cargo test -p kh2-bdx --test disc n_ex500`). Its family is N_EX570_BTL's (the
same mode names: `start_voice`, `check_mode`, `reflect_struggle`, `player_finish_struggle`, `gameset_voice_*`).

- **Stats.** Init hands `enmp` row 150 to `set_obj_param` (0x0736): 200 HP at world level 1. It reads no spawn
  parameter (no host 1.39 call), and sets the lowest HP of part 0 to 1 right after (1.231 at 0x0742): it cannot
  die in a fight. Distance thresholds 300 / 400 cm (R187e, context +0x30 / +0x34).
- **Damage (event 6).** A real hit whose effective reaction (host 2.7, the hit record's byte +0x26) is 4, 7, 9 or 11
  sends `player_finish_struggle` and then goes to `faint` when its HP is 2 or less, else drops prize row 122 (1.92:
  ten prizes of kind 0 and five of kind 1 in the runs: the Struggle orbs [inferred]); other real hits switch to
  `mode_revenge`. The app now hands the effective reaction with every hit (`AiHit::reaction`).
- **Struggle score.** Host 4.29 (FUN_001d6738) reads the score of side 0 or 1 from the manager at DAT_01d5dd20,
  which an event command creates (FUN_00180b48 -> FUN_001b9338); without it the function answers 0, as it does in
  the app (the Struggle rules are WP-7E's). The script compares 70 with side 1's value and picks
  `gameset_voice_win` / `_lose` / `_draw` from the two.
- **Host table.** 4.29 is not in `kh2_bdx::names`; the app now gives every enemy runtime the executable's host table
  (`HostTable::from_exe`, as the disc and oracle tests do), so such calls reach the host as `Call::Unknown` instead
  of stopping the script with `UnknownHost`.
- **Death.** Its `dead` mode (R0900) is the Dusk's (drop, element 2, flags 3 and 6, fade, hide), so
  `Kind::death_effect` is true; with the lowest HP at 1 the app never reaches it.
- **Not reached.** Event 15 (its handler compares host 2.25, the hit's attack id, with 1046..1048) and event 13
  are not delivered by the app; 4.0 / 4.1 (empty table entries) sit on the 1.32 task path, which was not reached.

Acceptance run (WP-7D): `KH2_BDXLOG=1 KH2_WAVE_GROUP=b_50 KH2_NO_SOUND=1 KH2_NO_MUSIC=1 KH2_KEYS="6:Q,<J every
0.45 s from 6.3 to 36 s>" kh2-game tt04`: modes `start_voice`, `mode_battle`, `check_mode`, `run_origin`,
`idle_short`, `idle_fix`, `move_target_near`, `atk_blow_2`, `atk_spin`, `back_step`, `mode_revenge`; no script
error, no unimplemented host call; Sora's hits flinch and blow it (HP 200 down to 176 in the first eight hits), its
blow-class hits send `player_finish_struggle` and drop prize row 122.

## The fifth enemy: N_EX600_BTL (WP-7D)

`N_EX600_BTL` (object 0x82f, ObjectType 4, `enemy::N_EX600`, called by its model) is the Struggle opponent of b_52
(btl program 0xb7, with a player entry). Model (14 meshes, 3135 triangles, 262 bones, 5 textures), motion set
(208 slots, 34 with motions), effect file and BDX script (7087 words) are on the disc (`cargo test -p kh2-bdx --test
disc n_ex600`). Same family as N_EX500_BTL: no spawn parameter, lowest HP 1 (1.231 at 0x0809), 4.29 for the score,
the same event 6 rule (effective reaction 4, 7, 9 or 11: `player_finish_struggle`, then `faint` at HP 2 or less,
else prize row 132), plus two branches of that handler that switch to `atk_air_counter`.

- **Stats.** `enmp` row 158 (0x07fd): 100 HP at world level 1. Distance thresholds 350 / 450 cm (R1a39).
- **Host 1.95** (0x173038 -> FUN_00183470), new: mover 1 continuing the object's last real movement (+0x860, host
  1.62): its horizontal part as the velocity, its vertical speed vy giving the apex time vy / g and the apex height
  (g = DAT_0037a884 = 20/49 per tick squared, the factor 0.5 at DAT_0037a888). Called by the second routine of
  `atk_air_counter` (R13e7), which then plays motion 4 when motion 202 was playing. The app starts its `Motor` jump to
  the same apex (rising) or a fall already that fast (falling) with the horizontal speed kept; approx: the eased
  rise of `Motor` is not the game's parabola. 1.95 is not in `kh2_bdx::names`: it arrives undecoded and is taken as
  the object's own (its only caller passes its own context).
- **Death.** `dead` (R09c8) is the Dusk's: `Kind::death_effect` true, not reached with the lowest HP at 1.

Acceptance run (WP-7D): `KH2_BDXLOG=1 KH2_WAVE_GROUP=b_52 KH2_NO_SOUND=1 KH2_NO_MUSIC=1 KH2_KEYS="6:Q,<J every
0.45 s from 6.3 to 36 s>" kh2-game tt04`: modes `start_voice`, `mode_battle`, `check_mode`, `btl_attack`,
`btl_long`, `footwork`, `footwork_not_voice`, `move_target_near`, `atk_swing_side`, `atk_combo_thrust`, `atk_guard`,
`atk_air_counter`, `land_check`, `back_step`, `mode_revenge`; no script error, no unimplemented host call (1.95
reached twice, `first host call 1.95 ? [Approx]`); its blow 168 knocked Sora down (reaction 4).

Not done for the three (WP-7D): the Struggle rules (WP-7E), enemy voice lines (1.262 stays a stub), event 13 and
event 15. `docs/hit-reaction.md`: all three use the default reaction slots of their sets.

## The sixth enemy: M_EX420 (WP-EN9a)

`M_EX420` (object 120 = 0x78, `00objentry` row 120: ObjectType 4, `NeoStatus` 1003, flags 4, `SpawnLimiter` 10; `enemy::M_EX420`,
called by its model: no name is in the data the app reads) is the enemy kind the area survey found most often missing
(33 areas: al00, al01, bb00, bb06, bb13, ca02, ..). al00's `btl` program 0xa loads b_20 with ten of them (the group also
holds `M_EX060`, still absent; test `al00_btl_program_a_holds_ten_m_ex420`, disc). Its BDX script (6501 words, `cargo run -p
kh2 -- bdx M_EX420`) has the modes `appear_shadow`, `move_shadow_round`, `move_shadow_target`, `atk_claw`, `atk_a_charge`,
`atk_s_out`, `atk_group_call`, `mode_battle`, `mode_revenge`, `mode_battle_boss`.

- **Stats.** The script's set-up routine R174d (0x174d) takes eight arguments from its one call (0x0656) and hands the
  first of them, 9 (pushed at 0x063e), to `set_obj_param` (host 2.9, 0x1774): `enmp` row 9. `KH2_BDXLOG` shows
  `SetObjParam { a: 9 }`; the harness enemy has 20 HP.
- **Death.** The `dead` mode (registered at 0x01e2, routine R082e at 0x082e) hands the object's +0x1c to host 1.92 (the
  app's drop), starts element 2 of its effect file (host 1.87 at 0x0853), releases it (0.79), sets bits 3 and 6, fades out
  and hides: `Kind::death_effect` true.
- **Hosts.** No new host call: every one it calls is in `enemy_host.rs` already; the Stub and Approx ones it reaches
  (stubs 1.105, 1.121, 0.79, 1.171, 1.206; approx 1.11, 1.23, 1.34, 1.56, 1.92) carry their own reasons in the table.

Acceptance run (WP-EN9a), `KH2_AREALOG=1 KH2_BDXLOG=1 KH2_NO_SOUND=1 KH2_NO_MUSIC=1 KH2_WAVE_GROUP=b_20 KH2_NO_PARTY=1
kh2-game al00` (Sora starts inside kind-2 group 0x30d, which creates serial 0x3fd): `area group b_20 kind-2 group 0x30d:
M_EX420 (serial 0x3fd, wave 0)`, then modes `appear_shadow`, `mode_battle`, `move_zigzag`, `idle_time`, `atk_claw` (it hit
Sora, 20 to 17 HP), `mode_revenge`. With `KH2_KEYS` (Q at 4 s, J every 0.5 s from 5.5 s): `M_EX420 hit: atkp 126 damage 3 hp 17`
.. `atkp 129 damage 6 hp 0`, `experience: enmp 9 defeated: +4`, `hp 0: switching to mode dead`, `drop (host 1.92) asked row 41`
and the prize objects picked up. No `Missing` host call and no script error in the log.

Gaps: the other 32 areas were not run (only al00's b_20); `M_EX060` and the rest of b_20 are absent kinds; the drop row
the script asks (41) was not traced further.

## The enemy M_EX880 (WP-EN9c)

`M_EX880` (object 317 = 0x13d, `00objentry` row 317: ObjectType 4, `NeoStatus` 3011, flags 0, `SpawnLimiter` 9; `enemy::M_EX880`,
called by its model: no name is in the data the app reads) is a Nobody that the area survey found in 13 areas (eh12, eh25, hb08,
hb18, he09, tt03, ..). Its BDX script (5035 words, `cargo run -p kh2 -- bdx M_EX880`) has the modes `appear`, `mode_battle`,
`btl_attack`, `btl_short`, `btl_long`, `atk_brush`, `atk_brush_all`, `atk_brush_revenge`, `atk_dragoon_jump`, `jump`, `move_target`,
`idle_battle`, `idle_time`, `mode_revenge`, `rvg_short`, `rvg_long`, `dusk_surprise`, `dead`. The rows 1184 (`M_EX880_DANCER`) and 2294
(`M_EX880_DANCER_EH`) are other objects with their own scripts and are not this kind.

- **Stats.** The set-up routine R1191 (0x1191) takes eight arguments from its one call (0x04ec); the first, 52 (pushed at 0x04d3),
  goes to `set_obj_param` (host 2.9, 0x11b8): `enmp` row 52 (HP 100 in the table; the harness enemy has 20 HP, as for the other kinds).
  `KH2_BDXLOG` shows `SetObjParam { a: 52 }`.
- **Death.** The `dead` mode (registered at 0x00eb, routine R06c3) hands the object's +0x1c to host 1.92, starts element 2 of its
  effect file (host 1.87 at 0x06e8), releases it (0.79), sets bits 3 and 6, fades out and hides: `Kind::death_effect` true.
- **Hosts.** No new host call: every one it calls is in `enemy_host.rs` already. The Stub ones it reaches are 0.79, 1.121, 1.171,
  1.206 and the mode-frozen test; the Approx ones are 1.11, 1.111, 1.120, 1.23, 1.55, 1.56, 1.60, 1.62, 1.64, 1.80, 1.87, 1.92, 2.5 and
  the mode-may-start test (each carries its reason in the table).

Acceptance run (WP-EN9c), tt03 (the first listed area; `btl` program 7 loads b_21 with two `M_EX880`, one Dusk and three absent
`M_EX950`; test `tt03_btl_program_7_holds_two_m_ex880`, disc): `KH2_PROGRAMS=0,7,0 KH2_WAVE_GROUP=b_21 KH2_NO_PARTY=1 KH2_AREALOG=1
KH2_BDXLOG=1 KH2_NO_SOUND=1 KH2_NO_MUSIC=1 kh2-game tt03` gives `area group b_21 kind-2 group 0x10b: M_EX880 (serial 0x36, wave 0)`
and the same for serial 0x67, then the modes `appear`, `mode_battle`, `btl_attack`, `atk_brush` (it hurt Sora, 20 to 16 HP),
`idle_battle`, `move_target`, `atk_brush_revenge`. With `KH2_KEYS` (Q at 4 s, J every 0.4 s from 5.5 s): `M_EX880 hit: atkp 127 damage 2
hp 18` .. `atkp 129 damage 6 hp 0`, `experience: enmp 52 defeated: +4 (lvpm 4 x enmp 100)`, `hp 0: switching to mode dead`, `drop (host
1.92) asked row 29` and `prize: drop of row 29 .. 2 objects`. No `Missing` host call and no script error in the log.

Gaps: the other 12 areas (eh12, eh25, hb08, hb18, he09, ..) were not run, only tt03; the group's `M_EX950` and Dusk-like members were
not examined; the `atk_dragoon_jump` and `atk_brush_all` modes were not seen in the run; the drop row (29) was not traced further.

## M_EX900 (WP-EN9d)

**Status: described, NOT spawned.** `enemy::M_EX900` exists but is not in `enemy::KINDS`: with it tt03's b_40 would have an
enemy that can stay invulnerable for over a minute (gap 1), which could soft-lock a Twilight Town mission. The app logs
`area groups: tt03 btl program 0x49 group b_40: Dusk, Dusk (not in the app: ["M_EX900"])` as before. Test
`m_ex900_is_described_but_not_spawned` (disc) checks the objentry row (309, type 4, NeoStatus 3004) and that b_40 skips it.
Everything below was built and run with the kind enabled (the host calls stay and serve any kind).

Event 17 sender search (WP-EN9d, second pass), all negative: the script has no literal 17 (`kh2 bdx M_EX900`); `FUN_0016c620`
(-> `FUN_0016c4c0`, `FUN_0016c550`) has 19 call sites (`mips_scan --jal 0x16c620`) with fixed ids 0, 6, 10, 0xb, 0xd, 0xe,
0xf, 0x10, 0x13, 0x17, 0x18, 0x1a, 0x1b (FUN_00169a90, 0016b3e0, 0016b420, 0016bfd0, 001767a8, 00178748, 00182f68,
00183080, 00185cf0, 001889f0, 001954f8, 001b21f0, 001b2478, 001b2588, 001b28a0, 001b3c70, 001b3d18) and one broadcast to
the global scripts (FUN_00195690, list 0x35e364); `FUN_0016c500` (7 sites: ids 7, 0xc, 0x14, 0x16 and the broadcast) and
`FUN_0016c550` (FUN_00187098, the mode machine's hooks, whose ids come from the mode record halfwords; FUN_001becb8, the link
release, whose event fields +0x57c/+0x580/+0x584 are only ever cleared: `--store-off 0x57c` -> FUN_00168d30, FUN_001beb90).
The only literal 0x11 handed to `FUN_0016c620` is at 0x188620 in the callback at 0x188498 (reached through `FUN_001889d0` ->
`FUN_00188cc0`, no direct caller): a command-record interpreter that sends event 0x11 with (object, halfword +4, halfword +6) of
a command record for one of its cases. That is the sender's shape (code 1 / 2 = halfword +4), but the producer of those command
records (the object-command queue behind `FUN_00188cc0`) and the jump-table case were not traced, and it is the kind of record the
event player / mission scripts queue, which the app does not have for enemies. No frame trigger or collision part was read in the
`M_EX900.mset`. Next step for a later WP: trace the producers of the records `FUN_00188cc0` walks.

`M_EX900` (object 309 = 0x135, `00objentry` row 309: ObjectType 4, `NeoStatus` 3004, weight 500, flags 0, `SpawnLimiter` 10;
`enemy::M_EX900`, called by its model) appears in 12 areas (eh11, eh25, he09, tt03, tt06, tt07, ..). In tt03 the `btl`
program 0x49 loads b_40 with it and two Dusks (test `tt03_btl_program_49_holds_an_m_ex900`, disc). Its BDX script
(7439 words, `cargo run -p kh2 -- bdx M_EX900`) has the modes `appear`, `dive`, `mode_battle`, `btl_attack`, `atk_upper`,
`mode_revenge`, `rvg_short`, `atk_dead`, `rc_crash`, `crash_dead`, `stun`, `freeze`, `leave`, `dead`.

- **Stats.** Its set-up routine R1ab8 (0x1ab8, one call at 0x08c8) hands `enmp` row 45 (pushed at 0x08af) to `set_obj_param`
  (host 2.9, 0x1adf): 32 HP in the harness, experience `+10` (`enmp 45 defeated: +10 (lvpm 4 x enmp 250)`).
- **Death.** The `dead` mode (registered at 0x037d) drops through host 1.92 (row 23 in the run: 6 prize objects and item
  343), then fades; `Kind::death_effect` true [inferred from the other kinds' dead modes, not traced].
- **New host calls** (`enemy_host.rs`): 1.13 queue_motion on itself (Approx: the 4-entry queue is kept in `Obj::motion_queue`,
  its first entry starts when the playing motion ends, a script `play_motion` drops the queue [inferred]; the consumer of the
  game's ring at the motion controller +0x3c was not traced; it matters: rc_crash plays motion 205 and queues 0); 1.311
  set_scaled_byte (FUN_001a0ba0: byte +0x1a8 + i of the status block = the enmp row's weakness i times the percent, rounded up;
  FUN_001a0610 multiplies the damage by byte [attack element] and byte +0x1ae (i = 6), FUN_0018b618 makes a hit ineffective
  when the product is 0; `enemy_ai` rescales `Stats::weakness`); 1.213 (0x1707d8 -> FUN_001699a8: halfword +0x4c of the
  objentry row = `NeoStatus`, 1 for every P_EX100 row; the script asks "is the target Sora"; Approx: the context lives in the
  script frame, its object is the target = the player, answer 1); 2.21 shared_block (FUN_001a5040, a process-wide
  `SharedBlocks`, the game's list is global); 2.45 hit_atkp_id (the halfword +2 of the hit record; `AiHit::atkp` carries it).
- **Run** (`KH2_AREALOG=1 KH2_BDXLOG=1 KH2_NO_SOUND=1 KH2_NO_MUSIC=1 KH2_NO_PARTY=1 KH2_WAVE_GROUP=b_40 kh2-game tt03`, Sora
  hitting with J every 0.7 s from 5.5 s): the two Dusks die, `M_EX900 (serial 0x11, wave 1) created near the player`, modes
  `appear`, `dive`, `mode_revenge`, .., the hits land (`M_EX900 hit: atkp 129 damage 6 hp 0`), `hp 0: switching to mode dead`,
  `experience: enmp 45 defeated: +10`, item 0x157 picked up. No `unimplemented host call` and no script error.

Gaps: (1) The script keeps a word at +0x18 of its context that is 1 while it flies (set by R0ddb at 0x0ddb, the dive) and makes the
main loop (0x02d3) set damage scale 6 to 0, so every hit is ineffective. It is reset to 0 by a real hit (0x021a: never, hits are
not real while it is 0), by clash event 19 (0x01ed), by event_end (0x0292) and by **event 17 code 2** (0x005c, which also
puts scale 6 back to 100); event 17 code 1 enters `crash_dead`. Nothing in the app sends event 17 and no sender was found
(callers of `FUN_0016c620` have no literal 0x11; `mips_scan --li 0x11` found `FUN_0016bbe8`, `FUN_001722c0`, `FUN_00178290`,
`FUN_00179880`, `FUN_0018852c`, none read in detail) [inferred: a motion-embedded or collision event]. So after a dive it stays
invulnerable for as long as it is not clashed (in a run it sat in `rc_crash` for over a minute, the fallback of the dive end when the
target is not within 100 cm: R12d9 -> R130f). It dies only when hit before its first dive or after the clash resets the flag.
(2) The dive's end distance (118 to 126 cm in the harness, the grab path needs under 100) depends on the app's approximate
motion control (1.61 / 1.12 are Approx), so the paired grab (motion 252, thread 0x1373) was not reached. (3) The other 11 areas were
not run; tt06 and tt07 are the other Twilight Town areas.

## The Struggle (WP-7E)

The Struggle rules are in `docs/struggle.md`. For the opponents: b_50, b_51 and b_52 (missions `TT04_MS00`,
`TT04_MS02`, `TT04_MS01`) are the Struggle matches; b_42 (`TT04_MS108`) is not. Host 4.29 now answers the
manager's counters (slot 0 the player's orbs, slot 1 the opponent's); every reaction an opponent takes knocks 3 or
15 orbs out of slot 1 (`FUN_001caeb0`); the opponents pick orbs up for slot 1; their prize drops make nothing
under the mission's header flag 0x40; global events reach their scripts as event 10, so the result 159 / 160 picks
`gameset_voice_lose` / `gameset_voice_win`.

WP-7E2: `player_finish_struggle` is a mode that the Struggle-tournament opponents `N_EX650_BTL10`, `N_EX670_BTL10`,
`N_EX680_BTL10` and `N_EX690_BTL10` register (host 1.6, next to `enemy_finish_struggle`); none of them is in tt04's
Struggle groups, so in tt04 the message finds no receiver (docs/struggle.md, "player_finish_struggle"). Host 1.34
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
