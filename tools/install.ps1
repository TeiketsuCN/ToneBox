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

$AppDir   = 'D:\Apps\FormatConverter'
$Version  = '0.1.0'
$ExeName  = 'format-converter.exe'
$Display  = '格式转换器'

Write-Host "== 1/4 构建 release ==" -ForegroundColor Cyan
# cargo 往 stderr 写 warning，配合 $ErrorActionPreference='Stop' 会被当成错误中断，
# 所以构建阶段临时放宽，构建完再收紧。
$ErrorActionPreference = 'Continue'
& "D:\FormatConverter\build.ps1" release 2>&1 | Out-Null
$ErrorActionPreference = 'Stop'
$built = 'D:\FormatConverter\target\release\format-converter.exe'
if (-not (Test-Path $built)) { throw "构建产物不存在: $built" }

Write-Host "== 2/4 安装到 $AppDir ==" -ForegroundColor Cyan
New-Item -ItemType Directory -Force -Path $AppDir | Out-Null
New-Item -ItemType Directory -Force -Path "$AppDir\ffmpeg" | Out-Null

Get-Process -Name 'format-converter','格式转换器' -ErrorAction SilentlyContinue | Stop-Process -Force
Start-Sleep -Milliseconds 500

Copy-Item -LiteralPath $built -Destination "$AppDir\$ExeName" -Force

# 随包 ffmpeg（LGPL 共享版，由 tools\fetch-ffmpeg.ps1 下载）
$ffSrc = "D:\FormatConverter\vendor\ffmpeg"
if (Test-Path (Join-Path $ffSrc "ffmpeg.exe")) {
    Get-ChildItem -LiteralPath $ffSrc -File | ForEach-Object {
        Copy-Item -LiteralPath $_.FullName -Destination "$AppDir\ffmpeg\$($_.Name)" -Force
    }
    Write-Host "已随包 ffmpeg"
} else {
    Write-Host "警告：没有 vendor\ffmpeg，安装后需另外装 ffmpeg（跑 tools\fetch-ffmpeg.ps1）"
}
@"
格式转换器 $Version
安装时间: $(Get-Date -Format 'yyyy-MM-dd HH:mm')
源码: D:\FormatConverter
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
  $s.Save()
  Write-Host "  $lnk"
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

Copy-Item -LiteralPath 'D:\FormatConverter\tools\uninstall.ps1' -Destination "$AppDir\uninstall.ps1" -Force

Write-Host "`n安装完成: $AppDir\$ExeName" -ForegroundColor Green
Write-Host "桌面 + 开始菜单快捷方式已就位，可在「应用和功能」里卸载。"
