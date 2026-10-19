"""sync.py: whether what is seen in a film happens with what is heard.

`measure(film, events, rate)` takes the rendered film, the things that
should be seen to happen on a beat, (seconds, what, where) with `where` the
part of the 1920x1080 picture to look at or None for all of it
(plan.seen()), and the film's pictures a second.

- Seen: each is looked for in the pictures within two of where it should
  be, as the picture in which that part of the picture starts to change:
  the most change from the picture before, less the change just before
  that (so that a fade or a moving camera does not count, and a cut, a word
  arriving or a letter typed does). Where a part is named, what is looked
  at there is the near-white of the type, not the picture behind it.
- Heard: the notes in the film's own sound, by an onset detector: the
  peaks of librosa's onset curve (spectral flux on a 23 ms window, so that
  a peak comes some 5 ms after the note it marks), at least 10 ms apart. A
  note's strength is its peak against the median peak's; strong is at
  least the median.

For each thing seen it names the note heard with it (the strongest within
one picture of the picture it was seen in, or else the nearest), how far
that note is from the picture, and its strength against the median note's.
On it is within one picture and at least as strong as the median note.

Used by edit.py; alone: sync.py FILM.mp4 [PICTURES-A-SECOND] measures it
against plan.seen().
"""
import subprocess
import sys
from pathlib import Path

import imageio_ffmpeg
import librosa
import numpy as np

FFMPEG = imageio_ffmpeg.get_ffmpeg_exe()
SMALL = (480, 270)  # the pictures are looked at a quarter of their size
SOUND = 22050
HOP = 32


def pictures(film):
    raw = subprocess.run([FFMPEG, "-hide_banner", "-loglevel", "error", "-i", str(film), "-vf", f"scale={SMALL[0]}:{SMALL[1]}:flags=area,format=gray",
                          "-f", "rawvideo", "-pix_fmt", "gray", "-"], capture_output=True, check=True).stdout
    return np.frombuffer(raw, np.uint8).reshape(-1, SMALL[1], SMALL[0])


def notes(film):
    """The onsets in the film's sound: when each begins, and by how many dB
    the sound rises with it."""
    raw = subprocess.run([FFMPEG, "-hide_banner", "-loglevel", "error", "-i", str(film), "-vn", "-ac", "1", "-ar", str(SOUND), "-f", "f32le", "-"],
                         capture_output=True, check=True).stdout
    y = np.frombuffer(raw, np.float32)
    envelope = librosa.onset.onset_strength(y=y, sr=SOUND, hop_length=HOP, n_fft=512)
    near = round(0.01 * SOUND / HOP)
    peaks = librosa.util.peak_pick(envelope, pre_max=near, post_max=near, pre_avg=5 * near, post_avg=5 * near, delta=float(np.median(envelope)), wait=near)
    return peaks * HOP / SOUND, envelope[peaks] / np.median(envelope[peaks])


def measure(film, events, rate):
    frames = pictures(film)
    count = len(frames)
    onsets, strength = notes(film)
    strong = strength >= 1
    lines = [f"what is seen against what is heard, in {film} ({rate} pictures a second, so a picture is {1000 / rate:.1f} ms)",
             f"notes in the sound: {len(onsets)}, of them strong (at least the median note's strength): {int(strong.sum())}", "",
             "  seconds  picture  seen at     the note heard with it  its strength    what",
             "                    (pictures)  (ms from that picture)  (x the median)"]
    offsets = []
    for seconds, what, where in events:
        expected = round(seconds * rate)
        x, y, w, h = where or (0, 0, 1920, 1080)
        sx, sy = SMALL[0] / 1920, SMALL[1] / 1080
        part = (slice(round(y * sy), round((y + h) * sy)), slice(round(x * sx), round((x + w) * sx)))
        if where:
            change = lambda n: np.mean((frames[n][part] > 160) != (frames[n - 1][part] > 160))  # noqa: E731
        else:
            change = lambda n: np.abs(frames[n][part].astype(np.int16) - frames[n - 1][part]).mean()  # noqa: E731
        near = range(max(3, expected - 2), min(count, expected + 3))
        starts = [change(n) - max(change(n - 1), change(n - 2)) for n in near]
        seen = near[int(np.argmax(starts))]
        at = seen / rate
        close = np.where(np.abs(onsets - at) <= 1 / rate)[0]
        k = int(close[np.argmax(strength[close])]) if len(close) else int(np.argmin(np.abs(onsets - at)))
        off = onsets[k] - at
        offsets.append((off, strength[k]))
        mark = "" if abs(off) <= 1 / rate else "  <-- more than a picture away"
        mark += "" if strong[k] else "  <-- weaker than the median note"
        lines.append(f"  {seconds:7.3f}  {expected:7d}  {seen - expected:+3d}         {off * 1000:+8.1f}               {strength[k]:4.1f}x         {what}{mark}")
    offsets, strengths = np.abs(np.array([o for o, _ in offsets])), np.array([x for _, x in offsets])
    within = offsets <= 1 / rate
    lines += ["", f"{len(offsets)} things seen; a note within a picture ({1000 / rate:.1f} ms) of {int(within.sum())} of them, "
              f"at least as strong as the median note for {int((within & (strengths >= 1)).sum())}; "
              f"the median distance {np.median(offsets) * 1000:.1f} ms, the furthest {offsets.max() * 1000:.1f} ms; "
              f"the weakest such note {strengths[within].min():.2f}x the median"]
    return lines


if __name__ == "__main__":
    sys.path.insert(0, str(Path(__file__).resolve().parent))
    import plan

    rate = int(sys.argv[2]) if len(sys.argv) > 2 else 24
    print("\n".join(measure(sys.argv[1], [(b * plan.BEAT, what, where) for b, what, where in plan.seen()], rate)))
