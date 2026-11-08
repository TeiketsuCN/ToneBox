//! 格式表 —— 侧栏、能力矩阵、百科文案的唯一数据源。
//!
//! ## 文件归属（并行开发防冲突）
//!
//! | 文件 | 归属 |
//! |---|---|
//! | `format/mod.rs` | 公共 API，改之前先留言 |
//! | `format/lossless.rs` | 机器 A |
//! | `format/lossy.rs` | 机器 B |
//! | `format/device.rs` | 双方都不动（只读格式） |
//!
//! ## 设计原则（重要，别改）
//!
//! 侧栏只列**能编码**的格式，也就是 `can_encode == true` 的那几个。
//! 209 种格式里绝大多数是"输入"，输入靠自动识别，不占 UI。
//! 再加一层 `primary`：分组内默认只显示常用的几个，其余收进"更多"。

pub mod device;
pub mod lossless;
pub mod lossy;
pub mod video;

pub use device::DEVICE;
pub use lossless::LOSSLESS;
pub use lossy::LOSSY;
pub use video::VIDEO;

use crate::ffmpeg::MediaInfo;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    Lossless,
    Lossy,
    Uncompressed,
    Container,
    Encrypted,
    Unknown,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Patent {
    /// 已过期或无专利，随便用
    Free,
    /// 部分专利仍在有效期，法务上有争议
    Expiring,
    /// 明确在保护期内
    Active,
}

impl Patent {
    pub fn label(self) -> &'static str {
        match self {
            Patent::Free => "无专利",
            Patent::Expiring => "专利部分过期",
            Patent::Active => "专利在保护期",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum Group {
    Lossless,
    Lossy,
    Device,
    /// 视频容器（mkv/mp4/webm/avi）。既有只读输入，也能当输出目标。
    Video,
}

impl Group {
    pub fn title(self) -> &'static str {
        match self {
            Group::Lossless => "无损",
            Group::Lossy => "有损",
            Group::Device => "设备专用",
            Group::Video => "视频容器",
        }
    }
    pub fn all() -> [Group; 4] {
        // 视频容器排在设备专用前面：ATRAC 那组还要装外部工具，
        // 视频是所有人都可能用到的目标，不该埋在最底下。
        [Group::Lossless, Group::Lossy, Group::Video, Group::Device]
    }
}

pub struct FormatSpec {
    pub id: &'static str,
    pub name: &'static str,
    pub ext: &'static str,
    pub kind: Kind,
    pub group: Group,
    pub patent: Patent,
    /// 能不能作为输出目标。false 的不会出现在侧栏。
    /// 是否**暴露为输出目标**（产品开关）。
    ///
    /// ⚠️ 这不是"技术上能不能编"。比如 AAC / AC-3 / WMA 用 ffmpeg 都能编，
    /// 但有专利或没人需要，所以这里写 `false`；反过来 Speex 无专利、能编，就写 `true`。
    /// 想表达技术能力请写进 `wiki.rs` 的长文案，不要改这个字段。
    pub can_encode: bool,
    /// 分组内是否默认显示。false 的收进「更多 ▾」。
    pub primary: bool,
    /// 侧栏展开时显示的小字（3 行以内，别写长）
    pub blurb: &'static str,
    /// ffmpeg 编码参数
    pub args: &'static [&'static str],
    /// 相对 PCM 的体积系数（用于侧栏预估）
    pub size_ratio: f32,
}

/// 全部格式（无损 → 有损 → 设备专用 → 视频容器）
pub fn all() -> impl Iterator<Item = &'static FormatSpec> {
    LOSSLESS.iter().chain(LOSSY).chain(DEVICE).chain(VIDEO)
}

/// 视频容器（能当输出的那几个）
pub fn video_containers() -> impl Iterator<Item = &'static FormatSpec> {
    VIDEO.iter().filter(|f| f.can_encode)
}

/// 这个格式是不是视频容器
pub fn is_video(spec: &FormatSpec) -> bool {
    spec.group == Group::Video
}

/// 侧栏里展示的格式：能编码的那些
pub fn browsable() -> impl Iterator<Item = &'static FormatSpec> {
    all().filter(|f| f.can_encode)
}

/// 某个分组里能编码的格式，分成「默认显示」和「更多」
pub fn split_group(group: Group) -> (Vec<&'static FormatSpec>, Vec<&'static FormatSpec>) {
    let mut main = Vec::new();
    let mut more = Vec::new();
    for f in browsable().filter(|f| f.group == group) {
        if f.primary {
            main.push(f);
        } else {
            more.push(f);
        }
    }
    (main, more)
}

pub fn by_id(id: &str) -> Option<&'static FormatSpec> {
    all().find(|f| f.id == id)
}

pub fn default_target() -> &'static FormatSpec {
    by_id("flac").expect("FLAC 必须在表里")
}

/// 源文件 → 可选目标 + 每个目标要提示什么
pub struct Choice {
    pub spec: &'static FormatSpec,
    pub warning: Option<&'static str>,
}

pub fn targets_for(src: Option<&MediaInfo>) -> Vec<Choice> {
    browsable()
        .map(|spec| {
            let warning = src.and_then(|i| match (i.is_lossy(), spec.kind) {
                (true, Kind::Lossless) | (true, Kind::Uncompressed) => {
                    Some("源已经是压缩过的，转成无损不会增加信息量")
                }
                (false, Kind::Lossy) => Some("目标是有损格式，转换会丢弃信息"),
                _ => None,
            });
            Choice { spec, warning }
        })
        .collect()
}

/// 粗估输出体积（字节）。只用于界面给个量级，不追求精确。
///
/// 有固定码率的格式按「码率 × 时长」算——旋钮一调，估计跟着变。
/// 无损、Vorbis 这种没有固定码率的，退回「PCM × size_ratio」的量级估算。
pub fn estimate_size(info: &MediaInfo, spec: &FormatSpec, o: &ConvertOptions) -> u64 {
    if info.duration_s <= 0.0 {
        return 0;
    }
    if is_video(spec) {
        return estimate_video_size(info, spec, o);
    }
    if let Some(kbps) = bitrate_of(spec, o) {
        return (info.duration_s * kbps as f64 * 1000.0 / 8.0) as u64;
    }
    // 选了采样率就按它算——44.1k 转 96k 的无损文件确实会更大
    let sr = output_rate(spec, o).unwrap_or(info.sample_rate);
    if sr == 0 {
        return 0;
    }
    let bits = if spec.kind == Kind::Uncompressed { 24.0 } else { 16.0 };
    let pcm = info.duration_s * sr as f64 * info.channels.max(1) as f64 * bits / 8.0;
    (pcm * spec.size_ratio as f64) as u64
}

/// 视频输出的体积：画面 + 声音一起算。
///
/// 重编码时按「宽 × 高 × 帧率 × 每像素比特」估，再按 CRF 校正——
/// 这不是精确码率（x264 的两遍编码会不一样），只用来给个量级。
pub fn estimate_video_size(info: &MediaInfo, spec: &FormatSpec, o: &ConvertOptions) -> u64 {
    if info.duration_s <= 0.0 {
        return 0;
    }
    let audio_kbps = if info.codec.is_empty() {
        // 纯视频文件没有音轨，别在估算里凭空加 900k
        0
    } else {
        match audio_plan(spec.id, &info.codec) {
            AudioPlan::Copy => info.bitrate_kbps,
            AudioPlan::Encode { kbps, .. } => kbps as u64,
        }
    };
    let video_kbps = match info.video_codec.as_deref() {
        Some(src_v) => {
            let vc = effective_video_codec(spec.id, o, src_v);
            video_estimate_kbps(info, o, vc)
        }
        None => 0,
    };
    (info.duration_s * (audio_kbps + video_kbps) as f64 * 1000.0 / 8.0) as u64
}

/// 估算视频流的码率（kbps）。copy 时尽量用源视频码率，没有就按总码率倒推。
fn video_estimate_kbps(info: &MediaInfo, o: &ConvertOptions, vc: VideoCodec) -> u64 {
    if vc == VideoCodec::Copy {
        if info.video_bitrate_kbps > 0 {
            return info.video_bitrate_kbps;
        }
        // MKV 这类容器不记单流码率：用总码率减掉音频，至少不会差一个数量级
        let total = (info.size_bytes as f64 * 8.0 / info.duration_s / 1000.0) as u64;
        return total.saturating_sub(info.bitrate_kbps);
    }
    if info.width == 0 || info.height == 0 {
        return 0;
    }
    let (w, h) = output_dims(info, o);
    let fps = if o.video_fps > 0 {
        o.video_fps as f64
    } else if info.fps > 0.0 {
        info.fps
    } else {
        30.0
    };
    // bits per pixel per frame：经验值，H.265/VP9/AV1 依次更省
    let bpp = match vc {
        VideoCodec::H264 => 0.09,
        VideoCodec::Hevc => 0.055,
        VideoCodec::Vp9 => 0.06,
        VideoCodec::Av1 => 0.045,
        VideoCodec::Copy => return 0,
    };
    // CRF 每差 6，码率大约差一倍；以 23 为基准
    let crf_factor = 2f64
        .powf((23.0 - o.video_crf.clamp(1, 51) as f64) / 6.0)
        .clamp(0.25, 4.0);
    ((w * h * fps * bpp * crf_factor) / 1000.0).clamp(150.0, 80_000.0) as u64
}

/// 实际输出的画面尺寸（保持宽高比，宽取偶数——H.264 要求）
pub fn output_dims(info: &MediaInfo, o: &ConvertOptions) -> (f64, f64) {
    let (w, h) = (info.width as f64, info.height as f64);
    if o.video_height == 0 || h <= 0.0 {
        return (w, h);
    }
    let nh = o.video_height as f64;
    let nw = ((w / h) * nh / 2.0).round() * 2.0;
    (nw, nh)
}

// ── 质量风险提示 ──────────────────────────────────────────

/// 一条「这么转会怎样」的提示，界面直接渲染成警告卡片。
#[derive(Debug)]
pub struct QualityWarning {
    /// 提示挂在哪个控件下面（界面按这个决定放哪儿，不靠猜标题）
    pub kind: WarningKind,
    pub title: &'static str,
    pub body: String,
}

/// 提示的归属：容器/格式、码率、采样率
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WarningKind {
    /// 有损源转无损：只是换了个更大的容器
    Container,
    /// 无损源转有损：必然丢细节
    Lossy,
    /// 有损→有损，目标码率更低
    LowerBitrate,
    /// 有损→有损，目标码率反而调高
    HigherBitrate,
    /// 显式选了更高的采样率
    Upsample,
    /// 输出采样率比源低（真正在丢高频）
    Downsample,
    /// 输出位深比源低（丢动态范围）
    BitDepthDown,
    /// 输出位深比源高（只是补零）
    BitDepthUp,
}

/// 源文件 → 目标格式 的质量风险。
///
/// 只依赖「有没有损 + 码率 + 采样率」，不判断内容：与其猜"掉多少细节"，
/// 不如把方向和数字说清楚。返回空数组表示没有需要提醒的。
pub fn quality_warnings(
    src: &MediaInfo,
    spec: &FormatSpec,
    o: &ConvertOptions,
) -> Vec<QualityWarning> {
    if is_video(spec) {
        return video_warnings(src, spec, o);
    }
    let mut out = Vec::new();
    let target_lossless = matches!(spec.kind, Kind::Lossless | Kind::Uncompressed);
    let target_kbps = bitrate_of(spec, o);
    let src_kbps = src.bitrate_kbps as u32;
    // DSD 的 sample_rate 是"字节率"、bits 是打包字节数，拿来做采样率/位深比较只会得出怪结论
    let src_dsd = src.codec.starts_with("dsd");

    if src.is_lossy() && target_lossless {
        out.push(QualityWarning {
            kind: WarningKind::Container,
            title: "不会增加任何信息量",
            body: format!(
                "源文件是 {} 有损压缩。转成 {} 只是换了个更大的容器，文件会变大，声音不会变好。",
                src.codec_label(),
                spec.name
            ),
        });
    } else if !src.is_lossy() && spec.kind == Kind::Lossy {
        out.push(QualityWarning {
            kind: WarningKind::Lossy,
            title: "会丢掉一部分细节",
            body: match target_kbps {
                Some(k) => format!(
                    "源文件是 {}（无损）。转成 {} {} kbps 会永久丢掉一部分信息——这是有损编码的定义决定的。留档请选无损格式，随身听可以接受。",
                    src.codec_label(),
                    spec.name,
                    k
                ),
                None => format!(
                    "源文件是 {}（无损）。转成 {} 会永久丢掉一部分信息；留档请选无损格式。",
                    src.codec_label(),
                    spec.name
                ),
            },
        });
    } else if src.is_lossy() && spec.kind == Kind::Lossy {
        if let (Some(k), true) = (target_kbps, src_kbps > 0) {
            // 8 kbps 的容差不值得提醒；差 20% 以上才算"调高码率"
            if k + 8 < src_kbps {
                out.push(QualityWarning {
                    kind: WarningKind::LowerBitrate,
                    title: "目标码率比源文件低",
                    body: format!(
                        "源是 {} {} kbps，目标是 {} {} kbps。这是一次二次有损编码，细节会再少一点；差得越多越明显。",
                        src.codec_label(),
                        src_kbps,
                        spec.name,
                        k
                    ),
                });
            } else if k > src_kbps + src_kbps / 5 {
                out.push(QualityWarning {
                    kind: WarningKind::HigherBitrate,
                    title: "码率调高也不会变好",
                    body: format!(
                        "源已经是 {} {} kbps 的有损文件。重新编码到 {} {} kbps 只会让文件变大：丢掉的高频不会回来，二次编码还会再掉一点。",
                        src.codec_label(),
                        src_kbps,
                        spec.name,
                        k
                    ),
                });
            }
        }
    }

    // 采样率方向：升采样只在用户显式选了更高档时提醒（Dolby 自动上 48k 是规范，不吓人）；
    // 降采样只要实际输出更低就提醒——这是真的在丢高频。DSD 的采样率是字节率，不参与比较。
    if !src_dsd {
        // 没有采样率旋钮的编码器也可能是锁死的（Opus 48k / ATRAC 44.1k），
        // 提示要按它实际会输出的采样率算，但不能因此给 ffmpeg 加 -ar。
        let out_rate = output_rate(spec, o).or_else(|| fixed_rate(spec.id));
        if let (Some(out_rate), true) = (out_rate, src.sample_rate > 0) {
            let can_pick_rate = !rate_choices(spec.id).is_empty();
            if out_rate > src.sample_rate && can_pick_rate && o.sample_rate.is_some() {
                out.push(QualityWarning {
                    kind: WarningKind::Upsample,
                    title: "升采样不会增加信息",
                    body: format!(
                        "源是 {} Hz，输出 {} Hz。高出来的那段频段是空的，文件只会变大，不会更接近母带。要原样保留就把采样率改回「自动」。",
                        src.sample_rate, out_rate
                    ),
                });
            } else if out_rate < src.sample_rate {
                let nyq = (out_rate as f64 / 2000.0 * 10.0).round() / 10.0;
                out.push(QualityWarning {
                    kind: WarningKind::Downsample,
                    title: "降采样会丢掉高频",
                    body: if o.sample_rate.is_some() {
                        format!(
                            "源是 {} Hz，输出 {} Hz。约 {} kHz 以上的频段会被切掉，文件会小一点，但内容回不来；要原样保留就把采样率改回「自动」。",
                            src.sample_rate, out_rate, nyq
                        )
                    } else {
                        format!(
                            "源是 {} Hz，这个格式最高只到 {} Hz，约 {} kHz 以上的频段会被丢掉。",
                            src.sample_rate, out_rate, nyq
                        )
                    },
                });
            }
        }
    }

    // 位深只有无损格式的旋钮能改；压缩型无损最高 24 bit，选「自动」时不比较。
    // 有损源和 DSD 的 bits 没有比较意义。
    let pcm_knob = matches!(spec.id, "wav" | "caf" | "w64" | "aiff" | "pcm");
    let compressed_knob = matches!(spec.id, "flac" | "alac" | "wavpack" | "tta");
    if let (Some(src_bits), true) = (
        src.bits,
        !src_dsd
            && !src.is_lossy()
            && (pcm_knob || compressed_knob)
            && o.bit_depth != BitDepth::Auto,
    ) {
        let (out_bits, label) = match o.bit_depth {
            BitDepth::Int16 => (16u32, "16"),
            BitDepth::Int24 => (24, "24"),
            // 压缩型无损最高 24 bit
            BitDepth::Int32f if compressed_knob => (24, "24"),
            BitDepth::Int32f => (32, "32f"),
            BitDepth::Auto => (0, "自动"),
        };
        if out_bits < src_bits {
            out.push(QualityWarning {
                kind: WarningKind::BitDepthDown,
                title: "降位深会丢掉动态范围",
                body: format!(
                    "源是 {} bit，输出 {} bit。最低的电平会被量化噪声盖住；要原样保留就选 {} bit。",
                    src_bits, label, src_bits
                ),
            });
        } else if out_bits > src_bits {
            out.push(QualityWarning {
                kind: WarningKind::BitDepthUp,
                title: "升位深不会增加精度",
                body: format!(
                    "源是 {} bit，输出 {} bit。多出来的低位是补零，文件只会变大，不会更接近母带。",
                    src_bits, label
                ),
            });
        }
    }
    out
}

/// 视频目标的提示：只说"会不会重编码 / 会不会掉分辨率"，
/// 不套音频那套"有损→无损"的判断（对视频容器没有意义）。
fn video_warnings(
    src: &MediaInfo,
    spec: &FormatSpec,
    o: &ConvertOptions,
) -> Vec<QualityWarning> {
    let mut out = Vec::new();
    let src_v = src.video_codec.clone().unwrap_or_default();
    let has_video = !src_v.is_empty();
    let vc = effective_video_codec(spec.id, o, &src_v);

    if has_video {
        let copy_ok = copy_video_ok(spec.id, &src_v);
        if o.video_codec == VideoCodec::Copy && !copy_ok {
            out.push(QualityWarning {
                kind: WarningKind::Container,
                title: "这个容器装不了原来的画面",
                body: format!(
                    "{} 不在 {} 接受的视频编码里，会自动改用 {} 重新编码；画质会掉一点，速度也慢。想无损换容器就选 MKV。",
                    src_v.to_uppercase(),
                    spec.name,
                    vc.label()
                ),
            });
        } else if o.video_codec != VideoCodec::Copy {
            out.push(QualityWarning {
                kind: WarningKind::Container,
                title: "画面会重新编码",
                body: format!(
                    "原画面是 {}，目标是 {} {}（CRF {}）。重编码一定会有损失；只想换容器就把视频编码改回「直接复制」。",
                    src_v.to_uppercase(),
                    spec.name,
                    vc.label(),
                    o.video_crf
                ),
            });
        } else if o.video_height > 0 || o.video_fps > 0 {
            out.push(QualityWarning {
                kind: WarningKind::Container,
                title: "改分辨率/帧率必须重新编码",
                body: "「直接复制」不能缩放画面，所以会自动改成重编码。".into(),
            });
        }
        if o.video_height > 0 && src.height > 0 && o.video_height < src.height {
            out.push(QualityWarning {
                kind: WarningKind::Container,
                title: "降低分辨率会丢掉画面细节",
                body: format!(
                    "源画面 {}×{}，输出高度 {}。降下去之后回不来；要留原画质就选「保持」。",
                    src.width, src.height, o.video_height
                ),
            });
        }
        if o.video_fps > 0 && src.fps > 0.0 && (o.video_fps as f64) < src.fps - 0.01 {
            out.push(QualityWarning {
                kind: WarningKind::Container,
                title: "降低帧率会丢掉运动细节",
                body: format!(
                    "源是 {:.3} fps，输出 {} fps。快速运动的画面会变卡顿；要原样保留就选「保持」。",
                    src.fps, o.video_fps
                ),
            });
        }
    }

    // 音轨：能复制就复制，不能就得按容器规矩重编——这件事必须说出来
    if !src.codec.is_empty() {
        if let AudioPlan::Encode { label, kbps, .. } = audio_plan(spec.id, &src.codec) {
            let lossy_src = src.is_lossy();
            out.push(QualityWarning {
                kind: WarningKind::Container,
                title: "音轨会被重编码",
                body: if lossy_src {
                    format!(
                        "{} 不在 {} 支持的音频编码里，音轨会转成 {} {}k。多一次有损编码，细节会再少一点。",
                        src.codec_label(),
                        spec.name,
                        label,
                        kbps
                    )
                } else {
                    format!(
                        "{} 不在 {} 支持的音频编码里，音轨会转成 {} {}k。",
                        src.codec_label(),
                        spec.name,
                        label,
                        kbps
                    )
                },
            });
        }
    }
    out
}

/// 真正会用的视频编码。用户选了 copy 但容器不收 / 要缩放时，自动兜底一个能用的。
pub fn effective_video_codec(
    container: &str,
    o: &ConvertOptions,
    src_video: &str,
) -> VideoCodec {
    let fallback = if container == "webm" {
        VideoCodec::Vp9
    } else {
        VideoCodec::H264
    };
    let mut c = o.video_codec;
    if c == VideoCodec::Copy {
        let src_bad = !src_video.is_empty() && !copy_video_ok(container, src_video);
        // copy 不能缩放/改帧率，必须换个真编码器
        let needs_filter = o.video_height > 0 || o.video_fps > 0;
        if src_bad || needs_filter {
            c = fallback;
        }
    }
    if !video_codecs_for(container).contains(&c) {
        c = fallback;
    }
    c
}

// ── 输出参数 ────────────────────────────────────────────

/// 位深。0 位深这种概念在音频里没有，所以用枚举而不是整数。
#[derive(Clone, Copy, PartialEq, Eq, Debug, serde::Serialize, serde::Deserialize)]
pub enum BitDepth {
    /// 跟源文件一样（压缩型无损的默认值：16 进 16 出、24 进 24 出）
    Auto,
    Int16,
    Int24,
    Int32f,
}

impl BitDepth {
    pub fn label(self) -> &'static str {
        match self {
            BitDepth::Auto => "自动（跟源）",
            BitDepth::Int16 => "16-bit",
            BitDepth::Int24 => "24-bit",
            BitDepth::Int32f => "32-bit 浮点",
        }
    }
    /// 小端（WAV / CAF / W64 / 裸 PCM 用）
    pub fn codec_le(self) -> &'static str {
        match self {
            // Auto 只存在于压缩型无损，那边的参数由 compressed_fmt() 管；
            // 这里给个安全兜底，别让 PCM 走到空字符串
            BitDepth::Auto => "pcm_s24le",
            BitDepth::Int16 => "pcm_s16le",
            BitDepth::Int24 => "pcm_s24le",
            BitDepth::Int32f => "pcm_f32le",
        }
    }
    /// 大端（AIFF 用）
    pub fn codec_be(self) -> &'static str {
        match self {
            BitDepth::Auto => "pcm_s24be",
            BitDepth::Int16 => "pcm_s16be",
            BitDepth::Int24 => "pcm_s24be",
            BitDepth::Int32f => "pcm_f32be",
        }
    }
    /// 压缩型无损（FLAC / ALAC / WavPack / TTA）的 -sample_fmt。
    /// `Auto` 返回 None = 不传，编码器自己跟着源位深走。
    pub fn compressed_fmt(self, id: &str) -> Option<&'static str> {
        let (low, high) = match id {
            // ALAC / WavPack 只吃 planar
            "alac" | "wavpack" => ("s16p", "s32p"),
            _ => ("s16", "s32"),
        };
        match self {
            BitDepth::Auto => None,
            BitDepth::Int16 => Some(low),
            BitDepth::Int24 | BitDepth::Int32f => Some(high),
        }
    }

    /// 裸 PCM 的 muxer 名
    pub fn raw_fmt(self) -> &'static str {
        match self {
            BitDepth::Auto => "s24le",
            BitDepth::Int16 => "s16le",
            BitDepth::Int24 => "s24le",
            BitDepth::Int32f => "f32le",
        }
    }
}

/// 视频流的处理方式。
///
/// `Copy` 是默认：绝大多数需求只是换个容器，重编码既慢又掉画质。
/// 要改分辨率/帧率时才必须选一个真编码器（copy 不能缩放）。
#[derive(Clone, Copy, PartialEq, Eq, Debug, serde::Serialize, serde::Deserialize)]
pub enum VideoCodec {
    /// 直接封装，不动画面
    Copy,
    H264,
    Hevc,
    Vp9,
    Av1,
}

impl VideoCodec {
    pub fn label(self) -> &'static str {
        match self {
            VideoCodec::Copy => "直接复制",
            VideoCodec::H264 => "H.264",
            VideoCodec::Hevc => "H.265",
            VideoCodec::Vp9 => "VP9",
            VideoCodec::Av1 => "AV1",
        }
    }
    /// 短标签（旋钮上放得下）
    pub fn short(self) -> &'static str {
        match self {
            VideoCodec::Copy => "复制",
            VideoCodec::H264 => "H.264",
            VideoCodec::Hevc => "H.265",
            VideoCodec::Vp9 => "VP9",
            VideoCodec::Av1 => "AV1",
        }
    }
    /// ffmpeg 编码器名；`Copy` 没有编码器
    pub fn encoder(self) -> Option<&'static str> {
        Some(match self {
            VideoCodec::Copy => return None,
            VideoCodec::H264 => "libx264",
            VideoCodec::Hevc => "libx265",
            VideoCodec::Vp9 => "libvpx-vp9",
            VideoCodec::Av1 => "libsvtav1",
        })
    }

    /// 这个编码有哪些可用的 ffmpeg 编码器（按优先级）。
    ///
    /// 随包那份 ffmpeg 没有 libx264/libx265（授权原因），但带了
    /// libopenh264 和 libkvazaar；用户自己的 ffmpeg 可能反过来。
    /// 所以不能把编码器名写死，运行时按"这个构建里有什么"挑第一个。
    pub fn candidates(self) -> &'static [&'static str] {
        match self {
            VideoCodec::Copy => &[],
            VideoCodec::H264 => &["libx264", "libopenh264"],
            VideoCodec::Hevc => &["libx265", "libkvazaar"],
            VideoCodec::Vp9 => &["libvpx-vp9"],
            VideoCodec::Av1 => &["libsvtav1", "libaom-av1", "librav1e"],
        }
    }
}

/// 从"这个构建里真有的编码器"里挑一个能编 `codec` 的。
///
/// `stored` 是配置里上次记住的编码器名：还在、还对就继续用，
/// 否则退回优先级列表第一个可用的。都没有就返回 None（界面该画暗）。
pub fn resolve_video_encoder(
    codec: VideoCodec,
    available: &std::collections::HashSet<String>,
    stored: &str,
) -> Option<&'static str> {
    let cands = codec.candidates();
    if !stored.is_empty() && cands.contains(&stored) && available.contains(stored) {
        return cands.iter().copied().find(|e| *e == stored);
    }
    cands.iter().copied().find(|e| available.contains(*e))
}

/// args 里实际用的编码器名（配置没记/记的不对时用默认候选）。
pub fn video_encoder_name(codec: VideoCodec, stored: &str) -> &'static str {
    let cands = codec.candidates();
    if cands.is_empty() {
        return "";
    }
    if cands.contains(&stored) {
        return cands.iter().copied().find(|e| *e == stored).unwrap_or(cands[0]);
    }
    cands[0]
}

/// 某个容器能选哪些视频编码（第一个永远是 copy 能力的占位，
/// 真正能不能 copy 要看源视频的编码，见 `copy_video_ok`）
pub fn video_codecs_for(container: &str) -> &'static [VideoCodec] {
    match container {
        "mkv" => &[
            VideoCodec::Copy,
            VideoCodec::H264,
            VideoCodec::Hevc,
            VideoCodec::Vp9,
            VideoCodec::Av1,
        ],
        "mp4" => &[
            VideoCodec::Copy,
            VideoCodec::H264,
            VideoCodec::Hevc,
            VideoCodec::Av1,
        ],
        "webm" => &[VideoCodec::Copy, VideoCodec::Vp9, VideoCodec::Av1],
        "avi" => &[VideoCodec::Copy, VideoCodec::H264],
        _ => &[],
    }
}

/// 源视频的编码能不能直接装进这个容器（不能就得重编码或换容器）。
pub fn copy_video_ok(container: &str, codec: &str) -> bool {
    let c = codec.to_ascii_lowercase();
    match container {
        // Matroska 基本通吃
        "mkv" => true,
        "mp4" => matches!(c.as_str(), "h264" | "hevc" | "av1"),
        // WebM 只认这两个（VP8 也算，但这里只开放 VP9/AV1）
        "webm" => matches!(c.as_str(), "vp8" | "vp9" | "av1"),
        // AVI 的老规矩：MPEG-4 Part 2 / H.264 都有人认，但容器里放 H.265 基本没设备能放
        "avi" => matches!(c.as_str(), "mpeg4" | "msmpeg4v3" | "h264" | "mjpeg"),
        _ => false,
    }
}

/// 视频容器里音频流的处理方式。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum AudioPlan {
    /// 原样复制（源编码这个容器收）
    Copy,
    /// 重编码成容器收的编码
    Encode {
        codec: &'static str,
        kbps: u32,
        label: &'static str,
    },
}

impl AudioPlan {
    pub fn label(self) -> String {
        match self {
            AudioPlan::Copy => "从源复制".to_string(),
            AudioPlan::Encode { label, kbps, .. } => format!("{label} {kbps}k"),
        }
    }
}

/// 这个容器收哪些音频编码（可以直接 `-c:a copy`）
fn audio_copy_ok(container: &str, codec: &str) -> bool {
    let c = codec.to_ascii_lowercase();
    match container {
        "mkv" => matches!(
            c.as_str(),
            "aac" | "aac_latm" | "mp3" | "mp2" | "ac3" | "eac3" | "dts" | "flac"
                | "vorbis" | "opus" | "alac"
        ) || c.starts_with("pcm_"),
        "mp4" => matches!(
            c.as_str(),
            "aac" | "aac_latm" | "alac" | "ac3" | "eac3" | "mp3" | "mp2"
        ),
        "webm" => matches!(c.as_str(), "opus" | "vorbis"),
        "avi" => matches!(c.as_str(), "mp3" | "mp2" | "ac3"),
        _ => false,
    }
}

/// 目标容器的音频方案：源编码能 copy 就 copy，否则按容器规矩重编。
///
/// 注意这是**有损/无损混着来**的地方：MP4 配 FLAC 源只能转 AAC，
/// 界面上要把这件事说出来，别让用户以为音轨没动过。
pub fn audio_plan(container: &str, src_codec: &str) -> AudioPlan {
    if !src_codec.is_empty() && audio_copy_ok(container, src_codec) {
        return AudioPlan::Copy;
    }
    match container {
        "mp4" => AudioPlan::Encode {
            codec: "aac",
            kbps: 256,
            label: "AAC",
        },
        "webm" => AudioPlan::Encode {
            codec: "libopus",
            kbps: 128,
            label: "Opus",
        },
        "avi" => AudioPlan::Encode {
            codec: "libmp3lame",
            kbps: 192,
            label: "MP3",
        },
        // MKV 什么都收；走到这里说明源是 WMA/ATRAC/DSD 之类，
        // 转 FLAC 至少不再丢东西（DSD 会被解码成 PCM，这是格式决定的）
        _ => AudioPlan::Encode {
            codec: "flac",
            kbps: 900,
            label: "FLAC",
        },
    }
}

/// 容器的一句话兼容性说明（输出列常驻）
pub fn container_note(container: &str) -> &'static str {
    match container {
        "mkv" => "Matroska 通吃；视频直接复制，音频能复制就复制。",
        "mp4" => "MP4 只收 H.264/H.265/AV1；音频只收 AAC/AC-3/ALAC/MP3。",
        "webm" => "WebM 只收 VP9/AV1 + Opus/Vorbis，浏览器里最省心。",
        "avi" => "AVI 给老设备用：画面 H.264、声音 MP3 最稳。",
        _ => "",
    }
}

/// 视频参数下面的小字：copy 不能缩放/改帧率，这条比什么都重要
pub fn video_knob_note(container: &str) -> &'static str {
    match container {
        "mp4" => "H.265 会加 hvc1 标记，苹果设备才认。",
        "webm" => "VP9/AV1 编码很慢，长视频建议先剪短。",
        "avi" => "AVI 装 H.265 基本没有播放器认，别选。",
        _ => "「直接复制」最快且不掉画质；要改分辨率/帧率才需要重编码。",
    }
}

/// 用户可调的输出参数。默认值就是各格式的推荐值。
///
/// `#[serde(default)]` 是给旧配置留的后路：新版本加了字段之后，
/// 老 `config.json` 里缺的字段会补默认值，而不是让整个设置解析失败被重置。
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct ConvertOptions {
    /// FLAC 压缩等级 0~12（不影响音质，只影响体积和速度）
    pub flac_level: u8,
    /// PCM 位深
    pub bit_depth: BitDepth,
    /// MP3 码率 kbps
    pub mp3_bitrate: u32,
    /// Opus 码率 kbps
    pub opus_bitrate: u32,
    /// Vorbis 质量 0~10
    pub vorbis_q: u8,
    /// AAC 码率 kbps
    pub aac_bitrate: u32,
    /// AC-3 码率 kbps（固定 48 kHz 输出）
    pub ac3_bitrate: u32,
    /// E-AC-3 码率 kbps（固定 48 kHz 输出）
    pub eac3_bitrate: u32,
    /// WMA（wmav2）码率 kbps
    pub wma_bitrate: u32,
    /// ATRAC3 码率 kbps（66 = LP4，132 = LP2；走外部的 atracdenc）
    pub atrac3_bitrate: u32,
    /// 输出采样率（Hz）。`None` = 自动：不传 `-ar`，跟着源文件走
    /// （Dolby 例外，自动按 48 kHz 规范）。
    pub sample_rate: Option<u32>,
    /// 视频流怎么处理（只有视频容器看它）
    pub video_codec: VideoCodec,
    /// 视频质量：CRF，越小越清晰也越大（18~32，默认 23）
    pub video_crf: u8,
    /// 输出视频高度（0 = 保持源分辨率；宽按比例自动算）
    pub video_height: u32,
    /// 输出帧率（0 = 保持源帧率）
    pub video_fps: u32,
    /// 实际用的 ffmpeg 视频编码器名（libx264 / libopenh264 / libx265 …）。
    /// 空 = 用该编码的首选；换机器后找不到时由 `resolve_video_encoder` 自动换。
    pub video_encoder: String,
}

impl Default for ConvertOptions {
    fn default() -> Self {
        Self {
            flac_level: 8,
            bit_depth: BitDepth::Auto,
            mp3_bitrate: 320,
            opus_bitrate: 96,
            vorbis_q: 6,
            aac_bitrate: 256,
            ac3_bitrate: 448,
            eac3_bitrate: 640,
            wma_bitrate: 192,
            atrac3_bitrate: 132,
            sample_rate: None,
            video_codec: VideoCodec::Copy,
            video_crf: 23,
            video_height: 0,
            video_fps: 0,
            video_encoder: String::new(),
        }
    }
}

/// 某个格式可选的码率档位（kbps）。空数组表示这个格式没有码率旋钮。
///
/// 界面照着这张表画按钮，不要在 GUI 里另写一份——那是格式知识，归 core。
pub fn bitrate_choices(id: &str) -> &'static [u32] {
    match id {
        "mp3" => &[128, 192, 256, 320],
        "opus" => &[64, 96, 128, 192],
        "aac" => &[128, 192, 256, 320],
        // AC-3 的合法码率是一串固定档位（32/40/48/56/64/80/96/112/128/160/192/
        // 224/256/320/384/448/512/576/640），界面只放常用的四档，
        // 448 是 DVD 立体声的默认值。
        "ac3" => &[192, 320, 448, 640],
        // E-AC-3 能到 6144，但立体声音乐超过 1024 没有意义，
        // 而且码率一高，老功放解码会先崩。
        "eac3" => &[192, 384, 640, 1024],
        // 老播放器通常只认到 192；给到 320 是留给新播放器。
        "wma" => &[128, 192, 256, 320],
        // ATRAC3 在 atracdenc 里是两个编码器名字，不是一个码率参数：
        // LP4 = 66k、LP2 = 132k（105k 只有 RealMedia 容器支持，本项目不做）
        "atrac3" => &[66, 132],
        _ => &[],
    }
}

/// 把配置里可能过时的值吸附到最近的合法档位。
///
/// 用户可能拿着上一版的 config.json，或者以后档位表改过；
/// 与其把一个 ffmpeg 不接受的值丢过去，不如就近取一档。
fn snap_bitrate(id: &str, v: u32) -> u32 {
    let choices = bitrate_choices(id);
    if choices.is_empty() || choices.contains(&v) {
        return v;
    }
    *choices
        .iter()
        .min_by_key(|c| c.abs_diff(v))
        .expect("档位表非空")
}

/// 这个格式在当前参数下的目标码率（kbps）。没有固定码率时返回 `None`。
///
/// 用来算体积估计，也用来让「输出」摘要和按钮上的数字对得上。
pub fn bitrate_of(spec: &FormatSpec, o: &ConvertOptions) -> Option<u32> {
    Some(snap_bitrate(
        spec.id,
        match spec.id {
            "mp3" => o.mp3_bitrate,
            "opus" => o.opus_bitrate,
            "aac" => o.aac_bitrate,
            "ac3" => o.ac3_bitrate,
            "eac3" => o.eac3_bitrate,
            "wma" => o.wma_bitrate,
            "atrac3" => o.atrac3_bitrate,
            // ATRAC1 只有 292k、ATRAC3+ 的 atracdenc 只支持 352k，都没有旋钮
            "atrac1" => 292,
            "atrac3p" => 352,
            // MP2 目前没做旋钮，码率写死在表里
            "mp2" => 256,
            _ => return None,
        },
    ))
}

/// 旋钮下面的一行小字：只写「选这个值时要注意什么」，格式介绍放 `blurb` / 百科。
pub fn knob_note(spec: &FormatSpec) -> Option<&'static str> {
    Some(match spec.id {
        "ac3" => "448k 是 DVD 立体声的默认值；多声道要按声道数另算。",
        "eac3" => "E-AC-3 同码率比 AC-3 省，但老功放、老电视只认 AC-3。",
        "wma" => "老播放器通常只认 128~192k，调高不一定放得动。",
        "atrac3" => "132k = LP2，66k = LP4。这档由外部的 atracdenc 决定，不是 ffmpeg。",
        _ => return None,
    })
}

/// 某个格式能输出的采样率档位（Hz）。空数组表示这个格式不开放采样率。
///
/// 档位全部用随包那份 ffmpeg 逐个试过，不是照规范抄的：
/// - libopus 无论 `-ar` 传什么，输出流都是 48k（它内部就这么设计），所以 Opus 没这个旋钮
/// - AAC-LC 只到 96k，192k 会被 ffmpeg 偷偷降到 96k
/// - Dolby 两个只支持 32/44.1/48，这里收 44.1/48
pub fn rate_choices(id: &str) -> &'static [u32] {
    match id {
        "flac" | "alac" | "wavpack" | "tta" | "wav" | "aiff" | "caf" | "w64" | "pcm" => {
            &[44100, 48000, 96000, 192000]
        }
        "mp3" | "mp2" => &[32000, 44100, 48000],
        "aac" => &[44100, 48000, 96000],
        "vorbis" => &[44100, 48000, 96000],
        "ac3" | "eac3" => &[44100, 48000],
        "wma" => &[44100, 48000],
        _ => &[],
    }
}

/// 把配置里可能过时的采样率吸附到最近的合法档位（和 `snap_bitrate` 同理）。
fn snap_rate(id: &str, v: u32) -> u32 {
    let choices = rate_choices(id);
    if choices.is_empty() || choices.contains(&v) {
        return v;
    }
    *choices
        .iter()
        .min_by_key(|c| c.abs_diff(v))
        .expect("档位表非空")
}

/// 实际要写进 ffmpeg 的采样率。`None` = 不传 `-ar`，跟着源文件走。
///
/// Dolby 是例外：用户没选时按规范给 48 kHz，而不是跟源。
/// 编码器写死的输出采样率（界面不给旋钮，但它一定会重采样到这个值）。
pub fn fixed_rate(id: &str) -> Option<u32> {
    match id {
        // libopus 内部只按 48 kHz 工作
        "opus" => Some(48000),
        // atracdenc 只吃 44.1 kHz
        "atrac1" | "atrac3" | "atrac3p" => Some(44100),
        _ => None,
    }
}

pub fn output_rate(spec: &FormatSpec, o: &ConvertOptions) -> Option<u32> {
    if rate_choices(spec.id).is_empty() {
        return None;
    }
    match o.sample_rate {
        Some(r) => Some(snap_rate(spec.id, r)),
        None if matches!(spec.id, "ac3" | "eac3") => Some(48000),
        None => None,
    }
}

/// 采样率旋钮下面那行小字。
pub fn rate_note(spec: &FormatSpec) -> &'static str {
    match spec.id {
        // Dolby 没有「自动」这个选项：它的默认值就是 48k，界面直接高亮 48k
        "ac3" | "eac3" => "默认 48 kHz（DVD/蓝光规范）；44.1k 能编，但不是标准做法。",
        _ => "自动 = 保持源采样率。升采样不会增加信息，只会让文件变大。",
    }
}

/// 某个格式在当前参数下有哪些可调的旋钮——UI 用它决定显示什么。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Knob {
    FlacLevel,
    BitDepth,
    Mp3Bitrate,
    OpusBitrate,
    VorbisQuality,
    AacBitrate,
    Ac3Bitrate,
    Eac3Bitrate,
    WmaBitrate,
    /// ATRAC3 的 LP2 / LP4 档（66 / 132 kbps），走外部 atracdenc
    Atrac3Bitrate,
    /// 输出采样率（自动 / 44.1k / 48k / 96k / 192k……），档位见 `rate_choices`
    SampleRate,
    /// 视频容器：视频流编码方式（复制 / H.264 / H.265 / VP9 / AV1）
    VideoCodec,
    /// 视频容器：画面质量（CRF）
    VideoQuality,
    /// 视频容器：输出高度（保持 / 1080p / 720p …）
    VideoScale,
    /// 视频容器：输出帧率（保持 / 30 / 60）
    VideoFps,
}

/// 这个格式能调什么。空数组表示"没得调，用默认值"。
pub fn knobs_for(spec: &FormatSpec) -> &'static [Knob] {
    match spec.id {
        "flac" => &[Knob::FlacLevel, Knob::BitDepth, Knob::SampleRate],
        "wav" | "caf" | "w64" => &[Knob::BitDepth, Knob::SampleRate],
        "aiff" => &[Knob::BitDepth, Knob::SampleRate],
        "pcm" => &[Knob::BitDepth, Knob::SampleRate],
        "alac" | "wavpack" | "tta" => &[Knob::BitDepth, Knob::SampleRate],
        "mp3" => &[Knob::Mp3Bitrate, Knob::SampleRate],
        // Opus 输出流固定 48k，给采样率旋钮只会骗人
        "opus" => &[Knob::OpusBitrate],
        "vorbis" => &[Knob::VorbisQuality, Knob::SampleRate],
        "aac" => &[Knob::AacBitrate, Knob::SampleRate],
        "ac3" => &[Knob::Ac3Bitrate, Knob::SampleRate],
        "eac3" => &[Knob::Eac3Bitrate, Knob::SampleRate],
        "wma" => &[Knob::WmaBitrate, Knob::SampleRate],
        "mp2" => &[Knob::SampleRate],
        // ATRAC 的采样率由 atracdenc 固定成 44.1k，只有 ATRAC3 有码率可选
        "atrac3" => &[Knob::Atrac3Bitrate],
        // 视频容器：画面参数 + 音频跟容器走（不给音频旋钮，免得和容器打架）
        "mkv" | "mp4" | "webm" | "avi" => &[
            Knob::VideoCodec,
            Knob::VideoQuality,
            Knob::VideoScale,
            Knob::VideoFps,
        ],
        _ => &[],
    }
}

/// 某个格式能选哪些位深。空数组 = 这个格式不该显示位深旋钮。
pub fn bit_depths_for(spec: &FormatSpec) -> &'static [BitDepth] {
    match spec.id {
        // 压缩型无损：可以降/升，但最高 24；默认跟源
        "flac" | "alac" | "wavpack" | "tta" => &[BitDepth::Auto, BitDepth::Int16, BitDepth::Int24],
        "wav" | "caf" | "w64" | "aiff" | "pcm" => {
            &[BitDepth::Int16, BitDepth::Int24, BitDepth::Int32f]
        }
        _ => &[],
    }
}

/// 这个格式要靠哪个**外部**编码器（不随包分发）。`None` = 走内置 ffmpeg。
///
/// 边界：ATRAC9 不在里面——atracdenc 不支持它，而且专利还在保护期。
pub fn external_encoder(id: &str) -> Option<&'static str> {
    match id {
        "atrac1" | "atrac3" | "atrac3p" => Some(crate::external::NAME),
        _ => None,
    }
}

/// 按参数生成 ffmpeg 编码参数。
///
/// 这是把「静态 args」换成「可调参数」的唯一入口，GUI 不要自己拼参数字符串。
pub fn args_for(spec: &FormatSpec, o: &ConvertOptions) -> Vec<String> {
    args_for_with(spec, o, None)
}

/// 和 `args_for` 一样，但能看见源文件信息。
///
/// 视频容器需要它：音频能不能 `copy`、视频编码能不能直接装进容器，
/// 都要看源流的编码。音频格式不需要，传 `None` 即可。
pub fn args_for_with(
    spec: &FormatSpec,
    o: &ConvertOptions,
    src: Option<&MediaInfo>,
) -> Vec<String> {
    if is_video(spec) {
        return video_args(spec, o, src);
    }
    let s = |v: &str| v.to_string();
    // PCM 系列没有「自动」：默认按 24-bit 处理
    let pcm_bit_depth = |b: BitDepth| if b == BitDepth::Auto { BitDepth::Int24 } else { b };
    let mut a = match spec.id {
        "flac" => {
            let mut v = vec![
                s("-c:a"),
                s("flac"),
                s("-compression_level"),
                o.flac_level.min(12).to_string(),
            ];
            if let Some(fmt) = o.bit_depth.compressed_fmt("flac") {
                v.push(s("-sample_fmt"));
                v.push(s(fmt));
            }
            v
        }
        "alac" | "wavpack" | "tta" => {
            let mut v = vec![s("-c:a"), s(spec.id)];
            if let Some(fmt) = o.bit_depth.compressed_fmt(spec.id) {
                v.push(s("-sample_fmt"));
                v.push(s(fmt));
            }
            v
        }
        "wav" | "caf" | "w64" => {
            let bd = pcm_bit_depth(o.bit_depth);
            vec![s("-c:a"), s(bd.codec_le())]
        }
        "aiff" => {
            let bd = pcm_bit_depth(o.bit_depth);
            vec![s("-c:a"), s(bd.codec_be())]
        }
        "pcm" => {
            let bd = pcm_bit_depth(o.bit_depth);
            vec![s("-c:a"), s(bd.codec_le()), s("-f"), s(bd.raw_fmt())]
        }
        "mp3" => vec![
            s("-c:a"),
            s("libmp3lame"),
            s("-b:a"),
            format!("{}k", snap_bitrate("mp3", o.mp3_bitrate)),
        ],
        "opus" => vec![
            s("-c:a"),
            s("libopus"),
            s("-b:a"),
            format!("{}k", snap_bitrate("opus", o.opus_bitrate)),
        ],
        "vorbis" => vec![
            s("-c:a"),
            s("libvorbis"),
            s("-q:a"),
            o.vorbis_q.min(10).to_string(),
        ],
        "aac" => vec![
            s("-c:a"),
            s("aac"),
            s("-b:a"),
            format!("{}k", snap_bitrate("aac", o.aac_bitrate)),
        ],
        // Dolby 的采样率默认 48 kHz（见 output_rate），这里不再写死
        "ac3" => vec![
            s("-c:a"),
            s("ac3"),
            s("-b:a"),
            format!("{}k", snap_bitrate("ac3", o.ac3_bitrate)),
        ],
        "eac3" => vec![
            s("-c:a"),
            s("eac3"),
            s("-b:a"),
            format!("{}k", snap_bitrate("eac3", o.eac3_bitrate)),
        ],
        "wma" => vec![
            s("-c:a"),
            s("wmav2"),
            s("-b:a"),
            format!("{}k", snap_bitrate("wma", o.wma_bitrate)),
        ],
        // 没列的格式用表里的默认参数
        _ => spec.args.iter().map(|a| a.to_string()).collect(),
    };
    // 采样率统一在最后加：用户选了就覆盖，Dolby 没选时按 48k 规范，
    // 其它格式没选就不传 -ar（跟着源走，不做无谓重采样）。
    if let Some(rate) = output_rate(spec, o) {
        a.push(s("-ar"));
        a.push(rate.to_string());
    }
    a
}

/// 视频容器的完整参数：映射第一条视频 + 第一条音轨，再分别决定 copy/重编码。
///
/// `0:v:0?` 的 `?` 是"没有视频流也别报错"：音频文件也能装进 mkv/mp4
/// （就变成只有声音的容器），比直接拒绝更有用。
fn video_args(spec: &FormatSpec, o: &ConvertOptions, src: Option<&MediaInfo>) -> Vec<String> {
    let s = |v: &str| v.to_string();
    let mut a = vec![
        s("-map"),
        s("0:v:0?"),
        s("-map"),
        s("0:a:0?"),
    ];
    let src_v = src.and_then(|i| i.video_codec.clone()).unwrap_or_default();
    let vc = effective_video_codec(spec.id, o, &src_v);
    // 编码器名按配置里记的（可能是 libopenh264 这种），没有就用首选
    let enc = video_encoder_name(vc, &o.video_encoder);
    // 质量参数：只有 libx264/libx265 吃 -crf；openh264/kvazaar 这类只认码率，
    // 就用同一套估算把 CRF 换算成 -b:v，滑块才不会变成摆设。
    let quality = |a: &mut Vec<String>, codec: VideoCodec| {
        let crf = o.video_crf.min(63);
        match enc {
            "libx264" | "libx265" => {
                a.extend([s("-crf"), crf.to_string(), s("-preset"), s("medium")]);
            }
            "libvpx-vp9" | "libaom-av1" => {
                a.extend([s("-crf"), crf.to_string(), s("-b:v"), s("0")]);
            }
            "libsvtav1" => {
                a.extend([s("-crf"), crf.to_string()]);
            }
            "librav1e" => {
                a.extend([s("-qp"), crf.to_string()]);
            }
            _ => {
                let kbps = src
                    .map(|i| video_estimate_kbps(i, o, codec))
                    .unwrap_or(2500)
                    .max(300);
                a.extend([s("-b:v"), format!("{kbps}k")]);
            }
        }
    };
    match vc {
        VideoCodec::Copy => {
            a.push(s("-c:v"));
            a.push(s("copy"));
        }
        VideoCodec::H264 => {
            a.extend([s("-c:v"), s(enc)]);
            quality(&mut a, vc);
        }
        VideoCodec::Hevc => {
            a.extend([s("-c:v"), s(enc)]);
            quality(&mut a, vc);
            // 苹果设备只认带 hvc1 标记的 H.265
            if spec.id == "mp4" {
                a.extend([s("-tag:v"), s("hvc1")]);
            }
        }
        VideoCodec::Vp9 => {
            a.extend([s("-c:v"), s(enc)]);
            quality(&mut a, vc);
            a.extend([s("-row-mt"), s("1")]);
        }
        VideoCodec::Av1 => {
            a.extend([s("-c:v"), s(enc)]);
            quality(&mut a, vc);
        }
    }
    // copy 不能缩放/改帧率（effective_video_codec 已经保证这里不会是 copy）
    if vc != VideoCodec::Copy {
        if o.video_height > 0 {
            a.extend([s("-vf"), format!("scale=-2:{}", o.video_height)]);
        }
        if o.video_fps > 0 {
            a.extend([s("-r"), o.video_fps.to_string()]);
        }
    }

    let src_a = src.map(|i| i.codec.as_str()).unwrap_or("");
    // 纯视频文件没有音轨：别再塞 -c:a（ffmpeg 会忽略，但日志里全是噪音）
    if !src_a.is_empty() {
        match audio_plan(spec.id, src_a) {
            AudioPlan::Copy => {
                a.push(s("-c:a"));
                a.push(s("copy"));
            }
            AudioPlan::Encode { codec, kbps, .. } => {
                a.extend([s("-c:a"), s(codec), s("-b:a"), format!("{kbps}k")]);
            }
        }
    }
    a
}

/// 一把预设——比一屏滑杆有用得多
pub struct Preset {
    pub id: &'static str,
    pub name: &'static str,
    pub note: &'static str,
    pub format: &'static str,
    pub apply: fn(&mut ConvertOptions),
}

pub static PRESETS: &[Preset] = &[
    Preset {
        id: "archive",
        name: "MD 归档",
        note: "无损，体积约 WAV 的六成",
        format: "flac",
        apply: |o| {
            o.flac_level = 8;
        },
    },
    Preset {
        id: "car",
        name: "车载",
        note: "MP3 320，什么都能放",
        format: "mp3",
        apply: |o| {
            o.mp3_bitrate = 320;
        },
    },
    Preset {
        id: "phone",
        name: "手机",
        note: "Opus 96，体积小一半有多",
        format: "opus",
        apply: |o| {
            o.opus_bitrate = 96;
        },
    },
    Preset {
        id: "edit",
        name: "再编辑",
        note: "WAV 24-bit，剪片混音用",
        format: "wav",
        apply: |o| {
            o.bit_depth = BitDepth::Int24;
        },
    },
];

#[cfg(test)]
mod tests {
    use super::*;

    fn spec(id: &str) -> &'static FormatSpec {
        by_id(id).unwrap()
    }

    /// 造一个 MediaInfo，测试里只关心 codec / 码率 / 采样率
    fn mi(codec: &str, kbps: u32, sr: u32) -> MediaInfo {
        MediaInfo {
            codec: codec.into(),
            container: codec.into(),
            sample_rate: sr,
            channels: 2,
            bitrate_kbps: kbps as u64,
            duration_s: 120.0,
            size_bytes: 0,
            bits: Some(16),
            video_codec: None,
            width: 0,
            height: 0,
            fps: 0.0,
            video_bitrate_kbps: 0,
        }
    }

    #[test]
    fn flac_level_reaches_args() {
        let mut o = ConvertOptions::default();
        o.flac_level = 12;
        let a = args_for(spec("flac"), &o);
        assert!(a.contains(&"12".to_string()), "压缩等级没进参数: {a:?}");
        o.flac_level = 0;
        let a = args_for(spec("flac"), &o);
        assert!(a.contains(&"0".to_string()));
    }

    #[test]
    fn bit_depth_changes_pcm_codec() {
        let mut o = ConvertOptions::default();

        o.bit_depth = BitDepth::Int16;
        assert!(args_for(spec("wav"), &o).contains(&"pcm_s16le".to_string()));

        o.bit_depth = BitDepth::Int32f;
        assert!(args_for(spec("wav"), &o).contains(&"pcm_f32le".to_string()));

        // AIFF 必须是大端
        assert!(args_for(spec("aiff"), &o).contains(&"pcm_f32be".to_string()));

        // 裸 PCM 还要带 muxer
        let a = args_for(spec("pcm"), &o);
        assert!(a.contains(&"f32le".to_string()), "裸 PCM 缺 muxer: {a:?}");
    }

    #[test]
    fn bitrate_reaches_args() {
        let mut o = ConvertOptions::default();
        o.mp3_bitrate = 128;
        assert!(args_for(spec("mp3"), &o).contains(&"128k".to_string()));
        o.opus_bitrate = 192;
        assert!(args_for(spec("opus"), &o).contains(&"192k".to_string()));
        o.vorbis_q = 9;
        assert!(args_for(spec("vorbis"), &o).contains(&"9".to_string()));
        o.aac_bitrate = 192;
        assert!(args_for(spec("aac"), &o).contains(&"192k".to_string()));
    }

    #[test]
    fn bit_depth_options_and_args() {
        // 压缩型无损：自动 / 16 / 24（没有 32f）
        assert_eq!(
            bit_depths_for(spec("flac")),
            &[BitDepth::Auto, BitDepth::Int16, BitDepth::Int24]
        );
        // PCM：16 / 24 / 32f（没有自动）
        assert_eq!(
            bit_depths_for(spec("wav")),
            &[BitDepth::Int16, BitDepth::Int24, BitDepth::Int32f]
        );
        assert!(bit_depths_for(spec("mp3")).is_empty());

        let mut o = ConvertOptions::default();
        // 默认「自动」：不传 -sample_fmt，编码器跟源位深
        assert!(!args_for(spec("flac"), &o).contains(&"-sample_fmt".to_string()));
        o.bit_depth = BitDepth::Int16;
        let a = args_for(spec("flac"), &o);
        assert!(a.windows(2).any(|w| w[0] == "-sample_fmt" && w[1] == "s16"));
        o.bit_depth = BitDepth::Int24;
        let a = args_for(spec("alac"), &o);
        assert!(a.windows(2).any(|w| w[0] == "-sample_fmt" && w[1] == "s32p"));
        let a = args_for(spec("wavpack"), &o);
        assert!(a.windows(2).any(|w| w[0] == "-sample_fmt" && w[1] == "s32p"));
        let a = args_for(spec("tta"), &o);
        assert!(a.windows(2).any(|w| w[0] == "-sample_fmt" && w[1] == "s32"));
        // PCM 遇到「自动」兜底成 24-bit
        o.bit_depth = BitDepth::Auto;
        assert!(args_for(spec("wav"), &o).contains(&"pcm_s24le".to_string()));
    }

    #[test]
    fn knobs_match_formats() {
        assert_eq!(knobs_for(spec("flac")), &[Knob::FlacLevel, Knob::BitDepth, Knob::SampleRate]);
        assert_eq!(knobs_for(spec("wav")), &[Knob::BitDepth, Knob::SampleRate]);
        assert_eq!(knobs_for(spec("mp3")), &[Knob::Mp3Bitrate, Knob::SampleRate]);
        assert_eq!(knobs_for(spec("ac3")), &[Knob::Ac3Bitrate, Knob::SampleRate]);
        assert_eq!(knobs_for(spec("eac3")), &[Knob::Eac3Bitrate, Knob::SampleRate]);
        assert_eq!(knobs_for(spec("wma")), &[Knob::WmaBitrate, Knob::SampleRate]);
        // 压缩型无损：位深 + 采样率
        assert_eq!(knobs_for(spec("alac")), &[Knob::BitDepth, Knob::SampleRate]);
        assert_eq!(knobs_for(spec("wavpack")), &[Knob::BitDepth, Knob::SampleRate]);
        // Opus 输出流固定 48k，不该给采样率旋钮
        assert_eq!(knobs_for(spec("opus")), &[Knob::OpusBitrate]);
        assert!(rate_choices("opus").is_empty());
    }

    #[test]
    fn every_knob_has_its_option_table() {
        // 防止「加了 Knob 但忘了档位表」：界面上会出现一排空按钮
        for f in all() {
            for k in knobs_for(f) {
                match k {
                    Knob::FlacLevel
                    | Knob::VorbisQuality
                    | Knob::BitDepth
                    // 视频旋钮的档位来自枚举/固定表，不走码率档位
                    | Knob::VideoCodec
                    | Knob::VideoQuality
                    | Knob::VideoScale
                    | Knob::VideoFps => {}
                    Knob::SampleRate => assert!(
                        !rate_choices(f.id).is_empty(),
                        "{} 有采样率旋钮却没有档位表",
                        f.id
                    ),
                    _ => assert!(
                        !bitrate_choices(f.id).is_empty(),
                        "{} 有码率旋钮却没有档位表",
                        f.id
                    ),
                }
            }
        }
    }

    #[test]
    fn dolby_and_wma_bitrate_reaches_args() {
        let mut o = ConvertOptions::default();

        o.ac3_bitrate = 640;
        let a = args_for(spec("ac3"), &o);
        assert!(a.contains(&"640k".to_string()), "AC-3 码率没进参数: {a:?}");
        // Dolby 必须 48kHz，否则码率档位对不上
        let ar = a.iter().position(|v| v == "-ar").expect("AC-3 缺 -ar");
        assert_eq!(a[ar + 1], "48000");

        o.eac3_bitrate = 1024;
        let a = args_for(spec("eac3"), &o);
        assert!(a.contains(&"1024k".to_string()), "E-AC-3 码率没进参数: {a:?}");
        let ar = a.iter().position(|v| v == "-ar").expect("E-AC-3 缺 -ar");
        assert_eq!(a[ar + 1], "48000");

        o.wma_bitrate = 128;
        assert!(args_for(spec("wma"), &o).contains(&"128k".to_string()));
    }

    #[test]
    fn stale_bitrate_snaps_to_allowed_choice() {
        // 老 config.json 里可能躺着一个现在已经不提供的档位
        let mut o = ConvertOptions::default();
        o.ac3_bitrate = 1000; // 旧值 / 手改值
        let a = args_for(spec("ac3"), &o);
        assert!(
            a.iter().any(|v| bitrate_choices("ac3").iter().any(|c| format!("{c}k") == *v)),
            "非法码率没有吸附到合法档位: {a:?}"
        );

        o.mp3_bitrate = 111;
        let a = args_for(spec("mp3"), &o);
        assert!(a.contains(&"128k".to_string()), "111 应吸附到最近的 128: {a:?}");
    }

    #[test]
    fn sample_rate_reaches_args() {
        let mut o = ConvertOptions::default();
        // 自动 = 不传 -ar，跟着源走，不做无谓重采样
        assert!(!args_for(spec("flac"), &o).iter().any(|v| v == "-ar"));

        o.sample_rate = Some(96000);
        let a = args_for(spec("flac"), &o);
        let i = a.iter().position(|v| v == "-ar").expect("FLAC 缺 -ar");
        assert_eq!(a[i + 1], "96000");

        // 这个格式不支持的采样率要吸附，不能把 192k 塞给 libmp3lame
        o.sample_rate = Some(192000);
        let a = args_for(spec("mp3"), &o);
        let i = a.iter().position(|v| v == "-ar").expect("MP3 缺 -ar");
        assert_eq!(a[i + 1], "48000");

        // Dolby：自动按 48k 规范，选了 44.1k 就按 44.1k
        o.sample_rate = None;
        assert!(args_for(spec("ac3"), &o).contains(&"48000".to_string()));
        o.sample_rate = Some(44100);
        assert!(args_for(spec("ac3"), &o).contains(&"44100".to_string()));

        // Opus 输出流固定 48k，传了也没用，不该出现在参数里
        o.sample_rate = Some(96000);
        assert!(!args_for(spec("opus"), &o).iter().any(|v| v == "-ar"));
    }

    #[test]
    fn estimate_follows_sample_rate() {
        let info = mi("flac", 900, 44100);
        let mut o = ConvertOptions::default();
        let a = estimate_size(&info, spec("wav"), &o);
        o.sample_rate = Some(96000);
        let b = estimate_size(&info, spec("wav"), &o);
        assert!(b > a, "96k 的 PCM 应该比 44.1k 大");
        // 采样率翻倍，PCM 体积也该大约翻倍
        let ratio = b as f64 / a as f64;
        assert!((2.0..2.3).contains(&ratio), "体积比例不对: {ratio}");
    }

    #[test]
    fn quality_warnings_cover_four_directions() {
        let mut o = ConvertOptions::default();

        // ① 高码率有损 → 低码率有损
        o.mp3_bitrate = 128;
        let w = quality_warnings(&mi("mp3", 320, 44100), spec("mp3"), &o);
        assert_eq!(w.len(), 1, "{w:?}");
        assert!(w[0].title.contains("比源文件低"));
        assert!(w[0].body.contains("320") && w[0].body.contains("128"));

        // ② 无损 → 有损
        let w = quality_warnings(&mi("flac", 900, 44100), spec("mp3"), &o);
        assert_eq!(w.len(), 1);
        assert!(w[0].title.contains("细节"), "{}", w[0].title);
        assert!(w[0].body.contains("128 kbps"), "{}", w[0].body);

        // ③ 有损 → 无损（原有提示不能丢）
        let w = quality_warnings(&mi("mp3", 320, 44100), spec("flac"), &o);
        assert_eq!(w.len(), 1);
        assert!(w[0].title.contains("不会增加"));

        // ④ 低码率 → 高码率：说"不会变好"，别说"会丢细节"
        o.mp3_bitrate = 320;
        let w = quality_warnings(&mi("mp3", 128, 44100), spec("mp3"), &o);
        assert_eq!(w.len(), 1);
        assert!(w[0].title.contains("不会变好"), "{}", w[0].title);

        // 码率差不多时不啰嗦
        assert!(quality_warnings(&mi("mp3", 320, 44100), spec("mp3"), &o).is_empty());

        // 升采样只在用户自己选了更高采样率时提醒
        let mut o2 = ConvertOptions::default();
        assert!(quality_warnings(&mi("flac", 900, 44100), spec("flac"), &o2).is_empty());
        o2.sample_rate = Some(96000);
        let w = quality_warnings(&mi("flac", 900, 44100), spec("flac"), &o2);
        assert_eq!(w.len(), 1);
        assert!(w[0].title.contains("升采样"));
    }

    /// 每个带「采样率 / 位深」旋钮的格式，改了值都必须出提示。
    /// 之前是靠人肉点，漏了好几种格式。
    #[test]
    fn every_rate_and_depth_knob_has_a_warning() {
        let src = mi("flac", 900, 44100); // 16 bit 无损
        let mut missing: Vec<String> = Vec::new();
        for f in all() {
            if !f.can_encode {
                continue;
            }
            let knobs = knobs_for(f);
            if knobs.contains(&Knob::SampleRate) {
                let choices = rate_choices(f.id);
                if let Some(&r) = choices.iter().find(|&&r| r != 44100) {
                    let mut o = ConvertOptions::default();
                    o.sample_rate = Some(r);
                    let w = quality_warnings(&src, f, &o);
                    if !w.iter().any(|x| {
                        matches!(
                            x.kind,
                            WarningKind::Upsample | WarningKind::Downsample
                        )
                    }) {
                        missing.push(format!("{} 采样率 {}", f.id, r));
                    }
                }
            }
            if knobs.contains(&Knob::BitDepth) {
                let mut o = ConvertOptions::default();
                o.bit_depth = BitDepth::Int24;
                let w = quality_warnings(&src, f, &o);
                if !w.iter().any(|x| x.kind == WarningKind::BitDepthUp) {
                    missing.push(format!("{} 位深 24", f.id));
                }
            }
        }
        assert!(missing.is_empty(), "这些组合没有提示: {missing:?}");
    }

    /// 锁死采样率的编码器（Opus 48k、ATRAC 44.1k）也必须提示重采样
    #[test]
    fn fixed_rate_encoders_warn_on_resample() {
        let hi = mi("flac", 900, 96000);
        for id in ["opus", "atrac1", "atrac3", "atrac3p"] {
            let o = ConvertOptions::default();
            let w = quality_warnings(&hi, spec(id), &o);
            assert!(
                w.iter().any(|x| x.kind == WarningKind::Downsample),
                "{id} 从 96k 源下来没有降采样提示: {w:?}"
            );
        }
    }

    #[test]
    fn old_config_without_new_fields_still_loads() {
        // 这是加字段时最容易踩的坑：缺字段会让整个 Settings 解析失败被重置
        let old = r#"{
            "flac_level": 8,
            "bit_depth": "Int24",
            "mp3_bitrate": 320,
            "opus_bitrate": 96,
            "vorbis_q": 6
        }"#;
        let o: ConvertOptions = serde_json::from_str(old).expect("老配置应该还能读");
        assert_eq!(o.aac_bitrate, 256);
        assert_eq!(o.ac3_bitrate, 448);
        assert_eq!(o.eac3_bitrate, 640);
        assert_eq!(o.wma_bitrate, 192);
        assert_eq!(o.atrac3_bitrate, 132);
        // 老配置里没有采样率 = 自动，不能变成 Some(0)
        assert_eq!(o.sample_rate, None);
    }

    #[test]
    fn estimate_follows_bitrate() {
        use crate::MediaInfo;
        let info = MediaInfo {
            codec: "flac".into(),
            container: "flac".into(),
            sample_rate: 44100,
            channels: 2,
            bitrate_kbps: 900,
            duration_s: 60.0,
            size_bytes: 0,
            bits: Some(16),
            video_codec: None,
            width: 0,
            height: 0,
            fps: 0.0,
            video_bitrate_kbps: 0,
        };
        let mut o = ConvertOptions::default();
        o.ac3_bitrate = 448;
        let a = estimate_size(&info, spec("ac3"), &o);
        // 60 秒 × 448 kbps ≈ 3.36 MB
        assert!((3_350_000..3_370_000).contains(&a), "448k/60s 应该约 3.36MB，实际 {a}");

        o.ac3_bitrate = 192;
        let b = estimate_size(&info, spec("ac3"), &o);
        assert!(b < a, "码率调低，预计体积应该跟着变小");

        // Vorbis 没有固定码率，走原来的 size_ratio 估算，不该是 0
        assert!(estimate_size(&info, spec("vorbis"), &o) > 0);
    }

    #[test]
    fn atrac_formats_are_external_and_optional() {
        for id in ["atrac1", "atrac3", "atrac3p"] {
            let s = spec(id);
            assert!(s.can_encode, "{id} 应该能作为输出目标");
            assert_eq!(external_encoder(id), Some("atracdenc"), "{id} 应该走外部编码器");
            assert!(encoder_for(id).is_none(), "{id} 不该被当成 ffmpeg 编码器");
            // atracdenc 只吃 44.1k，界面上不该出现采样率旋钮
            assert!(rate_choices(id).is_empty());
        }
        // ATRAC9 不开放：专利在保护期，atracdenc 也不支持
        assert!(external_encoder("atrac9").is_none());
        assert!(!spec("atrac9").can_encode);

        // ATRAC Advanced Lossless：只能读，而且它是真无损（别按 ATRAC 码率表猜）
        assert!(!spec("atrac_al").can_encode);
        assert!(external_encoder("atrac_al").is_none());
        assert_eq!(spec("atrac_al").kind, Kind::Lossless);

        // ATRAC3 只有 LP2 / LP4 两档
        assert_eq!(bitrate_choices("atrac3"), &[66, 132]);
        let mut o = ConvertOptions::default();
        assert_eq!(bitrate_of(spec("atrac3"), &o), Some(132));
        o.atrac3_bitrate = 66;
        assert_eq!(bitrate_of(spec("atrac3"), &o), Some(66));
        assert_eq!(bitrate_of(spec("atrac1"), &o), Some(292));
        assert_eq!(bitrate_of(spec("atrac3p"), &o), Some(352));
    }

    #[test]
    fn atrac_codec_names_match_atracdenc() {
        // 名字写错的话转换时才炸，而且报错来自外部工具，很难查
        assert_eq!(crate::external::codec_for("atrac1", 132), Some("atrac1"));
        assert_eq!(crate::external::codec_for("atrac3", 132), Some("atrac3"));
        assert_eq!(crate::external::codec_for("atrac3", 66), Some("atrac3_lp4"));
        assert_eq!(crate::external::codec_for("atrac3p", 352), Some("atrac3plus"));
        assert_eq!(crate::external::codec_for("flac", 132), None);
    }

    #[test]
    fn atrac_lossless_is_recognised_as_lossless() {
        // 这两个 codec 名是 ffmpeg 的：atrac3al / atrac3plusal。
        // 它们解出来是无损的，所以 is_lossy 必须是 false，
        // 否则界面会劝用户"转无损不会增加信息量"——正好说反。
        for (codec, label) in [("atrac3al", "ATRAC3 AL"), ("atrac3plusal", "ATRAC3+ AL")] {
            let m = mi(codec, 900, 44100);
            assert_eq!(m.codec_label(), label);
            assert!(!m.is_lossy(), "{codec} 不该被判成有损");
        }
    }

    #[test]
    fn presets_point_to_real_formats() {
        for p in PRESETS {
            assert!(by_id(p.format).is_some(), "预设指向了不存在的格式: {}", p.format);
        }
    }

    fn mi_bits(codec: &str, sr: u32, bits: u32, kbps: u32) -> MediaInfo {
        MediaInfo {
            bits: Some(bits),
            ..mi(codec, kbps, sr)
        }
    }

    #[test]
    fn hi_res_to_cd_flags_downsample_and_depth() {
        // 24bit/48k → WAV 16bit/44.1k：两处降级都要提醒
        let src = mi_bits("flac", 48_000, 24, 1018);
        let mut o = ConvertOptions::default();
        o.bit_depth = BitDepth::Int16;
        o.sample_rate = Some(44_100);
        let kinds: Vec<WarningKind> = quality_warnings(&src, spec("wav"), &o)
            .iter()
            .map(|w| w.kind)
            .collect();
        assert!(kinds.contains(&WarningKind::BitDepthDown), "{kinds:?}");
        assert!(kinds.contains(&WarningKind::Downsample), "{kinds:?}");
    }

    #[test]
    fn upsampling_and_up_bit_depth_are_flagged() {
        let src = mi_bits("flac", 44_100, 16, 900);
        let mut o = ConvertOptions::default();
        o.bit_depth = BitDepth::Int24;
        o.sample_rate = Some(192_000);
        let kinds: Vec<WarningKind> = quality_warnings(&src, spec("caf"), &o)
            .iter()
            .map(|w| w.kind)
            .collect();
        assert!(kinds.contains(&WarningKind::Upsample), "{kinds:?}");
        assert!(kinds.contains(&WarningKind::BitDepthUp), "{kinds:?}");
    }

    #[test]
    fn dsd_source_skips_rate_and_depth_warnings() {
        // DSD 的 sample_rate 是字节率、bits 是打包字节数，不能拿来比
        let src = mi_bits("dsd_lsbf_planar", 352_800, 8, 5_644);
        let mut o = ConvertOptions::default();
        o.bit_depth = BitDepth::Int16;
        o.sample_rate = Some(44_100);
        let kinds: Vec<WarningKind> = quality_warnings(&src, spec("wav"), &o)
            .iter()
            .map(|w| w.kind)
            .collect();
        assert!(!kinds.contains(&WarningKind::BitDepthDown), "{kinds:?}");
        assert!(!kinds.contains(&WarningKind::Downsample), "{kinds:?}");
    }

    #[test]
    fn lossy_source_has_no_bit_depth_warning() {
        // 有损源的 bits 没有意义，别提示"降位深"
        let src = mi_bits("mp3", 44_100, 16, 320);
        let mut o = ConvertOptions::default();
        o.bit_depth = BitDepth::Int16;
        let kinds: Vec<WarningKind> = quality_warnings(&src, spec("wav"), &o)
            .iter()
            .map(|w| w.kind)
            .collect();
        assert!(!kinds.contains(&WarningKind::BitDepthDown), "{kinds:?}");
        assert!(!kinds.contains(&WarningKind::BitDepthUp), "{kinds:?}");
    }

    // ── 视频容器 ──

    fn miv(codec: &str, video: &str, w: u32, h: u32, fps: f64, vkbps: u64) -> MediaInfo {
        MediaInfo {
            video_codec: Some(video.into()),
            width: w,
            height: h,
            fps,
            video_bitrate_kbps: vkbps,
            ..mi(codec, 192, 48_000)
        }
    }

    #[test]
    fn video_containers_are_selectable_targets() {
        for id in ["mkv", "mp4", "webm", "avi"] {
            let s = spec(id);
            assert!(s.can_encode, "{id} 应该能当输出");
            assert_eq!(s.group, Group::Video, "{id} 分组不对");
            assert!(is_video(s));
            assert!(
                knobs_for(s).contains(&Knob::VideoCodec),
                "{id} 缺视频编码旋钮"
            );
            // 容器不是音频编码器，探测那套不该把它当音频格式
            assert!(encoder_for(id).is_none(), "{id} 不该走音频编码器探测");
        }
        assert_eq!(
            video_containers().map(|f| f.id).collect::<Vec<_>>(),
            vec!["mkv", "mp4", "webm", "avi"]
        );
    }

    #[test]
    fn video_codec_matrix_respects_container() {
        // WebM 不收 H.264，MP4 不收 VP9（兼容性太差）
        assert!(!video_codecs_for("webm").contains(&VideoCodec::H264));
        assert!(!video_codecs_for("mp4").contains(&VideoCodec::Vp9));
        assert!(video_codecs_for("mkv").contains(&VideoCodec::Av1));
        assert!(copy_video_ok("mkv", "hevc"));
        assert!(!copy_video_ok("mp4", "vp9"));
        assert!(!copy_video_ok("avi", "hevc"));
    }

    #[test]
    fn audio_plan_copies_or_reencodes() {
        // 容器收就复制，别做无谓的有损重编码
        assert_eq!(audio_plan("mkv", "flac"), AudioPlan::Copy);
        assert_eq!(audio_plan("mp4", "aac"), AudioPlan::Copy);
        assert_eq!(audio_plan("webm", "opus"), AudioPlan::Copy);
        // MP4 装不了 FLAC → AAC；DSD 这类更不用想
        assert!(matches!(
            audio_plan("mp4", "flac"),
            AudioPlan::Encode { codec: "aac", .. }
        ));
        assert!(matches!(
            audio_plan("webm", "mp3"),
            AudioPlan::Encode { codec: "libopus", .. }
        ));
    }

    #[test]
    fn video_args_copy_when_possible() {
        let src = miv("aac", "h264", 1920, 1080, 23.976, 4000);
        let a = args_for_with(spec("mkv"), &ConvertOptions::default(), Some(&src));
        assert!(a.windows(2).any(|w| w[0] == "-c:v" && w[1] == "copy"), "{a:?}");
        assert!(a.windows(2).any(|w| w[0] == "-c:a" && w[1] == "copy"), "{a:?}");
        // 两个流都要映射，否则音频文件能出画面、视频文件会丢画面
        assert!(a.windows(2).any(|w| w[0] == "-map" && w[1] == "0:v:0?"));
        assert!(a.windows(2).any(|w| w[0] == "-map" && w[1] == "0:a:0?"));
    }

    #[test]
    fn video_args_fall_back_when_container_rejects_copy() {
        // VP9 装不进 MP4：自动换成 H.264，不能默默产出放不了的 mp4
        let src = miv("aac", "vp9", 1920, 1080, 30.0, 3000);
        let a = args_for_with(spec("mp4"), &ConvertOptions::default(), Some(&src));
        assert!(a.windows(2).any(|w| w[0] == "-c:v" && w[1] == "libx264"), "{a:?}");
        // H.265 要打 hvc1 标记，否则苹果设备不认
        let mut o = ConvertOptions::default();
        o.video_codec = VideoCodec::Hevc;
        let a = args_for_with(spec("mp4"), &o, Some(&src));
        assert!(a.windows(2).any(|w| w[0] == "-tag:v" && w[1] == "hvc1"), "{a:?}");
    }

    #[test]
    fn video_scale_forces_reencode() {
        // copy 不能缩放：选了 720p 就必须换成真编码器
        let src = miv("aac", "h264", 1920, 1080, 30.0, 4000);
        let mut o = ConvertOptions::default();
        o.video_height = 720;
        let a = args_for_with(spec("mkv"), &o, Some(&src));
        assert!(a.windows(2).any(|w| w[0] == "-c:v" && w[1] == "libx264"), "{a:?}");
        assert!(
            a.windows(2)
                .any(|w| w[0] == "-vf" && w[1] == "scale=-2:720"),
            "{a:?}"
        );
        // 提示也要说清楚"这是在重编码"
        let w = quality_warnings(&src, spec("mkv"), &o);
        assert!(w.iter().any(|x| x.title.contains("重新编码")), "{w:?}");
    }

    #[test]
    fn video_size_estimate_adds_video_and_audio() {
        let src = miv("aac", "h264", 1920, 1080, 30.0, 4000);
        let o = ConvertOptions::default();
        let est = estimate_size(&src, spec("mkv"), &o);
        // 120 秒 × (4000 + 192) kbps ≈ 62.9 MB
        assert!(
            (62_000_000..64_000_000).contains(&est),
            "copy 的体积估算不对: {est}"
        );
        // 换成 H.265 重编码应该明显更小
        let mut o2 = ConvertOptions::default();
        o2.video_codec = VideoCodec::Hevc;
        assert!(estimate_size(&src, spec("mkv"), &o2) < est);
    }

    #[test]
    fn video_warn_only_when_something_changes() {
        let src = miv("aac", "h264", 1920, 1080, 30.0, 4000);
        // 原样封装：不该有提示
        assert!(quality_warnings(&src, spec("mkv"), &ConvertOptions::default()).is_empty());
        // 装不进容器：提示会重编码
        let w = quality_warnings(&src, spec("webm"), &ConvertOptions::default());
        assert!(w.iter().any(|x| x.title.contains("装不了")), "{w:?}");
        // 降分辨率：提示会丢细节
        let mut o = ConvertOptions::default();
        o.video_height = 720;
        let w = quality_warnings(&src, spec("mkv"), &o);
        assert!(w.iter().any(|x| x.title.contains("分辨率")), "{w:?}");
    }
}

/// 这个格式要靠哪个 ffmpeg 编码器。
///
/// 用来做运行时探测：二进制里没有这个编码器，界面就把格式标成不可用，
/// 而不是等用户点了转换才报错。
pub fn encoder_for(id: &str) -> Option<&'static str> {
    Some(match id {
        "flac" => "flac",
        // 位深会换具体 pcm_* 名字，这里只确认 pcm 族在
        "wav" | "caf" | "w64" | "pcm" => "pcm_s16le",
        "aiff" => "pcm_s16be",
        "alac" => "alac",
        "wavpack" => "wavpack",
        "tta" => "tta",
        "mp3" => "libmp3lame",
        "opus" => "libopus",
        "vorbis" => "libvorbis",
        "mp2" => "mp2",
        "aac" => "aac",
        "ac3" => "ac3",
        "eac3" => "eac3",
        "wma" => "wmav2",
        _ => return None,
    })
}
