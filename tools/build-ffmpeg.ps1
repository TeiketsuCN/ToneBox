# 构建带语音编码库的 LGPL ffmpeg（Windows 入口）
#
# 这个脚本只做三件事：
#   1. 找到（或装好）MSYS2
#   2. 在 UCRT64 环境里调用 tools/build-ffmpeg.sh
#   3. 报告产物
#
# 真正干活的是 build-ffmpeg.sh —— 依赖包、liblc3、ffmpeg 的 configure/make 都在那边。
#
# 用法：
#   powershell -ExecutionPolicy Bypass -File tools\build-ffmpeg.ps1
#   powershell -ExecutionPolicy Bypass -File tools\build-ffmpeg.ps1 -Jobs 4
#
# 注意：
#   - 首次运行会装十几 GB 的依赖（MSYS2 + 工具链），并编译 20~40 分钟
#   - 产物直接写进 vendor\ffmpeg，覆盖现有随包版本；想保留旧的先自己备份
#   - 构建保持 LGPL（不带 GPL 组件），和现在的随包版本许可一致

param(
    [int]$Jobs = 0,
    [string]$Msys2 = 'C:\msys64',
    [string]$FfmpegVersion = '8.0'
)

$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
$script = Join-Path $root 'tools\build-ffmpeg.sh'

if (-not (Test-Path $script)) {
    throw "找不到 $script"
}

# ── 1. MSYS2 ────────────────────────────────────────────
$bash = Join-Path $Msys2 'usr\bin\bash.exe'
if (-not (Test-Path $bash)) {
    Write-Host "没找到 MSYS2（$Msys2），尝试用 winget 安装…" -ForegroundColor Yellow
    $winget = Get-Command winget -ErrorAction SilentlyContinue
    if (-not $winget) {
        throw "没有 winget。请手动安装 MSYS2（https://www.msys2.org/），或用 -Msys2 指定安装路径。"
    }
    & winget install --id MSYS2.MSYS2 --accept-source-agreements --accept-package-agreements --silent
    if ($LASTEXITCODE -ne 0) {
        throw "winget 安装 MSYS2 失败（退出码 $LASTEXITCODE）。请手动安装后重跑。"
    }
    if (-not (Test-Path $bash)) {
        throw "安装完还是找不到 $bash，请用 -Msys2 指定实际路径。"
    }
}

# ── 2. 把 Windows 路径转成 MSYS 路径 ───────────────────
# C:\a\b  ->  /c/a/b
function To-MsysPath([string]$p) {
    $full = [System.IO.Path]::GetFullPath($p)
    $drive = $full.Substring(0, 1).ToLower()
    $rest = $full.Substring(2) -replace '\\', '/'
    "/$drive$rest"
}
$msysRoot = To-MsysPath $root
$msysScript = "$msysRoot/tools/build-ffmpeg.sh"

# ── 3. 在 UCRT64 环境里跑 ───────────────────────────────
$env:CHERE_INVOKING = '1'
$env:MSYSTEM = 'UCRT64'
$env:MSYS2_PATH_TYPE = 'inherit'
# 线程数只在用户指定时才传；否则交给 sh 里的 "${JOBS:-$(nproc)}" 自己算
# （之前写成 PowerShell 字符串里的 '$(nproc)' 会被当字面量，make -j 直接报错）
$jobsExpr = if ($Jobs -gt 0) { "export JOBS='$Jobs' && " } else { '' }
$cmd = "cd '$msysRoot' && " +
       $jobsExpr +
       "export FFMPEG_VERSION='$FfmpegVersion' && " +
       "bash '$msysScript'"

Write-Host "==> 在 MSYS2 UCRT64 里构建 ffmpeg（第一次会很慢）" -ForegroundColor Cyan
& $bash -lc $cmd
if ($LASTEXITCODE -ne 0) {
    throw "构建失败（退出码 $LASTEXITCODE）"
}

# ── 4. 报告 ─────────────────────────────────────────────
$out = Join-Path $root 'vendor\ffmpeg'
$ff = Join-Path $out 'ffmpeg.exe'
if (-not (Test-Path $ff)) {
    throw "构建成功但没找到 $ff"
}
Write-Host "==> 产物清单" -ForegroundColor Cyan
Get-ChildItem $out -File | Sort-Object Name |
    Select-Object Name, @{ n = 'MB'; e = { [math]::Round($_.Length / 1MB, 1) } } |
    Format-Table -AutoSize

Write-Host "==> 新增编码器自检" -ForegroundColor Cyan
$enc = (& $ff -hide_banner -encoders 2>&1) -join "`n"
foreach ($e in @('libgsm', 'libvo_amrwbenc', 'libilbc', 'liblc3', 'libcodec2')) {
    "{0,-16} {1}" -f $e, $(if ($enc -match "(^|\s)$e(\s|`)") { '有' } else { '缺失（链接失败？）' })
}

Write-Host "==> 主机音乐解复用自检（libgme）" -ForegroundColor Cyan
$dem = (& $ff -hide_banner -demuxers 2>&1) -join "`n"
"gme (nsf/spc/vgm/psf)  {0}" -f $(if ($dem -match '(?m)^\s*D\s+libgme\s') { '有' } else { '缺失（libgme 没链上？）' })

Write-Host "==> 别忘了跑一遍 cargo test -p fc-core 确认格式表还对得上" -ForegroundColor Yellow
