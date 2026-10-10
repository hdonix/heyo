//! What the runtime's tests share: a stand-in for the protocol (the real one
//! takes over stdout when loaded), Heyo's directories in a temporary place,
//! and small extensions written there, which run as real ones do.
//!
//! Imports nothing of the runtime itself, so a test that imports this first
//! has the directories set before `context.ts` reads them.

import fs from "node:fs";
import os from "node:os";
import path from "node:path";

const root = fs.mkdtempSync(path.join(os.tmpdir(), "heyo-runtime-test-"));
process.env.HEYO_DATA_DIR = path.join(root, "data");
process.env.HEYO_CACHE_DIR = path.join(root, "cache");
process.env.HEYO_CONFIG_DIR = path.join(root, "config");
process.env.HEYO_EXTENSIONS_DIR = path.join(root, "data", "extensions");
process.on("exit", () => fs.rmSync(root, { recursive: true, force: true }));

export const supportDir = (extension: string) => path.join(root, "data", "support", extension);

/// What the runtime sent, the requests it made and how they are answered.
/// A request of an extension's (`ext.*`) is kept with whose it was.
export const sent: any[] = [];
export const requests: { method: string; params?: any; extension?: string; dir?: string; via?: string }[] = [];
export const answers: Record<string, (params: any) => unknown> = {};
const handlers = new Map<string, (message: any) => void>();
const hearing = new Map<number, (data: unknown) => void>();

/// Makes an extension's request as the real protocol does: in the name of
/// the command that is running.
async function ask(via: string | undefined, method: string, params?: unknown) {
  const [{ current }, { envelope }, { HeyoError }] = await Promise.all([import("./context"), import("./heyo/caller"), import("./heyo/error")]);
  const whose = envelope(current.value, via as "raycast" | undefined);
  if (!whose) throw new HeyoError("invalid", `${method} was asked for outside any command.`);
  requests.push({ method, params, ...whose });
  return answers[method]?.(params);
}

/// The module to mock `./protocol` with.
export const protocol = {
  send: (message: unknown) => void sent.push(message),
  on: (type: string, handler: (message: any) => void) => void handlers.set(type, handler),
  request: async (method: string, params?: unknown) => {
    requests.push({ method, params });
    return answers[method]?.(params);
  },
  ask: (method: string, params?: unknown) => ask(undefined, method, params),
  askThroughRaycast: (method: string, params?: unknown) => ask("raycast", method, params),
  hear: (subscription: number, listener: (data: unknown) => void) => {
    hearing.set(subscription, listener);
    return () => void hearing.delete(subscription);
  },
  start() {},
};

/// An event from the host, for whoever subscribed. False if nobody hears it.
export function event(subscription: number, data: unknown) {
  hearing.get(subscription)?.(data);
  return hearing.has(subscription);
}

/// A message from the host.
export function receive(message: { type: string } & Record<string, unknown>) {
  handlers.get(message.type)?.(message);
}

export function reset() {
  sent.length = 0;
  requests.length = 0;
  for (const method of Object.keys(answers)) delete answers[method];
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

/// Hands extensions the API and React the way `main.ts` does.
export async function provideModules() {
  const [{ default: api }, { provide }, { scopedReact }] = await Promise.all([import("./heyo"), import("./session"), import("./context")]);
  provide({ "@raycast/api": api, "@heyo/api": api, react: scopedReact });
}
