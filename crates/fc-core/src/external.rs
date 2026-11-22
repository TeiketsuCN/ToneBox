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

/// 找随包的 Speex 编码器（vendor\speex\speexenc.exe）。
pub fn find_speexenc() -> Option<std::path::PathBuf> {
    let exe = if cfg!(windows) { "speexenc.exe" } else { "speexenc" };
    let mut cands: Vec<std::path::PathBuf> = Vec::new();
    if let Ok(cur) = std::env::current_exe() {
        if let Some(dir) = cur.parent() {
            cands.push(dir.join("speex").join(exe));
            cands.push(dir.join(exe));
        }
    }
    if let Ok(cwd) = std::env::current_dir() {
        cands.push(cwd.join("vendor").join("speex").join(exe));
        cands.push(cwd.join("..").join("vendor").join("speex").join(exe));
    }
    cands.into_iter().find(|p| p.is_file())
}

/// 用 speexenc 编码：ffmpeg 先解成 8k/单声道/16bit WAV，再交给 speexenc。
pub fn speex_convert(
    tools: &Tools,
    exe: &Path,
    input: &Path,
    output: &Path,
    _total_s: f64,
    mut on_progress: ProgressFn,
) -> Result<u64> {
    let tag = temp_tag();
    let tmp_wav = std::env::temp_dir().join(format!("fc_speex_{tag}.wav"));
    let tmp_spx = std::env::temp_dir().join(format!("fc_speex_{tag}.spx"));
    on_progress(0.02);

    let mut dec = Command::new(&tools.ffmpeg);
    dec.args(["-v", "error", "-nostdin", "-y", "-i"])
        .arg(input)
        .args(["-map", "0:a:0", "-ac", "1", "-ar", "8000", "-c:a", "pcm_s16le"])
        .arg(&tmp_wav)
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    #[cfg(windows)]
    dec.creation_flags(CREATE_NO_WINDOW);
    let st = dec.status().context("启动 ffmpeg（Speex 解码）失败")?;
    if !st.success() {
        let _ = std::fs::remove_file(&tmp_wav);
        return Err(anyhow!("ffmpeg 解码成 8k WAV 失败"));
    }
    on_progress(0.55);

    let mut enc = Command::new(exe);
    enc.arg(&tmp_wav)
        .arg(&tmp_spx)
        .args(["--rate", "8000", "--quality", "8"])
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    #[cfg(windows)]
    enc.creation_flags(CREATE_NO_WINDOW);
    let st = enc.status().context("启动 speexenc 失败")?;
    if !st.success() {
        let _ = std::fs::remove_file(&tmp_wav);
        let _ = std::fs::remove_file(&tmp_spx);
        return Err(anyhow!("speexenc 编码失败"));
    }
    on_progress(0.95);
    std::fs::copy(&tmp_spx, output).context("复制 Speex 输出失败")?;
    let size = std::fs::metadata(output).map(|m| m.len()).unwrap_or(0);
    let _ = std::fs::remove_file(&tmp_wav);
    let _ = std::fs::remove_file(&tmp_spx);
    on_progress(1.0);
    Ok(size)
}
/// 找 WavPack 官方 CLI（wavpack.exe）。
///
/// 为什么需要它：ffmpeg 的 `wavpack` 编码器只有无损，**没有 hybrid 有损模式**
/// （`-h encoder=wavpack` 里只有 joint_stereo / optimize_mono 两个选项），
/// 而有损 WavPack 必须用官方 CLI 的 `-b` 参数。所以走和 atracdenc 一样的
/// "外部工具" 路线：用户自己装，程序只负责调用。
pub fn find_wavpack() -> Option<std::path::PathBuf> {
    let exe = if cfg!(windows) { "wavpack.exe" } else { "wavpack" };
    let mut cands: Vec<std::path::PathBuf> = Vec::new();
    if let Ok(cur) = std::env::current_exe() {
        if let Some(dir) = cur.parent() {
            cands.push(dir.join("wavpack").join(exe));
            cands.push(dir.join(exe));
        }
    }
    if let Some(path) = std::env::var_os("PATH") {
        for d in std::env::split_paths(&path) {
            cands.push(d.join(exe));
        }
    }
    cands.into_iter().find(|p| p.is_file())
}

/// 用 wavpack CLI 编有损（hybrid）：ffmpeg 先解成 WAV，再交给它带 `-b<kbps>` 压。
///
/// hybrid 模式会同时写出一个 `.wvc` 校正文件（默认行为是写），这里加 `-c`
/// 命令行里不加校正文件相关参数不行——所以显式传 `-c` 关掉它，
/// 不然输出目录里会莫名其妙多一个 .wvc。
pub fn wavpack_convert(
    tools: &Tools,
    exe: &Path,
    input: &Path,
    output: &Path,
    kbps: u32,
    _total_s: f64,
    mut on_progress: ProgressFn,
) -> Result<u64> {
    let tag = temp_tag();
    let tmp_wav = std::env::temp_dir().join(format!("fc_wv_{tag}.wav"));
    let tmp_wv = std::env::temp_dir().join(format!("fc_wv_{tag}.wv"));
    on_progress(0.02);

    let mut dec = Command::new(&tools.ffmpeg);
    dec.args(["-v", "error", "-nostdin", "-y", "-i"])
        .arg(input)
        .args(["-map", "0:a:0", "-vn", "-c:a", "pcm_s24le"])
        .arg(&tmp_wav)
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    #[cfg(windows)]
    dec.creation_flags(CREATE_NO_WINDOW);
    let st = dec.status().context("启动 ffmpeg（WavPack 解码）失败")?;
    if !st.success() {
        let _ = std::fs::remove_file(&tmp_wav);
        return Err(anyhow!("ffmpeg 解码成 WAV 失败"));
    }
    on_progress(0.55);

    let mut enc = Command::new(exe);
    enc.arg("-y")
        .arg("-q")
        // 注意：-b 的数值必须**连写**（-b384），拆成 "-b 384" 会被当成参数错误
        .arg(format!("-b{kbps}"))
        .arg(&tmp_wav)
        .arg("-o")
        .arg(&tmp_wv)
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    #[cfg(windows)]
    enc.creation_flags(CREATE_NO_WINDOW);
    let st = enc.status().context("启动 wavpack 失败")?;
    if !st.success() {
        let _ = std::fs::remove_file(&tmp_wav);
        let _ = std::fs::remove_file(&tmp_wv);
        return Err(anyhow!("wavpack 编码失败（检查 -b 码率是否合法）"));
    }
    on_progress(0.95);
    std::fs::copy(&tmp_wv, output).context("复制 WavPack 输出失败")?;
    let size = std::fs::metadata(output).map(|m| m.len()).unwrap_or(0);
    let _ = std::fs::remove_file(&tmp_wav);
    let _ = std::fs::remove_file(&tmp_wv);
    // 校正文件如果被写出来，顺手清掉，别在输出目录里留垃圾
    let _ = std::fs::remove_file(output.with_extension("wvc"));
    on_progress(1.0);
    Ok(size)
}
/// 找 OptimFROG 的官方 CLI（ofr.exe）。
///
/// 注意分发条款：官方 license 写明"把命令行编码器随其他软件分发可能需要授权"，
/// 所以我们**不随包**，只检测用户自己装的（程序目录 / optimfrog\ 子目录 / PATH）。
pub fn find_ofr() -> Option<std::path::PathBuf> {
    let exe = if cfg!(windows) { "ofr.exe" } else { "ofr" };
    let mut cands: Vec<std::path::PathBuf> = Vec::new();
    if let Ok(cur) = std::env::current_exe() {
        if let Some(dir) = cur.parent() {
            cands.push(dir.join("optimfrog").join(exe));
            cands.push(dir.join(exe));
        }
    }
    if let Some(path) = std::env::var_os("PATH") {
        for d in std::env::split_paths(&path) {
            cands.push(d.join(exe));
        }
    }
    cands.into_iter().find(|p| p.is_file())
}

/// 用 ofr.exe 编 OptimFROG：ffmpeg 先解成 24bit WAV，再交给它压。
///
/// `preset` 是官方 0~10 的压缩预设（8 = 5.1 倍速、省 3.06%；10 最慢最小）。
pub fn ofr_convert(
    tools: &Tools,
    exe: &Path,
    input: &Path,
    output: &Path,
    preset: u8,
    _total_s: f64,
    mut on_progress: ProgressFn,
) -> Result<u64> {
    let tag = temp_tag();
    let tmp_wav = std::env::temp_dir().join(format!("fc_ofr_{tag}.wav"));
    let tmp_ofr = std::env::temp_dir().join(format!("fc_ofr_{tag}.ofr"));
    on_progress(0.02);

    let mut dec = Command::new(&tools.ffmpeg);
    dec.args(["-v", "error", "-nostdin", "-y", "-i"])
        .arg(input)
        .args(["-map", "0:a:0", "-vn", "-c:a", "pcm_s24le", "-ac", "2"])
        .arg(&tmp_wav)
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    #[cfg(windows)]
    dec.creation_flags(CREATE_NO_WINDOW);
    let st = dec.status().context("启动 ffmpeg（OptimFROG 解码）失败")?;
    if !st.success() {
        let _ = std::fs::remove_file(&tmp_wav);
        return Err(anyhow!("ffmpeg 解码成 WAV 失败"));
    }
    on_progress(0.45);

    let mut enc = Command::new(exe);
    enc.arg("--encode")
        .arg(&tmp_wav)
        .args(["--output", &tmp_ofr.to_string_lossy()])
        .args(["--preset", &preset.min(10).to_string()])
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    #[cfg(windows)]
    enc.creation_flags(CREATE_NO_WINDOW);
    let st = enc.status().context("启动 ofr.exe 失败")?;
    if !st.success() {
        let _ = std::fs::remove_file(&tmp_wav);
        let _ = std::fs::remove_file(&tmp_ofr);
        return Err(anyhow!("OptimFROG 编码失败（检查输入 WAV 与 preset）"));
    }
    on_progress(0.95);
    std::fs::copy(&tmp_ofr, output).context("复制 OptimFROG 输出失败")?;
    let size = std::fs::metadata(output).map(|m| m.len()).unwrap_or(0);
    let _ = std::fs::remove_file(&tmp_wav);
    let _ = std::fs::remove_file(&tmp_ofr);
    on_progress(1.0);
    Ok(size)
}
/// 找 Musepack 官方编码器（mpcenc.exe，MSYS2 的 libmpcdec 包里带的）。
///
/// 许可 LGPL-2.1，可以随包；和 speex / wavpack 一样按"程序目录 → 子目录 → PATH"找。
pub fn find_mpcenc() -> Option<std::path::PathBuf> {
    let exe = if cfg!(windows) { "mpcenc.exe" } else { "mpcenc" };
    let mut cands: Vec<std::path::PathBuf> = Vec::new();
    if let Ok(cur) = std::env::current_exe() {
        if let Some(dir) = cur.parent() {
            cands.push(dir.join("musepack").join(exe));
            cands.push(dir.join(exe));
        }
    }
    if let Some(path) = std::env::var_os("PATH") {
        for d in std::env::split_paths(&path) {
            cands.push(d.join(exe));
        }
    }
    cands.into_iter().find(|p| p.is_file())
}

/// 用 mpcenc 编 Musepack：ffmpeg 先解成 16bit WAV，再交给它。
///
/// `profile` 0~4 对应官方档位：thumb / radio / standard / extreme / insane。
pub fn mpc_convert(
    tools: &Tools,
    exe: &Path,
    input: &Path,
    output: &Path,
    profile: u8,
    _total_s: f64,
    mut on_progress: ProgressFn,
) -> Result<u64> {
    let tag = temp_tag();
    let tmp_wav = std::env::temp_dir().join(format!("fc_mpc_{tag}.wav"));
    let tmp_mpc = std::env::temp_dir().join(format!("fc_mpc_{tag}.mpc"));
    let flag = match profile {
        0 => "--thumb",
        1 => "--radio",
        3 => "--extreme",
        4 => "--insane",
        _ => "--standard",
    };
    on_progress(0.02);

    let mut dec = Command::new(&tools.ffmpeg);
    dec.args(["-v", "error", "-nostdin", "-y", "-i"])
        .arg(input)
        .args(["-map", "0:a:0", "-vn", "-c:a", "pcm_s16le", "-ac", "2"])
        .arg(&tmp_wav)
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    #[cfg(windows)]
    dec.creation_flags(CREATE_NO_WINDOW);
    let st = dec.status().context("启动 ffmpeg（Musepack 解码）失败")?;
    if !st.success() {
        let _ = std::fs::remove_file(&tmp_wav);
        return Err(anyhow!("ffmpeg 解码成 WAV 失败"));
    }
    on_progress(0.45);

    let mut enc = Command::new(exe);
    enc.arg(flag)
        .arg("--silent")
        .arg(&tmp_wav)
        .arg(&tmp_mpc)
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    #[cfg(windows)]
    enc.creation_flags(CREATE_NO_WINDOW);
    let st = enc.status().context("启动 mpcenc 失败")?;
    if !st.success() {
        let _ = std::fs::remove_file(&tmp_wav);
        let _ = std::fs::remove_file(&tmp_mpc);
        return Err(anyhow!("Musepack 编码失败"));
    }
    on_progress(0.95);
    std::fs::copy(&tmp_mpc, output).context("复制 Musepack 输出失败")?;
    let size = std::fs::metadata(output).map(|m| m.len()).unwrap_or(0);
    let _ = std::fs::remove_file(&tmp_wav);
    let _ = std::fs::remove_file(&tmp_mpc);
    on_progress(1.0);
    Ok(size)
}

/// 找 PS1/PS2 编码器 psxavenc。
///
/// 许可 Zlib（可随包），但和 atracdenc 一样按"可选外部工具"处理：
/// 程序目录 → 程序目录\psxavenc → PATH。
pub fn find_psxavenc() -> Option<PathBuf> {
    let exe = if cfg!(windows) {
        "psxavenc.exe"
    } else {
        "psxavenc"
    };
    let mut cands: Vec<PathBuf> = Vec::new();
    if let Ok(cur) = std::env::current_exe() {
        if let Some(dir) = cur.parent() {
            cands.push(dir.join("psxavenc").join(exe));
            cands.push(dir.join(exe));
        }
    }
    if let Some(path) = std::env::var_os("PATH") {
        for d in std::env::split_paths(&path) {
            cands.push(d.join(exe));
        }
    }
    cands.into_iter().find(|p| p.is_file())
}

/// 用 psxavenc 编 PS1 XA（`kind = "xa"`）或 PS2 VAG（`kind = "vag"`）。
///
/// 两段式：先用 ffmpeg 解成 psxavenc 要的 WAV（XA：37.8kHz 立体声；
/// VAG：44.1kHz 单声道），再交给它。psxavenc 自己会做 ADPCM 编码。
pub fn psx_convert(
    tools: &Tools,
    exe: &Path,
    input: &Path,
    output: &Path,
    kind: &str,
    _total_s: f64,
    mut on_progress: ProgressFn,
) -> Result<u64> {
    let (rate, channels) = match kind {
        "xa" => (37800u32, 2u16),
        _ => (44100, 1),
    };
    let tag = temp_tag();
    let tmp_wav = std::env::temp_dir().join(format!("fc_psx_{tag}.wav"));
    let tmp_out = std::env::temp_dir().join(format!("fc_psx_{tag}.{kind}"));
    on_progress(0.02);

    let mut dec = Command::new(&tools.ffmpeg);
    dec.args(["-v", "error", "-nostdin", "-y", "-i"])
        .arg(input)
        .args(["-map", "0:a:0", "-vn", "-c:a", "pcm_s16le"])
        .args(["-ar", &rate.to_string(), "-ac", &channels.to_string()])
        .arg(&tmp_wav)
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    #[cfg(windows)]
    dec.creation_flags(CREATE_NO_WINDOW);
    let st = dec.status().context("启动 ffmpeg（psxavenc 解码）失败")?;
    if !st.success() {
        let _ = std::fs::remove_file(&tmp_wav);
        return Err(anyhow!("ffmpeg 解码成 WAV 失败"));
    }
    on_progress(0.5);

    let mut enc = Command::new(exe);
    enc.args(["-q", "-t", kind])
        .arg(&tmp_wav)
        .arg(&tmp_out)
        .stdout(Stdio::null())
        .stderr(Stdio::piped());
    #[cfg(windows)]
    enc.creation_flags(CREATE_NO_WINDOW);
    let out = enc.output().context("启动 psxavenc 失败")?;
    if !out.status.success() || !tmp_out.is_file() {
        let _ = std::fs::remove_file(&tmp_wav);
        let _ = std::fs::remove_file(&tmp_out);
        let err = String::from_utf8_lossy(&out.stderr);
        let mut tail: Vec<&str> = err.lines().rev().take(4).collect();
        tail.reverse();
        return Err(anyhow!(
            "psxavenc 编码失败（退出码 {:?}）：\n{}",
            out.status.code(),
            tail.join("\n")
        ));
    }
    on_progress(0.95);
    std::fs::copy(&tmp_out, output).context("复制 psxavenc 输出失败")?;
    let size = std::fs::metadata(output).map(|m| m.len()).unwrap_or(0);
    let _ = std::fs::remove_file(&tmp_wav);
    let _ = std::fs::remove_file(&tmp_out);
    on_progress(1.0);
    Ok(size)
}

/// 找 QOA 参考转换器（qoaconv.exe，MIT，50 KB 左右）。
pub fn find_qoaconv() -> Option<PathBuf> {
    let exe = if cfg!(windows) { "qoaconv.exe" } else { "qoaconv" };
    let mut cands: Vec<PathBuf> = Vec::new();
    if let Ok(cur) = std::env::current_exe() {
        if let Some(dir) = cur.parent() {
            cands.push(dir.join("qoa").join(exe));
            cands.push(dir.join(exe));
        }
    }
    if let Some(path) = std::env::var_os("PATH") {
        for d in std::env::split_paths(&path) {
            cands.push(d.join(exe));
        }
    }
    cands.into_iter().find(|p| p.is_file())
}

/// 用 qoaconv 编 QOA：ffmpeg 解成 16-bit WAV，再交给它。
pub fn qoa_convert(
    tools: &Tools,
    exe: &Path,
    input: &Path,
    output: &Path,
    _total_s: f64,
    mut on_progress: ProgressFn,
) -> Result<u64> {
    let tag = temp_tag();
    let tmp_wav = std::env::temp_dir().join(format!("fc_qoa_{tag}.wav"));
    let tmp_out = std::env::temp_dir().join(format!("fc_qoa_{tag}.qoa"));
    on_progress(0.02);

    let mut dec = Command::new(&tools.ffmpeg);
    dec.args(["-v", "error", "-nostdin", "-y", "-i"])
        .arg(input)
        .args(["-map", "0:a:0", "-vn", "-c:a", "pcm_s16le", "-ac", "2"])
        .arg(&tmp_wav)
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    #[cfg(windows)]
    dec.creation_flags(CREATE_NO_WINDOW);
    let st = dec.status().context("启动 ffmpeg（QOA 解码）失败")?;
    if !st.success() {
        let _ = std::fs::remove_file(&tmp_wav);
        return Err(anyhow!("ffmpeg 解码成 WAV 失败"));
    }
    on_progress(0.5);

    let mut enc = Command::new(exe);
    enc.arg(&tmp_wav)
        .arg(&tmp_out)
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    #[cfg(windows)]
    enc.creation_flags(CREATE_NO_WINDOW);
    let st = enc.status().context("启动 qoaconv 失败")?;
    if !st.success() || !tmp_out.is_file() {
        let _ = std::fs::remove_file(&tmp_wav);
        let _ = std::fs::remove_file(&tmp_out);
        return Err(anyhow!("qoaconv 编码失败"));
    }
    on_progress(0.95);
    std::fs::copy(&tmp_out, output).context("复制 QOA 输出失败")?;
    let size = std::fs::metadata(output).map(|m| m.len()).unwrap_or(0);
    let _ = std::fs::remove_file(&tmp_wav);
    let _ = std::fs::remove_file(&tmp_out);
    on_progress(1.0);
    Ok(size)
}

/// 找任天堂系编码器 VGAudioCli（MIT，单文件 exe）。
pub fn find_vgaudio() -> Option<PathBuf> {
    let exe = if cfg!(windows) {
        "VGAudioCli.exe"
    } else {
        "VGAudioCli"
    };
    let mut cands: Vec<PathBuf> = Vec::new();
    if let Ok(cur) = std::env::current_exe() {
        if let Some(dir) = cur.parent() {
            cands.push(dir.join("vgaudio").join(exe));
            cands.push(dir.join(exe));
        }
    }
    if let Some(path) = std::env::var_os("PATH") {
        for d in std::env::split_paths(&path) {
            cands.push(d.join(exe));
        }
    }
    cands.into_iter().find(|p| p.is_file())
}

/// 用 VGAudioCli 编 BRSTM / BCSTM / BFSTM / DSP / IDSP。
///
/// 输出格式由**文件后缀**决定（这是 VGAudioCli 的约定），所以临时文件
/// 必须沿用目标后缀。
pub fn vgaudio_convert(
    tools: &Tools,
    exe: &Path,
    input: &Path,
    output: &Path,
    _total_s: f64,
    mut on_progress: ProgressFn,
) -> Result<u64> {
    let ext = output
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("brstm")
        .to_ascii_lowercase();
    let tag = temp_tag();
    let tmp_wav = std::env::temp_dir().join(format!("fc_vg_{tag}.wav"));
    let tmp_out = std::env::temp_dir().join(format!("fc_vg_{tag}.{ext}"));
    on_progress(0.02);

    let mut dec = Command::new(&tools.ffmpeg);
    dec.args(["-v", "error", "-nostdin", "-y", "-i"])
        .arg(input)
        .args(["-map", "0:a:0", "-vn", "-c:a", "pcm_s16le", "-ar", "44100", "-ac", "2"])
        .arg(&tmp_wav)
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    #[cfg(windows)]
    dec.creation_flags(CREATE_NO_WINDOW);
    let st = dec.status().context("启动 ffmpeg（VGAudio 解码）失败")?;
    if !st.success() {
        let _ = std::fs::remove_file(&tmp_wav);
        return Err(anyhow!("ffmpeg 解码成 WAV 失败"));
    }
    on_progress(0.45);

    let mut enc = Command::new(exe);
    enc.arg("-i")
        .arg(&tmp_wav)
        .arg(&tmp_out)
        .stdout(Stdio::null())
        .stderr(Stdio::piped());
    #[cfg(windows)]
    enc.creation_flags(CREATE_NO_WINDOW);
    let out = enc.output().context("启动 VGAudioCli 失败（需要 .NET Framework 4.5+）")?;
    if !out.status.success() || !tmp_out.is_file() {
        let _ = std::fs::remove_file(&tmp_wav);
        let _ = std::fs::remove_file(&tmp_out);
        let err = String::from_utf8_lossy(&out.stderr);
        let mut tail: Vec<&str> = err.lines().rev().take(4).collect();
        tail.reverse();
        return Err(anyhow!(
            "VGAudioCli 编码失败（退出码 {:?}）：\n{}",
            out.status.code(),
            tail.join("\n")
        ));
    }
    on_progress(0.95);
    std::fs::copy(&tmp_out, output).context("复制 VGAudio 输出失败")?;
    let size = std::fs::metadata(output).map(|m| m.len()).unwrap_or(0);
    let _ = std::fs::remove_file(&tmp_wav);
    let _ = std::fs::remove_file(&tmp_out);
    on_progress(1.0);
    Ok(size)
}

/// 找 openmpt123（模块音乐渲染器，libopenmpt / BSD-3-Clause）。
pub fn find_openmpt123() -> Option<PathBuf> {
    let exe = if cfg!(windows) {
        "openmpt123.exe"
    } else {
        "openmpt123"
    };
    let mut cands: Vec<PathBuf> = Vec::new();
    if let Ok(cur) = std::env::current_exe() {
        if let Some(dir) = cur.parent() {
            cands.push(dir.join("openmpt123").join(exe));
            cands.push(dir.join(exe));
        }
    }
    if let Some(path) = std::env::var_os("PATH") {
        for d in std::env::split_paths(&path) {
            cands.push(d.join(exe));
        }
    }
    cands.into_iter().find(|p| p.is_file())
}

/// 模块音乐扩展名（MOD / XM / IT / S3M 等 tracker 格式）。
pub fn is_module_ext(path: &Path) -> bool {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    matches!(
        ext.as_str(),
        "mod" | "xm" | "it" | "s3m" | "mptm" | "mo3" | "umx" | "669" | "amf" | "ams" | "c67"
            | "dbm" | "digi" | "dmf" | "dsym" | "dtm" | "far" | "fmt" | "gdm" | "imf" | "ice"
            | "j2b" | "m15" | "mdl" | "med" | "mg" | "mmcmp" | "mt2" | "mtm" | "nst" | "okta"
            | "plm" | "psm" | "pt36" | "ptm" | "sfx" | "sfx2" | "st26" | "stk" | "stm" | "stp"
            | "ult" | "uw" | "xpk"
    )
}

/// 用 openmpt123 把模块音乐渲染成 44.1kHz 立体声 16-bit WAV。
pub fn openmpt_decode(exe: &Path, input: &Path, output: &Path) -> Result<()> {
    let mut cmd = Command::new(exe);
    cmd.args([
        "--batch",
        "--force",
        "--quiet",
        "--samplerate",
        "44100",
        "--channels",
        "2",
        "--no-float",
        "-o",
    ])
    .arg(output)
    .arg(input)
    .stdout(Stdio::null())
    .stderr(Stdio::piped());
    #[cfg(windows)]
    cmd.creation_flags(CREATE_NO_WINDOW);
    let out = cmd.output().context("启动 openmpt123 失败")?;
    if !out.status.success() || !output.is_file() {
        let err = String::from_utf8_lossy(&out.stderr);
        let mut tail: Vec<&str> = err.lines().rev().take(3).collect();
        tail.reverse();
        return Err(anyhow!(
            "openmpt123 渲染失败（退出码 {:?}）：\n{}",
            out.status.code(),
            tail.join("\n")
        ));
    }
    Ok(())
}

/// 找 vgmstream-cli（游戏流式音频解码器，ISC 许可）。
pub fn find_vgmstream() -> Option<PathBuf> {
    let exe = if cfg!(windows) {
        "vgmstream-cli.exe"
    } else {
        "vgmstream-cli"
    };
    let mut cands: Vec<PathBuf> = Vec::new();
    if let Ok(cur) = std::env::current_exe() {
        if let Some(dir) = cur.parent() {
            cands.push(dir.join("vgmstream").join(exe));
            cands.push(dir.join(exe));
        }
    }
    if let Some(path) = std::env::var_os("PATH") {
        for d in std::env::split_paths(&path) {
            cands.push(d.join(exe));
        }
    }
    cands.into_iter().find(|p| p.is_file())
}

/// 这些扩展名 ffmpeg 读不了（或读得不好），交给 vgmstream 先解成 WAV。
pub fn is_vgmstream_ext(path: &Path) -> bool {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    matches!(
        ext.as_str(),
        "psf" | "psf2" | "minipsf" | "minipsf2" | "ssf" | "minissf" | "brstm" | "bcstm"
            | "bfstm" | "bfwav" | "bcwav" | "hca" | "ads" | "ss2" | "svag" | "genh" | "dsp"
            | "idsp" | "vadpcm"
    )
}

/// 用 vgmstream-cli 把一个游戏音频文件解成 WAV（16-bit PCM）。
///
/// 输出文件名由调用方给；成功后可以直接拿它去 probe / 转码。
pub fn vgmstream_decode(exe: &Path, input: &Path, output: &Path) -> Result<()> {
    let mut cmd = Command::new(exe);
    cmd.args(["-o"])
        .arg(output)
        .arg(input)
        .stdout(Stdio::null())
        .stderr(Stdio::piped());
    #[cfg(windows)]
    cmd.creation_flags(CREATE_NO_WINDOW);
    let out = cmd.output().context("启动 vgmstream-cli 失败")?;
    if !out.status.success() || !output.is_file() {
        let err = String::from_utf8_lossy(&out.stderr);
        let mut tail: Vec<&str> = err.lines().rev().take(3).collect();
        tail.reverse();
        return Err(anyhow!(
            "vgmstream 解码失败（退出码 {:?}）：\n{}",
            out.status.code(),
            tail.join("\n")
        ));
    }
    Ok(())
}

/// 找 N64 VADPCM 编解码器（vadpcm.exe，MPL-2.0）。
pub fn find_vadpcm() -> Option<PathBuf> {
    let exe = if cfg!(windows) { "vadpcm.exe" } else { "vadpcm" };
    let mut cands: Vec<PathBuf> = Vec::new();
    if let Ok(cur) = std::env::current_exe() {
        if let Some(dir) = cur.parent() {
            cands.push(dir.join("vadpcm").join(exe));
            cands.push(dir.join(exe));
        }
    }
    if let Some(path) = std::env::var_os("PATH") {
        for d in std::env::split_paths(&path) {
            cands.push(d.join(exe));
        }
    }
    cands.into_iter().find(|p| p.is_file())
}

/// 用 vadpcm 编 N64 VADPCM：ffmpeg 先解成 16-bit 单声道 WAV，再交给它。
///
/// 输入必须是 16-bit mono（这是 vadpcm 的硬限制），输出固定 .aifc。
pub fn vadpcm_convert(
    tools: &Tools,
    exe: &Path,
    input: &Path,
    output: &Path,
    _total_s: f64,
    mut on_progress: ProgressFn,
) -> Result<u64> {
    let tag = temp_tag();
    let tmp_wav = std::env::temp_dir().join(format!("fc_vad_{tag}.wav"));
    let tmp_out = std::env::temp_dir().join(format!("fc_vad_{tag}.aifc"));
    on_progress(0.02);

    let mut dec = Command::new(&tools.ffmpeg);
    dec.args(["-v", "error", "-nostdin", "-y", "-i"])
        .arg(input)
        .args(["-map", "0:a:0", "-vn", "-c:a", "pcm_s16le", "-ac", "1"])
        .arg(&tmp_wav)
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    #[cfg(windows)]
    dec.creation_flags(CREATE_NO_WINDOW);
    let st = dec.status().context("启动 ffmpeg（VADPCM 解码）失败")?;
    if !st.success() {
        let _ = std::fs::remove_file(&tmp_wav);
        return Err(anyhow!("ffmpeg 解码成单声道 WAV 失败"));
    }
    on_progress(0.5);

    let mut enc = Command::new(exe);
    enc.args(["encode", "--quiet"])
        .arg(&tmp_wav)
        .arg(&tmp_out)
        .stdout(Stdio::null())
        .stderr(Stdio::piped());
    #[cfg(windows)]
    enc.creation_flags(CREATE_NO_WINDOW);
    let out = enc.output().context("启动 vadpcm 失败")?;
    if !out.status.success() || !tmp_out.is_file() {
        let _ = std::fs::remove_file(&tmp_wav);
        let _ = std::fs::remove_file(&tmp_out);
        let err = String::from_utf8_lossy(&out.stderr);
        let mut tail: Vec<&str> = err.lines().rev().take(4).collect();
        tail.reverse();
        return Err(anyhow!(
            "vadpcm 编码失败（退出码 {:?}）：\n{}",
            out.status.code(),
            tail.join("\n")
        ));
    }
    on_progress(0.95);
    std::fs::copy(&tmp_out, output).context("复制 VADPCM 输出失败")?;
    let size = std::fs::metadata(output).map(|m| m.len()).unwrap_or(0);
    let _ = std::fs::remove_file(&tmp_wav);
    let _ = std::fs::remove_file(&tmp_out);
    on_progress(1.0);
    Ok(size)
}

/// 外部编码器的输出自检。ffmpeg 读不了 `.xa/.dsp/.idsp/VADPCM .aifc/.ofr`，
/// 所以分别用扇区结构 / VGAudioCli / vadpcm / ofr 回读；其余格式直接用 ffmpeg 解 10 秒。
///
/// 这里**不比时长**：Musepack 的 SV8 头时长字段本来就偏短（解码出来是对的），
/// 用时长判断会误杀。返回一句人话说明用了哪种校验。
pub fn verify_output(id: &str, output: &Path, tools: &Tools, _expected_s: f64) -> Result<String, String> {
    let meta = std::fs::metadata(output).map_err(|e| format!("输出文件不存在：{e}"))?;
    if meta.len() == 0 {
        return Err("输出文件是 0 字节".to_string());
    }

    // PS1 XA：没有解码器，按 2336 字节/扇区的结构查
    if id == "xa" {
        return if meta.len() % 2336 == 0 {
            Ok(format!("XA 扇区结构检查通过（{} 个扇区）", meta.len() / 2336))
        } else {
            Err(format!("XA 文件大小 {} 不是 2336 的整数倍", meta.len()))
        };
    }

    // OptimFROG：官方 CLI 自带 --verify
    if id == "optimfrog" {
        let exe = find_ofr().ok_or_else(|| "找不到 ofr.exe，无法回读校验".to_string())?;
        let out = Command::new(&exe)
            .arg("--verify")
            .arg(output)
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .output()
            .map_err(|e| format!("启动 ofr.exe 失败：{e}"))?;
        if !out.status.success() {
            let err = String::from_utf8_lossy(&out.stderr);
            let why = err.lines().rev().find(|l| !l.trim().is_empty()).unwrap_or("ofr --verify 失败");
            return Err(format!("OptimFROG 回读失败：{why}"));
        }
        return Ok("ofr --verify 通过".to_string());
    }

    // 任天堂容器：dsp / idsp ffmpeg 读不了，统一用 VGAudioCli 解回 WAV
    if matches!(id, "vgaudio" | "dsp" | "idsp" | "brstm" | "bcstm" | "bfstm") {
        if let Some(exe) = find_vgaudio() {
            let tag = temp_tag();
            let tmp = std::env::temp_dir().join(format!("fc_vgv_{tag}.wav"));
            let res = Command::new(&exe)
                .arg("-i")
                .arg(output)
                .arg("-o")
                .arg(&tmp)
                .stdout(Stdio::null())
                .stderr(Stdio::piped())
                .output();
            let size = std::fs::metadata(&tmp).map(|m| m.len()).unwrap_or(0);
            let _ = std::fs::remove_file(&tmp);
            match res {
                Ok(o) if o.status.success() && size > 44 => {
                    return Ok("VGAudioCli 回读通过".to_string());
                }
                Ok(o) => {
                    let err = String::from_utf8_lossy(&o.stderr);
                    let why = err.lines().rev().find(|l| !l.trim().is_empty()).unwrap_or("回读失败");
                    return Err(format!("VGAudioCli 回读失败：{why}"));
                }
                Err(e) => return Err(format!("启动 VGAudioCli 失败：{e}")),
            }
        }
        // 没有 VGAudioCli 就退回 ffmpeg（brstm/bcstm/bfstm 能读）
    }

    // N64 VADPCM：vadpcm decode 回 WAV
    if id == "vadpcm" {
        let exe = find_vadpcm().ok_or_else(|| "找不到 vadpcm.exe，无法回读校验".to_string())?;
        let tag = temp_tag();
        let tmp = std::env::temp_dir().join(format!("fc_vadv_{tag}.wav"));
        let res = Command::new(&exe)
            .arg("decode")
            .arg(output)
            .arg(&tmp)
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .output();
        let size = std::fs::metadata(&tmp).map(|m| m.len()).unwrap_or(0);
        let _ = std::fs::remove_file(&tmp);
        return match res {
            Ok(o) if o.status.success() && size > 44 => Ok("vadpcm 回读通过".to_string()),
            Ok(o) => {
                let err = String::from_utf8_lossy(&o.stderr);
                let why = err.lines().rev().find(|l| !l.trim().is_empty()).unwrap_or("回读失败");
                Err(format!("vadpcm 回读失败：{why}"))
            }
            Err(e) => Err(format!("启动 vadpcm 失败：{e}")),
        };
    }

    // 其余外部输出 ffmpeg 都能解：解码前 10 秒
    let out = Command::new(&tools.ffmpeg)
        .args(["-v", "error", "-nostdin", "-i"])
        .arg(output)
        .args(["-map", "0", "-t", "10", "-f", "null", "-"])
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .output()
        .map_err(|e| format!("启动 ffmpeg 自检失败：{e}"))?;
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr);
        let why = err.lines().rev().find(|l| !l.trim().is_empty()).unwrap_or("解码失败");
        return Err(format!("输出无法解码：{why}"));
    }
    Ok("ffmpeg 解码自检通过".to_string())
}