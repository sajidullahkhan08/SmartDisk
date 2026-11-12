# Capture the initial empty state (no scan) for visual verification.
# Usage: powershell -NoProfile -File tools/shoot_empty.ps1 [-Exe target\release\smartdisk.exe]
param(
    [string]$Exe = 'target\release\smartdisk.exe',
    [string]$OutDir = 'target/perf'
)
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.Drawing
Add-Type -TypeDefinition @"
using System;
using System.Runtime.InteropServices;
public class ShotEmpty {
    [StructLayout(LayoutKind.Sequential)]
    public struct RECT { public int Left, Top, Right, Bottom; }
    [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr hWnd, out RECT rect);
    [DllImport("user32.dll")] public static extern bool PrintWindow(IntPtr hWnd, IntPtr hdc, uint flags);
}
"@

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
        $cr = New-Object ShotEmpty+RECT
        if ([ShotEmpty]::GetWindowRect($c, [ref]$cr)) {
            if (($cr.Right - $cr.Left) -gt 400 -and ($cr.Bottom - $cr.Top) -gt 300 -and $p.MainWindowTitle) {
                $hwnd = $c; break
            }
        }
    }
    Start-Sleep -Milliseconds 50
}
if ($hwnd -eq [IntPtr]::Zero) { throw 'no main window' }
Start-Sleep -Seconds 3

$r = New-Object ShotEmpty+RECT
[ShotEmpty]::GetWindowRect($hwnd, [ref]$r) | Out-Null
$w = $r.Right - $r.Left; $h = $r.Bottom - $r.Top
$bmp = New-Object System.Drawing.Bitmap $w, $h
$g = [System.Drawing.Graphics]::FromImage($bmp)
$hdc = $g.GetHdc()
[ShotEmpty]::PrintWindow($hwnd, $hdc, 0) | Out-Null
$g.ReleaseHdc($hdc)
$out = "$OutDir/ui-empty.png"
$bmp.Save($out, [System.Drawing.Imaging.ImageFormat]::Png)
$g.Dispose(); $bmp.Dispose()
Write-Output "saved=$out"

$p.CloseMainWindow() | Out-Null
if (-not $p.WaitForExit(10000)) { $p.Kill(); $p.WaitForExit() }
