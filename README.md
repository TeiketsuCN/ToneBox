# 声箱 ToneBox

> 一个讲道理的音频格式转换器：先告诉你"这么转会损失什么"，再让你决定。

![版本](https://img.shields.io/badge/版本-1.3.1-2ea44f)
![平台](https://img.shields.io/badge/平台-Windows%2010%2F11%20x64-0078d4)
![许可](https://img.shields.io/badge/许可-MIT-blue)
![ffmpeg](https://img.shields.io/badge/ffmpeg-8.0%20LGPL%20(随包)-orange)

![界面截图](docs/screenshot.jpg)

---

## 下载

| 版本 | 安装包 | 大小 | SHA256 |
|---|---|---|---|
| **1.3.1** | [ToneBox-1.3.1-win64.zip](https://github.com/TeiketsuCN/ToneBox/releases/download/v1.3.1/ToneBox-1.3.1-win64.zip) | 27.9 MB | `1F63CD6E0DFDC9EEDAF76588FE8431D373D56CD204C275517D3F7671FDF5F947` |
| 1.3.1 可选工具 | [ToneBox-1.3.1-extras-win64.zip](https://github.com/TeiketsuCN/ToneBox/releases/download/v1.3.1/ToneBox-1.3.1-extras-win64.zip) | 0.15 MB | `91AAE1390357E4B75AA83768970967DBDC9156C363BD277EA51C75A672A21636` |

- Windows 10 / 11 x64，解压即用，**不需要安装**
- 解压后双击 `format-converter.exe`
- 不要单独移动或删除 `ffmpeg\`、`speex\`、`wavpack\` 三个目录

### 可选外部工具包（单独下载）

主包里**不含**两个有分发限制的编码器，需要的另下 `ToneBox-1.3.1-extras-win64.zip`：

| 工具 | 包内形式 | 用途 |
|---|---|---|
| **atracdenc** | 直接带 `atracdenc\atracdenc.exe`（LGPL-2.1） | ATRAC1 / ATRAC3 / ATRAC3+ 输出 |
| **OptimFROG** | 带 `install-ofr.cmd` 一键脚本 | OptimFROG 输出 |

用法：把 extras 包**解压到 ToneBox 目录**（和 `format-converter.exe` 同级），ATRAC 立刻可用；
OptimFROG 需要双击一次 `install-ofr.cmd`，脚本会从官网下载并用官方 SHA-1 校验（不涉及再分发）。
- 卸载＝删掉这个文件夹：不写注册表、不装服务

> 这是测试性质的软件。转换前请备份原文件；设备和文件损坏请自己负责。

---

## 它做什么

- **转格式**：一次拖多个文件，统一目标格式，也可以给单个文件单独指定
- **讲清楚代价**：转之前就告诉你"降采样会切掉多少高频""升位深只是补零""有损转无损不会变好"
- **给出参数**：码率、位深、采样率、声道、压缩等级，每个格式只列它真正支持的
- **管理音乐库**：歌曲/专辑/风格/艺术家，只读扫描，不碰你的文件

**它不做什么**：不解 DRM、不联网、不上传文件、不做流媒体下载。全部本地处理。

---

## 支持的格式

当前**可输出格式 69 个**，另有约 50 个只读参考格式（能识别、能解释，但本程序不提供编码器）。

| 类别 | 可输出 |
|---|---|
| 无损 | FLAC、WAV、ALAC、AIFF、WavPack、TTA、CAF、W64、裸 PCM、TrueHD、MLP、Ogg FLAC、RF64、BWF、**OptimFROG**（需自装 ofr.exe） |
| 有损 | MP3、Opus、Vorbis、MP2、AAC、WMA v1/v2、Musepack、aptX / aptX HD / SBC、ADX、Nellymoser、WavPack 有损 |
| 空间音频 | AC-3、E-AC-3、AC-4（直通）、DTS、AAC 5.1 |
| 语音/电话 | G.711 A/μ、G.722、G.723.1、G.726、GSM 06.10、AMR-NB、AMR-WB、iLBC、LC3、Codec2、Speex、ADPCM 系列、DFPWM、RealAudio 1.0 |
| 音频容器 | WAV、AU / SND、AIFF、CAF、W64、IEC 61937（S/PDIF 直通）、RF64、BWF |
| 视频容器 | MKV、MP4、WebM、MOV、TS、M2TS、FLV、AVI、**MPEG-PS、VOB、OGV、WMV、3GP** |
| 设备专用 | ATRAC1、ATRAC3、ATRAC3+（需自行安装 atracdenc） |

视频容器支持：保留第一条画面 + 全部音轨，画面可复制或转 H.264 / H.265 / VP9 / AV1 / MPEG-2 / MPEG-4 / Theora / WMV2，音轨逐条决定复制还是重编码。

**只读格式**（能解析、能解释为什么不能输出）：APE、TAK、Shorten、WMA Lossless、DSD（DSF/DFF/DST）、DTS-HD/X、MPEG-H、USAC、EVS、LC3plus、TwinVQ、RealAudio Cook 等。

---

## 外部工具

| 工具 | 是否随包 | 用途 |
|---|---|---|
| `ffmpeg` / `ffprobe` | ✅ 随包 | 解码、编码、探测（LGPL v3 自建，带 libgsm / vo-amrwbenc / libilbc / liblc3 / codec2） |
| `speexenc` | ✅ 随包 | Speex 输出（BSD） |
| `wavpack` | ✅ 随包 | WavPack **有损**输出（hybrid 模式，ffmpeg 自带的编码器只做无损） |
| `atracdenc` | 📦 在 extras 包里 | ATRAC1 / ATRAC3 / ATRAC3+ 输出（LGPL-2.1） |
| `ofr` | 📦 extras 里一键下载 | OptimFROG 输出（官方许可限制再分发，脚本代下载） |

程序会自动在「程序目录 → 程序目录子目录 → PATH」里找这些工具。装了新的工具后，到**设置 → 依赖 / 外部工具 → 重新检测**即可，不用重启。

---

## 界面

- 左侧：输出格式（按类别折叠，只读格式灰显）+ 格式说明
- 中间：文件队列（拖进来就行），批量进度、失败原因逐条显示
- 右侧：选中文件的真实参数、质量警告、输出参数
- 顶部：设置、任务活动、转换进度；F11 / Esc 切换全屏
- 支持多套配置切换、导出导入；内置常用预设（MD 归档 / 车载 / 手机 / 再编辑）

---

## 构建

需要 Rust 的 MSVC 或 GNU 工具链（1.75+）。仓库不含二进制依赖，首次构建前先抓取随包工具：

```powershell
# 1. 抓依赖（ffmpeg / speex / wavpack / 测试样本）
powershell -ExecutionPolicy Bypass -File tools\fetch-ffmpeg.ps1
powershell -ExecutionPolicy Bypass -File tools\fetch-speex.ps1
powershell -ExecutionPolicy Bypass -File tools\fetch-wavpack.ps1

# 2. 编译
cargo build --release -p fc-gui

# 3. 跑测试（格式表、参数映射、警告逻辑）
cargo test -p fc-core --lib

# 4. 打包（版本号从 Cargo.toml 读）
powershell -ExecutionPolicy Bypass -File tools\package-release.ps1
```

想自己重新构建随包 ffmpeg（比如加别的编码库）：见 [`tools/build-ffmpeg.sh`](tools/build-ffmpeg.sh) 与 README 末尾的说明；首次约 20~40 分钟。

---

## 仓库结构

```
crates/
  fc-core/      格式表、参数、警告、ffmpeg/外部工具调用（纯逻辑，可测）
  fc-analyze/   频谱、响度、波形分析
  fc-player/    播放（rodio/cpal）
  fc-gui/       egui/eframe 界面（三栏 + 过场 + 播放器）
tools/          抓取、构建、打包、安装脚本
docs/           设计笔记、格式总览、法律边界等
```

---

## 设计红线

- **只做本地处理**：不联网、不上传、不解 DRM、不绕过任何版权保护
- **不假装**：没有编码器的格式就灰着并说明原因，不给假的可用状态
- **不改你的文件**：除非你明确选择「删除源文件」，否则只写新文件
- **许可干净**：程序本体 MIT，随包 ffmpeg 是 LGPL v3（不含 GPL 组件）

---

## 已知问题

- FLAC + 24bit + 截取/滤镜的组合，随包 ffmpeg 可能报 `invalid block size`：程序已内置两段式（先截成 WAV 再编码）规避
- 部分 Android 手机的录音（.amr）时长元数据不规范，显示可能偏短
- ATRAC9 只读：atracdenc 不支持，专利也在保护期
- 视频容器输出会保留第一条画面；多画面（多角度）不处理

---

## 许可与第三方

- 程序本体：**MIT**（见 [`LICENSE`](LICENSE)）
- 随包 ffmpeg：**LGPL-2.1-or-later**，按 LGPL 要求保留许可文件、允许替换 DLL、提供构建脚本
- 其他随包组件（Speex / WavPack / 各编解码库）的许可与来源见 [`THIRD_PARTY_NOTICES.md`](THIRD_PARTY_NOTICES.md)

---

## 更新日志

### 1.3.1（2026-11）
- 新增 **OptimFROG** 输出（走官方 ofr.exe，压缩预设 0–10 可调）
- 修复命令行 `--convert` 不支持外部编码器（ATRAC / Speex / WavPack 有损 / OptimFROG）的问题
- 新增**可选外部工具包**（extras）：atracdenc 直接带，OptimFROG 一键下载脚本

### 1.3（2026-11）
- 可输出格式 **43 → 68**：视频容器 8 → 13（MPEG-PS / VOB / OGV / WMV / 3GP）、新增 GSM / AMR-WB / iLBC / LC3 / Codec2 / DFPWM、WavPack 有损、IEC 61937、SND
- 随包 ffmpeg 换成自建 8.0（LGPL v3，152 MB → 59.6 MB）
- 分组理顺（空间音频 / 语音）、设置页新增依赖面板、徽章按运行时可用性着色
- 新格式补齐参数旋钮（AMR/G.723/G.726/WMA v1 码率、MLP 位深、Ogg FLAC 压缩等级、WavPack 有损码率）

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
