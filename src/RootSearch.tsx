// The root search: applications, commands, quicklinks, snippets and the
// calculator, with favorites, aliases and a Ctrl+K action panel; and at the
// top, what is in progress (the live activities, docs/live-activities.md 5.0).
//
// This is the list as it is drawn and the keys and clicks it takes. What a
// row is and does, it asks `rows.ts`.

import { composing } from "./ime";
import { useEffect, useMemo, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { ArrowRight, Star } from "lucide-react";

import type { ActivityView } from "../shared/bindings/ActivityView";
import type { Item } from "../shared/bindings/Item";
import { event } from "../shared/wire";
import { calculate, loadRates, type Calculation, type Rates } from "./calculator";
import { HotkeyHint, HotkeyRecorder } from "./hotkey";
import { ItemIcon } from "./ItemIcon";
import { ArgumentFields } from "./ArgumentFields";
import { afterAlias, missing, tabStop } from "./arguments";
import { FIRST, hold, locate } from "./reselect";
import { askAi, describe, open, rowsFor, type Context, type Palette, type Row, type RowAction } from "./rows";
import { step, underPointer, useReveal } from "./selection";
import { SelectionIndicator } from "./SelectionIndicator";
import { actionForShortcut, ActionOverlay, ShortcutHint } from "./raycast/actions";
import { ToastView } from "./raycast/CommandView";
import type { Toast } from "./raycast/model";

const SEARCH_PLACEHOLDER = "Search apps, commands, or calculate…";

export type { Item };

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
  // What was typed into the argument fields, and of which row.
  const [typed, setTyped] = useState<Context["typed"]>(null);
  // The row that did not open for a required argument left empty.
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

  // What is in progress: read when the window shows, and whenever the host
  // says it changed. The host renders the timers for the moment it is
  // asked, so while one is running and in view it is asked once a second.
  const [activities, setActivities] = useState<ActivityView[]>([]);
  const fetchActivities = () =>
    invoke<ActivityView[]>("activities")
      .then((all) => Array.isArray(all) && setActivities(all))
      .catch(() => {});
  useEffect(() => {
    fetchActivities();
    const heard = listen<ActivityView[]>(event.activities, ({ payload }) => setActivities(payload));
    return () => void heard.then((stop) => stop());
  }, [props.shown]);

  const [quicklinks, setQuicklinks] = useState<Item[]>([]);
  useEffect(() => {
    invoke<Item[]>("list_quicklinks").then(setQuicklinks);
  }, [props.version]);

  const calc = useMemo(() => calculate(query, rates), [query, rates]);
  const rows = useMemo(() => rowsFor(query, { activities, calc, items, quicklinks }), [query, activities, calc, items, quicklinks]);
  const keys = useMemo(() => rows.map((row) => row.key), [rows]);
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

  const ticking = !collapsed && rows.some((row) => row.type === "activity" && row.activity.live);
  useEffect(() => {
    if (!ticking) return;
    const timer = setInterval(fetchActivities, 1000);
    return () => clearInterval(timer);
  }, [ticking]);

  /// Which of the search field (0) and the argument fields (1…) has the focus.
  const stopOf = (element: EventTarget | null) => (element ? fieldRefs.current.indexOf(element as HTMLElement) + 1 : 0);
  const focusStop = (stop: number) => (stop === 0 ? props.inputRef.current : fieldRefs.current[stop - 1])?.focus();

  const palette: Palette = {
    invoke,
    showError: props.showError,
    showSuccess: props.showSuccess,
    openSettings: props.openSettings,
    ask: (question) =>
      new Promise((resolve) => {
        const answer = (value: string | null) => {
          setPrompt(null);
          refocus();
          resolve(value);
        };
        setPrompt({ ...question, onSubmit: answer, onCancel: () => answer(null) });
      }),
    askHotkey: setHotkeyItem,
    fillIn: (key, field) => {
      // Clicked, the row need not be the selected one yet.
      const index = keys.indexOf(key);
      if (index !== -1) setHeld({ key, index });
      setRefused(key);
      // After the field is there, and after the action panel gave the focus back.
      setTimeout(() => setTimeout(() => focusStop(field + 1)));
    },
    argumentsSent: () => {
      setTyped(null);
      setRefused(null);
    },
  };
  const context: Context = { query, typed, folded: collapsed };
  const about = current && describe(current, context, palette);

  // The selected row's arguments are fields in the search bar.
  const fields = about?.fields ?? [];
  const values = about?.values ?? {};
  const lacking = current && refused === current.key ? missing(fields, values) : [];
  fieldRefs.current.length = fields.length;
  // A field that goes (another row was selected) takes the focus with it:
  // without one, no key would reach the palette.
  useEffect(() => {
    if (!document.activeElement || document.activeElement === document.body) props.inputRef.current?.focus();
  }, [current?.key, fields.length]);

  // `heyo --open <id>`: the host already counted the use.
  useEffect(() => {
    if (!props.pendingOpen) return;
    const id = props.pendingOpen;
    props.onPendingHandled();
    invoke<Item | null>("get_item", { id }).then((item) => item && open(item, palette));
  }, [props.pendingOpen]);

  const groups = about?.actions ?? [];
  /// Runs an action chosen in the panel, or by its shortcut.
  const runAction = (action: RowAction) => {
    setPanelOpen(false);
    action.run();
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
      about?.primary?.run();
    } else if (e.key === "Tab" && !e.ctrlKey && !e.altKey && !e.shiftKey && query.trim()) {
      e.preventDefault();
      askAi(query.trim(), palette);
    } else if (e.key === "Escape") {
      if (query) props.setQuery("");
      else invoke("hide");
    } else if (e.ctrlKey || e.altKey) {
      const action = actionForShortcut(groups, e);
      if (action) {
        e.preventDefault();
        runAction(action);
      } else if (e.ctrlKey && e.key === "n") move(1);
      else if (e.ctrlKey && e.key === "p") move(-1);
    }
  };

  /// How a row looks: what it shows is its own, what it does it is asked.
  const draw = (row: Row, i: number) => {
    const isSelected = i === selected;
    const heading = row.heading && <div className="section-title">{row.heading}</div>;
    const pointer = { ...underPointer(() => pointAt(i)), onClick: () => describe(row, context, palette).primary?.run() };
    switch (row.type) {
      case "activity": {
        const { activity } = row;
        const classes = ["item", isSelected && "selected", activity.stale && "stale", activity.ended && "ended"].filter(Boolean).join(" ");
        return (
          <li key={row.key} className="item-wrap">
            {heading}
            <div className={classes} {...pointer}>
              <ItemIcon icon={activity.icon ?? "clock-16"} title={activity.name} colorKey={`${activity.extensionTitle}/${activity.name}`} />
              <span className="title">{activity.title}</span>
              {activity.subtitle && <span className="subtitle">{activity.subtitle}</span>}
              <span className="accessories">
                {activity.needsAttention && <span className="tag attention">Needs you</span>}
                {activity.compact && activity.compact !== activity.title && <span className="activity-value">{activity.compact}</span>}
                <span className="kind">{activity.ended ? "Ended" : activity.name}</span>
              </span>
            </div>
          </li>
        );
      }
      case "calc":
        return (
          <li key={row.key} className="calc-row" {...pointer}>
            {heading}
            <CalculatorCard calc={row.calc} selected={isSelected} />
          </li>
        );
      case "fallback":
        return (
          <li key={row.key} className="item-wrap">
            {heading}
            <div className={isSelected ? "item selected" : "item"} {...pointer}>
              <ItemIcon icon={row.fallback.icon} title={row.fallback.title} />
              <span className="title">{row.fallback.title}</span>
              <span className="accessories">
                <span className="kind">Fallback</span>
              </span>
            </div>
          </li>
        );
      case "item": {
        const { item } = row;
        const argument = item.kind === "Quicklink" ? afterAlias(item.alias, query) : null;
        return (
          <li key={row.key} className="item-wrap">
            {heading}
            <div className={isSelected ? "item selected" : "item"} {...pointer}>
              <ItemIcon icon={item.icon} title={item.title} colorKey={item.subtitle ? `${item.subtitle}/${item.title}` : item.title} />
              <span className="title">{item.title}</span>
              {argument ? <span className="subtitle argument">{argument}</span> : item.subtitle && <span className="subtitle">{item.subtitle}</span>}
              <span className="accessories">
                {item.hotkey && <HotkeyHint accelerator={item.hotkey} />}
                {item.alias && <span className="tag alias">{item.alias}</span>}
                {item.favorite && item.section !== "Favorites" && <Star size={13} className="favorite-star" />}
                <span className="kind">{item.kind}</span>
              </span>
            </div>
          </li>
        );
      }
    }
  };

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
        {current && fields.length > 0 ? (
          <>
            <ArgumentFields
              fields={fields}
              values={values}
              missing={lacking}
              onChange={(name, value) => {
                const { key } = current;
                setTyped((t) => ({ key, values: { ...(t?.key === key ? t.values : {}), [name]: value } }));
              }}
              register={(index, element) => (fieldRefs.current[index] = element)}
            />
            <span className="search-spacer" />
          </>
        ) : (
          <button className="search-hint" onClick={() => query.trim() && askAi(query.trim(), palette)} tabIndex={-1}>
            Ask AI <kbd>⇥</kbd>
          </button>
        )}
      </div>
      {!collapsed && (
      <ul className="list" ref={listRef}>
        <SelectionIndicator as="li" resetKey={query} />
        {rows.map(draw)}
        {rows.length === 0 && <li className="empty">No results</li>}
      </ul>
      )}
      <footer className="footer" hidden={collapsed}>
        {props.toast ? <ToastView toast={props.toast} onAction={props.onToastAction} /> : <span />}
        {/* The actions float over the end of the list, as one pill. */}
        <span className="footer-actions">
          {about?.primary && (
            <button className="footer-button" onClick={() => about.primary?.run()}>
              {about.primary.title} <kbd>↵</kbd>
            </button>
          )}
          <button className="footer-button" onClick={() => setPanelOpen(true)}>
            Actions <ShortcutHint shortcut={{ modifiers: ["ctrl"], key: "k" }} />
          </button>
        </span>
      </footer>
      {panelOpen && (
        <ActionOverlay
          title={about?.title}
          sections={groups}
          onRun={runAction}
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
