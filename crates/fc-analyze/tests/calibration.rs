//! 标定测试：用 samples/ 里的已知码率样本跑一遍，打印命中率。
//!
//! 样本不在 git 里。没有 samples/ 时测试会跳过，不阻塞 CI。
//! 运行：
//!   cargo test -p fc-analyze --test calibration -- --nocapture

use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use fc_analyze::analyze;
use fc_core::{locate, probe};
use serde_json::Value;

const ATRAC_TRUTH: &[(&str, u32)] = &[
    ("mc_sich_atrac3_66k.wav", 66),
    ("sample_atrac3_66k.wav", 66),
    ("mc_sich_atrac3_105k.wav", 105),
    ("mc_sich_atrac3_132k.wav", 132),
    ("psp_bgm01.at3", 65),
    ("psp_bgm03.at3", 65),
    ("SND0_atrac3plus_48k.at3", 49),
    ("oma_at3p_01.oma", 256),
    ("oma_at3p_02.oma", 256),
    ("oma_at3p_03.oma", 256),
    ("oma_at3p_04.oma", 256),
    ("oma_at3p_05.oma", 256),
    ("oma_at3p_06.oma", 256),
    ("aa3_0111019293.aa3", 128),
    ("aa3_sonateno14op27-2.aa3", 48),
    ("aea_christmas1970.aea", 292),
];

struct Case {
    path: PathBuf,
    truth_kbps: u32,
    kind: String,
}

fn find_samples() -> Option<PathBuf> {
    if let Ok(p) = env::var("FC_SAMPLES_DIR") {
        let p = PathBuf::from(p);
        if p.join("manifest_synthetic.json").is_file() {
            return Some(p);
        }
    }
    let local = PathBuf::from(r"D:\FormatConverter\samples");
    if local.join("manifest_synthetic.json").is_file() {
        return Some(local);
    }
    let repo = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../samples");
    if repo.join("manifest_synthetic.json").is_file() {
        return Some(repo);
    }
    None
}

fn ensure_ffmpeg_on_path() {
    if locate().is_some() {
        return;
    }
    let candidate = PathBuf::from(r"C:\ffmpeg\bin");
    if candidate.join("ffmpeg.exe").is_file() && candidate.join("ffprobe.exe").is_file() {
        let old = env::var("PATH").unwrap_or_default();
        env::set_var("PATH", format!("{};{}", candidate.display(), old));
    }
}

fn load_cases(root: &Path) -> Vec<Case> {
    let mut cases = Vec::new();
    let text = fs::read_to_string(root.join("manifest_synthetic.json"))
        .expect("读不到 manifest_synthetic.json");
    let manifest: Value = serde_json::from_str(&text).expect("manifest_synthetic.json 格式不对");
    for item in manifest.as_array().expect("manifest 应该是数组") {
        let file = item["file"].as_str().expect("manifest 缺 file");
        let truth = item["truth_kbps"].as_u64().expect("manifest 缺 truth_kbps") as u32;
        let kind = item["kind"].as_str().unwrap_or("unknown");
        cases.push(Case {
            path: root.join(file),
            truth_kbps: truth,
            kind: kind.to_string(),
        });
    }
    for (name, truth) in ATRAC_TRUTH {
        cases.push(Case {
            path: root.join("atrac").join(name),
            truth_kbps: *truth,
            kind: "atrac".to_string(),
        });
    }
    cases.sort_by(|a, b| a.path.cmp(&b.path));
    cases
}

#[test]
fn calibration_hit_rate() {
    let Some(root) = find_samples() else {
        eprintln!("跳过标定测试：找不到 samples/manifest_synthetic.json");
        return;
    };
    ensure_ffmpeg_on_path();
    let tools = locate().expect("找不到 ffmpeg/ffprobe，无法跑标定测试");
    let cases = load_cases(&root);
    assert!(cases.len() >= 58, "标定样本不足：{} 个", cases.len());

    let mut behavior_ok = 0usize;
    let mut bitrate_ok = 0usize;
    let mut non_opus_total = 0usize;
    let mut strict_all_ok = 0usize;
    let mut failures = Vec::new();

    for case in &cases {
        let info = match probe(&tools, &case.path) {
            Ok(v) => v,
            Err(e) => {
                failures.push(format!("{}: probe 失败: {e}", case.path.display()));
                continue;
            }
        };
        let report = match analyze(&case.path, &info) {
            Ok(v) => v,
            Err(e) => {
                failures.push(format!("{}: analyze 失败: {e}", case.path.display()));
                continue;
            }
        };
        let is_opus = info.codec == "opus";
        let lossless_true = case.truth_kbps == 0;
        let est = &report.estimate;

        let behavior_pass = if lossless_true {
            est.contains("无损")
        } else if is_opus {
            est == "不可判定"
        } else {
            est.contains(&case.truth_kbps.to_string())
        };
        let bitrate_pass = if is_opus {
            false
        } else if lossless_true {
            est.contains("无损")
        } else {
            est.contains(&case.truth_kbps.to_string())
        };

        if behavior_pass {
            behavior_ok += 1;
        }
        if bitrate_pass {
            bitrate_ok += 1;
            strict_all_ok += 1;
        }
        if !is_opus {
            non_opus_total += 1;
        }
        if !behavior_pass {
            failures.push(format!(
                "{:<48} truth={:<3} kind={:<8} estimate={:<8} cutoff={:>6.0}Hz slope={:>7.1}dB/kHz conf={:.2}",
                case.path.file_name().unwrap().to_string_lossy(),
                case.truth_kbps,
                case.kind,
                est,
                report.cutoff_hz,
                report.slope_db_per_khz,
                report.confidence
            ));
        }
    }

    let total = cases.len();
    let behavior_rate = behavior_ok as f64 / total as f64;
    let bitrate_rate = bitrate_ok as f64 / non_opus_total.max(1) as f64;
    // 最保守口径：Opus 按码率硬算必然失败，但产品要求它输出“不可判定”。
    let strict_all_rate = strict_all_ok as f64 / total as f64;

    println!("\n=== 频谱标定 ===");
    println!("样本总数: {total}（非 Opus: {non_opus_total}）");
    println!(
        "产品口径（Opus 输出“不可判定”视为正确）: {behavior_ok}/{total} = {:.1}%",
        behavior_rate * 100.0
    );
    println!(
        "非 Opus 码率命中: {bitrate_ok}/{non_opus_total} = {:.1}%",
        bitrate_rate * 100.0
    );
    println!(
        "最保守口径（Opus 按码率硬算，必然失败）: {strict_all_ok}/{total} = {:.1}%",
        strict_all_rate * 100.0
    );
    if !failures.is_empty() {
        println!("\n失败样本:");
        for f in &failures {
            println!("  {f}");
        }
    }

    assert!(
        behavior_rate >= 0.85,
        "产品口径命中率 {:.1}% 低于 85%",
        behavior_rate * 100.0
    );
    assert!(
        bitrate_rate >= 0.85,
        "非 Opus 码率命中率 {:.1}% 低于 85%",
        bitrate_rate * 100.0
    );
    assert!(
        strict_all_rate >= 0.85,
        "最保守口径命中率 {:.1}% 低于 85%",
        strict_all_rate * 100.0
    );
}
