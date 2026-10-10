//! The extensions being developed: `development.json` in Heyo's data
//! directory. `heyo dev` writes it, the host reads it to list their
//! commands (`extensions.rs`), and the runtime reads it to know whose
//! console to pass on.
//!
//! A development extension is built into Heyo's data directory, not run
//! from its source directory: the built files are what Heyo loads, and that
//! is where the webview may read icons from.

import fs from "node:fs";
import os from "node:os";
import path from "node:path";

export interface Development {
  /// The manifest's name.
  name: string;
  /// The source directory `heyo dev` was run on.
  source: string;
  /// Where it is built: the directory Heyo loads, and the extension's id.
  dir: string;
}

/// Heyo's data directory, as `paths.rs` has it. The host says where when it
/// starts this; the rest is for running without it.
export function dataRoot(): string {
  if (process.env.HEYO_DATA_DIR) return process.env.HEYO_DATA_DIR;
  const home = process.platform === "win32" ? process.env.APPDATA : undefined;
  return path.join(process.env.XDG_DATA_HOME || home || path.join(os.homedir(), ".local/share"), "heyo");
}

export const registryFile = () => path.join(dataRoot(), "development.json");
export const buildDir = (name: string) => path.join(dataRoot(), "development", name);
/// What the extension's commands print while they run in Heyo.
export const logFile = (dir: string) => `${dir}.log`;

export function registered(): Development[] {
  try {
    const { extensions } = JSON.parse(fs.readFileSync(registryFile(), "utf8"));
    return (Array.isArray(extensions) ? extensions : []).filter(
      (e): e is Development => typeof e?.name === "string" && typeof e?.source === "string" && typeof e?.dir === "string",
    );
  } catch {
    return [];
  }
}

function write(extensions: Development[]) {
  const file = registryFile();
  fs.mkdirSync(path.dirname(file), { recursive: true });
  const partial = `${file}.${process.pid}.tmp`;
  fs.writeFileSync(partial, JSON.stringify({ extensions }, null, 2) + "\n");
  fs.renameSync(partial, file);
}

/// The list with `entry` in it, in place of whatever had its name or was
/// built from its source directory.
export function withEntry(list: Development[], entry: Development): Development[] {
  return [...list.filter((e) => e.name !== entry.name && e.source !== entry.source), entry];
}

/// Splits the list by what `key` names: a source directory, a built
/// directory or an extension's name.
export function withoutEntry(list: Development[], key: string): { kept: Development[]; removed: Development[] } {
  const resolved = path.resolve(key);
  const matches = (e: Development) => e.name === key || e.source === resolved || e.dir === resolved || e.dir === key;
  return { kept: list.filter((e) => !matches(e)), removed: list.filter(matches) };
}

export function register(entry: Development) {
  write(withEntry(registered(), entry));
}

/// Forgets a development extension and deletes what was built of it. The
/// sources are not touched.
export function unregister(key: string): Development[] {
  const { kept, removed } = withoutEntry(registered(), key);
  if (removed.length === 0) return [];
  write(kept);
  for (const entry of removed) {
    // Only ever what `heyo dev` put there.
    if (path.dirname(entry.dir) !== path.join(dataRoot(), "development")) continue;
    fs.rmSync(entry.dir, { recursive: true, force: true });
    fs.rmSync(logFile(entry.dir), { force: true });
  }
  return removed;
}
