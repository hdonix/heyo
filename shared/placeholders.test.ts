import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { expand, formatDate, needsArgument } from "./placeholders";

const clipboard = (text: string) => ({ readClipboard: async () => text });
const nothing = clipboard("");

// 9 March 2026, 14:05:07, a Monday, in whatever zone the tests run in.
const NOW = new Date(2026, 2, 9, 14, 5, 7);

beforeEach(() => {
  vi.useFakeTimers();
  vi.setSystemTime(NOW);
});
afterEach(() => vi.useRealTimers());

describe("what the user typed", () => {
  it("goes where {query} and {argument} are", async () => {
    expect(await expand("https://github.com/search?q={query}", { ...nothing, query: "heyo" })).toBe("https://github.com/search?q=heyo");
    expect(await expand("Dear {argument},", { ...nothing, query: "Ada" })).toBe("Dear Ada,");
    expect(await expand("{query} and {query}", { ...nothing, query: "x" })).toBe("x and x");
    expect(await expand('{argument name="who"}!', { ...nothing, query: "you" })).toBe("you!");
  });

  it("falls back to the default, then to nothing", async () => {
    expect(await expand('Hi {argument name="who" default="there"}', nothing)).toBe("Hi there");
    expect(await expand('Hi {argument default="there"}', { ...nothing, query: "Ada" })).toBe("Hi Ada");
    expect(await expand('{argument default=""}|', nothing)).toBe("|");
    expect(await expand("[{query}]", nothing)).toBe("[]");
  });

  it("is encoded for a link, and only what is inserted", async () => {
    const context = { ...clipboard("a/b c"), query: "c++ & more", encode: true };
    expect(await expand("https://example.com/s?q={query}&x=1", context)).toBe("https://example.com/s?q=c%2B%2B%20%26%20more&x=1");
    expect(await expand("https://example.com/{clipboard}", context)).toBe("https://example.com/a%2Fb%20c");
    expect(await expand('https://example.com/?q={argument default="a b"}', { ...nothing, encode: true })).toBe("https://example.com/?q=a%20b");
    expect(await expand("{query}", { ...nothing, query: "a b" })).toBe("a b");
  });

  it("is asked for when the template has a place for it", () => {
    expect(needsArgument("https://github.com/search?q={query}")).toBe(true);
    expect(needsArgument('{argument name="who" default="you"}')).toBe(true);
    expect(needsArgument("{clipboard} on {date}")).toBe(false);
    expect(needsArgument("plain text")).toBe(false);
    expect(needsArgument("{ query }")).toBe(false);
    // Asking twice gives the same answer (the pattern keeps no position).
    expect(needsArgument("{query}")).toBe(true);
    expect(needsArgument("{query}")).toBe(true);
  });
});

describe("the clipboard", () => {
  it("goes where {clipboard} is", async () => {
    expect(await expand("> {clipboard}", clipboard("copied text"))).toBe("> copied text");
    expect(await expand("{clipboard}{clipboard}", clipboard("ab"))).toBe("abab");
  });

  it("is only read when asked for", async () => {
    const readClipboard = vi.fn(async () => "secret");
    expect(await expand("Hello {query}", { readClipboard, query: "x" })).toBe("Hello x");
    expect(readClipboard).not.toHaveBeenCalled();
    await expand("{clipboard} {clipboard}", { readClipboard });
    expect(readClipboard).toHaveBeenCalledTimes(1);
  });

  it("is text, not a template: placeholders in it stay as they are", async () => {
    expect(await expand("{clipboard}", { ...clipboard("{query} {date} {clipboard}"), query: "x" })).toBe("{query} {date} {clipboard}");
    expect(await expand("{query}", { ...nothing, query: "{clipboard}" })).toBe("{clipboard}");
    // Nor do "$&" and the like mean anything.
    expect(await expand("a {clipboard} b", clipboard("$& $1 $$"))).toBe("a $& $1 $$ b");
  });

  it("counts as empty when it cannot be read", async () => {
    const context = { readClipboard: async () => Promise.reject(new Error("no clipboard")), query: "x" };
    expect(await expand("[{clipboard}] {query}", context)).toBe("[] x");
  });
});

describe("the date and time", () => {
  it("are now, as the system writes them", async () => {
    expect(await expand("{date}", nothing)).toBe(NOW.toLocaleDateString());
    expect(await expand("{time}", nothing)).toBe(NOW.toLocaleTimeString());
    expect(await expand("{datetime}", nothing)).toBe(NOW.toLocaleString());
    expect(await expand("{day}", nothing)).toBe(NOW.toLocaleDateString(undefined, { weekday: "long" }));
  });

  it("follow a format when one is given", async () => {
    expect(await expand('{date format="yyyy-MM-dd"}', nothing)).toBe("2026-03-09");
    expect(await expand('{date format="yyyy-MM-dd HH:mm"}', nothing)).toBe("2026-03-09 14:05");
    expect(await expand('{time format="HH:mm:ss"}', nothing)).toBe("14:05:07");
    expect(await expand('{datetime format="d/M/yy h:mm a"}', nothing)).toBe("9/3/26 2:05 PM");
    expect(await expand('notes-{date format="yyyyMMdd"}.md', nothing)).toBe("notes-20260309.md");
  });

  it("format every token of the pattern", () => {
    const morning = new Date(2026, 0, 5, 0, 3, 9);
    expect(formatDate(morning, "yyyy yy MM M dd d")).toBe("2026 26 01 1 05 5");
    expect(formatDate(morning, "HH H hh h mm ss a")).toBe("00 0 12 12 03 09 AM");
    expect(formatDate(new Date(2026, 11, 25, 12, 0, 0), "hh a")).toBe("12 PM");
    expect(formatDate(new Date(2026, 11, 25, 23, 59, 0), "h:mm a")).toBe("11:59 PM");
    expect(formatDate(morning, "m s")).toBe("3 9");
    expect(formatDate(NOW, "MMMM MMM")).toBe(`${NOW.toLocaleString(undefined, { month: "long" })} ${NOW.toLocaleString(undefined, { month: "short" })}`);
    expect(formatDate(NOW, "EEEE EEE")).toBe(`${NOW.toLocaleString(undefined, { weekday: "long" })} ${NOW.toLocaleString(undefined, { weekday: "short" })}`);
  });

  it("keep quoted words as words", () => {
    // Without the quotes "at" would read as AM/PM and "day" as a day.
    expect(formatDate(NOW, "yyyy-MM-dd 'at' HH:mm")).toBe("2026-03-09 at 14:05");
    expect(formatDate(NOW, "'day' d 'of the month'")).toBe("day 9 of the month");
    expect(formatDate(NOW, "h 'o''clock'")).toBe("2 o'clock");
    expect(formatDate(NOW, "HH''mm")).toBe("14'05");
  });

  it("do not format what a month's or a day's name happens to spell", () => {
    // "March" has an "a" and an "h" in it; "Monday" a "d" and an "a".
    const written = formatDate(NOW, "EEEE d MMMM");
    expect(written).toBe(`${NOW.toLocaleString(undefined, { weekday: "long" })} 9 ${NOW.toLocaleString(undefined, { month: "long" })}`);
  });
});

describe("the rest", () => {
  it("{uuid} is a fresh UUID each time", async () => {
    const [a, b] = (await expand("{uuid} {uuid}", nothing)).split(" ");
    expect(a).toMatch(/^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/);
    expect(b).not.toBe(a);
  });

  it("{cursor} leaves nothing behind", async () => {
    expect(await expand("Hello {cursor}world", nothing)).toBe("Hello world");
    expect(await expand("https://example.com/{cursor}", { ...nothing, encode: true })).toBe("https://example.com/");
  });

  it("several placeholders are each replaced where they stand", async () => {
    const out = await expand('{argument name="n"}: {clipboard} ({date format="yyyy"}){cursor}', { ...clipboard("note"), query: "Re" });
    expect(out).toBe("Re: note (2026)");
  });

  it("what is not a placeholder is left as written", async () => {
    for (const text of ["no placeholders", "{unknown}", "{Query}", "{ query }", "{query", "query}", '{"json": {"date": 1}}', "function () { return 1; }", "{}", "{date format=yyyy}", "100% {sure}"]) {
      expect(await expand(text, { ...clipboard("x"), query: "x" }), text).toBe(text);
    }
    expect(await expand("", nothing)).toBe("");
  });
});
