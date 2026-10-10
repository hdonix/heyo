//! The conversations with the AI model that Heyo keeps, so that one can be
//! come back to: what a conversation is, how it grows, and the files they
//! are kept in.
//!
//! Each conversation is one JSON file, readable by the user alone (they hold
//! whatever was asked). Nothing else knows of them: deleting the file is
//! deleting the conversation.

import { randomBytes } from "node:crypto";
import fs from "node:fs";
import path from "node:path";

import { readJson, writeJson } from "../context";
import type { Message as Turn } from "../heyo/ai";

export interface Message {
  role: "user" | "assistant";
  content: string;
  /// When it was sent or received (ms since the epoch).
  at: number;
  /// An answer cut short: it was stopped, or the model failed part-way.
  partial?: boolean;
}

export interface Conversation {
  id: string;
  /// From the first question, until the user gives another.
  title: string;
  renamed?: boolean;
  createdAt: number;
  updatedAt: number;
  messages: Message[];
}

export const UNTITLED = "New Chat";
const TITLE_LENGTH = 60;

/// A title for a conversation that starts with `question`: its first line,
/// cut at a word when it is long.
export function titleOf(question: string): string {
  const line = question
    .split("\n")
    .map((l) => l.replace(/\s+/g, " ").trim())
    // Not a line of nothing but markup (a code fence, a rule).
    .find((l) => /[^\s`#>*_~=-]/.test(l));
  if (!line) return UNTITLED;
  // Counted in characters, not UTF-16 units: an emoji is not cut in half.
  const characters = Array.from(line);
  if (characters.length <= TITLE_LENGTH) return line;
  const cut = characters.slice(0, TITLE_LENGTH).join("");
  const space = cut.lastIndexOf(" ");
  // Text without spaces (Chinese, a long address) is cut where it is.
  return `${(space > TITLE_LENGTH / 2 ? cut.slice(0, space) : cut).replace(/[\s,;:.!?，。；：、]+$/u, "")}…`;
}

export function newConversation(now: number): Conversation {
  // Sorts by time of creation; the random part keeps two of one moment apart.
  const id = `${now.toString(36)}-${randomBytes(4).toString("hex")}`;
  return { id, title: UNTITLED, createdAt: now, updatedAt: now, messages: [] };
}

/// The conversation with a question added. The first question names it.
export function asked(conversation: Conversation, question: string, now: number): Conversation {
  const first = !conversation.messages.some((m) => m.role === "user");
  return {
    ...conversation,
    title: first && !conversation.renamed ? titleOf(question) : conversation.title,
    updatedAt: now,
    messages: [...conversation.messages, { role: "user", content: question, at: now }],
  };
}

/// The conversation with the answer to its last question added.
export function answered(conversation: Conversation, answer: string, now: number, partial = false): Conversation {
  const message: Message = { role: "assistant", content: answer, at: now, ...(partial ? { partial } : {}) };
  return { ...conversation, updatedAt: now, messages: [...conversation.messages, message] };
}

/// The conversation as it was before its last question was answered, to
/// have it answered again.
export function unanswered(conversation: Conversation): Conversation {
  const { messages } = conversation;
  return messages.at(-1)?.role === "assistant" ? { ...conversation, messages: messages.slice(0, -1) } : conversation;
}

export function renamed(conversation: Conversation, title: string): Conversation {
  const given = title.replace(/\s+/g, " ").trim();
  return given ? { ...conversation, title: given, renamed: true } : conversation;
}

/// A question and what was answered, if anything was yet.
export interface Exchange {
  question: Message;
  answer?: Message;
}

export function exchanges(messages: Message[]): Exchange[] {
  const out: Exchange[] = [];
  for (const message of messages) {
    const last = out.at(-1);
    if (message.role === "user") out.push({ question: message });
    else if (last && !last.answer) last.answer = message;
  }
  return out;
}

/// What the model is sent as the conversation so far: the questions that
/// were answered, each with its answer. A question that never got one (the
/// model failed) is left out, so that the turns alternate, which some
/// models insist on.
export function turnsOf(messages: Message[]): Turn[] {
  return exchanges(messages)
    .filter((e) => e.answer?.content.trim())
    .flatMap((e) => [
      { role: "user" as const, content: e.question.content },
      { role: "assistant" as const, content: e.answer!.content },
    ]);
}

/// One exchange as Markdown: the question quoted, and under it the answer,
/// as far as there is one. `writing` is the answer so far while the model is
/// at it; `error` why it failed.
export function exchangeMarkdown(exchange: Exchange, writing: string | null = null, error?: string): string {
  const question = `> ${exchange.question.content.trim().replace(/\n/g, "\n> ")}`;
  const parts = [question];
  if (exchange.answer) {
    parts.push(exchange.answer.content);
    if (exchange.answer.partial) parts.push("*The answer was cut short.*");
  } else if (writing !== null) parts.push(writing || "…");
  else if (!error) parts.push("*No answer yet.*");
  if (error) parts.push(`**Could not get an answer**\n\n${error}`);
  return parts.join("\n\n");
}

export function lastAnswer(conversation: Conversation): string | undefined {
  return conversation.messages.findLast((m) => m.role === "assistant" && m.content.trim())?.content;
}

/// The whole conversation as a Markdown document.
export function asMarkdown(conversation: Conversation): string {
  const parts = conversation.messages.map((m) => `## ${m.role === "user" ? "You" : "AI"}\n\n${m.content.trim()}`);
  return [`# ${conversation.title}`, ...parts].join("\n\n") + "\n";
}

/// The conversations a search finds: every word of it occurs in the title
/// or in something that was said. In the order given.
export function found(conversations: Conversation[], query: string): Conversation[] {
  const words = query.toLowerCase().split(/\s+/).filter(Boolean);
  if (!words.length) return conversations;
  return conversations.filter((c) => {
    const text = [c.title, ...c.messages.map((m) => m.content)].join("\n").toLowerCase();
    return words.every((word) => text.includes(word));
  });
}

// --- Files ---------------------------------------------------------------

/// A file read as a conversation, or nothing if it is not one.
function read(file: string): Conversation | undefined {
  const stored = readJson<Partial<Conversation> | null>(file, null);
  if (!stored || typeof stored.id !== "string" || !Array.isArray(stored.messages)) return undefined;
  const messages = stored.messages.filter((m) => (m?.role === "user" || m?.role === "assistant") && typeof m.content === "string");
  const createdAt = Number(stored.createdAt) || 0;
  return {
    id: stored.id,
    title: typeof stored.title === "string" && stored.title ? stored.title : UNTITLED,
    ...(stored.renamed ? { renamed: true } : {}),
    createdAt,
    updatedAt: Number(stored.updatedAt) || createdAt,
    messages,
  };
}

/// The conversations kept in a directory.
export function conversationsIn(directory: string) {
  // An id names a file in the directory and nothing outside it.
  const file = (id: string) => {
    if (!/^[A-Za-z0-9-]+$/.test(id)) throw new Error(`Not a conversation id: ${id}`);
    return path.join(directory, `${id}.json`);
  };
  const files = () => {
    try {
      return fs.readdirSync(directory).filter((name) => name.endsWith(".json"));
    } catch {
      return [];
    }
  };
  return {
    /// All of them, the one last spoken in first.
    list(): Conversation[] {
      return files()
        .flatMap((name) => read(path.join(directory, name)) ?? [])
        .sort((a, b) => b.updatedAt - a.updatedAt || b.createdAt - a.createdAt);
    },
    get(id: string): Conversation | undefined {
      return read(file(id));
    },
    save(conversation: Conversation) {
      fs.mkdirSync(directory, { recursive: true, mode: 0o700 });
      // `writeJson` writes the file whole, and for its owner alone.
      writeJson(file(conversation.id), conversation);
    },
    remove(id: string) {
      fs.rmSync(file(id), { force: true });
    },
    /// Removes every conversation; says how many there were.
    removeAll(): number {
      const all = files();
      for (const name of all) fs.rmSync(path.join(directory, name), { force: true });
      return all.length;
    },
  };
}

export type Conversations = ReturnType<typeof conversationsIn>;
