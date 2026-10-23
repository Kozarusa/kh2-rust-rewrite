# merge.ps1 -- merge a reviewed worker branch into the current branch (integration), verify, undo on failure.
#
#   powershell -NoProfile -ExecutionPolicy Bypass -File tools\merge.ps1 -Branch wp/JR1 -ExpectTests 1020 -Message 'Merge wp/JR1: journal limit rows'
#
# Steps:
#  1. refuses to start unless the checkout is clean and -Branch exists;
#  2. git merge --no-ff -Branch with -Message and the Co-Authored-By trailer;
#  3. runs verify.ps1 -ExpectTests <n> (this checkout's tools\verify.ps1; -MaxClippy defaults to local\gate.json);
#  4. FAIL: git revert -m 1 HEAD --no-edit, prints MERGE: REVERTED (the merge and its revert stay in history);
#     PASS: removes the branch's worktree (.claude/worktrees/wp-<id> or whichever worktree holds it) and deletes the
#     branch, prints MERGE: PASS.
# Never amends, forces, or resets hard. A failed merge itself (conflict) is aborted with `git merge --abort`.
# -VerifyScript only exists so the script itself can be tested with a stub gate; leave it at the default.
param(
    [Parameter(Mandatory = $true)][string]$Branch,
    [Parameter(Mandatory = $true)][int]$ExpectTests,
    [Parameter(Mandatory = $true)][string]$Message,
    [int]$MaxClippy = -1,
    [string]$VerifyScript = '',
    [string]$Trailer = 'Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>'
)
. (Join-Path $PSScriptRoot 'env.ps1')
if (-not $VerifyScript) { $VerifyScript = Join-Path $PSScriptRoot 'verify.ps1' }
$trailer = $Trailer
$root = $KhRoot
Set-Location $root

$dirty = git status --short
if ($dirty) { Write-Host 'MERGE: ABORTED (checkout not clean):'; $dirty | ForEach-Object { Write-Host "   $_" }; exit 2 }
git rev-parse --verify --quiet "refs/heads/$Branch" | Out-Null
if ($LASTEXITCODE -ne 0) { Write-Host "MERGE: ABORTED (no branch $Branch)"; exit 2 }

# locate the branch's worktree before merging
$wt = $null
$lines = git worktree list --porcelain
for ($i = 0; $i -lt $lines.Count; $i++) {
    if ($lines[$i] -like 'worktree *') { $path = $lines[$i].Substring(9) }
    if ($lines[$i] -eq "branch refs/heads/$Branch") { $wt = $path }
}
if ($wt -and ((Resolve-Path $wt).Path -eq (Resolve-Path $root).Path)) { Write-Host 'MERGE: ABORTED (branch is checked out here)'; exit 2 }

$before = (git rev-parse HEAD).Trim()
git merge --no-ff $Branch -m $Message -m $trailer
if ($LASTEXITCODE -ne 0) {
    git merge --abort
    Write-Host "MERGE: ABORTED (merge conflict, merge aborted, HEAD still $before)"
    exit 2
}
$merged = (git rev-parse --short HEAD).Trim()
Write-Host "merged $Branch as $merged, running verify"

powershell -NoProfile -ExecutionPolicy Bypass -File $VerifyScript -ExpectTests $ExpectTests -MaxClippy $MaxClippy
if ($LASTEXITCODE -ne 0) {
    git revert -m 1 HEAD --no-edit
    if ($LASTEXITCODE -ne 0) { Write-Host 'MERGE: REVERT FAILED, resolve by hand (do not reset --hard)'; exit 3 }
    Write-Host "MERGE: REVERTED ($merged undone by $((git rev-parse --short HEAD).Trim()); branch and worktree kept)"
    exit 1
}

if ($wt) {
    git worktree remove $wt
    if ($LASTEXITCODE -ne 0) { Write-Host "NOTE: worktree $wt not removed (dirty or locked); remove it by hand" }
}
git branch -d $Branch
if ($LASTEXITCODE -ne 0) { Write-Host "NOTE: branch $Branch not deleted" }
Write-Host "MERGE: PASS ($merged)"
exit 0
