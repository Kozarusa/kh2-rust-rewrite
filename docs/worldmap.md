# World map (research, WP-WM0)

Research only: nothing here is in the app yet. The app's pieces that touch the map are listed in section 9. Rules of the project apply (RULES.md A3
to A5): the USA disc is the only source, values and strings stay on the disc, so this file describes layouts and meanings and gives ids and
counts, never table contents or the game's wording. Tags: [V] read in the executable or on the disc, [I] inferred, [GAP] not found or not read.
Addresses are EE addresses; `FUN_xxxxxxxx` names are the decompilation's. Disc files are named as in the archive listing (`kh2 ls wm`).

## 0. Summary

- The world map is not a separate scene or overlay. It is an ordinary field area: world byte 0xf, area 0, files `ard/wm00.ard` and
  `map/us/wm00.map`, entered by the same jump task as every other area (`FUN_00104e18`) [V].
- Its screen is made of scripted objects of objentry type 0x11 (the executable's code at 0x2c1000..0x2d2000 and `FUN_002f7c00..`): twelve world
  symbols, a controllable cursor (the Gummi ship), nine Gummi route markers, plus panels (a world info panel, a "land" prompt, a destination
  list, an episode list) drawn from `field2d/us/wm0command.2dd`, `wm0field.2dd`, `file/us/wm_ws.2ld`, `70landing.bar` and `msg/us/wm.bar` [V].
- The cursor starts at the position of an entrance: the entrance number is looked up from the world the player left (15 pairs at 0x3790a8,
  `FUN_00149db8`), and each of 22 cursor spawn records belongs to one entrance [V].
- The landing destination is a table lookup on (world, the world's story state byte) in the `wlds` entry of `00progress.bin`, or a row picked from a
  destination list built from `70landing.bar` and `00worldpoint.bin`. The jump is `FUN_00104e18(destination, 1, 0, 0, 0)` [V].
- What each world looks like (hidden, dim, available), which episodes are listed and which events fire is driven by story flags through the `wldf`
  entry of `00progress.bin` into a 4-byte-per-world record table at 0x341858 and bit arrays at 0x3418a4 / 0x3419e4 / 0x3419f4 [V].

## 1. Entering the map

Three ways reach world 0xf, all ending in `FUN_00149df0(world, flags)` [V]:

1. **Save point row "World Map"** (docs/menus.md section 18.5). Menu kind 1, page 0x10 `FUN_00257ee8` asks the yes/no question (text id 0x3859,
   row messages 0x8a7b/0x8a7c/0x8a7d, the question list `0x380cb0`). In the page, row index 1 runs `FUN_002580b8` (`FUN_0014a7c8(fader 0x3564d0,
   0xf, 0)`: a fade out to black of 15 units, then `FUN_00295ac0(0)`) and goes to page 0x14; row 0 and the cancel results (-2) return to page 0
   [V code; which printed row is row 1 is menus.md's reading, [I]]. Page 0x14 `FUN_002580e8` waits for the fader's state word `+0xfc` to be 4
   (black), then `FUN_00103b18(1)` (the scene mode word `0x33e78c` = 1) and closes the menu. The field scene's entry `FUN_00103b38` reads that
   word: mode 1 calls `FUN_00149df0(-1, 0xffffffff80000000)` (mode 2 is the load path, mode 3 a fade in) [V]. `FUN_00103b08` clears the word first and
   `FUN_00103d30` clears 8 bytes at 0x33e798 afterwards.
2. **A trigger's jump record of type 4** (`FUN_0019d9d8` case 4, docs/events.md "WP-JT3"): story flags decide between world 4, world 0x12 and the
   map; the map case calls `FUN_00149df0(record world byte, 0)` after replacing the byte with the remembered one at `DAT_01c8bd90` [V].
3. **Event commands 0x10e and 0x10f of `FUN_002c4c58`** (the event-command executor, which also handles 0x100..0x113): 0x10f resolves
   (key, world) through `FUN_00198080` and 0x10e takes a literal destination; both call `FUN_00104e18(dest, 0xffffffff80000001, 0, 0, 0)`.
   The effect of the two commands is [V]; who sends them is [GAP] (no call site and no data pointer to `FUN_002c4c58` exists in the executable, so
   it is reached through a table built another way or by the event player) and that they are the exits of scripted scenes is [I].

**What `FUN_00149df0` does** [V]: it raises the global event signal 0x21 (`FUN_00197b70(0x21, 0)`, "leaving for the world map", docs/events.md
section 1 signals), builds a destination of (world 0xf, area 0, entrance, 0xff, 0xff, 0xff), and writes the same world, area and entrance into the
current-place bytes at 0x33e830..0x33e832 (so the saved "last world" is now 0xf). The entrance is `FUN_00149db8(world)`: the first of 15 `(entrance,
world)` pairs at 0x3790a8 whose world equals the argument, else 0. For argument -1 the argument becomes the world byte at 0x33e830 before it is
overwritten. The three program bytes stay 0xff, which keeps whatever `FUN_00104bd0` took from the program table `0x33e86f + 0xf * 0xc0` (section 3).
The second argument carries bit 31 (0x80000000) from the save point and the scripted exits, OR-ed with 1; the type-4 path passes 0.
`FUN_00104e18` stores the six destination bytes at 0x33e838, starts the jump task `FUN_00104d58` and the fade kind (docs/events.md section 2); when a
jump is already running (`DAT_003b2180` non-zero) it only overwrites the pending destination and ORs the fade argument with 0x80000000. What bit 31
changes in `FUN_0013ef68` is [GAP].

The 15 pairs cover the twelve worlds that have a symbol on the map plus three worlds without one (their entrances sit after the twelve) [V by
reading the table at run time]. Entrance 0 is the answer for a world not in the table and is the cursor's start position of a new game [I: entrance 0 has
a spawn record].

## 2. The map area wm00

Files [V, `kh2 ls wm`, `kh2 bar`]:

| file | contents |
|---|---|
| `ard/wm00.ard` | BAR: `map` script (15 programs), `evt` script (1 program), spawn files `m_00`, `m_01`, `m_50`, `m_51`. No `btl` script. |
| `map/us/wm00.map` | 5.6 MB map (two large MAP parts, a collision part, a sky part, many BOB prop pairs); `kh2-map` has not been pointed at it [GAP] |
| `msg/us/wm.bar` | one message table (207 messages) and one more entry (`md_m`, 33856 bytes) [GAP what `md_m` is] |
| `field2d/us/wm0command.2dd`, `wm0field.2dd` | panel art (sequence ids used by the panels, section 5) |
| `file/us/wm_ws.2ld` | loaded by the `evt` program (command 0x16 with the format `file/%s/wm_ws.2ld`) [V]; layout of the world select art [GAP] |
| `70landing.bar` | destination table, row art for 53 areas (section 6); loaded by `FUN_002f7c00` from `FUN_002c57b8` |
| `50wmentry.bin`, `50worldmap.bin` | two near-identical tables: an 8-byte header (version 2, count 35 and 33) and 0x80-byte records (an id, a type number 0x11, a 0x40-byte name of the form `ROUTE_SYMBOL_xx`, floats); no reader found [GAP] |
| `libretto/us/wm<N>.bar` | 13 event dialogue files (N = 502, 503, 512, 521, 524, 1660, 1661, 1662, 1823, 1824, 1836, 1837, 1838); loaded by event command 0x101 (`FUN_002c5598`, format 0x3832f0) [V] |
| `obj/WM_*.mdlx/.mset/.apdx` | symbol models (12), cursor, Gummi marker, checker, plus event props (cockpit, hangar, two Disney Castle props, three ship models) |
| `voice/us/event/wm3xxxx*.vag` | spoken lines of the wm events; not part of the screen |
| `dbg/localset-wm.bin`, `dbg/progress-wm.dbg`, `dbg/mission-wm.dbg` | debug data: the first is empty, the second lists 17 world-15 flags (ids 0x3c00..0x3c10) with names such as "<world>_CLEAR" and "EVENT_*", the third names the mission `WORLDMAP` with world 0xf [V] |

**The `map` script.** 15 programs with ids 0..9, 0xa..0xd and 0x19. Each runs the same three kinds of commands [V, `kh2_ard` format]:

1. opcode 1 (`map_visibility`): a different pair of show/hide masks per program, i.e. which map parts are drawn; id 0x19 shows the least;
2. opcode 0x15 (`battle_manager`, `FUN_001b9338`) with argument 0 = 0xc and the tag `WORLDMAP`: creates the manager at 0x1d5d7e8
   (`FUN_001b8b10`). It formats `msn/%s/%s.bar` (0x37bbb0) with the language and the tag, i.e. `msn/us/WORLDMAP.bar`, a 64-byte file with one 32-byte
   header record and no script (so the manager has no mission program) [V]. `FUN_001d5048` for type 0xc runs `FUN_001abfe8(1)` (party reset code 1)
   and sets the area camera type `DAT_0035cd38` to 4 [V]. What camera type 4 does is in docs/camera.md only if it lists type 4 [GAP: not checked here];
3. opcode 0 (`spawn`) for `m_00`, `m_01`, `m_50`, `m_51` (id 0x19 only the first two).

**Which program runs.** The program bytes come from the table at `0x33e86f + 0xf * 0xc0 + area * 3`, which only flag side effects (op 1) of the
world-15 flag page write. That page's entry in `00progress.bin` (`wm`, 17 flags) has an empty list for every flag, `07localset.bin` has no pairs for
world 15, and the jump records that lead here keep the program bytes (0xff) [V]. So the table keeps its start value and program 0 (`map`) and program 0
(`evt`) run, unless a scripted exit passes explicit bytes in its destination [I]. The other 14 programs are reachable only by an explicit destination;
who passes them is [GAP] (the callers of `FUN_00104e18` in the 0x2d0000..0x2e8000 minigame code were not read).

**Spawn files.** All entries use the 0x40-byte entity records of docs/events.md section 3; the kind byte +0x1c is 1 for the cursor and 0 for the rest
[V]:

| file | contents |
|---|---|
| `m_00` | 13 groups, one `WM_CURSOR` entity each, spawn kind 1 with +0x1d = 0..12 |
| `m_01` | 9 groups, one `WM_CURSOR` each, +0x1d = 13..21 |
| `m_50` | one group of 12 `WM_SYMBOL_xx` entities (kind 0) at the world positions: TT, HB, MU, BB, HE, DC, LM, CA, NM, AL, LK, EH |
| `m_51` | one group of 9 `WM_GUMI_01` entities (kind 0), +0x20 holds a route number 1..9 |

Entity kind 1 is created only when its +0x1d equals the entrance byte (`FUN_001981a0`, docs/events.md section 3). So exactly one cursor exists, standing
at the spawn record of the entrance of section 1. The 22 cursor positions are the places where the Gummi ship appears after leaving a world (entrances
1..12 are the symbol worlds' own, 13..15 the three extra worlds of the pair table, 16..21 are not in the table [I: Gummi route returns]).

## 3. The objects (objentry type 0x11, `FUN_0016e8c0` case 0x11 -> `FUN_002c58c8`)

The factory switches on the objentry flag byte (+5) [V]:

| flag byte | class | objentry rows | built by |
|---|---|---|---|
| 0 | world symbol | 12 (ids 0x3a2..0x3ac and 0x456) | `FUN_002c64d0` (size 0xb10, vtable 0x36f7c8) |
| 1 | Gummi route marker | `WM_GUMI_01` | `FUN_002c8b08` (size 0xaf0, vtable 0x36f920) |
| 2 | cursor | `WM_CURSOR` (id 0x3a1) | `FUN_002ccf28` -> `FUN_002cc8a0` (size 0xbe0, vtable 0x36fb00) |
| 3 | unused | no objentry row | `FUN_002cc518` (vtable 0x36fa40) |

All derive from the actor base (`FUN_00184bf8` / `FUN_00176e00`); the cursor derives from the player-side class (its vtable shares the slots of the
player-controlled actor, 0x177788, 0x1772b0, ...), so its movement is most likely the player's analog stick movement with a world-map state set
[I]; the cursor's states `FUN_002cca58`, `FUN_002ccb70`, `FUN_002ccca0`, `FUN_002cce10` were read only as far as section 4 uses them. The scripted
props `WM_COCKPIT`, `WM_HANGAR`, `WM_DC030/040` and `WM_EX020/030/100` are objentry type 2 (event actors), and `WM_CHECKER` type 13 [V].

**Symbols** (`FUN_002c66b8`) find their world in the 12-entry table at 0x383338 by object id (second word, low byte = world), read the world's 4-byte
record (`FUN_001b4e80(world)` = 0x341858 + 4 * world) and pick their look from byte 0 of it: 0 and 1 are the two dim states, 2 and above the normal one; bit 1 of byte 3 turns a marker
effect off and bit 0 turns an effect on (effect ids 0..5 in `FUN_0016a530/0016a570`) [V code; the visual meaning is [I]]. The TWTNW symbol (world 0x12)
adds a ring effect whose kind depends on story flag 0x481c (6 clear, 8 set) [V].

## 4. What the screen does

On contact the cursor's handler (`FUN_002cc9d0`, vtable slot 5) passes the object it touches to `FUN_002cca98` [V]:

- a symbol (objentry id field `+0x588` negative): `FUN_002cf618(world)` creates the **world info panel** (`FUN_002ce808`, an object of 0x1690 bytes) for
  that world; leaving the symbol removes it (`FUN_002cf698`). The panel shows the world's name and its current objective and previous event text:
  `FUN_002c9548` / `FUN_002c9580` take two text ids from the episode records of that world (`FUN_002a1e98`); the texts are ids 0x1e02..0x1e3d and 0x4c3b..
  0x4c54 of `msg/us/wm.bar` (episode titles and hints) [V that the ids are in that table, [I] which is title and which is hint];
- a Gummi route marker: `FUN_002d0b30(route)` shows the route info (the nine route texts 0x4a4d..0x4a55) [V].

The confirm button on a symbol (action ids 0x9f and 0x206 in `FUN_002ccb70`):

1. `FUN_002ccd88` first checks two scripted events [V]: object 0x3a2 (the Twilight Town symbol) with world flag 1 set and 0x13 clear starts event 1837,
   object 0x456 (the TWTNW symbol) with world flags 0x35 set and 0x37 clear starts event 1836 (`FUN_002c6a40`: a task of priority 15000 that plays the
   program of that number from the `wm` talk table and sets the manager's bit); in both cases no prompt follows and the info panel is removed
   (`FUN_002ccca0` calls `FUN_002cf698`, `FUN_002d0bc8`). Ported by WP-WM7: section 10 "W7 events". World flags are the bits at 0x3418a4 (`FUN_001b6a18`), section 6.
2. Otherwise `FUN_002cc1e0` creates the **window object** `FUN_002cbee8` (size 0x3600, one at a time, `DAT_0036fa10`) with the touched object. Its
   constructor asks `FUN_002d1d20(world)` for the destination rows of that world (section 7): none gives mode 0 (the land prompt), some give mode 1
   (the area list). The mode word is at +0x1f68.

**Window modes** [V code, meanings [I]]. Mode 0, built by `FUN_002cbcf8`: a two-row list, row 0 text = "Land in <world>" (the 19 ids at 0x383520, text id
0x4871 + world index for the 19 worlds), row 1 = the cancel text 0x4884 ("Maybe later"). The result handler `FUN_002cacd8`: row 0 sets the window's result
word +0x1f64 to 1, the window's done bit (+0x18 bit 1) and copies the six destination bytes of `FUN_001c1cf8(world)` (section 7) into +0x3428; row 1 and
the cancel result -2 set the result to 0 and close; the results -1 and -4 are "no input" and do nothing. Mode 1, built by `FUN_002cb508` (its caption
0x4c16 is the one found by a constant scan; 0x4c10..0x4c15 are the other captions): the **area list**, rows of 0x1c0 bytes from `FUN_002d1d20`. The handler
`FUN_002ca6a0` moves a row cursor, skips rows without the selectable bit (bit 1 of the row flags at row + 0x2130), shows the message 0x4c0d ("not
accessible now", box kind 0x756c) on a row with the locked bit (bit 0), and a selectable row copies its six destination bytes (row + 0x2134..) into
+0x3428 and sets the result to 1. In mode 1 the button mask 0x100 switches to mode 2 (`FUN_002ca3d0`) and in mode 2 the mask 0x208 to mode 3
(`FUN_002ca538`); mode 2 scrolls a second list (object +0x2f6c, rows of 0x18c bytes filled from the episode records, `FUN_002c9600` / `FUN_002c96f0`), so
mode 2 is the **episode list**; mode 3 is the area list again, entered from mode 2 (WP-WM6: `FUN_002ca538` starts layout 10 and goes back to the area layouts, section 10 "W6 art"). The masks are the R1 icon (0x100; the icon is in the switch caption's message) and, from the episode list, L1 or cancel (0x208, the same for the "back" caption); the triangle (0x400, area list only, `FUN_002c9510` opens the journal at the world's topic, `FUN_0024f3c0(0, FUN_00277c88(world))`) is a gap (WP-WM7, section 10 "W7 panel digits, sounds and the journal").

**Teardown does the leaving.** `FUN_002ca2e8` (window destroy) tests +0x1f64 == 1 and then calls `FUN_002cc4b0(window + 0x3428)`, i.e.
`FUN_00104e18(destination, 1, 0, 0, 0)`: fade kind 1, no wait, no flag, no reset code [V]. Everything else on the map (cursor, symbols) is left to the
normal area change. The sounds are `FUN_002d1760(k)` (k 1 cursor move, 2 confirm, 3 open, 4 cancel, 5 denied) [V]; WP-WM7 read the ids: `FUN_002c3318(k)` splits `k` into bank `k / 1000` and entry
`k % 1000` (`FUN_002c2258`), so they are entries 1 to 5 of bank 0 (section 10 "W7 panel digits, sounds and the journal").

## 5. Panels, art and text

[V] unless marked. Both panels draw from `file/us/wm_ws.2ld`, a `.2ld` (BAR: IMGZ and LAYD) with two images, two sequences and one LAYD of 72 layers in 11 layouts; the panels use sequence 1 with image 1 (`FUN_00239020(layd, 1)`, `FUN_00233c20(imgz, 1)`); sequence 0 (the world select art) is not used by them. The ids 0x7531 and the like are task or object type ids, not sequence ids, and `wm0command.2dd` / `wm0field.2dd` are not used by these panels. The details are in section 10 "W6 art (WP-WM6)".

Text: `msg/us/wm.bar` ids, grouped (counts only): tutorial lines for flying (3 ids), episode titles and hints (about 60), "Land in <world>" (19),
route names (9), menu captions (7), "new episode" notices (11), a few cutscene fragments (about 30 ids that begin with a sound-effect or pause
command and have almost no text). Cutscene lines are in the 13 libretto files. [V]

## 6. State: what unlocks what

All of this is the story-flag side-effect machinery of docs/progress.md / docs/events.md section 5, extended by the `wldf` list:

1. **Story flags** (0x33f6b0, `FUN_0017ff60`, docs/events.md "Side effects"). After a flag's own op list the setter calls `FUN_001b6da0(flag)` (the
   `wldf` hook) [V]. The app's `progress::Story` does not call it: "the calls after the list" are in `Story::unported` (docs/progress.md).
2. **`wldf`** (`00progress.bin`, one entry, 147 records of 16 bytes, `FUN_001b68c0` binary search on the key, `FUN_001b6a48` runs a list) [V layout]:
   byte +0 the world-flag index, +1 a kind (0, 1, 2), +4 u32 the story flag it follows, +8 u32 an event number (0 for none; 13 of them
   are the `wm` libretto files), +12 u32 offset of an action list inside the entry. `FUN_001b6da0(flag)` finds the records whose +4 equals the
   story flag and calls `FUN_001b68f8(+0)`: it sets world flag +0 (bit array 0x3418a4, `FUN_001b6a18` reads it), except for four hard-coded
   indexes (0x25, 0x2a, 0x2d, 0x2e) that are skipped when another world flag (0x73, 0x56, 0x54, 0x72) is already set; a record of kind other
   than 1, or whose test `FUN_00180330` is false, is also queued at 0x3418c4 (counter byte 0x3419c3) [V; the queue is the "new" notice source [I]].
3. **Action lists** run through `FUN_001d8d18` / `FUN_001b6a90` (u16 op, u16 count, then count pairs of (u16 id, u16 value)) [V]:

   | op | effect |
   |---|---|
   | 1, 2, 3, 7 | world record byte 0 of each listed world = 0, 1, 2, 3 (`FUN_001b6cb8` -> `FUN_001b4e98`: 3 on a value above 2 increments instead; 2 also clears bit 0 of byte 3) |
   | 4 | byte 0 = 2 and clear bit 1 of byte 3 |
   | 5 / 6 | set bit 1 / bit 0 of byte 3 |
   | 8..0xb | `FUN_001b6d30`: sets a byte of an entry found by `FUN_001c0950(id)` to 0..3 [GAP: whose table] |
   | 0xc / 0xd | add / remove episode `id` (`FUN_001c1410` sets its bit in 0x3419e4 and 0x3419f4; `FUN_001c1488` clears the second) |
   | 0xf | store a byte at 0x341a05 and, when the `dark` entry is loaded, run `FUN_001c6d10` (sets map-flag bits of the first 64, the area script's `map_show`/`hide` bits) |

   147 records: 131 carry action lists with 235 ops in all; the ops used are 1..10, 12, 13 and 15. [V]
4. **World record bytes** (`FUN_001b4e80(w)` = 0x341858 + 4 * w): byte 0 the map state used by the symbol (section 3); byte 1 and 2 are written by flag op
   0xd (`FUN_0017ff60`: the halfword's high byte to +1, low byte to +2); byte 1 is the key the landing table uses (section 7); byte 3 bit flags (above)
   [V]. Initial values are zero if the save block clearing at boot (docs/progress.md) covers 0x341858 [I]; the "not yet visited" meaning of 0 is [I].
5. **Episodes** (`epsd`, 43 records of 8 bytes): u16 id, byte +2 the world, +3 order inside the world, u16 +4 the title text id, u16 +6 the hint text id
   [V layout from `FUN_001c14e8`, `FUN_002c9600`]. `FUN_001c14e8(world, previous)` yields the next record of the world whose id bit is set in
   0x3419f4 (`FUN_001c15f0`, `FUN_001c1628`). The "Episode list" (mode 3) and the info panel read these.
6. **Destinations and locks** (docs/events.md section 5 "exit locks"). A row of the destination list shows only world points seen before: bit of the world
   point index in the array at 0x33f910 (`FUN_00195010`), which the exit and area code sets when the player enters the area (`FUN_00194fd8`); the row is locked when the
   area is locked by an exit-lock entry of `dsa` (`FUN_00195358(world, area)` non-zero): result 2 in `FUN_00195010`, flag bit 0 in `FUN_002d1d20`.
7. **Forced events** (section 4) test world flags 1, 0x13, 0x35, 0x37 (and the symbol effect tests story flag 0x481c).
8. **`wmky`** (11 records of 16 bytes) is loaded by `FUN_002c9310` and searched by `FUN_002c9320` (binary search, key at +0): used by the event
   executor (`FUN_002ce0b0` and `FUN_002ce808` call it) [V that it is a sorted key table; what the keys mean is [GAP]]. `link` (66 records) is read by
   the post-flag hook `FUN_001d39d0` [GAP].

## 7. Where a landing goes

- **Land prompt** [V]: `FUN_001c1cf8(world)` searches the `wlds` entry (34 records of 4 bytes, count at +4, records from +8; stored by `FUN_001c1ce8`
  in `DAT_01d5df68`) for the first record whose byte 0 equals the world and byte 1 equals byte 1 of that world's record (`FUN_001b4e80`, section 6.4).
  Byte 2 is the destination area when below 0x33, else a story-program key resolved with `FUN_00198080(key, world)` (docs/events.md `07localset`),
  byte 3 the entrance; the result is (world, area, entrance, 0xff, 0xff, 0xff). A world with no matching record returns the previous contents of the
  static result buffer (0x1d5df70) [V by reading; [I] that every reachable (world, state) pair has a record].
- **Destination list** [V]: `FUN_002d1d20(window, world)` reads the `land` table of `70landing.bar` (header word 1, count 53, records of 4 bytes: world
  point index, world, row in the list, picture index); for each record of the chosen world whose world point was seen it fills a 0x1c0-byte row:
  the label is the area's name from `msg/us/place.bin` (`FUN_0019e178`), the picture is the type-0x18 entry of `70landing.bar` number `picture index`
  (`FUN_002f7cc0`, 53 pictures named by area, 17472 bytes each), the destination is `FUN_00194f08(world point)` = the 3-byte record in `00worldpoint.bin`
  (docs/events.md "world points", 54 records of 4 bytes) [V]. The same file's `ws_i` entry (type 0x19, 1160 bytes, `FUN_002f7c90`) is the world select
  indexes [GAP].
- **Fade and flags**: `FUN_00104e18(dest, 1, 0, 0, 0)`, i.e. the normal black fade (fade kind 1), wait 0, no extra flag, party reset 0, so the destination area
  starts with the program bytes of its own program table [V].
- **Leaving without a landing**: the cancel results do nothing but close the window (`FUN_002cadf0`); the map itself is left only by a landing, a scripted event or
  the pause menu's own exits [GAP: the map's menu (captions 0x4c10..0x4c14) is opened from the cursor's input state, not found].
- **Gummi route markers**: choosing one starts a Gummi flight (the `ovl_gumibattle.x` overlay, docs/final-mix.md does not cover it); not in scope [GAP].

## 8. Recorded gaps

| gap | searched |
|---|---|
| cursor movement, camera type 4 behaviour, how the cursor is blocked by the symbols' radius | `FUN_00176e00`, `FUN_00183730`, `FUN_002cc8a0`, states in the vtable at 0x36fb00; only the confirm path and `FUN_002cca98` read |
| which button opens the map's command menu (captions 0x4c10..0x4c14) | `mips_scan --li 0x4c10` and 0x4c11..0x4c16 find only 0x4c16 in `FUN_002cb508`; the captions come through a table |
| who passes explicit program bytes to world 0xf | `07localset` world 15 has 0 pairs; world-15 flags have empty lists; scripted jump records not scanned |
| callers of `FUN_002c4c58` (event commands 0x100..0x113) | `mips_scan --jal`, `--addr`, and a scan of data words for 0x2c4c58 found nothing |
| the meaning of the `link`, `wmky`, `ws_i`, `md_m`, `50wmentry`, `50worldmap` files | layouts only (sections 2 and 6) |
| fade argument bit 31 | `FUN_0013ef68` not read |
| the contact test of `FUN_002cc9d0` | WP-WM5: unread (a distance stands in) |
| the map panels' art, layouts, texts and the episode list | done in WP-WM6 (section 10 "W6 art"); the remaining gaps are listed there |
| symbols' `FUN_002ccd88` event details and the `FUN_002c6a40` task | done in WP-WM7 (section 10 "W7 events"); the camera, the voices and the other 11 events stay gaps listed there |

## 9. What the app has

- Jump machinery: `area_jump.rs` (`WORLD_MAP`, `map_entrance` for the pair table, `type4_dest`, `resolve_record`, `JumpExtras`), the jump task with fade kinds,
  `Dest.programs`, `kh2_ard::localset`. `jump_target` returns an error for `WORLD_MAP` ("the world map is not available"); the type-4 harness `KH2_JUMP=4:..`
  exercises it.
- Area loading: `area.rs` knows only Twilight Town (`area_name(world, area)` is `tt<NN>` for world 2 and `None` otherwise); the loader `assets::load_area` and
  the area-script run (`kh2_ard::run`) are generic over the file name. `AreaRecord` holds world, area, entrance and the three program bytes.
- Story state: `progress::Story` (flags 0x33f6b0, program table 0x33e86f, exit locks 0x33f918, side-effect ops 1..5, 7, 0xc, 0xd); since WP-WM1 it also
  keeps the world records at 0x341858, the world flags 0x3418a4, the queue 0x3418c4 and the episode bits (stage W2 below); ops 6, 8, 9 and the hooks after the list
  except `FUN_001b6da0` are not ported (`Story::unported`).
- Panels, prompts and the landing: `worldmap_ui.rs` (W6, W7 and the art done below); the map is entered and left by default (`KH2_WORLDMAP=0` hides the save point row).
- Save point menu: `menu_save.rs` opens the World Map question on row 1 and enters the map (W3 done below); `savepoint.rs` builds the row (message 0x8a80). The menu kind 1 page machine
  and the card pages are ported (docs/menus.md 18.5).
- Fader: the screen-fader fade-out of a jump (`MenuScene::fade`, docs/menus.md) and the event player's fade object.
- Text and art readers: `kh2_image::text` (message tokens), `Seqd` (field-2d groups), `kh2_image::layd` (`.2ld`), the HUD scene (`hud_gfx`: sequence players, parents and anchors) for the panels.
- `kh2_ard::worldmap` parses `wldf`, `wlds`, `epsd`, `70landing.bar`'s `land`, `00worldpoint.bin` and the `wm.bar` ids (stage W1 below); nothing draws the symbols, cursor or markers, and no map area exists.

## 10. Porting plan

Each stage ends with a test that does not need the next one. File names are the likely owners; a stage that needs another owner says so.

**W1. Data readers (no behaviour).** `crates/kh2-ard` (new `worldmap.rs`; export from `lib.rs`): parse `wldf` (147 x 16 bytes), `wlds` (34 x 4), `epsd`
(43 x 8), `70landing.bar` (`land` 53 x 4, pictures), `msg/us/wm.bar` ids, `00worldpoint.bin` if not yet a reader. Disc tests with counts and layouts
(skip without `KH2_ISO`). Risk: low; bound every count (WP-KM1 lesson, no unbounded allocation).

**W2. World records and flag hooks.** `crates/kh2-app/src/progress.rs`: add the 4-byte world records (0x341858), the world-flag bits (0x3418a4), the
episode bits (0x3419e4 / 0x3419f4), the queue (0x3418c4), `FUN_001b6da0`/`FUN_001b68f8`/`FUN_001b6a90` and flag op 0xd. Proof: extend
`story_flag_setter_matches_the_game` (kh2-ee oracle of `FUN_0017ff60`, which already runs the whole setter) to compare these regions for all 19 worlds' flags.
Risk: medium; the setter's list runs `FUN_001803b0`.. and the `link` hook (`FUN_001d39d0`) whose effects are [GAP]; the oracle will show them. Also the save
file (`Progress::story_json`) must carry the new regions.

**W1 and W2 done (WP-WM1).** What was built and what the disc showed:

- `crates/kh2-ard/src/worldmap.rs`: `Wldf` (147 records of 16 bytes; sorted by the index byte, so the game's binary search `FUN_001b68c0` / `FUN_0030ff40` is ported
  as `Wldf::find` with the same probe order; action lists as `Action { op, data }`), `parse_wlds` (34) with `landing` (`FUN_001c1cf8`'s search, area below 0x33
  or a story-program key), `parse_epsd` (43), `parse_land` (`land` entry of `70landing.bar`, 53 records, then `ws_i` type 25 of 1160 bytes and 53 pictures of 17472
  bytes tagged by area, `AL02` .. `TT27`), `parse_worldpoints` (54 records of `00worldpoint.bin`, at most 0x36), `message_ids` (`wm.bar`, 207 ids; every id in
  section 5 that was tested is there: 0x4871..0x4884, 0x4a4d..0x4a55, 0x4c0d, 0x4c16, and every `epsd` title). Every count is the smaller of the header count and
  the records the bytes hold; a list command that runs past the entry ends the list. The disc test is `the_world_map_tables_have_the_documented_counts_and_layouts`.
  On the disc: 131 of the 147 `wldf` records carry an action list, 235 commands in all, ops 1..10, 12, 13 and 15, all kinds are 0..2.
- `progress.rs` (`Story`): the region 0x341858 ..= 0x341a05 as one byte array (`progress::world`), so a record index past the 19 worlds lands in the next
  field as the game's pointer arithmetic does. Story op 0xd writes record bytes 1 and 2 of the flag's world. After every set, `world_flags_follow` is
  `FUN_001b6da0`: each `wldf` record whose story flag matches runs `world_flag_set` (`FUN_001b68f8`: the four rival indexes 0x25/0x2a/0x2d/0x2e, the bit, the
  queue at 0x3418c4 with its count byte 0x3419c3 as the last queue byte, no queue for a kind 1 record while story flag 0x102b is set). `world_run_list` is
  `FUN_001b6a48` / `FUN_001b6a90` for ops 1..7, 0xc, 0xd, 0xf; nothing in the app calls it yet (the original runs it from `FUN_001b6e60`, which walks a
  list of world flags, and from `FUN_002c6bf8`). The region is saved as the hex string `world_state` (slot files, `story_json` when not all zero); a
  slot written before this has no such key and loads it clear.
- Proof: `story_flag_setter_matches_the_game` now also runs the real `FUN_001b6da0` with the disc's `wldf` entry at `DAT_01d5d790` and compares the whole region
  after every flag of every world, from a clear block and with story flag 0x102b set first (2830 flag runs; 1691 touch the region, 1572 queue a world flag).
- The other calls after the list were run unstubbed one at a time in the oracle (a throwaway edit, not committed): `FUN_001803b0`, `FUN_00180450`, `FUN_001805d0`,
  `FUN_001d39d0`, `FUN_001d3238`, `FUN_001be9c8`, `FUN_001d5718` and `FUN_00195130` ran without error and left the flag words, program table, exit locks and world
  region equal to the port for all 2830 runs, so none of them writes those regions in a block that is otherwise clear [the runs had no menu or actor state, so
  effects that need them are not excluded]. `FUN_00180558` is the exception: when story flags 0x102b and 0x1822 are set and 0x1807, 0x1809 are clear it sets story
  flag 0x1809 (0x1865 set) or 0x1807 through the setter itself; that is a story-flag effect outside the world region and is still not ported.
- Gaps kept: ops 8..0xb of the lists (`FUN_001b6d30` writes a byte of an entry `FUN_001c0950(id)` finds: whose table is unread), the episode list removal of op 0xd
  (`FUN_003335d0` on `DAT_01d5df60`, unread), the `FUN_001c6d10` call of op 0xf, the compare function `0x332e08` of the `wldf` search (no decompilation; a plain
  compare reproduces the oracle), the `link` hook `FUN_001d39d0` (no effect on the compared regions, effects elsewhere unread), the `wmky` and `ws_i` meanings,
  and the value of address 1 that `FUN_001b68f8` would read for a key the table lacks (taken as 0 [inferred]; the story hook only passes indexes the table holds; the other callers `FUN_001b6ec0`, `FUN_001d3820`, `FUN_0016d788`, `FUN_001b6870` were not read).

**W3. Entering from the save point and from type 4.** `crates/kh2-app/src/menu_save.rs` (page 0x10 question, page 0x14 fade wait, scene mode word) and
`area_jump.rs` (the mode-1 entry `FUN_00103b38` -> `FUN_00149df0(-1, 0x80000000)`, `Outcome` for the map). Needs W5 for the arrival. Until then
the log line stays but names the exact next step. Risk: low.

**W3 done (WP-WM4).** `menu_save.rs`: row 1 opens the question of page 0x10 (`Flow::map`; text 0x3859, Yes/No 0xa7c/0xa7d; No and cancel return to the list); Yes enters `Stage::MapFade`, 30 ticks of black over the open menu (`JumpMenus::map_fade`, painted by `area_jump::draw_fader`; `FUN_002580b8` = `FUN_0014a7c8(fader, 0xf, 0)`, `FUN_002580e8` waits for the fader's state 4), then the menu ends and sets `JumpTask::FadeOut { dest, kind: 1, t: FADE_TICKS - 1 }`: the jump task starts at black (bit 31 of the fade flags, as the after-event jumps do). The destination is world 0xf, area 0 and the entrance of the first pair of the table at 0x3790a8 (read at run time into `ExitConsts::map_entrances`) whose world is the current one (`FUN_00149db8`, argument -1 = the current-place world byte). Deliberate deviation: all of this works only with `KH2_WORLDMAP=1` (default off; the original always lists the row): the map has no landing or exit yet (W6, W7). Without it the row is not listed and `jump_target` keeps refusing `WORLD_MAP`; with it, type-4 records and `KH2_JUMP=4:..` load `wm00` too. The arrival is the ordinary area load of WM2/WM3 (symbols, cursor, hidden party). Gaps: the event signal 0x21 (`FUN_00197b70(0x21, 0)`) is not raised, the trailing arguments (0, 0, 0) of `FUN_00104e18` (wait, party words, reset) leave the party untouched, and the question's start cursor is [inferred]; its row order is proved from the widget records (docs/menus.md 18.5).

**W4. The map area loads.** `area.rs` (`area_name` for world 0xf -> `wm00`, prefix table of 19 worlds `WORLDS`), `assets.rs` / `kh2-map` for the 5.6 MB
map (two large parts, collision, sky, BOB props), the `map` programs (visibility masks), the manager creation (opcode 0x15 with type 0xc: camera type 4
and party reset 1, `FUN_001d5048`). Risks: the map reader on a file this size (run `kh2` tools only through `local/capped.ps1`), memory and draw cost,
BOB prop formats (`BOB` pairs, type 30 empty entries), hiding the party (class of the three friends) while the manager exists [GAP].

**W4 done (WP-WM2).** The map area loads through the ordinary field-area path; nothing else was needed in the map reader.

- `area.rs`: `WORLDS` (the 19 two-letter prefixes in world-byte order, the `ftst` column order of docs/events.md; world 0xf is `wm`), `world_of(name)`,
  `area_name(0xf, 0)` = `wm00` (still only Twilight Town and the map). `run_area_script` now sets `AreaRecord.world` from the loaded file's prefix, so the
  program table row is world 0xf's (`kh2-game wm00` used to take world 2's). `jump_target` still refuses `WORLD_MAP` (W7 removes that).
- `map/us/wm00.map` is a BAR whose part list is: one `MAP` pair (type 4 of 576912 bytes, textures 459128), `wm_0` (types 5, 14, 18), `ID_w` (type 6, the
  collision), `CH_w`, `COL_`, `SK0` pair, `out` and 33 `BOB` triples (type 4, 7, 30; type 30 is empty). The 5.6 MB are the props; the reader takes the
  first `MAP` pair, `SK0`, and the first type-6 entry, as for every area, and needed no change. The BOB props are not drawn (they are objects: W5, [GAP]).
  Run: 1185 meshes, collision 1730 triangles (COCT), camera collision 480 triangles; the process stayed under the 3500 MB cap; about 250 frames per second
  with `KH2_FPS=1` (debug build with optimisation, no enemies).
- Programs: the table gives (0, 0, 0). Map program 0 shows visibility mask 0x3, loads `m_00`, `m_01`, `m_50`, `m_51` (13, 9, 12 and 9 entities, none created yet),
  creates the manager and runs the `evt` program's command 0x16 (not ported).
- Manager (opcode 0x15, type 0xc, `WORLDMAP`): the run keeps `battle_manager`; the app logs "wants camera type 4 and party reset 1; not applied (gap)". Camera
  type 4 has no behaviour decoded (docs/camera.md lists types 0..3; `FUN_00177128` was not read for type 4) and the party reset code 1 (`FUN_001abfe8`) has no
  hook in `party.rs`. Both are gaps; the screen uses camera type 0 (follow) and the party is still placed (the party slot bytes log says world 2: `party.rs`
  does not read `AreaRecord.world`, outside this package). Sora and the HUD are visible on the map: hiding the party while the manager exists is [GAP].
- Harness: `kh2-game wm00` (with `KH2_AREALOG=1`) logs `map/us/wm00.map: 1185 meshes, collision 1730 triangles (COCT)` and the manager line. A screenshot at 10 s shows
  a dark blue sea with ripple rings and coloured ring shapes with the green and blue sky pieces left and right, Sora in front of the follow camera; the symbols,
  the cursor and the clouds of the real screen are objects or not drawn here.

**W5. Objects.** New `crates/kh2-app/src/worldmap.rs` plugin (add to a bundle in `plugins.rs`): spawn the 12 symbols with their state-driven look (section 3),
the cursor at its entrance record, the nine route markers; the `FUN_001981a0` entrance filter is already in `area.rs`. The cursor needs the world-map input
state (stick movement, near-object test `FUN_0019a3f8`/`FUN_002cca98`) which is [GAP]: stage a first version that moves the cursor like Sora's stick walk
and read the real states under the oracle. Risk: high; unknowns are movement, camera type 4 and the model set (`WM_CURSOR` mset).

**W5 done (WP-WM3).** `worldmap.rs` (`WorldMapPlugin`, in the world bundle) spawns the map's objects from the groups the `map` program loaded.

- Classes: objentry rows of type 0x11, told apart by the second byte (0 symbol, 1 marker, 2 cursor; `FUN_002c58c8`). Models and motion sets come from the row (`ModelName`, `AnimationName`), nothing is named in code. `entity_for_entrance` keeps one cursor record (entrance 0: object 0x3a1 at its `m_00` record; `KH2_ENTRANCE=3` puts it at another).
- Symbols: the world comes from the 12 pairs at 0x383338 (second word, low byte), the look from `Story::world_record(world)` byte 0 and byte 3. Correction of section 3 by reading `FUN_002c66b8` again: states 0 and 1 play motion slot 0xa1, 2 and up slot 0x97 (`FUN_0017e2d8(0, 0, obj + 0x140, slot)`); the colour word at 0x383398 + 4 * state is stored at +0x3ac (state 0 is 0, state 1 is 0x80202020, 2 and up 0x80808080; 0x80 is full); byte 3 bit 0 creates effect 3 (`FUN_0016a530(obj, 3, 1, 0)`, deleted when clear), state 3 and up creates effect 5 (`FUN_0016a570`), and bit 1 of byte 3 only decides a flag bit (+0x108 bit 6) together with state above 1. The app applies the colour as a material multiplier (rgb) and does not draw a state-0 symbol, because its colour word is 0 [I: alpha 0 means invisible]. Effects 3 and 5 and the TWTNW ring (kind 6, or 8 with story flag 0x481c, `FUN_002f7cf8`) are logged and not drawn [GAP]. Unit test `a_symbol_takes_its_motion_and_colour_from_its_worlds_state`; the disc test `the_symbol_table_and_colours_read_from_the_executable` shows the twelve distinct worlds (0x3a2 is world 2, 0x456 is world 0x12) and the colour words.
- Route markers (`WM_GUMI_01`, nine records of `m_51`): the objentry row exists, but `kh2_model::mdlx` returns nothing for `obj/WM_GUMI_01.mdlx` (the file reads, the reader does not accept it) and the disc has no `.mset` for it, so no marker is drawn [GAP: the reader, not owned here]. **WP-WM4 checked this: it is not a reader gap.** `obj/WM_GUMI_01.mdlx` (176 bytes) is a BAR with one entry, the collision shapes (`g_wm`, type 23, 144 bytes), and no model block (type 4) or texture: like the save point models it carries no mesh (`kh2 bar obj/WM_GUMI_01.mdlx`, `kh2 model` says "not a supported model"; `WM_CURSOR.mdlx` has `g_wm` 4, `tim_` 7 and `g_wm` 23). The marker's look is the effect file `obj/WM_GUMI_01.apdx` (54976 bytes, `kh2 pax`: two elements, effect 0 and 1 with `pppDrawMdl3` models of their own). The object's code (`FUN_002c8b08`, vtable 0x36f920: update `FUN_002c8b50`) takes a route number, reads the route's state byte from `FUN_001c0950(route)` (the Gummi route state, not ported) and, for state 1 or 2, clears or sets flag bits at +0x108 (`FUN_002c8bc8`), toggles three scene entries of the table at 0x36f9a8 (`FUN_002c8d18`) and starts effect element 0 when the state changes from 1 to 2 (`FUN_002c8c48`: `FUN_0016a570(obj, 0, 1, 0, 0)`). Which element is idle is not decided by this code, and the route states exist only with the Gummi project (W8), so the nine markers stay undrawn [GAP: `FUN_001c0950`'s table, the idle effect]. No reader change. Their update (`vtable 0x36f920`) was not read; a drawn marker would stand in its rest pose.
- Cursor [I]: the real states were not read. The first version puts Sora at the cursor's record (on the collision floor under it) and draws the Gummi ship at Sora's place with his yaw, so the stick walk moves it and the follow camera follows it (camera type 4 is still a gap); Sora's body and weapons and the party members are hidden. The model is drawn in its rest pose (no motion slot is known). `KH2_WMLOG=1` prints every object with its look.
- Party: `party.rs` takes the slot row's world from `DriveWorld`, not from `AreaRecord.world`, so the one-line fix is not trivial (the resource is built at start-up from the save); not changed. The party reset code 1 (`FUN_001abfe8`) is unread, so the members are only hidden.
- Harness: `KH2_STATE=<json with world_state> kh2-game wm00` shows symbols in different states (a state file whose `world_state` sets byte 0 of several worlds' records to 0..3).

**W6 and W7 done (WP-WM5).** `worldmap_ui.rs` (`WorldMapUiPlugin`, in the world bundle) adds the contact panel, the window and the landing; `area_jump::worldmap_enabled()` now defaults to on.

- Contact [I]: `touched` takes the nearest visible symbol within `CONTACT_RADIUS` (1500 cm on the ground plane, between the cursor's stand-in, Sora, and the symbol objects; the original's test is the actors' collision, `FUN_002cc9d0`, unread; the symbols are at least 4000 cm apart and the cursor's own spawn is about 1300 cm from its symbol). A state-0 symbol is not drawn and cannot be touched. The info panel (`FUN_002cf618`, removed again on leaving, `FUN_002cf698`) shows the world's name (the u32 table at 0x378040, `FUN_00105de0`, message of `wm.bar` or else `sys.bar`) and, when the world has an episode whose bit is set (`FUN_001c14e8(world, 0)` with the `epsd` bits, `Story::episode_seen`), its title and hint ids [I: which of the two texts is title and which is hint]. `FUN_002c9548` / `FUN_002c9580` read their two ids from a table of 12-byte records (`FUN_002a1c10(1)`, key byte 0 = `FUN_00277c88()`, story flag at +10, ids at +4 and +6) whose file is unread [GAP]; the episode records stand in for it. A fresh game has no episode bit, so the panel shows the name only.
- Window (`Window`, `Mode`, `Row`): the confirm key on a touched symbol (J or the left mouse button [I]) opens the area list when the world has rows (`area_rows` = `FUN_002d1d20`: the `land` records of the world whose world point is seen, `Story::world_point`, `00worldpoint.bin` record as destination with the program bytes kept, row order by the record's row field, locked when `ExitLocks::locked`), else the land prompt: row 0 the "Land in <world>" text (u16 table at 0x383520) with the landing destination, row 1 "Maybe later" 0x4884. Up and Down (and W, S) move the row cursor with wrap, confirm takes the row, K, Escape and Backspace cancel [I]; `Outcome` is `FUN_002cacd8`'s results (row 1 and cancel close, row 0 lands). A confirmed locked row shows message 0x4c0d until any confirm or cancel. While the window is open the keyboard and mouse are held for it (the pad does not move Sora). The forced events of `FUN_002ccd88` (TT symbol with world flag 1 set and 0x13 clear, TWTNW with 0x35 set and 0x37 clear) were not played here; WP-WM7 plays them (section 10 "W7 events").
- Draw (superseded by WP-WM6: the panels now draw from `wm_ws.2ld`, below): the HUD scene's message window frame (`kh2_image::window` kinds 3 for the info panel and 0 for the window, text font 3, colour 5) with the lines as one text; the row cursor is the selected row in a gold colour [I]. The art of the original panels is not drawn: `field2d/us/wm0command.2dd` was dumped (`dump2dd`, 98 groups of the camp command deck: "COMMANDS", "PARTY", ... with the green strips) and holds none of these windows, `wm0field.2dd` is `fiel`, `gaug`, `mapj`, `miss` HUD art, and `file/us/wm_ws.2ld` (the 7531..756c sequences) was not opened [GAP]. The destination pictures of `70landing.bar` (53 of 17472 bytes) are not drawn. The area list's caption is 0x4c15 "Select destination" [I]: the disc has 0x4c10 / 0x4c11 (button icon first) and the plain 0x4c15 / 0x4c16 "Select destination" / "Episode list", and `FUN_002cb508`, which uses 0x4c16, therefore builds the episode list (mode 2), not the area list as section 4 read it. The episode list (modes 2 and 3) is not ported. Menu sounds (`FUN_002d1760`) are not requested (every menu sound is muted, `menu_sound::MENU_SOUND_ENABLED`).
- Landing (`land_destination`, `FUN_001c1cf8`): the first `wlds` record of the world whose byte 1 equals byte 1 of the world's record (`Story::world_record`); an area of 0x33 or more is a story-program key resolved by `07localset` (`area_jump::disc_localset`) with entrance 0 and the key in the three program bytes (`FUN_00198080` read: it returns the key in bytes 3..5, entrance byte 2 = 0). On the disc world 2 has records for the keys 1, 9 and 10 only (areas 52, 107 and 119 of the program numbering). The app's baseline story has key 0 in every record (the new game's flag side effects are not run), a state no record has: `land_destination_or_first` then takes the world's first record [inferred], and `worldmap::symbol_state` shows Twilight Town normally while its record is 0 [inferred; otherwise the map could be empty and unreachable]. The landing is `JumpTask::FadeOut { dest, kind: 1, t: 0 }` (`FUN_00104e18(dest, 1, 0, 0, 0)`; the original starts it in the window's teardown), started only when the destination is an area the app has (`app_has`: Twilight Town; every other world is absent, not locked [I]); otherwise the window stays and the log says why. Sora's body, hidden while the cursor stands in for him, shows again once the map is left (`map_restore_sora`).
- No exit without a landing exists in the original (section 7). App-only safety nets [inferred, not the original's behaviour], added after review: (1) `worldmap::symbol_state` shows the Twilight Town symbol in its normal state whenever its own record is 0 (the baseline, or a story that revealed only the other worlds), because the app can land only there and a hidden symbol left nothing to touch; (2) Backspace with no window open returns to the area the map was entered from (`MapUi::from`; Twilight Town area 1 when unknown) and logs a warning. The selected row of the window is drawn white against the dark idle rows (the original's pointer group is unread).
- The gate is removed: `KH2_WORLDMAP` is on unless it is `0` (docs/menus.md 18.5). Proof: harness runs below; tests `worldmap_ui::tests::*` (rows, outcomes, lookup, contact, objective, list tokens, the disc test `a_fresh_games_twilight_town_land_prompt_returns_the_tables_entrance` with the `wlds` bytes read independently, the lock regression `the_story_lock_is_free_when_the_exit_locks_are_read`) and `worldmap::tests::a_hidden_twilight_town_shows_normally_and_other_worlds_follow_their_records`.
- Harness: `KH2_WMLOG=1` logs the contact, the window rows and the landing. From the Usual Spot save point: `KH2_REACTION_PRESS=8 KH2_SAVEPOINT_WATCH=5 KH2_REACTIONLOG=1 KH2_NO_ENEMIES=1 KH2_KEYS=11:Down,12:J,13:Left,14:J,24:J,25:J kh2-game tt02` goes to map entrance 1 (the Twilight Town symbol), shows the info panel, opens the area list with "The Usual Spot" (point 47 is seen at that save point), lands with `FUN_00104e18(world 2 area 2 entrance 99, 1, 0, 0, 0)` and shows tt02 again with Sora visible. Direct: `KH2_STATE=<state with world_state records at 2> KH2_ENTRANCE=1 KH2_KEYS=7:J kh2-game wm00` shows the land prompt ("Land in Twilight Town" / "Maybe later...") and `KH2_KEYS=7:J,8:J` lands in `tt01` (program key 52 resolves to area 1, programs 0x34).

**W6 art (WP-WM6).** `worldmap_ui.rs` draws the panels from `file/us/wm_ws.2ld` and the destination pictures of `70landing.bar`; `kh2_ard::worldmap::pictures` / `WS_I` read the pictures. Everything below is [V] from the executable and the disc unless marked; no value is in the code that the disc supplies (the layout table, the descriptors and the group sets are read from the executable at run time).

- **Files.** `wm_ws.2ld` is a BAR of one IMGZ (two 256x256 images) and one LAYD (72 layers of kind 1, 11 layouts, two sequences of 20 and 154 groups); the layers are all of sequence 1. `70landing.bar` has `land` (type 2), `ws_i` (type 25, a SEQD of 13 groups drawing a 128x128 picture: group 0xb is the 75x61 thumbnail, 9 a larger view, 10 and 12 variants) and 53 type-24 IMGD pictures of 17472 bytes; a `land` record's last byte numbers the picture among the type-24 entries (`FUN_002f7cc0`).
- **Layouts per mode** (the u16 table at 0x383458, four per mode: open, close, idle, 0xffff; `FUN_002ca2a8`): mode 0 layouts 0, 2, 1; mode 1 3, 5, 4; mode 2 6, 9, 8; mode 3 10, 5, 4. The info panel (`FUN_002ce808` -> `FUN_002cf3f8`) uses the entries of mode 0. A layout object plays its open layout, then the next one (`FUN_0023bbb8`) when it has played; the port swaps the whole layer set after the longest limited group (17 frames, 0.28 s) and plays the close layout when a window closes. Layouts 0 and 1 are the veil (a green scan-line vignette, groups 3 and 4, with the title banner groups 88, 89 (the glowing title) and 90); layouts 3, 4 and 10 are the area list screen (veil group 6/7, title, the card frame group 103/104/105, a blue pill 78/79/80, a green pill 82/83/84, two green plates 97..102, a gold line 68/69/70); layouts 6 and 8 the episode screen (blue veil 9/10/11/12, the back pill 85/86/87); layout 7 duplicates 10 and no table entry names it. The ids in the descriptors and group sets count the groups of all sequences (`FUN_00238f48`: sequence 0 has 20 groups, so id 0x58 is sequence-1 group 68).
- **Text objects** (`FUN_0023bcd0`, nine records of 0x20 bytes at 0x383548: triples (open, close, idle) of group ids for the modes 1, 2 and 3 at +0, +8 and +0x10, the message id at +0x18, 0xffffffff for none): each follows the layer of its id in the current layout. Messages of `wm.bar`: 0x4c11 on the blue pill (an R1 icon and the switch caption), 0x4c14 on the green pill (a triangle icon and the journal caption), 0x4c12 and 0x4c13 on the two plates (their captions), 0x4c10 on the back pill (an L1 icon, mode 2 only), 0x4c15 on the card frame (the list's caption); the descriptors without a message follow the two plates and carry the plates' bodies (below). A descriptor whose layer is an empty group (1 or 2) shows nothing.
- **Land prompt** (mode 0, `FUN_002cbcf8`): two bars (groups 0x3a open, 0x3b idle, 0x3c close; selected 0x39 with the pointer hand) `unk20` apart, the text starting at the group's x. The info panel and this window both use layouts 0 and 1, so both veils are drawn.
- **Area list** (modes 1 and 3, `FUN_002cb508`): per row a card of group 0x37 (locked marker 0x38) in a two-column grid, `unk20` across and `unk1c` down, starting at the frame layer's anchor plus the frame group's x and y; the card shows the area's picture (`ws_i` group 0xb); the selected area's name is on the gold line; the plates' bodies are the journal's two story texts of the world (`FUN_002c9548` / `FUN_002c9580` -> `FUN_002a1e98`: the last record of `jiminy.bar` entry 1 of the world's journal index (`FUN_00277c88`) whose story flag is set, `menu_journal::story_texts`; the ids are looked up in the map's table, the system table and the journal's [I]); when no record applies the original shows the fallback message (a placeholder of the system file), left empty here [I] because the app's baseline story has no flag set. A locked row shows a red plate (groups 0x5b, 0x5c, 0x5d) with message 0x4c0d.
- **Episode list** (mode 2): the panel (groups 0x5e, 0x5f, 0x60; the original hides it in modes 1 and 3 by giving it group 1) with a caption (0x4c16) and up to three rows (group 0x34, 90 apart; `FUN_002c95b8` counts them, the title is `FUN_002c9600` = the record's +4 text, the hint `FUN_002c9678` = +6) with a "new" marker (group 0x11) on rows whose episode is in the second bit array (`FUN_001c1628`); the scroll bar groups are the table at 0x383508 (up arrow 0x3e, bar 0x3d, down arrow 0x3f; the panel art carries its own track). The rows are children of the panel's anchor; the hint is drawn by the callback `FUN_002cb3e0`.
- **Info panel**: the veil, the name plate (group 0x97, then 0x98) with the world's name. The original adds a level number (`FUN_001bea40`, two digits), the time spent in the world (`FUN_0014c9b0`: hours and minutes) and a row of boxes from the `wmky` record (`FUN_002cf6d0`, groups 0x94 and 0x95): ported by WP-WM7 (section 10 "W7 panel digits, sounds and the journal"; the earlier text called the second value a three-digit and a two-digit number, they are the hours and minutes). WP-WM5's objective and hint lines in the info panel were wrong: `FUN_002c9548` / `FUN_002c9580` belong to the window's plates, not to the info panel.
- **Input**: Up, Down, Left, Right (and W, S, A, D) move the cursor (the grid moves one card with Left and Right, one row with Up and Down, wrapping) [I: the wrapping], J or the left mouse button confirms, K, Escape or Backspace cancel, and E or Tab is the R1 button (mask 0x100, and 0x208 from the episode list) [I: the keys]; the triangle (journal, Y) only logs [GAP, WP-WM7].
- **Inferred**: every text position (the original anchors text objects on the layers; the offsets are read off the art: caption pills centred, the plate captions 95 right and 11 down from the plate's corner, bodies 26 down, the area name 40 right of the gold line's corner, row titles 5 and hints 33 below the episode row's top), the text size (the group's size field, `FUN_00138fc8`; a value above 32 is not taken for one), the colour 0xe5e5e5 (`FUN_00138fc8` stores 0x80e5e5e5), the card cursor (group 54, a red outline of the card's size; the original's list widget draws its own), the rows' missing open animation (they appear in their idle group) and the scroll bar position when there are more than three episodes.
- **Harness**: `KH2_WMLOG=1` logs every window state and the plate message ids; `KH2_WMLOCK=<area>` shows the world's rows of that area as locked. From a state with `world_points` 47..53 (the seven Twilight Town rows) and `KH2_ENTRANCE=1`: `KH2_KEYS=7:J` shows the area list, `7:J,9:E` the episode list, `7:J,8:E,9.5:E` the area list again (mode 3). A state with the `world_state` episode bits for ids 32 and 33 (world 2's two episodes) fills the episode list, and story flags 0x895 and 0x898 give the plates' bodies. Tests: `worldmap_ui::tests::*` (the window modes, the grid and its wrapping, the episode scroll, the items of each screen, the layer set's open, idle and close phases, the text objects' layers, and on the disc `the_panel_art_and_tables_on_the_disc_fit_together`) and `kh2_ard::worldmap::tests::pictures_are_the_type_24_entries_in_order` plus the disc test's 53 pictures.
- **Gaps** (searched): the original's exact text positions and its list widget cursor (`FUN_00296aa8`, `FUN_00296fb8`: not read); what `FUN_002ca3d0` stores at +0x2f0c (0x5f, the episode panel's next group, taken as its idle group). The sounds, the journal button, the info panel's digits and boxes and the two forced events were done in WP-WM7.

**W7 events (WP-WM7).** `worldmap.rs` plays the two forced events of `FUN_002ccd88`. Everything is [V] from the executable and the disc unless marked.

- **Start.** A confirm on a symbol asks `FUN_002ccd88(object)` first (the checks are in section 4): it starts the event through `FUN_002c6a40(event)`, answers 1, and `FUN_002ccca0` then removes the info panel (`FUN_002cf698`) and the route info (`FUN_002d0bc8`); a 0 opens the prompt as before. The map's `forced_event(object, flag)` is that function; the flags are the world flags (`Story::world_flag`, bits at 0x3418a4: byte 0x4c of the `world_state` region holds flags 0 to 7). The Twilight Town event repeats on every confirm until world flag 0x13 is set (it follows story flag 0x105d, `wldf` record 14), the TWTNW one until 0x37 (story flag 0x4830, record 45); nothing in the event sets a flag. `FUN_002c6a40` makes a task of priority 15000 (`FUN_00102bf8(1, 15000, 0x2c6df8, 0x4000)`), keeps `{task, event}` at 0x36f850, sets bit 0 of word `+0x2c` of the object at `[0x35c998] + 0xb40` (the manager's bit; the app has no such object, so "an event plays" is `WmEvent::busy`), hides pause-menu record 0xc (`FUN_001bef48(0xc)`), takes the players' control (`FUN_00196280`), hides the field HUD (`FUN_00196760`) and calls `FUN_00139b38(0xc9)`; the task body `FUN_002c6bf8` runs `FUN_002c6db0(event)`: `FUN_002c5428` makes the event object (0x6c bytes, `FUN_002c4a80`, which finds the program of the event number in the talk table with `FUN_001b6740` and the callback 0x2c54f0) and a destroy task, then yields until `FUN_002c5588` (0x36f6a4 clear) says the event is over; `FUN_002c6b58` undoes the bit, the control, the HUD and the pause record. The app holds the pad (`Pad::lock`) and hides the HUD nodes that were visible at the start (the map panels' own excepted, [inferred] as `event_player::field_hud_hide` does) and shows them again afterwards (screenshots below); the pause menu is not held [GAP].
- **The program.** `libretto-wm.bar` (9104 bytes, the `wm` talk table of docs/events.md section 3) has 50 records; the programs of 1836 (30 commands) and 1837 (32) use ops the talk reader runs itself (1 message, 2 wait in frames, 5 fade out, 6 fade in, `FUN_001b65e0`) and the executor `FUN_002c4c58` (ops 0x100 to 0x113; `kh2_ard::libretto::Cut` reads them): 0x100 load the model of an objentry id (`FUN_0016e638`), 0x101 load `libretto/us/wm<event>.bar` (`FUN_002c5598`, format 0x3832f0), 0x102 make the actor of an objentry id in a slot (four slots at `+0x2c`, `FUN_002c5108`, class `FUN_002c8f70`, the id or'ed with 0x40000000), 0x103 start the first listed entry of the file as the slot's animation and queue the rest (`FUN_002c5278`, `FUN_002f54d0`, `FUN_002f54e8`), 0x111 only queue (`FUN_002c5340`), 0x108 play the voice entry (`FUN_002c5630`, `FUN_002c1bc0`), 0x109 / 0x10a the actor's position and rotation (`FUN_002c53c0` / `FUN_002c5400`, object `+0x220` / `+0x210`), 0x10b / 0x10c the stage's position and rotation (`FUN_002c9160` / `FUN_002c91d0`), 0x10d / 0x112 remove one / all actors (`FUN_002c5198` / `FUN_002c5230`), 0x106 finish the load (`FUN_002c55e8`), 0x107 the second message window class, 0x104 a move task with a duration (`FUN_002c7198`, used by 502 and others, not by 1836 or 1837), 0x105 pause, 0x10e / 0x10f leave to a world (`FUN_00104e18`, used by 512 and 1823), 0x110, 0x113. Op 1 makes a message window (`FUN_002c5db0`: class 0x1ab6a8, or 0x1ab8e0 after op 0x107) and waits until its state is 7 (`FUN_002c6010`); fades do not wait, the programs follow them with an op 2 of the same 12 frames.
- **The files.** Each entry of `libretto/us/wm<event>.bar` is addressed by its index: type 0x11 entries are BARs of one motion (type 9) and its triggers (type 16), like an `.anb` (the app's `entry_motion`); type 0x30 entries are VAG voices (`wm30601sr`, `wm30602go`: the last two letters name the speaker, 1836 Sora then Goofy, 1837 Chip (`ci`) then Dale (`de`)). 1836 plays in the cockpit: `WM_COCKPIT` (object 0x862), Sora (0x54, `P_EX100`), Donald (0x5c, `P_EX020`), Goofy (0x5d, `P_EX030`), messages 0x4bb6 and 0x4bb7; 1837 in the hangar: `WM_HANGAR` (0x863) and `N_DC030_WM` / `N_DC040_WM` (0x895, 0x896; Chip and Dale by the voice names), messages 0x4bb8 and 0x4bb9. The two sets are huge (the hangar's bind pose spans 1560 by 850 by 1080 cm, the cockpit 118000 cm) and the characters small (Chip 30 cm), so the stage matrices of ops 0x10b / 0x10c are shot positions.
- **The stage.** `FUN_002c8ff8` makes each actor's world matrix its own (rotation from `+0x210`, position from `+0x220`) times the stage matrix at 0x1e35400 (the stage rotation at 0x1e353f0 and position at 0x1e353e0, `FUN_0017c670`). The matrix of three angles is `FUN_0011b568` (turns about x, then y, then z, `FUN_002fb580`, `FUN_002fb628`, `FUN_002fb4d8`). The app ([inferred] sign) uses the usual right-handed turns about the game's axes, then the game-to-app axis flip. Running `FUN_0011b568` in kh2-ee (an oracle test) was tried and dropped: the emulator's VU0 macro broadcast multiplies return equal rows for every angle. The sign was chosen by the picture: with it Chip (left, turn -20 degrees) and Dale (right, +20) turn toward each other and the stage's 10 degree pitch looks down on the consoles; the opposite sign shows the hangar floor from below.
- **The camera is a gap.** No command of the programs sets a camera, and the stage sits near the game's origin, where the map has the Disney Castle symbol (object 0x3a7 at (0, 432, 0)): with the camera at the origin the symbol's mesh fills the view (screenshot looked at). The app therefore puts the stage and a fixed camera far above the map (`STAGE_ORIGIN`, 200000 cm up), looking along the game's +z with the field camera's field of view, from the start of op 0x106 to the end [inferred; the original's camera for these events was not found: [GAP] whether it is the map camera, which the stage is relative to]. The stage's own scenery fills the view; the sky of the map is not behind it.
- **Timing and audio.** The 60 Hz fixed tick runs the program (`Script::step`). Voices are not played (the app has no VAG path; each `Voice` op is logged with its entry [GAP]); nobody has listened. Op 0x105's sound (0x3e88) and ops 0x104, 0x10e, 0x10f, 0x110, 0x113 are read but not run (1836 and 1837 use none of them): the other 11 events of the table (502, 503, 512, 521, 524, 1660 to 1662, 1823, 1824, 1838, started by the wldf queue player `FUN_002c6bf8` and by the story, not by a symbol) are not played [GAP, outside this package].
- **Harness.** `KH2_STATE=<json>` with `world_state` bit 1 of byte 0x4c set (flag 1; the other 18 records' state byte 2) and `KH2_ENTRANCE=1 KH2_KEYS=7:J kh2-game wm00` touches the Twilight Town symbol and confirms: the log shows `forced event 0x72d`, `event 0x72d (1837) starts`, the libretto file with 8 entries, the three actors, the voice and message lines, and `event 0x72d over` after two more confirms (J) for the two messages. `KH2_WMEVENT=1836` (or 1837) with `KH2_WMEVENT_AT=<s>` plays an event on the map without touching a symbol (the cursor's spawn for entrance 12 is 5100 cm from the TWTNW symbol). Tests: `worldmap::tests::the_symbols_start_their_event_only_while_the_world_flags_say_so`, `a_program_waits_where_the_executor_waits`, `a_fade_runs_from_one_alpha_to_the_other_over_its_frames`, `an_actor_stands_where_the_stage_puts_it`, the disc test `the_programs_of_events_1836_and_1837_run_through_the_executors_ops_on_the_disc` (both programs run to their end, every motion entry parses, every voice entry is a VAG), `kh2_ard::libretto::tests::world_map_event_ops_read_as_the_executor_reads_them`.

**W7 panel digits, sounds and the journal (WP-WM7).**

- **Level, time and key boxes** (`FUN_002ce808`, `FUN_002cf6d0`). The info panel makes two more labelled lines and a row of boxes, all plates of sequence 1 of `wm_ws.2ld` (local groups, not the cumulative ids of the layouts): the label plate 0x41 (then 0x42; the word "Battle Lv" in the art) with the level as two digits (groups 0x12 + d, one `unk20` = 16 apart), the plate 0x48 (then 0x49, "Play time") with the hours (groups 0x1c + d, no leading zeros, at most three), the colon (0x26, advance 8) and the minutes as two digits (0x1c + d). The digit groups are 21 by 19 cm of art; `FUN_002cc288(value, count, out)` writes the last `count` decimal places and returns how many places the value has. The level is `FUN_001bea40`'s table at the current world: the sum over the rows of `btlv` (`00battle.bin`, 20 rows of 32 bytes: u32 bit, u32 story flag, 19 level bytes, `FUN_001be908`, `FUN_001be8d8`) whose bit is set, a bit being set when the row's story flag is set (`FUN_0017ff60` calls `FUN_001be9c8(flag)`); the app has no latch of those bits and reads the story flag [inferred]. The time is the word at `0x33e860 + 0x160c + 4 * world` split in `FUN_0014c7f0` (216000 frames an hour, 3600 a minute); the app has no per-world clock and no writer of the word was found (`mips_scan --load-off 0x160c` and `--addr 0x33fe6c`: none), so it shows 0:00 [GAP]. The key boxes come from `wmky` (`00progress.bin`, 11 records of 16 bytes sorted by world: u32 world, three pairs of story flags): `FUN_002c9358` counts the pairs whose first flag is set (the boxes drawn), `FUN_002c93c0` the pairs whose two flags are set (the filled ones); box i is group 0x95 (filled) while i is below the filled count, else 0x94, children of the name plate at `x - size * i` (the group's x is 140 and its size 20) and the row moved by half its width [inferred: the shift field `+0x254`, whose parent and sign were not traced]. `kh2_ard::worldmap::parse_wmky`, `find_keys`, `KeyBoxes::counts`, `parse_btlv`, `level_sums` read them (disc counts 11 and 20 are asserted). Screenshot looked at (`KH2_STATE` with story flags 0x8af, 0x8b9 and 0x817): the Twilight Town panel shows "Battle Lv 21", "Play time 0:00" and one filled keyhole box at the right end of the name bar; without the flags the level reads 00 and no box.
- **Sounds** (`FUN_002d1760(k)` -> `FUN_002c3318(k)` -> `FUN_002c23e0(handle, k)` -> `FUN_002c2338`: `FUN_002c2258` splits `k` into `k / 1000` (the bank) and `k % 1000` (the entry)). So 1 to 5 are entries 1 to 5 of bank 0, the system bank `se000.seb`: the same sounds as the pause menu's cursor (1), confirm (2), open (3), cancel (4) and refused (5) (docs/menus.md 1.7, `menu_sound`). The call sites (instruction scan of `FUN_002ca6a0`, `FUN_002cacd8`, `FUN_002d0fe0` for the `li a0` before each `jal 0x2d1760`): a cursor move on any list 1 (including the episode scroll), a confirm on a selectable row or on "Land" or the switch button 2, the "Maybe later" row 2, a cancel 4, the locked row 5 and the notice box's close 4; 3 is not used by the window. The app plays them through `menu_sound::play` (`worldmap_ui::sound`), whose switch `MENU_SOUND_ENABLED` is off by the user's report of 2026-10-10 (`KH2_SOUNDLOG=1` logs `menu se 0:<id> muted`): nobody has listened, and they will sound when that switch is turned on.
- **The journal button is a gap.** `FUN_002c9510(world)` = `FUN_0024f3c0(0, FUN_00277c88(world))` (`FUN_00276730(index)` stores the journal world, `FUN_0024f0b8(3, 0)` changes to the journal scene), only on the area list (modes 1 and 3, not while the notice box is up). The app's journal opens only from the camp menu: `menu_journal.rs` takes its `request` from a camp row while `Menu::phase` is open, and `show` takes the world from the area's name (`wm` has no journal index). The button (Y, the key the menu pages use for the triangle [inferred]) is read (`Press::Journal`, `Outcome::Journal`) and logs the journal index of the world (`worldmap: the journal button ...`). Needed outside this package (`menu.rs` / `menu_journal.rs`): a public entry that opens the camp's journal at page 10 (mode 3) for a given journal index with the pause menu closed afterwards, and the map's `Outcome::Journal` arm calls it.

**W6. Panels and prompts.** `worldmap_ui.rs` (new) using `Seqd` groups and the list widget family: the world info panel, the land prompt (two rows), the
destination list with pictures, the episode list, the not-accessible box. Texts from `wm.bar`, area names from `place.bin`. Risk: medium (the
`wm0command.2dd` / `wm0field.2dd` sequence ids per mode are known as tables 0x383458..0x383570 but their shapes were not opened).

**W7. Landing.** `worldmap.rs` + `area_jump.rs`: the `wlds` lookup (W1/W2), the destination list from `70landing` and the seen bits, then
`FUN_00104e18(dest, 1, 0, 0, 0)` through the existing jump task. The app has only Twilight Town, so a first version lists only the Twilight Town rows and
shows the others as absent (not as locked: the original shows only seen points) [I]. Remove the `jump_target` error for `WORLD_MAP` only when W4 works.
Test: with the flags of a fresh game the land prompt of Twilight Town returns the entrance a table lookup gives.

**W8. Events and routes (optional, last).** The two forced events are done (WP-WM7); the other 11 libretto events (event player + `kh2_ard::libretto`, docs/events.md), the
Gummi route markers (they start the gumibattle overlay and are a separate project).

Dependencies: W1 -> W2 -> W7; W3 and W4 independent of W2; W5 needs W4; W6 needs W1 and W5; W7 needs all of W2, W4..W6. Smallest user-visible slice: W1 + W3 + W4 +
W5 (cursor and symbols, no UI) then W6 + W7.

## 11. Decompile index and evidence

Executable: `FUN_00149df0`, `FUN_00149db8` (entrance table 0x3790a8), `FUN_00103b38`, `FUN_00103b18`, `FUN_00103b08`, `FUN_00104e18`, `FUN_00104bd0`, `FUN_00104b90`,
`FUN_0019d9d8`, `FUN_00257ee8`, `FUN_002580b8`, `FUN_002580e8`, `FUN_00257e70`, `FUN_0016e8c0`, `FUN_002c58c8`, `FUN_002c64d0`, `FUN_002c66b8`, `FUN_002c8b08`,
`FUN_002ccf28`, `FUN_002cc8a0`, `FUN_002cc518`, `FUN_002cca98`, `FUN_002ccb70`, `FUN_002ccca0`, `FUN_002ccd88`, `FUN_002cce10`, `FUN_002cc1e0`, `FUN_002cbee8`,
`FUN_002cbcf8`, `FUN_002cb508`, `FUN_002cacd8`, `FUN_002ca6a0`, `FUN_002ca2e8`, `FUN_002cc4b0`, `FUN_002ca2a8`, `FUN_002cf618`, `FUN_002c6a40`, `FUN_002c4c58`,
`FUN_002c5598`, `FUN_002c57b8`, `FUN_002c5af0`, `FUN_002f7c00`, `FUN_002f7c90`, `FUN_002f7cc0`, `FUN_002d1d20`, `FUN_001b9338`, `FUN_001b8b10`, `FUN_001d5048`,
`FUN_0017fd10`, `FUN_0017ff60`, `FUN_001b4e80`, `FUN_001b4e98`, `FUN_001b6a18`, `FUN_001b68c0`, `FUN_001b68f8`, `FUN_001b6a48`, `FUN_001b6a90`, `FUN_001b6cb8`,
`FUN_001b6d30`, `FUN_001b6da0`, `FUN_001c1410`, `FUN_001c1488`, `FUN_001c14e8`, `FUN_001c1628`, `FUN_001c1cf8`, `FUN_001c1ce8`, `FUN_001c6d10`, `FUN_00195010`,
`FUN_00194f08`, `FUN_0019e178`, `FUN_00198080`, `FUN_001981a0`, `FUN_002c9310`, `FUN_002c9320`, `FUN_002c9548`, `FUN_002c9580`, `FUN_002c9600`; WP-WM7: `FUN_002c6bf8`, `FUN_002c6db0`, `FUN_002c6b58`, `FUN_002c6ba8`, `FUN_002c5428`, `FUN_002c5588`, `FUN_002c4a80`, `FUN_002c4bb0`, `FUN_002c5108`, `FUN_002c5198`, `FUN_002c5230`, `FUN_002c5278`, `FUN_002c5340`, `FUN_002c53c0`,
`FUN_002c5400`, `FUN_002c5630`, `FUN_002c55e8`, `FUN_002c5a28`, `FUN_002c5a78`, `FUN_002c5db0`, `FUN_002c6010`, `FUN_002c7198`, `FUN_002c8ff8`, `FUN_002c8f70`, `FUN_002c9160`, `FUN_002c91d0`, `FUN_0017c670`, `FUN_0011b568`, `FUN_002fb580`, `FUN_002fb628`,
`FUN_001b6740`, `FUN_002ccb70`, `FUN_002ccca0`, `FUN_002cce10`, `FUN_002d1760`, `FUN_002c3318`, `FUN_002c2258`, `FUN_002c23e0`, `FUN_002ce808`, `FUN_002cf6d0`, `FUN_002c9358`, `FUN_002c93c0`, `FUN_002c9510`, `FUN_0024f3c0`, `FUN_0024f0b8`, `FUN_001bea40`, `FUN_001be828`,
`FUN_001be908`, `FUN_001be8d8`, `FUN_001be9c8`, `FUN_001be818`, `FUN_0014c9b0`, `FUN_0014c7f0`, `FUN_002cc288`, `FUN_00237618`, `FUN_00237658`, `FUN_00237698`.

Disc: `kh2 bar ard/wm00.ard`, `kh2 bar msn/us/WORLDMAP.bar`, `kh2 bar 70landing.bar`, `kh2 bar map/us/wm00.map`, `kh2 bar msg/us/wm.bar`,
`kh2 extract 00progress` entries (BAR of 26 entries: 19 worlds, `dsa`, `wlds`, `epsd`, `dark`, `wldf`, `link`, `wmky`) and `kh2 extract 07localset.bin`, all
run through `local/capped.ps1 -CapMB 2500`; the decoding was done with throwaway scripts (not committed). Executable searches used `mips_scan.py --jal`,
`--addr`, `--li`, `--load-off` as listed in section 8.
