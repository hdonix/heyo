//! Non-UI parts of `@raycast/api`. What touches the desktop is asked of the
//! host, which knows the session it runs in.

import fs from "node:fs";
import path from "node:path";

import { cachePath, current, ownsPalette, readJson, requireCurrent, runAs, supportPath, writeJson, type Current } from "../context";
import type { Application, Toast as WireToast } from "../../../shared/wire";
import { on, request, send } from "../protocol";

// --- Window --------------------------------------------------------------
//
// The palette is moved only by the command in it: a menu bar or background
// command, or one that finishes after the user went on to something else,
// leaves it as the user has it.

export const PopToRootType = { Default: "default", Immediate: "immediate", Suspended: "suspended" } as const;

export async function closeMainWindow(_options?: { clearRootSearch?: boolean; popToRootType?: string }) {
  if (ownsPalette()) await request("host.hide");
}

export async function popToRoot(_options?: { clearSearchBar?: boolean }) {
  if (ownsPalette()) send({ type: "pop-to-root" });
}

export async function clearSearchBar(_options?: { forceScrollToTop?: boolean }) {
  if (ownsPalette()) send({ type: "clear-search-bar" });
}

export async function showHUD(title: string, _options?: { clearRootSearch?: boolean; popToRootType?: string }) {
  await closeMainWindow();
  // The window is gone (or was never this command's), so the HUD is a
  // short-lived desktop notification.
  await request("host.hud", { text: title }).catch(() => {});
}

// --- Toasts --------------------------------------------------------------

interface ToastActionOptions {
  title: string;
  shortcut?: unknown;
  onAction: (toast: Toast) => void;
}

interface ToastOptions {
  title: string;
  message?: string;
  style?: string;
  primaryAction?: ToastActionOptions;
  secondaryAction?: ToastActionOptions;
}

/// The toasts on screen, each with the command that showed it.
const toasts = new Map<number, { toast: Toast; context: Current | null }>();
let nextToast = 0;

on("toast-action", ({ id, action }) => {
  const shown = toasts.get(id);
  if (!shown) return;
  const { toast, context } = shown;
  const target = action === "secondary" ? toast.secondaryAction : toast.primaryAction;
  if (target) runAs(context, () => target.onAction(toast));
});

export class Toast {
  static Style = { Success: "SUCCESS", Failure: "FAILURE", Animated: "ANIMATED" } as const;

  readonly id = ++nextToast;
  #options: ToastOptions;
  #visible = false;

  constructor(options: ToastOptions) {
    this.#options = { style: Toast.Style.Success, ...options };
  }

  get style() {
    return this.#options.style!;
  }
  set style(style) {
    this.#update({ style });
  }
  get title() {
    return this.#options.title;
  }
  set title(title) {
    this.#update({ title });
  }
  get message() {
    return this.#options.message;
  }
  set message(message) {
    this.#update({ message });
  }
  get primaryAction() {
    return this.#options.primaryAction;
  }
  set primaryAction(primaryAction) {
    this.#update({ primaryAction });
  }
  get secondaryAction() {
    return this.#options.secondaryAction;
  }
  set secondaryAction(secondaryAction) {
    this.#update({ secondaryAction });
  }

  #update(changes: Partial<ToastOptions>) {
    Object.assign(this.#options, changes);
    if (this.#visible) this.#send();
  }

  #send() {
    const { title, message, style, primaryAction, secondaryAction } = this.#options;
    send({
      type: "toast",
      toast: {
        id: this.id,
        title,
        message,
        style: style as WireToast["style"],
        primaryAction: primaryAction && { title: primaryAction.title },
        secondaryAction: secondaryAction && { title: secondaryAction.title },
      },
    });
  }

  async show() {
    this.#visible = true;
    toasts.set(this.id, { toast: this, context: current.value });
    this.#send();
  }

  async hide() {
    this.#visible = false;
    toasts.delete(this.id);
    send({ type: "toast-hide", id: this.id });
  }
}

export async function showToast(options: ToastOptions | string, title?: string, message?: string) {
  const toast = new Toast(typeof options === "string" ? { style: options, title: title ?? "", message } : options);
  await toast.show();
  return toast;
}

// --- Alerts --------------------------------------------------------------

export const Alert = {
  ActionStyle: { Default: "DEFAULT", Cancel: "CANCEL", Destructive: "DESTRUCTIVE" },
} as const;

interface AlertOptions {
  title: string;
  message?: string;
  icon?: unknown;
  primaryAction?: { title: string; style?: string; onAction?: () => void };
  dismissAction?: { title: string; style?: string; onAction?: () => void };
  rememberUserChoice?: boolean;
}

export async function confirmAlert(options: AlertOptions): Promise<boolean> {
  const { primaryAction, dismissAction } = options;
  const confirmed = await request<boolean>("ui.confirm", {
    title: options.title,
    message: options.message,
    icon: options.icon,
    primaryAction: { title: primaryAction?.title ?? "OK", style: primaryAction?.style },
    dismissAction: { title: dismissAction?.title ?? "Cancel" },
  });
  (confirmed ? primaryAction : dismissAction)?.onAction?.();
  return confirmed;
}

// --- Clipboard -----------------------------------------------------------

export type ClipboardContent = string | number | { text?: string; file?: string; html?: string };

export function clipboardPayload(content: ClipboardContent) {
  if (typeof content !== "object") return { text: String(content) };
  if (content.file) return { text: content.file };
  return { text: content.text ?? "", html: content.html };
}

/// Sends a paste keystroke to whatever window has focus after Heyo hides.
/// The host does it: it knows the session and whether the window is a
/// terminal (where paste is Ctrl+Shift+V), so extensions paste the same way
/// Heyo's own commands do.
export async function pasteKeystroke() {
  await request("host.paste").catch(() => {});
}

/// Text about to be typed into another application: without control
/// characters, which a terminal would act on.
export function pasteable(text: string) {
  // Keep tab and newline; drop the other C0 controls, DEL and the C1 range.
  return text.replace(/[\u0000-\u0008\u000b-\u001f\u007f-\u009f]/g, "");
}

export const Clipboard = {
  async copy(content: ClipboardContent, options?: { concealed?: boolean; transient?: boolean }) {
    // Concealed (passwords) and transient copies stay out of the clipboard history.
    const concealed = !!(options?.concealed || options?.transient);
    await request("host.clipboard.write", { ...clipboardPayload(content), concealed });
  },
  async paste(content: ClipboardContent) {
    await Clipboard.copy(content);
    await closeMainWindow();
    await pasteKeystroke();
  },
  async clear() {
    await request("host.clipboard.write", { text: "" });
  },
  async read(_options?: { offset?: number }) {
    const text = await request<string>("host.clipboard.read");
    return { text };
  },
  async readText(_options?: { offset?: number }) {
    return (await Clipboard.read()).text;
  },
};

// --- Opening things ------------------------------------------------------

export async function getApplications(_path?: string): Promise<Application[]> {
  return request<Application[]>("host.applications");
}

export async function getDefaultApplication(target: string): Promise<Application> {
  const id = await request<string | null>("host.defaultApplication", { target });
  const app = id ? (await getApplications()).find((a) => a.bundleId === id) : undefined;
  if (!app) throw new Error(`No default application for ${target}`);
  return app;
}

export async function open(target: string, application?: Application | string) {
  // The host knows how this desktop opens things.
  await request("host.open", { target, application: typeof application === "string" ? application : application?.path });
}

export async function showInFinder(target: string) {
  await open(path.dirname(target));
}

export async function trash(paths: string | string[]) {
  await request("host.trash", { paths: Array.isArray(paths) ? paths : [paths] });
}

export async function getSelectedText(): Promise<string> {
  const text = await request<string>("host.selection").catch(() => "");
  if (!text) throw new Error("Unable to get selected text");
  return text;
}

export async function getSelectedFinderItems(): Promise<{ path: string }[]> {
  return [];
}

// --- Storage -------------------------------------------------------------

const localStorageFile = () => path.join(supportPath(requireCurrent().extension.manifest.name), "local-storage.json");
type Stored = string | number | boolean;

export const LocalStorage = {
  async getItem<T extends Stored>(key: string): Promise<T | undefined> {
    return readJson<Record<string, T>>(localStorageFile(), {})[key];
  },
  async setItem(key: string, value: Stored) {
    const file = localStorageFile();
    writeJson(file, { ...readJson(file, {}), [key]: value });
  },
  async removeItem(key: string) {
    const file = localStorageFile();
    const items = readJson<Record<string, Stored>>(file, {});
    delete items[key];
    writeJson(file, items);
  },
  async allItems() {
    return readJson<Record<string, Stored>>(localStorageFile(), {});
  },
  async clear() {
    writeJson(localStorageFile(), {});
  },
};

type CacheSubscriber = (key: string | undefined, data: string | undefined) => void;

/// One namespace's data, shared by every `Cache` of it in this process.
/// (Extensions make many: `useCachedState` one per call. Each with a copy of
/// its own wrote the whole file from that copy, and lost the others' writes.)
interface CacheStore {
  file: string;
  data: Record<string, string>;
  /// What the file looked like when `data` was read or written.
  stamp: string;
  subscribers: Set<CacheSubscriber>;
}

// Weakly: a namespace nobody holds a `Cache` of any more gives its data back.
const cacheStores = new Map<string, WeakRef<CacheStore>>();

function cacheStore(file: string): CacheStore {
  let store = cacheStores.get(file)?.deref();
  if (!store) {
    store = { file, data: {}, stamp: "unread", subscribers: new Set() };
    cacheStores.set(file, new WeakRef(store));
  }
  return store;
}

function fileStamp(file: string) {
  try {
    const stat = fs.statSync(file);
    // Every write is a new file renamed into place.
    return `${stat.ino}:${stat.mtimeMs}:${stat.size}`;
  } catch {
    return "";
  }
}

/// The store's data, read again if the file changed under it: the other
/// runtime process (palette or background) may run a command of the same
/// extension and write the same namespace.
function cacheData(store: CacheStore) {
  const stamp = fileStamp(store.file);
  if (stamp !== store.stamp) {
    store.data = readJson(store.file, {});
    store.stamp = stamp;
  }
  return store.data;
}

/// Synchronous key-value cache, persisted as one JSON file per namespace.
/// Methods are bound: `@raycast/utils` passes `cache.subscribe` around unbound.
export class Cache {
  #store: CacheStore;

  constructor(options?: { namespace?: string; capacity?: number }) {
    const dir = cachePath(requireCurrent().extension.manifest.name);
    const name = (options?.namespace ?? "default").replace(/[^\w.-]/g, "_");
    this.#store = cacheStore(path.join(dir, `${name}.json`));
  }

  get storageDirectory() {
    return path.dirname(this.#store.file);
  }
  get isEmpty() {
    return Object.keys(cacheData(this.#store)).length === 0;
  }
  get = (key: string): string | undefined => cacheData(this.#store)[key];
  has = (key: string) => key in cacheData(this.#store);
  set = (key: string, data: string) => {
    cacheData(this.#store)[key] = data;
    this.#persist(key, data);
  };
  remove = (key: string) => {
    const all = cacheData(this.#store);
    if (!(key in all)) return false;
    delete all[key];
    this.#persist(key, undefined);
    return true;
  };
  clear = (options?: { notifySubscribers?: boolean }) => {
    this.#store.data = {};
    this.#write();
    if (options?.notifySubscribers ?? true) this.#store.subscribers.forEach((s) => s(undefined, undefined));
  };
  /// Hears of changes made through any `Cache` of the namespace in this process.
  subscribe = (subscriber: CacheSubscriber) => {
    const { subscribers } = this.#store;
    subscribers.add(subscriber);
    return () => {
      subscribers.delete(subscriber);
    };
  };

  #write() {
    const store = this.#store;
    try {
      writeJson(store.file, store.data);
    } catch (e) {
      console.error("heyo: failed to write cache", e);
    }
    store.stamp = fileStamp(store.file);
  }

  #persist(key: string, data: string | undefined) {
    this.#write();
    this.#store.subscribers.forEach((s) => s(key, data));
  }
}

export function unsupported(name: string) {
  return () => {
    throw new Error(`${name} is not supported by Heyo`);
  };
}
