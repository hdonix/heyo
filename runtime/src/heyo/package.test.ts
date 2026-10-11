// The npm package (`packages/api`): its declarations stand on their own, and
// its `index.js`, which is what an extension gets outside Heyo, passes
// Raycast's API through and makes Heyo's part refuse.
import fs from "node:fs";
import { createRequire } from "node:module";
import os from "node:os";
import path from "node:path";
import ts from "typescript";
import { afterAll, expect, test } from "vitest";

import raycastProvided from "../api";
import provided from "./index";
import { gained, modules, type ModuleName } from "./members";

const packageDir = path.join(__dirname, "../../../packages/api");

// A project that depends on `@heyo/api`, with a stand-in for Raycast's API.
const project = fs.mkdtempSync(path.join(os.tmpdir(), "heyo-api-package-"));
afterAll(() => fs.rmSync(project, { recursive: true, force: true }));

const fakeRaycast = `
  const calls = [];
  let command = "first";
  module.exports = {
    calls,
    setCommand: (name) => (command = name),
    List: function List() {},
    showToast: async () => "toasted",
    environment: { raycastVersion: "1.99.0", get commandName() { return command; } },
    Clipboard: {
      copy: async (...args) => void calls.push(["copy", ...args]),
      paste: async (...args) => void calls.push(["paste", ...args]),
    },
    AI: {
      Model: { Anthropic_Claude_Opus: "anthropic-claude-opus" },
      ask: (...args) => {
        calls.push(["ask", ...args]);
        return Object.assign(Promise.resolve("answered"), { on() {} });
      },
    },
  };`;

function install() {
  const modules = path.join(project, "node_modules");
  fs.mkdirSync(path.join(modules, "@raycast/api"), { recursive: true });
  fs.writeFileSync(path.join(modules, "@raycast/api/package.json"), JSON.stringify({ name: "@raycast/api", version: "1.99.0", main: "index.js" }));
  fs.writeFileSync(path.join(modules, "@raycast/api/index.js"), fakeRaycast);
  fs.mkdirSync(path.join(modules, "@heyo/api"), { recursive: true });
  for (const file of ["package.json", "index.js", "index.d.ts"]) fs.copyFileSync(path.join(packageDir, file), path.join(modules, "@heyo/api", file));
  const load = createRequire(path.join(project, "extension.js"));
  return { heyo: load("@heyo/api"), raycast: load("@raycast/api") };
}

const { heyo, raycast } = install();
const ownModules = Object.keys(modules) as ModuleName[];

test("the package is @heyo/api and leans on the Raycast API the extension has", () => {
  const manifest = JSON.parse(fs.readFileSync(path.join(packageDir, "package.json"), "utf8"));
  expect(manifest).toMatchObject({ name: "@heyo/api", main: "index.js", types: "index.d.ts" });
  expect(Object.keys(manifest.peerDependencies)).toEqual(["@raycast/api"]);
  expect(manifest.dependencies).toBeUndefined();
  for (const file of manifest.files) expect(fs.existsSync(path.join(packageDir, file)), file).toBe(true);
});

test("the declarations have no errors of their own", () => {
  const config = path.join(__dirname, "../../tsconfig.json");
  const { options } = ts.parseJsonConfigFileContent(ts.readConfigFile(config, ts.sys.readFile).config, ts.sys, path.dirname(config));
  const file = path.join(packageDir, "index.d.ts");
  // Declaration files are otherwise passed over (`skipLibCheck`).
  const program = ts.createProgram([file], { ...options, skipLibCheck: false });
  const source = program.getSourceFile(file)!;
  const problems = [...program.getSyntacticDiagnostics(source), ...program.getSemanticDiagnostics(source)];
  expect(problems.map((problem) => ts.flattenDiagnosticMessageText(problem.messageText, "\n"))).toEqual([]);
}, 60_000);

test("outside Heyo, Raycast's API comes through under its own names", async () => {
  expect(heyo.List).toBe(raycast.List);
  expect(await heyo.showToast()).toBe("toasted");
  expect(heyo.AI.Model).toBe(raycast.AI.Model);
  expect(heyo.Clipboard.copy).toBe(raycast.Clipboard.copy);
  // As it is when it is used, not as it was when the package loaded.
  const later = () => {};
  raycast.showToast = later;
  expect(heyo.showToast).toBe(later);
  // The environment still follows the running command.
  expect(heyo.environment.raycastVersion).toBe("1.99.0");
  expect(heyo.environment.commandName).toBe("first");
  raycast.setCommand("second");
  expect(heyo.environment.commandName).toBe("second");
});

test("outside Heyo, an extension can ask before it uses anything of Heyo's", async () => {
  expect(heyo.isHeyo).toBe(false);
  expect(heyo.has("Windows.list")).toBe(false);
  expect(heyo.environment).toMatchObject({ heyoVersion: "", apiVersion: 0, session: "unknown", offline: false });
  expect(["linux", "windows", "macos"]).toContain(heyo.environment.platform);
  expect("offline" in heyo.environment && "commandName" in heyo.environment).toBe(true);
  for (const name of ownModules) expect(await heyo[name].supported(), name).toBe("none");
});

test("outside Heyo, everything else of Heyo's refuses with a code", async () => {
  let refused = 0;
  for (const name of ownModules) {
    for (const [member, call] of Object.entries<(...args: unknown[]) => unknown>(heyo[name])) {
      if (member === "supported" || `${name}.${member}` === "Dictation.listen") continue;
      const error = await Promise.resolve(call()).then(
        () => undefined,
        (e) => e,
      );
      expect(error, `${name}.${member}`).toBeInstanceOf(heyo.HeyoError);
      expect(error, `${name}.${member}`).toMatchObject({ name: "HeyoError", code: "unsupported" });
      expect(error.message).toContain(`${name}.${member}`);
      refused++;
    }
  }
  // Every function declared (`members.ts`) but `supported` and `listen`.
  expect(refused).toBe(ownModules.reduce((count, name) => count + Object.keys(modules[name]).length - 1, 0) - 1);
  await expect(heyo.AI.provider()).rejects.toMatchObject({ code: "unsupported" });

  // `listen` gives something to stop, whose result is the refusal.
  const listening = heyo.Dictation.listen({ onSegment() {} });
  expect(() => listening.stop()).not.toThrow();
  await expect(listening.result).rejects.toMatchObject({ code: "unsupported" });
  expect(new heyo.HeyoError("denied")).toBeInstanceOf(Error);
});

test("outside Heyo, what Raycast's modules gained falls back to what Raycast does", async () => {
  raycast.calls.length = 0;
  expect(await heyo.AI.ask("Why?", { creativity: "low" })).toBe("answered");
  expect(await heyo.AI.ask("Why?", { system: "Be brief.", creativity: "low" })).toBe("answered");
  await heyo.Clipboard.paste("hello", { restoreClipboard: true });
  expect(raycast.calls).toEqual([
    ["ask", "Why?", { creativity: "low" }],
    // Raycast's `ask` knows no system prompt: it goes before the question.
    ["ask", "Be brief.\n\nWhy?", { creativity: "low" }],
    ["paste", "hello"],
  ]);
  // Pictures cannot be handed over; the promise still has its `on`.
  const asked = heyo.AI.ask("What is this?", { images: ["/tmp/a.png"] });
  expect(typeof asked.on).toBe("function");
  await expect(asked).rejects.toMatchObject({ code: "unsupported" });
  expect(raycast.calls.length).toBe(3);
});

test("the package has Heyo's part by the names it is declared under", () => {
  const added = (module: object, base: object) => Object.keys(module).filter((name) => !Object.hasOwn(base, name)).sort();
  // The stand-in for Raycast's API has few names of its own; Heyo's are these.
  const own = Object.keys(heyo).filter((name) => !Object.hasOwn(raycast, name)).sort();
  expect(own).toEqual(added(provided, raycastProvided));
  for (const name of ownModules) expect(own, name).toContain(name);
  // Module by module, the functions that are declared (`members.ts`): no
  // other, and none missing.
  for (const name of ownModules) expect(Object.keys(heyo[name]).sort(), name).toEqual(Object.keys(modules[name]).sort());
  // What a module of Raycast's gains is a function of its own there, a new
  // one or one that takes more than Raycast's does.
  for (const [name, functions] of Object.entries(gained)) {
    for (const [each, declared] of Object.entries(functions)) {
      expect(Object.hasOwn(heyo[name], each), `${name}.${each}`).toBe(true);
      expect(heyo[name][each] !== raycast[name][each], `${name}.${each}`).toBe(true);
      expect(Object.hasOwn(raycast[name], each), `${name}.${each}`).toBe("given" in declared);
    }
    expect(added(heyo[name], raycast[name]), name).toEqual(Object.keys(functions).filter((each) => !Object.hasOwn(raycast[name], each)).sort());
  }
  expect(typeof heyo.has).toBe("function");
});
