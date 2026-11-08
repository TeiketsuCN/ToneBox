//! 外部编码器：ATRAC 家族（atracdenc）。
//!
//! ## 为什么不随包
//!
//! atracdenc 是**逆向实现**的 ATRAC 编码器（LGPL-2.1+）。ATRAC3+ 的专利期
//! 刚过、ATRAC9 还在保护期。把"分发编码器"这件事留给用户自己做，程序只负责：
//!
//! - 检测用户装没装（[`find`]）
//! - 装了就用（[`convert`]），没装就在界面上明说"你系统里还没这个"
//!
//! 这样我们的分发包里没有一个 ATRAC 编码器，风险结构和"用户自己装 ffmpeg"一致。
//!
//! ## 调用链
//!
//! ```text
//! 任意输入 --ffmpeg--> 44100 Hz / 16bit / 立体声 WAV --atracdenc--> .oma / .aea
//! ```
//!
//! atracdenc 只吃 44.1k/16bit PCM（见它的 man page），所以必须先转一道。
//! 两段都用临时 ASCII 文件名：atracdenc 是 32 位程序，非 ASCII 路径在 Windows
//! 上容易出事（我们自己的样本就全是日文名）。

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use anyhow::{anyhow, Context, Result};

use crate::ffmpeg::{convert_with_inputs, ProgressFn, Tools};

#[cfg(windows)]
use std::os::windows::process::CommandExt;
#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// 外部编码器名。界面上要显示"你还没装 atracdenc"就用它。
pub const NAME: &str = "atracdenc";

/// 找 atracdenc：程序目录 → 程序目录\atracdenc → PATH。
///
/// 顺序和 ffmpeg 故意不同（ffmpeg 是先 PATH）：atracdenc 是"用户自己装"的，
/// 放在程序目录里最不容易和别的版本打架。
pub fn find() -> Option<PathBuf> {
    let exe = if cfg!(windows) {
        "atracdenc.exe"
    } else {
        "atracdenc"
    };
    let mut cands: Vec<PathBuf> = Vec::new();
    if let Ok(cur) = std::env::current_exe() {
        if let Some(dir) = cur.parent() {
            cands.push(dir.join(exe));
            cands.push(dir.join(NAME).join(exe));
        }
    }
    if let Some(path) = std::env::var_os("PATH") {
        for d in std::env::split_paths(&path) {
            cands.push(d.join(exe));
        }
    }
    cands.into_iter().find(|p| p.is_file())
}

/// 这个格式该用 atracdenc 的哪个编码器。
///
/// ATRAC3 的两个档位在 atracdenc 里是两个不同的 `-e` 名字，不是一个码率参数：
/// LP2 = `atrac3`（132k），LP4 = `atrac3_lp4`（66k）。实测 `--bitrate` 只对
/// RealMedia 容器生效，对 .oma 是静默忽略的。
pub fn codec_for(id: &str, atrac3_kbps: u32) -> Option<&'static str> {
    Some(match id {
        "atrac1" => "atrac1",
        "atrac3" => {
            if atrac3_kbps <= 66 {
                "atrac3_lp4"
            } else {
                "atrac3"
            }
        }
        "atrac3p" => "atrac3plus",
        _ => return None,
    })
}

fn temp_tag() -> String {
    static SEQ: AtomicUsize = AtomicUsize::new(0);
    format!(
        "{}_{}",
        std::process::id(),
        SEQ.fetch_add(1, Ordering::Relaxed)
    )
}

/// 用 atracdenc 编码。成功时返回输出文件字节数。
///
/// 进度：解码占 0~0.5，atracdenc 本身不报进度，所以直接跳到 0.55 再等它结束。
pub fn convert(
    tools: &Tools,
    exe: &Path,
    input: &Path,
    output: &Path,
    codec: &str,
    total_s: f64,
    on_progress: ProgressFn,
) -> Result<u64> {
    let ext = output
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("oma")
        .to_ascii_lowercase();
    let tag = temp_tag();
    let tmp_wav = std::env::temp_dir().join(format!("fc_atrac_{tag}.wav"));
    let tmp_out = std::env::temp_dir().join(format!("fc_atrac_{tag}.{ext}"));

    let cb = Arc::new(Mutex::new(on_progress));
    let cb_dec = cb.clone();
    let dec = convert_with_inputs(
        tools,
        input,
        &tmp_wav,
        &["-vn", "-c:a", "pcm_s16le", "-ar", "44100", "-ac", "2"],
        &[],
        total_s,
        Box::new(move |p| {
            if let Ok(mut f) = cb_dec.lock() {
                f(p * 0.5);
            }
        }),
    );
    let dec = match dec {
        Ok(_) => Ok(()),
        Err(e) => Err(e.context("先用 ffmpeg 解码成 WAV 失败")),
    };
    if let Err(e) = dec {
        let _ = std::fs::remove_file(&tmp_wav);
        return Err(e);
    }
    if let Ok(mut f) = cb.lock() {
        f(0.55);
    }

    let mut cmd = Command::new(exe);
    cmd.arg("-e")
        .arg(codec)
        .arg("-i")
        .arg(&tmp_wav)
        .arg("-o")
        .arg(&tmp_out)
        .stdout(Stdio::null())
        .stderr(Stdio::piped());
    #[cfg(windows)]
    cmd.creation_flags(CREATE_NO_WINDOW);

    let out = cmd.output().context(format!(
        "启动 {} 失败（杀毒软件拦了？路径：{}）",
        NAME,
        exe.display()
    ));
    let _ = std::fs::remove_file(&tmp_wav);
    let out = match out {
        Ok(o) => o,
        Err(e) => {
            let _ = std::fs::remove_file(&tmp_out);
            return Err(e);
        }
    };
    if !out.status.success() || !tmp_out.is_file() {
        let _ = std::fs::remove_file(&tmp_out);
        let err = String::from_utf8_lossy(&out.stderr);
        let mut tail: Vec<&str> = err.lines().rev().take(4).collect();
        tail.reverse();
        return Err(anyhow!(
            "{} 编码失败（退出码 {:?}）：\n{}",
            NAME,
            out.status.code(),
            tail.join("\n")
        ));
    }

    if let Ok(mut f) = cb.lock() {
        f(0.98);
    }
    // 临时目录可能和输出不在一个盘，用 copy 不用 rename
    std::fs::copy(&tmp_out, output).context("写输出文件失败（目录权限？）")?;
    let _ = std::fs::remove_file(&tmp_out);
    if let Ok(mut f) = cb.lock() {
        f(1.0);
    }
    Ok(std::fs::metadata(output).map(|m| m.len()).unwrap_or(0))
}
