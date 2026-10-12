// Generated from src-tauri/src/commands/bindings.rs by `cargo test`. Do not edit.
import type { ActivityView } from "./ActivityView";
import type { ExpandSnippet } from "./ExpandSnippet";
import type { InstallProgress } from "./InstallProgress";
import type { Item } from "./Item";
import type { RuntimeMessage } from "../wire";
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
  /** Opens the settings window, or brings it back if it is already open. */
  open_settings: { args: null; result: null };
  /** Binds a global hotkey to a root search item; an empty accelerator removes it. */
  set_item_hotkey: { args: { id: string; title: string; accelerator: string }; result: null };
  show_window: { args: null; result: null };
  get_item: { args: { id: string }; result: Item | null };
  /** Quicklinks, for offering the ones that take a query as fallbacks. */
  list_quicklinks: { args: null; result: Array<Item> };
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
  "set_item_hotkey",
  "show_window",
  "record_usage",
  "set_favorite",
  "move_favorite",
  "set_alias",
  "reset_ranking",
  "copy_text",
  "open_target",
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
  dictationInstall: InstallProgress;
  ocrInstall: InstallProgress;
  expandSnippet: ExpandSnippet;
  clipboardMeaningInstall: InstallProgress;
  activities: Array<ActivityView>;
};
