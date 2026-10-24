@echo off
rem Play-test one enemy group by hand. Usage: tools\playtest.bat <worktree-or-.> <area> <btl-programs-hex> <group> [extra env as NAME=VALUE ...]
rem   tools\playtest.bat .claude\worktrees\wp-EN10p eh10 0,1,0 b_00          (M_EX920, three instances)
rem   tools\playtest.bat .claude\worktrees\wp-EN10o tr02 0,35,0 b_60        (M_EX600; add KH2_ENTRANCE=1)
rem The worktree path is relative to the main checkout (machine path KH2_REPO). Copies that tree's built kh2-game.exe to
rem local\play\pt\ and runs the COPY (no lock on the build output).
setlocal
call "%~dp0machine.bat" || exit /b 1
if not defined KH2_ISO echo KH2_ISO is not set in local\machine.ps1 & exit /b 1
if not defined KH2_REPO echo KH2_REPO is not set in local\machine.ps1 & exit /b 1
cd /d "%KH2_REPO%"
set "TREE=%~1"
set "AREA=%~2"
set "KH2_PROGRAMS=%~3"
set "KH2_WAVE_GROUP=%~4"
if not "%~5"=="" set "%~5"
if not "%~6"=="" set "%~6"
if not exist "local\play\pt" mkdir "local\play\pt"
copy /y "%TREE%\target\debug\kh2-game.exe" "local\play\pt\kh2-game.exe" >nul
set "KH2_FRONT=1"
set "KH2_AREALOG=1"
set "KH2_HITLOG=1"
echo Options: KH2_FIELD_HZ=30 for the original 30 Hz field (default 60, F9 toggles); KH2_VSYNC=0 for uncapped rendering (F8 toggles).
"local\play\pt\kh2-game.exe" %AREA% > "local\play\pt\last.log" 2>&1
echo.
echo Game closed (exit code %errorlevel%). Log: local\play\pt\last.log. Press any key to close this window.
pause >nul
