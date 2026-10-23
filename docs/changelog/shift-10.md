## Shift 10 (2026-10-14/15)

- **WP-H1 / MN6 (text colour).** HUD and menu text use the exact glyph atlas (texel times vertex colour over 128 as the game does); colours above 0x80 no longer clamp to white; the per-bar compensation is gone. Verified by font and hud_text tests and before/after screenshots. Open: none.
- **WP-MN5 (menu gaps).** Preview fade follows the panel outline alpha; Pride Lands slot height rule (FUN_00250a00 with FUN_0027c218); card digit sizes fitted to the clip. Preview feet height needed no change (the old reference scale was wrong).
- **WP-MN6 face rings.** HUD face rings draw above the menu dim and under the frame (camera order 12). Open: an explicit node flag instead of the structural rule.
- **WP-MN7 (Party page, member status).** Donald and Goofy status tabs and the Party page (pages 0x14/0x16, slot swap) from the game's tables. Open: page 0x15, hidden-member list, partner spawn in slot 3, persisting slot bytes across areas.
- **WP-S5 / S6 (party specials).** Goofy's blade (scripted hand object), turbo, Donald's blizzard hosts, the class-5 pull of tornado, the target search hosts 6.3/6.4/6.5 (`kh2_bdx::search`, oracle over 400 scenes). Open: docs/party.md section 17 gaps.
- **WP-E3 (talk and save points).** Talk is FUN_00177b18 case 0x1f (no event 0xb): libretto reader, ScriptWindow pages and speech tail; save points heal +1 HP/MP per frame. Open: reaction_cmd integration, menu kind 1, talk camera, persisting world-point bits.

