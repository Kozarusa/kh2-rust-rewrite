# rig.ps1 -- the diff rig end to end: one scenario on the original (PCSX2) and on the port (turbo), compared by name.
#
#   powershell -NoProfile -ExecutionPolicy Bypass -File tools\rig.ps1 movement/tt-run-06
#   powershell -NoProfile -ExecutionPolicy Bypass -File tools\rig.ps1 scenarios\menu\camp-open-06.toml -PortOnly
#
# <scenario>   a path to a scenario .toml, or "<system>/<name>" under scenarios\ (the .toml may be left off).
# -PortOnly      run only the port and compare with the original trace already in local\rig\<name>\ (error when there is none).
# -OriginalOnly  run only the original (PCSX2) and write its trace; nothing is compared.
# -Fresh         record the original again even when the cached trace is current.
# -NoBuild       do not run cargo first (the exes in target\debug must be current).
# -Id <name>     the owner written into local\pcsx2.lock (default RIG).
#
# Original side: `kh2-rig run` takes local\pcsx2.lock, backs up the user's PINE slot 0 file, starts the patched PCSX2-MCP with the
# state, steps it one pad read at a time with the scenario's pad script and reads the named values (crates\kh2-rig). It refuses to start
# while any PCSX2 answers its debug or PINE port, and this script refuses while a pcsx2-qt process runs. The trace is cached in
# local\rig\<name>\trace-orig.csv with orig.key (a hash of the scenario, pad script, values.toml and the state file); a current cache
# is reused. Port side: kh2-game in turbo with KH2_STATE (the scenario's state as one file), KH2_PAD, KH2_TRACE, KH2_TRACE_AT and the
# scenario's seeds. Output in local\rig\<name>\ of the main checkout: trace-orig.csv, trace-port.csv, values.csv (every checked value
# of both sides, the difference, the tolerance, PASS/FAIL/INFO), report.txt (a line per row, the first failing k per value, and the
# last line "RIG <name> PASS|FAIL n/m"), port.log. Exit code: 0 pass, 1 fail, 2 error.
param(
    [Parameter(Mandatory = $true, Position = 0)][string]$Scenario,
    [switch]$PortOnly,
    [switch]$OriginalOnly,
    [switch]$Fresh,
    [switch]$NoBuild,
    [string]$Id = 'RIG'
)
$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'env.ps1')
Use-Machine KH2_ISO
if ($PortOnly -and $OriginalOnly) { Write-Host 'rig: -PortOnly and -OriginalOnly exclude each other'; exit 2 }

function Fail([string]$msg) { Write-Host "rig: $msg"; exit 2 }

# --- the scenario file --------------------------------------------------------------------------------------------------------
$file = $null
foreach ($c in @($Scenario, "$Scenario.toml", (Join-Path $KhRoot "scenarios\$Scenario"), (Join-Path $KhRoot "scenarios\$Scenario.toml"))) {
    if (Test-Path -LiteralPath $c -PathType Leaf) { $file = (Resolve-Path -LiteralPath $c).Path; break }
}
if (-not $file) { Fail "scenario '$Scenario' not found (a path, or <system>/<name> under scenarios\)" }

Set-Location $KhRoot
if (-not $NoBuild) {
    cargo build -p kh2-rig --bin kh2-rig -p kh2-app --bin kh2-game
    if ($LASTEXITCODE -ne 0) { Fail 'cargo build failed' }
}
$rigExe = Join-Path $KhRoot 'target\debug\kh2-rig.exe'
$gameExe = Join-Path $KhRoot 'target\debug\kh2-game.exe'
foreach ($e in @($rigExe, $gameExe)) { if (-not (Test-Path $e)) { Fail "$e is missing (build first, or drop -NoBuild)" } }

$env:KH2_RIG_LOCAL = Join-Path $KhMain 'local'
$plan = @{}
& $rigExe plan $file | ForEach-Object { $i = $_.IndexOf('='); if ($i -gt 0) { $plan[$_.Substring(0, $i)] = $_.Substring($i + 1) } }
if ($LASTEXITCODE -ne 0 -or -not $plan.name) { Fail "kh2-rig plan failed for $file" }
$name = $plan.name
$out = Join-Path $KhMain "local\rig\$name"
New-Item -ItemType Directory -Force $out | Out-Null
$origCsv = Join-Path $out 'trace-orig.csv'
$portCsv = Join-Path $out 'trace-port.csv'

# The state file of the scenario, resolved by the client's own rule; the same file is the port's KH2_STATE.
$stateJson = Join-Path $out 'state.json'
$stateFile = (& $rigExe port-state $file $stateJson | Select-Object -Last 1)
if ($LASTEXITCODE -ne 0 -or -not $stateFile) { Fail 'kh2-rig port-state failed (is the state file there?)' }

# --- the original -----------------------------------------------------------------------------------------------------------
if (-not $PortOnly) {
    $parts = @($file, $plan.pad, (Join-Path $KhRoot 'scenarios\values.toml'), $stateFile) | Where-Object { $_ }
    $key = ($parts | ForEach-Object { (Get-FileHash -Algorithm SHA1 -LiteralPath $_).Hash }) -join ''
    $keyFile = Join-Path $out 'orig.key'
    $current = (Test-Path $origCsv) -and (Test-Path $keyFile) -and ((Get-Content $keyFile -Raw).Trim() -eq $key)
    if ($current -and -not $Fresh) {
        Write-Host "rig: the original's trace is current, reused ($origCsv)"
    } else {
        $p = Get-Process pcsx2-qt -ErrorAction SilentlyContinue
        if ($p) { Fail 'a pcsx2-qt process is running (the rig needs the only PCSX2 instance; never stop one you did not start)' }
        Remove-Item $keyFile -ErrorAction SilentlyContinue
        Write-Host "rig: running $name on the original (PCSX2, state $stateFile)"
        & $rigExe run $file --id $Id --out $out
        if ($LASTEXITCODE -ne 0) { Fail 'the run on the original failed' }
        Set-Content -Path $keyFile -Value $key -Encoding ascii
    }
    if ($OriginalOnly) { Write-Host "rig: original trace in $origCsv"; exit 0 }
} elseif (-not (Test-Path $origCsv)) {
    Fail "-PortOnly: no original trace at $origCsv (run once without -PortOnly)"
}

# --- the port ---------------------------------------------------------------------------------------------------------------
Remove-Item $portCsv -ErrorAction SilentlyContinue
$saved = @{}
$set = @{
    KH2_TURBO = '1'; KH2_STATE = $stateJson; KH2_TRACE = $plan.names; KH2_TRACE_AT = $plan.at; KH2_TRACE_FILE = $portCsv
    KH2_PAD = $plan.pad; KH2_TRACE_END = $plan.frames
}
if ($plan['seed.drive']) { $set.KH2_DRIVE_SEED = $plan['seed.drive'] }
if ($plan['seed.auto_limit']) { $set.KH2_AUTO_LIMIT_SEED = $plan['seed.auto_limit'] }
foreach ($k in $set.Keys) { $saved[$k] = [Environment]::GetEnvironmentVariable($k); Set-Item "Env:$k" $set[$k] }
Write-Host "rig: running $name on the port (turbo, area $($plan.area))"
$log = Join-Path $out 'port.log'
try {
    $proc = Start-Process -FilePath $gameExe -ArgumentList $plan.area -WorkingDirectory $out -PassThru -RedirectStandardOutput $log -RedirectStandardError "$log.err"
    if (-not $proc.WaitForExit(600000)) { $proc.Kill(); Fail 'the port run did not finish in 10 minutes' }
} finally {
    foreach ($k in $saved.Keys) { [Environment]::SetEnvironmentVariable($k, $saved[$k]) }
}
if ($proc.ExitCode -ne 0 -or -not (Test-Path $portCsv)) { Fail "the port run failed (exit $($proc.ExitCode)); see $log and $log.err" }

# --- the comparison ---------------------------------------------------------------------------------------------------------
& $rigExe compare $file $origCsv $portCsv --out $out
$code = $LASTEXITCODE
Write-Host "rig: values table $out\values.csv, report $out\report.txt"
exit $code
