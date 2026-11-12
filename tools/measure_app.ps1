param(
    [string]$Exe = 'target\release\smartdisk.exe',
    [string]$ScanPath = '',
    [string]$Out = 'target\perf\app-measurements.md',
    [string]$Shot = 'target\perf\ui-scan.png',
    [int]$FieldX = 620,
    [int]$FieldY = 32,
    [int]$SettleSeconds = 3,
    [int]$IdleSeconds = 8
)
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.Windows.Forms
Add-Type -AssemblyName System.Drawing
Add-Type -TypeDefinition @"
using System;
using System.Runtime.InteropServices;
public class PerfWin {
    [StructLayout(LayoutKind.Sequential)]
    public struct RECT { public int Left, Top, Right, Bottom; }
    [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr hWnd, out RECT rect);
    [DllImport("user32.dll")] public static extern bool SetCursorPos(int x, int y);
    [DllImport("user32.dll")] public static extern void mouse_event(uint flags, uint dx, uint dy, uint data, UIntPtr extra);
    [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr hWnd);
    [DllImport("user32.dll")] public static extern bool PrintWindow(IntPtr hWnd, IntPtr hdc, uint flags);
    [DllImport("user32.dll")] public static extern bool ClientToScreen(IntPtr hWnd, ref POINT pt);
    [StructLayout(LayoutKind.Sequential)]
    public struct POINT { public int X, Y; }
}
"@

function Sample($proc) {
    $proc.Refresh()
    [pscustomobject]@{
        CpuMs = $proc.TotalProcessorTime.TotalMilliseconds
        WorkingSet = $proc.WorkingSet64
        Private = $proc.PrivateMemorySize64
        PeakWorkingSet = $proc.PeakWorkingSet64
    }
}

$exePath = (Resolve-Path $Exe).Path
$env:SLINT_BACKEND = 'winit-software'
$sw = [Diagnostics.Stopwatch]::StartNew()
$p = Start-Process -FilePath $exePath -PassThru
$startupMs = -1
$hwnd = [IntPtr]::Zero
while ($sw.Elapsed.TotalSeconds -lt 25) {
    $p.Refresh()
    if ($p.HasExited) { throw "SmartDisk exited during startup with code $($p.ExitCode)" }
    $candidate = $p.MainWindowHandle
    if ($candidate -ne 0 -and $p.Responding) {
        # Win32 and accessibility helpers can create small hidden windows before
        # the real UI exists; only accept a visible, reasonably sized main window.
        $candidateRect = New-Object PerfWin+RECT
        if ([PerfWin]::GetWindowRect($candidate, [ref]$candidateRect)) {
            $cw = $candidateRect.Right - $candidateRect.Left
            $ch = $candidateRect.Bottom - $candidateRect.Top
            if ($cw -gt 400 -and $ch -gt 300 -and $p.MainWindowTitle) {
                $hwnd = $candidate
                $startupMs = $sw.Elapsed.TotalMilliseconds
                break
            }
        }
    }
    Start-Sleep -Milliseconds 50
}
if ($startupMs -lt 0) { throw 'SmartDisk did not show a responsive main window within 25 seconds' }

Start-Sleep -Seconds $SettleSeconds
$idle0 = Sample $p
$idleWall = [Diagnostics.Stopwatch]::StartNew()
Start-Sleep -Seconds $IdleSeconds
$idle1 = Sample $p
$idleWall.Stop()
$idleCpuPercent = (($idle1.CpuMs - $idle0.CpuMs) / $idleWall.Elapsed.TotalMilliseconds) * 100.0

$scan = $null
if ($ScanPath) {
    Add-Type -TypeDefinition @"
using System;
using System.Runtime.InteropServices;
public class PerfRect {
    [StructLayout(LayoutKind.Sequential)]
    public struct RECT { public int Left, Top, Right, Bottom; }
    [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr hWnd, out RECT rect);
}
"@
    # FieldX/FieldY are client-area coordinates; convert so the varying
    # window frame size cannot push the click outside the header control.
    $pt = New-Object PerfWin+POINT
    $pt.X = $FieldX; $pt.Y = $FieldY
    [PerfWin]::ClientToScreen($hwnd, [ref]$pt) | Out-Null
    Set-Clipboard -Value $ScanPath

    # Click/paste/Enter can silently miss when the desktop steals foreground
    # during automation, so retry until the process actually burns CPU.
    $scanStart = $null
    for ($attempt = 1; $attempt -le 6; $attempt++) {
        [PerfWin]::SetForegroundWindow($hwnd) | Out-Null
        Start-Sleep -Milliseconds 300
        [PerfWin]::SetCursorPos($pt.X, $pt.Y) | Out-Null
        Start-Sleep -Milliseconds 150
        [PerfWin]::mouse_event(0x0002, 0, 0, 0, [UIntPtr]::Zero)
        Start-Sleep -Milliseconds 70
        [PerfWin]::mouse_event(0x0004, 0, 0, 0, [UIntPtr]::Zero)
        Start-Sleep -Milliseconds 400
        [System.Windows.Forms.SendKeys]::SendWait('^a')
        Start-Sleep -Milliseconds 100
        # SendKeys can lose the backslash with some keyboard layouts, so paste
        # the path through the clipboard instead of typing it key by key.
        [System.Windows.Forms.SendKeys]::SendWait('^v')
        Start-Sleep -Milliseconds 300
        # Baseline before Enter so a fast scan's CPU is still captured.
        $scanStart = Sample $p
        [System.Windows.Forms.SendKeys]::SendWait('{ENTER}')
        Start-Sleep -Milliseconds 900
        $probe = Sample $p
        if (($probe.CpuMs - $scanStart.CpuMs) -gt 5.0) { break }
        Write-Output "scan attempt $attempt did not engage; retrying"
    }
    Start-Sleep -Milliseconds 400
    # Diagnostic: capture the window right after paste+Enter to verify the
    # path field actually received focus and the pasted text.
    $r = New-Object PerfWin+RECT
    [PerfWin]::GetWindowRect($hwnd, [ref]$r) | Out-Null
    $pw = $r.Right - $r.Left; $ph = $r.Bottom - $r.Top
    $bmp2 = New-Object System.Drawing.Bitmap $pw, $ph
    $g2 = [System.Drawing.Graphics]::FromImage($bmp2)
    $hdc2 = $g2.GetHdc()
    [PerfWin]::PrintWindow($hwnd, $hdc2, 0) | Out-Null
    $g2.ReleaseHdc($hdc2)
    $diagShot = Join-Path (Split-Path $Shot -Parent) 'ui-scan-after-paste.png'
    $bmp2.Save($diagShot, [System.Drawing.Imaging.ImageFormat]::Png)
    $g2.Dispose(); $bmp2.Dispose()

    $scanWall = [Diagnostics.Stopwatch]::StartNew()
    $peak = $scanStart.WorkingSet
    $quietTicks = 0
    $lastCpu = $scanStart.CpuMs
    $scanCpuPercent = 0.0
    while ($scanWall.Elapsed.TotalSeconds -lt 150) {
        Start-Sleep -Milliseconds 250
        $sample = Sample $p
        if ($sample.WorkingSet -gt $peak) { $peak = $sample.WorkingSet }
        $delta = $sample.CpuMs - $lastCpu
        $lastCpu = $sample.CpuMs
        $util = ($delta / 250.0) * 100.0
        if ($util -gt $scanCpuPercent) { $scanCpuPercent = $util }
        if ($util -lt 2.0) { $quietTicks++ } else { $quietTicks = 0 }
        if ($scanWall.Elapsed.TotalSeconds -gt 3 -and $quietTicks -ge 8) { break }
    }
    $scanWall.Stop()
    $scanEnd = Sample $p
    $scan = [pscustomobject]@{
        Seconds = $scanWall.Elapsed.TotalSeconds
        CpuMs = $scanEnd.CpuMs - $scanStart.CpuMs
        PeakWorkingSet = $peak
        CpuPercentPeak = $scanCpuPercent
    }

    Start-Sleep -Milliseconds 600
    if (-not (Test-Path (Split-Path $Shot -Parent))) {
        New-Item -ItemType Directory -Path (Split-Path $Shot -Parent) -Force | Out-Null
    }
    $p.Refresh()
    $h = $hwnd
    [PerfWin]::GetWindowRect($h, [ref]$r) | Out-Null
    $w = $r.Right - $r.Left; $ht = $r.Bottom - $r.Top
    $bmp = New-Object System.Drawing.Bitmap $w, $ht
    $g = [System.Drawing.Graphics]::FromImage($bmp)
    $hdc = $g.GetHdc()
    [PerfWin]::PrintWindow($h, $hdc, 0) | Out-Null
    $g.ReleaseHdc($hdc)
    $bmp.Save($Shot, [System.Drawing.Imaging.ImageFormat]::Png)
    $g.Dispose(); $bmp.Dispose()
}

$postIdle = Sample $p
$dbDir = Join-Path $env:APPDATA 'SmartDisk'
$dbBytes = 0
Get-ChildItem -Path $dbDir -Filter 'smartdisk.db*' -ErrorAction SilentlyContinue | ForEach-Object { $dbBytes += $_.Length }

$p.CloseMainWindow() | Out-Null
if (-not $p.WaitForExit(10000)) { $p.Kill(); $p.WaitForExit() }

$report = @()
$report += '# SmartDisk application measurements'
$report += ''
$report += "- Date: $(Get-Date -Format 'yyyy-MM-dd HH:mm:ss')"
$report += "- Executable: $exePath"
$report += "- Renderer: winit-software (SLINT_BACKEND=winit-software)"
$report += "- Startup to responsive main window: $([math]::Round($startupMs, 1)) ms"
$report += "- Idle working set: $([math]::Round($idle1.WorkingSet / 1MB, 1)) MB (private: $([math]::Round($idle1.Private / 1MB, 1)) MB)"
$report += "- Idle CPU over ${IdleSeconds}s: $([math]::Round($idleCpuPercent, 2))% of one logical core"
$report += "- Process peak working set during session: $([math]::Round($postIdle.PeakWorkingSet / 1MB, 1)) MB"
if ($scan) {
    $report += "- UI scan path: $ScanPath"
    $report += "- UI scan wall time until idle: $([math]::Round($scan.Seconds, 2)) s"
    $report += "- UI scan CPU: $([math]::Round($scan.CpuMs, 0)) ms total; peak utilization $([math]::Round($scan.CpuPercentPeak, 0))% of one core"
    $report += "- UI scan peak working set: $([math]::Round($scan.PeakWorkingSet / 1MB, 1)) MB"
    $report += "- UI scan screenshot: $Shot"
}
$report += "- Database size (db + wal + shm): $([math]::Round($dbBytes / 1024.0, 1)) KiB at $dbDir"
$report | ForEach-Object { Write-Output $_ }
New-Item -ItemType Directory -Path (Split-Path $Out -Parent) -Force | Out-Null
$report | Set-Content -Path $Out -Encoding UTF8
Write-Output "saved=$Out"
