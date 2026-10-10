import { builtins as catalogue, type BuiltinName } from "../../../shared/bindings/builtins";
import type { Extension } from "../context";
import { ClipboardHistory } from "./clipboard";
import { CreateQuicklink, CreateSnippet, SearchQuicklinks, SearchSnippets } from "./library";
import { SelectionActions } from "./selection";
import { Extensions, Store } from "./store";
import { AudioDevices, KillProcess, SearchBookmarks, SearchFiles, SearchHistory, SwitchWindows, systemCommands } from "./system";
import { AskAi } from "./ask";
import { AiChat } from "./chat";
import { EmojiPicker } from "./emoji";
import { Timer } from "./timer";

/// Heyo's own commands, run like an extension. The list is the host's
/// (`BUILTINS` in extensions.rs), which shows them under the `@heyo` name.
export const builtinExtension: Extension = {
  dir: "",
  manifest: { name: "@heyo", title: "Heyo", commands: catalogue.map((command) => ({ ...command })) },
};

/// What runs for each. A command the host lists and this lacks does not compile.
export const builtins: Record<BuiltinName, unknown> = {
  store: Store,
  extensions: Extensions,
  "clipboard-history": ClipboardHistory,
  selection: SelectionActions,
  "ask-ai": AskAi,
  "ai-chat": AiChat,
  "create-quicklink": CreateQuicklink,
  "search-quicklinks": SearchQuicklinks,
  "create-snippet": CreateSnippet,
  "search-snippets": SearchSnippets,
  "switch-windows": SwitchWindows,
  emoji: EmojiPicker,
  "search-files": SearchFiles,
  "kill-process": KillProcess,
  "audio-devices": AudioDevices,
  "search-bookmarks": SearchBookmarks,
  "search-history": SearchHistory,
  timer: Timer,
  ...systemCommands,
};
