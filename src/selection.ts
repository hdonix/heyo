//! Which row of a list is selected, and how that changes: by the arrow
//! keys, or by the pointer moving over a row.
//!
//! The root search, an extension's list or grid, and the action panel are
//! all lists of this kind. What they share lives here: how a step lands,
//! when the selection is scrolled into view, and when a mouse event counts
//! as the pointer choosing a row (see `pointer.ts` for why not always).

import { useLayoutEffect, useRef, type DependencyList } from "react";

import { pointerDriven, pointerMoved } from "./pointer";

/// Where a step of `delta` rows from `index` lands among `count` rows.
/// Wrapping, the ends meet (a short list walked with the arrow keys);
/// otherwise the step stops at the ends (a long list, a page at a time).
export function step(index: number, delta: number, count: number, wrap: boolean): number {
  if (count <= 0) return 0;
  if (!wrap) return Math.min(count - 1, Math.max(0, index + delta));
  return (((index + delta) % count) + count) % count;
}

/// The props that let the pointer select a row: only by really moving.
export function underPointer(select: () => void) {
  return {
    onMouseMove: () => {
      if (pointerMoved()) select();
    },
  };
}

/// How far a list must scroll for a row to be inside its view, less the
/// padding kept clear at each end: negative is up, zero is "it is in view".
/// (All in the same coordinates, e.g. the window's.)
export function shortfall(row: { top: number; bottom: number }, view: { top: number; bottom: number }, padding: { top: number; bottom: number }): number {
  if (row.top < view.top + padding.top) return row.top - (view.top + padding.top);
  if (row.bottom > view.bottom - padding.bottom) return row.bottom - (view.bottom - padding.bottom);
  return 0;
}

/// Scrolls `list` just far enough to show `row`, at once, and returns how
/// far it went. The list's `scroll-padding` is kept clear.
export function reveal(list: HTMLElement, row: HTMLElement): number {
  const style = getComputedStyle(list);
  const padding = { top: parseFloat(style.scrollPaddingTop) || 0, bottom: parseFloat(style.scrollPaddingBottom) || 0 };
  const before = list.scrollTop;
  const by = shortfall(row.getBoundingClientRect(), list.getBoundingClientRect(), padding);
  if (by) list.scrollTop = before + by;
  return list.scrollTop - before;
}

/// A ref for the list. Whenever `selection` changes by the keyboard, the
/// `.selected` row in it is scrolled into view, before the change is
/// painted: a frame with the selection out of sight is a flicker. Under the
/// pointer the row is in view already, and scrolling would move the rows
/// under the pointer and the selection with them.
export function useReveal<E extends HTMLElement>(selection: DependencyList) {
  const list = useRef<E>(null);
  useLayoutEffect(() => {
    const row = list.current?.querySelector<HTMLElement>(".selected");
    if (list.current && row && !pointerDriven()) reveal(list.current, row);
  }, selection);
  return list;
}
