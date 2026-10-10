import { expect, test } from "vitest";

import en from "emojibase-data/en/compact.json";

import { isGlyph } from "./glyph";

const all = en.flatMap((e) => [e.unicode, ...(e.skins ?? []).map((s) => s.unicode)]);

test("every emoji is a glyph, however long its sequence", () => {
  expect(all.length).toBeGreaterThan(3000);
  expect(all.filter((char) => !isGlyph(char))).toEqual([]);
});

test("the list has the long sequences that used to be taken for image names", () => {
  // What the rule was: no more than four code points.
  const long = en.filter((e) => [...e.unicode].length > 4);
  expect(long.length).toBeGreaterThan(50);
  expect(long.every((e) => isGlyph(e.unicode))).toBe(true);
});

test("short symbols are glyphs", () => {
  for (const char of ["→", "⌘", "½", "™", "A", "#️⃣", "🏴󠁧󠁢󠁷󠁬󠁳󠁿", "👨‍👩‍👧‍👦"]) expect(isGlyph(char)).toBe(true);
});

test("names of images are not", () => {
  for (const name of ["icon.png", "command-icon", "/usr/share/icons/a.svg", "https://example.com/a", "data:image/png;base64,AA", "list-icon@dark", "12345", ""]) {
    expect(isGlyph(name)).toBe(false);
  }
});
