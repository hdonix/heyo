//! Dates in and out of the browser's date fields (`<input type="date">`
//! and `datetime-local`), which speak local time as text.

import { t } from "../../shared/i18n";

const pad = (n: number, width = 2) => String(n).padStart(width, "0");

/// A date as the field's text; empty for no date.
export function toDateInput(value: unknown, withTime: boolean): string {
  if (!(value instanceof Date) || isNaN(value.getTime())) return "";
  const day = `${pad(value.getFullYear(), 4)}-${pad(value.getMonth() + 1)}-${pad(value.getDate())}`;
  return withTime ? `${day}T${pad(value.getHours())}:${pad(value.getMinutes())}` : day;
}

/// The field's text as a date; null when it is empty or not a date. A day
/// without a time is that day's local midnight, which is what Raycast's
/// `isFullDay` looks for (the bare text would be read as midnight in UTC).
export function fromDateInput(text: string): Date | null {
  const match = /^(\d{4,})-(\d{2})-(\d{2})(?:T(\d{2}):(\d{2})(?::(\d{2}))?)?/.exec(text.trim());
  if (!match) return null;
  const [, year, month, day, hours = "0", minutes = "0", seconds = "0"] = match;
  const date = new Date(0);
  date.setFullYear(Number(year), Number(month) - 1, Number(day));
  date.setHours(Number(hours), Number(minutes), Number(seconds), 0);
  return isNaN(date.getTime()) ? null : date;
}

/// Whether `date` is allowed by `min` and `max` (both inclusive, either may be absent).
export function inRange(date: Date, min: unknown, max: unknown): boolean {
  if (min instanceof Date && date < min) return false;
  if (max instanceof Date && date > max) return false;
  return true;
}

export interface DateChoice {
  title: string;
  date: Date | null;
}

/// The dates offered beside the field of `Action.PickDate`: the ones asked
/// for most, of those `min` and `max` allow. Days are whole days; with a
/// time, they are at nine in the morning, except today which is in an hour.
export function dateChoices(now: Date, withTime: boolean, min?: unknown, max?: unknown): DateChoice[] {
  const day = (offset: number) => {
    const date = new Date(now.getFullYear(), now.getMonth(), now.getDate() + offset, withTime ? 9 : 0);
    return date;
  };
  const choices: DateChoice[] = [];
  if (withTime) {
    const soon = new Date(now.getTime() + 60 * 60 * 1000);
    soon.setSeconds(0, 0);
    choices.push({ title: t("In an Hour"), date: soon });
  } else choices.push({ title: t("Today"), date: day(0) });
  choices.push({ title: t("Tomorrow"), date: day(1) });
  // Monday of next week; on a Sunday that is tomorrow, already offered, so
  // the Monday after.
  const monday = ((8 - now.getDay()) % 7) || 7;
  choices.push({ title: t("Next Week"), date: day(monday === 1 ? 8 : monday) });
  return [...choices.filter((c) => inRange(c.date!, min, max)), { title: t("No Date"), date: null }];
}
