param([string]$Out)
$ErrorActionPreference = 'Stop'
$env:SLINT_BACKEND = 'winit-software'
$exe = Join-Path (Get-Location) 'target\debug\smartdisk.exe'
$p = Start-Process -FilePath $exe -PassThru
Start-Sleep -Seconds 7
if ($p.HasExited) { Write-Output ("EXITED code=" + $p.ExitCode); exit 1 }
$p.Refresh()
Write-Output ("pid=" + $p.Id + " mem=" + [math]::Round($p.WorkingSet64/1MB,1) + " title=" + $p.MainWindowTitle)
Add-Type -AssemblyName System.Drawing
Add-Type -TypeDefinition @"
using System;
using System.Runtime.InteropServices;
public class WinCapDefault {
    [StructLayout(LayoutKind.Sequential)]
    public struct RECT { public int Left, Top, Right, Bottom; }
    [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr hWnd, out RECT rect);
    [DllImport("user32.dll")] public static extern bool PrintWindow(IntPtr hWnd, IntPtr hdc, uint flags);
}
"@
$h = $p.MainWindowHandle
$r = New-Object WinCapDefault+RECT
[WinCapDefault]::GetWindowRect($h, [ref]$r) | Out-Null
$w = $r.Right - $r.Left; $hgt = $r.Bottom - $r.Top
Write-Output ("rect=" + $r.Left + "," + $r.Top + " " + $w + "x" + $hgt)
$bmp = New-Object System.Drawing.Bitmap $w, $hgt
$g = [System.Drawing.Graphics]::FromImage($bmp)
$hdc = $g.GetHdc()
$ok = [WinCapDefault]::PrintWindow($h, $hdc, 0)
$g.ReleaseHdc($hdc)
$bmp.Save($Out, [System.Drawing.Imaging.ImageFormat]::Png)
$g.Dispose(); $bmp.Dispose()
Write-Output ("saved=" + $Out + " pw=" + $ok)
