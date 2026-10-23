# Player and camera: contract

Ladder step L2. Ledger system name `player-camera`. Crates: `kh2-anim` (`moveset.rs` attack selection rules, `blend.rs`, `triggers.rs`),
`kh2-player` (`sora.rs`), `kh2-game-core` (`motor.rs`, `camera.rs`, `camera_rules.rs`, `camera_blockers.rs`), `kh2-event` (`intro.rs`).
Sheet ids: `game.player`, `game.sora`, `game.sora.dodge`, `game.intro`. Scenarios: `scenarios/player-camera/` (none yet).

Status: Sora's ground combo, air chain, air swing momentum, Square actions, Quick Run and the follow, look-around and lock-on cameras are ported;
the event and talk cameras are partly ported. The Roxas player class, camera type 4 (world map cursor) and several camera modes are open.

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

## Open gaps
- No dodge roll and no Quick Run tap/hold threshold exist in the USA executable (A180 and slot 838 are Final Mix only); the app's Square rule is
  ported from Final Mix (`movement.md` "Gaps", `foundations/final-mix.md`).
- `FUN_001783d8` (Square action with mover `0x16`, actor `+0xb50` = 6) is unidentified; Glide and Quick Run are the app's own states
  (`moveset-selection.md` "Not ported or not resolved").
- Camera: collision has one unresolved case, the event camera is only partly ported, the world map camera (type 4) behaviour and the camera at
  Sora's head after the last kill are not reproduced (`camera.md` "Gaps", `camera-event.md`).
- The Roxas player class, look-at of the head and Quick Run camera speed (L2 scope) have no doc yet.

## Where the details are
Section numbers in references ("section 5", "20.14") are the old chapter numbers that still stand in the headings of the files below.

| file | KB | covers (section numbers are the old chapter numbers) |
|---|---|---|
| [camera-event.md](camera-event.md) | 34 | Event camera; Talk camera; What does not change the camera -- verifie; Gaps; Checked in the app; Controls in the app; +2 more |
| [camera.md](camera.md) | 25 | The camera object; Per-tick order; Parameters and where they come from -- ver; Field of view -- verified; Follow camera; Smoothing; +4 more |
| [movement.md](movement.md) | 43 | sections 1, 2, 3, 4, 6, 6 |
| [moveset-selection.md](moveset-selection.md) | 27 | Where the table comes from; The attack press; The table selection; What this gives for Sora's table; Not ported or not resolved; The keyblade at A343 frame 18; +2 more |

Index of all systems: [../../README.md](../../README.md).
