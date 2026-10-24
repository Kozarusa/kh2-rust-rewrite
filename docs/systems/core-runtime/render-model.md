# render.model: what the port must reproduce and what it replaces (WP-L1-S4)

Ledger system `render.model` (step L1). Every row of it carries one `L1-S4` note in `sheets/functions.json`; this file is the
contract those notes point to. Texture-container parsing and texture animation are documented in
[../foundations/render.md](../foundations/render.md) and ported in `kh2-model` (`texanim.rs`); this file adds the classification of the
whole system and what the rig's frame comparison (ROADMAP section 6 part 4) must check.

Numbers after L1-S4 (275 rows, 41.3 KB, including the rows L1-S4 moved here from `core`):

| part | what | rows | bytes | ledger decision |
|---|---|---|---|---|
| (a) plumbing | GS registers, VRAM placement, texture upload, VIF/DMA packet build | 29 | 3.7 KB | `not-needed:hw` |
| (b) rules ported through kh2-model / bevy | texture-container footer, TEXA and UVSC animation, animation object lists, model matrices | 67 + 9 ported/verified | 7.4 KB | port L1-P23 |
| (c) what is drawn | view and frustum, the map's draw tree and culling, sky and map pass order, draw layers, translucent sort, map props | 123 | 24.5 KB | port L1-P25a/b/c (L1-S5) |
| (d) real gaps | model lighting, projected shadows, fog, screen clear colour, frame freeze copy, monochrome filter | 23 (+7 in core.task/core) | 4.3 KB | port L1-P26a/b (L1-S5) |
| dead | lv4 starts and functions with no call, no literal and no pointer word in the executable or the overlays | 24 | 2.4 KB | `not-needed:dead` |

## (a) Plumbing the port replaces (not-needed:hw)
Evidence: each function only builds GS register values, GIF tags or VIF UNPACK codes (`0x6c00....` words) into a packet, places a
texture in GS memory or kicks DMA; none of them decides what the player sees beyond what the rules in (b) to (d) already decide.
- GS register cache with change test: `0x108c00`, `0x108ce0`, `0x108e30`; packet templates `0x1081f8`, `0x108350`, getters `0x108e70..0x108e88`.
- GS memory: allocator `0x109168`, `0x109278`, placement `0x109308`, bind `0x109188`, upload `0x109590`, `0x1095f0`, `0x2337f0`.
- Packet build: `0x10a9d0`, `0x1091f0`, `0x134e30`, `0x121558`, `0x133e90`, `0x147de8`, `0x1b60d0`.

- Frame unit (core.task): display set-up `0x14d378`, DMA `0x14d5a0`, `0x14d5c8`, `0x14d608`, environment packets `0x14d670`, `0x14d768`,
  `0x14d8a0..0x14d9d0`, `0x14da38`, `0x14da40`, `0x14dad8`, vblank thread and semaphores `0x14d03c..0x14d338`.

## (b) Rules ported through kh2-model (port L1-P23)
The port already implements these; the package adds `/// orig` lines and factory oracle tests (pure parsers and updates over a byte block):
- Footer and tag walk: `0x132238` (`kh2_model::texanim::footer_offset`), `0x132288`, `0x1322e0`, `0x132320`, `0x132360`, `0x1323b0`,
  `0x132438` (`texanim::parse`), `0x1323a0`.
- TEXA objects: constructor and destructors `0x1326e8`, `0x132798`, `0x1327c0`, vtable query wrapper `0x132910` over `0x1327f8` (`Texa::anim_index`), request-and-start wrapper `0x1329e0` over `0x132950`
  (`TexaPlayer::start`), update `0x132a80` (`TexaPlayer::update`), slot request `0x132c90`, `0x132ca0`, `0x192c80` (one call into `0x11e850`; the slot
  request `0x133058` is `TexaPlayer::play_slot`), sprite lists `0x1331c8`, builder `0x1332a8` (the current sprite `0x132fc0` is `TexaPlayer::sprite`), frame fields `0x132e98..0x132f90`, state copy `0x132ce0`.
- UVSC objects: `0x1333c8`, `0x133430`, `0x133458`, kind-2 request test `0x133490` (the offsets `0x133628`/`0x1336d8` are `UvscPlayer::offset`), update `0x133518` (`UvscPlayer::update`), `0x1335d8`, `0x1335f8`.
- Animation object base and list: `0x1324a0`, `0x132520..0x1325b0`, `0x11e018`, `0x11e108`, `0x11e1c8`, `0x11e208`, `0x11e3b8`, `0x11e598`,
  `0x11e638`, `0x11e760`, `0x11e7f8` (every object's update once per field), range dispatcher `0x188bd8` with `0x188010`.
- Model unit: matrix swap `0x11d1f8`, animation links `0x132580`, `0x1325c0`, `0x1337c0`, `0x133948`, globals `0x134728`, `0x134898`, static
  init `0x134738`, `0x1347b0`, `0x1347d0`.

## (c) What is drawn (port L1-P25a, L1-P25b, L1-P25c; contract of WP-L1-S5)
Every rule below is stated as inputs, outputs, state (addresses), order and arithmetic. Offsets on "the tree" are from the map draw
tree object (pointer `0x355930`, built by `0x145748` or its twin `0x145598`); offsets on "an object" are from a draw object (the
`+0xc0..+0x11f` block the tree links: actors, props, the sky). `[inferred]` marks what the code alone does not settle.
Corrections to L1-S4: the tree box is six floats, not shorts; `0x147158` is the player see-through test, not a distance fade; the sky
is drawn after the map's list call, not before it; `0x11dc40` and `0x148e08` draw projected shadows on the map, not model lighting
(that is `0x134bd8`); `0x1471d0` is the sort and `0x1487a8` the sort key (L1-S4 had them swapped with "walk" and "sort").

### c1. View and projection (L1-P25a)
- View matrix buffers: `0x106878(sel)` returns `0x3b23f0 + 0x100 * idx` when `sel == 0` and the byte `0x349e44 == 0`, else
  `0x3b21f0 + 0x100 * idx`; `idx` is the word `0x349e40` (`0x106868`). Each buffer is 0x100 bytes of 4x4 float matrices.
- Field of view `0x106e48(focal, proj)`: `proj+0x40 = focal`; `proj+0x44 = 2 atan((512 / proj+0x4c) * 0.5 / focal)`;
  `proj+0x48 = 2 atan((416 / proj+0x50) * 0.5 / focal)` (constants `0x378114` 0.5, `0x378118` 512.0, `0x37811c` 416.0; `atan` is
  the library routine `0x2ffd38`). So the vertical-to-horizontal ratio of `tan(fov/2)` is `(416 / s_y) / (512 / s_x)`, which is
  0.8125 when the two scales match [inferred: the scale words, read them at run time].
- Camera object (vtable `0x349e50`): position `+0x50`, target `+0x60`, up `+0x70`, roll `+0x80` (setters `0x107190`, `0x1071a0`,
  `0x1071b0`, `0x1071c0`; three-vector set `0x1070e8`, two-vector set `0x107130`, each copies 16 bytes only when source and field
  differ). `0x106930` returns `proj+0x80` (the cull matrix); `0x106918`, `0x106928`, `0x106f28`, `0x106f48` are empty or store the vtable.
- Port today: `kh2-game-core/src/camera.rs` (`tick`, line ~519) turns the game's horizontal fov into Bevy's vertical one with
  `2 atan(0.75 tan(fov/2))`. Delta: 0.75 against 0.8125 (if the scales match): the port's picture is about 8 % too short vertically,
  which reads as a slightly zoomed-in, cropped top and bottom.

### c2. Frustum and box tests (L1-P25a; pure)
- Plane from three points `0x163070(out, a, b, c)`: `n = normalize((b - a) x (c - a))`, `out.xyz = n`, `out.w = -(n . a)`
  (normalize is `n * (1 / sqrt(n . n))` with the VU0 `DIV`/`SQRT`).
- Five planes `0x1469c8(out, eye, corners)` (corners: four points at 0x10 stride): plane 0 = (c0, c1, c2); plane 1 = (eye, c2, c3);
  plane 2 = (eye, c0, c1); plane 3 = (eye, c3, c0); plane 4 = (eye, c1, c2).
- Box to eight points `0x144a50(box, out)` (box = min xyz at 0..2, max xyz at 3..5; L1-P22 owns it): w = 1.0 (`0x378fe4`); the eight points in order:
  (min x, min y, min z), (min x, max y, min z), (max x, max y, min z), (max x, min y,
  min z), (min x, min y, max z), (min x, max y, max z), (max x, max y, max z), (max x, min y, max z).
- Clip-code box test `0x1465c8(m, points)` (m = 4x4 clip matrix, eight points): for each point p, `q = m * p` and the VU0 `CLIP` of
  q.xyz against |q.w| gives six bits; `and` starts at 0x3f, `or` at 0; after each point `and &= bits`, `or |= bits`; as soon as
  `and == 0 && or != 0` return 2 (crosses); after eight points return 1 when `and == 0` (all inside) and 0 otherwise (all outside one
  plane). `0x146648(m, box)` builds the points with `0x144a50` and calls it.
- Box against five planes `0x146b20(planes, box)`: for each plane i (0..4), choose per axis `box.max` when the normal component's sign
  bit is clear and `box.min` when set; `d = n . p + plane.w`; if `d >= 0` return 0; after all five return 1 (the box is on the
  negative side of every plane: inside an occluder volume [inferred: the volumes face outward]).
- Points against five planes `0x146bc8(planes, points)`: return 0 when any of the eight points has `n . p + w >= 0` on any plane, else 1.
- `0x146598`: 64-byte matrix copy (skipped when source == destination).
- Port today: none (Bevy's own frustum culling on mesh AABBs; `kh2-party` has a stand-in box test). Delta: no visible difference by
  itself; it feeds c3.

### c3. The map draw tree and its cull (L1-P25a)
- Build `0x145748(tree, file, flag, mask)`: `+0x0` = file; from the file header: nodes at `file + [+0x14]` (`+0x4`), groups at
  `file + [+0x1c]` (`+0x8`), third table at `file + [+0x24]` (`+0xc`); counts `+0x10 = [+0x18] / 0x30`, `+0x14 = [+0x20] / 0x1c`,
  `+0x18 = [+0x28] / 0x34`; `+0x778 = flag` (the see-through switch of c5); `+0x779 = 0`; the root node's box is set to
  (-1e8, -1e8, -1e8)..(1e8, 1e8, 1e8) (`0x37900c`/`0x379010`; `0x145598` uses `0x379004`/`0x379008`, the same values); then the
  group mask (c4) and the object lists are reset (`0x146c40`). 16 translucent batches of 0x370 bytes at `+0xc60`, 512 object slots
  of 8 bytes at `+0x4360`. Release `0x1459d0` runs the batch destructors and the camera/projection destructors.
- Node (0x30 bytes): `+0x00` up to eight child indices (s16, -1 ends the list); `+0x10` box, six floats (min xyz, max xyz); `+0x28`
  first group index, `+0x2a` end group index (s16, half open); `+0x2c` head of the node's object list (link at object `+0x110`).
- Group (0x1c bytes): `+0x0` shown byte, `+0x1` group number, `+0x4` box (six floats).
- Per-frame matrices `0x145bc8(tree)`: copies the current projection (`0x106878(1)`, 0x120 bytes) to `+0x20` and the current camera
  (`0x1068c0(1)`, 0x94 bytes) to `+0x120`; the camera's view matrix is at `+0x130..+0x16f`; `+0x1b0 = view * screen` (projection
  `+0x00`), `+0x1f0 = view * cull` (projection `+0x80`, the "cull" matrix), `+0x230 = view * gs-clip` (the projection's
  view-clip-GS matrix). Products are row vectors times matrix (`VMULA/VMADDA` by columns of the left operand).
- Occluder volumes `0x143f78`: `+0x770 = 0`; when the occluder object `0x355940` exists and the byte `0x35594f` is set, `0x165990`
  fills up to N volumes of five planes (0x50 bytes) at `+0x270` and their count at `+0x770` from the camera position (`0x1071c8`).
  `0x145fa0(tree, box)` returns 1 when the box is inside any volume (`0x146b20`).
- Walk `0x1463b0(tree, node, count)`, from node 0 into a u16 list (the caller's 2048-entry stack array, pointer set by `0x145f88` at
  `+0x1c`; the caller writes 0xffff after the last entry):
  1. `c = 0x146648(+0x1f0, node.box)`. `c == 0`: return (node and children skipped). `c == 1`: the inside walk below, return.
  2. `c == 2`: if `0x145fa0(node.box)` return. For each group g in `[first, end)` with shown byte set: `k = 0x146648(+0x1f0, g.box)`;
     `k == 1` append g; `k == 2` run `0x146040`: `k2 = 0x146648(+0x230, g.box)`; `k2 == 1` append g, `k2 == 2` append `g | 0x8000`
     (needs clipping). Then the node's objects by `0x146168` (below).
  3. Recurse into each child (at most eight).
- Inside walk `0x1462a8`: if the node is in an occluder volume return; append every shown group of the node; mark every object of the
  node visible (`0x146108`); recurse into the children (at most eight).
- Objects of a node `0x146168`: for each object o (flags `o+0xf8`): if bit 0x1000 is set: bit 0x20 set → visible with clip mark
  (`0x147128`), else visible (`0x1470f0`). Else if bit 0x800 is clear → visible. Else test its eight points (`o+0xfc` when not null,
  else the points of its box `o+0xd0` by `0x144a50`) with `0x1465c8(+0x1f0)`: 1 → visible; 0 → not; 2 → when bit 0x20 is set test
  again with `+0x230` (1 visible, 2 visible with clip mark, 0 not), when clear → visible.
- Visible mark `0x148920`: slot word `tree + 0x4364 + 8 * o+0x11c` gets bit 0; `o+0xf8` gets bit 2. `0x1470f0` also clears bit 4 of
  `o+0x8`, `0x147128` sets it (the clip mark). Per frame `0x146cf8` clears bit 2 of every registered object and bit 0 of its slot.
- Object registration: `0x146da0` (wrapper `0x143240`) adds an object whose flags have bit 4 or bit 8: slot word gets bit 1, `0x146d40`
  stores slot index `o+0x11c = +0x5360`, the object in slot `+0x4360 + 8 * index`, `+0x5360 += 1`, and pushes o on the list of node
  `o+0x118`. Move `0x146e40(tree, o, top)` (wrapper `0x1432b0`): when bit 0x1000 is clear and `0x146df0` returns 0 (the object has no
  alpha byte `o+0xeb`, or it is negative, or bit 0x10 is clear: sets bit 0x800 "cull me"; else clears it and returns 1), the node is
  searched again (`0x147058`: from the current node and depth `o+0x118`/`o+0x11a` when its box still holds the object's box, else from
  the root; `0x146f68` descends while `0x144ca8` says the node box contains the object box and keeps the deepest hit); otherwise the
  node and depth become 0. Then the object is re-registered and its children (`o+0xf0`, siblings `+0xec`) are moved with `top = 0`
  (children clear slot bit 1). `0x143260` is the search alone. The per-frame list clear `0x143300` (task, `0x146c80`) copies each
  object's matrix `+0x40` to `+0x80` and empties the node lists before the actors move.
- Pass `0x144168` (map draw task): nothing when the freeze byte `0x35594e == 1`. Else, in order: `0x145bc8`; `0x146cf8`; `0x143f78`;
  `0x143ff8` (the player box of c5); the walk from node 0; when the byte `0x35594d == 0` the map model (`0x355948`) is drawn with the
  group list through `0x11cb38` and the matrix `0x106878(1) * camera`; then the object pass `0x147300` (c5).
- Port today: `kh2-game-core/src/assets.rs` `load_area` spawns every map mesh once and leaves culling to Bevy (per-mesh AABB against
  the view frustum, `far` 1000 m). No groups, no occluders, no clip mark. Visible difference: map parts hidden in the original by a
  group switch or an occluder are drawn by the port (see c4); pop-in at the screen edge differs only by box size.

### c4. Map group show and hide (L1-P25a)
- `0x145aa8(tree, mask)` (also at build): clear every group's shown byte, then for each bit b (0..63) of the 64-bit mask set the shown
  byte of every group whose number is b (`0x145b80`). `0x145b40(tree, b)` clears the shown byte of every group numbered b.
- The collision switches call these: `0x142f78(b)` hides drawn group b (`0x145b40`) and disables collision group b in the five
  collision objects (`0x355944`, `0x355934`, `0x355940`, `0x35593c`, `0x355938` by `0x141800`); `0x143028(b)` shows it (`0x145b80`,
  `0x141840`). So one script opcode switches both.
- Port today: `kh2-game-core/src/world.rs` `World::set_group`/`apply_area_word` gate collision only; the drawn meshes stay. Delta:
  walls and props of a hidden group are visible (eh10 arena walls of groups 3 to 8; any area whose script hides a group). Needs the
  map's group table from the map file (the tree file block of `0x145748`) in `kh2-map`.

### c5. Object pass, sky, layers, sort and fades (L1-P25b)
`0x147300(tree)`, once per frame after the walk; `v = 0x106868()` is stored in `+0x536c` (the view index), and the per-view counters
`+0xc50 + 4v` (translucent batches) and `+0x77c + 4v` (see-through list) are zeroed. Order:
1. Sky: for each of the sky objects `+0x5370` (SK0) and `+0x5374` (SK1) (set by `0x146d88`, wrapper `0x1432d8`), `0x1484b0` draws it
   when its alpha byte `+0xeb != 0` and its bit 4 is set: state copy `0x147b90` (`+0xc..+0xf = +0xe8..+0xeb`), view state byte
   `o+0x10e+v = 0`, the fixed register words `o+0x20 = 0x31000070`, `o+0x18 = 0x800d8` [inferred: its own depth and alpha state], all
   mesh groups (list 0x8000, 0xffff), matrix `+0x1b0`. `0x148440` sets bit 4 on both skies, `0x148478` clears it; `0x143100` and
   `0x15f1b0` call the first, `0x143120` and `0x15f1c8` the second; `0x143140` sets `0x35594d = 1` (map off) and hides the sky,
   `0x143160` sets it to 0 and shows the sky. So the sky's call comes after the map's list call of c3 [inferred: the map's packets are
   queued, not kicked, so the screen order is the packet order: map, then sky, which only fills pixels the map left; confirm on a frame].
2. Draw layers reset `0x148948`: `+0x537c = 0`, 256 bucket words at `0x3b4070` zeroed.
3. For each slot i < `+0x5360`: `0x147728` (an object with bit 0x1000 and no `+0x120` gets its rotation from the camera's transposed
   rotation: a billboard [inferred]), key `z_i = 0x1487a8(tree, o, camera position)`: `z = (view * o+0xc0).z` (row 2 of `+0x130`);
   when bit 0x80 is set `0x148550(|camera - o+0xc0|)` (model level of detail [inferred]); when bit 0x100 is set the three-part fade
   `0x1485a8(d)` with d the same distance: for part j (1..3) with near/far pairs (`+0x60`,`+0x64`), (`+0x58`,`+0x5c`), (`+0x50`,`+0x54`)
   of the record at `o+0x114`: `d > far` → alpha byte 0 and part hidden (`0x11c5b0`); else part shown (`0x11c580`) and alpha
   `0x80 - (int)((d - near) / (far - near) * 128)` when `d >= near`, else 0x80 (`0x379030` = 128.0). Keys go to scratchpad
   `0x70000800 + 4i`; objects with bit 4 and without bit 0x40 also go to the sort arrays (`0x70000000` keys, `0x70001000` slot pointers).
4. Sort `0x1471d0`: quicksort (Hoare, middle pivot, recursive on the left part, loop on the right) ascending by key, swapping keys and
   slot pointers together; only when at least two objects.
5. Distance fade (same loop, every slot with bit 1 "registered"): for an object without bits 0x2000 and 0x40 and with fade distance
   `f = o+0x108 != 0`: `z > f` (key) → skipped this frame (not drawn, no shadow); `f - 400 < z <= f` → alpha byte `o+0x10c =
   (int)(o+0x10c * (1 - (z - (f - 400)) / 400))` (`0x379014` 1.0, `0x379018` 400.0; written back, so it compounds over frames
   [inferred: a writer resets it each frame; check]). Then the shadow pass of d2 for the object and its children (`0x148d58`, `0x148e08`).
6. Draw in sort order from the last (largest key) to the first, each object whose slot has bit 1, through `0x1479c0`. When `+0x779` is
   set (`0x1431d0`) the objects are instead drawn in groups of equal word `o+0x2c`, the group of the highest remaining index first
   (`0x147980` finds it), each group from high index to low.
7. `0x1479c0(z, tree, o)`: when `o+0x38` and `o+0x100` are set, the model light matrix `0x134bd8(z, o+0x38, o+0x40)` (d1). Then
   `0x147158(z, tree, o)` = 1 when no scene event runs (`0x224b18` is 0), `+0x778` is set, bit 0 of `o+0xf8` is set and the player's
   view depth `+0x774 <= z` (the object is behind the player): such objects go to the see-through list (`0x147bb8`: entries
   `+0x798`, count `+0xc40`, view count `+0x77c+4v`; state byte 1) and are drawn by the see-through effect `+0x790` at the end.
   Otherwise: state 1 from last frame → `0x147bb8` again; then, when `0 < o+0xeb` (alpha byte), batches `< 16` and bit 0x10 is set
   (and bit 0x4000 was not just cleared), `0x147c98` puts the object into batch `+0xc50+4v` (`+0xc60 + 0x370 * n`, entries
   `+0xc68`, count `+0xfc0`, alpha copy `+0xf = 0x80`, state 2) and counts the batch; else when state is 2 and batches `< 16` the same;
   then `0x148388` draws the opaque object now: layer byte `o+0x11f == 0` → `0x147e20` at once, else into bucket `0x148980`: the
   first free bucket at or after the layer number. Children (`+0xf0`, siblings `+0xec`) follow each rule recursively.
8. Layers `0x1489d8`: buckets 0..255 in order, each through `0x147e20` (model draw with its own view state; `0x147d98` clamps its
   rectangle).
9. Translucent batches: each batch of this view with a non-empty list is drawn by its effect (`0x1da840`) in batch order; then the
   see-through list when `+0x778` is set and its view count is not zero.
- Player box `0x143ff8` (when the byte `0x35594c` is set): `+0x774 = (camera * player position).z`; screen points of the player
  position p and p - (0, 200, 0) through `+0x1b0` (`0x2fbc10`, 12.4 fixed point); `h = (int)(((y0 - y1) >> 4) * 1.6)` into `+0xc4c`;
  `+0xc44 = (x0 >> 4) - h / 2 - 0x700`, `+0xc48` = a coordinate of the second point `>> 4` minus about `h / 4` and 0x730 [inferred: the decompile mixes the two
  points' stack slots; read the instructions] (`0x378fcc` 200.0, `0x378fd0` 1.6): the screen rectangle of the see-through effect.
- Mesh and model draw (packets hw; wrappers `0x1202d8`, `0x120308`): `0x11f560` draws a mesh when `(flag & 0xf) != 0`, splits passes on `+0x28 < 0x10000`, skips a mesh
  with `+0x2c` bit 0 and a lighting record; `0x121770` draws twice, the second with alpha `+0x28 >> 1`.
- Port today: `assets.rs` `material_for`: every mesh unlit, `AlphaMode::Blend` when the mesh is translucent else `Mask(0.25)`; the sky
  is two entities at the origin (`sky_follow`) with no culling (`sky_unculled`); Bevy sorts its blended meshes back to front per mesh
  by view depth, opaque first. Deltas and their look: (a) the sky is depth-tested with the map and can cover far map parts the
  original keeps (G4 green plane of render.md); (b) no layer order: decals and water drawn in layer order in the original z-fight or
  swap in the port; (c) no distance fade (`o+0x108`): NPCs and props cut off or never fade; (d) no see-through: Sora is hidden behind
  walls where the original shows him; (e) the three-part fade (bit 0x100) is missing: distant parts of props stay opaque.

### c6. Map props (L1-P25c)
- Create `0x15f3a0` at area load per prop record; set-up `0x15f490`/`0x15f8e0` (model, texture players, box from the model `0x160068`, colour); constructor
  `0x15fdc0` and destructor `0x15fe50`; a task per prop (`0x160588` creates it on `0x520a30` with body `0x1604a8`; the body runs the
  prop's two vtable calls when the flag word `0x35d268` bit 0x20 is clear and the byte `0x35a840` is 0, then links it at the tail of
  the list `0x520a40`/`0x520a44` by `+0x84`).
- Per frame `0x160428`: a prop whose `+0x180` bit 1 is clear (static) runs `0x160388` (centre) when `+0x2b8` bit 0 and is added to the
  tree (`0x143240`); a moving prop runs `0x1601a8` (world box), the centre, then is moved in the tree (`0x1432b0`).
- Colour `0x15ff50(p, r, g, b)`: `p+0x2a8 = (r * p+0x1a4) >> 7`, `p+0x2a9 = (g * p+0x1a5) >> 7`, `p+0x2aa = (b * p+0x1a6) >> 7`
  (bytes, 0x80 = 1.0).
- Motion: `0x160fb0` binds the motion holder (`+0x44` owner, clears `+0x40..+0x50`, empty box `+0x54`); `0x161010` looks up motion k
  (`0x1684c0(+0x40, 9, k)`); `0x1610b0` starts it (`+0x48` motion, `+0x4c` time, `+0x50` length from the motion's vtable `+0x8`),
  then the box `0x161238` from the motion's vtable `+0x14`; step `0x161130(dt)`: `t += dt`, matrix `0x1611b8(t)`, and when `t > len`:
  `t -= len` and the matrix at `len` (so a loop shows the last pose once); `0x161060` stops (`0x11c2a0` with the time, or the base
  pose `0x11c268`).
- Camera-following texture players `0x15fef8` (SK objects): stop or base pose `0x161060`, step `0x161130` by the global step
  `0x356f40`, then `0x11e7f8` (texture animation, L1-P23).
- Port today: `kh2-game-core` draws the map model and the two skies only; props of the map file are not spawned (the texture players
  of the sky run, `texanim.rs`). Visible: moving background objects (flags, wheels, the tt fountain parts) are missing.

## (d) Not reproduced yet (port L1-P26a, L1-P26b)
### d1. Model lighting (L1-P26a)
- Light matrices `0x134960(out, m, l0, l1, l2)` and `0x134bd8(out, lights, m)`: out rows 0..2 start at zero; the inverse of the 3x3
  of m is built from the cross products of its rows (`r1 x r2`, `r2 x r0`, `r0 x r1`) divided by the determinant `r0 . (r1 x r2)`, the
  translation by `-(row3 * inverse)`; each light direction (w = `0x378cc4`) is taken into model space by that inverse, negated in y and
  z [inferred: read the third component], normalized, and stored as a row; colours follow at `out+0x30..`. `0x11bb28` (3x3
  determinant), `0x11bb78` (cofactor) and `0x11bc08` (inverse) are the scalar forms used by d2.
- Light blocks: `0x134f40` init (direction (0, -1e7, 0) w 1.0 [inferred: a far point above], colour 1.0), copies `0x134f90`,
  `0x134fa8`, colour set/get `0x134fc0`/`0x134fe0` (`+0x10`), the global light block `0x354e30` (`0x135870`) with its direction
  `0x354e50` (`0x135880` set, `0x1358a0` get).
- Port today: every material `unlit` (`assets.rs`), vertex colour times texture. Delta: Sora, party and enemies are flat; in the
  original their shading follows the area's three directional lights.

### d2. Projected shadows on the map (L1-P26a)
- `0x148e08(tree, objs, n)` for an object with flag 8 or 0x40 (collected with its children by `0x148d58`) and alpha `o+0x10c != 0`:
  box = union of the boxes (`+0xd0`) of the n objects (`0x144778`); `ymax += 2 * (ymax - ymin) * 0.5 + o+0x104` (`0x379034` 0.5);
  if the box is visible (`0x146648(+0x1f0)`) and not in an occluder volume: flag 0x40 takes the object's own colour and alpha
  (`+0xe8..+0xeb`) and bit 0 of the mode; shadow kind 1 when `o+0xf8` bit 0x200, 2 when bit 0x400, else 0; the groups whose boxes meet
  the box (`0x148b68`, into the list at `+0x5364`) are drawn again with the shadow through `0x11dc40(map, mode, groups, objs, kind)`
  (colour 0x80, 0x80, 0x80, alpha 0xff). `0x148a58` does the same for all 32 groups with the block `0x484380`.
- `0x11dc40`: eight box corners (`0x11dac0`: union over the objects, w 1.0); mode bit 0 clear: projection from the global light
  `0x354e30` (`0x135028`); bit 0 set: a light at the box centre `(min + max) * 0.5` lifted by `|max - min| * 8` (`0x378968`,
  `0x37896c`); the projection times the map model's matrix `+0x80..+0xb0` (and the model matrix inverse `0x11bc08` when the model is
  not of kind 2); the record is registered in the first free 0x120-byte slot of `0x349e70` (count `0x349e68`) by `0x10a9f8` (object
  list, group list ending 0xffff, mode words, matrices).
- Port today: none (the app has no shadows). Delta: no dark blob under Sora, enemies and props on the floor.

### d3. Fog (L1-P26b)
- `0x10a5e0(r, g, b)` stores the colour bytes `0x34e0d0..0x34e0d2`; `0x10a630(near, far)`: `0x34e0b4 = near`, `0x34e0b8 = far`,
  `0x34e0bc = -255 / (far - near)`, `0x34e0c0 = 255 * far / (far - near)` (`0x378424` 255.0, `0x378428` -255.0), so the GS fog value
  is `F(z) = 0x34e0c0 + 0x34e0bc * z = 255 (far - z) / (far - near)` (255 none, 0 full colour); `0x10a6b0(min, max)` stores the clamp
  `0x34e0c8`/`0x34e0cc`; `0x10a6e8` reads the four words for the packet.
- Defaults (`0x108f68`): colour 0x80 each, near 5000, far 1e6, limits 0..255. Area fog (`0x15c018`): colour bytes `0x520644..`,
  near `0x520648` (default 1000), far `0x52064c` (default 1e6), min `0x520650` 0, max `0x520654` 255; when `0x15c798` finds the
  defaults (min 0, max 255, colour 0x808080) fog is off (`0x34e0b0 = 0`), else applied.
- Pass words: `0x34e0d4` is the current pass (1..11) of the frame's draw lists (`0x10a420`); objects without bit 2 of `+0x18` take the
  fog colour (`0x109df0`). `0x10a9b0` reads the pass word, `0x10ac98` returns the packet block `0x34e0d8`.
- Port today: no fog. Delta: far parts of foggy areas are sharp and bright in the port.

### d4. Screen clear (L1-P26b)
- Clear block of the frame unit `0x358000` (`0x14e370` returns it [inferred: `0x358000 + 0x2f0`, the block `0x14e348` passes to
  `0x14dad8`]): colour bytes `+0x1d6..+0x1d8` (`0x14da88` set, `0x14daa8` get, `0x14de68` set
  through the unit), switch byte `+0x1d9` (`0x14dac8`); `0x14dad8` sends the base packet `+0x10` and, when the switch is 1, the clear
  sprite `+0x90` with RGBAQ `r | g << 8 | b << 16 | 0x80 << 24 | 1 << 32`. `0x14d8f0` resets switch and colour to 0. The clear task
  `0x14dc70` runs `0x14e348(0x358000)` (= `0x14dad8(0x358000 + 0x2f0)`) and loads the background image (`0x14ddc0(0xd8)`) when `0x3584e0 == 1`.
- Port today: `camera.rs` inserts `ClearColor(0.05, 0.05, 0.08)` once. Delta: areas whose sky leaves gaps show the port's blue-black
  instead of the area's colour (black when the switch is off).

### d5. Frame freeze (L1-P26b)
- Area fade: `dk::storeImage` `0x14de00` → `0x14dbe8(unit, 0x484480)` copies the displayed frame (base `unit+0` in 2 KB pages, width
  `unit+4`, height `unit+6`) from GS memory to RAM; the map pass stops while `0x35594e == 1` (c3). The event freeze `0x14db58` sends a
  stored frame back in two halves (rows 0..h/2 and h/2..h at `+0x4e000`) [inferred: interlaced fields].
- Port today: `kh2-event/src/area_jump.rs` `draw_fader` darkens a full-screen node over the live scene. Delta: during the fade-out the
  port keeps animating the world (Sora and enemies move under the fade); the original holds the last frame.

### d6. Monochrome (L1-P26b)
- `0x355158` = 1 when the area is monochrome: set on area entry by `0x13aaa8` from `0x1934a0(world, area)` (with `0x1da908` for the
  effects); `0x13ab98` tests it; `0x13abb0` sets it and returns the old value; `0x13aa68` converts the loaded colour data (`0x131fd8`: walks a resource tree once, bit 0 of `+0x2` marks a
  node done, each colour word of a leaf goes through `0x131788` [inferred: to grey]) when
  `0x35515c` and the state are set.
- Port today: none. Delta: the monochrome world (Timeless River) is drawn in colour [inferred: the only world `0x1934a0` accepts].

## Oracle and frame-check plan
- Pure, factory in kh2-ee (VU0 macro mode and CLIP flags run in kh2-ee): `0x163070`, `0x1469c8`, `0x146b20`, `0x146bc8`, `0x1465c8`,
  `0x146648` (with `0x144a50`), `0x146598`, `0x106e48` (stub nothing; `0x2ffd38` runs), `0x106878`, `0x1070e8..0x1071c0`, `0x145aa8`,
  `0x145b40`, `0x145b80` (group array in RAM), `0x146df0`, `0x147158` (stub `0x224b18` to return 0 and 1), `0x1471d0` (two arrays),
  `0x1485a8` (stub `0x11c580`/`0x11c5b0` with `jr ra` and compare the alpha bytes), `0x148920`, `0x148948`, `0x148980`, `0x147980`,
  `0x15ff50`, `0x161130` (stub `0x1611b8`), `0x10a5e0`, `0x10a630`, `0x10a6b0`, `0x14da88`, `0x14daa8`, `0x14dac8`, `0x13abb0`,
  `0x134960`/`0x134bd8` (matrix and three lights), `0x11bb28`, `0x11bb78`, `0x11bc08`, `0x11dac0`, `0x143ff8` (camera block and
  stubbed player getter). Vectors: boxes fully inside, crossing each plane, outside each plane, degenerate (min == max), points on a
  plane (d = 0 exactly: returns 0 in `0x146b20`), w = 0 for the clip test; fov focal 0.5..4 and scales 0.5..2.
- Built object in memory (as L1-P10 did with `0x141500`): the tree. Lay a node table (root with eight children, two levels), a group
  table and three objects at free addresses, a projection and camera block, and compare the output list of `0x1463b0` (entries and
  0x8000 marks), the slot words after `0x146168`, and the node of `0x146f68`/`0x147058`/`0x146e40` with the port's `draw_tree`.
  Also `0x147300` with `0x147728`, `0x1479c0`, `0x1484b0`, `0x147e20`, `0x11cb38` and the effect creator stubbed by a recorder
  (`jr ra` after a store of a0 into a log array): the order of draws is the oracle.
- Frame comparison only (rig part 4, frames at checkpoints, 640x448 in the port, native in PCSX2; masks drop the HUD, the menu and the
  minimap rectangles; tolerance: mean absolute RGB difference over the unmasked pixels and the share of pixels over 24/255):
  1. `render-sky` (tt05 horizon, camera still): checkpoint after 120 vsyncs; mask: HUD; the sky region (rows above the map's
     horizon) mean difference <= 12, no 64x64 block with mean > 40. Functions `0x1484b0`, `0x148440`, `0x148478`, `0x143140`.
  2. `render-groups` (an area whose script hides a group, before and after the opcode): checkpoints at the vsync before and 4 after;
     the hidden group's screen box must change in both; functions `0x145aa8`, `0x145b40`, `0x145b80`, `0x142f78`.
  3. `render-cull-turn` (tt04, a 90-degree camera turn over 60 vsyncs past NPCs): every 10 vsyncs; per checkpoint the set of drawn
     objects (from the port's log and the original's slot words `+0x4364` read through PINE) must be equal; frames informational.
  4. `render-translucent` (a fountain or window area): 3 checkpoints; mean <= 12 over the translucent screen box.
  5. `render-props` (an area with moving props): checkpoints 30 vsyncs apart; prop motion times `+0x4c` read through PINE within one
     frame step; frames mean <= 16.
  6. `render-light` (Sora and one enemy, camera still): mask everything but the two models' screen boxes; mean <= 16.
  7. `render-fog` (a fog area at two camera distances): mean <= 12 over the far half of the picture.
  8. `area-change` (in L1-close): the frames from the fade start to black must equal the first one under the fade (the frozen frame),
     per-pixel mean <= 4 after scaling by the fade factor.
  9. `render-clear` (an area whose sky leaves gaps): the gap pixels' colour within 8 per channel of `+0x1d6..+0x1d8`.
- Checks 1 to 5 belong to L1-P25a/b/c, 6 and the shadow part of 6 to L1-P26a, 7 to 9 to L1-P26b; each needs the rig's frame part
  (package L1-RIGF) before it can pass.

## What the rig's frame comparison must check
The list above replaces the nine checks of L1-S4 one for one (same numbers; check 8 is the `area-change` scenario of L1-close).

## Decompile index
Read for this classification (main image): `0x106868..0x1071c0`, `0x108c00..0x10ace0`, `0x11bb28..0x121770`, `0x132238..0x1358a0`,
`0x143100..0x149090`, `0x14d03c..0x14dc70`, `0x15f1b0..0x161238`, `0x172f00`, `0x172f20`, `0x188010`, `0x188bd8`, `0x192c80`, `0x1da840`,
`0x228e78`, `0x23b0f0`, `0x2dfd58..0x2ecd50`, `0x2f55c0`, `0x2f5858`. Callers and pointer words from the private workspace and a scan of
the executable and `OVL.IMG` for 32-bit words equal to each address.

## Gaps
- `0x11f560` and `0x121770` hold draw rules besides the packet build (reviewer of L1-S4); L1-P25 ports the rules, the packets stay out.
- `0x147980` is named by 16 words of the executable: it may be a table value rather than only a function [inferred]; read before porting.
  L1-S5 read it as the group finder of the `+0x779` order (c5 step 6).
- `0x10a9f8` registers the shadow records of d2 (L1-S5; L1-S4 called them light records): part of L1-P26a.
- L1-S5 open points: the sky's screen order against the map (c5 step 1), the compounding of the distance fade alpha (c5 step 5), the
  occluder volume builder `0x165990` and the see-through effect `0x1da840` (outside render.model; read by L1-P25b), the
  projection scales `+0x4c`/`+0x50` (c1), the second point of `0x143ff8`, the monochrome world list of `0x1934a0`.
- Shift of rows: `0x148a58`, `0x148b68`, `0x148d58`, `0x148e08` (L1-S4: L1-P25) are the shadow pass and go to L1-P26a.
