import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { afterAll, describe, expect, it } from "vitest";

import { unifiedDiff } from "./diff";
import { applyMigration, diffOf, HEYO_API_RANGE, migrateManifest, planMigration, rewriteImports } from "./migrate";

const rewritten = (source: string) => rewriteImports(source).text;

describe("rewriting imports", () => {
  it.each([
    ['import { List } from "@raycast/api";', 'import { List } from "@heyo/api";'],
    ["import { List } from '@raycast/api';", "import { List } from '@heyo/api';"],
    ['import type { LaunchProps } from "@raycast/api";', 'import type { LaunchProps } from "@heyo/api";'],
    ['import api, { type Image, Icon as I } from "@raycast/api"', 'import api, { type Image, Icon as I } from "@heyo/api"'],
    ['import * as raycast from "@raycast/api";', 'import * as raycast from "@heyo/api";'],
    ['import {\n  Action,\n  ActionPanel,\n} from "@raycast/api";', 'import {\n  Action,\n  ActionPanel,\n} from "@heyo/api";'],
    ['import "@raycast/api";', 'import "@heyo/api";'],
    ['export { showToast } from "@raycast/api";', 'export { showToast } from "@heyo/api";'],
    ['export * from "@raycast/api";', 'export * from "@heyo/api";'],
    ['export type { Image } from "@raycast/api";', 'export type { Image } from "@heyo/api";'],
    ['const { List } = require("@raycast/api");', 'const { List } = require("@heyo/api");'],
    ["const api = require( '@raycast/api' )", "const api = require( '@heyo/api' )"],
    ['import api = require("@raycast/api");', 'import api = require("@heyo/api");'],
    ['const api = await import("@raycast/api");', 'const api = await import("@heyo/api");'],
    ['type Api = typeof import("@raycast/api");', 'type Api = typeof import("@heyo/api");'],
    ['import{List}from"@raycast/api"', 'import{List}from"@heyo/api"'],
  ])("%s", (before, after) => {
    expect(rewriteImports(before)).toEqual({ text: after, count: 1 });
  });

  it("rewrites what tests replace the module with", () => {
    expect(rewritten('vi.mock("@raycast/api", () => ({}));')).toBe('vi.mock("@heyo/api", () => ({}));');
    expect(rewritten('jest.mock("@raycast/api");')).toBe('jest.mock("@heyo/api");');
    expect(rewritten('await vi.importActual<typeof import("@raycast/api")>("@raycast/api")')).toBe(
      'await vi.importActual<typeof import("@heyo/api")>("@heyo/api")',
    );
  });

  it("leaves every other module alone", () => {
    const source = [
      'import { useFetch } from "@raycast/utils";',
      'import x from "@raycast/api-extra";',
      'import y from "@raycast/api/package.json";',
      'import z from "./@raycast/api";',
      'import React from "react";',
    ].join("\n");
    expect(rewriteImports(source)).toEqual({ text: source, count: 0 });
  });

  it("leaves the name alone where it is not a module being loaded", () => {
    const source = [
      '// import { List } from "@raycast/api";',
      '/* require("@raycast/api") */',
      'const name = "@raycast/api";',
      'const text = `import { List } from "@raycast/api"`;',
      'console.log("install", "@raycast/api");',
      'const notRequire = thing.require("@raycast/api");',
      'declare module "@raycast/api" {}',
    ].join("\n");
    expect(rewriteImports(source)).toEqual({ text: source, count: 0 });
  });

  it("is not thrown by what comes before the import", () => {
    const source = [
      "const pattern = /\"[^/]*'/g;",
      "const half = 1 / 2; // it's",
      "const tag = `a ${b ? `'` : '\"'} c`;",
      'const jsx = <p className="x">don\'t</p>;',
      'const late = require("@raycast/api");',
    ].join("\n");
    expect(rewriteImports(source).count).toBe(1);
    expect(rewritten(source)).toContain('require("@heyo/api")');
  });

  it("changes nothing else in a whole file", () => {
    const before = `import { Detail, showToast } from "@raycast/api";
import { usePromise } from "@raycast/utils";

// Uses "@raycast/api" for everything.
export default function Command() {
  const { data } = usePromise(async () => (await import("@raycast/api")).environment.commandName);
  return <Detail markdown={\`# \${data}\`} />;
}
`;
    const after = rewritten(before);
    expect(after).toBe(
      before.replace('showToast } from "@raycast/api"', 'showToast } from "@heyo/api"').replace('import("@raycast/api")', 'import("@heyo/api")'),
    );
  });
});

describe("migrating the manifest", () => {
  const manifest = (extra: object = {}) =>
    JSON.stringify({ name: "x", title: "X", commands: [], dependencies: { "@raycast/api": "^1.100.0", "@raycast/utils": "^2.0.0" }, ...extra }, null, 2) + "\n";

  it("adds the heyo field and the dependency, and keeps the rest", () => {
    const after = JSON.parse(migrateManifest(manifest()));
    expect(after.heyo).toEqual({ apiVersion: 1, permissions: [] });
    // Both stay: @raycast/utils asks for @raycast/api, and the types come from it.
    expect(after.dependencies).toEqual({ "@heyo/api": HEYO_API_RANGE, "@raycast/api": "^1.100.0", "@raycast/utils": "^2.0.0" });
    expect(Object.keys(after.dependencies)).toEqual(["@heyo/api", "@raycast/api", "@raycast/utils"]);
    expect(after.name).toBe("x");
  });

  it("keeps what the author already wrote", () => {
    const text = manifest({ heyo: { apiVersion: 1, permissions: ["windows"], platforms: ["linux"] }, dependencies: { "@heyo/api": "1.2.3" } });
    expect(migrateManifest(text)).toBe(text);
    const partial = JSON.parse(migrateManifest(manifest({ heyo: { permissions: ["ai"] } })));
    expect(partial.heyo).toEqual({ permissions: ["ai"], apiVersion: 1 });
  });

  it("writes the file the way it was written", () => {
    const tabs = JSON.stringify({ name: "x", dependencies: { zeta: "1", alpha: "1" } }, null, "\t");
    const after = migrateManifest(tabs);
    expect(after).toContain('\n\t"heyo": {\n\t\t"apiVersion": 1');
    expect(after.endsWith("\n")).toBe(false);
    // A list that was not in order is not reordered.
    expect(Object.keys(JSON.parse(after).dependencies)).toEqual(["zeta", "alpha", "@heyo/api"]);
  });

  it("makes dependencies when there were none", () => {
    expect(JSON.parse(migrateManifest('{ "name": "x" }')).dependencies).toEqual({ "@heyo/api": HEYO_API_RANGE });
  });
});

describe("a diff", () => {
  it("shows the changed lines with what is around them", () => {
    const before = ["a", "b", "c", "d", "e", "f", "g", "h", "i", "j"].join("\n");
    const after = before.replace("e", "E");
    expect(unifiedDiff("x.ts", before, after, 1)).toBe("--- a/x.ts\n+++ b/x.ts\n@@ -4,3 +4,3 @@\n d\n-e\n+E\n f\n");
    expect(unifiedDiff("x.ts", before, before)).toBe("");
  });

  it("keeps changes far apart in hunks of their own, and counts added lines", () => {
    const before = Array.from({ length: 30 }, (_, i) => `line ${i + 1}`);
    const after = [...before];
    after[1] = "changed";
    after.splice(25, 0, "added");
    const diff = unifiedDiff("f", before.join("\n"), after.join("\n"), 2);
    expect(diff.match(/^@@.*@@$/gm)).toEqual(["@@ -1,4 +1,4 @@", "@@ -24,4 +24,5 @@"]);
    expect(diff).toContain("\n+added\n");
  });
});

describe("migrating a directory", () => {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), "heyo-migrate-test-"));
  afterAll(() => fs.rmSync(dir, { recursive: true, force: true }));
  const write = (file: string, text: string) => {
    fs.mkdirSync(path.dirname(path.join(dir, file)), { recursive: true });
    fs.writeFileSync(path.join(dir, file), text);
  };
  const read = (file: string) => fs.readFileSync(path.join(dir, file), "utf8");

  write("package.json", JSON.stringify({ name: "x", commands: [{ name: "index" }], dependencies: { "@raycast/api": "^1.0.0" } }, null, 2) + "\n");
  write("src/index.tsx", 'import { List } from "@raycast/api";\nimport { helper } from "./lib/helper";\nexport default () => <List />;\n');
  write("src/lib/helper.ts", 'import { showHUD } from "@raycast/api";\nimport { showFailureToast } from "@raycast/utils";\nexport const helper = showHUD;\n');
  write("src/plain.ts", "export const one = 1;\n");
  write("node_modules/dep/index.js", 'require("@raycast/api");\n');
  write("dist/index.js", 'require("@raycast/api");\n');

  it("a dry run says what would change and writes nothing", () => {
    const changes = planMigration(dir);
    expect(changes.map((c) => [c.file, c.imports])).toEqual([
      ["package.json", 0],
      ["src/index.tsx", 1],
      ["src/lib/helper.ts", 1],
    ]);
    const diff = diffOf(changes);
    expect(diff).toContain('-import { List } from "@raycast/api";\n+import { List } from "@heyo/api";');
    expect(diff).toContain('+  "heyo": {');
    expect(diff).not.toContain("@raycast/utils\";\n+");
    expect(read("src/index.tsx")).toContain("@raycast/api");
  });

  it("the real run writes it, and a second finds nothing left", () => {
    applyMigration(dir, planMigration(dir));
    expect(read("src/index.tsx")).toContain('from "@heyo/api"');
    expect(read("src/lib/helper.ts")).toBe('import { showHUD } from "@heyo/api";\nimport { showFailureToast } from "@raycast/utils";\nexport const helper = showHUD;\n');
    expect(JSON.parse(read("package.json")).heyo).toEqual({ apiVersion: 1, permissions: [] });
    // What is installed and what is built are not the extension's sources.
    expect(read("node_modules/dep/index.js")).toContain("@raycast/api");
    expect(read("dist/index.js")).toContain("@raycast/api");
    expect(planMigration(dir)).toEqual([]);
  });
});
