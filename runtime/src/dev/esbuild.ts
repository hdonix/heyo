//! Finding esbuild, which builds extensions.
//!
//! esbuild is a JavaScript package around a program compiled for one
//! system and processor, so it cannot be folded into this tool the way
//! plain JavaScript is. It is looked for in this order:
//!
//!  1. the extension's own `node_modules`. `@raycast/api` depends on
//!     esbuild (Raycast's `ray build` is esbuild too), so an extension with
//!     its dependencies installed always has one, for the right machine,
//!     and building needs no network;
//!  2. the one Heyo installed before (below);
//!  3. installed now into Heyo's data directory, the way the speech and
//!     text-recognition engines are (`install.ts`): one exact version from a
//!     lockfile that ships with Heyo, checked by npm against its hashes,
//!     no install scripts run.

import fs from "node:fs";
import { createRequire } from "node:module";
import path from "node:path";

import type * as Esbuild from "esbuild";

import { installPackages } from "../install";
import lock from "../locks/esbuild.json";
import { dataRoot } from "./registry";

export type Api = typeof Esbuild;

const toolDir = () => path.join(dataRoot(), "tools", "esbuild");

/// Where esbuild might be for the extension in `dir`, best first. Each is
/// a file to resolve from and a word for where that is.
export function candidates(dir: string): { from: string; where: string }[] {
  const places = [{ from: path.join(dir, "package.json"), where: "the extension's node_modules" }];
  try {
    // With pnpm, only what the extension names itself is at the top.
    const api = createRequire(places[0].from).resolve("@raycast/api/package.json");
    places.push({ from: api, where: "@raycast/api's dependencies" });
  } catch {
    // Not installed.
  }
  places.push({ from: path.join(toolDir(), "package.json"), where: "Heyo's data directory" });
  return places;
}

async function load(from: string): Promise<Api | undefined> {
  try {
    const require = createRequire(from);
    const api = require(require.resolve("esbuild")) as Api;
    // `context` came with 0.17; and a package without its program for this
    // machine loads fine and fails at the first use.
    if (typeof api.context !== "function") return undefined;
    await api.transform("0");
    return api;
  } catch {
    return undefined;
  }
}

/// esbuild for building the extension in `dir`, installing it if there is
/// none. `say` is told where it came from when that is worth knowing.
export async function findEsbuild(dir: string, say: (message: string) => void = () => {}): Promise<Api> {
  for (const { from } of candidates(dir)) {
    const api = await load(from);
    if (api) return api;
  }
  const version = lock.packages["node_modules/esbuild"].version;
  say(`No esbuild in ${path.join(dir, "node_modules")} (run npm install there to build without the network). Installing esbuild ${version} for Heyo…`);
  try {
    fs.rmSync(toolDir(), { recursive: true, force: true });
    await installPackages(toolDir(), lock);
  } catch (e) {
    throw new Error(`Could not install esbuild: ${e instanceof Error ? e.message : e}. Run npm install in ${dir} and try again.`);
  }
  const api = await load(path.join(toolDir(), "package.json"));
  if (!api) throw new Error(`esbuild was installed into ${toolDir()} but does not run on this machine (${process.platform} ${process.arch}).`);
  return api;
}
