// The messages between the extension runtime (Node.js) and the webview, and
// the runtime's side of its conversation with the host.
//
// What the host itself reads or writes is declared in Rust
// (`src-tauri/src/wire.rs`) and generated into `./bindings`; this file adds
// what the host only passes along, so both ends still share one definition.

import type { AiProvider } from "./bindings/AiProvider";
import type { AudioDevice } from "./bindings/AudioDevice";
import type { Capture } from "./bindings/Capture";
import type { ExtCaller } from "./bindings/ExtCaller";
import type { ExtError } from "./bindings/ExtError";
import type { ExtRequest } from "./bindings/ExtRequest";
import type { ExtWindow } from "./bindings/ExtWindow";
import type { HostMessage } from "./bindings/HostMessage";
import type { HostRequest } from "./bindings/HostRequest";
import type { MemoryItem } from "./bindings/MemoryItem";
import type { MemoryResult } from "./bindings/MemoryResult";
import type { Presentation } from "./bindings/Presentation";
import type { RecognizedText } from "./bindings/RecognizedText";
import type { Rect } from "./bindings/Rect";
import type { Subscription } from "./bindings/Subscription";
import type { Support } from "./bindings/Support";
import type { WebviewMessage } from "./bindings/WebviewMessage";

export type { ExtCaller, ExtError, ExtRequest, HostMessage, HostRequest, WebviewMessage };
export { event } from "./bindings/events";

/// One node of a view; `children` are node ids.
export interface WireNode {
  id: number;
  type: string;
  props: Record<string, any>;
  children: number[];
}

/// The changed nodes of the view on top (all of them when `full`).
export interface WireView {
  key: number;
  depth: number;
  full: boolean;
  root: number[];
  nodes: WireNode[];
  removed: number[];
}

export interface Toast {
  id: number;
  title: string;
  message?: string;
  style: "SUCCESS" | "FAILURE" | "ANIMATED";
  primaryAction?: { title: string };
  secondaryAction?: { title: string };
}

/// What the runtime asks of the webview, by method.
export interface UiRequests {
  "ui.confirm": {
    title: string;
    message?: string;
    icon?: unknown;
    primaryAction: { title: string; style?: string };
    dismissAction: { title: string };
    /// A third choice, answered with "always". Only the host asks with
    /// one (`src-tauri/src/confirm.rs`).
    alwaysAction?: { title: string };
    /// The dialog starts on the dismiss button: Yes gives something away,
    /// and must not be what a key meant for something else answers. Only
    /// the host asks so (`Consent` in `src-tauri/src/confirm.rs`).
    cautious?: boolean;
  };
}

export type UiRequest = { [M in keyof UiRequests]: { type: "request"; id: number; method: M; params: UiRequests[M] } }[keyof UiRequests];

/// What the runtime tells the webview. `exited` is the host speaking for a
/// runtime that is gone.
export type RuntimeMessage =
  | {
      type: "launched";
      session: number;
      mode: string;
      title: string;
      fallbackText?: string;
      extension: { name: string; title: string; dir: string; icon?: string };
    }
  | { type: "render"; session: number; view: WireView | null }
  /// Without a session: a launch that failed before it had one.
  | { type: "closed"; session?: number }
  | { type: "pop-to-root" }
  | { type: "clear-search-bar" }
  | { type: "field"; node: number; op: "focus" | "reset" }
  | { type: "toast"; toast: Toast }
  | { type: "toast-hide"; id: number }
  | UiRequest
  | { type: "exited" };

/// What the runtime tells the host alone.
export type RuntimeNotice =
  | { type: "ready" }
  /// A menu bar command's menu; null takes it off the menu bar.
  | { type: "menu"; key: string; menu: unknown }
  | { type: "background-done"; id: number; error?: string };

/// A request, as the arguments of the runtime's `request()`.
export type Call =
  | { [R in HostRequest as R["method"]]: R extends { params: infer P } ? [method: R["method"], params: P] : [method: R["method"]] }[HostRequest["method"]]
  | { [M in keyof UiRequests]: [method: M, params: UiRequests[M]] }[keyof UiRequests];

/// What an extension asks of the host through `@heyo/api`, as the arguments
/// of the runtime's `ask()`.
export type ExtCall = {
  [R in ExtRequest as R["method"]]: R extends { params: infer P } ? [method: R["method"], params: P] : [method: R["method"]];
}[ExtRequest["method"]];

export type ExtMethod = ExtRequest["method"];

/// What goes after the method in asking for it: its parameters, or nothing.
export type ExtParams<M extends ExtMethod> = Extract<ExtRequest, { method: M }> extends { params: infer P } ? [params: P] : [];

/// An application as the host describes one (`application` in host.rs).
export interface Application {
  name: string;
  path: string;
  bundleId?: string;
  localizedName?: string;
  /// The file of its icon, which Raycast's does not tell.
  icon?: string;
}

/// What a search of the file index finds.
export interface FoundFiles {
  files: { path: string; isDirectory: boolean }[];
  /// The index is still being built: there may be more.
  scanning: boolean;
  /// How many files and folders there are to search.
  total: number;
}

/// `Table`, which has to say something for every one of `Keys` and for
/// nothing else.
type ForEach<Keys extends PropertyKey, Table extends Record<Keys, unknown> & Record<Exclude<keyof Table, Keys>, never>> = Table;

/// What the host answers each request of an extension's with; `null` where
/// it does something and has nothing to say. This is the one place the
/// runtime says so: `ext.rs` builds its answers as JSON, some from a type
/// that is generated into `./bindings` and some (an application, the files
/// found, the plain values) without one.
export type ExtReplies = ForEach<
  ExtMethod,
  {
    "ext.unsubscribe": null;
    "ext.supported": Support;
    "ext.windows.list": ExtWindow[];
    "ext.windows.active": ExtWindow | null;
    "ext.windows.activate": null;
    "ext.windows.close": null;
    "ext.windows.arrange": null;
    "ext.windows.setBounds": null;
    "ext.windows.monitors": Rect[];
    "ext.frontmostApplication": Application | null;
    "ext.screen.capture": Capture;
    "ext.screen.recognizeText": RecognizedText;
    "ext.dictation.installed": boolean;
    "ext.dictation.listen": Subscription;
    "ext.dictation.stop": null;
    "ext.memory.search": MemoryResult;
    "ext.memory.get": MemoryItem | null;
    "ext.clipboardHistory.copy": null;
    "ext.files.search": FoundFiles;
    "ext.system.run": string | null;
    "ext.system.appearance": "light" | "dark";
    "ext.system.notify": null;
    "ext.system.audioDevices": AudioDevice[];
    "ext.system.useAudioDevice": null;
    "ext.clipboard.paste": null;
    "ext.ai.provider": AiProvider;
    "ext.activity.start": { id: string };
    "ext.activity.update": null;
    "ext.activity.end": null;
    "ext.activity.list": string[];
    "ext.activity.get": boolean;
    "ext.activity.presentation": Presentation;
    "ext.activity.onEvent": Subscription;
  }
>;

/// Everything the runtime writes to the host. A request of an extension's
/// (`ext.*`) says whose it is.
export type Outgoing =
  | Exclude<RuntimeMessage, UiRequest | { type: "exited" }>
  | RuntimeNotice
  | ({ type: "request"; id: number; method: string; params?: unknown } & Partial<ExtCaller>);

/// The host's answer to a request: to `host.*` what went wrong is a
/// sentence, to `ext.*` it has a code.
export type HostReply = { type: "reply"; id: number; result?: unknown; error?: string | ExtError };

/// Everything the runtime reads from the host: its own messages, and the
/// webview's, which it relays.
export type Incoming = HostMessage | HostReply | Exclude<WebviewMessage, { type: "reply" }>;
