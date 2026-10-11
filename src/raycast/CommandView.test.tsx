// @vitest-environment happy-dom
//
// A command's view, drawn from the trees the runtime would send and driven
// by the events a person would cause. What goes to the runtime is what the
// host, played here, was called with as `runtime_send`.
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, expect, test } from "vitest";

import { fakeHost, type FakeHost } from "../host.fake";
import { CommandView, type CommandState } from "./CommandView";
import type { Node, View } from "./model";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const fn = { $fn: 1 };
let nextId = 1;
const node = (type: string, props: Record<string, unknown> = {}, ...children: Node[]): Node => ({ id: nextId++, type, props, children });
const slot = (name: string, child: Node) => node("slot", { name }, child);
/// The same node with other props, as a render that changed it.
const withProps = (original: Node, props: Record<string, unknown>): Node => ({ ...original, props: { ...original.props, ...props } });

let root: Root;
let host: HTMLElement;
let heyo: FakeHost;

beforeEach(() => {
  host = document.createElement("div");
  document.body.append(host);
  root = createRoot(host);
  heyo = fakeHost();
});

afterEach(() => {
  act(() => root.unmount());
  host.remove();
});

function show(tree: Node, key = 0, extra: Partial<Parameters<typeof CommandView>[0]> = {}) {
  const view: View = { key, depth: key + 1, tree: [tree] };
  const command: CommandState = { session: 1, mode: "view", title: "Test", extension: { name: "test", title: "Test", dir: "/ext" }, view };
  act(() => root.render(<CommandView command={command} toast={null} onToastAction={() => {}} searchReset={0} fieldOp={null} {...extra} />));
}

const $ = <E extends HTMLElement = HTMLElement>(selector: string) => host.querySelector<E>(selector)!;
const search = () => $<HTMLInputElement>("input.search");

function type(input: HTMLInputElement | HTMLTextAreaElement, text: string) {
  const proto = input instanceof HTMLTextAreaElement ? HTMLTextAreaElement.prototype : HTMLInputElement.prototype;
  act(() => {
    Object.getOwnPropertyDescriptor(proto, "value")!.set!.call(input, text);
    input.dispatchEvent(new Event("input", { bubbles: true }));
  });
}

function press(target: Element, key: string, init: KeyboardEventInit = {}) {
  act(() => {
    target.dispatchEvent(new KeyboardEvent("keydown", { key, bubbles: true, cancelable: true, ...init }));
  });
}

function composition(target: Element, phase: "compositionstart" | "compositionend") {
  act(() => {
    target.dispatchEvent(new Event(phase, { bubbles: true }));
  });
}

/// What was dispatched to the runtime, as [node, prop, ...args].
const dispatched = () => heyo.callsOf("runtime_send").flatMap(({ message: m }) => (m.type === "dispatch" ? [[m.node, m.prop, ...m.args]] : []));

test("the search bar shows what is typed at once, and takes from the extension only what is new", () => {
  const list = node("List", { searchText: "", onSearchTextChange: fn, ack: 0 });
  show(list);
  type(search(), "a");
  type(search(), "ab");
  expect(search().value).toBe("ab");
  expect(dispatched()).toEqual([
    [list.id, "onSearchTextChange", "a", 1],
    [list.id, "onSearchTextChange", "ab", 2],
  ]);

  // The answer to the first key arrives after the second was typed.
  show(withProps(list, { searchText: "a", ack: 1 }));
  expect(search().value).toBe("ab");
  type(search(), "abc");
  show(withProps(list, { searchText: "ab", ack: 2 }));
  expect(search().value).toBe("abc");
  show(withProps(list, { searchText: "abc", ack: 3 }));
  expect(search().value).toBe("abc");

  // The extension rewrites it, then clears it.
  show(withProps(list, { searchText: "ABC", ack: 3 }));
  expect(search().value).toBe("ABC");
  show(withProps(list, { searchText: "", ack: 3 }));
  expect(search().value).toBe("");
});

test("a value arriving while an input method composes waits for it to finish", () => {
  const list = node("List", { searchText: "", onSearchTextChange: fn, ack: 0 });
  show(list);
  composition(search(), "compositionstart");
  type(search(), "ni");
  show(withProps(list, { searchText: "changed", ack: 1 }));
  expect(search().value).toBe("ni");
  // Committed: the text is typed, and the extension answers that.
  type(search(), "你");
  composition(search(), "compositionend");
  show(withProps(list, { searchText: "你", ack: 2 }));
  expect(search().value).toBe("你");
});

test("a search bar the extension set from the start shows that", () => {
  show(node("List", { searchText: "preset", onSearchTextChange: fn, ack: 0 }));
  expect(search().value).toBe("preset");
});

test("coming back to a list finds its search text and selection as they were", () => {
  const items = ["Alice", "Bob", "Tom"].map((title) => node("List.Item", { title }));
  const list = node("List", {}, ...items);
  show(list);
  type(search(), "o");
  press(search(), "ArrowDown");
  const selected = () => $(".item.selected .title").textContent;
  expect(selected()).toBe("Tom");

  // Another list is pushed: it starts empty, at its top.
  show(node("List", {}, node("List.Item", { title: "One" }), node("List.Item", { title: "Other" })), 1);
  expect(search().value).toBe("");
  expect(selected()).toBe("One");
  type(search(), "ot");

  // And popped.
  show(list, 0);
  expect(search().value).toBe("o");
  expect(selected()).toBe("Tom");
  expect([...host.querySelectorAll(".item .title")].map((e) => e.textContent)).toEqual(["Bob", "Tom"]);

  // Pushed again, a view is new: nothing is remembered of the one popped.
  show(node("List", {}, node("List.Item", { title: "One" })), 2);
  expect(search().value).toBe("");
});

test("coming back does not jump to a selectedItemId that was already answered", () => {
  const items = ["a", "b", "c"].map((id) => node("List.Item", { id, title: id.toUpperCase() }));
  const list = node("List", { selectedItemId: "b" }, ...items);
  show(list);
  const selected = () => $(".item.selected .title").textContent;
  expect(selected()).toBe("B");
  press(search(), "ArrowDown");
  expect(selected()).toBe("C");
  show(node("Detail", { markdown: "pushed" }), 1);
  show(list, 0);
  expect(selected()).toBe("C");
  // Unless the extension now asks for another.
  show(node("Detail", { markdown: "pushed" }), 3);
  show(withProps(list, { selectedItemId: "a" }), 0);
  expect(selected()).toBe("A");
});

test("coming back to a search bar dropdown finds its selection", () => {
  const dropdown = () => $<HTMLSelectElement>("select.search-dropdown");
  const accessory = node("Dropdown", { tooltip: "Kind", onChange: fn, defaultValue: "a" }, node("Dropdown.Item", { value: "a", title: "A" }), node("Dropdown.Item", { value: "b", title: "B" }));
  const list = node("List", {}, slot("searchBarAccessory", accessory));
  show(list);
  expect(dropdown().value).toBe("a");
  act(() => {
    dropdown().value = "b";
    dropdown().dispatchEvent(new Event("change", { bubbles: true }));
  });
  expect(dispatched()).toEqual([[accessory.id, "onChange", "b"]]);
  show(node("Detail", { markdown: "pushed" }), 1);
  show(list, 0);
  expect(dropdown().value).toBe("b");
});

test("a form's text fields are typed into locally, and submitted as shown", () => {
  const name = node("Form.TextField", { id: "name", title: "Name", value: "", onChange: fn, ack: 0 });
  const notes = node("Form.TextArea", { id: "notes", defaultValue: "kept" });
  const submit = node("Action", { title: "Submit", submit: true, onSubmit: fn });
  const actions = slot("actions", node("ActionPanel", {}, submit));
  const base = node("Form", {}, name, notes, actions);
  const form = (fields: Node[]) => ({ ...base, children: [...fields, actions] });
  show(base);
  const input = () => $<HTMLInputElement>(`[data-node="${name.id}"]`);
  expect(document.activeElement).toBe(input());

  type(input(), "h");
  type(input(), "hi");
  expect(dispatched()).toEqual([
    [name.id, "onChange", "h", 1],
    [name.id, "onChange", "hi", 2],
  ]);
  // A late echo of the first key.
  show(form([withProps(name, { value: "h", ack: 1 }), notes]));
  expect(input().value).toBe("hi");
  // The extension capitalises.
  show(form([withProps(name, { value: "Hi", ack: 2 }), notes]));
  expect(input().value).toBe("Hi");

  heyo.forget();
  press(input(), "Enter");
  expect(dispatched()).toEqual([[submit.id, "onSubmit", { name: "Hi", notes: "kept" }]]);
});

test("coming back to a form finds what was filled in", () => {
  const name = node("Form.TextField", { id: "name" });
  const agree = node("Form.Checkbox", { id: "agree", label: "Agree" });
  const form = node("Form", {}, name, agree);
  show(form);
  type($<HTMLInputElement>(`[data-node="${name.id}"]`), "Ada");
  act(() => $<HTMLInputElement>(`[data-node="${agree.id}"]`).click());
  act(() => $<HTMLInputElement>(`[data-node="${agree.id}"]`).focus());
  show(node("Detail", { markdown: "pushed" }), 1);
  show(form, 0);
  expect($<HTMLInputElement>(`[data-node="${name.id}"]`).value).toBe("Ada");
  expect($<HTMLInputElement>(`[data-node="${agree.id}"]`).checked).toBe(true);
  // And the keyboard is where it was.
  expect(document.activeElement).toBe($(`[data-node="${agree.id}"]`));
});

test("a file picker asks the runtime to browse, and shows what was chosen there", () => {
  const picker = node("Form.FilePicker", { id: "files", allowMultipleSelection: true, onChange: fn, browse: fn });
  const submit = node("Action", { title: "Submit", submit: true, onSubmit: fn });
  const actions = slot("actions", node("ActionPanel", {}, submit));
  const form = node("Form", {}, picker, actions);
  show(form);
  const button = () => $<HTMLButtonElement>(".file-choose");
  expect(button().textContent).toBe("Choose Files…");
  act(() => button().click());
  expect(dispatched()).toEqual([[picker.id, "browse", []]]);

  // The browser is pushed over the form, something is chosen, and the form is back.
  show(node("List", {}), 1);
  heyo.forget();
  show({ ...form, children: [withProps(picker, { picked: { seq: 1, paths: ["/home/me/a.txt", "/home/me/dir/"] } }), actions] }, 0);
  expect([...host.querySelectorAll(".tag.file .file-name")].map((e) => e.textContent)).toEqual(["a.txt", "dir"]);
  expect(dispatched()).toEqual([[picker.id, "onChange", ["/home/me/a.txt", "/home/me/dir/"]]]);

  // One is taken out again; Enter on the button presses it rather than submitting.
  heyo.forget();
  act(() => $<HTMLButtonElement>(".file-remove").click());
  expect(dispatched()).toEqual([[picker.id, "onChange", ["/home/me/dir/"]]]);
  heyo.forget();
  press(button(), "Enter");
  expect(dispatched()).toEqual([]);
  press(button(), "Enter", { ctrlKey: true });
  expect(dispatched()).toEqual([[submit.id, "onSubmit", { files: ["/home/me/dir/"] }]]);
});

test("Action.PickDate asks for the date in the action panel", () => {
  const pick = node("Action", { title: "Due…", pickDate: { type: "date" }, onChange: fn });
  const list = node("List", {}, node("List.Item", { title: "Task" }, slot("actions", node("ActionPanel", {}, pick))));
  show(list);
  // The primary action: Enter opens the panel on the date, and nothing has run.
  press(search(), "Enter");
  const field = () => $<HTMLInputElement>(".date-panel input");
  expect(field().type).toBe("date");
  expect(document.activeElement).toBe(field());
  expect(dispatched()).toEqual([]);
  expect([...host.querySelectorAll(".date-choice .action-title")].map((e) => e.textContent)).toEqual(["Today", "Tomorrow", "Next Week", "No Date"]);

  type(field(), "2026-03-09");
  press(field(), "Enter");
  expect(dispatched()).toEqual([[pick.id, "onChange", { $date: new Date(2026, 2, 9).toISOString() }]]);
  expect(host.querySelector(".date-panel")).toBeNull();

  // Again, taking "No Date".
  heyo.forget();
  press(search(), "Enter");
  act(() => [...host.querySelectorAll<HTMLButtonElement>(".date-choice")].pop()!.click());
  expect(dispatched()).toEqual([[pick.id, "onChange", null]]);

  // Esc leaves without a date.
  heyo.forget();
  press(search(), "Enter");
  press(field(), "Escape");
  expect(host.querySelector(".date-panel")).toBeNull();
  expect(heyo.calls).toEqual([]);
});

test("Action.PickDate is reached from the list of actions too, and Esc goes back to it", () => {
  const open = node("Action", { title: "Open", onAction: fn });
  const pick = node("Action", { title: "Due…", pickDate: { type: "date_time", min: new Date(2026, 0, 1), max: new Date(2026, 11, 31, 23, 59) }, onChange: fn });
  show(node("List", {}, node("List.Item", { title: "Task" }, slot("actions", node("ActionPanel", {}, open, pick)))));
  press(search(), "k", { ctrlKey: true, code: "KeyK" });
  const rows = () => [...host.querySelectorAll<HTMLElement>(".action-panel .action-row:not(.date-choice)")];
  act(() => rows()[1].click());
  const field = () => $<HTMLInputElement>(".date-panel input");
  expect(field().type).toBe("datetime-local");
  expect([field().min, field().max]).toEqual(["2026-01-01T00:00", "2026-12-31T23:59"]);
  // A date outside what is allowed is not taken.
  type(field(), "2027-01-01T10:00");
  press(field(), "Enter");
  expect(dispatched()).toEqual([]);
  press(field(), "Escape");
  expect(host.querySelector(".date-panel")).toBeNull();
  expect(rows().map((row) => row.querySelector(".action-title")!.textContent)).toEqual(["Open", "Due…"]);
});

test("the panel lists an extension's actions in their sections, and a submenu opens in place", () => {
  const open = node("Action", { title: "Open", onAction: fn });
  const red = node("Action", { title: "Red", shortcut: { modifiers: ["cmd"], key: "r" }, onAction: fn });
  const blue = node("Action", { title: "Blue", onAction: fn });
  const colour = node("ActionPanel.Submenu", { title: "Colour" }, red, blue);
  const remove = node("Action", { title: "Delete", style: "destructive", onAction: fn });
  const panel = node("ActionPanel", { title: "Task" }, colour, open, node("ActionPanel.Section", { title: "Danger" }, remove));
  show(node("List", {}, node("List.Item", { title: "Task" }, slot("actions", panel))));
  const rows = () => [...host.querySelectorAll<HTMLElement>(".action-panel .action-row")];
  const titles = () => rows().map((row) => row.querySelector(".action-title")!.textContent);
  const filter = () => $<HTMLInputElement>(".action-search");

  // Enter is the first action that can run, which a submenu cannot.
  press(search(), "Enter");
  expect(dispatched()).toEqual([[open.id, "onAction"]]);
  expect($(".footer-button").textContent).toContain("Open");

  press(search(), "k", { ctrlKey: true, code: "KeyK" });
  expect($(".action-panel-title").textContent).toBe("Task");
  expect(titles()).toEqual(["Colour", "Open", "Delete"]);
  expect([...host.querySelectorAll(".action-panel .section-title")].map((e) => e.textContent)).toEqual(["Danger"]);
  expect(rows().map((row) => !!row.querySelector(".chevron"))).toEqual([true, false, false]);
  expect(rows().map((row) => row.classList.contains("destructive"))).toEqual([false, false, true]);
  type(filter(), "del");
  expect(titles()).toEqual(["Delete"]);
  type(filter(), "");

  // Into the submenu, back out with Esc, and in again to choose.
  press(filter(), "Enter");
  expect($(".action-panel-title").textContent).toBe("Colour");
  expect(titles()).toEqual(["Red", "Blue"]);
  press(filter(), "Escape");
  expect(titles()).toEqual(["Colour", "Open", "Delete"]);
  act(() => rows()[0].click());
  invoke.mockClear();
  act(() => rows()[1].click());
  expect(dispatched()).toEqual([[blue.id, "onAction"]]);
  expect(host.querySelector(".action-panel")).toBeNull();

  // An action in a submenu is reached by its shortcut without opening anything.
  invoke.mockClear();
  press(search(), "r", { ctrlKey: true, code: "KeyR" });
  expect(dispatched()).toEqual([[red.id, "onAction"]]);
});

test("Quick Look is given the selected item's file", () => {
  const look = (id: number) => ({ id, type: "Action", props: { title: "Quick Look", preview: true, shortcut: { modifiers: ["cmd"], key: "y" }, onAction: fn }, children: [] });
  const item = (title: string, path: string) => {
    const action = look(nextId++);
    return { action, node: node("List.Item", { title, quickLook: { path } }, slot("actions", node("ActionPanel", {}, node("Action", { title: "Open", onAction: fn }), action))) };
  };
  const first = item("One", "/tmp/one.png");
  const second = item("Two", "/tmp/two.txt");
  show(node("List", {}, first.node, second.node));
  press(search(), "y", { ctrlKey: true, code: "KeyY" });
  press(search(), "ArrowDown");
  press(search(), "y", { ctrlKey: true, code: "KeyY" });
  expect(dispatched()).toEqual([
    [first.action.id, "onAction", { path: "/tmp/one.png" }],
    [second.action.id, "onAction", { path: "/tmp/two.txt" }],
  ]);
});

test("an emoji of many code points is drawn as text, not loaded as an image", () => {
  const family = "👨‍👩‍👧‍👦";
  show(node("Grid", {}, node("Grid.Item", { content: family }), node("Grid.Item", { content: "🏴󠁧󠁢󠁷󠁬󠁳󠁿" })));
  expect(host.querySelectorAll("img").length).toBe(0);
  expect([...host.querySelectorAll(".grid-image.glyph")].map((e) => e.textContent)).toEqual([family, "🏴󠁧󠁢󠁷󠁬󠁳󠁿"]);
});
