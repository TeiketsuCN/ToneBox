//! 端到端示例：输入 MP3 + 封面图，走 `convert_with_inputs` 转 FLAC。
//!
//! `cargo run -p fc-core --example convert_cover -- <in.mp3> <cover.jpg> <out.flac>`

use std::path::{Path, PathBuf};

use fc_core::ffmpeg::convert_with_inputs;
use fc_core::metadata::{metadata_args, MetaWrite};
use fc_core::{locate, Tags};

fn main() -> anyhow::Result<()> {
    let mut argv = std::env::args_os().skip(1);
    let input = PathBuf::from(
        argv.next()
            .ok_or_else(|| anyhow::anyhow!("用法: convert_cover <in> <cover> <out>"))?,
    );
    let cover = PathBuf::from(
        argv.next()
            .ok_or_else(|| anyhow::anyhow!("用法: convert_cover <in> <cover> <out>"))?,
    );
    let output = PathBuf::from(
        argv.next()
            .ok_or_else(|| anyhow::anyhow!("用法: convert_cover <in> <cover> <out>"))?,
    );

    let tags = Tags {
        title: Some("封面测试".into()),
        artist: Some("机器 B".into()),
        album: Some("convert example".into()),
        track: Some("3".into()),
        date: Some("2026".into()),
    };
    let meta = metadata_args(&MetaWrite {
        tags: &tags,
        cover: Some(&cover),
    });

    let mut args: Vec<String> = vec!["-c:a".into(), "flac".into()];
    args.extend(meta);
    let arg_refs: Vec<&str> = args.iter().map(String::as_str).collect();
    let cover_path: &Path = cover.as_path();

    let tools = locate().ok_or_else(|| anyhow::anyhow!("找不到 ffmpeg/ffprobe"))?;
    let size = convert_with_inputs(
        &tools,
        &input,
        &output,
        &arg_refs,
        &[cover_path],
        30.0,
        Box::new(|p| print!("\r{p:>5.1}%")),
    )?;
    println!("\n完成: {} ({size} bytes)", output.display());
    Ok(())
}
