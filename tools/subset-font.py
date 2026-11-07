#!/usr/bin/env python3
"""
subset-font.py —— 从源码里扫出用到的字，生成子集字体

为什么必须子集化：
  完整 Noto Sans SC 单字重 8.1 MB，直接内嵌会让 exe 从 3 MB 涨到 11 MB。
  界面上实际只用几百个字，子集后 200~300 KB。

用法：
  python tools/subset-font.py

以后加了新文案忘了重新跑？不会漏字——运行时会退回系统字体兜底，
但字形会不一致。所以改完 UI 文案记得重跑一次。
"""
import io
import os
import subprocess
import sys
from pathlib import Path

ROOT = Path(r"D:\FormatConverter")
SRC_DIRS = [ROOT / "crates"]
CACHE = ROOT / "tools" / "fontcache"
ASSETS = ROOT / "crates" / "fc-gui" / "assets"
SRC_FONT = CACHE / "NotoSansSC-Regular.otf"
OUT_FONT = ASSETS / "NotoSansSC-Subset.otf"

# 基础字符集：ASCII 可见字符 + 常用中文标点
BASE = (
    "".join(chr(c) for c in range(0x20, 0x7F))
    + "·—…「」『』（）《》、。，！？：；""''【】～￥"
    + "←→↑↓▶◀●○■□✓✗▲▼"
)


def collect_chars() -> set:
    """扫所有 .rs 源码里的非 ASCII 字符（UI 文案都在里面）"""
    chars = set(BASE)
    for d in SRC_DIRS:
        for p in d.rglob("*.rs"):
            try:
                text = io.open(p, encoding="utf-8").read()
            except Exception:
                continue
            for ch in text:
                if ord(ch) > 0x7F:
                    chars.add(ch)
    return chars


def main():
    if not SRC_FONT.exists():
        print(f"缺少源字体: {SRC_FONT}")
        sys.exit(1)

    ASSETS.mkdir(parents=True, exist_ok=True)
    chars = collect_chars()
    text = "".join(sorted(chars))
    tmp = CACHE / "chars.txt"
    io.open(tmp, "w", encoding="utf-8").write(text)
    print(f"收集到 {len(chars)} 个字符")

    cmd = [
        sys.executable, "-m", "fontTools.subset",
        str(SRC_FONT),
        f"--text-file={tmp}",
        f"--output-file={OUT_FONT}",
        "--drop-tables+=DSIG",
        "--no-hinting",
        "--layout-features=*",
    ]
    subprocess.run(cmd, check=True)

    src_kb = SRC_FONT.stat().st_size / 1024
    out_kb = OUT_FONT.stat().st_size / 1024
    print(f"{src_kb:.0f} KB → {out_kb:.0f} KB  ({out_kb / src_kb:.1%})")

    # 顺手把许可拷过去
    lic = CACHE / "OFL.txt"
    if lic.exists():
        (ASSETS / "OFL.txt").write_bytes(lic.read_bytes())
        print("已复制 OFL.txt")


if __name__ == "__main__":
    main()
