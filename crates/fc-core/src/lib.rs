//! fc-core —— 转换引擎与格式表。
//!
//! 这个 crate 是两台机器并行开发的**契约面**：
//! 公开 API 冻结后，GUI 侧不再改这里，引擎侧不再碰 UI。

pub mod ffmpeg;
pub mod format;
pub mod identify;
pub mod tags;

pub use ffmpeg::{convert, locate, probe, MediaInfo, ProgressFn, Tools};
pub use identify::{identify, Encrypted};
pub use tags::{extract_cover, has_cover, read_tags, Tags};
pub use format::{
    all, args_for, browsable, by_id, default_target, estimate_size, knobs_for, split_group,
    targets_for, BitDepth, Choice, ConvertOptions, FormatSpec, Group, Kind, Knob, Patent, Preset,
    PRESETS,
};
