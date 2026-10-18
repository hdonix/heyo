"""music.py [OUT.wav]: the film's music, made here, note by note.

Nothing is fetched and nothing is sampled: every sound is computed below
(oscillators, filtered noise, envelopes), so the music is the film's own and
its beat grid is exact. 120 beats a minute: a beat is 0.5 s, 24000 samples
at 48 kHz and 15 pictures at 30 a second, so every beat falls on a picture.

The film is cut to SECTIONS (in beats) and WIPES (the beats on which a
scene gives way to the next, under a sweep of colour); the edit reads both
from here. As a module: `render()` gives the stereo music, `loudness()` and
`true_peak()` measure, `grid_report()` says how far the notes found by an
onset detector are from the grid.

Run by itself it writes OUT.wav (default music.wav) and prints the numbers.
Needs numpy and scipy (librosa only for the grid report).
"""
import sys
import wave

import numpy as np
from scipy.ndimage import minimum_filter1d
from scipy.signal import butter, fftconvolve, lfilter, sosfilt, sosfilt_zi

RATE = 48000
BPM = 120
BEAT = 60 / BPM
SPB = round(BEAT * RATE)  # samples a beat

# What the film shows, in beats. The music follows it: a build under the
# opening and the title, a drop where dictation begins, a lift for the
# windows that move on every beat, a breakdown, the last drop, a final hit
# on the end card.
SECTIONS = [
    ("intro", 0, 16),      # cold open (palette on beat 0), title from beat 8
    ("drop", 16, 48),      # dictation
    ("lift", 48, 60),      # windows: one layout a beat from 52
    ("groove", 60, 76),    # screenshot
    ("breakdown", 76, 96), # clipboard, calculator
    ("final", 96, 112),    # extensions
    ("end", 112, 128),     # end card: the final hit on 112
]
LENGTH = SECTIONS[-1][2]
TAIL = 1.0  # seconds after the last beat, for the sound to die
WIPES = [48, 60, 76, 88, 96]
SNAPS = list(range(52, 59))  # the beats the windows go to their places on

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
    freq = 52 + 120 * np.exp(-t / 0.03)
    body = sine(freq, n) * np.exp(-t / 0.2)
    click = highpass(noise(n), 2500) * np.exp(-t / 0.0025) * 0.35
    return np.tanh(1.6 * (body + click)) * 0.9 * gain


def clap(gain=1.0):
    n = int(0.45 * RATE)
    t = t_of(n)
    source = bandpass(noise(n), 900, 2600)
    env = np.zeros(n)
    for k, at in enumerate((0, 0.009, 0.018)):
        s = t >= at
        env[s] += np.exp(-(t[s] - at) / 0.005) * (0.8 if k < 2 else 1.0)
    s = t >= 0.024
    env[s] += 0.75 * np.exp(-(t[s] - 0.024) / 0.11)
    return source * env * 0.55 * gain


def snare(gain=1.0):
    n = int(0.3 * RATE)
    t = t_of(n)
    tone = sine(185 + 40 * np.exp(-t / 0.01), n) * np.exp(-t / 0.05)
    rattle = bandpass(noise(n), 1500, 9000) * np.exp(-t / 0.075)
    return (0.6 * tone + 0.55 * rattle) * gain


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
    return body * np.exp(-t / 0.75) * (1 - np.exp(-t / 0.002)) * 0.22 * gain


def impact(gain=1.0):
    """A deep hit with a burst on top: the cold open and the last card."""
    n = int(2.5 * RATE)
    t = t_of(n)
    boom = sine(40 + 60 * np.exp(-t / 0.08), n) * np.exp(-t / 0.45) * 0.7
    burst = lowpass(noise(n), 1800) * np.exp(-t / 0.12) * 0.5
    return np.tanh(1.4 * (boom + burst)) * 0.85 * gain


def riser(seconds, gain=1.0):
    n = int(seconds * RATE)
    t = t_of(n) / seconds
    cut = 300 * (9000 / 300) ** (t ** 1.6)
    body = sweep_filter(noise(n), cut, "low") * (t ** 2.2)
    tone = saw(220 * 2 ** (2 * t ** 1.3), n) * 0.12 * t ** 2
    return (body * 0.45 + lowpass(tone, 4000)) * gain


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

    def add(self, bus, sound, beat, gain=1.0, pan=0.0, send=0.0):
        at = round(beat * SPB)
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

    def drums_bar(bar_beat, full=True, claps=(1, 3), hats=True, kick_on=True):
        for b in range(4):
            if kick_on:
                track.add("kick", K, bar_beat + b)
                track.kicks.append(bar_beat + b)
            if b in claps:
                track.add("drums", CL, bar_beat + b, 0.9, send=0.25)
            if hats:
                track.add("drums", HO if full else HC, bar_beat + b + 0.5, 0.55 if full else 0.7, pan=0.15)
                if full:
                    for s in (0.25, 0.75):
                        track.add("drums", HC, bar_beat + b + s, 0.45 + 0.1 * (s == 0.75), pan=-0.2)

    def bass_bar(bar_beat, pattern="pump"):
        root = ROOTS[chord_at(bar_beat)]
        for b in range(4):
            if pattern == "pump":
                for s in (0.5,):
                    track.add("music", bass(root, 0.3, 0.2), bar_beat + b + s, 0.9)
                track.add("music", bass(root, 0.3, 0.16, 600), bar_beat + b + 0.75, 0.55)
            else:
                track.add("music", bass(root, 2.2, 1.9, 500), bar_beat, 0.8) if b == 0 else None

    def pad_bar(bar_beat, gain=1.0, cutoff=3200, beats=4):
        notes = chord_notes(chord_at(bar_beat))
        track.add("music", supersaw(notes, beats * BEAT + 0.6, beats * BEAT - 0.05, cutoff), bar_beat, 0.5 * gain, send=0.35)

    def arp_bar(bar_beat, gain=1.0, cutoff=5000):
        notes = chord_notes(chord_at(bar_beat))
        order = [0, 1, 2, 3, 2, 1, 2, 3] * 2
        for k in range(16):
            note = notes[order[k]] + 12
            track.add("music", pluck(note, 0.4, cutoff), bar_beat + k / 4, 0.55 * gain, pan=(-0.35 if k % 2 else 0.35), send=0.3)

    def hook_bars(start, end, octave=0, gain=1.0):
        for bar_beat in range(start, end, 4):
            bar = HOOK[int((bar_beat - start) // 4) % 4]
            for beat, length, note in bar:
                track.add("lead", lead(note + octave, length * BEAT + 0.4, length * BEAT * 0.9), bar_beat + beat, gain, send=0.25)

    def roll(start, beats, gain=1.0):
        """A snare roll: eighths, then sixteenths, growing."""
        half = start + beats / 2
        steps = [start + k / 2 for k in range(int(beats))] + [half + k / 4 for k in range(int(beats * 2))]
        for i, at in enumerate(sorted(set(steps))):
            if at >= start + beats - 0.5:
                break
            track.add("drums", SN, at, gain * (0.35 + 0.65 * i / max(1, len(steps))), send=0.2)

    for name, a, b in SECTIONS:
        for bar_beat in range(a, b, 4):
            in_section = bar_beat - a
            if name == "intro":
                # The cold open: a hit with the palette, then the build.
                if bar_beat == 0:
                    track.add("fx", impact(), 0, 0.8)
                    track.add("drums", crash(), 0, 0.9, send=0.3)
                cut = np.linspace(500 + 300 * in_section, 800 + 450 * (in_section + 4), int(4 * BEAT * RATE + 0.6 * RATE))
                pad_bar(bar_beat, 0.9, cut)
                if bar_beat >= 4:
                    arp_bar(bar_beat, 0.5 + 0.1 * in_section / 4, 1400 + 500 * in_section / 4)
                if bar_beat >= 8:
                    for k in range(4):
                        track.add("kick", K, bar_beat + k, 0.85)
                        track.kicks.append(bar_beat + k)
                        track.add("drums", HC, bar_beat + k + 0.5, 0.6)
                if bar_beat == 8:
                    track.add("fx", riser(8 * BEAT), 8, 0.8)
                if bar_beat == 12:
                    roll(12, 3.5, 0.9)
            elif name in ("drop", "groove", "final"):
                if bar_beat == a:
                    track.add("drums", crash(), a, 1.0, send=0.3)
                    track.add("fx", impact(), a, 0.45)
                drums_bar(bar_beat)
                bass_bar(bar_beat)
                pad_bar(bar_beat, 0.8)
                arp_bar(bar_beat, 0.75)
                if name == "final":
                    hook_bars(bar_beat, bar_beat + 4, 0, 1.0)
                if name == "drop" and in_section >= 16:
                    hook_bars(bar_beat, bar_beat + 4, -12, 0.55)
            elif name == "lift":
                if bar_beat == a:
                    track.add("drums", crash(), a, 1.0, send=0.3)
                drums_bar(bar_beat, claps=(0, 1, 2, 3) if bar_beat >= 52 else (1, 3))
                bass_bar(bar_beat)
                pad_bar(bar_beat, 0.85, 4200)
                arp_bar(bar_beat, 0.8, 6500)
                hook_bars(bar_beat, bar_beat + 4, 12 if bar_beat >= 52 else 0, 0.85)
            elif name == "breakdown":
                # Clipboard and calculator: lighter, then the build back.
                if bar_beat == a:
                    track.add("drums", crash(), a, 0.8, send=0.4)
                    track.add("kick", K, a, 0.9)
                pad_bar(bar_beat, 0.9, 2400)
                arp_bar(bar_beat, 0.6, 3500)
                for k in range(4):
                    track.add("drums", HC, bar_beat + k + 0.5, 0.5, pan=0.2)
                if bar_beat >= 84:
                    bass_bar(bar_beat, "long")
                if bar_beat >= 88:
                    for k in range(4):
                        track.add("kick", K, bar_beat + k, 0.7 if bar_beat < 92 else 0.85)
                        track.kicks.append(bar_beat + k)
                if bar_beat == 88:
                    track.add("fx", riser(8 * BEAT), 88, 0.75)
                if bar_beat == 92:
                    roll(92, 3.5, 0.85)
            elif name == "end":
                if bar_beat == a:
                    # The final hit, then the chord rings out.
                    track.add("fx", impact(), a, 0.9)
                    track.add("drums", crash(), a, 1.0, send=0.4)
                    track.add("kick", K, a, 1.0)
                    track.kicks.append(a)
                    notes = chord_notes("F")
                    track.add("music", supersaw(notes + [notes[0] + 24], (b - a) * BEAT + TAIL, (b - a) * BEAT - 2.5, 2600), a, 0.6, send=0.5)
                    track.add("music", bass(ROOTS["F"], 4.0, 3.0, 400), a, 0.7)
                    for k, note in enumerate([72, 77, 81, 84]):
                        track.add("lead", lead(note, 2.2, 0.6), a + k * 0.5, 0.35, send=0.5)
    # A sweep of noise under each change of scene, peaking on its beat.
    for beat in WIPES:
        track.add("fx", whoosh(0.7), beat - 0.7 * 0.55 / BEAT, 0.7)
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


def sidechain(n, kicks, depth=0.6, release=0.16):
    env = np.ones(n)
    t = t_of(int(release * 5 * RATE))
    dip = 1 - depth * np.exp(-t / release) * np.clip(t / 0.004, 0, 1) ** 0.5
    for beat in kicks:
        at = round(beat * SPB)
        seg = dip[: max(0, n - at)]
        env[at : at + len(seg)] = np.minimum(env[at : at + len(seg)], seg)
    return env


def limiter(x, ceiling=10 ** (-1.2 / 20), look=0.005, release=0.08):
    peak = np.abs(x).max(1)
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


def render():
    track = compose()
    n = track.n
    duck = sidechain(n, track.kicks)[:, None]
    ir = reverb_ir()
    send = track.buses["send"] + 0.0
    wet = np.stack([fftconvolve(send[:, 0], ir[:, 0])[:n], fftconvolve(send[:, 1], ir[:, 1])[:n]], 1)
    lead_bus = delay(track.buses["lead"], 0.75, 0.38, 0.3)
    mix = (
        track.buses["kick"] * 0.8
        + track.buses["drums"] * 1.0
        + track.buses["music"] * duck * 0.7
        + lead_bus * duck * 0.6
        + track.buses["fx"] * 0.7
        + wet * duck * 0.35
    )
    # glue: a gentle high-pass on everything, a soft clip on the bus
    mix = np.stack([highpass(mix[:, 0], 28), highpass(mix[:, 1], 28)], 1)
    mix = np.tanh(mix * 0.9) / 0.9
    # out after the last beat
    end = round((LENGTH * BEAT + TAIL) * RATE)
    mix = mix[:end]
    fade = round(TAIL * RATE * 0.9)
    mix[-fade:] *= np.linspace(1, 0, fade)[:, None] ** 2
    return master(mix)


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
    for name, a, b in SECTIONS:
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
