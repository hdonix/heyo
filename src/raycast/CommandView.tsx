// A running extension command: search bar, body, footer and action panel.

import { composing } from "../ime";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { ArrowLeft } from "lucide-react";

import { actionForShortcut, ActionOverlay, flatten, picksDate, sections, ShortcutHint } from "./actions";
import { DetailView } from "./Detail";
import { FormView, type FormValues } from "./FormView";
import { ExtensionDir, Image } from "./Image";
import { ListView } from "./ListView";
import { Memories, type ViewMemory } from "./memory";
import { content, encode, isFn, sendToRuntime, slot, type Node, type Toast, type View } from "./model";
import { useTyping } from "./typing";

export interface CommandState {
  session: number;
  mode: string;
  title: string;
  extension: { name: string; title: string; dir: string; icon?: string };
  view: View | null;
  /// Text carried over from the root search (a fallback command).
  initialQuery?: string;
}

export interface Keys {
  /// The body's own key handling (list navigation); true when handled.
  current?: (e: React.KeyboardEvent) => boolean;
  runPrimary?: (panel?: Node) => void;
  field?: (node: number, op: string) => void;
  /// The selected item's `quickLook`, for `Action.ToggleQuickLook`.
  quickLook?: unknown;
}

export interface BodyProps {
  node: Node;
  query: string;
  dispatch(node: Node, prop: string, ...args: unknown[]): void;
  keys: Keys;
  onActions(panel: Node | undefined): void;
}

function SearchDropdown(props: { node: Node; dispatch: BodyProps["dispatch"]; memory: ViewMemory }) {
  const { node, memory } = props;
  const options = useMemo(
    () =>
      content(node).flatMap((child): { section?: Node; item?: Node }[] =>
        child.type === "Dropdown.Section"
          ? [{ section: child }]
          : child.type === "Dropdown.Item"
            ? [{ item: child }]
            : [],
      ),
    [node],
  );
  const first = content(node).flatMap((c) => (c.type === "Dropdown.Section" ? content(c) : [c]))[0];
  // Coming back to the view, what was selected in it; the extension still has that.
  const remembered = memory.dropdown?.node === node.id ? memory.dropdown.value : undefined;
  const [local, setLocal] = useState<string | undefined>(remembered ?? node.props.defaultValue ?? first?.props.value);
  const value = node.props.value ?? local;
  memory.dropdown = { node: node.id, value: local };

  return (
    <select
      className="search-dropdown"
      title={node.props.tooltip}
      value={value ?? ""}
      onChange={(e) => {
        setLocal(e.target.value);
        if (isFn(node.props.onChange)) props.dispatch(node, "onChange", e.target.value);
      }}
    >
      {options.map((o) =>
        o.section ? (
          <optgroup key={o.section.id} label={o.section.props.title ?? ""}>
            {content(o.section).map((item) => (
              <option key={item.id} value={item.props.value}>
                {item.props.title}
              </option>
            ))}
          </optgroup>
        ) : (
          <option key={o.item!.id} value={o.item!.props.value}>
            {o.item!.props.title}
          </option>
        ),
      )}
    </select>
  );
}

export function CommandView(props: {
  command: CommandState;
  toast: Toast | null;
  onToastAction(toast: Toast): void;
  searchReset: number;
  fieldOp: { node: number; op: string; seq: number } | null;
}) {
  const { command } = props;
  const view = command.view;
  const root = view?.tree.find((n) => n.type !== "#text");
  const searchable = root?.type === "List" || root?.type === "Grid";

  const [query, setQuery] = useState("");
  const [actions, setActions] = useState<Node | undefined>();
  const [panelOpen, setPanelOpen] = useState(false);
  /// The `Action.PickDate` the panel is open for, when it was run directly.
  const [picker, setPicker] = useState<Node | undefined>();
  const memories = useRef(new Memories()).current;
  const memory = view ? memories.of(view.key) : undefined;
  const keys = useRef<Keys>({}).current;
  const formValues = useRef<FormValues>({ current: {} }).current;
  const inputRef = useRef<HTMLInputElement>(null);
  const panelRef = useRef<HTMLDivElement>(null);
  const throttle = useRef<ReturnType<typeof setTimeout>>(undefined);

  const dispatch = useCallback(
    (node: Node, prop: string, ...args: unknown[]) =>
      sendToRuntime({ type: "dispatch", session: command.session, node: node.id, prop, args: args.map(encode) }),
    [command.session],
  );

  // A pushed view starts with an empty search bar; one come back to has the
  // text it was left with. Decided while rendering, so that no view is ever
  // drawn with another's text.
  const [shownKey, setShownKey] = useState<number | undefined>(undefined);
  const arriving = view?.key !== shownKey;
  if (arriving) {
    setShownKey(view?.key);
    setQuery(memory?.query ?? "");
    setPanelOpen(false);
    setPicker(undefined);
  } else if (memory) memory.query = query;

  // What is typed in the search bar is the bar's own; the extension is told
  // and, when it controls `searchText`, heard when it says something new.
  const typing = useTyping({
    key: root?.id,
    text: arriving ? (memory?.query ?? "") : query,
    value: searchable ? root.props.searchText : undefined,
    ack: root?.props.ack,
    adopt: setQuery,
  });

  const onSearch = (text: string) => {
    setQuery(text);
    if (!root || !isFn(root.props.onSearchTextChange)) return;
    const seq = typing.next();
    clearTimeout(throttle.current);
    const tell = () => dispatch(root, "onSearchTextChange", text, seq);
    if (root.props.throttle) throttle.current = setTimeout(tell, 250);
    else tell();
  };

  // `clearSearchBar()` from the extension, which hears of it like any change.
  const lastReset = useRef(props.searchReset);
  useEffect(() => {
    if (lastReset.current === props.searchReset) return;
    lastReset.current = props.searchReset;
    onSearch("");
  }, [props.searchReset]);

  // A fallback command starts out searching for what was typed in the root search.
  const seeded = useRef(false);
  useEffect(() => {
    if (seeded.current || !searchable || !root || !command.initialQuery) return;
    seeded.current = true;
    onSearch(command.initialQuery);
  }, [searchable, root?.id]);

  useEffect(() => {
    if (searchable) inputRef.current?.focus();
    else if (root?.type !== "Form") panelRef.current?.focus();
    // Also when a view is replaced in place (a form giving way to its command).
  }, [view?.key, searchable, root?.type]);

  useEffect(() => {
    if (props.fieldOp) keys.field?.(props.fieldOp.node, props.fieldOp.op);
  }, [props.fieldOp]);

  // List, Grid and Form report their actions (they depend on the selection).
  const viewActions = root?.type === "Detail" ? slot(root, "actions") : root ? actions : undefined;
  const panelSections = useMemo(() => sections(viewActions), [viewActions]);
  const runnable = flatten(panelSections).filter((a) => a.type === "Action");

  /// Puts the keyboard back in the view. Whatever had the focus (the action
  /// panel's search field, a form that was replaced) may be gone, and with
  /// the focus on the page body no key reaches the panel.
  const refocus = () =>
    setTimeout(() => {
      if (panelRef.current?.contains(document.activeElement) && document.activeElement !== panelRef.current) return;
      (inputRef.current ?? panelRef.current)?.focus();
    });

  const closePanel = () => {
    setPanelOpen(false);
    setPicker(undefined);
  };

  const run = (action: Node) => {
    // The date is asked for first, in the panel.
    if (picksDate(action)) {
      setPicker(action);
      setPanelOpen(true);
      return;
    }
    closePanel();
    if (action.props.submit) dispatch(action, "onSubmit", formValues.current);
    // Quick Look is of the selected item, which only the list here knows.
    else if (action.props.preview) dispatch(action, "onAction", searchable ? keys.quickLook : undefined);
    else dispatch(action, "onAction");
    refocus();
  };
  const pickDate = (action: Node, date: Date | null) => {
    closePanel();
    if (isFn(action.props.onChange)) dispatch(action, "onChange", date);
    refocus();
  };
  keys.runPrimary = (panel) => {
    const action = flatten(sections(panel ?? viewActions)).find((a) => a.type === "Action");
    if (action) run(action);
  };

  const pop = () => sendToRuntime({ type: "pop", session: command.session });

  const onKeyDown = (e: React.KeyboardEvent) => {
    if (panelOpen || composing(e)) return;
    const inTextArea = (e.target as HTMLElement).tagName === "TEXTAREA";
    if (e.ctrlKey && e.code === "KeyK") {
      e.preventDefault();
      if (runnable.length || panelSections.length) setPanelOpen(true);
    } else if (e.key === "Escape") {
      e.preventDefault();
      if (searchable && query) onSearch("");
      else pop();
    } else if (e.key === "Enter" && !e.altKey && !e.shiftKey && !(inTextArea && !e.ctrlKey)) {
      e.preventDefault();
      // Ctrl+Enter is the secondary action, except in forms where it submits.
      const action = e.ctrlKey && root?.type !== "Form" ? runnable[1] : runnable[0];
      if (action) run(action);
    } else if (e.ctrlKey && e.code === "KeyT" && props.toast?.primaryAction) {
      e.preventDefault();
      props.onToastAction(props.toast);
    } else {
      const action = (e.ctrlKey || e.altKey || e.metaKey) && actionForShortcut(panelSections, e);
      if (action) {
        e.preventDefault();
        if (action.type === "Action") run(action);
      } else if (keys.current?.(e)) e.preventDefault();
    }
  };

  const bodyProps: BodyProps = { node: root!, query, dispatch, keys, onActions: setActions };
  let body: React.ReactNode;
  if (!root) {
    keys.current = undefined;
    body = (
      <div className="empty-view">
        <div className="empty-description">{command.mode === "no-view" ? `Running ${command.title}…` : ""}</div>
      </div>
    );
  } else if (searchable) body = <ListView key={view!.key} {...bodyProps} memory={memory!} />;
  else if (root.type === "Form") body = <FormView key={view!.key} {...bodyProps} formValues={formValues} memory={memory!} />;
  else if (root.type === "Detail") {
    keys.current = undefined;
    body = (
      <div className="body scroll">
        <DetailView node={root} dispatch={(n, p) => dispatch(n, p)} />
      </div>
    );
  } else body = <div className="empty-view" />;

  const accessory = searchable ? slot(root, "searchBarAccessory") : undefined;
  const title = root?.props.navigationTitle ?? command.title;
  const primary = runnable[0];

  return (
    <ExtensionDir.Provider value={command.extension.dir}>
      <div className="panel" onKeyDown={onKeyDown} ref={panelRef} tabIndex={-1}>
        <div className="search-row">
          <button className="back" onClick={pop} title="Back (Esc)">
            <ArrowLeft size={16} />
          </button>
          {searchable ? (
            <input
              ref={inputRef}
              className="search"
              placeholder={root.props.searchBarPlaceholder ?? "Search…"}
              value={query}
              onChange={(e) => onSearch(e.target.value)}
              {...typing.composition}
              spellCheck={false}
            />
          ) : (
            <div className="search view-title">{title}</div>
          )}
          {accessory && <SearchDropdown key={accessory.id} node={accessory} dispatch={dispatch} memory={memory!} />}
        </div>
        <div className={`loading-bar${root?.props.isLoading ? " active" : ""}`} />
        {body}
        <footer className="footer">
          {props.toast ? (
            <ToastView toast={props.toast} onAction={props.onToastAction} />
          ) : (
            <span className="footer-title">
              {command.extension.icon && <Image image={command.extension.icon} size={14} />}
              {title}
            </span>
          )}
          <span className="footer-actions">
            {primary && (
              <button className="footer-button" onClick={() => run(primary)}>
                {primary.props.title} <kbd>↵</kbd>
              </button>
            )}
            {runnable.length > 0 && (
              <button className="footer-button" onClick={() => setPanelOpen(true)}>
                Actions <ShortcutHint shortcut={{ modifiers: ["ctrl"], key: "k" }} />
              </button>
            )}
          </span>
        </footer>
        {panelOpen && (
          <ActionOverlay
            title={viewActions?.props.title}
            sections={panelSections}
            picker={picker}
            onRun={run}
            onPickDate={pickDate}
            onClose={() => {
              closePanel();
              (searchable ? inputRef.current : panelRef.current)?.focus();
            }}
          />
        )}
      </div>
    </ExtensionDir.Provider>
  );
}

export function ToastView({ toast, onAction }: { toast: Toast; onAction(toast: Toast): void }) {
  return (
    <span className={`toast ${toast.style.toLowerCase()}`}>
      <span className="toast-icon" />
      <span className="toast-title">{toast.title}</span>
      {toast.message && <span className="toast-message">{toast.message}</span>}
      {toast.primaryAction && (
        <button className="footer-button" onClick={() => onAction(toast)}>
          {toast.primaryAction.title} <ShortcutHint shortcut={{ modifiers: ["ctrl"], key: "t" }} />
        </button>
      )}
    </span>
  );
}
