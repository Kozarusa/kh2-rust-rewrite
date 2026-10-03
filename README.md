# kh2-rs

An experimental, from-scratch Rust rewrite of **Kingdom Hearts II** (PS2, USA, `SLUS_210.05`).
It reads all game data at runtime from **your own disc image**. No game assets, extracted files or decompiled code are in this repository, and none ever will be.

Status: a playable vertical slice. You run Sora around the ten Twilight Town areas, with maps, the sky, textures and Sora's model all decoded from your disc at startup.

Not done yet:
- Sora has no animation (he stays in his rest pose).
- Collision uses the render triangles; the game's COCT collision isn't read.
- No NPCs, events, combat, menus or audio.

`cargo run -p kh2 -- preflight` lists exactly what is done.

## Play

```
cargo run -p kh2 -- set-iso "path/to/Kingdom Hearts II (USA).iso"
cargo run -p kh2 -- fetch-names        # OpenKH's file-name list (Apache-2.0), saved to local/
cargo run --release -p kh2-app         # optional area argument, e.g. tt02
```

Controls:
- **WASD:** run, relative to the camera. Hold **Shift** to walk.
- **Space:** jump.
- **Right mouse drag or arrow keys:** orbit the camera. **Mouse wheel or PageUp/PageDown:** zoom.
- **1–9, 0:** switch Twilight Town areas (tt01–tt09, tt00).

For automated checks, `KH2_SCREENSHOT=out.png` saves a frame after 8 s and quits. Add `KH2_WALK=1` to hold W for 3 s first, and `KH2_YAW=<radians>` to turn the camera.

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
| `crates/kh2-app` | The game (`kh2-game`, Bevy 0.19): areas, Sora, controls, camera, stand-in collision |
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
