//! What the runtime's tests share: Heyo's directories in a temporary place,
//! a host on the other end of the runtime's conversation (`testhost.ts`),
//! and small extensions written there, which run as real ones do.
//!
//! A test imports this before anything of the runtime: the directories are
//! set here before `context.ts` reads them, which is why the runtime's own
//! modules are loaded from here only once that is done.

import fs from "node:fs";
import os from "node:os";
import path from "node:path";

import type { Incoming } from "../../shared/wire";
import type { Identity } from "./runtime";
import { testHost } from "./testhost";
import { pair } from "./transport";

const root = fs.mkdtempSync(path.join(os.tmpdir(), "heyo-runtime-test-"));
process.env.HEYO_DATA_DIR = path.join(root, "data");
process.env.HEYO_CACHE_DIR = path.join(root, "cache");
process.env.HEYO_CONFIG_DIR = path.join(root, "config");
process.env.HEYO_EXTENSIONS_DIR = path.join(root, "data", "extensions");
process.on("exit", () => fs.rmSync(root, { recursive: true, force: true }));

export const dataDir = process.env.HEYO_DATA_DIR;
export const supportDir = (extension: string) => path.join(dataDir, "support", extension);

// The conversation is the runtime's own; only what carries it is replaced.
const [ours, theirs] = pair();
const host = testHost(theirs);
void (await import("./protocol")).connect(ours);

/// What the runtime sent, the requests it made and how they are answered.
/// A request of an extension's (`ext.*`) is kept with whose it was.
export const { sent, requests, answers, reset } = host;

/// A message from the host.
export const receive: (message: Incoming) => void = host.tell;

/// An event from the host, for whoever subscribed.
export function event(subscription: number, data: unknown) {
  host.tell({ type: "event", subscription, data });
}

/// Starts the runtime with this host as `main.ts` does with Heyo: from
/// then on extensions are handed the API and React, and the host's
/// messages do what they do in Heyo.
export async function runtime(identity: Identity = { background: false }) {
  void (await import("./runtime")).start(ours, identity);
}

export const sleep = (ms: number) => new Promise((resolve) => setTimeout(resolve, ms));

/// Waits until `check` returns something, and returns it.
export async function until<T>(check: () => T | undefined | false | null, what = "the condition", limit = 3000): Promise<T> {
  const deadline = Date.now() + limit;
  for (;;) {
    const value = check();
    if (value) return value;
    if (Date.now() > deadline) throw new Error(`timed out waiting for ${what}`);
    await sleep(5);
  }
}

/// Writes an extension: `commands` maps a command's name to its mode and the
/// source of its built file. Each has a `token` preference whose default is
/// the extension's name, so whose preferences were read shows.
export function extension(name: string, commands: Record<string, { mode: "view" | "no-view" | "menu-bar"; source: string }>) {
  const dir = path.join(process.env.HEYO_EXTENSIONS_DIR!, name);
  fs.mkdirSync(dir, { recursive: true });
  const manifest = {
    name,
    title: name,
    author: "tester",
    commands: Object.entries(commands).map(([command, { mode }]) => ({ name: command, title: `${name} ${command}`, mode })),
    preferences: [{ name: "token", type: "textfield" as const, default: `token-of-${name}` }],
  };
  fs.writeFileSync(path.join(dir, "package.json"), JSON.stringify(manifest));
  for (const [command, { source }] of Object.entries(commands)) fs.writeFileSync(path.join(dir, `${command}.js`), source);
  return { dir, manifest };
}
