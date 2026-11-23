//! WAV / RF64 / BWF / W64 读取。
//!
//! 这几个都是一棵 chunk 树，差别只在「块头怎么表示长度」和「W64 用 16 字节 GUID
//! 当块 id」。解码成 f32 之后，位深差异就没了。

use std::path::Path;

use anyhow::{anyhow, Context, Result};

use super::{read_all, u16_le, u32_le, u64_le, WavInfo, WavSink};

/// W64 的 riff 魔数：`riff` + 固定 GUID 尾巴
const W64_TAIL: [u8; 12] = [
    0x2E, 0x91, 0xCF, 0x11, 0xA5, 0xD6, 0x28, 0xDB, 0x04, 0xC1, 0x00, 0x00,
];

pub fn decode(input: &Path, output: &Path) -> Result<WavInfo> {
    let b = read_all(input).with_context(|| format!("读 WAV 失败：{}", input.display()))?;
    if b.len() < 12 {
        return Err(anyhow!("文件太短，不是 WAV"));
    }
    if &b[0..4] == b"riff" && b.len() >= 16 && b[4..16] == W64_TAIL {
        return decode_w64(&b, output);
    }
    if &b[0..4] != b"RIFF" && &b[0..4] != b"RF64" {
        return Err(anyhow!("不是 RIFF/RF64 文件"));
    }
    if &b[8..12] != b"WAVE" {
        return Err(anyhow!("RIFF 里不是 WAVE"));
    }

    let mut fmt: Option<Fmt> = None;
    let mut data_off = 0usize;
    let mut data_len = 0usize;
    // RF64 的 ds64 里才有真实的 data 长度（>4 GB 时 RIFF 头写 0xFFFFFFFF）
    let mut ds64_data_len: Option<u64> = None;

    let mut pos = 12usize;
    while pos + 8 <= b.len() {
        let id = &b[pos..pos + 4];
        let len = u32_le(&b, pos + 4) as usize;
        let body = pos + 8;
        match id {
            b"fmt " => {
                if body + 16 <= b.len() {
                    fmt = Some(parse_fmt(&b[body..], len));
                }
            }
            b"ds64" => {
                // riff size(8) data size(8) sample count(8) table len(4)
                if body + 16 <= b.len() {
                    ds64_data_len = Some(u64::from_le_bytes(b[body + 8..body + 16].try_into().unwrap()));
                }
            }
            b"data" => {
                data_off = body;
                data_len = ds64_data_len
                    .map(|v| v as usize)
                    .unwrap_or(len)
                    .min(b.len().saturating_sub(body));
                break;
            }
            _ => {}
        }
        // 块按偶数对齐，跳过 padding
        let advance = 8 + len + (len & 1);
        if advance == 0 {
            break;
        }
        pos += advance;
    }

    let fmt = fmt.ok_or_else(|| anyhow!("WAV 里没有 fmt 块"))?;
    if data_off == 0 || data_len == 0 {
        return Err(anyhow!("WAV 里没有 data 块"));
    }
    let mut sink = WavSink::create(output, fmt.rate, fmt.channels)?;
    let bytes_per_sample = (fmt.bits / 8) as usize;
    if bytes_per_sample == 0 {
        return Err(anyhow!("位深为 0"));
    }
    let frame_bytes = bytes_per_sample * fmt.channels as usize;
    let frames = data_len / frame_bytes;
    let ch = fmt.channels as usize;
    let mut buf = Vec::with_capacity(frames * ch);
    for i in 0..frames {
        let off = data_off + i * frame_bytes;
        for c in 0..ch {
            let s = &b[off + c * bytes_per_sample..off + (c + 1) * bytes_per_sample];
            buf.push(sample_to_f32(s, fmt.bits, fmt.float));
        }
    }
    sink.write(&buf)?;
    sink.finish()
}

/// W64：块 id 是 16 字节 GUID（前 4 字节还是 ascii），长度是 u64
fn decode_w64(b: &[u8], output: &Path) -> Result<WavInfo> {
    let mut fmt: Option<Fmt> = None;
    let mut data_off = 0usize;
    let mut data_len = 0usize;
    let mut pos = 40usize; // GUID(16) + size(8) + riff GUID(16)
    while pos + 24 <= b.len() {
        let id = &b[pos..pos + 4];
        let len = u64::from_le_bytes(b[pos + 16..pos + 24].try_into().unwrap()) as usize;
        let body = pos + 24;
        match id {
            b"fmt " => {
                if body + 16 <= b.len() {
                    fmt = Some(parse_fmt(&b[body..], len));
                }
            }
            b"data" => {
                data_off = body;
                data_len = len.min(b.len().saturating_sub(body));
                break;
            }
            _ => {}
        }
        let advance = 24 + len + (len & 7);
        if advance == 0 {
            break;
        }
        pos += advance;
    }
    let fmt = fmt.ok_or_else(|| anyhow!("W64 里没有 fmt 块"))?;
    if data_off == 0 {
        return Err(anyhow!("W64 里没有 data 块"));
    }
    let bytes_per_sample = (fmt.bits / 8) as usize;
    let frame_bytes = bytes_per_sample * fmt.channels as usize;
    let frames = data_len / frame_bytes;
    let ch = fmt.channels as usize;
    let mut sink = WavSink::create(output, fmt.rate, fmt.channels)?;
    let mut buf = Vec::with_capacity(frames * ch);
    for i in 0..frames {
        let off = data_off + i * frame_bytes;
        for c in 0..ch {
            let s = &b[off + c * bytes_per_sample..off + (c + 1) * bytes_per_sample];
            buf.push(sample_to_f32(s, fmt.bits, fmt.float));
        }
    }
    sink.write(&buf)?;
    sink.finish()
}

struct Fmt {
    channels: u16,
    rate: u32,
    bits: u32,
    float: bool,
}

/// 解析 fmt 块。支持 PCM(1) / IEEE float(3) / EXTENSIBLE(0xFFFE)。
fn parse_fmt(b: &[u8], len: usize) -> Fmt {
    let tag = u16_le(b, 0);
    let channels = u16_le(b, 2);
    let rate = u32_le(b, 4);
    let bits = u16_le(b, 14) as u32;
    let mut float = tag == 3;
    let mut real_tag = tag;
    if tag == 0xFFFE && b.len() >= 40 {
        // EXTENSIBLE：subformat GUID 的前两个字节才是真正的格式号
        real_tag = u16_le(b, 24);
        float = real_tag == 3;
    }
    let _ = (len, real_tag);
    Fmt {
        channels,
        rate,
        bits,
        float,
    }
}

/// 8/16/24/32-bit 整数或 32/64-bit 浮点 → f32
fn sample_to_f32(s: &[u8], bits: u32, float: bool) -> f32 {
    if float {
        return match bits {
            32 => f32::from_le_bytes([s[0], s[1], s[2], s[3]]),
            64 => {
                let v = f64::from_le_bytes([s[0], s[1], s[2], s[3], s[4], s[5], s[6], s[7]]);
                v as f32
            }
            _ => 0.0,
        };
    }
    match bits {
        8 => (s[0] as f32 - 128.0) / 128.0,
        16 => i16::from_le_bytes([s[0], s[1]]) as f32 / 32768.0,
        24 => {
            let v = ((s[0] as i32) | ((s[1] as i32) << 8) | ((s[2] as i32) << 16)) << 8 >> 8;
            v as f32 / 8_388_608.0
        }
        32 => i32::from_le_bytes([s[0], s[1], s[2], s[3]]) as f32 / 2_147_483_648.0,
        _ => 0.0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(name: &str) -> std::path::PathBuf {
        let mut p = std::env::temp_dir();
        p.push(format!("native_wav_{}_{}.wav", name, std::process::id()));
        p
    }

    #[test]
    fn reads_pcm16_and_float32() {
        // 用 WavSink 造一个 float32 文件，再读回来
        let p = tmp("a");
        {
            let mut s = WavSink::create(&p, 8000, 1).unwrap();
            s.write(&[0.0, 0.5, -0.5]).unwrap();
            s.finish().unwrap();
        }
        let out = tmp("a_out");
        let info = decode(&p, &out).unwrap();
        assert_eq!(info.rate, 8000);
        assert_eq!(info.channels, 1);
        assert_eq!(info.frames, 3);
        let _ = std::fs::remove_file(&p);
        let _ = std::fs::remove_file(&out);
    }
}
