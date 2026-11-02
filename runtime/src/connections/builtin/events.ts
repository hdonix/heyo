/*! The calendar connectors parse iCalendar with ical.js (https://github.com/kewisch/ical.js),
 * Copyright Philipp Kewisch and its contributors, under the Mozilla Public License 2.0
 * (https://mozilla.org/MPL/2.0/). Its source is at that address and in its npm package. */

//! What the calendar connectors share: iCalendar text made items, each
//! occurrence of a recurring event one item, from 30 days back to 180
//! ahead; and how the items of a calendar are told apart from those of the
//! sync before, to remove what went.
//!
//! An item's id is the place it came from (a CalDAV object's address, or
//! the subscription) with the event's UID, and for an occurrence of a
//! recurring event, when it was due to begin.

import ICAL from "ical.js";

import type { ItemPerson } from "../../../../shared/bindings/ItemPerson";
import { t } from "../../../../shared/i18n";
import type { Item } from "../../heyo/connector";
import { plain } from "./common";

/// How far back and ahead events are brought, in days.
export const WINDOW = { back: 30, ahead: 180 };

/// The most occurrences of one event looked at: a rule without end, from
/// years ago, is still cut short.
const MOST_STEPS = 5000;

export interface Window {
  start: Date;
  end: Date;
}

export function windowAround(now: Date): Window {
  const day = 86_400_000;
  return { start: new Date(now.getTime() - WINDOW.back * day), end: new Date(now.getTime() + WINDOW.ahead * day) };
}

/// The day `now` is in, as the cursors note it: a new day moves the window.
export const dayOf = (now: Date) => now.toISOString().slice(0, 10);

/// Addresses of video meetings, by host.
const MEETINGS = [
  /(^|\.)zoom\.(us|com\.cn)$/,
  /^meet\.google\.com$/,
  /^teams\.(microsoft|live)\.com$/,
  /^meeting\.tencent\.com$/,
  /^voovmeeting\.com$/,
  /(^|\.)feishu\.cn$/,
  /(^|\.)larksuite\.com$/,
  /^meeting\.dingtalk\.com$/,
  /(^|\.)webex\.com(\.cn)?$/,
  /^whereby\.com$/,
  /^meet\.jit\.si$/,
  /^(.+\.)?gotomeeting\.com$/,
];

/// The first address of a video meeting in `texts`.
export function meetingLink(...texts: (string | undefined)[]): string | undefined {
  for (const text of texts) {
    for (const [found] of (text ?? "").matchAll(/https?:\/\/[^\s<>"')\]]+/g)) {
      try {
        const url = new URL(found.replace(/[.,;:]+$/, ""));
        if (MEETINGS.some((host) => host.test(url.hostname))) return url.toString();
      } catch {
        // Not an address after all.
      }
    }
  }
  return undefined;
}

/// An ICAL time as an item says it: a date alone for a whole day, local
/// time without an offset for a floating one, UTC otherwise.
function when(time: ICAL.Time): string {
  if (time.isDate) return time.toString().slice(0, 10);
  if (time.zone === ICAL.Timezone.localTimezone) return time.toString().slice(0, 19);
  return time.toJSDate().toISOString();
}

function person(property: ICAL.Property, role: ItemPerson["role"]): ItemPerson | undefined {
  const value = String(property.getFirstValue() ?? "");
  const address = value.replace(/^mailto:/i, "") || undefined;
  const name = (property.getParameter("cn") as string | undefined) || undefined;
  if (!address && !name) return undefined;
  return { ...(name ? { name } : {}), ...(address ? { address } : {}), role };
}

function itemOf(id: string, event: ICAL.Event, start: ICAL.Time, end: ICAL.Time | undefined): Item {
  const component = event.component;
  const description = event.description || undefined;
  const people = [
    ...(component.getFirstProperty("organizer") ? [person(component.getFirstProperty("organizer")!, "organizer")] : []),
    ...component.getAllProperties("attendee").map((attendee) => person(attendee, "attendee")),
  ].filter((who): who is ItemPerson => !!who);
  const conference = [component.getFirstPropertyValue("x-google-conference"), component.getFirstPropertyValue("conference"), component.getFirstPropertyValue("url")].map((value) => (value == null ? undefined : String(value)));
  const url = meetingLink(...conference, event.location, description) ?? conference.find((value) => value && /^https?:\/\//.test(value));
  const text = plain(description, /<[a-z][^>]*>/i.test(description ?? ""));
  return {
    id,
    kind: "event",
    title: event.summary?.trim() || t("(no title)"),
    time: when(start),
    // ical.js makes an event without an end end when it starts.
    ...(end && end.compare(start) !== 0 ? { end: when(end) } : {}),
    ...(people.length ? { people } : {}),
    ...(text ? { text } : {}),
    ...(url ? { url } : {}),
    ...(event.location ? { location: event.location } : {}),
  };
}

/// The events of `ics` that fall in `window`, as items whose ids begin
/// with `place`. Throws when it is not iCalendar.
export function eventsOf(ics: string, place: string, window: Window): Item[] {
  const root = new ICAL.Component(ICAL.parse(ics));
  for (const zone of root.getAllSubcomponents("vtimezone")) {
    const timezone = new ICAL.Timezone(zone);
    if (!ICAL.TimezoneService.has(timezone.tzid)) ICAL.TimezoneService.register(timezone);
  }
  const from = ICAL.Time.fromJSDate(window.start, true);
  const to = ICAL.Time.fromJSDate(window.end, true);
  // Each event with the exceptions to its rule (the same UID, with a RECURRENCE-ID).
  const byUid = new Map<string, { master?: ICAL.Component; exceptions: ICAL.Component[] }>();
  for (const vevent of root.getAllSubcomponents("vevent")) {
    if (String(vevent.getFirstPropertyValue("status") ?? "").toUpperCase() === "CANCELLED" && !vevent.hasProperty("recurrence-id")) continue;
    const uid = String(vevent.getFirstPropertyValue("uid") ?? `${vevent.getFirstPropertyValue("dtstart")}`);
    const entry = byUid.get(uid) ?? { exceptions: [] };
    if (vevent.hasProperty("recurrence-id")) entry.exceptions.push(vevent);
    else entry.master = vevent;
    byUid.set(uid, entry);
  }
  const items: Item[] = [];
  const within = (start: ICAL.Time, end: ICAL.Time | undefined) => start.compare(to) < 0 && (end ?? start).compare(from) >= 0;
  for (const [uid, { master, exceptions }] of byUid) {
    const base = `${place}#${uid}`;
    if (!master) {
      // Exceptions alone (an invitation to one occurrence): each is an event of its own.
      for (const exception of exceptions) {
        const event = new ICAL.Event(exception);
        if (within(event.startDate, event.endDate)) items.push(itemOf(`${base}@${when(event.recurrenceId)}`, event, event.startDate, event.endDate));
      }
      continue;
    }
    const event = new ICAL.Event(master, { exceptions, strictExceptions: true });
    if (!event.isRecurring()) {
      if (within(event.startDate, event.endDate)) items.push(itemOf(base, event, event.startDate, event.endDate));
      continue;
    }
    const occurrences = event.iterator();
    for (let step = 0, next: ICAL.Time | null; step < MOST_STEPS && (next = occurrences.next()); step++) {
      if (next.compare(to) >= 0) break;
      const details = event.getOccurrenceDetails(next);
      if (!within(details.startDate, details.endDate)) continue;
      if (String(details.item.component.getFirstPropertyValue("status") ?? "").toUpperCase() === "CANCELLED") continue;
      items.push(itemOf(`${base}@${when(details.recurrenceId)}`, details.item, details.startDate, details.endDate));
    }
  }
  return items;
}

/// The ids among `before` that are not among `now`: what to remove.
export function gone(before: string[] | undefined, now: Item[]): string[] {
  const kept = new Set(now.map((item) => item.id));
  return (before ?? []).filter((id) => !kept.has(id));
}
