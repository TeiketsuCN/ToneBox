//! EBU R128 / ITU-R BS.1770 响度测量。
//!
//! 自己按规范实现（K 加权 + 400ms 分块 + 门限），不引第三方 crate：
//! - K 加权 = 高架滤波 + 38 Hz 高通，系数用规范里 48 kHz 那组
//! - 分块 400 ms、重叠 75%（步进 100 ms）
//! - 门限：绝对 -70 LUFS，相对 -10 LU
//!
//! 只接受 **48 kHz** 的 PCM —— 解码时统一重采样到 48k，这样系数不用重新设计。

/// 一次测量的结果
#[derive(Debug, Clone, Copy)]
pub struct Loudness {
    /// 整合响度（LUFS）
    pub lufs: f32,
    /// 采样峰值（dBFS，不是真峰值）
    pub peak_dbfs: f32,
}

// BS.1770-4 附表：48 kHz 的 K 加权系数
const SHELF_B: [f32; 3] = [1.53512485958697, -2.69169618940638, 1.19839281085285];
const SHELF_A: [f32; 3] = [1.0, -1.69065929318241, 0.73248077421585];
const HPF_B: [f32; 3] = [1.0, -2.0, 1.0];
const HPF_A: [f32; 3] = [1.0, -1.99004745483398, 0.99007225036621];

const RATE: usize = 48_000;
const BLOCK: usize = RATE * 4 / 10; // 400 ms
const HOP: usize = RATE / 10; // 100 ms

/// 二阶 biquad（转置直接 II 型，状态是 [s1, s2]）
fn biquad(x: f32, z: &mut [f32; 2], b: &[f32; 3], a: &[f32; 3]) -> f32 {
    let y = b[0] * x + z[0];
    z[0] = b[1] * x - a[1] * y + z[1];
    z[1] = b[2] * x - a[2] * y;
    y
}

fn k_weight(x: f32, shelf: &mut [f32; 2], hpf: &mut [f32; 2]) -> f32 {
    let y = biquad(x, shelf, &SHELF_B, &SHELF_A);
    biquad(y, hpf, &HPF_B, &HPF_A)
}

/// 测整合响度。`interleaved` 是 48 kHz 的交错 PCM，`channels` 是声道数。
pub fn measure(interleaved: &[f32], channels: usize) -> Option<Loudness> {
    let ch = channels.max(1);
    let frames = interleaved.len() / ch;
    if frames < BLOCK {
        return None;
    }

    // 峰值
    let mut peak = 0.0f32;
    for s in interleaved {
        let a = s.abs();
        if a > peak {
            peak = a;
        }
    }
    let peak_dbfs = if peak > 0.0 {
        20.0 * peak.log10()
    } else {
        f32::NEG_INFINITY
    };

    // 逐声道 K 加权；每个"帧"记一个跨声道的加权功率（权重都是 1.0：单/立体声）
    let mut shelf = vec![[0.0f32; 2]; ch];
    let mut hpf = vec![[0.0f32; 2]; ch];

    // 滑动窗口：环形缓冲 + 运行和，内存只有 400ms 的 f64
    let mut ring = vec![0.0f64; BLOCK];
    let mut ring_sum = 0.0f64;
    let mut pos = 0usize;
    let mut seen = 0usize;
    let mut powers: Vec<f64> = Vec::new();

    for f in 0..frames {
        let mut p = 0.0f64;
        for c in 0..ch {
            let y = k_weight(interleaved[f * ch + c], &mut shelf[c], &mut hpf[c]);
            p += (y as f64) * (y as f64);
        }
        ring_sum += p - ring[pos];
        ring[pos] = p;
        pos = (pos + 1) % BLOCK;
        seen += 1;

        if seen >= BLOCK && (seen - BLOCK) % HOP == 0 {
            powers.push(ring_sum / BLOCK as f64);
        }
    }
    if powers.is_empty() {
        return None;
    }

    let block_lufs = |p: f64| -> f64 {
        if p <= 0.0 {
            f64::NEG_INFINITY
        } else {
            -0.691 + 10.0 * p.log10()
        }
    };

    // 绝对门限 -70 LUFS
    let abs_gated: Vec<f64> = powers
        .iter()
        .copied()
        .filter(|p| block_lufs(*p) > -70.0)
        .collect();
    if abs_gated.is_empty() {
        return None;
    }
    let abs_mean = abs_gated.iter().sum::<f64>() / abs_gated.len() as f64;

    // 相对门限：比（过绝对门限后的）平均响度低 10 LU 以上的块不算
    let rel_threshold = block_lufs(abs_mean) - 10.0;
    let gated: Vec<f64> = abs_gated
        .into_iter()
        .filter(|p| block_lufs(*p) > rel_threshold)
        .collect();
    if gated.is_empty() {
        return None;
    }
    let mean = gated.iter().sum::<f64>() / gated.len() as f64;
    let lufs = if mean > 0.0 {
        (-0.691 + 10.0 * mean.log10()) as f32
    } else {
        f32::NEG_INFINITY
    };
    Some(Loudness { lufs, peak_dbfs })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sine(amp: f32, hz: f32, secs: f32, ch: usize) -> Vec<f32> {
        let n = (48_000.0 * secs) as usize;
        let mut v = Vec::with_capacity(n * ch);
        for i in 0..n {
            let s = amp * (std::f32::consts::TAU * hz * i as f32 / 48_000.0).sin();
            for _ in 0..ch {
                v.push(s);
            }
        }
        v
    }

    #[test]
    fn measures_known_sine() {
        // 两个声道都是 -20 dBFS 的 1 kHz 正弦：
        // 每声道均方 0.005，相加 0.01 → -0.691 + 10log10(0.01) ≈ -20.7 LUFS
        let pcm = sine(0.1, 1000.0, 3.0, 2);
        let m = measure(&pcm, 2).expect("应该能测");
        assert!((m.lufs - -20.7).abs() < 1.0, "lufs = {}", m.lufs);
        assert!((m.peak_dbfs - -20.0).abs() < 0.2, "peak = {}", m.peak_dbfs);
    }

    #[test]
    fn quieter_is_lower() {
        let loud = measure(&sine(0.5, 1000.0, 2.0, 2), 2).unwrap();
        let quiet = measure(&sine(0.05, 1000.0, 2.0, 2), 2).unwrap();
        assert!(loud.lufs > quiet.lufs + 15.0);
    }

    #[test]
    fn silence_has_no_loudness() {
        let pcm = vec![0.0f32; 48_000 * 2 * 2];
        assert!(measure(&pcm, 2).is_none());
    }

    #[test]
    fn shorter_than_one_block_returns_none() {
        let pcm = sine(0.5, 1000.0, 0.1, 2);
        assert!(measure(&pcm, 2).is_none());
    }
}
