# Build the experimental Vulkan (wgpu) build and put it on the Desktop.
# It is completely separate from the installed OpenGL build:
#   - exe: Desktop\ToneBox-Vulkan\ToneBox-Vulkan.exe
#   - config: %APPDATA%\FormatConverter-Vulkan
#   - bundled ffmpeg/speex inside its own folder
$ErrorActionPreference = 'Stop'
$src = Split-Path -Parent $PSScriptRoot
$dst = Join-Path ([Environment]::GetFolderPath('Desktop')) 'ToneBox-Vulkan'
$target = if ($env:CARGO_TARGET_DIR) { $env:CARGO_TARGET_DIR } else { Join-Path $src 'target' }
$exe = Join-Path $target 'release\format-converter.exe'

Write-Host "== build Vulkan/wgpu ==" -ForegroundColor Cyan
& (Join-Path $src 'build.ps1') release --features vulkan
if (-not (Test-Path $exe)) { throw "build output missing: $exe" }

New-Item -ItemType Directory -Force -Path $dst | Out-Null
Get-Process -Name 'ToneBox-Vulkan' -ErrorAction SilentlyContinue | Stop-Process -Force
Start-Sleep -Milliseconds 400

Copy-Item $exe (Join-Path $dst 'ToneBox-Vulkan.exe') -Force
Copy-Item (Join-Path $src 'crates\fc-gui\assets\icon.ico') (Join-Path $dst 'ToneBox.ico') -Force
Copy-Item (Join-Path $src 'THIRD_PARTY_NOTICES.md') $dst -Force
Copy-Item (Join-Path $src 'LICENSE') $dst -Force

foreach ($name in @('ffmpeg', 'speex')) {
    $from = Join-Path $src "vendor\$name"
    $to = Join-Path $dst $name
    if (Test-Path $from) {
        if (Test-Path $to) { Remove-Item $to -Recurse -Force }
        Copy-Item $from $to -Recurse -Force
    }
}

@'
ToneBox Vulkan 实验版
- 使用 wgpu + Vulkan 渲染（不透明窗口，避免 wgpu 透明表面黑块）
- 配置目录：%APPDATA%\FormatConverter-Vulkan，和原版互不影响
- 随包 ffmpeg / speex；许可证见 THIRD_PARTY_NOTICES.md
'@ | Set-Content (Join-Path $dst 'README.txt') -Encoding UTF8

$ws = New-Object -ComObject WScript.Shell
$lnk = $ws.CreateShortcut((Join-Path ([Environment]::GetFolderPath('Desktop')) 'ToneBox Vulkan.lnk'))
$lnk.TargetPath = Join-Path $dst 'ToneBox-Vulkan.exe'
$lnk.WorkingDirectory = $dst
$lnk.IconLocation = Join-Path $dst 'ToneBox.ico'
$lnk.Description = 'ToneBox Vulkan 实验版'
$lnk.Save()
Write-Host "done: $dst"
