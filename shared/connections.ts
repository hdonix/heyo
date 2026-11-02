//! What an item a connector synced is said as, wherever it is shown: its
//! row in the root search (src/rows.ts) and its preview (the runtime's
//! `connection-item` command).

import type { ConnectionItem } from "./bindings/ConnectionItem";
import type { ItemKind } from "./bindings/ItemKind";
import type { ItemPerson } from "./bindings/ItemPerson";
import { locale, t } from "./i18n";

/// The kinds of item, in the order their groups are listed.
export const KINDS: ItemKind[] = ["email", "event", "message", "document", "task"];

/// What a kind of item is called, many of them: a group's title.
export function kindTitle(kind: ItemKind): string {
  switch (kind) {
    case "email":
      return t("Mail");
    case "event":
      return t("Events");
    case "message":
      return t("Messages");
    case "document":
      return t("Documents");
    case "task":
      return t("Tasks");
  }
}

/// The symbol of a kind of item (a Raycast icon name).
export function kindIcon(kind: ItemKind): string {
  switch (kind) {
    case "email":
      return "envelope-16";
    case "event":
      return "calendar-16";
    case "message":
      return "speech-bubble-16";
    case "document":
      return "blank-document-16";
    case "task":
      return "check-list-16";
  }
}

const named = (person: ItemPerson) => person.name?.trim() || person.address || "";

/// What copying the address of who it is from is called: a mail's sender,
/// a meeting's organizer.
export function copyFromTitle(item: ConnectionItem): string {
  return item.kind === "event" ? t("Copy Organizer Address") : t("Copy Sender Address");
}

/// Who it is from: the sender of a mail, the organizer of a meeting.
export function sender(item: ConnectionItem): ItemPerson | undefined {
  return item.people?.find((person) => person.role === "from" || person.role === "organizer");
}

/// The other party, as a row names them: who sent it or called it, else
/// the first of those it is to.
export function otherParty(item: ConnectionItem): string {
  const person = sender(item) ?? item.people?.[0];
  return person ? named(person) : "";
}

/// An ISO 8601 time as a date; a date alone is local midnight.
function dateOf(text: string): Date | null {
  const date = /^\d{4}-\d{2}-\d{2}$/.test(text) ? new Date(`${text}T00:00:00`) : new Date(text);
  return isNaN(date.getTime()) ? null : date;
}

const sameDay = (a: Date, b: Date) => a.toDateString() === b.toDateString();

/// Whole days from `now`'s day to `date`'s, by the calendar.
function daysBetween(now: Date, date: Date): number {
  const midnight = (d: Date) => new Date(d.getFullYear(), d.getMonth(), d.getDate()).getTime();
  return Math.round((midnight(date) - midnight(now)) / 86_400_000);
}

/// When it was sent, or begins, in a few words: yesterday, today or
/// tomorrow with the time, a weekday within a week, else the date; the
/// time with them where it says something (a mail of the last week, a
/// meeting whenever it is), and none for a whole day.
export function when(item: ConnectionItem, now = new Date()): string {
  const date = dateOf(item.time);
  if (!date) return "";
  const wholeDay = /^\d{4}-\d{2}-\d{2}$/.test(item.time);
  const days = daysBetween(now, date);
  const near = Math.abs(days) < 7;
  const time = wholeDay || (!near && item.kind !== "event") ? "" : date.toLocaleTimeString(locale(), { hour: "numeric", minute: "2-digit" });
  let day: string;
  if (Math.abs(days) <= 1) {
    const word = new Intl.RelativeTimeFormat(locale(), { numeric: "auto" }).format(days, "day");
    day = word.charAt(0).toLocaleUpperCase(locale()) + word.slice(1);
  } else if (near) day = date.toLocaleDateString(locale(), { weekday: "short" });
  else day = date.toLocaleDateString(locale(), date.getFullYear() === now.getFullYear() ? { month: "short", day: "numeric" } : { year: "numeric", month: "short", day: "numeric" });
  return time ? `${day} ${time}` : day;
}

/// When it was sent, or when it is (from and to), in full.
export function span(item: ConnectionItem): string {
  const start = dateOf(item.time);
  if (!start) return item.time;
  const wholeDay = /^\d{4}-\d{2}-\d{2}$/.test(item.time);
  const day: Intl.DateTimeFormatOptions = { weekday: "short", year: "numeric", month: "short", day: "numeric" };
  const time: Intl.DateTimeFormatOptions = { hour: "numeric", minute: "2-digit" };
  const begins = wholeDay ? start.toLocaleDateString(locale(), day) : start.toLocaleString(locale(), { ...day, ...time });
  const end = item.end ? dateOf(item.end) : null;
  if (!end || end.getTime() === start.getTime()) return begins;
  if (wholeDay) return `${begins} – ${end.toLocaleDateString(locale(), day)}`;
  return `${begins} – ${sameDay(start, end) ? end.toLocaleTimeString(locale(), time) : end.toLocaleString(locale(), { ...day, ...time })}`;
}

/// The people of a role, as a line names them.
export function people(item: ConnectionItem, ...roles: ItemPerson["role"][]): string {
  return (item.people ?? [])
    .filter((person) => roles.includes(person.role))
    .map((person) => (person.name && person.address ? `${person.name} <${person.address}>` : named(person)))
    .join(", ");
}

/// The item as its preview shows it: its title over what it says.
export function markdown(item: ConnectionItem): string {
  const title = item.title.trim() || t("(No Subject)");
  const text = item.text?.trim();
  return text ? `# ${title}\n\n${text}` : `# ${title}`;
}
