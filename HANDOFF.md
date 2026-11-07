# HANDOFF —— 新会话从这里开始

> 你是这个项目的**机器 B**。这份文件让你 30 秒内接上全部上下文。
> 不需要读任何聊天记录，所有决定都在这份和它指向的文档里。

---

## 一句话

**格式转换器**：不是"转换工具"，是"格式顾问"。
转换只是入口，判断（这文件真实音质多少）+ 解释（这格式是什么、有什么坑）才是产品价值。

Rust + egui，FFmpeg 子进程，cargo workspace。

---

## 当前状态（2026-11-08）

| 模块 | 状态 |
|---|---|
| `fc-core`（格式表 / probe / convert） | ✅ 可用 |
| `fc-player`（试听） | ✅ 可用 |
| `fc-gui`（三栏界面） | ✅ 可用，主机在持续改 |
| **`fc-analyze`（频谱分析）** | **⏳ 空壳，等你实现** |

---

## 你的边界

| 路径 | 能不能改 |
|---|---|
| `crates/fc-analyze/**` | ✅ 你的主战场 |
| `crates/fc-core/src/format/lossy.rs` | ✅ 可以（你的） |
| `crates/fc-core/src/format/mod.rs` | ⚠️ 改之前先留言 |
| `crates/fc-core/src/format/lossless.rs` | ❌ 主机的 |
| `crates/fc-core/src/ffmpeg.rs` | ✅ 可以 |
| `crates/fc-gui/**` | ❌ **一行都别碰** |
| `crates/fc-player/**` | ❌ 主机在管 |
| `docs/契约.md` | ⚠️ 改之前先留言 |

完整版见 `docs/契约.md`。

---

## 现在做什么

**读 `docs/任务-01-频谱分析.md`，照它做。**

一句话：实现 `crates/fc-analyze`，把 Python 参考实现 `tools/analyze.py` 移植成 Rust，
在样本集上做到档位命中率 ≥ 85%。

---

## 怎么汇报

不要指望能跟主机实时对话——两台机器之间**用 git 留言**：

```
docs/留言/from-B.md    ← 你写，主机读
docs/留言/from-A.md    ← 主机写，你读
```

规则：

1. **追加**，不要覆盖（别人的留言留着）
2. 每次做完一块就写一条：做了什么 / 遇到了什么 / 需要主机配合什么
3. `git push origin engine/xxx` 之后写留言，主机下次拉取就能看到

主机那边会定期 `git pull` 查留言。

---

## 铁律

1. ❌ 不碰 `fc-gui`
2. ❌ 不直接 push `main`，走 `engine/*` 分支
3. ❌ 不解密任何 DRM（这是产品的法律红线，见 `docs/法律边界-重新评估.md`）
4. ✅ 改完先 `cargo build` 通过再提交
5. ✅ 拿不准就在留言里问，别猜

---

## 必读的几份文档

| 文档 | 什么时候读 |
|---|---|
| `docs/机器B-接入指南.md` | **第一次来，先读这个**（装环境、clone、构建） |
| `docs/任务-01-频谱分析.md` | 你的第一个任务 |
| `docs/契约.md` | 你能改什么、API 长什么样 |
| `docs/样本清单.md` | 频谱算法的实测标定数据 |
| `docs/音频格式总览.md` | 209 种格式的分级清单 |

---

## 主机最近定了什么（免得你白干）

- 格式表**已经拆好了**：`format/lossless.rs`（主机的）/ `format/lossy.rs`（你的）/ `format/device.rs`（都不动）
  → `FORMATS` 静态量改成了 `all()` 函数，`FormatSpec` 多了个 `primary` 字段，详见 `docs/契约.md`
- 侧栏不显示"能解码但不能编码"的格式（AAC / AC-3 / ATRAC9），它们只在百科和解释里出现
- **这些对你的 `fc-analyze` 没有影响**，先把频谱做出来
