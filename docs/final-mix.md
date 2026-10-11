# Final Mix compared with the USA disc (WP-7F)

What differs between the user's USA disc (`SLUS_210.05`) and the user's Kingdom Hearts II Final Mix
disc (`SLPM_666.75`), as facts read from the two discs and executables. Nothing from either disc is
in the repository; the numbers below are counts and sizes printed by the tests, the rules are told
in our own words with the addresses they come from. Which edition is "the original" for kh2-rs is
the user's decision; this page only lists facts.

Every difference is marked:
* **(a) RULE**: behaviour in the FM executable that can be ported as code and fed with USA data.
* **(b) FM-ONLY**: data or content the USA disc does not have; it cannot be reproduced from the USA
  disc (a gap for a USA-only build).
* **(c) BOTH, DIFFERENT VALUES**: a table that exists on both discs with other rows or values. The
  USA values stay the run-time source unless the user decides otherwise.

Tests (all read-only, they skip unless both images are configured):
```
$env:KH2_ISO    = 'C:\Users\KoZa\Documents\AGENTS\ClaudeShit\Kingdom Hearts II (USA).iso'
$env:KH2_ISO_FM = 'C:\Users\KoZa\Documents\ChatGPT\Kingdom Hearts II - Final Mix (Japan).iso'
cargo test -p kh2 --test fm_exe  -- --nocapture --test-threads 1
cargo test -p kh2 --test fm_disc -- --nocapture --test-threads 1
```
The FM disc is opened with the explicit opt-in `kh2_archive::Archive::open_edition(path, true)`, or
for the command line with `KH2_ALLOW_OTHER_DISC=1` (example: `$env:KH2_ALLOW_OTHER_DISC='1';
$env:KH2_ISO=<FM image>; cargo run -p kh2 -- mset obj/P_EX100.mset`). Without it every disc but
`SLUS_210.05` is still refused (`kh2-archive` unit test `other_discs_need_the_explicit_opt_in`).

## Player controls

The user's reports describe a dodge roll that the USA executable does not have (docs/movement.md
section 2). The Final Mix executable has it. Addresses below are FM unless marked USA; the FM
function that corresponds to a USA function was found by matching instruction bodies (relocations
masked) and is given as `USA 0x... = FM 0x...`.

### Square on the ground (a) RULE, test `fm_square_handler_tries_guard_roll_quick_run`
The Square handler sits in the same slot of the player's method table (USA 0x35ca28 -> 0x177ea0;
FM 0x34e9d8 -> 0x178eb0). FM's order, in an idle range:
1. As USA: combo reset (USA 0x176a70 = FM 0x177a28), the mover-0x16 mode (USA 0x1783d8 -> FM
   0x179450, body changed, not examined), Glide (USA 0x178290 = FM 0x179308, unchanged), the
   explicit keys 37, 12, 38 (USA 0x177fa8 = FM 0x178fe8, unchanged).
2. **New:** if the stick magnitude (actor +0x1c) is exactly 0.0, try the guard first (USA 0x178080 =
   FM 0x1790c0, unchanged body).
3. **New:** the dodge roll, FM 0x179530.
4. Quick Run, FM 0x179148 (USA 0x178108).
5. The guard again (FM 0x1790c0).

Consequences, read from that order (not run in an emulator):
* Square while standing still: the guard.
* Square while moving with Dodge Roll learned: always the roll (it comes before Quick Run).
* Square while moving, Dodge Roll not learned, Quick Run learned: Quick Run at once (as USA).
* Square while moving with neither learned: the guard (as USA). So "guard only when still" holds
  only while Dodge Roll or Quick Run is learned.
* Square while standing still when the guard fails (no guard record or window): the roll is tried
  (the roll itself has no stick test).

### Dodge Roll (FM 0x179530) (a) RULE + (b) FM-ONLY data, test `fm_dodge_roll_is_growth_slot_4_with_motion_209`
* Level = growth slot **4** (FM 0x1c3618 = USA 0x1c13b8 with argument 4); level 0 ends here. The
  growth levels are filled by FM 0x1c35a0 (USA 0x1c1340) from an ability table: USA has 4 growth
  kinds (High Jump, Quick Run, Aerial Dodge, Glide; table USA 0x37bda0, 4 x 5 ability ids), FM has 5
  (table FM 0x36dde8, 5 x 5). The fifth row (the Dodge Roll ability ids) is **(b) FM-only**.
* Same gates as Quick Run: not riding (FM 0x19c090 = USA 0x19aa28), the actor-type flag 2 and
  +0x160 bit 0x40 clear, and FM 0x1b9d40 (USA 0x1b7e00) true.
* Turns Sora at once to the stick direction (+0x10) with FM 0x16aef0 (USA 0x16a828).
* Plays motion **209** with blend 0 (FM 0x17f488, USA 0x17e220), then queues the follow-up command
  FM 0x17f5e0 (USA 0x17e378, the same call the USA attack start makes).
* Invincibility (FM 0x176a60 = USA 0x175aa8) for `fmab` field +0x40 of the Dodge Roll level's row.
* It installs **no mover** and sets **no velocity** (no call to FM 0x16bd60 = USA 0x16b698). The
  roll's travel is therefore the motion's root motion [inferred: root motion is the only remaining
  displacement term of the per-tick mover, docs/movement.md "Who moves the actor"].
* Motion 209 is `A180` in FM `obj/P_EX100.mset` (slot 838): 0..26 at 30 fps. Its triggers
  (`cargo run -p kh2 -- mset obj/P_EX100.mset` on the FM disc with the opt-in): frame triggers
  **34 at tick 10** and **35 at tick 34**, range triggers 2 from tick 36 (idle range: Sora may act
  again), 49 over ticks 0..34, 14 from tick 8, plus sound/effect triggers. USA `P_EX100.mset` has no
  motion 209 and no `A180` (docs/movement.md): **(b) FM-only content**.
* `fmab` rows: USA rows are 16 floats; FM rows have a real 17th column (+0x40, the roll's
  invincibility per level; `cargo run -p kh2 -- table pref_fmab` with the opt-in prints 5 rows with
  that column). **(b) FM-only data.** On the USA disc the sheet's `DodgeRollStarTime` column reads
  the next row's first field (docs/movement.md).

### Tap and hold: the threshold is a motion trigger (a) RULE, test `fm_frame_triggers_34_and_35_are_new`
The motion frame-trigger handler switches over 32 types in USA (0x188498, table 0x37aa78) and over 36
in FM (0x189740, table 0x36caac). Types 32 and 33 do nothing in FM. Two are new:
* **Type 34** (FM 0x189c78): if the command object's (+0xb44) flags (+8) have bit 4 (Square still
  held; the USA command handler 0x174cf8 clears +0x10c bit 4 when the command lacks flag 4), start
  Quick Run along the facing vector (+0x550) through FM 0x1791e8 (the Quick Run start, which itself
  returns at once when Quick Run is not learned).
* **Type 35** (FM 0x189c9c): if Sora is riding or the type-flag-2 / +0x160 bit 0x40 test is true,
  switch to motion 4 with blend 16.0 (the float at FM 0x36caa8, printed by the test).

So the user's rule is the FM rule with a precise threshold: tap Square while moving = the roll; still
holding Square when the roll reaches **tick 10** (its type-34 trigger; trigger frames are 60 Hz ticks,
docs/research.md) = Quick Run in the direction the roll faces. Released before tick 10: the roll plays
on and Sora can act again from tick 36. The app's current 12-tick threshold (docs/movement.md, "the
brief's default") is not the original's number; tick 10 is. Porting needs the FM motion `A180`, so a
USA-only build cannot show the roll's own animation (b).

### Quick Run (FM 0x179148 -> 0x1791e8) (a) RULE
* Same numbers and calls as USA 0x178108: growth slot 1, velocity = direction x `fmab` +0x10,
  brake/time/stop brake from +0x14/+0xc/+0x18, motion 203, mover 0x10, +0x10c bit 4, invincibility
  from +0x1c. Its mover tick (USA 0x1c1660 = FM 0x1c38c0) is unchanged.
* Changes: the start moved into a helper (FM 0x1791e8) that takes the direction as an argument, so
  trigger 34 can start it along the facing; the USA test `0.0 < stick` (USA 0x1781a8, the one FPU
  compare in USA 0x178108) is gone from FM's Quick Run (the Square handler's stick test comes first).

### Other player-code differences found (addresses for a later port; purpose not all identified)
* **Attack (circle) handler** (method slot USA 0x35ca24 -> 0x177d58; FM 0x34e9d4 -> 0x178d28): same
  record choice (keys 39 and 41, combo picker USA 0x1cb928/0x1cbcf8); FM calls the attack start
  (FM 0x177008 = USA 0x176050, unchanged) **before** the special motion reset that USA does first
  (actor-type flag 0x20 and record id >= 211: motion 0 on the object at USA 0x35f078 / FM
  0x351068), and FM stores a callback (FM 0x178e80, plays motion 0 on that object) at
  (+0x140)+0x194. Which character this serves: not identified.
* **Command handler** (USA 0x174cf8 / FM 0x175c88): for an attack command (kind 1) the "may cancel"
  argument of the availability check (USA 0x176b00 / FM 0x177ab8) is also set when the stats block
  (USA 0x1768a8 = FM 0x177860) word +0x14 has bit 0x2000. The writer of that bit: not identified.
* **Stats block layout**: FUN_001792f0 (method slot USA 0x35ca20, FM 0x17a480) reads stats +0x50 in
  USA and +0x54 in FM [inferred: the growth level bytes from +0x20 grew by one, shifting later fields].
* USA 0x176d78 / FM 0x177d30 (damage gate): FM adds a test of (+0x48c)+8 before the HP-share test.
* USA 0x177110 / FM 0x1780d8: USA tail-calls 0x174920, FM calls FM 0x1758b0 then FM 0x17a718.
* Bodies that differ and were **not examined** (gap): mover table set-up USA 0x179880 / FM 0x17aad0,
  per-tick displacement USA 0x181e00 / FM 0x183088, gravity latch USA 0x169ee8 / FM 0x16a5b0, range
  triggers USA 0x188030 / FM 0x1892d8 (still 54 types), USA 0x177788 / FM 0x178758, mover-0x16 mode
  USA 0x1783d8 / FM 0x179450, USA 0x1c70f0 / FM 0x1c92a0.
* **Unchanged** (identical bodies): jump mover (USA 0x17b620, 0x17b6e0, 0x1832b8, 0x17b440), Aerial
  Dodge 0x17bad0, Glide 0x178290, guard 0x178080, Quick Run mover 0x1c1660, attack start 0x176050,
  leap aim 0x176400, stick zeroing 0x1778e8, explicit keys 0x177fa8, fall restart 0x16a2f0.

## 1. Disc identity and archive layout (test `fm_disc_identity_and_archive_layout`)
| | USA | Final Mix |
|---|---|---|
| boot executable (SYSTEM.CNF `BOOT2`) | `SLUS_210.05`, VER 1.00 | `SLPM_666.75`, VER 1.01 |
| executable size | 2,686,708 bytes | 2,629,364 bytes |
| region / video | NTSC (USA) | NTSC (Japan) |
| disc files | 15 | 15 (same names, the executable aside) |
| `KH2.IDX` entries | 4,925 | 5,159 |
| with the 19 world sub-indexes | 26,461 | 33,720 (all named by OpenKH's list) |
| `KH2.IMG` | 3,940,937,728 bytes | 2,933,575,680 bytes |
| `OVL.IDX` / `OVL.IMG` | 4 code overlays, 651,264 bytes | 17 entries: 5 compressed code overlays (adds `ovl_shop.x`) and 12 movies (`zmovie/fm/*.pss`, 1,371,772,928 bytes) |

`kh2-archive` opens the FM disc unchanged once the opt-in is given: same IDX layout, same hashes, same
LZ compression (the system tables and all 5 code overlays decompress). The only change made to
`kh2-archive` is the opt-in (`ALLOW_OTHER_DISC_ENV`, `Archive::open_edition`, `boot_executable`);
USA behaviour is identical. Movies moved from `KH2.IMG` (`movie/us/*`, 11 files) into `OVL.IMG`.

## 2. File lists (test `fm_file_lists_by_kind`)
By name hash and size; "changed" = present on both with another length (same length is not proof of
same content). Kinds from the OpenKH names.

| kind | only FM | only USA | size changed | same size |
|---|---|---|---|---|
| obj | 1,218 | 796 | 389 | 2,785 |
| anm | 339 | 120 | 81 | 12,557 |
| ard | 20 | 0 | 241 | 28 |
| msn | 303 | 270 | 0 | 0 |
| event/layout | 93 | 3 | 0 | 0 |
| event/effect | 50 | 0 | 4 | 406 |
| field2d | 50 | 46 | 0 | 0 |
| itempic | 42 | 72 | 0 | 210 |
| limit | 31 | 27 | 0 | 0 |
| gumibattle | 40 | 37 | 8 | 60 |
| se | 22 | 0 | 33 | 1,121 |
| bgm | 12 | 0 | 0 | 208 |
| magic | 0 | 0 | 0 | 43 |
| dbg | 0 | 0 | 52 | 16 |
| idx (world sub-indexes) | 0 | 0 | 19 | 0 |
| 2ld | 0 | 20 | 0 | 0 |
| msg | `msg/jp` 25 | `msg/us` 25 | | |
| menu | `menu/fm` 228 | `menu/us` 219 | | |
| map | `map/jp` 278 | `map/us` 266 | | |
| voice | `voice/fm` 5,986, `voice/fj` 6,541 | `voice/us` 6,105 | | `voice/us` 52 |
| movie | (in OVL.IMG) | `movie/us` 11 | | |

The full table, with the remaining small kinds, is printed by the test. Top-level system files with a
new size (c): `00battle.bin`, `03system.bin`, `00objentry.bin`, `00progress.bin`, `07localset.bin`,
`12soundinfo.bar`, `14mission.bar`, `50gumientry.bin`, `60gb_effect.bar`, seven `libretto-*.bar`;
only on FM (b): `15jigsaw.bin`, `eventviewer.bar`, `npack/worldobj.bar`, `vagstream/zzz.vas`,
`bgm/music063..069` with five new wave banks. `00common.bdx` grew by 16 bytes. Every `magic/` file and both `effect/` files
are the same size on both discs.

## 3. Tables the app reads (test `fm_tables_the_app_reads`)
Every layout of `sheets/tables.json` decodes on the FM disc with the USA layout and exact sizes (no
layout difference found); 17 tables have more rows (c), 7 are the same shape.

| table | USA rows | FM rows |
|---|---|---|
| atkp | 2,326 (277 trailing bytes) | 2,713 (277 trailing bytes) |
| enmp | 177 | 229 |
| plrp | 61 | 64 |
| przt | 141 | 184 |
| bons | 152 | 179 |
| vtbl | 238 | 241 |
| limt | 27 | 31 |
| fmlv | 38 | 45 |
| cmd | 636 (version 1) | 751 (version 2) |
| trsr | 382 | 430 |
| item / item_stat | 466 / 125 | 535 / 151 |
| objentry | 1,696 | 1,900 |
| pref_prty | 66 | 70 |
| pref (whole subfile) | 7,272 bytes | 7,372 bytes |
| same shape | btlv, sumn, magc, stop, sklt, lvup, lvpm, pref_plyr, pref_sstm, pref_fmab (5 rows) | |

`pref_fmab` decodes as 5 rows on both because it is read row-per-pointer; the FM rows really have
the 17th float (Player controls above). `cmd` changes its version word (1 -> 2). Shared row indexes
should not be assumed to mean the same thing: added rows were not checked for insertion versus
appending (gap).

## 4. Text and voice
* Text: FM has only `msg/jp` (25 files, the same 25 names as `msg/us`); no English text (b). The
  app's text path (`msg/us/*.bar`) finds nothing on FM.
* Language folders: the USA executable picks entry 2 ("us") of its table at USA 0x349d44 with a
  constant (`kh2_audio::voice::LANGUAGE_INDEX`). FM decides at boot: FM 0x105b58 reads
  `\SYSTEM.CNF;1`, compares the boot name with an edition table at FM 0x33cb20 (8 editions: SLPM
  662.33 jp, SLUS 210.05 us, five SLES releases uk/it/sp/gr/fr, SLPM 666.75 fm) and FM 0x105af8
  stores two folder names: the edition's own (0x33caf0, "fm") and, for edition 8, a second one
  "jp" (0x33caf4). That matches the disc: `voice/fm`, `menu/fm` and `msg/jp`, `map/jp`.
* Battle voices: FM keeps `voice/us/battle/*.vsb` (52 banks); 49 are byte-identical to the USA disc
  (test `fm_voice_battle_banks_are_the_usa_files`), so the app's battle-voice path
  (`voice/us/battle/...`) would find them.
* Event voices: two FM sets, `voice/fm/event` (5,985 files) and `voice/fj/event` (6,541); 5,984 and
  6,076 of their names exist in USA `voice/us/event`, but only 73 / 75 have the same size. Which set
  is English and which Japanese was not determined (nobody listened; the names suggest `fm` =
  English, `fj` = Japanese [inferred]); the selector of `fj` was not found (gap).

## 5. Executable: USA addresses do not apply (test `fm_executable_layout_and_app_addresses`)
* Both load at 0x100000; the FM image is 0x27a800 bytes, the USA image 0x288ebc. Code moves by a
  varying amount: about +0x250 around the motion code (USA 0x130ad8 = FM 0x130d28), +0xfd0..+0x1190
  in the player code (USA 0x177ea0 area = FM 0x178eb0), -0x6b00..-0x12d00 further up. The data tables checked
  below move by -0xd200 to -0xe0f8, each by its own amount; 9 of the 27 USA data addresses the app
  reads lie past the end of FM's image.
* Constant tables the app reads (`from_exe` readers) found in FM by their 16 bytes (unique hits;
  printed by the test): camera rows 0x378f04 -> 0x36ae84, wall margins 0x378e44 -> 0x36adc4,
  overhead 0x37939c -> 0x36b32c, event 0x379470 -> 0x36b400, turn 0x37a4c8 -> 0x36c4d4, battle flag
  0x37ad14 -> 0x36cd5c, guard 0x37a4a8 -> 0x36c4b4, reaction constants 0x37a4a0 -> 0x36c4ac,
  0x37a954 -> 0x36c984, 0x37ac74 -> 0x36ccbc, 0x37b408 -> 0x36d440, 0x35ce40 -> 0x34edf8, prizes
  0x37b478 -> 0x36d4b0, 0x37b4b0 -> 0x36d4e8, 0x37b540 -> 0x36d578, 0x37bc04 -> 0x36dc44,
  0x37bf34 -> 0x36df84, projection 0x3780f8 -> 0x36a0b0, drive digits 0x358998 -> 0x34a8a0, party
  AI parameters 0x34ea00 -> 0x341800. Same bytes, so the values are the same; only the addresses move.
* Not found by bytes (they hold pointers): HUD menu tables 0x3551c0 / 0x355530, speaker names
  0x35ee10, language table 0x349d44 (FM 0x33cb00 by its strings, section 4), BDX host group table
  0x35bce8 (FM 0x34dbf8 by code reference [inferred]); drive digit tables 0x3589c0 and 0x3589e8 are
  ambiguous by bytes (FM 0x34a8c8 / 0x34a8f0 by code reference [inferred]).
* Fixed USA function addresses the app depends on: `kh2_bdx::names` (host functions by address),
  the kh2-ee oracle tests (`crates/*/tests/*oracle*`, `kh2-fx/src/oracle_tests.rs`), the motion entry
  0x130ad8 (`kh2 rig-sweep`). Examples from the body match: 0x130ad8 = FM 0x130d28, 0x1ae440 = FM
  0x1afee8, 0x155bf0 = FM 0x1560a8, 0x20ab40 = FM 0x20d528; others changed body (0x1d7cb0 BDX
  interpreter -> FM 0x1da558, 0x1a6ff0, 0x1c77a8, 0x21a660) and would need their own check.
* `local/tools/mips_scan.py` reads `..\kh2_ps2\input\SLUS_210.05` only.

## 6. Staged plan for a second disc (proposal; nothing of it is implemented)
1. **Paths**: a second image path (`KH2_ISO_FM`, used by these tests), opened only through
   `Archive::open_edition(.., true)`; the app keeps refusing it until step 3.
2. **Rules on USA data** (no FM disc needed at run time): the Square order (guard when still, roll,
   Quick Run) and the type-34/35 trigger rules can be coded now; with USA data they have no roll
   motion, no Dodge Roll ability and no 17th `fmab` column, so they would stay switched off or use a
   stand-in (a user decision; the current app's 12-tick roll is such a stand-in, the original's
   threshold is the trigger at tick 10).
3. **Per-edition address table**: one struct per edition with every fixed executable address the app
   reads (section 5); fill the FM column with the byte matches above and verify each by a kh2-ee or
   byte test like `fm_executable_layout_and_app_addresses`. Choose the edition from `SYSTEM.CNF`
   (`Archive::boot_executable`), as FM itself does.
4. **Language**: FM's folders (`fm` for voice/menu, `jp` for msg/map); Japanese text needs a font
   and text path for `msg/jp` (not checked).
5. **First things to test on FM**: table decoding (already passes), Sora's motions from FM
   `P_EX100.mset` (it lists 89 slots with motion out of 839: how FM stores the others was not
   checked, gap), the roll A180 in the harness, then BDX scripts with the FM host table.

## Decompile index (FM addresses from the user's `SLPM_666.75`; read as instructions)
| address | read for |
|---|---|
| FM 0x178eb0 (USA 0x177ea0) | Square handler |
| FM 0x179530 | Dodge Roll start |
| FM 0x179148, 0x1791e8 (USA 0x178108) | Quick Run entry and start |
| FM 0x1c35a0, 0x1c3618, 0x36dde8 (USA 0x1c1340, 0x1c13b8, 0x37bda0) | growth levels |
| FM 0x189740, 0x36caac, 0x189c78, 0x189c9c (USA 0x188498, 0x37aa78) | frame triggers, types 34 and 35 |
| FM 0x178d28 (USA 0x177d58) | attack handler |
| FM 0x175c88 (USA 0x174cf8), USA 0x176b00 | command handler, availability check |
| FM 0x17a480, 0x177d30, 0x1780d8 | other changed player methods |
| FM 0x105af8, 0x105b58, 0x33cb00, 0x33cb20 | edition and language folders |
| FM 0x34e958.. / USA 0x35c9a8.. | player method table, compared slot by slot |

## Gaps (searched without result)
* Writer of stats +0x14 bit 0x2000 (FM): not searched beyond the command handler.
* Purpose of the attack-handler callback FM 0x178e80 and of the object at FM 0x351068.
* The selector between `voice/fm` and `voice/fj` (language folder code FM 0x105af8 only sets `fm`
  and `jp`).
* Changed but unexamined bodies listed under "Other player-code differences".
* Whether FM rows added to `cmd`, `item`, `enmp`, `atkp`, ... were inserted or appended.
* How FM `P_EX100.mset` stores the slots without an inline motion.
* No emulator run was made: the roll and the tick-10 hold were not watched in PCSX2.
