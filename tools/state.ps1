# state.ps1 -- prints the lines HANDOFF.md's State section needs. Changes nothing.
#
#   powershell -NoProfile -ExecutionPolicy Bypass -File tools\state.ps1            # runs verify.ps1 (slow: build, tests, clippy)
#   powershell -NoProfile -ExecutionPolicy Bypass -File tools\state.ps1 -NoVerify  # skips the gate, prints "not run"
# Planned (TODO: the orchestrator removes this line once `kh2 ledger report` exists, WP-P0C): print the ledger headline too.
param([switch]$NoVerify, [int]$ExpectTests = 0)
. (Join-Path $PSScriptRoot 'env.ps1')
$root = $KhRoot
Set-Location $root
Write-Host ("Last updated: {0}" -f (Get-Date -Format 'yyyy-MM-dd HH:mm'))
Write-Host ("HEAD: {0} on {1}" -f (git log -1 --format='%h %s').Trim(), (git rev-parse --abbrev-ref HEAD).Trim())
$dirty = @(git status --short).Count
Write-Host "Working tree: $(if ($dirty -eq 0) { 'clean' } else { "$dirty changed path(s)" })"
git merge-base --is-ancestor main HEAD
$mainOk = ($LASTEXITCODE -eq 0)
Write-Host ("main: {0}" -f $(if ($mainOk) { 'ancestor of HEAD (can follow)' } else { 'NOT an ancestor of HEAD' }))
if ($NoVerify) {
    Write-Host 'Tests: not run (-NoVerify)'; Write-Host 'Clippy: not run (-NoVerify)'
} else {
    $out = powershell -NoProfile -ExecutionPolicy Bypass -File (Join-Path $PSScriptRoot 'verify.ps1') -ExpectTests $ExpectTests 2>&1
    $t = $out | Select-String '^TESTS:' | Select-Object -First 1
    $c = $out | Select-String '^CLIPPY:' | Select-Object -First 1
    $v = $out | Select-String '^VERIFY:' | Select-Object -First 1
    Write-Host ("Tests: {0}" -f $(if ($t) { $t.Line.Trim() } else { 'no TESTS line (see verify output)' }))
    Write-Host ("Clippy: {0}" -f $(if ($c) { $c.Line.Trim() } else { 'no CLIPPY line' }))
    Write-Host ("Gate: {0}" -f $(if ($v) { $v.Line.Trim() } else { 'no VERIFY line' }))
}
Write-Host 'Worktrees (path, head, branch):'
git worktree list | ForEach-Object { Write-Host "   $_" }
$drv = Get-PSDrive C
Write-Host ("Free on C: {0:N0} GB" -f ($drv.Free / 1GB))
Write-Host ("kh2-game running: {0}" -f $(if (Get-Process kh2-game -ErrorAction SilentlyContinue) { 'yes (do not stop it)' } else { 'no' }))
