# 待办队列（等机器 B 就绪后开工）

> 2026-11-08 记录。这些是已经讨论定稿、只等排期的改动，**代码一行没动**。

---

## 已定稿的决定

| # | 决定 | 结论 | 状态 |
|---|---|---|---|
| 1 | 字体 | **Noto Sans SC**（SIL OFL），内嵌 + 子集化 | 待做 |
| 2 | 窗口标题栏 | **L2 方案**（无边框 + 子类化 WndProc），高度 **44px** | 待做 |
| 3 | 置顶开关 | 标题栏右上角图钉，`ViewportCommand::WindowLevel` | 待做 |
| 4 | 试听交互 | **双击队列行播放**，移除详情面板的「试听」按钮 | 待做 |
| 5 | 播放控制 | **▶ 与 ⏸ 共用同一个按钮** + ⏹ 独立，图标用 painter 画 | 待做 |
| 6 | 停止行为 | 停止并**归零** | 待做 |
| 7 | 播放条位置 | 窗口底部全局条（跨三栏） | 待做 |

---

## 任务清单

### A. 字体（Noto Sans SC）

- [ ] 下载 Noto Sans SC Regular + Medium/Bold（SIL OFL）
- [ ] 写 `tools/subset-font.py`：从 `crates/fc-gui/src/**` 里扫出所有用到的字符 + ASCII + 常用标点
- [ ] 子集化，目标：每字重 ≤ 250 KB
- [ ] 放进 `crates/fc-gui/assets/`，`include_bytes!` 内嵌
- [ ] 替换 `install_fonts()` 里读系统字体的逻辑
- [ ] 数字开 `tnum`（等宽数字），不行就用等宽字体兜底
- [ ] 附 OFL 许可文本到 `assets/`，并在「关于」里标注

**为什么必须子集化**：完整 Noto Sans SC 一个权重 10 MB，两个 20 MB，exe 会从 3 MB 涨到 23 MB。

**注意**：`tnum` 特性在 egui 里能不能用需要先验证，不行就退化到等宽字体。

### B. 自绘标题栏（L2）

- [ ] `ViewportBuilder::with_decorations(false)`
- [ ] `SetWindowSubclass` 挂 `WM_NCHITTEST`：
  - 标题栏 44px 区域 → `HTCAPTION`（拖拽 / 贴边分屏 / 双击最大化全由系统处理）
  - 外侧 8px → `HTLEFT / HTRIGHT / HTTOP / HTTOPLEFT …`
  - 最大化状态下关闭边缘检测
- [ ] 补 `WS_THICKFRAME`（否则 resize 消息不来）+ `WM_NCCALCSIZE` 去掉视觉边框
- [ ] `DWMWA_WINDOW_CORNER_PREFERENCE = 2` 保证 Win11 圆角
- [ ] 右上角四个按钮：图钉 / 最小化 / 最大化 / 关闭
- [ ] 把现在 header 的内容（标题 + 副标题）挪进标题栏，省一行高度

**必须保住的原生行为**（验收清单）：
- 拖动窗口
- 拖到屏幕边缘 / 顶部 → 分屏
- 双击标题栏 → 最大化 / 还原
- 右键标题栏 → 系统菜单
- 从任意边缘缩放

### C. 置顶开关

- [ ] 图钉图标（painter 画，不依赖字体）
- [ ] 未固定：暗色空心；已固定：青色实心
- [ ] 切换用 `ViewportCommand::WindowLevel(WindowLevel::AlwaysOnTop / Normal)`
- [ ] 如果 0.29 这个命令不好用，退回已验过的 Win32 `SetWindowPos(HWND_TOPMOST)`

### D. 试听交互改造

- [ ] 队列行 `double_clicked()` → 播放该文件
- [ ] 删除详情面板里的「试听」按钮，换成一行提示"双击队列文件试听"
- [ ] 底部播放条：▶⏸ / ⏹ / 文件名 / 进度条（可点跳转）/ 时间 / 音量
- [ ] 正在播放的行加"播放中"指示（小三角 + 淡色进度）
- [ ] 播放中点别的行 → 播放继续，只切详情

---

## 建议顺序

```
1. 字体（独立，先做完先看到效果）
2. 标题栏 L2（牵扯最多 unsafe，单独一轮专门测）
3. 置顶开关（依赖标题栏）
4. 试听改造（纯 UI 逻辑）
```

字体和试听改造互不影响，可以穿插着做。标题栏那一步建议单独一轮、只做它，测透了再继续。

---

## 与机器 B 的依赖

**没有依赖。** 这四项全部在 `crates/fc-gui`（B 不碰）。唯一的外部文件是字体资源 `crates/fc-gui/assets/`，也是我这边管。

所以 B 在做频谱分析的同时，我这边可以并行推 UI，不会冲突。

---

## 给 B 的一个提醒

如果 B 卡在编译，九成是 `build.ps1` 里那条**写死的 WinLibs 路径**：

```powershell
"C:\Users\23501\AppData\Local\Microsoft\WinGet\Packages\BrechtSanders.WinLibs.*\mingw64\bin"
```

B 的用户名不是 `23501`，路径对不上。改成它自己的路径，或者让脚本自动找。

（这条也可以由我改成自动探测，等开工时顺手做掉。）
