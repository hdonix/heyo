// @vitest-environment happy-dom
//
// The dictation window with the host played by a table of answers: what it
// shows of a recording as the host tells it, and what it tells the host
// when it is pressed.
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, expect, test } from "vitest";

import type { DictationEvent } from "../shared/bindings/DictationEvent";
import { DictationWindow } from "./Dictation";
import { fakeHost, type FakeHost } from "./host.fake";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

let root: Root;
let page: HTMLElement;
let host: FakeHost;

const settle = () => act(async () => void (await new Promise((resolve) => setTimeout(resolve))));

beforeEach(async () => {
  host = fakeHost();
  page = document.createElement("div");
  document.body.append(page);
  root = createRoot(page);
  act(() => root.render(<DictationWindow />));
  await settle();
});

afterEach(() => {
  act(() => root.unmount());
  page.remove();
});

/// The host says these of a recording, one after the other.
const told = (...events: DictationEvent[]) => act(() => events.forEach((event) => host.emit("dictation", event)));
const shown = () => page.querySelector(".dictation-text")?.textContent ?? null;
const pill = () => page.querySelector<HTMLElement>(".dictation")!;
const press = (element: Element, type: "mousedown" | "click") => act(() => void element.dispatchEvent(new MouseEvent(type, { bubbles: true })));

test("it shows the words as they are recognized, where nothing else does", () => {
  told({ type: "start", polish: "off" }, { type: "ready" });
  expect(shown()).toBeNull();
  told({ type: "partial", text: "The weather" });
  expect(shown()).toBe("The weather");
  // Each sentence is pasted when it is finished: the window is where to follow.
  told({ type: "writing", live: false }, { type: "segment", text: "The weather is nice." }, { type: "partial", text: " See you" });
  expect(shown()).toBe("The weather is nice. See you");
  told({ type: "result", text: "The weather is nice. See you soon.", raw: "The weather is nice. See you soon." });
  expect(shown()).toBe("The weather is nice. See you soon.");
  expect(pill().className).toContain("done");
});

test("it does not repeat words that are going into the application as they are spoken", () => {
  told({ type: "start", polish: "off" }, { type: "ready" }, { type: "writing", live: true }, { type: "partial", text: "The weather" });
  expect(shown()).toBeNull();
  told({ type: "segment", text: "The weather is nice." });
  expect(shown()).toBeNull();
  // The user typed: from here the words are only pasted when a sentence is
  // finished, and the window shows them again, all of them.
  told({ type: "writing", live: false }, { type: "partial", text: " See you" });
  expect(shown()).toBe("The weather is nice. See you");
  // The next recording starts with nothing decided.
  told({ type: "start", polish: "off" }, { type: "partial", text: "Hello" });
  expect(shown()).toBe("Hello");
});

test("it says what could not be done at the end", () => {
  told({ type: "start", polish: "clean" }, { type: "writing", live: true }, { type: "segment", text: "um hello." }, { type: "polishing" });
  expect(pill().className).toContain("polishing");
  told({ type: "result", text: "um hello.", raw: "um hello.", warning: "it could not safely take the place of the text, which was left as it was heard" });
  expect(shown()).toBe("Not polished: it could not safely take the place of the text, which was left as it was heard");

  told({ type: "start", polish: "off" }, { type: "writing", live: true }, { type: "segment", text: "Hello." }, { type: "result", text: "", raw: "", cancelled: true });
  expect(shown()).toBe("Cancelled");
  told({ type: "start", polish: "off" }, { type: "writing", live: true }, { type: "segment", text: "Hello." }, { type: "result", text: "", raw: "", cancelled: true, kept: true });
  expect(shown()).toBe("Cancelled. What was written is still there.");
  told({ type: "start", polish: "off" }, { type: "result", text: "", raw: "" });
  expect(shown()).toBe("Didn't catch anything");
  told({ type: "start", polish: "off" }, { type: "error", message: "no microphone" });
  expect(shown()).toBe("no microphone");
});

test("a press on it is told at once, and a click finishes or cancels", () => {
  told({ type: "start", polish: "off" }, { type: "ready" });
  // The button going down, before it is a click: it is not a click in the
  // application being written into.
  press(pill(), "mousedown");
  expect(host.calls).toEqual([["dictation_pressed"]]);
  press(pill(), "click");
  expect(host.calls).toEqual([["dictation_pressed"], ["dictation_toggle"]]);
  host.forget();
  const cancel = page.querySelector(".icon-button")!;
  press(cancel, "mousedown");
  press(cancel, "click");
  expect(host.calls).toEqual([["dictation_pressed"], ["dictation_cancel"]]);
  // Once the recording is over a click does nothing, but is still not the user's.
  told({ type: "polishing" });
  host.forget();
  press(pill(), "mousedown");
  press(pill(), "click");
  expect(host.calls).toEqual([["dictation_pressed"]]);
});
