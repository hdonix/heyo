//! The mark on the selected row of a list: one element that slides from row
//! to row, instead of each row turning a background on and off.

import { createElement, useLayoutEffect, useRef } from "react";

import { pointerDriven } from "./pointer";
import { reveal } from "./selection";

/// Put first inside a list (which must be positioned). It finds the
/// `.item.selected` there after every render and moves under it. When
/// `resetKey` changes (the query, the view) it is simply there, since
/// sliding across a list that has just changed means nothing.
export function SelectionIndicator(props: { resetKey?: unknown; as?: "div" | "li" }) {
  const ref = useRef<HTMLElement>(null);
  const lastKey = useRef<unknown>(Symbol());
  const lastRow = useRef<HTMLElement | null>(null);

  useLayoutEffect(() => {
    const mark = ref.current;
    const list = mark?.parentElement;
    if (!mark || !list) return;
    list.classList.add("has-indicator");
    const row = list.querySelector<HTMLElement>(".item.selected");
    if (!row) {
      mark.style.opacity = "0";
      return;
    }
    // It slides for the keyboard. Under the pointer it is simply there:
    // sliding after a pointer that is already somewhere else looks like lag.
    const appearing = mark.style.opacity !== "1" || lastKey.current !== props.resetKey || pointerDriven();
    lastKey.current = props.resetKey;
    // A row out of view is scrolled to, here and now, so that the mark can
    // be carried along: the list jumps by a row while the mark, seen from
    // the window, goes on from where it was. Left to slide through a list
    // that has jumped, it would leap back a row and catch up, at every step.
    const scrolled = !pointerDriven() && row !== lastRow.current ? reveal(list, row) : 0;
    lastRow.current = row;
    if (scrolled && !appearing) {
      const now = new DOMMatrixReadOnly(getComputedStyle(mark).transform).m42;
      mark.classList.add("still");
      mark.style.transform = `translateY(${now + scrolled}px)`;
      // Take the place before the slide from it begins.
      void mark.offsetHeight;
    }
    mark.classList.toggle("still", appearing);
    mark.style.transform = `translateY(${row.offsetTop}px)`;
    mark.style.height = `${row.offsetHeight}px`;
    mark.style.opacity = "1";
  });

  return createElement(props.as ?? "div", { ref, className: "selection-indicator", "aria-hidden": true });
}
