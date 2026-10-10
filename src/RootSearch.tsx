// The root search: applications, commands, quicklinks, snippets and the
// calculator, with favorites, aliases and a Ctrl+K action panel.

import { composing } from "./ime";
import { useEffect, useMemo, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { ArrowRight, Star } from "lucide-react";

import type { Item } from "../shared/bindings/Item";
import { expand, needsArgument } from "../shared/placeholders";
import { calculate, loadRates, type Calculation, type Rates } from "./calculator";
import { HotkeyHint, HotkeyRecorder } from "./hotkey";
import { ItemIcon } from "./ItemIcon";
import { ArgumentFields } from "./ArgumentFields";
import { afterAlias, argumentsOf, filled, missing, tabStop } from "./arguments";
import { FIRST, hold, locate } from "./reselect";
import { step, underPointer, useReveal } from "./selection";
import { SelectionIndicator } from "./SelectionIndicator";
import { actionForShortcut, ActionOverlay, type ActionSection, ShortcutHint } from "./raycast/actions";
import { ToastView } from "./raycast/CommandView";
import type { Node, Toast } from "./raycast/model";

const SETTINGS_ID = "heyo:settings";
const SEARCH_PLACEHOLDER = "Search apps, commands, or calculate…";

export type { Item };

/// What to do with a query nothing else answers: "Search Google for …".
interface Fallback {
  id: string;
  title: string;
  icon: string;
  run(query: string): void;
}

type Row = { type: "calc"; calc: Calculation } | { type: "item"; item: Item } | { type: "fallback"; fallback: Fallback };

/// What a row is told apart by, through a change of the rows.
const keyOf = (row: Row) => (row.type === "calc" ? "calc" : row.type === "item" ? row.item.id : `fallback-${row.fallback.id}`);

interface RootAction {
  title: string;
  icon: string;
  shortcut?: { modifiers: string[]; key: string };
  run(): void;
}

const readClipboard = () => invoke<string>("read_clipboard");

/// A small modal asking for one line of text.
function Prompt(props: {
  title: string;
  placeholder: string;
  initial?: string;
  onSubmit(value: string): void;
  onCancel(): void;
}) {
  const [value, setValue] = useState(props.initial ?? "");
  const inputRef = useRef<HTMLInputElement>(null);
  // Take focus after the action panel has handed it back to the search bar.
  useEffect(() => {
    const timer = setTimeout(() => inputRef.current?.focus());
    return () => clearTimeout(timer);
  }, []);
  return (
    <div className="overlay-backdrop center" onMouseDown={props.onCancel}>
      <div
        className="alert prompt"
        onMouseDown={(e) => e.stopPropagation()}
        onKeyDown={(e) => {
          e.stopPropagation();
          if (composing(e)) return;
          if (e.key === "Escape") props.onCancel();
          else if (e.key === "Enter") props.onSubmit(value);
        }}
      >
        <div className="alert-title">{props.title}</div>
        <input
          ref={inputRef}
          className="field-input"
          placeholder={props.placeholder}
          value={value}
          onChange={(e) => setValue(e.target.value)}
          spellCheck={false}
        />
        <div className="alert-buttons">
          <button onClick={props.onCancel}>Cancel</button>
          <button className="primary" onClick={() => props.onSubmit(value)}>
            Done
          </button>
        </div>
      </div>
    </div>
  );
}

/// Records a global hotkey for a root search item.
function HotkeyPrompt(props: { item: Item; onDone(): void }) {
  const [error, setError] = useState<string | null>(null);
  const save = (accelerator: string) =>
    invoke("set_item_hotkey", { id: props.item.id, title: props.item.title, accelerator })
      .then(props.onDone)
      .catch((e) => setError(String(e)));
  return (
    <div className="overlay-backdrop center" onMouseDown={props.onDone}>
      <div
        className="alert prompt"
        onMouseDown={(e) => e.stopPropagation()}
        onKeyDown={(e) => {
          e.stopPropagation();
          if (e.key === "Escape") props.onDone();
        }}
      >
        <div className="alert-title">Hotkey for {props.item.title}</div>
        <div className="prompt-hotkey">
          <HotkeyRecorder value={props.item.hotkey ?? null} onChange={save} autoStart />
        </div>
        <div className={error ? "prompt-hint error" : "prompt-hint"}>
          {error ?? "Press a key combination with Ctrl, Alt, Shift or Super. It works anywhere on your desktop."}
        </div>
        <div className="alert-buttons">
          <button onClick={props.onDone}>Close</button>
        </div>
      </div>
    </div>
  );
}

function CalculatorCard({ calc, selected }: { calc: Calculation; selected: boolean }) {
  return (
    <div className={`calc${selected ? " selected" : ""}`}>
      <div className="calc-side">
        <div className="calc-value">{calc.question}</div>
      </div>
      <ArrowRight className="calc-arrow" size={20} />
      <div className="calc-side">
        <div className="calc-value">{calc.answer}</div>
        <div className="calc-label">{calc.label}</div>
      </div>
    </div>
  );
}

export function RootSearch(props: {
  query: string;
  setQuery(query: string): void;
  inputRef: React.RefObject<HTMLInputElement | null>;
  /// Bumped when the index changes or the window is shown.
  version: number;
  /// Bumped each time the window is shown.
  shown: number;
  toast: Toast | null;
  onToastAction(toast: Toast): void;
  showError(title: string, message?: string): void;
  showSuccess(title: string): void;
  openSettings(): void;
  /// Show only the search field until something is typed.
  compact: boolean;
  /// An item to open from `heyo --open <id>` (quicklinks and snippets).
  pendingOpen: string | null;
  onPendingHandled(): void;
  dialog: React.ReactNode;
}) {
  const { query } = props;
  const [items, setItems] = useState<Item[]>([]);
  // The selection is a row, not a place: see `reselect.ts`.
  const [held, setHeld] = useState(FIRST);
  const [rates, setRates] = useState<Rates | null>(null);
  const [panelOpen, setPanelOpen] = useState(false);
  const [prompt, setPrompt] = useState<React.ComponentProps<typeof Prompt> | null>(null);
  const [hotkeyItem, setHotkeyItem] = useState<Item | null>(null);
  // What was typed into the argument fields, and for which item.
  const [typed, setTyped] = useState<{ id: string; values: Record<string, string> } | null>(null);
  // The item whose launch was refused for a required argument left empty.
  const [refused, setRefused] = useState<string | null>(null);
  const fieldRefs = useRef<(HTMLElement | null)[]>([]);

  // A panel or prompt left open when the window hid would still be there on
  // return, holding the keyboard while the search bar has the focus.
  // Arguments typed and not sent (a password, perhaps) do not wait either.
  useEffect(() => {
    setPanelOpen(false);
    setPrompt(null);
    setHotkeyItem(null);
    setTyped(null);
    setRefused(null);
  }, [props.shown]);
  // Drops responses that arrive after a newer query was sent.
  const requestId = useRef(0);
  // What the results on screen answer: other results for the same are a
  // refresh, through which the selection stays where it is.
  const answered = useRef<{ query: string; shown: number } | null>(null);

  useEffect(() => {
    const id = ++requestId.current;
    invoke<Item[]>("search", { query }).then((results) => {
      if (id !== requestId.current) return;
      setItems(results);
      if (answered.current?.query !== query || answered.current.shown !== props.shown) setHeld(FIRST);
      answered.current = { query, shown: props.shown };
    });
  }, [query, props.version]);

  useEffect(() => {
    loadRates().then(setRates);
  }, [props.version]);

  // Quicklinks that take a query double as fallbacks.
  const [queryLinks, setQueryLinks] = useState<Item[]>([]);
  useEffect(() => {
    invoke<Item[]>("list_quicklinks").then((links) => setQueryLinks(links.filter((l) => needsArgument(l.link ?? ""))));
  }, [props.version]);

  /// Hands the query to the AI model as a question.
  const askAi = (question: string) =>
    void invoke("launch", { id: "cmd:@heyo:ask-ai", fallbackText: question }).catch((e) => props.showError("Could not ask AI", String(e)));

  const calc = useMemo(() => calculate(query, rates), [query, rates]);
  const fallbacks: Fallback[] = useMemo(() => {
    if (!query.trim()) return [];
    const failed = (e: unknown) => props.showError("Could not open", String(e));
    return [
      {
        id: "google",
        title: "Search Google",
        icon: "magnifying-glass-16",
        run: (q) => invoke("open_target", { target: `https://www.google.com/search?q=${encodeURIComponent(q)}` }).catch(failed),
      },
      { id: "ask-ai", title: "Ask AI", icon: "stars-16", run: askAi },
      {
        id: "files",
        title: "Search Files",
        icon: "blank-document-16",
        run: (q) => invoke("launch", { id: "cmd:@heyo:search-files", fallbackText: q }).catch(failed),
      },
      ...queryLinks.map((link) => ({ id: link.id, title: link.title, icon: link.icon ?? "link-16", run: (q: string) => void openQuicklink(link, q).catch(failed) })),
    ];
  }, [query.trim() === "", queryLinks]);
  const rows: Row[] = useMemo(
    () => [
      ...(calc ? [{ type: "calc" as const, calc }] : []),
      ...items.map((item) => ({ type: "item" as const, item })),
      ...fallbacks.map((fallback) => ({ type: "fallback" as const, fallback })),
    ],
    [calc, items, fallbacks],
  );
  const keys = useMemo(() => rows.map(keyOf), [rows]);
  const selected = locate(keys, held);
  const current = rows[selected];
  const setSelected = (index: number | ((from: number) => number)) =>
    setHeld((h) => hold(keys, typeof index === "function" ? index(locate(keys, h)) : index));
  // The held row moved, or went and left its place to a neighbour: hold
  // what is selected now, so that the next change starts from here.
  useEffect(() => {
    if (held.key !== null && (keys[selected] !== held.key || selected !== held.index)) setHeld(hold(keys, selected));
  }, [keys]);

  const listRef = useReveal<HTMLUListElement>([selected]);

  const refocus = () => setTimeout(() => props.inputRef.current?.focus());

  // Compact: the search field alone, until there is something to show for.
  const collapsed = props.compact && !query.trim() && !panelOpen && !props.toast && !props.dialog;

  // The selected item's arguments are fields in the search bar (not while
  // the list is folded away, and with it what is selected).
  const fields = useMemo(() => (current?.type === "item" && !collapsed ? argumentsOf(current.item) : []), [current, collapsed]);
  /// What an item's fields hold. The text after its alias ("heyo" in
  /// "gh heyo") is its first argument until that field is typed in.
  const valuesOf = (item: Item) => filled(argumentsOf(item), typed?.id === item.id ? typed.values : {}, afterAlias(item.alias, query));
  const values = current?.type === "item" ? valuesOf(current.item) : {};
  const lacking = current?.type === "item" && refused === current.item.id ? missing(fields, values) : [];
  fieldRefs.current.length = fields.length;
  /// Which of the search field (0) and the argument fields (1…) has the focus.
  const stopOf = (element: EventTarget | null) => (element ? fieldRefs.current.indexOf(element as HTMLElement) + 1 : 0);
  const focusStop = (stop: number) => (stop === 0 ? props.inputRef.current : fieldRefs.current[stop - 1])?.focus();
  // A field that goes (another row was selected) takes the focus with it:
  // without one, no key would reach the palette.
  const fieldsOf = current?.type === "item" ? current.item.id : null;
  useEffect(() => {
    if (!document.activeElement || document.activeElement === document.body) props.inputRef.current?.focus();
  }, [fieldsOf, fields.length]);

  const openQuicklink = async (item: Item, argument?: string) => {
    const link = item.link ?? "";
    if (needsArgument(link) && argument === undefined) {
      // Opened by a hotkey while hidden: show the window to ask.
      await invoke("show_window");
      setPrompt({
        title: item.title,
        placeholder: "Query",
        onSubmit: (value) => {
          setPrompt(null);
          openQuicklink(item, value);
        },
        onCancel: () => {
          setPrompt(null);
          refocus();
        },
      });
      return;
    }
    const isUrl = /^[a-z][\w+.-]*:/i.test(link);
    const target = await expand(link, { query: argument, encode: isUrl, readClipboard });
    await invoke("open_target", { target });
  };

  const pasteSnippet = async (item: Item, paste: boolean, argument?: string) => {
    if (needsArgument(item.text ?? "") && argument === undefined) {
      await invoke("show_window");
      setPrompt({
        title: item.title,
        placeholder: "Text for {argument}",
        onSubmit: (value) => {
          setPrompt(null);
          pasteSnippet(item, paste, value);
        },
        onCancel: () => {
          setPrompt(null);
          refocus();
        },
      });
      return;
    }
    const text = await expand(item.text ?? "", { query: argument, readClipboard });
    await invoke("copy_text", { text, paste });
  };

  /// Opens an item; `given` are its arguments from the search bar. Without
  /// them (a hotkey, `heyo --open`) what needs an argument asks for it.
  const open = async (item: Item, recordUsage = true, given?: Record<string, string>) => {
    if (recordUsage) invoke("record_usage", { id: item.id });
    try {
      if (item.id === SETTINGS_ID) return props.openSettings();
      // Heyo's native commands (screenshots, …) run in the host.
      if (item.kind === "Quicklink") return await openQuicklink(item, given?.query);
      if (item.kind === "Snippet") return await pasteSnippet(item, true, given?.query);
      await invoke("launch", { id: item.id, arguments: given });
    } catch (e) {
      props.showError(`Could not open ${item.title}`, String(e));
    }
  };

  /// Opens an item from the list with what its fields hold. A required
  /// field left empty gets the focus instead, and nothing opens.
  const openWithArguments = (item: Item) => {
    // With no fields on screen to fill in, the item asks by itself.
    const asked = collapsed ? [] : argumentsOf(item);
    if (!asked.length) return void open(item);
    const given = valuesOf(item);
    const empty = missing(asked, given);
    if (!empty.length) {
      setTyped(null);
      setRefused(null);
      return void open(item, true, given);
    }
    // Clicked, the row need not be the selected one yet.
    const index = keys.indexOf(item.id);
    if (index !== -1) setHeld({ key: item.id, index });
    setRefused(item.id);
    // After the field is there, and after the action panel gave the focus back.
    setTimeout(() => setTimeout(() => focusStop(asked.findIndex((a) => a.name === empty[0]) + 1)));
  };

  // `heyo --open <id>`: the host already counted the use.
  useEffect(() => {
    if (!props.pendingOpen) return;
    const id = props.pendingOpen;
    props.onPendingHandled();
    invoke<Item | null>("get_item", { id }).then((item) => item && open(item, false));
  }, [props.pendingOpen]);

  const editAlias = (item: Item) =>
    setPrompt({
      title: `Alias for ${item.title}`,
      placeholder: "e.g. cb — leave empty to remove",
      initial: item.alias ?? "",
      onSubmit: (alias) => {
        setPrompt(null);
        invoke("set_alias", { id: item.id, alias }).catch((e) => props.showError("Could not set alias", String(e)));
        refocus();
      },
      onCancel: () => {
        setPrompt(null);
        refocus();
      },
    });

  const actionsFor = (row: Row | undefined): RootAction[][] => {
      if (!row) return [];
      if (row.type === "calc") {
        const { calc } = row;
        return [
          [
            { title: "Copy Answer", icon: "copy-clipboard-16", run: () => invoke("copy_text", { text: calc.value, paste: false }) },
            {
              title: "Paste Answer",
              icon: "clipboard-16",
              shortcut: { modifiers: ["ctrl"], key: "return" },
              run: () => invoke("copy_text", { text: calc.value, paste: true }),
            },
            {
              title: "Copy Question and Answer",
              icon: "copy-clipboard-16",
              shortcut: { modifiers: ["ctrl", "shift"], key: "c" },
              run: () => invoke("copy_text", { text: `${calc.question} = ${calc.answer}`, paste: false }),
            },
          ],
        ];
      }
      if (row.type === "fallback") {
        const { fallback } = row;
        return [[{ title: fallback.title, icon: fallback.icon, run: () => fallback.run(query.trim()) }]];
      }
      const { item } = row;
      const primary: RootAction[] = [
        {
          title:
            item.kind === "Application" ? "Open Application" : item.kind === "Snippet" ? "Paste Snippet" : item.kind === "Quicklink" ? "Open Quicklink" : "Open Command",
          icon: item.kind === "Snippet" ? "clipboard-16" : "arrow-right-16",
          run: () => openWithArguments(item),
        },
      ];
      if (item.kind === "Snippet") {
        primary.push({
          title: "Copy Snippet",
          icon: "copy-clipboard-16",
          shortcut: { modifiers: ["ctrl"], key: "return" },
          // With its argument when one was typed; else it asks.
          run: () => pasteSnippet(item, false, valuesOf(item).query || undefined),
        });
      }
      if (item.kind === "Quicklink") {
        primary.push({
          title: "Copy Link",
          icon: "copy-clipboard-16",
          shortcut: { modifiers: ["ctrl", "shift"], key: "c" },
          run: () => invoke("copy_text", { text: item.link ?? "", paste: false }),
        });
      }
      const manage: RootAction[] = [
        item.favorite
          ? {
              title: "Remove from Favorites",
              icon: "star-disabled-16",
              shortcut: { modifiers: ["ctrl", "shift"], key: "f" },
              run: () => invoke("set_favorite", { id: item.id, favorite: false }),
            }
          : {
              title: "Add to Favorites",
              icon: "star-16",
              shortcut: { modifiers: ["ctrl", "shift"], key: "f" },
              run: () => invoke("set_favorite", { id: item.id, favorite: true }),
            },
      ];
      if (item.favorite && item.section === "Favorites") {
        manage.push(
          {
            title: "Move Up in Favorites",
            icon: "arrow-up-16",
            shortcut: { modifiers: ["ctrl", "alt"], key: "arrowUp" },
            run: () => invoke("move_favorite", { id: item.id, offset: -1 }),
          },
          {
            title: "Move Down in Favorites",
            icon: "arrow-down-16",
            shortcut: { modifiers: ["ctrl", "alt"], key: "arrowDown" },
            run: () => invoke("move_favorite", { id: item.id, offset: 1 }),
          },
        );
      }
      manage.push(
        {
          title: item.alias ? "Edit Alias" : "Set Alias",
          icon: "tag-16",
          shortcut: { modifiers: ["ctrl", "shift"], key: "a" },
          run: () => editAlias(item),
        },
        {
          title: item.hotkey ? "Change Hotkey" : "Set Hotkey",
          icon: "keyboard-16",
          shortcut: { modifiers: ["ctrl", "shift"], key: "h" },
          run: () => setHotkeyItem(item),
        },
        ...(item.hotkey
          ? [
              {
                title: "Remove Hotkey",
                icon: "x-mark-circle-16",
                run: () =>
                  invoke("set_item_hotkey", { id: item.id, title: item.title, accelerator: "" }).catch((e) =>
                    props.showError("Could not remove hotkey", String(e)),
                  ),
              },
            ]
          : []),
        {
          title: "Reset Ranking",
          icon: "arrow-counter-clockwise-16",
          run: () => {
            invoke("reset_ranking", { id: item.id });
            props.showSuccess(`Reset ranking of ${item.title}`);
          },
        },
      );
      if (item.kind === "Command" || item.kind === "Quicklink" || item.kind === "Snippet") {
        manage.push({ title: "Open Settings", icon: "cog-16", shortcut: { modifiers: ["ctrl"], key: "," }, run: props.openSettings });
      }
      return [primary, manage];
  };

  // The action panel works on runtime-style nodes; these stand in for them.
  const groups = actionsFor(current);
  const handlers = new Map<number, () => void>();
  const panelSections: ActionSection[] = groups.map((group, g) => ({
    actions: group.map((action, a) => {
      const id = -(g * 100 + a + 1);
      handlers.set(id, action.run);
      const node: Node = { id, type: "Action", props: { title: action.title, icon: action.icon, shortcut: action.shortcut }, children: [] };
      return node;
    }),
  }));
  const runNode = (node: Node) => {
    setPanelOpen(false);
    handlers.get(node.id)?.();
    refocus();
  };

  /// The pointer came over a row. While an argument is being typed that
  /// selects nothing: its fields, and what they hold, would go with the row.
  const pointAt = (index: number) => {
    if (!stopOf(document.activeElement)) setSelected(index);
  };

  const onKeyDown = (e: React.KeyboardEvent) => {
    if (panelOpen || prompt || hotkeyItem || composing(e)) return;
    // In an argument field (1…) rather than the search field (0).
    const stop = stopOf(e.target);
    const move = (delta: number) => {
      e.preventDefault();
      // The fields belong to the row being left.
      if (stop) props.inputRef.current?.focus();
      setSelected((s) => step(s, delta, rows.length, true));
    };
    if (e.key === "Tab" && !e.ctrlKey && !e.altKey && fields.length) {
      e.preventDefault();
      focusStop(tabStop(stop, e.shiftKey, fields.length));
    } else if (stop && e.key === "Escape") {
      e.preventDefault();
      focusStop(0);
    } else if (stop && e.key === "Backspace" && (e.target as HTMLInputElement).value === "") {
      // Out of an empty field, back to the one before it.
      e.preventDefault();
      focusStop(stop - 1);
    } else if (e.ctrlKey && e.code === "KeyK") {
      e.preventDefault();
      if (groups.length) setPanelOpen(true);
    } else if (e.ctrlKey && e.key === ",") {
      e.preventDefault();
      props.openSettings();
    } else if (e.key === "ArrowDown" && !e.ctrlKey && !e.altKey) move(1);
    else if (e.key === "ArrowUp" && !e.ctrlKey && !e.altKey) move(-1);
    else if (e.key === "Enter" && !e.ctrlKey && !e.shiftKey && !e.altKey) {
      e.preventDefault();
      groups[0]?.[0]?.run();
    } else if (e.key === "Tab" && !e.ctrlKey && !e.altKey && !e.shiftKey && query.trim()) {
      e.preventDefault();
      askAi(query.trim());
    } else if (e.key === "Escape") {
      if (query) props.setQuery("");
      else invoke("hide");
    } else if (e.ctrlKey || e.altKey) {
      const node = actionForShortcut(panelSections, e);
      if (node) {
        e.preventDefault();
        runNode(node);
      } else if (e.ctrlKey && e.key === "n") move(1);
      else if (e.ctrlKey && e.key === "p") move(-1);
    }
  };

  let lastSection: string | undefined;
  return (
    <div className={collapsed ? "panel compact" : "panel"} onKeyDown={onKeyDown}>
      <div className="search-row">
        {/* With arguments after it, the search field is only as wide as its text. */}
        <label className={fields.length ? "search-fit fitted" : "search-fit"} data-text={query || SEARCH_PLACEHOLDER}>
          <input
            ref={props.inputRef}
            className="search"
            placeholder={SEARCH_PLACEHOLDER}
            value={query}
            onChange={(e) => props.setQuery(e.target.value)}
            autoFocus
            spellCheck={false}
          />
        </label>
        {fields.length > 0 && current?.type === "item" ? (
          <>
            <ArgumentFields
              fields={fields}
              values={values}
              missing={lacking}
              onChange={(name, value) => {
                const { id } = current.item;
                setTyped((t) => ({ id, values: { ...(t?.id === id ? t.values : {}), [name]: value } }));
              }}
              register={(index, element) => (fieldRefs.current[index] = element)}
            />
            <span className="search-spacer" />
          </>
        ) : (
          <button className="search-hint" onClick={() => query.trim() && askAi(query.trim())} tabIndex={-1}>
            Ask AI <kbd>⇥</kbd>
          </button>
        )}
      </div>
      {!collapsed && (
      <ul className="list" ref={listRef}>
        <SelectionIndicator as="li" resetKey={query} />
        {rows.map((row, i) => {
          const isSelected = i === selected;
          if (row.type === "calc") {
            return (
              <li key="calc" className="calc-row" {...underPointer(() => pointAt(i))} onClick={() => groups[0]?.[0]?.run()}>
                <div className="section-title">Calculator</div>
                <CalculatorCard calc={row.calc} selected={isSelected} />
              </li>
            );
          }
          if (row.type === "fallback") {
            const first = rows[i - 1]?.type !== "fallback";
            return (
              <li key={`fallback-${row.fallback.id}`} className="item-wrap">
                {first && <div className="section-title">Use “{query.trim()}” with…</div>}
                <div className={isSelected ? "item selected" : "item"} {...underPointer(() => pointAt(i))} onClick={() => row.fallback.run(query.trim())}>
                  <ItemIcon icon={row.fallback.icon} title={row.fallback.title} />
                  <span className="title">{row.fallback.title}</span>
                  <span className="accessories">
                    <span className="kind">Fallback</span>
                  </span>
                </div>
              </li>
            );
          }
          const { item } = row;
          const header = item.section && item.section !== lastSection ? item.section : calc && i === 1 ? "Results" : undefined;
          lastSection = item.section;
          const argument = item.kind === "Quicklink" ? afterAlias(item.alias, query) : null;
          return (
            <li key={item.id} className="item-wrap">
              {header && <div className="section-title">{header}</div>}
              <div
                className={isSelected ? "item selected" : "item"}
                {...underPointer(() => pointAt(i))}
                onClick={() => openWithArguments(item)}
              >
                <ItemIcon icon={item.icon} title={item.title} colorKey={item.subtitle ? `${item.subtitle}/${item.title}` : item.title} />
                <span className="title">{item.title}</span>
                {argument ? (
                  <span className="subtitle argument">{argument}</span>
                ) : (
                  item.subtitle && <span className="subtitle">{item.subtitle}</span>
                )}
                <span className="accessories">
                  {item.hotkey && <HotkeyHint accelerator={item.hotkey} />}
                  {item.alias && <span className="tag alias">{item.alias}</span>}
                  {item.favorite && item.section !== "Favorites" && <Star size={13} className="favorite-star" />}
                  <span className="kind">{item.kind}</span>
                </span>
              </div>
            </li>
          );
        })}
        {rows.length === 0 && <li className="empty">No results</li>}
      </ul>
      )}
      <footer className="footer" hidden={collapsed}>
        {props.toast ? <ToastView toast={props.toast} onAction={props.onToastAction} /> : <span />}
        {/* The actions float over the end of the list, as one pill. */}
        <span className="footer-actions">
          {groups[0]?.[0] && (
            <button className="footer-button" onClick={() => groups[0][0].run()}>
              {groups[0][0].title} <kbd>↵</kbd>
            </button>
          )}
          <button className="footer-button" onClick={() => setPanelOpen(true)}>
            Actions <ShortcutHint shortcut={{ modifiers: ["ctrl"], key: "k" }} />
          </button>
        </span>
      </footer>
      {panelOpen && (
        <ActionOverlay
          title={current?.type === "item" ? current.item.title : current?.type === "fallback" ? current.fallback.title : "Calculator"}
          sections={panelSections}
          onRun={runNode}
          onClose={() => {
            setPanelOpen(false);
            refocus();
          }}
        />
      )}
      {prompt && <Prompt {...prompt} />}
      {hotkeyItem && (
        <HotkeyPrompt
          item={hotkeyItem}
          onDone={() => {
            setHotkeyItem(null);
            refocus();
          }}
        />
      )}
      {props.dialog}
    </div>
  );
}
