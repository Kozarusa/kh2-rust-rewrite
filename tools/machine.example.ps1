# machine.example.ps1 -- the machine paths every tool script reads (through tools\env.ps1).
# Copy this file to local\machine.ps1 in the MAIN checkout (local\ is git-ignored), replace the placeholders, and keep it
# there: worktrees find it through git's common directory. Then run
#     powershell -NoProfile -ExecutionPolicy Bypass -File tools\env.ps1
# once; it checks every path and writes local\machine.env (the same values for the Python tools and the .bat files).
# Never commit real paths: this example keeps placeholders only.
@{
    # The user's own USA disc image (SLUS_210.05). Read at run time by the game, the tests and the CLI.
    KH2_ISO        = 'D:\path\to\Kingdom Hearts II (USA).iso'
    # The Final Mix image (SLPM_666.75): research only, never read by the app (RULES A3).
    KH2_ISO_FM     = 'D:\path\to\Kingdom Hearts II - Final Mix (Japan).iso'
    # The private reverse-engineering workspace (kh2_ps2): decomp\, decomp_typed\, input\SLUS_210.05, raw\fm_usa_match.json.
    KH2_DECOMP     = 'D:\path\to\kh2_ps2'
    # The installed PCSX2 (the user's own emulator; PINE enabled).
    KH2_PCSX2      = 'D:\path\to\PCSX2\pcsx2-qt.exe'
    # The patched PCSX2 with the debug server (breakpoints, memory reads over TCP 21512).
    KH2_PCSX2_MCP  = 'D:\path\to\PCSX2-MCP\pcsx2-qt.exe'
    # The disc image PCSX2 boots (the user keeps a copy next to the emulator).
    KH2_PCSX2_ISO  = 'D:\path\to\PCSX2\Games\Kingdom Hearts II (USA).iso'
    # PCSX2's data folder (inis, sstates, memcards).
    KH2_PCSX2_DATA = 'D:\path\to\Documents\PCSX2'
    # The user's reference videos of the original game (4.mp4 .. 10_menu_open_close.mp4).
    KH2_VIDEOS     = 'D:\path\to\Videos'
    # The main checkout of this repository (integration).
    KH2_REPO       = 'D:\path\to\kh2-rs'
}
