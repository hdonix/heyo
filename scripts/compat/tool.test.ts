import { describe, expect, it } from "vitest";

import { dummyArguments, dummyPreferences, dummyValue, requiredPreferences } from "./preferences.mjs";
import { renderHtml, renderMarkdown } from "./report.mjs";
import { insideHeyo, parseOptions, scrub } from "./run.mjs";
import { bwrapArgs, environment, layout } from "./sandbox.mjs";
import { mostDownloaded, USER_AGENT } from "./store.mjs";

describe("what a command must be given", () => {
  const manifest = {
    name: "demo",
    preferences: [
      { name: "token", type: "password", required: true, title: "API Token" },
      { name: "serverUrl", type: "textfield", required: true },
      { name: "limit", type: "textfield", required: true, default: "20" },
      { name: "compact", type: "checkbox", required: true },
      { name: "sort", type: "dropdown", required: true, data: [{ title: "Name", value: "name" }] },
      { name: "nickname", type: "textfield" },
    ],
    commands: [
      { name: "list", mode: "view" },
      { name: "open", mode: "no-view", preferences: [{ name: "editor", type: "appPicker", required: true }], arguments: [{ name: "query", type: "text" }, { name: "hours", type: "text" }, { name: "kind", type: "dropdown", data: [{ title: "A", value: "a" }] }] },
    ],
  };

  it("is what Heyo would ask for: required, and without a default", () => {
    expect(requiredPreferences(manifest, "list").map((p) => p.name)).toEqual(["token", "serverUrl"]);
    expect(requiredPreferences(manifest, "open")).toEqual([
      { name: "token", type: "password", title: "API Token", scope: "extension", secret: true },
      { name: "serverUrl", type: "textfield", title: "serverUrl", scope: "extension", secret: false },
      { name: "editor", type: "appPicker", title: "editor", scope: "command", secret: false },
    ]);
  });

  it("makes up values of the shape each preference wants", () => {
    expect(dummyValue({ name: "serverUrl", type: "textfield" })).toBe("https://example.invalid");
    expect(dummyValue({ name: "email", type: "textfield" })).toBe("user@example.com");
    expect(dummyValue({ name: "vaultPath", type: "textfield" }, "/h")).toBe("/h/Documents");
    expect(dummyValue({ name: "dir", type: "directory" }, "/h")).toBe("/h/Documents");
    expect(dummyValue({ name: "max", type: "textfield", title: "Max results limit" })).toBe("10");
    expect(dummyValue({ name: "name", type: "textfield" })).toBe("heyo-compat");
    expect(dummyValue({ name: "key", type: "password" })).toBe("heyo-compat-dummy-secret");
  });

  it("writes them where the runtime reads an extension's preferences", () => {
    const { stored, required } = dummyPreferences(manifest, "open", "/h");
    expect(stored).toEqual({
      extension: { token: "heyo-compat-dummy-secret", serverUrl: "https://example.invalid" },
      commands: { open: { editor: "/usr/share/applications/firefox.desktop" } },
    });
    expect(required).toHaveLength(3);
    expect(dummyPreferences({ name: "x", commands: [{ name: "a" }] }, "a").required).toEqual([]);
  });

  it("gives each argument something that could have been typed", () => {
    expect(dummyArguments(manifest.commands[1])).toEqual({ query: "test", hours: "1", kind: "a" });
    expect(dummyArguments(manifest.commands[0])).toBeUndefined();
  });
});

describe("how an extension is kept from the user's things", () => {
  const dirs = layout("/scratch/run");

  it("hands over an environment with nothing of the user's", () => {
    const env = environment(dirs, { extensionsDir: "/scratch/extensions" });
    expect(Object.keys(env).sort()).toEqual(
      ["HEYO_CACHE_DIR", "HEYO_CONFIG_DIR", "HEYO_DATA_DIR", "HEYO_EXTENSIONS_DIR", "HOME", "LANG", "LOGNAME", "PATH", "SHELL", "TMPDIR", "TZ", "USER", "XDG_CACHE_HOME", "XDG_CONFIG_HOME", "XDG_DATA_HOME", "XDG_RUNTIME_DIR", "XDG_STATE_HOME"].sort(),
    );
    for (const [name, value] of Object.entries(env)) {
      if (/^(HOME|TMPDIR|XDG_|HEYO_)/.test(name)) expect(value.startsWith("/scratch/")).toBe(true);
    }
    expect(environment(dirs, { background: true }).HEYO_BACKGROUND).toBe("1");
  });

  it("cuts the network unless it is asked for, and binds only what is named", () => {
    const base = { command: ["/usr/bin/node", "x.cjs"], readOnly: ["/repo/runtime/dist"], writable: ["/scratch/run"], exists: () => true };
    const cut = bwrapArgs(base);
    expect(cut).toContain("--unshare-all");
    expect(cut).not.toContain("--share-net");
    expect(bwrapArgs({ ...base, network: true })).toContain("--share-net");
    expect(cut.slice(-3)).toEqual(["--", "/usr/bin/node", "x.cjs"]);
    // Every bind, with where from: the system, a few files of /etc, and
    // the two paths given. Nothing of /home.
    const binds: string[] = [];
    cut.forEach((arg: string, i: number) => {
      if (arg === "--bind" || arg === "--ro-bind" || arg === "--ro-bind-try") binds.push(`${arg} ${cut[i + 1]}`);
    });
    expect(binds.filter((bind) => !bind.includes(" /etc/"))).toEqual(["--ro-bind /usr", "--ro-bind /repo/runtime/dist", "--bind /scratch/run"]);
    expect(cut.join(" ")).not.toMatch(/\/home\b/);
    expect(cut).toContain("--die-with-parent");
    expect(cut).toContain("--new-session");
  });
});

describe("the sample", () => {
  const listing = (name: string, download_count: number, more: object = {}) => ({ name, download_count, owner: { handle: "o" }, status: "active", ...more });

  it("is the most downloaded, the most first, whatever the Store's order", () => {
    const picked = mostDownloaded([listing("b", 5), listing("a", 50), listing("c", 20), listing("d", 1)], 3);
    expect(picked.map((l: { name: string }) => l.name)).toEqual(["a", "c", "b"]);
  });

  it("keeps the Store's order between equals, and leaves out repeats and what is not active", () => {
    const picked = mostDownloaded([listing("x", 5), listing("y", 5), listing("x", 5), listing("z", 99, { status: "deprecated" })], 10);
    expect(picked.map((l: { name: string }) => l.name)).toEqual(["x", "y"]);
  });

  it("asks the Store as the app does", () => {
    expect(USER_AGENT).toBe("node");
  });
});

describe("the options", () => {
  it("has a hundred extensions and a cache outside Heyo by default", () => {
    const options = parseOptions([]);
    expect(options.count).toBe(100);
    expect(options.network).toBe("on");
    expect(insideHeyo(options.cache)).toBe(false);
  });

  it("reads what is given", () => {
    expect(parseOptions(["--count", "250", "--network", "both", "--only", "a,b", "--force", "--report-only"])).toMatchObject({ count: 250, network: "both", only: ["a", "b"], force: true, reportOnly: true });
    expect(() => parseOptions(["--count", "many"])).toThrow(/number/);
    expect(() => parseOptions(["--network", "maybe"])).toThrow(/on, off or both/);
    expect(() => parseOptions(["--nope"])).toThrow(/unknown option/);
  });

  it("takes what describes this machine out of a record", () => {
    const record = {
      ms: 1500,
      toasts: [{ title: "Failed", message: "Command failed: /c/work/4242/audio-device/runs/use-combo1-net/heyo/data/support/x get" }],
      error: { message: "ENOENT '/c/work/4242/demo/extensions/demo/assets/tool'" },
      view: { titles: ["192.0.2.15", "fe80:0:0:0:1c2d:3eff:fe4a:5b6c", "aa:bb:cc:dd:ee:ff", "version 1.104.25", "at 12:30:45"] },
    };
    expect(scrub(record, "/c")).toEqual({
      ms: 1500,
      toasts: [{ title: "Failed", message: "Command failed: <scratch>/heyo/data/support/x get" }],
      error: { message: "ENOENT '<extension>/assets/tool'" },
      view: { titles: ["<ip address>", "<ip address>", "<hardware address>", "version 1.104.25", "at 12:30:45"] },
    });
  });

  it("knows Heyo's own directories, which the cache may never be in", () => {
    const env = {};
    expect(insideHeyo("/home/u/.local/share/heyo", "/home/u", env)).toBe(true);
    expect(insideHeyo("/home/u/.local/share/heyo/extensions/x", "/home/u", env)).toBe(true);
    expect(insideHeyo("/home/u/.config/heyo/compat", "/home/u", env)).toBe(true);
    expect(insideHeyo("/home/u/.cache/heyo", "/home/u", env)).toBe(true);
    expect(insideHeyo("/home/u/.local/share/heyo-compat", "/home/u", env)).toBe(false);
    expect(insideHeyo("/tmp/heyo-compat", "/home/u", env)).toBe(false);
    expect(insideHeyo("/data/heyo/x", "/home/u", { XDG_DATA_HOME: "/data" })).toBe(true);
  });
});

describe("the report", () => {
  const verdict = { status: "fails", reason: "macos-only", detail: "AppleScript", evidence: "ran to its end; threw: </script><script>alert(1)</script>" };
  const results = {
    generatedAt: "2026-10-10T12:00:00.000Z",
    heyo: { version: "0.1.0", commit: "abc1234", runtimeSha256: "0123456789ab" },
    method: { sample: "the 1 most-downloaded extensions of the Raycast Store", ordering: "download_count, descending", catalogue: 3000, listingFetchedAt: "2026-10-10T11:00:00.000Z", network: "both", sandbox: "bwrap", timeoutMs: 15000, memoryMb: 1024, node: "v24.0.0", platform: "linux arm64" },
    summary: {
      extensions: { total: 1, works: 0, configure: 0, partly: 0, unknown: 0, fails: 1 },
      commands: { total: 1, works: 0, configure: 0, partly: 0, unknown: 0, fails: 1 },
      offline: { extensions: { works: 0, configure: 0, partly: 0, unknown: 1, fails: 0 }, commands: {} },
      modes: { "no-view": 1 },
    },
    blockers: [{ blocker: "macos-only: AppleScript", reason: "macos-only", extensions: ["demo"], sole: ["demo"], commands: 1 }],
    staticRules: [{ rule: "applescript", extensions: 1, commands: 1 }],
    extensions: [
      {
        rank: 1,
        name: "demo",
        title: "Demo <b>",
        owner: "someone",
        downloads: 123456,
        platforms: ["macOS"],
        storeUrl: "https://www.raycast.com/someone/demo",
        status: "fails",
        reason: "macos-only",
        counts: { works: 0, configure: 0, partly: 0, unknown: 0, fails: 1 },
        findings: [],
        commands: [{ name: "go", title: "Go", mode: "no-view", verdict, static: { applescript: { error: 1 } }, findings: [], run: { host: { "host.hud": 1 }, error: { message: "spawn osascript ENOENT", raisedIn: "extension" } } }],
      },
    ],
  };

  it("is one page that fetches nothing", () => {
    const html = renderHtml(results);
    expect(html).not.toMatch(/<(?:script|link|img|iframe)[^>]+(?:src|href)=/i);
    expect(html).not.toMatch(/@import|url\(/);
    expect(html).toContain("prefers-color-scheme:dark");
  });

  it("says what it is about: the Heyo, the day, the sample and its limits", () => {
    const html = renderHtml(results);
    expect(html).toContain("Heyo 0.1.0");
    expect(html).toContain("abc1234");
    expect(html).toContain("2026-10-10");
    expect(html).toContain("download_count, descending");
    expect(html).toContain("A launch is not a use.");
  });

  it("carries its data without letting it out of its script element", () => {
    const html = renderHtml(results);
    const data = /<script type="application\/json" id="data">(.*?)<\/script>/s.exec(html)![1];
    expect(data).not.toContain("</script>");
    expect(JSON.parse(data).extensions[0].commands[0].evidence).toContain("</script><script>alert(1)</script>");
    expect(html).not.toContain("Demo <b>");
  });

  it("writes the same counts in Markdown", () => {
    const markdown = renderMarkdown(results);
    expect(markdown).toContain("| Fails | 1 | 100% |");
    expect(markdown).toContain("| 1 | [Demo <b>](https://www.raycast.com/someone/demo) | 123K | Fails | 0 / 0 / 0 / 0 / 1 |");
    expect(markdown).toContain("AppleScript | 1 | 1 | 1 | demo |");
    expect(markdown).toContain("commit `abc1234`");
  });
});
