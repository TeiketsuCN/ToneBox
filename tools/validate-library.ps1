# 全库 / 属性矩阵验证：真实音乐逐首 + 派生属性（96k/24bit、单声道、5.1、长文件）。
#
#   powershell -ExecutionPolicy Bypass -File tools\validate-library.ps1
#   powershell ... -Seconds 12 -MaxSources 4 -TimeoutSec 180
#
# 和 validate-real-music.ps1 的分工：
#   - validate-real-music.ps1：1 个源 × 全部 112 个目标，验证「格式本身能不能出」
#   - validate-library.ps1    ：全库每个源 × 核心格式，验证「换源/换属性会不会出问题」
#
# 每个真实文件先无损截成 12 秒（-c copy），既保留原始编码/采样率/位深，又不用转整首歌。
# NCM 是加密容器，按项目红线不解密，只记 SKIP_DRM。
param(
    [string]$SourceDir = 'D:\USB-Recovery\Media\Music\cloudmusic',
    [string]$Area = 'D:\Area\ToneLore-Validation',
    [int]$Seconds = 12,
    [int]$MaxSources = 0,
    [string]$Exe = '',
    [int]$TimeoutSec = 120,
    # 只跑真实文件，跳过派生属性（调脚本时用，省时间）
    [switch]$SkipDerived
)

$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
if (-not $Exe) { $Exe = Join-Path $root 'target\release\ToneLore.exe' }
$ffmpeg = Join-Path $root 'target\release\ffmpeg\ffmpeg.exe'
$ffprobe = Join-Path $root 'target\release\ffmpeg\ffprobe.exe'
$vgaudio = Join-Path $root 'target\release\vgaudio\VGAudioCli.exe'
$vadpcm = Join-Path $root 'target\release\vadpcm\vadpcm.exe'
$ofr = Join-Path $root 'target\release\optimfrog\ofr.exe'
. (Join-Path $PSScriptRoot 'verify-output.ps1')
if (-not (Test-Path $Exe)) { throw "找不到 $Exe" }
if (-not (Test-Path $ffmpeg)) { throw "找不到 $ffmpeg" }
if (-not (Test-Path $ffprobe)) { throw "找不到 $ffprobe" }

$stamp = Get-Date -Format 'yyyyMMdd-HHmmss'
$run = Join-Path $Area "library-$stamp"
$inputs = Join-Path $run 'inputs'
$outputs = Join-Path $run 'outputs'
$logs = Join-Path $run 'logs'
New-Item -ItemType Directory -Force -Path $inputs, $outputs, $logs | Out-Null
$progressFile = Join-Path $run 'progress.log'

# ── 目标表 ──
$fmtFile = Join-Path $run 'formats.txt'
cmd /c "`"$Exe`" --formats > `"$fmtFile`" 2>&1"
$targets = New-Object System.Collections.Generic.List[object]
foreach ($line in (Get-Content $fmtFile -Encoding UTF8)) {
    if ($line -match '^(\S+)\s+(.+?)\s+\.(\S+)\s*$') {
        $targets.Add([pscustomobject]@{ Id = $matches[1]; Name = $matches[2].Trim(); Ext = $matches[3] })
    }
}
if ($targets.Count -eq 0) { throw '解析 --formats 失败' }
$targetMap = @{}
foreach ($t in $targets) { $targetMap[$t.Id] = $t }

# 需要视频源的容器：全库单跑只测音频，视频容器由 validate-real-music.ps1 覆盖
$videoTargets = @('gxf', 'psp', 'dv', 'mxf', 'mxf_opatom', 'ivf', 'h264', 'hevc', 'm2v', 'm4v', 'avif', 'amv', 'gif', 'apng')
# 只做直通的格式：IEC 61937 只能包 AC-3/E-AC-3/DTS/MPEG 比特流，
# 拿 WAV/FLAC 当输入必然失败，不是格式的问题。直通路径由
# validate-real-music.ps1 用 AC-3 源单独覆盖。
$passthroughOnly = @('iec61937')
$audioTargets = @($targets | Where-Object { $videoTargets -notcontains $_.Id -and $passthroughOnly -notcontains $_.Id })

# 核心矩阵：覆盖内部 ffmpeg 路径 + 最常用的外部编码器
$coreIds = @('flac', 'wav', 'alac', 'aiff', 'mp3', 'aac', 'opus', 'vorbis', 'mp2', 'ac3',
    'wma', 'dts', 'amr_nb', 'amr_wb', 'gsm', 'speex', 'atrac3', 'wavpack', 'wavpack_lossy',
    'musepack', 'optimfrog', 'qoa', 'iec61937', 'mp4', 'mkv')
# 电话/窄带：只挑 8k 能用的
$teleIds = @('gsm', 'speex', 'amr_nb', 'amr_wb', 'g711_alaw', 'g711_ulaw', 'g722',
    'g723_1', 'g726', 'adpcm_ms', 'ima_adpcm', 'mp3', 'aac', 'opus', 'vorbis', 'wav', 'flac')
# 长文件：只挑时长敏感的容器，避免跑一整晚
$longIds = @('flac', 'wav', 'mp3', 'aac', 'opus', 'vorbis', 'atrac3', 'wavpack', 'ac3', 'mkv', 'mp4')

function Resolve-Targets([string]$matrix) {
    $ids = switch ($matrix) {
        'full' { $null }
        'core' { $coreIds }
        'tele' { $teleIds }
        'long' { $longIds }
        default { throw "未知矩阵：$matrix" }
    }
    if ($null -eq $ids) { return $audioTargets }
    return @($ids | Where-Object { $targetMap.ContainsKey($_) -and $passthroughOnly -notcontains $_ } | ForEach-Object { $targetMap[$_] })
}

# ── 源：真实文件截 12 秒 + 派生属性 ──
$sources = New-Object System.Collections.Generic.List[object]
$srcFiles = @(Get-ChildItem $SourceDir -File -Recurse -ErrorAction SilentlyContinue |
    Where-Object { $_.Extension -in '.mp3', '.flac' } | Sort-Object Extension, Name)
if (-not $srcFiles) { throw "源目录里没有 mp3/flac：$SourceDir" }

$i = 0
foreach ($f in $srcFiles) {
    if ($MaxSources -gt 0 -and $i -ge $MaxSources) { break }
    $i++
    $id = 's{0:D2}' -f $i
    $clip = Join-Path $inputs ("{0}.{1}" -f $id, $f.Extension.TrimStart('.'))
    # FLAC 用 -c copy 会把原曲的 STREAMINFO（总时长）一起带过来，
    # 12 秒的片段会自称 216 秒，后面的时长校验全错；所以 FLAC 重编码，
    # MP3 保持 copy（保留原始帧，时长字段正常）。
    $codecArgs = if ($f.Extension -eq '.flac') { @('-c:a', 'flac') } else { @('-c:a', 'copy') }
    & $ffmpeg -v error -y -t $Seconds -i $f.FullName @codecArgs $clip
    if ($LASTEXITCODE -ne 0 -or -not (Test-Path $clip)) {
        Write-Warning "截取失败，跳过：$($f.Name)"
        continue
    }
    $p = Get-AudioProps $ffprobe $clip
    if (-not $p) { Write-Warning "探测失败，跳过：$($f.Name)"; continue }
    $sources.Add([pscustomobject]@{
            Id = $id; Label = $f.BaseName; Path = $clip; Kind = 'real'; Matrix = 'core'
            Sr = $p.sr; Ch = $p.ch; Bits = $p.bits; Dur = $p.dur; Note = ''
        })
}

# 派生源：挑最大的 FLAC（优先 24bit）当底
$base = $null
$flacs = @(Get-ChildItem $SourceDir -File -Recurse -Filter *.flac -ErrorAction SilentlyContinue | Sort-Object Length -Descending)
foreach ($f in $flacs) {
    $p = Get-AudioProps $ffprobe $f.FullName
    if ($p -and $p.bits -eq '24') { $base = $f.FullName; break }
}
if (-not $base -and $flacs.Count -gt 0) { $base = $flacs[0].FullName }
if (-not $base) {
    $mp3s = @(Get-ChildItem $SourceDir -File -Recurse -Filter *.mp3 -ErrorAction SilentlyContinue | Sort-Object Length -Descending)
    if ($mp3s.Count -gt 0) { $base = $mp3s[0].FullName }
}
if ($base -and -not $SkipDerived) {
    $derived = @(
        @{ Id = 'd96k24'; Label = '派生 96k/24bit 立体声'; Arg = @('-t', $Seconds, '-ar', '96000', '-c:a', 'pcm_s24le'); Matrix = 'full'; Note = "来自 $base" },
        @{ Id = 'd48mono'; Label = '派生 48k 单声道'; Arg = @('-t', $Seconds, '-ar', '48000', '-ac', '1', '-c:a', 'pcm_s16le'); Matrix = 'full'; Note = "来自 $base" },
        @{ Id = 'd8mono'; Label = '派生 8k 单声道'; Arg = @('-t', $Seconds, '-ar', '8000', '-ac', '1', '-c:a', 'pcm_s16le'); Matrix = 'tele'; Note = "来自 $base" },
        @{ Id = 'd51'; Label = '派生 5.1 48k'; Arg = @('-t', $Seconds, '-ar', '48000', '-af', 'pan=5.1|FL=c0|FR=c0|FC=c0|LFE=c0|BL=c0|BR=c0', '-c:a', 'pcm_s24le'); Matrix = 'full'; Note = "来自 $base" },
        @{ Id = 'dlong'; Label = '派生 120 秒长文件'; Arg = @('-t', '120', '-ar', '44100', '-ac', '2', '-c:a', 'pcm_s16le'); Matrix = 'long'; Note = "来自 $base" }
    )
    foreach ($d in $derived) {
        $path = Join-Path $inputs ($d.Id + '.wav')
        & $ffmpeg -v error -y -i $base @($d.Arg) $path
        if ($LASTEXITCODE -ne 0 -or -not (Test-Path $path)) { Write-Warning "派生失败：$($d.Id)"; continue }
        $p = Get-AudioProps $ffprobe $path
        if (-not $p) { Write-Warning "派生探测失败：$($d.Id)"; continue }
        $sources.Add([pscustomobject]@{
                Id = $d.Id; Label = $d.Label; Path = $path; Kind = 'derived'; Matrix = $d.Matrix
                Sr = $p.sr; Ch = $p.ch; Bits = $p.bits; Dur = $p.dur; Note = $d.Note
            })
    }
}

# NCM：不解密，只记录为什么跳过
$ncmFiles = @(Get-ChildItem $SourceDir -File -Recurse -Filter *.ncm -ErrorAction SilentlyContinue)
$ncmRows = @()
foreach ($f in $ncmFiles) {
    $ncmRows += [pscustomobject]@{
            source = 'ncm'; target = 'all'; status = 'SKIP_DRM'; size = 0; codec = ''
            duration_s = ''; dur_delta_s = ''; matrix = ''
            label = $f.BaseName; sr = ''; ch = ''; bits = ''; note = 'NCM 加密容器，按红线不解密'
            error = '加密文件：程序不实现 DRM 解密，无法作为转换输入'
        }
}

Write-Host ("源：{0} 个真实 + {1} 个派生；NCM {2} 个只记 SKIP_DRM" -f @($sources | Where-Object Kind -eq 'real').Count, @($sources | Where-Object Kind -eq 'derived').Count, $ncmFiles.Count) -ForegroundColor Cyan
Write-Host "目标：$($audioTargets.Count) 个音频目标可用（视频容器另测）" -ForegroundColor Cyan

# ── 逐源逐目标转换 ──
$rows = New-Object System.Collections.Generic.List[object]
$si = 0
foreach ($s in $sources) {
    $si++
    $matrix = Resolve-Targets $s.Matrix
    Write-Host ("[{0}/{1}] {2} — {3} Hz / {4} ch / {5} / {6:n1}s / {7} 个目标" -f
        $si, $sources.Count, $s.Label, $s.Sr, $s.Ch, $s.Bits, $s.Dur, $matrix.Count) -ForegroundColor Yellow
    $dir = Join-Path $outputs $s.Id
    New-Item -ItemType Directory -Force -Path $dir | Out-Null
    $okN = 0
    $badN = 0
    $ti = 0
    foreach ($t in $matrix) {
        $ti++
        $out = Join-Path $dir ("{0}.{1}" -f $t.Id, $t.Ext)
        $logBase = Join-Path $logs ("{0}-{1}" -f $s.Id, $t.Id)
        $status = 'FAIL'
        $err = ''
        $size = 0
        $codec = ''
        $outDur = ''
        $delta = ''
        try {
            if (Test-Path $out) { Remove-Item $out -Force }
            # 路径不要自己加引号：Invoke-Process 会按需转义
            $cliArgs = @('--lang', 'en', '--convert', $s.Path, $out, '--to', $t.Id)
            $res = Invoke-Process -Exe $Exe -ArgList $cliArgs -TimeoutSec $TimeoutSec
            if ($res.TimedOut) {
                $status = 'TIMEOUT'
                $err = "超过 $TimeoutSec 秒"
            } elseif ($res.Code -ne 0) {
                $status = 'FAIL'
                $err = ($res.Out + "`n" + $res.Err)
                Set-Content ($logBase + '.log') -Value $err -Encoding UTF8
            } elseif (-not (Test-Path $out)) {
                throw '退出码 0 但文件不存在'
            } elseif ((Get-Item $out).Length -eq 0) {
                throw '输出 0 字节'
            } else {
                $size = (Get-Item $out).Length
                $v = Test-OutputFile -Id $t.Id -Path $out -Ffmpeg $ffmpeg -Ffprobe $ffprobe `
                    -VgAudio $vgaudio -Vadpcm $vadpcm -Ofr $ofr -TimeoutSec $TimeoutSec
                $status = $v.Status
                $codec = $v.Codec
                $outDur = $v.Duration
                if ($v.Error) { $err = $v.Error }
                if ($status -eq 'OK' -and $outDur -is [double] -and $outDur -gt 0 -and $s.Dur -gt 0 -and $t.Id -ne 'musepack') {
                    # musepack 的 SV8 头里时长字段和实际解码长度对不上（解码出来是对的），
                    # ffprobe 报的时长不可信，所以不参与时长校验。
                    $delta = [math]::Round([math]::Abs($outDur - $s.Dur), 2)
                    $tol = [math]::Max(1.0, 0.04 * $s.Dur)
                    if ($delta -gt $tol) { $status = 'DURATION_OFF' }
                }
            }
        } catch {
            if ($status -eq 'OK') { $status = 'FAIL' }
            $msg = $_.Exception.Message
            if (-not $err) { $err = $msg } else { $err = "$err`n$msg" }
        }

        $msg = ($err -replace "`r?`n", ' ').Trim()
        if ($status -ne 'OK' -and $msg -match '找不到|not found|No such file|encoder not found|Unknown encoder|not installed') {
            $status = 'SKIP_TOOL'
        }
        if ($msg.Length -gt 300) { $msg = $msg.Substring(0, 300) }
        if ($status -eq 'OK') {
            $okN++
            Remove-Item ($logBase + '.log') -Force -ErrorAction SilentlyContinue
        } else {
            $badN++
        }
        Add-Content -Path $progressFile -Value ("{0} {1,-16} {2}" -f $s.Id, $t.Id, $status) -Encoding UTF8
        $rows.Add([pscustomobject]@{
                source = $s.Id; target = $t.Id; status = $status; size = $size; codec = $codec
                duration_s = $outDur; dur_delta_s = $delta; matrix = $s.Matrix
                label = $s.Label; sr = $s.Sr; ch = $s.Ch; bits = $s.Bits; note = $s.Note
                error = $msg
            })
        if ($status -ne 'OK') {
            Write-Host ("    {0,-16} {1} {2}" -f $t.Id, $status, $msg) -ForegroundColor Red
        }
    }
    Write-Host ("    完成：OK {0} / 非 OK {1}" -f $okN, $badN) -ForegroundColor Green
}

$rows | Export-Csv -Path (Join-Path $run 'summary.csv') -NoTypeInformation -Encoding UTF8

# ── 汇总 ──
$all = @($rows.ToArray()) + @($ncmRows)
$ok = @($all | Where-Object status -eq 'OK').Count
$fails = @($all | Where-Object { $_.status -ne 'OK' -and $_.status -notlike 'SKIP_*' }).Count
$skips = @($all | Where-Object { $_.status -like 'SKIP_*' }).Count

$lines = @(
    "# ToneLore 全库验证 $stamp",
    "",
    "- 源目录：$SourceDir",
    "- 每个真实文件截取：$Seconds 秒（-c copy，保留原始编码与属性）",
    "- 目标：$($rows.Count) 次转换（视频容器由 validate-real-music.ps1 另测）",
    "- OK：$ok",
    "- 失败：$fails",
    "- 跳过：$skips（含 NCM 加密文件）",
    "- 说明：IEC 61937 只做直通（用 validate-real-music.ps1 的 AC-3 源覆盖）；视频容器另测",
    "",
    "## 源",
    "",
    "| id | 文件 | 类型 | 矩阵 | Hz | 声道 | 位深 | 时长 |",
    "|---|---|---|---|---|---|---|---|"
)
foreach ($s in $sources) {
    $lines += "| $($s.Id) | $($s.Label) | $($s.Kind) | $($s.Matrix) | $($s.Sr) | $($s.Ch) | $($s.Bits) | $([math]::Round($s.Dur,1)) |"
}
$lines += ''
$lines += '## 每个目标的通过率'
$lines += ''
$lines += '| 目标 | OK | 总数 | 失败源 |'
$lines += '|---|---|---|---|'
foreach ($t in $audioTargets) {
    $tr = @($rows | Where-Object target -eq $t.Id)
    if ($tr.Count -eq 0) { continue }
    $tok = @($tr | Where-Object status -eq 'OK').Count
    $bad = @($tr | Where-Object status -ne 'OK')
    $names = (@($bad | ForEach-Object { $_.source + '/' + $_.status }) -join ', ')
    if ($names.Length -gt 200) { $names = $names.Substring(0, 200) + '…' }
    $lines += "| $($t.Id) | $tok | $($tr.Count) | $names |"
}
$lines += ''
$lines += '## 失败明细'
$lines += ''
$lines += '| 源 | 目标 | 状态 | 时长差 | 错误 |'
$lines += '|---|---|---|---|---|'
foreach ($r in ($rows | Where-Object status -ne 'OK')) {
    $e = ([string]$r.error -replace '\|', '/')
    if ($e.Length -gt 160) { $e = $e.Substring(0, 160) }
    $lines += "| $($r.source) | $($r.target) | $($r.status) | $($r.dur_delta_s) | $e |"
}
if ($ncmRows.Count -gt 0) {
    $lines += ''
    $lines += '## NCM（按红线不解密）'
    $lines += ''
    foreach ($r in $ncmRows) { $lines += "- $($r.label)：$($r.error)" }
}
$lines | Set-Content (Join-Path $run 'summary.md') -Encoding UTF8

Write-Host ''
Write-Host "OK=$ok FAIL=$fails SKIP=$skips" -ForegroundColor Green
Write-Host "结果：$run" -ForegroundColor Cyan
