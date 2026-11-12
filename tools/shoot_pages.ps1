# Capture screenshots of SmartDisk pages for visual verification.
# Usage: powershell -NoProfile -File tools/shoot_pages.ps1 [-Exe target\release\smartdisk.exe]
param(
    [string]$Exe = 'target\release\smartdisk.exe',
    [string]$ScanPath = 'D:\sdui',
    [string]$OutDir = 'target/perf'
)
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.Windows.Forms
Add-Type -AssemblyName System.Drawing
Add-Type -TypeDefinition @"
using System;
using System.Runtime.InteropServices;
public class ShotWin {
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

function Click($hwnd, $x, $y) {
    # x/y are client-area coordinates, converted to screen via ClientToScreen.
    [ShotWin]::SetForegroundWindow($hwnd) | Out-Null
    Start-Sleep -Milliseconds 150
    $pt = New-Object ShotWin+POINT
    $pt.X = $x; $pt.Y = $y
    [ShotWin]::ClientToScreen($hwnd, [ref]$pt) | Out-Null
    [ShotWin]::SetCursorPos($pt.X, $pt.Y) | Out-Null
    Start-Sleep -Milliseconds 120
    [ShotWin]::mouse_event(0x0002, 0, 0, 0, [UIntPtr]::Zero)
    Start-Sleep -Milliseconds 60
    [ShotWin]::mouse_event(0x0004, 0, 0, 0, [UIntPtr]::Zero)
    Start-Sleep -Milliseconds 350
}

function Shoot($hwnd, $name) {
    $r = New-Object ShotWin+RECT
    [ShotWin]::GetWindowRect($hwnd, [ref]$r) | Out-Null
    $w = $r.Right - $r.Left; $h = $r.Bottom - $r.Top
    $bmp = New-Object System.Drawing.Bitmap $w, $h
    $g = [System.Drawing.Graphics]::FromImage($bmp)
    $hdc = $g.GetHdc()
    [ShotWin]::PrintWindow($hwnd, $hdc, 0) | Out-Null
    $g.ReleaseHdc($hdc)
    $out = "$OutDir/$name.png"
    $bmp.Save($out, [System.Drawing.Imaging.ImageFormat]::Png)
    $g.Dispose(); $bmp.Dispose()
    Write-Output "saved=$out"
}

$exePath = (Resolve-Path $Exe).Path
$env:SLINT_BACKEND = 'winit-software'
$p = Start-Process -FilePath $exePath -PassThru
$hwnd = [IntPtr]::Zero
$sw = [Diagnostics.Stopwatch]::StartNew()
while ($sw.Elapsed.TotalSeconds -lt 25) {
    $p.Refresh()
    if ($p.HasExited) { throw "exited early $($p.ExitCode)" }
    $c = $p.MainWindowHandle
    if ($c -ne 0 -and $p.Responding) {
        $cr = New-Object ShotWin+RECT
        if ([ShotWin]::GetWindowRect($c, [ref]$cr)) {
            if (($cr.Right - $cr.Left) -gt 400 -and ($cr.Bottom - $cr.Top) -gt 300 -and $p.MainWindowTitle) {
                $hwnd = $c; break
            }
        }
    }
    Start-Sleep -Milliseconds 50
}
if ($hwnd -eq [IntPtr]::Zero) { throw 'no main window' }
Start-Sleep -Seconds 3

# Start a scan so content pages have data.
Click $hwnd 620 32
[System.Windows.Forms.SendKeys]::SendWait('^a')
Start-Sleep -Milliseconds 100
Set-Clipboard -Value $ScanPath
[System.Windows.Forms.SendKeys]::SendWait('^v')
Start-Sleep -Milliseconds 300
[System.Windows.Forms.SendKeys]::SendWait('{ENTER}')
Start-Sleep -Seconds 3
Shoot $hwnd 'ui-overview'

Click $hwnd 100 155; Shoot $hwnd 'ui-explorer'
Click $hwnd 100 195; Shoot $hwnd 'ui-treemap'
Click $hwnd 100 495; Shoot $hwnd 'ui-settings'

$p.CloseMainWindow() | Out-Null
if (-not $p.WaitForExit(10000)) { $p.Kill(); $p.WaitForExit() }
