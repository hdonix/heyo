// The runtime started as `main.ts` starts it, with the test where the
// host is: what each of the host's messages does, and what the runtime
// lets pass because it came too late or for something that is gone.
import { beforeAll, beforeEach, expect, test, vi } from "vitest";

import { answers, extension, receive, requests, reset, runtime, sent, sleep, until } from "./testing";

import { environment } from "./api";
import { close } from "./session";

const world = globalThis as any;

/// A view that says who it is, with an action, and tells when it is left.
const view = (who: string) => `
  const { Action, ActionPanel, Detail } = require("@heyo/api");
  const React = require("react");
  exports.default = () => {
    React.useEffect(() => () => void globalThis.seen.push("${who} left"), []);
    const act = () => void globalThis.seen.push("${who} acted");
    return React.createElement(Detail, { markdown: "${who}", actions: React.createElement(ActionPanel, null, React.createElement(Action, { title: "Act", onAction: act })) });
  };
`;

const launched = () => sent.filter((m) => m.type === "launched");
const views = (session: number) => sent.filter((m) => m.type === "render" && m.session === session && m.view).map((m) => m.view);
const toasts = () => sent.filter((m) => m.type === "toast").map((m) => m.toast);

/// Has the host launch a command, and waits for its view: the session, and
/// the node of the view's action.
async function launch(dir: string, command: string) {
  const before = launched().length;
  receive({ type: "launch", extension: dir, command });
  const session: number = (await until(() => launched()[before], `${command} to be launched`)).session;
  const nodes = await until(() => views(session)[0]?.nodes, `the view of ${command}`);
  return { session, action: nodes.find((node: { type: string }) => node.type === "Action")?.id as number };
}

/// As the webview does when the user chooses the action.
const act = (session: number, node: number) => receive({ type: "dispatch", session, node, prop: "onAction", args: [] });

beforeAll(async () => {
  await runtime();
  // Wired, in conversation, and saying so.
  expect(sent).toEqual([{ type: "ready" }]);
});

beforeEach(async () => {
  close();
  await sleep(30);
  reset();
  world.seen = [];
});

test("a launch is answered with the command, then with its view", async () => {
  const { dir } = extension("alpha", { show: { mode: "view", source: view("alpha") } });
  const { session, action } = await launch(dir, "show");
  expect(sent[0]).toEqual({ type: "launched", session, mode: "view", title: "alpha show", extension: { name: "alpha", title: "alpha", dir } });
  expect(sent[1]).toMatchObject({ type: "render", session, view: { full: true, depth: 1 } });
  expect(views(session)[0].nodes[0]).toMatchObject({ type: "Detail", props: { markdown: "alpha" } });

  act(session, action);
  expect(world.seen).toEqual(["alpha acted"]);
});

test("another language is spoken by the command on screen, and by the next one's name", async () => {
  const copy = `
    const { Action, ActionPanel, Detail } = require("@heyo/api");
    const React = require("react");
    exports.default = () => {
      const [count] = React.useState(() => globalThis.seen.push("started"));
      return React.createElement(Detail, { markdown: String(count), actions: React.createElement(ActionPanel, null, React.createElement(Action.CopyToClipboard, { content: "x" })) });
    };
  `;
  const { dir } = extension("polyglot", { show: { mode: "view", source: copy } });
  const { session, action } = await launch(dir, "show");
  const titles = () => views(session).flatMap((view) => view.nodes.filter((node: { id: number }) => node.id === action).map((node: { props: { title: string } }) => node.props.title));
  expect(titles()).toEqual(["Copy to Clipboard"]);

  const chinese = { language: "zh_CN", plural: [0], messages: { "Copy to Clipboard": ["复制到剪贴板"], "Switch Windows": ["切换窗口"] } };
  receive({ type: "language", translations: chinese });
  await until(() => titles().length > 1, "the view to be drawn again");
  expect(titles()).toEqual(["Copy to Clipboard", "复制到剪贴板"]);
  // Drawn again, not started again: what it held it still holds.
  expect(world.seen).toEqual(["started"]);

  answers["ext.windows.list"] = () => [];
  answers["host.applications"] = () => [];
  receive({ type: "launch", extension: "@heyo", command: "switch-windows" });
  expect((await until(() => launched()[1], "the command to be launched")).title).toBe("切换窗口");
  receive({ type: "language", translations: { language: "en", plural: [], messages: {} } });
});

test("one of Heyo's own commands is launched by Heyo's name, and asks under it", async () => {
  answers["ext.windows.list"] = () => [
    { id: 7, title: "Notes", class: "notes", active: true },
    { id: 8, title: "Inbox", class: "mail", active: false },
  ];
  answers["host.applications"] = () => [];
  receive({ type: "launch", extension: "@heyo", command: "switch-windows" });
  const session: number = (await until(() => launched()[0], "the command to be launched")).session;
  expect(launched()[0]).toMatchObject({ title: "Switch Windows", extension: { name: "@heyo", dir: "" } });
  const rows = () => views(session).flatMap((shown) => shown.nodes.filter((node: { type: string }) => node.type === "List.Item"));
  await until(() => rows().length >= 2, "the windows to be shown");
  expect(requests.find((r) => r.method === "ext.windows.list")).toEqual({ method: "ext.windows.list", extension: "@heyo", dir: "" });

  // What the user chooses runs as the command, whatever is running by then.
  const actions = views(session).flatMap((shown) => shown.nodes.filter((node: any) => node.type === "Action" && node.props.title === "Switch to Window"));
  act(session, actions[0].id);
  const activate = await until(() => requests.find((r) => r.method === "ext.windows.activate"), "the window to be switched to");
  expect(activate).toEqual({ method: "ext.windows.activate", extension: "@heyo", dir: "", params: { id: 8 } });
});

test("a launch that cannot be is told as closed, and leaves the command on screen", async () => {
  const { dir } = extension("alpha", { show: { mode: "view", source: view("alpha") }, bar: { mode: "menu-bar", source: view("bar") } });
  const { session, action } = await launch(dir, "show");
  reset();
  const quiet = vi.spyOn(console, "error").mockImplementation(() => {});

  // No such extension, no such command, and one that is not the palette's.
  receive({ type: "launch", extension: `${dir}-gone`, command: "show" });
  receive({ type: "launch", extension: dir, command: "nowhere" });
  receive({ type: "launch", extension: dir, command: "bar" });
  quiet.mockRestore();
  // Without a session: none began.
  expect(sent.filter((m) => m.type !== "toast")).toEqual([{ type: "closed" }, { type: "closed" }, { type: "closed" }]);
  expect(toasts()).toMatchObject([
    { style: "FAILURE", title: "Error", message: expect.stringContaining("ENOENT") },
    { style: "FAILURE", title: "Error", message: "Unknown command alpha/nowhere" },
    { style: "FAILURE", title: "Error", message: "Menu bar commands run in the menu bar, not in the palette" },
  ]);

  expect(world.seen).toEqual([]);
  act(session, action);
  expect(world.seen).toEqual(["alpha acted"]);
});

test("a close for an earlier session leaves the one that came after", async () => {
  const alpha = extension("alpha", { show: { mode: "view", source: view("alpha") } });
  const beta = extension("beta", { show: { mode: "view", source: view("beta") } });
  const first = await launch(alpha.dir, "show");
  const second = await launch(beta.dir, "show");
  expect(world.seen).toEqual(["alpha left"]);

  // The webview closed the first while the second was on its way.
  receive({ type: "close", session: first.session });
  await sleep(20);
  expect(world.seen).toEqual(["alpha left"]);
  act(second.session, second.action);
  expect(world.seen).toEqual(["alpha left", "beta acted"]);

  // Its own close, or one that names none, closes it. The user left: the
  // webview is not told what it said itself.
  receive({ type: "close", session: second.session });
  await until(() => world.seen.includes("beta left"), "the second command to be left");
  const third = await launch(alpha.dir, "show");
  receive({ type: "close" });
  await until(() => world.seen.filter((line: string) => line === "alpha left").length === 2, "the third command to be left");
  expect(sent.some((m) => m.type === "closed")).toBe(false);
  act(third.session, third.action);
  expect(world.seen.filter((line: string) => line.endsWith("acted"))).toEqual(["beta acted"]);
});

test("input for a session that is over, or a node that is gone, does nothing", async () => {
  const alpha = extension("alpha", { show: { mode: "view", source: view("alpha") } });
  const beta = extension("beta", { show: { mode: "view", source: view("beta") } });
  const first = await launch(alpha.dir, "show");
  const second = await launch(beta.dir, "show");

  // Sent while the first command was on screen, read after the second began.
  act(first.session, first.action);
  act(first.session, second.action);
  act(second.session, first.action);
  act(second.session, 987_654);
  receive({ type: "pop", session: first.session });
  await sleep(20);
  expect(world.seen).toEqual(["alpha left"]);
  expect(toasts()).toEqual([]);
  expect(sent.some((m) => m.type === "closed")).toBe(false);

  // Popping its only view is leaving the command, which the webview is told.
  receive({ type: "pop", session: second.session });
  await until(() => sent.some((m) => m.type === "closed"), "the command to close");
  expect(sent.find((m) => m.type === "closed")).toEqual({ type: "closed", session: second.session });
});

test("what an action throws, now or later, is shown and ends nothing", async () => {
  const { dir } = extension("clumsy", {
    show: {
      mode: "view",
      source: `
        const { Action, ActionPanel, Detail } = require("@heyo/api");
        const React = require("react");
        const actions = React.createElement(ActionPanel, null,
          React.createElement(Action, { title: "Now", onAction: () => { throw new Error("at once"); } }),
          React.createElement(Action, { title: "Later", onAction: async () => { throw new Error("in time"); } }));
        exports.default = () => React.createElement(Detail, { markdown: "clumsy", actions });`,
    },
  });
  const { session } = await launch(dir, "show");
  const [now, later] = views(session)[0].nodes.filter((node: { type: string }) => node.type === "Action");
  const quiet = vi.spyOn(console, "error").mockImplementation(() => {});
  act(session, now.id);
  act(session, later.id);
  await until(() => toasts().length === 2, "both failures to be shown");
  quiet.mockRestore();
  expect(toasts()).toMatchObject([
    { style: "FAILURE", title: "Error", message: "at once" },
    { style: "FAILURE", title: "Error", message: "in time" },
  ]);
  expect(sent.some((m) => m.type === "closed")).toBe(false);
});

test("the appearance is what the host last said", () => {
  receive({ type: "appearance", dark: false });
  expect(environment.appearance).toBe("light");
  expect(environment.theme).toBe("light");
  receive({ type: "appearance", dark: true });
  expect(environment.appearance).toBe("dark");
});

test("which process it is decides whether it runs what the host has run in the background", async () => {
  const run = (dir: string) => ({ type: "background-run", id: 9, extension: dir, command: "work", launchType: "background", action: null }) as const;
  const work = { work: { mode: "no-view" as const, source: `exports.default = async () => void globalThis.seen.push("worked");` } };

  // The palette's process is not asked to, and would not.
  receive(run(extension("own", work).dir));
  await sleep(50);
  expect(world.seen).toEqual([]);
  expect(sent).toEqual([]);

  // A process is one or the other for as long as it lives: the other one
  // is another, loaded anew.
  vi.resetModules();
  const other = await import("./testing");
  await other.runtime({ background: true });
  other.receive(run(other.extension("own", work).dir));
  await other.until(() => other.sent.find((m) => m.type === "background-done"), "the run to be done");
  expect(world.seen).toEqual(["worked"]);
  expect(other.sent).toEqual([{ type: "ready" }, { type: "background-done", id: 9 }]);
});

test("an error with nobody to show it to is printed, and is not another error", async () => {
  // Before the conversation began, or after the host hung up. `main.ts`
  // reports what nothing caught, so a report that failed would be reported.
  vi.resetModules();
  const { reportError } = await import("./session");
  const printed = vi.spyOn(console, "error").mockImplementation(() => {});
  const uncaught: unknown[] = [];
  const note = (error: unknown) => void uncaught.push(error);
  process.on("unhandledRejection", note);
  reportError(new Error("lost"));
  await sleep(20);
  process.off("unhandledRejection", note);
  expect(printed.mock.calls).toEqual([[new Error("lost")]]);
  printed.mockRestore();
  expect(uncaught).toEqual([]);
});
