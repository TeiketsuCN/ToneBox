#!/usr/bin/env python3
"""spectrum-dump.py —— 诊断：按 1kHz 步长打印平均频谱（相对 1~4kHz 中频电平），看清断崖形状"""
import sys, os
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
sys.stdout.reconfigure(encoding="utf-8")
import numpy as np
from analyze import decode_mono, mean_spectrum, smooth, probe

for path in sys.argv[1:]:
    st, _ = probe(path)
    sr = int(st["sample_rate"])
    x = decode_mono(path, sr)
    db = smooth(mean_spectrum(x), 9)
    f = np.fft.rfftfreq(4096, 1.0 / sr)
    ref = np.median(db[(f >= 1000) & (f <= 4000)])
    half = sr // 2
    pts = [(int(k * 1000), db[np.argmin(np.abs(f - k * 1000))] - ref)
           for k in range(6, half // 1000 + 1)]
    body = " ".join(f"{k // 1000}:{v:.0f}" for k, v in pts)
    print(f"{os.path.basename(path)[:34]:<34} sr={sr} nyq={half} | {body}")
