# 第三方组件与署名

ToneBox 自身代码以 MIT 许可发布。随包/依赖的第三方组件如下，感谢原作者。

## FFmpeg

- 项目：FFmpeg
- 主页：<https://ffmpeg.org/>
- 许可：LGPL-2.1-or-later（随包为 LGPL 共享构建）
- 用途：解码/编码/探测
- 说明：`vendor/ffmpeg/` 下为随包二进制，许可证见 `vendor/ffmpeg/LICENSE-ffmpeg.txt`。

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

## 说明

- ToneBox 调用这些程序时使用子进程，不修改其源码。
- 如果你重新分发本项目的安装包，请一并保留本文件和 `vendor/*/LICENSE-*`。
- 如果只使用系统已安装的 ffmpeg/speexenc，则对应组件的许可由系统安装包负责。
