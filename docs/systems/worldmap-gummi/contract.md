# World map and Gummi: contract

Ladder step L12. Ledger system name `worldmap-gummi`. Crates: `kh2-ard` (`worldmap.rs`: `wldf`, `wlds`, `epsd`, `70landing.bar`, `00worldpoint.bin`,
`wm.bar` ids), `kh2-worldmap` (`worldmap.rs`, `worldmap_ui.rs`), `kh2-event` (`area_jump.rs`), `kh2-menu` (`menu_save.rs` for the entry from the save point).
Sheet ids: none (no `systems.json` row names the world map or the Gummi flight yet). Scenarios: `scenarios/worldmap-gummi/` (none yet).

Status: the world map is an ordinary field area (world byte 0xf, area 0, `ard/wm00.ard`, `map/us/wm00.map`) and the port loads it through the
normal area path; the symbols, cursor, panels, landing, forced events and route-marker props are ported in stages W1 to W8. The Gummi flight (overlays
ovl0 to ovl3, 3,789 functions) is entirely open: nothing is ported and only a static read of two savestates exists.

## What the system does
- Entering: the save point menu's World Map row (page 0x10 question, then the jump), jump type 4 decided by story flags, the cursor's start entrance
  from the world the player left (15 pairs at `0x3790a8`, `FUN_00149db8`; 22 cursor spawn records).
- The screen: scripted objects of objentry type 0x11 (code `0x2c1000..0x2d2000`, `FUN_0016e8c0` case 0x11 -> `FUN_002c58c8`): twelve world symbols,
  a controllable cursor (the Gummi ship, vtable `0x36fb00`), nine Gummi route markers (props, `WM_GUMI_01` has no model), the world info panel, the
  land prompt, the destination list and the episode list drawn from `wm0command.2dd`, `wm0field.2dd`, `file/us/wm_ws.2ld`, `70landing.bar`, `msg/us/wm.bar`.
- The landing: a lookup on (world, the world's story state byte) in the `wlds` entry of `00progress.bin`, or a row from the destination list;
  the jump is `FUN_00104e18(destination, 1, 0, 0, 0)`. State: world records at `0x341858`, world flags `0x3418a4`, queue `0x3418c4`, episode bits.
- The Gummi mission: the mode switch (program records), the per-frame loop, the missing pieces (gap list in `gummi.md`).

## State and owners
| state | where in the original | owner in the port |
|---|---|---|
| world records, world flags, queue, episode bits | `0x341858`, `0x3418a4`, `0x3418c4` | `progress::Story`, `Progress` |
| route state byte per route | `FUN_001c0950(route)` (`0x3419c3`+) | `worldmap.rs` |
| cursor object, symbols, markers | map objects (vtable `0x36fb00` for the cursor) | `worldmap.rs` |
| prop table (BAR type 0x15 `out`, 77 records) | map `+0x44` (`FUN_00149348`), `FUN_0015bd40` | `worldmap::map_props` |

## Entry points in the original
| address | what |
|---|---|
| `FUN_00149df0`, `FUN_00149db8`, `FUN_00104e18` | leave to the world map, start entrance of the cursor, area change |
| `FUN_002c58c8`, `FUN_002ccd88` | object type 0x11 creation, the two forced events |
| `FUN_002cc9d0`, `FUN_002cca58`, `FUN_002ccb70`, `FUN_002cdf08` | cursor input step, per tick, contact events |
| `FUN_002c8b50`, `FUN_002c8bb0` | a route marker's update and its route number |

## Open gaps
- The camera: camera type 4 (what the `WORLDMAP` manager stores) runs no case of `FUN_00177128`; where the map's view comes from is unfound; the cursor
  movement states and the symbols' blocking radius are only read at the confirm path (`worldmap.md` "8. Recorded gaps", `worldmap-plan.md` "W8").
- The other 11 libretto events, the route markers starting the Gummi flight, the map command menu's captions `0x4c10..0x4c14`, the meaning of the
  `link`, `wmky`, `ws_i`, `md_m`, `50wmentry`, `50worldmap` files.
- Gummi flight, Gummi menu and editor: the overlays' code and data model, `msg/us/gumi.bar`, the `gumi_sys` and `gumi_deco` file set, camera, controls,
  HUD, mission table, rewards and the hand-over (`gummi.md`). Host groups 3 and 9 (Gummi scripts) are empty in the executable's table.
- `worldmap.md` "9. What the app has" is the oldest status; the plan file's W1 to W8 "done" paragraphs are newer.

## Where the details are
Section numbers in references ("section 5", "20.14") are the old chapter numbers that still stand in the headings of the files below.

| file | KB | covers (section numbers are the old chapter numbers) |
|---|---|---|
| [gummi.md](gummi.md) | 6 | What the two states are; The mode switch; What runs each frame; What the app lacks for a Gummi mission; Not reached |
| [worldmap-plan.md](worldmap-plan.md) | 58 | sections 10, 11 |
| [worldmap.md](worldmap.md) | 27 | sections 0, 1, 2, 3, 4, 5, 6, 7, 8, 9 |

Index of all systems: [../../README.md](../../README.md).
