# 生成测试资产并跑真实编码矩阵（默认 ignored 的集成测试）。
#
#   powershell -ExecutionPolicy Bypass -File tools\run-integration.ps1

$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
Set-Location $root

& powershell -NoProfile -ExecutionPolicy Bypass -File (Join-Path $PSScriptRoot 'make-test-assets.ps1')
if ($LASTEXITCODE -ne 0) { throw '生成测试资产失败' }

$cargo = Join-Path $env:USERPROFILE '.cargo\bin\cargo.exe'
if (-not (Test-Path $cargo)) { $cargo = 'cargo' }
& $cargo test -p fc-core --test convert_matrix -- --ignored --nocapture
exit $LASTEXITCODE
