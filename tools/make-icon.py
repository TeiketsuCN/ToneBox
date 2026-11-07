"""生成应用图标（多尺寸 .ico）。

设计：深色圆角方块 + 青色播放三角 + 底部一条进度条。
不用文字——16px 下文字必糊，形状更耐缩。
"""
import io
import os

from PIL import Image, ImageDraw

OUT = r"D:\FormatConverter\crates\fc-gui\assets\icon.ico"
S = 1024  # 先画大图，再缩

BG = (20, 22, 29, 255)
ACCENT = (122, 226, 212, 255)
EDGE = (255, 255, 255, 28)


def rounded(size, radius):
    im = Image.new("RGBA", (size, size), (0, 0, 0, 0))
    d = ImageDraw.Draw(im)
    d.rounded_rectangle([0, 0, size - 1, size - 1], radius=radius, fill=BG)
    d.rounded_rectangle(
        [2, 2, size - 3, size - 3], radius=radius, outline=EDGE, width=max(1, size // 128)
    )
    return im


im = rounded(S, int(S * 0.22))
d = ImageDraw.Draw(im)

# 播放三角
cx, cy = S * 0.5, S * 0.45
r = S * 0.20
tri = [
    (cx - r * 0.72, cy - r),
    (cx - r * 0.72, cy + r),
    (cx + r * 1.05, cy),
]
d.polygon(tri, fill=ACCENT)

# 底部进度条
bar_w = S * 0.46
bar_h = S * 0.075
bx = (S - bar_w) / 2
by = S * 0.72
d.rounded_rectangle(
    [bx, by, bx + bar_w, by + bar_h], radius=bar_h / 2, fill=(255, 255, 255, 40)
)
d.rounded_rectangle(
    [bx, by, bx + bar_w * 0.62, by + bar_h], radius=bar_h / 2, fill=ACCENT
)

os.makedirs(os.path.dirname(OUT), exist_ok=True)
im.save(OUT, format="ICO", sizes=[(16, 16), (24, 24), (32, 32), (48, 48), (64, 64), (128, 128), (256, 256)])
# 顺手存一张 PNG 给窗口图标和文档用
im.resize((256, 256), Image.LANCZOS).save(
    r"D:\FormatConverter\crates\fc-gui\assets\icon.png"
)
print("icon written:", OUT, os.path.getsize(OUT), "bytes")
