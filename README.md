# kh2-rs

An experimental, from-scratch Rust rewrite of **Kingdom Hearts II** (PS2, USA, `SLUS_210.05`).
It reads all game data at runtime from **your own disc image**. No game assets, extracted files or decompiled code are in this repository, and none ever will be.

Status: a playable combat slice in Twilight Town (`tt00`, `tt02`, `tt04`) with the camp menu, shops and the first events. Everything is decoded from your disc at startup: maps, sky, textures, models, animations, hitboxes, stats, enemy scripts, effects and audio.

Sora:
- **Movement:** runs, walks, turns and jumps with his real speeds and jump height from the game's `pref` tables.
- **Animation:** plays his real motions (idle and battle stances, run, jump, fall, land, attacks, damage), GPU-skinned.
- **Keyblade:** holds the Kingdom Key in his right hand; it runs its own motion set in lockstep with his.
- **Combos:** a ground combo (A300 → A301 → A302 → finisher A303/A304) and a hovering air combo (A310–A314). Each hit chains only inside the motions' own combo windows.
- **Weapon trail:** the keyblade's own trail effect from its PAX, run by a port of the game's particle program runtime, shown exactly while the keyblade motion's trail trigger is on.
- **Hit sparks:** chosen like the game does (the attack's effect bits matched against the keyblade PAX's element categories) and run by the same runtime, with textures sampled from an emulated GS memory.
- **Hitboxes:** the keyblade's motion triggers switch on its real collision shapes for exactly the authored frames.
- **Damage:** comes from `atkp` with the game's formula, so damage numbers match the original.
- **Guard and lock-on:** guards (A320, with the A321 counter on a frontal block), dodge-rolls, and locks on with KH2-style camera assist.
- **Getting hit:** flinches, gets blown away, goes down and revives.

Sora (continued):
- **Abilities and drive forms:** the ability and stat block, growth abilities, Valor, Wisdom, Master, Final and Anti forms with their outfits and hand weapons, and limits.
- **Reaction commands:** native offers, the accept path, and Soldier's command 34 played on Sora; clashes between attacks.
- **Camera:** the game's field camera model, look-around (the model fades), area camera types and collision.
- **Magic (phase 1):** a Magic page in the command menu (T/G move, B back, J casts) with Fire, Blizzard, Thunder and Cure at level 1, their MP cost, charge and damage rows.
- **HUD:** gauges, the command menu with its reaction capsule, text with commands, the area-name card, banners and message windows.
- **Party:** Donald and Goofy follow, fight with the friend AI's target styles (including the class 16 chooser), and their Battle Style and use settings are set on the Customize page. Slot 3 is a reserve member, Auto Change swaps it in when a companion is knocked out, a jump record can rearrange the party and show the "Party Members have been switched" notice, Donald and Goofy are targets of the field command's items and have rows on the pause menu's Items and Equipment pages. Reserve members keep HP and MP across areas, regain HP with Auto Healing, and a partner's limit is offered as a reaction command by Auto Limit. The limit's own script now runs as a thread, but nothing it would show is ported, so no limit scene plays.
- **Summons:** the gauge, the Summon and Dismiss commands, and the Stitch, Genie, Chicken Little and Peter Pan scripts with their hosts (link break and routine, the item command, Stitch's limit records and heal, Genie's follow motions, Tinker Bell's revive sequence). The summon limits run only from a harness aid, not from play.
- **Menus:** the camp menu with Status, Abilities, Items, Equipment, Customize, Config, Party, Help, Save, Jiminy's Journal (world grid, story, characters, Heartless and Nobody lists with defeated counts, Ansem reports, limits, moogle list, quests, minigames, treasures, maps, album, and 3D models on the character and model pages) and the moogle's shop with synthesis, collection rewards and their texts. The BATTLE REPORT ending screen reads its counters from the save state. Sora's knock-out opens the game-over list (Continue, Load Game, Quit, Retry), and Load Game opens the Load screen on the slot files the Save menu wrote.
- **Townspeople:** their heads turn toward the player (head look-at controller).
- **Story and events:** story flags with their side effects, NPC talk, save points and an event player (actors, camera, subtitles, fades, hiding and showing the event's objects, the keyhole wipe above the HUD), jump types 4 to 6 and the jump record's party words.
- **Audio:** the game's sound driver, area music with the battle switch, voices and footsteps, and positional volume and pan. Mostly unlistened.

Enemies:
- **Shadows, Soldiers, Dusks and one more:** each has its own model, motions, hitboxes, and `enmp`/`lvpm` stats. In Twilight Town (tt04) the waves come from the area's own battle groups (Dusks and a Struggle opponent); other areas use stand-in Shadow/Soldier waves.
- **Behaviour:** they run the game's own AI scripts (BDX) in an interpreter that was checked against the game's code, so they chase, sink and resurface (Shadows), attack, flinch, get blown away and die as in the original.

Not done yet:
- Magic beyond level 1 (Magnet, Reflect, drive-form casts).
- Summons: the limit task (summon limits are reachable only through a harness aid), the camera's FPS mode, Genie's model parts.
- Party: the limit scene an accepted Auto Limit offer should start (the limit script runs and Sora, Donald and Goofy play its motions and effects, but other characters' participants, the camera event, the gauge animation and host 7.28 are missing), and the member-swap fade.
- Game over: the GAMEOVER scene model (WP-GO2), restoring the save body on Continue, and the Load screen's header art. Mickey's rescue is ported only as far as the decision, the chance and the alternate list; it is switched off because Mickey and his heal are not found, so the 11 missions that could offer it show the normal list.
- Other enemy types and some area-script group kinds.
- Journal: the viewer plate's fade and lighting on the model pages, and the page-turn gap; the World Map row of the save point menu, and the world map itself.
- BATTLE REPORT rows 1 and 2 (no source found), and the shop bits in the slot files.
- Events: the movie player and voices.
- Final Mix differences (only the USA release is supported).
- Some audio gaps (the landing footstep) and some effects (munny payout).
- COCT collision is used; some camera blockers and the Quick Run camera speed are open.

The open rows of `sheets/systems.json` are the to-do list.

`cargo run -p kh2 -- preflight` lists exactly what is done.

## Play

```
cargo run -p kh2 -- set-iso "path/to/Kingdom Hearts II (USA).iso"
cargo run -p kh2 -- fetch-names        # OpenKH's file-name list (Apache-2.0), saved to local/
cargo run --release -p kh2-app         # optional area argument, e.g. tt02
```

Controls:
- **WASD:** run, relative to the camera. Hold **Ctrl** to walk.
- **Space:** jump.
- **Left mouse button or J:** attack. Keep pressing to continue the combo; in the air it's the air combo.
- **K or E:** guard. With a direction held, dodge roll.
- **Q or Tab:** toggle lock-on.
- **Right mouse drag or arrow keys:** orbit the camera. **Mouse wheel or PageUp/PageDown:** zoom.
- **1–9, 0:** switch Twilight Town areas (tt01–tt09, tt00).
- **F1:** motion viewer for every Sora motion; step with **[** and **]**.
- **F3:** spawn an enemy wave.
- **F4:** show hit volumes (red = live attacks, green = hurtboxes).

Automated checks use these environment variables:
- `KH2_SCREENSHOT=out.png`: save a frame at `KH2_SHOT_AT` seconds (default 8).
- `KH2_KEYS=6:Q,6.5:J,6.8:J`: tap keys at given times.
- `KH2_WALK=1`: hold W from 3 s to 6 s.
- `KH2_YAW` / `KH2_DIST`: set the camera angle and distance.
- `KH2_VIEW_SLOT=<slot>`: open the motion viewer on that slot.
- `KH2_NO_ENEMIES=1`: disable enemy waves.
- `KH2_FPS=1`: log the frame rate.

## Tools

```
cargo run -p kh2 -- info
cargo run -p kh2 -- table plrp 5
cargo run -p kh2 -- model obj/P_EX100.mdlx --textures   # stats; PNGs go to local/tex/
cargo run -p kh2 -- validate        # decodes every modelled table, ticks the sheets
cargo run -p kh2 -- preflight
```

Only the USA release is supported. Other discs are refused, because table layouts differ between editions.

## Layout

| Path | Role |
|---|---|
| `sheets/` | **Source of truth.** JSON spreadsheets: `systems.json` (game systems), `tables.json` (binary layouts). They hold structure and status only, never game values. |
| `crates/kh2-iso` | ISO9660 reader |
| `crates/kh2-archive` | IDX/IMG + overlays: name hashes, LZ decompression |
| `crates/kh2-bar` | BAR container |
| `crates/kh2-sheets` | Spreadsheet method: preflight checklist and struts (rows → Rust) |
| `crates/kh2-tables` | Gameplay tables. Their structs are generated from `sheets/tables.json` by `build.rs`, which runs preflight first. |
| `crates/kh2-ps2` | PS2 hardware behaviour needed for assets: VIF1 unpacking into VU1 memory, DMA chains, VPU packets, GS swizzling |
| `crates/kh2-model` | Map and skeletal models, plus model textures, decoded into plain meshes and RGBA images |
| `crates/kh2-anim` | Motion sets, ANB motions and triggers, pose evaluation |
| `crates/kh2-app` | The game (`kh2-game`, Bevy 0.19) as a library plus a thin binary, with plugins in domain bundles: areas, skinned characters, Sora, Heartless, combat, camera, HUD |
| `crates/kh2` | Command-line tool |
| `docs/` | `METHOD.md` (how we work), `research.md` (sources) |
| `local/` | Your machine only (git-ignored): ISO path, downloaded name list, extracted files |

## How it is built

See [docs/METHOD.md](docs/METHOD.md). In short:
- Every system and binary layout is a row in a sheet. Every row generates code.
- A `null` cell is an unchecked box, and a row is done only when every cell is checked.
- `cargo build` runs a preflight checklist that overlaps all sheets, before compiling.

## Credits and licenses

kh2-rs is dual-licensed under MIT or Apache-2.0, at your option. It builds on community research, credited in [CREDITS.md](CREDITS.md).
Kingdom Hearts is © Disney / Square Enix. This is an unofficial fan project, not affiliated with or endorsed by them. You need your own legally obtained copy of the game.
