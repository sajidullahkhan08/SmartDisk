param([int]$TargetPid)
$ErrorActionPreference = 'Stop'
Add-Type -TypeDefinition @"
using System;
using System.Runtime.InteropServices;
public class ProbeFrame {
    [StructLayout(LayoutKind.Sequential)]
    public struct RECT { public int Left, Top, Right, Bottom; }
    [StructLayout(LayoutKind.Sequential)]
    public struct POINT { public int X, Y; }
    [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out RECT r);
    [DllImport("user32.dll")] public static extern bool GetClientRect(IntPtr h, out RECT r);
    [DllImport("user32.dll")] public static extern bool ClientToScreen(IntPtr h, ref POINT p);
    [DllImport("dwmapi.dll")] public static extern int DwmGetWindowAttribute(IntPtr h, int attr, out RECT r, int size);
    [DllImport("user32.dll")] public static extern uint GetDpiForWindow(IntPtr h);
}
"@
$p = Get-Process -Id $TargetPid
$h = $p.MainWindowHandle
$r = New-Object ProbeFrame+RECT
[ProbeFrame]::GetWindowRect($h, [ref]$r) | Out-Null
Write-Output ('GetWindowRect ' + $r.Left + ',' + $r.Top + ' - ' + $r.Right + ',' + $r.Bottom)
$e = New-Object ProbeFrame+RECT
[ProbeFrame]::DwmGetWindowAttribute($h, 9, [ref]$e, 16) | Out-Null
Write-Output ('ExtendedFrame ' + $e.Left + ',' + $e.Top + ' - ' + $e.Right + ',' + $e.Bottom)
$c = New-Object ProbeFrame+RECT
[ProbeFrame]::GetClientRect($h, [ref]$c) | Out-Null
$pt = New-Object ProbeFrame+POINT
[ProbeFrame]::ClientToScreen($h, [ref]$pt) | Out-Null
Write-Output ('Client ' + $c.Right + 'x' + $c.Bottom + ' at ' + $pt.X + ',' + $pt.Y)
Write-Output ('DPI ' + [ProbeFrame]::GetDpiForWindow($h))
