//! DSD 输出的转换胶水：ffmpeg 解 PCM → [`crate::dsdout`] 写 DSF/DFF。
//!
//! 为什么统一重采样到 44100：
//! - DSD 率只有两个家族（2.8224M 和 3.072M），源采样率五花八门（96k、192k…），
//!   与其做分数倍升采样，不如让 ffmpeg 先落到 44.1k 家族，再整数倍升，
//!   数值上也稳定（升采样倍率正好等于 64/128/256）。
//! - 声道固定下混到 2：DSD 容器我们只写 1~2 声道。

use std::io::Read;
use std::path::Path;
use std::process::{Command, Stdio};

use anyhow::{anyhow, Context, Result};

use crate::dsdout::{Container, DsdWriter, Tier};
use crate::ffmpeg::{ProgressFn, Tools};

#[cfg(windows)]
use std::os::windows::process::CommandExt;
#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// 目标 DSD 率都按 44.1k 家族算
const BASE_RATE: u32 = 44_100;

/// 把任意输入转成 DSD 文件（DSF 或 DFF）。返回文件字节数。
pub fn dsd_convert(
    tools: &Tools,
    input: &Path,
    output: &Path,
    tier: Tier,
    container: Container,
    total_s: f64,
    mut on_progress: ProgressFn,
) -> Result<u64> {
    on_progress(0.02);
    let mut child = Command::new(&tools.ffmpeg)
        .args(["-v", "error", "-nostdin", "-i"])
        .arg(input)
        .args([
            "-map",
            "0:a:0",
            "-vn",
            "-ac",
            "2",
            "-ar",
            &BASE_RATE.to_string(),
            "-f",
            "f32le",
            "-",
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .context("启动 ffmpeg（DSD 解码）失败")?;

    let mut writer = DsdWriter::new(output, container, tier, BASE_RATE, 2)
        .context("创建 DSD 文件失败")?;

    // 进度按已读字节 / 预估总字节算；估不准就退化成"到 90% 就停"
    let est_bytes = (total_s * BASE_RATE as f64 * 2.0 * 4.0).max(1.0);
    let mut read_bytes: f64 = 0.0;
    let mut buf = vec![0u8; 64 * 1024];
    let mut f32buf: Vec<f32> = Vec::with_capacity(16 * 1024);
    let mut stdout = child.stdout.take().ok_or_else(|| anyhow!("拿不到 ffmpeg 输出"))?;
    loop {
        let n = stdout.read(&mut buf).context("读 ffmpeg PCM 失败")?;
        if n == 0 {
            break;
        }
        read_bytes += n as f64;
        f32buf.clear();
        for chunk in buf[..n].chunks_exact(4) {
            f32buf.push(f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]));
        }
        writer.push(&f32buf).context("写 DSD 数据失败")?;
        let frac = (read_bytes / est_bytes).min(1.0);
        on_progress((0.02 + 0.93 * frac) as f32);
    }
    // 等 ffmpeg 收尾，拿它的 stderr 做失败说明
    let mut err = String::new();
    if let Some(mut e) = child.stderr.take() {
        let _ = e.read_to_string(&mut err);
    }
    let st = child.wait().context("等待 ffmpeg 退出失败")?;
    if !st.success() {
        let why = err.lines().rev().find(|l| !l.trim().is_empty()).unwrap_or("ffmpeg 解码失败");
        return Err(anyhow!("ffmpeg 解码失败：{why}"));
    }
    let size = writer.finish().context("收尾 DSD 文件失败")?;
    on_progress(1.0);
    Ok(size)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base_rate_is_44100_family() {
        // DSD64/128/256 都该落在 44.1k 家族的整数倍上
        for (tier, want) in [
            (Tier::D64, 2_822_400u32),
            (Tier::D128, 5_644_800),
            (Tier::D256, 11_289_600),
        ] {
            assert_eq!(tier.dsd_rate(BASE_RATE), want);
        }
    }

    #[test]
    fn containers_are_two() {
        assert_ne!(Container::Dsf, Container::Dff);
    }
}
