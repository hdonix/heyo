import { describe, expect, it } from "vitest";

import { eventsOf, gone, meetingLink, windowAround } from "./events";
import { calendar as ics, now } from "./testing";

const SHANGHAI = ["BEGIN:VTIMEZONE", "TZID:Asia/Shanghai", "BEGIN:STANDARD", "DTSTART:19700101T000000", "TZOFFSETFROM:+0800", "TZOFFSETTO:+0800", "TZNAME:CST", "END:STANDARD", "END:VTIMEZONE"];

const window = windowAround(now);

describe("iCalendar events", () => {
  it("makes an item of an event, in its time zone, with its people, place and meeting link", () => {
    const text = ics(
      ...SHANGHAI,
      "BEGIN:VEVENT",
      "UID:review-1",
      "DTSTART;TZID=Asia/Shanghai:20261105T100000",
      "DTEND;TZID=Asia/Shanghai:20261105T110000",
      "SUMMARY:Design review",
      "LOCATION:Room 3",
      "DESCRIPTION:Agenda first.\\nJoin: https://meeting.tencent.com/dm/abc123.",
      "ORGANIZER;CN=Ann:mailto:ann@example.com",
      "ATTENDEE;CN=Bo:mailto:bo@example.com",
      "END:VEVENT",
    );
    expect(eventsOf(text, "cal", window)).toEqual([
      {
        id: "cal#review-1",
        kind: "event",
        title: "Design review",
        time: "2026-11-05T02:00:00.000Z",
        end: "2026-11-05T03:00:00.000Z",
        people: [
          { name: "Ann", address: "ann@example.com", role: "organizer" },
          { name: "Bo", address: "bo@example.com", role: "attendee" },
        ],
        text: "Agenda first.\nJoin: https://meeting.tencent.com/dm/abc123.",
        url: "https://meeting.tencent.com/dm/abc123",
        location: "Room 3",
      },
    ]);
  });

  it("writes a whole day as a date, and a floating time without an offset", () => {
    const text = ics(
      "BEGIN:VEVENT", "UID:day", "DTSTART;VALUE=DATE:20261110", "DTEND;VALUE=DATE:20261111", "SUMMARY:Holiday", "END:VEVENT",
      "BEGIN:VEVENT", "UID:float", "DTSTART:20261111T090000", "SUMMARY:Standup", "END:VEVENT",
    );
    expect(eventsOf(text, "c", window).map((item) => [item.title, item.time, item.end])).toEqual([
      ["Holiday", "2026-11-10", "2026-11-11"],
      ["Standup", "2026-11-11T09:00:00", undefined],
    ]);
  });

  it("expands a recurring event within the window, with its exceptions, without what was cancelled", () => {
    const text = ics(
      "BEGIN:VEVENT",
      "UID:weekly",
      "DTSTART:20250106T080000Z",
      "DTEND:20250106T083000Z",
      "RRULE:FREQ=WEEKLY;BYDAY=MO",
      "EXDATE:20261109T080000Z",
      "SUMMARY:Weekly",
      "END:VEVENT",
      "BEGIN:VEVENT",
      "UID:weekly",
      "RECURRENCE-ID:20261116T080000Z",
      "DTSTART:20261116T090000Z",
      "DTEND:20261116T093000Z",
      "SUMMARY:Weekly (moved)",
      "END:VEVENT",
      "BEGIN:VEVENT",
      "UID:weekly",
      "RECURRENCE-ID:20261123T080000Z",
      "DTSTART:20261123T080000Z",
      "STATUS:CANCELLED",
      "SUMMARY:Weekly",
      "END:VEVENT",
    );
    const items = eventsOf(text, "c", window);
    const times = items.map((item) => item.time);
    // From 30 days back (4 October) to 180 ahead (2 May): Mondays.
    expect(times[0]).toBe("2026-10-05T08:00:00.000Z");
    expect(times.at(-1)).toBe("2027-04-26T08:00:00.000Z");
    expect(times).not.toContain("2026-11-09T08:00:00.000Z");
    expect(times).not.toContain("2026-11-23T08:00:00.000Z");
    const moved = items.find((item) => item.title === "Weekly (moved)")!;
    expect(moved).toMatchObject({ id: "c#weekly@2026-11-16T08:00:00.000Z", time: "2026-11-16T09:00:00.000Z" });
    expect(new Set(items.map((item) => item.id)).size).toBe(items.length);
  });

  it("leaves out what is outside the window and what was cancelled", () => {
    const text = ics(
      "BEGIN:VEVENT", "UID:old", "DTSTART:20250101T080000Z", "SUMMARY:Old", "END:VEVENT",
      "BEGIN:VEVENT", "UID:far", "DTSTART:20280101T080000Z", "SUMMARY:Far", "END:VEVENT",
      "BEGIN:VEVENT", "UID:off", "DTSTART:20261201T080000Z", "STATUS:CANCELLED", "SUMMARY:Off", "END:VEVENT",
    );
    expect(eventsOf(text, "c", window)).toEqual([]);
  });

  it("finds meeting links and says what went", () => {
    expect(meetingLink("Call at https://us02web.zoom.us/j/123?pwd=x, thanks")).toBe("https://us02web.zoom.us/j/123?pwd=x");
    expect(meetingLink("https://example.com/x", undefined, "https://vc.feishu.cn/j/42")).toBe("https://vc.feishu.cn/j/42");
    expect(meetingLink("https://example.com/agenda")).toBeUndefined();
    expect(gone(["a", "b"], [{ id: "b", kind: "event", title: "", time: "" }])).toEqual(["a"]);
  });

  it("refuses what is not iCalendar", () => {
    expect(() => eventsOf("<html>", "c", window)).toThrow();
  });
});
