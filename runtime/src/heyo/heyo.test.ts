// `@heyo/api` as the runtime provides it: what each function asks of the
// host (which its declaration in `members.ts` says) and in whose name, and
// what a command leaves behind when it ends.
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { beforeAll, beforeEach, expect, test } from "vitest";

import { answers, event, extension, requests, reset, runtime, sleep, until } from "../testing";

import raycast from "../api";
import { ended, runAs, type Current } from "../context";
import { close, launch } from "../session";
import { readImage, temperatureOf } from "./ai";
import { envelope } from "./caller";
import { fromReply, HeyoError } from "./error";
import api from "./index";
import { mailbox } from "./mailbox";
import { gained, members, modules, type Declared } from "./members";

const world = globalThis as any;
const as = (name: string): Current => ({ extension: extension(name, {}), command: "main", launchType: "userInitiated" });
const last = () => requests.at(-1)!;

beforeAll(() => runtime());

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

/// A function called, and the parameters that makes of its arguments
/// (none where the request takes none).
type Call = [call: () => unknown, params?: unknown];
/// Calls of every function declared. One declared and not called here
/// does not compile.
type Calls<Group> = { [Module in keyof Group]: { [Name in keyof Group[Module]]: [Call, ...Call[]] } };

test("each function asks the host for what its declaration says", async () => {
  const since = new Date(1_700_000_000_000);
  const bounds = { x: 1, y: 2, width: 300, height: 200 };
  const { Windows, Screen, Dictation, Memory, ClipboardHistory, Files, System, LiveActivity, AI, Clipboard } = api;
  let listening: ReturnType<typeof Dictation.listen> | undefined;
  const calls: Calls<typeof modules> & Calls<typeof gained> = {
    Windows: {
      supported: [[() => Windows.supported(), { module: "windows" }]],
      list: [[() => Windows.list()]],
      active: [[() => Windows.active()]],
      activate: [[() => Windows.activate(7), { id: 7 }]],
      close: [[() => Windows.close(7), { id: 7 }]],
      arrange: [
        [() => Windows.arrange(7, "left-half"), { id: 7, layout: "left-half" }],
        [() => Windows.arrange(7, { fraction: [0, 0, 4, 12] }), { id: 7, layout: { fraction: [0, 0, 4, 12] } }],
      ],
      setBounds: [[() => Windows.setBounds(7, bounds), { id: 7, bounds }]],
      monitors: [[() => Windows.monitors()]],
    },
    Screen: {
      supported: [[() => Screen.supported(), { module: "screen" }]],
      capture: [
        [() => Screen.capture("selection"), { target: "selection" }],
        [() => Screen.capture({ window: 7 }), { target: { window: 7 } }],
        [() => Screen.capture({ rect: bounds }), { target: { rect: bounds } }],
      ],
      recognizeText: [[() => Screen.recognizeText("/tmp/a.png"), { path: "/tmp/a.png" }]],
    },
    Dictation: {
      supported: [[() => Dictation.supported(), { module: "dictation" }]],
      installed: [[() => Dictation.installed()]],
      listen: [[() => (listening = Dictation.listen({ language: "en", indicator: false })), { language: "en", indicator: false }]],
    },
    Memory: {
      supported: [[() => Memory.supported(), { module: "memory" }]],
      search: [
        [() => Memory.search(), {}],
        [
          () => Memory.search({ text: "invoice", sources: ["clipboard"], kinds: ["link"], since, until: since, limit: 5 }),
          { text: "invoice", sources: ["clipboard"], kinds: ["link"], since: 1_700_000_000_000, until: 1_700_000_000_000, limit: 5 },
        ],
        [() => Memory.search({ text: "the bill", limit: 300, byMeaning: true }), { text: "the bill", limit: 300, byMeaning: true }],
      ],
      get: [[() => Memory.get("clipboard:4"), { id: "clipboard:4" }]],
    },
    ClipboardHistory: {
      supported: [[() => ClipboardHistory.supported(), { module: "clipboardHistory" }]],
      copy: [[() => ClipboardHistory.copy("clipboard:4"), { id: "clipboard:4" }]],
    },
    Files: {
      supported: [[() => Files.supported(), { module: "files" }]],
      search: [
        [() => Files.search("notes", { limit: 9 }), { query: "notes", limit: 9 }],
        [() => Files.search("notes"), { query: "notes" }],
      ],
    },
    System: {
      supported: [[() => System.supported(), { module: "system" }]],
      run: [[() => System.run("volume-up"), { action: "volume-up" }]],
      appearance: [[() => System.appearance()]],
      notify: [
        [() => System.notify("Done", "All of it"), { title: "Done", body: "All of it" }],
        [() => System.notify("Done"), { title: "Done" }],
      ],
      audioDevices: [[() => System.audioDevices()]],
      useAudioDevice: [[() => System.useAudioDevice("input", "alsa_input.usb"), { kind: "input", name: "alsa_input.usb" }]],
    },
    LiveActivity: {
      supported: [[() => LiveActivity.supported(), { module: "activity" }]],
      start: [
        [
          () => LiveActivity.start({ id: "a", name: "Build", icon: "hammer-16" }, { title: "Building" }),
          { attributes: { id: "a", name: "Build", icon: "hammer-16" }, content: { title: "Building" }, launchType: "userInitiated", seq: expect.any(Number) },
        ],
      ],
      list: [[() => LiveActivity.list()]],
      get: [[() => LiveActivity.get("a"), { id: "a" }]],
      presentation: [[() => LiveActivity.presentation()]],
      onEvent: [[() => LiveActivity.onEvent(() => {})]],
    },
    AI: {
      provider: [[() => AI.provider()]],
      // Pictures, which the model set up here is not given: it is asked which it is.
      ask: [[() => AI.ask("What is this?", { images: ["/tmp/shot.png"] })]],
    },
    Clipboard: {
      paste: [
        [() => Clipboard.paste("hello", { restoreClipboard: true }), { text: "hello" }],
        [() => Clipboard.paste({ text: "hi", html: "<b>hi</b>" }, { restoreClipboard: true }), { text: "hi", html: "<b>hi</b>" }],
      ],
    },
  };
  Object.assign(answers, {
    "ext.screen.capture": () => ({ path: path.join(os.tmpdir(), "heyo-no-such-capture.png"), width: 1, height: 1 }),
    "ext.dictation.listen": () => ({ subscription: 21 }),
    "ext.activity.start": () => ({ id: "a" }),
    "ext.activity.list": () => [],
    "ext.activity.onEvent": () => ({ subscription: 22 }),
    "ext.ai.provider": () => ({ kind: "local", model: "qwen3", local: true }),
  });

  const command = as("probe");
  const declared: Record<string, Record<string, Declared>> = { ...modules, ...gained };
  for (const { module, name, member, method } of members) {
    for (const [call, params] of (calls as Record<string, Record<string, Call[]>>)[module][name]) {
      const from = requests.length;
      // What it comes to is not looked at here, only what it asks.
      await Promise.resolve(runAs(command, call)).catch(() => {});
      const asked = await until(() => requests.slice(from).find((request) => request.method === method), `${member} to ask for ${method}`);
      // As it goes over the wire: what was left out is not there.
      expect(JSON.parse(JSON.stringify(asked.params ?? null)), member).toEqual(params ?? null);
      expect(asked, member).toMatchObject({ extension: "probe", dir: command.extension.dir });
      expect(asked.via, member).toBeUndefined();
      // One that is made from its declaration is that request and no more.
      if (!("byHand" in declared[module][name])) expect(requests.length - from, member).toBe(1);
    }
  }
  // Every function there is was declared, so every one was called.
  for (const [module, functions] of Object.entries(declared)) {
    if (module in modules) expect(Object.keys((api as Record<string, unknown>)[module] as object).sort(), module).toEqual(Object.keys(functions).sort());
    else for (const name of Object.keys(functions)) expect(api.has(`${module}.${name}`), `${module}.${name}`).toBe(true);
  }

  event(21, { kind: "result", text: "" });
  await listening!.result;
  ended(command);
  await until(() => last().method === "ext.unsubscribe", "the listening for activities to end");
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
    // As the host says it: Claude's model is named there.
    answers["ext.ai.provider"] = () => ({ kind: "claude", model: "claude-opus-5-5", local: false });
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
  // It is over: what still comes is not heard, and nothing is left to end.
  event(3, { kind: "segment", text: "late" });
  expect(segments).toEqual(["Hello.", "World."]);
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
  event(6, { kind: "segment", text: "Hello." });
  // The user leaves the command.
  close();
  await until(() => last().method === "ext.unsubscribe", "the subscription to be ended");
  expect(last()).toMatchObject({ params: { subscription: 6 }, extension: "scribe", dir: scribe.dir });
  // What still comes for it is not heard.
  let heard = false;
  world.listening.result.then(() => (heard = true));
  event(6, { kind: "result", text: "Hello." });
  await sleep(5);
  expect(heard).toBe(false);
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
