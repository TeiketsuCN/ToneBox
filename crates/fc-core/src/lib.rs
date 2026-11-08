//! fc-core —— 转换引擎与格式表。
//!
//! 这个 crate 是两台机器并行开发的**契约面**：
//! 公开 API 冻结后，GUI 侧不再改这里，引擎侧不再碰 UI。

pub mod ffmpeg;
pub mod format;
pub mod encoders;
pub mod external;
pub mod identify;
pub mod tags;
pub mod metadata;
pub mod wiki;
pub mod dsd;

pub use ffmpeg::{convert, locate, probe, MediaInfo, ProgressFn, Tools};
pub use dsd::{bit_rate_hz as dsd_bit_rate_hz, tier as dsd_tier};
pub use encoders::list_encoders;
/// 外部 ATRAC 编码器（atracdenc）：检测 + 调用。我们不随包分发它。
pub use external::find as find_atracdenc;
pub use identify::{identify, Encrypted};
pub use metadata::{metadata_args, MetaWrite};
pub use wiki::long_blurb;
pub use tags::{extract_cover, has_cover, read_tags, Tags};
pub use format::{
    all, args_for, bit_depths_for, bitrate_choices, bitrate_of, browsable, by_id, default_target, estimate_size,
    external_encoder, knob_note, knobs_for, output_rate, quality_warnings, rate_choices, rate_note,
    split_group, targets_for, BitDepth, Choice, ConvertOptions, FormatSpec, Group, Kind, Knob,
    Patent, Preset, QualityWarning, WarningKind, PRESETS, encoder_for,
};
