//! The CalDAV connector against Radicale, a CalDAV server, in a container:
//! run with `HEYO_CONTAINER_TESTS=1` where podman or docker is.

import { chmodSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import path from "node:path";
import { DAVClient } from "tsdav";
import { afterAll, beforeAll, describe, expect, it } from "vitest";

import type { Account } from "../../heyo/connector";
import { caldav } from "./calendar";
import { calendar, engine, recorder, start, type Container } from "./testing";

const command = engine();

describe.skipIf(!command)("CalDAV against Radicale", () => {
  let server: Container;
  let dir: string;
  let account: Account;
  let admin: DAVClient;
  let work: string;

  const soon = (days: number) => new Date(Date.now() + days * 86_400_000).toISOString().slice(0, 10).replaceAll("-", "");
  const event = (uid: string, days: number, title: string, ...more: string[]) =>
    calendar("BEGIN:VEVENT", `UID:${uid}`, `DTSTART:${soon(days)}T080000Z`, `DTEND:${soon(days)}T090000Z`, `SUMMARY:${title}`, ...more, "END:VEVENT");

  beforeAll(async () => {
    dir = mkdtempSync(path.join(tmpdir(), "heyo-radicale-"));
    writeFileSync(path.join(dir, "config"), "[server]\nhosts = 0.0.0.0:5232\n[auth]\ntype = htpasswd\nhtpasswd_filename = /config/users\nhtpasswd_encryption = plain\n[storage]\nfilesystem_folder = /data/collections\n");
    writeFileSync(path.join(dir, "users"), "ann:secret\n");
    chmodSync(dir, 0o755);
    server = await start(command!, "docker.io/tomsquest/docker-radicale:latest", [5232], { volumes: [`${dir}:/config:ro`], probe: "GET / HTTP/1.0\r\n\r\n" });
    const url = `http://127.0.0.1:${server.port(5232)}/`;
    account = { id: "cal", name: "Cal", config: { url, username: "ann", calendars: ["Work"] }, secret: async (name) => (name === "password" ? "secret" : undefined) };
    admin = new DAVClient({ serverUrl: url, credentials: { username: "ann", password: "secret" }, authMethod: "Basic", defaultAccountType: "caldav" });
    await admin.login();
    work = `${url}ann/work/`;
    await admin.makeCalendar({ url: work, props: { displayname: "Work" } });
    await admin.makeCalendar({ url: `${url}ann/home/`, props: { displayname: "Home" } });
  }, 120_000);

  afterAll(() => {
    server?.stop();
    if (dir) rmSync(dir, { recursive: true, force: true });
  });

  const put = (name: string, text: string) => admin.createCalendarObject({ calendar: { url: work }, filename: name, iCalString: text });

  it("checks the password and the calendars named", async () => {
    await expect(caldav.check(account)).resolves.toBeUndefined();
    await expect(caldav.check({ ...account, secret: async () => "nope" })).rejects.toMatchObject({ kind: "auth" });
    await expect(caldav.check({ ...account, config: { ...account.config, calendars: ["Gone"] } })).rejects.toMatchObject({ kind: "failed" });
  });

  it("syncs the events, recurring ones expanded, then what changed and what went", async () => {
    await put("review.ics", event("review", 2, "Review", "DESCRIPTION:Join https://meet.google.com/abc-defg-hij"));
    await put("standup.ics", event("standup", -40, "Standup", "RRULE:FREQ=DAILY"));
    await put("old.ics", event("old", -60, "Long ago"));

    const first = recorder();
    const cursor = await caldav.sync(account, undefined, first.io);
    const titles = first.put.map((item) => item.title);
    expect(titles.filter((title) => title === "Review")).toHaveLength(1);
    expect(titles).not.toContain("Long ago");
    // Daily, from 30 days back to 180 ahead.
    expect(titles.filter((title) => title === "Standup").length).toBeGreaterThanOrEqual(210);
    expect(first.put.find((item) => item.title === "Review")!.url).toBe("https://meet.google.com/abc-defg-hij");

    const same = recorder();
    await caldav.sync(account, cursor, same.io);
    expect([same.put, same.removed]).toEqual([[], []]);

    const objects = await admin.fetchCalendarObjects({ calendar: { url: work } });
    const review = objects.find((object) => object.url.endsWith("review.ics"))!;
    await admin.deleteCalendarObject({ calendarObject: review });
    await put("lunch.ics", event("lunch", 3, "Lunch"));
    const later = recorder();
    await caldav.sync(account, cursor, later.io);
    expect(later.put.map((item) => item.title)).toEqual(["Lunch"]);
    expect(later.removed).toEqual([first.put.find((item) => item.title === "Review")!.id]);
  });
});
