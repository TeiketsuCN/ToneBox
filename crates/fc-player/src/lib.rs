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

/// 缓冲上限：约 2 秒的立体声 48k f32
const MAX_QUEUE: usize = 48_000 * 2 * 2;

struct Inner {
    buf: Mutex<VecDeque<f32>>,
    playing: AtomicBool,
    volume: AtomicU32, // f32 的位模式
    /// 已播放的帧数（一帧 = 左右两个采样）
    pos_frames: AtomicU64,
    /// 已解码完毕
    eof: AtomicBool,
    channels: u16,
    sample_rate: u32,
}

impl Inner {
    fn volume(&self) -> f32 {
        f32::from_bits(self.volume.load(Ordering::Relaxed))
    }
    fn position_s(&self) -> f64 {
        self.pos_frames.load(Ordering::Relaxed) as f64 / self.sample_rate as f64
    }
}

pub struct Player {
    inner: Arc<Inner>,
    stream: cpal::Stream,
    ffmpeg: PathBuf,
    path: PathBuf,
    /// 解码线程代号：seek 时 +1，老线程发现自己过期就退出
    generation: Arc<AtomicU64>,
    my_gen: u64,
    duration_s: f64,
    paused: bool,
}

impl Player {
    /// 打开一个文件准备试听。会先探测声卡的实际输出格式，让 ffmpeg 直接按它解码。
    pub fn open(ffmpeg: &Path, path: &Path, duration_s: f64) -> Result<Self> {
        let host = cpal::default_host();
        let device = host
            .default_output_device()
            .ok_or_else(|| anyhow!("没有音频输出设备"))?;
        let supported = device.default_output_config()?;
        let sample_format = supported.sample_format();
        let config: cpal::StreamConfig = supported.into();
        let channels = config.channels;
        let sample_rate = config.sample_rate.0;

        let inner = Arc::new(Inner {
            buf: Mutex::new(VecDeque::with_capacity(MAX_QUEUE)),
            playing: AtomicBool::new(true),
            volume: AtomicU32::new(0.75f32.to_bits()),
            pos_frames: AtomicU64::new(0),
            eof: AtomicBool::new(false),
            channels,
            sample_rate,
        });

        let cb_inner = inner.clone();
        let err_fn = |e: cpal::StreamError| eprintln!("音频输出错误: {e}");

        let stream = match sample_format {
            SampleFormat::F32 => device.build_output_stream(
                &config,
                move |data: &mut [f32], _| fill_f32(&cb_inner, data),
                err_fn,
                None,
            )?,
            SampleFormat::I16 => device.build_output_stream(
                &config,
                move |data: &mut [i16], _| {
                    let mut tmp = vec![0f32; data.len()];
                    fill_f32(&cb_inner, &mut tmp);
                    for (d, s) in data.iter_mut().zip(tmp) {
                        *d = (s.clamp(-1.0, 1.0) * 32767.0) as i16;
                    }
                },
                err_fn,
                None,
            )?,
            other => return Err(anyhow!("不支持的输出采样格式: {other:?}")),
        };
        stream.play()?;

        let generation = Arc::new(AtomicU64::new(0));
        let mut p = Player {
            inner,
            stream,
            ffmpeg: ffmpeg.to_path_buf(),
            path: path.to_path_buf(),
            generation,
            my_gen: 0,
            duration_s,
            paused: false,
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
        let ch = inner.channels.to_string();
        let sr = inner.sample_rate.to_string();

        inner.eof.store(false, Ordering::SeqCst);
        {
            let mut q = inner.buf.lock().unwrap();
            q.clear();
        }
        inner.pos_frames.store(
            (start_s * inner.sample_rate as f64) as u64,
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
                .args(["-f", "f32le", "-"])
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
                        let samples: Vec<f32> = raw[..n]
                            .chunks_exact(4)
                            .map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
                            .collect();
                        let mut q = inner.buf.lock().unwrap();
                        q.extend(samples);
                        // 背压：满了就放掉锁睡一会儿，绝不阻塞声卡回调
                        while q.len() > MAX_QUEUE && gen_shared.load(Ordering::SeqCst) == my_gen {
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
    let Ok(mut q) = inner.buf.lock() else {
        data.fill(0.0);
        return;
    };
    for s in data.iter_mut() {
        *s = q.pop_front().unwrap_or(0.0) * vol;
    }
    let frames = data.len() / inner.channels.max(1) as usize;
    inner
        .pos_frames
        .fetch_add(frames as u64, Ordering::Relaxed);
}

impl Drop for Player {
    fn drop(&mut self) {
        // 让解码线程自杀
        self.generation.fetch_add(1, Ordering::SeqCst);
        self.inner.playing.store(false, Ordering::Relaxed);
        let _ = self.stream.pause();
    }
}
