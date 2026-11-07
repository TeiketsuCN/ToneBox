# 从 FFmpeg 官方公开样本库拉取 ATRAC 位流（测试用途）
# 用 Invoke-WebRequest（本机 curl 无法完成 TLS 握手），每个文件最多重试 5 次
$ErrorActionPreference = 'Continue'
$base = 'https://samples.ffmpeg.org'
$dst  = 'D:\FormatConverter\samples\atrac'
New-Item -ItemType Directory -Force -Path $dst | Out-Null

$map = @(
  # --- ATRAC3 (MDLP / Network Walkman) ---
  @('A-codecs/ATRAC3/SND0.AT3',                                'SND0_atrac3plus_48k.at3'),
  @('A-codecs/ATRAC3/mc_sich_at3_066.wav',                     'mc_sich_atrac3_66k.wav'),
  @('A-codecs/ATRAC3/mc_sich_at3_105.wav',                     'mc_sich_atrac3_105k.wav'),
  @('A-codecs/ATRAC3/mc_sich_at3_132.wav',                     'mc_sich_atrac3_132k.wav'),
  @('A-codecs/ATRAC3/sample.ATRAC3.66kbps.44100Hz.Stereo.wav', 'sample_atrac3_66k.wav'),
  @('A-codecs/ATRAC3/atrac3_psp/bgm01.at3',                    'psp_bgm01.at3'),
  @('A-codecs/ATRAC3/atrac3_psp/bgm03.at3',                    'psp_bgm03.at3'),
  @('A-codecs/ATRAC3/mc_sich.txt',                             'mc_sich_readme.txt'),
  @('A-codecs/ATRAC3/atrac3_psp/at3_frompsp.txt',              'psp_readme.txt'),
  # --- ATRAC3plus: 真 OpenMG .oma 容器 + .aa3 ---
  @('A-codecs/ATRAC3%2B/at3p_sample1.oma',                     'oma_at3p_01.oma'),
  @('A-codecs/ATRAC3%2B/at3p_sample2.oma',                     'oma_at3p_02.oma'),
  @('A-codecs/ATRAC3%2B/at3p_sample3.oma',                     'oma_at3p_03.oma'),
  @('A-codecs/ATRAC3%2B/at3p_sample4.oma',                     'oma_at3p_04.oma'),
  @('A-codecs/ATRAC3%2B/at3p_sample5.oma',                     'oma_at3p_05.oma'),
  @('A-codecs/ATRAC3%2B/at3p_sample6.oma',                     'oma_at3p_06.oma'),
  @('A-codecs/ATRAC3%2B/0111019293.aa3',                       'aa3_0111019293.aa3'),
  @('A-codecs/ATRAC3%2B/sonateno14op27-2.aa3',                 'aa3_sonateno14op27-2.aa3'),
  # --- ATRAC1 (MD SP 老碟) ---
  @('A-codecs/ATRAC1/Christmas1970.aea',                       'aea_christmas1970.aea'),
  @('A-codecs/ATRAC1/1kHzSoundgroupAT1.aea',                   'aea_1khz.aea')
)

foreach ($pair in $map) {
  $remote = $pair[0]
  $out    = Join-Path $dst $pair[1]
  if ((Test-Path $out) -and (Get-Item $out).Length -gt 2000) {
    Write-Output ("SKIP  {0}  {1} B" -f $pair[1], (Get-Item $out).Length); continue
  }
  for ($try = 1; $try -le 5; $try++) {
    try {
      Invoke-WebRequest -Uri "$base/$remote" -OutFile $out -UseBasicParsing -TimeoutSec 240
      Write-Output ("OK    {0}  {1} B  (try {2})" -f $pair[1], (Get-Item $out).Length, $try)
      break
    } catch {
      Write-Output ("FAIL  {0}  try {1}: {2}" -f $pair[1], $try, $_.Exception.Message)
      Start-Sleep -Seconds 2
    }
  }
}
Write-Output '---- DONE ----'
