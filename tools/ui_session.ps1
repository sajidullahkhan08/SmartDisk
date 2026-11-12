param([int]$TargetPid, [string]$OutDir)
$ProcessId = $TargetPid
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.Windows.Forms
Add-Type -AssemblyName System.Drawing
Add-Type -TypeDefinition @"
using System;
using System.Runtime.InteropServices;
public class UiDrive {
    [StructLayout(LayoutKind.Sequential)]
    public struct RECT { public int Left, Top, Right, Bottom; }
    [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr hWnd, out RECT rect);
    [DllImport("user32.dll")] public static extern bool SetCursorPos(int x, int y);
    [DllImport("user32.dll")] public static extern void mouse_event(uint flags, uint dx, uint dy, uint data, UIntPtr extra);
    [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr hWnd);
    [DllImport("user32.dll")] public static extern bool PrintWindow(IntPtr hWnd, IntPtr hdc, uint flags);
}
"@
function Activate {
    param([int]$AppPid)
    try { (New-Object -ComObject WScript.Shell).AppActivate($AppPid) | Out-Null } catch {}
    $p = Get-Process -Id $AppPid
    [UiDrive]::SetForegroundWindow($p.MainWindowHandle) | Out-Null
    Start-Sleep -Milliseconds 500
}
function Click {
    param([int]$X, [int]$Y)
    [UiDrive]::SetCursorPos($X, $Y) | Out-Null
    Start-Sleep -Milliseconds 120
    [UiDrive]::mouse_event(0x0002, 0, 0, 0, [UIntPtr]::Zero)
    Start-Sleep -Milliseconds 80
    [UiDrive]::mouse_event(0x0004, 0, 0, 0, [UIntPtr]::Zero)
    Start-Sleep -Milliseconds 350
}
function Capture {
    param([string]$Path)
    $p = Get-Process -Id $ProcessId
    $p.Refresh()
    $h = $p.MainWindowHandle
    $r = New-Object UiDrive+RECT
    [UiDrive]::GetWindowRect($h, [ref]$r) | Out-Null
    $w = $r.Right - $r.Left; $ht = $r.Bottom - $r.Top
    $bmp = New-Object System.Drawing.Bitmap $w, $ht
    $g = [System.Drawing.Graphics]::FromImage($bmp)
    $hdc = $g.GetHdc()
    [UiDrive]::PrintWindow($h, $hdc, 0) | Out-Null
    $g.ReleaseHdc($hdc)
    $bmp.Save($Path, [System.Drawing.Imaging.ImageFormat]::Png)
    $g.Dispose(); $bmp.Dispose()
    $script:shotIndex++
    Write-Output ("shot=" + $Path)
}
$shotIndex = 0
Activate -AppPid $ProcessId
$p = Get-Process -Id $ProcessId
$r = New-Object UiDrive+RECT
[UiDrive]::GetWindowRect($p.MainWindowHandle, [ref]$r) | Out-Null
$ox = $r.Left; $oy = $r.Top
Write-Output ("window at " + $ox + "," + $oy)

# Click the path field, select everything, type the controlled test path, press Enter.
Click -X ($ox + 700) -Y ($oy + 45)
Start-Sleep -Milliseconds 400
[System.Windows.Forms.SendKeys]::SendWait('^a')
Start-Sleep -Milliseconds 200
[System.Windows.Forms.SendKeys]::SendWait('D:\SmartDiskUiTest')
Start-Sleep -Milliseconds 300
[System.Windows.Forms.SendKeys]::SendWait('{ENTER}')
Start-Sleep -Seconds 1
Capture -Path (Join-Path $OutDir 'ui-01-scanning.png')
Start-Sleep -Seconds 6
Capture -Path (Join-Path $OutDir 'ui-02-result.png')
Write-Output 'done'
