// The runtime's conversation with the host, with the lines themselves in
// hand: what a request looks like on the wire and whose it says it is,
// which reply answers it, what a refusal becomes, where early events wait,
// and what happens to what was asked when the host hangs up.
import { afterEach, beforeEach, expect, test, vi } from "vitest";

import type { Incoming } from "../../shared/wire";
import { runAs, type Current } from "./context";
import { HeyoError } from "./heyo/error";
import { ask, askThroughRaycast, connect, hear, on, request, send } from "./protocol";
import { pair, type End } from "./transport";

const notes: Current = { extension: { dir: "/extensions/notes", manifest: { name: "notes", title: "Notes", commands: [] } }, command: "main", launchType: "userInitiated" };

/// The host's end, what was read there, and the conversation's end.
let host: End;
let lines: any[];
let over: Promise<void>;
const tell = (message: Incoming) => host.send(JSON.stringify(message));
/// How a promise ended, without waiting for one that has not.
const outcome = (promise: Promise<unknown>) =>
  Promise.race([
    promise.then(
      (value) => ({ value }),
      (error) => ({ error }),
    ),
    new Promise((resolve) => setImmediate(() => resolve("waiting"))),
  ]);

beforeEach(() => {
  const [ours, theirs] = pair();
  host = theirs;
  lines = [];
  host.receive(
    (line) => lines.push(JSON.parse(line)),
    () => {},
  );
  over = connect(ours);
});

afterEach(() => {
  // Nothing is left waiting, to fail when the next test begins anew.
  for (const line of lines) if (line.type === "request") tell({ type: "reply", id: line.id });
  vi.restoreAllMocks();
});

test("a reply answers the request with its number, whatever else is in flight", async () => {
  const [read, hud, apps] = [request("host.clipboard.read"), request("host.hud", { text: "Copied" }), request("host.applications")];
  const [first, second, third] = lines.map((line) => line.id);
  expect(lines).toEqual([
    { type: "request", id: first, method: "host.clipboard.read" },
    { type: "request", id: second, method: "host.hud", params: { text: "Copied" } },
    { type: "request", id: third, method: "host.applications" },
  ]);
  expect(new Set([first, second, third]).size).toBe(3);

  // Out of order, and with one nobody asked for in between.
  tell({ type: "reply", id: third, result: ["Files"] });
  tell({ type: "reply", id: third + 100, result: "stray" });
  expect(await outcome(apps)).toEqual({ value: ["Files"] });
  expect(await outcome(read)).toBe("waiting");
  tell({ type: "reply", id: first, result: { text: "clip" } });
  // Answered with nothing is answered.
  tell({ type: "reply", id: second });
  expect(await read).toEqual({ text: "clip" });
  expect(await hud).toBeUndefined();
  // Answered once: a second reply changes nothing.
  tell({ type: "reply", id: first, result: "again" });
  expect(await read).toEqual({ text: "clip" });
});

test("what the host says went wrong is the error the request fails with", async () => {
  const failing = request("host.trash", { paths: ["/gone"] });
  tell({ type: "reply", id: lines[0].id, error: "No such file." });
  const plain = await failing.catch((e) => e);
  expect(plain).toBeInstanceOf(Error);
  expect(plain).not.toBeInstanceOf(HeyoError);
  expect(plain).toMatchObject({ message: "No such file." });

  // To an extension's request the host gives a code.
  const denied = runAs(notes, () => ask("ext.memory.search", { text: "x" }));
  tell({ type: "reply", id: lines[1].id, error: { code: "denied", message: "Notes was not allowed to search what Heyo remembers." } });
  const refused = await denied.catch((e) => e);
  expect(refused).toBeInstanceOf(HeyoError);
  expect(refused).toMatchObject({ name: "HeyoError", code: "denied", message: "Notes was not allowed to search what Heyo remembers." });

  // A code this runtime does not know is a failure all the same.
  const odd = runAs(notes, () => ask("ext.windows.list"));
  tell({ type: "reply", id: lines[2].id, error: { code: "teapot", message: "Short and stout." } as any });
  expect(await odd.catch((e) => e)).toMatchObject({ name: "HeyoError", code: "failed", message: "Short and stout." });
});

test("an extension's request says whose it is, from the command that is running", async () => {
  void runAs(notes, () => ask("ext.windows.list"));
  void runAs(notes, () => askThroughRaycast("ext.windows.activate", { id: 7 }));
  // The runtime's own requests are nobody's, whoever runs.
  void runAs(notes, () => request("host.hide"));
  const [list, activate, hide] = lines;
  expect(lines).toEqual([
    { type: "request", id: list.id, method: "ext.windows.list", extension: "notes", dir: "/extensions/notes" },
    { type: "request", id: activate.id, method: "ext.windows.activate", extension: "notes", dir: "/extensions/notes", via: "raycast", params: { id: 7 } },
    { type: "request", id: hide.id, method: "host.hide" },
  ]);

  // What a command started is still its own when it gets round to asking.
  await runAs(notes, async () => {
    await new Promise((resolve) => setTimeout(resolve, 1));
    void ask("ext.frontmostApplication");
  });
  expect(lines[3]).toMatchObject({ method: "ext.frontmostApplication", extension: "notes" });
});

test("nothing is asked for an extension outside any command", async () => {
  for (const asking of [ask("ext.windows.list"), askThroughRaycast("ext.windows.list")]) {
    expect(await asking.catch((e) => e)).toMatchObject({ name: "HeyoError", code: "invalid", message: "ext.windows.list was asked for outside any command." });
  }
  expect(lines).toEqual([]);
});

test("events that come before anyone hears them wait, in order", () => {
  // The first events of a subscription can outrun the reply that names it.
  tell({ type: "event", subscription: 5, data: "one" });
  tell({ type: "event", subscription: 5, data: "two" });
  tell({ type: "event", subscription: 6, data: "another's" });
  const heard: unknown[] = [];
  const deaf = hear(5, (data) => heard.push(data));
  expect(heard).toEqual(["one", "two"]);
  tell({ type: "event", subscription: 5, data: "three" });
  expect(heard).toEqual(["one", "two", "three"]);
  deaf();
  tell({ type: "event", subscription: 5, data: "late" });
  expect(heard).toEqual(["one", "two", "three"]);
});

test("the host's messages go to whoever takes them, and a bad one ends nothing", () => {
  const errors = vi.spyOn(console, "error").mockImplementation(() => {});
  const seen: boolean[] = [];
  on("appearance", (message) => {
    if (!message.dark) throw new Error("too bright");
    seen.push(message.dark);
  });
  host.send("not a message");
  tell({ type: "appearance", dark: false });
  // Nobody takes these here.
  tell({ type: "pop", session: 1 });
  tell({ type: "appearance", dark: true });
  expect(seen).toEqual([true]);
  expect(errors.mock.calls.map(([first]) => String(first))).toEqual(["heyo-runtime: invalid message", "Error: too bright"]);

  // What the runtime tells is a line as it stands.
  send({ type: "toast-hide", id: 3 });
  expect(lines).toEqual([{ type: "toast-hide", id: 3 }]);
});

test("the host hanging up fails what was asked and ends the conversation", async () => {
  const [own, extensions] = [request("host.clipboard.read"), runAs(notes, () => ask("ext.windows.list"))];
  let ended = false;
  void over.then(() => (ended = true));
  expect(await outcome(own)).toBe("waiting");
  expect(ended).toBe(false);

  host.close();
  await over;
  const [first, second] = [await own.catch((e) => e), await extensions.catch((e) => e)];
  expect(first).toBeInstanceOf(Error);
  expect(first).toMatchObject({ message: "Heyo went away before it answered." });
  // An extension's request fails the way all of its requests fail.
  expect(second).toMatchObject({ name: "HeyoError", code: "failed", message: "Heyo went away before it answered." });

  // There is nobody to tell any more, and saying so is not silent.
  expect(() => send({ type: "pop-to-root" })).toThrow(/no host/);
  expect(await request("host.hide").catch((e) => e)).toMatchObject({ message: expect.stringMatching(/no host/) });
});

test("a conversation begun over another transport ends the one before", async () => {
  const waiting = request("host.clipboard.read");
  const [ours, theirs] = pair();
  const later: any[] = [];
  theirs.receive(
    (line) => later.push(JSON.parse(line)),
    () => {},
  );
  void connect(ours);
  expect(await waiting.catch((e) => e)).toMatchObject({ message: "Heyo went away before it answered." });

  // The old one is no longer listened to, nor is its hanging up this one's end.
  const asked = request("host.applications");
  tell({ type: "reply", id: later[0].id, result: "from the old host" });
  host.close();
  expect(await outcome(asked)).toBe("waiting");
  theirs.send(JSON.stringify({ type: "reply", id: later[0].id, result: [] }));
  expect(await asked).toEqual([]);
});
