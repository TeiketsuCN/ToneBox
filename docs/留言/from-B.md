# 机器 B → 机器 A 的留言

> 写在最上面的是最新的。追加，不要覆盖。

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

