#!/usr/bin/env python3
"""
analyze.py —— 频谱质量评估参考实现（后续按此重写为 Rust `analyze_spectrum`）

原则：只输出"推测 + 置信度 + 误判说明"，绝不输出结论。

流程：
  1. ffmpeg 子进程解码成单声道 f32le（保持原始采样率，不重采样）
  2. 4096 点 Hann 窗、50% 重叠，跨帧平均功率谱
  3. 以 1~4kHz 为参考电平，归一化为 dB
  4. 最陡跌落法找编码低通边沿（真实编码器边沿 ≥40dB/kHz，母带/噪底衰减缓得多）
  5. 按编码族查表 → 推测源码率 + 置信度 + 误判说明

关键实测结论（见 docs/样本清单.md）：
  - 同一张阈值表不能通用：ATRAC3 66k 的边沿在 15.2kHz，MP3 64k 在 11.5kHz
  - Opus 在 64k 仍保留全带宽，截止法对它失效，必须换判据
"""
import subprocess, sys, json, math, os
import numpy as np

sys.stdout.reconfigure(encoding="utf-8")

FFMPEG = os.environ.get("FFMPEG", r"C:\Users\23501\AppData\Local\Microsoft\WinGet\Links\ffmpeg.exe")
FFPROBE = FFMPEG.replace("ffmpeg.exe", "ffprobe.exe")

NFFT, HOP = 4096, 2048
REF_BAND = (1000.0, 4000.0)
SLOPE_WIN_HZ = 300.0        # 计算斜率用的半窗宽
SLOPE_MIN = 40.0            # 判定为编码低通边沿的陡度下限（dB/kHz）
KNEE_MIN_HZ = 4000.0

# cutoff(44.1kHz 标定) → 推测源码率档位。必须按编码族分开，否则 ATRAC 会被系统性判低两档。
# 数值来自本次实测标定（tools/calibrate.py），非拍脑袋。
TABLES = {
    "mp3":     [(22500, "无损/CD"), (20500, "320"), (19000, "256"), (17000, "192"),
                (15000, "128"), (12500, "96"), (0, "64")],
    "aac":     [(22500, "无损/CD"), (20000, "256"), (18000, "192"), (15500, "128"),
                (12500, "96"), (0, "64")],
    "atrac3":  [(22000, "无损/CD"), (20000, "256"), (18000, "192"), (17000, "132"),
                (15000, "105"), (13500, "66"), (0, "66")],
    "atrac3p": [(22500, "无损/CD"), (20500, "352"), (19500, "256"), (18000, "192"),
                (15500, "128"), (14000, "96"), (12000, "64"), (0, "48")],
    "atrac1":  [(22000, "无损/CD"), (18000, "292"), (0, "292")],
    "opus":    [(22500, "全带宽"), (0, "全带宽")],
    "vorbis":  [(22500, "无损/CD"), (19500, "320"), (17500, "192"), (15000, "128"),
                (12000, "96"), (0, "64")],
    "generic": [(22000, "无损/CD"), (19500, "320"), (17000, "192"), (15000, "128"),
                (12500, "96"), (10000, "64"), (0, "32~48")],
}

# 这些编码器在低码率仍用全带宽编码（噪声整形/带宽扩展），截止法不可用
FULLBAND_CODECS = {"opus"}


def probe(path):
    p = subprocess.run([FFPROBE, "-v", "quiet", "-print_format", "json",
                        "-show_format", "-show_streams", path], capture_output=True)
    j = json.loads(p.stdout or b"{}")
    st = next((s for s in j.get("streams", []) if s.get("codec_type") == "audio"), None)
    return st, j.get("format", {})


def decode_mono(path, sr):
    cmd = [FFMPEG, "-v", "error", "-i", path, "-map", "0:a:0",
           "-ac", "1", "-ar", str(sr), "-f", "f32le", "-"]
    p = subprocess.run(cmd, capture_output=True)
    if p.returncode != 0:
        raise RuntimeError(p.stderr.decode("utf-8", "ignore")[-300:])
    return np.frombuffer(p.stdout, dtype=np.float32).astype(np.float64)


def mean_spectrum(x):
    n = 1 + max(0, (len(x) - NFFT) // HOP)
    idx = np.arange(n) if n <= 4000 else np.linspace(0, n - 1, 4000).astype(int)
    win = np.hanning(NFFT)
    norm = np.sum(win ** 2)
    acc = np.zeros(NFFT // 2 + 1)
    used = 0
    for i in idx:
        seg = x[i * HOP: i * HOP + NFFT]
        if len(seg) < NFFT:
            break
        s = np.fft.rfft(seg * win)
        acc += (s.real ** 2 + s.imag ** 2) / norm
        used += 1
    acc /= max(1, used)
    with np.errstate(divide="ignore"):
        return 10.0 * np.log10(acc + 1e-30)


def smooth(a, w=9):
    return np.convolve(a, np.ones(w) / w, mode="same")


def find_edge(freqs, db, nyq, ref):
    """最陡跌落法。返回 (cutoff_hz, slope_db_per_khz, drop_db)；无陡边沿返回 (None, slope, 0)"""
    sm = smooth(db, 9)
    df = freqs[1] - freqs[0]
    w = max(1, int(SLOPE_WIN_HZ / df))
    lo = int(KNEE_MIN_HZ / df)
    hi = int((nyq - 400.0) / df)

    best_f, best_slope = None, 0.0
    for i in range(max(w, lo), min(hi, len(sm) - w - 1)):
        slope = (sm[i + w] - sm[i - w]) / (2 * w * df) * 1000.0
        if slope < best_slope:
            best_slope, best_f = slope, freqs[i]

    if best_f is None or best_slope > -SLOPE_MIN:
        return None, best_slope, 0.0

    b = (freqs >= best_f - 1200) & (freqs < best_f)
    a = (freqs > best_f + 200) & (freqs <= best_f + 1400)
    drop = float(np.mean(sm[b]) - np.mean(sm[a])) if b.sum() and a.sum() else -best_slope
    return float(best_f), best_slope, drop


def codec_family(codec):
    c = (codec or "").lower()
    if c.startswith("atrac3plus") or c == "atrac3p":
        return "atrac3p"
    if c.startswith("atrac3"):
        return "atrac3"
    if c.startswith("atrac1"):
        return "atrac1"
    if "mp3" in c:
        return "mp3"
    if c in ("aac", "aac_latm"):
        return "aac"
    if c == "opus":
        return "opus"
    if "vorbis" in c:
        return "vorbis"
    return "generic"


def lookup(family, cutoff, nyq):
    scale = nyq / 22050.0
    table = TABLES.get(family, TABLES["generic"])
    for hz, label in table:
        if cutoff >= hz * scale:
            return label
    return table[-1][1]


def analyze(path):
    st, fmt = probe(path)
    if not st:
        raise RuntimeError("没有可解码的音频流（很可能是 DRM 加密的 .oma）")
    sr = int(st["sample_rate"])
    nyq = sr / 2.0
    codec = st.get("codec_name", "")
    fam = codec_family(codec)

    x = decode_mono(path, sr)
    if len(x) < NFFT:
        raise RuntimeError("样本太少，无法分析")
    db = mean_spectrum(x)
    freqs = np.fft.rfftfreq(NFFT, 1.0 / sr)
    sm = smooth(db, 9)
    ref = np.median(sm[(freqs >= REF_BAND[0]) & (freqs <= REF_BAND[1])])

    cutoff, slope, drop = find_edge(freqs, db, nyq, ref)
    caveats = []
    fullband = fam in FULLBAND_CODECS

    if cutoff is None:
        cutoff = nyq
        caveats.append("未找到陡峭边沿：可能本来是无损，或高频被母带/话筒带宽限制")
        est = "无损/CD" if fam in ("atrac3", "atrac3p", "atrac1") else "无损/CD"
        conf = 0.35
    elif fullband:
        est = "不可判定"
        conf = 0.10
        caveats.append("该编码器在低码率也保留全带宽（噪声整形/带宽扩展），"
                       "频谱截止无法反推码率，需换判据（谱平坦度/噪声底电平）")
    else:
        est = lookup(fam, cutoff, nyq)
        # 边沿陡度 → 置信度：真的是编码器切的边沿通常 >50dB/kHz
        conf = 1.0 / (1.0 + math.exp(-(abs(slope) - 45.0) / 8.0))
        # 边沿上方残留能量高 → 噪底填充，判断可能偏高
        above = freqs > cutoff + 1000
        if above.sum() and np.median(sm[above]) - ref > -55:
            caveats.append("边沿上方仍有明显能量，疑似噪底填充（黑胶/模拟噪声），推测可能偏高")
            conf *= 0.75
        # 边沿下方能量偏低 → 音源本身高频就少
        below = (freqs > cutoff - 2000) & (freqs < cutoff - 400)
        if below.sum() and np.mean(sm[below]) - ref < -25:
            caveats.append("边沿下方能量已偏低，音源本身高频可能就少（电子乐/老录音），推测可能偏低")
            conf *= 0.8

    conf = round(max(0.05, min(0.97, conf)), 2)
    return {
        "file": os.path.basename(path),
        "codec": codec,
        "family": fam,
        "sample_rate": sr,
        "duration_s": round(len(x) / sr, 2),
        "cutoff_hz": round(cutoff),
        "slope_dbm_per_khz": round(slope, 1),
        "drop_db": round(drop, 1),
        "estimate": est,
        "confidence": conf,
        "caveats": caveats,
    }


if __name__ == "__main__":
    args = [a for a in sys.argv[1:] if not a.startswith("--")]
    as_json = "--json" in sys.argv
    out = []
    for f in args:
        try:
            r = analyze(f)
        except Exception as e:
            r = {"file": os.path.basename(f), "error": str(e)}
        out.append(r)
        if not as_json:
            if "error" in r:
                print(f"{r['file']:<32} ERROR {r['error']}")
            else:
                print(f"{r['file']:<32} {r['cutoff_hz']:>6}Hz {r['slope_dbm_per_khz']:>7.0f}dB/kHz "
                      f"{r['codec']:<10}→ {r['estimate']:<9} conf={r['confidence']:.2f} {'; '.join(r['caveats'])}")
    if as_json:
        print(json.dumps(out, ensure_ascii=False, indent=1))
