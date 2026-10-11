import { describe, expect, it } from "vitest";

import { type Engine, PARTIAL_INTERVAL_MS, SAMPLE_RATE, type Said, Session, VAD_WINDOW } from "./sentences";

/// A recognizer and a detector for tests. Audio is words: a word is a
/// stretch of samples of one loudness, which names it. The recognizer
/// writes the words it is given and ends with a mark, as SenseVoice does;
/// the detector finishes a sentence after a pause.
const WORD = SAMPLE_RATE / 4;
const PAUSE = Math.ceil((0.6 * SAMPLE_RATE) / VAD_WINDOW);

function engine(words: string[], mark = "."): Engine {
  let fed = 0;
  let start = -1;
  let quiet = 0;
  let speech: Float32Array[] = [];
  const done: { start: number; samples: Float32Array }[] = [];
  const merge = (chunks: Float32Array[]) => Float32Array.from(chunks.flatMap((chunk) => [...chunk]));
  const end = () => {
    if (start >= 0) done.push({ start, samples: merge(speech) });
    start = -1;
    speech = [];
  };
  return {
    decode(samples) {
      const heard: string[] = [];
      let run = 0;
      for (let i = 0; i <= samples.length; i++) {
        if (i < samples.length && samples[i] === samples[i - 1]) run++;
        else {
          const word = words[Math.round(samples[i - 1] * 100) - 1];
          // Half a word is not heard yet.
          if (word && run >= WORD / 2) heard.push(word);
          run = 1;
        }
      }
      return heard.length ? heard.join(" ") + mark : mark;
    },
    vad: {
      acceptWaveform(window) {
        const loud = window.some((sample) => sample !== 0);
        if (loud && start < 0) start = fed;
        if (start >= 0) speech.push(window.slice());
        quiet = loud ? 0 : quiet + 1;
        fed += window.length;
        if (start >= 0 && quiet >= PAUSE) end();
      },
      isDetected: () => start >= 0,
      isEmpty: () => done.length === 0,
      front: () => done[0],
      pop: () => void done.shift(),
      flush: end,
    },
  };
}

/// The audio of a sentence: its words (by number, from 1), then a pause.
function audio(sentence: number[], pause = 1) {
  const samples = new Float32Array(sentence.length * WORD + pause * SAMPLE_RATE);
  sentence.forEach((word, i) => samples.fill(word / 100, i * WORD, (i + 1) * WORD));
  return samples;
}

/// Feeds a recording a tenth of a second at a time, as the recorder does,
/// with the clock moving along, and collects what it says.
function record(recognizer: Engine, recording: Float32Array[], correct = (text: string) => text) {
  const said: Said[] = [];
  let now = 1000;
  const session = new Session(recognizer, correct, (line) => said.push(line), () => now);
  for (const samples of recording) {
    for (let offset = 0; offset < samples.length; offset += SAMPLE_RATE / 10) {
      session.feed(samples.subarray(offset, offset + SAMPLE_RATE / 10));
      now += 100;
    }
  }
  const text = session.finish();
  return { said, text, texts: (type: "partial" | "segment") => said.flatMap((line) => (line.type === type ? [line.text] : [])) };
}

const WORDS = ["the", "weather", "is", "nice", "today", "walk", "to", "park"];

describe("a recording", () => {
  it("says the sentence being spoken as it grows, and then the sentence", () => {
    const { texts, text } = record(engine(WORDS), [audio([1, 2, 3, 4, 5])]);
    // Nothing before there is enough to recognize; then every 300 ms, also
    // through the pause in which the sentence might still go on.
    expect([...new Set(texts("partial"))]).toEqual(["the weather is.", "the weather is nice.", "the weather is nice today."]);
    expect(texts("segment")).toEqual(["the weather is nice today."]);
    expect(text).toBe("the weather is nice today.");
  });

  it("recognizes the sentence in progress no oftener than its interval", () => {
    const { said } = record(engine(WORDS), [audio([1, 2, 3, 4, 5, 6, 7, 8])]);
    const partials = said.filter((line) => line.type === "partial").length;
    // Two seconds of speech, of which the first 0.8 s say nothing.
    expect(partials).toBeLessThanOrEqual(Math.ceil(2000 / PARTIAL_INTERVAL_MS));
    expect(partials).toBeGreaterThanOrEqual(3);
  });

  it("finishes each sentence at the pause after it", () => {
    const { said, texts, text } = record(engine(WORDS), [audio([1, 2, 3, 4]), audio([6, 7, 1, 8])]);
    expect(texts("segment")).toEqual(["the weather is nice.", "walk to the park."]);
    expect(text).toBe("the weather is nice.walk to the park.");
    // A sentence is said after the last that was heard of it in progress,
    // and nothing more is heard of it then.
    const order = said.filter((line) => line.type !== "level").map((line) => `${line.type} ${"text" in line ? line.text : ""}`);
    const first = order.indexOf("segment the weather is nice.");
    expect(order.slice(0, first).every((line) => line.startsWith("partial the"))).toBe(true);
    expect(order.slice(first + 1).every((line) => line.includes("walk"))).toBe(true);
  });

  it("finishes what was still being said when it is ended", () => {
    const { texts, text } = record(engine(WORDS), [audio([1, 2, 3, 4], 0)]);
    expect(texts("segment")).toEqual(["the weather is nice."]);
    expect(text).toBe("the weather is nice.");
  });

  it("writes the user's words their way, in progress and finished", () => {
    const { texts } = record(engine(WORDS), [audio([1, 2, 3, 4])], (text) => text.replace("weather", "Wetter"));
    expect(texts("partial")[0]).toBe("the Wetter is.");
    expect(texts("segment")).toEqual(["the Wetter is nice."]);
  });

  it("says nothing of noise", () => {
    // A sound the recognizer makes no word of: only a mark comes back.
    const { texts, text } = record(engine(WORDS), [audio([50, 50, 50, 50, 50])]);
    expect(texts("partial")).toEqual([]);
    expect(texts("segment")).toEqual([]);
    expect(text).toBe("");
  });

  it("says how loud it is, from 0 to 1", () => {
    const { said } = record(engine(WORDS), [audio([1, 2, 3, 4])]);
    const levels = said.flatMap((line) => (line.type === "level" ? [line.value] : []));
    expect(levels.length).toBeGreaterThan(10);
    expect(levels.every((value) => value >= 0 && value <= 1)).toBe(true);
    // Speech first, silence after.
    expect(levels[1]).toBeGreaterThan(0);
    expect(levels[levels.length - 1]).toBe(0);
  });
});
