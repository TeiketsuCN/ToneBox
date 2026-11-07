//! 有损组 —— **机器 B 负责**
//!
//! 加格式只在这个文件里加一条。
//! `primary: false` 的会收进侧栏的「更多 ▾」。
//!
//! 注意：`can_encode: false` 的格式**不占侧栏**，只出现在百科和解释里。
//! AAC / AC-3 这些建议保持 false（专利在保护期，见 docs/法律边界-重新评估.md）。

use super::{FormatSpec, Group, Kind, Patent};

pub static LOSSY: &[FormatSpec] = &[
    FormatSpec {
        id: "mp3",
        name: "MP3",
        ext: "mp3",
        kind: Kind::Lossy,
        group: Group::Lossy,
        patent: Patent::Free,
        can_encode: true,
        primary: true,
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
        primary: true,
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
        primary: true,
        blurb: "Ogg 里装的老牌开源有损格式。\n游戏行业用得多，现在基本被 Opus 取代。",
        args: &["-c:a", "libvorbis", "-q:a", "6"],
        size_ratio: 0.06,
    },
    // ── 以下收在「更多」里 ──
    FormatSpec {
        id: "mp2",
        name: "MP2",
        ext: "mp2",
        kind: Kind::Lossy,
        group: Group::Lossy,
        patent: Patent::Free,
        can_encode: true,
        primary: false,
        blurb: "MP3 的前身，MPEG-1 Layer II。\n广播和早期 DVD 用，现在几乎没人用。",
        args: &["-c:a", "mp2", "-b:a", "256k"],
        size_ratio: 0.08,
    },
    // ── 只能解码，不进侧栏 ──
    FormatSpec {
        id: "aac",
        name: "AAC",
        ext: "m4a",
        kind: Kind::Lossy,
        group: Group::Lossy,
        patent: Patent::Expiring,
        can_encode: false,
        primary: false,
        blurb: "比 MP3 更高效，同码率通常更好听。\n部分专利仍在有效期，本项目不做编码。",
        args: &["-c:a", "aac"],
        size_ratio: 0.1,
    },
];
