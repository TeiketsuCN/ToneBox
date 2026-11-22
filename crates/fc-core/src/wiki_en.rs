//! 格式百科长文的英文版（按 id 查；缺失回退中文 TABLE）。
//!
//! 不放进 i18n 词典：长文是内容，不是界面文案；按 id 存更稳，也不会被
//! 「原文即 key」的空格/续行搞乱。

pub static TABLE: &[(&str, &str)] = &[
    (
        "flac",
        "Free lossless compression published by Josh Cooley in 2001, still the de facto standard for \
         open-source lossless audio. Encoded files are about 55–65% of WAV size and decode \
         bit-for-bit identical to the original PCM. No patents, no license fees, supported almost \
         everywhere. FLAC is the default for lossless archiving unless you need the Apple ecosystem \
         (ALAC) or maximum compatibility with ancient players (WAV).",
    ),
    (
        "wav",
        "RIFF WAVE, defined by Microsoft and IBM in 1988/1991, an uncompressed PCM container and \
         still the universal audio exchange format. Recording, editing, game SFX and Windows system \
         sounds all use it. The catch is size: 24-bit/96 kHz stereo can reach several GB per hour. \
         A .wav may also contain ADPCM, MP3 or ATRAC3, so not every WAV is lossless. Archive in \
         FLAC, edit in WAV.",
    ),
    (
        "alac",
        "Apple's lossless codec, introduced in 2004 and open-sourced in 2011, popularised by \
         iTunes/QuickTime. It decodes bit-for-bit and compresses about as well as FLAC. The catch: \
         it usually lives in .m4a and shares the extension with AAC, so you must check the codec; \
         non-Apple devices and old car stereos support it less than FLAC. Use ALAC inside Apple's \
         ecosystem, FLAC for cross-platform archives.",
    ),
    (
        "aiff",
        "Apple's AIFF from 1988, an uncompressed PCM container equivalent to WAV; only the byte \
         order differs (AIFF big-endian, WAV little-endian). Common in Mac recording and old Pro \
         Tools projects, as .aiff or .aif. The catches are large files and mediocre Windows support; \
         AIFF-C can also hold compressed codecs, so not every AIFF is lossless. Use AIFF for Mac \
         compatibility, WAV for general exchange.",
    ),
    (
        "wavpack",
        "Open-source lossless codec from 1998, usually compressing slightly better than FLAC and \
         supporting a hybrid mode (lossy plus a correction file). It has a small following among \
         audiophiles and archivists, but far less hardware support than FLAC. The .wv ecosystem is \
         small and phones/car stereos often ignore it. Try it for maximum compression; use FLAC for \
         general archiving. Good for cold backups, not as a sole distribution format.",
    ),
    (
        "tta",
        "True Audio from 2004: a minimal lossless format with low encode/decode cost and average \
         compression. Found in older lossless music packs and a few players. The ecosystem is tiny: \
         ffmpeg decodes it but hardware support is poor, and many variants lack cover art or tags. \
         It offers no advantage over FLAC. Convert .tta files to FLAC when you meet them instead of \
         choosing it for archives.",
    ),
    (
        "caf",
        "Apple's Core Audio Format from 2005 is really a container that can hold PCM, AAC, ALAC and \
         more. Common in iOS/macOS recording and Logic projects, with support for very long files \
         and flexible metadata. The catch: it is not necessarily lossless (it may contain AAC), and \
         Windows player support is poor. Use WAV/FLAC for cross-platform work, CAF for Apple \
         recording workflows.",
    ),
    (
        "w64",
        "Sony's Wave64 from 2001, created to overcome WAV's 4 GB limit and common in long recordings \
         and post-production. The structure resembles WAV but uses 64-bit length fields. Consumer \
         players support it poorly and many tools only accept .wav. It usually holds PCM but can \
         hold other codecs. Use W64 for recordings over 4 GB, WAV for ordinary exchange: it solves \
         an engineering limit, not a quality problem.",
    ),
    (
        "pcm",
        "Headerless raw PCM: just samples, with bit depth, channels and sample rate left to \
         convention. Used by old games, embedded devices, hardware debugging and some telephony \
         systems. Players cannot guess the format, so you must specify it by hand; one wrong \
         parameter and it is noise. Use WAV for exchange or archiving. Raw PCM is for engineers \
         feeding specific hardware or reverse-engineering old assets.",
    ),
    (
        "mp3",
        "MPEG-1 Layer III, led by Fraunhofer in 1993; all patents expired by 2017, making it the \
         ceiling for lossy compatibility. Quality is mediocre at low bitrates, usable from 128k and \
         close to transparent at 320k. A .mp3 may also carry ID3/cover/ReplayGain, and old files may \
         be MP3Pro; converting to lossless adds no information. Choose MP3 for compatibility, Opus \
         for efficiency.",
    ),
    (
        "opus",
        "IETF standard from 2012, pushed by Xiph/Mozilla/Skype with no patent fees. 64 kbps matches \
         MP3 at 128 kbps, handles speech and music, and has low latency—good for streaming and games. \
         The catch is weak support in old players and car stereos; some Windows software needs a \
         decoder for .opus. It is full-band, so the cutoff heuristic does not work on it. Opus is the \
         first choice for new projects.",
    ),
    (
        "vorbis",
        "Xiph.Org's open-source lossy format from 2000, usually found in Ogg containers and common in \
         games, especially indie titles. It usually beats MP3 at the same bitrate and is patent-free. \
         The catch is much weaker hardware/car support than MP3; .ogg may also contain Opus or FLAC, \
         and its high-frequency handling is below Opus. Choose Vorbis for open-source compatibility, \
         Opus for new work; it suits old game assets, not new archives.",
    ),
    (
        "mp2",
        "MPEG-1 Layer II from 1991, MP3's predecessor, still used in broadcast, DVB, early DVD and \
         video production. Typical bitrates are 192–384 kbps; encode/decode latency is low and the \
         implementation is simple. The catch is poor efficiency and large files, limited consumer \
         player support, and .mp2 may be mistaken for MP3. Choose MP2 only when a broadcast/video \
         workflow demands it; otherwise use MP3 or Opus.",
    ),
    (
        "speex",
        "Xiph.Org's open-source speech codec from 2002, patent-free and designed for 8/16/32 kHz \
         voice at 2–44 kbps. The catch: it is not a music codec—its bandwidth tops out around 8 kHz \
         (at 16 kHz sampling), so music sounds metallic and hollow. Compared with AMR it is open and \
         cross-platform; compared with Opus it has been fully superseded. Use it only for tiny voice \
         files on devices that explicitly support Speex.",
    ),
    (
        "aac",
        "Introduced with MPEG-2 in 1997 and later the core audio codec of MPEG-4. MDCT plus \
         psychoacoustics usually sounds cleaner than MP3 at the same bitrate; AAC-LC is common and \
         HE-AAC uses SBR for highs. Phones, streaming and video tracks use it everywhere, and an .m4a \
         may be AAC or ALAC. The catch: AAC patent pools still charge, so this app does not encode it \
         (use Opus for lossy); old devices also differ in ADTS/MP4 support.",
    ),
    (
        "ac3",
        "Dolby Digital, introduced in cinemas in 1992 and later the DVD 5.1 standard. Fixed 48 kHz, \
         up to 5.1 channels, typically 192–640 kbps; dialnorm adjusts playback loudness. Common in old \
         DVDs, Blu-rays and TV tracks, and the baseline of home cinema. The catch: multichannel \
         bitrates are not comparable to stereo MP3, and .ac3 streams and AC-3 inside MKV are the same \
         codec. Dolby licensing is complex, so this app only decodes it; prefer E-AC-3 on new devices \
         and AC-3 on old ones.",
    ),
    (
        "eac3",
        "Dolby Digital Plus from 2004, AC-3's successor, designed for Blu-ray, streaming and ATSC. It \
         is more efficient at the same bitrate and supports more channels, often hidden inside \
         MP4/MKV without its own extension. The catch: old devices only accept AC-3 and confuse the \
         two. Patents are still active, so this app only decodes it. Prefer E-AC-3 on new devices, \
         AC-3 for old DVDs and car audio.",
    ),
    (
        "wma",
        "Microsoft's Windows Media Audio, launched in 1999 to fight MP3 at low bitrates, common on \
         2000s Windows, portable players and internet radio. The catch: a .wma may be v2, Pro, \
         Lossless or even Voice, so check the codec; some old files carry WMA DRM. Patents and the \
         ecosystem have expired, so this app only decodes it. Smaller than MP3 but less compatible; \
         it lost the mobile market to AAC.",
    ),
    (
        "wma_pro",
        "Windows Media Audio Professional, shipped with Windows Media 9 in 2003 for multichannel and \
         high bitrates, supporting 5.1 and 24-bit. Microsoft aimed at home cinema, but hardware and \
         content never arrived, leaving it a niche Windows format. The extension is still .wma, \
         players may not decode it, and ffprobe reports wmapro. This app only decodes it; consider \
         AC-3 or FLAC instead.",
    ),
    (
        "musepack",
        "A lossy format derived from MP2 in 1999, with SV7/SV8 generations, using higher bitrates to \
         aim for transparency; it had a following among audiophiles in the 2000s. The catch: the \
         encoder is closed, hardware support is scarce, and SV7/SV8 are incompatible. ffmpeg only \
         decodes it and this app does not encode it. It beat MP3 in reputation at equal bitrate but \
         is now historical—decode it, do not plan to encode back.",
    ),
    (
        "amr_nb",
        "3GPP narrowband speech from 1999: 8 kHz, 4.75–12.2 kbps, the workhorse of feature phones \
         and early phone recordings, found in Nokia devices, S60 and 3GP. The catch: bandwidth stops \
         at 4 kHz, so it sounds like a phone call and is usually mono; both .amr and .3gp may hold \
         it. It is a carrier-era standard, not music; this app only decodes it.",
    ),
    (
        "amr_wb",
        "3GPP wideband speech from 2001 (G.722.2): 16 kHz, 6.6–23.85 kbps, the main HD voice codec \
         before VoLTE and clearer than AMR-NB. The catch: it is still speech, not music; .awb/.3gp \
         may hold it and old devices may only know NB. Carrier and patent ecosystems are complex, so \
         this app only decodes it. Use it for voice material only.",
    ),
    (
        "cook",
        "RealNetworks' RealAudio codec from 1997, which powered streaming over narrowband internet \
         in the RealPlayer era—28.8k modems could finally make sound. The catch: .ra/.rm are just \
         containers that may hold Cook, ATRAC3 or MP3, so check the codec; quality cannot compare \
         with modern formats. ffmpeg decodes but does not encode it, and this app only decodes. Treat \
         it as a historical file.",
    ),
    (
        "he_aac",
        "An AAC extension from 2003 that uses SBR to rebuild high frequencies at low bitrates, with \
         HE-AACv2 adding parametric stereo; common in digital radio, streaming and old phone music \
         services. The catch: SBR-generated highs fool the cutoff heuristic, so this app reports \
         \"undetermined\"; profiles may say only AAC, so check ffprobe. AAC patent pools charge, so \
         this app only decodes it. Low bitrate: HE; high bitrate: AAC-LC.",
    ),
    (
        "xhe_aac",
        "MPEG-D USAC, finalised in 2019 and the newest AAC generation, aimed at more efficient \
         low-bitrate streaming and broadcast. The catch: ordinary AAC decoders may not support it, \
         the cutoff heuristic fails again and this app reports \"undetermined\"; the ecosystem is new \
         and old devices ignore it. Patents are active, so this app only decodes it. HE-AAC goes \
         lower, Opus is more of an internet standard.",
    ),
    (
        "atrac1",
        "Sony's first MiniDisc codec from 1992: MD SP at a fixed 292 kbps/44.1 kHz, compressing CD to \
         about one fifth. MD was the portable recording/player mainstream from the 1990s into the \
         early 2000s. The catch: .aea is MD's export container and .oma may be encrypted OpenMG; only \
         one 292k profile exists. ffmpeg decodes but does not encode; external atracdenc (LGPL, \
         install it yourself) is needed to write .aea.",
    ),
    (
        "atrac3",
        "Sony's codec for MDLP in 1999, with 66/105/132 kbps profiles that fit more songs on an MD; \
         the mainstay of later MiniDisc. The catch: .oma may hold ATRAC3 or ATRAC3+ and may be \
         encrypted OpenMG; cutoff varies a lot within the family. ffmpeg only decodes; atracdenc \
         enables LP2 132k and LP4 66k (105k is RealMedia-only). Bigger capacity than ATRAC1 with \
         quality compromises.",
    ),
    (
        "atrac3p",
        "Sony's ATRAC3+ from 2003, used by Hi-MD, Walkman and PSP at 48–352 kbps; clearly better than \
         ATRAC3 at the same bitrate and the mainstay of late MD and flash Walkmans. The catch: \
         extensions are still .oma/.at3/.aa3 and mix with ATRAC3, and .oma comes encrypted or not. \
         ffmpeg only decodes; atracdenc supports only the 352k profile. Better at low bitrates than \
         ATRAC3, older than ATRAC9.",
    ),
    (
        "atrac_al",
        "Sony's lossless format for SonicStage in 2006 (ATRAC Advanced Lossless): an ATRAC3/3+ core \
         plus a lossless residual that restores the original bit-for-bit. The catch: it exists almost \
         only in SonicStage .oma/.aa3 files, often wrapped in OpenMG; ffmpeg knows only the \
         atrac3al/atrac3plusal codec names and only decodes, and this project has no real sample for \
         end-to-end testing. No cross-platform ecosystem.",
    ),
    (
        "atrac9",
        "Sony's codec for PS4/PS Vita in 2013 (.at9), aimed at efficient lossy audio in games and \
         portable devices. It is newer than ATRAC3+ but confined to Sony platforms, with little \
         public documentation or tooling. Patents are active, so this app only identifies it and does \
         not promise decoding; ordinary players ignore it. More efficient but more closed than \
         ATRAC3+, with no cross-platform ecosystem like Opus/AAC.",
    ),
    (
        "mkv",
        "Matroska (.mkv) is an open container specification maintained by the community since 2002. \
         It defines no codecs itself; it packs video, multiple audio tracks, subtitles and chapters \
         into an extensible EBML structure and accepts almost any codec. The catch is player support: \
         browsers and iPhones do not natively play MKV and need remuxing or re-encoding. For \
         container-only changes it is the safest target: video and audio can be copied without \
         quality loss.",
    ),
    (
        "mp4",
        "MP4 (ISO/IEC 14496-14) is the de facto container for Apple devices, mobile and web video. It \
         accepts H.264/H.265/AV1 video and AAC/AC-3/ALAC/MP3 audio. The catch is a limited codec \
         matrix: FLAC, Vorbis and Opus inside MP4 are unplayable on most devices, so audio often needs \
         re-encoding. Choose it for compatibility with phones and editors; choose MKV for lossless \
         container changes.",
    ),
    (
        "webm",
        "WebM is Google's open container from 2010, essentially a Matroska subset designed for web \
         playback. It accepts only VP8/VP9/AV1 video and Opus/Vorbis audio, and browsers play it \
         natively without plug-ins. The catch is slow encoding, mediocre editor support and no \
         H.264/AAC—converting from MP4 re-encodes both video and audio. Use it for web backgrounds \
         and open-source demos, not for old devices.",
    ),
    (
        "avi",
        "AVI (1992) is a Windows 3.1-era container with a simple, forgiving structure; many car and \
         DVD players accept only it. Its codec matrix is narrow: video is usually MPEG-4 Part 2 / \
         H.264 / MJPEG, audio MP3/AC-3. The catch: no modern features (variable frame rate, multiple \
         audio track management, H.265 is ignored) and files can hit the 2 GB limit. Use it for old \
         devices; otherwise choose MP4/MKV.",
    ),
];
