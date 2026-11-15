//! 只读/参考格式：能识别、能解释，但当前构建不提供编码器。
//!
//! 这些不是“假装能转”的目标：侧栏会显示但不可选，点开只展开说明。
//! 需要真正输出时得换 ffmpeg 构建或外部编码器。

use super::{FormatSpec, Group, Kind, Patent};

macro_rules! ro {
    ($id:literal, $name:literal, $ext:literal, $group:expr, $kind:expr, $patent:expr, $blurb:literal) => {
        FormatSpec {
            id: $id,
            name: $name,
            ext: $ext,
            kind: $kind,
            group: $group,
            patent: $patent,
            can_encode: false,
            primary: false,
            blurb: $blurb,
            args: &[],
            size_ratio: 0.05,
        }
    };
}

pub static REFERENCE: &[FormatSpec] = &[
    // ── 无损/无损封装（只读）──
    ro!("wma_lossless", "WMA Lossless", "wma", Group::Lossless, Kind::Lossless, Patent::Active,
        "微软的无损 WMA。\n当前 ffmpeg 构建没有编码器，只读。"),
    ro!("ape", "APE", "ape", Group::Lossless, Kind::Lossless, Patent::Free,
        "Monkey's Audio，老无损格式。\n只读；没有可用编码器。"),
    ro!("tak", "TAK", "tak", Group::Lossless, Kind::Lossless, Patent::Free,
        "Tom's lossless Audio Kompressor。\n只读；没有可用编码器。"),
    ro!("optimfrog", "OptimFROG", "ofr", Group::Lossless, Kind::Lossless, Patent::Free,
        "以高压缩率著称的无损格式。\n只读；没有可用编码器。"),
    ro!("shorten", "Shorten", "shn", Group::Lossless, Kind::Lossless, Patent::Free,
        "早期无损压缩，现场录音存档常见。\n只读；没有可用编码器。"),
    ro!("mpeg4_als", "MPEG-4 ALS", "mp4", Group::Lossless, Kind::Lossless, Patent::Expiring,
        "MPEG-4 Audio Lossless Coding。\n只读；没有可用编码器。"),
    ro!("mpeg4_sls", "MPEG-4 SLS", "mp4", Group::Lossless, Kind::Lossless, Patent::Expiring,
        "Scalable Lossless Coding，可分层无损。\n只读；没有可用编码器。"),
    ro!("la", "LA", "la", Group::Lossless, Kind::Lossless, Patent::Free,
        "Lossless Audio，老式无损。\n只读；没有可用编码器。"),
    ro!("lpac", "LPAC", "lpac", Group::Lossless, Kind::Lossless, Patent::Free,
        "Lossless Predictive Audio Compression。\n只读；没有可用编码器。"),
    ro!("rkau", "RKAU", "rkau", Group::Lossless, Kind::Lossless, Patent::Free,
        "RK Audio 无损压缩。\n只读；没有可用编码器。"),
    ro!("bonk", "Bonk", "bonk", Group::Lossless, Kind::Lossless, Patent::Free,
        "Bonk 有损/无损混合编码。\n只读；没有可用编码器。"),
    // ── DSD 容器 ──
    ro!("dsdiff", "DSDIFF", "dff", Group::Container, Kind::Lossless, Patent::Free,
        "DSD 的 DFF 容器（DSDIFF）。\n当前只能读/分析，不写 DSD。"),
    ro!("dsf", "DSF", "dsf", Group::Container, Kind::Lossless, Patent::Free,
        "DSD 的 DSF 容器（Sony）。\n当前只能读/分析，不写 DSD。"),
    ro!("dst", "DST", "dst", Group::Container, Kind::Lossless, Patent::Free,
        "DSD 的无损压缩（Direct Stream Transfer）。\n只读；没有编码器。"),
    // ── 语音/电话 ──
    ro!("qcp", "QCP", "qcp", Group::Speech, Kind::Lossy, Patent::Expiring,
        "Qualcomm PureVoice 手机录音容器。\n只读；没有编码器。"),
    ro!("wma_voice", "WMA Voice", "wma", Group::Speech, Kind::Lossy, Patent::Active,
        "WMA 的低码率语音档。\n只读；没有编码器。"),
    ro!("g729", "G.729", "g729", Group::Speech, Kind::Lossy, Patent::Active,
        "G.729 低码率电话语音。\n只读；没有编码器。"),
    // ── DTS / 空间音频 ──
    ro!("dts_hd", "DTS-HD", "dts", Group::Spatial, Kind::Lossy, Patent::Active,
        "DTS-HD 家族（含 MA/HRA）。\n只读；没有开源编码器。"),
    ro!("dts_hd_hr", "DTS-HD High Resolution", "dts", Group::Spatial, Kind::Lossy, Patent::Active,
        "DTS-HD 的高码率有损档。\n只读；没有开源编码器。"),
    ro!("dts_x", "DTS:X", "dts", Group::Spatial, Kind::Lossy, Patent::Active,
        "DTS 的对象音频格式。\n只读；没有开源编码器。"),
    ro!("mpeg_surround", "MPEG Surround", "m4a", Group::Spatial, Kind::Lossy, Patent::Expiring,
        "MPEG Surround 空间音频。\n只读；没有编码器。"),
    ro!("mpeg_h_3d", "MPEG-H 3D Audio", "m4a", Group::Spatial, Kind::Lossy, Patent::Active,
        "MPEG-H 3D Audio 对象音频。\n只读；没有编码器。"),
    ro!("mpeg_h_3d_lc3", "MPEG-H 3D Audio LC3", "m4a", Group::Spatial, Kind::Lossy, Patent::Active,
        "MPEG-H 3D Audio 的 LC3 档。\n只读；没有编码器。"),
    // ── AAC/MPEG 语音与低码率 ──
    ro!("mp1", "MPEG-1 Layer I", "mp1", Group::Lossy, Kind::Lossy, Patent::Expiring,
        "MPEG-1 Layer I，早期 MPEG 音频。\n只读；没有编码器。"),
    ro!("mpeg1_layer2", "MPEG-1 Layer II", "mp2", Group::Lossy, Kind::Lossy, Patent::Expiring,
        "MPEG-1 Layer II，就是常说的 MP2。\n现有 MP2 输出可编码；这个是参考别名。"),
    ro!("he_aac_v2", "HE-AAC v2", "m4a", Group::Lossy, Kind::Lossy, Patent::Expiring,
        "HE-AAC + PS 立体声。\n只读；没有编码器。"),
    ro!("aac_lc", "AAC-LC", "m4a", Group::Lossy, Kind::Lossy, Patent::Expiring,
        "AAC Low Complexity。\n现有 AAC 输出就是 LC；这个是参考条目。"),
    ro!("aac_eld", "AAC-ELD", "m4a", Group::Lossy, Kind::Lossy, Patent::Expiring,
        "AAC Enhanced Low Delay，通话/低延迟。\n只读；没有编码器。"),
    ro!("aac_ld", "AAC-LD", "m4a", Group::Lossy, Kind::Lossy, Patent::Expiring,
        "AAC Low Delay，低延迟通信。\n只读；没有编码器。"),
    ro!("usac", "USAC", "m4a", Group::Lossy, Kind::Lossy, Patent::Active,
        "MPEG-D USAC / xHE-AAC 的核心。\n只读；没有编码器。"),
    ro!("lc3", "LC3", "lc3", Group::Lossy, Kind::Lossy, Patent::Active,
        "蓝牙 LE Audio 的 LC3。\n只读；没有编码器。"),
    ro!("lc3plus", "LC3plus", "lc3", Group::Lossy, Kind::Lossy, Patent::Active,
        "LC3plus，低延迟/高音质。\n只读；没有编码器。"),
    ro!("evs", "EVS", "evs", Group::Lossy, Kind::Lossy, Patent::Active,
        "3GPP EVS 增强语音。\n只读；没有编码器。"),
    ro!("twinvq", "TwinVQ", "vqf", Group::Lossy, Kind::Lossy, Patent::Expiring,
        "TwinVQ（VQF）老式音频编码。\n只读；没有编码器。"),
    ro!("mpeg4_hiln", "MPEG-4 HILN", "mp4", Group::Lossy, Kind::Lossy, Patent::Expiring,
        "MPEG-4 Harmonic and Individual Lines/Noise。\n只读；没有编码器。"),
    ro!("mpeg4_ssc", "MPEG-4 SSC", "mp4", Group::Lossy, Kind::Lossy, Patent::Expiring,
        "MPEG-4 Scalable to Lossless 的编码核心之一。\n只读；没有编码器。"),
    // ── 音频封装 ──
    ro!("bw64", "BW64", "bw64", Group::Container, Kind::Container, Patent::Free,
        "广播 WAV 64-bit 容器。\n只读参考；当前不作为输出。"),
    ro!("aifc", "AIFC", "aifc", Group::Container, Kind::Container, Patent::Free,
        "AIFF-C 压缩/多字节序变体。\n只读参考；当前不作为输出。"),
    // ── 语音：后缀按维基 / Asterisk 的格式定义 ──
    ro!("ilbc", "iLBC", "lbc", Group::Speech, Kind::Lossy, Patent::Free,
        "RFC 3951 的低码率语音，专为丢包场景设计（WebRTC 也在用）。\n文件头是 `#!iLBC20` / `#!iLBC30`，后缀 `.lbc`。\n只读；没有编码器。"),
    ro!("siren7", "Siren 7 (G.722.1)", "siren7", Group::Speech, Kind::Lossy, Patent::Expiring,
        "ITU G.722.1，Polycom 的 32kbps 宽带语音。\nAsterisk 用的后缀就是 `.siren7`。只读；没有编码器。"),
    ro!("siren14", "Siren 14 (G.722.1C)", "siren14", Group::Speech, Kind::Lossy, Patent::Expiring,
        "G.722.1 的 14kHz 扩展版，48kbps。\n只读；没有编码器。"),
    ro!("g719", "G.719", "g719", Group::Speech, Kind::Lossy, Patent::Expiring,
        "2008 年的 ITU 全带宽语音编码（Polycom + 爱立信）。\n只读；没有编码器。"),
    ro!("real_288", "RealAudio 2.0", "ra", Group::Speech, Kind::Lossy, Patent::Expiring,
        "RealAudio 2.0（RA28.8），1996 年前后的网络广播用它。\n只读；没有编码器。"),
    // ── 视频 / 广播容器（只读）──
    ro!("mxf", "MXF", "mxf", Group::Video, Kind::Container, Patent::Free,
        "广播行业的标准封装（Material Exchange Format）。\n只读参考；当前不作为输出。"),
];
