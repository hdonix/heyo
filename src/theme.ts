//! Which colour scheme the interface uses: the system's, or the one chosen
//! in the settings. The colours themselves are in styles/tokens.css, which
//! follows `color-scheme`; all this does is say which, on the root element.

import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

import type { Appearance } from "../shared/bindings/Appearance";
import type { SettingsView } from "../shared/bindings/SettingsView";
import { event } from "../shared/bindings/events";

export type { Appearance };

/// Whether this window is showing the dark scheme right now.
export function isDark() {
  const chosen = document.documentElement.dataset.theme;
  if (chosen === "light" || chosen === "dark") return chosen === "dark";
  return window.matchMedia("(prefers-color-scheme: dark)").matches;
}

/// Applies an appearance to this window. "system" leaves it to the
/// stylesheet, which follows the system.
export function applyAppearance(appearance: Appearance | undefined) {
  const root = document.documentElement;
  if (appearance === "light" || appearance === "dark") root.dataset.theme = appearance;
  else delete root.dataset.theme;
}

/// Keeps this window in the appearance the settings ask for, now and as
/// they change.
export function followAppearance() {
  invoke<SettingsView>("get_settings")
    .then((settings) => applyAppearance(settings.appearance))
    .catch(() => {});
  listen<SettingsView>(event.settingsChanged, ({ payload }) => applyAppearance(payload.appearance));
}
