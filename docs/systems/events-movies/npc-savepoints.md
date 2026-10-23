# NPCs, talk, save points and world points

[Contract](contract.md) | [Index](../../README.md)

## 3. NPCs

- **Objects** [V]. `FUN_0016e8c0` creates an object by its `00objentry` type: type 15 (the townspeople `*_RTN`,
  e.g. `N_EX500_RTN`, `N_EX510_RTN`, `N_EX520_RTN` in tt02) -> 0xb40 bytes, `FUN_00189858` (object class 5, vtable
  `0x35ced8`; it holds two path followers `FUN_0018b0d8` at +0xa40 and +0xaa0); type 7 save and world points
  (section 4); type 8 (`F_EX000`) and others the default class `FUN_001a51e8`. Twilight Town spawns 50+
  different type-15 models; the most common are `N_EX510_RTN` (19 entities), `N_EX500_RTN` (16),
  `N_EX590_RTN`, `N_EX580_RTN`, `N_EX520_RTN` (15 each).
- **Scripts** [V]. The townsperson models carry their own BDX (`kh2 bdx N_EX500_RTN`: program `rtn_default`,
  2961 words, entry events 0 and 1 only). They use group 1 hosts and the group 5 library (5.0, 5.2..5.7, 5.11,
  5.16, 5.17, 5.26, 5.27, 5.33, 5.34). The group 5 wrappers read the object's path at +0x508 (`FUN_002ea230`,
  `FUN_002ea080` -> `FUN_002d2328`, `FUN_002d25b0`) and the class-5 helpers (`FUN_0018a8b0`, `FUN_0018af08`,
  `FUN_0018af10`, `FUN_00189e70`).
- **Walk paths** [V for the layout, I for the meaning]: 8-byte header (`u16` id, `u16` point count, `u16` kind 1
  or 2, `u16` 0), then 12 bytes per point (three f32: a position in the game's space). Twilight Town groups
  hold them with type-15 entities (e.g. tt02 `a72_`, ids 0xf3..0x2ff). Which entity follows which path: the
  script asks for it by id through host 1.177 (below, WP-E2); `rtn_default` asks for -1, the group's first path.

**The class-5 runtime and the walk (WP-E2)** [V unless marked]. Read for the port in `crates/kh2-event/src/npc.rs`:

- *Where the townspeople come from.* Any loaded group's type-15 entity (`FUN_00181728` makes every entity of a
  group). In tt02 the `map` program 0 loads none: the kind-6 groups `a72_`, `b72_`, `c72_` (one `N_EX5x0_RTN`
  each, 7 activators, one walk path, 9 return parameters) come from `evt` programs 0x10 and 0x12, `b50_` from
  0xc (`KH2_PROGRAMS=0,0,10` in the app, printed by its area log).
- *Construction* (`FUN_00189858`): two 0x60-byte "attention followers" at +0xa40 and +0xaa0 (`FUN_0018b0d8`
  -> `FUN_0018b0f0` zeroes kind +0, mode +4, target +0xc, +0x20, bytes +0x30 and +0x31), the floats 360, 720, 180,
  60 (`0x37ab48..`) into +0xb10.., flags +0x588 |= 8 and +0x600 bits. Init (vtable slot 2, `FUN_001899d0`): bits 0..6
  of the spawn record's word +0x24 go into the bit array at +0xb04 (host 5.17 tests, 5.27 sets a bit); with bit
  0 set the follower is put in kind 2, mode 0, and its motion (+8) comes from the group's return parameters when
  the group's kind is 6 (`FUN_00199ca0`): the last entry with byte 0 = 0 and byte 3 = 4 names it by byte 1
  through `FUN_0018b1e0` (0, 1, 3 -> 0; 2 -> 0x11f; 4..13 -> value + 0x117).
- *The path* (+0x508). Host 1.177 (`0x1719d0` -> `FUN_00183c30(obj, id, 0)`) looks the id up in the object's
  own group (`FUN_00199cc0`: the path header's u16; a negative id takes the first path) and builds a 0xa0-byte
  record (`FUN_002d3b40`, ctor `FUN_002d2100`): the points as 0x20-byte entries (position, tangent) at +0x80,
  +0x10 = header byte +4 bit 0 (the path loops), +0x20 = 0.5 unless bit 1, the tangents of a cubic spline
  (`FUN_002d32f8`), the place +0x18 (segment index plus fraction) set to the point nearest the object
  (`FUN_002d50c0` -> `FUN_002d4dc8`), direction +0x84 forward, +0x85 "turn around at the ends" (clear only for
  objects with flag 0x588 bit 3 whose bit 2 is set), +0x86 lock. Host 1.178 (`FUN_00183e18`) and 5.3
  (`0x2ea080`) call `FUN_002d25b0(speed, blend, out, path, position, facing)`: the speed is clipped to 50
  (`0x3837b8`), `FUN_002d2a98` takes the point at the place and, while it is within `speed` of the object
  (horizontal distance), moves the place by `speed / segment length * 0.5` (at most 20 times; at an end of a
  non-looping path `FUN_002d2328` flips +0x84 when +0x85 is set, else the walk stops and the facing is answered);
  the answer is the unit vector from the object to that point. 5.2 (`0x2ea230` -> `FUN_002d2328`) flips the
  direction unless locked. 5.16 (`0x2ea768` -> `FUN_002d3148`) picks the direction toward another object. The
  movement itself is the root motion of the walk motion, as for enemies (the script never sets a movement vector).
- *The curve* (WP-E2b, ported in `PathRun`). `FUN_002d3b40` copies the points (position w 1.0, `DAT_003837f8`;
  with bit 3 of the class-5 array every height is the first point's), +0x10 = header byte +4 bit 0 (loop), +0x20 =
  0.5 (`DAT_003837fc`) unless bit 1 (tension 0: tangents of zero, the chords walked with an ease), the lock +0x86 =
  array bit 2 and +0x85 = its opposite; with array bit 1 clear `FUN_002d5398` puts the object at a random place
  (`FUN_001063b8` of the segment count) walking a random way (`FUN_00106308(2)`), else `FUN_002d50c0` the nearest
  place. Tangents (`FUN_002d32f8`): fewer than three points: the chord, loop flag cleared; inner points (all
  points of a loop) (next - previous) * tension, w = the index; the ends of an open path: the quadratic Bezier
  through the end and its two neighbours, passing the middle at the share f = |P1-P0| / (|P1-P0| + |P2-P1|)
  (control M = (P1 - P0 g^2 - P2 f^2) / 2fg, g = 1 - f), its end derivative scaled so that its derivative at
  the middle has the inner rule's length (`DAT_003837e8` 2, `DAT_003837f0` -2, `DAT_003837f4` 4). The curve is
  the cubic Hermite (`FUN_002d4298` / `FUN_002d2460`, weights 2s^3-3s^2+1, -2s^3+3s^2, s^3-2s^2+s, s^3-s^2).
  `FUN_002d2a98` (record +0x87 clear, stop index +0x8c = -1 from the ctor `FUN_002d2100`): the object is first
  projected onto the curve (`FUN_002d51e0`: for more than two points the local search, `FUN_002d4ad8` on a loop /
  `FUN_002d4800` on an open path: the rest of the current segment ahead and the next one, each by 33 halvings
  `FUN_002d4688` against the chords `FUN_002d4040`, accepted below `speed * 10`, `DAT_003837d0`; else the
  global `FUN_002d4dc8`: the nearest chord, refined by halvings strictly inside (1e-4 .. 0.9999)); if the
  projection is as far from the object as the old place within 1e-4 (`DAT_003837d4`) the old place stays. The
  walking parameter (+0x1c) then steps `speed / chord * 0.5` up to 20 times until the curve point is more than
  `speed` (horizontally) from the projection; past a segment end it takes the next one (wrapping on a loop), at
  an open end it turns around (+0x85, one more pass) or stops. `FUN_002d25b0`: the direction from the object
  to that point when it is more than `speed * 10` away (`DAT_003837c0`), else (blend 1.0, `DAT_00383e80` for
  5.3, `DAT_003837c4` for 1.178) from the projection. The tt02 paths are loops of 11 to 14 points with tension
  0.5 (`tt02_townspeople_name_each_other_and_carry_their_parts` walks each round).
- *The script* (`rtn_default`, the same program in every `N_EX5x0_RTN` read). Init registers thirteen modes
  (`near_wait`, `near`, `rtn_freeze`, `before_check`, `after_talk`, `encount_player`, `look`, `rtn_avoid`,
  `rtn_action`, `path_move`, `talk`, `static_idle`, `fix_idle`), sets the walk speed 5.0 in its context (+0x44)
  and hands nine floats (360, 720, 180 three times) to host 5.33 (`FUN_00189990`: object +0xb10..). Main: for
  four object ids (1075, 1872, 2162, 2313/2314) it binds a companion (1.114, 1.18 link, 5.26 = +0xb08); with
  bit 5 of the array it clears the dash mover (1.58); with bit 0 it goes to `fix_idle` (plays the follower's
  motion from 5.5), else 1.177(-1) and `path_move` when a path exists, `static_idle` otherwise. `path_move`:
  motion 129 (a turn) first; each tick 5.3 gives the direction, 0.21 the angle to the facing; walking (state 1)
  it switches to 129 when the angle passes 90 degrees, else to 1 (the walk) when within 3.6 degrees;
  `turn_toward` by 5 degrees per tick; 5.0 (the follower's test) hands a follower of kind 4 / 2 / 5 / 3 / 1 / 6
  / 7 to `encount_player` / `rtn_action` / `rtn_action` / `rtn_avoid` / a direction flip / `fix_idle` (and
  bit 0 set) / `rtn_freeze`. `static_idle` waits for the player within 200 cm then queues `look` (head toward
  the player, 1.174, and turning with 129 while 1.172 says he is outside the 15 degree cone, `0x37b950`).
  Motion 129 is started with `play_motion_at` at the walk's frame when the walk plays (1.69, 1.61); from 129
  to the walk the script plays 0 and queues 1 (1.13).
- *What the class does on its own* (vtable slot 3, `FUN_00189ae8`, every frame): with bit 0 clear it runs
  `FUN_0018a130`, the native attention logic: a region test of the player against the object's parts
  (`FUN_0018aa08`, part 5) starts a kind-4 follower with a timer from +0xb10 / +0xb14 (`FUN_00106408`), a
  partner townsperson whose return entry names this one (`FUN_0018a598`, `FUN_0018a7a0`) a kind-5 pair
  (`FUN_0018a828`, +0xb1c / +0xb20), else the region the object stands in (`FUN_00199d68` over the kind-6
  group's activators, byte +0x6c + 2 of the region keyed in the return entries) picks a weighted return entry
  (`FUN_0018b308`: byte 2 is the weight of a hundred, byte 1 the motion, byte 3 the target mode: 1 an object
  by id at +4, 2 the player, 3 a point at +4..+0xc) into a kind-6 or kind-2 follower. 5.0 (`FUN_0018a8b0` ->
  `FUN_0018b148`) answers true when the follower is forced (+0x31) or its region (+0x20) test (`FUN_001d8c18`)
  passes against +0x28 / +0x2c, and sets +0x30; 5.11 (`FUN_0018b250`) answers 0 while a done follower's timer
  (+0x50, `FUN_0018b228`) runs and the kind-4 part test or the kind-5 partner (motion 0x32, `+0xa4c` back-link)
  holds; 5.4 (`FUN_0018ae08`) copies the follower to +0xaa0, resets it and loads the +0xb00 timer from +0xb18 /
  +0xb24 / +0xb30 by kind; 5.5 / 5.7 read the follower's words 0..2 and its byte +0x24; 5.6 (`FUN_0018af10`) the
  unit direction to its target. 5.34 (`FUN_0016bbe8`) is the object's part-0x11 point (`FUN_001894d0`) or its
  lock-on point.
- *The attention logic, ported (WP-E2b, `class_tick` in `npc.rs`).* `FUN_0018a130` per frame while array bit
  0 is clear (not linked, `FUN_001beb00`): the follower timer (+0x50, `FUN_0018b228`) and +0xb00 count down by
  1 (`DAT_00356f40`). Unless the follower is of kind 4, or the previous one (+0xaa0) was and +0xb00 runs: the
  player's type-1 body meeting the object's first type-5 part (`FUN_0018aa08`: `FUN_0016b558(obj, 5)`,
  `FUN_001894d0`, the player `DAT_0035c998`'s type 1, `FUN_00189208`) keeps the follower as the previous one
  (when it has a kind) and starts a forced kind-4 follower, mode 2 (the player), flag 0, timer
  `FUN_00106408(+0xb10, +0xb14)` (360..720 frames from `rtn_default`'s block). Unless the follower is of kind 4 or
  5 (or the previous was 5 and +0xb00 runs): `FUN_0018a598` finds a partner: `FUN_0018a918(obj, 0)` takes the
  first other object (spawn-record halfword +0x1e different, `FUN_0016b928`) whose first type-0 part meets this
  one's; it must have +0x588 bit 3 and a follower not done, and both objects' return entries with byte 0 = 0
  must name the other by objentry id at +4 (`FUN_0018a7a0`, `FUN_0016b078`: 0x236 is the player, else the
  first object of that objentry). Then one timer `FUN_00106408(+0xb1c, +0xb20)` and `FUN_0018a828` on both:
  forced kind 5, mode 1 toward the other, the entry's motion (`FUN_0018b1e0` of byte 1), flag 1 for this one
  and for the partner when the facings (`FUN_0016a890`, +0x550) point away from each other (dot < 0). Else,
  once +0xb00 ran out (it is zeroed), `FUN_0018a4b0`: the region the object stands in (its position 10 cm higher,
  `DAT_0037ab88`; `FUN_00199d68`: kind-6 group only, the first of the group's volumes in record order whose
  record halfword +0x2c is 0 and that contains the point, `FUN_00198750` makes one per activator) is compared
  with +0xa30; on a change, unless the follower is done, it is reset and, inside a region, `FUN_0018b308(+0xb28,
  +0xb2c, ...)` draws: a number below 100 (`FUN_00106308`) against the summed weights (byte 2) of the return
  entries whose byte 0 equals the volume record's +2 and byte 1 is not 3; the last entry with byte 1 = 3 gives
  the flag (+0x24) with the chance of its byte 2. The chosen entry makes kind 6 (byte 1 = 1) or kind 2, motion
  `FUN_0018b1e0(byte 1)`, target by byte 3 (1 object by id, 2 the player, 3 the point at +4..+0xc, else none),
  region +0x20, threshold +0x28 = `FUN_001063b8(1.0)` (`DAT_0037abd4`), start +0x2c = `FUN_001d8c18(volume,
  position)` = (x' + sx) / 2sx in the volume's frame (sx at volume +0x50 = record +0x10), timer
  `FUN_00106408(+0xb28, +0xb2c)`. 5.0 (`FUN_0018b148`) passes when forced, or across the region past the
  threshold (at or below it when the start was >= 0.5, `DAT_0037ab98`, else at or above); 5.11 (`FUN_0018b250`)
  holds (0) while done with the timer running and: kind 4 the parts still meet, kind 5 the partner exists,
  points back (+0xa4c) and does not play motion 0x32 (+0x154), any other kind always; 5.6 (`FUN_0018af10`):
  live (forced or a region) with a mode: the target at height 0 (mode 3: the point with its own height) minus
  the object at height 0, normalized, else the facing. 5.4 (`FUN_0018ae08`) reloads +0xb00 always from +0xb30:
  it reads the kind after `FUN_0018b0f0` cleared it (instructions 0x18aeb0 `jal 0x18b0f0`, 0x18aeb8 `lw 0xa40`).
  tt02 data: the three townspeople (0x51d, 0x51e, 0x51f) name each other in byte-1 = 2 entries (motion 0x11f,
  `rtn_action`); their volumes are keyed 0, 1, 2: key 1 stops 30 of 100 (idle motion, flag 30), key 2 40 idle
  and 20 looking at the player (flag 50). Parts: type 0 a sphere of 90 lifted 115, type 5 a sphere of 200 at
  200 up, both on the object's own frame; Sora's type 1 an ellipsoid 35 / 55 lifted 85. Log: `KH2_NPCLOG=1`
  prints `attention: the player meets the talk part`, `attention: paired with`, `attention: region`.
  [approx] the game's one global `rand()` is a per-object generator; the object list searched for partners and
  ids holds the townspeople and the player only; the class pass runs before the script's tick and not before
  the script's first tick [inferred order]; the parts' enable masks are not applied.
- **1.172 / 1.173 cone** [WP-NPC1]: `npc::in_cone` ports `FUN_001af650` (see docs/systems/script-host/contract.md): without a
  type-0x14 part the flat 15 degree test; with one (`N_EX510_RTN`, `N_EX520_RTN` and the other `*_RTN` models;
  only `N_EX500_RTN` has none) the 3D direction from the part's point against its cone of 50 degrees. The part's
  point is the head bone's pose of the previous tick [inferred]. The head look-at controller (hosts 1.174 / 1.176) is ported, see "Head look-at" below, with 1.175 (a point)
  and the pose code's own joint turn left (**not ported, GAP**). 1.13 on the object starts
  the queued motion when the playing one ends; 5.34 answers the position (the tt models have no part 0x11; the
  lock-on fallback `FUN_0016b6e0` reads a type-3 part on a bone). The native `FUN_00189ae8` also raises +0x108
  bit 6 after five frames of spawn kind 2 without `FUN_00190668` (not read further); `FUN_00189ff0` (the
  class's reaction to a talk: it ends the look-at, sets the previous follower to kind 4 and +0xb00 to +0xb18 +
  `DAT_0037ab84`) is for E3.
- **Talk** [V; closed by WP-E3]. A type-5 reaction part with id 0x83 offers the command in the
  halfword +0x2c of the object's spawn record (docs/systems/drive-limits-summons/contract.md: 31 "talk" for most townspeople;
  `twilight_town_talk_programs` counts 206 townspeople with 31, one with 30, others 209..213, 293,
  555..559, 633). Command 31 has `Execute` 31 (0x1f). Earlier notes said every reaction command reaches
  the target's script as event 0xb; **that is not so for the talk**: Sora's class handler
  `FUN_001cf030` (decompiled for this WP: its own cases are Execute 0x48, 4, 5, 0x51, 0x49, 0x13c)
  hands everything else to `FUN_00177b18`, which has its own cases before the generic path
  `FUN_00177c20`: 0x1e (`FUN_00177d28` + `FUN_001a54a8`), **0x1f (`FUN_00177d28` + `FUN_001a56c0`)**,
  0x20 (`FUN_001a6db8`), 0x37 (`FUN_001784b8`, section 4) and 0x7f (`FUN_00197b70(record +6, 0)`). So
  the talk bumps no use counter, applies no CmdAction and sends no event 0xb (which is why
  `rtn_default` has no handler for it).
  - `FUN_00177d28` clears the player's vectors +0x20, +0x10 and his target record's +0x9c0.
    `FUN_001a56c0`: `FUN_001bef48(0xc)` (HUD list record 0xc), `FUN_001b7ce8(0)` (bit 0 of
    0x1d5d7b8: the player's control is locked; `FUN_001b7d50` tests the bit of the object's objentry
    type, read through `FUN_00176a18` by `FUN_001794f0`), then `FUN_001a53f8(target + 0x500, player,
    target record, task FUN_001a5728, callback FUN_001a5a60)`.
  - **The talk table** (`kh2_ard::libretto`). `FUN_0016b870` copies the spawn record's halfword
    +0x28 into the object's +0x500 (when not 0): the talk id. `FUN_001b6408` (called by `FUN_00168ba8`
    at area load) loads `libretto-%s.bar` with the world prefix (`FUN_001b6398`, format at 0x35f1d4)
    and keeps its first type-2 entry: a u32 count at +4, records of 8 bytes from +8 (`u16 id, u16 n,
    u32 offset`), searched by binary search (`FUN_001b64b0` -> `FUN_0030ff40`, compare
    `FUN_00332dc8` = key - id). `FUN_001a53f8` takes a reader from a pool of 8 (`FUN_001b6740`,
    `FUN_001b6510`) and keeps it at +0x504; the program is `u16 op, u16 count, count u32` commands
    ended by op 0 (`FUN_001d8d18`); `FUN_001b65e0` runs 2 wait (`FUN_00102460`), 3 fire event
    (`FUN_00197b70(w, 0)`), 4 stop (the place is kept: the next talk goes on after it,
    `FUN_001b6578` / `FUN_001a53b8`), 5..8 fades (`FUN_0014a7c8` / `FUN_0014a7f0`, time x
    `DAT_0037bb08` 0.5); other ops go to the callback: `FUN_001a5a60` op 1 = a message window of class
    `FUN_001aba58` (the same values as `FUN_001ab9e0`: font 3, colour 5, (256, 336), frame kind 0, the
    speech tail `FUN_001aa628(1, 1, 0)`) with the word as message id, registered (`FUN_001aad78`),
    waited for until its state (+0x70) is 3, then closed through its +0x20. The wait also ends when
    `FUN_001a5b18(10)` answers 0 (`FUN_0019abb0` of the player with command row 10) [GAP: which
    player state that is; not ported]. On the disc (`twilight_town_talk_programs`): 305 talk ids,
    ops used 1 (333) and 3 (4, event 101 = 0x65 after a message, tt11/tt12/tt36 `b51m`); nine tt
    entities name talk id 0x4a6 or 0x402, which have no record (they talk to nothing). tt02's four
    townspeople (`a72_`, `b72_`, `c72_`, `b50_`) each say one message of two pages.
  - **The task** `FUN_001a5728`: player record +0xb40 +0x2c bit 0 and 0x35d268 bit 8 are set, the
    target's +0x108 bit 2 (its only reader found is `FUN_0019a530`); the player faces the target
    (`FUN_0016a948` with `FUN_0018efa0` of the target record minus his position); the talk camera
    (`FUN_00159688`: camera kind 4 with the record, `FUN_00140d08` +0x40 = 4; second argument 1, or 0
    for a class-5 target whose array has bit 0x10 or 0x40, `FUN_00189e90`) [ported in WP-E8: docs/systems/player-camera/camera-event.md
    "Talk camera"];
    the three party members turn their heads (`FUN_001abf40` on +0x82c) [not ported]; the target faces
    the player (`FUN_0016a828`, or for class-5 objects `FUN_00189f40`: unless array bit 4, and its
    companion +0xb08 plays motion 0x32) and plays motion 0x32 (`FUN_0017e220`); `FUN_00192c50(target,
    0x21)` (on the object's +0x670 list, meaning GAP). After the program, unless an event runs
    (`FUN_00196418`): the facing comes back, `FUN_00192c68`, the class-5 end `FUN_00189ff0` (motion
    = the follower's +0xa48 with array bit 0, else 0; with bit 0 clear the head look-at ends, the
    previous follower +0xaa0 takes kind 4, the follower is reset and +0xb00 = +0xb18 + 60
    (`DAT_0037ab84`); the script goes to mode "after_talk" (`FUN_001870d8` / `FUN_00186db0`, string
    0x37ab78) when it has one), the camera comes back (`FUN_00159768`), the heads, the lock bit
    (`FUN_001b7d18`) and the flags are cleared, `FUN_001befc0(0xc)`. Command 30 is **Examine** (message
    0x81eb; WP-SH1 corrects "the shop people"): `FUN_001a54a8` runs the same table with the task `FUN_001a5510`
    (the player faces the target; no talk camera, no motion 0x32) and callback `FUN_001a5608` (window class
    `FUN_001ab7d8`, wait `FUN_001a5b18(0xe)`) around the script's "before_check" mode (`FUN_00189eb8`, string
    0x37ab68) [ported in WP-SH1: `npc::talk_step`, window class 2]. The shops are the "Shop" rows (Execute 0x7f,
    `FUN_00177b18` case 0x7f -> `FUN_00197b70`): docs/systems/ui/menus-struggle-result.md section 21.
  - **The window** (docs/systems/ui/hud-banners-windows.md "Message windows"). The base update `FUN_001ab170`: state 1 waits
    for the page (`FUN_001a8b48`) and starts the prompt (`FUN_001aa3a0`), state 2 reads the pad
    (`FUN_001aa430`: logical bits 0x40c, sound 0x18 `FUN_002c14a8`, then `FUN_001aacd8`: the next page
    when `FUN_001a8c78` -> `FUN_00190fa8` finds text after a command 0x10 (back to state 1), else the
    window's method +0 (`FUN_001ab328` for this class: prompt group 0x42, state 3)); the owner's +0x20
    (`FUN_001ab358` -> `FUN_001aacb0`: state 4, `FUN_00197b70(0x22, +0x80)`), state 4 the tail's
    closing group (`FUN_001aa680`: group + 1), 5 waits for it and starts the frame's closing group, 6
    waits, 7 gone. The tail is drawn from state 1 on at (x + w x -0.5, y - h / 2) (`FUN_001ab370` ->
    `FUN_001aa6b8`, `DAT_0035ea68` = {-0.5, -0.15, 0.15, 0.5}).

### Head look-at (WP-HL1)

What turns the head: the controller at object +0x4a0 (0x60 bytes), updated every tick by `FUN_001aea28` and read
by the motion code (`FUN_0017e6c0`, `FUN_0017e928`: `FUN_001aead0(obj + 0x4a0)` is the last argument of the
pose call `FUN_0011c2a0` / `FUN_0011c460`). Fields: +4 mode (0 off, 1 toward a point, 2 toward an object, 3
toward a direction), +8 low bit "needs the object's +0x160 bit 0x4000000" (set to 1 by the script hosts),
+0xc the joint, +0x10 / +0x14 the weight and its step per tick (`FUN_001abf40` sets, `FUN_001abf78`
advances: clamps at 0 and 1), +0x20 the aim point (w = the eased weight), +0x30 the target object, +0x40
the last look direction, +0x50 the head's yaw against the body.

Who uses it: the objects whose model has a collision record of type 0x14 (the BAR entry 23 table). Of the
disc's `obj/*.mdlx` listed in `local/kh2idx.txt`, 110 have one, all `*_RTN` townspeople; `N_EX510_RTN` and
`N_EX520_RTN` of tt02 have it, `N_EX500_RTN` has not, Sora's table has none. The record's halfword +6 is the
head bone (`N_EX510_RTN` 128, `N_EX520_RTN` 68, `N_EX680_TT_MAN_A_RTN` 62), its offset (+8..+0xc) a point in
the bone's frame (7 or 9 along x), and its two radii (+0x10, +0x12; 50 and 20 for most) are the yaw and
pitch limits in degrees (`DAT_0037b938` = pi / 180). `FUN_001aece8` (1.174) does nothing for an object
without the record. In the townspeople's scripts (`rtn_default`, mode `encount_player`; 683 calls in the
disc's scripts, `KH2_BDXLOG=1`) 1.174 is called with 20 frames when the kind-4 follower starts and 1.176
with 20 frames when it ends; 1.175 is in none of them. `FUN_00189dc8` (a class-5 companion's mode 1) and
`FUN_00228778` (mode 3 and the end, from the event player `FUN_00223a88`) are the other writers, not ported.

The rule (host 1.174 = `FUN_001aece8`, mode 2): the weight goes from 0 to 1 over the frames (`DAT_0037b930`
= 1; at once for frames of 0 or less) and the head's yaw against the body (+0x50) starts at 0. Every tick
(`FUN_001aea28`): advance the weight; a weight of 0 ends the mode; if the target object is gone
(`FUN_0019a3f8`) the mode ends; else `FUN_001aee90`:

1. The head point is the record's centre (`FUN_001894d0`: the bone's matrix times the offset, y lifted by
   +0xe); the target point is `FUN_0016bbe8(target)`: the target's type-0x11 record's centre, else its
   lock-on point. The direction to it gives a target yaw (horizontal) and a target pitch.
2. Yaw: the head's current yaw is the body's facing turned by +0x50. If the target yaw is more than 5
   degrees away (`DAT_0037b93c`) the head turns 5 degrees toward it (sign from the cross product's y), else
   it takes the target yaw whole. Then, if that is more than the record's yaw limit from the body's facing,
   it is put at the limit on that side.
3. Pitch: the last look direction's pitch (+0x40) against the target's: only when they differ by more than
   3 degrees (`DAT_0037b944`) does the pitch move, by exactly 3 degrees toward it (else it stays); then it
   is clamped to the pitch limit of the body's pitch (level for an upright object).
4. The new look direction is stored at +0x40, +0x50 = the signed angle between the body's facing and the
   new horizontal direction, and +0x20 = the head point plus the direction times the distance, w = 1
   (`DAT_0037b94c`).

The pose code gets +0x20 and the weight eased by `FUN_001aead0`: `(1 - cos(pi w)) / 2` (`DAT_0037b920` = pi,
`DAT_0037b924` = 1, `DAT_0037b928` = 0.5; `FUN_003000d8` is read as the cosine, which is what a 0 to 1
ease needs [inferred]). Host 1.176 (`FUN_001aec50`) tweens the weight from where it is to 0 over its frames;
the mode ends when it arrives.

In the app (`npc::look`, `skel::JointTurn`): `Look` holds mode, weight, step, +0x50 and the pitch;
`Look::tick` is steps 2 and 3 above in the app's space (the game's space is the app's turned half a turn
about x, which keeps the turning direction, so no sign mapping is needed); `npc_update` calls it after the
townsperson's move with the head point from the last pose (`pose[joint].transform_point3(offset)` in the
object's frame, as the talk camera reads bone records) and Sora's type-0x11 record (joint 0x8c, offset
(3, 0, 0)) read the same way from his own last pose. The result is a rotation (yaw +0x50, then pitch) scaled
from nothing by the eased weight, handed to the animator, which rotates the head bone and everything below
it about the head bone's origin on top of the motion's pose (`skel::turned_pose`; the pose itself is not
changed, so cross-fades start from the motion's pose). Log: `KH2_NPCLOG=1` prints `head look-at Start(..)` /
`Release(..)` and once a second the yaw, pitch and weight.

Harness: tt02 with `KH2_PROGRAMS=0,0,10 KH2_NPC_WATCH=2 KH2_NPC_WATCH_DIST=150 KH2_NO_ENEMIES=1`, shot at
9 s: the townsperson stands behind Sora with only his head showing; before the port his head follows the
idle motion (turned away, eyes shut), after it the head faces Sora's side (the 50 degree yaw limit and the
20 degree pitch limit, the target being 20 cm from him).

Front view [WP-NPC1]: tt02 `KH2_PROGRAMS=0,0,10 KH2_NPC_WATCH=2 KH2_NPC_WATCH_DIST=300 KH2_NPCLOG=1
KH2_NO_ENEMIES=1 KH2_SHOT_AT=9` (the camera behind Sora, the townsperson `N_EX510_RTN#2` facing it), once plain and once
with `KH2_NO_HEADLOOK=1` (a harness aid that leaves the head on the motion's pose). Both shots looked at, head
cropped and enlarged: without the controller the head looks away to the left of the picture; with it the face turns
to the camera (eyes and both cheeks toward it), the headband stays level and the neck stays joined to the collar. No
flip or twist. (Sora's own head covers part of the face; the pitch limit was reached in the 120 cm talk run.)

Why a tt06 run (`KH2_NPC_WATCH=2 KH2_NPC_WATCH_DIST=60`) logs no look-at lines [WP-NPC1]: townsperson 2 there is
`N_EX650_TT_ITEM_RTN` (group `k01_`, spawn flags 0x1). Its script does contain hosts 1.174 / 1.176 / 1.172 (0x4b3, 0x51f,
0x583, 0x5d6), but with spawn flag bit 0 set the class pass (`FUN_00189ae8`, which starts the kind-4 follower
when the player meets the talk part) is skipped and the follower stays in kind 2 with no walk path, the script's
mode stays `-`, so no code that calls 1.174 ever runs. The other five tt06 townspeople (spawn flags 0 with a walk path, or
`N_EX690_TT_WOMAN_A_RTN#5` with flags 1) were far from Sora. Not a missing port. Whether the ITEM model has a type-0x14
record was not checked (the 1.174 host never ran).

[inferred] and not done: (a) how the pose code behind `FUN_0011c2a0` (a virtual of the model class, vtable
+0xc of the pose object) turns the joint toward the aim point was not traced; the app applies the yaw and
pitch as a rotation of the head bone about its own origin and assumes the animation's head looks along the
body's facing. (b) The enable test of `FUN_001ae9e8` (bit 0 of +8 set by the hosts: then the object's +0x160
bit 0x4000000 must be set; its writer was not found) is not read: the controller runs while the object
exists. (c) The target of 1.174 is taken to be the player: its first argument arrives as 0x20001010 (not the
object's own handle) and the block the second one points at is the object's own address plus 0x40; the
scripts' `encount_player` passes the object `R03eb` returns for the player. (d) +0x40's initial value is
the target's facing vector (`FUN_0016a890`); the app starts level. (e) Modes 1 and 3 (`FUN_001aec78`,
`FUN_001aeda8`), 1.175, the class-5 companion writers and the party members' head turns of the talk
(`FUN_001abf40` on +0x82c) are not ported. (f) A target straight above or below the head has no horizontal
direction: the yaw stays (the game divides by a zero length). Tests: `npc::look::tests::*`,
`npc::tests::the_head_record_names_the_head_bone_and_the_limits`,
`skel::tests::a_joint_turn_rotates_the_subtree_about_the_joint`.

## 4. Save points and world points

- **Objects** [V]. `SAVE_POINT` (objentry 570) and `WORLD_POINT` (569) have type 7 -> `FUN_00194ab8` (0xa00 bytes,
  vtable `0x35e300`): sets object flag +0x588 |= 0x1000, +0x9fd = 1, starts its own effect element 1 (`FUN_0016a570(obj, 1,
  ..)`, handle +0x9f4, ended by `FUN_00194b68`). For object 569 (the world point) the pair (world, area) is looked
  up in `00worldpoint.bin` (`FUN_00194f68`: 4-byte records world, area, entrance, -; at most 0x36; the file is 216
  bytes, 0x36 records) and that point's bit is set at `0x33f910` (`FUN_00194fa8`; `FUN_00195010` answers 0 unseen,
  1 seen, 2 seen and locked). In tt02 they stand in `m_20` (world point) and `m_21` (save point), both at the same
  place; with the programs (0, 0, 0) the app loads `m_20` only (run below; tt02 is record 47). The object files hold no mesh and no motion
  set: one collision record (type 5, id 55, shape 1 = a cylinder of radius 50 and half height 50, lifted 50) and
  the effect file `obj/<name>.apdx` (elements 0, 1, 2) [V, `savepoint::tests::the_points_on_the_disc`].
- **The offer** [V]. The part offers command 55 like any object part (`FUN_0019cb70`). The cylinder against the
  player's ellipsoid is `FUN_00188fd0` (cylinder pairs `FUN_001890d8`; dispatch `FUN_00189208`; ported in
  `savepoint::overlaps`, the other pairs stay `reaction_parts::overlaps`). Its status (`FUN_001ba2c0` case 0x37):
  5 while the word 0x35fec0 has any bit (`FUN_001c6c08`: bit 1 the drive form start `FUN_001b44c8`, bit 2 a summon
  `FUN_001b4170`, bit 3 `FUN_001c6578`), then the flags and form mask as for every command.
- **On approach** [V]. The player's reaction manager `FUN_0019c0f0`: while the offered command (+0x26) is 55 and
  the offer status is below 4, `FUN_00194c08(the point)` runs every frame over the four party slots: a member
  whose object is not in the world (status block +0x24c = 0) gets its MP refilled (`FUN_001a0240`) and its maximum
  HP added (`FUN_001a0100`); a member in the world that is full (HP at maximum, no MP charge `FUN_001a05d0`, MP at
  maximum) is left; else it gains **1 HP** (`FUN_001a0140` -> method +0x78 = `FUN_001792f0` -> `FUN_00176c98` ->
  `FUN_00186238`) and **1 MP** (`FUN_001a0210` -> `FUN_00186368` -> `FUN_001a0180`; during a charge the charge's
  time is shortened by the factor at `DAT_00360130` +0x7c instead, `FUN_001a0540`) and +0x9fc is set; the first
  call also sets +0x9fc (+0x9fd). The point's update `FUN_00194b98` starts effect element 2 (handle +0x9f8) when
  +0x9fc is set and none runs, and ends a running one (`FUN_001d9f20`) when it is clear; +0x9fc is cleared every
  frame. So the heal is gradual (60 HP a second) while the prompt is shown, not on use, and the heal effect plays
  only while something is restored. (Earlier notes read "restored to their maximum": corrected.)
- **On use** [V]. Command 55 (`Execute` 0x37, message 34054) does not reach the point's script: `FUN_00177b18`
  case 0x37 = `FUN_001784b8(player, point)`: party reset kind 7 (`FUN_001abfe8`), every member's method +0x88
  (`FUN_00176758`), byte 0x33e86e = the low byte of the point's spawn word +0x20 (`FUN_0016b940(point, 1)` reads
  +0x820, copied by `FUN_0016b870`), bit 0 of 0x33febc = the player's objentry kind is 0xe (`FUN_001699a8`), then
  `FUN_0024f0b8(1, 0)` for the world point (objentry 0x239) or `FUN_0024f0b8(1, 2)` for the save point: **menu kind
  1** (scene 3, init `FUN_00255598`, which sets the three message ids 0x9fcf..0x9fd1 or a list from 0x380cd0 via
  `FUN_00254bf0`, and starts the task 0x2556f0), not the field save menu kind 7. **Kind 1 is the points' save
  screen** [V, WP-E7, read from the instructions of 0x2556f0 and the decompiled callees; docs/systems/ui/menus-save.md 18.5]: the
  same memory card driver and `save.2ld` groups as the file screens of kind 2, with a top list of "Save" (0x8a7e)
  and "World Map" (0x8a80); the mode word (0x36cd28, `FUN_0024f498`) 2 drops the World Map row, so the save point
  shows only "Save" and the world point both. `menu_save.rs` serves it (`SaveMenu::request_point`).

