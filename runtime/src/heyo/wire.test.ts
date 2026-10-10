// The runtime as the host starts it (built into one file, spoken to over
// stdin and stdout), with this test in the host's place: what an extension
// asks through `@heyo/api` goes over the wire with its name on it, and what
// the host answers, refuses and tells comes back to the extension. Heyo's
// own commands ask the same way, under Heyo's name and from no directory.
import { spawn, type ChildProcess } from "node:child_process";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { createInterface } from "node:readline";
import { build } from "esbuild";
import { afterAll, beforeAll, expect, test } from "vitest";

const root = fs.mkdtempSync(path.join(os.tmpdir(), "heyo-wire-test-"));
const bundle = path.join(root, "heyo-runtime.cjs");
const extension = path.join(root, "data", "extensions", "scribe");
const out = path.join(root, "out.json");

const commands: Record<string, { mode: string; source: string }> = {
  probe: {
    mode: "no-view",
    source: `
      const heyo = require("@heyo/api");
      exports.default = async () => {
        const { heyoVersion, apiVersion, session, extensionName } = heyo.environment;
        const result = { same: heyo === require("@raycast/api"), isHeyo: heyo.isHeyo, heyoVersion, apiVersion, session, extensionName, segments: [] };
        result.windows = await heyo.Windows.list();
        result.denied = await heyo.Memory.search({ text: "x" }).catch((e) => ({ name: e.name, code: e.code, message: e.message, ours: e instanceof heyo.HeyoError }));
        result.text = await heyo.Dictation.listen({ onSegment: (text) => result.segments.push(text) }).result;
        require("node:fs").writeFileSync(${JSON.stringify(out)}, JSON.stringify(result));
      };`,
  },
  listen: {
    mode: "view",
    source: `
      const { Detail, Dictation } = require("@heyo/api");
      const React = require("react");
      exports.default = () => {
        React.useEffect(() => void Dictation.listen().result.catch(() => {}), []);
        return React.createElement(Detail, { markdown: "Listening" });
      };`,
  },
};

let runtime: ChildProcess;
const received: any[] = [];
let onMessage: (message: any) => void = () => {};
const tell = (message: unknown) => runtime.stdin!.write(JSON.stringify(message) + "\n");

async function until<T>(check: () => T | undefined | false, what: string): Promise<T> {
  const deadline = Date.now() + 10_000;
  for (;;) {
    const value = check();
    if (value) return value;
    if (Date.now() > deadline) throw new Error(`timed out waiting for ${what}; the runtime said ${JSON.stringify(received.slice(-6))}`);
    await new Promise((resolve) => setTimeout(resolve, 10));
  }
}

beforeAll(async () => {
  await build({
    entryPoints: [path.join(__dirname, "../main.ts")],
    outfile: bundle,
    bundle: true,
    platform: "node",
    target: "node22",
    format: "cjs",
    jsx: "automatic",
    define: { "process.env.NODE_ENV": '"production"' },
    logLevel: "silent",
  });
  fs.mkdirSync(extension, { recursive: true });
  const manifest = { name: "scribe", title: "Scribe", commands: Object.entries(commands).map(([name, { mode }]) => ({ name, title: name, mode })) };
  fs.writeFileSync(path.join(extension, "package.json"), JSON.stringify(manifest));
  for (const [name, { source }] of Object.entries(commands)) fs.writeFileSync(path.join(extension, `${name}.js`), source);

  runtime = spawn(process.execPath, [bundle], {
    stdio: ["pipe", "pipe", "ignore"],
    env: {
      ...process.env,
      HEYO_DATA_DIR: path.join(root, "data"),
      HEYO_CONFIG_DIR: path.join(root, "config"),
      HEYO_CACHE_DIR: path.join(root, "cache"),
      HEYO_EXTENSIONS_DIR: path.join(root, "data", "extensions"),
      HEYO_VERSION: "9.9.9",
      HEYO_API_VERSION: "1",
      HEYO_SESSION: "wayland",
    },
  });
  createInterface({ input: runtime.stdout! }).on("line", (line) => {
    const message = JSON.parse(line);
    received.push(message);
    onMessage(message);
  });
  await until(() => received.some((m) => m.type === "ready"), "the runtime to start");
}, 60_000);

afterAll(() => {
  runtime?.kill();
  fs.rmSync(root, { recursive: true, force: true });
});

const asked = (method: string) => received.filter((m) => m.type === "request" && m.method === method);

test("an extension's requests go out under its name, and answers, refusals and events come back", async () => {
  onMessage = (message) => {
    if (message.type !== "request") return;
    const { id } = message;
    if (message.method === "ext.windows.list") tell({ type: "reply", id, result: [{ id: 7, title: "Notes", class: "notes", active: true }] });
    if (message.method === "ext.memory.search") tell({ type: "reply", id, error: { code: "denied", message: "Scribe was not allowed to search what Heyo remembers." } });
    if (message.method === "ext.dictation.listen") {
      // The first event is on the wire before the answer that names its subscription.
      tell({ type: "event", subscription: 5, data: { kind: "segment", text: "Hello." } });
      tell({ type: "reply", id, result: { subscription: 5 } });
      tell({ type: "event", subscription: 5, data: { kind: "segment", text: "World." } });
      tell({ type: "event", subscription: 5, data: { kind: "result", text: "Hello. World." } });
    }
  };
  tell({ type: "launch", extension, command: "probe" });
  await until(() => fs.existsSync(out), "the command to finish");
  expect(JSON.parse(fs.readFileSync(out, "utf8"))).toEqual({
    same: true,
    isHeyo: true,
    heyoVersion: "9.9.9",
    apiVersion: 1,
    session: "wayland",
    extensionName: "scribe",
    windows: [{ id: 7, title: "Notes", class: "notes", active: true }],
    denied: { name: "HeyoError", code: "denied", message: "Scribe was not allowed to search what Heyo remembers.", ours: true },
    segments: ["Hello.", "World."],
    text: "Hello. World.",
  });

  // As the specification writes a request: who asks beside what is asked.
  const [list] = asked("ext.windows.list");
  expect(list).toEqual({ type: "request", id: list.id, method: "ext.windows.list", extension: "scribe", dir: extension });
  expect(asked("ext.memory.search")[0]).toMatchObject({ extension: "scribe", dir: extension, params: { text: "x" } });
  // The recording ended by itself: there was nothing to unsubscribe from.
  await until(() => received.some((m) => m.type === "closed"), "the command to close");
  expect(asked("ext.unsubscribe")).toEqual([]);
});

test("a command that is closed gives up what it subscribed to", async () => {
  received.length = 0;
  onMessage = (message) => {
    if (message.type === "request" && message.method === "ext.dictation.listen") tell({ type: "reply", id: message.id, result: { subscription: 6 } });
  };
  tell({ type: "launch", extension, command: "listen" });
  await until(() => asked("ext.dictation.listen").length === 1, "the command to listen");
  await until(() => received.some((m) => m.type === "render" && m.view), "the command's view");
  // The user leaves the command.
  tell({ type: "close" });
  const [unsubscribe] = await until(() => asked("ext.unsubscribe").length > 0 && asked("ext.unsubscribe"), "the subscription to be ended");
  expect(unsubscribe).toMatchObject({ method: "ext.unsubscribe", params: { subscription: 6 }, extension: "scribe", dir: extension });
});

test("one of Heyo's own commands asks as an extension does, under Heyo's name and from nowhere", async () => {
  received.length = 0;
  const windows = [
    { id: 7, title: "Notes", class: "notes", active: true },
    { id: 8, title: "Inbox", class: "mail", active: false },
  ];
  onMessage = (message) => {
    if (message.type !== "request") return;
    if (message.method === "ext.windows.list") tell({ type: "reply", id: message.id, result: windows });
    if (message.method === "host.applications") tell({ type: "reply", id: message.id, result: [] });
    if (message.method === "ext.windows.activate") tell({ type: "reply", id: message.id, result: null });
  };
  tell({ type: "launch", extension: "@heyo", command: "switch-windows" });
  const [list] = await until(() => asked("ext.windows.list").length > 0 && asked("ext.windows.list"), "the windows to be asked for");
  expect(list).toEqual({ type: "request", id: list.id, method: "ext.windows.list", extension: "@heyo", dir: "" });
  // The private way to the same is gone.
  expect(asked("host.windows.list")).toEqual([]);

  // The window the user came from is the last row; choosing the first switches to the other.
  const rows = () => received.filter((m) => m.type === "render" && m.view).flatMap((m) => m.view.nodes.filter((node: any) => node.type === "List.Item"));
  await until(() => rows().length >= 2, "the windows to be shown");
  const titles = [...new Map(rows().map((row: any) => [row.id, row.props.title])).values()];
  expect(titles).toEqual(["Inbox", "Notes"]);
  const session = received.find((m) => m.type === "launched").session;
  const actions = received.filter((m) => m.type === "render" && m.view).flatMap((m) => m.view.nodes.filter((node: any) => node.type === "Action" && node.props.title === "Switch to Window"));
  tell({ type: "dispatch", session, node: actions[0].id, prop: "onAction", args: [] });
  const [activate] = await until(() => asked("ext.windows.activate").length > 0 && asked("ext.windows.activate"), "the window to be switched to");
  expect(activate).toMatchObject({ extension: "@heyo", dir: "", params: { id: 8 } });
  tell({ type: "close" });
});
