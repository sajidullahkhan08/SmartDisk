param([string]$Out)
$ErrorActionPreference = 'Stop'
$env:SLINT_BACKEND = 'winit-software'
$exe = Join-Path (Get-Location) 'target\debug\smartdisk.exe'
$p = Start-Process -FilePath $exe -PassThru
Start-Sleep -Seconds 8
if ($p.HasExited) {
    Write-Output ("EXITED code=" + $p.ExitCode)
    exit 1
}
$p.Refresh()
Write-Output ("RUNNING pid=" + $p.Id + " mem=" + [math]::Round($p.WorkingSet64/1MB,1) + " title=" + $p.MainWindowTitle)

Add-Type -AssemblyName System.Drawing
Add-Type -TypeDefinition @"
using System;
using System.Runtime.InteropServices;
public class WinShot {
    [StructLayout(LayoutKind.Sequential)]
    public struct RECT { public int Left, Top, Right, Bottom; }
    [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr hWnd, out RECT rect);
    [DllImport("user32.dll")] public static extern bool PrintWindow(IntPtr hWnd, IntPtr hdc, uint flags);
    [DllImport("user32.dll")] public static extern bool ShowWindow(IntPtr hWnd, int cmd);
}
"@
$h = $p.MainWindowHandle
[WinShot]::ShowWindow($h, 9) | Out-Null
Start-Sleep -Seconds 1
$r = New-Object WinShot+RECT
[WinShot]::GetWindowRect($h, [ref]$r) | Out-Null
$w = $r.Right - $r.Left
$hgt = $r.Bottom - $r.Top
$bmp = New-Object System.Drawing.Bitmap $w, $hgt
$g = [System.Drawing.Graphics]::FromImage($bmp)
$hdc = $g.GetHdc()
$ok = [WinShot]::PrintWindow($h, $hdc, 0)
$g.ReleaseHdc($hdc)
$bmp.Save($Out, [System.Drawing.Imaging.ImageFormat]::Png)
$g.Dispose(); $bmp.Dispose()
Write-Output ("saved=" + $Out + " printwindow=" + $ok + " pid=" + $p.Id)
