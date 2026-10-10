import fs from "node:fs";
import path from "node:path";
import { expect, test, vi } from "vitest";

import { extension } from "../testing";

vi.mock("../protocol", async () => (await import("../testing")).protocol);

import { cachePath, readJson, runAs, writeJson, type Current } from "../context";
import { Cache } from "./system";

const as = (name: string): Current => ({ extension: extension(name, {}), command: "main", launchType: "userInitiated" });
const fileOf = (name: string, namespace: string) => path.join(cachePath(name), `${namespace}.json`);

test("caches of one namespace share what they hold", () => {
  runAs(as("notes"), () => {
    const first = new Cache({ namespace: "shared" });
    const second = new Cache({ namespace: "shared" });
    first.set("a", "1");
    second.set("b", "2");
    first.set("c", "3");
    // Each used to write the file from its own copy, dropping the other's keys.
    expect(readJson(fileOf("notes", "shared"), {})).toEqual({ a: "1", b: "2", c: "3" });
    expect(second.get("a")).toBe("1");
    expect(first.has("b")).toBe(true);

    expect(second.remove("a")).toBe(true);
    expect(first.get("a")).toBeUndefined();
    expect(first.remove("a")).toBe(false);
    expect(readJson(fileOf("notes", "shared"), {})).toEqual({ b: "2", c: "3" });

    first.clear();
    expect(second.isEmpty).toBe(true);
    expect(readJson(fileOf("notes", "shared"), null)).toEqual({});
  });
});

test("a subscriber hears of changes made through another cache of the namespace", () => {
  runAs(as("notes"), () => {
    const heard: unknown[] = [];
    const first = new Cache({ namespace: "heard" });
    const unsubscribe = first.subscribe((key, data) => heard.push([key, data]));
    const second = new Cache({ namespace: "heard" });
    second.set("a", "1");
    second.remove("a");
    second.clear();
    second.clear({ notifySubscribers: false });
    unsubscribe();
    second.set("b", "2");
    expect(heard).toEqual([["a", "1"], ["a", undefined], [undefined, undefined]]);
  });
});

test("namespaces and extensions keep apart", () => {
  const one = runAs(as("notes"), () => new Cache({ namespace: "apart" }));
  const other = runAs(as("notes"), () => new Cache({ namespace: "elsewhere" }));
  const foreign = runAs(as("tasks"), () => new Cache({ namespace: "apart" }));
  one.set("k", "notes");
  expect(other.has("k")).toBe(false);
  expect(foreign.has("k")).toBe(false);
  foreign.set("k", "tasks");
  expect(one.get("k")).toBe("notes");
  expect(one.storageDirectory).toBe(cachePath("notes"));
});

test("what the other runtime process wrote is neither missed nor overwritten", () => {
  runAs(as("notes"), () => {
    const cache = new Cache({ namespace: "two-processes" });
    cache.set("mine", "1");
    // The same extension's command in the other process writes the file.
    const file = fileOf("notes", "two-processes");
    writeJson(file, { ...readJson(file, {}), theirs: "2" });
    expect(cache.get("theirs")).toBe("2");
    writeJson(file, { ...readJson(file, {}), more: "3" });
    cache.set("mine", "4");
    expect(JSON.parse(fs.readFileSync(file, "utf8"))).toEqual({ mine: "4", theirs: "2", more: "3" });
  });
});
