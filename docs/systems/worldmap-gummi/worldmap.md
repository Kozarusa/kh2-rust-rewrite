# World map (research, WP-WM0)

[Contract](contract.md) | [Index](../../README.md)

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
   and sets the area camera type `DAT_0035cd38` to 4 [V]. What camera type 4 does: nothing in the area init, see section 10 "W8" [V];
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
mode 2 is the **episode list**; mode 3 is the area list again, entered from mode 2 (WP-WM6: `FUN_002ca538` starts layout 10 and goes back to the area layouts, section 10 "W6 art"). The masks are the R1 icon (0x100; the icon is in the switch caption's message) and, from the episode list, L1 or cancel (0x208, the same for the "back" caption); the triangle (0x400, area list only, `FUN_002c9510` opens the journal, `FUN_0024f3c0(0, FUN_00277c88(world))`, mode 0) is ported by WP-JN4 (section 10 "W7 panel digits, sounds and the journal").

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
   | 8..0xb | `FUN_001b6d30`: for each id writes `op - 8` to `0x3419c3 + id` (`FUN_001c0950(id)` is that address): the route state byte read by section W8 (WM9) |
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
| cursor movement, camera type 4 behaviour, how the cursor is blocked by the symbols' radius | `FUN_00176e00`, `FUN_00183730`, `FUN_002cc8a0`, states in the vtable at 0x36fb00; only the confirm path and `FUN_002cca98` read. WP-WM8 (section 10 "W8"): the cursor's movement is the player class's own; area camera type 4 sets no camera target; the map's own camera object and the contact test stay gaps |
| which button opens the map's command menu (captions 0x4c10..0x4c14) | `mips_scan --li 0x4c10` and 0x4c11..0x4c16 find only 0x4c16 in `FUN_002cb508`; the captions come through a table |
| who passes explicit program bytes to world 0xf | `07localset` world 15 has 0 pairs; world-15 flags have empty lists; scripted jump records not scanned |
| callers of `FUN_002c4c58` (event commands 0x100..0x113) | `mips_scan --jal`, `--addr`, and a scan of data words for 0x2c4c58 found nothing |
| the meaning of the `link`, `wmky`, `ws_i`, `md_m`, `50wmentry`, `50worldmap` files | layouts only (sections 2 and 6) |
| route markers' idle effect (`WM_GUMI_01.apdx` elements 0 and 1), the writer of route states (wldf ops 8 to 0xb in `Story`), the route info (`FUN_002d0b30`) | WP-WM8 (section 10 "W8"): the props that carry the markers' look are drawn, the effect is not |
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

