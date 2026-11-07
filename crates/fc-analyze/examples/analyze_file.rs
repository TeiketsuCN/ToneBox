//! 手动性能测试入口：
//! `cargo run --release -p fc-analyze --example analyze_file -- <file>`

use std::path::PathBuf;
use std::time::Instant;

use fc_analyze::analyze;
use fc_core::{locate, probe};

fn main() -> anyhow::Result<()> {
    let path = std::env::args_os()
        .nth(1)
        .map(PathBuf::from)
        .ok_or_else(|| anyhow::anyhow!("用法: analyze_file <file>"))?;
    let tools = locate().ok_or_else(|| anyhow::anyhow!("找不到 ffmpeg/ffprobe"))?;

    let t = Instant::now();
    let info = probe(&tools, &path)?;
    let probe_s = t.elapsed().as_secs_f64();

    let t = Instant::now();
    let report = analyze(&path, &info)?;
    let analyze_s = t.elapsed().as_secs_f64();

    println!(
        "file={} duration={:.1}s cutoff={:.0}Hz estimate={} confidence={:.2} probe={:.3}s analyze={:.3}s",
        path.display(),
        info.duration_s,
        report.cutoff_hz,
        report.estimate,
        report.confidence,
        probe_s,
        analyze_s
    );
    Ok(())
}
