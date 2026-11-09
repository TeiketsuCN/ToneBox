from PIL import Image

im = Image.open(r'D:\FormatConverter\out\svg\github_direct.png').convert('RGBA')
bbox = im.getbbox()
im = im.crop(bbox)
side = max(im.size)
canvas = Image.new('RGBA', (side, side), (0, 0, 0, 0))
canvas.paste(im, ((side - im.width) // 2, (side - im.height) // 2), im)
canvas = canvas.resize((128, 128), Image.LANCZOS)
canvas.save(r'D:\FormatConverter\crates\fc-gui\assets\github.png')
print(canvas.size, bbox)
