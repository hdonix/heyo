import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { afterEach, beforeEach, expect, test } from "vitest";

import {
  answered,
  asked,
  asMarkdown,
  conversationsIn,
  exchangeMarkdown,
  exchanges,
  found,
  lastAnswer,
  newConversation,
  renamed,
  titleOf,
  turnsOf,
  unanswered,
  untitled,
  type Conversation,
} from "./conversations";

/// A conversation of questions and answers; `null` is a question left unanswered.
function talk(pairs: [string, string | null][], at = 1000): Conversation {
  let conversation = newConversation(at);
  for (const [question, answer] of pairs) {
    conversation = asked(conversation, question, ++at);
    if (answer !== null) conversation = answered(conversation, answer, ++at);
  }
  return conversation;
}

test("a conversation is named by its first question", () => {
  expect(titleOf("What is the capital of Norway?")).toBe("What is the capital of Norway?");
  expect(titleOf("  how do I\t undo   a commit  \n\nI already pushed it")).toBe("how do I undo a commit");
  // A first line of nothing but markup is not a name.
  expect(titleOf("```\n\nexplain this code\n```")).toBe("explain this code");
  expect(titleOf("   \n ")).toBe(untitled());
});

test("a long first question is cut at a word", () => {
  const title = titleOf("Explain the difference between a mutex and a semaphore, with an example of each in Rust");
  expect(title).toBe("Explain the difference between a mutex and a semaphore…");
  expect(Array.from(title).length).toBeLessThanOrEqual(61);
});

test("text without spaces is cut where it is, never inside a character", () => {
  const title = titleOf("写".repeat(100));
  expect(title).toBe(`${"写".repeat(60)}…`);
  // Two UTF-16 units each: cut by units, the last would be half a character.
  expect(titleOf("😀".repeat(100))).toBe(`${"😀".repeat(60)}…`);
});

test("only the first question names it, and not once the user has", () => {
  let conversation = asked(newConversation(1), "First question", 2);
  expect(conversation.title).toBe("First question");
  conversation = asked(answered(conversation, "An answer", 3), "Second question", 4);
  expect(conversation.title).toBe("First question");
  expect(conversation).toMatchObject({ createdAt: 1, updatedAt: 4 });

  const named = asked(renamed(newConversation(1), "  My   project "), "First question", 2);
  expect(named.title).toBe("My project");
  // An empty name is no name.
  expect(renamed(conversation, "  ").title).toBe("First question");
});

test("an answer belongs to the question before it", () => {
  const conversation = talk([
    ["one", "1"],
    ["two", null],
    ["three", "3"],
  ]);
  expect(exchanges(conversation.messages).map((e) => [e.question.content, e.answer?.content])).toEqual([
    ["one", "1"],
    ["two", undefined],
    ["three", "3"],
  ]);
});

test("the model is sent the answered questions, in turns", () => {
  const conversation = talk([
    ["one", "1"],
    ["two", null],
    ["three", "  "],
    ["four", "4"],
  ]);
  expect(turnsOf(conversation.messages)).toEqual([
    { role: "user", content: "one" },
    { role: "assistant", content: "1" },
    { role: "user", content: "four" },
    { role: "assistant", content: "4" },
  ]);
  expect(turnsOf([])).toEqual([]);
});

test("answering again starts from the question", () => {
  const conversation = talk([
    ["one", "1"],
    ["two", "2"],
  ]);
  expect(unanswered(conversation).messages.map((m) => m.content)).toEqual(["one", "1", "two"]);
  // Nothing to take back from a question still open.
  const open = talk([["one", null]]);
  expect(unanswered(open)).toBe(open);
});

test("an exchange reads as the question quoted over its answer", () => {
  const [exchange] = exchanges(talk([["line one\nline two", "The answer."]]).messages);
  expect(exchangeMarkdown(exchange)).toBe("> line one\n> line two\n\nThe answer.");

  const [open] = exchanges(talk([["why?", null]]).messages);
  expect(exchangeMarkdown(open, "")).toBe("> why?\n\n…");
  expect(exchangeMarkdown(open, "Because")).toBe("> why?\n\nBecause");
  expect(exchangeMarkdown(open, null, "No key is set.")).toBe("> why?\n\n**Could not get an answer**\n\nNo key is set.");
  expect(exchangeMarkdown(open)).toContain("No answer yet");

  const cut = exchanges(answered(talk([["why?", null]]), "Becau", 9, true).messages)[0];
  expect(exchangeMarkdown(cut)).toContain("cut short");
});

test("the whole conversation copies as a Markdown document", () => {
  const conversation = talk([
    ["What is 2 + 2?", "4"],
    ["And doubled?", "8"],
  ]);
  expect(asMarkdown(conversation)).toBe("# What is 2 + 2?\n\n## You\n\nWhat is 2 + 2?\n\n## AI\n\n4\n\n## You\n\nAnd doubled?\n\n## AI\n\n8\n");
  expect(lastAnswer(conversation)).toBe("8");
  expect(lastAnswer(talk([["hello", null]]))).toBeUndefined();
});

test("a search finds what was said as well as what it is called", () => {
  const rust = talk([["Borrow checker basics", "Ownership moves; references borrow."]]);
  const bread = talk([["Sourdough schedule", "Feed the starter the night before."]]);
  expect(found([rust, bread], "")).toEqual([rust, bread]);
  expect(found([rust, bread], "sourdough")).toEqual([bread]);
  expect(found([rust, bread], "REFERENCES borrow")).toEqual([rust]);
  // Every word must be there.
  expect(found([rust, bread], "starter ownership")).toEqual([]);
});

// --- Files ---------------------------------------------------------------

let directory: string;
beforeEach(() => {
  directory = path.join(fs.mkdtempSync(path.join(os.tmpdir(), "heyo-conversations-")), "conversations");
});
afterEach(() => fs.rmSync(path.dirname(directory), { recursive: true, force: true }));

test("a conversation saved is read back as it was", () => {
  const store = conversationsIn(directory);
  expect(store.list()).toEqual([]);
  const conversation = talk([["one", "1"]]);
  store.save(conversation);
  expect(store.get(conversation.id)).toEqual(conversation);
  expect(store.list()).toEqual([conversation]);
  expect(store.get("0-none")).toBeUndefined();
});

test("conversations are listed with the one last spoken in first", () => {
  const store = conversationsIn(directory);
  const old = talk([["old", "1"]], 1000);
  const recent = talk([["recent", "2"]], 5000);
  const revived = asked(talk([["revived", "3"]], 2000), "again", 9000);
  for (const conversation of [old, recent, revived]) store.save(conversation);
  expect(store.list().map((c) => c.title)).toEqual(["revived", "recent", "old"]);
});

test.skipIf(process.platform === "win32")("conversations are files for their owner alone", () => {
  const store = conversationsIn(directory);
  const conversation = talk([["private", "yes"]]);
  store.save(conversation);
  // Saved over, as each new message does: still private.
  store.save(answered(asked(conversation, "more", 2000), "ok", 2001));
  const file = path.join(directory, `${conversation.id}.json`);
  expect(fs.statSync(file).mode & 0o777).toBe(0o600);
  expect(fs.statSync(directory).mode & 0o777).toBe(0o700);
  // Nothing left over from writing.
  expect(fs.readdirSync(directory)).toEqual([`${conversation.id}.json`]);
});

test("one is deleted, or all", () => {
  const store = conversationsIn(directory);
  const [a, b, c] = [talk([["a", "1"]]), talk([["b", "2"]]), talk([["c", "3"]])];
  for (const conversation of [a, b, c]) store.save(conversation);
  store.remove(b.id);
  expect(store.list().map((x) => x.id).sort()).toEqual([a.id, c.id].sort());
  // Deleting what is gone already is not an error.
  store.remove(b.id);
  expect(store.removeAll()).toBe(2);
  expect(store.list()).toEqual([]);
  expect(fs.readdirSync(directory)).toEqual([]);
  // Nor is deleting all of nothing.
  expect(conversationsIn(path.join(directory, "absent")).removeAll()).toBe(0);
});

test("deleting all leaves alone what is not a conversation", () => {
  const store = conversationsIn(directory);
  store.save(talk([["a", "1"]]));
  fs.writeFileSync(path.join(directory, "notes.txt"), "mine");
  store.removeAll();
  expect(fs.readdirSync(directory)).toEqual(["notes.txt"]);
});

test("an id cannot name a file outside the directory", () => {
  const store = conversationsIn(directory);
  expect(() => store.get("../../etc/passwd")).toThrow();
  expect(() => store.remove("..")).toThrow();
  expect(() => store.save({ ...talk([["a", "1"]]), id: "a/b" })).toThrow();
});

test("a file that is not a conversation is passed over", () => {
  const store = conversationsIn(directory);
  const good = talk([["good", "1"]]);
  store.save(good);
  fs.writeFileSync(path.join(directory, "broken.json"), "{ not json");
  fs.writeFileSync(path.join(directory, "other.json"), JSON.stringify({ hello: "world" }));
  // Messages of unknown kinds are dropped, the rest kept.
  const odd = { id: "odd", title: "Odd", createdAt: 5, updatedAt: 5, messages: [{ role: "system", content: "x" }, { role: "user", content: "kept", at: 5 }] };
  fs.writeFileSync(path.join(directory, "odd.json"), JSON.stringify(odd));
  expect(store.list().map((c) => c.id).sort()).toEqual([good.id, "odd"].sort());
  expect(store.get("odd")?.messages).toEqual([{ role: "user", content: "kept", at: 5 }]);
});
