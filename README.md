# 格式转换器 · MVP

一个"格式顾问"，不是格式转换工具。转换只是入口，**判断 + 解释**才是产品价值。

当前版本：**MP3 → FLAC**（任意可解码格式 → FLAC），半透明玻璃 UI，Rust + egui。

---

## 这版做了什么

| 能力 | 状态 |
|---|---|
| 拖拽 / 选择文件 | ✅ |
| ffprobe 读取真实参数（编码、码率、采样率、声道、时长、体积） | ✅ |
| 有损源警告（转 FLAC 不会增加信息量） | ✅ |
| ffmpeg 子进程转 FLAC，实时进度 | ✅ |
| 加密文件识别 + 说明（不尝试解密） | ✅ |
| 命令行模式（`--convert`） | ✅ |
| 命令行传入文件直接打开 | ✅ |

---

## 一条设计红线

**遇到加密文件，只识别、只解释、不解密。**

代码里不允许出现任何 OpenMG / MagicGate / NCM / QMC 相关的解密逻辑，
也不需要——因为产品的主张是"告诉你为什么打不开，以及合法出路是什么"。

---

## 构建

本机没有 MSVC，用的是 **gnu 工具链 + WinLibs binutils**。

```powershell
# 一次性准备
rustup toolchain install stable-x86_64-pc-windows-gnu
winget install --id BrechtSanders.WinLibs.POSIX.UCRT -e

# 平时构建（脚本已把 PATH 配好）
.\build.ps1           # debug
.\build.ps1 release   # release
```

产物：`target\release\format-converter.exe`

---

## 用法

```powershell
# GUI
format-converter.exe
format-converter.exe "某个音频.mp3"

# 命令行（调试 / 批量）
format-converter.exe --convert 输入.mp3 输出.flac
```

ffmpeg 查找顺序：程序同目录 → 同目录 `ffmpeg\` → WinGet Links → `C:\ffmpeg\bin` → PATH。

---

## 已知问题 / 踩过的坑

1. **半透明在本机某些配置下渲染为不透明**
   `DWMWA_SYSTEMBACKDROP_TYPE`（亚克力/Mica）、`SetWindowCompositionAttribute`
   的 `ACCENT_ENABLE_ACRYLICBLURBEHIND`、`DwmExtendFrameIntoClientArea(-1)`
   三者任意一个都会覆盖 winit 的 per-pixel alpha，让窗口变全不透明。
   当前策略：只设深色标题栏，保留 `with_transparent(true)`，其余全部禁用。
   要毛玻璃就得换 Tauri + WebView2（CSS backdrop-filter）。

2. **eframe 0.29 的 glow 后端对 `from_rgba_unmultiplied` 会二次 gamma**
   实测 alpha=10 被渲染成 55% 白，alpha≈140。
   所有半透明色必须走 `Color32::from_rgba_premultiplied`，见 `tint()`。

3. **ffmpeg 9.0.1 + 24bit FLAC + `-ss`/滤镜 → "invalid block size: 8"**
   带定位或 atrim 的音频喂给 flac 编码器会直接失败。
   解法：两段式，先截成 WAV，再从 WAV 编码。

4. **PowerShell 5.1 读无 BOM 的 UTF-8 脚本会当 ANSI**，中文变乱码导致解析失败。
   本仓库的 `.ps1` 一律带 BOM。

---

## 下一步

- [ ] 频谱质量评估接进 GUI（分析器已经在 `tools/analyze.py` 跑通）
- [ ] DRM 指纹库（`docs/音频格式总览.md` 里的 C 级格式）
- [ ] 格式百科文案
