# Area survey: every loadable area run headless

Work package SV2, 2026-10-19. Written from run logs only; no game data (counts and kind names only). Gaps are gaps of the app, not bugs found in the original.

## Method

- Binary: `target\debug\kh2-game.exe <area>` of branch wp/SV2 with the collision fix below, one process at a time through `local/capped.ps1 -CapMB 3500`, a 150 s kill timeout per run, environment `KH2_NO_SOUND=1 KH2_NO_MUSIC=1 KH2_AREALOG=1 KH2_BDXLOG=1 KH2_SHOT_AT=16 KH2_SCREENSHOT=<png>`.
- Areas: every `ard/*.ard` of the disc index, 289 files (world prefixes as `area::WORLDS`). Each run lasts about 22 s: it loads, plays 16 s of game time, takes the screenshot and exits.
- Enemy kinds: the `area groups:` log line lists, per btl program group, the kinds the app has (`enemy::KINDS`) and those it skips ("not in the app"). For the survey only, a local uncommitted build also listed groups whose members were all skipped (the shipped code drops such groups from the log); that patch was reverted. Counts below are distinct kinds over all btl groups of the area, not only the groups the script loads.
- Screenshots were not looked at; "loads" means the log shows the map line (or the map warning) and the screenshot step ran.

## Result

- 289 of 289 areas run to the screenshot and exit 0 (after the fix). 0 panics, 0 hangs, 0 `unimplemented host call` lines.
- Before the fix, `eh22` and `eh29` were killed by the 3500 MB cap (`memory allocation of 2214592528 bytes failed`; capped.ps1 reports exit 1 with 'killed by the 3500 MB memory cap', 14 s and 11 s in). Reproduce on the commit before the fix: `powershell -NoProfile -ExecutionPolicy Bypass -File local/capped.ps1 -CapMB 3500 -Exe target\debug\kh2-game.exe -ArgLine eh22` with `KH2_ISO` set.
- 23 areas have no `map/us/<area>.map` on the disc and log the warning `not in archive` / `not found or not a map`: al08, dc07, hb21, hb22, hb23, hb24, hb25, hb26, hb27, hb28, hb29, hb30, hb31, hb32, hb33, hb34, hb35, hb36, hb37, hb38, hb39, hb40, po10. The area script and groups still run.
- The only area-script opcode read but not ported is 0x16 `load_file` (7 areas). The only battle manager seen is WORLDMAP (0xc, wm00), already a documented gap.

## Fix made

`crates/kh2-app/src/world.rs`: `World::push` put every triangle into a 400-unit cell grid, one entry per covered cell. `eh22` and `eh29` contain a floor millions of units wide (render bounds about 1.44 million either way), which needed over 2 GB of grid entries. A triangle that covers more than 1024 cells now goes to a `big` list that every `near` query tests directly. Ordinary maps are unchanged. Test: `a_floor_wider_than_the_grid_limit_is_found_without_grid_entries`. After the fix both areas load and exit 0 (21 s each).

## Collision groups (WP-IW1)

The COCT meshes carry a group number and the game only walks meshes whose group bit is in the area script's map visibility word (docs/movement.md section 8). `kh2 coct-walls <area>` lists the walls per group and which are off: eh10 (program 0,1,0) has 108 party-wall polygons in groups 3 to 8 that are off under word 0x407, he05 (word 0x3) 10 polygons in groups 2, 3, 12, 22, 32 and 62, al01 (word 0xc1ffffffff) none. The app made them all solid, which stopped Sora at the arena walls of eh10.

## Ranked gaps

### 1. Enemy kinds missing from `enemy::KINDS`

100 distinct kinds are skipped somewhere. Present kinds: Shadow (43 areas), Dusk, Soldier, Dancer, Seifer, N_EX500, N_EX600, M_EX940, M_EX850. Most common absent kinds by number of areas:

| kind | areas | example areas |
|---|---|---|
| M_EX420 | 33 | al00, al01, bb00, bb06, bb13, ca02 |
| M_EX760 | 15 | bb08, ca09, ca13, ca19, hb02, hb08 |
| B_CA040 | 14 | ca01, ca02, ca03, ca05, ca07, ca09 |
| M_EX950 | 14 | ca09, ca10, ca12, ca13, ca14, ca15 |
| M_EX650 | 13 | ca01, ca02, ca03, ca13, ca18, ca19 |
| M_EX880 | 13 | eh12, eh25, hb08, hb18, he09, tt03 |
| M_EX110 | 12 | al00, al01, al03, al07, al10, al12 |
| M_EX900 | 12 | eh11, eh25, he09, tt03, tt06, tt07 |
| M_EX660 | 11 | al14, ca14, ca15, ca20, he09, lk08 |
| M_EX520 | 11 | al14, bb00, bb04, bb06, bb09, bb11 |
| M_EX770 | 11 | ca01, ca02, hb01, hb02, hb04, hb11 |
| M_EX210 | 11 | ca02, ca03, ca09, ca12, ca13, ca19 |
| M_EX910 | 11 | eh08, eh12, eh16, hb08, hb09, he09 |
| M_EX690 | 10 | bb00, bb07, bb11, ca02, ca09, ca15 |
| B_CA020 | 10 | ca01, ca07, ca09, ca12, ca13, ca14 |
| M_EX730 | 10 | ca01, ca09, ca12, ca13, ca18, ca21 |
| M_EX050 | 9 | bb00, bb04, bb07, bb12, he05, he07 |
| M_EX510 | 9 | ca01, ca10, ca13, ca14, ca18, ca19 |
| M_EX600 | 9 | hb02, hb11, hb19, he09, tr02, tr03 |
| M_EX700 | 9 | he05, he07, he09, he11, tt13, tt25 |
| M_EX620 | 8 | al00, al01, al07, al10, al12, al13 |
| M_EX920 | 8 | eh06, eh10, eh11, eh17, he09, mu10 |
| M_EX060 | 7 | al00, al01, al10, al12, al13, al14 |
| M_EX120 | 7 | al01, al07, al12, al13, mu01, mu03 |
| M_EX020_RAW | 7 | bb01, dc02, eh02, hb16, lk03, mu05 |

Estimate: each kind needs its BDX script, motion set and host calls (docs/enemy-ai.md). The top ten appear in 11 to 33 areas each, so they unlock combat in most areas outside Twilight Town. B_CA* are Port Royal boss objects.

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
