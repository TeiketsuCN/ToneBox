# 机器 B → 机器 A 的留言

> 写在最上面的是最新的。追加，不要覆盖。

---

## 2026-11-08 · B → A（长文件性能）

**完成**：先量后改。release 构建，测试文件是 `pop48k_ref.flac` 循环拼的 30/60 分钟 FLAC。

| 文件 | 改造前 | 改造后 |
|---|---:|---:|
| 30 分钟 | 2.096s | **0.342s** |
| 60 分钟 | 6.084s / 峰值 1655MB | **0.345s / 峰值 21.4MB** |

**方案**：≤10 分钟整段解码；>10 分钟取 10% / 50% / 90% 三个 30 秒窗口，共 90 秒，避免只看开头。内存从整段 `Vec<f32>` 改成 90 秒窗口。

**一个坑**：一开始阈值设 180 秒，`aa3_sonateno14op27-2.aa3`（328 秒）被采样漏掉，60/60 掉到 59/60。改成 600 秒后，5~6 分钟的曲目仍整段分析，30 分钟以上才采样，标定恢复 60/60。

**验证**：`cargo test -p fc-analyze` 产品口径 60/60、非 Opus 52/52、最保守 52/60；`reference_values` 全过。报告在 `crates/fc-analyze/PERF.md`，另加了 `examples/analyze_file.rs` 方便复测。

**分支**：`engine/perf`，已 push。只动 `crates/fc-analyze/**` 和留言，`identify.rs`、`fc-gui` 没碰。

---

## 2026-11-08 · B → A（Python 参考实现已对齐）

**完成**：`tools/analyze.py` 已按 Rust 版重写对齐：

- Opus 判断提前到“无陡峭边沿”之前
- 去掉 Nyquist 缩放，阈值改绝对 Hz
- 无损按 codec 判定
- 一并补齐档位区间、MP3 128/192/320 的陡度细分、AAC 无边缘 256、HE-AAC profile 特判

**验证**：

- `tools/calibrate.py`：57/65 = 88%（65 里有 5 个重复文件；去重后 52/60 = 86.7%，和 Rust 版一致，8 个 Opus 按设计算“错”）。
- 7 个 reference 样本的 cutoff/slope/estimate 与 Rust 版一致。
- `py_compile` 通过。

**HE-AAC 真实样本**：没搞到，按你说的跳过，没有硬造。

**分支**：`engine/py-align`，已 push。

**注意**：`docs/报告-频谱标定.md` 里“与 Python 版有意不同的四处”现在已经过时；按你这次给的路径限制我没动 docs，合并时你顺手改一下，或者下轮我补。

---

## 2026-11-08 · B → A（lossy 百科 + HE-AAC）

**完成**：`format/lossy.rs` 补了 11 条只读百科，全部 `can_encode: false` / `primary: false`，按你的清单：
AC-3、E-AC-3、WMA、WMA Pro、Musepack、Speex、AMR-NB、AMR-WB、RealAudio (Cook)、HE-AAC、xHE-AAC。
`mod.rs` 没动，ID 都查过，没有重复。

**HE-AAC**：`analyze` 现在会用 ffprobe 读 `stream.profile`，识别到 HE-AAC / xHE-AAC 就直接输出“不可判定”（置信度 0.12），不套 AAC 表。AAC-LC 样本不受影响，60/60 复跑仍过。当前样本集没有真实 HE-AAC，只验证了 profile 字符串分支，报告里标了风险，后面需要补 2~3 个真实样本。

**验证**：`cargo build` 通过；`cargo test -p fc-analyze` 的单元测试、60/60 标定、reference 回归全过。

**分支**：`engine/lossy-atlas`，已 push。`mod.rs`、`fc-gui`、`fc-player` 没碰。

**排队**：`tools/analyze.py` 的三处对齐（Opus 提前、绝对 Hz、无损按 codec）还没动，等这个合了再做。

---

## 2026-11-08 · B → A（pull 确认）

已 pull 到 `4c4c071`，合并进 `engine/spectrum`（merge `56ffdfc`）。全工程 `cargo build` 通过；`cargo test -p fc-analyze` 仍是产品口径 60/60、非 Opus 52/52、最保守 52/60 = 86.7%。

`format/lossy.rs` 的新边界收到，任务 01 的实现仍在 `0ff31c8`，等你合并；格式表拆分没有影响 `fc-analyze`。

**需要 A**：先合 `engine/spectrum`。合完我按你的提醒补 `lossy.rs` 的只读百科条目（AC-3 / E-AC-3 / WMA / Musepack 等），只动 `lossy.rs`，不碰 `mod.rs`。如果先做别的，留言说一声。

---

## 2026-11-08 · B → A

**完成**：环境已按接入指南跑通，任务 01 已实现并推送到 `engine/spectrum`（实现提交 `0ff31c8`）。

- `crates/fc-analyze/src/lib.rs`：解码、4096 点 Hann 窗、平均功率谱、最陡斜率、按编码族查表全部移植完；Opus 输出“不可判定”。
- 标定集：44 个对照样本 + 16 个真实 ATRAC，共 60 个。产品口径（Opus 算正确）60/60；非 Opus 码率 52/52；最保守口径（Opus 按码率硬算）52/60 = 86.7%。
- 测试：`crates/fc-analyze/tests/calibration.rs`、`reference_values.rs`；报告：`docs/报告-频谱标定.md`。
- `cargo build` 通过；`--convert` MP3→FLAC、`--play` ATRAC3+ 两项验收通过。
- 依赖新增 `rustfft`，所以 `Cargo.lock` 有变更；`fc-gui` 和 `fc-player` 没碰。

**需要 A**：合并 `engine/spectrum`。另外 `format.rs` 的三模块拆分我还没动，等你更新 `docs/契约.md` 并在留言里说一声后，我再把 lossy 条目挪进 `format/lossy.rs`。

**分支**：`engine/spectrum`，已 push；`main` 未动。

