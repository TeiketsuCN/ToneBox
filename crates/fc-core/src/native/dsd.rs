//! DSF / DFF（DSD）读取 + 抽取成 PCM。
//!
//! DSD 是 1-bit 流，解码 = 低通 + 降采样（decimation）。这里用两级：
//! 1. 8 点滑动平均把 2.8 MHz 先降到 352.8 kHz（简单、够快，而且能把
//!    高频量化噪声先压掉一大截）；
//! 2. 63 抽头窗函数 sinc FIR 再降到目标采样率（176.4 kHz），负责把
//!    44.1 kHz 以上的残留噪声挡掉。
//!
//! 目标采样率固定 176400：DSD64 是 /16、DSD128 /32、DSD256 /64，
//! 整除关系简单，后面交给 ffmpeg 再降采样也不会打架。

use std::fs::File;
use std::io::{BufReader, Read};
use std::path::Path;

use anyhow::{anyhow, Context, Result};

use super::{u16_be, u32_be, u32_le, u64_be, u64_le, WavInfo, WavSink};

/// 输出 PCM 采样率
const TARGET_HZ: u32 = 176_400;
/// 第一级平均的点数
const BOXCAR: usize = 8;
/// FIR 抽头数（奇数，便于对称）
const TAPS: usize = 63;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Dsf,
    Dff,
}

/// 打开文件、解析头，返回 (kind, 声道, DSD 采样率, 每声道 1-bit 采样数, 数据起点)
fn parse_header(b: &[u8]) -> Result<(Kind, usize, u32, u64, usize)> {
    if b.len() < 28 {
        return Err(anyhow!("文件太短"));
    }
    if &b[0..4] == b"DSD " {
        // DSF：小端
        // 0 "DSD " / 4 chunk size / 12 file size / 20 metadata
        // 28 "fmt " ...
        if b.len() < 92 || &b[28..32] != b"fmt " {
            return Err(anyhow!("DSF 缺少 fmt 块"));
        }
        // fmt 块 body 从 40 开始：40 version / 44 fmtid / 48 channel type /
        // 52 channel num / 56 sampling freq / 60 bits / 64 sample count(8) /
        // 72 block size / 76 reserved
        let channels = u32_le(b, 52) as usize;
        // 规范里存的就是 1-bit 采样率（DSD64 = 2822400）。
        // 注意 ffprobe 会把它显示成字节率（352800），别被它带偏。
        let rate = u32_le(b, 56);
        let samples = u64_le(b, 64);
        let block = u32_le(b, 72) as usize;
        if &b[80..84] != b"data" {
            return Err(anyhow!("DSF 缺少 data 块"));
        }
        let _ = block;
        let dsd_rate = rate;
        Ok((Kind::Dsf, channels.max(1), dsd_rate, samples, 92))
    } else if &b[0..4] == b"FRM8" {
        // DFF：大端，块结构
        // FRM8(4) + size(8) + 表单类型 "DSD "(4) 之后才开始有子块
        let mut pos = 16usize;
        let mut channels = 2usize;
        let mut dsd_rate = 0u32;
        while pos + 12 <= b.len() {
            let id = &b[pos..pos + 4];
            let size = u64_be(b, pos + 4) as usize;
            let body = pos + 12;
            match id {
                b"PROP" => {
                    let mut p = body;
                    let end = (body + size).min(b.len());
                    while p + 12 <= end {
                        let sid = &b[p..p + 4];
                        let ssize = u64_be(b, p + 4) as usize;
                        let sbody = p + 12;
                        match sid {
                            // PROP 里 "SND " 只是个区段标记，本身没有长度字段
                            b"SND " => {
                                p += 4;
                                continue;
                            }
                            b"FS  " => dsd_rate = u32_be(b, sbody),
                            b"CHNL" => channels = u16_be(b, sbody) as usize,
                            _ => {}
                        }
                        p = sbody + ssize + (ssize & 1);
                    }
                    pos = body + size + (size & 1);
                }
                b"DSD " => {
                    if dsd_rate == 0 {
                        return Err(anyhow!("DFF 缺少采样率"));
                    }
                    let bits = (size as u64) * 8 / channels.max(1) as u64;
                    return Ok((Kind::Dff, channels.max(1), dsd_rate, bits, body));
                }
                _ => {
                    pos = body + size + (size & 1);
                }
            }
        }
        Err(anyhow!("DFF 里没有 DSD 数据块"))
    } else {
        Err(anyhow!("不是 DSF/DFF"))
    }
}

/// 生成窗函数 sinc 低通（Blackman 窗）
fn lowpass(cutoff_norm: f64) -> Vec<f32> {
    let mut h = Vec::with_capacity(TAPS);
    let m = (TAPS - 1) as f64;
    let mut sum = 0.0f64;
    for i in 0..TAPS {
        let n = i as f64 - m / 2.0;
        let sinc = if n.abs() < 1e-9 {
            1.0
        } else {
            let x = std::f64::consts::PI * n * cutoff_norm;
            x.sin() / x
        };
        // Blackman
        let w = 0.42 - 0.5 * (2.0 * std::f64::consts::PI * i as f64 / m).cos()
            + 0.08 * (4.0 * std::f64::consts::PI * i as f64 / m).cos();
        let v = sinc * w;
        sum += v;
        h.push(v as f32);
    }
    // 归一化直流增益
    let g = sum as f32;
    for v in &mut h {
        *v /= g;
    }
    h
}

/// 单声道的抽取器
struct Decim {
    /// 第一级平均
    acc: f32,
    n: usize,
    /// 第二级 FIR
    hist: Vec<f32>,
    idx: usize,
    taps: Vec<f32>,
    /// 每多少个第一级输出取一个
    m: usize,
    count: usize,
}

impl Decim {
    fn new(m: usize, cutoff_norm: f64) -> Self {
        Decim {
            acc: 0.0,
            n: 0,
            hist: vec![0.0; TAPS],
            idx: 0,
            taps: lowpass(cutoff_norm),
            m: m.max(1),
            count: 0,
        }
    }

    /// 送一个 ±1 的 DSD 位；返回下一个 PCM 样本（可能没有）
    fn push(&mut self, bit: bool) -> Option<f32> {
        self.acc += if bit { 1.0 } else { -1.0 };
        self.n += 1;
        if self.n < BOXCAR {
            return None;
        }
        let avg = self.acc / BOXCAR as f32;
        self.acc = 0.0;
        self.n = 0;
        self.hist[self.idx] = avg;
        self.idx = (self.idx + 1) % TAPS;
        self.count += 1;
        if self.count < self.m {
            return None;
        }
        self.count = 0;
        // 卷积：hist 环形缓冲，从最旧到最新
        let mut sum = 0.0f32;
        let mut k = self.idx;
        for t in 0..TAPS {
            sum += self.hist[k] * self.taps[t];
            k = (k + 1) % TAPS;
        }
        Some(sum)
    }
}

pub fn decode(input: &Path, output: &Path) -> Result<WavInfo> {
    let mut f = BufReader::new(
        File::open(input).with_context(|| format!("打开 DSD 失败：{}", input.display()))?,
    );
    // 头最多几百字节，先读 1 KB 再说
    let mut head = vec![0u8; 1024];
    let got = f.read(&mut head)?;
    head.truncate(got);
    let (kind, channels, dsd_rate, samples, data_off) = parse_header(&head)?;
    if channels == 0 || channels > 2 {
        return Err(anyhow!("DSD 只支持 1~2 声道，这个文件是 {channels}"));
    }
    let factor = (dsd_rate / TARGET_HZ).max(1) as usize;
    if dsd_rate % TARGET_HZ != 0 || factor % BOXCAR != 0 {
        return Err(anyhow!(
            "DSD 率 {dsd_rate} 不是 176400 的整数倍（{factor}），暂不支持"
        ));
    }
    let m = factor / BOXCAR;
    let cutoff = 0.45;
    let mut dec: Vec<Decim> = (0..channels).map(|_| Decim::new(m, cutoff)).collect();
    let mut sink = WavSink::create(output, TARGET_HZ, channels as u16)?;

    // 头已经读掉了，回到数据起点再整体读入（DSD 文件大，但解码是流式的，逐块处理）
    use std::io::Seek;
    f.seek(std::io::SeekFrom::Start(data_off as u64))?;
    let mut all = Vec::new();
    f.read_to_end(&mut all)?;
    let data = &all[..];

    // 每声道先各自收集，最后再交织成帧序列
    let mut outc: Vec<Vec<f32>> = (0..channels).map(|_| Vec::new()).collect();
    match kind {
        Kind::Dsf => {
            // 分声道 block：每块 4096 字节，先左后右，LSB-first
            let block = 4096usize;
            let mut pos = 0usize;
            while pos < data.len() {
                for c in 0..channels {
                    let start = pos + c * block;
                    if start >= data.len() {
                        break;
                    }
                    let end = (start + block).min(data.len());
                    for byte in &data[start..end] {
                        for bit in 0..8 {
                            let on = byte & (1 << bit) != 0;
                            if let Some(s) = dec[c].push(on) {
                                outc[c].push(s);
                            }
                        }
                    }
                }
                pos += block * channels;
            }
        }
        Kind::Dff => {
            // 交织，MSB-first
            let mut ci = 0usize;
            for byte in data {
                for bit in 0..8 {
                    let on = byte & (0x80 >> bit) != 0;
                    if let Some(s) = dec[ci].push(on) {
                        outc[ci].push(s);
                    }
                    ci = (ci + 1) % channels;
                }
            }
        }
    }

    let frames = outc.iter().map(|v| v.len()).min().unwrap_or(0);
    let mut inter = Vec::with_capacity(frames * channels);
    for i in 0..frames {
        for ch in &outc {
            inter.push(ch[i]);
        }
    }
    sink.write(&inter)?;
    let _ = samples;
    sink.finish()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dsdout::{encode_memory, Container, Tier};

    fn tmp(name: &str) -> std::path::PathBuf {
        let mut p = std::env::temp_dir();
        p.push(format!("native_dsd_{}_{}", name, std::process::id()));
        p
    }

    #[test]
    fn roundtrip_tone_survives() {
        // 用我们自己的编码器造 DSD，再用这里的解码器解回来
        let rate = 44100u32;
        let n = rate as usize; // 1 秒
        let mut pcm = Vec::with_capacity(n * 2);
        for i in 0..n {
            let s = (2.0 * std::f64::consts::PI * 440.0 * i as f64 / rate as f64).sin() * 0.5;
            pcm.push(s as f32);
            pcm.push(s as f32);
        }
        for (container, tag) in [(Container::Dsf, "dsf"), (Container::Dff, "dff")] {
            let dsd = tmp(tag);
            encode_memory(&dsd, container, Tier::D64, rate, 2, &pcm).unwrap();
            let wav = tmp(&format!("{tag}_out"));
            let info = decode(&dsd, &wav).unwrap();
            assert_eq!(info.rate, TARGET_HZ);
            assert_eq!(info.channels, 2);
            assert!(info.frames > 100_000, "帧数太少：{}", info.frames);
            // 读回 f32 检查 440Hz 主峰
            let b = super::super::read_all(&wav).unwrap();
            let data = &b[44..];
            let mut left = Vec::with_capacity(data.len() / 8);
            for ch in data.chunks_exact(8) {
                left.push(f32::from_le_bytes([ch[0], ch[1], ch[2], ch[3]]));
            }
            let sr = TARGET_HZ as f64;
            let power = |freq: f64| -> f64 {
                let w = 2.0 * std::f64::consts::PI * freq / sr;
                let (mut re, mut im) = (0.0, 0.0);
                for (i, v) in left.iter().enumerate() {
                    re += *v as f64 * (w * i as f64).cos();
                    im -= *v as f64 * (w * i as f64).sin();
                }
                re * re + im * im
            };
            let p440 = power(440.0);
            let p5k = power(5000.0);
            assert!(p440 > p5k * 50.0, "{tag}: 440Hz={p440} 5kHz={p5k}");
            let _ = std::fs::remove_file(&dsd);
            let _ = std::fs::remove_file(&wav);
        }
    }
}
