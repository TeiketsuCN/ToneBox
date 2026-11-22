# Build a Windows release zip (exe + ffmpeg + short readme).
# Usage:
#   powershell -NoProfile -ExecutionPolicy Bypass -File tools\package-release.ps1
#   powershell ... -Out D:\ToneBox-beta1.2-win64.zip
#   powershell ... -Ver beta1.3          # 默认从 Cargo.toml 取

param(
    [string]$Out = "",
    [string]$Ver = ""
)

$ErrorActionPreference = 'Stop'
$src = Split-Path -Parent $PSScriptRoot

# 版本号从 Cargo.toml 取，别再手写：1.2.0-beta.1 -> beta1.2
# （手写的那次在发 beta 1.2 时还留着 beta1.0，包名和 README 全对不上）
if (-not $Ver) {
    $toml = Get-Content (Join-Path $src 'Cargo.toml') -Raw
    if ($toml -match '(?m)^version\s*=\s*"(\d+)\.(\d+)\.\d+-beta\.(\d+)"') {
        $Ver = "beta$($Matches[1]).$($Matches[2])"
    } elseif ($toml -match '(?m)^version\s*=\s*"(\d+)\.(\d+)\.(\d+)"') {
        # 正式版：1.3.0 -> 1.3
        $Ver = "$($Matches[1]).$($Matches[2]).$($Matches[3])"
    } else {
        throw "无法从 Cargo.toml 解析版本号，请用 -Ver 指定"
    }
}
$ver = $Ver
if (-not $Out) {
    $Out = Join-Path $src "dist\ToneBox-$ver-win64.zip"
}

Write-Host "== 1/4 cargo build --release ==" -ForegroundColor Cyan
Push-Location $src
try {
    $cargo = (Get-Command cargo -ErrorAction SilentlyContinue)
    if (-not $cargo) { $cargo = Join-Path $env:USERPROFILE '.cargo\bin\cargo.exe' }
    & $cargo build --release -p fc-gui
    if ($LASTEXITCODE -ne 0) { throw "cargo build failed: $LASTEXITCODE" }
} finally {
    Pop-Location
}
$targetRoot = if ($env:CARGO_TARGET_DIR) { $env:CARGO_TARGET_DIR } else { Join-Path $src 'target' }
$exe = Join-Path $targetRoot 'release\format-converter.exe'
if (-not (Test-Path $exe)) { throw "missing $exe" }

$ffmpeg = Join-Path $src 'vendor\ffmpeg'
if (-not (Test-Path (Join-Path $ffmpeg 'ffmpeg.exe'))) {
    throw "ffmpeg not found; run tools\fetch-ffmpeg.ps1 first"
}

$stage = Join-Path $src "dist\ToneBox-$ver-win64"
if (Test-Path $stage) { Remove-Item $stage -Recurse -Force }
New-Item -ItemType Directory -Force -Path $stage | Out-Null
New-Item -ItemType Directory -Force -Path (Join-Path $stage 'ffmpeg') | Out-Null

Write-Host "== 2/4 copy exe + ffmpeg ==" -ForegroundColor Cyan
Copy-Item $exe (Join-Path $stage 'format-converter.exe') -Force
Copy-Item (Join-Path $ffmpeg '*') (Join-Path $stage 'ffmpeg') -Recurse -Force

# ATRAC 编码器（atracdenc）是可选组件，只随 extras 包发出，不进主包

# 随包 Speex 编码器（BSD；speexenc + DLL + LICENSE）
$speex = Join-Path $src 'vendor\speex'
if (Test-Path $speex) {
    Copy-Item $speex (Join-Path $stage 'speex') -Recurse -Force
}

# 随包 WavPack CLI（BSD；有损 hybrid 模式只有官方 CLI 支持，ffmpeg 只做无损）
$wavpack = Join-Path $src 'vendor\wavpack'
if (Test-Path $wavpack) {
    Copy-Item $wavpack (Join-Path $stage 'wavpack') -Recurse -Force
}

# 随包 Musepack 编码器（LGPL-2.1；mpcenc + libmpcdec DLL + LICENSE）
$musepack = Join-Path $src 'vendor\musepack'
if (Test-Path $musepack) {
    Copy-Item $musepack (Join-Path $stage 'musepack') -Recurse -Force
}

Write-Host "== 3/4 write readme ==" -ForegroundColor Cyan
$disp = $ver -replace '^beta', ''
$readme = @"
ToneBox beta $disp (Windows x64)

TEST BUILD. Back up your files before converting or playing.
You are responsible for any device/file damage.

Run: double-click format-converter.exe
First start uses the Aurora gradient and the Large window size.

ffmpeg is bundled in .\ffmpeg and found automatically. Do not move it.
Speex encoding uses the bundled .\speex\speexenc (BSD, see THIRD_PARTY_NOTICES.md).
WavPack lossy (hybrid) uses the bundled .\wavpack\wavpack.exe (BSD).
Musepack encoding uses the bundled .\musepack\mpcenc.exe (LGPL).
ATRAC encoding needs atracdenc (included only if atracdenc\ exists).
OptimFROG needs your own ofr.exe (not included: official license restricts redistribution).

Uninstall: delete this folder. No registry entries, no services.
"@
Set-Content -Path (Join-Path $stage 'README.txt') -Value $readme -Encoding ASCII
Copy-Item (Join-Path $src 'LICENSE') (Join-Path $stage 'LICENSE') -Force
Copy-Item (Join-Path $src 'THIRD_PARTY_NOTICES.md') (Join-Path $stage 'THIRD_PARTY_NOTICES.md') -Force

Write-Host "== 4/4 zip ==" -ForegroundColor Cyan
$dist = Split-Path -Parent $Out
if (-not (Test-Path $dist)) { New-Item -ItemType Directory -Force -Path $dist | Out-Null }
if (Test-Path $Out) { Remove-Item $Out -Force }
Compress-Archive -Path (Join-Path $stage '*') -DestinationPath $Out -CompressionLevel Optimal
$mb = [math]::Round((Get-Item $Out).Length / 1MB, 1)
Write-Host "done: $Out ($mb MB)" -ForegroundColor Green
