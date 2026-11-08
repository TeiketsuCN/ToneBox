# 发布 Beta 1.0（Windows）

## 这一版是什么

ToneBox 测试版 beta 1.0。首次公开的 Windows 构建，重点是把「格式判断 + 解释」和播放/设备链路跑通；
**不是稳定版**，转码和播放前请自行备份文件。

> 建议所有操作备份 · 慎防设备损坏，请自己负责

## 发布前检查

- `cargo test -p fc-core --lib`（65 条）
- `cargo test -p fc-player --lib`（6 条）
- `powershell -File build.ps1` 退出码 0
- 用 `tools\install.ps1` 安装后手动过一遍：
  - 拖入 FLAC/MP3/视频，详情页不空白
  - 播放/暂停/拖动进度/换输出设备
  - 引擎和设备：设备、蓝牙、引擎三页
  - 播放栏：拖动、左右拉伸、上下拉伸、收起/恢复
  - 设置里小/中/大窗口切换
- 清空个人配置（见下），再用全新配置启动一次，确认默认主题「极光」、默认窗口「大」。

## 清空个人配置（打包/截图前）

```powershell
$cfg = "$env:APPDATA\FormatConverter"
Remove-Item "$cfg\config.json*" -Force -ErrorAction SilentlyContinue
```

不会删音乐文件；只删本机配置（窗口、主题、设备选择、音乐库索引）。

## 打 Windows 发布包

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File tools\fetch-ffmpeg.ps1   # 若 vendor\ffmpeg 不存在
powershell -NoProfile -ExecutionPolicy Bypass -File tools\package-release.ps1
```

产物：`dist\ToneBox-beta1.0-win64.zip`（exe + ffmpeg + 运行说明）。

## 上传 GitHub

仓库目前是本地远端，需要先建 GitHub 仓库：

```bash
git remote add github https://github.com/<你的用户名>/ToneBox.git
git push -u github main
git tag -a v1.0.0-beta.1 -m "ToneBox beta 1.0"
git push github v1.0.0-beta.1
```

然后在 GitHub Releases 里新建 `v1.0.0-beta.1`，上传 `dist\ToneBox-beta1.0-win64.zip`，
发布说明直接引用本文件 + `README.md` 的能力表。

## 已知限制（发布说明里要写）

- ASIO 未支持（需要 ASIO SDK + 驱动）
- 连续播放（gapless）未做：现在是单文件试听
- DSD 只解成 PCM，DoP/原生 DSD 未做
- 蓝牙只能看协议（A2DP/AVRCP/HFP/HSP/LE Audio），不能配对接管，也读不到 SBC/AAC/aptX/LDAC
- 视频只保留第一条画面 + 全部音轨；字幕/多画面未做
- 测试版：所有操作请先备份

## 许可

仓库里还没有 LICENSE。公开前需要决定：MIT / Apache-2.0 / 保留所有权利。
没有许可证时默认是「保留所有权利」，别人不能合法复用；建议加一个再公开。
