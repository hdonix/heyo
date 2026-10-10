import { expect, test } from "vitest";

import type { Item } from "../shared/bindings/Item";
import { afterAlias, argumentsOf, filled, missing, tabStop, type Argument } from "./arguments";

const item = (rest: Partial<Item>): Item => ({ id: "x", title: "X", subtitle: null, icon: null, kind: "Command", favorite: false, ...rest });

const city: Argument = { name: "city", type: "text", placeholder: "City", required: true };
const key: Argument = { name: "key", type: "password", required: false };
const unit: Argument = {
  name: "unit",
  type: "dropdown",
  required: true,
  data: [
    { title: "Celsius", value: "c" },
    { title: "Fahrenheit", value: "f" },
  ],
};

test("a command asks for what its manifest declares", () => {
  expect(argumentsOf(item({ arguments: [city, unit] }))).toEqual([city, unit]);
  expect(argumentsOf(item({}))).toEqual([]);
  expect(argumentsOf(item({ kind: "Application" }))).toEqual([]);
});

test("a quicklink or snippet with a placeholder asks for one text", () => {
  const link = item({ kind: "Quicklink", link: "https://github.com/search?q={query}" });
  expect(argumentsOf(link)).toEqual([{ name: "query", type: "text", placeholder: "Query", required: true }]);
  const snippet = item({ kind: "Snippet", text: "Dear {argument name=\"who\"}, it is {date}." });
  expect(argumentsOf(snippet)).toMatchObject([{ name: "query", required: true }]);
  // Nothing to ask where nothing is to be filled in.
  expect(argumentsOf(item({ kind: "Quicklink", link: "https://example.com/{date}" }))).toEqual([]);
  expect(argumentsOf(item({ kind: "Snippet", text: "{clipboard}" }))).toEqual([]);
});

test("a placeholder with a default may stay empty", () => {
  const link = item({ kind: "Quicklink", link: 'https://example.com/{query default="home"}' });
  expect(argumentsOf(link)).toMatchObject([{ required: false }]);
  // One without a default among them still needs the text.
  const both = item({ kind: "Snippet", text: '{argument default="a"} and {argument}' });
  expect(argumentsOf(both)).toMatchObject([{ required: true }]);
});

test("the text after an alias is told from the alias", () => {
  expect(afterAlias("gh", "gh heyo launcher")).toBe("heyo launcher");
  expect(afterAlias("GH", "  gh   heyo ")).toBe("heyo");
  expect(afterAlias("gh", "gh")).toBeNull();
  expect(afterAlias("gh", "ghost story")).toBeNull();
  expect(afterAlias(undefined, "gh heyo")).toBeNull();
});

test("every field has a value: typed, else after the alias for the first", () => {
  expect(filled([city, key], {}, null)).toEqual({ city: "", key: "" });
  expect(filled([city, key], {}, "oslo")).toEqual({ city: "oslo", key: "" });
  expect(filled([city, key], { city: "Bergen", key: "s3" }, "oslo")).toEqual({ city: "Bergen", key: "s3" });
  // Cleared by hand stays cleared.
  expect(filled([city], { city: "" }, "oslo")).toEqual({ city: "" });
  // Nothing that was not asked for.
  expect(filled([city], { city: "a", other: "b" }, null)).toEqual({ city: "a" });
});

test("a dropdown holds one of its choices or nothing", () => {
  expect(filled([unit], { unit: "f" }, null)).toEqual({ unit: "f" });
  expect(filled([unit], { unit: "kelvin" }, null)).toEqual({ unit: "" });
  // After an alias, a choice may be named by its title.
  expect(filled([unit], {}, "celsius")).toEqual({ unit: "c" });
  expect(filled([unit], {}, "warm")).toEqual({ unit: "" });
});

test("required fields left empty keep an item from opening", () => {
  expect(missing([city, key, unit], { city: "", key: "", unit: "" })).toEqual(["city", "unit"]);
  expect(missing([city, key, unit], { city: "   ", key: "", unit: "c" })).toEqual(["city"]);
  expect(missing([city, key, unit], { city: "Oslo", key: "", unit: "c" })).toEqual([]);
  expect(missing([], {})).toEqual([]);
});

test("Tab goes round the search field and the argument fields", () => {
  expect(tabStop(0, false, 2)).toBe(1);
  expect(tabStop(1, false, 2)).toBe(2);
  expect(tabStop(2, false, 2)).toBe(0);
  expect(tabStop(0, true, 2)).toBe(2);
  expect(tabStop(1, true, 1)).toBe(0);
});
