#!/usr/bin/env bash
# 构建带语音编码库的 LGPL ffmpeg（供 ToneBox 随包分发）
#
# 为什么需要：随包那份是 BtbN 的 lgpl-shared 构建，缺 libgsm / libvo_amrwbenc /
# libilbc / liblc3 / libcodec2 这五个库，所以 GSM、AMR-WB、iLBC、LC3、Codec2
# 五个格式在发布版里是灰的。这个脚本把它们编进去。
#
# 许可：全部为 LGPL 兼容（libgsm ISC、vo-amrwbenc Apache-2.0、libilbc BSD、
# liblc3 Apache-2.0、codec2 LGPL-2.1），所以构建保持 LGPL，不引入 GPL。
#
# 用法（在 MSYS2 UCRT64 环境里）：
#   tools/build-ffmpeg.sh              # 默认 8 线程，源码放 build/ffmpeg
#   JOBS=4 tools/build-ffmpeg.sh
#
# 产物：vendor/ffmpeg/（ffmpeg.exe / ffprobe.exe / av*.dll），
# 以及可能需要的依赖 DLL（如果静态链接失败）。

set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
BUILD="$ROOT/build/ffmpeg"
PREFIX="$BUILD/prefix"
OUT="$ROOT/vendor/ffmpeg"
JOBS="${JOBS:-$(nproc)}"
FFMPEG_VERSION="${FFMPEG_VERSION:-8.0}"
LC3_VERSION="${LC3_VERSION:-1.1.3}"

echo "==> 项目根目录: $ROOT"
echo "==> 线程数: $JOBS"

# ── 1. 依赖 ─────────────────────────────────────────────
# 用 MSYS2 的预编译包，省掉自己编十几个库
PKGS=(
  make nasm pkgconf git
  mingw-w64-ucrt-x86_64-gcc
  mingw-w64-ucrt-x86_64-cmake
  mingw-w64-ucrt-x86_64-meson
  mingw-w64-ucrt-x86_64-gsm
  mingw-w64-ucrt-x86_64-vo-amrwbenc
  mingw-w64-ucrt-x86_64-libilbc
  mingw-w64-ucrt-x86_64-codec2
  mingw-w64-ucrt-x86_64-opencore-amr
  mingw-w64-ucrt-x86_64-opus
  mingw-w64-ucrt-x86_64-libvorbis
  mingw-w64-ucrt-x86_64-libvpx
  mingw-w64-ucrt-x86_64-libtheora
  mingw-w64-ucrt-x86_64-lame
  mingw-w64-ucrt-x86_64-openh264
  mingw-w64-ucrt-x86_64-kvazaar
  mingw-w64-ucrt-x86_64-dav1d
  mingw-w64-ucrt-x86_64-aom
  mingw-w64-ucrt-x86_64-zlib
  # 主机芯片音乐（NSF/SPC/GBS/VGM/PSF/SID…）走 ffmpeg 的 libgme 解复用器
  mingw-w64-ucrt-x86_64-libgme
)
echo "==> 安装依赖包…"
pacman -S --needed --noconfirm "${PKGS[@]}"

mkdir -p "$BUILD" "$PREFIX" "$OUT"
export PKG_CONFIG_PATH="$PREFIX/lib/pkgconfig:${PKG_CONFIG_PATH:-}"

# ── 2. liblc3（MSYS2 没有包，自己编）────────────────────
if [ ! -f "$PREFIX/lib/liblc3.a" ] && [ ! -f "$PREFIX/lib/liblc3.dll.a" ]; then
  echo "==> 构建 liblc3 $LC3_VERSION"
  cd "$BUILD"
  # 上一次失败的克隆可能留下空目录，先清掉
  if [ -d liblc3 ] && [ ! -f liblc3/meson.build ]; then rm -rf liblc3; fi
  if [ ! -d liblc3 ]; then
    git clone --depth 1 --branch "v$LC3_VERSION" https://github.com/google/liblc3.git \
      || git clone --depth 1 https://github.com/google/liblc3.git
  fi
  cd liblc3
  # liblc3 用 meson（不是 CMake），静态库装到私有前缀
  # 选项名在不同版本间可能变，失败就退回默认选项再试一次
  if ! meson setup build --prefix="$PREFIX" --buildtype=release -Dtools=false; then
    rm -rf build
    meson setup build --prefix="$PREFIX" --buildtype=release
  fi
  ninja -C build
  ninja -C build install
else
  echo "==> liblc3 已存在，跳过"
fi

# ── 3. ffmpeg 源码 ──────────────────────────────────────
cd "$BUILD"
if [ ! -d ffmpeg ]; then
  echo "==> 下载 ffmpeg $FFMPEG_VERSION"
  if [ ! -f "ffmpeg-$FFMPEG_VERSION.tar.xz" ]; then
    curl -L --fail -o "ffmpeg-$FFMPEG_VERSION.tar.xz" \
      "https://ffmpeg.org/releases/ffmpeg-$FFMPEG_VERSION.tar.xz"
  fi
  tar xf "ffmpeg-$FFMPEG_VERSION.tar.xz"
  mv "ffmpeg-$FFMPEG_VERSION" ffmpeg
fi

# ── 4. configure ────────────────────────────────────────
# 开关取自当前随包构建（BtbN lgpl-shared）里我们真正用得上的部分，
# 去掉一堆用不到的视频滤镜/硬件加速依赖，再加上五个语音库。
CONFIGURE=(
  --prefix="$PREFIX"
  --arch=x86_64
  --target-os=mingw32
  --enable-shared --disable-static
  --enable-version3
  --disable-debug
  --enable-pthreads --disable-w32threads
  --enable-zlib --enable-iconv --enable-schannel
  # 音频编码
  --enable-libmp3lame
  --enable-libopus
  --enable-libvorbis
  --enable-libopencore-amrnb --enable-libopencore-amrwb
  --enable-libgsm
  --enable-libvo-amrwbenc
  --enable-libilbc
  --enable-liblc3
  --enable-libcodec2
  # 主机芯片音乐：libgme 提供 NSF/SPC/GBS/VGM/PSF/SID 等的解复用（渲染成 PCM）
  --enable-libgme
  # 视频编解码（ToneBox 的视频容器要用）
  --enable-libvpx
  --enable-libtheora
  --enable-libopenh264
  --enable-libkvazaar
  # SVT-AV1 不启用：MSYS2 的 4.x 接口和 ffmpeg 8.0 对不上，AV1 编码交给 libaom
  --enable-libaom
  --enable-libdav1d
  # 明确关掉 GPL / 非自由组件，保持 LGPL
  --disable-gpl --disable-nonfree
  --disable-libx264 --disable-libx265 --disable-libfdk-aac
)

cd "$BUILD/ffmpeg"
echo "==> configure"
./configure "${CONFIGURE[@]}" \
  --extra-cflags="-I$PREFIX/include" \
  --extra-ldflags="-L$PREFIX/lib"

echo "==> make -j$JOBS"
make -j"$JOBS"
make install

# ── 5. 收集产物 ─────────────────────────────────────────
echo "==> 收集到 vendor/ffmpeg"
cp -f "$PREFIX/bin/ffmpeg.exe" "$PREFIX/bin/ffprobe.exe" "$OUT/"
cp -f "$PREFIX"/bin/av*.dll "$PREFIX"/bin/sw*.dll "$OUT/" 2>/dev/null || true

# 依赖库的 DLL 用 ldd 解析（MSYS2 的库在 /ucrt64/bin，不在私有前缀里），
# 但**必须过滤掉系统 DLL**（kernel32 之类不能随包分发）
echo "==> 收集依赖 DLL"
ldd "$PREFIX/bin/ffmpeg.exe" "$PREFIX/bin/ffprobe.exe" 2>/dev/null \
  | awk '/=> \//{print $3}' | grep -i '\.dll$' | sort -u | while read -r d; do
      base=$(basename "$d")
      case "$base" in
        # MSYS2 运行时库，从 /ucrt64/bin 取；其余是 Windows 自带的，跳过
        lib*.dll) cp -f "$d" "$OUT/" ;;
      esac
    done

# 旧版本 DLL 留着会让 exe 找不到正确的库，顺手清掉
for f in avcodec-63.dll avformat-63.dll avutil-61.dll avfilter-12.dll \
         avdevice-63.dll swresample-7.dll swscale-10.dll; do
  rm -f "$OUT/$f"
done

# libgme 还依赖 zlib1.dll（MSYS2 的 zlib 运行时），它不是 lib* 前缀，
# ldd 的过滤列表会漏掉它 —— 少了它 ffmpeg 直接 0xC0000135 起不来
if [ -f /ucrt64/bin/zlib1.dll ]; then
  cp -f /ucrt64/bin/zlib1.dll "$OUT/"
fi

# 许可证
cp -f "$BUILD/ffmpeg/COPYING.LGPLv3" "$OUT/LICENSE-ffmpeg.txt" 2>/dev/null || true

echo "==> 完成。自检："
"$OUT/ffmpeg.exe" -hide_banner -encoders 2>&1 | grep -Ei 'libgsm|libvo_amrwbenc|libilbc|liblc3|libcodec2' || echo "（没找到新编码器，检查链接是否成功）"
"$OUT/ffmpeg.exe" -hide_banner -demuxers 2>&1 | grep -i 'libgme' || echo "（没找到 libgme 解复用器）"
