# 抓 VGAudioCli（任天堂容器编码器，MIT）
#
# 产物：vendor\vgaudio\VGAudioCli.exe + LICENSE-vgaudio.txt
# 依赖：.NET Framework 4.5+（Windows 10/11 自带）
#
# 用法：
#   powershell -ExecutionPolicy Bypass -File tools\fetch-vgaudio.ps1
#   powershell ... -Proxy http://127.0.0.1:7890

param(
    [string]$Version = 'v2.2.1',
    [string]$Proxy = ''
)

$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
$dest = Join-Path $root 'vendor\vgaudio'
New-Item -ItemType Directory -Force -Path $dest | Out-Null

$zip = Join-Path $env:TEMP "vgaudio-$Version.zip"
$url = "https://github.com/Thealexbarney/VGAudio/releases/download/$Version/VGAudioCli.zip"
Write-Host "==> 下载 $url"
$iwr = @{ Uri = $url; OutFile = $zip; UseBasicParsing = $true; TimeoutSec = 300 }
if ($Proxy) { $iwr.Proxy = $Proxy }
Invoke-WebRequest @iwr

$tmp = Join-Path $env:TEMP "vgaudio-$Version"
if (Test-Path $tmp) { Remove-Item $tmp -Recurse -Force }
Expand-Archive -LiteralPath $zip -DestinationPath $tmp -Force
# 优先用 standalone 版（单文件，不依赖同目录 DLL）
$exe = Get-ChildItem $tmp -Recurse -Filter 'VGAudioCli.exe' |
    Where-Object { $_.DirectoryName -like '*standalone*' } |
    Select-Object -First 1
if (-not $exe) { $exe = Get-ChildItem $tmp -Recurse -Filter 'VGAudioCli.exe' | Select-Object -First 1 }
Copy-Item $exe.FullName $dest -Force

$lic = @{ Uri = 'https://raw.githubusercontent.com/Thealexbarney/VGAudio/master/LICENSE'; OutFile = (Join-Path $dest 'LICENSE-vgaudio.txt'); UseBasicParsing = $true; TimeoutSec = 60 }
if ($Proxy) { $lic.Proxy = $Proxy }
try { Invoke-WebRequest @lic } catch { Write-Host "警告：许可证没下下来，请手动补" -ForegroundColor Yellow }

Remove-Item $zip -Force -ErrorAction SilentlyContinue
Remove-Item $tmp -Recurse -Force -ErrorAction SilentlyContinue
Write-Host "==> 完成：$dest\VGAudioCli.exe" -ForegroundColor Green
