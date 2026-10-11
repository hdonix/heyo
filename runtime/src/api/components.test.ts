// The components that do more than name a host element, rendered the way a
// command is (the renderer, the navigation stack) and driven the way the
// webview drives them (`dispatch` on a node's function prop).
import fs from "node:fs";
import path from "node:path";
import { createElement as h, useState, type ReactNode } from "react";
import { afterEach, beforeEach, expect, test } from "vitest";

import { dataDir, requests, reset, sent } from "../testing";

import { foreground, runAs, scoped, type Current } from "../context";
import { dispatch, mount, visibleChildren, type Container, type HostNode } from "../renderer";
import { Action, ActionPanel, Form, List } from "./components";
import { Navigator, type Navigation } from "./navigation";

const extension = { dir: "", manifest: { name: "test-extension", title: "Test", commands: [] } };
const storedFile = path.join(dataDir, "support", "test-extension", "stored-values.json");
const settle = () => new Promise((resolve) => setTimeout(resolve, 20));

let unmount: (() => void) | undefined;
/// The command the views under test belong to.
let command: Current;

beforeEach(() => {
  fs.rmSync(dataDir, { recursive: true, force: true });
  fs.mkdirSync(dataDir, { recursive: true });
  command = foreground.value = { extension, command: "main", launchType: "userInitiated" };
  reset();
});

afterEach(async () => {
  unmount?.();
  unmount = undefined;
  await settle();
  fs.rmSync(dataDir, { recursive: true, force: true });
});

/// Runs `element` as a command's root view.
async function render(element: ReactNode) {
  const container: Container = { children: [], onCommit() {} };
  let navigation!: Navigation;
  unmount?.();
  const root = h(Navigator, { root: element, onPopRoot() {}, expose: (n: Navigation) => void (navigation = n) });
  unmount = runAs(command, () =>
    mount(scoped(command, root), container, (e) => {
      throw e;
    }),
  );
  await settle();
  /// The root node of the view on top, as the session would send it.
  const top = () => visibleChildren(container.children.filter((c) => c.type === "View").at(-1)!)[0];
  return { top, depth: () => container.children.length, navigation };
}

function all(node: HostNode, type: string): HostNode[] {
  return [...(node.type === type ? [node] : []), ...node.children.flatMap((child) => all(child, type))];
}

const one = (node: HostNode, type: string, where: (n: HostNode) => boolean = () => true) => {
  const found = all(node, type).find(where);
  if (!found) throw new Error(`no ${type}`);
  return found;
};

/// What the webview does: calls a function prop of a node.
async function call(node: HostNode, prop: string, ...args: unknown[]) {
  runAs(command, () => dispatch(node.id, prop, args));
  await settle();
}

test("a search bar's change comes back with its number, in the render that answers it", async () => {
  const heard: unknown[][] = [];
  function Command() {
    const [text, setText] = useState("");
    return h(List, {
      searchText: text,
      onSearchTextChange: (...args: unknown[]) => {
        heard.push(args);
        setText(String(args[0]).toUpperCase());
      },
    });
  }
  const { top } = await render(h(Command));
  expect(top().props).toMatchObject({ searchText: "", ack: 0 });
  await call(top(), "onSearchTextChange", "ab", 2);
  expect(top().props).toMatchObject({ searchText: "AB", ack: 2 });
  // The extension is given the text, and nothing else.
  expect(heard).toEqual([["ab"]]);
});

test("an extension that answers late is given the number of the keystroke it echoes", async () => {
  let setText!: (text: string) => void;
  function Command() {
    const [text, state] = useState("");
    setText = state;
    // Heard at once, shown only later: a fetch, a timeout.
    return h(List, { searchText: text, onSearchTextChange() {} });
  }
  const { top } = await render(h(Command));
  await call(top(), "onSearchTextChange", "a", 1);
  await call(top(), "onSearchTextChange", "ab", 2);
  await call(top(), "onSearchTextChange", "abc", 3);
  // Nothing shown yet is as new as the last change handled.
  expect(top().props).toMatchObject({ searchText: "", ack: 3 });
  // The answer to the second keystroke: numbered as such, so the webview
  // keeps "abc" on screen. Shown again, it is still that answer.
  setText("ab");
  await settle();
  expect(top().props).toMatchObject({ searchText: "ab", ack: 2 });
  setText("ab");
  await settle();
  expect(top().props).toMatchObject({ searchText: "ab", ack: 2 });
  // Caught up.
  setText("abc");
  await settle();
  expect(top().props).toMatchObject({ searchText: "abc", ack: 3 });
  // A value never typed is the extension's own: as new as the last change.
  setText("ABC");
  await settle();
  expect(top().props).toMatchObject({ searchText: "ABC", ack: 3 });
  // Typed again later, the same text is a new change.
  await call(top(), "onSearchTextChange", "ab", 4);
  setText("ab");
  await settle();
  expect(top().props).toMatchObject({ searchText: "ab", ack: 4 });
});

test("a view that does not listen to its search bar is given no number", async () => {
  const { top } = await render(h(List, { searchText: "fixed" }));
  expect(top().props.ack).toBeUndefined();
});

test("a text field's change comes back with its number", async () => {
  function Command() {
    const [name, setName] = useState("");
    return h(Form, null, h(Form.TextField, { id: "name", value: name, onChange: setName }), h(Form.Checkbox, { id: "on", label: "On", onChange() {} }));
  }
  const { top } = await render(h(Command));
  await call(one(top(), "Form.TextField"), "onChange", "x", 5);
  expect(one(top(), "Form.TextField").props).toMatchObject({ value: "x", ack: 5 });
  // Only what is typed into is numbered.
  expect(one(top(), "Form.Checkbox").props.ack).toBeUndefined();
});

test("storeValue keeps what a form was submitted with, and starts from it next time", async () => {
  const when = new Date(2026, 4, 17, 9, 30);
  const changes: Record<string, unknown[]> = { name: [], plain: [] };
  const submitted: unknown[] = [];
  const form = () =>
    h(
      Form,
      { actions: h(ActionPanel, null, h(Action.SubmitForm, { onSubmit: (values: unknown) => void submitted.push(values) })) },
      h(Form.TextField, { id: "name", storeValue: true, defaultValue: "default", onChange: (v: unknown) => void changes.name.push(v) }),
      h(Form.PasswordField, { id: "secret", storeValue: true }),
      h(Form.Checkbox, { id: "agree", label: "Agree", storeValue: true }),
      h(Form.DatePicker, { id: "when", storeValue: true }),
      h(Form.TagPicker, { id: "tags", storeValue: true }),
      h(Form.FilePicker, { id: "files", storeValue: true }),
      h(Form.Dropdown, { id: "kind", storeValue: true }),
      h(Form.TextArea, { id: "notes", storeValue: true }),
      h(Form.TextField, { id: "plain", defaultValue: "default", onChange: (v: unknown) => void changes.plain.push(v) }),
    );

  let view = await render(form());
  // Nothing kept yet: fields are as the extension wrote them.
  expect(one(view.top(), "Form.TextField").props.defaultValue).toBe("default");
  expect(changes.name).toEqual([]);

  const values = { name: "Ada", secret: "hunter2", agree: true, when, tags: ["a", "b"], files: ["/tmp/x"], kind: "two", notes: "line\nline", plain: "not kept" };
  // A date crosses from the webview as `{ $date }`.
  await call(one(view.top(), "Action"), "onSubmit", { ...values, when: { $date: when.toISOString() } });
  expect(submitted).toEqual([values]);
  // Private to the user, like the preferences.
  expect(fs.statSync(storedFile).mode & 0o777).toBe(0o600);

  view = await render(form());
  const field = (type: string, id: string) => one(view.top(), type, (n) => n.props.id === id).props;
  expect(field("Form.TextField", "name").defaultValue).toBe("Ada");
  expect(field("Form.PasswordField", "secret").defaultValue).toBe("hunter2");
  expect(field("Form.Checkbox", "agree").defaultValue).toBe(true);
  expect(field("Form.DatePicker", "when").defaultValue).toEqual(when);
  expect(field("Form.TagPicker", "tags").defaultValue).toEqual(["a", "b"]);
  expect(field("Form.FilePicker", "files").defaultValue).toEqual(["/tmp/x"]);
  expect(field("Form.Dropdown", "kind").defaultValue).toBe("two");
  expect(field("Form.TextArea", "notes").defaultValue).toBe("line\nline");
  // The field that did not ask is as it was written.
  expect(field("Form.TextField", "plain").defaultValue).toBe("default");
  // The extension hears of the value put back, and of nothing else.
  expect(changes).toEqual({ name: ["Ada"], plain: [] });

  // Another command of the same extension has its own.
  command = foreground.value = { extension, command: "other", launchType: "userInitiated" };
  view = await render(form());
  expect(field("Form.TextField", "name").defaultValue).toBe("default");
});

test("a search bar dropdown with storeValue starts from the last selection", async () => {
  const changes: unknown[] = [];
  const dropdown = (items: string[]) =>
    h(List, {
      searchBarAccessory: h(
        List.Dropdown,
        { tooltip: "Kind", storeValue: true, onChange: (v: unknown) => void changes.push(v) },
        items.map((value) => h(List.Dropdown.Item, { key: value, value, title: value })),
      ),
    });

  let view = await render(dropdown(["a", "b", "c"]));
  expect(one(view.top(), "Dropdown").props.defaultValue).toBe("a");
  expect(changes).toEqual(["a"]);
  await call(one(view.top(), "Dropdown"), "onChange", "c");
  expect(changes).toEqual(["a", "c"]);

  view = await render(dropdown(["a", "b", "c"]));
  expect(one(view.top(), "Dropdown").props.defaultValue).toBe("c");
  expect(changes).toEqual(["a", "c", "c"]);

  // The item it was kept for is gone: the first one, as if nothing was kept.
  view = await render(dropdown(["a", "b"]));
  expect(one(view.top(), "Dropdown").props.defaultValue).toBe("a");
});

test("a dropdown without storeValue keeps nothing", async () => {
  const view = await render(h(List, { searchBarAccessory: h(List.Dropdown, { tooltip: "Kind", onChange() {} }, h(List.Dropdown.Item, { value: "a", title: "a" })) }));
  await call(one(view.top(), "Dropdown"), "onChange", "a");
  expect(fs.existsSync(storedFile)).toBe(false);
});

test("a file picker opens a browser over the form and takes what is chosen there", async () => {
  const folder = path.join(dataDir, "files");
  fs.mkdirSync(path.join(folder, "sub"), { recursive: true });
  fs.writeFileSync(path.join(folder, "a.txt"), "a");
  fs.writeFileSync(path.join(folder, ".hidden"), "h");

  const view = await render(h(Form, null, h(Form.FilePicker, { id: "file" })));
  expect(view.top().type).toBe("Form");
  await call(one(view.top(), "Form.FilePicker"), "browse", [path.join(folder, "a.txt")]);

  // The browser, in the folder of what is already chosen.
  expect(view.top().type).toBe("List");
  const titles = () => all(view.top(), "List.Item").map((item) => item.props.title);
  expect(titles()).toEqual(["..", "sub", "a.txt"]);

  const row = one(view.top(), "List.Item", (item) => item.props.title === "a.txt");
  expect(all(row, "Action").map((a) => a.props.title)).toContain("Choose File");
  await call(one(row, "Action", (a) => a.props.title === "Choose File"), "onAction");

  expect(view.top().type).toBe("Form");
  expect(one(view.top(), "Form.FilePicker").props.picked).toEqual({ seq: 1, paths: [path.join(folder, "a.txt")] });
});

test("a file picker for several files and folders lets them be marked, hidden ones too", async () => {
  const folder = path.join(dataDir, "files");
  fs.mkdirSync(path.join(folder, "sub"), { recursive: true });
  fs.writeFileSync(path.join(folder, "a.txt"), "a");
  fs.writeFileSync(path.join(folder, ".hidden"), "h");

  const view = await render(h(Form, null, h(Form.FilePicker, { id: "file", allowMultipleSelection: true, canChooseDirectories: true, showHiddenFiles: true })));
  await call(one(view.top(), "Form.FilePicker"), "browse", [path.join(folder, "a.txt")]);
  const row = (title: string) => one(view.top(), "List.Item", (item) => item.props.title === title);
  const act = (title: string, action: string) => call(one(row(title), "Action", (a) => a.props.title === action), "onAction");
  expect(all(view.top(), "List.Item").map((item) => item.props.title)).toEqual([expect.stringContaining("files"), "..", "sub", ".hidden", "a.txt"]);

  // What was chosen before is marked; marking again takes it out.
  expect(row("a.txt").props.accessories).toEqual([{ tag: "Selected" }]);
  await act(".hidden", "Select");
  await act("sub", "Select");
  await act("a.txt", "Deselect");
  await act("sub", "Done (2 Selected)");

  expect(view.top().type).toBe("Form");
  expect(one(view.top(), "Form.FilePicker").props.picked).toEqual({ seq: 1, paths: [path.join(folder, ".hidden"), path.join(folder, "sub")] });
});

test("a file picker for folders only shows no files, and walks into folders", async () => {
  const folder = path.join(dataDir, "files");
  fs.mkdirSync(path.join(folder, "sub", "deeper"), { recursive: true });
  fs.writeFileSync(path.join(folder, "a.txt"), "a");

  const view = await render(h(Form, null, h(Form.FilePicker, { id: "file", canChooseFiles: false, canChooseDirectories: true })));
  await call(one(view.top(), "Form.FilePicker"), "browse", [path.join(folder, "a.txt")]);
  expect(all(view.top(), "List.Item").map((item) => item.props.title)).toEqual([expect.stringContaining("files"), "..", "sub"]);

  const sub = one(view.top(), "List.Item", (item) => item.props.title === "sub");
  // Enter walks in; choosing it is the action after.
  expect(all(sub, "Action").map((a) => a.props.title).slice(0, 2)).toEqual(["Open Folder", "Choose Folder"]);
  await call(all(sub, "Action")[0], "onAction");
  expect(view.top().props.navigationTitle).toBe("sub");
  expect(sent).toContainEqual({ type: "clear-search-bar" });

  const here = all(view.top(), "List.Item")[0];
  await call(one(here, "Action"), "onAction");
  expect(one(view.top(), "Form.FilePicker").props.picked.paths).toEqual([path.join(folder, "sub")]);
});

test("Action.PickDate tells the webview what to ask for, and hears the date", async () => {
  const picked: unknown[] = [];
  const min = new Date(2026, 0, 1);
  const view = await render(
    h(List, {
      actions: h(ActionPanel, null, h(Action.PickDate, { title: "Due…", type: Action.PickDate.Type.Date, min, onChange: (date: unknown) => void picked.push(date) })),
    }),
  );
  const action = one(view.top(), "Action");
  expect(action.props).toMatchObject({ title: "Due…", pickDate: { type: "date", min } });
  // As the webview sends it.
  await call(action, "onChange", { $date: "2026-02-03T00:00:00.000Z" });
  await call(action, "onChange", null);
  expect(picked).toEqual([new Date("2026-02-03T00:00:00.000Z"), null]);
  expect(Action.PickDate.isFullDay(new Date(2026, 1, 3))).toBe(true);
  expect(Action.PickDate.isFullDay(new Date(2026, 1, 3, 10))).toBe(false);
});

test("Action.CreateSnippet and Action.CreateQuicklink open Heyo's forms, filled in", async () => {
  const view = await render(
    h(List, {
      actions: h(
        ActionPanel,
        null,
        h(Action.CreateSnippet, { snippet: { name: "Greeting", text: "Hello {clipboard}", keyword: "hi" } }),
        h(Action.CreateQuicklink, { quicklink: { name: "Search", link: "https://example.com/?q={query}" } }),
      ),
    }),
  );
  const [snippet, quicklink] = all(view.top(), "Action");
  expect([snippet.props.title, quicklink.props.title]).toEqual(["Create Snippet", "Create Quicklink"]);

  await call(snippet, "onAction");
  expect(view.top()).toMatchObject({ type: "Form", props: { navigationTitle: "Create Snippet" } });
  const defaults = (type: string) => Object.fromEntries(all(view.top(), type).map((f) => [f.props.id, f.props.defaultValue]));
  expect({ ...defaults("Form.TextField"), ...defaults("Form.TextArea") }).toEqual({ name: "Greeting", text: "Hello {clipboard}", keyword: "hi" });

  // Saving it writes Heyo's snippets and comes back to the extension's view.
  await call(one(view.top(), "Action"), "onSubmit", { name: "Greeting", text: "Hello {clipboard}", keyword: "hi" });
  expect(JSON.parse(fs.readFileSync(path.join(dataDir, "snippets.json"), "utf8"))).toMatchObject([{ name: "Greeting", text: "Hello {clipboard}", keyword: "hi" }]);
  expect(view.top().type).toBe("List");

  await call(all(view.top(), "Action")[1], "onAction");
  expect(view.top()).toMatchObject({ type: "Form", props: { navigationTitle: "Create Quicklink" } });
  expect(defaults("Form.TextField")).toEqual({ name: "Search", link: "https://example.com/?q={query}" });
});

test("Action.ToggleQuickLook shows the selected item's file, and the same key closes it", async () => {
  const file = path.join(dataDir, "notes.txt");
  fs.writeFileSync(file, "some ``` text");
  const shortcut = { modifiers: ["cmd"], key: "y" };
  const view = await render(h(List, null, h(List.Item, { title: "Notes", quickLook: { path: file }, actions: h(ActionPanel, null, h(Action.ToggleQuickLook, null)) })));
  const toggle = one(view.top(), "Action");
  // The webview is told to send the selection's `quickLook` along.
  expect(toggle.props).toMatchObject({ title: "Quick Look", preview: true, shortcut });
  expect(one(view.top(), "List.Item").props.quickLook).toEqual({ path: file });

  await call(toggle, "onAction", { path: file });
  expect(view.top()).toMatchObject({ type: "Detail", props: { navigationTitle: "notes.txt", markdown: "````\nsome ``` text\n````" } });
  const close = one(view.top(), "Action");
  expect(close.props).toMatchObject({ title: "Close Quick Look", shortcut });
  await call(close, "onAction");
  expect(view.top().type).toBe("List");
});

test("Quick Look of what cannot be shown here opens it; of nothing, says so", async () => {
  const binary = path.join(dataDir, "archive.bin");
  fs.writeFileSync(binary, Buffer.from([0x50, 0x4b, 0, 0, 1, 2, 3]));
  const view = await render(h(List, { actions: h(ActionPanel, null, h(Action.ToggleQuickLook, null)) }));
  await call(one(view.top(), "Action"), "onAction", { path: binary });
  expect(view.top().type).toBe("List");
  expect(requests).toContainEqual({ method: "host.open", params: expect.objectContaining({ target: binary }) });

  reset();
  await call(one(view.top(), "Action"), "onAction", undefined);
  expect(sent.find((m) => m.type === "toast")?.toast).toMatchObject({ style: "FAILURE", title: "Nothing to preview" });
  await call(one(view.top(), "Action"), "onAction", { path: path.join(dataDir, "missing.txt") });
  expect(sent.filter((m) => m.type === "toast").at(-1)?.toast).toMatchObject({ style: "FAILURE", title: "Could not preview the file" });
});
