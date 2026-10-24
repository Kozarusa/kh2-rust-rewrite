# render.model: what the port must reproduce and what it replaces (WP-L1-S4)

Ledger system `render.model` (step L1). Every row of it carries one `L1-S4` note in `sheets/functions.json`; this file is the
contract those notes point to. Texture-container parsing and texture animation are documented in
[../foundations/render.md](../foundations/render.md) and ported in `kh2-model` (`texanim.rs`); this file adds the classification of the
whole system and what the rig's frame comparison (ROADMAP section 6 part 4) must check.

Numbers after L1-S4 (275 rows, 41.3 KB, including the rows L1-S4 moved here from `core`):

| part | what | rows | bytes | ledger decision |
|---|---|---|---|---|
| (a) plumbing | GS registers, VRAM placement, texture upload, VIF/DMA packet build | 34 | 8.6 KB | `not-needed:hw` |
| (b) rules ported through kh2-model / bevy | texture-container footer, TEXA and UVSC animation, animation object lists, model matrices | 67 + 9 ported/verified | 7.4 KB | port L1-P23 |
| (c) what is drawn | view and frustum, the map's draw tree and culling, sky and map pass order, draw layers, translucent sort, map props | 119 | 20.2 KB | port L1-P25 |
| (d) real gaps | model and map lighting, fog, screen clear colour, frame freeze copy, monochrome filter | 22 (+7 in core.task/core) | 3.6 KB | port L1-P26 |
| dead | lv4 starts and functions with no call, no literal and no pointer word in the executable or the overlays | 24 | 2.4 KB | `not-needed:dead` |

## (a) Plumbing the port replaces (not-needed:hw)
Evidence: each function only builds GS register values, GIF tags or VIF UNPACK codes (`0x6c00....` words) into a packet, places a
texture in GS memory or kicks DMA; none of them decides what the player sees beyond what the rules in (b) to (d) already decide.
- GS register cache with change test: `0x108c00`, `0x108ce0`, `0x108e30`; packet templates `0x1081f8`, `0x108350`, getters `0x108e70..0x108e88`.
- GS memory: allocator `0x109168`, `0x109278`, placement `0x109308`, bind `0x109188`, upload `0x109590`, `0x1095f0`, `0x2337f0`.
- Packet build: `0x10a9d0`, `0x10a9f8` (lighting packet), `0x1091f0`, `0x134e30`, `0x121558`, `0x121770`, `0x133e90`, `0x147de8`, `0x1b60d0`.
- Mesh DMA chain `0x11f560` with its pointer-word wrappers `0x1202d8`, `0x120308`: the mesh rules it reads (texture index bits 1..5 of the
  mesh entry `+0xc`, material flags) are already ported in `kh2-model` (`lib.rs`, `texture.rs`); the chain itself is not.
- Frame unit (core.task): display set-up `0x14d378`, DMA `0x14d5a0`, `0x14d5c8`, `0x14d608`, environment packets `0x14d670`, `0x14d768`,
  `0x14d8a0..0x14d9d0`, `0x14da38`, `0x14da40`, `0x14dad8`, vblank thread and semaphores `0x14d03c..0x14d338`.

## (b) Rules ported through kh2-model (port L1-P23)
The port already implements these; the package adds `/// orig` lines and factory oracle tests (pure parsers and updates over a byte block):
- Footer and tag walk: `0x132238` (`kh2_model::texanim::footer_offset`), `0x132288`, `0x1322e0`, `0x132320`, `0x132360`, `0x1323b0`,
  `0x132438` (`texanim::parse`), `0x1323a0`.
- TEXA objects: constructor and destructors `0x1326e8`, `0x132798`, `0x1327c0`, request test `0x132910` (`Texa::anim_index`), start `0x1329e0`
  (`TexaPlayer::start`), update `0x132a80` (`TexaPlayer::update`), slot request `0x132c90`, `0x132ca0`, `0x192c80` (`TexaPlayer::play_slot`),
  sprite lists `0x1331c8`, `0x1332a8` (`TexaPlayer::sprite`), frame fields `0x132e98..0x132f90`, state copy `0x132ce0`.
- UVSC objects: `0x1333c8`, `0x133430`, `0x133458`, offset `0x133490` (`UvscPlayer::offset`), update `0x133518` (`UvscPlayer::update`), `0x1335d8`, `0x1335f8`.
- Animation object base and list: `0x1324a0`, `0x132520..0x1325b0`, `0x11e018`, `0x11e108`, `0x11e1c8`, `0x11e208`, `0x11e3b8`, `0x11e598`,
  `0x11e638`, `0x11e760`, `0x11e7f8` (every object's update once per field), range dispatcher `0x188bd8` with `0x188010`.
- Model unit: matrix swap `0x11d1f8`, animation links `0x132580`, `0x1325c0`, `0x1337c0`, `0x133948`, globals `0x134728`, `0x134898`, static
  init `0x134738`, `0x1347b0`, `0x1347d0`.

## (c) What is drawn (port L1-P25)
These change which polygons reach the screen and in what order. The port draws through bevy, so each rule becomes a bevy system or a
`kh2-map`/`kh2-model` query; the oracle is the factory where a function is pure (box and plane tests), the rig's frames elsewhere.
- View: current view matrix buffer `0x106878`, index `0x106868`, frustum `0x106930`, field of view `0x106e48`, camera object `0x106f28..0x1071c0`.
- Frustum tests: planes from a matrix `0x1469c8`, copy `0x146598`, box against six planes `0x1465c8`, box visible `0x146648` (the party code
  uses a stand-in), sphere `0x146b20`, outside `0x146bc8`.
- The map's draw tree (nodes of 0x30 bytes with a short box at `+0x10`; object lists at `+0x5360`): build `0x145748`, release `0x1459d0`, per-frame
  cull `0x145bc8` over the current view, recursive walk `0x1462a8`, `0x1463b0`, insert `0x146040`, objects add/move `0x143240`, `0x1432b0`,
  `0x146da0`, `0x146e40`, node search `0x143260`, `0x146f68`, `0x147058`, visible marks `0x1470f0`, `0x147128`, `0x148920`, list reset `0x143300`,
  `0x146c40`, `0x146c80`.
- Map groups: show/hide by mask `0x145aa8`, `0x145b40`, `0x145b80` (the collision group switch `0x142f78`/`0x143028` calls these too, so an
  area script that disables a collision group also hides its drawn group), group box test `0x148b68`.
- Pass order: `0x147300` draws the sky first (`0x148440`/`0x148478` mark it, on/off `0x143140`/`0x143160`, `0x15f1b0`/`0x15f1c8`), then map
  groups, then objects by draw flags (`0x1479c0`: opaque `0x147bb8`, translucent `0x147c98`, second view `0x147728`), then the draw layers
  (buckets at `0x3b4070`: `0x148948`, `0x148980`, `0x1489d8`) and the translucent list sorted by distance (`0x1485a8`, `0x1487a8`).
- Distance fade: `0x147158` (skipped while a scene event runs).
- Per-frame passes: `0x143f78`, `0x143ff8`, `0x144168` (not while the freeze flag `0x35594e` is set).
- Map props (background objects of the map file): create `0x15f3a0`, set-up `0x15f490`, `0x15f8e0`, constructor/destructor `0x15fdc0`,
  `0x15fe50`, per-frame `0x160428`, task `0x160588`/`0x1604a8`, colour scale `0x15ff50`, box `0x160068`, `0x1601a8`, `0x160388`, motion
  `0x160fb0..0x161238`, texture players that follow the camera `0x15fef8` (the sky).

## (d) Real gaps (port L1-P26)
Nothing in the port reproduces these today; each is visible:
- Model lighting: `0x11dc40` builds the light set of a model from its object's light record (`0x11dac0`, record at object `+0xd0`), with the
  inverse matrix `0x11bc08` (`0x11bb78`, `0x11bb28`); map group lights `0x148a58`, `0x148e08`; light matrices `0x134960`, `0x134bd8`; light
  blocks `0x134f40..0x135880`, `0x1358a0` (global light at `0x354e30`/`0x354e50`). The app lights models with bevy's own lights.
- Fog: colour `0x10a5e0` (`0x34e0d0..0x34e0d2`), near/far and scale `0x10a630`, limits `0x10a6b0`, state `0x10a9b0`, `0x10ac98`.
- Screen clear colour and switch: `0x14da88`, `0x14daa8`, `0x14dac8`, `0x14de68`, the clear task `0x14dc70` (priority 100000) with its
  optional background image.
- Frame freeze: the area fade copies the last frame (`0x14dbe8`) and the event freeze does the same (`0x14db58`); the port fades over the
  live scene (`kh2-event` `area_jump.rs`).
- Monochrome filter state `0x13ab98`.

## What the rig's frame comparison must check
For L1's gate G3 the frame part of the rig (section 6 part 4) needs, per scenario, masks and checkpoints that make these visible:
1. Sky behind everything: a frame looking at the horizon in Twilight Town (tt04/tt05 sky, the G4 green-plane case of render.md).
2. Map group switch: an area whose script disables a group (collision and drawing both off) before and after the switch.
3. Culling at the screen edge and the distance fade: a slow camera turn past props and NPCs (no pop-in differences).
4. Translucent order: overlapping translucent map parts and effects (a fountain or window area).
5. Props: the moving background objects of an area (their motion and colour).
6. Lighting: Sora and one enemy under the area's lights, compared on shading only (mask the HUD).
7. Fog: an area with fog at two distances.
8. Area change: the frozen frame under the fade-out (the port must hold the last frame, not keep drawing).
9. Clear colour: an area whose sky does not cover the screen (the colour behind it).
Each check names the functions above so a failure maps to its row.

## Decompile index
Read for this classification (main image): `0x106868..0x1071c0`, `0x108c00..0x10ace0`, `0x11bb28..0x121770`, `0x132238..0x1358a0`,
`0x143100..0x149090`, `0x14d03c..0x14dc70`, `0x15f1b0..0x161238`, `0x172f00`, `0x172f20`, `0x188010`, `0x188bd8`, `0x192c80`, `0x1da840`,
`0x228e78`, `0x23b0f0`, `0x2dfd58..0x2ecd50`, `0x2f55c0`, `0x2f5858`. Callers and pointer words from the private workspace and a scan of
the executable and `OVL.IMG` for 32-bit words equal to each address.

## Gaps
- The part of `0x11f560` that is not packet build (which mesh entries it skips) was not separated; L1-P25 must read it again for skip rules.
- `0x147980` is named by 16 words of the executable: it may be a table value rather than only a function [inferred]; read before porting.
- Whether `0x10a9f8` (the lighting packet) also holds a lighting rule the (d) list misses was not checked [inferred: it only packs].
