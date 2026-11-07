//! fc-core —— 转换引擎与格式表。
//!
//! 这个 crate 是两台机器并行开发的**契约面**：
//! 公开 API 冻结后，GUI 侧不再改这里，引擎侧不再碰 UI。

pub mod ffmpeg;
pub mod format;
pub mod encoders;
pub mod identify;
pub mod tags;
pub mod metadata;
pub mod wiki;

pub use ffmpeg::{convert, locate, probe, MediaInfo, ProgressFn, Tools};
pub use encoders::list_encoders;
pub use identify::{identify, Encrypted};
pub use metadata::{metadata_args, MetaWrite};
pub use wiki::long_blurb;
pub use tags::{extract_cover, has_cover, read_tags, Tags};
pub use format::{
    all, args_for, bitrate_choices, bitrate_of, browsable, by_id, default_target, estimate_size,
    knob_note, knobs_for, split_group, targets_for, BitDepth, Choice, ConvertOptions, FormatSpec,
    Group, Kind, Knob, Patent, Preset, PRESETS, encoder_for,
};
