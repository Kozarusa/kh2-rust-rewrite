# Pause menu: NEW tags, switch, Help overlay, item pictures

[Contract](contract.md) | [Index](../../README.md)

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

Page 0x1a (Customize) also has Help topics in the original (`FUN_00271078`); not ported [GAP: not read; WP-CU1 read only the help line `FUN_002743e8`, not the Help overlay (-7) of these pages].

### 16.4 Item pictures [V file and id, I placement]

The item window (`FUN_0029ed50`; page 2 and the pick pages `FUN_0026b1c8`, page 0xb `FUN_0025bad0`) shows the item's picture and its stat rows. The picture id is the s16 at item `+0x14` (`Picture` of the `item` sheet, -1 none; `FUN_00283520`); the picture cache (`FUN_002a07c8`, slots at `DAT_0036d3ac`) reads **`itempic/item-%03d.imd`** (the format at 0x3829c0, `FUN_002a0ba0` -> `FUN_00167d20`): 128 by 128, 8-bit IMGD files, 282 of them in the disc (`menu_m7::item_pictures_decode`; item-001 is the Potion jar of clip 6 at 12.5 s). `FUN_00283578` waits for the cursor to rest (`DAT_0036d174`, counted down from the float at 0x3823c4 = 0x4170 high word; the delay was not converted) and `FUN_002830f8` draws the slot as a sprite scaled to `unk20 / 100` of the window group's width (100 % where the window has no frame group). The picture windows of the pick pages are the triples at 0x381a78 (weapons 0xd5/0xd7/0xd6: box 376..505, forms 0xdf.., armor 0xb0.., accessories 0x9e.., items 0xb9..: box 257..414 by 72..230), page 2's are 0x11f and 0x120 (box 324..505 by 72..326), page 0xb's 0x131 and 0x132; the strip below the picture is 0x381a68. Port: `Pictures` (`menu_pic.rs`) loads the IMD on first use and keeps the texture; the picture is centred across the window's last-frame box and 10 units under its top [inferred: the sprite's anchor was not read]. Seen in `local/m7/eq1.png` (the Weapons list: the keyblade picture in the window with the strip line below) and the logs `menu: item picture N loaded (128x128)`. Not drawn: the stat rows of the same window (`FUN_0029f1f8`, `FUN_0029f890`: AP, Strength, Magic, Defense and the four resistances from the item's `item_stat` row, the ability of the item as the ninth row; groups 0x20..0x26 and 0x76..0x7c, labels at 0x382990 / 0x3829a8) [GAP: the rows' layout was not placed].

### 16.5 Status: the forms' and summons' tabs [V numbers, I layout]

`FUN_002673c0`'s forms case (0x267788) and summons case (0x2678dc), read from the instructions: the level (form record `+2`; the summon byte `+0xdce`), the experience in total (`FUN_001d3c98`, `FUN_001d3d48`: the record's experience (`+4`, `0x340e4c`) plus the `fmlv` Exp of the levels 1 .. level-1), the experience to the next level (`FUN_001d3c38`, `FUN_001d3cf8`: the Exp of the row at the level minus the record's experience while level < cap, else 0; cap `FUN_001d3bd0` / `FUN_001d4e68` = 2 + the owned form items (summon charms), 7 instead of 6) and the gauge row `level + 2`. The forms' second column is the leader's strength, magic, defense and the four resistances. The tab list uses the group sets at 0x380870 for these tabs and the names `0x381a38 + 2 * form` and message 0xc5b1. The summons' tab lists, besides its four rows, bit `10 + j` for each summon charm j held (`FUN_00268758`); what those rows print was not read (the case writes no buffer for them): the port draws their labels and an empty value [GAP]. The party members' tabs: 19.1. Tests: `menu_status::tests::a_forms_experience_is_the_record_plus_the_rows_below_its_level`, `the_cap_is_two_plus_the_owned_items_and_seven_instead_of_six`, `the_forms_and_summons_tabs_have_their_own_rows_and_values`. Screenshots `local/m7/st0.png` (Valor tab on the list) and `st1.png` (its numbers).

### 16.6 Party cards and the HUD items deck

**Cards.** `FUN_00295bc0` returns `DAT_0036d1f8`, the object `FUN_00295720` creates (`FUN_0023a018`, vtable 0x36ca50): the **3D party models** standing in the world (`FUN_0023a520` loads the member's model and animation 0x8b, the head and eye textures, the per-slot lights); it is not the name/LV/HP/MP card. The card's drawing code was not found (searched: `FUN_00250a00`, `FUN_002506b8`, `FUN_00251100`, `FUN_0023a848`, `FUN_0023b3a8..0023b5d0`). The cards stay as built in M1 (shapes and colours from clip 10); the party is Sora alone. [GAP]

**Items page overlapping the Magic row (accept scenario `items-page`).** It is the field's command deck (`hud_menu.rs`, the Items sub list under COMMANDS), not a pause menu page: the header plate of ITEMS is drawn over the Magic row. Resolved by WP-F2 item C as the original's own look: the game draws every stacked list in full and places each list's header by its own row count (docs/systems/ui/contract.md, command menu "Structure"), so a one-entry sub list's ITEMS plate sits where the parent's third row is. No change.

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

