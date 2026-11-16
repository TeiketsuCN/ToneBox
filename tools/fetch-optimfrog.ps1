# 下载 OptimFROG 的命令行编码器（ofr.exe）到 vendor\optimfrog
#
# 注意：官方许可写明"把命令行编码器随其他软件分发可能需要授权"，
# 所以 ToneBox **不随发布包分发** ofr.exe。这个脚本只是给本地开发/测试用，
# 你自己发布时不要把 optimfrog\ 目录打进 zip。
#
# 用法：
#   powershell -ExecutionPolicy Bypass -File tools\fetch-optimfrog.ps1

$ErrorActionPreference = 'Stop'
$src = Split-Path -Parent $PSScriptRoot
$vendor = Join-Path $src 'vendor\optimfrog'
$tmp = Join-Path $env:TEMP ('tonebox_ofr_' + [guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Force -Path $vendor, $tmp | Out-Null

$base = 'http://losslessaudio.org/Downloads/A1C0/5100/Win/'
$name = 'OptimFROG_Win_x64_5100.zip'
$zip = Join-Path $tmp $name

Write-Host "downloading $base$name"
& curl.exe -L --fail --max-time 300 -o $zip ($base + $name)
if ($LASTEXITCODE -ne 0) { throw "download failed" }

# 官方 SHA-1 校验（.sha1 文件格式：<hash> *<filename>）
$sha1 = (Invoke-WebRequest -Uri ($base + $name + '.sha1') -UseBasicParsing -TimeoutSec 60).Content
$sha1Text = [System.Text.Encoding]::ASCII.GetString($sha1)
$expect = ($sha1Text -split '\s+')[0].ToLower()
$actual = (Get-FileHash $zip -Algorithm SHA1).Hash.ToLower()
if ($expect -ne $actual) { throw "SHA-1 mismatch: expect $expect, got $actual" }
Write-Host "SHA-1 ok: $actual"

& tar.exe -xf $zip -C $tmp
$inner = Join-Path $tmp 'OptimFROG_Win_x64_5100'
if (-not (Test-Path (Join-Path $inner 'ofr.exe'))) { throw "ofr.exe not found in package" }

Copy-Item (Join-Path $inner 'ofr.exe') $vendor -Force
Copy-Item (Join-Path $inner 'license.html') $vendor -Force
Copy-Item (Join-Path $inner 'changelog.txt') $vendor -Force

$notice = @"
OptimFROG / Lossless Audio (ofr.exe) —— 分发说明

来源：http://losslessaudio.org/  （OptimFROG 5.100, Win/x64, 2016-09-02）
SHA-1（原 zip）：$expect

许可（摘自 ofr.exe --help 与官方 license.html）：
  "Free for non-commercial and commercial use. However, distribution
   of command-line Encoders with other software may need permission."

因此 ToneBox **不随发布包分发 ofr.exe**：
- 仓库里的 vendor/optimfrog/ 只用于本地开发测试（vendor/ 已 gitignore）
- 发布包里不含它；用户需要自己从官网下载放到程序目录或 optimfrog\ 子目录
- 程序只在检测到它时调用，不做任何再分发

请保留 license.html 与 changelog.txt 一起，别单独拷 exe。
"@
Set-Content -Path (Join-Path $vendor 'NOTICE-distribution.txt') -Value $notice -Encoding UTF8

Remove-Item $tmp -Recurse -Force
Write-Host "OptimFROG installed to $vendor"
Get-ChildItem $vendor -File | Select-Object Name, @{n = 'KB'; e = { [math]::Round($_.Length / 1KB, 1) } } | Format-Table -AutoSize
