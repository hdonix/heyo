import { describe, expect, it } from "vitest";

import { pixelRatio, toOverlay, windowAt, type Rect } from "./screenshotWindows";

const rect = (x: number, y: number, width: number, height: number): Rect => ({ x, y, width, height });

describe("pixelRatio", () => {
  it("is how many captured pixels one overlay pixel covers", () => {
    expect(pixelRatio(1920, 1920, 1)).toBe(1);
    expect(pixelRatio(3840, 1920, 2)).toBe(2);
    expect(pixelRatio(2560, 2560 / 1.5, 1.5)).toBeCloseTo(1.5);
    // The overlay's real size wins over the scale the host was told.
    expect(pixelRatio(3840, 1920, 1)).toBe(2);
  });

  it("falls back to the monitor's scale", () => {
    expect(pixelRatio(undefined, 1920, 2)).toBe(2);
    expect(pixelRatio(3840, 0, 2)).toBe(2);
    expect(pixelRatio(undefined, 0, 0)).toBe(1);
  });
});

describe("toOverlay", () => {
  it("leaves a rectangle alone at scale 1", () => {
    expect(toOverlay(rect(80, 100, 800, 600), 1)).toEqual(rect(80, 100, 800, 600));
  });

  it("halves it at scale 2", () => {
    // A window at (4000, 200) on a monitor whose corner is (3840, 0): the
    // host sends (160, 200), which is (80, 100) in the overlay.
    expect(toOverlay(rect(160, 200, 1600, 1200), 2)).toEqual(rect(80, 100, 800, 600));
    expect(toOverlay(rect(0, 0, 3840, 2160), 2)).toEqual(rect(0, 0, 1920, 1080));
  });

  it("keeps touching windows touching at a fractional scale", () => {
    const [left, right] = [toOverlay(rect(0, 0, 1281, 1440), 1.5), toOverlay(rect(1281, 0, 1279, 1440), 1.5)];
    expect(left.x + left.width).toBe(right.x);
    expect(right.x + right.width).toBeCloseTo(2560 / 1.5);
  });

  it("goes back to the same captured pixels when the selection is rendered", () => {
    const ratio = 1.25;
    const window = rect(37, 53, 1001, 777);
    const shown = toOverlay(window, ratio);
    expect(Math.round(shown.x * ratio)).toBe(window.x);
    expect(Math.round(shown.y * ratio)).toBe(window.y);
    expect(Math.round(shown.width * ratio)).toBe(window.width);
    expect(Math.round(shown.height * ratio)).toBe(window.height);
  });
});

describe("windowAt", () => {
  // Topmost first: a dialog over an editor over a maximized browser.
  const windows = [rect(300, 300, 400, 300), rect(100, 100, 800, 600), rect(0, 0, 1920, 1080)];

  it("finds the topmost window under the pointer", () => {
    expect(windowAt(windows, 350, 350)).toBe(0);
    expect(windowAt(windows, 150, 150)).toBe(1);
    expect(windowAt(windows, 1000, 900)).toBe(2);
  });

  it("finds a window by the part of it that shows", () => {
    // Under the dialog's rectangle the editor cannot be reached; beside it, it can.
    expect(windowAt(windows, 299, 350)).toBe(1);
    expect(windowAt(windows, 700, 350)).toBe(1);
  });

  it("counts the left and top edges in, the right and bottom ones out", () => {
    const pair = [rect(0, 0, 960, 1080), rect(960, 0, 960, 1080)];
    expect(windowAt(pair, 959.5, 10)).toBe(0);
    expect(windowAt(pair, 960, 10)).toBe(1);
    expect(windowAt(pair, 1920, 10)).toBe(-1);
    expect(windowAt(pair, 10, 1080)).toBe(-1);
  });

  it("finds nothing over the bare desktop", () => {
    expect(windowAt([rect(100, 100, 200, 200)], 50, 50)).toBe(-1);
    expect(windowAt([], 50, 50)).toBe(-1);
  });
});
