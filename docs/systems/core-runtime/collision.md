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

## 9. The COCT queries, moving collision objects and the geometry unit (WP-L1-S3, spec for L1-P10)

Scope: ledger clusters L1:coct (0x141898..0x144328, 0x163f98..0x1657d8) and L1:geom (0x144368..0x149ae8), 175 functions
unread before this package. Coordinates are the game's (Y down); the app flips Y and Z. Field offsets are named by owner:
`coll +0x..` is the collider record that every query takes (the actor keeps it at `actor +0x590`, section 5 and
`docs/systems/player-camera/moveset-selection.md`), `oct +0x..` is a built collision object (0xe0 bytes, built by
`FUN_00141500`, section 8), `cull +0x..` the map draw-cull object of 9.6.

**Status (WP-L1-P10):** ported in `crates/kh2-map/src/oct.rs` and `geom.rs` and checked against the original in kh2-ee by
`crates/kh2-game-core/tests/orig_coct.rs` (random COCT files built by the game's own `FUN_00141500`, plus ten real areas): the
box leaves, the polygon walker with its handler table, the ray cast, the floor ray with the snap rule, the vertical probe, the
closest floor, the floor kind probe, the floor query `FUN_00143560` (without the lighting colour words), the sphere sweep with
its solver, and `FUN_00143840` for a map with no moving collision objects. Not ported: the moving-object pool and its two
passes (`FUN_00165430`, `FUN_001654f8`), the placement matrix of a moved object (`FUN_00142858`) and the branches that
depend on it, the other rays (`FUN_00143960`, `FUN_00143ce8`, `FUN_00143b80`, `FUN_00143c28`), the draw-cull half of 9.6. The
app's mover does not call any of this yet (it is the mover's package, L2); `World::walk_step`, `floor_ray` and `floor_info`
expose it in the game's space.

### 9.1 The collision slots and their accessors
The five collision objects of section 8 and the draw-cull object live in globals; the wrappers in 0x143100..0x143e90 only
pick a slot and forward.

| slot | what reads it | evidence |
|---|---|---|
| `DAT_00355930` | the draw-cull object of 9.6 (not collision) | `FUN_00143100` / `FUN_00143120` forward to `FUN_00148440` / `FUN_00148478`; `FUN_00143140` / `FUN_00143160` set `DAT_0035594d` then call them; `FUN_00143240`, `FUN_00143260`, `FUN_001432b0`, `FUN_001432d8` register objects in it (9.6); `FUN_001431d0` / `FUN_001431e8` set / clear `cull +0x779` |
| `DAT_00355934` | the walking map: every party, enemy and ray query | `FUN_00143840`, `FUN_001439f8`, `FUN_00143960`, `FUN_00143ce8`, `FUN_00143aa0`, `FUN_00143b80`; accessors `FUN_00143dc0` (polygon vertices, `FUN_00142120`), `FUN_00143de0` (plane, `FUN_00142190`), `FUN_00143e68` (vertices into a buffer, `FUN_001422d8`), `FUN_00143360` (`FUN_00141690`), `FUN_001433e8` (box of a tree node, `FUN_00142818`) |
| `DAT_00355938` | the second map collision, optional | `FUN_00143c28` (ray, kind 0xf, returns 0 when the slot is empty), `FUN_00143410` (node box); `FUN_00143d68` and `FUN_00143e00` take it instead of `DAT_00355934` when their flag argument is set and the slot is non-zero |
| `DAT_0035593c` | the floor colour lookup | `FUN_00143778` (a ray with the select mask cleared and kind 0xf; the hit polygon's byte +0 picks the colour table, `FUN_0015c690`); `FUN_00143560` (9.4) |
| `DAT_00355940` | a box list read by `FUN_00165990` | `FUN_00143e90` (fills 0x355960 / count 0x355950 while `DAT_0035594f` is set), `FUN_00143ee0` (tests a box against those entries with `FUN_00146bc8`), `FUN_00143f78` (fills `cull +0x270` / count `cull +0x770` from the camera, `FUN_001068c0(1)`) [inferred: occluder volumes; their consumer is `FUN_00145fa0`, 9.6] |
| `DAT_00355944` | the floor kind probe | `FUN_001434b8`: the vertical probe (`FUN_00142760`) and the closest floor (`FUN_001425a0`) of 9.3; returns bits 14..15 of the floor polygon's attribute word (0 without a slot or a floor) |
| `DAT_00355948` | an object with a vtable, freed by `FUN_00142e68` | `FUN_00143230` (word +0), `FUN_00143208` (bytes +0x54..+0x56), `FUN_001430d8` / `FUN_001430e8` (byte +0x8c) [inferred: the drawn map model; not a collision object] |

* Flags: `DAT_0035594c` (set: `FUN_00143ff8` computes the player's projected screen box into `cull +0xc44..+0xc4c`), `DAT_0035594e`
  (set by `FUN_00143180`, cleared by `FUN_00143190`; while it is 1 `FUN_00144168` skips the whole draw-cull pass), `DAT_0035594f`
  (`FUN_001431a0` reads, `FUN_001431b0` sets, `FUN_001431c0` clears; gates the `DAT_00355940` boxes).
* `FUN_00142e68` frees the slots in the order 0x355944, 0x355940, 0x35593c, 0x355938, 0x355948 (through its vtable entry +0x28),
  0x355934, then 0x355930 (`FUN_001459d0` first), after `FUN_0014ead0` and `FUN_00153378`.
* `FUN_00142b10` walks every polygon of a collision object and, for an attribute with bits 17..18 set, calls
  `FUN_0014ea50((attr >> 17) & 3, polygon)` [inferred: registers sound or effect polygons; consumer not read].

### 9.2 Polygon records and the object's transform
* Polygon `i` of `oct` is 16 bytes at `oct +0xc + i * 0x10`: halfwords +2, +4, +6 are vertex indices into the vertex table
  (`oct +0x10`, 16 bytes per vertex), +8 the fourth vertex or 0xffff for a triangle, +0xa the plane index (`oct +0x18`, 16 bytes:
  normal xyz and distance w), +0xe the attribute index (`oct +0x14`, 4-byte words). Byte +0 and +1 of the record are read by the
  colour lookup (`FUN_00143778`, `FUN_00143560`). `FUN_001418a0` returns the vertex pointers and a count 3 or 4 (`FUN_00142120` does
  the same into the static buffer 0x484360), `FUN_00142190` the plane pointer, `FUN_001422b0` the attribute word pointer.
* Every collision object has a placement: `FUN_00142858(oct, m)` stores the matrix at `oct +0x50`, its inverse (built with
  VU0 outer products and one divide) at `oct +0x90`, and sets `oct +0xd1 = 1` when `m` is the identity (diagonal compared with
  `DAT_00378f74`). With `+0xd1 == 0` every query maps its points into the object's frame with `+0x90` and the results back with
  `+0x50` (`FUN_00164138`, `FUN_00141910`, `FUN_001422d8`); `FUN_001421b8` returns a plane, its normal rotated and renormalized
  (w replaced by `DAT_00378f5c`). The map object is the identity case.
* `FUN_00142a88(oct)` returns 1 (object skipped by both moving-object passes) when any basis row of `oct +0x50` has a squared
  length below `DAT_00378f78`: a collapsed (scaled to zero) object.
* Select mask: `FUN_00141b30(oct, mask, value, invert, b)` writes `+0x40`, `+0x44`, `+0xd0`, `+0xd3`. The walker keeps a polygon when
  `(attr & mask) == value` (inverted when `+0xd0` is set) and the kind handler of section 5 accepts it. The armed values:

  | caller | mask, value, invert | kind | what |
  |---|---|---|---|
  | `FUN_00164138` sphere sweep | 0xf, 4, 1 | collider bits 3..6 | walls: floor-kind 4 polygons ignored |
  | `FUN_001423d8` floor ray | 0xf, 4, 1 | collider bits 3..6 | the same filter for the ground |
  | `FUN_00142760` vertical probe | 0, 0, 0 | 0xf (all) | every polygon |
  | `FUN_001439f8`, `FUN_00143aa0`, `FUN_00143c28`, `FUN_00143778` rays | 0, 0, 0 | argument / 0xf | every polygon the kind accepts |
  | `FUN_00143b80` ray | 0x200, 0x200, 0 | 0xf | attribute bit 0x200 only |
  | `FUN_00143960` ray | 0xf, 4, 1 (and +0xd3 = 1) | argument | as the floor ray |
  | `FUN_00143ce8` ray | 0xf, argument, 0 | argument | one floor kind only |

  `FUN_001439d8` and `FUN_00143a78` are thin wrappers of `FUN_001439f8` and `FUN_00143aa0`.

### 9.3 Ground: the floor ray, the vertical probe and the closest floor
* Floor ray `FUN_001423d8(oct, coll)` (read by WP-5A, `moveset-selection.md`): a segment at the sweep's end `coll +0x30` from
  `lift + clamp(|coll +0x14|, 0, 100)` above (clamp only for kind 0) to `3 * lift` below (`lift` = `coll +0x6c`), filter 0xf/4
  inverted, cast by `FUN_00141f08`. No hit: bit 33 of the flag word `coll +0x70` set and the floor height `coll +0x68` = 320000
  (`DAT_00378f68`). Hit: `+0x68` = hit y, bit 33 cleared. Both end in `FUN_00141910`.
* `FUN_00141910(oct, polygon, coll, hit)`: no polygon: `coll +0x60` = 0xffff and bit 32 set. With `coll +0x70` bit 0 clear (the
  walking case), the hit is accepted as the new end (`coll +0x30` = hit, `+0x60` = polygon, bit 32 clear) when
  `hit.y <= start.y + max(0, delta.y) + (lift - coll +0x24) + coll +0x80 - 1.0`, else bit 32 set (a drop too deep to snap to). With
  bit 0 set the end point (mapped into the object frame) is tested against the polygon by `FUN_00163248`; inside: bit 32 = (floor
  kind == 2), end = hit; outside: bit 32 set.
* Vertical probe `FUN_00142760(oct, coll)`: the box from `(x, y - lift, z)` to `(x, y + DAT_00378f70, z)` of `coll +0x30`, quantized by
  `FUN_00144d68`, collected by `FUN_00141b50` with no attribute filter and kind 0xf into the hit table; returns the count.
* Closest floor `FUN_001425a0(oct, out_y, count, coll)`: for each collected polygon (indices at `oct +0x3c`) whose plane has the
  probe point (`coll +0x30`, y raised by the lift) on its front or on it, and whose outline contains the point in XZ (`FUN_00161ed0`),
  the height on the plane `y = (-d - (n.z * z + n.x * x)) / n.y`; keeps the one nearest the raised probe y, closer than
  `DAT_00378f6c`; returns its index (0xffff none) and writes `out_y`.
* **Status (WP-L1-P10): the roof stop is found.** The sweep itself stops a rising body: the sphere (radius `coll +0x20`, centre
  one lift above the foot) is pushed out of every polygon the kind-0 handler accepts (attribute bit 0x10), whichever way the polygon
  faces, so a roof stops the head one radius plus 0.008 under it (`a_roof_stops_a_jump_in_the_game_through_the_sweep`, game and
  port agree; a roof without bit 0x10 does not stop the rise). An airborne body has bit 0 of `coll +0x70` set (`FUN_00182690`),
  which makes the floor ray report instead of snapping the end down. `World::ceiling` (every downward triangle, attribute
  ignored) has no counterpart in the original and should go when the mover is rewired.
* Ceiling: no ceiling probe exists in these clusters. Head contact is the sphere sweep itself (its start is lowered by the lift,
  so the sphere rides at body height) [inferred: no function in 0x141898..0x149ae8 or 0x163f98..0x1657d8 casts upward or looks for
  a downward-facing polygon; the vertical probe box above reaches only `DAT_00378f70` over the foot]. The app's `World::ceiling`
  (every downward triangle) has no counterpart; L1-P10 must find what stops a jump under a roof (candidates: the sweep with the
  air delta, `FUN_00143b80`) before keeping or removing it.

### 9.4 The floor information query
`FUN_00143560(coll, out)` (read before, summarized here because P10 ports it): clears the colour table and runs the floor colour
lookup in `DAT_0035593c` (`FUN_00142760` + `FUN_001425a0`; the polygon's byte +0 picks the table, byte +1 goes to `out +0x2c`), feeds
the raised foot point to `FUN_00153540`, stores the floor kind of 9.1 (`FUN_001434b8`) at `out +0x30` and the floor polygon
`coll +0x60` at `out +0x24`. Without a floor polygon the normal is straight up (`DAT_00378fc0`, `DAT_00378fc4`). With one, the normal
comes from `FUN_00143d68` (map) or `FUN_001421b8` on the moving object `coll +0x78`; `out +0x34` = attribute halfword +2 bit 0
(`FUN_00143e00` / `FUN_001422b0`), `out +0x28` = polygon byte +0, and `out +0x10` = the negated normal.

### 9.5 The full walking step `FUN_00143840` and the moving collision objects
* `FUN_00143840(coll)`: `+0x62` and `+0x64` = 0xffff, bit 34 cleared, a local floor value = `DAT_00378fc8`; then
  1. `FUN_00165430(coll)`: the sphere sweep against every moving collision object (below); a hit rewrites the delta `+0x10` =
     `+0x30 - +0x00` so the map sweep continues from the pushed end;
  2. `FUN_00164138(DAT_00355934, coll)`: the sphere sweep against the map;
  3. `coll +0x78` = 0, then `FUN_001654f8(coll, &floor)`: the floor ray against every moving object (the highest wins). None:
     the map floor ray `FUN_001423d8`. One: `+0x10` rewritten as in 1 and the map sweep runs again;
  4. if the local floor changed, `coll +0x68` = it and bit 33 cleared. Returns the OR of the sweeps' contact bits (bit 34).
* The 0x164138 sweep `FUN_00164138(oct, coll)`: kind 6 (`(coll +0x70 & 0x78) == 0x30`) sets mode bit 2 and clears bit 1 first. The
  start is `coll +0x00` with y lowered by the lift `coll +0x6c`, the end start + delta `coll +0x10`, both mapped into the object frame
  when `+0xd1 == 0`. Steps `n = floor(|delta| / r) + 1` (`r` = `coll +0x20`), step = delta / n (one step of the whole delta when the
  quotient is 0). The candidate box is the end point's box widened by `r * DAT_0037953c`, merged with the start (`FUN_00144410`),
  quantized (`FUN_00144d68`), and the polygons come from `FUN_00141b50` with filter 0xf/4 inverted and the collider kind. No
  polygon: the end is start + delta. Otherwise `FUN_00163f98` runs once per step (the solver of section 5), each step starting at
  the previous result `coll +0x30`. Then, unless the kind is 6 and the end equals the start, a tunnelling guard casts the segment
  start to end with `FUN_00141f08` (the same kind); a hit puts the end back on the start and returns 0. The end's y gets the lift
  back; the result is bit 34 of `coll +0x70`.
* Moving objects (lifts, platforms, other actors' bodies [inferred]): a pool at `DAT_0035a8a0` (`FUN_001652b8`: 64 entries of 16
  bytes, a free-index stack at +0x400 with its count at +0x500, allocator `DAT_0035a8a4`; freed by `FUN_00165388`). An entry is
  `{owner, oct, flags, next}`: `FUN_001653d8(file, owner)` pops an index and `FUN_001650f0` builds the collision object
  (`FUN_00141500(new 0xe0 block, file, all groups)`) and links it at the tail (`DAT_005210c0` head, `DAT_005210c4` tail);
  `FUN_00165188` unlinks, frees and pushes the index back; `FUN_00165290` / `FUN_001652a0` set / clear flag bit 0 (disabled);
  `FUN_00165250(entry, m)` places it (`FUN_00142858`). `FUN_001650e0` clears an entry. The list is walked by `FUN_00165910`.
  `FUN_00165430` sweeps against each entry whose owner is not the collider's owner `coll +0x7c`, not disabled and not collapsed
  (`FUN_00142a88`); `FUN_001654f8` casts the floor ray against the same entries and keeps the highest floor (smallest y, start
  `DAT_0037955c`) when the hit is a real floor (`coll +0x70` bit 32 clear after the ray), copying the whole 0x84-byte collider of
  that result back and setting `coll +0x78` = the entry.
* `FUN_00164690` and `FUN_00164780` serve the other sphere pass `FUN_00164ce8` (through `FUN_00143388`, section 8.1):
  `FUN_00164780(?, polygon, point, out, a, b)` walks the polygon's edges (next-vertex tables 0x35a880 for 3 vertices, 0x35a890 for
  4), projects the point on each edge line and keeps the projections inside the edge's box widened by the epsilon vector
  `DAT_005210b0..bc` (`DAT_00379548`, w `DAT_0037954c`); with several it keeps the one nearest the point and returns its edge ends
  in `a`, `b` (returns 0 when none). `FUN_00164690(?, a, dir, b)` returns `|normalize(b - a) - dir| * DAT_00379544`
  [inferred: an angle measure between an edge and a direction; its use inside `FUN_00164ce8` was not read].

### 9.6 The geometry unit: boxes, frusta and the map draw-cull octree
The L1:geom run holds two different things: box and plane helpers that collision uses, and the map draw-cull object
(`DAT_00355930`, built by `FUN_00145598` / `FUN_00145748`), which is drawing, not collision.

Helpers (collision and drawing):
* Float box (min xyz, max xyz): set from six floats `FUN_00144388`, `FUN_001443a8`, `FUN_001446d8`; from two corners in any order
  `FUN_001446f8` (wrappers `FUN_001443c8`, `FUN_001443e0`, `FUN_00144518`, `FUN_00144530`: the last two take a segment's two points);
  union `FUN_00144778` (wrappers `FUN_001443f8`, `FUN_00144410`); sphere (centre, radius at +0x10) `FUN_00144428`, `FUN_00144488`;
  polygon bounds `FUN_00144548` (3 or 4 vertices; wrappers `FUN_001444e8`, `FUN_00144500`); centre `FUN_00144820` (min + extent *
  `DAT_00378fe0`, w `DAT_00378fdc`); box transformed by a matrix then re-bounded `FUN_001448c0` (its 8 corners from `FUN_00144a50`, w
  `DAT_00378fe4`); corners with y and z negated `FUN_00144af8` (w `DAT_00378fe8`) [inferred: a frame flip for the draw side].
* Tests: overlap `FUN_00144c28` (closed intervals on all three axes), containment `FUN_00144ca8` (second inside first).
* Quantized box (six int16, the walker's form): clear `FUN_00144d28`, `FUN_00144d48`; from a float box `FUN_00144d68(out, box, level)`
  (subtract the per-level offset `DAT_00355e60[level]` from the min and add it to the max, scale by `DAT_00355e70[level]` unless level
  0, clamp min with `DAT_00378fec` / max with `DAT_00378ff8` and both into `DAT_00378ff0..DAT_00378ff4`, truncate); back to floats
  `FUN_00145000` (scale `DAT_00355e60[level]` again, level from `FUN_00141888`); its corners `FUN_001450f8` (w `DAT_00378ffc`);
  overlap `FUN_00145498`, containment `FUN_00145518` (the int16 twins of the float tests).
* Planes and frusta (VU0): `FUN_001466a0`, `FUN_00146808` six planes of an 8-corner box (`FUN_00163070` per face); `FUN_001469c8`
  five planes of a pyramid (apex and four base corners; wrappers `FUN_00146af0`, `FUN_00146b08`); `FUN_00146970` point inside six
  planes (all signed distances negative); `FUN_00146b20` box inside five planes (the box corner on each plane's negative side);
  `FUN_00146bc8` all 8 corners behind one of five planes (box fully hidden); `FUN_001465c8` 8 corners against a 4x4 clip matrix with
  VCLIP: 0 all outside one side, 1 all inside, 2 straddling; `FUN_00146598` copies a 4x4 matrix.

Draw-cull object (`cull`, size about 0x5380) [inferred: map mesh and object culling for drawing, from its camera, projection and
GS scissor writes]:
* Build `FUN_00145598` / `FUN_00145748` (twins with different initial box constants `DAT_00379004/8`, `DAT_0037900c/10`): over a file
  whose header gives three tables: nodes (+0x14 offset, +0x18 size / 0x30), meshes (+0x1c, +0x20 / 0x1c) and a third (+0x24, +0x28 /
  0x34); sets the root box, enables mesh groups from the map visibility word (`FUN_00145aa8`: clear byte +0 of every 0x1c-byte mesh,
  then `FUN_00145b80` per set bit, the drawing twin of section 8's `FUN_00141768`; `FUN_00145b40` is the group-off twin) and clears
  the per-node object lists (`FUN_00146c40`). Destroy `FUN_001458f8`, `FUN_001459d0`.
* Per frame `FUN_00144168` (skipped while `DAT_0035594e == 1`): camera planes (`FUN_00145bc8`: two clip matrices into `cull +0x1f0`
  and `+0x230` from the camera and projection, `FUN_00146598`), object flags reset (`FUN_00146cf8`), occluders (`FUN_00143f78`),
  the player's screen box (`FUN_00143ff8`), the visible mesh list (`FUN_001463b0` recursive over the 8 children of each node with
  `FUN_00146648`, `FUN_001462a8` for a fully visible node, `FUN_00146040` and `FUN_00146018` append, index | 0x8000 for a straddling
  mesh; `FUN_00145fa0` drops a node hidden by an occluder), the draw call `FUN_0011cb38` with the camera-space matrix unless
  `DAT_0035594d`, then `FUN_00147300`. Setters `FUN_00145f88` (+0x1c list buffer), `FUN_00145f90` (+0x5368), `FUN_00145f98` (+0x5364).
* Objects in the octree (records with flags at +0xf8, box at +0xd0, node +0x118, depth +0x11a, slot +0x11c, link +0x110, children
  +0xf0 / siblings +0xec): insert `FUN_00146da0` and `FUN_00146d40`, re-place `FUN_00146e40` / `FUN_00147058` / `FUN_00146f68` (deepest
  node whose box contains the object's, `FUN_00144ca8`), `FUN_00146df0`, `FUN_00146c80` (copy +0x40 matrix to +0x80, clear the lists),
  visibility marks `FUN_00146108`, `FUN_00146168`, `FUN_001470f0`, `FUN_00147128`, `FUN_00148920`, `FUN_00148478`; scene-event gate
  `FUN_00147158` (`FUN_00224b18`); billboard matrix `FUN_00147728`; draw ordering `FUN_00147980`, `FUN_00147b90`, `FUN_00147bb8`,
  `FUN_00147c98`, `FUN_00148388`, `FUN_00148d58`, `FUN_00147e20`, the sort table 0x3b4070 (`FUN_00148948`, `FUN_00148980`,
  `FUN_001489d8`); shadow or decal passes `FUN_001484b0`, `FUN_00148a58`, `FUN_00148ae0`, `FUN_00148b68`, `FUN_00148e08`,
  `FUN_00149068`, `FUN_001490a0` [inferred from the GS packet setup at 0x484380 and `FUN_0011dc40`]; GS scissor `FUN_00147d98`,
  `FUN_00147de8` (register 0x18).
* The tail 0x1490c0..0x149ad0 is a small record unrelated to the octree: clear `FUN_001490f0` (wrappers `FUN_001490c0`, `FUN_001490d8`),
  getters of +0x20..+0x4c (`FUN_00149198`, `FUN_001491a0`, `FUN_001491a8`, `FUN_001491b0`, `FUN_001491b8`, `FUN_001491c0`,
  `FUN_001491c8`, `FUN_001491d0`, `FUN_001491d8`, `FUN_001491e8`, `FUN_001491f0`) and of the 12-byte entries at +0x7c / +0x80
  (`FUN_00149238`, `FUN_00149250`); a file load by area name `FUN_00149268` / `FUN_001492b8` / `FUN_001492e8`, a cache read of 0x438
  bytes `FUN_00149328` and its free `FUN_00149ad0` [inferred: the area's light or layout file].
* Ledger consequence: the draw-cull functions above are render work, not core.collision; this package marks them in their notes
  (`S3:draw`) and leaves the system label to the orchestrator (proposed: `render.model`).

### 9.7 What the port has, and the delta for L1-P10
| game | port today | delta |
|---|---|---|
| `FUN_00164138` sphere sweep, `r` = `coll +0x20`, n = floor(len / r) + 1 steps, box widened by `r * DAT_0037953c`, tunnelling guard | `World::slide_with` (capsule, `ceil(len / (r / 2))` steps up to 32, push `r - d + 0.01`) and `motor::sweep_wall` | step count, box, guard and solver differ; port `FUN_00163f98` / `FUN_001632b8` and the sweep exactly |
| filter 0xf/4 inverted + kind handler | `stops_party` + `group_enabled` | the floor-kind 4 test is missing for the ground; walls use `stops_party` (kind 0 only) |
| floor ray `FUN_001423d8` + `FUN_00141910` snap rule | `World::ground` (highest upward triangle in the span), `ground_m` | no attribute filter, no snap limit (bit 32), no 320000 sentinel |
| no ceiling probe | `World::ceiling` | find the game's roof stop (9.3) before keeping it |
| vertical probe + closest floor `FUN_00142760` / `FUN_001425a0`, floor info `FUN_00143560` | none (floor kind, colour, normal from `World::ground`) | port as `Coct` methods |
| rays `FUN_00141f08` under the mask table of 9.2 | `World::raycast`, `raycast_m`, `cast_m`, `blocked_m` | masks 0x200 and per-kind not modelled |
| moving collision objects (pool 0x35a8a0, `FUN_00165430`, `FUN_001654f8`) | none | needed for lifts and platforms; the list is empty in a plain field, so the map path can be ported first |
| object placement `FUN_00142858`, frame mapping when `+0xd1 == 0` | none (map is identity) | only with moving objects |
| geometry helpers 9.6 | glam boxes inside `world.rs` | port the box and quantized-box leaves into `kh2-map` for exactness |

### 9.8 The rig deviation "Sora stops at x -215" (tt-run-06)
**Status (WP-L1-P10): `coll +0x20` is the body radius.** The collider is the record at `actor +0x590` (`coll +0x20` = `actor +0x5b0`,
`+0x24` = `+0x5b4`, `+0x6c` = `+0x5fc`, `+0x70` = `+0x600`, `+0x30` = `+0x5c0`). The only instruction in the executable that stores
`+0x5b0` is the one in `FUN_00169af0` (0x169b44, the body shape's halfword +0x10); `FUN_00183eb0` overrides `coll +0x20` only on its
own local copy (the push-out loop of `move_to_space`). So the sweep radius is 90 for Sora (the value read from the model in
`moveset-selection.md`), not the port's 35: walls would stop the body 55 units earlier once the mover calls the sweep. PCSX2 was not run
(the single writer settles it).
Read from `local/rig/tt-run-06` (report, traces and the port log): the area is tt21 (Mansion: Computer Room, `map/us/tt21.map`,
one type-6 entry, word 0xe7). The port's Sora reaches z = 400 (game) around k150 and slides along it, then stops at x = -214.99 by
k200 and stays. `kh2 coct-walls tt21` lists a party wall (0x172, mesh 107, group 0, enabled) on the plane x = -250 from z 150 to 500
(game), 130 high, and a 0x1f2 wall (mesh 50) on z = 435 for x -475..-20: the stop point is 35 units (the port capsule radius) from
both, a corner. The original never goes there: its facing turns towards +x from k130 (0.60, 0.80 at k200, port still -0.53, 0.85)
and it ends at (103.7, 488.6), where no wall of that corner is. So the cause the spec shows is the heading deviation of the same
scenario (facing does not follow the stick, DEVIATIONS 2026-10-23, L2), not the collision rule. decided: the -215 row is a
consequence of the facing row; L1-P10 should re-run tt-run-06 after the facing fix. One collision difference remains open in the
same picture: the game's sphere radius is `coll +0x20`, and the body shape radius `actor +0x5b0` was read as 90 for Sora
(`moveset-selection.md`), while the port uses 35; which value `coll +0x20` holds was not traced [inferred], and with 90 the port
would stop 55 units earlier at every wall. P10 must read `coll +0x20` in a trace before porting the sweep.

### 9.9 Oracle vectors plan for L1-P10
Pure leaves (factory `fuzz(&SPEC, n >= 1000)`, no stubs; kh2-ee supports the VU0 macro ops used, `crates/kh2-ee/src/vu.rs`):
* Float boxes: `FUN_001446f8`, `FUN_00144778`, `FUN_00144428`, `FUN_00144548` (3 and 4 vertex records), `FUN_00144820`,
  `FUN_001448c0`, `FUN_00144a50`, `FUN_00144af8`, `FUN_00144c28`, `FUN_00144ca8`; inputs: random boxes with min/max either order,
  degenerate (min = max) and extremes +-1e5.
* Quantized boxes: `FUN_00144d68` (levels 0..7; the tables 0x355e60 / 0x355e70 are the executable's own data in kh2-ee memory),
  `FUN_00145000`, `FUN_00145498`, `FUN_00145518`; inputs crossing the clamps.
* Planes: `FUN_001466a0`, `FUN_001469c8`, `FUN_00146970`, `FUN_00146b20`, `FUN_00146bc8`, `FUN_001465c8` (random matrices, boxes
  inside, outside and straddling), `FUN_00164690`, `FUN_00164780` (random triangles and quads with the points near edges).
* Collision object leaves: `FUN_001418a0`, `FUN_00142190`, `FUN_001422b0`, `FUN_00142858` (identity and random rigid matrices, check
  `+0x50`, `+0x90`, `+0xd1`), `FUN_00142a88`.

Stateful vectors (one per query, stub-free): generate a random COCT file in the test (a flat floor, random walls of attributes
0x172 / 0x82 / 0x22 / 0x1f2, a slope, a kind-4 floor, two mesh groups), write it into kh2-ee memory, build the object by calling the
game's own `FUN_00141500(block, file, word)` in the emulator, store it in `DAT_00355934`, and leave the moving-object list empty
(`DAT_005210c0` = 0, so `FUN_00165910` returns 0 without a stub). Then call, with a random collider record (start, delta, radius,
lift, flag word with kinds 0/1/3/6, owner): `FUN_00142760` + `FUN_001425a0` (count and floor index), `FUN_001423d8` (`+0x30`,
`+0x60`, `+0x68`, bits 32..33), `FUN_00164138` (`+0x30`, bit 34, return), `FUN_00143840` (all of `coll +0x00..+0x84`), and the
rays `FUN_00141f08` under each row of the mask table. Compare the port's `kh2_map` functions on the same file bytes. A second stage
adds one moving object through the game's `FUN_001652b8` / `FUN_001653d8` / `FUN_00165250` calls in the emulator (a placed copy of
the same file) to cover `FUN_00165430` and `FUN_001654f8`. The 0x164138 sweep vectors are the gate for 9.8: the same start and
stick delta as tt-run-06 at k150.

### 9.10 Decompile index and gaps (WP-L1-S3)
* Read in full: every function cited in 9.1 to 9.9, plus `FUN_00143840`, `FUN_00143560`, `FUN_001439f8`, `FUN_00143aa0`,
  `FUN_00143b80`, `FUN_00143c28`, `FUN_00164138`, `FUN_001423d8`, `FUN_00142760`, `FUN_00141b30`, `FUN_00141910` (re-read).
* Gaps: `FUN_00141f08` (the ray cast), `FUN_00163f98`, `FUN_001632b8`, `FUN_00165910`, `FUN_00164ce8`, `FUN_00146648`, `FUN_00147300`,
  `FUN_00148440`, `FUN_00165990`, `FUN_00163248` and `FUN_00161ed0` were not re-read in this package (cited from sections 5 and 8 or
  from their callers); the constants `DAT_00378f6c`, `DAT_00378f70`, `DAT_0037953c`, `DAT_00379544`, `DAT_00379548`, `DAT_0037955c`
  were not read (the oracle reads them from the executable); the roof stop of a jump (9.3) and `coll +0x20` for Sora (9.8) are open.
**Status (WP-L1-P10):** read and ported: `FUN_00141f08`, `FUN_00141d90`, `FUN_001623a0`, `FUN_00163f98`, `FUN_001632b8`, `FUN_00163248`, `FUN_00161ed0`, `FUN_00162028`, `FUN_00162678`, `FUN_001625e0`, `FUN_001626f0`, `FUN_00330c80`, `FUN_00141720`, `FUN_00141768`, `FUN_00143560`, `FUN_001434b8`; the data constants (0x378f50..0x378ff8, 0x379500..0x37955c, the level tables 0x355e60 and 0x355e70) are small rule values now in `oct.rs` and `geom.rs`. Still open: the pool functions of the moving objects, `FUN_00165910`'s non-empty path, `FUN_00142858`'s inverse, `FUN_00153540` (floor lighting colour), and what the vector lanes the VU ops leave alone hold (the w lanes of the collider's vectors are not compared and nothing reads them).
