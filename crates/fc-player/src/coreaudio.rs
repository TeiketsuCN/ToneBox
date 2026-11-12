//! Core Audio 引擎。
//!
//! 先说清楚事实：**Apple 的 Core Audio 是闭源的、macOS/iOS 独占**，没有能在
//! Windows 上"装一个"的开源实现。开源的只有跨平台封装（cpal / JUCE / PortAudio /
//! miniaudio），它们各自在 macOS 上调用 Core Audio。
//!
//! 所以这里的做法是：
//! - **macOS 构建**：走 `cpal` 的 coreaudio 后端，就是真正的 Core Audio
//!   （`Engine::CoreAudio` 在 mac 上直接映射到共享输出那条路）；
//! - **Windows 构建**：做一层**兼容层**——把 Core Audio 的对象模型
//!   （AudioObject / AudioStreamBasicDescription 那套形状）映射到 WASAPI，
//!   让上层代码用同一种方式问"默认输出是什么、格式是什么"。
//!   声音上它仍然是 WASAPI，界面上也照实写「兼容层（底层 WASAPI）」。
//!
//! 这样做的好处：引擎页真的能点亮这一项、能出声、代码结构也统一；
//! 坏处只有一个——在 Windows 上它**不是** Apple 的实现，我们不假装它是。

/// Core Audio 的 `AudioStreamBasicDescription` 形状（只保留用得到的字段）
#[derive(Clone, Copy, Debug, Default)]
pub struct Asbd {
    /// 采样率（Hz）
    pub sample_rate: f64,
    /// 格式 ID：'lpcm'
    pub format_id: u32,
    /// 声道数
    pub channels: u32,
    /// 每声道位深
    pub bits: u32,
    /// 一帧字节数
    pub bytes_per_frame: u32,
}

impl Asbd {
    fn lpcm(sample_rate: f64, channels: u32, bits: u32) -> Self {
        Self {
            sample_rate,
            format_id: 0x6C70_636D, // 'lpcm'
            channels,
            bits,
            bytes_per_frame: channels * bits / 8,
        }
    }

    pub fn format_id_str(&self) -> &'static str {
        if self.format_id == 0x6C70_636D {
            "lpcm"
        } else {
            "?"
        }
    }

    pub fn summary(&self) -> String {
        format!(
            "{} Hz · {}ch · {}-bit int · {}",
            self.sample_rate as u32,
            self.channels,
            self.bits,
            self.format_id_str()
        )
    }
}

/// 这一项在当前平台上到底是什么
pub fn platform_note() -> &'static str {
    if cfg!(target_os = "macos") {
        "Apple 的系统音频层（真 Core Audio）"
    } else if cfg!(windows) {
        "兼容层：Core Audio 的对象模型 → WASAPI（不是 Apple 的实现）"
    } else {
        "兼容层：底下是 ALSA/PulseAudio"
    }
}

/// 引擎页显示的一句话状态
pub fn status_line() -> String {
    match snapshot() {
        Some((dev, asbd)) => format!("{}：{} · {}", platform_note(), dev, asbd.summary()),
        None => format!("{}：查不到默认输出", platform_note()),
    }
}

/// 按 Core Audio 的方式问一次"默认输出设备和它的流格式"
pub fn snapshot() -> Option<(String, Asbd)> {
    #[cfg(windows)]
    {
        use wasapi::Direction;
        let _ = wasapi::initialize_mta();
        let en = wasapi::DeviceEnumerator::new().ok()?;
        let dev = en.get_default_device(Direction::Render).ok()?;
        let name = dev.get_friendlyname().unwrap_or_else(|_| "默认输出".into());
        let mut client = dev.get_iaudioclient().ok()?;
        let fmt = client.get_mixformat().ok()?;
        let rate = fmt.get_samplespersec();
        let ch = fmt.get_nchannels();
        let bits = fmt.get_validbitspersample().max(fmt.get_bitspersample()) as u32;
        // WASAPI 混音格式是 float，这里按 Core Audio 的 lpcm 口径报 32 位
        return Some((name, Asbd::lpcm(rate as f64, ch as u32, 32.min(bits.max(16)))));
    }
    #[cfg(not(windows))]
    {
        // 非 Windows 上交给 cpal 的默认 host（macOS 就是 Core Audio）
        let host = cpal::default_host();
        let dev = host.default_output_device()?;
        let name = dev.name().ok()?;
        let cfg = dev.default_output_config().ok()?;
        let rate = cfg.sample_rate().0 as f64;
        let ch = cfg.channels() as u32;
        Some((name, Asbd::lpcm(rate, ch, 32)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn asbd_fields_are_consistent() {
        let a = Asbd::lpcm(48_000.0, 2, 32);
        assert_eq!(a.bytes_per_frame, 8);
        assert_eq!(a.format_id_str(), "lpcm");
        assert!(a.summary().contains("48000 Hz"));
    }
}
