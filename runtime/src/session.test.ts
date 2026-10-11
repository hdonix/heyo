// The palette process: one command at a time on screen, but one that
// finishes late is still itself and no longer has the palette.
import fs from "node:fs";
import { createRequire } from "node:module";
import path from "node:path";
import v8 from "node:v8";
import vm from "node:vm";
import { beforeAll, beforeEach, expect, test } from "vitest";

import { answers, extension, requests, reset, runtime, sent, sleep, supportDir, until } from "./testing";

import { activeSession, close, launch, loadModule } from "./session";

const world = globalThis as any;

beforeAll(() => runtime());

beforeEach(async () => {
  close();
  await sleep(30);
  reset();
  world.seen = [];
});

const launched = () => sent.filter((m) => m.type === "launched").map((m) => m.extension.name);
const methods = () => requests.map((r) => r.method);

const show = `
  const api = require("@raycast/api");
  const React = require("react");
  exports.default = () => React.createElement(api.Detail, { markdown: api.environment.extensionName });
`;

test("a command that finishes after the next one started is still itself, and leaves the palette alone", async () => {
  const alpha = extension("alpha", {
    slow: {
      mode: "no-view",
      source: `
        const api = require("@raycast/api");
        exports.default = async () => {
          await globalThis.gate;
          globalThis.seen.push({ extension: api.environment.extensionName, command: api.environment.commandName, token: api.getPreferenceValues().token });
          await api.LocalStorage.setItem("owner", "alpha");
          await api.closeMainWindow();
          await api.popToRoot();
          await api.showHUD("Done, late");
          await api.launchCommand({ name: "slow" }).catch((e) => globalThis.seen.push(e.message));
        };`,
    },
  });
  const beta = extension("beta", { show: { mode: "view", source: show } });
  let open = () => {};
  world.gate = new Promise<void>((resolve) => (open = resolve));

  launch(alpha, "slow");
  await sleep(20);
  launch(beta, "show");
  const second = activeSession()!.id;
  await until(() => sent.some((m) => m.type === "render" && m.session === second && m.view), "the second command's view");
  open();
  await until(() => world.seen.length === 2, "the first command to finish");

  expect(world.seen[0]).toEqual({ extension: "alpha", command: "slow", token: "token-of-alpha" });
  expect(JSON.parse(fs.readFileSync(path.join(supportDir("alpha"), "local-storage.json"), "utf8"))).toEqual({ owner: "alpha" });
  expect(fs.existsSync(path.join(supportDir("beta"), "local-storage.json"))).toBe(false);
  // The HUD is told; the palette is neither hidden nor reset, and the
  // command in it is still there.
  expect(methods()).toEqual(["host.hud"]);
  expect(sent.some((m) => m.type === "pop-to-root" || m.type === "closed")).toBe(false);
  expect(world.seen[1]).toMatch(/no longer the one in the palette/);
  expect(activeSession()?.id).toBe(second);
  expect(launched()).toEqual(["alpha", "beta"]);
});

test("the command in the palette may hide it", async () => {
  const hider = extension("hider", {
    hide: { mode: "no-view", source: `const api = require("@raycast/api"); exports.default = async () => { await api.showHUD("Copied"); };` },
  });
  launch(hider, "hide");
  await until(() => sent.some((m) => m.type === "closed"), "the command to end");
  expect(methods()).toEqual(["host.hide", "host.hud"]);
});

test("a launch that cannot be leaves the command on screen", async () => {
  const beta = extension("beta", { show: { mode: "view", source: show }, bar: { mode: "menu-bar", source: show } });
  launch(beta, "show");
  const session = activeSession();
  expect(() => launch(beta, "nowhere")).toThrow(/Unknown command/);
  expect(() => launch(beta, "bar")).toThrow(/menu bar/);
  expect(activeSession()).toBe(session);
});

const go = (options: string) => `
  const api = require("@raycast/api");
  exports.default = async () => { await api.launchCommand(${options}); };
`;

test("launching another extension's command asks the user once", async () => {
  extension("beta", { show: { mode: "view", source: show } });
  const caller = extension("caller", { go: { mode: "no-view", source: go(`{ name: "show", extensionName: "beta", ownerOrAuthorName: "tester" }`) } });
  const permissions = path.join(supportDir("caller"), "launch-permissions.json");
  const asked = () => requests.filter((r) => r.method === "ui.confirm");

  answers["ui.confirm"] = () => false;
  launch(caller, "go");
  await until(() => sent.some((m) => m.type === "closed"), "the refused launch to end");
  expect(asked()).toHaveLength(1);
  expect(asked()[0].params.title).toBe("Allow caller to launch commands of beta?");
  expect(launched()).toEqual(["caller"]);
  expect(sent.find((m) => m.type === "toast").toast.message).toMatch(/not allowed to launch commands of beta/);
  expect(fs.existsSync(permissions)).toBe(false);

  answers["ui.confirm"] = () => true;
  launch(caller, "go");
  await until(() => launched().includes("beta"), "the allowed launch");
  expect(asked()).toHaveLength(2);
  expect(JSON.parse(fs.readFileSync(permissions, "utf8"))).toEqual(["beta"]);

  // Remembered: not asked again.
  launch(caller, "go");
  await until(() => launched().filter((name) => name === "beta").length === 2, "the second launch");
  expect(asked()).toHaveLength(2);
});

test("a menu bar or background command is launched through the host", async () => {
  const own = extension("own", {
    bar: { mode: "menu-bar", source: show },
    work: { mode: "no-view", source: `exports.default = async () => {};` },
    refresh: { mode: "no-view", source: go(`{ name: "bar", type: api.LaunchType.UserInitiated }`) },
    start: { mode: "no-view", source: go(`{ name: "work", type: api.LaunchType.Background, arguments: { a: "1" } }`) },
  });
  launch(own, "refresh");
  await until(() => sent.some((m) => m.type === "closed"), "the launcher to end");
  launch(own, "start");
  await until(() => sent.filter((m) => m.type === "closed").length === 2, "the second launcher to end");
  expect(requests).toEqual([
    { method: "host.launchCommand", params: { extension: own.dir, command: "bar", background: false, arguments: undefined, context: undefined, fallbackText: undefined } },
    { method: "host.launchCommand", params: { extension: own.dir, command: "work", background: true, arguments: { a: "1" }, context: undefined, fallbackText: undefined } },
  ]);
  // Nothing of it ran here.
  expect(launched()).toEqual(["own", "own"]);
});

// Launching evaluates a command's file anew each time. Interval commands do
// that for as long as Heyo runs, so an evaluation must be freed once its
// command is done. Each evaluation of this module holds about 2 MB.
test("a command's module is freed after it was loaded again", () => {
  const { dir } = extension("heavy", {
    work: { mode: "no-view", source: `exports.held = new Array(250_000).fill(0).map((_, i) => i + 0.5); exports.default = () => {};` },
  });
  const file = path.join(dir, "work.js");
  v8.setFlagsFromString("--expose-gc");
  const collect = vm.runInNewContext("gc") as () => void;
  const used = () => {
    collect();
    return process.memoryUsage().heapUsed;
  };
  const megabytes = (bytes: number) => Math.round(bytes / 1024 / 1024);
  const ROUNDS = 100;

  // How it was done: out of the cache, and required again.
  const require = createRequire(__filename);
  let before = used();
  for (let i = 0; i < ROUNDS; i++) {
    delete require.cache[file];
    require(file);
  }
  const old = megabytes(used() - before);

  before = used();
  for (let i = 0; i < ROUNDS; i++) loadModule(file);
  const now = megabytes(used() - before);

  console.log(`heap kept after loading a 2 MB module ${ROUNDS} times: ${old} MB with delete+require, ${now} MB with loadModule`);
  expect(old).toBeGreaterThan(100);
  expect(now).toBeLessThan(10);
});
