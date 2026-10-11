import { expect, test } from "vitest";

import { changed, monitorOf, toWindow } from "./windows";

const left = { x: 0, y: 0, width: 1920, height: 1080 };
const right = { x: 1920, y: 0, width: 2560, height: 1440 };

test("a window is on the monitor that holds its centre", () => {
  expect(monitorOf({ x: 100, y: 100, width: 800, height: 600 }, [left, right])).toBe(0);
  expect(monitorOf({ x: 1800, y: 100, width: 800, height: 600 }, [left, right])).toBe(1);
  // Off every monitor, or of unknown place: the first.
  expect(monitorOf({ x: -5000, y: 0, width: 10, height: 10 }, [left, right])).toBe(0);
  expect(monitorOf(undefined, [left, right])).toBe(0);
});

test("a window is told the way Raycast tells it", () => {
  const window = toWindow({ id: 73, title: "Notes", class: "notes", active: true, bounds: { x: 2000, y: 40, width: 900, height: 700 } }, [left, right]);
  expect(window).toMatchObject({
    id: "73",
    active: true,
    desktopId: "2",
    bounds: { position: { x: 2000, y: 40 }, size: { width: 900, height: 700 } },
  });
});

test("bounds given in part keep the rest", () => {
  const current = { x: 10, y: 20, width: 300, height: 200 };
  expect(changed(current, { position: { x: 50 } })).toEqual({ x: 50, y: 20, width: 300, height: 200 });
  expect(changed(current, { size: { width: 640, height: 480 } })).toEqual({ x: 10, y: 20, width: 640, height: 480 });
  expect(changed(current, {})).toEqual(current);
  // The host takes whole pixels.
  expect(changed(current, { position: { x: 10.6, y: 0.2 } })).toMatchObject({ x: 11, y: 0 });
});
