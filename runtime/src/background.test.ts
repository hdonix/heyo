// The background process: commands side by side, each with its own context,
// and menu bar commands that stay mounted. The extensions are real files,
// loaded and rendered as the runtime does it.
import fs from "node:fs";
import path from "node:path";
import { beforeAll, beforeEach, expect, test, vi } from "vitest";

import { answers, extension, provideModules, receive, requests, reset, sent, sleep, supportDir, until } from "./testing";

vi.mock("./protocol", async () => (await import("./testing")).protocol);

import { limits, mountedKeys, startBackground, unmountAll } from "./background";

const defaults = { ...limits };
const world = globalThis as any;
let nextRun = 0;

/// Runs a command as the host asks for it; resolves with its `background-done`.
function run(dir: string, command: string, more: Record<string, unknown> = {}) {
  const id = ++nextRun;
  receive({ type: "background-run", id, extension: dir, command, launchType: "background", action: null, ...more });
  return until(() => sent.find((m) => m.type === "background-done" && m.id === id), `${command} to be done`);
}

const menus = (dir: string, command: string) => sent.filter((m) => m.type === "menu" && m.key === `${dir}:${command}`).map((m) => m.menu);

/// A no-view command that says who it is between waits, and once more after it is over.
const reporter = (who: string) => `
  const api = require("@raycast/api");
  exports.default = async () => {
    const say = (when) => globalThis.seen.push({
      who: "${who}", when,
      extension: api.environment.extensionName,
      command: api.environment.commandName,
      token: api.getPreferenceValues().token,
      support: api.environment.supportPath,
    });
    for (let i = 0; i < 4; i++) {
      await new Promise((resolve) => setTimeout(resolve, 5));
      say("running");
      await api.LocalStorage.setItem("owner", "${who}");
      await api.LocalStorage.setItem("${who}", i);
    }
    setTimeout(() => say("late"), 40);
  };
`;

/// A menu bar command: its title says who it is (as its render, its effect
/// and a handler see it) and how often its item was clicked.
const menu = (who: string) => `
  const api = require("@raycast/api");
  const React = require("react");
  globalThis.loads["${who}"] = (globalThis.loads["${who}"] ?? 0) + 1;
  exports.default = () => {
    const [clicks, setClicks] = React.useState(0);
    const [tick, setTick] = React.useState(0);
    const [inEffect, setInEffect] = React.useState("");
    React.useEffect(() => {
      const listener = () => setTick((t) => t + 1);
      globalThis.bus.add(listener);
      return () => globalThis.bus.delete(listener);
    }, []);
    React.useEffect(() => {
      globalThis.seen.push({ who: "${who}", when: "effect", extension: api.environment.extensionName, token: api.getPreferenceValues().token });
      setInEffect(api.environment.extensionName);
    }, [tick]);
    globalThis.seen.push({ who: "${who}", when: "render", extension: api.environment.extensionName, token: api.getPreferenceValues().token });
    return React.createElement(api.MenuBarExtra, { title: api.environment.extensionName + " " + inEffect + " " + clicks + " " + tick },
      React.createElement(api.MenuBarExtra.Section, { title: "Section" },
        React.createElement(api.MenuBarExtra.Item, {
          title: "Count",
          onAction: async () => {
            await new Promise((resolve) => setTimeout(resolve, 1));
            globalThis.seen.push({ who: "${who}", when: "action", extension: api.environment.extensionName, token: api.getPreferenceValues().token });
            await api.LocalStorage.setItem("clicked", "${who}");
            setClicks((c) => c + 1);
          },
        }),
      ),
    );
  };
`;

beforeAll(async () => {
  await provideModules();
  startBackground();
});

beforeEach(async () => {
  // Let what the test before left behind (unmounts, late timers) run out.
  unmountAll();
  await sleep(60);
  Object.assign(limits, defaults);
  reset();
  world.seen = [];
  world.loads = {};
  world.bus = new Set();
});

const storage = (name: string) => JSON.parse(fs.readFileSync(path.join(supportDir(name), "local-storage.json"), "utf8"));

test("two commands running at once each see their own extension", async () => {
  const alpha = extension("alpha", { sync: { mode: "no-view", source: reporter("alpha") } });
  const beta = extension("beta", { sync: { mode: "no-view", source: reporter("beta") } });
  const done = await Promise.all([run(alpha.dir, "sync"), run(beta.dir, "sync")]);
  expect(done.map((d) => d.error)).toEqual([undefined, undefined]);
  await until(() => world.seen.filter((s: any) => s.when === "late").length === 2, "the late timers");

  // They did interleave: neither finished before the other started.
  const order = world.seen.filter((s: any) => s.when === "running").map((s: any) => s.who);
  expect(order.indexOf("beta")).toBeLessThan(order.lastIndexOf("alpha"));
  expect(order.indexOf("alpha")).toBeLessThan(order.lastIndexOf("beta"));

  expect(world.seen).toHaveLength(10);
  for (const s of world.seen) {
    expect(s, `${s.who} ${s.when}`).toMatchObject({ extension: s.who, command: "sync", token: `token-of-${s.who}`, support: supportDir(s.who) });
  }
  expect(storage("alpha")).toEqual({ owner: "alpha", alpha: 3 });
  expect(storage("beta")).toEqual({ owner: "beta", beta: 3 });
});

test("menu bar commands mounted together keep their own context in renders, effects and handlers", async () => {
  const alpha = extension("alpha", { bar: { mode: "menu-bar", source: menu("alpha") } });
  const beta = extension("beta", { bar: { mode: "menu-bar", source: menu("beta") } });
  await Promise.all([run(alpha.dir, "bar"), run(beta.dir, "bar")]);
  expect(mountedKeys()).toEqual([`${alpha.dir}:bar`, `${beta.dir}:bar`]);
  expect(menus(alpha.dir, "bar").at(-1)).toMatchObject({ title: "alpha alpha 0 0", items: [{ kind: "section", children: [{ kind: "item", title: "Count", enabled: true }] }] });
  expect(menus(beta.dir, "bar").at(-1).title).toBe("beta beta 0 0");

  // Both render again in one turn that belongs to neither.
  world.seen = [];
  for (const listener of world.bus) listener();
  await until(() => menus(alpha.dir, "bar").at(-1).title === "alpha alpha 0 1" && menus(beta.dir, "bar").at(-1).title === "beta beta 0 1", "both menus to change");
  expect(world.seen.map((s: any) => s.when)).toEqual(expect.arrayContaining(["render", "effect"]));
  for (const s of world.seen) expect(s, `${s.who} ${s.when}`).toMatchObject({ extension: s.who, token: `token-of-${s.who}` });

  // And a click on each, at once.
  world.seen = [];
  await Promise.all([run(alpha.dir, "bar", { action: [0, 0] }), run(beta.dir, "bar", { action: [0, 0] })]);
  for (const s of world.seen) expect(s, `${s.who} ${s.when}`).toMatchObject({ extension: s.who, token: `token-of-${s.who}` });
  expect(world.seen.filter((s: any) => s.when === "action")).toHaveLength(2);
  expect(storage("alpha").clicked).toBe("alpha");
  expect(storage("beta").clicked).toBe("beta");
});

test("a click calls the mounted command's action instead of running the command again", async () => {
  const gamma = extension("gamma", { bar: { mode: "menu-bar", source: menu("gamma") } });
  await run(gamma.dir, "bar");
  expect(world.loads.gamma).toBe(1);

  const done = await run(gamma.dir, "bar", { action: [0, 0], launchType: "userInitiated" });
  expect(done.error).toBeUndefined();
  // The command's own state changed, and its new menu follows.
  await until(() => menus(gamma.dir, "bar").at(-1).title === "gamma gamma 1 0", "the menu after the click");
  await run(gamma.dir, "bar", { action: [0, 0] });
  await until(() => menus(gamma.dir, "bar").at(-1).title === "gamma gamma 2 0", "the menu after the second click");
  expect(world.loads.gamma).toBe(1);

  // A run without an action (the schedule, Refresh) starts the command afresh.
  await run(gamma.dir, "bar");
  expect(world.loads.gamma).toBe(2);
  expect(menus(gamma.dir, "bar").at(-1).title).toBe("gamma gamma 0 0");
  expect(mountedKeys()).toEqual([`${gamma.dir}:bar`]);
});

test("a menu that did not change is not sent again", async () => {
  const delta = extension("delta", {
    bar: {
      mode: "menu-bar",
      source: `
        const api = require("@raycast/api");
        const React = require("react");
        exports.default = () => {
          const [, setTick] = React.useState(0);
          React.useEffect(() => void globalThis.bus.add(() => setTick((t) => t + 1)), []);
          return React.createElement(api.MenuBarExtra, { title: "same" });
        };`,
    },
  });
  await run(delta.dir, "bar");
  for (const listener of world.bus) listener();
  await sleep(150);
  expect(menus(delta.dir, "bar")).toHaveLength(1);
});

test("a command that renders without pause still gets its menu, at a measured pace", async () => {
  limits.menuGap = 150;
  const busy = extension("busy", {
    bar: {
      mode: "menu-bar",
      source: `
        const api = require("@raycast/api");
        const React = require("react");
        exports.default = () => {
          const [tick, setTick] = React.useState(0);
          React.useEffect(() => {
            const timer = setInterval(() => setTick((t) => t + 1), 10);
            return () => clearInterval(timer);
          }, []);
          return React.createElement(api.MenuBarExtra, { title: String(tick) });
        };`,
    },
  });
  expect((await run(busy.dir, "bar")).error).toBeUndefined();
  await sleep(700);
  // About one per gap, though it rendered some seventy times.
  const count = menus(busy.dir, "bar").length;
  expect(count).toBeGreaterThanOrEqual(3);
  expect(count).toBeLessThanOrEqual(7);
});

test("only so many commands stay mounted, and one that is not still answers a click", async () => {
  limits.mounted = 1;
  const one = extension("one", { bar: { mode: "menu-bar", source: menu("one") } });
  const two = extension("two", { bar: { mode: "menu-bar", source: menu("two") } });
  await run(one.dir, "bar");
  await run(two.dir, "bar");
  expect(mountedKeys()).toEqual([`${two.dir}:bar`]);

  // The old way: the command is run, then the item at the path is called.
  const done = await run(one.dir, "bar", { action: [0, 0], launchType: "userInitiated" });
  expect(done.error).toBeUndefined();
  expect(world.loads.one).toBe(2);
  await until(() => menus(one.dir, "bar").at(-1).title === "one one 1 0", "the menu after the click");
  expect(mountedKeys()).toEqual([`${one.dir}:bar`]);
});

test("a mounted command nobody uses is unmounted after a while", async () => {
  limits.idle = 40;
  const idle = extension("idle", { bar: { mode: "menu-bar", source: menu("idle") } });
  await run(idle.dir, "bar");
  expect(mountedKeys()).toEqual([`${idle.dir}:bar`]);
  await until(() => mountedKeys().length === 0, "the command to be unmounted");
  // Its effects were cleaned up: it no longer listens.
  await until(() => world.bus.size === 0, "the effect's cleanup");
});

test("a command that never settles is given up, and holds up nobody else", async () => {
  limits.timeout = 80;
  const stuck = extension("stuck", { hang: { mode: "no-view", source: `exports.default = () => new Promise(() => {});` } });
  const quick = extension("quick", { go: { mode: "no-view", source: `exports.default = async () => { globalThis.seen.push("quick"); };` } });
  const hanging = run(stuck.dir, "hang");
  const done = await run(quick.dir, "go");
  expect(done.error).toBeUndefined();
  expect(world.seen).toEqual(["quick"]);
  expect(sent.some((m) => m.type === "background-done" && m.id !== done.id)).toBe(false);
  expect((await hanging).error).toMatch(/still running after/);
  // Its next run is not behind the one that hangs.
  expect((await run(stuck.dir, "hang")).error).toMatch(/still running after/);
});

test("a menu bar command that never renders a menu is given up and unmounted", async () => {
  limits.timeout = 80;
  const blank = extension("blank", {
    bar: { mode: "menu-bar", source: `const React = require("react"); exports.default = () => { React.use(new Promise(() => {})); };` },
  });
  expect((await run(blank.dir, "bar")).error).toMatch(/still running after/);
  expect(mountedKeys()).toEqual([]);
});

test("a background command does not move the palette", async () => {
  const mover = extension("mover", {
    go: {
      mode: "no-view",
      source: `
        const api = require("@raycast/api");
        exports.default = async () => {
          await api.closeMainWindow();
          await api.popToRoot();
          await api.clearSearchBar();
          await api.showHUD("Synced");
        };`,
    },
  });
  expect((await run(mover.dir, "go")).error).toBeUndefined();
  // The HUD is still told; nothing hides or resets the palette.
  expect(requests.map((r) => r.method)).toEqual(["host.hud"]);
  expect(sent.filter((m) => m.type !== "background-done")).toEqual([]);
});

test("a sign-in started by a background command hears back, without bringing the palette up", async () => {
  const signer = extension("signer", {
    sync: {
      mode: "no-view",
      source: `
        const api = require("@raycast/api");
        exports.default = async () => {
          const client = new api.OAuth.PKCEClient({ redirectMethod: api.OAuth.RedirectMethod.Web, providerName: "Probe" });
          const request = await client.authorizationRequest({ endpoint: "https://example.com/authorize", clientId: "id", scope: "read" });
          globalThis.oauthState = request.state;
          const { authorizationCode } = await client.authorize(request);
          await client.setTokens({ accessToken: authorizationCode });
          globalThis.seen.push({ code: authorizationCode, extension: api.environment.extensionName });
        };`,
    },
  });
  const done = run(signer.dir, "sync");
  await until(() => requests.some((r) => r.method === "host.open"), "the browser to be opened");
  // The host passes the redirect to both runtime processes; the state says whose it is.
  receive({ type: "oauth-callback", url: "raycast://oauth?package_name=Extension&code=other&state=someone-else" });
  receive({ type: "oauth-callback", url: `raycast://oauth?package_name=Extension&code=abc&state=${world.oauthState}` });
  expect((await done).error).toBeUndefined();
  expect(world.seen).toEqual([{ code: "abc", extension: "signer" }]);
  expect(requests.map((r) => r.method)).toEqual(["host.open"]);
  expect(JSON.parse(fs.readFileSync(path.join(supportDir("signer"), "oauth-Probe.json"), "utf8")).accessToken).toBe("abc");
});

const launcher = (options: string) => `
  const api = require("@raycast/api");
  exports.default = async () => { await api.launchCommand(${options}); };
`;
const marker = (what: string) => `
  const api = require("@raycast/api");
  exports.default = async (props) => {
    globalThis.seen.push({ what: "${what}", extension: api.environment.extensionName, launchType: props.launchType, arguments: props.arguments, context: props.launchContext });
  };
`;

test("launchCommand runs a background command of the same extension here", async () => {
  const own = extension("own", {
    start: { mode: "no-view", source: launcher(`{ name: "work", type: api.LaunchType.Background, arguments: { city: "Oslo" }, context: { n: 1 } }`) },
    work: { mode: "no-view", source: marker("work") },
  });
  expect((await run(own.dir, "start")).error).toBeUndefined();
  await until(() => world.seen.length === 1, "the launched command");
  expect(world.seen[0]).toEqual({ what: "work", extension: "own", launchType: "background", arguments: { city: "Oslo" }, context: { n: 1 } });
  expect(requests).toEqual([]);
});

test("launchCommand checks its target before doing anything", async () => {
  const own = extension("own", {
    missing: { mode: "no-view", source: launcher(`{ name: "nowhere" }`) },
    stranger: { mode: "no-view", source: launcher(`{ name: "work", extensionName: "not-installed", ownerOrAuthorName: "nobody" }`) },
    view: { mode: "view", source: `exports.default = () => null;` },
    viewInBackground: { mode: "no-view", source: launcher(`{ name: "view", type: api.LaunchType.Background }`) },
    viewFromInterval: { mode: "no-view", source: launcher(`{ name: "view" }`) },
  });
  expect((await run(own.dir, "missing")).error).toMatch(/has no command nowhere/);
  expect((await run(own.dir, "stranger")).error).toMatch(/nobody\/not-installed is not installed/);
  expect((await run(own.dir, "viewInBackground")).error).toMatch(/cannot be launched in the background/);
  // Opening the palette is for the user's click, not for a scheduled run.
  expect((await run(own.dir, "viewFromInterval")).error).toMatch(/only when the user clicks/);
  expect(requests).toEqual([]);

  // Asked for by the user, the host opens it in the palette.
  expect((await run(own.dir, "viewFromInterval", { launchType: "userInitiated" })).error).toBeUndefined();
  expect(requests).toEqual([{ method: "host.launchCommand", params: { extension: own.dir, command: "view", background: false, arguments: undefined, context: undefined, fallbackText: undefined } }]);
});

test("another extension's command is launched from the background only once that was allowed", async () => {
  extension("target", { work: { mode: "no-view", source: marker("target work") } });
  const caller = extension("caller", {
    start: { mode: "no-view", source: launcher(`{ name: "work", type: api.LaunchType.Background, extensionName: "target", ownerOrAuthorName: "tester" }`) },
  });
  // Nobody can be asked here: the dialog is not even tried.
  answers["ui.confirm"] = () => true;
  expect((await run(caller.dir, "start")).error).toMatch(/caller is not allowed to launch commands of target/);
  expect(requests).toEqual([]);
  expect(world.seen).toEqual([]);

  // What the user's answer in the palette leaves behind.
  fs.writeFileSync(path.join(supportDir("caller"), "launch-permissions.json"), JSON.stringify(["target"]));
  expect((await run(caller.dir, "start")).error).toBeUndefined();
  await until(() => world.seen.length === 1, "the launched command");
  expect(world.seen[0]).toMatchObject({ what: "target work", extension: "target", launchType: "background" });
});
