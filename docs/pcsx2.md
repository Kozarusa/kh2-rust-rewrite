# PCSX2 as ground truth (the original USA game running in an emulator)

Set up 2026-10-06 with the user's blessing ("do what you must"). PCSX2 v2.8.2: `C:\Program Files\PCSX2\pcsx2-qt.exe`; data folder
`C:\Users\KoZa\Documents\PCSX2` (BIOS SCPH-70012/77008/90006, `inis\PCSX2.ini`; backup of the original ini: `PCSX2.ini.bak-kh2rs`).
Change made: `EnablePINE = true` (PINE slot 28011 = TCP localhost). No savestates exist yet (the user may create some with F1).

Launch (PowerShell): `Start-Process 'C:\Program Files\PCSX2\pcsx2-qt.exe' -ArgumentList '-batch','--','"C:\Users\KoZa\Documents\ClaudeShit\Kingdom Hearts II (USA).iso"'`
(boots to the opening cutscene in about 20 s). Close it when done (`Stop-Process -Name pcsx2-qt`): it competes for CPU with builds.

Tools (local/tools):
- `pine.py`: read and write the PS2's RAM while the real game runs (`status`, `r32 <addr> [n]`, `r8`, `rf`, `watch <addr> [s] [32|16|8|f]`,
  `save <slot>`, `load <slot>`; module `Pine`). EE addresses are the same as in the decompilation (e.g. save block header at 0x33e860 reads
  "KH2U-"; inventory 0x340ce8; experience 0x340e48; battle flag word 0x1c8a7d8). Verified 2026-10-06: status, id SLUS-21005, reads.
- `pcsx2_shot.py <png>`: grabs the emulator window's client area (menu bar included) with PIL; window must be visible.
Uses: (1) measure what a rule really does (timings, values) by watching memory while playing; (2) find who changes a field by
watching it around an action; (3) side-by-side pictures of the original for looks (hurt face, menus, orbs, effects). For code-level
breakpoints use PCSX2's own debugger (Debug > Open Debugger; it shows the game's MIPS and can break on reads and writes).
Limits: no key injection yet (the user plays, or savestates are loaded with `pine.py load <slot>`); PINE has no screenshot call.
Final Mix: the same works with the Final Mix image (research only, ORCHESTRATION one-ISO rule).

## Code-level debugging: PCSX2-MCP (added 2026-10-06 with the user's yes)
The patched emulator from github.com/hkmodd/PCSX2-MCP (release v1.0.0, win64 zip) is unpacked in `local/tools/pcsx2-mcp/` (untracked). It is a full PCSX2 build with a DebugServer on 127.0.0.1:21512 (no authentication; local use only) that adds registers, MIPS disassembly, conditional breakpoints, memory watchpoints, stepping, thread and stack walks to PINE's memory and savestate calls. `local/tools/dbg.py` sends raw JSON requests to it, so Node and the MCP bridge are not needed. Do not run its `setup-mcp.bat` (it writes other tools' MCP configs). Launch `pcsx2-qt.exe` from that folder with the USA iso, not the installed PCSX2; the two share the data folder `C:/Users/KoZa/Documents/PCSX2`, so do not run both at once. Typical use: set a write watchpoint (`set_memcheck`) on a field to find the code that changes it, then read the disassembly around the hit PC.
