//! The `@heyo/api` module handed to extensions: everything of
//! `@raycast/api` under the same names, and Heyo's own on top
//! (docs/extension-api.md; the types an extension author sees are in
//! `packages/api/index.d.ts`, and `types.check.ts` holds the two together).
//!
//! `@raycast/api` is given this same object. An extension that imports
//! both, or `@heyo/api` with `@raycast/utils` (which imports the other),
//! then shares one navigation stack, one set of toasts and one cache.

import type { Support } from "../../../shared/bindings/Support";
import raycast from "../api";
import { LiveActivity } from "./activity";
import { AI } from "./ai";
import { Dictation } from "./dictation";
import { HeyoError } from "./error";
import { ClipboardHistory, Memory } from "./memory";
import { Screen } from "./screen";
import { Clipboard, Files, System } from "./system";
import { Windows } from "./windows";

export type { Support };

const SESSIONS = ["x11", "wayland", "windows", "unknown"] as const;

/// What `environment` says beside what Raycast's does. The host says it as
/// it starts the runtime (`runtime.rs`).
export const heyoEnvironment = {
  /// For logs and reports; ask `has()` what there is, not this.
  heyoVersion: process.env.HEYO_VERSION ?? "",
  apiVersion: Number(process.env.HEYO_API_VERSION) || 1,
  platform: (process.platform === "win32" ? "windows" : "linux") as "linux" | "windows",
  session: SESSIONS.find((session) => session === process.env.HEYO_SESSION) ?? "unknown",
  /// Heyo has no "nothing leaves this computer" switch yet: never true.
  offline: false,
};

/// Raycast's `environment`, whose fields follow the running command, with Heyo's.
const environment = Object.defineProperties(
  {},
  { ...Object.getOwnPropertyDescriptors(raycast.environment), ...Object.getOwnPropertyDescriptors(heyoEnvironment) },
) as typeof raycast.environment & typeof heyoEnvironment;

/// What is there by name but only ever refuses.
const absent = ["BrowserExtension"];

/// Whether a part of the API is there and does something, by its name as
/// imported: `has("Screen.recognizeText")`.
function has(feature: string): boolean {
  if (absent.some((name) => feature === name || feature.startsWith(`${name}.`))) return false;
  let at: unknown = api;
  for (const part of feature.split(".")) {
    if ((typeof at !== "object" && typeof at !== "function") || at === null || !Object.hasOwn(at, part)) return false;
    at = (at as Record<string, unknown>)[part];
  }
  return at !== undefined;
}

const api = {
  ...raycast,
  AI,
  Clipboard,
  environment,

  isHeyo: true as const,
  has,
  HeyoError,
  Windows,
  Screen,
  Dictation,
  Memory,
  ClipboardHistory,
  Files,
  System,
  LiveActivity,
};

export default api;
