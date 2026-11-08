//! fc-analyze —— 频谱质量评估。
//!
//! 实现照 `tools/analyze.py`：
//!   1. ffmpeg 解码成单声道 f32le，保持原始采样率
//!   2. 4096 点 Hann 窗、50% 重叠，跨帧平均功率谱
//!   3. 以 1~4kHz 中频段为参考电平归一化到 dB
//!   4. ±300Hz 窗口内最陡负斜率作为低通边沿
//!   5. 按编码族查表，并给出置信度和误判说明
//!
//! 阈值表按 `docs/样本清单.md` 的实测数据重排。这里只输出推测，
//! 不把“截止频率 → 码率”当成确定结论；母带带宽受限时给区间而不是硬猜。

use std::cmp::Ordering;
use std::path::{Path, PathBuf};
use std::process::Command;

pub mod health;

use anyhow::{anyhow, bail, Context, Result};
use fc_core::{locate, MediaInfo};
use rustfft::num_complex::Complex;
use rustfft::FftPlanner;

const NFFT: usize = 4096;
const HOP: usize = 2048;
const MAX_FRAMES: usize = 4000;
const REF_BAND_LO: f64 = 1000.0;
const REF_BAND_HI: f64 = 4000.0;
const SLOPE_WIN_HZ: f64 = 300.0;
const SLOPE_MIN: f64 = 40.0;
const KNEE_MIN_HZ: f64 = 4000.0;

/// 超过这个时长就不再整段解码。10 分钟的整段 f32 单声道约 106MB，
/// 30 分钟约 317MB；频谱特征只需要几十秒。
const FULL_DECODE_LIMIT_S: f64 = 600.0;
/// 长文件取三个 30 秒窗口，共 90 秒，避免只看开头。
const SAMPLE_WINDOW_S: f64 = 30.0;
const SAMPLE_POINTS: [f64; 3] = [0.1, 0.5, 0.9];

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

#[derive(Debug, Clone)]
pub struct QualityReport {
    /// 频谱低通边沿（Hz）。未找到陡峭边沿时等于 Nyquist。
    pub cutoff_hz: f64,
    /// 边沿处的陡度（dB/kHz，负值）
    pub slope_db_per_khz: f64,
    /// 推测的源码率档位描述，如 "128"、"无损/CD"
    pub estimate: String,
    /// 0.0 ~ 1.0
    pub confidence: f64,
    /// 置信度是怎么算出来的：每条带"加分/扣分"和理由，界面按顺序列出来
    pub confidence_notes: Vec<ConfidenceNote>,
    /// 误判风险说明，永远不为空才好
    pub caveats: Vec<String>,
    /// Some = 这是 DSD：频谱不参与判档，界面上换一张"格式判定"卡
    pub dsd: Option<DsdReport>,
}

/// 置信度的一条依据：`delta` 是这一条对总分的加减（百分比 + 是否加分），
/// `None` 表示"结论行"（比如最终值），不显示正负号。
#[derive(Debug, Clone)]
pub struct ConfidenceNote {
    pub delta: Option<(f32, bool)>,
    pub reason: String,
}

impl ConfidenceNote {
    pub fn plus(pct: f32, reason: impl Into<String>) -> Self {
        Self {
            delta: Some((pct.abs(), true)),
            reason: reason.into(),
        }
    }
    pub fn minus(pct: f32, reason: impl Into<String>) -> Self {
        Self {
            delta: Some((pct.abs(), false)),
            reason: reason.into(),
        }
    }
    pub fn plain(reason: impl Into<String>) -> Self {
        Self {
            delta: None,
            reason: reason.into(),
        }
    }
}

/// DSD 的结论：只有格式事实，没有"推测码率"
#[derive(Debug, Clone)]
pub struct DsdReport {
    /// 64 / 128 / 256 / 512 / 1024，0 = 认不出来
    pub tier: u32,
    /// 真正的 1-bit 采样率（Hz）
    pub bit_rate_hz: f64,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
enum Family {
    Mp3,
    Aac,
    Atrac3,
    Atrac3p,
    Atrac1,
    Opus,
    Vorbis,
    Generic,
}

impl Family {
    fn of(codec: &str) -> Self {
        let c = codec.to_ascii_lowercase();
        if c.starts_with("atrac3plus") || c == "atrac3p" {
            Family::Atrac3p
        } else if c.starts_with("atrac3") {
            Family::Atrac3
        } else if c.starts_with("atrac1") {
            Family::Atrac1
        } else if c.contains("mp3") {
            Family::Mp3
        } else if c == "aac" || c == "aac_latm" {
            Family::Aac
        } else if c == "opus" {
            Family::Opus
        } else if c.contains("vorbis") {
            Family::Vorbis
        } else {
            Family::Generic
        }
    }
}

struct Guess {
    estimate: &'static str,
    /// 乘到边沿陡度置信度上的系数。区间/重叠档位要压低。
    confidence_scale: f64,
    caveat: Option<&'static str>,
}

impl Guess {
    fn exact(estimate: &'static str) -> Self {
        Self {
            estimate,
            confidence_scale: 1.0,
            caveat: None,
        }
    }

    fn unsure(estimate: &'static str, scale: f64, caveat: &'static str) -> Self {
        Self {
            estimate,
            confidence_scale: scale,
            caveat: Some(caveat),
        }
    }
}

/// 分析一个文件的频谱质量。
pub fn analyze(path: &Path, info: &MediaInfo) -> Result<QualityReport> {
    if info.sample_rate == 0 {
        bail!("采样率未知，无法做频谱分析");
    }
    // DSD 是 1-bit 脉冲密度编码，没有 MP3/AAC 那种"码率档位"，截止频率法在这里不适用。
    // 直接按格式下结论，也别为了这张卡去解 90 秒的 PCM 再套 PCM 的阈值表。
    if info.codec.starts_with("dsd") {
        return Ok(QualityReport {
            cutoff_hz: 0.0,
            slope_db_per_khz: 0.0,
            estimate: "无损（DSD 编码）".to_string(),
            confidence: 1.0,
            confidence_notes: vec![
                ConfidenceNote::plus(100.0, "格式本身就是 DSD，不需要靠频谱猜档位"),
            ],
            caveats: vec![
                "DSD 是 1-bit 脉冲密度编码，没有按码率区分的档位；截止频率法在 DSD 上不适用，所以这里不做有损推测。"
                    .to_string(),
                "能确认的只有文件确实是 DSD。它是原生 DSD 录音，还是从 PCM 转制来的，本工具判断不了（要看发行信息或母带来源）。"
                    .to_string(),
            ],
            dsd: Some(DsdReport {
                tier: fc_core::dsd_tier(&info.codec, info.sample_rate, info.bitrate_kbps),
                bit_rate_hz: fc_core::dsd_bit_rate_hz(info.sample_rate),
            }),
        });
    }
    let tools = locate().ok_or_else(|| {
        anyhow!("找不到 ffmpeg / ffprobe。请安装 FFmpeg，或把 ffmpeg.exe、ffprobe.exe 放到 PATH / C:\\ffmpeg\\bin。")
    })?;

    let sr = info.sample_rate as f64;
    let samples = decode_mono(&tools.ffmpeg, path, info.sample_rate, info.duration_s)?;
    let (freqs, db) = mean_spectrum(&samples, sr);
    let sm = smooth(&db, 9);
    let ref_db = median(&sm, &freqs, REF_BAND_LO, REF_BAND_HI);

    let nyquist = sr / 2.0;
    let (edge, slope) = find_edge(&freqs, &sm, nyquist);
    let cutoff = edge.unwrap_or(nyquist);

    let family = Family::of(&info.codec);
    let lossless = is_lossless_codec(&info.codec);
    let he_aac = family == Family::Aac && is_he_aac(&tools.ffprobe, path);
    let mut caveats: Vec<String> = Vec::new();
    let mut notes: Vec<ConfidenceNote> = Vec::new();

    let (estimate, mut base_conf) = if lossless {
        caveats.push(
            "编码本身是无损格式；频谱只能说明文件没有被再次有损压缩，不能证明母带质量。"
                .to_string(),
        );
        // 无损不是"猜"的，置信度由证据加分，界面把它一条条列出来
        let mut c = 0.80f64;
        notes.push(ConfidenceNote::plus(
            80.0,
            format!("编码标记为无损（{}）", info.codec),
        ));
        let nyquist = info.sample_rate as f64 / 2.0;
        if cutoff >= nyquist * 0.96 {
            c += 0.10;
            notes.push(ConfidenceNote::plus(
                10.0,
                format!(
                    "频谱一直延伸到 {:.1} kHz（Nyquist {:.1} kHz）",
                    cutoff / 1000.0,
                    nyquist / 1000.0
                ),
            ));
        } else {
            notes.push(ConfidenceNote::minus(
                10.0,
                format!(
                    "频谱只到 {:.1} kHz，离 Nyquist {:.1} kHz 还有距离",
                    cutoff / 1000.0,
                    nyquist / 1000.0
                ),
            ));
        }
        if slope.abs() >= 30.0 {
            c += 0.05;
            notes.push(ConfidenceNote::plus(
                5.0,
                format!("高频滚降 {:.0} dB/kHz 够陡", slope.abs()),
            ));
        }
        ("无损/CD".to_string(), c.min(0.97))
    } else if family == Family::Opus {
        caveats.push(
            "Opus 在低码率也保留全带宽（噪声整形/带宽扩展），频谱截止无法反推码率，需换判据（谱平坦度/噪声底电平）。"
                .to_string(),
        );
        ("不可判定".to_string(), 0.10)
    } else if he_aac {
        caveats.push(
            "HE-AAC / xHE-AAC 使用 SBR/PS 频带复制，低码率仍保留高频；截止法对它和 Opus 一样失效。"
                .to_string(),
        );
        notes.push(ConfidenceNote::minus(
            88.0,
            "SBR/PS 频带复制让截止法失效，只能给很低的分",
        ));
        ("不可判定".to_string(), 0.12)
    } else if edge.is_none() {
        let (est, conf, why) = no_edge_guess(family);
        caveats.push(why.to_string());
        notes.push(ConfidenceNote::minus(50.0, "没找到陡峭边沿，用格式先验给一个低分"));
        (est.to_string(), conf)
    } else {
        let guess = classify(family, cutoff);
        let sigmoid = 1.0 / (1.0 + (-(slope.abs() - 45.0) / 8.0).exp());
        let mut conf = sigmoid * guess.confidence_scale;
        notes.push(ConfidenceNote::plus(
            (sigmoid * 100.0) as f32,
            format!("边沿陡度 {:.0} dB/kHz，基础分", slope.abs()),
        ));
        if guess.confidence_scale < 0.999 {
            notes.push(ConfidenceNote::minus(
                ((1.0 - guess.confidence_scale) * 100.0) as f32,
                format!(
                    "档位 {} 属于区间/重叠档，打 {:.2} 折",
                    guess.estimate, guess.confidence_scale
                ),
            ));
        } else {
            notes.push(ConfidenceNote::plain(format!(
                "档位 {} 唯一，不打折",
                guess.estimate
            )));
        }
        if let Some(note) = guess.caveat {
            caveats.push(note.to_string());
        }

        // 边沿上方残留能量高 → 噪底填充，推测可能偏高。
        let above: Vec<f64> = freqs
            .iter()
            .zip(sm.iter())
            .filter(|(f, _)| **f > cutoff + 1000.0)
            .map(|(_, v)| *v)
            .collect();
        if !above.is_empty() && median_of_slice(&above) - ref_db > -55.0 {
            caveats.push(
                "边沿上方仍有明显能量，疑似噪底填充（黑胶/模拟噪声），推测可能偏高。".to_string(),
            );
            conf *= 0.75;
            notes.push(ConfidenceNote::minus(25.0, "边沿上方仍有明显能量，疑似噪底填充"));
        }

        // 边沿下方能量偏低 → 音源本身高频就少。
        let below: Vec<f64> = freqs
            .iter()
            .zip(sm.iter())
            .filter(|(f, _)| **f > cutoff - 2000.0 && **f < cutoff - 400.0)
            .map(|(_, v)| *v)
            .collect();
        if !below.is_empty() && mean_of_slice(&below) - ref_db < -25.0 {
            caveats.push(
                "边沿下方能量已偏低，音源本身高频可能就少（电子乐/老录音），推测可能偏低。"
                    .to_string(),
            );
            conf *= 0.8;
            notes.push(ConfidenceNote::minus(20.0, "边沿下方能量偏低，音源本身高频就少"));
        }

        (guess.estimate.to_string(), conf)
    };

    // ATRAC 的阈值表只有 16 个文件、10 个档位，其中 6 档只有 1 个样本，也没有留出集。
    // 所以结论只当参考：压低置信度，并在界面上明说——别让它看起来和 MP3 一样确定。
    if matches!(family, Family::Atrac3 | Family::Atrac3p) {
        caveats.push(
            "ATRAC 阈值表只有 16 个样本、10 个档位（其中 6 档只有 1 个样本），也没有留出集验证；这一点只能当参考，不要当结论。"
                .to_string(),
        );
        base_conf = base_conf.min(0.60);
        notes.push(ConfidenceNote::minus(40.0, "ATRAC 阈值表样本太少，封顶 60%"));
    }

    let confidence = (round2(base_conf.clamp(0.05, 0.97))).clamp(0.05, 0.97);
    notes.push(ConfidenceNote::plain(format!(
        "最终 {:.0}%（上限 97%，留一点给未知因素）",
        confidence * 100.0
    )));
    Ok(QualityReport {
        cutoff_hz: cutoff,
        slope_db_per_khz: slope,
        estimate,
        confidence,
        confidence_notes: notes,
        caveats,
        dsd: None,
    })
}

fn is_lossless_codec(codec: &str) -> bool {
    let c = codec.to_ascii_lowercase();
    c.starts_with("pcm_")
        || matches!(
            c.as_str(),
            "flac"
                | "alac"
                | "wavpack"
                | "ape"
                | "tta"
                | "truehd"
                | "mlp"
                | "shorten"
                // ATRAC Advanced Lossless：解出来是无损的，别按 ATRAC 码率表去猜
                | "atrac3al"
                | "atrac3plusal"
        )
}

fn no_edge_guess(family: Family) -> (&'static str, f64, &'static str) {
    match family {
        Family::Aac => (
            "256",
            0.55,
            "未找到陡峭边沿。AAC 在高码率会把带宽保留到 Nyquist，按 256k 档处理；无法排除无损或母带带宽受限。",
        ),
        Family::Mp3 => (
            "256~320",
            0.45,
            "未找到陡峭边沿。MP3 高码率或母带带宽受限时都可能是这样，只能给区间。",
        ),
        Family::Atrac3p => (
            "256~352",
            0.40,
            "未找到陡峭边沿。ATRAC3+ 高码率与母带带宽受限无法仅凭截止频率区分。",
        ),
        Family::Atrac3 => (
            "132~292",
            0.35,
            "未找到陡峭边沿。ATRAC3 高码率与母带带宽受限无法仅凭截止频率区分。",
        ),
        Family::Atrac1 => (
            "292",
            0.40,
            "未找到陡峭边沿。ATRAC1 只有 292k 一档，按该档处理。",
        ),
        Family::Vorbis => (
            "320",
            0.45,
            "未找到陡峭边沿。Vorbis 高码率或母带带宽受限时都可能是这样。",
        ),
        Family::Opus => ("不可判定", 0.10, "截止法对 Opus 无效。"),
        Family::Generic => (
            "无损/CD",
            0.35,
            "未找到陡峭边沿：可能本来是无损，或高频被母带/话筒带宽限制。",
        ),
    }
}

fn classify(family: Family, cutoff: f64) -> Guess {
    // 表按 44.1/48kHz 实测数据标定，单位 Hz。
    // 同一档位在不同母带上可能重叠，所以允许区间。
    match family {
        Family::Mp3 => {
            if cutoff >= 20000.0 {
                Guess::exact("320")
            } else if cutoff >= 19500.0 {
                Guess::unsure("256", 0.70, "256k MP3 样本不足，该档位边界是插值估计。")
            } else if cutoff >= 17500.0 {
                Guess::exact("192")
            } else if cutoff >= 16000.0 {
                // 源带宽受限时，128/192/320 的截止频率在 16~17.5kHz 撞在一起。
                // 不再用边沿陡度硬猜单档：留出集验证显示这个规则只被 pop48k 一张母版支撑。
                Guess::unsure(
                    "128/192/320",
                    0.35,
                    "截止频率落在 128/192/320 的重叠区；母带带宽受限时无法单值区分，只能给区间。",
                )
            } else if cutoff >= 15800.0 {
                Guess::exact("128")
            } else if cutoff >= 12500.0 {
                Guess::exact("96")
            } else {
                Guess::exact("64")
            }
        }
        Family::Aac => {
            if cutoff >= 20500.0 {
                Guess::exact("256")
            } else if cutoff >= 18500.0 {
                Guess::exact("192")
            } else if cutoff >= 14000.0 {
                Guess::unsure(
                    "128~256",
                    0.50,
                    "128k 与 256k 在母带带宽受限时截止频率重叠，只能给出区间。",
                )
            } else {
                Guess::exact("64")
            }
        }
        Family::Atrac3 => {
            if cutoff >= 18000.0 {
                // 132k 的实测边沿只有 17.7kHz，18kHz 以上没有样本可依
                Guess::unsure(
                    "132",
                    0.55,
                    "ATRAC3 132k 实测边沿 17.7kHz，18kHz 以上没有样本，按最高档外推。",
                )
            } else if cutoff >= 16000.0 {
                Guess::unsure(
                    "66~132",
                    0.50,
                    "ATRAC3 66k 与 132k 的截止频率重叠（同族样本差异大于档位差异），只能给区间。",
                )
            } else if cutoff >= 14500.0 {
                Guess::unsure(
                    "66~105",
                    0.50,
                    "ATRAC3 66k 与 105k 的截止频率重叠（同族样本差异大于档位差异），只能给区间。",
                )
            } else {
                Guess::exact("66")
            }
        }
        Family::Atrac3p => {
            if cutoff >= 19300.0 {
                // 6 个 256k 样本全部来自同一母版，只能算"一个证据"
                Guess::unsure(
                    "256",
                    0.55,
                    "ATRAC3+ 256k 的边沿只由一个母版的 6 个样本标定，没有第二个来源验证。",
                )
            } else if cutoff >= 18500.0 {
                // 48k 那个样本是钢琴曲（高频本来就少），边沿 19.0kHz，和 256k 撞在一起。
                // 以前在这里直接给 "48"，等于把异常值当规律。
                Guess::unsure(
                    "48/256",
                    0.25,
                    "48k 的样本是钢琴曲（高频本来就少），实测边沿 19.0kHz，与 256k 重叠；截止法分不开这两档。",
                )
            } else if cutoff >= 16000.0 {
                // 128k 实测在 15.3kHz，这一整段没有真实样本
                Guess::unsure(
                    "128~256",
                    0.30,
                    "16~18.5kHz 这一段没有真实 ATRAC3+ 样本，128k 与 256k 之间属于外推。",
                )
            } else if cutoff >= 14500.0 {
                Guess::unsure(
                    "65~128",
                    0.50,
                    "ATRAC3+ 65k 与 128k 的截止频率几乎重合，只能给区间。",
                )
            } else {
                Guess::exact("49")
            }
        }
        Family::Atrac1 => Guess::exact("292"),
        Family::Opus => Guess::unsure("不可判定", 0.10, "截止法对 Opus 无效。"),
        Family::Vorbis => {
            if cutoff >= 19500.0 {
                Guess::exact("320")
            } else if cutoff >= 17500.0 {
                Guess::exact("192")
            } else if cutoff >= 15000.0 {
                Guess::exact("128")
            } else if cutoff >= 12000.0 {
                Guess::exact("96")
            } else {
                Guess::exact("64")
            }
        }
        Family::Generic => {
            if cutoff >= 19500.0 {
                Guess::exact("320")
            } else if cutoff >= 17000.0 {
                Guess::exact("192")
            } else if cutoff >= 15000.0 {
                Guess::exact("128")
            } else if cutoff >= 12500.0 {
                Guess::exact("96")
            } else if cutoff >= 10000.0 {
                Guess::exact("64")
            } else {
                Guess::exact("32~48")
            }
        }
    }
}

fn profile_is_he_aac(profile: &str) -> bool {
    let p = profile.to_ascii_uppercase();
    p.contains("HE-AAC") || p.contains("HE_AAC") || p.contains("XHE-AAC") || p.contains("XHE_AAC")
}

/// 从 ffprobe 的 stream profile 里识别 HE-AAC / xHE-AAC。
/// `MediaInfo` 没有 profile 字段（公开 API 冻结），这里单独问一次 ffprobe。
fn is_he_aac(ffprobe: &Path, path: &Path) -> bool {
    let mut cmd = Command::new(ffprobe);
    cmd.args([
        "-v",
        "error",
        "-select_streams",
        "a:0",
        "-show_entries",
        "stream=profile",
        "-of",
        "default=noprint_wrappers=1:nokey=1",
    ])
    .arg(path);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }

    let out = match cmd.output() {
        Ok(out) if out.status.success() => out,
        _ => return false,
    };
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .any(profile_is_he_aac)
}

fn sample_windows(duration_s: f64) -> Vec<(f64, f64)> {
    if duration_s <= FULL_DECODE_LIMIT_S {
        return Vec::new();
    }
    let span = (duration_s - SAMPLE_WINDOW_S).max(0.0);
    SAMPLE_POINTS
        .iter()
        .map(|fraction| (span * fraction, SAMPLE_WINDOW_S))
        .collect()
}

fn decode_mono(
    ffmpeg: &PathBuf,
    path: &Path,
    sample_rate: u32,
    duration_s: f64,
) -> Result<Vec<f32>> {
    let windows = sample_windows(duration_s);
    if windows.is_empty() {
        return decode_segment(ffmpeg, path, sample_rate, None, None);
    }

    let mut samples = Vec::new();
    for (start, len) in windows {
        let part = decode_segment(ffmpeg, path, sample_rate, Some(start), Some(len))?;
        samples.extend_from_slice(&part);
    }
    if samples.len() < NFFT {
        bail!("样本太短，无法分析（少于 4096 个采样点）");
    }
    Ok(samples)
}

fn decode_segment(
    ffmpeg: &PathBuf,
    path: &Path,
    sample_rate: u32,
    start_s: Option<f64>,
    len_s: Option<f64>,
) -> Result<Vec<f32>> {
    let mut cmd = Command::new(ffmpeg);
    cmd.args(["-v", "error", "-nostdin"]);
    if let Some(start) = start_s {
        cmd.args(["-ss", &format!("{start:.3}")]);
    }
    cmd.arg("-i").arg(path);
    if let Some(len) = len_s {
        cmd.args(["-t", &format!("{len:.3}")]);
    }
    cmd.args(["-map", "0:a:0", "-ac", "1", "-ar"])
        .arg(sample_rate.to_string())
        .args(["-f", "f32le", "-"]);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }

    let out = cmd.output().context("启动 ffmpeg 解码失败")?;
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr);
        let tail: String = err
            .chars()
            .rev()
            .take(400)
            .collect::<String>()
            .chars()
            .rev()
            .collect();
        bail!("ffmpeg 解码失败：\n{}", tail.trim());
    }
    if out.stdout.len() < NFFT * 4 {
        bail!("样本太短，无法分析（少于 4096 个采样点）");
    }
    Ok(out
        .stdout
        .chunks_exact(4)
        .map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
        .collect())
}


/// 测整合响度（EBU R128）。整轨解码到 48 kHz 立体声，再用 fc-core 的测量实现算。
///
/// 这是唯一一条"把整首歌解一遍"的路径：几分钟的歌大概 1~3 秒，别在 UI 线程上调。
pub fn loudness(ffmpeg: &Path, path: &Path) -> Result<fc_core::loudness::Loudness> {
    let mut cmd = Command::new(ffmpeg);
    cmd.args(["-v", "error", "-nostdin", "-i"]).arg(path);
    cmd.args(["-map", "0:a:0", "-ac", "2", "-ar", "48000", "-f", "f32le", "-"]);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    let out = cmd.output().context("启动 ffmpeg 解码失败（响度）")?;
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr);
        let tail: String = err.chars().rev().take(400).collect::<String>().chars().rev().collect();
        bail!("ffmpeg 解码失败（响度）：\n{}", tail.trim());
    }
    let pcm: Vec<f32> = out
        .stdout
        .chunks_exact(4)
        .map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
        .collect();
    fc_core::loudness::measure(&pcm, 2).context("样本太短，测不出响度（至少 400ms）")
}

/// 播放栏用的波形：整轨解成 8 kHz 立体声，按桶取每段的最小/最大峰值。
///
/// 返回的每个声道一个 Vec（通常是 2 个：左/右），每项是 `(min, max)`。
/// 8 kHz 足够画包络；一首 4 分钟的歌解码不到 0.3 秒。
pub fn waveform(ffmpeg: &Path, path: &Path, buckets: usize) -> Result<Vec<Vec<(f32, f32)>>> {
    const CH: usize = 2;
    const RATE: u32 = 8_000;

    let mut cmd = Command::new(ffmpeg);
    cmd.args(["-v", "error", "-nostdin", "-i"]).arg(path);
    cmd.args(["-map", "0:a:0", "-ac", "2", "-ar"])
        .arg(RATE.to_string())
        .args(["-f", "f32le", "-"]);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    let out = cmd.output().context("启动 ffmpeg 解码失败（波形）")?;
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr);
        let tail: String = err
            .chars()
            .rev()
            .take(300)
            .collect::<String>()
            .chars()
            .rev()
            .collect();
        bail!("ffmpeg 解码失败（波形）：\n{}", tail.trim());
    }
    let pcm: Vec<f32> = out
        .stdout
        .chunks_exact(4)
        .map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
        .collect();
    let frames = pcm.len() / CH;
    if frames < 64 {
        bail!("样本太短，画不出波形");
    }
    let buckets = buckets.clamp(64, 4000);
    let mut chans: Vec<Vec<(f32, f32)>> = Vec::with_capacity(CH);
    for c in 0..CH {
        let mut v = Vec::with_capacity(buckets);
        for i in 0..buckets {
            let a = frames * i / buckets;
            let b = (frames * (i + 1) / buckets).max(a + 1).min(frames);
            let mut mn = 0.0f32;
            let mut mx = 0.0f32;
            for f in a..b {
                let x = pcm[f * CH + c];
                if x < mn {
                    mn = x;
                }
                if x > mx {
                    mx = x;
                }
            }
            v.push((mn, mx));
        }
        chans.push(v);
    }
    // 两个声道几乎一样（单声道源）时，右边那条没意义，省一条线
    if chans.len() == 2 {
        let same = chans[0]
            .iter()
            .zip(chans[1].iter())
            .all(|(a, b)| (a.0 - b.0).abs() < 1e-3 && (a.1 - b.1).abs() < 1e-3);
        if same {
            chans.truncate(1);
        }
    }
    Ok(chans)
}

/// 给界面画频谱用：对数频率轴上的 (Hz, dB) 点。
///
/// 跟 `analyze` 用同一条解码 + FFT 路径，只是不判质量：按对数频率分带，
/// 每个带取带内平均 dB（比"最近邻 bin"平整得多，不会出现锯齿和台阶）。
pub fn spectrum(
    ffmpeg: &Path,
    path: &Path,
    sample_rate: u32,
    bins: usize,
) -> Result<Vec<(f64, f64)>> {
    let sr = sample_rate.clamp(8_000, 192_000);
    // 只解 8 秒就够画曲线，别为了一张图把整首歌解一遍
    let samples = decode_segment(&ffmpeg.to_path_buf(), path, sr, None, Some(8.0))?;
    let (freqs, db) = mean_spectrum(&samples, sr as f64);
    let bins = bins.clamp(16, 2048);
    let f_lo = 20.0f64;
    let f_hi = (sr as f64 / 2.0).min(22_050.0);
    let ratio = f_hi / f_lo;
    let mut out: Vec<(f64, f64)> = Vec::with_capacity(bins);
    for i in 0..bins {
        let t = i as f64 / (bins - 1) as f64;
        let f = f_lo * ratio.powf(t);
        let f_next = f_lo * ratio.powf((i + 1) as f64 / (bins - 1) as f64);
        // 这个对数带覆盖的线性 bin 区间（至少一个 bin）
        let k0 = ((f / (sr as f64) * NFFT as f64).floor() as usize).max(1);
        let k1 = (((f_next / (sr as f64) * NFFT as f64).ceil() as usize).max(k0 + 1))
            .min(freqs.len() - 1);
        let slice = &db[k0..=k1.min(freqs.len() - 1)];
        // 带内平均（dB 平均），再夹到 -110..0
        let v = (slice.iter().sum::<f64>() / slice.len() as f64).clamp(-110.0, 0.0);
        out.push((f, v));
    }
    // 三点平滑，抹掉带边界的小台阶
    let mut smooth = out.clone();
    for i in 1..out.len().saturating_sub(1) {
        smooth[i].1 = (out[i - 1].1 + out[i].1 * 2.0 + out[i + 1].1) / 4.0;
    }
    // 峰值归一到 0 dB，图上看起来才有意义（绝对电平跟增益有关）
    let peak = smooth.iter().map(|p| p.1).fold(f64::NEG_INFINITY, f64::max);
    if peak.is_finite() {
        for p in smooth.iter_mut() {
            p.1 = (p.1 - peak).max(-100.0);
        }
    }
    Ok(smooth)
}

fn mean_spectrum(x: &[f32], sample_rate: f64) -> (Vec<f64>, Vec<f64>) {
    let n_frames = 1 + x.len().saturating_sub(NFFT) / HOP;
    let frame_indices: Vec<usize> = if n_frames <= MAX_FRAMES {
        (0..n_frames).collect()
    } else {
        (0..MAX_FRAMES)
            .map(|k| {
                let t = k as f64 * (n_frames - 1) as f64 / (MAX_FRAMES - 1) as f64;
                (t as usize).min(n_frames - 1)
            })
            .collect()
    };

    let win: Vec<f64> = (0..NFFT)
        .map(|i| {
            let t = 2.0 * std::f64::consts::PI * i as f64 / (NFFT - 1) as f64;
            0.5 - 0.5 * t.cos()
        })
        .collect();
    let win_norm: f64 = win.iter().map(|v| v * v).sum();

    let mut planner = FftPlanner::<f64>::new();
    let fft = planner.plan_fft_forward(NFFT);
    let mut buf = vec![Complex::new(0.0, 0.0); NFFT];
    let mut acc = vec![0.0f64; NFFT / 2 + 1];
    let mut used = 0usize;

    for &idx in &frame_indices {
        let start = idx * HOP;
        if start + NFFT > x.len() {
            break;
        }
        for j in 0..NFFT {
            buf[j] = Complex::new(x[start + j] as f64 * win[j], 0.0);
        }
        fft.process(&mut buf);
        for (k, slot) in acc.iter_mut().enumerate() {
            *slot += buf[k].norm_sqr() / win_norm;
        }
        used += 1;
    }

    let used = used.max(1) as f64;
    let db: Vec<f64> = acc
        .iter()
        .map(|v| 10.0 * (v / used + 1e-30).log10())
        .collect();
    let freqs: Vec<f64> = (0..=NFFT / 2)
        .map(|k| k as f64 * sample_rate / NFFT as f64)
        .collect();
    (freqs, db)
}

/// 9 点滑动平均，边界补零（等价于 np.convolve(..., mode="same")）。
fn smooth(a: &[f64], w: usize) -> Vec<f64> {
    let mut out = vec![0.0; a.len()];
    let half = (w / 2) as isize;
    for (i, slot) in out.iter_mut().enumerate() {
        let mut sum = 0.0;
        for j in 0..w {
            let idx = i as isize + j as isize - half;
            if idx >= 0 && (idx as usize) < a.len() {
                sum += a[idx as usize];
            }
        }
        *slot = sum / w as f64;
    }
    out
}

fn find_edge(freqs: &[f64], sm: &[f64], nyquist: f64) -> (Option<f64>, f64) {
    let df = freqs[1] - freqs[0];
    let w = ((SLOPE_WIN_HZ / df) as usize).max(1);
    let lo = (KNEE_MIN_HZ / df) as usize;
    let hi = ((nyquist - 400.0).max(0.0) / df) as usize;
    let start = w.max(lo);
    let end = hi.min(sm.len().saturating_sub(w + 1));

    let mut best_f = None;
    let mut best_slope = 0.0f64;
    for i in start..end {
        let slope = (sm[i + w] - sm[i - w]) / (2.0 * w as f64 * df) * 1000.0;
        if slope < best_slope {
            best_slope = slope;
            best_f = Some(freqs[i]);
        }
    }

    if best_f.is_some() && best_slope <= -SLOPE_MIN {
        (best_f, best_slope)
    } else {
        (None, best_slope)
    }
}

fn values_in_band(sm: &[f64], freqs: &[f64], lo: f64, hi: f64) -> Vec<f64> {
    freqs
        .iter()
        .zip(sm.iter())
        .filter(|(f, _)| **f >= lo && **f <= hi)
        .map(|(_, v)| *v)
        .collect()
}

fn median(sm: &[f64], freqs: &[f64], lo: f64, hi: f64) -> f64 {
    median_of_slice(&values_in_band(sm, freqs, lo, hi))
}

fn median_of_slice(values: &[f64]) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    let mut v = values.to_vec();
    v.sort_by(|a, b| a.partial_cmp(b).unwrap_or(Ordering::Equal));
    let n = v.len();
    if n % 2 == 1 {
        v[n / 2]
    } else {
        (v[n / 2 - 1] + v[n / 2]) / 2.0
    }
}

fn mean_of_slice(values: &[f64]) -> f64 {
    if values.is_empty() {
        0.0
    } else {
        values.iter().sum::<f64>() / values.len() as f64
    }
}

fn round2(v: f64) -> f64 {
    (v * 100.0).round() / 100.0
}

#[cfg(test)]
mod tests {
    use super::{classify, profile_is_he_aac, sample_windows, Family};

    #[test]
    fn detects_he_aac_profiles() {
        assert!(profile_is_he_aac("HE-AAC"));
        assert!(profile_is_he_aac("HE-AACv2"));
        assert!(profile_is_he_aac("xHE-AAC"));
        assert!(profile_is_he_aac("he_aac"));
        assert!(!profile_is_he_aac("LC"));
        assert!(!profile_is_he_aac(""));
    }

    #[test]
    fn mp3_overlap_is_an_interval_not_a_guess() {
        let guess = classify(Family::Mp3, 16969.0);
        assert_eq!(guess.estimate, "128/192/320");
        assert!(guess.confidence_scale < 0.5);
    }

    #[test]
    fn long_files_use_three_in_bounds_windows() {
        assert!(sample_windows(30.0).is_empty());
        assert!(sample_windows(600.0).is_empty());
        assert!(!sample_windows(600.5).is_empty());

        let windows = sample_windows(3600.0);
        assert_eq!(windows.len(), 3);
        for (start, len) in &windows {
            assert!(*start >= 0.0);
            assert!(*start + *len <= 3600.0 + 1e-9);
        }
        assert!(windows[0].0 < windows[1].0);
        assert!(windows[1].0 < windows[2].0);
    }
}
