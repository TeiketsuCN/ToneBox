# 声箱 ToneBox —— 应用总结

> 一句话：一个"先讲清楚代价、再让你转"的 Windows 音频格式转换器。
> 转换只是入口，它真正干的事是**把格式知识摊开给你看**。

- 文档版本：对应应用 **1.4.0**（VOC / AAC-LATM / MXF / Musepack 已随 1.4 发布）
- 整理时间：2026-11
- 仓库：<https://github.com/TeiketsuCN/ToneBox>

---

## 1. 它是什么

| 项目 | 内容 |
|---|---|
| 形态 | Windows 10/11 x64 桌面程序，解压即用（免安装） |
| 技术栈 | Rust + egui/eframe 0.29（glow 后端）、winit、ffmpeg 子进程 |
| 代码结构 | cargo workspace，4 个 crate：`fc-core` / `fc-analyze` / `fc-player` / `fc-gui` |
| 许可 | 程序本体 MIT；随包 ffmpeg 为 LGPL-2.1-or-later（不含 GPL 组件） |
| 体积 | 主包 28.0 MB（含 ffmpeg 59.6 MB 解压后）；可选工具包 0.15 MB |
| 当前能力 | **73 个可输出格式**、47 个只读参考格式（合计 120 条）、14 个视频容器 |

它**不是**：不是批量压缩器、不联网、不解 DRM、不做流媒体下载、不改你的源文件（除非你明确勾选删除）。

---

## 2. 设计理念（和普通转换器的区别）

1. **先说代价，再动手**
   转之前就告诉你会损失什么："降采样会切掉 13.4 kHz 以上"、"16bit 源转 24bit 只是补零，文件会变大"、"有损源转无损不会变好"。这些提示不是弹窗恐吓，而是挂在参数旁边的一行小字。

2. **不假装**
   - 没有编码器的格式：灰显 + 点开解释"为什么不能转成它"
   - 表里能编、但本机 ffmpeg 缺库（比如没编译进 libgsm 的构建）：徽章也会变灰
   - 颜色 = **此刻真能不能输出**，不是"理论上支持"

3. **参数只给真的有意义的**
   每个格式只显示它真正支持的旋钮（码率 / 位深 / 采样率 / 声道 / 压缩等级 / 质量档）；固定码率的编解码器（aptX、SBC、ADX…）直接写"这个格式没有可调参数"。

4. **只读参考格式常驻**
   APE、TAK、DSD、DTS-HD、MPEG-H、xHE-AAC 等 47 个格式能识别、能解释，但不提供编码器——它们在侧栏灰着，点开就有说明。

---

## 3. 功能清单

### 3.1 转换

- 多文件拖拽进队列，统一目标格式；单个文件可在队列里单独覆盖
- 三种执行路径：
  1. **ffmpeg 一把梭**（绝大多数格式）
  2. **外部编码器**（ATRAC / Speex / WavPack 有损 / OptimFROG / Musepack）
  3. **纯容器直通**（AC-4、IEC 61937 只能复制，源不对会明确报错）
- 视频容器：保留**第一条画面 + 全部音轨**，画面可复制或转 H.264/H.265/VP9/AV1/MPEG-2/MPEG-4/Theora/WMV2，音轨逐条决定复制还是重编
- 响度归一化（LUFS）、采样率/声道/位深转换、元数据与歌词 sidecar
- 失败原因逐条显示（不是"转换失败"四个字）

### 3.2 格式百科

点侧栏任何格式，下面展开它的说明：专利状态、典型码率、适合谁、有什么坑。覆盖全部 120 个条目（73 可输出 + 47 只读）。

### 3.3 音乐库

只读扫描本地文件夹 → 歌曲/专辑/风格/艺术家/文件四种视图，不写标签、不动文件。

### 3.4 播放与分析

- 内置播放器（rodio/cpal），播放列表跟随队列
- 频谱图、波形、响度（EBU R128）分析；长文件只抽样 60 秒，分析进程降优先级

### 3.5 预设与配置

- 内置预设：MD 归档 / 车载 / 手机 / 再编辑
- 用户自设预设；多套配置切换、重命名、导出导入
- 设置页「依赖 / 外部工具」：ffmpeg（版本 + 编码器数）、ffprobe、atracdenc、speexenc、wavpack、ofr、mpcenc 逐项显示路径与状态，支持"重新检测"

### 3.6 界面

- 三栏：左（格式列表）/ 中（队列）/ 右（选中文件详情 + 输出参数）
- 启动过场：15 项真实检查逐条显示，然后"正在加载项目…"
- 默认全屏，F11 / Esc 回到 1656×912 窗口
- 自定义标题栏、壁纸毛玻璃、格式徽章按扩展名与可用性着色

### 3.7 命令行（同目录 exe）

```
format-converter.exe --version                  显示版本（1.4.1 起）
format-converter.exe --help                     列出全部命令行参数
format-converter.exe --formats                  列出所有可输出格式
format-converter.exe --convert in.flac out.mpc --to musepack
format-converter.exe --tools                    探测 ffmpeg / atracdenc / speexenc / wavpack / ofr / mpcenc
format-converter.exe --devices                  列出音频设备
format-converter.exe --play <file>              直接播放
format-converter.exe --health <file>            内容体检
```

---

## 4. 格式能力

### 4.1 可输出（73 个，按组）

| 组 | 数量 | 格式 |
|---|---|---|
| **无损** | 13 | FLAC、WAV、ALAC、AIFF、WavPack、TTA、CAF、W64、裸 PCM、TrueHD、MLP、Ogg FLAC、OptimFROG* |
| **有损** | 13 | MP3、Opus、Vorbis、MP2、AAC、WMA v1/v2、Musepack*、aptX、aptX HD、SBC、ADX、WavPack 有损*、DFPWM |
| **语音/电话** | 19 | G.711 A/μ、G.722、G.723.1、G.726、GSM 06.10†、AMR-NB、AMR-WB†、iLBC†、LC3†、Codec2†、Speex*、ADPCM 系列（MS/IMA/Yamaha/QuickTime）、Nellymoser、RealAudio 1.0 |
| **空间音频** | 5 | AC-3、E-AC-3、AC-4（直通）、DTS、AAC 5.1 |
| **音频容器** | 6 | WAV/AU/SND、RF64、BWF、IEC 61937、**VOC**、**AAC-LATM** |
| **视频容器** | 14 | MKV、MP4、WebM、MOV、TS、M2TS、FLV、AVI、MPEG-PS、VOB、OGV、WMV、3GP、**MXF** |
| **设备专用** | 3 | ATRAC1、ATRAC3、ATRAC3+* |
| **合计** | **73** | |

`*` = 需要外部工具（见第 5 节）；`†` = 需要 ffmpeg 构建里带对应库（随包 LGPL 构建不包含，完整构建如 gyan 有）

### 4.2 只读参考（47 个）

APE、TAK、OptimFROG 旧条目、Shorten、WMA Lossless/Pro/Voice、DSD（DSF/DFF/DST）、DTS-HD/HR/X、MPEG-H 3D、MPEG Surround、USAC/xHE-AAC、LC3plus、EVS、TwinVQ、MPEG-4 HILN/SSC、RealAudio 2.0/Cook、QCP、G.729、Siren 7/14、G.719、DSDIFF、MXF 旧条目等。

原因分三类：**没有开源编码器**（专利/闭源）、**ffmpeg 只做解码**、**容器/规范写不出来**（如 AIFC、BW64）。

---

## 5. 外部工具与许可

| 工具 | 是否随包 | 许可 | 用途 |
|---|---|---|---|
| ffmpeg / ffprobe 8.0 | ✅ 主包 | LGPL-2.1-or-later | 解码、编码、探测（207 个编码器） |
| speexenc | ✅ 主包 | BSD-3-Clause | Speex 输出 |
| mpcenc | ✅ 主包 | LGPL-2.1 | Musepack 输出 |
| wavpack.exe | ✅ 主包 | BSD-3-Clause | WavPack **有损**（hybrid）输出 |
| atracdenc | 📦 extras 包 | LGPL-2.1 | ATRAC1/3/3+ 输出 |
| ofr.exe（OptimFROG） | ⬇️ extras 内一键下载 | 闭源免费（再分发需授权） | OptimFROG 输出 |

关于两个"不能随包"的：

- **atracdenc**：LGPL 可再分发，放在 `ToneBox-x.y.z-extras-win64.zip` 的 `atracdenc\` 里
- **OptimFROG**：官方 license 写明 *"distribution of command-line Encoders with other software may need permission"*，所以 extras 里放的是 `install-ofr.cmd` —— 双击后从官网下载、用官方 SHA-1 校验、放进 `optimfrog\`

程序查找顺序统一为：**程序目录 → 对应子目录（atracdenc\ / speex\ / wavpack\ / optimfrog\ / musepack\）→ PATH**。

---

## 6. 架构

```
crates/
  fc-core/    格式表与全部规则：FormatSpec、能力矩阵、参数、警告、
              ffmpeg/外部工具调用、配置、探测（纯逻辑，可单测）
  fc-analyze/ 频谱、波形、响度分析（调 ffmpeg，解析输出）
  fc-player/  播放引擎（rodio/cpal）
  fc-gui/     egui 界面：三栏、过场、播放器面板、设置、百科
tools/        抓取依赖、构建 ffmpeg、打包、安装脚本
docs/         设计笔记、格式总览、截图
```

### 数据流

```
拖入文件
  → ffprobe 探测（编码/采样率/位深/声道/时长/音轨）
  → 质量警告计算（有损↔无损、采样率升降、位深升降、容器兼容）
  → 参数面板（按 knobs_for 生成对应旋钮）
  → 生成任务（Ffmpeg / Atrac / Speex / Wavpack / Ofr / Mpc 之一）
  → 后台线程执行，mpsc 回报进度
  → 写文件 + 通知（成功/失败原因）
```

### 关键表

| 表 | 作用 |
|---|---|
| `all()` | 全部格式（lossless→lossy→device→video→feasible→reference） |
| `encoder_for(id)` | 这个格式靠哪个编码器（缺了就灰） |
| `external_encoder(id)` | 要哪个外部工具（atracdenc / wavpack / ofr / mpcenc） |
| `video_codecs_for(container)` | 容器收哪些画面编码 |
| `copy_video_ok` / `audio_copy_ok` | 能不能直接复制流 |
| `audio_plan(container, codec)` | 容器默认音频编码 |
| `bitrate_choices` / `rate_choices` / `bit_depths_for` | 各旋钮的档位 |
| `knobs_for` | 该格式显示哪些旋钮 |
| `ext_choices` | 多后缀格式的后缀选项 |

---

## 7. 踩过的坑（都写进代码注释了）

1. **eframe 0.29 glow 后端对 `from_rgba_unmultiplied` 会二次 gamma**
   → 半透明一律走 `from_rgba_premultiplied`（见 `tint()`）
2. **FLAC + 24bit + 截取/滤镜 → `invalid block size`**
   → 两段式：先截成 WAV 再从 WAV 编码
3. **PowerShell 5.1 读无 BOM 的 UTF-8 脚本会当 ANSI**
   → 仓库所有 `.ps1` 一律带 BOM（编辑后必须补回）
4. **`Invoke-RestMethod -Body <字符串>` 按 ASCII 编码**
   → 发中文 JSON 必须先 `UTF8.GetBytes()`，否则发布说明全是 `?`
5. **ffmpeg 命令行位置很讲究**
   → `wavpack -b` 必须连写 `-b384`；`au` 写 `.snd` 要显式 `-f au`；`ofr --encode … --output …`
6. **视频容器的音频不是"能 copy 就 copy"**
   → MXF 只接受 48kHz（源 44.1k 必须重编码）；PCM 目标不能写 `-b:a`
7. **`output_rate` 对视频容器会提前返回**
   → MXF 的 48k 强制要单独处理
8. **徽章颜色只看 `can_encode` 会骗人**
   → 改成 `!can_encode || !avail`，跟随运行时可用性
9. **Git 提交前没注意 `build/`**
   → 曾把 ffmpeg 源码树（15052 个文件）提交进去，已回退并加 `.gitignore`

---

## 8. 构建与发布

```powershell
# 抓依赖
powershell -File tools\fetch-ffmpeg.ps1
powershell -File tools\fetch-speex.ps1
powershell -File tools\fetch-wavpack.ps1
powershell -File tools\fetch-atracdenc.ps1     # 可选
powershell -File tools\fetch-optimfrog.ps1     # 可选

# 编译与测试
cargo build --release -p fc-gui
cargo test -p fc-core --lib                    # 71 项

# 打包
powershell -File tools\package-release.ps1     # 主包
powershell -File tools\package-extras.ps1      # 可选工具包
```

自行构建随包 ffmpeg（带语音编码库）：`tools/build-ffmpeg.ps1`，约 20~40 分钟，
产物同为 LGPL v3，含 libgsm / vo-amrwbenc / libilbc / liblc3 / codec2 / libaom 等。

### 发布记录

| 版本 | 日期 | 要点 | 产物 |
|---|---|---|---|
| 1.0.0-beta.1 | 2026-11 | 首个公开测试版 | 主包 |
| 1.2.0-beta.1 | 2026-11 | 格式扩容、侧栏七组 | 主包 |
| **1.3.0** | 2026-11 | 视频容器 8→13、GSM/AMR-WB/iLBC/LC3/Codec2、自建 ffmpeg 8.0 | 27.8 MB |
| **1.3.1** | 2026-11 | OptimFROG、CLI 外部编码器修复、extras 工具包 | 27.9 MB + 0.15 MB |
| **1.4.0** | 2026-11 | VOC / AAC-LATM / MXF / Musepack（69→73）、musepack 随包 | 28.0 MB + 0.15 MB |

---

## 9. 质量保证

- **71 项单元测试**（格式表完整性、参数映射、警告逻辑、视频容器矩阵、后缀表）
- **真实编码验证**：每加一个格式都用随包 ffmpeg/外部工具实际压一段音频，比对
  `ffprobe` 输出的编码、采样率、声道；视频容器还会验证画面/音轨编码
- **发布会验三次哈希**：本地文件、GitHub 返回的 digest、重新下载后的文件
- 已知限制：DSD 输出、HE-AAC/ELD、APE/TAK 编码、AIFC/BW64 写入均不可行（原因见第 4.2 节）

---

## 10. 1.4 新增内容（已发布）

1.4.0 新增并实测、随包发布的部分：

| 新增 | 类型 | 实测结果 |
|---|---|---|
| **VOC**（Creative Voice） | 音频容器 | 529 KB PCM |
| **AAC (LATM/LOAS)** | 广播封装 | 49 KB |
| **MXF** | 视频容器（MPEG-2 + PCM 48k） | 528 KB |
| **Musepack**（mpcenc，LGPL，随包） | 有损格式 | 15.7 KB（5 档质量） |

加上这四个，可输出格式从 **69 → 73**。

---

## 11. 文件索引

| 路径 | 内容 |
|---|---|
| `README.md` | 面向用户的下载与使用说明 |
| `THIRD_PARTY_NOTICES.md` | 第三方组件许可与来源 |
| `docs/screenshot.jpg` | 界面截图 |
| `docs/音频格式总览.md` | 格式知识库（长文） |
| `docs/法律边界-重新评估.md` | 版权/DRM 边界的判断依据 |
| `docs/契约.md` | 模块间接口约定 |
| `docs/FFmpeg-LGPL.md` | 随包 ffmpeg 的合规说明 |
| `tools/build-ffmpeg.sh` | 自建带语音编码库的 ffmpeg |
| `tools/package-release.ps1` / `package-extras.ps1` | 两种发布包 |
| `crates/fc-core/src/format/` | 格式表与全部规则（本项目的核心） |
