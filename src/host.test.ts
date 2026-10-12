// The window's side of the host, with the host played: calls, events, and
// where a failure goes.
import { afterEach, expect, test, vi } from "vitest";

import { call, ignored, listen, reported, reportFailure, reportFailuresIn, type Failure } from "./host";
import { fakeHost } from "./host.fake";

afterEach(() => void vi.restoreAllMocks());

test("a call reaches the host with its arguments and brings back the answer", async () => {
  const host = fakeHost({ read_clipboard: "copied", search: ({ query }) => [{ id: query, title: query, subtitle: null, icon: null, kind: "Command", favorite: false }] });
  expect(await call("read_clipboard")).toBe("copied");
  expect((await call("search", { query: "timer" })).map((item) => item.id)).toEqual(["timer"]);
  // One that answers nothing needs no telling.
  expect(await call("copy_text", { text: "a", paste: false })).toBeNull();
  expect(host.calls).toEqual([["read_clipboard"], ["search", { query: "timer" }], ["copy_text", { text: "a", paste: false }]]);
  expect(host.callsOf("copy_text")).toEqual([{ text: "a", paste: false }]);
  host.forget();
  expect(host.calls).toEqual([]);
});

test("a command the test did not answer fails, and so does one the host refuses", async () => {
  const host = fakeHost();
  await expect(call("read_clipboard")).rejects.toThrow("the fake host was not told what read_clipboard answers");
  host.answer("read_clipboard", "copied");
  expect(await call("read_clipboard")).toBe("copied");
  // In the host's own words, as Tauri rejects with them.
  host.refuse("copy_text", "the clipboard is busy");
  await expect(call("copy_text", { text: "a", paste: true })).rejects.toBe("the clipboard is busy");
  expect(host.calls.map(([command]) => command)).toEqual(["read_clipboard", "read_clipboard", "copy_text"]);
});

test("a failure is shown where the window says, and logged where it says nothing", async () => {
  const host = fakeHost();
  host.refuse("open_settings", "no display");
  const log = vi.spyOn(console, "error").mockImplementation(() => {});
  const seen: Failure[] = [];
  const stop = reportFailuresIn((failure) => seen.push(failure));

  await call("open_settings").catch(reported("Could not open settings"));
  await call("open_settings").catch(reported());
  reportFailure("The download failed");
  // Meant to pass unnoticed.
  await call("open_settings").catch(ignored);
  expect(seen).toEqual([{ title: "Could not open settings", message: "no display" }, { title: undefined, message: "no display" }, { title: undefined, message: "The download failed" }]);
  expect(log).not.toHaveBeenCalled();

  // The window is gone, or never had a place for it.
  stop();
  await call("open_settings").catch(reported("Could not open settings"));
  expect(seen).toHaveLength(3);
  expect(log.mock.calls).toEqual([["Could not open settings: no display"]]);
});

test("an event is heard until listening stops", async () => {
  const host = fakeHost();
  const opened: string[] = [];
  let shown = 0;
  const stop = listen("openItem", (id) => opened.push(id));
  listen("shown", () => shown++);
  // Listening is in place once the bridge has answered.
  await Promise.resolve();
  host.emit("openItem", "quicklink:gh");
  host.emit("shown");
  expect([opened, shown]).toEqual([["quicklink:gh"], 1]);

  stop();
  await Promise.resolve();
  await Promise.resolve();
  host.emit("openItem", "snippet:sig");
  host.emit("shown");
  expect([opened, shown]).toEqual([["quicklink:gh"], 2]);
});
