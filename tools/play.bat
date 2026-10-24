@echo off
rem Builds kh2-game in the main checkout (machine path KH2_REPO, branch integration), copies the exe to local\play\ and runs
rem the COPY, so a running game never locks target\debug\kh2-game.exe (the orchestrator can rebuild while you play).
rem Optional first argument: area (default tt04, e.g. tt00).   tools\play.bat tt00
setlocal
call "%~dp0machine.bat" || exit /b 1
if not defined KH2_ISO echo KH2_ISO is not set in local\machine.ps1 & exit /b 1
if not defined KH2_REPO echo KH2_REPO is not set in local\machine.ps1 & exit /b 1
set "KH2_FRONT=1"
cd /d "%KH2_REPO%"
set "AREA=%~1"
if "%AREA%"=="" set "AREA=tt04"
cargo build -p kh2-app --bin kh2-game
if not exist "local\play" mkdir "local\play"
copy /y "target\debug\kh2-game.exe" "local\play\kh2-game.exe" >nul
if "%AREA%"=="tt04" if "%KH2_WAVE_GROUP%"=="" set "KH2_WAVE_GROUP=b_44"
echo Options: KH2_FIELD_HZ=30 for the original 30 Hz field (default 60, Numpad + toggles); KH2_VSYNC=0 for uncapped rendering (Numpad - toggles).
"local\play\kh2-game.exe" %AREA%
echo.
echo Game closed (exit code %errorlevel%). Press any key to close this window.
pause >nul
