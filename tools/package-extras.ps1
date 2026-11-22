# 可选外部工具包（extras）打包脚本
#
# 内容：
#   atracdenc/         ATRAC 编码器（LGPL-2.1，可随包，带许可证与源码地址）
#   install-ofr.cmd    双击即可下载 OptimFROG 的 ofr.exe 并放到 optimfrog\
#   README.txt         中文说明
#
# 为什么不直接把 ofr.exe 放进 zip：
#   OptimFROG 官方 license 写明 "distribution of command-line Encoders with
#   other software may need permission"，再分发有风险；所以这里放的是
#   **一键下载脚本**，由用户自己从官网取，效果一样但不涉及再分发。
#
# 用法：
#   powershell -ExecutionPolicy Bypass -File tools\package-extras.ps1
#   powershell ... -Ver 1.4.0

param(
    [string]$Out = "",
    [string]$Ver = ""
)

$ErrorActionPreference = 'Stop'
$src = Split-Path -Parent $PSScriptRoot

if (-not $Ver) {
    $toml = Get-Content (Join-Path $src 'Cargo.toml') -Raw
    if ($toml -match '(?m)^version\s*=\s*"(\d+)\.(\d+)\.(\d+)"') {
        $Ver = "$($Matches[1]).$($Matches[2]).$($Matches[3])"
    } elseif ($toml -match '(?m)^version\s*=\s*"(\d+)\.(\d+)\.\d+-beta\.(\d+)"') {
        $Ver = "beta$($Matches[1]).$($Matches[2])"
    } else {
        throw "无法从 Cargo.toml 解析版本号，请用 -Ver 指定"
    }
}

if (-not $Out) {
    $Out = Join-Path $src "dist\ToneBox-$Ver-extras-win64.zip"
}

$stage = Join-Path $src "dist\ToneBox-$Ver-extras"
if (Test-Path $stage) { Remove-Item $stage -Recurse -Force }
New-Item -ItemType Directory -Force -Path $stage | Out-Null

Write-Host "== 1/3 ATRAC 编码器 ==" -ForegroundColor Cyan
$atracSrc = Join-Path $src 'vendor\atracdenc'
$atracExe = Join-Path $atracSrc 'atracdenc.exe'
if (-not (Test-Path $atracExe)) {
    # 兼容只有 target\release 的情况
    $alt = Join-Path $src 'target\release\atracdenc\atracdenc.exe'
    if (Test-Path $alt) { $atracSrc = Split-Path $alt -Parent } else { throw "找不到 atracdenc.exe，先跑 tools\fetch-atracdenc.ps1" }
}
$atracDst = Join-Path $stage 'atracdenc'
New-Item -ItemType Directory -Force -Path $atracDst | Out-Null
Copy-Item (Join-Path $atracSrc 'atracdenc.exe') $atracDst -Force
$lic = Join-Path $atracSrc 'LICENSE-atracdenc.txt'
if (Test-Path $lic) { Copy-Item $lic $atracDst -Force }
$srcNote = @"
atracdenc —— ATRAC1 / ATRAC3 / ATRAC3+ 编码器
项目主页：https://github.com/dcherednik/atracdenc
许可：LGPL-2.1-or-later（见同目录 LICENSE-atracdenc.txt）
源码：https://github.com/dcherednik/atracdenc
说明：按 LGPL 要求，这里提供二进制；如需修改后的源码请见上面的项目地址，
      或联系打包者索取。
"@
Set-Content -Path (Join-Path $atracDst 'SOURCE-说明.txt') -Value $srcNote -Encoding UTF8

Write-Host "== 2/3 OptimFROG 下载脚本 ==" -ForegroundColor Cyan
$ofrPs1 = @'
# 下载 OptimFROG 的 ofr.exe 到本目录下的 optimfrog\（不涉及再分发）
# 由 install-ofr.cmd 调用；需要联网。
$ErrorActionPreference = 'Stop'
$here = Split-Path -Parent $MyInvocation.MyCommand.Path
$dest = Join-Path $here 'optimfrog'
$tmp = Join-Path $env:TEMP ('tonebox_ofr_' + [guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Force -Path $dest, $tmp | Out-Null

$base = 'http://losslessaudio.org/Downloads/A1C0/5100/Win/'
$name = 'OptimFROG_Win_x64_5100.zip'
$zip = Join-Path $tmp $name

Write-Host "正在从 losslessaudio.org 下载 OptimFROG 5.100 ..." -ForegroundColor Cyan
Invoke-WebRequest -Uri ($base + $name) -OutFile $zip -UseBasicParsing -TimeoutSec 300

$sha1 = [System.Text.Encoding]::ASCII.GetString(
    (Invoke-WebRequest -Uri ($base + $name + '.sha1') -UseBasicParsing -TimeoutSec 60).Content)
$expect = ($sha1 -split '\s+')[0].ToLower()
$actual = (Get-FileHash $zip -Algorithm SHA1).Hash.ToLower()
if ($expect -ne $actual) { throw "SHA-1 校验失败：官方 $expect，实际 $actual" }
Write-Host "SHA-1 校验通过：$actual" -ForegroundColor Green

& tar.exe -xf $zip -C $tmp
$inner = Join-Path $tmp 'OptimFROG_Win_x64_5100'
if (-not (Test-Path (Join-Path $inner 'ofr.exe'))) { throw "压缩包里没找到 ofr.exe" }
Copy-Item (Join-Path $inner 'ofr.exe') $dest -Force
Copy-Item (Join-Path $inner 'license.html') $dest -Force
Copy-Item (Join-Path $inner 'changelog.txt') $dest -Force
Remove-Item $tmp -Recurse -Force

Write-Host "完成：$dest\ofr.exe" -ForegroundColor Green
Write-Host "回到程序里点 设置 → 依赖 / 外部工具 → 重新检测"
'@
Set-Content -Path (Join-Path $stage 'install-ofr.ps1') -Value $ofrPs1 -Encoding UTF8

$cmd = @'
@echo off
chcp 65001 >nul
echo.
echo  ToneBox - install OptimFROG encoder (ofr.exe)
echo.
powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0install-ofr.ps1"
echo.
pause
'@
Set-Content -Path (Join-Path $stage 'install-ofr.cmd') -Value $cmd -Encoding ASCII

$ofrNote = @"
为什么这里没有直接放 ofr.exe
--------------------------------
OptimFROG 的官方许可（license.html 与 ofr.exe --help 都有）写明：

  "Free for non-commercial and commercial use. However, distribution
   of command-line Encoders with other software may need permission."

也就是说换个地方再分发它的命令行编码器，需要作者同意。
所以这里给的是 install-ofr.cmd：双击它，脚本会把官方安装包从
losslessaudio.org 下载下来、用官方 SHA-1 校验、只取出 ofr.exe 放进 optimfrog\。
对你来说是一步；对作者来说是"用户自己下载"。

如果你就是 OptimFROG 作者、或已获得授权，把 ofr.exe 直接放进 optimfrog\ 即可。
"@
Set-Content -Path (Join-Path $stage 'OFR-许可说明.txt') -Value $ofrNote -Encoding UTF8

$readme = @"
ToneBox 可选外部工具包
======================

这个包里是两样**不随主程序一起发**的编码器。
把整个压缩包解压到 ToneBox 目录（format-converter.exe 所在的那层），然后：

1) ATRAC（atracdenc）
   已包含在 atracdenc\ 里，解压完就能用。
   程序会自动找到它，不用做别的。

2) OptimFROG（ofr.exe）
   双击 install-ofr.cmd，脚本会从官网下载并校验，放到 optimfrog\。
   需要联网。

装完后回到程序：设置 → 依赖 / 外部工具 → 重新检测，
就能看到 atracdenc / ofr 亮起来。

这两个工具都是可选的：
- 没有 atracdenc：ATRAC1 / ATRAC3 / ATRAC3+ 不能输出（其它格式不受影响）
- 没有 ofr.exe：OptimFROG 不能输出（其它格式不受影响）

许可说明见各子目录里的文本。
"@
Set-Content -Path (Join-Path $stage 'README.txt') -Value $readme -Encoding UTF8

Write-Host "== 3/3 zip ==" -ForegroundColor Cyan
$dist = Split-Path -Parent $Out
if (-not (Test-Path $dist)) { New-Item -ItemType Directory -Force -Path $dist | Out-Null }
if (Test-Path $Out) { Remove-Item $Out -Force }
Compress-Archive -Path (Join-Path $stage '*') -DestinationPath $Out -CompressionLevel Optimal
$mb = [math]::Round((Get-Item $Out).Length / 1MB, 2)
Write-Host "done: $Out ($mb MB)" -ForegroundColor Green
