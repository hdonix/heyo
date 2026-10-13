//! What a row of the root search is and does.
//!
//! The list has four kinds of row: what is in progress (the live
//! activities), the calculator's answer, what the search found
//! (applications, commands, quicklinks, snippets), and what else the query
//! can be used with. `rowsFor` puts them in order, each with the key it is
//! told apart by and the heading it stands under. `describe` says what a row
//! does: its actions in groups, the first of the first group being what
//! Enter, a click on the row and the footer's button do.
//!
//! Nothing here draws or keeps anything. What an action does, it does by
//! calling the host, and through the `Palette` it is handed: the palette
//! around the list.

import type { ActivityView } from "../shared/bindings/ActivityView";
import type { Item } from "../shared/bindings/Item";
import { expand, needsArgument } from "../shared/placeholders";
import { afterAlias, argumentsOf, filled, missing, type Argument } from "./arguments";
import type { Calculation } from "./calculator";
import { call, ignored, reported } from "./host";
import type { ActionSection, PanelAction } from "./raycast/actions";
import { t } from "../shared/i18n";

const SETTINGS_ID = "heyo:settings";

/// What of the palette a row's actions act through.
export interface Palette {
  showError(title: string, message?: string): void;
  showSuccess(title: string): void;
  openSettings(): void;
  /// Asks for one line of text; null when none is given.
  ask(question: { title: string; placeholder: string; initial?: string }): Promise<string | null>;
  /// Asks for a hotkey for the item, and binds it.
  askHotkey(item: Item): void;
  /// A required argument of the row was left empty and nothing opened: its
  /// field, the one at `field` among the row's, wants the keyboard.
  fillIn(key: string, field: number): void;
  /// A row opened with what its fields held, which they need not keep.
  argumentsSent(): void;
}

/// What the palette holds that a row's actions depend on.
export interface Context {
  /// What is typed in the search field.
  query: string;
  /// What was typed into the argument fields, and of which row.
  typed: { key: string; values: Record<string, string> } | null;
  /// The list is folded away (compact mode), and the argument fields with it.
  folded: boolean;
}

/// What to do with a query nothing else answers: "Search Google for …".
interface Fallback {
  id: string;
  title: string;
  icon: string;
  run(query: string, palette: Palette): void;
}

export type Row = {
  /// What the row is told apart by, through a change of the rows.
  key: string;
  /// The title over it, where a section of the list begins.
  heading?: string;
} & (
  | { type: "activity"; activity: ActivityView }
  | { type: "calc"; calc: Calculation }
  | { type: "item"; item: Item }
  | { type: "fallback"; fallback: Fallback }
);

export interface RowAction extends PanelAction {
  icon: string;
  run(): void;
}

export interface Description {
  /// The row's name, over its actions in the panel.
  title: string;
  /// The fields it has in the search bar while it is selected, and what they hold.
  fields: Argument[];
  values: Record<string, string>;
  actions: ActionSection<RowAction>[];
  /// The first action of the first group.
  primary?: RowAction;
}

/// What there is to show for a query.
export interface Found {
  activities: ActivityView[];
  calc: Calculation | null;
  items: Item[];
  /// Every quicklink: those that take a query double as fallbacks.
  quicklinks: Item[];
}

/// What kind of thing an item is, as its row says it.
export function kindName(kind: Item["kind"]): string {
  switch (kind) {
    case "Application":
      return t("Application");
    case "Command":
      return t("Command");
    case "Quicklink":
      return t("Quicklink");
    case "Snippet":
      return t("Snippet");
  }
}

/// The title of a section the host put an item in. The host names the
/// section in English, which is also what it is told apart by.
function sectionTitle(section: string): string {
  return section === "Favorites" ? t("Favorites") : section === "Suggestions" ? t("Suggestions") : section;
}

/// Whether an activity answers a query: by its title, what it is, or whose it is.
function activityMatches(activity: ActivityView, query: string): boolean {
  const q = query.trim().toLowerCase();
  if (!q) return true;
  return [activity.title, activity.name, activity.extensionTitle, activity.subtitle ?? ""].some((text) => text.toLowerCase().includes(q));
}

/// The rows for a query, in the order they are listed.
export function rowsFor(query: string, found: Found): Row[] {
  const text = query.trim();
  const activities = found.activities.filter((activity) => activityMatches(activity, query));
  const above = activities.length > 0 || found.calc !== null;
  let section: string | undefined;
  return [
    ...activities.map((activity, i): Row => ({ type: "activity", key: `activity-${activity.key}`, heading: i === 0 ? t("In Progress") : undefined, activity })),
    ...(found.calc ? [{ type: "calc", key: "calc", heading: t("Calculator"), calc: found.calc } satisfies Row] : []),
    ...found.items.map((item, i): Row => {
      // What was found needs a title only under something else.
      const heading = item.section && item.section !== section ? sectionTitle(item.section) : above && i === 0 ? t("Results") : undefined;
      section = item.section;
      return { type: "item", key: item.id, heading, item };
    }),
    ...(text ? fallbacks(found.quicklinks) : []).map(
      (fallback, i): Row => ({ type: "fallback", key: `fallback-${fallback.id}`, heading: i === 0 ? t("Use “{0}” with…", text) : undefined, fallback }),
    ),
  ];
}

/// What a row does, in the palette as `context` has it.
export function describe(row: Row, context: Context, palette: Palette): Description {
  const { title, groups, fields = [], values = {} } = ofKind(row, context, palette);
  const actions = groups.filter((group) => group.length).map((group) => ({ actions: group }));
  return { title, fields, values, actions, primary: actions[0]?.actions[0] };
}

/// What each kind of row says of itself; `describe` makes the rest of that.
interface Own {
  title: string;
  groups: RowAction[][];
  fields?: Argument[];
  values?: Record<string, string>;
}

function ofKind(row: Row, context: Context, palette: Palette): Own {
  switch (row.type) {
    case "activity":
      return ofActivity(row.activity, palette);
    case "calc":
      return ofCalculation(row.calc);
    case "item":
      return ofItem(row.item, context, palette);
    case "fallback":
      return ofFallback(row.fallback, context, palette);
  }
}

function ofActivity(activity: ActivityView, palette: Palette): Own {
  const failed = (e: unknown) => palette.showError(t("Could not act on {0}", activity.name), String(e));
  const act = (button: number | null) => void call("activity_action", { key: activity.key, button }).catch(failed);
  return {
    title: activity.title,
    groups: [
      // Enter does what the activity itself does, else its first button.
      [
        ...(activity.hasAction ? [{ title: t("Open"), icon: "arrow-right-16", run: () => act(null) }] : []),
        ...activity.buttons.map((button, index) => ({ title: button.title, icon: button.icon ?? "circle-16", run: () => act(index) })),
      ],
      activity.dismissible
        ? [{ title: t("End Activity"), icon: "x-mark-circle-16", run: () => void call("activity_dismiss", { key: activity.key }).catch(failed) }]
        : [],
    ],
  };
}

function ofCalculation(calc: Calculation): Own {
  const copy = (text: string, paste: boolean) => void call("copy_text", { text, paste }).catch(reported(t("Could not copy")));
  return {
    title: t("Calculator"),
    groups: [
      [
        { title: t("Copy Answer"), icon: "copy-clipboard-16", run: () => copy(calc.value, false) },
        { title: t("Paste Answer"), icon: "clipboard-16", shortcut: { modifiers: ["ctrl"], key: "return" }, run: () => copy(calc.value, true) },
        {
          title: t("Copy Question and Answer"),
          icon: "copy-clipboard-16",
          shortcut: { modifiers: ["ctrl", "shift"], key: "c" },
          run: () => copy(`${calc.question} = ${calc.answer}`, false),
        },
      ],
    ],
  };
}

function ofFallback(fallback: Fallback, context: Context, palette: Palette): Own {
  return { title: fallback.title, groups: [[{ title: fallback.title, icon: fallback.icon, run: () => fallback.run(context.query.trim(), palette) }]] };
}

/// Hands a question to the AI model.
export function askAi(question: string, palette: Palette) {
  void call("launch", { id: "cmd:@heyo:ask-ai", fallbackText: question }).catch((e) => palette.showError(t("Could not ask AI"), String(e)));
}

function fallbacks(quicklinks: Item[]): Fallback[] {
  const failed = (palette: Palette) => (e: unknown) => palette.showError(t("Could not open"), String(e));
  return [
    {
      id: "google",
      title: t("Search Google"),
      icon: "magnifying-glass-16",
      run: (q, palette) => void call("open_target", { target: `https://www.google.com/search?q=${encodeURIComponent(q)}` }).catch(failed(palette)),
    },
    { id: "ask-ai", title: t("Ask AI"), icon: "stars-16", run: askAi },
    {
      id: "files",
      title: t("Search Files"),
      icon: "blank-document-16",
      run: (q, palette) => void call("launch", { id: "cmd:@heyo:search-files", fallbackText: q }).catch(failed(palette)),
    },
    ...quicklinks
      .filter((link) => needsArgument(link.link ?? ""))
      .map((link) => ({
        id: link.id,
        title: link.title,
        icon: link.icon ?? "link-16",
        run: (q: string, palette: Palette) => void openQuicklink(link, palette, q).catch(failed(palette)),
      })),
  ];
}

/// What an item's fields hold. The text after its alias ("heyo" in
/// "gh heyo") is its first argument until that field is typed in.
const valuesOf = (item: Item, { typed, query }: Context) =>
  filled(argumentsOf(item), typed?.key === item.id ? typed.values : {}, afterAlias(item.alias, query));

function ofItem(item: Item, context: Context, palette: Palette): Own {
  // With no fields on screen to fill in, the item asks by itself.
  const fields = context.folded ? [] : argumentsOf(item);
  const values = valuesOf(item, context);
  const { id } = item;

  /// Opens the item with what its fields hold. A required field left empty
  /// keeps it from opening, and is asked for instead.
  const openWithArguments = () => {
    const empty = missing(fields, values);
    if (empty.length) return palette.fillIn(id, fields.findIndex((field) => field.name === empty[0]));
    if (fields.length) palette.argumentsSent();
    // Only the ranking would be the poorer for it.
    void call("record_usage", { id }).catch(ignored);
    void open(item, palette, fields.length ? values : undefined);
  };

  const primary: RowAction[] = [
    {
      title: item.kind === "Application" ? t("Open Application") : item.kind === "Snippet" ? t("Paste Snippet") : item.kind === "Quicklink" ? t("Open Quicklink") : t("Open Command"),
      icon: item.kind === "Snippet" ? "clipboard-16" : "arrow-right-16",
      run: openWithArguments,
    },
  ];
  if (item.kind === "Snippet") {
    primary.push({
      title: t("Copy Snippet"),
      icon: "copy-clipboard-16",
      shortcut: { modifiers: ["ctrl"], key: "return" },
      // With its argument when one was typed; else it asks.
      run: () => void pasteSnippet(item, palette, false, values.query || undefined),
    });
  }
  if (item.kind === "Quicklink") {
    primary.push({
      title: t("Copy Link"),
      icon: "copy-clipboard-16",
      shortcut: { modifiers: ["ctrl", "shift"], key: "c" },
      run: () => void call("copy_text", { text: item.link ?? "", paste: false }).catch(reported(t("Could not copy"))),
    });
  }

  const manage: RowAction[] = [
    {
      title: item.favorite ? t("Remove from Favorites") : t("Add to Favorites"),
      icon: item.favorite ? "star-disabled-16" : "star-16",
      shortcut: { modifiers: ["ctrl", "shift"], key: "f" },
      run: () => void call("set_favorite", { id, favorite: !item.favorite }).catch(reported(t("Could not change favorites"))),
    },
  ];
  if (item.favorite && item.section === "Favorites") {
    manage.push(
      {
        title: t("Move Up in Favorites"),
        icon: "arrow-up-16",
        shortcut: { modifiers: ["ctrl", "alt"], key: "arrowUp" },
        run: () => void call("move_favorite", { id, offset: -1 }).catch(reported(t("Could not change favorites"))),
      },
      {
        title: t("Move Down in Favorites"),
        icon: "arrow-down-16",
        shortcut: { modifiers: ["ctrl", "alt"], key: "arrowDown" },
        run: () => void call("move_favorite", { id, offset: 1 }).catch(reported(t("Could not change favorites"))),
      },
    );
  }
  manage.push(
    {
      title: item.alias ? t("Edit Alias") : t("Set Alias"),
      icon: "tag-16",
      shortcut: { modifiers: ["ctrl", "shift"], key: "a" },
      run: () => void editAlias(item, palette),
    },
    {
      title: item.hotkey ? t("Change Hotkey") : t("Set Hotkey"),
      icon: "keyboard-16",
      shortcut: { modifiers: ["ctrl", "shift"], key: "h" },
      run: () => palette.askHotkey(item),
    },
  );
  if (item.hotkey) {
    manage.push({
      title: t("Remove Hotkey"),
      icon: "x-mark-circle-16",
      run: () =>
        void call("set_item_hotkey", { id, title: item.title, accelerator: "" }).catch((e) => palette.showError(t("Could not remove hotkey"), String(e))),
    });
  }
  manage.push({
    title: t("Reset Ranking"),
    icon: "arrow-counter-clockwise-16",
    run: () =>
      void call("reset_ranking", { id })
        .then(() => palette.showSuccess(t("Reset ranking of {0}", item.title)))
        .catch(reported(t("Could not reset ranking"))),
  });
  if (item.kind === "Command" || item.kind === "Quicklink" || item.kind === "Snippet") {
    manage.push({ title: t("Open Settings"), icon: "cog-16", shortcut: { modifiers: ["ctrl"], key: "," }, run: palette.openSettings });
  }
  return { title: item.title, fields, values, groups: [primary, manage] };
}

/// Opens an item. `given` are its arguments from the search bar; without
/// them (a hotkey, `heyo --open`) what needs an argument asks for it.
export async function open(item: Item, palette: Palette, given?: Record<string, string>): Promise<void> {
  try {
    if (item.id === SETTINGS_ID) return palette.openSettings();
    if (item.kind === "Quicklink") return await openQuicklink(item, palette, given?.query);
    if (item.kind === "Snippet") return await pasteSnippet(item, palette, true, given?.query);
    // Applications, extensions' commands, and Heyo's own (screenshots, …), which run in the host.
    await call("launch", { id: item.id, arguments: given });
  } catch (e) {
    palette.showError(t("Could not open {0}", item.title), String(e));
  }
}

const readClipboard = () => call("read_clipboard");

async function openQuicklink(item: Item, palette: Palette, argument?: string): Promise<void> {
  const link = item.link ?? "";
  if (needsArgument(link) && argument === undefined) {
    // Opened by a hotkey while hidden: show the window to ask.
    await call("show_window");
    const asked = await palette.ask({ title: item.title, placeholder: t("Query") });
    if (asked === null) return;
    argument = asked;
  }
  const isUrl = /^[a-z][\w+.-]*:/i.test(link);
  const target = await expand(link, { query: argument, encode: isUrl, readClipboard });
  await call("open_target", { target });
}

async function pasteSnippet(item: Item, palette: Palette, paste: boolean, argument?: string): Promise<void> {
  if (needsArgument(item.text ?? "") && argument === undefined) {
    await call("show_window");
    const asked = await palette.ask({ title: item.title, placeholder: t("Text for {0}", "{argument}") });
    if (asked === null) return;
    argument = asked;
  }
  const text = await expand(item.text ?? "", { query: argument, readClipboard });
  await call("copy_text", { text, paste });
}

async function editAlias(item: Item, palette: Palette) {
  const alias = await palette.ask({ title: t("Alias for {0}", item.title), placeholder: t("e.g. cb — leave empty to remove"), initial: item.alias ?? "" });
  if (alias !== null) void call("set_alias", { id: item.id, alias }).catch((e) => palette.showError(t("Could not set alias"), String(e)));
}
