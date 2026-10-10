import { describe, expect, it } from "vitest";

import { createObserver, hasContent, readStack } from "./observe.mjs";

const node = (id: number, type: string, props: object = {}, children: number[] = []) => ({ id, type, props, children });
const render = (view: object | null) => ({ type: "render", session: 1, view });

describe("what a view holds", () => {
  it("counts the items of a list and names a few", () => {
    const o = createObserver();
    o.message({ type: "launched", session: 1 });
    o.message(
      render({
        key: 1,
        depth: 1,
        full: true,
        root: [1],
        removed: [],
        nodes: [
          node(1, "List", { isLoading: true, onSearchTextChange: { $fn: 1 } }, [2]),
          node(2, "List.Section", { title: "Recent" }, [3, 4]),
          node(3, "List.Item", { title: "First" }, [5]),
          node(4, "List.Item", { title: { value: "Second" } }),
          node(5, "slot", { name: "actions" }, [6]),
          node(6, "ActionPanel", {}, [7]),
          node(7, "Action.OpenInBrowser", { url: "https://example.com" }),
        ],
      }),
    );
    expect(o.state.view).toMatchObject({ root: "List", isLoading: true, items: 2, sections: 1, actions: 1, titles: ["First", "Second"], searchable: true });
    expect(hasContent(o.state.view)).toBe(true);
  });

  it("applies a render that only carries what changed", () => {
    const o = createObserver();
    o.message(render({ key: 1, depth: 1, full: true, root: [1], removed: [], nodes: [node(1, "List", { isLoading: true }, [2]), node(2, "List.Item", { title: "a" })] }));
    o.message(render({ key: 1, depth: 1, full: false, root: [1], removed: [2], nodes: [node(1, "List", { isLoading: false }, [3]), node(3, "List.EmptyView", { title: "Nothing here" })] }));
    expect(o.state.view).toMatchObject({ isLoading: false, items: 0, emptyView: "Nothing here" });
    expect(hasContent(o.state.view)).toBe(false);
    expect(o.state.renders).toBe(2);
  });

  it("says a detail has content when it has text, a form when it has fields", () => {
    const o = createObserver();
    o.message(render({ key: 1, depth: 1, full: true, root: [1], removed: [], nodes: [node(1, "Detail", { markdown: "# Hello" })] }));
    expect(o.state.view).toMatchObject({ root: "Detail", markdown: 7, excerpt: "# Hello" });
    expect(hasContent(o.state.view)).toBe(true);
    o.message(render({ key: 2, depth: 2, full: true, root: [5], removed: [], nodes: [node(5, "Form", {}, [6, 7]), node(6, "Form.TextField", { id: "a" }), node(7, "Form.Description", { text: "x" })] }));
    expect(o.state.view).toMatchObject({ root: "Form", fields: 1, depth: 2 });
    expect(hasContent(o.state.view)).toBe(true);
  });

  it("sees no view where a command without one has its empty place", () => {
    const o = createObserver();
    o.message(render({ key: 1, depth: 1, full: true, root: [], removed: [], nodes: [] }));
    expect(o.state.view).toBeNull();
    o.message(render(null));
    expect(o.state.view).toBeNull();
  });

  it("keeps a toast as it last was, and knows when the command ended", () => {
    const o = createObserver();
    o.message({ type: "toast", toast: { id: 1, style: "ANIMATED", title: "Loading" } });
    o.message({ type: "toast", toast: { id: 1, style: "FAILURE", title: "Failed", message: "No such thing" } });
    o.message({ type: "toast", toast: { id: 2, style: "SUCCESS", title: "Copied" } });
    expect(o.state.toasts).toEqual([
      { style: "FAILURE", title: "Failed", message: "No such thing" },
      { style: "SUCCESS", title: "Copied", message: undefined },
    ]);
    expect(o.state.closed).toBe(false);
    o.message({ type: "closed", session: 1 });
    expect(o.state.closed).toBe(true);
  });

  it("counts what a menu bar command put in its menu", () => {
    const o = createObserver();
    o.message({ type: "menu", key: "x", menu: { title: "3", items: [{ kind: "item", title: "a" }, { kind: "section", children: [{ kind: "item" }, { kind: "separator" }] }, { kind: "submenu", children: [{ kind: "item" }] }] } });
    o.message({ type: "background-done", id: 1 });
    expect(o.state.menu).toEqual({ title: "3", items: 3 });
    expect(o.state.done).toBe(true);
    expect(o.state.doneError).toBeUndefined();
  });
});

describe("where an error came from", () => {
  const where = { extensionDir: "/work/extensions/demo", runtimeFile: "/heyo/heyo-runtime.cjs" };

  it("finds the first frame inside the extension", () => {
    const stderr = [
      "some log line",
      "TypeError: Cannot read properties of undefined (reading 'map')",
      "    at Object.useMemo (/heyo/heyo-runtime.cjs:100:5)",
      "    at Command (/work/extensions/demo/index.js:42:17)",
      "    at node:internal/process/task_queues:95:5",
      "",
    ].join("\n");
    expect(readStack(stderr, "Cannot read properties of undefined (reading 'map')", where)).toEqual({
      raisedIn: "runtime",
      top: "Object.useMemo (/heyo/heyo-runtime.cjs:100:5)",
      frame: "Command (index.js:42:17)",
    });
  });

  it("says an error was raised in the extension when its stack starts there", () => {
    const stderr = "Error: spawn osascript ENOENT\n    at run (/work/extensions/demo/go.js:1:10)\n    at /heyo/heyo-runtime.cjs:5:5\n";
    expect(readStack(stderr, "spawn osascript ENOENT", where)).toMatchObject({ raisedIn: "extension", frame: "run (go.js:1:10)" });
  });

  it("has nothing to say without a stack", () => {
    expect(readStack("just a line\n", "boom", where)).toBeUndefined();
  });
});
