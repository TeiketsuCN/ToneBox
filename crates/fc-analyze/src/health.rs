//! 内容体检：判断"这段音频到底是不是有效内容"，不判断"好不好听"。
//!
//! 全部是确定性 DSP，不需要模型：
//! - 静音占比、峰值、削波、直流偏移、左右声道相关
//! - 谱平坦度（白噪声）、单频占比（测试音）、音高稳定度（自相关）
//! - 响度范围（LRA，近似）
//! - 音乐性弱指标：onset 密度、节拍强度
//!
//! 输出的是"分项 + 结论"，不是伪造的概率分数。

use std::io::Read;
use std::path::Path;
use std::process::{Command, Stdio};

use anyhow::{Context, Result};
use rustfft::num_complex::Complex;
use rustfft::FftPlanner;

const RATE: usize = 44_100;
const WIN_FRAMES: usize = RATE / 10; // 100ms
const LONG_WIN: usize = RATE * 3; // 3s
const FFT_N: usize = 2048;
const ONSET_N: usize = 1024;
const ONSET_HOP: usize = 512;

/// 严重程度
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    Ok,
    Warn,
    Bad,
}

/// 一条体检结果
#[derive(Debug, Clone)]
pub struct Check {
    pub name: &'static str,
    pub value: String,
    pub severity: Severity,
}

/// 结论
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    Normal,
    MostlySilent,
    Clipped,
    TestTone,
    Noise,
    Uncertain,
}

impl Verdict {
    pub fn label(self) -> &'static str {
        match self {
            Verdict::Normal => "像正常音乐",
            Verdict::MostlySilent => "内容异常：几乎全是静音",
            Verdict::Clipped => "内容异常：大比例削波",
            Verdict::TestTone => "像测试信号（单频/扫频）",
            Verdict::Noise => "像噪声，不像音乐",
            Verdict::Uncertain => "无法判断（内容过于简单）",
        }
    }
}

/// 体检报告
#[derive(Debug, Clone)]
pub struct Health {
    pub duration_s: f64,
    pub channels: u32,
    /// 非静音时长占比 0..1
    pub active_ratio: f64,
    pub peak_dbfs: f64,
    pub clip_ratio: f64,
    pub dc_offset: f64,
    /// 左右声道相关系数；单声道固定 1.0
    pub stereo_corr: f64,
    pub lra_lu: f64,
    /// 谱平坦度中位数 0..1（越大越像噪声）
    pub flatness: f64,
    /// 峰值附近 ±3 bin 的能量占比中位数
    pub tonal_share: f64,
    pub onset_rate: f64,
    pub beat_strength: f64,
    /// 音高稳定度中位数 0..1
    pub pitch_stability: f64,
    pub checks: Vec<Check>,
    pub verdict: Verdict,
}

fn median(v: &mut Vec<f64>) -> f64 {
    if v.is_empty() {
        return 0.0;
    }
    v.sort_by(|a, b| a.partial_cmp(b).unwrap());
    v[v.len() / 2]
}

/// 体检一个文件。整轨解码（44.1k，最多 2 声道），统计量流式累加。
pub fn analyze(tools: &fc_core::Tools, path: &Path) -> Result<Health> {
    // 先问一下几个声道：单声道如果强行升成双声道，ffmpeg 会做 -3dB 的等功率上混，
    // 峰值和削波判断就全错了。
    let info = fc_core::probe(tools, path).context("读不到音频信息（内容体检）")?;
    let channels = info.channels.clamp(1, 2);
    let ch = channels as usize;

    let mut cmd = Command::new(&tools.ffmpeg);
    cmd.args(["-v", "error", "-nostdin", "-i"])
        .arg(path)
        .args(["-map", "0:a:0", "-ac"])
        .arg(channels.to_string())
        .args(["-ar"])
        .arg(RATE.to_string())
        .args(["-f", "f32le", "-"])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    // 不隐藏窗口的话，每选中一首歌都会弹一个黑框出来
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(crate::CREATE_NO_WINDOW);
    }
    let mut child = cmd.spawn().context("启动 ffmpeg 失败（内容体检）")?;

    let mut out = child.stdout.take().context("拿不到 ffmpeg 输出")?;
    let mut planner = FftPlanner::<f32>::new();
    let fft = planner.plan_fft_forward(FFT_N);
    let fft_onset = planner.plan_fft_forward(ONSET_N);

    let mut frames: u64 = 0;
    let mut sum_l = 0.0f64;
    let mut sum_r = 0.0f64;
    let mut sum_ll = 0.0f64;
    let mut sum_rr = 0.0f64;
    let mut sum_lr = 0.0f64;
    let mut peak = 0.0f32;
    let mut clipped: u64 = 0;
    let mut total_samples: u64 = 0;

    let mut win_acc = 0.0f64;
    let mut win_n = 0usize;
    let mut silent_wins = 0u64;
    let mut total_wins = 0u64;

    let mut long_acc = 0.0f64;
    let mut long_n = 0usize;
    let mut long_rms: Vec<f64> = Vec::new();

    let mut since_spec = 0usize;
    let mut spec_buf: Vec<f32> = Vec::with_capacity(FFT_N);
    let mut flats: Vec<f64> = Vec::new();
    let mut tonals: Vec<f64> = Vec::new();
    let mut pitches: Vec<f64> = Vec::new();

    let mut onset_buf: Vec<f32> = Vec::with_capacity(ONSET_N);
    let mut prev_mag: Option<Vec<f32>> = None;
    let mut envelope: Vec<f32> = Vec::new();
    let mut since_onset = 0usize;

    let mut buf = vec![0u8; 64 * 1024];
    let mut carry: Vec<u8> = Vec::new();
    let frame_bytes = 4 * ch;
    loop {
        let n = out.read(&mut buf).context("读 ffmpeg 输出失败")?;
        if n == 0 {
            break;
        }
        carry.extend_from_slice(&buf[..n]);
        let usable = carry.len() / frame_bytes * frame_bytes;
        let mut i = 0;
        while i + frame_bytes <= usable {
            let l = f32::from_le_bytes([carry[i], carry[i + 1], carry[i + 2], carry[i + 3]]);
            let r = if ch == 2 {
                f32::from_le_bytes([carry[i + 4], carry[i + 5], carry[i + 6], carry[i + 7]])
            } else {
                l
            };
            i += frame_bytes;

            sum_l += l as f64;
            sum_r += r as f64;
            sum_ll += (l as f64) * (l as f64);
            sum_rr += (r as f64) * (r as f64);
            sum_lr += (l as f64) * (r as f64);
            let a = l.abs().max(r.abs());
            if a > peak {
                peak = a;
            }
            if a >= 0.999_5 {
                clipped += 1;
            }
            total_samples += 1;
            frames += 1;

            let mono = (l + r) * 0.5;
            win_acc += (mono as f64) * (mono as f64);
            win_n += 1;
            if win_n >= WIN_FRAMES {
                let rms = (win_acc / win_n as f64).sqrt();
                if rms < 10f64.powf(-60.0 / 20.0) {
                    silent_wins += 1;
                }
                total_wins += 1;
                win_acc = 0.0;
                win_n = 0;
            }
            long_acc += (mono as f64) * (mono as f64);
            long_n += 1;
            if long_n >= LONG_WIN {
                let rms = (long_acc / long_n as f64).sqrt();
                if rms > 0.0 {
                    long_rms.push(rms);
                }
                long_acc = 0.0;
                long_n = 0;
            }

            since_spec += 1;
            if spec_buf.len() < FFT_N {
                spec_buf.push(mono);
            }
            if spec_buf.len() == FFT_N && since_spec >= RATE * 2 {
                let (flat, tonal, pitch) = analyze_window(&spec_buf, &fft);
                flats.push(flat);
                tonals.push(tonal);
                pitches.push(pitch);
                since_spec = 0;
                spec_buf.clear();
            }

            since_onset += 1;
            if onset_buf.len() < ONSET_N {
                onset_buf.push(mono);
            }
            if onset_buf.len() == ONSET_N && since_onset >= ONSET_HOP {
                if let Some(prev) = &prev_mag {
                    let mag = onset_mag(&onset_buf, &fft_onset);
                    let mut flux = 0.0f32;
                    let mut sum = 0.0f32;
                    for (m, p) in mag.iter().zip(prev.iter()) {
                        flux += (*m - p).max(0.0);
                        sum += *m;
                    }
                    envelope.push(if sum > 1e-6 { flux / sum } else { 0.0 });
                    prev_mag = Some(mag);
                } else {
                    prev_mag = Some(onset_mag(&onset_buf, &fft_onset));
                }
                since_onset = 0;
                onset_buf.clear();
            }
        }
        carry.drain(..usable);
    }
    let _ = child.wait();

    if frames == 0 {
        anyhow::bail!("没有解出任何采样");
    }
    let duration_s = frames as f64 / RATE as f64;
    let active_ratio = if total_wins > 0 {
        1.0 - silent_wins as f64 / total_wins as f64
    } else {
        0.0
    };
    let peak_dbfs = if peak > 0.0 {
        20.0 * (peak as f64).log10()
    } else {
        -120.0
    };
    let clip_ratio = clipped as f64 / total_samples.max(1) as f64;
    let dc = ((sum_l + sum_r) / (2.0 * total_samples.max(1) as f64)).abs();
    let stereo_corr = if ch == 1 {
        1.0
    } else if sum_ll > 1e-12 && sum_rr > 1e-12 {
        (sum_lr / (sum_ll.sqrt() * sum_rr.sqrt())).clamp(-1.0, 1.0)
    } else {
        1.0
    };
    let lra_lu = {
        if long_rms.len() >= 3 {
            let mut v: Vec<f64> = long_rms.iter().map(|x| 20.0 * x.log10()).collect();
            v.sort_by(|a, b| a.partial_cmp(b).unwrap());
            let pick = |f: f64| v[((v.len() - 1) as f64 * f) as usize];
            (pick(0.95) - pick(0.10)).max(0.0)
        } else {
            0.0
        }
    };
    let flatness = median(&mut flats);
    let tonal_share = median(&mut tonals);
    let pitch_stability = median(&mut pitches);
    let (onset_rate, beat_strength) = rhythm(&envelope, ONSET_HOP as f64 / RATE as f64);

    let mut checks = Vec::new();
    let mut push = |name: &'static str, value: String, severity: Severity| {
        checks.push(Check {
            name,
            value,
            severity,
        });
    };
    push(
        "有效音频",
        format!("{:.1}% 非静音", active_ratio * 100.0),
        if active_ratio > 0.9 {
            Severity::Ok
        } else if active_ratio > 0.5 {
            Severity::Warn
        } else {
            Severity::Bad
        },
    );
    push(
        "峰值",
        format!("{peak_dbfs:.1} dBFS"),
        if peak_dbfs > -0.1 {
            Severity::Warn
        } else if peak_dbfs < -40.0 {
            Severity::Bad
        } else {
            Severity::Ok
        },
    );
    push(
        "削波",
        if clip_ratio > 0.0 {
            format!("{:.3}% 样本贴顶", clip_ratio * 100.0)
        } else {
            "无".to_string()
        },
        if clip_ratio > 0.02 {
            Severity::Bad
        } else if clip_ratio > 0.0 {
            Severity::Warn
        } else {
            Severity::Ok
        },
    );
    push(
        "直流偏移",
        format!("{dc:.5}"),
        if dc > 0.02 {
            Severity::Warn
        } else {
            Severity::Ok
        },
    );
    push(
        "立体声相关",
        if ch == 1 {
            "单声道".to_string()
        } else {
            format!("{stereo_corr:.2}")
        },
        Severity::Ok,
    );
    push(
        "谱平坦度",
        format!("{flatness:.2}（1 = 白噪声）"),
        if flatness > 0.45 {
            Severity::Bad
        } else if flatness > 0.3 {
            Severity::Warn
        } else {
            Severity::Ok
        },
    );
    push(
        "音高稳定度",
        format!("{pitch_stability:.2}"),
        if pitch_stability < 0.12 {
            Severity::Warn
        } else {
            Severity::Ok
        },
    );
    push(
        "节拍强度",
        format!("{beat_strength:.2}"),
        if beat_strength < 0.08 {
            Severity::Warn
        } else {
            Severity::Ok
        },
    );
    push(
        "响度范围",
        format!("{lra_lu:.1} LU"),
        if lra_lu < 0.5 {
            Severity::Warn
        } else {
            Severity::Ok
        },
    );

    let verdict = if active_ratio < 0.05 {
        Verdict::MostlySilent
    } else if clip_ratio > 0.05 {
        Verdict::Clipped
    } else if flatness > 0.35 && pitch_stability < 0.3 {
        Verdict::Noise
    } else if pitch_stability > 0.85 && flatness < 0.1 && tonal_share > 0.8 {
        Verdict::TestTone
    } else if beat_strength < 0.05 && pitch_stability < 0.12 && onset_rate < 0.5 {
        Verdict::Uncertain
    } else {
        Verdict::Normal
    };

    Ok(Health {
        duration_s,
        channels,
        active_ratio,
        peak_dbfs,
        clip_ratio,
        dc_offset: dc,
        stereo_corr,
        lra_lu,
        flatness,
        tonal_share,
        onset_rate,
        beat_strength,
        pitch_stability,
        checks,
        verdict,
    })
}

/// 返回 (谱平坦度, 峰值附近能量占比, 音高稳定度)
fn analyze_window(buf: &[f32], fft: &std::sync::Arc<dyn rustfft::Fft<f32>>) -> (f64, f64, f64) {
    let n = buf.len();
    let mut data: Vec<Complex<f32>> = (0..n)
        .map(|i| {
            // Hann 窗：不加窗时纯音的旁瓣会摊到整个谱上
            let w = 0.5 - 0.5 * (std::f32::consts::TAU * i as f32 / (n - 1) as f32).cos();
            Complex::new(buf[i] * w, 0.0)
        })
        .collect();
    fft.process(&mut data);
    let half = FFT_N / 2;
    let mut power = Vec::with_capacity(half);
    for c in data.iter().take(half).skip(1) {
        power.push((c.re * c.re + c.im * c.im) as f64);
    }
    let total: f64 = power.iter().sum();
    let flatness = if total > 0.0 {
        let n = power.len() as f64;
        let log_mean = power.iter().map(|p| (p + 1e-20).ln()).sum::<f64>() / n;
        let geo = log_mean.exp();
        let arith = total / n;
        (geo / arith.max(1e-20)).clamp(0.0, 1.0)
    } else {
        0.0
    };
    let peak_i = power
        .iter()
        .enumerate()
        .max_by(|a, b| a.1.partial_cmp(b.1).unwrap())
        .map(|(i, _)| i)
        .unwrap_or(0);
    let lo = peak_i.saturating_sub(3);
    let hi = (peak_i + 4).min(power.len());
    let near: f64 = power[lo..hi].iter().sum();
    let tonal = if total > 0.0 {
        (near / total).clamp(0.0, 1.0)
    } else {
        0.0
    };

    // 音高稳定度：时域自相关，在 80–1000 Hz 的滞后范围里找峰
    let mean = buf.iter().sum::<f32>() / n as f32;
    let mut energy = 0.0f64;
    for x in buf {
        energy += ((*x - mean) as f64) * ((*x - mean) as f64);
    }
    let lag_min = (RATE / 1000).max(2);
    let lag_max = (RATE / 80).min(n / 2);
    let mut best = 0.0f64;
    if energy > 1e-9 {
        for lag in lag_min..lag_max {
            let mut acc = 0.0f64;
            for i in 0..(n - lag) {
                acc += ((buf[i] - mean) as f64) * ((buf[i + lag] - mean) as f64);
            }
            let norm = acc / energy;
            if norm > best {
                best = norm;
            }
        }
    }
    (flatness, tonal, best.clamp(0.0, 1.0))
}

fn onset_mag(buf: &[f32], fft: &std::sync::Arc<dyn rustfft::Fft<f32>>) -> Vec<f32> {
    let mut data: Vec<Complex<f32>> = buf.iter().map(|x| Complex::new(*x, 0.0)).collect();
    fft.process(&mut data);
    data.iter()
        .take(ONSET_N / 2)
        .map(|c| (c.re * c.re + c.im * c.im).sqrt())
        .collect()
}

/// 从 onset 包络算 (每秒 onset 数, 节拍强度)
fn rhythm(env: &[f32], hop_s: f64) -> (f64, f64) {
    if env.len() < 8 {
        return (0.0, 0.0);
    }
    let mean = env.iter().map(|x| *x as f64).sum::<f64>() / env.len() as f64;
    let var = env
        .iter()
        .map(|x| {
            let d = *x as f64 - mean;
            d * d
        })
        .sum::<f64>()
        / env.len() as f64;
    let sd = var.sqrt();
    if sd / mean.abs().max(1e-9) < 0.02 {
        return (0.0, 0.0);
    }
    let thr = mean + 0.8 * sd;
    let mut onsets = 0usize;
    let mut prev_above = false;
    for v in env {
        let above = *v as f64 > thr;
        if above && !prev_above {
            onsets += 1;
        }
        prev_above = above;
    }
    let rate = onsets as f64 / (env.len() as f64 * hop_s).max(1e-6);

    let min_lag = (0.25 / hop_s).round() as usize;
    let max_lag = ((2.0 / hop_s).round() as usize).min(env.len() / 2);
    let mut zero = 0.0f64;
    for v in env {
        let d = *v as f64 - mean;
        zero += d * d;
    }
    let mut best = 0.0f64;
    if zero > 1e-9 && max_lag > min_lag {
        for lag in min_lag..=max_lag {
            let mut acc = 0.0f64;
            for i in 0..(env.len() - lag) {
                acc += (env[i] as f64 - mean) * (env[i + lag] as f64 - mean);
            }
            let norm = acc / zero;
            if norm > best {
                best = norm;
            }
        }
    }
    (rate, best.clamp(0.0, 1.0))
}
