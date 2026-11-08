//! 元数据写入 —— 只生成 ffmpeg 参数，不自己起进程。
//!
//! 典型用法（注意所有输入必须放在 `-map` 之前）：
//!
//! ```text
//! ffmpeg -i input.mp3 -i cover.jpg \
//!   -map 0:a:0 -map 1:v:0 -c:v copy -disposition:v:0 attached_pic \
//!   -c:a flac -metadata title=... output.flac
//! ```
//!
//! **不要**在已有的 `-map 0:a:0` 后面直接拼本函数的返回值：有封面时它会再插入
//! 一个 `-i`，ffmpeg 会把前面的 `-map` 当成封面输入选项而报错。调用方需要把
//! 输入、映射、编码参数按上面的顺序重排，或者在没有封面的情况下才直接追加
//! 纯 `-metadata` 参数。
//!
//! 注意：WAV / AIFF 等容器不支持 `attached_pic`，调用方应按目标格式决定是否传
//! `cover`；FLAC / MP3 / M4A 实测可用。

use std::path::Path;

use crate::tags::Tags;

/// 这次转换要写入的元数据。
pub struct MetaWrite<'a> {
    /// 已有标签；None / 空串 / 纯空白都不会产出参数。
    pub tags: &'a Tags,
    /// 临时封面文件路径，由 GUI 写出来；None 表示不写封面。
    pub cover: Option<&'a Path>,
}

/// 只产出 ffmpeg 参数片段，不启动进程。
///
/// 返回顺序：封面输入/映射/编码参数，然后 title / artist / album / track / date。
/// 空标签会被跳过——`-metadata title=` 会把标签写成空值，比不写更糟。
pub fn metadata_args(m: &MetaWrite<'_>) -> Vec<String> {
    let mut args = Vec::new();

    if let Some(cover) = m.cover.filter(|p| !p.as_os_str().is_empty()) {
        args.push("-i".to_string());
        args.push(cover.to_string_lossy().into_owned());
        args.push("-map".to_string());
        args.push("0:a:0".to_string());
        args.push("-map".to_string());
        args.push("1:v:0".to_string());
        args.push("-c:v".to_string());
        args.push("copy".to_string());
        args.push("-disposition:v:0".to_string());
        args.push("attached_pic".to_string());
    }

    push_tag(&mut args, "title", &m.tags.title);
    push_tag(&mut args, "artist", &m.tags.artist);
    push_tag(&mut args, "album", &m.tags.album);
    push_tag(&mut args, "track", &m.tags.track);
    push_tag(&mut args, "date", &m.tags.date);

    args
}

fn push_tag(args: &mut Vec<String>, key: &str, value: &Option<String>) {
    let Some(value) = value else {
        return;
    };
    let value = value.trim();
    if value.is_empty() {
        return;
    }
    args.push("-metadata".to_string());
    args.push(format!("{key}={value}"));
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn tags(title: Option<&str>, artist: Option<&str>) -> Tags {
        Tags {
            title: title.map(str::to_string),
            artist: artist.map(str::to_string),
            ..Default::default()
        }
    }

    #[test]
    fn empty_tags_emit_nothing() {
        let tags = Tags::default();
        let args = metadata_args(&MetaWrite {
            tags: &tags,
            cover: None,
        });
        assert!(args.is_empty(), "空标签不该产出参数: {args:?}");
    }

    #[test]
    fn blank_tags_are_skipped() {
        let tags = tags(Some("   "), Some(""));
        let args = metadata_args(&MetaWrite {
            tags: &tags,
            cover: None,
        });
        assert!(args.is_empty(), "空白标签不该产出参数: {args:?}");
    }

    #[test]
    fn tags_are_emitted_in_order() {
        let tags = Tags {
            title: Some("歌名".into()),
            artist: Some("艺术家".into()),
            album: Some("专辑".into()),
            genre: Some("风格".into()),
            track: Some("3".into()),
            date: Some("2026".into()),
            ..Default::default()
        };
        let args = metadata_args(&MetaWrite {
            tags: &tags,
            cover: None,
        });
        assert_eq!(
            args,
            vec![
                "-metadata",
                "title=歌名",
                "-metadata",
                "artist=艺术家",
                "-metadata",
                "album=专辑",
                "-metadata",
                "track=3",
                "-metadata",
                "date=2026",
            ]
        );
    }

    #[test]
    fn cover_adds_second_input_and_attached_pic() {
        let tags = tags(Some("歌名"), None);
        let cover = PathBuf::from(r"D:\temp\cover.jpg");
        let args = metadata_args(&MetaWrite {
            tags: &tags,
            cover: Some(&cover),
        });

        assert_eq!(args[0], "-i");
        assert_eq!(args[1].as_str(), cover.to_string_lossy());
        assert!(args.windows(2).any(|w| w[0] == "-map" && w[1] == "0:a:0"));
        assert!(args.windows(2).any(|w| w[0] == "-map" && w[1] == "1:v:0"));
        assert!(args.windows(2).any(|w| w[0] == "-c:v" && w[1] == "copy"));
        assert!(args
            .windows(2)
            .any(|w| w[0] == "-disposition:v:0" && w[1] == "attached_pic"));
        assert!(args
            .windows(2)
            .any(|w| w[0] == "-metadata" && w[1] == "title=歌名"));
    }
}
