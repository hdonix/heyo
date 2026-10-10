//! `heyo migrate`: turns an extension written for Raycast into one written
//! for Heyo's API. The sources import `@heyo/api` where they imported
//! `@raycast/api` (the same object in Heyo, and a superset elsewhere), and
//! the manifest gains the `heyo` field and the dependency.
//!
//! `@raycast/utils` and everything else is left alone: it goes on asking
//! for `@raycast/api` and is handed the same object.

import fs from "node:fs";
import path from "node:path";

import { unifiedDiff } from "./diff";
import { readPackage, sourceFiles } from "./files";
import { lex, type Token } from "./lex";
import { API_VERSION } from "./manifest";

const FROM = "@raycast/api";
const TO = "@heyo/api";
/// The range of `@heyo/api` written into `dependencies`.
export const HEYO_API_RANGE = `^${API_VERSION}.0.0`;

/// Functions whose first argument names a module: loading it, or (in
/// tests) replacing it.
const LOADERS = new Set(["require", "import"]);
const MOCKERS = new Set(["mock", "doMock", "unmock", "dontMock", "requireActual", "requireMock", "importActual", "importMock"]);

/// Rewrites every place a source names the Raycast API as a module:
/// `import … from`, `import type`, `export … from`, a bare `import "…"`,
/// `require()`, `import()` and the mocking calls of jest and vitest. The
/// same text in a comment, in another string or as part of another name
/// (`@raycast/api-extra`, `@raycast/utils`) is not touched.
export function rewriteImports(source: string): { text: string; count: number } {
  const tokens = lex(source).filter((token) => token.kind !== "comment");
  const text = (token: Token | undefined) => (token ? source.slice(token.start, token.end) : "");
  const isWord = (token: Token | undefined, word?: string | Set<string>) =>
    token?.kind === "word" && (word === undefined || (typeof word === "string" ? text(token) === word : word.has(text(token))));

  const places: Token[] = [];
  tokens.forEach((token, index) => {
    if (token.kind !== "string" || source.slice(token.start + 1, token.end - 1) !== FROM) return;
    const before = tokens[index - 1];
    if (isWord(before, "from") || isWord(before, "import")) return void places.push(token);
    if (text(before) !== "(") return;
    // The function called, past type arguments: `importActual<typeof api>(…)`.
    let at = index - 2;
    if (text(tokens[at]) === ">") {
      for (let depth = 0; at >= 0; at--) {
        if (text(tokens[at]) === ">") depth++;
        else if (text(tokens[at]) === "<" && --depth === 0) break;
      }
      at--;
    }
    const callee = tokens[at];
    const member = text(tokens[at - 1]) === ".";
    if ((isWord(callee, LOADERS) && !member) || (isWord(callee, MOCKERS) && member)) places.push(token);
  });

  let out = "";
  let from = 0;
  for (const place of places) {
    out += source.slice(from, place.start + 1) + TO;
    from = place.end - 1;
  }
  return { text: out + source.slice(from), count: places.length };
}

/// The indentation a JSON text was written with.
function indentOf(text: string): string | number {
  return /^([ \t]+)"/m.exec(text)?.[1] ?? 2;
}

/// Adds the `heyo` field and the `@heyo/api` dependency to a manifest's
/// text, keeping what is there (an existing `heyo` field too) and how the
/// file is written.
export function migrateManifest(text: string): string {
  const manifest = JSON.parse(text);
  if (typeof manifest !== "object" || manifest === null || Array.isArray(manifest)) throw new Error("package.json is not an object");
  const before = JSON.stringify(manifest);

  if (manifest.heyo === undefined) manifest.heyo = { apiVersion: API_VERSION, permissions: [] };
  else {
    manifest.heyo.apiVersion ??= API_VERSION;
    manifest.heyo.permissions ??= [];
  }

  const dependencies: Record<string, string> = manifest.dependencies ?? {};
  if (!dependencies[TO] && !manifest.devDependencies?.[TO] && !manifest.peerDependencies?.[TO]) {
    const names = Object.keys(dependencies);
    const sorted = names.every((name, i) => i === 0 || names[i - 1].localeCompare(name) <= 0);
    dependencies[TO] = HEYO_API_RANGE;
    // npm keeps them in order; a list that was not is left as it is.
    manifest.dependencies = sorted ? Object.fromEntries(Object.entries(dependencies).sort(([a], [b]) => a.localeCompare(b))) : dependencies;
  }

  if (JSON.stringify(manifest) === before) return text;
  return JSON.stringify(manifest, null, indentOf(text)) + (text.endsWith("\n") ? "\n" : "");
}

export interface Change {
  /// Relative to the extension's directory.
  file: string;
  before: string;
  after: string;
  /// How many imports were rewritten (0 for the manifest).
  imports: number;
}

/// What migrating the extension in `dir` changes. Nothing is written.
export function planMigration(dir: string): Change[] {
  const changes: Change[] = [];
  const manifest = readPackage(dir).text;
  const migrated = migrateManifest(manifest);
  if (migrated !== manifest) changes.push({ file: "package.json", before: manifest, after: migrated, imports: 0 });
  for (const file of sourceFiles(dir)) {
    const before = fs.readFileSync(path.join(dir, file), "utf8");
    const { text, count } = rewriteImports(before);
    if (count > 0) changes.push({ file, before, after: text, imports: count });
  }
  return changes;
}

export function applyMigration(dir: string, changes: Change[]) {
  for (const change of changes) fs.writeFileSync(path.join(dir, change.file), change.after);
}

export const diffOf = (changes: Change[]) => changes.map((change) => unifiedDiff(change.file, change.before, change.after)).join("");
