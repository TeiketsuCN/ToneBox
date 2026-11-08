from PIL import Image, ImageDraw

S = 512
im = Image.new('RGBA', (S, S), (0, 0, 0, 0))
d = ImageDraw.Draw(im)
accent = (122, 226, 214, 255)
dark = (14, 26, 34, 255)

# rounded background
d.rounded_rectangle((24, 24, S - 24, S - 24), radius=104, fill=accent)
# speaker box outline
d.rounded_rectangle((118, 150, 318, 362), radius=28, outline=dark, width=22)
# speaker cone: circle + triangle pointing right
d.ellipse((150, 226, 222, 298), fill=dark)
d.polygon([(206, 262), (258, 226), (258, 298)], fill=dark)
# sound waves
for r in (46, 82):
    d.arc((300 - r, 256 - r, 300 + r, 256 + r), start=-52, end=52, fill=dark, width=20)

im.save(r'D:\FormatConverter\crates\fc-gui\assets\icon.png')
im.save(
    r'D:\FormatConverter\crates\fc-gui\assets\icon.ico',
    sizes=[(256, 256), (128, 128), (64, 64), (48, 48), (32, 32), (16, 16)],
)
print('written')
