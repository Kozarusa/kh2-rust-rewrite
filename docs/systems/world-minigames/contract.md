# World mechanics and minigames: contract

Ladder step L11. Ledger system name `world-minigames`. Crates: `kh2-event` (`struggle.rs`, `intro.rs`), `kh2-hud` (`hud_mission.rs`), `kh2-menu` (`menu_struggle.rs`).
Sheet ids: `game.struggle`. Scenarios: `scenarios/world-minigames/` (none yet).

Status: only the Struggle (the Twilight Town mission manager for groups b_50, b_51, b_52) is documented and ported: rules, end by counter or time,
win and lose events, HUD, result menu, the battle intro. The rest of L11 (the other player classes, Atlantica, Light Cycle, 100 Acre Wood games,
skateboard, Agrabah's carpet, Mulan's morale, Port Royal, escort and timed missions, the Olympus cups, every other mission manager kind) has no doc.

## What the system does
- Which battles are Struggle matches, the mission file (`msn/us/<name>.bar`), counters, the clock, orbs (orb effect file `obj/PRIZE_TT.mdlx`, orb
  kinds and digit tables from the executable), the mission script and the end, the start and end sequence (WP-7E2), the player's knock-out,
  Sora fighting alone (no party members, WP-S8), the pickup skips, `player_finish_struggle`, the HUD element records.
- The mission manager object: create / load / start / destroy / update / end (`FUN_001b9338`, `FUN_001b8b10`, `FUN_001b8d80`, `FUN_001b8ee8`,
  `FUN_001b90d0`, `FUN_001b9140`, `FUN_001b95a0`); header records read by `FUN_001b9378`; the area-script interpreter's case 0x15 starts it.
- The battle intro (WP-I1): camera, effect, INFORMATION bar. The global clock (`0x14c560`, `0x14c650..0x14c7f0`). The "collect at once" bit after the
  end (WP-FL3).

## State and owners
| state | where in the original | owner in the port |
|---|---|---|
| mission manager header and counters | manager object (header words, `+4` bit 9 is read by the game-over Mickey offer) | `struggle.rs` |
| clock | global clock `0x14c560..` | `struggle.rs`, `hud_mission.rs` |
| HUD element records | `0x1d6110`, `0x1d61c8`, `0x1d64d0`, `0x1d6ac0`, `0x1d6f00`, `0x1d72c8` | `hud_mission.rs` |
| group-4 hosts | table at `0x35bce8`, code `0x1d6290..0x1d7598`, `0x19e910..0x19ed28` | `enemy_host.rs` (script-host) |

## Entry points in the original
| address | what |
|---|---|
| `FUN_001cf9e8`, `FUN_001caeb0`, `FUN_001cadd8` | Struggle rule entry points named by the sheet |
| `FUN_001b9338` .. `FUN_001b95a0` | mission manager lifecycle |
| `0x1cf508..0x1cfb60` | the Struggle's own functions (index in `struggle.md`) |

## Open gaps
- Result layouts, area jump after the result, the intro in full (sheet row `game.struggle`); the target gauge over the TIME plate is an inferred fix.
- Everything outside the Struggle: no doc, no code (see Status). The mission-manager kinds are collected in the ladder step, not here yet.

## Where the details are
Section numbers in references ("section 5", "20.14") are the old chapter numbers that still stand in the headings of the files below.

| file | KB | covers (section numbers are the old chapter numbers) |
|---|---|---|
| [struggle.md](struggle.md) | 42 | Which battles are Struggle matches; The mission file; Counters; The clock; Orbs; The mission script and the end; +11 more |

Index of all systems: [../../README.md](../../README.md).
