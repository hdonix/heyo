// What the rows of the root search are and do, with the palette around them
// played by one that writes down what is asked of it.
import { expect, test } from "vitest";

import type { ActivityView } from "../shared/bindings/ActivityView";
import type { Item } from "../shared/bindings/Item";
import type { Calculation } from "./calculator";
import type { Command } from "./host";
import { fakeHost } from "./host.fake";
import { askAi, describe, open, rowsFor, type Context, type Description, type Found, type Palette, type Row } from "./rows";

const item = (id: string, rest: Partial<Item> = {}): Item => ({ id, title: id, subtitle: null, icon: "box-16", kind: "Command", favorite: false, ...rest });
const files = item("app:files", { title: "Files", kind: "Application" });
const notes = item("cmd:/ext:notes", { title: "Notes" });
const weather = item("cmd:/ext:weather", {
  title: "Weather",
  arguments: [
    { name: "city", type: "text", placeholder: "City", required: true },
    { name: "days", type: "text", placeholder: "Days", required: false },
    { name: "key", type: "password", placeholder: "API key", required: true },
  ],
});
const github = item("quicklink:gh", { title: "GitHub", kind: "Quicklink", link: "https://github.com/search?q={query}", alias: "gh" });
const docs = item("quicklink:docs", { title: "Docs", kind: "Quicklink", link: "https://example.com/docs" });
const greeting = item("snippet:hi", { title: "Greeting", kind: "Snippet", text: "Hello {argument}, it is {clipboard}" });
const signature = item("snippet:sig", { title: "Signature", kind: "Snippet", text: "Kind regards" });

const activity = (key: string, rest: Partial<ActivityView> = {}): ActivityView => ({
  key,
  extension: "@heyo",
  extensionTitle: "Heyo",
  id: key.split("/")[1],
  name: "Timer",
  title: "25 min",
  compact: "24:59",
  buttons: [
    { title: "Pause", icon: "pause-16", destructive: false },
    { title: "+1 min", destructive: false },
  ],
  hasAction: false,
  dismissible: true,
  needsAttention: false,
  stale: false,
  size: "compact",
  live: true,
  ended: false,
  ...rest,
});
const timer = activity("@heyo/timer-1");
const sum: Calculation = { question: "6 × 7", answer: "42", value: "42", label: "Result" };

const nothing: Found = { activities: [], calc: null, items: [], quicklinks: [] };
const rows = (query: string, found: Partial<Found>) => rowsFor(query, { ...nothing, ...found });
const rowOf = (found: Item) => rows("", { items: [found] })[0];
const laidOut = (list: Row[]) => list.map((row) => [row.key, row.heading]);

/// A palette that writes down what is asked of it, in one list with what
/// is asked of the host. The host answers with `answers` and fails the
/// commands in `failing`; a prompt is answered with the next of `replies`,
/// and cancelled when there is none.
function palette(play: { answers?: Parameters<typeof fakeHost>[0]; failing?: Command[]; replies?: string[] } = {}) {
  const host = fakeHost(play.answers);
  for (const command of play.failing ?? []) host.refuse(command, `${command} failed`);
  const asked = host.calls as unknown[][];
  const replies = [...(play.replies ?? [])];
  const note = (what: string) => (...with_: unknown[]) => void asked.push([what, ...with_.filter((w) => w !== undefined)]);
  const fake: Palette = {
    showError: note("showError"),
    showSuccess: note("showSuccess"),
    openSettings: note("openSettings"),
    ask: async (question) => {
      asked.push(["ask", question]);
      return replies.shift() ?? null;
    },
    askHotkey: note("askHotkey"),
    fillIn: note("fillIn"),
    argumentsSent: note("argumentsSent"),
  };
  return { fake, asked };
}

const typing = (rest: Partial<Context> = {}): Context => ({ query: "", typed: null, folded: false, ...rest });
/// What was asked once everything an action started has come to rest.
const settled = async (asked: unknown[][]) => {
  await new Promise((resolve) => setTimeout(resolve));
  return asked;
};

const shortcutOf = (shortcut: unknown) => {
  const { modifiers, key } = shortcut as { modifiers: string[]; key: string };
  return [...modifiers, key].join("+");
};
/// The actions as the panel lists them: by title, with the shortcut of those that have one.
const listed = (about: Description) => about.actions.map((group) => group.actions.map((a) => (a.shortcut ? `${a.title} (${shortcutOf(a.shortcut)})` : a.title)));
const action = (about: Description, title: string) => about.actions.flatMap((group) => group.actions).find((a) => a.title === title)!;

/// Runs a row's action (its primary one unless named) and says what was asked.
async function doing(row: Row, how: { title?: string; context?: Partial<Context>; play?: Parameters<typeof palette>[0] } = {}) {
  const { fake, asked } = palette(how.play);
  const about = describe(row, typing(how.context), fake);
  (how.title ? action(about, how.title) : about.primary!).run();
  return settled(asked);
}

test("the rows come in order: what is in progress, the calculator, what was found, what else the query can do", () => {
  const list = rows(" timer ", { activities: [timer], calc: sum, items: [files, notes], quicklinks: [github, docs] });
  expect(laidOut(list)).toEqual([
    ["activity-@heyo/timer-1", "In Progress"],
    ["calc", "Calculator"],
    ["app:files", "Results"],
    ["cmd:/ext:notes", undefined],
    ["fallback-google", "Use “timer” with…"],
    ["fallback-ask-ai", undefined],
    ["fallback-files", undefined],
    // Of the quicklinks, the one that takes a query.
    ["fallback-quicklink:gh", undefined],
  ]);
  expect(list.map((row) => row.type)).toEqual(["activity", "calc", "item", "item", "fallback", "fallback", "fallback", "fallback"]);
});

test("what was found has a heading only under something else, or where the host gave it sections", () => {
  expect(laidOut(rows("no", { items: [files, notes] })).slice(0, 2)).toEqual([
    ["app:files", undefined],
    ["cmd:/ext:notes", undefined],
  ]);
  const home = [item("a", { section: "Favorites" }), item("b", { section: "Favorites" }), item("c", { section: "Suggestions" })];
  expect(laidOut(rows("", { activities: [timer, activity("@heyo/timer-2")], items: home }))).toEqual([
    ["activity-@heyo/timer-1", "In Progress"],
    ["activity-@heyo/timer-2", undefined],
    ["a", "Favorites"],
    ["b", undefined],
    ["c", "Suggestions"],
  ]);
});

test("with nothing typed there is nothing to use the query with", () => {
  expect(rows("  ", { items: [files], quicklinks: [github] }).map((row) => row.key)).toEqual(["app:files"]);
});

test("with a query, an activity stays if the query names it", () => {
  const shown = (query: string) => rows(query, { activities: [timer] }).some((row) => row.type === "activity");
  expect(shown("")).toBe(true);
  expect(shown("25")).toBe(true);
  expect(shown("TIMER")).toBe(true);
  expect(shown("heyo")).toBe(true);
  expect(shown("weather")).toBe(false);
});

test("an activity does what it does itself, else its first button; and can be ended", async () => {
  const [row] = rows("", { activities: [timer] });
  const about = describe(row, typing(), palette().fake);
  expect(about.title).toBe("25 min");
  expect(listed(about)).toEqual([["Pause", "+1 min"], ["End Activity"]]);
  expect(about.primary?.title).toBe("Pause");
  expect(action(about, "+1 min").icon).toBe("circle-16");

  expect(await doing(row)).toEqual([["activity_action", { key: "@heyo/timer-1", button: 0 }]]);
  expect(await doing(row, { title: "+1 min" })).toEqual([["activity_action", { key: "@heyo/timer-1", button: 1 }]]);
  expect(await doing(row, { title: "End Activity" })).toEqual([["activity_dismiss", { key: "@heyo/timer-1" }]]);
  expect(await doing(row, { play: { failing: ["activity_action"] } })).toEqual([
    ["activity_action", { key: "@heyo/timer-1", button: 0 }],
    ["showError", "Could not act on Timer", "activity_action failed"],
  ]);

  const [build] = rows("", { activities: [activity("ci/build", { name: "Build", title: "Building", hasAction: true, dismissible: false })] });
  expect(listed(describe(build, typing(), palette().fake))).toEqual([["Open", "Pause", "+1 min"]]);
  expect(await doing(build)).toEqual([["activity_action", { key: "ci/build", button: null }]]);
});

test("an activity with nothing to do has no actions, and no primary one", () => {
  const [row] = rows("", { activities: [activity("ci/wait", { buttons: [], dismissible: false })] });
  const about = describe(row, typing(), palette().fake);
  expect(about.actions).toEqual([]);
  expect(about.primary).toBeUndefined();
});

test("the calculator copies its answer, pastes it, or copies the question with it", async () => {
  const [row] = rows("6*7", { calc: sum });
  const about = describe(row, typing({ query: "6*7" }), palette().fake);
  expect(about.title).toBe("Calculator");
  expect(listed(about)).toEqual([["Copy Answer", "Paste Answer (ctrl+return)", "Copy Question and Answer (ctrl+shift+c)"]]);
  expect(await doing(row)).toEqual([["copy_text", { text: "42", paste: false }]]);
  expect(await doing(row, { title: "Paste Answer" })).toEqual([["copy_text", { text: "42", paste: true }]]);
  expect(await doing(row, { title: "Copy Question and Answer" })).toEqual([["copy_text", { text: "6 × 7 = 42", paste: false }]]);
});

test("a fallback runs its search with the query", async () => {
  const context = { query: "  cats & dogs " };
  const [google, ai, search, link] = rows(context.query, { quicklinks: [github] });
  const about = describe(google, typing(context), palette().fake);
  expect(about.title).toBe("Search Google");
  expect(listed(about)).toEqual([["Search Google"]]);

  expect(await doing(google, { context })).toEqual([["open_target", { target: "https://www.google.com/search?q=cats%20%26%20dogs" }]]);
  expect(await doing(ai, { context })).toEqual([["launch", { id: "cmd:@heyo:ask-ai", fallbackText: "cats & dogs" }]]);
  expect(await doing(search, { context })).toEqual([["launch", { id: "cmd:@heyo:search-files", fallbackText: "cats & dogs" }]]);
  expect(describe(link, typing(context), palette().fake).title).toBe("GitHub");
  expect(await doing(link, { context })).toEqual([["open_target", { target: "https://github.com/search?q=cats%20%26%20dogs" }]]);

  expect((await doing(google, { context, play: { failing: ["open_target"] } })).pop()).toEqual(["showError", "Could not open", "open_target failed"]);
  expect((await doing(link, { context, play: { failing: ["open_target"] } })).pop()).toEqual(["showError", "Could not open", "open_target failed"]);
  expect((await doing(ai, { context, play: { failing: ["launch"] } })).pop()).toEqual(["showError", "Could not ask AI", "launch failed"]);
});

test("Ask AI hands the question to the model", async () => {
  const { fake, asked } = palette();
  askAi("why is the sky blue", fake);
  expect(await settled(asked)).toEqual([["launch", { id: "cmd:@heyo:ask-ai", fallbackText: "why is the sky blue" }]]);
});

test("an application is launched, and its use counted", async () => {
  const about = describe(rowOf(files), typing(), palette().fake);
  expect(about.title).toBe("Files");
  expect(about.fields).toEqual([]);
  expect(listed(about)).toEqual([
    ["Open Application"],
    ["Add to Favorites (ctrl+shift+f)", "Set Alias (ctrl+shift+a)", "Set Hotkey (ctrl+shift+h)", "Reset Ranking"],
  ]);
  expect(await doing(rowOf(files))).toEqual([
    ["record_usage", { id: "app:files" }],
    ["launch", { id: "app:files", arguments: undefined }],
  ]);
  expect((await doing(rowOf(files), { play: { failing: ["launch"] } })).pop()).toEqual(["showError", "Could not open Files", "launch failed"]);
});

test("a command, a quicklink and a snippet each say what opening them is, and lead to the settings", () => {
  const titles = (found: Item) => listed(describe(rowOf(found), typing(), palette().fake));
  const manage = ["Add to Favorites (ctrl+shift+f)", "Set Alias (ctrl+shift+a)", "Set Hotkey (ctrl+shift+h)", "Reset Ranking", "Open Settings (ctrl+,)"];
  expect(titles(notes)).toEqual([["Open Command"], manage]);
  expect(titles(docs)).toEqual([["Open Quicklink", "Copy Link (ctrl+shift+c)"], manage]);
  expect(titles(signature)).toEqual([["Paste Snippet", "Copy Snippet (ctrl+return)"], manage]);
});

test("what an item already has changes what can be done about it", () => {
  const settled = item("cmd:/ext:todo", { title: "Todo", favorite: true, section: "Favorites", alias: "td", hotkey: "<Control>t" });
  expect(listed(describe(rowOf(settled), typing(), palette().fake))[1]).toEqual([
    "Remove from Favorites (ctrl+shift+f)",
    "Move Up in Favorites (ctrl+alt+arrowUp)",
    "Move Down in Favorites (ctrl+alt+arrowDown)",
    "Edit Alias (ctrl+shift+a)",
    "Change Hotkey (ctrl+shift+h)",
    "Remove Hotkey",
    "Reset Ranking",
    "Open Settings (ctrl+,)",
  ]);
  // A favorite found by a query is not in its place among the favorites.
  const found = listed(describe(rowOf(item("cmd:/ext:todo", { favorite: true })), typing(), palette().fake))[1];
  expect(found[0]).toBe("Remove from Favorites (ctrl+shift+f)");
  expect(found).not.toContain("Move Up in Favorites (ctrl+alt+arrowUp)");
});

test("favorites are added, removed and moved", async () => {
  const favorite = rowOf(item("cmd:/ext:todo", { favorite: true, section: "Favorites" }));
  expect(await doing(rowOf(notes), { title: "Add to Favorites" })).toEqual([["set_favorite", { id: "cmd:/ext:notes", favorite: true }]]);
  expect(await doing(favorite, { title: "Remove from Favorites" })).toEqual([["set_favorite", { id: "cmd:/ext:todo", favorite: false }]]);
  expect(await doing(favorite, { title: "Move Up in Favorites" })).toEqual([["move_favorite", { id: "cmd:/ext:todo", offset: -1 }]]);
  expect(await doing(favorite, { title: "Move Down in Favorites" })).toEqual([["move_favorite", { id: "cmd:/ext:todo", offset: 1 }]]);
});

test("an alias is asked for, starting from the one there is, and set unless the prompt is left", async () => {
  expect(await doing(rowOf(notes), { title: "Set Alias", play: { replies: ["nt"] } })).toEqual([
    ["ask", { title: "Alias for Notes", placeholder: "e.g. cb — leave empty to remove", initial: "" }],
    ["set_alias", { id: "cmd:/ext:notes", alias: "nt" }],
  ]);
  // Emptied, it is removed: that is the host's to do.
  expect(await doing(rowOf(github), { title: "Edit Alias", play: { replies: [""] } })).toEqual([
    ["ask", { title: "Alias for GitHub", placeholder: "e.g. cb — leave empty to remove", initial: "gh" }],
    ["set_alias", { id: "quicklink:gh", alias: "" }],
  ]);
  expect(await doing(rowOf(notes), { title: "Set Alias" })).toHaveLength(1);
  expect((await doing(rowOf(notes), { title: "Set Alias", play: { replies: ["nt"], failing: ["set_alias"] } })).pop()).toEqual([
    "showError",
    "Could not set alias",
    "set_alias failed",
  ]);
});

test("a hotkey is asked for by the palette, and removed here", async () => {
  const bound = item("cmd:/ext:todo", { title: "Todo", hotkey: "<Control>t" });
  expect(await doing(rowOf(notes), { title: "Set Hotkey" })).toEqual([["askHotkey", notes]]);
  expect(await doing(rowOf(bound), { title: "Change Hotkey" })).toEqual([["askHotkey", bound]]);
  expect(await doing(rowOf(bound), { title: "Remove Hotkey" })).toEqual([["set_item_hotkey", { id: "cmd:/ext:todo", title: "Todo", accelerator: "" }]]);
  expect((await doing(rowOf(bound), { title: "Remove Hotkey", play: { failing: ["set_item_hotkey"] } })).pop()).toEqual([
    "showError",
    "Could not remove hotkey",
    "set_item_hotkey failed",
  ]);
});

test("the ranking is reset, and said to be", async () => {
  expect(await doing(rowOf(files), { title: "Reset Ranking" })).toEqual([
    ["reset_ranking", { id: "app:files" }],
    ["showSuccess", "Reset ranking of Files"],
  ]);
});

test("the settings open from an item's actions, and as the item that they are", async () => {
  expect(await doing(rowOf(notes), { title: "Open Settings" })).toEqual([["openSettings"]]);
  expect(await doing(rowOf(item("heyo:settings", { title: "Settings" })))).toEqual([["record_usage", { id: "heyo:settings" }], ["openSettings"]]);
});

test("a command's arguments are its fields, and it opens with what they hold", async () => {
  const typed = { key: weather.id, values: { city: "Oslo", key: "s3cret" } };
  const about = describe(rowOf(weather), typing({ typed }), palette().fake);
  expect(about.fields.map((field) => field.name)).toEqual(["city", "days", "key"]);
  expect(about.values).toEqual({ city: "Oslo", days: "", key: "s3cret" });
  expect(await doing(rowOf(weather), { context: { typed } })).toEqual([
    ["argumentsSent"],
    ["record_usage", { id: weather.id }],
    ["launch", { id: weather.id, arguments: { city: "Oslo", days: "", key: "s3cret" } }],
  ]);
  // What was typed for another row is not this one's.
  expect(describe(rowOf(weather), typing({ typed: { ...typed, key: "cmd:/ext:other" } }), palette().fake).values).toEqual({ city: "", days: "", key: "" });
});

test("a required argument left empty keeps the row from opening, and its field is asked for", async () => {
  expect(await doing(rowOf(weather))).toEqual([["fillIn", weather.id, 0]]);
  expect(await doing(rowOf(weather), { context: { typed: { key: weather.id, values: { city: "Oslo" } } } })).toEqual([["fillIn", weather.id, 2]]);
  expect(await doing(rowOf(github))).toEqual([["fillIn", "quicklink:gh", 0]]);
});

test("a quicklink opens with its query, typed in its field or after its alias", async () => {
  const opened = [
    ["argumentsSent"],
    ["record_usage", { id: "quicklink:gh" }],
    ["open_target", { target: "https://github.com/search?q=heyo%20launcher" }],
  ];
  expect(await doing(rowOf(github), { context: { typed: { key: "quicklink:gh", values: { query: "heyo launcher" } } } })).toEqual(opened);
  const context = { query: "gh heyo launcher" };
  expect(describe(rowOf(github), typing(context), palette().fake).values).toEqual({ query: "heyo launcher" });
  expect(await doing(rowOf(github), { context })).toEqual(opened);
  // One that asks for nothing just opens.
  expect(await doing(rowOf(docs))).toEqual([["record_usage", { id: "quicklink:docs" }], ["open_target", { target: "https://example.com/docs" }]]);
  expect(await doing(rowOf(github), { title: "Copy Link" })).toEqual([["copy_text", { text: "https://github.com/search?q={query}", paste: false }]]);
});

test("a snippet is pasted, or copied, with its argument and the clipboard filled in", async () => {
  const context = { typed: { key: "snippet:hi", values: { query: "Ada" } } };
  const play = { answers: { read_clipboard: "noon" } };
  expect(await doing(rowOf(signature))).toEqual([["record_usage", { id: "snippet:sig" }], ["copy_text", { text: "Kind regards", paste: true }]]);
  expect(await doing(rowOf(greeting), { context, play })).toEqual([
    ["argumentsSent"],
    ["record_usage", { id: "snippet:hi" }],
    ["read_clipboard"],
    ["copy_text", { text: "Hello Ada, it is noon", paste: true }],
  ]);
  expect(await doing(rowOf(greeting), { title: "Copy Snippet", context, play })).toEqual([["read_clipboard"], ["copy_text", { text: "Hello Ada, it is noon", paste: false }]]);
  // Copied without its argument, it asks for it.
  expect(await doing(rowOf(greeting), { title: "Copy Snippet", play: { ...play, replies: ["Bo"] } })).toEqual([
    ["show_window"],
    ["ask", { title: "Greeting", placeholder: "Text for {argument}" }],
    ["read_clipboard"],
    ["copy_text", { text: "Hello Bo, it is noon", paste: false }],
  ]);
});

test("with the list folded away there are no fields, and what needs an argument asks for it", async () => {
  const context = { folded: true };
  expect(describe(rowOf(github), typing(context), palette().fake).fields).toEqual([]);
  expect(await doing(rowOf(github), { context, play: { replies: ["heyo"] } })).toEqual([
    ["record_usage", { id: "quicklink:gh" }],
    ["show_window"],
    ["ask", { title: "GitHub", placeholder: "Query" }],
    ["open_target", { target: "https://github.com/search?q=heyo" }],
  ]);
  // A command's arguments are then the command's to ask for.
  expect(await doing(rowOf(weather), { context })).toEqual([["record_usage", { id: weather.id }], ["launch", { id: weather.id, arguments: undefined }]]);
});

test("an item opened from outside the list is not counted again, and asks for what it needs", async () => {
  const opening = async (found: Item, play?: Parameters<typeof palette>[0]) => {
    const { fake, asked } = palette(play);
    await open(found, fake);
    return settled(asked);
  };
  expect(await opening(notes)).toEqual([["launch", { id: "cmd:/ext:notes", arguments: undefined }]]);
  expect(await opening(github, { replies: ["heyo"] })).toEqual([
    ["show_window"],
    ["ask", { title: "GitHub", placeholder: "Query" }],
    ["open_target", { target: "https://github.com/search?q=heyo" }],
  ]);
  // The prompt left: nothing opens.
  expect(await opening(github)).toHaveLength(2);
  expect(await opening(greeting, { replies: ["Ada"], answers: { read_clipboard: "noon" } })).toEqual([
    ["show_window"],
    ["ask", { title: "Greeting", placeholder: "Text for {argument}" }],
    ["read_clipboard"],
    ["copy_text", { text: "Hello Ada, it is noon", paste: true }],
  ]);
  expect((await opening(docs, { failing: ["open_target"] })).pop()).toEqual(["showError", "Could not open Docs", "open_target failed"]);
});

test("what could not be opened is said so, also when its argument came from the prompt", async () => {
  for (const [found, command] of [[github, "open_target"], [greeting, "copy_text"]] as const) {
    const { fake, asked } = palette({ replies: ["heyo"], failing: [command] });
    await open(found, fake);
    expect((await settled(asked)).pop()).toEqual(["showError", `Could not open ${found.title}`, `${command} failed`]);
  }
});
