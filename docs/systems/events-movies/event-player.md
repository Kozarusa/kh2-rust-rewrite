# Event player

[Contract](contract.md) | [Index](../../README.md)

## 6. The event player

- **Where events live** [V]. In the area file, as BAR entries of type 0x16 named by number (`102`, `117`, ...;
  `FUN_00180a28` finds one by tag). Twilight Town has 190. Animation files are loaded by name: `anm/` + world +
  `/` + model + `/` + name + `.anb` (`FUN_0022cbb0`, strings at `0x37f208`, `0x37f210`, `0x37f218`); also
  `event/layout/%s/%s.2ld`, `movie/%s/%s.pss` (`FUN_00225860`, event opcode 0x44), `voice/%s/event/` + name + `.vag`
  (`FUN_0022bff8`), `event/effect/` + `.pax` [V for the strings and their users].
- **Format** [V]. A list of commands `u16 size (bytes, header included), u16 opcode, operands`, ended by a size of 0
  (`kh2_ard::parse_event`; every tt event parses to its last two bytes). Command 0 carries the memory size at +4
  and the event's name at +0xa (`FUN_00223910`: `tt_event_102`). Opcodes used in tt (hex counts): 0 be, 1 50c,
  2 f57, 3 be, 6 56f, 8 be, 9 496, a 168, b 238, c 18, d 239, e 6a, f be, 10 be, 12 129, 13 566, 14 1b4, 15 8f0,
  16 1a5, 17 73, 18 110, 19 790, 1a 721, 1b 2e, 1d 36, 1e 36, 1f 33, 20 6, 22 88, 23 74, 24 1ea, 27 12, 28 6, 29 bc,
  2a b4, 2b 41d, 2c 16f1, 2d 1d1, 2e c0, 2f 31, 30 31, 35 7, 36 6b, 3a e, 3d 7, 3f 42, 40 2, 42 12, 44 5, 47 70,
  49 10, 4b 8, 4d 6, 4f a, 53 e.
- **Two readers** [V]. `FUN_002245d0` walks the list once to set up (cases 1, 8, 0xf, 0x10, 0x13 (camera: sets
  `DAT_0036bd21` = 1, `FUN_0022b4f8`), 0x1d, 0x1e, 0x23, 0x24, 0x2a, 0x2e, 0x2f, 0x32, 0x44, 0x46, 0x4c, 0x4e); at
  the end it loads (`FUN_00222a88`), and sets the ready flag `0x36bcf4`. The thread `FUN_00223a88` (started by
  `FUN_002248f0`) waits for ready, then runs the timeline commands (cases 2, 6, 9..0xe, 0x12, 0x15..0x1c, 0x1f,
  0x20..0x22, 0x27..0x29, 0x2b..0x2e, 0x30, 0x31, 0x33..0x37, 0x3a..0x3d, 0x3f..0x45, 0x47..0x4b, 0x4d, 0x4f..0x53).
  A timeline command's first operand is a frame: `FUN_00224a90(frame)` yields until the event's frame counter
  `DAT_0036bcf0` reaches it, and the command is dropped when the event left the play state or is being skipped
  (`FUN_00222630`).
- **Per frame** [V]. `FUN_002233d8`: when the event has a camera, `FUN_0015c8f8(DAT_0036bcdc)` (the override of
  docs/camera.md); pad/fade handling; `FUN_00224fa8` (frame step); `FUN_001963e0`. The camera per frame is
  `FUN_0022b0f8` (called from the setup reader's frame path at `0x22481c`): it counts the entries of the
  128-entry table `DAT_0036bbb4` (`id, u16 start, u16 end`) active at the current frame and feeds the track to
  `FUN_0015ce00`. Opcode 6 commands in tt events are `u16 index, u16 start frame, u16 end frame, u16 0`, which
  matches that table [I: the camera cuts]. The camera keys themselves (opcode 0x13 lists of counts) are not
  decoded [GAP: `FUN_0022b4f8`, `FUN_0022b0f8` read].
- **Actors** [V for the layout seen, I for the meaning]: opcode 1 = `u16 objentry id, u16 actor index, name`
  (`p_ex110`, `h_ex510`, ... in tt02 event 102); opcode 0x24 lists the `.anb` motions per actor
  (`anm_tt/...`); opcode 2 places an actor (index, position floats). Motion playback: `FUN_0022cc88` builds the
  `.anb` name; the rest not read.
- **Message lines** [V]. Opcode 0x2d (`FUN_002275a8`): +4 frame, +6 key, +8 message id, +0xa action (0 open,
  1 close, 2 wait until the window reaches state 3), +0xc window kind (0: class 0x1ab6a8, 1: class 0x1ab8e0;
  docs/hud.md). [V, WP-E6b] Action 1 finds the slot of (key +6, message +8) among the 32, calls the window's
  close method (vtable +0x20) and clears the slot, so the wait that follows finds nothing and returns at once
  (close without waiting). Action 2 finds the slot and, unless the event is skipped (`FUN_002225e0`), sets
  `DAT_0036b9c9` (`FUN_00222528`) and yields until the window's state (+0x70) is 3, then clears it
  (`FUN_00222538`); while `DAT_0036b9c9` is set `FUN_00222588` answers 1 and `FUN_00222728` does not advance the
  counter: the event's frame stands still until the window waits for the button. Opcode 0x15 (`FUN_00227478`): +4 frame, +6 key, +8 message id, +0xa 0 open / else close, the
  subtitle line class 0x1aae60. Up to 32 open windows at `0x1dc00e0` keyed by (key, message id). In tt02 event 102
  the 0x15 lines open and close ids 0x3376..0x337b.
- **Start and end** [V]. A trigger with an event tag calls `FUN_00196428`: a 0xb0-byte event object
  (`FUN_00195898`: kind row from the argument, `FUN_001d1128`; bit 7 of the argument -> `FUN_00138708(1)`), the
  jump record copied into it, `FUN_00195ae0` starts the player (`FUN_002232c8`), stores the object at
  `DAT_0035e3a4` (host 1.34 `event_active` reads it), hides HUD parts (`FUN_001bef48(0xc)`, `FUN_0014c748(4)`).
  `FUN_00195c08` steps the object's own states (0..9). When the event ends (`FUN_001964d0`) the chain event 0x4c
  fires for kinds with bit 0; the event's end (`FUN_00225028`) fires 0x12 or 99. Other starters: `FUN_001ad1c0`,
  `FUN_001ad378` (the game over and the revive: kind 0xf, the event `re_gameover-0a`, the first type 0x16 entry of `00battle.bin` through `FUN_00193f08(0)`; WP-RV2, docs/summons.md), `FUN_001b3468`.
- **What the start holds back, and what the end gives back** [V, WP-E6b]. `FUN_00195898` takes the kind row of
  `arg & 0x3f` (`FUN_001d1128`: `FUN_0030ff40` searches the `evtp` rows of `03system.bin`, 8 bytes, by their kind
  byte; +2 the flags; `struggle::event_kinds`). `FUN_00195ae0` then: flag bit 3 clear -> `FUN_00196280`
  (`FUN_001b7ce8(0)`, `(1)`: the players' control bits of 0x1d5d7b8); bit 2 clear -> `FUN_00196760` (`FUN_0014ef48`
  -> `FUN_0013da78`: the field HUD's draw tags 3 to 6, docs/struggle.md). The two calls the brief named are not HUD
  hides: `FUN_001bef48(0xc)` (disassembled: `a0` passes to `FUN_001bef10`, a search of the 0x40-byte rows of
  `DAT_01d5df08`, whose row is copied into slot `row +0x24` of 0x1d5dd88; `FUN_001befc0(0xc)` clears the slot)
  is pause-menu record 0xc (docs/struggle.md: the pause-menu record table), and `FUN_0014c748(4)` sets bit 4 of
  `DAT_00356f1c`, which only `FUN_0014c780` reads (callers `FUN_0014a080` pause menu, `FUN_0014c4a8`, `FUN_0014c7b8`
  the play clock): both are undone by `FUN_001959f0` (`FUN_0014c760(4)`, `FUN_001befc0(0xc)`) when the object is
  destroyed. **Nothing hides the player or the party models**: `FUN_00195ae0`, `FUN_00195d28` (state 0: the fade by
  kind row +1, `FUN_0014a918` / `FUN_0015d058`), `FUN_00196280` and `FUN_00196760` touch control bits, faders and
  HUD only [the event's own commands may hide actors: opcodes 3, 0xd, 0xf, 0x10 not read].
  State 9 (`FUN_00196170`): `FUN_0019d9c8(+0xc)` (the jump record's word 0 is not 0) decides; without a jump the
  fade-ins of kind flag bit 4 (`FUN_0014a7f0(.., 6, 0)` on two faders), `FUN_0016f8e0(-200)` for flag bit 0,
  control back (`FUN_001962a0`) and, for bit 2 clear, the field HUD back (`FUN_00196790`); with or without, it then
  calls `FUN_0019d9d8(+0xc, 0x80000000)`, the trigger jump of `area_jump.rs` with bit 31 in the fade flags
  (`FUN_00104e18`).
- **Skipping** [GAP]: `FUN_00224b18` (an event is running and `FUN_002c17e0`) and `FUN_00222630` drop commands;
  the pad check and what a skip jumps to were not read.
- **The timeline cursor** [V, WP-E6]. The setup reader and the timeline thread each keep one offset into the
  list (`DAT_0036ba28`, `DAT_0036ba24`); the thread runs the commands in FILE order, and a command of its switch
  waits in `FUN_00224a90` until `DAT_0036bcf0` reaches its frame, so a later command with an earlier frame runs
  right after the one before it. Commands outside the switch (the setup ones among them) pass without waiting.
  Frames: opcode 2 at +0x26, 6 and 0x1f none, the rest +4 [V for 0x12, 0x15, 0x2c, 0x2d, 0x44; inferred for the
  others]. The counter (`FUN_00222728`): `DAT_0036bcec += DAT_00356f40` (1.0) a game frame, times `DAT_0037ec50`
  (0.5); while a movie plays (`FUN_002f5ed0`) it is the movie's frame (`FUN_002f5eb8(7, 0)`). The end
  (`FUN_00222668`): once `DAT_0036bcf8` (opcode 8, -1 when absent, `FUN_002237b8`) is reached, `FUN_002235c0`
  (state 4). A size-0 command parks the thread for good.
- **Actor table** [V, WP-E6]. `DAT_0036bbcc`, 0x60 rows of 0x28 bytes (`FUN_00224548` takes the first row with
  word 0 = -1): word 0 the objentry id (opcode 1 +4), word 1 the actor index (+6), word 2 the object (0 until
  made), +0x17 the model name (+8). `FUN_002290e8` finds a row by objentry id (the model name for `.anb`
  paths), `FUN_002244b0` by actor index.
- **Load blocks, opcode 0x24** [V, WP-E6]. `FUN_00222a48` lists the command (64 slots, `DAT_0036bbd8`);
  `FUN_00222a88` takes the next block while `DAT_0036bcf0` lies within its +6..+8 frames. +4 counts the
  sub-records from +0xc (`u16 kind, u16 size`): 0x25 a motion (`FUN_0022cc88`: +4 objentry id, +6 actor
  index, +8 flag, +0xa name `anm_tt/ETT10201E`), 0x26 (`FUN_0022c070`), 0x38 a model (+4 objentry id, +6 index,
  +8 name), 0x39 (`FUN_00227df0`), 0x3e (`FUN_00226100`) [read by WP-E6d, below]. The `.anb` path
  (`FUN_0022cbb0`, strings 0x37f208 `anm/`, 0x37f210 `/`, 0x37f218 `.anb`): `anm/` + the name's prefix past
  its first four characters + `/` + model + the rest + `.anb`, e.g. `anm/tt/h_ex510/ETT10201E.anb`.
- **Motions, opcode 0x2c** [V, WP-E6]. `FUN_0022cd90`: at frame +4, actor +0xe: the actor's object is made
  here if it has none (`FUN_0016e8c0(DAT_0037f220, objentry | 0x40000000)`), the motion record of (model, name
  +0x12) is found (`FUN_002242a0`) and started through `FUN_0017e1c8(2 * +0x10, 2 * +8, object + 0x140, ..)`
  = `FUN_0017dd38(blend, start frame, ..)` (the same start as every other motion). +6 is read by nothing
  [V, WP-EV1: searched, see Unknowns]; +0xa / +0xc are a loop (below, WP-E6d).
- **Placements, opcode 2** [V, WP-E6]. `FUN_002293b8`: only when +4 is 0, at frame +0x26, actor +0x24:
  position +8..+0x14 (`FUN_0016a700`, the game's space) and rotation +0x14..+0x20 in degrees (`FUN_002292b0`,
  `DAT_0037f0c0` = pi / 180; yaw only when x and z are 0, `FUN_0016a7f0`).
- **Fades, opcode 0x12** [V for the layout]. `FUN_00226830`: at frame +4, +8 picks the fade object
  (`FUN_0014a978`, `FUN_0014a948`, `FUN_0014a918`) and +6 its length; not ported.
- **Motion loops, opcode 0x2c +0xa / +0xc** [V, WP-E6d]. `FUN_0022c9b0` fills the actor's row of the motion
  slots `DAT_0036bbc8` (64 rows of 0x40 bytes; `FUN_0022c928` the actor's row by index +0xe, `FUN_0022c970` a
  free one): +0x10 a counter = 2 * (+8 - +0x10), +0x14 = 2 * +8, +0x18 = +0xa, +0x1a = +0xc, +0x30 = 1.
  `FUN_0022ca18` (every frame, called at `0x22482c` by the setup reader `FUN_002245d0`) steps every row: with
  +0x1a not -1 and 2 * +0x1a < counter, the counter becomes 2 * +0x18 + the overshoot and the motion is started
  again there with no cross-fade (`FUN_0017e1c8(0, frame, ..)`), and unless `FUN_001beb00` the object is put
  back at the place `FUN_0016b960` kept at the row's first step (+0x20); then the counter counts
  `DAT_00356f40` (1.0). In tt02/03/04, 306 of 1159 opcode-0x2c commands loop. Ported (`MotionClock`) except
  the place put back [the app's event actors take no root motion, inferred]. [WP-E6e] The put-back is ported:
  `FUN_0016b960` answers object +0x540 (the position) unless `FUN_001beb00` (object +0x570 set: +0x70), the row
  copies it at its first step (+0x30 = 1, only on looping rows) and `FUN_0016a618` writes it back to +0x540
  (w = `DAT_0037a1b4`) and refreshes the object (`FUN_001921e8`) at every loop. Here `keep_place` / `put_back`
  on the last opcode-2 place (test `a_looping_motion_puts_its_actor_back_where_the_loop_began`); the rotation
  stays.
- **Opcode 0x4d: hide or show objects** [V, WP-EV1: the rule below; WP-E6e read the call]. `FUN_00226e80`: at
  frame +4, +6 = 0 -> `FUN_001b7fa8(2000)`, else `FUN_001b8040(2000)`: every object of the list at
  `DAT_01d5d7a0` (next at +0x8b4) without +0x588 bit 0x20 whose method +0x3c answers below 2000 gets object
  +0x10c bit 0x80 set (resp. cleared). Bit 0x80 is a hide: `FUN_0019af68` (true when +0x10c 0x80 or 0x100, ...)
  makes `FUN_00191c18` clear bit 4 of +0x768 [inferred: the draw bit] and the weapon test of docs/movement.md.
  Method +0x3c of the actor class (vtable 0x35b950) is `FUN_0016bc68`: the +0x28 tag of the object's
  `obj/%s.mdlx` resource (`FUN_0016ed08`, format 0x37a430; `FUN_0016f800` finds it in the list `DAT_0035bd88`;
  `FUN_0016f8e0(2000)` at the event start (state 2) moves tags of 2000 and above to 10000+). **WP-EV1, the rule.**
  Every object class vtable read (0x35b950, 0x35ced8, 0x35eea8, 0x35f580, 0x35c9a8, 0x35c908, 0x35e300) has
  `FUN_0016bc68` at +0x3c, and the compare in `FUN_001b7fa8` / `FUN_001b8040` is signed. The tag is the second
  argument of `FUN_0016e430` (the model load): -1 means -200 for an object the event made (id or'ed with
  0x40000000, `FUN_0016ef80`, objentry type not 7 or 0x16: the model load of load record 0x38 `FUN_00229198`;
  that opcode 0x2c's `FUN_0016e8c0` object uses the same resource is [inferred]), else the word of 0x37a278 by ObjectType (E6f, above). Result: opcode 0x4d (+6 = 0) hides
  every object that exists at that moment and is either made by the event (-200) or of ObjectType 5, 6, 8, 10,
  11, 13, 18 or 23 (tags 1030..1110, the field's props: the tt areas spawn F_EX000, F_TT010 and F_TT010_SORA of
  them, 217 entities); the player, friends, townspeople, enemies, save points and summons (2100..2700) are
  never touched, and the explicit tags other callers pass (0x13ef `FUN_002a18a0`, 0x96a / 0x974 from
  `FUN_001b3f78` and `FUN_00174fb0`, -200 `FUN_002c4c58`) lie at 2000 or above except the -200 ones.
  +6 not 0 clears the bit on the same set. It is a one-time pass over the list, not a state: an object made
  afterwards (the event's actors appear at their first opcode 0x2c) is not hidden by an earlier hide. How it
  ends: only a later opcode 0x4d with +6 not 0, or the object's end. Bit 0x80 is the hide flag (above). On the disc, over all 268 `.ard` files (test `opcode_0x4d_on_the_whole_disc`), there are 29 of them in 21
  areas: bb12 (112, 113), ca01 (102), ca06 (206), ca07 (120), ca24 (214), eh16 (126), he00 (124, 141),
  he03 (102: hide at frame 4, show at 379), nm06 (126: hide at 6181, show at 6843), nm10 (206), po04 (102),
  tr01 (106), tt01 (101), tt06 (305), tt09 (209), tt12 (521: a show only), tt16 (515), tt32 (312), wi01 (102, 103),
  wi02 (121), wi03 (119), wi04 (109, 129), wi07 (106). Most are a hide at frame 0 or 1, before the event's actors
  exist, so they hide the area's props, not Sora. Only he03 102 and nm06 126 hide and later show. Not ported: an
  id with no objentry row (`FUN_0016ef80` needs the row) and the objects with flag +0x588 bit 0x20 that the list
  walk skips; the app's event actors have neither (comment on `resource_tag`). Ported: `event_player::resource_tag`, `op_4d_touches`, `op_4d_hidden` and
  `Spawned::hidden` (an event actor that exists at the pass is hidden or shown; a hidden one stays hidden when
  opcode 2 places it) [SUPERSEDED by WP-EV2, below]; tests `opcode_0x4d_hides_and_shows_the_objects_whose_tag_is_below_2000`,
  `opcode_0x4d_touches_the_object_types_the_table_puts_below_2000` (reads the table from the executable).
  The app draws none of the prop types (no F_* object is spawned by the app), so nothing else is hidden
  [inferred: those objects are not in the app]. Log: `op 0x4d hides 0 event actor(s)` at tt01 101 frame 0.
  **WP-EV2 (the +0x588 bit 0x20 skip).** The walk's first test (`(object +0x588 & 0x20) == 0`, `FUN_001b7fa8` /
  `FUN_001b8040`) was listed above as "not set by anything the app has". It is set exactly on the event's actors:
  opcode 0x2c (`FUN_0022cd90`) calls `FUN_0016e8c0(.., actor id | 0x40000000, ..)`; its first branch (`FUN_0016ef80`
  yes: the id has an objentry row whose type is not 7 or 0x16) allocates 0x9c0 bytes and runs `FUN_00196a70`, which
  runs the object base constructor `FUN_00168d30`, sets the vtable 0x35e3b0 and ORs 0x20 into +0x588.
  `mips_scan --jal 0x196a70` finds that one caller; `--jal 0x16e8c0` lists 11 callers
  (opcode 0x2c, `FUN_0022cd90`, among them). Other writers of bit 0x20 into +0x588: none (the other `[0x162]` ORs in
  the decomp are 0x200, 0x20000, 0x200000, 0x2000000, 0x20000000). So opcode 0x4d never hides or shows an event
  actor; it only reaches the props of ObjectType 5, 6, 8, 10, 11, 13, 18, 23, which the app does not spawn in events.
  The load record 0x38 (`FUN_00229198` -> `FUN_0016e638`, tag -200) loads a model resource, not an object, so it is
  no object in the list. Ported: `Spawned::hidden` and `op_4d_hidden` removed; `op_4d_walk_touches(flags, tag)`;
  tests `opcode_0x4d_hides_and_shows_the_objects_whose_tag_is_below_2000` (the flag skips a -200 object),
  `opcode_0x4d_never_hides_an_event_actor` (the real `step` path). Log: `op 0x4d hides 0 event actor(s) (flag +0x588
  bit 0x20 skips them: ...)`.
- **Opcode 2 +6** [V, WP-EV2]: `FUN_002293b8` reads +4 (must be 0), +8..+0x20, +0x24, +0x26 only; values seen 0,
  1247, 2258, 30716. Its only call site is the timeline switch (`mips_scan --jal 0x2293b8`: `FUN_00223a88`), it is
  not referenced as data (`--addr 0x2293b8`: none), and its callees (`FUN_00224418`, `FUN_0016a700`, `FUN_002292b0`)
  get values, not the command pointer. `--load-off 6` over 0x220000..0x23ffff lists 94 loads; the ones near
  the opcode handlers belong to other opcodes' handlers. So +6 is never read [inferred: an editor value, as for 0x2c].
- **Opcode 3** [V for the readers]: in neither switch (`FUN_002245d0`, `FUN_00223a88`), so both readers pass it.
  Every tt02/03/04 event has one: `u16 area number, "tt"` (tt02: 2, tt03: 3, tt04: 4) [inferred: the area the
  event belongs to]. Not a show/hide.
- **Opcode 0xd: the drawn mesh groups** [V, WP-E6e]. `FUN_00228b48`: at frame +4, actor +8's object
  (`FUN_002244b0`, row +8), `FUN_0011c550(object +0x670, cmd + 0xa)`: object +0x670 is the model instance
  (`FUN_00168d30`: made by the model resource's method +0x24); `FUN_0011c550` calls method +0x34 of its +0x14
  (the model class) with its +0x104 (one draw-flag byte a mesh) and the u16 list at +0xa. The model classes'
  vtables were found by the address of their +0x24 create method in the executable's data: 0x354b18
  (`FUN_0011f390`, a map-style model; +0x34 `FUN_0011f4a0`), 0x354b98 (`FUN_00120b48`, the skeletal model;
  +0x34 `FUN_00120fd0`, +0x3c `FUN_00121178`, +0x40 `FUN_00121278`), 0x354c00 (`FUN_00123be0`, a multi-model;
  +0x34 `FUN_00123f68` forwards to each sub-model). `FUN_00120b48` sets every mesh's flag to 1; `FUN_00120fd0`
  first clears the low four bits of every flag (`FUN_0011eda0`), then for each list value up to 0xffff ORs 1
  (bit 15 clear) or 2 (bit 15 set) into every mesh whose group, bits 6..10 of its 0x20-byte header's flags word
  (model +0x1c the count, headers from +0x20), is `value & 0x7fff`; the same for the second model at model +0xc
  (`FUN_001205b8`, group = bits 0..4). The draw `FUN_00121a78` skips a mesh whose flag has no low bit set. So
  opcode 0xd says which mesh groups of the actor are drawn (the others are not) [what flag 2 changes in the
  draw: WP-EV1, below]. +0x3c / +0x40 show / hide one group (callers `FUN_00192cd0`, `FUN_00192ce8`, `FUN_001485a8`).
  Ported: `mesh_groups`, `mesh_flags` hide the actor's mesh entities of unlisted groups (test
  `opcode_0xd_draws_only_the_listed_mesh_groups`: all 150 commands in tt02/03/04 reach a readable model; the
  tt lists name groups 0 and 2..0x1f, i.e. they hide group 1 (N_EX500_RTN: mesh 2 of 4) [inferred: an alternate part]); the second model is not drawn by the app.
  **WP-EV1 (vtable +0x34 and flag 2).** The +0x34 call is exactly `FUN_00120fd0` above for the skeletal model
  (vtable 0x354b98); the map-style model's (0x354b18) `FUN_0011f4a0` and the multi-model's (0x354c00) `FUN_00123f68`
  are the same call for those classes, nothing else calls it. Flag value 2 (bit 15 of a list value): the draw
  `FUN_00121a78` tests `flag & 0xf` (twice) and `flag & 4`, `FUN_0011ee30` (called by `FUN_0011c5e0` /
  `FUN_0011c648`) tests `flag & 4`; nothing tests value 2 on its own (`FUN_0011c7d8`, `FUN_0011cba8`,
  `FUN_0011d4f8` only pass the array on), so a mesh with flag 2 draws like one with flag 1 [V for the readers
  found by `mips_scan --load-off 0x104` in 0x11c000..0x124000; WP-EV2: the scan over the whole executable gives 65
  hits, the others outside 0x11c000..0x124000 read a different field (`FUN_0017b9e0` and `FUN_0017dd38` use +0x104 of
  the motion block at object +0x140, `FUN_002f2970` of a particle object, the rest are float loads), so no reader of
  the model instance's flag value 2 exists].
- **Opcode 0xf** [V]. Setup reader, `FUN_00226f50`: waits while `FUN_001962c0` (the event object not yet in
  its play states), sets `DAT_0036bd10` = 1 (the timeline thread `FUN_00223a88` waits for it before anything)
  and makes the event's tables (`FUN_00222cc0`: the camera table `DAT_0036bbb4`, the motion slots, the actor
  table, ...). Every tt02/03/04 event has it. Ported: without it the timeline does not run.
- **Opcode 0x10: the fade-in after the setup, not an end fade** [V, WP-E6e corrects E6d]. Setup reader,
  `FUN_00226fa0`: `DAT_0036bd0c` = +4. The thread `FUN_00223a88`, once the setup is done (`DAT_0036b9e0` 2,
  ready `DAT_0036bcf4`) and the event object has bit 2 (`FUN_00196320`), calls `FUN_00196348(2 *
  DAT_0036bd0c)` BEFORE its first command: event object +0x90 = that, state 5 (`FUN_00195fb8`), then state 6
  (`FUN_00195ff8`): fader 0x3564d0 (`FUN_0014a918`) mode 1 (`FUN_0014a7f0`, the fade IN, as area_jump.rs reads
  it) over `(int)(+0x90 * DAT_0037b054)` frames for kind row byte +1 = 1 or 3 (black, `+0xf4` 0) or 2 (white),
  then state 7 or 8 (play). tt events: 20 frames, 40 ticks (`FUN_0014a710`: twice the frames). Before it, state
  0 (`FUN_00195d28`, at the trigger) fades the same fader OUT (`FUN_0014a7c8`, mode 3) over `(int)(row +4 (a
  float) * DAT_0037b050)` frames for row byte +1 = 1 (black) / 2 (white), or plays the keyhole layout for 3
  (`FUN_0015d058`); states 1..3 (`FUN_00195e68`, `FUN_00195ef8`, `FUN_00195f68`) wait for that fade
  (`FUN_001966f8`: fader `+0xfc` != 4) before they set bit 2, so the timeline starts on a covered screen. Flag
  bit 4 adds two other faders (0x3566f0, 0x356800) at 0 frames (state 6) and 6 frames (state 9, fade-in): not
  ported. The event's END (`FUN_002233d8` -> `FUN_001963e0`, state 9 `FUN_00196170`) fades only those and the
  faders 0x356910 / 0x356a20 (`DAT_0036bef0`); the screen fader goes black at the end only through the trigger
  jump (area_jump.rs). Ported in `event_player.rs` (`kind_rows`, `start_fade`, `setup_fade`, `ScreenFade`,
  `EventPlayer::screen_fade`; test `the_event_object_fades_out_then_in_by_its_kind_row`): the timeline is held
  while the start fade runs; drawn only once `area_jump::draw_fader` reads `screen_fade` (outside this WP's
  files). The keyhole layout of kind 3 is not started from here [GAP].
- **Load sub-records 0x26, 0x39, 0x3e** [V, WP-E6d]. 0x26 (`FUN_0022c070(rec + 4, block parity)`): a voice,
  `FUN_0022bff8` = 0x37f178 `voice/%s/event/` with `DAT_00349d40` + name + 0x37f188 `.vag`, loaded into a file
  slot of kind 3 (`FUN_00224200`, `DAT_0036bbc0`, 128 rows of 0x54) through `FUN_00224030` / `FUN_00167d20`
  (callback 0x222a28). 0x39 (`FUN_00227df0(rec + 6, rec +4)`): an effect, a free slot (`FUN_002241c8`) of kind
  2 with +4 at slot +0xc, `FUN_00227d80` = 0x37f090 `event/effect/` + name + 0x37f0a0 `.pax`. 0x3e
  (`FUN_00226100(rec + 4)`): `FUN_002260b0` = 0x37f038 `event/layout/%s/%s.2ld` with `DAT_00349d40` and the
  name, handle at `DAT_01dc0078`, `FUN_0016fac8(.., 0x13ec, 0)`. `DAT_00349d40` is the region (`us`), not the
  world [inferred: the USA disc has `voice/us/event/tt10203or.vag`, `event/layout/us/tt_telop.2ld`,
  `movie/us/opn.pss` and no `tt` folder there]. Test `twilight_town_event_load_records_name_files_on_the_disc`:
  35 events, 290 voices, 8 effects, 1 layout, every file on the disc. Ported: parsed, paths logged at the
  start; the voices are not played (which command starts one was not read), the effects and layout not drawn.
- **Movies, opcode 0x44** [V]. `FUN_00225860`: at frame +4, plays `movie/<region>/<+6>.pss` (format 0x37f010
  with `DAT_00349d40`, `us`; WP-E6d) and waits for it; the tt01 events `100`, `200`, `400`, `600` are the movies `opn`,
  `me2`, `me4`, `me6` with their subtitle lines (test `the_tt01_short_events_are_movies_with_subtitle_lines`).
- **World map events are not area events** [V, WP-WM7]. The events of the `wm` world (1836 and 1837 started by a confirm on a symbol, `FUN_002ccd88` -> `FUN_002c6a40`) are programs of the talk table (`libretto-wm.bar`, the talk table of section 3) run by their own executor `FUN_002c4c58` (ops 0x100 to 0x113: actors, animations of the entries of `libretto/us/wm<event>.bar`, voices, a stage matrix, fades, message windows), not commands of a type-0x16 entry, and they do not use the event player above (no `evtp` kind row, no frame counter: the executor waits in frames). The port is in `worldmap.rs` (`Script`, `WmEvent`; `kh2_ard::libretto::Cut` reads the ops); docs/worldmap.md section 10 "W7 events" has the ops, the files and the gaps.

