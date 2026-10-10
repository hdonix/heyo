// Picking a window in the screenshot overlay: where the host's window
// rectangles are in the overlay, and which one the pointer is on.
//
// The host sends them in physical pixels from the capture's top left corner
// (it has already taken the monitor's origin off). The overlay covers the
// capture exactly, so one overlay (CSS) pixel is `captureWidth / overlayWidth`
// physical ones: the monitor's scale factor, measured rather than assumed.

export interface Rect {
  x: number;
  y: number;
  width: number;
  height: number;
}

/// Physical pixels per overlay pixel. `scale` (the monitor's, as the host
/// knows it) stands in when the sizes are not known.
export function pixelRatio(captureWidth: number | undefined, overlayWidth: number, scale: number) {
  return captureWidth && overlayWidth > 0 ? captureWidth / overlayWidth : scale > 0 ? scale : 1;
}

/// A rectangle of the capture in overlay pixels. Its edges are converted,
/// not its size, so windows that touch in the capture touch in the overlay.
export function toOverlay(rect: Rect, ratio: number): Rect {
  const [left, top] = [rect.x / ratio, rect.y / ratio];
  return { x: left, y: top, width: (rect.x + rect.width) / ratio - left, height: (rect.y + rect.height) / ratio - top };
}

/// The index of the topmost window under a point, or -1. `windows` are
/// topmost first; a window's right and bottom edges are outside it, so of
/// two windows side by side each point belongs to one.
export function windowAt(windows: Rect[], x: number, y: number) {
  return windows.findIndex((w) => x >= w.x && x < w.x + w.width && y >= w.y && y < w.y + w.height);
}
