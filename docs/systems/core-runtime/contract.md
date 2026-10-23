# Core runtime: contract

Ladder step L1 (PLAN section 5). Ledger system name `core-runtime`. Crates: `kh2-ard` (area files, area scripts, story-flag lists), `kh2-map`
(COCT collision), `kh2-game-core` (`progress.rs`, `area.rs`, `world.rs`, `schedule.rs`, `harness.rs`), `kh2-event` (`area_jump.rs`, `savepoint.rs`).
Sheet ids: `world.collision`, `world.event`, `game.progress`, `game.story`, `game.jump`. Scenarios: `scenarios/core-runtime/` (none yet).

Status: the saved-state, story-flag, area-script, area-load and walking-collision parts are documented and mostly ported. The task
system, frame step and RNG are documented in [runtime.md](runtime.md) (WP-L1-S1), the memory-card unit in [memcard.md](memcard.md) (WP-L1-S2). Still without a doc: COCT
ceiling and raycasts (the geometry unit), the file unit.

## Clusters (ledger notes `L1:<cluster>`, WP-L1-S1)
The ledger's first seeding put most of the core runtime under other systems (audio, enemy, event.player, player.sora, ui.menu). L1-S1
re-labelled 709 rows by translation unit: the executable is linked unit by unit, so a contiguous address run between named functions of
one unit is one unit [inferred: unit bounds from the names of the research match table, not from link maps].

| cluster | system | address runs | fns | KB |
|---|---|---|---|---|
| task | core.task | 0x101d68..0x102b98, 0x102bc8..0x102c30, 0x105440..0x105618 | 45 | 4.1 |
| frame | core.task | 0x14c6b0..0x14dc70 (timers, clock, vsync) | 64 | 5.5 |
| rng | core | 0x1062f0..0x106408 | 5 | 0.3 |
| area | core.area | 0x1047a8..0x104bd0, 0x104d58..0x1052a0, 0x105cd8..0x105fb8, 0x149b08..0x14a1e0, 0x1688a0..0x1698d0, 0x193380..0x193ef8, 0x1982c8..0x19a3f8 (spawn groups), 0x19d8d0..0x19de48 (jumps) | 144 | 21.6 |
| script | core.script | 0x180948..0x181688 | 25 | 3.4 |
| story | core.story | 0x17ff60..0x180870, 0x19de70..0x19e038, 0x1a7780..0x1a78c8 | 31 | 3.4 |
| save | core.save | 0x105638..0x105890, 0x187868..0x187e08 (plus the 141 rows already core.save) | 18 | 2.1 |
| memcard | core.save | 0x230e90..0x2331b0, 0x23c830..0x23e598 (card io, file layer, crc) | 132 | 15.7 |
| coct | core.collision | 0x141898..0x144328, 0x163f98..0x1657d8 (walking collider) | 89 | 16.8 |
| geom | core.collision | 0x144368..0x149ae8 (boxes, lines, ray tests) | 130 | 21.2 |
| file | core.file | 0x167cc0..0x1687b8 | 29 | 3.0 |

## What the system does
- Keeps the player's saved game: the save block (`0x33e860`, `0xb4e0` bytes) and the party block inside it (`0x33fec0`); the port's
  `progress::Progress` mirrors the first `0x1998` bytes of the party block as raw bytes with typed accessors, so every rule is a port of
  the routine that reads or writes those bytes and the kh2-ee oracle compares whole blocks.
- Holds the story: flags are bits at `0x33f6b0` (flag `id` = bit `id & 0x1f` of word `(id & 0x3ff) >> 5` of page `id >> 10`); setting a flag
  runs its side-effect list from `00progress.bin` (area programs, exit locks, world-point bits, world records, more flags).
- Loads areas: the area record at `0x33e830` (world, area, entrance, three program ids), the area file `ard/<world><NN>.ard`, the three area
  scripts `map`, `btl`, `evt`, spawn groups, exit volumes, trigger records, area music, camera and party at the entrance.
- Moves between areas: every change goes through `FUN_00104e18(destination, fade, wait, flag, reset)`; jump records (`FUN_0019d9d8`) have six types.
- Decides what stops the walking player: the collider sweep and the polygon filters over COCT, and the invisible-wall rule of the arenas.

## State and owners
| state | where in the original | owner in the port |
|---|---|---|
| save block, party block | `0x33e860`, `0x33fec0` (init `FUN_00105638`, `FUN_00187b38`) | `progress::Progress` (resource) |
| inventory counts, key-item bits, obtained-once bits | party block `+0xe28`, `+0xf68`, `+0x1630` (`FUN_001a1a18/1c30/1cc8`, `FUN_001a18d8`) | `Progress` |
| experience total, summon experience | `+0xf88` (`FUN_001b51e0`), `+0xf8c` (`FUN_001d4ec8`) | `Progress` |
| area record (world, area, entrance, programs) | `0x33e830` (`FUN_00104bd0`) | `area.rs` `AreaRecord` |
| program ids per area | save `0x33e86f + world * 0xc0 + area * 3` | `area.rs`, written by story flags and jumps |
| story flags | `0x33f6b0` (`FUN_001802e0` test, `FUN_00180298` clear, `FUN_0017ff60` set) | `progress::story`, `kh2_ard::progress` |
| world records, world-map state | `0x340c9c + 4 * w` (`FUN_001a71c0`) and the world-map region | `Progress` (WP-WM1 region) |
| collision polygon filter | handler table `0x378f7c` chosen by actor `+0x600` bits 3..6 | `world.rs`, `kh2-map` |

## Entry points in the original
| address | what |
|---|---|
| `FUN_00101d68` -> `FUN_00105638` | boot: clears and sets up the save block |
| `FUN_00187d20`, `FUN_00187b38` | the new game's party block |
| `FUN_001689c0`, `FUN_00180870`, `FUN_00181148` | area thread loads the area file, runs the `map`, `btl`, `evt` scripts |
| `FUN_00180ad8`, `FUN_00180b48` | one program's commands, one command (32 opcodes) |
| `FUN_00104e18` | the area change; callers `FUN_00198fa0` (exit), `FUN_0019d9d8` (jump record), hosts 1.30/1.31, `FUN_00149df0` (world map) |
| `FUN_00198750`, `FUN_001989d8`, `FUN_00198b30` | spawn group activators, per-frame exit test, group kinds 1 to 10 |
| `FUN_0017fd10` | loads the 19 world entries of `00progress.bin` |
| `FUN_00143840` -> `FUN_00164138`, `FUN_00141b50` | walking collider sweep and the polygon filter |
| `FUN_00101d68` loop -> `FUN_001029d8` -> `FUN_001028a0` | one pass over the task list per loop ([runtime.md](runtime.md)) |
| `FUN_0014ccf8` -> `FUN_0014cdd0` | frame task (priority 800000): vblank wait and the frame step |
| `FUN_003117a8`, `FUN_00106308`, `FUN_001063b8` | RNG and its range helpers, seeded by `FUN_001062f0(0x875b2f)` at boot |

## Layouts (C7, WP-FAC-8)
- Objects are allocated one by one from the general heap (`FUN_00104fa8(size)`, slot 0 of the allocator at `DAT_0033e850`); the factory
  `FUN_0016e8c0` picks the class and its size from objentry byte +4: base object 0x9c0, standard objects 0xa00, character classes 0xaf0,
  enemy classes 0xb00, party members and summons 0xd00, player classes 0xb70 to 0xd20 by objentry id (`FUN_00178e40`; Sora 0xb80,
  `FUN_001ce820`). There is no single actor size.
- Class scheme: each constructor calls its base and then stores its own vtable at +0: base `FUN_00168d30` (0x35b950), standard
  `FUN_00196b58` (0x35e408), battle `FUN_00184bf8` (0x35ce50), then character `FUN_001b20b8` (0x35f110), enemy `FUN_00184820`
  (0x35cdc0), playable `FUN_001746e0` (0x35c908), player `FUN_00176e00` (0x35c9a8), friend `FUN_0017c718` (0x35cc18). The port keeps
  the real vtable address in the block's first word and dispatches on it.
- Handles: +4 holds the object itself (`FUN_00168d30`). Object list: head `DAT_01d5d7a0`, tail `DAT_01d5d7a4`, link +0x8b4, sorted by
  the priority byte +0x9ad (`FUN_001b7440`, called by the base constructor). Port handles stay `SELF_BASE + slot * 0x1000` (every class
  fits), 0 for null; oracle tests translate handles to kh2-ee addresses.
- Blocks are `layout!` byte blocks of the allocation size with typed accessors; migration order and the field tables are in the
  FAC-8 spec (step L1: save block, object list, status pool).

## Open gaps
- L1 parts with no doc: memory-card format (132 functions, nearly all unread), COCT ceiling and ray casts (geometry unit, 114 unread),
  the file unit; the wait inside the frame task (`FUN_0014d1a8`) and the thread sleep writers (`runtime.md` "Gaps").
- Ledger: the game-mode units next to the task manager (MENU 0x103058..0x103e20, GUMIBATTLE 0x103ed0..0x104788) and the object
  units `0x1b72f0..` keep their old labels; their seeding looks wrong too (pause menu and Gummi modes, labelled event.player/core) and
  belongs to the L8/L12 specs.
- Story: where the story gives Sora his keyblade, spells, forms and Guard (the app baseline stands in for them, `progress.md` "App baseline");
  `FUN_001803b0` and `FUN_00180450` are not ported (`progress.md` "Gaps").
- Area scripts: every opcode of `FUN_00180b48` is ported (`kh2_ard::script`, oracle-verified) and group kinds 7 and 9 are ported (`kh2_ard::kinds`; kind 9 without an oracle,
  `areas.md` section 1a). What is left is wiring: the app applies `ScriptRun::file_loads` (opcode 0x16), `entries` (0xa) and the extra variables, and calls kind 9.
  (`script-host/area-survey.md`).
- Collision: the walking collider is ported over the app's own `World::slide`/`ground`, not `FUN_00143840` itself (`collision.md`).

## Where the details are
Section numbers in references such as "section 5" or "20.14" follow the old chapter numbers kept in the headings; the table says which file holds them.

Section numbers in references ("section 5", "20.14") are the old chapter numbers that still stand in the headings of the files below.

| file | KB | covers (section numbers are the old chapter numbers) |
|---|---|---|
| [areas.md](areas.md) | 41 | sections 0, 1, 2, 7, 7b |
| [collision.md](collision.md) | 18 | sections 5, 8 |
| [progress.md](progress.md) | 32 | Where the state lives; Inventory rules; The new game; Start state in the app; `KH2_STATE` file schema; Story flags; +5 more |
| [story-flags.md](story-flags.md) | 3 | section 5 |
| [runtime.md](runtime.md) | 7 | boot, task system, frame step, RNG |
| [memcard.md](memcard.md) | 16 | card driver, save file layer, CRC, .ps2 image, oracle plan |

Index of all systems: [../../README.md](../../README.md).
