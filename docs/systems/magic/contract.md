# Magic: contract

Ladder step L4. Ledger system name `magic`. Crates: `kh2-app` (`magic.rs`, `magic_rules.rs`, the menu and HUD command lists in `items.rs`),
`kh2-tables` (`magc`, `magi` readers), `kh2-bdx` (the spell scripts run as BDX). Sheet ids: `game.sora.magic`. Scenarios: `scenarios/magic/` (none yet).

Status: Fire, Blizzard, Thunder and Cure (levels 1 to 3), Magnet, Reflect, the drive-form variants, the magic turn, MP charge and the lock-on
controller are ported in phases 1 and 2. Magic on party members (L4 scope) has no doc yet.

## What the system does
- Data: `magc` rows (56 bytes; per spell and level: `Id` slot 0 Fire, 1 Blizzard, 2 Thunder, 3 Cure, 4 Magnet, 5 Reflect; resource BAR
  `magic/FIRE_1.mag`; the owning `Item`; `Command`), the `cmd` rows (`Execute` 2, `Argument` the slot, `Cost` MP, 255 = all MP), the `magi`
  rows of 03system (cast kinds, read by the scripts as floats), `prty` MpDrive for the charge time, `atkp` rows for the spell attacks.
- Casting: the cast start (`FUN_001a4798`, caller `FUN_00179400`, executor `FUN_00174cf8`), the magic turn, the spell script run in the BDX
  interpreter (`kh2 bdx magic/FIRE_1.mag`), frame trigger 17 pays MP and sends event 21 (`FUN_001a4a60`, case 0x11 of `FUN_00188498`).
- Lock-on controller (`FUN_0019b7a8` frame, `FUN_0019b3f0` mode change), the target point (host 1.88, `FUN_00172f40`).
- MP and the MP charge (`FUN_00186440` reads `+0x12c` -> `+0x1c` of the player object); Magnet's pull; Final Thunder's scatter.

## State and owners
| state | where in the original | owner in the port |
|---|---|---|
| six loaded spell resources | table `0x35e810` (`FUN_001a45c0` load, `FUN_001a4530` free) | `magic.rs` |
| magic thread list | `FUN_001a42b8` link, `FUN_001a43b8` destroy, `FUN_001a44a8` clear | `magic.rs` |
| per-spell switch bits | party block `+0xfec` (`FUN_001a4b30/4b68/4bb8`) | `Progress::toggle_spell_switch` |
| global flag word, bit 3 | `0x35d268` (readers `FUN_001905e8`, callers `FUN_001a3848`, `FUN_001bd358`) | `magic.rs` |
| MP charge timer | player `+0x1bc` (left) and `+0x1c0` | `hud_gauge::MpRecharge` |

## Entry points in the original
| address | what |
|---|---|
| `FUN_00193610` -> `FUN_001a4c68`, `FUN_001a4c78` | `magc` table pointer (`0x1c95520`) and the row of a slot |
| `FUN_001bbee8`, `FUN_001b6f48`, `FUN_001a4dd0` | magic list, command lookup, slot command |
| `FUN_001cc1f0` -> `FUN_001cc140` | `magi` row for a cast kind (a missing row falls back to row 0) |

## Open gaps
- The menu's input code (cursor, wrap, close after cast), Blizzard/Thunder/Cure in the interpreter, the pull mover 15 against 14, the line-test
  mask 2 of host 1.248, Sora as the victim of reaction class 5, the HUD MP bar's fill direction (`magic-turn.md` "Not ported / gaps").
- Party members casting has no doc; the lock-on controller's mode-1 marker and its re-pick (`FUN_0018f538` case 6) are open.

## Where the details are
Section numbers in references ("section 5", "20.14") are the old chapter numbers that still stand in the headings of the files below.

| file | KB | covers (section numbers are the old chapter numbers) |
|---|---|---|
| [magic-spells.md](magic-spells.md) | 25 | Data; The command list; Casting; MP and the MP charge; Fire; Attack objects and hits; +3 more |
| [magic-turn.md](magic-turn.md) | 37 | The magic turn; Running the scripts in the interpreter; Phase 2; Not ported / gaps; Decompile index |

Index of all systems: [../../README.md](../../README.md).
