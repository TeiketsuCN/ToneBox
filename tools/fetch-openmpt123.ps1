# 抓 openmpt123（模块音乐渲染器，BSD-3-Clause）
#
# 产物：vendor\openmpt123\openmpt123.exe + libopenmpt 依赖 DLL + LICENSE
# 依赖：MSYS2 UCRT64（pacman 装 mingw-w64-ucrt-x86_64-libopenmpt）
#
# 用法：
#   powershell -ExecutionPolicy Bypass -File tools\fetch-openmpt123.ps1

param(
    [string]$Msys2 = 'C:\msys64'
)

$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
$dest = Join-Path $root 'vendor\openmpt123'
New-Item -ItemType Directory -Force -Path $dest | Out-Null

$pacman = Join-Path $Msys2 'usr\bin\pacman.exe'
if (-not (Test-Path $pacman)) { throw "找不到 $pacman，先装 MSYS2" }
Write-Host "==> 安装 libopenmpt"
& $pacman -S --needed --noconfirm mingw-w64-ucrt-x86_64-libopenmpt
if ($LASTEXITCODE -ne 0) { throw "pacman 安装失败（退出码 $LASTEXITCODE）" }

$bin = Join-Path $Msys2 'ucrt64\bin'
Copy-Item (Join-Path $bin 'openmpt123.exe') $dest -Force
# openmpt123 的依赖（libopenmpt + 它链接的编解码库）；缺一个都起不来
$dlls = @(
    'libopenmpt-0.dll', 'libFLAC.dll', 'libgcc_s_seh-1.dll', 'libmp3lame-0.dll',
    'libmpg123-0.dll', 'libogg-0.dll', 'libopus-0.dll', 'libportaudio.dll',
    'libsndfile-1.dll', 'libstdc++-6.dll', 'libvorbis-0.dll', 'libvorbisenc-2.dll',
    'libvorbisfile-3.dll', 'libwinpthread-1.dll', 'zlib1.dll'
)
foreach ($d in $dlls) {
    $p = Join-Path $bin $d
    if (Test-Path $p) { Copy-Item $p $dest -Force }
}
$licDir = Join-Path $Msys2 'ucrt64\share\licenses\libopenmpt'
if (Test-Path $licDir) {
    Copy-Item (Join-Path $licDir '*') (Join-Path $dest 'LICENSE-libopenmpt.txt') -Force
}
Write-Host "==> 完成：$dest\openmpt123.exe" -ForegroundColor Green
Get-ChildItem $dest | Select-Object Name, @{ n = 'MB'; e = { [math]::Round($_.Length / 1MB, 1) } } | Format-Table -AutoSize