import { describe, expect, it } from "vitest";

import { blockerOf, classifyCommand, classifyExtension, describe as describeRun, errorReason, judge, normalize, rankBlockers, rankStatic } from "./classify.mjs";

const list = (items: number, more: object = {}) => ({ root: "List", items, titles: [], isLoading: false, content: items > 0, ...more });
const view = (v: object, more: object = {}) => ({ outcome: "view", ms: 1500, toasts: [], effects: [], view: v, ...more });
const finished = (more: object = {}) => ({ outcome: "finished", ms: 300, toasts: [], effects: [], ...more });
const failure = (title: string, message?: string) => ({ style: "FAILURE", title, message });
const finding = (rule: string, severity = "error", message = "x") => ({ rule, severity, message, file: "index.js" });

describe("what an error speaks of", () => {
  it("knows AppleScript and macOS programs", () => {
    expect(errorReason("spawn osascript ENOENT")).toEqual({ reason: "macos-only", detail: "AppleScript" });
    expect(errorReason("AppleScript is only supported on macOS")).toEqual({ reason: "macos-only", detail: "AppleScript" });
    expect(errorReason("spawn /usr/bin/caffeinate ENOENT")).toEqual({ reason: "macos-only", detail: "caffeinate" });
    expect(errorReason("/bin/sh: 1: pbpaste: not found")).toEqual({ reason: "macos-only", detail: "pbpaste" });
    expect(errorReason("Command failed: defaults read -g AppleInterfaceStyle")).toEqual({ reason: "macos-only", detail: "defaults" });
    expect(errorReason("speedtest is a macOS program and cannot run on Linux. This extension does not support Linux.").reason).toBe("macos-only");
    expect(errorReason("ENOENT: no such file or directory, scandir '/Applications'")).toEqual({ reason: "macos-only", detail: "a macOS path" });
    expect(errorReason("ENOENT: no such file or directory, open '<scratch>/home/Library/Application Support/Code/storage.json'").reason).toBe("macos-only");
  });

  it("tells a program the test machine lacks from one only macOS has", () => {
    expect(errorReason("spawn docker ENOENT")).toEqual({ reason: "missing-program", detail: "docker" });
    expect(errorReason("/bin/sh: 1: /usr/local/bin/brew: not found")).toEqual({ reason: "missing-program", detail: "brew" });
    expect(errorReason('spawnSync "undefined" ENOENT')).toEqual({ reason: "missing-program", detail: "undefined" });
    expect(errorReason("zsh: command not found: op")).toEqual({ reason: "missing-program", detail: "op" });
  });

  it("names the file of another application that is not there", () => {
    expect(errorReason("ENOENT: no such file or directory, open '<scratch>/home/.config/Code/User/globalStorage/state.vscdb'")).toEqual({
      reason: "missing-file",
      detail: "~/.config/Code/User/globalStorage/state.vscdb",
    });
  });

  it("names the part of the API a bundle called and did not find", () => {
    expect(errorReason("(0 , import_api.getSelectedFinderItems) is not a function")).toEqual({ reason: "missing-api", detail: "getSelectedFinderItems" });
    expect(errorReason("import_api2.AI.ask is not a function")).toEqual({ reason: "missing-api", detail: "AI.ask" });
    expect(errorReason("Cannot find module 'swift:../swift'")).toEqual({ reason: "missing-module", detail: "swift:../swift" });
  });

  it("puts an error with nothing else to go by where it was raised", () => {
    expect(errorReason("x is not iterable", { raisedIn: "runtime" }).reason).toBe("heyo-runtime");
    expect(errorReason("x is not iterable", { raisedIn: "extension" }).reason).toBe("extension-error");
    expect(errorReason("x is not iterable").reason).toBe("extension-error");
  });

  it("knows credentials and the network", () => {
    expect(errorReason("Request failed with status code 401").reason).toBe("needs-credentials");
    expect(errorReason("Invalid API key provided").reason).toBe("needs-credentials");
    expect(errorReason("getaddrinfo ENOTFOUND api.example.com").reason).toBe("needs-network");
    expect(errorReason("fetch failed").reason).toBe("needs-network");
  });

  it("takes out of a message what differs between extensions", () => {
    expect(normalize("Cannot open /home/a/b/c.json at line 42\nmore")).toBe("Cannot open <path> at line N");
    expect(normalize("failed in <scratch>/home/x.db")).toBe("failed in <path>");
  });
});

describe("the verdict on a command", () => {
  it("works: showed its own content, no error, nothing static against it", () => {
    const verdict = classifyCommand({ run: view(list(3, { titles: ["a", "b", "c"] })) });
    expect(verdict.status).toBe("works");
    expect(verdict.evidence).toContain("List with 3 items");
  });

  it("works: a command without a view that ran to its end", () => {
    expect(classifyCommand({ run: finished({ effects: [{ method: "host.open", params: "x" }] }) })).toMatchObject({ status: "works", evidence: expect.stringContaining("asked the host to: open") });
  });

  it("does not take an empty or loading view for a working command", () => {
    expect(classifyCommand({ run: view(list(0, { emptyView: "No results" })) })).toMatchObject({ status: "unknown", reason: "empty-view", detail: "No results" });
    expect(classifyCommand({ run: view(list(4, { isLoading: true })) })).toMatchObject({ status: "unknown", reason: "still-loading" });
    expect(classifyCommand({ run: { outcome: "timeout", ms: 15000, toasts: [], effects: [] } })).toMatchObject({ status: "unknown", reason: "timeout" });
    expect(classifyCommand({ run: undefined })).toMatchObject({ status: "unknown", reason: "not-run" });
  });

  it("does not round a clean launch up when the code has macOS-only parts", () => {
    const verdict = classifyCommand({ run: view(list(3)), findings: [finding("applescript"), finding("mac-path", "warning")] });
    expect(verdict).toMatchObject({ status: "partly", reason: "macos-only" });
    expect(verdict.detail).toContain("applescript");
    // A warning alone does not take "works" away.
    expect(classifyCommand({ run: view(list(3)), findings: [finding("mac-path", "warning")] }).status).toBe("works");
  });

  it("names the missing API a clean launch would still run into", () => {
    expect(classifyCommand({ run: view(list(3)), findings: [finding("api-missing", "error", "AI.refreshModels is not in Heyo's API (unstable)")] })).toMatchObject({
      status: "partly",
      reason: "missing-api",
      detail: "AI.refreshModels",
    });
  });

  it("fails for what is thrown, by its kind", () => {
    const thrown = (message: string, raisedIn = "extension") => finished({ error: { message, raisedIn }, toasts: [failure("Error", message)] });
    expect(classifyCommand({ run: thrown("spawn osascript ENOENT") })).toMatchObject({ status: "fails", reason: "macos-only", detail: "AppleScript" });
    expect(classifyCommand({ run: thrown("(0 , import_api.foo) is not a function") })).toMatchObject({ status: "fails", reason: "missing-api", detail: "foo" });
    expect(classifyCommand({ run: thrown("boom", "runtime") })).toMatchObject({ status: "fails", reason: "heyo-runtime" });
    expect(classifyCommand({ run: thrown("boom") })).toMatchObject({ status: "fails", reason: "extension-error", detail: "boom" });
  });

  it("fails for a macOS program even when a list came up around it", () => {
    const run = view(list(6), { toasts: [failure("Failed to fetch latest data", "spawn /usr/bin/sw_vers ENOENT")] });
    expect(classifyCommand({ run })).toMatchObject({ status: "fails", reason: "macos-only", detail: "sw_vers" });
  });

  it("is partly working when it showed its content and complained besides", () => {
    const run = view(list(6), { toasts: [failure("Could not load favourites")] });
    expect(classifyCommand({ run })).toMatchObject({ status: "partly", reason: "extension-reported" });
  });

  it("does not call a message of the extension's own a failure of Heyo's", () => {
    expect(classifyCommand({ run: finished({ toasts: [failure("No timers found!")] }) })).toMatchObject({ status: "unknown", reason: "extension-reported", detail: "No timers found!" });
  });

  it("guesses macOS, and says it is a guess, for a nameless failure in code with AppleScript", () => {
    const run = finished({ toasts: [failure("Could not get your playlists")] });
    const verdict = classifyCommand({ run, findings: [finding("applescript")] });
    expect(verdict).toMatchObject({ status: "fails", reason: "macos-only", inferred: true });
    expect(verdict.detail).toBe("probably: its code has applescript");
    expect(blockerOf(verdict)).toBe("macos-only: probably (a failure in code with macOS-only parts)");
  });

  it("takes macOS from what the command said in passing", () => {
    const hud = finished({ effects: [{ method: "host.hud", params: "AppleScript is only supported on macOS" }] });
    expect(classifyCommand({ run: hud })).toMatchObject({ status: "fails", reason: "macos-only", detail: "AppleScript" });
    const logged = finished({ toasts: [failure("Failed opening a new window")], stderr: "Error: spawn osascript ENOENT\n    at x" });
    expect(classifyCommand({ run: logged })).toMatchObject({ status: "fails", reason: "macos-only", detail: "AppleScript" });
    // A log that mentions a missing module is not taken for the cause.
    const other = finished({ toasts: [failure("Nothing to do")], stderr: "Cannot find module 'x'" });
    expect(classifyCommand({ run: other }).reason).toBe("extension-reported");
  });

  it("stops at Heyo's AI where a command asked for a model and got an error", () => {
    const run = finished({ host: { "host.ai": 1 }, error: { message: "Could not resolve authentication method.", raisedIn: "runtime" }, toasts: [] });
    expect(classifyCommand({ run })).toMatchObject({ status: "configure", reason: "needs-ai" });
  });

  it("fails for a view that threw while rendering, whatever the error screen holds", () => {
    const run = view({ root: "Detail", markdown: 900, errorScreen: true, content: false }, { error: { message: "x is undefined", raisedIn: "extension", frame: "Command (index.js:1:2)" } });
    const verdict = classifyCommand({ run });
    expect(verdict).toMatchObject({ status: "fails", reason: "extension-error" });
    expect(verdict.evidence).toContain("the runtime's error screen");
  });

  it("stops at configuration when made-up values brought nothing up", () => {
    const prefs = [{ name: "url", type: "textfield", secret: false }];
    for (const run of [view(list(0), { preferences: prefs }), view(list(2, { isLoading: true }), { preferences: prefs }), { outcome: "timeout", ms: 15000, toasts: [], effects: [], preferences: prefs }]) {
      expect(classifyCommand({ run })).toMatchObject({ status: "configure", reason: "needs-preferences" });
    }
  });

  it("takes an extension at its word when it says it is for another system", () => {
    expect(classifyCommand({ run: finished({ toasts: [failure("Failed detecting text", "Windows OCR is only available on Windows")] }) })).toMatchObject({ status: "fails", reason: "windows-only" });
    expect(classifyCommand({ run: finished({ toasts: [failure("Failed", "Barcode detection is currently available only on macOS")] }) })).toMatchObject({ status: "fails", reason: "macos-only", detail: "says it is for macOS" });
    expect(classifyCommand({ run: finished({ toasts: [failure("Failed", "AppleReminders is a macOS program and cannot run on Linux.")] }) }).detail).toBe("a macOS program it ships or downloads");
  });

  it("names the API members a bundle uses and the runtime lacks", () => {
    const absent = ["List.Item.Detail.Metadata.Foo"];
    expect(classifyCommand({ run: view(list(3)), absent })).toMatchObject({ status: "partly", reason: "missing-api", detail: "List.Item.Detail.Metadata.Foo" });
    const threw = view({ root: "Detail", errorScreen: true, content: false }, { error: { message: "Minified React error #130; visit https://react.dev/errors/130", raisedIn: "runtime" } });
    expect(classifyCommand({ run: threw, absent })).toMatchObject({ status: "fails", reason: "missing-api", detail: "List.Item.Detail.Metadata.Foo" });
    // The same error with nothing missing in sight stays the runtime's.
    expect(classifyCommand({ run: threw })).toMatchObject({ status: "fails", reason: "heyo-runtime" });
  });

  it("knows its own made-up argument when it comes back in an error", () => {
    const run = finished({ arguments: { model: "test" }, error: { message: "Unexpected token 'e', \"test\" is not valid JSON", raisedIn: "extension" }, toasts: [] });
    expect(classifyCommand({ run })).toMatchObject({ status: "unknown", reason: "needs-input" });
  });

  it("does not take a screen that says what is missing for a working command", () => {
    const detail = (excerpt: string) => view({ root: "Detail", markdown: excerpt.length, excerpt, content: true });
    expect(classifyCommand({ run: detail("# 🚨 Error: `yt-dlp` is not installed Please press ⏎") })).toMatchObject({ status: "unknown", reason: "says-missing" });
    expect(classifyCommand({ run: detail("# No vaults found Please use Obsidian to create a vault") })).toMatchObject({ status: "unknown", reason: "says-missing", detail: "No vaults found Please use Obsidian to create a vault" });
    expect(classifyCommand({ run: detail("# You will need the following to run the extension") }).reason).toBe("says-missing");
    expect(classifyCommand({ run: detail("# Notion connection No saved Notion connection was found. Use Reconnect Notion to sign in again") })).toMatchObject({ status: "configure", reason: "needs-credentials" });
    expect(classifyCommand({ run: finished({ effects: [{ method: "host.hud", params: "❌ Please install ScreenOCR extension" }] }) }).reason).toBe("says-missing");
    // Text that is the command's work stays what it is.
    expect(classifyCommand({ run: detail("# Test Topics referred to by the same term") }).status).toBe("works");
    expect(classifyCommand({ run: finished({ effects: [{ method: "host.hud", params: "Converted to No Case" }] }) }).status).toBe("works");
  });

  it("is inconclusive where a command hands over to another application's own link", () => {
    const open = (target: string) => finished({ effects: [{ method: "host.open", params: JSON.stringify({ target }) }] });
    expect(classifyCommand({ run: open("cleanshot://capture-area") })).toMatchObject({ status: "unknown", reason: "needs-app", detail: "cleanshot://" });
    expect(classifyCommand({ run: open("https://example.com") }).status).toBe("works");
    expect(classifyCommand({ run: open("/home/user/Documents") }).status).toBe("works");
    expect(blockerOf({ status: "unknown", reason: "needs-app", detail: "cleanshot://" })).toBe("needs-app: cleanshot://");
  });

  it("names what Heyo's API says it cannot do", () => {
    const run = finished({ error: { message: "The browser extension API is not supported by Heyo", raisedIn: "runtime" }, toasts: [] });
    expect(classifyCommand({ run })).toMatchObject({ status: "fails", reason: "missing-api", detail: "The browser extension API" });
  });

  it("says an error of the extension's is suspect where its code has macOS parts", () => {
    const run = finished({ error: { message: "The \"path\" argument must be of type string", raisedIn: "extension" }, toasts: [] });
    expect(classifyCommand({ run, findings: [finding("mac-path", "warning")] })).toMatchObject({ status: "fails", reason: "extension-error", macSuspect: true });
    expect(classifyCommand({ run }).macSuspect).toBeUndefined();
  });

  it("stops at configuration, and does not call it working", () => {
    const prefs = [{ name: "token", type: "password", secret: true }];
    expect(classifyCommand({ run: view(list(3), { preferences: prefs }) })).toMatchObject({ status: "configure", reason: "needs-preferences" });
    expect(classifyCommand({ run: view(list(0), { preferences: prefs, toasts: [failure("Request failed", "401 Unauthorized")] }) })).toMatchObject({ status: "configure", reason: "needs-credentials" });
    // A made-up server cannot be reached: that says nothing about the network.
    expect(classifyCommand({ run: view(list(0), { preferences: prefs, toasts: [failure("Error", "getaddrinfo ENOTFOUND example.invalid")] }) })).toMatchObject({ status: "configure", reason: "needs-preferences" });
    expect(classifyCommand({ run: finished({ preferences: prefs, toasts: [failure("Something odd")] }) })).toMatchObject({ status: "configure", reason: "needs-preferences" });
    expect(classifyCommand({ run: { outcome: "oauth", ms: 900, toasts: [], effects: [], oauth: { provider: "GitHub" } } })).toMatchObject({ status: "configure", reason: "needs-oauth", detail: "GitHub" });
    expect(classifyCommand({ run: view(list(0), { toasts: [failure("Please sign in first")] }) })).toMatchObject({ status: "configure", reason: "needs-credentials" });
  });

  it("still fails with made-up preferences when the trouble is not theirs", () => {
    const prefs = [{ name: "token", type: "password", secret: true }];
    const run = finished({ preferences: prefs, error: { message: "spawn osascript ENOENT", raisedIn: "extension" }, toasts: [] });
    expect(classifyCommand({ run })).toMatchObject({ status: "fails", reason: "macos-only" });
  });

  it("is inconclusive for what the test machine lacks", () => {
    expect(classifyCommand({ run: view(list(0), { toasts: [failure("Failed", "spawn docker ENOENT")] }) })).toMatchObject({ status: "unknown", reason: "missing-program", detail: "docker" });
    expect(classifyCommand({ run: view(list(0), { network: false, toasts: [failure("Failed", "fetch failed")] }) })).toMatchObject({ status: "unknown", reason: "needs-network" });
    expect(classifyCommand({ run: finished({ arguments: { hours: "test" }, toasts: [failure("Please enter a number")] }) })).toMatchObject({ status: "unknown", reason: "needs-input" });
  });

  it("fails when the runtime process ended", () => {
    expect(classifyCommand({ run: { outcome: "crashed", stopped: "exit", exit: { code: 134, signal: null }, ms: 800, toasts: [] } })).toMatchObject({ status: "fails", reason: "crash", detail: "exit 134" });
    expect(classifyCommand({ run: { outcome: "crashed", stopped: "memory", exit: { memory: "over 1024 MiB" }, ms: 800, toasts: [] } }).detail).toBe("over 1024 MiB");
  });

  it("takes a menu with items for a working menu bar command", () => {
    expect(classifyCommand({ run: { outcome: "menu", ms: 1000, toasts: [], effects: [], menu: { title: "3", items: 4 } } }).status).toBe("works");
    expect(classifyCommand({ run: { outcome: "no-menu", ms: 1000, toasts: [], effects: [] } })).toMatchObject({ status: "unknown", reason: "no-menu" });
  });

  it("says in words what was seen", () => {
    const run = view(list(2, { titles: ["One", "Two"] }), { typed: "test", toasts: [failure("Oops", "really")], effects: [{ method: "host.hud", params: "Done" }] });
    expect(describeRun(run)).toBe("List with 2 items (“One”, “Two”), after typing “test”, in 1.5 s; failure toast: Oops — really; HUD: Done");
  });
});

describe("the verdict on an extension", () => {
  const of = (...statuses: [string, string?][]) => classifyExtension(statuses.map(([status, reason]) => ({ verdict: { status, reason } })));

  it("works only when every command does", () => {
    expect(of(["works"], ["works"]).status).toBe("works");
    expect(of(["works"], ["configure", "needs-oauth"]).status).toBe("configure");
    expect(of(["configure", "needs-oauth"]).status).toBe("configure");
  });

  it("is partly working for a mix, and fails when nothing gets anywhere", () => {
    expect(of(["works"], ["fails", "macos-only"])).toMatchObject({ status: "partly", reason: "macos-only" });
    expect(of(["works"], ["unknown", "empty-view"]).status).toBe("partly");
    expect(of(["partly", "macos-only"]).status).toBe("partly");
    expect(of(["fails", "macos-only"], ["fails", "macos-only"]).status).toBe("fails");
    expect(of(["fails", "macos-only"], ["unknown", "timeout"]).status).toBe("fails");
    expect(of(["unknown", "timeout"]).status).toBe("unknown");
    expect(of().status).toBe("unknown");
  });

  it("is worked out again from a stored record, counted rules included", () => {
    const record = {
      name: "demo",
      commands: [
        { name: "a", run: view(list(2)), static: { applescript: { error: 3 } }, findings: [] },
        { name: "b", run: view(list(2)), offlineRun: view(list(0), { network: false, toasts: [failure("x", "fetch failed")] }), static: {}, findings: [] },
      ],
    };
    const judged = judge(record);
    expect(judged.commands.map((c: { verdict: { status: string } }) => c.verdict.status)).toEqual(["partly", "works"]);
    expect(judged.status).toBe("partly");
    expect(judged.commands[1].offline.verdict).toMatchObject({ status: "unknown", reason: "needs-network" });
    expect(judged.offline.counts).toMatchObject({ partly: 1, unknown: 1 });
  });

  it("names the commonest reason and counts the commands", () => {
    const verdict = of(["works"], ["fails", "macos-only"], ["fails", "macos-only"], ["unknown", "timeout"]);
    expect(verdict.reason).toBe("macos-only");
    expect(verdict.counts).toEqual({ works: 1, configure: 0, partly: 0, unknown: 1, fails: 2 });
  });
});

describe("what holds the most extensions back", () => {
  const extension = (name: string, ...verdicts: object[]) => ({ name, commands: verdicts.map((verdict) => ({ verdict, static: {} })) });
  const mac = { status: "fails", reason: "macos-only", detail: "AppleScript" };
  const api = { status: "fails", reason: "missing-api", detail: "foo" };

  it("counts extensions, and those a blocker alone holds back", () => {
    const ranked = rankBlockers([
      extension("a", mac, mac, { status: "works" }),
      extension("b", mac, api),
      extension("c", api),
      extension("d", mac),
    ]);
    expect(ranked[0]).toMatchObject({ blocker: "macos-only: AppleScript", extensions: ["a", "b", "d"], sole: ["a", "d"], commands: 4 });
    expect(ranked[1]).toMatchObject({ blocker: "missing-api: foo", extensions: ["b", "c"], sole: ["c"], commands: 2 });
  });

  it("does not count a working command as blocked", () => {
    expect(blockerOf({ status: "works" })).toBeUndefined();
    expect(blockerOf({ status: "unknown", reason: "empty-view", detail: "No results" })).toBe("empty-view");
    expect(blockerOf({ status: "partly", reason: "macos-only", detail: "launches, but its code has applescript" })).toBe("macos-only: in code the launch did not reach");
  });

  it("counts the static rules by extension and command, notes left out", () => {
    const ranked = rankStatic([
      { commands: [{ static: { applescript: { error: 2 } } }, { static: { applescript: { error: 1 }, "prop-ignored": { info: 3 } } }] },
      { commands: [{ static: { "mac-path": { warning: 1 } } }] },
    ]);
    expect(ranked).toEqual([
      { rule: "applescript", extensions: 1, commands: 2 },
      { rule: "mac-path", extensions: 1, commands: 1 },
    ]);
  });
});
