//! `LiveActivity`: something in progress, shown by Heyo while the command
//! is away (docs/live-activities.md, section 6; permission `activity`).
//!
//! The host keeps the activity and counts its timers; this is the thin
//! side. Dates go over the wire as milliseconds. Every update carries a
//! sequence number (6.2), so that of two updates from different places
//! (the command that started the activity, the handler of a button) the
//! older cannot overwrite the newer: the number is the time, pushed on by
//! one where two fall in the same millisecond, which orders them across
//! the two runtime processes too.

import type { ActivityAttributes } from "../../../shared/bindings/ActivityAttributes";
import type { ActivityContent } from "../../../shared/bindings/ActivityContent";
import type { ActivityTimer } from "../../../shared/bindings/ActivityTimer";
import type { Presentation } from "../../../shared/bindings/Presentation";
import type { Support } from "../../../shared/bindings/Support";
import { current, runAs, type Current } from "../context";
import { ask } from "../protocol";
import { subscribe } from "./events";

export interface Timer {
  start: Date;
  end?: Date;
  pausedAt?: Date;
  countsDown?: boolean;
  style?: "clock" | "relative";
}

export interface Progress {
  value?: number;
  interval?: Pick<Timer, "start" | "end" | "pausedAt">;
}

export type Value = string | { timer: Timer } | { progress: Progress } | { icon: unknown };

export type Action =
  | { open: string }
  | { copy: string }
  | { launch: { command: string; arguments?: Record<string, string>; context?: unknown } }
  | { event: string; data?: unknown }
  | { end: true };

export interface Button {
  title: string;
  icon?: unknown;
  style?: "regular" | "destructive";
  action: Action;
}

export interface Attributes {
  id: string;
  name: string;
  icon: unknown;
  tint?: unknown;
  action?: Action;
  dismissible?: boolean;
  fallback?: "notify" | "drop";
}

export interface Content {
  compact?: Value;
  title: string;
  subtitle?: string;
  body?: { progress: Progress } | { timer: Timer } | { text: string };
  actions?: Button[];
  needsAttention?: boolean;
  staleAt?: Date;
  relevance?: number;
}

export interface StartOptions {
  announce?: boolean;
}

export interface Alert {
  expand?: boolean;
}

export interface Handle {
  readonly id: string;
  update(content: Partial<Content>, options?: { alert?: Alert }): Promise<void>;
  end(final?: Partial<Content>, options?: { keep?: "none" | "brief" }): Promise<void>;
}

export type Event =
  | { kind: "action"; id: string; event: string; data?: unknown }
  | { kind: "dismissed"; id: string }
  | { kind: "expired"; id: string }
  | { kind: "elapsed"; id: string; at: number };

export type { Presentation };

let last = 0;

/// The next sequence number: the time, and never one already given.
export function sequence(now = Date.now()): number {
  last = Math.max(now, last + 1);
  return last;
}

const millis = (date: Date | number | undefined) => (date === undefined ? undefined : typeof date === "number" ? date : date.getTime());

/// Strips what is undefined, so the wire carries what was given and the
/// host's patch leaves the rest alone.
function defined<T extends object>(object: T): T {
  return Object.fromEntries(Object.entries(object).filter(([, value]) => value !== undefined)) as T;
}

function wireTimer(timer: Pick<Timer, "start" | "end" | "pausedAt"> & Partial<Timer>): ActivityTimer {
  return defined({ start: millis(timer.start)!, end: millis(timer.end), pausedAt: millis(timer.pausedAt), countsDown: timer.countsDown, style: timer.style });
}

function wireProgress(progress: Progress) {
  return defined({ value: progress.value, interval: progress.interval && wireTimer(progress.interval) });
}

function wireValue(value: Value): ActivityContent["compact"] {
  if (typeof value === "string") return value;
  if ("timer" in value) return { timer: wireTimer(value.timer) };
  if ("progress" in value) return { progress: wireProgress(value.progress) };
  return value;
}

function wireBody(body: NonNullable<Content["body"]>): ActivityContent["body"] {
  if ("timer" in body) return { timer: wireTimer(body.timer) };
  if ("progress" in body) return { progress: wireProgress(body.progress) };
  return body;
}

/// The content as the host reads it: dates as milliseconds, nothing undefined.
export function wireContent(content: Partial<Content>): ActivityContent {
  return defined({
    compact: content.compact === undefined ? undefined : wireValue(content.compact),
    title: content.title,
    subtitle: content.subtitle,
    body: content.body === undefined ? undefined : wireBody(content.body),
    actions: content.actions?.map((button) => defined({ title: button.title, icon: button.icon, style: button.style, action: button.action })),
    needsAttention: content.needsAttention,
    staleAt: millis(content.staleAt),
    relevance: content.relevance,
  }) as ActivityContent;
}

/// A handle acts as the command that made it, wherever it is used from:
/// the activity is that command's extension's.
function handle(id: string, command: Current | null): Handle {
  return {
    id,
    async update(content, options) {
      await runAs(command, () => ask("ext.activity.update", { id, content: wireContent(content), alert: options?.alert, seq: sequence() }));
    },
    async end(final, options) {
      await runAs(command, () => ask("ext.activity.end", { id, final: final && wireContent(final), keep: options?.keep }));
    },
  };
}

export const LiveActivity = {
  supported: () => ask<Support>("ext.supported", { module: "activity" }),

  /// Starts an activity, or updates the one of that id. The host is told
  /// how the command was launched: only one the user ran may start one.
  async start(attributes: Attributes, content: Content, options?: StartOptions): Promise<Handle> {
    const command = current.value;
    const launchType = command?.launchType === "background" ? "background" : "userInitiated";
    const params = {
      attributes: defined(attributes) as ActivityAttributes,
      content: wireContent(content),
      announce: options?.announce,
      launchType: launchType as "userInitiated" | "background",
      seq: sequence(),
    };
    const { id } = await ask<{ id: string }>("ext.activity.start", params);
    return handle(id, command);
  },

  async list(): Promise<Handle[]> {
    const command = current.value;
    const ids = await ask<string[]>("ext.activity.list");
    return ids.map((id) => handle(id, command));
  },

  async get(id: string): Promise<Handle | undefined> {
    const command = current.value;
    return (await ask<boolean>("ext.activity.get", { id })) ? handle(id, command) : undefined;
  },

  presentation: () => ask<Presentation>("ext.activity.presentation"),

  /// Hears the events of the extension's activities for as long as the
  /// command runs. Returns how to stop.
  async onEvent(listener: (event: Event) => void): Promise<() => void> {
    const subscribed = await subscribe(["ext.activity.onEvent"], (data) => listener(data as Event));
    return () => void subscribed.cancel();
  },
};
