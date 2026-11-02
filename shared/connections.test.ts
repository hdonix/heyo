// How an item a connector synced is said: its time in a row, who it is from.
import { expect, test } from "vitest";

import type { ConnectionItem } from "./bindings/ConnectionItem";
import { copyFromTitle, otherParty, when } from "./connections";

const now = new Date(2026, 10, 3, 10, 0); // Tuesday 3 November 2026, 10:00
const at = (time: string, rest: Partial<ConnectionItem> = {}): ConnectionItem => ({ id: "1", kind: "email", title: "x", time, ...rest });
const local = (day: number, hours: number, minutes = 0) => {
  const date = new Date(2026, 10, day, hours, minutes);
  const pad = (n: number) => String(n).padStart(2, "0");
  return `${date.getFullYear()}-${pad(date.getMonth() + 1)}-${pad(date.getDate())}T${pad(hours)}:${pad(minutes)}`;
};
const clock = (day: number, hours: number, minutes = 0) => new Date(2026, 10, day, hours, minutes).toLocaleTimeString("en", { hour: "numeric", minute: "2-digit" });

test("a time near now is said by its day and its time, a time further by its date", () => {
  expect(when(at(local(3, 14)), now)).toBe(`Today ${clock(3, 14)}`);
  expect(when(at(local(2, 21, 38)), now)).toBe(`Yesterday ${clock(2, 21, 38)}`);
  expect(when(at(local(4, 14), { kind: "event" }), now)).toBe(`Tomorrow ${clock(4, 14)}`);
  // Within a week, its weekday.
  expect(when(at(local(6, 9), { kind: "event" }), now)).toBe(`Fri ${clock(6, 9)}`);
  // Further: the date; a meeting keeps its time, a mail does not.
  expect(when(at(local(20, 9)), now)).toBe("Nov 20");
  expect(when(at(local(20, 9), { kind: "event" }), now)).toBe(`Nov 20 ${clock(20, 9)}`);
  expect(when(at("2025-03-01T09:00"), now)).toBe("Mar 1, 2025");
  // A whole day has no time.
  expect(when(at("2026-11-04", { kind: "event" }), now)).toBe("Tomorrow");
});

test("who it is from is its sender or organizer, and copying their address is called so", () => {
  const mail = at(local(3, 9), { people: [{ address: "a@example.com", role: "to" }, { name: "Zhang Wei", address: "z@example.com", role: "from" }] });
  const meeting = at(local(3, 9), { kind: "event", people: [{ name: "Li Na", role: "organizer" }] });
  expect([otherParty(mail), copyFromTitle(mail)]).toEqual(["Zhang Wei", "Copy Sender Address"]);
  expect([otherParty(meeting), copyFromTitle(meeting)]).toEqual(["Li Na", "Copy Organizer Address"]);
});
