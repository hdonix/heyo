// The language Heyo speaks, for what the web view and the runtime say
// themselves. The text is written in English where it is said, and the
// English is the key (the host's side is `src-tauri/src/i18n.rs`):
//
//   t("Search for actions…")
//   t("Running {0}…", title)
//   tc("verb", "Open")
//   tn("{n} entry", "{n} entries", count)
//
// A text that is marked where it is written and said elsewhere, as the
// host's names of Heyo's own commands are, is said with `translate`.
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

/// The language spoken as dates and numbers are written in it (`zh-CN`):
/// what `toLocaleString` and `Intl` are handed, in place of the system's.
export function locale() {
  return spoken.language.replace("_", "-");
}

/// Calls `hear` whenever another language is spoken, until the function
/// returned is called.
export function onLanguage(hear: () => void) {
  hearers.add(hear);
  return () => void hearers.delete(hear);
}

/// A whole number as the language spoken writes it: its thousands apart,
/// by what that language parts them with. (As the host writes one:
/// `number` in `src-tauri/src/i18n.rs`, by the same message.)
export function number(n: number): string {
  return String(n).replace(/\B(?=(\d{3})+$)/g, tc("between a number's thousands", ","));
}

/// Fills a text's placeholders: `{}` is the next argument, `{0}` the one
/// at that place, `{n}` the number, written as `number` writes one. One
/// there is nothing for stays.
function fill(text: string, args: unknown[], n?: number): string {
  let next = 0;
  return text.replace(/\{(\d*|n)\}/g, (whole, name: string) => {
    if (name === "n") return n === undefined ? whole : number(n);
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

/// A text that was marked for translating where it was written, which is
/// not here: in the language spoken, or itself where that has none for it.
export function translate(text: string) {
  return forms(text)?.[0] ?? text;
}

/// The text for `n` things, `{n}` being the number.
export function tn(one: string, many: string, n: number, ...args: unknown[]) {
  // The host writes the forms out up to 199; from there on a number takes
  // the form of 100 and its last two digits.
  const form = spoken.plural[n < spoken.plural.length ? n : 100 + (n % 100)] ?? Number(n !== 1);
  return fill(forms(one)?.[form] ?? (n === 1 ? one : many), args, n);
}
