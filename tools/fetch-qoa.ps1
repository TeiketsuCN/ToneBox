# 抓并编译 qoaconv（QOA 参考转换器，MIT）
#
# 产物：vendor\qoa\qoaconv.exe + LICENSE-qoa.txt + qoa.h/qoaconv.c
# 依赖：MSYS2 的 UCRT64 gcc（C:\msys64\ucrt64\bin\gcc.exe）
#
# 用法：
#   powershell -ExecutionPolicy Bypass -File tools\fetch-qoa.ps1
#   powershell ... -Proxy http://127.0.0.1:7890

param(
    [string]$Proxy = '',
    [string]$Msys2 = 'C:\msys64'
)

$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
$dest = Join-Path $root 'vendor\qoa'
New-Item -ItemType Directory -Force -Path $dest | Out-Null

$base = 'https://raw.githubusercontent.com/phoboslab/qoa/master'
foreach ($f in @('qoa.h', 'qoaconv.c', 'LICENSE')) {
    $out = Join-Path $dest $(if ($f -eq 'LICENSE') { 'LICENSE-qoa.txt' } else { $f })
    $iwr = @{ Uri = "$base/$f"; OutFile = $out; UseBasicParsing = $true; TimeoutSec = 60 }
    if ($Proxy) { $iwr.Proxy = $Proxy }
    Invoke-WebRequest @iwr
}

$gcc = Join-Path $Msys2 'ucrt64\bin\gcc.exe'
if (-not (Test-Path $gcc)) { throw "找不到 $gcc，先装 MSYS2 UCRT64 工具链" }

$bash = Join-Path $Msys2 'usr\bin\bash.exe'
$msys = '/' + $dest.Substring(0, 1).ToLower() + ($dest.Substring(2) -replace '\\', '/')
$cmd = "export PATH=/ucrt64/bin:/usr/bin:`$PATH; cd '$msys' && gcc -O2 -o qoaconv.exe qoaconv.c -lm"
Write-Host "==> 编译 qoaconv"
& $bash -lc $cmd
if ($LASTEXITCODE -ne 0) { throw "gcc 编译失败（退出码 $LASTEXITCODE）" }

Write-Host "==> 完成：$dest\qoaconv.exe" -ForegroundColor Green
Get-ChildItem $dest | Select-Object Name, @{ n = 'KB'; e = { [math]::Round($_.Length / 1KB, 1) } } | Format-Table -AutoSize
