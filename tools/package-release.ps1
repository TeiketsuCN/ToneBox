# Build a Windows release zip (exe + ffmpeg + short readme).
# Usage:
#   powershell -NoProfile -ExecutionPolicy Bypass -File tools\package-release.ps1
#   powershell ... -Out D:\ToneBox-beta1.0-win64.zip

param(
    [string]$Out = ""
)

$ErrorActionPreference = 'Stop'
$src = Split-Path -Parent $PSScriptRoot
$ver = 'beta1.0'
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

$atrac = Join-Path $src 'vendor\atracdenc'
if (Test-Path $atrac) {
    Copy-Item $atrac (Join-Path $stage 'atracdenc') -Recurse -Force
}

Write-Host "== 3/4 write readme ==" -ForegroundColor Cyan
$readme = @"
ToneBox beta 1.0 (Windows x64)

TEST BUILD. Back up your files before converting or playing.
You are responsible for any device/file damage.

Run: double-click format-converter.exe
First start uses the Aurora gradient and the Large window size.

ffmpeg is bundled in .\ffmpeg and found automatically. Do not move it.
ATRAC encoding needs atracdenc (included only if atracdenc\ exists).

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
