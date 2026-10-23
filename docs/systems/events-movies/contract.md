# Events, NPCs and movies: contract

Ladder step L10. Ledger system name `events-movies`. Crates: `kh2-ard` (`parse_event`, `libretto.rs` talk records, `localset.rs`), `kh2-app`
(`event_player.rs`, `npc.rs`, `savepoint.rs`, `intro.rs` for the event camera tracks). Sheet ids: `game.events`, `world.talk`.
Scenarios: `scenarios/events-movies/` (none yet).

Status: the event player runs area events with actors, placements, camera tracks, opcode families listed in `event-player.md` (E6, E6b to E6f), NPC
talk with the talk camera and head look-at, save point offers and heals; world points are read. Open L10 scope: the event player complete
(opcodes with unknown meaning), event voices and lip sync, subtitles, the PSS movie player (MPEG-2 video and ADPCM audio, decoded in Rust from
the disc), credits and endings: the movie player and credits have no doc yet.

## What the system does
- Events live in the area file as BAR entries of type 0x16 named by number (`FUN_00180a28` finds one by tag; Twilight Town has 190). The format is
  a list of commands `u16 size, u16 opcode, operands` ending with size 0 (`kh2_ard::parse_event`); command 0 carries the memory size and the event
  name (`FUN_00223910`). Animation files load by name `anm/<world>/<model>/<name>.anb` (`FUN_0022cbb0`), layouts `event/layout/<a>/<b>.2ld`, movies
  `movie/<a>/<b>.pss` (`FUN_00225860`, event opcode 0x44), voices `voice/<a>/event/<name>.vag` (`FUN_0022bff8`), effects `event/effect/*.pax`.
- Triggers (opcode 0xc and the trigger subcommands) fire events; an event's start holds back and its end gives back state (`FUN_00195898` takes the
  kind row of the `evtp` table, `struggle::event_kinds`).
- NPCs: the class-5 runtime and the walk (WP-E2), talk (`FUN_00177b18` case 0x1f, `FUN_00159688`), the talk camera, the head look-at controller
  (`FUN_001aead0`), `libretto-<world>.bar` talk records, `07localset.bin` for program-to-area keys.
- Save points and world points: the save point offers (menu kind 1) and heal; the same memory-card driver as the save menu.

## State and owners
| state | where in the original | owner in the port |
|---|---|---|
| running event, its actors and placements | the event thread (`FUN_00223910`) | `event_player.rs` |
| event kind rows | `evtp` of `03system.bin`, loaded to `DAT_01d6a92c` (read by `FUN_001d1128`) | `struggle::event_kinds` |
| NPC walk path, mode `+0x87`, stop index `+0x8c`, head look-at output | class-5 object | `npc.rs` |
| talk state | `FUN_001a5b18`, `FUN_0019abb0` of the player with command row 10 | `npc.rs`, `camera.rs` (mode 4) |

## Entry points in the original
| address | what |
|---|---|
| `FUN_00180a28`, `FUN_00223910`, `FUN_0022cbb0` | find an event, run its command 0, load an animation |
| `FUN_00180ff8`, `FUN_001975e0`, `FUN_00197588`, `FUN_00197670` | triggers and event firing |
| `FUN_00195898`, `FUN_00195ae0`, `FUN_001959f0` | event start and end state |
| `FUN_00177b18`, `FUN_00159688`, `FUN_001aece8`, `FUN_001aead0` | talk, talk camera, head look-at |
| `FUN_00225860`, `FUN_0022bff8` | movie command, event voice |

## Open gaps
- Meanings of event opcodes 0xa, 0xb, 0xe, 0x11, 0x16, trigger subs 3, 6, 8 and flag ops 2 to 5; the talk wait's early end; the head look-at joint
  turn; host 5.8/5.9/5.10/5.12..5.15/5.18..5.25/5.28..5.32 (`events-plan.md` "10. Unknowns").
- The World Map row of the save point menu (what the exit loads) and the page-0 help (`menus-save.md` 18.5).
- The movie player, event voices and lip sync, subtitles beyond the line class `0x1aae60`, credits and endings have no doc.

## Where the details are
Section numbers in references ("section 5", "20.14") are the old chapter numbers that still stand in the headings of the files below.

| file | KB | covers (section numbers are the old chapter numbers) |
|---|---|---|
| [event-player.md](event-player.md) | 25 | section 6 |
| [events-plan.md](events-plan.md) | 43 | sections 8, 9, 10 |
| [npc-savepoints.md](npc-savepoints.md) | 34 | sections 3, 4 |

Index of all systems: [../../README.md](../../README.md).
