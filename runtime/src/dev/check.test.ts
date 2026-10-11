import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { afterAll, describe, expect, it } from "vitest";

import { checkExtension, checkManifest, checkSource, formatReport, heyoModulesUsed, knownGaps, ownCode, RULES } from "./check";

const rules = (source: string) => checkSource("src/x.ts", source).map((f) => f.rule);
const found = (source: string) => checkSource("src/x.ts", source).map((f) => `${f.rule}@${f.line}`);

describe("macOS-only code", () => {
  it("AppleScript, however it is run", () => {
    expect(found('import { runAppleScript } from "@raycast/utils";\nawait runAppleScript(`tell application "Finder" to activate`);')).toEqual([
      "applescript@1",
      "applescript@2",
    ]);
    expect(rules('execSync("osascript -e \'display dialog 1\'")')).toEqual(["applescript"]);
    expect(rules('import { runAppleScript } from "run-applescript";')).toEqual(["applescript"]);
    const [finding] = checkSource("src/x.ts", "  await runAppleScript(script);");
    expect(finding).toMatchObject({ severity: "error", file: "src/x.ts", line: 1, column: 9, excerpt: "await runAppleScript(script);" });
  });

  it("open with macOS flags, but not open with a plain target", () => {
    expect(rules('exec(`open -a "Safari" ${url}`)')).toEqual(["open-flags"]);
    expect(rules('execFile("open", ["-b", "com.apple.Safari"])')).toEqual(["open-flags"]);
    expect(rules('exec("open -R " + file)')).toEqual(["open-flags"]);
    expect(rules('exec(`xdg-open ${url}`); exec("open " + url); const open = -a;')).toEqual([]);
  });

  it("programs only macOS has, where one is started", () => {
    expect(rules('execSync("pbpaste")')).toEqual(["mac-program"]);
    expect(rules('const { stdout } = await execa("defaults", ["read", domain]);')).toEqual(["mac-program"]);
    expect(rules('spawn(\n  "mdfind",\n  [query],\n);')).toEqual(["mac-program"]);
    expect(rules('const say = "/usr/bin/say";')).toEqual(["mac-program"]);
    expect(rules("await $`pmset -g batt`")).toEqual(["mac-program"]);
    // The same words where nothing is run.
    expect(rules('const title = "say something"; const mode = "security";')).toEqual([]);
    expect(rules('exec("git status"); exec("ls -la");')).toEqual([]);
  });

  it("paths only macOS has", () => {
    expect(rules('const apps = fs.readdirSync("/Applications");')).toEqual(["mac-path"]);
    expect(rules('const app = "/Applications/Safari.app";')).toEqual(["mac-path"]);
    expect(rules("const db = `${homedir()}/Library/Safari/History.db`; const x = '~/Library/Caches';")).toEqual(["mac-path"]);
    expect(rules('path.join(homedir(), "Library", "Application Support", "Code")')).toEqual(["mac-path"]);
    expect(rules('const brew = "/opt/homebrew/bin/gh";')).toEqual(["mac-path"]);
    expect(rules('const url = "https://example.com/Applications"; const title = "Applications"; const lib = "/usr/lib/Library";')).toEqual([]);
  });

  it("a branch for macOS with nothing for the others", () => {
    expect(rules('if (process.platform === "darwin") run();')).toEqual(["darwin-only"]);
    expect(rules('if (process.platform !== "darwin") throw new Error("macOS only");')).toEqual(["darwin-only"]);
    expect(rules('if (os.platform() == "darwin") run();')).toEqual(["darwin-only"]);
    expect(checkSource("x", 'if (process.platform !== "darwin") return;')[0].message).toContain("turns away");
    // An alternative in the same file.
    expect(rules('const cmd = process.platform === "darwin" ? "open" : process.platform === "win32" ? "start" : "xdg-open";')).toEqual([]);
    expect(rules('if (process.platform === "darwin") a(); else if (process.platform === "linux") b();')).toEqual([]);
  });

  it("Swift helpers", () => {
    expect(rules('import { pick } from "swift:../swift/color-picker";')).toEqual(["swift"]);
  });

  it("is not looked for in comments", () => {
    expect(rules('// runAppleScript("x")\n/* open -a Safari\n   /Applications */\nconst a = 1; // process.platform === "darwin"')).toEqual([]);
  });
});

describe("the API", () => {
  it("has the lists the conformance tests keep", () => {
    expect(knownGaps.api).toContain("render");
    expect(knownGaps.api.length).toBeGreaterThan(5);
    expect(Object.keys(knownGaps.props).length).toBeGreaterThan(0);
  });

  it("members Heyo does not have, under whatever name they were imported", () => {
    expect(found('import { render } from "@raycast/api";\nrender(<Command />);')).toEqual(["api-missing@1"]);
    expect(found('import { AI } from "@raycast/api";\nawait AI.ask(prompt);\nawait AI.experimental_decide(x);')).toEqual(["api-missing@3"]);
    expect(found('import { environment as env } from "@heyo/api";\nconst mode = env.entryPointMode;')).toEqual(["api-missing@2"]);
    expect(found('import * as api from "@raycast/api";\napi.OAuth.clientIdMetadataDocument();')).toEqual(["api-missing@2"]);
    expect(found('const { useActionPanel: panel } = require("@raycast/api");')).toEqual(["api-missing@1"]);
    // What a bundler leaves of an import.
    expect(found('var import_api = require("@raycast/api");\n(0, import_api.render)(x);\nimport_api.AI.refreshModels();')).toEqual(["api-missing@2", "api-missing@3"]);
    expect(checkSource("x", 'import { render } from "@raycast/api";')[0].message).toContain("render is not in Heyo's API");
  });

  it("only when the name is the API's", () => {
    expect(rules('import { render } from "@testing-library/react";\nrender(<App />);')).toEqual([]);
    expect(rules('import { List } from "@raycast/api";\nconst AI = { experimental_decide() {} };\nAI.experimental_decide();')).toEqual([]);
    expect(rules('import { environment } from "@raycast/api";\nconst x = other.environment.entryPointMode;')).toEqual([]);
  });

  it("what Heyo has and cannot do", () => {
    const findings = checkSource("x", 'import { BrowserExtension, updateCommandMetadata } from "@raycast/api";\nawait BrowserExtension.getTabs();\nawait updateCommandMetadata({ subtitle });');
    expect(findings.map((f) => [f.rule, f.severity, f.line])).toEqual([
      ["api-unsupported", "error", 2],
      ["api-unsupported", "info", 1],
    ]);
  });

  it("props that are taken and ignored", () => {
    const source = `import { Form, Detail } from "@raycast/api";
export default () => (
  <Form
    enableDrafts
    actions={<Detail enableDrafts={x > 1} />}
  >
    <Form.TextArea id="a" enableMarkdown={true} />
    <Form.TextField id="b" enableMarkdown />
  </Form>
);`;
    expect(checkSource("x.tsx", source).map((f) => [f.rule, f.line, f.message.split(" ")[0]])).toEqual([
      ["prop-ignored", 4, "Form"],
      ["prop-ignored", 7, "Form.TextArea"],
    ]);
  });

  it("the Heyo modules a source uses", () => {
    expect(heyoModulesUsed('import { Windows, List, Dictation as D } from "@heyo/api";')).toEqual(["Dictation", "Windows"]);
    expect(heyoModulesUsed('import * as heyo from "@heyo/api";\nawait heyo.Screen.capture();')).toEqual(["Screen"]);
    // Not from Heyo's package: something else of that name.
    expect(heyoModulesUsed('import { Windows } from "./windows";\nimport { List } from "@raycast/api";')).toEqual([]);
  });
});

describe("the manifest", () => {
  const exists = (file: string) => ["assets/icon.png", "src/a.tsx"].includes(file);
  const base = { name: "x", title: "X", icon: "icon.png", commands: [{ name: "a", title: "A", mode: "view" }] };

  it("says what the author had in mind", () => {
    expect(checkManifest(base, exists)).toEqual([]);
    expect(checkManifest({ ...base, platforms: ["macOS"] }, exists).map((f) => f.rule)).toEqual(["platforms"]);
    expect(checkManifest({ ...base, platforms: ["macOS", "Windows"] }, exists)).toEqual([]);
    expect(checkManifest({ ...base, tools: [{ name: "t" }] }, exists).map((f) => f.rule)).toEqual(["ai-tools"]);
  });

  it("passes on what building would refuse", () => {
    const findings = checkManifest({ ...base, commands: [{ name: "b", title: "B", mode: "view" }] }, exists);
    expect(findings).toEqual([{ rule: "manifest", severity: "error", file: "package.json", message: 'command "b" has no source: expected src/b.tsx (or .ts, .jsx, .js)' }]);
  });
});

describe("a whole extension", () => {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), "heyo-check-test-"));
  afterAll(() => fs.rmSync(dir, { recursive: true, force: true }));
  const write = (file: string, content: string | Buffer) => {
    fs.mkdirSync(path.dirname(path.join(dir, file)), { recursive: true });
    fs.writeFileSync(path.join(dir, file), content);
  };
  write(
    "package.json",
    JSON.stringify({
      name: "mac-things",
      title: "Mac Things",
      icon: "icon.png",
      platforms: ["macOS"],
      commands: [{ name: "index", title: "Index", mode: "view" }],
      dependencies: { "@raycast/api": "^1.0.0", "@heyo/api": "^1.0.0", native: "1.0.0", plain: "1.0.0" },
      heyo: { apiVersion: 1, permissions: ["ai"] },
    }),
  );
  write("assets/icon.png", "png");
  write("assets/helper", Buffer.from("cffaedfe0c000001", "hex"));
  write("src/index.tsx", 'import { List, Windows } from "@heyo/api";\nimport { front } from "./lib/front";\nexport default () => <List navigationTitle={front()} />;\n');
  write("src/lib/front.ts", 'import { runAppleScript } from "@raycast/utils";\nexport const front = () => runAppleScript("tell application \\"System Events\\" to get name");\n');
  write("node_modules/native/package.json", JSON.stringify({ name: "native", dependencies: { deeper: "1" } }));
  write("node_modules/native/build/Release/native.node", "");
  write("node_modules/plain/package.json", JSON.stringify({ name: "plain" }));
  write("node_modules/deeper/package.json", JSON.stringify({ name: "deeper", gypfile: true }));
  write("node_modules/unused/package.json", JSON.stringify({ name: "unused", gypfile: true }));
  write("node_modules/unused/index.js", 'require("child_process").exec("osascript")');

  it("is reported file by file, worst first", () => {
    const report = checkExtension(dir);
    expect(report.extension).toEqual({ name: "mac-things", title: "Mac Things", dir });
    expect(report.files).toBe(2);
    expect(report.findings.map((f) => [f.severity, f.rule, f.file, f.line])).toEqual([
      ["error", "mac-binary", "assets/helper", undefined],
      ["error", "applescript", "src/lib/front.ts", 1],
      ["error", "applescript", "src/lib/front.ts", 2],
      ["warning", "permission-missing", "package.json", undefined],
      ["warning", "native-addon", "package.json", undefined],
      ["warning", "native-addon", "package.json", undefined],
      ["info", "platforms", "package.json", undefined],
    ]);
    expect(report.summary).toEqual({ error: 3, warning: 3, info: 1 });
    // What is used, and what that uses; not everything that is installed.
    expect(report.findings.filter((f) => f.rule === "native-addon").map((f) => f.message.split(" ")[0])).toEqual(["deeper", "native"]);
    expect(report.findings.find((f) => f.rule === "permission-missing")!.message).toContain('uses Windows, which needs the "windows" permission');
    // Every rule a finding names is one that is described.
    for (const finding of report.findings) expect(RULES[finding.rule]).toBeTruthy();
  });

  it("reads for a person and for a program", () => {
    const report = checkExtension(dir);
    const text = formatReport(report);
    expect(text).toContain("Mac Things — 2 source files read");
    expect(text).toContain("error   [applescript] src/lib/front.ts:1:10");
    expect(text).toContain("3 errors, 3 warnings, 1 note.");
    expect(JSON.parse(JSON.stringify(report))).toEqual(report);
  });

  it("a built extension is read for its own code, not the libraries bundled into it", () => {
    const built = fs.mkdtempSync(path.join(os.tmpdir(), "heyo-check-built-"));
    const bundle = [
      '"use strict";',
      "// node_modules/@raycast/utils/dist/run-applescript.js",
      "async function runAppleScript(script) {",
      '  return exec("osascript", ["-e", script]);',
      "}",
      "// src/lib/front.ts",
      'var import_api = require("@raycast/api");',
      'var front = () => runAppleScript("return 1");',
      "// src/index.tsx",
      "var tabs = () => import_api.BrowserExtension.getTabs();",
    ].join("\n");
    fs.writeFileSync(path.join(built, "package.json"), JSON.stringify({ name: "b", title: "B", commands: [{ name: "index", title: "I", mode: "view" }, { name: "off", title: "O", mode: "view" }] }));
    fs.writeFileSync(path.join(built, "index.js"), bundle);
    const report = checkExtension(built);
    fs.rmSync(built, { recursive: true, force: true });
    expect(ownCode(bundle).split("\n").map((line) => line.trim() !== "")).toEqual([true, false, false, false, false, true, true, true, true, true]);
    expect(ownCode(bundle).length).toBe(bundle.length);
    expect(report.findings.map((f) => [f.rule, f.file, f.line])).toEqual([
      ["applescript", "index.js", 8],
      ["api-unsupported", "index.js", 10],
      // A command of the manifest that was not built; and no icon.
      ["manifest", "package.json", undefined],
      ["manifest", "package.json", undefined],
    ]);
    expect(report.findings[2].message).toContain('command "off" has no source');
  });

  it("finds nothing in one written for Heyo that lists what it uses", () => {
    const report = checkExtension(path.join(__dirname, "fixtures/progress"));
    expect(report.findings).toEqual([]);
    expect(formatReport(report)).toContain("Nothing found that would not work on Heyo.");
  });

  it("says so when there is nothing", () => {
    const clean = fs.mkdtempSync(path.join(os.tmpdir(), "heyo-check-clean-"));
    fs.mkdirSync(path.join(clean, "src"));
    fs.writeFileSync(path.join(clean, "package.json"), JSON.stringify({ name: "ok", title: "Ok", commands: [{ name: "a", title: "A", mode: "no-view" }] }));
    fs.writeFileSync(path.join(clean, "src/a.ts"), 'import { showHUD } from "@raycast/api";\nexport default () => showHUD("hi");\n');
    const report = checkExtension(clean);
    fs.rmSync(clean, { recursive: true, force: true });
    // No icon is a warning of the manifest, and nothing else.
    expect(report.findings.map((f) => f.rule)).toEqual(["manifest"]);
    expect(report.summary.error).toBe(0);
    expect(() => checkExtension(path.join(clean, "gone"))).toThrow("no package.json");
  });
});
