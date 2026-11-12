param([int]$TargetPid, [string]$OutDir, [string]$Pages = 'treemap,largest,types,cleanup,duplicates,search,history,settings')
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.Windows.Forms
Add-Type -AssemblyName System.Drawing
Add-Type -TypeDefinition @"
using System;
using System.Runtime.InteropServices;
public class UiNav {
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
[UiNav]::SetForegroundWindow($p.MainWindowHandle) | Out-Null
Start-Sleep -Milliseconds 600
$p.Refresh()
$r = New-Object UiNav+RECT
[UiNav]::GetWindowRect($p.MainWindowHandle, [ref]$r) | Out-Null
$ox = $r.Left; $oy = $r.Top
function ClickRel([int]$x, [int]$y) {
    [UiNav]::SetCursorPos($ox + $x, $oy + $y + 36) | Out-Null
    Start-Sleep -Milliseconds 100
    [UiNav]::mouse_event(0x0002, 0, 0, 0, [UIntPtr]::Zero)
    Start-Sleep -Milliseconds 70
    [UiNav]::mouse_event(0x0004, 0, 0, 0, [UIntPtr]::Zero)
    Start-Sleep -Milliseconds 500
}
function Shot([string]$name) {
    $p2 = Get-Process -Id $TargetPid
    $p2.Refresh()
    $h = $p2.MainWindowHandle
    $rr = New-Object UiNav+RECT
    [UiNav]::GetWindowRect($h, [ref]$rr) | Out-Null
    $w = $rr.Right - $rr.Left; $ht = $rr.Bottom - $rr.Top
    $bmp = New-Object System.Drawing.Bitmap $w, $ht
    $g = [System.Drawing.Graphics]::FromImage($bmp)
    $hdc = $g.GetHdc()
    [UiNav]::PrintWindow($h, $hdc, 0) | Out-Null
    $g.ReleaseHdc($hdc)
    $path = Join-Path $OutDir ('page-' + $name + '.png')
    $bmp.Save($path, [System.Drawing.Imaging.ImageFormat]::Png)
    $g.Dispose(); $bmp.Dispose()
    Write-Output ('shot=' + $path)
}
$coords = @{
    overview = @(101,114); explorer = @(89,150); treemap = @(90,182); largest = @(89,218);
    types = @(93,254); cleanup = @(95,315); duplicates = @(89,351); search = @(85,388);
    history = @(85,424); settings = @(84,461)
}
foreach ($name in $Pages.Split(',')) {
    $name = $name.Trim()
    $c = $coords[$name]
    if (-not $c) { Write-Output ('unknown page ' + $name); continue }
    ClickRel -x $c[0] -y $c[1]
    Start-Sleep -Milliseconds 500
    Shot -name $name
}
