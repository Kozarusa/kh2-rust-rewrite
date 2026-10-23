# Research: what exists for Kingdom Hearts II (surveyed 2026-10-03)

| Source | License | Content | Use for kh2-rs |
|---|---|---|---|
| [OpenKH/OpenKh](https://github.com/OpenKH/OpenKh) | Apache-2.0 | 57 KH2 format docs (`docs/kh2/`; web version at openkh.dev); C# libraries for all formats; a partial MonoGame engine (`OpenKh.Game`); PS2 graphics-data decoding (`OpenKh.Ps2`); a file-name list | Main format reference; it targets Final Mix and the PC release first, so verify against the USA disc |
| [govanify/kh2](https://github.com/govanify/kh2) | MIT | Matching decomp of the KH2 **Final Mix** ELF (`SLPM_666.75`), about 41% matched; ~6.5k symbols; class and file layout recovered from debug symbols in a PS3 E3 kiosk build | Names and structure of game code; the code is close to the USA ELF |
| [OpenKH editions.md](https://github.com/OpenKH/OpenKh/blob/master/docs/kh2/editions.md) | Apache-2.0 | Edition id per release: USA `SLUS_21005` at `0x349D44`; confirm button, space width and save header per edition | Edition behaviour |
| [Yokimitsuro/khdays-decomp](https://github.com/Yokimitsuro/khdays-decomp) | CC0 | Complete matching decomp of 358/2 Days (DS) | Shared conventions, loosely |
| [govanify.com: RE of MDLX](https://govanify.com/post/re-3d-model-mdlx) | – | Model-format reverse-engineering notes | Background for `kh2-mdlx` |

No Rust KH2 code exists, so every reader here is new.

## Verified on the USA disc (by `kh2 validate`)

- `KH2.IDX` has 4,925 entries. With the 19 world sub-indexes (`000xx.idx`) there are 26,461; 26,456 are named by the OpenKH list.
- `OVL.IDX`/`OVL.IMG`: 4 LZ-compressed code overlays, using the same IDX format.
- 18 tables decode with exact sizes. PS2 differences from OpenKH: `stop` entries are 4 bytes (u16, u16). `atkp` has 277 trailing bytes that are not modelled yet.

## Motion rig (checked against the game with `kh2 rig-sweep`)

KH2 motions are a control rig, not plain FK. The USA motion entry point is `0x130ad8`, which runs setup, curves, constraint activations and the rig solve. The same three routines named in OpenKH's `Mlink.cs` for another build sit 0x30 lower in the USA executable. `kh2-ee` runs them on the user's own executable, and `kh2-anim/src/rig.rs` reproduces their behaviour:

- Key times are **60 Hz ticks**: `tick = frame × 60 / fps`. Each curve has pre/post modes (hold, extrapolate along the end tangent, repeat, zero).
- Joints are evaluated in joint-table order. A single cursor walks the constraint table, and an IK chain root consumes constraints while it searches for its goal.
- Position constraints and limiters work in the parent's *final* (scaled) space. Children inherit rotation from the parent's *unscaled* world matrix, whose translation is only relative.
- In two-bone IK, flag value 3 flips the knee or elbow. The second joint's translation is the first joint's length (the bone record's scale.w).
- Limiter quirks reproduced: an undamped sphere limit never clamps, and the damped box's minimum side adds its soft term.
- Expressions do trig in degrees; `AT_FRAME` reads a curve at `x × 60 / fps`; `VARIABLE_FC` is `frame_start + frame`.

Result: every interpolated motion of Sora (base, Valor, Wisdom, Limit, Master, Final and Anti forms), the Kingdom Key, the Shadow and the Soldier matches within 0.08 units on a roughly 180-unit character. The one exception is a Soldier arm at full extension, at 0.15. The leftover difference is the game's polynomial sin/cos.

## Player controller (behaviour read from the game's code; nothing copied)

- **Coordinates:** the game's world is model space with Y and Z negated. This holds for COCT vertices and normals, motion roots, and object matrices.
- **Logic rate:** 60 Hz. All `prty`/`fmab`/`sstm` speeds are units per tick.
- **Input:** the stick maps camera-relative. Below 0.8 magnitude Sora walks, above it he runs; there are only two speeds. Ground velocity equals direction × speed instantly, with no acceleration.
- **Turning:** at most prty TurnSpeed (0.2618 rad) per tick, snapping when the remaining angle is within that.
- **Jump of height h:**
  - T = sqrt(2h/g), with g = 20/49.
  - Rise: y = base + h·sin(sin(t/T·π/2)·π/2). Fall: parabola ½g(t−T)², with each tick's drop capped at FallMax.
  - Short hop: releasing jump between tick 6 and T−5 replaces the rest of the rise with a 250/49-unit bump. A ceiling ends the rise.
- **Air control:** v = 0.9·v + 0.1·input, or ×0.95 with no input.
- **Root motion:** motions with an enabled root block (word at root+0x0C) move the object by the root's per-tick delta, rotated by facing. Root yaw deltas turn the object, and poses are drawn relative to the root.
- **Growth abilities** (level = the stats byte; the `fmab` row is level − 1; level 5 is MAX):
  - **High Jump:** jump height HighJumpHeight, motion 201.
  - **Aerial Dodge:** a second jump of AirDodgeHeight at AirDodgeSpeed, once per jump below MAX, motion 202.
  - **Quick Run:** speed AirSlideSpeed, ×AirSlideBrake per tick while held, for at most AirSlideTime ticks. Then StopBrake and end motion 204; star time AirSlideStarTime.
  - **Glide:** square in the air at least GlideStartHeight above the ground. Speed = speed·accel + GlideSpeed·(1−accel) along facing; turns at GlideTurnSpeed; sinks min((h−FallHeight)·FallRatio, FallMax); ends on release or below GlideEndHeight. Motions 205–208.
