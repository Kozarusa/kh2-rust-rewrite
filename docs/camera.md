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
  - **The right stick turns the player.** In the same method, while `FUN_00140f98` holds and `FUN_001b7e00(player)` holds (method +0x40, and the motion's range-trigger flags +0x160 have bit 4, 8 or 0x10, the free-move motions: idle, walk, run carry range 0, docs/audio.md "Footstep leftovers"), the logical pad word (`**(player+0xb38)`) is tested: bit 0x100000 (right stick left) adds the float at 0x37a4c8, else bit 0x200000 (right stick right) adds the float at 0x37a4cc, each times the tick scale `DAT_00356f40`, to the player's facing (`FUN_00182318`: +0x55c through `FUN_0016a790`, and +0x66c wrapped by `FUN_0017bbc0`; skipped while `FUN_0019ad98` holds). On the user's disc they are -3 and +3 degrees. Since the camera's look yaw is the player's facing every tick, this is how the view turns left and right: with the camera's right stick, not with the left stick. In the app's yaw (the game's facing `r` is the app's `PI - r`, `enemy::area_groups`) the left push turns the facing (and the view) to the left. Both rates are read from the executable at run time (`LookParams::turn_left/turn_right`, `look_turn`).
  - Who enters: only Select (`FUN_0013fe20`, above). The only store of the constant 1 into the mode field (`DAT_00355880`, or camera +0x40) in the decompiled functions 0x130000..0x170000 is that one; the direct writers of `DAT_00355880` are `FUN_00140c50` and `FUN_00140f28` (0) and `FUN_00140e90` (restores the mode it found after its forced tick). A mode written through a computed value elsewhere would not be found by this search.
- **App** (WP-FIX-11): V (or `KH2_KEYS` `t:V`) is Select and is the only way in (`Rig::tick`, test `look_around_starts_only_from_select`). While `OrbitCamera::first_person` is set, `camera::gather_input` clears the pad's move direction (`sora::Pad::dir`, `stick`) each frame, so Sora stands where he is (the original applies no velocity); the left/right arrows (the right stick) turn Sora by the two rates (`camera::tick` before the rig's tick; test `look_turn_is_the_right_stick_and_turns_left_for_left`), and up/down tilt the view as before. Before the fix the left stick moved Sora in camera-relative directions while the camera yaw itself followed his facing, so any sideways key made him and the view spin (the "glitchy when moving" report). The player is allowed to look in `State::Ground` (stand-in for the actor flags `FUN_0019ae30` and `FUN_001b7e00`, **inferred**). **[inferred]**: what the original does with the left stick's facing and motion requests while looking (the stick-to-motion code was not found to test the camera mode; only the velocity application does); the app ignores the left stick then (Sora idles, does not turn).

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

## Event camera (modes 3-10 and the override) -- partly ported

- **Override**: `FUN_001406e8` runs every tick after the mode update: when `DAT_00359038` is set (by `FUN_0015c8f8(1)`, called from the event/cutscene frame `FUN_002233d8` while an event with a camera track is playing) the camera object's drawn look-at, position, fov and roll are replaced by the event's (`0x5206f0`, `0x520700`, `0x520710`, roll `0x520714`) and the flag is cleared again; the event player sets it each frame. `FUN_0015c8c8` ends the override (clears the flag, fades the player in, un-pauses the camera task `FUN_00106818(1)`, and `FUN_0015ca08` reseeds the event springs from the field camera).
- **Per event frame** (`FUN_0015ce00`, called from `FUN_0022b0f8` once per camera track entry with the track's position, look-at, fov and roll): the previous wanted values are saved (`FUN_0015cb08`); optionally (the sixth argument) the position is pulled in with `FUN_0013f8f8` (blockers per the seventh argument); optionally (the eighth argument) the three springs run (`FUN_0015cb70`, `FUN_0015cd20`): look-at with (t 0.1, k 2.5, c 0.5, rest 1), position (t 0.07, same k, c, rest), and (fov, roll) (t 0.07, k 2.5, c 0.5, rest 0.001); the result goes to the projection. Constants are `0x379470..0x379490`, read at run time (`EventParams`).
- **Hand-over**: `FUN_0015c8f8(1)` (the event camera starts) pauses the field camera task (`FUN_00106818(0)`), and calls `FUN_00140e90`, which places the field camera behind the player (`FUN_0013fc90`, `FUN_0013f470`), runs one forced tick (`FUN_00140a80(.., 1)`: no collision, warp-style) and returns its position/look-at pair; then the event's drawn values are read from the projection (`FUN_001071c8`, `FUN_001072d8`, `FUN_00106d90`, `FUN_00107318`). At the end `FUN_0015c8c8` clears the override, resumes the task and reseeds the event springs (`FUN_0015ca08`). Which values the field camera continues from after an event is **inferred**: the app's `Rig::handover` resets the rig behind the subject and installs the event's final view so the spring runs on from there. The one-forced-tick details beyond this are **not ported**.
- **App**: `EventCamera` (`start`, `frame`, `view`) is the pure rule; the `CamEvent` resource holds an active event camera (the field camera idles while it is set, like `FUN_00106818(1)`) and a pending hand-over view. A mission's start event drives it since WP-I1 (below); no map script does yet.
- **The intro camera (WP-I1, `intro.rs`, `CamEvent::track`).** A mission's start event (docs/struggle.md "The
  battle intro") carries a camera track, and the app now plays it:
  - *Records* (event opcode 0x13, setup `FUN_0022b4f8` stores the command by its index +4 in `DAT_0036bbb0`):
    eight channels, each a pair (u16 first key, i16 key count) at +6..+0x24, keys of 16 bytes from +0x28: word 0 the
    type (top 3 bits) and the time in 1/512 s (low 29 bits, signed), then value, incoming and outgoing tangent.
    Channels in order (`FUN_0022adb8`): position x, y, z, look-at x, y, z, roll and field of view, both in degrees
    (0x37f144 / 0x37f148 and 0x37f15c are pi / 180); y and z are negated into the game's Y-down space, so the keys
    are the app's (Y up) coordinates as they stand; a channel with no keys keeps 0 (the field of view 1.0,
    0x37f154). Port `intro::Track`, `intro::shot`.
  - *Evaluation* (`FUN_0022ab08`): time = event frame / 30 (0x37f138) in seconds, key time units 512 a second
    (0x37f13c); before the first key and after the last the value runs on along a line (to the neighbour for a
    linear key, else the key's own tangent, per key time unit); between keys `FUN_0022a9d8`: type 0 holds, 1 is
    linear, 2..4 a cubic Hermite with key i's outgoing and key i+1's incoming tangent, others 0. Port
    `intro::channel`; test `the_camera_keys_evaluate_as_fun_0022ab08_does` runs the game's own `FUN_0022ab08` in
    kh2-ee over every channel of the Seifer intro and 160 frames (worst relative difference 3.7e-6).
  - *Cuts* (opcode 6, `FUN_0022a970` -> 128 entries `id, start, end` at `DAT_0036bbb4`): each frame
    `FUN_0022b0f8` evaluates every record whose cut holds the event frame `DAT_0036bcf0` at the frame minus the cut
    start, transforms position and look-at by the event's matrix `DAT_0036bce8` and calls `FUN_0015ce00` with the
    roll, the field of view, the map pull-in flag (`DAT_0036bce4`, opcode 0x46) and its blocker flag
    (`DAT_0036bce5`, opcode 0x46 +6) and the spring flag (`DAT_0036bcdb`, opcode 0x4c). `DAT_0036bce8` is the
    event object's +0x50 (`FUN_00195ae0` -> `FUN_002232c8` param 2), which `FUN_00195898` fills by `FUN_0011b460`
    from 0x354a60 (zero in the file: filled at run time) [inferred: the unit matrix, so the keys are map
    coordinates; the app uses the unit matrix, and the clip's wide shot and Seifer shot backgrounds agree].
  - *Space of the keys (FIX5, 2026-10-22, user report: the orbit spawned under Sora).* The unit-matrix reading above
    (map coordinates) put the orbit of a field battle's start event at the map origin: under the floor where the ground
    is above the key heights, and 10 m from Sora in `TR03_MS103` (orbit of radius 3.6 m round (5, 92, 0), no fly-back).
    The original's own frames (`local/reports/pcsx2-ref/combat-intro-orbit-1..6.jpg`, a Twilight Town battle start,
    PCSX2) swing round Sora at chest height with the ground below him, so the keys are in the player's space. The app
    now maps them with `intro::player_space` (origin his feet, +z his facing, +y up, +x his left [inferred handedness]),
    refreshed each tick. Evidence for the space: `TT04_MS500`'s first look-at (0, 107, 355) is the 3.46 m to Seifer at
    head height, its last key looks at the origin from 5.6 m behind. `DAT_0036bce8` stays the unit matrix in the code
    (no writer found, `FUN_0022b0f8` multiplies by it); how the original gets the player's space for the other tracks
    is not found [gap]. `TT04_MS500`'s wide shot had matched the unit matrix with the b_45 places: with the player's
    space it needs his entry facing (-90 degrees); the harness placement in `enemy.rs` now sets it (`AreaGroup::player_yaw`,
    app yaw `PI - r`). Looked at (tt04 b_42, b_43, b_44, b_45, tr03 b_40): every start orbit circles Sora at chest height with the
    ground below him and the enemies and friends around; the direction of turn against the PCSX2 frames is not measured. Test
    `the_keys_are_in_the_players_space`.
  - *The fly-back* (opcode 0x31 +6 = 1, `FUN_0022bc40` -> `FUN_0022b590`, at its frame +4): two keys of type 2 with
    zero tangents per channel (the work track at `DAT_0036bd60`), at `(frame << 9) / 30` the view drawn then
    (`FUN_001071c8`, `FUN_001072d8`, `FUN_00107318`) and at the end frame `DAT_0036bcf8` the field camera placed
    behind the player (`FUN_00140e90`), both kept in the player's space (`FUN_0016aa08`, the player's matrix); from
    then on `FUN_0022b0f8` evaluates this track (`FUN_0022af88`) instead of the records. The app takes both ends
    in map space at the first frame (the player does not move while the start layout holds him) and eases between
    them (`intro::follow`; `CamEvent::behind` is `Rig::new` for the current subject, filled by `camera::tick`).
    Test `the_fly_back_eases_from_the_event_view_to_the_field_camera`.
  - *Hand-over*: while `CamEvent::track` comes the event camera runs `EventCamera::frame` with it (no springs: the
    intro has no opcode 0x4c; the pull-in against the map when opcode 0x46 is present); when it stops the field
    camera takes the last view over (`Rig::handover`, as `FUN_0015c8c8`). Logs `camera: intro camera on` / `off`.
  - *The smear at the cut* (the clip, 4.0 to 4.5 s): the start event's opcode 0x3c at frame 120 (`FUN_00226188`:
    +4 frame, +6 effect, +8 mode; mode 0 `FUN_001db868(DAT_0036bd54, effect, 1, 3, 0x9c41, 0)`) starts effect 0 of
    the mission's own effect file (BAR type 0x12 `miss`, loaded into the manager's +0xc by `FUN_001db4d0` in
    `FUN_001b8ee8`). Its particle data 0 (life 12): a camera-facing shape (`pppDrawMatrixFront`, `pppDrawShapeX`)
    and three `pppSnoMdlDttNorm` lens models (one model, 768 vertices within +-800 x +-800 x -800..300; the screen
    shifted along the normals, amplitude about -20 fading, colour alpha 112 down to 0). WP-I2 makes it draw:
    - `pppKeDMat` (handler 0x1fa130, table entry 0x364bb0; view * bind (0x1db0f80) times the object's +0x10 into
      the program's work slot 1, then the projection 0x1db1000) is ported in kh2-fx (`prog::ke_dmat`; the lens
      reads its model matrix there; without it every triangle failed the clip test, w = 0). Oracle test
      `ke_dmat_matches_the_game` (12 random bind, view and local matrices, the game's handler in kh2-ee).
    - Element flag 0x20000 (both elements of the file have it): `FUN_001dc138` sets bit 2 of the bind record
      (+0x5c), and `FUN_001dd0d0` then calls `FUN_001dc978` every tick: the element's own translation (+0x1c),
      rotation (+0x28) and scale (+0x34) are put through the camera's inverse view (`FUN_001068c0(1)`), so the
      effect rides in camera space. Effect 0's element sits at (0, 0, 5.0) with scale 0.0189: the lens fills the
      view 5 units in front of the camera. `fx::tick` does the same for any element with the flag
      (`fx::ELEMENT_IN_CAMERA_SPACE`, `env.bind = view_inv * element_matrix`).
    - Seen: `local/i2_effect_compare.png` (clip / just before / during): during the 12 ticks the figures show
      doubled, shifted copies (Seifer's coat and Sora's legs ghosted) and are clean again after.
    - **WP-I3: the lens reads the display buffer.** The effect has ONE `pppSnoMdlDttNorm` program (group 2,
      three keyed entries at frames 0, 8 and 12), and every entry has byte +0x14 set, +0x15 set (taper), +0x16
      clear, +0x17 set (no depth test), +0x1c zero (ALPHA 0x44) (test `kh2-fx/tests/intro_lens.rs`
      `the_intro_lens_reads_the_display_buffer`). With +0x14 set the draw handler 0x20a7e8 builds the texture
      register from `DAT_00360b04`, `DAT_00360b08`, `DAT_00360b1c` (512 x 448 at GS page 0), else from `DAT_00360b0c`,
      `DAT_00360b10`, `DAT_00360b18` (512 x 416 at page 0xd8); the values come from `FUN_001da4b8` ->
      `FUN_001e4350` (stack arguments 0x70, 0xd8, 0: read from the instructions at 0x1da528..0x1da598).
      `FUN_0014dcd0` sets page 0 up as the display buffer (512 x 448) and page 0xd8 as the draw buffer (512 x 416,
      Z at 0x70), and `FUN_0014dfe8` builds the packet that copies each finished frame from the draw buffer to the
      display buffer; the field's FRAME register is 0x800d8 (the draw buffer) in `FUN_0011d4f8`, `FUN_0013e858`,
      `FUN_0014b000` and others. So while the scene is drawn the lens shows the PREVIOUS frame's picture, HUD
      included, shifted along the normals and tinted, blended over the new scene; frame after frame the ghost is
      carried on and shifted again. That is what the clip shows [measured, `local/clip_sheet.png`]: at 4.10 to
      4.30 s the camera already looks at the board behind Roxas while a fading ghost of the wide shot (Seifer on
      the left, the arched door) stays over it; at 4.35 s the HUD's INFORMATION text and deck appear doubled.
      The earlier reading (each lens batch reads the frame buffer being drawn, three batches compounding) is
      wrong: there is one batch and it reads the other buffer.
    - **The effect clock** (WP-I3, measured): one program frame per field frame. The draw `FUN_001e3d00` advances
      the instance by one frame (`FUN_001e6240`: time +0x58 += 0x1000, the objects step, the life test); the
      update `FUN_001e3898` adds the frame step (`DAT_00360b00` = `DAT_00356f40`, set by `FUN_001daa20` through
      `FUN_001e4620`) to an accumulator +0x98 and runs one more frame only for each 2.0 (`DAT_0037ce98`) beyond the
      first, a catch-up for slow frames. The field frame takes two vblanks (step 2.0), so an effect frame lasts
      1/30 s and the lens's life of 12 frames 0.4 s (frames 120 to 132 of the event, the clip's 4.0 to 4.4 s).
      Oracle test `kh2-fx/tests/clock_oracle.rs` `the_effect_program_runs_one_frame_per_two_vblanks` (the game's
      update and draw in kh2-ee: step 2.0 x 30 frames -> 30 program frames; step 4.0 x 15 -> 30). The app ran
      one per 60 Hz tick: every effect ran twice too fast. `fx::TICKS_PER_PROGRAM_FRAME` = 2 now applies to all
      effects (the bind record's clock, the start wait and the fade still step every tick, as the game scales
      them by the frame step).
    - **The app** (`fx_lens.rs`, `fx::lens`): batches with byte +0x14 set are not meshes; `fx::tick` keeps them as
      `LensBatch`es (camera-space elements in view space, so they stay in front of the camera as drawn) in
      `LensDraws`. Render: the 2D cameras (HUD, menu) copy the finished picture into a history texture after they
      draw (Core2d post-process); the 3D camera's post-process copies the scene and draws the lens triangles over
      it from the history of the previous effect frame (blend SrcAlpha / OneMinusSrcAlpha, texture position and
      colour interpolated linearly on the screen, as the GS does). The history textures swap only when a new
      effect frame drew a lens (or no lens is up), so both rendered frames of a field frame show the same ghost.
      Batches with +0x14 clear keep the transmission path (`DistortMaterial`). Seen:
      `local/i3/compare1.png` (clip 4.10 / 4.20 / 4.30 s against event frames 122 / 126 / 128): strong ghosts of
      the earlier views stay over the new view and the INFORMATION bar is doubled, as in the clip. Harness:
      `KH2_INTRO_SHOTS=<prefix> KH2_INTRO_SHOTS_AT=116,118,...` with `KH2_WAVE_GROUP=b_45 ... kh2-game tt04`.
    - **WP-FX13: the history has the display buffer's size.** `fx_lens.rs` `HISTORY_SIZE` = 512 x 448 (GS page 0,
      `FUN_0014dcd0`), not the window's: each feedback pass (copy down, bilinear sample, copy down) blurs by a
      console texel, so the ghost stretches into a smooth blur instead of separate copies [inferred from the
      buffer sizes above; the clip's blur width was not measured]. Seen (looked at, event frame 126 harness shot):
      Sora's legs and the right-hand fighters are smoothly smeared sideways. No before shot was taken in this
      worktree. Test `the_history_is_the_display_buffer_size`. Addresses: 0x20a7e8, 0x14dcd0, 0x14dfe8 (read earlier).
    - **Still different** [measured, same images]: the clip's ghost is a smooth stretched blur, the app's showed (before WP-FX13)
      separate copies a few pixels apart (the app's picture is 1280 x 720 against the console's 512 x 416, so
      the same shift in screen fractions leaves sharper steps; the console's bilinear sample of a half-size
      buffer smooths them) [inferred]; and the views ghosted differ because the fighters stand elsewhere in shots
      1 and 2 (docs/struggle.md gap 1). Not modelled: the vertical offset between the 448-line display buffer
      and the 416-line draw buffer (taken as the same screen), GS alpha factors above 1.0 (clamped by the blend),
      and the depth test for display-buffer lenses with byte +0x17 clear (none is known).
    - **Open after WP-FX13** (review follow-up, WP-PL1): the "smear" the user reported was never pinned to a scope. Whether it is
      only the intro's lens ghost (the cut at 4.0 s, the effect above) or any other effect that reads the display buffer was
      not confirmed by the user, so the fix covers the intro lens only. The residual stepping that remains comes from the
      416-line draw buffer against the 448-line display buffer: the app takes both as the same screen and does not model the
      vertical offset between them (listed under "Not modelled" above).
- **Area events (WP-E6)**: `event_player.rs` drives the same `CamEvent::track` from an area event's camera
  records (opcode 0x13) and cut table (opcode 6), parsed by `intro::Track`, evaluated at the event frame
  (`FUN_0022b0f8`) every game tick while the event plays; the keys are taken in map space as for the intro
  [inferred, same `KEY_SPACE`]. Run `KH2_EVENT=102 kh2-game tt02`: the log lists the event's 23 cuts and the
  camera logs `intro camera on`; the screenshot at 29 s (looked at) shows the event's view of the friends,
  not the field camera behind Sora. Not ported: opcode 0x31 fly-back and 0x4c springs for area events (the
  fields are parsed; only the intro uses the fly-back).
- **Modes 3, 6, 7, 9, 10** (`FUN_00158bb8`, `FUN_00158908`, `FUN_00158250`, `FUN_0014e5e0`, `FUN_00158518`) are **not ported**: mode 3 is a lock-on-style framing on a script target (`FUN_00159da8` with `DAT_00379394` and `FUN_0018f0c0(0x358af0)`), the others are the special-purpose cameras (names only inferred, see above). Only the entry points are recorded. Mode 4 is the talk camera (next section).

## Talk camera (mode 4, WP-E8) -- verified unless marked

Started by the talk task (`FUN_001a5728`, docs/events.md section 3 "Talk") and ended by it; nothing else calls the start or the end (`mips_scan --jal 0x159688 / 0x159768`: one call each, at 0x1a5810 and 0x1a5928).

- **Start** `FUN_00159688(record, a1)`: `FUN_00140f28(DAT_003793d8)` ends a running look-around (player fade-in over that many ticks, mode 0), then the mode is 4; the 5-word target record (the player's reaction record, controller +0xb44 +0xc, copied by `FUN_001a53f8` to `0x1c95574`: target object, part index, ...) is copied to `0x358b18..0x358b28`, `a1` to `DAT_00358b11`, and `DAT_00358b10` = 1 (place the view on the next tick). It also calls `FUN_00161b20(DAT_003793dc)` (below). The caller passes `a1` = 1 unless the target is a class-5 object (+0x588 bit 3) for which `FUN_00189e90` answers 0, i.e. its bit array +0xb04 has bit 0x10 or 0x40 (read at 0x1a57e0..0x1a5804).
- **Each tick** `FUN_00159648`: when `DAT_00358b10` is set it is cleared and `FUN_001592d8` places the view; then, every tick, `FUN_0013fbc0` collides the wanted position (look-at point to wanted position, blockers on). `FUN_001407b8` smooths as usual; mode 4 is not in its collision cases (0, 3, 6, 8 / 1, 7, 9, 10 / 2), and the mode does not reset the fov (`FUN_0013f508` is not called), so the talk fov stays the wanted one.
- **The view** `FUN_001592d8`: `FUN_0018f108(0x358b18)` gives the look-at point and two numbers: the target's collision record of type 0x13 whose part index is the record's word 1 (`FUN_0016b558(target, 0x13, part)`; `FUN_0016b558` compares the record's byte +0), read by `FUN_001894d0`: the centre (offset in the joint's frame, minus the lift +0xe) is the look-at point, the radius +0x10 the distance and +0x12 the height. Without such a record `FUN_0018eec0` takes the type-3 record with that index, else the first type-3 one (`FUN_0018ee20`), else the object's position (then the two numbers are not written: stack values, **not ported**). Then:
  - `a1` = 1: yaw = `atan2` (`FUN_003003e8`) of the player's position (`FUN_0016b960` of the followed object) minus the look-at point in x and z, wrapped (`FUN_0017bbc0`); a rotation by it (`FUN_0011b460`, `FUN_0011b4d8`) of `(0, 0, distance)` is added to the look-at point and the height subtracted from y (game Y down: raised). The camera sits on the horizontal line from the look-at point through the player, `distance` out, `height` up.
  - `a1` = 0: the target's matrix (+0x6b0..+0x6ec) is inverted (its rotation transposed, translation `-R^T t`), the look-at point taken into the object's frame, moved by `(0, 0, -distance)`, the height subtracted from its y, and taken back: the camera is behind the look-at point along the object's local z. **Inferred**: that the matrix is the object's facing rotation with local +z forward (as `reaction_parts::shape_of`).
  - The fov is `DAT_00520500` (set from 0x3793e4 by the static initialiser `FUN_00159808` -> `FUN_001597a0`, which also clears `0x358b18`), the roll 0, and `FUN_0013f560` copies the wanted position, look-at and fov to their last-tick and drawn slots: the view **cuts** to the talk shot.
- **End** `FUN_00159768`: a task `FUN_00159728` (`FUN_00102bc8(1, 0x6586, ..)`) sets the warp flag (`FUN_00140e28`) and the mode to 0, or 2 with a lock-on (`FUN_00140ce8`); the next tick's `FUN_00140e48` resets the fov to the row's and snaps every drawn value to the follow camera's wanted one: a **cut back** to the follow camera with the yaw and radius it had before the talk (mode 4 never touches 0x48 / 0x4c / 0x50). It also calls `FUN_00161b20(DAT_003793e0)`.
- **`FUN_00161b20` -> `FUN_00161a10(t, 1)`** at both ends (t = 8 from 0x3793dc / 0x3793e0): starts render tasks (`FUN_001618b8` with `+4 = 2t`, rate `128 / 2t` from 0x3794f8; `FUN_00161b70` installs a list of draw tasks; `FUN_001619d8` -> `FUN_0014de00` -> `FUN_0014dbe8(.., 0x484480)`; `FUN_001616e8` -> `FUN_00161570` draws two 512 x 416 sprites with alphas `DAT_0035a86c` / `DAT_0035a868`). **Inferred**: a full-screen dissolve from a copy of the previous picture over 2t = 16 frames that hides the two cuts. **Not ported** (render side; the sprites' textures and the alpha steps `LAB_00161810` / `LAB_00161750` have no decompilation).
- **App**: `rules::TalkShot`, `rules::talk_pose` (the pose of both branches), `Rig::start_talk`, `Rig::end_talk` and the mode-4 tick (`Mode::Talk`); `camera::CamEvent::talk` takes `TalkCam::Start(shot)` / `TalkCam::End` from `npc::talk_step`, which builds the shot with `npc::talk_shot` (the record lookup above, on the object's own frame only). The constants are read from the executable (`TalkParams`: 0x3793e4, 0x3793d8). Logs `camera: talk camera on (FUN_00159688, mode 4) ...`, `camera: talk view placed (FUN_001592d8, a cut) ...`, `camera: talk camera off ...`, `camera: back to the follow camera ...`. Tests `talk_pose_places_the_camera_past_the_player_or_in_the_objects_frame`, `talk_camera_cuts_in_holds_and_warps_back`, `exe_table_round_trip` (camera_rules.rs), `tt02_townspeople_carry_their_talk_camera_records` (npc.rs, disc).
- **Disc data** (that test): each of tt02's four townspeople (N_EX500_RTN, N_EX510_RTN twice, N_EX520_RTN) has one type-0x13 record with part index 0 (its talk part's), on a bone (joints 60, 125, 65), offset 2 or 3 along the bone's x, lift 0 or -5, distance 90 or 80 and height 0: the look-at point is the head (about 125..141 cm up in the rest pose) and the camera sits 80..90 cm from it toward Sora at the same height, a face close-up. `FUN_001894d0` reads a bone record only when the object's +0x7f8 has bit 0x400, which `FUN_001b7be8` sets (with 0x100) after the object's pose was computed (`FUN_0017e928` into +0x6b0); the app reads the bone from the body's last pose (`Animator::pose`, the offset in the bone's frame as `combat::shape_volume` does) [inferred: one frame old; the 2..3 cm offset's axis].
- **Run** (`KH2_PROGRAMS=0,0,10 KH2_NPC_WATCH=1 KH2_NPC_WATCH_DIST=120 KH2_REACTIONLOG=1 KH2_NPCLOG=1 KH2_REACTION_PRESS=7 KH2_KEYS=11:J,14:J KH2_NO_ENEMIES=1 KH2_NO_SOUND=1 KH2_NO_MUSIC=1 KH2_SHOT_AT=18 kh2-game tt02`): `camera: talk camera on (FUN_00159688, mode 4): look [-324, 152, 279], distance 90, height 0, AroundPlayer, fov 1.047`, `camera: talk view placed (FUN_001592d8, a cut): pos [-291, 152, 195] ...`, then after the window closes `camera: talk camera off ...` and `camera: back to the follow camera: pos [37, 211, 480] look [-251, 170, 89] yaw 0.63 radius 600`. Seen (screenshot at 9 s): the townsperson's head and shoulders fill the picture with the message window under them; at 18 s the field camera is behind Sora again. Not compared with the original: the reference clips have no townsperson talk (`local/VIDEOS.md`).

## What does not change the camera -- verified by absence

The follow and lock-on updates read the player's position, facing and run speed, the pad, and nothing else of the player. There is no code in them for jumping, falling, gliding, getting hit or blown away, combos, finishers, guarding or rolling: the camera follows the look-at point (player position + 170) through the spring, so a jump lifts it with the lag. The game's scripts can warp or reset the camera (`YS::trap_camera_warp` `FUN_00173cd8`, `trap_camera_reset` `FUN_00173cf8`), mission and event cameras replace it, and special areas select other modes; none of that exists in the app yet.

## Gaps (not ported, or unknown)

- Modes 3, 6, 7, 9, 10 (scripted/cannon/scope/book/light-cycle cameras): entry points only (above). The talk camera (mode 4) is ported except the dissolve `FUN_00161a10` and the fallback without a type-0x13 / type-3 record on the object's frame. The event camera track's format is decoded and a mission's start event drives it (WP-I1, "The intro camera"); map-script events (area triggers) do not yet. The smear at the intro's cut is drawn since WP-I3 (a display-buffer feedback lens); what still differs is listed there.
- The area camera opcode (what sets `DAT_0035cd38`) and its argument are not decoded; the types work through `KH2_CAM_TYPE`.
- Dynamic blockers: the list is filled from the kind 0x12 records (`camera_blockers.rs`). Not modelled: the object-list walk order of `FUN_001b7c88` (the app walks its animated bodies in query order; it matters only past 64 entries); the skip of objects with `+0x108` bit 0x80000 or 0x10000000 (`FUN_0019a440`); the per-group enable test (`FUN_0019a6a0`: the lists at `+0x474` / `+0x47c` and the mask at `*(obj+0x8d8)+4`), so every record counts as enabled; the party members' weapons for the numbers 0x4003..0x4008 (the object's own blade is used). The only writer of flag bit 0x400 in `+0x7f8` found is `FUN_001b7be8` (ORs 0x500 at the end; called twice from `FUN_001b7a90`, the loop over the object queue at `DAT_01d5d7b0`, which looks like object teardown); no clear of the bit exists; readers are `FUN_001894d0` and `FUN_0019ada8`. All 30 stores to `+0x7f8` were checked (`mips_scan --store-off 0x7f8`). So a live object does not have it and records on ordinary bones give the empty entry [inferred]; whether some object gets the bit another way (a store through a register mask) is open. Models with kind 0x12 records: B_HE110, B_HE100, B_HE020, B_MU120, B_EX260, B_LK120, F_AL170, F_TT000, N_DC010_RTN/BTL, N_PO010_SIT_RTN (scan cut short; none of the app's spawnable enemies has one).
- The look-around: the fade (`actor+0x834`) is applied to the model (`sora::apply_player_fade`); the player's pose while looking is not found in the original and not invented; the stand-in for the player-state flags (`FUN_0019ae30`, `FUN_001b7e00`: ground state only); the left stick while looking (ignored in the app, see "The player while looking"); the pitch sign; the direction of the look-at collision cast. Whether the original plays a player motion while looking is not found in the camera code.
- The lock-on target point and the exact lock key behaviour (R2/L1 variants) are only partly decoded. R2's auto-camera switch and the zoom-block player flag (`FUN_0019ae68`) are not modelled.
- Quick Run speed: the smoothing time reads the actor field `+0x114` ("run speed", `FUN_001407b8`: `(speed - 8) * 0.0006 + 0.07`; also the auto-camera rate). It is stored per actor at spawn from a table (`FUN_001a3448`: `+0x114 = +0xc` of an entry; `FUN_002d8430`/`FUN_002de620` copy it between actor blocks). WP-5E searched every function in the decompilation that contains `0x114`: the writers are `FUN_001a3448` (the spawn-time copy above, reached only through `FUN_0018e5e8` by a pointer), `FUN_002d8430`/`FUN_002de620`/`FUN_002defd8`/`FUN_002df1a0` (a different object class, copies between blocks), and unrelated structs (`FUN_0013ecf8`, `FUN_00145bc8`, `FUN_001e3d00`, `FUN_001f1d10`, ...). The Quick Run start `FUN_00178108` (growth slot 1) writes the velocity (+0x20 from the actor's direction times the fmab row's speed), +0xf0/+0xf4/+0xf8 (the row's brake, time and star time), motion 0xcb and mover 0x10, and never +0x114; the player-side functions that touch it (`FUN_001774d8`, `FUN_001775e8`, the stick-to-speed scaling) only read it. No code that writes it from the Quick Run ability (fmab) was found, so it is **not known** that Quick Run changes it; the app keeps the base `prty` RunSpeed (8) rather than invent a value. Hook if it is later proven: `Subject::run_speed` in `camera.rs::tick`.
- The camera collision is the map's type 11 data only. Attribute-5 polygons follow the rule above; the *meaning* of the surface is not stated by the code.
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
- Look-around (`KH2_SCREENSHOT=x.png KH2_SHOT_AT=13 KH2_CAM_LOG=1 KH2_KEYS=5:V,8:Up:1.5,11:V`, `kh2-game tt00`): `mode Look` with the camera 150 up at Sora's eye ([115,150,-36]) and the look-at 350 ahead ([115,150,314]), `fade` 0.03 then 0.00 (fading out), with Up held the pitch reaches 1.22 (70 degrees) and the look-at point rises to y 474, the second V returns `mode Follow` with `fade` rising 0.17 -> 1.00 and the camera back behind him at [144,251,-454].
- Area types: `KH2_CAM_TYPE=1 ... tt00`: `mode Overhead`, position = look-at + (600, 500 up, 0) -> [715,651,-35] over [115,151,-35], fov 1.05; V switches to the fixed view [51,1492,0] over [256,1777,0]. `KH2_CAM_TYPE=2 ... tt02`: row 2, radius 600, fov 1.05 (the collision squeezes the camera to 88 behind). `KH2_CAM_TYPE=3 ... tt04`: `mode Behind`, row 3.
- Attribute 5: the `tt02` camera collision has 12 triangles with flags word 0x5 and `tt04` has 2 (`cargo test -p kh2-app twilight -- --nocapture`); `tt00` has none.

## Controls in the app

WASD left stick (Ctrl half tilt), arrow keys right stick (left/right turn, up/down zoom), C or middle mouse button R3 (reset behind Sora), V Select (look-around; the second view of the overhead camera), right-drag and wheel turn and zoom directly, Page Up/Down zoom. Movement stays relative to the camera yaw. The arrow keys follow the stick rule above, so Left moves the camera to its left; the old orbit camera moved it the other way. Right-drag keeps its old sense (the view follows the mouse).

Harness: `KH2_YAW` and `KH2_DIST` (metres) still set the yaw and the orbit radius; the harness keys gained `A S D C Left Right Up Down`. `KH2_CAM_LOG=1` logs the camera, `KH2_CAM_RENDER=1` collides with the render meshes instead of the map's camera collision.

## Camera at Sora's head after the last kill (WP-CAM1) -- not reproduced

A reviewer once saw a close-up of Sora's hair, with no enemy text, after the last M_EX110 died (`KH2_PROGRAMS=0,2,0 KH2_WAVE_GROUP=b_01 KH2_WAVE_AT_GROUP=1 KH2_NO_PARTY=1`, J presses, `al03`). Runs on integration 246deca with `KH2_CAM_LOG=1`: with J held every second from 8 s (shots at 27, 32, 38 and 45 s, all looked at), and the same with Q at 7 s (locked on, the target dies, the lock clears), the camera stays in `mode Follow` for the whole run, 410..430 units from Sora horizontally (the orbit radius is 420, or 380 after a lock-on, which stays until a stick or R3 changes it). No `look-around` line, no dynamic-blocker clip. `tt00` and `tt04` with the default enemies and Q/J presses: also only `Follow`. After the three M_EX110 the forced program goes on to Shadows, so "no enemy text" is a gap between waves, not a camera state.
The only way to put the camera at Sora's eye (`eye_height` 150 above his feet, the look-around, mode 1) is Select: `V` or `KH2_KEYS` `t:V` (`Rig::tick`, test `look_around_starts_only_from_select`, which also toggles a lock-on every 13th tick and asserts the mode never becomes `Look`). A lock-on ending because its target died only returns the rig to `Follow`. A real `V` press reaching the window (another process typing into it) would give exactly the reported frame. Conclusion: no camera bug found, no code change; not proven to be harness-only either, since the reviewer's frame could not be reproduced.

## World map camera, second observation (WP-DYN2)
In three user savestates (the map during its load and two Gummi flights) the camera object keeps the same position, look-at, mode 0 and no follow target: the map and the flight do not write it (docs/dynamic-tracing.md section 5). The map's view source is still unknown.
