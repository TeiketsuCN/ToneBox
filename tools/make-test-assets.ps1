# 生成集成测试用的合成音频资产（无版权问题）。
#
#   powershell -ExecutionPolicy Bypass -File tools\make-test-assets.ps1
#
# 产物在 test-assets\（已 gitignore）。需要随包 ffmpeg 或 -Ffmpeg 指定。

param(
    [string]$Ffmpeg = ''
)

$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
if (-not $Ffmpeg) {
    $Ffmpeg = Join-Path $root 'target\release\ffmpeg\ffmpeg.exe'
}
if (-not (Test-Path $Ffmpeg)) {
    throw "找不到 ffmpeg：$Ffmpeg（用 -Ffmpeg 指定）"
}
$out = Join-Path $root 'test-assets'
New-Item -ItemType Directory -Force -Path $out | Out-Null

function Gen([string]$name, [string[]]$ffArgs) {
    $path = Join-Path $out $name
    & $Ffmpeg -v error -y @ffArgs $path
    if ($LASTEXITCODE -ne 0) { throw "生成失败：$name" }
    $kb = [math]::Round((Get-Item $path).Length / 1KB, 1)
    Write-Host ("  {0,-24} {1,8} KB" -f $name, $kb)
}

Write-Host "== 合成测试资产 -> $out" -ForegroundColor Cyan

Gen 'sine16-stereo.wav' @('-f', 'lavfi', '-i', 'sine=frequency=440:duration=2', '-ar', '44100', '-ac', '2', '-c:a', 'pcm_s16le')
Gen 'sine16-mono.wav' @('-f', 'lavfi', '-i', 'sine=frequency=440:duration=2', '-ar', '44100', '-ac', '1', '-c:a', 'pcm_s16le')
Gen 'sine24-stereo.wav' @('-f', 'lavfi', '-i', 'sine=frequency=440:duration=2', '-ar', '48000', '-ac', '2', '-c:a', 'pcm_s24le')
Gen 'sine32f-stereo.wav' @('-f', 'lavfi', '-i', 'sine=frequency=440:duration=2', '-ar', '48000', '-ac', '2', '-c:a', 'pcm_f32le')
Gen 'noise16-stereo.wav' @('-f', 'lavfi', '-i', 'anoisesrc=color=pink:duration=2', '-ar', '44100', '-ac', '2', '-c:a', 'pcm_s16le')
Gen 'silence16-stereo.wav' @('-f', 'lavfi', '-i', 'anullsrc=r=44100:cl=stereo', '-t', '2', '-c:a', 'pcm_s16le')
Gen 'sine16-short.wav' @('-f', 'lavfi', '-i', 'sine=frequency=440:duration=0.3', '-ar', '44100', '-ac', '2', '-c:a', 'pcm_s16le')
Gen 'cover.jpg' @('-f', 'lavfi', '-i', 'color=c=teal:s=300x300:d=1', '-frames:v', '1')

Write-Host "== 完成：$(@(Get-ChildItem $out -File).Count) 个文件" -ForegroundColor Green
