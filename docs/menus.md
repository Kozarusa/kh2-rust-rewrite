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
| 3 | 2 | `FUN_002756f8` / `FUN_00275bd8` | **Jiminy's Journal** (loads `jmemo.2ld`); the camp's Journal entry leaves the camp for this scene |
| 1 | 3 | `FUN_00255598` / `FUN_00255608` | [I] title-side menu (tiny init) |
| 2 | 4 | `FUN_002589f0` / `FUN_00258a68` | [I] title: load/save file screens (`FUN_00258b38` loads `save.2ld`, index 1) |
| 4 | 5 | `FUN_0025f168` / `FUN_0025f710` | [I] shops and synthesis (opened by field events 0x13, 0x66..0x75, 0x83..0x85, 0x8a through `FUN_00197b70`; uses `itemshop.2ld`, index 4) |
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
| 0x1a, 0x1b | `FUN_00271078` | Customize: shortcut and spell pages |
| 0x1c | `FUN_002716d0` | Customize [I] |
| 0x1d..0x21 | `FUN_00272018` | Customize: the item-use and Auto-Reload lists |
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
two) was not read; use the clip's timings.

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
The state it changes is the party arrangement (`docs/party.md`: `FUN_0010fcd0`, `FUN_0010f220`); the page code and the equipment
return were not read in detail [GAP: `FUN_0026dbd8`, `FUN_0026e158`, `FUN_0026df80`, `FUN_0026d378`].

### 2.5b Journal [V name only]

Entries 5 and 6 of the top list leave the camp (`FUN_0024f4f0`, `FUN_00296458`) for menu kind 3 with `jmemo.2ld`. Only the entry,
its two names and its help text are in scope (the screen itself is not specified here).

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
(the port saves to a file of its own). [GAP: the page logic of kinds 7/8.]

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
| `crates/kh2-app/src/sora.rs` or the pad reader | the pause: while `Menu.open`, no `Pad` input reaches Sora and the `Sim` sets stop (`Time<Virtual>` paused, or a run condition on the `Sim` sets, `schedule.rs`) |

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
- **Party (0x14..0x16), the Help pages (0x25) and the tutorial `.bar` resources, Journal, Save pages (kinds 7, 8):** not read beyond
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

## 9. M1 as built (WP-M1)

Code: `menu.rs` (state, input, freeze, transition), `menu_gfx.rs` (draw path and camera, a copy of the HUD's because `hud_gfx` is private), `menu_text.rs`, `menu_pages.rs` (page 0 table and flags), `kh2-image::layd` (the `.2ld` reader).

- **Closed gaps (addresses).** System sounds: `FUN_002be49c(2, id, ...)`: the 2 is the driver SLOT that holds `se/se000.seb` (docs/audio.md "System sounds": `FUN_002c0eb0` loads it at boot, `FUN_002c0da8` names the slot bank 0), not bank 2 (WP-SE2 correction; `se002.seb` is a two-entry area bank, so the earlier `PlaySe::number(2000 + id)` never found its data). The menu sounds are entries of `se000.seb`, request numbers `id` (log form `se 0:<id>`): cursor 1, confirm 2, cancel 4, refused 5, open 3 (`menu::menu_sound`; `FUN_002c2b70` at 0x2c2cd8 is the same call with a slot taken from a request). Row groups: each entry's set is (enter, leave or inactive, loop); `FUN_00296fb8` starts a selected row in set B[0] with B[2] as the follow-up, `FUN_00298808` (list inactive) switches rows to index 1, `FUN_002988e8` back to index 2. Row pitch is the group's `unk20` (`FUN_00296bd0` with `FUN_00237698`), 26. The row text starts at the row group's anchor point (`FUN_00139048` with the group-offset flag cleared by `FUN_00139888`). Layers: camp layouts 0, 1, 2 are the enter, hold and leave phases (groups 12/39/110, 13/30/40/111, 14/41/112). Panel outline groups 0x2e/0x2f/0x30 (`FUN_00250a00`), Munny and Time plates 0x6b/0x6d/0x6c and 0x71/0x73/0x72, digit groups 0x33+d (Munny) and 0x3d+d with colon 0x47 (Time), NEW tag groups 0x31/0x32 (`DAT_003807e8/ec`), hand group 0x19 (`FUN_00253f98`), glint 0x1b/0x1d/0x1c.
- **Measured from clip 10, not read [F]**: the fit of footage pixels to menu units (x_px = 190 + 1.133 x, y_px = 26.5 + 1.087 (y + 16) at 960x540, which also shows the HUD's 16 line offset applies), row label offset from the anchor (-7, 1.5), the hand and glint offsets from the row anchor, area name, help line and digit placement, party card shapes and colours, the 2 frames per tick pace, the dim (0.3 brightness, in gamma space), the 14-frame close.
- **Not done / gaps**: the party card widget (`FUN_00295bc0` object vtable +0x18, `FUN_0023b348`..`FUN_0023b5d0`) was not decoded, so the card is drawn from shapes; NEW tags (`FUN_00293ba0`, `FUN_00294b58`.. decide them) are not drawn; the party is Sora alone; sub-pages beep (refused sound). The deselect group switch of the row the cursor leaves is [I].
- **Harness**: `KH2_KEYS` knows Enter, Escape, Backspace; scripted keys are pressed in `PreUpdate`. `KH2_MENU_FLAGS=0x70,0x72` replaces the story-flag set (default 0x70..0x73). Log lines: `menu: open`, `menu: page 0 cursor N`, `menu: close`.

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
- **[GAP] Not ported: the forms', summons' and members' tabs.** Their rows and number sources are in 10.2, but the form experience table (`FUN_001d3b90`) and the other
  members' stat blocks have no counterpart in the app; the tab list therefore shows only the tabs the port draws. The party is Sora alone (docs/party.md).
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
- **Page 0x19** (`FUN_002707f8`): the characters (Sora). Confirm opens page 0x1a (`0x1b` for another character: Donald and Goofy are not ported), cancel goes to page 0, -4 closes. Help: message 0x3b89.
- **Page 0x1a**, Sora's settings (`FUN_00271078`, rows built by `FUN_00270ae0`, the working table by `FUN_002723c8`): the row kinds of the leader are the bytes at 0x381d40: 0, 1, 2 (the others' table, 0x381d48, is 3, 4, 2). **Kind 0**: four shortcut rows (the u16 at save `+0x2600 + 2 k`), text = the icon of the item whose command the word is (`FUN_00272fc8`: the first consumable or spell item in table order with that command through `FUN_001a1f78`) and the command's own name (`FUN_00272e68`: the command row's message), or `---` (message 0x448) when empty; each row is tagged with its own button label (messages 0x47d..0x480, the button icons 37, 34, 36, 35). **Kind 1**: one row per owned spell in slot order (Fire, Blizzard, Thunder, Cure, Magnet, Reflect: the type 0x12 items of `ItemIndex::magic_items` with a count), tagged "MP Charge" (0x481); the flag is `FUN_001a4bb8` (the switch bit **clear** is "MP Charge"). **Kind 2**: one row per distinct battle item (the id each slot carries, or reserves when empty), tagged "Auto-Reload" (0x482); the flag is that slot's reserved id equal to the item. At the end of the routine each shortcut row takes the flag of the last row with the same item. The row icon is `FUN_00255368(type, flag)` through the table at 0x3809d0 (type, unequipped icon, equipped icon; 0xb when unlisted), and `FUN_00272b20` passes the flag for items and its **negation for spells**: a spell that is on shows icon 8 and one that is off icon 18, which are the icons of the sub page's two rows. Cancel goes to page 0x19; -6 (the quick key, `Y`) switches a spell row (`FUN_00274e50`) or an item row (`FUN_00274f70`, after the question when needed) and does nothing on a shortcut row; -10 and -11 switch the character (refused with one character); confirm opens the kind's sub page: 0x1d, 0x1e or 0x1f (`FUN_00271078` jump table 0x381bb4: the kinds 1 to 4 load 0x1e to 0x21).
- **Page 0x1d**, the shortcut's command (`FUN_00272018`, list `FUN_00273d30`): row 0 "Remove Shortcut settings." (message 0x1df7), then the stock list filtered with the argument 10 of `FUN_00283810` (bit 0, the consumables, and bit 18, the spells), spells first because `FUN_002845a0` keys a spell by its table position and everything else by position plus 1000 (oracle: `stock_list_matches_the_game` checks it as its filter 5). At most 8 rows are shown. Confirm runs **`FUN_00274dd0`**: the u16 at `+0x2600 + 2 * slot` becomes `FUN_001a1f78(item)`: a consumable's own command (`item +0x10`), a spell's loaded command (`FUN_001a4dd0`), 0 for the removal row. The slot is the settings page's remembered cursor, `FUN_00252fe8(0x1a)`.
- **Pages 0x1e and 0x1f**: two rows whose index is the flag: 0x1e "[icon 18]Don't MP Charge" (0x1df9) then "[icon 8]MP Charge" (0x1df8) (table 0x381d20); 0x1f "[icon 0]Auto-Reload OFF" (0x1dfd) then "[icon 12]Auto-Reload ON" (0x1dfc) (table 0x381d30). A confirm on the row that is already the state changes nothing; otherwise `FUN_00274c88` runs the switch (`FUN_00274e50` flips the bit of the word at save `+0x264c`; `FUN_00274f70` sets or clears the reserved id of every battle item slot that carries or reserves the item, the items stay in their slots).
- **Page 0x1c**, the question (`FUN_00271688`, message 0x46a): when Auto-Reload is switched **off** for an item that is on and a slot reserves it without carrying it (`FUN_00275288`: an empty slot waiting for a refill), the information window asks "Turn auto-reload function Off?" with No and Yes; Yes switches it off, No and cancel go back.

**Evidence that runs.** `progress::tests::menu_writers_match_the_game`: the game's `FUN_001a4b68`, `FUN_001a4b30` and `FUN_001a4bb8` (and the flip `FUN_00274e50` makes of them) in kh2-ee on the new-game block against `set_spell_switch` and `toggle_spell_switch` over eight bits and six flips, the shortcut word at save `+0x2600` (party block `+0xfa0`), and the stock take: `oracle: menu writers equal (spell switch set/clear/flip [...], 4 shortcut words, 18 stock takes; counts floor at 0)`. `menu::customize::tests` (the rows, the shortcut flag copy, the page flows, the candidate order), `progress::tests::auto_reload_reserves_and_releases_every_slot_of_the_item`, `progress::tests::shortcuts_spell_switches_and_boosts_write_their_words`. `cargo test -p kh2-image --test menu_items -- --nocapture` prints the Customize tables.

**Layout [I].** Header tabs "Customize" and "Sora"; the window 0xe3 then 0xe4 (anchor (111, 84)), nine rows of the kind's plate (0x108, 0x107, 0x106; pitch 26) with the section tag 0x11a, 0x119, 0x118 to the left of the first row of a kind (each shortcut row has its own); the sub pages' panels (the enter and hold groups of the triples at 0x381cd8: 0xc6/0xc7 for the shortcuts, 0xbd/0xbe for spells, 0xb4/0xb5 for items; 248 by 254 at the left edge) with the title strip 0x124 (the button label or the section name) and rows of plate 0x123 (pitch 28, eight rows fit); the question uses the Yes / No information window of section 13. No clip of these pages exists.

**Not done / gaps.** Donald's and Goofy's pages (kinds 3 and 4: Battle Style and their ability list, `FUN_00275398`, `FUN_00275410`) and the character switch; the Help overlay (-7) and the NEW tags; the start row of page 0x1d (the port starts on the row of the shortcut's current item [inferred]); the field's L1 shortcut list that uses the shortcuts is `items::CommandLists` list 5, which the app can reach with no key (docs/items.md 4); **the spell switch word is not read by the spell availability**: `MagicData::status` (`magic.rs`) passes `false` for the `toggle_bit` of `magic_rules::availability` (docs/magic.md: with too little MP a spell whose bit is clear is "short of MP" and still castable, a set bit makes it unavailable, `FUN_001ba2c0` through `FUN_001a4bb8`), so the page's switch is stored and logged but changes nothing in a fight yet. The change is outside this stage's files (it needs `Progress::spell_switch(slot)` for the command's slot at the callers of `MagicData::status`).

**Harness.** `KH2_KEYS=3:Enter,3.5:Down,4:J,4.5:J` opens Sora's settings (Customize is the second row of the list when Abilities is hidden); `5:Down,5.4:J,5.8:Down,6.2:J` stores Fire in shortcut 1; `Y` on a spell or an item row switches it. `KH2_STATE` `{"sora_items": [1, 1, 3], "magic": [1, 1, 1, 1]}` (`local/state-customize.json`) gives two Potions and an Ether in Sora's slots (the first pickup also sets shortcut 0) and four spells. Log lines: `menu: page 0x19`, `menu: page 0x1a`, `menu: page 0x1d|0x1e|0x1f|0x1c`, `menu: shortcut S = command C (<name>), word 0x0031, shortcuts [..]`, `menu: spell slot S MP Charge|Don't MP Charge (switch word 0x..)`, `menu: auto-reload item I ON|OFF (slots [(item, reserved), ..])`.

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

`FUN_002673c0`'s forms case (0x267788) and summons case (0x2678dc), read from the instructions: the level (form record `+2`; the summon byte `+0xdce`), the experience in total (`FUN_001d3c98`, `FUN_001d3d48`: the record's experience (`+4`, `0x340e4c`) plus the `fmlv` Exp of the levels 1 .. level-1), the experience to the next level (`FUN_001d3c38`, `FUN_001d3cf8`: the Exp of the row at the level minus the record's experience while level < cap, else 0; cap `FUN_001d3bd0` / `FUN_001d4e68` = 2 + the owned form items (summon charms), 7 instead of 6) and the gauge row `level + 2`. The forms' second column is the leader's strength, magic, defense and the four resistances. The tab list uses the group sets at 0x380870 for these tabs and the names `0x381a38 + 2 * form` and message 0xc5b1. The summons' tab lists, besides its four rows, bit `10 + j` for each summon charm j held (`FUN_00268758`); what those rows print was not read (the case writes no buffer for them): the port draws their labels and an empty value [GAP]. Not ported: the party members' tabs (Donald and Goofy have no status block in the app). Tests: `menu_status::tests::a_forms_experience_is_the_record_plus_the_rows_below_its_level`, `the_cap_is_two_plus_the_owned_items_and_seven_instead_of_six`, `the_forms_and_summons_tabs_have_their_own_rows_and_values`. Screenshots `local/m7/st0.png` (Valor tab on the list) and `st1.png` (its numbers).

### 16.6 Party cards and the HUD items deck

**Cards.** `FUN_00295bc0` returns `DAT_0036d1f8`, the object `FUN_00295720` creates (`FUN_0023a018`, vtable 0x36ca50): the **3D party models** standing in the world (`FUN_0023a520` loads the member's model and animation 0x8b, the head and eye textures, the per-slot lights); it is not the name/LV/HP/MP card. The card's drawing code was not found (searched: `FUN_00250a00`, `FUN_002506b8`, `FUN_00251100`, `FUN_0023a848`, `FUN_0023b3a8..0023b5d0`). The cards stay as built in M1 (shapes and colours from clip 10); the party is Sora alone. [GAP]

**Items page overlapping the Magic row (accept scenario `items-page`).** It is the field's command deck (`hud_menu.rs`, the Items sub list under COMMANDS), not a pause menu page: the header plate of ITEMS is drawn over the Magic row. Resolved by WP-F2 item C as the original's own look: the game draws every stacked list in full and places each list's header by its own row count (docs/hud.md, command menu "Structure"), so a one-entry sub list's ITEMS plate sits where the parent's third row is. No change.

### 16.7 Harness

`KH2_KEYS` knows `X` (-5) and `Select` / `Tab` (-7). Examples: `KH2_KEYS=6:Enter,6.5:Down,7:J,7.6:X,8.4:X` (Abilities, then to Items, then back); `6:Enter,6.5:Down,7:J,7.6:J,8.2:Tab,9.2:J` (the Help overlay from the Abilities list, then its second page); with `KH2_STATE=local/m7/state-equip.json` (the state of section 15) `6:Enter,6.6:J,7.2:J,7.8:J,9:Down` shows the pick list with the pictures and the NEW tags. Log lines: `menu: new tags: new items N, new spells N, abilities known N (fresh N)`, `menu: help topic T (N pages), return to page 0x..`, `menu: help done, page 0x..`, `menu: item picture N loaded (WxH)`.

## 17. Growth and the other equippable abilities as menu rows (WP-G8)

Code: `abilities.rs` (`Abilities::follow_progress`, the `follow_progress` system, the `abilities: growth level ...` log), `progress.rs` (`upgrade_ability`), `menu_abilities.rs` (`infos`).

**The original's rule [V].** High Jump, Quick Run, Aerial Dodge and Glide are ordinary ability items (type 0x13, kind 3 = the Growth group of page 0x10), four rows each: ids 94..97, 98..101, 102..105, 106..109 (LV1, LV2, LV3, MAX), bits 2..17, AP 2, 2, 3, 3 per level on this disc. They sit in the member's 64-slot list (`record + 0x54`) like any other ability, are toggled by `FUN_0027d8b8` (bit 15) through the page's Equip / Don't Equip prompt or the quick key, and cost AP by the same rule (`FUN_0027d6c0`). `FUN_001c0e50` applies a slot only when bit 15 is set and `FUN_001c1340` takes, per group, the highest set bit of the table at 0x37bda0: **a growth ability's level is the highest equipped level of its group**; an unequipped level counts for nothing. The controller reads it through `FUN_001c13b8` (Quick Run `FUN_00178108` slot 1, Glide `FUN_00178290` slot 3, High Jump `FUN_0016bd98` slot 0; docs/moveset-selection.md). Sora obtains them from the drive forms' level-ups: `FUN_001d3da8` calls `FUN_001d3f18(record + 0x54, 0x40, form, display level - 1)`, which replaces the first slot holding the chain's id one level below (0x37c556) with the next level and **keeps that slot's bit 15**; the first level fills the first empty slot, so it arrives unequipped (`progress::tests::growth_upgrade_matches_the_game`, kh2-ee: `oracle: FUN_001d3f18 equal for 50 calls`). The new game gives Sora none of them (`FUN_001c0cb0(0)`, docs/progress.md); the app baseline does not either.

**What the port does.** The menu already built a row for every slot with an ability item (`Table::build`, section 12); the growth rows are such rows (group Growth, the item's own name, help text and AP). What was missing was the path from the record to the controller: `Abilities` copied Sora's list once at start, so a level-up's gift or a later change of the record did not reach the block. Now `Abilities::follow_progress` (system `follow_progress`, FixedUpdate before `sync_controller`, run when `Progress` changed) copies Sora's list, equipment and the app's Dodge Roll level from `Progress` whenever they differ and rebuilds the block; the harness's additions (`KH2_ABILITY_IDS`, `KH2_ABILITIES`) stay after the copy. `sync_controller` then writes `sora::Growth` and logs `abilities: growth level High Jump a, Quick Run b, Aerial Dodge c, Glide d, Dodge Roll e (from Sora's equipped abilities | the drive form's own list | the KH2_GROWTH override)` on the first tick and on every change. `KH2_GROWTH` stays an override outside a form (docs/movement.md section 2). `Progress::upgrade_ability(member, old, new)` is the port of `FUN_001d3f18` for the form level-up (its call site is `prize.rs` `form_exp`, outside this WP: see the report; until it is wired the level-up only logs the upgrade). Tests: `abilities::tests::growth_level_is_the_highest_equipped_level`, `the_block_follows_the_record`, `progress::tests::growth_upgrade_keeps_the_equipped_bit_and_matches_the_array_form`, `growth_upgrade_matches_the_game`.

**Dodge Roll** is not an ability on the USA disc (no item row, no name message; docs/movement.md section 2, docs/final-mix.md): it stays the app field `Progress::dodge_roll` (baseline level 1, `KH2_STATE` `"dodge_roll"`), with no menu row. A row for it would need a name and an id the disc does not have [GAP, kept out on purpose].

**Every ability row of the disc** (`menu_abilities::tests::every_ability_row_of_the_disc_is_a_menu_row`, `cargo test -p kh2-app --bin kh2-game -- every_ability_row --nocapture` prints the full list): 145 rows; each is a row of its group with its own AP when Sora holds it (checked for all 145 in 64-slot pages), every name is in `sys.bar`. Rows with AP 0 (the drive forms' actions such as Brave Shot, Wisdom Shot, the dummies, Final Arcana) cannot be toggled (`FUN_0027dc30`), as in the original. What the app reads of each (78 of 145): the 16 growth rows (growth levels), the 27 stat abilities of `FUN_001c0f40` (the block), the attack table gates of Sora's moveset (Guard, Upper Slash, the slashes and aerial actions 262..273 and their form copies), the limit gates (198..211, `limit.rs`), Magic Lock-On (403). **Not read by the app yet [GAP: their readers test the bit through `FUN_001768a8`, not traced in this WP]:** Scan (138), Aerial Recovery (158), Auto Valor / Wisdom / Master / Final / Summon (385..389), Leaf Bracer (402), No Experience (404), Second Chance (415), Once More (416), Auto Limit (417), Auto Change (418), Auto Healing (420); Donald's and Goofy's actions (165..168, 176, 177, 187, 190, 195, 215, 423..448: the party is Sora alone); the forms' own actions (216..225, 246, 247, 257..261, 519..530: they gate the forms' movesets, which this test does not load [inferred]). They are equippable and cost AP in the menu; equipping them sets their bit and nothing else.

**Harness.** `KH2_STATE=local/g8/state-swap.json` (`{"abilities": [94], "abilities_off": [98], "dodge_roll": 1}`), `KH2_KEYS=6:Enter,6.5:Down,7:J,7.6:J,8.2:Y,8.8:Down,9.4:Y` (open Abilities, Sora, take High Jump LV1 off, equip Quick Run LV1): `abilities: growth level High Jump 1, Quick Run 0, ...` at start, `abilities: High Jump LV1 (id 94, slot 0) taken off, no longer equipped; AP 0/2`, `abilities: growth level High Jump 0, Quick Run 0, ...`, `abilities: Quick Run LV1 (id 98, slot 1) equipped; AP 2/2`, `abilities: growth level High Jump 0, Quick Run 1, ...`; screenshot `local/g8/menu-swap.png` (the Growth group with High Jump LV1 unequipped and Quick Run LV1 equipped, AP 2/2). The movement reads it: with `local/g8/state-growth.json` (all four LV1 equipped) `KH2_KEYS=5.5:W:3.5,7.6:K:0.6` logs `sora quick run: Square held 10 ticks, Quick Run level 1`; with `state-swap.json` (Quick Run off) `sora dodge roll: Square held 10 ticks but Quick Run is not learned (level 0)`.