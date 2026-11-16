# 第三方组件与署名

ToneBox 自身代码以 MIT 许可发布。随包/依赖的第三方组件如下，感谢原作者。

## FFmpeg

- 项目：FFmpeg
- 主页：<https://ffmpeg.org/>
- 许可：LGPL-2.1-or-later（`--enable-version3 --disable-gpl --disable-nonfree`，
  不含 GPL 组件）
- 用途：解码/编码/探测
- 版本：8.0，用 MSYS2 UCRT64 的 mingw-w64 gcc 16.2.0 构建
- 构建脚本：`tools/build-ffmpeg.sh`（`tools/build-ffmpeg.ps1` 是 Windows 入口）
- 许可证全文：`vendor/ffmpeg/LICENSE-ffmpeg.txt`

随包 ffmpeg 链接的编解码库（均为 LGPL 兼容许可，DLL 位于 `vendor/ffmpeg/`）：

| 库 | 许可 | 用途 |
|---|---|---|
| libmp3lame | LGPL-2.1 | MP3 编码 |
| libopus | BSD-3-Clause | Opus 编解码 |
| libvorbis / libogg | BSD-3-Clause | Vorbis 编解码 |
| libvpx | BSD-3-Clause | VP8/VP9 编解码 |
| libtheora | BSD-3-Clause | Theora 编解码 |
| libaom | BSD-2-Clause | AV1 编解码 |
| libdav1d | BSD-2-Clause | AV1 解码 |
| libopenh264 | BSD-2-Clause | H.264 编码 |
| libkvazaar | BSD-3-Clause | H.265 编码 |
| opencore-amr | Apache-2.0 | AMR-NB 编解码 |
| **libgsm** | ISC | GSM 06.10 编码 |
| **vo-amrwbenc** | Apache-2.0 | AMR-WB 编码 |
| **libilbc** | BSD-3-Clause | iLBC 编解码 |
| **liblc3** | Apache-2.0 | LC3 编解码 |
| **codec2** | LGPL-2.1 | Codec2 编解码 |

## Speex（speexenc / libspeex）

- 项目：Speex
- 作者：Jean-Marc Valin / Xiph.Org
- 主页：<https://www.speex.org/>
- 源码：<https://gitlab.xiph.org/xiph/speex>
- 许可：BSD 3-Clause
- 用途：`vendor/speex/speexenc.exe` 输出 `.spx`
- 二进制来源：MSYS2 的 `mingw-w64-ucrt-x86_64-speex` 1.2.1-1 包
  <https://packages.msys2.org/packages/mingw-w64-ucrt-x86_64-speex>
- 许可证全文：`vendor/speex/LICENSE-speex.txt`

## SpeexDSP（libspeexdsp-1.dll）

- 项目：SpeexDSP
- 作者：Jean-Marc Valin / Xiph.Org
- 源码：<https://gitlab.xiph.org/xiph/speexdsp>
- 许可：BSD 3-Clause
- 二进制来源：MSYS2 `mingw-w64-ucrt-x86_64-speexdsp` 1.2.1-1
- 许可证全文：`vendor/speex/LICENSE-speexdsp.txt`

## libogg（libogg-0.dll）

- 项目：libogg
- 作者：Xiph.Org
- 主页：<https://www.xiph.org/ogg/>
- 许可：BSD 3-Clause
- 二进制来源：MSYS2 `mingw-w64-ucrt-x86_64-libogg` 1.3.5-1
- 许可证全文：`vendor/speex/LICENSE-libogg.txt`

## WavPack（wavpack.exe）

- 项目：WavPack
- 作者：David Bryant
- 主页：<https://www.wavpack.com/>
- 许可：BSD 3-Clause
- 用途：`vendor/wavpack/wavpack.exe` 输出**有损** WavPack（hybrid 模式，
  ffmpeg 自带的 wavpack 编码器只支持无损）
- 二进制来源：MSYS2 `mingw-w64-ucrt-x86_64-wavpack` 5.9.0-1
- 许可证全文：`vendor/wavpack/LICENSE-wavpack.txt`

## OptimFROG（ofr.exe，可选，不随包）

- 项目：OptimFROG / Lossless Audio
- 作者：Florin Ghido
- 主页：<http://losslessaudio.org/>
- 许可：闭源免费软件（个人与商业使用免费）
- 用途：OptimFROG 无损输出（ffmpeg 连完整解码器都没有）
- **分发限制**：官方 `license.html` 与 `ofr.exe --help` 均写明
  "distribution of command-line Encoders with other software may need permission"，
  所以 **ToneBox 不随包分发 ofr.exe**，只检测用户自己安装的版本。
- 版本：5.100（Win/x64, 2016-09-02），官方 zip 的 SHA-1：
  `abf21227c5417c984ee51ba391e2bcb5546d033b`

## atracdenc（可选，不随包）

- 项目：atracdenc
- 主页：<https://github.com/ika-musume/atracdenc>
- 许可：LGPL-2.1-or-later
- 用途：ATRAC1 / ATRAC3 / ATRAC3+ 编码
- 说明：**不随本程序分发**，用户自行安装后由程序在设置页检测调用。
  如果你自己放进 `atracdenc/` 目录，请连同它的许可一起保留。

## 说明

- ToneBox 调用这些程序时使用子进程，不修改其源码。
- 如果你重新分发本项目的安装包，请一并保留本文件和 `vendor/*/LICENSE-*`。
- 如果只使用系统已安装的 ffmpeg/speexenc/wavpack，则对应组件的许可由系统安装包负责。
