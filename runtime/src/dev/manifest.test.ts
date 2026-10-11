import { describe, expect, it } from "vitest";

import { parseArgs } from "./main";
import { validateManifest } from "./manifest";
import { withEntry, withoutEntry, type Development } from "./registry";

const files = (...names: string[]) => (file: string) => names.includes(file);
const errors = (manifest: unknown, exists = files()) =>
  validateManifest(manifest, exists).problems.filter((p) => p.severity === "error").map((p) => p.message);
const warnings = (manifest: unknown, exists = files()) =>
  validateManifest(manifest, exists).problems.filter((p) => p.severity === "warning").map((p) => p.message);

const good = {
  name: "weather",
  title: "Weather",
  icon: "icon.png",
  commands: [
    { name: "today", title: "Today", mode: "view" },
    { name: "refresh", title: "Refresh", mode: "no-view", interval: "10m" },
    { name: "bar", title: "In the Menu Bar", mode: "menu-bar", icon: "bar.png" },
  ],
};
const present = files("assets/icon.png", "assets/bar.png", "src/today.tsx", "src/refresh.ts", "src/bar.jsx");

describe("a manifest", () => {
  it("that is in order has nothing said about it, and each command its source", () => {
    const { problems, commands } = validateManifest(good, present);
    expect(problems).toEqual([]);
    expect(commands).toEqual([
      { name: "today", entry: "src/today.tsx" },
      { name: "refresh", entry: "src/refresh.ts" },
      { name: "bar", entry: "src/bar.jsx" },
    ]);
  });

  it("prefers .tsx, then .ts, .jsx, .js, as Raycast does", () => {
    const { commands } = validateManifest(good, files("assets/icon.png", "assets/bar.png", "src/today.js", "src/today.ts", "src/refresh.ts", "src/bar.jsx"));
    expect(commands[0].entry).toBe("src/today.ts");
  });

  it("must name commands that exist", () => {
    expect(errors(good, files("assets/icon.png", "assets/bar.png", "src/today.tsx", "src/bar.jsx"))).toEqual([
      'command "refresh" has no source: expected src/refresh.tsx (or .ts, .jsx, .js)',
    ]);
    expect(errors({ ...good, commands: [] }, present)[0]).toContain('"commands" is missing or empty');
    expect(errors({ ...good, commands: undefined }, present)[0]).toContain('"commands" is missing or empty');
    expect(errors({ ...good, commands: [{ title: "Nameless", mode: "view" }] }, present)).toEqual(['commands[0] has no "name"']);
    expect(errors({ ...good, commands: [good.commands[0], good.commands[0]] }, present)).toEqual(['command "today" is declared twice']);
  });

  it("must use modes Heyo knows", () => {
    const odd = { ...good, commands: [{ name: "today", title: "Today", mode: "widget" }, { name: "refresh", title: "Refresh" }] };
    expect(errors(odd, present)).toEqual([
      'command "today" has the mode "widget", which Heyo does not know (view, no-view, menu-bar)',
      'command "refresh" has no "mode" (view, no-view, menu-bar)',
    ]);
  });

  it("must name icons that exist; the dark variant alone will do", () => {
    expect(errors(good, files("src/today.tsx", "src/refresh.ts", "src/bar.jsx"))).toEqual([
      "the icon assets/icon.png does not exist",
      'command "bar": the icon assets/bar.png does not exist',
    ]);
    expect(errors(good, files("assets/icon@dark.png", "assets/bar@dark.png", "src/today.tsx", "src/refresh.ts", "src/bar.jsx"))).toEqual([]);
    expect(warnings({ ...good, icon: undefined }, present)[0]).toContain('"icon" is missing');
  });

  it("must have a name that can be a directory", () => {
    expect(errors({ ...good, name: "../escape" }, present)[0]).toContain('"name" must be');
    expect(errors({ ...good, name: undefined }, present)).toEqual(['"name" is missing']);
    expect(errors({ ...good, commands: [{ name: "a/b", title: "T", mode: "view" }] }, present)[0]).toContain("becomes a file name");
    expect(errors([], present)).toEqual(["package.json is not an object"]);
  });

  it("is warned about what Heyo reads differently", () => {
    const odd = {
      ...good,
      commands: [
        { name: "today", title: "Today", mode: "view", interval: "5m", arguments: [{ name: "city", type: "date" }] },
        { name: "refresh", title: "Refresh", mode: "no-view", interval: "soon" },
      ],
    };
    expect(warnings(odd, present)).toEqual([
      'command "today": a command with a view does not run on an interval',
      'command "today": argument "city" has the type "date"; Heyo shows it as text',
      'command "refresh": "interval" should look like 30s, 10m, 1h or 1d',
    ]);
  });

  it("has its heyo field checked", () => {
    expect(validateManifest({ ...good, heyo: { apiVersion: 1, permissions: ["windows", "ai", "activity"] } }, present).problems).toEqual([]);
    expect(errors({ ...good, heyo: { apiVersion: 2 } }, present)).toEqual(['"heyo.apiVersion" is 2, but this Heyo provides version 1']);
    expect(errors({ ...good, heyo: { apiVersion: "1" } }, present)[0]).toContain("whole number");
    expect(errors({ ...good, heyo: { apiVersion: 1, permissions: "windows" } }, present)).toEqual(['"heyo.permissions" must be a list']);
    expect(warnings({ ...good, heyo: { apiVersion: 1, permissions: ["telepathy"] } }, present)[0]).toContain('"telepathy"');
  });
});

describe("the command line", () => {
  it("reads the four commands", () => {
    expect(parseArgs(["build"])).toEqual({ command: "build", dir: ".", out: undefined, sourcemap: false });
    expect(parseArgs(["build", "ext", "-o", "/tmp/out", "--sourcemap"])).toEqual({ command: "build", dir: "ext", out: "/tmp/out", sourcemap: true });
    expect(parseArgs(["dev", "ext"])).toEqual({ command: "dev", dir: "ext" });
    expect(parseArgs(["dev", "--remove", "ext"])).toEqual({ command: "dev-remove", key: "ext" });
    expect(parseArgs(["dev", "--list"])).toEqual({ command: "dev-list" });
    expect(parseArgs(["migrate", "--dry-run", "ext"])).toEqual({ command: "migrate", dir: "ext", dryRun: true });
    expect(parseArgs(["ext", "check", "ext", "--json"])).toEqual({ command: "check", dir: "ext", json: true });
    expect(parseArgs(["ext", "check"])).toEqual({ command: "check", dir: ".", json: false });
  });

  it("says what it did not understand", () => {
    expect(parseArgs([])).toEqual({ command: "help" });
    expect(parseArgs(["build", "--help"])).toEqual({ command: "help" });
    expect(parseArgs(["deploy"])).toEqual({ command: "usage", problem: "Unknown command: deploy" });
    expect(parseArgs(["ext"])).toMatchObject({ command: "usage" });
    expect(parseArgs(["ext", "publish"])).toEqual({ command: "usage", problem: "Unknown command: ext publish" });
    expect(parseArgs(["check"])).toMatchObject({ command: "usage" });
    expect(parseArgs(["migrate", "--json"])).toEqual({ command: "usage", problem: "Unknown option for migrate: --json" });
    expect(parseArgs(["build", "-o"])).toEqual({ command: "usage", problem: "-o needs a value" });
    expect(parseArgs(["build", "a", "b"])).toMatchObject({ command: "usage" });
  });
});

describe("the list of extensions in development", () => {
  const a: Development = { name: "a", source: "/src/a", dir: "/data/development/a" };
  const b: Development = { name: "b", source: "/src/b", dir: "/data/development/b" };

  it("has one entry per extension and per source directory", () => {
    expect(withEntry([a], b)).toEqual([a, b]);
    expect(withEntry([a, b], { ...a, source: "/elsewhere/a" })).toEqual([b, { ...a, source: "/elsewhere/a" }]);
    // Renamed in its manifest: the old name's entry goes.
    expect(withEntry([a, b], { name: "c", source: "/src/a", dir: "/data/development/c" }).map((e) => e.name)).toEqual(["b", "c"]);
  });

  it("gives one up by its name, its source or where it is built", () => {
    expect(withoutEntry([a, b], "a")).toEqual({ kept: [b], removed: [a] });
    expect(withoutEntry([a, b], "/src/b")).toEqual({ kept: [a], removed: [b] });
    expect(withoutEntry([a, b], "/data/development/a")).toEqual({ kept: [b], removed: [a] });
    expect(withoutEntry([a, b], "/src/c")).toEqual({ kept: [a, b], removed: [] });
  });
});
