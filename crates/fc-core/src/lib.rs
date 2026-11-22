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
pub mod loudness;
pub mod ncm;
pub mod qmc;

pub use ffmpeg::{convert, locate, probe, verify_output, AudioTrackInfo, MediaInfo, ProgressFn, Tools};
pub use dsd::{bit_rate_hz as dsd_bit_rate_hz, tier as dsd_tier};
pub use loudness::Loudness as LoudnessReport;
pub use encoders::{list_decoders, list_encoders, version as ffmpeg_version};
/// 外部 ATRAC 编码器（atracdenc）：检测 + 调用。我们不随包分发它。
pub use external::find as find_atracdenc;
pub use external::find_speexenc;
pub use external::find_wavpack;
pub use external::find_ofr;
pub use external::find_mpcenc;
pub use external::find_psxavenc;
pub use external::find_qoaconv;
pub use external::find_vgaudio;
pub use external::find_vadpcm;
pub use external::{find_vgmstream, is_vgmstream_ext, vgmstream_decode};
pub use external::{find_openmpt123, is_module_ext, openmpt_decode};
pub use identify::{identify, Encrypted};
pub use metadata::{metadata_args, MetaWrite};
pub use wiki::long_blurb;

/// 主机芯片音乐扩展名（ffmpeg 的 libgme 解复用）。
///
/// 这些文件没有固定时长（libgme 会一直渲染），转换时统一只取前 3 分钟，
/// 免得一个 .nsf 出来几小时的 FLAC。播放/进度也按这个兜底。
pub fn is_chiptune(path: &std::path::Path) -> bool {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    matches!(
        ext.as_str(),
        "nsf" | "nsfe" | "spc" | "gbs" | "vgm" | "vgz" | "gym" | "psf" | "psf2" | "usf"
            | "2sf" | "gsf" | "qsf" | "ssf" | "sid" | "hes" | "kss" | "ay" | "sap"
    )
}

/// 芯片音乐默认渲染时长（秒）。NSF/SPC 这类没有"曲目长度"字段。
pub const CHIPTUNE_SECONDS: u32 = 180;
pub use tags::{extract_cover, has_cover, read_tags, Tags};
pub use format::{
    all, args_for, args_for_with, audio_plan, bit_depths_for, bitrate_choices, bitrate_of, browsable,
    by_id, channel_note, channels_for, container_note, copy_video_ok, default_target, ext_choices,
    effective_video_codec, estimate_size, output_channels, ChannelMode,
    estimate_video_size, external_encoder, is_video, knob_note, knobs_for, output_dims, output_rate,
    quality_warnings, rate_choices, rate_note, resolve_video_encoder, split_group, targets_for,
    video_codecs_for, video_containers, video_encoder_name, video_knob_note, video_only, AudioPlan, BitDepth,
    Choice, ConvertOptions, FormatSpec, Group, Kind, Knob, Patent, Preset, QualityWarning, VideoCodec,
    WarningKind, PRESETS, encoder_for,
};
