# Run a program under a hard memory cap (a Windows job object). The process is killed by the OS when it commits more than
# the cap, so a runaway tool can not take the whole machine down.
#   powershell -NoProfile -ExecutionPolicy Bypass -File tools\capped.ps1 -CapMB 2500 -Exe target\debug\kh2.exe -ArgLine 'model obj/X.mdlx --shapes'
# Only for commands that loop over many disc files or models (memory rule of 2026-10-22); normal game and test runs go uncapped.
# Exit code: the program's own, or 137 when the cap killed it. Default cap 3000 MB.
param(
    [int]$CapMB = 3000,
    [Parameter(Mandatory = $true)][string]$Exe,
    [string]$ArgLine = ''
)
$ErrorActionPreference = 'Stop'

Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
public static class KhJob {
    [StructLayout(LayoutKind.Sequential)] public struct BASIC { public long PerProcessUserTimeLimit; public long PerJobUserTimeLimit; public uint LimitFlags; public UIntPtr MinimumWorkingSetSize; public UIntPtr MaximumWorkingSetSize; public uint ActiveProcessLimit; public UIntPtr Affinity; public uint PriorityClass; public uint SchedulingClass; }
    [StructLayout(LayoutKind.Sequential)] public struct IOC { public ulong a, b, c, d, e, f; }
    [StructLayout(LayoutKind.Sequential)] public struct EXT { public BASIC Basic; public IOC Io; public UIntPtr ProcessMemoryLimit; public UIntPtr JobMemoryLimit; public UIntPtr PeakProcessMemoryUsed; public UIntPtr PeakJobMemoryUsed; }
    [DllImport("kernel32.dll", CharSet = CharSet.Unicode)] public static extern IntPtr CreateJobObject(IntPtr attr, string name);
    [DllImport("kernel32.dll")] public static extern bool SetInformationJobObject(IntPtr job, int cls, ref EXT info, int len);
    [DllImport("kernel32.dll")] public static extern bool AssignProcessToJobObject(IntPtr job, IntPtr proc);
    public static IntPtr Make(ulong bytes) {
        IntPtr job = CreateJobObject(IntPtr.Zero, null);
        EXT e = new EXT();
        // JOB_OBJECT_LIMIT_PROCESS_MEMORY (0x100) | JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE (0x2000)
        e.Basic.LimitFlags = 0x100 | 0x2000;
        e.ProcessMemoryLimit = new UIntPtr(bytes);
        if (!SetInformationJobObject(job, 9, ref e, Marshal.SizeOf(typeof(EXT)))) throw new Exception("SetInformationJobObject failed");
        return job;
    }
}
'@

$job = [KhJob]::Make([uint64]$CapMB * 1MB)
$psi = New-Object System.Diagnostics.ProcessStartInfo
$psi.FileName = (Resolve-Path $Exe -ErrorAction SilentlyContinue | Select-Object -First 1 -ExpandProperty Path)
if (-not $psi.FileName) { $psi.FileName = $Exe }
$psi.Arguments = $ArgLine
$psi.UseShellExecute = $false
$proc = [System.Diagnostics.Process]::Start($psi)
[void][KhJob]::AssignProcessToJobObject($job, $proc.Handle)
$proc.WaitForExit()
$code = $proc.ExitCode
# a process killed for the memory cap exits with STATUS_COMMITMENT_LIMIT or a failed allocation abort
if ($code -eq -1073741801 -or $code -eq 0xC000012D -or $code -eq -1073740791) { Write-Error "killed by the $CapMB MB memory cap"; exit 137 }
exit $code

