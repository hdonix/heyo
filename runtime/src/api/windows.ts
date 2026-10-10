//! Raycast's `WindowManagement`, answered by the host's window module.
//!
//! What differs from macOS: a "desktop" here is a monitor (Linux and Windows
//! have no Spaces an application can address); "fullscreen" maximizes; and
//! positions and sizes are physical pixels.
//!
//! Reaching other applications' windows is the user's to allow (the
//! `windows` permission), also for an extension from the store that says
//! nothing of permissions: the host asks the first time.

import type { Rect } from "../../../shared/bindings/Rect";
import { askThroughRaycast as ask } from "../protocol";
import type { Application } from "./system";

/// A window as the host lists it.
export interface HostWindow {
  id: number;
  title: string;
  class: string;
  active: boolean;
  bounds?: Rect;
}

export const DesktopType = { User: "User", FullScreen: "FullScreen" } as const;

export interface Bounds {
  position: { x: number; y: number };
  size: { width: number; height: number };
}

type BoundsChange = { position?: { x?: number; y?: number }; size?: { width?: number; height?: number } };

/// The monitor a rectangle is on: the one holding its centre, else the first.
export function monitorOf(bounds: Rect | undefined, monitors: Rect[]): number {
  if (!bounds) return 0;
  const [x, y] = [bounds.x + bounds.width / 2, bounds.y + bounds.height / 2];
  const index = monitors.findIndex((m) => x >= m.x && x < m.x + m.width && y >= m.y && y < m.y + m.height);
  return Math.max(index, 0);
}

export function toWindow(window: HostWindow, monitors: Rect[]) {
  const { x = 0, y = 0, width = 0, height = 0 } = window.bounds ?? {};
  return {
    id: String(window.id),
    bounds: { position: { x, y }, size: { width, height } } as Bounds,
    desktopId: String(monitorOf(window.bounds, monitors) + 1),
    fullScreenSettable: true,
    resizable: true,
    positionable: true,
    active: window.active,
  };
}

/// Where a window ends up when only some of its bounds are given.
export function changed(current: Rect | undefined, change: BoundsChange): Rect {
  const from = current ?? { x: 0, y: 0, width: 800, height: 600 };
  return {
    x: Math.round(change.position?.x ?? from.x),
    y: Math.round(change.position?.y ?? from.y),
    width: Math.round(change.size?.width ?? from.width),
    height: Math.round(change.size?.height ?? from.height),
  };
}

const listed = async () => {
  const [windows, monitors] = await Promise.all([ask<HostWindow[]>("ext.windows.list"), ask<Rect[]>("ext.windows.monitors")]);
  return { windows, monitors };
};

export const WindowManagement = {
  DesktopType,

  async getActiveWindow() {
    const [window, monitors] = await Promise.all([ask<HostWindow | null>("ext.windows.active"), ask<Rect[]>("ext.windows.monitors")]);
    if (!window) throw new Error("No window is focused");
    return toWindow({ ...window, active: true }, monitors);
  },

  async getWindowsOnActiveDesktop() {
    const { windows, monitors } = await listed();
    const active = windows.find((w) => w.active);
    const desktop = monitorOf(active?.bounds, monitors);
    return windows.filter((w) => monitorOf(w.bounds, monitors) === desktop).map((w) => toWindow(w, monitors));
  },

  async getDesktops() {
    const { windows, monitors } = await listed();
    const active = monitorOf(windows.find((w) => w.active)?.bounds, monitors);
    return monitors.map((monitor, index) => ({
      id: String(index + 1),
      screenId: String(index + 1),
      size: { width: monitor.width, height: monitor.height },
      active: index === active,
      type: DesktopType.User,
    }));
  },

  async setWindowBounds(options: { id: string; bounds: BoundsChange | "fullscreen"; desktopId?: string }) {
    const id = Number(options.id);
    if (options.bounds === "fullscreen") return void (await ask("ext.windows.arrange", { id, layout: "maximize" }));
    const { windows } = await listed();
    const window = windows.find((w) => w.id === id);
    if (!window) throw new Error(`No window with the id ${options.id}`);
    await ask("ext.windows.setBounds", { id, bounds: changed(window.bounds, options.bounds) });
  },
};

export async function getFrontmostApplication(): Promise<Application> {
  const application = await ask<Application | null>("ext.frontmostApplication");
  if (!application) throw new Error("No application is in front");
  return application;
}
