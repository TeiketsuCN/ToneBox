# 随包 FFmpeg 的构建与 LGPL 合规

发布包里的 FFmpeg 是 **LGPL v3** 构建（`--enable-version3`，未启用 `--enable-gpl`），以 shared DLL
形式放在 `ffmpeg\` 目录。ToneBox 通过 `ffmpeg.exe` / `ffprobe.exe` 子进程调用它，不静态链接。

## 版本

```
ffmpeg version N-128560-g967f0ffb10-20261107
```

## configure 参数（来自 `ffmpeg -version`）

```
--prefix=/ffbuild/prefix --pkg-config-flags=--static --pkg-config=pkg-config --cross-prefix=x86_64-w64-mingw32- --arch=x86_64 --target-os=mingw32 --enable-version3 --disable-debug --enable-shared --disable-static --disable-w32threads --enable-pthreads --enable-iconv --enable-zlib --enable-libxml2 --enable-libvmaf --enable-fontconfig --enable-libharfbuzz --enable-libfreetype --enable-libfribidi --enable-lcms2 --enable-vulkan --enable-libdav1d --enable-libvorbis --enable-librav1e --enable-librsvg --disable-libxcb --disable-xlib --disable-libpulse --enable-gmp --enable-lzma --enable-liblcevc-dec --enable-opencl --enable-amf --enable-libaom --disable-avisynth --enable-chromaprint --disable-libdavs2 --disable-libdvdread --disable-libdvdnav --disable-libfdk-aac --enable-ffnvcodec --enable-cuda-llvm --disable-frei0r --enable-libgme --enable-libkvazaar --enable-libaribb24 --enable-libaribcaption --enable-libass --enable-libbluray --enable-libcurl --enable-libjxl --enable-libmp3lame --enable-libopus --enable-libplacebo --enable-librist --enable-libssh --enable-libtheora --enable-libvpx --enable-libwebp --enable-libzmq --enable-lv2 --enable-libvpl --enable-openal --enable-liboapv --enable-libopencore-amrnb --enable-libopencore-amrwb --enable-libopenh264 --enable-libopenjpeg --enable-libopenmpt --disable-librubberband --enable-schannel --enable-sdl2 --enable-libsnappy --enable-libsoxr --enable-libsrt --enable-libsvtav1 --enable-libtwolame --enable-libuavs3d --disable-libdrm --enable-vaapi --enable-vapoursynth --disable-libvidstab --enable-libvvenc --disable-whisper --disable-libx264 --disable-libx265 --disable-libxavs2 --disable-libxvid --enable-libzimg --enable-libzvbi --extra-cflags=-DLIBTWOLAME_STATIC --extra-cxxflags= --extra-libs=-lgomp --extra-ldflags=-pthread --extra-ldexeflags= --cc=x86_64-w64-mingw32-gcc --cxx=x86_64-w64-mingw32-g++ --ar=x86_64-w64-mingw32-gcc-ar --ranlib=x86_64-w64-mingw32-gcc-ranlib --nm=x86_64-w64-mingw32-gcc-nm --extra-version=20261107
```

## 发布包里必须带

- `ffmpeg/LICENSE-ffmpeg.txt`（LGPL 文本）
- `THIRD_PARTY_NOTICES.md`（本仓库根目录，打包脚本会复制到 zip 根目录）
- 说明：用户可以用自己编译的同版本 LGPL FFmpeg DLL 替换 `ffmpeg\` 下的 DLL

## 不随包提供

- libx264 / libx265 等 GPL 组件（本次构建明确 disable）
- 任何修改过的 FFmpeg 源码

## 源码获取

- FFmpeg 官方：https://ffmpeg.org/download.html
- 本构建对应快照：`N-128560-g967f0ffb10`
- 若需要本包所用源码副本，可通过仓库 issue 联系维护者；维护者提供与二进制对应的源码（有效期三年）。
