// @vitest-environment happy-dom
//
// The settings window with the host played by a table of answers: it loads,
// shows a page, and a switch changes one setting through the host and goes
// back when the host refuses.
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, expect, test } from "vitest";

import type { Capabilities } from "../shared/bindings/Capabilities";
import type { ExtensionInfo } from "../shared/bindings/ExtensionInfo";
import type { SettingsView } from "../shared/bindings/SettingsView";
import { fakeHost, type FakeHost } from "./host.fake";
import { SettingsWindow } from "./Settings";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const settings: SettingsView = {
  appearance: "system",
  hideOnBlur: true,
  compactMode: false,
  keepCommand: false,
  nodePath: "",
  clipboardHistory: true,
  clipboardRetentionDays: 90,
  disabledCommands: [],
  favorites: [],
  aliases: {},
  dictationLanguage: "auto",
  dictationSource: "",
  dictationPolish: "off",
  dictationHoldKey: "",
  dictationLive: true,
  anthropicApiKey: "",
  aiProvider: "claude",
  localModelUrl: "http://127.0.0.1:8000/v1",
  localModelName: "",
  clipboardPaused: false,
  clipboardIgnoredApps: [],
  dictationVocabulary: [],
  confirmTerminalPaste: true,
  snippetExpansion: false,
  snippetExpansionIgnoredApps: [],
  clipboardMeaning: false,
  renderer: "auto",
  liveActivities: true,
  activitiesOff: [],
  anthropicApiKeySet: false,
  launchAtLogin: false,
  nodeVersion: { Ok: "v24.0.0" },
  nodeOverridden: false,
  extensionsDir: "/home/ada/.local/share/heyo/extensions",
  settingsPath: "/home/ada/.config/heyo/settings.json",
  version: "0.1.0",
  canUpdate: false,
};

const capabilities: Capabilities = {
  session: "x11",
  windows: "full",
  paste: "full",
  selection: "full",
  clipboardWatch: "full",
  hotkeys: "full",
  holdKey: "full",
  capture: "full",
  overlay: "full",
  snippetExpansion: "full",
  liveDictation: "full",
  holdKeys: [],
  extension: "none",
};

const builtin: ExtensionInfo = {
  id: "@heyo",
  name: "@heyo",
  title: "Heyo",
  description: "Commands built into Heyo.",
  author: null,
  icon: "store-16",
  builtin: true,
  commands: [{ id: "cmd:@heyo:clipboard-history", name: "clipboard-history", title: "Clipboard History", description: null, mode: "view", icon: null, preferences: [] }],
  preferences: [],
  values: { extension: {}, commands: {} },
};

let root: Root;
let page: HTMLElement;
let host: FakeHost;

/// Lets what the host answered reach the window.
const settle = () => act(async () => void (await new Promise((resolve) => setTimeout(resolve))));

beforeEach(async () => {
  host = fakeHost({
    get_settings: settings,
    get_hotkeys: { supported: false, toggle: null, clipboard: null, dictation: null },
    get_capabilities: capabilities,
    list_extensions: [builtin],
    list_applications: [{ id: "app:firefox", title: "Firefox", subtitle: "Web Browser", icon: null, kind: "Application", favorite: false }],
    item_hotkeys: {},
  });
  page = document.createElement("div");
  document.body.append(page);
  root = createRoot(page);
  act(() => root.render(<SettingsWindow />));
  await settle();
});

afterEach(() => {
  act(() => root.unmount());
  page.remove();
});

const text = (selector: string) => page.querySelector(selector)?.textContent ?? null;

/// The switch in the row of that label.
function toggle(label: string) {
  const row = [...page.querySelectorAll(".st-row")].find((row) => row.querySelector(".st-row-label")?.textContent === label);
  return row!.querySelector<HTMLInputElement>("input.st-switch")!;
}

test("the window asks the host for what it shows, and opens on General", () => {
  expect(host.calls.map(([command]) => command).sort()).toEqual(["get_capabilities", "get_hotkeys", "get_settings", "item_hotkeys", "list_applications", "list_extensions"]);
  expect(text(".st-titlebar-title")).toBe("General");
  expect([...page.querySelectorAll(".st-nav-item")].map((item) => item.textContent)).toEqual(["General", "Shortcuts", "Dictation", "Screenshots", "Advanced", "About", "Applications", "Heyo Commands"]);
  expect(toggle("Compact Mode").checked).toBe(false);
  expect(toggle("Hide When Focus Is Lost").checked).toBe(true);
  expect(text(".st-error")).toBeNull();
});

test("a switch shows its new value at once and sends the host that one change", async () => {
  let answer!: (changed: SettingsView) => void;
  host.answer("change_settings", () => new Promise((resolve) => (answer = resolve)));
  host.forget();
  act(() => toggle("Compact Mode").click());
  // Before the host has said anything.
  expect(toggle("Compact Mode").checked).toBe(true);
  expect(host.calls).toEqual([["change_settings", { changes: { compactMode: true } }]]);

  // The host's word is what stays: here it also turned something else off.
  await act(async () => answer({ ...settings, compactMode: true, hideOnBlur: false }));
  expect(toggle("Compact Mode").checked).toBe(true);
  expect(toggle("Hide When Focus Is Lost").checked).toBe(false);
  expect(text(".st-error")).toBeNull();
});

test("when the host refuses a change the switch goes back and the window says why", async () => {
  let refuse!: (why: string) => void;
  host.answer("change_settings", () => new Promise((_, reject) => (refuse = reject)));
  host.forget();
  act(() => toggle("Compact Mode").click());
  expect(toggle("Compact Mode").checked).toBe(true);

  await act(async () => refuse("there is no setting called compactMode"));
  await settle();
  // It asked the host again for how things stand.
  expect(host.calls.map(([command]) => command)).toEqual(["change_settings", "get_settings"]);
  expect(toggle("Compact Mode").checked).toBe(false);
  expect(text(".st-error")).toBe("there is no setting called compactMode");

  // A change that goes through clears the line.
  host.answer("change_settings", ({ changes }) => ({ ...settings, ...changes }));
  act(() => toggle("Compact Mode").click());
  await settle();
  expect(toggle("Compact Mode").checked).toBe(true);
  expect(text(".st-error")).toBeNull();
});

test("settings changed elsewhere are asked for whole, and a failed call is said in the window", async () => {
  // A favorite set in the palette: every window hears, without the places of the files.
  host.answer("get_settings", { ...settings, compactMode: true });
  host.forget();
  act(() => host.emit("settingsChanged", { ...settings, compactMode: true, extensionsDir: "", settingsPath: "" }));
  await settle();
  expect(host.calls).toEqual([["get_settings"]]);
  expect(toggle("Compact Mode").checked).toBe(true);

  // Another page makes its own calls; one the host fails is not lost.
  host.refuse("dictation_status", "pactl is not installed");
  act(() => [...page.querySelectorAll<HTMLButtonElement>(".st-nav-item")].find((item) => item.textContent === "Dictation")!.click());
  await settle();
  expect(text(".st-titlebar-title")).toBe("Dictation");
  expect(text(".st-error")).toBe("pactl is not installed");
});
