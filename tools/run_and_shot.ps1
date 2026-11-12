param([string]$Out, [int]$W = 1400, [int]$H = 880)
$ErrorActionPreference = 'Stop'
$env:SLINT_BACKEND = 'winit-software'
$exe = Join-Path (Get-Location) 'target\debug\smartdisk.exe'
$p = Start-Process -FilePath $exe -PassThru
Start-Sleep -Seconds 6
if ($p.HasExited) { Write-Output ("EXITED code=" + $p.ExitCode); exit 1 }

Add-Type -AssemblyName System.Drawing
Add-Type -TypeDefinition @"
using System;
using System.Runtime.InteropServices;
public class WinShot3 {
    [StructLayout(LayoutKind.Sequential)]
    public struct RECT { public int Left, Top, Right, Bottom; }
    [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr hWnd, out RECT rect);
    [DllImport("user32.dll")] public static extern bool GetClientRect(IntPtr hWnd, out RECT rect);
    [DllImport("user32.dll")] public static extern bool PrintWindow(IntPtr hWnd, IntPtr hdc, uint flags);
    [DllImport("user32.dll")] public static extern bool SetWindowPos(IntPtr hWnd, IntPtr after, int x, int y, int cx, int cy, uint flags);
    [DllImport("user32.dll")] public static extern bool ShowWindow(IntPtr hWnd, int cmd);
}
"@
$p.Refresh()
$h = $p.MainWindowHandle
Write-Output ("pid=" + $p.Id + " handle=" + $h + " title=" + $p.MainWindowTitle)
if ($h -eq 0) { Write-Output 'NO WINDOW'; exit 2 }
[WinShot3]::ShowWindow($h, 9) | Out-Null
[WinShot3]::SetWindowPos($h, [IntPtr]0, 100, 60, $W, $H, 0x0004) | Out-Null
Start-Sleep -Seconds 2

$r = New-Object WinShot3+RECT
[WinShot3]::GetWindowRect($h, [ref]$r) | Out-Null
$c = New-Object WinShot3+RECT
[WinShot3]::GetClientRect($h, [ref]$c) | Out-Null
Write-Output ("window=" + $r.Left + "," + $r.Top + " " + ($r.Right-$r.Left) + "x" + ($r.Bottom-$r.Top) + " client=" + ($c.Right-$c.Left) + "x" + ($c.Bottom-$c.Top))
$w = $r.Right - $r.Left
$hgt = $r.Bottom - $r.Top
if ($w -le 0) { Write-Output 'BAD RECT'; exit 3 }

$bmp = New-Object System.Drawing.Bitmap $w, $hgt
$g = [System.Drawing.Graphics]::FromImage($bmp)
$hdc = $g.GetHdc()
$ok = [WinShot3]::PrintWindow($h, $hdc, 0)
$g.ReleaseHdc($hdc)
$bmp.Save($Out, [System.Drawing.Imaging.ImageFormat]::Png)
$g.Dispose(); $bmp.Dispose()
Write-Output ("saved=" + $Out + " printwindow=" + $ok)
