// The declaration of `@heyo/api` (`members.ts`) against what the host
// declares of the same requests (`shared/bindings/needs.ts`, written by the
// host's tests from `permissions.rs`): every request is somebody's, the
// host names each function as it is declared here, and a function needs
// what its request needs.
import fs from "node:fs";
import path from "node:path";
import { expect, test } from "vitest";

import { needs, permissions } from "../../../shared/bindings/needs";
import { beside, gained, members, modules, permissionsOf, type ModuleName } from "./members";

const methods = Object.keys(needs) as (keyof typeof needs)[];

test("every request the host answers is a declared function's, or is accounted for beside them", () => {
  const declared = new Set<string>(members.map((member) => member.method));
  expect(methods.filter((method) => !declared.has(method) && !(method in beside)), "nobody asks for these").toEqual([]);
  expect(Object.keys(beside).filter((method) => declared.has(method)), "said twice").toEqual([]);
  expect([...declared, ...Object.keys(beside)].filter((method) => !(method in needs)), "the host does not answer these").toEqual([]);
});

test("the host names a function, when it refuses it, as it is declared", () => {
  for (const method of methods) {
    const asking = members.filter((member) => member.method === method);
    const named = needs[method]?.member ?? null;
    if (method in beside) {
      expect(named, method).toBe((beside as Record<string, string | null>)[method]);
    } else if (named === null) {
      // What needs nothing is never refused, and has no name there: only
      // then may several functions be one request (`supported`).
      expect(asking.length, method).toBeGreaterThan(0);
    } else {
      expect(asking.map((member) => member.member), method).toContain(named);
      // Any other that comes to it is one of Raycast's, given more.
      expect(asking.filter((member) => member.member !== named && !member.given), method).toEqual([]);
    }
  }
});

test("a function needs what its request needs", () => {
  const needed = (name: string) => members.find((member) => member.member === name)!.permission;
  expect(needed("Windows.list")).toBe("windows");
  expect(needed("LiveActivity.start")).toBe("activity");
  // Asking what is possible is free.
  expect(members.filter((member) => member.name === "supported").map((member) => member.permission)).toEqual(Object.keys(modules).map(() => null));
  expect(needed("LiveActivity.presentation")).toBeNull();
  // One of Raycast's needs it only for what Heyo adds.
  expect([needed("AI.provider"), needed("AI.ask"), needed("Clipboard.paste")]).toEqual(["ai", "ai", "input.paste"]);
  expect(members.filter((member) => member.given).map((member) => member.member)).toEqual(["AI.ask", "Clipboard.paste"]);
  expect(Object.keys(gained)).toEqual(["AI", "Clipboard"]);

  // A module's, between its functions: not what nothing of it uses yet.
  expect(permissionsOf("Screen")).toEqual(["screen.capture", "screen.text"]);
  expect(permissionsOf("Memory")).toEqual(["memory.read"]);
  for (const module of Object.keys(modules)) expect(permissionsOf(module as ModuleName).length, module).toBeGreaterThan(0);
  for (const member of members) if (member.permission) expect(permissions, member.member).toContain(member.permission);
});

test("what an author reads names every permission, module and function", () => {
  const read = (file: string) => fs.readFileSync(path.join(__dirname, "../../..", file), "utf8");
  const spec = read("docs/extension-api.md");
  const readme = read("packages/api/README.md");
  // The specification's table of permissions (5.1) has a row for each the host has.
  for (const permission of permissions) expect(spec, permission).toContain(`\n| \`${permission}\` |`);
  /// A module as the specification writes it out (section 7).
  const written = (module: string) => new RegExp(`\\nexport const ${module}: [^\\n]*\\{\\n([\\s\\S]*?)\\n\\};`).exec(spec)?.[1] ?? "";
  for (const { module, name, member, permission } of members) {
    expect(written(module), member).toContain(`  ${name}(`);
    if (permission) expect(readme, member).toContain(`\`${permission}\``);
    if (module in gained) expect(readme, member).toContain(`\`${member}(`);
    else if (name !== "supported") expect(readme.includes(`\`${name}\``) || readme.includes(`\`${member}\``), member).toBe(true);
  }
  for (const module of Object.keys(modules)) expect(readme, module).toContain(`\`${module}\``);
});

test("every module of Heyo's can say how well this session supports it", () => {
  for (const [module, functions] of Object.entries(modules)) {
    expect(functions.supported, module).toMatchObject({ method: "ext.supported" });
    expect(functions.supported.params?.(), module).toEqual({ module: module === "LiveActivity" ? "activity" : module[0].toLowerCase() + module.slice(1) });
  }
});
