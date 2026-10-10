import { expect, test } from "vitest";

import { Memories } from "./memory";

test("a view come back to is as it was left", () => {
  const memories = new Memories();
  const list = memories.of(0);
  list.query = "milk";
  list.selectedId = 42;
  list.scrollTop = 310;
  // A view is pushed, used, and popped.
  memories.of(1).query = "other";
  expect(memories.of(0)).toBe(list);
  expect(memories.of(0)).toMatchObject({ query: "milk", selectedId: 42, scrollTop: 310 });
});

test("a view that was popped is forgotten", () => {
  const memories = new Memories();
  memories.of(0);
  memories.of(1).query = "pushed";
  memories.of(0);
  // The next view pushed never reuses a number, but nothing is kept either way.
  expect(memories.of(1).query).toBe("");
  expect(memories.of(2).query).toBe("");
});

test("a new view starts empty", () => {
  expect(new Memories().of(3)).toEqual({ query: "", selectedId: null, scrollTop: 0, picked: {} });
});
