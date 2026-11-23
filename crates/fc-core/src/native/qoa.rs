//! QOA（Quite OK Audio）原生解码器。
//!
//! ## 为什么按大端读
//!
//! QOA 规范里文件头、帧头、LMS 状态、slice 全部是 **big endian**（qoaconv 和
//! ffmpeg 都这么读），所以这里每个整数都走 `u16_be` / `u32_be` / `u64_be`。
//!
//! ## 文件结构
//!
//! ```text
//! 文件头 8 字节:  "qoaf" + u32 BE 总样本数（注意是每声道样本数，不是交织总数）
//! 帧:            8 字节帧头 + 每声道 16 字节 LMS 状态 + slice
//! 帧头:          channels u8 | samplerate u24 | samples u16 | frame_size u16
//! LMS 状态:      history[4] + weights[4]，每个 i16 BE（先 history 后 weights）
//! slice:         8 字节 = 64 bit，编 20 个样本
//!                高 4 bit 是 scale factor，之后是 20 个 3-bit 量化残差
//! ```
//!
//! 一帧最多 256 个 slice（5120 个样本）；多声道按 slice 交织：
//! `(ch0, slice0), (ch1, slice0), (ch0, slice1), ...`，所以不能按帧简单交织。
//!
//! ## 为什么结果能和 ffmpeg 逐位一致
//!
//! QOA 解码是纯整数、确定性的：预测值 = `(sum(weights[i] * history[i])) >> 13`，
//! 残差查 16×8 的 dequant 表（表里已经乘过 scale factor），相加后 clamp 到
//! i16，再用「去量化后的残差」做 Sign-Sign LMS 更新。量化表和整数语义照抄
//! 参考实现，输出就完全相同；这里的测试直接拿 ffmpeg 的 f32 解码结果比对。
//!
//! 有损只发生在编码端（每样本量化成 3 bit 残差），解码端不会再有误差。

use std::path::Path;

use anyhow::{anyhow, Context, Result};

use super::{read_all, u16_be, u32_be, u64_be, WavInfo, WavSink};

/// 一帧最多 256 个 slice，每个 slice 编 20 个样本。
const SLICE_LEN: usize = 20;
/// 每帧 slice 数上限（规范常量，用来拒绝明显损坏的帧头）。
const SLICES_PER_FRAME: usize = 256;
/// LMS 阶数：4 个历史样本 + 4 个权重。
const LMS_LEN: usize = 4;
/// 每声道 LMS 状态的字节数：4 个 i16 history + 4 个 i16 weights = 16 字节。
const LMS_BYTES: usize = LMS_LEN * 2 * 2;

/// 去量化表：`[scale_factor][quantized]`，直接落成最终残差值。
///
/// 标准实现里只列 8 个基准值（0.75 / 2.5 / 4.5 / 7 的正负），再乘
/// `round((sf+1)^2.75)` 得到这张 16 档的表。这里把乘好的结果写死，
/// 避免运行时再算一次浮点、也避免和参考实现的取整规则产生偏差
/// （第 14 档就是典型：4288/7718/12005，而不是手算更"整齐"的 4286/7716/12000）。
const DEQUANT_TAB: [[i32; 8]; 16] = [
    [1, -1, 3, -3, 5, -5, 7, -7],
    [5, -5, 18, -18, 32, -32, 49, -49],
    [16, -16, 53, -53, 95, -95, 147, -147],
    [34, -34, 113, -113, 203, -203, 315, -315],
    [63, -63, 210, -210, 378, -378, 588, -588],
    [104, -104, 345, -345, 621, -621, 966, -966],
    [158, -158, 528, -528, 950, -950, 1477, -1477],
    [228, -228, 760, -760, 1368, -1368, 2128, -2128],
    [316, -316, 1053, -1053, 1895, -1895, 2947, -2947],
    [422, -422, 1405, -1405, 2529, -2529, 3934, -3934],
    [548, -548, 1828, -1828, 3290, -3290, 5117, -5117],
    [696, -696, 2320, -2320, 4176, -4176, 6496, -6496],
    [868, -868, 2893, -2893, 5207, -5207, 8099, -8099],
    [1064, -1064, 3548, -3548, 6386, -6386, 9933, -9933],
    [1286, -1286, 4288, -4288, 7718, -7718, 12005, -12005],
    [1536, -1536, 5120, -5120, 9216, -9216, 14336, -14336],
];

/// 一个声道的 LMS 预测器状态。
#[derive(Clone, Copy, Default)]
struct Lms {
    history: [i32; LMS_LEN],
    weights: [i32; LMS_LEN],
}

impl Lms {
    fn from_bytes(b: &[u8], mut off: usize) -> Self {
        let mut s = Lms::default();
        for i in 0..LMS_LEN {
            // i16 BE 读出后符号扩展：history 和 weights 都是有符号的
            s.history[i] = i16::from_be_bytes([b[off], b[off + 1]]) as i32;
            off += 2;
        }
        for i in 0..LMS_LEN {
            s.weights[i] = i16::from_be_bytes([b[off], b[off + 1]]) as i32;
            off += 2;
        }
        s
    }

    /// 预测下一个样本。
    ///
    /// 用 `wrapping_*` 而不是普通乘加：ffmpeg 的参考解码器用 32-bit 无符号乘法
    /// 再累加（C 里等价于模 2^32 回绕）。正常音频不会溢出，但畸形文件可能让
    /// 中间结果越界；debug 版普通运算会 panic，而回绕才和参考实现逐位一致。
    fn predict(&self) -> i32 {
        let mut p: i32 = 0;
        for i in 0..LMS_LEN {
            p = p.wrapping_add(self.weights[i].wrapping_mul(self.history[i]));
        }
        p >> 13
    }

    /// Sign-Sign LMS 更新：权重只按 history 的符号加减 `delta`，历史整体左移。
    ///
    /// 注意 `delta` 来自**去量化后的残差**（不是原始误差），并且先右移 4 位；
    /// 这是 QOA 能只用 4 个权重就压住大多数信号的关键，抄错这一步输出会整体漂移。
    fn update(&mut self, sample: i32, residual: i32) {
        let delta = residual >> 4;
        for i in 0..LMS_LEN {
            let step = if self.history[i] < 0 { -delta } else { delta };
            self.weights[i] = self.weights[i].wrapping_add(step);
        }
        for i in 0..LMS_LEN - 1 {
            self.history[i] = self.history[i + 1];
        }
        self.history[LMS_LEN - 1] = sample;
    }
}

/// 16-bit 饱和：QOA 的重建值可能超出 i16（预测 + 残差），参考实现一律 clamp。
fn clamp_s16(v: i32) -> i32 {
    v.clamp(-32768, 32767)
}

pub fn decode(input: &Path, output: &Path) -> Result<WavInfo> {
    let b = read_all(input).with_context(|| format!("读 QOA 失败：{}", input.display()))?;

    // 最少要能放下 8 字节文件头 + 8 字节第一帧帧头
    if b.len() < 16 {
        return Err(anyhow!(
            "文件只有 {} 字节，QOA 至少需要 8 字节文件头 + 8 字节帧头",
            b.len()
        ));
    }
    if &b[0..4] != b"qoaf" {
        return Err(anyhow!(
            "不是 QOA 文件：魔数应为 qoaf，实际是 {:?}",
            &b[0..4]
        ));
    }

    // 文件头里的总样本数是「每声道」，不是交织后的总数。
    let total = u32_be(&b, 4) as u64;
    if total == 0 {
        // 规范允许 0 表示流式（总长未知）；本解码器要求已知总长，
        // 否则既没法预校验每帧长度，也没法在写完 WAV 头时确定帧数。
        return Err(anyhow!(
            "QOA 文件头的总样本数为 0：这是流式文件的标记，本解码器只支持静态文件"
        ));
    }

    // 文件头只有 8 字节；声道数和采样率是从第一帧帧头里读的。
    let channels = b[8] as u16;
    let rate = u32_be(&b, 8) & 0x00FF_FFFF;
    if channels == 0 {
        return Err(anyhow!("QOA 帧头声道数为 0"));
    }
    if rate == 0 {
        return Err(anyhow!("QOA 帧头采样率为 0"));
    }
    // WavSink 的 WAV 头里字节率是 u32 = rate × channels × 4。QOA 允许 16 MHz /
    // 255 声道的极端组合，乘出来会超出 u32；提前报错，别等到写头时 debug 溢出 panic。
    if rate as u64 * channels as u64 * 4 > u32::MAX as u64 {
        return Err(anyhow!(
            "采样率 {rate} × {channels} 声道超出 WAV 头 u32 字节率的表示范围"
        ));
    }

    let ch = channels as usize;
    let mut sink = WavSink::create(output, rate, channels)?;

    let mut lms = vec![Lms::default(); ch];
    // 逐帧解码：每帧的 LMS 状态独立写在帧头后面，帧与帧之间不延续。
    let mut pos = 8usize;
    let mut decoded = 0u64;
    let mut frame_no = 0u64;

    while decoded < total {
        if b.len() - pos < 8 {
            return Err(anyhow!(
                "第 {frame_no} 帧帧头不完整：文件头声明 {total} 个样本，但只解出 {decoded} 个"
            ));
        }

        let frame_channels = b[pos] as u16;
        let frame_rate = u32_be(&b, pos) & 0x00FF_FFFF;
        let samples = u16_be(&b, pos + 4) as usize;
        let frame_size = u16_be(&b, pos + 6) as usize;

        // 静态 QOA 的每帧参数必须和第一帧一致（规范要求）。
        if frame_channels != channels {
            return Err(anyhow!(
                "第 {frame_no} 帧声道数 {frame_channels} 与文件开头 {channels} 不一致"
            ));
        }
        if frame_rate != rate {
            return Err(anyhow!(
                "第 {frame_no} 帧采样率 {frame_rate} 与文件开头 {rate} 不一致"
            ));
        }
        // 0 样本会让下面的 while 原地打转，必须当成损坏文件拒绝。
        if samples == 0 {
            return Err(anyhow!("第 {frame_no} 帧样本数为 0"));
        }
        if samples > SLICES_PER_FRAME * SLICE_LEN {
            return Err(anyhow!(
                "第 {frame_no} 帧样本数 {samples} 超过一帧上限 {}",
                SLICES_PER_FRAME * SLICE_LEN
            ));
        }
        if decoded + samples as u64 > total {
            return Err(anyhow!(
                "第 {frame_no} 帧样本数 {samples} 超出文件头声明的总数 {total}（已解 {decoded}）"
            ));
        }

        // 帧头里声明的 frame_size 是完整帧长（含帧头）；先校验它真的在文件里，
        // 后面的所有切片读取都以它为边界，避免越界读。
        if frame_size > b.len() - pos {
            return Err(anyhow!(
                "第 {frame_no} 帧声明 {frame_size} 字节，但文件里只剩 {} 字节",
                b.len() - pos
            ));
        }
        let slices = (samples + SLICE_LEN - 1) / SLICE_LEN;
        let need = 8 + LMS_BYTES * ch + 8 * slices * ch;
        if frame_size < need {
            return Err(anyhow!(
                "第 {frame_no} 帧只有 {frame_size} 字节，解 {samples} 个样本至少需要 {need} 字节"
            ));
        }

        // LMS 状态是每声道 16 字节，按声道顺序紧跟帧头。
        let mut p = pos + 8;
        for c in 0..ch {
            lms[c] = Lms::from_bytes(&b, p);
            p += LMS_BYTES;
        }

        let mut pcm = vec![0f32; samples * ch];
        let mut sample_index = 0usize;
        while sample_index < samples {
            let n = (samples - sample_index).min(SLICE_LEN);
            for c in 0..ch {
                // slice 按声道交织：同一段样本里先 ch0 的 slice，再 ch1 的……
                let mut slice = u64_be(&b, p);
                p += 8;
                let scale_factor = (slice >> 60) as usize;
                let state = &mut lms[c];
                for k in 0..n {
                    let predicted = state.predict();
                    // 每个残差 3 bit，从高往低取；取完左移，下一轮又落在第 57 位。
                    let quantized = ((slice >> 57) & 0x7) as usize;
                    let residual = DEQUANT_TAB[scale_factor][quantized];
                    let reconstructed = clamp_s16(predicted + residual);

                    // WAV 是 f32：QOA 本身是 16-bit 有损，统一按 /32768 落到 [-1,1)，
                    // 和 ffmpeg 的 s16→f32 换算一致，后面比较才能逐位相等。
                    pcm[(sample_index + k) * ch + c] = reconstructed as f32 / 32768.0;

                    slice <<= 3;
                    state.update(reconstructed, residual);
                }
            }
            sample_index += n;
        }

        sink.write(&pcm)?;
        decoded += samples as u64;
        pos += frame_size;
        frame_no += 1;
    }

    if decoded != total {
        return Err(anyhow!(
            "解出的样本数 {decoded} 与文件头声明的 {total} 不一致"
        ));
    }
    sink.finish()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::{Path, PathBuf};
    use std::process::Command;

    // 本机固定的参考工具位置；也可以用环境变量覆盖，换机器/CI 时不必改代码。
    const QOACONV_PATH: &str = r"D:\Apps\ToneLore\qoa\qoaconv.exe";
    const FFMPEG_PATH: &str = r"D:\Apps\ToneLore\ffmpeg\ffmpeg.exe";

    fn tool(env: &str, fixed: &str) -> Option<PathBuf> {
        if let Some(p) = std::env::var_os(env) {
            let p = PathBuf::from(p);
            if p.is_file() {
                return Some(p);
            }
        }
        let p = PathBuf::from(fixed);
        p.is_file().then_some(p)
    }

    /// 没有参考工具就跳过（并打印原因），不让 `cargo test` 因环境失败。
    fn tools() -> Option<(PathBuf, PathBuf)> {
        match (
            tool("TONELORE_QOACONV", QOACONV_PATH),
            tool("TONELORE_FFMPEG", FFMPEG_PATH),
        ) {
            (Some(q), Some(f)) => Some((q, f)),
            _ => {
                eprintln!(
                    "skip: 找不到 qoaconv / ffmpeg；可用 TONELORE_QOACONV、TONELORE_FFMPEG 指定路径"
                );
                None
            }
        }
    }

    fn tmp_dir(tag: &str) -> PathBuf {
        let p = std::env::temp_dir().join(format!("fc_core_qoa_{}_{}", tag, std::process::id()));
        let _ = std::fs::remove_dir_all(&p);
        std::fs::create_dir_all(&p).expect("建临时目录失败");
        p
    }

    /// 写 16-bit PCM WAV。qoaconv 的输入用 PCM16 最稳，不依赖它怎么处理 float WAV。
    fn write_wav_s16(path: &Path, rate: u32, channels: u16, samples: &[i16]) {
        use std::io::Write;
        let data_len = (samples.len() * 2) as u32;
        let block = channels as u32 * 2;
        let mut f = std::fs::File::create(path).expect("建测试 WAV 失败");
        f.write_all(b"RIFF").unwrap();
        f.write_all(&(36 + data_len).to_le_bytes()).unwrap();
        f.write_all(b"WAVEfmt ").unwrap();
        f.write_all(&16u32.to_le_bytes()).unwrap();
        f.write_all(&1u16.to_le_bytes()).unwrap(); // WAVE_FORMAT_PCM
        f.write_all(&channels.to_le_bytes()).unwrap();
        f.write_all(&rate.to_le_bytes()).unwrap();
        f.write_all(&(rate * block).to_le_bytes()).unwrap();
        f.write_all(&(block as u16).to_le_bytes()).unwrap();
        f.write_all(&16u16.to_le_bytes()).unwrap();
        f.write_all(b"data").unwrap();
        f.write_all(&data_len.to_le_bytes()).unwrap();
        let mut bytes = Vec::with_capacity(samples.len() * 2);
        for s in samples {
            bytes.extend_from_slice(&s.to_le_bytes());
        }
        f.write_all(&bytes).unwrap();
    }

    /// 确定性测试信号：正弦 + 伪随机噪声 + 近静音交替，
    /// 让编码器尽量用过不同的 scale factor，覆盖 dequant 表的多行。
    fn signal(rate: u32, channels: u16, frames: usize) -> Vec<i16> {
        let ch = channels as usize;
        let mut out = Vec::with_capacity(frames * ch);
        let mut seed: u32 = 0x1234_5678;
        for i in 0..frames {
            for c in 0..ch {
                seed = seed.wrapping_mul(1_103_515_245).wrapping_add(12_345);
                let noise = ((seed >> 8) % 1200) as i32 - 600;
                let phase = 2.0 * std::f32::consts::PI * (220.0 + 110.0 * c as f32) * i as f32
                    / rate as f32;
                let mut v = (12_000.0 * phase.sin()) as i32;
                match (i / 3000) % 3 {
                    1 => v = noise,
                    2 => v = (v as f32 * 0.001) as i32,
                    _ => {}
                }
                out.push(v.clamp(-32768, 32767) as i16);
            }
        }
        out
    }

    /// 读 f32 LE 原始数据。WavSink 的头固定 44 字节（RIFF + fmt16 + data），
    /// 它的 data 起始偏移在 mod.rs 里写死，这里直接跳过。
    fn read_f32le(path: &Path, skip: usize) -> Vec<f32> {
        let b = read_all(path).expect("读结果失败");
        let body = &b[skip..];
        assert_eq!(body.len() % 4, 0, "f32 数据长度不是 4 的倍数");
        body.chunks_exact(4)
            .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
            .collect()
    }

    fn run(cmd: &mut Command) {
        let out = cmd.output().expect("启动参考工具失败");
        assert!(
            out.status.success(),
            "参考工具执行失败：status={:?}\nstdout={}\nstderr={}",
            out.status,
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
    }

    /// 完整链路：造 WAV → qoaconv 编 .qoa → 本解码器 → 和 ffmpeg 的 f32 解码逐样本比。
    fn check_roundtrip(tag: &str, rate: u32, channels: u16, frames: usize) {
        let Some((qoaconv, ffmpeg)) = tools() else {
            return;
        };
        let dir = tmp_dir(tag);
        let in_wav = dir.join("in.wav");
        let qoa = dir.join("in.qoa");
        let out_wav = dir.join("out.wav");
        let ref_f32 = dir.join("ref.f32");

        write_wav_s16(&in_wav, rate, channels, &signal(rate, channels, frames));
        run(Command::new(&qoaconv).arg(&in_wav).arg(&qoa));
        assert!(qoa.is_file(), "qoaconv 没有产出 .qoa");

        let info = decode(&qoa, &out_wav).expect("QOA 解码失败");
        assert_eq!(info.rate, rate);
        assert_eq!(info.channels, channels);
        assert_eq!(info.frames, frames as u64);

        // ffmpeg 把 QOA 解成 s16，再按 /32768 转 f32；本解码器走的是同一条路径。
        run(Command::new(&ffmpeg)
            .args(["-v", "error", "-y", "-i"])
            .arg(&qoa)
            .args(["-f", "f32le"])
            .arg(&ref_f32));

        let mine = read_f32le(&out_wav, 44);
        let reference = read_f32le(&ref_f32, 0);
        assert_eq!(mine.len(), frames * channels as usize, "解出的样本数不对");
        assert_eq!(mine.len(), reference.len(), "和 ffmpeg 的样本数不一致");

        let mut max_diff = 0.0f32;
        let mut worst = 0usize;
        for (i, (a, b)) in mine.iter().zip(reference.iter()).enumerate() {
            let d = (a - b).abs();
            if d > max_diff {
                max_diff = d;
                worst = i;
            }
        }
        println!(
            "[qoa] {tag}: {} Hz / {} ch / {} 帧，最大误差 {:e}（第 {} 个样本）",
            info.rate, info.channels, info.frames, max_diff, worst
        );
        assert!(
            max_diff < 1e-6,
            "{tag}: 与 ffmpeg 解码不一致，第 {worst} 个样本误差 {max_diff}"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn decodes_stereo_44k1_and_matches_ffmpeg() {
        // 52917 不是 20 的倍数：最后一个 slice 只有 17 个样本，顺便验证不足 20 的处理。
        check_roundtrip("stereo44k1", 44_100, 2, 52_917);
    }

    #[test]
    fn decodes_mono_48k_and_matches_ffmpeg() {
        check_roundtrip("mono48k", 48_000, 1, 36_123);
    }

    #[test]
    fn decodes_multichannel_and_matches_ffmpeg() {
        // 3 声道、96 kHz：验证 slice 级声道交织和 24-bit 采样率读取。
        check_roundtrip("tri96k", 96_000, 3, 12_001);
    }

    #[test]
    fn rejects_invalid_input() {
        let dir = tmp_dir("invalid");

        // 魔数不对
        let bad_magic = dir.join("bad_magic.qoa");
        std::fs::write(&bad_magic, b"RIFF....WAVEfmt ").unwrap();
        assert!(decode(&bad_magic, &dir.join("o1.wav")).is_err());

        // 流式文件（总样本数为 0）
        let streaming = dir.join("streaming.qoa");
        let mut b = Vec::new();
        b.extend_from_slice(b"qoaf");
        b.extend_from_slice(&0u32.to_be_bytes());
        b.extend_from_slice(&[1, 0, 0x1F, 0x40, 0, 20, 0, 0x30]); // 1ch / 8000 / 20 / 48
        std::fs::write(&streaming, &b).unwrap();
        assert!(decode(&streaming, &dir.join("o2.wav")).is_err());

        // frame_size 超出文件实际长度
        let truncated = dir.join("truncated.qoa");
        b[4..8].copy_from_slice(&20u32.to_be_bytes());
        std::fs::write(&truncated, &b).unwrap();
        assert!(decode(&truncated, &dir.join("o3.wav")).is_err());

        let _ = std::fs::remove_dir_all(&dir);
    }
}
