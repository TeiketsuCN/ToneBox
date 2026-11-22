# 抓 psxavenc（PS1 XA / PS2 VAG 编码器，Zlib 许可）
#
# 产物：vendor\psxavenc\psxavenc.exe + LICENSE-psxavenc.txt
# 发布时由 tools\package-extras.ps1 打进 extras 包。
#
# 用法：
#   powershell -ExecutionPolicy Bypass -File tools\fetch-psxavenc.ps1
#   powershell ... -Proxy http://127.0.0.1:7890     # GitHub 直连不通时用

param(
    [string]$Version = 'v0.3.1',
    [string]$Proxy = ''
)

$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
$dest = Join-Path $root 'vendor\psxavenc'
New-Item -ItemType Directory -Force -Path $dest | Out-Null

$zip = Join-Path $env:TEMP "psxavenc-$Version.zip"
$url = "https://github.com/WonderfulToolchain/psxavenc/releases/download/$Version/psxavenc-windows.zip"
Write-Host "==> 下载 $url"

$iwr = @{ Uri = $url; OutFile = $zip; UseBasicParsing = $true; TimeoutSec = 300 }
if ($Proxy) { $iwr.Proxy = $Proxy }
Invoke-WebRequest @iwr

$tmp = Join-Path $env:TEMP "psxavenc-$Version"
if (Test-Path $tmp) { Remove-Item $tmp -Recurse -Force }
Expand-Archive -LiteralPath $zip -DestinationPath $tmp -Force
Copy-Item (Join-Path $tmp 'bin\psxavenc.exe') $dest -Force

# 许可证（Zlib）从仓库取；拿不到就提示手动补
$licUrl = 'https://raw.githubusercontent.com/WonderfulToolchain/psxavenc/main/LICENSE'
$lic = @{ Uri = $licUrl; OutFile = (Join-Path $dest 'LICENSE-psxavenc.txt'); UseBasicParsing = $true; TimeoutSec = 60 }
if ($Proxy) { $lic.Proxy = $Proxy }
try { Invoke-WebRequest @lic } catch { Write-Host "警告：许可证没下下来，请手动补 LICENSE-psxavenc.txt" -ForegroundColor Yellow }

Remove-Item $zip -Force -ErrorAction SilentlyContinue
Remove-Item $tmp -Recurse -Force -ErrorAction SilentlyContinue

Write-Host "==> 完成：$dest\psxavenc.exe" -ForegroundColor Green
Get-ChildItem $dest | Select-Object Name, @{ n = 'MB'; e = { [math]::Round($_.Length / 1MB, 1) } } | Format-Table -AutoSize
