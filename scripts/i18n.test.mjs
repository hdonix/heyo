// Holds the translations to the sources: the template to what the code
// says, and each language's file to the template. When it fails for the
// template or for a file's messages, `pnpm i18n` writes both anew.

import { readFileSync } from "node:fs";
import { relative } from "node:path";
import { describe, expect, it } from "vitest";

import { extract, forms, key, languages, parse, placeholders, ROOT, sources, TEMPLATE, template } from "./i18n.mjs";

const messages = extract();
const keys = messages.map(key);

describe("the template", () => {
  it("has every string the sources mark, and no other (else: pnpm i18n)", () => {
    expect(readFileSync(TEMPLATE, "utf8")).toBe(template(messages));
  });

  // The reader of the sources walks past comments and strings to find the
  // calls. This looks for them the plain way, a line at a time, so that a
  // call the walk lost its way before is not lost silently.
  it("misses no call a plain search finds", () => {
    const found = new Set(keys);
    for (const { file, text: source } of sources().filter(({ file }) => !file.endsWith(".slint"))) {
      const lines = source.split("\n").filter((line) => !/^\s*(\/\/|\*)/.test(line));
      for (const [, text] of lines.join("\n").matchAll(/(?<![\w.])(?:t|tf|marked)\(\s*"((?:[^"\\]|\\.)*)"/g)) {
        expect(found.has(JSON.parse(`"${text}"`)), `${file}: "${text}"`).toBe(true);
      }
    }
  });

  it("has no text that braces could not be told from placeholders in", () => {
    for (const message of messages) {
      for (const text of [message.text, message.plural ?? ""]) {
        expect(text.replace(/\{(\d*|n)\}/g, ""), text).not.toMatch(/[{}]/);
      }
    }
  });
});

describe.each(languages())("$language", ({ path }) => {
  const { fields, messages: translated } = parse(readFileSync(path, "utf8"));
  const file = relative(ROOT, path);

  it("says what it is", () => {
    expect(Object.keys(fields), file).toEqual(expect.arrayContaining(["Language", "X-Language-Name", "Plural-Forms"]));
  });

  it("has the template's messages, in its order (else: pnpm i18n)", () => {
    expect(translated.map(key)).toEqual(keys);
  });

  it("keeps each text's placeholders in its translation", () => {
    for (const message of translated) {
      const source = placeholders(message.plural ?? message.text);
      message.forms.forEach((form, place) => {
        if (!form) return;
        // A form for one number alone may leave the number out.
        const has = placeholders(form);
        const wanted = message.plural === undefined ? source : source.filter((name) => name !== "n" || has.includes("n"));
        expect(has, `${file}: "${message.text}" [${place}]`).toEqual(wanted);
      });
      if (message.plural !== undefined) expect(message.forms.length, `${file}: "${message.text}"`).toBe(forms(fields));
    }
  });
});
