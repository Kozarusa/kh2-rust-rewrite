# HUD (WP-4B phase 1 and WP-4B2 phase 2)

Status of what the in-battle HUD uses, where it comes from, and what is still open. "Verified" means read from the game's code or data and checked against the running result; "inferred" means a reading that fits the data but was not confirmed in code.

Decomp addresses are for the USA executable (SLUS_210.05), local study copy only.

## Files used (all read from the disc at run time)

- `field2d/us/<world>0field.2dd` (for example `tt0field.2dd`; `zz0field.2dd` is the fallback; the loader is at 0x13a7e8, format string at 0x378d78). A BAR of (IMGD type 24, SEQD type 25) pairs in this order: `fiel` (lock-on reticle, text banners, munny digits), `gaug` (Sora's gauges, target gauge, drive gauge pieces), `mapj` (minimap frame), `miss` (mission marks). The pair accessors are 0x13a980 (image) and 0x13a998 (sequence), the loader 0x13a8a8. Verified.
- `field2d/us/<world>0command.2dd` (loader 0x13a6d0, format strings 0x378d38 and 0x378d58: `<world><variant>command.2dd`, the variant a byte of the area record, `FUN_001935e8`; Twilight Town has variants 0 to 6, most worlds 0 and 1). One `comw` pair (IMGD 512x256, SEQD with 98 groups): the command menu. Pair accessors 0x13a9b0 and 0x13a9c0. This port always uses variant 0 and falls back to `zz0command.2dd`. Verified.
- `obj/P_EX100.apdx`, BAR entries `face` (IMGD + SEQD): Sora's portrait inside the ring (member resource +0x7b0, tag "face", fetched by 0x16b998). Verified.
- `msg/us/sys.bar`, `msg/us/fontimage.bar`, `msg/us/fontinfo.bar` (paths at 0x37ad50, 0x37b6f0, 0x37b6d8): the text system, see "Text". `03system.bin` entry `ftst`: text colours.
- Formats: IMGD per OpenKH `docs/kh2/file/type/image.md`; SEQD per OpenKH `OpenKh.Kh2/Sequence.cs` (header, sprites 0x2c bytes, sprite parts 0x14, sprite groups 4, animations 0x90, animation groups 0x24). Layout checked against section sizes of real files. The pixel order of swizzled IMGDs (flag bit 2) is the PSMCT32 upload canvas; decoded with the same GS tables as model textures. Verified on real files (images look right).

## Sequence player rules (verified in code)

- Frame stepping: 0x2378a0. A group starts at frame 0; one logic tick adds 1.0 (times a speed that is 1.0 for all HUD objects found). When the frame reaches the group end it restarts at the group's `loop_start` field; a non-zero `loops` count stops after that many plays and holds the last frame; zero loops forever. Group length: 0x236800 and 0x2367a8 (largest animation end, minus one for animations with flag 0x8000, capped by the group's explicit end). The tick rate (60 Hz) is inferred.
- Animation evaluation and drawing: 0x2379a8 (draw), 0x236cf8 (anchor), 0x235450 (quad build). An animation draws when `start <= frame <= end` (not on the end frame with flag 0x8000). Interpolation t = (frame - start) / (end - start); without flag 1 the time curve is a cubic Bezier with control values `curve_y_start`, `curve_y_end`. Translation and pivot are integer lerps; pivot is added to the sprite part before scale and rotation; scale = Scale x ScaleX (or ScaleY), rotation is the 2x2 reduction of the x, y, z Euler matrix at 0x2346a8; bounce adds `amplitude * sin|cos(t * pi * count)` (flag 8 selects cosine, flag 0x10 disables). Flags seen: 1 linear, 2 anchor, 4 nearest filtering, 0x20 no rotation, 0x40 no scale, 0x80 colour steps, 0x400 no colour, 0x4000 no pivot, 0x8000 exclusive end.
- Colour: per-channel `(sprite colour * animation colour) >> 7` (0x80 is 1.0), 0x230738. Blend preset from the animation's blend field: 0 alpha, 1 additive, 2 subtractive (GS ALPHA values set up at 0x236598); flag bit 2 selects nearest instead of bilinear texture filtering (TEX1 presets, same function). Zero-width UV sprites stretch one texel column.
- Positions: a HUD object's anchor is the translation (plus rotated pivot) of its first animation flagged 2 at the current frame (0x236cf8). A child is drawn at `parent base + parent anchor + parent offset + own offset` (0x138bf0, 0x138f38). Verified.
- Animation group record (0x24 bytes, accessors at 0x237618 to 0x237698): +0x10 x, +0x14 y (added to a node's position when its flag 0x10 is set, not used by anything ported here), +0x1c passed to the text drawer, +0x20 a width in pixels that row layouts add up (target pips, munny digits). The SEQD parser's names for these are `x`, `y`, `unk1c`, `unk20`. Verified in code (0x237698 callers at 0x1540a0 and 0x15ae80).
- Screen: draw code places vertices at GS (0x7000 + 16 x, 0x7300 + 16 y). The 512x448 screen origin is therefore taken as HUD y = 0 at 16 lines below the top (inferred from 0x7300 versus the usual 0x7200; the draw-environment setup was not found). The 3D projection used for the reticle uses the same offset. Text drawing uses the same constants (0x1a80d8), so text shares the HUD's coordinates.

## Text (WP-4B2)

Crate `kh2-image`, modules `font` and `text`; app side `hud_text.rs`.

### Files

- `msg/us/fontimage.bar`: three type-36 entries. `sys` (0x10000 bytes) is a 512x256 atlas of 4-bit pixels, low nibble first; `evt` (0x20000) is 512x512; `icon` (0xa400) is 256x160 8-bit indices followed by a 1024-byte palette. The loader at 0x1a9190 uploads `sys` to GS page 0 and `evt` to page 0x800 as PSMT4HL (the high-alpha-nibble format) and `icon` to 0x1800 as PSMT8H with the palette at 0x3ffc. Verified.
- The glyph atlases only use the low two bits of each nibble (counts of nibble values in both files: 0 to 3). The game samples them through rows of the icon palette: 0x1a9460 builds TEX0 with palette offset (CSA) 8 plus the page (glyph / glyphs per page) and 1, plus 2 for font mode 3 and 4 for mode 4. Palette row 8 has alpha 0, 42, 98, 128 for pixel values 0 to 3 (0x80 is opaque), row 9 picks by the high two bits, row 10 has 0, 0, 68, 128, row 12 is black at alpha 64, grey 68 at 128 and white at 128. The port reads the rows from the file. Verified (palette decode, and the text looks right).
- `msg/us/fontinfo.bar`: type-2 entries `sys` (280 bytes), `evt` (336), `icon` (60): one advance width in pixels per glyph or icon. The type-25 entry is not used by the text code that was read. The widths are loaded at 0x1a8ff8 into the font descriptors. Verified (280 = 28 x 10 cells, 336 = 21 x 16, 60 = 10 x 6).
- Glyph cells: font descriptors are built at 0x1a9e60 with 0x1a93e0: font ids 0, 1, 2 and 5 draw the `sys` atlas with 18x24 cells, ids 3 and 4 the `evt` atlas with 24x32 cells; icons are 24x24. The cell heights come from the override at 0x1a9148, which runs for every language but one (the value 1 of `FUN_00105cd8`, taken to be Japanese); the pitch of the glyph rows in the files (24 and 32 pixels) agrees with it. Glyph `g` is at column `g % cols`, row `g / cols` (cols 28 for `sys`, 21 for `evt`). Verified.
- Modes: the font id is also its mode. 0 and 5 plain, 1 drop shadow, 2 outline, 3 plain with the thin ramp (`evt`), 4 the shaded ramp with a skewed draw (`evt`). Mode 1 draws the glyph again 2 pixels right and down first in the shadow colour; mode 2 draws it four times at (-2, 0), (0, 2), (2, 0), (0, -2) (offsets at 0x35ea28; draw code 0x1a8698). The shadow colour is black with alpha 0x60 scaled by the text alpha (0x1a8de0 sets 0x60000000, 0x1a8ce0 scales it). Verified for 0 to 3; mode 4 is drawn here as a plain glyph with its palette row (the skewed draw at 0x1a9818 is not ported).

### Message table and byte code

- `sys.bar` entry `sys` (type 2): u32 version (1), u32 count, count x (u32 id, u32 offset), texts at the offsets. Table fields that name text (`Cmd.MessageId`, `Item.Name`) have bit 15 set; the file's id is the low 15 bits (the game's lookup is 0x190c40, with a fallback to message 0x8adc). 2,869 messages in the USA file. Verified.
- A byte of 0x20 or more is one glyph, glyph = byte - 0x20 (`FUN_00191030`). Bytes 0x19 to 0x1f followed by a byte select glyph (byte - 0x19) * 0x100 + next + 0xe0. Every other byte below 0x20 is a command; its length is read from the table at 0x37ae50 (`FUN_00190f68` advances by it): 0 end (1 byte), 1 space (1), 2 newline (1), 3 reset colour and size (1), 4 colour from `ftst` (2), 7 colour RGBA (5), 9 icon (2), 0x0a size in pixels (2), 0x0b width in percent (2), 0x0c extra line gap (2), 0x0d variable (1), 0x10 end of text (1), 0x11 position (5), 0x13 (5), 0x14 (3), others per the table. Only 1, 2, 3, 7, 9, 0x0a, 0x0b, 0x0d, 0x08, 0x11, 0x13 and 0x14 occur in `sys.bar` (histogram of the file); 0x0c, 0x0e and 0x15 to 0x17 are parsed by the game but absent from it. Verified.
- Which glyph is which character was read off the atlas and checked against messages: digits 1 to 10, `+ - x`, A to Z from glyph 14, `! ? % /`, the period at glyph 47 (byte 0x4f), the apostrophe at 204, lower case from 122, the accented letters after `z`, buttons and arrows near 207 to 222. `text::glyph_char` maps them; glyphs it does not know print as `<gN>`. The older `kh2_tables::msg::decode` has several of these punctuation marks wrong (0x48 is `!`, 0x49 is `?`, 0x50 is `,`), which is not edited here.

### Layout (0x1a8210, 0x1a8698, 0x191108, 0x1a7f60)

- A message object holds a font, a size scale S and a width scale X. Size comes from the caller (command bars use 14) or command 0x0a: S = size / byte +0x11 of the descriptor (18 or 24). Width factor X = percent / 100 (command 0x0b). A glyph quad is `cell_w * S * X` wide and `cell_h * S` high with its bottom on the baseline. The advance is `(int)(width * S * X + 0.5)`; a space is `(descriptor +0x10) / 3` times the same factors.
- The first baseline is the line top plus the line height, the largest `byte +0x11 * S` on the line. A newline adds the descriptor's line gap (+0x14: 6 for the `sys` fonts, 2 for `evt`) and the extra gap of command 0x0c, then the next line's height. For font 1 at size 14 the line pitch is 20 pixels. Icons are 24x24 and span 20 pixels above the baseline to 4 below it; their advance is the icon width table. Two long dashes (glyph 0x46) in a row overlap: the second is glyph 0x47 drawn 2 pixels (scaled) to the left. Verified in code; rendered output checked by eye against the layout rules.
- Alignment of a block (0x1a8018, table 0x37b5e4): 0 left, 1 right, 2 centre, 3 centre both ways, 4 centre x and bottom, 5 bottom. Implemented per line.
- `Messages` and `layout` are pure; tests in `kh2-image/tests/disc.rs` read the disc (`KH2_ISO`) and skip without it.

### Colours

- Command 4 reads `ftst` of `03system.bin` (loaded at 0x193610 into 0x1a7dd8, looked up at 0x1a7df8): u32 version, u32 count (9), records of 0x50 bytes with a u32 id (1 to 8 and 10) and one RGBA word per world index 0 to 18. World index to code: zz, es, tt, di, hb, bb, he, al, mu, po, lk, lm, dc, wi, nm, wm, ca, tr, eh (table at 0x349dc0). Verified.

## Sora's panel (class built at 0x156520, per-frame at 0x157a50)

Verified in code unless marked.

- Objects (gaug sequence groups, decimal): circle 137 (danger 132), ring back 121 (danger 122), ring trail 121 then 126 after a hit, ring fill 128, bar back 124 (danger 125), bar trail 127, bar fill 129, cap 123, MP label 116, MP back 119, MP fill 115, MP cap 120. Table at 0x358970, init at 0x156108 and 0x1563c8, switching at 0x157230.
- Anchor of the panel is group 137's translation (455, 355). The ring layers are children of it; the MP bar objects are children of the MP label (anchor (-40, 31) when settled), so the MP bar's right end sits at (415, 386) and it grows to the left.
- HP value rule (0x155400, 0x155530, 0x1552a0, 0x155378): below 60 HP the value is a ring sweep of pi/40 per HP (constant at 0x379374), starting at the west end and running clockwise, 270 degrees at 60. At 60 and above the ring is full and a bar extends left from the ring's bottom, 3 pixels per HP over 60 (bar sprites are 100 pixels wide and scaled). The same rule is used for three values: maximum HP (back layer), the HP before the last hit (trail) and the current HP. The cap piece rotates by minus the maximum's sweep on the ring, and sits at the bar's far end rotated 90 degrees on the bar.
- Hit and heal (0x1574d8): on a change, the trail value becomes the previous HP when HP dropped and the new HP when it rose, and the trail groups (126 and 127) restart; they fade out by their own animation.
- Danger (0x1a0be0): HP at or below `max >> 2` switches circle, back layers and face to the red variants. The face groups are 3 (normal) and 0 (pulsing red).
- MP (0x157628): bar length is the maximum MP in pixels, fill is the current MP. While MP recharges the panel uses the recharge look, see below.
- The ring layers are clipped by "mask" objects (rectangles written to the GS depth buffer, 0x13e010, test registers 0x32001). The clip is implemented here as an exact wedge test; the shape follows from the ring's measured coverage (all but the south-west quadrant) and the cap's rotation. Inferred.

### MP recharge look (WP-4B2)

- `FUN_001a05d0` is true while the player's float at +0x1c0 is positive; +0x1bc is the progress and `FUN_001a05f0` returns `+0x1bc / +0x1c0`. When it turns true, 0x157628 switches the four MP objects from groups 116, 119, 115, 120 to 110, 113, 109, 114 (red art) and the label's follow-up from 117 to 111. The back bar is then drawn at scale 1.0 (100 pixels, `DAT_00379370`) with the cap 100 pixels left, and the fill scales by the progress ratio (0x1556a0, 0x1555f8). Verified in code; the pictures match (a magenta fill on a dark red bar).
- Input: resource `MpRecharge { progress, total }`; the player module fills it from its recharge state. `KH2_HUDMPRECHARGE=<progress>:<total>` forces it.

### Drive gauge (WP-4B2)

- Objects 12 to 15 of the panel (built at 0x155c78, per-frame 0x1577d0, mode switch 0x156868): all children of the HP circle's anchor; the digit, MAX tag, burst and fill are children of the bar. Groups by the player's mode byte at +0x1af (0 normal, 2 and 3 two form variants, 4 locked): bar intro 21 / 50 / 140 / 18 (0x155a30), bar idle 23 / 51 / 141 / 19 (0x155a88), fill 25 / 49 / 139 / 25 (0x155b08). The digit group is `table[points]` with the three tables at 0x3589c0 (groups 28 to 37), 0x3589e8 (54 to 63) and 0x358998 (144 to 153) selected by the mode (0x155978). The MAX tag is group 20, shown when `points >= max` (+0x1b1 and +0x1b2); a point gained plays group 22 (and sound 0x20 or 0x21, not ported). Verified: the objects, the groups, the tables; the picture lines up between the HP ring and the MP bar.
- The orange fill is shown through a rotating mask (object at +0x30f0 built by 0x13ded0: a 50x120 rectangle turned by `fraction * 0.004014` radians, 0x155738; in the form modes by `value * 23 / count` degrees, 0x1557f8). That mask geometry is not ported: the port reveals the fill sprite from its left edge by the fraction (0 to 99 of the next point). This is an approximation.
- Not ported: the form-time fill of modes 2 and 3 (0x155bf0 turns the form's remaining time into a value; the port shows the digit and the bar only), the lock chains' attach and release animation, the burst sound. `FUN_00155f50` hides pieces by mode; the port shows the bar, fill and digit in every mode.
- Input: resource `DriveGauge { enabled, mode, points, max, fraction }`, off until the player has a drive gauge. `KH2_HUDDRIVE=<points>:<max>:<fraction>[:mode]` (mode 0, 2, 3, 4) forces it.

## Target gauge (class at 0x1540a0, update 0x154498, hit 0x154758, bind 0x19b4a8)

- Anchor (498, 31) from group 38 ("HP" tag). One 200-pixel segment per 200 HP: back bar (group 39), cap (40), trail (41, flashes then fades after a hit), current (43), frame (42, drawn full length). Bar widths in pixels equal the HP inside the segment; the segment arithmetic (C truncating division) is ported from 0x154498. Verified.
- Pip row (WP-4B2; 0x1540a0, 0x153ff8, 0x1543f0, 0x154758): up to 16 pairs of objects, children of the tag, laid out to the left with each pair a further `unk20` of group 45 (see the group record above). Group 45 is the pip's back and is shown for every segment below the top one, `(max - 1) / 200` of them. Group 44 is a filled pip, shown for each full segment left under the current one; a pip that was lost on the last hit (up to the previous HP's segments) switches to group 46. Verified in code; checked with 650 of 900 HP (three filled pips on four backs).
- The gauge is bound to the player's target when a status flag is set (status +0x1cc bit 0x80000, set at 0x14dfe8); which enemies get it and when it goes away were not resolved. This port shows it for the lock-on target and for 5 seconds after an enemy loses HP. Inferred stand-in.
- `KH2_HUDTARGETTEST=<hp>:<max>` draws the gauge for those values without an enemy.

## Lock-on reticle

- Pool of field icons: 0x13e5a0 (nine slots), created by 0x13e528 with a group number (0x13e1f0: the icon is the group, and a second member of group 20 is its speech bubble, hidden at creation), placed at a projected world point (0x13e968). The controller is 0x11a9b0: the icon group is 25 once a target is acquired and 3 otherwise; with no target the point is the camera position plus the camera's forward vector times a distance (0x11a5f8, the constant at 0x3788f8 is 300 units). The icon is hidden only while an event is running (`FUN_00196418` is `DAT_0035e3a4 != 0`, the event object). So the free-aim reticle is on screen whenever nothing is locked. Verified in code; the reading that group 3 is the yellow bracket is from the picture.
- Bubble (WP-4B2): 0x19bbc0 shows the group-20 member through 0x13e438 when the target's flag +0x588 has bit 1 and one of its timers at +0xac8 or +0xacc is positive. What sets those is not traced. Input: resource `TargetBubble(bool)`; `KH2_HUDBUBBLE=1` forces it. Group 20 looks like a speech bubble with a small emblem.
- The other 0x19bbc0 icon kinds (groups 5, 1, 0x1b, 0x17 above other targetable objects) are not drawn.

## Command menu (WP-4B2)

Module `hud_menu.rs`; art `comw` from the command file; group tables read from the executable at run time (`Tables::from_exe`).

- Structure (0x13ca30 builds it, 0x13c4b0 draws a list): up to three pages (a list per menu depth), each with a kind (`list[0]`), a count and entries of 12 bytes (command id at +0xe, a style byte at +0x11, a status byte at +0x12). A page is drawn at x = 28 + 16 * page. Bars are 22 pixels apart and the list ends at y = 410: bar `i` of `n` is at y = 410 - 22 n + 22 i. The header banner is at (x, 388 - 22 n) (0x13bc50); the reaction capsule at (36, 377 - 22 n) (0x13cef8, using the deepest page's count). The selected bar is drawn 8 pixels to the right (0x13c288). Verified in code.
- Groups: 18 kinds in the table at 0x3551c0, 12 words each: header groups (page 0 to 2), selected-bar groups, bar groups, last-bar groups. The art of each kind from the sheet: 0 and 6 COMMANDS (cyan), 1 MAGIC, 2 ITEMS, 3 FORMS, 4 PARTY, 7 SUMMONS, 8 FRIENDS, 9 COMBINATION, 11 PARTY again; 5, 10 and 12 to 17 all use group 64, the SHORTCUTS banner (with the town silhouette, a single group). Kinds 0 and 6 open with the groups at 0x355530 (17, 85, 30) and then idle on 0x355540 (18, 86, 31); the others use their table group directly (0x13bb58). The selected bar also draws a medallion (0x355560: 45, 47, 46 by page), the icon of the command (switch at 0x13bd48 on `cmd.CmdIcon`: 1 to 7 give groups 37, 41, 39, 44, 43, 38, 40) and an arrow (group 20 for kind 0, 21 for the others). Verified (tables), the pictures match the game's menu.
- Names: the bar's text is the command's message (`cmd.MessageId`, +8 of the command record, looked up with 0x190c40), font 1 (drop shadow) at size 14, 10 pixels right of and 7 above the bar's origin (0x13abf0 sets +0x190 = 10, +0x194 = -7, +0x19c = 14, +0x1a0 = 1). The colour is a table by the entry's style byte (normal 0x355600, selected 0x355628; styles 0 white, 1 grey, 2 orange); the list-mode table at 0x3555d8 is not used. Verified in code.
- Reaction capsule: with a command the groups are 2 (0x355678), 5 for entry type 7, 3 when the command's flag bit 3 is set; without one it is group 0 (empty). The capsule's text is the command's name at the same offset as a bar's.
- Input: resource `CommandMenu { visible, pages: [{ kind, entries: [{ message, icon, style }], cursor }], reaction }`. The player module fills it. `KH2_HUDMENU=<kind>:<rows>:<cursor>[:r][;...]` shows lists with names from the command table.
- Not done: item counts (groups 88 to 97, `x0` to `x9`, one digit each), MP costs and other per-entry values, the second set of row groups for the "build all" path (0x13cca8, groups 64 to 68, used when the list is re-created without animation), the open and close transitions of rows, the base panel nodes (groups 6 and 7 at 0x13d260, whose job was not worked out), the shortcut buttons (groups 85 to 97 area), the Friends and Party pop-ups' own layout, the variants 1 to 6 of the Twilight Town file, and the key handling. The medallion and icon placement assumes they sit at the bar's origin (their groups carry their own offsets); that is inferred.

## Text banners and the munny popup (WP-4B2)

Module `hud_banner.rs`.

- The `fiel` image holds the word strips FINISH!, COMBO!, CHANCE!, PUSH!, OBTAINED MUNNY!, NEXT LEVEL and the munny digits. The groups were matched to words by the texel rectangles each group draws: 66 FINISH!, 62 COMBO!, 58 and 60 CHANCE!, 64 PUSH!, the odd group before each (57, 61, 63, 65) is the short opening animation (a triangle button with a burst), 48 NEXT LEVEL, 17 OBTAINED MUNNY!. Inferred from the sprite rectangles and the pictures; the callers that create these objects (0x151970 with group 0x11 and 0x151d48 with group 0x30, each with six child players of groups 0x1c and 0x26) were not followed, so when each word is shown and where is not known.
- Munny popup (0x15b198 builds, 0x15ae80 lays out, 0x15b010 formats): the root is group 16 (the munny symbol), the digits are groups 6 to 15 (table 0x358c50), the amount is formatted with `%d`. Each digit advances by the `unk20` of its group; the root's offset is half the total width minus the symbol's width and digit `k` is at `sum of earlier advances - half`, so the group is centred on the popup's position. The game places the popup at a projected world point and the counter caps at 9,999,999 (seven digits). Verified in code; not hooked to a game event (the port has no munny drops).
- Inputs: `HudBanner(Option<{ kind, pos }>)` and `MunnyPopups(Vec<{ amount, pos }>)`. `KH2_HUDBANNER=<push|chance|combo|finish|nextlevel|munny>[:x:y]`, `KH2_HUDMUNNY=<amount>[:x:y]`, and `KH2_HUDFREEZE=<frame>` to hold their sequences at a frame for a screenshot.

## Damage numbers

The original draws no damage numbers. The only floating-number system found is the digit popup at 0x15b198 (above), called from the enemy-defeat code (0x1b25d0) and 0x1c46a8 with a value that is added to the munny counter (0x1b51e0, capped at 9,999,999). So it is a munny gain popup. The old floating damage text remains only as a debug aid behind `KH2_DEBUGHUD=1` or F9.

## Game-state inputs (resources the integrator fills)

| Resource (module) | Fields | Source in the game |
|---|---|---|
| `CommandMenu` (hud_menu) | `visible`, pages with `kind`, `entries { message, icon, style }`, `cursor`, `reaction { message, alt, kind7 }` | the command lists at player +0x974 (0x13ca30), `cmd` records (`MessageId`, `CmdIcon`) |
| `DriveGauge` (hud_gauge) | `enabled`, `mode`, `points`, `max`, `fraction` | player +0x1af, +0x1b1, +0x1b2, +0x1b0 |
| `MpRecharge` (hud_gauge) | `progress`, `total` | player +0x1bc, +0x1c0 |
| `TargetBubble` (hud_target) | bool | owner +0x588 bit 1 and timers +0xac8 / +0xacc |
| `HudBanner`, `MunnyPopups` (hud_banner) | see above | not traced |

Sora's HP and MP and the lock-on target are still read from the existing `Sora` and `Enemy` components.

## Debug aids

`KH2_DEBUGHUD=1` or F9: developer text and damage text. `KH2_HUDLOG=<seconds>`: dump every HUD node and draw batch once. `KH2_HUDHIDE=3,9`: hide nodes by index. `KH2_HUDTEST=hp,max,mp,mpmax`: draw other values than the game state holds (display only). Phase 2: `KH2_HUDTEXT=<hex id>[:font[:size[:x:y]]]` (comma separated), `KH2_HUDMENU`, `KH2_HUDDRIVE`, `KH2_HUDMPRECHARGE`, `KH2_HUDTARGETTEST`, `KH2_HUDBUBBLE`, `KH2_HUDBANNER`, `KH2_HUDMUNNY`, `KH2_HUDFREEZE` as described above.

## Rendering notes

A second camera (order 10, render layer 7, same MSAA setting as the 3D camera; a different setting smeared the previous frames) shows the 512x448 HUD scaled to fit the window; wider windows get empty margins. Quads become triangles with vertex colours, one mesh per run of equal texture, blend and filter, rebuilt every frame. Colours are modulated in linear light with the vertex colour raised to 2.2 so opaque colours match the PS2's gamma-space product; the alpha blend itself happens in linear light, so translucent edges differ slightly from the original. Text uses the same path: the font atlases and the icon atlas become textures (one per palette row), glyph quads are ordinary quads.

## Not done, and what each needs

- Party members' panels (0x154a78, objects of 0x1570 bytes): the build is read (circle 71, rings 64, 65, 66 with three mask objects of 0x13e010, the member's own `face` from its apdx, placement by the group record's `unk20` times the slot at 0x154a78's end, HP-to-angle rules at 0x154818 and 0x154930 that use rotating masks like the drive fill), but the masks are not ported and the port has no party members to draw. Needs the party and the mask geometry.
- Area-name card: not found. `msg/us/place.bin` is a Shift-JIS table of the original (Japanese) place names, not text for display; the per-world message files (`msg/us/tt.bar` and the others) are the likely source and the card's drawing code was not located. Minimap: the `mapj` pair (128x128 image, 2 groups) and `radar.tm2` exist; the pair's callers are 0x15d058 and 0x15d1a8 but KH2 shows no minimap in normal play, so nothing is drawn.
- Level-up screen text and the digit counters of the Sora panel (HP numbers, hidden by default; groups 28 to 37 of `gaug` and the larger digit set at the top of the image).
- Text: command 0x0e (skew), 0x0f, 0x11, 0x13 to 0x17 callbacks (position set, typewriter timer, per-glyph callbacks), the skewed draw of font mode 4, glyph pages above the first (the second plane of the atlases is empty in the USA data), message windows themselves (frame, scrolling, page breaks).
- UV scrolling inside a sprite rectangle (no scrolling sprites in these files; the player wraps without the GS region clamp).
- Whether the 3D view itself is a 512x416 region (the projection offset suggests it).
