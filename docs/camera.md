# The field camera

What the USA build's camera does and how `crates/kh2-app/src/camera.rs` (Bevy side) and `camera_rules.rs` (pure math, unit tested) reproduce it. Everything was read from a local decompilation and disassembly of the user's executable; nothing is copied. Function addresses are USA addresses (`SLUS_210.05`, file offset = address - 0xff000). Each claim is marked **verified** (read directly in the code, checked against the data) or **inferred** (follows from the code but depends on an assumption named next to it).

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
4. `FUN_001406e8`: if an event (cutscene) camera is active (`FUN_0015c998`), its position, look-at, fov and roll replace 0x00/0x10 and the rest is skipped in effect. Not ported.
5. `FUN_001407b8`: smoothing, collision, and handing the result to the projection and view (`FUN_00106c48`, `FUN_00107160`).

## Parameters and where they come from -- verified

The parameter table is built by a static initialiser (`FUN_00141018`) from constants in the executable's data. The app reads those constants from `SLUS_210.05` on the disc at run time (`Params::from_exe`); none are in the repository. The addresses, value kinds and meaning:

- Four rows of (fov, radius, lock radius, min radius, max radius, height factor), selected by `FUN_00140d48` into 0xdc, 0xc8, 0xcc, 0xd0, 0xd4, 0xd8. Row 0 is outdoor, row 1 indoor, row 2 and 3 are for area-data camera types (below). Addresses: row 0 at 0x378f34..0x378f48; rows 1-3 share 0x378f04 (fov), 0x378f20, 0x378f24, 0x378f28 and differ in the maximum (0x378f30 for rows 1 and 3, 0x378f4c for row 2); height factor 0x378f2c.
- Yaw swing limit per tick 0x378f08, auto-camera rate base and per-speed term 0x378f0c, 0x378f10, right-stick yaw rate 0x378f14.

Row choice (`FUN_00177128`): `DAT_0035cd38` is set by the area script's camera opcode (OpenKH `areadata.md` "Camera": 0 default, 1 locked overhead with Select, 2 indoor type with lower fov and restricted range, 3 left stick turns the camera). With 0, the row is 1 when the area's `arif` flags (`03system.bin`, OpenKH `Arif.cs`) have bit 1 (indoor) set, else 0 (`FUN_00193478`). Types 1-3 use rows 0, 2 and 3 with other modes (5, and 8 via `FUN_00158200`). The app implements type 0; Twilight Town areas have no camera opcode.

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
   - Select pressed while the player is controllable: look-around mode 1 (not ported, see Gaps).
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
- On a hit the camera is placed at the hit point plus 5 along the polygon normal (`DAT_00378e48`; 30 for another caller that passes a flag). Polygons whose attribute (low four bits) equals 5 trigger a second cast with a different end point (`FUN_00143e40` == 5); what that surface class is is **unresolved**, the app treats it like any hit.
- A list of dynamic blockers (spheres/boxes registered by objects, `FUN_0015a418`) is tested first when the caller asks; not ported (no such objects yet).
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

## What does not change the camera -- verified by absence

The follow and lock-on updates read the player's position, facing and run speed, the pad, and nothing else of the player. There is no code in them for jumping, falling, gliding, getting hit or blown away, combos, finishers, guarding or rolling: the camera follows the look-at point (player position + 170) through the spring, so a jump lifts it with the lag. The game's scripts can warp or reset the camera (`YS::trap_camera_warp` `FUN_00173cd8`, `trap_camera_reset` `FUN_00173cf8`), mission and event cameras replace it, and special areas select other modes; none of that exists in the app yet.

## Gaps (not ported, or unknown)

- Look-around mode 1 (Select): a first-person view from the player's eye (`FUN_0016be40`), right stick turns (yaw 3 degrees per tick) and tilts (pitch limited to +-70 degrees, `FUN_00140530`), look-at point 350 ahead. Not ported; it also plays a player motion (`FUN_001abf40`).
- Scripted/event/talk/cannon/scope/book/light-cycle cameras (modes 3-10, event override), the area-data camera types 1-3, the dynamic blocker list, the unresolved collision attribute 5.
- The lock-on target point and the exact lock key behaviour (R2/L1 variants) are only partly decoded. R2's auto-camera switch and the zoom-block player flag (`FUN_0019ae68`) are not modelled.
- Player run speed uses the base `prty` value, so Quick Run does not speed up the camera's smoothing.
- The camera collision is the map's type 11 data only; surfaces the original treats specially (attribute 5) are plain walls here.
- The `KH2_CAM_RENDER` environment variable switches the camera's collision to the render meshes (the old behaviour) for comparison.

## Checked in the app

Scripted scenes with the harness (`KH2_KEYS`, `KH2_CAM_LOG=1` logs the camera twice a second):

- Twilight Town `tt00` and `tt04` take row 0 (their `arif` flags have no indoor bit), `tt02` takes row 1 (indoor bit set). The camera collision entries have 18 (`tt00`, a box around the arena), 337 (`tt02`) and 395 (`tt04`) triangles and parse with the ordinary COCT reader.
- Standing: the camera sits straight behind Sora at the row's default radius, a little above the look-at point, looking slightly down.
- Right stick: Left held for one second turns the camera half a revolution (60 ticks of 3 degrees), Up held for a second takes the radius to the row's minimum, C returns yaw and radius to the defaults.
- Auto-camera: running right (D) swings the camera behind Sora (yaw settles opposite his facing) and he follows an arc, as camera-relative control does in the game.
- Wall: backing Sora toward the arena wall pushes the camera against it; the camera stops 5 off the wall and the orbit radius shrinks from the default to under half.
- Lock-on (Q): the camera swings so Sora is between it and the target and the radius relaxes to the lock radius.
- Sora's movement keys stay relative to the camera yaw (`OrbitCamera.yaw`, read by `sora::read_pad`).

## Controls in the app

WASD left stick (Ctrl half tilt), arrow keys right stick (left/right turn, up/down zoom), C or middle mouse button R3 (reset behind Sora), right-drag and wheel turn and zoom directly, Page Up/Down zoom. Movement stays relative to the camera yaw. The arrow keys follow the stick rule above, so Left moves the camera to its left; the old orbit camera moved it the other way. Right-drag keeps its old sense (the view follows the mouse).

Harness: `KH2_YAW` and `KH2_DIST` (metres) still set the yaw and the orbit radius; the harness keys gained `A S D C Left Right Up Down`. `KH2_CAM_LOG=1` logs the camera, `KH2_CAM_RENDER=1` collides with the render meshes instead of the map's camera collision.
