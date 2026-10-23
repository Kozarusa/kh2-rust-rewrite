@echo off
rem machine.bat -- loads the machine paths for the .bat tools: `call "%~dp0machine.bat" || exit /b 1`.
rem Reads local\machine.env (NAME=VALUE lines written by tools\env.ps1 from local\machine.ps1) of this checkout, else the
rem main checkout's; a variable that is already set wins. Also sets KH2_TOOLROOT (this checkout) and KH2_MAINROOT.
for %%d in ("%~dp0..") do set "KH2_TOOLROOT=%%~fd"
set "KH2_MAINROOT=%KH2_TOOLROOT%"
for /f "delims=" %%g in ('git -C "%KH2_TOOLROOT%" rev-parse --path-format^=absolute --git-common-dir 2^>nul') do set "KH2_GITCOMMON=%%g"
if defined KH2_GITCOMMON for %%d in ("%KH2_GITCOMMON%\..") do set "KH2_MAINROOT=%%~fd"
set "KH2_ENVFILE=%KH2_TOOLROOT%\local\machine.env"
if not exist "%KH2_ENVFILE%" set "KH2_ENVFILE=%KH2_MAINROOT%\local\machine.env"
if not exist "%KH2_ENVFILE%" (
  echo machine paths missing: no local\machine.env. Copy tools\machine.example.ps1 to %KH2_MAINROOT%\local\machine.ps1, fill it in,
  echo then run: powershell -NoProfile -ExecutionPolicy Bypass -File tools\env.ps1
  exit /b 1
)
set "KH2_FMSET="
if defined KH2_ISO_FM set "KH2_FMSET=1"
for /f "usebackq eol=# tokens=1,* delims==" %%a in ("%KH2_ENVFILE%") do if not defined %%a set "%%a=%%b"
set "KH2_ENVFILE="
set "KH2_GITCOMMON="
rem KH2_ISO_FM would switch on the Final Mix tests in a cargo run started from here: research only, so it is not kept.
if not defined KH2_FMSET set "KH2_ISO_FM="
set "KH2_FMSET="
exit /b 0
