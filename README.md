# ToneLore 声识 — 音频格式顾问与转换器

> 一个讲道理的音频格式转换器：先告诉你「这么转会损失什么」，再让你决定。
>
> 转换只是入口，它真正干的事是把格式知识摊开给你看。

[English (short)](README.en.md) · [English (full)](#english) · [简体中文](README.md)

![版本](https://img.shields.io/badge/版本-2.0.2-2ea44f)
![平台](https://img.shields.io/badge/平台-Windows%2010%2F11%20x64-0078d4)
![许可](https://img.shields.io/badge/许可-MIT-blue)
![ffmpeg](https://img.shields.io/badge/ffmpeg-8.0%20LGPL--2.1%20(随包)-orange)

---

## 它适合谁

- **想知道代价的人**：同一首歌转 MP3、Opus、AAC 到底差在哪，采样率/位深/声道会怎么被改，哪些格式只是“能读不能写”。
- **手里有奇怪文件的人**：PSF/SPC/VGM 芯片音乐、PS1 XA、N64 VADPCM、BRSTM、MOD/XM、CUE 整轨。
- **要批量归档的人**：需要保留标签、封面、ReplayGain，并且转完能回读自检。

**不适合谁**：只想“点一下、稳定出活、别问问题”的生产用户。ToneLore 会把限制和风险摆在你面前，有时这很烦。

---

## 一分钟了解

**能做什么**

- **112 个输出目标**：65 个音频编码/容器 + 36 个视频容器 + 11 个设备专用；另有 47 个只读参考格式
- **解释代价**：每个格式都写清楚有无专利、体积倍率、截止频率、会不会重采样/改声道
- **当顾问**：转之前就告诉你「这步会损失什么」，而不是转完才后悔

**不能做什么**

- **不解 DRM**：NCM / QMC / KGM / OMA 只读明文封面和尾部信息，**没有解密逻辑**
- **不假装支持**：没有编码器的格式灰显并说明原因（APE / TAK / DSD / WMA Lossless 等只读）
- **不联网、不上传、不做流媒体下载**：所有处理都在本机

---

## 界面

![界面截图](docs/screenshot.jpg)

右侧面板就是“格式顾问”：选中一个输出格式，下面会列出它的代价、限制和适用场景。
左侧只读格式灰显，鼠标停上去会解释为什么不能输出。

---

## 下载

| 版本 | 包 | 大小 | SHA256 |
|---|---|---|---|
| **2.0.2** | [ToneLore-2.0.2-win64.zip](https://github.com/TeiketsuCN/ToneLore/releases/download/v2.0.2/ToneLore-2.0.2-win64.zip) | 28.4 MB | `EB6517E04B758181AB5440FE1E83CFD1284A9290B2D5F5FD32C092BBEFBCB7FF` |
| 2.0.2 可选工具 | [ToneLore-2.0.2-extras-win64.zip](https://github.com/TeiketsuCN/ToneLore/releases/download/v2.0.2/ToneLore-2.0.2-extras-win64.zip) | 13.1 MB | `5E2B5BC50BC8905931B60CD2FC38387940E20F27200AE8D4728827A685F6B902` |
| 2.0.1 | [ToneLore-2.0.1-win64.zip](https://github.com/TeiketsuCN/ToneLore/releases/download/v2.0.1/ToneLore-2.0.1-win64.zip) | 28.4 MB | `18075D5C078DCEFE98B8728B9312BD5FEB7043B779825C992B8150889249B2EC` |
| 2.0.1 可选工具 | [ToneLore-2.0.1-extras-win64.zip](https://github.com/TeiketsuCN/ToneLore/releases/download/v2.0.1/ToneLore-2.0.1-extras-win64.zip) | 13.1 MB | `D99D9EFFBCE1172EB6B9CDE8A2468180ED50DC231FF15D575C69B691704FC1E0` |
| 2.0.0 | [ToneLore-2.0.0-win64.zip](https://github.com/TeiketsuCN/ToneLore/releases/download/v2.0.0/ToneLore-2.0.0-win64.zip) | 28.3 MB | `60ECA25205AF649CC8EAFAB28DE335305BE7F0ABEE5BBD4A92C60DC95D290B0F` |
| 2.0.0 可选工具 | [ToneLore-2.0.0-extras-win64.zip](https://github.com/TeiketsuCN/ToneLore/releases/download/v2.0.0/ToneLore-2.0.0-extras-win64.zip) | 13.1 MB | `AFEF0CECFD63720F1BC0E715BC6011DCE94A20D02A820FB408A283792EF0E405` |
| **1.8.0** | [ToneBox-1.8.0-win64.zip](https://github.com/TeiketsuCN/ToneLore/releases/download/v1.8.0/ToneBox-1.8.0-win64.zip) | 28.3 MB | `007C9C2D0E1AB69A4258409E935C610E2F722AB87E0BAA7FF0A24DD158FBA65B` |
| 1.8.0 可选工具 | [ToneBox-1.8.0-extras-win64.zip](https://github.com/TeiketsuCN/ToneLore/releases/download/v1.8.0/ToneBox-1.8.0-extras-win64.zip) | 13.1 MB | `1E3A6F4625D53F2B1748C39E14D697998BC4B8DA9E33B1549CE1DF8A44134512` |

- **Windows 10 / 11 x64**，解压即用，不需要安装
- 解压后双击 `ToneLore.exe`；不要单独移动 `ffmpeg\`、`speex\`、`wavpack\`、`musepack\` 四个目录
- 解压即用、不写注册表、不装服务；删掉文件夹就是卸载

### 为什么只有 Windows

音频独占、DSD DoP、设备枚举都直接绑在 WASAPI 上，换平台不是改个 UI。macOS / Linux 在路线图上，
但还没有时间表；当前理论上可以用 Wine 跑 CLI（未验证，不承诺）。

### 可选外部工具包（单独下载）

主包不带分发受限或体积较大的工具，需要时另下 `ToneBox-1.8.0-extras-win64.zip`：

| 工具 | 许可 | 用途 |
|---|---|---|
| **atracdenc** | LGPL-2.1 | ATRAC1 / ATRAC3 / ATRAC3+ |
| **psxavenc** | Zlib | PS1 XA-ADPCM / PS2 VAG |
| **VGAudioCli** | MIT | BRSTM / BCSTM / BFSTM / DSP / IDSP |
| **qoaconv** | MIT | QOA |
| **vadpcm** | MPL-2.0 | N64 VADPCM（`.aifc`） |
| **vgmstream** | ISC | PSF / ADS / BRSTM / BCSTM / HCA 等输入 |
| **openmpt123** | BSD-3-Clause | MOD / XM / IT / S3M 等模块输入 |
| **OptimFROG** | 闭源免费 | `install-ofr.cmd` 从官网下载 + SHA-1 校验，不再分发 |

用法：把 extras 包**解压到 ToneLore 目录**（和 `ToneLore.exe` 同级），除 OptimFROG 外都立刻可用；
OptimFROG 双击一次 `install-ofr.cmd`。装完回程序里点 **设置 → 依赖 / 外部工具 → 重新检测**。

> **Beta 阶段**：核心转换路径有测试覆盖，边界组合（如 24bit FLAC + 滤镜、特殊容器）仍可能踩坑。
> 建议先备份原文件；遇到问题请开 issue，附上源文件信息与 `--health` 输出。

---

## 格式数字怎么算

「112 个」不是把只读格式也算进去凑数：

| 类别 | 数量 | 例子 |
|---|---|---|
| 音频编码 / 容器 | **65** | FLAC、MP3、Opus、AAC、AMR、Speex、QOA、WAV、CAF、BWF… |
| 视频容器 | **36** | MKV、MP4、WebM、MOV、MXF、VCD、AMV、GIF、APNG… |
| 设备专用 | **11** | ATRAC1/3/3+、PS1 XA、PS2 VAG、BRSTM、DSP、N64 VADPCM… |
| **合计可输出** | **112** | |
| 只读参考（能读不能写） | 47 | APE、TAK、DSD、WMA Lossless、DTS-HD… |

「只读」不是“不支持”：这些文件可以正常当输入，转成 FLAC / MP3 / WAV 都没问题，只是本程序不提供**写入**它们的编码器。

### 常见格式的读写边界

| 源格式 | → FLAC | → MP3 | → WAV | 作为输出目标 |
|---|---|---|---|---|
| APE | ✅ | ✅ | ✅ | ❌ 只读 |
| TAK | ✅ | ✅ | ✅ | ❌ 只读 |
| DSD（DSF / DFF） | ✅ | ✅ | ✅ | ❌ 只读 |
| WMA Lossless | ✅ | ✅ | ✅ | ❌ 只读 |
| AMR | ✅ | ✅ | ✅ | ✅ AMR-NB / AMR-WB |
| MKA | ✅ | ✅ | ✅ | ❌ 只读（要封装请用 MKV） |

---

## 支持的格式（可输出）

| 类别 | 可输出 |
|---|---|
| 无损（13） | FLAC、WAV、ALAC、AIFF、WavPack、TTA、CAF、W64、裸 PCM、TrueHD、MLP、Ogg FLAC、OptimFROG（需 ofr.exe） |
| 有损（18） | MP3、Opus、Vorbis、MP2、AAC、WMA v1/v2、Musepack、QOA、aptX / aptX HD / SBC、ADX、WavPack 有损、Westwood AUD、KVAG、Argonaut ASF、APM |
| 空间音频（5） | AC-3、E-AC-3、AC-4（直通）、DTS、AAC 5.1 |
| 语音/电话（20） | G.711 A/μ、G.722、G.723.1、G.726、GSM 06.10、AMR-NB、AMR-WB、iLBC、LC3、Codec2、Speex、ADPCM 系列、DFPWM、RealAudio 1.0、Nellymoser、ALP |
| 音频容器（9） | VOC、AAC-LATM、IEC 61937、AU / SND、RF64、BWF、SMAF（MMF）、SoX、IRCAM |
| 视频容器（36） | MKV、MP4、WebM、MOV、TS、M2TS、FLV、AVI、MPEG-PS、VOB、OGV、WMV、3GP、MXF、DV、GXF、NUT、MXF OPAtom、3G2、F4V、ISMV、WTV、VCD、SVCD、PSP、iPod (M4V)、IVF、H.264 / H.265 / MPEG-2 / MPEG-4 裸流、AVIF、AMV、SWF、GIF、APNG |
| 设备专用（11） | ATRAC1 / ATRAC3 / ATRAC3+（atracdenc）、PS1 XA / PS2 VAG（psxavenc）、BRSTM / BCSTM / BFSTM / DSP / IDSP（VGAudioCli）、N64 VADPCM（vadpcm） |

视频容器支持保留第一条画面 + 全部音轨；画面可复制或转 H.264 / H.265 / VP9 / AV1 / ProRes / DNxHR /
FFV1 / VP8 / MJPEG / MPEG-2 / MPEG-4 / Theora / WMV2 / DV，音轨逐条决定复制还是重编码。

**只读格式**（能解析、能解释为什么不能输出）：APE、TAK、Shorten、WMA Lossless、DSD（DSF/DFF/DST）、
DTS-HD/X、MPEG-H、USAC、EVS、LC3plus、TwinVQ、RealAudio Cook 等。

> 主机芯片音乐（NSF / SPC / GBS / VGM / PSF 等）**可以当输入**：随包 ffmpeg 用 libgme
> 渲染成 PCM，再转成 FLAC/MP3。它们本身不是波形音频，所以不能作为输出，
> 转换时默认只渲染前 3 分钟。细节见 [docs/主机音频格式.md](docs/主机音频格式.md)。

---

## 外部工具

| 工具 | 是否随包 | 用途 |
|---|---|---|
| `ffmpeg` / `ffprobe` | ✅ 主包 | 解码、编码、探测（LGPL-2.1-or-later 自建，带 libgsm / vo-amrwbenc / libilbc / liblc3 / codec2 / libgme） |
| `speexenc` | ✅ 主包 | Speex 输出（BSD） |
| `wavpack` | ✅ 主包 | WavPack **有损**输出（hybrid 模式） |
| `mpcenc` | ✅ 主包 | Musepack 输出（LGPL-2.1） |
| `atracdenc` | 📦 extras | ATRAC1 / ATRAC3 / ATRAC3+（LGPL-2.1） |
| `psxavenc` | 📦 extras | PS1 XA-ADPCM / PS2 VAG（Zlib） |
| `VGAudioCli` | 📦 extras | BRSTM / BCSTM / BFSTM / DSP / IDSP（MIT） |
| `qoaconv` | 📦 extras | QOA（MIT） |
| `vadpcm` | 📦 extras | N64 VADPCM（MPL-2.0） |
| `vgmstream` | 📦 extras | PSF / ADS / BRSTM / BCSTM / HCA 等输入（ISC） |
| `openmpt123` | 📦 extras | MOD / XM / IT / S3M 等模块输入（BSD-3-Clause） |
| `ofr` | ⬇️ extras 内一键下载 | OptimFROG（官方许可限制再分发） |

程序会自动在「程序目录 → 程序目录子目录 → PATH」里找这些工具。
装了新工具后，到 **设置 → 依赖 / 外部工具 → 重新检测** 即可，不用重启。

---

## 命令行

```
ToneLore.exe --version                 显示版本
ToneLore.exe --help                    列出全部参数
ToneLore.exe --formats                 列出可输出格式（id / 名称 / 后缀）
ToneLore.exe --convert in.flac out.mpc --to musepack [--vcodec prores]
ToneLore.exe --tools                   探测 ffmpeg / 各外部编码器
ToneLore.exe --devices                 列出音频设备
ToneLore.exe --play <file> [秒数]       直接试听
ToneLore.exe --health <file>           内容体检
ToneLore.exe --split-cue <cue> [--to flac] [--out-dir 目录]   按 CUE 分轨
ToneLore.exe --register                注册到「打开方式」
ToneLore.exe --unregister              移除注册
ToneLore.exe <文件>                     打开并直接试听（双击文件的效果）
```

格式 id 见 `--formats`；`--convert` 会自动分派给 ffmpeg 或对应的外部编码器。

---

## 怎么验证这个包

- **没有代码签名**：项目没有购买证书，Windows 可能弹 SmartScreen。介意的话先在沙箱/虚拟机里跑，
  或把包传到 VirusTotal 自行扫描（我们没有预先生成的扫描链接，不伪造）。
- **校验 SHA256**（PowerShell）：

  ```powershell
  Get-FileHash .\ToneBox-1.8.0-win64.zip -Algorithm SHA256
  ```

  和上面的值逐字比对。
- **自己构建**：命令见下面「构建」，任何人可以用仓库里的脚本复现同一个包。
- **随包 ffmpeg**：由 [`tools/build-ffmpeg.sh`](tools/build-ffmpeg.sh) 从 FFmpeg 源码构建（LGPL-2.1-or-later，
  不含 GPL 组件），构建脚本和第三方许可都在仓库里。
- **CI 现状**：[`.github/workflows/ci.yml`](.github/workflows/ci.yml) 在 push/PR 上跑 `cargo test`；
  **发布包目前由本地 `tools/package-release.ps1` 构建**（命令见下），CI 自动出包还在计划中，不想把没验证的东西写进 README。

---

## 构建

需要 Rust 的 MSVC 或 GNU 工具链（1.75+）。仓库不含二进制依赖，首次构建前先抓取随包工具：

```powershell
# 1. 抓依赖（ffmpeg / speex / wavpack / musepack）
powershell -ExecutionPolicy Bypass -File tools\fetch-ffmpeg.ps1
powershell -ExecutionPolicy Bypass -File tools\fetch-speex.ps1
powershell -ExecutionPolicy Bypass -File tools\fetch-wavpack.ps1

# 可选：extras 包用到的外部工具
powershell -ExecutionPolicy Bypass -File tools\fetch-atracdenc.ps1
powershell -ExecutionPolicy Bypass -File tools\fetch-psxavenc.ps1
powershell -ExecutionPolicy Bypass -File tools\fetch-vgaudio.ps1
powershell -ExecutionPolicy Bypass -File tools\fetch-qoa.ps1
powershell -ExecutionPolicy Bypass -File tools\fetch-vgmstream.ps1
powershell -ExecutionPolicy Bypass -File tools\fetch-openmpt123.ps1
powershell -ExecutionPolicy Bypass -File tools\fetch-optimfrog.ps1

# 2. 编译
cargo build --release -p fc-gui

# 3. 测试（格式表、参数映射、警告逻辑、容器矩阵、CUE 解析）
cargo test -p fc-core --lib

# 4. 打包（版本号从 Cargo.toml 读）
powershell -ExecutionPolicy Bypass -File tools\package-release.ps1
powershell -ExecutionPolicy Bypass -File tools\package-extras.ps1
```

想自己重新构建随包 ffmpeg（带语音编码库 + libgme）：见
[`tools/build-ffmpeg.sh`](tools/build-ffmpeg.sh) 与 [`tools/build-ffmpeg.ps1`](tools/build-ffmpeg.ps1)；
首次约 20~40 分钟。

---

## 设计红线

- **只做本地处理**：不联网、不上传、不解 DRM、不绕过任何版权保护
- **不假装**：没有编码器的格式就灰着并说明原因；做不到的事不写进格式表
- **不改你的文件**：除非你明确选择「删除源文件」，否则只写新文件
- **许可干净**：程序本体 MIT；随包 ffmpeg 是 **LGPL-2.1-or-later**（不含 GPL 组件）；
  `ofr.exe` 不随包分发

---

## 已知问题

- FLAC + 24bit + 截取/滤镜的组合，随包 ffmpeg 可能报 `invalid block size`：程序已内置两段式规避
- 部分 Android 录音（.amr）时长元数据不规范，显示可能偏短
- ATRAC9 只读：atracdenc 不支持，专利也在保护期
- 视频容器只保留第一条画面；多画面（多角度）不处理
- 芯片音乐默认只渲染 3 分钟（NSF/SPC 没有曲目长度字段，libgme 会一直循环）
- **HE-AAC / xHE-AAC / AAC-ELD：能解码**（ffmpeg 原生），**不提供编码**（要 libfdk-aac，非自由）
- APE / TAK / DSD / AIFC / BW64 输出没有可用编码器，归入只读参考

---

## 许可与第三方

- 程序本体：**MIT**（见 [`LICENSE`](LICENSE)）
- 随包 ffmpeg：**LGPL-2.1-or-later**，保留许可文件、允许替换 DLL、提供构建脚本
- 其他随包/可选组件（Speex、WavPack、libgme、libopenmpt、psxavenc、VGAudio、QOA、vgmstream 等）
  的许可与来源见 [`THIRD_PARTY_NOTICES.md`](THIRD_PARTY_NOTICES.md)

---

## 更新日志

### 2.0.2（2026-11）
- 修「引擎和设备」页报 `尚未调用 CoInitialize`：COM 改为按线程初始化，设备列表正常显示
- 外部编码器输出统一自检：XA 查扇区结构、DSP/IDSP 用 VGAudioCli、VADPCM 用 vadpcm、OptimFROG 用 `ofr --verify`；
  修掉 GUI 里 XA / DSP / VADPCM / OptimFROG 被 ffmpeg 自检误判为失败的问题
- 设置五个标签统一放进滚动容器（「关于」「产出与存储」在小窗口也能滚到底）
- 歌词搬到播放区：播放栏新增「词」按钮，面板按时间轴高亮、点任意一行跳转、右上角直接进 `.lrc` 编辑器；
  左侧不再有独立的「歌词管理」大项

### 2.0.1（2026-11）
- 设置页真正分栏：显示 / 行为 / 依赖工具 / 产出与存储 / 关于各自独立单页
  （以前点前三个标签都是同一整页，「产出与存储」甚至打不开）
- 5.1 声道源转 WMA / LC3 / ADX / VOC / Musepack / QOA / OptimFROG 不再失败，自动下混成立体声
- 验证补齐：112 个输出格式全部通过真实音乐验证；全库 26 首真实歌曲跑核心格式 931/943 通过，
  12 个失败都是 96k / 单声道 / 5.1 等属性上限（已在 `docs/验证-全库属性矩阵-2026-11-23.md` 记录）

### 2.0.0（2026-11）
- 品牌统一为 **ToneLore（声识）**：窗口、启动进度条、关于页、exe、配置目录、文件关联、安装包
- 旧 `%APPDATA%\FormatConverter` 配置自动迁移到 `%APPDATA%\ToneLore`（歌单/预设不丢）
- 收录 1.8/1.9 的可靠性、格式顾问、歌单与设置页改动

### 1.8.0（2026-11）
- 1.8 可靠性：合成测试资产 + 10 例真实编码矩阵、自检真解码、无损 PCM 校验、同名策略、临时清理、错误分类/重试、原子输出
- 1.9 顾问：Tags 扩展、容器标签能力表与丢失警告、Rule/Prediction 分级、场景推荐（归档/手机/车载/语音）+ 多方案
- 音乐库：自建歌单（新建/删除/加歌/持久化）
- 设置页：Display/Behavior/Tools/About 标签，About 独立（版本、GitHub、二维码、许可）；输出摘要不再截断

### 1.7.1（2026-11）
- 修复「元数据」折叠区内容不在标题下方：歌词设置移到 Metadata 上方，展开项紧跟标题
- 修复右上角输出摘要太长压住「+ 任务」：按剩余宽度截断
- 修复封面选择行超出右栏、按钮不透明：自适应宽度 + 透明/描边按钮，说明自动换行

### 1.7.0（2026-11）
- 可输出格式 **107 → 112**：Argonaut ASF、AMV、SWF、GIF、APNG；MOV/MKV/MXF/WebM/AVI 新增 ProRes / FFV1 / DNxHR / VP8 / MJPEG 编码
- **双击文件直接播放**：带文件路径启动会入队并自动试听
- **注册到「打开方式」**：`--register` / 设置页按钮；不静默抢默认（Windows 要求用户确认）
- **vgmstream 输入**：PSF / PS2 ADS / BRSTM / BCSTM / BFSTM / HCA 可以先解成 WAV 再转（extras 里带）
- **openmpt123 输入**：MOD / XM / IT / S3M 等模块音乐可以直接拖进来（extras 里带）
- **转换后自检**：每个输出都重新 probe，坏文件/时长离谱直接标失败，不假装成功
- **ReplayGain 标签写入**：源文件的 `replaygain_track_gain` 会带进 FLAC / MP3 等输出
- **CUE 分轨**：`--split-cue album.cue --to flac --out-dir out`，或设置页「CUE 分轨」按钮；按 TRACK/INDEX 切轨并写轨号/标题/艺术家
- **English UI**：设置页 Language 切换中英，或 `--lang en`；界面、格式说明、内容体检、质量提示、百科长文均已英文化

### 1.6.1（2026-11）
- 可输出格式 **106 → 107**：新增 **N64 VADPCM**（`.aifc`，外部 vadpcm，MPL-2.0）
- N64 VADPCM 只收 16-bit 单声道；1.6 的能力全部保留

### 1.6.0（2026-11）
- 可输出格式 **95 → 106**
- 新增 VCD / SVCD / PSP / iPod (M4V)、IVF、H.264 / H.265 / MPEG-2 / MPEG-4 裸流、AVIF、KVAG
- 纯视频容器不再被自动补音轨；VCD/SVCD 会强制 352×288 / 480×576、MP2 224k
- 1.5 的主机音乐输入、PS1/PS2、任天堂容器、QOA 全部保留

### 1.5.0（2026-11）
- 可输出格式 **73 → 95**
- **主机芯片音乐可以当输入**：ffmpeg 加 libgme，NSF / SPC / GBS / VGM / PSF 等可用
- 新增 PS1 XA-ADPCM / PS2 VAG（psxavenc）、QOA（qoaconv）、
  BRSTM / BCSTM / BFSTM / DSP / IDSP（VGAudioCli）
- 新增视频容器：DV、GXF、NUT、MXF OPAtom、3G2、F4V、ISMV、WTV
- 新增老游戏语音：Westwood AUD、APM、ALP、SMAF（MMF）；容器 SoX、IRCAM
- 修复 CLI 转视频时写死 `libx264` 的问题（随包构建只有 libopenh264）
- `--version` / `--help` 现在是真正的 CLI 参数
- extras 包新增 psxavenc / VGAudioCli / qoaconv，打包自动生成 `SHA256SUMS`

### 1.4.0（2026-11）
- 可输出格式 **69 → 73**：VOC、AAC-LATM / LOAS、MXF、Musepack
- 随包 Musepack 编码器（LGPL-2.1），Musepack 开箱可用

### 1.3.1（2026-11）
- 新增 OptimFROG 输出（走官方 ofr.exe）
- 修复命令行 `--convert` 不支持外部编码器
- 新增可选外部工具包（extras）

### 1.3（2026-11）
- 可输出格式 **43 → 68**：视频容器 8 → 13、GSM / AMR-WB / iLBC / LC3 / Codec2 / DFPWM、WavPack 有损、IEC 61937、SND
- 随包 ffmpeg 换成自建 8.0（LGPL-2.1-or-later，152 MB → 59.6 MB）

### 1.2
- 格式扩容（Speex / AMR-NB / G.711 / G.722 / ADPCM / AU / RF64 / BWF）
- 侧栏七组重组、只读格式常驻显示、频谱分析省电优化

### 1.0
- 首个公开测试版：三栏界面、队列转换、格式百科、音乐库

---

## 赞助

项目免费开源，接受自愿赞助，不影响任何人在 MIT 下的权利。

<img width="1080" height="1620" alt="微信" src="https://github.com/user-attachments/assets/f53df0ad-01b1-4a4a-a9c2-c7db07b8cd3e" />
<img width="1242" height="1692" alt="支付宝" src="https://github.com/user-attachments/assets/9647d9f2-6607-46c3-93e8-423f9ebdf1c1" />

---

# English

﻿# ToneLore — an audio format advisor and converter

> A converter that tells you what a conversion will cost **before** you commit.
>
> Conversion is only the entry point; the real job is laying the format knowledge out in front of you.


![version](https://img.shields.io/badge/version-1.7.1-2ea44f)
![platform](https://img.shields.io/badge/platform-Windows%2010%2F11%20x64-0078d4)
![license](https://img.shields.io/badge/license-MIT-blue)
![ffmpeg](https://img.shields.io/badge/ffmpeg-8.0%20LGPL--2.1%20(bundled)-orange)

---

## Who it is for

- **People who want to know the cost**: what really changes between MP3, Opus and AAC; how sample rate,
  bit depth and channels are altered; which formats are read-only.
- **People with odd files**: PSF/SPC/VGM chiptunes, PS1 XA, N64 VADPCM, BRSTM, MOD/XM, CUE sheets.
- **People archiving in bulk**: keep tags, cover art and ReplayGain, and re-probe every output afterwards.

**Not for** people who just want "one click, stable output, no questions". ToneLore puts limits and
trade-offs in front of you, which can be annoying.

---

## In one minute

**What it does**

- **112 output targets**: 65 audio codecs/containers + 36 video containers + 11 device formats;
  plus 47 read-only reference formats
- **Explains the cost**: patent status, size ratio, cutoff frequency, resampling/channel changes
- **Acts as an advisor**: tells you what a step will lose before you run it

**What it does not do**

- **No DRM**: NCM / QMC / KGM / OMA are only inspected for plaintext cover art and footer metadata.
  There is **no decryption code**.
- **No fake support**: formats without an encoder are greyed out with a reason
  (APE / TAK / DSD / WMA Lossless are read-only)
- **No network**: no upload, no telemetry, no streaming downloads

---

## Interface

![screenshot](docs/screenshot.jpg)

The panel on the right is the format advisor: select a target and it lists the cost, limits and
typical use. Read-only formats are greyed out on the left, with a reason on hover.

---

## Download

| Version | Package | Size | SHA256 |
|---|---|---|---|
| **2.0.2** | [ToneLore-2.0.2-win64.zip](https://github.com/TeiketsuCN/ToneLore/releases/download/v2.0.2/ToneLore-2.0.2-win64.zip) | 28.4 MB | `EB6517E04B758181AB5440FE1E83CFD1284A9290B2D5F5FD32C092BBEFBCB7FF` |
| 2.0.2 可选工具 | [ToneLore-2.0.2-extras-win64.zip](https://github.com/TeiketsuCN/ToneLore/releases/download/v2.0.2/ToneLore-2.0.2-extras-win64.zip) | 13.1 MB | `5E2B5BC50BC8905931B60CD2FC38387940E20F27200AE8D4728827A685F6B902` |
| 2.0.1 | [ToneLore-2.0.1-win64.zip](https://github.com/TeiketsuCN/ToneLore/releases/download/v2.0.1/ToneLore-2.0.1-win64.zip) | 28.4 MB | `18075D5C078DCEFE98B8728B9312BD5FEB7043B779825C992B8150889249B2EC` |
| 2.0.1 可选工具 | [ToneLore-2.0.1-extras-win64.zip](https://github.com/TeiketsuCN/ToneLore/releases/download/v2.0.1/ToneLore-2.0.1-extras-win64.zip) | 13.1 MB | `D99D9EFFBCE1172EB6B9CDE8A2468180ED50DC231FF15D575C69B691704FC1E0` |
| 2.0.0 | [ToneLore-2.0.0-win64.zip](https://github.com/TeiketsuCN/ToneLore/releases/download/v2.0.0/ToneLore-2.0.0-win64.zip) | 28.3 MB | `60ECA25205AF649CC8EAFAB28DE335305BE7F0ABEE5BBD4A92C60DC95D290B0F` |
| 2.0.0 可选工具 | [ToneLore-2.0.0-extras-win64.zip](https://github.com/TeiketsuCN/ToneLore/releases/download/v2.0.0/ToneLore-2.0.0-extras-win64.zip) | 13.1 MB | `AFEF0CECFD63720F1BC0E715BC6011DCE94A20D02A820FB408A283792EF0E405` |
| **1.8.0** | [ToneBox-1.8.0-win64.zip](https://github.com/TeiketsuCN/ToneLore/releases/download/v1.8.0/ToneBox-1.8.0-win64.zip) | 28.3 MB | `007C9C2D0E1AB69A4258409E935C610E2F722AB87E0BAA7FF0A24DD158FBA65B` |
| 1.7.1 extras | [ToneBox-1.8.0-extras-win64.zip](https://github.com/TeiketsuCN/ToneLore/releases/download/v1.8.0/ToneBox-1.8.0-extras-win64.zip) | 13.1 MB | `1E3A6F4625D53F2B1748C39E14D697998BC4B8DA9E33B1549CE1DF8A44134512` |

- **Windows 10 / 11 x64**, portable, no installation
- Unzip and run `ToneLore.exe`; do not move `ffmpeg\`, `speex\`, `wavpack\`, `musepack\` separately
- Writes no registry keys, installs no service; delete the folder to uninstall

### Why Windows only

Exclusive audio output, DSD DoP and device enumeration are tied directly to WASAPI. Porting is not a UI
change. macOS / Linux are on the roadmap with no ETA; the CLI can theoretically run under Wine
(untested, not promised).

### Optional extras package

The main package omits tools that are large or have redistribution limits. Grab
`ToneBox-1.8.0-extras-win64.zip` if you need them:

| Tool | License | Purpose |
|---|---|---|
| **atracdenc** | LGPL-2.1 | ATRAC1 / ATRAC3 / ATRAC3+ |
| **psxavenc** | Zlib | PS1 XA-ADPCM / PS2 VAG |
| **VGAudioCli** | MIT | BRSTM / BCSTM / BFSTM / DSP / IDSP |
| **qoaconv** | MIT | QOA |
| **vadpcm** | MPL-2.0 | N64 VADPCM (`.aifc`) |
| **vgmstream** | ISC | PSF / ADS / BRSTM / BCSTM / HCA input |
| **openmpt123** | BSD-3-Clause | MOD / XM / IT / S3M input |
| **OptimFROG** | freeware, closed | `install-ofr.cmd` downloads it from the official site with SHA-1 check; not redistributed |

Unzip the extras package **into the ToneLore folder** (next to `ToneLore.exe`). Everything except
OptimFROG works immediately; double-click `install-ofr.cmd` once for that one. Then open
**Settings → Dependencies / external tools → Re-detect**.

> **Beta**: core conversion paths are covered by tests, but edge combinations (24-bit FLAC + filters,
> unusual containers) can still break. Back up originals; open an issue with the source info and
> `--health` output if something fails.

---

## How the numbers add up

"112" is not inflated with read-only formats:

| Category | Count | Examples |
|---|---|---|
| Audio codecs / containers | **65** | FLAC, MP3, Opus, AAC, AMR, Speex, QOA, WAV, CAF, BWF… |
| Video containers | **36** | MKV, MP4, WebM, MOV, MXF, VCD, AMV, GIF, APNG… |
| Device formats | **11** | ATRAC1/3/3+, PS1 XA, PS2 VAG, BRSTM, DSP, N64 VADPCM… |
| **Total output targets** | **112** | |
| Read-only references | 47 | APE, TAK, DSD, WMA Lossless, DTS-HD… |

"Read-only" does not mean "unsupported": those files work fine as input and can be converted to
FLAC / MP3 / WAV. The app just has no **encoder** for writing them.

### Common read/write boundaries

| Source | → FLAC | → MP3 | → WAV | As an output target |
|---|---|---|---|---|
| APE | ✅ | ✅ | ✅ | ❌ read-only |
| TAK | ✅ | ✅ | ✅ | ❌ read-only |
| DSD (DSF / DFF) | ✅ | ✅ | ✅ | ❌ read-only |
| WMA Lossless | ✅ | ✅ | ✅ | ❌ read-only |
| AMR | ✅ | ✅ | ✅ | ✅ AMR-NB / AMR-WB |
| MKA | ✅ | ✅ | ✅ | ❌ read-only (use MKV to mux) |

---

## Supported output formats

| Category | Targets |
|---|---|
| Lossless (13) | FLAC, WAV, ALAC, AIFF, WavPack, TTA, CAF, W64, raw PCM, TrueHD, MLP, Ogg FLAC, OptimFROG (needs ofr.exe) |
| Lossy (18) | MP3, Opus, Vorbis, MP2, AAC, WMA v1/v2, Musepack, QOA, aptX / aptX HD / SBC, ADX, lossy WavPack, Westwood AUD, KVAG, Argonaut ASF, APM |
| Spatial (5) | AC-3, E-AC-3, AC-4 (pass-through), DTS, AAC 5.1 |
| Speech / telephony (20) | G.711 A/μ, G.722, G.723.1, G.726, GSM 06.10, AMR-NB, AMR-WB, iLBC, LC3, Codec2, Speex, ADPCM family, DFPWM, RealAudio 1.0, Nellymoser, ALP |
| Audio containers (9) | VOC, AAC-LATM, IEC 61937, AU / SND, RF64, BWF, SMAF (MMF), SoX, IRCAM |
| Video containers (36) | MKV, MP4, WebM, MOV, TS, M2TS, FLV, AVI, MPEG-PS, VOB, OGV, WMV, 3GP, MXF, DV, GXF, NUT, MXF OPAtom, 3G2, F4V, ISMV, WTV, VCD, SVCD, PSP, iPod (M4V), IVF, H.264 / H.265 / MPEG-2 / MPEG-4 raw streams, AVIF, AMV, SWF, GIF, APNG |
| Device (11) | ATRAC1 / ATRAC3 / ATRAC3+ (atracdenc), PS1 XA / PS2 VAG (psxavenc), BRSTM / BCSTM / BFSTM / DSP / IDSP (VGAudioCli), N64 VADPCM (vadpcm) |

Video containers keep the first video stream plus all audio tracks. Video can be copied or encoded to
H.264 / H.265 / VP9 / AV1 / ProRes / DNxHR / FFV1 / VP8 / MJPEG / MPEG-2 / MPEG-4 / Theora / WMV2 / DV;
each audio track is set to copy or re-encode individually.

**Read-only formats** (parsed and explained, but no encoder): APE, TAK, Shorten, WMA Lossless,
DSD (DSF/DFF/DST), DTS-HD/X, MPEG-H, USAC, EVS, LC3plus, TwinVQ, RealAudio Cook, etc.

> Chiptunes (NSF / SPC / GBS / VGM / PSF …) **work as input**: the bundled ffmpeg renders them to PCM
> through libgme, then converts to FLAC/MP3. They are not waveform audio, so they cannot be output
> targets; rendering is limited to the first 3 minutes by default. See
> [docs/主机音频格式.md](docs/主机音频格式.md) (Chinese).

---

## External tools

| Tool | Bundled? | Purpose |
|---|---|---|
| `ffmpeg` / `ffprobe` | ✅ main | decode, encode, probe (self-built LGPL-2.1-or-later with libgsm / vo-amrwbenc / libilbc / liblc3 / codec2 / libgme) |
| `speexenc` | ✅ main | Speex output (BSD) |
| `wavpack` | ✅ main | lossy WavPack (hybrid mode) |
| `mpcenc` | ✅ main | Musepack output (LGPL-2.1) |
| `atracdenc` | 📦 extras | ATRAC1 / ATRAC3 / ATRAC3+ (LGPL-2.1) |
| `psxavenc` | 📦 extras | PS1 XA-ADPCM / PS2 VAG (Zlib) |
| `VGAudioCli` | 📦 extras | BRSTM / BCSTM / BFSTM / DSP / IDSP (MIT) |
| `qoaconv` | 📦 extras | QOA (MIT) |
| `vadpcm` | 📦 extras | N64 VADPCM (MPL-2.0) |
| `vgmstream` | 📦 extras | PSF / ADS / BRSTM / BCSTM / HCA input (ISC) |
| `openmpt123` | 📦 extras | MOD / XM / IT / S3M input (BSD-3-Clause) |
| `ofr` | ⬇️ downloaded by extras | OptimFROG (official license restricts redistribution) |

Tools are searched in "program directory → program subdirectories → PATH". After installing a new tool,
use **Settings → Dependencies / external tools → Re-detect**; no restart needed.

---

## Command line

```
ToneLore.exe --version                 show version
ToneLore.exe --help                    list all arguments
ToneLore.exe --formats                 list output formats (id / name / extension)
ToneLore.exe --convert in.flac out.mpc --to musepack [--vcodec prores]
ToneLore.exe --tools                   detect ffmpeg / external encoders
ToneLore.exe --devices                 list audio devices
ToneLore.exe --play <file> [seconds]   play directly
ToneLore.exe --health <file>           content health check
ToneLore.exe --split-cue <cue> [--to flac] [--out-dir DIR]   split by CUE
ToneLore.exe --register                register in "Open with"
ToneLore.exe --unregister             remove the registration
ToneLore.exe <file>                    open and play (same as double-click)
```

Format ids come from `--formats`; `--convert` dispatches to ffmpeg or the matching external encoder.

---

## How to verify this package

- **Not code-signed**: no certificate was purchased, so Windows SmartScreen may warn. Run it in a
  sandbox/VM if you prefer, or upload it to VirusTotal yourself (we do not fabricate a scan link).
- **Check SHA256** (PowerShell):

  ```powershell
  Get-FileHash .\ToneBox-1.8.0-win64.zip -Algorithm SHA256
  ```

  Compare it with the value above, character by character.
- **Build it yourself**: commands below; anyone can reproduce the package from the scripts in this repo.
- **Bundled ffmpeg**: built from FFmpeg sources by [`tools/build-ffmpeg.sh`](tools/build-ffmpeg.sh)
  (LGPL-2.1-or-later, no GPL components). The build script and third-party notices are in the repo.
- **CI today**: [`.github/workflows/ci.yml`](.github/workflows/ci.yml) runs `cargo test` on push/PR.
  **Release packages are currently built locally with `tools/package-release.ps1`** (commands below);
  CI-built releases are planned, and we will not put unverified claims in this README.

---

## Building

Rust with the MSVC or GNU toolchain (1.75+). The repo does not ship binaries; fetch the bundled tools first:

```powershell
# 1. fetch dependencies (ffmpeg / speex / wavpack / musepack)
powershell -ExecutionPolicy Bypass -File tools\fetch-ffmpeg.ps1
powershell -ExecutionPolicy Bypass -File tools\fetch-speex.ps1
powershell -ExecutionPolicy Bypass -File tools\fetch-wavpack.ps1

# optional: tools used by the extras package
powershell -ExecutionPolicy Bypass -File tools\fetch-atracdenc.ps1
powershell -ExecutionPolicy Bypass -File tools\fetch-psxavenc.ps1
powershell -ExecutionPolicy Bypass -File tools\fetch-vgaudio.ps1
powershell -ExecutionPolicy Bypass -File tools\fetch-qoa.ps1
powershell -ExecutionPolicy Bypass -File tools\fetch-vgmstream.ps1
powershell -ExecutionPolicy Bypass -File tools\fetch-openmpt123.ps1
powershell -ExecutionPolicy Bypass -File tools\fetch-optimfrog.ps1

# 2. build
cargo build --release -p fc-gui

# 3. tests (format table, argument mapping, warnings, container matrix, CUE parser)
cargo test -p fc-core --lib

# 4. package (version read from Cargo.toml)
powershell -ExecutionPolicy Bypass -File tools\package-release.ps1
powershell -ExecutionPolicy Bypass -File tools\package-extras.ps1
```

To rebuild the bundled ffmpeg (speech codecs + libgme): see
[`tools/build-ffmpeg.sh`](tools/build-ffmpeg.sh) and [`tools/build-ffmpeg.ps1`](tools/build-ffmpeg.ps1);
the first build takes roughly 20–40 minutes.

---

## Design rules

- **Local only**: no network, no upload, no DRM circumvention, no copyright-protection bypass
- **No pretending**: formats without an encoder are greyed out with a reason; nothing unsupported is
  listed as supported
- **Never modify your files**: only new files are written, unless you explicitly choose "delete source"
- **Clean licensing**: app is MIT; bundled ffmpeg is **LGPL-2.1-or-later** (no GPL components);
  `ofr.exe` is not redistributed

---

## Known issues

- 24-bit FLAC + trim/filters can make the bundled ffmpeg report `invalid block size`; a two-step
  workaround is built in
- Some Android recordings (.amr) have non-standard duration metadata and may display short
- ATRAC9 is read-only: atracdenc does not support it and patents are still in force
- Video containers keep only the first video stream; multi-angle video is not handled
- Chiptunes render for 3 minutes by default (NSF/SPC have no track length; libgme loops forever)
- **HE-AAC / xHE-AAC / AAC-ELD can be decoded** (native ffmpeg), **but not encoded**
  (encoding needs libfdk-aac, non-free)
- APE / TAK / DSD / AIFC / BW64 have no usable encoder and are read-only references

---

## License and third parties

- Application: **MIT** (see [`LICENSE`](LICENSE))
- Bundled ffmpeg: **LGPL-2.1-or-later**; license files kept, DLL replacement allowed, build scripts provided
- Other bundled/optional components (Speex, WavPack, libgme, libopenmpt, psxavenc, VGAudio, QOA,
  vgmstream, etc.): see [`THIRD_PARTY_NOTICES.md`](THIRD_PARTY_NOTICES.md)

---

## Changelog

### 1.7.1 (2026-11)
- Fixed Metadata content not sitting under its header: lyrics moved above Metadata, expanded fields now follow the header
- Fixed the Output summary pushing into the + Task button: truncates to the remaining width
- Fixed the cover row overflowing the right panel and opaque buttons: adaptive width, transparent/outlined buttons, wrapped hint

### 1.7.0 (2026-11)
- Output targets **107 → 112**: Argonaut ASF, AMV, SWF, GIF, APNG; ProRes / FFV1 / DNxHR / VP8 / MJPEG
  encoders for MOV/MKV/MXF/WebM/AVI
- **Double-click to play**: launching with a file path queues it and starts playback
- **"Open with" registration**: `--register` / Settings button; never silently steals the default
- **vgmstream input**: PSF / PS2 ADS / BRSTM / BCSTM / BFSTM / HCA are decoded to WAV first
- **openmpt123 input**: MOD / XM / IT / S3M modules can be dropped in directly
- **Post-conversion check**: every output is re-probed; broken files or wildly wrong durations fail loudly
- **ReplayGain tag writing**: source `replaygain_track_gain` is carried into FLAC / MP3 outputs
- **CUE splitting**: `--split-cue album.cue --to flac --out-dir out`, or the Settings button; writes
  track number / title / artist
- **English UI**: switch language in Settings, or launch with `--lang en`; the interface, format blurbs,
  content-health checks, quality warnings and encyclopedia articles are all available in English

### 1.6.1 (2026-11)
- Output targets **106 → 107**: N64 VADPCM (`.aifc`, external vadpcm, MPL-2.0)
- N64 VADPCM accepts 16-bit mono only; all 1.6 capabilities kept

### 1.6.0 (2026-11)
- Output targets **95 → 106**
- VCD / SVCD / PSP / iPod (M4V), IVF, H.264 / H.265 / MPEG-2 / MPEG-4 raw streams, AVIF, KVAG
- Video-only containers no longer get a fake audio track; VCD/SVCD force 352×288 / 480×576 and MP2 224k
- All 1.5 input support, PS1/PS2, Nintendo containers and QOA kept

### 1.5.0 (2026-11)
- Output targets **73 → 95**
- **Chiptunes as input**: ffmpeg with libgme; NSF / SPC / GBS / VGM / PSF and more
- PS1 XA-ADPCM / PS2 VAG (psxavenc), QOA (qoaconv), BRSTM / BCSTM / BFSTM / DSP / IDSP (VGAudioCli)
- New video containers: DV, GXF, NUT, MXF OPAtom, 3G2, F4V, ISMV, WTV
- New legacy audio: Westwood AUD, APM, ALP, SMAF (MMF); containers SoX, IRCAM
- Fixed CLI hard-coding `libx264` although the bundled build only has libopenh264
- `--version` / `--help` became real CLI arguments
- Extras package gained psxavenc / VGAudioCli / qoaconv; packaging emits `SHA256SUMS`

### 1.4.0 (2026-11)
- Output targets **69 → 73**: VOC, AAC-LATM / LOAS, MXF, Musepack
- Bundled Musepack encoder (LGPL-2.1), works out of the box

### 1.3.1 (2026-11)
- OptimFROG output via the official ofr.exe
- Fixed CLI `--convert` not supporting external encoders
- Added the optional extras package

### 1.3 (2026-11)
- Output targets **43 → 68**: video containers 8 → 13, GSM / AMR-WB / iLBC / LC3 / Codec2 / DFPWM,
  lossy WavPack, IEC 61937, SND
- Bundled ffmpeg switched to a self-built 8.0 (LGPL-2.1-or-later, 152 MB → 59.6 MB)

### 1.2
- More formats (Speex / AMR-NB / G.711 / G.722 / ADPCM / AU / RF64 / BWF)
- Seven sidebar groups, read-only formats always visible, spectrum analysis power saving

### 1.0
- First public beta: three-pane UI, queue conversion, format encyclopedia, music library

---

## Sponsoring

The project is free and open source; voluntary sponsorship does not affect anyone's rights under MIT.

<img width="1080" height="1620" alt="WeChat" src="https://github.com/user-attachments/assets/f53df0ad-01b1-4a4a-a9c2-c7db07b8cd3e" />
<img width="1242" height="1692" alt="Alipay" src="https://github.com/user-attachments/assets/9647d9f2-6607-46c3-93e8-423f9ebdf1c1" />
