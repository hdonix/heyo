// `AI.ask`: what the model is given for a conversation, a system prompt and
// text to work on; how the answer comes while it is written; how it fails.
import { afterEach, expect, test, vi } from "vitest";

// The model: records what it was asked, and answers when the test says so.
const model = vi.hoisted(() => ({
  calls: [] as { options: any; finish(answer: string): void; fail(error: unknown): void }[],
}));
vi.mock("../ai", async () => ({
  ...(await vi.importActual<typeof import("../ai")>("../ai")),
  askModel: (options: any) =>
    new Promise<string>((finish, fail) => {
      options.signal?.addEventListener("abort", () => fail(new Error("aborted")));
      model.calls.push({ options, finish, fail });
    }),
}));

import { answers, extension, requests } from "../testing";

import { runAs, type Current } from "../context";
import { AI, type AskOptions, type Message } from "./ai";
import { HeyoError } from "./error";
import { gained } from "./members";

const as: Current = { extension: extension("notes", {}), command: "main", launchType: "userInitiated" };
const asked = () => requests.filter((r) => r.method === "ext.ai.provider").length;
const last = () => model.calls.at(-1)!;
// Until the model has been asked once more: the way there is some awaits
// long, and how long those take is the machine's business.
let seen = 0;
const tick = () =>
  vi.waitFor(() => {
    expect(model.calls.length).toBeGreaterThan(seen);
    seen = model.calls.length;
  });

afterEach(() => {
  seen = 0;
  model.calls.length = 0;
  requests.length = 0;
});

test("asked as Raycast's API is asked, the model gets the prompt and nothing of Heyo's is asked for", async () => {
  const answer = runAs(as, () => AI.ask("Why?", { creativity: "low" }));
  await tick();
  expect(asked()).toBe(0);
  expect(last().options).toMatchObject({ prompt: "Why?", history: [], temperature: 0.5 });
  expect(last().options.system).toMatch(/helpful assistant/);
  last().finish("Because.");
  expect(await answer).toBe("Because.");
});

test("each option Heyo adds has the host asked whether the extension may use it", async () => {
  answers["ext.ai.provider"] = () => ({ kind: "local", model: "qwen3", local: true });
  const samples: Record<string, AskOptions> = {
    system: { system: "Be brief." },
    images: { images: ["/tmp/shot.png"] },
    messages: { messages: [] },
    material: { material: true },
  };
  // The options the declaration says make it Heyo's, which the developer
  // tools go by (`heyoGainsUsed` in dev/check.ts).
  expect(Object.keys(samples)).toEqual(gained.AI.ask.given);
  for (const options of Object.values(samples)) {
    requests.length = 0;
    runAs(as, () => AI.ask("Why?", options)).catch(() => {});
    await vi.waitFor(() => expect(asked()).toBe(1));
  }
});

test("a conversation goes to the model in order, after the system prompt, and the prompt is its next message", async () => {
  answers["ext.ai.provider"] = () => ({ kind: "local", model: "qwen3", local: true });
  const messages: Message[] = [
    { role: "user", content: "One?" },
    { role: "assistant", content: "1." },
    { role: "user", content: "Two?" },
    { role: "assistant", content: "2." },
  ];
  const answer = runAs(as, () => AI.ask("Three?", { system: "Count.", messages: messages.map((m) => ({ ...m, extra: true }) as Message) }));
  await tick();
  // What Heyo adds is used, so the extension's leave to use it is checked.
  expect(asked()).toBe(1);
  expect(last().options).toMatchObject({ system: "Count.", prompt: "Three?", history: messages });
  // Only the turns, in the shape the model takes: nothing else of what was passed.
  expect(Object.keys(last().options.history[0])).toEqual(["role", "content"]);
  last().finish("3.");
  expect(await answer).toBe("3.");
});

test("the answer comes whole as it grows and in pieces, to whoever asked for either", async () => {
  const whole: string[] = [];
  const pieces: string[] = [];
  const answer = runAs(as, () => AI.ask("Go", { onText: (text) => whole.push(text) }));
  answer.on("data", (chunk) => pieces.push(chunk));
  await tick();
  last().options.onText("Ray");
  last().options.onText("Rayleigh scat");
  last().options.onText("Rayleigh scat");
  last().options.onText("Rayleigh scattering.");
  expect(whole).toEqual(["Ray", "Rayleigh scat", "Rayleigh scat", "Rayleigh scattering."]);
  expect(pieces).toEqual(["Ray", "leigh scat", "tering."]);
  last().finish("Rayleigh scattering.");
  expect(await answer).toBe("Rayleigh scattering.");
});

test("text to work on is material, not a conversation", async () => {
  answers["ext.ai.provider"] = () => ({ kind: "claude", model: "", local: false });
  const answer = runAs(as, () => AI.ask("hello world", { system: "Translate it.", material: true }));
  await tick();
  // No history at all: the model (`ai.ts`) fences the prompt as material.
  expect(last().options.history).toBeUndefined();
  expect(last().options).toMatchObject({ system: "Translate it.", prompt: "hello world" });
  last().finish("hallo Welt");
  expect(await answer).toBe("hallo Welt");

  await expect(runAs(as, () => AI.ask("x", { material: true, messages: [] }))).rejects.toMatchObject({ name: "HeyoError", code: "invalid" });
});

test("turns out of order are a wrong call", async () => {
  answers["ext.ai.provider"] = () => ({ kind: "claude", model: "", local: false });
  const invalid = async (messages: Message[]) => {
    const error = await runAs(as, () => AI.ask("next", { messages })).then(
      () => undefined,
      (e) => e,
    );
    expect(error).toBeInstanceOf(HeyoError);
    expect(error.code).toBe("invalid");
  };
  await invalid([{ role: "assistant", content: "first" }]);
  await invalid([{ role: "user", content: "a" }, { role: "user", content: "b" }]);
  // The prompt is the next question: the turns end with an answer.
  await invalid([{ role: "user", content: "a" }]);
  await invalid([{ role: "user", content: "a" }, { role: "assistant", content: 1 as never }]);
  expect(model.calls).toHaveLength(0);
});

test("withdrawn is cancelled; anything else is failed, with the reason in words", async () => {
  const controller = new AbortController();
  const withdrawn = runAs(as, () => AI.ask("Wait", { signal: controller.signal }));
  await tick();
  controller.abort();
  await expect(withdrawn).rejects.toMatchObject({ name: "HeyoError", code: "cancelled" });

  const failed = runAs(as, () => AI.ask("Fail"));
  await tick();
  last().fail(new Error("The local model returned no text."));
  await expect(failed).rejects.toMatchObject({ name: "HeyoError", code: "failed", message: "The local model returned no text." });
});
