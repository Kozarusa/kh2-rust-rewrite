# Core runtime: contract

Ladder step L1 (PLAN section 5). Ledger system name `core-runtime`. Crates: `kh2-ard` (area files, area scripts, story-flag lists), `kh2-map`
(COCT collision), `kh2-game-core` (`progress.rs`, `area.rs`, `world.rs`, `schedule.rs`, `harness.rs`), `kh2-event` (`area_jump.rs`, `savepoint.rs`).
Sheet ids: `world.collision`, `world.event`, `game.progress`, `game.story`, `game.jump`. Scenarios: `scenarios/core-runtime/` (none yet).

Status: the saved-state, story-flag, area-script, area-load and walking-collision parts are documented and mostly ported. The rest of L1
(object and task system, frame order and timing, RNG seeded from RAM, the memory-card save format, COCT ceiling and raycasts) has no doc yet:
those are the first spec packages of the step.

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

## Open gaps
- L1 parts with no doc: object and task system, frame order, RNG, memory-card format, COCT ceiling and ray casts.
- Story: where the story gives Sora his keyblade, spells, forms and Guard (the app baseline stands in for them, `progress.md` "App baseline");
  `FUN_001803b0` and `FUN_00180450` are not ported (`progress.md` "Gaps").
- Area scripts: group kinds 7 and 9 (`FUN_001ccb10`, `FUN_0019a000`) not read; opcode 0x16 `load_file` is the only opcode read and not ported
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

Index of all systems: [../../README.md](../../README.md).
