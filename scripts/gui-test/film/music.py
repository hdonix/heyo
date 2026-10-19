"""music.py [OUT.wav]: the film's music, made here, note by note.

Nothing is fetched and nothing is sampled: every sound is computed below
(oscillators, filtered noise, envelopes), so the music is the film's own and
its beat grid is exact. 120 beats a minute: a beat is 0.5 s, 24000 samples
at 48 kHz and 12 pictures at 24 a second: every sixteenth is 3 pictures.

It is written from the film's own list of what happens when (plan.py):
a sound for each thing seen to happen (the palette appearing, each letter
typed, each word of a caption punching in, each cut and sweep, each window
going to its place, each letter of the address on the last card), over
grooves that follow the film's sections. As a module: `render()` gives the
stereo music, `loudness()` and `true_peak()` measure, `grid_report()` says
how far the notes found by an onset detector are from the grid.

Run by itself it writes OUT.wav (default music.wav) and prints the numbers.
Needs numpy and scipy (librosa only for the grid report).
"""
import sys
import wave

from pathlib import Path

import numpy as np
from scipy.ndimage import minimum_filter1d
from scipy.signal import butter, fftconvolve, lfilter, sosfilt, sosfilt_zi

sys.path.insert(0, str(Path(__file__).resolve().parent))
import plan  # noqa: E402

RATE = 48000
BPM = plan.BPM
BEAT = plan.BEAT
SPB = round(BEAT * RATE)  # samples a beat
# The music is composed in the beats as written (plan.WRITTEN, each sweep a
# single beat) and every sound is moved to the film's beats as it is put
# down (plan.shift); the section titles held between get their own bars.
W = plan.WRITTEN
SECTIONS = W.SECTIONS
LENGTH = plan.LENGTH
TAIL = plan.TAIL

KEY = 53  # F3
MAIN = ["F", "C", "Dm", "Bb"]
SOFT = ["Dm", "Bb", "F", "C"]
ROOTS = {"F": 41, "C": 36, "Dm": 38, "Bb": 46, "Gm": 43, "Am": 45}


def chord_notes(name):
    """The chord's notes, voiced close around A4/C5 for the pad."""
    root = ROOTS[name] % 12
    third = 3 if name.endswith("m") else 4
    tones = [(root + i) % 12 for i in (0, third, 7)]
    notes = []
    for tone in tones:
        # nearest to the middle of the pad's range
        note = min(range(55, 75), key=lambda n: (n % 12 != tone, abs(n - 64)))
        notes.append(note)
    return sorted(notes) + [sorted(notes)[0] + 12]


def hz(note):
    return 440.0 * 2 ** ((note - 69) / 12)


RNG = np.random.default_rng(20261019)


def noise(n):
    return RNG.standard_normal(n).astype(np.float64)


def t_of(n):
    return np.arange(n) / RATE


def section_of(beat):
    for name, a, b in SECTIONS:
        if a <= beat < b:
            return name
    return "end"


def chord_at(beat):
    name = section_of(beat)
    progression = SOFT if name in ("intro", "breakdown") else MAIN
    if name == "end":
        return "F"
    start = dict((s[0], s[1]) for s in SECTIONS)[name]
    return progression[int((beat - start) // 4) % 4]


# --- Oscillators -------------------------------------------------------------


def blep(phase, dt):
    """PolyBLEP correction for a saw whose phase wraps at 1."""
    out = np.zeros_like(phase)
    a = phase < dt
    x = phase[a] / dt[a]
    out[a] = x + x - x * x - 1
    b = phase > 1 - dt
    x = (phase[b] - 1) / dt[b]
    out[b] = x * x + x + x + 1
    return out


def saw(freq, n, start_phase=0.0):
    freq = np.broadcast_to(np.asarray(freq, float), (n,)).copy()
    dt = freq / RATE
    phase = (start_phase + np.cumsum(dt)) % 1.0
    return 2 * phase - 1 - blep(phase, dt)


def square(freq, n, start_phase=0.0):
    freq = np.broadcast_to(np.asarray(freq, float), (n,)).copy()
    dt = freq / RATE
    phase = (start_phase + np.cumsum(dt)) % 1.0
    other = (phase + 0.5) % 1.0
    return (np.where(phase < 0.5, 1.0, -1.0) + blep(phase, dt) - blep(other, dt))


def sine(freq, n, start_phase=0.0):
    freq = np.broadcast_to(np.asarray(freq, float), (n,))
    return np.sin(2 * np.pi * (start_phase + np.cumsum(freq) / RATE))


def lowpass(x, cutoff, order=2):
    return sosfilt(butter(order, min(cutoff, RATE * 0.45), "low", fs=RATE, output="sos"), x)


def highpass(x, cutoff, order=2):
    return sosfilt(butter(order, cutoff, "high", fs=RATE, output="sos"), x)


def bandpass(x, low, high, order=2):
    return sosfilt(butter(order, [low, min(high, RATE * 0.45)], "band", fs=RATE, output="sos"), x)


def sweep_filter(x, cutoffs, kind="low", block=512):
    """A filter whose cutoff moves: `cutoffs` is one value a sample, used a
    block at a time with the filter's state carried over."""
    out = np.zeros_like(x)
    zi = None
    for i in range(0, len(x), block):
        c = float(np.clip(cutoffs[min(i + block // 2, len(cutoffs) - 1)], 40, RATE * 0.45))
        sos = butter(2, c, kind, fs=RATE, output="sos")
        if zi is None:
            zi = sosfilt_zi(sos) * 0
        out[i : i + block], zi = sosfilt(sos, x[i : i + block], zi=zi)
    return out


def adsr(n, attack, decay, sustain, release, held):
    """An envelope n samples long: held for `held` seconds, then released."""
    t = t_of(n)
    env = np.where(t < attack, t / max(attack, 1e-6), sustain + (1 - sustain) * np.exp(-(t - attack) / max(decay, 1e-6)))
    rel = t > held
    env[rel] *= np.exp(-(t[rel] - held) / max(release, 1e-6))
    return env


# --- Instruments -------------------------------------------------------------


def kick(gain=1.0):
    n = int(0.5 * RATE)
    t = t_of(n)
    freq = 56 + 150 * np.exp(-t / 0.022)
    body = sine(freq, n) * np.exp(-t / 0.13)
    click = highpass(noise(n), 3000) * np.exp(-t / 0.0018) * 0.9
    knock = sine(1200 * np.exp(-t / 0.004) + 200, n) * np.exp(-t / 0.006) * 0.5
    return np.tanh(2.4 * (body + click + knock)) * 0.95 * gain


def clap(gain=1.0):
    n = int(0.4 * RATE)
    t = t_of(n)
    source = bandpass(noise(n), 900, 4200)
    env = np.zeros(n)
    for k, at in enumerate((0, 0.007, 0.014)):
        s = t >= at
        env[s] += np.exp(-(t[s] - at) / 0.004) * (1.0 if k == 0 else 0.7)
    s = t >= 0.02
    env[s] += 0.6 * np.exp(-(t[s] - 0.02) / 0.08)
    body = sine(210 + 60 * np.exp(-t / 0.008), n) * np.exp(-t / 0.03) * 0.5
    return np.tanh(1.8 * (source * env + body)) * 0.75 * gain


def snare(gain=1.0):
    n = int(0.3 * RATE)
    t = t_of(n)
    tone = sine(190 + 60 * np.exp(-t / 0.008), n) * np.exp(-t / 0.045)
    rattle = bandpass(noise(n), 1500, 10000) * np.exp(-t / 0.07)
    snap = highpass(noise(n), 4000) * np.exp(-t / 0.003) * 0.8
    return np.tanh(1.6 * (0.7 * tone + 0.6 * rattle + snap)) * gain


def tick(note, gain=1.0):
    """A bright, short pitched click: a letter typed."""
    n = int(0.25 * RATE)
    t = t_of(n)
    f = hz(note)
    body = (sine(f, n) + 0.5 * sine(2 * f, n) + 0.25 * square(f * 3.01, n)) * np.exp(-t / 0.05)
    click = highpass(noise(n), 5000) * np.exp(-t / 0.0015)
    return (0.5 * body + 0.6 * click) * gain


def bell(note, gain=1.0):
    """Two sines, one modulating the other: a struck, glassy note."""
    n = int(1.6 * RATE)
    t = t_of(n)
    f = hz(note)
    mod = np.sin(2 * np.pi * f * 3.5 * t) * 2.2 * np.exp(-t / 0.25)
    body = np.sin(2 * np.pi * f * t + mod) * np.exp(-t / 0.5)
    strike = highpass(noise(n), 3000) * np.exp(-t / 0.002) * 0.4
    return (0.45 * body + strike) * gain


def tom(note=45, gain=1.0):
    n = int(0.6 * RATE)
    t = t_of(n)
    body = sine(hz(note) * (1 + 0.8 * np.exp(-t / 0.02)), n) * np.exp(-t / 0.22)
    click = highpass(noise(n), 2500) * np.exp(-t / 0.002) * 0.5
    return np.tanh(1.8 * (body + click)) * 0.8 * gain


def rim(gain=1.0):
    """A woodblock-like knock: a word of the title."""
    n = int(0.15 * RATE)
    t = t_of(n)
    body = sine(1650, n) * np.exp(-t / 0.018) + 0.5 * sine(820, n) * np.exp(-t / 0.03)
    click = highpass(noise(n), 4000) * np.exp(-t / 0.0015)
    return (0.6 * body + 0.5 * click) * gain


def swell(seconds, gain=1.0):
    """A reversed cymbal: noise that grows into the beat it ends on."""
    n = int(seconds * RATE)
    x = np.linspace(0, 1, n)
    return highpass(noise(n), 3000) * x ** 3 * 0.35 * gain


def hat(open_=False, gain=1.0):
    n = int((0.35 if open_ else 0.08) * RATE)
    t = t_of(n)
    metal = sum(square(f, n) for f in (3150, 4350, 5520, 6780, 8020)) / 5
    body = highpass(0.5 * noise(n) + metal, 7200, order=4)
    return body * np.exp(-t / (0.11 if open_ else 0.022)) * 0.5 * gain


def crash(gain=1.0):
    n = int(2.6 * RATE)
    t = t_of(n)
    body = highpass(noise(n), 4200, order=2) + 0.3 * bandpass(noise(n), 2000, 6000)
    return body * np.exp(-t / 0.6) * (1 - np.exp(-t / 0.0008)) * 0.3 * gain


def impact(gain=1.0):
    """A deep hit with a burst on top: the cold open and the last card."""
    n = int(2.5 * RATE)
    t = t_of(n)
    boom = sine(45 + 90 * np.exp(-t / 0.06), n) * np.exp(-t / 0.35) * 0.6
    burst = lowpass(noise(n), 2500) * np.exp(-t / 0.08) * 0.6
    burst += highpass(noise(n), 3000) * np.exp(-t / 0.003) * 0.6
    return np.tanh(1.4 * (boom + burst)) * 0.85 * gain


def riser(seconds, gain=1.0):
    n = int(seconds * RATE)
    t = t_of(n) / seconds
    cut = 300 * (9000 / 300) ** (t ** 1.6)
    body = sweep_filter(noise(n), cut, "low") * (t ** 2.2)
    tone = saw(220 * 2 ** (2 * t ** 1.3), n) * 0.12 * t ** 2
    out = (body * 0.45 + lowpass(tone, 4000)) * gain
    # silent for the last sixteenth: the hit it leads to is heard alone
    gap = int(BEAT / 4 * RATE)
    out[-gap:] = 0
    out[-gap - 480 : -gap] *= np.linspace(1, 0, 480)
    return out


def whoosh(seconds=0.6, gain=1.0):
    """Noise swept up then down: under a sweep of colour across the film."""
    n = int(seconds * RATE)
    x = np.linspace(0, 1, n)
    shape = np.sin(np.pi * x) ** 2
    cut = 500 + 7000 * np.sin(np.pi * np.clip(x * 1.1, 0, 1)) ** 2
    return sweep_filter(noise(n), cut, "low") * shape * 0.35 * gain


def supersaw(notes, seconds, held, cutoff, detune=(-14, -7, 0, 7, 14)):
    """A chord of detuned saws, spread across the two sides."""
    n = int(seconds * RATE)
    left = np.zeros(n)
    right = np.zeros(n)
    for note in notes:
        for k, cents in enumerate(detune):
            wave_ = saw(hz(note) * 2 ** (cents / 1200), n, RNG.random())
            pan = (k / (len(detune) - 1)) if len(detune) > 1 else 0.5
            left += wave_ * np.sqrt(1 - pan)
            right += wave_ * np.sqrt(pan)
    env = adsr(n, 0.04, 0.6, 0.75, 0.35, held)
    scale = 1 / (len(notes) * len(detune)) ** 0.5
    if np.ndim(cutoff):
        return np.stack([sweep_filter(left, cutoff), sweep_filter(right, cutoff)], 1) * env[:, None] * scale
    return np.stack([lowpass(left, cutoff), lowpass(right, cutoff)], 1) * env[:, None] * scale


def bass(note, seconds, held, bright=900):
    n = int(seconds * RATE)
    f = hz(note)
    body = 0.6 * saw(f, n) + 0.4 * square(f * 1.003, n)
    t = t_of(n)
    body = lowpass(body, bright) * 0.8 + 0.4 * lowpass(body, bright * 3) * np.exp(-t / 0.04)
    sub = sine(f / 2, n) * 0.4
    return (body + sub) * adsr(n, 0.004, 0.2, 0.8, 0.05, held) * 0.55


def pluck(note, seconds, cutoff=5000):
    n = int(seconds * RATE)
    t = t_of(n)
    raw = saw(hz(note), n) + 0.5 * square(hz(note) * 2.002, n)
    bright = lowpass(raw, cutoff)
    dark = lowpass(raw, 700)
    k = np.exp(-t / 0.06)
    return (bright * k + dark * (1 - k)) * np.exp(-t / 0.22) * 0.35


def lead(note, seconds, held):
    n = int(seconds * RATE)
    t = t_of(n)
    vib = 1 + 0.004 * np.sin(2 * np.pi * 5.5 * t) * np.clip((t - 0.15) / 0.2, 0, 1)
    f = hz(note) * vib
    body = 0.5 * saw(f * 1.004, n) + 0.5 * saw(f * 0.996, n) + 0.3 * square(f / 2, n)
    body = lowpass(body, 3800)
    return body * adsr(n, 0.008, 0.25, 0.7, 0.12, held) * 0.32


# --- The tune ----------------------------------------------------------------

# The hook, a bar on each of the main chords: (beat, beats, note).
HOOK = [
    [(0, 0.5, 72), (0.5, 0.5, 69), (1, 0.5, 72), (1.5, 1, 77), (2.5, 0.5, 76), (3, 1, 72)],
    [(0, 0.5, 76), (0.5, 0.5, 74), (1, 1, 72), (2, 0.5, 67), (2.5, 0.5, 72), (3, 1, 74)],
    [(0, 0.5, 74), (0.5, 0.5, 72), (1, 0.5, 74), (1.5, 1, 77), (2.5, 0.5, 76), (3, 1, 74)],
    [(0, 1, 74), (1, 0.5, 72), (1.5, 0.5, 70), (2, 1, 69), (3, 1, 67)],
]


class Track:
    def __init__(self, seconds):
        self.n = int(seconds * RATE)
        self.buses = {name: np.zeros((self.n, 2)) for name in ("drums", "kick", "music", "lead", "fx", "send")}
        self.kicks = []

    def add(self, bus, sound, beat, gain=1.0, pan=0.0, send=0.0, held=False):
        """`sound` at `beat` as written, or at that beat of the film itself
        when `held` (in the bars of a section title)."""
        at = round((beat if held else plan.shift(beat)) * SPB)
        if at >= self.n:
            return
        if sound.ndim == 1:
            sound = np.stack([sound * np.sqrt(1 - (pan + 1) / 2) * np.sqrt(2), sound * np.sqrt((pan + 1) / 2) * np.sqrt(2)], 1) / np.sqrt(2)
        sound = sound[: self.n - at] * gain
        self.buses[bus][at : at + len(sound)] += sound
        if send:
            self.buses["send"][at : at + len(sound)] += sound * send


def compose():
    track = Track(LENGTH * BEAT + TAIL + 2.0)
    K, CL, SN = kick(), clap(), snare()
    HC, HO = hat(), hat(True)
    O, E = W.OPEN, W.END

    def hit_kick(beat, gain=1.0, held=False):
        track.add("kick", K, beat, gain, held=held)
        track.kicks.append(beat if held else plan.shift(beat))

    def drums_bar(bar_beat, full=True, claps=(1, 3), hats=True, kick_on=True):
        for b in range(4):
            if kick_on:
                hit_kick(bar_beat + b)
            if b in claps:
                track.add("drums", CL, bar_beat + b, 0.9, send=0.12)
            if hats:
                track.add("drums", HO if full else HC, bar_beat + b + 0.5, 0.35 if full else 0.5, pan=0.15)
                if full:
                    for s in (0.25, 0.75):
                        track.add("drums", HC, bar_beat + b + s, 0.25 + 0.07 * (s == 0.75), pan=-0.2)

    def bass_bar(bar_beat, pattern="pump"):
        root = ROOTS[chord_at(bar_beat)]
        for b in range(4):
            if pattern == "pump":
                track.add("music", bass(root, 0.3, 0.2), bar_beat + b + 0.5, 0.9)
                track.add("music", bass(root, 0.3, 0.16, 600), bar_beat + b + 0.75, 0.55)
            elif b == 0:
                track.add("music", bass(root, 2.2, 1.9, 500), bar_beat, 0.8)

    def pad_bar(bar_beat, gain=1.0, cutoff=3200, beats=4):
        notes = chord_notes(chord_at(bar_beat))
        track.add("music", supersaw(notes, beats * BEAT + 0.6, beats * BEAT - 0.05, cutoff), bar_beat, 0.38 * gain, send=0.2)

    def arp_bar(bar_beat, gain=1.0, cutoff=5000):
        notes = chord_notes(chord_at(bar_beat))
        order = [0, 1, 2, 3, 2, 1, 2, 3] * 2
        for k in range(16):
            track.add("music", pluck(notes[order[k]] + 12, 0.4, cutoff), bar_beat + k / 4, 0.3 * gain, pan=(-0.35 if k % 2 else 0.35), send=0.18)

    def hook_bars(start, end, octave=0, gain=1.0):
        for bar_beat in range(start, end, 4):
            bar = HOOK[int((bar_beat - start) // 4) % 4]
            for beat, length, note in bar:
                track.add("lead", lead(note + octave, length * BEAT + 0.4, length * BEAT * 0.9), bar_beat + beat, 0.8 * gain, send=0.2)

    def roll(start, beats, gain=1.0):
        """A snare roll: eighths, then sixteenths, growing."""
        half = start + beats / 2
        steps = sorted(set([start + k / 2 for k in range(int(beats))] + [half + k / 4 for k in range(int(beats * 2))]))
        for i, at in enumerate(steps):
            if at >= start + beats - 0.5:
                break
            track.add("drums", SN, at, gain * (0.3 + 0.6 * i / max(1, len(steps))), send=0.1)

    def stab(beat, name, gain=1.0, cutoff=5000):
        notes = chord_notes(name)
        track.add("music", supersaw(notes + [notes[0] + 12], 0.5, 0.1, cutoff), beat, 0.55 * gain, send=0.3)

    # --- The opening, a sound for each thing that happens ---------------------
    track.add("fx", swell(O["palette"] * BEAT), 0, 0.9)
    track.add("fx", impact(), O["palette"], 0.9)
    track.add("drums", crash(), O["palette"], 1.0, send=0.2)
    hit_kick(O["palette"])
    stab(O["palette"], "F", 0.8)
    for beat, note in zip(O["letters"], (77, 81, 84, 89)):
        track.add("fx", tick(note), beat, 1.6, pan=0.1)
        track.add("fx", rim(), beat, 0.7)
    track.add("drums", SN, O["enter"], 1.0, send=0.15)
    track.add("drums", CL, O["enter"], 0.6)
    hit_kick(O["editor"])
    track.add("drums", crash(0.6), O["editor"], 0.7)
    stab(O["editor"], "C", 0.9)
    track.add("music", bass(ROOTS["C"], 1.0, 0.8, 600), O["editor"], 0.8)
    track.add("music", supersaw(chord_notes("Dm"), 4 * BEAT + 0.6, 4 * BEAT, np.linspace(600, 2400, int((4 * BEAT + 0.6) * RATE))), O["editor"], 0.3)
    track.add("drums", tom(43), O["move"], 1.0, send=0.1)
    hit_kick(O["move"], 0.8)
    track.add("fx", riser((O["title"] - O["move"]) * BEAT), O["move"], 0.6)
    # The title.
    track.add("fx", impact(), O["title"], 1.0)
    track.add("drums", crash(), O["title"], 1.0, send=0.2)
    for k in range(int(O["title"]), int(O["throw"]) + 1):
        if k < O["throw"]:
            hit_kick(k, 0.9)
    pad_bar(8, 0.7, 1800)
    pad_bar(12, 0.8, np.linspace(1800, 5000, int((4 * BEAT + 0.6) * RATE)))
    track.add("fx", bell(84), O["icon"], 1.0, send=0.3)
    track.add("drums", SN, O["icon"], 1.4)
    track.add("drums", crash(0.5), O["icon"], 0.8)
    track.add("drums", CL, O["icon"], 0.9)
    for beat, note in zip(O["name"], (81, 84, 86, 89)):
        track.add("fx", tick(note), beat, 2.0)
        track.add("fx", rim(), beat, 1.0)
        track.add("drums", SN, beat, 0.6 if beat < O["name"][-1] else 1.1)
        track.add("music", pluck(note, 0.5, 7000), beat, 0.5, send=0.2)
    for beat in O["sub"]:
        track.add("fx", rim(), beat, 1.0, pan=-0.1)
        track.add("drums", SN, beat, 0.45)
    for beat, note in zip(O["pills"], (77, 81, 84, 89, 93, 96)):
        track.add("fx", tick(note), beat, 1.4)
        track.add("music", pluck(note, 0.4, 8000), beat, 0.5, send=0.2)
    track.add("fx", riser((O["throw"] - 12) * BEAT), 12, 0.7)
    track.add("drums", SN, O["throw"], 1.6, send=0.2)
    track.add("drums", CL, O["throw"], 1.0)
    track.add("fx", impact(0.6), O["throw"], 0.6)

    # --- The scenes ---------------------------------------------------------
    for name, a, b in SECTIONS[1:-1]:
        for bar_beat in range(a, b, 4):
            in_section = bar_beat - a
            if name in ("drop", "groove", "final"):
                drums_bar(bar_beat)
                bass_bar(bar_beat)
                pad_bar(bar_beat, 0.8)
                arp_bar(bar_beat, 0.75)
                if name == "final":
                    hook_bars(bar_beat, bar_beat + 4, 0, 1.0)
                if name == "drop" and in_section >= 16:
                    hook_bars(bar_beat, bar_beat + 4, -12, 0.55)
            elif name == "lift":
                drums_bar(bar_beat, claps=(0, 1, 2, 3) if bar_beat >= 52 else (1, 3))
                bass_bar(bar_beat)
                pad_bar(bar_beat, 0.85, 4200)
                arp_bar(bar_beat, 0.8, 6500)
                hook_bars(bar_beat, bar_beat + 4, 12 if bar_beat >= 52 else 0, 0.85)
            elif name == "breakdown":
                # Clipboard and calculator: lighter, then the build back.
                pad_bar(bar_beat, 0.9, 2400)
                arp_bar(bar_beat, 0.6, 3500)
                for k in range(4):
                    track.add("drums", HC, bar_beat + k + 0.5, 0.35, pan=0.2)
                    if bar_beat < 88 and k in (0, 2):
                        hit_kick(bar_beat + k, 0.8)
                if bar_beat >= 84:
                    bass_bar(bar_beat, "long")
                if bar_beat >= 88:
                    for k in range(4):
                        hit_kick(bar_beat + k, 0.85 if bar_beat < 92 else 0.95)
                if bar_beat == 88:
                    track.add("fx", riser(8 * BEAT), 88, 0.6)
                if bar_beat == 92:
                    roll(92, 3.5, 0.8)
    # A hit on every cut: an impact and a crash under a sweep or a new
    # section, a crash and a kick on the others.
    starts = {a for _, a, _ in SECTIONS}
    # (the breakdown is quieter, and what is written for its pictures is
    # made louder to stand out as much as elsewhere)
    lift = lambda beat: 1.4 if section_of(beat) == "breakdown" else 1.0  # noqa: E731
    for beat in W.CUTS[2:]:
        if beat in W.WIPES or beat in starts:
            track.add("fx", impact(), beat, 0.75)
            track.add("drums", crash(), beat, 1.0, send=0.15)
        else:
            track.add("drums", crash(), beat, 0.9 * lift(beat))
            track.add("drums", CL, beat, 0.8 * lift(beat))
        if plan.shift(beat) not in track.kicks:
            hit_kick(beat)
    for beat in W.WIPES:
        track.add("fx", whoosh(0.6), beat - (0.6 + BEAT / 4) / BEAT, 0.6)
    # A clap under each word of a caption as it punches in; a heavier hit
    # under the word in blue.
    for caption in W.CAPTIONS:
        for word, beat in caption["words"]:
            if word in caption["accent"]:
                track.add("drums", SN, beat, 1.6 * lift(beat), send=0.1)
                track.add("drums", CL, beat, 1.0 * lift(beat))
                track.add("drums", crash(0.5), beat, 0.5)
            else:
                track.add("drums", CL, beat, 1.4 * lift(beat), pan=0.1)
                track.add("drums", SN, beat, 0.6 * lift(beat))
                track.add("fx", rim(), beat, 0.8)

    # --- The last card --------------------------------------------------------
    track.add("fx", impact(1.2), E["slam"], 1.0)
    track.add("drums", crash(), E["slam"], 1.1, send=0.25)
    hit_kick(E["slam"], 1.1)
    stab(E["slam"], "F", 1.0)
    track.add("music", bass(ROOTS["F"], 2.0, 1.6, 500), E["slam"], 0.8)
    track.add("drums", CL, E["line"], 1.0, send=0.2)
    hit_kick(E["line"], 0.9)
    stab(E["line"], "Bb", 0.9)
    track.add("drums", crash(0.6), E["line"], 0.6)
    track.add("drums", SN, E["url"], 1.0, send=0.15)
    hit_kick(E["url"], 0.8)
    stab(E["url"], "C", 0.8)
    track.add("music", supersaw(chord_notes("C"), 8 * BEAT, 8 * BEAT - 0.3, 1400), E["url"], 0.22)
    scale = (77, 79, 81, 84, 86, 89, 91, 93)
    for k, beat in enumerate(E["letters"]):
        track.add("fx", tick(scale[(k * 3) % len(scale)]), beat, 1.6, pan=(-0.2 if k % 2 else 0.2))
    # The caret blinks on the hats: off on the open hat, on on the closed.
    for beat in E["blinks"]:
        track.add("drums", HO if beat % 1 else HC, beat, 1.2, pan=0.15)
        track.add("fx", rim(), beat, 0.5)
    hit_kick(E["chord"], 1.1)
    track.add("fx", impact(), E["chord"], 0.8)
    track.add("drums", crash(), E["chord"], 1.0, send=0.5)
    notes = chord_notes("F")
    ring = (W.LENGTH - E["chord"]) * BEAT + TAIL
    track.add("music", supersaw(notes + [notes[0] + 12, notes[0] + 24], ring, (E["fade"][0] - E["chord"]) * BEAT, 3000), E["chord"], 0.75, send=0.6)
    track.add("music", bass(ROOTS["F"], ring, ring - 1.5, 400), E["chord"], 0.8)
    for k, note in enumerate([72, 77, 81, 84]):
        track.add("lead", lead(note, 2.4, 0.5), E["chord"] + k * 0.5, 0.3, send=0.6)

    # --- The section titles, held -------------------------------------------
    # Struck as the title lands; a stab of the next section's first chord on
    # each beat, claps, hats growing to sixteenths, a snare fill and a riser
    # into the scene, which comes in on the sweep's hit as before.
    for (start, end, _), wipe in zip(plan.TITLES, sorted(W.WIPES)):
        name = chord_at(wipe)
        notes = chord_notes(name)
        track.add("fx", whoosh(0.6), start - (0.6 + BEAT / 4) / BEAT, 0.6, held=True)
        track.add("fx", impact(), start, 0.9, held=True)
        track.add("drums", crash(), start, 1.0, send=0.2, held=True)
        track.add("music", supersaw(notes + [notes[0] + 12], 0.5, 0.1, 5000), start, 0.6, send=0.3, held=True)
        track.add("music", bass(ROOTS[name], 0.45, 0.35, 700), start, 0.9, held=True)
        for k in range(1, plan.HOLD):
            hit_kick(start + k, 0.9, held=True)
            track.add("music", supersaw(notes + [notes[0] + 12], 0.35, 0.08, 2000 + 1200 * k), start + k, 0.4, send=0.2, held=True)
            track.add("music", bass(ROOTS[name], 0.3, 0.2, 600), start + k + 0.5, 0.7, held=True)
            if k % 2:
                track.add("drums", CL, start + k, 0.9, held=True)
        for k in range(4 * (plan.HOLD - 1)):
            at = start + 0.5 + k / 4
            if at >= end - 1:
                break
            track.add("drums", HC, at, 0.25 + 0.3 * k / (4 * plan.HOLD), pan=0.15, held=True)
        for k in range(4):
            track.add("drums", SN, end - 1 + k / 4, 0.4 + 0.2 * k, send=0.1, held=True)
        track.add("fx", riser((plan.HOLD - 1) * BEAT), start + 1, 0.5, held=True)
    return track


# --- Mixing ------------------------------------------------------------------


def reverb_ir(seconds=2.2):
    n = int(seconds * RATE)
    t = t_of(n)
    ir = np.stack([noise(n), noise(n)], 1) * np.exp(-t / (seconds / 6.9))[:, None]
    ir[: int(0.012 * RATE)] = 0  # pre-delay
    ir = np.stack([lowpass(ir[:, 0], 6500), lowpass(ir[:, 1], 6500)], 1)
    return ir / np.sqrt((ir ** 2).sum(0))


def delay(x, beats=0.75, feedback=0.35, mix=0.25):
    d = round(beats * SPB)
    out = x.copy()
    echo = x.copy()
    for k in range(1, 6):
        echo = np.roll(echo, d, axis=0)
        echo[:d] = 0
        echo = echo * feedback
        # ping-pong
        echo = echo[:, ::-1]
        out += echo * mix / feedback
    return out


def sidechain(n, kicks, depth=0.8, release=0.14):
    env = np.ones(n)
    t = t_of(int(release * 5 * RATE))
    dip = 1 - depth * np.exp(-t / release) * np.clip(t / 0.004, 0, 1) ** 0.5
    for beat in kicks:
        at = round(beat * SPB)
        seg = dip[: max(0, n - at)]
        env[at : at + len(seg)] = np.minimum(env[at : at + len(seg)], seg)
    return env


def limiter(x, ceiling=10 ** (-1.3 / 20), look=0.005, release=0.08):
    from scipy.signal import resample_poly

    # the peaks between the samples too (four times over), as true peak
    # meters see them
    over = np.abs(resample_poly(x, 4, 1, axis=0)).max(1)
    peak = over[: len(x) * 4].reshape(-1, 4).max(1)
    need = np.minimum(1.0, ceiling / np.maximum(peak, 1e-9))
    w = round(look * RATE)
    held = minimum_filter1d(need, 2 * w + 1)
    a = np.exp(-1 / (release * RATE))
    smooth = lfilter([1 - a], [1, -a], held - 1) + 1
    gain = np.minimum(held, smooth)
    # delay the signal by the look-ahead so the gain arrives first
    shifted = np.roll(x, w, axis=0)
    shifted[:w] = 0
    gain = np.roll(minimum_filter1d(gain, 2 * w + 1), w)
    return shifted * gain[:, None], w


def k_weight(x):
    b1, a1 = [1.53512485958697, -2.69169618940638, 1.19839281085285], [1.0, -1.69065929318241, 0.73248077421585]
    b2, a2 = [1.0, -2.0, 1.0], [1.0, -1.99004745483398, 0.99007225036621]
    return lfilter(b2, a2, lfilter(b1, a1, x, axis=0), axis=0)


def loudness(x):
    """Integrated loudness, LUFS (ITU-R BS.1770-4, gated), of 48 kHz stereo."""
    y = k_weight(x)
    block, step = round(0.4 * RATE), round(0.1 * RATE)
    power = np.array([(y[i : i + block] ** 2).mean(0).sum() for i in range(0, len(y) - block, step)])
    lufs = -0.691 + 10 * np.log10(np.maximum(power, 1e-12))
    gated = power[lufs > -70]
    relative = -0.691 + 10 * np.log10(gated.mean()) - 10
    gated = power[(lufs > -70) & (lufs > relative)]
    return -0.691 + 10 * np.log10(gated.mean())


def short_term(x, seconds=3.0):
    y = k_weight(x)
    block, step = round(seconds * RATE), round(0.5 * RATE)
    power = np.array([(y[i : i + block] ** 2).mean(0).sum() for i in range(0, len(y) - block, step)])
    return -0.691 + 10 * np.log10(np.maximum(power, 1e-12))


def true_peak(x):
    from scipy.signal import resample_poly

    return 20 * np.log10(np.abs(resample_poly(x, 4, 1, axis=0)).max())


def master(x, target=-14.0):
    """To the target loudness, peaks held under -1 dBTP."""
    for _ in range(4):
        x = x * 10 ** ((target - loudness(x)) / 20)
        x, w = limiter(x)
        x = np.concatenate([x[w:], np.zeros((w, 2))])
        if abs(loudness(x) - target) < 0.1 and true_peak(x) <= -1.0:
            break
    while true_peak(x) > -1.0:
        x *= 10 ** ((-1.05 - true_peak(x)) / 20)
    return x


def parts():
    """The music in two parts, as they leave the mixer: the hits (drums and
    the sounds written for what is seen) and the bed (bass, chords, the
    arpeggio, the hook, the reverb). The edit turns the bed down further
    than the hits under speech, so that a cut or a word still lands."""
    track = compose()
    n = track.n
    duck = sidechain(n, track.kicks)[:, None]
    ir = reverb_ir()
    send = track.buses["send"] + 0.0
    wet = np.stack([fftconvolve(send[:, 0], ir[:, 0])[:n], fftconvolve(send[:, 1], ir[:, 1])[:n]], 1)
    lead_bus = delay(track.buses["lead"], 0.75, 0.38, 0.3)
    hits = track.buses["kick"] * 1.0 + track.buses["drums"] * 1.1 + track.buses["fx"] * 0.8
    bed = track.buses["music"] * duck * 0.55 + lead_bus * duck * 0.5 + wet * duck * 0.25
    end = round((LENGTH * BEAT + TAIL) * RATE)
    # the last chord and its reverb, faded with the picture
    fade = round((LENGTH + TAIL / BEAT - plan.END["fade"][0]) * BEAT * RATE)
    out = []
    for part in (hits, bed):
        part = np.stack([highpass(part[:, 0], 35), highpass(part[:, 1], 35)], 1)[:end]
        part[-fade:] *= np.linspace(1, 0, fade)[:, None] ** 1.5
        out.append(part)
    return out


def glue(mix):
    """A soft clip on the bus, then to the film's loudness."""
    return master(np.tanh(mix * 0.9) / 0.9)


def render():
    hits, bed = parts()
    return glue(hits + bed)


def write(path, x):
    with wave.open(str(path), "wb") as out:
        out.setnchannels(2)
        out.setsampwidth(2)
        out.setframerate(RATE)
        out.writeframes((np.clip(x, -1, 1) * 32767).astype(np.int16).tobytes())


def bands(x):
    """Share of the energy in each band, in dB relative to the whole."""
    mono = x.mean(1)
    spec = np.abs(np.fft.rfft(mono)) ** 2
    f = np.fft.rfftfreq(len(mono), 1 / RATE)
    total = spec.sum()
    out = []
    for lo, hi in ((20, 60), (60, 250), (250, 2000), (2000, 6000), (6000, 20000)):
        out.append((f"{lo}-{hi} Hz", 10 * np.log10(spec[(f >= lo) & (f < hi)].sum() / total)))
    return out


def grid_report(x):
    """Onsets found by librosa against the 16th-note grid."""
    import librosa

    mono = x.mean(1).astype(np.float32)
    sr = 22050
    y = librosa.resample(mono, orig_sr=RATE, target_sr=sr)
    # Backtracked: each onset taken where its rise begins, not at the peak
    # of the detector's curve (which comes some 13 ms after the note).
    onsets = librosa.onset.onset_detect(y=y, sr=sr, hop_length=64, units="time", backtrack=True)
    sixteenth = BEAT / 4
    off = (onsets + sixteenth / 2) % sixteenth - sixteenth / 2
    tempo = librosa.beat.beat_track(y=y, sr=sr, hop_length=128, start_bpm=BPM)[0]
    return len(onsets), float(np.median(off)), float(np.percentile(np.abs(off), 90)), float(np.atleast_1d(tempo)[0])


def describe(x):
    lines = [
        f"tempo {BPM} beats a minute, {LENGTH} beats ({LENGTH * BEAT:.1f} s) and {TAIL:.1f} s to ring out",
        f"integrated loudness {loudness(x):.2f} LUFS, true peak {true_peak(x):.2f} dBTP, sample peak {20 * np.log10(np.abs(x).max()):.2f} dBFS",
    ]
    st = short_term(x)
    lines.append(f"short-term loudness (3 s): {st.min():.1f} to {st.max():.1f} LUFS")
    for name, a, b in plan.SECTIONS:
        part = x[round(a * BEAT * RATE) : round(b * BEAT * RATE)]
        lines.append(f"  {name:10s} beats {a:3d}-{b:3d}: {loudness(part):6.1f} LUFS")
    lines.append("spectrum: " + ", ".join(f"{name} {db:.1f} dB" for name, db in bands(x)))
    return lines


if __name__ == "__main__":
    out = sys.argv[1] if len(sys.argv) > 1 else "music.wav"
    music = render()
    write(out, music)
    for line in describe(music):
        print(line)
    count, median, p90, tempo = grid_report(music)
    print(f"onsets found: {count}; off the 16th grid by {median * 1000:+.1f} ms (median, signed), {p90 * 1000:.1f} ms (90th percentile, either way); librosa's tempo {tempo:.1f}")
