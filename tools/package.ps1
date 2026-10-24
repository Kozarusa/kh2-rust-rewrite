# Builds the release game and assembles dist\kh2-rs\ for players: kh2-game.exe, README.txt and "Play kh2-rs.bat".
# No game data goes in: the player brings their own Kingdom Hearts II (USA) disc image.
#   powershell -NoProfile -ExecutionPolicy Bypass -File tools\package.ps1 [-SkipBuild]
param([switch]$SkipBuild)
$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
Set-Location $root
if (-not $SkipBuild) {
    cargo build --release -p kh2-app --bin kh2-game
    if ($LASTEXITCODE -ne 0) { throw 'release build failed' }
}
$exe = Join-Path $root 'target\release\kh2-game.exe'
if (-not (Test-Path $exe)) { throw "missing $exe" }
$out = Join-Path $root 'dist\kh2-rs'
if (Test-Path $out) { Remove-Item -Recurse -Force $out }
New-Item -ItemType Directory -Force $out | Out-Null
Copy-Item $exe (Join-Path $out 'kh2-game.exe')

$readme = @'
kh2-rs
======

kh2-rs is an unofficial, from-scratch rewrite of Kingdom Hearts II in Rust. It contains NO game data. It reads everything
from YOUR OWN Kingdom Hearts II disc image (PlayStation 2, USA release, serial SLUS-210.05, a .iso file). Other editions,
including Final Mix, are not supported.

Starting
--------
Double-click kh2-game.exe (or "Play kh2-rs.bat"). The first time, a window asks for your disc image; pick the .iso file.
The choice is remembered. To pick another disc later, run: kh2-game.exe --choose-disc
The game starts in Twilight Town (a combat slice); there is no title screen yet.

Controls
--------
WASD            run (relative to the camera); hold Ctrl to walk
Space           jump
Left mouse / J  attack (keep pressing for the combo; in the air, the air combo)
K or E          guard; with a direction held, dodge roll
Q or Tab        toggle lock-on
Right mouse drag or arrow keys   orbit the camera
Mouse wheel or PageUp / PageDown zoom
1-9, 0          switch Twilight Town areas
F11 or Alt+Enter   toggle borderless fullscreen

Options (run kh2-game.exe --help in a terminal, or from "Play kh2-rs.bat" add them after the name):
--choose-disc, --windowed, --fullscreen, --console, and an area name such as tt02 as the first plain argument.

Where things are
----------------
Settings (disc path, window size, fullscreen): %APPDATA%\kh2-rs\config.toml
Log of the last two runs:                      %APPDATA%\kh2-rs\kh2-rs.log and kh2-rs.1.log
If the game closes unexpectedly it names the log file; that file is what to send when reporting a problem.

kh2idx.txt is a list of file names (made by `kh2 fetch-names` from OpenKH's public name list) that the game uses to find maps.
Keep it next to kh2-game.exe or in %APPDATA%\kh2-rs. Without it some maps will not load; the game then shows a message.

Licence: MIT or Apache-2.0. Kingdom Hearts is a trademark of its owners; this project is not affiliated with them.
'@
$names = Join-Path $root 'local\kh2idx.txt'
if (Test-Path $names) { Copy-Item $names (Join-Path $out 'kh2idx.txt') } else { Write-Host 'warning: local\kh2idx.txt is missing; run kh2 fetch-names first' }
Set-Content -Path (Join-Path $out 'README.txt') -Value $readme -Encoding ASCII
Set-Content -Path (Join-Path $out 'Play kh2-rs.bat') -Value "@start `"`" `"%~dp0kh2-game.exe`" %*" -Encoding ASCII
Get-ChildItem $out | Select-Object Name, Length | Format-Table | Out-String | Write-Host
Write-Host "Packaged to $out"
