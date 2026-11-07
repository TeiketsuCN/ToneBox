//! 格式表 —— 侧栏、能力矩阵、百科文案的唯一数据源。
//!
//! ## 文件归属（并行开发防冲突）
//!
//! | 文件 | 归属 |
//! |---|---|
//! | `format/mod.rs` | 公共 API，改之前先留言 |
//! | `format/lossless.rs` | 机器 A |
//! | `format/lossy.rs` | 机器 B |
//! | `format/device.rs` | 双方都不动（只读格式） |
//!
//! ## 设计原则（重要，别改）
//!
//! 侧栏只列**能编码**的格式，也就是 `can_encode == true` 的那几个。
//! 209 种格式里绝大多数是"输入"，输入靠自动识别，不占 UI。
//! 再加一层 `primary`：分组内默认只显示常用的几个，其余收进"更多"。

pub mod device;
pub mod lossless;
pub mod lossy;

pub use device::DEVICE;
pub use lossless::LOSSLESS;
pub use lossy::LOSSY;

use crate::ffmpeg::MediaInfo;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    Lossless,
    Lossy,
    Uncompressed,
    Container,
    Encrypted,
    Unknown,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Patent {
    /// 已过期或无专利，随便用
    Free,
    /// 部分专利仍在有效期，法务上有争议
    Expiring,
    /// 明确在保护期内
    Active,
}

impl Patent {
    pub fn label(self) -> &'static str {
        match self {
            Patent::Free => "无专利",
            Patent::Expiring => "专利部分过期",
            Patent::Active => "专利在保护期",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum Group {
    Lossless,
    Lossy,
    Device,
}

impl Group {
    pub fn title(self) -> &'static str {
        match self {
            Group::Lossless => "无损",
            Group::Lossy => "有损",
            Group::Device => "设备专用",
        }
    }
    pub fn all() -> [Group; 3] {
        [Group::Lossless, Group::Lossy, Group::Device]
    }
}

pub struct FormatSpec {
    pub id: &'static str,
    pub name: &'static str,
    pub ext: &'static str,
    pub kind: Kind,
    pub group: Group,
    pub patent: Patent,
    /// 能不能作为输出目标。false 的不会出现在侧栏。
    /// 是否**暴露为输出目标**（产品开关）。
    ///
    /// ⚠️ 这不是"技术上能不能编"。比如 AAC / AC-3 / WMA 用 ffmpeg 都能编，
    /// 但有专利或没人需要，所以这里写 `false`；反过来 Speex 无专利、能编，就写 `true`。
    /// 想表达技术能力请写进 `wiki.rs` 的长文案，不要改这个字段。
    pub can_encode: bool,
    /// 分组内是否默认显示。false 的收进「更多 ▾」。
    pub primary: bool,
    /// 侧栏展开时显示的小字（3 行以内，别写长）
    pub blurb: &'static str,
    /// ffmpeg 编码参数
    pub args: &'static [&'static str],
    /// 相对 PCM 的体积系数（用于侧栏预估）
    pub size_ratio: f32,
}

/// 全部格式（无损 → 有损 → 设备专用）
pub fn all() -> impl Iterator<Item = &'static FormatSpec> {
    LOSSLESS.iter().chain(LOSSY).chain(DEVICE)
}

/// 侧栏里展示的格式：能编码的那些
pub fn browsable() -> impl Iterator<Item = &'static FormatSpec> {
    all().filter(|f| f.can_encode)
}

/// 某个分组里能编码的格式，分成「默认显示」和「更多」
pub fn split_group(group: Group) -> (Vec<&'static FormatSpec>, Vec<&'static FormatSpec>) {
    let mut main = Vec::new();
    let mut more = Vec::new();
    for f in browsable().filter(|f| f.group == group) {
        if f.primary {
            main.push(f);
        } else {
            more.push(f);
        }
    }
    (main, more)
}

pub fn by_id(id: &str) -> Option<&'static FormatSpec> {
    all().find(|f| f.id == id)
}

pub fn default_target() -> &'static FormatSpec {
    by_id("flac").expect("FLAC 必须在表里")
}

/// 源文件 → 可选目标 + 每个目标要提示什么
pub struct Choice {
    pub spec: &'static FormatSpec,
    pub warning: Option<&'static str>,
}

pub fn targets_for(src: Option<&MediaInfo>) -> Vec<Choice> {
    browsable()
        .map(|spec| {
            let warning = src.and_then(|i| match (i.is_lossy(), spec.kind) {
                (true, Kind::Lossless) | (true, Kind::Uncompressed) => {
                    Some("源已经是压缩过的，转成无损不会增加信息量")
                }
                (false, Kind::Lossy) => Some("目标是有损格式，转换会丢弃信息"),
                _ => None,
            });
            Choice { spec, warning }
        })
        .collect()
}

/// 粗估输出体积（字节）。只用于侧栏给个量级，不追求精确。
pub fn estimate_size(info: &MediaInfo, spec: &FormatSpec) -> u64 {
    if info.duration_s <= 0.0 || info.sample_rate == 0 {
        return 0;
    }
    let bits = if spec.kind == Kind::Uncompressed { 24.0 } else { 16.0 };
    let pcm = info.duration_s * info.sample_rate as f64 * info.channels.max(1) as f64 * bits / 8.0;
    (pcm * spec.size_ratio as f64) as u64
}

// ── 输出参数 ────────────────────────────────────────────

/// 位深。0 位深这种概念在音频里没有，所以用枚举而不是整数。
#[derive(Clone, Copy, PartialEq, Eq, Debug, serde::Serialize, serde::Deserialize)]
pub enum BitDepth {
    Int16,
    Int24,
    Int32f,
}

impl BitDepth {
    pub fn label(self) -> &'static str {
        match self {
            BitDepth::Int16 => "16-bit",
            BitDepth::Int24 => "24-bit",
            BitDepth::Int32f => "32-bit 浮点",
        }
    }
    /// 小端（WAV / CAF / W64 / 裸 PCM 用）
    pub fn codec_le(self) -> &'static str {
        match self {
            BitDepth::Int16 => "pcm_s16le",
            BitDepth::Int24 => "pcm_s24le",
            BitDepth::Int32f => "pcm_f32le",
        }
    }
    /// 大端（AIFF 用）
    pub fn codec_be(self) -> &'static str {
        match self {
            BitDepth::Int16 => "pcm_s16be",
            BitDepth::Int24 => "pcm_s24be",
            BitDepth::Int32f => "pcm_f32be",
        }
    }
    /// 裸 PCM 的 muxer 名
    pub fn raw_fmt(self) -> &'static str {
        match self {
            BitDepth::Int16 => "s16le",
            BitDepth::Int24 => "s24le",
            BitDepth::Int32f => "f32le",
        }
    }
}

/// 用户可调的输出参数。默认值就是各格式的推荐值。
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ConvertOptions {
    /// FLAC 压缩等级 0~12（不影响音质，只影响体积和速度）
    pub flac_level: u8,
    /// PCM 位深
    pub bit_depth: BitDepth,
    /// MP3 码率 kbps
    pub mp3_bitrate: u32,
    /// Opus 码率 kbps
    pub opus_bitrate: u32,
    /// Vorbis 质量 0~10
    pub vorbis_q: u8,
    /// AAC 码率 kbps
    #[serde(default = "default_aac_bitrate")]
    pub aac_bitrate: u32,
}

fn default_aac_bitrate() -> u32 {
    256
}

impl Default for ConvertOptions {
    fn default() -> Self {
        Self {
            flac_level: 8,
            bit_depth: BitDepth::Int24,
            mp3_bitrate: 320,
            opus_bitrate: 96,
            vorbis_q: 6,
            aac_bitrate: 256,
        }
    }
}

/// 某个格式在当前参数下有哪些可调的旋钮——UI 用它决定显示什么。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Knob {
    FlacLevel,
    BitDepth,
    Mp3Bitrate,
    OpusBitrate,
    VorbisQuality,
    AacBitrate,
}

/// 这个格式能调什么。空数组表示"没得调，用默认值"。
pub fn knobs_for(spec: &FormatSpec) -> &'static [Knob] {
    match spec.id {
        "flac" => &[Knob::FlacLevel],
        "wav" | "caf" | "w64" => &[Knob::BitDepth],
        "aiff" => &[Knob::BitDepth],
        "pcm" => &[Knob::BitDepth],
        "mp3" => &[Knob::Mp3Bitrate],
        "opus" => &[Knob::OpusBitrate],
        "vorbis" => &[Knob::VorbisQuality],
        "aac" => &[Knob::AacBitrate],
        _ => &[],
    }
}

/// 按参数生成 ffmpeg 编码参数。
///
/// 这是把「静态 args」换成「可调参数」的唯一入口，GUI 不要自己拼参数字符串。
pub fn args_for(spec: &FormatSpec, o: &ConvertOptions) -> Vec<String> {
    let s = |v: &str| v.to_string();
    match spec.id {
        "flac" => vec![
            s("-c:a"),
            s("flac"),
            s("-compression_level"),
            o.flac_level.min(12).to_string(),
        ],
        "wav" | "caf" | "w64" => vec![s("-c:a"), s(o.bit_depth.codec_le())],
        "aiff" => vec![s("-c:a"), s(o.bit_depth.codec_be())],
        "pcm" => vec![
            s("-c:a"),
            s(o.bit_depth.codec_le()),
            s("-f"),
            s(o.bit_depth.raw_fmt()),
        ],
        "mp3" => vec![
            s("-c:a"),
            s("libmp3lame"),
            s("-b:a"),
            format!("{}k", o.mp3_bitrate),
        ],
        "opus" => vec![
            s("-c:a"),
            s("libopus"),
            s("-b:a"),
            format!("{}k", o.opus_bitrate),
        ],
        "vorbis" => vec![
            s("-c:a"),
            s("libvorbis"),
            s("-q:a"),
            o.vorbis_q.min(10).to_string(),
        ],
        "aac" => vec![
            s("-c:a"),
            s("aac"),
            s("-b:a"),
            format!("{}k", o.aac_bitrate),
        ],
        // 没列的格式用表里的默认参数
        _ => spec.args.iter().map(|a| a.to_string()).collect(),
    }
}

/// 一把预设——比一屏滑杆有用得多
pub struct Preset {
    pub id: &'static str,
    pub name: &'static str,
    pub note: &'static str,
    pub format: &'static str,
    pub apply: fn(&mut ConvertOptions),
}

pub static PRESETS: &[Preset] = &[
    Preset {
        id: "archive",
        name: "MD 归档",
        note: "无损，体积约 WAV 的六成",
        format: "flac",
        apply: |o| {
            o.flac_level = 8;
        },
    },
    Preset {
        id: "car",
        name: "车载",
        note: "MP3 320，什么都能放",
        format: "mp3",
        apply: |o| {
            o.mp3_bitrate = 320;
        },
    },
    Preset {
        id: "phone",
        name: "手机",
        note: "Opus 96，体积小一半有多",
        format: "opus",
        apply: |o| {
            o.opus_bitrate = 96;
        },
    },
    Preset {
        id: "edit",
        name: "再编辑",
        note: "WAV 24-bit，剪片混音用",
        format: "wav",
        apply: |o| {
            o.bit_depth = BitDepth::Int24;
        },
    },
];

#[cfg(test)]
mod tests {
    use super::*;

    fn spec(id: &str) -> &'static FormatSpec {
        by_id(id).unwrap()
    }

    #[test]
    fn flac_level_reaches_args() {
        let mut o = ConvertOptions::default();
        o.flac_level = 12;
        let a = args_for(spec("flac"), &o);
        assert!(a.contains(&"12".to_string()), "压缩等级没进参数: {a:?}");
        o.flac_level = 0;
        let a = args_for(spec("flac"), &o);
        assert!(a.contains(&"0".to_string()));
    }

    #[test]
    fn bit_depth_changes_pcm_codec() {
        let mut o = ConvertOptions::default();

        o.bit_depth = BitDepth::Int16;
        assert!(args_for(spec("wav"), &o).contains(&"pcm_s16le".to_string()));

        o.bit_depth = BitDepth::Int32f;
        assert!(args_for(spec("wav"), &o).contains(&"pcm_f32le".to_string()));

        // AIFF 必须是大端
        assert!(args_for(spec("aiff"), &o).contains(&"pcm_f32be".to_string()));

        // 裸 PCM 还要带 muxer
        let a = args_for(spec("pcm"), &o);
        assert!(a.contains(&"f32le".to_string()), "裸 PCM 缺 muxer: {a:?}");
    }

    #[test]
    fn bitrate_reaches_args() {
        let mut o = ConvertOptions::default();
        o.mp3_bitrate = 128;
        assert!(args_for(spec("mp3"), &o).contains(&"128k".to_string()));
        o.opus_bitrate = 192;
        assert!(args_for(spec("opus"), &o).contains(&"192k".to_string()));
        o.vorbis_q = 9;
        assert!(args_for(spec("vorbis"), &o).contains(&"9".to_string()));
        o.aac_bitrate = 192;
        assert!(args_for(spec("aac"), &o).contains(&"192k".to_string()));
    }

    #[test]
    fn knobs_match_formats() {
        assert_eq!(knobs_for(spec("flac")), &[Knob::FlacLevel]);
        assert_eq!(knobs_for(spec("wav")), &[Knob::BitDepth]);
        assert_eq!(knobs_for(spec("mp3")), &[Knob::Mp3Bitrate]);
        // 无损自适应格式不该有旋钮
        assert!(knobs_for(spec("alac")).is_empty());
        assert!(knobs_for(spec("wavpack")).is_empty());
    }

    #[test]
    fn presets_point_to_real_formats() {
        for p in PRESETS {
            assert!(by_id(p.format).is_some(), "预设指向了不存在的格式: {}", p.format);
        }
    }
}
