// Heyo's own commands, written against `@heyo/api`: what each asks of the
// host and in whose name, and that it shows what it did when it asked the
// host directly.
import { createElement, type ReactNode } from "react";
import { afterEach, beforeEach, expect, test } from "vitest";

import { answers, extension, requests, reset, sent, sleep, until } from "../testing";

import { Navigator } from "../api/navigation";
import { foreground, runAs, scoped, type Current } from "../context";
import { envelope } from "../heyo/caller";
import { dispatch, mount, visibleChildren, type Container, type HostNode } from "../renderer";
import { ClipboardHistory } from "./clipboard";
import { builtinExtension } from "./index";
import { SelectionActions } from "./selection";
import { AudioDevices, SearchFiles, SwitchWindows, systemCommands } from "./system";

const settle = (ms = 25) => sleep(ms);
const own = (command: string): Current => ({ extension: builtinExtension, command, launchType: "userInitiated", user: true });

let unmount: (() => void) | undefined;
let command: Current;

beforeEach(() => {
  reset();
  // Nothing the host is asked for fails for want of an answer.
  answers["host.applications"] = () => [];
});

afterEach(async () => {
  unmount?.();
  unmount = undefined;
  foreground.value = null;
  await settle();
});

/// Runs one of Heyo's commands as the session does: as the command in the palette.
async function show(name: string, element: ReactNode) {
  command = foreground.value = own(name);
  const container: Container = { children: [], onCommit() {} };
  const errors: unknown[] = [];
  const root = createElement(Navigator, { root: element, onPopRoot() {}, expose() {} });
  unmount = runAs(command, () => mount(scoped(command, root), container, (e) => errors.push(e)));
  await settle();
  const top = () => visibleChildren(container.children.filter((c) => c.type === "View").at(-1)!)[0];
  const all = (type: string, under: HostNode = top()): HostNode[] => [...(under.type === type ? [under] : []), ...visibleChildren(under).flatMap((child) => all(type, child))];
  /// Also what is not on screen at the moment (the detail of a row that is not selected).
  const deep = (type: string, under: HostNode = top()): HostNode[] => [...(under.type === type ? [under] : []), ...under.children.flatMap((child) => deep(type, child))];
  return {
    errors,
    top,
    all,
    deep,
    items: () => all("List.Item"),
    titles: () => all("List.Item").map((item) => item.props.title),
    sections: () => all("List.Section").map((section) => [section.props.title, section.props.subtitle, all("List.Item", section).map((item) => item.props.title)]),
    empty: () => all("List.EmptyView")[0]?.props,
    /// What the webview does when an action is chosen.
    async act(item: HostNode, title: string) {
      const action = all("Action", item).find((a) => a.props.title === title);
      if (!action) throw new Error(`no action ${title}: ${all("Action", item).map((a) => a.props.title)}`);
      runAs(command, () => dispatch(action.id, "onAction", []));
      await settle();
    },
    async type(text: string) {
      runAs(command, () => dispatch(top().id, "onSearchTextChange", [text, 1]));
      await settle();
    },
  };
}

/// What was asked of the host, in order; what an extension could ask too
/// goes out under Heyo's own name.
const asked = () => requests.map((r) => (r.extension === undefined ? r.method : `${r.method} as ${r.extension}${r.dir}`));
const paramsOf = (method: string) => requests.filter((r) => r.method === method).map((r) => JSON.parse(JSON.stringify(r.params ?? null)));

test("Heyo's own commands ask in Heyo's name, which no installed extension has", () => {
  expect(envelope(own("switch-windows"))).toEqual({ extension: "@heyo", dir: "" });
  // An extension may call itself what it likes; it is installed somewhere,
  // and says so with every request. (The host refuses the pair: `permissions.rs`.)
  const pretender = extension("@heyo", {});
  const whose = envelope({ extension: pretender, command: "main", launchType: "userInitiated" })!;
  expect(whose.extension).toBe("@heyo");
  expect(whose.dir).toBe(pretender.dir);
  expect(whose.dir).not.toBe("");
});

test("Switch Windows lists the windows with the one to go back to first", async () => {
  answers["ext.windows.list"] = () => [
    { id: 1, title: "main.rs", class: "Code", active: true, bounds: { x: 0, y: 0, width: 800, height: 600 }, monitor: 0 },
    { id: 2, title: "Inbox", class: "firefox", active: false },
    { id: 3, title: "Terminal", class: "org.gnome.Ptyxis", active: false },
  ];
  answers["host.applications"] = () => [{ name: "Firefox", path: "/usr/share/applications/firefox.desktop", bundleId: "firefox.desktop", icon: "/icons/firefox.png" }];
  const view = await show("switch-windows", createElement(SwitchWindows));
  expect(view.errors).toEqual([]);
  expect(asked().sort()).toEqual(["ext.windows.list as @heyo", "host.applications"]);

  // Most recently used first, but for the window the user came from.
  expect(view.items().map((item) => [item.props.title, item.props.subtitle, item.props.icon, item.props.accessories])).toEqual([
    ["Inbox", "firefox", "/icons/firefox.png", undefined],
    ["Terminal", "org.gnome.Ptyxis", expect.anything(), undefined],
    ["main.rs", "Code", expect.anything(), [{ tag: "Current" }]],
  ]);

  reset();
  await view.act(view.items()[0], "Switch to Window");
  expect(asked()).toEqual(["ext.windows.activate as @heyo"]);
  expect(paramsOf("ext.windows.activate")).toEqual([{ id: 2 }]);

  // Closing one asks again for what is left.
  reset();
  answers["ext.windows.list"] = () => [{ id: 1, title: "main.rs", class: "Code", active: true }];
  await view.act(view.items()[1], "Close Window");
  expect(paramsOf("ext.windows.close")).toEqual([{ id: 3 }]);
  await until(() => view.titles().length === 1, "the list to be read again");
  expect(asked()).toEqual(["ext.windows.close as @heyo", "ext.windows.list as @heyo"]);
  expect(view.titles()).toEqual(["main.rs"]);
});

test("Switch Windows says so when the windows cannot be listed", async () => {
  const { HeyoError } = await import("../heyo/error");
  answers["ext.windows.list"] = () => {
    throw new HeyoError("unsupported", "Reaching other applications' windows is not supported in this session.");
  };
  const view = await show("switch-windows", createElement(SwitchWindows));
  expect(view.titles()).toEqual([]);
  expect(view.empty()).toMatchObject({ title: "No open windows" });
  expect(sent.find((m) => m.type === "toast")).toMatchObject({ toast: { title: "Could not list windows", message: "Reaching other applications' windows is not supported in this session." } });
});

test("the system commands have the host do it, and say what it said", async () => {
  const run = (name: keyof typeof systemCommands) => runAs(own(name), () => systemCommands[name]());
  answers["ext.system.run"] = ({ action }) => (action === "volume-up" ? "Volume 40%" : null);
  await run("volume-up");
  expect(asked()).toEqual(["ext.system.run as @heyo", "host.hud"]);
  expect(paramsOf("ext.system.run")).toEqual([{ action: "volume-up" }]);
  expect(paramsOf("host.hud")).toEqual([{ text: "Volume 40%" }]);

  // Nothing to say: nothing said.
  reset();
  answers["ext.system.run"] = () => null;
  await run("toggle-dark-mode");
  expect(asked()).toEqual(["ext.system.run as @heyo"]);

  // What ends the session, or cannot be undone, is confirmed in the command's own words first.
  for (const [name, title, yes] of [
    ["restart", "Restart the computer?", "Restart"],
    ["shut-down", "Shut down the computer?", "Shut Down"],
    ["log-out", "Log out?", "Log Out"],
    ["empty-trash", "Empty the trash?", "Empty Trash"],
  ] as const) {
    reset();
    answers["ui.confirm"] = () => false;
    await run(name);
    expect(asked(), name).toEqual(["ui.confirm"]);
    expect(paramsOf("ui.confirm")[0], name).toMatchObject({ title, primaryAction: { title: yes, style: "DESTRUCTIVE" } });

    reset();
    answers["ui.confirm"] = () => true;
    await run(name);
    expect(asked(), name).toEqual(["ui.confirm", "ext.system.run as @heyo"]);
    expect(paramsOf("ext.system.run"), name).toEqual([{ action: name }]);
  }

  // Every one of them goes the same way.
  for (const name of Object.keys(systemCommands) as (keyof typeof systemCommands)[]) {
    reset();
    answers["ui.confirm"] = () => true;
    await run(name);
    expect(paramsOf("ext.system.run"), name).toEqual([{ action: name }]);
    expect(requests.some((r) => r.method === "host.system"), name).toBe(false);
  }
});

test("Audio Devices shows the speakers and the microphones, and switches", async () => {
  let current = "speakers";
  answers["ext.system.audioDevices"] = () => [
    { name: "speakers", description: "Built-in Speakers", kind: "output", current: current === "speakers" },
    { name: "headset", description: "USB Headset", kind: "output", current: current === "headset" },
    { name: "mic", description: "Built-in Microphone", kind: "input", current: true },
  ];
  answers["ext.system.useAudioDevice"] = ({ name }) => void (current = name);
  const view = await show("audio-devices", createElement(AudioDevices));
  expect(asked()).toEqual(["ext.system.audioDevices as @heyo"]);
  expect(view.sections()).toEqual([
    ["Output", undefined, ["Built-in Speakers", "USB Headset"]],
    ["Input", undefined, ["Built-in Microphone"]],
  ]);
  const marked = () =>
    view
      .items()
      .filter((item) => item.props.accessories)
      .map((item) => item.props.title);
  expect(marked()).toEqual(["Built-in Speakers", "Built-in Microphone"]);

  reset();
  answers["ext.system.audioDevices"] = () => [{ name: "headset", description: "USB Headset", kind: "output", current: true }];
  await view.act(view.items()[1], "Use as Output");
  expect(asked()).toEqual(["ext.system.useAudioDevice as @heyo", "ext.system.audioDevices as @heyo"]);
  expect(paramsOf("ext.system.useAudioDevice")).toEqual([{ kind: "output", name: "headset" }]);
  expect(marked()).toEqual(["USB Headset"]);
  expect(sent.filter((m) => m.type === "toast").at(-1)).toMatchObject({ toast: { title: "Output: USB Headset" } });
});

test("Search Files has the names read again, searches as typed, and says how many there are", async () => {
  const found = [
    { path: "/home/u/notes.md", isDirectory: false },
    { path: "/home/u/notes", isDirectory: true },
  ];
  answers["ext.files.search"] = ({ query }) => ({ files: query ? found : [], scanning: false, total: 90_000 });
  const view = await show("search-files", createElement(SearchFiles));
  expect(asked().sort()).toEqual(["ext.files.search as @heyo", "host.files.refresh"]);
  expect(paramsOf("ext.files.search")).toEqual([{ query: "" }]);
  expect(view.empty()).toMatchObject({ title: "Type to search your files" });
  expect(view.empty().description).toContain(`${(90_000).toLocaleString()} files and folders`);

  requests.length = 0;
  await view.type("notes");
  await until(() => view.titles().length, "the files found");
  expect(asked()).toEqual(["ext.files.search as @heyo"]);
  expect(paramsOf("ext.files.search")).toEqual([{ query: "notes" }]);
  expect(view.items().map((item) => [item.props.title, item.props.subtitle])).toEqual([
    ["notes.md", "/home/u"],
    ["notes", "/home/u"],
  ]);
  expect(view.top().props.isLoading).toBe(false);
});

test("Search Files asks again until the names have been read", async () => {
  let scans = 2;
  answers["ext.files.search"] = () => ({ files: [], scanning: scans-- > 0, total: 0 });
  const view = await show("search-files", createElement(SearchFiles));
  expect(view.top().props.isLoading).toBe(true);
  expect(view.empty()).toMatchObject({ title: "Type to search your files", description: "Reading file names…" });
  await until(() => view.top().props.isLoading === false, "the scan to finish", 2000);
  expect(paramsOf("ext.files.search").length).toBe(3);
});

const day = 86_400_000;
const copied = (id: number, more: Record<string, unknown>) => ({
  id: `clipboard:${id}`,
  source: "clipboard",
  kind: "text",
  at: Date.now() - 1000 * id,
  firstAt: Date.now() - day,
  timesCopied: 2,
  pinned: false,
  ...more,
});

test("Clipboard History reads the history as an extension would, and shows all it did", async () => {
  const items = [
    copied(9, { text: "the newest\nsecond line", app: "firefox", appName: "Firefox", appIcon: "/icons/firefox.png" }),
    copied(8, { kind: "image", image: "/data/clipboard/images/8.png", width: 640, height: 480, app: "gimp", appName: "gimp" }),
    copied(7, { kind: "link", text: "https://example.com", at: Date.now() - 30 * day }),
    copied(3, { kind: "color", text: "#ff8800", pinned: true }),
  ];
  answers["ext.memory.search"] = () => ({ items, understood: { terms: [] }, relaxed: false });
  answers["host.clipboardHistory.status"] = () => ({ recording: true, paused: false });
  const view = await show("clipboard-history", createElement(ClipboardHistory));
  expect(view.errors).toEqual([]);
  expect(asked().sort()).toEqual(["ext.memory.search as @heyo", "host.clipboardHistory.status"]);
  // As many as the view ever showed; no kind while every kind is shown.
  expect(paramsOf("ext.memory.search")).toEqual([{ text: "", limit: 300 }]);

  expect(view.sections()).toEqual([
    ["Pinned", undefined, ["#ff8800"]],
    ["Today", undefined, ["the newest", "Image (640×480)"]],
    ["Older", undefined, ["https://example.com"]],
  ]);
  const [pinned, newest, image, link] = view.items();
  // The number an entry always went by in the list.
  expect(view.items().map((item) => item.props.id)).toEqual(["3", "9", "8", "7"]);
  expect(newest.props.icon).toEqual({ source: "/icons/firefox.png", fallback: expect.anything() });
  expect(newest.props.accessories).toEqual([{ icon: "/icons/firefox.png", text: undefined, tooltip: "Copied from Firefox" }]);
  expect(newest.props.keywords).toEqual(["firefox", "Firefox"]);
  expect(image.props.icon).toBe("/data/clipboard/images/8.png");
  expect(image.props.accessories).toEqual([{ icon: undefined, text: "gimp", tooltip: "Copied from gimp" }]);
  expect(pinned.props.icon).toMatchObject({ tintColor: "#ff8800" });
  expect(link.props.accessories).toBeUndefined();

  const labels = (item: typeof newest) => Object.fromEntries(view.deep("Metadata.Label", item).map((label) => [label.props.title, label.props.text]));
  expect(labels(newest)).toMatchObject({ Application: "Firefox", "Content type": "Text", Characters: "22", Words: "4", "Times copied": "2" });
  expect(labels(newest)["First copied"]).not.toBe(labels(newest)["Last copied"]);
  expect(labels(image)).toMatchObject({ Application: "gimp", "Content type": "Image", Dimensions: "640 × 480" });
  expect(labels(link)).toMatchObject({ Application: "Unknown", "Content type": "Link" });

  // Putting an entry back is what any extension may do. Nothing was searched for: nothing to note.
  reset();
  await view.act(newest, "Copy to Clipboard");
  expect(asked()).toEqual(["ext.clipboardHistory.copy as @heyo", "host.hide", "host.hud"]);
  expect(paramsOf("ext.clipboardHistory.copy")).toEqual([{ id: "clipboard:9" }]);

  // The keeping of the history is Heyo's own, by the entry's number.
  reset();
  answers["ext.memory.search"] = () => ({ items, understood: { terms: [] }, relaxed: false });
  answers["host.clipboardHistory.status"] = () => ({ recording: true, paused: false });
  await view.act(newest, "Pin Entry");
  expect(paramsOf("host.clipboardHistory.pin")).toEqual([{ id: 9, pinned: true }]);
  await view.act(view.items()[0], "Unpin Entry");
  expect(paramsOf("host.clipboardHistory.pin")[1]).toEqual({ id: 3, pinned: false });
  await view.act(view.items()[3], "Delete Entry");
  expect(paramsOf("host.clipboardHistory.delete")).toEqual([{ id: 7 }]);
  // Each is followed by reading the history again.
  expect(requests.filter((r) => r.method === "ext.memory.search").length).toBe(3);

  reset();
  answers["ext.memory.search"] = () => ({ items, understood: { terms: [] }, relaxed: false });
  answers["host.clipboardHistory.status"] = () => ({ recording: true, paused: false });
  answers["ui.confirm"] = () => false;
  await view.act(view.items()[1], "Delete All Entries");
  expect(asked()).toEqual(["ui.confirm"]);
  answers["ui.confirm"] = () => true;
  await view.act(view.items()[1], "Delete All Entries");
  expect(asked()).toContain("host.clipboardHistory.clear");

  reset();
  answers["ext.memory.search"] = () => ({ items, understood: { terms: [] }, relaxed: false });
  answers["host.clipboardHistory.status"] = () => ({ recording: true, paused: false });
  await view.act(view.items()[1], "Pause Recording");
  expect(paramsOf("host.clipboardHistory.pause")).toEqual([{ paused: true }]);
  expect(asked().slice(1).sort()).toEqual(["ext.memory.search as @heyo", "host.clipboardHistory.status"]);
});

test("Clipboard History searches as typed, says how the query was read, and notes what was used", async () => {
  const from = new Date(2026, 9, 3).getTime();
  const links = [copied(5, { kind: "link", text: "https://one.example", app: "firefox" }), copied(4, { kind: "link", text: "https://two.example" })];
  answers["ext.memory.search"] = ({ text, kinds }) =>
    text
      ? { items: links, understood: { terms: ["example"], app: "firefox", kind: kinds?.[0], from, to: from + day }, relaxed: false }
      : { items: [copied(5, { text: "plain" })], understood: { terms: [] }, relaxed: false };
  answers["host.clipboardHistory.status"] = () => ({ recording: true, paused: true });
  const view = await show("clipboard-history", createElement(ClipboardHistory));
  // Paused: said in the search field and over the first section.
  expect(view.top().props.searchBarPlaceholder).toMatch(/^Recording is paused · Search/);
  expect(view.sections()).toEqual([["Today", "Recording is paused", ["plain"]]]);
  expect(view.all("Action", view.items()[0]).map((a) => a.props.title)).toContain("Resume Recording");

  requests.length = 0;
  await view.type("example from firefox yesterday ");
  expect(paramsOf("ext.memory.search")).toEqual([{ text: "example from firefox yesterday ", limit: 300 }]);
  const day3 = new Date(from).toLocaleDateString(undefined, { month: "short", day: "numeric" });
  expect(view.sections()).toEqual([["Results", `Recording is paused · ${day3} · from firefox · “example”`, ["https://one.example", "https://two.example"]]]);

  // The filter is one kind, asked for as an extension asks for kinds.
  requests.length = 0;
  runAs(command, () => dispatch(view.deep("Dropdown")[0].id, "onChange", ["link"]));
  await settle();
  expect(paramsOf("ext.memory.search")).toEqual([{ text: "example from firefox yesterday ", kinds: ["link"], limit: 300 }]);
  expect(view.sections()[0][1]).toBe(`Recording is paused · ${day3} · from firefox · Link · “example”`);

  // What was searched for and where the entry stood goes to the host, which measures the search by it.
  requests.length = 0;
  await view.act(view.items()[1], "Paste to Active App");
  expect(asked()).toEqual(["ext.clipboardHistory.copy as @heyo", "host.clipboardHistory.chose", "host.hide", "host.paste"]);
  expect(paramsOf("ext.clipboardHistory.copy")).toEqual([{ id: "clipboard:4" }]);
  expect(paramsOf("host.clipboardHistory.chose")).toEqual([{ id: 4, query: "example from firefox yesterday", rank: 1 }]);
});

test("Clipboard History adds what the query means once typing pauses, where that is on", async () => {
  const words = { items: [copied(2, { text: "an invoice" })], understood: { terms: ["invoice"] }, relaxed: false };
  const meant = { items: [copied(2, { text: "an invoice" }), copied(6, { text: "the bill for March", byMeaning: true })], understood: { terms: ["invoice"] }, relaxed: false };
  let progress: unknown = { indexed: 40, waiting: 2 };
  answers["ext.memory.search"] = ({ byMeaning }) => (byMeaning ? meant : words);
  answers["host.clipboardHistory.status"] = () => ({ recording: true, paused: false, meaning: progress });
  const view = await show("clipboard-history", createElement(ClipboardHistory));
  // Nothing typed: nothing to mean.
  await sleep(250);
  expect(paramsOf("ext.memory.search")).toEqual([{ text: "", limit: 300 }]);

  requests.length = 0;
  await view.type("invoice");
  // The words at once, while the history is still being read for meaning.
  expect(view.sections()).toEqual([["Today", "Finding by meaning is still reading 2 entries", ["an invoice"]]]);
  await until(() => view.titles().length === 2, "what the query means");
  expect(paramsOf("ext.memory.search")).toEqual([
    { text: "invoice", limit: 300 },
    { text: "invoice", limit: 300, byMeaning: true },
  ]);
  // In order of likeness, not of time, and marked.
  expect(view.sections()).toEqual([["Results", "Finding by meaning is still reading 2 entries", ["an invoice", "the bill for March"]]]);
  expect(view.items()[1].props.accessories[0]).toMatchObject({ tooltip: expect.stringContaining("Found by its meaning") });

  // A question that was overtaken is not asked about.
  requests.length = 0;
  runAs(command, () => dispatch(view.top().id, "onSearchTextChange", ["invoic", 2]));
  await sleep(40);
  runAs(command, () => dispatch(view.top().id, "onSearchTextChange", ["invoi", 3]));
  await sleep(400);
  expect(paramsOf("ext.memory.search")).toEqual([
    { text: "invoic", limit: 300 },
    { text: "invoi", limit: 300 },
    { text: "invoi", limit: 300, byMeaning: true },
  ]);

  // Where finding by meaning is off, it is never asked for.
  requests.length = 0;
  progress = undefined;
  await view.type("bill");
  await sleep(300);
  expect(paramsOf("ext.memory.search")).toEqual([{ text: "bill", limit: 300 }]);
});

test("Clipboard History says so when the history cannot be read, or is not kept", async () => {
  answers["host.clipboardHistory.status"] = () => ({ recording: false, paused: false });
  answers["ext.memory.search"] = () => ({ items: [], understood: { terms: [] }, relaxed: false });
  const off = await show("clipboard-history", createElement(ClipboardHistory));
  expect(off.empty()).toMatchObject({ title: "Clipboard history is empty", description: expect.stringContaining("Recording is turned off") });
  expect(off.top().props.searchBarPlaceholder).toMatch(/^Recording is turned off · /);
  unmount?.();

  reset();
  answers["ext.memory.search"] = () => {
    throw new Error("the history is locked");
  };
  const failed = await show("clipboard-history", createElement(ClipboardHistory));
  expect(failed.titles()).toEqual([]);
  expect(failed.top().props.isLoading).toBe(false);
  expect(sent.find((m) => m.type === "toast")).toMatchObject({ toast: { title: "Could not load clipboard history", message: "HeyoError: the history is locked" } });
});

test("Selection Actions reads the selection the way Raycast's API does", async () => {
  answers["host.selection"] = () => "  some words  ";
  const view = await show("selection", createElement(SelectionActions));
  expect(asked()).toEqual(["host.selection"]);
  expect(view.titles().length).toBeGreaterThan(3);
  unmount?.();

  reset();
  answers["host.selection"] = () => "";
  const none = await show("selection", createElement(SelectionActions));
  expect(none.titles()).toEqual([]);
  expect(none.empty()).toBeDefined();
});

test("none of Heyo's commands asks the host directly for what an extension can ask for", async () => {
  const fs = await import("node:fs");
  const path = await import("node:path");
  const heyos = [
    // Heyo's index of extensions, and of file names.
    "host.reload",
    "host.files.refresh",
    // The keeping of the clipboard history.
    "host.clipboardHistory.status",
    "host.clipboardHistory.chose",
    "host.clipboardHistory.pin",
    "host.clipboardHistory.delete",
    "host.clipboardHistory.clear",
    "host.clipboardHistory.pause",
    // Not an extension's to ask Heyo for: Node has them.
    "host.processes.list",
    "host.processes.end",
  ];
  const used = new Set<string>();
  for (const file of fs.readdirSync(__dirname).filter((name) => /\.tsx?$/.test(name) && !name.includes(".test."))) {
    for (const [, method] of fs.readFileSync(path.join(__dirname, file), "utf8").matchAll(/"(host\.[A-Za-z.]+)"/g)) used.add(method);
  }
  expect([...used].sort()).toEqual([...heyos].sort());
});
