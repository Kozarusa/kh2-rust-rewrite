# Menus: the pause (camp) menu and its screens, implementation spec

[Contract](contract.md) | [Index](../../README.md)

Work package 7N. Research only: nothing here is implemented in the app yet. Like `docs/summons.md`, this note is
meant to let a later work package build the menu without reading the game code again. Function names are
addresses in the main executable (`SLUS_210.05`, USA, PS2). No game values are copied into the repository: every
table below is read from the user's disc at run time by the tests listed in section 0.

Marks: **[V]** read in the code or the data (address given) or checked by a disc test; **[I]** inferred (fits the
code, no line states it); **[F]** seen in the user's footage (a video frame, described as seen); **[GAP]** not found,
with what was searched (section 7).

**The footage is the HD remaster, not the PS2 disc.** The clips (`local/VIDEOS.md`: 5, 6, 7 and 10) show Kingdom
Hearts II Final Mix on PC: "Checking PC for save data", a "Y" button prompt where the PS2 game says Triangle, "NEW!" tags,
a Roxas prologue. The PS2 text on the user's disc differs (for example the save prompts say "memory card (PS2)",
message 0x37cc). Layout, element names and behaviour are the same family; where the disc and the footage differ the
disc wins, and the difference is called out.

## 0. Evidence that runs

- `cargo test -p kh2-image --test menu_resources -- --nocapture`: parses `menu/us/{pause,save,camp,jmemo,itemshop}.2ld`
  (IMGZ + LAYD, section 1.3), prints their images, layers, layouts and embedded sequences, checks the numbers section 1.3
  quotes, and checks the message text of every top-level entry. `KH2_MENU_DUMP=<dir>` also writes the whole system
  message table (`sys_messages.txt`, outside the repository) for reading.
- `cargo test -p kh2-image --test menu_tables -- --nocapture`: reads the executable and prints and checks the menu kind
  table (1.2), the top-level entry table with its story-flag conditions and the page each entry opens (2.1), the ten
  Config rows with the option-word bit each row's reader uses (2.8), and the menu sound table (1.7).
- `local/tools/mips_scan.py` (PLAYBOOK R1b) was used for call sites; the decompiler for the functions that had no
  `.c` file (R1a: `DecompileAt.java`; 12 of the menu's functions had to be created that way).

## 1. Entry, exit, files, input, sound

### 1.1 How the pause menu opens [V]

The field task `FUN_00138758` runs every frame. It opens the camp menu when all of these hold:

- `FUN_00138748` is false (`DAT_00354f70 == 0`), `DAT_0033e828 != 0`, `FUN_001039d0` is 0 (`DAT_0033e788`, no scene change
  is pending), `*(0x3564d0 + 0xfc) == 0` (`FUN_0014a918`), `DAT_003715c8 == 0` (`FUN_002f4c40`), and the pad is not locked
  (`DAT_0035b940 == 0`, `FUN_00168240`);
- the first pad (`FUN_001678e8(0)` = 0x35b7c0) has **logical bit 2** newly pressed. In the shared button table at 0x379698
  that bit is **physical bit 0x8, Start** [V].

Then: if `FUN_001bf030` finds a pending "start prompt" object (`FUN_001beff8`, or `FUN_001bef10(6)` while `FUN_00190a38`)
the save menu opens instead (`FUN_0024f330` -> menu kind 7, see 2.9); otherwise `FUN_0024f0b8(0, -1)` opens menu kind 0, the
camp. The player must be controllable by that gate, not by the menu code. Events open menus with `FUN_00197b70` (kinds 4 and
the shop events) and `FUN_0019d9d8` (`FUN_0024f300` = kind 5, the camp opened on a given page, see below).

`FUN_0024f0b8(kind, arg)`: finds the kind's record in the table at 0x380560 (`FUN_0024f250`), writes `arg` (unless -999) into
the kind's mode word (record `+0xc`; kind 0: value -1 means "default"), and requests the record's **scene** (`+2`) through
`FUN_001033a8` (`DAT_003b10d0`). `FUN_00103678` then creates the scene-change task (`FUN_001036b8`, priority 330000).
A menu is therefore a game **scene**, not an overlay: the field scene stops and the menu scene's init (`+4`) runs. Closing
returns to the field through `FUN_001038a0` (the saved return hook `DAT_0033e794`, else `DAT_0033e764 = 3`).

### 1.2 The menu kinds (table at 0x380560, 16 records, printed by `menu_tables`) [V]

| kind | scene | init / term | what it is |
|---|---|---|---|
| 0 | 0 | `FUN_0024faf0` / `FUN_0024ff68` | **the camp menu** (Items, Abilities, Customize, Party, Status, Config; opens at the top page). Loads `camp.2ld` |
| 5 | 1 | `FUN_00250450` / `FUN_00250498` | the camp opened at page 0x14 or 0x15 (the Party screen, by `FUN_0026ef80`); calls kind 0's init. Opened by `FUN_0024f300(a, b)` after `FUN_0026ef70(b)` |
| 3 | 2 | `FUN_002756f8` / `FUN_00275bd8` | **Jiminy's Journal** (loads `jmemo.2ld`); the camp's Journal entry leaves the camp for this scene (section 20) |
| 1 | 3 | `FUN_00255598` / `FUN_00255608` | **the save points' save screen** (mode word 0x36cd28: 0 world point, 2 save point; section 18.4) |
| 2 | 4 | `FUN_002589f0` / `FUN_00258a68` | [I] title: load/save file screens (`FUN_00258b38` loads `save.2ld`, index 1) |
| 4 | 5 | `FUN_0025f168` / `FUN_0025f710` | **shops** and synthesis (opened by field events 0x13, 0x66..0x75, 0x83..0x85, 0x8a through `FUN_00197b70`; uses `itemshop.2ld`, index 4; section 21) |
| 6 | 6 | `FUN_0025f038` | [I] |
| 7 | 0x32 | `FUN_002540c8` / `FUN_00254210` | **the in-field save menu** wrapper; starts kind 8 as its page machine |
| 8 | -1 | `FUN_0026f420` / `FUN_0026f468` | the save pages (sub-menu of kind 7) |
| 9, 0xd, 0xa, 0xb, 0xc, 0xe, 0xf | 7, 8, 0x33, 0x34, 0x35, 9, 0x384 | various | not read (world map, mission and Gummi screens [I]) |

Page ids and the list widget are shared by all of them (`DAT_0036d1d4` = current page).

### 1.3 Files [V]

`FUN_0024f558` (menu boot) preloads `pause.2ld` (index 0) and `save.2ld` (index 1) into the buffer at 0x1dc0e00; the others
load on demand by index through `FUN_0024f908` (`FUN_00255258` waits for them): 2 `camp.2ld`, 3 `jmemo.2ld`, 4
`itemshop.2ld`, 5 `gm_ps.2ld`. The path format is `menu/%s/%s` (0x3806e8) with the language folder (`us`); names at
0x380670. Text comes from `msg/us/sys.bar` (message ids; bit 15 of a stored id is ignored, `FUN_00190c40`).

**1.3.1 What a `.2ld` is.** A BAR of two entries: type 29 `IMGZ` (the images) and type 28 `LAYD` (the layout). `save.2ld`
adds a second `LAYD` ("load", the load screen) and a type 37 entry ("kh2"). **IMGZ**: magic, 0x100, 0x10, count, then one
(offset, size) pair per image at 0x10; each image is an ordinary IMGD (`kh2_image::imgd`). **LAYD**: magic, 0x100, then
three (count, offset) pairs, all u32: **layers** (0x18 bytes: kind, SEQD index, animation group, 0, x, y), **layouts** (0x14
bytes: u16 first layer, u16 layer count, zeros) and **SEQD offsets** (each SEQD runs to the next offset). A SEQD is the
ordinary `kh2_image::seqd` sequence (sprites, parts, sprite groups, animations, animation groups); the animation groups are
the drawable things. The menu code never names a pixel: it names an animation group (`0x84`, ...) and the SEQD plays it.
The images are 8-bit grey-white art tinted by the SEQD's vertex colours (the camp image shows the plate shapes, the digits
0..9 and `:`, "NEW!", "AP", "L2"/"R2" key caps, the "MUNNY"/"TIME"/"MENU" word art, a speech bubble and the castle
silhouette; the camp image has a Mickey-head ring that is the footage's bottom-right watermark).

| file | images | sequences (animation groups) | layers / layouts |
|---|---|---|---|
| `pause.2ld` | 1 (64x64) | 1 (19) | 1 / 1 |
| `camp.2ld` | 1 (256x256) | 2: bars (7) and everything else (351) | 10 / 3 |
| `save.2ld` | 3 (256x256, 512x256, 256x256) | 3 (88, 9, 21) | 10 / 3, plus the "load" LAYD |
| `jmemo.2ld` | 1 (256x256) | 6 (7, 4, 4, 4, 4, 155) | 6 / 6 |
| `itemshop.2ld` | 2 (256x256) | 3 (3, 3, 257) | 68 / 18 |

(printed by `menu_resources`). `camp.2ld` sequence 0 groups 1..3 are the 512x381 main panel (enter by sliding in from the right over 15 frames, a 30-frame hold, leave
16 frames) and 4..6 the 512x34 bottom bar at y 381 (the same three phases, sliding from the left): a 512x448 canvas, the canvas the HUD uses
(`hud_gfx::SCREEN`) [V: the group frame ranges and bounding boxes printed by the `seqdump` example].
Sequence 1 holds the rows: group `0x85` is the dark idle row plate, `0x84` the red-edged **selected** row plate, `0x86` another
dark plate, `0x87` draws nothing at frame 10 (the pointer's anchor) [V: rendered with the `seqdump` example]. Group 0 is a wide
header strip, groups 12 and 13 full-screen panels (frames up to 1001).

### 1.4 The camp's main loop [V]

Init `FUN_0024faf0`: loads `camp.2ld` (`FUN_0024f738(2)`, `FUN_00255258(2)`), sets layout ids `FUN_00295918(0x26, 3)`, builds the
widgets (a party object `DAT_0036ce90` for the four party slots, three list widgets `DAT_0036ce9c/cea0/cea4`, a help text
window `DAT_0036cecc`, a layer object `DAT_0036cebc`, and the page modules: items, abilities, customize, party, status,
config, shop-like and help modules, each with its own init and term: 0x293688, 0x25a968, 0x268d68, 0x26d1b0, 0x265b30,
0x2703f0, 0x2787c8, 0x27dd28, 0x291e10, 0x2a0348, 0x282e18, 0x27f498, ...; the terms run from `FUN_0024ff68`). It registers the task
`FUN_00250590` (priority 2000).

Task `FUN_00250590`: waits for `camp.2ld`, builds the layers (`FUN_002506b8`: the "MENU" banner layer, the Munny/Time panel,
the area name, the help bar), plays the **open sound** (`FUN_0023e738(0xb,
1)`, 1.7), then loops until `FUN_002514d0` returns 1: each frame `FUN_002514d0` runs the **page handler** for the current page
(a switch on `FUN_002959c0` = `DAT_0036d1d4`) and `FUN_00251d38` draws.

| page | handler | screen |
|---|---|---|
| 0 | `FUN_00251848` | **top list** (2.1) |
| 1 | `FUN_0025ae18` | **Items**: character tabs + Stock (2.2) |
| 2 | `FUN_00269580` | Items: the chosen character's categories (Weapons, Armor, Accessories, Items) |
| 3, 4 | `FUN_0026a960`, `FUN_0026ab10` | Items: sub-prompts [I] |
| 5..9 | `FUN_0026a528` | Items: the item list of one category (page = 5 + category) |
| 0xa | `FUN_0025b6e0` | Items: Stock |
| 0xb..0xe | `FUN_0025bfa8`, `0x25c590`, `0x25c930`, `0x25cba0` | Items: use / target pickers [I] |
| 0xf | `FUN_002790e0` (entry `FUN_00278aa8`) | **Abilities**: character list (2.3) |
| 0x10 | `FUN_00279ac0` | Abilities: the character's abilities by group |
| 0x11..0x13 | `FUN_0027a578` | Abilities: the equip / don't-equip prompt (messages 0x3627, 0x3629) |
| 0x14, 0x15, 0x16 | `FUN_0026dbd8`, `0x26e158`, `0x26df80` | **Party** (2.5) |
| 0x17, 0x18 | `FUN_00266000`, `FUN_00266760` | **Status** (2.6) |
| 0x19 | `FUN_002707f8` | **Customize**: character list (2.4) |
| 0x1a, 0x1b | `FUN_00271078` | Customize: Sora's settings (0x1a), a companion's (0x1b, section 14b) |
| 0x1c | `FUN_002716d0` | Customize [I] |
| 0x1d..0x21 | `FUN_00272018` | Customize: the sub pages (0x1d shortcut command, 0x1e spell, 0x1f Auto-Reload, 0x20 Battle Style, 0x21 an ability's use setting) |
| 0x24 | `FUN_0027df50` | **Config** (2.8) |
| 0x25 | `FUN_002920d8` | the **Help** overlay (the tutorial pages, 2.7) |

Page 0 is entered with `FUN_00295a00`/`FUN_00295a30` (`DAT_0036d1dc` is the requested next page, copied to the current page
when the leave animation ends). `DAT_0036d1e0` is the page's phase: 0 selecting, 1 entering, 2 leaving; `FUN_00295a90`
advances it. Each page remembers its cursor (`FUN_00252f98(page, index, scroll)`, read back by `FUN_00252fb8/00252fe8`).

### 1.5 Closing [V]

A list result of -2 (cancel) on page 0, or -4 on any page, calls `FUN_00295ba0` (`DAT_0036d1f0 = 1`, "closing"). `FUN_002514d0`
then waits for the pages' leave animations (`FUN_002a1578`, `FUN_00139fe0(DAT_0036cebc)`), runs `FUN_002529e8` and
`FUN_00250ec8` (free every widget, reset every page module) and `FUN_001038a0` (back to the field). Cancel on a deeper page
steps back one page (each handler sets the previous page with `FUN_00295a00` and plays the leave animation).

### 1.6 The open and close transition [F measured, V where noted]

Clip 10 (`10_menu_open_close.mp4`, 1920x1080, 60 frames per second, 81 frames, Sora with Donald and Goofy at the Underworld
Entrance, a Save prompt up). Frame numbers below are the clip's frame index at 60 Hz; the clip was analysed by region
brightness and looked at (`local/videoframes/10_menu_open_close_*.png` and a contact sheet of frames 21, 22, 24, 26, 30, 40, 56,
59, 62, 66).

**Open.** Frame 21 is the plain field with its HUD (command deck with "Save" at the left, party icons and Sora's face at
the right). **Frame 22 (t = 0.367 s): in one frame the whole picture, field and HUD, drops to about 30 % brightness** (mean
34.5 to 10.7) and stays dim: the field is frozen and dimmed, not replaced. Then, over about 9 frames:

- frames 23 to 31: the "MENU" banner and the list rows fade in (banner brightness 3 to 51 over frames 23..32; list 5 to 27);
- frames 24 to 32: the Munny/Time plate (bottom left) and the help bar (bottom, right of it) slide/fade in; frames 26 to 29: the
  area name (top right, "Underworld Entrance") fades in;
- frames 25 to 36: the party stands in front of the dimmed field with LV/HP/MP cards (Sora, Donald, Goofy) fading in;
- frame ~37 to 39 (0.62 s): the pointer hand and the red selected plate appear on the first row ("Items" in frame 40; frame 0.7 s
  shows the pointer on "Abilities" after the player pressed Down).

The dimmed field HUD (the command deck with "Save", the party icons) stays visible behind the menu, dimmed.

**Close.** The list's red highlight leaves at frame 54; the banner fades from frame 57 to about 66 (about 10 frames), the area
name between 59 and 62, and the field returns to full brightness linearly over frames 56 to 69 (14 frames, 0.23 s); the field HUD
is back at frame 70. There is no iris and no black frame.

[V] The code that corresponds: the sequence layer groups of `camp.2ld` sequence 0 (top bar enter 15 frames, leave 16; bottom bar
the same) and the page leave/enter animations (`DAT_0036d1e0` phases); the dim is a scene-level darkening started by the camp task
(`FUN_002a1448` starts the helper task `FUN_002a14d0` at priority 140000 and sets `DAT_0036d3d1`; `FUN_002a1578` asks whether
it has finished) [I: that this task is the dim; its brightness curve was not read, so the 30 % and the 14-frame return are the
footage's numbers]. The open sound is `FUN_0023e738(0xb, 1)` (1.7). **The sequence frames in the clip settle faster than the
group lengths** (the 15-frame bar finishes in about 8 clip frames): how the game steps a menu sequence (one frame per tick or
two) was not read; use the clip's timings. (Superseded by 1.6.1: the PS2 code steps one frame per tick; the remaster clip is faster.)

### 1.6.1 The menu's sequence step (WP-MN1) [V]

The 2D frame step is `FUN_002378a0`: frame `+0xfc` (and the group time `+0x100`) grows by `DAT_00356f44 * object +0x44` per call.
`+0x44` is the object's speed: `FUN_002341e8` writes 0x37f3c8 and `FUN_00236650` 0x37f404, both 1.0 on this disc, and no menu code
writes it again (`mips_scan --store-off 0x44` over 0x24f000..0x2a2000: only stack and u16 record stores). `DAT_00356f44` is the frame's
time step: `FUN_0014cdd0` (each frame, from `FUN_0014ccf8`) sets it to the vblank count since the last frame (`DAT_00356f38`, counted by
the vblank handler `FUN_0014cc88`) in the fixed mode (`DAT_00356f50 != 0`) or to 2.0 (0x379174) otherwise, capped at 6.0 (0x379178);
`DAT_00356f4c` is how many extra vblanks a frame waits (`FUN_0014cba0`). The boot (`FUN_0014cd38`) sets fixed mode with one extra vblank
(the field: 30 frames per second, step 2.0); **a menu scene's start `FUN_0024ef30` (and the other menu starter `FUN_0023ef58`) calls
`FUN_00239e78`, which saves the mode and sets fixed mode with no extra vblank: the menu runs 60 frames per second with step 1.0**;
`FUN_0024f028` / `FUN_0023f020` restore it (`FUN_00239ec0`). So a menu sequence advances **one frame per 60 Hz tick**. The 15-frame enter
groups therefore take about 15 ticks (0.25 s); the port's log measures the whole open phase (until every layer's enter group has
finished) at 17 to 18 ticks, against 9 at the earlier pace (2 frames per tick, measured on the remaster clip 10), which halved every menu
animation (open and close slides, row and cursor animations, the hand, the glint, the PAUSE band letters); `menu::PLAYER_SPEED` is now 1.0 (`KH2_MENU_SPEED` overrides it for comparisons). Log line `menu: open animation done after N
ticks (S sequence frames per 60 Hz tick)`. Test `menu::tests::menu_sequences_step_one_frame_per_tick` (reads the three constants).
[inferred] a menu frame takes one vblank (the menu draws little); a frame that took two would step 2.0.

### 1.7 Input and sound [V unless marked]

**Logical input.** Each pad has four 64-bit words (held, newly pressed, released, repeat; copied by `FUN_00336a60`) built by
`FUN_001676a0` from the physical bits through two tables of (physical mask -> logical bit): the shared table at 0x379698 plus
the active set (`FUN_00167908(n)` copies `DAT_0035b888[n]` into `DAT_0035b8ac`; set 1 = field 0x379768, 2 = 0x379988, 3 = 0x379ab8,
4 = 0x379af8, 5 = 0x379b88; set 1 is selected by the area init at 0x102a5c, set 2 at 0x103e20, set 4 by `FUN_00104280`). Shared
table: physical Start (0x8) -> logical 0x2; Up 0x10 or stick -> 0x10; Down -> 0x20; Left -> 0x40; Right -> 0x80; two synthetic
physical bits 0x01000000 and 0x02000000 (set by the pad layer, [I] the confirm and cancel buttons after the Cross/Circle
choice) -> logical 0x4 and 0x8. Which set the camp runs under was not found [GAP]; the results below are given by the
logical bit each tests.

**The list widget** `FUN_00297148` (every menu list; widget `+0x34` holds the allowed-button mask: pages set 0x73 when they enter
(`FUN_00298ce0(list, 0x73)`), OR in 0x100 where a screen switch exists and 0x400 where Help exists (`FUN_00298d50` reads it);
the mask for the shoulder results was not found being set [GAP]) returns one number per frame:

| result | logical bit tested (newly pressed word) | allowed by mask |
|---|---|---|
| index >= 0 | 0x4 (confirm) | 0x10 |
| -2 cancel | 0x8 | 0x20 |
| -4 | 0x100 | 0x40 |
| -5, -6, -7, -8, -9, -10, -11 | 0x800, 0x1000, 0x2000, 0x4000, 0x8000, 0x10000, 0x20000 | 0x100, 0x200, 0x400, 0x1000 (-8), 0x2000 (-9), 0x4000, 0x8000 |
| -12, -13 | 0x40 (Left) and 0x80 (Right) in the pressed word | 0x4, 0x8 |
| -1 | nothing; Up/Down moved the cursor | |

Up/Down (`FUN_002977a8`): the repeat word with the list's up/down bits moves one row; **a new press at the first or last row wraps,
a held repeat does not** [V]. Left/Right on the Config rows return -12/-13 (change value).
How the camp handlers use them: -2 steps back (page 0: closes), -4 closes the whole menu, -5 on Items goes to Abilities and on
Abilities to Items (the hint "X Items" / "X Abilities" in the footage), -7 opens the Help page 0x25 (the "SELECT button: Help"
hint, message 0x4bdd; `FUN_00292088(topic, returnPage)`), -10/-11 switch to the previous/next character (the "LT RT Switch" hint,
message 0x361a), -6 on Items starts Use/unequip. A page uses a result only if it handles it; other results do nothing.

**Sounds.** `FUN_00295bf0(result, moved, ...)` picks the sound kind and `FUN_0023e738(kind, play)` starts it through
`FUN_002be49c(2, id, 0, 0x3fff, 0x40)`: moved or Left/Right = kind 1, confirm = 2, cancel = 3, confirm on a disabled row (the page's
validity callback says no) and a refused -4 = 4, -4 = 5 (close) or 4; the **open** sound is kind 0xb. Kind -> id through the s16
table at 0x36cb48 and the u16 table at 0x37f654 (printed by `menu_tables`): cursor 1, confirm 2, cancel 4, refused 5, close 2,
open 3. These are ids of channel 2 of the sound driver; their mapping to the app's `PlaySe` numbers was not traced [GAP:
`audio.rs` has `PlaySe::number(n, apdx)` for the area banks; the system bank of channel 2 is not loaded by the app].

### 1.8 The freeze, and the combat PAUSE band (WP-MP1)

**Freeze.** The original stops the field by a scene change: `FUN_00138758` -> `FUN_0024f0b8` -> `FUN_001033a8` makes the camp a game scene,
so no field task (actors, camera, clocks, spawns, hit processing) runs while it is open, and the sound driver (the BGM) is not a field task
[V for the structure, 1.1; which task flag the field tasks poll was not traced]. The app has no scene change: `schedule::Pause`
(`menu`, `band`) is set by `menu::input` in `PreUpdate`, the frame of the open input and before the 60 Hz sets and the `Update` systems of
that frame; `schedule::sim_running` is the one run condition, put on the whole `Sim` chain (`SchedulePlugin`) and on the systems that live
outside the sets (camera `tick` and `gather_input`, `area_jump::tick`, `texanim::tick`, `party_hud`, `summon::stops`, `fx::weapon_appear`).
The Struggle clock, drive timers, the battle flag timers, the wave spawner, hit processing, effects and the area jump task are all in
those sets. `Pause::menu` clears when the close fade has finished (the world stays frozen during the 14 close frames, as before);
the next 60 Hz tick runs from the same state, no catch-up (the fixed clock keeps ticking, only its systems wait). The HUD, the menu and the
music run on. Proof: `KH2_PAUSELOG=1` prints `pauselog f<frame> frozen <bool> phase <phase> sora (x y z) camera (x y z) enemies [first]`
every 30 frames; in a tt04 `KH2_WAVE_GROUP=b_44` run with `KH2_KEYS=7:W:8,10:Enter,13:Enter` Sora (z 854 -> 894 -> 902) and the camera
(z 4.0 -> 4.4) move until the open, stay at (0 -10 902) / (0 2.4 4.4) for all 34 frozen lines, and move on after the close. Tests:
`schedule::tests::a_pause_stops_every_set_and_a_resume_runs_them_again`, `frozen_means_menu_or_band`.

**The PAUSE band.** The user's frame of the original (PCSX2) shows, when the pause button is pressed in combat, the scene frozen and
dimmed with a dark band across the middle, thin violet rules at its edges and PAUSE in orange-gold italic letters, no menu rows. The picture
is `menu/us/pause.2ld` (list index 0 of the menu boot, 1.3): one 64x64 image (the letters P A U S E stacked, a few sparkles) and one sequence
of 19 groups, one layer. Read from the file [V]: group 0 black at alpha 64/128 (the dim), 14 the band (black gradient to transparent at
both ends, violet rule top and bottom), 15 the five letters sliding in one after the other (frames 20 to 99) and holding, 11 and 12 a pill
that opens (red / blue), 2 to 5 and 16 to 18 white flash and black fades, 6 to 9 sparkles, 10 the anchor (256, 216). The port plays 0 (as
the scene dim) and 15 only. **Fix 2026-10-12 (user frame vs the original):** group 15 is the whole band (it carries its own dark body, rows
-15 to 15, violet rules at rows -17/-15 and 15/17, and the letters); group 14 is a taller band (rows -53 to 53) that the original does not show
with it, so it is no longer drawn (it caused the two extra rules at about 40% and 63% of the height). The HUD is hidden while the band is up
(`menu_pause::tick` switches the HUD camera, `hud::HudCamera`, off and back on at the close; the debug text stays). **Veil [GAP]:** the file's only
dim is group 0 (black, alpha 64 of 128, normal blend; group 1 is opaque black, 2 and 3 white flash / fade); no group holds a pale or additive veil, so
the hazy, washed-out look of the original is not in this file (a screen-copy effect of the original's draw code, not found) and the port keeps the
black half-alpha dim. Dump of all 19 groups: test `dump_pause_groups` (`-- --nocapture`). **Rule [GAP]:** `FUN_00138758` opens the camp (or the save menu when a start prompt object exists,
`FUN_001bf030`) whatever the battle state is; `FUN_0024fa20` (the buffer of the current `.2ld`) has two callers, `FUN_0024f908` (the
loader) and `FUN_002a04f8` (the list widget, called by the camp, the shop and the journal builders `FUN_002506b8`, `FUN_0025fcd8`,
`FUN_002761e8`); `0x1dc0e00` and `0x36cdc8` are used only by the loader family (`FUN_0024f558`, `FUN_0024f688`, `FUN_0024f7a8`,
`FUN_0024f898`, `FUN_0024fa20`, `FUN_0024fa38`); the scene records 0x380560 (kinds 0 to 0xf) were read for a pause scene (kind 0xf, scene
0x384, init `FUN_002f7b38`, is the landing-screen family, not it). So nothing that plays `pause.2ld` was found in the decompilation of this
US executable. [inferred] "in combat" = the game's battle flag (`audio_bgm::BattleFlag`, word 0x1c8a7d8, `FUN_001903d8`, docs/audio.md):
with it set, the open input shows the band (`menu_pause.rs`) and freezes the world like the menu; the same button or a cancel closes it
(no fade: the close groups 4, 5, 18 were not tried); the music keeps playing. Layout [inferred]: the dim is the scene dim at brightness 0.5,
the band's middle is the picture's middle line (menu y 208), the letters are centred in the band. Not played [GAP]: groups 2 to 13 and 16 to 18.
`KH2_NO_PAUSE_BAND=1` makes the menu open in combat (harness aid). Screenshots: tt04 `KH2_WAVE_GROUP=b_44`, `KH2_KEYS=7:W:8,10:Enter,13:Enter`,
`KH2_SHOT_AT=11` and `12.9` (the same frame twice; the letters finish sliding) against a run without the Enter keys (the world moved on).

**Reopen bug (play-test 2026-10-11, fixed).** After one open and close, the second open showed no row names, area name, help line, Munny or
Time numbers: the close fades those labels to alpha 0 (`label_fade`) and hides every node, and the open did not reset the fade, while the
digit refresh showed a digit only when its group changed. Reproduced at tt03 with `KH2_NO_ENEMIES=1 KH2_KEYS=6:Enter,8:Enter,9.5:Enter`,
`KH2_SHOT_AT=11` (before: plates and NEW tags only; after: all text). Tests `menu::tests::a_reopen_restores_the_label_fades`,
`the_refresh_shows_a_digit_whose_group_did_not_change`.

## 2. The screens

### 2.1 The top list (page 0) [V]

Eight possible rows in a table of 0x24-byte entries at **0x3808a8** (printed and checked by `menu_tables`):

| entry | label (message) | help line | row shown when | selecting it |
|---|---|---|---|---|
| 0 | Items (0x44b) | 0x370d "Equip weapons and armor, or use items in stock." | story flag 0x70 set | page 1 |
| 1 | Abilities (0x44d) | 0x370f "Equip Abilities." | any party member has an ability (first slot of the list, `FUN_00253d38`) | page 0xf |
| 2 | Customize (0x451) | 0x3713 "Change Character settings." | flag 0x72 | page 0x19 |
| 3 | Party (0x44e) | 0x3710 "Switch Party Members." | flag 0x71 | page 0x14 |
| 4 | Status (0x44f) | 0x3711 "Show Status." | always | page 0x17 |
| 5 | Journal (0x450) | 0x3712 "Read Jiminy's Journal." | flag 0x73 | leaves for kind 3 |
| 6 | Journal (0x4539) | 0x453a "Read Roxas's Journal." | flag 0x73 **clear** | leaves for kind 3 |
| 7 | Config (0x3617) | 0x3715 "Change Game settings." | always | page 0x24 |

Entry layout: +0..+7 and +8..+0xf are two sets of four u16 animation group ids (`0x85, 0x87, 0x86, -1` and `0x85, 0x87, 0x84,
-1`: idle plate, pointer, plate / idle, pointer, **selected plate**), +0x12 name, +0x14 help, +0x1c up to four flag ids that must
all be set, +0x20 up to four that must all be clear. A flag id is bit n of the word array at **0x33fdb0** (`FUN_001a5328`; these are
story/tutorial bits the new game clears in `FUN_00187d20`/`FUN_001b6870`, [GAP: the writers of 0x70..0x73: tutorial events]). The
bit mask of the shown rows is `FUN_00253d38`; `FUN_00253ec0` maps a list index to an entry. The jump table at 0x380794 maps entry to
page (`FUN_00251848` tail).

On the PS2 disc entry 6 replaces entry 5 while flag 0x73 is clear. In the footage (Roxas) the list is Items, Abilities, Customize,
Status, Journal, Config: Party is hidden (flag 0x71 clear) and "Journal" is entry 6 [F]. In the clip with Sora, Donald and Goofy
every row is present, "NEW!" tags sit on Items, Abilities, Customize, Status and Journal [F]; the tag is the camp image's "NEW!"
sprite; the code that decides it was not read [GAP].

Layers around the list [F + V]: the **MENU** banner and the area name (`msg/us/place.bin` through the world/area record,
`FUN_002506b8`), the character card(s), the Munny and Time plate (bottom left; the digits 0..9 and ":" are in the image; Munny from
the save block munny, `docs/progress.md`; Time is the play time), the help bar (bottom: the entry's help message while a row is
highlighted), the Mickey-head watermark (bottom right). Card text: `LV`, `HP`, `MP` rows per party member shown standing in front.

### 2.2 Items and equipment (pages 1, 2, 5..9, 0xa..0xe) [V for the writers; I for the flow]

Page 1 lists one row per party member (up to 4, `FUN_002829c0(DAT_0036ce90, i)`) and a last row **Stock** (message 0x455).
Choosing a member goes to page 2 (his categories); choosing Stock goes to page 0xa. The categories (label table 0x381a18, help
table above it): **Weapons** (0x45b: the main keyblade; for Sora the rows for the forms' sub-weapons follow: Valor, Wisdom, Master,
Final, labels at 0x381a38, helps 0x45e/0x460/0x462), **Armor** (0x463), **Accessories** (0x465), **Items** (0x467). The list page
for category c is page 5 + c (table 0x381970). Each page shows the character's slots on the left (the footage's "Weapons: Kingdom
Key, Armor: --, Accessories: --, Items: Potion"), the candidate list on the right with an item preview (picture and help line; the
footage's pot icon and "Restores 40% of the target's HP. An equippable item.").

**The writers** (the equipment is the party record of `docs/progress.md`; "give(id, 100, 0)" = `Progress::give(id, 100, false)` back to
the inventory, "take" = `FUN_001a1ba8(id, 100)` = `Progress::use_item(id, 1)`):

| function | change | effect |
|---|---|---|
| `FUN_0026c420(item)` | the selected member's main weapon, record `+0` | if different: give the old one back, write the new, take it out of the stock, then `FUN_002a1060(member object, 0, item, 0)` refreshes the live weapon |
| `FUN_0026c4d0(form, item)` | a form's sub-weapon (form record `+0`) | same swap on the form record; refreshes the live weapon when `form` is the current form (`FUN_00282df8`) |
| `FUN_0026c588(slot, item)` | armor slot, record `+0x14 + 2*slot` | give back the old, write, take the new; `item` 0 empties |
| `FUN_0026c610(slot, item)` | accessory slot, record `+0x24 + 2*slot` | same |
| `FUN_0026c698(slot, item)` | battle item slot, record `+0x34 + 2*slot` (reserved id `+0x44`) | gives the old back; writes the new, or, when the item is not carried (the row's flag byte `(row-1)*6 + 2` of the list table is 0), writes it into the reserved column and leaves the slot empty |
| `FUN_001a1ba8(id, to)` | take | `to == 100`: stock count - 1; else clear the member's matching battle item slot |

Which character is edited is page 1's cursor (`FUN_00252fb8(1)`). The **stock** can hold at most 99 of an item (`FUN_001a1768`:
the check in 4.3 of `docs/progress.md`). Using an item from the menu (the footage's "Use" and "Use on who?" prompts, messages
0x456, 0x3941, 0x3943, "All" 0x3944, "The item will be used on {}." 0x394e, "Throw away" 0x3942/0x394f) runs the item effect
`FUN_001a12a0` and takes one (`FUN_001a1c30`); the Stock page lists the stock with counts; some items are "Usable only from the Main
Menu" (the drive recovery and the stat boosts, messages 0x39d9..0x39e4: Power/Magic/Defense Boost +1, AP Boost +1 maximum AP) [V text;
the menu's pages and the boosts' effect code were read in WP-M3: section 13].

### 2.3 Abilities (pages 0xf, 0x10, 0x11..0x13) [V for the data and the writer; I for the AP rule]

Page 0xf lists the party members; page 0x10 shows the chosen member's abilities in **four groups: Action, Growth, Support,
Weapons** (labels 0x32ee, 0x32ef, 0x32f0, 0x32ed, table 0x382070). Rows are built into a working table `DAT_0036d110` (a header,
then 8 bytes per ability: the item id at `+0xc` of the row address, an equipped flag at `+0xe`, the slot index in the member's ability
list at `+0xf`, a "locked" byte at `+0x10`, a cost byte at `+0x11` printed through the format string at 0x382028 [I: the AP cost;
the AP value is shown as "AP" in the footage]). The **equip toggle** `FUN_0027d8b8(row, on)` writes the member's ability list
(`record + 0x54 + 2*slot`, `docs/progress.md`) as `(old & 0x7fff) | on << 15`: **bit 15 is the equipped flag**, exactly as
`docs/abilities.md` has it. Confirm opens page 0x11..0x13 with two options (messages 0x3627 "Equip" and 0x3629 "Don't Equip", helps
0x3628 and 0x362a, `FUN_0027a068`); the row's group (`FUN_0027c6f8`) selects the page.

The footage's AP bar ("AP 0/2", then 2/2 after equipping; "AP 4/5" with a bar) and the rule that an ability needs AP free are
[F]; where the menu compares the cost with the character's maximum AP (record `+8`; `lvup` AP) was not found in the handlers read
[GAP: searched `FUN_0027d960`, `FUN_0027d708`, `FUN_0027c780`, `FUN_0027c240`, `FUN_0027dc08`]. Port the rule from the footage and
`docs/abilities.md` (`ap_used()` = sum of the equipped abilities' costs <= maximum AP) and mark it [I]. A message 0x3372 "There are
no Abilities." is shown for an empty list. Sora's list in the footage shows "Aerial Recovery" with an equipped marker.

### 2.4 Customize (pages 0x19, 0x1a..0x21) [V for the writers; I for the flow]

Page 0x19 lists the characters; the character pages let the player (a) choose which command each of the four **shortcut slots** holds
and (b) set per-item and per-spell options. **Shortcuts**: `FUN_00274dd0` writes the chosen row's command into the u16 at
**save block `+0x2600 + 2*slot` (0x340e60 + 2*slot, the "4 command shortcuts" of `docs/progress.md`)**; the command of an item comes
from `FUN_001a1f78` (`docs/progress.md`); the slot being edited is the page's remembered index `FUN_00252fe8(0x1a)`. **Spell
switches**: `FUN_00274e50` toggles bit `item.+4` of the per-spell switch word at **save `+0x264c` (0x340eac)** with `FUN_001a4b68`
(set) or `FUN_001a4b30` (clear) after reading it with `FUN_001a4bb8`, then updates the row's on/off marker; the labels "Use" (0x456) and
"Not Used" (0x458 / 0x485 / 0x3616) are the two states. Other messages of the page: "Auto-Reload" 0x482, "MP Charge" 0x481, "Select
item." 0x459. In the footage the tutorial pages teach "Customize -> select the character -> Select an item -> Select Auto-Reload ON;
the Y shortcut toggles Auto-Reload" [F].

### 2.5 Party (pages 0x14, 0x15, 0x16) [partly V]

Lists the party and the members that can join; the footage's Party entry is present with Sora, Donald and Goofy. Messages: "Select Party
Member." 0x46b, "Switch Party Member." 0x46c, "Party Members have been switched." 0x46d, "Party In" 0x46e, "Party Out" 0x46f, "Goofy's
equipment has been put in stock." 0x470 (a removed member's equipment goes back to the stock), per-world refusals ("Auron cannot be
switched at this time." 0x473.., "Party Members cannot be switched and Summons cannot be used until the fight is settled." 0x476).
The state it changes is the party arrangement: the four slot bytes of the world's record (docs/party.md, "Party presets and the slot
bytes"; `FUN_001a75e8` swaps two of them). **Read and ported in WP-MN7: section 19** (pages 0x14 and 0x16, their lists, the columns, the
3D models and the swap; the equipment return remains a gap, 19.4; page 0x15 in WP-PT1).

### 2.5b Journal [V: section 20]

Entries 5 and 6 of the top list leave the camp for menu kind 3 with `jmemo.2ld`: the handler at 0x251c38 (jump table 0x380794)
writes the mode word 0x36cd34 (`FUN_0024f4d0`: 1 for entry 5, 2 for entry 6), queues the scenes 2 then 0 (`FUN_00296458(2, 0)`)
and closes the camp. The scene itself: section 20 (WP-MN9).

### 2.6 Status (pages 0x17, 0x18) [V, read in WP-M2: see section 10]

Page 0x17 (`FUN_00265c48` builds it): one row per tab (the bit mask at `DAT_0036cfe8 + 0x18`, up to nine tabs) with the numbers of
the highlighted tab previewed beside it; page 0x18 (`FUN_00266328`) shows the chosen tab's rows across the screen, two columns of
ten, with a help text per row. The numbers come from the status block `FUN_0019f920` builds out of the member record
(`docs/progress.md`: HP `+4`/`+5`, MP `+6`/`+7`, AP `+8`, strength, magic, defense `+9..+0xb`, level `+0xf`), the `lvup` row and the
equipment's `item_stat` additions (`docs/abilities.md`). The rows, their labels, helps and number sources are in section 10.

### 2.7 Help overlay (page 0x25) [V structure; text F]

Select (-7) on a page opens page 0x25 with a topic (`FUN_00292088(topic, returnPage)`; topics seen: 0 = Items 0x3373-style, 2 =
Abilities, 3 = Customize) and the **tutorial pages** the footage shows ("What is Auto-Reload? PAGE 2/4", "What are Abilities? PAGE 3/4 and
4/4", "How to equip Abilities", the "This shortcut will help" strip). The pages are `menu/us/*.bar` resources (`ab_010_*.bar`,
`at_010_*.bar`, `mp_010_*.bar`, `sw_010_*.bar`; "ab" abilities, "at" auto-reload, "mp" MP charge, "sw" switch; `_o/_r/_s` = pages) and
`jiminy.bar`; their format was not decoded [GAP].

### 2.8 Config (page 0x24) [V]

A list of ten rows (nine visible at a time, `FUN_00298fe0`/`FUN_00299010`); each row has 1..3 values; Left/Right (-12/-13,
`FUN_0027f0d0`) change the highlighted row's value with wraparound; the help line under the list shows the current value's help.
Rows (table at **0x382158**, 16 bytes per row: value count, label, three value labels, three help lines; printed by
`menu_tables`), the **option word** is the u32 at **save `+0x31b0`** (0x33fa10; `FUN_001057a8` returns the save block 0x33e860);
`FUN_0027e1f8` reads the rows on entry with the readers shown, `FUN_0027e398` writes them back when the page is left (cancel or close):

| row | label | values (UI index 0, 1, 2) | option word bits: reader `FUN_`; **UI index** the page keeps (from `FUN_0027e1f8`) |
|---|---|---|---|
| 0 | Field Camera | Auto, Manual | bit 4 (`0023ec68`, reads the inverse); UI index = bit 4. Also: the page never leaves rows 0 and 1 both at UI index 1 (changing one to 1 sets the other to 0, `FUN_0027f0d0`) |
| 1 | Right Analog Stick | Camera, Command Menu | bit 5 (`0023ece0`, reads the inverse); UI index = bit 5; the same exclusion |
| 2 | Camera Up/Down | Original, Reverse | bit 8 (`0023ee68`); UI index = bit 8 |
| 3 | Camera Left/Right | Original, Reverse | bit 7 (`0023edf8`); UI index = bit 7 |
| 4 | **Summon Effects** | Auto, Full, None | **bits 9..10** (`0023eed8`): raw 0 = None (UI 2), 1 = Full (UI 1), 2 = Auto (UI 0), 3 reads as Full; written back raw = 0 for UI 2, 2 for UI 0, 1 for UI 1 (the arrival option `FUN_001c65f0` reads, `docs/summons.md`) |
| 5 | Navigational Map | ON, OFF | bit 3 (`0023ebf8`); UI index = NOT bit 3 (bit set = ON) |
| 6 | Vibration | ON, OFF | bit 0 (`0023eb10`); UI index = NOT bit 0 (bit set = ON); changing it calls `FUN_00239d28` (to ON) or `FUN_00239d38` (to OFF) at once |
| 7 | Sound | Stereo, Pro Logic II, Mono | bits 1..2 (`0023eb78`): raw 0 Mono (UI 2), 2 Pro Logic II (UI 1), 1 Stereo (UI 0); changing it calls `FUN_002bddf4(raw)` at once |
| 8 | Command Menu | Kingdom Hearts <heart> (themed per world), Kingdom Hearts (original) | bit 6 (`0023ed80`, reads the inverse); UI index = bit 6 |
| 9 | Game Mode | Beginner, Standard, Proud (read-only text: "cannot be changed once the game has started") | the u32 at save `+0x1658` (`FUN_00105750`); not written by this page |

The UI-index column is derived from the loader (`FUN_0027e1f8`) applied to the reader; `FUN_0027e398` stores through the setters
(`FUN_0023ec98`, `ed10`, `ee90`, `ee20`, `ef00`, `ec20`, `eb38`, `eba0`, `edb0`; the bits they write were not each read, [I] the inverse of
the readers): a port must round-trip, so that the reader returns what the page showed (the test prints each reader's shift, mask and
inversion). The labels are text messages 0x3717..0x373d and 0x42f5..0x4305 (the PS2 rows; the footage's PC build
has other rows).

### 2.9 Save (menu kinds 7 and 8, `save.2ld`) [partly V]

Opened by the Save command (field reaction "Save", `docs/events.md`, `FUN_0024f330`) or Start while a save prompt is pending (1.1). Kind
7 sets up a page machine in kind 8 (`FUN_0026f480`); the pages use `save.2ld` (images 256x256, 512x256, 256x256; layouts for the SAVE
header, the file rows, the information box; the "load" LAYD) and the type 37 entry. Footage (clip 7, remaster) [F]: green theme, a pointer
list ("Save", "Title Screen"), an information box ("INFORMATION: Checking ... Do not turn off the power"), a file list with four rows
(file number, character icon, LV, Munny, play time, location name, or "Save new file."), a question box ("Overwrite existing file?" with
Yes/No), the help line "Save data to storage device." / "Select file to save." and the button hint "Y Delete file". The PS2 disc text:
0x3858 "Save data.", 0x3859 "Jump to World Map?", 0x385a "Save data in MEMORY CARD slot {}.", 0x385b "Load data from MEMORY CARD slot {}.",
0x37cc..0x37da and 0x385d memory card information messages. The save file format and the memory card calls are out of scope
(the port saves to a file of its own). The page logic of kinds 7 and 8 was read in WP-M8: section 18.

## 3. The footage, element by element (what the spec must reproduce)

| clip | what is on screen | source in the game |
|---|---|---|
| 5, 6, 10 | "MENU" banner top left (tilted word art), area name top right with a thin line, a dark panel strip | layers of camp.2ld (sequence 0 bars; sequence 1 header groups), `FUN_002506b8` |
| 5, 6, 10 | the list: dark plates with labels; "NEW!" tags; the red-edged **selected** plate and a white hand pointer to its left | entry table 0x3808a8 (2.1), groups 0x84/0x85; pointer = group 0x87 [I] |
| 5, 10 | character cards: name tab, "LV n", "HP cur/max" (green), "MP cur/max" (blue-green) under each standing party member | page 0 (`FUN_00251100` builds the 8 row slots; the cards' code not read [GAP]) |
| 5, 6, 10 | Munny and Time (bottom left, orange "MUNNY", cyan "TIME", yellow/cyan digits) | Munny plate layer; digits from the camp image |
| 6 | help bar at the bottom (one or two lines) | `FUN_00252c08`, `FUN_0029d820/0029d860`; messages 0x370d.. |
| 6 | top right hints: "Help" (Select) "LT RT Switch" "X Items" (or "X Abilities") | messages 0x4bdd "SELECT button:Help", 0x361a, 0x3373 "[icon 36]Items", 0x3619 "[icon 36]Abilities" |
| 6 | Abilities: character tab "Roxas", group tabs, rows with AP cost, scroll bar, AP bar "AP 0/2" | 2.3 |
| 6 | Items: tabs "Roxas"/"Stock"; slots Weapons/Armor/Accessories/Items; item preview pot and text | 2.2 |
| 5 | tutorial pages (page n/4, picture, text, "Learn how to equip Abilities!" button, the shortcut hint box) | 2.7 |
| 7 | green SAVE variants of the same shell | 2.9 |
| 10 | open: field dims in one frame, then panels fade in; close: panels fade out, field lights back in about 14 frames | 1.6 |

Differences between the remaster footage and the PS2 disc to expect: the green/blue colour theme of the shell per mode (the camp is blue,
the save shell green) is a per-mode colour set (the layer tint; `FUN_0029e9d8(..., 0x380898, 0x3807f8)` takes colour tables);
PS2 shows Triangle/Circle/Cross glyphs where the footage shows Y/B/A; PS2 has "Config" with the vibration and sound rows, the
remaster's rows differ; the PS2 save text is memory-card text.

## 4. Where it goes in the app

The app's HUD stack is `hud.rs` with `hud_gfx.rs` (the 512x448 canvas, `SeqFile::from_bar` for `.2dd` files, `HudScene`/`Node`),
`hud_text.rs`, `hud_msgwin.rs` (message window), `hud_menu.rs` (the field command deck, not this menu); the state is
`progress::Progress` (`docs/progress.md`); plugins go in the third `add_plugins` tuple of `main.rs` (PLAYBOOK R9). The menu stays out of
those files (the HUD and fx fixes edit them): new files only, one small hook each in `main.rs`, `harness.rs`, `progress.rs`.

| file (new unless noted) | contents |
|---|---|
| `crates/kh2-image/src/layd.rs` + `pub mod layd;` in `lib.rs` | the `.2ld` reader of 1.3.1: `Layd { layers, layouts, seqs }`, `split_imgz` (the offset-table form); tests move from `menu_resources.rs` |
| `crates/kh2-app/src/menu.rs` | `MenuPlugin`: resource `Menu { open, page, phase, cursors, fade }`, the page state machine of 1.4 (page ids, phases, remembered cursors, results of 1.7), the open/close transition of 1.6; run condition for the field systems |
| `crates/kh2-app/src/menu_gfx.rs` | loading `camp.2ld`: IMGZ -> Bevy images, SEQD -> `hud_gfx::SeqFile`; drawing a layer = a node playing animation group g of sequence s at (x, y) on the HUD canvas (render layer 7). Reuses `HudScene`/`Node` if the HUD fixes leave them `pub(crate)`; otherwise its own small copy of the draw path |
| `crates/kh2-app/src/menu_pages.rs` (+ `menu_items.rs`, `menu_abilities.rs`, `menu_config.rs`, ... per stage) | each page's rows and result handling (sections 2.1..2.9) |
| `crates/kh2-app/src/menu_text.rs` | messages for rows and help: `kh2_image::text::Messages` of `msg/us/sys.bar` through the existing `hud_text` layout (`text::layout`) |
| `crates/kh2-app/src/progress.rs` (outside this WP; the stage that needs it adds) | setters mirroring 2.2: `equip_weapon(member, item)`, `equip_form_weapon(form, item)`, `equip_armor/accessory/item(member, slot, item)`, `set_ability_equipped(member, slot, on)`, `shortcut(slot)/set_shortcut`, `set_spell_switch(slot, on)` (exists as accessors), `option_word()/set_option_word()` (a u32 at save `+0x31b0`) |
| `crates/kh2-app/src/main.rs` | `menu::MenuPlugin` into the third tuple |
| `crates/kh2-app/src/harness.rs` | key names for the menu (Start, confirm, cancel, the shoulder keys, Select) in `KH2_KEYS` |
| `crates/kh2-app/src/sora.rs` or the pad reader | the pause: while `Menu.open`, no `Pad` input reaches Sora and the `Sim` sets stop (done as a run condition on the `Sim` sets, `schedule::Pause`, 1.8) |

Keys (proposal, [I]): Enter = Start, J = confirm (the app's attack key), K or Backspace = cancel, `[` and `]` = the shoulder switch (already
`pad.prev/next`), V = Select/Help (already Select in `camera.md`), arrow keys = the list, arrows left/right = Config values; the harness takes the
same names. The in-game fixed 60 Hz tick is `data::LOGIC_HZ`; menu animation runs in `Update` on real frames (the field is stopped).

## 5. Staged plan

Each stage is file-disjoint from the HUD and fx fixes (only new `menu*.rs`, `layd.rs` and one-line hooks). Acceptance for a stage is run
with the harness (`KH2_KEYS`, `KH2_SCREENSHOT`, PLAYBOOK R5); a harness key opens the menu without the pad. `Progress` for the checks is
`KH2_STATE` (docs/progress.md).

**M1: open, close, the top page (2.1) and the transition (1.6).** Files: `layd.rs`, `menu.rs`, `menu_gfx.rs`, `menu_pages.rs` (page 0 only),
`menu_text.rs`, the hooks (`main.rs`, `harness.rs`, the pause hook). Port: the `.2ld` reader; the Start gate of 1.1 (the conditions the app has:
player controllable, no event); the field freeze; the open dim (one frame to 0.3 brightness) and the 14-frame close fade; the banner, area
name, Munny/Time plate, help bar, party cards (name, LV, HP, MP from `Progress`/the app's stats), the list with the flags of 2.1 (default flags set:
the app's baseline) and the pointer; Up/Down with wrap on a new press; Cancel and Start close; the open sound. Acceptance: `KH2_KEYS=2:Enter` and
`KH2_SCREENSHOT` at 3 s shows the list with the red plate on Items, the cards and the help line "Equip weapons and armor, or use items in stock."
(LOOK at it against clip 10 frame 40 and clip 6 at 0.0 s); `KH2_KEYS=2:Enter,3:Down,3.4:Down,4:Escape` log lines `menu: open`, `menu: page 0 cursor 2`,
`menu: close`; a test that the entry flags of 2.1 hide Party with flag 0x71 clear. Not in M1: any sub-page (rows just beep: the refused sound).

**M2: Status (2.6).** `menu_status.rs`. Port the character list and the numbers screen (strength, magic, defense, AP, HP/MP, level, EXP to next from
`lvup`); L/R switch of character. Acceptance: screenshot of Status for Sora; a unit test of the number sources against `Progress` and `lvup`;
mark labels the doc lists as [GAP] explicitly.

**M3: Items and Customize (2.2, 2.4).** `menu_items.rs`, `menu_customize.rs`, `progress.rs` setters for `FUN_001a1ba8` / use / shortcuts / spell
switches. Port: Stock list with counts, Use on a member (the effect `FUN_001a12a0` through `Progress`), Throw away; Customize shortcuts at save
`+0x2600`, the spell switches at `+0x264c`. Acceptance: tests of the setters against the oracle (kh2-ee runs `FUN_001a1a18`/`1c30`/`1ba8` on the
block, as `progress::tests` does); a screenshot of the Stock page and of Customize; a harness run that sets a shortcut and a log line showing the
shortcut word.

**M4: Equipment (2.2).** `menu_equip.rs`. Port the five writers of 2.2 on `Progress`; refresh the live weapon (`sora`/`abilities`) after a change
(`FUN_002a1060` equivalent: `Progress::equipment()` is already read by `abilities.rs`). Acceptance: tests that swap weapon/armor/accessory and keep stock
counts (oracle: `FUN_0026c420`, `c588`, `c610` run on a block); a screenshot of the Armor list; a run showing the Kingdom Key's ability still applies
after re-equipping.

**M5: Abilities (2.3).** `menu_abilities.rs`. Port the four groups, the equip toggle (bit 15) and the Equip/Don't Equip prompt, the AP bar and the
AP rule marked [I]; the app's `Abilities` resource reacts to `Progress` already. Acceptance: tests that toggling sets/clears bit 15 of the right slot and
that over-cost equipping is refused; a screenshot matching clip 6 at 4.4 s; a harness run `KH2_KEYS` that equips an ability and a log line
`abilities: ... equipped`.

**M6: Config (2.8).** `menu_config.rs`. Port the ten rows over the option word with exact round trip; apply what the app already has: Summon Effects
(for a later summon WP: the bits are stored), Field Camera and the camera rows (`camera.rs` reads them), Navigational Map, Command Menu theme (`hud_menu`),
Sound/Vibration stored only. Acceptance: a test that every row's value survives a write/read through the option word (and equals the reader of
`menu_tables`); a screenshot of the page; `KH2_STATE` option word restored by the page.

**M7 (optional): Party (2.5), Help (2.7), Save (2.9).** After the above; each needs its own reading first (the gaps).

## 6. What the port must keep exactly

- Page ids and which result moves where (1.4, 1.7); the top list's flag rules (2.1); wrap only on a new press (1.7).
- The writers of 2.2, 2.3, 2.4 (what is written where, bit 15 of an ability slot, the shortcut word, the switch word, the option word bits of 2.8).
- No invented behaviour: items the footage shows but the code was not read for (the AP rule, the NEW tags, the Status fields, the Party page flow)
  are implemented only when read, or implemented conservatively and logged as `[I]`.

## 7. Gaps, with what was searched

- **Which button set the camp runs under, and the physical/logical meaning of the synthetic bits 0x01000000/0x02000000.** Searched: the six sets at
  0x379768..0x379b88 (`menu_tables` does not print them), the nine callers of `FUN_00167908` (area init 0x102a5c: set 1; 0x103e20: set 2;
  `FUN_00104280`: set 4; the rest in overlays), `FUN_001676a0`/`FUN_00167660`/`FUN_00167738`. The pad layer that fills the device word (0x166xxx)
  was not read.
- **The AP rule of the Abilities screen** (2.3): the handlers read do not compare costs. Searched `FUN_0027d960`, `0027d708`, `0027c780`, `0027c240`,
  `0027dc08`, `0027c6f8`, `0027a068`.
- **The dim and the transition curves** (1.6): taken from the footage; the game's helper task `0x2a14d0` and the sequence step were not read.
- **The "NEW!" tags**, the cards' source fields and the Munny/Time plate code (page 0 layers): `FUN_00251100`, `FUN_002506b8` only skimmed.
- **Pages 3, 4, 0x16** and the Items category list code (page 2, 5..9): purposes inferred from callers and message tables. (Pages 0xa..0xe were read in WP-M3, section 13; 0x19..0x1f in section 14.)
- **Party (0x14, 0x16: read in WP-MN7, section 19; 0x15 in WP-PT1), the Help pages (0x25) and the tutorial `.bar` resources, Journal (page 0 read in WP-MN9, section 20), Save pages (kinds 7, 8):** not read beyond
  structure. (Status 0x17, 0x18 was read in WP-M2: section 10; its open gaps are listed there: the forms', summons' and members' tabs are not ported, the layout origin is [I].)
- **Menu kinds 1, 2, 4, 6, 9, 0xa..0xf:** named by inference only.
- **Where flags 0x70..0x73 are set** (tutorial/story bits at 0x33fdb0): not searched.
- **The system sound bank (channel 2) mapping to the app's `PlaySe` numbers.**

## 8. Decompile index

| address | what |
|---|---|
| 0x138758 | field task: the Start gate (1.1) |
| 0x24f0b8, 0x24f250, 0x24f2a0, 0x24f300, 0x24f330, 0x24f458 | open a menu kind; kind record lookup; kind lists; open camp on a page; save menu; leave |
| 0x380560 | menu kind table (1.2); 0x380550/0x377e40/0x377e50 scene lists |
| 0x24f558, 0x24f908, 0x24fa50, 0x255258, 0x24f738 | menu file loader (index -> `.2ld`), wait, free others |
| 0x24faf0, 0x24ff68, 0x250450, 0x250498, 0x250590 | camp init, term, init at a page, task |
| 0x2506b8, 0x251100, 0x251848, 0x2514d0, 0x251d38 | layers, page 0 build, page 0 handler, per-frame page dispatch, draw |
| 0x253d38, 0x253ec0, 0x3808a8, 0x380794 | top list visibility, index to entry, entry table, jump table |
| 0x25ae18, 0x25aac0, 0x269580, 0x26a528 | Items pages |
| 0x26c420, 0x26c4d0, 0x26c588, 0x26c610, 0x26c698, 0x1a1ba8, 0x1a18d8, 0x1a1768 | equipment writers, take, give, room |
| 0x278aa8, 0x279ac0, 0x27a578, 0x27d8b8, 0x27a068 | Abilities pages and the equip toggle |
| 0x2707f8, 0x271078, 0x272018, 0x274dd0, 0x274e50, 0x1a1f78, 0x1a4b30, 0x1a4b68, 0x1a4bb8 | Customize, shortcut write, spell switch |
| 0x265c48, 0x266000, 0x266760 | Status |
| 0x27df50, 0x27ddf0, 0x27e1f8, 0x27e398, 0x27f0d0, 0x382158, 0x23eb10..0x23eed8 | Config page, build, read rows, write rows, change value, row table, readers |
| 0x297148, 0x2977a8, 0x295bf0, 0x23e738, 0x23e828, 0x2be49c, 0x36cb48, 0x37f654 | list widget input, cursor move, sound kind, play, id table |
| 0x295918..0x295e40 | page state (`DAT_0036d1d4..0x36d200`) |
| 0x1678e8, 0x167908, 0x1676a0, 0x379698, 0x379768, 0x379988 | pad words, set selection, logical build, button tables |
| 0x1036b8, 0x103678, 0x1033a8, 0x1038a0 | scene change task, request, return |
| 0x25ae18, 0x25b6e0, 0x25bfa8, 0x25c590, 0x25c930, 0x25cba0 | Items pages 1, 0xa, 0xb, 0xc, 0xd, 0xe (section 13) |
| 0x25b2a0, 0x25bad0, 0x25c2c8, 0x25cae8, 0x25c878, 0x25dae8, 0x25ddc0, 0x25eb20 | their builders, the empty text, the help line, the cursor after a change |
| 0x25e598, 0x25e8a0, 0x25e928, 0x25e950, 0x25e978, 0x25e9e8, 0x25ea40, 0x25eab0, 0x25eba8, 0x25e838, 0x25e7a0 | stock row, usable type, can throw away, group item, single target, can use, use, prompt choice, validity, filter step, list builder switch |
| 0x283a70, 0x283810, 0x283a58, 0x2845a0, 0x2846f0, 0x284020, 0x284088, 0x283be0 | stock list builder, filter word, type test, sort, hidden items, cell texts, equipment list |
| 0x1a15a0, 0x1a14c8, 0x1a11a0, 0x1a12a0, 0x1a1fe0, 0x1a0ce0, 0x1a0d30, 0x1a0d80, 0x1a0dd0, 0x1a0ec0, 0x1a0ee0, 0x1a0f00, 0x1a0f20 | use check, use on member, effect, group items, the four boosts and their limit tests |
| 0x381060, 0x381068, 0x381090, 0x3823d0, 0x382450, 0x380a60, 0x36cf28, 0x380880, 0x380888 | usable types, single-target items, prompt rows, filter pairs, hidden items, type names, OK/Yes/No messages, information window groups |
| 0x2707f8, 0x271078, 0x270ae0, 0x272018, 0x2723c8, 0x273d30, 0x271688, 0x274c88, 0x274dd0, 0x274e50, 0x274f70, 0x275288, 0x272fc8, 0x272e68, 0x255368 | Customize pages, rows, sub pages, table builder, candidate list, question, writers, command to item, row text, icon |
| 0x381d40, 0x381ca0, 0x381cb0, 0x381cc0, 0x381cd0, 0x381d20, 0x381d30, 0x3809d0, 0x381bb4 | row kinds, row groups, tag groups, heading messages, shortcut labels, sub page rows, icon table, confirm jump table |
| 0x2556f0, 0x255598, 0x255608, 0x2557a8, 0x255c10, 0x255a10, 0x255d38, 0x255f80, 0x257880, 0x257ee8, 0x2580b8, 0x2580e8, 0x258138, 0x24f498, 0x380570, 0x380cf0, 0x36cd28 | menu kind 1 (the points' save screen): task, init/term, build, top list, page machine, pages 0/1/0xd/0x10/0x14, draw, mode word, record, row records (section 18.5) |
| 0x24f330, 0x254a18, 0x1ad9a0, 0x1bf030, 0x1beff8, 0x1bef10 | Save: open (menu kind 7), the request block, the save prompt callers (section 18) |
| 0x2540c8, 0x254210, 0x254380, 0x2543e0, 0x254548, 0x2544b8, 0x254800, 0x254a08 | kind 7: init, term, task, build, per frame, block copy, header group, next sub-kind |
| 0x26f420, 0x26f468, 0x26f4a8, 0x26f0d0, 0x26f2c8, 0x26f508, 0x26f698, 0x26f8a8, 0x26f948, 0x26fd20, 0x26fda0, 0x26ff98, 0x26fff8, 0x270098 | kind 8 (pointer list): init, term, task, objects, build, rows, plates, party card, per frame, page 0, page 1 |
| 0x2589f0, 0x258a68, 0x258bf0, 0x258cf8, 0x258eb0, 0x2590c0, 0x259288, 0x259648, 0x259728, 0x259b78, 0x259e88, 0x25a158, 0x25a1b8 | kind 2 (file screens): init, term, build, dispatch, pages 1, 2, 4/6, 0xa, 3, 0xb (file list), 0xf (question), 0x14, draw |
| 0x23e828, 0x36cb48, 0x37f654, 0x27f248, 0x27f310 | sound kind to id; kind 0xd (the info picture scene) |

