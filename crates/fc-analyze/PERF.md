# fc-analyze 长文件性能报告

> 2026-11-08 · 机器 B · 分支 `engine/perf`
> 测试对象：`samples/lossless/pop48k_ref.flac` 循环拼成的 30 分钟 / 60 分钟 FLAC
> 构建：`cargo build --release -p fc-analyze --example analyze_file`
> 环境：Windows 11，rustc 1.99.1 GNU，FFmpeg 9.0.2，SSD

---

## 一、结论

| 文件 | 改造前 analyze | 改造后 analyze | 改造前峰值工作集* | 改造后峰值工作集* |
|---|---:|---:|---:|---:|
| 30 分钟 | 2.096s | **0.342s** | — | — |
| 60 分钟 | 6.084s | **0.345s** | **1655.1MB** | **21.4MB** |

\* 峰值工作集是父进程 `analyze_file.exe` 的 `WorkingSet64`，40ms 采一次；不含 ffmpeg 子进程的短暂峰值。

改造前 1 小时已经超过 3 秒，且要把整段解码结果读进内存；改造后时间基本与文件长度无关，内存从 GB 级降到 20MB 级。
30 分钟文件的频谱结果只变了 12Hz（16805 → 16793Hz），estimate 仍是“无损/CD”，置信度 0.92。

---

## 二、改造前的问题

`decode_mono()` 用 `ffmpeg ... -f f32le -` 一次性 `Command::output()`，把整段音频读进 `Vec<f32>`：

- 单声道 44.1kHz f32：1 小时约 **635MB**，30 分钟约 **317MB**
- `Command::output()` 还要额外持有 ffmpeg 的 stdout 缓冲，峰值接近 2 倍
- 虽然 `mean_spectrum()` 只取最多 4000 帧，但解码和缓冲仍然是全曲

1 小时实测 analyze 6.084s，峰值 1655MB；这还只是单声道，长播客/音乐会录音会明显更糟。

---

## 三、改造方案

`FULL_DECODE_LIMIT_S = 600.0`：

- **≤ 10 分钟**：整段解码，保持和原来完全一致的结果。
- **> 10 分钟**：取 **3 个 30 秒窗口**，共 90 秒：
  - 窗口起点分别是可播放范围的 10% / 50% / 90%
  - 不是只取开头，避免前奏是纯人声/静音导致采样偏
  - 每个窗口单独 `ffmpeg -ss <start> -t 30` 解码，再拼接
  - 内存上限约 90s × 44.1kHz × 4B ≈ **16MB**，加上进程开销约 21MB

窗口函数 `sample_windows(duration_s)` 有单元测试，保证三个窗口不越界、顺序递增。

为什么是 600 秒而不是 90 秒：标定集里的 `aa3_sonateno14op27-2.aa3` 时长 328 秒，
它的 19kHz 边沿不在任意 30 秒窗口里；如果阈值设成 180 秒，这个样本会被采样漏掉，
命中率从 60/60 掉到 59/60。600 秒让 5~6 分钟的曲目仍然整段分析，长播客才采样。

---

## 四、回归结果

`cargo test -p fc-analyze -- --nocapture`：

```
产品口径: 60/60 = 100.0%
非 Opus: 52/52 = 100.0%
最保守口径（Opus 按码率硬算）: 52/60 = 86.7%
```

`reference_values` 7 个样本仍与 Python 参考实现一致；HE-AAC profile 单测和窗口单测都通过。

---

## 五、复现

```powershell
# 30 分钟 / 60 分钟测试文件
ffmpeg -y -v error -stream_loop 59 -i samples\lossless\pop48k_ref.flac `
  -t 1800 -c:a flac -compression_level 0 out\long30.flac
ffmpeg -y -v error -stream_loop 119 -i samples\lossless\pop48k_ref.flac `
  -t 3600 -c:a flac -compression_level 0 out\long60.flac

cargo run --release -p fc-analyze --example analyze_file -- out\long30.flac
cargo run --release -p fc-analyze --example analyze_file -- out\long60.flac
```

---

## 六、剩余风险

- 长文件只采样 90 秒；如果整首歌的有效频谱特征只出现在未被采到的区间，仍可能误判。
  对“整轨编码参数一致”的真实文件风险低，因为编码器低通是全曲统一的。
- 600 秒以内仍然整段解码，10 分钟文件峰值约 100MB；这是有意的精度/内存折中。
- 当前测试文件是同一段音频循环，窗口之间没有内容差异；下一步最好拿真实 1 小时播客
  再跑一次，确认采样窗口和全曲结论一致。
