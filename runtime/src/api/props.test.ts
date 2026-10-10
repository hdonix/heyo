// Which props of Raycast's components Heyo never looks at. For each
// component of the real package, the names of its props are taken from its
// type; a prop whose name occurs nowhere in the code that renders components
// (the webview, and the runtime's own handling) cannot be doing anything.
// Those are listed in `props.gaps.json`, and the test fails when the list
// and the code disagree.
//
// This finds props that are certainly ignored. A prop that is mentioned may
// still be handled only in part: that takes looking, not counting.
import fs from "node:fs";
import path from "node:path";
import ts from "typescript";
import { expect, test } from "vitest";

const here = __dirname;
const repo = path.join(here, "../../..");
const probe = path.join(here, "props.probe.ts");

/// Everything that could read a prop: the webview and the runtime's components.
function renderingCode(): string {
  const files: string[] = [];
  const walk = (dir: string) => {
    for (const entry of fs.readdirSync(dir, { withFileTypes: true })) {
      const file = path.join(dir, entry.name);
      if (entry.isDirectory()) walk(file);
      else if (/\.tsx?$/.test(entry.name) && !/\.test\.tsx?$/.test(entry.name)) files.push(file);
    }
  };
  walk(path.join(repo, "src"));
  for (const file of ["components.ts", "navigation.tsx"]) files.push(path.join(here, file));
  for (const file of ["background.ts", "session.ts", "renderer.ts"]) files.push(path.join(here, "..", file));
  return files.map((file) => fs.readFileSync(file, "utf8")).join("\n");
}

function unread(): Record<string, string[]> {
  const options = ts.parseJsonConfigFileContent(
    ts.readConfigFile(path.join(here, "../../tsconfig.json"), ts.sys.readFile).config,
    ts.sys,
    path.join(here, "../.."),
  ).options;
  const host = ts.createCompilerHost(options);
  const read = host.getSourceFile.bind(host);
  const source = `import * as raycast from "@raycast/api";\nexport { raycast };\n`;
  host.getSourceFile = (file, ...rest) => (file === probe ? ts.createSourceFile(file, source, ts.ScriptTarget.ES2023, true) : read(file, ...rest));
  const exists = host.fileExists.bind(host);
  host.fileExists = (file) => file === probe || exists(file);
  const program = ts.createProgram([probe], options, host);
  const checker = program.getTypeChecker();
  const module = checker.getSymbolAtLocation(program.getSourceFile(probe)!)!;
  const root = checker.getTypeOfSymbol(checker.getExportsOfModule(module)[0]);

  const code = renderingCode();
  const mentioned = (name: string) => new RegExp(`(^|[^\\w$])${name}([^\\w$]|$)`).test(code);
  const result: Record<string, string[]> = {};

  const visit = (type: ts.Type, name: string, depth: number) => {
    // A component: called with its props, and named like one.
    const call = type.getCallSignatures()[0];
    const last = name.split(".").pop()!;
    if (call && call.parameters.length === 1 && /^[A-Z]/.test(last)) {
      const props = checker.getTypeOfSymbol(call.parameters[0]);
      const names = checker
        .getPropertiesOfType(checker.getNonNullableType(props))
        .map((p) => p.name)
        .filter((p) => p !== "children" && p !== "key" && p !== "ref");
      const missing = names.filter((p) => !mentioned(p)).sort();
      if (missing.length) result[name] = missing;
    }
    if (depth >= 4) return;
    for (const member of checker.getPropertiesOfType(type)) {
      if (!/^[A-Z]/.test(member.name)) continue;
      const memberType = checker.getTypeOfSymbol(member);
      if (memberType.getCallSignatures().length) visit(memberType, `${name}.${member.name}`, depth + 1);
    }
  };
  for (const name of ["List", "Grid", "Detail", "Form", "Action", "ActionPanel", "MenuBarExtra"]) {
    const symbol = checker.getPropertyOfType(root, name);
    if (symbol) visit(checker.getTypeOfSymbol(symbol), name, 1);
  }
  return result;
}

test("the props Heyo never reads are exactly the ones listed", () => {
  const found = unread();
  if (process.env.HEYO_PRINT_GAPS) console.log(JSON.stringify(found, null, 1));
  const listed = JSON.parse(fs.readFileSync(path.join(here, "props.gaps.json"), "utf8"));
  expect(found).toEqual(listed);
}, 60_000);
