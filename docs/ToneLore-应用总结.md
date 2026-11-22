# 声识 ToneLore —— 应用总结

> 一句话：一个"先讲清楚代价、再让你转"的 Windows 音频格式转换器。
> 转换只是入口，它真正干的事是**把格式知识摊开给你看**。

- 文档版本：对应应用 **2.0.0**（112 个可输出；English UI + 新格式/输入）
- 整理时间：2026-11
- 仓库：<https://github.com/TeiketsuCN/ToneLore>

---

## 1. 它是什么

| 项目 | 内容 |
|---|---|
| 形态 | Windows 10/11 x64 桌面程序，解压即用（免安装） |
| 技术栈 | Rust + egui/eframe 0.29（glow 后端）、winit、ffmpeg 子进程 |
| 代码结构 | cargo workspace，4 个 crate：`fc-core` / `fc-analyze` / `fc-player` / `fc-gui` |
| 许可 | 程序本体 MIT；随包 ffmpeg 为 LGPL-2.1-or-later（不含 GPL 组件） |
| 体积 | 主包 28.0 MB（含 ffmpeg 59.6 MB 解压后）；可选工具包 0.15 MB |
| 当前能力 | **112 个可输出格式**、47 个只读参考格式（合计 159 条）、36 个视频容器 |

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
  2. **外部编码器**（ATRAC / Speex / WavPack 有损 / OptimFROG / Musepack / PS1 XA / PS2 VAG / QOA / 任天堂容器）
  3. **纯容器直通**（AC-4、IEC 61937 只能复制，源不对会明确报错）
- **主机芯片音乐当输入**：NSF / SPC / GBS / VGM / PSF 等经随包 ffmpeg 的 libgme 渲染成 PCM
- **vgmstream 输入**：PSF / PS2 ADS / BRSTM / BCSTM / BFSTM / HCA 先解成 WAV 再走正常流程
- **模块音乐输入**：MOD / XM / IT / S3M 经 openmpt123 渲染成 WAV
  （默认 180 秒），再走正常转换流程
- 视频容器：保留**第一条画面 + 全部音轨**，画面可复制或转 H.264/H.265/VP9/AV1/MPEG-2/MPEG-4/Theora/WMV2/DV，音轨逐条决定复制还是重编
- 响度归一化（LUFS）、采样率/声道/位深转换、元数据与歌词 sidecar
- 失败原因逐条显示（不是"转换失败"四个字）

### 3.2 格式百科

点侧栏任何格式，下面展开它的说明：专利状态、典型码率、适合谁、有什么坑。覆盖全部 154 个条目（107 可输出 + 47 只读）。

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
ToneLore.exe --version                  显示版本（1.4.1 起）
ToneLore.exe --help                     列出全部命令行参数
ToneLore.exe --formats                  列出所有可输出格式
ToneLore.exe --convert in.flac out.mpc --to musepack
ToneLore.exe --tools                    探测 ffmpeg / atracdenc / speexenc / wavpack / ofr / mpcenc
ToneLore.exe --devices                  列出音频设备
ToneLore.exe --play <file>              直接播放
ToneLore.exe --health <file>            内容体检
```

---

## 4. 格式能力

### 4.1 可输出（112 个，按组）

| 组 | 数量 | 格式 |
|---|---|---|
| **无损** | 13 | FLAC、WAV、ALAC、AIFF、WavPack、TTA、CAF、W64、裸 PCM、TrueHD、MLP、Ogg FLAC、OptimFROG* |
| **有损** | 18 | MP3、Opus、Vorbis、MP2、AAC、WMA v1/v2、Musepack*、aptX、aptX HD、SBC、ADX、WavPack 有损*、DFPWM、Westwood AUD、APM、QOA、KVAG、**Argonaut ASF** |
| **语音/电话** | 20 | G.711 A/μ、G.722、G.723.1、G.726、GSM 06.10†、AMR-NB、AMR-WB†、iLBC†、LC3†、Codec2†、Speex*、ADPCM 系列（MS/IMA/Yamaha/QuickTime）、Nellymoser、RealAudio 1.0、**ALP ADPCM** |
| **空间音频** | 5 | AC-3、E-AC-3、AC-4（直通）、DTS、AAC 5.1 |
| **音频容器** | 9 | WAV/AU/SND、RF64、BWF、IEC 61937、VOC、AAC-LATM、**SMAF（MMF）**、**SoX**、**IRCAM** |
| **视频容器** | 36 | MKV、MP4、WebM、MOV、TS、M2TS、FLV、AVI、MPEG-PS、VOB、OGV、WMV、3GP、MXF、DV、GXF、NUT、MXF OPAtom、3G2、F4V、ISMV、WTV、**VCD**、**SVCD**、**PSP**、**iPod (M4V)**、**IVF**、**H.264 裸流**、**H.265 裸流**、**MPEG-2 裸流**、**MPEG-4 裸流**、AVIF、AMV、SWF、**GIF**、**APNG** |
| **设备专用** | 11 | ATRAC1、ATRAC3、ATRAC3+*、PS1 XA-ADPCM*、PS2 VAG*、**BRSTM、BCSTM、BFSTM、DSP、IDSP**（VGAudioCli）、**N64 VADPCM**（vadpcm） |
| **合计** | **112** | |

`*` = 需要外部工具（见第 5 节）；`†` = 需要 ffmpeg 构建里带对应库（随包 LGPL 构建不包含，完整构建如 gyan 有）

### 4.2 只读参考（47 个）

APE、TAK、OptimFROG 旧条目、Shorten、WMA Lossless/Pro/Voice、DSD（DSF/DFF/DST）、DTS-HD/HR/X、MPEG-H 3D、MPEG Surround、USAC/xHE-AAC、LC3plus、EVS、TwinVQ、MPEG-4 HILN/SSC、RealAudio 2.0/Cook、QCP、G.729、Siren 7/14、G.719、DSDIFF、MXF 旧条目等。

原因分三类：**没有开源编码器**（专利/闭源）、**ffmpeg 只做解码**、**容器/规范写不出来**（如 AIFC、BW64）。

主机芯片音乐（NSF / SPC / GBS / VGM / PSF 等）是第四类：它们不是波形音频，而是「芯片程序 + 采样」，
播放时才产生声音，无法从 WAV 转出来，详见 `docs/主机音频格式.md`。

---

## 5. 外部工具与许可

| 工具 | 是否随包 | 许可 | 用途 |
|---|---|---|---|
| ffmpeg / ffprobe 8.0 | ✅ 主包 | LGPL-2.1-or-later | 解码、编码、探测（207 个编码器；含 libgme） |
| speexenc | ✅ 主包 | BSD-3-Clause | Speex 输出 |
| mpcenc | ✅ 主包 | LGPL-2.1 | Musepack 输出 |
| wavpack.exe | ✅ 主包 | BSD-3-Clause | WavPack **有损**（hybrid）输出 |
| psxavenc | 📦 extras 包 | Zlib | PS1 XA / PS2 VAG 输出 |
| vgmstream | 📦 extras 包 | ISC | PSF / ADS / BRSTM / BCSTM / HCA 等输入 |
| openmpt123 | 📦 extras 包 | BSD-3-Clause | MOD / XM / IT / S3M 等输入 |
| VGAudioCli | 📦 extras 包 | MIT | BRSTM / BCSTM / BFSTM / DSP / IDSP 输出 |
| qoaconv | 📦 extras 包 | MIT | QOA 输出 |
| atracdenc | 📦 extras 包 | LGPL-2.1 | ATRAC1/3/3+ 输出 |
| ofr.exe（OptimFROG） | ⬇️ extras 内一键下载 | 闭源免费（再分发需授权） | OptimFROG 输出 |

关于两个"不能随包"的：

- **atracdenc**：LGPL 可再分发，放在 `ToneLore-x.y.z-extras-win64.zip` 的 `atracdenc\` 里
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
10. **libgme 需要 `zlib1.dll`，而 ldd 的 `lib*.dll` 过滤会漏掉它**
    → 构建脚本只收集 `lib` 前缀的依赖，漏了 `zlib1.dll`，ffmpeg 直接 0xC0000135 起不来；
    已在 `build-ffmpeg.sh` 里显式补拷
11. **芯片音乐没有时长，libgme 会一直渲染**
    → 一个 749 字节的 test.nsf 能渲出 3.4 小时；统一按 `CHIPTUNE_SECONDS = 180` 截断

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
cargo test -p fc-core --lib                    # 75 项

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
| **1.5.0** | 2026-11 | 73→95：主机音乐输入、老游戏语音、PS1/PS2、任天堂容器、8 个视频容器 | 28.2 MB + 4.74 MB |
| **1.6.0** | 2026-11 | 95→106：VCD/SVCD/PSP/iPod、裸流与 AVIF、KVAG | 28.2 MB + 4.74 MB |
| **1.8.0** | 2026-11 | 1.8 可靠性（测试矩阵/自检/无损校验/原子输出/冲突策略）+ 1.9 顾问（Tags/能力表/场景）+ 歌单 + 设置标签/关于 | 28.3 MB + 13.1 MB |
| **1.7.1** | 2026-11 | 修复：Metadata 顺序、输出摘要截断、封面行溢出 | 28.3 MB + 13.1 MB |
| **1.7.0** | 2026-11 | 107→112：ASF/AMV/SWF/GIF/APNG、ProRes/DNxHR/FFV1/VP8/MJPEG、vgmstream/openmpt、CUE、English UI | 28.3 MB + 13.1 MB |
| **1.6.1** | 2026-11 | 106→107：N64 VADPCM（vadpcm，MPL-2.0） | 28.2 MB + 4.8 MB |

---

## 9. 质量保证

- **75 项单元测试**（格式表完整性、参数映射、警告逻辑、视频容器矩阵、后缀表）
- **真实编码验证**：每加一个格式都用随包 ffmpeg/外部工具实际压一段音频，比对
  `ffprobe` 输出的编码、采样率、声道；视频容器还会验证画面/音轨编码
- **发布会验三次哈希**：本地文件、GitHub 返回的 digest、重新下载后的文件
- 已知限制：DSD 输出、HE-AAC/ELD、APE/TAK 编码、AIFC/BW64 写入均不可行（原因见第 4.2 节）

---

## 10. 新增内容（1.4 / 1.5 均已发布）

### 10.1 1.4.0（已发布）

1.4.0 新增并实测、随包发布的部分：

| 新增 | 类型 | 实测结果 |
|---|---|---|
| **VOC**（Creative Voice） | 音频容器 | 529 KB PCM |
| **AAC (LATM/LOAS)** | 广播封装 | 49 KB |
| **MXF** | 视频容器（MPEG-2 + PCM 48k） | 528 KB |
| **Musepack**（mpcenc，LGPL，随包） | 有损格式 | 15.7 KB（5 档质量） |

加上这四个，可输出格式从 **69 → 73**。

### 10.2 1.5.0（已发布）

| 新增 | 类型 | 实测结果 |
|---|---|---|
| **Westwood AUD** | 老游戏语音（ADPCM） | 133 KB（44.1k 立体声） |
| **APM**（Ubisoft） | 老游戏语音（ADPCM） | 132 KB |
| **ALP ADPCM** | 老游戏语音（ADPCM，22.05k 单声道） | 33 KB |
| **SMAF（MMF）** | 铃声容器（Yamaha ADPCM，8k 单声道） | 12 KB |
| **SoX** | 音频容器（PCM 32-bit） | 1.06 MB |
| **IRCAM** | 音频容器（PCM 16-bit） | 530 KB |
| **DV / GXF / NUT / MXF OPAtom** | 视频容器 | DV 720×576 / GXF 单声道 / NUT 直通 / MXF OPAtom |
| **3G2 / F4V / ISMV / WTV** | 视频容器 | MP4 家族与老流媒体/录制容器，全部实测可出 |
| **PS1 XA-ADPCM / PS2 VAG** | 设备专用（外部 psxavenc） | XA 133 KB / VAG 76 KB（3 秒正弦） |
| **QOA** | 有损（外部 qoaconv） | 106 KB（3 秒立体声） |
| **BRSTM / BCSTM / BFSTM / DSP / IDSP** | 设备专用（外部 VGAudioCli） | 各约 151 KB（3 秒） |
| **主机芯片音乐输入** | 输入（libgme） | NSF 渲染 3 分钟 → 10.7 MB FLAC |

可输出格式 **73 → 95**。NSF/SPC/VGM/PSF 等芯片音乐现在能当输入（默认渲染 3 分钟），但仍不能作为输出。

### 10.3 1.6.0（已发布）

| 新增 | 类型 | 实测结果 |
|---|---|---|
| **VCD / SVCD** | 光盘标准 | MPEG-1/MPEG-2 + MP2，352×288 / 480×576 固定规格 |
| **PSP / iPod (M4V)** | 设备变体 | H.264 + AAC，PSP 480×272 最稳 |
| **IVF / H.264 / H.265 / MPEG-2 / MPEG-4 裸流 / AVIF** | 纯视频 | 无音轨，显式 `-f` 指定封装器 |
| **KVAG** | 老游戏语音 | IMA ADPCM，22.05kHz 单声道 |

可输出格式 **95 → 106**。

### 10.3b 1.7.0（已发布）
| i18n | 中英切换（设置页 / `--lang en`）；696 条界面词典、34 条百科长文、体检/质量提示双语 |
|---|---|
| 新增 | 说明 |
|---|---|
| 转换后自检 | 每个输出重新 probe，坏文件/时长偏差 > max(2s, 8%) 标失败 |
| ReplayGain | 源标签 `replaygain_track_gain` 写入输出（FLAC/MP3 实测保留） |
| CUE 分轨 | `--split-cue` 解析 FILE/TRACK/INDEX，按轨切 FLAC 等并写轨号/标题/艺术家（实测 3s+7s 两轨、标签正确） |

### 10.4 1.6.1（已发布）

| 新增 | 类型 | 实测结果 |
|---|---|---|
| **N64 VADPCM** | 设备专用（外部 vadpcm） | 22.05k 单声道 2 秒 → 25 KB `.aifc`；解码回 2.0005s |

可输出格式 **106 → 107**。

---

## 11. 文件索引

| 路径 | 内容 |
|---|---|
| `README.md` | 面向用户的下载与使用说明 |
| `THIRD_PARTY_NOTICES.md` | 第三方组件许可与来源 |
| `docs/screenshot.jpg` | 界面截图 |
| `docs/音频格式总览.md` | 格式知识库（长文） |
| `docs/主机音频格式.md` | NSF/SPC/PSF/VGM 为什么不能转，以及替代方案 |
| `docs/法律边界-重新评估.md` | 版权/DRM 边界的判断依据 |
| `docs/契约.md` | 模块间接口约定 |
| `docs/FFmpeg-LGPL.md` | 随包 ffmpeg 的合规说明 |
| `tools/build-ffmpeg.sh` | 自建带语音编码库的 ffmpeg |
| `tools/package-release.ps1` / `package-extras.ps1` | 两种发布包 |
| `crates/fc-core/src/format/` | 格式表与全部规则（本项目的核心） |

---

## 21. 2026-11-23 当前状态（2.0.0 之后）

- 品牌：ToneLore / 声识；仓库、exe、配置文件关联、安装包均已统一
- 已发布：2.0.0（112 输出目标、English UI、可靠性/顾问、歌单、About）
- 2.0.0 后已进 main、未发 release：
  - 独立控制台（日志 + 命令 `help/version/logs/clear` + 落盘）
  - 歌词管理页（`.lrc` 编辑器、试听打轴、封面列表）、详情页同步歌词
  - 设置 Storage 页（默认导出目录、日志、缓存/临时文件）
  - PSP 分辨率修复、音乐库播放修复、启动过渡 650ms
- 真实音乐验证：99 OK / 12 SKIP_EXTERNAL / 1 FAIL（PSP，已修未全量重跑）
- 已知限制：设置 Display/Behavior/Tools 未分栏；外部编码器无自动验证；歌词只读同名 `.lrc`
