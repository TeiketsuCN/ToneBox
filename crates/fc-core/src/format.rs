//! 格式表 —— 侧栏、能力矩阵、百科文案的唯一数据源。
//!
//! 设计原则（重要，别改）：
//! 侧栏只列**能编码**的格式，也就是 `can_encode == true` 的那几个。
//! 209 种格式里绝大多数是"输入"，输入靠自动识别，不占 UI。
//! 所以侧栏永远只有个位数条目，不会随格式库膨胀而爆炸。

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

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Group {
    Lossless,
    Lossy,
    Device,
    ReadOnly,
}

impl Group {
    pub fn title(self) -> &'static str {
        match self {
            Group::Lossless => "无损",
            Group::Lossy => "有损",
            Group::Device => "设备专用",
            Group::ReadOnly => "只识别",
        }
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
    pub can_encode: bool,
    /// 侧栏展开时显示的小字（3 行以内，别写长）
    pub blurb: &'static str,
    /// ffmpeg 编码参数
    pub args: &'static [&'static str],
    /// 相对 PCM 的体积系数（用于侧栏预估）
    pub size_ratio: f32,
}

pub static FORMATS: &[FormatSpec] = &[
    FormatSpec {
        id: "flac",
        name: "FLAC",
        ext: "flac",
        kind: Kind::Lossless,
        group: Group::Lossless,
        patent: Patent::Free,
        can_encode: true,
        blurb: "无损压缩，体积约为 WAV 的 55~65%。\n解码后与原始 PCM 逐比特一致。\n无专利，任何软件都能放。",
        args: &["-c:a", "flac", "-compression_level", "8"],
        size_ratio: 0.62,
    },
    FormatSpec {
        id: "wav",
        name: "WAV",
        ext: "wav",
        kind: Kind::Uncompressed,
        group: Group::Lossless,
        patent: Patent::Free,
        can_encode: true,
        blurb: "未压缩 PCM，最大的体积，最好的兼容性。\n适合还要再编辑的素材，不适合长期归档。",
        args: &["-c:a", "pcm_s24le"],
        size_ratio: 1.0,
    },
    FormatSpec {
        id: "alac",
        name: "ALAC",
        ext: "m4a",
        kind: Kind::Lossless,
        group: Group::Lossless,
        patent: Patent::Free,
        can_encode: true,
        blurb: "Apple 的无损格式，压缩率和 FLAC 相当。\n只在你需要塞进 Apple 生态时才选它。",
        args: &["-c:a", "alac"],
        size_ratio: 0.66,
    },
    FormatSpec {
        id: "aiff",
        name: "AIFF",
        ext: "aiff",
        kind: Kind::Uncompressed,
        group: Group::Lossless,
        patent: Patent::Free,
        can_encode: true,
        blurb: "Apple 的未压缩格式，和 WAV 等价。\nMac 上剪片、做母带时用。",
        args: &["-c:a", "pcm_s24be"],
        size_ratio: 1.0,
    },
    FormatSpec {
        id: "mp3",
        name: "MP3",
        ext: "mp3",
        kind: Kind::Lossy,
        group: Group::Lossy,
        patent: Patent::Free,
        can_encode: true,
        blurb: "1993 年的有损格式，专利 2017 年全部到期。\n兼容性无敌，音质全看码率。",
        args: &["-c:a", "libmp3lame", "-b:a", "320k"],
        size_ratio: 0.09,
    },
    FormatSpec {
        id: "opus",
        name: "Opus",
        ext: "opus",
        kind: Kind::Lossy,
        group: Group::Lossy,
        patent: Patent::Free,
        can_encode: true,
        blurb: "2012 年的开源有损格式。\n64kbps 就能打平 MP3 128kbps。\n新项目首选，但老播放器可能不认。",
        args: &["-c:a", "libopus", "-b:a", "96k"],
        size_ratio: 0.03,
    },
    FormatSpec {
        id: "vorbis",
        name: "Vorbis",
        ext: "ogg",
        kind: Kind::Lossy,
        group: Group::Lossy,
        patent: Patent::Free,
        can_encode: true,
        blurb: "Ogg 里装的老牌开源有损格式。\n游戏行业用得多，现在基本被 Opus 取代。",
        args: &["-c:a", "libvorbis", "-q:a", "6"],
        size_ratio: 0.06,
    },
    // ── 以下 can_encode = false，不进侧栏，只出现在百科与解释里 ──
    FormatSpec {
        id: "aac",
        name: "AAC",
        ext: "m4a",
        kind: Kind::Lossy,
        group: Group::Lossy,
        patent: Patent::Expiring,
        can_encode: false,
        blurb: "比 MP3 更高效，同码率通常更好听。\n部分专利仍在有效期，本项目不做编码。",
        args: &["-c:a", "aac"],
        size_ratio: 0.1,
    },
    FormatSpec {
        id: "atrac3",
        name: "ATRAC3",
        ext: "oma",
        kind: Kind::Lossy,
        group: Group::Device,
        patent: Patent::Free,
        can_encode: false,
        blurb: "索尼 1999 年的 MDLP 编码。\n只能解码（专利已过期），无法编码。",
        args: &[],
        size_ratio: 0.06,
    },
    FormatSpec {
        id: "atrac3p",
        name: "ATRAC3+",
        ext: "oma",
        kind: Kind::Lossy,
        group: Group::Device,
        patent: Patent::Expiring,
        can_encode: false,
        blurb: "索尼 2003 年的编码，Hi-MD / Walkman 用。\n只能解码，且分加密与非加密两种。",
        args: &[],
        size_ratio: 0.06,
    },
    FormatSpec {
        id: "atrac1",
        name: "ATRAC1",
        ext: "aea",
        kind: Kind::Lossy,
        group: Group::Device,
        patent: Patent::Free,
        can_encode: false,
        blurb: "初代 MD 的编码（1992）。\n码率 292kbps，只能解码。",
        args: &[],
        size_ratio: 0.09,
    },
    FormatSpec {
        id: "atrac9",
        name: "ATRAC9",
        ext: "at9",
        kind: Kind::Lossy,
        group: Group::Device,
        patent: Patent::Active,
        can_encode: false,
        blurb: "索尼 2013 年的编码，PS4 / Vita 用。\n专利还在保护期，只做识别。",
        args: &[],
        size_ratio: 0.06,
    },
];

/// 侧栏里展示的格式：能编码的那些
pub fn browsable() -> impl Iterator<Item = &'static FormatSpec> {
    FORMATS.iter().filter(|f| f.can_encode)
}

pub fn by_id(id: &str) -> Option<&'static FormatSpec> {
    FORMATS.iter().find(|f| f.id == id)
}

pub fn default_target() -> &'static FormatSpec {
    by_id("flac").unwrap()
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
    let pcm = info.duration_s
        * info.sample_rate as f64
        * info.channels.max(1) as f64
        * bits
        / 8.0;
    (pcm * spec.size_ratio as f64) as u64
}
