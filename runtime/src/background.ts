//! Menu bar and background commands, run in a runtime process of their own
//! so they never disturb the command open in the palette.
//!
//!   in:  {type:"background-run", id, extension, command, launchType, action?}
//!   out: {type:"menu", key, menu}   {type:"background-done", id, error?}
//!
//! A run of a menu bar command mounts it, waits until it has settled and
//! reports its menu. The command then STAYS mounted: when its own state
//! changes it renders again and the new menu is sent, and a click on a menu
//! item (`action`, the item's path: child indices from the menu's root)
//! calls the `onAction` that is already there instead of loading and running
//! the command again, which took as long as the command's network requests.
//!
//! What staying mounted costs is a command's module, its React tree and
//! whatever timers it keeps, so it is bounded:
//!
//!  - at most `limits.mounted` commands stay mounted; one more unmounts the
//!    one used (run or clicked) longest ago;
//!  - a command neither run nor clicked for `limits.idle` is unmounted.
//!
//! A click on a command that is not mounted takes the old way: the command
//! is run, and the item at the path is called once the menu has settled.
//! A run without an action (the schedule, "Refresh", the user opening the
//! command) always starts the command afresh, as Raycast does.
//!
//! Each command has its own context (`context.ts`), so different commands
//! run side by side; only the runs of one command wait for each other.

import fs from "node:fs";
import path from "node:path";
import { createElement as h, type ComponentType } from "react";

import { asCommand, ended, missingPreferences, readManifest, runAs, scoped, type Current, type Extension } from "./context";
import type { HostMessage } from "../../shared/wire";
import { on, send } from "./protocol";
import { mount, visibleChildren, type Container, type HostNode } from "./renderer";
import { loadCommand } from "./session";

export const limits = {
  /// How long a run (or a clicked item's action) may take before it is no
  /// longer waited for. Nothing can stop a command that never settles; this
  /// keeps it from holding up its own later runs for ever.
  timeout: 60_000,
  /// A menu still loading after this long is shown as it is.
  loading: 30_000,
  /// How many menu bar commands stay mounted.
  mounted: 6,
  /// A mounted command not used for this long is unmounted.
  idle: 30 * 60_000,
  /// A mounted command that keeps rendering changes its menu this often at most.
  menuGap: 1_000,
};

/// Commits that follow each other this closely are one change...
const SETTLE = 60;
/// ...but a command that never stops rendering is looked at this long after
/// the first of them.
const SETTLE_LIMIT = 300;

interface MenuNode {
  kind: "item" | "submenu" | "section" | "separator";
  title?: string;
  subtitle?: string;
  /// Whether clicking does something.
  enabled?: boolean;
  children?: MenuNode[];
}

export type Run = Extract<HostMessage, { type: "background-run" }>;

/// A menu bar command that is mounted.
interface Mounted {
  key: string;
  extension: Extension;
  context: Current;
  container: Container;
  unmount(): void;
  /// The nodes of the menu as it was last sent, by path: what a click means,
  /// even if the command has rendered something else since.
  items: Map<string, HostNode>;
  /// The menu last sent, to send only changes.
  sent?: string;
  sentAt: number;
  /// When it was last run or clicked.
  used: number;
  loadingSince?: number;
  /// When the first commit not yet looked at happened.
  changedAt?: number;
  settle?: NodeJS.Timeout;
  idle?: NodeJS.Timeout;
  /// The run waiting for the first menu.
  first?: { resolve(): void; reject(error: unknown): void };
}

const mounted = new Map<string, Mounted>();

const text = (value: unknown) => (typeof value === "string" || typeof value === "number" ? String(value) : undefined);

/// An icon as a file the tray can show, if it is one.
function iconFile(icon: unknown, extension: Extension): string | undefined {
  const source = typeof icon === "string" ? icon : icon && typeof icon === "object" ? (icon as any).source : undefined;
  const name = typeof source === "string" ? source : source && typeof source === "object" ? (source.dark ?? source.light) : undefined;
  if (typeof name !== "string" || !/\.(png|jpe?g)$/i.test(name)) return undefined;
  const file = path.isAbsolute(name) ? name : path.join(extension.dir, "assets", name);
  return fs.existsSync(file) ? file : undefined;
}

/// The menu a node stands for; `items` collects the nodes by their paths,
/// which index the same lists the host builds its menu from.
function menuOf(node: HostNode, at: number[], items: Map<string, HostNode>): MenuNode | undefined {
  const children = () => menusOf(node, at, items);
  switch (node.type) {
    case "MenuBarExtra.Item":
      items.set(at.join("."), node);
      return { kind: "item", title: text(node.props.title) ?? "", subtitle: text(node.props.subtitle), enabled: typeof node.props.onAction === "function" };
    case "MenuBarExtra.Submenu":
      return { kind: "submenu", title: text(node.props.title) ?? "", children: children() };
    case "MenuBarExtra.Section":
      return { kind: "section", title: text(node.props.title), children: children() };
    case "MenuBarExtra.Separator":
      return { kind: "separator" };
    default:
      return undefined;
  }
}

function menusOf(parent: HostNode, at: number[], items: Map<string, HostNode>): MenuNode[] {
  const menus: MenuNode[] = [];
  for (const child of visibleChildren(parent)) {
    const menu = menuOf(child, [...at, menus.length], items);
    if (menu) menus.push(menu);
  }
  return menus;
}

/// Rejects when `work` takes longer than the limit; the work itself goes on.
function limited<T>(work: Promise<T> | T, what: string): Promise<T> {
  return new Promise<T>((resolve, reject) => {
    const timer = setTimeout(() => reject(new Error(`${what} was still running after ${Math.round(limits.timeout / 1000)} s and is no longer waited for`)), limits.timeout);
    Promise.resolve(work)
      .then(resolve, reject)
      .finally(() => clearTimeout(timer));
  });
}

function drop(m: Mounted) {
  if (mounted.get(m.key) === m) mounted.delete(m.key);
  clearTimeout(m.settle);
  clearTimeout(m.idle);
  // Unmounting inside a commit is not allowed.
  setImmediate(() => {
    m.unmount();
    ended(m.context);
  });
}

/// Marks a command as used, and makes room: the idle clock starts again and
/// the commands over the limit, least recently used first, are unmounted.
function touch(m: Mounted) {
  m.used = Date.now();
  clearTimeout(m.idle);
  m.idle = setTimeout(() => drop(m), limits.idle);
  const settled = [...mounted.values()].filter((other) => !other.first).sort((a, b) => a.used - b.used);
  for (const other of settled.slice(0, Math.max(0, settled.length - limits.mounted))) {
    if (other !== m) drop(other);
  }
}

/// Sends the command's menu if it has one to show and it changed.
function publish(m: Mounted) {
  m.changedAt = undefined;
  if (mounted.get(m.key) !== m) return;
  const root = m.container.children.find((c) => c.type === "MenuBarExtra");
  // Rendering nothing removes the command from the menu bar.
  if (!root) {
    send({ type: "menu", key: m.key, menu: null });
    m.first?.resolve();
    return drop(m);
  }
  if (root.props.isLoading) {
    m.loadingSince ??= Date.now();
    const left = m.loadingSince + limits.loading - Date.now();
    if (left > 0) {
      // No commit may follow to look again.
      m.settle = setTimeout(() => publish(m), left);
      return;
    }
  }
  m.loadingSince = undefined;
  const items = new Map<string, HostNode>();
  const menu = {
    title: text(root.props.title),
    tooltip: text(root.props.tooltip),
    icon: iconFile(root.props.icon, m.extension),
    items: menusOf(root, [], items),
  };
  m.items = items;
  const json = JSON.stringify(menu);
  if (json !== m.sent) {
    m.sent = json;
    m.sentAt = Date.now();
    send({ type: "menu", key: m.key, menu });
  }
  m.first?.resolve();
}

/// Loads and mounts a menu bar command; resolves once its first menu is sent.
function mountCommand(key: string, extension: Extension, context: Current, launchProps: object): Promise<void> {
  // The menu the old one sent stays in the menu bar until the new one's is ready.
  const old = mounted.get(key);
  if (old) drop(old);

  const waiting = new Promise<void>((resolve, reject) => {
    const m: Mounted = {
      key,
      extension,
      context,
      items: new Map(),
      sentAt: 0,
      used: Date.now(),
      unmount: () => {},
      first: { resolve, reject },
      container: {
        children: [],
        onCommit() {
          clearTimeout(m.settle);
          const now = Date.now();
          m.changedAt ??= now;
          const settled = Math.min(now + SETTLE, m.changedAt + SETTLE_LIMIT);
          // The first menu and the answer to a click come at once; a command
          // that renders all the time changes its menu at a slower pace.
          const gap = m.sent === undefined ? 0 : m.sentAt + limits.menuGap;
          m.settle = setTimeout(() => publish(m), Math.max(settled, gap) - now);
        },
      },
    };
    const failed = (error: unknown) => {
      if (m.first) return m.first.reject(error);
      // Later, nobody waits: the next click or run starts it again.
      console.error(`heyo: ${key}:`, error);
      drop(m);
    };
    mounted.set(key, m);
    try {
      runAs(context, () => {
        const entry = asCommand(context, loadCommand(extension, context.command) as ComponentType<object>);
        m.unmount = mount(scoped(context, h(entry, launchProps)), m.container, failed);
      });
    } catch (e) {
      reject(e);
    }
  });

  const m = mounted.get(key)!;
  return limited(waiting, "Rendering the menu").then(
    () => {
      m.first = undefined;
      if (mounted.get(key) === m) touch(m);
    },
    (error) => {
      m.first = undefined;
      drop(m);
      throw error;
    },
  );
}

/// Calls the clicked item of a mounted command.
async function click(m: Mounted, action: number[]) {
  touch(m);
  // What the click changes shows at once, however recently the menu changed.
  m.sentAt = 0;
  const onAction = m.items.get(action.join("."))?.props.onAction;
  if (typeof onAction !== "function") return;
  // The user asked for this, whatever started the command.
  const context = { ...m.context, user: true };
  try {
    await limited(
      runAs(context, () => onAction({ type: "left-click" })),
      "The menu item's action",
    );
  } finally {
    ended(context);
  }
}

async function runCommand(run: Run): Promise<void> {
  const key = `${run.extension}:${run.command}`;
  const extension: Extension = { dir: run.extension, manifest: readManifest(run.extension) };
  const command = extension.manifest.commands.find((c) => c.name === run.command);
  if (!command) throw new Error(`Unknown command ${key}`);

  if (run.action) {
    const m = mounted.get(key);
    if (m) return click(m, run.action);
  }

  // Nothing can ask for them here; the command works once they are set.
  if (missingPreferences(extension, run.command).length) {
    throw new Error(`${command.title} needs its preferences set first (Heyo Settings)`);
  }
  const user = run.launchType === "userInitiated";
  const context: Current = { extension, command: run.command, launchType: run.launchType, user };
  const launchProps = { arguments: run.arguments ?? {}, launchType: run.launchType, launchContext: run.launchContext };

  if (command.mode !== "menu-bar") {
    const entry = () => (loadCommand(extension, run.command) as (props: object) => unknown)(launchProps);
    try {
      await limited(
        runAs(context, async () => entry()),
        command.title,
      );
    } finally {
      ended(context);
    }
    return;
  }

  await mountCommand(key, extension, context, launchProps);
  const m = mounted.get(key);
  if (run.action && m) await click(m, run.action);
}

// The runs of one command follow each other: they share its storage and its
// place in the menu bar. Different commands do not wait for each other.
const queues = new Map<string, Promise<void>>();

/// Runs a command here; resolves when the run is over, with its error if it failed.
export function enqueue(run: Run): Promise<string | undefined> {
  const key = `${run.extension}:${run.command}`;
  const result = (queues.get(key) ?? Promise.resolve()).then(async () => {
    try {
      await runCommand(run);
      return undefined;
    } catch (e) {
      console.error(`heyo: ${key}:`, e);
      return e instanceof Error ? e.message : String(e);
    }
  });
  const tail = result.then(() => {
    if (queues.get(key) === tail) queues.delete(key);
  });
  queues.set(key, tail);
  return result;
}

let started = false;

/// Whether this is the process for menu bar and background commands.
export function isBackground() {
  return started;
}

export function startBackground() {
  started = true;
  on("background-run", (run) => {
    enqueue(run).then((error) => send({ type: "background-done", id: run.id, error }));
  });
}

/// For tests: the commands that are mounted, and a clean slate.
export function mountedKeys() {
  return [...mounted.keys()];
}

export function unmountAll() {
  [...mounted.values()].forEach(drop);
}
