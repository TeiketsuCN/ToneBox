//! FLAC 原生解码器：只用 std + anyhow，不依赖任何第三方 FLAC 库。
//!
//! 解码路径很短：`fLaC` 魔数 → 元数据块（只保留 STREAMINFO，其余整块跳过）→
//! 一帧一帧解（帧头 → 每声道子帧 → 声道去相关）→ 每帧立刻写进 `WavSink`。
//!
//! 为什么逐帧写：长曲目解出来的 f32 采样能到几百 MB，攒在一个 Vec 里既浪费内存
//! 也推迟了落盘；`WavSink` 内部是 BufWriter，按帧写不会变成每帧一次系统调用。

use std::path::Path;

use anyhow::{anyhow, bail, Context, Result};

use super::{read_all, u16_be, u64_be, WavInfo, WavSink};

/// STREAMINFO 里真正参与解码的字段。
struct StreamInfo {
    rate: u32,
    channels: u32,
    bits: u32,
    /// 0 表示编码器没有写总样本数，此时靠帧的 block size 累加
    total_samples: u64,
}

/// 一帧的帧头。`rate` / `bits` 已经按 STREAMINFO 补全过。
struct FrameHeader {
    block_size: u32,
    rate: u32,
    channels: u32,
    /// 4 bit 的声道分配：0..=7 独立，8 left/side，9 right/side，10 mid/side
    assignment: u8,
    bits: u32,
    /// 固定块长时是帧号，可变块长时是首样本号；只用于报错定位
    number: u64,
    variable: bool,
}

struct DecodedFrame {
    header: FrameHeader,
    /// 已去相关、已缩放成交织 f32
    samples: Vec<f32>,
    /// 下一帧的起始字节偏移（帧尾 CRC-16 之后）
    end: usize,
}

/// 报错时区分帧头里的数字是帧号还是首样本号。
fn frame_number_kind(header: &FrameHeader) -> &'static str {
    if header.variable {
        "样本号"
    } else {
        "帧号"
    }
}

/// 从 FLAC 解码成 32-bit float WAV。
pub fn decode(input: &Path, output: &Path) -> Result<WavInfo> {
    let data = read_all(input)?;
    let (stream, mut pos) = parse_metadata(&data)
        .with_context(|| format!("解析 {} 的 FLAC 元数据失败", input.display()))?;

    let mut sink: Option<WavSink> = None;
    let mut frame_index = 0u64;
    let mut written = 0u64;

    loop {
        if pos >= data.len() {
            break;
        }
        let rest = &data[pos..];
        if !has_frame_sync(rest) {
            // 文件尾常见的 ID3v1 / 全零填充不算损坏；其余位置断同步就报错
            if looks_like_trailing(rest) {
                break;
            }
            bail!(
                "第 {pos} 字节处丢失帧同步（读到 {:02X} {:02X}）",
                rest[0],
                rest.get(1).copied().unwrap_or(0)
            );
        }

        let frame = decode_frame(&data, pos, &stream).with_context(|| {
            format!("解码第 {} 帧（偏移 {pos} 字节）失败", frame_index + 1)
        })?;
        if frame.end <= pos {
            bail!("第 {} 帧长度非正，拒绝继续解码", frame_index + 1);
        }

        if sink.is_none() {
            // 需求：帧头与 STREAMINFO 冲突时以帧头为准。WAV 头一旦写下就无法改，
            // 所以第一帧在这里定下采样率/声道数。
            sink = Some(WavSink::create(
                output,
                frame.header.rate,
                frame.header.channels as u16,
            )?);
        }
        let sink = sink.as_mut().expect("sink 刚刚初始化");
        if frame.header.channels != sink.channels() as u32 {
            bail!(
                "第 {} 帧（{} {}）声道数 {} 与首帧 {} 不一致，WAV 头无法中途改变",
                frame_index + 1,
                frame_number_kind(&frame.header),
                frame.header.number,
                frame.header.channels,
                sink.channels()
            );
        }
        if frame.header.rate != sink.rate() {
            bail!(
                "第 {} 帧（{} {}）采样率 {} Hz 与首帧 {} Hz 不一致，WAV 头无法中途改变",
                frame_index + 1,
                frame_number_kind(&frame.header),
                frame.header.number,
                frame.header.rate,
                sink.rate()
            );
        }

        sink.write(&frame.samples)?;
        written += frame.header.block_size as u64;
        pos = frame.end;
        frame_index += 1;

        // 总样本数已知时提前收尾，避免把尾部标签误当成帧继续啃
        if stream.total_samples != 0 && written >= stream.total_samples {
            break;
        }
    }

    // 没有音频帧也产出一个合法的空 WAV，让上层不用为“空文件”单独开分支
    let sink = match sink {
        Some(s) => s,
        None => WavSink::create(output, stream.rate, stream.channels as u16)?,
    };
    sink.finish()
}

fn has_frame_sync(rest: &[u8]) -> bool {
    rest.len() >= 2 && rest[0] == 0xFF && (rest[1] & 0xFE) == 0xF8
}

fn looks_like_trailing(rest: &[u8]) -> bool {
    // 最小帧（空帧头 + CRC）也要 6 字节；更短的不可能是帧
    rest.len() < 6
        || rest.starts_with(b"TAG")
        || rest.starts_with(b"ID3")
        || rest.iter().all(|&b| b == 0)
}

fn parse_metadata(data: &[u8]) -> Result<(StreamInfo, usize)> {
    let mut pos = 0usize;

    // 有的抓轨工具会在 FLAC 前塞 ID3v2；直接跳过，别把标签当损坏数据
    if data.len() >= 10 && &data[0..3] == b"ID3" {
        let size = ((data[6] as usize & 0x7F) << 21)
            | ((data[7] as usize & 0x7F) << 14)
            | ((data[8] as usize & 0x7F) << 7)
            | (data[9] as usize & 0x7F);
        let footer = if data[5] & 0x10 != 0 { 10 } else { 0 };
        pos = 10 + size + footer;
        if pos > data.len() {
            bail!("文件开头的 ID3v2 标签长度 {size} 超出文件大小");
        }
    }

    if data.len() < pos + 4 || &data[pos..pos + 4] != b"fLaC" {
        bail!("缺少 fLaC 魔数，这不是 FLAC 文件");
    }
    pos += 4;

    let mut stream: Option<StreamInfo> = None;
    loop {
        if pos + 4 > data.len() {
            bail!("元数据块头在偏移 {pos} 字节处被截断");
        }
        let last = data[pos] & 0x80 != 0;
        let kind = data[pos] & 0x7F;
        let len = ((data[pos + 1] as usize) << 16)
            | ((data[pos + 2] as usize) << 8)
            | (data[pos + 3] as usize);
        pos += 4;

        let end = pos
            .checked_add(len)
            .ok_or_else(|| anyhow!("元数据块长度溢出"))?;
        if end > data.len() {
            bail!("元数据块（类型 {kind}）声明长度 {len} 超出文件大小");
        }

        if kind == 0 {
            if stream.is_some() {
                bail!("FLAC 里出现重复的 STREAMINFO 元数据块");
            }
            stream = Some(parse_streaminfo(&data[pos..end])?);
        }
        pos = end;
        if last {
            break;
        }
    }

    let stream = stream.ok_or_else(|| anyhow!("FLAC 缺少 STREAMINFO 元数据块"))?;
    Ok((stream, pos))
}

fn parse_streaminfo(b: &[u8]) -> Result<StreamInfo> {
    if b.len() < 34 {
        bail!("STREAMINFO 只有 {} 字节，至少需要 34 字节", b.len());
    }
    let min_block = u16_be(b, 0);
    let max_block = u16_be(b, 2);
    if min_block == 0 || max_block == 0 || min_block > max_block {
        bail!("STREAMINFO 的块长范围非法：{min_block}..{max_block}");
    }

    // 采样率 20 bit、声道数-1 3 bit、位深-1 5 bit、总样本数 36 bit，正好 64 bit
    let packed = u64_be(b, 10);
    let rate = (packed >> 44) as u32;
    let channels = (((packed >> 41) & 0x7) + 1) as u32;
    let bits = (((packed >> 36) & 0x1F) + 1) as u32;
    let total_samples = packed & 0x0F_FFFF_FFFF;

    if rate == 0 {
        bail!("STREAMINFO 的采样率为 0");
    }
    // FLAC 规范允许 4..=32 bit；范围外的值说明文件坏了
    if !(4..=32).contains(&bits) {
        bail!("STREAMINFO 的位深 {bits} 超出 FLAC 的 4..32 范围");
    }
    Ok(StreamInfo {
        rate,
        channels,
        bits,
        total_samples,
    })
}

fn read_frame_header<'a>(
    data: &'a [u8],
    start: usize,
    stream: &StreamInfo,
) -> Result<(FrameHeader, BitReader<'a>)> {
    let mut r = BitReader::new(data, start)?;

    if r.read_bits(14)? != 0x3FFE {
        bail!("偏移 {start} 字节处不是 FLAC 帧同步码");
    }
    if r.read_bits(1)? != 0 {
        bail!("帧头的保留位不是 0");
    }
    let variable = r.read_bits(1)? == 1;

    let block_size_code = r.read_bits(4)? as u32;
    let rate_code = r.read_bits(4)? as u32;
    let channel_code = r.read_bits(4)? as u32;
    let bits_code = r.read_bits(3)? as u32;
    if r.read_bits(1)? != 0 {
        bail!("帧头的第二个保留位不是 0");
    }

    let number = read_utf8_number(&mut r)?;

    let block_size = match block_size_code {
        0 => bail!("帧头使用了保留的块长编码 0"),
        1 => 192,
        n @ 2..=5 => 576u32 << (n - 2),
        6 => r.read_bits(8)? as u32 + 1,
        7 => r.read_bits(16)? as u32 + 1,
        n @ 8..=15 => 256u32 << (n - 8),
        _ => unreachable!(),
    };
    if block_size == 0 {
        bail!("帧头给出的块长为 0");
    }

    let rate = match rate_code {
        0 => stream.rate,
        1 => 88_200,
        2 => 176_400,
        3 => 192_000,
        4 => 8_000,
        5 => 16_000,
        6 => 22_050,
        7 => 24_000,
        8 => 32_000,
        9 => 44_100,
        10 => 48_000,
        11 => 96_000,
        12 => r.read_bits(8)? as u32 * 1_000,
        13 => r.read_bits(16)? as u32,
        14 => r.read_bits(16)? as u32 * 10,
        15 => bail!("帧头使用了保留的采样率编码 15"),
        _ => unreachable!(),
    };
    if rate == 0 {
        bail!("帧头给出的采样率为 0");
    }

    let channels = match channel_code {
        0..=7 => channel_code + 1,
        8..=10 => 2,
        _ => bail!("帧头使用了保留的声道分配 {channel_code}"),
    };

    let bits = match bits_code {
        0 => stream.bits,
        1 => 8,
        2 => 12,
        3 => bail!("帧头使用了保留的位深编码 3"),
        4 => 16,
        5 => 20,
        6 => 24,
        7 => 32,
        _ => unreachable!(),
    };
    if !(4..=32).contains(&bits) {
        bail!("帧头给出的位深 {bits} 非法");
    }

    // 帧头末尾是 CRC-8，覆盖从同步码到这里的全部字节；校验它能挡住假同步
    let crc_pos = r.byte_pos();
    let expect_crc = *data
        .get(crc_pos)
        .ok_or_else(|| anyhow!("帧头 CRC-8 在偏移 {crc_pos} 字节处缺失"))?;
    let actual_crc = crc8(&data[start..crc_pos]);
    if actual_crc != expect_crc {
        bail!(
            "帧头 CRC-8 校验失败（偏移 {start} 字节：计算 {actual_crc:#04X}，文件 {expect_crc:#04X}）"
        );
    }
    r.read_bits(8)?;

    Ok((
        FrameHeader {
            block_size,
            rate,
            channels,
            assignment: channel_code as u8,
            bits,
            number,
            variable,
        },
        r,
    ))
}

/// FLAC 的帧号/样本号用 UTF-8 变体编码，最长 7 字节（36 bit 样本号）。
fn read_utf8_number(r: &mut BitReader) -> Result<u64> {
    let first = r.read_bits(8)? as u8;
    if first & 0x80 == 0 {
        return Ok(first as u64);
    }

    let (follow, mut value) = if first & 0xE0 == 0xC0 {
        (1usize, (first & 0x1F) as u64)
    } else if first & 0xF0 == 0xE0 {
        (2, (first & 0x0F) as u64)
    } else if first & 0xF8 == 0xF0 {
        (3, (first & 0x07) as u64)
    } else if first & 0xFC == 0xF8 {
        (4, (first & 0x03) as u64)
    } else if first & 0xFE == 0xFC {
        (5, (first & 0x01) as u64)
    } else if first == 0xFE {
        (6, 0)
    } else {
        bail!("帧号 UTF-8 的首字节 {first:#04X} 非法");
    };

    for _ in 0..follow {
        let b = r.read_bits(8)? as u8;
        if b & 0xC0 != 0x80 {
            bail!("帧号 UTF-8 的续字节 {b:#04X} 非法");
        }
        value = (value << 6) | (b & 0x3F) as u64;
    }
    Ok(value)
}

fn decode_frame(data: &[u8], start: usize, stream: &StreamInfo) -> Result<DecodedFrame> {
    let (header, mut r) = read_frame_header(data, start, stream)?;
    let n = header.block_size as usize;

    let mut channels: Vec<Vec<i64>> = Vec::with_capacity(header.channels as usize);
    for ch in 0..header.channels {
        // 去相关声道（side）比普通声道多 1 bit，这是 FLAC 防止差值溢出的约定；
        // 10 = mid/side 时第 2 声道是 side，也适用
        let side = match header.assignment {
            8 => ch == 1,  // left/side
            9 => ch == 0,  // right/side
            10 => ch == 1, // mid/side
            _ => false,
        };
        let bps = header.bits + u32::from(side);
        channels.push(decode_subframe(&mut r, bps, header.block_size)?);
    }

    // 子帧到字节边界后有两个字节的帧尾 CRC-16；不校验它不影响正确性，
    // 但必须跳过去才能找到下一帧
    r.align()?;
    r.read_bits(16)?;
    let end = r.byte_pos();

    for (i, c) in channels.iter().enumerate() {
        if c.len() != n {
            bail!("第 {i} 声道解出 {} 个样本，与块长 {n} 不符", c.len());
        }
    }

    match header.assignment {
        8 => {
            // left/side：side = left - right，所以 right = left - side
            let (left, right) = channels.split_at_mut(1);
            for i in 0..n {
                right[0][i] = left[0][i].wrapping_sub(right[0][i]);
            }
        }
        9 => {
            // right/side：side = left - right，所以 left = right + side
            let (side, right) = channels.split_at_mut(1);
            for i in 0..n {
                side[0][i] = right[0][i].wrapping_add(side[0][i]);
            }
        }
        10 => {
            let (mid_c, side_c) = channels.split_at_mut(1);
            let mid = &mut mid_c[0];
            let side = &mut side_c[0];
            for i in 0..n {
                let s = side[i];
                // mid 在编码时右移过 1 位，丢掉的最低位由 side 的最低位带回
                let m = mid[i].wrapping_shl(1) | (s & 1);
                mid[i] = m.wrapping_add(s) >> 1;
                side[i] = m.wrapping_sub(s) >> 1;
            }
        }
        _ => {}
    }

    // 按源位深归一化：8 bit -> 1/128，16 bit -> 1/32768，24/32 bit 同理
    let scale = 1.0f32 / (1u64 << (header.bits - 1)) as f32;
    let mut samples = Vec::with_capacity(n * header.channels as usize);
    for i in 0..n {
        for c in &channels {
            samples.push(c[i] as f32 * scale);
        }
    }

    Ok(DecodedFrame {
        header,
        samples,
        end,
    })
}

fn decode_subframe(r: &mut BitReader, bps: u32, block_size: u32) -> Result<Vec<i64>> {
    // 32 bit 源 + side 声道会到 33 bit；实现上限留到 56 位足够覆盖
    if bps == 0 || bps > 56 {
        bail!("子帧位深 {bps} 非法");
    }
    if r.read_bits(1)? != 0 {
        bail!("子帧的零填充位不是 0");
    }

    let kind = r.read_bits(6)? as u32;
    let wasted = if r.read_bits(1)? == 1 {
        // wasted bits 用一元码存 k-1 个 0 再跟一个 1，所以实际位数是零的个数 + 1
        let zeros = r.read_unary()?;
        let wasted = zeros + 1;
        if wasted >= bps {
            bail!("wasted bits（{wasted}）不小于子帧位深（{bps}）");
        }
        wasted
    } else {
        0
    };
    // 先按“丢掉 wasted 位之后”的位深去解，最后再整体左移补回来
    let effective_bits = bps - wasted;
    let n = block_size as usize;

    let mut out = match kind {
        0 => {
            let v = r.read_signed(effective_bits)?;
            vec![v; n]
        }
        1 => {
            let mut v = Vec::with_capacity(n);
            for _ in 0..n {
                v.push(r.read_signed(effective_bits)?);
            }
            v
        }
        8..=15 => {
            let order = (kind - 8) as usize;
            if order > 4 {
                bail!("FIXED 子帧的阶数 {order} 超出 0..=4");
            }
            decode_fixed(r, effective_bits, n, order)?
        }
        32..=63 => {
            let order = (kind - 31) as usize;
            decode_lpc(r, effective_bits, n, order)?
        }
        _ => bail!("遇到保留的子帧类型 {kind:#08b}"),
    };

    if wasted > 0 {
        for v in &mut out {
            *v = v.wrapping_shl(wasted);
        }
    }
    Ok(out)
}

fn decode_fixed(r: &mut BitReader, bps: u32, n: usize, order: usize) -> Result<Vec<i64>> {
    let mut out = Vec::with_capacity(n);
    for _ in 0..order {
        out.push(r.read_signed(bps)?);
    }
    let residual = decode_residual(r, n, order)?;
    for &res in &residual {
        let len = out.len();
        // 四种固定预测器；都用 wrapping 运算，损坏数据最多解出噪声，不会 panic
        let pred = match order {
            0 => 0,
            1 => out[len - 1],
            2 => out[len - 1]
                .wrapping_mul(2)
                .wrapping_sub(out[len - 2]),
            3 => out[len - 1]
                .wrapping_mul(3)
                .wrapping_sub(out[len - 2].wrapping_mul(3))
                .wrapping_add(out[len - 3]),
            4 => out[len - 1]
                .wrapping_mul(4)
                .wrapping_sub(out[len - 2].wrapping_mul(6))
                .wrapping_add(out[len - 3].wrapping_mul(4))
                .wrapping_sub(out[len - 4]),
            _ => unreachable!(),
        };
        out.push(res.wrapping_add(pred));
    }
    Ok(out)
}

fn decode_lpc(r: &mut BitReader, bps: u32, n: usize, order: usize) -> Result<Vec<i64>> {
    let mut out = Vec::with_capacity(n);
    for _ in 0..order {
        out.push(r.read_signed(bps)?);
    }

    let precision_raw = r.read_bits(4)? as u32;
    if precision_raw == 15 {
        bail!("LPC 系数量化精度用了保留值 1111");
    }
    let precision = precision_raw + 1;
    let shift = r.read_signed(5)? as i32;

    let mut coef = Vec::with_capacity(order);
    for _ in 0..order {
        coef.push(r.read_signed(precision)?);
    }

    let residual = decode_residual(r, n, order)?;
    for &res in &residual {
        let base = out.len() - order;
        let mut sum: i64 = 0;
        for (j, &c) in coef.iter().enumerate() {
            sum = sum.wrapping_add(c.wrapping_mul(out[base + order - 1 - j]));
        }
        // qlp shift 正常是右移，但负值在规范里代表左移，这里按边界处理
        let pred = if shift >= 0 {
            sum >> shift
        } else {
            sum.wrapping_shl((-shift) as u32)
        };
        out.push(res.wrapping_add(pred));
    }
    Ok(out)
}

fn decode_residual(r: &mut BitReader, n: usize, order: usize) -> Result<Vec<i64>> {
    let method = r.read_bits(2)? as u32;
    let param_bits = match method {
        0 => 4u32,
        1 => 5u32,
        _ => bail!("残差使用了保留的 Rice 编码方法 {method}"),
    };
    let escape = (1u64 << param_bits) - 1;

    let partition_order = r.read_bits(4)? as u32;
    let partitions = 1u32 << partition_order;
    if n as u32 % partitions != 0 {
        bail!("块长 {n} 无法均分成 2^{partition_order} 个 Rice 分区");
    }
    let partition_samples = n as u32 / partitions;
    if partition_order > 0 && partition_samples < order as u32 {
        bail!(
            "Rice 首分区只有 {partition_samples} 个样本，放不下 {order} 阶预测的预热样本"
        );
    }
    if n < order {
        bail!("块长 {n} 小于预测阶数 {order}");
    }

    let mut out = Vec::with_capacity(n - order);
    for p in 0..partitions {
        // 首分区要让出 order 个预热样本，所以比其它分区短
        let count = if p == 0 {
            partition_samples - order as u32
        } else {
            partition_samples
        } as usize;

        let param = r.read_bits(param_bits)?;
        if param == escape {
            // escape 分区直接存定长原始残差；位宽为 0 表示整段都是 0
            let width = r.read_bits(5)? as u32;
            for _ in 0..count {
                out.push(if width == 0 {
                    0
                } else {
                    r.read_signed(width)?
                });
            }
        } else {
            for _ in 0..count {
                let q = r.read_unary()? as u64;
                let rem = if param == 0 {
                    0
                } else {
                    r.read_bits(param as u32)?
                };
                let v = q
                    .checked_mul(1u64 << param)
                    .map(|v| v | rem)
                    .ok_or_else(|| anyhow!("Rice 残差超出 64 位范围，数据可能已损坏"))?;
                // Rice 码存的是 zigzag 值：偶数 -> 正，奇数 -> 负
                out.push(((v >> 1) as i64) ^ -((v & 1) as i64));
            }
        }
    }

    if out.len() != n - order {
        bail!("残差数量 {} 与期望 {} 不符", out.len(), n - order);
    }
    Ok(out)
}

/// FLAC 帧头 CRC-8：多项式 x^8 + x^2 + x + 1（0x07），初值 0。
fn crc8(data: &[u8]) -> u8 {
    let mut crc = 0u8;
    for &byte in data {
        crc ^= byte;
        for _ in 0..8 {
            crc = if crc & 0x80 != 0 {
                (crc << 1) ^ 0x07
            } else {
                crc << 1
            };
        }
    }
    crc
}

/// 按位读取 FLAC 的大端位流。
struct BitReader<'a> {
    data: &'a [u8],
    /// 当前位偏移
    pos: usize,
    /// 位终点（`data.len() * 8`）
    end: usize,
}

impl<'a> BitReader<'a> {
    fn new(data: &'a [u8], byte_pos: usize) -> Result<Self> {
        if byte_pos > data.len() {
            bail!("位流起点 {byte_pos} 超出文件长度 {}", data.len());
        }
        Ok(Self {
            data,
            pos: byte_pos * 8,
            end: data.len() * 8,
        })
    }

    fn bits_left(&self) -> usize {
        self.end - self.pos
    }

    fn read_bits(&mut self, n: u32) -> Result<u64> {
        // 调用方最多读 33 位；限制 56 位是为了避免 u64 移位越界
        if n > 56 {
            bail!("单次读取 {n} 位超出实现上限 56");
        }
        if self.bits_left() < n as usize {
            bail!(
                "FLAC 数据在偏移 {} 字节处提前结束（还需要 {n} 位）",
                self.pos >> 3
            );
        }

        let mut value = 0u64;
        let mut left = n;
        while left > 0 {
            let byte = self.data[self.pos >> 3] as u64;
            let off = self.pos & 7;
            let avail = 8 - off;
            let take = avail.min(left as usize);
            let shift = avail - take;
            let mask = (1u64 << take) - 1;
            value = (value << take) | ((byte >> shift) & mask);
            self.pos += take;
            left -= take as u32;
        }
        Ok(value)
    }

    fn read_signed(&mut self, n: u32) -> Result<i64> {
        if n == 0 {
            return Ok(0);
        }
        let v = self.read_bits(n)?;
        let sign = 1u64 << (n - 1);
        if v & sign != 0 {
            Ok(v as i64 - (1i64 << n))
        } else {
            Ok(v as i64)
        }
    }

    /// 读一元码，返回 1 之前 0 的个数。
    fn read_unary(&mut self) -> Result<u32> {
        let mut count: u32 = 0;
        loop {
            if self.pos >= self.end {
                bail!("一元码在文件末尾被截断");
            }
            let byte = self.data[self.pos >> 3];
            let off = self.pos & 7;
            // 只保留当前字节里还没消费的低位
            let v = byte & (0xFFu8 >> off);
            if v == 0 {
                count = count.saturating_add((8 - off) as u32);
                self.pos += 8 - off;
                continue;
            }
            let zeros = v.leading_zeros() - off as u32;
            count = count.saturating_add(zeros);
            self.pos += zeros as usize + 1;
            return Ok(count);
        }
    }

    fn align(&mut self) -> Result<()> {
        let aligned = (self.pos + 7) & !7;
        if aligned > self.end {
            bail!("FLAC 帧在字节对齐的填充位处提前结束");
        }
        self.pos = aligned;
        Ok(())
    }

    fn byte_pos(&self) -> usize {
        self.pos / 8
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::OsStr;
    use std::path::PathBuf;
    use std::process::Command;

    const FFMPEG: &str = r"D:\Apps\ToneLore\ffmpeg\ffmpeg.exe";

    struct TempDir {
        path: PathBuf,
    }

    impl TempDir {
        fn new(tag: &str) -> Self {
            let mut path = std::env::temp_dir();
            path.push(format!("tonelore_flac_{tag}_{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&path);
            std::fs::create_dir_all(&path).expect("创建临时目录失败");
            Self { path }
        }

        fn file(&self, name: &str) -> PathBuf {
            self.path.join(name)
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.path);
        }
    }

    fn run(mut cmd: Command, what: &str) {
        let out = cmd
            .output()
            .unwrap_or_else(|e| panic!("无法启动 ffmpeg（{what}）：{e}"));
        assert!(
            out.status.success(),
            "ffmpeg 执行失败（{what}）：{cmd:?}\n{}",
            String::from_utf8_lossy(&out.stderr)
        );
    }

    fn ffmpeg(args: impl IntoIterator<Item = impl AsRef<OsStr>>, what: &str) {
        let mut cmd = Command::new(FFMPEG);
        cmd.args(args);
        run(cmd, what);
    }

    /// 用 ffmpeg 的 lavfi 生成双音测试 WAV（单声道只有 440Hz）。
    fn make_source(
        dir: &TempDir,
        name: &str,
        rate: u32,
        channels: u16,
        seconds: f64,
        sample_fmt: &str,
    ) -> PathBuf {
        let expr = match channels {
            1 => "0.4*sin(2*PI*440*t)".to_string(),
            2 => "0.4*sin(2*PI*440*t)|0.4*sin(2*PI*660*t)".to_string(),
            _ => panic!("测试只生成单声道或双声道"),
        };
        let lavfi = format!("aevalsrc={expr}:s={rate}:d={seconds}");
        let path = dir.file(name);
        ffmpeg(
            [
                "-y",
                "-v",
                "error",
                "-f",
                "lavfi",
                "-i",
                &lavfi,
                "-c:a",
                sample_fmt,
                path.to_str().unwrap(),
            ],
            "生成测试 WAV",
        );
        path
    }

    fn encode_flac(dir: &TempDir, src: &Path, name: &str, extra: &[&str]) -> PathBuf {
        let path = dir.file(name);
        let mut cmd = Command::new(FFMPEG);
        cmd.args(["-y", "-v", "error", "-i"])
            .arg(src)
            .args(["-c:a", "flac"])
            .args(extra)
            .arg(&path);
        run(cmd, "编码 FLAC");
        path
    }

    fn reference_decode(flac: &Path, dst: &Path) {
        ffmpeg(
            [
                "-y",
                "-v",
                "error",
                "-i",
                flac.to_str().unwrap(),
                "-f",
                "f32le",
                dst.to_str().unwrap(),
            ],
            "参考解码",
        );
    }

    /// 读本机 WavSink 产出的 float32 WAV，返回 (采样率, 声道数, 交织样本)。
    fn read_wav_f32(path: &Path) -> (u32, u16, Vec<f32>) {
        let b = std::fs::read(path).expect("读 WAV 失败");
        assert!(b.len() >= 44, "WAV 太短");
        assert_eq!(&b[0..4], b"RIFF");
        assert_eq!(&b[8..12], b"WAVE");

        let mut pos = 12usize;
        let mut rate = 0u32;
        let mut channels = 0u16;
        let mut samples = Vec::new();
        while pos + 8 <= b.len() {
            let id = &b[pos..pos + 4];
            let len = u32::from_le_bytes(b[pos + 4..pos + 8].try_into().unwrap()) as usize;
            let body = pos + 8;
            assert!(body + len <= b.len(), "WAV chunk 长度越界");
            if id == b"fmt " {
                let format = u16::from_le_bytes(b[body..body + 2].try_into().unwrap());
                assert_eq!(format, 3, "输出的 WAV 不是 IEEE float");
                channels = u16::from_le_bytes(b[body + 2..body + 4].try_into().unwrap());
                rate = u32::from_le_bytes(b[body + 4..body + 8].try_into().unwrap());
                let bits = u16::from_le_bytes(b[body + 14..body + 16].try_into().unwrap());
                assert_eq!(bits, 32, "输出的 WAV 不是 32-bit float");
            } else if id == b"data" {
                assert_eq!(len % 4, 0);
                samples.reserve(len / 4);
                for c in b[body..body + len].chunks_exact(4) {
                    samples.push(f32::from_le_bytes(c.try_into().unwrap()));
                }
            }
            pos = body + len + (len & 1);
        }
        (rate, channels, samples)
    }

    /// 解 FLAC，再和 ffmpeg 的 f32le 参考逐样本比对。
    fn compare_with_ffmpeg(flac: &Path, tag: &str) {
        let out = flac.with_file_name(format!("{tag}.out.wav"));
        let refp = flac.with_file_name(format!("{tag}.ref.f32"));

        let info = decode(flac, &out).unwrap_or_else(|e| panic!("[{tag}] 本机解码失败：{e:#}"));
        reference_decode(flac, &refp);

        let (rate, channels, ours) = read_wav_f32(&out);
        let reference = std::fs::read(&refp).expect("读参考 f32 失败");
        assert_eq!(rate, info.rate, "[{tag}] 采样率不一致");
        assert_eq!(channels as u32, info.channels as u32, "[{tag}] 声道数不一致");
        assert_eq!(info.frames, (ours.len() / channels as usize) as u64);
        assert_eq!(
            ours.len() * 4,
            reference.len(),
            "[{tag}] 样本数不一致：本机 {}，ffmpeg {}",
            ours.len(),
            reference.len() / 4
        );

        let mut max_err = 0f32;
        let mut worst = 0usize;
        for (i, &s) in ours.iter().enumerate() {
            let r = f32::from_le_bytes(reference[i * 4..i * 4 + 4].try_into().unwrap());
            let err = (s - r).abs();
            if err > max_err {
                max_err = err;
                worst = i;
            }
        }
        assert!(
            max_err < 1e-6,
            "[{tag}] 最大误差 {max_err} 超过 1e-6（样本 {worst}：本机 {}，ffmpeg {}）",
            ours[worst],
            f32::from_le_bytes(reference[worst * 4..worst * 4 + 4].try_into().unwrap())
        );
        eprintln!(
            "[{tag}] OK：{} Hz / {} ch / {} 帧，最大误差 {max_err:.3e}",
            rate, channels, info.frames
        );
    }

    #[test]
    fn decode_16bit_stereo_matches_ffmpeg() {
        let dir = TempDir::new("s16");
        let wav = make_source(&dir, "src16.wav", 44_100, 2, 1.0, "pcm_s16le");
        let flac = encode_flac(&dir, &wav, "t16.flac", &[]);
        compare_with_ffmpeg(&flac, "s16");
    }

    #[test]
    fn decode_24bit_stereo_matches_ffmpeg() {
        let dir = TempDir::new("s24");
        let wav = make_source(&dir, "src24.wav", 44_100, 2, 1.0, "pcm_s24le");
        let flac = encode_flac(&dir, &wav, "t24.flac", &["-bits_per_raw_sample", "24"]);
        compare_with_ffmpeg(&flac, "s24");
    }

    #[test]
    fn decode_mono_matches_ffmpeg() {
        let dir = TempDir::new("mono");
        let wav = make_source(&dir, "src_mono.wav", 48_000, 1, 1.0, "pcm_s16le");
        let flac = encode_flac(&dir, &wav, "mono.flac", &["-compression_level", "8"]);
        compare_with_ffmpeg(&flac, "mono");
    }

    #[test]
    fn decode_192khz_matches_ffmpeg() {
        let dir = TempDir::new("192k");
        let wav = make_source(&dir, "src192.wav", 192_000, 2, 0.5, "pcm_s16le");
        let flac = encode_flac(&dir, &wav, "t192.flac", &[]);
        compare_with_ffmpeg(&flac, "192k");
    }

    #[test]
    fn decode_silence_noise_and_fixed_predictor_matches_ffmpeg() {
        let dir = TempDir::new("shapes");

        // 数字静音：编码器会选 CONSTANT 子帧，残差全 0 的分区还会走 Rice escape
        let silence = dir.file("silence.wav");
        ffmpeg(
            [
                "-y",
                "-v",
                "error",
                "-f",
                "lavfi",
                "-i",
                "aevalsrc=0|0:s=44100:d=0.5",
                "-c:a",
                "pcm_s16le",
                silence.to_str().unwrap(),
            ],
            "生成静音 WAV",
        );
        let flac = encode_flac(&dir, &silence, "silence.flac", &["-compression_level", "12"]);
        compare_with_ffmpeg(&flac, "silence");

        // 白噪声几乎不可预测，编码器更可能用 VERBATIM / 大 Rice 参数
        let noise = dir.file("noise.wav");
        ffmpeg(
            [
                "-y",
                "-v",
                "error",
                "-f",
                "lavfi",
                "-i",
                "anoisesrc=d=0.3:c=white:r=44100:a=0.5",
                "-c:a",
                "pcm_s16le",
                noise.to_str().unwrap(),
            ],
            "生成噪声 WAV",
        );
        let flac = encode_flac(&dir, &noise, "noise.flac", &["-compression_level", "12"]);
        compare_with_ffmpeg(&flac, "noise");

        // 强制只用固定预测器，覆盖 FIXED 0..=4 阶
        let wav = make_source(&dir, "src_fixed.wav", 44_100, 2, 0.5, "pcm_s16le");
        let flac = encode_flac(
            &dir,
            &wav,
            "fixed.flac",
            &[
                "-lpc_type",
                "fixed",
                "-compression_level",
                "12",
                "-max_partition_order",
                "8",
            ],
        );
        compare_with_ffmpeg(&flac, "fixed");
    }

    #[test]
    fn decode_long_stream_with_multibyte_frame_numbers_matches_ffmpeg() {
        let dir = TempDir::new("long");
        // 25 秒单声道 = 1,102,500 样本；即便块长 8192 也有 130+ 帧，
        // 帧号会超过 127，必须走 2 字节 UTF-8 编码
        let wav = make_source(&dir, "long.wav", 44_100, 1, 25.0, "pcm_s16le");
        let flac = encode_flac(&dir, &wav, "long.flac", &["-compression_level", "5"]);
        compare_with_ffmpeg(&flac, "long");
    }

    #[test]
    fn decode_all_stereo_decorrelation_modes() {
        let dir = TempDir::new("modes");
        let wav = make_source(&dir, "src_modes.wav", 44_100, 2, 0.5, "pcm_s16le");
        for mode in ["indep", "left_side", "right_side", "mid_side"] {
            let flac = encode_flac(
                &dir,
                &wav,
                &format!("mode_{mode}.flac"),
                &[
                    "-compression_level",
                    "12",
                    "-ch_mode",
                    mode,
                    "-exact_rice_parameters",
                    "1",
                ],
            );
            compare_with_ffmpeg(&flac, &format!("mode_{mode}"));
        }
    }

    /// 直接喂子帧位流，覆盖 ffmpeg 很难稳定造出的三个边界：
    /// wasted bits、LPC 的负 qlp shift、Rice escape 分区。
    #[test]
    fn subframe_wasted_bits_negative_shift_and_escape() {
        // CONSTANT + wasted=2：存 -7，解出来应当是 -28
        let mut w = BitWriter::new();
        w.write_bits(0, 1); // 零填充位
        w.write_bits(0, 6); // CONSTANT
        w.write_bits(1, 1); // wasted bits 标志
        w.write_bits(0, 1); // 一元码：一个 0
        w.write_bits(1, 1); // 再加 1，wasted = 2
        w.write_signed(-7, 14); // 有效位深 16 - 2 = 14
        let mut r = BitReader::new(&w.bytes, 0).unwrap();
        assert_eq!(decode_subframe(&mut r, 16, 4).unwrap(), vec![-28; 4]);

        // LPC order=1、precision=3、shift=-1、coef=2 -> pred = 4 * prev
        let mut w = BitWriter::new();
        w.write_bits(0, 1);
        w.write_bits(32, 6); // LPC，阶数 = 32 - 31 = 1
        w.write_bits(0, 1); // 无 wasted bits
        w.write_signed(1, 16); // 预热样本
        w.write_bits(2, 4); // 精度原始值 2 -> 每个系数 3 bit
        w.write_signed(-1, 5); // shift = -1，负值必须按左移处理
        w.write_signed(2, 3); // coef = 2
        w.write_bits(0, 2); // Rice 方法 0
        w.write_bits(0, 4); // 分区阶 0
        w.write_bits(0, 4); // Rice 参数 0
        for _ in 0..3 {
            // 残差 1 -> zigzag 2 -> 商 2（两个 0 加一个 1）
            w.write_bits(0, 2);
            w.write_bits(1, 1);
        }
        let mut r = BitReader::new(&w.bytes, 0).unwrap();
        assert_eq!(
            decode_subframe(&mut r, 16, 4).unwrap(),
            vec![1, 5, 21, 85],
            "负 qlp shift 必须按左移还原"
        );

        // FIXED 0 阶 + Rice escape（位宽 0）：整段残差都是 0
        let mut w = BitWriter::new();
        w.write_bits(0, 1);
        w.write_bits(8, 6); // FIXED 0 阶
        w.write_bits(0, 1);
        w.write_bits(0, 2); // Rice 方法 0
        w.write_bits(0, 4); // 分区阶 0
        w.write_bits(15, 4); // 4-bit 参数的 escape 值
        w.write_bits(0, 5); // 原始位宽 0
        let mut r = BitReader::new(&w.bytes, 0).unwrap();
        assert_eq!(decode_subframe(&mut r, 16, 4).unwrap(), vec![0; 4]);
    }

    /// 8/32 bit 在 ffmpeg 的 FLAC 编码器里造不出来（它只出 16/24 bit），
    /// 所以这里手写一个最小 FLAC：STREAMINFO + 一个 VERBATIM 帧。
    #[test]
    fn decode_8bit_and_32bit_synthetic() {
        let dir = TempDir::new("synthetic");
        for bits in [8u32, 32] {
            let samples: Vec<i64> = if bits == 8 {
                vec![-128, -127, -64, -1, 0, 1, 64, 126, 127, 0, -1, 1, -32, 32, 5, -5]
            } else {
                vec![
                    i32::MIN as i64,
                    -(1i64 << 30),
                    -123_456_789,
                    -1,
                    0,
                    1,
                    123_456_789,
                    1i64 << 30,
                    i32::MAX as i64,
                    42,
                    -42,
                    7,
                    -7,
                    0,
                    1,
                    -1,
                ]
            };
            let bytes = make_synthetic_flac(bits, 16, &samples);
            let flac = dir.file(&format!("synthetic{bits}.flac"));
            std::fs::write(&flac, &bytes).unwrap();
            let wav = dir.file(&format!("synthetic{bits}.wav"));
            let info = decode(&flac, &wav).unwrap_or_else(|e| panic!("{bits} bit 解码失败：{e:#}"));
            assert_eq!(info.frames, 16);
            let (rate, channels, got) = read_wav_f32(&wav);
            assert_eq!(rate, 44_100);
            assert_eq!(channels, 1);
            assert_eq!(got.len(), samples.len());
            let scale = 1.0f32 / (1u64 << (bits - 1)) as f32;
            for (i, &s) in samples.iter().enumerate() {
                let expect = s as f32 * scale;
                assert!(
                    (got[i] - expect).abs() < 1e-9,
                    "{bits} bit 第 {i} 个样本：得到 {}，期望 {expect}",
                    got[i]
                );
            }
            eprintln!("[synthetic{bits}] OK：{} 个样本", samples.len());
        }
    }

    #[test]
    fn truncated_or_corrupt_frame_returns_error() {
        let dir = TempDir::new("bad");
        let samples: Vec<i64> = (0..16).collect();
        let bytes = make_synthetic_flac(16, 16, &samples);

        // 截掉尾部 3 字节：帧数据不完整，必须报错而不是 panic/死循环
        let truncated = dir.file("truncated.flac");
        std::fs::write(&truncated, &bytes[..bytes.len() - 3]).unwrap();
        let out = dir.file("truncated.wav");
        assert!(decode(&truncated, &out).is_err(), "截断文件应当报错");

        // 打坏帧同步码：应当明确报“丢失帧同步”
        let mut corrupt = bytes.clone();
        let sync_at = corrupt
            .windows(2)
            .position(|w| w == [0xFF, 0xF8])
            .expect("找不到帧同步");
        corrupt[sync_at] = 0x00;
        let corrupt_path = dir.file("corrupt.flac");
        std::fs::write(&corrupt_path, &corrupt).unwrap();
        let err = decode(&corrupt_path, &dir.file("corrupt.wav")).unwrap_err();
        assert!(
            format!("{err:#}").contains("帧同步"),
            "错误信息应当说明同步丢失，实际：{err:#}"
        );
    }

    // ---- 手写最小 FLAC（只用于覆盖 ffmpeg 造不出的 8/32 bit） ----

    struct BitWriter {
        bytes: Vec<u8>,
        /// 当前字节已经写了几位（0 表示下一次写会新开字节）
        used: u8,
    }

    impl BitWriter {
        fn new() -> Self {
            Self {
                bytes: Vec::new(),
                used: 0,
            }
        }

        fn write_bits(&mut self, value: u64, n: u32) {
            for i in (0..n).rev() {
                if self.used == 0 {
                    self.bytes.push(0);
                }
                let bit = ((value >> i) & 1) as u8;
                let last = self.bytes.last_mut().unwrap();
                *last |= bit << (7 - self.used);
                self.used = (self.used + 1) % 8;
            }
        }

        fn write_signed(&mut self, value: i64, n: u32) {
            let mask = if n == 64 { u64::MAX } else { (1u64 << n) - 1 };
            self.write_bits(value as u64 & mask, n);
        }

        fn align(&mut self) {
            // 已经写进 bytes 的那半个字节保留，下一次写会新开字节
            self.used = 0;
        }
    }

    fn make_synthetic_flac(bits: u32, block_size: usize, samples: &[i64]) -> Vec<u8> {
        assert!(block_size <= 256 && samples.len() == block_size);
        let mut file = Vec::new();
        file.extend_from_slice(b"fLaC");

        let mut si = Vec::new();
        si.extend_from_slice(&(block_size as u16).to_be_bytes());
        si.extend_from_slice(&(block_size as u16).to_be_bytes());
        si.extend_from_slice(&[0, 0, 0, 0, 0, 0]); // min/max frame size 未知
        let packed = (44_100u64 << 44)
            | (((1u64) - 1) << 41)
            | (((bits as u64) - 1) << 36)
            | samples.len() as u64;
        si.extend_from_slice(&packed.to_be_bytes());
        si.extend_from_slice(&[0u8; 16]); // MD5 全 0 表示没计算
        file.push(0x80); // last-metadata-block + STREAMINFO
        file.extend_from_slice(&(si.len() as u32).to_be_bytes()[1..]);
        file.extend_from_slice(&si);

        let mut w = BitWriter::new();
        w.write_bits(0x3FFE, 14); // sync
        w.write_bits(0, 1); // reserved
        w.write_bits(0, 1); // 固定块长
        w.write_bits(6, 4); // 8-bit 块长
        w.write_bits(0, 4); // 采样率取 STREAMINFO
        w.write_bits(0, 4); // 1 声道独立
        let bits_code = match bits {
            8 => 1,
            16 => 4,
            24 => 6,
            32 => 7,
            _ => 0, // 其它位深回落到 STREAMINFO
        };
        w.write_bits(bits_code, 3);
        w.write_bits(0, 1); // reserved
        w.write_bits(0, 8); // UTF-8 帧号 0
        w.write_bits((block_size - 1) as u64, 8);
        let crc = crc8(&w.bytes);
        w.write_bits(crc as u64, 8);

        // 一个 VERBATIM 子帧：位深与 STREAMINFO 一致，没有 wasted bits
        w.write_bits(0, 1);
        w.write_bits(1, 6);
        w.write_bits(0, 1);
        for &s in samples {
            w.write_signed(s, bits);
        }
        w.align();
        w.write_bits(0, 16); // 帧尾 CRC-16；本解码器不校验，写 0 即可
        file.extend_from_slice(&w.bytes);
        file
    }
}
