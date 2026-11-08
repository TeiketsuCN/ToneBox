//! 视频容器组 —— 既是输入，也能当输出目标。
//!
//! 和音频格式不同，这里存的不是"一种编码"，而是**容器**：同一个 mkv 里
//! 视频流可以是 copy（直接封装）或 h264/hevc/vp9/av1，音频流可以是 copy
//! 或按容器规矩重编。所以能力矩阵（哪个容器收哪种编码）写在这个文件里，
//! GUI 只照着画按钮。
//!
//! 产品边界：
//! - 只处理**第一条**视频流和第一条音轨；多音轨/字幕不做（音乐用户不需要）。
//! - AVI 的文件名后缀保持 `.avi`，但编码组合受老播放器限制，选项只给 copy/h264。
//! - 不做 DRM，也不做"从视频里扒流媒体"。

use super::{FormatSpec, Group, Kind, Patent};

pub static VIDEO: &[FormatSpec] = &[
    FormatSpec {
        id: "mkv",
        name: "MKV",
        ext: "mkv",
        kind: Kind::Container,
        group: Group::Video,
        patent: Patent::Free,
        can_encode: true,
        primary: true,
        blurb: "Matroska：什么都能装，音轨/字幕都留得住。\n默认直接复制，不重编码画面。",
        args: &[],
        size_ratio: 0.0,
    },
    FormatSpec {
        id: "mp4",
        name: "MP4",
        ext: "mp4",
        kind: Kind::Container,
        group: Group::Video,
        patent: Patent::Free,
        can_encode: true,
        primary: true,
        blurb: "手机、苹果设备、剪辑软件最认的容器。\n只收 H.264/H.265/AV1 和 AAC/AC-3/ALAC。",
        args: &[],
        size_ratio: 0.0,
    },
    FormatSpec {
        id: "webm",
        name: "WEBM",
        ext: "webm",
        kind: Kind::Container,
        group: Group::Video,
        patent: Patent::Free,
        can_encode: true,
        primary: true,
        blurb: "网页用的开放容器，只收 VP9/AV1 + Opus/Vorbis。\n浏览器里直接播，剪辑软件支持一般。",
        args: &[],
        size_ratio: 0.0,
    },
    FormatSpec {
        id: "avi",
        name: "AVI",
        ext: "avi",
        kind: Kind::Container,
        group: Group::Video,
        patent: Patent::Free,
        can_encode: true,
        primary: false,
        blurb: "老播放器/老设备用。编码组合很有限，\n画面只能 copy 或 H.264，音频基本只认 MP3/AC-3。",
        args: &[],
        size_ratio: 0.0,
    },
];
