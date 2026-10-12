"""beside.py LEFT.png RIGHT.png OUT.png [LEFT-LABEL RIGHT-LABEL]: two pictures side by side.

For comparing the Slint settings window with the web one: the same page,
taken the same way, left and right, each under its name. Run with python3 -I.
"""
import sys

from PIL import Image, ImageDraw

left, right, out = sys.argv[1:4]
labels = sys.argv[4:6] if len(sys.argv) >= 6 else ["Slint", "web"]
pictures = [Image.open(left).convert("RGB"), Image.open(right).convert("RGB")]
band = max(24, pictures[0].height // 30)
width = sum(picture.width for picture in pictures)
height = max(picture.height for picture in pictures) + band
sheet = Image.new("RGB", (width, height), (32, 32, 36))
draw = ImageDraw.Draw(sheet)
x = 0
for picture, label in zip(pictures, labels):
    sheet.paste(picture, (x, band))
    draw.text((x + 12, band // 4), label, fill=(240, 240, 240))
    x += picture.width
sheet.save(out)
