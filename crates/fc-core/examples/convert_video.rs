//! 视频转换的端到端小工具（开发/验证用，不进安装包）。
//!
//! 用法：
//! ```text
//! set FC_FFMPEG=D:\Apps\FormatConverter\ffmpeg
//! cargo run -p fc-core --example convert_video -- in.mp4 out.mkv [copy|h264|hevc|vp9|av1]
//! ```
//!
//! 它走的是 GUI 同一条路径：probe → args_for_with → convert → estimate_size，
//! 所以命令行能过，界面上就大概率没问题。

use std::path::{Path, PathBuf};

use fc_core::{ConvertOptions, FormatSpec, Tools, VideoCodec};

fn tools() -> Option<Tools> {
    if let Ok(dir) = std::env::var("FC_FFMPEG") {
        let d = PathBuf::from(dir);
        return Some(Tools {
            ffmpeg: d.join("ffmpeg.exe"),
            ffprobe: d.join("ffprobe.exe"),
        });
    }
    fc_core::locate()
}

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let (Some(input), Some(output)) = (args.get(1), args.get(2)) else {
        eprintln!("用法: convert_video <输入> <输出.mkv|mp4|webm|avi> [copy|h264|hevc|vp9|av1]");
        std::process::exit(2);
    };
    let codec = match args.get(3).map(|s| s.as_str()).unwrap_or("copy") {
        "copy" => VideoCodec::Copy,
        "h264" => VideoCodec::H264,
        "hevc" => VideoCodec::Hevc,
        "vp9" => VideoCodec::Vp9,
        "av1" => VideoCodec::Av1,
        other => anyhow::bail!("未知编码 {other}"),
    };
    let tools = tools().expect("找不到 ffmpeg（可以设 FC_FFMPEG 指向目录）");
    let container = Path::new(output)
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("");
    let spec: &'static FormatSpec =
        fc_core::by_id(container).ok_or_else(|| anyhow::anyhow!("不是视频容器: {container}"))?;

    let info = fc_core::probe(&tools, Path::new(input))?;
    println!(
        "源: 画面 {:?} {}×{} {:.3}fps / 音频 {} {}kbps / {:.1}s",
        info.video_label(),
        info.width,
        info.height,
        info.fps,
        info.codec_label(),
        info.bitrate_kbps,
        info.duration_s
    );

    let mut o = ConvertOptions::default();
    o.video_codec = codec;
    // 可选：FC_HEIGHT=720 / FC_FPS=30 验证缩放和改帧率
    if let Ok(h) = std::env::var("FC_HEIGHT") {
        o.video_height = h.parse().unwrap_or(0);
    }
    if let Ok(f) = std::env::var("FC_FPS") {
        o.video_fps = f.parse().unwrap_or(0);
    }
    let available = fc_core::list_encoders(&tools.ffmpeg);
    if let Some(e) = fc_core::resolve_video_encoder(codec, &available, "") {
        println!("编码器: {e}");
        o.video_encoder = e.to_string();
    }
    let a = fc_core::args_for_with(spec, &o, Some(&info));
    println!("参数: {a:?}");
    println!(
        "预计 {} MB",
        fc_core::estimate_size(&info, spec, &o) as f64 / 1_048_576.0
    );

    let refs: Vec<&str> = a.iter().map(|s| s.as_str()).collect();
    fc_core::convert(
        &tools,
        Path::new(input),
        Path::new(output),
        &refs,
        info.duration_s,
        Box::new(|p| {
            print!("\r{p:.0}%");
            use std::io::Write;
            let _ = std::io::stdout().flush();
        }),
    )?;
    println!();
    println!(
        "完成: {output} · {} MB",
        std::fs::metadata(output)?.len() as f64 / 1_048_576.0
    );
    Ok(())
}
