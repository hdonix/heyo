// `Form`: fields keep their values here; `Action.SubmitForm` receives them all.

import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { X } from "lucide-react";

import { fromDateInput, toDateInput } from "./dates";
import { Image } from "./Image";
import type { ViewMemory } from "./memory";
import { content, isFn, slot, type Node } from "./model";
import { useTyping } from "./typing";
import type { BodyProps } from "./CommandView";
import { t } from "../../shared/i18n";

const FIELD_TYPES = new Set([
  "Form.TextField",
  "Form.PasswordField",
  "Form.TextArea",
  "Form.Checkbox",
  "Form.DatePicker",
  "Form.Dropdown",
  "Form.TagPicker",
  "Form.FilePicker",
]);

/// The fields that are typed into. What they hold is theirs (typing.ts).
const TEXT_TYPES = new Set(["Form.TextField", "Form.PasswordField", "Form.TextArea"]);

interface Option {
  value: string;
  title: string;
  icon?: unknown;
  section?: string;
}

function options(node: Node): Option[] {
  return content(node).flatMap((child): Option[] => {
    if (child.type.endsWith(".Section")) {
      return options(child).map((o) => ({ ...o, section: child.props.title }));
    }
    return child.props.value !== undefined ? [{ value: child.props.value, title: child.props.title, icon: child.props.icon }] : [];
  });
}

function emptyValue(field: Node): unknown {
  switch (field.type) {
    case "Form.Checkbox":
      return false;
    case "Form.TagPicker":
    case "Form.FilePicker":
      return [];
    case "Form.DatePicker":
      return null;
    case "Form.Dropdown":
      return options(field)[0]?.value ?? "";
    default:
      return "";
  }
}

const initialValue = (field: Node) => field.props.value ?? field.props.defaultValue ?? emptyValue(field);

const fileName = (path: string) => path.replace(/[\\/]+$/, "").split(/[\\/]/).pop() || path;

/// What the button of a file picker says it chooses.
export function pickerLabel(props: { allowMultipleSelection?: unknown; canChooseFiles?: unknown; canChooseDirectories?: unknown }) {
  const many = !!props.allowMultipleSelection;
  const folders = !!props.canChooseDirectories;
  if (props.canChooseFiles !== false && folders) return many ? t("Choose Files or Folders…") : t("Choose File or Folder…");
  if (folders) return many ? t("Choose Folders…") : t("Choose Folder…");
  return many ? t("Choose Files…") : t("Choose File…");
}

/// The current form values, read by `Action.SubmitForm`.
export type FormValues = { current: Record<string, unknown> };

/// A text field. It shows `text`, which the form keeps; `onType` is the
/// user changing it (with the number the change is sent under, when the
/// extension listens), `adopt` the extension replacing it.
function TextInput(props: {
  field: Node;
  text: string;
  events: Record<string, unknown>;
  onType(text: string, seq: number | undefined): void;
  adopt(text: string): void;
}) {
  const { field } = props;
  const typing = useTyping({ key: field.id, text: props.text, value: field.props.value, ack: field.props.ack, adopt: props.adopt });
  const shared = {
    ...props.events,
    ...typing.composition,
    placeholder: field.props.placeholder,
    value: props.text,
    onChange: (e: React.ChangeEvent<HTMLInputElement | HTMLTextAreaElement>) =>
      props.onType(e.target.value, isFn(field.props.onChange) ? typing.next() : undefined),
  };
  if (field.type === "Form.TextArea") return <textarea {...shared} className="field-input textarea" rows={4} />;
  return <input {...shared} className="field-input" type={field.type === "Form.PasswordField" ? "password" : "text"} spellCheck={false} />;
}

export function FormView({ node, dispatch, keys, onActions, formValues, memory }: BodyProps & { formValues: FormValues; memory: ViewMemory }) {
  const fields = content(node).filter((c) => FIELD_TYPES.has(c.type));
  // A form come back to (a view was pushed over it) has what it had.
  const [values, setValues] = useState<Record<string, unknown>>(() => memory.values ?? {});
  memory.values = values;
  const containerRef = useRef<HTMLDivElement>(null);

  // Register new fields with their initial value; follow controlled values.
  const merged: Record<string, unknown> = {};
  for (const field of fields) {
    const id = field.props.id;
    const local = id in values ? values[id] : initialValue(field);
    merged[id] = TEXT_TYPES.has(field.type) || field.props.value === undefined ? local : field.props.value;
  }
  formValues.current = merged;

  useEffect(() => onActions(slot(node, "actions")), [slot(node, "actions")]);

  const element = (id: number) => containerRef.current?.querySelector<HTMLElement>(`[data-node="${id}"]`);

  // Focus the field that had the focus, the autoFocus field, or the first
  // one; and scroll back to where the form was.
  useLayoutEffect(() => {
    const returning = fields.find((f) => f.id === memory.focused);
    const target = returning ?? fields.find((f) => f.props.autoFocus) ?? fields[0];
    if (target) element(target.id)?.focus({ preventScroll: !!returning });
    if (containerRef.current && memory.scrollTop) containerRef.current.scrollTop = memory.scrollTop;
  }, []);

  const set = (field: Node, value: unknown) => setValues((v) => ({ ...v, [field.props.id]: value }));

  // `ref.focus()` / `ref.reset()` from the extension.
  keys.field = (id, op) => {
    const field = fields.find((f) => f.id === id);
    if (!field) return;
    if (op === "focus") element(id)?.focus();
    else set(field, field.props.defaultValue ?? emptyValue(field));
  };
  keys.current = () => false;

  const change = (field: Node, value: unknown, seq?: number) => {
    set(field, value);
    if (isFn(field.props.onChange)) dispatch(field, "onChange", value, ...(seq === undefined ? [] : [seq]));
  };

  // What was chosen in the file browser (a view pushed over this one, in
  // the runtime) arrives as `picked`, numbered; each is taken once.
  const picked = fields.map((f) => (f.type === "Form.FilePicker" && f.props.picked ? `${f.id}:${f.props.picked.seq}` : "")).join(",");
  useEffect(() => {
    for (const field of fields) {
      const choice = field.props.picked;
      if (field.type !== "Form.FilePicker" || !choice) continue;
      const token = `${field.id}:${choice.seq}`;
      if (memory.picked[field.props.id] === token) continue;
      memory.picked[field.props.id] = token;
      change(field, Array.isArray(choice.paths) ? choice.paths : []);
    }
  }, [picked]);

  const events = (field: Node) => ({
    "data-node": field.id,
    onFocus: () => {
      memory.focused = field.id;
      if (isFn(field.props.onFocus)) dispatch(field, "onFocus", { target: { id: field.props.id, value: merged[field.props.id] }, type: "focus" });
    },
    onBlur: () => isFn(field.props.onBlur) && dispatch(field, "onBlur", { target: { id: field.props.id, value: merged[field.props.id] }, type: "blur" }),
  });

  const control = (field: Node) => {
    const p = field.props;
    const value = merged[p.id];
    switch (field.type) {
      case "Form.TextField":
      case "Form.PasswordField":
      case "Form.TextArea":
        return (
          <TextInput
            field={field}
            text={String(value ?? "")}
            events={events(field)}
            onType={(text, seq) => change(field, text, seq)}
            adopt={(text) => set(field, text)}
          />
        );
      case "Form.Checkbox":
        return (
          <label className="checkbox">
            <input {...events(field)} type="checkbox" checked={!!value} onChange={(e) => change(field, e.target.checked)} />
            {p.label}
          </label>
        );
      case "Form.DatePicker": {
        const withTime = p.type !== "date";
        return (
          <input
            {...events(field)}
            className="field-input"
            type={withTime ? "datetime-local" : "date"}
            min={toDateInput(p.min, withTime) || undefined}
            max={toDateInput(p.max, withTime) || undefined}
            value={toDateInput(value, withTime)}
            onChange={(e) => change(field, fromDateInput(e.target.value))}
          />
        );
      }
      case "Form.Dropdown": {
        const opts = options(field);
        const sections = [...new Set(opts.map((o) => o.section))];
        return (
          <select {...events(field)} className="field-input" value={String(value ?? "")} onChange={(e) => change(field, e.target.value)}>
            {sections.map((section) => {
              const items = opts.filter((o) => o.section === section).map((o) => (
                <option key={o.value} value={o.value}>
                  {o.title}
                </option>
              ));
              return section ? (
                <optgroup key={section} label={section}>
                  {items}
                </optgroup>
              ) : (
                items
              );
            })}
          </select>
        );
      }
      case "Form.TagPicker": {
        const selected = new Set(Array.isArray(value) ? (value as string[]) : []);
        return (
          <div className="tag-picker" {...events(field)} tabIndex={0}>
            {options(field).map((o) => (
              <span
                key={o.value}
                className={`tag clickable${selected.has(o.value) ? " active" : ""}`}
                onClick={() => {
                  const next = new Set(selected);
                  if (!next.delete(o.value)) next.add(o.value);
                  change(field, [...next]);
                }}
              >
                {o.icon != null && <Image image={o.icon} size={12} />}
                {o.title}
              </span>
            ))}
          </div>
        );
      }
      case "Form.FilePicker": {
        const paths = Array.isArray(value) ? (value as unknown[]).map(String) : [];
        // Enter on a button presses it; in the rest of a form it submits.
        const press = (e: React.KeyboardEvent) => {
          if (e.key === "Enter" && !e.ctrlKey) e.stopPropagation();
        };
        return (
          <div className="file-picker">
            {paths.map((path) => (
              <span key={path} className="tag file" title={path}>
                <span className="file-name">{fileName(path)}</span>
                <button className="file-remove" title={t("Remove")} tabIndex={-1} onClick={() => change(field, paths.filter((other) => other !== path))}>
                  <X size={12} />
                </button>
              </span>
            ))}
            {/* The choosing is done in a view the runtime pushes over this one. */}
            <button {...events(field)} className="file-choose" onKeyDown={press} onClick={() => isFn(p.browse) && dispatch(field, "browse", paths)}>
              {pickerLabel(p)}
            </button>
          </div>
        );
      }
      default:
        return null;
    }
  };

  return (
    <div className="body form" ref={containerRef} onScroll={(e) => (memory.scrollTop = e.currentTarget.scrollTop)}>
      {content(node).map((child) => {
        if (child.type === "Form.Separator") return <hr key={child.id} className="form-separator" />;
        if (child.type === "Form.Description") {
          return (
            <div key={child.id} className="form-row">
              <div className="form-label">{child.props.title}</div>
              <div className="form-description">{child.props.text}</div>
            </div>
          );
        }
        if (!FIELD_TYPES.has(child.type)) return null;
        return (
          <div key={child.id} className="form-row">
            <div className="form-label">{child.props.title}</div>
            <div className="form-control">
              {control(child)}
              {child.props.error && <div className="form-error">{child.props.error}</div>}
              {child.props.info && <div className="form-info">{child.props.info}</div>}
            </div>
          </div>
        );
      })}
    </div>
  );
}
