# 下载 atracdenc（ATRAC 编码器，LGPL-2.1+）到程序目录
#
# 为什么单独放、不随包：
#   atracdenc 是社区逆向实现的 ATRAC 编码器。ATRAC3+ 的专利期刚过、
#   ATRAC9 还在保护期，编码器的许可状况随版本变化。所以：
#   - 不放进安装包（install.ps1 不碰这个目录，build.ps1 也不复制）
#   - 程序只在检测到它的时候才允许 ATRAC 输出
#   - 装不装、装哪个版本，由用的人决定
#
# 合法信息：https://github.com/dcherednik/atracdenc  （LGPL-2.1-or-later）
#
# 用法：
#   powershell -ExecutionPolicy Bypass -File tools\fetch-atracdenc.ps1
#   powershell -ExecutionPolicy Bypass -File tools\fetch-atracdenc.ps1 -Dir "D:\某处\atracdenc"
#
# GitHub 直连不稳（国内经常 504 / API 限流），所以脚本按这个顺序试：
#   1. API 查最新 release → 走 asset 端点（需要 Accept: application/octet-stream）
#   2. 钉住的版本直链
#   3. 用 curl.exe 再试一遍上面两条
# 全失败就打印手动下载地址，不假装成功。

param(
    [string]$Dir = "D:\Apps\FormatConverter\atracdenc",
    [string]$Version = "0.2.3"
)

$ErrorActionPreference = "Stop"
$ProgressPreference = "SilentlyContinue"

$exe = Join-Path $Dir "atracdenc.exe"
if (Test-Path $exe) {
    Write-Host "[跳过] 已经装着：$exe"
    exit 0
}

$zip = Join-Path $env:TEMP "atracdenc-$Version.zip"
$direct = "https://github.com/dcherednik/atracdenc/releases/download/$Version/atracdenc-win-x86_$Version.zip"
$urls = @($direct)

try {
    $rel = Invoke-RestMethod -Uri "https://api.github.com/repos/dcherednik/atracdenc/releases/latest" -TimeoutSec 30
    $asset = $rel.assets | Where-Object { $_.name -like "*win*.zip" } | Select-Object -First 1
    if ($asset) {
        if ($rel.tag_name) { $Version = $rel.tag_name }
        # asset 端点必须带 Accept，否则返回 JSON 元数据
        $urls = @($asset.url) + $urls
        Write-Host "最新版本：$Version（$($asset.name)）"
    }
} catch {
    Write-Host "（API 不可用，退回钉住的 $Version）"
}

function Get-Pkg([string]$url) {
    if (Test-Path $zip) { Remove-Item $zip -Force }
    # api.github.com 的 asset 端点要让服务端返回二进制，不能缺 Accept
    $isApi = $url -like "https://api.github.com/*"
    try {
        if ($isApi) {
            Invoke-WebRequest -Uri $url -Headers @{ Accept = "application/octet-stream" } `
                -OutFile $zip -UseBasicParsing -TimeoutSec 600
        } else {
            Invoke-WebRequest -Uri $url -OutFile $zip -UseBasicParsing -TimeoutSec 600
        }
    } catch {
        & curl.exe -L --max-time 600 -o $zip $url 2>&1 | Out-Null
    }
    return (Test-Path $zip) -and ((Get-Item $zip).Length -gt 50000)
}

$ok = $false
foreach ($u in $urls) {
    Write-Host "尝试下载：$u"
    if (Get-Pkg $u) { $ok = $true; break }
}
if (-not $ok) {
    Write-Host ""
    Write-Host "下载失败。手动装只有三步：" -ForegroundColor Yellow
    Write-Host "  1. 浏览器打开 https://github.com/dcherednik/atracdenc/releases"
    Write-Host "  2. 下 atracdenc-win-x86_*.zip，解压出 x86\atracdenc.exe"
    Write-Host "  3. 把它放到 $Dir\atracdenc.exe，然后 设置 → 重新检测外部工具"
    exit 1
}

Write-Host "解压…"
$tmp = Join-Path $env:TEMP "atracdenc-unz"
if (Test-Path $tmp) { Remove-Item $tmp -Recurse -Force }
Expand-Archive -Path $zip -DestinationPath $tmp -Force
$found = Get-ChildItem $tmp -Recurse -File -Filter "atracdenc.exe" | Select-Object -First 1
if (-not $found) { throw "压缩包里没找到 atracdenc.exe" }

New-Item -ItemType Directory -Path $Dir -Force | Out-Null
Copy-Item $found.FullName $exe -Force

Remove-Item $tmp -Recurse -Force
Remove-Item $zip -Force

Write-Host "完成：$exe"
Write-Host "回到程序里点 设置 → 重新检测外部工具，ATRAC1 / ATRAC3 / ATRAC3+ 就能选了。"
