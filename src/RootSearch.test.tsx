// @vitest-environment happy-dom
// The root search with the host played by a table of answers: arguments in
// the search bar, the selection through a refresh, and which row a key or a
// click is for. What each kind of row then does is in rows.test.ts.
import { act, createRef, useState } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, expect, test, vi } from "vitest";

import type { Item } from "../shared/bindings/Item";

const host = vi.hoisted(() => ({ results: [] as unknown[], activities: [] as unknown[], calc: null as unknown, calls: [] as [string, any][] }));
vi.mock("./calculator", () => ({ calculate: () => host.calc, loadRates: async () => null }));

import type { ActivityView } from "../shared/bindings/ActivityView";
import { fakeHost } from "./host.fake";
import { RootSearch } from "./RootSearch";

(globalThis as any).IS_REACT_ACT_ENVIRONMENT = true;

const item = (id: string, rest: Partial<Item> = {}): Item => ({ id, title: id, subtitle: null, icon: "box-16", kind: "Command", favorite: false, ...rest });
const weather = item("cmd:/ext:weather", {
  title: "Weather",
  arguments: [
    { name: "city", type: "text", placeholder: "City", required: true },
    { name: "key", type: "password", placeholder: "API key", required: false },
    {
      name: "unit",
      type: "dropdown",
      placeholder: "Unit",
      required: false,
      data: [
        { title: "Celsius", value: "c" },
        { title: "Fahrenheit", value: "f" },
      ],
    },
  ],
});
const github = item("quicklink:gh", { title: "GitHub", kind: "Quicklink", link: "https://github.com/search?q={query}", alias: "gh" });

let root: Root;
let container: HTMLElement;
let refresh: () => void;
/// What the palette is shown with, where a test wants other than the usual.
let given: Partial<React.ComponentProps<typeof RootSearch>> = {};
const inputRef = createRef<HTMLInputElement>();

function Harness() {
  const [query, setQuery] = useState("");
  const [version, setVersion] = useState(0);
  refresh = () => setVersion((v) => v + 1);
  return (
    <RootSearch
      query={query}
      setQuery={setQuery}
      inputRef={inputRef}
      version={version}
      shown={0}
      toast={null}
      onToastAction={() => {}}
      showError={(title, message) => {
        throw new Error(`${title}: ${message}`);
      }}
      showSuccess={() => {}}
      openSettings={() => {}}
      compact={false}
      pendingOpen={null}
      onPendingHandled={() => {}}
      dialog={null}
      {...given}
    />
  );
}

const settle = () => act(() => new Promise<void>((resolve) => setTimeout(resolve, 5)));

async function show(results: Item[], activities: ActivityView[] = []) {
  host.results = results;
  host.activities = activities;
  host.calls.length = 0;
  container = document.body.appendChild(document.createElement("div"));
  root = createRoot(container);
  await act(async () => root.render(<Harness />));
  await settle();
  inputRef.current!.focus();
}

beforeEach(() => {
  vi.spyOn(console, "error").mockImplementation(() => {});
  const found = () => host.results as Item[];
  host.calls = fakeHost({
    search: found,
    list_quicklinks: [],
    activities: () => host.activities as ActivityView[],
    get_item: ({ id }) => found().find((item) => item.id === id) ?? null,
  }).calls as [string, any][];
});
afterEach(() => {
  act(() => root.unmount());
  container.remove();
  host.calc = null;
  given = {};
  vi.restoreAllMocks();
});

const press = (key: string, init: KeyboardEventInit = {}) =>
  act(async () => {
    document.activeElement!.dispatchEvent(new KeyboardEvent("keydown", { key, bubbles: true, cancelable: true, ...init }));
  });

/// Types into a field the way the web view reports it to React.
const type = (field: HTMLInputElement, text: string) =>
  act(async () => {
    Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value")!.set!.call(field, text);
    field.dispatchEvent(new Event("input", { bubbles: true }));
  });

const fields = () => [...container.querySelectorAll<HTMLElement>(".search-row .argument")];
const selectedTitle = () => container.querySelector(".item.selected .title")?.textContent;
const launches = () => host.calls.filter(([command]) => command === "launch").map(([, args]) => args);

test("a command's arguments are fields in the search bar, reached with Tab", async () => {
  await show([weather, item("cmd:/ext:notes")]);
  expect(fields().map((f) => f.getAttribute("placeholder") ?? f.textContent)).toEqual(["City", "API key", "Unit"]);
  expect((fields()[1] as HTMLInputElement).type).toBe("password");
  // Tab is the fields' here, not Ask AI's.
  expect(container.querySelector(".search-hint")).toBeNull();

  await press("Tab");
  expect(document.activeElement).toBe(fields()[0]);
  await press("Tab");
  expect(document.activeElement).toBe(fields()[1]);
  await press("Tab", { shiftKey: true });
  expect(document.activeElement).toBe(fields()[0]);
  await press("Escape");
  expect(document.activeElement).toBe(inputRef.current);
  // An item that asks for nothing has no fields.
  await press("ArrowDown");
  expect(fields()).toEqual([]);
  expect(container.querySelector(".search-hint")).not.toBeNull();
});

test("Enter waits for the required arguments, then launches with all of them", async () => {
  await show([weather]);
  await press("Enter");
  await settle();
  expect(launches()).toEqual([]);
  // The empty required field is marked and has the focus.
  expect(document.activeElement).toBe(fields()[0]);
  expect(fields()[0].classList.contains("missing")).toBe(true);

  await type(fields()[0] as HTMLInputElement, "Oslo");
  await press("Tab");
  await type(fields()[1] as HTMLInputElement, "s3cret");
  await press("Tab");
  // The dropdown: the arrow keys choose, and the list of items stays put.
  await press("ArrowDown");
  await press("ArrowDown");
  expect(fields()[2].textContent).toBe("Fahrenheit");
  expect(selectedTitle()).toBe("Weather");

  await press("Enter");
  await settle();
  expect(launches()).toEqual([{ id: weather.id, arguments: { city: "Oslo", key: "s3cret", unit: "f" } }]);
});

test("Enter while an input method composes does not launch", async () => {
  await show([weather]);
  await press("Tab");
  await type(fields()[0] as HTMLInputElement, "ao");
  await press("Enter", { isComposing: true });
  await settle();
  expect(launches()).toEqual([]);
  await press("Enter");
  await settle();
  expect(launches()).toHaveLength(1);
});

test("a quicklink's query is such a field, filled by what follows its alias", async () => {
  await show([github]);
  expect(fields().map((f) => f.getAttribute("placeholder"))).toEqual(["Query"]);
  await press("Enter");
  await settle();
  // Nothing opened without the query: its field has the focus.
  expect(host.calls.some(([command]) => command === "open_target")).toBe(false);
  expect(document.activeElement).toBe(fields()[0]);

  await type(inputRef.current!, "gh heyo launcher");
  await settle();
  expect((fields()[0] as HTMLInputElement).value).toBe("heyo launcher");
  inputRef.current!.focus();
  await press("Enter");
  await settle();
  expect(host.calls.find(([command]) => command === "open_target")?.[1]).toEqual({ target: "https://github.com/search?q=heyo%20launcher" });
});

test("a quicklink opened from outside asks for its query, and the search field has the keyboard after", async () => {
  given = { pendingOpen: github.id };
  await show([github]);
  const asked = container.querySelector<HTMLInputElement>(".prompt input")!;
  asked.focus();
  await type(asked, "heyo");
  await press("Enter");
  await settle();
  expect(host.calls.find(([command]) => command === "open_target")?.[1]).toEqual({ target: "https://github.com/search?q=heyo" });
  expect(container.querySelector(".prompt")).toBeNull();
  expect(document.activeElement).toBe(inputRef.current);
});

test("the selected row stays selected when the results are fetched again", async () => {
  const [a, b, c] = [item("a"), item("b"), item("c")];
  await show([a, b, c]);
  expect(selectedTitle()).toBe("a");
  await press("ArrowDown");
  await press("ArrowDown");
  expect(selectedTitle()).toBe("c");

  // The index reloaded: something new came first.
  host.results = [item("new"), a, b, c];
  await act(async () => refresh());
  await settle();
  expect(selectedTitle()).toBe("c");

  // The selected item is gone: the row in its place.
  host.results = [item("new"), a, b, item("d")];
  await act(async () => refresh());
  await settle();
  expect(selectedTitle()).toBe("d");
  host.results = [a];
  await act(async () => refresh());
  await settle();
  expect(selectedTitle()).toBe("a");
});

test("with nothing chosen, the first result stays the selection through a refresh", async () => {
  await show([item("a"), item("b")]);
  host.results = [item("new"), item("a"), item("b")];
  await act(async () => refresh());
  await settle();
  expect(selectedTitle()).toBe("new");
});

test("a new query starts from the first result again", async () => {
  await show([item("a"), item("b"), item("c")]);
  await press("ArrowDown");
  expect(selectedTitle()).toBe("b");
  await type(inputRef.current!, "x");
  await settle();
  expect(selectedTitle()).toBe("a");
});

test("what is typed in a field, and the focus in it, survive a refresh", async () => {
  await show([item("a"), weather]);
  await press("ArrowDown");
  await press("Tab");
  await type(fields()[0] as HTMLInputElement, "Oslo");
  host.results = [item("new"), item("a"), weather];
  await act(async () => refresh());
  await settle();
  expect(selectedTitle()).toBe("Weather");
  expect((fields()[0] as HTMLInputElement).value).toBe("Oslo");
  expect(document.activeElement).toBe(fields()[0]);
});

test("with no row there is nothing the action panel could be named after", async () => {
  await show([]);
  expect(container.querySelector(".empty")?.textContent).toBe("No results");
  await act(async () => [...container.querySelectorAll<HTMLElement>(".footer-button")].pop()!.click());
  expect(container.querySelector(".action-panel")?.textContent).toContain("No actions");
  expect(container.querySelector(".action-panel-title")).toBeNull();
});

test("a click on the calculator does what the calculator does, whichever row is selected", async () => {
  host.calc = { question: "6 × 7", answer: "42", value: "42", label: "Result" };
  await show([item("a")]);
  await press("ArrowDown");
  expect(selectedTitle()).toBe("a");
  await act(async () => container.querySelector<HTMLElement>(".calc-row")!.click());
  await settle();
  expect(launches()).toEqual([]);
  expect(host.calls.filter(([command]) => command === "copy_text").map(([, args]) => args)).toEqual([{ text: "42", paste: false }]);
});

test("a click on a row is for that row, and the footer's button for the selected one", async () => {
  await show([item("a"), item("b")]);
  await type(inputRef.current!, "cats");
  await settle();
  expect([...container.querySelectorAll(".section-title")].map((e) => e.textContent)).toEqual(["Use “cats” with…"]);
  const titled = (title: string) => [...container.querySelectorAll<HTMLElement>(".item")].find((row) => row.querySelector(".title")?.textContent === title)!;
  await act(async () => titled("Search Google").click());
  await act(async () => titled("b").click());
  await settle();
  expect(host.calls.filter(([command]) => command === "open_target").map(([, args]) => args)).toEqual([{ target: "https://www.google.com/search?q=cats" }]);
  expect(launches()).toEqual([{ id: "b", arguments: undefined }]);

  expect(selectedTitle()).toBe("a");
  const primary = container.querySelector<HTMLElement>(".footer-button")!;
  expect(primary.textContent).toContain("Open Command");
  await act(async () => primary.click());
  await settle();
  expect(launches().pop()).toEqual({ id: "a", arguments: undefined });
});

test("an action's shortcut runs it for the selected row, without the panel", async () => {
  await show([item("a"), item("b")]);
  await press("ArrowDown");
  await press("f", { ctrlKey: true, shiftKey: true, code: "KeyF" });
  await settle();
  expect(host.calls.filter(([command]) => command === "set_favorite").map(([, args]) => args)).toEqual([{ id: "b", favorite: true }]);
  expect(container.querySelector(".action-panel")).toBeNull();
});

test("folded away, the palette is the search field alone until something is typed", async () => {
  given = { compact: true };
  await show([github]);
  expect(container.querySelector(".panel.compact")).not.toBeNull();
  expect(container.querySelector(".list")).toBeNull();
  // The first result is still what Enter opens, with no field to fill in: it asks.
  expect(fields()).toEqual([]);
  await press("Enter");
  await settle();
  expect(container.querySelector(".prompt .alert-title")?.textContent).toBe("GitHub");
  await press("Escape");

  await type(inputRef.current!, "g");
  await settle();
  expect(container.querySelector(".panel.compact")).toBeNull();
  expect(selectedTitle()).toBe("GitHub");
});

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

const acted = () => host.calls.filter(([command]) => command === "activity_action").map(([, args]) => args);
const fetched = () => host.calls.filter(([command]) => command === "activities").length;

test("what is in progress comes first, and Enter is for the row it is on", async () => {
  const build = activity("ci/build", { name: "Build", title: "Building", extensionTitle: "CI", compact: "3/9", hasAction: true, buttons: [] });
  await show([item("a"), item("b")], [activity("@heyo/timer-1"), build]);
  expect(fetched()).toBeGreaterThan(0);
  expect([...container.querySelectorAll(".item .title")].map((e) => e.textContent)).toEqual(["25 min", "Building", "a", "b"]);
  expect([...container.querySelectorAll(".section-title")].map((e) => e.textContent)).toEqual(["In Progress", "Results"]);
  expect(container.querySelector(".activity-value")?.textContent).toBe("24:59");
  expect(selectedTitle()).toBe("25 min");

  await press("Enter");
  await settle();
  expect(acted()).toEqual([{ key: "@heyo/timer-1", button: 0 }]);
  // The timer's text is asked for again once a second while it is in view.
  const before = fetched();
  await act(() => new Promise<void>((resolve) => setTimeout(resolve, 1100)));
  expect(fetched()).toBeGreaterThan(before);
});

test("the action panel is the selected row's, and runs what is chosen in it", async () => {
  await show([item("a")], [activity("@heyo/timer-1")]);
  await press("k", { ctrlKey: true, code: "KeyK" });
  await settle();
  expect(container.querySelector(".action-panel-title")?.textContent).toBe("25 min");
  const actions = () => [...container.querySelectorAll<HTMLElement>(".action-panel .action-row")];
  expect(actions().map((row) => row.querySelector(".action-title")?.textContent)).toEqual(["Pause", "+1 min", "End Activity"]);

  await act(async () => actions()[2].click());
  await settle();
  expect(host.calls.filter(([command]) => command === "activity_dismiss").map(([, args]) => args)).toEqual([{ key: "@heyo/timer-1" }]);
  expect(container.querySelector(".action-panel")).toBeNull();
  expect(document.activeElement).toBe(inputRef.current);
});
