# ToneBox（格式转换器）

一个"格式顾问"，不是格式转换工具。转换只是入口，**判断 + 解释**才是产品价值。
顺带能管理音乐库、试听、记住你几套常用配置。

Rust + egui，FFmpeg 子进程，cargo workspace 四个 crate（`fc-core` / `fc-analyze` / `fc-player` / `fc-gui`）。\n\n> **测试版 beta 1.0** · 建议所有操作备份 · 慎防设备损坏，请自己负责\n

---

## 当前能力

| 能力 | 状态 |
|---|---|
| 多文件拖拽 / 队列 | ✅ |
| 视频容器输入（MKV / MP4 / MOV / AVI / WEBM…，第一条画面 + 全部音轨） | ✅ |
| 音乐库（歌曲 / 专辑 / 风格 / 艺术家 / 自选文件夹，只读扫描） | ✅ |
| 用户配置（多套切换 / 重命名 / 导出导入）+ 用户自设预设 | ✅ |
| 播放栏频谱（按钮 / F2） | ✅ |
| 可拉伸播放栏（拖顶部改高度，波形跟着变；高度记配置） | ✅ |
| 全局输出格式 + 单文件覆盖 | ✅ |
| 输出格式侧栏（点开显示该格式的解释） | ✅ |
| ffprobe 读真实参数 | ✅ |
| 有损源警告（转无损不会增加信息量） | ✅ |
| 转 FLAC / WAV / ALAC / AIFF / WavPack / TTA / CAF / W64 / 裸 PCM | ✅ |
| 转 MP3 / Opus / Vorbis / MP2 / AAC / AC-3 / E-AC-3 / WMA | ✅ |
| 转 ATRAC1 / ATRAC3 / ATRAC3+（需自装 [atracdenc](https://github.com/dcherednik/atracdenc)，不随包） | ✅ |
| 码率、压缩等级、位深可调（档位表在 core，不在界面） | ✅ |
| 采样率可调（自动 / 44.1k / 48k / 96k / 192k，按格式给档位） | ✅ |
| 响度归一化（EBU R128）与预览（源/目标包络、峰值余量、限幅提示） | ✅ |
| 转换前的质量风险提示（降码率、无损转有损、升采样、抄大容器） | ✅ |
| 视频容器输出（MKV / MP4 / WEBM / AVI；直接复制或转 H.264/H.265/VP9/AV1；多音轨逐条保留/重编） | ✅ |
| 格式百科（33 条长文案，可从百科直接设为输出格式） | ✅ |
| 元数据与内嵌封面 | ✅ |
| 试听（ffmpeg 解码 → 声卡，支持 ATRAC/OMA） | ✅ |
| 音频设备检测（输出/输入、耳机/蓝牙/HDMI/USB，`引擎和设备`页，手绘图标+厂商标记） | ✅ |
| 播放输出设备选择、WASAPI 独占直通（按源采样率） | ✅ |
| 播放时 ReplayGain（读 gain 标签） | ✅ |
| 加密文件识别 + 解释（不解密） | ✅ |
| 频谱质量评估（标定集 60/60，ATRAC 结论偏保守；DSD 不做有损判定） | ✅ |
| 命令行 `--convert` | ✅ |

---

## 仓库结构

```
crates/
├── fc-core/      格式表 + probe + convert + 标签/元数据/百科   【引擎】
├── fc-analyze/   频谱分析（标定 60/60）                      【引擎】
├── fc-player/    试听（ffmpeg + cpal）                       【本机管】
└── fc-gui/       四栏界面 + 队列状态机                        【界面，本机管】
docs/
├── 契约.md               并行开发的归属表与冻结 API
├── 机器B-接入指南.md      另一台机器怎么接进来
├── 样本清单.md            44 个对照样本 + 实测标定数据
├── 音频格式总览.md        209 种格式的分级清单
└── 法律边界-重新评估.md   风险地图
tools/           样本下载、标定、截图脚本
```

---

## 一条设计红线

**遇到加密文件，只识别、只解释、不解密。**

代码里不允许出现任何 OpenMG / MagicGate / NCM / QMC 相关的解密逻辑。

---

## 布局规范（改 UI 前先读这一节）

踩过的坑：右栏文字被窗口边缘切掉、队列行撑破中央面板。原因是**没有锁宽度**。

规则：

1. **面板宽度是常量**：`LEFT_W = 216`、`RIGHT_W = 300`，不要写魔法数字。
2. **每个面板开头 `ui.set_width(ui.available_width())`**，把内容宽度锁死。
3. **长文本必须显式 wrap**：用 `para()` 助手（`TextWrapMode::Wrap`）。
4. **单行文本必须显式截断**：文件名用 `TextWrapMode::Truncate`，
   并给左栏留出右侧控件宽度（`row_w - 168.0`）。
5. **不要在 `Frame` 内部再 `set_width`**——会和 `inner_margin` 叠加，反向溢出。
6. 详情面板用**键值栅格**而不是一排 chip，chip 多了必然溢出。

窗口默认 1302×700，最小 1212×600（四栏：216 格式 + 中央队列 + 300 详情 + 272 输出）。
`RIGHT_W` / `OUT_W` 等宽度都是 `main.rs` 顶部的常量，改布局先改那几个数。

---

## 构建

本机没有 MSVC，用的是 **gnu 工具链 + WinLibs binutils**。

```powershell
# 一次性准备
rustup toolchain install stable-x86_64-pc-windows-gnu
winget install --id BrechtSanders.WinLibs.POSIX.UCRT -e

# 平时构建
.\build.ps1           # debug
.\build.ps1 release   # release → target\release\format-converter.exe
```

## 用法

```powershell
format-converter.exe                      # 空白启动
format-converter.exe a.mp3 b.oma          # 直接载入多个文件
format-converter.exe --convert in.mp3 out.flac
format-converter.exe --play in.oma 5      # 试听 5 秒，验证声卡链路
```

ffmpeg 查找顺序：程序同目录 → 同目录 `ffmpeg\` → WinGet Links → `C:\ffmpeg\bin` → PATH。

---

## 已知问题

1. **半透明在本机渲染为不透明**
   `DWMWA_SYSTEMBACKDROP_TYPE`、`ACCENT_ENABLE_ACRYLICBLURBEHIND`、
   `DwmExtendFrameIntoClientArea(-1)` 三者任意一个都会覆盖 winit 的 per-pixel alpha。
   当前策略：只设深色标题栏，其余全部禁用。要毛玻璃得换 Tauri + WebView2。

2. **eframe 0.29 的 glow 后端对 `from_rgba_unmultiplied` 会二次 gamma**
   alpha=10 实测渲染成 55% 白。所有半透明色必须走 `from_rgba_premultiplied`，见 `tint()`。

3. **ffmpeg 9.0.1 + 24bit FLAC + `-ss`/滤镜 → `invalid block size: 8`**
   解法：两段式，先截成 WAV 再从 WAV 编码。

4. **PowerShell 5.1 读无 BOM 的 UTF-8 脚本会当 ANSI**，中文乱码导致解析失败。
   仓库里的 `.ps1` 一律带 BOM。
