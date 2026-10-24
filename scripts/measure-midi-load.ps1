<#
.SYNOPSIS
    测量 MIDI 加载峰值内存与耗时（性能卡验收用）。

.DESCRIPTION
    在独立子进程中运行 `examples/load_peak_probe`，避免与 cargo/其它进程共享读数。
    Windows 下直接读操作系统的峰值计数（不是采样）：

      * PeakPagefileUsage  —— 峰值私有提交（与任务管理器"内存"、MemoryMonitor 的
        PrivateUsage 同口径）。mmap 化后这一项应下降 ≈ 文件大小。
      * PeakWorkingSetSize —— 峰值工作集，**包含 mmap 文件页**。文件改成映射后这一项
        基本不会下降，属预期，不要拿它宣称"物理内存下降"。

    其它平台退化为 /usr/bin/time 提示（本脚本只负责 Windows）。

.PARAMETER Midi
    待测 MIDI 文件路径。

.PARAMETER Reps
    重复次数，默认 3，最后取中位数。

.PARAMETER Exe
    探针路径；缺省时使用 target/release/examples/load_peak_probe[.exe]，不存在则自动构建。

.PARAMETER InputMode
    传给加载器的输入方式：`mmap`（默认）或 `read`（强制整份读取，用于改造前后对照）。

.PARAMETER Label
    输出标签（例如 baseline / mmap），只影响打印。

.EXAMPLE
    pwsh scripts/measure-midi-load.ps1 -Midi "D:\midi\big.mid" -InputMode read -Label baseline
    pwsh scripts/measure-midi-load.ps1 -Midi "D:\midi\big.mid" -InputMode mmap -Label mmap
#>
param(
    [Parameter(Mandatory = $true)][string]$Midi,
    [int]$Reps = 3,
    [string]$Exe = '',
    [ValidateSet('', 'mmap', 'read')][string]$InputMode = '',
    [string]$Label = 'run'
)

$ErrorActionPreference = 'Stop'

if (-not (Test-Path -LiteralPath $Midi)) {
    throw "MIDI 文件不存在: $Midi"
}

$repoRoot = Split-Path -Parent $PSScriptRoot
if (-not $Exe) {
    $name = if ($IsWindows -or $env:OS -eq 'Windows_NT') { 'load_peak_probe.exe' } else { 'load_peak_probe' }
    $Exe = Join-Path $repoRoot "target/release/examples/$name"
    if (-not (Test-Path -LiteralPath $Exe)) {
        Write-Host "构建探针: cargo build --release -p lumino-midi-loader --example load_peak_probe"
        Push-Location $repoRoot
        try {
            cargo build --release -p lumino-midi-loader --example load_peak_probe
        }
        finally {
            Pop-Location
        }
    }
}
$Exe = [System.IO.Path]::GetFullPath($Exe)

if (-not ($IsWindows -or $env:OS -eq 'Windows_NT')) {
    Write-Warning "本脚本的峰值读取依赖 Windows psapi；其它平台请用: /usr/bin/time -v <exe> <midi>"
}

if (-not ('MidiLoadPeak' -as [type])) {
    Add-Type -TypeDefinition @'
using System;
using System.Diagnostics;
using System.Runtime.InteropServices;
using System.Threading;

public class MidiLoadPeakResult {
    public long PeakPrivateBytes;
    public long PeakWorkingSetBytes;
    public long WallMs;
    public int ExitCode;
    public string Stdout = "";
}

public class MidiLoadPeak {
    [StructLayout(LayoutKind.Sequential)]
    private struct PMC {
        public uint cb;
        public uint PageFaultCount;
        public UIntPtr PeakWorkingSetSize;
        public UIntPtr WorkingSetSize;
        public UIntPtr QuotaPeakPagedPoolUsage;
        public UIntPtr QuotaPagedPoolUsage;
        public UIntPtr QuotaPeakNonPagedPoolUsage;
        public UIntPtr QuotaNonPagedPoolUsage;
        public UIntPtr PagefileUsage;
        public UIntPtr PeakPagefileUsage;
    }

    [DllImport("psapi.dll", SetLastError = true)]
    private static extern bool GetProcessMemoryInfo(IntPtr hProcess, ref PMC counters, uint cb);

    public static MidiLoadPeakResult Measure(string exe, string arguments, string inputMode) {
        var psi = new ProcessStartInfo(exe, arguments) {
            UseShellExecute = false,
            RedirectStandardOutput = true,
            RedirectStandardError = false,
        };
        if (!string.IsNullOrEmpty(inputMode)) {
            psi.EnvironmentVariables["LUMINO_MIDI_INPUT"] = inputMode;
        }
        var sw = Stopwatch.StartNew();
        var proc = Process.Start(psi);
        string stdout = proc.StandardOutput.ReadToEnd();
        while (!proc.HasExited) {
            Thread.Sleep(2);
        }
        sw.Stop();

        var result = new MidiLoadPeakResult();
        result.WallMs = sw.ElapsedMilliseconds;
        result.ExitCode = proc.ExitCode;
        result.Stdout = stdout;

        var pmc = new PMC();
        pmc.cb = (uint)Marshal.SizeOf(typeof(PMC));
        // 进程已退出但句柄仍被 Process 对象持有，因此峰值计数依然可读。
        if (GetProcessMemoryInfo(proc.Handle, ref pmc, pmc.cb)) {
            result.PeakPrivateBytes = (long)pmc.PeakPagefileUsage.ToUInt64();
            result.PeakWorkingSetBytes = (long)pmc.PeakWorkingSetSize.ToUInt64();
        }
        proc.Dispose();
        return result;
    }
}
'@
}

function ConvertFrom-ProbeOutput([string]$text) {
    $map = @{}
    foreach ($line in ($text -split "`r?`n")) {
        $trimmed = $line.Trim()
        if (-not $trimmed) { continue }
        foreach ($part in ($trimmed -split ' (?=[A-Z0-9_]+=)')) {
            $idx = $part.IndexOf('=')
            if ($idx -gt 0) {
                $map[$part.Substring(0, $idx)] = $part.Substring($idx + 1)
            }
        }
    }
    return $map
}

$rows = @()
$lastOut = ''
for ($i = 1; $i -le $Reps; $i++) {
    $argStr = '"' + $Midi + '"'
    $r = [MidiLoadPeak]::Measure($Exe, $argStr, $InputMode)
    if ($r.ExitCode -ne 0) {
        throw "探针退出码 $($r.ExitCode)"
    }
    if ($i -eq $Reps) { $lastOut = $r.Stdout }
    $kv = ConvertFrom-ProbeOutput $r.Stdout
    $rows += [pscustomobject]@{
        Rep          = $i
        PeakPrivateM = [math]::Round($r.PeakPrivateBytes / 1MB, 1)
        PeakWorkingM = [math]::Round($r.PeakWorkingSetBytes / 1MB, 1)
        WallMs       = $r.WallMs
        LoaderMs     = [int]$kv['LOADER_MS']
        FileMiB      = [math]::Round([double]$kv['FILE_BYTES'] / 1MB, 1)
        Notes        = [long]$kv['NOTES']
        Tracks       = [int]$kv['TRACKS']
        Cc           = [long]$kv['CC_N']
    }
}

$median = { param($values) ($values | Sort-Object)[[int](($values.Count - 1) / 2)] }

[pscustomobject]@{
    Label            = $Label
    InputMode        = if ($InputMode) { $InputMode } else { 'default(mmap)' }
    File             = Split-Path -Leaf $Midi
    FileMiB          = $rows[0].FileMiB
    Notes            = $rows[0].Notes
    Tracks           = $rows[0].Tracks
    'PeakPrivateMiB' = & $median ($rows.PeakPrivateM)
    'PeakWorkingMiB' = & $median ($rows.PeakWorkingM)
    'LoaderMs'       = & $median ($rows.LoaderMs)
}

$rows | Format-Table -AutoSize

Write-Host ''
Write-Host '== 文档指纹（末次运行，用于改动前后 diff）==' -ForegroundColor Cyan
$lastKv = ConvertFrom-ProbeOutput $lastOut
foreach ($key in @('INPUT_MODE', 'NOTES', 'TRACKS', 'DIVISION', 'TOTAL_TICKS', 'TEMPO_N', 'TEMPO_HASH', 'TIMESIG_N', 'TIMESIG_HASH', 'KEYSIG_N', 'KEYSIG_HASH', 'CC_N', 'CC_HASH', 'LYRICS_N', 'MARKERS_N', 'TEXT_N', 'TEXT_ALL_N', 'TEXT_HASH', 'SYSEX_N', 'SYSEX_HASH', 'NOTES_HASH', 'NAMES_HASH', 'PORTS_HASH', 'DOC_NOTE_MIB')) {
    if ($lastKv.ContainsKey($key)) {
        Write-Host ("{0}={1}" -f $key, $lastKv[$key])
    }
}
