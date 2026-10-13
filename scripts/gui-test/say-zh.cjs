#!/usr/bin/env node
// say-zh.cjs OUT.wav SENTENCE...: speech in Chinese, or Chinese with English
// in it, for the film (demo-film.sh).
//
// say.py speaks English only (Flite). This speaks with the MeloTTS model for
// Chinese and English through sherpa-onnx, the library Heyo's dictation
// already installs, and writes what say.py writes: 16 kHz mono 16-bit PCM,
// half a second of quiet, then each sentence followed by a pause long enough
// to count as finished. It prints the length in seconds, as say.py does.
//
//   HEYO_TTS_MODEL  the unpacked vits-melo-tts-zh_en (from sherpa-onnx's
//                   tts-models release); default ~/.cache/heyo-build/tts/…
//   HEYO_TTS_LIB    a node_modules holding sherpa-onnx-node; default the one
//                   beside Heyo's dictation models
const fs = require("fs");
const os = require("os");
const path = require("path");

const RATE = 16000;
const PAUSE = 1.3;

const home = os.homedir();
const model = process.env.HEYO_TTS_MODEL || path.join(home, ".cache/heyo-build/tts/vits-melo-tts-zh_en");
const lib = process.env.HEYO_TTS_LIB || path.join(home, ".local/share/heyo/dictation/node_modules");
const [out, ...sentences] = process.argv.slice(2);
if (!out || !sentences.length) {
  console.error("usage: say-zh.cjs OUT.wav SENTENCE...");
  process.exit(2);
}
for (const needed of [path.join(model, "model.onnx"), path.join(lib, "sherpa-onnx-node")]) {
  if (!fs.existsSync(needed)) {
    console.error(`say-zh.cjs: ${needed} is not there`);
    process.exit(1);
  }
}
const sherpa = require(path.join(lib, "sherpa-onnx-node"));

const tts = new sherpa.OfflineTts({
  model: {
    vits: {
      model: path.join(model, "model.onnx"),
      lexicon: path.join(model, "lexicon.txt"),
      tokens: path.join(model, "tokens.txt"),
      dictDir: path.join(model, "dict"),
    },
    numThreads: 4,
    provider: "cpu",
  },
  ruleFsts: ["date.fst", "number.fst"].map((f) => path.join(model, f)).join(","),
});

const parts = [new Float32Array(RATE / 2)];
for (const sentence of sentences) {
  const spoken = tts.generate({ text: sentence, sid: 0, speed: 1.0 });
  const samples =
    spoken.sampleRate === RATE
      ? spoken.samples
      : new sherpa.LinearResampler(spoken.sampleRate, RATE).resample(spoken.samples, true);
  parts.push(samples, new Float32Array(Math.round(PAUSE * RATE)));
}
const length = parts.reduce((n, p) => n + p.length, 0);
const pcm = Buffer.alloc(44 + length * 2);
pcm.write("RIFF", 0);
pcm.writeUInt32LE(36 + length * 2, 4);
pcm.write("WAVEfmt ", 8);
pcm.writeUInt32LE(16, 16);
pcm.writeUInt16LE(1, 20);
pcm.writeUInt16LE(1, 22);
pcm.writeUInt32LE(RATE, 24);
pcm.writeUInt32LE(RATE * 2, 28);
pcm.writeUInt16LE(2, 32);
pcm.writeUInt16LE(16, 34);
pcm.write("data", 36);
pcm.writeUInt32LE(length * 2, 40);
let at = 44;
for (const part of parts) {
  for (const sample of part) {
    pcm.writeInt16LE(Math.round(Math.max(-1, Math.min(1, sample)) * 32767), at);
    at += 2;
  }
}
fs.writeFileSync(out, pcm);
console.log((length / RATE).toFixed(1));
