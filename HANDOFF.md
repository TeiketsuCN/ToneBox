# HANDOFF —— 新会话从这里开始

> 这份文件让你 30 秒接上全部上下文。**不需要读任何聊天记录**，所有决定都在这份
> 和它指向的文档里。
>
> 最近一次大改：2026-11-08 深夜。项目已从"两台机器并行"回到**单人开发**——
> 机器 B 已收工，交接在 `docs/留言/from-B.md` 最上面那条。
>
> 2026-11-08 晚补的一轮：AC-3 / E-AC-3 / WMA 码率旋钮、百科页直接设为输出格式、
> 体积估算改成按码率算、ATRAC 结论压保守（见"还没做的"上面那节）。

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

**测试**：`cargo test -p fc-core --lib` = 33 个；`cargo test -p fc-analyze` = 标定 60/60（约 55 秒）。
**分支**：`main` 工作区干净，已 push。

---

## 界面长什么样

四栏：**输出格式（216） · 队列 · 详情（300） · 输出（272）**，窗口 1302×700。

- **侧栏**：无损 9 + 有损 8（MP3/Opus/Vorbis/MP2/AAC/AC-3/E-AC-3/WMA）+ 只读格式（暗色）
  + 设备专用（**ATRAC1 / ATRAC3 / ATRAC3+ 装了外部的 atracdenc 才能选**；ATRAC AL、ATRAC9 只读）。
  点开显示三行小字说明；缺编码器/缺外部工具的格式名画横线，悬停说明缺什么。
- **结果**：**缺少编码器的格式名中间画横线**并提示"你的机器上似乎没有这个编码器"。
- **队列**：36×36 封面 + 标签标题 + `艺术家 · 专辑 · MP3 · 323 kbps · 3:40`。
  行内 `×` 删除、`···` 选单文件输出格式（选了就把 `···` 换成格式名）。
- **详情**：标签标题 / 艺术家·专辑 / 小字文件名 + 技术参数 + 质量评估 + 质量风险提示卡片
  （降码率、无损转有损、升采样、抄大容器——文案和判断都在 core 的 `quality_warnings`）。
- **输出列**：预设（MD归档/车载/手机/再编辑）、参数旋钮（FLAC 等级 / 位深 / MP3·Opus·AAC·
  **AC-3·E-AC-3·WMA 码率** / **采样率 自动·44.1k·48k·96k·192k，按格式给档位**）、输出到、
  转换后、元数据（可折叠）、预计体积、开始转换按钮。
- **设置**：整页覆盖层（侧栏左下），含外观/行为/关于/支持开发。关于里有
  「ATRAC 编码 = atracdenc（LGPL-2.1+，需自行安装）」、检测状态和「重新检测外部工具」。
- **格式百科**：侧栏标题右边「百科」按钮，左列表 + 右长文 + 搜索，29 条全有文案。
  打开时对准当前输出格式；看完可以直接「把输出格式设为 X」回主界面。

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
| 码率档位表放 core（`bitrate_choices`） | GUI 只照着画按钮；界面里写死 `[128, 192, 256, 320]` 算违规 |
| 有固定码率的格式按「码率 × 时长」估体积 | AC-3 448k 就是 448 kbps，用 PCM × size_ratio 会差三倍 |
| 百科能直接「设为输出格式」 | 用户此刻的任务就是挑格式，别让他关掉百科回侧栏再找一遍 |
| ATRAC 单样本档位给区间、置信度上限 0.60 | 16 样本 10 档、6 档只有 1 个、没有留出集，不能装得和 MP3 一样确定 |
| 采样率可选，但**按格式给档位** | Opus 输出永远 48k、AAC 只到 96k、Dolby 只到 48k，给错档要么失败要么被偷偷改掉 |
| 升采样要明说"不会增加信息" | 顾问不能帮着把文件变大；44.1k→96k 体积翻倍、内容一样 |
| 质量提示放 core（`quality_warnings`） | 界面只画卡片；判断和文案归引擎，和格式表同一条规矩 |
| **ATRAC 编码走外部工具，不打包** | atracdenc（LGPL-2.1+）由用户自己装；我们只检测+调用。分发编码器才是专利风险，调用不是 |
| ATRAC9 不开放 | 专利 2033 前后才到期，atracdenc 也不支持，继续只读 |
| 外部工具探测和内置编码器同一套 UI 语言 | 缺了就画横线 + 说明缺什么，不让用户点下去才炸 |

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

1. **HE-AAC / xHE-AAC 没有真实样本**，只验证了 profile 分支
2. **ATRAC 样本不够**：16 个文件 10 档、6 档只有 1 个，没有留出集。
   结论已经压保守（区间 + caveat + 置信度 0.60），但要真正验证还得再收来源
3. **质量提示只比原始码率，没有按编码器效率折算**：Opus 96 和 MP3 96 不是一回事，
   现在会被判成"同码率"不提醒；同格式同码率重转（二次编码损耗）也没提示。
   要做得更准，需要一张编码器效率系数表（可先只做 Opus/AAC/MP3 三家）
4. **Inno Setup 安装器**还没做（发布期才需要）；exe 图标和版本资源已有
5. **代码签名**没有（证书约 ¥1000~2000/年，免费项目先不签，README 里写清楚）

已经做完、不要重复做：AC-3 / E-AC-3 / WMA 码率旋钮、百科页设为输出格式、
体积估算按码率算、采样率档位、质量风险提示、ATRAC 外部编码器通道。

**ATRAC 的对外口径**（别说过头）：装了 atracdenc 才能转 ATRAC，没装只能解码；
编码器不随本程序分发。而且**转出来的 `.oma` 没有 OpenMG 包装，不能直接塞回 Hi-MD**——
要灌 MD 仍然只有索尼原厂工具或硬件回录。

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
