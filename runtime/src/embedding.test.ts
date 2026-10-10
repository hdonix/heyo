import { describe, expect, it } from "vitest";

import { padded, pooled, prefixed, Tokenizer, type TokenizerFile } from "./embedding";
import { type Lock, manifest } from "./install";
import embedLock from "./locks/embed.json";

/// A vocabulary small enough to work out by hand. Ids 0–3 are the markers,
/// as in the real one.
function vocabulary(pieces: [string, number][]): TokenizerFile {
  return {
    normalizer: { normalizers: [] },
    model: { unk_id: 3, vocab: [["<s>", 0], ["<pad>", 0], ["</s>", 0], ["<unk>", 0], ...pieces] },
  };
}

const tiny = new Tokenizer(
  vocabulary([
    ["▁", -2],
    ["▁he", -3],
    ["▁hello", -4],
    ["llo", -3],
    ["l", -5],
    ["o", -5],
    ["▁world", -4],
    ["▁wor", -3],
    ["ld", -3.5],
    ["发", -4],
    ["票", -4],
    ["发票", -5],
    ["▁发票", -6],
  ]),
);
const id = (piece: string) => ["▁", "▁he", "▁hello", "llo", "l", "o", "▁world", "▁wor", "ld", "发", "票", "发票", "▁发票"].indexOf(piece) + 4;

describe("the tokenizer", () => {
  it("cuts each word the most likely way", () => {
    // "▁hello" (-4) beats "▁he" + "llo" (-6); "▁world" (-4) beats "▁wor" + "ld" (-6.5).
    expect(tiny.encode("hello world", 16)).toEqual([0, id("▁hello"), id("▁world"), 2]);
    // No space between Chinese words: one "word", cut into its pieces.
    expect(tiny.encode("发票", 16)).toEqual([0, id("▁发票"), 2]);
    expect(tiny.encode("发票发票", 16)).toEqual([0, id("▁发票"), id("发票"), 2]);
  });

  it("puts a marker at each end and one for a space", () => {
    expect(tiny.encode("", 16)).toEqual([0, 2]);
    // Runs of spaces are one; a space at the end is a word of its own.
    expect(tiny.encode("hello   world ", 16)).toEqual([0, id("▁hello"), id("▁world"), id("▁"), 2]);
  });

  it("makes one unknown piece of a run of characters it has no piece for", () => {
    expect(tiny.encode("hello zzz", 16)).toEqual([0, id("▁hello"), id("▁"), 3, 2]);
    expect(tiny.encode("zq发票", 16)).toEqual([0, id("▁"), 3, id("发票"), 2]);
    // Characters outside the basic plane are one character each.
    expect(tiny.encode("😀😀", 16)).toEqual([0, id("▁"), 3, 2]);
  });

  it("does not take text that spells a marker for the marker", () => {
    const ids = tiny.encode("</s> hello", 16);
    expect(ids.at(-1)).toBe(2);
    expect(ids.slice(1, -1)).not.toContain(2);
    expect(ids.slice(1, -1)).not.toContain(0);
  });

  it("stops at the limit, and still ends with the end marker", () => {
    const long = Array.from({ length: 50 }, () => "hello").join(" ");
    const ids = tiny.encode(long, 8);
    expect(ids).toHaveLength(8);
    expect(ids[0]).toBe(0);
    expect(ids.at(-1)).toBe(2);
    expect(ids.slice(1, -1).every((piece) => piece === id("▁hello"))).toBe(true);
  });
});

describe("pooling", () => {
  it("fills short texts up and says which ids are filling", () => {
    const { ids, mask, longest } = padded([
      [0, 7, 8, 2],
      [0, 9, 2],
    ]);
    expect(longest).toBe(4);
    expect([...ids].map(Number)).toEqual([0, 7, 8, 2, 0, 9, 2, 1]);
    expect([...mask].map(Number)).toEqual([1, 1, 1, 1, 1, 1, 1, 0]);
  });

  it("is the mean over the tokens that are text, at length one", () => {
    // Two texts of up to three tokens, vectors of two numbers.
    const hidden = Float32Array.from([
      ...[1, 0, 3, 0, 5, 8], // text 0: mean (3, 8/3)
      ...[0, 2, 0, 4, 100, 100], // text 1: its third token is filling
    ]);
    const mask = BigInt64Array.from([1n, 1n, 1n, 1n, 1n, 0n]);
    const [first, second] = pooled(hidden, mask, 2, 3, 2);
    const length = Math.hypot(3, 8 / 3);
    expect(first[0]).toBeCloseTo(3 / length, 6);
    expect(first[1]).toBeCloseTo(8 / 3 / length, 6);
    // The filling does not count: the mean is (0, 3), which is (0, 1) at length one.
    expect(second[0]).toBeCloseTo(0, 6);
    expect(second[1]).toBeCloseTo(1, 6);
    for (const vector of [first, second]) expect(Math.hypot(...vector)).toBeCloseTo(1, 6);
  });

  it("survives a text with no tokens and a vector of nothing", () => {
    const [vector] = pooled(new Float32Array(4), BigInt64Array.from([0n, 0n]), 1, 2, 2);
    expect([...vector]).toEqual([0, 0]);
  });

  it("puts the model's two words in front", () => {
    expect(prefixed("query", "发票")).toBe("query: 发票");
    expect(prefixed("passage", "ssh deploy@host")).toBe("passage: ssh deploy@host");
  });
});

describe("the embedding engine's lockfile", () => {
  const lock: Lock = embedLock;

  it("names one exact version", () => {
    expect(manifest(lock).dependencies).toEqual({ "onnxruntime-node": "1.30.0" });
    expect(lock.packages["node_modules/onnxruntime-node"].version).toBe("1.30.0");
  });

  it("pins every package to the registry's file and its hash", () => {
    const installed = Object.entries(lock.packages).filter(([where]) => where !== "");
    expect(installed.length).toBeGreaterThan(1);
    for (const [where, entry] of installed) {
      expect(entry.resolved, where).toMatch(/^https:\/\/registry\.npmjs\.org\/.+\.tgz$/);
      expect(entry.integrity, where).toMatch(/^sha512-[A-Za-z0-9+/]{86}==$/);
    }
  });
});
