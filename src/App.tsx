import { useCallback, useEffect, useRef, useState } from "react";

import { CommandView, type CommandState } from "./raycast/CommandView";
import { sendToRuntime, TreeStore, type Toast } from "./raycast/model";
import { call, listen, reportFailuresIn, reported } from "./host";
import { RootSearch } from "./RootSearch";
import type { SettingsView } from "../shared/bindings/SettingsView";
import { expand } from "../shared/placeholders";
import type { UiRequests } from "../shared/wire";

type Confirm = UiRequests["ui.confirm"] & { id: number };

export default function App() {
  const [query, setQuery] = useState("");
  // Bumped to refresh the root search (index changed, window shown).
  const [version, setVersion] = useState(0);
  const [shown, setShown] = useState(0);
  const [pendingOpen, setPendingOpen] = useState<string | null>(null);
  const [command, setCommand] = useState<CommandState | null>(null);
  const [toast, setToast] = useState<Toast | null>(null);
  const [confirm, setConfirm] = useState<Confirm | null>(null);
  const [searchReset, setSearchReset] = useState(0);
  const [fieldOp, setFieldOp] = useState<{ node: number; op: string; seq: number } | null>(null);
  const settingsRef = useRef<SettingsView | null>(null);
  const [compact, setCompact] = useState(false);
  const inputRef = useRef<HTMLInputElement>(null);
  const commandRef = useRef(command);
  commandRef.current = command;
  const toastRef = useRef(toast);
  toastRef.current = toast;
  const treeStore = useRef(new TreeStore());
  // Set as soon as "launched" arrives; renders may follow before React does.
  const sessionRef = useRef<number | null>(null);

  const refresh = useCallback(() => setVersion((v) => v + 1), []);

  const showError = useCallback(
    (title: string, message?: string) => setToast({ id: -Date.now(), title, message, style: "FAILURE" }),
    [],
  );
  const showSuccess = useCallback((title: string) => setToast({ id: -Date.now(), title, style: "SUCCESS" }), []);

  const backToRoot = useCallback(() => {
    setCommand(null);
    setTimeout(() => inputRef.current?.focus());
  }, []);

  // A call to the host that failed is said in the palette's toast.
  useEffect(() => reportFailuresIn(({ title, message }) => (title ? showError(title, message) : showError(message))), [showError]);

  useEffect(() => {
    call("get_settings")
      .then((s) => {
        settingsRef.current = s;
        setCompact(s.compactMode);
      })
      .catch(reported("Could not read the settings"));
  }, []);

  useEffect(() => {
    const heard = [
      listen("shown", () => {
        // Unless configured otherwise, reopening starts from the root search.
        if (settingsRef.current?.keepCommand && commandRef.current) return;
        if (commandRef.current) sendToRuntime({ type: "close", session: commandRef.current.session });
        setCommand(null);
        setQuery("");
        setToast(null);
        // The command waiting on a dialog must still get its answer.
        setConfirm((open) => {
          if (open) sendToRuntime({ type: "reply", id: open.id, result: false });
          return null;
        });
        setShown((n) => n + 1);
        refresh();
        inputRef.current?.focus();
      }),
      listen("settingsChanged", (settings) => {
        settingsRef.current = settings;
        setCompact(settings.compactMode);
      }),
      listen("indexReady", refresh),
      listen("openItem", (id) => {
        if (commandRef.current) sendToRuntime({ type: "close", session: commandRef.current.session });
        setCommand(null);
        setPendingOpen(id);
      }),
      // A snippet's keyword was typed in another application: the host
      // has the text, and placeholders are filled in here as for a snippet
      // chosen in the palette. (What would ask for an argument gets its
      // default: there is nobody to ask.)
      listen("expandSnippet", (asked) => {
        expand(asked.text, { readClipboard: () => call("read_clipboard") })
          .then((text) => call("snippet_expanded", { request: asked.request, text }))
          .catch(reported("Could not expand the snippet"));
      }),
      listen("runtime", (m) => {
        const current = commandRef.current;
        switch (m.type) {
          case "launched":
            treeStore.current = new TreeStore();
            sessionRef.current = m.session;
            setToast(null);
            setCommand({ session: m.session, mode: m.mode, title: m.title, extension: m.extension, view: null, initialQuery: m.fallbackText });
            break;
          case "render":
            if (sessionRef.current !== m.session) break;
            {
              const view = m.view && treeStore.current.apply(m.view);
              setCommand((c) => (c && c.session === m.session ? { ...c, view } : c));
            }
            break;
          case "closed":
            if (current && (m.session === undefined || m.session === current.session)) {
              // A finished no-view command closes the window unless it left a message.
              if (current.mode === "no-view" && !toastRef.current) call("hide").catch(reported());
              backToRoot();
            }
            break;
          case "pop-to-root":
            if (current) sendToRuntime({ type: "close", session: current.session });
            backToRoot();
            break;
          case "clear-search-bar":
            setSearchReset((n) => n + 1);
            break;
          case "field":
            setFieldOp({ node: m.node, op: m.op, seq: Date.now() });
            break;
          case "toast":
            setToast(m.toast);
            break;
          case "toast-hide":
            setToast((t) => (t?.id === m.id ? null : t));
            break;
          case "request":
            setConfirm({ id: m.id, ...m.params });
            break;
          case "exited":
            if (current) {
              showError("The extension runtime stopped");
              backToRoot();
            }
            break;
        }
      }),
    ];
    return () => heard.forEach((stop) => stop());
  }, [refresh, backToRoot]);

  // Success toasts fade; failures stay until replaced.
  useEffect(() => {
    if (!toast || toast.style !== "SUCCESS") return;
    const timer = setTimeout(() => setToast((t) => (t === toast ? null : t)), 2500);
    return () => clearTimeout(timer);
  }, [toast]);

  const openSettings = () => call("open_settings").catch(reported("Could not open settings"));

  const onToastAction = (t: Toast) =>
    sendToRuntime({ type: "toast-action", id: t.id, action: "primary" });

  const answer = (confirmed: boolean | "always") => {
    if (!confirm) return;
    sendToRuntime({ type: "reply", id: confirm.id, result: confirmed });
    setConfirm(null);
  };

  // A question that gives something away (a permission) starts on No, so
  // that Enter meant for the search bar does not give it.
  const cautious = !!confirm?.cautious;
  const dismissButton = confirm && (
    <button data-first={cautious || undefined} onClick={() => answer(false)}>
      {confirm.dismissAction.title}
    </button>
  );
  const primaryButton = confirm && (
    <button data-first={!cautious || undefined} className={confirm.primaryAction.style === "DESTRUCTIVE" ? "destructive" : "primary"} onClick={() => answer(true)}>
      {confirm.primaryAction.title}
    </button>
  );
  // The question keeps the keyboard while it is up. The view behind it takes
  // the focus for itself as it renders (its search bar, its panel), and Tab
  // would walk out into it: either way a key meant for the question would
  // go to something the question covers.
  const alertRef = useRef<HTMLDivElement>(null);
  const asked = confirm?.id;
  useEffect(() => {
    if (asked === undefined) return;
    const hold = () => {
      const alert = alertRef.current;
      if (alert && !alert.contains(document.activeElement)) alert.querySelector<HTMLElement>("[data-first]")?.focus();
    };
    hold();
    document.addEventListener("focusin", hold);
    return () => document.removeEventListener("focusin", hold);
  }, [asked]);
  const dialog = confirm && (
    <div
      className="overlay-backdrop center"
      onKeyDown={(e) => {
        e.stopPropagation();
        if (e.key === "Escape") answer(false);
      }}
    >
      <div className="alert" ref={alertRef}>
        <div className="alert-title">{confirm.title}</div>
        {confirm.message && <div className="alert-message">{confirm.message}</div>}
        {confirm.alwaysAction ? (
          // Three choices do not fit side by side.
          <div className="alert-buttons stacked">
            {primaryButton}
            <button onClick={() => answer("always")}>{confirm.alwaysAction.title}</button>
            {dismissButton}
          </div>
        ) : (
          <div className="alert-buttons">
            {dismissButton}
            {primaryButton}
          </div>
        )}
      </div>
    </div>
  );

  if (command) {
    return (
      <>
        <CommandView
          // A new command starts with fresh search text, selection and form values.
          key={command.session}
          command={command}
          toast={toast}
          onToastAction={onToastAction}
          searchReset={searchReset}
          fieldOp={fieldOp}
        />
        {dialog}
      </>
    );
  }

  return (
    <RootSearch
      query={query}
      setQuery={setQuery}
      inputRef={inputRef}
      version={version}
      shown={shown}
      toast={toast}
      onToastAction={onToastAction}
      showError={showError}
      showSuccess={showSuccess}
      compact={compact}
      openSettings={openSettings}
      pendingOpen={pendingOpen}
      onPendingHandled={() => setPendingOpen(null)}
      dialog={dialog}
    />
  );
}
