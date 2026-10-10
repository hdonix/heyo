import { describe, expect, it } from "vitest";

import { appleScriptTargets, missingMembers, rankApi, rankTargets, sortMissing, usedMembers } from "./api.mjs";

/// As `PROBE` writes it: functions marked, plain values 1, a Proxy open.
const surface = {
  List: { $fn: 1, Item: { $fn: 1, Detail: { $fn: 1, Metadata: { $fn: 1, Label: { $fn: 1 } } } }, EmptyView: { $fn: 1 } },
  showToast: { $fn: 1 },
  Toast: { $fn: 1, Style: { Success: 1, Failure: 1 } },
  environment: { supportPath: 1, isDevelopment: 1 },
  AI: { ask: { $fn: 1 }, Model: { $open: 1 } },
  Icon: { Star: 1 },
};

describe("the API a built command names", () => {
  it("reads what a bundler leaves: import_api, numbered or wrapped", () => {
    const used = usedMembers(`
      var import_api = require("@raycast/api");
      var import_api2 = __toESM(require("@raycast/api"));
      (0, import_api.showToast)({ style: import_api.Toast.Style.Failure });
      jsx(import_api2.List.Item.Detail.Metadata.Label, {});
      import_api2.default.environment.supportPath.replace("a", "b");
      other.List.Nothing;
    `);
    expect([...used.keys()].sort()).toEqual(["List.Item.Detail.Metadata.Label", "Toast.Style.Failure", "environment.supportPath.replace", "showToast"]);
    expect([...used.values()].every(Boolean)).toBe(true);
  });

  it("does not trust a minifier's one letter", () => {
    const used = usedMembers(`var e=require("@raycast/api");e.List.Item;function f(e){return e.target.value}`);
    expect(used.get("List.Item")).toBe(false);
    expect(used.get("target.value")).toBe(false);
  });
});

describe("what of it the runtime lacks", () => {
  const missing = (source: string) => missingMembers(usedMembers(source), surface);
  const api = `var import_api = require("@raycast/api");`;

  it("names a member up to the first part that is not there", () => {
    expect(missing(`${api} import_api.List.Item.Detail.Metadata.TagList.Item; import_api.getSelectedFinderItems(); import_api.List.Item;`)).toEqual(["List.Item.Detail.Metadata.TagList", "getSelectedFinderItems"]);
  });

  it("does not look inside plain values, or past what answers to any name", () => {
    expect(missing(`${api} import_api.environment.supportPath.replace; import_api.AI.Model.Anything_At_All; import_api.Toast.Style.Failure.length;`)).toEqual([]);
  });

  it("lets a function be a function", () => {
    expect(missing(`${api} import_api.showToast.bind(null); import_api.List.displayName;`)).toEqual([]);
  });

  it("takes from an untrusted name only what starts as the API does", () => {
    expect(missing(`var e=require("@raycast/api");e.List.Nope;e.target.value;e.somethingElse()`)).toEqual(["List.Nope"]);
  });

  it("tells what breaks from what only shows less", () => {
    expect(sortMissing(["Icon.NewThing", "List.Nope", "Color.Odd"])).toEqual({ breaking: ["List.Nope"], cosmetic: ["Icon.NewThing", "Color.Odd"] });
  });
});

describe("what AppleScript speaks to", () => {
  it("reads the applications told, as scripts are written in bundles", () => {
    const source = [
      'await runAppleScript(`tell application "Finder" to get selection`);',
      "const s = 'tell application \\\"System Events\\\" to keystroke \"v\" using command down';",
      'tell app "Music"\n  play\nend tell',
      'tell application id "com.apple.Safari" to activate',
      "`tell application \"${app.name}\" to quit`",
      'Application("Google Chrome").windows()',
      'myApplication("no")',
    ].join("\n");
    expect(appleScriptTargets(source)).toEqual(["(named at run time)", "Finder", "Google Chrome", "Music", "System Events", "com.apple.Safari"]);
    expect(appleScriptTargets("const tell = 1;")).toEqual([]);
  });

  it("counts them by extension", () => {
    const ranked = rankTargets([
      { name: "a", commands: [{ api: { tells: ["Finder", "System Events"] } }, { api: { tells: ["Finder"] } }] },
      { name: "b", commands: [{ api: { tells: ["System Events"] } }, {}] },
      { name: "c", commands: [{ api: { tells: ["System Events"] } }] },
    ]);
    expect(ranked).toEqual([
      { name: "System Events", extensions: ["a", "b", "c"], commands: 3 },
      { name: "Finder", extensions: ["a"], commands: 2 },
    ]);
  });
});

describe("the missing members that matter most", () => {
  it("counts extensions and commands, and the commands that did not launch cleanly", () => {
    const ranked = rankApi([
      { name: "a", commands: [{ api: { breaking: ["X"], cosmetic: ["Icon.Y"] }, verdict: { status: "fails" } }, { api: { breaking: ["X"], cosmetic: [] }, verdict: { status: "works" } }] },
      { name: "b", commands: [{ api: { breaking: ["X", "Z"], cosmetic: [] }, verdict: { status: "partly" } }, { verdict: { status: "works" } }] },
    ]);
    expect(ranked).toEqual([
      { name: "X", cosmetic: false, extensions: ["a", "b"], commands: 3, notClean: 2 },
      { name: "Z", cosmetic: false, extensions: ["b"], commands: 1, notClean: 1 },
      { name: "Icon.Y", cosmetic: true, extensions: ["a"], commands: 1, notClean: 1 },
    ]);
  });
});
