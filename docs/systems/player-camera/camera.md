# The field camera

[Contract](contract.md) | [Index](../../README.md)

What the USA build's camera does and how `crates/kh2-game-core/src/camera.rs` (Bevy side) and `camera_rules.rs` (pure math, unit tested) reproduce it. Everything was read from a local decompilation and disassembly of the user's executable; nothing is copied. Function addresses are USA addresses (`SLUS_210.05`, file offset = address - 0xff000). Each claim is marked **verified** (read directly in the code, checked against the data) or **inferred** (follows from the code but depends on an assumption named next to it).

Coordinates. The game keeps Y pointing down; the app uses map-model space, Y up (a rotation of 180 degrees about X, so handedness is the same). Below, heights are written Y up. All lengths are game units (cm). The logic rate is 60 Hz and every per-tick rule below is applied once per tick; the app runs the camera in `FixedUpdate` right after the simulation (`Sim::EffectTick`).

## The camera object

One global object at `0x355840`, updated by a task (`FUN_00140be0` -> `FUN_00140a80`). Fields (offset: meaning), **verified** unless marked:

| Offset | Meaning |
|---|---|
| 0x00 / 0x10 / 0x20 | smoothed look-at point / smoothed camera position / smoothed (field of view, roll); these are what is drawn (`FUN_001407b8` ends by handing 0x10 and 0x00 to the projection) |
| 0x30, 0x34 | field of view and roll for this tick; reset every tick to the defaults at 0xdc, 0xe0 (`FUN_0013f508`) |
| 0x38, 0x39 | flags set by scripts: warp (snap the smoothing, `FUN_00140e48`) and reset behind the player |
| 0x3a | auto-camera enabled this tick |
| 0x3b | lock-on active |
| 0x3c | parameter row |
| 0x40 | mode |
| 0x44 | the followed object (the player) |
| 0x48 | orbit radius |
| 0x4c / 0x50 | current yaw / target yaw |
| 0x60 / 0x70 | wanted camera position / last tick's |
| 0x80 / 0x90 | wanted look-at point / last tick's |
| 0xa0 / 0xb0 | wanted (fov, roll) / last tick's |
| 0xc0, 0xc4 | look-around yaw and pitch (mode 1 only) |
| 0xc8, 0xcc | default radius, lock-on radius |
| 0xd0, 0xd4 | smallest and largest radius |
| 0xd8 | height curve factor |

## Per-tick order (`FUN_00140a80`) -- verified

1. `FUN_00140768`: copy wanted position, look-at and fov to their "last tick" slots.
2. Mode update, a switch on the mode:
   - 0 follow camera (`FUN_001402c8`)
   - 1 look-around from the player's eye (`FUN_00140678` / `FUN_00140530`)
   - 2 lock-on camera (`FUN_0015a0f0`)
   - 3, 5, 6, 7, 8, 9, 10 and 4: scripted and special cameras (`FUN_00158bb8`, `FUN_00158eb0`, `FUN_00158908`, `FUN_00158250`, `FUN_001581a0`, `FUN_0014e5e0`, `FUN_00158518`; 4 is the talk camera, `FUN_00159648`). Their create/destroy neighbours carry the names Cannon, Scope, FPS, Book, LightCycle and Talk in a public matching decompilation; which number is which is **inferred** from that and not needed here.
3. `FUN_0013f520`: wanted fov/roll from 0x30/0x34. `FUN_00140e48`: if the warp flag is set, snap every smoothed value to its wanted value (`FUN_0013f560`).
4. `FUN_001406e8`: if an event (cutscene) camera is active (`FUN_0015c998` = `DAT_00359038`), its position, look-at, fov and roll (`FUN_0015c9d0/9b8/9e8`, from `0x5206f0`, `0x520700`, `0x520710`) replace 0x00/0x10/0x20, and the flag is cleared. See "Event camera" below.
5. `FUN_001407b8`: smoothing, collision, and handing the result to the projection and view (`FUN_00106c48`, `FUN_00107160`).

## Parameters and where they come from -- verified

The parameter table is built by a static initialiser (`FUN_00141018`) from constants in the executable's data. The app reads those constants from `SLUS_210.05` on the disc at run time (`Params::from_exe`); none are in the repository. The addresses, value kinds and meaning:

- Four rows of (fov, radius, lock radius, min radius, max radius, height factor), selected by `FUN_00140d48` into 0xdc, 0xc8, 0xcc, 0xd0, 0xd4, 0xd8. Row 0 is outdoor, row 1 indoor, row 2 and 3 are for area-data camera types (below). Addresses: row 0 at 0x378f34..0x378f48; rows 1-3 share 0x378f04 (fov), 0x378f20, 0x378f24, 0x378f28 and differ in the maximum (0x378f30 for rows 1 and 3, 0x378f4c for row 2); height factor 0x378f2c.
- Yaw swing limit per tick 0x378f08, auto-camera rate base and per-speed term 0x378f0c, 0x378f10, right-stick yaw rate 0x378f14.

Row choice (`FUN_00177128`): `DAT_0035cd38` is set by the area script's camera opcode (OpenKH `areadata.md` "Camera": 0 default, 1 locked overhead with Select, 2 indoor type with lower fov and restricted range, 3 left stick turns the camera). With 0, the row is 1 when the area's `arif` flags (`03system.bin`, OpenKH `Arif.cs`) have bit 1 (indoor) set, else 0 (`FUN_00193478`). Types 1-3 use rows 0, 2 and 3 with other modes (5, and 8 via `FUN_00158200`). All four are implemented (`area_setup`); Twilight Town areas have no camera opcode, so `KH2_CAM_TYPE=1|2|3` forces a type for tests. See "Area camera types" below.

The numbers themselves are game data and are deliberately not written here; run `cargo run -p kh2 -- ...` or read the addresses above. Their shape: the outdoor row has a wider field of view, a shorter radius and a steeper height curve than the indoor row.

## Field of view -- verified

`FUN_00106c48` takes the camera's fov as the **horizontal** angle of a 4:3 picture: `focal = (512/0.8)*0.5/tan(a/2)` and the vertical tangent comes out as `0.75 * tan(a/2)` (the projection constants are 512 x 416 and scale factors 0.8 / 0.8667 set in `FUN_001078f0`). The app sets Bevy's vertical fov to `2*atan(0.75*tan(a/2))` and lets the window aspect widen the picture. Near plane 1 cm.

## Follow camera (mode 0) -- verified

`FUN_001402c8`, in order:

1. **Look-at point** (`FUN_0013f740`): the player's position (the one at object+0x70) raised 170.
2. **Right stick** (`FUN_0013fe20`). The pad stores each stick as a unit direction (x right, y down) with a magnitude 0..1 (`FUN_00167198`). Mapped through the field button table (`0x379768`, selected by `FUN_00167908`), the right stick gives the logical bits `0x40000` up, `0x80000` down, `0x100000` left, `0x200000` right; R3 gives `0x20000`; Select `0x400000`.
   - Stick left: yaw += `stick_yaw * magnitude`; right: yaw -= the same; both 0x4c and 0x50 are set. This is the game's yaw sign; in the app's convention (offset `(sin yaw, ., cos yaw)`) a stick pushed left moves the camera to its own left. Derived from `FUN_0013fc28` (`atan2(-fx, -fz)` puts the camera opposite the facing) and the COCT axis flip. The horizontal/vertical reverse options of the Config menu (bits 7 and 8 of the config word, `FUN_0023edf8`, `FUN_0023ee68`) swap the left/right and up/down masks; the app uses the default (not reversed). **Inferred**: that "not reversed" is the default setting.
   - Zoom (`FUN_0013fd08`): stick up, radius -= magnitude * 8 (clamped to the minimum); stick down, radius += magnitude * 8 (clamped to the maximum). Skipped while a player state flag (`FUN_0019ae68`) is set unless R2 is held. The app does not model that flag.
   - R3 pressed (`FUN_0013fdd8`): target yaw = behind the player, radius = default, the smoothed position snaps to the wanted one; if the left stick is held the yaw snaps too (`FUN_0013f7a0` with the snap flag), otherwise it swings at the yaw limit.
   - Select pressed while the player is controllable (`FUN_0019ae30`): look-around mode 1 (see "Look-around").
3. **Auto-camera** (`FUN_00140128`), only if enabled (`0x3a`; R2 held turns it off for the default config, `FUN_001580e8`), the snap flag is clear, the player moved horizontally since last tick (`FUN_001831f0`), the left stick is held, the right stick is not, and the left stick's y component is under 0.9 (not pulling straight back): yaw approaches "behind the player" by `(|ly| + 1) * 2 * (|lx| + 1) * magnitude * (base + per_speed * (run speed - 8))` radians per tick, and the radius drifts back to its default by 0.5 per tick. Run speed is the object's field +0x114; for Sora it is the `prty` RunSpeed (8), which is why the formula subtracts 8.
4. **Wanted position** (`FUN_0013f7a0`): the yaw moves toward the target yaw by at most the yaw limit per tick (or snaps); then
   `pos = look + radius * (sin yaw, ., cos yaw)` horizontally and `y = look.y - 150 + height_factor * (radius - min_radius)^2`.
   With the outdoor row at its default radius this puts the camera about 80 above the look-at point, a little over 10 degrees down; at the maximum radius about 35 degrees; at the minimum radius below the look-at point.
5. **Collision of the wanted position** against the camera collision (below).
6. Then the common tail: smoothing and a second collision.

## Smoothing (`FUN_001407b8`, spring `FUN_0013f600`) -- verified

With `t = (run speed - 8) * 0.0006 + 0.07` (0.07 on a warp):

- look-at: `spring(cur, new, prev, t + 0.03, 2.5, 0.5, 1)`
- position: `spring(cur, new, prev, t, 2.5, 0.5, 1)`
- fov and roll: the same with rest length 0.001

`spring`: `d1 = cur - new`, `len = |d1|`, `u = d1/len`, `d2 = (prev - new) * t`, `cur += u * t * (2.5 * (1 - len) + 0.5 * u.d2)`. So the camera closes 25% of the gap to its look-at per tick and about 17.5% of the gap to its position, with a feed-forward term from the target's motion. A running character is followed with a lag of roughly 30 units.

## Collision (`FUN_0013f8f8`) -- verified, one case unresolved

- The test ray runs from the look-at point to the camera position (`FUN_00143c28` -> `FUN_00141f08`, mask 0xf) against **the map's camera collision**, a COCT in the map BAR entry of type 11 (OpenKH `bar.md`: "OCH Camera Collision"; named `CH_t...` in Twilight Town maps). The camera object code reaches it through `DAT_00355938`, loaded in `FUN_00142ba8` from the second collision slot of the map description; the main collision (`DAT_00355934`) is **not** used for camera rays. **Inferred**: that slot is the type 11 entry (order of the BAR entries in the maps and OpenKH's name). If a map has no such entry the function returns "no hit" and the camera passes through everything.
- On a hit the camera is placed at the hit point plus 5 along the polygon normal (`DAT_00378e48`; `DAT_00378e44` = 30 when the caller's last argument is 1 and no second cast ran; no field mode passes 1). **Attribute 5** (`FUN_00143e40(polygon) & 0xf == 5`, the low four bits of the COCT flags word): the polygon's normal and hit point are kept, a second cast runs from the look-at point to (end x, hit y, end z) (`FUN_0013f8f8` lines building `auStack_70`; the vector `end - first hit` is only used to fix the y), the point becomes that cast's hit, or (a miss leaves the cast's in/out end point untouched) that constructed end point itself, and the margin is always `DAT_00378e48`. Data: the camera collisions of `tt02` (12 triangles, flags word 0x5) and `tt04` (2 triangles, 0x5) carry it, `tt00` none. **Inferred**: what the surface is for (a slope or ledge the camera may rise over); the second cast's exact end point is read from the decompiled stack variables and could differ in the Y sign handling. Implemented in `collide`; the cast returns the polygon's flags word (`cast_world`).
- Camera rays only hit **front faces** (`FUN_001623a0` returns false when the polygon normal points along the ray). The app's `cast_world` does the same on COCT worlds.
- **Dynamic blockers** (`FUN_0015a418`, `FUN_0015a3e8`, `FUN_0016bce0`): the list at `DAT_00358b90` (0x800 bytes, 0x20 per entry, at most 0x40 entries) is rebuilt on every call from the collision shapes of kind 0x12 that live objects own (`FUN_0016b4e0(obj, 0x12)`; `FUN_001894d0` copies a radius (short at +0x10 of the shape record, here the second word of the first argument) and a centre transformed from a joint). Each entry is a **sphere**. The clip takes the segment from the look-at point to the wanted point, skips a sphere that already contains the start (`FUN_00162c88`), and moves the end to the nearest entry point (`FUN_00162ce0`, ray/sphere with a 0.5 factor on the half-length). **Quirk kept**: on a skipped sphere the original does not advance its list pointer, so every later entry is never tested. `FUN_0013fbc0` (wanted position, follow-type modes) and `FUN_001407b8` (smoothed position, modes 0, 3, 6, 8) pass the flag that tests blockers; the clip result then goes on into the map cast. In the app `CamBlockers` is the per-tick list (`Blocker { center, radius }`), filled each tick by `camera_blockers.rs` before the camera runs: every animated object, every record of kind 0x12 of its model (`Shape` kind 18), at most 64 (`MAX_BLOCKERS`). `FUN_001894d0` writes radius = short at +0x10 of the record; the centre is the record's offset (+8, +0xa, +0xc) in its frame, moved by the short at +0xe (the same point as `fx::shape_point`). Only a record whose bone number has bit 0x4000 (the object's own frames, `FUN_0016aa18`, `fx::object_bone`) or an object with flag bit 0x400 in +0x7f8 gets that centre; any other record is written as radius 0 at (0, 0, 0) and still takes a list slot. The app never sets +0x7f8 bit 0x400 (writer not found, see the gaps), so a record on an ordinary bone is that empty entry. Harness: the log lines `camera: N dynamic blockers` (count changes) and `camera: dynamic blocker clip on|off` (a blocker moved a collided position).
- Applied twice per tick in modes 0, 3, 6, 8: to the wanted position (ray from the chest) and to the smoothed position (ray from the smoothed look-at); a hit on the second also replaces the wanted position so the next tick starts from the corrected one. The lock-on camera casts from the chest in both.
- The app has no polygon normal from `World::raycast_m`, so `pull_in` finds the plane from two extra rays 3 units to the side, checks the pushed-out point is reachable, and otherwise backs off 5 along the ray.

## Lock-on camera (mode 2) -- verified

Entered by `FUN_0015a180` (lock-on starts: sets 0x3b, mode 2, stores the target), left by `FUN_0015a250`. Each tick (`FUN_00159da8`):

- The look-at point is `chest + (target - chest) * 0.3` (`DAT_00379410`). The camera still orbits the chest (`FUN_0015a070` computes the wanted position from the chest, then restores the look-at point).
- The chest and target are projected with last tick's view (`FUN_00159908`) and tested against a safe area of x 32..480, y 8..408 of the 512 x 416 picture.
- A point below the bottom edge pulls the radius in by 2 per tick (checked for the target and for the player).
- If the distance from the look-at point to a point 100 above the player's feet is at most 300, or the direction to it is not steeper than 30 degrees downward: unless a point was below the bottom, the radius relaxes toward the lock radius at 1 per tick; if any point is outside the safe area, the target yaw becomes the direction from the previous look-at point through the player, offset by 20 degrees when the point left through the left or right edge. Otherwise (target far and the player far below the look-at point) the look-at point is the chest.
- The right stick turns and zooms as in the follow camera, R3 sets the yaw as above and the radius to the lock radius.

The target point the app uses is the locked enemy's position raised 100; the game uses the object's own target point (`FUN_0018efe8`, **not decoded**).

## Look-around (mode 1) -- verified unless marked

`DAT_00355880` is the mode field itself (`0x355840 + 0x40`); the mode readers `FUN_00140cd8`, `FUN_00140f98` (mode is 1 and a player exists) and `FUN_00140fe0` (mode is 7) read it.

- **Enter** (`FUN_0013fe20`, follow camera): Select newly pressed (`pad+8 & 0x400000`) while `FUN_0019ae30(player)` holds (player's actor flag word at `*(actor+0xc)+8` has 0x80, `actor+0x160` lacks 0x4000, `actor+0x168` lacks 2): mode = 1, look yaw (0xc0) = yaw + pi (`DAT_00378e50`), pitch (0xc4) = 0 (`FUN_0013fca0`), and the player's draw fade (`actor+0x834`) starts at 1 and runs to 0 over 30 ticks (`FUN_001abf40(0x378e58, 0, 0x378e54, ...)`; `FUN_001abf78` advances it, `FUN_00193270` multiplies it into the draw factor). The follow camera's own tick does not run afterwards.
- **Each tick** (`FUN_00140678` -> `FUN_00140530`) while the player may look and Select is not pressed: R3 (`0x20000`) zeroes the pitch; the camera position (0x60) is the player's position (`FUN_0016b960`: actor+0x70, or +0x540 when it rides something) raised by the character's `prty` `SubjectiveOffset` (`FUN_0016be40`; the 13th float, +0x30 of the row `FUN_001cc1c8` returns, indexed by the model's character number); the yaw (0x4c, 0x50) is the behind-the-player yaw and the look yaw is that plus pi, i.e. **the player's facing**. The right stick does **not** turn the look yaw: only the stick's vertical changes the pitch: `pitch -= stick_y * stick_magnitude * 3 degrees` per tick (`DAT_004842ec` = 0x378f14, the same number as the yaw rate), unless `FUN_0019ae68` (actor flag 8) is set. (The WP brief said "right stick yaw 3 degrees per tick"; the code shows the 3 degrees is the pitch rate and the yaw follows the player.) `FUN_00140418` wraps both angles, clamps the pitch to -70..+70 degrees (`DAT_004842f0/f4` = 0x378f18/1c) and puts the look-at point at camera position + a vector of length 350 (`DAT_00378e74`) along +z rotated by the pitch (`FUN_0011b490`) then the yaw (`FUN_0011b4d8`). **Inferred**: the sign of the pitch against the stick (the app raises the view for stick up; the game's Y-down rotation convention was not traced).
- **Collision**: `FUN_001407b8` case 1 calls `FUN_00140398`: a camera-collision cast between the two points pulls the look-at point to the hit. The direction of that cast (eye to look-at) is **inferred**; the decompiled call's operands are mangled. The camera position itself is not collided.
- **Smoothing** still runs on position and look-at (`FUN_001407b8` before the switch), with the same springs, so the view settles into the head in a few ticks; fov is the row's.
- **Exit** (`FUN_00140530` else branch, taken when the player may not look, or Select is pressed again): `FUN_00140f28(30)` fades the player back in (0 -> 1 over `DAT_00378e78` = 30 ticks) and sets the mode to 0; the yaw becomes behind the player; the rest of that tick is framed like the follow camera without stick handling. Starting a lock-on in mode 1 does the same through `FUN_0015a180`.
- **Player motion** (the brief's `FUN_001abf40`): that function is not a motion call; it is the fade setter (value, step) described above. The app exposes the fade as `OrbitCamera::player_fade` (1 = drawn) and `OrbitCamera::first_person`; `sora::apply_player_fade` applies the fade (below). Whether the original also plays a player motion is **not found** in these functions.
- **Player fade** (WP-5E): the game stores `base alpha * FUN_00193270(player)` in the draw alpha byte (`FUN_00191c18`: `+0x75b = (+0x3af) * factor`), and `FUN_00193270` is `+0x82c * +0x834`, times the parent object's factor for a child object (`+0x570`, unless the child's `+0x576` bit 4), so the keyblade, a child of the player, fades with him. **Verified** by reading. The app does the same: while the fade is below 1, every mesh of Sora's body and weapons switches to alpha blending with the fade as its alpha (the original alpha mode is kept in `sora::FadeBase` and restored at 1). At 0 nothing is drawn. Not checked against the game: that its renderer treats the alpha byte exactly as Bevy's blend does (the PS2 alpha byte is 0..128).
- **The player while looking** (WP-FIX-11, **verified** by reading the player class vtable at 0x35c9a8 in the user's executable: +0x18 is `FUN_001779e8`, +0x84 is `FUN_00179108`):
  - **No movement.** The player's per-tick method +0x18 (`FUN_001779e8`, called from the object update `FUN_001b74e0`) ends with `FUN_00174bd8`, which adds the velocity (+0x10..+0x1c) to the position (`FUN_0016a948`) only when method +0x84 says so. For the player that is `FUN_00179108`: the base test `FUN_00174c28` **and not** `FUN_00140f98` (camera mode 1). So while the look-around runs the player does not move, whatever the left stick does.
  - **The right stick turns the player.** In the same method, while `FUN_00140f98` holds and `FUN_001b7e00(player)` holds (method +0x40, and the motion's range-trigger flags +0x160 have bit 4, 8 or 0x10, the free-move motions: idle, walk, run carry range 0, docs/systems/audio/audio.md "Footstep leftovers"), the logical pad word (`**(player+0xb38)`) is tested: bit 0x100000 (right stick left) adds the float at 0x37a4c8, else bit 0x200000 (right stick right) adds the float at 0x37a4cc, each times the tick scale `DAT_00356f40`, to the player's facing (`FUN_00182318`: +0x55c through `FUN_0016a790`, and +0x66c wrapped by `FUN_0017bbc0`; skipped while `FUN_0019ad98` holds). On the user's disc they are -3 and +3 degrees. Since the camera's look yaw is the player's facing every tick, this is how the view turns left and right: with the camera's right stick, not with the left stick. In the app's yaw (the game's facing `r` is the app's `PI - r`, `enemy::area_groups`) the left push turns the facing (and the view) to the left. Both rates are read from the executable at run time (`LookParams::turn_left/turn_right`, `look_turn`).
  - Who enters: only Select (`FUN_0013fe20`, above). The only store of the constant 1 into the mode field (`DAT_00355880`, or camera +0x40) in the decompiled functions 0x130000..0x170000 is that one; the direct writers of `DAT_00355880` are `FUN_00140c50` and `FUN_00140f28` (0) and `FUN_00140e90` (restores the mode it found after its forced tick). A mode written through a computed value elsewhere would not be found by this search.
- **App** (WP-FIX-11): V (or `KH2_KEYS` `t:V`) is Select and is the only way in (`Rig::tick`, test `look_around_starts_only_from_select`). While `OrbitCamera::first_person` is set, `camera::gather_input` clears the pad's move direction (`sora::Pad::dir`, `stick`) each frame, so Sora stands where he is (the original applies no velocity); the left/right arrows (the right stick) turn Sora by the two rates (`camera::tick` before the rig's tick; test `look_turn_is_the_right_stick_and_turns_left_for_left`), and up/down tilt the view as before. Before the fix the left stick moved Sora in camera-relative directions while the camera yaw itself followed his facing, so any sideways key made him and the view spin (the "glitchy when moving" report). The player is allowed to look in `State::Ground` (stand-in for the actor flags `FUN_0019ae30` and `FUN_001b7e00`, **inferred**). **[inferred]**: what the original does with the left stick's facing and motion requests while looking (the stick-to-motion code was not found to test the camera mode; only the velocity application does); the app ignores the left stick then (Sora idles, does not turn).

## The drawn view and the player (WP-L2-S1)
- The player's stick is mapped by the drawn view (smoothed position +0x10 to look-at +0x00, after collision, via `FUN_00107160` and the
  matrix of `FUN_00106f78`), not by the orbit yaw +0x4c (`controller.md` "The stick vector"). The port maps it by `OrbitCamera.yaw`.
- The auto-camera and zoom read the pad's sticks. (Until WP-L2-P2 the port's `camera::tick` read the keyboard only, so a rig pad script
  never turned the camera; see "Pad sticks, the frame step and the oracle" below.) Rig tt-run-06: both cameras stand at x 870 (the room's camera collision [inferred]); the original's slides to z 405 by
  k 80 while the port's stays at 390.6.
- Quick Run camera speed: the smoothing time and the auto-camera rate take the followed object's run speed +0x114 each tick, so they
  follow whatever writes +0x114; the port passes the `prty` row's RunSpeed. Quick Run (`FUN_00178108`) writes its speed into the move
  vector, not +0x114 (`movement.md` section 2), so during Quick Run the camera keeps the run-speed spring and lags more [inferred: no
  other writer of +0x114 for the player was searched; an L2 package checks it with a rig scenario].

## Pad sticks, the frame step and the oracle (WP-L2-P2)
- **Sticks from the pad.** `camera::tick` takes its sticks from `rig::PadState` when a pad script fills it (`scripted`), through the
  original's dead zone and scale (`rig::stick_vector`, `FUN_00167198`), and R3 and Select as button edges; without a script the
  keyboard is the pad as before (WASD, arrows, C, V, mouse). So a rig pad script runs the auto-camera (`FUN_00140128`) and the stick
  yaw and zoom like the original.
- **Frame step.** The original multiplies every per-frame camera rate by `DAT_00356f40` (2.0 for a field frame at 30 Hz, else 1.0):
  the right-stick yaw (`FUN_0013fe20`), the zoom (`FUN_0013fd08`), the auto-camera rate (`FUN_00140128`), the radius drift
  (`FUN_00140068`) and the yaw swing limit (`FUN_0013f7a0`); the springs of `FUN_001407b8` take no step. The camera task runs once per
  game frame. The app runs the logic as many 60 Hz ticks per frame as the step says, so at a step above 1 the camera runs on the
  frame's last tick only, with `TickIn::step` as the factor (before, it ran every tick: two springs per frame pulled the camera in
  about twice as fast, and the rates were halved). At 60 Hz (step 1.0, the default for play) nothing changes.
- **R3 with the left stick held** (`FUN_0013fe20` returns 1, `FUN_0013f7a0` is passed 1) also snaps the smoothed look-at point and fov to
  the wanted ones (`FUN_0013f560`); the port snapped the position only.
- **The rig's `cam.pos` and `cam.look`** are the camera object's wanted position `+0x60` and look-at point `+0x80` (`values.toml`), not the
  drawn ones: `OrbitCamera::wanted` carries them (game units, map-model axes).
- **Oracle** (`crates/kh2-game-core/tests/orig_l2_p2.rs`): kh2-ee runs 0x13f740, 0x13f600, 0x140768, 0x13fd08, 0x140068, 0x140128,
  0x13f7a0, the smoothing branch of 0x1407b8, 0x1402c8 (collision stubbed) and the task 0x140be0 in mode 0 on a camera object at
  0x355840, and the port's `Rig` on the same values. The yaw of the original is `PI` minus the port's yaw (the port's Y-up mirror).
  Not covered: modes 1 to 10 of 0x140a80, the script flag `+0x39`, Select, the event camera. The logical stick bits of the pad layer
  (`FUN_00157f98` and its three siblings) are stubbed from the stick's direction [inferred].
- **Rig result** (`movement/tt-run-06`, 2026-10-24): the camera rows pass at k 40 to 320 (position within 1.1 cm, look-at point within 2.1 cm,
  the latter being Sora's own offset); `core-runtime/area-change` k 42 to 120: camera and Sora within 0.26 cm, facing within 5e-5, then
  the original's Sora is deflected and slowed by an obstacle at about (660, -380) that the port does not have (not a camera matter).
  Remaining camera-side residue: the eye sits 1 cm off in z from the first moving frame on in tt-run-06, because Sora's facing differs by
  0.15 degrees at k 42 (original 57.25 degrees, port 57.43) and the auto-camera follows the facing; the cause is in the heading or the
  turn (WP-L2-P1's code), not found.
- **Gaps**: the rotation of `FUN_0013f7a0` is the game's own sine series (`FUN_002fb628`, `FUN_002fb460`; the cosine is the root of one minus
  the sine squared), the port uses libm: positions differ by up to 5e-4 of the radius near a quarter turn. The script flag `+0x39`
  (`FUN_00140288`) has no port counterpart. `FUN_0019ae68` (the player's up-down flag that skips the zoom unless R2 is held) is not modelled.

## Area camera types -- verified unless marked

`FUN_00177128` (run when an area loads) from `DAT_0035cd38`, set by the area script's camera opcode:

| type | `FUN_00140c50` row | mode | extra |
|---|---|---|---|
| 0 | 0, or 1 if `arif` indoor (`FUN_00193478`) | 0 follow | |
| 1 | 0 | 5 overhead (`FUN_00159248`) | |
| 2 | 2 | 0 follow | restricted maximum radius (row 2: 0x378f4c) |
| 3 | 0 here; `FUN_00158200` then re-selects row 3 | 8 behind (`FUN_00158200`) | |

(The type-3 first call `FUN_00140c50(.., 0)` is overwritten by `FUN_00140d48(camera, 3)` in `FUN_00158200`, so the effective row is 3.)

- **Mode 5, type 1, overhead** (`FUN_00158eb0`): a fixed-angle camera. Each tick, unless the player is not controllable, Select toggles `DAT_00358b08` (`FUN_00158e50`, also gated by `FUN_00161b60`, which the app does not model). In the following view the target is the point `raise` (150, `0x3793c8`) above the floor under the player, found with a vertical camera cast from 100 above the player to 10000 below (`FUN_00158d88`; the player's own position if no hit); the look-at point eases to it: it moves `gap * min(gap, 200) / 200` (200 = `0x3793cc`) per tick, so the last 200 units close quadratically; the camera is the look-at point plus a fixed world offset (`0x3793a8`, `0x3793b0`: 600 in x and 500 up) and the fov is `DAT_005204b0` (= `0x3793d0`). In the Select view (`DAT_00358b08`) the position and look-at are fixed world points (`0x3793b4/b8`, `0x3793bc/c0`: (51, 1492 up) and (256, 1777 up) with z 0) and the fov is `0x3793c4`. Nothing is smoothed (`FUN_0013f560` copies the wanted values to the drawn ones) and there is no collision; the project's `FUN_001407b8` case 5 is not in the collision list, so none runs. The camera's roll is 0.
- **Mode 8, type 3, behind** (`FUN_001581a0`): while the player may be controlled, `FUN_00158130`: R3 held (`0x20000`) resets the radius to the default, else the right stick zooms (`FUN_0013fd08`); the yaw is always the behind-the-player yaw (no stick turn, no auto-camera). Then the usual look-at point (`FUN_0013f740`), wanted position (`FUN_0013f7a0`) and collision (`FUN_0013fbc0`). Smoothing and the second collision run as in mode 0 (mode 8 is in that case list).
- **Type 2** is the follow camera with table row 2 (the indoor fov and radii, a larger maximum). Nothing else differs in the code that was read (**inferred**: `FUN_00140d48(.., 2)` is the only effect).
- The tables' fovs/radii for rows 2 and 3 are in `Params`; the tests cover row selection and each mode's rule. Not covered: what the opcode's argument bytes look like in an area script (OpenKH `areadata.md` "Camera" describes the type; not decoded here).

