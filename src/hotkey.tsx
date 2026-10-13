// Global hotkeys: recording a key combination as a GTK accelerator (the
// format GNOME custom shortcuts use) and displaying one.

import { useEffect, useRef, useState } from "react";
import { X } from "lucide-react";

import { t } from "../shared/i18n";

const MODIFIER_LABELS: Record<string, string> = {
  Control: "Ctrl",
  Primary: "Ctrl",
  Ctrl: "Ctrl",
  Shift: "Shift",
  Alt: "Alt",
  Super: "Super",
  Meta: "Super",
};

export function accelParts(accel: string) {
  const mods = [...accel.matchAll(/<(\w+)>/g)].map((m) => MODIFIER_LABELS[m[1]] ?? m[1]);
  const key = accel.replace(/<\w+>/g, "");
  const label = key === "space" ? "Space" : key.length === 1 ? key.toUpperCase() : key.replace(/_/g, " ");
  return [...mods, label];
}

const CODE_NAMES: Record<string, string> = {
  Space: "space",
  Enter: "Return",
  Tab: "Tab",
  Backspace: "BackSpace",
  Delete: "Delete",
  Insert: "Insert",
  Home: "Home",
  End: "End",
  PageUp: "Page_Up",
  PageDown: "Page_Down",
  ArrowUp: "Up",
  ArrowDown: "Down",
  ArrowLeft: "Left",
  ArrowRight: "Right",
  Backquote: "grave",
  Minus: "minus",
  Equal: "equal",
  BracketLeft: "bracketleft",
  BracketRight: "bracketright",
  Backslash: "backslash",
  Semicolon: "semicolon",
  Quote: "apostrophe",
  Comma: "comma",
  Period: "period",
  Slash: "slash",
};

/// A GTK accelerator for a key press, or null for a bare modifier or a key
/// that needs a modifier.
function toAccelerator(e: React.KeyboardEvent, superDown: boolean): string | null {
  let key: string | undefined;
  if (/^Key[A-Z]$/.test(e.code)) key = e.code.slice(3).toLowerCase();
  else if (/^Digit\d$/.test(e.code)) key = e.code.slice(5);
  else if (/^F\d+$/.test(e.code)) key = e.code;
  else key = CODE_NAMES[e.code];
  if (!key) return null;
  const mods =
    (e.ctrlKey ? "<Control>" : "") + (e.shiftKey ? "<Shift>" : "") + (e.altKey ? "<Alt>" : "") + (e.metaKey || superDown ? "<Super>" : "");
  if (!mods && !/^F\d+$/.test(key)) return null;
  return mods + key;
}

export function HotkeyRecorder(props: { value: string | null; onChange(accel: string): void; autoStart?: boolean }) {
  const [recording, setRecording] = useState(!!props.autoStart);
  const buttonRef = useRef<HTMLButtonElement>(null);
  // Focus after any action panel has handed focus back to the search bar.
  useEffect(() => {
    if (!props.autoStart) return;
    const timer = setTimeout(() => buttonRef.current?.focus());
    return () => clearTimeout(timer);
  }, []);

  // WebKitGTK sets no modifier flag for Super, but reports the key itself.
  const superDown = useRef(false);
  const isSuper = (e: React.KeyboardEvent) => /^(OS|Meta)(Left|Right)$/.test(e.code) || e.key === "Super";

  const onKeyUp = (e: React.KeyboardEvent) => {
    if (isSuper(e)) superDown.current = false;
  };

  const onKeyDown = (e: React.KeyboardEvent) => {
    if (isSuper(e)) superDown.current = true;
    if (!recording) return;
    e.preventDefault();
    e.stopPropagation();
    if (e.key === "Escape") return setRecording(false);
    const accel = toAccelerator(e, superDown.current);
    if (accel) {
      setRecording(false);
      props.onChange(accel);
    }
  };

  return (
    <span className="hotkey">
      <button
        ref={buttonRef}
        className={`hotkey-field${recording ? " recording" : ""}`}
        onClick={() => setRecording(true)}
        onKeyDown={onKeyDown}
        onKeyUp={onKeyUp}
        onBlur={() => {
          superDown.current = false;
          setRecording(false);
        }}
      >
        {recording ? (
          <span className="hotkey-placeholder">{t("Press a shortcut…")}</span>
        ) : props.value ? (
          accelParts(props.value).map((part, i) => <kbd key={i}>{part}</kbd>)
        ) : (
          <span className="hotkey-placeholder">{t("Record Hotkey")}</span>
        )}
      </button>
      {props.value && !recording && (
        <button className="icon-button" title={t("Remove Hotkey")} onClick={() => props.onChange("")}>
          <X size={13} />
        </button>
      )}
    </span>
  );
}


/// An accelerator as key caps, e.g. Ctrl Shift V.
export function HotkeyHint({ accelerator }: { accelerator: string }) {
  return (
    <span className="shortcut">
      {accelParts(accelerator).map((part, i) => (
        <kbd key={i}>{part}</kbd>
      ))}
    </span>
  );
}
