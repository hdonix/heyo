//! `Windows`: the windows of other applications (permission `windows`).

import type { ExtWindow } from "../../../shared/bindings/ExtWindow";
import type { Rect } from "../../../shared/bindings/Rect";
import type { Support } from "../../../shared/bindings/Support";
import type { WindowLayout } from "../../../shared/bindings/WindowLayout";
import { ask } from "../protocol";

export type { ExtWindow as Window, Rect, WindowLayout as Layout };

export const Windows = {
  supported: () => ask<Support>("ext.supported", { module: "windows" }),
  /// The most recently used first; none of Heyo's own.
  list: () => ask<ExtWindow[]>("ext.windows.list"),
  /// The window the user is working in: with the palette up, the one that
  /// was in front before it.
  async active(): Promise<ExtWindow | undefined> {
    return (await ask<ExtWindow | null>("ext.windows.active")) ?? undefined;
  },
  async activate(id: number): Promise<void> {
    await ask("ext.windows.activate", { id });
  },
  async close(id: number): Promise<void> {
    await ask("ext.windows.close", { id });
  },
  async arrange(id: number, layout: WindowLayout): Promise<void> {
    await ask("ext.windows.arrange", { id, layout });
  },
  /// Physical pixels.
  async setBounds(id: number, bounds: Rect): Promise<void> {
    await ask("ext.windows.setBounds", { id, bounds });
  },
  monitors: () => ask<Rect[]>("ext.windows.monitors"),
};
