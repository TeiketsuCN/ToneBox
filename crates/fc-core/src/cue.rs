//! CUE 分轨：解析 `.cue`，按 TRACK/INDEX 把整轨切成单曲。
//!
//! 支持常见的单文件 CUE（FILE 一次，多个 TRACK），也支持多 FILE 的 CUE
//! （每个 TRACK 指向不同文件）。INDEX 00 是预间隙，真正起点用 INDEX 01；
//! 只有 INDEX 00 时退回它。

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use anyhow::{anyhow, Context, Result};

use crate::ffmpeg::Tools;
use crate::format::by_id;

#[derive(Debug, Clone)]
pub struct CueTrack {
    pub file: PathBuf,
    pub number: u32,
    pub title: Option<String>,
    pub performer: Option<String>,
    /// 相对所在文件的起点（秒）
    pub start_s: f64,
}

#[derive(Debug, Clone)]
pub struct CueSheet {
    pub title: Option<String>,
    pub performer: Option<String>,
    pub tracks: Vec<CueTrack>,
}

fn parse_time(s: &str) -> Option<f64> {
    let mut parts = s.trim().split(':');
    let mm: f64 = parts.next()?.trim().parse().ok()?;
    let ss: f64 = parts.next()?.trim().parse().ok()?;
    let ff: f64 = parts.next().and_then(|p| p.trim().parse().ok()).unwrap_or(0.0);
    Some(mm * 60.0 + ss + ff / 75.0)
}

fn quoted(s: &str) -> Option<String> {
    let start = s.find('"')?;
    let rest = &s[start + 1..];
    let end = rest.find('"')?;
    Some(rest[..end].to_string())
}

/// 解析 CUE 文本（UTF-8；GBK 会按替换字符处理，标题可能乱码但不影响分轨）。
pub fn parse(cue_path: &Path) -> Result<CueSheet> {
    let bytes = std::fs::read(cue_path).with_context(|| format!("读不了 CUE：{}", cue_path.display()))?;
    let text = String::from_utf8_lossy(&bytes);
    // 记事本/PowerShell 写出来的 CUE 常带 BOM，不去掉第一行就认不出来
    let text = text.trim_start_matches('\u{feff}');
    let dir = cue_path.parent().unwrap_or_else(|| Path::new("."));

    let mut sheet = CueSheet {
        title: None,
        performer: None,
        tracks: Vec::new(),
    };
    let mut current_file: Option<PathBuf> = None;
    let mut current: Option<CueTrack> = None;
    let mut index00: Option<f64> = None;

    for raw in text.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with("REM") {
            continue;
        }
        let upper = line.to_ascii_uppercase();
        if upper.starts_with("FILE ") {
            if let Some(name) = quoted(line) {
                let p = Path::new(&name);
                current_file = Some(if p.is_absolute() {
                    p.to_path_buf()
                } else {
                    dir.join(p)
                });
            }
        } else if upper.starts_with("TRACK ") {
            if let Some(t) = current.take() {
                sheet.tracks.push(t);
            }
            index00 = None;
            let number = line
                .split_whitespace()
                .nth(1)
                .and_then(|n| n.parse().ok())
                .unwrap_or((sheet.tracks.len() + 1) as u32);
            current = Some(CueTrack {
                file: current_file.clone().unwrap_or_default(),
                number,
                title: None,
                performer: None,
                start_s: 0.0,
            });
        } else if upper.starts_with("TITLE ") {
            let value = quoted(line);
            match current.as_mut() {
                Some(t) => t.title = value,
                None => sheet.title = value,
            }
        } else if upper.starts_with("PERFORMER ") {
            let value = quoted(line);
            match current.as_mut() {
                Some(t) => t.performer = value,
                None => sheet.performer = value,
            }
        } else if upper.starts_with("INDEX ") {
            let mut it = line.split_whitespace();
            let _ = it.next();
            let idx = it.next().unwrap_or("");
            let time = it.next().and_then(parse_time);
            match (idx, time) {
                ("01", Some(t)) => {
                    if let Some(cur) = current.as_mut() {
                        cur.start_s = t;
                    }
                }
                ("00", Some(t)) => index00 = Some(t),
                _ => {}
            }
        }
    }
    if let Some(t) = current.take() {
        sheet.tracks.push(t);
    }
    // 只有 INDEX 00 的轨退回 00
    for t in sheet.tracks.iter_mut() {
        if t.start_s == 0.0 {
            if let Some(t0) = index00 {
                t.start_s = t0;
            }
        }
    }
    if sheet.tracks.is_empty() {
        return Err(anyhow!("CUE 里没有 TRACK"));
    }
    Ok(sheet)
}

/// 一轨的结束时间：同一文件里的下一轨起点；最后一轨为 None（到文件末尾）。
pub fn track_end(sheet: &CueSheet, i: usize) -> Option<f64> {
    let cur = &sheet.tracks[i];
    for next in sheet.tracks.iter().skip(i + 1) {
        if next.file == cur.file {
            let end = next.start_s;
            if end > cur.start_s + 0.001 {
                return Some(end);
            }
        }
    }
    None
}

fn safe_name(s: &str) -> String {
    let mut out: String = s
        .chars()
        .map(|c| match c {
            '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*' => '_',
            c => c,
        })
        .collect();
    out = out.trim().trim_end_matches('.').to_string();
    if out.is_empty() {
        "track".to_string()
    } else {
        out
    }
}

/// 按 CUE 分轨。`target` 是输出格式 id（默认 flac），返回生成的文件列表。
pub fn split(
    tools: &Tools,
    cue_path: &Path,
    out_dir: &Path,
    target: &str,
    mut on_progress: impl FnMut(usize, usize),
) -> Result<Vec<PathBuf>> {
    let spec = by_id(target).ok_or_else(|| anyhow!("未知输出格式：{target}"))?;
    let sheet = parse(cue_path)?;
    std::fs::create_dir_all(out_dir)
        .with_context(|| format!("建不了输出目录：{}", out_dir.display()))?;

    let total = sheet.tracks.len();
    let mut made = Vec::new();
    for (i, t) in sheet.tracks.iter().enumerate() {
        if !t.file.is_file() {
            return Err(anyhow!("CUE 指向的音频不存在：{}", t.file.display()));
        }
        let title = t
            .title
            .clone()
            .or_else(|| sheet.title.clone())
            .unwrap_or_else(|| format!("Track {:02}", t.number));
        let name = format!("{:02} - {}.{}", t.number, safe_name(&title), spec.ext);
        let out = out_dir.join(name);

        let mut cmd = Command::new(&tools.ffmpeg);
        cmd.args(["-v", "error", "-nostdin", "-y"]);
        // -ss 放在 -i 前面：走关键帧/快速定位，切轨够用
        cmd.args(["-ss", &format!("{:.3}", t.start_s)]);
        if let Some(end) = track_end(&sheet, i) {
            cmd.args(["-t", &format!("{:.3}", (end - t.start_s).max(0.0))]);
        }
        cmd.arg("-i").arg(&t.file).args(["-map", "0:a:0"]);
        cmd.args(spec.args);
        cmd.args(["-metadata", &format!("track={}", t.number)]);
        cmd.args(["-metadata", &format!("title={title}")]);
        if let Some(p) = t.performer.clone().or_else(|| sheet.performer.clone()) {
            cmd.args(["-metadata", &format!("artist={p}")]);
        }
        if let Some(a) = sheet.title.clone() {
            cmd.args(["-metadata", &format!("album={a}")]);
        }
        cmd.arg(&out)
            .stdout(Stdio::null())
            .stderr(Stdio::piped());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            cmd.creation_flags(0x0800_0000);
        }
        let res = cmd.output().context("启动 ffmpeg 分轨失败")?;
        if !res.status.success() {
            let err = String::from_utf8_lossy(&res.stderr);
            let mut tail: Vec<&str> = err.lines().rev().take(3).collect();
            tail.reverse();
            return Err(anyhow!(
                "第 {} 轨切分失败（退出码 {:?}）：\n{}",
                t.number,
                res.status.code(),
                tail.join("\n")
            ));
        }
        made.push(out);
        on_progress(i + 1, total);
    }
    Ok(made)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_times() {
        assert!((parse_time("01:02:03").unwrap() - 62.04).abs() < 1e-6);
        assert!((parse_time("00:03:00").unwrap() - 3.0).abs() < 1e-6);
    }

    #[test]
    fn parses_simple_sheet() {
        let dir = std::env::temp_dir().join(format!("tb_cue_test_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let cue = dir.join("a.cue");
        std::fs::write(
            &cue,
            "PERFORMER \"Someone\"\nTITLE \"Album\"\nFILE \"a.wav\" WAVE\n  TRACK 01 AUDIO\n    TITLE \"One\"\n    INDEX 01 00:00:00\n  TRACK 02 AUDIO\n    TITLE \"Two\"\n    INDEX 01 00:03:00\n",
        )
        .unwrap();
        let s = parse(&cue).unwrap();
        assert_eq!(s.tracks.len(), 2);
        assert_eq!(s.tracks[0].title.as_deref(), Some("One"));
        assert!((s.tracks[1].start_s - 3.0).abs() < 1e-6);
        assert_eq!(track_end(&s, 0), Some(3.0));
        assert_eq!(track_end(&s, 1), None);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn sanitizes_names() {
        assert_eq!(safe_name("a/b:c"), "a_b_c");
        assert_eq!(safe_name("   "), "track");
    }
}
