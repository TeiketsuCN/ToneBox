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
