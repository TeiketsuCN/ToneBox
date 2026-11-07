# 逆向：Salt Player for Windows 的毛玻璃是怎么做的

> 2026-11-08。目的：搞清楚参考品的实现路径，决定我们自己的方案。
> 方法：进程枚举 → 模块列表 → 目录结构 → 二进制字符串扫描 → 窗口样式探查。

---

## 一、它是什么

| 项 | 值 |
|---|---|
| 名称 | Salt Player for Windows 1.19.3 |
| 发行 | Steam（`D:\Program Files (x86)\Steam\steamapps\common\Salt Player for Windows`） |
| 体积 | 337 MB |
| 包名 | `com.moriafly.salt` |

## 二、技术栈（从文件推出来的）

```
Salt Player for Windows.exe        0.5 MB  jpackage 启动器
app\
  Salt Player for Windows.cfg              mainclass=MainKt（Kotlin）
  ffmpeg-x64.dll                 106.2 MB  解码
  skiko-windows-x64.dll           12.7 MB  Skia 的 Kotlin 绑定 → Compose Multiplatform
  icudtl.dat                       3.7 MB
  voxzen.aot                     120.3 MB  JDK AOT 缓存（-XX:AOTCache）
  resources\
    bass*.dll                              音频引擎（BASS）
    bassasio / basswasapi                  输出后端
    bassflac / bassopus / bassdsd          解码器
    MACDll64.dll                           Monkey's Audio
    steam_api64.dll                        Steam 集成
    FluentLib.dll                          Win32 辅助（导入 DwmSetWindowAttribute）
    bg_wallpaper.jpg               0.2 MB  内置兜底壁纸（蓝色缎带）
runtime\                                     jlink 出来的 JRE
```

进程实际加载：`jvm.dll` `skiko-windows-x64.dll` `d3d12.dll` `dwmapi.dll` `opengl32.dll` `uxtheme.dll`

## 三、关键结论：毛玻璃是"画"出来的，不是系统给的

**窗口样式：**

```
Class   : SunAwtFrame
Style   : 0x16C70000
ExStyle : 0x00000100  (WS_EX_WINDOWEDGE)
WS_EX_LAYERED            = False
WS_EX_NOREDIRECTIONBITMAP = False
```

**这是一个完全普通的不透明窗口，物理上不可能透出桌面。**

**但 AOT 缓存里出现了这些符号：**

```
dev.chrisbanes.haze            HazeBlurStyle, blurRadius, noiseFactor, RenderEffectParams
io.github.compose-fluent       WindowsAcrylic, WindowsMica, WindowsMicaAlt, LocalWindowAcrylicContainer
com.moriafly.salt.ui.material  MaterialType.Acrylic / BlurryGlass
org_jetbrains_skiko_skiko
```

`dev.chrisbanes.haze` 是 Compose Multiplatform 上有名的"玻璃拟态"库，
它在**合成层内部**对内容做实时模糊（Skia RenderEffect），跟窗口透明毫无关系。

**所以它的做法是：**

1. 读系统壁纸（`%APPDATA%\Microsoft\Windows\Themes\TranscodedWallpaper`，或注册表原图）
2. 在应用内当背景画出来，按窗口位置取偏移
3. 用 Haze 对背景做模糊 + 噪点
4. 上面叠半透明卡片
5. 窗口本身完全不透明

`bg_wallpaper.jpg` 是"读不到壁纸时的兜底"。

它另外还提供 Windows Acrylic / Mica 模式（`compose-fluent` + `FluentLib.dll` 调 `DwmSetWindowAttribute`），
那是设置里的可选项，不是默认路径。

## 四、为什么我们不照抄 DWM 那条路

我们试过三种系统级做法，全部失败：

| API | 结果 |
|---|---|
| `DWMWA_SYSTEMBACKDROP_TYPE`（Win11 亚克力/Mica） | 窗口变成完全不透明，per-pixel alpha 失效 |
| `ACCENT_ENABLE_ACRYLICBLURBEHIND`（经典亚克力） | 同上 |
| `DwmExtendFrameIntoClientArea(-1)`（Aero 玻璃） | 本环境（glow + winit）下无效 |

根因：eframe/glow 的透明窗口和 DWM 的合成接口互斥。

**而 Salt Player 用的就是第二条路（画背景），跟我们的选择一致。**

## 五、我们采用的做法（`crates/fc-gui/src/bg.rs`）

1. 壁纸来源优先级：`TranscodedWallpaper` → 注册表 `HKCU\Control Panel\Desktop\WallPaper` → 系统默认 `img0.jpg`
2. 解码：`ImageReader::with_guessed_format()`（**注意：TranscodedWallpaper 没有扩展名，`image::open` 会失败**）
3. 缩小 8 倍 → 3 次盒式模糊 → 放大 2 倍（线性过滤再补一次糊）
4. 上传成 egui 纹理
5. 每帧画在最底层，**UV 按窗口屏幕坐标偏移** —— 所以窗口拖动时背景跟着桌面走
6. 压一层 `rgba(8,10,16,96)` 保证任何壁纸上文字都够对比

**性能关键：壁纸只模糊一次。** 窗口移动只改 UV，不重算模糊，开销接近零。

## 六、已知局限

- 壁纸是"幻灯片放映"或纯色时，只能回退到默认图
- 多显示器、不同 DPI 混合时 UV 会有偏差（目前只按主屏算）
- 窗口跨屏拖动时背景会错位，属于可接受的瑕疵
