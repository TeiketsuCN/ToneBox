//! DSD 输出：PCM → 1-bit sigma-delta 调制 → DSF / DFF 容器。
//!
//! ffmpeg 只有 DSD 解码器、没有编码器，所以这里自己实现：
//! 线性插值升采样 + 二阶单比特噪声整形 + 按规范写容器。
//!
//! 音质定位：这是「PCM 转 DSD」的重调制，不是录音端直出。二阶调制器在 64 倍
//! 过采样下带内噪声底大约 -85 dB，听感接近高解析 PCM，但不会比源 PCM 更好。

use std::fs::File;
use std::io::{BufWriter, Seek, SeekFrom, Write};
use std::path::Path;

/// 每声道每个 block 的字节数（DSF 规范写死 4096）
const BLOCK: usize = 4096;

/// DSF 头固定长度
const DSF_HEAD: u64 = 92;

/// DSD 档位：DSD64 = 2.8224 MHz
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tier {
    D64,
    D128,
    D256,
}

impl Tier {
    pub fn from_u32(v: u32) -> Option<Tier> {
        match v {
            64 => Some(Tier::D64),
            128 => Some(Tier::D128),
            256 => Some(Tier::D256),
            _ => None,
        }
    }

    pub fn multiplier(self) -> u32 {
        match self {
            Tier::D64 => 64,
            Tier::D128 => 128,
            Tier::D256 => 256,
        }
    }

    /// 目标 DSD 采样率（Hz）
    pub fn dsd_rate(self, base_hz: u32) -> u32 {
        base_hz.saturating_mul(self.multiplier())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Container {
    Dsf,
    Dff,
}

// ── 二阶单比特 sigma-delta 调制器 ─────────────────────────────
//
// CIFB：两个积分器串联，输出符号做反馈。系数 1.0 / 0.5 是单比特二阶的常用取值。
// 积分器钳位是为了大信号时不失稳啸叫 —— 稳定比理论噪声底重要。
struct Mod2 {
    i1: f64,
    i2: f64,
}

impl Mod2 {
    fn new() -> Self {
        Mod2 { i1: 0.0, i2: 0.0 }
    }

    fn step(&mut self, x: f64) -> bool {
        let y = if self.i2 >= 0.0 { 1.0 } else { -1.0 };
        self.i1 = (self.i1 + x - y).clamp(-8.0, 8.0);
        self.i2 = (self.i2 + self.i1 - 0.5 * y).clamp(-8.0, 8.0);
        y > 0.0
    }
}

/// 线性插值升采样：每个输入样本产出 `ratio` 个输出样本。
///
/// 用线性而不是零阶保持：零阶保持会在输入采样率的整数倍处留下强镜像。
struct Interp {
    prev: f64,
    cur: f64,
    phase: u32,
    ratio: u32,
}

impl Interp {
    fn new(ratio: u32) -> Self {
        Interp {
            prev: 0.0,
            cur: 0.0,
            phase: 0,
            ratio: ratio.max(1),
        }
    }

    fn push(&mut self, v: f64) -> f64 {
        if self.phase == 0 {
            self.prev = self.cur;
            self.cur = v;
        }
        let t = self.phase as f64 / self.ratio as f64;
        self.phase = (self.phase + 1) % self.ratio;
        self.prev + (self.cur - self.prev) * t
    }
}

/// 位打包：DSF 是 LSB-first，DFF 是 MSB-first
struct BitPack {
    byte: u8,
    n: u8,
    lsb_first: bool,
}

impl BitPack {
    fn new(lsb_first: bool) -> Self {
        BitPack {
            byte: 0,
            n: 0,
            lsb_first,
        }
    }

    fn push(&mut self, bit: bool) -> Option<u8> {
        if bit {
            if self.lsb_first {
                self.byte |= 1 << self.n;
            } else {
                self.byte |= 0x80 >> self.n;
            }
        }
        self.n += 1;
        if self.n == 8 {
            self.n = 0;
            let b = self.byte;
            self.byte = 0;
            Some(b)
        } else {
            None
        }
    }

    fn flush(&mut self) -> Option<u8> {
        if self.n == 0 {
            return None;
        }
        self.n = 0;
        let b = self.byte;
        self.byte = 0;
        Some(b)
    }
}

/// DSD 写入器：push 交织 f32 PCM，finish 时补齐尾部并回填长度字段。
pub struct DsdWriter {
    out: BufWriter<File>,
    container: Container,
    channels: usize,
    samples: u64,
    block: Vec<Vec<u8>>,
    pending: Vec<u8>,
    data_bytes: u64,
    packs: Vec<BitPack>,
    interp: Vec<Interp>,
    mods: Vec<Mod2>,
    dither: u32,
}

impl DsdWriter {
    /// `in_rate` 必须是 44100 或 48000 家族（调用方已重采样）。
    pub fn new(
        out: &Path,
        container: Container,
        tier: Tier,
        in_rate: u32,
        channels: usize,
    ) -> std::io::Result<Self> {
        assert!((1..=2).contains(&channels), "DSD 只支持 1~2 声道");
        let dsd_rate = tier.dsd_rate(in_rate);
        let ratio = tier.multiplier();
        let f = File::create(out)?;
        let mut w = DsdWriter {
            out: BufWriter::new(f),
            container,
            channels,
            samples: 0,
            block: (0..channels).map(|_| Vec::with_capacity(BLOCK)).collect(),
            pending: Vec::with_capacity(8192),
            data_bytes: 0,
            packs: (0..channels)
                .map(|_| BitPack::new(container == Container::Dsf))
                .collect(),
            interp: (0..channels).map(|_| Interp::new(ratio)).collect(),
            mods: (0..channels).map(|_| Mod2::new()).collect(),
            dither: 0x9E37_79B9,
        };
        match container {
            Container::Dsf => w.write_dsf_header(dsd_rate)?,
            Container::Dff => w.write_dff_header(dsd_rate)?,
        }
        Ok(w)
    }

    fn next_dither(&mut self) -> f64 {
        self.dither ^= self.dither << 13;
        self.dither ^= self.dither >> 17;
        self.dither ^= self.dither << 5;
        (self.dither as f64 / u32::MAX as f64 - 0.5) * 2.0 * 1e-4
    }

    pub fn push(&mut self, interleaved: &[f32]) -> std::io::Result<()> {
        let ch = self.channels;
        let frames = interleaved.len() / ch;
        let ratio = self.interp[0].ratio;
        let mut bytes: Vec<(usize, u8)> = Vec::with_capacity(ch * 8);
        for i in 0..frames {
            for c in 0..ch {
                let v = interleaved[i * ch + c] as f64;
                for _ in 0..ratio {
                    let s = self.interp[c].push(v);
                    let d = self.next_dither();
                    let x = (s + d).clamp(-1.0, 1.0);
                    let bit = self.mods[c].step(x);
                    if c == 0 {
                        self.samples += 1;
                    }
                    if let Some(b) = self.packs[c].push(bit) {
                        bytes.push((c, b));
                    }
                }
            }
            for (c, b) in bytes.drain(..) {
                self.emit(c, b)?;
            }
        }
        Ok(())
    }

    fn emit(&mut self, c: usize, byte: u8) -> std::io::Result<()> {
        match self.container {
            Container::Dsf => {
                self.block[c].push(byte);
                if self.block[c].len() == BLOCK {
                    let blk = std::mem::take(&mut self.block[c]);
                    self.out.write_all(&blk)?;
                    self.data_bytes += blk.len() as u64;
                    self.block[c] = blk;
                    self.block[c].clear();
                }
            }
            Container::Dff => {
                self.pending.push(byte);
                if self.pending.len() == self.channels {
                    self.out.write_all(&self.pending)?;
                    self.data_bytes += self.pending.len() as u64;
                    self.pending.clear();
                }
            }
        }
        Ok(())
    }

    pub fn finish(mut self) -> std::io::Result<u64> {
        for c in 0..self.channels {
            if let Some(b) = self.packs[c].flush() {
                self.emit(c, b)?;
            }
        }
        match self.container {
            Container::Dsf => {
                for c in 0..self.channels {
                    if !self.block[c].is_empty() {
                        let mut blk = std::mem::take(&mut self.block[c]);
                        blk.resize(BLOCK, 0);
                        self.out.write_all(&blk)?;
                        self.data_bytes += BLOCK as u64;
                    }
                }
            }
            Container::Dff => {
                if !self.pending.is_empty() {
                    let rest = std::mem::take(&mut self.pending);
                    self.out.write_all(&rest)?;
                    self.data_bytes += rest.len() as u64;
                }
            }
        }
        let ch = self.channels;
        let data = self.data_bytes;
        let samples = self.samples;
        let container = self.container;
        let mut f = self.out.into_inner().map_err(|e| e.into_error())?;
        match container {
            Container::Dsf => {
                let total = DSF_HEAD + data;
                f.seek(SeekFrom::Start(12))?;
                f.write_all(&total.to_le_bytes())?;
                f.seek(SeekFrom::Start(64))?;
                f.write_all(&samples.to_le_bytes())?;
                f.seek(SeekFrom::Start(84))?;
                f.write_all(&(data + 12).to_le_bytes())?;
                f.flush()?;
                Ok(total)
            }
            Container::Dff => {
                let total = dff_data_offset(ch) + data;
                f.seek(SeekFrom::Start(4))?;
                f.write_all(&(total - 12).to_be_bytes())?;
                f.seek(SeekFrom::Start(dff_data_offset(ch) - 8))?;
                f.write_all(&data.to_be_bytes())?;
                f.flush()?;
                Ok(total)
            }
        }
    }
}

fn dff_prop_len(channels: usize) -> usize {
    4 + 16 + (14 + 4 * channels) + 32
}

fn dff_data_offset(channels: usize) -> u64 {
    (12 + 4 + 16 + (12 + dff_prop_len(channels)) + 12) as u64
}

impl DsdWriter {
    fn write_dsf_header(&mut self, dsd_rate: u32) -> std::io::Result<()> {
        self.out.write_all(b"DSD ")?;
        self.out.write_all(&28u64.to_le_bytes())?;
        self.out.write_all(&0u64.to_le_bytes())?;
        self.out.write_all(&0u64.to_le_bytes())?;
        self.out.write_all(b"fmt ")?;
        self.out.write_all(&52u64.to_le_bytes())?;
        self.out.write_all(&1u32.to_le_bytes())?;
        self.out.write_all(&0u32.to_le_bytes())?;
        let ctype: u32 = if self.channels == 1 { 1 } else { 2 };
        self.out.write_all(&ctype.to_le_bytes())?;
        self.out.write_all(&(self.channels as u32).to_le_bytes())?;
        self.out.write_all(&dsd_rate.to_le_bytes())?;
        self.out.write_all(&1u32.to_le_bytes())?;
        self.out.write_all(&0u64.to_le_bytes())?;
        self.out.write_all(&(BLOCK as u32).to_le_bytes())?;
        self.out.write_all(&0u32.to_le_bytes())?;
        self.out.write_all(b"data")?;
        self.out.write_all(&0u64.to_le_bytes())?;
        Ok(())
    }

    fn write_dff_header(&mut self, dsd_rate: u32) -> std::io::Result<()> {
        let prop = dff_prop_len(self.channels);
        self.out.write_all(b"FRM8")?;
        self.out.write_all(&0u64.to_be_bytes())?;
        self.out.write_all(b"DSD ")?;
        self.out.write_all(b"FVER")?;
        self.out.write_all(&4u64.to_be_bytes())?;
        self.out.write_all(&0x0105_0000u32.to_be_bytes())?;
        self.out.write_all(b"PROP")?;
        self.out.write_all(&(prop as u64).to_be_bytes())?;
        self.out.write_all(b"SND ")?;
        self.out.write_all(b"FS  ")?;
        self.out.write_all(&4u64.to_be_bytes())?;
        self.out.write_all(&dsd_rate.to_be_bytes())?;
        let chnl = 2 + 4 * self.channels;
        self.out.write_all(b"CHNL")?;
        self.out.write_all(&(chnl as u64).to_be_bytes())?;
        self.out.write_all(&(self.channels as u16).to_be_bytes())?;
        for id in ["SLFT", "SRGT"].iter().take(self.channels) {
            self.out.write_all(id.as_bytes())?;
        }
        self.out.write_all(b"CMPR")?;
        self.out.write_all(&20u64.to_be_bytes())?;
        self.out.write_all(b"DSD ")?;
        self.out.write_all(&[0u8])?;
        self.out.write_all(b"not compressed")?;
        self.out.write_all(&[0u8])?;
        self.out.write_all(b"DSD ")?;
        self.out.write_all(&0u64.to_be_bytes())?;
        Ok(())
    }
}

/// 一次性把整段交织 PCM 编码成 DSD 文件，返回文件字节数。
pub fn encode_memory(
    out: &Path,
    container: Container,
    tier: Tier,
    in_rate: u32,
    channels: usize,
    interleaved: &[f32],
) -> std::io::Result<u64> {
    let mut w = DsdWriter::new(out, container, tier, in_rate, channels)?;
    w.push(interleaved)?;
    w.finish()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read;

    fn tmp(name: &str) -> std::path::PathBuf {
        let mut p = std::env::temp_dir();
        p.push(format!("dsdout_{}_{}.tmp", name, std::process::id()));
        p
    }

    fn sine(freq: f64, amp: f64, rate: u32, ch: usize, secs: f64) -> Vec<f32> {
        let n = (rate as f64 * secs) as usize;
        let mut v = Vec::with_capacity(n * ch);
        for i in 0..n {
            let s = (2.0 * std::f64::consts::PI * freq * i as f64 / rate as f64).sin() * amp;
            for _ in 0..ch {
                v.push(s as f32);
            }
        }
        v
    }

    #[test]
    fn silence_is_about_half_ones() {
        let p = tmp("silence");
        let pcm = vec![0.0f32; 44100 * 2];
        encode_memory(&p, Container::Dsf, Tier::D64, 44100, 2, &pcm).unwrap();
        let mut buf = Vec::new();
        File::open(&p).unwrap().read_to_end(&mut buf).unwrap();
        let data = &buf[DSF_HEAD as usize..];
        let ones: u32 = data.iter().map(|b| b.count_ones()).sum();
        let ratio = ones as f64 / (data.len() * 8) as f64;
        assert!(ratio > 0.40 && ratio < 0.60, "静音时 1 的比例是 {ratio}");
        let _ = std::fs::remove_file(&p);
    }

    #[test]
    fn sine_survives_lowpass() {
        let p = tmp("sine");
        let pcm = sine(440.0, 0.5, 44100, 1, 1.0);
        encode_memory(&p, Container::Dsf, Tier::D64, 44100, 1, &pcm).unwrap();
        let mut buf = Vec::new();
        File::open(&p).unwrap().read_to_end(&mut buf).unwrap();
        let data = &buf[DSF_HEAD as usize..];
        let mut bits = Vec::with_capacity(data.len() * 8);
        for b in data {
            for i in 0..8 {
                bits.push(if b & (1 << i) != 0 { 1.0 } else { -1.0 });
            }
        }
        let n = bits.len() / 64;
        let mut low = Vec::with_capacity(n);
        for i in 0..n {
            let s: f64 = bits[i * 64..(i + 1) * 64].iter().sum();
            low.push(s / 64.0);
        }
        let power = |freq: f64| -> f64 {
            let w = 2.0 * std::f64::consts::PI * freq / 44100.0;
            let (mut re, mut im) = (0.0, 0.0);
            for (i, v) in low.iter().enumerate() {
                re += v * (w * i as f64).cos();
                im -= v * (w * i as f64).sin();
            }
            re * re + im * im
        };
        let p440 = power(440.0);
        let p5k = power(5000.0);
        assert!(p440 > p5k * 100.0, "440Hz={p440} 5kHz={p5k}");
        let _ = std::fs::remove_file(&p);
    }

    #[test]
    fn dsf_header_is_sane() {
        let p = tmp("dsf");
        let pcm = sine(1000.0, 0.3, 44100, 2, 0.1);
        let total = encode_memory(&p, Container::Dsf, Tier::D64, 44100, 2, &pcm).unwrap();
        let mut h = [0u8; 92];
        File::open(&p).unwrap().read_exact(&mut h).unwrap();
        assert_eq!(&h[0..4], b"DSD ");
        assert_eq!(u64::from_le_bytes(h[4..12].try_into().unwrap()), 28);
        assert_eq!(u64::from_le_bytes(h[12..20].try_into().unwrap()), total);
        assert_eq!(&h[28..32], b"fmt ");
        assert_eq!(u64::from_le_bytes(h[32..40].try_into().unwrap()), 52);
        assert_eq!(u32::from_le_bytes(h[48..52].try_into().unwrap()), 2);
        assert_eq!(u32::from_le_bytes(h[52..56].try_into().unwrap()), 2);
        assert_eq!(u32::from_le_bytes(h[56..60].try_into().unwrap()), 2_822_400);
        assert_eq!(u32::from_le_bytes(h[60..64].try_into().unwrap()), 1);
        assert_eq!(u64::from_le_bytes(h[64..72].try_into().unwrap()), 4410 * 64);
        assert_eq!(u32::from_le_bytes(h[72..76].try_into().unwrap()), 4096);
        assert_eq!(&h[80..84], b"data");
        assert_eq!(
            u64::from_le_bytes(h[84..92].try_into().unwrap()),
            total - 92 + 12
        );
        assert_eq!(std::fs::metadata(&p).unwrap().len(), total);
        let _ = std::fs::remove_file(&p);
    }

    #[test]
    fn dff_header_is_sane() {
        let p = tmp("dff");
        let pcm = sine(1000.0, 0.3, 44100, 2, 0.1);
        let total = encode_memory(&p, Container::Dff, Tier::D64, 44100, 2, &pcm).unwrap();
        let mut buf = Vec::new();
        File::open(&p).unwrap().read_to_end(&mut buf).unwrap();
        assert_eq!(&buf[0..4], b"FRM8");
        assert_eq!(u64::from_be_bytes(buf[4..12].try_into().unwrap()), total - 12);
        assert_eq!(&buf[12..16], b"DSD ");
        assert_eq!(&buf[16..20], b"FVER");
        assert_eq!(&buf[32..36], b"PROP");
        assert_eq!(&buf[44..48], b"SND ");
        let idx = buf.windows(4).position(|w| w == b"FS  ").unwrap();
        let rate = u32::from_be_bytes(buf[idx + 12..idx + 16].try_into().unwrap());
        assert_eq!(rate, 2_822_400);
        let off = dff_data_offset(2) as usize;
        assert_eq!(&buf[off - 12..off - 8], b"DSD ");
        assert_eq!(
            u64::from_be_bytes(buf[off - 8..off].try_into().unwrap()),
            total - off as u64
        );
        let _ = std::fs::remove_file(&p);
    }

    #[test]
    fn length_is_stable() {
        let pcm = sine(440.0, 0.4, 44100, 2, 0.2);
        let a = tmp("a");
        encode_memory(&a, Container::Dsf, Tier::D64, 44100, 2, &pcm).unwrap();
        let b = tmp("b");
        let mut w = DsdWriter::new(&b, Container::Dsf, Tier::D64, 44100, 2).unwrap();
        let frames = pcm.len() / 2;
        let third = frames / 3 * 2;
        w.push(&pcm[..third]).unwrap();
        w.push(&pcm[third..third * 2]).unwrap();
        w.push(&pcm[third * 2..]).unwrap();
        w.finish().unwrap();
        let la = std::fs::metadata(&a).unwrap().len();
        let lb = std::fs::metadata(&b).unwrap().len();
        assert_eq!(la, lb, "分块推送和一次性推送长度不一致");
        let _ = std::fs::remove_file(&a);
        let _ = std::fs::remove_file(&b);
    }

    #[test]
    fn mono_48k_dsd128() {
        let p = tmp("mono48");
        let pcm = sine(1000.0, 0.5, 48000, 1, 0.2);
        encode_memory(&p, Container::Dff, Tier::D128, 48000, 1, &pcm).unwrap();
        let mut buf = Vec::new();
        File::open(&p).unwrap().read_to_end(&mut buf).unwrap();
        let idx = buf.windows(4).position(|w| w == b"FS  ").unwrap();
        let rate = u32::from_be_bytes(buf[idx + 12..idx + 16].try_into().unwrap());
        assert_eq!(rate, 6_144_000);
        let _ = std::fs::remove_file(&p);
    }
}
