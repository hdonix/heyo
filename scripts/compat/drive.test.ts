// Launches small extensions written here in the real runtime bundle, to
// hold the driver to what the runtime actually says. Skipped where the
// bundle has not been built (`pnpm build:runtime`).

import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { afterAll, describe, expect, it } from "vitest";

import { classifyCommand } from "./classify.mjs";
import { launchCommand } from "./drive.mjs";

const runtimeFile = path.resolve(__dirname, "..", "..", "runtime", "dist", "heyo-runtime.cjs");
const root = fs.mkdtempSync(path.join(os.tmpdir(), "heyo-compat-test-"));
afterAll(() => fs.rmSync(root, { recursive: true, force: true }));

const commands: Record<string, { mode: string; source: string }> = {
  list: {
    mode: "view",
    source: `const { List } = require("@raycast/api"); const { createElement: h } = require("react");
      exports.default = () => h(List, null, h(List.Item, { title: "One" }), h(List.Item, { title: "Two" }));`,
  },
  search: {
    mode: "view",
    source: `const { List } = require("@raycast/api"); const { createElement: h, useState } = require("react");
      exports.default = () => { const [q, setQ] = useState("");
        return h(List, { onSearchTextChange: setQ }, q ? h(List.Item, { title: "Found " + q }) : h(List.EmptyView, { title: "Type something" })); };`,
  },
  opener: {
    mode: "no-view",
    source: `const { open, showHUD, getApplications } = require("@raycast/api");
      exports.default = async () => { const apps = await getApplications(); await open("https://example.com/" + apps.length); await showHUD("Opened"); };`,
  },
  script: {
    mode: "no-view",
    source: `const { execFileSync } = require("node:child_process");
      exports.default = async () => { execFileSync("osascript", ["-e", "beep"]); };`,
  },
  broken: {
    mode: "view",
    source: `exports.default = () => { throw new Error("it broke"); };`,
  },
  secret: {
    mode: "view",
    source: `const { Detail, getPreferenceValues } = require("@raycast/api"); const { createElement: h } = require("react");
      exports.default = () => h(Detail, { markdown: "token is " + getPreferenceValues().token + ", home is " + process.env.HOME + ", display is " + process.env.DISPLAY });`,
  },
  gone: {
    mode: "no-view",
    source: `exports.default = async () => { process.kill(process.pid, "SIGKILL"); await new Promise(() => {}); };`,
  },
};

const manifest = {
  name: "compat-fixture",
  title: "Fixture",
  commands: Object.entries(commands).map(([name, { mode }]) => ({
    name,
    title: name,
    mode,
    ...(name === "secret" ? { preferences: [{ name: "token", type: "password", required: true }] } : {}),
  })),
};
const extensionDir = path.join(root, "extensions", manifest.name);
fs.mkdirSync(extensionDir, { recursive: true });
fs.writeFileSync(path.join(extensionDir, "package.json"), JSON.stringify(manifest));
for (const [name, { source }] of Object.entries(commands)) fs.writeFileSync(path.join(extensionDir, `${name}.js`), source);

const launch = (name: string) =>
  launchCommand({
    runtimeFile,
    extensionDir,
    manifest,
    command: manifest.commands.find((c) => c.name === name),
    scratch: fs.mkdtempSync(path.join(root, "run-")),
    sandbox: "none",
    timeoutMs: 8_000,
    quietMs: 300,
  });

describe.skipIf(!fs.existsSync(runtimeFile))("a launch in the real runtime", () => {
  it("sees the list a view command shows", async () => {
    const run = await launch("list");
    expect(run).toMatchObject({ outcome: "view", stopped: "settled", view: { root: "List", items: 2, titles: ["One", "Two"], isLoading: false, content: true } });
    expect(classifyCommand({ run }).status).toBe("works");
  });

  it("types into an empty list that searches as you type", async () => {
    const run = await launch("search");
    expect(run).toMatchObject({ outcome: "view", typed: "test", view: { items: 1, titles: ["Found test"] } });
  });

  it("answers the host's part and opens nothing", async () => {
    const run = await launch("opener");
    expect(run.outcome).toBe("finished");
    expect(run.host).toMatchObject({ "host.applications": 1, "host.open": 1, "host.hud": 1 });
    expect(run.effects).toContainEqual({ method: "host.open", params: expect.stringContaining("https://example.com/5") });
    expect(classifyCommand({ run }).status).toBe("works");
  });

  it("reports what a command throws, with where in the extension", async () => {
    const run = await launch("script");
    expect(run.error).toMatchObject({ message: expect.stringContaining("osascript"), raisedIn: "node", frame: expect.stringContaining("script.js") });
    expect(classifyCommand({ run })).toMatchObject({ status: "fails", reason: "macos-only", detail: "AppleScript" });
  });

  it("knows the runtime's error screen from a view", async () => {
    const run = await launch("broken");
    expect(run.view).toMatchObject({ errorScreen: true, content: false });
    expect(run.error).toMatchObject({ message: "it broke", raisedIn: "extension", frame: expect.stringContaining("broken.js") });
    expect(classifyCommand({ run })).toMatchObject({ status: "fails", reason: "extension-error" });
  });

  it("fills a required preference with a made-up value, in an environment that is not the user's", async () => {
    const run = await launch("secret");
    expect(run.preferences).toEqual([{ name: "token", type: "password", title: "token", scope: "command", secret: true }]);
    expect(run.view.excerpt).toMatch(/^token is heyo-compat-dummy-secret, home is .*heyo-compat-test-.*\/home, display is undefined$/);
    expect(classifyCommand({ run })).toMatchObject({ status: "configure", reason: "needs-preferences" });
  });

  it("goes on when the runtime process dies", async () => {
    const run = await launch("gone");
    expect(run).toMatchObject({ outcome: "crashed", exit: { signal: "SIGKILL" } });
    expect(classifyCommand({ run })).toMatchObject({ status: "fails", reason: "crash" });
  });
});
