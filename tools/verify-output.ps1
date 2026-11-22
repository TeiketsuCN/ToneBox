# 输出验证的公共逻辑，被 validate-real-music.ps1 / validate-library.ps1 点源加载。
#
# 为什么单独写：ffmpeg 读不了 .xa / .dsp / .idsp / VADPCM 的 .aifc / .ofr，
# 这些目标必须用生成它们的工具回读，否则验证脚本会把「转换成功」误报成失败。
#
# 返回：Status / Verify / Codec / Duration / Error
#   Status = OK / DECODE_FAIL / TOOL_DECODE_FAIL / STRUCTURE_FAIL / UNVERIFIED
#   Verify = ffmpeg / vgaudio / vadpcm / ofr / structure / size

function Get-AudioProps([string]$Ffprobe, [string]$Path) {
    # 这里故意用 & 而不是 .NET Process：ffprobe 用重定向管道启动时会卡住
    # （CreateNoWindow + 重定向在 PS 5.1 下读 stdin 不返回）。
    # 同时临时把 ErrorActionPreference 调回 Continue：脚本里是 Stop，
    # ffprobe 往 stderr 写一行错误会被当成终止错误，回退分支就走不到了。
    $old = $ErrorActionPreference
    $ErrorActionPreference = 'Continue'
    try {
        $json = & $Ffprobe -v error -print_format json -show_format -show_streams $Path 2>$null
        if ($LASTEXITCODE -ne 0 -or -not $json) { return $null }
    } finally {
        $ErrorActionPreference = $old
    }
    try {
        $j = ($json | Out-String) | ConvertFrom-Json
    } catch {
        return $null
    }
    $s = $j.streams | Where-Object { $_.codec_type -eq 'audio' } | Select-Object -First 1
    if (-not $j.format -or -not $j.format.duration) { $dur = 0.0 } else { $dur = [double]$j.format.duration }
    [pscustomobject]@{
        sr    = if ($s) { [int]$s.sample_rate } else { 0 }
        ch    = if ($s) { [int]$s.channels } else { 0 }
        bits  = if ($s -and $s.bits_per_raw_sample -and [int]$s.bits_per_raw_sample -gt 0) { [string][int]$s.bits_per_raw_sample } elseif ($s) { [string]$s.sample_fmt } else { '' }
        codec = if ($s) { [string]$s.codec_name } else { '' }
        dur   = $dur
    }
}

function Quote-Arg([string]$s) {
    if ($s -match '[\s"]') { return '"' + ($s -replace '"', '\"') + '"' }
    return $s
}

# 用 .NET Process 而不是 Start-Process：Start-Process -PassThru 拿不到 ExitCode
# （$p.ExitCode 返回 $null，导致所有成功都被当成失败）。
function Invoke-Process {
    param([string]$Exe, [string[]]$ArgList, [int]$TimeoutSec = 120)
    $psi = New-Object System.Diagnostics.ProcessStartInfo
    $psi.FileName = $Exe
    $psi.Arguments = (($ArgList | ForEach-Object { Quote-Arg $_ }) -join ' ')
    $psi.UseShellExecute = $false
    $psi.CreateNoWindow = $true
    $psi.RedirectStandardOutput = $true
    $psi.RedirectStandardError = $true
    # 必须重定向并立刻关掉 stdin：后台任务里父进程的 stdin 是没关的管道，
    # ffmpeg/ffprobe 会去读它并卡死（表现是 WaitForExit 超时、没有任何输出）。
    $psi.RedirectStandardInput = $true
    $p = [System.Diagnostics.Process]::Start($psi)
    try { $p.StandardInput.Close() } catch { }
    $timedOut = $false
    if (-not $p.WaitForExit($TimeoutSec * 1000)) {
        $timedOut = $true
        try { $p.Kill() } catch { }
        $p.WaitForExit()
    }
    $out = $p.StandardOutput.ReadToEnd()
    $err = $p.StandardError.ReadToEnd()
    [pscustomobject]@{
        Code     = if ($timedOut) { -999 } else { $p.ExitCode }
        Out      = $out
        Err      = $err
        TimedOut = $timedOut
    }
}

function Invoke-Capture([string]$Exe, [string[]]$ArgList, [int]$TimeoutSec) {
    $res = Invoke-Process -Exe $Exe -ArgList $ArgList -TimeoutSec $TimeoutSec
    [pscustomobject]@{ Code = $res.Code; Text = (($res.Out + ' ' + $res.Err) -replace '\s+', ' ').Trim() }
}

function Test-OutputFile {
    param(
        [string]$Id,
        [string]$Path,
        [string]$Ffmpeg,
        [string]$Ffprobe,
        [string]$VgAudio,
        [string]$Vadpcm,
        [string]$Ofr,
        [int]$TimeoutSec = 120
    )
    $r = [pscustomobject]@{ Status = 'OK'; Verify = 'ffmpeg'; Codec = ''; Duration = ''; Error = '' }
    if (-not (Test-Path $Path)) { $r.Status = 'FAIL'; $r.Error = '文件不存在'; return $r }
    $size = (Get-Item $Path).Length
    if ($size -eq 0) { $r.Status = 'FAIL'; $r.Error = '输出 0 字节'; return $r }

    # 裸 PCM 没有头，ffprobe 也没法读：只查大小
    if ($Id -eq 'pcm') { $r.Verify = 'size'; $r.Error = 'raw PCM：只查大小'; return $r }

    # PS1 XA：ffmpeg 没有 XA 解复用器，按扇区结构查（2336 字节/扇区）
    if ($Id -eq 'xa') {
        if (($size % 2336) -eq 0) {
            $r.Verify = 'structure'
            $r.Error = "XA 结构检查通过（2336 字节/扇区 × $($size / 2336)）"
        } else {
            $r.Status = 'STRUCTURE_FAIL'
            $r.Error = "XA 大小 $size 不是 2336 的整数倍"
        }
        return $r
    }

    $pr = Get-AudioProps $Ffprobe $Path
    if ($pr) {
        $r.Codec = $pr.codec
        $r.Duration = $pr.dur
        $old = $ErrorActionPreference
        $ErrorActionPreference = 'Continue'
        try {
            $null = & $Ffmpeg -v error -nostdin -t 5 -i $Path -f null - 2>$null
            $decCode = $LASTEXITCODE
        } finally {
            $ErrorActionPreference = $old
        }
        if ($decCode -ne 0) { $r.Status = 'DECODE_FAIL'; $r.Error = 'ffmpeg 解码失败' }
        return $r
    }

    # ffmpeg 读不了：用生成它的工具回读成 WAV 再验
    $tmp = Join-Path $env:TEMP ("tl_verify_" + [guid]::NewGuid().ToString('N') + '.wav')
    try {
        switch ($Id) {
            { $_ -in 'dsp', 'idsp', 'brstm', 'bcstm', 'bfstm' } {
                if (-not $VgAudio -or -not (Test-Path $VgAudio)) {
                    $r.Status = 'UNVERIFIED'; $r.Error = 'ffmpeg 读不了该容器，也没找到 VGAudioCli'; return $r
                }
                $res = Invoke-Capture $VgAudio @('-i', $Path, '-o', $tmp) $TimeoutSec
                if ($res.Code -ne 0 -or -not (Test-Path $tmp)) {
                    $r.Status = 'TOOL_DECODE_FAIL'
                    $r.Error = "VGAudioCli 回读失败：" + (($res.Text -replace '\s+', ' ').Trim())
                    return $r
                }
                $r.Verify = 'vgaudio'
            }
            'vadpcm' {
                if (-not $Vadpcm -or -not (Test-Path $Vadpcm)) {
                    $r.Status = 'UNVERIFIED'; $r.Error = 'ffmpeg 读不了 VADPCM，也没找到 vadpcm.exe'; return $r
                }
                $res = Invoke-Capture $Vadpcm @('decode', $Path, $tmp) $TimeoutSec
                if ($res.Code -ne 0 -or -not (Test-Path $tmp)) {
                    $r.Status = 'TOOL_DECODE_FAIL'
                    $r.Error = "vadpcm 回读失败：" + (($res.Text -replace '\s+', ' ').Trim())
                    return $r
                }
                $r.Verify = 'vadpcm'
            }
            'optimfrog' {
                if (-not $Ofr -or -not (Test-Path $Ofr)) {
                    $r.Status = 'UNVERIFIED'; $r.Error = 'ffmpeg 读不了 OFR，也没找到 ofr.exe'; return $r
                }
                $res = Invoke-Capture $Ofr @('--verify', $Path) $TimeoutSec
                if ($res.Code -ne 0) {
                    $r.Status = 'TOOL_DECODE_FAIL'
                    $r.Error = "ofr --verify 失败：" + (($res.Text -replace '\s+', ' ').Trim())
                    return $r
                }
                $r.Verify = 'ofr'
                return $r
            }
            default {
                $r.Status = 'UNVERIFIED'
                $r.Error = 'ffmpeg 读不了该格式，也没有可用的回读工具'
                return $r
            }
        }
        $pr2 = Get-AudioProps $Ffprobe $tmp
        if (-not $pr2) { $r.Status = 'UNVERIFIED'; $r.Error = '回读出的 WAV 也读不了'; return $r }
        $r.Codec = $pr2.codec
        $r.Duration = $pr2.dur
        return $r
    } finally {
        Remove-Item $tmp -Force -ErrorAction SilentlyContinue
    }
}
