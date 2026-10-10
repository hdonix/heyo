// The built-in timer, run as the host runs it: from the palette with the
// duration typed in the search bar, then in the background process for
// each event of its activity, under Heyo's own name throughout.
import { beforeAll, beforeEach, expect, test, vi } from "vitest";

import { answers, receive, requests, reset, sent, until } from "../testing";

vi.mock("../protocol", async () => (await import("../testing")).protocol);

import { startBackground } from "../background";
import { foreground, runAs, type Current } from "../context";
import { builtinExtension } from "./index";
import { content, describe, parseDuration, Timer } from "./timer";

const own: Current = { extension: builtinExtension, command: "timer", launchType: "userInitiated", user: true };
const MINUTE = 60_000;
let nextRun = 0;

/// The host runs the handler in the background with an event (`activities.rs`).
function deliver(event: unknown, launchType = "userInitiated") {
  const id = ++nextRun;
  receive({ type: "background-run", id, extension: "@heyo", command: "timer", launchType, action: null, launchContext: { activity: event } });
  return until(() => sent.find((m) => m.type === "background-done" && m.id === id), "the handler to be done");
}

const of = (method: string) => requests.filter((r) => r.method === method);
const titles = (actions: { title: string }[]) => actions.map((a) => a.title);

beforeAll(() => startBackground());

beforeEach(() => {
  reset();
  answers["ext.activity.start"] = (params) => ({ id: params.attributes.id });
  answers["ext.activity.get"] = () => true;
});

test("a duration is read as typed", () => {
  expect(parseDuration("25m")).toBe(25 * MINUTE);
  expect(parseDuration("1h 10m")).toBe(70 * MINUTE);
  expect(parseDuration("90s")).toBe(90_000);
  expect(parseDuration(" 25 ")).toBe(25 * MINUTE);
  expect(parseDuration("1.5h")).toBe(90 * MINUTE);
  expect(parseDuration("2 hours 5 minutes")).toBe(125 * MINUTE);
  expect(parseDuration("1h30")).toBeUndefined();
  for (const bad of ["", "soon", "0m", "5x", "m"]) expect(parseDuration(bad), bad).toBeUndefined();
  expect(describe(25 * MINUTE)).toBe("25 min");
  expect(describe(70 * MINUTE)).toBe("1 h 10 min");
  expect(describe(90_000)).toBe("1 min 30 s");
  expect(describe(0)).toBe("0 s");
});

test("the content holds the countdown in both forms and the buttons carry the span", () => {
  const span = { start: 1_700_000_000_000, end: 1_700_000_000_000 + 25 * MINUTE };
  const running = content(span);
  expect(running.title).toBe("25 min");
  expect(running.subtitle).toMatch(/^Until /);
  expect(running.compact).toEqual({ timer: { start: new Date(span.start), end: new Date(span.end), pausedAt: undefined } });
  expect(running.body).toEqual(running.compact);
  expect(titles(running.actions!)).toEqual(["Pause", "+1 min", "End"]);
  expect(running.actions![0].action).toEqual({ event: "pause", data: span });
  expect(running.actions![2].action).toEqual({ end: true });
  const paused = content({ ...span, pausedAt: span.start + MINUTE });
  expect(paused.subtitle).toBe("Paused");
  expect(titles(paused.actions!)).toEqual(["Resume", "+1 min", "End"]);
});

test("the timer starts its activity, pauses, gains a minute and ends when told it ran out", async () => {
  // From the palette, with the argument typed in the search bar.
  foreground.value = own;
  const before = Date.now();
  await runAs(own, () => Timer({ arguments: { duration: "25m" } }));
  foreground.value = null;
  expect(of("host.hide")).toHaveLength(1);
  const [started] = of("ext.activity.start");
  expect(started).toMatchObject({ extension: "@heyo", dir: "" });
  const { attributes, content: shown, launchType, seq } = started.params;
  expect(attributes).toEqual({ id: expect.stringMatching(/^timer-\d+$/), name: "Timer", icon: "clock-16" });
  expect(launchType).toBe("userInitiated");
  expect(typeof seq).toBe("number");
  const { start, end } = shown.compact.timer;
  expect(start).toBeGreaterThanOrEqual(before);
  expect(end - start).toBe(25 * MINUTE);
  expect(shown.title).toBe("25 min");
  expect(titles(shown.actions)).toEqual(["Pause", "+1 min", "End"]);
  const span = shown.actions[0].action.data;
  expect(span).toEqual({ start, end });

  // The user presses Pause: the host runs the handler with the event.
  await deliver({ kind: "action", id: attributes.id, event: "pause", data: span });
  const [paused] = of("ext.activity.update");
  expect(paused).toMatchObject({ extension: "@heyo", params: { id: attributes.id } });
  expect(paused.params.content.compact.timer.pausedAt).toBeGreaterThanOrEqual(start);
  expect(paused.params.content.subtitle).toBe("Paused");
  expect(titles(paused.params.content.actions)).toEqual(["Resume", "+1 min", "End"]);
  expect(paused.params.seq).toBeGreaterThan(seq);

  // Resume: the time it stood still is added at the end.
  const stood = paused.params.content.actions[0].action.data;
  await deliver({ kind: "action", id: attributes.id, event: "resume", data: stood });
  const resumed = of("ext.activity.update")[1].params.content.compact.timer;
  expect(resumed.pausedAt).toBeUndefined();
  expect(resumed.end - resumed.start).toBe(25 * MINUTE);
  expect(resumed.end).toBeGreaterThanOrEqual(end);

  // +1 min.
  await deliver({ kind: "action", id: attributes.id, event: "plus", data: { start, end } });
  const longer = of("ext.activity.update")[2].params.content;
  expect(longer.compact.timer.end).toBe(end + MINUTE);
  expect(longer.title).toBe("26 min");

  // The countdown ran out (the host says so): the activity ends with its
  // final content, which the host turns into the notification.
  await deliver({ kind: "elapsed", id: attributes.id, at: end }, "background");
  const [over] = of("ext.activity.end");
  expect(over).toMatchObject({ extension: "@heyo", params: { id: attributes.id, final: { title: "Time's up", compact: "0:00", actions: [] } } });
  // Nothing new was started along the way.
  expect(of("ext.activity.start")).toHaveLength(1);

  // An event for an activity that is gone does nothing.
  answers["ext.activity.get"] = () => false;
  await deliver({ kind: "action", id: attributes.id, event: "pause", data: span });
  expect(of("ext.activity.update")).toHaveLength(3);
  expect(sent.filter((m) => m.type === "background-done").every((m) => m.error === undefined)).toBe(true);
});

test("without a length it says so and starts nothing", async () => {
  foreground.value = own;
  await runAs(own, () => Timer({ arguments: { duration: "soon" } }));
  foreground.value = null;
  expect(of("ext.activity.start")).toHaveLength(0);
  expect(sent.find((m) => m.type === "toast")?.toast).toMatchObject({ style: "FAILURE", title: "Say how long" });
});
