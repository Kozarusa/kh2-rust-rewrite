# Player and camera: contract

Ladder step L2. Ledger system name `player-camera`. Crates: `kh2-anim` (`moveset.rs` attack selection rules, `blend.rs`, `triggers.rs`),
`kh2-player` (`sora.rs`), `kh2-game-core` (`motor.rs`, `camera.rs`, `camera_rules.rs`, `camera_blockers.rs`), `kh2-event` (`intro.rs`).
Sheet ids: `game.player`, `game.sora`, `game.sora.dodge`, `game.intro`. Scenarios: `scenarios/player-camera/` (none yet).

Status: Sora's ground combo, air chain, air swing momentum, Square actions, Quick Run and the follow, look-around and lock-on cameras are ported;
the event and talk cameras are partly ported. The Roxas player class, camera type 4 (world map cursor) and several camera modes are open.
L2 spec (WP-L2-S1, 2026-10-24): `controller.md`; package list `local/specs/L2.md`. No L2 function is oracle-verified yet. Known
deviations: the stick is mapped by the orbit yaw instead of the drawn view, and the camera ignores the pad's sticks under the rig.

## What the system does
- Sora's controller: walking and running, jump and fall, the party command executor (`FUN_001778e8` ticks a pending command, `FUN_00174cf8` runs
  it), attack presses and their selection from the `ptya` table (key, flags, leap records), air swings that keep their momentum, the Square
  actions (guard, Glide, Quick Run, High Jump, Aerial Dodge as growth abilities) and the blend between motions.
- The mover model: per-tick displacement = mover result + motion root motion + secondary velocity (`FUN_00181e00`); the jump mover (mover 1,
  vtable `0x35cbe0`) brakes horizontal velocity by 0.9/0.95 per tick; the ground mover follows the stick.
- The field camera: one global object at `0x355840`, updated by a task (`FUN_00140be0` -> `FUN_00140a80`) in the order copy-last, mode update
  (follow 0, look-around 1, lock-on 2, scripted 3 to 10), fov and roll, event override, smoothing and collision, projection.
- The scripted cameras: event camera tracks (modes 3 to 10 and the override at `0x5206f0..`), the talk camera (mode 4, `FUN_00159648`), area
  camera types, the battle intro camera and effect.

## State and owners
| state | where in the original | owner in the port |
|---|---|---|
| camera object (mode, radius, yaw, smoothed and wanted vectors) | `0x355840` fields `+0x00..+0xd8` | `camera.rs` (`FixedUpdate`, after `Sim::EffectTick`), pure math in `camera_rules.rs` |
| event camera override | `DAT_00359038` flag, vectors `0x5206f0/0x700/0x710` | `camera.rs`, `event_player.rs` |
| attack table (`ptya`), key, flags, combo counters | actor `+0xaf0` (`FUN_00175ff8`), counters reset by `FUN_00176a70` | `kh2_anim::moveset`, `sora.rs` |
| pending command, mover flags, stick | actor `+0xb44`, mover 1 / mover `0x16` | `sora.rs` `Attack`, `QuickRun`, `Dash`; `motor.rs` `Motor`, `Jump` |
| actor motion state flags | actor `+0x160` bits (range triggers 0 to 3, 24, 30) set by `FUN_00188030` | `sora.rs` |
| collision kind of the body | actor `+0x600` bits 3..6 | see core-runtime `collision.md` |

## Entry points in the original
| address | what |
|---|---|
| `FUN_00140a80`, `FUN_001407b8`, `FUN_0013f600`, `FUN_0013f8f8` | camera per-tick order, smoothing, spring, collision |
| `FUN_001402c8`, `FUN_00140678`, `FUN_0015a0f0`, `FUN_00159648` | follow, look-around, lock-on, talk camera |
| `FUN_00174cf8`, `FUN_00177ea0`, `FUN_001758b0` | attack press, Square action chain, Aerial Recovery / key 40 |
| `FUN_001cb928`, `FUN_001cbcf8`, `FUN_001cba68`, `FUN_001cbf48` | the table selection |
| `FUN_00176050`, `FUN_0017b278`, `FUN_0017b6a0` | attack start (leap or no velocity write), jump set-up, jump mover tick |
| `FUN_00181e00`, `FUN_00182370` | per-tick displacement and root motion, real displacement at `+0x860` |
| `FUN_0017e220` .. `FUN_0017e6c0` | motion request, blend set-up, cut test, blend step |
| `FUN_001772f8`, `FUN_001774d8`, `FUN_00177360` | stick reader by mover, stick to move vector +0x10 through the drawn view matrix |
| `FUN_00174bd8`, `FUN_0016a948`, `FUN_0016a2f0`, `FUN_0017bc30` | wanted yaw +0x66c from +0x10, facing +0x55c turned at +0x11c x frame step |
| `FUN_0017af10`, `FUN_0017b040`, `FUN_00179880` | ground mover: motion class, displacement along +0x10; mover table |
| `FUN_001778e8`, `FUN_001ceda8` | controller tick (base, Sora) |

## Layouts (C7, WP-FAC-8)
- Actor header (first 0x9c0 bytes, `FUN_00168d30`): +0 vtable, +4 self, +8 objentry, +0xc current mover, +0x10 direction, +0x20
  velocity, +0x108/+0x10c flags, +0x140 motion controller, +0x3b0 team, +0x48c status block, +0x540 position, +0x550 rotation, +0x588
  class flags, +0x670 model instance, +0x81c spawn vector, +0x82c fade ramp, +0x8b4 object list link, +0x9ad list priority. Player
  classes add +0xaf0..+0xb28 (`FUN_001746e0`), +0xb38..+0xb60 (`FUN_00176e00`) and Sora +0xb68..+0xb70 (`FUN_001ce820`, size 0xb80).
- Motion controller at actor +0x140, 0x1c8 bytes (`FUN_0017d768`; end inferred from the next field at +0x308): +8 owner, +0x14 motion
  id, +0x18 serial, +0x1c mode (all -1 at start), +0x20..+0x28 flags, +0x30 time, +0xf0 the owner's effect list, +0xf4 rate,
  +0xf8/+0xf9 range bytes, +0x14c sixteen words of -1.
- Motion addresses traced (WP-RIG-4, PCSX2, state .06): the player pointer 0x35c998 is the actor base; actor +0x154 (controller +0x14)
  read 0 idle, 1 walk, 2 run, 151 the ground attack; actor +0x158 = id * 4 + 2 there (the slot); actor +0x170 (controller +0x30) the
  motion time, +2.0 per field frame, wrapping at actor +0x16c (180 idle, 72 walk, 40 run, 50 attack in that state) [inferred: the loop
  length]. Rig names `sora.motion`, `sora.motion_frame` (`scenarios/values.toml`).
- Movers are shared descriptors, not per-actor blocks: `FUN_00179868(id)` reads the pointer table at 0x35ca48 (ids 0 to 0x3f) built by
  `FUN_00179880` at 0x1c88eb0; `FUN_0016b648` calls the descriptor's enter slot and stores it at actor +0xc. All mover state is in the
  actor block (+0x10, +0x20, +0xf0, +0x880). The port stores the descriptor's static address and keeps the table as constants.

## Open gaps
- No dodge roll and no Quick Run tap/hold threshold exist in the USA executable (A180 and slot 838 are Final Mix only); the app's Square rule is
  ported from Final Mix (`movement.md` "Gaps", `foundations/final-mix.md`).
- `FUN_001783d8` (Square action with mover `0x16`, actor `+0xb50` = 6) is unidentified; Glide and Quick Run are the app's own states
  (`moveset-selection.md` "Not ported or not resolved").
- Camera: collision has one unresolved case, the event camera is only partly ported, the world map camera (type 4) behaviour and the camera at
  Sora's head after the last kill are not reproduced (`camera.md` "Gaps", `camera-event.md`).
- The Roxas player class is not identified among the six derived player classes (`controller.md` "Player classes"); the head look-at
  (0x1ae9e8..0x1aee90) is located, not read; Quick Run camera speed: `camera.md` "The drawn view and the player".

## Where the details are
Section numbers in references ("section 5", "20.14") are the old chapter numbers that still stand in the headings of the files below.

| file | KB | covers (section numbers are the old chapter numbers) |
|---|---|---|
| [camera-event.md](camera-event.md) | 34 | Event camera; Talk camera; What does not change the camera -- verifie; Gaps; Checked in the app; Controls in the app; +2 more |
| [camera.md](camera.md) | 25 | The camera object; Per-tick order; Parameters and where they come from -- ver; Field of view -- verified; Follow camera; Smoothing; +4 more |
| [controller.md](controller.md) | 9 | Player classes; The stick vector; Heading and facing; Ground mover; The controller tick; Square actions; Head look-at; What the port does today; Gaps |
| [movement.md](movement.md) | 43 | sections 1, 2, 3, 4, 6, 6 |
| [moveset-selection.md](moveset-selection.md) | 27 | Where the table comes from; The attack press; The table selection; What this gives for Sora's table; Not ported or not resolved; The keyblade at A343 frame 18; +2 more |

Index of all systems: [../../README.md](../../README.md).
