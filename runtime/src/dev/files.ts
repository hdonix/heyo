//! The files of an extension's source directory.

import fs from "node:fs";
import path from "node:path";

const SOURCE = /\.(?:[cm]?[jt]s|[jt]sx)$/;
/// Never an extension's own code: what is installed, built or kept by tools.
const SKIPPED = new Set(["node_modules", "dist", "build", "out", "coverage", "metadata", "assets"]);

/// The JavaScript and TypeScript sources under `dir`, as paths relative to
/// it, `/`-separated and sorted.
export function sourceFiles(dir: string): string[] {
  const found: string[] = [];
  const walk = (relative: string) => {
    let entries: fs.Dirent[];
    try {
      entries = fs.readdirSync(path.join(dir, relative), { withFileTypes: true });
    } catch {
      return;
    }
    for (const entry of entries) {
      if (entry.name.startsWith(".")) continue;
      const child = relative ? `${relative}/${entry.name}` : entry.name;
      if (entry.isDirectory()) {
        if (!SKIPPED.has(entry.name)) walk(child);
      } else if (entry.isFile() && SOURCE.test(entry.name) && !entry.name.endsWith(".d.ts")) {
        found.push(child);
      }
    }
  };
  walk("");
  return found.sort();
}

/// The manifest of the extension in `dir`, and its text as written.
export function readPackage(dir: string): { text: string; manifest: any } {
  const file = path.join(dir, "package.json");
  let text: string;
  try {
    text = fs.readFileSync(file, "utf8");
  } catch {
    throw new Error(`${dir} is not an extension: it has no package.json`);
  }
  try {
    return { text, manifest: JSON.parse(text) };
  } catch (e) {
    throw new Error(`${file} is not valid JSON: ${e instanceof Error ? e.message : e}`);
  }
}

/// `exists` for `validateManifest`: whether a path relative to `dir` is a file.
export const existsIn = (dir: string) => (file: string) => {
  try {
    return fs.statSync(path.join(dir, file)).isFile();
  } catch {
    return false;
  }
};
