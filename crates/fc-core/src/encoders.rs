//! 运行时探测 ffmpeg 到底带哪些编码器。
//!
//! 为什么需要：开发机装的是 gyan 全量版（有 `libspeex`），随包的是 LGPL 共享版
//! （**没有** `libspeex`）。按开发机验证过的功能，装到用户机器上可能直接失败。
//! 与其相信"应该没问题"，不如开机问一次二进制自己。

use std::collections::HashSet;
use std::path::Path;
use std::process::{Command, Stdio};

#[cfg(windows)]
use std::os::windows::process::CommandExt;

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// 问 ffmpeg 要编码器清单。失败返回空集合（界面会把所有编码格式标成不可用，
/// 但至少不会假装能用）。
pub fn list_encoders(ffmpeg: &Path) -> HashSet<String> {
    let mut cmd = Command::new(ffmpeg);
    cmd.args(["-hide_banner", "-encoders"])
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    #[cfg(windows)]
    cmd.creation_flags(CREATE_NO_WINDOW);

    let Ok(out) = cmd.output() else {
        return HashSet::new();
    };
    parse(&String::from_utf8_lossy(&out.stdout))
}

/// 解析 `ffmpeg -encoders` 的输出。
///
/// 格式是六列标志 + 名字 + 描述：
/// ```text
///  A....D aac                  AAC (Advanced Audio Coding)
///  A....D libmp3lame           libmp3lame MP3
/// ```
fn parse(text: &str) -> HashSet<String> {
    let mut set = HashSet::new();
    for line in text.lines() {
        let mut it = line.split_whitespace();
        let Some(flags) = it.next() else { continue };
        // 标志列是形如 A....D 的六字符；描述行、版本行都不会满足
        if flags.len() != 6 || !flags.starts_with('A') && !flags.starts_with('V') && !flags.starts_with('S') {
            continue;
        }
        if !flags.chars().all(|c| c == '.' || c.is_ascii_alphabetic()) {
            continue;
        }
        if let Some(name) = it.next() {
            set.insert(name.to_string());
        }
    }
    set
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_encoder_lines() {
        let text = "\
Encoders:
 A....D aac                  AAC (Advanced Audio Coding)
 A....D libmp3lame           libmp3lame MP3
 V....D libx264              libx264 H.264
 =======
 ";
        let s = parse(text);
        assert!(s.contains("aac"));
        assert!(s.contains("libmp3lame"));
        assert!(s.contains("libx264"));
        assert!(!s.contains("Encoders:"));
        assert!(!s.contains("======="));
    }
}
