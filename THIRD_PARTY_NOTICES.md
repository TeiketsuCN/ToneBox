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
- 构建额外启用：`--enable-libgme`（主机芯片音乐解码，见下）
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

## Musepack（mpcenc / libmpcdec-6.dll）

- 项目：Musepack（MPC）
- 主页：<https://www.musepack.net/>
- 许可：LGPL-2.1-or-later
- 用途：`vendor/musepack/mpcenc.exe` 输出 `.mpc`（ffmpeg 只有解码器）
- 二进制来源：MSYS2 `mingw-w64-ucrt-x86_64-libmpcdec` 1~0.1+r475-2
- 许可证全文：`vendor/musepack/LICENSE-musepack.txt`

## libgme（Game Music Emu，主机芯片音乐解码）

- 项目：Game Music Emu
- 主页：<https://github.com/libgme/game-music-emu>
- 许可：LGPL-2.1-or-later
- 用途：随包 ffmpeg 的 libgme 解复用，把 NSF / SPC / GBS / VGM / PSF / SID 等芯片音乐渲染成 PCM
- 二进制来源：MSYS2 `mingw-w64-ucrt-x86_64-libgme` 0.6.5-2（`libgme.dll`；另需 zlib1.dll 运行时）
- 许可证全文：`vendor/ffmpeg/LICENSE-libgme.txt`

## psxavenc（PS1 XA / PS2 VAG 编码器，可选，在 extras 包）

- 项目：psxavenc
- 主页：<https://github.com/WonderfulToolchain/psxavenc>
- 许可：Zlib
- 用途：`psxavenc.exe` 输出 PS1 XA-ADPCM（`.xa`）与 PS2 VAG（`.vag`）
- 分发：Zlib 允许随包，放在 `ToneBox-x.y.z-extras-win64.zip` 的 `psxavenc\` 里
- 版本：v0.3.1（官方 Windows release）
- 许可证全文：`vendor/psxavenc/LICENSE-psxavenc.txt`

## VGAudio / VGAudioCli（任天堂容器，可选，在 extras 包）

- 项目：VGAudio
- 主页：<https://github.com/Thealexbarney/VGAudio>
- 许可：MIT
- 用途：`VGAudioCli.exe` 输出 BRSTM / BCSTM / BFSTM / DSP / IDSP
- 运行要求：.NET Framework 4.5+（Windows 10/11 自带）
- 版本：v2.2.1（官方 release 的 net451 standalone）
- 许可证全文：`vendor/vgaudio/LICENSE-vgaudio.txt`

## QOA / qoaconv（可选，在 extras 包）

- 项目：Quite OK Audio（QOA）
- 作者：Dominic Szablewski（phoboslab）
- 主页：<https://github.com/phoboslab/qoa>
- 许可：MIT
- 用途：`qoaconv.exe` 输出 `.qoa`（4-bit LMS 有损，约 3.5 bit/样本）
- 来源：用 MSYS2 gcc 从官方 `qoa.h` + `qoaconv.c` 编译（源码在 `vendor/qoa/`）
- 许可证全文：`vendor/qoa/LICENSE-qoa.txt`
## openmpt123 / libopenmpt（模块音乐输入，可选，在 extras 包）

- 项目：OpenMPT / libopenmpt
- 主页：<https://lib.openmpt.org/>
- 许可：BSD-3-Clause
- 用途：`openmpt123.exe` 把 MOD / XM / IT / S3M 等模块音乐渲染成 WAV，再交给 ffmpeg 转码
- 二进制来源：MSYS2 `mingw-w64-ucrt-x86_64-libopenmpt` 0.8.10-1（含 openmpt123.exe 与依赖 DLL）
- 许可证全文：`vendor/openmpt123/LICENSE-libopenmpt.txt`

## vgmstream（游戏音频输入，可选，在 extras 包）

- 项目：vgmstream
- 主页：<https://github.com/vgmstream/vgmstream>
- 许可：ISC 为主，含若干第三方许可（见 `vendor/vgmstream/COPYING`）
- 用途：`vgmstream-cli.exe` 把 PSF / PS2 ADS / BRSTM / BCSTM / BFSTM / HCA 等解成 WAV，
  再交给 ffmpeg 转码（ffmpeg 读不了这些容器）
- 版本：r2168（官方 Windows x64 release）
- 许可证全文：`vendor/vgmstream/COPYING`

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
