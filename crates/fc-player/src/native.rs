//! 自研播放引擎（Windows）。
//!
//! 和「WASAPI 共享」的区别：那条路是 cpal 的流，我们只往里塞数据；
//! 这条路从解码出来的 s32 开始就是自己管的：
//!
//!   ffmpeg → s32 队列 → 转 f32 → **自己的窗函数 sinc 重采样** → 自己的环形缓冲
//!          → 直接调 WASAPI 共享模式写设备
//!
//! 为什么要自己重采样：源 44.1k、设备混音 48k 时，谁都得转一次。cpal 那条路
//! 是让 Windows 去转（或者干脆按设备采样率丢给系统），这条路由我们决定
//! 转换质量（32 抽头窗函数 sinc），以后要接无缝播放/DSD 也在这条线上做。

use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::channel;
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::Duration;

use anyhow::{anyhow, Context, Result};
use wasapi::{Direction, DeviceEnumerator, SampleType, StreamMode, WaveFormat};

use crate::Inner;

// ── 重采样 ──────────────────────────────────────────────
/// 立体声窗函数 sinc 重采样（块处理，跨块保留历史样本）。
pub struct Resampler {
    /// 输出/输入 采样率比
    ratio: f64,
    /// 单边抽头数（总抽头 = 2 * half）
    half: usize,
    /// 抗混叠截止（相对奈奎斯特）
    cutoff: f64,
    /// 上一块末尾的输入样本
    hist: Vec<[f32; 2]>,
    /// 下一个输出样本的位置（相对 hist 起点，单位：输入样本）
    frac: f64,
}

impl Resampler {
    pub fn new(in_rate: u32, out_rate: u32) -> Self {
        let ratio = if in_rate > 0 {
            out_rate as f64 / in_rate as f64
        } else {
            1.0
        };
        // 降采样时要压低截止，避免混叠；升采样截止保持在 0.95
        let cutoff = 0.95 * if ratio < 1.0 { ratio } else { 1.0 };
        Self {
            ratio,
            half: 16,
            cutoff,
            hist: Vec::new(),
            frac: 0.0,
        }
    }

    pub fn ratio(&self) -> f64 {
        self.ratio
    }

    /// 输入一块立体声，输出对应长度（≈ block.len() * ratio）的立体声
    pub fn process(&mut self, block: &[[f32; 2]]) -> Vec<[f32; 2]> {
        if block.is_empty() {
            return Vec::new();
        }
        let h = self.half;
        let mut cat: Vec<[f32; 2]> = Vec::with_capacity(self.hist.len() + block.len());
        cat.extend_from_slice(&self.hist);
        cat.extend_from_slice(block);
        let n = cat.len();
        let hist_len = self.hist.len();

        let mut out = Vec::with_capacity((block.len() as f64 * self.ratio) as usize + 4);
        let mut p = self.frac;
        // 需要 p 右边还有 h 个样本可用
        let limit = n as f64 - 1.0 - h as f64;
        while p <= limit {
            let i = p.floor() as isize;
            let f = p - i as f64;
            let mut acc = [0.0f32; 2];
            let mut wsum = 0.0f32;
            for j in -(h as isize) + 1..=h as isize {
                let idx = i + j;
                if idx < 0 || idx as usize >= n {
                    continue;
                }
                let t = idx as f64 - p; // 与输出点的距离（输入样本单位）
                let x = self.cutoff * t;
                let sinc = if x.abs() < 1e-9 {
                    1.0
                } else {
                    let px = std::f64::consts::PI * x;
                    px.sin() / px
                };
                let w = 0.5 * (1.0 + (std::f64::consts::PI * t / h as f64).cos());
                let c = (self.cutoff * sinc * w) as f32;
                let s = cat[idx as usize];
                acc[0] += s[0] * c;
                acc[1] += s[1] * c;
                wsum += c;
            }
            if wsum.abs() > 1e-6 {
                acc[0] /= wsum;
                acc[1] /= wsum;
            }
            out.push(acc);
            p += 1.0 / self.ratio;
        }

        // 保留最后 h 个输入样本做下一块的左边上下文
        let keep = h.min(n);
        self.hist = cat[n - keep..].to_vec();
        // 把位置换算到新的 hist 起点
        self.frac = p - (n - keep) as f64;
        if self.frac < 0.0 {
            self.frac = 0.0;
        }
        let _ = hist_len;
        out
    }
}

// ── 输出后端 ────────────────────────────────────────────
pub struct Out {
    stop: Arc<AtomicBool>,
    handle: Option<JoinHandle<()>>,
}

impl Out {
    pub fn stop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
    }
}

impl Drop for Out {
    fn drop(&mut self) {
        self.stop();
    }
}

/// 打开自研引擎。返回 (后端, 设备采样率, 声道, 描述)
pub fn start(
    device_name: Option<&str>,
    source_rate: u32,
    inner: Arc<Inner>,
) -> Result<(Out, u32, u16, String)> {
    let (tx, rx) = channel::<Result<(u32, u16, String)>>();
    let stop = Arc::new(AtomicBool::new(false));
    let stop_thread = stop.clone();
    let name = device_name.map(|s| s.to_string());

    let handle = std::thread::spawn(move || {
        if let Err(e) = run(name.as_deref(), source_rate, inner, stop_thread, &tx) {
            let _ = tx.send(Err(e));
        }
    });

    match rx.recv_timeout(Duration::from_secs(5)) {
        Ok(Ok((rate, ch, label))) => Ok((
            Out {
                stop,
                handle: Some(handle),
            },
            rate,
            ch,
            label,
        )),
        Ok(Err(e)) => {
            let _ = handle.join();
            Err(e)
        }
        Err(_) => {
            stop.store(true, Ordering::SeqCst);
            Err(anyhow!("自研引擎初始化超时"))
        }
    }
}

fn run(
    device_name: Option<&str>,
    source_rate: u32,
    inner: Arc<Inner>,
    stop: Arc<AtomicBool>,
    tx: &std::sync::mpsc::Sender<Result<(u32, u16, String)>>,
) -> Result<()> {
    let _ = wasapi::initialize_mta();
    let enumerator = DeviceEnumerator::new().context("创建 WASAPI 枚举器失败")?;
    let device = match device_name.filter(|s| !s.is_empty()) {
        Some(want) => {
            let coll = enumerator
                .get_device_collection(Direction::Render)
                .context("枚举输出设备失败")?;
            let mut found = None;
            for d in &coll {
                if let Ok(d) = d {
                    if d.get_friendlyname().map(|n| n == want).unwrap_or(false) {
                        found = Some(d);
                        break;
                    }
                }
            }
            found
                .or_else(|| enumerator.get_default_device(Direction::Render).ok())
                .ok_or_else(|| anyhow!("找不到输出设备 {want}"))?
        }
        None => enumerator
            .get_default_device(Direction::Render)
            .context("没有默认输出设备")?,
    };
    let name = device.get_friendlyname().unwrap_or_else(|_| "输出设备".into());
    let mut client = device.get_iaudioclient().context("打开音频客户端失败")?;

    // 直接用设备混音格式（一般是 48k / f32 / 2ch）
    let format = client.get_mixformat().context("读设备混音格式失败")?;
    let rate = format.get_samplespersec();
    let channels = format.get_nchannels();
    let block_align = format.get_blockalign() as usize;

    client
        .initialize_client(
            &format,
            Direction::Render,
            StreamMode::PollingShared {
                autoconvert: false,
                buffer_duration_hns: 100_000,
            },
            None,
        )
        .map_err(|e| anyhow!("共享模式初始化失败：{e}"))?;
    let render = client.get_audiorenderclient().context("获取渲染客户端失败")?;

    let mut resampler = Resampler::new(source_rate.max(8_000), rate);
    // 设备侧的待写帧（交错 f32）
    let mut pending: VecDeque<[f32; 2]> = VecDeque::new();

    client.start_stream().context("启动输出流失败")?;

    let label = format!(
        "{name} · 自研引擎 · 源 {} Hz → 设备 {} Hz · f32 · {}ch",
        source_rate, rate, channels
    );
    let _ = tx.send(Ok((rate, channels, label)));

    let silence = vec![0u8; 4096 * block_align];
    let mut scratch: Vec<u8> = Vec::with_capacity(4096 * 8);

    while !stop.load(Ordering::SeqCst) {
        let space = match client.get_available_space_in_frames() {
            Ok(n) => n as usize,
            Err(_) => break,
        };
        if space == 0 {
            std::thread::sleep(Duration::from_millis(2));
            continue;
        }
        let want = space.min(1024);

        // 攒够设备帧：从队列取 s32 → f32 → 重采样
        while pending.len() < want {
            let playing = inner.playing.load(Ordering::Relaxed);
            if !playing {
                break;
            }
            let mut src: Vec<[f32; 2]> = Vec::with_capacity(1024);
            {
                let Ok(mut q) = inner.buf.lock() else { break };
                let avail = q.len() / 8;
                if avail == 0 {
                    break;
                }
                let take = avail.min(1024);
                for _ in 0..take {
                    let mut b = [0u8; 8];
                    for slot in b.iter_mut() {
                        *slot = q.pop_front().unwrap_or(0);
                    }
                    let l = i32::from_le_bytes([b[0], b[1], b[2], b[3]]);
                    let r = i32::from_le_bytes([b[4], b[5], b[6], b[7]]);
                    src.push([l as f32 / 2147483648.0, r as f32 / 2147483648.0]);
                }
            }
            if src.is_empty() {
                break;
            }
            // 位置按"消耗的源帧"记，播放进度才和源时长对得上
            inner
                .pos_frames
                .fetch_add(src.len() as u64, Ordering::Relaxed);
            let out = resampler.process(&src);
            pending.extend(out);
        }

        let frames = want.min(pending.len());
        scratch.clear();
        for _ in 0..frames {
            if let Some(f) = pending.pop_front() {
                scratch.extend_from_slice(&f[0].to_le_bytes());
                scratch.extend_from_slice(&f[1].to_le_bytes());
            }
        }
        if frames > 0 {
            if render.write_to_device(&scratch, None).is_err() {
                break;
            }
        }
        if frames < want {
            let rest = (want - frames) * block_align;
            let _ = render.write_to_device(&silence[..rest.min(silence.len())], None);
        }
    }

    let _ = client.stop_stream();
    Ok(())
}

/// 只给单测用：把一段 mono 正弦重采样，返回输出
#[cfg(test)]
pub(crate) fn resample_test_signal(in_rate: u32, out_rate: u32, freq: f64, secs: f64) -> Vec<f32> {
    let n = (in_rate as f64 * secs) as usize;
    let block: Vec<[f32; 2]> = (0..n)
        .map(|i| {
            let t = i as f64 / in_rate as f64;
            let v = (2.0 * std::f64::consts::PI * freq * t).sin() as f32;
            [v, v]
        })
        .collect();
    let mut r = Resampler::new(in_rate, out_rate);
    // 分块喂，模拟真实的分块处理
    let mut out = Vec::new();
    for chunk in block.chunks(512) {
        for f in r.process(chunk) {
            out.push(f[0]);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn zero_crossings(v: &[f32]) -> usize {
        v.windows(2)
            .filter(|w| (w[0] <= 0.0 && w[1] > 0.0) || (w[0] >= 0.0 && w[1] < 0.0))
            .count()
    }

    #[test]
    fn upsample_keeps_pitch_and_length() {
        let out = resample_test_signal(44_100, 48_000, 1000.0, 1.0);
        // 长度 ≈ 48000（分块边界会差几个样本）
        assert!(
            (out.len() as i64 - 48_000).abs() < 64,
            "长度 {} 偏离 48000",
            out.len()
        );
        // 1 kHz 正弦：1 秒应有 ~2000 次过零
        let zc = zero_crossings(&out[1000..out.len() - 1000]);
        let secs = (out.len() - 2000) as f64 / 48_000.0;
        let freq = zc as f64 / 2.0 / secs;
        assert!(
            (freq - 1000.0).abs() < 30.0,
            "输出频率 {freq:.1} Hz，应该接近 1000"
        );
    }

    #[test]
    fn downsample_keeps_pitch() {
        let out = resample_test_signal(48_000, 44_100, 1000.0, 1.0);
        assert!(
            (out.len() as i64 - 44_100).abs() < 64,
            "长度 {} 偏离 44100",
            out.len()
        );
        let zc = zero_crossings(&out[1000..out.len() - 1000]);
        let secs = (out.len() - 2000) as f64 / 44_100.0;
        let freq = zc as f64 / 2.0 / secs;
        assert!((freq - 1000.0).abs() < 30.0, "输出频率 {freq:.1} Hz");
    }

    #[test]
    fn passthrough_when_rates_equal() {
        let out = resample_test_signal(48_000, 48_000, 440.0, 0.5);
        assert!((out.len() as i64 - 24_000).abs() < 64, "长度 {}", out.len());
    }
}
