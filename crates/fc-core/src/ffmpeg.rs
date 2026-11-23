//! ffmpeg 子进程封装。
//!
//! 设计决定（见项目简报）：不链接 libav，一律起子进程。
//! 理由之一是崩溃隔离——畸形老文件可能让 ffmpeg segfault，
//! 子进程死了主程序还在，可以报错继续下一个。

use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use anyhow::{anyhow, Context, Result};
use serde::Deserialize;

#[cfg(windows)]
use std::os::windows::process::CommandExt;

/// 不弹出黑框
#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

#[derive(Clone)]
pub struct Tools {
    pub ffmpeg: PathBuf,
    pub ffprobe: PathBuf,
}

/// 当前这份 ffmpeg 是否真的带某个解码器。
/// 这样换一份带 AC-4/Audio Vivid 解码器的开源构建时，程序能自动放行，
/// 不需要再改死代码里的“没有解码器”。
pub fn has_decoder(tools: &Tools, codec: &str) -> bool {
    // 这个函数会被详情页每帧调用：绝不能在 UI 线程每帧起一次 ffmpeg。
    // 只查一次，把 decoder 名字缓存起来。
    static DECODERS: std::sync::OnceLock<std::collections::HashSet<String>> =
        std::sync::OnceLock::new();
    let set = DECODERS.get_or_init(|| {
        let mut cmd = Command::new(&tools.ffmpeg);
        cmd.args(["-hide_banner", "-decoders"]);
        #[cfg(windows)]
        cmd.creation_flags(CREATE_NO_WINDOW);
        let mut set = std::collections::HashSet::new();
        if let Ok(out) = cmd.output() {
            let text = String::from_utf8_lossy(&out.stdout);
            for line in text.lines() {
                for w in line.split_whitespace() {
                    if !w.starts_with('-') {
                        set.insert(w.to_string());
                    }
                }
            }
        }
        set
    });
    set.contains(codec)
}

fn find_in_path(name: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path) {
        let cand = dir.join(name);
        if cand.is_file() {
            return Some(cand);
        }
    }
    None
}

/// 依次尝试：PATH → 程序同目录 → 常见安装位置
pub fn locate() -> Option<Tools> {
    let exe_dir = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(Path::to_path_buf));

    let mut dirs: Vec<PathBuf> = Vec::new();
    if let Some(d) = exe_dir.clone() {
        dirs.push(d.clone());
        dirs.push(d.join("ffmpeg"));
        dirs.push(d.join("bin"));
    }
    if let Some(local) = std::env::var_os("LOCALAPPDATA") {
        dirs.push(PathBuf::from(local).join(r"Microsoft\WinGet\Links"));
    }
    dirs.push(PathBuf::from(r"C:\ffmpeg\bin"));
    dirs.push(PathBuf::from(r"C:\Program Files\ffmpeg\bin"));

    for d in &dirs {
        let f = d.join("ffmpeg.exe");
        let p = d.join("ffprobe.exe");
        if f.is_file() && p.is_file() {
            return Some(Tools {
                ffmpeg: f,
                ffprobe: p,
            });
        }
    }
    match (find_in_path("ffmpeg.exe"), find_in_path("ffprobe.exe")) {
        (Some(f), Some(p)) => Some(Tools {
            ffmpeg: f,
            ffprobe: p,
        }),
        _ => None,
    }
}

#[derive(Deserialize)]
struct ProbeOut {
    #[serde(default)]
    streams: Vec<Stream>,
    format: Option<Format>,
}

#[derive(Deserialize)]
struct Stream {
    codec_type: Option<String>,
    codec_name: Option<String>,
    /// MP4 sample entry（Audio Vivid 的 av3a 在 ffprobe 里 codec_name 是空的）
    codec_tag_string: Option<String>,
    /// 编码 profile（E-AC-3/TrueHD 的 Atmos 标记会出现在这里）
    profile: Option<String>,
    sample_rate: Option<String>,
    channels: Option<u32>,
    bit_rate: Option<String>,
    duration: Option<String>,
    /// FLAC / ALAC / PCM 的真实位深（ffprobe 给的是字符串）
    bits_per_raw_sample: Option<String>,
    bits_per_sample: Option<u32>,
    /// 视频流的像素尺寸 / 帧率（音频流没有）
    width: Option<u32>,
    height: Option<u32>,
    avg_frame_rate: Option<String>,
    r_frame_rate: Option<String>,
    /// 内嵌封面在 ffprobe 里也是一条 video 流，别把它当成"这是视频文件"
    disposition: Option<Disposition>,
    /// 音轨自己的语言/标题（多音轨视频用）
    tags: Option<std::collections::HashMap<String, String>>,
}

#[derive(Deserialize, Default)]
struct Disposition {
    #[serde(default)]
    attached_pic: u32,
}

#[derive(Deserialize)]
struct Format {
    format_name: Option<String>,
    duration: Option<String>,
    bit_rate: Option<String>,
    size: Option<String>,
}

/// 一条音频流的信息（视频文件可能有多条音轨）。
#[derive(Clone, Debug, Default)]
pub struct AudioTrackInfo {
    pub codec: String,
    pub channels: u32,
    pub bitrate_kbps: u64,
    pub language: Option<String>,
    pub title: Option<String>,
}

impl AudioTrackInfo {
    /// 显示用的短标签："AAC · 2ch · eng"
    pub fn label(&self) -> String {
        let mut s = match self.codec.as_str() {
            "ac4" => "杜比全景声 AC-4".to_string(),
            "av3a" => "菁彩声 Audio Vivid".to_string(),
            "eac3_atmos" => "杜比全景声 E-AC-3".to_string(),
            "truehd_atmos" => "杜比全景声 TrueHD".to_string(),
            "ac3" => "杜比 AC-3".to_string(),
            "eac3" => "杜比 E-AC-3".to_string(),
            "truehd" => "杜比 TrueHD".to_string(),
            "dts" => "DTS".to_string(),
            "amr_nb" => "AMR-NB".to_string(),
            "amr_wb" => "AMR-WB".to_string(),
            other => other.to_uppercase(),
        };
        if self.channels > 0 {
            s.push_str(&format!(" · {}ch", self.channels));
        }
        if let Some(l) = self.language.as_deref().filter(|l| !l.is_empty()) {
            s.push_str(&format!(" · {l}"));
        }
        s
    }
}

#[derive(Clone, Debug)]
pub struct MediaInfo {
    pub codec: String,
    pub container: String,
    pub sample_rate: u32,
    pub channels: u32,
    pub bitrate_kbps: u64,
    pub duration_s: f64,
    pub size_bytes: u64,
    /// 位深。无损才有意义（16 / 24 / 32），有损一般为 None。
    pub bits: Option<u32>,
    /// 全部音频流（第一条仍是 `codec` / `bitrate_kbps` 那一条）
    pub audio_tracks: Vec<AudioTrackInfo>,
    /// 视频流编码（h264 / hevc / vp9 / av1 …）。纯音频文件是 None。
    pub video_codec: Option<String>,
    /// 画面宽高（像素）。纯音频文件是 0。
    pub width: u32,
    pub height: u32,
    /// 帧率。ffprobe 给的是 "30000/1001" 这种分数，这里已经算成小数。
    pub fps: f64,
    /// 视频流自己的码率（kbps）。容器不记单流码率时是 0。
    pub video_bitrate_kbps: u64,
}

impl MediaInfo {
    /// 有没有真正的画面（内嵌封面不算）
    pub fn has_video(&self) -> bool {
        self.video_codec.is_some() || (self.width > 0 && self.height > 0)
    }

    /// 视频编码的短标签；没有画面就 None
    pub fn video_label(&self) -> Option<&str> {
        let c = self.video_codec.as_deref()?;
        Some(match c {
            "h264" => "H.264",
            "hevc" => "H.265",
            "vp8" => "VP8",
            "vp9" => "VP9",
            "av1" => "AV1",
            "mpeg4" => "MPEG-4",
            "mpeg2video" => "MPEG-2",
            "msmpeg4v3" => "DivX",
            "mjpeg" => "MJPEG",
            other => other,
        })
    }
}

impl MediaInfo {
    /// 位深标签：24 -> "24 bit"；不知道就空串
    pub fn bits_label(&self) -> String {
        self.bits.map(|b| format!("{b} bit")).unwrap_or_default()
    }
}

impl MediaInfo {
    /// 该编码是否已经有损压缩过。MP3→FLAC 这种"往大容器里搬"的判断全靠它。
    pub fn is_lossy(&self) -> bool {
        matches!(
            self.codec.as_str(),
            "mp3"
                | "mp3float"
                | "aac"
                | "aac_latm"
                | "vorbis"
                | "opus"
                | "wmav1"
                | "wmav2"
                | "atrac1"
                | "atrac3"
                | "atrac3p"
                | "atrac3plus"
                | "ac3"
                | "eac3"
                | "mp2"
                | "cook"
                | "musepack7"
                | "mpc8"
                // AC-4 / Audio Vivid 都是有损空间音频，不能当 Hi-Res 无损
                | "ac4"
                | "av3a"
                | "eac3_atmos"
                | "dts"
                | "amr_nb"
                | "amr_wb"
        )
    }

    pub fn codec_label(&self) -> &str {
        match self.codec.as_str() {
            "mp3" | "mp3float" => "MP3",
            "aac" | "aac_latm" => "AAC",
            "vorbis" => "Vorbis",
            "opus" => "Opus",
            "flac" => "FLAC",
            "alac" => "ALAC",
            "pcm_s16le" | "pcm_s24le" | "pcm_s32le" | "pcm_f32le" => "PCM",
            "atrac1" => "ATRAC1",
            "atrac3" => "ATRAC3",
            "atrac3p" | "atrac3plus" => "ATRAC3+",
            // ATRAC Advanced Lossless 的两个 codec 名（只解不编）
            "atrac3al" => "ATRAC3 AL",
            "atrac3plusal" => "ATRAC3+ AL",
            "dsd_lsbf" | "dsd_msbf" | "dsd_lsbf_planar" | "dsd_msbf_planar" => "DSD",
            "ac4" => "杜比全景声 AC-4",
            "av3a" => "菁彩声 Audio Vivid",
            "eac3_atmos" => "杜比全景声 E-AC-3",
            "truehd_atmos" => "杜比全景声 TrueHD",
            "ac3" => "杜比 AC-3",
            "eac3" => "杜比 E-AC-3",
            "truehd" => "杜比 TrueHD",
            "dts" => "DTS",
            "amr_nb" => "AMR-NB",
            "amr_wb" => "AMR-WB",
            "wmav2" => "WMA",
            "wavpack" => "WavPack",
            "ape" => "APE",
            other => other,
        }
    }

    /// 内置 ffmpeg 没有解码器的有损空间音频：杜比全景声 AC-4、菁彩声 Audio Vivid。
    pub fn needs_external_decoder(&self) -> bool {
        matches!(self.codec.as_str(), "ac4" | "av3a")
    }

    /// 不管能不能解，先认成空间音频：Atmos（AC-4 / E-AC-3 JOC / TrueHD）或菁彩声。
    pub fn is_spatial(&self) -> bool {
        matches!(
            self.codec.as_str(),
            "ac4" | "av3a" | "eac3_atmos" | "truehd_atmos"
        )
    }
}

fn parse_u64(s: &Option<String>) -> u64 {
    s.as_deref().and_then(|v| v.parse().ok()).unwrap_or(0)
}

fn parse_f64(s: &Option<String>) -> f64 {
    s.as_deref().and_then(|v| v.parse().ok()).unwrap_or(0.0)
}

/// 把“承载 Atmos 的普通编码”标出来：E-AC-3 JOC / TrueHD Atmos。
/// AC-3 本身装不了 Atmos，只有 profile 明确带 Atmos/JOC 才认。
fn normalize_codec(codec: &str, profile: Option<&str>) -> String {
    let p = profile.unwrap_or("").to_ascii_lowercase();
    let atmos = p.contains("atmos") || p.contains("joc");
    if atmos {
        match codec {
            "eac3" | "ac3" => return "eac3_atmos".to_string(),
            "truehd" => return "truehd_atmos".to_string(),
            _ => {}
        }
    }
    codec.to_string()
}

pub fn probe(tools: &Tools, file: &Path) -> Result<MediaInfo> {
    let mut cmd = Command::new(&tools.ffprobe);
    cmd.args([
        "-v",
        "quiet",
        "-print_format",
        "json",
        "-show_format",
        "-show_streams",
    ])
    .arg(file);
    #[cfg(windows)]
    cmd.creation_flags(CREATE_NO_WINDOW);

    let out = cmd.output().context("启动 ffprobe 失败")?;
    if out.stdout.is_empty() {
        return Err(anyhow!("读不出音频流。可能是 DRM 加密文件，或文件损坏。"));
    }
    let parsed: ProbeOut = serde_json::from_slice(&out.stdout).context("解析 ffprobe 输出失败")?;

    let audio = parsed
        .streams
        .iter()
        .find(|s| s.codec_type.as_deref() == Some("audio"));
    // 内嵌封面也是一条 video 流，不算画面
    let video = parsed.streams.iter().find(|s| {
        s.codec_type.as_deref() == Some("video")
            && s.disposition
                .as_ref()
                .map(|d| d.attached_pic == 0)
                .unwrap_or(true)
    });
    if audio.is_none() && video.is_none() {
        return Err(anyhow!("文件里没有音频流，也没有画面。可能是 DRM 加密文件，或文件损坏。"));
    }

    let fmt = parsed.format.unwrap_or(Format {
        format_name: None,
        duration: None,
        bit_rate: None,
        size: None,
    });

    // 芯片音乐（libgme 解复用）没有固定时长：它是按曲目循环渲染的，
    // ffprobe 的 duration 是 N/A。这里给个 3 分钟兜底，别让"读不出时长"
    // 的加密判定把 .nsf/.spc/.vgm 这类文件误杀。
    let gme = fmt
        .format_name
        .as_deref()
        .map(|n| n.contains("gme"))
        .unwrap_or(false);
    let duration_s = {
        let d = parse_f64(&fmt.duration);
        if d > 0.0 {
            d
        } else {
            let st = audio.or(video);
            st.map(|s| parse_f64(&s.duration)).unwrap_or(0.0)
        }
    };
    let duration_s = if duration_s <= 0.0 && gme {
        crate::CHIPTUNE_SECONDS as f64
    } else {
        duration_s
    };
    // 有流但读不出时长（AMV、裸流等）：按 0 处理，别误判成加密。
    // 真正的加密容器在 identify() 里有魔数判断，ffprobe 也多半读不出流。

    // 位深：优先 ffprobe 的 bits_per_raw_sample（FLAC/ALAC 会给），
    // 其次 bits_per_sample，最后按 PCM 的 codec 名兜底
    let bits = audio.and_then(|st| {
        st.bits_per_raw_sample
            .as_deref()
            .and_then(|s| s.parse::<u32>().ok())
            .filter(|b| *b > 0)
            .or_else(|| st.bits_per_sample.filter(|b| *b > 0))
            .or_else(|| match st.codec_name.as_deref() {
                Some("pcm_s16le") | Some("pcm_s16be") => Some(16),
                Some("pcm_s24le") | Some("pcm_s24be") => Some(24),
                Some("pcm_s32le") | Some("pcm_s32be") => Some(32),
                Some("pcm_f32le") | Some("pcm_f32be") => Some(32),
                _ => None,
            })
    });

    // 全部音轨：视频可能带多条，转换时默认全部保留
    let audio_tracks: Vec<AudioTrackInfo> = parsed
        .streams
        .iter()
        .filter(|s| s.codec_type.as_deref() == Some("audio"))
        .map(|s| {
            let tag = |k: &str| {
                s.tags
                    .as_ref()
                    .and_then(|m| m.get(k).cloned())
                    .filter(|v| !v.trim().is_empty())
            };
            let raw = s
                .codec_name
                .clone()
                .filter(|c| !c.is_empty())
                .or_else(|| match s.codec_tag_string.as_deref() {
                    Some("av3a") => Some("av3a".to_string()),
                    _ => None,
                })
                .unwrap_or_default();
            AudioTrackInfo {
                codec: normalize_codec(&raw, s.profile.as_deref()),
                channels: s.channels.unwrap_or(0),
                bitrate_kbps: parse_u64(&s.bit_rate) / 1000,
                language: tag("language"),
                title: tag("title"),
            }
        })
        .collect();

    // 视频容器的 format.bit_rate 是"画面 + 声音"的总码率。有音轨码率就优先用它；
    // 像 MKV 这种容器不记音轨码率的，宁可不填（0），也别把总码率当成音频码率
    // ——"353kbps 的 AAC"这种假数据比空着更糟。
    let has_video = video.is_some();
    let st_kbps = audio.map(|s| parse_u64(&s.bit_rate) / 1000).unwrap_or(0);
    let bitrate_kbps = if st_kbps > 0 {
        st_kbps
    } else if has_video {
        0
    } else {
        parse_u64(&fmt.bit_rate) / 1000
    };

    // 帧率：avg_frame_rate 更有代表性（可变帧率会写成 r_frame_rate 的上限）；
    // 有些容器给 "0/0"，这时退回 r_frame_rate。
    let fps = video
        .and_then(|v| {
            parse_rate(&v.avg_frame_rate)
                .filter(|f| *f > 0.0)
                .or_else(|| parse_rate(&v.r_frame_rate))
        })
        .unwrap_or(0.0);

    Ok(MediaInfo {
        codec: audio
            .and_then(|s| {
                let raw = s
                    .codec_name
                    .clone()
                    .filter(|c| !c.is_empty())
                    .or_else(|| match s.codec_tag_string.as_deref() {
                        Some("av3a") => Some("av3a".to_string()),
                        _ => None,
                    })?;
                Some(normalize_codec(&raw, s.profile.as_deref()))
            })
            .unwrap_or_default(),
        container: fmt.format_name.clone().unwrap_or_default(),
        sample_rate: audio
            .map(|s| parse_u64(&s.sample_rate) as u32)
            .unwrap_or(0),
        channels: audio.and_then(|s| s.channels).unwrap_or(0),
        bitrate_kbps,
        duration_s,
        size_bytes: parse_u64(&fmt.size),
        bits,
        audio_tracks,
        video_codec: video.and_then(|v| v.codec_name.clone()),
        width: video.and_then(|v| v.width).unwrap_or(0),
        height: video.and_then(|v| v.height).unwrap_or(0),
        fps,
        video_bitrate_kbps: video.map(|v| parse_u64(&v.bit_rate) / 1000).unwrap_or(0),
    })
}

/// ffprobe 的帧率是 "30000/1001" 这种分数，算成小数方便显示和比较。
fn parse_rate(s: &Option<String>) -> Option<f64> {
    let v = s.as_deref()?;
    let (num, den) = match v.split_once('/') {
        Some((a, b)) => (a.parse::<f64>().ok()?, b.parse::<f64>().ok()?),
        None => (v.parse::<f64>().ok()?, 1.0),
    };
    if den <= 0.0 {
        return None;
    }
    Some(num / den)
}

/// 转换进度回调
pub type ProgressFn = Box<dyn FnMut(f32) + Send>;

/// 兼容旧调用：无附加输入，返回 `()`。
///
/// 需要封面等第二输入时用 [`convert_with_inputs`]。
pub fn convert(
    tools: &Tools,
    input: &Path,
    output: &Path,
    args: &[&str],
    total_s: f64,
    on_progress: ProgressFn,
) -> Result<()> {
    convert_with_inputs(tools, input, output, args, &[], total_s, on_progress).map(|_| ())
}

/// 支持附加输入的转换。
///
/// `extra_inputs` 里的文件（例如封面）会作为额外的 `-i`，**所有 `-i` 都在最前面**：
///
/// ```text
/// ffmpeg -i input -i cover ... -map 0:a:0 -map 1:v:0 ... output
/// ```
///
/// `args` 里如果已经带了 `-i cover`（例如 `metadata_args` 的返回值），这里会把它
/// 提到输入区并按路径去重，调用方不用手动拆。成功时返回输出文件字节数。
pub fn convert_with_inputs(
    tools: &Tools,
    input: &Path,
    output: &Path,
    args: &[&str],
    extra_inputs: &[&Path],
    total_s: f64,
    mut on_progress: ProgressFn,
) -> Result<u64> {
    // 原生解码优先：DSF/DFF（以及以后的自由格式）先用我们自己的解码器解成 WAV。
    // 解不出来就原样退回 ffmpeg —— 新解码器有 bug 也不该把用户能转的文件转坏。
    let native_tmp = crate::native::decode_to_temp(input);
    let _native_guard = crate::native::TempGuard(native_tmp.clone());
    let effective_input: &Path = match &native_tmp {
        Some(p) => p.as_path(),
        None => input,
    };
    let (inputs, rest) = prepare_inputs(effective_input, args, extra_inputs);
    // 先写同目录的 .part.<ext>，成功后再改名：失败/中断不会留下半成品，
    // 也不会覆盖掉已存在的目标文件（除非调用方本来就要覆盖）。
    let tmp = part_path(output);
    let _ = std::fs::remove_file(&tmp);

    let mut cmd = Command::new(&tools.ffmpeg);
    cmd.args(["-y", "-hide_banner", "-nostdin"]);
    for p in &inputs {
        cmd.arg("-i").arg(p);
    }
    // 如果 args 里已经有 -map，就完全按调用方的映射来（视频容器/纯视频裸流都是这样）；
    // 音频格式没有 -map，才补一条「第一条音轨」。
    if !rest.iter().any(|a| a == "-map") {
        cmd.args(["-map", "0:a:0"]);
    }
    cmd.args(&rest)
        .arg(&tmp)
        .args(["-progress", "pipe:1", "-nostats"])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(windows)]
    cmd.creation_flags(CREATE_NO_WINDOW);

    let mut child = cmd.spawn().context("启动 ffmpeg 失败")?;

    // stderr 必须单独抽干，否则管道满了会死锁
    let mut stderr = child.stderr.take().unwrap();
    let err_thread = std::thread::spawn(move || {
        let mut buf = String::new();
        let mut reader = BufReader::new(&mut stderr);
        let mut line = String::new();
        while matches!(reader.read_line(&mut line), Ok(n) if n > 0) {
            buf.push_str(&line);
            line.clear();
        }
        buf
    });

    if let Some(stdout) = child.stdout.take() {
        let reader = BufReader::new(stdout);
        for line in reader.lines().map_while(Result::ok) {
            // ffmpeg 的 out_time_ms 历史原因其实是微秒
            if let Some(v) = line.strip_prefix("out_time_us=") {
                if let (Ok(us), true) = (v.trim().parse::<f64>(), total_s > 0.0) {
                    on_progress((us / 1_000_000.0 / total_s).clamp(0.0, 0.99) as f32);
                }
            }
        }
    }

    let status = child.wait().context("等待 ffmpeg 结束失败")?;
    let err = err_thread.join().unwrap_or_default();
    if !status.success() {
        let _ = std::fs::remove_file(&tmp);
        let tail: String = err.lines().rev().take(4).collect::<Vec<_>>().join("\n");
        return Err(anyhow!("ffmpeg 转换失败：\n{tail}"));
    }
    // 覆盖模式：Windows 上 rename 不会自动覆盖已有文件
    if output.exists() {
        std::fs::remove_file(output).with_context(|| format!("无法替换 {}", output.display()))?;
    }
    std::fs::rename(&tmp, output)
        .with_context(|| format!("无法把临时文件改名为 {}", output.display()))?;
    on_progress(1.0);
    Ok(std::fs::metadata(output).map(|m| m.len()).unwrap_or(0))
}

/// `song.flac` → `song.part.flac`（同目录，保留扩展名让 ffmpeg 仍能选对 muxer）。
pub fn part_path(output: &Path) -> PathBuf {
    let stem = output
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "output".to_string());
    let ext = output
        .extension()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default();
    let name = if ext.is_empty() {
        format!("{stem}.part")
    } else {
        format!("{stem}.part.{ext}")
    };
    output.with_file_name(name)
}

/// 把 input、args 里的 `-i`、extra_inputs 合成输入列表，并返回去掉 `-i` 的其余参数。
fn prepare_inputs(
    input: &Path,
    args: &[&str],
    extra_inputs: &[&Path],
) -> (Vec<PathBuf>, Vec<String>) {
    let mut inputs = vec![input.to_path_buf()];
    let mut rest = Vec::new();
    let mut i = 0;

    while i < args.len() {
        if args[i] == "-i" && i + 1 < args.len() {
            let p = PathBuf::from(args[i + 1]);
            if !inputs.contains(&p) {
                inputs.push(p);
            }
            i += 2;
        } else {
            rest.push(args[i].to_string());
            i += 1;
        }
    }

    for p in extra_inputs {
        let p = p.to_path_buf();
        if !inputs.contains(&p) {
            inputs.push(p);
        }
    }
    (inputs, rest)
}

fn has_audio_map(args: &[String]) -> bool {
    args.windows(2)
        .any(|w| w[0] == "-map" && (w[1] == "0:a" || w[1].starts_with("0:a:")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hoists_input_embedded_in_args_before_maps() {
        let args = [
            "-i",
            "cover.jpg",
            "-map",
            "0:a:0",
            "-map",
            "1:v:0",
            "-c:v",
            "copy",
        ];
        let (inputs, rest) = prepare_inputs(Path::new("in.mp3"), &args, &[]);

        assert_eq!(
            inputs,
            vec![PathBuf::from("in.mp3"), PathBuf::from("cover.jpg")]
        );
        let rest: Vec<&str> = rest.iter().map(String::as_str).collect();
        assert_eq!(rest, vec!["-map", "0:a:0", "-map", "1:v:0", "-c:v", "copy"]);
    }

    #[test]
    fn extra_inputs_are_added_once_and_keep_order() {
        let extra = [Path::new("cover.jpg")];
        let args = ["-i", "cover.jpg", "-map", "0:a:0", "-map", "1:v:0"];
        let (inputs, rest) = prepare_inputs(Path::new("in.mp3"), &args, &extra);

        assert_eq!(
            inputs,
            vec![PathBuf::from("in.mp3"), PathBuf::from("cover.jpg")]
        );
        assert!(has_audio_map(&rest));
    }
}

/// 时长是否在容差内：超过 max(2 秒, 8%) 就算不对。
///
/// 有些容器（AMV、裸流）probe 出来 duration 是 N/A，这种情况下跳过时长检查，
/// 只要求能回读、能解出流。
fn duration_ok(expected_s: f64, actual_s: f64) -> bool {
    if actual_s <= 0.5 || expected_s <= 1.0 {
        return true;
    }
    (actual_s - expected_s).abs() <= (expected_s * 0.08).max(2.0)
}

/// 转换后自检：重新 probe + **真解码前 10 秒**，确认不是 0 字节/坏文件/时长离谱。
///
/// 只解前 10 秒是有意的：完整解码长视频会让批量转换慢一倍，而坏文件
/// 通常在头部或前几帧就暴露了。无损逐样本校验在 `verify_lossless` 里单独做。
pub fn verify_output(tools: &Tools, path: &Path, expected_s: f64) -> Result<(), String> {
    let meta = std::fs::metadata(path).map_err(|why| format!("输出文件不存在：{why}"))?;
    if meta.len() == 0 {
        return Err("输出文件是 0 字节".to_string());
    }
    // 裸 PCM 没有头部，probe 认不出来；只能确认非空，解码参数由用户约定
    if path
        .extension()
        .map(|e| e.to_string_lossy().eq_ignore_ascii_case("pcm"))
        .unwrap_or(false)
    {
        return Ok(());
    }
    let info = probe(tools, path).map_err(|e| format!("输出回读失败：{e:#}"))?;
    if !duration_ok(expected_s, info.duration_s) {
        return Err(format!(
            "时长不符：输出 {:.2}s，输入 {:.2}s",
            info.duration_s, expected_s
        ));
    }

    let mut cmd = Command::new(&tools.ffmpeg);
    cmd.args(["-v", "error", "-nostdin", "-i"])
        .arg(path)
        .args(["-map", "0", "-t", "10", "-f", "null", "-"])
        .stdout(Stdio::null())
        .stderr(Stdio::piped());
    #[cfg(windows)]
    cmd.creation_flags(CREATE_NO_WINDOW);
    let out = cmd
        .output()
        .map_err(|e| format!("启动 ffmpeg 解码自检失败：{e}"))?;
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr);
        let why = err.lines().rev().find(|l| !l.trim().is_empty()).unwrap_or("解码失败");
        return Err(format!("输出无法解码：{why}"));
    }
    Ok(())
}

#[cfg(test)]
mod verify_tests {
    use super::duration_ok;

    #[test]
    fn duration_accepts_close_values() {
        assert!(duration_ok(180.0, 180.5));
        assert!(duration_ok(3.0, 3.0));
    }

    #[test]
    fn duration_rejects_wild_values() {
        assert!(!duration_ok(180.0, 240.0));
        assert!(!duration_ok(60.0, 10.0));
    }

    #[test]
    fn duration_skips_unknown_output() {
        // AMV / 裸流 probe 出来是 N/A（0），不该因此判失败
        assert!(duration_ok(120.0, 0.0));
        assert!(duration_ok(0.5, 999.0));
    }
}
/// 把第一条音轨解码成 s32le 后做哈希；用于无损转换的逐样本校验。
pub fn pcm_hash(tools: &Tools, path: &Path) -> Result<u64, String> {
    let mut cmd = Command::new(&tools.ffmpeg);
    cmd.args(["-v", "error", "-nostdin", "-i"])
        .arg(path)
        .args(["-map", "0:a:0", "-f", "s32le", "-acodec", "pcm_s32le", "-"])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(windows)]
    cmd.creation_flags(CREATE_NO_WINDOW);
    let mut child = cmd.spawn().map_err(|e| format!("启动解码校验失败：{e}"))?;
    let mut out = child.stdout.take().ok_or("拿不到解码输出")?;
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    let mut buf = vec![0u8; 64 * 1024];
    use std::hash::Hasher;
    loop {
        let n = std::io::Read::read(&mut out, &mut buf).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        hasher.write(&buf[..n]);
    }
    let status = child.wait().map_err(|e| e.to_string())?;
    if !status.success() {
        return Err("解码校验失败".to_string());
    }
    Ok(hasher.finish())
}

/// 无损转换校验：采样率/声道一致时，比对解码后的 PCM 哈希。
pub fn verify_lossless(tools: &Tools, source: &Path, output: &Path) -> Result<(), String> {
    let a = probe(tools, source).map_err(|e| format!("源文件回读失败：{e:#}"))?;
    let b = probe(tools, output).map_err(|e| format!("输出回读失败：{e:#}"))?;
    if a.sample_rate != b.sample_rate || a.channels != b.channels {
        return Err(format!(
            "采样率/声道不同，无法逐样本比较（源 {} Hz/{}ch，输出 {} Hz/{}ch）",
            a.sample_rate, a.channels, b.sample_rate, b.channels
        ));
    }
    let ha = pcm_hash(tools, source)?;
    let hb = pcm_hash(tools, output)?;
    if ha != hb {
        return Err("PCM 校验和不一致（无损转换被改动）".to_string());
    }
    Ok(())
}