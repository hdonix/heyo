//! One recording, as the dictation helper hears it: audio goes in, and out
//! come the sentence being spoken, as far as it is recognized (`partial`),
//! and each sentence once the speaker has paused after it (`segment`).
//!
//! The recognizer is not a streaming one: it is given the whole sentence so
//! far, again and again, and may write its beginning differently each time.
//! Whoever shows the partial text must be ready to correct it.
//!
//! What is said here is text as it is to be written one piece after the
//! other: a sentence begins with the space that separates it from the one
//! before, where the language has one, so that the pieces only need joining.

/// The recognizer and the voice-activity detector, as much of them as a
/// recording uses (`sherpa-onnx-node`, or a test's own).
export interface Engine {
  decode(samples: Float32Array): string;
  vad: {
    acceptWaveform(samples: Float32Array): void;
    isDetected(): boolean;
    isEmpty(): boolean;
    front(): { start: number; samples: Float32Array };
    pop(): void;
    flush(): void;
    /// Forgets all it has heard, and counts its samples from nothing again.
    reset(): void;
  };
}

/// What a recording says while it runs.
export type Said = { type: "level"; value: number } | { type: "partial"; text: string } | { type: "segment"; text: string };

export const SAMPLE_RATE = 16000;
export const VAD_WINDOW = 512;
/// How often the sentence in progress is re-recognized.
export const PARTIAL_INTERVAL_MS = 300;
/// Shorter audio than this decodes to noise ("Yeah.", stray syllables).
const MIN_PARTIAL_SECONDS = 0.8;
/// Audio kept from before speech was detected, so first syllables survive.
const PREROLL_SECONDS = 0.4;

function concat(chunks: Float32Array[]) {
  const out = new Float32Array(chunks.reduce((n, c) => n + c.length, 0));
  let offset = 0;
  for (const chunk of chunks) {
    out.set(chunk, offset);
    offset += chunk.length;
  }
  return out;
}

/// SenseVoice returns a lone punctuation mark for noise.
const meaningful = (text: string) => /[\p{L}\p{N}]/u.test(text);

/// Chinese and Japanese writing, and the full-width marks that go with it:
/// nothing stands between two sentences there.
const UNSPACED = /[\p{Script=Han}\p{Script=Hiragana}\p{Script=Katakana}\u3000-\u303f\uff00-\uffef]/u;

/// What goes between what was said so far and the next sentence: a space,
/// unless the text before ends in writing that has none between sentences
/// (or in a space already, or there is no text before).
export function between(before: string, sentence: string): string {
  const last = [...before].pop();
  if (last === undefined || !sentence || /\s/u.test(last) || UNSPACED.test(last) || /^\s/u.test(sentence)) return "";
  return " ";
}

/// A sentence still being spoken, without the mark the recognizer ends
/// whatever it is given with: the full stop after "The weather is." would
/// only have to be taken back when the next word comes.
export function unfinished(text: string): string {
  const open = text.replace(/[\s.,!?;:…。，！？；：、~—–-]+$/u, "");
  return meaningful(open) ? open : text;
}

/// One recording: feeds audio to the detector and says partial and final text.
export class Session {
  private pending = new Float32Array(0);
  /// Audio of the sentence in progress (with pre-roll).
  private speech: Float32Array[] = [];
  private speechSamples = 0;
  /// Samples given to the detector so far; `speech` ends here.
  private fed = 0;
  private speaking = false;
  private lastPartial = 0;
  private levelEnergy = 0;
  private levelCount = 0;
  private levelAt = 0;
  /// The finished sentences, each as it joins those before it.
  readonly segments: string[] = [];

  constructor(
    private engine: Engine,
    private say: (said: Said) => void,
    private now: () => number = Date.now,
  ) {
    // The detector outlives a recording. Left as it was, it would go on
    // counting samples where the last one stopped, and no sentence of this
    // one would be found in what was kept of it (`withPreroll`).
    engine.vad.reset();
  }

  feed(samples: Float32Array) {
    // Loudness as RMS (peaks saturate on any speech), on a square-root scale
    // so quiet and loud speech both show movement.
    for (const s of samples) this.levelEnergy += s * s;
    this.levelCount += samples.length;
    if (this.now() - this.levelAt > 40 && this.levelCount) {
      const rms = Math.sqrt(this.levelEnergy / this.levelCount);
      this.say({ type: "level", value: Math.min(1, Math.sqrt(rms) * 1.6) });
      this.levelEnergy = 0;
      this.levelCount = 0;
      this.levelAt = this.now();
    }

    const joined = concat([this.pending, samples]);
    let offset = 0;
    for (; offset + VAD_WINDOW <= joined.length; offset += VAD_WINDOW) {
      const window = joined.subarray(offset, offset + VAD_WINDOW);
      this.engine.vad.acceptWaveform(window);
      this.speech.push(window.slice());
      this.speechSamples += VAD_WINDOW;
      this.fed += VAD_WINDOW;
      this.speaking ||= this.engine.vad.isDetected();
      if (!this.speaking) this.trimPreroll();
      this.drain();
    }
    this.pending = joined.slice(offset);

    const seconds = this.speechSamples / SAMPLE_RATE;
    if (this.speaking && seconds >= MIN_PARTIAL_SECONDS && this.now() - this.lastPartial >= PARTIAL_INTERVAL_MS) {
      this.lastPartial = this.now();
      const text = unfinished(this.engine.decode(concat(this.speech)));
      if (meaningful(text)) this.say({ type: "partial", text: this.joined(text) });
    }
  }

  /// The sentence as it is written after the ones before it.
  private joined(text: string) {
    return between(this.segments.join(""), text) + text;
  }

  private trimPreroll() {
    const keep = PREROLL_SECONDS * SAMPLE_RATE;
    while (this.speech.length && this.speechSamples - this.speech[0].length >= keep) {
      this.speechSamples -= this.speech.shift()!.length;
    }
  }

  /// Says the sentences the detector has finished.
  private drain() {
    const { vad } = this.engine;
    while (!vad.isEmpty()) {
      const segment = vad.front();
      vad.pop();
      const text = this.engine.decode(this.withPreroll(segment));
      this.speech = [];
      this.speechSamples = 0;
      this.speaking = false;
      if (!meaningful(text)) continue;
      const sentence = this.joined(text);
      this.segments.push(sentence);
      this.say({ type: "segment", text: sentence });
    }
  }

  /// The detector's segment, extended backwards: it trims first syllables.
  private withPreroll(segment: { start: number; samples: Float32Array }): Float32Array {
    const buffer = concat(this.speech);
    const bufferStart = this.fed - buffer.length;
    const end = segment.start + segment.samples.length;
    const start = Math.max(bufferStart, segment.start - PREROLL_SECONDS * SAMPLE_RATE);
    // Fall back to the bare segment if the positions do not line up.
    if (!(segment.start >= bufferStart && end <= this.fed && start < end)) return segment.samples;
    return buffer.subarray(start - bufferStart, end - bufferStart);
  }

  /// Ends the recording: what was still being said is finished, and all of
  /// it is returned.
  finish() {
    this.engine.vad.flush();
    this.drain();
    return this.segments.join("");
  }
}
