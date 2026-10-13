"""demo-film.py FILM: cuts what demo-film.sh captured into the film.

FILM is the directory the capture left: capture.mkv (the whole 3840x2160
screen), epoch and moments.jsonl (when each thing happened, by the clock the
film's pictures carry, and where on the screen it is), and what was spoken.
Out of it come FILM/heyo.mp4 (H.264 and AAC) and FILM/heyo.webm (VP9 and
Opus), 1920x1080 at 30 pictures a second, and FILM/cuts.txt, which says
how far each cut is from a beat.

How it is cut is written below as a list of shots (`storyboard`). A shot is
so many beats long, so every cut is on a beat of the music. It says which
stretch of the capture it shows and how fast (the waiting is left out and
typing is hurried; what is dictated runs as it was, with its sound), and
where the camera is: a 16:9 part of the screen, moved with an ease from
one place to the next. A thing that must happen on a beat (a window going
to its place, a palette appearing) is found in the capture by looking for
the picture in which the screen changes. Titles and captions are drawn
here, over the picture.

The music is fetched once and kept ($HEYO_FILM_CACHE/music), and is what
its SHA-256 says or it is not used:

    "Newer Wave" by Kevin MacLeod (incompetech.com)
    https://incompetech.com/music/royalty-free/mp3-royaltyfree/Newer%20Wave.mp3
    Licensed under Creative Commons: By Attribution 4.0
    https://creativecommons.org/licenses/by/4.0/
    110 beats a minute, as its composer lists it

The licence asks for that credit, which is on the film's last card. Another
track: HEYO_FILM_MUSIC_URL, HEYO_FILM_MUSIC_SHA256, HEYO_FILM_MUSIC_BPM (as
listed; the beats themselves are found in the sound), HEYO_FILM_MUSIC_CREDIT
(the line on the last card; empty for music that asks for none) and
HEYO_FILM_MUSIC_START (seconds into the track at which the film begins; it
begins on the first beat at or after it that has sound).

    HEYO_FILM_DRAFT=1    a quick film at half the size, MP4 only
    HEYO_FILM_STILLS=1   also FILM/stills/: the first, middle and last
                         picture of every shot, to look the film over

Needs numpy, Pillow, OpenCV, librosa, fontTools with brotli and
imageio-ffmpeg (demo-film.sh makes a virtual environment with them).
"""
import hashlib
import importlib.util
import json
import math
import os
import re
import subprocess
import sys
import urllib.request
import wave
from pathlib import Path

import cv2
import imageio_ffmpeg
import librosa
import numpy as np
from fontTools.ttLib import TTFont
from PIL import Image, ImageDraw, ImageFilter, ImageFont
from scipy.signal import fftconvolve

HERE = Path(__file__).resolve().parent
REPO = HERE.parent.parent
FFMPEG = imageio_ffmpeg.get_ffmpeg_exe()
FILM = Path(sys.argv[1]) if len(sys.argv) > 1 else sys.exit(__doc__)
CACHE = Path(os.environ.get("HEYO_FILM_CACHE") or Path.home() / ".cache/heyo-build/film")
DRAFT = bool(os.environ.get("HEYO_FILM_DRAFT"))
STILLS = bool(os.environ.get("HEYO_FILM_STILLS"))

MUSIC = {
    "url": os.environ.get("HEYO_FILM_MUSIC_URL", "https://incompetech.com/music/royalty-free/mp3-royaltyfree/Newer%20Wave.mp3"),
    "sha256": os.environ.get("HEYO_FILM_MUSIC_SHA256", "ef26589e066cc2f60f9523f8a3c17e543f57fc9e1145efc3bc46f7e5db92d0bc"),
    "bpm": float(os.environ.get("HEYO_FILM_MUSIC_BPM", "110")),
    "credit": os.environ.get("HEYO_FILM_MUSIC_CREDIT", "Music: “Newer Wave” by Kevin MacLeod (incompetech.com) · CC BY 4.0"),
    "start": float(os.environ.get("HEYO_FILM_MUSIC_START", "0")),
}

# The capture, and the film made of it.
SCREEN = (3840, 2160)
RATE = 30
SIZE = (960, 540) if DRAFT else (1920, 1080)
SOUND = 48000


def run(*arguments, **how):
    return subprocess.run([FFMPEG, "-hide_banner", "-loglevel", "error", *map(str, arguments)], capture_output=True, **how)


# --- The capture ---------------------------------------------------------------


class Capture:
    """The film of the screen: a picture by its number, and the moments."""

    def __init__(self, directory):
        self.path = directory / "capture.mkv"
        described = subprocess.run([FFMPEG, "-hide_banner", "-i", str(self.path)], capture_output=True, text=True).stderr
        # The first picture's time, counted as the moments are (demo-film.sh).
        self.first = float(re.search(r"start: (-?[\d.]+)", described).group(1))
        epoch = float((directory / "epoch").read_text())
        self.moments = {}
        for line in (directory / "moments.jsonl").read_text().splitlines():
            moment = json.loads(line)
            self.moments[moment["name"]] = (moment["at"] - epoch, moment["box"])
        self.decoder = None
        self.number = None
        self.picture = None
        self.late = self.clock()

    def number_at(self, seconds):
        return max(0, round((seconds - self.first) * RATE))

    def open(self, number, *filters, length=None):
        # Half a picture early: the first one read is the one asked for.
        arguments = ["-ss", f"{max(0, (number - 0.5) / RATE):.4f}", "-i", self.path]
        if length:
            arguments += ["-frames:v", str(length)]
        if filters:
            arguments += ["-vf", ",".join(filters)]
        return subprocess.Popen([FFMPEG, "-hide_banner", "-loglevel", "error", *map(str, arguments), "-f", "rawvideo", "-pix_fmt", "bgr24", "-"], stdout=subprocess.PIPE)

    def read(self, number):
        """Picture `number` of the capture, whole."""
        if number == self.number:
            return self.picture
        size = SCREEN[0] * SCREEN[1] * 3
        if self.decoder is None or not self.number < number <= self.number + 90:
            self.close()
            self.decoder = self.open(number)
            self.number = number - 1
        while self.number < number:
            data = self.decoder.stdout.read(size)
            if len(data) < size:
                # Past the end of the capture: the last picture there is.
                self.close()
                return self.picture
            self.number += 1
        self.picture = np.frombuffer(data, np.uint8).reshape(SCREEN[1], SCREEN[0], 3)
        return self.picture

    def close(self):
        if self.decoder:
            self.decoder.kill()
            self.decoder.wait()
        self.decoder = None

    def small(self, start, seconds, box):
        """The pictures from `start` on, that part of them, small and grey."""
        x, y, w, h = box
        across = min(w, 240) // 2 * 2
        down = max(2, round(h * across / w) // 2 * 2)
        number = self.number_at(start)
        raw = self.open(number, f"crop={w}:{h}:{x}:{y}", f"scale={across}:{down}", "format=gray", length=round(seconds * RATE)).communicate()[0]
        return number, np.frombuffer(raw, np.uint8)[: len(raw) // (across * down * 3) * across * down * 3].reshape(-1, down, across, 3)[..., 0].astype(np.int16)

    def changes(self, start, box, within=4.0, by=2.0):
        """When that part of the screen first differs from what it is at
        `start`: the time of the picture it does so in. None if it does not."""
        number, pictures = self.small(start, within, box)
        differs = np.abs(pictures - pictures[0]).mean(axis=(1, 2)) > by
        return (number + int(np.argmax(differs))) / RATE + self.first if differs.any() else None

    def clock(self):
        """How far the moments' clock is ahead of the pictures', at the
        capture's start and end: the pointer jumped as two moments were
        written, and the picture it jumps in is found."""
        found = []
        for name in ("sync", "sync-end"):
            if name + "-after" not in self.moments:
                continue
            marked = (self.moments[name + "-before"][0] + self.moments[name + "-after"][0]) / 2
            seen = self.changes(marked - 0.8, (3660, 1660, 96, 96), within=2.0)
            if seen is not None:
                found.append((marked, marked - seen))
        if not found:
            return lambda at: 0.0
        (first, a), (last, b) = found[0], found[-1]
        print(f"demo-film: the moments' clock is {a * 1000:+.0f} ms from the pictures' at the start, {b * 1000:+.0f} ms at the end")
        return lambda at: a if last == first else a + (b - a) * (at - first) / (last - first)

    def at(self, name, plus=0.0):
        """A moment, in the pictures' time."""
        marked = self.moments[name][0]
        return marked - self.late(marked) + plus

    def box(self, name):
        return tuple(self.moments[name][1])


# --- The music ---------------------------------------------------------------


def music_file():
    kept = CACHE / "music" / (MUSIC["sha256"] + ".mp3")
    if not kept.exists():
        kept.parent.mkdir(parents=True, exist_ok=True)
        print(f"demo-film: fetching the music from {MUSIC['url']}")
        request = urllib.request.Request(MUSIC["url"], headers={"User-Agent": "heyo-demo-film"})
        with urllib.request.urlopen(request, timeout=600) as answer:
            data = answer.read()
        fetched = kept.with_suffix(".part")
        fetched.write_bytes(data)
        fetched.rename(kept)
    if hashlib.sha256(kept.read_bytes()).hexdigest() != MUSIC["sha256"]:
        sys.exit(f"demo-film: {kept} is not the music its name says (SHA-256); remove it, or say which it is: HEYO_FILM_MUSIC_SHA256")
    return kept


def sound_of(path, rate, channels):
    raw = run("-i", path, "-ac", channels, "-ar", rate, "-f", "f32le", "-").stdout
    return np.frombuffer(raw, np.float32).reshape(-1, channels)


class Beats:
    """The beats of the track: a grid of one tempo, fitted to where its
    notes begin. `at(n)` is beat n, in seconds of the track."""

    def __init__(self, path):
        self.rate = 22050
        self.sound = sound_of(path, self.rate, 1)[:, 0]
        hop = 64
        self.strength = librosa.onset.onset_strength(y=self.sound, sr=self.rate, hop_length=hop)
        self.times = np.arange(len(self.strength)) * hop / self.rate
        listed = 60 / MUSIC["bpm"]
        # The tempo and where the beats fall: the grid on which the notes'
        # beginnings are strongest. Twice, the second time finely.
        period, phase = listed, 0.0
        for periods, step in ((np.linspace(listed * 0.97, listed * 1.03, 241), 0.004), (None, 0.001)):
            if periods is None:
                periods = np.linspace(period * 0.9985, period * 1.0015, 241)
            best = -1.0
            for candidate in periods:
                phases = np.arange(0, candidate, step)
                grid = phases[:, None] + np.arange(int(self.times[-1] / candidate) - 1)[None, :] * candidate
                score = np.interp(grid, self.times, self.strength).mean(axis=1)
                if score.max() > best:
                    best, period, phase = score.max(), candidate, phases[int(score.argmax())]
        self.period, self.phase = period, phase
        # The film begins on the first beat on which a note is struck (after
        # the place asked for): the track's first, taken for the first of a
        # bar.
        count = int((self.times[-1] - phase) / period)
        struck = np.array([self.strength[(self.times >= phase + n * period - 0.04) & (self.times <= phase + n * period + 0.04)].max() for n in range(count)])
        n = max(0, math.ceil((MUSIC["start"] - phase) / period))
        while struck[n] < 0.5 * np.median(struck):
            n += 1
        self.zero = phase + n * period
        # Against beats and onsets found another way, for the report.
        tempo, tracked = librosa.beat.beat_track(y=self.sound, sr=self.rate, hop_length=hop, start_bpm=MUSIC["bpm"], units="time")
        off = (tracked - self.phase + period / 2) % period - period / 2
        self.tracked = (float(np.atleast_1d(tempo)[0]), float(np.median(np.abs(off))), float(np.percentile(np.abs(off), 90)))
        self.onsets = librosa.onset.onset_detect(onset_envelope=self.strength, sr=self.rate, hop_length=hop, units="time", backtrack=False)

    def at(self, n):
        return self.zero + n * self.period


# --- The storyboard ----------------------------------------------------------


def view(cx, cy, width):
    """A 16:9 part of the screen that wide around that point, kept on it."""
    width = min(width, SCREEN[0])
    height = width * 9 / 16
    return (min(max(cx, width / 2), SCREEN[0] - width / 2), min(max(cy, height / 2), SCREEN[1] - height / 2), width)


# The screen under the desktop's top bar, which no shot shows: all of its
# width, so a little wider than 16:9 (the picture is made 3 % taller to
# fill the film's; nothing at a window's edge is cut off). Only to stand
# still on: a view of four numbers is not moved to or from.
BAR = 64
WORK = (SCREEN[0] / 2, BAR + (SCREEN[1] - BAR) / 2, SCREEN[0], SCREEN[1] - BAR)


class Shot:
    """`beats` long. `parts`: (beats, from, to) stretches of the capture, in
    its seconds, one after the other (to=None: the picture at `from`, held;
    to="live": as fast as it happened). `camera`: (beat, view) places, eased
    between; two at one beat are a jump. `caption`: the words under the
    picture, or over it when they begin with ^."""

    def __init__(self, name, parts, camera, caption=None):
        self.name, self.camera, self.caption = name, camera, caption
        self.parts = []
        for beats, start, end in parts:
            self.parts.append((beats, start, end))
        self.beats = sum(part[0] for part in self.parts)

    def source(self, beat, period):
        """The capture's time shown at that beat of the shot, and whether
        that stretch runs as it happened."""
        for beats, start, end in self.parts:
            if beat < beats or (beats, start, end) == self.parts[-1]:
                if end is None:
                    return start, False
                if end == "live":
                    return start + beat * period, True
                return start + (end - start) * min(1.0, beat / beats), False
            beat -= beats

    def place(self, beat):
        before = self.camera[0]
        for after in self.camera:
            if after[0] > beat:
                if after[0] == before[0]:
                    return after[1]
                x = (beat - before[0]) / (after[0] - before[0])
                x = x * x * x * (x * (x * 6 - 15) + 10)
                (ax, ay, aw), (bx, by, bw) = before[1], after[1]
                return (ax + (bx - ax) * x, ay + (by - ay) * x, math.exp(math.log(aw) + (math.log(bw) - math.log(aw)) * x))
            before = after
        return before[1]


def storyboard(capture):
    at, box, changes = capture.at, capture.box, capture.changes
    whole = (0, 0, *SCREEN)

    def changed(start, part, **how):
        seen = changes(start, part, **how)
        if seen is None:
            print(f"demo-film: nothing changed in {part} after {start:.2f}; cutting on the moment as marked", file=sys.stderr)
            return start + 0.3
        return seen

    palette = box("open-palette")
    px, py = palette[0] + palette[2] / 2, palette[1] + palette[3] / 2
    # The palette nearly as large as it is, a little above the middle: the
    # caption has the room below it.
    PALETTE = view(px, py + 85, 2200)
    shots = []

    # The name over the desktop, then the palette on the beat, a search, and
    # the application there on the next bar.
    appears = changed(at("open-desktop"), palette)
    editor = box("open-editor")
    shots.append(Shot("title", [(4, at("open-desktop"), None)], [(0, WORK)]))
    shots.append(Shot("open", [
        (1, appears - 0.02, "live"),
        (2, at("open-typing", -0.1), at("open-typed", 0.3)),
        (1, at("open-found", -0.1), None),
        (4, at("open-editor", 1.2), None),
    ], [(0, view(px, py, 2560)), (4, view(px, py, 2560)), (4, view(editor[0] + editor[2] / 2, editor[1] + editor[3] / 2, 3200))]))

    # Dictation. The window that listens, close; then the words where the
    # cursor is, as they are said.
    pill = box("english-pill")
    note = box("dictation-editor")
    nx = note[0] + note[2] / 2
    WIDE = view(nx, (note[1] + pill[1] + pill[3]) / 2, 2880)
    PILL = view(pill[0] + pill[2] / 2, pill[1] + pill[3] / 2 - 50, 1100)
    WORDS = view(nx, note[1] + 440, 1920)
    # The cut to the words comes a beat and a half into the sentence: the
    # window is seen hearing it first.
    listening = speech_starts(capture, "english") + SPEECH["english"]["sentences"][0][0] - 2.5 * PERIOD
    shots.append(Shot("dictation-listening", [(4, listening, "live")], [(0, WIDE), (0.6, WIDE), (2.2, PILL)], "Say it. It’s typed."))
    shots.append(Shot("dictation-words", [(8, listening + 4 * PERIOD, "live")], [(0, WORDS)], "Say it. It’s typed."))
    # Chinese with English in it: the first two sentences, then the last.
    heard = speech_starts(capture, "chinese")
    sentences = SPEECH["chinese"]["sentences"]
    shots.append(Shot("dictation-mixed", [(12, heard + sentences[0][0] - 0.3, "live")], [(0, WORDS)], "Mix languages mid-sentence."))
    shots.append(Shot("dictation-mixed-on", [(8, heard + sentences[-1][0] - 0.25, "live")], [(0, view(nx, note[1] + 705, 1740))], "Mix languages mid-sentence."))

    # Windows: asked for by name once, then a layout on every beat.
    shots.append(Shot("windows-ask", [
        (2, at("windows-untidy"), None),
        (4, at("windows-palette", -0.6), at("windows-typed", -0.6)),
        (2, at("windows-typed", -0.1), None),
    ], [(0, view(px, py, 3000)), (1.2, view(px, py, 3000)), (2.4, PALETTE)], "Windows, right where you want them."))
    shots.append(Shot("windows-layouts", [
        (2, at("windows-1"), None), (2, at("windows-2"), None), (2, at("windows-3"), None), (2, at("windows-4"), None),
        (1, at("windows-5"), None), (1, at("windows-6"), None), (2, at("windows-7"), None),
    ], [(0, WORK)], "Windows, right where you want them."))

    # A screenshot: selected, marked, pinned, and still there while the work
    # goes on.
    selected = box("screenshot-selected")
    sx, sy = selected[0] + selected[2] / 2, selected[1] + selected[3] / 2
    LEFT = view(0, BAR + 2600 * 9 / 32, 2600)
    MARKS = view(box("windows-7")[0] + 940, sy + 300, 1880)
    BESIDE = view(0, BAR + 2800 * 9 / 32, 2800)
    dimmed = changed(at("screenshot-before"), (2100, 200, 1400, 700))
    pinned = changed(at("screenshot-pin-button", 0.1), (2100, 200, 1400, 700))
    shots.append(Shot("screenshot-select", [
        (1, dimmed - 0.02, "live"),
        (2, at("screenshot-drag", -0.4), at("screenshot-drag", 1.4)),
        (1, at("screenshot-selected", -0.1), None),
    ], [(0, LEFT)], "Capture it. Mark it up."))
    shots.append(Shot("screenshot-mark", [
        (2, at("screenshot-selected"), at("screenshot-rectangle", -0.3)),
        (2, at("screenshot-rectangle", -0.3), at("screenshot-arrow", -0.3)),
        (3, at("screenshot-arrow", -0.3), at("screenshot-text", -0.5)),
        (1, at("screenshot-text", -0.1), None),
    ], [(0, MARKS)], "Capture it. Mark it up."))
    shots.append(Shot("screenshot-pin", [
        (1, pinned - 0.02, "live"),
        (3, at("screenshot-carry", -0.3), at("screenshot-carried", -0.4)),
    ], [(0, MARKS), (0.2, MARKS), (1.6, BESIDE)], "Pin it where you need it."))
    shots.append(Shot("screenshot-beside", [
        (3, at("screenshot-work", 0.1), at("screenshot-worked", -1.0)),
        (1, at("screenshot-worked", -0.1), None),
    ], [(0, BESIDE)], "Pin it where you need it."))

    # The clipboard's history, then a thing found by what is remembered of it.
    shots.append(Shot("clipboard", [(4, at("clipboard-open", -0.1), None)], [(0, PALETTE)], "Everything you copied, found again."))
    shots.append(Shot("clipboard-links", [
        (2, at("clipboard-typing-1", -0.05), at("clipboard-typing-1", 1.0)),
        (2, at("clipboard-links", -0.1), None),
    ], [(0, PALETTE)], "Everything you copied, found again."))
    shots.append(Shot("clipboard-terminal", [
        (2, at("clipboard-typing-2", -0.05), at("clipboard-typing-2", 1.8)),
        (2, at("clipboard-terminal", -0.1), None),
    ], [(0, PALETTE)], "Everything you copied, found again."))

    # The calculator: the question typed, the camera on the answer. The
    # caption is above here: below is the rest of the palette.
    ASKED = view(px, palette[1] + 210, 1840)
    ANSWER = view(px, palette[1] + 210, 1560)
    shots.append(Shot("calculator-currency", [
        (3, at("calculator-typing-1", -0.1), at("calculator-typing-1", 2.0)),
        (3, at("calculator-currency", -0.1), None),
    ], [(0, ASKED), (2.6, ASKED), (4, ANSWER)], "^Answers as you type."))
    shots.append(Shot("calculator-time", [
        (3, at("calculator-typing-2", -0.1), at("calculator-typing-2", 2.3)),
        (3, at("calculator-time", -0.1), None),
    ], [(0, ANSWER)], "^Answers as you type."))

    # The Raycast Store: searched, Enter, installed, running.
    shots.append(Shot("store", [
        (2, at("store-open", -0.1), None),
        (3, at("store-typing", -0.05), at("store-typing", 1.7)),
        (3, at("store-found", -0.1), None),
    ], [(0, PALETTE)], "Raycast extensions, one Enter away."))
    shots.append(Shot("store-install", [
        (1, at("store-enter", -0.02), "live"),
        (3, at("store-settled", -0.1), None),
    ], [(0, PALETTE)], "Raycast extensions, one Enter away."))
    shots.append(Shot("store-running", [
        (2, at("store-running", -0.1), None),
        (2, at("store-typing-2", -0.05), at("store-typing-2", 1.0)),
        (4, at("store-blue", -0.1), None),
    ], [(0, PALETTE)], "Raycast extensions, one Enter away."))
    shots.append(Shot("end", [(12, at("open-desktop"), None)], [(0, WORK)]))
    return shots


# --- What is said ------------------------------------------------------------

SPEECH = {}


def speech(directory):
    """What was spoken, and where each sentence of it begins and ends."""
    for name, file in (("english", "spoken-en.wav"), ("chinese", "spoken-zh.wav")):
        with wave.open(str(directory / file)) as recording:
            rate = recording.getframerate()
            sound = np.frombuffer(recording.readframes(recording.getnframes()), np.int16).astype(np.float32) / 32768
        loud = np.abs(sound[: len(sound) // 160 * 160]).reshape(-1, 160).max(axis=1) > 0.01
        sentences, begun, last = [], None, None
        for index, is_loud in enumerate(loud):
            now = index * 160 / rate
            if is_loud:
                begun, last = now if begun is None else begun, now
            elif begun is not None and now - last > 0.6:
                sentences.append((begun, last))
                begun = None
        if begun is not None:
            sentences.append((begun, last))
        # Each recording as loud as the other, whatever voice made it.
        spoken = np.abs(sound) > 0.02
        sound = sound * min(0.16 / np.sqrt((sound[spoken] ** 2).mean()), 0.95 / np.abs(sound).max())
        SPEECH[name] = {"sound": sound, "rate": rate, "sentences": sentences}


def speech_starts(capture, name):
    """When Heyo began to hear the recording, in the pictures' time: when
    its window stops saying that it is starting. (Later dictations have
    nothing to start: then it is when the window appears.)"""
    if "starts" not in SPEECH[name]:
        x, y, w, h = capture.box(name + "-pill")
        appeared = capture.changes(capture.at(name + "-asked"), (x, y, w, h)) or capture.at(name + "-asked", 0.3)
        words = (x + w * 2 // 5, y + 8, w // 2, h - 16)
        said = capture.changes(appeared + 0.15, words, within=2.5, by=6.0)
        SPEECH[name]["starts"] = said if said is not None else appeared
    return SPEECH[name]["starts"]


# --- Words on the picture ----------------------------------------------------


def font(size, weight=600):
    """Inter, the interface's own typeface, from the packages the
    interface is built with."""
    scale = SIZE[0] / 1920
    made = CACHE / "inter.ttf"
    if not made.exists():
        packed = REPO / "node_modules/@fontsource-variable/inter/files/inter-latin-wght-normal.woff2"
        if packed.exists():
            made.parent.mkdir(parents=True, exist_ok=True)
            unpacked = TTFont(packed)
            unpacked.flavor = None
            unpacked.save(made)
    try:
        face = ImageFont.truetype(str(made), round(size * scale))
        face.set_variation_by_axes([weight])
        return face
    except OSError:
        return ImageFont.truetype("DejaVuSans-Bold.ttf" if weight > 500 else "DejaVuSans.ttf", round(size * scale))


def caption_picture(words):
    """A caption: white words on a dark, rounded bar."""
    scale = SIZE[0] / 1920
    face = font(42, 600)
    left, top, right, bottom = face.getbbox(words)
    pad_x, pad_y = round(34 * scale), round(20 * scale)
    height = round(58 * scale) + 2 * pad_y
    picture = Image.new("RGBA", (right - left + 2 * pad_x, height), (0, 0, 0, 0))
    draw = ImageDraw.Draw(picture)
    draw.rounded_rectangle((0, 0, picture.width - 1, picture.height - 1), height // 2, fill=(13, 15, 20, 214))
    draw.text((pad_x - left, height / 2), words, font=face, fill=(255, 255, 255, 255), anchor="lm")
    return np.asarray(picture).astype(np.float32)


def lay(frame, picture, cx, cy, opacity):
    """`picture` (RGBA) over the frame, its middle at cx, cy."""
    if opacity <= 0:
        return
    h, w = picture.shape[:2]
    x, y = round(cx - w / 2), round(cy - h / 2)
    x0, y0, x1, y1 = max(x, 0), max(y, 0), min(x + w, frame.shape[1]), min(y + h, frame.shape[0])
    if x0 >= x1 or y0 >= y1:
        return
    part = picture[y0 - y : y1 - y, x0 - x : x1 - x]
    alpha = part[..., 3:4] * (opacity / 255)
    under = frame[y0:y1, x0:x1].astype(np.float32)
    frame[y0:y1, x0:x1] = (under + (part[..., 2::-1] - under) * alpha).astype(np.uint8)


def ease(x):
    x = min(1.0, max(0.0, x))
    return x * x * (3 - 2 * x)


def icon(size):
    return Image.open(REPO / "src-tauri/icons/icon.png").convert("RGBA").resize((size, size), Image.Resampling.LANCZOS)


def shadowed(size, draw_on, blur, strength):
    """What `draw_on` draws, with a soft shadow of it underneath."""
    layer = Image.new("RGBA", size, (0, 0, 0, 0))
    draw_on(layer)
    shadow = Image.new("RGBA", size, (0, 0, 0, 0))
    shadow.putalpha(layer.getchannel("A").point(lambda a: int(a * strength)))
    shadow = shadow.filter(ImageFilter.GaussianBlur(blur))
    return Image.alpha_composite(shadow, layer)


def title_picture(dark):
    """The name, large, with the icon beside it and a line under it."""
    scale = SIZE[0] / 1920
    ink = (255, 255, 255, 255) if dark else (12, 28, 62, 255)
    name, line = font(168, 700), font(46, 500)
    mark = icon(round(190 * scale))
    gap = round(44 * scale)
    name_width = round(name.getlength("Heyo"))
    width = max(mark.width + gap + name_width, round(line.getlength("The launcher for Linux"))) + round(120 * scale)
    height = round(400 * scale)

    def draw_on(layer):
        draw = ImageDraw.Draw(layer)
        x = (width - (mark.width + gap + name_width)) // 2
        layer.alpha_composite(mark, (x, round(40 * scale)))
        draw.text((x + mark.width + gap, round(40 * scale) + mark.height / 2), "Heyo", font=name, fill=ink, anchor="lm")
        draw.text((width / 2, round(310 * scale)), "The launcher for Linux", font=line, fill=ink, anchor="mm")

    return np.asarray(shadowed((width, height), draw_on, 18 * scale, 0.25 if dark else 0.0)).astype(np.float32)


def end_card(dark):
    """The backdrop of the two cards (the desktop's, with nothing on it),
    and the last card's layers, which come one after the other: the name,
    what it is, where it is, and the music's credit."""
    scale = SIZE[0] / 1920
    spec = importlib.util.spec_from_file_location("backdrop", HERE / "readme-backdrop.py")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    back = np.asarray(module.backdrop("dark" if dark else "light", *SIZE))[..., ::-1].copy()
    ink = (255, 255, 255, 255) if dark else (12, 28, 62, 255)
    soft = (255, 255, 255, 225) if dark else (12, 28, 62, 225)
    layers = []

    def layer(draw_on):
        picture = Image.new("RGBA", SIZE, (0, 0, 0, 0))
        draw_on(picture, ImageDraw.Draw(picture))
        layers.append(np.asarray(picture).astype(np.float32))

    mark = icon(round(210 * scale))
    name = font(150, 700)
    total = mark.width + round(44 * scale) + round(name.getlength("Heyo"))
    x = (SIZE[0] - total) // 2

    def first(picture, draw):
        picture.alpha_composite(mark, (x, round(290 * scale)))
        draw.text((x + mark.width + round(44 * scale), round(290 * scale) + mark.height / 2), "Heyo", font=name, fill=ink, anchor="lm")

    def second(picture, draw):
        draw.text((SIZE[0] / 2, round(600 * scale)), "Free and open source", font=font(54, 600), fill=ink, anchor="mm")

    def third(picture, draw):
        face = font(44, 600)
        words = "github.com/hdonix/heyo"
        half, tall = face.getlength(words) / 2 + 44 * scale, 44 * scale
        draw.rounded_rectangle((SIZE[0] / 2 - half, 710 * scale - tall, SIZE[0] / 2 + half, 710 * scale + tall), tall, fill=(13, 15, 20, 224))
        draw.text((SIZE[0] / 2, round(710 * scale)), words, font=face, fill=(255, 255, 255, 255), anchor="mm")

    def credit(picture, draw):
        draw.text((SIZE[0] / 2, round(990 * scale)), MUSIC["credit"], font=font(34, 500), fill=soft, anchor="mm")

    for draw_on in (first, second, third):
        layer(draw_on)
    if MUSIC["credit"]:
        layer(credit)
    return back, layers


# --- The film ----------------------------------------------------------------


def framed(picture, place):
    """That view of the capture's picture, at the film's size."""
    cx, cy, width = place[:3]
    w = max(2, round(width / 2) * 2)
    h = round(place[3]) if len(place) > 3 else round(w * 9 / 32) * 2
    x = int(min(max(round(cx - w / 2), 0), SCREEN[0] - w))
    y = int(min(max(round(cy - h / 2), 0), SCREEN[1] - h))
    part = picture[y : y + h, x : x + w]
    return cv2.resize(part, SIZE, interpolation=cv2.INTER_AREA if w >= SIZE[0] else cv2.INTER_CUBIC)


def soundtrack(music_path, beats, shots, starts, length, period):
    """The music from the beat the film begins on, turned down under what
    is said, and what is said where its pictures are."""
    track = sound_of(music_path, SOUND, 2)
    first = round(beats.zero * SOUND)
    music = np.zeros((length, 2), np.float32)
    have = track[first : first + length]
    music[: len(have)] = have
    music *= 0.5 / max(1e-6, np.percentile(np.abs(music), 99.9))
    said = np.zeros(length, np.float32)
    for shot, shot_start in zip(shots, starts):
        beat = 0
        for part_beats, start, end in shot.parts:
            if end == "live":
                for name, spoken in SPEECH.items():
                    if "starts" not in spoken:
                        continue
                    # The recording was played from when Heyo began to hear.
                    sound = librosa.resample(spoken["sound"], orig_sr=spoken["rate"], target_sr=SOUND)
                    a = round((start - spoken["starts"]) * SOUND)
                    n = round(part_beats * period * SOUND)
                    piece = sound[max(a, 0) : max(a + n, 0)].copy()
                    if not len(piece) or np.abs(piece).max() < 0.02:
                        continue
                    fade = min(len(piece) // 2, SOUND // 25)
                    piece[:fade] *= np.linspace(0, 1, fade)
                    piece[-fade:] *= np.linspace(1, 0, fade)
                    out = round((shot_start + beat * period) * SOUND) + max(-a, 0)
                    piece = piece[: max(0, length - out)]
                    said[out : out + len(piece)] += piece
            beat += part_beats
    # Down under speech, and through the pauses between sentences: from
    # a second before a word to a second after the last.
    speaking = (np.abs(said) > 0.02).astype(np.float32)
    window = SOUND // 2
    near = fftconvolve(speaking, np.ones(4 * window), mode="same") > 0.5
    duck = np.clip(fftconvolve(near.astype(np.float32), np.hanning(window) / np.hanning(window).sum(), mode="same"), 0, 1)
    music *= (1 - 0.72 * duck)[:, None]
    # In at once, on its first note; out with the last card.
    out = min(length, round(2.5 * SOUND))
    music[-out:] *= np.linspace(1, 0, out)[:, None] ** 2
    mixed = np.clip(music + said[:, None], -1, 1)
    return mixed, speaking


def main():
    global PERIOD
    capture = Capture(FILM)
    speech(FILM)
    music_path = music_file()
    beats = Beats(music_path)
    PERIOD = period = beats.period
    print(f"demo-film: the music is at {60 / period:.2f} beats a minute (listed: {MUSIC['bpm']:g}); the film begins {beats.zero:.3f} s into it")
    speech_starts(capture, "english")
    shots = storyboard(capture)

    # Where each shot begins: on a beat, at the picture nearest to it.
    start_beats = np.concatenate([[0], np.cumsum([shot.beats for shot in shots])])
    cuts = [round(beat * period * RATE) for beat in start_beats]
    total = cuts[-1]
    dark = capture.read(capture.number_at(capture.at("open-desktop")))[::40, ::40].mean() < 110

    captions = {}
    for shot in shots:
        if shot.caption and shot.caption not in captions:
            captions[shot.caption] = caption_picture(shot.caption.lstrip("^"))
    # A caption stays over the shots that share it: when it comes and goes.
    spans = []
    for index, shot in enumerate(shots):
        if not shot.caption:
            continue
        if spans and spans[-1][0] == shot.caption and spans[-1][2] == cuts[index]:
            spans[-1][2] = cuts[index + 1]
        else:
            spans.append([shot.caption, cuts[index], cuts[index + 1]])
    title = title_picture(dark)
    card, layers = end_card(dark)

    sound, _ = soundtrack(music_path, beats, shots, [cut / RATE for cut in cuts[:-1]], round(total / RATE * SOUND), period)
    sound_path = FILM / "sound.wav"
    with wave.open(str(sound_path), "wb") as out:
        out.setnchannels(2)
        out.setsampwidth(2)
        out.setframerate(SOUND)
        out.writeframes((sound * 32767).astype(np.int16).tobytes())

    outputs = ["-map", "0:v", "-map", "1:a", "-c:v", "libx264", "-preset", "ultrafast" if DRAFT else "slow", "-crf", "18", "-pix_fmt", "yuv420p",
               "-c:a", "aac", "-b:a", "192k", "-movflags", "+faststart", FILM / ("draft.mp4" if DRAFT else "heyo.mp4")]
    if not DRAFT:
        outputs += ["-map", "0:v", "-map", "1:a", "-c:v", "libvpx-vp9", "-crf", "30", "-b:v", "0", "-row-mt", "1", "-cpu-used", "2", "-pix_fmt", "yuv420p",
                    "-c:a", "libopus", "-b:a", "128k", FILM / "heyo.webm"]
    encoder = subprocess.Popen([FFMPEG, "-hide_banner", "-loglevel", "error", "-y", "-f", "rawvideo", "-pix_fmt", "bgr24", "-s", f"{SIZE[0]}x{SIZE[1]}", "-r", str(RATE), "-i", "-",
                                "-i", str(sound_path), *map(str, outputs)], stdin=subprocess.PIPE)
    stills = FILM / "stills"
    if STILLS:
        stills.mkdir(exist_ok=True)
        for old in stills.glob("*.jpg"):
            old.unlink()

    fade = RATE // 4
    index = 0
    for number in range(total):
        while number >= cuts[index + 1]:
            index += 1
        shot = shots[index]
        beat = max(0.0, number / RATE / period - start_beats[index])
        if shot.name == "title":
            frame = card.copy()
        elif shot.name == "end":
            frame = card.copy()
            for place, picture in enumerate(layers):
                # One on each second beat; the credit with the last of them.
                since = beat - 2 * min(place, 2)
                lay(frame, picture, SIZE[0] / 2, SIZE[1] / 2 + (1 - ease(since / 0.8)) * 14 * SIZE[1] / 1080, ease(since / 0.8))
        else:
            seconds, _ = shot.source(beat, period)
            frame = framed(capture.read(capture.number_at(seconds)), shot.place(beat)).copy()
        if shot.name == "title":
            shown = ease(number / (0.5 * RATE)) * ease((cuts[1] - number) / (0.2 * RATE))
            lay(frame, title, SIZE[0] / 2, SIZE[1] / 2 - (1 - ease(number / (0.6 * RATE))) * 10, shown)
        for words, comes, goes in spans:
            if comes <= number < goes:
                shown = ease((number - comes - 3) / fade) * ease((goes - 2 - number) / fade)
                height = 0.115 if words.startswith("^") else 0.905
                lay(frame, captions[words], SIZE[0] / 2, SIZE[1] * height + (1 - ease((number - comes - 3) / fade)) * 10 * SIZE[1] / 1080, shown)
        # Out of black, and into it.
        dim = ease((number + 1) / (0.4 * RATE)) * ease((total - number) / (0.9 * RATE))
        if dim < 1:
            frame = (frame * dim).astype(np.uint8)
        encoder.stdin.write(frame.tobytes())
        if STILLS and number in (cuts[index], (cuts[index] + cuts[index + 1]) // 2, cuts[index + 1] - 1):
            where = {cuts[index]: "a", cuts[index + 1] - 1: "c"}.get(number, "b")
            cv2.imwrite(str(stills / f"{index:02d}-{shot.name}-{where}.jpg"), frame, [cv2.IMWRITE_JPEG_QUALITY, 90])
    capture.close()
    encoder.stdin.close()
    if encoder.wait() != 0:
        sys.exit("demo-film: the film could not be written")

    # How far each cut is from a beat: the picture it is in against the
    # grid, and against the nearest note found in the sound by itself.
    lines = [
        f"music: {MUSIC['url']}",
        f"tempo fitted: {60 / period:.3f} beats a minute (listed {MUSIC['bpm']:g}); the film's first picture is {beats.zero:.3f} s into the track",
        f"librosa's own beat tracker: {beats.tracked[0]:.1f} beats a minute, its beats off this grid by {beats.tracked[1] * 1000:.0f} ms (median), {beats.tracked[2] * 1000:.0f} ms (90th percentile)",
        "",
        "cut  picture  seconds  beat  from the grid  from the nearest onset  shot",
    ]
    worst = 0.0
    for place, (cut, beat) in enumerate(zip(cuts, start_beats)):
        off = cut / RATE - beat * period
        onset = beats.onsets[np.argmin(np.abs(beats.onsets - (beats.zero + cut / RATE)))] - (beats.zero + cut / RATE)
        worst = max(worst, abs(off))
        name = shots[place].name if place < len(shots) else "(the end)"
        lines.append(f"{place:3d}  {cut:7d}  {cut / RATE:7.3f}  {int(beat):4d}  {off * 1000:+10.1f} ms  {onset * 1000:+17.1f} ms  {name}")
    lines.append("")
    lines.append(f"the cut furthest from its beat: {worst * 1000:.1f} ms (a picture is {1000 / RATE:.1f} ms)")
    (FILM / "cuts.txt").write_text("\n".join(lines) + "\n")
    print(lines[-1])
    for made in ("draft.mp4",) if DRAFT else ("heyo.mp4", "heyo.webm"):
        print(f"demo-film: {FILM / made}, {total / RATE:.1f} s, {(FILM / made).stat().st_size / 1048576:.1f} MB")


PERIOD = 60 / MUSIC["bpm"]
if __name__ == "__main__":
    main()
