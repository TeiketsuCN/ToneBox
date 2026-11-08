//! fc-player —— 试听。
//!
//! 为什么要自己做：老 `.oma` 的文件名经常是 `001.oma`、`MGR_0001.oma`，
//! 不听一遍根本不知道它是什么。试听是这个产品的核心能力，不是附件。
//!
//! 为什么用 ffmpeg 解码而不是 symphonia/rodio：
//! rodio 解不了 ATRAC / OpenMG，而我们的目标用户手上全是这些。
//! 走 ffmpeg 子进程，所有能转的格式都能听，而且沿用了"崩溃隔离"的架构。
//!
//! 结构：
//!   ffmpeg（PCM f32 流） → 有界队列 → cpal 回调 → 声卡
//! 用有界队列做背压：解码比播放快时，解码线程会阻塞，不会撑爆内存。

pub mod devices;
#[cfg(windows)]
mod exclusive;

use std::collections::VecDeque;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::{anyhow, Result};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::SampleFormat;

#[cfg(windows)]
use std::os::windows::process::CommandExt;
#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// 缓冲上限：约 2 秒的立体声 48k **s32 字节**（4 字节/采样）。
/// 队列里统一存 s32le：共享输出再转 f32，独占输出直接原样写声卡（位完美）。
const MAX_QUEUE_BYTES: usize = 48_000 * 2 * 4 * 2;

struct Inner {
    buf: Mutex<VecDeque<u8>>,
    playing: AtomicBool,
    volume: AtomicU32, // f32 的位模式
    /// ReplayGain 的线性增益（1.0 = 不应用）
    gain: AtomicU32,
    /// 已播放的帧数（一帧 = 左右两个采样）
    pos_frames: AtomicU64,
    /// 已解码完毕
    eof: AtomicBool,
    channels: AtomicU32,
    sample_rate: AtomicU32,
}

impl Inner {
    fn new(sample_rate: u32, channels: u16, gain: f32) -> Self {
        Self {
            buf: Mutex::new(VecDeque::with_capacity(MAX_QUEUE_BYTES)),
            playing: AtomicBool::new(true),
            volume: AtomicU32::new(0.75f32.to_bits()),
            gain: AtomicU32::new(gain.to_bits()),
            pos_frames: AtomicU64::new(0),
            eof: AtomicBool::new(false),
            channels: AtomicU32::new(channels as u32),
            sample_rate: AtomicU32::new(sample_rate),
        }
    }
    fn set_format(&self, sample_rate: u32, channels: u16) {
        self.sample_rate.store(sample_rate, Ordering::SeqCst);
        self.channels.store(channels as u32, Ordering::SeqCst);
    }
    fn volume(&self) -> f32 {
        f32::from_bits(self.volume.load(Ordering::Relaxed))
    }
    fn gain(&self) -> f32 {
        f32::from_bits(self.gain.load(Ordering::Relaxed))
    }
    fn position_s(&self) -> f64 {
        self.pos_frames.load(Ordering::Relaxed) as f64
            / self.sample_rate.load(Ordering::Relaxed).max(1) as f64
    }
}

/// 打开播放器时的选项（设备 / 采样率跟随 / ReplayGain）。
#[derive(Clone, Debug, Default)]
pub struct PlayOptions {
    /// WASAPI 友好名；None / 空 = 系统默认输出
    pub device_name: Option<String>,
    /// 源文件采样率。>0 时尽量按它开流（共享模式下不保证能改，失败会退回设备默认）
    pub source_rate: u32,
    /// ReplayGain 曲目增益（dB）
    pub replaygain_db: Option<f32>,
    /// 独占输出（还没接）
    pub exclusive: bool,
}

impl PlayOptions {
    fn gain(&self) -> f32 {
        match self.replaygain_db {
            Some(db) if db.is_finite() => 10f32.powf(db / 20.0).clamp(0.05, 8.0),
            _ => 1.0,
        }
    }
}

/// 输出后端：共享（cpal）或独占（WASAPI 直通）。
enum Output {
    Shared(cpal::Stream),
    #[cfg(windows)]
    Exclusive(exclusive::Out),
}

pub struct Player {
    inner: Arc<Inner>,
    output: Output,
    ffmpeg: PathBuf,
    path: PathBuf,
    /// 解码线程代号：seek 时 +1，老线程发现自己过期就退出
    generation: Arc<AtomicU64>,
    my_gen: u64,
    duration_s: f64,
    paused: bool,
    /// 当前实际用的输出（设备名 · 采样率 · 声道 · 是否 ReplayGain）
    output_label: String,
}

/// 按给定格式建 cpal 输出流；失败交给调用方决定是否退回默认格式。
fn build_stream(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    sample_format: SampleFormat,
    inner: Arc<Inner>,
    err: fn(cpal::StreamError),
) -> Result<cpal::Stream> {
    Ok(match sample_format {
        SampleFormat::F32 => device.build_output_stream(
            config,
            move |data: &mut [f32], _| fill_f32(&inner, data),
            err,
            None,
        )?,
        SampleFormat::I16 => device.build_output_stream(
            config,
            move |data: &mut [i16], _| {
                let mut tmp = vec![0f32; data.len()];
                fill_f32(&inner, &mut tmp);
                for (d, s) in data.iter_mut().zip(tmp) {
                    *d = (s.clamp(-1.0, 1.0) * 32767.0) as i16;
                }
            },
            err,
            None,
        )?,
        other => return Err(anyhow!("不支持的输出采样格式: {other:?}")),
    })
}

/// 设备在 `want` 这个采样率下有没有 2 声道的输出配置。
/// 共享模式 Windows 不保证接受（只认混音格式），所以调用方要能退回默认。
fn pick_config(device: &cpal::Device, want: u32) -> Option<cpal::SupportedStreamConfig> {
    let ranges = device.supported_output_configs().ok()?;
    for r in ranges {
        if r.channels() < 2 {
            continue;
        }
        // 只挑我们能喂的采样格式：U8/I32 这些没写转换，别选进来
        if !matches!(r.sample_format(), SampleFormat::F32 | SampleFormat::I16) {
            continue;
        }
        if r.min_sample_rate().0 <= want && want <= r.max_sample_rate().0 {
            return Some(r.with_sample_rate(cpal::SampleRate(want)));
        }
    }
    None
}

impl Player {
    /// 打开一个文件准备试听。会先探测声卡的实际输出格式，让 ffmpeg 直接按它解码。
    pub fn open(ffmpeg: &Path, path: &Path, duration_s: f64) -> Result<Self> {
        Self::open_with(ffmpeg, path, duration_s, &PlayOptions::default())
    }

    /// 带设备 / 采样率跟随 / ReplayGain / 独占的打开方式。
    pub fn open_with(
        ffmpeg: &Path,
        path: &Path,
        duration_s: f64,
        opts: &PlayOptions,
    ) -> Result<Self> {
        #[cfg(windows)]
        if opts.exclusive {
            // 独占：s32 字节直通，忽略音量/ReplayGain（位完美优先）
            let inner = Arc::new(Inner::new(opts.source_rate.max(44_100), 2, 1.0));
            match exclusive::start(opts.device_name.as_deref(), opts.source_rate, inner.clone()) {
                Ok((out, rate, ch, label)) => {
                    inner.set_format(rate, ch);
                    let mut p = Player {
                        inner,
                        output: Output::Exclusive(out),
                        ffmpeg: ffmpeg.to_path_buf(),
                        path: path.to_path_buf(),
                        generation: Arc::new(AtomicU64::new(0)),
                        my_gen: 0,
                        duration_s,
                        paused: false,
                        output_label: label,
                    };
                    p.spawn_decoder(0.0);
                    return Ok(p);
                }
                Err(e) => {
                    // 独占被占用/不支持：退回共享，并把原因写进输出描述
                    let note = format!("独占不可用（{e}）· 已退回共享");
                    return Self::open_shared(ffmpeg, path, duration_s, opts, Some(note));
                }
            }
        }
        Self::open_shared(ffmpeg, path, duration_s, opts, None)
    }

    /// 共享输出（cpal）：原来的播放路径。
    fn open_shared(
        ffmpeg: &Path,
        path: &Path,
        duration_s: f64,
        opts: &PlayOptions,
        note: Option<String>,
    ) -> Result<Self> {
        let host = cpal::default_host();
        let mut device = host
            .default_output_device()
            .ok_or_else(|| anyhow!("没有音频输出设备"))?;
        // 选设备：按名字找；找不到就退回默认（设备被拔掉是常事，不该因此播不了）
        let mut device_label = device.name().unwrap_or_else(|_| "默认输出".into());
        if let Some(want) = opts.device_name.as_deref().filter(|s| !s.is_empty()) {
            if let Ok(list) = host.output_devices() {
                if let Some(d) = list
                    .into_iter()
                    .find(|d| d.name().map(|n| n == want).unwrap_or(false))
                {
                    device_label = want.to_string();
                    device = d;
                }
            }
        }

        // 采样率跟随：源文件 44.1k 就别无谓地上 48k。
        // 共享模式下 Windows 只认混音格式，这里失败就退回设备默认，不报错。
        let default_cfg = device.default_output_config()?;
        let mut supported = default_cfg.clone();
        if opts.source_rate >= 8000 {
            if let Some(cfg) = pick_config(&device, opts.source_rate) {
                supported = cfg;
            }
        }
        let mut config: cpal::StreamConfig = supported.clone().into();
        let mut inner = Arc::new(Inner::new(
            config.sample_rate.0,
            config.channels,
            opts.gain(),
        ));

        let mut sample_format = supported.sample_format();
        let err_fn = |e: cpal::StreamError| eprintln!("音频输出错误: {e}");

        let mut stream = match build_stream(&device, &config, sample_format, inner.clone(), err_fn) {
            Ok(s) => s,
            Err(e) => {
                // 按源采样率开流失败（共享模式很常见）：退回设备混音格式再来一次
                if config.sample_rate != default_cfg.sample_rate()
                    || config.channels != default_cfg.channels()
                    || sample_format != default_cfg.sample_format()
                {
                    config = default_cfg.clone().into();
                    sample_format = default_cfg.sample_format();
                    inner = Arc::new(Inner::new(
                        config.sample_rate.0,
                        config.channels,
                        opts.gain(),
                    ));
                    build_stream(&device, &config, sample_format, inner.clone(), err_fn)
                        .map_err(|_| e)?
                } else {
                    return Err(e);
                }
            }
        };
        stream.play()?;

        let gain_on = (opts.gain() - 1.0).abs() > 0.001;
        let mut output_label = format!(
            "{device_label} · {} kHz · {}ch · {}",
            config.sample_rate.0 as f32 / 1000.0,
            config.channels,
            if gain_on { "ReplayGain 开" } else { "直通" }
        );
        if let Some(note) = note {
            output_label = format!("{note} · {output_label}");
        }

        let generation = Arc::new(AtomicU64::new(0));
        let mut p = Player {
            inner,
            output: Output::Shared(stream),
            ffmpeg: ffmpeg.to_path_buf(),
            path: path.to_path_buf(),
            generation,
            my_gen: 0,
            duration_s,
            paused: false,
            output_label,
        };
        p.spawn_decoder(0.0);
        Ok(p)
    }

    fn spawn_decoder(&mut self, start_s: f64) {
        self.my_gen += 1;
        let my_gen = self.my_gen;
        self.generation.store(my_gen, Ordering::SeqCst);

        let inner = self.inner.clone();
        let gen_shared = self.generation.clone();
        let ffmpeg = self.ffmpeg.clone();
        let path = self.path.clone();
        let ch = inner.channels.load(Ordering::Relaxed).to_string();
        let sr = inner.sample_rate.load(Ordering::Relaxed).to_string();

        inner.eof.store(false, Ordering::SeqCst);
        {
            let mut q = inner.buf.lock().unwrap();
            q.clear();
        }
        inner.pos_frames.store(
            (start_s * inner.sample_rate.load(Ordering::Relaxed) as f64) as u64,
            Ordering::SeqCst,
        );

        std::thread::spawn(move || {
            let mut cmd = Command::new(&ffmpeg);
            // -ss 放在 -i 前：快速定位，够用
            cmd.args(["-v", "error", "-nostdin"])
                .arg("-ss")
                .arg(format!("{start_s:.3}"))
                .arg("-i")
                .arg(&path)
                .args(["-map", "0:a:0", "-ac", &ch, "-ar", &sr])
                .args(["-f", "s32le", "-"])
                .stdout(Stdio::piped())
                .stderr(Stdio::null());
            #[cfg(windows)]
            cmd.creation_flags(CREATE_NO_WINDOW);

            let Ok(mut child) = cmd.spawn() else { return };
            let Some(mut out) = child.stdout.take() else {
                return;
            };

            let mut raw = [0u8; 16384];
            loop {
                if gen_shared.load(Ordering::SeqCst) != my_gen {
                    break;
                }
                match out.read(&mut raw) {
                    Ok(0) => {
                        inner.eof.store(true, Ordering::SeqCst);
                        break;
                    }
                    Ok(n) => {
                        let n = n - (n % 4);
                        let mut q = inner.buf.lock().unwrap();
                        // 直接压 s32le 原始字节：独占输出要原样写给声卡
                        q.extend(&raw[..n]);
                        // 背压：满了就放掉锁睡一会儿，绝不阻塞声卡回调
                        while q.len() > MAX_QUEUE_BYTES && gen_shared.load(Ordering::SeqCst) == my_gen {
                            drop(q);
                            std::thread::sleep(Duration::from_millis(8));
                            q = inner.buf.lock().unwrap();
                        }
                    }
                    Err(_) => break,
                }
            }
            let _ = child.kill();
            let _ = child.wait();
        });
    }

    pub fn play(&mut self) {
        if self.paused {
            self.paused = false;
            self.inner.playing.store(true, Ordering::Relaxed);
        }
    }

    pub fn pause(&mut self) {
        self.paused = true;
        self.inner.playing.store(false, Ordering::Relaxed);
    }

    /// 停止并归零：下次按播放从头开始
    pub fn stop(&mut self) {
        self.pause();
        self.spawn_decoder(0.0);
    }

    pub fn toggle(&mut self) {
        if self.paused {
            self.play();
        } else {
            self.pause();
        }
    }

    pub fn is_playing(&self) -> bool {
        !self.paused
    }

    pub fn seek(&mut self, to_s: f64) {
        let to = to_s.clamp(0.0, self.duration_s.max(0.0));
        let was_playing = !self.paused;
        self.spawn_decoder(to);
        if was_playing {
            self.inner.playing.store(true, Ordering::Relaxed);
        }
    }

    pub fn set_volume(&self, v: f32) {
        self.inner
            .volume
            .store(v.clamp(0.0, 1.0).to_bits(), Ordering::Relaxed);
    }

    pub fn volume(&self) -> f32 {
        self.inner.volume()
    }

    pub fn position_s(&self) -> f64 {
        self.inner.position_s()
    }

    pub fn duration_s(&self) -> f64 {
        self.duration_s
    }

    /// 当前输出的一句话描述（给"编解码"页/播放栏看）
    pub fn output_label(&self) -> &str {
        &self.output_label
    }

    pub fn finished(&self) -> bool {
        self.inner.eof.load(Ordering::Relaxed)
            && self.inner.buf.lock().map(|q| q.is_empty()).unwrap_or(true)
    }
}

fn fill_f32(inner: &Inner, data: &mut [f32]) {
    if !inner.playing.load(Ordering::Relaxed) {
        data.fill(0.0);
        return;
    }
    let vol = inner.volume();
    let gain = inner.gain();
    let Ok(mut q) = inner.buf.lock() else {
        data.fill(0.0);
        return;
    };
    for s in data.iter_mut() {
        let mut b = [0u8; 4];
        for v in b.iter_mut() {
            *v = q.pop_front().unwrap_or(0);
        }
        let raw = i32::from_le_bytes(b);
        *s = (raw as f32 / 2_147_483_648.0 * vol * gain).clamp(-1.0, 1.0);
    }
    let frames = data.len() / inner.channels.load(Ordering::Relaxed).max(1) as usize;
    inner
        .pos_frames
        .fetch_add(frames as u64, Ordering::Relaxed);
}

impl Drop for Player {
    fn drop(&mut self) {
        // 让解码线程自杀
        self.generation.fetch_add(1, Ordering::SeqCst);
        self.inner.playing.store(false, Ordering::Relaxed);
        match &mut self.output {
            Output::Shared(stream) => {
                let _ = stream.pause();
            }
            #[cfg(windows)]
            Output::Exclusive(out) => out.stop(),
        }
    }
}
