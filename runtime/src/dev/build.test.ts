// Building real sources and running what was built, in the runtime as
// Heyo runs it: the built command asks for the API and React and is handed
// the runtime's own.
import * as esbuild from "esbuild";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { afterAll, beforeAll, beforeEach, describe, expect, it, vi } from "vitest";

import { reset, runtime, sent, sleep, until } from "../testing";

import { readManifest } from "../context";
import { activeSession, close, launch, relaunch } from "../session";
import { build, checkOut, copyStatic, esbuildOptions, ManifestError, prepare } from "./build";
import { format, refresh, teeConsole } from "./console";
import { follow } from "./develop";
import { buildDir, logFile, register, registered, unregister } from "./registry";

const sample = path.join(__dirname, "fixtures/sample");
const scratch = fs.mkdtempSync(path.join(os.tmpdir(), "heyo-build-test-"));
afterAll(() => fs.rmSync(scratch, { recursive: true, force: true }));

beforeAll(() => runtime());

beforeEach(async () => {
  close();
  await sleep(30);
  reset();
});

/// The titles of the list items in the view last rendered.
const titles = () => {
  const views = sent.filter((m) => m.type === "render" && m.view);
  const nodes = new Map<number, any>();
  for (const message of views) for (const node of message.view.nodes) nodes.set(node.id, node);
  return [...nodes.values()].filter((n) => n.type === "List.Item").map((n) => n.props.title);
};

describe("heyo build", () => {
  const out = path.join(scratch, "sample");

  it("makes what Heyo loads: the manifest, a file per command, the assets", async () => {
    // A command from before, no longer in the manifest.
    fs.mkdirSync(out, { recursive: true });
    fs.writeFileSync(path.join(out, "removed.js"), "");
    const built = await build(sample, out);
    expect(built.commands).toEqual(["hello", "quiet"]);
    expect(built.warnings).toEqual([]);
    expect(fs.readdirSync(out).sort()).toEqual(["assets", "hello.js", "package.json", "quiet.js"]);
    expect(fs.readFileSync(path.join(out, "package.json"), "utf8")).toBe(fs.readFileSync(path.join(sample, "package.json"), "utf8"));
    expect(fs.existsSync(path.join(out, "assets/icon.png"))).toBe(true);

    const hello = fs.readFileSync(path.join(out, "hello.js"), "utf8");
    // What the runtime provides is asked for, not carried.
    expect(hello).toContain('require("@raycast/api")');
    expect(hello).toContain('require("react/jsx-runtime")');
    expect(hello).toContain('require("react")');
    expect(hello).not.toContain("react.production");
    // What the extension itself has is carried.
    expect(hello).toContain("Bonjour");
    expect(hello.length).toBeLessThan(5000);
  });

  it("and it runs", async () => {
    launch({ dir: out, manifest: readManifest(out) }, "hello");
    await until(() => titles().length === 3, "the list");
    expect(titles()).toEqual(["Hello", "Hallo", "Bonjour"]);
    expect(sent.find((m) => m.type === "launched")).toMatchObject({ mode: "view", title: "Say Hello", extension: { name: "sample", dir: out } });
  });

  it("refuses to build over the sources", () => {
    expect(() => checkOut("/src/ext", "/src/ext")).toThrow("overwrite");
    expect(() => checkOut("/src/ext", "/src")).toThrow("overwrite");
    expect(() => checkOut("/src/ext", "/src/ext/src/out")).toThrow("overwrite");
    expect(() => checkOut("/src/ext", "/src/ext/assets")).toThrow("overwrite");
    expect(() => checkOut("/src/ext", "/src/ext/dist")).not.toThrow();
    expect(() => checkOut("/src/ext", "/src/ext-built")).not.toThrow();
  });

  it("says what is wrong with the manifest, and with the sources", async () => {
    const broken = path.join(scratch, "broken");
    fs.mkdirSync(path.join(broken, "src"), { recursive: true });
    fs.writeFileSync(path.join(broken, "package.json"), JSON.stringify({ name: "broken", title: "Broken", commands: [{ name: "a", title: "A", mode: "view" }, { name: "b", title: "B", mode: "tab" }] }));
    fs.writeFileSync(path.join(broken, "src/a.tsx"), "export default () => <List>;\n");
    const failure = await build(broken, path.join(scratch, "broken-out")).catch((e) => e);
    expect(failure).toBeInstanceOf(ManifestError);
    expect(failure.message).toContain('command "b" has the mode "tab"');
    expect(failure.message).toContain('command "b" has no source');
    expect(() => prepare(path.join(scratch, "nowhere"))).toThrow("no package.json");
  });

  it("asks esbuild for one CommonJS file per command, with the runtime's modules left out", () => {
    const options = esbuildOptions("/src/ext", [{ name: "a", entry: "src/a.tsx" }, { name: "b", entry: "src/b.ts" }], "/out", false);
    expect(options).toMatchObject({
      absWorkingDir: "/src/ext",
      entryPoints: { a: "src/a.tsx", b: "src/b.ts" },
      outdir: "/out",
      bundle: true,
      format: "cjs",
      platform: "node",
      jsx: "automatic",
      sourcemap: false,
      external: ["@raycast/api", "@heyo/api", "react", "react/jsx-runtime", "react/jsx-dev-runtime"],
    });
    expect(esbuildOptions("/src/ext", [], "/out", true).sourcemap).toBe("linked");
  });
});

describe("heyo dev", () => {
  // Sources of its own, since it is changed; nothing to install, as all it
  // imports is provided.
  const source = path.join(scratch, "draft-src");
  const write = (text: string) => {
    fs.mkdirSync(path.join(source, "src"), { recursive: true });
    fs.writeFileSync(path.join(source, "package.json"), JSON.stringify({ name: "draft", title: "Draft", commands: [{ name: "show", title: "Show", mode: "view" }] }));
    fs.writeFileSync(
      path.join(source, "src/show.tsx"),
      `import { List } from "@heyo/api";
export default function Show() {
  console.log("rendering", { item: ${JSON.stringify(text)} });
  console.warn("careful");
  return <List><List.Item title=${JSON.stringify(text)} /></List>;
}
`,
    );
  };
  const out = buildDir("draft");
  const rebuild = async () => {
    const prepared = prepare(source);
    await esbuild.build(esbuildOptions(source, prepared.commands, out, true));
    copyStatic(source, out, prepared);
  };

  it("lists the extension until it is taken out", async () => {
    write("first");
    await rebuild();
    register({ name: "draft", source, dir: out });
    register({ name: "draft", source, dir: out });
    expect(registered()).toEqual([{ name: "draft", source, dir: out }]);
    expect(fs.readdirSync(out).sort()).toEqual(["package.json", "show.js", "show.js.map"]);
  });

  it("passes on what its commands print, and starts the command again when it is rebuilt", async () => {
    fs.writeFileSync(logFile(out), "");
    const printed: string[] = [];
    const unfollow = follow(logFile(out), (text) => printed.push(text), 10);
    teeConsole();
    refresh();
    const quiet = vi.spyOn(process.stderr, "write").mockImplementation(() => true);
    try {
      launch({ dir: out, manifest: readManifest(out) }, "show");
      const first = activeSession()!.id;
      await until(() => titles().includes("first"), "the first build's view");
      await until(() => printed.join("").includes("careful"), "the console in the log");
      expect(printed.join("")).toMatch(/^\d\d:\d\d:\d\d \[show\] rendering \{ item: 'first' \}\n\d\d:\d\d:\d\d \[show\] warning: careful\n/);

      write("second");
      await rebuild();
      reset();
      // Another extension's rebuild is nothing to this command.
      relaunch(buildDir("other"));
      expect(activeSession()!.id).toBe(first);
      relaunch(out);
      expect(activeSession()!.id).not.toBe(first);
      await until(() => titles().includes("second"), "the second build's view");
      expect(sent.filter((m) => m.type === "launched").map((m) => m.title)).toEqual(["Show"]);
      await until(() => printed.join("").includes("'second'"), "the second build's console");
    } finally {
      quiet.mockRestore();
      unfollow();
    }
  });

  it("closes the command when its extension is gone", async () => {
    launch({ dir: out, manifest: readManifest(out) }, "show");
    await until(() => titles().length > 0, "the view");
    const session = activeSession()!.id;
    expect(unregister(source).map((e) => e.name)).toEqual(["draft"]);
    expect(registered()).toEqual([]);
    expect(fs.existsSync(out)).toBe(false);
    expect(fs.existsSync(logFile(out))).toBe(false);
    refresh();
    reset();
    const quiet = vi.spyOn(console, "error").mockImplementation(() => {});
    relaunch(out);
    quiet.mockRestore();
    expect(activeSession()).toBeNull();
    expect(sent.some((m) => m.type === "closed" && m.session === session)).toBe(true);
    expect(unregister(source)).toEqual([]);
  });

  it("writes a console call as a line of the log", () => {
    const at = new Date(2026, 0, 2, 3, 4, 5);
    expect(format("log", "show", ["a %s", "b", { c: 1 }], at)).toBe("03:04:05 [show] a b { c: 1 }\n");
    expect(format("error", "show", ["failed"], at)).toBe("03:04:05 [show] error: failed\n");
  });
});
