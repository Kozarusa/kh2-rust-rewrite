## Shift 15 (2026-10-17)

**WP-PL2 (party follow-ups).** Continue no longer loses the allies' restored HP and MP to the live party sync; items heal a down member and revive him (FUN_001a15a0 has no KO test; vtable +0x78 FUN_00176c98 -> FUN_00186238 -> FUN_0017cee8). Drive Recovery and Dodge Roll findings recorded. Verified by four unit tests. Open: MP of an Elixir on a KO member (FUN_001a0210), the FUN_0017cee8 flag-8 branch.

**WP-MK3 (Mickey rescue, docs).** Control swap not ported (Mickey needs his own player body and command deck); HP 1 writer not found; searched functions listed in docs/menus.md 26.5. `RESCUE_PORTED` stays false.

**WP-RV2 (revive event, hosts, weapon words, docs).** Revive is event kind 0xf playing re_gameover-0a with a 100-tick fade (0x356b30); new host rows 0.75/0.76/0.61/0.98, 10.50/10.58, 1.52-1.54, 1.131, 1.301; Donald's and Goofy's weapon words are written by host 0.75 from 00common.bdx event 7 at boot. Open: what lifts the black after the revive.

**WP-SH6 (shop).** Count-up, tab grid with sums, row origins (window record not added [inferred]), and the `seqd` fix (load emptied layd.seqs). Verified by four tests and the moogle/shop screenshots. Open: group 0x65 words, row plates, page 10's second column.

**WP-GO2 (game over).** The scene model plays GAMEOVER.mset (FUN_001ade58/001add40); Continue runs the party refresher FUN_001abfe8(8): full HP and MP (replaces the area-start HP/MP reading); Load uses its own header layer set. Open: wait lengths, whether the field Sora is hidden.

**WP-LS2 (limit scripts).** Object memory for limit scripts, hosts 2.23/2.56/1.131 and requests for 2.47/2.50/2.59: all 15 limit scripts run to the end. Open: visible camera, effects, gauges (LS2b), 1.160, 6.57.

**WP-GO2b (game over follow-ups).** The list opens 122 ticks after the knock-out for types 0 and 3 (event kind 0xf fade, FUN_001ad9a0 step 2); a test for the whole Continue path and the ally refresh. Open: type 1's wait, window alpha (FUN_0026f948 group 0x11 trigger).

**WP-RV3 (revive timeline, weapon words).** `Revive` keeps the 42-tick camera event and the fade 22..122, Tinker Bell at tick 122; the 13 member and form weapon words come from 00common.bdx event 7 at boot (host 0.75/0.76) instead of two constants; host 1.301 answers 0. Verified by five tests.

**WP-RV4 (revive fade wiring).** The pad is locked for the 42 ticks and a white full-screen fade runs over ticks 22..122 (flag-1 fade, FUN_00226830 case 2 -> FUN_0014a7c8 -> FUN_0014a710; corrects RV2's black); the game over list uses the same white ramp instead of the black dim. Open: what lifts the fade after tick 122, linear ramp and blend [inferred].

**WP-LS2b (limit camera).** Host 2.50 plays the limit BAR's type 0x16 event (kind 0xb, camera track) through new `limit_camera.rs`; off by default (`KH2_LIMIT_CAMERA=1`) because participants have no models and the key frame is [inferred]. Verified by six tests. Open: 7.28 effect object, HUD gauge animation, hosts 1.160/6.57 (need a kh2-bdx accessor for script memory).

