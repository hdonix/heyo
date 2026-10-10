// `@heyo/api` as the runtime provides it: what each function asks of the
// host and in whose name, and what a command leaves behind when it ends.
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { beforeAll, beforeEach, expect, test, vi } from "vitest";

import { answers, event, extension, provideModules, requests, reset, sleep, until } from "../testing";

vi.mock("../protocol", async () => (await import("../testing")).protocol);

import raycast from "../api";
import { ended, runAs, type Current } from "../context";
import { close, launch } from "../session";
import { readImage, temperatureOf } from "./ai";
import { envelope } from "./caller";
import { fromReply, HeyoError } from "./error";
import api from "./index";
import { mailbox } from "./mailbox";

const world = globalThis as any;
const as = (name: string): Current => ({ extension: extension(name, {}), command: "main", launchType: "userInitiated" });
const last = () => requests.at(-1)!;

beforeAll(provideModules);

beforeEach(async () => {
  close();
  await sleep(10);
  reset();
});

test("a request is made in the name of the command that is running", async () => {
  const [notes, tiles] = [as("notes"), as("tiles")];
  await Promise.all([runAs(notes, () => api.Windows.list()), runAs(tiles, () => api.Files.search("report"))]);
  expect(requests).toEqual([
    { method: "ext.windows.list", params: undefined, extension: "notes", dir: notes.extension.dir },
    { method: "ext.files.search", params: { query: "report", limit: undefined }, extension: "tiles", dir: tiles.extension.dir },
  ]);
  // What comes later of a command's work is still that command's.
  await runAs(notes, async () => {
    await sleep(5);
    await api.Memory.get("clipboard:1");
  });
  expect(last().extension).toBe("notes");

  // The name is the manifest's; nothing an extension passes can be another's.
  expect(envelope(notes)).toEqual({ extension: "notes", dir: notes.extension.dir });
  expect(envelope(notes, "raycast")).toEqual({ extension: "notes", dir: notes.extension.dir, via: "raycast" });
  expect(envelope(null)).toBeUndefined();
  await runAs(tiles, () => api.Memory.search({ text: "x", extension: "notes", dir: "/elsewhere" } as never));
  expect(last()).toMatchObject({ extension: "tiles", dir: tiles.extension.dir });
  expect(last().params).not.toHaveProperty("extension");
});

test("each function asks the host for what its name says", async () => {
  const since = new Date(1_700_000_000_000);
  const bounds = { x: 1, y: 2, width: 300, height: 200 };
  const { Windows, Screen, Dictation, Memory, ClipboardHistory, Files, System, AI, Clipboard } = api;
  answers["ext.screen.capture"] = () => ({ path: path.join(os.tmpdir(), "heyo-no-such-capture.png"), width: 1, height: 1 });
  const cases: [() => Promise<unknown>, string, unknown][] = [
    [() => Windows.supported(), "ext.supported", { module: "windows" }],
    [() => Windows.list(), "ext.windows.list", undefined],
    [() => Windows.active(), "ext.windows.active", undefined],
    [() => Windows.activate(7), "ext.windows.activate", { id: 7 }],
    [() => Windows.close(7), "ext.windows.close", { id: 7 }],
    [() => Windows.arrange(7, "left-half"), "ext.windows.arrange", { id: 7, layout: "left-half" }],
    [() => Windows.arrange(7, { fraction: [0, 0, 4, 12] }), "ext.windows.arrange", { id: 7, layout: { fraction: [0, 0, 4, 12] } }],
    [() => Windows.setBounds(7, bounds), "ext.windows.setBounds", { id: 7, bounds }],
    [() => Windows.monitors(), "ext.windows.monitors", undefined],
    [() => Screen.supported(), "ext.supported", { module: "screen" }],
    [() => Screen.capture("selection"), "ext.screen.capture", { target: "selection" }],
    [() => Screen.capture({ window: 7 }), "ext.screen.capture", { target: { window: 7 } }],
    [() => Screen.capture({ rect: bounds }), "ext.screen.capture", { target: { rect: bounds } }],
    [() => Screen.recognizeText("/tmp/a.png"), "ext.screen.recognizeText", { path: "/tmp/a.png" }],
    [() => Dictation.supported(), "ext.supported", { module: "dictation" }],
    [() => Dictation.installed(), "ext.dictation.installed", undefined],
    [() => Memory.supported(), "ext.supported", { module: "memory" }],
    [() => Memory.search(), "ext.memory.search", {}],
    [
      () => Memory.search({ text: "invoice", sources: ["clipboard"], kinds: ["link"], since, until: since, limit: 5 }),
      "ext.memory.search",
      { text: "invoice", sources: ["clipboard"], kinds: ["link"], since: 1_700_000_000_000, until: 1_700_000_000_000, limit: 5 },
    ],
    [() => Memory.search({ text: "the bill", limit: 300, byMeaning: true }), "ext.memory.search", { text: "the bill", limit: 300, byMeaning: true }],
    [() => Memory.get("clipboard:4"), "ext.memory.get", { id: "clipboard:4" }],
    [() => ClipboardHistory.supported(), "ext.supported", { module: "clipboardHistory" }],
    [() => ClipboardHistory.search(), "ext.clipboardHistory.search", {}],
    [() => ClipboardHistory.search("key", { kind: "text", limit: 3 }), "ext.clipboardHistory.search", { text: "key", kind: "text", limit: 3 }],
    [() => ClipboardHistory.copy("clipboard:4"), "ext.clipboardHistory.copy", { id: "clipboard:4" }],
    [() => Files.supported(), "ext.supported", { module: "files" }],
    [() => Files.search("notes", { limit: 9 }), "ext.files.search", { query: "notes", limit: 9 }],
    [() => System.supported(), "ext.supported", { module: "system" }],
    [() => System.run("volume-up"), "ext.system.run", { action: "volume-up" }],
    [() => System.appearance(), "ext.system.appearance", undefined],
    [() => System.notify("Done", "All of it"), "ext.system.notify", { title: "Done", body: "All of it" }],
    [() => System.audioDevices(), "ext.system.audioDevices", undefined],
    [() => System.useAudioDevice("input", "alsa_input.usb"), "ext.system.useAudioDevice", { kind: "input", name: "alsa_input.usb" }],
    [() => AI.provider(), "ext.ai.provider", undefined],
    [() => Clipboard.paste("hello", { restoreClipboard: true }), "ext.clipboard.paste", { text: "hello" }],
    [() => Clipboard.paste({ text: "hi", html: "<b>hi</b>" }, { restoreClipboard: true }), "ext.clipboard.paste", { text: "hi", html: "<b>hi</b>" }],
  ];
  const command = as("probe");
  for (const [call, method, params] of cases) {
    if (method === "ext.ai.provider") answers[method] = () => ({ kind: "claude", model: "", local: false });
    await runAs(command, call);
    // As it goes over the wire: what was left out is not there.
    expect({ method: last().method, params: JSON.parse(JSON.stringify(last().params ?? null)) }, method).toEqual({ method, params: params ?? null });
    expect(last().via, method).toBeUndefined();
  }
});

test("what the host answers with nothing comes back as nothing", async () => {
  await runAs(as("probe"), async () => {
    answers["ext.windows.active"] = () => null;
    answers["ext.memory.get"] = () => null;
    answers["ext.system.run"] = () => null;
    expect(await api.Windows.active()).toBeUndefined();
    expect(await api.Memory.get("clipboard:9")).toBeUndefined();
    expect(await api.System.run("lock-screen")).toBeUndefined();
    answers["ext.system.run"] = () => "Volume 40%";
    expect(await api.System.run("volume-up")).toBe("Volume 40%");
    answers["ext.ai.provider"] = () => ({ kind: "claude", model: "", local: false });
    expect(await api.AI.provider()).toEqual({ kind: "claude", model: "claude-opus-5-5", local: false });
    answers["ext.ai.provider"] = () => ({ kind: "local", model: "qwen3", local: true });
    expect(await api.AI.provider()).toEqual({ kind: "local", model: "qwen3", local: true });
  });
});

test("Raycast's window functions ask the same things, and say they came that way", async () => {
  answers["ext.windows.active"] = () => ({ id: 7, title: "Notes", class: "notes", active: true, bounds: { x: 0, y: 0, width: 800, height: 600 } });
  answers["ext.windows.list"] = () => [{ id: 7, title: "Notes", class: "notes", active: true, bounds: { x: 0, y: 0, width: 800, height: 600 } }];
  answers["ext.windows.monitors"] = () => [{ x: 0, y: 0, width: 1920, height: 1080 }];
  answers["ext.frontmostApplication"] = () => ({ name: "Notes", path: "/usr/share/applications/notes.desktop" });
  await runAs(as("store-extension"), async () => {
    expect((await api.WindowManagement.getActiveWindow()).id).toBe("7");
    await api.WindowManagement.getDesktops();
    await api.WindowManagement.setWindowBounds({ id: "7", bounds: "fullscreen" });
    await api.WindowManagement.setWindowBounds({ id: "7", bounds: { position: { x: 10 } } });
    expect((await api.getFrontmostApplication()).name).toBe("Notes");
  });
  expect(requests.every((r) => r.via === "raycast" && r.extension === "store-extension")).toBe(true);
  expect(requests.map((r) => r.method)).toEqual([
    "ext.windows.active",
    "ext.windows.monitors",
    "ext.windows.list",
    "ext.windows.monitors",
    "ext.windows.arrange",
    "ext.windows.list",
    "ext.windows.monitors",
    "ext.windows.setBounds",
    "ext.frontmostApplication",
  ]);
  expect(requests[4].params).toEqual({ id: 7, layout: "maximize" });
  expect(requests[7].params).toEqual({ id: 7, bounds: { x: 10, y: 0, width: 800, height: 600 } });
  // Nothing of windows goes round the permission any more.
  expect(requests.some((r) => r.method.startsWith("host.windows") || r.method === "host.frontmostApplication")).toBe(false);
});

test("pasting the way Raycast does is asked the way it was", async () => {
  await runAs(as("probe"), () => api.Clipboard.paste("hello"));
  expect(requests.map((r) => r.method)).toEqual(["host.clipboard.write", "host.paste"]);
});

test("nothing can be asked outside a command", async () => {
  await expect(api.Windows.list()).rejects.toMatchObject({ name: "HeyoError", code: "invalid" });
  expect(requests).toEqual([]);
});

test("what the host says went wrong has a code", () => {
  const denied = fromReply({ code: "denied", message: "Window access was not allowed." });
  expect(denied).toBeInstanceOf(HeyoError);
  expect(denied).toBeInstanceOf(Error);
  expect(denied).toMatchObject({ name: "HeyoError", code: "denied", message: "Window access was not allowed." });
  // A code this runtime does not know is a failure all the same.
  expect(fromReply({ code: "teleported", message: "x" })).toMatchObject({ code: "failed", message: "x" });
  expect(fromReply({})).toMatchObject({ code: "failed" });
  // Heyo's own requests keep failing with a sentence.
  const plain = fromReply("the host cannot answer host.x");
  expect(plain).not.toBeInstanceOf(HeyoError);
  expect(plain.message).toBe("the host cannot answer host.x");
  expect(new HeyoError("unsupported").message).not.toBe("");
});

test("events that come before anyone hears them wait, in order and not for ever", () => {
  const events = mailbox(3);
  const heard: unknown[] = [];
  // The host wrote these before its answer to the subscribing request was read.
  events.deliver(7, "first");
  events.deliver(7, "second");
  const deaf = events.hear(7, (data) => heard.push(data));
  events.deliver(7, "third");
  expect(heard).toEqual(["first", "second", "third"]);
  deaf();
  events.deliver(7, "after");
  // Another subscription's are its own.
  events.hear(8, (data) => heard.push(["eight", data]));
  events.deliver(8, "x");
  expect(heard.slice(3)).toEqual([["eight", "x"]]);
  // A listener that was replaced is not taken away by the old one's leaving.
  const replaced: unknown[] = [];
  const old = events.hear(9, () => {});
  events.hear(9, (data) => replaced.push(data));
  old();
  events.deliver(9, "kept");
  expect(replaced).toEqual(["kept"]);

  // No more than a few are kept, of no more than a few subscriptions.
  const small = mailbox(2);
  for (const n of [1, 2, 3]) small.deliver(20, n);
  small.deliver(21, "a");
  small.deliver(22, "b");
  const late: unknown[] = [];
  small.hear(20, (data) => late.push(data));
  small.hear(21, (data) => late.push(data));
  small.hear(22, (data) => late.push(data));
  // Subscription 20 gave way to the newer two.
  expect(late).toEqual(["a", "b"]);
});

test("listening hands over each sentence and then all that was said", async () => {
  answers["ext.dictation.listen"] = () => ({ subscription: 3 });
  const segments: string[] = [];
  const command = as("scribe");
  const listening = runAs(command, () => api.Dictation.listen({ language: "en", indicator: false, onSegment: (text) => segments.push(text) }));
  await until(() => requests.length === 1, "the request to listen");
  expect(requests[0]).toMatchObject({ method: "ext.dictation.listen", params: { language: "en", indicator: false }, extension: "scribe" });
  await sleep(5);
  event(3, { kind: "segment", text: "Hello." });
  event(3, { kind: "segment", text: "World." });
  listening.stop();
  await until(() => requests.length === 2, "the request to stop");
  expect(last()).toMatchObject({ method: "ext.dictation.stop", params: { subscription: 3 }, extension: "scribe" });
  event(3, { kind: "result", text: "Hello. World." });
  await expect(listening.result).resolves.toBe("Hello. World.");
  expect(segments).toEqual(["Hello.", "World."]);
  // It is over: nobody hears the subscription, and nothing is left to end.
  expect(event(3, { kind: "segment", text: "late" })).toBe(false);
  listening.stop();
  ended(command);
  await sleep(5);
  expect(requests.length).toBe(2);
});

test("a recording that fails, is discarded or cannot start says so with a code", async () => {
  const command = as("scribe");
  answers["ext.dictation.listen"] = () => ({ subscription: 4 });
  const failing = runAs(command, () => api.Dictation.listen());
  await sleep(5);
  event(4, { kind: "error", code: "failed", message: "no microphone" });
  await expect(failing.result).rejects.toMatchObject({ code: "failed", message: "no microphone" });

  // Aborted: the host is told to drop it.
  answers["ext.dictation.listen"] = () => ({ subscription: 5 });
  const abort = new AbortController();
  const aborted = runAs(command, () => api.Dictation.listen({ signal: abort.signal }));
  await sleep(5);
  abort.abort();
  await expect(aborted.result).rejects.toMatchObject({ code: "cancelled" });
  await until(() => last().method === "ext.unsubscribe", "the subscription to be ended");
  expect(last()).toMatchObject({ params: { subscription: 5 }, extension: "scribe" });

  // Aborted before it began: no recording is started at all.
  reset();
  const never = runAs(command, () => api.Dictation.listen({ signal: AbortSignal.abort() }));
  await expect(never.result).rejects.toMatchObject({ code: "cancelled" });
  expect(requests).toEqual([]);

  // Another recording is going, or the engine is not there.
  answers["ext.dictation.listen"] = () => {
    throw new HeyoError("unavailable", "Dictation is not set up yet.");
  };
  await expect(runAs(command, () => api.Dictation.listen()).result).rejects.toMatchObject({ code: "unavailable" });
});

test("what a command subscribed to ends with the command", async () => {
  answers["ext.dictation.listen"] = () => ({ subscription: 6 });
  const scribe = extension("scribe", {
    listen: {
      mode: "view",
      source: `
        const { Detail, Dictation } = require("@heyo/api");
        const React = require("react");
        exports.default = () => {
          React.useEffect(() => {
            globalThis.listening = Dictation.listen();
            globalThis.listening.result.catch((e) => (globalThis.listenError = e));
          }, []);
          return React.createElement(Detail, { markdown: "Listening" });
        };`,
    },
  });
  launch(scribe, "listen");
  await until(() => requests.some((r) => r.method === "ext.dictation.listen"), "the command to listen");
  await sleep(10);
  expect(event(6, { kind: "segment", text: "Hello." })).toBe(true);
  // The user leaves the command.
  close();
  await until(() => last().method === "ext.unsubscribe", "the subscription to be ended");
  expect(last()).toMatchObject({ params: { subscription: 6 }, extension: "scribe", dir: scribe.dir });
  expect(event(6, { kind: "result", text: "Hello." })).toBe(false);
});

test("a capture is the command's for as long as it runs", async () => {
  const file = path.join(fs.mkdtempSync(path.join(os.tmpdir(), "heyo-capture-test-")), "capture-1.png");
  fs.writeFileSync(file, "png");
  answers["ext.screen.capture"] = () => ({ path: file, width: 4, height: 3 });
  answers["ext.screen.recognizeText"] = ({ path: given }) => ({ text: `text of ${given}` });
  const command = as("reader");
  await runAs(command, async () => {
    expect(await api.Screen.capture("screen")).toEqual({ path: file, width: 4, height: 3 });
    // Recognizing what is on screen is a capture and then a reading of it.
    expect(await api.Screen.recognizeText({ window: 7 })).toEqual({ text: `text of ${file}` });
  });
  expect(requests.map((r) => r.method)).toEqual(["ext.screen.capture", "ext.screen.capture", "ext.screen.recognizeText"]);
  expect(fs.existsSync(file)).toBe(true);
  ended(command);
  await until(() => !fs.existsSync(file), "the capture to be removed");
  fs.rmSync(path.dirname(file), { recursive: true, force: true });
});

test("both names give an extension the same module", async () => {
  const both = extension("both", {
    compare: {
      mode: "no-view",
      source: `
        const heyo = require("@heyo/api");
        const raycast = require("@raycast/api");
        exports.default = async () => {
          globalThis.compared = { same: heyo === raycast, isHeyo: raycast.isHeyo, list: heyo.List === raycast.List, name: heyo.environment.extensionName };
        };`,
    },
  });
  world.compared = undefined;
  launch(both, "compare");
  await until(() => world.compared, "the command to run");
  expect(world.compared).toEqual({ same: true, isHeyo: true, list: true, name: "both" });
});

test("everything of Raycast's API is there under its own name", () => {
  const extended = ["AI", "Clipboard", "environment"];
  for (const [name, value] of Object.entries(raycast)) {
    if (!extended.includes(name)) expect((api as Record<string, unknown>)[name], name).toBe(value);
  }
  // What Heyo extends keeps all it had.
  for (const name of extended) {
    const [theirs, ours] = [(raycast as any)[name], (api as any)[name]];
    for (const member of Object.getOwnPropertyNames(theirs)) expect(Object.hasOwn(ours, member), `${name}.${member}`).toBe(true);
  }
  expect(api.AI.Model).toBe(raycast.AI.Model);
  expect(api.Clipboard.copy).toBe(raycast.Clipboard.copy);
});

test("the environment follows the command and says what Heyo this is", () => {
  expect(api.isHeyo).toBe(true);
  expect(api.environment).toMatchObject({ apiVersion: 1, platform: process.platform === "win32" ? "windows" : "linux", offline: false });
  expect(["x11", "wayland", "windows", "unknown"]).toContain(api.environment.session);
  expect(typeof api.environment.heyoVersion).toBe("string");
  expect(typeof api.environment.raycastVersion).toBe("string");
  runAs(as("notes"), () => expect(api.environment.extensionName).toBe("notes"));
  runAs(as("tiles"), () => expect(api.environment.extensionName).toBe("tiles"));
});

test("has() says what is there and does something", () => {
  for (const feature of ["Windows.list", "Windows.setBounds", "Screen.recognizeText", "Dictation.listen", "Memory.search", "ClipboardHistory.copy", "Files.search", "System.run", "AI.provider", "AI.ask", "List", "List.Item", "Clipboard.paste", "isHeyo"]) {
    expect(api.has(feature), feature).toBe(true);
  }
  // Not in this version, there only to refuse, or not there at all.
  for (const feature of ["Windows.watch", "Memory.add", "Memory.remove", "Dictation.transcribe", "BrowserExtension", "BrowserExtension.getTabs", "Overlay", "Windows.list.length.x", "", "toString"]) {
    expect(api.has(feature), feature).toBe(false);
  }
});

test("creativity is a temperature as Raycast counts it", () => {
  expect([temperatureOf("none"), temperatureOf("low"), temperatureOf("medium"), temperatureOf("high"), temperatureOf("maximum")]).toEqual([0, 0.5, 1, 1.5, 2]);
  expect([temperatureOf(0.3), temperatureOf(7), temperatureOf(-1), temperatureOf(Number.NaN), temperatureOf(undefined)]).toEqual([0.3, 2, 0, undefined, undefined]);
});

test("an image goes to the model by what kind it is", () => {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), "heyo-image-test-"));
  fs.writeFileSync(path.join(dir, "shot.PNG"), Buffer.from([1, 2, 3]));
  expect(readImage(path.join(dir, "shot.PNG"))).toEqual({ mediaType: "image/png", data: "AQID" });
  expect(() => readImage(path.join(dir, "notes.txt"))).toThrowError(expect.objectContaining({ code: "invalid" }));
  expect(() => readImage(path.join(dir, "missing.jpg"))).toThrowError(expect.objectContaining({ code: "invalid" }));
  fs.rmSync(dir, { recursive: true, force: true });
});

test("a question with pictures is refused where the model cannot be given them", async () => {
  answers["ext.ai.provider"] = () => ({ kind: "local", model: "qwen3", local: true });
  const asked = runAs(as("looker"), () => api.AI.ask("What is this?", { images: ["/tmp/shot.png"] }));
  expect(typeof asked.on).toBe("function");
  await expect(asked).rejects.toMatchObject({ code: "unsupported" });
  // The permission is asked of the host; the model's settings never were.
  expect(requests.map((r) => r.method)).toEqual(["ext.ai.provider"]);
});
