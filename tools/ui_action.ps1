param(
    [int]$TargetPid,
    [int]$X = -1,
    [int]$Y = -1,
    [string]$Text = '',
    [switch]$Enter,
    [switch]$DoubleClick,
    [switch]$RightClick,
    [int]$Wheel = 0,
    [int]$WaitMs = 700,
    [string]$Out = ''
)
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.Windows.Forms
Add-Type -AssemblyName System.Drawing
Add-Type -TypeDefinition @"
using System;
using System.Runtime.InteropServices;
public class UiAction {
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
[UiAction]::SetForegroundWindow($p.MainWindowHandle) | Out-Null
Start-Sleep -Milliseconds 400
$p.Refresh()
$r = New-Object UiAction+RECT
[UiAction]::GetWindowRect($p.MainWindowHandle, [ref]$r) | Out-Null
if ($X -ge 0 -and $Y -ge 0) {
    [UiAction]::SetCursorPos(($r.Left + $X), ($r.Top + $Y)) | Out-Null
    Start-Sleep -Milliseconds 150
    if ($RightClick) {
        [UiAction]::mouse_event(0x0008, 0, 0, 0, [UIntPtr]::Zero)
        Start-Sleep -Milliseconds 70
        [UiAction]::mouse_event(0x0010, 0, 0, 0, [UIntPtr]::Zero)
    } else {
        [UiAction]::mouse_event(0x0002, 0, 0, 0, [UIntPtr]::Zero)
        Start-Sleep -Milliseconds 70
        [UiAction]::mouse_event(0x0004, 0, 0, 0, [UIntPtr]::Zero)
    }
    if ($DoubleClick) {
        Start-Sleep -Milliseconds 80
        [UiAction]::mouse_event(0x0002, 0, 0, 0, [UIntPtr]::Zero)
        Start-Sleep -Milliseconds 60
        [UiAction]::mouse_event(0x0004, 0, 0, 0, [UIntPtr]::Zero)
    }
    Start-Sleep -Milliseconds 300
}
if ($Wheel -ne 0) {
    if ($X -lt 0) {
        [UiAction]::SetCursorPos(($r.Left + 900), ($r.Top + 500)) | Out-Null
    }
    if ($Wheel -gt 0) { $delta32 = [uint32]120 } else { $delta32 = [uint32]4294967176 }
    for ($i = 0; $i -lt [Math]::Abs($Wheel); $i++) {
        [UiAction]::mouse_event(0x0800, 0, 0, $delta32, [UIntPtr]::Zero)
        Start-Sleep -Milliseconds 80
    }
}
if ($Text) {
    Start-Sleep -Milliseconds 150
    [System.Windows.Forms.SendKeys]::SendWait($Text)
    Start-Sleep -Milliseconds 250
}
if ($Enter) {
    [System.Windows.Forms.SendKeys]::SendWait('{ENTER}')
    Start-Sleep -Milliseconds 300
}
Start-Sleep -Milliseconds $WaitMs
if ($Out) {
    $p.Refresh()
    $h = $p.MainWindowHandle
    $rr = New-Object UiAction+RECT
    [UiAction]::GetWindowRect($h, [ref]$rr) | Out-Null
    $w = $rr.Right - $rr.Left; $ht = $rr.Bottom - $rr.Top
    $bmp = New-Object System.Drawing.Bitmap $w, $ht
    $g = [System.Drawing.Graphics]::FromImage($bmp)
    $hdc = $g.GetHdc()
    [UiAction]::PrintWindow($h, $hdc, 0) | Out-Null
    $g.ReleaseHdc($hdc)
    $bmp.Save($Out, [System.Drawing.Imaging.ImageFormat]::Png)
    $g.Dispose(); $bmp.Dispose()
    Write-Output ('saved=' + $Out)
}
Write-Output 'ok'
