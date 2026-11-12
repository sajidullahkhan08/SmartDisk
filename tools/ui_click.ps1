param([int]$TargetPid, [int]$X, [int]$Y, [string]$Out, [int]$WaitMs = 1200, [int]$OffsetY = 32)
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.Drawing
Add-Type -TypeDefinition @"
using System;
using System.Runtime.InteropServices;
public class UiClick {
    [StructLayout(LayoutKind.Sequential)]
    public struct RECT { public int Left, Top, Right, Bottom; }
    [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr hWnd, out RECT rect);
    [DllImport("user32.dll")] public static extern bool SetCursorPos(int x, int y);
    [DllImport("user32.dll")] public static extern void mouse_event(uint flags, uint dx, uint dy, uint data, UIntPtr extra);
    [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr hWnd);
    [DllImport("user32.dll")] public static extern bool PrintWindow(IntPtr hWnd, IntPtr hdc, uint flags);
}
"@
$p = Get-Process -Id $TargetPid
try { (New-Object -ComObject WScript.Shell).AppActivate($TargetPid) | Out-Null } catch {}
[UiClick]::SetForegroundWindow($p.MainWindowHandle) | Out-Null
Start-Sleep -Milliseconds 400
$p.Refresh()
$r = New-Object UiClick+RECT
[UiClick]::GetWindowRect($p.MainWindowHandle, [ref]$r) | Out-Null
$ox = $r.Left; $oy = $r.Top
[UiClick]::SetCursorPos(($ox + $X), ($oy + $Y + $OffsetY)) | Out-Null
Start-Sleep -Milliseconds 150
[UiClick]::mouse_event(0x0002, 0, 0, 0, [UIntPtr]::Zero)
Start-Sleep -Milliseconds 80
[UiClick]::mouse_event(0x0004, 0, 0, 0, [UIntPtr]::Zero)
Start-Sleep -Milliseconds $WaitMs
if ($Out) {
    $p.Refresh()
    $h = $p.MainWindowHandle
    $rr = New-Object UiClick+RECT
    [UiClick]::GetWindowRect($h, [ref]$rr) | Out-Null
    $w = $rr.Right - $rr.Left; $ht = $rr.Bottom - $rr.Top
    $bmp = New-Object System.Drawing.Bitmap $w, $ht
    $g = [System.Drawing.Graphics]::FromImage($bmp)
    $hdc = $g.GetHdc()
    [UiClick]::PrintWindow($h, $hdc, 0) | Out-Null
    $g.ReleaseHdc($hdc)
    $bmp.Save($Out, [System.Drawing.Imaging.ImageFormat]::Png)
    $g.Dispose(); $bmp.Dispose()
    Write-Output ('saved=' + $Out)
}
Write-Output 'clicked'
