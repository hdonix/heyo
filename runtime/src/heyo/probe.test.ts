// The probe extensions the sandbox runs (`scripts/gui-test/heyo-api.sh`),
// run here against a host that answers by rote: they load as extensions do,
// and write down what they were answered and what they were refused.
import { execFileSync } from "node:child_process";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { beforeAll, beforeEach, expect, test } from "vitest";

import { answers, requests, reset, runtime, sleep, until } from "../testing";

import { readManifest } from "../context";
import { close, launch } from "../session";
import { HeyoError } from "./error";

const probes = path.join(__dirname, "../../../scripts/gui-test/extensions");
const out = fs.mkdtempSync(path.join(os.tmpdir(), "heyo-probe-test-"));
process.env.HEYO_PROBE_OUT = path.join(out, "probe.json");

const item = { id: "clipboard:2", source: "clipboard", kind: "text", text: "probe text", at: 5, pinned: false };

beforeAll(() => runtime());

beforeEach(async () => {
  close();
  await sleep(10);
  reset();
});

/// Runs a probe command and reads what it wrote.
async function run(extension: string, command: string, name: string) {
  const dir = path.join(probes, extension);
  const file = path.join(out, `probe-${name}.json`);
  fs.rmSync(file, { force: true });
  launch({ dir, manifest: readManifest(dir) }, command);
  await until(() => fs.existsSync(file), `${command} to write its results`);
  return JSON.parse(fs.readFileSync(file, "utf8"));
}

test("every probe command is a file that parses, and the manifest lists what they use", () => {
  for (const extension of ["api-probe", "api-probe-bare"]) {
    const dir = path.join(probes, extension);
    for (const command of readManifest(dir).commands) execFileSync(process.execPath, ["--check", path.join(dir, `${command.name}.js`)]);
  }
  const manifest = JSON.parse(fs.readFileSync(path.join(probes, "api-probe/package.json"), "utf8"));
  expect(manifest.heyo.apiVersion).toBe(1);
  expect(manifest.heyo.permissions).toEqual(expect.arrayContaining(["windows", "screen.capture", "microphone", "memory.read", "clipboard.history", "system"]));
  expect(JSON.parse(fs.readFileSync(path.join(probes, "api-probe-bare/package.json"), "utf8")).heyo).toBeUndefined();
});

test("the probe of what there is asks under its own name", async () => {
  answers["ext.supported"] = () => "full";
  answers["ext.dictation.installed"] = () => false;
  answers["ext.ai.provider"] = () => ({ kind: "local", model: "qwen3", local: true });
  const info = await run("api-probe", "heyo-info", "info");
  expect(info).toMatchObject({
    isHeyo: true,
    sameModule: true,
    environment: { apiVersion: 1, extensionName: "api-probe", commandName: "heyo-info", offline: false },
    has: { "Windows.list": true, "Screen.recognizeText": true, "Windows.watch": false, "Memory.add": false },
    supported: { Windows: "full", Screen: "full", Dictation: "full", Memory: "full", ClipboardHistory: "full", Files: "full", System: "full" },
    dictationInstalled: false,
    provider: { kind: "local", model: "qwen3", local: true },
  });
  expect(requests.every((r) => r.extension === "api-probe" && r.dir === path.join(probes, "api-probe"))).toBe(true);
});

test("the probes of the memory and of being asked write down answers and refusals", async () => {
  answers["ext.memory.search"] = () => ({ items: [item], understood: { terms: ["probe"] }, relaxed: false });
  answers["ext.memory.get"] = ({ id }) => {
    if (id === "nonsense") throw new HeyoError("invalid", "not an id");
    return id === item.id ? item : null;
  };
  const memory = await run("api-probe", "heyo-memory", "memory");
  expect(memory.recent).toEqual([{ id: "clipboard:2", source: "clipboard", kind: "text", text: "probe text", pinned: false, at: "number" }]);
  expect(memory.memory.understood).toEqual({ terms: ["probe"] });
  expect(memory.one.id).toBe("clipboard:2");
  expect(memory.none).toBeNull();
  expect(memory.badId).toEqual({ error: "invalid", message: "not an id" });
  expect(memory.copied).toBeNull();
  expect(requests.at(-1)).toMatchObject({ method: "ext.clipboardHistory.copy", params: { id: "clipboard:2" } });

  close();
  await sleep(10);
  answers["ext.memory.search"] = () => {
    throw new HeyoError("denied", "API Probe was not allowed to search what Heyo remembers.");
  };
  const asked = await run("api-probe", "heyo-asked", "asked");
  expect(asked.denied).toMatchObject({ error: "denied" });
  expect(asked.deniedAgain).toMatchObject({ error: "denied" });
  expect([asked.allowed, asked.allowedAgain]).toEqual([1, 1]);
});

test("the probe without a manifest goes through Raycast's window functions too", async () => {
  const refuse = (permission: string) => () => {
    throw new HeyoError("denied", `needs the "${permission}" permission`);
  };
  Object.assign(answers, {
    "ext.files.search": refuse("files.search"),
    "ext.windows.list": refuse("windows"),
    "ext.screen.capture": refuse("screen.capture"),
    "ext.ai.provider": refuse("ai"),
    "ext.supported": () => "full",
    "ext.frontmostApplication": () => ({ name: "Notes", path: "" }),
    "ext.windows.active": () => ({ id: 7, title: "Notes", class: "notes", active: true }),
    "ext.windows.monitors": () => [{ x: 0, y: 0, width: 1920, height: 1080 }],
  });
  const bare = await run("api-probe-bare", "bare", "bare");
  expect(bare).toMatchObject({
    files: { error: "denied" },
    windows: { error: "denied" },
    selection: { error: "denied" },
    provider: { error: "denied" },
    supported: "full",
    frontmost: "Notes",
    activeWindow: "7",
    windowsAfter: { error: "denied" },
  });
  const through = requests.filter((r) => r.via === "raycast").map((r) => r.method);
  expect(through).toEqual(["ext.frontmostApplication", "ext.windows.active", "ext.windows.monitors"]);
  expect(requests.filter((r) => r.method === "ext.windows.list").every((r) => r.via === undefined)).toBe(true);
});
