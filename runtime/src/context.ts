//! Extension manifests, on-disk locations and the running command's context.

import { AsyncLocalStorage } from "node:async_hooks";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import React from "react";

// The host says where Heyo keeps things (`paths.rs`); the fallbacks are for
// running the runtime on its own.
const home = os.homedir();
const root = (given: string, xdg: string, fallback: string) =>
  process.env[given] || path.join(process.env[xdg] || path.join(home, fallback), "heyo");

const dataRoot = root("HEYO_DATA_DIR", "XDG_DATA_HOME", ".local/share");
export const cacheRoot = root("HEYO_CACHE_DIR", "XDG_CACHE_HOME", ".cache");
export const extensionsDir = process.env.HEYO_EXTENSIONS_DIR || path.join(dataRoot, "extensions");
const supportRoot = path.join(dataRoot, "support");
const preferencesDir = path.join(root("HEYO_CONFIG_DIR", "XDG_CONFIG_HOME", ".config"), "preferences");

export interface Preference {
  name: string;
  type: "textfield" | "password" | "checkbox" | "dropdown" | "appPicker" | "file" | "directory";
  title?: string;
  label?: string;
  description?: string;
  placeholder?: string;
  required?: boolean;
  default?: unknown;
  data?: { title: string; value: string }[];
}

export interface Argument {
  name: string;
  type: "text" | "password" | "dropdown";
  placeholder?: string;
  required?: boolean;
  data?: { title: string; value: string }[];
}

export interface CommandManifest {
  name: string;
  title: string;
  subtitle?: string;
  description?: string;
  mode: "view" | "no-view" | "menu-bar";
  icon?: string;
  arguments?: Argument[];
  preferences?: Preference[];
}

export interface Manifest {
  name: string;
  title: string;
  description?: string;
  icon?: string;
  author?: string;
  owner?: string;
  commands: CommandManifest[];
  preferences?: Preference[];
}

export interface Extension {
  dir: string;
  manifest: Manifest;
}

export function readManifest(dir: string): Manifest {
  return JSON.parse(fs.readFileSync(path.join(dir, "package.json"), "utf8"));
}

export function installedExtensions(): Extension[] {
  let names: string[];
  try {
    names = fs.readdirSync(extensionsDir);
  } catch {
    return [];
  }
  return names.flatMap((name) => {
    const dir = path.join(extensionsDir, name);
    try {
      return [{ dir, manifest: readManifest(dir) }];
    } catch {
      return [];
    }
  });
}

export function supportPath(extension: string) {
  const dir = path.join(supportRoot, extension);
  fs.mkdirSync(dir, { recursive: true });
  return dir;
}

export function cachePath(extension: string) {
  const dir = path.join(cacheRoot, extension);
  fs.mkdirSync(dir, { recursive: true });
  return dir;
}

export function readJson<T>(file: string, fallback: T): T {
  try {
    return JSON.parse(fs.readFileSync(file, "utf8"));
  } catch {
    return fallback;
  }
}

export function writeJson(file: string, value: unknown) {
  fs.mkdirSync(path.dirname(file), { recursive: true });
  const tmp = `${file}.${process.pid}.tmp`;
  // Preferences, LocalStorage and the like can hold tokens: private to the user.
  fs.writeFileSync(tmp, JSON.stringify(value, null, 2), { mode: 0o600 });
  fs.renameSync(tmp, file);
}

// --- Preferences ---------------------------------------------------------

interface StoredPreferences {
  extension: Record<string, unknown>;
  commands: Record<string, Record<string, unknown>>;
}

const preferencesFile = (extension: string) => path.join(preferencesDir, `${extension}.json`);

export function storedPreferences(extension: string): StoredPreferences {
  const stored = readJson<Partial<StoredPreferences>>(preferencesFile(extension), {});
  return { extension: stored.extension ?? {}, commands: stored.commands ?? {} };
}

export function savePreferences(extension: string, prefs: StoredPreferences) {
  writeJson(preferencesFile(extension), prefs);
}

function defaultValue(pref: Preference) {
  if (pref.default !== undefined) return pref.default;
  if (pref.type === "checkbox") return false;
  if (pref.type === "dropdown") return pref.data?.[0]?.value;
  return undefined;
}

/// Extension and command preferences merged, with manifest defaults applied.
export function preferenceValues(ext: Extension, command?: string) {
  const stored = storedPreferences(ext.manifest.name);
  const cmd = ext.manifest.commands.find((c) => c.name === command);
  const values: Record<string, unknown> = {};
  for (const [defs, saved] of [
    [ext.manifest.preferences ?? [], stored.extension],
    [cmd?.preferences ?? [], (command && stored.commands[command]) || {}],
  ] as const) {
    for (const pref of defs) {
      const value = saved[pref.name] ?? defaultValue(pref);
      if (value !== undefined) values[pref.name] = value;
    }
  }
  return values;
}

export function missingPreferences(ext: Extension, command: string) {
  const values = preferenceValues(ext, command);
  const cmd = ext.manifest.commands.find((c) => c.name === command);
  return [...(ext.manifest.preferences ?? []), ...(cmd?.preferences ?? [])].filter(
    (p) => p.required && (values[p.name] === undefined || values[p.name] === ""),
  );
}

// --- The running command -------------------------------------------------
//
// Several commands can be alive at once: those kept in the menu bar, or one
// that finishes late in the palette while the next has started. So "the
// command that is running" is not one global but a context that follows each
// command's own work:
//
//  - whatever a command starts (timers, promises, child processes) carries
//    its context, through Node's AsyncLocalStorage;
//  - React renders the trees of several commands in one turn, so while a
//    component renders, the context is that of the tree it belongs to, read
//    from a React context put around each command (`scoped`);
//  - effects run after rendering, in a turn that may belong to another
//    command: the hooks handed to extensions (`scopedReact`) remember the
//    context their component rendered in.

export interface Current {
  extension: Extension;
  command: string;
  launchType: string;
  /// The user asked for what is running (a launch, a click on a menu item),
  /// so it may bring a command up in the palette.
  user?: boolean;
}

const storage = new AsyncLocalStorage<Current>();
const Scope = React.createContext<Current | null>(null);

/// The command in the palette. What runs there outside any command's context
/// (React's own turns for Heyo's built-in views) belongs to it.
export const foreground: { value: Current | null } = { value: null };

// React's own note of what it is doing: null while it renders nothing, which
// spares the attempt below. Without it (another React) the attempt decides.
const internals = (React as any).__CLIENT_INTERNALS_DO_NOT_USE_OR_WARN_USERS_THEY_CANNOT_UPGRADE as { H: unknown } | undefined;

/// The command whose component is rendering right now, if one is.
function rendering(): Current | null {
  if (internals && internals.H === null) return null;
  try {
    // Reading a context is allowed anywhere in a render, and only there.
    return React.use(Scope);
  } catch {
    return null;
  }
}

export const current = {
  get value(): Current | null {
    return rendering() ?? storage.getStore() ?? foreground.value;
  },
};

export function requireCurrent(): Current {
  const value = current.value;
  if (!value) throw new Error("No command is running");
  return value;
}

/// Runs `f` as the command: everything it starts keeps that context.
export function runAs<T>(context: Current | null, f: () => T): T {
  return context ? storage.run(context, f) : f();
}

/// `element` as the command's tree.
export function scoped(context: Current, element: React.ReactNode) {
  return React.createElement(Scope.Provider, { value: context }, element);
}

// What a command leaves behind at the host (a subscription, the captures it
// was given) is taken back when it ends. Whoever runs commands says when
// that is (`ended`): the palette's session when it closes, the background
// when a run is over or a menu bar command is unmounted.

const endings = new WeakMap<Current, Set<() => void>>();
const over = new WeakSet<Current>();

/// Has `undo` run when the command ends, or at once if it already has.
/// Returns how to take that back.
export function whenEnded(context: Current, undo: () => void): () => void {
  if (over.has(context)) {
    undo();
    return () => {};
  }
  let waiting = endings.get(context);
  if (!waiting) endings.set(context, (waiting = new Set()));
  waiting.add(undo);
  return () => void waiting.delete(undo);
}

/// The command is over: what it left to be undone is.
export function ended(context: Current) {
  over.add(context);
  const waiting = endings.get(context);
  endings.delete(context);
  waiting?.forEach((undo) => {
    try {
      runAs(context, undo);
    } catch (e) {
      console.error(e);
    }
  });
}

/// A command's component, rendering as the command: what its body starts
/// (a request made while rendering, not in an effect) is the command's too.
/// Components further down are only covered for what they read.
export function asCommand<P>(context: Current, Component: React.ComponentType<P>): React.ComponentType<P> {
  if (typeof Component !== "function" || Component.prototype?.isReactComponent) return Component;
  const render = Component as (props: P) => React.ReactNode;
  return (props: P) => runAs(context, () => render(props));
}

/// Whether the caller is the command in the palette, which alone may hide,
/// show or reset it. A background command never is, nor is a command that
/// finishes after the user moved on.
export function ownsPalette() {
  const value = current.value;
  return !!value && value === foreground.value;
}

type Effect = () => void | (() => void);

const effect = (hook: (create: Effect, deps?: React.DependencyList) => void) => (create: Effect, deps?: React.DependencyList) => {
  const context = current.value;
  hook(() => {
    const cleanup = runAs(context, create);
    return typeof cleanup === "function" ? () => runAs(context, cleanup) : cleanup;
  }, deps);
};

type Subscribe = (onChange: () => void) => () => void;
// React subscribes again whenever `subscribe` is another function, so each
// one keeps a single wrapper per command.
const subscriptions = new WeakMap<Subscribe, WeakMap<Current, Subscribe>>();

function useSyncExternalStore<T>(subscribe: Subscribe, getSnapshot: () => T, getServerSnapshot?: () => T) {
  const context = current.value;
  let wrapped = subscribe;
  if (context && typeof subscribe === "function") {
    let known = subscriptions.get(subscribe);
    if (!known) subscriptions.set(subscribe, (known = new WeakMap()));
    wrapped = known.get(context) ?? ((onChange) => runAs(context, () => subscribe(onChange)));
    known.set(context, wrapped);
  }
  return React.useSyncExternalStore(wrapped, getSnapshot, getServerSnapshot);
}

/// The `react` extensions get: React itself, with the hooks that call back
/// after rendering doing so as the command they belong to.
export const scopedReact: typeof React = {
  ...React,
  useEffect: effect(React.useEffect),
  useLayoutEffect: effect(React.useLayoutEffect),
  useInsertionEffect: effect(React.useInsertionEffect),
  useSyncExternalStore,
};
