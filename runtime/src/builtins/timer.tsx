//! The built-in "Timer" command: a countdown typed in the search bar
//! ("25m", "1h 10m", "90s"), kept as a live activity that the host counts
//! down by itself. The proof of the whole path (docs/live-activities.md,
//! section 8): the command ends as soon as the activity has started; the
//! buttons come back as events, for which the host runs this same command
//! in the background with the event as its launch context; and when the
//! countdown runs out the host says so (`elapsed`), and the activity ends
//! with a notification.
//!
//! The activity holds all the state: a button's `data` carries the span
//! it was shown with, so the handler needs nothing remembered here.

import { closeMainWindow, showToast, Toast } from "../api/system";
import { LiveActivity, type Button, type Content, type Event } from "../heyo/activity";
import { locale, t } from "../../../shared/i18n";

/// Where a timer stands: its span, and where it was paused.
export interface Span {
  start: number;
  end: number;
  pausedAt?: number;
}

const UNITS: Record<string, number> = { h: 3_600_000, hr: 3_600_000, hour: 3_600_000, m: 60_000, min: 60_000, minute: 60_000, s: 1000, sec: 1000, second: 1000 };

/// A length of time as typed: "25m", "1h 10m", "90s", "1.5h", or a bare
/// number of minutes. Undefined for anything else, or for nothing.
export function parseDuration(text: string): number | undefined {
  const trimmed = text.trim().toLowerCase();
  if (!trimmed) return undefined;
  if (/^\d+(\.\d+)?$/.test(trimmed)) return Math.round(Number(trimmed) * UNITS.m) || undefined;
  let total = 0;
  let rest = trimmed;
  while (rest) {
    const part = /^(\d+(?:\.\d+)?)\s*([a-z]+)\s*/.exec(rest);
    if (!part) return undefined;
    const unit = UNITS[part[2].replace(/s$/, "")] ?? UNITS[part[2]];
    if (!unit) return undefined;
    total += Number(part[1]) * unit;
    rest = rest.slice(part[0].length);
  }
  return Math.round(total) || undefined;
}

/// "25 min", "1 h 10 min", "1 h 30 s".
export function describe(ms: number): string {
  const seconds = Math.round(ms / 1000);
  const parts = [
    [Math.floor(seconds / 3600), (n: number) => t("{0} h", n)],
    [Math.floor((seconds % 3600) / 60), (n: number) => t("{0} min", n)],
    [seconds % 60, (n: number) => t("{0} s", n)],
  ] as const;
  const said = parts.filter(([n]) => n > 0).map(([n, say]) => say(n));
  return said.length ? said.join(" ") : t("{0} s", 0);
}

const clock = (at: number) => new Date(at).toLocaleTimeString(locale(), { hour: "2-digit", minute: "2-digit" });

/// The activity's content for a span: the countdown in both forms, and
/// the buttons, each carrying the span so the handler can go on from it.
export function content(span: Span): Content {
  const timer = { start: new Date(span.start), end: new Date(span.end), pausedAt: span.pausedAt === undefined ? undefined : new Date(span.pausedAt) };
  const paused = span.pausedAt !== undefined;
  const actions: Button[] = [
    paused ? { title: t("Resume"), icon: "play-16", action: { event: "resume", data: span } } : { title: t("Pause"), icon: "pause-16", action: { event: "pause", data: span } },
    { title: t("+1 min"), icon: "plus-16", action: { event: "plus", data: span } },
    { title: t("End"), icon: "stop-16", style: "destructive", action: { end: true } },
  ];
  return {
    title: describe(span.end - span.start),
    subtitle: paused ? t("Paused") : t("Until {0}", clock(span.end)),
    compact: { timer },
    body: { timer },
    actions,
  };
}

/// What a button or the end of the countdown does to the timer.
async function handle(event: Event) {
  const activity = await LiveActivity.get(event.id);
  if (!activity) return;
  if (event.kind === "elapsed") {
    await activity.end({ title: t("Time's up"), subtitle: "", compact: "0:00", body: { text: t("The timer has run out.") }, actions: [] });
    return;
  }
  if (event.kind !== "action") return;
  const span = event.data as Span;
  const now = Date.now();
  switch (event.event) {
    case "pause":
      await activity.update(content({ ...span, pausedAt: now }));
      break;
    case "resume": {
      // The time it stood still is added at the end.
      const stood = now - (span.pausedAt ?? now);
      await activity.update(content({ start: span.start + stood, end: span.end + stood }));
      break;
    }
    case "plus":
      await activity.update(content({ ...span, end: span.end + 60_000 }));
      break;
  }
}

export interface TimerProps {
  arguments?: { duration?: string };
  launchContext?: unknown;
}

/// Run from the palette with a duration: starts the countdown. Run by the
/// host for an event of the activity: handles it.
export async function Timer(props: TimerProps) {
  const event = (props.launchContext as { activity?: Event } | undefined)?.activity;
  if (event) return handle(event);
  const length = parseDuration(props.arguments?.duration ?? "");
  if (!length) {
    await showToast({ style: Toast.Style.Failure, title: t("Say how long"), message: t("For example 25m, 1h 10m or 90s") });
    return;
  }
  await closeMainWindow();
  const start = Date.now();
  await LiveActivity.start({ id: `timer-${start}`, name: t("Timer"), icon: "clock-16" }, content({ start, end: start + length }));
}
