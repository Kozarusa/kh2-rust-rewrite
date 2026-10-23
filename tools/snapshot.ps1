# Commits any unsaved work in every worktree so a rate-limit cut loses nothing.
#   powershell -NoProfile -ExecutionPolicy Bypass -File tools\snapshot.ps1 [-Trailer 'Co-Authored-By: ...']
param([string]$Trailer = 'Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>')
. (Join-Path $PSScriptRoot 'env.ps1')
$root = $KhRoot
foreach ($line in (git -C $root worktree list --porcelain | Select-String '^worktree ')) {
    $wt = $line.ToString().Substring(9)
    $dirty = git -C $wt status --short
    if ($dirty) {
        git -C $wt add -A | Out-Null
        git -C $wt commit -q -m "WIP snapshot (automatic, may not compile)`n`n$Trailer" | Out-Null
        Write-Host "snapshot: $wt"
    }
}
