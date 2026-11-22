# 抓并编译 N64 VADPCM 编解码器（MPL-2.0）
#
# 产物：vendor\vadpcm\vadpcm.exe + LICENSE-vadpcm.txt
# 依赖：MSYS2 UCRT64（cmake / ninja / gcc）与 git
#
# 用法：
#   powershell -ExecutionPolicy Bypass -File tools\fetch-vadpcm.ps1
#   powershell ... -Proxy http://127.0.0.1:7890

param(
    [string]$Proxy = '',
    [string]$Msys2 = 'C:\msys64'
)

$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
$src = Join-Path $root 'build\vadpcm'
$dest = Join-Path $root 'vendor\vadpcm'
New-Item -ItemType Directory -Force -Path $dest | Out-Null

$git = 'C:\Program Files\Git\cmd\git.exe'
if (-not (Test-Path $git)) { $git = 'git' }
if (-not (Test-Path $src)) {
    $proxyArgs = @()
    if ($Proxy) { $proxyArgs = @('-c', "http.proxy=$Proxy", '-c', "https.proxy=$Proxy") }
    & $git -C $root @proxyArgs clone --depth 1 https://github.com/depp/vadpcm build/vadpcm
    if ($LASTEXITCODE -ne 0) { throw 'clone vadpcm 失败' }
}

$bash = Join-Path $Msys2 'usr\bin\bash.exe'
if (-not (Test-Path $bash)) { throw "找不到 $bash，先装 MSYS2" }
$msys = '/' + $src.Substring(0, 1).ToLower() + ($src.Substring(2).Replace('\', '/'))
$cmd = "export PATH=/ucrt64/bin:/usr/bin:`$PATH; cd '$msys' && cmake -B build -G Ninja -DCMAKE_BUILD_TYPE=Release && cmake --build build"
& $bash -lc $cmd
if ($LASTEXITCODE -ne 0) { throw "vadpcm 构建失败（退出码 $LASTEXITCODE）" }

Copy-Item (Join-Path $src 'build\vadpcm.exe') $dest -Force
Copy-Item (Join-Path $src 'LICENSE.txt') (Join-Path $dest 'LICENSE-vadpcm.txt') -Force
Write-Host "==> 完成：$dest\vadpcm.exe" -ForegroundColor Green