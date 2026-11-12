# Capture the Overview page in dark theme for visual verification.
# Toggles the Settings "Dark theme" checkbox on and back off so the
# persisted user setting is left unchanged.
# Usage: powershell -NoProfile -File tools/shoot_dark.ps1 [-Exe target\release\smartdisk.exe]
param(
    [string]$Exe = 'target\release\smartdisk.exe',
    [string]$ScanPath = 'D:\sdbench',
    [string]$OutDir = 'target/perf'
)
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.Windows.Forms
Add-Type -AssemblyName System.Drawing
Add-Type -TypeDefinition @"
using System;
using System.Runtime.InteropServices;
public class ShotDark {
    [StructLayout(LayoutKind.Sequential)]
    public struct RECT { public int Left, Top, Right, Bottom; }
    [StructLayout(LayoutKind.Sequential)]
    public struct POINT { public int X, Y; }
    [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr hWnd, out RECT rect);
    [DllImport("user32.dll")] public static extern bool SetCursorPos(int x, int y);
    [DllImport("user32.dll")] public static extern void mouse_event(uint flags, uint dx, uint dy, uint data, UIntPtr extra);
    [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr hWnd);
    [DllImport("user32.dll")] public static extern bool PrintWindow(IntPtr hWnd, IntPtr hdc, uint flags);
    [DllImport("user32.dll")] public static extern bool ClientToScreen(IntPtr hWnd, ref POINT pt);
}
"@

function Click($hwnd, $x, $y) {
    [ShotDark]::SetForegroundWindow($hwnd) | Out-Null
    Start-Sleep -Milliseconds 150
    $pt = New-Object ShotDark+POINT
    $pt.X = $x; $pt.Y = $y
    [ShotDark]::ClientToScreen($hwnd, [ref]$pt) | Out-Null
    [ShotDark]::SetCursorPos($pt.X, $pt.Y) | Out-Null
    Start-Sleep -Milliseconds 120
    [ShotDark]::mouse_event(0x0002, 0, 0, 0, [UIntPtr]::Zero)
    Start-Sleep -Milliseconds 60
    [ShotDark]::mouse_event(0x0004, 0, 0, 0, [UIntPtr]::Zero)
    Start-Sleep -Milliseconds 350
}

function Shoot($hwnd, $name) {
    $r = New-Object ShotDark+RECT
    [ShotDark]::GetWindowRect($hwnd, [ref]$r) | Out-Null
    $w = $r.Right - $r.Left; $h = $r.Bottom - $r.Top
    $bmp = New-Object System.Drawing.Bitmap $w, $h
    $g = [System.Drawing.Graphics]::FromImage($bmp)
    $hdc = $g.GetHdc()
    [ShotDark]::PrintWindow($hwnd, $hdc, 0) | Out-Null
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
        $cr = New-Object ShotDark+RECT
        if ([ShotDark]::GetWindowRect($c, [ref]$cr)) {
            if (($cr.Right - $cr.Left) -gt 400 -and ($cr.Bottom - $cr.Top) -gt 300 -and $p.MainWindowTitle) {
                $hwnd = $c; break
            }
        }
    }
    Start-Sleep -Milliseconds 50
}
if ($hwnd -eq [IntPtr]::Zero) { throw 'no main window' }
Start-Sleep -Seconds 3

# Scan so the Overview page has content.
Click $hwnd 620 32
[System.Windows.Forms.SendKeys]::SendWait('^a')
Start-Sleep -Milliseconds 100
Set-Clipboard -Value $ScanPath
[System.Windows.Forms.SendKeys]::SendWait('^v')
Start-Sleep -Milliseconds 300
[System.Windows.Forms.SendKeys]::SendWait('{ENTER}')
Start-Sleep -Seconds 3

# Settings page, toggle dark theme on and apply it via "Save settings"
# (client coords; the checkbox sits at about y=191 and the save button
# at about y=95 once the title bar is excluded).
Click $hwnd 100 495
Start-Sleep -Milliseconds 400
Click $hwnd 1355 191
Start-Sleep -Milliseconds 300
Click $hwnd 1315 95
Start-Sleep -Milliseconds 400
Shoot $hwnd 'ui-dark-settings'

# Back to Overview and capture.
Click $hwnd 100 117
Start-Sleep -Milliseconds 400
Shoot $hwnd 'ui-dark'

# Restore the original theme (toggle off, save) and close.
Click $hwnd 100 495
Start-Sleep -Milliseconds 400
Click $hwnd 1355 191
Start-Sleep -Milliseconds 300
Click $hwnd 1315 95
Start-Sleep -Milliseconds 400

$p.CloseMainWindow() | Out-Null
if (-not $p.WaitForExit(10000)) { $p.Kill(); $p.WaitForExit() }
