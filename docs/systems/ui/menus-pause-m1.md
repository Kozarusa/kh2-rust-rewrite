# Pause menu: M1 as built

[Contract](contract.md) | [Index](../../README.md)

## 9. M1 as built (WP-M1)

Code: `menu.rs` (state, input, freeze, transition), `menu_gfx.rs` (draw path and camera, a copy of the HUD's because `hud_gfx` is private), `menu_text.rs`, `menu_pages.rs` (page 0 table and flags), `kh2-image::layd` (the `.2ld` reader).

- **Closed gaps (addresses).** System sounds: `FUN_002be49c(2, id, ...)`: the 2 is the driver SLOT that holds `se/se000.seb` (docs/systems/audio/audio.md "System sounds": `FUN_002c0eb0` loads it at boot, `FUN_002c0da8` names the slot bank 0), not bank 2 (WP-SE2 correction; `se002.seb` is a two-entry area bank, so the earlier `PlaySe::number(2000 + id)` never found its data). The menu sounds are entries of `se000.seb`, request numbers `id` (log form `se 0:<id>`): cursor 1, confirm 2, cancel 4, refused 5, open 3 (`menu::menu_sound`; `FUN_002c2b70` at 0x2c2cd8 is the same call with a slot taken from a request). Row groups: each entry's set is (enter, leave or inactive, loop); `FUN_00296fb8` starts a selected row in set B[0] with B[2] as the follow-up, `FUN_00298808` (list inactive) switches rows to index 1, `FUN_002988e8` back to index 2. Row pitch is the group's `unk20` (`FUN_00296bd0` with `FUN_00237698`), 26. The row text starts at the row group's anchor point (`FUN_00139048` with the group-offset flag cleared by `FUN_00139888`). Layers: camp layouts 0, 1, 2 are the enter, hold and leave phases (groups 12/39/110, 13/30/40/111, 14/41/112). Panel outline groups 0x2e/0x2f/0x30 (`FUN_00250a00`), Munny and Time plates 0x6b/0x6d/0x6c and 0x71/0x73/0x72, digit groups 0x33+d (Munny) and 0x3d+d with colon 0x47 (Time), NEW tag groups 0x31/0x32 (`DAT_003807e8/ec`), hand group 0x19 (`FUN_00253f98`), glint 0x1b/0x1d/0x1c.
- **Measured from clip 10, not read [F]**: the fit of footage pixels to menu units (x_px = 190 + 1.133 x, y_px = 26.5 + 1.087 (y + 16) at 960x540, which also shows the HUD's 16 line offset applies), row label offset from the anchor (-7, 1.5), the hand and glint offsets from the row anchor, area name, help line and digit placement, party card shapes and colours, the 2 frames per tick pace (replaced by the code's 1 per tick in WP-MN1, 1.6.1), the dim (0.3 brightness, in gamma space), the 14-frame close.
- **Not done / gaps**: the party card widget (`FUN_00295bc0` object vtable +0x18, `FUN_0023b348`..`FUN_0023b5d0`) was not decoded, so the card is drawn from shapes; NEW tags (`FUN_00293ba0`, `FUN_00294b58`.. decide them) are not drawn; the party is Sora alone; sub-pages beep (refused sound). The deselect group switch of the row the cursor leaves is [I].
- **Harness**: `KH2_KEYS` knows Enter, Escape, Backspace; scripted keys are pressed in `PreUpdate`. `KH2_MENU_FLAGS=0x70,0x72` replaces the story-flag set (default 0x70..0x73). Log lines: `menu: open`, `menu: page 0 cursor N`, `menu: close`.

### 9.1 The frame at wide windows (WP-MN1, user request 2026-10-12) [inferred rule, F look]

The original draws the camp on the 512 x 448 canvas (a 4:3 picture); the app's menu camera fits that canvas into the window
(`AutoMin`), so at 16:9 the frame stopped short of the window's sides. The HD remaster's frames (the user's references) show the frame
parts at the screen's edges and corners. Rule (`menu_gfx::stretch_quads`, applied to the layout's layers, i.e. the frame: the
backdrop group 12/13, the frame group 39/40 with the header band, the MENU pill bar, the side rails and the bottom panels, the
orange area rule 110/111, the rail glow 30): per quad, a side that lies (at the part's rest place) within 16 units of a canvas edge or
beyond it is carried out to the window's edge, when the quad's texture is drawn stretched along that axis (a band or a flat fill:
texture span at most half the quad's size; pictures such as the Mickey watermark and the rounded corner pieces keep their shape) and
the part reaches into the canvas on that axis (the rail glow, wholly left of the canvas, keeps its width). The part's rest place is
its enter group's last frame (layout 0) or its hold / leave group's first frame (layouts 1, 2); while it slides, the carried side moves
with it. Content (rows, cards, preview, text, cursor) stays on the 4:3 layout. The bottom panels end at y 420 of the canvas's 432 and
now reach the bottom edge. `KH2_MENU_NO_STRETCH=1` turns it off; `KH2_WINDOW=<w>x<h>` sets the window size (harness aid); log line
`menu: frame layer N rest box [..], window view [..]`. Test `menu_gfx::tests::frame_parts_reach_the_window_edges`.

Pictures (WP-MN2, user request 2026-10-12): the Mickey watermark behind the help text (two touching picture quads, canvas box
429..523 by 341..417) is not stretched but **translated**: a group of touching picture quads (texture span more than half the quad's
size along the axis) whose box at rest reaches within 16 units of one canvas edge of an axis, and not the opposite one, moves as a
whole by the distance of the window's edge to the canvas edge (`menu_gfx::picture_shifts`). It keeps its size and its distance to the
screen's corner (here it overhangs the right edge by 11 units, so it sits partly tucked into the corner, as the remaster's frames show
it); at 4:3 nothing moves. The group is judged as a whole so the two halves never tear apart; the slide of the part is followed like the
carried sides. The rounded cap pieces and everything mid-canvas stay (no group of them reaches an edge of its axis). Looked at: 1280x720,
1024x768 and `KH2_WINDOW=1600x600` (tt03, `KH2_NO_ENEMIES=1 KH2_KEYS=6:Enter,8:Enter,9.5:Enter`, `KH2_SHOT_AT=11`).

Bars of pieces (WP-JH1, user report 2026-10-16, `local/reports/journal-help-bar-2026-10-16.png`): the journal's help line is a bar of Reviewer finding (2026-10-16): the user's picture is a transient of the journal page 2 open animation (`KH2_SHOT_AT` 10.3 and 10.45 with keys `7:Enter,8:Down,8.2:Down,8.4:Down,8.6:Down,9.2:J,9.8:Down`), present at every window size tried (1280x720 up to 3440x1440, 1000x1000), not a wide-window break; the steady state was fine before. After the fix the transient frame shows one continuous bar at every size. The cause stays [inferred]; no original address was read.
quads in the layer group of the page frame (group 41, `FUN_00278468` draws the text on object group 0x7b): a left cap (box 47..61 x
387..409 at rest, picture), a flat body (61..488, texture flat along x), a right cap (488..502) and Jiminy's icon 5 units left of the left cap
(15.8..41.8 x 388..419, bobbing). `picture_shifts` judged each touching picture group alone: at a wide window the icon (its left side
within 16 units of the canvas edge once it bobs) alone moved to the window's left edge, leaving a cap and the body behind (the user's
crop); at a tall window (1000x1000) the caps, body and icon moved down by the bottom distance (32 units) while the text, a node of its own,
stayed. Rule now (`menu_gfx::bar_parts`, test `a_bar_of_pieces_is_not_torn_apart`): quads that are not flat on both axes and lie within
`BAR_GAP` (8 units) of each other form a chain; a chain with a quad flat along only one axis (the body) is a bar and its pictures are not
translated, so the bar and its text keep the canvas place [inferred: the HD remaster's rule for these bars is not known]. The Mickey
watermark (pictures only) still moves. Looked at: page 0xe at 1280x720, 1920x1080, 2560x1080, 1600x900, 1024x768, 1000x1000, 800x1000 and
3440x1440 (one continuous bar with the icon at its left end at every size), pages 0, 0xc (also 0xd), 9 and 0xf at 2560x1080 and 1000x1000.
Harness: `KH2_JOURNAL_QUADS=late` repeats the node dump every 240 frames, `KH2_JOURNAL_QUADS2=1` adds each quad's rest and drawn corner.
Seen and left: (the list pages' left sheet is carried to the window edge since WP-JR1); at 1000x1000 the right side of the camp's area name is clipped (the Mickey rule of 9.1).

