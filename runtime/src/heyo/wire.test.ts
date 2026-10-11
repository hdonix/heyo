// The runtime as the host starts it: built into one file and spoken to over
// its stdin and stdout, with this test in the host's place. What the
// conversation does is tested where it is loaded as a module
// (`protocol.test.ts`, `runtime.test.ts`); here is what only the program
// shows: that it talks over real pipes, keeps them to itself, is the
// process it was started as, and ends when the host hangs up.
import { spawn, type ChildProcess } from "node:child_process";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { build } from "esbuild";
import { afterAll, beforeAll, expect, test } from "vitest";

import { testHost } from "../testhost";
import { streams } from "../transport";
import { HeyoError } from "./error";

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
        // Neither of these is the extension's to do to the runtime.
        console.log("printed by the extension");
        process.stdout.write("{ not for the host\\n");
        process.exit(3);
        const { heyoVersion, apiVersion, session, extensionName } = heyo.environment;
        const result = { same: heyo === require("@raycast/api"), isHeyo: heyo.isHeyo, heyoVersion, apiVersion, session, extensionName, segments: [] };
        result.windows = await heyo.Windows.list();
        result.denied = await heyo.Memory.search({ text: "x" }).catch((e) => ({ name: e.name, code: e.code, message: e.message, ours: e instanceof heyo.HeyoError }));
        result.text = await heyo.Dictation.listen({ onSegment: (text) => result.segments.push(text) }).result;
        require("node:fs").writeFileSync(${JSON.stringify(out)}, JSON.stringify(result));
      };`,
  },
  work: { mode: "no-view", source: `exports.default = async () => {};` },
};

const running: ChildProcess[] = [];

/// Starts the program as `runtime.rs` does, and waits until it is ready.
async function start(environment: Record<string, string> = {}) {
  const child = spawn(process.execPath, [bundle], {
    stdio: ["pipe", "pipe", "pipe"],
    env: {
      ...process.env,
      HEYO_DATA_DIR: path.join(root, "data"),
      HEYO_CONFIG_DIR: path.join(root, "config"),
      HEYO_CACHE_DIR: path.join(root, "cache"),
      HEYO_EXTENSIONS_DIR: path.join(root, "data", "extensions"),
      HEYO_VERSION: "9.9.9",
      HEYO_API_VERSION: "1",
      HEYO_SESSION: "wayland",
      ...environment,
    },
  });
  running.push(child);
  const errors: string[] = [];
  child.stderr!.on("data", (text) => errors.push(String(text)));
  const host = testHost(streams(child.stdout!, (text) => child.stdin!.write(text)));
  const until = async <T>(check: () => T | undefined | false, what: string): Promise<T> => {
    const deadline = Date.now() + 10_000;
    for (;;) {
      const value = check();
      if (value) return value;
      if (Date.now() > deadline) throw new Error(`timed out waiting for ${what}; the runtime said ${JSON.stringify(host.sent.slice(-6))} and printed ${errors.join("")}`);
      await new Promise((resolve) => setTimeout(resolve, 10));
    }
  };
  await until(() => host.sent.some((m) => m.type === "ready"), "the runtime to start");
  return { child, host, until, printed: () => errors.join("") };
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
}, 60_000);

afterAll(() => {
  running.forEach((child) => child.kill());
  fs.rmSync(root, { recursive: true, force: true });
});

test("an extension's requests go out under its name, and answers, refusals and events come back", async () => {
  const { host, until, printed } = await start();
  host.answers["ext.windows.list"] = () => [{ id: 7, title: "Notes", class: "notes", active: true }];
  host.answers["ext.memory.search"] = () => {
    throw new HeyoError("denied", "Scribe was not allowed to search what Heyo remembers.");
  };
  host.answers["ext.dictation.listen"] = () => {
    // The first event is on the wire before the answer that names its subscription.
    host.tell({ type: "event", subscription: 5, data: { kind: "segment", text: "Hello." } });
    setImmediate(() => {
      host.tell({ type: "event", subscription: 5, data: { kind: "segment", text: "World." } });
      host.tell({ type: "event", subscription: 5, data: { kind: "result", text: "Hello. World." } });
    });
    return { subscription: 5 };
  };
  host.tell({ type: "launch", extension, command: "probe" });
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
  const asked = (method: string) => host.requests.filter((r) => r.method === method);
  expect(asked("ext.windows.list")).toEqual([{ method: "ext.windows.list", extension: "scribe", dir: extension }]);
  expect(asked("ext.memory.search")).toEqual([{ method: "ext.memory.search", extension: "scribe", dir: extension, params: { text: "x" } }]);
  // The recording ended by itself: there was nothing to unsubscribe from.
  await until(() => host.sent.some((m) => m.type === "closed"), "the command to close");
  expect(asked("ext.unsubscribe")).toEqual([]);

  // Stdout stayed the host's (it read nothing it could not), and the
  // process the runtime's: what the extension printed went to stderr, and
  // its exit was not one.
  expect(printed()).toContain("printed by the extension");
  expect(printed()).toContain("{ not for the host");
  expect(printed()).toContain("ignored process.exit(3)");
});

test("started as the background process, it runs what the host has run there", async () => {
  const { host, until } = await start({ HEYO_BACKGROUND: "1" });
  host.tell({ type: "background-run", id: 4, extension, command: "work", launchType: "background", action: null });
  await until(() => host.sent.some((m) => m.type === "background-done"), "the run to be done");
  expect(host.sent.filter((m) => m.type !== "ready")).toEqual([{ type: "background-done", id: 4 }]);
});

test("the host hanging up ends the process", async () => {
  const { child } = await start();
  const exited = new Promise((resolve) => child.on("exit", resolve));
  child.stdin!.end();
  expect(await exited).toBe(0);
});
