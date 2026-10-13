// The language Heyo speaks, for what the web view and the runtime say
// themselves. The text is written in English where it is said, and the
// English is the key (the host's side is `src-tauri/src/i18n.rs`):
//
//   t("Search for actions…")
//   t("Running {0}…", title)
//   tc("verb", "Open")
//   tn("{n} entry", "{n} entries", count)
//
// The text is a literal, for `pnpm i18n` to gather (`scripts/i18n.mjs`).
// Which language it is said in is the host's to decide: it hands over that
// language's translations (`speak`), at the start and whenever it changes.

import type { Translations } from "./bindings/Translations";

export type { Translations };

let spoken: Translations = { language: "en", plural: [], messages: {} };
const hearers = new Set<() => void>();

/// Speaks that language from now on, and tells whoever asked to hear.
export function speak(translations: Translations) {
  spoken = translations;
  for (const hear of hearers) hear();
}

/// The code of the language spoken: `en`, `zh_CN`.
export function language() {
  return spoken.language;
}

/// Calls `hear` whenever another language is spoken, until the function
/// returned is called.
export function onLanguage(hear: () => void) {
  hearers.add(hear);
  return () => void hearers.delete(hear);
}

/// Fills a text's placeholders: `{}` is the next argument, `{0}` the one
/// at that place, `{n}` the number. One there is nothing for stays.
function fill(text: string, args: unknown[], n?: number) {
  let next = 0;
  return text.replace(/\{(\d*|n)\}/g, (whole, name: string) => {
    if (name === "n") return n === undefined ? whole : String(n);
    const place = name === "" ? next++ : Number(name);
    return place < args.length ? String(args[place]) : whole;
  });
}

const forms = (text: string, context?: string) => spoken.messages[context ? `${context}\u0004${text}` : text];

/// That text in the language spoken, its placeholders filled.
export function t(text: string, ...args: unknown[]) {
  return fill(forms(text)?.[0] ?? text, args);
}

/// The same, where the English alone does not say which of its meanings
/// it is.
export function tc(context: string, text: string, ...args: unknown[]) {
  return fill(forms(text, context)?.[0] ?? text, args);
}

/// The text for `n` things, `{n}` being the number.
export function tn(one: string, many: string, n: number, ...args: unknown[]) {
  // The host writes the forms out up to 199; from there on a number takes
  // the form of 100 and its last two digits.
  const form = spoken.plural[n < spoken.plural.length ? n : 100 + (n % 100)] ?? Number(n !== 1);
  return fill(forms(one)?.[form] ?? (n === 1 ? one : many), args, n);
}
