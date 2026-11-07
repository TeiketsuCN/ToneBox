# 构建脚本：本机没有 MSVC，用 gnu 工具链 + WinLibs 的 binutils
$env:PATH = @(
  "$env:USERPROFILE\.cargo\bin",
  "C:\Users\23501\AppData\Local\Microsoft\WinGet\Packages\BrechtSanders.WinLibs.POSIX.UCRT_Microsoft.Winget.Source_8wekyb3d8bbwe\mingw64\bin",
  $env:PATH
) -join ';'

Set-Location "D:\FormatConverter"
if ($args -contains 'release') {
  cargo build --release 2>&1
} else {
  cargo build 2>&1
}
exit $LASTEXITCODE