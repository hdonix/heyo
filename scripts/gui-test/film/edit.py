"""edit.py FILM: makes the film out of what demo-film.sh captured.

FILM is the directory the capture left: capture.mkv (the whole 3840x2160
screen), epoch and moments.jsonl (when each thing happened, by the clock the
film's pictures carry, and where on the screen it is), and what was spoken.

The film is put together in three parts:

- The music (music.py, beside this): made here, note by note, at 120 beats
  a minute, from the list of what happens when (plan.py), so that a
  sixteenth is exactly 3 pictures (24 a second). Everything below is
  counted in its beats.
- The plate (FILM/composition/assets/plate.mp4): the capture retimed to the
  film, still 3840x2160. Each shot is so many beats long and says which
  stretch of the capture it shows and how fast (the waiting is left out,
  typing hurried, what is dictated runs as it happened, with its sound).
  A thing that must happen on a beat (a window going to its place, a
  palette appearing) is found in the capture by the picture in which the
  screen changes, and cut there.
- The composition (composition/, beside this): HyperFrames, HTML with GSAP,
  rendered through headless Chromium and FFmpeg. It moves a camera over
  the plate, sets the titles, the captions, the sweeps between scenes and
  the two cards, all on the beat. What it needs to know (where the camera
  is, when each word arrives) is written here into data.js.

Out of it come FILM/heyo.mp4 (H.264 and AAC, 1920x1080, 24 a second),
FILM/heyo.webm (VP9 and Opus), FILM/music.wav, FILM/sound.wav (the music
with what is said, as mixed), FILM/cuts.txt (each cut against the beat)
and FILM/sync.txt (what is seen against what is heard, measured in the
film as made: sync.py).

    HEYO_FILM_DRAFT=1    a quick film, MP4 only, draft quality
    HEYO_FILM_STILLS=1   also FILM/stills/: the first, middle and last
                         picture of every shot and every caption at its
                         fullest, to look the film over
    HEYO_FILM_HYPERFRAMES  the HyperFrames command (demo-film.sh installs it)

Needs numpy, scipy, librosa and imageio-ffmpeg.
"""
import json
import math
import os
import re
import shutil
import subprocess
import sys
import wave
from pathlib import Path

import imageio_ffmpeg
import librosa
import numpy as np
from scipy.signal import fftconvolve

HERE = Path(__file__).resolve().parent
REPO = HERE.parent.parent.parent
sys.path.insert(0, str(HERE))
import music  # noqa: E402
import plan  # noqa: E402
import sync  # noqa: E402

FFMPEG = imageio_ffmpeg.get_ffmpeg_exe()
FILM = Path(sys.argv[1]) if len(sys.argv) > 1 else sys.exit(__doc__)
DRAFT = bool(os.environ.get("HEYO_FILM_DRAFT"))
STILLS = bool(os.environ.get("HEYO_FILM_STILLS"))

SCREEN = (3840, 2160)
RATE = 24  # the film: a sixteenth of a beat is 3 pictures
CAPTURED = 30  # the capture
SOUND = music.RATE
PERIOD = music.BEAT
SECTION = {name: (a, b) for name, a, b in plan.SECTIONS}


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
        return max(0, round((seconds - self.first) * CAPTURED))

    def open(self, number, *filters, length=None):
        # Half a picture early: the first one read is the one asked for.
        arguments = ["-ss", f"{max(0, (number - 0.5) / CAPTURED):.4f}", "-i", self.path]
        if length:
            arguments += ["-frames:v", str(length)]
        if filters:
            arguments += ["-vf", ",".join(filters)]
        return subprocess.Popen([FFMPEG, "-hide_banner", "-loglevel", "error", *map(str, arguments), "-f", "rawvideo", "-pix_fmt", "yuv420p", "-"], stdout=subprocess.PIPE)

    def read(self, number):
        """Picture `number` of the capture, whole, as raw YUV 4:2:0."""
        if number == self.number:
            return self.picture
        size = SCREEN[0] * SCREEN[1] * 3 // 2
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
        self.picture = data
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
        process = subprocess.Popen([FFMPEG, "-hide_banner", "-loglevel", "error", "-ss", f"{max(0, (number - 0.5) / CAPTURED):.4f}", "-i", str(self.path),
                                    "-frames:v", str(round(seconds * CAPTURED)), "-vf", f"crop={w}:{h}:{x}:{y},scale={across}:{down},format=gray",
                                    "-f", "rawvideo", "-pix_fmt", "gray", "-"], stdout=subprocess.PIPE)
        raw = process.communicate()[0]
        return number, np.frombuffer(raw, np.uint8)[: len(raw) // (across * down) * across * down].reshape(-1, down, across).astype(np.int16)

    def changes(self, start, box, within=4.0, by=2.0):
        """When that part of the screen first differs from what it is at
        `start`: the time of the picture it does so in. None if it does not."""
        number, pictures = self.small(start, within, box)
        differs = np.abs(pictures - pictures[0]).mean(axis=(1, 2)) > by
        return (number + int(np.argmax(differs))) / CAPTURED + self.first if differs.any() else None

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
        print(f"edit: the moments' clock is {a * 1000:+.0f} ms from the pictures' at the start, {b * 1000:+.0f} ms at the end")
        return lambda at: a if last == first else a + (b - a) * (at - first) / (last - first)

    def at(self, name, plus=0.0):
        """A moment, in the pictures' time."""
        marked = self.moments[name][0]
        return marked - self.late(marked) + plus

    def box(self, name):
        return tuple(self.moments[name][1])


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


# --- The storyboard ----------------------------------------------------------


def view(cx, cy, width):
    """A 16:9 part of the screen that wide around that point, kept on it."""
    width = min(width, SCREEN[0])
    height = width * 9 / 16
    return (min(max(cx, width / 2), SCREEN[0] - width / 2), min(max(cy, height / 2), SCREEN[1] - height / 2), width)


WHOLE = view(SCREEN[0] / 2, SCREEN[1] / 2, SCREEN[0])
# Without the desktop's top bar: the whole width less 64 pixels' worth.
WORK = view(SCREEN[0] / 2, SCREEN[1], SCREEN[0] - 64 * 16 / 9 * 2)


class Shot:
    """`beats` long. `parts`: (beats, from, to) stretches of the capture, in
    its seconds, one after the other (to=None: the picture at `from`, held;
    to="live": as fast as it happened). `camera`: (beat, view[, ease]) places,
    eased between (GSAP's eases; the default is a slow in and out); two at
    one beat are a jump. `card`: drawn by the composition, not the capture."""

    def __init__(self, name, parts, camera=(), card=None):
        self.name, self.parts, self.camera, self.card = name, parts, list(camera), card
        self.beats = sum(part[0] for part in parts)

    def source(self, beat):
        """The capture's time shown at that beat of the shot."""
        for beats, start, end in self.parts:
            if beat < beats or (beats, start, end) == self.parts[-1]:
                if end is None:
                    return start
                if end == "live":
                    return start + beat * PERIOD
                return start + (end - start) * min(1.0, beat / beats)
            beat -= beats


def typed(capture, start, end, region, count):
    """Where `count` letters were typed into that part of the screen: for
    each, the time of a picture that shows it settled (the last before the
    next letter, or a little after the last). Letters are found as the dark
    of the text grows."""
    number, pictures = capture.small(start, end - start, region)
    ink = (pictures < 140).sum(axis=(1, 2))
    jumps = [i for i in range(1, len(ink)) if ink[i] > ink[i - 1] + 4]
    if len(jumps) < count:
        sys.exit(f"edit: {len(jumps)} letters typed in {region}, not {count}")
    jumps = jumps[:count]
    frames = [b - 1 for b in jumps[1:]] + [min(len(ink) - 1, jumps[-1] + 9)]
    return [(number + frame) / CAPTURED + capture.first for frame in frames]


def storyboard(capture):
    at, box = capture.at, capture.box

    def changed(start, part, **how):
        seen = capture.changes(start, part, **how)
        if seen is None:
            print(f"edit: nothing changed in {part} after {start:.2f}; cutting on the moment as marked", file=sys.stderr)
            return start + 0.3
        return seen

    palette = box("open-palette")
    px, py = palette[0] + palette[2] / 2, palette[1] + palette[3] / 2

    def low(width, foot=845):
        """The palette that wide, its foot `foot` pictures down the film:
        the caption has the room below it."""
        return view(px, palette[1] + palette[3] - foot / 1920 * width + width * 9 / 32, width)

    PALETTE = low(2440)
    shots = []

    # Cold open, retimed so that each thing lands on its beat (plan.OPEN):
    # the desktop; the palette; "edit", a letter a sixteenth (each held from
    # the picture in which it appears); Enter, the palette gone; the editor.
    O = plan.OPEN
    appears = changed(at("open-desktop"), palette)
    letters = typed(capture, at("open-typing", -0.2), at("open-typed", 0.6), (palette[0] + 20, palette[1] + 10, 700, 110), 4)
    gone = changed(at("open-enter", -0.05), (palette[0] + 100, palette[1] + 100, 1300, 700))
    editor = box("open-editor")
    opened = changed(gone + 0.02, (editor[0] + 40, editor[1] + 40, 900, editor[3] - 80))
    steps = [b - a for a, b in zip(O["letters"], O["letters"][1:] + [O["enter"]])]
    shots.append(Shot("open", [
        (O["palette"], appears - 0.6, None),
        (O["letters"][0] - O["palette"], appears, "live"),
        *[(step, shown, None) for step, shown in zip(steps, letters)],
        (O["editor"] - O["enter"], gone, opened - 1 / CAPTURED),
        (O["title"] - O["editor"], opened, "live"),
    ], [(0, view(px, py, 2100)), (O["editor"], view(px, py, 2100)),
        # with the editor, straight to it; then, on its beat, in closer
        (O["editor"], view(editor[0] + editor[2] / 2, editor[1] + editor[3] / 2, 3000)),
        (O["move"] - 1 / (RATE * PERIOD), view(editor[0] + editor[2] / 2, editor[1] + editor[3] / 2, 3000)),
        (O["title"], view(editor[0] + editor[2] / 2, editor[1] + editor[3] / 2 - 120, 2300), "power3.out")]))
    shots.append(Shot("title", [(8, at("open-editor", 1.2), None)], card="title"))

    # Dictation. The window that listens, close; then the words where the
    # cursor is, as they are said, the window still in the picture.
    pill = box("english-pill")
    note = box("dictation-editor")
    nx = note[0] + note[2] / 2
    WIDE = view(nx, (note[1] + pill[1] + pill[3]) / 2, 2880)
    PILL = view(pill[0] + pill[2] / 2, pill[1] + pill[3] / 2 - 50, 1100)
    WORDS = view(nx, SCREEN[1], 2500)
    NEARER = view(nx, SCREEN[1], 2300)
    listening = speech_starts(capture, "english") + SPEECH["english"]["sentences"][0][0] - 2.5 * PERIOD
    shots.append(Shot("dictation-listening", [(4, listening, "live")], [(0, WIDE), (1.5, PILL, "power3.out")]))
    shots.append(Shot("dictation-words", [(8, listening + 4 * PERIOD, "live")], [(0, PILL), (2.5, WORDS, "power2.inOut")]))
    heard = speech_starts(capture, "chinese")
    sentences = SPEECH["chinese"]["sentences"]
    # Chinese with English in it: every sentence, as it was said.
    shots.append(Shot("dictation-mixed", [(20, heard + sentences[0][0] - 0.3, "live")], [(0, WORDS), (20, NEARER, "power1.inOut")]))

    # Window layouts: asked for by name once, then one on every beat.
    shots.append(Shot("windows-ask", [
        (1, at("windows-untidy"), None),
        (2, at("windows-palette", -0.3), at("windows-typed", -0.4)),
        (1, at("windows-typed", -0.1), None),
    ], [(0, view(px, py, 3000)), (1, view(px, py, 3000)), (3, PALETTE, "power2.inOut")]))
    snaps = [f"windows-{k}" for k in range(1, 8)]
    shots.append(Shot("windows-layouts", [(1, at(name), None) for name in snaps[:-1]] + [(2, at(snaps[-1]), None)], [(0, WORK), (8, view(SCREEN[0] / 2, SCREEN[1], 3500), "none")]))

    # A screenshot: selected, marked, pinned, and still there while the work
    # goes on.
    selected = box("screenshot-selected")
    sy = selected[1] + selected[3] / 2
    LEFT = view(0, 64 + 2600 * 9 / 32, 2600)
    MARKS = view(box("windows-7")[0] + 940, sy + 300, 1880)
    BESIDE = view(0, 64 + 2800 * 9 / 32, 2800)
    dimmed = changed(at("screenshot-before"), (2100, 200, 1400, 700))
    pinned = changed(at("screenshot-pin-button", 0.1), (2100, 200, 1400, 700))
    shots.append(Shot("screenshot-select", [
        (1, dimmed - 0.02, "live"),
        (2, at("screenshot-drag", -0.4), at("screenshot-drag", 1.4)),
        (1, at("screenshot-selected", -0.1), None),
    ], [(0, LEFT), (4, view(LEFT[0] + 200, LEFT[1] + 60, 2300), "power2.inOut")]))
    shots.append(Shot("screenshot-mark", [
        (1.5, at("screenshot-selected"), at("screenshot-rectangle", -0.3)),
        (1.5, at("screenshot-rectangle", -0.3), at("screenshot-arrow", -0.3)),
        (2, at("screenshot-arrow", -0.3), at("screenshot-text", -0.5)),
        (1, at("screenshot-text", -0.1), None),
    ], [(0, MARKS), (6, view(MARKS[0] + 60, MARKS[1], 1700), "none")]))
    shots.append(Shot("screenshot-pin", [
        (1, pinned - 0.02, "live"),
        (2, at("screenshot-carry", -0.3), at("screenshot-carried", -0.4)),
    ], [(0, MARKS), (0.2, MARKS), (1.8, BESIDE, "power3.inOut")]))
    shots.append(Shot("screenshot-beside", [
        (2, at("screenshot-work", 0.1), at("screenshot-worked", -1.0)),
        (1, at("screenshot-worked", -0.1), None),
    ], [(0, BESIDE)]))

    # The clipboard's history, then a thing found by what is remembered of it.
    shots.append(Shot("clipboard", [(2, at("clipboard-open", -0.1), None)], [(0, low(2520)), (2, low(2480), "none")]))
    shots.append(Shot("clipboard-links", [
        (2, at("clipboard-typing-1", -0.05), at("clipboard-typing-1", 1.0)),
        (2, at("clipboard-links", -0.1), None),
    ], [(0, low(2480)), (4, low(2420), "power2.inOut")]))
    shots.append(Shot("clipboard-terminal", [
        (2, at("clipboard-typing-2", -0.05), at("clipboard-typing-2", 1.8)),
        (4, at("clipboard-terminal", -0.1), None),
    ], [(0, low(2420)), (6, low(2400), "none")]))

    # The calculator: the question typed, the camera on the answer.
    ASKED = view(px, palette[1] + 210, 1840)
    ANSWER = view(px, palette[1] + 210, 1560)
    shots.append(Shot("calculator-currency", [
        (2, at("calculator-typing-1", -0.1), at("calculator-typing-1", 2.0)),
        (2, at("calculator-currency", -0.1), None),
    ], [(0, ASKED), (1.8, ASKED), (3, ANSWER, "power3.inOut")]))
    shots.append(Shot("calculator-time", [
        (2, at("calculator-typing-2", -0.1), at("calculator-typing-2", 2.3)),
        (2, at("calculator-time", -0.1), None),
    ], [(0, ANSWER), (4, view(px, palette[1] + 210, 1480), "none")]))

    # The Raycast Store: searched, Enter, installed, running.
    shots.append(Shot("store", [
        (1, at("store-open", -0.1), None),
        (2, at("store-typing", -0.05), at("store-typing", 1.7)),
        (1, at("store-found", -0.1), None),
    ], [(0, low(2520)), (4, low(2480), "none")]))
    shots.append(Shot("store-install", [
        (1, at("store-enter", -0.02), "live"),
        (3, at("store-settled", -0.1), None),
    ], [(0, low(2480)), (4, low(2420), "none")]))
    shots.append(Shot("store-running", [
        (1, at("store-running", -0.1), None),
        (2, at("store-typing-2", -0.05), at("store-typing-2", 1.0)),
        (5, at("store-blue", -0.1), None),
    ], [(0, low(2440)), (3, low(2440)), (8, low(2400), "power2.inOut")]))
    shots.append(Shot("end", [(plan.LENGTH - SECTION["end"][0], at("open-editor", 1.2), None)], card="end"))
    return shots


# --- The plate -----------------------------------------------------------------


def plate(capture, shots, starts, total, path):
    """The capture retimed to the film: picture for picture, whole."""
    blank = bytes(SCREEN[0] * SCREEN[1]) + bytes([128]) * (SCREEN[0] * SCREEN[1] // 2)
    encoder = subprocess.Popen([FFMPEG, "-hide_banner", "-loglevel", "error", "-y", "-f", "rawvideo", "-pix_fmt", "yuv420p", "-s", f"{SCREEN[0]}x{SCREEN[1]}",
                                "-r", str(RATE), "-i", "-", "-c:v", "libx264", "-preset", "veryfast", "-crf", "16" if DRAFT else "12", "-g", str(RATE),
                                "-pix_fmt", "yuv420p", "-movflags", "+faststart", str(path)], stdin=subprocess.PIPE)
    index = 0
    for number in range(total):
        while index + 1 < len(shots) and number >= starts[index + 1]:
            index += 1
        shot = shots[index]
        if shot.card:
            encoder.stdin.write(blank)
            continue
        beat = (number - starts[index]) / RATE / PERIOD
        encoder.stdin.write(capture.read(capture.number_at(shot.source(beat))))
    capture.close()
    encoder.stdin.close()
    if encoder.wait() != 0:
        sys.exit("edit: the plate could not be written")


# --- The sound -----------------------------------------------------------------


def soundtrack(hits, bed, shots, starts, length):
    """The music, turned down under what is said (the bed by 12 dB, the hits
    by 4: a cut or a word still lands), and what is said where its pictures
    are."""
    said = np.zeros(length, np.float32)
    for shot, shot_start in zip(shots, starts):
        beat = 0
        for part_beats, start, end in shot.parts:
            if end == "live":
                for spoken in SPEECH.values():
                    if "starts" not in spoken:
                        continue
                    # The recording was played from when Heyo began to hear.
                    sound = librosa.resample(spoken["sound"], orig_sr=spoken["rate"], target_sr=SOUND)
                    a = round((start - spoken["starts"]) * SOUND)
                    n = round(part_beats * PERIOD * SOUND)
                    piece = sound[max(a, 0) : max(a + n, 0)].copy()
                    if not len(piece) or np.abs(piece).max() < 0.02:
                        continue
                    fade = min(len(piece) // 2, SOUND // 25)
                    piece[:fade] *= np.linspace(0, 1, fade)
                    piece[-fade:] *= np.linspace(1, 0, fade)
                    out = round((shot_start / RATE + beat * PERIOD) * SOUND) + max(-a, 0)
                    piece = piece[: max(0, length - out)]
                    said[out : out + len(piece)] += piece
            beat += part_beats
    # both parts at the level the music has alone
    gain = 10 ** ((-14 - music.loudness(hits + bed)) / 20)
    parts = []
    for part in (hits, bed):
        whole = np.zeros((length, 2))
        whole[: min(length, len(part))] = part[:length] * gain
        parts.append(whole)
    # Down under speech, and through the pauses between sentences: from
    # half a second before a word to a second after the last.
    speaking = (np.abs(said) > 0.02).astype(np.float32)
    window = SOUND // 4
    near = fftconvolve(speaking, np.ones(6 * window), mode="same") > 0.5
    duck = np.clip(fftconvolve(near.astype(np.float32), np.hanning(window) / np.hanning(window).sum(), mode="same"), 0, 1)
    tune = parts[0] * (1 - 0.37 * duck)[:, None] + parts[1] * (1 - 0.75 * duck)[:, None]
    return music.glue(tune + said[:, None] * 1.25), duck


def write_wav(path, sound):
    with wave.open(str(path), "wb") as out:
        out.setnchannels(2)
        out.setsampwidth(2)
        out.setframerate(SOUND)
        out.writeframes((np.clip(sound, -1, 1) * 32767).astype(np.int16).tobytes())


# --- The composition ---------------------------------------------------------


def composition(shots, start_beats, capture, total, project):
    """data.js: what the composition needs, in seconds of the film."""
    cameras = []
    for shot, beat0 in zip(shots, start_beats):
        if shot.card:
            continue
        keys = []
        for key in shot.camera:
            b, place = key[0], key[1]
            ease = key[2] if len(key) > 2 else "power2.inOut"
            keys.append({"t": round((beat0 + b) * PERIOD, 4), "x": round(place[0], 1), "y": round(place[1], 1), "w": round(place[2], 1), "ease": ease})
        cameras.append({"from": round(beat0 * PERIOD, 4), "to": round((beat0 + shot.beats) * PERIOD, 4), "keys": keys})
    # Where each window went, flashed on the beat it went there on.
    layouts = next(s for s in shots if s.name == "windows-layouts")
    beat0 = start_beats[shots.index(layouts)]
    flashes = []
    for k in range(1, 8):
        x, y, w, h = capture.box(f"windows-{k}")
        # a little inside the window, so that all four sides are seen
        flashes.append({"t": round((beat0 + k - 1) * PERIOD, 4), "x": x + 28, "y": y + 28, "w": w - 56, "h": h - 56})
    seconds = lambda beats: [round(b * PERIOD, 4) for b in beats]  # noqa: E731
    data = {
        "duration": total / RATE,
        "rate": RATE,
        "beat": PERIOD,
        "cards": [{"kind": s.card, "from": round(b * PERIOD, 4), "to": round((b + s.beats) * PERIOD, 4)} for s, b in zip(shots, start_beats) if s.card],
        "cameras": cameras,
        "flashes": flashes,
        "punches": [[round(b * PERIOD, 4), amount] for b, amount in plan.punches()],
        "wipes": [{"t": round(b * PERIOD, 4), "word": word} for b, word in sorted(plan.WIPES.items())],
        "title": {key: (seconds(value) if isinstance(value, list) else round(value * PERIOD, 4)) for key, value in plan.OPEN.items()}
        | {"sub_words": plan.TITLE_SUB, "pills_words": plan.TITLE_PILLS},
        "end": {key: (seconds(value) if isinstance(value, (list, tuple)) else round(value * PERIOD, 4)) for key, value in plan.END.items()}
        | {"address": plan.ADDRESS},
        "captions": [
            {"from": c["from"] * PERIOD, "to": c["to"] * PERIOD, "kicker": c["kicker"], "place": c["place"],
             "words": [{"text": w, "t": t * PERIOD, "accent": w in c["accent"]} for w, t in c["words"]]}
            for c in plan.CAPTIONS
        ],
    }
    (project / "data.js").write_text("window.FILM = " + json.dumps(data, indent=1, ensure_ascii=False) + ";\n")
    for name in ("film.js", "film.css"):
        shutil.copy(HERE / "composition" / name, project / name)
    page = (HERE / "composition" / "index.html").read_text().replace("{{DURATION}}", f"{total / RATE:.4f}")
    (project / "index.html").write_text(page)
    shutil.copy(REPO / "src-tauri/fonts/Inter-Variable.ttf", project / "assets/Inter-Variable.ttf")
    shutil.copy(REPO / "src-tauri/icons/icon.png", project / "assets/icon.png")
    return data


def render(project, out):
    command = os.environ.get("HEYO_FILM_HYPERFRAMES", "hyperframes").split()
    env = dict(os.environ, HYPERFRAMES_NO_TELEMETRY="1", DO_NOT_TRACK="1", HYPERFRAMES_SKIP_SKILLS="1")
    env.pop("DISPLAY", None)
    env.pop("WAYLAND_DISPLAY", None)
    # The FFmpeg it calls by name: the one the edit uses.
    tools = FILM / "tools"
    tools.mkdir(exist_ok=True)
    if not (tools / "ffmpeg").exists():
        (tools / "ffmpeg").symlink_to(FFMPEG)
    env["PATH"] = f"{tools}:{env['PATH']}"
    arguments = [*command, "render", "-o", str(out), "--fps", str(RATE), "--quality", "draft" if DRAFT else "high", "--video-frame-format", "png"]
    print("edit: " + " ".join(arguments))
    if subprocess.run(arguments, cwd=project, env=env, stdout=subprocess.DEVNULL).returncode != 0:
        sys.exit("edit: HyperFrames could not render the film")


def stills(out, shots, start_beats, data):
    """The first, middle and last picture of each shot, and every caption
    when all its words are there, to look the film over."""
    folder = FILM / "stills"
    shutil.rmtree(folder, ignore_errors=True)
    folder.mkdir()
    wanted = {}
    for shot, beat0 in zip(shots, start_beats):
        a, b = round(beat0 * PERIOD * RATE), round((beat0 + shot.beats) * PERIOD * RATE) - 1
        for where, number in (("a", a), ("b", (a + b) // 2), ("c", b)):
            wanted[number] = f"{shot.name}-{where}"
    for k, caption in enumerate(data["captions"]):
        wanted[round((caption["words"][-1]["t"] + 0.6) * RATE)] = f"caption-{k}"
    for wipe in data["wipes"]:
        wanted[round(wipe["t"] * RATE)] = f"wipe-{wipe['word']}"
    numbers = sorted(wanted)
    pick = "+".join(f"eq(n\\,{n})" for n in numbers)
    subprocess.run([FFMPEG, "-hide_banner", "-loglevel", "error", "-i", str(out), "-vf", f"select='{pick}'", "-vsync", "0", "-q:v", "3", str(folder / "%03d.jpg")], check=True)
    for k, number in enumerate(numbers):
        made = folder / f"{k + 1:03d}.jpg"
        if made.exists():
            made.rename(folder / f"{number:05d}-{wanted[number]}.jpg")


def main():
    capture = Capture(FILM)
    speech(FILM)
    speech_starts(capture, "english")
    shots = storyboard(capture)
    start_beats = np.concatenate([[0], np.cumsum([shot.beats for shot in shots])])
    assert list(start_beats[:-1]) == plan.CUTS, f"the shots begin on {list(start_beats[:-1])}, the plan says {plan.CUTS}"
    assert start_beats[-1] == plan.LENGTH, f"the shots are {start_beats[-1]} beats, the plan {plan.LENGTH}"
    starts = [round(b * PERIOD * RATE) for b in start_beats]
    total = round((plan.LENGTH * PERIOD + plan.TAIL) * RATE)

    project = FILM / "composition"
    (project / "assets").mkdir(parents=True, exist_ok=True)
    print("edit: the music")
    hits, bed = music.parts()
    score = music.glue(hits + bed)
    write_wav(FILM / "music.wav", score)
    sound, duck = soundtrack(hits, bed, shots, starts, round(total / RATE * SOUND))
    write_wav(project / "assets/sound.wav", sound)
    shutil.copy(project / "assets/sound.wav", FILM / "sound.wav")
    print("edit: the plate")
    plate(capture, shots, starts[:-1], total, project / "assets/plate.mp4")
    data = composition(shots, start_beats, capture, total, project)
    out = FILM / ("draft.mp4" if DRAFT else "heyo.mp4")
    render(project, out)
    if not DRAFT:
        subprocess.run([FFMPEG, "-hide_banner", "-loglevel", "error", "-y", "-i", str(out), "-c:v", "libvpx-vp9", "-crf", "30", "-b:v", "0", "-row-mt", "1",
                        "-cpu-used", "2", "-pix_fmt", "yuv420p", "-c:a", "libopus", "-b:a", "128k", str(FILM / "heyo.webm")], check=True)
    if STILLS:
        stills(out, shots, start_beats, data)

    # How each cut and each sweep meets the beat: the picture it is in, and
    # the nearest note the onset detector finds in the music by itself.
    y = librosa.resample(score.mean(1).astype(np.float32), orig_sr=SOUND, target_sr=22050)

    def nearest_onset(seconds):
        """The note an onset detector finds nearest to that time, looking
        a second either side of it."""
        a = max(0, round((seconds - 1) * 22050))
        found = librosa.onset.onset_detect(y=y[a : a + 2 * 22050], sr=22050, hop_length=64, units="time", backtrack=True) + a / 22050
        return found[np.argmin(np.abs(found - seconds))] - seconds if len(found) else float("nan")

    lines = [f"music: made by music.py, {music.BPM} beats a minute; a beat is {PERIOD * RATE:g} pictures",
             f"the mix: {music.loudness(sound):.1f} LUFS integrated, {music.true_peak(sound):.1f} dBTP; the music alone {music.loudness(score):.1f} LUFS",
             "", "cut  picture  seconds   beat  from the grid  nearest onset  shot"]
    worst = 0.0
    for place, (number, beat) in enumerate(zip(starts, start_beats)):
        off = number / RATE - beat * PERIOD
        onset = nearest_onset(number / RATE)
        worst = max(worst, abs(off))
        name = shots[place].name if place < len(shots) else "(the end)"
        lines.append(f"{place:3d}  {number:7d}  {number / RATE:7.3f}  {beat:5.1f}  {off * 1000:+10.1f} ms  {onset * 1000:+10.1f} ms  {name}")
    lines.append("")
    lines.append(f"the cut furthest from its beat: {worst * 1000:.1f} ms (a picture is {1000 / RATE:.1f} ms)")
    count, median, p90, tempo = music.grid_report(score)
    lines.append(f"the music's onsets: {count}, off the sixteenth-note grid by {median * 1000:+.1f} ms (median), {p90 * 1000:.1f} ms (90th percentile); librosa hears {tempo:.1f} beats a minute")
    lines.append(f"the music is turned down under speech for {duck.mean() * total / RATE:.1f} s of the film")
    (FILM / "cuts.txt").write_text("\n".join(lines) + "\n")
    print("\n".join(lines[:2] + lines[-4:]))
    # What is seen against what is heard, measured in the film as made.
    report = sync.measure(out, [(b * PERIOD, what, where) for b, what, where in plan.seen()], RATE)
    (FILM / "sync.txt").write_text("\n".join(report) + "\n")
    print("\n".join(report[-4:]))
    print(f"edit: {out}, {total / RATE:.1f} s, {out.stat().st_size / 1048576:.1f} MB")


if __name__ == "__main__":
    main()
