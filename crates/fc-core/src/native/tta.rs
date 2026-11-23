//! TTA（True Audio）原生解码器。
//!
//! TTA 是“帧独立”的无损音频格式：每一帧都从零初始化预测器、LMS 滤波器和
//! Rice 编码器状态，帧与帧之间不共享历史。因此可以顺序逐帧解码，不需要
//! 随机访问，也不会因为丢一帧而污染后面所有帧。
//!
//! 这里的算法按 FFmpeg 的 `libavcodec/tta.c` / `ttadsp.c` / `ttadata.c`
//! 逐位重写，目标是与
//! `ffmpeg -v error -i x.tta -f f32le ref.f32` 的输出逐样本一致：
//!
//! 1. 每帧先做两状态 Rice 熵解码，得到有符号残差；
//! 2. 残差经过 8 阶 sign-sign LMS 混合滤波器（TTA 的“自适应预测”）；
//! 3. 再加上固定一阶预测 `PRED(predictor, k)`；
//! 4. 多声道时最后做级联的反向声道去相关。
//!
//! 为什么整数运算大量使用 `wrapping_*`：C 参考实现依赖 32 位补码回绕
//! （`qm[i] -= dx[i]`、`round += dl[i] * qm[i]`、`sum += value` 都可能溢出）。
//! Rust 的 debug 构建遇到溢出会 panic，这里必须显式回绕，才能既匹配参考
//! 输出、又满足“坏文件返回 Err 而不是 panic”。

use std::path::Path;

use anyhow::{anyhow, Context, Result};

use super::{read_all, u16_le, u32_le, WavInfo, WavSink};

/// LMS 滤波器阶数（TTA 固定 8 阶）
const MAX_ORDER: usize = 8;
/// Rice 编码器每帧的初始 k
const RICE_K_INIT: u32 = 10;
/// 与 FFmpeg 解码器一致的上限；TTA 规范本身允许更多，但实际文件不会超过
const MAX_CHANNELS: u16 = 16;
/// 采样率上限沿用 FFmpeg 的保护值，避免后面乘法/分配失控
const MAX_RATE: u32 = 0x7F_FFFF;

/// TTA 标准帧长：`rate * 256 / 245`（约 1.045 秒）。
///
/// 为什么不用 `rate / 25`：TTA 的时间基是 1.044897959... 秒/帧，
/// 官方编码器和 FFmpeg 都用 256/245 这个有理数。用 `rate / 25` 会让
/// 帧边界和参考解码错位，后面的样本就全对不上了。
fn frame_length(rate: u32) -> Result<usize> {
    let len = (rate as u64) * 256 / 245;
    if len == 0 || len > usize::MAX as u64 {
        return Err(anyhow!("TTA 帧长计算失败（采样率 {rate}）"));
    }
    Ok(len as usize)
}

/// `shift_1[k] = 1 << k`，但按 FFmpeg 的表在 k >= 32 时饱和。
///
/// 正常情况下 Rice 的 k 只有十几，这个饱和分支只是为了让损坏的流
/// 也走 Err 路径而不是在 Rust 里 panic。
#[inline]
fn shift_1(k: u32) -> u32 {
    match k {
        0..=31 => 1u32 << k,
        32..=39 => 0x8000_0000,
        _ => u32::MAX,
    }
}

/// FFmpeg 的 `ff_tta_shift_16[k]` 就是 `shift_1[k + 4]`。
#[inline]
fn shift_16(k: u32) -> u32 {
    shift_1(k.saturating_add(4))
}

/// 小端、LSB-first 的位读取器。
///
/// TTA 用 `BITSTREAM_READER_LE`：一个字节里的最低位先出现。所以不能
/// 直接拿 `u32::from_le_bytes` 按 32 位块读，必须一位一位按 LSB 顺序取。
struct BitReader<'a> {
    data: &'a [u8],
    /// 已经消费的位数
    bit: usize,
}

impl<'a> BitReader<'a> {
    fn new(data: &'a [u8]) -> Self {
        BitReader { data, bit: 0 }
    }

    fn read_bit(&mut self) -> Result<u32> {
        if self.bit >= self.data.len() * 8 {
            return Err(anyhow!("TTA 帧数据提前结束"));
        }
        let b = (self.data[self.bit >> 3] >> (self.bit & 7)) & 1;
        self.bit += 1;
        Ok(b as u32)
    }

    /// 读 n 位（n <= 32），第一个读到的位是结果的 bit0
    fn read_bits(&mut self, n: u32) -> Result<u32> {
        if n == 0 {
            return Ok(0);
        }
        if n > 32 {
            return Err(anyhow!("TTA 位读取越界：一次读 {n} 位"));
        }
        let mut v = 0u32;
        for i in 0..n {
            v |= self.read_bit()? << i;
        }
        Ok(v)
    }

    /// 数连续的 1，直到读到 0（0 也被消费掉），与 FFmpeg `get_unary(gb, 0, n)` 等价
    fn read_unary(&mut self) -> Result<u32> {
        let mut n = 0u32;
        loop {
            if self.read_bit()? == 0 {
                return Ok(n);
            }
            n = n
                .checked_add(1)
                .ok_or_else(|| anyhow!("TTA unary 计数溢出"))?;
        }
    }

    /// 对齐到下一个字节边界（FFmpeg `align_get_bits`）
    fn align(&mut self) {
        self.bit = (self.bit + 7) & !7;
    }

    /// 当前字节位置（调用前先 align）
    fn byte_pos(&self) -> usize {
        self.bit / 8
    }
}

/// 两状态 Rice 解码器。
///
/// TTA 的关键细节：先看 unary 是否为 0 选 k0；否则用 k1，并且 unary 要减 1。
/// k1 通道解出的值还要加上 `1 << k0`（escape 偏移），随后用它更新 k0 的
/// 统计量。这个顺序不能交换，否则自适应会慢半拍、输出就会错。
struct Rice {
    k0: u32,
    k1: u32,
    sum0: u32,
    sum1: u32,
}

impl Rice {
    fn new() -> Self {
        Rice {
            k0: RICE_K_INIT,
            k1: RICE_K_INIT,
            sum0: shift_16(RICE_K_INIT),
            sum1: shift_16(RICE_K_INIT),
        }
    }

    /// 从位流里解出一个残差（已做 zig-zag 还原）
    fn decode(&mut self, br: &mut BitReader<'_>) -> Result<i32> {
        let mut unary = br.read_unary()?;
        // depth = 0 走 k0；depth = 1 走 k1，且 unary 里那个“额外的 1”要去掉
        let depth = if unary == 0 {
            0u32
        } else {
            unary -= 1;
            1u32
        };
        let k = if depth == 0 { self.k0 } else { self.k1 };
        if k > 31 {
            return Err(anyhow!("TTA Rice 参数 k={k} 超出 31，流可能已损坏"));
        }
        let low = br.read_bits(k)?;
        // C 里是 uint32 运算后赋给 int32，这里显式截断成同样的位模式
        let mut value = (((unary as u64) << k) | low as u64) as u32 as i32;

        if depth == 1 {
            // sum1 统计的是未加 escape 偏移的原始值
            self.sum1 = self
                .sum1
                .wrapping_add(value as u32)
                .wrapping_sub(self.sum1 >> 4);
            if self.k1 > 0 && self.sum1 < shift_16(self.k1) {
                self.k1 -= 1;
            } else if self.sum1 > shift_16(self.k1 + 1) {
                self.k1 += 1;
            }
            // 这里用的还是“更新前”的 k0，顺序与参考实现一致
            value = value.wrapping_add(shift_1(self.k0) as i32);
        }

        // depth == 1 时也 fallthrough 到这里，用加上偏移后的 value 更新 k0
        self.sum0 = self
            .sum0
            .wrapping_add(value as u32)
            .wrapping_sub(self.sum0 >> 4);
        if self.k0 > 0 && self.sum0 < shift_16(self.k0) {
            self.k0 -= 1;
        } else if self.sum0 > shift_16(self.k0 + 1) {
            self.k0 += 1;
        }

        // zig-zag：0,1,-1,2,-2,... 与 C 的 `1 + ((v >> 1) ^ ((v & 1) - 1))` 等价
        Ok(1 + ((value >> 1) ^ ((value & 1) - 1)))
    }
}

/// 8 阶 sign-sign LMS 混合预测滤波器（TTA 的核心）
struct Filter {
    /// 自适应系数
    qm: [i32; MAX_ORDER],
    /// 符号历史
    dx: [i32; MAX_ORDER],
    /// 延迟样本历史
    dl: [i32; MAX_ORDER],
    shift: i32,
    round: i32,
    error: i32,
}

impl Filter {
    fn new(bps_bytes: usize) -> Self {
        // FFmpeg `ff_tta_filter_configs = {10, 9, 10, 12}`，按字节位深取
        let shift = match bps_bytes {
            1 => 10,
            2 => 9,
            3 => 10,
            _ => 12,
        };
        Filter {
            qm: [0; MAX_ORDER],
            dx: [0; MAX_ORDER],
            dl: [0; MAX_ORDER],
            shift,
            // C 里 c->round = 1 << (shift - 1)
            round: 1i32 << (shift - 1),
            error: 0,
        }
    }

    /// 输入残差，返回加上自适应预测后的值。
    ///
    /// 逐行对应 FFmpeg 的 `tta_filter_process_c()`。注意 C 里 `qm` 是用
    /// `uint32_t*` 做加减的，但结果位模式与 i32 的 wrapping 运算完全相同。
    fn process(&mut self, input: i32) -> i32 {
        if self.error < 0 {
            for i in 0..MAX_ORDER {
                self.qm[i] = self.qm[i].wrapping_sub(self.dx[i]);
            }
        } else if self.error > 0 {
            for i in 0..MAX_ORDER {
                self.qm[i] = self.qm[i].wrapping_add(self.dx[i]);
            }
        }

        // 8 个乘积的和整体按 32 位回绕；分组顺序不影响模 2^32 结果
        let mut round = self.round;
        for i in 0..MAX_ORDER {
            round = round.wrapping_add(self.dl[i].wrapping_mul(self.qm[i]));
        }

        // 先滑动历史，再根据更新后的 dl 生成新的 dx/d
        for i in 0..4 {
            self.dx[i] = self.dx[i + 1];
            self.dl[i] = self.dl[i + 1];
        }
        // 右移是算术右移（i32 >> 在 Rust 里就是算术右移）
        self.dx[4] = (self.dl[4] >> 30) | 1;
        self.dx[5] = ((self.dl[5] >> 30) | 2) & !1;
        self.dx[6] = ((self.dl[6] >> 30) | 2) & !1;
        self.dx[7] = ((self.dl[7] >> 30) | 4) & !3;

        self.error = input;
        let out = input.wrapping_add(round >> self.shift);

        // 顺序必须严格照抄：dl[4] 用旧 dl[5]、dl[5] 用旧 dl[6]、
        // dl[6] 用旧 dl[7]，然后 dl[7] 才被 out 覆盖。
        self.dl[4] = self.dl[5].wrapping_neg();
        self.dl[5] = self.dl[6].wrapping_neg();
        self.dl[6] = out.wrapping_sub(self.dl[7]);
        self.dl[7] = out;
        self.dl[5] = self.dl[5].wrapping_add(self.dl[6]);
        self.dl[4] = self.dl[4].wrapping_add(self.dl[5]);

        out
    }
}

/// 单声道在一帧内的全部自适应状态。每帧都重新 new 一次（TTA 帧独立）。
struct ChannelState {
    predictor: i32,
    filter: Filter,
    rice: Rice,
}

impl ChannelState {
    fn new(bps_bytes: usize) -> Self {
        ChannelState {
            predictor: 0,
            filter: Filter::new(bps_bytes),
            rice: Rice::new(),
        }
    }
}

/// 固定一阶预测 `((x << k) - x) >> k`。
///
/// C 版本的中间量是 uint64 再逻辑右移、最后截成 int32；对 32 位结果而言，
/// 等价于先做 64 位有符号乘法再算术右移——把两种写法都验算过，
/// 负数情况取低 32 位后结果相同。
#[inline]
fn pred(x: i32, k: u32) -> i32 {
    (((x as i64).wrapping_mul((1i64 << k) - 1)) >> k) as i32
}

/// 反向声道去相关（解码侧）。
///
/// 编码器把 (c0, c1) 存成 (c1 - c0, c0 + ……)，多声道再级联。
/// 解码必须从最后一个声道往前倒着还原，否则前面声道的值已经被改掉。
fn decorrelate(group: &mut [i32]) {
    let n = group.len();
    if n <= 1 {
        return;
    }
    // C: for (*p += *r / 2; ...) —— 先给最后一个声道加上前一个声道的一半
    group[n - 1] = group[n - 1].wrapping_add(group[n - 2] / 2);
    // 然后 r 从倒数第二个一路向前：*r = *(r+1) - *r
    let mut r = n - 2;
    loop {
        group[r] = group[r + 1].wrapping_sub(group[r]);
        if r == 0 {
            break;
        }
        r -= 1;
    }
}

/// CRC-32/IEEE（反射多项式 0xEDB88320），TTA 头和每帧末尾都用它
fn crc32_ieee(data: &[u8]) -> u32 {
    let mut crc = 0xFFFF_FFFFu32;
    for &byte in data {
        crc ^= byte as u32;
        for _ in 0..8 {
            let mask = (crc & 1).wrapping_neg();
            crc = (crc >> 1) ^ (0xEDB8_8320 & mask);
        }
    }
    !crc
}

struct Header {
    channels: u16,
    bits: u16,
    rate: u32,
    /// 每声道总样本数
    samples: u32,
    /// TTA1 魔数在文件里的偏移（跳过前置 ID3v2）
    base: usize,
}

/// 解析 22 字节 TTA1 头。
fn parse_header(b: &[u8]) -> Result<Header> {
    // 有些 TTA 文件前面带 ID3v2 标签（ffmpeg 的 TTA demuxer 也允许）。
    // 支持它只是多算一个偏移，不改变后面的任何逻辑。
    let base = id3v2_prefix_len(b);
    if b.len() < base + 22 {
        return Err(anyhow!(
            "TTA 文件太短：需要至少 {} 字节，实际 {}",
            base + 22,
            b.len()
        ));
    }
    let h = &b[base..base + 22];
    if &h[0..4] != b"TTA1" {
        return Err(anyhow!("不是 TTA1 文件（魔数不匹配）"));
    }

    let format = u16_le(h, 4);
    if format == 2 {
        return Err(anyhow!("TTA format=2 是加密流，本解码器不支持"));
    }
    if format != 1 {
        return Err(anyhow!("TTA 格式号 {format} 不支持（只支持 1=PCM）"));
    }

    let channels = u16_le(h, 6);
    if channels == 0 || channels > MAX_CHANNELS {
        return Err(anyhow!("TTA 声道数 {channels} 非法（1..={MAX_CHANNELS}）"));
    }
    let bits = u16_le(h, 8);
    if !matches!(bits, 8 | 16 | 24) {
        return Err(anyhow!("TTA 位深 {bits} 不支持（只支持 8/16/24）"));
    }
    let rate = u32_le(h, 10);
    if rate == 0 || rate > MAX_RATE {
        return Err(anyhow!("TTA 采样率 {rate} 非法"));
    }
    let samples = u32_le(h, 14);
    if samples == 0 {
        return Err(anyhow!("TTA 头里的总样本数为 0"));
    }
    // 头 CRC 覆盖前 18 字节。校验它很便宜，而且能最早发现“偏移/字节序
    // 理解错了”，比解到一半才发现参数不对强。
    let stored = u32_le(h, 18);
    let calc = crc32_ieee(&h[0..18]);
    if stored != calc {
        return Err(anyhow!(
            "TTA 头 CRC 校验失败：文件 0x{stored:08X} / 计算 0x{calc:08X}"
        ));
    }

    Ok(Header {
        channels,
        bits,
        rate,
        samples,
        base,
    })
}

/// 前置 ID3v2 标签长度；没有就返回 0。
fn id3v2_prefix_len(b: &[u8]) -> usize {
    if b.len() < 10 || &b[0..3] != b"ID3" {
        return 0;
    }
    // ID3v2 的 size 是 4 个“syncsafe”字节：每字节只用低 7 位
    let size = ((b[6] & 0x7F) as usize) << 21
        | ((b[7] & 0x7F) as usize) << 14
        | ((b[8] & 0x7F) as usize) << 7
        | (b[9] & 0x7F) as usize;
    // flags bit4 表示后面还有一个 10 字节 footer
    let footer = if b[5] & 0x10 != 0 { 10 } else { 0 };
    10usize.saturating_add(size).saturating_add(footer)
}

/// 解码一个 TTA 文件为 32-bit float WAV。
///
/// 总体布局（与 FFmpeg muxer 一致）：
/// `22 字节头 | seek 表(total_frames * u32) + u32 CRC | 帧0 | 帧1 | ...`
pub fn decode(input: &Path, output: &Path) -> Result<WavInfo> {
    let data = read_all(input).with_context(|| format!("读 TTA 文件失败：{}", input.display()))?;
    let h = parse_header(&data)?;
    let channels = h.channels as usize;
    let bps_bytes = (h.bits / 8) as usize;

    let frame_len = frame_length(h.rate)?;
    let samples = h.samples as u64;
    // 最后一帧可能不满；若整除，则最后一帧也是满帧
    let tail = (samples % frame_len as u64) as usize;
    let last_len = if tail == 0 { frame_len } else { tail };
    let total_frames = (samples / frame_len as u64 + u64::from(last_len < frame_len)) as usize;

    // ── seek 表 ──────────────────────────────────────────────
    // 每个 u32 是一帧的字节数，之后还有 4 字节 CRC。我们不校验这个 CRC，
    // 但必须跳过 seek 表才能找到第一帧；同时用每帧长度把位流限制在帧内，
    // 这样即使某一帧内部有问题，也不会读穿到下一帧。
    let seek_off = h.base + 22;
    let seek_bytes = total_frames
        .checked_mul(4)
        .ok_or_else(|| anyhow!("TTA seek 表大小溢出"))?;
    let audio_start = seek_off
        .checked_add(seek_bytes)
        .and_then(|v| v.checked_add(4))
        .ok_or_else(|| anyhow!("TTA seek 表偏移溢出"))?;
    if audio_start > data.len() {
        return Err(anyhow!(
            "TTA seek 表不完整：需要到 {audio_start} 字节，文件只有 {} 字节",
            data.len()
        ));
    }

    let mut sink = WavSink::create(output, h.rate, h.channels as u16)
        .with_context(|| format!("创建 WAV 失败：{}", output.display()))?;
    // f32 缩放系数：有符号定点满量程 2^(bits-1)。
    // 24-bit 时 ffmpeg 内部把它放进 S32 的高 24 位（×256），再按 2^31
    // 归一化，等价于这里的 2^23——所以不需要额外左移 8 位。
    let scale = 1.0f32 / (1u32 << (h.bits - 1)) as f32;

    let mut pos = audio_start;
    let mut decoded_frames: u64 = 0;
    let mut frame_buf: Vec<f32> = Vec::with_capacity(frame_len * channels);

    for fi in 0..total_frames {
        let size = u32_le(&data, seek_off + fi * 4) as usize;
        // 每帧至少要有 4 字节 CRC
        if size < 4 {
            return Err(anyhow!("TTA 第 {fi} 帧长度 {size} 非法"));
        }
        let end = pos
            .checked_add(size)
            .ok_or_else(|| anyhow!("TTA 第 {fi} 帧偏移溢出"))?;
        if end > data.len() {
            return Err(anyhow!(
                "TTA 第 {fi} 帧越界：{pos}..{end}，文件只有 {} 字节",
                data.len()
            ));
        }

        // 帧独立：所有状态每帧重置
        let mut states: Vec<ChannelState> = (0..channels)
            .map(|_| ChannelState::new(bps_bytes))
            .collect();
        let framelen = if fi + 1 == total_frames {
            last_len
        } else {
            frame_len
        };

        let mut br = BitReader::new(&data[pos..end]);
        let mut pcm: Vec<i32> = Vec::with_capacity(framelen * channels);
        for _ in 0..framelen {
            for c in 0..channels {
                let st = &mut states[c];
                let residual = st.rice.decode(&mut br)?;
                let mut value = st.filter.process(residual);
                // 固定一阶预测：8-bit 用 k=4，16/24-bit 用 k=5
                let p = match bps_bytes {
                    1 => pred(st.predictor, 4),
                    2 | 3 => pred(st.predictor, 5),
                    _ => pred(st.predictor, 5),
                };
                value = value.wrapping_add(p);
                st.predictor = value;
                pcm.push(value);
            }
            // 一组（所有声道同一时刻）解完之后再做去相关
            if channels > 1 {
                let start = pcm.len() - channels;
                decorrelate(&mut pcm[start..]);
            }
        }

        // 残差流结束后先字节对齐，再跳过 4 字节帧 CRC。
        // 这里不校验 CRC（任务允许跳过），但必须跳过它才能和 seek 表里的
        // 帧长度对上；同时检查解出的数据没有吃掉 CRC 的空间。
        br.align();
        let used = br.byte_pos();
        if used + 4 > size {
            return Err(anyhow!(
                "TTA 第 {fi} 帧长度不足：解出 {used} 字节 + 4 字节 CRC > {size}"
            ));
        }

        frame_buf.clear();
        for &s in &pcm {
            frame_buf.push(s as f32 * scale);
        }
        sink.write(&frame_buf)?;

        decoded_frames += framelen as u64;
        pos = end;
    }

    if decoded_frames != samples {
        return Err(anyhow!(
            "TTA 样本数不一致：头里 {samples}，实际解出 {decoded_frames}"
        ));
    }

    sink.finish()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::{Path, PathBuf};
    use std::process::Command;

    /// 找 ffmpeg：环境变量 → 本机固定路径 → PATH。
    /// 测试机没有 ffmpeg 时打印原因并跳过，不把整个工作区卡红。
    fn find_ffmpeg() -> Option<PathBuf> {
        if let Some(p) = std::env::var_os("TONELORE_FFMPEG") {
            let p = PathBuf::from(p);
            if p.is_file() {
                return Some(p);
            }
        }
        for cand in [
            r"D:\Apps\ToneLore\ffmpeg\ffmpeg.exe",
            r"C:\ffmpeg\bin\ffmpeg.exe",
        ] {
            let p = PathBuf::from(cand);
            if p.is_file() {
                return Some(p);
            }
        }
        if let Some(path) = std::env::var_os("PATH") {
            for dir in std::env::split_paths(&path) {
                let p = dir.join("ffmpeg.exe");
                if p.is_file() {
                    return Some(p);
                }
            }
        }
        None
    }

    /// 每个测试独立的临时目录，Drop 时清理
    struct Lab {
        dir: PathBuf,
        ffmpeg: PathBuf,
    }

    impl Lab {
        fn new(tag: &str) -> Option<Lab> {
            let ffmpeg = match find_ffmpeg() {
                Some(p) => p,
                None => {
                    eprintln!("[tta] 跳过 {tag}：找不到 ffmpeg（可设 TONELORE_FFMPEG）");
                    return None;
                }
            };
            let dir =
                std::env::temp_dir().join(format!("tonelore_tta_{}_{}", tag, std::process::id()));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).expect("创建临时目录");
            Some(Lab { dir, ffmpeg })
        }

        fn path(&self, name: &str) -> PathBuf {
            self.dir.join(name)
        }

        fn run(&self, mut cmd: Command, what: &str) {
            let out = cmd
                .output()
                .unwrap_or_else(|e| panic!("{what}：启动 ffmpeg 失败：{e}"));
            assert!(
                out.status.success(),
                "{what} 失败（{:?}）：\n{}",
                out.status,
                String::from_utf8_lossy(&out.stderr)
            );
        }
    }

    impl Drop for Lab {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.dir);
        }
    }

    /// 生成一段左右声道不同、带低频起伏的测试 WAV
    fn make_wav(lab: &Lab, wav: &Path, rate: u32, channels: u16, codec: &str, seconds: f64) {
        let expr = if channels == 2 {
            "0.6*sin(2*PI*440*t)|0.5*sin(2*PI*660*t)+0.1*sin(2*PI*97*t)"
        } else {
            "0.55*sin(2*PI*523.25*t)+0.07*sin(2*PI*71*t)"
        };
        let src = format!("aevalsrc={expr}:s={rate}:d={seconds}");
        let mut cmd = Command::new(&lab.ffmpeg);
        cmd.args(["-y", "-v", "error", "-f", "lavfi", "-i"])
            .arg(&src)
            .args(["-c:a", codec])
            .arg(wav);
        lab.run(cmd, "生成测试 WAV");
    }

    /// 解出 WavSink 写的 32-bit float WAV 的采样
    fn wav_f32(path: &Path) -> Vec<f32> {
        let b = read_all(path).expect("读 WAV");
        assert_eq!(&b[0..4], b"RIFF", "输出不是 RIFF");
        let mut pos = 12usize;
        while pos + 8 <= b.len() {
            let len = u32_le(&b, pos + 4) as usize;
            if &b[pos..pos + 4] == b"data" {
                let end = (pos + 8 + len).min(b.len());
                return b[pos + 8..end]
                    .chunks_exact(4)
                    .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
                    .collect();
            }
            pos += 8 + len + (len & 1);
        }
        panic!("WAV 里没有 data 块");
    }

    /// 完整回路：WAV → ffmpeg 编 TTA → 本解码器 vs ffmpeg 解 f32le
    fn roundtrip(tag: &str, rate: u32, channels: u16, codec: &str, bits: u16, seconds: f64) {
        let lab = match Lab::new(tag) {
            Some(l) => l,
            None => return,
        };
        let wav = lab.path("src.wav");
        let tta = lab.path("test.tta");
        let ours = lab.path("ours.wav");
        let reference = lab.path("ref.f32");

        make_wav(&lab, &wav, rate, channels, codec, seconds);

        // 1) ffmpeg 编码成 TTA
        let mut cmd = Command::new(&lab.ffmpeg);
        cmd.args(["-y", "-v", "error", "-i"])
            .arg(&wav)
            .args(["-c:a", "tta"])
            .arg(&tta);
        lab.run(cmd, "ffmpeg 编码 TTA");

        // 确认测试真的覆盖了目标位深/声道/采样率
        let tb = read_all(&tta).expect("读 TTA");
        assert_eq!(u16_le(&tb, 6), channels, "TTA 声道数");
        assert_eq!(u16_le(&tb, 8), bits, "TTA 位深（{tag}）");
        assert_eq!(u32_le(&tb, 10), rate, "TTA 采样率");

        // 2) 本解码器
        let info = decode(&tta, &ours).unwrap_or_else(|e| panic!("{tag} 解码失败: {e:#}"));
        assert_eq!(info.rate, rate);
        assert_eq!(info.channels, channels);
        let want_samples = u32_le(&tb, 14) as u64;
        assert_eq!(info.frames, want_samples, "帧数应等于 TTA 头里的样本数");

        // 3) ffmpeg 参考解码（原始 f32，小端，交织）
        let mut cmd = Command::new(&lab.ffmpeg);
        cmd.args(["-y", "-v", "error", "-i"])
            .arg(&tta)
            .args(["-f", "f32le"])
            .arg(&reference);
        lab.run(cmd, "ffmpeg 参考解码");

        let got = wav_f32(&ours);
        let rb = read_all(&reference).expect("读 ref.f32");
        let expect: Vec<f32> = rb
            .chunks_exact(4)
            .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
            .collect();
        assert_eq!(
            got.len(),
            expect.len(),
            "{tag} 样本数不一致：本解码器 {} vs ffmpeg {}",
            got.len(),
            expect.len()
        );

        let mut max_diff = 0.0f32;
        let mut max_at = 0usize;
        for (i, (a, b)) in got.iter().zip(expect.iter()).enumerate() {
            let d = (a - b).abs();
            if d > max_diff {
                max_diff = d;
                max_at = i;
            }
        }
        println!(
            "[tta] {tag}: 每声道 {} 样本 / 共 {} 个 f32 / {} Hz / {} ch / {} bit，最大误差 {:.3e}（第 {} 个 f32）",
            info.frames,
            got.len(),
            rate,
            channels,
            bits,
            max_diff,
            max_at
        );
        assert!(
            max_diff < 1e-6,
            "{tag} 与 ffmpeg 参考解码不一致：最大误差 {max_diff:e}，第 {max_at} 个样本（{} vs {}）",
            got[max_at],
            expect[max_at]
        );
    }

    #[test]
    fn matches_ffmpeg_16bit_stereo_44k() {
        roundtrip("s16_44k_stereo", 44_100, 2, "pcm_s16le", 16, 1.7);
    }

    #[test]
    fn matches_ffmpeg_24bit_stereo_48k() {
        roundtrip("s24_48k_stereo", 48_000, 2, "pcm_s24le", 24, 2.2);
    }

    #[test]
    fn matches_ffmpeg_8bit_mono_44k() {
        roundtrip("u8_44k_mono", 44_100, 1, "pcm_u8", 8, 1.3);
    }

    #[test]
    fn matches_ffmpeg_24bit_mono_48k() {
        // 同时覆盖 24-bit、单声道、48 kHz 三个条件，并跨多个完整帧
        roundtrip("s24_48k_mono", 48_000, 1, "pcm_s24le", 24, 3.3);
    }

    #[test]
    fn id3v2_prefix_len_uses_syncsafe_size() {
        // "ID3" + 版本 4.0 + flags=0 + syncsafe size 字节 [1,2,3,0]
        // （size 每个字节只用低 7 位，拼出来是 2_130_304）
        let mut b = Vec::new();
        b.extend_from_slice(b"ID3\x04\x00\x00");
        b.extend_from_slice(&[0x01, 0x02, 0x03, 0x00]);
        assert_eq!(id3v2_prefix_len(&b), 10 + 2_130_304);
        // footer 标志（flags bit4）再加 10 字节
        b[5] = 0x10;
        assert_eq!(id3v2_prefix_len(&b), 10 + 2_130_304 + 10);
        // 不是 ID3 就是 0
        assert_eq!(id3v2_prefix_len(b"TTA1xxxxxx"), 0);
    }

    #[test]
    fn rejects_bad_header_instead_of_panicking() {
        let dir = std::env::temp_dir().join(format!("tonelore_tta_bad_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("创建临时目录");

        // 太短：连 22 字节头都不够
        let short = dir.join("short.tta");
        std::fs::write(&short, b"TTA1").unwrap();
        assert!(decode(&short, &dir.join("a.wav")).is_err());

        // 头字段合理但 CRC 错误
        let mut h = Vec::new();
        h.extend_from_slice(b"TTA1");
        h.extend_from_slice(&1u16.to_le_bytes());
        h.extend_from_slice(&2u16.to_le_bytes());
        h.extend_from_slice(&16u16.to_le_bytes());
        h.extend_from_slice(&44_100u32.to_le_bytes());
        h.extend_from_slice(&1_000u32.to_le_bytes());
        h.extend_from_slice(&0u32.to_le_bytes()); // 故意不写正确 CRC
        let bad = dir.join("badcrc.tta");
        std::fs::write(&bad, &h).unwrap();
        let err = decode(&bad, &dir.join("b.wav")).unwrap_err();
        assert!(
            format!("{err:#}").contains("CRC"),
            "期望 CRC 错误，实际：{err:#}"
        );

        // 非 TTA 文件
        let other = dir.join("other.bin");
        std::fs::write(&other, vec![0x55u8; 64]).unwrap();
        assert!(decode(&other, &dir.join("c.wav")).is_err());

        let _ = std::fs::remove_dir_all(&dir);
    }
}
