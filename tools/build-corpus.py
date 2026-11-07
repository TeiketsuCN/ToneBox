#!/usr/bin/env python3
"""
build-corpus.py —— 用本机无损母版生成"已知真值"的对照样本集

为什么要对照：ATRAC 是黑盒（我们只有解码器、没有编码器），
但 MP3/AAC/Opus 的码率是我们自己设定的，可用来标定"频谱截止 ↔ 码率"映射曲线，
再拿真 ATRAC 文件去校验这条曲线站不站得住。

实现要点（两个坑都已踩过）：
  1. -ss 定位 / atrim 滤镜后的音频喂给 flac 编码器会触发 ffmpeg 9.0.1 的
     "invalid block size: 8" 错误。故改为两段式：先截成 WAV，再从 WAV 编码。
  2. 两段式还有个好处：所有编码器看到的是同一份 PCM，对照更干净。
"""
import subprocess, os, json

FFMPEG = os.environ.get("FFMPEG", r"C:\Users\23501\AppData\Local\Microsoft\WinGet\Links\ffmpeg.exe")
ROOT   = r"D:\FormatConverter\samples"
WORK   = r"D:\FormatConverter\out\work"
SEC, START = 30, 30          # 取第 30~60 秒

SOURCES = [
    # (母版路径, 代号, 备注)
    (r"D:\FLAC\夜に駆ける - ALKALOID,Double Face.flac",                     "pop48k",    "现代流行/48kHz，应满带宽"),
    (r"D:\UserData\Music\The Beatles - Hey Jude.flac",                      "beatles69", "1969 录音，母带带宽本身有限 → 误判场景"),
    (r"D:\迅雷下载\ツィゴイネルワイゼン Op20 - 葉加瀬太郎.flac",                "violin",    "小提琴独奏，高频以泛音为主"),
    (r"D:\FLAC\Pay No Mind (Lemaitre Remix) - Madeon,Passion Pit.flac",     "electro",   "电子/合成器，高频天然稀少 → 误判场景"),
]

LOSSY = [
    (["-c:a", "libmp3lame", "-b:a", "64k"],  64,  "mp3",  "mp3"),
    (["-c:a", "libmp3lame", "-b:a", "96k"],  96,  "mp3",  "mp3"),
    (["-c:a", "libmp3lame", "-b:a", "128k"], 128, "mp3",  "mp3"),
    (["-c:a", "libmp3lame", "-b:a", "192k"], 192, "mp3",  "mp3"),
    (["-c:a", "libmp3lame", "-b:a", "320k"], 320, "mp3",  "mp3"),
    (["-c:a", "aac", "-b:a", "64k"],         64,  "aac",  "m4a"),
    (["-c:a", "aac", "-b:a", "128k"],        128, "aac",  "m4a"),
    (["-c:a", "aac", "-b:a", "256k"],        256, "aac",  "m4a"),
    (["-c:a", "libopus", "-b:a", "64k"],     64,  "opus", "opus"),
    (["-c:a", "libopus", "-b:a", "96k"],     96,  "opus", "opus"),
]


def run(cmd, tolerate=False):
    p = subprocess.run(cmd, capture_output=True)
    if p.returncode != 0 and not tolerate:
        raise RuntimeError(p.stderr.decode("utf-8", "ignore")[-400:])


def main():
    os.makedirs(WORK, exist_ok=True)
    for d in ("lossless", "mp3", "aac", "opus"):
        os.makedirs(os.path.join(ROOT, d), exist_ok=True)

    manifest = []
    for src, code, note in SOURCES:
        if not os.path.exists(src):
            print(f"[skip] 母版不存在: {src}")
            continue

        # 第一段：无损截出 30 秒 PCM
        cut = os.path.join(WORK, f"{code}.wav")
        run([FFMPEG, "-y", "-v", "error", "-i", src, "-t", str(START + SEC),
             "-af", f"atrim=start={START}:end={START + SEC},asetpts=N/SR/TB",
             "-map", "0:a:0", "-c:a", "pcm_s24le", cut])

        # 基准：FLAC
        ref = os.path.join(ROOT, "lossless", f"{code}_ref.flac")
        run([FFMPEG, "-y", "-v", "error", "-i", cut, "-c:a", "flac", ref])
        manifest.append(dict(file=f"lossless/{code}_ref.flac", truth_kbps=0,
                             kind="lossless", note=note, source=os.path.basename(src)))
        print(f"[ref ] {code}")

        # 码率阶梯
        for params, kbps, sub, ext in LOSSY:
            enc = params[1].replace("libmp3lame", "mp3").replace("lib", "")
            out = os.path.join(ROOT, sub, f"{code}_{enc}_{kbps}k.{ext}")
            run([FFMPEG, "-y", "-v", "error", "-i", cut, "-ac", "2", *params, out])
            manifest.append(dict(file=f"{sub}/{code}_{enc}_{kbps}k.{ext}", truth_kbps=kbps,
                                 kind=enc, note=note, source=os.path.basename(src)))
        print(f"[enc ] {code} × {len(LOSSY)}")

    with open(os.path.join(ROOT, "manifest_synthetic.json"), "w", encoding="utf-8") as f:
        json.dump(manifest, f, ensure_ascii=False, indent=2)
    print(f"\n生成 {len(manifest)} 个对照样本")


if __name__ == "__main__":
    main()
