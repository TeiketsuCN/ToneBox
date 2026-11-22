# ToneBox — an audio format advisor and converter

> A converter that tells you what a conversion will cost **before** you commit.

ToneBox is a Windows (x64) desktop tool for audio and video format conversion. Its selling point is not
"more formats" but **format advice**: every target explains its patent status, size ratio, cutoff
frequency, and whether it will resample or remix. Read-only formats are listed and explained instead of
being hidden.

**[中文 README](README.md)**

---

## What it does

- **112 output targets**: 65 audio codecs/containers + 36 video containers + 11 device formats;
  plus 47 read-only reference formats.
- **Explains the cost**: bitrate/size, cutoff, sample-rate and channel changes, patent status.
- **Game/legacy audio**: PSF / SPC / VGM / NSF inputs (libgme), PS1 XA, PS2 VAG, N64 VADPCM,
  BRSTM/BCSTM/BFSTM/DSP, QOA, KVAG, MOD/XM/IT/S3M (openmpt123).
- **Batch workflow**: queue, tags, cover art, ReplayGain, output self-check (re-probe every output),
  CUE split, double-click-to-play, "Open with" registration.

## What it does **not** do

- **No DRM**: NCM / QMC / KGM / OMA files are only inspected for plaintext cover art and footer
  metadata. There is **no decryption code**.
- **No fake support**: formats without an encoder are greyed out with a reason
  (APE / TAK / DSD / WMA Lossless are read-only).
- **No network**: no upload, no telemetry, no streaming download.

## Download

| Version | Package | Size | SHA256 |
|---|---|---|---|
| **1.6.1** | [ToneBox-1.6.1-win64.zip](https://github.com/TeiketsuCN/ToneBox/releases/download/v1.6.1/ToneBox-1.6.1-win64.zip) | 28.2 MB | `DFC03D29089F8F20BDA6EE4438A7FD8DC1175BFAB3ADD659C09F7DA2884A2FD2` |
| 1.6.1 extras | [ToneBox-1.6.1-extras-win64.zip](https://github.com/TeiketsuCN/ToneBox/releases/download/v1.6.1/ToneBox-1.6.1-extras-win64.zip) | 4.8 MB | `ADCEB7528927418B668A30BF6387003A6765D316FE925870C6CACA088D3AB1FC` |

Windows 10/11 x64, portable (no installer). The extras package adds optional encoders/decoders
(atracdenc, psxavenc, VGAudioCli, qoaconv, vadpcm, vgmstream, openmpt123); OptimFROG is downloaded by
its own installer because its license restricts redistribution.

## Verification

- **Not code-signed.** Windows SmartScreen may warn; run it in a sandbox/VM if you prefer.
- Verify the SHA256 with `Get-FileHash .\ToneBox-1.6.1-win64.zip -Algorithm SHA256`.
- Build from source: `cargo test -p fc-core --lib` and `cargo build --release -p fc-gui`;
  packaging scripts are in `tools/`.
- The bundled ffmpeg is built by `tools/build-ffmpeg.sh` from FFmpeg sources (LGPL-2.1-or-later,
  no GPL components). CI currently runs tests only; release packages are built locally with the
  same scripts in this repository.

## Status and limits

- **Beta**: core conversion paths are covered by tests; edge combinations may still fail.
  Back up originals and open an issue with `--health` output if something breaks.
- **Windows only** for now: exclusive audio output, DSD DoP and device enumeration are tied to WASAPI.
  macOS/Linux are on the roadmap, no ETA.
- **Video**: first video stream + all audio tracks; multi-angle video is not handled.
- **Chiptunes** are rendered for 3 minutes by default (no length metadata in NSF/SPC).
- **HE-AAC / xHE-AAC / AAC-ELD** can be decoded, but not encoded (encoding needs libfdk-aac,
  which is non-free).

## License

- Application: **MIT**.
- Bundled ffmpeg: **LGPL-2.1-or-later**.
- Third-party components and licenses: [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md).
