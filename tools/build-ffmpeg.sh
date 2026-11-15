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
LC3_VERSION="${LC3_VERSION:-1.0.6}"

echo "==> 项目根目录: $ROOT"
echo "==> 线程数: $JOBS"

# ── 1. 依赖 ─────────────────────────────────────────────
# 用 MSYS2 的预编译包，省掉自己编十几个库
PKGS=(
  make nasm pkgconf git
  mingw-w64-ucrt-x86_64-gcc
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
  mingw-w64-ucrt-x86_64-svt-av1
  mingw-w64-ucrt-x86_64-zlib
)
echo "==> 安装依赖包…"
pacman -S --needed --noconfirm "${PKGS[@]}"

mkdir -p "$BUILD" "$PREFIX" "$OUT"

# ── 2. liblc3（MSYS2 没有包，自己编）────────────────────
if [ ! -f "$PREFIX/lib/liblc3.a" ] && [ ! -f "$PREFIX/lib/liblc3.dll.a" ]; then
  echo "==> 构建 liblc3 $LC3_VERSION"
  cd "$BUILD"
  if [ ! -d liblc3 ]; then
    git clone --depth 1 --branch "v$LC3_VERSION" https://github.com/google/liblc3.git
  fi
  cd liblc3
  # 静态库 + 头文件，装到私有前缀
  cmake -B build -DCMAKE_BUILD_TYPE=Release \
        -DCMAKE_INSTALL_PREFIX="$PREFIX" \
        -DBUILD_SHARED_LIBS=OFF \
        -DLC3_BUILD_TOOLS=OFF -DLC3_BUILD_EXAMPLES=OFF
  cmake --build build -j "$JOBS"
  cmake --install build
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
  # 视频编解码（ToneBox 的视频容器要用）
  --enable-libvpx
  --enable-libtheora
  --enable-libopenh264
  --enable-libkvazaar
  --enable-libsvtav1
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

# 动态链接时，依赖库的 DLL 也得跟着走
for dll in libgsm.dll libvo-amrwbenc.dll libilbc.dll liblc3.dll libcodec2.dll \
           libmp3lame.dll libopus-0.dll libvorbis-0.dll libvorbisenc-2.dll \
           libogg-0.dll libvpx.dll libtheora-0.dll openh264-2.dll libkvazaar.dll \
           libdav1d.dll SvtAv1Enc.dll libwinpthread-1.dll libgcc_s_seh-1.dll; do
  if [ -f "$PREFIX/bin/$dll" ]; then cp -f "$PREFIX/bin/$dll" "$OUT/"; fi
done

# 许可证
cp -f "$BUILD/ffmpeg/COPYING.LGPLv3" "$OUT/LICENSE-ffmpeg.txt" 2>/dev/null || true

echo "==> 完成。自检："
"$OUT/ffmpeg.exe" -hide_banner -encoders 2>&1 | grep -Ei 'libgsm|libvo_amrwbenc|libilbc|liblc3|libcodec2' || echo "（没找到新编码器，检查链接是否成功）"
