//! WASAPI 独占输出（Windows）。
//!
//! 为什么要单独一套：共享模式下信号要过 Windows 混音器、采样率被锁成混音格式，
//! 想要"按源采样率、不经过混音、整数直通"只能开独占。
//!
//! 这里的约定：
//! - 输出格式固定 **32-bit int / 2ch / 源采样率**；ffmpeg 也按 s32le 解码，
//!   队列里的字节原样写进声卡，不做音量、不做 ReplayGain（这就是"直通"）。
//! - 暂停时不推进播放位置，但仍然写静音，避免设备欠载。
//! - 独占被别的程序占用/设备不支持时，`start` 返回错误，上层退回共享输出。

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::channel;
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::Duration;

use anyhow::{anyhow, Context, Result};
use wasapi::{Direction, DeviceEnumerator, SampleType, StreamMode, WaveFormat};

use crate::Inner;

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

/// 启动独占输出。成功时返回 (后端, 实际采样率, 声道, 设备描述)。
///
/// 内部线程先协商格式、把结果发回来，主线程拿到采样率后再启动 ffmpeg 解码，
/// 避免"解码器按 44.1k 出、设备却锁在 48k"。
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
        let prepared = prepare(name.as_deref(), source_rate);
        let (label, rate, channels, client, render, bytes_per_frame) = match prepared {
            Ok(v) => v,
            Err(e) => {
                let _ = tx.send(Err(e));
                return;
            }
        };
        let _ = tx.send(Ok((rate, channels, label)));
        if client.start_stream().is_err() {
            return;
        }
        // 静音块：欠载/暂停时写它，保持设备不断流
        let silence = vec![0u8; 4096 * bytes_per_frame];
        loop {
            if stop_thread.load(Ordering::SeqCst) {
                break;
            }
            let space = match client.get_available_space_in_frames() {
                Ok(n) => n as usize,
                Err(_) => break,
            };
            if space == 0 {
                std::thread::sleep(Duration::from_millis(2));
                continue;
            }
            let frames = space.min(4096);
            let need = frames * bytes_per_frame;
            let playing = inner.playing.load(Ordering::Relaxed);
            let mut wrote = false;
            if playing {
                if let Ok(mut q) = inner.buf.lock() {
                    if q.len() >= need {
                        if render.write_to_device_from_deque(frames, &mut q, None).is_ok() {
                            inner.pos_frames.fetch_add(frames as u64, Ordering::Relaxed);
                            wrote = true;
                        }
                    } else if inner.eof.load(Ordering::Relaxed) && q.len() >= bytes_per_frame {
                        // 尾巴不够一整块：把剩下的写完，再用静音补齐，别让 finished() 永远等不到
                        let tail = q.len() / bytes_per_frame;
                        if render.write_to_device_from_deque(tail, &mut q, None).is_ok() {
                            let rest = (frames - tail) * bytes_per_frame;
                            let _ = render.write_to_device(&silence[..rest], None);
                            inner.pos_frames.fetch_add(tail as u64, Ordering::Relaxed);
                            wrote = true;
                        }
                    }
                }
            }
            if !wrote {
                // 暂停或数据还没到：写静音，不推进位置
                let _ = render.write_to_device(&silence[..need], None);
            }
        }
        let _ = client.stop_stream();
    });

    match rx.recv_timeout(Duration::from_secs(3)) {
        Ok(Ok((rate, channels, label))) => Ok((
            Out {
                stop,
                handle: Some(handle),
            },
            rate,
            channels,
            label,
        )),
        Ok(Err(e)) => {
            let _ = handle.join();
            Err(e)
        }
        Err(_) => {
            stop.store(true, Ordering::SeqCst);
            Err(anyhow!("独占输出初始化超时（设备可能被占用）"))
        }
    }
}

type Prepared = (
    String,
    u32,
    u16,
    wasapi::AudioClient,
    wasapi::AudioRenderClient,
    usize,
);

fn prepare(device_name: Option<&str>, source_rate: u32) -> Result<Prepared> {
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

    // 采样率：优先源采样率；设备不认就退到 48k / 44.1k
    let mut rates: Vec<u32> = Vec::new();
    if source_rate >= 8000 {
        rates.push(source_rate);
    }
    for r in [48_000u32, 44_100] {
        if !rates.contains(&r) {
            rates.push(r);
        }
    }

    let mut chosen: Option<(WaveFormat, u32)> = None;
    for rate in rates {
        // 32 位容器、32 或 24 个有效位都试试：不少板载声卡只认 24 位
        // （24 位有效位在 32 位容器里是左对齐的，和 ffmpeg s32le 一致）
        for valid in [32usize, 24] {
            let fmt = WaveFormat::new(32, valid, SampleType::Int, rate as usize, 2, None);
            if let Ok(fmt) = client.is_supported_exclusive_with_quirks(&fmt) {
                let r = fmt.get_samplespersec();
                chosen = Some((fmt, r));
                break;
            }
        }
        if chosen.is_some() {
            break;
        }
    }
    let (format, rate) = chosen.ok_or_else(|| {
        anyhow!("这台设备的独占模式不支持 32-bit 整数输出（可在设置里用共享输出）")
    })?;

    let (_def_period, min_period) = client.get_device_period().unwrap_or((10_000, 3_000));
    let period = client
        .calculate_aligned_period_near(min_period.max(3_000), Some(128), &format)
        .unwrap_or(min_period.max(3_000));
    let mode = StreamMode::PollingExclusive {
        buffer_duration_hns: period * 4,
        period_hns: period,
    };
    client
        .initialize_client(&format, Direction::Render, mode, None)
        .map_err(|e| anyhow!("独占初始化失败：{e}"))?;
    let render = client
        .get_audiorenderclient()
        .context("获取渲染客户端失败")?;

    let channels = format.get_nchannels();
    let bytes_per_frame = format.get_blockalign() as usize;
    let valid = format.get_validbitspersample();
    let mut label = format!(
        "{name} · 独占直通 · {} Hz · {}-bit int · {}ch",
        rate, valid, channels
    );
    if source_rate >= 8000 && rate != source_rate {
        label.push_str(&format!("（源 {} Hz 重采样）", source_rate));
    }
    Ok((label, rate, channels, client, render, bytes_per_frame))
}
