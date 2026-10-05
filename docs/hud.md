# HUD (WP-4B, phase 1)

Status of what the in-battle HUD uses, where it comes from, and what is still open. "Verified" means read from the game's code or data and checked against the running result; "inferred" means a reading that fits the data but was not confirmed in code.

Decomp addresses are for the USA executable (SLUS_210.05), local study copy only.

## Files used (all read from the disc at run time)

- `field2d/us/<world>0field.2dd` (for example `tt0field.2dd`; `zz0field.2dd` is the fallback; the loader is at 0x13a7e8, format string at 0x378d78). A BAR of (IMGD type 24, SEQD type 25) pairs in this order: `fiel` (lock-on reticle, text banners, munny digits), `gaug` (Sora's gauges, target gauge, drive gauge pieces), `mapj` (minimap frame), `miss` (mission marks). The pair accessors are 0x13a980 (image) and 0x13a998 (sequence), the loader 0x13a8a8. Verified.
- `obj/P_EX100.apdx`, BAR entries `face` (IMGD + SEQD): Sora's portrait inside the ring (member resource +0x7b0, tag "face", fetched by 0x16b998). Verified.
- Formats: IMGD per OpenKH `docs/kh2/file/type/image.md`; SEQD per OpenKH `OpenKh.Kh2/Sequence.cs` (header, sprites 0x2c bytes, sprite parts 0x14, sprite groups 4, animations 0x90, animation groups 0x24). Layout checked against section sizes of real files. The pixel order of swizzled IMGDs (flag bit 2) is the PSMCT32 upload canvas; decoded with the same GS tables as model textures. Verified on real files (images look right).

## Sequence player rules (verified in code)

- Frame stepping: 0x2378a0. A group starts at frame 0; one logic tick adds 1.0 (times a speed that is 1.0 for all HUD objects found). When the frame reaches the group end it restarts at the group's `loop_start` field; a non-zero `loops` count stops after that many plays and holds the last frame; zero loops forever. Group length: 0x236800 and 0x2367a8 (largest animation end, minus one for animations with flag 0x8000, capped by the group's explicit end). The tick rate (60 Hz) is inferred.
- Animation evaluation and drawing: 0x2379a8 (draw), 0x236cf8 (anchor), 0x235450 (quad build). An animation draws when `start <= frame <= end` (not on the end frame with flag 0x8000). Interpolation t = (frame - start) / (end - start); without flag 1 the time curve is a cubic Bezier with control values `curve_y_start`, `curve_y_end`. Translation and pivot are integer lerps; pivot is added to the sprite part before scale and rotation; scale = Scale x ScaleX (or ScaleY), rotation is the 2x2 reduction of the x, y, z Euler matrix at 0x2346a8; bounce adds `amplitude * sin|cos(t * pi * count)` (flag 8 selects cosine, flag 0x10 disables). Flags seen: 1 linear, 2 anchor, 4 nearest filtering, 0x20 no rotation, 0x40 no scale, 0x80 colour steps, 0x400 no colour, 0x4000 no pivot, 0x8000 exclusive end.
- Colour: per-channel `(sprite colour * animation colour) >> 7` (0x80 is 1.0), 0x230738. Blend preset from the animation's blend field: 0 alpha, 1 additive, 2 subtractive (GS ALPHA values set up at 0x236598); flag bit 2 selects nearest instead of bilinear texture filtering (TEX1 presets, same function). Zero-width UV sprites stretch one texel column.
- Positions: a HUD object's anchor is the translation (plus rotated pivot) of its first animation flagged 2 at the current frame (0x236cf8). A child is drawn at `parent base + parent anchor + parent offset + own offset` (0x138bf0, 0x138f38). Verified.
- Screen: draw code places vertices at GS (0x7000 + 16 x, 0x7300 + 16 y). The 512x448 screen origin is therefore taken as HUD y = 0 at 16 lines below the top (inferred from 0x7300 versus the usual 0x7200; the draw-environment setup was not found). The 3D projection used for the reticle uses the same offset.

## Sora's panel (class built at 0x156520, per-frame at 0x157a50)

Verified in code unless marked.

- Objects (gaug sequence groups, decimal): circle 137 (danger 132), ring back 121 (danger 122), ring trail 121 then 126 after a hit, ring fill 128, bar back 124 (danger 125), bar trail 127, bar fill 129, cap 123, MP label 116, MP back 119, MP fill 115, MP cap 120. Table at 0x358970, init at 0x156108 and 0x1563c8, switching at 0x157230.
- Anchor of the panel is group 137's translation (455, 355). The ring layers are children of it; the MP bar objects are children of the MP label (anchor (-40, 31) when settled), so the MP bar's right end sits at (415, 386) and it grows to the left.
- HP value rule (0x155400, 0x155530, 0x1552a0, 0x155378): below 60 HP the value is a ring sweep of pi/40 per HP (constant at 0x379374), starting at the west end and running clockwise, 270 degrees at 60. At 60 and above the ring is full and a bar extends left from the ring's bottom, 3 pixels per HP over 60 (bar sprites are 100 pixels wide and scaled). The same rule is used for three values: maximum HP (back layer), the HP before the last hit (trail) and the current HP. The cap piece rotates by minus the maximum's sweep on the ring, and sits at the bar's far end rotated 90 degrees on the bar.
- Hit and heal (0x1574d8): on a change, the trail value becomes the previous HP when HP dropped and the new HP when it rose, and the trail groups (126 and 127) restart; they fade out by their own animation.
- Danger (0x1a0be0): HP at or below `max >> 2` switches circle, back layers and face to the red variants. The face groups are 3 (normal) and 0 (pulsing red).
- MP (0x155f..., 0x157628): bar length is the maximum MP in pixels, fill is the current MP; a different group set (110, 113, 109, 114) and a ratio-based fill are used while MP recharges (status +0x1bc/+0x1c0); not drawn yet (see gaps).
- The ring layers are clipped by "mask" objects (rectangles written to the GS depth buffer, 0x13e010, test registers 0x32001). The clip is implemented here as an exact wedge test; the shape follows from the ring's measured coverage (all but the south-west quadrant) and the cap's rotation. Inferred.

## Target gauge (class at 0x1540a0, update 0x154498, hit 0x154758, bind 0x19b4a8)

- Anchor (498, 31) from group 38 ("HP" tag). One 200-pixel segment per 200 HP: back bar (group 39), cap (40), trail (41, flashes then fades after a hit), current (43), frame (42, drawn full length). Bar widths in pixels equal the HP inside the segment; the segment arithmetic (C truncating division) is ported from 0x154498. Verified.
- The gauge is bound to the player's target when a status flag is set (status +0x1cc bit 0x80000, set at 0x14dfe8); which enemies get it and when it goes away were not resolved. This port shows it for the lock-on target and for 5 seconds after an enemy loses HP. Inferred stand-in.
- Not drawn: the row of pips for remaining segments (groups 44 to 46, 0x154758).

## Lock-on reticle

- Pool of field icons: 0x13e5a0 (nine slots), created by 0x13e528 with a group number, placed at a projected world point (0x13e968). The controller is 0x11a9b0: group 3 (orange) is the default, group 25 (blue) is used once a target is acquired; this port shows group 25 on the lock-on target only. The free-aim orange reticle (point a fixed distance ahead of the camera, 0x11a5f8) and the second icon (group 20, a speech bubble shown when the target is alert, 0x19bbc0 and 0x13e438) are not drawn. Reading of which group means what is inferred.

## Damage numbers

The original draws no damage numbers. The only floating-number system found is the digit popup at 0x15b198 (digit groups 6 to 15 of the `fiel` sequence, prefix group 16, digit table 0x358c50, formatter 0x15b010), called from the enemy-defeat code (0x1b25d0) and 0x1c46a8 with a value that is added to the munny counter (0x1b51e0, capped at 9,999,999). So it is a munny gain popup. It is not drawn here (the port has no munny drops). The old floating damage text remains only as a debug aid behind `KH2_DEBUGHUD=1` or F9.

## Debug aids

`KH2_DEBUGHUD=1` or F9: developer text and damage text. `KH2_HUDLOG=<seconds>`: dump every HUD node and draw batch once. `KH2_HUDHIDE=3,9`: hide nodes by index. `KH2_HUDTEST=hp,max,mp,mpmax`: draw other values than the game state holds (display only).

## Rendering notes

A second camera (order 10, render layer 7, same MSAA setting as the 3D camera; a different setting smeared the previous frames) shows the 512x448 HUD scaled to fit the window; wider windows get empty margins. Quads become triangles with vertex colours, one mesh per run of equal texture, blend and filter, rebuilt every frame. Colours are modulated in linear light with the vertex colour raised to 2.2 so opaque colours match the PS2's gamma-space product; the alpha blend itself happens in linear light, so translucent edges differ slightly from the original.

## Not done, and what each needs (phase 2 and leftovers)

- MP recharge look: needs Sora's recharge state (status +0x1bc/+0x1c0, 0x1a0540, 0x1a05a8) from the player module.
- Drive gauge and form gauge (objects 12 to 15 of the panel, groups 17 to 37, 0x155978 tables 0x3589c0, 0x3589e8, 0x358998): needs the player-data work.
- Party members' panels (0x154a78, sizes 0x1570): need the party.
- Command menu (`field2d/us/<world>0command.2dd`, loader 0x13a6d0): phase 2.
- Level-up, "CHANCE!", "FINISH!", "COMBO!" banners and munny digits: groups exist in `fiel`; their callers were not traced.
- Area name card: not found (candidates: `msg/us/place.bin`, `00place.bin`). Minimap: `mapj` pair and `radar.tm2` exist; not traced.
- Digit counters of the Sora panel (HP numbers, hidden by default), the pip row of the target gauge, the lock-on bubble, text rendering (`msg/us/sys.bar`, `fontimage.bar`, `fontinfo.bar` are located but not decoded).
- UV scrolling inside a sprite rectangle (no scrolling sprites in these files; the player wraps without the GS region clamp).
- Whether the 3D view itself is a 512x416 region (the projection offset suggests it).
