//! Writing search cases to measure the clipboard search by: for an entry of
//! the history, the queries a person might type some days later to find it
//! again. A model writes them, and only one on this computer: an entry is
//! whatever was copied.
//!
//! What a model writes for this is too long, too tidy and too close to the
//! words of the entry, where real queries are a word or two. So the prompt
//! asks for short ones of named types, and what comes back is checked
//! against the entry rather than trusted.

import { classifyModelAddress } from "../../src/modelAddress";
import { type AiSettings, streamLocal } from "./llm";

/// An entry to write queries for.
export interface CaseEntry {
  id: number;
  text: string;
  /// The application it was copied in, if known.
  app?: string;
  /// When, as a person would say it at the time of asking: "yesterday", "3 days ago".
  copied: string;
}

export interface CaseRequest {
  ai: AiSettings;
  entries: CaseEntry[];
}

/// The kinds of query, which the measurements are reported by.
export const TYPES = {
  keyword: "one or two words (or two to four Chinese characters) that occur in the text",
  paraphrase: "the same thing said in other words, using none of the text's own distinctive words",
  description: "what kind of thing it is rather than what it says, as in “the ssh command for the test server”",
  "cross-language": "what it is about, in Chinese if the text is English and in English if the text is Chinese",
  partial: "a vague memory of one part of it, a few words, perhaps slightly wrong",
  exact: "a fragment of an address, a number or an identifier in the text, copied exactly (only if the text has one)",
  hinted: "a word of the text together with when it was copied or which application it came from",
} as const;

export type CaseType = keyof typeof TYPES;

export interface Case {
  type: CaseType;
  query: string;
}

/// Longer than this and it is not something typed into a search field.
const MAX_WORDS = 7;
const MAX_CHARACTERS = 40;
/// A model that reads this much has read enough to describe it.
const MAX_TEXT = 1500;

const SYSTEM = `You help test a search over a person's clipboard history. Given something they copied, write the searches they might type a few days later to find it again. People type very little: one to four words, lower case, no punctuation, no quotes, never a whole sentence.

Write one search of each of these types that makes sense for the text, as JSON lines of the form {"type": "...", "query": "..."} and nothing else:
${Object.entries(TYPES)
  .map(([type, what]) => `- ${type}: ${what}`)
  .join("\n")}`;

export function prompt(entry: CaseEntry) {
  const source = entry.app ? ` in ${entry.app}` : "";
  return `(Copied ${entry.copied}${source}.)\n${entry.text.slice(0, MAX_TEXT)}`;
}

const HAN = /\p{Script=Han}/u;

/// The words of a text that say what it is about: Latin words of four
/// letters or more, and every pair of neighbouring Chinese characters.
function distinctive(text: string): Set<string> {
  const found = new Set<string>();
  for (const word of text.toLowerCase().match(/[\p{L}\p{N}]+/gu) ?? []) {
    if (!HAN.test(word)) {
      if (word.length >= 4) found.add(word);
      continue;
    }
    const characters = [...word];
    for (let i = 0; i + 1 < characters.length; i++) found.add(characters[i] + characters[i + 1]);
  }
  return found;
}

/// Whether a query the model wrote is one of its type for this text. A
/// paraphrase that repeats the text's words measures the keyword search
/// again, and an "exact" fragment that is not in the text finds nothing by
/// design: both would make the numbers lie.
export function acceptable(candidate: Case, text: string): boolean {
  const query = candidate.query;
  if (!query || query.length > MAX_CHARACTERS || query.split(/\s+/).length > MAX_WORDS || query.includes("\n")) return false;
  const lower = text.toLowerCase();
  switch (candidate.type) {
    case "exact":
      return lower.includes(query.toLowerCase());
    case "keyword":
      return query
        .toLowerCase()
        .split(/\s+/)
        .every((word) => lower.includes(word));
    case "paraphrase":
    case "description":
    case "cross-language": {
      const own = distinctive(text);
      return ![...distinctive(query)].some((word) => own.has(word));
    }
    default:
      return true;
  }
}

/// The cases in a model's answer: the lines that are what was asked for,
/// one of a type, whatever else it wrote around them.
export function parse(answer: string, text: string): Case[] {
  const cases: Case[] = [];
  for (const line of answer.split("\n")) {
    const json = line.match(/\{.*\}/)?.[0];
    if (!json) continue;
    let candidate: Case;
    try {
      candidate = JSON.parse(json);
    } catch {
      continue;
    }
    if (typeof candidate.query !== "string" || !(candidate.type in TYPES)) continue;
    candidate = { type: candidate.type, query: candidate.query.trim().replace(/^["“”']+|["“”']+$/g, "") };
    if (cases.some((other) => other.type === candidate.type || other.query === candidate.query)) continue;
    if (acceptable(candidate, text)) cases.push(candidate);
  }
  return cases;
}

/// Refuses any model that is not on this computer, whatever the settings say.
export function localOnly(ai: AiSettings) {
  if (classifyModelAddress(ai.localUrl).where !== "this-computer") {
    throw new Error(`Cases are written from what you copied, so only a model on this computer may write them, and ${ai.localUrl || "no address"} is not one. Set a local model in Heyo Settings → Advanced.`);
  }
}

export async function casesFor(entry: CaseEntry, ai: AiSettings): Promise<Case[]> {
  localOnly(ai);
  const answer = await streamLocal({ settings: { ...ai, provider: "local" }, system: SYSTEM, prompt: prompt(entry) });
  return parse(answer, entry.text);
}
