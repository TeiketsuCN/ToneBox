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

# 迁移说明由仓库里的 迁移说明.txt 带过去（robocopy 已经复制），不再现生成，
# 免得每次打包都把仓库里的这份弄脏。


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
