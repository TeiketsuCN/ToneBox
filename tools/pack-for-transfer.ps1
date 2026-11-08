# 把整个工作区打成可以直接拷到另一台电脑的 zip
#
# 打进去的：
#   crates/ docs/ tools/ Cargo.toml Cargo.lock build.ps1 README.md HANDOFF.md .gitignore
#   .git（历史）、vendor\ffmpeg（152MB，ffmpeg 不进 git）
#   加了 -WithSamples 再带上 samples\
# 不打的：
#   target\（4GB，机器相关）、out\（截图）、app\（旧的构建产物）
#
# 用法：
#   powershell -ExecutionPolicy Bypass -File tools\pack-for-transfer.ps1
#   powershell ... -WithSamples -Out E:\ToneBox-transfer.zip

param(
    [string]$Out = "",
    [switch]$WithSamples
)

$ErrorActionPreference = 'Stop'

$src = Split-Path -Parent $PSScriptRoot
if (-not $Out) {
    $Out = Join-Path ([Environment]::GetFolderPath('Desktop')) 'FormatConverter-transfer.zip'
}

$stage = Join-Path $env:TEMP ("fc-transfer-" + [guid]::NewGuid().ToString('N').Substring(0, 6))
New-Item -ItemType Directory -Force -Path $stage | Out-Null

$excludeDirs = @(
    (Join-Path $src 'target'),
    (Join-Path $src 'out'),
    (Join-Path $src 'app')
)
if (-not $WithSamples) {
    $excludeDirs += (Join-Path $src 'samples')
}

Write-Host "== 1/3 复制源码（跳过 target / out / app）==" -ForegroundColor Cyan
$robo = @(
    $src, $stage, '/E',
    '/XF', '*.pdb',
    '/NFL', '/NDL', '/NJH', '/NJS',
    '/XD'
) + $excludeDirs
& robocopy @robo | Out-Null
if ($LASTEXITCODE -ge 8) { throw "robocopy 失败，exit=$LASTEXITCODE" }

# 包内说明，别让新机器上手时抓瞎
$note = @"
ToneBox 迁移包（$(Get-Date -Format 'yyyy-MM-dd HH:mm')）

源码 + .git + vendor\ffmpeg$(if ($WithSamples) { ' + samples' } else { '' })，不含 target / out。

新机器上：
1) 装 Rust（gnu 工具链）：
   .\tools\rustup-init.exe -y --default-host x86_64-pc-windows-gnu --profile minimal
2) 装 MinGW（WinLibs）：
   winget install BrechtSanders.WinLibs.POSIX.UCRT
3) 改 build.ps1 里写死的 WinLibs 路径和 Set-Location；改 tools\install.ps1 的 `$AppDir
4) powershell -ExecutionPolicy Bypass -File tools\install.ps1

细节见 docs\迁移-新电脑.md
"@
$note | Set-Content -Path (Join-Path $stage '迁移说明.txt') -Encoding UTF8

Write-Host "== 2/3 压缩（几百 MB，等一会儿）==" -ForegroundColor Cyan
if (Test-Path -LiteralPath $Out) { Remove-Item -LiteralPath $Out -Force }
Add-Type -AssemblyName System.IO.Compression.FileSystem
[System.IO.Compression.ZipFile]::CreateFromDirectory(
    $stage, $Out,
    [System.IO.Compression.CompressionLevel]::Optimal,
    $true
)

Write-Host "== 3/3 清理临时目录 ==" -ForegroundColor Cyan
Remove-Item -LiteralPath $stage -Recurse -Force

$size = (Get-Item -LiteralPath $Out).Length / 1MB
Write-Host ("完成：{0}（{1:N0} MB）" -f $Out, $size) -ForegroundColor Green
