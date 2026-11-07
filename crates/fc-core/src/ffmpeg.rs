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
            return Some(Tools { ffmpeg: f, ffprobe: p });
        }
    }
    match (find_in_path("ffmpeg.exe"), find_in_path("ffprobe.exe")) {
        (Some(f), Some(p)) => Some(Tools { ffmpeg: f, ffprobe: p }),
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
}

#[derive(Deserialize)]
struct Format {
    format_name: Option<String>,
    duration: Option<String>,
    bit_rate: Option<String>,
    size: Option<String>,
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
        return Err(anyhow!(
            "读不出音频流。可能是 DRM 加密文件，或文件损坏。"
        ));
    }
    let parsed: ProbeOut = serde_json::from_slice(&out.stdout).context("解析 ffprobe 输出失败")?;

    let st = parsed
        .streams
        .iter()
        .find(|s| s.codec_type.as_deref() == Some("audio"))
        .ok_or_else(|| anyhow!("文件里没有音频流。如果是加密文件（.ncm / .mflac / OpenMG .oma），音频数据被加密，无法解码。"))?;

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
            parse_f64(&st.duration)
        }
    };
    if duration_s <= 0.0 {
        return Err(anyhow!(
            "读不出时长，文件很可能是加密的：容器头可识别，但音频数据是密文。\n\
             常见来源：网易云 .ncm、QQ 音乐 .mflac/.qmc、酷狗 .kgm、Sony OpenMG .oma。\n\
             本工具不提供解密功能。"
        ));
    }

    Ok(MediaInfo {
        codec: st.codec_name.clone().unwrap_or_default(),
        container: fmt.format_name.clone().unwrap_or_default(),
        sample_rate: parse_u64(&st.sample_rate) as u32,
        channels: st.channels.unwrap_or(0),
        bitrate_kbps: parse_u64(&fmt.bit_rate) / 1000,
        duration_s,
        size_bytes: parse_u64(&fmt.size),
    })
}

/// 转换进度回调
pub type ProgressFn = Box<dyn FnMut(f32) + Send>;

pub fn convert(
    tools: &Tools,
    input: &Path,
    output: &Path,
    args: &[&str],
    total_s: f64,
    mut on_progress: ProgressFn,
) -> Result<()> {
    let mut cmd = Command::new(&tools.ffmpeg);
    cmd.args(["-y", "-hide_banner", "-nostdin", "-i"])
        .arg(input)
        .args(["-map", "0:a:0"])
        .args(args)
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
    Ok(())
}
