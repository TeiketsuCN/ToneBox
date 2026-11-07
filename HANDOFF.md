# HANDOFF —— 新会话从这里开始

> 这份文件让你 30 秒接上全部上下文。**不需要读任何聊天记录**，所有决定都在这份
> 和它指向的文档里。
>
> 最近一次大改：2026-11-08 深夜。项目已从"两台机器并行"回到**单人开发**——
> 机器 B 已收工，交接在 `docs/留言/from-B.md` 最上面那条。

---

## 一句话

**格式转换器**：不是"转换工具"，是"**格式顾问**"。
转换只是入口；**判断**（这文件真实音质多少）+ **解释**（这格式是什么、有什么坑）才是产品价值。

Rust + egui（eframe 0.29 / glow），FFmpeg **子进程**，cargo workspace。
目标用户：MiniDisc / Hi-MD / 老 Walkman 用户，手上有一堆 `.oma` / `.at3`。

---

## 当前状态（全部可用）

| 模块 | 状态 |
|---|---|
| `fc-core` — 格式表 / probe / convert / tags / metadata / wiki / identify / encoders | ✅ |
| `fc-analyze` — 频谱分析 | ✅ 标定 60/60 |
| `fc-player` — 试听 | ✅ |
| `fc-gui` — 四栏界面 | ✅ 见下 |
| 安装包 | ✅ `D:\Apps\FormatConverter`，**157 MB**（自带 ffmpeg） |

**测试**：`cargo test -p fc-core --lib` = 23 个；`cargo test -p fc-analyze` = 标定 60/60。
**分支**：`main` 在 `a351810`，工作区干净，已 push。

---

## 界面长什么样

四栏：**输出格式（216） · 队列 · 详情（300） · 输出（272）**，窗口 1302×700。

- **侧栏**：无损 9 + 有损 8（MP3/Opus/Vorbis/MP2/AAC/AC-3/E-AC-3/WMA）+ 只读格式（暗色）
  + 设备专用（ATRAC 四条）。点开显示三行小字说明。
- **结果**：**缺少编码器的格式名中间画横线**并提示"你的机器上似乎没有这个编码器"。
- **队列**：36×36 封面 + 标签标题 + `艺术家 · 专辑 · MP3 · 323 kbps · 3:40`。
  行内 `×` 删除、`···` 选单文件输出格式（选了就把 `···` 换成格式名）。
- **详情**：标签标题 / 艺术家·专辑 / 小字文件名 + 技术参数 + 质量评估 + 警告卡片。
- **输出列**：预设（MD归档/车载/手机/再编辑）、参数旋钮、输出到、转换后、
  元数据（可折叠）、预计体积、**开始转换按钮钉在最下面**。
- **设置**：整页覆盖层（侧栏左下），含外观/行为/关于/支持开发。
- **格式百科**：侧栏标题右边「百科」按钮，左列表 + 右长文 + 搜索，29 条全有文案。

---

## 产品判断（别改这些，都是想过的）

| 决定 | 理由 |
|---|---|
| 全局一个输出格式 + 每文件覆盖 | 大多用户只想要一个结果 |
| 百科挂在侧栏小字上，**不做新窗口** | 用户当前任务是"挑格式"，不是"读文章" |
| 长文案只在百科页，**不进侧栏** | 80~150 字塞进 216px 会变成文字墙 |
| 播放 ▶/⏸ 共用一个按钮，⏹ 独立且归零 | |
| 不做「更多 / 收起」折叠 | 减小行高比折叠更直接 |
| 参数**立即生效**，没有「应用」按钮 | 转换还没开始，没有撤销成本 |
| **不写 DRM 解密代码** | 唯一法律红线，见 `docs/法律边界-重新评估.md` |
| 分析结果输出"不可判定"而不是硬猜 | Opus / HE-AAC 用截止频率判断必错 |

---

## 技术地基（改动前必读）

| 文档 | 内容 |
|---|---|
| `docs/契约.md` | 模块归属、`can_encode` / `primary` 语义 |
| `docs/待办-UI改造.md` | 全部界面决定 + **踩坑记录**（读它省几小时） |
| `docs/法律边界-重新评估.md` | 为什么打包 LGPL ffmpeg、为什么开放 AAC |
| `docs/样本清单.md` | 真实样本与实测截止频率 |
| `docs/逆向-SaltPlayer.md` | 毛玻璃方案来源 |
| `crates/fc-analyze/PERF.md` | 长文件采样策略 |
| `docs/留言/from-B.md` | 机器 B 的全部交付与**走前三点提醒** |

### 几条硬约束

1. **颜色必须用 `Color32::from_rgba_premultiplied`**。用 unmultiplied 会被 eframe 二次 gamma。
2. **顶部 44px 是原生命中区**（`WM_NCHITTEST` 返回 `HTCAPTION`），
   放在那里的按钮点不到，覆盖层内容要下移 44px。
3. **整行点击会吃掉行内按钮**：`ui.interact(整行)` 注册在按钮之后会盖住它们。
   解法是整行矩形从按钮左边界截断。
4. **PowerShell 脚本必须 UTF-8 with BOM**，否则 PS 5.1 按 ANSI 解析中文会炸。
5. **PS 5.1 的 `Get-Content` 会把 UTF-8 当 GBK 读**，改 `config.json` 要用
   `[System.IO.File]::ReadAllText($p, [Text.Encoding]::UTF8)`。
6. **新版格式必须同步更新 `format/mod.rs` 的 `encoder_for()`**，
   否则运行时探测判成"没有编码器"，界面画横线把功能挡住。
7. **验证编码能力要用随包那份 ffmpeg**（`D:\Apps\FormatConverter\ffmpeg\ffmpeg.exe`），
   不是开发机 PATH 里那份——两份构建的编码器集合不一样。

---

## 构建 / 安装

```powershell
# 构建（需要 WinLibs 的 dlltool，build.ps1 里已处理）
powershell -NoProfile -ExecutionPolicy Bypass -File D:\FormatConverter\build.ps1

# 安装（含随包 ffmpeg）
powershell -NoProfile -ExecutionPolicy Bypass -File D:\FormatConverter\tools\install.ps1

# ffmpeg 首次拉取（约 73 MB → 解压 152 MB，进 vendor/，不进 git）
powershell -NoProfile -ExecutionPolicy Bypass -File D:\FormatConverter\tools\fetch-ffmpeg.ps1
```

**命令行**（不起界面，方便排查）：

```
format-converter --tools              # 当前用的 ffmpeg/ffprobe 路径
format-converter --identify <文件>     # 是不是加密的、哪家的
format-converter --tags <文件>         # 标签与封面
format-converter --convert in out     # 转 FLAC
format-converter --play <文件>         # 试听
```

---

## 还没做的（按优先级）

1. **码率旋钮**：AC-3/E-AC-3/WMA 现在写死 448/640/192 kbps，没有可调
2. **百科页直接设为输出目标**：现在只能看，不能"从这个格式转"
3. **ATRAC 结论要更保守**：16 个样本 10 档、6 档只有 1 个，没有留出集
   （B 的提醒，见 from-B.md 走前第三条）
4. **HE-AAC / xHE-AAC 没有真实样本**，只验证了 profile 分支
5. **exe 图标和版本资源**已有；Inno Setup 安装器还没做

---

## 对外口径（别说错）

分析结果**不是"准确率 100%"**。

正确说法：

> 标定集 60/60。按母版留出验证是 29/32，加入区间规则后同一批 32/32——
> 但那是**覆盖已知失败**，不是泛化准确率。Opus 和 HE-AAC 会明确输出"不可判定"。

---

## 风格

- 中文注释，写"**为什么**"不写"做了什么"
- 提交信息用中文，一句话说清改了什么
- 踩到的坑写进 `docs/待办-UI改造.md` 的坑列表，不要只留在脑子里
- 宁可显示"不可判定"，也不要给一个自信的错误答案
