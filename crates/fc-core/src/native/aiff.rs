//! AIFF / AIFC / CAF / AU 读取（大端家族）。
//!
//! 四个容器的共同点：PCM 都是大端、块结构简单。真正麻烦的只有 AIFF 的 80-bit
//! 扩展浮点采样率，和 CAF 的 lpcm 标志位。

use std::path::Path;

use anyhow::{anyhow, Context, Result};

use super::{read_all, u16_be, u32_be, u64_be, WavInfo, WavSink};

// ── AIFF / AIFC ───────────────────────────────────────────
pub fn decode_aiff(input: &Path, output: &Path) -> Result<WavInfo> {
    let b = read_all(input).with_context(|| format!("读 AIFF 失败：{}", input.display()))?;
    if b.len() < 12 || &b[0..4] != b"FORM" {
        return Err(anyhow!("不是 FORM 文件"));
    }
    let is_aifc = &b[8..12] == b"AIFC";
    if &b[8..12] != b"AIFF" && !is_aifc {
        return Err(anyhow!("FORM 里既不是 AIFF 也不是 AIFC"));
    }
    let mut channels = 0u16;
    let mut frames = 0u32;
    let mut bits = 0u16;
    let mut rate = 0u32;
    let mut compression = *b"NONE";
    let mut ssnd_off = 0usize;
    let mut ssnd_len = 0usize;

    let mut pos = 12usize;
    while pos + 8 <= b.len() {
        let id = &b[pos..pos + 4];
        let len = u32_be(&b, pos + 4) as usize;
        let body = pos + 8;
        match id {
            b"COMM" => {
                if body + 18 <= b.len() {
                    channels = u16_be(&b, body);
                    frames = u32_be(&b, body + 2);
                    bits = u16_be(&b, body + 6);
                    rate = ext80_to_u32(&b[body + 8..body + 18]);
                    if is_aifc && body + 22 <= b.len() {
                        compression = [b[body + 18], b[body + 19], b[body + 20], b[body + 21]];
                    }
                }
            }
            b"SSND" => {
                // offset(4) blockSize(4) 之后才是采样
                if body + 8 <= b.len() {
                    let off = u32_be(&b, body) as usize;
                    ssnd_off = body + 8 + off;
                    ssnd_len = len.saturating_sub(8 + off).min(b.len().saturating_sub(ssnd_off));
                }
            }
            _ => {}
        }
        pos += 8 + len + (len & 1);
    }
    if channels == 0 || rate == 0 || ssnd_off == 0 {
        return Err(anyhow!("AIFF 缺少 COMM/SSND"));
    }
    // AIFC 的压缩类型：NONE = 大端 PCM，sowt = 小端 PCM，fl32/FL32 = float32
    let little = matches!(&compression, b"sowt");
    let float = matches!(&compression, b"fl32" | b"FL32" | b"fl64" | b"FL64");
    let bytes_ps = (bits / 8).max(1) as usize;
    let ch = channels as usize;
    let frame_bytes = bytes_ps * ch;
    let n = ssnd_len / frame_bytes;
    let mut sink = WavSink::create(output, rate, channels)?;
    let mut buf = Vec::with_capacity(n * ch);
    for i in 0..n {
        let base = ssnd_off + i * frame_bytes;
        for c in 0..ch {
            let s = &b[base + c * bytes_ps..base + (c + 1) * bytes_ps];
            buf.push(sample_to_f32(s, bits, float, little));
        }
    }
    // COMM 里的帧数是权威值；SSND 可能被 padding 撑长
    if frames > 0 && (frames as usize) < n {
        buf.truncate(frames as usize * ch);
    }
    sink.write(&buf)?;
    sink.finish()
}

/// 80-bit 扩展浮点（1 符号 + 15 指数 + 64 尾数）→ u32 采样率
fn ext80_to_u32(b: &[u8]) -> u32 {
    let exp = u16::from_be_bytes([b[0], b[1]]) & 0x7FFF;
    let mant = u64::from_be_bytes(b[2..10].try_into().unwrap());
    if exp == 0 || exp == 0x7FFF {
        return 0;
    }
    // value = mant * 2^(exp - 16383 - 63)
    let shift = exp as i32 - 16383 - 63;
    let v = if shift >= 0 {
        (mant as f64) * 2f64.powi(shift)
    } else {
        (mant as f64) / 2f64.powi(-shift)
    };
    v.round().clamp(0.0, u32::MAX as f64) as u32
}

// ── CAF ───────────────────────────────────────────────────
pub fn decode_caf(input: &Path, output: &Path) -> Result<WavInfo> {
    let b = read_all(input).with_context(|| format!("读 CAF 失败：{}", input.display()))?;
    if b.len() < 8 || &b[0..4] != b"caff" {
        return Err(anyhow!("不是 CAF 文件"));
    }
    let mut rate = 0f64;
    let mut channels = 0u32;
    let mut bits = 0u32;
    let mut is_float = false;
    let mut little_endian = false;
    let mut data_off = 0usize;
    let mut data_len = 0usize;

    let mut pos = 8usize;
    while pos + 12 <= b.len() {
        let id = &b[pos..pos + 4];
        let len = i64::from_be_bytes(b[pos + 4..pos + 12].try_into().unwrap());
        let body = pos + 12;
        let len = if len < 0 { b.len() - body } else { len as usize };
        match id {
            b"desc" => {
                if body + 32 <= b.len() {
                    rate = f64::from_be_bytes(b[body..body + 8].try_into().unwrap());
                    let fmt = &b[body + 8..body + 12];
                    let flags = u32_be(&b, body + 12);
                    channels = u32_be(&b, body + 24);
                    bits = u32_be(&b, body + 28);
                    is_float = fmt == b"lpcm" && (flags & 0x1) != 0;
                    // kAudioFormatFlagIsBigEndian = 0x2
                    little_endian = fmt == b"lpcm" && (flags & 0x2) == 0;
                }
            }
            b"data" => {
                // 前 4 字节是 edit count
                data_off = body + 4;
                data_len = len.saturating_sub(4).min(b.len().saturating_sub(data_off));
                break;
            }
            _ => {}
        }
        pos = body + len + (len & 1);
    }
    if rate <= 0.0 || channels == 0 || data_off == 0 {
        return Err(anyhow!("CAF 缺少 desc/data"));
    }
    let bytes_ps = (bits / 8).max(1) as usize;
    let ch = channels as usize;
    let frame_bytes = bytes_ps * ch;
    let n = data_len / frame_bytes;
    let mut sink = WavSink::create(output, rate.round() as u32, channels as u16)?;
    let mut buf = Vec::with_capacity(n * ch);
    for i in 0..n {
        let base = data_off + i * frame_bytes;
        for c in 0..ch {
            let s = &b[base + c * bytes_ps..base + (c + 1) * bytes_ps];
            buf.push(sample_to_f32(s, bits as u16, is_float, little_endian));
        }
    }
    sink.write(&buf)?;
    sink.finish()
}

// ── AU ────────────────────────────────────────────────────
pub fn decode_au(input: &Path, output: &Path) -> Result<WavInfo> {
    let b = read_all(input).with_context(|| format!("读 AU 失败：{}", input.display()))?;
    if b.len() < 24 || &b[0..4] != b".snd" {
        return Err(anyhow!("不是 AU 文件"));
    }
    let data_off = u32_be(&b, 4) as usize;
    let data_len = {
        let v = u32_be(&b, 8) as usize;
        if v == 0 || v == u32::MAX as usize {
            b.len().saturating_sub(data_off)
        } else {
            v.min(b.len().saturating_sub(data_off))
        }
    };
    let encoding = u32_be(&b, 12);
    let rate = u32_be(&b, 16);
    let channels = u32_be(&b, 20).max(1);
    let (bytes_ps, float) = match encoding {
        1 => (1usize, false), // µ-law
        2 => (1, false),      // 8-bit linear
        3 => (2, false),
        4 => (3, false),
        5 => (4, false),
        6 => (4, true),
        7 => (8, true),
        27 => (1, false), // A-law
        _ => return Err(anyhow!("AU 编码 {encoding} 还不支持")),
    };
    let ch = channels as usize;
    let frame_bytes = bytes_ps * ch;
    let n = data_len / frame_bytes;
    let mut sink = WavSink::create(output, rate, channels as u16)?;
    let mut buf = Vec::with_capacity(n * ch);
    for i in 0..n {
        let base = data_off + i * frame_bytes;
        for c in 0..ch {
            let s = &b[base + c * bytes_ps..base + (c + 1) * bytes_ps];
            let v = match encoding {
                1 => ulaw_to_f32(s[0]),
                27 => alaw_to_f32(s[0]),
                2 => (s[0] as f32 - 128.0) / 128.0,
                _ => sample_to_f32(s, (bytes_ps * 8) as u16, float, false),
            };
            buf.push(v);
        }
    }
    sink.write(&buf)?;
    sink.finish()
}

fn ulaw_to_f32(u: u8) -> f32 {
    let u = !u;
    let sign = u & 0x80;
    let exp = (u >> 4) & 0x07;
    let mant = u & 0x0F;
    let mut v = (((mant as i32) << 3) + 0x84) << exp;
    v -= 0x84;
    if sign != 0 {
        v = -v;
    }
    v as f32 / 32768.0
}

fn alaw_to_f32(a: u8) -> f32 {
    let a = a ^ 0x55;
    let sign = a & 0x80;
    let exp = (a >> 4) & 0x07;
    let mant = a & 0x0F;
    let mut v = (mant as i32) << 4;
    v += 8;
    if exp > 0 {
        v = (v + 0x100) << (exp - 1);
    }
    if sign != 0 {
        v = -v;
    }
    v as f32 / 32768.0
}

/// 大/小端整数与浮点 → f32
fn sample_to_f32(s: &[u8], bits: u16, float: bool, little: bool) -> f32 {
    let get = |n: usize| -> Vec<u8> {
        if little {
            let mut v = s[..n].to_vec();
            v.reverse();
            v
        } else {
            s[..n].to_vec()
        }
    };
    if float {
        return match bits {
            32 => {
                let v = get(4);
                f32::from_be_bytes([v[0], v[1], v[2], v[3]])
            }
            64 => {
                let v = get(8);
                f64::from_be_bytes([v[0], v[1], v[2], v[3], v[4], v[5], v[6], v[7]]) as f32
            }
            _ => 0.0,
        };
    }
    match bits {
        8 => (s[0] as f32 - 128.0) / 128.0,
        16 => {
            let v = get(2);
            i16::from_be_bytes([v[0], v[1]]) as f32 / 32768.0
        }
        24 => {
            let v = get(3);
            let x = ((v[0] as i32) << 16) | ((v[1] as i32) << 8) | v[2] as i32;
            let x = (x << 8) >> 8;
            x as f32 / 8_388_608.0
        }
        32 => {
            let v = get(4);
            i32::from_be_bytes([v[0], v[1], v[2], v[3]]) as f32 / 2_147_483_648.0
        }
        _ => 0.0,
    }
}

/// 便捷：从内存里取 PCM（给测试用）
#[allow(dead_code)]
pub(crate) fn _unused(b: &[u8]) -> u64 {
    u64_be(b, 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ext80_rate() {
        // 44100 的 80-bit 扩展浮点表示：0x400E AC44 0000 0000 0000
        let b = [
            0x40, 0x0E, 0xAC, 0x44, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        ];
        assert_eq!(ext80_to_u32(&b), 44100);
    }

    #[test]
    fn ext80_rate_48000() {
        let b = [
            0x40, 0x0E, 0xBB, 0x80, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        ];
        assert_eq!(ext80_to_u32(&b), 48000);
    }
}
