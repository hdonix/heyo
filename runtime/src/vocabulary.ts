//! The user's own words for dictation: names, jargon and product names the
//! recognizer does not know how to write.
//!
//! The recognizer (SenseVoice) cannot be given words to listen for, so the
//! vocabulary is applied to what it wrote. A listed term is found however it
//! was cased and spaced ("type script", "Typescript" → "TypeScript"), and a
//! `heard as -> written as` line replaces one wording with another
//! ("jason" → "JSON", "杰森" → "JSON").
//!
//! What this cannot do: tell that a word was misheard. A Chinese name
//! written with the wrong characters of the same sound is only fixed if
//! those wrong characters are listed as "heard as"; and since Chinese has no
//! spaces, a "heard as" in Chinese is replaced wherever its characters
//! occur, also inside a longer word.

export interface Vocabulary {
  /// How terms are to be written.
  terms: string[];
  /// What is heard → how it is to be written.
  replacements: [string, string][];
}

/// Reads the setting: one entry per line, `Term` or `heard as -> Term`
/// (`→` and `=>` do as well).
export function parseVocabulary(lines: readonly string[] | undefined): Vocabulary {
  const vocabulary: Vocabulary = { terms: [], replacements: [] };
  for (const line of lines ?? []) {
    if (typeof line !== "string") continue;
    const pair = /^(.*?)(?:->|=>|→)(.*)$/su.exec(line);
    const [heard, written] = pair ? [pair[1].trim(), pair[2].trim()] : ["", line.trim()];
    if (!written) continue;
    if (heard) vocabulary.replacements.push([heard, written]);
    // What it is written as is itself a term: "json" becomes "JSON" too.
    if (!vocabulary.terms.includes(written)) vocabulary.terms.push(written);
  }
  return vocabulary;
}

const SPACE = /[\s\-‐‑]/u;
/// Joins the parts of a name written as one word: "node.js", "snake_case".
const JOINER = /[._]/u;
/// A letter or digit of a script that puts spaces between words. Chinese,
/// Japanese and Korean text does not, so there a word may start anywhere.
const SPACED = /[\p{L}\p{N}]/u;
const UNSPACED = /[\p{Script=Han}\p{Script=Hiragana}\p{Script=Katakana}\p{Script=Hangul}]/u;
const wordy = (char: string | undefined) => char !== undefined && SPACED.test(char) && !UNSPACED.test(char);

/// What a wording is recognized by: its letters, lowercase, without spaces,
/// hyphens or the dots and underscores inside it.
function key(text: string) {
  let out = "";
  for (const char of text) if (!SPACE.test(char) && !JOINER.test(char)) out += char.toLowerCase();
  return out;
}

export type Corrector = (text: string) => string;

/// Builds the correction pass for a vocabulary. Building is the costly part
/// and is done once; correcting a sentence then takes time that depends on
/// the sentence, not on how many terms there are.
export function corrector(vocabulary: Vocabulary): Corrector {
  const written = new Map<string, string>();
  for (const term of vocabulary.terms) {
    const k = key(term);
    if (k && !written.has(k)) written.set(k, term);
  }
  // A replacement wins over a term that reads the same.
  for (const [heard, term] of vocabulary.replacements) {
    const k = key(heard);
    if (k) written.set(k, term);
  }
  if (!written.size) return (text) => text;
  // Longest first: "Visual Studio Code" before "Visual Studio".
  const lengths = [...new Set([...written.keys()].map((k) => k.length))].sort((a, b) => b - a);
  const firsts = new Set([...written.keys()].map((k) => k[0]));

  return (text) => {
    // The text as its key would be, and where each piece of it came from.
    const chars = [...text];
    let skeleton = "";
    /// For each UTF-16 unit of `skeleton`, the index in `chars` it stands for.
    const from: number[] = [];
    for (let i = 0; i < chars.length; i++) {
      const char = chars[i];
      if (SPACE.test(char)) continue;
      if (JOINER.test(char) && wordy(chars[i - 1]) && wordy(chars[i + 1])) continue;
      const lower = char.toLowerCase();
      skeleton += lower;
      for (let unit = 0; unit < lower.length; unit++) from.push(i);
    }

    let out = "";
    /// Up to where `chars` has been written out.
    let done = 0;
    for (let at = 0; at < skeleton.length; ) {
      const start = from[at];
      // Not in the middle of a character, nor of a word.
      const begins = (at === 0 || from[at - 1] !== start) && firsts.has(skeleton[at]) && !(wordy(chars[start]) && wordy(chars[start - 1]));
      let matched = 0;
      if (begins) {
        for (const length of lengths) {
          if (at + length > skeleton.length) continue;
          const end = from[at + length - 1];
          if (at + length < skeleton.length && from[at + length] === end) continue;
          if (wordy(chars[end]) && wordy(chars[end + 1])) continue;
          const term = written.get(skeleton.slice(at, at + length));
          if (term === undefined) continue;
          out += chars.slice(done, start).join("") + term;
          done = end + 1;
          matched = length;
          break;
        }
      }
      at += matched || 1;
    }
    return out + chars.slice(done).join("");
  };
}

/// The vocabulary as the polishing model is told it; empty without one.
export function vocabularyPrompt(vocabulary: Vocabulary, limit = 400): string {
  if (!vocabulary.terms.length) return "";
  const heard = new Map<string, string[]>();
  for (const [from, term] of vocabulary.replacements) heard.set(term, [...(heard.get(term) ?? []), from]);
  const lines = vocabulary.terms.slice(0, limit).map((term) => {
    const misheard = heard.get(term);
    return misheard ? `- ${term} (may appear as: ${misheard.join(", ")})` : `- ${term}`;
  });
  return `\n\nThe speaker uses the names and terms below. Where the transcript has one of them, or something that sounds like one where it would make sense, write it exactly as listed. Do not insert them where they were not said.\n${lines.join("\n")}`;
}
