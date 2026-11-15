# 下载 WavPack 官方 CLI（BSD-3-Clause）到 vendor\wavpack
#
# 为什么需要它：
#   ffmpeg 的 wavpack 编码器**只有无损**（`-h encoder=wavpack` 里只有
#   joint_stereo / optimize_mono 两个选项），而 WavPack 的有损（hybrid）
#   模式必须用官方 CLI 的 -b 参数才能写出来。
#
# 二进制取自 MSYS2 的 mingw-w64-ucrt-x86_64-wavpack 包（官方源码编译）。
# 用法：
#   powershell -ExecutionPolicy Bypass -File tools\fetch-wavpack.ps1

$ErrorActionPreference = 'Stop'
$src = Split-Path -Parent $PSScriptRoot
$vendor = Join-Path $src 'vendor\wavpack'
$tmp = Join-Path $env:TEMP ('tonebox_wavpack_' + [guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Force -Path $vendor, $tmp | Out-Null

$pkg = 'https://mirror.msys2.org/mingw/ucrt64/mingw-w64-ucrt-x86_64-wavpack-5.9.0-1-any.pkg.tar.zst'
$file = Join-Path $tmp 'wavpack.pkg.tar.zst'
Write-Host "downloading $pkg"
& curl.exe -L --fail --max-time 300 -o $file $pkg
if ($LASTEXITCODE -ne 0) { throw "download failed: $pkg" }
& tar.exe -xf $file -C $tmp
if ($LASTEXITCODE -ne 0) { throw "extract failed: $file" }

$bin = Join-Path $tmp 'ucrt64\bin'
Copy-Item (Join-Path $bin 'wavpack.exe') $vendor -Force
Copy-Item (Join-Path $bin 'libwavpack-1.dll') $vendor -Force

# 许可证：MSYS2 放在 share\licenses\wavpack 下
$licDir = Join-Path $tmp 'ucrt64\share\licenses\wavpack'
if (Test-Path $licDir) {
    $lic = Get-ChildItem $licDir -File | Select-Object -First 1
    if ($lic) { Copy-Item $lic.FullName (Join-Path $vendor 'LICENSE-wavpack.txt') -Force }
}

Remove-Item $tmp -Recurse -Force
Write-Host "WavPack installed to $vendor"
Get-ChildItem $vendor -File | Select-Object Name, @{n = 'KB'; e = { [math]::Round($_.Length / 1KB, 1) } } | Format-Table -AutoSize
