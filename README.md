# kh2-rs

An experimental, from-scratch Rust rewrite of **Kingdom Hearts II** (PS2, USA, `SLUS_210.05`).
It reads all game data at runtime from **your own disc image**. No game assets, extracted files or decompiled code are in this repository, and none ever will be.

Status: a playable combat slice in Twilight Town. Everything is decoded from your disc at startup: maps, sky, textures, models, animations, hitboxes and stats.

Sora:
- **Movement:** runs, walks, turns and jumps with his real speeds and jump height from the game's `pref` tables.
- **Animation:** plays his real motions (idle and battle stances, run, jump, fall, land, attacks, damage), GPU-skinned.
- **Keyblade:** holds the Kingdom Key in his right hand; it runs its own motion set in lockstep with his.
- **Combos:** a ground combo (A300 → A301 → A302 → finisher A303/A304) and a hovering air combo (A310–A314). Each hit chains only inside the motions' own combo windows.
- **Weapon trail:** the keyblade's own trail effect from its PAX, run by a port of the game's particle program runtime, shown exactly while the keyblade motion's trail trigger is on.
- **Hitboxes:** the keyblade's motion triggers switch on its real collision shapes for exactly the authored frames.
- **Damage:** comes from `atkp` with the game's formula, so damage numbers match the original.
- **Guard and lock-on:** guards (A320, with the A321 counter on a frontal block), dodge-rolls, and locks on with KH2-style camera assist.
- **Getting hit:** flinches, gets blown away, goes down and revives.

Enemies:
- **Shadows and Soldiers:** spawn in waves with their own models, motions, hitboxes, and `enmp`/`lvpm` stats.
- **Behaviour:** they chase, sink and resurface (Shadows), attack, flinch, get blown away and die.

Not done yet:
- IK and motion constraints.
- Magic, abilities, Drive Forms, Limits and reaction commands.
- The game's real enemy AI scripts (BDX); a stand-in AI is used.
- COCT collision; the render triangles are used instead.
- Other effects (hit sparks, Heartless deaths, magic), audio, NPCs, events and menus.

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
| `crates/kh2-app` | The game (`kh2-game`, Bevy 0.19): areas, skinned characters, Sora, Heartless, combat, camera, HUD |
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
