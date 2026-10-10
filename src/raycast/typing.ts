//! Who owns the text in a field the extension controls (`searchText` with
//! `onSearchTextChange`, a form field's `value` with `onChange`).
//!
//! The extension lives in another process. Were the field to show only what
//! the extension last said, each keystroke would wait for a round trip, and
//! a slow extension would answer the first key after the third was typed:
//! letters lost, the caret thrown to the end, an input method's composition
//! broken off. So what is typed belongs to the field here, at once. The
//! extension is told, and is listened to only when it says something new.
//!
//! Each change sent carries a number, and the runtime's components send
//! back the highest number they have handled (`ack`) beside the value. A
//! value that comes with an older number was decided before the extension
//! knew what has been typed since, and is ignored: its answer to the latest
//! keystroke is still on its way.

import { useLayoutEffect, useRef } from "react";

export interface Typing {
  /// What the field shows.
  text: string;
  /// How many changes were made here; the last one sent carries this number.
  seq: number;
  /// The extension's value the text last agreed with. While the extension
  /// still says this, it has said nothing new.
  base: string;
}

/// A field that starts out showing `text`. If the extension's value is
/// another, that is news, and `reconcile` takes it.
export const startTyping = (text: string, ack: number | undefined): Typing => ({
  text,
  // The runtime's count outlives this field (a view left and come back to).
  seq: ack ?? 0,
  base: text,
});

/// The user changed the text.
export const typed = (field: Typing, text: string): Typing => ({ ...field, text, seq: field.seq + 1 });

/// The extension's `value`, with the number of the last change it had
/// handled, has arrived (or still stands): what the field becomes.
///
/// The value is taken only when all of this holds: nothing is being
/// composed; the extension has seen everything typed; and the value is
/// neither what the field shows nor what the extension said before. That is
/// the extension clearing the field or rewriting what was typed, never an
/// echo of an earlier keystroke.
export function reconcile(field: Typing, value: string | undefined, ack: number | undefined, composing: boolean): Typing {
  if (value === undefined || composing) return field;
  if (ack !== undefined && ack < field.seq) return field;
  if (value === field.text) return field.base === value ? field : { ...field, base: value };
  if (value === field.base) return field;
  return { ...field, text: value, base: value };
}

/// Keeps a field's text local-first. `text` is what the field shows, kept
/// by the caller, who gives `adopt` to have it replaced when the extension
/// really changes it. `key` names the field: a different one starts afresh.
///
/// Returns `next()`, the number to send with a change the user just made,
/// and the composition handlers the input must be given.
export function useTyping(options: { key: unknown; text: string; value: unknown; ack: unknown; adopt(text: string): void }) {
  const value = options.value == null ? undefined : String(options.value);
  const ack = typeof options.ack === "number" ? options.ack : undefined;
  const state = useRef({ key: options.key, ...startTyping(options.text, ack) });
  if (state.current.key !== options.key) state.current = { key: options.key, ...startTyping(options.text, ack) };
  state.current.text = options.text;
  const composing = useRef(false);
  const latest = useRef({ value, ack, adopt: options.adopt });
  latest.current = { value, ack, adopt: options.adopt };

  const settle = () => {
    const before = state.current;
    const after = reconcile(before, latest.current.value, latest.current.ack, composing.current);
    state.current = { ...after, key: before.key };
    if (after.text !== before.text) latest.current.adopt(after.text);
  };
  // Before the paint, so a value that is taken never shows a frame late.
  useLayoutEffect(settle, [value, ack, options.key]);

  return {
    next: () => ++state.current.seq,
    composition: {
      onCompositionStart: () => {
        composing.current = true;
      },
      // What arrived during the composition was held back; look again now.
      onCompositionEnd: () => {
        composing.current = false;
        // After the input event that carries the committed text.
        setTimeout(settle);
      },
    },
  };
}
