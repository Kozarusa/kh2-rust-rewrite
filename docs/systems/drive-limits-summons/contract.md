# Drive forms, limits, reaction commands, summons: contract

Ladder step L5 (the reaction commands, the limit scenes and the summons are shared with party, L6). Ledger system name `drive-limits-summons`.
Crates: `kh2-app` (`drive.rs`, `limit.rs`, `limit_script.rs`, `limit_camera.rs`, `limit_visual.rs`, `summon.rs`, `reaction_cmd.rs`,
`reaction_parts.rs`, `revive_fade.rs`, `mickey.rs`), `kh2-bdx` (`reaction.rs`, the scripts of limits and summons), `kh2-tables` (`fmlv`,
`limt`, `sumn`, `cmd`). Sheet ids: `game.summons` (the row `game.sora.abilities` covers drive forms, limits, summons and reaction commands too
and sits in `combat-effects`). Scenarios: `scenarios/drive-limits-summons/` (none yet).

Status: drive forms run (Valor, Wisdom, Master, Final, Anti with outfit variants and hand weapons); reaction commands are ported from the native
offers, the accept path and the Soldier's command 34; limits run as a BDX thread with their task hosts, but no limit scene is visible yet;
Stitch, Genie, Chicken Little and Peter Pan run without a missing host and Tinker Bell's revive is a state machine; summon limit scripts do not run.

## What the system does
- Drive: the gauge in whole points with a 0..100 fraction (`FUN_001a0290` gain, `FUN_001a0320` spend), the Drive command checks and cost, the
  Anti and Final roll, the change motion, the body swap, the form timer (drained per 60 Hz tick), the form's weapons by hand, world outfit
  variants (`_NM`, `_TR`, `_WI`).
- Gauge mode 3 (summon timer): the same status fields as the form timer, selected by the mode byte at player status `+0x1af` (1 points, 2 form
  timer, 3 summon timer, 4 locked); start `FUN_001a0480(status, 3, level)`, drain `FUN_00177788`.
- Reaction commands: five sources of an offer (reaction parts in the object file, type 14 records, spawn-record part 0x83, scripts, Mickey's
  class), the prompt rules, the accept path, the target side (Soldier's command 34), the 124 reaction rows, clash.
- Limits: 27 `limt` rows (single-partner, Trinity, summon limits), the limit task (`FUN_001b28a0`, events 22 and 23), the limit script, camera,
  visuals, Auto Limit (the partner's limit offered as a reaction command).
- Summons: `sumn` rows (4), the summon command and the charm item, spawn and arrival, the player during it, despawn, what each summon does,
  the objects the summon scripts make (Tinker Bell, Chicken Little's child), revive.

## State and owners
| state | where in the original | owner in the port |
|---|---|---|
| drive gauge, mode, form timer | player status `+0x1af`, `+0x1b0..+0x1b8` | `drive::rules::Points`, `DriveGauge` |
| current form, active summon, summon level, Genie form | party block `+0xdcc..+0xdcf` (`DAT_00340c88` bytes 4 to 6) | `Progress` |
| form records | party block `+0xc3c + f * 0x28` | `Progress` |
| limit task | `DAT_01d5d6b0` (`FUN_00104fa8(0xdc)`), tested by `FUN_001b3c00` | `limit.rs` |
| pending reaction command | the player's command slot `+0xb44` (command id at `+6`) | `reaction_cmd.rs` |
| summon objects and their scripts | `sumn` `Entity1` (spawned by `FUN_001c6260`), `Entity2` preloaded | `summon.rs` `SummonHost` |

## Entry points in the original
| address | what |
|---|---|
| `FUN_001a0290`, `FUN_001a0320`, `FUN_001a0358`, `FUN_001a0480` | drive gain, spend, apply, set the gauge mode |
| `FUN_0019c670`, `FUN_0019cad0`, `FUN_0019cb70`, `FUN_0019c520` | player controller, offer walk over the object list, one object, candidate |
| `FUN_00188e70` | walks the collision records (type mask `0x4020`) |
| `FUN_001cf030` case 0x51 -> `FUN_001b28a0` | limit start (command id at `+6`); `FUN_001d2040` finds the `limt` row |
| `FUN_001d4c68`, `FUN_001d4ca0`, `FUN_001d4d28`, `FUN_001d4da8` | `sumn` rows, row of a command, active row, preload |
| `FUN_001c6260`, `FUN_001c6710` | summon start and arrival |
| `FUN_0010b780` | picks the summon's AI object by NeoStatus |

## Open gaps
- Drive: transformation visuals and sounds, which routine starts A121, the weapon bones, the outfit writer, per-form stat changes (`drive-forms.md`
  "Gaps", `combat-effects/player-data.md` "Gaps (not found)").
- Reaction commands: type 14 records have no data on the disc; the target-side handlers beyond command 34 (`reaction-commands.md` "Gaps of this section").
- Limits: no limit scene is visible (partner objects, motion, effect and camera hosts), the task's end test, the Trinity row that needs no
  partner (`limits.md`).
- Summons: what starts the arrival event, who delivers events 22 and 23, summon stats, the friend AI engine of the summons, the airborne mover's
  flight, the camera FPS mode, Genie's model parts (`summons-spec.md` "7. Unknowns", `summons-objects-*.md`).

## Where the details are
Section numbers in references ("section 5", "20.14") are the old chapter numbers that still stand in the headings of the files below.

| file | KB | covers (section numbers are the old chapter numbers) |
|---|---|---|
| [drive-forms.md](drive-forms.md) | 34 | What is implemented; Verified facts; Hook points in sora.rs; Gaps; Integration status |
| [limits.md](limits.md) | 54 | Limits; The limit task; Limit script; Limit script, part 2; Limit script, part 3; Limit camera; +3 more |
| [reaction-commands.md](reaction-commands.md) | 44 | Reaction commands; Hook points; Harness press timing |
| [summons-objects-1.md](summons-objects-1.md) | 29 | Objects the summon scripts make |
| [summons-objects-2.md](summons-objects-2.md) | 21 |  |
| [summons-objects-3.md](summons-objects-3.md) | 17 |  |
| [summons-spec.md](summons-spec.md) | 32 | sections 1, 2, 3, 4, 5, 7 |
| [summons-stages.md](summons-stages.md) | 48 | section 6 |

Index of all systems: [../../README.md](../../README.md).
