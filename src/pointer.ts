//! Whether the pointer or the keyboard is driving, and whether a mouse
//! event is the pointer really moving.
//!
//! Lists select the row under the pointer. But the web view also sends
//! mouse events when the page moves under a pointer that is still (a list
//! scrolling to keep the selection in view), and acting on those makes the
//! selection chase the scrolling: the highlight jumps and flickers. So rows
//! select only on real movement, and a list scrolls its selection into view
//! only when the keyboard moved it.

let x = -1;
let y = -1;
let moved = false;
let pointer = false;

window.addEventListener(
  "mousemove",
  (event) => {
    moved = event.screenX !== x || event.screenY !== y;
    if (moved) {
      x = event.screenX;
      y = event.screenY;
      pointer = true;
    }
  },
  // Before any handler of a row sees the same event.
  true,
);
// Turning the wheel is the pointer's doing too, though nothing moves.
window.addEventListener("wheel", () => (pointer = true), { capture: true, passive: true });
window.addEventListener("keydown", () => (pointer = false), true);

/// During a mouse event: whether the pointer actually moved.
export function pointerMoved() {
  return moved;
}

/// Whether what last changed things was the pointer (rather than a key).
export function pointerDriven() {
  return pointer;
}
