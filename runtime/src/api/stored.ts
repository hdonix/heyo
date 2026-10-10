//! What `storeValue` keeps: the last value of a form field or a search bar
//! dropdown, to start from the next time the command shows it.
//!
//! One file per extension, in its support directory beside its
//! LocalStorage, holding for each command the values by field. Like the
//! preferences it is private to the user, since a field may hold a secret
//! (Raycast keeps a `Form.PasswordField` with `storeValue` like any other).

import path from "node:path";

import { readJson, requireCurrent, supportPath, writeJson } from "../context";
import { decode } from "../renderer";

type Stored = Record<string, Record<string, unknown>>;

/// Dates survive the file as `{ $date }`, the way they cross to the webview.
const encode = (value: unknown): unknown =>
  value instanceof Date ? { $date: value.toISOString() } : Array.isArray(value) ? value.map(encode) : value;

/// The value kept for `key` of `command`; undefined when there is none.
export function readStored(file: string, command: string, key: string): unknown {
  const value = readJson<Stored>(file, {})[command]?.[key];
  return value === undefined ? undefined : decode(value);
}

/// Keeps `values` for `command`, beside what is already kept.
export function writeStored(file: string, command: string, values: Record<string, unknown>) {
  const kept: Record<string, unknown> = {};
  for (const [key, value] of Object.entries(values)) {
    if (value !== undefined) kept[key] = encode(value);
  }
  if (!Object.keys(kept).length) return;
  const all = readJson<Stored>(file, {});
  writeJson(file, { ...all, [command]: { ...all[command], ...kept } });
}

function place() {
  const { extension, command } = requireCurrent();
  return { file: path.join(supportPath(extension.manifest.name), "stored-values.json"), command };
}

/// A form field and a dropdown may share an id.
export const fieldKey = (id: string) => `field:${id}`;
export const dropdownKey = (id: string | undefined) => `dropdown:${id ?? ""}`;

/// For the running command.
export function stored(key: string): unknown {
  const { file, command } = place();
  return readStored(file, command, key);
}

export function store(values: Record<string, unknown>) {
  const { file, command } = place();
  writeStored(file, command, values);
}

// The fields on screen that asked to be kept, by id. `Action.SubmitForm`
// receives every value of the form; these are the ones to keep.
const kept = new Map<string, number>();

/// Registers a field with `storeValue`; returns how to take it back.
export function keep(id: string) {
  kept.set(id, (kept.get(id) ?? 0) + 1);
  return () => {
    const count = (kept.get(id) ?? 1) - 1;
    if (count > 0) kept.set(id, count);
    else kept.delete(id);
  };
}

/// A form was submitted with `values`.
export function storeSubmitted(values: unknown) {
  if (!values || typeof values !== "object" || !kept.size) return;
  const out: Record<string, unknown> = {};
  for (const [id, value] of Object.entries(values)) {
    if (kept.has(id)) out[fieldKey(id)] = value;
  }
  store(out);
}
