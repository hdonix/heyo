// The fields the selected item's arguments get in the search bar. The root
// search owns the values and the keys that move between fields (Tab) and
// launch (Enter); a field only edits its own value.

import { useState } from "react";

import type { Argument } from "./arguments";
import { composing } from "./ime";
import { step } from "./selection";

interface FieldProps {
  argument: Argument;
  value: string;
  /// Required, empty, and the user tried to launch.
  missing: boolean;
  onChange(value: string): void;
  register(element: HTMLElement | null): void;
}

const label = (argument: Argument) => argument.placeholder || argument.name;

function TextField({ argument, value, missing, onChange, register }: FieldProps) {
  return (
    <input
      ref={register}
      className={missing ? "argument missing" : "argument"}
      type={argument.type === "password" ? "password" : "text"}
      placeholder={label(argument)}
      aria-label={label(argument)}
      aria-required={argument.required}
      aria-invalid={missing}
      value={value}
      onChange={(e) => onChange(e.target.value)}
      // Wide enough for its hint, growing a little with what is typed.
      size={Math.min(24, Math.max(label(argument).length, value.length, 6))}
      spellCheck={false}
      autoComplete="off"
    />
  );
}

/// A dropdown argument: its choices open under it while it has the focus,
/// and the arrow keys or a first letter pick one. Drawn here, because a
/// native menu is a window of its own, and the palette hides when another
/// window takes the focus.
function ChoiceField({ argument, value, missing, onChange, register }: FieldProps) {
  const [open, setOpen] = useState(false);
  const choices = argument.data ?? [];
  const index = choices.findIndex((c) => c.value === value);

  const onKeyDown = (e: React.KeyboardEvent) => {
    if (composing(e) || e.ctrlKey || e.altKey || e.metaKey) return;
    let next = -1;
    if (e.key === "ArrowDown") next = index === -1 ? 0 : step(index, 1, choices.length, true);
    else if (e.key === "ArrowUp") next = index === -1 ? choices.length - 1 : step(index, -1, choices.length, true);
    else if (e.key.length === 1 && e.key !== " ") {
      // The next choice starting with the letter, after the current one.
      const from = index + 1;
      const order = [...choices.slice(from), ...choices.slice(0, from)];
      const found = order.find((c) => c.title.toLowerCase().startsWith(e.key.toLowerCase()));
      next = found ? choices.indexOf(found) : -1;
    } else if ((e.key === "Backspace" || e.key === "Delete") && index !== -1) {
      e.preventDefault();
      e.stopPropagation();
      return onChange("");
    } else return;
    // These keys are the field's: the list under the search bar stays put.
    e.preventDefault();
    e.stopPropagation();
    if (choices[next]) onChange(choices[next].value);
  };

  return (
    <span className="argument-choice">
      <button
        ref={register}
        type="button"
        className={`argument choice${missing ? " missing" : ""}${index === -1 ? " unset" : ""}`}
        aria-haspopup="listbox"
        aria-expanded={open}
        aria-label={label(argument)}
        onFocus={() => setOpen(true)}
        onBlur={() => setOpen(false)}
        onKeyDown={onKeyDown}
      >
        {index === -1 ? label(argument) : choices[index].title}
      </button>
      {open && choices.length > 0 && (
        <ul className="argument-menu" role="listbox">
          {choices.map((choice) => (
            <li
              key={choice.value}
              role="option"
              aria-selected={choice.value === value}
              className={choice.value === value ? "selected" : undefined}
              // Before the button loses the focus, and without taking it.
              onMouseDown={(e) => {
                e.preventDefault();
                onChange(choice.value);
              }}
            >
              {choice.title}
            </li>
          ))}
        </ul>
      )}
    </span>
  );
}

export function ArgumentFields(props: {
  fields: Argument[];
  values: Record<string, string>;
  /// Names of the required fields found empty at the last try to launch.
  missing: string[];
  onChange(name: string, value: string): void;
  /// Hands each field's element over, by its place among the fields.
  register(index: number, element: HTMLElement | null): void;
}) {
  return (
    <>
      {props.fields.map((argument, i) => {
        const Field = argument.type === "dropdown" ? ChoiceField : TextField;
        return (
          <Field
            key={argument.name}
            argument={argument}
            value={props.values[argument.name] ?? ""}
            missing={props.missing.includes(argument.name)}
            onChange={(value) => props.onChange(argument.name, value)}
            register={(element) => props.register(i, element)}
          />
        );
      })}
    </>
  );
}
