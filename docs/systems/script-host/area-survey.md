# Area survey: every loadable area run headless

Work package SV2, 2026-10-19. Written from run logs only; no game data (counts and kind names only). Gaps are gaps of the app, not bugs found in the original.

## Method

- Binary: `target\debug\kh2-game.exe <area>` of branch wp/SV2 with the collision fix below, one process at a time through `local/capped.ps1 -CapMB 3500`, a 150 s kill timeout per run, environment `KH2_NO_SOUND=1 KH2_NO_MUSIC=1 KH2_AREALOG=1 KH2_BDXLOG=1 KH2_SHOT_AT=16 KH2_SCREENSHOT=<png>`.
- Areas: every `ard/*.ard` of the disc index, 289 files (world prefixes as `area::WORLDS`). Each run lasts about 22 s: it loads, plays 16 s of game time, takes the screenshot and exits.
- Enemy kinds: the `area groups:` log line lists, per btl program group, the kinds the app has (`enemy::KINDS`) and those it skips ("not in the app"). For the survey only, a local uncommitted build also listed groups whose members were all skipped (the shipped code drops such groups from the log); that patch was reverted. Counts below are distinct kinds over all btl groups of the area, not only the groups the script loads. The per-world tables below are the SV2 data (they predate EN9 to EN11f, so their "present" and "absent" columns undercount the kinds the app has now); the ranking in "Ranked gaps" was refreshed by WP-SV3.
- Screenshots were not looked at; "loads" means the log shows the map line (or the map warning) and the screenshot step ran.

## Result

- 289 of 289 areas run to the screenshot and exit 0 (after the fix). 0 panics, 0 hangs, 0 `unimplemented host call` lines.
- Before the fix, `eh22` and `eh29` were killed by the 3500 MB cap (`memory allocation of 2214592528 bytes failed`; capped.ps1 reports exit 1 with 'killed by the 3500 MB memory cap', 14 s and 11 s in). Reproduce on the commit before the fix: `powershell -NoProfile -ExecutionPolicy Bypass -File local/capped.ps1 -CapMB 3500 -Exe target\debug\kh2-game.exe -ArgLine eh22` with `KH2_ISO` set.
- 23 areas have no `map/us/<area>.map` on the disc and log the warning `not in archive` / `not found or not a map`: al08, dc07, hb21, hb22, hb23, hb24, hb25, hb26, hb27, hb28, hb29, hb30, hb31, hb32, hb33, hb34, hb35, hb36, hb37, hb38, hb39, hb40, po10. The area script and groups still run.
- The only area-script opcode read but not ported is 0x16 `load_file` (7 areas). The only battle manager seen is WORLDMAP (0xc, wm00), already a documented gap.

## Fix made

`crates/kh2-app/src/world.rs`: `World::push` put every triangle into a 400-unit cell grid, one entry per covered cell. `eh22` and `eh29` contain a floor millions of units wide (render bounds about 1.44 million either way), which needed over 2 GB of grid entries. A triangle that covers more than 1024 cells now goes to a `big` list that every `near` query tests directly. Ordinary maps are unchanged. Test: `a_floor_wider_than_the_grid_limit_is_found_without_grid_entries`. After the fix both areas load and exit 0 (21 s each).

## Collision groups (WP-IW1)

The COCT meshes carry a group number and the game only walks meshes whose group bit is in the area script's map visibility word (docs/systems/core-runtime/collision.md section 8). `kh2 coct-walls <area>` lists the walls per group and which are off: eh10 (program 0,1,0) has 108 party-wall polygons in groups 3 to 8 that are off under word 0x407, he05 (word 0x3) 10 polygons in groups 2, 3, 12, 22, 32 and 62, al01 (word 0xc1ffffffff) none. The app made them all solid, which stopped Sora at the arena walls of eh10.

## Area opcode 0xb (WP-IW3)

Survey of every `ard/*.ard` on the disc (269 files; all `map`, `btl` and `evt` programs, nested opcode 0x1c commands included), read with `kh2_ard::Area::parse`, no game run. 183 commands in 73 areas, all in `map` programs except 19 in `btl` programs (hb08, hb16, he09, mu07, nm07, eh25). The mask is 64-bit (argument 0 low, argument 1 high). Distinct masks: bit 62 (0x4000000000000000) 142 times, bit 61 15, bit 59 4, bits 61 and 62 3, 0x4000000100401000 (bits 12, 22, 32, 62; he05 programs 0x1a, 0x6f, 0x1b) 3, bit 58 3, bit 60 3, bit 31 2, bit 11 2, bit 41 2, bit 21 2, bits 58 to 62 once, bit 56 once. Most areas carry bit 62 in several programs. The areas the app starts in: tt04 programs 0x4f and 0x50 (bit 62), eh10 program 0x39 (bit 62), he05 programs 0, 1, 2, 0x1a, 0x1b, 0x1c, 0x6f. tt00 and tt02 have none. Areas: al00 al03 al05 al09 al13 al14 bb00 bb01 bb03 bb04 bb11 bb15 ca01 ca02 ca05 ca10 ca14 ca18 dc00 eh10 eh14 eh15 eh19 eh25 hb01 hb04 hb07 hb08 hb09 hb16 hb17 hb18 hb20 he02 he05 he06 he07 he09 he17 he18 he19 lk04 lk05 lk15 mu01 mu02 mu03 mu05 mu07 mu08 mu09 mu10 nm00 nm03 nm06 nm07 nm09 nm10 tr02 tr04 tr09 tt03 tt04 tt05 tt08 tt14 tt19 tt25 tt29 tt30 tt41 wi02 wi03. Effect in the app: the word is stored and the hosts 1.46 / 1.47 apply it (docs/systems/core-runtime/collision.md 8.1).

## Ranked gaps

### 1. Enemy kinds missing from `enemy::KINDS`

Refreshed by WP-SV3 (2026-10-20) after EN9 to EN11f: `enemy::KINDS` has 38 kinds and every kind of the old top 25 is in it, so the old ranking table is replaced by the tables below. Source: the disc only, no game run; nothing beyond object ids, model names and counts is written here.

**Method.** A throwaway test (deleted) parsed `ard/<world><nn>.ard` for the 18 world prefixes and nn 00 to 59 (1080 candidates, the 289 files that exist, as in "Method" above) with `kh2_ard::Area::parse`, listed with `Area::spawned()` every entity of every spawn group that any program of the `map`, `btl` and `evt` scripts loads, and looked each object up in `00objentry` (`kh2_tables::tables::Objentry`). An entity counts as an enemy kind when its `ObjectType` is 4 or 21 (0x15, the second enemy class that `M_EX940` has) and its model is not among the 38 in `KINDS`. Columns: "areas" is distinct areas; "groups" is distinct (script, program, spawn tag) triples, so a group that three programs load counts three times; "members" is the entities of the kind over those triples; "sole groups" counts the triples in which every other member is a kind the app has (or the player and friends), so adding the kind alone makes the whole group playable. The script columns come from `cargo run -p kh2 -- bdx <model>` (through `local/capped.ps1 -CapMB 2500`, one process at a time): word count; "= X" when the listing body equals kind X's (same script, only the model differs, like `M_EX020_RAW`); the `enmp` row (the literal handed to `set_obj_param`, host 2.9, found in the caller of the set-up routine; a static read, `?` where the call has another shape); and the host calls the script makes that no script of the 38 kinds makes and that `enemy_host.rs` has no arm for. That last column is a grep of `enemy_host.rs` by group and index, so [inferred]: an arm may exist under another form, and the real answer is the `missing` line of `KH2_BDXLOG=1`. Hosts 0.9 (the runtime's own thread start), 1.36, 1.37 (arms exist), 1.114 (`FindObject`) and 1.311 (an arm for the gauge target) were removed from it by hand. The `enmp` method reproduced the rows of kinds already in `KINDS` (`M_EX660` 38, `M_EX650` 65, `M_EX950` 50, `M_EX110` 0).

**Totals.** 88 absent objects of ObjectType 4 or 21 appear in spawn groups: 23 base enemy kinds, 41 world variants and twins (suffixes `_WI` Timeless River, `_NM` Halloween and Christmas Town, `_TR` Space Paranoids, `_AL` Agrabah, `_RAW`, `_LC`, `_DC`, `_EH`), 8 bosses (`B_*`), 4 Struggle or event opponents (`N_EX610_BTL`, `N_EX610_BTL2`, `N_AL070_BTL`, `P_EX130`; the first two have NeoStatus 5058 and stand in groups with FRIEND and PLAYER entries, [inferred] Struggle matches) and 12 world objects that are not enemies. The old count of 100 included models that are not ObjectType 4 or 21.

**Blocked or special.**
- `M_EX900` (12 areas): in `KINDS` since WP-EN12a (soaked in tt19 b_00 and tt03 b_40: every dive ends through the event 17 frame trigger, docs/systems/script-host/enemies-m-ex8-9.md "M_EX900").
- `M_EX950` is in `KINDS` (EN9e). Its reaction-command modes (`rc_*`, hosts 1.165 and 1.237, event 11 with 312 or 323) and its two spawn objects were never reached; that is a gap of the kind, not an absent kind.
- Bosses: `B_CA020`, `B_CA030`, `B_CA040` (14 areas together, each needs 1.222 and 2.29), `B_CA000` (ca10, eh02, eh03; needs 0.32 and 1.250), `B_LK110_PHANTOM` (lk03, lk04, lk05, lk07, group b_35 in each, no missing host call: the cheapest boss), `B_EX210_EH`, `B_EX350` and `B_MU130` (the last two have no usable BDX entry: `kh2 bdx` reports an empty or missing script, so their AI lives elsewhere).
- Not enemies: `F_*` and `EH_G_*` objects (Timeless River props, the Hollow Bastion map objects F_BB050 and F_BB060, the 100 Acre Wood game F_PO090 family, the Gummi enemies of eh22 and eh26). They need their own handling, not an enemy kind.

#### Base kinds (not yet in `KINDS`, ranked by areas)

| kind | areas | groups (members) | objentry id / type / NeoStatus | enmp row | script | host calls the enemy host lacks | groups with no other absent kind | first areas |
|---|---|---|---|---|---|---|---|---|
| `M_EX710` | 28 | 57 (166) | 122 / 21 / 1025 | 11 | 6782 w | 1.247 random_free_point, 2.36 nop_2_36 | 37 | bb00 bb04 bb08 bb09 |
| `M_EX130` | 21 | 40 (158) | 73 / 21 / 1036 | 2 | 7663 w | - | 26 | al00 al03 al12 al13 |
| `M_EX900` | 12 | 13 (44) | 309 / 4 / 3004 | 45 | 7439 w | - | 13 | eh11 eh25 he09 tt03 |
| `M_EX680` | 10 | 18 (47) | 16 / 21 / 1022 | 66 | 12124 w | - | 4 | hb01 hb04 hb09 hb11 |
| `M_EX570` | 9 | 19 (56) | 9 / 21 / 1011 | 60 | 5178 w | - | 9 | he09 mu01 mu02 mu03 |
| `M_EX890` | 8 | 9 (32) | 308 / 21 / 3003 | 44 | 6901 w | - | 9 | bb00 bb04 eh07 he09 |
| `M_EX550` | 7 | 9 (84) | 7 / 4 / 1009 | 58 | 7185 w | 1.52 bits_47c_set | 5 | dc00 dc02 dc03 he05 |
| `M_EX580` | 7 | 24 (138) | 10 / 4 / 1012 | 61 | 4331 w | - | 11 | he09 mu01 mu02 mu03 |
| `M_BB010_AX` | 6 | 13 (21) | 368 / 4 / 1033 | ? | 6451 w | 1.52 bits_47c_set, 1.107 set_f_234 | 4 | bb06 bb07 bb08 bb09 |
| `M_BB010_SWORD` | 6 | 15 (26) | 367 / 4 / 1032 | ? | 4725 w | 1.52 bits_47c_set, 1.107 set_f_234 | 5 | bb06 bb07 bb08 bb09 |
| `M_EX590` | 6 | 10 (52) | 75 / 4 / 1013 | 4 | 6556 w | - | 7 | al01 bb08 he09 mu02 |
| `M_EX610` | 6 | 10 (65) | 1114 / 4 / 1015 | 101 | 7410 w | - | 1 | hb09 hb16 hb18 tr01 |
| `M_EX500` | 5 | 7 (28) | 2 / 4 / 1004 | 54 | 7498 w | 1.125 tween_start | 7 | he08 he09 he11 he16 |
| `M_EX670` | 5 | 15 (51) | 15 / 21 / 1021 | ? | 10738 w | 1.52 bits_47c_set, 1.218 set_spawn_param, 1.270 motion_register | 5 | lk03 lk04 lk05 lk07 |
| `M_EX720` | 5 | 11 (52) | 19 / 4 / 1026 | 80 | 7044 w | 1.218 set_spawn_param | 3 | lk03 lk04 lk05 lk07 |
| `M_EX780` | 5 | 9 (37) | 20 / 4 / 1039 | 81 | 5588 w | - | 4 | lk03 lk04 lk05 lk07 |
| `M_EX740` | 4 | 5 (36) | 124 / 4 / 1044 | 13 | 5677 w | - | 3 | he09 he15 he16 tt39 |
| `M_EX800` | 4 | 5 (32) | 1575 / 21 / 1037 | 39 | 7054 w | 1.52 bits_47c_set | 0 | dc00 dc02 he09 lk04 |
| `M_EX800_MU` | 4 | 7 (24) | 2238 / 21 / 1037 | 39 | 7054 w | 1.52 bits_47c_set | 3 | mu02 mu07 mu08 mu09 |
| `M_EX200` | 1 | 1 (2) | 118 / 4 / 1042 | 8 | 4635 w | - | 1 | he09 |
| `M_EX530` | 1 | 5 (36) | 5 / 4 / 1007 | 126 | 16640 w | 1.247 random_free_point, 1.248 scatter_point | 1 | hb09 |
| `M_EX540` | 1 | 1 (2) | 6 / 4 / 1008 | 57 | 6261 w | - | 0 | he09 |
| `M_EX640` | 1 | 1 (2) | 13 / 21 / 1018 | 64 | 4160 w | - | 0 | he09 |

#### World variants and twins

Rows whose script column says "= X" run the same listing as `X`, so the work is a `Kind` constant with the variant's model and `enmp` row plus a check of its model and motion set (the `M_EX020_RAW` precedent, docs/systems/script-host/contract.md). The others have their own script.

| kind | areas | groups (members) | objentry id / type / NeoStatus | enmp row | script | host calls the enemy host lacks | groups with no other absent kind | first areas |
|---|---|---|---|---|---|---|---|---|
| `M_EX020_WI` | 5 | 8 (48) | 1840 / 4 / 1001 | 34 | 5333 w, = M_EX020 | - | 0 | wi01 wi02 wi03 wi06 |
| `M_EX540_WI` | 5 | 7 (28) | 1828 / 4 / 1008 | 57 | 6261 w | - | 0 | wi01 wi02 wi04 wi05 |
| `M_EX550_WI` | 5 | 5 (35) | 1829 / 4 / 1009 | 58 | 7185 w | 1.52 bits_47c_set | 1 | wi01 wi02 wi03 wi04 |
| `M_EX660_WI` | 5 | 5 (50) | 1843 / 4 / 1020 | 38 | 7449 w, = M_EX660 | - | 0 | wi01 wi02 wi03 wi05 |
| `M_EX010_WI` | 4 | 4 (17) | 1849 / 4 / 1000 | 33 | 4891 w, = M_EX010 | - | 0 | wi02 wi03 wi04 wi07 |
| `M_EX120_NM` | 4 | 4 (20) | 1834 / 4 / 1035 | 1 | 4357 w, = M_EX120 | - | 0 | nm02 nm03 nm04 nm06 |
| `M_EX200_NM` | 4 | 12 (59) | 119 / 4 / 1042 | 8 | 4635 w | - | 0 | nm00 nm02 nm03 nm04 |
| `M_EX500_NM` | 4 | 5 (18) | 1827 / 4 / 1004 | 54 | 7498 w | 1.125 tween_start | 0 | nm02 nm03 nm04 nm06 |
| `M_EX640_WI` | 4 | 5 (16) | 1831 / 21 / 1018 | 64 | 4160 w | - | 0 | wi03 wi04 wi05 wi06 |
| `M_EX760_NM` | 4 | 6 (31) | 1845 / 4 / 1028 | 36 | 7982 w, = M_EX760 | - | 0 | nm00 nm02 nm04 nm06 |
| `M_EX010_NM` | 3 | 5 (15) | 1838 / 4 / 1000 | 33 | 4891 w, = M_EX010 | - | 0 | nm00 nm03 nm04 |
| `M_EX020_NM` | 3 | 5 (20) | 1220 / 4 / 1001 | 34 | 5333 w, = M_EX020 | - | 0 | nm00 nm02 nm06 |
| `M_EX050_WI` | 3 | 3 (10) | 2025 / 4 / 1002 | 35 | 5580 w, = M_EX050 | - | 0 | wi02 wi05 wi06 |
| `M_EX420_NM` | 3 | 8 (23) | 1836 / 4 / 1003 | 9 | 6501 w, = M_EX420 | - | 0 | nm02 nm04 nm06 |
| `M_EX560_WI` | 3 | 4 (16) | 1830 / 4 / 1010 | 59 | 4715 w, = M_EX560 | - | 0 | wi04 wi05 wi07 |
| `M_EX660_RAW` | 3 | 7 (36) | 1368 / 4 / 1020 | ? | 7587 w | - | 7 | he09 lk13 mu07 |
| `M_EX700_NM` | 3 | 6 (39) | 1833 / 4 / 1024 | 68 | 5265 w | - | 0 | nm02 nm03 nm06 |
| `M_EX790_HALLOWEEN_NM` | 3 | 6 (18) | 1775 / 21 / 1046 | ? | 4394 w | - | 0 | nm00 nm02 nm03 |
| `M_AL020_FIRE_RAW` | 2 | 3 (30) | 1670 / 4 / 1030 | 5 | 4137 w, = M_AL020_FIRE | - | 1 | al10 he09 |
| `M_AL020_ICEE_RAW` | 2 | 2 (17) | 1671 / 4 / 1031 | 6 | 5129 w, = M_AL020_ICEE | - | 0 | al10 he09 |
| `M_EX010_TR` | 2 | 2 (3) | 1839 / 4 / 1000 | 33 | 4891 w, = M_EX010 | - | 1 | tr01 tr03 |
| `M_EX120_TR` | 2 | 3 (9) | 1852 / 4 / 1035 | 1 | 4357 w, = M_EX120 | - | 1 | tr04 tr06 |
| `M_EX610_RAW` | 2 | 2 (31) | 1875 / 4 / 1015 | 101 | 7410 w | - | 1 | tr03 tr07 |
| `M_EX650_TR` | 2 | 3 (7) | 1832 / 4 / 1019 | 65 | 5895 w, = M_EX650 | - | 1 | tr01 tr06 |
| `M_EX770_TR` | 2 | 3 (6) | 1847 / 4 / 1029 | 37 | 7953 w, = M_EX770 | - | 0 | tr04 tr06 |
| `M_EX130_AL` | 1 | 2 (6) | 1888 / 4 / 1036 | ? | 4901 w | - | 0 | al14 |
| `M_EX520_AL` | 1 | 2 (50) | 1887 / 4 / 1006 | ? | 4600 w | - | 0 | al14 |
| `M_EX530_TR` | 1 | 1 (4) | 1835 / 4 / 1007 | 126 | 16640 w | 1.247 random_free_point, 1.248 scatter_point | 0 | tr04 |
| `M_EX600_LC` | 1 | 4 (44) | 1675 / 4 / 0 | ? | 2896 w | 1.222 objentry_word0 | 0 | tr02 |
| `M_EX600_LC_ATK` | 1 | 6 (68) | 1717 / 4 / 0 | ? | 2896 w | 1.222 objentry_word0 | 0 | tr02 |
| `M_EX600_LC_CHG` | 1 | 6 (72) | 1718 / 4 / 0 | ? | 2896 w | 1.222 objentry_word0 | 0 | tr02 |
| `M_EX600_LC_GRD` | 1 | 6 (52) | 1719 / 4 / 0 | ? | 2896 w | 1.222 objentry_word0 | 0 | tr02 |
| `M_EX620_AL` | 1 | 2 (20) | 1760 / 4 / 1016 | ? | 5144 w | - | 0 | al14 |
| `M_EX660_AL` | 1 | 2 (64) | 1889 / 4 / 1020 | ? | 4691 w | - | 0 | al14 |
| `M_EX750_NM` | 1 | 2 (8) | 1837 / 4 / 1045 | 14 | 5899 w, = M_EX750 | - | 0 | nm04 |
| `M_EX760_RAW` | 1 | 3 (3) | 1468 / 4 / 1028 | ? | 8479 w | - | 0 | hb17 |
| `M_EX770_RAW` | 1 | 3 (3) | 1469 / 4 / 1029 | ? | 7768 w | - | 0 | hb17 |
| `M_EX790_CHRISTMAS_NM` | 1 | 8 (19) | 1777 / 21 / 1041 | ? | 5606 w | - | 0 | nm06 |
| `M_EX800_DC` | 1 | 2 (54) | 2288 / 21 / 1037 | 39 | 7852 w | 0.8 debug_print_8, 1.52 bits_47c_set | 2 | dc00 |
| `M_EX800_RAW` | 1 | 1 (1) | 1576 / 21 / 1037 | 39 | 7054 w | 1.52 bits_47c_set | 1 | dc00 |
| `M_EX880_DANCER_EH` | 1 | 1 (5) | 2294 / 4 / 0 | 132 | 5292 w | 1.222 objentry_word0 | 0 | eh24 |

#### Bosses, Struggle opponents and event objects

| kind | areas | groups (members) | objentry id / type / NeoStatus | enmp row | script | host calls the enemy host lacks | groups with no other absent kind | first areas |
|---|---|---|---|---|---|---|---|---|
| `B_CA040` | 14 | 31 (111) | 1109 / 4 / 5053 | 105 | 9186 w | 1.222 objentry_word0, 2.29 attack_flag_8_or_10 | 20 | ca01 ca02 ca03 ca05 |
| `B_CA020` | 10 | 15 (42) | 1107 / 4 / 5051 | 103 | 7278 w | 1.222 objentry_word0, 2.29 attack_flag_8_or_10 | 3 | ca01 ca07 ca09 ca12 |
| `B_CA030` | 6 | 13 (24) | 1108 / 4 / 5052 | 104 | 7335 w | 1.218 set_spawn_param, 1.222 objentry_word0, 2.29 attack_flag_8_or_10 | 3 | ca01 ca07 ca13 ca19 |
| `B_LK110_PHANTOM` | 5 | 5 (11) | 2099 / 4 / 5026 | 163 | 4034 w | - | 5 | lk03 lk04 lk05 lk07 |
| `B_CA000` | 3 | 3 (3) | 358 / 4 / 2009 | ? | 3878 w | 0.32 fader_out, 1.250 set_scale_250 | 3 | ca10 eh02 eh03 |
| `B_EX210_EH` | 1 | 1 (1) | 2286 / 4 / 0 | - | 2438 w | - | 0 | eh24 |
| `B_EX350` | 1 | 2 (2) | 2226 / 4 / 0 | - | none | - | 2 | eh22 |
| `B_MU130` | 1 | 1 (2) | 721 / 4 / 0 | - | none | - | 1 | mu08 |
| `N_EX610_BTL` | 1 | 1 (1) | 2156 / 4 / 5058 | ? | 6982 w | - | 1 | tt05 |
| `N_EX610_BTL2` | 1 | 1 (14) | 2256 / 4 / 5058 | ? | 6982 w | - | 1 | tt36 |
| `N_AL070_BTL` | 1 | 3 (3) | 1593 / 4 / 0 | - | 5226 w | - | 3 | al14 |
| `P_EX130` | 1 | 1 (1) | 1876 / 4 / 14 | ? | 4488 w | - | 1 | tt10 |

#### Next twelve kinds, in order

Order = areas unlocked per unit of work: no missing host call first, then one cheap call, groups that become fully playable, small scripts, and early worlds on ties. Each recipe is the harness of the EN10 and EN11 briefs: `KH2_PROGRAMS=0,<btl program hex>,0 KH2_WAVE_GROUP=<tag> KH2_WAVE_AT_GROUP=1 KH2_NO_PARTY=1 KH2_AREALOG=1 KH2_BDXLOG=1 KH2_HITLOG=1 KH2_NO_SOUND=1 KH2_NO_MUSIC=1 kh2-game <area>` through `local/capped.ps1 -CapMB 2500`, J presses from about 10 s, a 90 s soak. None of the twelve tags holds a player entry (read from the spawn files), so `KH2_WAVE_AT_GROUP=1` puts Sora at the centre of the forced group. Where a tag holds several kind-2 groups the table names the group that carries the kind and the centre of its first volume in app metres (x, z; the "volume k centre" of `KH2_PROGRAMS=0,<btl>,0 kh2 coct-walls <area>` in game units divided by 100), to use as `KH2_WAVE_AT_GROUP=x,z`. That command also reported, for each centre, whether a party wall lies on the straight path from entrance 0; "clear" below means none did.

| # | kind | area / btl program / tag | kind-2 group (id: members; centre x,z) | enmp | missing host calls | note |
|---|---|---|---|---|---|---|
| 1 | `M_EX900` | tt19 / 1 / b_00 | 299: 3 `M_EX900`; (-2, 0) clear | 45 | none | done in WP-EN12a: in `KINDS`, 117 s soak and a kill in tt03 b_40 (docs/systems/script-host/contract.md) |
| 2 | `M_EX130` | bb13 / a / b_20 | 13: 4 `M_EX130`, 3 `M_EX420`; (0, 4) clear | 2 | none | ObjectType 21; 21 areas; 26 sole groups |
| 3 | `M_EX710` | tt39 / 4 / b_10 | 156: 2 `M_EX710`; (0, -6) clear (the volume centre has y 4013: the room lies far above the entrances) | 11 | 1.247 `random_free_point` (an Approx port exists for party members: `party_rules::free_point`), 2.36 (empty function) | ObjectType 21; the widest kind (28 areas, 37 sole groups); shares 11 groups with `M_EX130` |
| 4 | `M_EX890` | tt29 / 7 / b_21 | 145: 4 `M_EX890`; (1, 0) clear | 44 | none | ObjectType 21; Twilight Town (tt08, tt17, tt29, tt36), Beast's Castle, eh07; 9 groups, all sole |
| 5 | `M_EX680` | hb04 / a / b_20 | 126: 1 `M_EX680`, 2 `M_EX420`; (2, 20) clear; group 125 (3 `M_EX770`, 1 `M_EX680`) has centre (1, -1) with a wall on the path | 66 | none | ObjectType 21; 12124 words, the biggest of the list; 10 areas (Hollow Bastion, Space Paranoids, Coliseum) |
| 6 | `M_EX570` | mu01 / 2 / b_01 | 70: 3 `M_EX570`; (6, 5), two party walls on the path from both entrances | 60 | none | ObjectType 21; Land of Dragons; stands with `M_EX580` in 9 groups |
| 7 | `M_EX580` | mu01 / 1 / b_00 | 59: 1 `M_EX580`, 3 `M_EX020`; (-20, 0) clear | 61 | none | 7 areas, 24 groups, 138 members; 4331 words |
| 8 | `M_EX780` | lk03 / 1 / b_00 | 33: 3 `M_EX780`; (-11, -30) clear | 81 | none | Pride Lands (lk03, lk04, lk05, lk07, lk08); shares groups with `M_EX670` and `M_EX720` |
| 9 | `M_EX740` | he16 / a / b_20 | 59: 3 `M_EX740`, 2 `M_EX420`; (-11, 5), a party wall on the path | 13 | none | Coliseum; 4 areas, but he15 holds groups of 9 and 15 of it with `M_EX420` and `M_EX730` (large waves: soak) |
| 10 | `M_EX550` | he11 / a / b_20 | 84: 4 `M_EX550`; (17, 7), a party wall on the path (group 83: 7 `M_EX700`, 1 `M_EX550`, centre (-2, 7) clear) | 58 | 1.52 `bits_47c_set` (bit array at object +0x47c; the app has 1.54 as a Stub, reader FUN_0019a6a0 of the collision parts) | 7 areas (Disney Castle, Coliseum) |
| 11 | `M_EX500` | he11 / 1 / b_00 | 71: 3 `M_EX500`; (-2, 7) clear | 54 | 1.125 `tween_start` (linear ramp of a value pair at object +0x8c8, FUN_001abf40; `M_EX500_NM` needs it too) | 5 areas in the Coliseum |
| 12 | `M_EX590` | bb08 / 23 / b_70 | 1381: 1 `M_EX590`, 4 `M_EX760`; (8, 24) clear (group 1349 is a kind-1 group of 4 `M_EX590` with no volume) | 4 | none (1.311 has an arm) | 6 areas (al01, bb08, he09, mu02, nm02, nm06); `enmp` 4 is the literal pushed at 0x07b3 |

After these twelve, in rough order: the same-script variants as one batch (`M_EX010_WI`, `_NM`, `_TR` = Soldier; `M_EX020_NM`, `_WI` = Shadow; `M_EX050_WI`, `M_EX120_NM`, `_TR`, `M_EX420_NM`, `M_EX560_WI`, `M_EX650_TR`, `M_EX660_WI`, `M_EX750_NM`, `M_EX760_NM`, `M_EX770_TR`, `M_AL020_FIRE_RAW`, `M_AL020_ICEE_RAW`: a model and an objentry row each, but every model's motion set and the world's own groups must be looked at); `M_EX610` (hb09, hb16, hb18, tr01, tr04, tr09; no missing call; most of its groups also hold `M_EX680` or `M_EX710`, so it follows items 3 and 5); `M_BB010_AX` and `M_BB010_SWORD` (Beast's Castle; 1.52, 1.107); `M_EX720` and `M_EX670` (Pride Lands; 1.218, 1.270); `M_EX530` (16640 words; 1.247, 1.248); the Timeless River, Halloween Town and Space Paranoids sets; `B_LK110_PHANTOM` as the first boss.

Not verified: no run of any kind above was made (docs only); the `enmp` rows are static reads; the "missing host calls" column is a grep of `enemy_host.rs`; boss versus enemy is by name prefix and NeoStatus (5xxx), [inferred]; names such as "Phantom" and "Port Royal" come from model names and area prefixes, not from data the app reads.

### 2. Missing map files

23 areas (hb21 to hb40 are 20 of them) have no map. The loader keeps the previous world; what the original draws there is unknown.

### 3. Host calls (Stub/Approx, never Missing)

No run printed `unimplemented host call`. Stub calls first-used by the app's enemy kinds: 1.105 ClearFlag768 (122 areas); 1.121 SetTargetMode (122 areas); 1.171 GlobalFlag4 (122 areas); 1.206 GlobalFlag80 (122 areas); 1.39 SpawnParam (122 areas); 0.79 ReleaseEffect (77 areas); 65535.0 ModeFrozen (32 areas). They appear wherever a Shadow, Soldier or Dusk spawns, so they are not area-specific. Friend scripts add `65535.0 ModeFrozen` (hit stop).

### 4. Area-script opcode 0x16 `load_file`

Read but not ported in hb05, tr00, tr05, tr08, tt06, tt07, wm00. Effect unknown.

## Per world

Columns: map = meshes/collision triangles or none; groups = groups the area script loads; present = enemy kinds the app has among the area's groups; absent = distinct skipped kinds.

### es

| area | map | groups | present | absent | notes |
|---|---|---|---|---|---|
| es00 | 1414/138 | 0 | - | 0 |  |
| es01 | 1/2 | 0 | - | 0 |  |

Absent kinds in this world: none.

### tt Twilight Town

| area | map | groups | present | absent | notes |
|---|---|---|---|---|---|
| tt00 | 97/16 | 1 | - | 0 |  |
| tt01 | 1809/57 | 1 | - | 0 |  |
| tt02 | 980/1418 | 5 | - | 0 |  |
| tt03 | 1449/1478 | 8 | Dusk, Seifer | 3 |  |
| tt04 | 1394/1663 | 10 | Dancer, Dusk, M_EX940, N_EX500, N_EX600, Seifer | 0 |  |
| tt05 | 2389/1948 | 9 | Dusk, N_EX500, N_EX600 | 1 |  |
| tt06 | 2420/1693 | 11 | Dusk | 3 | opcode 0x16 not ported |
| tt07 | 1902/3701 | 12 | Dancer, Dusk | 3 | opcode 0x16 not ported |
| tt08 | 1666/690 | 6 | Dusk | 1 |  |
| tt09 | 2085/859 | 16 | - | 0 |  |
| tt10 | 2300/3899 | 7 | Dusk, M_EX940 | 1 |  |
| tt11 | 1543/1118 | 7 | - | 0 |  |
| tt12 | 2263/1746 | 5 | Dusk | 1 |  |
| tt13 | 1161/3694 | 5 | Dusk, Shadow, Soldier | 6 |  |
| tt14 | 1959/1606 | 5 | Dusk, Shadow | 4 |  |
| tt15 | 1054/1895 | 8 | Dusk | 3 |  |
| tt16 | 649/846 | 4 | - | 3 |  |
| tt17 | 2176/765 | 6 | Dusk | 0 |  |
| tt18 | 205/413 | 4 | - | 0 |  |
| tt19 | 798/412 | 4 | Dusk | 2 |  |
| tt20 | 792/132 | 1 | - | 0 |  |
| tt21 | 1189/1178 | 8 | - | 0 |  |
| tt22 | 544/440 | 7 | Dancer, Dusk | 0 |  |
| tt23 | 1460/479 | 3 | - | 0 |  |
| tt24 | 1000/96 | 1 | - | 0 |  |
| tt25 | 1462/2024 | 5 | Dusk, Shadow, Soldier | 5 |  |
| tt26 | 442/647 | 5 | - | 0 |  |
| tt27 | 1218/878 | 8 | - | 0 |  |
| tt28 | 1305/997 | 4 | - | 0 |  |
| tt29 | 2114/492 | 4 | Shadow | 3 |  |
| tt30 | 2328/566 | 4 | Shadow, Soldier | 3 |  |
| tt31 | 1238/1718 | 3 | Dusk, Shadow | 3 |  |
| tt32 | 483/97 | 5 | Dusk | 0 |  |
| tt33 | 1132/653 | 4 | Dusk | 0 |  |
| tt34 | 728/98 | 1 | - | 0 |  |
| tt35 | 1004/106 | 1 | - | 0 |  |
| tt36 | 485/1014 | 6 | Dusk, Soldier | 3 |  |
| tt37 | 2060/1769 | 9 | Dusk, Soldier | 4 |  |
| tt38 | 1248/1718 | 3 | Dancer, Dusk, Shadow | 2 |  |
| tt39 | 1154/1718 | 3 | Dusk, M_EX940, Shadow, Soldier | 1 |  |
| tt40 | 28/77 | 3 | Dusk | 1 |  |
| tt41 | 545/1603 | 1 | Dusk | 2 |  |

Absent kinds in this world: M_EX900, M_EX880, M_EX950, M_EX910, M_EX700, M_EX420, M_EX750, M_EX650, M_EX920, M_EX770, M_EX760, N_EX610_BTL, F_PO090_ETC, F_PO090_TT, P_EX130, M_EX020_RAW, M_EX730, N_EX610_BTL2, M_EX740.

### di

| area | map | groups | present | absent | notes |
|---|---|---|---|---|---|
| di00 | 1639/4955 | 1 | - | 0 |  |
| di01 | 2106/380 | 1 | - | 0 |  |
| di02 | 2021/2987 | 1 | - | 0 |  |

Absent kinds in this world: none.

### hb Hollow Bastion

| area | map | groups | present | absent | notes |
|---|---|---|---|---|---|
| hb00 | 1292/4 | 1 | - | 0 |  |
| hb01 | 586/511 | 4 | - | 1 |  |
| hb02 | 639/646 | 3 | - | 3 |  |
| hb03 | 871/1786 | 5 | - | 0 |  |
| hb04 | 1279/3854 | 3 | - | 2 |  |
| hb05 | 2682/2160 | 15 | - | 0 | opcode 0x16 not ported |
| hb06 | 1780/2857 | 9 | - | 0 |  |
| hb07 | 1500/4113 | 4 | Soldier | 0 |  |
| hb08 | 1923/1372 | 5 | Dusk, Shadow, Soldier | 3 |  |
| hb09 | 1521/4318 | 9 | Dusk, Shadow, Soldier | 4 |  |
| hb10 | 1538/2795 | 0 | - | 0 | WARN kh2_app::area: area hb10: ard/hb10.ard not readable; no area script runs |
| hb11 | 2002/4618 | 4 | Shadow, Soldier | 4 |  |
| hb12 | 1326/769 | 3 | - | 0 |  |
| hb13 | 1074/1768 | 8 | - | 0 |  |
| hb14 | 971/2 | 1 | - | 0 |  |
| hb15 | 1123/3 | 1 | - | 0 |  |
| hb16 | 1693/3320 | 3 | Shadow, Soldier | 4 |  |
| hb17 | 838/838 | 1 | - | 2 |  |
| hb18 | 1817/5377 | 4 | Dancer | 4 |  |
| hb19 | 2199/4138 | 4 | Soldier | 3 |  |
| hb20 | 842/1572 | 1 | Dancer, Dusk | 1 |  |
| hb21 | none | 0 | - | 0 | WARN kh2_app::assets: ard/hb21.ard: not in archive: ard/hb21.ard |
| hb22 | none | 0 | - | 0 | WARN kh2_app::assets: ard/hb22.ard: not in archive: ard/hb22.ard |
| hb23 | none | 0 | - | 0 | WARN kh2_app::assets: ard/hb23.ard: not in archive: ard/hb23.ard |
| hb24 | none | 0 | - | 0 | WARN kh2_app::assets: ard/hb24.ard: not in archive: ard/hb24.ard |
| hb25 | none | 0 | - | 0 | WARN kh2_app::assets: ard/hb25.ard: not in archive: ard/hb25.ard |
| hb26 | none | 0 | - | 0 | WARN kh2_app::assets: ard/hb26.ard: not in archive: ard/hb26.ard |
| hb27 | none | 0 | - | 0 | WARN kh2_app::assets: ard/hb27.ard: not in archive: ard/hb27.ard |
| hb28 | none | 0 | - | 0 | WARN kh2_app::assets: ard/hb28.ard: not in archive: ard/hb28.ard |
| hb29 | none | 0 | - | 0 | WARN kh2_app::assets: ard/hb29.ard: not in archive: ard/hb29.ard |
| hb30 | none | 0 | - | 0 | WARN kh2_app::assets: ard/hb30.ard: not in archive: ard/hb30.ard |
| hb31 | none | 0 | - | 0 | WARN kh2_app::assets: ard/hb31.ard: not in archive: ard/hb31.ard |
| hb32 | none | 0 | - | 0 | WARN kh2_app::assets: ard/hb32.ard: not in archive: ard/hb32.ard |
| hb33 | none | 0 | - | 0 | WARN kh2_app::assets: ard/hb33.ard: not in archive: ard/hb33.ard |
| hb34 | none | 0 | - | 0 | WARN kh2_app::assets: ard/hb34.ard: not in archive: ard/hb34.ard |
| hb35 | none | 0 | - | 0 | WARN kh2_app::assets: ard/hb35.ard: not in archive: ard/hb35.ard |
| hb36 | none | 0 | - | 0 | WARN kh2_app::assets: ard/hb36.ard: not in archive: ard/hb36.ard |
| hb37 | none | 0 | - | 0 | WARN kh2_app::assets: ard/hb37.ard: not in archive: ard/hb37.ard |
| hb38 | none | 0 | - | 0 | WARN kh2_app::assets: ard/hb38.ard: not in archive: ard/hb38.ard |
| hb39 | none | 0 | - | 0 | WARN kh2_app::assets: ard/hb39.ard: not in archive: ard/hb39.ard |
| hb40 | none | 0 | - | 0 | WARN kh2_app::assets: ard/hb40.ard: not in archive: ard/hb40.ard |

Absent kinds in this world: M_EX760, M_EX770, M_EX420, M_EX600, M_EX610, M_EX910, M_EX880, M_EX530, M_EX020_RAW, M_EX770_RAW, M_EX760_RAW.

### bb Beast Castle

| area | map | groups | present | absent | notes |
|---|---|---|---|---|---|
| bb00 | 1445/1550 | 10 | Dusk | 5 |  |
| bb01 | 494/395 | 5 | - | 1 |  |
| bb02 | 256/193 | 5 | - | 0 |  |
| bb03 | 1329/917 | 9 | - | 0 |  |
| bb04 | 913/1349 | 3 | Dusk, Shadow | 2 |  |
| bb05 | 639/1014 | 4 | - | 0 |  |
| bb06 | 1906/1713 | 4 | Shadow, Soldier | 4 |  |
| bb07 | 1325/424 | 4 | Shadow | 4 |  |
| bb08 | 1031/2063 | 10 | Shadow, Soldier | 4 |  |
| bb09 | 975/416 | 5 | - | 4 |  |
| bb10 | 88/123 | 5 | - | 0 |  |
| bb11 | 541/548 | 7 | - | 4 |  |
| bb12 | 393/455 | 8 | Shadow | 3 |  |
| bb13 | 1931/965 | 3 | - | 1 |  |
| bb14 | 1254/5 | 1 | - | 0 |  |
| bb15 | 1850/940 | 1 | - | 0 |  |

Absent kinds in this world: M_EX520, M_BB010_SWORD, M_BB010_AX, M_EX050, M_EX420, M_EX560, M_EX690, M_EX020_RAW, M_EX760, M_EX590.

### he Olympus Coliseum

| area | map | groups | present | absent | notes |
|---|---|---|---|---|---|
| he00 | 419/311 | 4 | - | 2 |  |
| he01 | 774/1503 | 5 | - | 0 |  |
| he02 | 916/1673 | 5 | - | 0 |  |
| he03 | 594/1170 | 7 | - | 0 |  |
| he04 | 145/167 | 6 | - | 0 |  |
| he05 | 374/1087 | 4 | Shadow | 6 |  |
| he06 | 672/1004 | 4 | Dusk | 0 |  |
| he07 | 348/1085 | 5 | - | 5 |  |
| he08 | 169/85 | 1 | Shadow, Soldier | 2 |  |
| he09 | 830/718 | 1 | Dancer, Dusk, M_EX940, Shadow, Soldier | 37 |  |
| he10 | 175/614 | 7 | - | 0 |  |
| he11 | 527/1172 | 6 | - | 3 |  |
| he12 | 313/938 | 8 | - | 0 |  |
| he13 | 301/25 | 1 | - | 0 |  |
| he14 | 920/1568 | 1 | - | 0 |  |
| he15 | 375/1628 | 6 | Shadow | 6 |  |
| he16 | 633/2003 | 6 | Soldier | 5 |  |
| he17 | 102/1069 | 6 | - | 2 |  |
| he18 | 957/1673 | 1 | - | 0 |  |
| he19 | 191/426 | 1 | - | 0 |  |

Absent kinds in this world: M_EX050, M_EX500, M_EX550, M_EX700, M_EX750, M_EX520, M_EX690, M_EX510, M_EX730, M_EX740, M_AL020_FIRE, M_AL020_ICEE, M_EX420, M_EX880, M_EX920, M_EX200, M_EX580, M_EX660, M_EX950, M_EX600, M_EX660_RAW, M_EX540, M_EX060, M_EX630, M_AL020_FIRE_RAW, M_EX560, M_EX650, M_EX210, M_EX590, M_EX760, M_EX770, M_BB010_SWORD, M_EX620, M_AL020_ICEE_RAW, M_BB010_AX, M_EX900, M_EX910, M_EX110.

### al Agrabah

| area | map | groups | present | absent | notes |
|---|---|---|---|---|---|
| al00 | 1577/2956 | 13 | Shadow, Soldier | 6 |  |
| al01 | 957/1222 | 7 | Soldier | 8 |  |
| al02 | 1101/342 | 5 | - | 0 |  |
| al03 | 434/1185 | 5 | Shadow | 2 |  |
| al04 | 1085/569 | 1 | - | 0 |  |
| al05 | 2733/180 | 2 | - | 0 |  |
| al06 | 203/931 | 8 | - | 0 |  |
| al07 | 891/2422 | 4 | Shadow, Soldier | 4 |  |
| al08 | none | 4 | - | 0 |  |
| al09 | 429/1337 | 6 | - | 0 |  |
| al10 | 648/2436 | 4 | - | 6 |  |
| al11 | 1311/2192 | 6 | - | 0 |  |
| al12 | 779/2012 | 4 | Shadow, Soldier | 6 |  |
| al13 | 715/530 | 13 | - | 6 |  |
| al14 | 1705/8789 | 5 | - | 9 |  |
| al15 | 1218/515 | 3 | - | 0 |  |

Absent kinds in this world: M_EX110, M_EX620, M_EX060, M_AL020_FIRE, M_AL020_ICEE, M_EX120, M_EX630, M_EX420, M_EX590, M_AL020_ICEE_RAW, M_AL020_FIRE_RAW, N_AL070_BTL, M_EX660, M_EX520, M_EX520_AL, M_EX620_AL, M_EX130_AL, M_EX660_AL.

### mu Land of Dragons

| area | map | groups | present | absent | notes |
|---|---|---|---|---|---|
| mu00 | 1211/322 | 6 | - | 0 |  |
| mu01 | 884/1652 | 6 | Shadow, Soldier | 4 |  |
| mu02 | 1540/1885 | 8 | Shadow, Soldier | 4 |  |
| mu03 | 1930/2270 | 5 | Shadow, Soldier | 3 |  |
| mu04 | 923/3770 | 6 | - | 0 |  |
| mu05 | 274/613 | 6 | - | 5 |  |
| mu06 | 197/1388 | 6 | Dusk, Shadow | 3 |  |
| mu07 | 688/955 | 4 | Shadow | 3 |  |
| mu08 | 306/268 | 4 | - | 3 |  |
| mu09 | 710/588 | 4 | - | 1 |  |
| mu10 | 203/395 | 5 | Dusk | 1 |  |
| mu11 | 331/711 | 5 | - | 0 |  |
| mu12 | 758/3238 | 6 | - | 0 |  |

Absent kinds in this world: M_EX580, M_EX660, M_EX420, M_EX210, M_EX120, M_EX590, M_EX520, M_EX020_RAW, M_EX660_RAW, M_EX650, B_MU130, M_EX920.

### po 100 Acre Wood

| area | map | groups | present | absent | notes |
|---|---|---|---|---|---|
| po00 | 879/247 | 11 | - | 0 |  |
| po01 | 1218/2141 | 6 | - | 0 |  |
| po02 | 826/2265 | 6 | - | 0 |  |
| po03 | 1176/3971 | 7 | - | 0 |  |
| po04 | 2831/2858 | 6 | - | 0 |  |
| po05 | 869/2897 | 6 | - | 0 |  |
| po06 | 624/41 | 1 | - | 0 |  |
| po07 | 3308/10413 | 1 | - | 0 |  |
| po08 | 1576/309 | 1 | - | 0 |  |
| po09 | 638/3049 | 6 | - | 1 |  |
| po10 | none | 2 | - | 0 |  |

Absent kinds in this world: F_PO090.

### lk Pride Lands

| area | map | groups | present | absent | notes |
|---|---|---|---|---|---|
| lk00 | 1190/6679 | 5 | - | 0 |  |
| lk01 | 1409/1661 | 4 | - | 0 |  |
| lk02 | 1326/3908 | 4 | - | 0 |  |
| lk03 | 757/6762 | 4 | Shadow, Soldier | 7 |  |
| lk04 | 1724/8636 | 5 | Shadow, Soldier | 5 |  |
| lk05 | 741/7195 | 5 | Shadow, Soldier | 5 |  |
| lk06 | 930/1591 | 5 | - | 0 |  |
| lk07 | 1145/6026 | 4 | Soldier | 5 |  |
| lk08 | 1295/4001 | 4 | Shadow, Soldier | 5 |  |
| lk09 | 1919/3096 | 5 | - | 0 |  |
| lk10 | 1307/393 | 1 | - | 0 |  |
| lk11 | 913/652 | 1 | - | 0 |  |
| lk12 | 881/194 | 1 | - | 0 |  |
| lk13 | 1923/1678 | 2 | - | 1 |  |
| lk14 | 1815/650 | 1 | - | 0 |  |
| lk15 | 1632/7154 | 1 | - | 0 |  |
| lk16 | 375/1531 | 1 | - | 0 |  |

Absent kinds in this world: M_EX720, M_EX420, B_LK110_PHANTOM, M_EX780, M_EX110, M_EX760, M_EX020_RAW, M_EX660, M_EX660_RAW.

### lm Atlantica

| area | map | groups | present | absent | notes |
|---|---|---|---|---|---|
| lm00 | 1779/576 | 3 | - | 0 |  |
| lm01 | 1443/614 | 1 | - | 0 |  |
| lm02 | 1185/1493 | 5 | - | 0 |  |
| lm03 | 1123/1619 | 1 | - | 0 |  |
| lm04 | 1375/1095 | 1 | - | 0 |  |
| lm05 | 1159/5518 | 1 | - | 0 |  |
| lm06 | 1339/863 | 1 | - | 0 |  |
| lm07 | 1699/598 | 1 | - | 0 |  |
| lm08 | 1363/613 | 1 | - | 0 |  |
| lm09 | 2606/388 | 1 | - | 0 |  |
| lm10 | 592/38 | 1 | - | 0 |  |

Absent kinds in this world: none.

### dc Disney Castle

| area | map | groups | present | absent | notes |
|---|---|---|---|---|---|
| dc00 | 1164/2554 | 6 | - | 1 |  |
| dc01 | 1335/356 | 5 | - | 0 |  |
| dc02 | 995/1140 | 5 | Shadow | 2 |  |
| dc03 | 1771/5490 | 5 | Shadow, Soldier | 1 |  |
| dc04 | 479/866 | 6 | - | 0 |  |
| dc05 | 608/1351 | 7 | - | 0 |  |
| dc06 | 705/725 | 5 | - | 0 |  |
| dc07 | none | 0 | - | 0 |  |

Absent kinds in this world: M_EX550, M_EX020_RAW.

### wi Timeless River

| area | map | groups | present | absent | notes |
|---|---|---|---|---|---|
| wi00 | 933/1352 | 18 | - | 0 |  |
| wi01 | 1490/781 | 6 | - | 4 |  |
| wi02 | 733/1227 | 6 | - | 6 |  |
| wi03 | 1864/1831 | 4 | - | 5 |  |
| wi04 | 877/1040 | 10 | - | 4 |  |
| wi05 | 2027/268 | 4 | - | 5 |  |
| wi06 | 2137/836 | 5 | - | 4 |  |
| wi07 | 804/645 | 5 | - | 5 |  |
| wi08 | 335/4 | 0 | - | 0 |  |

Absent kinds in this world: M_EX540_WI, M_EX020_WI, M_EX660_WI, M_EX550_WI, M_EX010_WI, M_EX050_WI, M_EX560_WI, F_WI310, F_WI000.

### nm Halloween Town

| area | map | groups | present | absent | notes |
|---|---|---|---|---|---|
| nm00 | 1384/2148 | 6 | - | 5 |  |
| nm01 | 1203/1326 | 6 | - | 0 |  |
| nm02 | 977/2554 | 8 | - | 8 |  |
| nm03 | 1384/1782 | 2 | - | 5 |  |
| nm04 | 1482/1422 | 4 | - | 7 |  |
| nm05 | 550/473 | 5 | - | 0 |  |
| nm06 | 478/1548 | 8 | - | 8 |  |
| nm07 | 668/594 | 2 | - | 0 |  |
| nm08 | 1022/385 | 6 | - | 0 |  |
| nm09 | 1238/704 | 5 | - | 0 |  |
| nm10 | 844/662 | 4 | - | 0 |  |

Absent kinds in this world: M_EX760_NM, M_EX200_NM, M_EX500_NM, M_EX120_NM, M_EX020_NM, M_EX010_NM, M_EX420_NM, M_EX700_NM, M_EX420, M_EX590, M_EX750_NM.

### wm World Map

| area | map | groups | present | absent | notes |
|---|---|---|---|---|---|
| wm00 | 1185/1730 | 4 | - | 0 | opcode 0x16 not ported; WORLDMAP manager (gap); WARN kh2_app::worldmap: worldmap: obj/WM_GUMI_01.mdlx: holds no model block (col |

Absent kinds in this world: none.

### ca Port Royal

| area | map | groups | present | absent | notes |
|---|---|---|---|---|---|
| ca00 | 1772/606 | 8 | - | 0 |  |
| ca01 | 1047/1552 | 7 | Dusk, Shadow | 7 |  |
| ca02 | 1373/2065 | 6 | Shadow, Soldier | 6 |  |
| ca03 | 1184/1139 | 4 | - | 3 |  |
| ca04 | 1222/437 | 4 | - | 0 |  |
| ca05 | 1393/1530 | 4 | - | 1 |  |
| ca06 | 217/856 | 5 | - | 0 |  |
| ca07 | 1759/1098 | 2 | - | 3 |  |
| ca08 | 2064/1163 | 5 | - | 0 |  |
| ca09 | 472/902 | 4 | - | 8 |  |
| ca10 | 981/1764 | 3 | Shadow | 5 |  |
| ca11 | 1742/452 | 6 | - | 0 |  |
| ca12 | 1596/2604 | 4 | - | 6 |  |
| ca13 | 516/1888 | 4 | Soldier | 11 |  |
| ca14 | 1085/1983 | 5 | - | 5 |  |
| ca15 | 1100/1591 | 5 | - | 5 |  |
| ca16 | 2041/1045 | 4 | - | 0 |  |
| ca17 | 1946/1806 | 1 | - | 0 |  |
| ca18 | 1373/1530 | 2 | Shadow | 4 |  |
| ca19 | 1373/1530 | 2 | Soldier | 6 |  |
| ca20 | 1373/1530 | 2 | - | 3 |  |
| ca21 | 1184/1137 | 2 | - | 5 |  |
| ca22 | 1184/1137 | 2 | - | 5 |  |
| ca23 | 1676/856 | 1 | - | 0 |  |
| ca24 | 1477/1517 | 2 | - | 0 |  |
| ca25 | 2595/1163 | 1 | - | 0 |  |

Absent kinds in this world: B_CA040, B_CA020, M_EX650, M_EX210, B_CA030, M_EX510, M_EX730, M_EX950, M_EX420, M_EX690, M_EX760, M_EX660, M_EX770, M_EX560, B_CA000.

### tr Space Paranoids

| area | map | groups | present | absent | notes |
|---|---|---|---|---|---|
| tr00 | 1311/486 | 7 | - | 0 | opcode 0x16 not ported; WARN kh2_app::area_jump: jump: world 17 area 1 is not in the app; staying |
| tr01 | 590/867 | 7 | Shadow | 3 |  |
| tr02 | 733/111 | 3 | - | 5 |  |
| tr03 | 438/488 | 5 | - | 3 |  |
| tr04 | 768/552 | 6 | - | 5 |  |
| tr05 | 442/961 | 9 | - | 0 | opcode 0x16 not ported |
| tr06 | 1297/429 | 3 | - | 4 |  |
| tr07 | 126/258 | 2 | - | 2 |  |
| tr08 | 1215/396 | 10 | - | 0 | opcode 0x16 not ported |
| tr09 | 1096/120 | 3 | - | 1 |  |
| tr10 | 1305/429 | 4 | - | 0 |  |
| tr11 | 1235/396 | 5 | - | 0 |  |

Absent kinds in this world: M_EX600, M_EX610, M_EX650_TR, M_EX010_TR, M_EX610_RAW, M_EX770_TR, M_EX120_TR, M_EX600_LC_GRD, M_EX600_LC_ATK, M_EX600_LC_CHG, M_EX600_LC, M_EX530_TR.

### eh The World That Never Was

| area | map | groups | present | absent | notes |
|---|---|---|---|---|---|
| eh00 | 560/3176 | 1 | - | 0 |  |
| eh01 | 1637/114 | 5 | - | 0 |  |
| eh02 | 1516/3875 | 4 | Shadow | 3 |  |
| eh03 | 1162/3408 | 6 | Shadow | 2 |  |
| eh04 | 900/1232 | 6 | - | 0 |  |
| eh05 | 962/66 | 1 | - | 0 |  |
| eh06 | 1868/1481 | 5 | - | 2 |  |
| eh07 | 1976/312 | 6 | - | 0 |  |
| eh08 | 1976/312 | 6 | - | 1 |  |
| eh09 | 2054/615 | 5 | - | 0 |  |
| eh10 | 2092/1247 | 4 | - | 1 |  |
| eh11 | 2092/1247 | 3 | - | 2 |  |
| eh12 | 1918/889 | 4 | M_EX940 | 2 |  |
| eh13 | 1295/1935 | 8 | - | 0 |  |
| eh14 | 1699/700 | 3 | Dusk | 1 |  |
| eh15 | 1017/1386 | 3 | Dusk, M_EX940 | 0 |  |
| eh16 | 1554/591 | 4 | Dancer, M_EX940 | 1 |  |
| eh17 | 1589/2108 | 5 | - | 2 |  |
| eh18 | 1583/1538 | 2 | - | 0 |  |
| eh19 | 1173/1740 | 2 | - | 0 |  |
| eh20 | 1331/144 | 2 | - | 0 |  |
| eh21 | 728/98 | 1 | - | 0 |  |
| eh22 | 1931/2 | 1 | - | 5 |  |
| eh23 | 1696/595 | 2 | - | 0 |  |
| eh24 | 1509/1667 | 1 | - | 2 |  |
| eh25 | 2230/215 | 2 | M_EX940 | 2 |  |
| eh26 | 1687/136 | 3 | - | 1 |  |
| eh27 | 1474/519 | 2 | - | 0 |  |
| eh28 | 2341/2292 | 2 | - | 0 |  |
| eh29 | 2879/1248 | 1 | - | 0 |  |

Absent kinds in this world: M_EX920, M_EX910, M_EX420, B_CA000, M_EX870, M_EX900, M_EX880, M_EX020_RAW, M_EX950, B_EX350, EH_G_EX290, EH_G_EX250_FLY, EH_G_EX320, EH_G_EX120, M_EX880_DANCER_EH, B_EX210_EH, EH_G_EX250.

## Not verified

- Screenshots were not opened: whether a map renders correctly is unknown. Audio not listened to, video not watched.
- Each run is 16 s of idle play. Scripts that need movement, trigger boxes or story flags were not driven; with default program bytes some areas load few or no groups, so enemy and host-call figures cover only what spawns at load.
- Per-run logs are not committed.
