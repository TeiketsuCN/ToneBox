//! 原生解码器：不经过 ffmpeg、自己实现的自由格式解码。
//!
//! ## 范围
//!
//! 只收"规范公开 + 无有效专利"的格式（详见文档里的合规说明）：
//! WAV/RF64/BWF/W64/AU/AIFF/AIFC/CAF 容器、FLAC、ALAC、WavPack、TTA、Musepack、
//! QOA、Shorten、DSF/DFF（DSD）。
//!
//! 有活跃专利的（AAC 系、AC-4、DTS、LC3、aptX、MQA、Atmos）一律不进这里 ——
//! 自己写也躲不开专利，写在自由格式上才有意义。
//!
//! ## 为什么统一落成 WAV
//!
//! 管线后面还要交给 ffmpeg 编码成目标格式。把解码结果写成 32-bit float WAV，
//! 后面就是普通的"wav 进 → 目标出"，不需要给每种源格式单独开一条路径，
//! 也不会在中间多做一次量化。

pub mod aiff; // AIFF / AIFC / CAF（大端家族）
pub mod alac; // MP4 里的 Apple Lossless
pub mod dsd; // DSF / DFF
pub mod flac;
pub mod musepack;
pub mod qoa;
pub mod shorten;
pub mod tta;
pub mod wav; // WAV / W64 / RF64 / BWF
pub mod wavpack;

use std::fs::File;
use std::io::{BufWriter, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

use anyhow::{anyhow, Result};

/// 解码完成后拿到的 PCM 基本参数
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WavInfo {
    pub rate: u32,
    pub channels: u16,
    /// 每声道帧数
    pub frames: u64,
}

impl WavInfo {
    pub fn duration_s(&self) -> f64 {
        if self.rate == 0 {
            0.0
        } else {
            self.frames as f64 / self.rate as f64
        }
    }
}

/// 32-bit float WAV 写入器：所有原生解码器的统一出口。
pub struct WavSink {
    out: BufWriter<File>,
    rate: u32,
    channels: u16,
    frames: u64,
}

impl WavSink {
    pub fn create(path: &Path, rate: u32, channels: u16) -> Result<Self> {
        if rate == 0 || channels == 0 {
            return Err(anyhow!("非法 PCM 参数：{rate} Hz / {channels} 声道"));
        }
        let f = File::create(path)?;
        let mut out = BufWriter::new(f);
        // 头先按 0 写，收尾回填长度
        out.write_all(b"RIFF")?;
        out.write_all(&0u32.to_le_bytes())?;
        out.write_all(b"WAVE")?;
        out.write_all(b"fmt ")?;
        out.write_all(&16u32.to_le_bytes())?;
        out.write_all(&3u16.to_le_bytes())?; // WAVE_FORMAT_IEEE_FLOAT
        out.write_all(&channels.to_le_bytes())?;
        out.write_all(&rate.to_le_bytes())?;
        let block = (channels as u32) * 4;
        out.write_all(&(rate * block).to_le_bytes())?;
        out.write_all(&(block as u16).to_le_bytes())?;
        out.write_all(&32u16.to_le_bytes())?;
        out.write_all(b"data")?;
        out.write_all(&0u32.to_le_bytes())?;
        Ok(WavSink {
            out,
            rate,
            channels,
            frames: 0,
        })
    }

    pub fn rate(&self) -> u32 {
        self.rate
    }

    pub fn channels(&self) -> u16 {
        self.channels
    }

    /// 写交织 f32 采样（长度必须是声道数整数倍）
    pub fn write(&mut self, interleaved: &[f32]) -> Result<()> {
        let ch = self.channels as usize;
        if ch == 0 || interleaved.len() % ch != 0 {
            return Err(anyhow!(
                "PCM 长度 {} 不是声道数 {} 的整数倍",
                interleaved.len(),
                ch
            ));
        }
        self.frames += (interleaved.len() / ch) as u64;
        let mut bytes = Vec::with_capacity(interleaved.len() * 4);
        for s in interleaved {
            bytes.extend_from_slice(&s.to_le_bytes());
        }
        self.out.write_all(&bytes)?;
        Ok(())
    }

    pub fn finish(mut self) -> Result<WavInfo> {
        let info = WavInfo {
            rate: self.rate,
            channels: self.channels,
            frames: self.frames,
        };
        let data_bytes = self.frames * self.channels as u64 * 4;
        self.out.flush()?;
        let mut f = self.out.into_inner().map_err(|e| e.into_error())?;
        f.seek(SeekFrom::Start(4))?;
        f.write_all(&((36 + data_bytes) as u32).to_le_bytes())?;
        f.seek(SeekFrom::Start(40))?;
        f.write_all(&(data_bytes as u32).to_le_bytes())?;
        f.flush()?;
        Ok(info)
    }
}

/// 支持的源格式
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Codec {
    Wav,
    Aiff,
    Caf,
    Au,
    Flac,
    Alac,
    WavPack,
    Tta,
    Musepack,
    Qoa,
    Shorten,
    Dsd,
    Vorbis,
    Opus,
}

impl Codec {
    pub fn name(self) -> &'static str {
        match self {
            Codec::Wav => "WAV",
            Codec::Aiff => "AIFF",
            Codec::Caf => "CAF",
            Codec::Au => "AU",
            Codec::Flac => "FLAC",
            Codec::Alac => "ALAC",
            Codec::WavPack => "WavPack",
            Codec::Tta => "TTA",
            Codec::Musepack => "Musepack",
            Codec::Qoa => "QOA",
            Codec::Shorten => "Shorten",
            Codec::Dsd => "DSD",
            Codec::Vorbis => "Vorbis",
            Codec::Opus => "Opus",
        }
    }
}

/// 按扩展名 + 魔数判断这个文件该交给谁。
///
/// 只认明确无害的格式；认不出来返回 `None`，调用方继续走 ffmpeg。
pub fn sniff(path: &Path) -> Option<Codec> {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    let magic = read_magic(path, 12).unwrap_or_default();
    match ext.as_str() {
        "wav" | "wave" | "w64" | "rf64" | "bwf" => return Some(Codec::Wav),
        "aif" | "aiff" | "aifc" => return Some(Codec::Aiff),
        "caf" => return Some(Codec::Caf),
        "au" | "snd" => return Some(Codec::Au),
        "flac" => return Some(Codec::Flac),
        "wv" => return Some(Codec::WavPack),
        "tta" => return Some(Codec::Tta),
        "mpc" => return Some(Codec::Musepack),
        "qoa" => return Some(Codec::Qoa),
        "shn" => return Some(Codec::Shorten),
        "dsf" | "dff" => return Some(Codec::Dsd),
        "ogg" | "oga" => return Some(Codec::Vorbis),
        "opus" => return Some(Codec::Opus),
        "m4a" | "mp4" => {
            // m4a 里既可能是 AAC 也可能是 ALAC；只有 alac 的 magic cookie 才自己解
            return if magic.starts_with(b"\0\0\0") && magic.len() >= 8 { None } else { None };
        }
        _ => {}
    }
    // 扩展名不认识时按魔数兜底
    if magic.starts_with(b"fLaC") {
        return Some(Codec::Flac);
    }
    if magic.starts_with(b"wvpk") {
        return Some(Codec::WavPack);
    }
    if magic.starts_with(b"TTA1") {
        return Some(Codec::Tta);
    }
    if magic.starts_with(b"qoaf") {
        return Some(Codec::Qoa);
    }
    if magic.starts_with(b"MPCK") || magic.starts_with(b"MP+") {
        return Some(Codec::Musepack);
    }
    if magic.starts_with(b"DSD ") {
        return Some(Codec::Dsd);
    }
    if magic.starts_with(b"FRM8") {
        return Some(Codec::Dsd);
    }
    None
}

fn read_magic(path: &Path, n: usize) -> Option<Vec<u8>> {
    use std::io::Read;
    let mut f = File::open(path).ok()?;
    let mut buf = vec![0u8; n];
    let got = f.read(&mut buf).ok()?;
    buf.truncate(got);
    Some(buf)
}

/// 把任意受支持的源解成 f32 WAV。返回 PCM 参数。
pub fn decode_to_wav(input: &Path, output: &Path) -> Result<(Codec, WavInfo)> {
    let codec = sniff(input)
        .ok_or_else(|| anyhow!("不是原生解码器认识的格式：{}", input.display()))?;
    let info = match codec {
        Codec::Wav => wav::decode(input, output)?,
        Codec::Aiff => aiff::decode_aiff(input, output)?,
        Codec::Caf => aiff::decode_caf(input, output)?,
        Codec::Au => aiff::decode_au(input, output)?,
        Codec::Flac => flac::decode(input, output)?,
        Codec::Alac => alac::decode(input, output)?,
        Codec::WavPack => wavpack::decode(input, output)?,
        Codec::Tta => tta::decode(input, output)?,
        Codec::Musepack => musepack::decode(input, output)?,
        Codec::Qoa => qoa::decode(input, output)?,
        Codec::Shorten => shorten::decode(input, output)?,
        Codec::Dsd => dsd::decode(input, output)?,
        // Vorbis / Opus 还没实现：直接报错，调用方退回 ffmpeg
        Codec::Vorbis | Codec::Opus => {
            return Err(anyhow!("{} 的原生解码器还没实现", codec.name()))
        }
    };
    Ok((codec, info))
}

/// 原生解码优先清单：只有这些格式先走自己的解码器，其余照常交给 ffmpeg。
///
/// 为什么不一上来就全接管：我们的解码器是新的，出 bug 时宁可退回 ffmpeg
/// 也不要把用户本来能转的文件转坏。清单里的格式如果解不出来（返回 Err），
/// 调用方会自动回退。
pub fn prefer_native(codec: Codec) -> bool {
    matches!(
        codec,
        Codec::Dsd
            | Codec::Flac
            | Codec::Alac
            | Codec::WavPack
            | Codec::Tta
            | Codec::Musepack
            | Codec::Qoa
            | Codec::Shorten
    )
}

/// 试着自己解码：成功返回临时 WAV 路径，失败返回 None（调用方回退 ffmpeg）。
pub fn decode_to_temp(input: &Path) -> Option<PathBuf> {
    let codec = sniff(input)?;
    if !prefer_native(codec) {
        return None;
    }
    let mut tmp = std::env::temp_dir();
    tmp.push(format!(
        "tonelore_native_{}_{}.wav",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.subsec_nanos())
            .unwrap_or(0)
    ));
    match decode_to_wav(input, &tmp) {
        Ok((_, info)) if info.frames > 0 => Some(tmp),
        Ok(_) => {
            let _ = std::fs::remove_file(&tmp);
            None
        }
        Err(_) => {
            let _ = std::fs::remove_file(&tmp);
            None
        }
    }
}

/// 临时文件守卫：离开作用域就删，转换失败也不会留在 temp 里
pub struct TempGuard(pub Option<PathBuf>);

impl Drop for TempGuard {
    fn drop(&mut self) {
        if let Some(p) = self.0.take() {
            let _ = std::fs::remove_file(p);
        }
    }
}

/// 给各个解码器用的小工具：读整个文件（中小文件够用）
pub(crate) fn read_all(path: &Path) -> Result<Vec<u8>> {
    use std::io::Read;
    let mut f = File::open(path)?;
    let mut buf = Vec::new();
    f.read_to_end(&mut buf)?;
    Ok(buf)
}

/// 小端/大端读取助手
pub(crate) fn u16_le(b: &[u8], off: usize) -> u16 {
    u16::from_le_bytes([b[off], b[off + 1]])
}
pub(crate) fn u32_le(b: &[u8], off: usize) -> u32 {
    u32::from_le_bytes([b[off], b[off + 1], b[off + 2], b[off + 3]])
}
pub(crate) fn u64_le(b: &[u8], off: usize) -> u64 {
    u64::from_le_bytes([
        b[off],
        b[off + 1],
        b[off + 2],
        b[off + 3],
        b[off + 4],
        b[off + 5],
        b[off + 6],
        b[off + 7],
    ])
}
pub(crate) fn u16_be(b: &[u8], off: usize) -> u16 {
    u16::from_be_bytes([b[off], b[off + 1]])
}
pub(crate) fn u32_be(b: &[u8], off: usize) -> u32 {
    u32::from_be_bytes([b[off], b[off + 1], b[off + 2], b[off + 3]])
}
pub(crate) fn u64_be(b: &[u8], off: usize) -> u64 {
    u64::from_be_bytes([
        b[off],
        b[off + 1],
        b[off + 2],
        b[off + 3],
        b[off + 4],
        b[off + 5],
        b[off + 6],
        b[off + 7],
    ])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wav_sink_roundtrip_header() {
        let mut p = std::env::temp_dir();
        p.push(format!("native_sink_{}.wav", std::process::id()));
        {
            let mut s = WavSink::create(&p, 44100, 2).unwrap();
            s.write(&[0.0, 0.0, 0.5, -0.5, 0.25, 0.25]).unwrap();
            let info = s.finish().unwrap();
            assert_eq!(info.rate, 44100);
            assert_eq!(info.channels, 2);
            assert_eq!(info.frames, 3);
        }
        let b = read_all(&p).unwrap();
        assert_eq!(&b[0..4], b"RIFF");
        assert_eq!(u32_le(&b, 4), (b.len() - 8) as u32);
        assert_eq!(u32_le(&b, 40), 3 * 2 * 4);
        let _ = std::fs::remove_file(&p);
    }

    #[test]
    fn sniff_by_extension_and_magic() {
        let mut p = std::env::temp_dir();
        p.push(format!("sniff_{}.flac", std::process::id()));
        std::fs::write(&p, b"fLaC\0\0\0\0").unwrap();
        assert_eq!(sniff(&p), Some(Codec::Flac));
        let _ = std::fs::remove_file(&p);
    }
}
