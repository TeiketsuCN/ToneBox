//! 格式百科长文案。
//!
//! 为什么不塞进 `FormatSpec`：那要给 29 个条目全部加字段，一次改动横跨
//! `lossless.rs` / `lossy.rs` / `device.rs` 三个文件、两条人的分工线。
//! 单独一张 `id -> 长文` 的表，谁写内容谁就只改这个文件。
//!
//! 文案要求：150~250 字，写清「是什么 / 什么场合遇到 / 坑在哪 / 和邻居怎么选」。
//! `can_encode` 是产品开关，不是技术能力；文案里可以说明“ffmpeg 能编但本工具不提供”。

/// 查一个格式的长文案。没有就返回 `None`，界面回退到 `FormatSpec.blurb`。
pub fn long_blurb(id: &str) -> Option<&'static str> {
    TABLE.iter().find(|(k, _)| *k == id).map(|(_, v)| *v)
}

/// 已经写了长文案的格式数量（界面用来显示进度）
pub fn covered() -> usize {
    TABLE.len()
}

static TABLE: &[(&str, &str)] = &[
    // ── 无损 ──
    (
        "flac",
        "2001 年由 Josh Cooley 发布的自由无损压缩格式，至今仍是开源无损的事实标准。\
         编码后约为 WAV 的 55~65%，解码还原与原始 PCM 逐比特一致。\
         没有专利、没有授权费，几乎所有软件和硬件都支持。\
         需要无损归档、又不想被 Apple 生态绑住时，FLAC 是首选；\
         要极致兼容老播放器才考虑 WAV，要 Apple 生态就用 ALAC。",
    ),
    (
        "wav",
        "1988/1991 年微软和 IBM 定的 RIFF WAVE，未压缩 PCM 容器，至今仍是音频交换的通用格式。\
         录音、剪辑、游戏音效和 Windows 系统音都常用。\
         坑是体积大，24bit/96kHz 立体声一小时能到几 GB；.wav 里也可能装 ADPCM/MP3/ATRAC3，\
         不是所有 WAV 都无损。要归档选 FLAC，要兼容/再编辑选 WAV，别拿它当无损压缩。",
    ),
    (
        "alac",
        "Apple 2004 年推出、2011 年开源的无损压缩，随 iTunes/QuickTime 普及。\
         解码后与 PCM 逐比特一致，压缩率与 FLAC 接近。\
         坑是它通常装在 .m4a 里，和 AAC 共用扩展名，必须看 codec；\
         非 Apple 设备/老车机的支持不如 FLAC。要进 Apple 生态选 ALAC，跨平台归档选 FLAC。",
    ),
    (
        "aiff",
        "1988 年 Apple 的 AIFF，未压缩 PCM 容器，和 WAV 等价，区别是字节序：AIFF 大端、WAV 小端。\
         Mac 录音和老 Pro Tools 工程常见，.aiff/.aif 都可能。\
         坑是文件大、Windows 支持一般；AIFF-C 还能装压缩编码，不是所有 AIFF 都无损。\
         要 Mac 兼容选 AIFF，要通用选 WAV。",
    ),
    (
        "wavpack",
        "1998 年的开源无损格式，压缩率通常略优于 FLAC，还支持混合模式（有损 + 修正文件）。\
         在发烧友和归档圈有少量用户，但硬件/播放器支持远不如 FLAC。\
         坑是 .wv 生态小，手机/车机经常不认；混合模式不是普通无损。\
         要极致压缩率可以试，要通用归档还是 FLAC。它适合做冷备份，不适合当唯一分发格式。",
    ),
    (
        "tta",
        "2004 年的 True Audio，结构极简的无损格式，编码/解码开销低，压缩率一般。\
         多见于老无损音乐包和少数播放器。\
         坑是生态很小，ffmpeg 能解但硬件支持差；不支持封面/标签的变体不少。\
         和 FLAC 比没有优势，只在遇到 .tta 时用它，不要主动选它做归档。它更像历史格式，遇到就转 FLAC，别长期保留。",
    ),
    (
        "caf",
        "2005 年 Apple 的 Core Audio Format，本质是容器，能装 PCM、AAC、ALAC 等。\
         iOS/macOS 录音和 Logic 工程常用，优势是支持超长文件和更灵活的元数据。\
         坑是它不等于无损：里面可能是 AAC；Windows 播放器支持差。\
         要跨平台选 WAV/FLAC，要 Apple 录音工作流才选 CAF。",
    ),
    (
        "w64",
        "2001 年 Sony 的 Wave64，为了解决 WAV 的 4GB 上限，常见于长时间录音/影视后期。\
         结构和 WAV 类似，但用 64 位长度字段。\
         坑是消费级播放器支持差，很多软件只认 .wav；它通常装 PCM，也可能装其他编码。\
         超过 4GB 的录音选 W64，普通交换还是 WAV。它解决的是工程问题，不是音质问题。",
    ),
    (
        "pcm",
        "没有文件头的裸 PCM，只有采样数据，位深/声道/采样率全靠约定。\
         老游戏、嵌入式、硬件调试和某些电话系统会用。\
         坑是播放器基本认不出，必须手动指定格式；参数错一位就是噪音。\
         要交换/归档选 WAV，只有喂特定硬件或逆向老资源时才用裸 PCM。它适合工程师，普通用户拿到 .pcm 先问参数，别直接双击，别乱试。",
    ),
    // ── 有损：能编码的 ──
    (
        "mp3",
        "1993 年 Fraunhofer 主导的 MPEG-1 Layer III，2017 年专利全部到期，是有损音乐兼容性的天花板。\
         低码率下音质一般，128k 以上开始可用，320k 接近透明。\
         坑是 .mp3 里也可能有 ID3/封面/ReplayGain，老文件还可能是 MP3Pro；转无损不会增加信息。\
         要兼容选 MP3，要效率选 Opus。",
    ),
    (
        "opus",
        "2012 年 IETF 标准，Xiph/Mozilla/Skype 等推动，专利免费。\
         64kbps 就能打平 MP3 128kbps，语音/音乐通吃，延迟低，适合流媒体和游戏。\
         坑是老播放器/车机支持差，.opus 在部分 Windows 软件里要装解码器；\
         它是全带宽编码，频谱截止法对它无效。新项目首选 Opus。",
    ),
    (
        "vorbis",
        "2000 年 Xiph.Org 的开源有损格式，Ogg 容器里常见，游戏行业（尤其独立游戏）用得多。\
         同码率通常优于 MP3，专利免费。\
         坑是硬件/车机支持远不如 MP3，.ogg 也可能装 Opus/FLAC；高频处理不如 Opus。\
         要开源兼容选 Vorbis，要新项目选 Opus。它适合老游戏资源，不适合新归档。",
    ),
    (
        "mp2",
        "1991 年的 MPEG-1 Layer II，MP3 的前身，广播、DVB、早期 DVD 和视频制作里还在用。\
         码率通常 192~384kbps，编码/解码延迟低、实现简单。\
         坑是效率低、体积大，消费级播放器支持一般；.mp2 也可能被误认成 MP3。\
         只有在广播/视频工作流明确要求时选 MP2，否则用 MP3/Opus。",
    ),
    (
        "speex",
        "2002 年 Xiph.Org 的开源语音编码，专利免费。专为 8/16/32 kHz 的人声设计，\
         码率 2~44 kbps。坑在于它根本不是音乐编码器——频率上限只有 8 kHz（16 kHz 采样），\
         编出来会带明显金属感和空洞感。和 AMR 比它开源跨平台，和 Opus 比它已被完全取代。\
         只有在需要极小体积存人声、且目标设备明确支持 Speex 时才用它。",
    ),
    // ── 有损：只读 / 不建议编码 ──
    (
        "aac",
        "1997 年 MPEG-2 提出，后来成为 MPEG-4 的核心音频编码。MDCT + 心理声学，同码率通常比 MP3 干净；\
         AAC-LC 常见，HE-AAC 用 SBR 保高频。手机、流媒体、视频音轨到处都是，.m4a 里可能是 AAC 也可能是 ALAC。\
         坑是 AAC 专利池仍在收费，技术上 ffmpeg 能编，但本工具不提供——想要有损请用 Opus；\
         老设备对 ADTS/MP4 封装支持也不一。",
    ),
    (
        "ac3",
        "1992 年随 Dolby Digital 进影院，后来成为 DVD 的 5.1 标准。固定 48kHz、最多 5.1，\
         码率常见 192~640kbps；dialnorm 会改回放响度。老 DVD、蓝光、电视音轨常见，是家庭影院的老 baseline。\
         坑是多声道码率不能和立体声 MP3 直接比，.ac3 裸流和 MKV 里的 AC-3 是同一编码。\
         技术上 ffmpeg 能编，但 Dolby 授权复杂，本工具只解码；新设备优先 E-AC-3，老设备选 AC-3。",
    ),
    (
        "eac3",
        "2004 年的 Dolby Digital Plus，AC-3 的继任者，为蓝光、流媒体和 ATSC 设计。\
         同码率效率更高、声道更多，常藏在 MP4/MKV 里，不一定有独立扩展名。\
         坑是老设备只认 AC-3，不认 E-AC-3；它和 AC-3 容易混。\
         技术上 ffmpeg 能编，但专利仍在保护期，本工具只解码。新设备优先 E-AC-3，老 DVD/车载选 AC-3。",
    ),
    (
        "wma",
        "1999 年微软随 Windows Media 推出，想在低码率市场对抗 MP3，2000 年代 Windows、便携播放器和网络电台很常见。\
         坑是 .wma 下可能是 v2、Pro、Lossless 甚至 Voice，必须看 codec；部分老文件还有 WMA DRM。\
         技术上 ffmpeg 能编，但专利/生态都过期了，本工具只解码。\
         和 MP3 比它更小但兼容差，和 AAC 比它没赢下移动端。",
    ),
    (
        "wma_pro",
        "2003 年随 Windows Media 9 推出，定位多声道/高码率，支持 5.1 和 24bit。\
         微软想进家庭影院，但硬件和片源没铺开，最后只在小众 Windows 环境存在。\
         坑是扩展名仍叫 .wma，普通播放器不一定能解；ffprobe 里 codec 是 wmapro。\
         技术上 ffmpeg 能编，但没人需要，本工具只解码。多声道 Windows 文件才考虑它，否则选 AC-3 或转 FLAC。",
    ),
    (
        "musepack",
        "1999 年从 MP2 派生的有损格式，SV7/SV8 两代，用较高码率追求“听不出差别”，2000 年代在发烧友圈有点名气。\
         坑是编码器闭源、硬件支持少，SV7 和 SV8 还不兼容；现在基本没有生态。\
         ffmpeg 只有解码器，本工具不提供编码。\
         和 MP3 比它同码率口碑好，和 Opus 比它已是历史格式。遇到只解码，别指望转回去。",
    ),
    (
        "amr_nb",
        "1999 年 3GPP 的窄带语音编码，8kHz、4.75~12.2kbps，是功能机和早期手机录音的主力，诺基亚、S60、3GP 里到处都是。\
         坑是频带只到 4kHz，听感就是电话音，而且多为单声道；.amr 和 .3gp 都可能是它。\
         它是运营商时代的标准，不是音乐格式，技术上 ffmpeg 可编但没必要，本工具只解码。\
         和 AMR-WB 比它更窄但兼容更老。",
    ),
    (
        "amr_wb",
        "2001 年 3GPP 的宽带语音编码（G.722.2），16kHz、6.6~23.85kbps，是 VoLTE 之前的高清通话主力，比 AMR-NB 清楚。\
         坑是它仍是语音，不是音乐；.awb/.3gp 都可能，老设备可能只认 NB。\
         技术上 ffmpeg 可编，但运营商/专利生态复杂，本工具只解码。\
         和 AMR-NB 比它频带翻倍，和 Opus 比它更老。只用于语音素材。",
    ),
    (
        "cook",
        "1997 年 RealNetworks 的 RealAudio 编码，靠 RealPlayer 在窄带互联网做流媒体，28.8k 猫时代能出声靠的就是它。\
         坑是 .ra/.rm 只是容器，里面可能是 Cook、ATRAC3、MP3，必须看 codec；音质和现代格式没法比。\
         ffmpeg 能解但不会编，本工具只解码。\
         和 MP3 比它是流式优先，和 AAC 比它属于 RealPlayer 时代，遇到只当历史文件处理。",
    ),
    (
        "he_aac",
        "2003 年出现的 AAC 扩展，用 SBR 在低码率下重建高频，HE-AACv2 再加参数立体声，数字广播、流媒体和老手机音乐服务常用。\
         坑是 SBR 造出来的高频会让频谱截止法误判，所以本项目对它输出“不可判定”；\
         profile 有时只写 AAC，必须看 ffprobe 的 profile。\
         技术上 ffmpeg 能编，但 AAC 专利池收费，本工具只解码；低码率选 HE，高码率选 AAC-LC。",
    ),
    (
        "xhe_aac",
        "2019 年定稿的 MPEG-D USAC，AAC 系最新一代，目标是把低码率流媒体/广播做得更高效。\
         坑是普通 AAC 解码器不一定支持，频谱截止法同样失效，本项目输出“不可判定”；生态还新，老设备基本不认。\
         技术上 ffmpeg 能编，但专利在保护期，本工具只解码。\
         和 HE-AAC 比它更低码率，和 Opus 比它更偏广播标准。",
    ),
    // ── 设备专用 ──
    (
        "atrac1",
        "1992 年索尼 MiniDisc 的初代编码，MD SP 固定 292kbps、44.1kHz，把 CD 压到约五分之一。\
         MD 在 1990 年代到 2000 年代初是便携录音/随身听的主流。\
         坑是 .aea 是 MD 导出容器，.oma 可能是加密 OpenMG；它只有 292k 一档。\
         ffmpeg 只解不编；装了外部的 atracdenc（LGPL，自己装）才能编出 .aea，没装就只解码。\
         和 ATRAC3 比它码率高、容量小；和 MP3 比它是索尼封闭生态。",
    ),
    (
        "atrac3",
        "1999 年索尼为 MDLP 推出的编码，66/105/132kbps 三档，让一张 MD 装更多歌，是 MD 后期的主力。\
         坑是 .oma 里可能是 ATRAC3 或 ATRAC3+，还可能是加密 OpenMG；同族不同码率截止频率差异很大。\
         ffmpeg 只解不编；装了 atracdenc 才能编 LP2 132k / LP4 66k 两档（105k 只有 RealMedia 容器支持）。\
         和 ATRAC1 比它容量大但音质妥协，和 ATRAC3+ 比它效率低。",
    ),
    (
        "atrac3p",
        "2003 年索尼的 ATRAC3+，Hi-MD、Walkman、PSP 都用过，码率 48~352kbps，同码率比 ATRAC3 明显好，\
         是 MD 末期和闪存 Walkman 的主力。坑是扩展名仍是 .oma/.at3/.aa3，和 ATRAC3 混在一起；\
         .oma 有加密和不加密两种。ffmpeg 只解不编；atracdenc 目前只能编 352k 一档，装了才可选。\
         和 ATRAC3 比它低码率更强，和 ATRAC9 比它更老。",
    ),
    (
        "atrac_al",
        "2006 年索尼给 SonicStage 推出的无损格式（ATRAC Advanced Lossless）：本质是 ATRAC3/3+ 的核加一层无损残差，\
         解码后能逐比特还原原文件。坑是它几乎只存在于 SonicStage 导出的 .oma/.aa3 里，而且常常套着 OpenMG；\
         ffmpeg 只认 atrac3al / atrac3plusal 两个 codec 名、只解不编，本项目也没有真实样本做端到端验证。\
         和 FLAC 比它没有跨平台生态，和 ATRAC3+ 比它无损但体积大。",
    ),
    (
        "atrac9",
        "2013 年索尼为 PS4/PS Vita 推出的编码，.at9，定位游戏和便携设备的高效率有损。\
         它比 ATRAC3+ 新，但生态只限索尼平台，公开资料和工具都少。\
         坑是专利仍在保护期，本项目只做识别，不承诺解码；普通播放器基本不认。\
         和 ATRAC3+ 比它效率更高但更封闭，和 Opus/AAC 比它没有跨平台生态。",
    ),
    // ── 视频容器 ──
    (
        "mkv",
        "Matroska（.mkv）是开放容器规范，2002 年起由社区维护。它本身不定义编码，\
         而是把视频、多条音轨、字幕、章节装进一个可扩展的 EBML 结构里，几乎什么编码都收。\
         坑是兼容性看播放器：浏览器和 iPhone 原生不认 mkv，需要转封装或重编码。\
         做「只换容器」时它是最安全的目标：画面和声音都能直接复制，不掉质量。",
    ),
    (
        "mp4",
        "MP4（ISO/IEC 14496-14）是苹果生态和移动设备的事实标准容器，也是网络视频的主流交付格式。\
         它能装 H.264/H.265/AV1 画面，音频收 AAC/AC-3/ALAC/MP3。\
         坑是容器收的编码组合有限：FLAC、Vorbis、Opus 装进去多数设备放不出来，所以转 mp4 时音轨常要重编码。\
         要兼容一切手机和剪辑软件选它；要无损换容器选 mkv。",
    ),
    (
        "webm",
        "WebM 是 Google 主导的开放容器（2010），本质是 Matroska 的子集，专为网页播放设计。\
         只收 VP8/VP9/AV1 画面和 Opus/Vorbis 声音，浏览器原生支持不需要插件。\
         坑是编码慢、剪辑软件支持一般，而且装不了 H.264/AAC——源是 mp4 时画面和声音都要重编码。\
         做网页背景视频、开源项目演示选它；要兼容老设备别选。",
    ),
    (
        "avi",
        "AVI（1992）是 Windows 3.1 时代的老容器，结构简单、容错好，很多车载和 DVD 播放机只认它。\
         它能装的编码组合非常有限：画面基本是 MPEG-4 Part 2 / H.264 / MJPEG，声音 MP3/AC-3。\
         坑是它不支持现代特性（可变帧率、多音轨管理、H.265 基本没人认），文件也容易超过 2GB 限制。\
         给老设备交付时选它；其余场景用 mp4/mkv。",
    ),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lookup_hits_and_misses() {
        assert!(long_blurb("flac").is_some());
        assert!(long_blurb("不存在的格式").is_none());
    }

    #[test]
    fn no_duplicate_ids() {
        let mut seen = std::collections::HashSet::new();
        for (id, _) in TABLE {
            assert!(seen.insert(*id), "wiki 表里 id 重复: {id}");
        }
    }

    #[test]
    fn every_entry_points_to_real_format() {
        for (id, _) in TABLE {
            assert!(
                crate::format::by_id(id).is_some(),
                "wiki 里写了不存在的格式: {id}"
            );
        }
    }
}
