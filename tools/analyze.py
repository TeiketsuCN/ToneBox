#!/usr/bin/env python3
"""
analyze.py —— 频谱质量评估参考实现（与 Rust `fc-analyze` 对齐）

原则：只输出"推测 + 置信度 + 误判说明"，绝不输出结论。

流程：
  1. ffmpeg 子进程解码成单声道 f32le（保持原始采样率，不重采样）
  2. 4096 点 Hann 窗、50% 重叠，跨帧平均功率谱
  3. 以 1~4kHz 为参考电平，归一化为 dB
  4. 最陡跌落法找编码低通边沿（真实编码器边沿 ≥40dB/kHz，母带/噪底衰减缓得多）
  5. 按编码族查档位表；母带带宽受限时给区间，不硬猜

关键实测结论（见 docs/样本清单.md）：
  - 同一张阈值表不能通用：ATRAC3 66k 的边沿在 15.2kHz，MP3 64k 在 11.5kHz
  - 阈值用绝对 Hz，不按 Nyquist 缩放；44.1/48kHz 下低通位置近似固定
  - Opus / HE-AAC 在低码率仍保留全带宽，截止法失效，必须输出"不可判定"
"""
import json
import math
import os
import subprocess
import sys

import numpy as np

sys.stdout.reconfigure(encoding="utf-8")

FFMPEG = os.environ.get(
    "FFMPEG", r"C:\Users\23501\AppData\Local\Microsoft\WinGet\Links\ffmpeg.exe"
)
FFPROBE = FFMPEG.replace("ffmpeg.exe", "ffprobe.exe")

NFFT, HOP = 4096, 2048
REF_BAND = (1000.0, 4000.0)
SLOPE_WIN_HZ = 300.0        # 计算斜率用的半窗宽
SLOPE_MIN = 40.0            # 判定为编码低通边沿的陡度下限（dB/kHz）
KNEE_MIN_HZ = 4000.0

LOSSLESS_CODECS = {
    "flac", "alac", "wavpack", "ape", "tta", "truehd", "mlp", "shorten",
}


def probe(path):
    p = subprocess.run(
        [FFPROBE, "-v", "quiet", "-print_format", "json",
         "-show_format", "-show_streams", path],
        capture_output=True,
    )
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


def find_edge(freqs, db, nyq):
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


def is_lossless_codec(codec):
    c = (codec or "").lower()
    return c.startswith("pcm_") or c in LOSSLESS_CODECS


def profile_is_he_aac(profile):
    p = (profile or "").upper()
    return (
        "HE-AAC" in p
        or "HE_AAC" in p
        or "XHE-AAC" in p
        or "XHE_AAC" in p
    )


def classify(family, cutoff):
    """返回 (estimate, confidence_scale, caveat)。阈值单位 Hz，按 44.1/48kHz 实测标定。"""
    if family == "mp3":
        if cutoff >= 20000:
            return "320", 1.0, None
        if cutoff >= 19500:
            return "256", 0.70, "256k MP3 样本不足，该档位边界是插值估计。"
        if cutoff >= 17500:
            return "192", 1.0, None
        if cutoff >= 16000:
            # 源带宽受限时，128/192/320 的截止频率在 16~17.5kHz 撞在一起。
            # 不再用边沿陡度硬猜单档：留出集验证显示这个规则只被 pop48k 一张母版支撑。
            return ("128/192/320", 0.35,
                    "截止频率落在 128/192/320 的重叠区；母带带宽受限时无法单值区分，只能给区间。")
        if cutoff >= 15800:
            return "128", 1.0, None
        if cutoff >= 12500:
            return "96", 1.0, None
        return "64", 1.0, None

    if family == "aac":
        if cutoff >= 20500:
            return "256", 1.0, None
        if cutoff >= 18500:
            return "192", 1.0, None
        if cutoff >= 14000:
            return ("128~256", 0.50,
                    "128k 与 256k 在母带带宽受限时截止频率重叠，只能给出区间。")
        return "64", 1.0, None

    if family == "atrac3":
        if cutoff >= 18000:
            return "132", 1.0, None
        if cutoff >= 16000:
            return ("66~132", 0.50,
                    "ATRAC3 66k 与 132k 的截止频率重叠（同族样本差异大于档位差异），只能给区间。")
        if cutoff >= 14500:
            return ("66~105", 0.50,
                    "ATRAC3 66k 与 105k 的截止频率重叠（同族样本差异大于档位差异），只能给区间。")
        return "66", 1.0, None

    if family == "atrac3p":
        if cutoff >= 19300:
            return "256", 1.0, None
        if cutoff >= 18500:
            return "48", 1.0, None
        if cutoff >= 16000:
            return "128", 1.0, None
        if cutoff >= 14500:
            return ("65~128", 0.50,
                    "ATRAC3+ 65k 与 128k 的截止频率几乎重合，只能给区间。")
        return "49", 1.0, None

    if family == "atrac1":
        return "292", 1.0, None

    if family == "opus":
        return "不可判定", 0.10, "截止法对 Opus 无效。"

    if family == "vorbis":
        if cutoff >= 19500:
            return "320", 1.0, None
        if cutoff >= 17500:
            return "192", 1.0, None
        if cutoff >= 15000:
            return "128", 1.0, None
        if cutoff >= 12000:
            return "96", 1.0, None
        return "64", 1.0, None

    # generic
    if cutoff >= 19500:
        return "320", 1.0, None
    if cutoff >= 17000:
        return "192", 1.0, None
    if cutoff >= 15000:
        return "128", 1.0, None
    if cutoff >= 12500:
        return "96", 1.0, None
    if cutoff >= 10000:
        return "64", 1.0, None
    return "32~48", 1.0, None


def no_edge_guess(family):
    """返回 (estimate, confidence, caveat)。没有陡峭边沿时按编码族给保守推测。"""
    if family == "aac":
        return ("256", 0.55,
                "未找到陡峭边沿。AAC 在高码率会把带宽保留到 Nyquist，按 256k 档处理；无法排除无损或母带带宽受限。")
    if family == "mp3":
        return ("256~320", 0.45,
                "未找到陡峭边沿。MP3 高码率或母带带宽受限时都可能是这样，只能给区间。")
    if family == "atrac3p":
        return ("256~352", 0.40,
                "未找到陡峭边沿。ATRAC3+ 高码率与母带带宽受限无法仅凭截止频率区分。")
    if family == "atrac3":
        return ("132~292", 0.35,
                "未找到陡峭边沿。ATRAC3 高码率与母带带宽受限无法仅凭截止频率区分。")
    if family == "atrac1":
        return "292", 0.40, "未找到陡峭边沿。ATRAC1 只有 292k 一档，按该档处理。"
    if family == "vorbis":
        return "320", 0.45, "未找到陡峭边沿。Vorbis 高码率或母带带宽受限时都可能是这样。"
    if family == "opus":
        return "不可判定", 0.10, "截止法对 Opus 无效。"
    return ("无损/CD", 0.35,
            "未找到陡峭边沿：可能本来是无损，或高频被母带/话筒带宽限制。")


def analyze(path):
    st, _fmt = probe(path)
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

    cutoff, slope, drop = find_edge(freqs, db, nyq)
    edge_missing = cutoff is None
    if cutoff is None:
        cutoff = nyq
    caveats = []
    he_aac = fam == "aac" and profile_is_he_aac(st.get("profile", ""))

    if is_lossless_codec(codec):
        caveats.append("编码本身是无损格式；频谱只能说明文件没有被再次有损压缩，不能证明母带质量。")
        est, conf = "无损/CD", 0.92
    elif fam == "opus":
        caveats.append(
            "Opus 在低码率也保留全带宽（噪声整形/带宽扩展），频谱截止无法反推码率，"
            "需换判据（谱平坦度/噪声底电平）。"
        )
        est, conf = "不可判定", 0.10
    elif he_aac:
        caveats.append(
            "HE-AAC / xHE-AAC 使用 SBR/PS 频带复制，低码率仍保留高频；"
            "截止法对它和 Opus 一样失效。"
        )
        est, conf = "不可判定", 0.12
    elif edge_missing:
        est, conf, why = no_edge_guess(fam)
        caveats.append(why)
    else:
        est, scale, note = classify(fam, cutoff)
        # 边沿陡度 → 置信度：真的是编码器切的边沿通常 >50dB/kHz
        conf = 1.0 / (1.0 + math.exp(-(abs(slope) - 45.0) / 8.0)) * scale
        if note:
            caveats.append(note)
        # 边沿上方残留能量高 → 噪底填充，判断可能偏高
        above = freqs > cutoff + 1000
        if above.sum() and np.median(sm[above]) - ref > -55:
            caveats.append("边沿上方仍有明显能量，疑似噪底填充（黑胶/模拟噪声），推测可能偏高。")
            conf *= 0.75
        # 边沿下方能量偏低 → 音源本身高频就少
        below = (freqs > cutoff - 2000) & (freqs < cutoff - 400)
        if below.sum() and np.mean(sm[below]) - ref < -25:
            caveats.append(
                "边沿下方能量已偏低，音源本身高频可能就少（电子乐/老录音），推测可能偏低。"
            )
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
                print(
                    f"{r['file']:<32} {r['cutoff_hz']:>6}Hz "
                    f"{r['slope_dbm_per_khz']:>7.0f}dB/kHz "
                    f"{r['codec']:<10}→ {r['estimate']:<9} "
                    f"conf={r['confidence']:.2f} {'; '.join(r['caveats'])}"
                )
    if as_json:
        print(json.dumps(out, ensure_ascii=False, indent=1))
