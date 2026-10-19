"""plan.py: what happens in the film, beat by beat.

Everything that is seen to happen on a beat is listed here once: the
sections, the cuts, the sweeps between scenes, the windows going to their
places, each word of each caption, each letter typed in the opening and on
the last card. The edit puts the pictures there (edit.py, and the
composition through data.js); the music is written from the same list
(music.py), a sound for each of them; and sync.py measures the rendered
film against it.

Times are in beats from the film's first picture; at 120 beats a minute a
beat is 0.5 s, 12 pictures (the film is 24 a second, so that every
sixteenth of a beat is a picture, 3 of them).
"""

from types import SimpleNamespace

BPM = 120
BEAT = 60 / BPM

# What the film shows, and the music under it.
SECTIONS = [
    ("intro", 0, 16),      # cold open (the palette on beat 1), title from beat 8
    ("drop", 16, 48),      # dictation
    ("lift", 48, 60),      # window layouts: one a beat from 52
    ("groove", 60, 76),    # screenshot
    ("breakdown", 76, 96), # clipboard, calculator
    ("final", 96, 112),    # extensions
    ("end", 112, 128),     # end card
]
LENGTH = SECTIONS[-1][2]
TAIL = 1.0  # seconds after the last beat: the last chord's reverb, the fade

# Where a shot begins (edit.py's storyboard is held to this).
CUTS = [0, 8, 16, 20, 28, 48, 52, 60, 64, 70, 73, 76, 78, 82, 88, 92, 96, 100, 104, 112]
# Cuts under a sweep of colour that carries the next scene's name.
WIPES = {48: "Window Layouts", 60: "Screenshots", 76: "Clipboard", 88: "Calculator", 96: "Extensions"}
# The windows going to their places, one a beat.
SNAPS = list(range(52, 59))

# The opening: the desktop, the palette, "edit" typed a letter a
# sixteenth, Enter, the editor; the camera; the title card.
OPEN = {
    "palette": 1,
    "letters": [2, 2.25, 2.5, 2.75],
    "enter": 3.5,
    "editor": 4,
    "move": 6,
    "title": 8,
    "icon": 8.5,
    "name": [9, 9.25, 9.5, 9.75],
    "sub": [11, 11.5, 12, 12.5, 13],
    "pills": [13.5, 13.75, 14, 14.25, 14.5, 14.75],
    "throw": 15.5,
}
TITLE_SUB = ["The", "keyboard", "launcher", "for", "Linux."]
TITLE_PILLS = ["Dictation", "Layouts", "Screenshots", "Clipboard", "Calculator", "Extensions"]

# The last card: the name slammed in, the line, the address typed a letter
# a sixteenth, the caret blinking on the hats, the last chord, the fade.
ADDRESS = "github.com/hdonix/heyo"
END = {
    "slam": 112,
    "line": 114,
    "url": 116,
    "letters": [116.5 + k / 4 for k in range(len(ADDRESS))],
    "blinks": [b / 2 for b in range(2 * 122 + 1, 2 * 124)],
    "chord": 124,
    "fade": (126, LENGTH + TAIL / BEAT),
}

# The words over the picture: (word, the beat it punches in on); those in
# `accent` are marked in the icon's blue, on a heavier hit.
CAPTIONS = [
    {"from": 16, "to": 28, "kicker": "", "place": "bottom", "words": [("Say", 17), ("it.", 17.5), ("It’s", 18.5), ("typed.", 19)], "accent": ["typed."]},
    {"from": 28, "to": 47.5, "kicker": "", "place": "bottom", "words": [("Mix", 28.5), ("languages", 29), ("mid-sentence.", 30)], "accent": ["languages"]},
    {"from": 48, "to": 59.5, "kicker": "Window Layouts", "place": "bottom", "words": [("Every", 49), ("window,", 49.5), ("right", 50), ("where", 50.5), ("you", 51), ("want", 51.5), ("it.", 52)], "accent": ["right"]},
    {"from": 60, "to": 70, "kicker": "Screenshots", "place": "bottom", "words": [("Capture", 61), ("it.", 61.5), ("Mark", 62), ("it", 62.5), ("up.", 63)], "accent": ["Mark"]},
    {"from": 70, "to": 75.5, "kicker": "Screenshots", "place": "bottom", "words": [("Pin", 70.5), ("it", 71), ("over", 71.5), ("your", 72), ("work.", 72.5)], "accent": ["Pin"]},
    {"from": 76, "to": 87.5, "kicker": "Clipboard", "place": "bottom", "words": [("Everything", 77), ("you", 77.25), ("copied,", 77.5), ("found", 78.5), ("again.", 79)], "accent": ["found"]},
    {"from": 88, "to": 95.5, "kicker": "Calculator", "place": "top", "words": [("Answers", 89), ("as", 89.25), ("you", 89.5), ("type.", 90)], "accent": ["type."]},
    {"from": 96, "to": 111.5, "kicker": "Extensions", "place": "bottom", "words": [("Raycast", 97), ("extensions,", 97.5), ("one", 98), ("Enter", 98.5), ("away.", 99)], "accent": ["Enter"]},
]


# --- The section titles held -------------------------------------------------
#
# Everything above is written as if each sweep came and went on one beat.
# Each section's title is held on screen instead: the film stops for HOLD
# beats where a sweep is (its own music, the next scene after it). What is
# written above is kept as WRITTEN (the music is composed in those beats and
# moved by shift()); the names above are then rebound to the film's beats.

HOLD = 4
WRITTEN = SimpleNamespace(SECTIONS=SECTIONS, LENGTH=LENGTH, CUTS=CUTS, WIPES=WIPES, SNAPS=SNAPS, OPEN=OPEN, END=END, CAPTIONS=CAPTIONS)


def shift(beat):
    """A written beat in the film's beats: later by HOLD for each section
    title held before it (a scene that begins on a sweep begins after it)."""
    return beat + HOLD * sum(1 for wipe in WRITTEN.WIPES if beat >= wipe)


def _shifted(value):
    if isinstance(value, (list, tuple)):
        return type(value)(_shifted(v) for v in value)
    return shift(value)


# The titles: (the beat the title comes on, the beat the scene does).
TITLES = [(shift(wipe) - HOLD, shift(wipe), word) for wipe, word in sorted(WRITTEN.WIPES.items())]
SECTIONS = sorted([(name, shift(a), shift(b) - (HOLD if b in WRITTEN.WIPES else 0)) for name, a, b in WRITTEN.SECTIONS]
                  + [("title", a, b) for a, b, _ in TITLES], key=lambda section: section[1])
LENGTH = shift(WRITTEN.LENGTH)
CUTS = sorted([shift(beat) for beat in WRITTEN.CUTS] + [a for a, _, _ in TITLES])
WIPES = {a: word for a, _, word in TITLES}
SNAPS = _shifted(WRITTEN.SNAPS)
OPEN = {key: _shifted(value) for key, value in WRITTEN.OPEN.items()}
END = {key: _shifted(value) for key, value in WRITTEN.END.items()}
CAPTIONS = [dict(caption, **{"from": shift(caption["from"]), "to": shift(caption["to"]), "words": [(w, shift(b)) for w, b in caption["words"]]})
            for caption in WRITTEN.CAPTIONS]


def punches():
    """The picture punched on the kicks while a scene is on: (beat, how
    much). Lightly, and only on the bar, while words are being dictated."""
    out = []
    for beat in range(16, 112):
        if beat in WRITTEN.WIPES or beat in WRITTEN.SNAPS:
            continue
        if beat < 48:
            if beat % 4 == 0:
                out.append((beat, 0.014))
        else:
            out.append((beat, 0.024 if beat % 4 == 0 else 0.01))
    out += [(beat, 0.035) for beat in WRITTEN.SNAPS]
    return sorted((shift(beat), amount) for beat, amount in out)


def seen():
    """Everything that is seen to happen on a beat: (beat, what, where),
    where is the part of the film's picture (x, y, w, h at 1920x1080) to
    look at for it, or None for all of it."""
    events = []
    events.append((OPEN["palette"], "palette appears", None))
    for k, beat in enumerate(OPEN["letters"]):
        events.append((beat, f"letter {'edit'[k]!r} typed", None))
    events += [(OPEN["enter"], "Enter: palette closes", None), (OPEN["editor"], "editor opens", None), (OPEN["move"], "camera moves in", None),
               (OPEN["title"], "cut to the title", None), (OPEN["icon"], "icon", (560, 300, 260, 260))]
    # where each is on the title card (measured on a rendered picture)
    letters = [(830, 130), (960, 120), (1080, 120), (1200, 140)]
    for (x, w), letter, beat in zip(letters, "Heyo", OPEN["name"]):
        events.append((beat, f"title letter {letter!r}", (x, 320, w, 250)))
    words = [(552, 88), (660, 230), (905, 220), (1135, 65), (1218, 150)]
    for (x, w), word, beat in zip(words, TITLE_SUB, OPEN["sub"]):
        events.append((beat, f"subtitle {word!r}", (x, 565, w, 85)))
    pills = [(336, 178), (532, 164), (714, 224), (958, 188), (1164, 196), (1378, 204)]
    for (x, w), word, beat in zip(pills, TITLE_PILLS, OPEN["pills"]):
        events.append((beat, f"pill {word!r}", (x, 685, w, 80)))
    events.append((OPEN["throw"], "title thrown at the camera", None))
    for beat in CUTS[2:]:
        events.append((beat, "title: " + WIPES[beat] if beat in WIPES else "cut", None))
    for beat in SNAPS[1:]:
        events.append((beat, "window to its place", None))
    for caption in CAPTIONS:
        # the line of the headline
        y = (958, 104) if caption["place"] == "bottom" else (92, 110)
        for word, beat in caption["words"]:
            if beat not in CUTS:
                events.append((beat, f"caption word {word!r}", (0, y[0], 1920, y[1])))
    events += [(END["slam"], "end: Heyo slammed in", None), (END["line"], "end: Free and open source", (640, 555, 640, 80)),
               (END["url"], "end: address pill", (600, 668, 720, 108))]
    for letter, beat in zip(ADDRESS, END["letters"]):
        events.append((beat, f"address letter {letter!r}", (600, 668, 720, 108)))
    for beat in END["blinks"]:
        events.append((beat, "caret " + ("off" if beat % 1 else "on"), (1200, 680, 70, 90)))
    events.append((END["chord"], "end: last chord", None))
    return sorted(set(events), key=lambda e: e[0])
