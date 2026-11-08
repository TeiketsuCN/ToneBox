//! 独占输出冒烟测试（没有耳朵也能看：位置是否在走、标签对不对）
//! `cargo run -p fc-player --example exclusive_test -- <file> [秒数]`

use std::time::Duration;

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let Some(input) = args.get(1) else {
        eprintln!("用法: exclusive_test <文件> [秒数]");
        std::process::exit(2);
    };
    let secs: f64 = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(1.5);
    let src_rate: u32 = args.get(3).and_then(|s| s.parse().ok()).unwrap_or(48_000);
    let ffmpeg = std::env::var("FC_FFMPEG")
        .map(|d| std::path::PathBuf::from(d).join("ffmpeg.exe"))
        .unwrap_or_else(|_| std::path::PathBuf::from("ffmpeg.exe"));

    let opts = fc_player::PlayOptions {
        exclusive: true,
        source_rate: src_rate,
        ..Default::default()
    };
    let mut p = fc_player::Player::open_with(
        &ffmpeg,
        std::path::Path::new(input),
        30.0,
        &opts,
    )?;
    println!("output: {}", p.output_label());
    let t0 = std::time::Instant::now();
    let mut last = -1.0;
    while t0.elapsed().as_secs_f64() < secs {
        std::thread::sleep(Duration::from_millis(300));
        let pos = p.position_s();
        println!("pos {pos:.3}s");
        last = pos;
    }
    p.pause();
    println!("ok, last pos {last:.3}s");
    Ok(())
}
