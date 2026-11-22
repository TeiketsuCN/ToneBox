//! 真实编码矩阵的集成测试（默认 ignored）。
//!
//! 运行前先生成合成资产：
//!   powershell -ExecutionPolicy Bypass -File tools\make-test-assets.ps1
//! 再跑：
//!   cargo test -p fc-core --test convert_matrix -- --ignored --nocapture
//!
//! 这些测试真的调用 ffmpeg、真的写文件；不在 CI 默认跑，避免每次提交都拖几分钟。

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
static SEQ: AtomicUsize = AtomicUsize::new(0);

use fc_core::{by_id, convert, probe, verify_output, Tools};

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .expect("repo root")
        .to_path_buf()
}

fn tools() -> Option<Tools> {
    if let (Ok(ffmpeg), Ok(ffprobe)) = (
        std::env::var("TONELORE_FFMPEG"),
        std::env::var("TONELORE_FFPROBE"),
    ) {
        return Some(Tools {
            ffmpeg: PathBuf::from(ffmpeg),
            ffprobe: PathBuf::from(ffprobe),
        });
    }
    let dir = repo_root().join("target").join("release").join("ffmpeg");
    let ffmpeg = dir.join("ffmpeg.exe");
    let ffprobe = dir.join("ffprobe.exe");
    (ffmpeg.is_file() && ffprobe.is_file()).then_some(Tools { ffmpeg, ffprobe })
}

fn assets() -> PathBuf {
    repo_root().join("test-assets")
}

fn out_dir() -> PathBuf {
    let d = repo_root().join("target").join("test-out");
    std::fs::create_dir_all(&d).ok();
    d
}

fn run_case(tools: &Tools, input: &Path, format_id: &str) -> (PathBuf, String) {
    let spec = by_id(format_id).unwrap_or_else(|| panic!("unknown format {format_id}"));
    let src = probe(tools, input).expect("probe input");
    let seq = SEQ.fetch_add(1, Ordering::Relaxed);
    let out = out_dir().join(format!(
        "{}-{}-{}.{}",
        input.file_stem().unwrap().to_string_lossy(),
        format_id,
        seq,
        spec.ext
    ));
    let _ = std::fs::remove_file(&out);
    convert(tools, input, &out, spec.args, src.duration_s, Box::new(|_| {}))
        .unwrap_or_else(|e| panic!("convert {input:?} -> {format_id} failed: {e:#}"));
    verify_output(tools, &out, src.duration_s)
        .unwrap_or_else(|e| panic!("verify {format_id} failed: {e}"));
    let info = probe(tools, &out).expect("probe output");
    let size = std::fs::metadata(&out).map(|m| m.len()).unwrap_or(0);
    println!(
        "OK {:>6} -> {:>6}  {:>8} bytes  {:>6} Hz  {} ch  {:.2}s",
        input.extension().unwrap_or_default().to_string_lossy(),
        format_id,
        size,
        info.sample_rate,
        info.channels,
        info.duration_s
    );
    (out, info.codec)
}

#[test]
#[ignore]
fn convert_matrix() {
    let Some(tools) = tools() else {
        eprintln!("skip: ffmpeg not found (target/release/ffmpeg or TONELORE_FFMPEG)");
        return;
    };
    let a = assets();
    if !a.join("sine16-stereo.wav").is_file() {
        eprintln!("skip: run tools\\make-test-assets.ps1 first");
        return;
    }

    // 有损 -> 无损、无损 -> 有损、无损 -> 无损、24bit、噪声、语音
    let cases: &[(&str, &str)] = &[
        ("sine16-stereo.wav", "flac"),
        ("sine16-stereo.wav", "mp3"),
        ("sine16-stereo.wav", "opus"),
        ("sine16-stereo.wav", "aac"),
        ("sine24-stereo.wav", "flac"),
        ("sine32f-stereo.wav", "flac"),
        ("sine16-stereo.wav", "alac"),
        ("noise16-stereo.wav", "flac"),
        ("silence16-stereo.wav", "mp3"),
        ("sine16-stereo.wav", "wavpack_lossy"),
    ];

    let mut ok = 0;
    for (file, fmt) in cases {
        let input = a.join(file);
        if !input.is_file() {
            println!("SKIP missing asset {file}");
            continue;
        }
        let (_, codec) = run_case(&tools, &input, fmt);
        assert!(!codec.is_empty(), "{fmt}: empty codec");
        ok += 1;
    }
    println!("matrix: {ok}/{} cases passed", cases.len());
}

#[test]
#[ignore]
fn lossy_to_lossless_keeps_duration() {
    let Some(tools) = tools() else { return };
    let input = assets().join("sine16-stereo.wav");
    if !input.is_file() {
        return;
    }
    // 先做一份 mp3，再从 mp3 转 flac：验证“有损转无损”路径也能通过自检
    let (mp3, _) = run_case(&tools, &input, "mp3");
    let (flac, _) = run_case(&tools, &mp3, "flac");
    let info = probe(&tools, &flac).expect("probe flac");
    assert!((info.duration_s - 2.0).abs() < 0.5, "duration {:.2}", info.duration_s);
}
#[test]
#[ignore]
fn lossless_hash_verifies_and_detects_lossy() {
    let Some(tools) = tools() else { return };
    let input = assets().join("sine24-stereo.wav");
    if !input.is_file() {
        return;
    }
    let (flac, _) = run_case(&tools, &input, "flac");
    fc_core::verify_lossless(&tools, &input, &flac)
        .expect("FLAC should decode to the same PCM as the 24-bit WAV");

    let (mp3, _) = run_case(&tools, &input, "mp3");
    assert!(
        fc_core::verify_lossless(&tools, &input, &mp3).is_err(),
        "lossy MP3 must not match the source PCM"
    );
}