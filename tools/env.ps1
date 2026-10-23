# env.ps1 -- machine paths and repository roots for every tool script. Nothing machine-specific is tracked:
# the paths live in the untracked file local\machine.ps1 (copy tools\machine.example.ps1 there and fill it in).
#
# Dot-source it from a script or a shell before cargo:   . "$PSScriptRoot\env.ps1"
#                                    or, in a shell:     . .\tools\env.ps1; Use-Machine KH2_ISO; cargo test -p kh2-bdx
# Run it directly to print the values and (re)write local\machine.env (the same values as NAME=VALUE lines, read by the
# Python tools and the .bat files):                       powershell -NoProfile -ExecutionPolicy Bypass -File tools\env.ps1
#
# After dot-sourcing:
#   $KhRoot   the checkout this script belongs to (a worktree or the main checkout), found from the script's own path
#   $KhMain   the main checkout (the parent of git's common directory), where the shared untracked local\ lives
#   Use-Machine KH2_ISO,KH2_DECOMP   sets the named variables in $env: and stops with a clear message when one is missing
#   Get-KhLocal 'local\x.json'       the file in this checkout's local\ if present, else the main checkout's
# A variable already set in the environment wins over local\machine.ps1 (so a shell can point KH2_ISO elsewhere).
$KhRoot = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
$KhMain = $KhRoot
$gitCommon = git -C $KhRoot rev-parse --path-format=absolute --git-common-dir 2>$null
if ($LASTEXITCODE -eq 0 -and $gitCommon) { $KhMain = (Resolve-Path (Split-Path -Parent $gitCommon.Trim())).Path }

function Get-KhLocal([string]$rel) {
    foreach ($b in @($KhRoot, $KhMain)) { $f = Join-Path $b $rel; if (Test-Path $f) { return (Resolve-Path $f).Path } }
    return $null
}

# Stops when the current directory is inside another checkout than the script's own (a worker running the main
# checkout's copy would check the main checkout, not its worktree).
function Assert-KhHere([string]$what) {
    $top = git rev-parse --show-toplevel 2>$null
    if ($LASTEXITCODE -eq 0 -and $top -and ((Resolve-Path $top.Trim()).Path -ne $KhRoot)) {
        throw "${what}: this copy works on $KhRoot, but you are in $($top.Trim()): run that checkout's own tools\$what instead"
    }
}

function Get-KhMachineFile {
    $f = Get-KhLocal 'local\machine.ps1'
    if (-not $f) {
        throw ("machine paths missing: no local\machine.ps1 in $KhRoot or $KhMain. Copy tools\machine.example.ps1 to " +
            "$KhMain\local\machine.ps1 and fill in the paths of this machine.")
    }
    return $f
}

# Variables that name a file or folder that must exist.
$KhMachinePaths = @('KH2_ISO', 'KH2_ISO_FM', 'KH2_DECOMP', 'KH2_PCSX2', 'KH2_PCSX2_MCP', 'KH2_PCSX2_ISO', 'KH2_PCSX2_DATA', 'KH2_VIDEOS', 'KH2_REPO')

function Use-Machine([string[]]$Names) {
    $file = Get-KhMachineFile
    $m = & $file
    if (-not ($m -is [hashtable])) { throw "machine paths: $file must end with a hashtable @{ KH2_ISO = '...'; ... } (see tools\machine.example.ps1)" }
    # Only the named variables are exported: KH2_ISO_FM, for example, would switch on the Final Mix tests.
    foreach ($n in $Names) {
        if (-not [Environment]::GetEnvironmentVariable($n) -and $m.ContainsKey($n)) { Set-Item "Env:$n" ([string]$m[$n]) }
    }
    foreach ($n in $Names) {
        $v = [Environment]::GetEnvironmentVariable($n)
        if (-not $v) { throw "machine path $n is not set: add $n = '<path>' to $file (see tools\machine.example.ps1)" }
        if ($KhMachinePaths -contains $n -and -not (Test-Path -LiteralPath $v)) { throw "machine path $n = '$v' does not exist: fix it in $file" }
    }
    Write-KhMachineEnv $file $m
}

# local\machine.env next to machine.ps1, rewritten when missing or older than machine.ps1.
function Write-KhMachineEnv([string]$file, [hashtable]$m, [switch]$Force) {
    $envFile = Join-Path (Split-Path -Parent $file) 'machine.env'
    if (-not $Force -and (Test-Path $envFile) -and ((Get-Item $envFile).LastWriteTime -ge (Get-Item $file).LastWriteTime)) { return }
    $lines = @('# generated from machine.ps1 by tools\env.ps1: edit machine.ps1, not this file')
    foreach ($k in ($m.Keys | Sort-Object)) { $lines += "$k=$($m[$k])" }
    [IO.File]::WriteAllLines($envFile, $lines)
}

if ($MyInvocation.InvocationName -ne '.') {
    $file = Get-KhMachineFile
    $m = & $file
    Write-KhMachineEnv $file $m -Force
    Write-Host "checkout: $KhRoot"
    Write-Host "main checkout: $KhMain"
    Write-Host "machine file: $file"
    foreach ($k in ($m.Keys | Sort-Object)) {
        $ok = if ($KhMachinePaths -contains $k) { if (Test-Path -LiteralPath $m[$k]) { 'ok' } else { 'MISSING' } } else { '' }
        Write-Host ("  {0,-15} {1} {2}" -f $k, $m[$k], $ok)
    }
}
