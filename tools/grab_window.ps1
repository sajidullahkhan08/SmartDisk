param([int]$ProcessId, [string]$Out)
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.Drawing
Add-Type -TypeDefinition @"
using System;
using System.Runtime.InteropServices;
public class WinGrab {
    [StructLayout(LayoutKind.Sequential)]
    public struct RECT { public int Left, Top, Right, Bottom; }
    [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr hWnd, out RECT rect);
    [DllImport("user32.dll")] public static extern bool PrintWindow(IntPtr hWnd, IntPtr hdc, uint flags);
    [DllImport("user32.dll")] public static extern bool SetWindowPos(IntPtr hWnd, IntPtr after, int x, int y, int cx, int cy, uint flags);
    [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr hWnd);
}
"@
$p = Get-Process -Id $ProcessId
$h = $p.MainWindowHandle
Write-Output ("handle=" + $h)
[WinGrab]::SetWindowPos($h, [IntPtr]-1, 0, 0, 0, 0, 0x0001 -bor 0x0002 -bor 0x0040) | Out-Null
[WinGrab]::SetForegroundWindow($h) | Out-Null
Start-Sleep -Seconds 2
$r = New-Object WinGrab+RECT
$ok = [WinGrab]::GetWindowRect($h, [ref]$r)
Write-Output ("getrect=" + $ok)
$w = $r.Right - $r.Left
$hgt = $r.Bottom - $r.Top
Write-Output ("rect=" + $r.Left + "," + $r.Top + " " + $w + "x" + $hgt)
$bmp = New-Object System.Drawing.Bitmap $w, $hgt
$g = [System.Drawing.Graphics]::FromImage($bmp)
$hdc = $g.GetHdc()
$pw = [WinGrab]::PrintWindow($h, $hdc, 0)
$g.ReleaseHdc($hdc)
$bmp.Save($Out, [System.Drawing.Imaging.ImageFormat]::Png)
$g.Dispose(); $bmp.Dispose()
Write-Output ("saved=" + $Out + " printwindow=" + $pw)
