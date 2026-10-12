"""subset.py IN OUT: a font cut down to the characters of GB 2312.

That is the 6763 Chinese characters of the 1980 national standard (what
"common Chinese" most often means in such a cut), with the punctuation and
full-width forms Chinese is written with, and what the font has of Latin.
Run with python3 -I. Used to find out what a subset saves, and what it
loses: experiments/slint-settings/results/fonts.md.
"""
import sys

from fontTools import subset
from fontTools.ttLib import TTFont

source, target = sys.argv[1:3]
wanted = set(range(0x20, 0x250)) | set(range(0x2000, 0x2070)) | set(range(0x3000, 0x3040)) | set(range(0xFF00, 0xFFF0))
for lead in range(0xB0, 0xF8):
    for trail in range(0xA1, 0xFF):
        try:
            wanted.add(ord(bytes([lead, trail]).decode("gb2312")))
        except UnicodeDecodeError:
            pass
options = subset.Options()
options.layout_features = ["*"]
options.name_IDs = ["*"]
options.notdef_outline = True
options.glyph_names = False
font = TTFont(source)
cutter = subset.Subsetter(options)
cutter.populate(unicodes=wanted)
cutter.subset(font)
font.save(target)
kept = TTFont(target, lazy=True).getBestCmap()
print(f"{target}: {len(kept)} characters, {sum(1 for c in kept if 0x4E00 <= c <= 0x9FFF)} of them Chinese")
