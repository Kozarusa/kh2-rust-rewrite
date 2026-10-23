# UI: HUD, command menu, menus, shops, journal, save: contract

Ladder step L8. Ledger system name `ui`. Crates: `kh2-image` (`seqd.rs`, `layd.rs`, `imgd.rs`, `text.rs`, `font.rs`, `window.rs`: sequence, layout,
image, text and message-window readers), `kh2-app` (`hud*.rs`, `menu*.rs` and `menu_journal/`, `items.rs`, `savepoint.rs`).
Sheet ids: `ui.menu`, `ui.save`, `ui.load`, `ui.journal`, `ui.shop`, `ui.tutorial`, `ui.battle_report`, `game.items`, `game.gameover`.
Scenarios: `scenarios/ui/` (none yet).

Status: the field HUD (Sora's panel, target gauge, lock-on reticle, command menu, banners, INFORMATION bar, message and tutorial windows) is ported.
The pause menu (Status, Abilities, Items, Customize, Equipment, Config, Party, Help, NEW tags), Save and Load, Jiminy's Journal, shops with
synthesis, the Struggle's result menu, the BATTLE REPORT and the game over are ported in stages M1 to M8 and the work packages named in the files.
Still open: the title screen, new game, config at the title, journal lighting, and the footage-only items in `menus-pause.md` "7. Gaps".

## What the system does
- HUD: files `field2d/us/<world>0field.2dd` and `<world>0command.2dd` (BARs of IMGD + SEQD pairs), `obj/P_EX100.apdx` "face", `msg/us/sys.bar`
  and the font files; the sequence player rules; Sora's panel (class built at `0x156520`, per frame `0x157a50`), target gauge (class `0x1540a0`,
  update `0x154498`, hit `0x154758`, bind `0x19b4a8`), lock-on reticle, command menu, banners, text engine, message and tutorial windows.
- Command manager: the object per player whose vtable `0x35f260` (base `0x35f238`, constructor `FUN_001baf48`) drives the command lists (deck, magic,
  items, forms, targets), their builders, availability (`FUN_001ba2c0`), input and issue, and the item command (`FUN_001bc578`, `FUN_001ba9b0`,
  `FUN_001a12a0`).
- Pause (camp) menu: Start gate `FUN_00138758` -> `FUN_0024f0b8`, camp task `FUN_00250590`, the field is stopped (`schedule::Pause`), pages by id
  (0x14/0x16 Party, 0x17/0x18 Status, 0x19..0x1f Customize, 0x24 Config, 0x25 Help, ...), list results (-5 switch, -7 Help), system sounds from
  `se000.seb`.
- Menu kinds: 1 save point, 2 Load, 3 Jiminy's Journal (`jiminy.bar`), 4 shop, 7/8 Save (`save.2ld`), 0xe BATTLE REPORT, plus the Struggle's result
  menu (record row 0x1a) and the game over (alternate list with Mickey, Continue / Load Game).

## State and owners
| state | where in the original | owner in the port |
|---|---|---|
| command manager lists (stack at manager `+0x974`) | player command manager | `items.rs`, `hud_menu::CommandMenu` |
| option word, game mode, NEW tags, shop and journal records | save block (option word, `+0x1658`, record tables) | `Progress` |
| pause state | the camp task | `schedule::Pause`, `menu.rs` |
| menu sequences, page objects, node animation groups | the menu's `.2ld` layouts | `menu_gfx.rs`, `menu_*.rs` |
| drive gauge, MP charge, target bubble, banners | player `+0x1af..`, `+0x1bc`, owner `+0x588` bit 1 | `hud_gauge.rs`, `hud_target.rs`, `hud_banner.rs` |
| Journal counters (kills, treasures) | save-block counters; writers `FUN_00187e08(0x33fec0, NeoStatus)` | `menu_journal/kills.rs`, `Progress::count_*` |

## Entry points in the original
| address | what |
|---|---|
| `FUN_00138758`, `FUN_0024f0b8`, `FUN_00250590` | Start gate, scene gate, camp task |
| `FUN_00257e70`, `FUN_002580b8`, `FUN_002580e8` | save point menu question and World Map row |
| `FUN_00291140`, `FUN_00291418` | shop entry points (menu kind 4) |
| `FUN_002b3d80`, `FUN_002b4170`, `FUN_002acfb8`, `FUN_002afca0` | journal page builders (moogle, report, treasures) |
| `FUN_001cec00`, `FUN_001793a8`, `FUN_001cb118`, `FUN_001ad378`, `FUN_001ad9a0` | game over decision, list build, revive path |
| `FUN_00156520`, `FUN_00157a50`, `FUN_001540a0`, `FUN_00154498` | Sora's panel and target gauge |

## Open gaps
- Camp: the button set it runs under, the AP rule of the Abilities screen, the dim and transition curves (taken from footage), pages 3, 4, 0x16,
  menu kinds 6, 9, 0xa..0xf named by inference only (`menus-pause.md` "7. Gaps"). The footage is the HD remaster, not the PS2 disc.
- HUD: minimap (`mapj` pair and `radar.tm2` exist), glyph pages above the first, message window close states 4 to 7 and page breaks
  (`hud-notes.md` "Not done, and what each needs").
- Command manager: Party and Limit rows are stand-ins, a downed member is skipped (`game.items` row, `items.md` "Gaps").
- Hidden-member list, the World Map row of the save point menu, the Help overlay of the Customize pages (sheet `ui.menu` open list).
- Title, new game, config at the title, tutorials beyond the mission windows, and the journal's lighting have no doc yet.

## Where the details are
Section numbers in references ("section 5", "20.14") are the old chapter numbers that still stand in the headings of the files below.

| file | KB | covers (section numbers are the old chapter numbers) |
|---|---|---|
| [hud-banners-windows.md](hud-banners-windows.md) | 24 | Text banners and the experience popup; The INFORMATION bar; Damage numbers; HP digits; Message windows; Tutorial windows |
| [hud-notes.md](hud-notes.md) | 14 | Game-state inputs; Screen-edge layout; Debug aids; Rendering notes; Not done, and what each needs; Phase 3 |
| [hud-panels.md](hud-panels.md) | 50 | Files used; Sequence player rules; Text; Sora's panel; Target gauge; Lock-on reticle; +1 more |
| [items.md](items.md) | 26 | sections 1, 2, 3, 4, 5, 6, 7 |
| [menus-abilities.md](menus-abilities.md) | 11 | sections 12, 17 |
| [menus-battle-report.md](menus-battle-report.md) | 10 | section 25 |
| [menus-config.md](menus-config.md) | 4 | section 11 |
| [menus-customize.md](menus-customize.md) | 20 | sections 14, 14b |
| [menus-equipment.md](menus-equipment.md) | 10 | section 15 |
| [menus-gameover.md](menus-gameover.md) | 55 | section 26 |
| [menus-items.md](menus-items.md) | 19 | sections 13, 22 |
| [menus-journal-pages.md](menus-journal-pages.md) | 50 | sections 20.10, 20.11, 20.11a, 20.12, 20.13, 20.14 |
| [menus-journal.md](menus-journal.md) | 47 | section 20 |
| [menus-new-help.md](menus-new-help.md) | 24 | section 16 |
| [menus-party.md](menus-party.md) | 17 | section 19 |
| [menus-pause-m1.md](menus-pause-m1.md) | 8 | section 9 |
| [menus-pause.md](menus-pause.md) | 57 | sections 0, 1, 2, 3, 4, 5, 6, 7, 8 |
| [menus-save.md](menus-save.md) | 29 | section 18 |
| [menus-shop.md](menus-shop.md) | 56 | sections 22, 23, 24 |
| [menus-status.md](menus-status.md) | 10 | section 10 |
| [menus-struggle-result.md](menus-struggle-result.md) | 3 | section 21 |

Index of all systems: [../../README.md](../../README.md).
