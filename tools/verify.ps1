# verify.ps1 -- the build/test/clippy gate. Checks the checkout this script lives in (a worker worktree or the main
# checkout), whatever the current directory. Prints one PASS/FAIL line at the end.
#
#   powershell -NoProfile -ExecutionPolicy Bypass -File tools\verify.ps1 -ExpectTests 1381 -MaxClippy 2
#   powershell -NoProfile -ExecutionPolicy Bypass -File tools\verify.ps1          # counts from local\gate.json
#
# Without -ExpectTests / -MaxClippy the baseline comes from local\gate.json of the main checkout ({"tests": n, "clippy": m}).
# A PASS in the main checkout on branch integration writes the measured counts there (the test count never goes down),
# so nobody edits this file after a merge. KH2_ISO comes from local\machine.ps1 (tools\env.ps1).
param(
    [int]$ExpectTests = -1,
    [int]$MaxClippy = -1,
    [switch]$SkipBuild
)
. (Join-Path $PSScriptRoot 'env.ps1')
Assert-KhHere 'verify.ps1'
Use-Machine @('KH2_ISO')
Set-Location $KhRoot
Remove-Item Env:CARGO_TARGET_DIR -ErrorAction SilentlyContinue   # never share a target dir between checkouts
$gateFile = Join-Path $KhMain 'local\gate.json'
$gate = $null
if (Test-Path $gateFile) { $gate = Get-Content $gateFile -Raw | ConvertFrom-Json }
if ($ExpectTests -lt 0) { if ($gate) { $ExpectTests = [int]$gate.tests } else { throw "verify: pass -ExpectTests (no $gateFile)" } }
if ($MaxClippy -lt 0) { if ($gate) { $MaxClippy = [int]$gate.clippy } else { throw "verify: pass -MaxClippy (no $gateFile)" } }
$pass = $true

if (-not $SkipBuild) {
    if (Get-Process kh2-game -ErrorAction SilentlyContinue) {
        Write-Host 'NOTE: kh2-game is running (its exe is locked): using cargo check instead of cargo build.'
        $b = cargo check --workspace --tests 2>&1
    } else {
        $b = cargo build --workspace 2>&1
    }
    $errs = @($b | Select-String '^error')
    if ($errs.Count -gt 0) {
        $pass = $false
        Write-Host "BUILD: $($errs.Count) error line(s):"
        $errs | Select-Object -First 15 | ForEach-Object { Write-Host "  $($_.Line)" }
    } else {
        Write-Host 'BUILD: clean'
    }
}

$t = cargo test --workspace --no-fail-fast 2>&1
$passed = 0; $failed = 0
foreach ($l in ($t | Select-String 'test result')) {
    if ($l.Line -match '(\d+) passed') { $passed += [int]$Matches[1] }
    if ($l.Line -match '(\d+) failed') { $failed += [int]$Matches[1] }
}
$names = @($t | Select-String '^test .* \.\.\. FAILED' | ForEach-Object { $_.Line })
Write-Host "TESTS: $passed passed, $failed failed (expect >= $ExpectTests passed, 0 failed)"
$names | ForEach-Object { Write-Host "  $_" }
if ($failed -gt 0 -or $passed -lt $ExpectTests) { $pass = $false }
if (@($t | Select-String '^error(\[|:)').Count -gt 0) { $pass = $false; Write-Host 'TESTS: compile error in a test target' }

$c = cargo clippy --workspace 2>&1
$warn = 0; $per = @()
foreach ($l in ($c | Select-String 'generated (\d+) warnings?')) {
    if ($l.Line -match '`([^`]+)`.*generated (\d+) warning') { $per += "$($Matches[1])=$($Matches[2])"; $warn += [int]$Matches[2] }
}
Write-Host "CLIPPY: $warn warnings (max $MaxClippy): $($per -join ' ')"
if ($warn -gt $MaxClippy) { $pass = $false }
if (@($c | Select-String '^error').Count -gt 0) { $pass = $false; Write-Host 'CLIPPY: error lines present' }

$git = git status --short 2>&1 | Select-Object -First 5
if ($git) { Write-Host 'GIT: working tree not clean:'; $git | ForEach-Object { Write-Host "  $_" } }

if ($pass) {
    $branch = (git rev-parse --abbrev-ref HEAD).Trim()
    if ($KhRoot -eq $KhMain -and $branch -eq 'integration') {
        $nt = [Math]::Max($passed, $(if ($gate) { [int]$gate.tests } else { 0 }))
        $nc = $warn
        ('{{"tests": {0}, "clippy": {1}, "head": "{2}"}}' -f $nt, $nc, (git rev-parse --short HEAD).Trim()) | Set-Content $gateFile
        Write-Host "GATE: local\gate.json now tests $nt, clippy $nc"
    }
    Write-Host 'VERIFY: PASS'
} else { Write-Host 'VERIFY: FAIL'; exit 1 }
