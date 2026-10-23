# review.ps1 -- the orchestrator's mechanical check of a worker branch BEFORE merging it.
# It replaces judgement with a fixed list. It changes nothing in git.
#
#   powershell -NoProfile -ExecutionPolicy Bypass -File tools\review.ps1 -Branch wp/7A -Own 'crates/kh2-app/src/fx.rs,crates/kh2-bdx/**,docs/magic.md' -Tests 'name_one,name_two'
#
# Checks, in order (each prints OK or a problem line; the last line is REVIEW: PASS or REVIEW: FAIL):
#  1. the branch's worktree (if any) has no uncommitted changes;
#  2. every file the branch changed (integration...branch) matches one of the -Own globs (the brief's ownership);
#     files outside are listed as OUTSIDE (not automatically a failure: read the report's
#     "Outside-ownership edits" and decide; the brief's exceptions are allowed);
#  3. every test name the report claims (-Tests) exists as `fn <name>` on the branch;
#  4. hygiene of the diff: no binary/asset files added, no decompiler names (iVar1, param_2, ...) in added lines
#     under crates/, CREDITS.md still CRLF;
#  4b. WARN (never fails) for each test fn added on the branch whose body has no assert*! macro / should_panic;
#  5. trial-merge.ps1 (text merge + cargo check --workspace --tests) against integration.
# After REVIEW: PASS: merge with --no-ff, then run verify.ps1 with the new expected test count.
param(
    [Parameter(Mandatory = $true)][string]$Branch,
    [string]$Own = '',
    [string]$Tests = '',
    [string]$Base = 'integration',
    [switch]$SkipTrial
)
. (Join-Path $PSScriptRoot 'env.ps1')
$root = $KhRoot
Set-Location $root
$fail = $false

# 1. worktree clean
$wt = $null
$lines = git worktree list --porcelain
for ($i = 0; $i -lt $lines.Count; $i++) {
    if ($lines[$i] -like 'worktree *') { $path = $lines[$i].Substring(9) }
    if ($lines[$i] -eq "branch refs/heads/$Branch") { $wt = $path }
}
if ($wt) {
    $dirty = git -C $wt status --short
    if ($dirty) { Write-Host "PROBLEM 1: worktree $wt has uncommitted changes:"; $dirty | ForEach-Object { Write-Host "   $_" }; $fail = $true }
    else { Write-Host "OK 1: worktree $wt is clean" }
} else { Write-Host "OK 1: no worktree holds $Branch (nothing uncommitted can exist)" }

# 2. ownership
$files = git diff --name-only "$Base...$Branch"
Write-Host ("    {0} file(s) changed on {1} since it left {2}" -f $files.Count, $Branch, $Base)
if ($Own) {
    $globs = $Own -split ',' | ForEach-Object { $_.Trim() } | Where-Object { $_ }
    $outside = @()
    foreach ($f in $files) {
        $ok = $false
        foreach ($g in $globs) {
            $rx = '^' + ([regex]::Escape($g) -replace '\\\*\\\*', '.*' -replace '\\\*', '[^/]*') + '$'
            if ($f -match $rx) { $ok = $true; break }
        }
        if (-not $ok) { $outside += $f }
    }
    if ($outside.Count) { Write-Host 'NOTE 2: files OUTSIDE the given ownership (check the report explains each):'; $outside | ForEach-Object { Write-Host "   OUTSIDE $_" } }
    else { Write-Host 'OK 2: every changed file is inside the ownership' }
} else { Write-Host 'SKIP 2: no -Own given'; $files | ForEach-Object { Write-Host "   $_" } }

# 3. claimed tests exist
if ($Tests) {
    foreach ($t in ($Tests -split ',' | ForEach-Object { $_.Trim() } | Where-Object { $_ })) {
        $hit = git grep -l "fn $t\b" $Branch -- crates
        if ($hit) { Write-Host "OK 3: test $t in $($hit -join ', ')" } else { Write-Host "PROBLEM 3: claimed test $t not found on $Branch"; $fail = $true }
    }
} else { Write-Host 'SKIP 3: no -Tests given (pass the test names the report quotes)' }

# 4. hygiene of the diff
$bins = $files | Where-Object { $_ -match '\.(png|wav|bin|iso|img|idx|elf|vsb|seb|wd|tm2|mdlx|apdx|pax|mset|bar|bdx|ard)$' }
if ($bins) { Write-Host 'PROBLEM 4: asset/binary files in the diff:'; $bins | ForEach-Object { Write-Host "   $_" }; $fail = $true }
$added = git diff "$Base...$Branch" -- crates | Select-String -Pattern '^\+' | Select-String -Pattern '\b(iVar|uVar|lVar|puVar|piVar|pcVar|bVar|param_)[0-9]+\b'
if ($added) { Write-Host 'PROBLEM 4: decompiler names in added crate lines:'; $added | Select-Object -First 5 | ForEach-Object { Write-Host "   $_" }; $fail = $true }
if ($files -contains 'CREDITS.md') {
    $numstat = (git diff --numstat "$Base...$Branch" -- CREDITS.md) -split '\s+'
    if ($numstat[1] -ne '0') { Write-Host "PROBLEM 4: CREDITS.md lost $($numstat[1]) line(s): only added rows are allowed"; $fail = $true }
    $tmp = [IO.Path]::GetTempFileName()
    cmd /c "git show ${Branch}:CREDITS.md > `"$tmp`""
    if (([IO.File]::ReadAllText($tmp)) -notmatch "`r`n") { Write-Host 'PROBLEM 4: CREDITS.md is no longer CRLF'; $fail = $true }
    else { Write-Host "OK 4: CREDITS.md +$($numstat[0]) row(s), still CRLF" }
    Remove-Item $tmp
}
if (-not $bins -and -not $added) { Write-Host 'OK 4: no assets, no decompiler names in added crate lines' }
if ($files -contains 'sheets/systems.json') { Write-Host 'PROBLEM 4: the branch changed sheets/systems.json (only the orchestrator edits it; revert that file in the merge)'; $fail = $true }

# 4b. every test fn ADDED on the branch must contain an assert (WARN only: the reviewer decides; it does not change PASS/FAIL).
# Counts: assert*, a #[should_panic] attribute. A bare .unwrap()/expect() or an eprintln-only body does not count.
$noAssert = 0
foreach ($f in ($files | Where-Object { $_ -match '^crates/.*\.rs$' })) {
    $addedNames = @(git diff -U0 "$Base...$Branch" -- $f | Select-String -Pattern '^\+\s*(?:pub\s+)?(?:async\s+)?fn\s+(\w+)' | ForEach-Object { $_.Matches[0].Groups[1].Value })
    if (-not $addedNames.Count) { continue }
    $tmp = [IO.Path]::GetTempFileName()
    cmd /c "git show ${Branch}:$f > `"$tmp`" 2>nul"
    $src = [IO.File]::ReadAllText($tmp); Remove-Item $tmp
    foreach ($n in ($addedNames | Select-Object -Unique)) {
        $m = [regex]::Match($src, '((?:#\[[^\]]*\]\s*)+)(?:pub\s+)?(?:async\s+)?fn\s+' + [regex]::Escape($n) + '\s*\(')
        if (-not $m.Success -or $m.Groups[1].Value -notmatch '#\[(?:\w+::)*test\b') { continue }
        if ($m.Groups[1].Value -match 'should_panic') { continue }
        $open = $src.IndexOf('{', $m.Index + $m.Length); if ($open -lt 0) { continue }
        $depth = 0; $end = $open
        for ($k = $open; $k -lt $src.Length; $k++) {
            if ($src[$k] -eq '{') { $depth++ } elseif ($src[$k] -eq '}') { $depth--; if ($depth -eq 0) { $end = $k; break } }
        }
        $body = $src.Substring($open, $end - $open + 1)
        if ($body -notmatch '\bassert\w*!') { Write-Host "WARN: test $n has no assert ($f)"; $noAssert++ }
    }
}
if ($noAssert -eq 0) { Write-Host 'OK 4b: every added test contains an assert' } else { Write-Host "NOTE 4b: $noAssert added test(s) without assert (WARN only, does not fail the review)" }

# 5. trial merge
if (-not $SkipTrial) {
    powershell -NoProfile -ExecutionPolicy Bypass -File (Join-Path $PSScriptRoot 'trial-merge.ps1') -Branches $Branch -Base $Base
    if ($LASTEXITCODE -ne 0) { Write-Host 'PROBLEM 5: trial merge failed (text conflict or compile error above; see ORCHESTRATOR.md O4)'; $fail = $true }
    else { Write-Host 'OK 5: trial merge clean and compiles' }
}

if ($fail) { Write-Host 'REVIEW: FAIL'; exit 1 } else { Write-Host 'REVIEW: PASS'; exit 0 }
