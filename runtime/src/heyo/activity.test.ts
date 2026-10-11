// `LiveActivity` as the runtime provides it: what each call asks of the
// host, with dates as milliseconds and a sequence number that only grows;
// in whose name a handle acts; and how events reach a listening command.
import { beforeAll, beforeEach, expect, test } from "vitest";

import { answers, event, extension, requests, reset, runtime, sleep, until } from "../testing";

import { ended, runAs, type Current } from "../context";
import { LiveActivity, sequence, wireContent, type Event } from "./activity";
import api from "./index";

const as = (name: string, launchType = "userInitiated"): Current => ({ extension: extension(name, {}), command: "main", launchType });
const last = () => requests.at(-1)!;
const sent = (method: string) => requests.filter((r) => r.method === method);

beforeAll(() => runtime());

beforeEach(() => {
  reset();
  answers["ext.activity.start"] = (params) => ({ id: params.attributes.id });
  answers["ext.activity.list"] = () => ["a", "b"];
  answers["ext.activity.get"] = (params) => params.id === "a";
  answers["ext.activity.onEvent"] = () => ({ subscription: 11 });
});

test("the sequence number is the time and never repeats", () => {
  const first = sequence(1000);
  expect(sequence(1000)).toBe(first + 1);
  expect(sequence(999)).toBe(first + 2);
  expect(sequence(5000)).toBe(5000);
  expect(sequence()).toBeGreaterThanOrEqual(Date.now() - 1);
});

test("dates go over the wire as milliseconds, and nothing undefined does", () => {
  const start = new Date(1_700_000_000_000);
  const end = new Date(1_700_000_060_000);
  const wired = wireContent({
    title: "Focus",
    compact: { timer: { start, end, style: "relative" } },
    body: { progress: { value: undefined, interval: { start, end, pausedAt: start } } },
    staleAt: end,
    actions: [{ title: "Stop", action: { end: true } }],
    subtitle: undefined,
  });
  expect(wired).toEqual({
    title: "Focus",
    compact: { timer: { start: 1_700_000_000_000, end: 1_700_000_060_000, style: "relative" } },
    body: { progress: { interval: { start: 1_700_000_000_000, end: 1_700_000_060_000, pausedAt: 1_700_000_000_000 } } },
    staleAt: 1_700_000_060_000,
    actions: [{ title: "Stop", action: { end: true } }],
  });
  expect(Object.keys(wired)).not.toContain("subtitle");
  expect(wireContent({ compact: "3/9", body: { text: "all" } })).toEqual({ compact: "3/9", body: { text: "all" } });
  expect(wireContent({ compact: { icon: "mic-16" } })).toEqual({ compact: { icon: "mic-16" } });
});

test("a start says how the command was launched, and numbers what follows", async () => {
  const user = as("ci");
  const handle = await runAs(user, () => LiveActivity.start({ id: "build", name: "Build", icon: "hammer-16" }, { title: "Building" }, { announce: false }));
  expect(handle.id).toBe("build");
  const started = last();
  expect(started).toMatchObject({ method: "ext.activity.start", extension: "ci", dir: user.extension.dir });
  expect(started.params).toMatchObject({ attributes: { id: "build", name: "Build", icon: "hammer-16" }, content: { title: "Building" }, announce: false, launchType: "userInitiated" });
  const seq = started.params.seq as number;
  expect(typeof seq).toBe("number");

  await handle.update({ subtitle: "step 2" }, { alert: { expand: true } });
  expect(last()).toMatchObject({ method: "ext.activity.update", extension: "ci", params: { id: "build", content: { subtitle: "step 2" }, alert: { expand: true } } });
  expect(last().params.seq).toBeGreaterThan(seq);
  const second = last().params.seq;
  await handle.update({ title: "Testing" });
  expect(last().params.seq).toBeGreaterThan(second);

  await handle.end({ title: "Built" }, { keep: "none" });
  expect(last()).toMatchObject({ method: "ext.activity.end", params: { id: "build", final: { title: "Built" }, keep: "none" } });
  await handle.end();
  expect(last().params).toEqual({ id: "build", final: undefined, keep: undefined });

  // A command run on a schedule says so; the host refuses it a start.
  await runAs(as("ci", "background"), () => LiveActivity.start({ id: "x", name: "X", icon: "x" }, { title: "X" }));
  expect(last().params.launchType).toBe("background");
});

test("a handle acts as the command that made it, from wherever it is used", async () => {
  const ci = as("ci");
  const handle = await runAs(ci, () => LiveActivity.start({ id: "build", name: "Build", icon: "hammer-16" }, { title: "Building" }));
  await runAs(as("other"), () => handle.update({ title: "Still" }));
  expect(last()).toMatchObject({ method: "ext.activity.update", extension: "ci" });
  // Outside any command, too.
  await handle.end();
  expect(last()).toMatchObject({ method: "ext.activity.end", extension: "ci" });
});

test("the rest ask the host for what their names say", async () => {
  await runAs(as("ci"), async () => {
    expect(await LiveActivity.supported()).toBeUndefined();
    expect(last()).toMatchObject({ method: "ext.supported", params: { module: "activity" } });
    const all = await LiveActivity.list();
    expect(all.map((h) => h.id)).toEqual(["a", "b"]);
    expect(last().method).toBe("ext.activity.list");
    expect((await LiveActivity.get("a"))?.id).toBe("a");
    expect(await LiveActivity.get("zz")).toBeUndefined();
    expect(last()).toMatchObject({ method: "ext.activity.get", params: { id: "zz" } });
    answers["ext.activity.presentation"] = () => ({ support: "none", compactCharacters: 10, expanded: false, buttons: false, version: 1 });
    expect(await LiveActivity.presentation()).toEqual({ support: "none", compactCharacters: 10, expanded: false, buttons: false, version: 1 });
    // A handle from `list` is the command's too.
    await all[1].update({ title: "B" });
    expect(last()).toMatchObject({ method: "ext.activity.update", extension: "ci", params: { id: "b" } });
  });
  // It is part of the API under its name.
  expect(api.LiveActivity).toBe(LiveActivity);
  expect(api.has("LiveActivity.start")).toBe(true);
});

test("a listening command hears its activities' events until it ends", async () => {
  const ci = as("ci");
  const heard: [Event, string][] = [];
  const stop = await runAs(ci, () => LiveActivity.onEvent((e) => heard.push([e, (api.environment as { extensionName: string }).extensionName])));
  expect(last()).toMatchObject({ method: "ext.activity.onEvent", extension: "ci" });
  const pause: Event = { kind: "action", id: "build", event: "pause", data: { end: 5 } };
  event(11, pause);
  event(11, { kind: "elapsed", id: "build", at: 9 });
  await sleep(5);
  // Delivered as the command: what the listener does is its own.
  expect(heard).toEqual([
    [pause, "ci"],
    [{ kind: "elapsed", id: "build", at: 9 }, "ci"],
  ]);
  stop();
  await until(() => sent("ext.unsubscribe").length === 1, "the subscription to end");
  expect(last()).toMatchObject({ method: "ext.unsubscribe", extension: "ci", params: { subscription: 11 } });
  event(11, { kind: "dismissed", id: "build" });
  expect(heard).toHaveLength(2);

  // Or with the command.
  const dl = as("dl");
  await runAs(dl, () => LiveActivity.onEvent(() => {}));
  ended(dl);
  await until(() => sent("ext.unsubscribe").length === 2, "the second subscription to end");
  expect(last()).toMatchObject({ extension: "dl", params: { subscription: 11 } });
});
