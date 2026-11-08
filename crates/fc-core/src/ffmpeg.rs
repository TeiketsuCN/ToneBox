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
        let mut s = self.codec.to_uppercase();
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
            "wmav2" => "WMA",
            "wavpack" => "WavPack",
            "ape" => "APE",
            other => other,
        }
    }
}

fn parse_u64(s: &Option<String>) -> u64 {
    s.as_deref().and_then(|v| v.parse().ok()).unwrap_or(0)
}

fn parse_f64(s: &Option<String>) -> f64 {
    s.as_deref().and_then(|v| v.parse().ok()).unwrap_or(0.0)
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

    // 时长是判断"是否加密"最可靠的低成本信号：
    // 加密文件的容器头是密文，ffprobe 只能认出外层格式，读不出时长和码率。
    let duration_s = {
        let d = parse_f64(&fmt.duration);
        if d > 0.0 {
            d
        } else {
            let st = audio.or(video);
            st.map(|s| parse_f64(&s.duration)).unwrap_or(0.0)
        }
    };
    if duration_s <= 0.0 {
        return Err(anyhow!(
            "读不出时长，文件很可能是加密的：容器头可识别，但音频数据是密文。\n\
             常见来源：网易云 .ncm、QQ 音乐 .mflac/.qmc、酷狗 .kgm、Sony OpenMG .oma。\n\
             本工具不提供解密功能。"
        ));
    }

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
            AudioTrackInfo {
                codec: s.codec_name.clone().unwrap_or_default(),
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
            .and_then(|s| s.codec_name.clone())
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
    let (inputs, rest) = prepare_inputs(input, args, extra_inputs);

    let mut cmd = Command::new(&tools.ffmpeg);
    cmd.args(["-y", "-hide_banner", "-nostdin"]);
    for p in &inputs {
        cmd.arg("-i").arg(p);
    }
    // 如果 args 里已经有 -map 0:a，别再补一条，避免输出两条音轨。
    if !has_audio_map(&rest) {
        cmd.args(["-map", "0:a:0"]);
    }
    cmd.args(&rest)
        .arg(output)
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
        let tail: String = err.lines().rev().take(4).collect::<Vec<_>>().join("\n");
        return Err(anyhow!("ffmpeg 转换失败：\n{tail}"));
    }
    on_progress(1.0);
    Ok(std::fs::metadata(output).map(|m| m.len()).unwrap_or(0))
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
