# 第三方组件与许可

ToneBox 本身以 MIT 许可发布（见 `LICENSE`）。它**通过独立进程调用 FFmpeg**，并随 Windows 发布包附带
FFmpeg 的共享库与可执行文件。FFmpeg 不是 MIT 代码，发布时必须遵守它自己的许可。

## FFmpeg

- 版本：`N-128560-g967f0ffb10-20261107`
- 构建：Windows x86_64，**LGPL v3（--enable-version3，未开启 --enable-gpl）**，shared DLL
- 许可证：LGPL-3.0-or-later（见 `vendor/ffmpeg/LICENSE-ffmpeg.txt`，发布包内为 `ffmpeg/LICENSE-ffmpeg.txt`）
- 源码与构建配置：https://ffmpeg.org/download.html
  - 本包对应源码可用 FFmpeg 官方快照 `N-128560-g967f0ffb10`；
  - 完整 configure 参数记录在 `docs/FFmpeg-LGPL.md`，可按该参数重新构建同一份 LGPL 二进制。
- 书面要约：如需要，可联系仓库维护者获取对应 FFmpeg 源码副本（有效期三年）。
- 合规要点：
  - 保留 FFmpeg 的版权与许可证文件，不删除、不修改 DLL 的许可声明；
  - 用户可以用自己编译的同版本 LGPL FFmpeg DLL 替换发布包里的 DLL（App 通过独立进程调用，不静态链接）；
  - 不随包提供 GPL 组件（本构建禁用了 libx264/libx265）。

## Rust / GUI 依赖

主要依赖及其常见许可（以各 crate 的 `Cargo.toml` / LICENSE 为准）：

- `eframe` / `egui`：MIT OR Apache-2.0
- `image`：MIT OR Apache-2.0
- `serde` / `serde_json`：MIT OR Apache-2.0
- `anyhow`：MIT OR Apache-2.0
- `rfd`：MIT
- `wasapi` / `windows`：MIT OR Apache-2.0（Windows API 绑定）
- `cpal`：MIT OR Apache-2.0

完整依赖树可用 `cargo tree` 查看；发布二进制时建议同时附上 `cargo metadata` 生成的依赖清单。
