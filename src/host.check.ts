// Holds the typed call to its word: a command the host does not have, an
// argument it does not take and an answer it does not give must not compile,
// in the application or in a test's fake. Compiled with the web view
// (`tsc`), never run: each line under `@ts-expect-error` is an error, and
// this file stops compiling the day one of them is not.

import type { Item } from "../shared/bindings/Item";
import { call, listen } from "./host";
import { fakeHost } from "./host.fake";

export async function calls() {
  const items: Item[] = await call("search", { query: "a" });
  // What may be null may be left out.
  await call("launch", { id: items[0].id });
  await call("launch", { id: "cmd:@heyo:ask-ai", fallbackText: "why", arguments: { city: "Oslo" } });
  await call("hide");
  await call("change_settings", { changes: { compactMode: true, launchAtLogin: false } });

  // @ts-expect-error not a command (`search` is)
  await call("serach", { query: "a" });
  // @ts-expect-error not its argument (`query` is)
  await call("search", { text: "a" });
  // @ts-expect-error an argument too many
  await call("search", { query: "a", limit: 5 });
  // @ts-expect-error an argument of the wrong kind
  await call("copy_text", { text: "a", paste: "yes" });
  // @ts-expect-error an argument is missing
  await call("copy_text", { text: "a" });
  // @ts-expect-error it takes arguments
  await call("search");
  // @ts-expect-error it takes none
  await call("hide", {});
  // @ts-expect-error it answers with items, not a sentence
  const sentence: string = await call("search", { query: "a" });
  // @ts-expect-error not one of Heyo's own hotkeys
  await call("set_hotkey", { name: "screenshot", accelerator: "" });
  // @ts-expect-error the API key has a command of its own
  await call("change_settings", { changes: { anthropicApiKey: "sk-ant" } });
  // @ts-expect-error told by the host, not a setting to change
  await call("change_settings", { changes: { nodeVersion: { Ok: "v24.0.0" } } });
  return sentence;
}

export function events() {
  listen("openItem", (id) => id.toUpperCase());
  listen("shown", () => {});

  // @ts-expect-error not an event (`settingsChanged` is)
  listen("settingsChange", () => {});
  // @ts-expect-error it carries the id of an item, not a number
  listen("openItem", (id: number) => id);
  // @ts-expect-error an installation's progress has no such field
  listen("ocrInstall", (progress) => progress.percent);
}

export function fakes() {
  const host = fakeHost({ read_clipboard: "copied", search: ({ query }) => [{ id: query, title: query, subtitle: null, icon: null, kind: "Command", favorite: false }] });
  host.answer("get_item", null);
  host.emit("openItem", "quicklink:gh");
  host.emit("shown");
  const queries: string[] = host.callsOf("search").map(({ query }) => query);

  // @ts-expect-error not what the host would answer
  fakeHost({ read_clipboard: 3 });
  // @ts-expect-error not a command
  fakeHost({ serach: [] });
  // @ts-expect-error not what the host would answer
  host.answer("get_hotkeys", { supported: "yes" });
  // @ts-expect-error not a command
  host.refuse("format_disk", "no");
  // @ts-expect-error it carries the id of an item
  host.emit("openItem");
  // @ts-expect-error it carries nothing
  host.emit("shown", 1);
  // @ts-expect-error `search` is not called with a title
  host.callsOf("search").map(({ title }) => title);
  return queries;
}
