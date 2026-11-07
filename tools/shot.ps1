param(
  [string]$Out = "D:\FormatConverter\out\shot.png",
  [string]$Proc = "format-converter",
  [int]$Pad = 0
)
Add-Type -AssemblyName System.Drawing
Add-Type -Path "D:\FormatConverter\tools\shot.cs"

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
