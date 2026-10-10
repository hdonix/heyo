import { expect, test } from "vitest";

import { FIRST, hold, locate } from "./reselect";

test("with nothing chosen the first row is selected, whichever it is", () => {
  expect(locate(["a", "b", "c"], FIRST)).toBe(0);
  expect(locate(["new", "a", "b", "c"], FIRST)).toBe(0);
  expect(locate([], FIRST)).toBe(0);
});

test("a chosen row stays selected wherever a refresh puts it", () => {
  const held = hold(["a", "b", "c"], 1);
  expect(held).toEqual({ key: "b", index: 1 });
  expect(locate(["a", "b", "c"], held)).toBe(1);
  expect(locate(["new", "a", "b", "c"], held)).toBe(2);
  expect(locate(["b", "a"], held)).toBe(0);
});

test("when the chosen row is gone, the row nearest its place is selected", () => {
  const held = hold(["a", "b", "c", "d"], 2);
  // The next row moved up into its place.
  expect(locate(["a", "b", "d"], held)).toBe(2);
  // It was the last: the row that is last now.
  expect(locate(["a", "b"], held)).toBe(1);
  expect(locate([], held)).toBe(0);
});

test("the first row, once chosen, is held like any other", () => {
  const held = hold(["a", "b"], 0);
  expect(locate(["new", "a", "b"], held)).toBe(1);
});

test("nothing can be held in an empty list", () => {
  expect(hold([], 0)).toEqual(FIRST);
  expect(hold(["a"], 5)).toEqual({ key: "a", index: 0 });
});
