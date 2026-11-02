import { builtins as catalogue, type BuiltinName } from "../../../shared/bindings/builtins";
import type { Extension } from "../context";
import { t, translate } from "../../../shared/i18n";
import { ClipboardHistory } from "./clipboard";
import { CreateQuicklink, CreateSnippet, SearchQuicklinks, SearchSnippets } from "./library";
import { SelectionActions } from "./selection";
import { Extensions, Store } from "./store";
import { AudioDevices, KillProcess, SearchBookmarks, SearchFiles, SearchHistory, SwitchWindows, systemCommands } from "./system";
import { AskAi } from "./ask";
import { AiChat } from "./chat";
import { EmojiPicker } from "./emoji";
import { Timer } from "./timer";
import { ConnectionItemPreview } from "./connection";

/// Commands of Heyo's own that the host starts itself and the root search
/// does not list, so that they are not among the host's (`BUILTINS`).
const unlisted = [
  // An item a connector synced, previewed (`preview_connection`).
  { name: "connection-item", title: () => t("Preview"), mode: "view" as const, icon: "eye-16", run: ConnectionItemPreview },
];

/// Heyo's own commands, run like an extension. The list is the host's
/// (`BUILTINS` in extensions.rs), which shows them under the `@heyo` name.
/// Their titles are the host's too, and marked for translating there: a
/// command is named in the language spoken when it is asked for.
export const builtinExtension: Extension = {
  dir: "",
  manifest: {
    name: "@heyo",
    title: "Heyo",
    get commands() {
      const listed = catalogue.map((command) => ({ ...command, title: translate(command.title) }));
      return [...listed, ...unlisted.map(({ run: _, title, ...command }) => ({ ...command, title: title() }))];
    },
  },
};

/// What runs for each. A command the host lists and this lacks does not compile.
export const builtins: Record<BuiltinName, unknown> & Record<string, unknown> = {
  ...Object.fromEntries(unlisted.map((command) => [command.name, command.run])),
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
