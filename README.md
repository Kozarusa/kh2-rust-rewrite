# kh2-rs

A fan-made, from-scratch rewrite of **Kingdom Hearts II** (PS2, USA) in Rust.

It is **not** a copy of the game. This project contains no game files at all. To play, you supply **your own Kingdom Hearts II (USA) disc image** (`.iso`), and the game reads everything from it when it starts.

## What works today

A small playable slice of Twilight Town: run around as Sora, fight Heartless and Nobodies with combos, magic, guard and lock-on, use the pause menu and shops, and watch the first story events. Donald and Goofy fight beside you.

It is an early work in progress. Most of the game is not here yet: other worlds, most bosses and the title screen are missing, and some things look or sound wrong. Please do not expect the full game.

## How to play

**You need:**
- A Windows PC with a graphics card that supports Vulkan or DirectX 12 (any recent one).
- A disc image of Kingdom Hearts II **USA** (`Kingdom Hearts II (USA).iso`) that you made from your own copy. Other editions, including Final Mix, are refused.
- [Rust](https://rustup.rs) (the installer is one click; it also asks you to install Microsoft's C++ build tools, say yes).

**Steps:**
1. Download this project (green **Code** button, then **Download ZIP**, or `git clone`) and open the folder in PowerShell.
2. Build the player package:
   ```
   powershell -NoProfile -ExecutionPolicy Bypass -File tools\package.ps1
   ```
   The first build takes a while (10 to 20 minutes). It creates the folder `dist\kh2-rs\`.
3. Open `dist\kh2-rs\` and double-click **`kh2-game.exe`** (or `Play kh2-rs.bat`).
4. The first time, a window asks for your `.iso`. Pick it. The game remembers it for next time.

The game starts straight in Twilight Town. There is no title screen yet.

If something goes wrong, the log is at `%APPDATA%\kh2-rs\kh2-rs.log`. To pick another disc later, run `kh2-game.exe --choose-disc`.

## Controls

| Key | Action |
|---|---|
| **W A S D** | Run (hold **Ctrl** to walk) |
| **Space** | Jump |
| **Left mouse button** or **J** | Attack (keep pressing for combos) |
| **K** or **E** | Guard |
| **Q** or **Tab** | Lock on |
| **Right mouse drag** or **arrow keys** | Turn the camera |
| **Mouse wheel** or **PageUp / PageDown** | Zoom |
| **1 to 9, 0** | Switch Twilight Town areas |
| **F11** or **Alt+Enter** | Fullscreen |
| **Numpad +** / **Numpad -** | Smooth 60 Hz or original 30 Hz game speed / vsync on or off |

## Is it the real game?

No. It is a re-creation. It aims to behave like the original, and it is checked against the original game's own code, but it is not finished and it is not an official product.

## For developers

Quick start: `cargo run -p kh2 -- set-iso "path\to\Kingdom Hearts II (USA).iso"` then `cargo run --release -p kh2-app`.

## Credits and licenses

kh2-rs is dual-licensed under MIT or Apache-2.0, at your option. It builds on community research, credited in [CREDITS.md](CREDITS.md).
Kingdom Hearts is © Disney / Square Enix. This is an unofficial fan project, not affiliated with or endorsed by them. You need your own legally obtained copy of the game.
