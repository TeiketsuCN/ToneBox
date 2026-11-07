#!/usr/bin/env python3
"""
calibrate.py —— 用对照样本标定"截止频率 ↔ 码率"映射，并给出每族阈值表建议

输出：
  - out/calibration.csv   每个样本的实测截止 + 真值码率
  - 控制台：按 (编码族, 码率) 聚合的截止均值/最小值/最大值
"""
import os, sys, json, glob
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
sys.stdout.reconfigure(encoding="utf-8")
import csv
from analyze import analyze

ROOT = r"D:\FormatConverter\samples"
OUT = r"D:\FormatConverter\out"


def collect():
    rows = []
    # 对照集：文件名里带真值码率
    for pat, sub in (("mp3/*.mp3", "mp3"), ("aac/*.m4a", "aac"), ("opus/*.opus", "opus")):
        for p in glob.glob(os.path.join(ROOT, pat)):
            name = os.path.basename(p)
            stem = os.path.splitext(name)[0]
            try:
                kbps = int(stem.split("_")[-1].rstrip("k"))
            except ValueError:
                continue
            src = name.split("_")[0]
            rows.append((p, sub, kbps, src))
    # 无损基准
    for p in glob.glob(os.path.join(ROOT, "lossless", "*.flac")):
        rows.append((p, "lossless", 0, os.path.basename(p).split("_")[0]))
    # 真 ATRAC：码率以 ffprobe 实测为准，真值另外从已知清单来
    atrac_truth = {
        "mc_sich_atrac3_66k.wav": 66, "sample_atrac3_66k.wav": 66,
        "mc_sich_atrac3_105k.wav": 105, "mc_sich_atrac3_132k.wav": 132,
        "psp_bgm01.at3": 65, "psp_bgm03.at3": 65,
        "SND0_atrac3plus_48k.at3": 49, "oma_at3p_01.oma": 256,
        "oma_at3p_02.oma": 256, "oma_at3p_03.oma": 256, "oma_at3p_04.oma": 256,
        "oma_at3p_05.oma": 256, "oma_at3p_06.oma": 256,
        "aa3_0111019293.aa3": 128, "aa3_sonateno14op27-2.aa3": 48,
        "aea_christmas1970.aea": 292,
    }
    for p in glob.glob(os.path.join(ROOT, "atrac", "*")):
        if os.path.splitext(p)[1] not in (".wav", ".at3", ".oma", ".aa3", ".aea"):
            continue
        b = os.path.basename(p)
        if b in atrac_truth:
            rows.append((p, "atrac", atrac_truth[b], "atrac"))
    return rows


def main():
    os.makedirs(OUT, exist_ok=True)
    rows = collect()
    results = []
    for p, sub, kbps, src in rows:
        try:
            r = analyze(p)
        except Exception as e:
            print(f"[skip] {os.path.basename(p)}: {e}")
            continue
        r.update(truth_kbps=kbps, subset=sub, source=src)
        results.append(r)

    with open(os.path.join(OUT, "calibration.csv"), "w", newline="", encoding="utf-8-sig") as f:
        w = csv.DictWriter(f, fieldnames=["subset", "source", "truth_kbps", "family", "codec",
                                          "sample_rate", "cutoff_hz", "slope_dbm_per_khz",
                                          "drop_db", "estimate", "confidence", "file"])
        w.writeheader()
        for r in results:
            w.writerow({k: r.get(k) for k in w.fieldnames})

    # 聚合
    groups = {}
    for r in results:
        groups.setdefault((r["family"], r["truth_kbps"]), []).append(r["cutoff_hz"])

    print(f"{'编码族':<10}{'真值kbps':>9}{'样本数':>7}{'截止均值':>10}{'最小':>8}{'最大':>8}")
    for (fam, kb), cuts in sorted(groups.items(), key=lambda x: (x[0][0], -x[0][1])):
        print(f"{fam:<10}{kb:>9}{len(cuts):>7}{sum(cuts) / len(cuts):>10.0f}{min(cuts):>8.0f}{max(cuts):>8.0f}")

    # 命中率：estimate 字符串里包含真值码率
    ok = sum(1 for r in results
             if str(r["truth_kbps"]) in str(r["estimate"]) or
             (r["truth_kbps"] == 0 and "无损" in r["estimate"]))
    print(f"\n档位命中率（含无损）：{ok}/{len(results)} = {ok / max(1, len(results)):.0%}")


if __name__ == "__main__":
    main()
