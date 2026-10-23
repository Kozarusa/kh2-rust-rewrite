# Collision polygons and invisible walls

[Contract](contract.md) | [Index](../../README.md)

## 5. Which collision polygons stop the walking player (WP-G3; user report 2026-10-11: invisible walls)

The user met invisible walls in Tram Common and Station Heights. Place names (`msg/us/place.bin` through
`hud_card`, log `hud: area card for tt7: "Market Street: Tram Common"`): **tt07 = Tram Common, tt06 =
Station Heights** (tt04 and tt05 are both Sandlot, tt08 Station Plaza).

### The original's rule
* The player's body collider is swept by `FUN_00143840` -> `FUN_00164138` (the sphere, stepped through
  `FUN_00163f98`; `FUN_0019a8e8` is the actor's entry). The polygons come from `FUN_00141b50`, which walks
  the map's tree and keeps a polygon when BOTH hold:
  1. the filter armed by `FUN_00164138` itself (`FUN_00141b30(map, 0xf, 4, 1, 0)`: mask 0xf, value 4,
     inverted): the attribute's low nibble is **not 4**;
  2. the handler `FUN_0032e3b8(map, kind)` picks from the table at 0x378f7c accepts the attribute word.
     `kind` is bits 3..6 of the collider flag word (actor +0x600; `(word << 0x1d) >> 0x20 & 0xf`, the
     argument of the `FUN_00141b50` call at 0x164138). The handlers (code at 0x32e3dc..0x32e44c, read
     from the executable's instructions; only index 4 has a decompiled `.c`):

     | kind | handler | accepts |
     |---|---|---|
     | 0, 6 | 0x32e438 | attribute & 0x10 (PARTY_COLLIDE) |
     | 1 | 0x32e3dc | attribute & 0x20 (OBJECT_COLLIDE) |
     | 2 | 0x32e3f0 | attribute & 0x40 |
     | 3 | 0x32e3fc | attribute & 0x10 or attribute & 0x80000 |
     | 4 | 0x32e420 | attribute & 0x80 (ATTACK_COLLIDE; section 3) |
     | 5 | 0x32e42c | attribute & 0x100 |
     | 7..15 | 0x32e444 / 0x32e44c | everything |

* Party members have kind 0: their constructors `FUN_001746e0` and `FUN_00174800` clear bits 3..6 of
  +0x600 (word 0x180 of the object masked with 0xffffff87). Other classes set theirs: `FUN_001a5178`, `FUN_001a51e8` kind 1
  (objects), `FUN_00189858` kind 3 (townspeople), `FUN_001d5c70`, `FUN_001d5cf8` a computed kind.
* The solver `FUN_001632b8` then SKIPS a polygon whose attribute has bit 0x100000 (it tests it first, in all
  three of its passes). It is one-sided: `FUN_001625e0` takes the signed plane distance of the sphere centre
  and collides only for `0 <= d <= radius`, so a polygon cannot be walked into from behind.
* `FUN_00330c80` records, for a hit polygon with attribute 0x800 or 0x1000, the polygon index and normal in
  the collider (+0x62, +0x40 and +0x64, +0x50): the barrier bits, not a collision decision.

### What the app did, and does now
* Before: `World::slide` (the player's wall slide, `Motor::move_by`) stopped at EVERY wall-like triangle
  (normal more than 60 degrees from vertical) whatever the polygon's attribute. A map's walls with attack-
  only (0x82) or object-only (0x22, 0x62, 0x80002) attributes therefore blocked Sora although the game's
  walking filter drops them. `kh2_map::stops_party(attr)` is the filter; `World::slide` applies it to maps
  with COCT data (`slide_with(.., party)` takes the choice; the enemies' simple mover `step_m` keeps the old
  behaviour because their kind was not read).
* Survey of the disc (wall-like polygons by attribute): the real walls are 0x172, 0x1f2, 0x1172, 0x972,
  0x3f1, 0x3f2, 0x7f1, 0x803f1; the others are 0x82 (several hundred per Twilight Town area: tt06 352,
  tt07 480, tt10 861), 0x22 (tt07 114, tt06 7), 0x80002 (tt07 63), 0x62, 0x284. Test of the walk-through
  below counts them.
* Tram Common examples (tt07): the box z = 2100..2500, x = 550..1600 (0x80002 on the -z face, 0x22 on
  the +z face) and the volumes at x = 3700..4100 (0x80002 / 0x22) in the middle of the plaza; Station
  Heights (tt06): 0x22 slabs at x = -1250, 0x82 walls at x = 1400 and 450 on the lower level.
* Test `world::tests::invisible_walls_of_tram_common_and_station_heights_do_not_stop_sora` (reads
  `map/us/tt06.map` and `tt07.map`, needs `KH2_ISO`): five scripted runs of 30 ticks of 8 units into such
  walls. The old rule (every polygon stops) halts Sora in front of the plane (for example z = 2065 at the
  box face z = 2100), the walking filter lets him through; four real walls (0x172 at x = 313 in tt06,
  0x1f2 at z = -1700 in tt07) stop him at the same point under both. Unit test
  `kh2_map::tests::walking_filter_keeps_party_collide_only` covers the attribute rule.
* Harness (the log `KH2_TRACE=1` prints one `trace ... pos (x y z)` line per tick; build of the old rule =
  the integration build in the main checkout):
  * tt07 `KH2_ENTRANCE=2 KH2_KEYS=6:W:4.0` (Sora at (4650, 0, 1100) facing -x): old rule `pos (4538 0 1100)`
    and he stays there; walking filter `pos (4522 0 1100)`, `(4122 0 1100)`, `(3722 5 1100)` ... `(3128 -246
    1114)`: through the x = 4100 volume.
  * tt06 `KH2_ENTRANCE=0 KH2_KEYS=6:S:3.0` (Sora at (-1175, 0, -300), the wall at x = -1250 behind him): old
    rule stops at `pos (-1215 20 -300)`; walking filter `(-1223 20 -300)`, `(-1255 40 -300)` ... `(-1786 40 -305)`.

### Not ported / gaps
* The floor and ceiling probes keep their own rule: `World::ground` takes every upward polygon and
  `World::ceiling` every downward one; the game's floor ray (`FUN_001423d8`, filter armed by
  `FUN_00143840`'s callee) was not compared for attributes (PARTY_STAND bit 0x1, "player fall" 0x4).
* The mode bits of the collider (bits 1..2 of +0x600; `FUN_00169af0` sets bit 1 from byte +4 of the body
  shape) and the map flag +0xd1 select special passes of `FUN_001632b8` that treat attribute nibble 1
  differently; not ported (the app uses one pass).
* Kinds other than 0: enemies (`step_m` keeps stopping at every wall), the townspeople (kind 3) and objects
  (kind 1) were not matched to their app movers. `prize.rs` calls `World::slide` and therefore gets the party
  filter now: see the report's outside-ownership note.
* The wall/floor split by the 60-degree normal is the app's; the game's sphere solver pushes out of every
  accepted polygon whatever its slope.

## 8. Invisible walls in the arenas (WP-IW1; user report 2026-10-20: Sora cannot walk into any enemy spawn region)

Cause: the app treated every COCT mesh as solid. The game's collision walker visits a mesh only while the mesh's
group is switched on by the map visibility word, and the area script leaves the arena walls of eh10 switched off.

* The walker `FUN_00141b50` (mesh loop at its `*(char *)(mesh + 0x10) != 0` test) skips a mesh whose byte +0x10 is 0.
  `FUN_00141500` (builds each collision object, called per slot from `FUN_00142ba8`, slot 0 = map entry type 6 at map
  +0x30) ends with `FUN_00141768(coct, word)`: it clears byte +0x10 of every 0x14-byte mesh entry, then for every set
  bit `g` of the 64-bit word calls `FUN_00141840(coct, g)`, which sets byte +0x10 = 1 in the meshes whose byte +0x11 is
  `g`. (`FUN_00141800` is the clearing twin.) So mesh byte +0x11 is the mesh group, a bit number of the word; the
  value stored in +0x10 on the disc is overwritten (it is 0 in the files read).
* The word is the map visibility word: `FUN_00149b40(word)` -> `FUN_00142ba8(.., word)` / `FUN_0015e6c0(.., word)`
  (the same word also selects drawn mesh groups, `FUN_0015bdb8`, and the model animations, `FUN_001db928`). It comes
  from the area script: default -1, opcode 1 sets it, opcodes 0x1a and 0x1b add and remove bits at the end of the run
  (`FUN_00181148`, docs/systems/core-runtime/contract.md). `kh2_ard::run::ScriptRun::map_visibility` already holds the value; nothing used it for
  collision.
* Data (`kh2 coct-walls <area>`, programs `KH2_PROGRAMS=0,1,0`): eh10 has one type-6 entry (tag `ID_e`, 819 polygons,
  400 meshes, 390 tree nodes) and the word is 0x407 (groups 0, 1, 2 and 10). Its party walls (attribute 0x172, passing
  `stops_party`) are 477, of which 369 are in enabled meshes. Groups 3 to 8 hold 108 wall polygons (boxes of walls around the arena
  centre: 900 units across in group 6, about 1480 across in groups 4, 5 and 7, long walls in groups 3 and 8):
  [inferred] they are the arena walls that close while a fight runs, and the area script has
  them off in program 0,1,0 (the btl program with the b_00 group). he05: word 0x3, 10 party-wall polygons in groups
  2, 3, 12, 22, 32, 62 are off (the group 12 wall, mesh 122, lies between entrance 1 and several volumes). al01:
  word 0xc1ffffffff (bits 0 to 32, 38 and 39), no party wall is in an off group (the walls crossed on straight lines are
  enabled building walls).
* Walk test `eh10_arena_walls_of_hidden_groups_do_not_block_the_spawn_volume`: from the app's startup spawn point
  (-503, 0, 260) the ungated slide ends at (-425, 0, -5), stopped at x = -425, 25 units short of the x = -450 wall of the 900-unit box (group 6 by its bounds [inferred]); from entrance 0 (0, 0, 1850) it
  ends at z = 425; entrance 1 reaches the volume either way. With the word applied all three starts end within 60 units
  of the b_00 volume centre. `he05_and_al01_entrances_walk_into_their_clear_spawn_volumes` keeps five straight
  walks (entrance to volume) working with the word applied.
* Fix in this tree: `kh2_map::Coct` keeps `meshes` (polygon range, byte +0x10, group byte +0x11, halfword +0x12) and
  `nodes`; `Mesh::enabled(word)` is `FUN_00141768`'s rule. `World` keeps each triangle's mesh and group
  (`mesh_of`, `group_of`) and `enabled_groups` (default all ones, so nothing changes until it is set);
  `World::slide` skips triangles of groups outside it. Wired (second pass): `area::run_area_script` writes `script.map_visibility` into
  `World::enabled_groups` whenever the loaded area's run differs from the world's mask; `World::slide`, `touching_m`,
  `cast_m` and `motor::sweep_wall` skip triangles of off groups.
* Hypotheses checked: H1 confirmed (above). H2 (barrier bits 0x800 and 0x1000 always solid): the walls that block in eh10
  carry 0x172, not those bits; not the cause. H4 (first type-6 entry): eh10, he05 and al01 each have exactly one
  type-6 entry, and the game too takes the first (`FUN_00149348`: `FUN_001684c0(file, 6, 0)` into map +0x30);
  not the cause. H3 (stand-in spawn point): the startup spawn (-503, 260) lies between the outer and the inner boxes, which is
  how the play-test hit the wall at 450; the placement itself is not wrong, but it is only reachable once the walls
  are gated. H5 (`World::ground` ignores attributes): not checked; the failing walks end at walls, not floors.
* Gaps: the tool's wall test is a point ray at 40 units above the entrance (not `motor::sweep_wall`, because
  the `kh2` crate does not link `kh2-app`); halfword +0x12 of a mesh is not read by the walker (searched
  `FUN_00141b50`, `FUN_00141768`, `FUN_00141800`, `FUN_00141840`). Run-time rewrites (traced second pass): the area word is
  applied once (`FUN_001047a8` -> `FUN_00149b40(DAT_0035cd10)`; `FUN_0015e6c0` copies it to the live mask
  `DAT_0035a810`). After that only three things change a group: `FUN_0015e9a8` (group on: sets the bit in
  `DAT_0035a810`, then `FUN_00143028` -> `FUN_00141840` on every collision slot `DAT_00355930..44`) and
  `FUN_0015e928` (off: `FUN_00142f78` -> `FUN_00141800`), reached from BDX host calls 1.37 and 1.36 and from area opcode 0xb
  (`DAT_0035cd18` mask, docs/systems/core-runtime/contract.md). No battle manager writes the area word. So an arena that closes during a
  fight is done by an enemy or event script calling host 1.37 / 1.36 (implemented by WP-IW2, below).

### 8.1 Run-time group switches and the other queries (WP-IW2)

* Host 1.36 `mask_35a810_clear` (0x172418 -> `FUN_0015e928`) and 1.37 `mask_35a810_set` (0x172430 -> `FUN_0015e9a8`): clear
  or set bit `i` of the live mask `DAT_0035a810`, then `FUN_00142f78` / `FUN_00143028` run `FUN_00141800` / `FUN_00141840` (the
  enabled-byte writers of 8 above) on each of the five collision slots `DAT_00355930..44`; the same functions also run the drawn
  mesh group hooks (`FUN_00153458` / `FUN_001534c8`, `FUN_0015bfb8` / `FUN_0015bf68`) and, while `DAT_0035a80c == 1`, the model
  animation hooks (`FUN_001dbad8` / `FUN_001db928`). `enemy_host.rs` has one `approx` arm for both (collision bit exact; the draw
  and animation hooks are not ported): it queues `(group, on)` (bits above 63 dropped), and `area::run_area_script` applies the
  queue to `World::set_group` in call order every frame, through `area::sync_groups`.
* The area word is written once per world (`World::apply_area_word`, `FUN_001047a8` -> `FUN_00149b40` -> `FUN_0015e6c0` copies it
  to `DAT_0035a810` a single time). The IW1 wiring wrote it whenever it differed, which would have undone a run-time switch on the
  next frame; a freshly loaded area builds a fresh `World`, so the word applies again there. `KH2_AREALOG=1` logs
  `area <name>: world collision mask 0x.. applied (script word 0x..; FUN_00141768)` and one line per switch
  (`collision group N on|off (host 1.37|1.36); world mask now 0x..`). Test:
  `the_world_gets_the_area_word_once_and_then_the_script_switches`.
* Which queries the group filter covers: all of them. Every collision query of the game ends in the mesh loop of
  `FUN_00141b50` (callers `FUN_00142760` the vertical ground probe with mask 0xf, `FUN_00141f08` the ray cast under
  `FUN_00143aa0` / `FUN_00143b80` / `FUN_00143c28` and others, `FUN_00164138` the capsule sweep, `FUN_00164ce8` via
  `FUN_00143388`), and the loop skips a mesh whose enabled byte is 0 before it looks at a polygon. So the filter now lives in
  `World::near`, the candidate lookup every query shares: `ground`, `ceiling`, `raycast` / `raycast_m`, `blocked_m`, `slide`,
  `touching_m`, `cast_m` and (through its own loop) `motor::sweep_wall` skip triangles of off groups. `spawn_point` and
  `spawn_point_band` are the app's stand-in start search (no game function), gated the same way so they never pick the floor of a
  hidden mesh. The world is shared by the party and the enemies' simple mover (`step_m`, `ground_m`), so enemies also stop seeing a
  hidden floor or wall. `CameraWorld` is built from render meshes without groups and is unaffected. Test:
  `hidden_groups_are_no_floor_ceiling_wall_or_spawn_point` (synthetic floors, ceiling and wall of two groups).
* Who calls the switches in eh10: of the six scripted objects the spawn groups of eh10 create (`M_EX920` in `b_00`, `B_EX140` in
  `b_40`, `P_EX020_RTN` and `P_EX030_RTN` in `e03_`, `N_EX900_RTN` in `d00_`, `N_EX560_RTN` in `b00_`), only `B_EX140` calls 1.36
  (3 call sites) and 1.37 (4 call sites), with group 2 and 8 as immediates and one global variable (`load global+0x314`).
  `M_EX920`, the kind of the user's `b_00` group, calls neither, so the b_00 arena stays as the area word 0x407 leaves it
  (walls of groups 3 to 8 off) for the whole fight [inferred from the scripts; the group-8 and group-2 switches of the boss in `b_40`
  are applied when that script runs]. Across the disc 82 BDX scripts use 1.36 (321 call sites) and 1.37 in 48 scripts (228 call sites)
  (`kh2 bdx --scan`); only eh10 was walked through for kinds. Test: `eh10_enemy_scripts_and_the_collision_group_switches`.
* Area opcode 0xb (`DAT_0035cd18`, `FUN_00180b48` case 0xb) only stores a 64-bit mask (argument 0 low word, argument 1 high word;
  `FUN_00181148` zeroes it at the start of a run); the groups change when a script calls the parameterless host functions
  0x172800 (`FUN_0019d4c0`: for bit 0 to 63, `FUN_0015e9a8` group-on per set bit) and 0x172818 (`FUN_0019d520`: `FUN_0015e928`
  group-off per set bit). WP-IW3 found them in the executable's group 1 table (group pointer table 0x35bce8, entries `{function,
  flags}` of 8 bytes; the entry of 1.36 is at file offset 0x25cec8): 1.46 `area_mask_groups_on` (0x172800, flags 0, no arguments, no
  result) and 1.47 `area_mask_groups_off` (0x172818, the same). The table is not monotonic in the index (1.38 0x170950, 1.41 0x1727b0,
  1.44 0x1727d0, 1.45 0x1727e8, 1.46 0x172800, 1.47 0x172818, 1.48 0x1723d8), so the place came from the entry whose function field equals
  the address, not from position. Port: `kh2_ard::run::ScriptRun::word_35cd18` (test `opcode_0xb_stores_the_64_bit_group_mask`; the
  last 0xb command wins as in the game); `area::run_area_script` hands the loaded area's word to `enemy_host::set_area_mask` every
  frame; `enemy_host.rs` arms 1.46 / 1.47 (`approx`, same note as 1.36/1.37) push one `(group, on)` edit per set bit, lowest first,
  into the queue of 8.1 (test `the_area_mask_hosts_switch_each_set_bit`). Limits: the word is the loaded area's value as the last
  area-script run left it [inferred: the game's global is also read by nothing else that was traced; `FUN_0019d4c0`/`FUN_0019d520` are
  the only readers found in the decomp folder]; later area runs inside one world do not happen in the app. Which areas have a 0xb
  command: docs/systems/script-host/area-survey.md "Area opcode 0xb". No script call site of 1.46/1.47 was counted (`kh2 bdx --scan` was not run for them).
* Walk test tightened: `eh10_arena_walls_of_hidden_groups_do_not_block_the_spawn_volume` now requires every gated walk to arrive
  within `walk_to`'s own 60-unit stop radius (it was 80 or inside the volume), and requires the ungated world to wall in exactly
  the startup spawn and entrance 0 with their ends more than 100 units from the gated ends (entrance 1 arrives either way).
  Measured: gated ends 55.3, 54.7 and 59.3 units from the volume centre.
* Decompile index: `FUN_00141b50`, `FUN_00141500`, `FUN_00141768`, `FUN_00141800`, `FUN_00141840`, `FUN_00142ba8`,
  `FUN_00149b40`, `FUN_00149348`, `FUN_0015e6c0`, `FUN_001491f8` (map +0x50), `FUN_001db4d0`, `FUN_001db928`.
