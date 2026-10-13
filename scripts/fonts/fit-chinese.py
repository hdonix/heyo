#!/usr/bin/env python3
"""fit-chinese.py IN.otf OUT.otf: Noto Sans SC made to sit beside Inter.

Slint sets Latin in Inter and Chinese in Noto Sans SC (src-tauri/fonts,
src-tauri/src/slint_windows/fonts.rs), and as Google ships it the second
does not go with the first: its line is 1.45 of the size where Inter's is
1.21, so a row with Chinese in it is taller and its text sits lower, and
its characters are 0.85 to 0.9 of the size where Inter's capitals are
0.73, so they look a size larger than the Latin beside them.

This writes the font with Inter's line (ascent and descent, in the three
places a font says them) and its characters at 94%: the outlines are
untouched and the em they are measured in is made larger, 1000 to 1064,
which shrinks advances with them. The SIL Open Font License allows a
changed font; Noto Sans SC reserves no name.

Needs fontTools. Run on the font as downloaded, not on its own output.
"""
import sys

from fontTools.ttLib import TTFont

# Inter's, as fractions of its em (hhea and OS/2 typo in Inter-Variable.ttf).
ASCENT, DESCENT = 0.96875, 0.2412109375
EM = 1064

source, out = sys.argv[1:]
font = TTFont(source)
if font["head"].unitsPerEm != 1000:
    sys.exit(f"fit-chinese: {source} has an em of {font['head'].unitsPerEm}, not the 1000 of the font as shipped")
font["head"].unitsPerEm = EM
ascent, descent = round(ASCENT * EM), round(DESCENT * EM)
hhea, os2 = font["hhea"], font["OS/2"]
hhea.ascent, hhea.descent, hhea.lineGap = ascent, -descent, 0
os2.sTypoAscender, os2.sTypoDescender, os2.sTypoLineGap = ascent, -descent, 0
os2.usWinAscent, os2.usWinDescent = ascent, descent
font.save(out)
