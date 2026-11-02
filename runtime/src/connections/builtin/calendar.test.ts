import http from "node:http";
import type { AddressInfo } from "node:net";
import type { DAVCalendar } from "tsdav";
import { afterAll, beforeAll, describe, expect, it } from "vitest";

import type { Account } from "../../heyo/connector";
import { loader } from "../load";
import { addressOf, caldav, ics, syncCaldav, type Dav } from "./calendar";
import { calendar as calendarText, now, recorder } from "./testing";

const event = (uid: string, day: string, title: string) => ["BEGIN:VEVENT", `UID:${uid}`, `DTSTART:${day}T080000Z`, `SUMMARY:${title}`, "END:VEVENT"];

/// A CalDAV server in memory: calendars with tags, objects with ETags.
class Server implements Dav {
  calendarsThere: (DAVCalendar & { held: Map<string, { etag: string; data: string }> })[] = [];
  read: string[] = [];

  calendar(url: string, name: string, ctag: string) {
    const calendar = { url, displayName: name, ctag, components: ["VEVENT"], held: new Map() };
    this.calendarsThere.push(calendar);
    return calendar;
  }
  async calendars() {
    return this.calendarsThere;
  }
  async objects(calendar: DAVCalendar) {
    this.read.push(calendar.url);
    const there = this.calendarsThere.find((each) => each.url === calendar.url)!;
    return [...there.held].map(([url, object]) => ({ url, ...object }));
  }
}

describe("CalDAV", () => {
  it("reads a calendar only when its tag changed, and makes items only of the objects that changed", async () => {
    const server = new Server();
    const work = server.calendar("/cal/work/", "Work", "1");
    work.held.set("/cal/work/a.ics", { etag: "a1", data: calendarText(...event("a", "20261105", "Review")) });
    work.held.set("/cal/work/b.ics", { etag: "b1", data: calendarText(...event("b", "20261106", "Lunch")) });
    server.calendar("/cal/home/", "Home", "1").held.set("/cal/home/c.ics", { etag: "c1", data: calendarText(...event("c", "20261107", "Gym")) });

    const first = recorder();
    const cursor = await syncCaldav(server, ["Work"], undefined, first.io, now);
    expect(first.put.map((item) => [item.id, item.title])).toEqual([
      ["/cal/work/a.ics#a", "Review"],
      ["/cal/work/b.ics#b", "Lunch"],
    ]);

    // Nothing changed: the calendar is not read.
    server.read = [];
    const second = recorder();
    const same = await syncCaldav(server, ["Work"], cursor, second.io, now);
    expect([server.read, second.put]).toEqual([[], []]);

    // One object changed, one went.
    work.ctag = "2";
    work.held.set("/cal/work/a.ics", { etag: "a2", data: calendarText(...event("a", "20261105", "Review, moved")) });
    work.held.delete("/cal/work/b.ics");
    const third = recorder();
    const next = await syncCaldav(server, ["Work"], JSON.parse(JSON.stringify(same)), third.io, now);
    expect(third.put.map((item) => item.title)).toEqual(["Review, moved"]);
    expect(third.removed).toEqual(["/cal/work/b.ics#b"]);

    // A new day: everything is made again, the window having moved.
    const fourth = recorder();
    await syncCaldav(server, ["Work"], next, fourth.io, new Date(now.getTime() + 86_400_000));
    expect(fourth.put.map((item) => item.title)).toEqual(["Review, moved"]);
  });

  it("removes the items of a calendar no longer synced, and says which name is not there", async () => {
    const server = new Server();
    server.calendar("/cal/work/", "Work", "1").held.set("/cal/work/a.ics", { etag: "a1", data: calendarText(...event("a", "20261105", "Review")) });
    server.calendar("/cal/home/", "Home", "1");
    const cursor = await syncCaldav(server, [], undefined, recorder().io, now);
    const later = recorder();
    await syncCaldav(server, ["Home"], cursor, later.io, now);
    expect(later.removed).toEqual(["/cal/work/a.ics#a"]);
    await expect(syncCaldav(server, ["Gone"], cursor, recorder().io, now)).rejects.toMatchObject({ kind: "failed", message: "No calendar is named Gone." });
  });

  it("are what accounts of @heyo:caldav and @heyo:ics are synced by", () => {
    expect(loader()({ extension: "@heyo", command: "caldav" })).toBe(caldav);
    expect(loader()({ extension: "@heyo", command: "ics" })).toBe(ics);
  });
});

describe("ICS subscriptions", () => {
  let server: http.Server;
  let base: string;
  let body = calendarText(...event("a", "20261105", "Review"));
  let etag = '"1"';
  const asked: (string | undefined)[] = [];

  beforeAll(async () => {
    server = http.createServer((request, response) => {
      asked.push(request.headers["if-none-match"]);
      if (request.url === "/secret.ics") return void response.writeHead(401).end();
      if (request.url === "/page") return void response.writeHead(200, { "content-type": "text/html" }).end("<html>Sign in</html>");
      if (request.headers["if-none-match"] === etag) return void response.writeHead(304).end();
      response.writeHead(200, { "content-type": "text/calendar", etag }).end(body);
    });
    await new Promise<void>((done) => server.listen(0, "127.0.0.1", done));
    base = `http://127.0.0.1:${(server.address() as AddressInfo).port}`;
  });
  afterAll(() => void server.close());

  const account = (url: string): Account => ({ id: "ics", name: "ICS", config: { url }, secret: async () => undefined });

  it("brings the events, then nothing while the ETag is the same, then what changed", async () => {
    const first = recorder();
    const cursor = await ics.sync(account(`${base}/cal.ics`), undefined, first.io);
    expect(first.put.map((item) => item.title)).toEqual(["Review"]);

    const second = recorder();
    const same = await ics.sync(account(`${base}/cal.ics`), cursor, second.io);
    expect(asked.at(-1)).toBe('"1"');
    expect([second.put, same]).toEqual([[], cursor]);

    body = calendarText(...event("b", "20261106", "Lunch"));
    etag = '"2"';
    const third = recorder();
    await ics.sync(account(`${base}/cal.ics`), cursor, third.io);
    expect(third.put.map((item) => item.title)).toEqual(["Lunch"]);
    expect(third.removed).toEqual(["ics#a"]);
  });

  it("says what is wrong with an address", async () => {
    await expect(ics.check(account(`${base}/cal.ics`))).resolves.toBeUndefined();
    await expect(ics.check(account(`${base}/secret.ics`))).rejects.toMatchObject({ kind: "auth" });
    await expect(ics.check(account(`${base}/page`))).rejects.toMatchObject({ kind: "failed" });
    await expect(ics.check(account("http://127.0.0.1:1/cal.ics"))).rejects.toMatchObject({ kind: "unavailable" });
    expect(addressOf(account("webcal://example.com/a.ics")).toString()).toBe("https://example.com/a.ics");
    expect(() => addressOf(account("ftp://example.com"))).toThrow("is not an address");
  });
});
