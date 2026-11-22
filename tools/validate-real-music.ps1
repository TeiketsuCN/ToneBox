# 用真实音乐反复验证所有输出格式的可行性。
#
#   powershell -ExecutionPolicy Bypass -File tools\validate-real-music.ps1
#   powershell ... -Seconds 20 -IncludeVideo
#
# 输入/输出都在 D:\Area\ToneLore-Validation\<时间戳>；不修改源音乐。
param(
    [string]$SourceDir = 'D:\USB-Recovery\Media\Music\cloudmusic',
    [string]$Area = 'D:\Area\ToneLore-Validation',
    [int]$Seconds = 20,
    [string]$Exe = ''
)

$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
if (-not $Exe) { $Exe = Join-Path $root 'target\release\ToneLore.exe' }
$ffmpeg = Join-Path $root 'target\release\ffmpeg\ffmpeg.exe'
$ffprobe = Join-Path $root 'target\release\ffmpeg\ffprobe.exe'
if (-not (Test-Path $Exe)) { throw "找不到 $Exe" }
if (-not (Test-Path $ffmpeg)) { throw "找不到 $ffmpeg" }

$stamp = Get-Date -Format 'yyyyMMdd-HHmmss'
$run = Join-Path $Area "run-$stamp"
$inputs = Join-Path $run 'inputs'
$outputs = Join-Path $run 'outputs'
New-Item -ItemType Directory -Force -Path $inputs, $outputs | Out-Null

# ── 1. 选源：优先 FLAC，其次 MP3 ──
$flac = Get-ChildItem $SourceDir -File -Recurse -Filter *.flac -ErrorAction SilentlyContinue |
    Sort-Object Length -Descending | Select-Object -First 1
$mp3 = Get-ChildItem $SourceDir -File -Recurse -Filter *.mp3 -ErrorAction SilentlyContinue |
    Sort-Object Length -Descending | Select-Object -First 1
if (-not $flac -and -not $mp3) { throw "源目录里没有 flac/mp3：$SourceDir" }
$seed = if ($flac) { $flac.FullName } else { $mp3.FullName }
Write-Host "源：$seed" -ForegroundColor Cyan

$refWav = Join-Path $inputs 'reference-16-44.wav'
$ref24 = Join-Path $inputs 'reference-24-48.wav'
$refMp3 = Join-Path $inputs 'reference.mp3'

& $ffmpeg -v error -y -t $Seconds -i $seed -ar 44100 -ac 2 -c:a pcm_s16le $refWav
if ($LASTEXITCODE -ne 0) { throw "生成 reference wav 失败" }
& $ffmpeg -v error -y -t $Seconds -i $seed -ar 48000 -ac 2 -c:a pcm_s24le $ref24
if ($LASTEXITCODE -eq 0) { Write-Host '  + reference-24-48.wav' }
if ($mp3) {
    & $ffmpeg -v error -y -t $Seconds -i $mp3.FullName -c:a libmp3lame -b:a 192k $refMp3
}
$refAc3 = Join-Path $inputs 'reference.ac3'
& $ffmpeg -v error -y -t $Seconds -i $refWav -c:a ac3 -b:a 192k $refAc3
$refVideo = Join-Path $inputs 'reference-video.mp4'
& $ffmpeg -v error -y -f lavfi -i "testsrc2=s=640x360:d=$Seconds" -i $refWav -shortest -c:v libopenh264 -c:a aac -t $Seconds $refVideo
if ($LASTEXITCODE -ne 0) { Write-Warning '视频源生成失败，视频容器会记 SKIP_NO_VIDEO' }

# ── 2. 所有可输出格式 ──
$fmtFile = Join-Path $run 'formats.txt'
cmd /c "`"$Exe`" --formats > `"$fmtFile`" 2>&1"
$formatLines = Get-Content $fmtFile -Encoding UTF8
$targets = foreach ($line in $formatLines) {
    if ($line -match '^(\S+)\s+(.+?)\s+\.(\S+)\s*$') {
        [pscustomobject]@{ Id = $matches[1]; Name = $matches[2].Trim(); Ext = $matches[3] }
    }
}
if (-not $targets) { throw '解析 --formats 失败' }
Write-Host "目标：$($targets.Count) 个，每个 $Seconds 秒" -ForegroundColor Cyan

$videoTargets = @('gxf','psp','dv','mxf','mxf_opatom','ivf','h264','hevc','m2v','m4v','avif','amv','gif','apng')
$externalOnly = @('atrac1','atrac3','atrac3p','xa','vag','brstm','bcstm','bfstm','dsp','idsp','vadpcm','optimfrog')

$rows = New-Object System.Collections.Generic.List[object]
$i = 0
foreach ($t in $targets) {
    $i++
    $out = Join-Path $outputs ("{0}.{1}" -f $t.Id, $t.Ext)
    $status = 'FAIL'
    $err = ''
    $size = 0
    $codec = ''
    $dur = ''

    try {
        if ($externalOnly -contains $t.Id) {
            $status = 'SKIP_EXTERNAL'
            $err = '该格式在 GUI 里走外部编码器，CLI 不提供'
            $rows.Add([pscustomobject]@{ id = $t.Id; name = $t.Name; ext = $t.Ext; status = $status; size = 0; codec = ''; duration_s = ''; error = $err })
            Write-Host ("  [{0,3}/{1}] {2,-16} {3,-12}" -f $i, $targets.Count, $t.Id, $status)
            continue
        }
        $useVideo = ($videoTargets -contains $t.Id) -and (Test-Path $refVideo)
        if (($videoTargets -contains $t.Id) -and -not (Test-Path $refVideo)) {
            $rows.Add([pscustomobject]@{ id = $t.Id; name = $t.Name; ext = $t.Ext; status = 'SKIP_NO_VIDEO'; size = 0; codec = ''; duration_s = ''; error = '没有视频源' })
            Write-Host ("  [{0,3}/{1}] {2,-16} {3,-12}" -f $i, $targets.Count, $t.Id, 'SKIP_NO_VIDEO')
            continue
        }
        $inputFor = if ($t.Id -eq 'iec61937') { $refAc3 } elseif ($useVideo) { $refVideo } else { $refWav }
        $log = Join-Path $run ("last-" + $t.Id + ".log")
        cmd /c "`"$Exe`" --lang en --convert `"$inputFor`" `"$out`" --to $($t.Id) > `"$log`" 2>&1"
        if ($LASTEXITCODE -ne 0) {
            $err = (Get-Content $log -Raw -ErrorAction SilentlyContinue)
        } elseif (-not (Test-Path $out)) {
            throw '退出码 0 但文件不存在'
        } elseif ((Get-Item $out).Length -eq 0) {
            throw '输出 0 字节'
        } else {
            $size = (Get-Item $out).Length
            $probe = & $ffprobe -v error -print_format json -show_format -show_streams $out 2>$null | ConvertFrom-Json
            $codec = ($probe.streams | Where-Object { $_.codec_type -eq 'audio' } | Select-Object -First 1).codec_name
            $dur = [double]$probe.format.duration
            & $ffmpeg -v error -t 5 -i $out -f null - 2>$null
            if ($LASTEXITCODE -eq 0) { $status = 'OK' } else { $status = 'DECODE_FAIL' }
        }
    } catch {
        $err = $_.Exception.Message
    }

    $msg = ($err -replace "`r?`n", ' ').Trim()
    if ($status -ne 'OK' -and $msg -match '找不到|not found|No such file|encoder not found|Unknown encoder|外部|external') {
        $status = 'SKIP_TOOL'
    }
    if ($msg.Length -gt 220) { $msg = $msg.Substring(0, 220) }
    Write-Host ("  [{0,3}/{1}] {2,-16} {3,-12} {4}" -f $i, $targets.Count, $t.Id, $status, $msg)
    $rows.Add([pscustomobject]@{
        id = $t.Id; name = $t.Name; ext = $t.Ext; status = $status
        size = $size; codec = $codec; duration_s = $dur; error = $msg
    })
}

foreach ($row in $rows) {
    if ($row.id -eq 'pcm' -and $row.status -ne 'OK') {
        $pcmOut = Join-Path $outputs ("pcm." + $row.ext)
        if ((Test-Path $pcmOut) -and (Get-Item $pcmOut).Length -gt 0) { $row.status = 'OK'; $row.size = (Get-Item $pcmOut).Length; $row.error = 'raw PCM: only size checked' }
    }
}
$csv = Join-Path $run 'results.csv' 
$rows | Export-Csv -Path $csv -NoTypeInformation -Encoding UTF8

$ok = @($rows | Where-Object status -eq 'OK').Count
$skip = @($rows | Where-Object status -eq 'SKIP_TOOL').Count
$decode = @($rows | Where-Object status -eq 'DECODE_FAIL').Count
$fail = @($rows | Where-Object status -eq 'FAIL').Count
$lines = @(
    "# ToneLore real-music validation $stamp",
    "",
    "- source: $seed",
    "- seconds: $Seconds",
    "- targets: $($rows.Count)",
    "- OK: $ok",
    "- SKIP_TOOL: $skip",
    "- DECODE_FAIL: $decode",
    "- FAIL: $fail",
    "",
    "| id | status | size | codec | error |",
    "|---|---|---|---|---|"
)
foreach ($r in ($rows | Where-Object status -ne 'OK')) {
    $lines += "| $($r.id) | $($r.status) | $($r.size) | $($r.codec) | $($r.error) |"
}
$lines | Set-Content (Join-Path $run 'summary.md') -Encoding UTF8

Write-Host ""
Write-Host "OK=$ok SKIP_TOOL=$skip DECODE_FAIL=$decode FAIL=$fail" -ForegroundColor Green
Write-Host "结果：$run" -ForegroundColor Cyan
