// How much of `@raycast/api` Heyo provides, asked of the compiler: every value
// the real package exports, and the members of each, must exist on the object
// the runtime hands to extensions. What is knowingly absent is listed in
// `conformance.gaps.json`; anything else missing fails, and so does a gap
// that has since been filled.
//
// This compares names, three levels deep. It does not compare signatures or
// the props of components.
import fs from "node:fs";
import path from "node:path";
import ts from "typescript";
import { expect, test } from "vitest";

const here = __dirname;
const probe = path.join(here, "conformance.probe.ts");
const DEPTH = 3;

function missing(): string[] {
  const source = `import * as raycast from "@raycast/api";\nimport heyo from "./index";\nexport { raycast, heyo };\n`;
  const options = ts.parseJsonConfigFileContent(
    ts.readConfigFile(path.join(here, "../../tsconfig.json"), ts.sys.readFile).config,
    ts.sys,
    path.join(here, "../.."),
  ).options;
  const host = ts.createCompilerHost(options);
  const read = host.getSourceFile.bind(host);
  host.getSourceFile = (file, ...rest) => (file === probe ? ts.createSourceFile(file, source, ts.ScriptTarget.ES2023, true) : read(file, ...rest));
  const exists = host.fileExists.bind(host);
  host.fileExists = (file) => file === probe || exists(file);
  const program = ts.createProgram([probe], options, host);
  const checker = program.getTypeChecker();
  const module = checker.getSymbolAtLocation(program.getSourceFile(probe)!)!;
  const typeOf = (name: string) => {
    const symbol = checker.getExportsOfModule(module).find((s) => s.name === name)!;
    return checker.getTypeOfSymbol(symbol);
  };

  const gaps: string[] = [];
  const isValue = (symbol: ts.Symbol) => (symbol.flags & ts.SymbolFlags.Value) !== 0;
  /// What has members worth descending into: objects and classes, not strings and the like.
  const structured = (type: ts.Type) => (type.flags & ts.TypeFlags.Object) !== 0 && !checker.isArrayType(type);

  const compare = (theirs: ts.Type, ours: ts.Type, at: string, depth: number) => {
    for (const member of checker.getPropertiesOfType(theirs)) {
      if (!isValue(member) || member.name.startsWith("__") || member.name === "prototype") continue;
      // How React describes a component, not something an extension asks for.
      if (at && ["propTypes", "displayName", "defaultProps", "contextTypes", "$$typeof"].includes(member.name)) continue;
      // What the package keeps to itself is not part of what it offers.
      const hidden = ts.ModifierFlags.Private | ts.ModifierFlags.Protected;
      if (member.declarations?.some((d) => ts.getCombinedModifierFlags(d) & hidden)) continue;
      const name = at ? `${at}.${member.name}` : member.name;
      const mine = checker.getPropertyOfType(ours, member.name);
      if (!mine) {
        // A table that answers for any name (the AI models) has them all.
        if (!checker.getIndexInfoOfType(ours, ts.IndexKind.String)) gaps.push(name);
        continue;
      }
      const [theirType, myType] = [checker.getTypeOfSymbol(member), checker.getTypeOfSymbol(mine)];
      if (depth < DEPTH && structured(theirType) && structured(myType) && !(myType.flags & ts.TypeFlags.Any)) {
        compare(theirType, myType, name, depth + 1);
        // A class: what its instances have, too.
        const [theirNew, myNew] = [theirType.getConstructSignatures()[0], myType.getConstructSignatures()[0]];
        if (theirNew && myNew) compare(theirNew.getReturnType(), myNew.getReturnType(), `${name}#`, depth + 1);
      }
    }
  };
  compare(typeOf("raycast"), typeOf("heyo"), "", 1);
  return gaps.sort();
}

test("what Heyo lacks of @raycast/api is exactly what is listed", () => {
  const listed = JSON.parse(fs.readFileSync(path.join(here, "conformance.gaps.json"), "utf8")) as Record<string, string[]>;
  const known = Object.values(listed).flat().sort();
  const found = missing();
  if (process.env.HEYO_PRINT_GAPS) console.log(JSON.stringify(found, null, 1));
  expect(found.filter((name) => !known.includes(name)), "missing and not listed").toEqual([]);
  expect(known.filter((name) => !found.includes(name)), "listed but no longer missing").toEqual([]);
}, 60_000);
