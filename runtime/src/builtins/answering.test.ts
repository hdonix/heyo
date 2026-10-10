import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { afterEach, beforeEach, expect, test } from "vitest";

import { answersFor, type Ask } from "./answering";
import { answered, asked, conversationsIn, newConversation, renamed, type Conversations } from "./conversations";

/// A model that answers when the test says so.
function model() {
  const calls: (Parameters<Ask>[0] & { finish(answer: string): void; fail(error: Error): void })[] = [];
  const ask: Ask = (options) =>
    new Promise<string>((resolve, reject) => {
      calls.push({ ...options, finish: resolve, fail: reject });
      // As the real ones do: stopping is failing.
      options.signal.addEventListener("abort", () => reject(new Error("aborted")));
    });
  return { ask, calls };
}

let directory: string;
let store: Conversations;
beforeEach(() => {
  directory = fs.mkdtempSync(path.join(os.tmpdir(), "heyo-answering-"));
  store = conversationsIn(directory);
});
afterEach(() => fs.rmSync(directory, { recursive: true, force: true }));

const describe = (error: unknown) => `Failed: ${(error as Error).message}`;
const question = (text = "What is 2 + 2?") => {
  const conversation = asked(newConversation(1000), text, 1000);
  store.save(conversation);
  return conversation;
};

test("the answer is kept in the conversation when the model is done", async () => {
  const { ask, calls } = model();
  const answers = answersFor(store, ask, describe, () => 2000);
  const conversation = question();
  const answer = answers.start(conversation);
  expect(answers.of(conversation.id)).toBe(answer);
  await Promise.resolve();

  calls[0].onText("Fo");
  expect(answer.text).toBe("Fo");
  // Nothing is written while it grows.
  expect(store.get(conversation.id)?.messages).toHaveLength(1);

  calls[0].finish("  Four.\n");
  const outcome = await answer.settled;
  expect(outcome.error).toBeUndefined();
  expect(outcome.conversation?.messages.at(-1)).toEqual({ role: "assistant", content: "Four.", at: 2000 });
  expect(store.get(conversation.id)).toEqual(outcome.conversation);
  expect(answers.of(conversation.id)).toBeUndefined();
});

test("the model is sent the question, and what was said before it", async () => {
  const { ask, calls } = model();
  const answers = answersFor(store, ask, describe);
  const earlier = answered(question("first"), "1", 1001);
  // One the model never answered is not sent again.
  const conversation = asked(asked(earlier, "lost", 1002), "second", 1003);
  answers.start(conversation);
  await Promise.resolve();
  expect(calls[0].prompt).toBe("second");
  expect(calls[0].history).toEqual([
    { role: "user", content: "first" },
    { role: "assistant", content: "1" },
  ]);
});

test("a conversation has one answer written at a time", async () => {
  const { ask, calls } = model();
  const answers = answersFor(store, ask, describe);
  const conversation = question();
  const first = answers.start(conversation);
  expect(answers.start(conversation)).toBe(first);
  await Promise.resolve();
  expect(calls).toHaveLength(1);
  // Another conversation is answered alongside.
  const other = asked(newConversation(3000), "other", 3000);
  expect(answers.start(other)).not.toBe(first);
  // And there must be a question to answer.
  expect(() => answers.start(answered(asked(newConversation(4000), "q", 4000), "a", 4001))).toThrow();
});

test("stopped, what was written is kept and marked as cut short", async () => {
  const { ask, calls } = model();
  const answers = answersFor(store, ask, describe, () => 2000);
  const conversation = question();
  const answer = answers.start(conversation);
  await Promise.resolve();
  calls[0].onText("Four, beca");
  answers.stop(conversation.id);
  const outcome = await answer.settled;
  // Stopping is not a failure to tell the user of.
  expect(outcome.error).toBeUndefined();
  expect(store.get(conversation.id)?.messages.at(-1)).toEqual({ role: "assistant", content: "Four, beca", at: 2000, partial: true });
});

test("a failure is told, and the question stays to be tried again", async () => {
  const { ask, calls } = model();
  const answers = answersFor(store, ask, describe);
  const conversation = question();
  const answer = answers.start(conversation);
  await Promise.resolve();
  calls[0].fail(new Error("no key"));
  const outcome = await answer.settled;
  expect(outcome.error).toBe("Failed: no key");
  expect(outcome.conversation).toEqual(conversation);
  expect(store.get(conversation.id)).toEqual(conversation);
  // Free to be answered again.
  expect(answers.of(conversation.id)).toBeUndefined();
  expect(answers.start(conversation)).not.toBe(answer);
});

test("a model that cannot even be asked fails the same way", async () => {
  const answers = answersFor(
    store,
    () => {
      throw new Error("no settings");
    },
    describe,
  );
  const outcome = await answers.start(question()).settled;
  expect(outcome.error).toBe("Failed: no settings");
});

test("an answer goes into the conversation as it is kept by then", async () => {
  const { ask, calls } = model();
  const answers = answersFor(store, ask, describe);
  const conversation = question();
  const answer = answers.start(conversation);
  await Promise.resolve();
  store.save(renamed(conversation, "Sums"));
  calls[0].finish("Four.");
  const outcome = await answer.settled;
  expect(outcome.conversation).toMatchObject({ title: "Sums", renamed: true });
  expect(outcome.conversation?.messages).toHaveLength(2);
});

test("nothing is written for a conversation deleted meanwhile", async () => {
  const { ask, calls } = model();
  const answers = answersFor(store, ask, describe);
  const [one, two] = [question("one"), asked(newConversation(5000), "two", 5000)];
  store.save(two);
  const [first, second] = [answers.start(one), answers.start(two)];
  await Promise.resolve();
  calls[0].onText("partly");

  answers.drop(one.id);
  store.remove(one.id);
  expect(calls[0].signal.aborted).toBe(true);
  expect(calls[1].signal.aborted).toBe(false);
  expect(await first.settled).toEqual({});
  expect(store.get(one.id)).toBeUndefined();

  // All of them, when all conversations are deleted.
  answers.drop();
  store.removeAll();
  expect(await second.settled).toEqual({});
  expect(store.list()).toEqual([]);
});
