$ErrorActionPreference = 'Stop'
$env:SLINT_BACKEND = 'winit-software'
$exe = Join-Path (Get-Location) 'target\debug\smartdisk.exe'
$p = Start-Process -FilePath $exe -PassThru
Start-Sleep -Seconds 6
Add-Type -TypeDefinition @"
using System;
using System.Runtime.InteropServices;
public class WinProbe {
    [StructLayout(LayoutKind.Sequential)]
    public struct RECT { public int Left, Top, Right, Bottom; }
    [DllImport("user32.dll", SetLastError=true)] public static extern bool GetWindowRect(IntPtr hWnd, out RECT rect);
    [DllImport("user32.dll")] public static extern bool IsWindow(IntPtr hWnd);
    [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr hWnd);
    [DllImport("user32.dll", SetLastError=true)] public static extern bool SetWindowPos(IntPtr hWnd, IntPtr after, int x, int y, int cx, int cy, uint flags);
}
"@
$p.Refresh()
$h = $p.MainWindowHandle
Write-Output ("pid=" + $p.Id + " handle=" + $h + " exited=" + $p.HasExited)
Write-Output ("iswindow=" + [WinProbe]::IsWindow($h) + " visible=" + [WinProbe]::IsWindowVisible($h))
$r = New-Object WinProbe+RECT
$ok = [WinProbe]::GetWindowRect($h, [ref]$r)
Write-Output ("getrect=" + $ok + " err=" + [Runtime.InteropServices.Marshal]::GetLastWin32Error() + " rect=" + $r.Left + "," + $r.Top + "," + $r.Right + "," + $r.Bottom)
$ok2 = [WinProbe]::SetWindowPos($h, [IntPtr]::Zero, 200, 100, 1300, 820, 0x0004)
Write-Output ("setpos=" + $ok2 + " err=" + [Runtime.InteropServices.Marshal]::GetLastWin32Error())
Start-Sleep -Seconds 1
$r2 = New-Object WinProbe+RECT
$ok3 = [WinProbe]::GetWindowRect($h, [ref]$r2)
Write-Output ("getrect2=" + $ok3 + " err=" + [Runtime.InteropServices.Marshal]::GetLastWin32Error() + " rect=" + $r2.Left + "," + $r2.Top + "," + $r2.Right + "," + $r2.Bottom)
Write-Output ("still=" + (-not $p.HasExited))
