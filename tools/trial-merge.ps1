# trial-merge.ps1 -- tells you BEFORE merging whether branches merge cleanly AND still compile (tests included).
# Touches nothing: it builds throwaway commits with git plumbing and checks them out in a temporary worktree.
#
#   powershell -NoProfile -ExecutionPolicy Bypass -File tools\trial-merge.ps1 -Branches wp/2G2
#   powershell -NoProfile -ExecutionPolicy Bypass -File tools\trial-merge.ps1 -Branches wp/2G2,wp/2B2   # in this order onto integration
#
# Works on the checkout this script lives in (normally the main checkout). The compile check uses its OWN target dir (target-trial, git-ignored):
# NEVER point CARGO_TARGET_DIR of one checkout at another's target dir (it corrupted artifacts in shift 4:
# "struct Scene has no field named reach" from a stale rlib). The first run builds all dependencies (slow, ~5-10 min).
param(
    [Parameter(Mandatory = $true)][string[]]$Branches,
    [string]$Base = 'integration',
    [switch]$KeepWorktree
)
$Branches = @($Branches | ForEach-Object { $_ -split ',' } | Where-Object { $_ })   # -File passes "a,b" as one string
. (Join-Path $PSScriptRoot 'env.ps1')
Use-Machine @('KH2_ISO')
$root = $KhRoot
Set-Location $root

$cur = (git rev-parse $Base).Trim()
foreach ($b in $Branches) {
    $out = git merge-tree --write-tree $cur $b 2>&1
    if ($LASTEXITCODE -ne 0) {
        Write-Host "TEXT CONFLICT merging $b onto $Base (plus earlier branches):"
        $out | ForEach-Object { Write-Host "  $_" }
        exit 1
    }
    $tree = ($out | Select-Object -First 1).Trim()
    $cur = (git commit-tree $tree -p $cur -p $b -m "trial merge of $b (dangling)").Trim()
    Write-Host "text merge clean: $b"
}

$wt = Join-Path $root '.claude\worktrees\_trial'
if (Test-Path $wt) { git worktree remove --force $wt | Out-Null }
git worktree add --detach $wt $cur 2>&1 | Select-Object -Last 1
Push-Location $wt
$env:CARGO_TARGET_DIR = Join-Path $root 'target-trial'
$o = cargo check --workspace --tests 2>&1
$code = $LASTEXITCODE
Pop-Location
Remove-Item Env:CARGO_TARGET_DIR -ErrorAction SilentlyContinue
if (-not $KeepWorktree) { git worktree remove --force $wt | Out-Null; git worktree prune }

if ($code -eq 0) {
    Write-Host 'COMPILE: clean (cargo check --workspace --tests)'
    exit 0
}
Write-Host "COMPILE: FAILED (exit $code). First errors:"
$o | Select-String -Pattern '^error' -Context 0, 6 | Select-Object -First 12 | ForEach-Object { Write-Host $_.ToString() }
Write-Host 'Fix these in the merge commit (or ask the branch owner to merge integration first and fix them).'
exit 1
