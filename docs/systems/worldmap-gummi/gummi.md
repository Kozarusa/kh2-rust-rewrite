# Gummi missions: what the savestates hold (WP-DYN2, static read)

Source: the user's savestates `KH2_Gummi_Mission.p2s` and `KH2_Gummi_Mission0.p2s` (2026-10-20, not in the repository), read offline: the zip member
`eeMemory.bin` is the 32 MB main RAM, so every address below is an EE address as in the decompilation. Nothing was run: the only emulator
open on the machine was the user's own running game (its PINE port answered, the game ran live at another place), and it was left alone. So
every item is [V] from memory contents unless marked, and none was reproduced in a second run; what needs a run is listed in the last section.
No game data is in this file.

## What the two states are

- Both: world byte `0x33e830` = 0x0f (the world map's world), area 0, entrance 4; battle flag word `0x1c8a7d8` = 0; camera type word `0x35cd38` = 4.
  The screenshot of `KH2_Gummi_Mission` (looked at) is a pale pink and white Gummi ship on a white screen: a flash or fade at the start of a flight.
  `KH2_Gummi_Mission0` (looked at) is the ship from behind in a blurred tunnel of cloud with a dark disc ahead: a flight under way.
  Word `0x33fe64` (it grows between the states; [inferred] a tick counter) is 0xdd963 in the first and 0xdda95 in the second, so they are 306 ticks apart [inferred: one play session].
- The Gummi flight is not a field scene. It is the overlay `ovl_gumibattle.x`, loaded to the overlay area at `0x1f18000` (the only memory that differs from the
  world-map state over `0x1f18000..0x1f82000`; non-zero up to `0x1f90000`). The loader is `FUN_00103e20` (0x103e20): it loads the name at 0x377ec0 through
  `FUN_00167ee0`, allocates 0xce0000 bytes (pointer kept at 0x33e7cc) and a small 8-byte block (0x33e7d0), then jumps to the overlay entry `0x1f18014` with
  argument 0. Its twin `FUN_00103ed0` (0x103ed0) calls the entry with argument 1, frees both blocks and flushes. The overlay entry `0x1f18d80`
  (arguments: 0 or 1, record pointer): with 0 it takes a 0x8000-byte block (`FUN_001024f8`) into record +4 (0x33e7a4 reads 0x1f7c260), then runs the
  overlay's own constructor list; with 1 it runs the destructor list [V, disassembled from the state].
  The menu twin is `ovl_gumimenu.x` (`FUN_00104280` / `FUN_00104388`, record 0x33e7e0, 160000 bytes) and the title is `ovl_title.x` (`FUN_00102ca0`); the world map and
  the Gummi flight are not the same overlay.

## The mode switch (program records)

The game keeps "program" records of 0x38 bytes in the executable's data from `0x33e6e0`: word +0 the shared vtable 0x33e6b0, +8 an id, +0x18 the memory budget
(`FUN_00104130` writes 170000 and flag 0x4000 for the battle record, `FUN_001044d8` 160000 and 0x3000 for the menu record), +0x20 init and +0x24 term function,
+0x1c a loaded flag. The records seen: 0x33e6e0 (id 0x46), 0x33e710 (id 0x54, 130000), 0x33e748 (id 0x4d, 140000, init callbacks 0x102fa8 and 0x103078),
0x33e7a0 (id 0x47, 170000, init 0x103e20, term 0x103ed0: the Gummi flight) and 0x33e7e0 (id 0x47, 160000, init 0x104280, term 0x104388: the Gummi menu).
Between the world-map state and the flight state: record 0x33e6e0 is cleared (its +4 pointer 0x399110 and flag go to 0), record 0x33e7a0 is filled (+4 = 0x1f7c260,
flag +0x1c = 1, buffers 0x33e7cc = 0xfa6b40 and 0x33e7d0 = 0xfa6b80, which is the allocation of `FUN_00103e20`), the words 0x33e818 (0x1166b40) and 0x33e820 (1) change too (owner not identified; they sit in the menu record's block),
and the word 0x33e784 of record 0x33e748 goes 1 to 2. So the mode switch is: the world-map program is torn down and the Gummi program's init runs; the engine
itself keeps running underneath (the same function table, the same camera object and place record). The caller that picks the record was not found (needs a
breakpoint on 0x103e20: gap).
The previous-place record `0x33e848` (the one `FUN_00149bb8` compares with 0x33e830, `FUN_00105070`) reads world 5, area 3, entrance 0x63 in the world-map state and
0x0f, 0, 4 in both flight states [V]: the flight does not change the place bytes at all.

## What runs each frame

Not observed. What the memory shows: the overlay's object arrays live in the heap around 0x1189000 to 0x11d4000 (and 0x1d07000). Words at an object's start that point
into the overlay, with the number of objects (`KH2_Gummi_Mission`, identical in `KH2_Gummi_Mission0`): 0x1f24320 and 0x1f24378: 51 each (one class, 0xcd0 bytes apart
from 0x11bef78); 0x1f6aed8: 24; 0x1f6aea8: 8; 0x1f42100 and 0x1f42130: 8 each. These are classes of the overlay; the project has no decompilation of it (the
Ghidra program holds the executable only), so the per-frame functions were not named.

## What the app lacks for a Gummi mission (gap list, not a port)

The app has no Gummi flight at all (docs/systems/worldmap-gummi/worldmap-plan.md "Route markers"). It needs, at least: the overlay's code and data model as a Rust port (the overlay is a separate
program of about 0x6a000 bytes of code and data with its own object classes), the `gumi.bar` text (`msg/us/gumi.bar`) and the gummi file set loaded in the state (`gumi_sys`,
`gumi_deco`, `wm_ws.2ld`), its own camera, controls and HUD, the mission table and the rewards, and the hand-over (the route marker contact on the map starts it, the
end returns to the map). None of this was read; each is a gap.

## Not reached

- A break on 0x103e20 (who selects the Gummi program and with which route), the per-frame functions of the overlay (a breakpoint on any object's vtable function), the mission
  record and the Gummi controls: need the patched emulator (`local/tools/pcsx2-mcp`) with the two states loaded.
- The states hold no enemy object (battle flag 0 and the actor pool is the map's), so they cannot show the +0x570 link or M_EX950 (docs/systems/foundations/dynamic-tracing.md section 5).
