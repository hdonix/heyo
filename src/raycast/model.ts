// The UI tree the extension runtime sends, plus helpers to read it.

import type { Toast, WebviewMessage, WireView } from "../../shared/wire";
import { call } from "../host";
import { t } from "../../shared/i18n";

export type { Toast, WireView };

export interface Node {
  id: number;
  type: string;
  props: Record<string, any>;
  children: Node[];
}

export interface View {
  key: number;
  depth: number;
  tree: Node[];
}

export function sendToRuntime(message: WebviewMessage) {
  return call("runtime_send", { message }).catch((e) => console.error("runtime_send", e));
}

/// Revives `{ $date }` values in props received from the runtime.
export function decode(value: any): any {
  if (value === null || typeof value !== "object") return value;
  if (Array.isArray(value)) return value.map(decode);
  if ("$date" in value) return new Date(value.$date);
  const out: Record<string, unknown> = {};
  for (const k in value) out[k] = decode(value[k]);
  return out;
}

/// Rebuilds the view tree from render messages. Unchanged subtrees keep
/// their object identity, so memoized rows skip re-rendering.
export class TreeStore {
  private key = -1;
  private records = new Map<number, { type: string; props: Record<string, any>; children: number[] }>();
  private parents = new Map<number, number>();
  private built = new Map<number, Node>();

  apply(view: WireView): View {
    if (view.full || view.key !== this.key) {
      this.records.clear();
      this.parents.clear();
      this.built.clear();
    }
    this.key = view.key;
    for (const id of view.removed) {
      this.records.delete(id);
      this.parents.delete(id);
      this.built.delete(id);
    }
    for (const node of view.nodes) {
      this.records.set(node.id, { type: node.type, props: decode(node.props), children: node.children });
      for (const child of node.children) this.parents.set(child, node.id);
    }
    // A changed node invalidates the built objects of its ancestors.
    for (const node of view.nodes) {
      for (let id: number | undefined = node.id; id !== undefined && this.built.has(id); id = this.parents.get(id)) {
        this.built.delete(id);
      }
    }
    return { key: view.key, depth: view.depth, tree: view.root.map((id) => this.build(id)) };
  }

  private build(id: number): Node {
    const cached = this.built.get(id);
    if (cached) return cached;
    const record = this.records.get(id);
    const node: Node = record
      ? { id, type: record.type, props: record.props, children: record.children.map((c) => this.build(c)) }
      : { id, type: "#missing", props: {}, children: [] };
    this.built.set(id, node);
    return node;
  }
}

/// Encodes callback arguments for the runtime.
export function encode(value: unknown): unknown {
  if (value instanceof Date) return { $date: value.toISOString() };
  if (Array.isArray(value)) return value.map(encode);
  if (value && typeof value === "object") {
    return Object.fromEntries(Object.entries(value).map(([k, v]) => [k, encode(v)]));
  }
  return value;
}

/// Whether a prop holds a callback (sent as `{ $fn: 1 }`).
export const isFn = (value: unknown) => typeof value === "object" && value !== null && "$fn" in value;

export function slot(node: Node | undefined, name: string): Node | undefined {
  return node?.children.find((c) => c.type === "slot" && c.props.name === name)?.children[0];
}

export const content = (node: Node) => node.children.filter((c) => c.type !== "slot");

/// Strings that may be given as `{ value, tooltip }`.
export function textOf(value: unknown): string {
  if (value == null) return "";
  if (typeof value === "object" && "value" in value) return textOf((value as { value: unknown }).value);
  return String(value);
}

export function tooltipOf(value: unknown): string | undefined {
  return typeof value === "object" && value !== null && "tooltip" in value
    ? String((value as { tooltip: unknown }).tooltip)
    : undefined;
}

/// Raycast's named colours, as the tokens that hold them for each scheme
/// (styles/tokens.css).
const RAYCAST_COLORS: Record<string, string> = {
  "raycast-blue": "var(--raycast-blue)",
  "raycast-green": "var(--raycast-green)",
  "raycast-magenta": "var(--raycast-magenta)",
  "raycast-orange": "var(--raycast-orange)",
  "raycast-purple": "var(--raycast-purple)",
  "raycast-red": "var(--raycast-red)",
  "raycast-yellow": "var(--raycast-yellow)",
  "raycast-brown": "var(--raycast-brown)",
  "raycast-primary-text": "var(--color-text)",
  "raycast-secondary-text": "var(--color-text-muted)",
};

/// CSS color for a Raycast `ColorLike` (named, raw, or `{ light, dark }`).
export function cssColor(color: unknown): string | undefined {
  if (typeof color === "string") return RAYCAST_COLORS[color] ?? color;
  if (color && typeof color === "object" && "dark" in color) {
    const dark = cssColor((color as { dark: unknown }).dark);
    const light = cssColor((color as { light?: unknown }).light) ?? dark;
    // Each in its scheme.
    return dark && light ? `light-dark(${light}, ${dark})` : dark;
  }
  return undefined;
}

/// The units a time ago or to come is counted in, the longest first: its
/// seconds, and how that many of it are said, past and future.
const SPANS: [number, (n: number) => string, (n: number) => string][] = [
  [31_536_000, (n) => t("{0}y", n), (n) => t("in {0}y", n)],
  [2_592_000, (n) => t("{0}mo", n), (n) => t("in {0}mo", n)],
  [604_800, (n) => t("{0}w", n), (n) => t("in {0}w", n)],
  [86_400, (n) => t("{0}d", n), (n) => t("in {0}d", n)],
  [3_600, (n) => t("{0}h", n), (n) => t("in {0}h", n)],
  [60, (n) => t("{0}m", n), (n) => t("in {0}m", n)],
];

/// Raycast-style compact relative time ("now", "5m", "3h", "2d").
export function relativeTime(date: Date) {
  const seconds = (Date.now() - date.getTime()) / 1000;
  const abs = Math.abs(seconds);
  for (const [size, past, future] of SPANS) {
    if (abs >= size) return (seconds < 0 ? future : past)(Math.floor(abs / size));
  }
  return t("now");
}

/// Raycast's default list filtering: every query word must occur in the
/// title or a keyword.
export function matchesQuery(query: string, title: string, keywords: unknown[] = []) {
  const words = query.toLowerCase().split(/\s+/).filter(Boolean);
  if (!words.length) return true;
  const haystack = [title, ...keywords.map(String)].map((s) => s.toLowerCase());
  return words.every((w) => haystack.some((h) => h.includes(w)));
}
