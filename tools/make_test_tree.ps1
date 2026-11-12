$ErrorActionPreference = 'Stop'
$root = 'D:\Non-Web Projects\SmartDisk\target\ui-test-tree'
if (Test-Path $root) { Remove-Item -Recurse -Force $root }
$dirs = @('Documents','Images','Videos','Temp','Archive','Documents\Projects')
foreach ($d in $dirs) { New-Item -ItemType Directory -Force -Path (Join-Path $root $d) | Out-Null }

function New-Rand([string]$p, [int]$mb) {
    $bytes = New-Object byte[] ($mb * 1MB)
    (New-Object System.Random 42).NextBytes($bytes)
    [IO.File]::WriteAllBytes($p, $bytes)
}
New-Rand (Join-Path $root 'Documents\big-report.pdf') 5
New-Rand (Join-Path $root 'Videos\sample.bin') 18
New-Rand (Join-Path $root 'Archive\bundle.zip') 7
New-Rand (Join-Path $root 'Temp\cache.tmp') 2
New-Rand (Join-Path $root 'Temp\old.tmp') 1
foreach ($i in 1..40) {
    $p = Join-Path $root ('Documents\Projects\note-' + $i + '.txt')
    [IO.File]::WriteAllText($p, ('note ' + $i + ' ' + ('x' * (200 + $i * 7))))
}

$dup = New-Object byte[] (3MB)
(New-Object System.Random 7).NextBytes($dup)
[IO.File]::WriteAllBytes((Join-Path $root 'Images\photo.jpg'), $dup)
[IO.File]::WriteAllBytes((Join-Path $root 'Archive\photo-copy.jpg'), $dup)
$dup2 = New-Object byte[] (800KB)
(New-Object System.Random 11).NextBytes($dup2)
[IO.File]::WriteAllBytes((Join-Path $root 'Images\logo.png'), $dup2)
[IO.File]::WriteAllBytes((Join-Path $root 'Archive\logo-backup.png'), $dup2)
for ($i = 1; $i -le 8; $i++) {
    $p = Join-Path $root ('Images\thumb-' + $i + '.png')
    $n = 20000 + $i * 997
    $bytes = New-Object byte[] $n
    (New-Object System.Random (100 + $i)).NextBytes($bytes)
    [IO.File]::WriteAllBytes($p, $bytes)
}
Write-Output ('created at ' + $root)
