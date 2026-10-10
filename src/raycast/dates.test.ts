import { expect, test } from "vitest";

import { dateChoices, fromDateInput, inRange, toDateInput } from "./dates";

test("a day goes into the field and comes back as local midnight", () => {
  const date = fromDateInput("2026-03-09")!;
  expect([date.getFullYear(), date.getMonth(), date.getDate(), date.getHours(), date.getMinutes()]).toEqual([2026, 2, 9, 0, 0]);
  expect(toDateInput(date, false)).toBe("2026-03-09");
});

test("a day and time go in and come back the same", () => {
  const date = fromDateInput("2026-12-31T23:45")!;
  expect([date.getDate(), date.getHours(), date.getMinutes()]).toEqual([31, 23, 45]);
  expect(toDateInput(date, true)).toBe("2026-12-31T23:45");
  expect(toDateInput(date, false)).toBe("2026-12-31");
});

test("what is not a date is no date", () => {
  expect(fromDateInput("")).toBeNull();
  expect(fromDateInput("soon")).toBeNull();
  expect(toDateInput(null, true)).toBe("");
  expect(toDateInput(new Date(NaN), true)).toBe("");
  expect(toDateInput("2026-01-01", false)).toBe("");
});

test("min and max are inclusive", () => {
  const min = new Date(2026, 0, 10);
  const max = new Date(2026, 0, 20);
  expect(inRange(new Date(2026, 0, 10), min, max)).toBe(true);
  expect(inRange(new Date(2026, 0, 20), min, max)).toBe(true);
  expect(inRange(new Date(2026, 0, 9), min, max)).toBe(false);
  expect(inRange(new Date(2026, 0, 21), min, undefined)).toBe(true);
});

test("the choices offered are whole days, or times, within what is allowed", () => {
  // A Saturday afternoon.
  const now = new Date(2026, 9, 10, 15, 30, 20);
  const days = dateChoices(now, false);
  expect(days.map((c) => c.title)).toEqual(["Today", "Tomorrow", "Next Week", "No Date"]);
  expect(days.map((c) => c.date && toDateInput(c.date, true))).toEqual(["2026-10-10T00:00", "2026-10-11T00:00", "2026-10-12T00:00", null]);

  const times = dateChoices(now, true);
  expect(times.map((c) => c.date && toDateInput(c.date, true))).toEqual(["2026-10-10T16:30", "2026-10-11T09:00", "2026-10-12T09:00", null]);

  // On a Monday, next week is the Monday after.
  expect(toDateInput(dateChoices(new Date(2026, 9, 12, 8), false)[2].date, false)).toBe("2026-10-19");

  const limited = dateChoices(now, false, new Date(2026, 9, 11), new Date(2026, 9, 11, 23));
  expect(limited.map((c) => c.title)).toEqual(["Tomorrow", "No Date"]);
});
