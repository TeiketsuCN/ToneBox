param(
  [string]$Out = "",
  [string]$Proc = "ToneLore",
  [int]$Pad = 0
)
# 脚本在 tools\ 下，项目根目录 = 上一级；换目录、换盘符都不用改
$root = Split-Path -Parent $PSScriptRoot
if ($Out -eq "") { $Out = Join-Path $root "out\shot.png" }
New-Item -ItemType Directory -Force -Path (Split-Path -Parent $Out) | Out-Null
Add-Type -AssemblyName System.Drawing
Add-Type -Path (Join-Path $PSScriptRoot "shot.cs")

$p = Get-Process -Name $Proc -ErrorAction SilentlyContinue |
     Where-Object { $_.MainWindowHandle -ne 0 } | Select-Object -First 1
if (-not $p) { Write-Host "找不到进程窗口: $Proc"; exit 1 }

$h = $p.MainWindowHandle
[WinShot]::ShowWindow($h, 9) | Out-Null
[WinShot]::SetForegroundWindow($h) | Out-Null
Start-Sleep -Milliseconds 1200

$r = [WinShot]::Rect($h)
$x = $r[0] - $Pad; $y = $r[1] - $Pad; $w = $r[2] + 2 * $Pad; $ht = $r[3] + 2 * $Pad
Write-Host "window ${w}x${ht} at ($x,$y)"

$bmp = New-Object System.Drawing.Bitmap $w, $ht
$g = [System.Drawing.Graphics]::FromImage($bmp)
$g.CopyFromScreen($x, $y, 0, 0, (New-Object System.Drawing.Size($w, $ht)))
$bmp.Save($Out, [System.Drawing.Imaging.ImageFormat]::Png)
$g.Dispose(); $bmp.Dispose()
Write-Host "saved $Out"
