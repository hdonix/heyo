// Generated from src-tauri/src/commands/bindings.rs by `cargo test`. Do not edit.
import type { ActivityView } from "./ActivityView";
import type { Capabilities } from "./Capabilities";
import type { ClipboardMeaningStatus } from "./ClipboardMeaningStatus";
import type { ClipboardSource } from "./ClipboardSource";
import type { DictationEvent } from "./DictationEvent";
import type { DictationStatus } from "./DictationStatus";
import type { ExpandSnippet } from "./ExpandSnippet";
import type { ExtensionInfo } from "./ExtensionInfo";
import type { HotkeyName } from "./HotkeyName";
import type { Hotkeys } from "./Hotkeys";
import type { InstallProgress } from "./InstallProgress";
import type { Item } from "./Item";
import type { OcrStatus } from "./OcrStatus";
import type { Permission } from "./Permission";
import type { PermissionRow } from "./PermissionRow";
import type { PreferenceValues } from "./PreferenceValues";
import type { RuntimeMessage } from "../wire";
import type { ScreenshotCapture } from "./ScreenshotCapture";
import type { ScreenshotFinish } from "./ScreenshotFinish";
import type { Settings } from "./Settings";
import type { SettingsView } from "./SettingsView";
import type { WebviewMessage } from "./WebviewMessage";

/** The commands the host takes from its windows: the arguments of each, and what it answers. */
export type Commands = {
  search: { args: { query: string }; result: Array<Item> };
  /**
   * Opens the item chosen in the palette: an application, a command of an
   * extension's or one of Heyo's own. (Quicklinks and snippets the palette
   * opens itself.) `arguments` are those typed in the search bar for a
   * command that takes some.
   */
  launch: { args: { id: string; fallbackText?: string | null; arguments?: { [key in string]: string } | null }; result: null };
  /** Relays a message from the webview to the extension runtime. */
  runtime_send: { args: { message: WebviewMessage }; result: null };
  hide: { args: null; result: null };
  get_settings: { args: null; result: SettingsView };
  /**
   * Changes some settings: `changes` holds the new values of the ones that
   * change, by name, and nothing else. `launchAtLogin` may be among them.
   */
  change_settings: { args: { changes: Partial<Omit<Settings, "anthropicApiKey">> & { launchAtLogin?: boolean } }; result: SettingsView };
  /** Stores the Claude API key; an empty one removes it. */
  set_anthropic_api_key: { args: { key: string }; result: SettingsView };
  /** Opens the settings window, or brings it back if it is already open. */
  open_settings: { args: null; result: null };
  open_extensions_folder: { args: null; result: null };
  list_extensions: { args: null; result: Array<ExtensionInfo> };
  set_extension_preferences: { args: { extension: string; values: PreferenceValues }; result: null };
  uninstall_extension: { args: { id: string }; result: null };
  clear_clipboard_history: { args: null; result: null };
  get_hotkeys: { args: null; result: Hotkeys };
  set_hotkey: { args: { name: HotkeyName; accelerator: string }; result: Hotkeys };
  /** Binds a global hotkey to a root search item; an empty accelerator removes it. */
  set_item_hotkey: { args: { id: string; title: string; accelerator: string }; result: null };
  item_hotkeys: { args: null; result: { [key in string]: string } };
  screenshot_finish: { args: { request: ScreenshotFinish }; result: string };
  ocr_status: { args: null; result: OcrStatus };
  ocr_install: { args: null; result: null };
  dictation_status: { args: null; result: DictationStatus };
  dictation_install: { args: null; result: null };
  dictation_toggle: { args: null; result: null };
  dictation_cancel: { args: null; result: null };
  show_window: { args: null; result: null };
  get_item: { args: { id: string }; result: Item | null };
  list_applications: { args: null; result: Array<Item> };
  /** Quicklinks, for offering the ones that take a query as fallbacks. */
  list_quicklinks: { args: null; result: Array<Item> };
  /** What the running session can do, for explaining what is unavailable. */
  get_capabilities: { args: null; result: Capabilities };
  /** Installs the GNOME Shell extension that Wayland sessions need. */
  install_shell_extension: { args: null; result: Capabilities };
  /**
   * The models an OpenAI-compatible server on this computer offers; an error
   * says why it could not be asked.
   */
  local_models: { args: { url: string }; result: Array<string> };
  /** Counts a use of a root search item, for ranking. */
  record_usage: { args: { id: string }; result: null };
  set_favorite: { args: { id: string; favorite: boolean }; result: null };
  /** Moves a favorite up (`offset` < 0) or down. */
  move_favorite: { args: { id: string; offset: number }; result: null };
  /** Sets an item's alias; an empty one removes it. */
  set_alias: { args: { id: string; alias: string }; result: null };
  reset_ranking: { args: { id: string }; result: null };
  read_clipboard: { args: null; result: string };
  /**
   * Copies text and hides the palette; with `paste`, also pastes it into the
   * window that regains focus. Several lines on their way into a terminal
   * are asked about first (`pasting`), and the palette stays for the answer.
   */
  copy_text: { args: { text: string; paste: boolean }; result: null };
  /** Opens a URL or path with the default application and hides the palette. */
  open_target: { args: { target: string }; result: null };
  /**
   * The applications that can be named as ones not to record from, for the
   * settings window.
   */
  clipboard_sources: { args: null; result: Array<ClipboardSource> };
  /** Where finding by meaning stands, for the settings window. */
  clipboard_meaning_status: { args: null; result: ClipboardMeaningStatus };
  /** Downloads what finding by meaning needs. */
  clipboard_meaning_install: { args: null; result: null };
  /** The version that can be installed, if there is one newer than this. */
  check_for_update: { args: null; result: string | null };
  /**
   * Downloads the newer version, checks its signature, installs it and
   * starts Heyo again.
   */
  install_update: { args: null; result: null };
  /**
   * Puts a description of this Heyo and where it runs on the clipboard, for
   * a bug report: versions, the session, the end of the log, secrets removed.
   */
  copy_diagnostics: { args: null; result: null };
  /** The permissions of the extension installed in `id`. */
  extension_permissions: { args: { id: string }; result: Array<PermissionRow> };
  /** Takes back what an extension was allowed; it asks again when it next needs it. */
  revoke_extension_permission: { args: { extension: string; permission: Permission }; result: null };
  /** The palette's answer to `EXPAND_SNIPPET`. */
  snippet_expanded: { args: { request: number; text: string }; result: null };
  /** The activities as the palette shows them, now. */
  activities: { args: null; result: Array<ActivityView> };
  /**
   * The user chose a button of an activity (by its index), or the activity
   * itself (no button).
   */
  activity_action: { args: { key: string; button?: number | null }; result: null };
  /** The user ended an activity. */
  activity_dismiss: { args: { key: string }; result: null };
};

/** The commands that answer nothing: a host played in a test answers them unasked. */
export const answersNothing = [
  "launch",
  "runtime_send",
  "hide",
  "open_settings",
  "open_extensions_folder",
  "set_extension_preferences",
  "uninstall_extension",
  "clear_clipboard_history",
  "set_item_hotkey",
  "ocr_install",
  "dictation_install",
  "dictation_toggle",
  "dictation_cancel",
  "show_window",
  "record_usage",
  "set_favorite",
  "move_favorite",
  "set_alias",
  "reset_ranking",
  "copy_text",
  "open_target",
  "clipboard_meaning_install",
  "install_update",
  "copy_diagnostics",
  "revoke_extension_permission",
  "snippet_expanded",
  "activity_action",
  "activity_dismiss",
] as const satisfies ReadonlyArray<keyof Commands>;

/** What each of the host's events (`event` in ./events) carries. */
export type Events = {
  shown: null;
  indexReady: null;
  settingsChanged: SettingsView;
  openItem: string;
  runtime: RuntimeMessage;
  dictation: DictationEvent;
  dictationInstall: InstallProgress;
  ocrInstall: InstallProgress;
  screenshot: ScreenshotCapture;
  expandSnippet: ExpandSnippet;
  clipboardMeaningInstall: InstallProgress;
  activities: Array<ActivityView>;
};
