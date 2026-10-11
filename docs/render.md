# Rendering notes: texture regions and texture animation

How map and model textures are sampled and animated, as the original does it. Code:
`crates/kh2-model/src/texture.rs` (GS-info decode, `Axis` regions), `crates/kh2-model/src/texanim.rs`
(footer parse and the players), `crates/kh2-app/src/assets.rs` (crop + UV transform),
`crates/kh2-app/src/texanim.rs` (per-tick plugin).

## tt04 roofs: the black rectangles (WP-7M step 1)

Symptom: black rectangles over tt04's roof tiles and awnings (accept `tt04-dusk.png`, upper right).

Guess checked first: texture animation with a blank base frame. **Refuted.** The `MAP` texture
container of `map/us/tt04.map` has no TEXA or UVSC block: its footer (palette end, FUN_00132238) is
only the `_KN5` end marker (`cargo test -p kh2-model --test disc -- --nocapture`:
`tt04.map "MAP" textures: footer starts with Some("_KN5")`, `tt04 MAP: 130 textures, 1394 meshes, 0 TEXA, 0 UVSC`).

Cause, (d) "something else": **GS CLAMP region modes.** Most tt04 textures are sub-images of shared
GS pages: several GS-infos point at the same upload (same TBP0) with different CSA (palette), and each
restricts sampling with its CLAMP register to its own part of the page:

* REGION_REPEAT (WMS/WMT = 3): texel `(t & MSK) | FIX`, MSK in the MINU/MINV field, FIX in MAXU/MAXV.
  E.g. a 128-wide roof texture with UMSK 63: only texels 0..63 repeat, while the mesh UVs run to ~900 texels.
* REGION_CLAMP (2): texels MINU..=MAXU / MINV..=MAXV.

The app sampled the whole page with plain repeat / clamp, so every other run of texels showed the
neighbouring sub-image decoded through this GS-info's palette, which mostly comes out black.
`tt04_map_has_no_texture_animation_and_samples_region_wrapped_atlases` lists 35 texture axes whose mesh
UVs run past a region smaller than the texture. The draw code sends the GS-info registers as stored
(FUN_0011f560 sends the mesh's GS-info block with FUN_00133e90; TEX0 at +0x70, CLAMP at +0x80).

Located with `KH2_TEXTINT`: textures 39, 51, 53, 54, 55, 58, 14, 30, 44 (all REGION_REPEAT with a
64-texel U mask on 128-texel textures) are the red-tile roofs; 69, 76, 78, 79, 81, 83, 85, 91, 92, 101,
128, 74 include the yellow-tile roof and awning edges.

Fix: `Texture::region` (`Axis::from_clamp`) gives the sampled run per axis; the app crops the image to it
and maps UVs with `u' = (u·W − start) / len` (`assets::region_affine`), sampler repeat or clamp-to-edge
per the region's mode. REGION_REPEAT is exact when `MSK + 1` is a power of two and FIX has no mask
bits (every tt04 texture); other masks fall back to plain REPEAT (none found).

## Texture container footer

After the palette (`palette offset + 4 × colour count`, FUN_00132238) come tagged blocks
`tag[4], u32 size, body[size]` (OpenKH `raw-texture.md`). The game accepts only `TEXA`, `UVSC`,
`_DMY` and `_KN5` (FUN_00132438), walks them with FUN_001323b0 / FUN_00132360 and stops at `_KN5` or an
unknown tag. FUN_0011e208 makes a player object per block (TEXA: 0x290 bytes, vtable 0x354da0; UVSC:
0x2c bytes, vtable 0x354dd0) and links them on the model's list (+0x38).

TEXA body (offsets from the body start, as the game reads them):

| off | meaning | reader |
|---|---|---|
| +2 u16 | GS-info index the sprites go into | FUN_001331c8 |
| +4 u16 | extra halfwords per frame entry (entry = 8 + 2n bytes) | FUN_00132a80 |
| +6 u16 | sprite bits per pixel | FUN_00132fc0 |
| +8 / +0xa u16 | first / last slot of the slot table | FUN_001327f8 |
| +0xc u16 | animation count | FUN_001327f8 |
| +0xe u16 | sprite count | (parse) |
| +0x10..+0x16 u16 | destination x, y, width, height in texels | FUN_00132f00..FUN_00132f90 |
| +0x18 u32 | slot table offset: u16 per slot, animation index + 1, 0 none | FUN_001327f8 |
| +0x1c u32 | animation table: u32 offset of each animation's first frame | FUN_001328a8 |
| +0x20 u32 | sprite pixels: sprite k at `+k × (w × bpp / 8) × h`, linear in the texture's PSM | FUN_00132fc0 |
| +0x24 u32 | default animation index | FUN_001328a8 |

Frame entry: `u16 ctrl, u16 min, u16 max, u16 sprite, …`; `ctrl & 15`: 0 show sprite, 1 show none
(the base image), 2 jump by `ctrl >> 4` (signed) entries, 3 stop. Note: the slot table holds u16
entries (OpenKH lists 4-byte entries; the reader is FUN_001327f8).

UVSC body: `u32 index, f32 speed_u, f32 speed_v`. A map mesh names its UVSC entry (index + 1) in
bits 1..5 of its +0xc word (FUN_0011f560 reads `(+0x2c >> 1) & 0x1f`), a skeletal mesh in bits 22..26
of its header flags (FUN_00121a78).

## Runtime (ported exactly; oracle-tested)

* Clock: FUN_0011e7f8 calls every object's update with `DAT_00356f40`, the field delta (1.0 per 60 Hz
  field, FUN_0014cbf8 / FUN_0014cdd0), times the object's speed +0x18 (1.0, `DAT_00378cb0`). The app
  ticks in `FixedUpdate` at 60 Hz with dt 1.0.
* TEXA start FUN_00132950: when the requested animation (+0x274, the default at setup, FUN_001328a8)
  differs from the playing one (+0x278): flag 1 (no advance this field), frame length (+0x280) =
  random in `[min, max]`, time (+0x284) = 0, current frame (+0x27c) = its first entry.
* TEXA update FUN_00132a80: start unless flag 4 (a slot request this field); a jump entry at the
  current frame is followed; then `time += dt` and while `length <= time` step to the next entry
  (jump: follow, or end if flag 2; stop: hold) and draw a new length.
* Frame length FUN_00106380 → FUN_00106308: `min + fixuns(n × rand() / 2147483520.0)`, `n = max + 1 − min`,
  `fixuns` = FUN_00326940 (negative → 0). The game's `rand()` (FUN_003117a8) is one LCG for the whole
  game; the app uses its own generator of the same form, so the random lengths differ in sequence
  `[inferred]` but not in range.
* Current sprite FUN_00132fc0: the current entry's sprite when its code is 0, else none. Each frame the
  draw code re-uploads the base image and then uploads each showing sprite at (x, y) with the
  texture's DBP, TBW and PSM (FUN_0011f560 / FUN_00121a78 → FUN_001095f0). The app rebuilds the
  image from the base pixels plus the showing sprites when a sprite changes.
* Slot requests FUN_00133058: flag 4 and start the animation the slot names. Motion **range trigger 23**
  requests its first parameter as slot every field its range covers the motion frame: the range
  dispatcher FUN_00188bd8 calls FUN_00188030 for each range containing the frame (end 0xffff open),
  whose case 0x17 calls FUN_00192c80 → FUN_0011e850 (every TEXA whose slot table has it).
  Sora's attack and idle motions use it (eyes and mouth: `KH2_TEXLOG=1` shows
  `motion range trigger 23 slot None -> Some(68) (A300)` and the sprite changes that follow).
* UVSC update FUN_00133518: unless stopped (flag 2) or just started (flag 1): `acc_u += (int)(speed_u × dt)`,
  `acc_v` likewise (i32, wrapping), time +0xc += dt.
* UV offset FUN_00133628 / FUN_001336d8: `(fixuns((float)(u32)acc × (span / size)) >> 16) & 0xfff` with
  `span = MINU + 1` (MINV + 1) of CLAMP and `size` the texture width (height, table 0x34e898 = 1 << n);
  applied only when the CLAMP's horizontal mode is REPEAT or REGION_REPEAT (both draw paths test
  `CLAMP & 3` alone). The app treats 4096 as one texture width/height and adds the offset to the UVs
  before the region transform `[inferred]`: the VU1 microcode that consumes it was not read; the 12-bit
  mask is what suggests one wrap = 4096.

Data found: Sora (`obj/P_EX100.mdlx`) has 2 TEXA blocks on texture 3: eyes (default animation:
sprite, sprite, then nothing for 30..240 fields, loop = a blink) and mouth (default: nothing, stop).
The tt04 sky (`SK0`) has 3 UVSC entries; one sky mesh scrolls by entry 0 (about half its texture per
minute). The tt04 `MAP` and `BOB` texture containers have no animation.

## Debug aids

* `KH2_TEXLOG=1`: per drawn mesh at load: texture, size, region, TEXA count, UVSC; every sprite change
  and every range-trigger slot change.
* `KH2_TEXTINT=<i,j,…>`: draw those texture indexes plain magenta (find the meshes under a symptom).
* `KH2_TEXA_PIN=<texa>:<sprite>`: hold one sprite (e.g. `0:5` closes Sora's eyes) for a look.

## Decompile index

| address | what |
|---|---|
| FUN_0011e018, FUN_0011e1c8, FUN_0011e208 | texture setup, player objects per footer block |
| FUN_00132238, FUN_00132288 | footer start; texture collection entry |
| FUN_001322e0, FUN_00132320, FUN_00132360, FUN_001323b0, FUN_00132438 | tag, size, next, find, known-tag test |
| FUN_001324a0, FUN_001326e8, FUN_001333c8 | object constructors (base, TEXA, UVSC) |
| FUN_001327f8, FUN_001328a8, FUN_00132910, FUN_001329e0, FUN_00133058, FUN_00133168 | animation index, default, match, play by index / slot |
| FUN_00132950, FUN_00132a80, FUN_00132c90, FUN_00132ca0, FUN_00132ce0 | TEXA start, update, stop, active, equal (for linking) |
| FUN_00132ec8, FUN_00132ee8, FUN_00132f00..FUN_00132fc0 | upload packet, double buffer, rectangle, sprite pixels |
| FUN_001331c8, FUN_001332a8 | TEXA objects per GS-info |
| FUN_00133490, FUN_001334f8, FUN_00133518, FUN_001335c8, FUN_001335d8, FUN_001335f8 | UVSC match, reset, update, stop, active, data |
| FUN_00133628, FUN_001336d8, FUN_0010ace0 (table 0x34e898) | UV offsets |
| FUN_0011f560, FUN_00121a78 | map / model draw: region test, scroll, sprite upload |
| FUN_001095f0 | sprite upload (BITBLTBUF/TRXPOS/TRXREG per PSM) |
| FUN_0011e7f8, FUN_00143300, FUN_0015fef8 | per-frame update of a model's objects |
| FUN_0014cbf8, FUN_0014cdd0 | field delta `DAT_00356f40` |
| FUN_00106380, FUN_00106308, FUN_003117a8, FUN_00326940 | random length, rand, float → unsigned |
| FUN_0011e850, FUN_0011e8c0, FUN_0011e930, FUN_00192c50/68/80 | slot requests on all TEXA of a model |
| FUN_00188010, FUN_00188030, FUN_00188bd8, FUN_0017f5b8 | motion range triggers (case 0x17 = TEXA slot) |

## Gaps

* Linked TEXA objects (FUN_0011e3b8 / vtable +0x20 FUN_00132ce0): in a texture *collection* (container
  magic −1, FUN_0011e018) equal TEXA blocks of later sets follow the first set's timing. The decoder does
  not read collections at all (`texture::decode` needs magic 0); none of tt04 / P_EX100 is one.
* Other slot-request callers not wired: FUN_0011e8c0 (FUN_00142ba8, FUN_0015f490, FUN_0015f8e0,
  FUN_0023b0f0, FUN_00192c50 ← FUN_00172f00, FUN_00228e78, FUN_002dfff0) and FUN_0011e930 (FUN_00192c68
  ← FUN_00172f20, FUN_00228e78, FUN_002dfd58): event / script paths not traced.
* VU1 use of the UV offset (units, sign) not read; see `[inferred]` above.
* tt04's `BOB` models (BAR type 4/7/30 entries tagged `BOB`) are not drawn by the app at all
  (`kh2_model::map_file` loads only `MAP` and `SK0`); their textures have no animation.

## Sky placement (G4, tt05 green plane)

Symptom: in the second Sandlot (`tt05`) a flat green plane covered the floor and moved with the camera.
Cause: the sky model (`SK0`) has ground meshes at world height -200 (tt05 mesh 7, tt04 mesh 5, x -60935..65904,
z -43041..48327), while the app moved the whole sky to the camera, which put that backdrop 2 m below the camera
and through the floor. The game builds the sky object at position 0 (FUN_0015e6c0 -> FUN_0015f3a0) and its update
(FUN_0015fef8) only advances texture players, so the app now keeps the sky at the origin
(`assets::sky_follow`). `[inferred]`: that no other code moves it. Gap: maps also carry `SK1` skies (tt00, tt03,
tt24..28; counts at map +0x6c SK0 / +0x70 SK1, FUN_00149348) that `kh2_model::map_file` does not load, and the
dome meshes (about 3.5 million units wide) lie beyond the camera's far plane.
