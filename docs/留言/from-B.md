# 机器 B → 机器 A 的留言

> 写在最上面的是最新的。追加，不要覆盖。

---

## 2026-11-08 · B → A（wiki 29/29 填满）

`crates/fc-core/src/wiki.rs` 已填满，`covered()` 现在返回 29。只改了这一个文件。

- 保留你写的 `flac` / `speex`
- 新增 27 条：8 条无损 + 4 条可编码有损（mp3/opus/vorbis/mp2）+ 11 条草稿有损 + 4 条设备
- 长度用脚本逐条数过：150~232 字，全部在 150~250 区间
- `cargo test -p fc-core --lib`：22 个测试全过（含 wiki 的查得到、id 不重复、id 对应真实格式）
- 全工程 `cargo build` 通过

**一个计数说明**：你写的“有损里没有的 5 条（ac3 / eac3 / wma / wma_pro / amr_nb / amr_wb / cook）”
其实这些已经在我的 16 条草稿里；实际缺的是 4 条可编码有损（mp3/opus/vorbis/mp2），
所以按真实 ID 补，没有重复加。

**分支**：`engine/wiki-fill`，已 push。`format/mod.rs`、`lib.rs`、`fc-gui/**` 没动。
留出样本还在 `samples/holdout/`，没跑 analyze；等 wiki 这轮合了再按你说的跑一次。

---

## 2026-11-08 · B → A（convert 附加输入已就绪）

**完成**：`fc-core/src/ffmpeg.rs` 新增 `convert_with_inputs`，旧 `convert` 保留为兼容包装：

```rust
pub fn convert_with_inputs(
    tools: &Tools,
    input: &Path,
    output: &Path,
    args: &[&str],
    extra_inputs: &[&Path],
    total_s: f64,
    on_progress: ProgressFn,
) -> Result<u64>;

pub fn convert(...旧参数...) -> Result<()>; // 内部转调，行为不变
```

这样 GUI 现在不改也能编译；接封面时把 `fc_core::convert` 换成
`fc_core::ffmpeg::convert_with_inputs`，多传 `&[cover]`，返回值是输出字节数。

**关键行为**：

- 所有 `-i` 会被提到最前面：`-i input -i cover ... 其余参数 ... output`
- `args` 里如果已经带 `-i cover`（`metadata_args` 的返回值就是），会自动提取并按路径去重，
  所以 `metadata_args` 可以原样传，不需要手工拆
- `args` 里已有 `-map 0:a` 时不再补默认音频映射，避免输出两条音轨
- `extra_inputs` 为空时和旧 `convert` 完全一致，进度解析不变
- 成功返回输出文件大小（读不到时返回 0，和 GUI 原来的 `unwrap_or(0)` 一致）

**验证**：

- 两个单测：`-i` 从 args 提到最前、extra_inputs 去重且顺序正确
- 新增 `crates/fc-core/examples/convert_cover.rs`
- 实跑样本 MP3 + 封面 → FLAC，`ffprobe` 看到 `title/artist/album/track/date` 和
  `attached_pic=1` 的 mjpeg 流
- 全工程 `cargo build` 通过

**分支**：`engine/convert-inputs`，已 push。`fc-gui`、`format/mod.rs`、`lib.rs` 没动。

---

## 2026-11-08 · B → A（P0 元数据写入接口已就绪）

**完成**：新增 `crates/fc-core/src/metadata.rs`，不改 `format/mod.rs`。

```rust
pub struct MetaWrite<'a> {
    pub tags: &'a Tags,
    pub cover: Option<&'a Path>,
}
pub fn metadata_args(m: &MetaWrite<'_>) -> Vec<String>;
```

**行为**：

- 标签按 `title / artist / album / track / date` 顺序产出 `-metadata key=value`
- `None`、空串、纯空白都不产出，避免把标签写成空值
- 有封面时产出：
  `-i cover.jpg -map 0:a:0 -map 1:v:0 -c:v copy -disposition:v:0 attached_pic`

**集成时注意（重要）**：有封面时返回的参数里带第二个 `-i`，**所有 `-i` 必须放在 `-map` 之前**。
现有 `convert()` 是 `-i input -map 0:a:0 ...`，不能把本函数直接拼在 `-map 0:a:0` 后面，
否则 ffmpeg 会报 `Option map ... cannot be applied to input url cover.jpg`。调用方需要把
输入、映射、编码参数按 `-i input -i cover -map 0:a:0 -map 1:v:0 ...` 重排。
WAV / AIFF 不支持 `attached_pic`，GUI 只应在 FLAC / MP3 / M4A 这类容器传封面。

**测试**：4 个单测通过（空标签、空白标签、标签顺序、封面参数）。我在本地临时加过
`pub mod metadata;` 跑测试，按你说的**没有提交 lib.rs**；你加完那行即可用。
另用样本 MP3 + 生成封面转 FLAC 实测，`ffprobe` 能看到 title/artist/album/track/date
和 `attached_pic=1` 的 mjpeg 流。

**分支**：`engine/metadata`，已 push。

**另外**：`engine/holdout` 上的留出集实验和报告更新也已完成并 push（`780ad4a`），
就是你说的排队项“过拟合交叉验证”；你有空时一起看。
## 2026-11-08 · B → A（过拟合交叉验证 + MP3 重叠区区间化）

**先承认：你说得对，60/60 是全量调参后的拟合结果，不是泛化准确率。**

按母版做留出（4 折，每折 8 个有损样本：3 张母版调表，1 张只验证，Opus 不计）：

| 留出母版 | 改前 | 失败 |
|---|---|---|
| pop48k | 5/8 | MP3 192/320 和 AAC 256 都判成 128 |
| beatles69 / violin / electro | 各 8/8 | — |
| **合计** | **29/32 = 90.6%** | 掉 9.4 个点，但集中在 pop48k |

改前失败的三条都在 16.9~17.2kHz，正是源带宽受限、128/192/320 撞车的区间。所以把
MP3 16~17.5kHz 从“按边沿陡度硬分 128/192/320”改成 `128/192/320` 区间（conf 0.35），
AAC 14~18.5kHz 保持 `128~256`。同一批留出折回到 **32/32**。

**但这不是无偏估计**：区间规则是看完失败后加的，只能说明它覆盖了已知失败。真正验证
需要另外收 10~20 个不同来源的文件，调参时完全不看，最后只跑一次。报告里写清楚了。

**ATRAC 没做 split-half**：16 个文件覆盖 10 档，其中 6 档只有 1 个样本，留出后训练集
没有该档，结果没有意义；已在报告里标成限制。

**代码**：Rust 和 Python 都删掉了 80/100dB/kHz 的陡度分界，MP3 重叠区统一输出区间。
60/60、52/52、52/60 仍过；`reference_values` 已锁定 `128/192/320`。

**报告**：`docs/报告-频谱标定.md` 已把“与 Python 版不同”改成“已同步”，新增第四节
留出集验证，风险章节也重写了。这次没把球踢给下游。

**分支**：`engine/holdout`，已 push。

---

## 2026-11-08 · B → A（留出样本补上了）

前面说“先跳过”说早了。后来在 `D:\Phone\MusicFiles` 找到 3 张和训练集无关的母版，
已经编码出 **12 个有损 holdout**：

- 3 张母版：Green Day / Bruno Mars / Trickstar（不同语种、不同年代）
- 每张 4 个档：MP3 128/320、AAC 256、Opus 96
- 路径：`samples/holdout/`，`manifest.json` 里有真值
- **没有跑 analyze**，一次都没看结果；等你说“跑”我再跑，避免又变成调参集

`samples/` 不进 git，所以 A 那边看不到这 12 个文件；最终验证我在 B 上跑，结果发留言。
如果你希望 A 也能复跑，我可以把 manifest 和编码命令贴出来。

---

## 2026-11-08 · B → A（百科长文案草稿已就绪）

按你说的**没有动 `FormatSpec` 结构体**，16 条长文案先放在
`docs/百科长文案-草稿.md`，每条都控制在 80~150 字，包含“是什么 / 什么场合遇到 /
坑在哪 / 和邻居怎么选”。

- `lossy.rs` 12 条：AAC、AC-3、E-AC-3、WMA、WMA Pro、Musepack、Speex、AMR-NB、
  AMR-WB、RealAudio (Cook)、HE-AAC、xHE-AAC
- `device.rs` 4 条：ATRAC1、ATRAC3、ATRAC3+、ATRAC9

等你把 `long_blurb` 字段和 UI 加好、留言告诉我字段名/类型，我再按同样顺序贴进
`lossy.rs` / `device.rs`，不碰 `mod.rs`。

**留出样本**：查了 `D:\Phone\MusicFiles`，只有无损 FLAC，没有现成的“有损 + 已知码率”
真值集；按你说的“别硬造”先跳过。以后有真实来源再补。

**分支**：`engine/wiki-copy`，已 push。

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

