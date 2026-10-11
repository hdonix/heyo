// The settings window, laid out like Raycast's: a sidebar with a search
// field, the core pages and one entry per extension; the content pane is
// built from grouped cards whose rows have a label on the left and a control
// on the right. Changes apply and save immediately.

import { composing } from "./ime";
import { useEffect, useMemo, useState } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import {
  AppWindow,
  Blocks,
  ChevronDown,
  ChevronRight,
  ChevronUp,
  CircleCheck,
  CircleX,
  FolderOpen,
  Keyboard,
  List,
  Search,
  X,
  TextCursor,
} from "lucide-react";

import appIcon from "../src-tauri/icons/128x128.png";
import type { Capabilities } from "../shared/bindings/Capabilities";
import type { ClipboardMeaningStatus } from "../shared/bindings/ClipboardMeaningStatus";
import type { ClipboardSource } from "../shared/bindings/ClipboardSource";
import type { CommandInfo } from "../shared/bindings/CommandInfo";
import type { DictationStatus } from "../shared/bindings/DictationStatus";
import type { ExtensionInfo } from "../shared/bindings/ExtensionInfo";
import type { HotkeyName } from "../shared/bindings/HotkeyName";
import type { Hotkeys } from "../shared/bindings/Hotkeys";
import type { OcrStatus } from "../shared/bindings/OcrStatus";
import type { Permission } from "../shared/bindings/Permission";
import type { PermissionRow } from "../shared/bindings/PermissionRow";
import type { Preference } from "../shared/bindings/Preference";
import type { SettingsView } from "../shared/bindings/SettingsView";
import { call, listen, reported, reportFailure, reportFailuresIn, type Args } from "./host";
import { HotkeyRecorder } from "./hotkey";
import { classifyModelAddress } from "./modelAddress";
import { Image } from "./raycast/Image";

/// The settings that change, as `change_settings` takes them.
type Changes = Args<"change_settings">["changes"];

const RETENTION = [
  { days: 1, title: "1 Day" },
  { days: 7, title: "7 Days" },
  { days: 30, title: "1 Month" },
  { days: 90, title: "3 Months" },
  { days: 180, title: "6 Months" },
  { days: 365, title: "1 Year" },
];

const BUILTIN = "@heyo";
/// Pseudo-extension grouping the applications.
const APPLICATIONS = "@applications";
const CLIPBOARD_COMMAND = "cmd:@heyo:clipboard-history";

// --- Building blocks -------------------------------------------------------

function Card(props: { title?: string; children: React.ReactNode }) {
  return (
    <section className="st-section">
      {props.title && <h2 className="st-heading">{props.title}</h2>}
      <div className="st-card">{props.children}</div>
    </section>
  );
}

/// One setting: label and description on the left, control on the right.
function Row(props: { label: React.ReactNode; description?: React.ReactNode; icon?: unknown; children?: React.ReactNode }) {
  return (
    <div className="st-row">
      {props.icon !== undefined && <Image image={props.icon} size={22} />}
      <div className="st-row-text">
        <div className="st-row-label">{props.label}</div>
        {props.description && <div className="st-row-description">{props.description}</div>}
      </div>
      <div className="st-row-control">{props.children}</div>
    </div>
  );
}

function Switch(props: { checked: boolean; onChange(value: boolean): void }) {
  return (
    <input type="checkbox" className="st-switch" checked={props.checked} onChange={(e) => props.onChange(e.target.checked)} />
  );
}

/// A destructive button that asks for a second click before acting.
function ConfirmButton(props: { label: string; confirm: string; onConfirm(): void }) {
  const [armed, setArmed] = useState(false);
  useEffect(() => {
    if (!armed) return;
    const timer = setTimeout(() => setArmed(false), 3000);
    return () => clearTimeout(timer);
  }, [armed]);
  return (
    <button
      className={`button destructive${armed ? " armed" : ""}`}
      onClick={() => {
        if (!armed) return setArmed(true);
        setArmed(false);
        props.onConfirm();
      }}
    >
      {armed ? props.confirm : props.label}
    </button>
  );
}

/// A text input that saves on blur or Enter.
function TextInput(props: {
  value: string;
  placeholder?: string;
  password?: boolean;
  /// Borderless, for table cells.
  plain?: boolean;
  onCommit(value: string): void;
}) {
  const [text, setText] = useState(props.value);
  useEffect(() => setText(props.value), [props.value]);
  const commit = () => text !== props.value && props.onCommit(text);
  return (
    <input
      className={`st-input${props.plain ? " plain" : ""}`}
      type={props.password ? "password" : "text"}
      placeholder={props.placeholder}
      value={text}
      spellCheck={false}
      onChange={(e) => setText(e.target.value)}
      onBlur={commit}
      onKeyDown={(e) => e.key === "Enter" && !composing(e) && commit()}
    />
  );
}

function PreferenceRow(props: { pref: Preference; value: unknown; onChange(value: unknown): void }) {
  const { pref } = props;
  const value = props.value ?? pref.default;
  const label = `${pref.title ?? pref.label ?? pref.name}${pref.required ? " *" : ""}`;
  let control: React.ReactNode;
  if (pref.type === "checkbox") {
    control = <Switch checked={value === true} onChange={props.onChange} />;
  } else if (pref.type === "dropdown") {
    control = (
      <select className="st-select" value={String(value ?? pref.data?.[0]?.value ?? "")} onChange={(e) => props.onChange(e.target.value)}>
        {(pref.data ?? []).map((d) => (
          <option key={d.value} value={d.value}>
            {d.title}
          </option>
        ))}
      </select>
    );
  } else {
    control = (
      <TextInput
        value={value == null ? "" : String(value)}
        password={pref.type === "password"}
        placeholder={pref.placeholder ?? (pref.type === "directory" || pref.type === "file" ? "/path" : "")}
        onCommit={props.onChange}
      />
    );
  }
  const description = pref.type === "checkbox" && pref.label && pref.title ? pref.label : pref.description;
  return (
    <Row label={label} description={description}>
      {control}
    </Row>
  );
}

// --- Pages -----------------------------------------------------------------

interface PageProps {
  settings: SettingsView;
  save(changes: Changes): void;
  hotkeys: Hotkeys | null;
  setHotkey(name: HotkeyName, accel: string): void;
  itemHotkeys: Record<string, string>;
  setItemHotkey(item: { id: string; title: string }, accel: string): void;
  extensions: ExtensionInfo[];
  reload(): void;
  /// Stores the Claude API key; an empty one removes it.
  setApiKey(key: string): void;
}

/// What the Wayland card says about the extension, and the button it offers.
const SHELL_EXTENSION: Record<Exclude<Capabilities["extension"], "none" | "active">, { description: string; action?: string }> = {
  missing: {
    description:
      "On Wayland, switching and arranging windows, pasting, clipboard history, screenshots and the hold-to-talk key go through a small GNOME Shell extension that only Heyo can use.",
    action: "Install",
  },
  outdated: {
    description:
      "This version of Heyo comes with a newer extension than the one installed. Update it, then log out and back in. An extension that is already running keeps working until then, without what the new one adds.",
    action: "Update",
  },
  pending: {
    description: "In place. Log out and back in to finish: GNOME loads a new or updated extension only when a session starts.",
  },
};

/// On GNOME's Wayland sessions most of what Heyo does with other
/// applications goes through its Shell extension; this installs it, and
/// updates it when Heyo carries a newer one.
function ShellExtension() {
  const [state, setState] = useState<Capabilities["extension"]>("none");
  useEffect(() => {
    call("get_capabilities")
      .then((c) => setState(c.extension))
      .catch(reported());
  }, []);
  if (state === "none" || state === "active") return null;
  const install = () =>
    call("install_shell_extension")
      .then((c) => setState(c.extension))
      .catch(reported());
  const { description, action } = SHELL_EXTENSION[state];
  return (
    <Card title="Wayland">
      <Row label="GNOME Shell Extension" description={description}>
        {action && (
          <button className="button" onClick={install}>
            {action}
          </button>
        )}
      </Row>
    </Card>
  );
}

function General({ settings, save, hotkeys, setHotkey }: PageProps) {
  return (
    <>
      <ShellExtension />
      <Card>
        <Row label="Open at Login">
          <Switch checked={settings.launchAtLogin} onChange={(launchAtLogin) => save({ launchAtLogin })} />
        </Row>
        <Row
          label="Heyo Hotkey"
          description={
            hotkeys && !hotkeys.supported ? (
              <>
                Bind a desktop keyboard shortcut to <code>heyo</code>.
              </>
            ) : undefined
          }
        >
          {hotkeys?.supported && <HotkeyRecorder value={hotkeys.toggle} onChange={(accel) => setHotkey("toggle", accel)} />}
        </Row>
      </Card>
      <Card title="Appearance">
        <Row label="Theme" description="Light, dark, or whichever the system is using">
          <select className="st-select" value={settings.appearance} onChange={(e) => save({ appearance: e.target.value as SettingsView["appearance"] })}>
            <option value="system">System</option>
            <option value="light">Light</option>
            <option value="dark">Dark</option>
          </select>
        </Row>
      </Card>
      <Card title="Window">
        <Row label="Compact Mode" description="Show only the search field until you type">
          <Switch checked={settings.compactMode} onChange={(compactMode) => save({ compactMode })} />
        </Row>
        <Row label="Hide When Focus Is Lost" description="Close the window when you click elsewhere">
          <Switch checked={settings.hideOnBlur} onChange={(hideOnBlur) => save({ hideOnBlur })} />
        </Row>
        <Row
          label="Keep the Running Command"
          description="Reopening Heyo returns to the command you were using instead of the root search"
        >
          <Switch checked={settings.keepCommand} onChange={(keepCommand) => save({ keepCommand })} />
        </Row>
      </Card>
      <Card title="Live Activities">
        <Row
          label="Show What Is in Progress"
          description="Timers, downloads and what extensions report as under way, at the top of the root search, with a notification when one ends or needs you"
        >
          <Switch checked={settings.liveActivities} onChange={(liveActivities) => save({ liveActivities })} />
        </Row>
      </Card>
    </>
  );
}

type Modification = "hotkey" | "alias" | "enabled" | "disabled";
type Category = "all" | "applications" | "extensions";

const MODIFICATIONS: { id: Modification; title: string; Icon: typeof Keyboard }[] = [
  { id: "hotkey", title: "Hotkey Set", Icon: Keyboard },
  { id: "alias", title: "Alias Set", Icon: TextCursor },
  { id: "enabled", title: "Enabled", Icon: CircleCheck },
  { id: "disabled", title: "Disabled", Icon: CircleX },
];

const CATEGORIES: { id: Category; title: string; Icon: typeof Keyboard }[] = [
  { id: "all", title: "All categories", Icon: List },
  { id: "applications", title: "Applications", Icon: AppWindow },
  { id: "extensions", title: "Extensions", Icon: Blocks },
];

/// The filter menu at the top right of the Shortcuts page.
function ShortcutFilter(props: {
  modifications: Set<Modification>;
  toggle(id: Modification): void;
  category: Category;
  setCategory(category: Category): void;
}) {
  const [open, setOpen] = useState(false);
  const current = CATEGORIES.find((c) => c.id === props.category)!;
  const active = props.modifications.size;
  return (
    <div className="st-filter-menu">
      <button className="st-filter-button" onClick={() => setOpen(!open)}>
        <current.Icon size={15} />
        {current.title}
        {active > 0 && <span className="st-filter-count">{active}</span>}
        {open ? <ChevronUp size={14} /> : <ChevronDown size={14} />}
      </button>
      {open && (
        <>
          <div className="st-popover-backdrop" onClick={() => setOpen(false)} />
          <div
            className="st-popover"
            onKeyDown={(e) => {
              if (e.key !== "Escape") return;
              e.preventDefault();
              e.stopPropagation();
              setOpen(false);
            }}
          >
            <div className="st-popover-heading">Modifications</div>
            {MODIFICATIONS.map(({ id, title, Icon }) => (
              <label key={id} className="st-popover-item">
                <input type="checkbox" className="st-check" checked={props.modifications.has(id)} onChange={() => props.toggle(id)} />
                <Icon size={15} />
                {title}
              </label>
            ))}
            <div className="st-popover-separator" />
            <div className="st-popover-heading">Categories</div>
            {CATEGORIES.map(({ id, title, Icon }) => (
              <label key={id} className="st-popover-item">
                <input type="checkbox" className="st-check" checked={props.category === id} onChange={() => props.setCategory(id)} />
                <Icon size={15} />
                {title}
              </label>
            ))}
          </div>
        </>
      )}
    </div>
  );
}

/// The Shortcuts table: applications and extensions with their commands,
/// each with an alias, a hotkey and (for commands) whether it is enabled.
function Shortcuts(props: PageProps) {
  const { settings, save } = props;
  const [query, setQuery] = useState("");
  const [modifications, setModifications] = useState<Set<Modification>>(new Set());
  const [category, setCategory] = useState<Category>("all");
  // Groups and extensions the user expanded; everything starts collapsed
  // except the Extensions group.
  const [expanded, setExpanded] = useState<Set<string>>(new Set(["extensions"]));

  const disabled = useMemo(() => new Set(settings.disabledCommands), [settings.disabledCommands]);
  const q = query.toLowerCase();
  const filtering = !!q || modifications.size > 0;

  const toggleIn = <T,>(set: Set<T>, value: T) => {
    const next = new Set(set);
    if (!next.delete(value)) next.add(value);
    return next;
  };
  const isOpen = (id: string) => filtering || expanded.has(id);

  const setEnabled = (ids: string[], enabled: boolean) => {
    const next = new Set(disabled);
    ids.forEach((id) => (enabled ? next.delete(id) : next.add(id)));
    save({ disabledCommands: [...next] });
  };

  const setAlias = (id: string, alias: string) => {
    const aliases = { ...settings.aliases };
    if (alias.trim()) aliases[id] = alias.trim();
    else delete aliases[id];
    save({ aliases });
  };

  const visible = (c: CommandInfo, group: string, isApp: boolean) => {
    if (q && !c.title.toLowerCase().includes(q) && !group.toLowerCase().includes(q)) return false;
    if (modifications.has("hotkey") && !props.itemHotkeys[c.id]) return false;
    if (modifications.has("alias") && !settings.aliases[c.id]) return false;
    const wantsEnabled = modifications.has("enabled");
    const wantsDisabled = modifications.has("disabled");
    if (wantsEnabled !== wantsDisabled) {
      // Applications are always enabled.
      const enabled = isApp || !disabled.has(c.id);
      if (enabled !== wantsEnabled) return false;
    }
    return true;
  };

  const applications = props.extensions.find((e) => e.id === APPLICATIONS);
  const extensions = props.extensions.filter((e) => e.id !== APPLICATIONS);
  const apps = (applications?.commands ?? []).filter((c) => visible(c, "Applications", true));
  const groups = extensions
    .map((ext) => ({ ext, commands: ext.commands.filter((c) => visible(c, ext.title, false)) }))
    .filter((g) => g.commands.length);

  const commandRow = (c: CommandInfo, options: { indent?: boolean; app?: boolean }) => (
    <div key={c.id} className={`st-trow${options.indent ? " indent" : ""}`}>
      <span className="st-tname">
        <Image image={c.icon ?? null} size={18} />
        <span>{c.title}</span>
      </span>
      <TextInput plain value={settings.aliases[c.id] ?? ""} placeholder="Add Alias" onCommit={(alias) => setAlias(c.id, alias)} />
      <span className="st-thotkey">
        {props.hotkeys?.supported && c.id !== CLIPBOARD_COMMAND && (
          <HotkeyRecorder value={props.itemHotkeys[c.id] ?? null} onChange={(accel) => props.setItemHotkey(c, accel)} />
        )}
      </span>
      <span className="st-tcheck">
        {!options.app && (
          <input type="checkbox" className="st-check" checked={!disabled.has(c.id)} onChange={(e) => setEnabled([c.id], e.target.checked)} />
        )}
      </span>
    </div>
  );

  const groupHeader = (id: string, title: string, icon: string) => (
    <button className="st-group-header" onClick={() => setExpanded(toggleIn(expanded, id))}>
      <Image image={icon} size={18} />
      {title}
      {isOpen(id) ? <ChevronDown size={14} /> : <ChevronRight size={14} />}
    </button>
  );

  const columns = (
    <div className="st-thead">
      <span>Name</span>
      <span>Alias</span>
      <span>Hotkey</span>
      <span className="st-tcheck">Enabled</span>
    </div>
  );

  return (
    <>
      <div className="st-toolbar">
        <div className="st-filter grow">
          <Search size={14} />
          <input placeholder="Search…" value={query} onChange={(e) => setQuery(e.target.value)} spellCheck={false} />
        </div>
        <ShortcutFilter
          modifications={modifications}
          toggle={(id) => setModifications(toggleIn(modifications, id))}
          category={category}
          setCategory={setCategory}
        />
      </div>

      {category !== "extensions" && (apps.length > 0 || !filtering) && (
        <div className="st-card st-table">
          {groupHeader("applications", "Applications", "app-window-grid-3x3-16")}
          {isOpen("applications") && (
            <>
              {columns}
              {apps.map((c) => commandRow(c, { app: true }))}
            </>
          )}
        </div>
      )}

      {category !== "applications" && (groups.length > 0 || !filtering) && (
        <div className="st-card st-table">
          {groupHeader("extensions", "Extensions", "plug-16")}
          {isOpen("extensions") && (
            <>
              {columns}
              {groups.map(({ ext, commands }) => {
                // An extension with one command is shown as that command.
                if (ext.commands.length === 1) return commandRow(commands[0], {});
                const ids = ext.commands.map((c) => c.id);
                const enabledCount = ids.filter((id) => !disabled.has(id)).length;
                return (
                  <div key={ext.id}>
                    <div className="st-trow">
                      <button className="st-tname st-expander" onClick={() => setExpanded(toggleIn(expanded, ext.id))}>
                        <Image image={ext.icon} size={18} />
                        <span>{ext.title}</span>
                        {isOpen(ext.id) ? <ChevronDown size={13} /> : <ChevronRight size={13} />}
                      </button>
                      <span />
                      <span />
                      <span className="st-tcheck">
                        <input
                          type="checkbox"
                          className="st-check"
                          checked={enabledCount > 0}
                          ref={(el) => {
                            if (el) el.indeterminate = enabledCount > 0 && enabledCount < ids.length;
                          }}
                          onChange={(e) => setEnabled(ids, e.target.checked)}
                        />
                      </span>
                    </div>
                    {isOpen(ext.id) && commands.map((c) => commandRow(c, { indent: true }))}
                  </div>
                );
              })}
            </>
          )}
        </div>
      )}

      {filtering && apps.length === 0 && groups.length === 0 && <p className="st-note">Nothing matches.</p>}
    </>
  );
}

function useCapabilities() {
  const [capabilities, setCapabilities] = useState<Capabilities | null>(null);
  useEffect(() => {
    call("get_capabilities").then(setCapabilities).catch(reported());
  }, []);
  return capabilities;
}

const DICTATION_LANGUAGES = [
  { value: "auto", title: "Detect Automatically" },
  { value: "zh", title: "Chinese (Mandarin)" },
  { value: "yue", title: "Cantonese" },
  { value: "en", title: "English" },
  { value: "ja", title: "Japanese" },
  { value: "ko", title: "Korean" },
];

const POLISH_MODES = [
  { value: "off", title: "Off", description: "The text is left as it was recognized." },
  {
    value: "clean",
    title: "Clean Up",
    description: "When you finish, Claude removes filler words, applies your corrections and fixes punctuation, then the result is pasted.",
  },
  {
    value: "translate",
    title: "Translate to English",
    description: "When you finish, Claude cleans up what you said and pastes it in English.",
  },
];

/// The user's own words, one to a line. Saved when the field is left, so a
/// line half typed (or half composed in an input method) is never applied.
function Vocabulary({ lines, onCommit }: { lines: string[]; onCommit(lines: string[]): void }) {
  const stored = lines.join("\n");
  const [text, setText] = useState(stored);
  useEffect(() => setText(stored), [stored]);
  const commit = () => {
    const next = text
      .split("\n")
      .map((line) => line.trim())
      .filter(Boolean);
    if (next.join("\n") !== stored) onCommit(next);
  };
  return (
    <textarea
      className="field-input textarea"
      rows={6}
      spellCheck={false}
      placeholder={"TypeScript\nKubernetes\njason -> JSON\n杰森 -> JSON"}
      value={text}
      onChange={(e) => setText(e.target.value)}
      onBlur={commit}
    />
  );
}

function Dictation({ settings, save, hotkeys, setHotkey }: PageProps) {
  const capabilities = useCapabilities();
  const [status, setStatus] = useState<DictationStatus | null>(null);
  const [progress, setProgress] = useState<string | null>(null);

  const refresh = () => call("dictation_status").then(setStatus).catch(reported());
  useEffect(() => {
    refresh();
    return listen("dictationInstall", (e) => {
      if (e.type === "progress") {
        const percent = e.total ? ` ${Math.round(((e.received ?? 0) / e.total) * 100)}%` : "…";
        setProgress(`${e.step}${percent}`);
      } else {
        setProgress(null);
        if (e.type === "error") reportFailure(e.message);
        refresh();
      }
    });
  }, []);

  const install = () => {
    setProgress("Starting…");
    call("dictation_install").catch((e) => {
      setProgress(null);
      reportFailure(String(e));
    });
  };

  const installing = progress !== null || !!status?.installing;
  const mode = POLISH_MODES.find((m) => m.value === settings.dictationPolish) ?? POLISH_MODES[0];
  const microphones = status?.microphones ?? [];

  return (
    <>
      <p className="st-note">
        Hold the hotkey while you speak and let go, or tap it to start and tap again to finish: what you said is typed
        into the window you are working in. Speech is recognized on this computer.
      </p>
      <Card>
        <Row
          label="Speech Recognition"
          description={
            status?.installed
              ? "SenseVoice is installed. Audio never leaves this computer."
              : "A one-time download of about 270 MB (SenseVoice speech model)."
          }
        >
          {status?.installed ? (
            <span className="st-badge">Installed</span>
          ) : installing ? (
            <span className="st-row-description">{progress ?? "Downloading…"}</span>
          ) : (
            <button className="button" onClick={install}>
              Download
            </button>
          )}
        </Row>
        <Row
          label="Hotkey"
          description={
            hotkeys && !hotkeys.supported ? (
              <>
                Bind a desktop keyboard shortcut to <code>heyo --dictate</code>.
              </>
            ) : (
              "Tap to start and tap again to finish, or hold it while you speak"
            )
          }
        >
          {hotkeys?.supported && <HotkeyRecorder value={hotkeys.dictation} onChange={(accel) => setHotkey("dictation", accel)} />}
        </Row>
        <Row
          label="Hold-to-Talk Key"
          description={
            capabilities?.holdKey === "none"
              ? "A single key cannot be watched in this desktop session; use the hotkey instead."
              : settings.dictationHoldKey
                ? "Hold it and speak; let go to finish. A quick tap, or using it in a shortcut, does nothing."
                : "A single key to hold while you speak. It keeps working as usual in shortcuts."
          }
        >
          {capabilities && capabilities.holdKey !== "none" && (
            <select className="st-select" value={settings.dictationHoldKey} onChange={(e) => save({ dictationHoldKey: e.target.value })}>
              <option value="">None</option>
              {capabilities.holdKeys.map((k) => (
                <option key={k.value} value={k.value}>
                  {k.title}
                </option>
              ))}
            </select>
          )}
        </Row>
        {capabilities && (
          <Row
            label="Type as You Speak"
            description={
              capabilities.dictationWords[0] === "composed"
                ? `Your words go into the window as you speak, through the input method: the sentence you are speaking is shown there as text being composed, and becomes the window's text when it is finished. Your own input method (Pinyin, say) is set aside while you dictate and comes back when you finish. ${
                    capabilities.dictationWords.includes("rewritten")
                      ? "A window that takes no input method gets the words by pasting, corrected where they stand until you press a key or click."
                      : "A window that takes no input method gets each sentence pasted when it is finished."
                  } When this is off, nothing is shown before a sentence is finished.`
                : capabilities.dictationWords[0] === "rewritten"
                  ? "Your words go into the window as they are recognized, and are corrected there when a later word changes an earlier one. Heyo only ever erases what it wrote itself: once you press a key or click while dictating, it stops correcting and adds each finished sentence where the cursor is. In terminals, password managers and the applications excluded from snippet expansion, and when this is off, each sentence is pasted when it is finished."
                  : capabilities.session === "wayland"
                    ? "Not available on Wayland, where Heyo cannot tell whether you typed in between, and so must not erase anything. Each sentence is pasted when it is finished."
                    : "Not available in this desktop session. Each sentence is pasted when it is finished."
            }
          >
            {capabilities.dictationWords[0] !== "pasted" && <Switch checked={settings.dictationLive} onChange={(dictationLive) => save({ dictationLive })} />}
          </Row>
        )}
      </Card>

      <Card title="Input">
        <Row
          label="Microphone"
          description={
            status && microphones.length === 0 ? (
              <span className="error">No microphone found. Connect one to use dictation.</span>
            ) : undefined
          }
        >
          <select className="st-select" value={settings.dictationSource} onChange={(e) => save({ dictationSource: e.target.value })}>
            <option value="">System Default</option>
            {microphones.map((m) => (
              <option key={m.name} value={m.name}>
                {m.description}
              </option>
            ))}
          </select>
        </Row>
        <Row label="Language" description="Automatic detection handles mixed Chinese and English">
          <select className="st-select" value={settings.dictationLanguage} onChange={(e) => save({ dictationLanguage: e.target.value })}>
            {DICTATION_LANGUAGES.map((l) => (
              <option key={l.value} value={l.value}>
                {l.title}
              </option>
            ))}
          </select>
        </Row>
      </Card>

      <Card title="Vocabulary">
        <Row
          label="Your Words"
          description={
            <>
              Names, jargon and product names, one to a line, written the way you want them: “type script” then comes
              out as TypeScript. For a word that is heard as something else, write <code>heard as -&gt; written as</code>.
              The recognizer cannot be taught new words, so these are put right in its text; a Chinese name heard as
              other characters of the same sound is only fixed if you list those characters as “heard as”. Polishing
              with AI is given the list too.
            </>
          }
        />
        <div className="st-row">
          <Vocabulary lines={settings.dictationVocabulary} onCommit={(dictationVocabulary) => save({ dictationVocabulary })} />
        </div>
      </Card>

      <Card title="Polish with AI">
        <Row label="Mode" description={mode.description}>
          <select className="st-select" value={mode.value} onChange={(e) => save({ dictationPolish: e.target.value })}>
            {POLISH_MODES.map((m) => (
              <option key={m.value} value={m.value}>
                {m.title}
              </option>
            ))}
          </select>
        </Row>
        {mode.value !== "off" && settings.aiProvider !== "local" && !settings.anthropicApiKeySet && (
          <Row label="Claude API Key" description="Polishing needs a Claude API key or a local model. Set either in Advanced." />
        )}
      </Card>
    </>
  );
}

/// Whether the local model server answers, and with which models.
function LocalModelStatus({ url }: { url: string }) {
  const [status, setStatus] = useState("Checking…");
  useEffect(() => {
    let current = true;
    setStatus("Checking…");
    call("local_models", { url })
      .then((models) => current && setStatus(models.length ? `Connected: ${models.join(", ")}` : "Connected, but the server lists no models."))
      .catch((e) => current && setStatus(`Not reachable: ${e}. Any OpenAI-compatible server works (vLLM, LM Studio, Ollama).`));
    return () => {
      current = false;
    };
  }, [url]);
  return (
    <>
      {status} <ModelAddressNote url={url} />
    </>
  );
}

/// Says where text sent to the model goes: it is only "local" when the
/// address is this computer's.
function ModelAddressNote({ url }: { url: string }) {
  const address = classifyModelAddress(url);
  if (address.where === "this-computer") return <>Text stays on this computer.</>;
  if (address.where === "unknown") return <span className="error">This is not an http or https address.</span>;
  return (
    <span className="error">
      {address.encrypted
        ? `This address is not this computer: the text you select, ask about and dictate is sent to ${address.host}.`
        : `This address is not this computer, and the connection is not encrypted: the text you select, ask about and dictate is sent to ${address.host} over plain http, readable by anyone on the network in between. Use https, or a server on this computer.`}
    </span>
  );
}

function Screenshots(props: PageProps) {
  const { hotkeys, itemHotkeys, setItemHotkey } = props;
  const [status, setStatus] = useState<OcrStatus | null>(null);
  const [installing, setInstalling] = useState(false);

  const refresh = () => call("ocr_status").then(setStatus).catch(reported());
  useEffect(() => {
    refresh();
    return listen("ocrInstall", (e) => {
      if (e.type === "progress") return setInstalling(true);
      setInstalling(false);
      if (e.type === "error") reportFailure(e.message);
      refresh();
    });
  }, []);

  const hotkeyRow = (id: string, title: string, description: string) => (
    <Row label={title} description={description}>
      {hotkeys?.supported && <HotkeyRecorder value={itemHotkeys[id] ?? null} onChange={(accel) => setItemHotkey({ id, title }, accel)} />}
    </Row>
  );

  return (
    <>
      <p className="st-note">
        Drag to select part of the screen, or click a window to select exactly that. Mark it up with rectangles,
        arrows or the pen, then copy it, save it to Pictures/Screenshots, pin it on top of other windows, or copy the
        text in it.
      </p>
      <Card title="Hotkeys">
        {hotkeyRow("heyo:screenshot", "Capture Screenshot", "Enter copies, Ctrl+S saves, Esc cancels")}
        {hotkeyRow("heyo:screenshot-text", "Capture Text from Screen", "Drag over text and it is copied")}
      </Card>
      <Card title="Text Recognition">
        <Row
          label="Recognition Engine"
          description={
            status?.installed
              ? "PaddleOCR is installed. Chinese and English; images never leave this computer."
              : "A one-time download of about 340 MB (PaddleOCR, Chinese and English)."
          }
        >
          {status?.installed ? (
            <span className="st-badge">Installed</span>
          ) : installing || status?.installing ? (
            <span className="st-row-description">Downloading…</span>
          ) : (
            <button
              className="button"
              onClick={() => {
                setInstalling(true);
                call("ocr_install").catch((e) => {
                  setInstalling(false);
                  reportFailure(String(e));
                });
              }}
            >
              Download
            </button>
          )}
        </Row>
      </Card>
    </>
  );
}

function Advanced({ settings, save, setApiKey }: PageProps) {
  const node = settings.nodeVersion;
  // Bumped after a key is stored, to empty the field it was typed into.
  const [keyField, setKeyField] = useState(0);
  return (
    <>
      <Card title="Extensions">
        <Row
          label="Node.js"
          description={
            <span className={"Err" in node ? "error" : undefined}>
              {"Ok" in node ? `Using ${node.Ok}.` : node.Err}
              {settings.nodeOverridden && " HEYO_NODE overrides this setting."} Extensions need Node.js 22 or newer.
            </span>
          }
        >
          <TextInput value={settings.nodePath} placeholder="node (from PATH)" onCommit={(nodePath) => save({ nodePath })} />
        </Row>
        <Row label="Extensions Folder" description={settings.extensionsDir}>
          <button className="button" onClick={() => call("open_extensions_folder").catch(reported())}>
            <FolderOpen size={14} /> Open
          </button>
        </Row>
      </Card>
      <Card title="Rendering">
        <Row
          label="Renderer"
          description="How the interface is drawn. Automatic keeps to the CPU on an NVIDIA driver without modesetting (the GPU cannot share frames there) and lets WebKit choose elsewhere; GPU with shared memory draws on the GPU and copies each frame, which is faster at 200% scale and keeps every CSS effect. Takes effect when Heyo is next started."
        >
          <select className="st-select" value={settings.renderer} onChange={(e) => save({ renderer: e.target.value as SettingsView["renderer"] })}>
            <option value="auto">Automatic</option>
            <option value="cpu">CPU</option>
            <option value="gpu-shared-memory">GPU, shared memory</option>
            <option value="gpu-dma-buf">GPU, DMA-BUF (WebKit's default)</option>
          </select>
        </Row>
      </Card>
      <Card title="AI">
        <Row label="Model" description="Answers Selected Text Actions and polishes dictation.">
          <select className="st-select" value={settings.aiProvider === "local" ? "local" : "claude"} onChange={(e) => save({ aiProvider: e.target.value })}>
            <option value="claude">Claude</option>
            <option value="local">Local Model</option>
          </select>
        </Row>
        {settings.aiProvider === "local" ? (
          <>
            <Row label="Server" description={<LocalModelStatus url={settings.localModelUrl} />}>
              <TextInput value={settings.localModelUrl} placeholder="http://127.0.0.1:8000/v1" onCommit={(localModelUrl) => save({ localModelUrl })} />
            </Row>
            <Row label="Model Name" description="Leave empty to use the first model the server offers.">
              <TextInput value={settings.localModelName} placeholder="automatic" onCommit={(localModelName) => save({ localModelName })} />
            </Row>
          </>
        ) : (
          <Row label="API Key" description="Leave empty to use ANTHROPIC_API_KEY or an `ant auth login` profile.">
            <TextInput
              password
              key={keyField}
              value=""
              placeholder={settings.anthropicApiKeySet ? "Saved — type a new key to replace it" : "sk-ant-…"}
              onCommit={(key) => {
                if (!key.trim()) return;
                setApiKey(key);
                setKeyField((n) => n + 1);
              }}
            />
            {settings.anthropicApiKeySet && (
              <button className="button" onClick={() => setApiKey("")}>
                Remove
              </button>
            )}
          </Row>
        )}
      </Card>
      <Card title="Pasting">
        <Row
          label="Ask Before Pasting Several Lines into a Terminal"
          description="A terminal runs each pasted line as a command. Dictation never asks: there, line breaks become spaces."
        >
          <Switch checked={settings.confirmTerminalPaste} onChange={(confirmTerminalPaste) => save({ confirmTerminalPaste })} />
        </Row>
      </Card>
      <Card title="Data">
        <Row label="Settings File" description={settings.settingsPath} />
      </Card>
    </>
  );
}

/// Looking for a newer Heyo, in copies of Heyo that can install one.
function Updates() {
  // undefined: not asked yet; null: this is the newest; else the newer version.
  const [newer, setNewer] = useState<string | null>();
  const [busy, setBusy] = useState(false);
  const run = (work: Promise<unknown>) => {
    setBusy(true);
    work.catch(reported()).finally(() => setBusy(false));
  };
  if (newer) {
    return (
      <button className="button" disabled={busy} onClick={() => run(call("install_update"))}>
        {busy ? "Installing…" : `Install Heyo ${newer} and Restart`}
      </button>
    );
  }
  return (
    <button className="button" disabled={busy} onClick={() => run(call("check_for_update").then(setNewer))}>
      {busy ? "Checking…" : newer === null ? "Heyo Is Up to Date" : "Check for Updates"}
    </button>
  );
}

function About({ settings }: PageProps) {
  const [copied, setCopied] = useState(false);
  const copyDiagnostics = () =>
    call("copy_diagnostics")
      .then(() => {
        setCopied(true);
        setTimeout(() => setCopied(false), 2500);
      })
      .catch(reported());
  return (
    <div className="st-about">
      <img src={appIcon} width={96} height={96} alt="" />
      <div className="st-about-name">Heyo</div>
      <div className="st-about-version">Version {settings.version}</div>
      <p>A command palette launcher for Linux that runs Raycast extensions.</p>
      {settings.canUpdate && <Updates />}
      <button className="button" onClick={copyDiagnostics}>
        {copied ? "Copied" : "Copy Diagnostics"}
      </button>
      <p className="st-about-hint">
        For a bug report: versions, the desktop session and the end of Heyo's log, with keys and tokens removed. Read it before you share it.
      </p>
    </div>
  );
}

/// The applications whose copies are never recorded: those listed, and a
/// menu of the installed ones and the ones copied from to add another.
function IgnoredApplications({ settings, save }: PageProps) {
  const [sources, setSources] = useState<ClipboardSource[]>([]);
  useEffect(() => {
    call("clipboard_sources").then(setSources).catch(reported());
  }, []);
  const ignored = settings.clipboardIgnoredApps;
  const offered = sources.filter((source) => !ignored.includes(source.id));
  const option = (source: ClipboardSource) => (
    <option key={source.id} value={source.id}>
      {source.name}
    </option>
  );
  return (
    <Card title="Ignored Applications">
      {ignored.map((id) => {
        const source = sources.find((s) => s.id === id);
        return (
          <Row key={id} icon={source?.icon ?? "app-window-16"} label={source?.name ?? id} description={source ? undefined : "Not installed"}>
            <button className="button" onClick={() => save({ clipboardIgnoredApps: ignored.filter((other) => other !== id) })}>
              Remove
            </button>
          </Row>
        );
      })}
      <Row
        label="Never Record Copies From"
        description="What you copy while a window of one of these applications is in front stays out of the history. Password managers are always left out."
      >
        <select
          className="st-select"
          value=""
          onChange={(e) => e.target.value && save({ clipboardIgnoredApps: [...ignored, e.target.value] })}
        >
          <option value="">Add Application…</option>
          {offered.some((s) => s.seen) && <optgroup label="Copied From">{offered.filter((s) => s.seen).map(option)}</optgroup>}
          <optgroup label="Installed">{offered.filter((s) => !s.seen).map(option)}</optgroup>
        </select>
      </Row>
    </Card>
  );
}

/// Finding clipboard entries by what they mean: the switch, the download it
/// needs, and how far reading the history has got.
function FindByMeaning({ settings, save }: PageProps) {
  const [status, setStatus] = useState<ClipboardMeaningStatus | null>(null);
  const [installing, setInstalling] = useState(false);
  const refresh = () => call("clipboard_meaning_status").then(setStatus).catch(reported());
  useEffect(() => {
    refresh();
    return listen("clipboardMeaningInstall", (e) => {
      if (e.type === "progress") return setInstalling(true);
      setInstalling(false);
      if (e.type === "error") reportFailure(e.message);
      refresh();
    });
  }, [settings.clipboardMeaning]);
  // While entries are being read, say how many are left.
  const reading = settings.clipboardMeaning && status?.installed && status.waiting > 0;
  useEffect(() => {
    if (!reading) return;
    const timer = setInterval(refresh, 2000);
    return () => clearInterval(timer);
  }, [reading]);

  const on = settings.clipboardMeaning;
  const progress = !status?.installed
    ? ""
    : status.problem
      ? ` It is not working at present: ${status.problem}`
      : status.waiting > 0
        ? ` Reading the history: ${status.waiting.toLocaleString()} ${status.waiting === 1 ? "entry" : "entries"} to go; until then some are found by their words only.`
        : ` ${status.indexed.toLocaleString()} ${status.indexed === 1 ? "entry" : "entries"} can be found this way.`;
  return (
    <Row
      label="Find by Meaning"
      description={
        <>
          “服务器登录命令” or “server login command” finds an ssh line: entries are found by what they are about as well as by
          their words, in Chinese and English alike. A language model on this computer reads what you copy; nothing is sent
          anywhere.{" "}
          {status?.installed
            ? on && progress
            : "A one-time download of about 135 MB, or 250 MB if text recognition is not installed. It takes about 350 MB of memory while you search, and none a few minutes after."}
        </>
      }
    >
      {status && !status.installed ? (
        installing || status.installing ? (
          <span className="st-row-description">Downloading…</span>
        ) : (
          <button
            className="button"
            onClick={() => {
              setInstalling(true);
              call("clipboard_meaning_install").catch((e) => {
                setInstalling(false);
                reportFailure(String(e));
              });
            }}
          >
            Download
          </button>
        )
      ) : (
        <Switch checked={on} onChange={(clipboardMeaning) => save({ clipboardMeaning })} />
      )}
    </Row>
  );
}

function ClipboardCard(props: PageProps) {
  const { settings, save, hotkeys, setHotkey } = props;
  return (
    <>
      <Card title="Clipboard History">
        {hotkeys?.supported && (
          <Row label="Hotkey" description="Opens Clipboard History from anywhere">
            <HotkeyRecorder value={hotkeys.clipboard} onChange={(accel) => setHotkey("clipboard", accel)} />
          </Row>
        )}
        <Row label="Record Clipboard History" description="Copies marked as secret by password managers are never recorded">
          <Switch checked={settings.clipboardHistory} onChange={(clipboardHistory) => save({ clipboardHistory })} />
        </Row>
        {settings.clipboardHistory && (
          <Row
            label="Pause Recording"
            description="Nothing you copy is recorded while this is on. “Pause or Resume Clipboard History” in the root search does the same, and can have a hotkey."
          >
            <Switch checked={settings.clipboardPaused} onChange={(clipboardPaused) => save({ clipboardPaused })} />
          </Row>
        )}
        <Row label="Keep History For" description="Pinned entries are kept forever">
          <select
            className="st-select"
            value={settings.clipboardRetentionDays}
            onChange={(e) => save({ clipboardRetentionDays: Number(e.target.value) })}
          >
            {RETENTION.map((r) => (
              <option key={r.days} value={r.days}>
                {r.title}
              </option>
            ))}
          </select>
        </Row>
        <Row label="Clear History" description="Pinned entries are not deleted">
          <ConfirmButton
            label="Clear…"
            confirm="Click Again to Clear"
            onConfirm={() => call("clear_clipboard_history").catch(reported())}
          />
        </Row>
        {settings.clipboardHistory && <FindByMeaning {...props} />}
      </Card>
      {settings.clipboardHistory && <IgnoredApplications {...props} />}
    </>
  );
}

const PERMISSION_STATES: Record<PermissionRow["state"], string> = {
  allowed: "Allowed by you",
  asks: "Asks you the first time it is used",
  free: "Needs no asking",
};

/// What an extension may ask Heyo for: what it lists in its manifest, and
/// what the user has allowed, which can be taken back here. Live
/// activities are not asked about; they have a switch instead.
function Permissions({ ext, settings, save }: { ext: ExtensionInfo; settings: SettingsView; save(changes: Changes): void }) {
  const [rows, setRows] = useState<PermissionRow[]>([]);
  const load = () => call("extension_permissions", { id: ext.id }).then(setRows).catch(reported());
  useEffect(() => {
    setRows([]);
    load();
  }, [ext.id]);

  const revoke = (permission: Permission) =>
    call("revoke_extension_permission", { extension: ext.name, permission })
      .then(load)
      .catch(reported());

  const activitiesOff = settings.activitiesOff.includes(ext.name);
  const setActivities = (on: boolean) => save({ activitiesOff: on ? settings.activitiesOff.filter((name) => name !== ext.name) : [...settings.activitiesOff, ext.name] });

  if (rows.length === 0) return null;
  return (
    <>
      <Card title="Permissions">
        {rows.map((row) => (
          <Row
            key={row.permission}
            label={row.allows.charAt(0).toUpperCase() + row.allows.slice(1)}
            description={row.permission === "activity" ? (settings.liveActivities ? "Declared; turn it off here" : "Live activities are off in General") : PERMISSION_STATES[row.state]}
          >
            {row.permission === "activity" ? (
              <Switch checked={settings.liveActivities && !activitiesOff} onChange={setActivities} />
            ) : (
              row.state === "allowed" && (
                <button className="button" onClick={() => revoke(row.permission)}>
                  Revoke
                </button>
              )
            )}
          </Row>
        ))}
      </Card>
      <p className="st-note">
        Permissions are about what Heyo hands over or does for an extension. They do not confine it: an extension is a program on this computer, and
        can read your files and start other programs by itself.
      </p>
    </>
  );
}

/// Snippets that expand as their keyword is typed: the switch, and the
/// applications it leaves alone.
function SnippetExpansionCard({ settings, save }: PageProps) {
  const capabilities = useCapabilities();
  const [sources, setSources] = useState<ClipboardSource[]>([]);
  const on = settings.snippetExpansion;
  useEffect(() => {
    if (!on) return;
    call("clipboard_sources").then(setSources).catch(reported());
  }, [on]);
  if (!capabilities) return null;
  const available = capabilities.snippetExpansion !== "none";
  const excluded = settings.snippetExpansionIgnoredApps;
  return (
    <Card title="Snippet Expansion">
      <Row
        label="Expand Snippets as You Type"
        description={
          available
            ? "Typing a snippet's keyword in any application replaces it with the snippet's text. While this is on, Heyo watches what you type to find the keywords; none of it is stored. “Pause or Resume Snippet Expansion” in the root search switches it too."
            : capabilities.session === "wayland"
              ? "Not available on Wayland, where an application cannot see what is typed in another. Snippets can still be pasted from the root search."
              : "Not available in this desktop session. Snippets can still be pasted from the root search."
        }
      >
        {available && <Switch checked={on} onChange={(snippetExpansion) => save({ snippetExpansion })} />}
      </Row>
      {available && on && (
        <>
          {excluded.map((id) => {
            const source = sources.find((s) => s.id === id);
            return (
              <Row key={id} icon={source?.icon ?? "app-window-16"} label={source?.name ?? id}>
                <button className="button" onClick={() => save({ snippetExpansionIgnoredApps: excluded.filter((other) => other !== id) })}>
                  Remove
                </button>
              </Row>
            );
          })}
          <Row
            label="Never Expand In"
            description="Password managers are always left out. A password field in any other application cannot be told from an ordinary one. Nothing expands while an input method (Chinese, Japanese, …) is in use."
          >
            <select
              className="st-select"
              value=""
              onChange={(e) => e.target.value && save({ snippetExpansionIgnoredApps: [...excluded, e.target.value] })}
            >
              <option value="">Add Application…</option>
              {sources
                .filter((source) => !excluded.includes(source.id))
                .map((source) => (
                  <option key={source.id} value={source.id}>
                    {source.name}
                  </option>
                ))}
            </select>
          </Row>
        </>
      )}
    </Card>
  );
}

/// The page of one extension (or of Heyo's own commands, or Applications).
function ExtensionPage(props: PageProps & { ext: ExtensionInfo; onUninstalled(): void }) {
  const { ext, settings, save } = props;
  const [query, setQuery] = useState("");
  const isApps = ext.id === APPLICATIONS;
  const disabled = new Set(settings.disabledCommands);

  const setEnabled = (id: string, enabled: boolean) => {
    const next = new Set(disabled);
    if (enabled) next.delete(id);
    else next.add(id);
    save({ disabledCommands: [...next] });
  };

  const savePreference = async (command: string | null, name: string, value: unknown) => {
    const values = { extension: { ...ext.values.extension }, commands: { ...ext.values.commands } };
    if (command) values.commands[command] = { ...values.commands[command], [name]: value };
    else values.extension[name] = value;
    try {
      await call("set_extension_preferences", { extension: ext.name, values });
      props.reload();
    } catch (e) {
      reportFailure(String(e));
    }
  };

  const uninstall = () =>
    call("uninstall_extension", { id: ext.id })
      .then(props.onUninstalled)
      .catch(reported());

  const q = query.toLowerCase();
  const commands = ext.commands.filter((c) => !q || c.title.toLowerCase().includes(q));

  return (
    <>
      <header className="st-ext-header">
        <Image image={ext.icon} size={44} />
        <div>
          <div className="st-ext-title">{ext.title}</div>
          <div className="st-row-description">
            {ext.development ? `In development from ${ext.development}` : ext.author ? `by ${ext.author}` : ext.builtin ? "Built into Heyo" : ""}
          </div>
        </div>
      </header>
      {ext.description && <p className="st-note">{ext.description}</p>}

      {ext.preferences.length > 0 && (
        <Card title="Preferences">
          {ext.preferences.map((pref) => (
            <PreferenceRow
              key={pref.name}
              pref={pref}
              value={ext.values.extension[pref.name]}
              onChange={(v) => savePreference(null, pref.name, v)}
            />
          ))}
        </Card>
      )}

      {isApps && (
        <div className="st-filter">
          <Search size={14} />
          <input placeholder="Search applications…" value={query} onChange={(e) => setQuery(e.target.value)} spellCheck={false} />
        </div>
      )}
      <Card title={isApps ? undefined : "Commands"}>
        {commands.map((c) => (
          <Row key={c.id} label={c.title} description={c.description} icon={c.icon ?? null}>
            {props.hotkeys?.supported && c.id !== CLIPBOARD_COMMAND && (
              <HotkeyRecorder value={props.itemHotkeys[c.id] ?? null} onChange={(accel) => props.setItemHotkey(c, accel)} />
            )}
            {!isApps && <Switch checked={!disabled.has(c.id)} onChange={(enabled) => setEnabled(c.id, enabled)} />}
          </Row>
        ))}
        {commands.length === 0 && <Row label="No matches" />}
      </Card>

      {ext.id === BUILTIN && <ClipboardCard {...props} />}
      {ext.id === BUILTIN && <SnippetExpansionCard {...props} />}

      {ext.commands
        .filter((c) => c.preferences.length > 0)
        .map((c) => (
          <Card key={c.id} title={`${c.title} Preferences`}>
            {c.preferences.map((pref) => (
              <PreferenceRow
                key={pref.name}
                pref={pref}
                value={ext.values.commands[c.name]?.[pref.name]}
                onChange={(v) => savePreference(c.name, pref.name, v)}
              />
            ))}
          </Card>
        ))}

      {!ext.builtin && !isApps && <Permissions ext={ext} settings={settings} save={save} />}

      {!ext.builtin && (
        <Card>
          <Row
            label={ext.development ? "Stop Developing" : "Uninstall Extension"}
            description={ext.development ? "Takes it out of Heyo; its sources are not touched" : "Its preferences are kept"}
          >
            <ConfirmButton
              label={ext.development ? "Remove" : "Uninstall"}
              confirm={ext.development ? "Click Again to Remove" : "Click Again to Uninstall"}
              onConfirm={uninstall}
            />
          </Row>
        </Card>
      )}
    </>
  );
}

// --- Window ----------------------------------------------------------------

const PAGES = [
  { id: "general", title: "General", icon: "cog-16", color: "var(--color-text-muted)", Body: General },
  { id: "shortcuts", title: "Shortcuts", icon: "keyboard-16", color: "var(--raycast-blue)", Body: Shortcuts },
  { id: "dictation", title: "Dictation", icon: "microphone-16", color: "var(--raycast-red)", Body: Dictation },
  { id: "screenshots", title: "Screenshots", icon: "camera-16", color: "var(--raycast-green)", Body: Screenshots },
  { id: "advanced", title: "Advanced", icon: "wrench-screwdriver-16", color: "var(--raycast-purple)", Body: Advanced },
  { id: "about", title: "About", icon: "info-01-16", color: "var(--raycast-orange)", Body: About },
] as const;

/// The settings window. Esc closes it.
export function SettingsWindow() {
  const [page, setPage] = useState<string>("general");
  const [search, setSearch] = useState("");
  const [settings, setSettings] = useState<SettingsView | null>(null);
  const [hotkeys, setHotkeys] = useState<Hotkeys | null>(null);
  const [extensions, setExtensions] = useState<ExtensionInfo[]>([]);
  const [itemHotkeys, setItemHotkeys] = useState<Record<string, string>>({});
  const [error, setError] = useState<string | null>(null);

  // A call to the host that failed is said in the line at the bottom.
  useEffect(() => reportFailuresIn(({ message }) => setError(message)), []);

  const reload = () =>
    Promise.all([call("list_extensions"), call("list_applications"), call("item_hotkeys")])
      .then(([list, apps, bound]) => {
        const applications: ExtensionInfo = {
          id: APPLICATIONS,
          name: APPLICATIONS,
          title: "Applications",
          description: "Give an application a hotkey to launch it from anywhere.",
          author: null,
          icon: "app-window-grid-3x3-16",
          builtin: true,
          commands: apps.map((a) => ({
            id: a.id,
            name: a.id,
            title: a.title,
            description: a.subtitle,
            mode: "application",
            icon: a.icon,
            preferences: [],
          })),
          preferences: [],
          values: { extension: {}, commands: {} },
        };
        setExtensions([...list, applications]);
        setItemHotkeys(bound);
      })
      .catch(reported());

  const read = () => call("get_settings").then(setSettings).catch(reported());

  useEffect(() => {
    read();
    call("get_hotkeys").then(setHotkeys).catch(reported());
    reload();
    // Favorites and aliases also change from the palette, and this window's
    // copy must follow those changes. The event goes to every window and
    // leaves out what only this one may know (where the files are): ask
    // for the whole.
    return listen("settingsChanged", () => void read());
  }, []);

  // Esc closes the window wherever focus is (even on the page body). The
  // hotkey recorder stops propagation of the Esc that cancels recording.
  useEffect(() => {
    const onKeyDown = (e: KeyboardEvent) => {
      if (e.key === "Escape" && !e.defaultPrevented && !composing(e)) {
        e.preventDefault();
        getCurrentWindow().close();
      }
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, []);

  /// Changes some settings: only what changes is sent, and the host answers
  /// with all of them as they now are.
  const save = async (changes: Changes) => {
    if (!settings) return;
    setSettings({ ...settings, ...changes });
    try {
      setSettings(await call("change_settings", { changes }));
      setError(null);
    } catch (e) {
      reportFailure(String(e));
      read();
    }
  };

  const setHotkey = (name: HotkeyName, accelerator: string) =>
    call("set_hotkey", { name, accelerator })
      .then((h) => {
        setHotkeys(h);
        setError(null);
      })
      .catch(reported());

  const setItemHotkey = async (item: { id: string; title: string }, accelerator: string) => {
    try {
      await call("set_item_hotkey", { id: item.id, title: item.title, accelerator });
      setItemHotkeys((h) => {
        const next = { ...h };
        if (accelerator) next[item.id] = accelerator;
        else delete next[item.id];
        return next;
      });
      setError(null);
    } catch (e) {
      reportFailure(String(e));
    }
  };

  // Applications first, then Heyo's own commands, then installed extensions.
  const sidebarExtensions = useMemo(() => {
    const rank = (e: ExtensionInfo) => (e.id === APPLICATIONS ? 0 : e.id === BUILTIN ? 1 : 2);
    return [...extensions].sort((a, b) => rank(a) - rank(b) || a.title.localeCompare(b.title));
  }, [extensions]);

  const q = search.toLowerCase();
  const matches = (title: string, more: string[] = []) => !q || [title, ...more].some((t) => t.toLowerCase().includes(q));
  const pages = PAGES.filter((p) => matches(p.title));
  const shownExtensions = sidebarExtensions.filter((e) =>
    matches(e.title, e.id === APPLICATIONS ? [] : e.commands.map((c) => c.title)),
  );

  const corePage = PAGES.find((p) => p.id === page);
  const ext = extensions.find((e) => e.id === page);
  const props: PageProps | null = settings && {
    settings,
    save,
    hotkeys,
    setHotkey,
    itemHotkeys,
    setItemHotkey,
    extensions,
    reload,
    setApiKey: (key) =>
      call("set_anthropic_api_key", { key })
        .then((saved) => {
          setSettings(saved);
          setError(null);
        })
        .catch(reported()),
  };

  return (
    <div className="st-window">
      <aside className="st-sidebar">
        {/* The window has no frame of the desktop's: it is moved by its top edge. */}
        <div className="st-drag" data-tauri-drag-region />
        <div className="st-search">
          <Search size={15} />
          <input placeholder="Search settings…" value={search} onChange={(e) => setSearch(e.target.value)} spellCheck={false} />
        </div>
        <nav className="st-nav">
          {pages.map((p) => (
            <button key={p.id} className={`st-nav-item${p.id === page ? " active" : ""}`} onClick={() => setPage(p.id)}>
              <span className="st-nav-icon" style={{ "--tile-color": p.color } as React.CSSProperties}>
                <Image image={p.icon} size={14} />
              </span>
              {p.title}
            </button>
          ))}
          {pages.length > 0 && shownExtensions.length > 0 && <div className="st-nav-gap" />}
          {shownExtensions.map((e) => (
            <button key={e.id} className={`st-nav-item${e.id === page ? " active" : ""}`} onClick={() => setPage(e.id)}>
              <span className="st-nav-icon plain">
                <Image image={e.icon} size={20} />
              </span>
              <span className="st-nav-title">{e.id === BUILTIN ? "Heyo Commands" : e.title}</span>
            </button>
          ))}
        </nav>
      </aside>
      <main className="st-content">
        <header className="st-titlebar" data-tauri-drag-region>
          <span className="st-titlebar-title" data-tauri-drag-region>
            {corePage?.title ?? (ext ? (ext.id === BUILTIN ? "Heyo Commands" : ext.title) : "Settings")}
          </span>
          <button className="st-close" title="Close (Esc)" onClick={() => getCurrentWindow().close()}>
            <X size={14} />
          </button>
        </header>
        <div className="st-page" key={page}>
          {props && corePage && <corePage.Body {...props} />}
          {props && ext && <ExtensionPage {...props} ext={ext} onUninstalled={() => (setPage("general"), reload())} />}
        </div>
        {error && <div className="st-error">{error}</div>}
      </main>
    </div>
  );
}
