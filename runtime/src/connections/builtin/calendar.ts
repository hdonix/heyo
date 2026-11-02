//! Calendars: CalDAV (`@heyo:caldav`, through tsdav) and subscriptions to
//! an iCalendar address (`@heyo:ics`). Both bring the events from 30 days
//! back to 180 ahead (`events.ts`).
//!
//! - CalDAV: a calendar whose ctag (or sync token) is what it was is not
//!   read again; one that changed is read for the window, and only the
//!   objects whose ETag changed are made items again. What is no longer
//!   there, or no longer in the window, is removed.
//! - ICS: the address is asked with the ETag it answered last; a 304 is
//!   nothing new.
//! - Either way, once a day everything is read again whole, since the
//!   window moved: the occurrences that came into it are brought.

import { DAVClient, type DAVCalendar } from "tsdav";

import { t } from "../../../../shared/i18n";
import { ConnectorError, type Account, type Connector, type Item, type SyncIo } from "../../heyo/connector";
import { cursorOf, failure, list, required, secret, stopped } from "./common";
import { dayOf, eventsOf, gone, windowAround, type Window } from "./events";

/// A refusal of the credentials, as tsdav words it (`… 401 Unauthorized`).
const refused = (error: any) => /\b(401|403)\b|unauthori[sz]ed|forbidden/i.test(String(error?.message ?? ""));

/// Puts what `place` has now and removes what it had before and has not.
async function bring(io: SyncIo, items: Item[], before: string[] | undefined) {
  await io.put(items);
  const went = gone(before, items);
  if (went.length) await io.remove(went);
}

// --- CalDAV --------------------------------------------------------------------

interface Calendar {
  /// Its sync token or ctag when it was last read.
  tag?: string;
  /// Its objects, by address: their ETag, and the ids of their items.
  objects: Record<string, { etag?: string; ids: string[] }>;
}

export interface CaldavCursor {
  day: string;
  calendars: Record<string, Calendar>;
}

/// What is asked of a CalDAV server, so that tests can give a fake one.
export interface Dav {
  calendars(): Promise<DAVCalendar[]>;
  objects(calendar: DAVCalendar, window: Window): Promise<{ url: string; etag?: string; data?: string }[]>;
}

const nameOf = (calendar: DAVCalendar) => (typeof calendar.displayName === "string" ? calendar.displayName : calendar.url);

/// The calendars of events `names` asks for, all of them when it names none.
export async function chosen(dav: Dav, names: string[]): Promise<DAVCalendar[]> {
  const all = (await dav.calendars()).filter((calendar) => !calendar.components?.length || calendar.components.includes("VEVENT"));
  if (!all.length) throw new ConnectorError("failed", t("The server has no calendars."));
  if (!names.length) return all;
  const missing = names.find((name) => !all.some((calendar) => nameOf(calendar) === name));
  if (missing) throw new ConnectorError("failed", t("No calendar is named {0}.", missing));
  return all.filter((calendar) => names.includes(nameOf(calendar)));
}

export async function syncCaldav(dav: Dav, names: string[], cursor: CaldavCursor | undefined, io: SyncIo, now: Date): Promise<CaldavCursor> {
  const day = dayOf(now);
  const window = windowAround(now);
  const sameDay = cursor?.day === day;
  const calendars = await chosen(dav, names);
  const next: CaldavCursor = { day, calendars: {} };
  for (const [index, calendar] of calendars.entries()) {
    stopped(io.signal);
    io.progress(index / calendars.length, nameOf(calendar));
    const before = cursor?.calendars[calendar.url];
    const tag = calendar.syncToken ?? calendar.ctag;
    if (before && sameDay && tag && before.tag === tag) {
      next.calendars[calendar.url] = before;
      continue;
    }
    const kept: Calendar = { tag, objects: {} };
    const items: Item[] = [];
    const went: string[] = [];
    for (const object of await dav.objects(calendar, window)) {
      const had = before?.objects[object.url];
      if (had && sameDay && object.etag && had.etag === object.etag) {
        kept.objects[object.url] = had;
        continue;
      }
      let made: Item[] = [];
      try {
        made = object.data ? eventsOf(object.data, object.url, window) : [];
      } catch (error) {
        // One object that is not iCalendar does not stop the others.
        console.error(`heyo-connections: ${object.url}: ${error}`);
      }
      items.push(...made);
      went.push(...gone(had?.ids, made));
      kept.objects[object.url] = { etag: object.etag, ids: made.map((item) => item.id) };
    }
    // The objects no longer there, or no longer in the window.
    for (const [url, had] of Object.entries(before?.objects ?? {})) if (!kept.objects[url]) went.push(...had.ids);
    await io.put(items);
    if (went.length) await io.remove(went);
    next.calendars[calendar.url] = kept;
  }
  // The calendars no longer synced.
  const dropped = Object.entries(cursor?.calendars ?? {}).filter(([url]) => !next.calendars[url]);
  const ids = dropped.flatMap(([, calendar]) => Object.values(calendar.objects).flatMap((object) => object.ids));
  if (ids.length) await io.remove(ids);
  io.progress(1);
  return next;
}

/// A signed-in tsdav client as a `Dav`.
async function davOf(account: Account): Promise<Dav> {
  const client = new DAVClient({
    serverUrl: required(account, "url", t("Server")),
    credentials: { username: required(account, "username", t("User Name")), password: await secret(account, "password") },
    authMethod: "Basic",
    defaultAccountType: "caldav",
  });
  await client.login();
  return {
    calendars: () => client.fetchCalendars(),
    objects: async (calendar, window) =>
      (await client.fetchCalendarObjects({ calendar, timeRange: { start: window.start.toISOString(), end: window.end.toISOString() } })).map((object) => ({
        url: object.url,
        etag: object.etag ?? undefined,
        data: typeof object.data === "string" ? object.data : undefined,
      })),
  };
}

export const caldav: Connector = {
  async check(account) {
    try {
      await chosen(await davOf(account), list(account, "calendars"));
    } catch (error) {
      throw failure(error, refused);
    }
  },
  async sync(account, cursor, io) {
    try {
      const dav = await davOf(account);
      return JSON.stringify(await syncCaldav(dav, list(account, "calendars"), cursorOf<CaldavCursor>(cursor), io, new Date()));
    } catch (error) {
      stopped(io.signal);
      throw failure(error, refused);
    }
  },
};

// --- ICS subscriptions ---------------------------------------------------------

export interface IcsCursor {
  day: string;
  etag?: string;
  ids: string[];
}

/// The address as fetch takes it: `webcal:` is HTTPS.
export function addressOf(account: Account): URL {
  const given = required(account, "url", t("Address")).replace(/^webcals?:\/\//i, "https://");
  try {
    const url = new URL(given);
    if (url.protocol === "https:" || url.protocol === "http:") return url;
  } catch {
    // Said below.
  }
  throw new ConnectorError("failed", t("{0} is not an address.", given));
}

/// The calendar at the address, or `undefined` when it has not changed
/// since `etag`.
async function fetchIcs(url: URL, etag: string | undefined, signal?: AbortSignal): Promise<{ text: string; etag?: string } | undefined> {
  const response = await fetch(url, { headers: { Accept: "text/calendar, */*", ...(etag ? { "If-None-Match": etag } : {}) }, signal, redirect: "follow" });
  if (response.status === 304) return undefined;
  if (response.status === 401 || response.status === 403) throw Object.assign(new Error(`${response.status} ${response.statusText}`), { status: response.status });
  if (!response.ok) {
    const kind = response.status >= 500 || response.status === 429 ? "unavailable" : "failed";
    throw new ConnectorError(kind, t("The address answered {0}.", `${response.status} ${response.statusText}`));
  }
  const text = await response.text();
  if (!/BEGIN:VCALENDAR/i.test(text.slice(0, 2048))) throw new ConnectorError("failed", t("The address did not answer with a calendar: {0}", text.slice(0, 80).replace(/\s+/g, " ")));
  return { text, etag: response.headers.get("etag") ?? undefined };
}

export const ics: Connector = {
  async check(account) {
    try {
      const fetched = await fetchIcs(addressOf(account), undefined);
      eventsOf(fetched!.text, "ics", windowAround(new Date()));
    } catch (error) {
      throw failure(error);
    }
  },
  async sync(account, cursor, io) {
    const before = cursorOf<IcsCursor>(cursor);
    const now = new Date();
    const day = dayOf(now);
    try {
      io.progress(0);
      const fetched = await fetchIcs(addressOf(account), before?.day === day ? before.etag : undefined, io.signal);
      if (!fetched) return JSON.stringify(before);
      const items = eventsOf(fetched.text, "ics", windowAround(now));
      await bring(io, items, before?.ids);
      io.progress(1);
      return JSON.stringify({ day, etag: fetched.etag, ids: items.map((item) => item.id) } satisfies IcsCursor);
    } catch (error) {
      stopped(io.signal);
      throw failure(error);
    }
  },
};
