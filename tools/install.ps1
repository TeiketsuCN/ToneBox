# 安装脚本：把 release 产物装到固定位置，并建立标准入口
#
# 为什么不用 Program Files：
#   写 Program Files 需要管理员权限，开发期每次改代码重新编译都弹一次 UAC。
#   现在装到用户目录（D:\Apps），免提权；等发布给外人时再出 Inno Setup 安装包。
#
# 名字策略：
#   exe 用 ASCII（format-converter.exe），避免某些工具处理中文文件名出问题；
#   快捷方式用中文（格式转换器），用户看到的是中文。

$ErrorActionPreference = 'Stop'

# 脚本在 tools\ 下，项目根目录 = 上一级；换机器、换盘符都不用改
$src = Split-Path -Parent $PSScriptRoot
# D 盘没有就装到用户目录（新机器可能只有 C:）
$AppDir = if (Test-Path 'D:\') { 'D:\Apps\FormatConverter' } else { Join-Path $env:LOCALAPPDATA 'Programs\FormatConverter' }

$Version  = '1.0.0-beta.1'
$ExeName  = 'format-converter.exe'
$Display  = 'ToneBox'
# 老名字的快捷方式要在安装时删掉，不然桌面上会并排两个
$OldDisplay = '格式转换器'

Write-Host "== 1/4 构建 release ==" -ForegroundColor Cyan
# cargo 往 stderr 写 warning，配合 $ErrorActionPreference='Stop' 会被当成错误中断，
# 所以构建阶段临时放宽，构建完再收紧。
$ErrorActionPreference = 'Continue'
& (Join-Path $src "build.ps1") release 2>&1 | Out-Null
$ErrorActionPreference = 'Stop'
$targetRoot = if ($env:CARGO_TARGET_DIR) { $env:CARGO_TARGET_DIR } else { Join-Path $src 'target' }
$built = Join-Path $targetRoot 'release\format-converter.exe'
if (-not (Test-Path $built)) { throw "构建产物不存在: $built" }

Write-Host "== 2/4 安装到 $AppDir ==" -ForegroundColor Cyan
New-Item -ItemType Directory -Force -Path $AppDir | Out-Null
New-Item -ItemType Directory -Force -Path "$AppDir\ffmpeg" | Out-Null

Get-Process -Name 'format-converter','格式转换器' -ErrorAction SilentlyContinue | Stop-Process -Force
Start-Sleep -Milliseconds 500

Copy-Item -LiteralPath $built -Destination "$AppDir\$ExeName" -Force
Copy-Item -LiteralPath (Join-Path $src 'crates\fc-gui\assets\icon.ico') -Destination "$AppDir\ToneBox.ico" -Force

# 随包 ffmpeg（LGPL 共享版，由 tools\fetch-ffmpeg.ps1 下载）
$ffSrc = Join-Path $src 'vendor\ffmpeg'
if (Test-Path (Join-Path $ffSrc "ffmpeg.exe")) {
    Get-ChildItem -LiteralPath $ffSrc -File | ForEach-Object {
        Copy-Item -LiteralPath $_.FullName -Destination "$AppDir\ffmpeg\$($_.Name)" -Force
    }
    Write-Host "已随包 ffmpeg"
} else {
    Write-Host "警告：没有 vendor\ffmpeg，安装后需另外装 ffmpeg（跑 tools\fetch-ffmpeg.ps1）"
}
@"
ToneBox $Version
安装时间: $(Get-Date -Format 'yyyy-MM-dd HH:mm')
源码: $src
"@ | Set-Content -Path "$AppDir\version.txt" -Encoding UTF8

Write-Host "== 3/4 创建快捷方式 ==" -ForegroundColor Cyan
$ws = New-Object -ComObject WScript.Shell
$targets = @(
  (Join-Path ([Environment]::GetFolderPath('Desktop'))        "$Display.lnk"),
  (Join-Path ([Environment]::GetFolderPath('Programs'))       "$Display.lnk")
)
foreach ($lnk in $targets) {
  $s = $ws.CreateShortcut($lnk)
  $s.TargetPath       = "$AppDir\$ExeName"
  $s.WorkingDirectory = $AppDir
  $s.Description      = $Display
  $s.IconLocation     = "$AppDir\ToneBox.ico"
  $s.Save()
  Write-Host "  $lnk"
}
# 老名字的桌面/开始菜单快捷方式删掉，避免重名两个
# 注意：NTFS 不区分大小写，"tonebox.lnk" 会匹配到刚到建的 "ToneBox.lnk"，
# 所以这里只能删真正的旧名字（中文），不能按大小写变体删。
foreach ($lnk in @(
  (Join-Path ([Environment]::GetFolderPath('Desktop'))  "$OldDisplay.lnk"),
  (Join-Path ([Environment]::GetFolderPath('Programs')) "$OldDisplay.lnk")
)) {
  if (Test-Path -LiteralPath $lnk) { Remove-Item -LiteralPath $lnk -Force; Write-Host "  已删除旧快捷方式 $lnk" }
}

Write-Host "== 4/4 注册卸载项 ==" -ForegroundColor Cyan
$key = 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\FormatConverter'
New-Item -Path $key -Force | Out-Null
Set-ItemProperty -Path $key -Name DisplayName     -Value $Display
Set-ItemProperty -Path $key -Name DisplayVersion  -Value $Version
Set-ItemProperty -Path $key -Name Publisher       -Value 'FormatConverter (open source)'
Set-ItemProperty -Path $key -Name InstallLocation -Value $AppDir
Set-ItemProperty -Path $key -Name DisplayIcon     -Value "$AppDir\$ExeName"
Set-ItemProperty -Path $key -Name NoModify        -Value 1 -Type DWord
Set-ItemProperty -Path $key -Name NoRepair        -Value 1 -Type DWord
Set-ItemProperty -Path $key -Name UninstallString -Value "powershell.exe -NoProfile -ExecutionPolicy Bypass -File `"$AppDir\uninstall.ps1`""

Copy-Item -LiteralPath (Join-Path $src 'tools\uninstall.ps1') -Destination "$AppDir\uninstall.ps1" -Force

& "$env:SystemRoot\System32\ie4uinit.exe" -show 2>$null | Out-Null
Write-Host "`n安装完成: $AppDir\$ExeName" -ForegroundColor Green
Write-Host "桌面 + 开始菜单快捷方式已就位，可在「应用和功能」里卸载。"
