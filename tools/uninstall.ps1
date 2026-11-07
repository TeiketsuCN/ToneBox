# 卸载：删快捷方式 + 注册表项 + 程序目录
$ErrorActionPreference = 'SilentlyContinue'

$AppDir  = 'D:\Apps\FormatConverter'
$Display = '格式转换器'

Get-Process -Name 'format-converter','格式转换器' | Stop-Process -Force
Start-Sleep -Milliseconds 500

# 快捷方式
foreach ($lnk in @(
  (Join-Path ([Environment]::GetFolderPath('Desktop'))  "$Display.lnk"),
  (Join-Path ([Environment]::GetFolderPath('Programs')) "$Display.lnk")
)) {
  if (Test-Path -LiteralPath $lnk) { Remove-Item -LiteralPath $lnk -Force }
}

# 注册表
Remove-Item -Path 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\FormatConverter' -Recurse -Force

# 程序目录（先删自己，再删目录）
$me = $MyInvocation.MyCommand.Path
if ($me -and $me.StartsWith($AppDir, [StringComparison]::OrdinalIgnoreCase)) {
  Start-Process powershell.exe -ArgumentList @(
    '-NoProfile','-ExecutionPolicy','Bypass','-Command',
    "Start-Sleep -Milliseconds 600; Remove-Item -LiteralPath '$AppDir' -Recurse -Force"
  ) -WindowStyle Hidden
} else {
  Remove-Item -LiteralPath $AppDir -Recurse -Force
}

Write-Host "已卸载。"
