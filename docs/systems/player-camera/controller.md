# The player controller (ladder step L2)

[Contract](contract.md) | [Index](../../README.md)

How the USA executable turns the left stick into a heading, a facing, a speed and a motion, written for the L2 ports (WP-L2-S1, spec
`local/specs/L2.md`). Read from the user's executable (`SLUS_210.05`, file offset = address - 0xff000) and the PCSX2 rig traces of
`movement/tt-run-06` and `core-runtime/area-change` (state .06, Roxas in the Mansion computer room tt21). Inferences are marked
`[inferred]`. Constants are named by address; the ports read them from the executable at run time.

## Player classes
- The base player class vtable is at 0x35c9a8 (constructor `FUN_00176e00`, actor +0x588 |= 0x80). Its per-tick slots: +0x14
  `FUN_001778e8` (controller tick), +0x18 `FUN_001779e8` (right-stick turn while looking, then `FUN_00174bd8`), +0x84 `FUN_00179108`
  (may the stick move the actor), +0x90 `FUN_001772f8` (read the stick into the actor). Sora's class at 0x360410 (`FUN_001ce718` /
  `FUN_001ce820`, +0x588 |= 0x1000000) replaces +0x14 with `FUN_001ceda8` (calls `FUN_001778e8` first, then the summon and reaction
  checks), and +0x08, +0x1c, +0x20, +0x54, +0x5c, +0x98.
- Five more classes derive from the base (constructors `FUN_001ccbb0` vtable 0x3601e0, `FUN_001cd450` 0x360288, `FUN_001cda78` 0x360360
  with +0x588 |= 0x2000000, `FUN_001cf370` 0x3604b8 with +0x588 |= 0x4000000, and two in 0x2cc8a0 / 0x2d79d0). Which one is Roxas is
  not known [inferred: 0x3601e0 or 0x3604b8; the classes 0x360288 and 0x360360 are cited by the drive docs]. The first L2 Roxas step is
  a trace: read the word at actor +0 of the player 0x35c998 in state .06. The two classes 0x3601e0 and 0x360288 replace the stick slot
  +0x90 (`FUN_001ccd20`, `FUN_001cd748`); 0x3604b8 keeps the base one.

## The stick vector (`FUN_001772f8` -> `FUN_001774d8`) -- read
- `FUN_001772f8` picks the stick reader by the current mover's flag word (`*(actor+0xc)+8`) and id (`*(actor+0xc)+4`): flag 8 ->
  `FUN_001775e8`; id 2 -> `FUN_001775a0` (stick zeroed, y set to the constant 0x37a4c0 when the stick points down); id 0x18 or 0x35 ->
  `FUN_00177710` (an object at 0x36fd60 maps the stick); every other mover -> `FUN_001774d8`.
- `FUN_001774d8`: when the left stick's magnitude (pad record at actor +0xb44, +0x9cc) is above 0, the stick record (+0x9c0: unit x right,
  y down; magnitude in w; dead zone and scale by `FUN_00167198`, constants 0x37968c and 0x379690) goes through `FUN_00177360`; the
  actor's move vector +0x10 takes the result and its w becomes the actor's walk speed +0x110 when the magnitude is below 0.8
  (0x37a4bc), else the run speed +0x114. Without stick: +0x10 = (`FUN_00300278`, 0, `FUN_003000d8`) of the
  wanted yaw +0x66c [inferred: sine and cosine, the libm pair at 0x300000], with w = 0 (no speed).
- `FUN_00177360` is the camera mapping: the vector (x, 0, -y, 0) is multiplied by the transpose of the 4x4 matrix at +0x10 of the view
  block `FUN_001068c0(1)` (0x3b25f0 + index * 0x90, index `DAT_00349e40`); y is cleared; the vector is normalised; w = the magnitude.
  That matrix is built by `FUN_00106f78` (called from `FUN_001076e8`) from the view block's eye (+0x50) and look-at point (+0x60):
  forward = normalise(look - eye) at +0x30, the side axis from the cross product with the up vector +0x70. `FUN_001407b8` writes the
  camera's smoothed position (camera +0x10) and look-at point (camera +0x00) into that block through `FUN_00107160`.
- So **stick up moves the player along the drawn view's horizontal forward (camera position to look-at point, after smoothing and
  collision)**, stick right along the view's right. It is not the camera's orbit yaw (camera +0x4c): where collision holds the camera,
  the two differ. [inferred: the player's tick reads the view of the previous frame, since the camera task runs after the objects;
  the rig agrees: at k 42 the heading equals the direction from the port's k 40 camera position to Sora within 0.2 degrees.]

## Heading and facing -- read
- `FUN_00174bd8` (end of the +0x18 slot): when slot +0x84 allows it (`FUN_00179108` = `FUN_00174c28` and not look-around), the wanted
  yaw +0x66c = atan2(x, z) of +0x10 (`FUN_0016a948`). `FUN_00174c28` holds when the stick magnitude +0x1c > 0, the actor flags +0x168
  have 4 clear or 8 set, the current mover's flag word has 0x100 (the ground mover's word is 0x180) and +0x160 bit 31 is clear.
- The facing follows in the object tick `FUN_0016a2f0`, after the mover's tick slot (+0x18): unless `FUN_0019ad98` (free rotation)
  holds, facing +0x55c = `FUN_0017bc30(+0x55c, +0x66c, turn speed +0x11c * frame step DAT_00356f40)`, which moves the angle toward
  the wanted one by at most the step, wrapping with `FUN_0017bbc0`; `FUN_0016a790` writes it back (direction vector +0x550 by
  `FUN_0017be98`). The port's `Motor::turn_to` uses the turn speed without the frame step [inferred: twice too slow at frame step 2;
  the rig cannot tell, the 90 degree turn at k 120 is done by k 130 on both sides].
- Movement does not wait for the facing: the ground mover moves along +0x10 at once (below), so a sharp turn moves sideways for a few
  frames while the body turns.

## Ground mover (mover 0, descriptor 0x1c88eb0, vtable 0x35cbb0, flags 0x180) -- read
- Table built by `FUN_00179880`: mover 0 = 0x35cbb0 (ground), 1 = 0x35cbe0 (jump, flags 0x181), 2 = 0x35e2d0, 3 = 0x35cb80,
  4 = 0x35e4a8, 5 = 0x35e4d8, and so on to 0x3f.
- Tick slot +0x18 `FUN_0017af10`: the motion class from the speed in +0x10 w: 0 idle, 1 walk, 2 run when w >= (walk + run) * 0.5
  (0x37a4e8); a player (+0x588 bit 0x80) also starts foot effects (`FUN_001b1c90` 10 and 11 on the first moving tick, 8 and 9 on the
  stop) unless its part number (`FUN_001699a8`) is 0xe. Then `FUN_0017aeb8` requests motion (0x37a4e4 + class) through
  `FUN_0017e2d8` when +0x160 has bit 4, or bit 0x10 and the class is not 0 (the free-move ranges).
- Displacement slot +0x1c `FUN_0017b040`: unless +0x160 bit 0x40 (then `FUN_001832b8` with +0xf0 twice) or +0x164 bit 1 (then
  `FUN_001832b8` with the row at +0x12c, +0x28 / +0x2c by +0x810): with speed w > 0 the step is normalise(+0x10) * w * frame step and
  actor +0x20 = that displacement times w again (oracle, WP-L2-P3; not a plain velocity); without speed, both are 0. The fourth word of the result is 1.0 (0x37a4ec).
- Rig numbers (state .06, frame step 2): 16.00 cm per game frame at full stick (Roxas run speed 8), first move on the frame after the
  input, no acceleration, stop on the first frame without input (`local/specs/RIG.md` 1.2).

## The controller tick (`FUN_001778e8`) -- read
Unless +0x10c bit 4: the stick slot (+0x90); a pending command at the command record (+4 non-zero) clears +0xb48, and with record flag
0x10 goes through `FUN_00178c40`; `FUN_00197b70(0x1f, id)`; `FUN_00174cf8` runs the attack press and returns a command to start
(`FUN_001b9da8`, +0x108 bit 2 cleared). Outside the free-move ranges (+0x160 bits 4, 8, 0x10 all clear) and with +0x160 bit 31 clear,
the move vector +0x10 is zeroed (no stick during attacks). Then `FUN_00179140`; always `FUN_00176528`.

## Square actions and growth (summary; details in `movement.md` section 2)
`FUN_00177ea0` (slot +0x80) chains guard `FUN_00178080`, Quick Run `FUN_00178108`, `FUN_00178290`, `FUN_001783d8` (unidentified,
mover 0x16) and `FUN_00177fa8`; High Jump and Aerial Dodge act in the jump mover (`FUN_0017b278`, `FUN_0017b6a0`). Glide is the app's
own state. Quick Run's camera: the camera reads the player's run speed +0x114 every tick (smoothing time and auto-camera rate,
`camera.md`), so a run speed changed by a growth level changes the camera speed with it; the port passes the `prty` value instead.

## Head look-at (`FUN_0016a2f0` -> `FUN_001aea28`) -- located, not read
Every object tick ends with the look-at unit on the actor's look-at block (+0x4a0, 0x60 bytes [inferred from the typed layout];
functions 0x1ae9e8..0x1aee90, the largest 0x1aee90 at 1984 bytes). The rows were labelled event.player (events drive it too); L2-S1 moved them to player.sora. Who sets
the target for the player (an enemy, a talk partner) is not read [gap].

## What the port does today (2026-10-24, integration f1a23f8)
- `sora::read_pad` maps the stick with `OrbitCamera.yaw` (the rig's orbit yaw), not the drawn view: under collision the heading stays
  at the start heading (rig tt-run-06: facing constant from k 40 to 120, original turns from 212.0 to 214.0 degrees as the camera
  slides along the wall).
- `camera::tick` builds the camera's sticks from the keyboard only, so under a rig pad script the auto-camera (`FUN_00140128`) never
  sees the left stick and the camera never swings behind Sora (rig cam.pos 14 cm off at k 80, 334 at k 160).
- The facing turn has no frame step (above); the motion class uses `move_speed < run_speed` (same result for Sora's rows).

## Decompile index
0x1772f8, 0x1774d8, 0x177360, 0x1775a0, 0x1775e8, 0x177710, 0x1068c0, 0x106f78, 0x1076e8, 0x107160, 0x174bd8, 0x174c28, 0x179108,
0x16a948, 0x16a2f0, 0x16a790, 0x17bc30, 0x1778e8, 0x1779e8, 0x1ceda8, 0x179880, 0x17af10, 0x17aeb8, 0x17b040, 0x2f2828, 0x167198;
vtables 0x35c9a8, 0x360410, 0x3601e0, 0x360288, 0x360360, 0x3604b8, 0x36fb00, 0x36fe60, 0x35cbb0, 0x35cbe0 read from the executable.

## Gaps
- Roxas's class (above). The rate +0x11c for the player: its writer for players was not found (`FUN_002f2828` sets it for one
  mover from the row at +0x12c, +0xc); the port's `prty` turn speed is the stand-in.
- The caller of `FUN_001076e8` (no direct call in the decompiled text; an indirect call [inferred]).
- `FUN_001775e8` (mover flag 8): its button bits 0x20000000 / 0x40000000 / 0x100000000 setting y from +0x124 were not identified.
