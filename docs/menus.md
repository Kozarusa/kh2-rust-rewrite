# Menus: the pause (camp) menu and its screens, implementation spec

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

## 9. M1 as built (WP-M1)

Code: `menu.rs` (state, input, freeze, transition), `menu_gfx.rs` (draw path and camera, a copy of the HUD's because `hud_gfx` is private), `menu_text.rs`, `menu_pages.rs` (page 0 table and flags), `kh2-image::layd` (the `.2ld` reader).

- **Closed gaps (addresses).** System sounds: `FUN_002be49c(2, id, ...)`: the 2 is the driver SLOT that holds `se/se000.seb` (docs/audio.md "System sounds": `FUN_002c0eb0` loads it at boot, `FUN_002c0da8` names the slot bank 0), not bank 2 (WP-SE2 correction; `se002.seb` is a two-entry area bank, so the earlier `PlaySe::number(2000 + id)` never found its data). The menu sounds are entries of `se000.seb`, request numbers `id` (log form `se 0:<id>`): cursor 1, confirm 2, cancel 4, refused 5, open 3 (`menu::menu_sound`; `FUN_002c2b70` at 0x2c2cd8 is the same call with a slot taken from a request). Row groups: each entry's set is (enter, leave or inactive, loop); `FUN_00296fb8` starts a selected row in set B[0] with B[2] as the follow-up, `FUN_00298808` (list inactive) switches rows to index 1, `FUN_002988e8` back to index 2. Row pitch is the group's `unk20` (`FUN_00296bd0` with `FUN_00237698`), 26. The row text starts at the row group's anchor point (`FUN_00139048` with the group-offset flag cleared by `FUN_00139888`). Layers: camp layouts 0, 1, 2 are the enter, hold and leave phases (groups 12/39/110, 13/30/40/111, 14/41/112). Panel outline groups 0x2e/0x2f/0x30 (`FUN_00250a00`), Munny and Time plates 0x6b/0x6d/0x6c and 0x71/0x73/0x72, digit groups 0x33+d (Munny) and 0x3d+d with colon 0x47 (Time), NEW tag groups 0x31/0x32 (`DAT_003807e8/ec`), hand group 0x19 (`FUN_00253f98`), glint 0x1b/0x1d/0x1c.
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

## 10. Status as built (WP-M2)

Code: `menu_status.rs` (the number rules, the tables read from the executable, the two pages), `kh2-image/tests/menu_status.rs` (disc tests
that print the tables and the groups), hooks in `menu.rs` (the plugin, the Status row opens page 0x17, a sub-page's keys, `page0_nodes`),
`harness.rs` (keys `LB`, `RB`).

### 10.1 The pages and their input [V]

- **Tabs** (`FUN_002683c8` fills the u16 at `+0x18` of the page object): bit 0 the leader (when the party count `FUN_00282940` is not 0), bits 1..4
  the four forms (`FUN_00282b80`: the form's item is held and `FUN_00282b20` is false; that function tests the save byte at `+0xc` of
  the save block against a -1-terminated list at 0x3823a8 [GAP: what the byte is]), bit 5 the summons (`FUN_00268338`: any of the four items at
  0x381760 is held), bits 6.. the other party members. The list shows the set bits in ascending order (`FUN_00268828` list index -> tab, `FUN_00268880`
  the reverse).
- **Page 0x17** (`FUN_00265c48`, handler `FUN_00266000`): the tab rows use the group sets at 0x380860 (leader and members: idle 0x85, 0x87, 0x86; selected
  0x85, 0x87, 0x84, the same as page 0's list) and 0x380870 (forms and summons: 0x81, 0x83, 0x82 / 0x80); the label is the member's name
  (message `0x37ba90[member id]`, `FUN_001b5890`), a form's name (`0x381a38 + 2..`) or message 0xc5b1; the help line is the message of table 0x3817d0
  ("{}'s Status." with the member's name for the leader and the members, a sentence per form and for the summons). Cancel (-2): back to page 0
  (the tab cursor is remembered with `FUN_00252f98(0x17, tab)`); -4: close the menu; a confirm: page 0x18.
- **Page 0x18** (`FUN_00266328`, handler `FUN_00266760`): two list widgets (`DAT_0036ce9c`, `DAT_0036cea0`), one per column. Up/Down move in the column
  (the list widget of 1.7); Left/Right (-12, -13) `FUN_00268a08` toggle the byte `+0x1c` of the page object (the active column) and put the cursor on
  the other column at the same row, limited to its row count; -10 and -11 `FUN_00268c78` step the tab (-10 the previous, -11 the next, wrap at both
  ends; the page redraws for the new tab); -2 back to 0x17; -4 closes. A confirm has no case (nothing happens).
- The rows of the active column's cursor show their help message (`FUN_00267de0`: the table row's second u16).

### 10.2 Rows and number sources [V]

The row tables are at 0x3817e8 (leader and members), 0x381860 (forms), 0x3818d8 (summons): per table two blocks of 0x3c bytes (the columns), ten rows
of 6 bytes (name message, help message, row type; type 0xffff ends the block). `menu_status` reads them at run time; the disc test
`status_tables_have_the_documented_shape` prints every row with its text. A shown row is a bit `column * 10 + row` of the word at `+0x14` of the page object,
set per tab by the jump table 0x381768 (`FUN_00268568`): the leader and the members' case 0x2685a0 (bits 0..5 and 10..16; the leader alone also
bit 6 when form 1 is available, bit 7 always, bit 17 when a summon item is held, bits 8, 18, 9, 19 for forms 1, 2, 3, 4), forms 0x2686f8 (bits 0..3, 10..16),
summons 0x268758 (bits 0..3 and bit 10 + j for each summon j held). Shown rows are packed: a missing bit leaves no gap. The values are text
formatted by `FUN_001918a0` (a `sprintf` with the formats at 0x3816b8 `%d`, 0x3816c0 `%d/%d`, 0x3816c8 `%d%%`) into 0x20 byte buffers at `+0x520` of the page
object, filled by `FUN_002673c0` (jump table 0x3816d0; the leader's case is 0x267444, read from the instructions because the decompilation lacks it).

| row | label (message) | value | source |
|---|---|---|---|
| 0 | LV (0xc559) | `%d` | the member record `+0xf` (`Progress::sora().level`) |
| 1 | Experience (0xc55a) | `%d` | the word `0x340e48` (`Progress::experience`) |
| 2 | Next LV (0xc55b) | `%d` | `FUN_001878b0`: the Exp column of the `lvup` row for the current level (`FUN_001b50c0` clamps the level to the table length) minus the total; 0 at level 99 |
| 3 | HP (0xc55c) | `%d/%d` | status `+0` (the record's HP held at the maximum) and `+4` |
| 4 | MP (0xc55d) | `%d/%d` | status `+0x180` and `+0x184` |
| 5 | AP (0xc55e) | `%d/%d` | `FUN_002688d8`: the sum of the **equipped abilities' costs** (bit 15 of the record's ability slots; cost = the high byte of the item's `StatEntry`, item row `+7`) over status `+0x18e` = the `lvup` AP of the level + the record's AP byte `+8` + the `item_stat` AP of the equipment (`FUN_0019f920`, `FUN_001a0c08`) |
| 6 | Drive Gauge (0xc55f) | `%d/%d` | status `+0x1b1`, `+0x1b2` (the saved gauge bytes, party block `+0xdd1`, `+0xdd2`); shown with form 1 |
| 7 | Bonus LV (0xc567) | `%d` | the byte at party block `+0xfa8` (0x340e68) |
| 8, 9, 18, 19 | Valor LV, Master LV, Wisdom LV, Final LV (0xc5aa, 0xc5ac, 0xc5ab, 0xc5ad) | `%d` | the form record `+2` (`FUN_00187f60`) of forms 1, 3, 2, 4 |
| 10, 11, 12 | Strength, Magic, Defense (0xc560..2) | `%d` | status `+0x188`, `+0x18a`, `+0x18c` = the `lvup` row of the level + the record's bytes `+9`, `+0xa`, `+0xb` + the equipment's `item_stat` Attack, Magic, Defense |
| 13..16 | Fire, Blizzard, Thunder, Dark Resistance (0xc563..6) | `%d%%` | `100 - status[+0x1a9..+0x1ac]`; the seven bytes start at 100 and every equipped item multiplies each by its `item_stat` byte (+8 .. +0xe: physical, fire, ice, thunder, dark, light, general) `/ 100` rounded up (`FUN_001a0c08`) |
| 17 | Summon LV (0xc568) | `%d` | the byte at party block `+0xdce` (`Progress::summon_level`) |

"The equipment" is the record's two weapon words, the armor and accessory slots and, in a form, the form record's weapon (`FUN_0019f920`); the battle
item slots are not part of it. The forms' and summons' tabs print: the form level (record `+2`), its experience (`FUN_001d3c98`: the record's experience plus the
table entries below the level, the table from `FUN_001d3b90`), the experience to the next level (`FUN_001d3c38`: the table entry at the level minus the record's
experience) and the gauge row `level + 2`; the summons' tab the same from the summon level (`+0xdce`) and experience (`0x340e4c`, `FUN_001d3d48`,
`FUN_001d3cf8`). The second column of a form tab is the leader's strength, magic, defense and resistances. The party members' tabs print the leader's rows 0..5 and 10..16
from the member's own status object.

Checked on the disc (`the_leaders_numbers_follow_lvup_the_record_and_the_abilities_resource`): the new game with the app baseline gives LV 1, Next LV 40 (the Exp of `lvup` row 0), HP 20/20, MP 100/100,
AP 2/2 (Guard costs the 2 that `lvup` row 0 gives), Strength 5 (row 2 + Kingdom Key 3), Magic 7 (6 + 1), Defense 2, every resistance 0 %, Drive Gauge 3/3, and the same strength, magic, defense,
used AP and AP total as the `Abilities` resource computes (`equipment_stats`, `ap_used`, `ap_max` + the record's AP byte, which is 0 here).

**The AP rule** that 2.3 lists as a gap is therefore known for the display: the number the Abilities screen compares is the same `FUN_002688d8` sum over
`FUN_0019f920`'s total; the check that refuses an over-cost equip was still not found in the handlers read. `Abilities::ap_max()` (`abilities.rs`) leaves out the record's own AP byte `+8`.

### 10.3 What the port draws, and what is inferred

- Ported: the leader's tab (both pages, all rows of the table above with their conditions), the tab cursor, the column and row cursors with the wrap rules, the shoulder
  switch (`cycle`, a no-op with one tab), the help lines (the row's help message; "{}'s Status." with the name), the sounds (cursor, confirm, cancel).
- The forms' and summons' tabs: 16.5. The party members' tabs (Donald, Goofy): 19.1 (WP-MN7).
- **[I] Layout.** No footage shows Status (clips 5, 6, 7 and 10 do not). The groups come from `camp.2ld` sequence 1: a row plate of the wide rows (groups 0x143..0x147, one per row
  type) is 200 wide and 20 high above its origin with a row pitch of 24 (`group.unk1c`) and a column pitch of 232 (`group.unk20`), the value group of the page 0x17 preview
  (0x89, 0x8b, 0x8f, 0x90, 0x96) 164 wide with a pitch of 170; the value text is placed at the group's own x (180, resp. 160) and right aligned (the alignment is [I]). Where the list
  objects start (`FUN_00138ed0` on the lists, `FUN_0029b600`), the colours and the fonts of the labels and values were not read: they follow page 0's rows. Page 0x17 shows the tab list where
  page 0's list is and the preview plates at the right; page 0x18 the wide rows across the screen; page 0's own rows, cards and panel are hidden while the Status pages show (what the
  original does with them was not read). Munny and Time stay.
- **[I]** the exp-to-next number is held at 0 where the experience total passes the row (the original never shows that state: every gain runs the level-up check `FUN_001b52f8`;
  the app has no level-up yet).
- **[I]** forms are "available" by `Progress::owns_form`; `FUN_00282b20`'s exclusion is not applied.

### 10.4 Harness

`KH2_KEYS` knows `LB` and `RB` (`[` and `]`, the shoulder buttons) besides Enter, Escape, Backspace, J (confirm), Up, Down, Left, Right. Log lines: `menu: page 0x17`, `menu: page 0x18`,
`menu: status tab N`, `menu: status row C R`, `menu: status column C row R`, `menu: page 0`. Example: `KH2_KEYS=2:Enter,3:Down,3.1:Down,3.2:Down,3.3:Down,3.6:J` then a screenshot opens Status (Abilities
is the second row with the baseline's Guard, so Status is the fifth).

## 11. M6 as built (WP-M6: the Config page)

Code: `menu_config.rs`, a child module of `menu` (`#[path] pub mod config;` in `menu.rs`; `ConfigPlugin` is added by `MenuPlugin`), so it reads the menu's state without new exports. `Progress` gained the option word and the game mode (`option_word()`, `set_option_word()`, `game_mode()`; `KH2_STATE` keys `option_word` and `game_mode`). `camera.rs` reads the resource `menu::config::Options`.

- **Closed gaps (addresses).** The new-game option word: `FUN_0023e9a0` writes `(word & 0xfffff809) | 0x40b` (vibration on, Stereo, Navigational Map on, Summon Effects Auto, every other row at its first value); the app starts from 0x40b. The nine setters and the write-back order: `FUN_0027e398` calls `FUN_0023ec98, ed10, ee90, ee20, ef00, ec20, eb38, eba0, edb0` for rows 0..8 (row 9 writes nothing). `FUN_0023ed10` (Right Analog Stick) clears the Field Camera bit when the stick is on Command Menu and the camera on Manual, so rows 0 and 1 are never both at 1 after a write. `FUN_0027f0d0`: Right adds one and wraps to 0, Left subtracts and wraps to the last value; the page keeps rows 0 and 1 from both standing at 1 (the changed row wins); `FUN_0027f078` removes Left/Right on row 9. Only Vibration (`FUN_00239d28/38`) and Sound (`FUN_002bddf4`) act at once; everything else is applied when the page is left. Layout: panel group 0xe9 then 0xea, nine plates of group 0x117 (pitch = its `unk1c` 26, label size = its `size` 13), value cells whose groups are the pairs at 0x382140/0x382148/0x382150 (chosen, other; x pitch = the cell group's `unk1c`), all children of the page layer (`FUN_0029ba00`); the list widget's own rows are drawn blank (`DAT_0036cdec`).
- **Where the game reads each row** (callers by `mips_scan --jal`): Field Camera `FUN_001580e8` (flips the pad's camera-hold bit, so Auto = the camera moves by itself and is fixed while R2 is held; the help text 0x3720 says exactly that); Right Analog Stick `FUN_00167310` (pad layer: with Command Menu the stick drives the command menu while L2 is held, help 0x3724); Camera Up/Down `FUN_00157f98`, `00157fe8`; Left/Right `FUN_00158040`, `00158090` (each picks one of two logical camera bits from a table at 0x358a68 / 0x358a78, i.e. Reverse swaps them); Summon Effects `FUN_001c65f0` (raw 1 plays every time, 2 only the first time, **raw 3 plays nothing** although the page shows Full for it); Navigational Map `FUN_0015b798` (the minimap draw); Vibration `FUN_00167310`, `FUN_00280c30`, `FUN_00281040`, `FUN_00281588`, `FUN_0023ea18`; Sound `FUN_0023ea18` (applied at load); Command Menu `FUN_0013a6d0` (the per-world themed menu when the bit is clear).
- **What the app applies.** `Options` (decoded from the word once at start and after every write-back; log line `menu: config applied ...`): Field Camera Manual turns the camera's `auto` input off (the R2 hold is not modelled); Camera Up/Down and Left/Right Reverse flip the right-stick axes in `camera.rs` [inferred mapping of axes]. Stored and exposed, not yet read by anything: Summon Effects (`Options::summon_raw()` for the summon work), Navigational Map, Command Menu theme (`hud_menu`), Right Analog Stick, Sound, Vibration.
- **Game Mode** (row 9) shows `Progress::game_mode()`, default Standard [inferred: the app has no title screen]; it is read-only as in the original.
- **Not done / gaps**: the original's Config footage does not exist in the clips, so positions inside the panel come from the groups' own coordinates and the value text is moved 10 units right of the cell anchor by eye; the hand on the cursor row is [I] (the cursor row is also drawn white); the help line is the current value's help (`+10, +12, +14` of the row, [I] for row 9); the page's open and leave animations and the scroll of the ninth row are not animated; the cards of page 0 are hidden while the page is up [I].
- **Harness**: `KH2_KEYS=3:Enter,4:Up,5:J` opens the page (Up from the first row wraps to Config); Left/Right change the value, Up/Down move (wrap on a new press only), Escape/K/Backspace leave with write-back, Enter writes back and closes. `KH2_STATE` with `{"option_word": N}` sets the start word; `KH2_MENU_CONFIG_LOG=1` prints the page's group data and the help message ids. Log lines: `menu: page 0x24 (Config) option word ...`, `menu: config row N -> V`, `menu: config leave: option word A -> B`.

## 12. M5 as built (WP-M5: Abilities)

Code: `menu_abilities.rs` (the table, the page machine, the screen), `progress.rs` (`ability_slots`, `set_ability_equipped`), hooks in `menu.rs` (`pub(crate) input`, `Menu::current_entry`, `Menu::page0_nodes`), `harness.rs` (keys `Y`, `[`, `]`), `main.rs` (plugin in the third tuple).

**Rules read this stage (addresses), closing the AP gap of 2.3 and section 7.**
- The row table `DAT_0036d110` is built by `FUN_0027a7e8` from the 64-slot list (`record + 0x54`): header +2 = the AP maximum (status `+0x18e`), +4 = AP in use, +6 character count, +7 row count, +8..+0xb rows per group; rows of 8 bytes at +0xc: +0 id, +2 equipped (bit 15), +3 slot, +4 **group**, +5 **cost**, +6 command slot. (Section 2.3 called +4 "locked": it is the group.) Group from the item's `StatEntry` low byte: 0 -> Support (2), 1 and 2 -> Action (0), 3 -> Growth (1); cost = high byte. `FUN_0027adc0` appends the keyblade's own ability (its `item_stat` Ability) as a Weapons row, equipped, cost 0, slot 0xff. `FUN_0027b1b0` sorts by group, then (not Weapons) by the position of the item in the item table.
- **AP in use** is the sum of the costs of the equipped rows (`FUN_0027b150`). **The rule:** `FUN_0027d6c0(row)`: an equipped row is always allowed; an unequipped row only when `cost <= max(0, max - used)`. The list's validity callback `FUN_0027dc30` (passed to `FUN_00295bf0`) refuses a confirm, `-6` or a row whose cost byte is 0 (the keyblade's row) and, for a character's own list, an unaffordable row (refused sound 4, nothing else). Equip prompt page 0x11..0x13 (group 0, 1, 2) starts on the row's current state (`FUN_0027c780`); its two rows are "Don't Equip" (index 0, 0x3629) and "Equip" (index 1, 0x3627), table 0x3820a8; `FUN_0027d960` writes `FUN_0027d8b8(row, option != 0)`; cancel (-2) returns to 0x10 unchanged.
- **AP maximum**: `FUN_0019f920`: the `lvup` AP of the level + the record's AP byte (`+8`, raised by AP Boost, `FUN_001a0dd0`) + the equipment's AP.
- **AP bar** (`FUN_0027c9e8`): the label is "used/max" (format 0x382030); the bar previews the highlighted row: used - cost if it is equipped, used + cost otherwise, held to the maximum. Two gauges: the wider (the larger of now and after) uses group 0x10d (gain) or 0x10f (loss, after < now), the narrower (the smaller) 0x10e. This is why the footage's bar is blue at 0/2 on an unequipped row and yellow at 2/2 on an equipped one.
- **Page 0xf** (`FUN_00278aa8`) lists the characters (Sora, then the drive forms `FUN_00278eb0`, then the summons, then the party members) and previews the highlighted one's equipped abilities (`FUN_0027b668`); help line message at 0x38204c. **Page 0x10** shows nine rows (`FUN_002794c8`), the group name tag on the first row of each group (`FUN_0027b9a8`), cost plate group 0xfb, plates by group from 0x382060, tags 0x382068, equipped icons through `FUN_00255520` / the icon table at 0x3809d0 (type, icon, equipped icon). Headers: three tabs (`FUN_0029cf78` with 0x380828: groups 0x4b enter, 0x4d leave, 0x4c hold; pitch 120). The list window is groups 0xe6/0xe7/0xe8 (children hang on its anchor (111, 84); pitch 26).
- Keys: Up/Down (wrap on a new press only), confirm J, cancel K/Escape/Backspace, Start closes the menu (page 0 then closes), `[` `]` the character switch (-10/-11, refused with one character: `FUN_0027dc30`), `Y` the quick toggle (-6, `FUN_0027d708`; shown only for rows with a cost). The physical buttons of -5, -6, -7 stay a [GAP] (1.7).

**Not done / gaps (kept conservative).** The party is Sora alone: no drive form, summon, Donald or Goofy rows on page 0xf and no four-row command-slot prompt of group 0 for them (`FUN_0027a068`'s `iStack_6c = 4` branch, `FUN_002748d8`). The Items switch (-5), Help (-7) and the "NEW" tags are not drawn. The AP maximum uses the record level (`Progress`), not the Sora entity's. Row, tab and prompt text positions are placed from the groups' own anchors and compared with clip 6 at 4.4 s by eye; the prompt (pages 0x11..0x13) has no footage (the remaster toggles with Y directly), so its look is the groups' own. The scroll thumb is not drawn.

**Harness.** `KH2_KEYS` knows `Y`, `[` and `]`. Log lines: `menu: page 0xf`, `menu: page 0x10`, `menu: page 0x11`, `abilities: <name> (id N, slot S) equipped; AP u/m`, `... taken off, no longer equipped; AP u/m`. `KH2_STATE` with `abilities` / `abilities_off` sets the list (docs/progress.md).

## 13. M3 as built, part 1: the Items pages (WP-M3)

Code: `menu_items.rs`, a child module of `menu` (`#[path] pub mod item_pages;`; its plugin is added by `MenuPlugin`, so `main.rs` is untouched), `progress.rs` (`apply_boost`, `boost_maxed`), `kh2-image/tests/menu_items.rs` (disc tests that print the tables and group boxes). Page 0's confirm on the Items row (entry 0) opens page 1 (the module takes the key before `menu::input`, as the Abilities pages do).

**Pages and what a key does [V from the page handlers, addresses below].**
- **Page 1** (`FUN_0025ae18`): one row per party member, then the Stock row (message 0x455). Confirm on Stock opens page 0xa (the stock list is built with filter 0, `FUN_0025e790(0)`); confirm on a member opens page 2, his categories (`FUN_00269580`, stage M4, section 15); cancel (-2) goes to page 0, -4 closes, -5 would switch to Abilities (not ported, no entry point in `menu_abilities`). The help line is message 0x3b86 on a member and 0x3b87 on Stock (`FUN_0025ddc0`: 0xbb86 / 0xbb87).
- **Page 0xa**, the Stock (`FUN_0025b6e0`, built by `FUN_0025b2a0`): a grid of two columns and nine rows (`FUN_00298e40(list, 2, 9)`). The entries come from `FUN_00283a70(list, 0, arg)` and the sort `FUN_002845a0`: every item row, in table order, whose type is in the filter word (`FUN_00283810`), of which the player owns at least one (`FUN_001a1cc8`), and that no set story flag hides (`FUN_002846f0`, pairs at 0x382450: three items under flag 0x4d; the app has no such flag, so nothing is hidden). Entries are 6 bytes: id, count byte (the low byte of the count), flag byte, sort key. There are **five filters**, cycled by -10 (add one) and -11 (subtract one) with wraparound (`FUN_0025e838`), starting at 0 on entry: the five stubs at 0x25e7e0..0x25e830 pass the arguments 0, 0xd, 0xb, 0xc, 0xe, whose words are: everything but spells, abilities and forms (types 0..13, 14, 15, 16, 17, 20), types 0 and 1, types 2..13 (the pair table at 0x3823d0), types 14 and 15, types 16, 17 and 20. The cell text is the item's icon byte and name (`FUN_00284088` through `FUN_00255520`) and a count `*%d` (format at 0x382408) unless the count is 1; the font has no asterisk and its glyph 13 is the multiplication sign, which the port prints [inferred]. An empty list shows "There are no items." (0x45a) in the first cell with group 0x12e (`FUN_0025dae8`). Movement is the grid mover `FUN_00297cc0`: Up and Down move one row, Left and Right wrap inside the row; the port wraps top and bottom on a new press only (the list rule of 1.7) and clamps to the last cell of a short last row [inferred]. Confirm opens page 0xb (not offered on an empty list, `FUN_0025b2a0` clears the mask's 0x10), cancel goes back to page 1 with the cursor on Stock, -4 closes. The help line is the item's description (`item +0xa`, `FUN_0025ddc0`).
- **Page 0xb**, the prompt for the highlighted item (`FUN_0025bfa8`, built by `FUN_0025bad0`): rows "Use" and "Throw away" (messages 0x456, 0x457 at 0x381090) when the item's type is one of the usable types (0x381060: types 0 and 1, `FUN_0025e8a0`), else only "Throw away". "Throw away" is greyed and refused when the item has no sell price (`item +0xe`, `FUN_0025e928`); the choice rule is `FUN_0025eab0` (Use row of a usable type: use; otherwise throw away if it can be, else refused) with the validity callback `FUN_0025eba8`. The title strip shows the item's own list text (icon, name, count).
- **Page 0xc**, the target list (`FUN_0025c590`, built by `FUN_0025c2c8`): one row "All" (message 0x3944) for the group items (5, 6, 7, 0x83: `FUN_001a1fe0`); one row for the two items that go to one member only (0x112, 0x113: the pair at 0x381068, `FUN_0025e978`); else one row per party member, titled "Use on who?" (0x3943). Confirm asks `FUN_0025e9e8` = `FUN_001a14c8` = **`FUN_001a15a0`** whether the item would change anything on the member (any of the four slots for a group item). If it would not: sound kind 8 and page 0xd. If it would: sound kind 0x11, **`FUN_0025ea40` = `FUN_001a11a0(item, member, 0, 100)` then `FUN_001a1ba8(item, 100)`** (the effect with no Item Boost, then one off the stock), and back to page 0xa. Cancel goes back to page 0xb, -4 closes the menu.
- **Page 0xd** (`FUN_0025c930`): the notice "{type}{name} will have no effect if used." (message 0x3b90; the two variables are the type's name message through the table at 0x380a60, `FUN_002553c0`, and the item's name) in the information window with an OK row; every key goes back to page 0xc.
- **Page 0xe** (`FUN_0025cba0`, built by `FUN_0025cae8`): "Throw {}{} away?" (0x394f) with the rows No and Yes (messages 0x1fd1, 0x1fd0 at 0x36cf2c, 0x36cf2a); Yes (row 1) runs `FUN_001a1ba8(item, 100)` and returns to page 0xa; No and cancel return to page 0xb.

**The use check, `FUN_001a15a0` [V, oracle].** HP items (1, 2, 5) need HP below the maximum; MP items (3, 6) need MP below the maximum or the MP charge running; the combined items (4, 7, 0x83) are usable unless HP is full and neither the charge runs nor MP is below full; 0x112 and 0x113 need the drive points below the maximum; the stat boosts (0x114 strength, 0x115 magic, 0x116 defense, 0x117 AP) need the record byte (`+9`, `+0xa`, `+0xb`, `+8`) not above 99 (`FUN_001a0ec0`, `0ee0`, `0f00`, `0f20`); everything else is "no effect". **The boosts' effect** (`FUN_001a0ce0`, `0d30`, `0d80`, `0dd0`): the record byte plus the amount, held at 100 (when the sum would pass 100 only the room is added); the live status block gets the same amount, which the app recomputes from the record (the Status page reads it: after one Power Boost the strength shows 6 where the new game shows 5). The amount is the item's `Flag1 | Rank << 8` word, which is 1 for all four [inferred: the decompilation dropped the argument; the item text says "+1"].

**Evidence that runs.**
- `item_pages::tests::can_use_matches_the_game`: kh2-ee runs `FUN_001a15a0` on the user's executable (the status record stubbed as `docs/items.md` does for `FUN_001a12a0`, `+0x238` pointing at a record holding the four boost bytes) for every item row over five HP/MP/charge/drive states and five record variants: `oracle: FUN_001a15a0 equal for 11650 cases (210 usable) over 466 item rows`.
- `item_pages::tests::stock_list_matches_the_game`: `FUN_00283a70` with the five stubs' arguments (and argument 10, see section 14) and `FUN_002845a0` on a party block with counts, against `stock`: `oracle: FUN_00283a70 + FUN_002845a0 equal for 18 lists (1116 entries ...)`.
- `progress::tests::menu_writers_match_the_game`: `FUN_001a1ba8(id, 100)` (the throw away and the take after a use) against `Progress::take_item(id, INVENTORY)` on the new-game block, together with the Customize writers of section 14.
- The page machine is tested without the disc (`item_pages::tests`: the stock opens from page 1, a use asks for a member then takes one and returns to the stock, a use that would do nothing shows the notice, throwing away asks first and only Yes discards, the filters cycle, the grid moves and scrolls). `cargo test -p kh2-image --test menu_items -- --nocapture` prints the tables of this section and the camp groups' boxes.

**Sounds.** Kind 8 (the notice) is not below 6, so its id is the u16 at 0x37f654 + 2 * 8 = 2, the confirm sound; kind 0x11 (an item used) is the u16 at 0x37f654 + 2 * 0x11 = 37 (`FUN_0023e828`). In the port `PlaySe::system(id)` (an entry of `se000.seb`), as in section 9; the pages' own constants still hold the old `2000 + id` form, which `menu_sound::system_id` maps (WP-SE2). The filter step (L1 / R1, list results -10 / -11: `FUN_0025b6e0` -> `FUN_0025e838`) goes through `FUN_00295bf0`, which maps -10 and -11 to kind 2, the confirm sound (2002), not the cursor's (fixed in WP-F1 item 9). **Mute switch (WP-F1 item 14, user report 2026-10-10: the menu sounds are wrong):** every menu sound request of the pause menu and its pages passes `menu::menu_sound::play`, gated by `menu_sound::MENU_SOUND_ENABLED` (false for now: no request reaches the mixer; `KH2_SOUNDLOG=1` logs `menu se 0:<id> muted`). WP-SE2 corrected the id mapping (`se 0:<id>`), the switch stays false: the user wants the menu sounds muted for now. Set it to `true` to hear them again; the ids stay in the code.

**Layout [the groups are read; the positions inside them are placed by eye, there is no clip of these pages].** Header tabs as the Abilities pages (group 0x4b then 0x4c): "Items" alone on page 1, "Items" and "Stock" on the pages after it [inferred]. Page 1 (compared with clip 6 at 10.2 s, which shows one tab "Items", the rows "Roxas" and "Stock" as page 0's rows, and at the right the member's slots, one line each: the weapon, the armor slots, the accessory slots, the battle item slots, empty slots as an icon and `---`, message 0x448, with a tinted plate per category): the rows are page 0's (entry 0's group sets); the preview is drawn in the list window 0xe6 as plain lines, an empty weapon or battle item slot with the icon of its type from the table at 0x3809d0, an empty armor or accessory slot with no icon, and **without the category plates [GAP: the preview builder was not found; `FUN_00269028` is page 2's builder, not page 1's]**. Page 0xa: the panel 0x125 then 0x126 (anchor (24, 83)), cells at the group 0x130's pitch (230 across, 26 down), the selected cell group 0x12f (a 216 wide plate with the red outline). Pages 0xb to 0xe: the prompt panel 0x128 then 0x129 (anchor (26, 112), 248 by 102) with the title strip 0x124 above the first row and rows of plate 0x123 (pitch 28), the item window 0x131 then 0x132 on the right (257..506 by 72..326: the item's **picture is not drawn, [GAP]**: the picture id is `item +0x14` and `FUN_0029ed50` draws it from another file), the target panel 0x12b then 0x12c under the prompt panel (anchor (26, 210)), and the information windows `0x380880` and `0x380888` (groups in the order enter, leave, hold: 6, 8, 7 for the notice and 9, 0xb, 0xa for the question; the question's list has the 24 pitch of its group). In the original the Stock grid is not hidden behind the prompt panels (they stack beside it); the port draws the prompt pages without it.

**Not done / gaps (kept conservative).** (Page 2 and the equipment pages 5..9 are stage M4: section 15.) The -5 Items to Abilities switch and the Help overlay. The item picture. The NEW tags. The cursor after the list shrinks (`FUN_0025eb20` steps the scroll and the cursor back by a row or a cell; the port puts the cursor on the last entry [inferred]). The filter tab strip (messages 0x390c ALL, 0x390f, 0x390d, 0x390e, which are icon tokens; four messages for five filters, `0x381078` words 0x15c, 0x15e, 0x15d are the tab groups): the port does not draw it, the active filter is only logged. The party is Sora alone, so "All" and Sora's row test the same member, and a use applies to the `Sora` entity (HP, MP, the MP charge, the drive gauge) and the record (the boosts); Donald's and Goofy's status blocks have no counterpart. The story flags that hide stock items (0x382450) are never set in the app.

**Harness.** `KH2_KEYS` as in section 10 (`Enter` opens the menu, `J` confirms, `Escape` or `K` cancels, `LB` and `RB` change the filter). `KH2_STATE` `{"items": {"1": 7, ...}}` fills the stock (`local/state-stock.json` was used: ids 1 Potion, 2 Hi-Potion, 3 Ether, 4 Elixir, 5 Mega-Potion, 7 Megalixir, 274 Drive Recovery, 276 Power Boost, 277 Magic Boost). Log lines: `menu: page 0x1`, `menu: stock filter N: M items`, `menu: page 0xa cursor N <name> xC`, `menu: page 0xb|0xc|0xd|0xe`, `menu: item used: <name> (id N, row R) on Sora: HP a -> b, MP a -> b[, boost +n (record byte a -> b)], stock a -> b`, `menu: item thrown away: <name> (id N), stock a -> b`. Example (screenshots in `local/m3/`): `KH2_KEYS=3:Enter,3.5:J,4:Down,4.5:J` is the Stock page; `... 5.2:J` the prompt; `5.2:J,5.8:J` the target list; `5.2:J,5.8:J,6.4:J` the notice (full HP); `5.2:J,5.6:Down,6:J` the throw away question.

## 14. M3 as built, part 2: the Customize pages (WP-M3)

Code: `menu_customize.rs`, a child module of `menu` (`#[path] pub mod customize;`), `progress.rs` (`shortcut`, `shortcuts`, `set_shortcut`, `set_spell_switch`, `toggle_spell_switch`, `set_auto_reload`, `reload_off_needs_question`). Page 0's confirm on the Customize row (entry 2) opens page 0x19.

**Pages [V from the handlers; the sub page layout [I]].**
- **Page 0x19** (`FUN_002707f8`): the characters (Sora). Confirm opens page 0x1a (`0x1b` for another character, section 14b), cancel goes to page 0, -4 closes. Help: message 0x3b89.
- **Page 0x1a**, Sora's settings (`FUN_00271078`, rows built by `FUN_00270ae0`, the working table by `FUN_002723c8`): the row kinds of the leader are the bytes at 0x381d40: 0, 1, 2 (the others' table, 0x381d48, is 3, 4, 2). **Kind 0**: four shortcut rows (the u16 at save `+0x2600 + 2 k`), text = the icon of the item whose command the word is (`FUN_00272fc8`: the first consumable or spell item in table order with that command through `FUN_001a1f78`) and the command's own name (`FUN_00272e68`: the command row's message), or `---` (message 0x448) when empty; each row is tagged with its own button label (messages 0x47d..0x480, the button icons 37, 34, 36, 35). **Kind 1**: one row per owned spell in slot order (Fire, Blizzard, Thunder, Cure, Magnet, Reflect: the type 0x12 items of `ItemIndex::magic_items` with a count), tagged "MP Charge" (0x481); the flag is `FUN_001a4bb8` (the switch bit **clear** is "MP Charge"). **Kind 2**: one row per distinct battle item (the id each slot carries, or reserves when empty), tagged "Auto-Reload" (0x482); the flag is that slot's reserved id equal to the item. At the end of the routine each shortcut row takes the flag of the last row with the same item. The row icon is `FUN_00255368(type, flag)` through the table at 0x3809d0 (type, unequipped icon, equipped icon; 0xb when unlisted), and `FUN_00272b20` passes the flag for items and its **negation for spells**: a spell that is on shows icon 8 and one that is off icon 18, which are the icons of the sub page's two rows. Cancel goes to page 0x19; -6 (the quick key, `Y`) switches a spell row (`FUN_00274e50`) or an item row (`FUN_00274f70`, after the question when needed) and does nothing on a shortcut row; -10 and -11 switch the character (refused with one character); confirm opens the kind's sub page: 0x1d, 0x1e or 0x1f (`FUN_00271078` jump table 0x381bb4: the kinds 1 to 4 load 0x1e to 0x21).
- **Page 0x1d**, the shortcut's command (`FUN_00272018`, list `FUN_00273d30`): row 0 "Remove Shortcut settings." (message 0x1df7), then the stock list filtered with the argument 10 of `FUN_00283810` (bit 0, the consumables, and bit 18, the spells), spells first because `FUN_002845a0` keys a spell by its table position and everything else by position plus 1000 (oracle: `stock_list_matches_the_game` checks it as its filter 5). At most 8 rows are shown. Confirm runs **`FUN_00274dd0`**: the u16 at `+0x2600 + 2 * slot` becomes `FUN_001a1f78(item)`: a consumable's own command (`item +0x10`), a spell's loaded command (`FUN_001a4dd0`), 0 for the removal row. The slot is the settings page's remembered cursor, `FUN_00252fe8(0x1a)`.
- **Pages 0x1e and 0x1f**: two rows whose index is the flag: 0x1e "[icon 18]Don't MP Charge" (0x1df9) then "[icon 8]MP Charge" (0x1df8) (table 0x381d20); 0x1f "[icon 0]Auto-Reload OFF" (0x1dfd) then "[icon 12]Auto-Reload ON" (0x1dfc) (table 0x381d30). A confirm on the row that is already the state changes nothing; otherwise `FUN_00274c88` runs the switch (`FUN_00274e50` flips the bit of the word at save `+0x264c`; `FUN_00274f70` sets or clears the reserved id of every battle item slot that carries or reserves the item, the items stay in their slots).
- **Page 0x1c**, the question (`FUN_00271688`, message 0x46a): when Auto-Reload is switched **off** for an item that is on and a slot reserves it without carrying it (`FUN_00275288`: an empty slot waiting for a refill), the information window asks "Turn auto-reload function Off?" with No and Yes; Yes switches it off, No and cancel go back.

**Evidence that runs.** `progress::tests::menu_writers_match_the_game`: the game's `FUN_001a4b68`, `FUN_001a4b30` and `FUN_001a4bb8` (and the flip `FUN_00274e50` makes of them) in kh2-ee on the new-game block against `set_spell_switch` and `toggle_spell_switch` over eight bits and six flips, the shortcut word at save `+0x2600` (party block `+0xfa0`), and the stock take: `oracle: menu writers equal (spell switch set/clear/flip [...], 4 shortcut words, 18 stock takes; counts floor at 0)`. `menu::customize::tests` (the rows, the shortcut flag copy, the page flows, the candidate order), `progress::tests::auto_reload_reserves_and_releases_every_slot_of_the_item`, `progress::tests::shortcuts_spell_switches_and_boosts_write_their_words`. `cargo test -p kh2-image --test menu_items -- --nocapture` prints the Customize tables.

**Layout [I].** Header tabs "Customize" and "Sora"; the window 0xe3 then 0xe4 (anchor (111, 84)), nine rows of the kind's plate (0x108, 0x107, 0x106; pitch 26) with the section tag 0x11a, 0x119, 0x118 to the left of the first row of a kind (each shortcut row has its own); the sub pages' panels (the enter and hold groups of the triples at 0x381cd8: 0xc6/0xc7 for the shortcuts, 0xbd/0xbe for spells, 0xb4/0xb5 for items; 248 by 254 at the left edge) with the title strip 0x124 (the button label or the section name) and rows of plate 0x123 (pitch 28, eight rows fit); the question uses the Yes / No information window of section 13. No clip of these pages exists.

**Not done / gaps.** (Donald's and Goofy's pages and the character switch: section 14b.) The Help overlay (-7) and the NEW tags; the start row of page 0x1d (the port starts on the row of the shortcut's current item [inferred]); the field's L1 shortcut list that uses the shortcuts is `items::CommandLists` list 5, which the app can reach with no key (docs/items.md 4); **the spell switch word is not read by the spell availability**: `MagicData::status` (`magic.rs`) passes `false` for the `toggle_bit` of `magic_rules::availability` (docs/magic.md: with too little MP a spell whose bit is clear is "short of MP" and still castable, a set bit makes it unavailable, `FUN_001ba2c0` through `FUN_001a4bb8`), so the page's switch is stored and logged but changes nothing in a fight yet. The change is outside this stage's files (it needs `Progress::spell_switch(slot)` for the command's slot at the callers of `MagicData::status`).

**Harness.** `KH2_KEYS=3:Enter,3.5:Down,4:J,4.5:J` opens Sora's settings (Customize is the second row of the list when Abilities is hidden); `5:Down,5.4:J,5.8:Down,6.2:J` stores Fire in shortcut 1; `Y` on a spell or an item row switches it. `KH2_STATE` `{"sora_items": [1, 1, 3], "magic": [1, 1, 1, 1]}` (`local/state-customize.json`) gives two Potions and an Ether in Sora's slots (the first pickup also sets shortcut 0) and four spells. Log lines: `menu: page 0x19`, `menu: page 0x1a`, `menu: page 0x1d|0x1e|0x1f|0x1c`, `menu: shortcut S = command C (<name>), word 0x0031, shortcuts [..]`, `menu: spell slot S MP Charge|Don't MP Charge (switch word 0x..)`, `menu: auto-reload item I ON|OFF (slots [(item, reserved), ..])`.

## 14b. Customize page 0x1b: a companion's settings (WP-PA4)

Code: `menu_customize.rs` (`Kind::Style`, `Kind::Ability`, `build_friend_rows`, `write_friend`, the pages `STYLE` 0x20 and `ABILITY` 0x21, the four-row `RELOAD` for a companion), `progress.rs` (`ItemInfo::stat`), `party.rs` (`record_setting`, the item handler's setting). Page 0x1b is **the same handler as 0x1a** (`FUN_00271078`: every branch tests the remembered character `FUN_00252fb8(0x19)` or the sub pages' list); only the row table and its kinds change. [V from the decompilation; the layout [I].]

**The character list** (`FUN_002707f8`, page 0x19): one row per member of the party menu object (`FUN_00282940` counts, `FUN_002829c0(party, i)` gives the member's status object whose `+0x238` is the saved record). Confirm on row 0 opens page 0x1a, on any other row page 0x1b (`uVar5 = 0x1a, 0x1b if the row is not 0`); when the row differs from the remembered character the settings cursor is reset (`FUN_00252f98(0x1a, 0, 0)`). The app's list is the leader and the members standing in the field in slot order (`menu_party::roster`), the layout of the rows below the first is [I].

**Rows** (`FUN_002723c8`, the working table at `DAT_0036d030`, 8 bytes a row: `+8` id, `+10` flag, `+11` the use setting): the kind list of a character is the three bytes of `FUN_00272af8`: **0x381d40 = 0, 1, 2** for the leader, **0x381d48 = 3, 4, 2** for the others. Kind 3 is one row, the Battle Style: the table header byte `+3` copies record `+0xd4` (the list shows it as its first row and the table has no row for it, hence the `+1` of the list index on page 0x1b in `FUN_00274f70`, `FUN_00275288`, `FUN_00272018`). Kind 4 lists the record's ability words (`record +0x54`, 64 words) that have bit 15 set (equipped) and whose item row has `+6` (`StatEntry` low byte, `item_stat`'s kind) equal 2, in item-table order (the selection sort at the end of the case orders them by their table position); each row's `+10` is 1 and `+11` is record `+0xd5 + FUN_00114348(id)`. Kind 2 lists the battle items as for the leader, with `+11` read the same way. Heading messages by kind (0x381cc0): 0x47d, 0x481, 0x482, **0x483, 0x484** (Battle Style, Abilities); row plates by kind (0x381ca0): 0x108, 0x107, 0x106, **0xfa, 0xf8**; section tags (0x381cb0): 0x11a, 0x119, 0x118, **0x115, 0x114**; sub page panels (the triples at 0x381cd8): **0xa2 / 0xa3** (styles) and **0x99 / 0x9a** (use settings). The row text: kind 3 `FUN_00272f80` draws the style's name (messages 0x381c18 + 4 * style: 0x486..0x48b); kinds 4 and 2 `FUN_00272e68` (the type icon of the item through 0x3809d0 and its name); then `FUN_00272b20` draws for kinds 2 and 4 of a companion the **icon of the entry of the kind's table that holds the use setting** (`FUN_002748d8(kind, byte)` and its `+4`).

**The choice tables** (8-byte entries: `+0` message, `+2` help message, `+4` icon, `+5` the value written): `FUN_002748d8(kind, value)` picks the table, 0x381d58 for kind 3 (6 entries, values 0..5: Technic, Target, Huddle, Party, Sora and Relentless Attack, messages 0x486..0x48b, help 0x1e55..0x1e5a), 0x381d88 for kind 4 (3 entries, values 0, 1, 2, messages 0x1e4e..0x1e50 "Free", "Well-Balanced", "Rare", icons 0x1a, 0x1b, 0x1c), 0x381da0 for kind 2 (2 entries, values 0 and 2, messages 0x1e51 "Often" and 0x1e52 "Last hope!", icons 0x1a and 0x1c). The pages the jump table 0x381bb4 opens for the kinds 0..4 are 0x1d, 0x1e, 0x1f, **0x20, 0x21** (the code at 0x271580..0x271644 loads those constants).

**Sub pages and writers.** All of them run `FUN_00272018`, whose confirm calls **`FUN_00274c88`**: kind 3 runs **`FUN_00275398(row)`**, kind 4 **`FUN_00275410(row)`**, kind 2 `FUN_00274f70` (when the row differs from the state).
- **0x20, Battle Style** (rows built by `FUN_00274158`, six): `FUN_00275398(row)` writes the **record byte `+0xd4`** = the row (the quick key passes -1: the byte plus one, back to 0 above 5) and copies it into the table header. The page opens on the row of the current style [I: the start row is the remembered list cursor, not read].
- **0x21, an ability's use setting** (`FUN_002741c8`, three): `FUN_00275410(row)` writes the value of the entry (`+5`) into **record `+0xd5 + FUN_00114348(ability id)`** and into the row's `+11`; the quick key passes -1: the entry after the one holding the current value, 0 when none holds it or after the last.
- **0x1f, Auto-Reload** for a companion lists **four rows** (`FUN_00274090` builds the leader's two, then two more from the same message table when the character is not 0): Auto-Reload OFF and ON, each for the use setting 0 and then 2; row `i` shows message 0x1dfd / 0x1dfc by `i % 2` and, `FUN_002747d0`, the extra text of the 0x381da0 entry `i / 2` ("Often", "Last hope!"). The state of a row is the Auto-Reload flag plus 2 when the use setting is 2. `FUN_00274f70` for a companion: the page's row `r` gives flag `r & 1` and use setting `(r & 2) ? 2 : 0`, the reserved ids of the slots carrying or reserving the item are set or cleared exactly as for the leader, then **record `+0xd5 + FUN_00114348(item)`** takes the use setting. The quick key (-6) on a companion's item row steps the four states (`((setting != 0) << 1 | flag) + 1`, 0 above 3). The "turn Auto-Reload off?" question (`FUN_00275288`) asks for a companion when the **parity of the settings list's cursor** is even (the code reads the list cursor even on page 0x1b; as read, the port keeps it), when the page-0x1f row is an "off" row (even), and a slot reserves the item without carrying it; Yes (`FUN_002716d0`) runs `FUN_00274f70` (the quick key) or lets the page finish the write.
- The help line (`FUN_002743e8`): the highlighted row's item description (`item +10`), and on page 0x1b the help message of the choice table's entry for the row's setting is **appended**; the Battle Style row shows the style's help message; sub pages show the help message of the highlighted entry (0x20, 0x21), of the on/off pair at 0x381d38 (0x1f, plus the 0x381da2 entry for a companion) or 0x381d28 (0x1e). The app shows the first message only [GAP: the appended second one].
- **-10 / -11** (`FUN_00275538`: the next / previous character, wrapping; the new page is 0x1a for row 0 and 0x1b otherwise, the cursor on the first row) switch the character from any settings page; with one character the key is refused.

**The setting-byte index** (`FUN_00114348(id)`): the seven consumables of 0x352eb0 (ids 1..7) are 0..6; any other id is `7 + FUN_001144d0(id)`, which scans the 29 entries of 0x352ec0 (four words each) for the entry whose word 1 is the id and returns its word 3 (0..3, so the abilities share bytes by their index within a member); an id the table lacks makes the original add 7 to -1 and hit byte 6's neighbour, which the port does not do (it writes nothing). The settings block of the record is `+0xd4`, byte `1 + k`. `party.rs` reads them: `FUN_00113888` (HP item) tests byte `1 + FUN_00114348(item)` below 2 (`item_setting`), and the AI records' own index is `record_setting` (`FUN_0010f220`, docs/party.md).

**Evidence that runs.** `menu::customize::tests`: `the_character_list_has_a_row_per_member_and_the_other_rows_open_page_0x1b`, `the_companions_rows_are_the_style_the_kind_two_abilities_in_table_order_and_the_battle_items`, `the_style_row_writes_setting_byte_zero_of_the_members_record` (page flow, the six rows wrap, the quick key 3, 4, 5, 0, 1, the switch to Goofy and the wrap through the leader), `an_abilitys_use_setting_is_written_to_its_byte_past_the_consumables`, `a_companions_item_row_has_four_states_and_asks_on_the_off_rows_when_a_slot_waits`, `the_pages_writes_survive_a_state_file_round_trip` (page writes, `slot_json`, `apply_json`), `the_executables_tables_give_the_companions_pages` (needs `KH2_ISO`: the kind lists, the three choice tables, the 29 setting ids, every message exists); `party::tests::party_follows_what_the_customize_page_writes` (Donald's Style row driven through the page picks the far enemy in PA3's scene, and `record_setting` reads the ability byte).

**Layout [I].** The group ids and messages above are read; the positions inside them are placed from the group boxes (there is no clip of these pages): the setting icon at the right end of a companion's ability and item rows, the extra text of page 0x1f's rows (placed so that it does not overlap the row's label; the original puts it at `FUN_00237698` of the row, a value the row group's `unk20` (100) does not match), the rows of the character list below the first (pitch 26).

**Setting byte bound (FX12a).** The use-setting table at 0x352ec0 has 29 entries; the disc test `the_executables_tables_give_the_companions_pages` writes through every one of them: the largest index `k` is 10, so the highest byte is `1 + 7 + 10 = 11`, well inside the 32-byte settings block (`rec::SETTINGS_LEN`). The column `+12` is not dense (29 entries, largest value 10). `write_friend` now logs a warning if a write is ever refused. The AI side (`party.rs` `item_setting`) reads only the consumables 1..=7; other items have no handler that reads a setting [inferred from the port's handler list, not checked against `FUN_00113888` for other ids].

**Harness.** In tt04 the pause menu's rows are `[0, 1, 2, 3, 4, 5, 7]`: `KH2_KEYS=3:Enter,3.5:Down,3.8:Down,4.3:J` opens page 0x19, `4.8:Down,5.3:J` Donald's page 0x1b (`Down` again for Goofy), `J` on a row opens its sub page (style: `6:J`, `Down` x n, `J` to write), `Y` is the quick key, `[` and `]` the previous and next character. `KH2_STATE` `{"party_settings": {"2": 4}}` presets the style byte. Log lines: `menu: page 0x1b|0x20|0x21|0x1f`, `menu: member M battle style V (setting byte 0 = ..)`, `menu: member M ability A use setting V (setting byte B = ..)`, `menu: member M item I auto-reload ON|OFF use setting V (byte B = .., slots [..])`. Screenshots (not committed): `local/pa4/donald.png` (page 0x1b), `style-page.png`, `style-after.png`, `ability-page.png`, `item-page.png`.

**Gaps.** The appended second help message; the start rows of the sub pages (the port starts on the row of the current value [I]); the Help overlay (-7); the character list's own row layout; `FUN_00275288`'s parity test is ported as read but its intent is unclear (a quirk of the original [I]); an id absent from 0x352ec0 (no row of the new game's records is one). Not searched: who else writes record `+0xd4..` (the new-game block zeroes it, `FUN_001875c8`).

## 15. M4 as built: the Equipment pages (WP-M4)

Code: `menu_equipment.rs`, a child module of `menu` (`#[path] pub mod equipment;`; `EquipmentPlugin` is added by `MenuPlugin`), `progress.rs` (`equip_weapon`, `equip_form_weapon`, `equip_armor`, `equip_accessory`, `settle_battle_items`, `item_type::ARMOR/ACCESSORY`), hooks in `menu_items.rs` (the member row of page 1 confirms with `Effect::Member`; `ItemsMenu::suspended` hands keys and screen to the new module while page 2 and below are up), `kh2-image/tests/menu_equipment.rs` (disc tests that print the tables and the group boxes).

**Pages [V from the handlers, addresses below].**
- **The equipment list** `FUN_00283be0` (mode flag `+0x1008` = 1; per-category counts at `+0x1010`: `FUN_00283f28`): rows in the order weapon (record `+0`), the drive forms' keyblades, armor slots (`+0x14`), accessory slots (`+0x24`), battle item slots (`+0x34`, reserved ids `+0x44`). The forms listed are the set bits of `FUN_00282bf8 & FUN_00282c60`: a form the player has (`FUN_00282b80`) and that has a second hand (`FUN_00282bc8`: forms 1, 3, 4 = Valor, Master, Final; Wisdom has none), and only for the leader (`FUN_00282cc0`). Each 6-byte entry: id, `+2` carried (1 unless the id is 0 or the row is an empty battle item slot showing its reserved id), `+3` the Auto-Reload flag (battle item rows only: reserved id equals the shown id; an empty slot with nothing reserved compares 0 with 0 and has it set).
- **Page 2** (`FUN_00269580`, built by `FUN_00269028`, redrawn by `FUN_0026adc0`): one list of those rows, nine visible, cursor and scroll remembered (`FUN_00252f98(2, ..)`). `FUN_0026ac70(row)` maps a row to its category by the cumulative counts, `FUN_0026ad08(row)` a forms' row to its form. Confirm opens page `5 + category` (table 0x381970: 5 weapons, 6 forms' weapons, 7 armor, 8 accessories, 9 items); cancel goes to page 1, -4 closes; -10/-11 refuse with one character (`FUN_0026d058`: needs two party members); **-6 (the Y key) only on an item row** (mask 0x200 from `FUN_00269028` / `FUN_0026b9a0`): refused on an empty row (the callback tests the row's id), else it switches Auto-Reload of the row's item: when it is on and a slot reserves it without carrying it (`FUN_0026c968`, `FUN_0026a930`) the question page 3 ("Turn auto-reload function Off?", message 0x846a, No/Yes) comes first, else `FUN_0026c860` sets or clears the reserved id of every slot that carries or reserves the item and the list is rebuilt. -5 (to Abilities) and -7 (Help) are not ported.
- **Pages 5 to 9** (`FUN_0026a528`): the candidates are `FUN_00283a70(list, member + 1, filter)`: the item table in order, rows of the category's type (`FUN_00283810`: the character's keyblade type from the pair table 0x3823d0 for filter 1 -- Sora 2; armor type 14 `0x4000`; accessories type 15 `0x8000`; consumables type 0) that the player owns at least one of, the count byte kept. Pages 7, 8 and 9 add a first row that empties the slot ("icon ---", `FUN_002844e8`); pages 5 and 6 do not. **Page 9 also lists the received consumables** (the 8 ids at save `+0x25f0`, `FUN_00283e50`) the stock has run out of, with a count of 0 (the cell shows `x0`), then `FUN_002845a0` sorts by table position. Eight rows are visible. Confirm: the callback `FUN_0026d058` refuses (sound 4) when the item now in the slot cannot go back to the stock (`FUN_0026c260` = `FUN_001a18b8(id, 100, 1)`, `Progress::room`): pages 5 and 6 whenever the list is not empty, pages 7 to 9 when the slot holds something. Page 8 then runs `FUN_0026ca28`: with a non-empty slot, the swap is refused when AP maximum - AP used - (AP of the item now held - AP of the new item) is below 0 (the `item_stat` AP byte `+7` of both; used = the equipped abilities' costs) and page 4 shows message 0xc22e ("This accessory cannot be changed with current abilities equipped. To equip, first unequip abilities from the Ability menu."), OK closes it. Otherwise `FUN_0026c280(item)` does nothing when the row already holds the item, else the writer of the category (jump table 0x3819cc, read from the instructions at 0x26c338..0x26c414: `FUN_0026c420(item)`, `FUN_0026c4d0(form, item)`, `FUN_0026c588(row - earlier counts, item)`, `FUN_0026c610(...)`, `FUN_0026c698(...)`; the slot is the row minus the counts of the earlier categories) and the status is refreshed (`FUN_00282898`); the page returns to 2. Leaving a pick page (confirm, cancel, -4) runs `FUN_0026c770` (an item carried by a slot takes the reserved id another slot holds for it) and `FUN_001877c8` (refill): `Progress::settle_battle_items`. `FUN_0026c698`'s "carried" test is the list entry's count byte (the closed point of M3's `[inferred]`).
- **The writers** (all `give(old, 100, 0)` then write then `take(new, 100)`; the oracle test shows byte for byte equality): `FUN_0026c420` gives the old weapon back whether it is 0 or not, `FUN_0026c588` / `FUN_0026c610` only when the old id is not 0 and take the new one only when it is not 0. **Found by the oracle:** the game's item lookup answers a null row for id 0 and its zero bytes make `give(0)` bump count slot 0 (save `+0xe28`); the port does not model that (a keyblade is always held in the menu). `FUN_002a1060(member, hand, item, 0)` is a queue of 8 (member, hand, item) triples at `DAT_0036d3c0` for the field to rebuild the held weapon: the app's counterpart is `Abilities::equipment = Progress::equipment()` and `recompute()`, logged as `menu: abilities refreshed`. The Abilities page reads AP from the same record, so it stays consistent.

**Evidence that runs.**
- `progress::tests::equipment_writers_match_the_game`: `FUN_0026c420`, `c4d0`, `c588`, `c610` in kh2-ee on the block the game built (the page's party object faked: `DAT_0036ce90` -> a row whose status object's `+0x238` is Sora's record; `FUN_002a1060` stubbed) against the port over 23 swaps (swap, the same item again, removal, swap back, forms 1, 3, 4): `oracle: equipment writers equal after 23 swaps (weapon, armor, accessory, form weapon; stock counts and slots)`.
- `progress::tests::equipment_writers_swap_with_the_stock_and_keep_the_counts`, `leaving_the_equipment_lists_settles_the_reservations_and_refills` (no disc needed).
- `menu::equipment::tests` (9 tests): the rows, the pick lists (received items at 0), the AP rule, page flows (open, pick, same item, refused swap, notice, Auto-Reload question, wrap and the single-character switch).
- `cargo test -p kh2-image --test menu_equipment -- --nocapture` prints the tables of this section and the camp groups' boxes.

**Layout [the groups are read; the positions inside them are placed from their boxes, there is no clip of these pages].** Header tabs "Items", "Sora" and on a pick page the category's name [inferred]. Page 2: the window 0xe3 then 0xe4, rows of the plates at 0x3819e8 (0x109 weapon, 0x10a/0x10b/0x10c per form bit, 0xfc armor, 0xf8 accessories, 0x106 items; pitch 26), the tag plate left of the first row of a category and of every form row (0x381a08: 0x11b, 0x11c, 0x116, 0x114, 0x118; 0x381a28: 0x11c, 0x11d, 0x11e) with the label messages 0x45b, 0x45d (Valor), 0x3353 (Wisdom, unused), 0x45f, 0x461, 0x463, 0x465, 0x467. The help line is the message after the category label (0x45c "Change main weapon." ... 0x468 "Change Items."): the system table holds them as consecutive pairs and nothing else uses them [inferred; the help table builder was not read]. Pick pages: the list window per category (0x381a48 triples, enter / leave / hold), rows of the plate pair at 0x381a98 (idle 0xd3 0xdd 0xaf 0x9d 0xb8, selected one lower), the help line is the item's description. An empty row shows the icon of the type table 0x382460 (armor 14, accessories 15: unlisted types, icon 0xb; items 0) and `---`; a battle item with Auto-Reload on shows the equipped icon of its type (`FUN_00255520`). Screenshots (not committed): `local/m4-list.png` (page 2), `m4-armor.png` (Armor list), `m4-items.png` (Items list), `m4-after.png` (page 2 after swaps).

**Not done / gaps (kept conservative).** The right-hand window of each page (the item picture window, groups 0x11f/0x120 on page 2 and the picture and strip groups 0x381a68 / 0x381a78 on pages 5 to 9: the picture is `item +0x14` drawn from another file, as in section 13) and the marker group of a battle item waiting for a refill (`FUN_002530a0` returns 3 for it: group 0x7a of the table 0x3807f8; its anchor was not read, groups 0x7a/0x7b are 1 quad at (0, 0)); the NEW tags (`FUN_00293ba0`, `FUN_00293c78` in `FUN_0026bd08`); the help line of the empty row of pages 7 to 9 (`FUN_00284588` / `FUN_00284598`); -5 (to Abilities) and -7 (the Help overlay); Donald and Goofy (the party is Sora alone: no character switch, no status refresh `FUN_00282898` counterpart for members); the forms' availability uses `Progress::owns_form` without the exclusion `FUN_00282b20`.

**Harness.** `KH2_KEYS=3:Enter,3.6:J,4.2:J` opens page 2 (Enter opens the menu, J on Items, J on Sora); Up/Down move, J confirms, Escape/K/Backspace cancel, Y switches Auto-Reload on an item row. `local/state-equip.json` (not committed): `{"weapon": 41, "armor": [67], "accessories": [8], "items": {"42": 1, "43": 1, "480": 1, "481": 1, "68": 2, "78": 1, "9": 1, "10": 1, "38": 1, "2": 2, "3": 1}, "forms": {"1": {"owned": true}, "3": {"owned": true}}, "sora_items": [1, 1, 3]}`. Log lines: `menu: page 0x2`, `menu: page 0x5..0x9 (N candidates)`, `menu: page 0x3|0x4`, `menu: equipped <category> slot S: <old> (stock a -> b) replaced by <new> (stock c -> d), changed true`, `menu: abilities refreshed: equipment [..], AP u/m, item N grants ability A (applied)`, `menu: auto-reload item I ON|OFF (slots [..])`, `menu: battle item slots [(item, reserved), ..]`. Example: `KH2_KEYS=3:Enter,3.6:J,4.2:J,4.8:J,5.4:Down,5.8:J,6.4:J,6.8:J` swaps the Kingdom Key for Hidden Dragon and back (log: `item 41 grants ability 414 (applied)`).

## 16. M7 as built: NEW tags, the -5 switch, the Help overlay, item pictures, Status tabs (WP-M7)

Code: `menu_new.rs` (`menu::new_tags`), `menu_help.rs` (`menu::help`), `menu_pic.rs` (`menu::pictures`), `menu_gfx.rs` (`Node::pic`, `MenuScene::extent`), `menu.rs` (`Switch`, `Menu::switch`, `Menu::carry`, `Menu::help`), the Items, Equipment and Abilities modules, `menu_status.rs`; `harness.rs` keys `X` and `Select` / `Tab`; `kh2-image/tests/menu_m7.rs` (disc tests that print the tables and groups and dump pictures with `KH2_DUMP_DIR`).

### 16.1 NEW tags [V for the rules, F for the look]

The camp keeps two bit arrays over the item ids (0x220 bits). The **seen** array lives in the save block (`FUN_002937e8` = `FUN_001057a8() + 0x31b4`). `FUN_00293808` builds the rest when the camp starts: `FUN_00293de8` sets the **fresh** bit of every owned item (`FUN_001a1cc8`) whose type is 0x12 (a spell, `FUN_00293ea0`) or one of the bytes at 0x382808 (`FUN_00293b60`: 0..8, 10..13, 6, 14..17, 20; negative ends) and whose seen bit is clear; `FUN_00293838` collects what the party wears into a third array that `FUN_00293758` merges into the seen array when the camp ends. A row is NEW when its fresh bit is set (`FUN_00293ba0`; spells `FUN_00293eb0`) and its tag is the **viewed** form when its seen bit is set too (`FUN_00293c78`, `FUN_00293fa8`). `FUN_00293d98(id)` sets a seen bit; the Stock grid calls it when the cursor reaches a NEW cell (`FUN_0025dcf8`), the equipment pick lists on the highlighted row (`FUN_0026c068`), the Abilities page on the highlighted ability (`FUN_0027be78` -> `FUN_00295178`).

**Where the tag is drawn.** Page 0 (`FUN_00251100`): Items (entry 0) when any fresh item of the listed types exists, Customize (entry 2) for fresh spells, Abilities (entry 1) for fresh abilities (below); Status (entry 4, `FUN_00294260`, the summon items at 0x382820 / 0x382830) and the Journal (entry 5, `DAT_0036cedc`) have no source in the app and never carry a tag. The group is `DAT_003807e8` = 0x32 while one fresh member is unseen and `DAT_003807ec` = 0x31 once every one has been seen. The same pair of groups tags the Stock row of page 1 (`FUN_0025aac0`), each Stock cell (`FUN_0025dae8`, groups at 0x381070 / 0x381074), each candidate of the pick lists (`FUN_0026bd08`, 0x3819e0 / 0x3819e4), each ability row and the character row of page 0xf (`FUN_0027b9a8`, `FUN_00278aa8`, 0x382044 / 0x382048); in every case 0x32 is the unseen and 0x31 the seen form. The tag object is a child of the row (`FUN_0029ba00`) placed with `FUN_00138ed0` at the row group's own `x` and `y` (the plate's right end); the group's box ends 4 units left of its origin (`menu_m7` prints it: (-43, 7)-(-4, 19)). The port puts it at the row's anchor plus the row group's `x`, `y` (checked against clip 10 at 0.5 s: the tag ends short of the plate's right end and overlaps a long label such as Customize, as in the footage).

**Abilities** (`FUN_00294470`). The save block holds the list of every ability the leader ever had in his slots (u16 x 0x40 at +0x98 of the seen region) and a seen bit per list index (+0x44). The camp appends the slot abilities that are not in the list, an entry is fresh while its seen bit is clear (`FUN_00294b58`), and an entry whose ability has left the slots is marked seen. The port keeps both arrays in `NewTags` for the session (the original keeps them in the save; the app has no save file for them yet). Tests: `menu::new_tags::tests::*` (fresh sets, viewed form, a seen item is not fresh the next time and worn items become seen, abilities), the type list is checked against the executable (`the_executables_type_list`, `menu_m7::new_tag_tables_have_the_documented_shape`).

### 16.2 The -5 switch [V pages, I physical button]

`FUN_0025ae18` (page 1) -5 sets the target page 0xf; `FUN_002790e0` (page 0xf) -5 page 1; `FUN_00269580` (page 2) -5 page 0x10; `FUN_00279ac0` (page 0x10) -5 page 2. The list mask has the switch bit (0x100) only while the other side's row is on page 0 (`FUN_00253f80`: bit 1 Abilities for page 1 and 2, bit 0 Items for 0xf and 0x10). The sound is kind 2 (the confirm: `FUN_00295bf0` returns kind 2 for -5, -6, -7, -8, -9, -10, -11 before the page's validity callback). The stage's modules hand over through `Menu::switch` (`ToAbilities { second }`, `ToItems { second }`) and `Menu::carry` (page 0's hidden nodes, so the top list does not flash between the two screens). The physical button is the footage's "X": the port reads `X` (harness `X`); Help (-7) is `Tab` (the footage's Select; harness `Select` or `Tab`). Log lines `menu: switch to Abilities`, `menu: switch to Items`, `menu: switch to Items (page 2)`.

### 16.3 The Help overlay (page 0x25) [V]

`FUN_00292088(topic, return page)` (called by -7 on page 0x10 with topic 2 and the return page 0x10, on page 2 with topic 0 for an Items row and 3 for a form's row, on pages 9 and 6 with topics 0 and 3; `FUN_0026a528`, `FUN_00269580`, `FUN_00279ac0`) lists the pages of the topic (`FUN_00292b90`; table at 0x382730, nine entries of 8 bytes: the stem pointer, the story flag id `FUN_001802e0` tests, the topic byte, a flags byte: at_010, at_020 topic 0 (Auto-Reload), mp_010, mp_020 topic 1 (MP Charge), ab_010, ab_020, ab_040 topic 2 (Abilities), sw_010, sw_020 topic 3 (Sub-Weapons)). Each page is the file `menu/us/<stem>_<variant>.bar` (`%s_%c.bar` at 0x3826d8; the variants `r`, `o`, `s` are chosen by `FUN_002928e8` from the story flags 0x8d0, 0x8d1, 0x8db; the port uses `s`, the one every `.bar` has), a BAR with one IMGD entry: **a 512 by 512 8-bit picture with the tutorial text baked in** (the footage's pages are the remaster's redraw of these). The title is the topic's message (0x382780 + 2 * topic, 0x4bd9..0x4bdc "What is Auto-Reload?", "What is MP Charge?", "What are Abilities?", "What are Sub-Weapons?") on group 0xf then 0x10, the counter is message 0x4bd8 ("PAGE") and the format `%d/%d` (0x3826b8) on group 0x12 then 0x13. `FUN_002920d8`: only the confirm (logical bit 4) does anything: it plays kind 2, goes to the next page, and after the last returns to the stored page (`FUN_00292ae0` marks the tutorials seen). Pages 0x25 is drawn over the page that asked; that page's module stops reading keys and drawing while `Menu::page` is 0x25. Tests: `menu::help::tests::*` (the pages of a topic, the file name, and with the disc the nine entries and their 512 by 512 images), `menu_m7::help_tables_and_pages_have_the_documented_shape`. Not drawn: the two prompt sprites (groups 1 and 4 of the layout, `FUN_00292638`: "next page" and "close") and the page fade [GAP]; the app has no tutorial flags, so every page of a topic is listed.

Page 0x1a (Customize) also has Help topics in the original (`FUN_00271078`); not ported [GAP: not read].

### 16.4 Item pictures [V file and id, I placement]

The item window (`FUN_0029ed50`; page 2 and the pick pages `FUN_0026b1c8`, page 0xb `FUN_0025bad0`) shows the item's picture and its stat rows. The picture id is the s16 at item `+0x14` (`Picture` of the `item` sheet, -1 none; `FUN_00283520`); the picture cache (`FUN_002a07c8`, slots at `DAT_0036d3ac`) reads **`itempic/item-%03d.imd`** (the format at 0x3829c0, `FUN_002a0ba0` -> `FUN_00167d20`): 128 by 128, 8-bit IMGD files, 282 of them in the disc (`menu_m7::item_pictures_decode`; item-001 is the Potion jar of clip 6 at 12.5 s). `FUN_00283578` waits for the cursor to rest (`DAT_0036d174`, counted down from the float at 0x3823c4 = 0x4170 high word; the delay was not converted) and `FUN_002830f8` draws the slot as a sprite scaled to `unk20 / 100` of the window group's width (100 % where the window has no frame group). The picture windows of the pick pages are the triples at 0x381a78 (weapons 0xd5/0xd7/0xd6: box 376..505, forms 0xdf.., armor 0xb0.., accessories 0x9e.., items 0xb9..: box 257..414 by 72..230), page 2's are 0x11f and 0x120 (box 324..505 by 72..326), page 0xb's 0x131 and 0x132; the strip below the picture is 0x381a68. Port: `Pictures` (`menu_pic.rs`) loads the IMD on first use and keeps the texture; the picture is centred across the window's last-frame box and 10 units under its top [inferred: the sprite's anchor was not read]. Seen in `local/m7/eq1.png` (the Weapons list: the keyblade picture in the window with the strip line below) and the logs `menu: item picture N loaded (128x128)`. Not drawn: the stat rows of the same window (`FUN_0029f1f8`, `FUN_0029f890`: AP, Strength, Magic, Defense and the four resistances from the item's `item_stat` row, the ability of the item as the ninth row; groups 0x20..0x26 and 0x76..0x7c, labels at 0x382990 / 0x3829a8) [GAP: the rows' layout was not placed].

### 16.5 Status: the forms' and summons' tabs [V numbers, I layout]

`FUN_002673c0`'s forms case (0x267788) and summons case (0x2678dc), read from the instructions: the level (form record `+2`; the summon byte `+0xdce`), the experience in total (`FUN_001d3c98`, `FUN_001d3d48`: the record's experience (`+4`, `0x340e4c`) plus the `fmlv` Exp of the levels 1 .. level-1), the experience to the next level (`FUN_001d3c38`, `FUN_001d3cf8`: the Exp of the row at the level minus the record's experience while level < cap, else 0; cap `FUN_001d3bd0` / `FUN_001d4e68` = 2 + the owned form items (summon charms), 7 instead of 6) and the gauge row `level + 2`. The forms' second column is the leader's strength, magic, defense and the four resistances. The tab list uses the group sets at 0x380870 for these tabs and the names `0x381a38 + 2 * form` and message 0xc5b1. The summons' tab lists, besides its four rows, bit `10 + j` for each summon charm j held (`FUN_00268758`); what those rows print was not read (the case writes no buffer for them): the port draws their labels and an empty value [GAP]. The party members' tabs: 19.1. Tests: `menu_status::tests::a_forms_experience_is_the_record_plus_the_rows_below_its_level`, `the_cap_is_two_plus_the_owned_items_and_seven_instead_of_six`, `the_forms_and_summons_tabs_have_their_own_rows_and_values`. Screenshots `local/m7/st0.png` (Valor tab on the list) and `st1.png` (its numbers).

### 16.6 Party cards and the HUD items deck

**Cards.** `FUN_00295bc0` returns `DAT_0036d1f8`, the object `FUN_00295720` creates (`FUN_0023a018`, vtable 0x36ca50): the **3D party models** standing in the world (`FUN_0023a520` loads the member's model and animation 0x8b, the head and eye textures, the per-slot lights); it is not the name/LV/HP/MP card. The card's drawing code was not found (searched: `FUN_00250a00`, `FUN_002506b8`, `FUN_00251100`, `FUN_0023a848`, `FUN_0023b3a8..0023b5d0`). The cards stay as built in M1 (shapes and colours from clip 10); the party is Sora alone. [GAP]

**Items page overlapping the Magic row (accept scenario `items-page`).** It is the field's command deck (`hud_menu.rs`, the Items sub list under COMMANDS), not a pause menu page: the header plate of ITEMS is drawn over the Magic row. Resolved by WP-F2 item C as the original's own look: the game draws every stacked list in full and places each list's header by its own row count (docs/hud.md, command menu "Structure"), so a one-entry sub list's ITEMS plate sits where the parent's third row is. No change.

**Update (WP-MN1): the x values belong to the 3D preview.** `FUN_00250a00` passes 0x2f, 0x91, 0xf3 (minus `(n * 98 - 98) / n`) to the
preview object's slot setup (vtable +0x18 = `FUN_0023a520`), not to a card: they are the centres of the members' slot cameras, see 16.8.
The cards' x (`menu::card_x`) keeps them plus the clip-10 fit constant: each card's centre lies 24 units right of its member's preview
centre, which the reference frame (8.webp, Ballroom) also shows. The cards' own drawing code is still not found [GAP, searched as above
and `FUN_0023aca8`, the preview's draw, which draws only the models].

**Card look (WP-MN3, user request 2026-10-13, "make it look like the original")** [F, measured on the HD reference 8.webp, 1920 x 1080].
Scale of that frame: 2.6 pixels per unit (the list's row pitch of 26 units is 67.6 px there); cross-check: the card top lies 67.5
units above the bottom panel's top edge there and 67.6 units in the app. Measured, in units from the card's left or top: card 99.6
wide and 71.3 high; cards 103.8 apart (gap about 4); one dark see-through body (colour about (2, 2, 4), alpha 0.84) for plate and
rows alike (no lighter plate fill, no gradient); a light rim 2.5 thick along the top, its left end cut at 45 degrees from (0, 8.5) to
(7.5, 0); a second light rim 2.2 thick at y 24.2 where the plate ends; a dim rim 2.3 wide down the left edge; name cap height 11.9,
its cap top at y 8, centred; LV, HP and MP centres at y 34.7, 49.2 and 64.2, labels 4.6 from the left, values 5 from the right;
cap heights 9.1 (labels, LV value) and 8.6 (HP and MP values). The two lines under HP (y 54.8, 2.1 thick) and MP (y 70, 2 thick) span
4.2 .. width - 4.4 and are **gauges**: the lit part is the share hp / max hp and mp / max mp (the original frame shows Donald's MP
20/100 and Goofy's 33/100 with a short cyan part and a dim track to the right). Colours (display values): yellow (244, 218, 40), green
(0, 200, 0), cyan (16, 200, 232), name white (232), rims 100 to 120 on top and middle, 45 on the left. The app: `menu::card`, the `at::CARD_*`
constants, `card_x` (pitch 104 around the middle card, the row shifted 4 left so the third card ends at the canvas edge: the middle
card keeps its place over its member's preview, the outer two are 6 units further out; the previews keep the code's 98).
Text sizes re-fitted by WP-MN5 (below, "Card text, WP-MN5"). Not matched [GAP]: the original's rims are slightly bevelled
(brighter centre), ours are flat.

**Card text, WP-MN5** [F, measured]. The earlier "20 % wider" note came from the 8.webp frame (2.6 pixels per unit); the clip frame
`10_menu_open_close_0.5` has a different scale: its content is the 4:3 picture at **2.163 pixels per unit**, x centre 960, y 0 at
pixel row 90 (row pitch 56.2 px = 26 units, card 98.5 units wide, `CARD_TOP` 259 lands at row 651). A pixel-box measurement (bright
green / cyan / yellow pixels per glyph) on that frame and on the app's shot at the same unit scale gave, for the original: HP and MP
digits 8.3 to 8.8 high, 7.9 to 8.3 wide, **10.6 apart**; "LV" 8.8 high and 14.3 wide. The app at the old sizes (12.0 / 12.5): digits
8.1 high, 7.5 wide, 9.96 apart; "LV" 10.0 high and 14.3 wide. So the digits were about 6 % **small**, not too wide, and the LV row
was 13 % too tall at the right width. Now `at::CARD_VALUE_SIZE` 12.4 (digits 8.7 high, 8.1 wide, 10.6 apart) and `at::CARD_LV_SIZE`
11.0 with `Token::Width(113)` (`at::CARD_LV_WIDTH`; "LV" 8.7 high, 13.7 wide). The HP and MP labels keep 12.5 (their glyphs
touch the gauge lines in the frame, not measurable). The card widget's own draw code is still not found [GAP, searched above], so
these are fits to the footage, not rules from the code. The name keeps its size.

### 16.7 Harness

`KH2_KEYS` knows `X` (-5) and `Select` / `Tab` (-7). Examples: `KH2_KEYS=6:Enter,6.5:Down,7:J,7.6:X,8.4:X` (Abilities, then to Items, then back); `6:Enter,6.5:Down,7:J,7.6:J,8.2:Tab,9.2:J` (the Help overlay from the Abilities list, then its second page); with `KH2_STATE=local/m7/state-equip.json` (the state of section 15) `6:Enter,6.6:J,7.2:J,7.8:J,9:Down` shows the pick list with the pictures and the NEW tags. Log lines: `menu: new tags: new items N, new spells N, abilities known N (fresh N)`, `menu: help topic T (N pages), return to page 0x..`, `menu: help done, page 0x..`, `menu: item picture N loaded (WxH)`.

### 16.8 Party cards and the 3D party preview (WP-MN1)

**Cards.** One card per party member in slot order, up to three side by side (`FUN_00250a00` walks slots 0..2 of the party object and
caps the count at 3): Sora, then the members standing in the field by their party slot (`party::PartyMember`). Name: the member's
message (`0x37ba90[member id]`, `FUN_001b5890`, the table `menu_status` also reads); LV: the member's record `+0xf`
(`Progress::member(id).level`); HP and MP: the live values (Sora's entity; the members' `PartyMember::hp/hp_max/mp/mp_max`, the HUD's
source). The card's look since WP-MN3: see the "Card look" paragraph before 16.7.

**Preview** (`menu_preview.rs`) [V for the numbers, I for the rendering]. `FUN_00295720` creates the object `DAT_0036d1f8`
(`FUN_0023a018`, vtable 0x36ca50: +4 update `FUN_0023a920`, +8 draw `FUN_0023aca8`, +0x18 slot setup `FUN_0023a520`) when a menu scene
starts. Per slot (`FUN_00250a00`): `FUN_0023a520(slot, model | 0x20000000, 0x8b, 2, x, y)`: x as above, y = 0x60, or 0x3d when the save
byte `+0xc` is 10 (`FUN_0027c218`); the slot's projection `FUN_0023a848` -> `FUN_00106ba0(512, 0.8, 0.866667, x + 2048, y + 2048, 1,
1.677e7, 1)` and `FUN_00106c48(0x37f49c = 40 degrees horizontal)`; the field camera's centre is (2048, 2048) (`FUN_001078f0`, 0x37812c),
so the slot is centred x, y from the screen's middle. View: eye (0, -50, -800) (0x37f468, 0x37f46c), look-at (0, 0, 55) (0x37f470), up
(0, -1, 0) (0x37f474). Model: translation (0, 0, -100) (`FUN_0023b348(0x3806f8)` writes slot `+0x678`, the z of the translation row that
`FUN_0023a920` copies into the matrix), rotation 0 (`FUN_0023b3a8`, `FUN_0023b3c8`: slot `+0x660`, `+0x664`, degrees, `FUN_0011b568`),
scale 1 (0x37f478), colour (0x60, 0x60, 0x60) of 0x80 (`FUN_0023b448`), alpha from the panel outline object's colour (`FUN_0023b4e0`
with the object `DAT_0036ceac` that `FUN_0023b4b8` attaches). Motion: `FUN_001a7a48(set, 0x8b, 2)` = entry 0x8b * 4 + 2 of a player set
(fallback +3, +2, +1, +0) or entry 0x8b of another set; it loops; the frame advances by `FUN_0014cbe8` (the frame step, 1 per tick in a
menu, 1.6.1); `FUN_0023a920` switches to the variant motions `+0x6d8`/`+0x6dc` after a random 10..29 x 0x37f4a8 frames when they exist
(they are not set on page 0: not ported). The model's draw offset by an attached object (`+0x14e1`, set only by the Party page
`FUN_0026d8a8`) is not used on page 0. Computed: the slot centres are menu x 238, 336, 434 for three members and the model's feet land at
menu y 314 (`menu::preview::tests::the_slots_sit_where_the_original_puts_them`), i.e. the feet are behind the cards' upper part.
**WP-MN5 re-measured the placement against clip 10** (frame 0.5, 2.163 pixels per unit, y 0 at row 90; the app's shot of the same
area scaled alike, `tt03` with `KH2_KEYS=3:Enter`, shot at 7 s): Sora's hair top is at y 131.8 in the clip and 133.3 in the app, his
centre at x 237.5 (the code says 238), his belt at 224 and 223.5; Donald's head top 212.7 against 220.5 and Goofy's hat 145.6 against
128.4 differ by their poses (the clip stands in a different frame of the idle loop), their x centres agree. So the position and the
scale of the code's camera match the footage within about 2 units for Sora; the feet are hidden behind the cards in the clip too (the
cards' top is at y 259; the legs show through the dark card bodies in the 8.webp frame). The earlier "feet at about y 265" came from
8.webp (a different pixel scale), where the legs vanish behind the cards, not from a visible foot; nothing in the clips shows a foot
at 265. **No change to the rule.** One real difference found in the code: the slot y is 0x3d, not 0x60, when byte `+0xc` of the save
block (0x33e860, `FUN_001057a8`) is 10 (`FUN_0027c218`); world index 10 is `lk` (Pride Lands, the table at 0x349dc0): the models
stand 35 units higher there (feet at 279). The app passed `false` always; `menu_preview::in_world_10(area name)` now passes the
area's world [inferred: that byte is the world the player stands in; `FUN_00149df0` saves it and writes 0xf (the world map)]. Test
`menu::preview::tests::pride_lands_raise_the_slot`. Side-by-side: `local/mn5-cmp-feet.png` (not committed).

Port: a camera of its own (render layer 9, `Camera3d` with the original's vertical focal length 762 menu units, the 512 x 416 buffer
shown 4:3; 703 horizontally, applied as a horizontal scale of the models) renders the members into a 1024 x 896 picture of the canvas,
drawn by the menu as a node right after the panel outline: the models stand in front of the dimmed field and the panel's scan lines,
behind the rows and the cards. The camera and the models exist only while page 0 shows (the open and the close, WP-MN5: see "Fade"). One perspective replaces the camera per slot: each
model is shifted sideways to its slot centre [inferred: exact at the model's depth]. The models' motion is stepped by the preview
itself (the simulation and `skel::animate` are stopped while the menu is open). Log lines `menu: party preview slot S of N: <model>
motion Some(558) at menu (x, y), depth 702`, `menu: party preview removed`; `KH2_NO_MENU_PREVIEW=1` turns it off, `KH2_PREVIEW_YAW=<deg>`
turns the models (harness aid). Screenshot (tt03, three members): Sora, Donald and Goofy face the camera between the list and the
right-hand frame, lower legs behind the cards.

**Fade (WP-MN5).** The models' alpha is the panel outline object's colour alpha (`FUN_0023b4e0` reads `DAT_0036ceac`, the object
`FUN_00250a00` builds with the groups 0x2e / 0x2f / 0x30 of `camp.2ld` sequence 1). The preview picture's node takes that alpha every
frame (`menu_preview::fade`, from the outline node's quads: `alpha_of`; the picture's quad alpha is `Node::label_fade`, `menu_gfx`),
and the camera now lives through the close (`Phase::Closing`) and is removed when the menu is closed. Measured with
`KH2_MENU_LOG_FADE=1`: `menu: party preview alpha 0.06 .. 0.94 (phase Opening)` in 16 steps of 0.0625 (one per tick), `1.00 (phase
Open)`, then `0.86, 0.72, 0.58, 0.43, 0.29, 0.15, 0.00 (phase Closing)` in 7 ticks. One alpha on the finished picture instead of per
model: models do not blend through one another (the original's per-model alpha would show a body through an arm). The app's skeleton
alpha path (`enemy_ai.rs`, the look-around fade) was not used: it fades each material, the picture alpha does the same with one
number [inferred: the same look]. Screenshots (`KH2_KEYS=3:Enter,5:Escape`, `KH2_SHOT_AT` 3.15 and 5.05): the models are see-through
at both, full in the hold.

**Face rings above the dim and under the frame (WP-MN5 gap D, done in WP-MN6).** In clip 10 the party faces (the HUD rings at the right edge) stay bright over the dim and sit **under** the menu's frame (the bottom panel cuts the third ring). The port now has four cameras in this order: world (0), HUD (10, layer 7), the **dim** (11, `menu_gfx::DIM_LAYER` 9: a black `Sprite` whose alpha is `MenuScene::dim`, no longer a quad of the menu scene), the **ring panels** (12, `hud_gfx::RING_LAYER` 6) and the menu (13, layer 8: the frame, plates, text, cards). `HudScene::ring_panels` marks the HUD nodes of a ring panel and the HUD draw gives their batches the ring layer: a node tree anchored to the right screen edge with three or more descendants (Sora's circle with HP ring, MP bar, drive gauge and face from `hud_gauge`; the party circles with three rings and a face from `hud_party`). [inferred: the structural rule stands in for an explicit flag, which would need a line in `hud_gauge.rs` and `hud_party.rs`; the clip shows rings bright and the command deck dim, the original's draw order of the menu objects was not read; searched earlier: `FUN_00250a00`, `FUN_002506b8`.] The rings are also drawn by the ring camera when no menu is open (no visible change: nothing else overlaps them). Screenshot `KH2_KEYS=2:Enter KH2_SHOT_AT=3` on `tt04`: Donald's and Goofy's rings bright at the right edge, Sora's cut by the bottom panel's top edge; the command deck stays dim. Open: the menu's open and close fades of the rings (in the clip the rings do not fade with the dim; here they never dim).

## 17. Growth and the other equippable abilities as menu rows (WP-G8)

Code: `abilities.rs` (`Abilities::follow_progress`, the `follow_progress` system, the `abilities: growth level ...` log), `progress.rs` (`upgrade_ability`), `menu_abilities.rs` (`infos`).

**The original's rule [V].** High Jump, Quick Run, Aerial Dodge and Glide are ordinary ability items (type 0x13, kind 3 = the Growth group of page 0x10), four rows each: ids 94..97, 98..101, 102..105, 106..109 (LV1, LV2, LV3, MAX), bits 2..17, AP 2, 2, 3, 3 per level on this disc. They sit in the member's 64-slot list (`record + 0x54`) like any other ability, are toggled by `FUN_0027d8b8` (bit 15) through the page's Equip / Don't Equip prompt or the quick key, and cost AP by the same rule (`FUN_0027d6c0`). `FUN_001c0e50` applies a slot only when bit 15 is set and `FUN_001c1340` takes, per group, the highest set bit of the table at 0x37bda0: **a growth ability's level is the highest equipped level of its group**; an unequipped level counts for nothing. The controller reads it through `FUN_001c13b8` (Quick Run `FUN_00178108` slot 1, Glide `FUN_00178290` slot 3, High Jump `FUN_0016bd98` slot 0; docs/moveset-selection.md). Sora obtains them from the drive forms' level-ups: `FUN_001d3da8` calls `FUN_001d3f18(record + 0x54, 0x40, form, display level - 1)`, which replaces the first slot holding the chain's id one level below (0x37c556) with the next level and **keeps that slot's bit 15**; the first level fills the first empty slot, so it arrives unequipped (`progress::tests::growth_upgrade_matches_the_game`, kh2-ee: `oracle: FUN_001d3f18 equal for 50 calls`). The new game gives Sora none of them (`FUN_001c0cb0(0)`, docs/progress.md); the app baseline does not either.

**What the port does.** The menu already built a row for every slot with an ability item (`Table::build`, section 12); the growth rows are such rows (group Growth, the item's own name, help text and AP). What was missing was the path from the record to the controller: `Abilities` copied Sora's list once at start, so a level-up's gift or a later change of the record did not reach the block. Now `Abilities::follow_progress` (system `follow_progress`, FixedUpdate before `sync_controller`, run when `Progress` changed) copies Sora's list, equipment and the app's Dodge Roll level from `Progress` whenever they differ and rebuilds the block; the harness's additions (`KH2_ABILITY_IDS`, `KH2_ABILITIES`) stay after the copy. `sync_controller` then writes `sora::Growth` and logs `abilities: growth level High Jump a, Quick Run b, Aerial Dodge c, Glide d, Dodge Roll e (from Sora's equipped abilities | the drive form's own list | the KH2_GROWTH override)` on the first tick and on every change. `KH2_GROWTH` stays an override outside a form (docs/movement.md section 2). `Progress::upgrade_ability(member, old, new)` is the port of `FUN_001d3f18` for the form level-up (its call site is `prize.rs` `form_exp`, outside this WP: see the report; until it is wired the level-up only logs the upgrade). Tests: `abilities::tests::growth_level_is_the_highest_equipped_level`, `the_block_follows_the_record`, `progress::tests::growth_upgrade_keeps_the_equipped_bit_and_matches_the_array_form`, `growth_upgrade_matches_the_game`.

**Dodge Roll** is not an ability on the USA disc (no item row, no name message; docs/movement.md section 2, docs/final-mix.md): it stays the app field `Progress::dodge_roll` (baseline level 1, `KH2_STATE` `"dodge_roll"`), with no menu row. A row for it would need a name and an id the disc does not have [GAP, kept out on purpose].

**Every ability row of the disc** (`menu_abilities::tests::every_ability_row_of_the_disc_is_a_menu_row`, `cargo test -p kh2-app --bin kh2-game -- every_ability_row --nocapture` prints the full list): 145 rows; each is a row of its group with its own AP when Sora holds it (checked for all 145 in 64-slot pages), every name is in `sys.bar`. Rows with AP 0 (the drive forms' actions such as Brave Shot, Wisdom Shot, the dummies, Final Arcana) cannot be toggled (`FUN_0027dc30`), as in the original. What the app reads of each (78 of 145): the 16 growth rows (growth levels), the 27 stat abilities of `FUN_001c0f40` (the block), the attack table gates of Sora's moveset (Guard, Upper Slash, the slashes and aerial actions 262..273 and their form copies), the limit gates (198..211, `limit.rs`), Magic Lock-On (403). **Not read by the app yet [GAP: their readers test the bit through `FUN_001768a8`, not traced in this WP]:** Scan (138), Aerial Recovery (158), Auto Valor / Wisdom / Master / Final / Summon (385..389), Leaf Bracer (402), No Experience (404), Second Chance (415), Once More (416), Auto Limit (417), Auto Change (418), Auto Healing (420); Donald's and Goofy's actions (165..168, 176, 177, 187, 190, 195, 215, 423..448: the party is Sora alone); the forms' own actions (216..225, 246, 247, 257..261, 519..530: they gate the forms' movesets, which this test does not load [inferred]). They are equippable and cost AP in the menu; equipping them sets their bit and nothing else.

**Harness.** `KH2_STATE=local/g8/state-swap.json` (`{"abilities": [94], "abilities_off": [98], "dodge_roll": 1}`), `KH2_KEYS=6:Enter,6.5:Down,7:J,7.6:J,8.2:Y,8.8:Down,9.4:Y` (open Abilities, Sora, take High Jump LV1 off, equip Quick Run LV1): `abilities: growth level High Jump 1, Quick Run 0, ...` at start, `abilities: High Jump LV1 (id 94, slot 0) taken off, no longer equipped; AP 0/2`, `abilities: growth level High Jump 0, Quick Run 0, ...`, `abilities: Quick Run LV1 (id 98, slot 1) equipped; AP 2/2`, `abilities: growth level High Jump 0, Quick Run 1, ...`; screenshot `local/g8/menu-swap.png` (the Growth group with High Jump LV1 unequipped and Quick Run LV1 equipped, AP 2/2). The movement reads it: with `local/g8/state-growth.json` (all four LV1 equipped) `KH2_KEYS=5.5:W:3.5,7.6:K:0.6` logs `sora quick run: Square held 10 ticks, Quick Run level 1`; with `state-swap.json` (Quick Run off) `sora dodge roll: Square held 10 ticks but Quick Run is not learned (level 0)`.

## 18. Save as built (WP-M8: menu kinds 7 and 8, `save.2ld`)

Code: `menu_save.rs` (child module `menu::save`, registered in `MenuPlugin`'s tuple), hooks in `menu.rs` (`input` leaves the pad to the Save menu while `SaveMenu::is_open`, and the field stays frozen), `progress.rs` (the slot file block), `harness.rs` (key `F2`), test `crates/kh2-image/tests/menu_save.rs` (dump of `save.2ld`).

### 18.1 What the original's pages are [V unless marked]

- **Opening.** `FUN_00138758` (the Start gate of 1.1) calls `FUN_0024f330(request block, party)` when `FUN_001bf030` finds a pending start prompt object: the table at `0x1d5dec8` (five blocks of 0x40 bytes, `FUN_001beff8`) or `FUN_001bef10(6)`; a save point's prompt `FUN_001ad9a0` (state 5) calls it too. `FUN_0024f330` stores the block and the party pointer (`FUN_00254a18`: `DAT_0036cf00/04`), prepares the sound (`FUN_0023e860`) and requests menu kind 7 (scene 0x32). If the block's byte `+0x25` is clear it also clears the word `+0xfc` of `FUN_0014a918`'s object.
- **Kind 7** (`FUN_002540c8` init, `FUN_00254210` term, task `FUN_00254380`, read from the instructions: it runs `FUN_002543e0` (creates two layer objects), then `FUN_00254548` every frame): a wrapper. `DAT_0036ceec` is the sub-kind (8 at the start), `DAT_0036ceee` the next sub-kind the page requested (`FUN_00254a08(n)`); `FUN_00254548` ends the kind when the sub-kind is negative (`FUN_001038a0`, back to the field) and otherwise lets `FUN_0024f000` start the requested one. `FUN_002544b8` copies the block's `+0x14` (a header group) into the layer object (`FUN_00254800`: group 0x7531 of the loaded file) and the block and party into kind 8 (`FUN_0026f480`).
- **Kind 8** (`FUN_0026f420` / `FUN_0026f468`; task `FUN_0026f4a8`: `FUN_002703c8` loads the file index 0 (`FUN_0024f6e8(0)`), `FUN_0026f508` builds, then `FUN_0026fd20` each frame until it returns 1): the **pointer list** page. `FUN_0026f0d0` (init) creates the objects: the list widget `DAT_0036d010` (`FUN_002969b0(0x38)`), three row objects, a hand, a picture object (`FUN_0029e308`, group 0x7546) and the table `DAT_0036d018` of nine action words. `FUN_0026f698` fills the list from the block: `+0x16` the header group (`FUN_0026f8a8(0, group)`), `+0x18` the information message (a second plate, group 0xe), the party object `DAT_0036d020` (group 0xd and a card, `FUN_0026f948`), then per option `i < byte +2`: a u16 action code at `+4+4i` (0 = no row) and a message at `+6+4i`, one row object each (`FUN_00299f30(0xc, ...)`); the action code goes to `DAT_0036d018[row]`; the cursor starts at `DAT_0036d024` (the last chosen row). The page handler `FUN_0026fda0`: list result -2 or -4 closes (`FUN_0023e8b0`, `FUN_00295ba0`); a row runs by action code: **1** requests sub-kind 2 (`FUN_00254a08(2)`: the file screens), **10** sub-kind 3 (the Journal), **0x11** sub-kind 1, **3** and **4** start field events 0x8b and 0x8d (`FUN_001b9c58`), **5** `FUN_00222820` (a flag and `FUN_001d3a48`) then closes, **6** closes, **0xc** (when `FUN_001b9b38`: the save object has a position table) shows the picture of page 1 (`FUN_0026ff98`: group 0x754b at the position the save object holds, `FUN_0027f478`; page 1 waits for a confirm or cancel, `FUN_0026fff8`, sound kind 9), anything else calls the party object's own handler. After a row the cursor is stored and the page closes (except 0xc). Sounds: `FUN_00295bf0` (cursor kind 1, confirm 2, cancel 3, refused 4); the open sound is kind 0xb (`FUN_0026f0d0`). The block's rows for the "Save" prompt are built by the save object [GAP]; the code-1 row is the one that saves.
- **Kind 2** (`FUN_002589f0` init: `FUN_002bbed8`, `FUN_002bbef0(0x13)`, `FUN_00254bf0`, `FUN_002bb790` = the memory card driver; task `FUN_00258b38` (no `.c`), build `FUN_00258bf0`, per frame `FUN_00258cf8`) is the **file screens** page machine on `save.2ld` (loaded by index 1). Page ids by handler: 1 `FUN_00258eb0` (a two-row list, row index = mode: `FUN_0023d2f8(row)`), 2 `FUN_002590c0` (the memory card check: a seven-way jump table at `0x380e3c`), 3 `FUN_00259728` (the file read and write sequence: states 10..0x13 call the memory card status `FUN_00230ea0/002317c8/00231788/00231808` and the slot data `FUN_0023d538`, `FUN_0023da78`, `FUN_0023dad8`), 4 and 6 `FUN_00259288` (information boxes: messages `0x8a8c`, `0x8a89`, `0xb85d`: memory card errors and the check), 0xa `FUN_00259648`, **0xb `FUN_00259b78` (the file list)**: a list widget `DAT_0036d50c` over the slots; -2 backs to page 1, -4 closes; a row calls `FUN_0023ddb0(slot)` (does the slot hold a file) and, when it does, shows a message (`0x8a91`) and goes to page 0xf; **0xf `FUN_00259e88`** (the Yes/No question: states 0x32..0x34 write, sound kind 0xd, then page 0x14), **0x14 `FUN_0025a158`** (waits for the field object's word to be 4, ends the kind: `FUN_00103b18(2)`, `FUN_00295ba0`). `FUN_0025a1b8` is the per-page draw (a jump table at `0x380ebc`; the list's hand `FUN_0029a9c0`, the plate group `0x754a`). The memory card driver itself (`FUN_002bb*` to `FUN_002bd*`, `FUN_00230*` to `FUN_0023d*`) is out of scope: the port saves to a folder of its own.
- **Sounds** (`FUN_0023e828(kind)`: kinds below 6 go through the s16 table at `0x36cb48` {6, 7, 8, 9, 10, 8}, then the u16 table at `0x37f654`): cursor kind 1 = id 1, confirm 2 = id 2, cancel 3 = id 4, refused 4 = id 5, open 0xb = id 3, the page-1 picture's confirm 9 = id 4, the write's completion 0xd = id 0x24. The port logs them as `menu se 0:<id> muted` (`KH2_SOUNDLOG=1`; the menu's sounds stay off, `menu_sound::MENU_SOUND_ENABLED`).
- **Texts** (`msg/us/sys.bar`, bit 15 of an id ignored): "Save" `0xa7e`, Yes `0xa7c`, No `0xa7d`, the pointer list's help "Save data." `0x3858`, the check box `0xa89`, "Select file to save." `0xaa1`, "Save new file." `0xa9d`, the QUESTION `0xaa4` "Overwrite existing file?", the write boxes `0xa90` (saving) and `0xa92` (overwriting), the slot name `0xa86` "MEMORY CARD slot 1", LV `0x39fc`, Munny `0x37db`; a box's header ("INFORMATION" with icon 10, "QUESTION" with icon 11) is part of its message. **Not on this disc:** "Title Screen", "Save data to storage device." and "Delete file" (the footage's remaster texts): the PS2 list has no such row; the port draws only the rows the disc's texts support.

### 18.2 The picture file [V: test `menu_save`]

`menu/us/save.2ld`: image 0 (256x256: the frame, rows, boxes, digits), image 1 (512x256: nine pictures of 255x62, not used), image 2 (256x256: the world logos). Sequence 0 (88 groups, image 0) holds everything the screens draw; sequence 1 (9 groups, image 1); sequence 2 (21 groups, image 2: groups 1 to 18 are the logos of Agrabah, Beast's Castle, Port Royal, Disney Castle (4, 5, 7), The World That Never Was (6), Hollow Bastion (8), Radiant Garden (9), Olympus Coliseum, Pride Land, Atlantica, Land of Dragons, Halloween Town, 100 Acre Wood, Space Paranoids, Twilight Town (17), Timeless River (18); 19 and 20 the Gummi ship). The LAYD's ten layers sit in three layouts: enter (backdrop 7, header and bottom panels with the SAVE word 20, area rule 69), hold (8, rail 18, 21, 70), leave (9, 22, 71). Other groups of sequence 0: 0 to 3 the tab pill under the header (0 red, loops; 1 and 2 black), 4/5/6 the box (enter, hold, leave; the one box is the INFORMATION and the QUESTION), 10 to 13 and 48 to 50 the hand (12: a loop of 89 frames), 27 to 36 Munny digits, 37 to 46 Time digits, 47 the colon, 51 to 53 and 77 to 80 row pills (77 red with outline and glint, 78/79/80 black enter/hold/leave; pitch `unk20` 26), 54 to 65 the scroll bar and its arrows, 66 to 68 the Munny plate, 72 to 74 the Time plate, 81 to 83 the file panel, 84 to 87 a file row (84 red, loops; 85/86/87 black enter/hold/leave; pitch 60; the ring for the file number is in the art). `cargo test -p kh2-image --test menu_save -- --nocapture` prints all of it; `KH2_SAVE_DUMP=<dir>` rasterises every group.

### 18.3 What the port does

Open: `F2` (harness key `F2`; it stands for the field Save command, for which the app has no object [inferred]) with the camp's gate (the player in control, no event or mission layout, no menu or PAUSE band): the picture dims as the camp's, the frame's enter groups play, the open sound id 3. **Top:** one row, "Save", selected, with the hand and the help `0x3858`; confirm (id 2) goes on, cancel or Start closes (id 4). **Checking:** the tab under the header, the information box with `0xa89`, 45 ticks [inferred from the footage: about a second]. **Files:** four rows from the slot folder; a row shows the file number, the world's logo (`logo_group`), LV, Munny, the play time as `h:mm` and the area's name (`place.bin`), or "Save new file."; Up and Down move the hand and wrap; confirm on an empty row writes at once, on an occupied one asks (the question starts on No [inferred from the footage: the hand rests on No]; Left and Right move between Yes and No); cancel on the list returns to the top page. **Writing:** the box with `0xa90` or `0xa92` for 45 ticks, then the slot file is written, id 0x24 sounds and the menu closes (`FUN_0025a158` ends the kind after the write; the remaster's clip shows the list once more, which the disc code does not). Log lines: `menu: save open (dir ...)`, `menu: save page 0 (pointer list, 1 row)`, `menu: save page 2 (checking ...)`, `menu: save page 0xb (file list, slots [..])`, `menu: save file cursor N`, `menu: save slot N is occupied: overwrite question`, `menu: save question Yes|No`, `menu: save slot N writing (overwrite|new file)`, `menu: save slot N written (<path>)`, `menu: save close`.

**The slot files.** `progress::save_dir()` is `KH2_SAVE_DIR` or `local/saves` (git-ignored; nothing else is written, and the PS2 memory cards are never touched). Slot `n` is `slot0n.json`: a JSON object in the `KH2_STATE` schema (docs/progress.md): `block` (the party block as hex), `munny`, `option_word`, `game_mode`, `dodge_roll`, `new_seen` (the NEW tags' seen words, save `+0x31b4`), `moogle`, `shop_seen`, `shop_visits`, `record_bits`, `story_raw` and `world_points` (the state kept outside the block, docs/progress.md; WP-PRG1), plus `format` and `slot` (the row's data: level, munny, play time in seconds, area code; the state reader ignores them). `KH2_STATE=local/saves/slot01.json` restores the state: `Progress::apply_json` puts the block back byte for byte (test `progress::tests::a_slot_file_restores_the_block_byte_for_byte`).

### 18.4 Gaps

- The memory card driver and the pages' card states (1, 3, 4, 6, 0xa and 0x14 beyond the structure above): the port has no card. Searched: `FUN_00258cf8`, `FUN_00258eb0`, `FUN_002590c0`, `FUN_00259288`, `FUN_00259648`, `FUN_00259728`, `FUN_00259b78`, `FUN_00259e88`, `FUN_0025a158`, `FUN_0025a1b8`; their callees `FUN_002bb*` to `FUN_002bd*`, `FUN_00230ea0`, `FUN_0023d*`, `FUN_0023da78` were not read.
- Who builds the request block (the save object; the remaster's option list "Save", "Title Screen"): not found. Searched: callers of `FUN_0024f330` (`FUN_00138758`, `FUN_001ad9a0` only), `FUN_001bef10`, `FUN_001beff8`; the block's writers around `0x1d5dec8` were not scanned. The disc has no "Title Screen" text, so the port shows the one row.
- The NEW tags' seen words are not saved by the menu: `new_tags::NewTags` keeps `seen` private and has no getter (`menu_new.rs` is outside this WP). `Progress` stores and restores the words (`new_seen`, `set_new_seen`), so the wiring is two lines once `NewTags` offers `seen_words()` and `set_seen_words()`.
- The footage's "Y Delete file" is the remaster's; no delete exists in the PS2 handlers read. Not ported.
- The numbers marked [inferred] in `menu_save.rs` `pos` (text offsets, the hand's place, the pills, the logo scale) are fitted to the footage; the groups are the disc's.

### 18.5 Menu kind 1: the save points' screen (WP-E7)

**What the original does [V unless marked].** A save or world point's accept (`FUN_001784b8`, docs/events.md section 4) calls `FUN_0024f0b8(1, mode)` (mode 0 world point, 2 save point). Kind 1's record (0x380570: scene 3, init `FUN_00255598`, term `FUN_00255608`, mode word 0x36cd28, which `FUN_0024f498` returns; test `savepoint::tests::menu_kind_one_is_the_points_save_list`):

- **Init** `FUN_00255598`: the memory card driver setup of kind 2 (`FUN_002bbed8(0, 0)`, `FUN_002bbef0(0x13)`, `FUN_002bbf00(0x380d38)`, `FUN_00254bf0(0x380cd0)`, `FUN_002bb790`), `DAT_0036cf30 = 1`, then the task 0x2556f0 (priority 2000).
- **Task 0x2556f0** (no `.c`; read from the instructions): `FUN_0016ff50` / `FUN_0016fb20`, `FUN_0024f738(1)` (ends the other kinds' tasks), `FUN_00255258(1)` / `FUN_002552c0(1, task)` (wait for the file loads), `FUN_002bd660`, `FUN_00149fa8`, one frame's yield (`FUN_00102438(task, 1)`), the build `FUN_002557a8`, then each frame: sound kind 0xb (`FUN_0023e738(0xb, 1)`, the open sound id 3), `FUN_00233cd8`, `FUN_00233178`, the page machine `FUN_00255a10` until it returns 1, `FUN_00258138` (the per-page draw) and a yield.
- **Build** `FUN_002557a8`: the layer object with header group 0x7532, the Munny and Time plates (`FUN_0029cf78`, `FUN_0025ec28` with the group triples at 0x380c98/0x380cd8 and 0x380c88/0x380ce0), the plate 0x7533, two text bars (0x753b), page 0 (`FUN_002959d0(0, 0)`), and the top list `FUN_00255c10`.
- **Top list** `FUN_00255c10`: rows 0 and 1 of the list widget `DAT_0036d50c` from 0x24-byte records at 0x380cf0 (group triples idle 78/80/79 and selected 78/80/77 of `save.2ld` sequence 0, the same groups as kind 8's rows; the message at +0x12, i.e. 0x380d02 + 0x24 * row): row 0 = 0x8a7e "Save", row 1 = 0x8a80 "World Map". **Row 1 is skipped when the mode is 2**: the save point lists only "Save".
- **Page machine** `FUN_00255a10` (`DAT_0036d1d4` = page): 0 `FUN_00255d38` (the top list: row 0 -> page 1 via `FUN_00295a00(1)` and the open sound `FUN_0025ef28`; row 1 -> page 0x10 via `FUN_00257e70`; -2 / -4 close with `FUN_0025ef28`, `FUN_00295ba0`), 1 `FUN_00255f80` (a two-row list, row = `FUN_0023d2f8(row)` as kind 2's page 1; -2 back to page 0), 2 `FUN_002561e0` (the card check, jump table 0x380b74), 3 `FUN_00256db8` (card states 10..0x13), 4/6/9/0x13 `FUN_002563d8` (information boxes 0x8a89, 0xb85d), 7/0xc `FUN_00256788`, 8 `FUN_00256b60`, 0xa `FUN_00256cd8`, **0xb `FUN_002572c0` (the file list: 0x8aa4 overwrite question -> 0xc, write 0x8a90 -> 0xd)**, **0xd `FUN_00257880` (the write: states 0x1e..0x2b; at 0x2b sound kind 0xc, id 0x24 (table 0x37f654 entry 0xc), and page 0xb again: the list shows once more; a card error shows 0x8a8b on page 6)**, 0x10 `FUN_00257ee8` (the World Map question, `FUN_00255010(.., 0x7549, .., 0x380cb0)`: messages 0x8a7b "OK", 0x8a7c "Yes", 0x8a7d "No"; Yes -> `FUN_002580b8`: `FUN_0014a7c8(field object, 0xf, 0)` and page 0x14), 0x11 `FUN_002576d8`, 0x12 `FUN_00257178`, 0x14 `FUN_002580e8` (waits for the field object's word +0xfc to be 4, then `FUN_00103b18(1)` and the close). Once closing (`FUN_00295bb0`), the machine waits for the card and the objects, then `FUN_002bce90`, `FUN_00258838` and **`FUN_001038a0`: back to the field scene** (1.1); the term `FUN_00255608` frees the card driver.

So kind 1 is kind 2's file screens with a top list of its own, and it differs from the field menu (kinds 7/8/2) in two visible ways: the rows (Save, and World Map at a world point) and the list showing again after a write instead of the close.

**What the port does.** `savepoint::point_use` calls `SaveMenu::request_point(mode)`; `menu_save` opens on the next frame with `rows_for_mode(point_rows(exe), mode)` (`point_rows` reads 0x380d02 + 0x24 * row from `SLUS_210.05` at load), the same pages as 18.3 otherwise, and `Flow::relist`: after the write the file list shows again (`menu: save point page 0xb again after the write (FUN_00257880)`). The field is frozen while it is up (`schedule::Pause::menu`, as the original's scene change stops the field scene); the close returns to the field without an area load (`menu: save point menu closed (menu kind 1 mode M): back to the field (FUN_001038a0)`). Two rows move with Up/Down and wrap [inferred: the widget `FUN_002967b8(list, 4, 1)`], the idle row draws group 79. Test `menu::save::tests::the_point_menu_lists_its_second_row_only_outside_mode_2`. Run: `KH2_REACTION_PRESS=8 KH2_SAVEPOINT_WATCH=5 KH2_REACTIONLOG=1 KH2_NO_ENEMIES=1 KH2_KEYS=11:Down,12:J,13:K kh2-game tt02` (tt02's world point wins the offer: mode 0, two rows).

**Gaps.** The World Map row's question and exit (pages 0x10 and 0x14; what `FUN_0014a7c8(.., 0xf, 0)` and `FUN_00103b18(1)` load was not read): the row logs `not ported` and does nothing. Page 1 (the two-row list `FUN_00255f80`) is skipped as in 18.3 (the port has no card to pick). The top page's help line: kind 1 sets none of kind 8's (`0x3858`), and what `FUN_0029d1d0(0, DAT_00380d02, 0)` (`FUN_00255f50`) draws was not read: the port draws no help on that page. The byte 0x33e86e (the point's spawn word) is not stored in `Progress` (a save file does not yet record which point it was written at).

## 19. Party page and the members' Status tabs (WP-MN7)

Code: `menu_party.rs` (child module `menu::party_page`, `PartyPlugin` in `MenuPlugin`'s tuple), `menu_status.rs` (`member_numbers`, `StatRules::table_of`, the roster), `menu_preview.rs` (the 3D models stand on the Party page's places: `Stage`, `model_drop`, rebuilt when the page or the order of the members changes), `menu.rs` (the Party row opens page 0x14). No clip shows either screen (clips 5, 6, 7 and 10 stop at page 0), so every look below that the code does not give is `[I]`; the groups, numbers and rules are read from the code and the disc.

### 19.1 Status: Donald's and Goofy's tabs [V code, I look]

- **Which tabs.** `FUN_002683c8` (the tab mask, 10.1) sets bits 6, 7, 8 for the party slots 1..3 (`for k in 1..count`), so each member standing in the party has a tab after the leader, the forms and the summons; `menu_status::tab_mask` already had the rule, the port now fills the list with the members standing in the field (`party_page::roster`: the `PartyMember` entities by slot; the app spawns the two companions, not the partner of slot 3). The tab's name is the member's name message (`0x37ba90[member id]`, as the leader's), its help line "{}'s Status." with that name (`tab_help[6..=8]`, 0x3817d0).
- **Which rows.** The jump table 0x381768 sends tabs 0, 6, 7 and 8 to the same case 0x2685a0; it sets bits 0..5 and 10..16 and returns for a tab that is not the leader's (`bne a0, zero, 0x2686d0` at 0x2685f8): a member shows LV, Experience, Next LV, HP, MP, AP in the first column and Strength, Magic, Defense and the four resistances in the second; no Drive Gauge, Bonus LV, form or summon rows (`menu_status::member_bits`).
- **Which numbers.** The value jump table 0x3816d0 also sends tabs 0, 6, 7, 8 to one case, 0x267444: `addiu t7, v0, -5; movn s0, t7, v0` makes the party slot `tab - 5` (0 for the leader), `FUN_002829c0(party, slot)` the status object (entry `+0x14` of `FUN_00282738`, `FUN_0019fe48(member, 0)`), and every value is read from it exactly as the leader's (10.2): LV the record's byte `+0xf` (`status +0x238`), **Experience the one party-wide word `0xf88` of the party block** (the case loads it for every tab, `lw a2, 0xf88(s7)` at 0x26749c: a member's tab prints Sora's total), Next LV `FUN_001878b0(record, status +0x244)`: the Exp column of the member's own `lvup` table at its level (`FUN_001b50c0`, the table by `FUN_001b58d8`) minus the shared total `DAT_00340e48`, HP `status +0 / +4`, MP `+0x180 / +0x184`, AP `FUN_002688d8(slot)` over `+0x18e`, Strength, Magic, Defense `+0x188 / +0x18a / +0x18c`, resistances `100 - (+0x1a9 .. +0x1ac)`.
- **Port.** `member_numbers(progress, member, rules, live)`: the same sums as the leader's `numbers` over the member's record (`Progress::member(id)`: weapon words, armor, accessories, ability list, the bonus bytes `+8 .. +0xb`), the member's `lvup` table (`StatRules::table_of`: id 1..=13 is table `id - 1`, 14 and 15 as `FUN_001b58d8`, missing or empty tables fall back to Sora's, the rule `party::PartyData::member_status` already uses), and the equipped items' `item_stat` rows (no form weapon). HP and MP are the field members' live values (`PartyMember::hp .. mp_max`, as the page-0 cards). [I] the status block's equipment is the two weapon words and the armor and accessory slots, as `FUN_0019f920` builds the leader's (10.2: it was read for the leader; the member's call is the same function on its own record, not re-read).
- **Seen** (`tt03`, Donald and Goofy in the field, new game with the baseline; screenshots `local/mn7/status2.png` and `status3.png`, not committed): the tab list shows Sora, the four forms, Donald, Goofy; Donald's tab LV 1, Experience 0, Next LV 26, HP 18/18, MP 100/100, AP 5/5, Strength 2, Magic 6, Defense 2, all resistances 0%, help "Donald's Status."; Goofy's page 0x18 (reached with the right shoulder `RB`): LV 1, Next LV 13, HP 25/25, MP 100/100, AP 4/4, Strength 6, Magic 0, Defense 2.
- Tests: `menu_status::tests::a_members_tab_shows_the_leaders_rows_without_the_extras`, `a_members_numbers_use_its_own_table_record_and_equipment`, `the_tab_list_has_one_tab_per_companion`.

### 19.2 The Party page, pages 0x14 and 0x16 [V code and groups, I text and hand]

- **Page 0x14** (`FUN_0026dbd8`, built by `FUN_0026d378`): a **horizontal** list: both list widgets get the previous / next masks 0x40 and 0x80 (Left and Right: `FUN_00296d10(list, 0x40, 0x80, 0, 0)`) and the rows sit at `FUN_00296bd0(list, i, -1, 0)`: x = the row group's `unk20` (118) times the row, y 0, children of the page layer. One row per entry of the menu party object (`FUN_00282738`: each of the four slots of the arrangement that names a member with a model; `FUN_002829c0`), each with: a **column** plate (group `0x13b`, `0x13c` for the fourth row: `(i == 3) + 0x13b`), a **name plate** object (group `0x13e` for a free row, `0x13f` for row 0 and any locked row: the words at 0x381adc and 0x381ae0) whose text field `+0x18c` holds the member's name message (`FUN_001b5868`), and a **3D model** (`FUN_0026d8a8`).
- **Geometry** (the disc test `the_party_groups_exist_in_camp` prints and asserts it): the page layer is group `0x138` (enter, 16 frames, alpha 64 to 127), `0x139` (hold, loops) and `0x13a` (leave, `FUN_0026e2f8`), a 492 x 254 dark panel with a light rim at (12, 72)-(504, 326), anchor (24, 83). A column is 116 x 210 at (-5, -4)-(111, 206) from the layer anchor plus 118 per row; its colour is dark red (0x13b), the fourth row's dark blue (0x13c). The marker `0x13d` (`FUN_0026ed88`, drawn under the hand by `FUN_00251d38`) is the same box one unit larger with a bright red outline whose colour pulses (red 240, 120, 240 over its 60-frame loop; the disc test asserts the change). The name plate is a 124 x 49 strip at (-8, 189)-(116, 238), grey for a free row and green for row 0 and locked rows, anchor (51, 189). The hand is the camp's (`FUN_00253f98` answers 0x19 and 0x17 for a page not in the table at 0x3807d8) attached to the row (`FUN_0029ac68`), the second hand (0x17, waiting) on the chosen row while page 0x16 runs (`FUN_00251d38`: the active list's hand index is 1 there).
- **The models.** `FUN_0026d8a8` attaches each model to its name plate (`FUN_0023b4b8`; `+0x14e1` set), the draw `FUN_0023aca8` then places the slot at the plate's screen position minus (0x100, 0xd0) plus the slot offset `FUN_0023b228(.., plate x, plate y - 0x8a)`, with the projection centre moved by `FUN_0023a848(.., 0, 0x8a)`: the net is the plate group's anchor on the layer, so the **model's origin stands 9.8 menu units under the plate's anchor** (the same slot camera as page 0, 16.8: `menu_preview::model_drop`): x 75 + 118 per row, y 281.8. [I] the sum of the two terms (the layer's base, the group anchor) as "plate anchor on the layer"; it puts the models on their columns just above the name plates, the only reading that fits the rest of the code. `FUN_0023b5d0` (member kind 8 only) and `FUN_0023b5b0(.., 0xf)` (kind 3) shift or retune single models; not ported [GAP: their effect was not read]. A swap exchanges the two slots' models (`FUN_0023b6d0`).
- **Disabled rows.** Row 0 and every row for which `FUN_001a7268(party, slot)` is true (the slot byte has bit 0x80, or the member is in the hidden list at 0x1d5e380, `FUN_001c7be8`) are disabled (`FUN_00298f78` mask); the cursor skips them (`FUN_00296fb8` calls `FUN_002978f8` for a disabled row; the two movers `FUN_002978f8` and `FUN_00297af0` step to the next enabled row, wrapping when the list wraps, and stay when no other row is enabled). The cursor starts on the first enabled row from the second one on (`FUN_0026d378` loop). **The list can move and confirm only when** `rows >= 3` and `rows - locked_companions >= 3` (the tail of `FUN_0026d378`: otherwise `mask & ~0x13`: no confirm, no move; cancel stays).
- **Help line** (`FUN_0026e2a0`): page 0x14: the word at 0x381ae4 (message 0x46b, "Select Party Member.") when the list can switch, else the word at 0x381ae8 (message 0x3b88: the "no party members to switch" line); page 0x16: the literal 0x846c (0x46c, "Switch Party Member."). The texts are read from `sys.bar` at run time.
- **Page 0x16** (`FUN_0026df80`, entered by `FUN_0026dec8` from a confirm on page 0x14): the second list is the same rows; its disabled mask is the first list's plus the chosen row, its cursor the next row or, when there is none or it is disabled, the previous one. A confirm calls `FUN_0026eeb0`: `FUN_001a75e8(party, a, b)` swaps the two **slot bytes** of the arrangement, `FUN_00282908` rebuilds the menu party object, the plates' name pointers (`+0x948`) and the 3D models (`FUN_0023b6d0`) trade places; a cancel does not swap. Both end the page (`FUN_00295a90(1)`) and `FUN_0026df80`'s phase-2 branch returns to page 0x14 (`FUN_002959d0(0x14)`) with the first list's cursor where it was. A cancel on page 0x14 goes back to the camp's list (`FUN_00295a00(0)`) and plays the leave animations (`FUN_0026e2f8` `0x13a`). `FUN_0026ef90` (end of the page and of the camp) tells the field that the arrangement changed (`FUN_00252f48`) when the saved copy (`DAT_0036d000`, made by `FUN_0026d1b0`) differs.

### 19.3 What the port does

- **Opening.** The camp's Party row (flag 0x71, visible in the baseline) opens page 0x14: page 0's nodes are hidden (and come back on the return, with the cards rebuilt in the new order), the frame plays 0x138 then 0x139, a column and a name plate stand per row, the hand and the marker sit on the cursor row, the help line is shown; the 3D models stand on the places above (the preview camera is rebuilt for the page and again after a swap). The rows are the leader and the members standing in the field (`party_page::rows`: `PartyMember` by slot, locked when the slot byte of `PartySlots` has 0x80). Left and Right move over the enabled rows (new press wraps, a held repeat does not), `J` confirms, `Esc`/`Backspace`/`K` cancel, Start closes the camp, as on every page.
- **The switch.** Page 0x14 confirm: page 0x16 (cursor, mask and help as above, the waiting hand on the chosen row). Page 0x16 confirm: `PartySlots::bytes` swap the two slots and the two `PartyMember::slot` values trade, so the members follow the other's place in the field and the cards, the Status tabs and the HUD rings take the new order [I: the field object's `slot` stands for the follow position; the original re-reads the arrangement when the camp ends, `FUN_0026ef90`]. Log lines: `menu: party page 0x14: 3 rows, cursor 1, disabled 0b0001, switchable true`, `menu: party cursor N (page 0x..)`, `menu: party row N chosen, page 0x16 cursor M`, `menu: party switched 2 (slot 1) with 3 (slot 2), slot bytes [00, 02, 01, 10]`, `menu: page 0x14`, `menu: page 0`.
- **[I] what is not from the code:** the names' place and size (centred on the plate's anchor, 4 units under the plate's top edge, font 1 size 15, white for the cursor row and grey for the others, like the camp's rows), the hand's offset from the row's anchor (the camp's (-13, 0)), the instant hide of the page's objects on the way back (the original plays the leave groups), the frame's enter without per-object animations, and the sound kinds (the camp's cursor, confirm and cancel; the menu's sounds are muted anyway, `menu_sound::MENU_SOUND_ENABLED`).
- **Seen** (`tt03`, `KH2_NO_ENEMIES=1`; screenshots in `local/mn7/`, not committed): `party1.png` page 0x14 (three dark-red columns with Sora, Donald and Goofy standing, name plates under them, Sora's green, the marker around Donald, the hand at his column's top, "Select Party Member."), `party2.png` page 0x16 (two hands, the marker on Goofy, "Switch Party Member."), `party3.png` after the switch (Goofy in the middle, Donald on the right, the log `slot bytes [00, 02, 01, 10]`), `party4.png` with `KH2_NO_PARTY=1` (one column, "There are no Party Members to switch.", Left/Right do nothing), `party7.png` page 0 after the switch (the cards read Sora, Goofy, Donald, the models stand in the same order).
- Tests (`party_page::tests`): the rows and the leader's lock, `can_switch`, the first cursor, the cursor over disabled rows and the wrap rule, the partner list, the byte swap and the slot trade, and the disc tests `the_party_groups_exist_in_camp` (the frame, column, plate and marker numbers above) and `the_executables_tables_name_the_plates_and_the_help_lines` (0x381adc .. 0x381ae8, the messages exist).

### 19.4 Gaps

- **Page 0x15** (ported in WP-PT1, `party_switch.rs`, docs/party.md section 21; the frame is [inferred]) (`FUN_0026e158`, the notice window "Party Members have been switched." / "Party In: .. Party Out: .." that `FUN_0026e130` and `FUN_0026e618` build for the camp opened through menu kind 5, `FUN_0024f300(a, b)` after `FUN_0026ef70(b)` with the previous party): ported; a jump record with word 6 = 1 or 2 opens it (`party::jump_party`). Structure (`FUN_0026e618`, `FUN_0026e970`): it builds two comma-separated text blocks (`+0x16c4`, `+0x1744` of the page object, `FUN_001b5868` names, an icon code 10 before a locked member) of the members that came in and those that went out, compared with the stored previous party, under the headers 0x846e / 0x846f (0x46e "Party In", 0x46f "Party Out"), over the window text 0x846d (0x46d "Party Members have been switched."); a last line is message 0xcc69 when `DAT_0036cd30 == 2` and otherwise a per-member message of the table 0x381b08. The page's handler leaves for page 0x14 on a confirm or cancel (`FUN_0026e8d0`, `FUN_00295a00(0x14)`).
- **The effect on the field.** The original's swap acts on the world's slot bytes; the app's members are spawned at the area's load from `PartySlots::bytes` and follow by `PartyMember::slot`, so the port swaps both at once (the members trade follow places when the camp closes). Since WP-MN8 (docs/party.md section 18) `PartySlots` keeps the bytes of each world across area loads, a changed member set rebuilds the members, and a filled byte 3 spawns the partner (the fourth column then shows it). Not run in the game: nothing in the app fills byte 3 or changes the set yet.
- **Hidden-member list** (`FUN_001c7be8`, the words at 0x1d5e380 with the count at 0x35ff30): its writers were not searched, so no row is locked by it. Searched: `FUN_001a7268`, `FUN_001a7698`, `FUN_001a7608`.
- **Leave animations and the page's object enter animations** (the list rows' `FUN_00296b48` groups with a follow-up; `FUN_0026d378`'s `FUN_00296fb8`): the rows' group sets are four copies of 0x13b/0x13c, so the highlight is the marker and the hand; the page's own leave (`0x13a` and the list leave) is not played.
- **`FUN_0023b5d0(preview, slot, 0x14, 0x20)`** for member kind 8 and **`FUN_0023b5b0(.., 0xf)`** for kind 3 in `FUN_0026d8a8`: per-model tweaks (a motion-frame range step and a flag byte at `+0xb60`) not read; no member of those kinds stands in the field of the app.
- The name text's own placement, size and colour (`FUN_00139048` on the plate object's `+0x18c` text): not read, fitted by eye (19.3).

### 19.5 Decompile index (WP-MN7)

| address | what |
|---|---|
| 0x26d1b0, 0x26d220, 0x26d348, 0x26d360 | Party page object init (copy of the party object `DAT_0036d000`), term, per-frame wrappers |
| 0x26d378 | page build: lists, column and plate objects, disabled mask, cursor, switchable rule |
| 0x26d7b0, 0x26dec8 | second list (page 0x14 / 0x15), the partner list of page 0x16 |
| 0x26d8a8, 0x23a848, 0x23b228, 0x23b348, 0x23b4b8, 0x23b6d0, 0x23aca8 | the 3D models: slot setup, projection centre, draw offset, translation, attach to plate, swap, draw |
| 0x26dbd8, 0x26df80, 0x26e158 | pages 0x14, 0x16, 0x15 |
| 0x26e130, 0x26e2a0, 0x26e2f8, 0x26e398, 0x26e508, 0x26e618, 0x26e8d0, 0x26e8e8, 0x26e970 | page 0x15 init, help line, leave, free, busy test, notice window, its leave test and texts |
| 0x26ecc8, 0x26ed28, 0x26ed88, 0x26eeb0, 0x26ef70, 0x26ef80, 0x26ef90, 0x26f028 | marker test and creation, the swap, the stored previous party, the changed-arrangement test, slot to row |
| 0x282738, 0x282908, 0x282940, 0x282968, 0x282980, 0x2829c0, 0x2829d8, 0x2829f0, 0x282a20, 0x282a78, 0x282d78 | menu party object: rebuild, copy, count, entry fields |
| 0x1a7200, 0x1a7268, 0x1a71e8, 0x1a75e8, 0x1a7608, 0x1a7698, 0x1c7be8 | slot to member, locked slot, slot byte, swap two slots, member to slot, in-party test, hidden list |
| 0x296bd0, 0x296d10, 0x296fb8, 0x298f78, 0x298f80, 0x298f88, 0x2977a8, 0x2978f8, 0x297af0 | list widget: row placement, move masks, cursor set, disabled mask and tests, movers |
| 0x251d38, 0x253f98, 0x29a9c0, 0x29ac68, 0x29ad48 | camp per-frame draw, hand groups, hand create and attach |
| 0x3816d0, 0x381768, 0x267444, 0x2685a0 | Status per-tab value and row jump tables, the leader's and the members' case |
| 0x381ab0 .. 0x381b10 | Party page words: plate groups, help messages, notice texts |
## 20. Jiminy's Journal: menu kind 3 (WP-MN9)

### 20.1 Entry, scene and exit [V unless marked]

- **From the camp.** Page 0's handler (`FUN_00251848`, jump table 0x380794, entries 5 and 6 both at 0x251c38): `FUN_00253ec0`
  gives the row's entry; the mode word 0x36cd34 (`FUN_0024f4d0`) becomes 1 for entry 5 (Jiminy's Journal) and 2 for entry 6
  (Roxas's); `FUN_00296458(2, 0)` queues the menu scenes 2 (kind 3) and then 0 (the camp); `FUN_00296470(0x250ec8)` starts the
  wipe with the camp's teardown as its callback; `FUN_00295ba0` closes the camp. The scene runner `FUN_00296278` runs the
  queued scene's init, its task until it ends, its term, then shifts the queue (`DAT_0036d206` <- `0x208` <- `0x20a`): **the
  journal returns to the camp**. (The other opener, `FUN_0024f3c0` from 0x2c9510, calls `FUN_0024f0b8(3, mode)` with mode 0.)
- **Init `FUN_002756f8`.** Loads menu file 3 (`jmemo.2ld`, `FUN_0024f738(3)`, `FUN_00255258(3)`), creates two list widgets
  (`FUN_002969b0(0x38)`, `FUN_002967b8(.., 0x12, 1 / 0)`), the data holder `DAT_0036d0dc` (bytes: +0 journal world, +1 page
  family, +2 Roxas flag), the file list `FUN_002a1a10` with `jiminy.bar` (`FUN_002a1bd0`: `FUN_002a19c8(0, 0)`, names at
  0x36d3e0: `jiminy.bar`, `jm_world.2ld`, `jm_top_` + world + `.2ld`, `jm_photo/`, `jmface_`; suffixes `.2ld` 0x36d3d8 and
  `.bin` 0x36d3dc, built by `FUN_002a18e8`) and the fifteen page modules' inits (`FUN_002afa48` .. `FUN_002b5c60`). Then the
  **start page**: `m` = the mode word (negative -> 0); the Roxas flag `+2` = (m == 2); when m != 2, `+0` = the current world's
  journal index (`FUN_00277c88(DAT_0036d038)`; `DAT_0036d038` = -1 from the camp, so the world byte 0x33e830) and a negative
  index makes m = 0. Page: m 1 -> **2**; m 0 -> **0** (`+0` = -1); m 2 -> **1** (`+0` = 0x13); m 3 -> **10**; others 0
  (`menu_journal::start`).
- **World index `FUN_00277c88`.** A jump table at 0x381f38, one stub per game world (19), each `jr ra` with the index in the
  delay slot: -1 for worlds 0, 3 and 15, 0 for Twilight Town (2), and so on (read at run time: `read_world_index`).
- **Task 0x276338** (no `.c`; read from the instructions): `FUN_002552c0(3, task)` (waits for the files), `FUN_00277be8`,
  `FUN_002a1858(0, 0)` (the `jiminy.bar` request), `FUN_00276550` / `FUN_002765a0` (the message file 0x65 = `msg/us/jm.bar`, WP-MN10: 20.7), `FUN_002b4890`,
  yields, `FUN_002a1bd0` / `FUN_002a1c30(0)` unless `DAT_0036d040 & 4`, `FUN_002b48c8`, `FUN_00276488(page)` (the page's own
  `jiminy.bar` sub-files), waits while `DAT_0036d106`, `FUN_002b7100`, `FUN_002b9b28`, **the build `FUN_00276740`**, the list
  widget `FUN_002a04f8`, the open sound (`FUN_0023e738(0xb, 1)`), then each frame `FUN_00233cd8` and **`FUN_00276918`** until
  it returns 1, with `FUN_00276bb0` (the draw) between; the end `FUN_002765e8`.
- **Build `FUN_00276740`.** The layout object (`FUN_0023b920(0xa0)`, `FUN_0029b530(.., 0x7557, 0, 0, t[0], t[2])` with
  `t` = 0x381f98 (layouts 0, 2, 1: enter, leave, hold) or 0x381fa0 (3, 5, 4) when the Roxas flag is set, `FUN_00278788`), the
  plates `FUN_0029cf78(0, 5, layout, 0x381fc0, 0x381fc8)`, the header object 0x7559 (`FUN_002779f8(0, 0)`: groups from
  0x381fa8, the first then the third), the plate object 0x7558 (`FUN_002772d0`: groups 0x2c then 0x2d), then the start page's
  builder: 0 `FUN_002a2248`, 1 `FUN_002a4188`, 2 `FUN_002a3540`, 10 `FUN_002b4b18` (after `FUN_0029d820(0, 0x3251)`).
- **Page handlers `FUN_00276918`** (page = `DAT_0036d1d4`): 0 `FUN_002a27d8`, 1 `FUN_002a44a8`, 2 `FUN_002a3da0`, 3
  `FUN_002a5218`, 4 `FUN_002a6010`, 5 `FUN_002a6578`, 6/0xb/0x20 `FUN_002ae790`, 7/8/0x19/0x1a `FUN_002afe98`, 9/0x1b
  `FUN_002b6120`, 10/0x1c `FUN_002b4dd8`, 0xc `FUN_002a7978`, 0xd `FUN_002a8e90`, 0xe/0x11/0x14 `FUN_002a9590`, 0xf/0x12/0x15
  `FUN_002a9b80`, 0x10/0x13/0x16 `FUN_002aa0b0`, 0x17 `FUN_002accb8`, 0x18 `FUN_002ad3d0`, 0x1d `FUN_002b25a8`, 0x1e
  `FUN_002b2d78`, 0x1f `FUN_002b3f10`. Once closing (`FUN_00295bb0`), it waits for the objects, tears down (`FUN_002761e8`) and
  returns 1 (`FUN_001038a0`): the scene ends.

### 20.2 The data: `jiminy.bar` [V layout of entry 0; I the others' use]

`menu/us/jiminy.bar` is a BAR of nine type-43 entries, each with a 4-byte magic, 0x10 at +4 and a record count at +8: "worl"
(JMWO, 17), "stor" (JMST), "albu" (JMAL), "char" (JMCH), "anse" (JMAN), "diag" (JMDI), "limi" (JMLI), "mini" (JMMG), "ques"
(JMQU). Entry 0 (the worlds) has 0x10-byte records from +0x10: +0 the journal world id (i8), +1 the two-letter world code, +4
and +6 two message ids, **+8 the story flag that opens the world** (`FUN_002a1db8` -> `FUN_001802e0`), +0xe a second flag
(`FUN_002a1de8`, read by `FUN_00276488` for id 1). Entry 3 is read by `FUN_002a1c30` (0x24-byte records, +4 a bit number) and
entry 5 (0xc-byte records with a sub-list at +8): the bits of the 12-word table `DAT_0036d450` [the meaning: GAP]. The
other entries belong to the pages that are not read. `jm_world.2ld` is a BAR of 17 IMGD pictures ("wmic"), one per journal
world id, 0x10 the locked world.

### 20.3 Page 0: the world grid [V]

- **Build `FUN_002a2248`.** 0x12 list objects (`FUN_00296b48`, every group of the record set = `DAT_0036d050` = 1) in
  sequence 5 (`DAT_0036d04c`), each with two children (types 0x755f and 0x7560, group 1). Walking `jiminy.bar` entry 0, a
  record with an id below 0x10 gets a cell when its flag is set or when flag 0x4837 is (`FUN_002783f8`); the cell plays 0x97
  and, for an open world, `FUN_002b9658(id)` / `FUN_002b9798(id)` put the first child on group 0x98 or 0x99 (the words at
  0x382aec / 0x382af0) and `FUN_002baf08(id)` the second on 0x9a. The world picture (`FUN_002a2bc0`, once `jm_world.2ld` has
  arrived): an object of sequence 2 drawing picture `id` (0x10 when the flag is clear) of the `jm_world.2ld` list
  (`FUN_0023c478`, `FUN_002332e0`), group 1 then 2 (2 alone when built before the pictures arrived), child of the cell.
- **Place `FUN_002a25f0`.** Rows of six; each full row centred (`x = -(pitch_x * (cols - 1)) / 2 + col * pitch_x`), the
  last partial row centred on its own count, the rows centred (`y = row * pitch_y - (pitch_y * (rows - 1)) / 2`), C integer
  division. `pitch_x` is the cell group's +0x20 (`FUN_00237698`), `pitch_y` its +0x1c (`FUN_00237678`): 80 and 98 for
  group 0x97. The offsets are the objects' own positions (`FUN_00138ed0`); the cell group itself is drawn around the screen
  centre (port log: the 0x97 box sits at x 215..295 with offset 0).
- **Cursor.** `FUN_002a2f38(+0 or 0xf when negative)`: the place of that world among the shown records with an id below 0x14;
  the widget gets 6 columns, 3 rows (`FUN_00298e40(list, 6, 3)`), the cursor cell plays 0x96 and the others 0x97
  (`FUN_002a31e8`).
- **Handler `FUN_002a27d8`.** List result >= 0: `FUN_002a2e68` gives the world id, `+0` = it, page 2 (0xc when the id is
  above 0xe); -8 / -9 page switches through `FUN_00277928`; **-2 and -4 close the scene** (`FUN_00295ba0`).
- **Page 2 `FUN_002a3540` / `FUN_002a3da0`** (read for the exits only): an 8-row list of the world's categories (loads
  `jm_top_<world>.2ld` through `FUN_002a45b0`); a row leads to page 6, 3, 4, 7, 8, 9, 10 or 0xb (the message at 0x382b88 +
  2 * row goes to the help line); **-2 goes back to page 0**, -4 closes.

### 20.4 What the port does (`crates/kh2-app/src/menu_journal.rs`)

A confirm on camp entry 5 or 6 sets the mode (1 / 2), takes `menu.page` (0x300, not a camp page) and opens the scene on the
next frame: it loads `jmemo.2ld`, `jm_world.2ld` and `jiminy.bar` once, hides every node the camp had (restored on the way
back), plays layout `t[0]` then `t[2]` (the frame, carried out to the window's edges as the camp's frame is [inferred]), the
header object (0x381fa8 groups) and the plate 0x7558 (0x2c, 0x2d), and the cells of 20.3 with the app's story flags
(`Progress::story`). (WP-MN10: start pages 1 and 2 are now built, 20.7; page 0 follows a cancel on page 2, with the cursor on the
world.) Left/Right/Up/Down move the cursor [inferred: one cell, one row of six, no
wrap], a confirm opens page 2 (ids above 0xe: refused with a log line), K / Esc / Enter close the scene and the camp's
page 0 comes back. Tests: `menu_journal::tests::{the_start_page_follows_the_mode_and_the_world,
camp_rows_five_and_six_write_modes_one_and_two, cells_need_the_story_flag_or_the_all_worlds_flag,
the_grid_centres_rows_of_six, page_zero_state_machine, the_disc_journal_files_and_tables}`. Run (rows of a `KH2_STATE` with the
flags of nine worlds; the camp has six rows there, Journal is the fifth):
`KH2_STATE=<{"story_flags": [2097, 4098, 5122, 6195, 7204, 8193, 9217, 10243]}> KH2_NO_ENEMIES=1 KH2_KEYS=7:Enter,8:Down,8.2:Down,8.4:Down,8.6:Down,9.2:J,10:Right kh2-game tt02`
(`journal: open (FUN_002756f8): mode 1, journal world 0, start page 2 ...; 9 cells ...`); `KH2_JOURNAL_QUADS=1` logs each
node's box once.

### 20.5 Gaps

- **Pages 4 to 0x20** (WP-MN10 ported 1 and 2: 20.7; WP-MN11 page 3, the help line, the cursor and L1/R1: 20.8; WP-MN12 pages 4, 5, 6, 7, 8 and 0xb: 20.9) (the world's categories, characters, enemies, the album, Ansem's reports, limits, minigames, quests,
  the Roxas pages, the photo and face files): not read beyond the exits of page 2.
- **The children's groups 0x98 / 0x99 / 0x9a** (`FUN_002b9658`, `FUN_002b9798`, `FUN_002baf08`, the words at 0x382aec /
  0x382af0): WP-MN12 ported 0x98 / 0x99 (20.9); 0x9a (every category complete) is still open.
- **The plates `FUN_0029cf78(0, 5, .., 0x381fc0, 0x381fc8)`** (groups 0x71/0x73/0x72 and 0x23/0x25/0x24): not drawn; what
  they show on this scene was not read.
- **Text:** the world names (record +4 / +6) and any help line of page 0 (`FUN_0029d820` is called for page 10 only) were
  not traced to a drawer: the port draws no text on the page.
- **Sounds:** none of the scene's own were read (`FUN_00276550` / `FUN_002765a0` are the message file, 20.7); the menu sounds are muted anyway.
- **The wipe** (`FUN_00296470`: `pause.2ld` group 5 as object 0x7554) is not played; the camp's cursor on the way back
  stays where it was [inferred].
- **Footage:** no reference clip shows the journal (`local/VIDEOS.md`); the screen was compared with nothing but the code.

### 20.6 Decompile index (WP-MN9)

`FUN_002756f8`, `FUN_00275bd8`, task 0x276338 (instructions), `FUN_00276038`, `FUN_002761e8`, `FUN_00276488`,
`FUN_00276550`, `FUN_002765a0`, `FUN_002765e8`, `FUN_00276740`, `FUN_00276918`, `FUN_00276bb0`, `FUN_00277c88` (+ stubs
0x277cbc..0x277d3c), `FUN_002779f8`, `FUN_002772d0`, `FUN_00278788`, `FUN_0029cf78`, `FUN_0029b530`, `FUN_0029b600`,
`FUN_00296b48`, `FUN_002a1a10`, `FUN_002a1bd0`, `FUN_002a1858`, `FUN_002a18e8`, `FUN_002a19c8`, `FUN_002a1c10`,
`FUN_002a1c30`, `FUN_002a1db8`, `FUN_002a1de8`, `FUN_002a2108`, `FUN_002a21b8`, `FUN_002a2248`, `FUN_002a25f0`,
`FUN_002a27d8`, `FUN_002a2a90`, `FUN_002a2af0`, `FUN_002a2b48`, `FUN_002a2bc0`, `FUN_002a2d98`, `FUN_002a2e68`,
`FUN_002a2f38`, `FUN_002a3008`, `FUN_002a31e8`, `FUN_002a3540`, `FUN_002a3da0`, `FUN_002783f8`, `FUN_00237678`,
`FUN_00237698`, `FUN_0024f0b8`, `FUN_0024f3c0`, `FUN_0024f4d0`, `FUN_0024f4e0`, `FUN_0024f4f0`, `FUN_00250ec8`,
`FUN_00296278`, `FUN_00296458`, `FUN_00296470`, `FUN_001038a0`, `FUN_00103d58`, camp handler 0x251c38 (instructions).

### 20.7 Pages 2 and 1 (WP-MN10) [V unless marked]

- **Message file.** Task 0x276338's `FUN_002765a0` calls `FUN_00190e70(0x65, 0)`: `FUN_00190dd0(0x65)` builds
  `msg/%s/%s.bar` with "jm" (0x37ad38), and `FUN_00190b70(0, file)` puts it in message slot 0; `FUN_00190c40` looks an id
  up slot by slot (0x1c8a7f4, three slots), so the journal's ids come from `msg/us/jm.bar` before the system file.
  (`FUN_00276550` asks the same file with 0x13ec through `FUN_0016fac8`; what that second argument is was not read.)
- **Objects** (`FUN_002a3540` page 2, `FUN_002a4188` page 1). Six layer objects of sequence 5, groups 0x89, 0x93, 0x91,
  3, 0x92, 3 (page 1: 0x8d, 0x90, 0x94, 3, 0x95, 3); 0, 1, 2 and 4 hang from the header object `DAT_0036d0e8` (the 0x7559
  of `FUN_002779f8`), 3 from 2 and 5 from 4. `FUN_002a3d08` gives 2 and 4 the two story texts of the world
  (`FUN_002a1e98`: `jiminy.bar` entry 1, 0xc-byte records, +0 world, +4 / +6 the ids, used when the flag +0xa is set,
  the last such record wins, 0x8adc otherwise) and 3 and 5 the labels 0x3253 / 0x3254, centred (+0x1a4 = 2).
- **Text placement** (`FUN_00139048`): from the object's position (`FUN_00138f38`), plus the group's x/y (+0x10/+0x14)
  when flag 0x10 is set, in font +0x1a0 (1 for these objects), size the group's +0x18 (15 when 0), colour +0x198
  (0xe5e5e5, `FUN_00138fc8`) when flag 8 is set. `FUN_00138fc8` sets flags 4, 8, 0x10; `FUN_00139888` clears 0x10,
  `FUN_00139860` clears 8. The port: rows and the holders 2/4 use the group offset, 3/5 do not; holders 2/4 lose flag 8
  (colour from the group: the port keeps 0xe5e5e5 [inferred]).
- **Rows** (page 2): 8 list objects (`FUN_00296b48`, both group sets four 3s: the row's group does not change with the
  cursor) as children of layer 0, `k *` the 0x89 group's +0x1c apart (`FUN_00138ed0`), each with two children on group 3
  (the marks: `FUN_002a39b8` puts the first on 0x8a and the second on 0x8b / 0x8c (0x382b80/84) from the categories'
  progress tests: not ported, GAP). The rows are categories 0..7 (`FUN_002a4960`) without 3 for journal world 8
  (`FUN_002b1650`), 5 unless `FUN_002b6f00` (entry 8, 0x10-byte records, u16 world) and 6 unless `FUN_002b5bb0` (entry 7,
  8-byte records) finds a record that the all-worlds flag 0x4837 or a progress test opens. **The port opens 5 and 6
  only with the all-worlds flag** [conservative: the progress tests `FUN_002b6d40` and the minigame bytes 0x341620 have no
  app data]. A row's text is the u16 at 0x382b88 + 2 * category; its help line the one at 0x382b98 (`FUN_002a4a68` ->
  `FUN_00278430` -> `FUN_00278468`, object 0x755d: not drawn, GAP).
- **Title** (page 2): object `DAT_0036d0f0 + 0x350` (`FUN_00277258`: groups 0x35 then 0x36, centred), text
  `FUN_002a3070(world, 0)`: the world record's +4, or +0xa when its flag +0xe is set (`FUN_00277640`). A root object: the
  port draws its text from the group's anchor (`label_anchor`) [inferred: the reading `menu_gfx` uses for the camp rows].
- **Pictures** (`FUN_002a45b0`, `FUN_002a4750`): `jm_top_<name>.2ld` (`FUN_002a19c8(2, name)`), name = the world
  record's code on page 2 (world 1 with its second flag: the string at 0x36d460), the string at 0x36d45c on page 1; picture
  0 in sequence 0x382b6c at layer 0's group x/y, picture 1 in sequence 0x382b68 on layer 1; groups 0x382b78 / 0x382b7c
  and 0x382b70 / 0x382b74 (enter, hold) when the file arrives after the build (the per-frame `FUN_002a4750(1)`), the hold
  alone when it was already there [inferred: the port plays enter then hold].
- **Handlers.** Page 2 (`FUN_002a3da0`): Up/Down move the row cursor (the list widget, wrap on a new press), a confirm
  opens the category page 6, 3, 4, 7, 8, 9, 10 or 0xb (**not ported: refused with a log line**), -2 goes to page 0
  (`FUN_002a2248`, the cursor on the world), -4 closes the scene. Page 1 (`FUN_002a44a8`): -2 and -4 close the scene.
  Page 0's confirm on a world up to id 0xe now builds page 2.
- **First cursor of page 0** (review fix): the index is taken within the shown cells (`first_cursor`), since records
  0x10..0x13 that `FUN_002a2f38` counts have no cell.
- **Port.** `menu_journal::{build12, page2, page1, categories, story_texts, read_page_tables}`. Tests:
  `menu_journal::tests::{page_two_rows_and_state_machine, story_texts_take_the_last_open_record, the_disc_page_two_data,
  the_first_cursor_indexes_the_shown_cells}`. Run as in 20.4; the log: `journal: page 2 (FUN_002a3540 / FUN_002a4188):
  world 0 'tt', jm_top_tt.2ld 2 pictures, story texts [8adc, 8adc], rows [0, 1, 2, 3, 4, 7], text drawn`.
- **Gaps.** The row cursor's highlight (no group change; the widget's cursor object was not found), the help line, the
  L/R page hints (`FUN_00277860` / `FUN_00277740`: ids from 0x381fb8 with a button prefix, `FUN_002774e8` objects 0x2f/0x30
  and 0x32/0x33), the marks, the page switches -8/-9 (`FUN_00277928`), the category pages, the children groups of page 0
  (`FUN_002b9658` / `FUN_002b9798` / `FUN_002baf08` aggregate the same per-category progress tests). Roxas's page 1 is
  ported but was not reached in a run (the test state's camp has no Roxas row). No footage of the journal exists.
- **Decompile index (MN10).** `FUN_002a3540`, `FUN_002a3da0`, `FUN_002a4188`, `FUN_002a44a8`, `FUN_002a4750`,
  `FUN_002a3d08`, `FUN_002a39b8`, `FUN_002a4a68`, `FUN_002a45b0`, `FUN_002a4960`, `FUN_002a46d8`, `FUN_002a34a8`,
  `FUN_002a3070`, `FUN_002a1e98`, `FUN_002a1e78`, `FUN_002a3008`, `FUN_002b1650`, `FUN_002b6f00`, `FUN_002b6d40`,
  `FUN_002b5bb0`, `FUN_001d36e8`, `FUN_00277640`, `FUN_00277740`, `FUN_00277860`, `FUN_00277258`, `FUN_002774e8`,
  `FUN_002772d0`, `FUN_00278430`, `FUN_00278468`, `FUN_002779f8`, `FUN_00277b20`, `FUN_00277eb0`, `FUN_00277d40`,
  `FUN_00277d60`, `FUN_00277dc8`, `FUN_00277de8`, `FUN_002a19c8`, `FUN_002a4b18`, `FUN_002a4b28`, `FUN_0029b600`,
  `FUN_0029b6c0`, `FUN_0029ba00`, `FUN_00296b48`, `FUN_00296aa8`, `FUN_00299f30`, `FUN_00299e38`, `FUN_00138fc8`,
  `FUN_00138948`, `FUN_00139430`, `FUN_001389b0`, `FUN_00138a60`, `FUN_00138ed0`, `FUN_00138f38`, `FUN_00139048`,
  `FUN_00139850`, `FUN_00139860`, `FUN_00139878`, `FUN_00139888`, `FUN_001398a0`, `FUN_00237518`, `FUN_00237618`,
  `FUN_00237638`, `FUN_00237658`, `FUN_00237678`, `FUN_001a7ed8`, `FUN_00190c40`, `FUN_00190dd0`, `FUN_00190e40`,
  `FUN_00190e70`, `FUN_00190b70`, `FUN_00276550`, `FUN_002765a0`, `FUN_002b9658`, `FUN_002b9798`, `FUN_002baf08`,
  `FUN_002a04f8`; data 0x382b68..0x382ba6, 0x381fb8, 0x36d45c, 0x36d460, 0x37ad38.

### 20.8 Help line, cursor, L1/R1, page 3 the story (WP-MN11) [V unless marked]

- **Help line** (`FUN_00278468`, instructions 0x278468..0x278554): object `DAT_0036d100 + 0xe78`, type 0x755d,
  sequence 5, **group 0x7b**, a child of the plate 0x7558 (`DAT_0036d0f0 + 0x4f8`) when that one is alive; the text
  goes to +0x18c; flag 8 cleared (`FUN_00139860`); a second call on a live object plays 0x7b again; `FUN_002785a0`
  plays 0x7c (the leave) at every page switch. Callers: page 2's build through `FUN_002a4a68` (page 1: 0x493f, else
  the u16 at 0x382b98 + 2 * the cursor row's category), page 3's build through `FUN_002a54a8` (0x48d7), the other
  category pages (`FUN_002afa00`: 0x382f04 / 0x382f08). **Nothing calls `FUN_002a4a68` on a cursor move** (`--jal
  0x2a4a68`: the build only; a tail `j` was not scanned): the line keeps the category of the row the build put the
  cursor on. Page 0 sets none (the bar stays empty). Port: `chrome`.
- **List cursor** (`FUN_00276bb0`, the per-frame draw): while the list `DAT_0036d0e0` has a live row object
  (`FUN_00298a90`), +0xe is 0, the scene is idle (`FUN_00295ad0() == 1`) and `DAT_0036d104` is 0, it creates cursor
  slot 0 (`FUN_00277158` -> `FUN_0029a9c0(0, seq 5, .., 0x21, 0x21)`) and hands it the cursor row
  (`FUN_0029ac68`: target +0x180, flag 0x800 `FUN_00139918`), and object 0x7571 (`FUN_002771c0` ->
  `FUN_0029af00(DAT_0036d0fc, 0x7571, .., 0x381fc8)`: the follower groups, enter [0] then [2]) on the same row
  unless `DAT_0036d105` (no writer found with `--addr 0x36d105`: taken as 0 [inferred]). `DAT_0036d104` is set by
  `FUN_00277e70(1)` in the builds of page 1 (`FUN_002a4188`) and page 3 (`FUN_002a4c38`): those pages have none.
  Port: the cursor (group 0x21) and the follower drawn at the cursor row of page 2 or the cursor cell of page 0
  [inferred: an object with a target is drawn at the target's position]; `place_cursor`. This is the row
  highlight MN10 looked for (the rows' own groups never change).
- **L1 / R1** (`FUN_00277860`, `FUN_00277740`, `FUN_002774e8`, `FUN_00277928`): the names at 0x381fb8 belong to
  pages 0, 2 and 0xc (`FUN_00277928` maps a name's place in the table to 0, 2, 0xc). On page 0 the left hint is name 2
  and the right name 1; on pages 1 and 2 left 0, right 2; on 0xc left 1, right 0. When the journal world byte read
  with `lbu` is above 0xe (so -1 too) a hint that would name page 2 names 0 (page 0xc: 2) instead. The texts:
  left = icon `FUN_00255418(0x400)` (pairs at 0x380ab0, 0xb when absent) + the name, right = the name + icon
  `FUN_00255418(0x800)` (control code 9, `FUN_002554c0`). Objects `DAT_0036d0f0` (groups 0x2f then 0x30) and
  `DAT_0036d0f0 + 0x1a8` (0x32 then 0x33), centred (+0x1a4 = 2), made by `FUN_00277258` -> `FUN_002774e8`; only the
  builds of pages 0 and 2 write the texts; the category teardown `FUN_00277ec0` frees the `DAT_0036d100` objects
  only, so the hints and the title stay on page 3 [inferred from the teardown]. The list results -9 and -8 come from
  bits 0x8000 / 0x4000 of the pad word when the list flags 0x2000 / 0x1000 are set (`FUN_00297148`; the builds OR
  0x3000 in); -8 takes the right hint's page (`FUN_002a27d8`, `FUN_002a3da0`). Port: -9 = L1 = Q / PageUp, -8 = R1 =
  E / PageDown [inferred: the side each icon is drawn on]; pages 0 and 2 switch to each other, page 0xc is refused.
- **Page 3, the story** (category 1; build `FUN_002a4c38`, data `FUN_002a5060`, draw callback 0x2a54c8 (no `.c`,
  read from the instructions), marks `FUN_002a5708`, handler `FUN_002a5218`). Entries: every record of `jiminy.bar`
  entry 1 of the journal world whose flag +0xa is set (`FUN_002a1e78`), in file order, text +8; `FUN_00191760`
  counts each text's lines. Objects: the tab of plate 0 (`FUN_0029d1d0(0, -1, 0)`: groups 0x381fc0 [0] then [2],
  the category's row text put there by page 2's confirm `FUN_0029d820(0, ..)`, font 1, its follower 0x381fc8), the
  backdrop 0x755a (`FUN_00278658`: group 0x381f88[family], family 1 from `FUN_00277d60(1)` of page 2), layers 0x5e
  and 0x62 under the header, one list row (groups 2) under 0x5e carrying the draw callback, ten mark objects (group
  2) one line height apart. The callback clips to ten line heights (`FUN_0014b6c0`), starts with the entry that
  holds the first shown line, draws each text in **font 3** at the 0x5e group's size (15 when 0) with an extra gap
  of `pitch - size - font gap` (`FUN_001a8cc0`, never below 0) so a line is the group's +0x1c high, and moves down
  an entry's lines times the line height. The list count is the lines of all entries but the last, plus one, plus
  the last's lines beyond ten (`story_count`); Up/Down step a line; L1/R1 a page of ten (+0x1c = 10,
  `FUN_00299010`; +0x27 set: the list pages itself in `FUN_00298370` and returns no -9/-8) [inferred: clamped];
  -2 goes back to page 2 with the row the confirm kept (`FUN_00277dc8` / `FUN_00277de8`), -4 closes. The colour is
  the row's +0x198 = 0x60000008 [inferred: that word is the callback's colour argument]. Port: `build3`,
  `story_text`; the scroll bar `FUN_00299878(DAT_0036d0f8, ..)` is not drawn as its own object (the 0x62 layer
  draws its arrows); the marks stay on group 2 (their 0x382ba8 / 0x382bac follow the read bits of `DAT_0036d4e8`,
  which the app has no data for).
- **Run.** The MN10 command with a `KH2_STATE` holding Twilight Town's first story flags:
  `KH2_STATE=<{"story_flags": [2097, 4098, 5122, 6195, 7204, 8193, 9217, 10243, 2197, 2200, 2201, 2204]}>
  KH2_NO_ENEMIES=1 KH2_KEYS=7:Enter,8:Down,8.2:Down,8.4:Down,8.6:Down,9.2:J,10.5:Down,11:J,12:E,12.5:Down kh2-game tt02`
  (`journal: page 3 (FUN_002a4c38): world 0, 4 story entries [2afa, 2afb, 2afc, 2afd], 41 lines, list count 32,
  line height 26, gap 6`, then `first line 10`, `first line 11`). Tests: `menu_journal::tests::{hints_name_the_neighbour_pages,
  story_page_entries_lines_and_state_machine, the_disc_category_page_tables}`.
- **Gaps** (WP-MN12 ported Characters, Character Links, Treasures, Maps and the Album: 20.9; what is still open is listed there). Characters (page 6 / 0xb, `FUN_002ae3e8`, handler `FUN_002ae790`): the faces come from entry 5
  (`FUN_002af398`: records of the world opened by `FUN_002a2018`, story flags or an item count) placed at the
  record's x/y in groups from 0x382f30, but their marks and the "met" state are the bits of `DAT_0036d4e8`
  (`FUN_002b7208` / `FUN_002b72c0`, the journal's own save data) and the face file `jmface_` (`FUN_002af7a0`,
  `FUN_002a19f8`) with `FUN_002af6c0` / `FUN_002af800` were not read: not ported. Album (page 4, `FUN_002a5a68`,
  entry 2 and `FUN_002b79d8`), Treasures / Maps (7 / 8, `FUN_002afca0`), quests (9, `FUN_002b5e50`), minigames (10,
  `FUN_002b4b18`), Character Links (0xb, `FUN_002ae3e8` with the second family) and page 0xc (`FUN_002a7220`): not
  read beyond their first calls; whether they draw an empty frame without data was not established, so they stay
  refused. The help line's colour (flag 8 cleared: the group's colour) is kept at 0xe5e5e5 [inferred]. Page 0's
  cursor and follower are drawn on the cell (the same draw code runs there) on top of the 0x96 group. No footage of
  the journal exists (`local/VIDEOS.md`).
- **Decompile index (MN11).** `FUN_00278468` (instructions), `FUN_002785a0`, `FUN_002785d8`, `FUN_00277740`,
  `FUN_00277860`, `FUN_002774e8`, `FUN_00277928`, `FUN_00277258`, `FUN_00277338`, `FUN_002776e0`, `FUN_002779f8`,
  `FUN_00277b20`, `FUN_00277b40`, `FUN_00277e70`, `FUN_00277eb0`, `FUN_00277ec0`, `FUN_00278658`, `FUN_00276bb0`,
  `FUN_00277158`, `FUN_002771c0`, `FUN_0029a9c0`, `FUN_0029ac68`, `FUN_0029ad48`, `FUN_0029aaf8`, `FUN_0029af00`,
  `FUN_0029b3a0`, `FUN_0029cf78`, `FUN_0029d1d0`, `FUN_0029d3e8`, `FUN_0029d820`, `FUN_0029d860`, `FUN_00297148`,
  `FUN_002977a8`, `FUN_002978f8`, `FUN_00298370`, `FUN_00298a80`, `FUN_00298a90`, `FUN_00298ce0`, `FUN_00298cf0`,
  `FUN_00298d58`, `FUN_00298fb0`, `FUN_00298fe0`, `FUN_00299010`, `FUN_002967b8`, `FUN_00296c50`, `FUN_00296f98`,
  `FUN_00296fb8`, `FUN_00255418`, `FUN_002554c0`, `FUN_002554e8`, `FUN_002554f0`, `FUN_00295bf0`, 0x2a4ac0
  (instructions), `FUN_002a46d8`, `FUN_002a34a8`, `FUN_002a4680`, `FUN_002a4c38`, `FUN_002a5060`, 0x2a54c8
  (instructions), `FUN_002a5708`, `FUN_002a54a8`, `FUN_002a5218`, `FUN_002a4ba8`, `FUN_002a39b8`, `FUN_002a5a68`,
  `FUN_002ae3e8`, `FUN_002af398`, `FUN_002af2b0`, `FUN_002afa00`, `FUN_002af7a0`, `FUN_002ae790`, `FUN_002afca0`,
  `FUN_002b5e50`, `FUN_002b4b18`, `FUN_002b7208`, `FUN_002b72c0`, `FUN_002b9360`, `FUN_002b9480`, `FUN_002a2018`,
  `FUN_002af688`, `FUN_002a1e78`, `FUN_00139918`; data 0x381f88, 0x381fb8, 0x381fc0, 0x381fc8, 0x380ab0,
  0x382b98, 0x382ba8, 0x382bac, 0x382f04..0x382f30.

### 20.9 Scroll bar, NEW tags, Characters, Character Links, Treasures, Maps, Album (WP-MN12) [V unless marked]

Code: `menu_journal.rs` and its submodules `menu_journal/{marks,chars,lists,treasure_page,album_page}.rs` (`marks`, `lists` and the
data parts of `chars` / `album_page` are pure functions with tests; the rest is node glue).

- **Text colour (the rule behind several pages).** `FUN_00139048` draws an object's text with the colour `FUN_00237518`
  gives (`+0xdc` of the sprite player, computed by `FUN_00236cf8` together with the group's anchor: the animation flagged 2 that
  covers the frame, its colour blended from start to end). Flag 8 (set by `FUN_00138fc8`, cleared by `FUN_00139860`) replaces the
  RGB with the object's own word `+0x198` (0xe5e5e5), keeping the group's alpha. Objects that clear flag 8 therefore take the
  group's whole colour: group 2 (64, 70, 60, 0x80: the dark ink of the album rows), 0x46 (16, 24, 0: the Treasures labels), 0x53
  (16, 24, 0: the name plate), 0x7e (240, 240, 240: the info text on its black bar), 0x7f (250, 240, 0: the total's count), 0x55 /
  0x56 (16, 8, 0 / 16, 16, 0 with alpha 100 / 96: the picture page's caption and title). The port reads the end colour of the group's
  last flagged animation (`group_color`; a fade-in such as the total label's is not followed). The holders of pages 1 and 2
  (flag 8 kept) and the rows of page 2 (WP-MN10, group 3: 0x80 white, the same look) are left as they were.
- **Holder texts of pages 1 and 2** (fix): `FUN_00139048` starts at the object's position `FUN_00138f38` (the group's animated
  anchor) and adds the group's x / y when flag 0x10 is kept; the port drew them from the node's origin, at the screen's corner.
  They now use `label_anchor` (the Summary and Objective texts sit in their panes).
- **Scroll bar** (`FUN_00299878`, called by the builds of page 3 with the 0x62 layer and of pages 7 / 8 with the 0x43 layer;
  `FUN_00299c10` moves it): a four-part object (`FUN_0029bdd0`: a part is a layout object with a group, a follow-up group in +0x150
  and an offset / scale; `FUN_0029cae8` places them from the parent's position plus the object's x / y) whose groups come from the
  table 0x381fd0 (three equal sets 1, 0x75, 0x74, 0x76: track [empty], top cap, body, bottom cap; a part's follow-up is the same
  group again, so it loops). x / y = the parent layer's current group x / y; the track is that group's +0x18 (224 on page 3, 200
  on pages 7 / 8), the body's own length the +0x18 of group 0x74 (100). Body scale y = `track / body * min(visible / total, 1)`
  (`total` as given, `visible` 10 or 9). `FUN_00299c10(pos)`: `pos` is clamped to `total - visible` (`total` here is the larger of
  the two numbers); parts 1 and 2 sit at `(int)(track / total * pos)`, part 3 at that plus `(int)(track * min(visible / total, 1))`,
  one less when the list fits and the sum is exact. Port: `marks::Bar` (tested), `place_bar`, `bar_pos`.
- **Page 3 opens on the last entry** (fix): `FUN_002a4c38` ends with `FUN_00298d98(list, n)` and `FUN_00299c10(bar, n)`
  where `n` is the sum of the lines of every entry but the last; the first line shown is the last entry's first line (the
  port started at line 0). The scroll bar is made for `n + max(10, last entry's lines)`.
- **New / seen bit sets** (`marks.rs`). `DAT_0036d4e8` (0x128 bytes, allocated by `FUN_002b7060` at the scene's init;
  `FUN_002b7100` fills it at every open: `FUN_002b7160` characters (+0), `7590` the Ansem reports (entry 4, +0x120), `7818` album
  (entry 2, +0x34 + 4 * world), `7cd0` limits (entry 6, +0x74), `7f58` quests (entry 8, +0x80), `8390` minigames (entry 7,
  +0x78), `87b8` treasures (+0xd0), `8b88` maps (+0x110), `8f18` (+0x124), `9198` story (entry 1, +0x90 + 4 * world)) holds a bit for
  every entry that exists now and is not in the seen set. The seen set is in the save at `FUN_002b70e0` = `0x33e860 + 0x38f0`
  (the same offsets). A tag is drawn for an entry with a new bit: new until the seen bit is set, then the seen group (`FUN_002b9360`
  / `FUN_002b9480` and siblings: story 0x39 / 0x3a at 0x382ba8 / 0x382bac, rows 0x8b / 0x8c at 0x382b80 / 0x382b84, cells 0x98 / 0x99
  at 0x382aec / 0x382af0, characters 0x18 / 0x19 and links 0x1b / 0x1c at 0x382f0c.., treasure and map labels 0x3f / 0x40 at
  0x382f58 / 0x382f5c, album 0x5a / 0x5b at 0x382bdc / 0x382be0). `DAT_0036d4f4` is a second set (the completion percentage
  `FUN_002b9bf0`: every entry that exists, no seen filter); its aggregate (`FUN_002baca8` story, `ba080` album, `ba6d8` treasures,
  `ba898` maps, `bad98` characters / links) draws the completion mark 0x8a on page 2's rows. Port: the app has no journal save
  data, so the seen set lives in the `Journal` resource for the run and starts empty (a state that never opened the journal shows
  everything as new [inferred: the new-game save has the bits clear]); the new set is rebuilt in `show` from the story flags, the
  item counts and the tables below.
  - *Page 3's marks* (`FUN_002a5708`, every cursor move through the list callback `FUN_002a5450`): the entries that start in the
    ten rows get a tag on their first row; the entry whose first line is on row 0 is marked seen first (`FUN_002b95d8`).
    `marks::story_mark_rows`.
  - *Page 2's rows* (`FUN_002a39b8`): per category the second child shows the aggregate (any entry new: 0x8b, all of them seen:
    0x8c) and the first the completion mark 0x8a. Characters: `FUN_002af5e0(0, world)`, links `(1, -1)`.
  - *Page 0's cells* (`FUN_002a2248`): an open world's second child shows `FUN_002b9658` / `FUN_002b9798` (any category new /
    all seen; world 0xf takes every world); the first child's 0x9a (`FUN_002baf08`: every category complete) needs the quest,
    minigame and limit lists the app has no data for: GAP.
- **Characters (6) and Character Links (0xb)** (one builder `FUN_002ae3e8`, handler `FUN_002ae790`; page 6 is category 0, 0xb
  category 7; page 0x20, which shares the builder and is reached from page 0xc, is not ported). The faces are the sub-records (8 bytes: character u16, x, y i16, +6 drawn when not 0)
  of the groups of `jiminy.bar` entry 5 (0xc-byte headers: +0 flag, +2 off flag, +4 family = the journal world or 0x10 for the
  links, +5 count, +6 kind: 0 base, 1 flag, 2 item count, +8 offset), `FUN_002af398`: the base group when it is on
  (`FUN_002a2018`: kind 2 reads `FUN_001a1cc8(item) > 0`, else the story flag, and not the off flag), then every group of the
  family that is on, in file order. A face is a frame object (group from 0x382f30 by the character record's style byte +2: 0xa..0xf,
  0x10..0x15 for the links), its picture (a child in sequence 0 of `jmemo.2ld`: groups 1 then 2, links 4 then 5, at
  0x382f20 / 0x382f24 / +6; the texture is picture `+1` of `jmface_<name>.2ld`, `<name>` from the pointer table 0x36d3f8 by
  family: tt hb bb he al mu po lk lm dc wi nm ca tr eh, "", ex, heart, no) and a tag child. Entry 3 ("char", 0x24-byte records): +1 picture, +2 style,
  +4 bit, +6 name message, +0x10 flags (0x400: the world's name is the message's variable, `FUN_00191a30`). The pointer is object
  0x7571 group 0x22 moved three units a frame while a direction is held inside the bounds of group 0x52 (its anchor to its x /
  y: (60, 80) to (490, 370); `FUN_002aeea8`, digital branch; left beats right, up beats down), first at the middle of the bounds
  or on the face the page was left on (`FUN_002aebb8`). The face under it is the first whose centre is within half the frame
  group's +0x20 (32) on both axes (`FUN_002af180`, inclusive); its name goes on the plate (group 0x53, font 3, centred, the
  group's colour) and the hover frame (0x16, 0x1a on the links, a child of the face's picture) appears. The tag of a face:
  0x18 / 0x19 (links 0x1b / 0x1c) for a character with a new bit. Cancel goes back to page 2 (row kept), Start closes; a confirm
  opens the character's own page 0xf (`FUN_002aa358`, with its list pages 0xe / 0x10): **not ported, refused with a log line**,
  so a character's seen bit (`FUN_002b73b8`, set there) never gets set in the app. Help line 0x48d5 (page 6), 0x48d6.
  Run (the state of an area that has Sora, Donald, Goofy and the friends): `local\x\state-c.json`-like
  `{"story_flags": [..., 2197, 2201, 5161]}`, `KH2_KEYS=7:Enter,8:Down,8.2:Down,8.4:Down,8.6:Down,9.2:J,10.5:J` (page 6),
  `KH2_JOURNAL_HOLD=11.5:Down:0.6` holds a direction for the pointer (the scripted keys are released every frame while the
  window has no focus), links: five Downs on page 2, then J. Log: `journal: page 0x6 (FUN_002ae3e8): family 0x0, 6 faces ...`,
  `journal: pointer [275, 303] -> face Some(2)`.
- **Treasures (7) and Maps (8)** (one builder `FUN_002afca0`; objects `FUN_002b0e78`; redraw `FUN_002b0678`, `FUN_002b09e8`,
  `FUN_002b0cc8`; handler `FUN_002afe98`; cursor `FUN_002b1890`; pages 0x19 / 0x1a, the same builder reached from page 0xc [inferred: Roxas's], are not ported). The rows of the table `trsr`
  (`03system.bin`, `DAT_01c95590`, 0xc-byte rows, iterated by `FUN_001a68d0`): treasures (`FUN_002b00e0`) are the chests (type 0)
  whose flag (+8) is neither 0 nor 0x800 and is set (or the all-worlds flag 0x4837 `FUN_002783f8`); maps (`FUN_002b0228`) are
  the rows whose item is of type 0x16, owned (`FUN_001a1cc8`) or listed by the all-worlds flag. An entry: the journal world
  index of the row's game world (`FUN_00277c88`), found (a treasure: the chest's bit `FUN_001a6a90` at `DAT_0033fdcc`; a map: owned),
  the row's ordinal and item. Sorted by world then ordinal (`FUN_002b0358`); a section per world of ceil(n / 8) rows, an open
  world without entries keeps one (empty) row, treasures leave out world 8 (`FUN_002b1650`). Page objects: layers 0x3b and 0x43,
  nine rows of eight cells (group pitch 28 x 28 from 0x48; found / not found 0x48 / 0x47, maps 0x3e / 0x3d, blank 2), a label per
  row (font 3, group 0x46, shown where a section starts; the world's name, or one question mark per glyph while the world's cell
  does not exist `FUN_0029bb48`: glyph 41, the sprintf of `FUN_001918a0` turns the ASCII `?` into it) with "There's nothing yet!"
  (0x4a0f / 0x4a12) at the label's offset for an empty section and the new tag 0x3f / 0x40, the info text under the cursor
  (`FUN_002b0cc8`: found: the item's name, with its second icon on page 7 (`FUN_001a1ee0`: control 9, item +0x17); not found:
  question marks; group 0x7e, centred, offset = the layer group's x / y), the total `found/listed` (only with the all-worlds
  flag; label 0x4a11 / 0x4a13 group 0x3c, count group 0x7f), the scroll bar (parent 0x43) and the cursor frame 0x41 (0x42 on
  an empty row) with the list hand (0x21, `FUN_00277158`) on it. The page opens scrolled to the journal world's section
  (`FUN_002b1e18`). Cursor (`FUN_002b1890`, `lists::navigate`, tested): Down / Up step a row and scroll at the view's edge, a new
  press wraps at the ends; L1 / R1 move a view (to the end, then to the row's last cell, then wrap); Left / Right wrap inside
  the row; the column is cut to the new row's cells. Each move marks the world under the cursor seen (`FUN_002b0df8` ->
  `FUN_002b8a98` / `FUN_002b8e38`). Help 0x48f4 / 0x48f5. **The app has no opened-chest data, so every listed chest is "not
  found"** and a world's treasure completion mark needs a world without chests. Run: `local\x\state-t.json`-like flags of the
  chests (`2256 2210 2286` for Twilight Town), `KH2_KEYS=...,10:Down,10.2:Down,10.4:Down,11:J` (treasures), four Downs
  (maps; the all-worlds flag 18487 lists unowned maps). Log: `journal: page 7 (FUN_002afca0): 164 entries (164 new), 0 found,
  24 rows, cursor ..., section rows [...]`, `journal: page 7 cursor Some(Cursor { top: 9, at: 1 }) after Right`.
- **Album (4)** (`FUN_002a5a68`, handler `FUN_002a6010`, helpers `FUN_002a5ef0`, `FUN_002a6e48`, `FUN_002a6e78`). Entry 2 ("albu",
  0xc bytes): +0 world, +1 / +2 two ASCII digits, +6 flag, +8 title, +0xa caption (messages of `jm.bar`). The list: the
  records of the world whose flag is set (or all of them with the all-worlds flag: those show question marks), eight rows from
  layer 0x58 (pitch 26), font 3, group 2's colour, a tag child (0x5a new / 0x5b seen), or one row "There's nothing yet!"
  (0x32dd). The row under the cursor counts as seen. Up / Down wrap (the list widget), cancel goes to page 2 (row kept), a confirm
  opens page 5; page 4's own first row is the one page 5 kept (`FUN_00277de8(4)`), else 0. Help 0x48d8.
  - *Picture page 5* (`FUN_002a62d0`, `FUN_002a6850`, `FUN_002a6c80`, `FUN_002a6bd8`, `FUN_002a6ad0`): layer 0x54, the picture
    (a sequence-1 object, groups 1 then 2, at the layer group's x / y (160, -113)) from `jm_photo/<world code><digits>.bin` (a BAR
    of one image), the title (text object group 0x56) and the caption (list row 0, group 0x55). L1 / R1 go to the next / previous
    existing picture (`FUN_002a6850`, wrapping), cancel goes back to page 4 with the row kept (`FUN_00277dc8`), help 0x48d9.
- **Gaps (WP-MN12).** The characters' own page 0xf and its list pages (`FUN_002aa358`, `FUN_002ab7d8`, `FUN_002a98f0`; confirm on a
  face is refused); the album's and the characters' seen bits of the entries only those pages mark; the minigames and quests
  pages (9, 10: `FUN_002b5e50`, `FUN_002b4b18`) and Roxas's pages 0xc, 0x19 .. 0x20; page 0's completion mark 0x9a and the quest /
  minigame / limit aggregates; opened chests (treasures never show as found); the wipe, sounds and the `FUN_002779f8(0, 2)` header
  hide the handlers do when a direction is pressed; the stick-driven pointer (`FUN_002aeea8` analog branch: `stick * 6 + 0.5`);
  the held-pad auto-repeat of the cursor moves (the port takes one step per press, wraps on every press).
  Text colour of page 2's rows (see the rule above) was left at the WP-MN10 reading.
- **WP-MN14 status.** The character page 0xf, the Quests and Minigames pages, page 0xc and page 0's completion mark 0x9a are ported (20.10); what stays open is the 3D model, the enemy pages, the Ansem reports, limits and the moogle list.
- **WP-MN15 status.** The text lists of page 0xc (the Character Files worlds and characters, the Heartless, the Nobodies, the Ansem reports, the limits) and the pages behind them, the enemy pages with the defeated-enemy count and the reaction rows, the moogle summary and world 0xf's mark 0x9a are ported (20.11); the defeated-enemy counters, the reaction use counts and the limit counts are saved in the slot (`docs/progress.md`). Open: the 3D model pages. WP-HK1 added the writers: see 20.11a.
- **WP-MN13 status.** Nothing in the app opens chests yet (gap, WP-FX12c): the original sets a chest's bit in `FUN_001a6910` (reached by `FUN_001a6a58` from the chest-open sequence `FUN_001a6db8`, and by `FUN_001a6a78` from the reward list of an area trigger in `FUN_001978b0`, whose only callers are `FUN_00197670` and `FUN_00197a60`), and the app has no chest object and does not run a trigger's reward list, so a chest is "found" only through a state file. Done: persistence. `Progress` keeps the opened chests (`chest_opened`, record bits from 224 on) and the journal seen set (`journal_seen`, slot key `journal_seen`); `Marks` loads it when the journal opens (`load_seen`) and the tick writes new marks back (`take_dirty`); the Treasures page marks chests found from the chest bit and a world's treasure completion needs every chest of it opened (`treasures_complete`). Not started (no decompile work done beyond reading): the character page 0xf (`FUN_002aa358` builds layer group 0x49, name 0x4a, sub-title 0x4b from `char` +0xa, a counter row 0x4c when `char` +0x10 has 0x800, scroll arrows 0x4f / 0x50, the face frame by style from 0x382ea0, the description message `char` +8 in a 6-line scissored view (`FUN_002ab678`); pages 0x12 / 0x15 add the four drop rows, so those are the enemy pages; the face is loaded through the slot cache `FUN_002a07c8`), the Quests / Minigames pages (page 9 reads entry 8 via `FUN_002b6478` / `FUN_002b6650`, page 10 entry 7 via `FUN_002b5130` / `FUN_002b52c0`; ten rows of group 0x68 / 0x6a with a child 2, world header rows group 100, values from the minigame records at `0x341620 + 8 * n` (`FUN_001d36e8`: byte 0 state, +4 score) and `FUN_002b6d40` for quests; both lists are empty without the all-worlds flag since the app keeps neither record), page 0xc, Roxas's pages, page 0's mark 0x9a.

- **Decompile index (MN12).** `FUN_00299878`, `FUN_00299bf0`, `FUN_0029bdd0`, `FUN_0029bfa0`, `FUN_0029be88`, `FUN_0029c0b0`,
  `FUN_0029c130`, `FUN_0029c208`, `FUN_0029c2d8`, `FUN_0029c408`, `FUN_0029c440`, `FUN_0029c490`, `FUN_0029c568`, `FUN_0029c5b0`,
  `FUN_0029c880`, `FUN_0029c8f0`, `FUN_0029cae8`, `FUN_00299c10`, `FUN_00277e58`, `FUN_00298d98`, `FUN_00298d58`, `FUN_00298e08`,
  `FUN_00296fb8`, `FUN_002a4c38`, `FUN_002a5450`, `FUN_002a5708`, `FUN_002a5060`, `FUN_002b7060`, `FUN_002b70a8`, `FUN_002b70e0`,
  `FUN_002b7100`, `FUN_002b7160`, `FUN_002b7208`, `FUN_002b72c0`, `FUN_002b73b8`, `FUN_002b7408`, `FUN_002b74b8`, `FUN_002b7590`,
  `FUN_002b7668`, `FUN_002b76e8`, `FUN_002b7818`, `FUN_002b7928`, `FUN_002b79d8`, `FUN_002b7af8`, `FUN_002b7c50`, `FUN_002b7e28`,
  `FUN_002b7f58`, `FUN_002b8048`, `FUN_002b8390`, `FUN_002b87b8`, `FUN_002b88b0`, `FUN_002b8990`, `FUN_002b8a98`, `FUN_002b8c58`,
  `FUN_002b8d38`, `FUN_002b8e38`, `FUN_002b8fd0`, `FUN_002b9058`, `FUN_002b9198`, `FUN_002b92b0`, `FUN_002b9360`, `FUN_002b9480`,
  `FUN_002b95d8`, `FUN_002b9658`, `FUN_002b9798`, `FUN_002b99d0`, `FUN_002b9ab0`, `FUN_002b9b28`, `FUN_002b9bf0`, `FUN_002b9ce8`,
  `FUN_002b9e30`, `FUN_002b9f68`, `FUN_002baca8`, `FUN_002ba080`, `FUN_002ba6d8`, `FUN_002ba898`, `FUN_002bad98`, `FUN_002baf08`,
  `FUN_002a1c30`, `FUN_002a1f58`, `FUN_002a1f78`, `FUN_002a1ff8`, `FUN_002a2018`, `FUN_002a2090`, `FUN_002a39b8`, `FUN_002af5e0`,
  `FUN_002ae3e8`, `FUN_002ae790`, `FUN_002ae230`, `FUN_002ae2a0`, `FUN_002af180`, `FUN_002af2b0`, `FUN_002af398`, `FUN_002af688`,
  `FUN_002af6c0`, `FUN_002af730`, `FUN_002af7a0`, `FUN_002af800`, `FUN_002aebb8`, `FUN_002aecf0`, `FUN_002aee60`, `FUN_002aeea8`,
  `FUN_002afa00`, `FUN_002aa358`, `FUN_002a19f8`, `FUN_002a18e8`, `FUN_002a19c8`, `FUN_002a1858`, `FUN_00278658`, `FUN_00277d60`,
  `FUN_0029d1d0`, `FUN_0029d820`, `FUN_002afca0`, `FUN_002afe98`, `FUN_002b0228`, `FUN_002b00e0`, `FUN_002b0358`, `FUN_002b0e78`,
  `FUN_002b1e18`, `FUN_002b0df8`, `FUN_002b0c98`, `FUN_002b0678`, `FUN_002b09e8`, `FUN_002b0cc8`, `FUN_002b1c30`, `FUN_002b1ca0`,
  `FUN_002b1d78`, `FUN_002b1398`, `FUN_002b1890`, `FUN_002b16e0`, `FUN_002b16a0`, `FUN_002b1650`, `FUN_002b1660`, `FUN_002b1680`,
  `FUN_001a68d0`, `FUN_001a66e8`, `FUN_001a6a90`, `FUN_001a1cc8`, `FUN_001a1ee0`, `FUN_0029bb48`, `FUN_00191660`, `FUN_00191020`,
  `FUN_00190f68`, `FUN_001918a0`, `FUN_00277158`, `FUN_002771c0`, `FUN_0029a9c0`, `FUN_002783f8`, `FUN_002a3120`, `FUN_002a3008`,
  `FUN_002a5a68`, `FUN_002a6010`, `FUN_002a6578`, `FUN_002a5ef0`, `FUN_002a5fd8`, `FUN_002a6e48`, `FUN_002a6e78`, `FUN_002a59d8`,
  `FUN_002a62d0`, `FUN_002a6850`, `FUN_002a6c80`, `FUN_002a6b80`, `FUN_002a6bd8`, `FUN_002a6a38`, `FUN_002a6ad0`, `FUN_00139048`,
  `FUN_00138fc8`, `FUN_00139860`, `FUN_00139888`, `FUN_00237518`, `FUN_00237618`, `FUN_00237638`, `FUN_00237658`, `FUN_00237678`,
  `FUN_00237698`; data 0x381fd0, 0x381fe8, 0x382aec..0x382af0, 0x382b80, 0x382ba8..0x382bac, 0x382bdc..0x382be0, 0x382ef4..0x382f64,
  0x382f50, 0x3828b8, 0x36d3f8..0x36d460, 0x33fdcc.
- **Tests (WP-MN12).** `menu_journal::marks::tests::*` (tags, aggregates, the mark rows, the thumb), `chars::tests::*` (groups of a
  family, items, pointer, hit test, tags), `lists::tests::*` (rows and sections, the start, the cursor moves, pages, the view),
  `treasure_page::tests::*`, `album_page::tests::*`, `menu_journal::tests::{story_records_exist_with_their_flag_and_complete_with_the_last,
  the_disc_face_page_data, the_disc_list_page_data, the_disc_category_page_tables}`.

## 21. The pause-menu record table and the Struggle's result menu (WP-STRM)

### 20.10 A character's own page, Quests, Minigames, page 0xc (WP-MN14) [V unless marked]

Code: `menu_journal.rs` (`build_char_page`, `char_view`, `page_char`), `menu_journal/{games,games_page,root_page}.rs`; the minigame records in
`progress.rs` (slot key `minigames`, docs/progress.md).

- **A character's own page 0xf** (confirm on a face of page 6 / 0xb; `FUN_002ae790` first marks the character seen, `FUN_002b73b8(char)`,
  then `FUN_002aa358(list, page)` keeps the page's face list (the u16 character ids at +0x50, 8-byte stride) and the page to return to
  in `DAT_0036d49c`, stores the face as the page-0xe cursor (`FUN_00277dc8(0xe, min(face, 9), max(face - 9, 0))`) and builds with
  `FUN_002a98f0`). Objects (`FUN_002aadf0`, parent the page header unless said): the layer `DAT_0036d100` (the same object as page 6's
  0x51 layer, re-made) group 0x49; under it the face frame (group = the u16 table 0x382ea0 by the record's style, 10 .. 15) and, from
  `FUN_002ab8f0`, the face picture (the picture `char` +1 of `jmface_<family>.2ld`, groups 1 then 2 as on page 6), both offset by the
  layer group's x / y (`FUN_00138ed0(.., FUN_00237618, FUN_00237638)` [inferred: the group's x / y, as the face pages' bounds]); the name
  (group 0x4a, font 3, `+0x198 = 0x60000008`: the ink [8, 0, 0, 0x60] as page 3's text; the message `char` +6, with the world's name as its
  variable when `+0x10` has 0x400) and the sub-title (0x4b, `char` +0xa when not 0); the description message `char` +8 drawn by
  the list row's callback `FUN_002ab678` in font 3 with the page's pitch and size from the layer group (as page 3: `story_metrics`), a scissor
  of seven lines (`FUN_0014b6c0(x, y, 0x200 - x, pitch * 7)`) and the scroll position as a line offset; the list count is `lines - 6` when
  the text has more than seven lines, else 1 (`FUN_00298d58`); the two arrows 0x4f / 0x50 are shown by `FUN_002ab578` (none with one position,
  the up arrow hidden at the first, the down arrow at the last). Port: `chars::{desc_positions, arrows, desc_scroll}` (tested). Help line
  0x48f2 (`FUN_002ac450`, pages 0xf / 0x12 / 0x15). The list flags (`FUN_002a98f0`: 0x200, plus 0xc000 when more than one listed character
  exists) enable L2 / R2 (-11 / -10); with `FUN_00298fe0(list, 1)` and `FUN_00299010(list, 7)` Up / Down scroll one line and L1 / R1 a page
  of seven [inferred: clamped, as page 3's list].
  Handler `FUN_002a9b80`: -11 / -10 call `FUN_002aa600` (`chars::neighbour`: the next / previous listed character that exists, wrapping
  to the first / last one that does, else the same one; `FUN_002ac4c8` marks it seen) and rebuild the page; -2 (cancel) goes to page 0xe /
  0x11 / 0x14, whose handler `FUN_002a9590` re-enters the page `DAT_0036d49c` with `FUN_002ae3e8(stored cursor)`: the face page with the
  pointer on the character last shown (`first_pointer(.., Some(face))`); -4 closes; -6 opens page 0x10 (the 3D model viewer).
  Run (log lines `journal: page 0xf (FUN_002a98f0): character 116 of 6 (...)`, `journal: page 0xf R2 (-10) (FUN_002aa600) -> character 117`,
  `journal: page 0xf cancel -> page 0x6 with face 4 ...`): `local/x/state-c.json` = `{"story_flags": [2097, 4098, 5122, 6195, 7204, 8193,
  9217, 10243, 2197, 2200, 2201, 2204, 5161]}`, `KH2_KEYS=7:Enter,8:Down,8.2:Down,8.4:Down,8.6:Down,9.2:J,10.5:J,13:J`,
  `KH2_JOURNAL_HOLD=11.5:Down:0.6`, `KH2_SCREENSHOT=.. KH2_SHOT_AT=15`: Hayner's page (the frame group 12 with his picture at the top left
  of the page, the name on the plate, six lines of description and the down arrow; screenshot looked at). Tests:
  `chars::tests::{the_description_scrolls_by_lines_and_pages, the_character_pair_skips_those_that_do_not_exist_and_wraps}`,
  `menu_journal::tests::the_disc_character_page_data` (every character of every family has its name, description and sub-title messages; the
  frame table is 10 .. 15).
  Not ported: the 3D model (`FUN_002abf60` loads the model `char` +0xc with the placement fields +0x12 .. +0x20 into the object at
  `DAT_0036d494` / `FUN_00295bc0`) and the page 0x10 view (-6, rotate -13 / -12 `FUN_002ac228`), the counter row (group 0x4c, message
  0x489b, `FUN_002a1f78`: the defeated-enemy count of a record with flag 0x800, from `FUN_00187e68(save +0x1660, enemy +0x4c)`), and the
  four drop rows of the enemy pages 0x12 / 0x15 (the enemy lists 0xd .. 0x16 are not ported).
- **Quests (9) and Minigames (10)** (`FUN_002b5e50` / `FUN_002b4b18`, handlers `FUN_002b6120` / `FUN_002b4dd8`). Layers 99 (0x63) and 0x6b
  (holds the scroll bar's track, `FUN_00299878(.., total = max(rows, 10), visible 10)`), ten row objects (`FUN_002b6650` / `FUN_002b52c0`: group 0x68
  or 0x69 for a quest, 0x6a for a game, font 3, the ink 0x60000008, a child 2 for the tag; a game row also has a text child at the row
  group's x / y with the value, right aligned), the list count `rows - 9` when there are more than ten (`FUN_00298d58`), L1 / R1 scroll a page of ten
  (`FUN_00299010(list, 10)`). Row texts and marks (`FUN_002b6920`, `FUN_002b5698`): a world header (group 100, `FUN_002a3070(world, 0)`
  or question marks while the world's cell does not exist, `FUN_002a3120`), a quest group header (group 0x66, the message with the group's
  number, question marks unless some quest of the group is known or done, `FUN_002b6fb8`), a quest row (0x68; 0x69 when done; the name
  `ques` +4, question marks for the all-worlds listing), a game row (the name `mini` +2, question marks below state 2, the value text);
  the tags are the words 0x382ff8 (quests) and 0x382fec (games), both 0x39 (new) / 0x3a (seen); the rows in view are marked seen when
  drawn (`FUN_002b6c20`: the quest's index in entry 8 into the seen set +0x80; `FUN_002b5aa8`: the game's index in entry 7, +0x78). Help
  lines 0x48f6 / 0x48f7. The page opens on the journal world's header row (`FUN_002b63c0` / `FUN_002b5078`: the first row with the world's header, kept inside
  the list count) except from page 0xc, where it opens at the top (pages 0x1b / 0x1c). Cancel goes to page 2 (page 0xc from 0x1b / 0x1c).
  - *The lists.* Quests (`FUN_002b6478`, `jiminy.bar` entry 8 "ques", 0x10-byte records: +0 world, +2 group, +4 name message, +6 test flags,
    +8 the flag / bit, +0xa the minigame record, +0xc the limit, +0xe the done flag / bit): a record is listed when `FUN_002b6d40` gives not 0;
    a header row for a world's first listed record and one for a record of another group than the last one. Minigames (`FUN_002b5130`, entry 7
    "mini", 8-byte records: +0 world, +2 name, +4 unit message, +6 the minigame record number): listed when the record's state byte
    (`FUN_001d36e8(n) = 0x341620 + 8 n`) is 2 or more, or with the all-worlds flag; a header per world. Page 2's rows 5 / 6 need a
    listed quest of the world (`FUN_002b6f00`) / a game with state above 1 (`FUN_002b5bb0`) or the all-worlds flag. Port: `games::{quest_rows,
    game_rows, quests_exist, games_exist, first_row, positions}` (tested).
  - *The quest test* `FUN_002b6d40` (`games::quest_state`): known when the flag +8 is set (a story flag with bit 1 of +6, else the record bit
    `FUN_001a5328`) and, with bit 4, the minigame record's state is not 0; not known: 3 with the all-worlds flag (the name shows as
    question marks), else 0. A known quest is done (2) by the first test of its flags: 0x10 the story flag +0xe, 0x20 the record bit +0xe,
    0x1000 the game's state not 0, 0x2000 state 2 or more, else with a state of 2 or more the score (state 4 is a time, whole seconds
    rounded up: `FUN_0014c7f0` splits the word, 0x34bc0 frames an hour and 0xe10 a minute) compared with +0xc: 0x100 done at or below,
    0x200 done at or above; otherwise 1 (open). The disc's quests use the flags 0x11, 0x21, 0x145, 0x245, 0x246, 0x1041 and 0x2042.
  - *The minigame value* (`FUN_002b5698`, the branch for `FUN_00105cd8() != 1`, which is the USA disc's [inferred: docs/hud.md takes
    value 1 for Japanese]): state 0 / 1 the message 0x4b71; state 2 the message 0x4b80 ("Round") then the score `%d`; state 4 the unit
    message and the time `%02d'%02d"%02d` (minutes, seconds, hundredths = `frames % 60 * 1.6667`); other states the score and then the unit
    message. `FUN_001918a0` maps the apostrophe and the quote to glyphs 55 and 65.
  - *New bits, aggregates, the world mark.* `FUN_002b7f58` (quests: the records whose state is 1 or 2), `FUN_002b8390` (games: state 2 or
    more) feed the seen sets (`Cat::Quest` 5, `Cat::Minigame` 6 in the slot's `journal_seen`); the completion buffers (`FUN_002ba2c0`,
    `FUN_002ba450`) take the quests that are done and the games with state 2 or more, `FUN_002ba3a8` / `FUN_002ba538` ask whether every one
    of a world is in them. Page 0's cell mark 0x9a (`FUN_002baf08`, world below 0xf): the album (`FUN_002ba080`), quests, games, treasures,
    maps, story and characters (`FUN_002baca8`, `FUN_002bad98(0, world)`) and the links (`FUN_002bad98(1, world)`) of the world are all
    complete (`menu_journal::world_complete`); world 0xf adds the limits (`FUN_002ba238`) and the Ansem reports, which have no app data and
    are not drawn. Page 2's rows 5 / 6 get their new tag and completion mark.
  - *Persistence.* `Progress::minigame(n)` / `set_minigame(n, state, score)` (slot key `minigames`, triples). Nothing in the app plays a
    minigame, so only a state file fills them.
  Run (screenshots looked at): `local/x/state-q.json` = the state-c flags plus 2224 and `"minigames": [[14,4,720],[12,1,0],[15,2,150],
  [11,4,3600],[13,3,77],[16,2,5]]`; `KH2_KEYS=7:Enter,8:Down,8.2:Down,8.4:Down,8.6:Down,9.2:J,10:Down,10.2:Down,10.4:Down,10.6:Down,10.8:Down,11.2:J`
  (quests; one more Down for the minigames): the quest page shows "Twilight Town" / "Mail Delivery" / "Try again" / "Complete within 14
  seconds" with the orange mouse marks and the scroll bar; the minigame page shows five rows with "Time: 00'12"00", "Round 150",
  "Time: 01'00"00", "77Time:" (the score of a game whose unit is the message "Time:", as the table has it) and "Round 5". With the all-worlds
  flag (18487) the lists hold 114 quest rows and every minigame, as question marks. Log lines: `journal: page 9 (FUN_002b5e50 / FUN_002b4b18):
  world 0, 21 rows [...]`, `journal: page 9 first row 10`. Tests: `games::tests::*`, `menu_journal::tests::the_disc_quest_and_minigame_data`.
- **Page 0xc** (`FUN_002a7220`, handler `FUN_002a7978`; world 0xf's page, reached from page 0 for an id above 0xe, and by L1 / R1 from
  pages 0 and 2: `hints`). Layers 0x85 and 0x8f, the two `jm_top_jm.2ld` pictures (both in their second group, `FUN_002a7ec0(0)`), the title (world 0xf's
  name), up to eleven rows of the categories listed by `FUN_002a80c0` (`root_page::listed`): 0 the Ansem reports (page 0x17: needs a report
  owned, `FUN_002ade10`), 1 / 2 / 3 the character lists (0xd, 0x11, 0x14), 4 / 5 treasures / maps (0x19, 0x1a), 6 quests (0x1b: listed
  quest), 7 minigames (0x1c: played game), 8 (0x1d: a limit used, `FUN_002b37d8`), 9 (0x1f: the moogle's first word or save word +0x3750,
  `FUN_002b4838`), 10 the character links (0x20); the all-worlds flag lists every category. The row texts are the words at 0x382d08
  (13945 .. 13948, 18650 .. 18656), the help lines (`FUN_002a81e8`, at the cursor) the words at 0x382d20, the new tag groups the words at
  0x382d00 (0x87 / 0x88), the completion mark 0x86 (`FUN_002a7558`). A confirm sets the tab's text from 0x382d08 and opens the row's page
  (`FUN_002a7978`'s switch: 0x17, 0xd, 0x11, 0x14, 0x19, 0x1a, 0x1b, 0x1c, 0x1d, 0x1f, 0x20); the pages the app has are run as
  7, 8, 9, 10 and 0xb with a cancel back to page 0xc (`Open::from_c`; they open at their first row: `FUN_002b1e18` zeroes the scroll of
  pages 0x19 / 0x1a, and `FUN_002b6120` / `FUN_002b4dd8` return to 0xc). The others (the Ansem reports, the heartless and nobody lists,
  categories 8 and 9) were refused with a log line until WP-MN15 (they open since, 20.11). Tests: `root_page::tests::*`, `menu_journal::tests::the_disc_character_page_data` (the
  table words, the groups).
  Run: `local/x/state-q.json`, `KH2_KEYS=7:Enter,8:Down,8.2:Down,8.4:Down,8.6:Down,9.2:J,10:Escape,11:Left,12:J` (screenshot looked at: the
  picture of Jiminy, "Character Files" with NEW, "The Heartless", "The Nobodies", "Treasures", "Maps", "Missions", "Minigames",
  "Character Links", the help line "See character information!", the hint buttons "Select World"); then `13:Down` x5, `14.2:J` the
  quests, `15.5:Escape` back.
- **Gaps (WP-MN14).** The 3D model and the model view page 0x10 of a character (-6); the counter row and the defeated-enemy counts of
  the enemy pages; the enemy lists 0xd .. 0x16 and their drop rows; the Ansem reports (0x17, 0x18, entry 4); categories 8 (0x1d, 0x1e: limits,
  entry 6) and 9 (0x1f: the moogle list, `FUN_002b3d80`); page 0x20 as its own page (it runs as page 0xb); Roxas's own pages 0x19 / 0x1a
  (they run as the world pages 7 / 8, whose world list is the same table [inferred]); the page-turn sound and wipe the handlers start on
  a direction (`FUN_002779f8`); the limits' part of page 0's completion mark for world 0xf (`FUN_002ba238`); the Ansem reports' new tag in
  page 0xc's row 0. Nothing in the app sets the opened-chest bits (WP-MN13) or plays a minigame, so only a state file shows found chests
  and minigame scores.
- **Decompile index (MN14).** `FUN_002aa358`, `FUN_002a98f0`, `FUN_002aadf0`, `FUN_002ab678`, `FUN_002ab7d8`, `FUN_002ab838`, `FUN_002ab8f0`,
  `FUN_002ab578`, `FUN_002ac380`, `FUN_002ac450`, `FUN_002ac4c8`, `FUN_002aa600`, `FUN_002a9b80`, `FUN_002a9590`, `FUN_002aa0b0`, `FUN_002abf60`,
  `FUN_002abdc8`, `FUN_002abef0`, `FUN_002ab540`, `FUN_002a1fc0`, `FUN_002a1f78`, `FUN_002a07c8` (only through its callers), `FUN_00298fe0`,
  `FUN_00299010`, `FUN_002977a8`, `FUN_00297148`, `FUN_002b73b8`, `FUN_002b5e50`, `FUN_002b4b18`, `FUN_002b6120`, `FUN_002b4dd8`, `FUN_002b6478`,
  `FUN_002b6650`, `FUN_002b6920`, `FUN_002b6c20`, `FUN_002b6c00`, `FUN_002b63c0`, `FUN_002b6d40`, `FUN_002b6618`, `FUN_002b6fb8`, `FUN_002b6f00`,
  `FUN_002b5130`, `FUN_002b52c0`, `FUN_002b5698`, `FUN_002b5078`, `FUN_002b5aa8`, `FUN_002b5a88`, `FUN_002b5288`, `FUN_002b5bb0`, `FUN_002b7f58`,
  `FUN_002b8048`, `FUN_002b80c8`, `FUN_002b81a0`, `FUN_002b8390`, `FUN_002b8470`, `FUN_002b84f0`, `FUN_002b85c8`, `FUN_002ba3a8`, `FUN_002ba538`,
  `FUN_002ba2c0`, `FUN_002ba450`, `FUN_002baf08`, `FUN_002a7220`, `FUN_002a7978`, `FUN_002a7558`, `FUN_002a7ec0`, `FUN_002a7d80`, `FUN_002a7e48`,
  `FUN_002a7190`, `FUN_002a80c0`, `FUN_002a81e8`, `FUN_002ade10`, `FUN_002b37d8`, `FUN_002b4838`, `FUN_002b4798`, `FUN_002b47c8`, `FUN_002ad6a0`,
  `FUN_00277740`, `FUN_00277860`, `FUN_00277b20`, `FUN_00277eb0`, `FUN_00277e18`, `FUN_00277dc8`, `FUN_001d36e8`, `FUN_001a5328`, `FUN_0014c7f0`,
  `FUN_00105cd8`, `FUN_001918a0`, `FUN_002b1e18`, `FUN_002b16a0`, `FUN_002b1650`.

### 20.11 The text lists, enemy pages, Ansem reports, limits and moogle (WP-MN15) [V unless marked]

Code: `menu_journal/{name_list,enemies,report_page,moogle_page,kills}.rs` (`enemies`, the data parts of the others and `kills` hold the pure
rules with tests; the rest is node glue), `chars.rs` (the record family, `enemies_present`, `family_list`), `marks.rs` (`Cat::Report`,
`Cat::Limit`, `Cat::Moogle`), `progress.rs` (the counters below). Page 0xc now opens every row it lists: nothing is refused any more.

- **Defeated-enemy counts (the writer).** The enemies' defeat method `FUN_00184a30` (reached by `FUN_001acfe0` and `FUN_001b02f0`) calls
  `FUN_00187e08(0x33fec0, id)` with `id = FUN_001699a8(actor)` = the NeoStatus (`objentry` `+0x4c`) of the defeated object. The function does
  nothing unless story flag 0x8d0 is set (`FUN_00180850` -> `FUN_001802e0`), finds the word with `FUN_00187dc8` (`save + 0x1660 + 0xff0 + 4 *
  (id - 1000)` for ids 1000 .. 1046, `+ 0x1170 + 4 * (id - 3000)` for 3000 .. 3011, nothing else) and adds 1 up to 9999. The party members' defeat
  method `FUN_001c46a8` counts the ids 0x404 / 0x405 the same way [the journal pages do not read those slots; not traced further]. The reader is `FUN_002a1f78(record)`:
  for a `char` record with flag 0x800 it takes the objentry named by the record's `+0xc` (`FUN_0016e208`) and returns the counter of its NeoStatus
  (`FUN_00187e68`). Port: `Progress::{enemy_counter_id, enemy_defeats, count_defeat, set_enemy_defeats}` (slot key `enemy_kills`, pairs
  `[NeoStatus, count]`), `menu_journal::kills`: a system watches the `Enemy` components, counts each one whose HP has reached 0 once (the `Counted`
  marker) under the NeoStatus of its spawn record's object id (`NeoByObject`, read from `00objentry`; an enemy of the harness's own waves has
  object id 0 and counts nothing). The gate is the original's: in a plain run (no state file) the app's baseline leaves story flag 0x8d0 clear, so no count rises until a state file or an area script sets it. The app's defeat point is outside this work package's files; the hook is read-only. Tests
  `progress::tests::enemy_defeat_counters_count_and_are_saved`, `kills::tests::*`, `menu_journal::tests::the_disc_enemy_list_data` (every one of
  the 56 enemy records has a distinct objentry NeoStatus with a counter slot).
- **Existence of a record** (`FUN_002a1c30`, run by `FUN_002b7160` and by page 0xd's mask): the first loop sets the bit of every `char` record whose
  `FUN_002a1f78` is not 0, that is the enemy records (flag 0x800) of an enemy defeated at least once; the second loop (ported by MN12) the
  sub-records of every group that exists. `name_list::exist_set` is the union; it feeds the new bits, the lists and the completion marks. The record
  index is also its bit number (`+4`, checked on the disc).
- **Record families** (`char` `+0`): 0 .. 0xe the worlds, 0x10 the links, 0x11 the Heartless (57 records, 46 with the counter flag), 0x12 the
  Nobodies (11, 10 with it). `FUN_002ab7d8` names the face file by it: `ex` for 0x10, `heart` for 0x11, `no` for 0x12 (the table at 0x36d3f8).
- **Page 0xd** (`FUN_002a8a38`, handler `FUN_002a8e90`; page 0xc row 1 "Character Files"). Ten list objects (layers 0x58 and 0x5d, the second holds the
  scroll bar's track, pitch from group 0x58), one row per world: a mask built by `FUN_002aa430` (bit 0 always, bit `w + 1` for each world below
  0xf whose cell exists or with the all-worlds flag); bit 0 shows world 0x10's name ("Other"), bit `k` world `k - 1`. A world whose cell does not
  exist shows question marks. The tag is the aggregate of the family's records (`FUN_002b7408` any new, `FUN_002b74b8` all seen; the words at
  0x382e90 / 0x382e94 = groups 0x5a / 0x5b). A confirm keeps the row and the scroll (`FUN_00277dc8(0xd, ..)`), names the chosen world on a second
  plate (`FUN_0029d820(1, ..)`) and opens page 0xe at its top; a cancel goes to page 0xc. Coming from page 0xc the cursor starts on the first row
  (`FUN_002a8948` sets the flag that picks the journal's own world, and world 0xf has no row).
- **Pages 0xe, 0x11, 0x14** (`FUN_002a91a0`, `FUN_002aaae0`, handler `FUN_002a9590`; `FUN_002ac370(0x11 / 0x12)` selects the Heartless and Nobody family).
  `FUN_002aa528` lists the records of the family that exist (or all with the all-worlds flag: the unknown ones show question marks and no tag,
  `FUN_0029bb48`), each name from the record's `+6` (flag 0x400: the world's name is the message's variable). With no entry the single row reads the
  message 0x36b3, the hand is hidden and nothing can be chosen. The redraw (`FUN_002aa848`) also marks the entry under the cursor seen
  (`FUN_002ac4c8`: `FUN_002b73b8`, and its tag goes to the seen group at once). A confirm opens the character's page: 0xf from 0xe, 0x12 from 0x11,
  0x15 from 0x14 (`FUN_002aa358` keeps the list; `DAT_0036d49c = 0xff` makes the cancel return to the list page with the cursor on the character
  shown, L2 / R2 `FUN_002aa600` moving it). A cancel goes to 0xd from 0xe and to 0xc from the others. The widget (`FUN_002977a8` -> `FUN_002978f8`
  / `FUN_00297af0`, page steps `FUN_00298370` with `FUN_00298fe0(list, 1)`, `FUN_00299010(list, 10)`): a step inside the ten rows moves the
  cursor, at the edge the view scrolls by one, past the ends a new press wraps [inferred: the wrap byte of these lists, as the Album's]; L1 / R1 move the
  view by ten, to the end then to the last row, `enemies::step` (tested). The help lines are 0x48ee, 0x48ef, 0x48f0, 0x48f1 (`FUN_002ac450`).
- **An enemy's page (0x12, 0x15)** is the character page 0xf (`FUN_002a98f0`, `FUN_002aadf0`) plus the counter row and the reaction rows. The
  picture is group 1 then 2 whatever the family (`FUN_002ab8f0`; the links pages used 4 / 5 before this work package: fixed). *Counter row* (records with flag
  0x800, group 0x4c): the label is the message 0x489b ("Slain"), the value a child at the group's x / y (136, 0), right aligned, font size 15 (group `+0x18`),
  the group `FUN_00278418(2)` = 0x7f's colour, the text `FUN_001918a0(buf, 0x382de0, count)` with the format `%d`. *Reaction rows* (pages 0x12 and
  0x15 only): `FUN_001d0258(NeoStatus)` binary-searches the `rcct` table (five command ids per enemy); each command with a row in the command table
  (`FUN_001b6f48`) is listed when its use count is not 0 (`FUN_001b7168`: the u16 array at `0x3410b0` indexed by the command's `ReactionCount`, capped
  at 999) or with the all-worlds flag; group 0x4d, one pitch apart (the pitch is negative, -27: the rows stack upwards), the name from the command's
  `MessageId` (question marks while the count is 0) and the count in the same child layout. [inferred: a command row of `CmdAction` 2 whose id is not 2 is
  followed to the party member's own command by `FUN_001a4dd0` in `FUN_001b6f48`; the port lists the row as found.] Port: `enemies::reaction_rows`
  (tested), `menu_journal::enemy_rows`. The counts live in `reaction_cmd::ReactionUses`; an accepted reaction also writes the saved copy (`reaction_cmd::bump_use` ->
  `Progress::set_reaction_use`, slot key `reaction_uses`, pairs `[slot, count]`; WP-HK1) and the journal shows the larger of the two. Run (looked at): `local/x/state-e.json` = the character
  state's flags (docs 20.10) plus 2256 and `"enemy_kills": [[1000,12],[1001,3],[1002,7],[1034,1],[1013,2],[3000,4],[3003,9]]`,
  `"reaction_uses": [[2,5],[41,12]]`, `KH2_KEYS=7:Enter,8:Down,8.2:Down,8.4:Down,8.6:Down,9.2:J,10:Escape,11:Left,12:J,13:Down,14:J,15:Down,15.3:Down,16:J`
  (the Heartless list, Large Body's page: "Slain 7", "Guard 12", "Kickback 5"). Log lines `journal: page 0x11 (...)`, `journal: the counter row (group 0x4c,
  FUN_002a1f78): 7 defeated`, `journal: enemy page: NeoStatus 1002, 2 reaction rows [...]`. The 3D model and page 0x10 / 0x13 / 0x16 stay out.
### 20.11a The counter writers (WP-HK1)
- **Limit best counts.** `FUN_001b32d8` (and its twin `FUN_001b3178`), the limit task's end (`FUN_001b2850`, `FUN_001b3798`, `FUN_001b35e8`), calls `FUN_001d25b0(*task, task[0x28])`: the
  `limt` `Id` byte (`*task`, the constructor `FUN_001b2c10` stores the record pointer) indexes the u16 array `0x3414b0`, which rises to the word at task `+0xa0`. That word is
  `+0x38` of the gauge object at task `+0x68`: `FUN_001b2e08` sets the gauge's range 0 .. 999 (`FUN_001cf8a0`) and raises the marker to the stored best (`FUN_001d00c0`
  with `FUN_001d2590`); `FUN_001cffa8` adds to the gauge's value (`+0x2c`) and raises the marker to it (the only writers of `+0x38` besides the resets `FUN_001cfdc0` /
  `FUN_001cfe28`, found with `mips_scan.py --store-off 0x38`). [inferred] The value is the limit's hit count: `FUN_001cffa8` has no direct caller (virtual). Port:
  `limit::LimitHits` (`start`, `add` = `FUN_001cffa8`, `finish` = `FUN_001d25b0` through `Progress::raise_limit_best`), called from `limit::try_limit` when the duration ends.
  Gap: the limit task body is not ported, so nothing adds hits during a limit; `KH2_LIMIT_HITS=<n>` adds `n` at the start (a debug aid). Test `limit::tests::the_end_of_a_limit_raises_the_slots_best`.
- **Reaction use counts.** `reaction_cmd::accept` calls `bump_use` (live `ReactionUses` and `Progress::set_reaction_use`); test `reaction_cmd::tests::a_use_is_counted_live_and_saved`.
- **Defeats.** `FUN_00184a30` runs on each entry into the defeat mode (`FUN_001acfe0`, the only direct caller; others are virtual). `kills::count_generic` counts an object whose HP is 0 once and
  re-arms it when its HP is above 0 again [inferred: a revive re-enters the defeat mode]; the app has no reviving enemy yet. Tests `kills::tests::the_system_counts_a_defeat_once_and_only_with_the_flag`, `a_revived_enemy_counts_each_defeat`.
- **Page 0xc's marks** (`FUN_002a7558`): row 1 any new bit in the families 0 .. 0xe and 0x10, complete when `FUN_002b9d88` finds every record of those
  families existing; rows 2 and 3 the families 0x11 and 0x12 the same way; row 0 the reports (`FUN_002b7668(-1)`, complete `FUN_002b9f10`); row 8 the
  limits; row 9 the moogle (`FUN_002b8fd0(-1)`, `FUN_002b9058(-1)`, `FUN_002bab48(-1)`).
- **Ansem reports (pages 0x17, 0x18)**. `jiminy.bar` entry 4 ("JMAN", 13 records of 8 bytes: +0 the item, +2 the title message, +4 the text message). Page
  0x17 (`FUN_002ac900`, handler `FUN_002accb8`, rows `FUN_002ad760`, redraw `FUN_002adc18`): the reports whose item is owned (`FUN_002a1ff8`) or all with
  the all-worlds flag (question marks), the new tag from the bits at `+0x120` (words 0x382edc / 0x382ee0), the cursor's entry marked seen
  (`FUN_002ade80` -> `FUN_002b77c0`), help 0x48ec. Page 0x18 (`FUN_002acfb8`, handler `FUN_002ad3d0`, text `FUN_002adcb0`): layers 0x5f and 0x62
  (the scroll bar's track), the title centred in group 0x60 (font 3), the text nine lines at a time (the list count is the lines minus eight when
  there are more than nine), Up / Down one line, L1 / R1 nine [inferred: clamped], L2 / R2 (-11 / -10, `FUN_002ad998`, when more than one is owned) the
  previous / next owned report, a cancel back to 0x17 with the cursor on it, help 0x48ed. Page 0xc lists row 0 when a report is owned
  (`FUN_002ade10`). Port: `name_list::report_rows`, `report_page`. Run: the state above plus `"items": {"226":1,"227":1,"228":1,"233":1}`,
  `12:J,13:J,14:Down,14.5:J,16:E` (the second report, "Secret Ansem Report 2", scrolled a page).
- **Limits (pages 0x1d, 0x1e)**. `jiminy.bar` entry 6 (21 records of 8 bytes: +0 the limit's command id, +2 the name, +4 the text). The use count of a
  limit (`FUN_002a20e0`: above 0 = used) is `u16[0x3414b0 + 2 * slot]` where `slot` is byte 0 of the `limt` row whose `Command` is the record's id
  (`FUN_001d2518`, `FUN_001d2590`; the table has 21 slots, some commands twice). The writer is `FUN_001d25b0(row, value)` from the limit's end
  (`FUN_001b32d8`, `FUN_001b3178`): the slot is raised to `value = limit object +0xa0` when it is below. The meaning of that field is not traced
  [GAP: the page prints it with the unit message 0x4b06 ("Hits"), so it is probably the hit count]. The app has no writer yet (outside this work
  package's files, see the report): `Progress::{limit_best, raise_limit_best}` (slot key `limit_best`, pairs `[slot, count]`) is filled by a state file.
  Page 0x1d (`FUN_002b2130`, rows `FUN_002b30d8`, handler `FUN_002b25a8`): layers 0x6c and 0x6d, the used limits (or all: question marks and a count
  of 0), each with a right-aligned child at the layer group's x / y that reads the count (`FUN_001918a0(buf, 0x382f68, n)`, format `%d`) and the
  unit message 0x4b06, tags 0x39 / 0x3a (the words at 0x382f98), help 0x48f8. Page 0x1e (`FUN_002b2878`, handler `FUN_002b2d78`) is the report page
  with the title group 0x61 (left aligned: no alignment is set) and the count right-aligned at that group's x / y, help 0x4b00. Page 0xc lists row 8
  when some limit is used (`FUN_002b37d8`); the tab reads "Combo Attacks". Run: `"limit_best": [[5,14],[4,22],[13,3]]`, rows 6 of page 0xc, `14:J`.
- **The moogle page (0x1f)**. `FUN_002b3d80` (layer 99), rows `FUN_002b4170`, handler `FUN_002b3f10`, help 0x48f9. The synthesis record (`Moogle`):
  row 0 "Moogle level N" with the level's name (`level + 1` through `%d`; names at 0x382fc0, messages 0xbcd2 ..) at the right; rows 1 .. 3 the
  materials known (of 50), the collection rewards paid (of the 50 `cond` rows) and the creations made (bits 0 .. 44 of `made`, of the recipes' entries:
  one each, two with a higher rank), as `%d` or, with the all-worlds flag, `%d/%d` (0x382fa0, 0x382fa8). Rows 0 and 3 are listed when the moogle is
  available (unlock bit 0 or 10, `FUN_002b4798`), rows 1 and 2 when some material was handed over (`FUN_002b47c8`); with the all-worlds flag an unlisted
  row reads the message 0x36f8 (question marks). A row is complete (second group 0x69 instead of 0x68, `FUN_002ba978`) at level 8, 50 materials, 50
  rewards, 45 creations. The new bits of the listed rows are made at the scene's open (`FUN_002b8f18`) and every row is marked seen when the page is
  built (`FUN_002b9138`), so a row that was new shows the seen tag 0x3a. Page 0xc lists row 9 when `FUN_002b4838` holds (available, or a material, or
  the all-worlds flag). Port: `moogle_page`; the totals come from `mixdata.bar` (`FUN_002b48c8`). Run: `"moogle": {"unlocks":[0],"level":3,
  "known":[0..11],"rewards":[0,1,2],"made":[0,1,2,3,4]}` ("Moogle level 4 Skilled Moogle", 12, 3, 5), `12:J`, seven Downs, `J`.
- **World 0xf's mark 0x9a (item D).** Page 0's cell builder `FUN_002a2248` picks the mark: for every record below 0x10 whose cell exists
  it calls `FUN_002baf08(world)` (its only caller) and draws the literal group 0x9a on the cell's first child when it answers yes. World 0xf
  asks (`FUN_002baf08`, `menu_journal::journal_complete`): every character record exists (`FUN_002b9d88(-1)`), the 13 reports are owned (`FUN_002b9f10`), every
  limit is used (`FUN_002ba238`: the completion bits at `+0x74`), every quest, minigame, treasure and map of every world is complete, the moogle's
  four rows are complete (`FUN_002bab48(-1)`: the bits at `+0x124` set by `FUN_002ba978`) and the links are complete (`FUN_002bad98(1, 0xf)`).
- **Pages 0x19, 0x1a and 0x20 are not Roxas's.** They are page 0xc's own numbers for the Treasures, Maps and Character Links builders (`FUN_002afca0`,
  `FUN_002afe98`, `FUN_002ae3e8` / `FUN_002ae790`); the only places that read the page number are `FUN_002b1e18` (the scroll starts at 0 for 0x19 / 0x1a),
  `FUN_002b16a0` (the treasure test: page 7 or 0x19) and the cancel target (page 0xc for 0x19, 0x1a, 0x20, else page 2). The MN14 port already runs them as
  7, 8 and 0xb with `from_c` set, so nothing further is needed [V by reading the three handlers and builders].
- **Gaps (WP-MN15).** The writer of the limit counters (the limit object's `+0xa0` and where the app ends a limit); the party members' defeat counters
  (ids 0x404 / 0x405, `FUN_001c46a8`) have no reader here; the hook that calls `Progress::count_defeat` is a system in `kills.rs` reading the
  `Enemy` components, not the defeat method; the reaction use counts are not written by `reaction_cmd.rs`; the 3D model and the model pages (0x10,
  0x13, 0x16) of a character or an enemy; the page-turn sound and wipe (WP-JR1: `FUN_002779f8` has no sound, see above); the held-pad auto-repeat of the list cursors; the exact right edge of the
  right-aligned values (WP-JR1 moved the text off the limit row's tag).
- **Journal polish (WP-JR1).** (1) A limit row's NEW tag (`FUN_002b2130`: the tag object is made with group 2, hung from the value object and never
  moved; `FUN_002b30d8` only switches its group to 0x39 / 0x3a) is centred on the value's origin, which is the right end of the right-aligned text,
  so it covered the last 23 units of "Hits" [V: the quads of group 0x39 span x -23 .. 23]. The original's right edge of the text is not recoverable from
  the code [GAP]; the port ends the text at the tag's left edge (`name_list::tag_left_reach`, test `a_limit_value_ends_where_its_tag_begins`) [inferred]:
  the tag cannot sit right of the text because the scroll bar starts 12 units after its right edge. (2) The left sheet of every journal page that has one (worlds 0xd, characters, story, album, treasures, quests, minigames, the character's own page 0xf; group 88 / 94 / 99 / 81 / 73 ..., box -148 .. 501, or .. 383 on page 0xf) is carried to the window's left edge by `stretch_quads` like the frame's rail (`menu_journal::sheet_stretch`, run for every journal node after each page build by `carry_sheets`, test `the_sheet_box_is_carried_whole`) [inferred]; the rule is a quad set starting left of x -100 and at least 400 wide (the book's base starts at -80 and stays). Looked at 2560x1080 and 1280x720: pages 0xd, 3 (story), 6 (characters), 4 (album), 7 (treasures), 9 / 10 (quests, minigames), 0xf at 2560x1080 reach the window edge, 1280x720 unchanged. Pages 0, 1 / 2 and 0xc have no left sheet (the dark book), so nothing is open. (3) The page-turn sound and wipe: `FUN_002779f8(dir, mode)` only switches the header object's group (table 0x381fa8 / 0x381fb0,
  `FUN_00138e78` or `FUN_0029b600` with a group, then writes +0x17c); it plays no sound. Nothing was ported: the sound the handlers play on a direction (if any) is
  not in that function [GAP: callers' sound calls not traced], and the header's second / third group animation is not in the port.
- **Decompile index (MN15).** `FUN_00184a30`, `FUN_001acfe0`, `FUN_001b02f0`, `FUN_001c46a8`, `FUN_00187dc8`, `FUN_00187e08`, `FUN_00187e68`,
  `FUN_00180850`, `FUN_001802e0`, `FUN_001699a8`, `FUN_0016e208`, `FUN_002a1f78`, `FUN_002a1c30`, `FUN_002a1c10`, `FUN_002a1fc0`, `FUN_002a1ff8`,
  `FUN_002a20e0`, `FUN_002a8958`, `FUN_002a8a38`, `FUN_002a8e90`, `FUN_002a8948`, `FUN_002a8568`, `FUN_002a84d8`, `FUN_002a91a0`, `FUN_002a9590`,
  `FUN_002aa430`, `FUN_002aa4b0`, `FUN_002aa528`, `FUN_002aa600`, `FUN_002aa848`, `FUN_002aa8d0`, `FUN_002aaae0`, `FUN_002ab8f0`, `FUN_002ac370`,
  `FUN_002ac450`, `FUN_002ac4c8`, `FUN_002aadf0`, `FUN_001d0258`, `FUN_001b6f48`, `FUN_001b7168`, `FUN_001a4dd0`, `FUN_00278418`, `FUN_001918a0`,
  `FUN_0029bb48`, `FUN_0029d1d0`, `FUN_0029d820`, `FUN_0029d3e8`, `FUN_0029cf78`, `FUN_00297148`, `FUN_002977a8`, `FUN_002978f8`, `FUN_00297af0`,
  `FUN_00298370`, `FUN_00296fb8`, `FUN_00298d58`, `FUN_00298fe0`, `FUN_00299010`, `FUN_002ac900`, `FUN_002accb8`, `FUN_002acfb8`, `FUN_002ad3d0`,
  `FUN_002ad6a0`, `FUN_002ad760`, `FUN_002ad998`, `FUN_002adba0`, `FUN_002adc18`, `FUN_002adc78`, `FUN_002adcb0`, `FUN_002ade10`, `FUN_002ade50`,
  `FUN_002ade80`, `FUN_002b7668`, `FUN_002b76e8`, `FUN_002b77c0`, `FUN_002b2130`, `FUN_002b25a8`, `FUN_002b2878`, `FUN_002b3018`, `FUN_002b30d8`,
  `FUN_002b3640`, `FUN_002b37d8`, `FUN_002b3878`, `FUN_002b38a8`, `FUN_001d2518`, `FUN_001d2590`, `FUN_001d25b0`, `FUN_001b32d8`, `FUN_002b3d80`,
  `FUN_002b3f10`, `FUN_002b3bc0`, `FUN_002b4170`, `FUN_002b4778`, `FUN_002b48c8`, `FUN_002b4798`, `FUN_002b47c8`, `FUN_002b4838`, `FUN_002b8f18`,
  `FUN_002b8fd0`, `FUN_002b9058`, `FUN_002b9138`, `FUN_002b99d0`, `FUN_002b9b28`, `FUN_002b9bf0`, `FUN_002b9d88`, `FUN_002b9f10`, `FUN_002ba238`,
  `FUN_002ba978`, `FUN_002babb8`, `FUN_002bab48`, `FUN_002baf08`, `FUN_002a2248`, `FUN_002b1e18`, `FUN_002b16a0`, `FUN_002afe98`, `FUN_00237658`,
  `FUN_00237678`, `FUN_00237698`; data 0x382de0, 0x382e90, 0x382edc, 0x382f68, 0x382f98, 0x382fa0 .. 0x382fc4, 0x3410b0, 0x3414b0, 0x36d3f8.
- **Tests (WP-MN15).** `enemies::tests::*` (the widget's moves, the world rows, the reaction rows), `kills::tests::*`, `chars::tests::{an_enemy_record_exists_once_the_enemy_was_defeated,
  a_family_list_has_the_records_that_exist_or_all_with_the_all_worlds_flag}`, `name_list::tests::*`, `report_page::tests::*`, `moogle_page::tests::*`,
  `marks::tests::every_category_keeps_its_saved_number`, `menu_journal::tests::{the_disc_enemy_list_data, formats_are_written_with_the_fonts_glyphs}`,
  `progress::tests::{enemy_defeat_counters_count_and_are_saved, reaction_use_counts_are_saved, limit_best_counts_only_go_up_and_are_saved}`,
  `root_page::tests::*`.

### 21.1 The table [V]

`DAT_01d5df08` (the "pause-menu records" of `FUN_001bef10` / `FUN_001bef48` / `FUN_001befc0`) points at entry `slct`
(tag at 0x37c558) of `14mission.bar`, set by `FUN_001d3408` through `FUN_001beed0`. The entry: +4 a u32 row count
(23 on the disc), rows of 0x40 bytes from +8, sorted by the u16 key at +0; `FUN_001bef10(key)` is the binary search
`FUN_0030ff40` (compare `0x333250`, no `.c`; the sorted keys fit a u16 compare [inferred]). A row is the request
block of menu kind 7 (section 18.1): +2 option count, +3 the first cursor row (`FUN_002544b8` -> `FUN_0026f480`),
+4 + 4i (u16 action code, u16 message) for up to four options, +0x14 kind 7's header group (`FUN_00254800`, -1 none),
+0x16 kind 8's header group (`FUN_0026f8a8(0, ..)`), +0x18 the information message, +0x25 bit 0: the list's flags
0x60 are cleared (`FUN_0026f508` -> `FUN_00298cf0`), so `FUN_00297148` never answers -2 (cancel, pad bit 8) or -4
(pad bit 0x100), and `FUN_0024f330` clears word +0xfc of `FUN_0014a918`'s object. Test
`menu_struggle::tests::the_disc_result_menu_row_texts_and_groups` checks the order and row 0x1a.

### 21.2 Row 0x1a: the Struggle's result menu [V]

Opened by the mission end's last step (`FUN_001b96e0`, docs/struggle.md "The end"). Two rows: code 4 "Quit" (0x8ab2),
code 3 "Retry" (0x8ab1); `FUN_0026fda0` sends both to `FUN_001b9c58` (events 0x8d and 0x8b, the manager ends, the
trigger's jump). No help line is set by kind 8 for it [GAP: where the Save menu's help 0x3858 comes from was not read].

### 21.3 What the port does

`crates/kh2-app/src/menu_struggle.rs` (`StruggleMenuPlugin`, the fourth plugin tuple of `main.rs`): reads the table at
the first request, dims the field (`DIM_BRIGHTNESS`), sets `Pause::menu`, draws the rows as the Save menu's pointer list
(`save.2ld` sequence 0 groups 77 selected, 79 idle, hand 12, placements of `menu_save.rs`) [inferred: kind 8's
resources 0x753c / 0x7532 were not resolved to a file], Up / Down move and wrap [inferred], J confirms, the cancel is
ignored for row 0x1a, the close fade is the camp's (`CLOSE_FRAMES`). Logs: `struggle menu: open record 0x1a ...`,
`struggle menu: cursor N`, `struggle menu: row N chosen, code C (FUN_0026fda0)`, `struggle menu: closed: back to the
field (FUN_001038a0)`, `struggle menu: event 0x8d: trigger jump type 1 to world 2 area 4 entrance 55`. Not drawn: kind
7's frame, the header group +0x16, the menu sounds' set-up `FUN_0023e860` (the sounds go through `menu_sound`, muted).
Code 0xc's picture page and the other rows' codes (1, 5, 6, 10, 0x11) are not reached by row 0x1a; the port takes
only codes 3 and 4 to the manager (`Mission::menu_exit`).
WP-PRG1: a record without +0x25 bit 0 can be cancelled; `FUN_0026fda0` only closes the page for the cancel (page -2 ->
`FUN_0023e8b0`) and never reaches `FUN_001b9c58`, so the original leaves the manager running. The port ends it
(`menu_exit(0)`, no jump) because with the menu gone nothing could [inferred, a port choice against a soft lock]. A code
chosen while a jump task runs is queued and applied when the task is idle [inferred]. Tests
`a_cancel_ends_the_manager_and_a_choice_during_a_jump_is_queued`.

## 22. Shops: menu kind 4 (WP-SH1)

### 22.1 Who opens a shop [V]

Command 30 is **"Examine"** (cmd row 30: Execute 0x1e, message 0x81eb), not a shop. The shopkeepers offer the **"Shop"**
rows (cmd rows 195..212: message 0x84c7, Execute 0x7f; also 213, 453 and 622 with Execute 0x7f and a shop event). Their
Arguments (cmd +4) are the shop events: 195 0x66, 196 0x74, 197..200 0x6c..0x6f, 201..204 0x70..0x73, 205 0x75, 206..208
0x83..0x85, 209..212 0x67..0x6a, 213 0x6b, 453 0x13, 622 0x8a (`menu_shop::tests::the_disc_shop_file_rows_and_shopkeepers`).
Twilight Town's shopkeepers (townspeople, objentry type 15): tt06 `k01_` `N_EX650_TT_ITEM_RTN` (212, event 0x6a, "Wantz's
Item Shop"), tt06 `z70_` `N_EX700_TT_WEAPON_RTN` (209, 0x67), tt07 `170_` `N_EX640_SHOP_RTN` (213, 0x6b), `q70_`
`N_EX680_TT_PROTECT_RTN` (210, 0x68), `t70_` `N_EX690_TT_ACCE_RTN` (211, 0x69); their groups are loaded by tt06 evt
programs 0xe, 0x10..0x14 and tt07 evt programs 0x10, 0x12, 0x16.

- Offer: the availability switch `FUN_001ba2c0` treats Execute 0x7f as it treats 0x37 (the save point): status 5 while
  `FUN_001c6c08(0x35fec0, 0)` answers non-zero; then the row's flags and form mask (`FUN_001b70b8`, `FUN_001b7290`).
- Accept: Sora's class handler gives Execute 0x7f to `FUN_00177b18`, case 0x7f: `FUN_00197b70(halfword +6 of the pending
  command record at actor +0xb44, 0)`. [inferred: that halfword is the row's Argument; its writer was not traced. Every
  row with Execute 0x7f whose Argument is a shop event has a record in the shop file.]
- `FUN_00197b70` sends 0x13, 0x66..0x75, 0x83..0x85, 0x8a to `FUN_0024f0b8(4, event)`: menu kind 4, mode word = event.
- Examine (`FUN_001a54a8`): `FUN_001bef48(0xc)`, `FUN_001b7ce8(0)`, `FUN_001a53f8(target +0x500, player, record, task
  FUN_001a5510, callback FUN_001a5608)`. The task turns the player to the target, runs the class-5 script's
  "before_check" mode around the program (`FUN_00189eb8`, `FUN_00189f28` = `FUN_00189ff0(0)`), and unlocks unless an event
  runs. The callback's op 1 opens window class `FUN_001ab7d8` (`FUN_001aaf20(win, 3, 4, message, 0, 0x100, 0x20, 3)`:
  class 2 of `kh2_image::window::CLASSES`, at the top of the screen) and waits for state 3 or `FUN_001a5b18(0xe)` = 0.
  On the disc the Examine targets are mostly `F_EX000` gimmicks (objentry type 8, 262 entities over all worlds) and one
  townsperson (tt03 `a61_` `B_EX110_RTN`); every program is one message, none fires a shop event.
- Text command 0x12 (`FUN_001a8978`, a page's layout) also calls `FUN_00197b70(u16, 0)`; no shop message uses it.

### 22.2 The shop file [V]

`FUN_00193610` keeps `03system.bin`'s type-0x29 entry ("shop") through `FUN_00265ac0` (`DAT_0036cfe4`). `FUN_00265660`:
magic `TZSH` (u32 0x48535a54), u16 version 7 at +4. +6 i16 record count (21 on the disc), +0xc u32 offset of a list of up
to 0x80 item ids (`FUN_00265ad0`: an item's place in it is its bit in the save's array at +0x3714, the shop's "new" marks,
`FUN_00265280`), +0x10 the records, 0x18 bytes each: +0 event, +2 a record bit (array 0x33fdb0), +4 the header message
(the shop's name), +6..+0xc four words copied to the state [GAP: their readers], +0xe flags, +0x10 group count, +0x14 u32
group offset. A group is 8 bytes: i16 record bit (negative: always on), u16 count, u32 offset of `count` item ids.

`FUN_00265688(event, state)` (the state is 0x820 bytes at `DAT_0036cfbc`): the record whose +0 is the event; its record
bit is set when clear (`FUN_001a52b0`); state +0 = message, +2..+8 the words, +0xc bit 0 = flag 0x100, bit 1 = flag 0x200,
+0xd = (flag 0x80 clear and the first group has items). Each group that is on adds its items with the kind mask 0x1f;
with flag 0x80 the always-on groups of every other record whose record bit is set and whose own flag 0x80 is clear add
theirs, with the record's flags word as the mask. An item is added once (`FUN_00265970`) and only when its kind bit
(`FUN_00265a50`: item type 0, 1 -> 8; 2..0xd -> 1; 0xe -> 2; 0xf -> 4; 0x10 -> 0x10) is in the mask. Twilight Town's
weapon, armor, accessory and item shops have flags 0x181, 0x182, 0x184, 0x188 (one kind each, shared); the moogle
records 0x31f (every kind, the third row, gated groups 0x49..0x60 of synthesis materials).

### 22.3 The pages [V]

Init `FUN_0025f168`, task `FUN_0025fc18`: loads `itemshop.2ld` (`FUN_0024f738(4)`), builds the screen (`FUN_0025fcd8`,
`FUN_0025fef0`), opens with sound kind 0x10 when state +0xc bit 1 is set, else 0xb, and runs `FUN_00260540` each frame.
Layout triples (enter, leave, hold) at 0x381240: (0xf, 0x11, 0x10), (4, 6, 5), (0xc, 0xe, 0xd) (`FUN_00263f00`).

- **Page 0** (`FUN_00260748`): four rows from the table 0x381300 (0x24 bytes, the camp entry layout): Buy 0x8acc, Sell
  0x8acd, Synthesize 0xbbb5, Quit 0x8ace. `FUN_00264da8`: Buy and Sell need state +0xc bit 0 and a non-empty list, the
  third row bit 1, Quit always. Confirm: Buy -> page 1, Sell -> page 4, the third row -> page 8 (`FUN_00284c10` and the
  synthesis pages), Quit or a cancel -> the end. -4 (Start) ends it from any page.
- **Pages 1 / 4** (`FUN_00260ec8`): the buy list (state +0xa count, +0x10 ids) or the sell list (`FUN_00264be8`: every
  item row in table order with a type below 0x12, a count, and a sell price; +0x41a count, +0x420 ids, +0x620 counts).
  `FUN_00265550` refuses a row (the refused sound): buying when the player has more than 0x62 or the price (item +0xc)
  is above the munny (save +0x1600, `FUN_0023e5c0`); selling when the sell price (item +0xe) is 0. A confirm writes the
  selection (`DAT_0036cfc4`: item +2, count +4 = 1, price +6, total +8) and goes to page 2 / 5; a cancel to page 0.
- **Pages 2 / 5** (`FUN_002614e0`, `FUN_00261a28`): the pad bits 0x10, 0x20, 0x40, 0x80 (Up, Down, Left, Right) step the
  count by +1, -1, -10, +10. Buying, owned + count stops at 99 and a step the munny does not cover is cut to what it
  covers (none: refused); selling stops at the owned count; the count stays at least 1. Confirm -> page 3 / 6, cancel ->
  page 1 / 4.
- **Pages 3 / 6** (`FUN_00261d70`, rows by `FUN_00261bf8`): row 0 "No" (0x8ad0), row 1 "Yes" (0x8acf), the cursor on row 0.
  Row 0 or a cancel: sound kind 8, back to page 2 / 5. Row 1: sound kind 0xf; buying `FUN_0023e6b0(total)` (munny down)
  and `FUN_001a1a18(item, count)`; selling `FUN_0023e620(total)` and `FUN_001a1c30(item, count)` (`FUN_00283480` when the
  item is gone); then the munny digits, the lists again (`FUN_00264be8`) and page 1 / 4.
- **Page 7** (`FUN_00262140`, `FUN_002621a0`): when `FUN_00265498` (flags not 0x200 alone, and the save's byte +0x3724 bit
  `1 << (state +0xd != 0)` clear) the shop opens on a text (0x381128 or 0x38112c) and sets that bit [ported in WP-SH3:
  section 24.1].
- The end (`FUN_00260540` once closing): waits for the objects, `FUN_00284ae0`, `FUN_00262de0`, `FUN_001038a0` (back to
  the field).

### 22.4 What the port does (`crates/kh2-app/src/menu_shop.rs`)

`reaction_cmd::status` offers Execute 0x7f rows (blocked in a drive form, the part of 0x35fec0 the app has [approx]);
`reaction_cmd::accept` turns an accepted Shop row into `ShopRequest(event)`. `ShopPlugin` reads the shop file, the item
table and `itemshop.2ld` on the first request, builds the stock (`menu_shop::stock`, the record bit kept in
`Progress::record_bit`), dims the field and freezes it (`Pause::menu`), and runs pages 0..6 with the rules above
(`menu_shop::Flow`, `step_quantity`, `row_open`, `sell_list`, `deal`). The frame is the layout triple's enter layout
holding on its hold layout; the rows, names, prices, counts, the detail line, the question and the munny are text labels
the port places [inferred positions, not read from the list widget]. A close plays the leave layout and the camp's
close dim. Examine (command 30) runs the talk program in window class 2 (`npc.rs`).

Not ported (the first-visit text and the "new" marks came in WP-SH3, section 24): the list widget's own look (row plates, the pointer, scrolling), the help line, the munny plate's digits art, the open
sound's exact entry [inferred: kind k is the u16 at 0x37f654 + 2 k, the rule of kinds 0xc and 0xd].

Harness: `KH2_PROGRAMS=0,0,10 KH2_NPC_WATCH=2 KH2_NPC_WATCH_DIST=60 KH2_STATE=<{"munny": 500}> KH2_REACTION_PRESS=8
KH2_KEYS=10:J,10.6:J,11.2:Up,11.8:J,12.4:Down,13:J kh2-game tt06` buys two Potions at Wantz's. Log lines: `reaction:
accepted command 212 (Execute 0x7f, FUN_00177b18): event 0x6a`, `shop: open menu kind 4`, `shop: page 1 -> page 2`,
`shop: bought item 0x1 x2 for 80`, `shop: close`, `shop: menu kind 4 (0x6a) closed`.

### 22.5 Decompile index (WP-SH1)

| address | what |
|---|---|
| 0x177b18, 0x1778e8, 0x1ba2c0 | the Execute switch (case 0x7f), the pending command, the availability switch (case 0x7f) |
| 0x197b70, 0x24f0b8 | events to menu kind 4 |
| 0x1a54a8, 0x1a5510, 0x1a5608, 0x1a53f8, 0x1a53b8, 0x1ab7d8, 0x1aaf20, 0x1aacb0, 0x189eb8, 0x189f28 | Examine |
| 0x1a8978 | text command 0x12 fires an event |
| 0x193610, 0x265ac0, 0x265650, 0x265660, 0x265688, 0x265970, 0x265a50, 0x265ad0, 0x265280 | the shop file and the stock |
| 0x25f168, 0x25fc18, 0x25fcd8, 0x25fef0, 0x264da8, 0x264e20, 0x265498 | init, task, screen, top rows |
| 0x260540, 0x260748, 0x260ec8, 0x2614e0, 0x261910, 0x261a28, 0x261bf8, 0x261d70, 0x262140, 0x2621a0, 0x263f00 | the pages |
| 0x264b98, 0x264be8, 0x265550, 0x255588, 0x1a6730, 0x23e5c0, 0x23e6b0 | rows, the sell list, the refusals, munny |
| data 0x381240, 0x381300, 0x37f654, 0x381128 | layouts, top rows, sound kinds, first-visit texts |

Gaps: the writer of the pending command's +6 (searched: `FUN_001778e8`, `FUN_0019c670`); the readers of the record's words
+6..+0xc; `FUN_001c6c08`'s bits 2 and 3 of 0x35fec0; `FUN_001a5b18(0xe)`; `FUN_0023e680`; the list widget's positions
(`FUN_00296b48`, `FUN_0029b600`, `FUN_00296d10`); page 7. The third row (pages 8 and after): section 23.

## 23. The shop's third row: synthesis (WP-SH2)

### 23.1 The data [V]

`FUN_00284798` (the third row's setup) allocates the page state (`DAT_0036d194`, 0x1130 bytes), the screen objects
(`DAT_0036d198`), the data holder (`DAT_0036d19c`) and the session marks (`DAT_0036d1a0`), and lists the item rows of type
0x10 in table order into the state's +0x314 (the material list, at most 0x32; 50 on the disc). `FUN_00291ae0` loads
`menu/us/mixdata.bar` (name at `PTR_s_mixdata_bar_0036d190`); `FUN_0029e630(holder, k)` answers entry k:

| k | tag | layout (after a 0x10-byte header: magic, version 3, u32 count) | readers |
|---|---|---|---|
| 0 | `reci` | 0x20 per recipe: +0 the recipe's item id, +2 kind (0 taught by its item, 1..3 a development set), +3 rank, +4 the item made, +6 the higher-rank item (0 none), +8 six (u16 material, i16 count) lines ending at a non-positive id | `FUN_002906e0`, `FUN_00290058`, `FUN_00290ef8`, `FUN_00290468` |
| 1 | `cond` | 0xc per collection reward (+0 text, +2 reward, +4 type, +5..+7 condition, +8 count, +0xa record bit) | `FUN_00291140` (ported in WP-SH3: 24.3) |
| 2 | `leve` | 0xc per level: +0 name, +2 text, +4 i16 unlock bit, +8 u32 points | `FUN_00290bb8`, `FUN_00290c28` |
| 3 | `exp` | +0x10..+0x16 the points of a material by its rank (item +5), +0x18 f32 the first bonus' factor, +0x1c a story flag, +0x1e the unlock bit it grants | `FUN_00290468`, `FUN_00284c10` |

On the disc (`menu_shop::tests::the_disc_mixdata_recipes_levels_and_tables`): 23 recipes (10 of kind 0), 50 rewards, 9
levels with rising thresholds, factor 2.0, the story flag's bit is 10. The executable's tables: 0x3825e0 the seven row
messages of page 9, 0x382690 / 0x382688 / 0x382680 the bonus materials by rank (`FUN_002910f0(rank, k)` for k 0, 1, 2),
0x382578 a byte per rank (the unlock bit that halves that rank on the made list), 0x382548 the detail rows' help,
0x382628 the opening notices, 0x3826a0 the level names, 0x3825b8 the layout triple (8, 10, 9) of these screens.

### 23.2 The record [V]

`FUN_002906c0` answers save +0x3728. +0/+4 a bit per material once handed over; +8 the reward bits; +0x10 two bits per
recipe (bit 2r learned); +0x18 one bit per list entry made (entry 2r the item, 2r + 1 the higher rank); +0x20 the same
once seen; +0x28 the unlock bits (`FUN_002903b8`): 0..2 the development sets, 3 the points and halving bonuses, 4 the
higher rank, 5 two bonuses at once, 6..9 the made list's halving per rank, 10 the recipe row; +0x30 the points (at most
99,999); +0x34 the stock and +0xfc the totals, a word per material slot (at most 999); +0x1c4 the level. The port keeps
it as `progress::Moogle` (`Progress::moogle`; a slot file carries it; `KH2_STATE` key `moogle`: the bit sets, `points`,
`level`, `stock`, `totals`).

### 23.3 The pages [V unless marked]

- **Opening** (`FUN_00284c10`, page 8): the `exp` story flag set grants its bit (10). `FUN_00288758` / `FUN_002887c8`:
  every material held goes to the stock and the totals (999 at most each) and leaves the inventory; its known bit is
  set. With bit 10, `FUN_002888f8` / `FUN_002889e0`: a kind-0 recipe whose item is held and that is not learned is
  learned and its item taken (a learned one's item stays; an item of the other kinds is taken). `FUN_00291140` checks
  the rewards [ported in WP-SH3: 24.3]. The first page is 10 after a hand-over, else 0x10 after recipes, else 9; the notices of
  the state's +0x130 bits come first (`FUN_00287950`, page 0x13: bit 0 the hand-over, bit 5 the recipes, 1..4 rewards).
- **Page 9** (`FUN_00284ea8`, `FUN_00285388`): seven rows; row 0 shows when something was made, row 1 with a set bit
  0..2, row 2 with bit 10, rows 3..6 always. Row k leads (`FUN_0028a6f0`): 0 -> 0xc, 1 -> 0xe, 2 -> 0x10, 3 -> 0x17,
  4 -> 10, 5 -> 0xb, 6 -> page 0 (the shop's top rows, `FUN_0025fef0`); a cancel -> page 0, -4 the end.
- **Pages 0xc / 0xe / 0x10** (`FUN_00290058`, `FUN_00288fc0`, `FUN_00286580`): the made list (every entry made), the
  development list (learned recipes of sets 1..3 whose bit is set, not made), the recipe list (learned kind-0 recipes not
  made); at most 0x2d entries, an empty list shows 0xbe22. An entry is dimmed and refused (`FUN_00291b58`) unless some
  way of making it is in stock. A confirm opens its detail page with rows (+0x106b): make and quit always, the points
  bonus and the halving bonus with bit 3, the higher rank (a recipe with +6) with bit 4 (`FUN_002908c0`); the cursor
  starts on make when the item can be made as it is, else row 2 (3 for a higher-rank entry) [inferred: the widget
  keeps a cursor past the last row on the last].
- **Need** (`FUN_002906e0`, `FUN_00290618`): a line needs `count / div` rounded up, at least 1; div is 1, 2 with the
  rank's bit (0x382578) on the made list, doubled by the halving bonus. The higher-rank bonus and the halving bonus
  each need one of their material (k 2, k 1); both at once need bit 5.
- **Detail pages 0xd / 0xf / 0x11** (`FUN_00286bc8`, jump table 0x38249c): row kinds 1..3 toggle the bonus flags
  +0x106a bits 0..2 when `FUN_00291b58` allows (a flag already set always; else fewer set than allowed, 1 or 2 with bit
  5, and `FUN_002908c0` with the stock check; on 0xf and 0x11 not the higher rank; the flag tested is the shown row's
  minus one). Make (0x286f40): a higher-rank entry without that bonus -> 0xbef4, no way in stock -> 0xbef5, the item
  at 99 -> 0xbf06 (page 0x12, a text then back); else page 0x14 (`FUN_0028ee28`: 0xbef6 and the item's name). Quit and
  a cancel go back to the list.
- **Pages 0x14..0x16**: closing the result window makes it (`FUN_00287fb0` -> `FUN_00288170`: `FUN_001a1a18(item, 1)`,
  the item being +4 for a plain entry without the higher-rank bonus, else +6). `FUN_00290ef8` / `FUN_002909d0` take
  each line's need and one of each chosen bonus material from the stock and add the points (`FUN_00290468`: each line's
  full count times its rank's points, times the factor with the points bonus) [the original counts them down a frame
  at a time; the port at once]. `FUN_00288228` marks the entry made and seen (2r + 1 when the bonus raised a plain
  entry), `FUN_00290c28` grants the bits of the levels reached and the development sets they open (`FUN_00290cc0`),
  `FUN_00290bb8` sets the level; each level gained shows 0xbed1 with its name (page 0x16), then the list again.
- **Pages 10 and 0x17** (`FUN_002858e8`, `FUN_00287240`, `FUN_0028f9a8`): the materials with a total (10, titled 0xbcdf)
  or with stock (0x17, 0xbd90); a cancel goes to page 9 (the rows, their NEW tags and the story flag's extra rows: 24.9).

### 23.4 What the port does (`menu_shop::synth`)

`ShopMenu` loads `mixdata.bar` and the tables with the shop; the third row's confirm runs `synth::Session::start` (the
opening) and the pages above (`Session::key`, `entries`, `entry_open`, `detail_rows`, `make_check`, `make`), drawn as
text labels over the synthesis layout triple like the shop pages [inferred positions; the detail page also lists each
line as name and need / stock, an inferred layout: `FUN_002894f0` and `FUN_00289aa8` were not read]. Harness:
`KH2_PROGRAMS=0,0,16 KH2_NPC_WATCH=5 KH2_NPC_WATCH_DIST=60 KH2_REACTION_PRESS=8 KH2_STATE=<{"munny": 500, "items":
{"382": 1, "345": 6, "329": 2, "317": 2, "333": 2}, "moogle": {"unlocks": [10]}}> KH2_KEYS=10:J,11:J,11.6:J,12.2:K,12.8:J,
13.4:J,14:J,14.6:J,15.2:K,15.8:K,16.4:K kh2-game tt07` makes a Mega-Potion at the moogle (`N_EX640_SHOP_RTN`, event
0x6b): `synth: open the third row`, `synth: made item 0x5`, `shop: close`.

Not ported (page 0xb, the rewards and the NEW marks came in WP-SH3, section 24): the
moogle's animation (`FUN_00283520`, item +0x14), the window and list widgets' own look, the frame-by-frame count-down.

### 23.5 Decompile index (WP-SH2)

| address | what |
|---|---|
| 0x260540 | the page switch, pages 8..0x17 |
| 0x284798, 0x284908, 0x291ae0, 0x291b38, 0x29e4a8, 0x29e630 | setup, teardown, `mixdata.bar`, entries |
| 0x284c10, 0x288758, 0x2887c8, 0x2888f8, 0x2889e0, 0x291140 | the opening |
| 0x284ea8, 0x285388, 0x28a6f0, 0x28a748 | page 9 |
| 0x286378, 0x286580, 0x288fc0, 0x290058, 0x291b58, 0x2906e0, 0x290618, 0x290400, 0x2910b0, 0x2910f0 | the lists, the need |
| 0x286bc8, 0x38249c (0x286f40 .. 0x287228), 0x2908c0, 0x290978, 0x289bf8, 0x290e68 | the detail page |
| 0x287780, 0x28e980, 0x287950, 0x2879e8, 0x2877a0 | texts, the opening notices |
| 0x287ee0, 0x28ee28, 0x287fb0, 0x288170, 0x288228, 0x290ef8, 0x2909d0, 0x290468, 0x290c28, 0x290bb8, 0x290cc0, 0x288480, 0x2884e8 | the make, points, levels |
| 0x2858e8, 0x285b30, 0x287240, 0x287498, 0x28ffc8, 0x28f9a8, 0x28aa40 | the material pages |
| 0x2906c0, 0x2903b8, 0x2783f8 | the record, the unlock bits, story flag 0x4837 |

Gaps: (page 0xb and the rewards' conditions are closed in section 24); the result window's key handling (`FUN_0028e7f0`, `FUN_0028f2c0`: the port makes the item on any confirm or cancel);
the list widget (`FUN_00296b48`, `FUN_00299878`); the entries `FUN_002783f8` (story flag 0x4837) adds with mark 0x80
are ported in 24.9; `FUN_002909d0` with a bonus material outside the list writes the stock at slot -1 (the points word) [not
ported].

## 24. Page 7, the shop's NEW marks, the collection rewards and page 0xb (WP-SH3)

### 24.1 Page 7: the first-visit text [V]

`FUN_0025fcd8` builds the screen, runs `FUN_0025fef0` (the top rows) and then asks `FUN_00265498`; true opens page 7 and
runs `FUN_00262140`. `FUN_00265498`: the state's halfword at +0xc (bit 0 the Buy and Sell rows, bit 1 the third row, byte
+0xd the "first group" flag of `FUN_00265688`) must not be exactly 2 under the mask 0xff02, i.e. the shop is not the
third row alone, and the save's byte at +0x3724 must have the bit `1 << (+0xd != 0)` clear. So there are two classes of
shop: those whose record has no flag 0x80 and a first group with items (the moogle's records, flags 0x31f; bit 1) and
the shared ones (flag 0x80: Twilight Town's weapon, armor, accessory and item shops, flags 0x181..0x188; bit 0). A
moogle without wares of its own (the third row alone: tt07's `N_EX640_SHOP_RTN`, event 0x6b) never shows a text.
The text is the message id held in a word of the executable: 0x381128 when +0xd is clear, 0x38112c when it is set
(0xc401 and 0xc402 on the disc, read at run time; both say that items of earlier shops reach the shops of the bigger
towns). `FUN_00262140` opens a message window through `FUN_00262350` (window object 0x7546 and a list widget 0x7549, the
text by `FUN_00190c40`). `FUN_002621a0` waits: only a list result of 0 (confirm) or -2 (cancel) closes it
(`FUN_00262450`); a Start press just plays its sound. When the window has gone (`FUN_002624a0`) it sets the bit
(`|= 1 << (+0xd != 0)` in the save byte) and returns to page 0.

### 24.2 The shop's NEW marks [V]

The shop file's list of up to 0x80 item ids (`FUN_00265ad0`: an item's place in it, -1 past the end) has one bit per
place in the save at +0x3714 (4 words). `FUN_0025fef0` runs `FUN_00265280` at every open: each item of the buy list
(state +0xa count, +0x10 ids) whose save bit is clear sets its bit in a per-visit set (`DAT_0036cfd0`, 0x14 bytes): the
items that are NEW this visit. `FUN_00265348(item)` sets the item's save bit when it is NEW this visit and the bit is
still clear (`FUN_002653d8`: NEW this visit; `FUN_00265420`: NEW this visit and its save bit is set now). It is called
for the item under the cursor when page 1 is entered (`FUN_00260a78`) and when the cursor moves (`FUN_00264668`,
`FUN_00260ec8`). The tags are sprite groups of `itemshop.2ld`: NEW = group 0x78 (word 0x381238), VIEWED = group 0x77
(word 0x38123c).
- Buy list rows (`FUN_002626e8`, page 1 only): an item that is NEW this visit has the NEW tag until its save bit is
  set, then the VIEWED tag.
- Top page (`FUN_00262bd0`): the Buy row (its display index `FUN_00264e80(0)`) has a tag when any item is NEW this visit:
  NEW while one of them is unseen, else VIEWED. The third row has a NEW tag (group 0x78, literal) when `FUN_002919e8`
  answers non-zero and the record has the third row: a recipe learned (record +0x10 bit) and not seen (+0x20), else a
  made entry (+0x18) not seen, over 0x2d bits.
`itemshop.2ld`'s sequence 2 (257 groups) holds the groups: 0x77 (last frame 9) and 0x78 (last frame 0x77) are one 40 x
12 rectangle ending at the origin (quad x -39..1, y 6..18) over (0, 128)-(40, 140) of the file's second image, which is the
atlas with the NEW! art (the first image is the frame panel). The row plate group 0x62 has the extent 0x10e (the camp's
tags sit at the plate extent: `FUN_00138ed0(tag, FUN_00237618(row + 0x18), ..)`).

### 24.3 The collection rewards [V]

`FUN_00284c10` runs `FUN_00291140` after the hand-over (so the totals are the new ones). It clears the page's list (state
+0x184, 8 bytes per entry: the reward index and a byte "reached"; +0x60 the count) and walks the 50 `cond` rows:
`FUN_00291418(a, b, c, count)` with the row's bytes +5, +6, +7 and halfword +8 decides "reached"; a row that is not
reached is listed only when story flag 0x4837 is set (`FUN_002783f8`). For a reached row whose bit in the record's
reward word (+8) is clear: the bit is set, the session's reward mark (`DAT_0036d1a0` +0x18) too, and
- kind (+4) 0: the item (+2) is added with `FUN_001a1a18` when `FUN_001a18b8(item, 100, 1)` allows (count below 99; a
  unique item not yet owned), notice bit 2 (value 2 of the state's +0x130);
- kind 1, reward 0: unlock bit 0 (record +0x28), notice 4, then `FUN_00290cc0` learns the Free Development recipes of set
  1; reward 1: unlock bit 1, notice 8, the same; any other reward n: unlock bit n, notice 0x10, the record bit of
  halfword +0xa (`FUN_001a52b0`) and the first material of the row's category and rank is marked in the state's +0x134
  (`FUN_00291628`; the marks' reader is the notice page, 24.9).
The notices are the opening's texts 1..4 of the table at 0x382628.
The test `FUN_00291418` walks the 50 material slots (state +0x314: the item ids of type 0x10 in table order) with the
record's totals (+0xfc, the amount of each ever handed over): the slot counts when `FUN_00291560(item +4, a)` and
`FUN_002915f8(item +5, b)` hold: a in 0xd, 0xe any category; below 0xd the category itself; 0x10 one of three special
categories (the bytes at 0x382698: 9, 10, 11 on the disc); any other value one that is not special. b 4 and 5 any
rank; below 4 the rank itself. A matching slot with a total of 0 fails the whole test when a is 0xd or b is 4 (the
"collect all" rows); otherwise the totals add (at most 99,999) and the matching slots with a total are counted; c 0
compares the sum with the count, else the number of slots. The disc's 50 rows read as their texts say (test
`the_disc_shop_layout_has_the_tag_groups_and_the_rewards_read_as_conditions` prints them): 0..10 "Get n types of
Materials" (a 0xe, b 5, c 1; n = 1, 5 .. 45, all 50 with a 0xd, b 4), 11..15 "Obtain total amount of n" (c 0), 16..21 "all
materials of a category" (a 6..0xb, b 4, count 4), 22..25 "all of a rank" (a 0xd, b 0..3, count 12 or 13), 26..49
"n or more of one category and rank" (c 0; reward kind 1 with the record bits 0x49..0x60, the gated groups of the moogle's
shop, section 22.2). Rewards 0, 2, 3 and so on are items (24 of the 50), 26 are unlocks.

### 24.4 Page 0xb, the reward list [V]

Row 5 of page 9 leads to page 0xb (`FUN_00285388`, `FUN_0028a6f0`). `FUN_00285e10` builds it (the list widget
`FUN_00296b48`, 8 visible rows from state +0x60 entries; `FUN_0028aa40` -> `FUN_0028b470` the row widgets,
`FUN_0028bc70(0xbce1)` the title, message 0xbce1) and `FUN_0028a1b8` fills the rows (the same filler serves page 10's
twin): each listed reward shows its `cond` text (+0), a plate group 0x61 when reached (0x24 when not), the text style
of the table at 0x382608 (reached / not), and a session NEW tag (group 0x78 when the page's byte +0x112d is set, else
0x77; `DAT_003825ac` / `DAT_003825b0`) when the reward is among the ones paid at this opening. An empty list shows the text
0xc194. A reached reward's detail text is the item for kind 0, else the texts 0xc1c3 (reward 0), 0xcc7d (reward 1),
0xc1c4 (the rest); the exact form of the second column, reached and not (24.9). The handler `FUN_00286060`: Up and Down move; a confirm only plays the
refused sound (`FUN_00291b58` answers 0 for page 0xb); cancel (-2) goes to page 9 (the cursor on row 5);
**-5 goes to page 10** (state +0 = 4) and **-5 on page 10 comes back** (`FUN_00285b30`, state +0 = 5): the two pages are
tabs of each other; -4 ends the menu. The help text of the page is 0xbce0 (`FUN_00288f00`).

### 24.5 The synthesis marks [V]

`DAT_0036d1a0` (0x28 bytes, cleared by `FUN_00284c10`) holds the visit's marks: +0 the entries (bit 2r, 2r + 1) learned or
made and not seen (`FUN_002916d8` at the end of the opening, over 0x2d bits) plus the ones learned during the visit
(`FUN_002889e0`, `FUN_00290cc0`); +8 the materials handed over for the first time (`FUN_002887c8`); +0x18 the rewards
paid. The state's byte +0x112d is set by the opening and cleared by the handlers that finish leaving page 9
(`FUN_00285388`), page 10 (`FUN_00285b30`) and the Recipes list, page 0x10 (`FUN_00286580`); the stores of that byte are
only in these three (`mips_scan --store-off 0x112d`), so leaving page 0xb, 0x17 or the other lists keeps it.
- Page 9 (`FUN_00284ea8`): rows 3 and 4 have a tag while +8 has a bit, row 5 while +0x18 has one, NEW while +0x112d is
  set and VIEWED after; row 1 (Free Development sets) and row 2 (Recipes) when the session's entries hold one of their
  entries (`FUN_002917c8`): NEW while one of them is unseen (`FUN_00291880`, record +0x20), else VIEWED; row 0 (Creations)
  a VIEWED tag when `FUN_00291960` finds a made entry among the session's.
- The lists (`FUN_00288fc0`): the cursor's entry is marked seen in the record (+0x20) every frame; an entry of the
  session's marks has the NEW tag while its seen bit is clear, VIEWED after.

### 24.6 What the port does (WP-SH3)

`menu_shop::synth::pay_rewards` / `cond_met` (the rewards), `Session` (page 0xb as `Page::Rewards`, `Marks`, `see`,
`entry_mark`, `row_mark`, `has_new`), `menu_shop::{item_place, fresh_at_open, item_tag, buy_tag, first_visit_bit}` and
`Page::First` (page 7) in `crates/kh2-app/src/menu_shop.rs`; `Progress` keeps the save bytes (`shop_seen` +0x3714,
`shop_visit` +0x3724, `Moogle::rewards` +8; all three are in the slot files since WP-PRG1). The tags are the sprite groups 0x77 / 0x78 on
the file's NEW! art; their place (left of the hand) and the page 0xb rows' columns are the port's [inferred: the rows have
no plate]. `X` is -5.
Harness (a state file; `KH2_STATE` is a path, `local/sh3/state-moogle.json` is
`{"munny": 500, "items": {"317": 2, "329": 2, "333": 2, "345": 6, "349": 1}}`; keys `shop_seen`, `shop_visits` and
`moogle.rewards` set the new bytes):
- Page 7 and the tags at Wantz's: `KH2_PROGRAMS=0,0,10 KH2_NPC_WATCH=2 KH2_NPC_WATCH_DIST=60 KH2_REACTION_PRESS=8
  KH2_KEYS=10:J,11:J kh2-game tt06` (log `shop: first visit of this class (save +0x3724 bit 0 clear): page 7, text 0xc401`,
  `shop: page 7 dismissed: save +0x3724 bit 0 set`, `shop: 2 NEW items`, `shop: item place 4 seen (save +0x3714 bit set,
  FUN_00265348)`, `shop: tags Buy row Some(New), ...`).
- The rewards and page 0xb at the moogle: `KH2_PROGRAMS=0,0,16 KH2_NPC_WATCH=5 KH2_NPC_WATCH_DIST=60
  KH2_REACTION_PRESS=8 KH2_STATE=local/sh3/state-moogle.json KH2_KEYS=10:J,11:J,12:J,13:J,14.5:X kh2-game tt07` (the
  third row opens with three notices, page 10, then -5 to page 0xb; log `synth: open the third row ... notices 0x7`,
  `synth: page 0xa -> page 0xb`).

### 24.7 Decompile index (WP-SH3)

| address | what |
|---|---|
| 0x25fcd8, 0x265498, 0x262140, 0x262350, 0x2621a0, 0x262450, 0x2624a0, 0x262540, 0x262558 | page 7 |
| 0x265280, 0x265ad0, 0x265348, 0x2653d8, 0x265420, 0x262bd0, 0x2626e8, 0x260a78, 0x264668, 0x260ec8, 0x264e80 | the shop's NEW marks |
| 0x291140, 0x291418, 0x291560, 0x2915f8, 0x291628, 0x2783f8, 0x1a18b8, 0x1a1768, 0x1a52b0, 0x290cc0 | the rewards |
| 0x286060, 0x285e10, 0x28b470, 0x28aa40, 0x28a1b8, 0x28ba50, 0x285b30, 0x285388, 0x288f00, 0x288da8 | page 0xb and the neighbours |
| 0x2916d8, 0x2917c8, 0x291880, 0x291960, 0x2919e8, 0x284ea8, 0x288fc0, 0x2889e0, 0x2887c8, 0x28f490 | the synthesis marks |
| data 0x381128, 0x381238, 0x382698, 0x3825ac, 0x382608, 0x382628 | first-visit texts, tag groups, special categories, row mark groups, row styles, notices |

Gaps: the window origin of the list pages and the exact place of the tags on the shop pages (the widgets `FUN_00296b48`,
`FUN_0029b600`, `FUN_00299878`: see 24.9); the look of the message window of page 7 (the port's text in the synthesis
window style); the difference between the VIEWED group 0x77 and the NEW group 0x78 beyond the animation length (both use
the same art rectangle); the count-up animation of page 10 and the rank and category tabs it highlights (24.9);
`+0x3714`, `+0x3724` and the record `+8` are in the slot files since WP-PRG1 (a reload does not pay the rewards again).

### 24.8 The command menu's Genie "Drive!" (WP-FX12b)

The in-play command menu (`items.rs`, not the pause menu) lists "Drive!" (command 605) as the third entry of the second deck while Genie is
out: `FUN_001bc6e0` tests the summon state `DAT_01d5e368 == 2` (`FUN_001c6918`) and the NeoStatus 0xc9 of the summon object (`FUN_001c6940`), calls
`FUN_001bcf10` and puts 0x25d where Party sits. `FUN_001bcf10` rebuilds list 3 (the manager's +0x180) from `FUN_001c6a20` (forms 1..4 whose item
count, `FUN_001a1cc8`, is above 0) and the command table at 0x37c036 (`FUN_001c6b10`; the disc has 535..538). The commands' `Execute` is their id, so
the generic executor `FUN_00177c20` -> `FUN_0016b420` raises event 11 on Genie's script (summons.md); the app issues `SummonRequest::Form`. Key path in the
app: H swaps the deck (the other deck's cursor kept), T / G move, J confirms, B goes back. Tests `second_deck_shows_drive_with_genie_and_issues_the_form`,
`genie_drive_rows_and_table_come_from_the_disc`. Gap: the entry's target-record parameter (`FUN_0018f490(+0xa20, Genie, 0)`).

- **MN15 review notes.** [inferred] World 0xf's limits test uses "best count above 0" (`FUN_002a20e0`); the original's `FUN_002ba238` tests the completion bits at `DAT_0036d4f4 + 0x74`, and their equivalence is not shown. `Counted` is now removed when an enemy's HP is above 0 again, so a revive counts each defeat (WP-HK1, 20.11a); `count_defeats` has a system-level test through `count_generic`. Section 14's "0x19..0x1f" for Customize is a different use of the numbers than the journal pages 0x19, 0x1a and 0x20.
- **MN15 review notes.** [inferred] World 0xf's limits test uses "best count above 0" (`FUN_002a20e0`); the original's `FUN_002ba238` tests the completion bits at `DAT_0036d4f4 + 0x74`, and their equivalence is not shown. The once-only `Counted` marker counts an enemy that revives only on its first defeat; the original counts each defeat. `count_defeats` itself has no system-level test (only `count_one`). Section 14's "0x19..0x1f" for Customize is a different use of the numbers than the journal pages 0x19, 0x1a and 0x20.

### 24.9 Shop gaps closed: reward texts, notice marks, material tags, story flag 0x4837, list geometry (WP-SH4)

**The second column of a page 0xb row [V]** (`FUN_0028a1b8`; the text lands in the row's second text widget,
`FUN_0028b470`: state `+0x52a + 0x40 k` is its string). Reached (the "reached" byte of the list entry, `FUN_00291140`):
- an item reward: `FUN_001a1f40(item, icon)` builds the command 9 (an icon) followed by the item's name (item `+8`).
  The icon is the material's category icon (`0x382660[item +4]`, 0x26..0x32 on the disc) for an item of type 0x10, else
  `FUN_00255368(type, 0)`: the first icon of the triple table at 0x3809d0 (0xb for a type not in it).
- another reward: the message 0xc1c3 (set 0), 0xcc7d (set 1) or 0xc1c4 with its variable (command 0xd) replaced by the text
  of 0xc1c5 (`FUN_00191a30`; on the disc 0xc1c5 is one icon).

Not reached (listed only with story flag 0x4837, below; the entry's byte is 0): the text of message 0xc1c5 for an
unlock, or the item's kind for an item reward (`0x382640[item +4]` for a material, `FUN_002553c0(type)` = the pairs at
0x380a60 otherwise; on the disc every kind text is one icon), then `FUN_0029bb48(message)`: the string at 0x3828b8 (`?`)
once for each character of the message (`FUN_00191660`: glyphs and icons up to a line end; spaces are not counted; 5 in
other languages), so an unreached reward shows its icon and as many question marks as the reward has letters. The row
plate group is 0x61 reached, 0x24 not (`DAT_003825a8`, `DAT_003825a0`), the text style the table at 0x382608
(`FUN_0028f468`). Port: `synth::reward_text` (`RewardText::{Item, Hidden, Filled}`) with `reward_tokens`, `masked`, `fill`
and `shown_chars` in `menu_shop.rs`; test `a_reward_row_shows_the_item_behind_its_icon_or_its_kind_and_question_marks`
and the disc test `the_disc_reward_texts_notices_and_list_extents`, which prints every kind text and asserts that each of
the 50 rewards has a text.

**The state's marks +0x134 and +0x13c [V]** (`FUN_00291140` writes, `FUN_002879e8` reads). Both are bit arrays that
only name what a notice is about. An item reward sets the bit of its reward index in `+0x13c` (notice 2); an unlock
reward other than sets 0 and 1 sets, in `+0x134`, the bit of the place of the first material whose category (item `+4`)
and rank (item `+5`) equal the row's bytes +5 and +6 (`FUN_00291628`, `FUN_002910b0`; notice 0x10). The notice page
(`FUN_002879e8`, `FUN_00287950`) shows the lowest set bit of `+0x130` and, for notice 1 and notice 4, the lowest bit of
`+0x13c` / `+0x134` as well: it calls `FUN_0028e980(message, kind text, name)` with the item's kind text (the same as
above) and the item's name (`+8`), clears that one bit, and clears the notice bit only when the array is empty. So every
reward item and every marked material has its own notice. The texts 0xbed0 ("Obtained {}{}") and 0xc193 ("{}{} can now be
purchased at shops") each hold two variables, filled in order by `FUN_00191a30`. Port: `Rewards::materials` (the `+0x134`
bits, by place), `Rewards::items` (`+0x13c`), `Session::queue` and `queue_item` (one entry per bit), `text_item`,
`first_material`; test `the_item_and_unlock_notices_name_the_reward_item_and_the_material_the_reward_marks`; the
screenshot of the first notice reads "Obtained", the item's icon and "Elixir" (the name in yellow), "for completing the
collection.".

**NEW tags of pages 10 and 0x17 [V]** (`FUN_0028f9a8`, both pages): a material handed over for the first time at this
opening (session `+8`, `FUN_002887c8`) has the tag group 0x78 (NEW) while the page byte `+0x112d` is set, else 0x77
(VIEWED); the byte falls when page 10 is left (24.5). The row also has a plate whose group comes from the table at 0x382600
by rank, the category label (`0x382640`) and the number (page 10 the state's `+0x68`, the displayed total that counts
up to the record's `+0xfc`, `FUN_0028f490`; page 0x17 the stock). Port: `synth::material_mark`; test
`a_material_handed_over_for_the_first_time_carries_a_tag_on_the_material_pages`; the screenshot of page 10 shows five
materials each with a NEW tag. Not ported: the count-up and the session marks `+0x10`, `+0x20`, `+0x24` it writes
(`FUN_0028f490`, `FUN_0028f698`: a highlight of the rank and category tabs while a total counts up; the port's page shows
the final totals at once, like the make's count-down) and the grid of rank and category tabs (`FUN_0028aaa8`,
`FUN_0028f850`, `FUN_0028fea0`): the port has no tabs, so there is nothing to mark.

**Story flag 0x4837 (`FUN_002783f8`) [V]**: the app reads it from the story flag store (`Store::story`, `Session::all`,
read when the third row opens). The baseline leaves flags clear, so by default none of this shows: the lists are as
before. With it set:
- page 0xb lists the rewards not reached too (already ported, 24.3), now with the hidden texts above;
- page 10 lists every material, those never handed over with their name as question marks and a 0 (`FUN_0028f9a8`,
  count `FUN_0028ffc8(1)`); page 0x17 is unchanged;
- the lists of page 9's rows 0, 1 and 2 (`FUN_00290058`) hold entries with the flag 0x80 (`synth::HIDDEN`, `entries_all`):
  Creations every entry not made (the higher rank's, odd, only of a recipe that has one), Free Development a recipe of
  set 1..3 and Recipes a recipe item (kind 0) that is not learned and not made. `FUN_00288fc0` shows such an entry as its
  item's name in question marks, dimmed (`FUN_0028f468(1)`), with no number and no tag, and never marks it seen; a confirm
  is refused (`FUN_00291b58` finds no recipe for it).
Not ported: the category tabs of `FUN_0028aaa8`, which also test the flag (the port has no tab grid). Test
`story_flag_4837_lists_what_the_player_cannot_have_yet_and_never_opens_it`; the screenshot of page 0xb with the flag
(`KH2_STATE` with `"story_flags": [18487]`) shows nine rows, the unreached ones dim with an icon and question marks, and
the screenshot of page 10 shows the materials never handed over as question marks with a 0.

**List geometry [V in part]**. The row builder of page 0xb (`FUN_0028b470`; `FUN_0028aa40` calls it for the pages other than 10, and `FUN_0028b188` for page 10) places row k's plate widget (group 0x62)
at `(0, k * +0x1c)` of the window widget, and its second text and the NEW tag at the group's x (`FUN_00138ed0(widget, +0x10,
+0x14)`) from it, both texts at the size the group holds in +0x18 (0xf when 0). On the disc (read by
`the_disc_reward_texts_notices_and_list_extents`, which asserts them) group 0x62 has x 270, y 0, size 13, +0x1c 23: so the
rows are 23 apart (the port used 24), the second text starts 270 to the right of the first and the NEW tag ends there
(quad x -39..1), and the text is 13 pixels tall. Port: `reward_metrics` reads them from the file and page 0xb uses them.
Page 10's rows are rebuilt by `FUN_0028f9a8` from group 0x60 and page 0x17's by the same function from group 0x6e; group 0x60 has (x 177, size 15, +0x1c 23, +0x20 228: two
columns of nine, page 10) and group 0x6e (x 175, size 16, +0x1c 26, +0x20 28), so page 10 shares page 0xb's pitch 23 and
page 0x17 has 26; the port applies these pitches (`group_pitch`) to its single column and not the grid's second column. Not found: the window widget's own origin on the screen
(`FUN_0029b600(window, 0x7537, ..., 0x67, 0x68, 0)` with groups whose records hold x 460, y 123; `FUN_0029ba00` parents it;
no layout layer carries a position, all are at 0,0; the widget classes `FUN_00296aa8`, `FUN_00299e38`, `FUN_00299878` set
the rest at run time), so the port keeps its own first row (70, 120), the title and the hand. The shop pages 1 and 4 keep
the port's pitch and columns for the same reason (their rows come from the generic list widget, not a row builder with
readable offsets).

Decompile index (WP-SH4):

| address | what |
|---|---|
| 0x28a1b8, 0x29bb48, 0x191660, 0x191820, 0x191870, 0x191a30, 0x1a1f40, 0x1a1ee0, 0x2553c0, 0x255368 | the second column of a reward row |
| 0x291140, 0x291628, 0x2910b0, 0x2879e8, 0x287950, 0x28e980 | the marks +0x134 / +0x13c and the notices |
| 0x28f9a8, 0x28ffc8, 0x28f490, 0x28f698, 0x28fea0, 0x28f468, 0x2887c8 | pages 10 and 0x17 |
| 0x290058, 0x288fc0, 0x28aaa8, 0x2783f8 | story flag 0x4837 |
| 0x28b470, 0x285e10, 0x237618, 0x237638, 0x237658, 0x237678, 0x237698, 0x138ed0, 0x29b6c0, 0x29ba00 | the row geometry |
| data 0x382640, 0x382660, 0x380a60, 0x3809d0, 0x3828b8, 0x382628, 0x382600 | kind texts, icons, type tables, the mask string, notices, plate groups |

Gaps (WP-SH4): the window origin of the list pages (above); `FUN_0028e980`'s window class (0x7546) and its colour and
placement, which the port draws as a text window; the second text's clip width (`FUN_00296d10` sets +6..+0xc of the list);
`FUN_00191a30`'s handling of a text with more variables than texts (the port drops the extra variable).

## 25. The battle report: menu kind 0xe (WP-JT4)

**Entry [V].** A trigger's jump record of type 5 (`FUN_0019d9d8` case 5) calls `FUN_0024f0b8(0xe, -1)`: record 0x380650 of the
table (1.2), scene 9, init `FUN_00292f18`, task `FUN_00292fc8`, page `FUN_00293020`. The init loads `ending.2ld` (`FUN_00293648`,
`FUN_002935e8`: the name `PTR_s_ending_2ld_0036d1b0`), and the task builds the page once, then loops (`FUN_00233cd8`,
`FUN_00102438`) without reading the pad. No jump follows.

**The page [V].** The backdrop is a layout of `ending.2ld` chosen by `FUN_001d77d0`: game mode (save `+0x1658`) 1 -> layout 1, mode 0
-> 0, mode 2 -> 3 when the completion percentage `FUN_002787b0` is 100, else 2. Each layout has three layers (seq 1 groups 1 and 2,
then seq 0 group 4, "The End"); a layer's `kind` (1 to 4 for the backdrops, 0) is the image of the file it draws from. The page
object has a title (message 0xcc24 "BATTLE REPORT", group 1 of sequence 0, centred, `+0x1a4 = 2`) and eight rows, each a group 2
object (pitch 40 = `unk1c` of the group, `FUN_00138ed0(row, 0, 40 * i)`) with a heading (message 0xcc18 .. 0xcc1f) and a value
object (group 3, offset 204, 20, right-aligned, `+0x108c = 1`). The values are strings built by `FUN_001918a0` (`%d`, `%d%%`,
` %d`) plus message 0xcc20 ("Times"): row 0 the mode name (0xc331 + mode: Beginner, Standard, Proud Mode); row 1 `FUN_002787b0`
% ("Completion of Jiminy's Journal"); row 2 `FUN_001de018(save + 0xacc0)` % ("Completion of Gummi Routes", the mean of 27 floats at
`+0x20` of the records `save + 0xacc0 + 0x40 * (3 * a + b - 1)`); row 3 u16 save `+0x2fdc` ("Healed Party Members"); row 4 u16 `+0x2dbe`
("Saved by Mickey", written by `FUN_001cda78` / `FUN_001cdb98`); row 5 the form with the largest u16 of the five at `+0x2fe2` ("Most
used Form": the name from 0x3827e8, entry k + 1 for counter k) and its count; row 6 u16 `+0x2fea` ("Used AntiForm"); row 7 the summon
with the largest u16 of `+0x2fec + 2 * i`, i = 0 to 3 (names 0xc594 Chicken Little, 0xc592 Genie, 0xc591 Stitch, 0xc593 Peter Pan, table 0x382800). The
running maximum starts at -1 and only a larger value replaces it, so all zeros give the first entry.

**What the port does.** `menu_ending.rs` (`EndingPlugin`, `Report`, `rows`, `layout_for`, `first_max`): `area_jump::JumpMenus.ending` (set by
`take_record` on `Outcome::OpenMenu { kind: 0xe }`) opens it; the layout, the title and the rows are drawn from the file with the
disc's texts; the field is frozen. Tests `the_layout_follows_the_mode_and_completion`, `the_most_used_entry_is_the_first_maximum`,
`the_rows_use_the_tables_of_the_executable`, `the_file_and_texts_exist`. Harness: `KH2_JUMP=5:0:0 KH2_JUMP_AT=4 KH2_SHOT_AT=8 kh2-game tt02`
logs "jump record type 5: opens menu kind 0xe ..." and "menu: kind 0xe open ... layout 1, rows [...]"; the screenshot shows the
black left side with the title, eight headings with their values on the line below, "The End" at the bottom left, and the artwork on the right.

**[inferred], gaps.** The rows' origin (8, 20), the value on the line under its heading (the value offset 204, 20 is the group's x and y;
on the heading's line the two overlap) and the colour (white) are not read from the code; no original frame of this screen was available.
Closing on a confirm, cancel or Start is the app's own (the task never reads the pad; the original leaves by a scene change from an
event). Rows 1 to 7 are 0: no counter exists in the app (`FUN_002787b0` needs the journal's completion aggregate, 20.9; the counters
at save `+0x2fdc`, `+0x2fe2`, `+0x2fea`, `+0x2fec` have no writer found by `mips_scan --store-off`, so they are written through a
computed base). Row 0 uses the app's game mode.

**Kind 5, the camp before a jump (`FUN_0024f300`).** `FUN_0024f300(a, b)` is `FUN_0026ef70(b)` (`DAT_0036cffc = b`) then
`FUN_0024f0b8(5, a)`; `FUN_0019d9d8` passes b = 0x1c8bda0, an address, so `FUN_0026ef80()` is never 0 and kind 5's init
`FUN_00250450` always opens page 0x15 (the "Party Members have been switched." notice, 19.4), not page 0x14. That page lists
the members that came in and went out of the party (word 4's arrangement). Ported in WP-PT1 (`party_switch.rs`, docs/party.md section 21):
the arrangement is applied (`party::jump_party`) and the notice opens over the frozen field; its frame geometry is [inferred] (addresses
`FUN_0024f300`, `FUN_00250450`, `FUN_0026ef80`, `FUN_0026e618`, `FUN_0026e970`). The wait is JT4's (events.md).

