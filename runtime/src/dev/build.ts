//! `heyo build`: an extension's sources made into what Heyo loads, which is
//! also what `ray build` makes and the Raycast Store hands out: the
//! manifest, one CommonJS file per command and the assets.
//!
//! Each command is bundled with everything it imports except what the
//! runtime provides (`PROVIDED`): the API under both its names, and React,
//! which must be the runtime's own for hooks to work.

import fs from "node:fs";
import path from "node:path";

import type { BuildOptions, Message } from "esbuild";

import { findEsbuild, type Api } from "./esbuild";
import { existsIn, readPackage } from "./files";
import { PROVIDED, validateManifest, type CommandEntry, type Problem } from "./manifest";

/// The manifest does not describe something that can be built.
export class ManifestError extends Error {
  constructor(public problems: Problem[]) {
    super(problems.filter((p) => p.severity === "error").map((p) => p.message).join("\n"));
  }
}

export interface Prepared {
  name: string;
  /// The manifest as written, to copy.
  manifest: string;
  commands: CommandEntry[];
  warnings: string[];
}

/// Reads and checks the extension in `dir`. Throws a `ManifestError` if it
/// cannot be built as it is.
export function prepare(dir: string): Prepared {
  const { text, manifest } = readPackage(dir);
  const { problems, commands } = validateManifest(manifest, existsIn(dir));
  if (problems.some((p) => p.severity === "error")) throw new ManifestError(problems);
  return { name: manifest.name, manifest: text, commands, warnings: problems.map((p) => p.message) };
}

/// How esbuild is asked to build the commands. `development` builds are
/// for `heyo dev`: with source maps, so errors name the lines of the source.
export function esbuildOptions(dir: string, commands: CommandEntry[], out: string, development: boolean): BuildOptions {
  return {
    absWorkingDir: dir,
    entryPoints: Object.fromEntries(commands.map((command) => [command.name, command.entry])),
    outdir: out,
    bundle: true,
    platform: "node",
    target: "node22",
    format: "cjs",
    jsx: "automatic",
    external: PROVIDED,
    sourcemap: development ? "linked" : false,
    define: { "process.env.NODE_ENV": development ? '"development"' : '"production"' },
    logLevel: "silent",
  };
}

/// Refuses an output directory that building into would damage.
export function checkOut(dir: string, out: string) {
  const inside = (parent: string, child: string) => {
    const relative = path.relative(parent, child);
    return relative === "" || (!relative.startsWith("..") && !path.isAbsolute(relative));
  };
  if (inside(out, dir) || inside(path.join(dir, "src"), out) || inside(path.join(dir, "assets"), out) || inside(path.join(dir, "node_modules"), out)) {
    throw new Error(`Cannot build into ${out}: it would overwrite the extension's own files. Choose another directory with -o.`);
  }
}

/// Puts the manifest and the assets beside the built commands, and removes
/// the files of commands that are no longer in the manifest.
export function copyStatic(dir: string, out: string, prepared: Prepared) {
  fs.mkdirSync(out, { recursive: true });
  const built = new Set(prepared.commands.flatMap((c) => [`${c.name}.js`, `${c.name}.js.map`]));
  for (const file of fs.readdirSync(out)) {
    if (/\.js(?:\.map)?$/.test(file) && !built.has(file)) fs.rmSync(path.join(out, file), { force: true });
  }
  fs.writeFileSync(path.join(out, "package.json"), prepared.manifest);
  const assets = path.join(dir, "assets");
  fs.rmSync(path.join(out, "assets"), { recursive: true, force: true });
  if (fs.existsSync(assets)) fs.cpSync(assets, path.join(out, "assets"), { recursive: true });
}

export const formatMessages = (api: Api, messages: Message[], kind: "error" | "warning", color: boolean) =>
  api.formatMessagesSync(messages, { kind, color, terminalWidth: process.stderr.columns || 100 }).join("");

export interface Built {
  out: string;
  commands: string[];
  /// Warnings of the manifest check and of esbuild, formatted.
  warnings: string[];
}

/// Builds the extension in `dir` into `out`. Throws with esbuild's own
/// account of what is wrong when the sources do not build.
export async function build(dir: string, out: string, options: { sourcemap?: boolean; say?: (message: string) => void; color?: boolean } = {}): Promise<Built> {
  dir = path.resolve(dir);
  out = path.resolve(out);
  checkOut(dir, out);
  const prepared = prepare(dir);
  const api = await findEsbuild(dir, options.say);
  try {
    const settings = { ...esbuildOptions(dir, prepared.commands, out, false), sourcemap: options.sourcemap ? ("linked" as const) : false };
    const result = await api.build(settings).catch((e: { errors?: Message[] }) => {
      if (!e.errors?.length) throw e;
      throw new Error(formatMessages(api, e.errors, "error", !!options.color).trimEnd());
    });
    copyStatic(dir, out, prepared);
    const warnings = result.warnings.length ? [formatMessages(api, result.warnings, "warning", !!options.color).trimEnd()] : [];
    return { out, commands: prepared.commands.map((c) => c.name), warnings: [...prepared.warnings, ...warnings] };
  } finally {
    await api.stop();
  }
}
