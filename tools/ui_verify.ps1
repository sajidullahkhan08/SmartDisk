param([int]$TargetPid, [string]$OutDir)
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.Windows.Forms
Add-Type -AssemblyName System.Drawing
Add-Type -TypeDefinition @"
using System;
using System.Runtime.InteropServices;
public class UiVerify {
    [StructLayout(LayoutKind.Sequential)]
    public struct RECT { public int Left, Top, Right, Bottom; }
    [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr hWnd, out RECT rect);
    [DllImport("user32.dll")] public static extern bool SetCursorPos(int x, int y);
    [DllImport("user32.dll")] public static extern void mouse_event(uint flags, uint dx, uint dy, uint data, UIntPtr extra);
    [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr hWnd);
    [DllImport("user32.dll")] public static extern bool PrintWindow(IntPtr hWnd, IntPtr hdc, uint flags);
}
"@

$script:p = Get-Process -Id $TargetPid
function Activate {
    try { (New-Object -ComObject WScript.Shell).AppActivate($TargetPid) | Out-Null } catch {}
    $script:p.Refresh()
    [UiVerify]::SetForegroundWindow($script:p.MainWindowHandle) | Out-Null
    Start-Sleep -Milliseconds 400
}
function Origin {
    $r = New-Object UiVerify+RECT
    [UiVerify]::GetWindowRect($script:p.MainWindowHandle, [ref]$r) | Out-Null
    return @($r.Left, $r.Top)
}
function Click([int]$x, [int]$y) {
    $o = Origin
    [UiVerify]::SetCursorPos(($o[0] + $x), ($o[1] + $y)) | Out-Null
    Start-Sleep -Milliseconds 120
    [UiVerify]::mouse_event(0x0002, 0, 0, 0, [UIntPtr]::Zero)
    Start-Sleep -Milliseconds 70
    [UiVerify]::mouse_event(0x0004, 0, 0, 0, [UIntPtr]::Zero)
    Start-Sleep -Milliseconds 350
}
function Shot([string]$name) {
    $script:p.Refresh()
    $h = $script:p.MainWindowHandle
    $rr = New-Object UiVerify+RECT
    [UiVerify]::GetWindowRect($h, [ref]$rr) | Out-Null
    $w = $rr.Right - $rr.Left; $ht = $rr.Bottom - $rr.Top
    $bmp = New-Object System.Drawing.Bitmap $w, $ht
    $g = [System.Drawing.Graphics]::FromImage($bmp)
    $hdc = $g.GetHdc()
    [UiVerify]::PrintWindow($h, $hdc, 0) | Out-Null
    $g.ReleaseHdc($hdc)
    $path = Join-Path $OutDir ('page-' + $name + '.png')
    $bmp.Save($path, [System.Drawing.Imaging.ImageFormat]::Png)
    $g.Dispose(); $bmp.Dispose()
    Write-Output ('shot=' + $name)
}
function FindAccentButton([string]$file, [int]$x0, [int]$x1, [int]$y0, [int]$y1) {
    $bmp = [System.Drawing.Bitmap]::FromFile($file)
    $minx = 99999; $miny = 99999; $maxx = 0; $maxy = 0
    $x1 = [Math]::Min($x1, $bmp.Width)
    $y1 = [Math]::Min($y1, $bmp.Height)
    for ($y = $y0; $y -lt $y1; $y++) {
        for ($x = $x0; $x -lt $x1; $x++) {
            $c = $bmp.GetPixel($x, $y)
            if ($c.B -gt 200 -and $c.R -lt 120 -and $c.G -gt 90 -and $c.G -lt 160) {
                if ($x -lt $minx) { $minx = $x }; if ($x -gt $maxx) { $maxx = $x }
                if ($y -lt $miny) { $miny = $y }; if ($y -gt $maxy) { $maxy = $y }
            }
        }
    }
    $bmp.Dispose()
    if ($maxx -eq 0) { return $null }
    return @([int](($minx + $maxx) / 2), [int](($miny + $maxy) / 2))
}

Activate
$scanBtn = FindAccentButton (Join-Path $OutDir 'launch.png') 1150 1420 20 80
Write-Output ('scan button=' + ($scanBtn -join ','))
Click $scanBtn[0] $scanBtn[1]
Start-Sleep -Seconds 1
Shot 'scanning'
Start-Sleep -Seconds 7
Shot 'overview'

$nav = @{
    explorer = 184; treemap = 222; largest = 260; types = 298;
    cleanup = 358; duplicates = 395; search = 434; history = 472; settings = 511
}
foreach ($name in @('treemap', 'largest', 'types', 'cleanup')) {
    Click 92 $nav[$name]
    Start-Sleep -Milliseconds 500
    Shot $name
}

Click 92 $nav['duplicates']
Start-Sleep -Milliseconds 500
Shot 'duplicates'
$dupBtn = FindAccentButton (Join-Path $OutDir 'page-duplicates.png') 1150 1420 90 170
Write-Output ('dup button=' + ($dupBtn -join ','))
if ($dupBtn) {
    Click $dupBtn[0] $dupBtn[1]
    Start-Sleep -Seconds 6
    Shot 'dup-results'
}

Click 92 $nav['search']
Start-Sleep -Milliseconds 500
Shot 'search'

Click 92 $nav['history']
Start-Sleep -Milliseconds 500
Shot 'history'

Click 92 $nav['settings']
Start-Sleep -Milliseconds 500
Shot 'settings'

Click 92 $nav['explorer']
Start-Sleep -Milliseconds 500
Shot 'explorer'
Write-Output 'done'
