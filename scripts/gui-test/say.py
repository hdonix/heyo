#!/usr/bin/python3
"""say.py OUT.wav SENTENCE...: speech for the dictation tests.

Speaks the sentences with Flite (libflite and its cmu_us_slt voice, which
Debian and Ubuntu package as libflite1), each followed by a pause long
enough for the sentence to count as finished, and writes 16 kHz mono 16-bit
PCM: what Heyo's dictation helper reads in place of a microphone
(HEYO_DICTATION_WAV). Nothing is recorded and nothing is kept in the
repository; the same sentences give the same audio every time."""
import array
import ctypes
import sys
import wave

RATE = 16000
PAUSE = 1.3


class Wave(ctypes.Structure):
    _fields_ = [("type", ctypes.c_char_p), ("sample_rate", ctypes.c_int), ("num_samples", ctypes.c_int), ("num_channels", ctypes.c_int), ("samples", ctypes.POINTER(ctypes.c_short))]


def voice():
    try:
        flite = ctypes.CDLL("libflite.so.1", mode=ctypes.RTLD_GLOBAL)
        for needed in ("libflite_usenglish.so.1", "libflite_cmulex.so.1"):
            ctypes.CDLL(needed, mode=ctypes.RTLD_GLOBAL)
        spoken = ctypes.CDLL("libflite_cmu_us_slt.so.1", mode=ctypes.RTLD_GLOBAL)
    except OSError as e:
        sys.exit(f"say.py: Flite is not installed ({e}); on Debian and Ubuntu: apt install libflite1")
    flite.flite_init()
    spoken.register_cmu_us_slt.restype = ctypes.c_void_p
    spoken.register_cmu_us_slt.argtypes = [ctypes.c_char_p]
    flite.flite_text_to_wave.restype = ctypes.POINTER(Wave)
    flite.flite_text_to_wave.argtypes = [ctypes.c_char_p, ctypes.c_void_p]
    return flite, spoken.register_cmu_us_slt(None)


def at_rate(samples, rate):
    """Resamples by linear interpolation: speech for a recognizer needs no better."""
    if rate == RATE:
        return samples
    out = array.array("h")
    for i in range(int(len(samples) * RATE / rate)):
        x = i * rate / RATE
        a = int(x)
        b = min(a + 1, len(samples) - 1)
        out.append(int(samples[a] + (samples[b] - samples[a]) * (x - a)))
    return out


def main():
    out, sentences = sys.argv[1], sys.argv[2:]
    flite, spoken = voice()
    pcm = array.array("h", [0] * (RATE // 2))
    for sentence in sentences:
        made = flite.flite_text_to_wave(sentence.encode(), spoken).contents
        pcm.extend(at_rate(array.array("h", made.samples[: made.num_samples]), made.sample_rate))
        pcm.extend([0] * int(RATE * PAUSE))
    with wave.open(out, "wb") as w:
        w.setnchannels(1)
        w.setsampwidth(2)
        w.setframerate(RATE)
        w.writeframes(pcm.tobytes())
    print(f"{len(pcm) / RATE:.1f}")


main()
