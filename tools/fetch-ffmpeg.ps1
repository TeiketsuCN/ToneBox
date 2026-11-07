# 下载并裁剪 LGPL 版 ffmpeg 到 vendor\ffmpeg\
#
# 为什么用 LGPL 共享版而不是官网常见的 GPL 静态版：
#   1. GPL 构建（--enable-gpl，带 x264/x265）一旦随包分发，我们就要承担 GPLv3 义务；
#      我们只做音频，根本用不到那些组件。
#   2. 共享版（exe + dll）比静态版小一半：148 MB vs 约 400 MB（两个静态 exe）。
#   3. LGPL 允许随程序分发动态库，只要保留许可证、允许替换。
#
# 产物不进 git（见 .gitignore 的 vendor/），构建/安装时现拉。

$ErrorActionPreference = "Stop"
$ProgressPreference = "SilentlyContinue"

$root = Split-Path $PSScriptRoot -Parent
$vendor = Join-Path $root "vendor\ffmpeg"
$zipPath = Join-Path $root "vendor\ffmpeg-lgpl.zip"

if ((Test-Path (Join-Path $vendor "ffmpeg.exe")) -and (Test-Path (Join-Path $vendor "ffprobe.exe"))) {
    $have = (Get-ChildItem $vendor -File | Measure-Object Length -Sum).Sum / 1MB
    Write-Host "[跳过] vendor\ffmpeg 已存在（$([math]::Round($have,1)) MB）"
    exit 0
}

New-Item -ItemType Directory -Path (Split-Path $vendor) -Force | Out-Null

Write-Host "查询 BtbN FFmpeg-Builds 最新版本…"
$rel = Invoke-RestMethod -Uri "https://api.github.com/repos/BtbN/FFmpeg-Builds/releases/latest" -TimeoutSec 60
$asset = $rel.assets | Where-Object { $_.name -like "*win64-lgpl-shared*.zip" } | Select-Object -First 1
if (-not $asset) { throw "没找到 win64-lgpl-shared 构建包" }

Write-Host "下载 $($asset.name)（$([math]::Round($asset.size/1MB,1)) MB）…"
Invoke-WebRequest -Uri $asset.browser_download_url -OutFile $zipPath -UseBasicParsing -TimeoutSec 1800

Write-Host "解压…"
$tmp = Join-Path $root "vendor\_ffmpeg_tmp"
if (Test-Path $tmp) { Remove-Item $tmp -Recurse -Force }
Expand-Archive -Path $zipPath -DestinationPath $tmp -Force

$binSrc = Get-ChildItem $tmp -Recurse -Directory -Filter bin | Select-Object -First 1
if (-not $binSrc) { throw "压缩包里没有 bin 目录" }

New-Item -ItemType Directory -Path $vendor -Force | Out-Null

# ffmpeg/ffprobe + 全部 DLL，但丢掉 ffplay（18 MB，用不到）
Get-ChildItem $binSrc.FullName -File | Where-Object { $_.Name -ne "ffplay.exe" } | ForEach-Object {
    Copy-Item $_.FullName (Join-Path $vendor $_.Name) -Force
}

# 许可证必须随二进制一起分发（LGPL 要求）
$lic = Get-ChildItem $tmp -Recurse -File -Filter "LICENSE.txt" | Select-Object -First 1
if ($lic) { Copy-Item $lic.FullName (Join-Path $vendor "LICENSE-ffmpeg.txt") -Force }

Remove-Item $tmp -Recurse -Force
Remove-Item $zipPath -Force

$have = (Get-ChildItem $vendor -File | Measure-Object Length -Sum).Sum / 1MB
Write-Host "完成：vendor\ffmpeg（$([math]::Round($have,1)) MB）"
Get-ChildItem $vendor -File | Sort-Object Length -Descending |
    Select-Object Name, @{n='MB';e={[math]::Round($_.Length/1MB,1)}} | Format-Table -AutoSize
