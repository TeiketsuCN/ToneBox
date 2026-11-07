//! 回归测试：锁定几个与 Python 参考实现对齐的实测值。
//!
//! 这些数值来自 tools/analyze.py 在本机样本上的输出（docs/样本清单.md 同批数据）。
//! 只做小范围检查，完整命中率见 tests/calibration.rs。

use std::env;
use std::path::{Path, PathBuf};

use fc_analyze::analyze;
use fc_core::{locate, probe};

struct Expect {
    file: &'static str,
    cutoff: f64,
    slope: f64,
    estimate: &'static str,
}

const CASES: &[Expect] = &[
    Expect {
        file: r"mp3\pop48k_mp3_64k.mp3",
        cutoff: 11496.0,
        slope: -104.2,
        estimate: "64",
    },
    Expect {
        file: r"mp3\pop48k_mp3_128k.mp3",
        cutoff: 16758.0,
        slope: -105.5,
        estimate: "128/192/320",
    },
    Expect {
        file: r"aac\beatles69_aac_256k.m4a",
        cutoff: 22050.0,
        slope: -19.8,
        estimate: "256",
    },
    Expect {
        file: r"atrac\oma_at3p_01.oma",
        cutoff: 19466.0,
        slope: -91.8,
        estimate: "256",
    },
    Expect {
        file: r"atrac\sample_atrac3_66k.wav",
        cutoff: 15364.0,
        slope: -74.2,
        estimate: "66~105",
    },
    Expect {
        file: r"opus\pop48k_opus_64k.opus",
        cutoff: 20262.0,
        slope: -74.3,
        estimate: "不可判定",
    },
    Expect {
        file: r"lossless\pop48k_ref.flac",
        cutoff: 16805.0,
        slope: -61.9,
        estimate: "无损/CD",
    },
];

fn samples_dir() -> Option<PathBuf> {
    for p in [
        env::var_os("FC_SAMPLES_DIR").map(PathBuf::from),
        Some(PathBuf::from(r"D:\FormatConverter\samples")),
        Some(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../samples")),
    ]
    .into_iter()
    .flatten()
    {
        if p.join("manifest_synthetic.json").is_file() {
            return Some(p);
        }
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

#[test]
fn reference_values_match_python() {
    let Some(root) = samples_dir() else {
        eprintln!("跳过回归测试：找不到 samples/");
        return;
    };
    ensure_ffmpeg_on_path();
    let tools = locate().expect("找不到 ffmpeg/ffprobe，无法跑回归测试");

    for case in CASES {
        let path = root.join(case.file);
        assert!(path.is_file(), "样本不存在: {}", path.display());
        let info = probe(&tools, &path).expect("probe 失败");
        let report = analyze(&path, &info).expect("analyze 失败");

        assert!(
            (report.cutoff_hz - case.cutoff).abs() <= 2.0,
            "{} cutoff 偏差: 期望 {}，实际 {}",
            case.file,
            case.cutoff,
            report.cutoff_hz
        );
        assert!(
            (report.slope_db_per_khz - case.slope).abs() <= 0.2,
            "{} slope 偏差: 期望 {}，实际 {}",
            case.file,
            case.slope,
            report.slope_db_per_khz
        );
        assert!(
            report.estimate.contains(case.estimate),
            "{} estimate 偏差: 期望包含 {}，实际 {}",
            case.file,
            case.estimate,
            report.estimate
        );
    }
}
