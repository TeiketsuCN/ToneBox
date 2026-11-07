# 机器 B 接入指南

> 给另一台机器（内网）上的 DeepSeek Harness 用。
> 照着做，30 分钟内能编译、跑通、拿到第一个任务。

**主机：`192.168.31.192`（KOALA\23501）**

---

## 一、前置

- Windows 10/11
- 和主机在同一个内网（能 ping 通 `192.168.31.192`）
- 主机的共享用**专用本地账户** `fcsync`（密码见下），不是主账户 `23501`

> 为什么不用主账户：主机上的 `23501` 是**微软账户**（PrincipalSource = MicrosoftAccount），
> SMB 不接受 `KOALA\23501` + 微软密码，用户名得写成 `MicrosoftAccount\邮箱`，很难用。
> 所以专门建了一个本地账户 `fcsync` 只用于同步。

没有 Visual Studio / MSVC 也没关系，我们走 gnu 工具链。

---

## 二、连上主机的共享

主机已经建好两个共享：

| 共享 | 内容 | 权限 |
|---|---|---|
| `\\192.168.31.192\FormatConverterGit` | git 裸仓库 | 读写 |
| `\\192.168.31.192\FormatConverterSamples` | 样本数据（56 MB） | 只读 |

先连一次，会提示输入密码：

```powershell
net use \\192.168.31.192\FormatConverterGit /user:KOALA\fcsync Fc-2026-sync
```

验证：

```powershell
dir \\192.168.31.192\FormatConverterGit
```

---

## 三、装工具链

```powershell
# 1. Git（如果没装）
winget install --id Git.Git -e --silent

# 2. Rust（gnu 工具链，不是 msvc）
$dst = "$env:TEMP\rustup-init.exe"
Invoke-WebRequest "https://static.rust-lang.org/rustup/dist/x86_64-pc-windows-msvc/rustup-init.exe" -OutFile $dst
& $dst -y --default-toolchain stable-x86_64-pc-windows-gnu --profile minimal --no-modify-path

# 3. binutils（gnu 工具链缺 dlltool.exe，cpal 等 crate 的 build.rs 会用到）
winget install --id BrechtSanders.WinLibs.POSIX.UCRT -e --silent

# 4. ffmpeg（跑测试要用）
winget install --id Gyan.FFmpeg -e --silent
```

> rustup-init.exe 本身是 msvc 版，没关系——它只负责安装，
> 装出来的默认工具链是 `stable-x86_64-pc-windows-gnu`。

---

## 四、克隆

```powershell
# 先给 git 放行 UNC 路径（否则报 dubious ownership）
git config --global --add safe.directory '\\192.168.31.192\FormatConverterGit\FormatConverter.git'

git clone \\192.168.31.192\FormatConverterGit\FormatConverter.git D:\FormatConverter
cd D:\FormatConverter
```

---

## 五、构建

仓库里的 `build.ps1` 已经把 PATH 配好了，但 **WinLibs 的安装路径写死在里面**。
如果你的装到别处，改那一行：

```powershell
.\build.ps1            # debug
.\build.ps1 release    # release
```

**踩坑提醒**：PowerShell 5.1 读**无 BOM** 的 UTF-8 脚本会当 ANSI，中文全乱码然后解析失败。
仓库里的 `.ps1` 都带 BOM。你自己新写脚本时记得存成 UTF-8 with BOM。

---

## 六、验证（不开界面）

```powershell
robocopy \\192.168.31.192\FormatConverterSamples D:\FormatConverter\samples /E

.\target\debug\format-converter.exe --convert samples\mp3\pop48k_mp3_64k.mp3 D:\out.flac
.\target\debug\format-converter.exe --play samples\atrac\oma_at3p_01.oma 3
```

第一条打印 `输入: MP3 / 64 kbps ...` 和 `完成:`，第二条打印位置在推进，
就说明 ffprobe → ffmpeg → 声卡整条链路都通了。

---

## 七、你的第一个任务

**读 `docs/任务-01-频谱分析.md`**，照它做。

一句话：实现 `crates/fc-analyze`，把 Python 参考实现 `tools/analyze.py` 移植成 Rust，
在样本集上做到档位命中率 ≥ 85%。

---

## 八、你负责什么

| 路径 | 归属 |
|---|---|
| `crates/fc-analyze/**` | **你** |
| `crates/fc-core/src/format.rs` | **你**（加格式、改文案） |
| `crates/fc-core/src/ffmpeg.rs` | **你**（probe / convert） |
| `crates/fc-player/**` | 主机 |
| `crates/fc-gui/**` | **主机** ← 一行都别碰 |
| `docs/契约.md` | 双方，改之前先提 |

完整边界见 `docs/契约.md`。

---

## 九、分支与交付

```
main          受保护，只由主机合并
engine/*      你的活，比如 engine/spectrum
```

干完一块就 `git push origin engine/spectrum`，然后告诉主机"可以合并了"。

**不要 rebase 别人的提交，不要 force push，不要直接 push main。**

---

## 十、踩过的坑（省你两小时）

1. `-ss` / `atrim` 之后的音频喂给 flac 编码器 → `invalid block size: 8`
   解法：两段式，先截成 WAV 再从 WAV 编码。
2. eframe 的 glow 后端对 `from_rgba_unmultiplied` 会二次 gamma
   解法：所有半透明色用 `Color32::from_rgba_premultiplied`。
3. `DWMWA_SYSTEMBACKDROP_TYPE` 等毛玻璃接口会覆盖 per-pixel alpha，窗口变不透明
   解法：别用，我们走的是"画壁纸"路线，见 `docs/逆向-SaltPlayer.md`。
4. `TranscodedWallpaper` 没有扩展名，`image::open()` 会失败
   解法：`ImageReader::with_guessed_format()`。
5. PowerShell 5.1 + 无 BOM UTF-8 → 中文乱码导致脚本解析失败。
6. UNC 路径 clone 会报 dubious ownership → 加 `safe.directory`。

---

## 十一、连不上怎么办

按顺序排查：

```powershell
ping 192.168.31.192                     # 通不通
Test-NetConnection 192.168.31.192 -Port 445   # SMB 端口
net use                                 # 看已建立的连接
```

- ping 不通 → 网络/防火墙问题，不在这台机器上解决
- 445 不通 → 主机的防火墙规则没生效，让主机重启一下 `LanmanServer` 或重新加规则
- 能连但提示拒绝访问 → 密码不对，或用了错的用户名格式（必须是 `KOALA\23501`）

**兜底方案**：主机可以 `git bundle create repo.bundle --all`，用 U 盘拷过来
`git clone repo.bundle`。没有网络也能干活。
