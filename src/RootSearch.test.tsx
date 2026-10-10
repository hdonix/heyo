// @vitest-environment happy-dom
// The root search with the host played by a table of answers: arguments in
// the search bar, and the selection through a refresh.
import { act, createRef, useState } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, expect, test, vi } from "vitest";

import type { Item } from "../shared/bindings/Item";

const host = vi.hoisted(() => ({ results: [] as unknown[], calls: [] as [string, any][] }));
vi.mock("@tauri-apps/api/core", () => ({
  convertFileSrc: (path: string) => path,
  invoke: async (command: string, args: unknown) => {
    host.calls.push([command, args]);
    return command === "search" ? host.results : command === "list_quicklinks" ? [] : undefined;
  },
}));
vi.mock("./calculator", () => ({ calculate: () => null, loadRates: async () => null }));

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
    />
  );
}

const settle = () => act(() => new Promise<void>((resolve) => setTimeout(resolve, 5)));

async function show(results: Item[]) {
  host.results = results;
  host.calls.length = 0;
  container = document.body.appendChild(document.createElement("div"));
  root = createRoot(container);
  await act(async () => root.render(<Harness />));
  await settle();
  inputRef.current!.focus();
}

beforeEach(() => vi.spyOn(console, "error").mockImplementation(() => {}));
afterEach(() => {
  act(() => root.unmount());
  container.remove();
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

test("a command without arguments launches as before", async () => {
  await show([item("cmd:/ext:notes")]);
  await press("Enter");
  await settle();
  expect(launches()).toEqual([{ id: "cmd:/ext:notes", arguments: undefined }]);
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
