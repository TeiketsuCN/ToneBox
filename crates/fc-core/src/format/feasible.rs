//! 当前随包 ffmpeg 就能编码、但之前没开放的格式。
//!
//! 这些都在本机用 vendor/ffmpeg 实测过：
//! G.711 A/μ-law、G.722、MS ADPCM、IMA ADPCM、Nellymoser(FLV)、
//! AAC 5.1、AU、RF64、BWF。

use super::{FormatSpec, Group, Kind, Patent};

pub static FEASIBLE: &[FormatSpec] = &[
    FormatSpec {
        id: "g711_alaw",
        name: "G.711 A-law",
        ext: "wav",
        kind: Kind::Lossy,
        group: Group::Speech,
        patent: Patent::Expiring,
        can_encode: true,
        primary: false,
        blurb: "电话/G.711 A-law，8 kHz 单声道。\n随包 ffmpeg 可直接编码成 WAV。",
        args: &["-c:a", "pcm_alaw", "-ar", "8000", "-ac", "1"],
        size_ratio: 0.08,
    },
    FormatSpec {
        id: "g711_ulaw",
        name: "G.711 μ-law",
        ext: "wav",
        kind: Kind::Lossy,
        group: Group::Speech,
        patent: Patent::Expiring,
        can_encode: true,
        primary: false,
        blurb: "北美电话/G.711 μ-law，8 kHz 单声道。\n随包 ffmpeg 可直接编码成 WAV。",
        args: &["-c:a", "pcm_mulaw", "-ar", "8000", "-ac", "1"],
        size_ratio: 0.08,
    },
    FormatSpec {
        id: "g722",
        name: "G.722",
        ext: "g722",
        kind: Kind::Lossy,
        group: Group::Speech,
        patent: Patent::Expiring,
        can_encode: true,
        primary: false,
        blurb: "G.722 宽带语音，16 kHz 单声道。\n随包 ffmpeg 有 g722 编码器。",
        args: &["-c:a", "g722", "-ar", "16000", "-ac", "1", "-f", "g722"],
        size_ratio: 0.1,
    },
    FormatSpec {
        id: "adpcm_ms",
        name: "ADPCM (MS)",
        ext: "wav",
        kind: Kind::Lossy,
        group: Group::Speech,
        patent: Patent::Expiring,
        can_encode: true,
        primary: false,
        blurb: "Microsoft ADPCM，WAV 里常见。\n有损，但比 PCM 小很多。",
        args: &["-c:a", "adpcm_ms"],
        size_ratio: 0.25,
    },
    FormatSpec {
        id: "ima_adpcm",
        name: "IMA ADPCM",
        ext: "wav",
        kind: Kind::Lossy,
        group: Group::Speech,
        patent: Patent::Expiring,
        can_encode: true,
        primary: false,
        blurb: "IMA/DVI ADPCM，游戏和电话常用。\n输出 WAV，4-bit 差分。",
        args: &["-c:a", "adpcm_ima_wav"],
        size_ratio: 0.25,
    },
    FormatSpec {
        id: "nellymoser",
        name: "Nellymoser",
        ext: "flv",
        kind: Kind::Lossy,
        group: Group::Speech,
        patent: Patent::Expiring,
        can_encode: true,
        primary: false,
        blurb: "Flash 的 Nellymoser 语音编码。\n只能封装成 FLV；8 kHz 单声道。",
        args: &["-c:a", "nellymoser", "-ar", "8000", "-ac", "1", "-vn", "-f", "flv"],
        size_ratio: 0.06,
    },
    FormatSpec {
        id: "aac_multichannel",
        name: "AAC Multichannel",
        ext: "m4a",
        kind: Kind::Lossy,
        group: Group::Spatial,
        patent: Patent::Expiring,
        can_encode: true,
        primary: false,
        blurb: "AAC-LC 多声道（这里固定 5.1 / 384k）。\n不是对象音频；只是多声道 AAC。",
        args: &["-c:a", "aac", "-ac", "6", "-b:a", "384k", "-vn"],
        size_ratio: 0.12,
    },
    FormatSpec {
        id: "au",
        name: "AU",
        ext: "au",
        kind: Kind::Uncompressed,
        group: Group::Container,
        patent: Patent::Free,
        can_encode: true,
        primary: false,
        blurb: "Sun/NeXT AU，未压缩 PCM。\n随包 ffmpeg 有 au 封装器。",
        args: &["-c:a", "pcm_s16be", "-f", "au"],
        size_ratio: 1.0,
    },
    FormatSpec {
        id: "rf64",
        name: "RF64",
        ext: "wav",
        kind: Kind::Uncompressed,
        group: Group::Container,
        patent: Patent::Free,
        can_encode: true,
        primary: false,
        blurb: "超过 4 GB 的 WAV 扩展（RF64）。\n用 wav 封装器 + -rf64 always。",
        args: &["-c:a", "pcm_s24le", "-rf64", "always"],
        size_ratio: 1.0,
    },
    FormatSpec {
        id: "bwf",
        name: "BWF",
        ext: "wav",
        kind: Kind::Uncompressed,
        group: Group::Container,
        patent: Patent::Free,
        can_encode: true,
        primary: false,
        blurb: "Broadcast Wave Format，带广播 bext 元数据。\n随包 ffmpeg 用 -write_bext。",
        args: &["-c:a", "pcm_s24le", "-write_bext", "1"],
        size_ratio: 1.0,
    },
    // ── 蓝牙编码：喂耳机/发射器的，不是拿来存档的 ──
    FormatSpec {
        id: "aptx",
        name: "aptX",
        ext: "aptx",
        kind: Kind::Lossy,
        group: Group::Lossy,
        patent: Patent::Expiring,
        can_encode: true,
        primary: false,
        blurb: "蓝牙 aptX 的编码，固定 352kbps、4:1 压缩。\nCSR 的基础专利多已到期；蓝牙里它是实时流，\n**没有标准文件后缀**，.aptx 只是 ffmpeg muxer 的叫法。",
        args: &["-c:a", "aptx", "-f", "aptx"],
        size_ratio: 0.25,
    },
    FormatSpec {
        id: "aptx_hd",
        name: "aptX HD",
        ext: "aptxhd",
        kind: Kind::Lossy,
        group: Group::Lossy,
        patent: Patent::Expiring,
        can_encode: true,
        primary: false,
        blurb: "aptX HD：576kbps / 24bit，主打「高解析蓝牙」。\n带宽够了，但仍然是 4:1 压缩，不等于无损。\n.aptxhd 同样是 ffmpeg 自己定的，不是通用后缀。",
        args: &["-c:a", "aptx_hd", "-f", "aptx_hd"],
        size_ratio: 0.41,
    },
    FormatSpec {
        id: "sbc",
        name: "SBC",
        ext: "sbc",
        kind: Kind::Lossy,
        group: Group::Lossy,
        patent: Patent::Free,
        can_encode: true,
        primary: false,
        blurb: "蓝牙 A2DP 的「最低配」编码，任何蓝牙音频设备都必须支持。\n质量大致相当于 MP3 128kbps，胜在兼容性。\n.sbc 也是 ffmpeg 习惯，蓝牙标准里没有规定文件名。",
        args: &["-c:a", "sbc", "-f", "sbc"],
        size_ratio: 0.23,
    },
    // ── 语音 / 游戏 ADPCM ──
    FormatSpec {
        id: "adpcm_yamaha",
        name: "ADPCM (Yamaha)",
        ext: "wav",
        kind: Kind::Lossy,
        group: Group::Speech,
        patent: Patent::Expiring,
        can_encode: true,
        primary: false,
        blurb: "雅马哈 ADPCM，老声卡和游戏机里常见。\n输出 WAV，4-bit 差分，体积约为 PCM 的四分之一。",
        args: &["-c:a", "adpcm_yamaha"],
        size_ratio: 0.25,
    },
    FormatSpec {
        id: "adpcm_ima_qt",
        name: "IMA ADPCM (QT)",
        ext: "mov",
        kind: Kind::Lossy,
        group: Group::Speech,
        patent: Patent::Expiring,
        can_encode: true,
        primary: false,
        blurb: "QuickTime 里的 IMA ADPCM，老 Mac 视频工程常用。\n输出 .mov，4-bit 差分。",
        args: &["-c:a", "adpcm_ima_qt"],
        size_ratio: 0.25,
    },
    FormatSpec {
        id: "g723_1",
        name: "G.723.1",
        ext: "wav",
        kind: Kind::Lossy,
        group: Group::Speech,
        patent: Patent::Expiring,
        can_encode: true,
        primary: false,
        blurb: "1996 年的 ITU 窄带语音编码，5.3 / 6.3kbps。\n会议系统和语音网关里常见，只适合人声。\n输出装进 WAV 容器，本软件和其它播放器都能认。",
        args: &["-c:a", "g723_1", "-ar", "8000", "-ac", "1"],
        size_ratio: 0.02,
    },
    FormatSpec {
        id: "g726",
        name: "G.726",
        ext: "wav",
        kind: Kind::Lossy,
        group: Group::Speech,
        patent: Patent::Free,
        can_encode: true,
        primary: false,
        blurb: "ITU G.726 电话 ADPCM（16/24/32/40kbps）。\n交换机和语音网关用得多，8kHz 单声道。\n输出装进 WAV 容器，便于试听和再导入。",
        args: &["-c:a", "g726", "-ar", "8000", "-ac", "1"],
        size_ratio: 0.04,
    },
    FormatSpec {
        id: "real_144",
        name: "RealAudio 1.0",
        ext: "rm",
        kind: Kind::Lossy,
        group: Group::Speech,
        patent: Patent::Expiring,
        can_encode: true,
        primary: false,
        blurb: "1995 年 RealAudio 1.0 的语音编码（RA144）。\n当年「网上听广播」就靠它，音质大致是 AM 水平。",
        args: &["-c:a", "real_144", "-ar", "8000", "-ac", "1"],
        size_ratio: 0.02,
    },
    // ── 游戏 / 广播 / 容器里的无损 ──
    FormatSpec {
        id: "adx",
        name: "ADX（Sega/CRI）",
        ext: "adx",
        kind: Kind::Lossy,
        group: Group::Lossy,
        patent: Patent::Expiring,
        can_encode: true,
        primary: false,
        blurb: "世嘉 Dreamcast、PS2 时代大量用过的游戏音频（CRI ADX）。\n4-bit ADPCM，文件头是 `80 00`，很多游戏解包工具认这个。\n想听老游戏原声、或者往老引擎里塞音频时用。",
        args: &["-c:a", "adpcm_adx"],
        size_ratio: 0.25,
    },
    FormatSpec {
        id: "ogg_flac",
        name: "FLAC (Ogg)",
        ext: "oga",
        kind: Kind::Lossless,
        group: Group::Lossless,
        patent: Patent::Free,
        can_encode: true,
        primary: false,
        blurb: "把 FLAC 装进 Ogg 容器里（RFC 5334 里的 `.oga` 涵盖 Ogg FLAC）。\n压缩和原生 FLAC 一样，但认它的播放器少得多。\n除非某个老工具只吃 .oga，否则用原生 FLAC。",
        args: &["-c:a", "flac", "-f", "ogg"],
        size_ratio: 0.62,
    },
    FormatSpec {
        id: "mlp",
        name: "MLP",
        ext: "mlp",
        kind: Kind::Lossless,
        group: Group::Lossless,
        patent: Patent::Expiring,
        can_encode: true,
        primary: false,
        blurb: "Meridian Lossless Packing，Dolby TrueHD 的核心编码。\n无损、支持多声道，DVD-Audio 时代的东西。\nffmpeg 把它标成实验性，所以固定带 `-strict -2`。",
        args: &["-c:a", "mlp", "-strict", "-2"],
        size_ratio: 0.55,
    },
    FormatSpec {
        id: "wma_v1",
        name: "WMA v1",
        ext: "wma",
        kind: Kind::Lossy,
        group: Group::Lossy,
        patent: Patent::Expiring,
        can_encode: true,
        primary: false,
        blurb: "WMA 的第一版（v1），比常见的 v2 更老。\n只有要喂 2003 年前后的老播放器/车机时才需要。",
        args: &["-c:a", "wmav1", "-b:a", "192k"],
        size_ratio: 0.14,
    },
];

#[cfg(test)]
mod tests {
    use super::*;

    /// 这些是实测能用**随包** ffmpeg 编出来的，别在重构时弄丢。
    #[test]
    fn extra_codecs_stay_listed() {
        let ids: Vec<&str> = crate::format::all().map(|f| f.id).collect();
        for id in [
            "aptx",
            "aptx_hd",
            "sbc",
            "adpcm_yamaha",
            "adpcm_ima_qt",
            "g723_1",
            "g726",
            "real_144",
            "adx",
            "ogg_flac",
            "mlp",
            "wma_v1",
        ] {
            let f = crate::format::by_id(id).unwrap_or_else(|| panic!("表里没有 {id}"));
            assert!(f.can_encode, "{id} 应该是可编码的");
            assert!(!f.args.is_empty(), "{id} 没有编码参数");
        }
    }

    /// 多后缀只放 ffmpeg 认得的别名，改错会直接转不出来。
    #[test]
    fn ext_choices_are_the_verified_ones() {
        use crate::format::{by_id, ext_choices};
        assert_eq!(ext_choices(by_id("aac").unwrap()), vec!["m4a", "aac"]);
        assert_eq!(ext_choices(by_id("opus").unwrap()), vec!["opus", "ogg"]);
        assert_eq!(ext_choices(by_id("vorbis").unwrap()), vec!["ogg", "oga"]);
        assert_eq!(ext_choices(by_id("wma").unwrap()), vec!["wma", "asf"]);
        assert_eq!(ext_choices(by_id("aiff").unwrap()), vec!["aiff", "aif"]);
        assert_eq!(ext_choices(by_id("mkv").unwrap()), vec!["mkv", "mka"]);
        assert_eq!(ext_choices(by_id("mp4").unwrap()), vec!["mp4", "m4v"]);
        assert_eq!(ext_choices(by_id("pcm").unwrap()), vec!["pcm", "raw"]);
        // 单后缀的格式不能凭空变多
        assert_eq!(ext_choices(by_id("flac").unwrap()), vec!["flac"]);
        assert_eq!(ext_choices(by_id("mp3").unwrap()), vec!["mp3"]);
    }}