// Action panels: collecting actions, keyboard shortcuts, and the ⌃K overlay.
//
// The overlay shows actions in sections, whoever they are from: an
// extension's `ActionPanel` (read from its tree by `sections`) or the root
// search's own. It hands back the action that was chosen; what running it
// means is the caller's.

import { composing } from "../ime";
import { useEffect, useMemo, useRef, useState } from "react";
import { step, underPointer, useReveal } from "../selection";

import { dateChoices, fromDateInput, inRange, toDateInput } from "./dates";
import { Image } from "./Image";
import { content, matchesQuery, type Node } from "./model";

/// What the panel shows of an action, and how the action takes part in it.
export interface PanelAction {
  title: string;
  /// An `Image.ImageLike`.
  icon?: unknown;
  /// A `Keyboard.Shortcut`.
  shortcut?: unknown;
  destructive?: boolean;
  /// Of a submenu, what is in it: chosen, it opens in place.
  submenu?: ActionSection<this>[];
  /// Of `Action.PickDate`, the dates it takes: chosen, it asks for one.
  pickDate?: { type?: string; min?: unknown; max?: unknown };
}

export interface ActionSection<A extends PanelAction = PanelAction> {
  title?: string;
  actions: A[];
}

/// An action of an extension, and the node to tell the extension of.
export interface NodeAction extends PanelAction {
  node: Node;
}

const ofNode = (node: Node): NodeAction => ({
  node,
  title: node.props.title ?? "",
  icon: node.props.icon,
  shortcut: node.props.shortcut,
  destructive: node.props.style === "destructive",
  submenu: node.type === "ActionPanel.Submenu" ? sections(node) : undefined,
  pickDate: node.props.pickDate || undefined,
});

/// Groups an `ActionPanel`'s children into sections; bare actions form an
/// untitled leading section. Submenus are kept as entries.
export function sections(panel: Node | undefined): ActionSection<NodeAction>[] {
  if (!panel) return [];
  const out: ActionSection<NodeAction>[] = [];
  let loose: ActionSection<NodeAction> | undefined;
  for (const child of content(panel)) {
    if (child.type === "ActionPanel.Section") {
      loose = undefined;
      out.push({ title: child.props.title, actions: content(child).map(ofNode) });
    } else if (child.type === "Action" || child.type === "ActionPanel.Submenu") {
      if (!loose) out.push((loose = { actions: [] }));
      loose.actions.push(ofNode(child));
    }
  }
  return out.filter((s) => s.actions.length);
}

export const flatten = <A extends PanelAction>(secs: ActionSection<A>[]) => secs.flatMap((s) => s.actions);

type Shortcut = { modifiers: string[]; key: string };

function platformShortcut(shortcut: any): Shortcut | undefined {
  if (!shortcut || typeof shortcut !== "object") return undefined;
  if ("modifiers" in shortcut) return shortcut;
  return shortcut.Windows ?? shortcut.windows ?? shortcut.macOS;
}

// On Linux, macOS's ⌘ maps to Ctrl like on Windows.
const MODIFIERS: Record<string, string> = {
  cmd: "ctrl",
  ctrl: "ctrl",
  opt: "alt",
  alt: "alt",
  shift: "shift",
  windows: "meta",
};

const KEYS: Record<string, string> = {
  return: "Enter",
  enter: "Enter",
  delete: "Backspace",
  backspace: "Backspace",
  deleteForward: "Delete",
  tab: "Tab",
  arrowUp: "ArrowUp",
  arrowDown: "ArrowDown",
  arrowLeft: "ArrowLeft",
  arrowRight: "ArrowRight",
  pageUp: "PageUp",
  pageDown: "PageDown",
  home: "Home",
  end: "End",
  space: " ",
  escape: "Escape",
};

const KEY_LABELS: Record<string, string> = {
  Enter: "↵",
  Backspace: "⌫",
  Delete: "⌦",
  ArrowUp: "↑",
  ArrowDown: "↓",
  ArrowLeft: "←",
  ArrowRight: "→",
  " ": "Space",
};

export function matchesShortcut(e: React.KeyboardEvent | KeyboardEvent, raw: unknown) {
  const shortcut = platformShortcut(raw);
  if (!shortcut) return false;
  const mods = new Set(shortcut.modifiers.map((m) => MODIFIERS[m]));
  if (e.ctrlKey !== mods.has("ctrl") || e.altKey !== mods.has("alt")) return false;
  if (e.shiftKey !== mods.has("shift") || e.metaKey !== mods.has("meta")) return false;
  const key = shortcut.key;
  if (KEYS[key]) return e.key === KEYS[key];
  if (/^[a-z]$/.test(key)) return e.code === `Key${key.toUpperCase()}`;
  if (/^[0-9]$/.test(key)) return e.code === `Digit${key}`;
  return e.key === key;
}

export function ShortcutHint({ shortcut }: { shortcut: unknown }) {
  const s = platformShortcut(shortcut);
  if (!s) return null;
  const mods = [...new Set(s.modifiers.map((m) => MODIFIERS[m]))];
  const order = ["ctrl", "alt", "shift", "meta"];
  const labels = { ctrl: "Ctrl", alt: "Alt", shift: "Shift", meta: "Super" } as Record<string, string>;
  const key = KEYS[s.key] ?? s.key;
  return (
    <span className="shortcut">
      {order
        .filter((m) => mods.includes(m))
        .map((m) => (
          <kbd key={m}>{labels[m]}</kbd>
        ))}
      <kbd>{KEY_LABELS[key] ?? key.toUpperCase()}</kbd>
    </span>
  );
}

/// Finds the action whose shortcut matches a key press, searching submenus.
export function actionForShortcut<A extends PanelAction>(secs: ActionSection<A>[], e: React.KeyboardEvent) {
  const search = (actions: A[]): A | undefined => {
    for (const action of actions) {
      if (action.submenu) {
        if (matchesShortcut(e, action.shortcut)) return undefined;
        const found = search(flatten(action.submenu));
        if (found) return found;
      } else if (matchesShortcut(e, action.shortcut)) return action;
    }
    return undefined;
  };
  return search(flatten(secs));
}

/// What `Action.PickDate` shows in the panel: a date field, and the dates
/// asked for most as buttons. Enter in the field takes its date.
function DatePanel(props: { dates: NonNullable<PanelAction["pickDate"]>; onPick(date: Date | null): void }) {
  const { type, min, max } = props.dates;
  const withTime = type !== "date";
  const [text, setText] = useState("");
  const inputRef = useRef<HTMLInputElement>(null);
  useEffect(() => inputRef.current?.focus(), []);
  const choices = useMemo(() => dateChoices(new Date(), withTime, min, max), [withTime, min, max]);
  const date = fromDateInput(text);
  const day: Intl.DateTimeFormatOptions = { weekday: "short", month: "short", day: "numeric" };
  const format = withTime ? { ...day, hour: "numeric", minute: "2-digit" } as const : day;

  return (
    <div className="date-panel">
      <div className="action-list">
        {choices.map((choice) => (
          <button key={choice.title} className="action-row date-choice" onClick={() => props.onPick(choice.date)}>
            <span className="action-title">{choice.title}</span>
            {choice.date && <span className="date-choice-value">{choice.date.toLocaleString(undefined, format)}</span>}
          </button>
        ))}
      </div>
      <input
        ref={inputRef}
        className="action-search date-input"
        type={withTime ? "datetime-local" : "date"}
        min={toDateInput(min, withTime) || undefined}
        max={toDateInput(max, withTime) || undefined}
        value={text}
        onChange={(e) => setText(e.target.value)}
        onKeyDown={(e) => {
          if (e.key !== "Enter" || composing(e)) return;
          e.preventDefault();
          if (date && inRange(date, min, max)) props.onPick(date);
        }}
      />
    </div>
  );
}

interface Level<A extends PanelAction> {
  title?: string;
  sections: ActionSection<A>[];
  /// The action whose date is being asked for, instead of a list of actions.
  picker?: A;
}

/// The ⌃K overlay. Submenus open in place; Esc goes back up. With `picker`
/// it opens asking for that action's date (the action was run by its
/// shortcut, or as the primary action).
export function ActionOverlay<A extends PanelAction>(props: {
  title?: string;
  sections: ActionSection<A>[];
  picker?: A;
  onRun(action: A): void;
  onPickDate?(action: A, date: Date | null): void;
  onClose(): void;
}) {
  const [stack, setStack] = useState<Level<A>[]>([
    props.picker ? { title: props.picker.title, sections: [], picker: props.picker } : { title: props.title, sections: props.sections },
  ]);
  const [query, setQuery] = useState("");
  const [selected, setSelected] = useState(0);
  const inputRef = useRef<HTMLInputElement>(null);
  const top = stack[stack.length - 1];

  // Each action with its place among all of them, which is what its row is
  // told apart by while the query hides others.
  const visible = useMemo(() => {
    let place = 0;
    return top.sections
      .map((s) => ({ title: s.title, rows: s.actions.map((action) => ({ action, place: place++ })).filter((row) => matchesQuery(query, row.action.title)) }))
      .filter((s) => s.rows.length);
  }, [top, query]);
  const flat = visible.flatMap((s) => s.rows.map((row) => row.action));

  useEffect(() => inputRef.current?.focus(), [stack]);
  useEffect(() => setSelected(0), [query, stack]);
  const listRef = useReveal<HTMLDivElement>([selected]);

  const activate = (action: A | undefined) => {
    if (!action) return;
    if (action.submenu) {
      setStack([...stack, { title: action.title, sections: action.submenu }]);
      setQuery("");
    } else if (action.pickDate) {
      setStack([...stack, { title: action.title, sections: [], picker: action }]);
      setQuery("");
    } else props.onRun(action);
  };

  const onKeyDown = (e: React.KeyboardEvent) => {
    e.stopPropagation();
    if (composing(e)) return;
    // Asking for a date: the field and the buttons have the keys (Tab goes
    // between them), all but Esc.
    if (top.picker) {
      if (e.key === "Tab") {
        // Around the panel, not on to the buttons behind it.
        const stops = [...e.currentTarget.querySelectorAll<HTMLElement>(".date-panel button, .date-panel input")];
        e.preventDefault();
        stops[step(stops.indexOf(document.activeElement as HTMLElement), e.shiftKey ? -1 : 1, stops.length, true)]?.focus();
      } else if (e.key === "Escape") {
        e.preventDefault();
        if (stack.length > 1) setStack(stack.slice(0, -1));
        else props.onClose();
      }
      return;
    }
    // Tab would move the focus to the buttons behind the panel.
    if (e.key === "Tab") return e.preventDefault();
    const move = (d: number) => {
      e.preventDefault();
      setSelected((s) => step(s, d, flat.length, true));
    };
    if (e.key === "ArrowDown") move(1);
    else if (e.key === "ArrowUp") move(-1);
    else if (e.key === "Enter") {
      e.preventDefault();
      activate(flat[selected]);
    } else if (e.key === "Escape" || (e.ctrlKey && e.code === "KeyK")) {
      e.preventDefault();
      if (stack.length > 1 && e.key === "Escape") setStack(stack.slice(0, -1));
      else props.onClose();
    } else {
      const action = actionForShortcut(top.sections, e);
      if (action) {
        e.preventDefault();
        activate(action);
      }
    }
  };

  let index = -1;
  return (
    <div className="overlay-backdrop" onMouseDown={props.onClose}>
      <div className="action-panel" onMouseDown={(e) => e.stopPropagation()} onKeyDown={onKeyDown}>
        {top.title && <div className="action-panel-title">{top.title}</div>}
        {top.picker?.pickDate && <DatePanel dates={top.picker.pickDate} onPick={(date) => props.onPickDate?.(top.picker!, date)} />}
        <div className="action-list" ref={listRef} hidden={!!top.picker}>
          {visible.map((section, si) => (
            <div key={si} className="action-section">
              {section.title && <div className="section-title">{section.title}</div>}
              {section.rows.map(({ action, place }) => {
                const i = ++index;
                return (
                  <div
                    key={place}
                    className={`action-row${i === selected ? " selected" : ""}${action.destructive ? " destructive" : ""}`}
                    {...underPointer(() => setSelected(i))}
                    onClick={() => activate(action)}
                  >
                    <Image image={action.icon} size={16} />
                    <span className="action-title">{action.title}</span>
                    {action.submenu ? <span className="chevron">›</span> : null}
                    <ShortcutHint shortcut={action.shortcut} />
                  </div>
                );
              })}
            </div>
          ))}
          {flat.length === 0 && <div className="empty">No actions</div>}
        </div>
        {!top.picker && (
          <input
            ref={inputRef}
            className="action-search"
            placeholder="Search for actions…"
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            spellCheck={false}
          />
        )}
      </div>
    </div>
  );
}
