# 抓 vgmstream-cli（游戏流式音频解码器）
#
# 产物：vendor\vgmstream\vgmstream-cli.exe + 依赖 DLL + COPYING
# 用途：PSF / PS2 ADS / BRSTM / BCSTM / BFSTM / HCA 等先解成 WAV，再交给 ffmpeg
#
# 用法：
#   powershell -ExecutionPolicy Bypass -File tools\fetch-vgmstream.ps1
#   powershell ... -Proxy http://127.0.0.1:7890

param(
    [string]$Version = 'r2168',
    [string]$Proxy = ''
)

$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
$dest = Join-Path $root 'vendor\vgmstream'
New-Item -ItemType Directory -Force -Path $dest | Out-Null

$zip = Join-Path $env:TEMP "vgmstream-$Version.zip"
$url = "https://github.com/vgmstream/vgmstream/releases/download/$Version/vgmstream-win64.zip"
Write-Host "==> 下载 $url"
$iwr = @{ Uri = $url; OutFile = $zip; UseBasicParsing = $true; TimeoutSec = 300 }
if ($Proxy) { $iwr.Proxy = $Proxy }
Invoke-WebRequest @iwr

$tmp = Join-Path $env:TEMP "vgmstream-$Version"
if (Test-Path $tmp) { Remove-Item $tmp -Recurse -Force }
Expand-Archive -LiteralPath $zip -DestinationPath $tmp -Force
Copy-Item (Join-Path $tmp '*') $dest -Recurse -Force

Remove-Item $zip -Force -ErrorAction SilentlyContinue
Remove-Item $tmp -Recurse -Force -ErrorAction SilentlyContinue
Write-Host "==> 完成：$dest\vgmstream-cli.exe" -ForegroundColor Green
Get-ChildItem $dest -File | Select-Object Name, @{ n = 'MB'; e = { [math]::Round($_.Length / 1MB, 1) } } | Format-Table -AutoSize