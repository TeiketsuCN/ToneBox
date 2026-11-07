//! fc-analyze —— 频谱质量评估。
//!
//! 状态：**未实现**，契约已定，等机器 B 接手。
//!
//! 参考实现已经在本机跑通（Python 版，见 `tools/analyze.py`），
//! 本次在 44 个对照样本 + 15 个真实 ATRAC 文件上做过标定，结论：
//!
//! 1. **同一张阈值表不能通用**：ATRAC3 66k 的低通边沿在 15.2kHz，
//!    而 MP3 64k 在 11.5kHz。混用会把 ATRAC 系统性判低两档。
//! 2. **Opus 在 64k 仍保留全带宽**（噪声整形），截止法对它完全失效，
//!    必须换判据（谱平坦度 / 噪声底电平）。
//! 3. 判据用"最陡跌落"而不是"跌破 -80dB"：真实编码器边沿 ≥40dB/kHz，
//!    母带衰减和噪底要缓得多。
//!
//! 移植时请照抄 `tools/analyze.py` 的 `find_edge()` 与 `TABLES`，
//! 系数都是实测标定出来的，不要凭直觉改。

use std::path::Path;

use fc_core::MediaInfo;

#[derive(Debug, Clone)]
pub struct QualityReport {
    /// 频谱低通边沿（Hz）。未找到陡峭边沿时等于 Nyquist。
    pub cutoff_hz: f64,
    /// 边沿处的陡度（dB/kHz，负值）
    pub slope_db_per_khz: f64,
    /// 推测的源码率档位描述，如 "128"、"无损/CD"
    pub estimate: String,
    /// 0.0 ~ 1.0
    pub confidence: f64,
    /// 误判风险说明，永远不为空才好
    pub caveats: Vec<String>,
}

/// 分析一个文件的频谱质量。
///
/// 实现要点（照 `tools/analyze.py`）：
///   1. ffmpeg 解码成单声道 f32le，**保持原始采样率**，不要重采样
///   2. 4096 点 Hann 窗、50% 重叠、跨帧平均功率谱
///   3. 以 1~4kHz 中频段为参考电平归一化到 dB
///   4. 找 ±300Hz 窗口内的最陡负斜率作为边沿
///   5. 按编码族查表（mp3 / aac / atrac3 / atrac3p / opus 各一张）
pub fn analyze(_path: &Path, _info: &MediaInfo) -> anyhow::Result<QualityReport> {
    anyhow::bail!("频谱分析尚未实现（契约见本文件头注释，参考实现见 tools/analyze.py）")
}
