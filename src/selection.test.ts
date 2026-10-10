// @vitest-environment happy-dom
import { expect, test } from "vitest";

import { pointerDriven } from "./pointer";
import { shortfall, step, underPointer } from "./selection";

const move = (screenX: number, screenY: number) => window.dispatchEvent(new MouseEvent("mousemove", { screenX, screenY }));

test("a wrapping step joins the ends", () => {
  expect(step(0, 1, 3, true)).toBe(1);
  expect(step(2, 1, 3, true)).toBe(0);
  expect(step(0, -1, 3, true)).toBe(2);
  expect(step(1, -8, 3, true)).toBe(2);
});

test("a step that does not wrap stops at the ends", () => {
  expect(step(1, 8, 5, false)).toBe(4);
  expect(step(1, -8, 5, false)).toBe(0);
  expect(step(2, 1, 5, false)).toBe(3);
});

test("an empty list has nothing to step to", () => {
  expect(step(0, 1, 0, true)).toBe(0);
  expect(step(3, -1, 0, false)).toBe(0);
});

test("the pointer selects a row only by really moving", () => {
  const chosen: string[] = [];
  const row = underPointer(() => chosen.push("row"));

  move(10, 10);
  row.onMouseMove();
  expect(chosen).toEqual(["row"]);

  // The list scrolled under a pointer that is still: the web view sends a
  // mouse event for the row now under it, at the same place on the screen.
  move(10, 10);
  row.onMouseMove();
  expect(chosen).toEqual(["row"]);

  move(10, 14);
  row.onMouseMove();
  expect(chosen).toEqual(["row", "row"]);
});

test("the last of pointer and keyboard to act is the one driving", () => {
  move(20, 20);
  expect(pointerDriven()).toBe(true);
  window.dispatchEvent(new KeyboardEvent("keydown", { key: "ArrowDown" }));
  expect(pointerDriven()).toBe(false);
  // A still pointer does not take over from the keyboard.
  move(20, 20);
  expect(pointerDriven()).toBe(false);
  move(21, 20);
  expect(pointerDriven()).toBe(true);
});

test("a list scrolls only as far as it takes to show the row", () => {
  const view = { top: 100, bottom: 500 };
  const padding = { top: 32, bottom: 56 };
  // In view, clear of the padding: nothing to do.
  expect(shortfall({ top: 200, bottom: 238 }, view, padding)).toBe(0);
  // Under the actions that float over the end of the list.
  expect(shortfall({ top: 430, bottom: 468 }, view, padding)).toBe(24);
  // Above, under a section heading's space.
  expect(shortfall({ top: 110, bottom: 148 }, view, padding)).toBe(-22);
  // Far below: just far enough, not to the middle.
  expect(shortfall({ top: 900, bottom: 938 }, view, padding)).toBe(494);
});
