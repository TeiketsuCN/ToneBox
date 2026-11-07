//! 标签与封面提取。
//!
//! 为什么单独一个文件、而不是并进 `ffmpeg.rs`：
//! `ffmpeg.rs` 归机器 B，这边是给队列做显示用的（标题/艺术家/封面），
//! 职责也不同——它是"读文件里的元信息"，不是"探测音频参数"。
//!
//! 代价是多起一次 ffprobe（约 30ms）。队列里几十个文件也扛得住。

use std::path::Path;
use std::process::{Command, Stdio};

use serde::Deserialize;

#[cfg(windows)]
use std::os::windows::process::CommandExt;

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// 从文件里读出来的显示信息
#[derive(Clone, Debug, Default)]
pub struct Tags {
    pub title: Option<String>,
    pub artist: Option<String>,
    pub album: Option<String>,
    pub track: Option<String>,
    pub date: Option<String>,
}

impl Tags {
    /// 队列里那一行主标题：有标签用标签，没有就退回文件名
    pub fn display_title(&self, fallback: &str) -> String {
        match &self.title {
            Some(t) if !t.trim().is_empty() => t.clone(),
            _ => fallback.to_string(),
        }
    }

    /// 副标题：艺术家 · 专辑
    pub fn display_sub(&self) -> String {
        let mut parts = Vec::new();
        if let Some(a) = &self.artist {
            if !a.trim().is_empty() {
                parts.push(a.trim().to_string());
            }
        }
        if let Some(al) = &self.album {
            if !al.trim().is_empty() {
                parts.push(al.trim().to_string());
            }
        }
        parts.join(" · ")
    }

    pub fn is_empty(&self) -> bool {
        self.title.is_none() && self.artist.is_none() && self.album.is_none()
    }
}

#[derive(Deserialize)]
struct ProbeTags {
    format: Option<Fmt>,
}

#[derive(Deserialize)]
struct Fmt {
    tags: Option<std::collections::HashMap<String, String>>,
}

/// 读标签。失败返回空 Tags，不报错——没有标签是很正常的情况。
pub fn read_tags(ffprobe: &Path, file: &Path) -> Tags {
    let mut cmd = Command::new(ffprobe);
    cmd.args([
        "-v",
        "quiet",
        "-print_format",
        "json",
        "-show_entries",
        "format_tags",
    ])
    .arg(file)
    .stdout(Stdio::piped())
    .stderr(Stdio::null());
    #[cfg(windows)]
    cmd.creation_flags(CREATE_NO_WINDOW);

    let Ok(out) = cmd.output() else {
        return Tags::default();
    };
    let Ok(parsed) = serde_json::from_slice::<ProbeTags>(&out.stdout) else {
        return Tags::default();
    };
    let Some(map) = parsed.format.and_then(|f| f.tags) else {
        return Tags::default();
    };

    // ffmpeg 的 tag key 大小写不统一（mp3 常是全大写，flac 是混合），统一小写查
    let get = |names: &[&str]| -> Option<String> {
        for (k, v) in map.iter() {
            let lk = k.to_ascii_lowercase();
            if names.contains(&lk.as_str()) && !v.trim().is_empty() {
                return Some(v.trim().to_string());
            }
        }
        None
    };

    Tags {
        title: get(&["title"]),
        artist: get(&["artist", "album_artist", "albumartist", "performer"]),
        album: get(&["album"]),
        track: get(&["track", "tracknumber"]),
        date: get(&["date", "year"]),
    }
}

/// 判断有没有内嵌封面（一次 ffprobe）
pub fn has_cover(ffprobe: &Path, file: &Path) -> bool {
    let mut cmd = Command::new(ffprobe);
    cmd.args([
        "-v",
        "quiet",
        "-print_format",
        "json",
        "-show_entries",
        "stream=codec_type:stream_disposition=attached_pic",
        "-select_streams",
        "v",
    ])
    .arg(file)
    .stdout(Stdio::piped())
    .stderr(Stdio::null());
    #[cfg(windows)]
    cmd.creation_flags(CREATE_NO_WINDOW);

    let Ok(out) = cmd.output() else {
        return false;
    };
    let s = String::from_utf8_lossy(&out.stdout);
    // 有 attached_pic=1 才算封面；普通视频轨不算
    s.contains("\"attached_pic\": 1") || s.contains("\"attached_pic\":1")
}

/// 抽出内嵌封面，返回原始字节（JPEG 或 PNG）。没有封面返回 None。
///
/// 不落盘：直接从 ffmpeg 的 stdout 读，省一次写文件和清理。
pub fn extract_cover(ffmpeg: &Path, file: &Path) -> Option<Vec<u8>> {
    let mut cmd = Command::new(ffmpeg);
    cmd.args(["-v", "error", "-i"])
        .arg(file)
        .args([
            "-map",
            "0:v:0",
            "-c",
            "copy",
            "-f",
            "image2pipe",
            "-",
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    #[cfg(windows)]
    cmd.creation_flags(CREATE_NO_WINDOW);

    let out = cmd.output().ok()?;
    if !out.status.success() {
        return None;
    }
    let bytes = out.stdout;
    // 太小的一定不是图（有些文件会输出空或几字节）
    if bytes.len() < 512 {
        return None;
    }
    // 校验一下确实是 JPEG / PNG
    let is_jpeg = bytes.starts_with(&[0xFF, 0xD8, 0xFF]);
    let is_png = bytes.starts_with(&[0x89, b'P', b'N', b'G']);
    if is_jpeg || is_png {
        Some(bytes)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_falls_back_to_filename() {
        let t = Tags::default();
        assert_eq!(t.display_title("a.oma"), "a.oma");
        assert!(t.is_empty());
    }

    #[test]
    fn display_prefers_tag() {
        let t = Tags {
            title: Some("真正的歌名".into()),
            artist: Some("某人".into()),
            album: Some("某专辑".into()),
            ..Default::default()
        };
        assert_eq!(t.display_title("x.mp3"), "真正的歌名");
        assert_eq!(t.display_sub(), "某人 · 某专辑");
    }

    #[test]
    fn blank_tag_is_treated_as_missing() {
        let t = Tags {
            title: Some("   ".into()),
            ..Default::default()
        };
        assert_eq!(t.display_title("x.mp3"), "x.mp3");
    }
}
