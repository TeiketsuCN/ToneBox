# 机器 B 接入指南

给另一台机器上的 DeepSeek Harness 用。照着做，20 分钟内能编译并跑起来。

---

## 一、前置

- Windows 10/11
- **没有** Visual Studio / MSVC 也没关系，用 gnu 工具链
- 需要联网（第一次拉依赖）

---

## 二、装工具链

```powershell
# 1. Rust（gnu 工具链，不是 msvc）
$dst = "$env:TEMP\rustup-init.exe"
Invoke-WebRequest "https://static.rust-lang.org/rustup/dist/x86_64-pc-windows-msvc/rustup-init.exe" -OutFile $dst
& $dst -y --default-toolchain stable-x86_64-pc-windows-gnu --profile minimal --no-modify-path

# 2. binutils（gnu 工具链缺 dlltool.exe，有些 crate 的 build.rs 会用到）
winget install --id BrechtSanders.WinLibs.POSIX.UCRT -e --silent
```

> ⚠️ rustup 的 rustup-init.exe 本身是 msvc 版，这没关系——它只负责安装，
> 装出来的目标工具链是 `stable-x86_64-pc-windows-gnu`。

---

## 三、拉代码

```powershell
git clone \\192.168.31.192\D$\git\FormatConverter.git D:\FormatConverter
cd D:\FormatConverter
```

需要和主机 **同一个 Windows 账号**（或先用 `net use \\192.168.31.192\D$ /user:23501`）。

拉不到就走云仓库或者让对方 `git bundle` 给你。

---

## 四、构建

仓库根目录的 `build.ps1` 已经把 PATH 配好了（含 WinLibs 路径）。
**如果你的 WinLibs 装到别的位置，改一下 `build.ps1` 里那一行**。

```powershell
.\build.ps1            # debug
.\build.ps1 release    # release，产物 target\release\format-converter.exe
```

> 注意：PowerShell 5.1 读**无 BOM** 的 UTF-8 脚本会当 ANSI，中文全乱码然后解析失败。
> 仓库里的 `.ps1` 都带 BOM。你自己新写脚本时记得存成 UTF-8 with BOM。

---

## 五、验证

不需要 GUI，直接跑命令行模式：

```powershell
.\target\debug\format-converter.exe --convert 某个.mp3 out.flac
```

打印出 `输入: MP3 / ...` 和 `完成:` 就说明整条链路（ffprobe → ffmpeg → 进度回调）是通的。

---

## 六、你负责什么

**只改这三个地方：**

1. `crates/fc-analyze/**` —— 频谱分析，当前是 stub，等你实现
2. `crates/fc-core/src/format.rs` —— 格式表（加格式、改文案、补专利状态）
3. `crates/fc-core/src/ffmpeg.rs` —— probe / convert

**不要改 `crates/fc-gui/**`**，那是界面，两边同时改必冲突。

完整边界见 `docs/契约.md`。

---

## 七、样本（不进 git，自己生成）

仓库 `gitignore` 了 `samples/` 和 `out/`，因为体积大。需要时：

```powershell
# 真实 ATRAC 位流（FFmpeg 官方公开样本库，含 .oma / .at3 / .aa3 / .aea）
powershell -ExecutionPolicy Bypass -File tools\fetch-samples.ps1

# 用本机无损母版生成 44 个已知码率的对照样本（需要改 SOURCES 里的路径）
python tools\build-corpus.py
```

`tools/analyze.py` 是频谱分析的**参考实现**，已经标定过，先跑它再移植。

---

## 八、分支约定

```
main        受保护，只有主机的 GUI 提交能直接进
engine/*    机器 B 的活，比如 engine/spectrum
```

干完一块就 `git push origin engine/spectrum`，然后告诉主机合并。
**不要 rebase 别人的提交，不要 force push。**

---

## 九、踩过的坑（省你两小时）

1. `-ss` / `atrim` 后的音频喂给 flac 编码器 → `invalid block size: 8`
   解决：两段式，先截成 WAV，再从 WAV 编码。
2. eframe 的 glow 后端对 `from_rgba_unmultiplied` 会二次 gamma
   解决：所有半透明色用 `Color32::from_rgba_premultiplied`。
3. `DWMWA_SYSTEMBACKDROP_TYPE` 等毛玻璃接口会覆盖 per-pixel alpha，窗口变不透明
   解决：不要用，见 `docs/契约.md` 和 README。
4. PowerShell 5.1 + 无 BOM UTF-8 → 中文乱码导致脚本解析失败。
