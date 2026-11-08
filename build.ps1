# 构建脚本：本机没有 MSVC，用 gnu 工具链 + WinLibs 的 binutils
# 路径按"脚本所在目录 + WinGet 实际安装位置"推，换机器、换用户名都不用改
$root = Split-Path -Parent $PSCommandPath
$winlibs = Get-ChildItem "$env:LOCALAPPDATA\Microsoft\WinGet\Packages\BrechtSanders.WinLibs.POSIX.UCRT_*\mingw64\bin" -Directory -ErrorAction SilentlyContinue |
  Select-Object -First 1
$paths = @("$env:USERPROFILE\.cargo\bin")
if ($winlibs) { $paths += $winlibs.FullName }
$paths += $env:PATH
$env:PATH = $paths -join ';'

Set-Location $root
if ($args -contains 'release') {
  cargo build --release 2>&1
} else {
  cargo build 2>&1
}
exit $LASTEXITCODE