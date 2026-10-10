//! The `@raycast/api` module handed to extensions.

import { randomUUID } from "node:crypto";
import path from "node:path";
import { useId } from "react";

import { enqueue, isBackground } from "../background";
import { current, installedExtensions, ownsPalette, preferenceValues, readJson, requireCurrent, supportPath, writeJson, type Extension } from "../context";
import { request } from "../protocol";
import { launch, openPreferences } from "../session";
import { Action, ActionPanel, Detail, Form, Grid, List, MenuBarExtra } from "./components";
import { streamClaude } from "../ai";
import { Icon } from "./icon";
import { useNavigation } from "./navigation";
import { OAuth } from "./oauth";
import * as system from "./system";
import { getFrontmostApplication, WindowManagement } from "./windows";

export const LaunchType = { UserInitiated: "userInitiated", Background: "background" } as const;

export const Color = {
  Blue: "raycast-blue",
  Green: "raycast-green",
  Magenta: "raycast-magenta",
  Orange: "raycast-orange",
  Purple: "raycast-purple",
  Red: "raycast-red",
  Yellow: "raycast-yellow",
  Brown: "raycast-brown",
  PrimaryText: "raycast-primary-text",
  SecondaryText: "raycast-secondary-text",
} as const;

export const Image = { Mask: { Circle: "circle", RoundedRectangle: "roundedRectangle" } } as const;

const shortcut = (modifiers: string[], key: string) => ({ modifiers, key });

export const Keyboard = {
  Shortcut: {
    Common: {
      Copy: shortcut(["cmd", "shift"], "c"),
      CopyDeeplink: shortcut(["cmd", "shift"], "c"),
      CopyName: shortcut(["cmd", "shift"], "."),
      CopyPath: shortcut(["cmd", "shift"], ","),
      Save: shortcut(["cmd"], "s"),
      Duplicate: shortcut(["cmd"], "d"),
      Edit: shortcut(["cmd"], "e"),
      MoveDown: shortcut(["cmd", "shift"], "arrowDown"),
      MoveUp: shortcut(["cmd", "shift"], "arrowUp"),
      New: shortcut(["cmd"], "n"),
      Open: shortcut(["cmd"], "o"),
      OpenWith: shortcut(["cmd", "shift"], "o"),
      Pin: shortcut(["cmd", "shift"], "p"),
      Refresh: shortcut(["cmd"], "r"),
      Remove: shortcut(["ctrl"], "x"),
      RemoveAll: shortcut(["ctrl", "shift"], "x"),
      ToggleQuickLook: shortcut(["cmd"], "y"),
    },
  },
};

/// Live view of the running command; extensions often read it at import time.
export const environment = {
  appearance: "dark",
  theme: "dark",
  textSize: "medium",
  raycastVersion: "1.100.0",
  isDevelopment: false,
  get extensionName() {
    return current.value?.extension.manifest.name ?? "";
  },
  get ownerOrAuthorName() {
    const manifest = current.value?.extension.manifest;
    return manifest?.owner ?? manifest?.author ?? "";
  },
  get commandName() {
    return current.value?.command ?? "";
  },
  get commandMode() {
    const c = current.value;
    return c?.extension.manifest.commands.find((cmd) => cmd.name === c.command)?.mode ?? "view";
  },
  get launchType() {
    return current.value?.launchType ?? LaunchType.UserInitiated;
  },
  get assetsPath() {
    return `${requireCurrent().extension.dir}/assets`;
  },
  get supportPath() {
    return supportPath(requireCurrent().extension.manifest.name);
  },
  canAccess(api: unknown) {
    return api !== BrowserExtension;
  },
};

export function getPreferenceValues() {
  const { extension, command } = requireCurrent();
  return preferenceValues(extension, command);
}

export async function openExtensionPreferences() {
  openPreferences();
}

export const openCommandPreferences = openExtensionPreferences;

interface LaunchCommandOptions {
  name: string;
  type?: string;
  extensionName?: string;
  ownerOrAuthorName?: string;
  arguments?: Record<string, string>;
  context?: Record<string, unknown>;
  fallbackText?: string;
}

const ownerOf = (extension: Extension) => extension.manifest.owner ?? extension.manifest.author ?? "";

/// The extension a launch means: the caller's own unless another is named.
function launchTarget(caller: Extension, options: LaunchCommandOptions): Extension {
  const { extensionName, ownerOrAuthorName } = options;
  const named = (extension: Extension) =>
    extension.manifest.name === extensionName && (!ownerOrAuthorName || ownerOf(extension) === ownerOrAuthorName);
  if (!extensionName || named(caller)) return caller;
  const found = installedExtensions().find(named);
  if (!found) throw new Error(`The extension ${ownerOrAuthorName ? `${ownerOrAuthorName}/` : ""}${extensionName} is not installed`);
  return found;
}

/// Lets an extension launch another's commands only once the user has said
/// it may. The answer is kept with the caller (its support directory), for
/// all of its commands; a refusal is not kept, so the user is asked again.
async function allowLaunch(caller: Extension, target: Extension) {
  const file = path.join(supportPath(caller.manifest.name), "launch-permissions.json");
  const allowed = readJson<string[]>(file, []);
  if (allowed.includes(target.manifest.name)) return;
  const refused = `${caller.manifest.title} is not allowed to launch commands of ${target.manifest.title}`;
  // Nobody can be asked from the menu bar or the background.
  if (isBackground()) throw new Error(`${refused}: it has to ask once from a command opened in Heyo`);
  const yes = await system.confirmAlert({
    title: `Allow ${caller.manifest.title} to launch commands of ${target.manifest.title}?`,
    message: "It will not ask again.",
    primaryAction: { title: "Allow" },
    dismissAction: { title: "Don't Allow" },
  });
  if (!yes) throw new Error(refused);
  // Read again: another launch may have been allowed while this one waited.
  writeJson(file, [...new Set([...readJson<string[]>(file, []), target.manifest.name])]);
}

/// Starts another command: one of the caller's extension, or of another the
/// user has allowed it to launch. Everything is checked before anything is
/// closed or started.
export async function launchCommand(options: LaunchCommandOptions) {
  const caller = requireCurrent();
  const target = launchTarget(caller.extension, options);
  const command = target.manifest.commands.find((c) => c.name === options.name);
  if (!command) throw new Error(`${target.manifest.title} has no command ${options.name}`);
  const background = options.type === LaunchType.Background;
  if (background && command.mode === "view") throw new Error(`${command.title} has a view and cannot be launched in the background`);
  if (target !== caller.extension) await allowLaunch(caller.extension, target);

  const here = { arguments: options.arguments, launchContext: options.context, fallbackText: options.fallbackText };
  const elsewhere = { extension: target.dir, command: command.name, arguments: options.arguments, context: options.context, fallbackText: options.fallbackText };

  // The menu bar and the background are the background process's.
  if (background || command.mode === "menu-bar") {
    if (!isBackground()) return void (await request("host.launchCommand", { ...elsewhere, background }));
    // Not waited for: a command may launch itself, and its runs follow each other.
    void enqueue({
      type: "background-run",
      id: 0,
      extension: target.dir,
      command: command.name,
      launchType: background ? LaunchType.Background : LaunchType.UserInitiated,
      action: null,
      arguments: options.arguments,
      launchContext: options.context,
    });
    return;
  }

  // The palette is given only to the command in it, or to the user's click
  // on a menu item.
  if (ownsPalette()) return launch(target, command.name, here);
  if (isBackground() && caller.user) return void (await request("host.launchCommand", { ...elsewhere, background: false }));
  throw new Error(
    isBackground()
      ? `${command.title} was not opened: a background command opens the palette only when the user clicks its menu`
      : `${command.title} was not opened: the command that asked is no longer the one in the palette`,
  );
}

export async function updateCommandMetadata(_metadata: { subtitle?: string | null }) {}

export const AI = {
  /// Any model named is answered by the one chosen in Heyo's settings.
  Model: new Proxy({}, { get: (_, name) => String(name) }) as Record<string, string>,
  Creativity: { None: "none", Low: "low", Medium: "medium", High: "high", Maximum: "maximum" },
  /// Answers with the model chosen in Heyo's settings; `model` and
  /// `creativity` are accepted and ignored.
  ask(prompt: string, options?: { signal?: AbortSignal }) {
    const listeners: ((chunk: string) => void)[] = [];
    let seen = "";
    const answer = streamClaude({
      system: "You are a helpful assistant inside a desktop launcher extension. Follow the request exactly and answer concisely.",
      prompt,
      history: [],
      signal: options?.signal,
      onText: (text) => {
        const chunk = text.slice(seen.length);
        seen = text;
        if (chunk) listeners.forEach((listener) => listener(chunk));
      },
    });
    // Raycast's promise also emits the answer piece by piece.
    return Object.assign(answer, {
      on(event: string, listener: (chunk: string) => void) {
        if (event === "data") listeners.push(listener);
        return this;
      },
    });
  },
};

export const BrowserExtension = {
  getContent: system.unsupported("The browser extension API"),
  getTabs: system.unsupported("The browser extension API"),
};

export { OAuth };

export const Clipboard = system.Clipboard;
export const LocalStorage = system.LocalStorage;

/// Names from older versions of the API, which extensions built against
/// them still use.
const deprecated = {
  ActionPanelItem: Action,
  ActionPanelSection: ActionPanel.Section,
  ActionPanelSubmenu: ActionPanel.Submenu,
  CopyToClipboardAction: Action.CopyToClipboard,
  OpenAction: Action.Open,
  OpenInBrowserAction: Action.OpenInBrowser,
  OpenWithAction: Action.OpenWith,
  PasteAction: Action.Paste,
  PushAction: Action.Push,
  ShowInFinderAction: Action.ShowInFinder,
  SubmitFormAction: Action.SubmitForm,
  TrashAction: Action.Trash,
  FormCheckbox: Form.Checkbox,
  FormDatePicker: Form.DatePicker,
  FormDropdown: Form.Dropdown,
  FormDropdownItem: Form.Dropdown.Item,
  FormDropdownSection: Form.Dropdown.Section,
  FormSeparator: Form.Separator,
  FormTagPicker: Form.TagPicker,
  FormTagPickerItem: Form.TagPicker.Item,
  FormTextArea: Form.TextArea,
  FormTextField: Form.TextField,
  ListItem: List.Item,
  ListSection: List.Section,
  ToastStyle: system.Toast.Style,
  AlertActionStyle: system.Alert.ActionStyle,
  ImageMask: Image.Mask,
  allLocalStorageItems: system.LocalStorage.allItems,
  getLocalStorageItem: system.LocalStorage.getItem,
  setLocalStorageItem: system.LocalStorage.setItem,
  removeLocalStorageItem: system.LocalStorage.removeItem,
  clearLocalStorage: system.LocalStorage.clear,
  copyTextToClipboard: (text: string) => system.Clipboard.copy(text),
  pasteText: (text: string) => system.Clipboard.paste(text),
  clearClipboard: system.Clipboard.clear,
  /// `preferences.name.value`, before `getPreferenceValues`.
  preferences: new Proxy({}, { get: (_, name) => ({ value: (getPreferenceValues() as Record<string, unknown>)[String(name)] }) }) as Record<
    string,
    { value: unknown }
  >,
};

export default {
  ...deprecated,
  useId,
  Action,
  ActionPanel,
  AI,
  Alert: system.Alert,
  BrowserExtension,
  Cache: system.Cache,
  captureException: (e: unknown) => console.error(e),
  clearSearchBar: system.clearSearchBar,
  Clipboard,
  closeMainWindow: system.closeMainWindow,
  Color,
  confirmAlert: system.confirmAlert,
  Detail,
  environment,
  Form,
  getApplications: system.getApplications,
  getDefaultApplication: system.getDefaultApplication,
  getFrontmostApplication,
  getPreferenceValues,
  getSelectedFinderItems: system.getSelectedFinderItems,
  getSelectedText: system.getSelectedText,
  Grid,
  Icon,
  Image,
  Keyboard,
  launchCommand,
  LaunchType,
  List,
  LocalStorage,
  MenuBarExtra,
  OAuth,
  open: system.open,
  openCommandPreferences,
  openExtensionPreferences,
  popToRoot: system.popToRoot,
  PopToRootType: system.PopToRootType,
  randomId: () => randomUUID(),
  showHUD: system.showHUD,
  showInFinder: system.showInFinder,
  showToast: system.showToast,
  Toast: system.Toast,
  trash: system.trash,
  updateCommandMetadata,
  useNavigation,
  WindowManagement,
};
