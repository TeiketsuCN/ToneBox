//! DSD 的档位和 1-bit 采样率。
//!
//! ffprobe 对 DSF 报的采样率是"字节率"（DSD64 = 352800 Hz），不是真正的 1-bit 频率；
//! 档位优先用码率反推（立体声 DSD 码率 = 档位 × 88.2 kbps），没有码率时再看容器：
//! DSF（`dsd_lsbf*`）报字节率，DFF（`dsd_msbf*`）报 1-bit 率。44.1k / 48k 两个家族
//! 都按 64 / 128 / 256 / 512 / 1024 认。

const LEVELS: [u32; 5] = [64, 128, 256, 512, 1024];

/// 档位：64 / 128 / 256 / 512 / 1024，0 = 认不出来
pub fn tier(codec: &str, sample_rate: u32, bitrate_kbps: u64) -> u32 {
    if sample_rate == 0 {
        return 0;
    }
    // 码率反推最稳，和容器写法无关（单声道会只有一半，留给下面的采样率兜底）
    if bitrate_kbps > 0 {
        let est = bitrate_kbps as f64 * 1000.0 / 88_200.0;
        for l in LEVELS {
            if (est - l as f64).abs() <= l as f64 * 0.06 {
                return l;
            }
        }
    }
    let from_hz = |hz: u64| -> u32 {
        for base in [44_100u64, 48_000] {
            if hz >= base && hz % base == 0 && LEVELS.contains(&((hz / base) as u32)) {
                return (hz / base) as u32;
            }
        }
        0
    };
    let byte_rate_style = !codec.to_ascii_lowercase().contains("msbf");
    if byte_rate_style {
        // DSF：ffprobe 给的是字节率，×8 才是 1-bit 率
        let t = from_hz(sample_rate as u64 * 8);
        if t > 0 {
            return t;
        }
        from_hz(sample_rate as u64)
    } else {
        let t = from_hz(sample_rate as u64);
        if t > 0 {
            return t;
        }
        from_hz(sample_rate as u64 * 8)
    }
}

/// 真正的 1-bit 采样率（Hz）：ffprobe 的字节率 × 8
pub fn bit_rate_hz(sample_rate: u32) -> f64 {
    sample_rate as f64 * 8.0
}

#[cfg(test)]
mod tests {
    use super::{bit_rate_hz, tier};

    #[test]
    fn dsf_byte_rate_maps_to_tier() {
        // 用户那批 NativeDSD 的 64fs 文件：352800 Hz / 5644 kbps
        assert_eq!(tier("dsd_lsbf_planar", 352_800, 5_644), 64);
        assert_eq!(tier("dsd_lsbf_planar", 705_600, 11_289), 128);
        assert_eq!(tier("dsd_lsbf_planar", 1_411_200, 22_579), 256);
        assert_eq!(tier("dsd_lsbf_planar", 2_822_400, 45_158), 512);
    }

    #[test]
    fn raw_bit_rate_maps_to_tier() {
        // DFF 直接报 1-bit 率
        assert_eq!(tier("dsd_msbf", 2_822_400, 0), 64);
        assert_eq!(tier("dsd_msbf_planar", 5_644_800, 0), 128);
    }

    #[test]
    fn mono_dsd_falls_back_to_sample_rate() {
        // 单声道码率只有一半，码率反推会失败，得靠采样率
        assert_eq!(tier("dsd_lsbf_planar", 352_800, 2_822), 64);
    }

    #[test]
    fn unknown_rate_is_zero() {
        assert_eq!(tier("dsd_lsbf", 0, 0), 0);
        assert_eq!(tier("flac", 48_000, 0), 0);
        assert_eq!(bit_rate_hz(352_800), 2_822_400.0);
    }
}
