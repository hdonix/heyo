//! One declaration for every function Heyo adds to the API: the module it
//! is in, its name, and the request it asks of the host.
//!
//! What else there is to know of a function follows from its declaration
//! instead of being said again somewhere:
//! - a function that is nothing but its request is made from it
//!   (`implement` in `asking.ts`). One with behaviour of its own is written
//!   in its module's file, and `implement` does not compile until every
//!   function declared so, and no other, is given;
//! - the permission it needs is the request's, which the host says
//!   (`needed` in `permissions.rs`, written into `shared/bindings/needs.ts`).
//!   The host also names the function when it refuses it, and
//!   `members.test.ts` holds its names to these;
//! - what it answers with is the request's too (`ExtReplies` in
//!   `shared/wire.ts`);
//! - `packages/api` has the same names: the types by `types.check.ts`, the
//!   stand-in for outside Heyo by `package.test.ts`;
//! - the developer tools take from here which modules there are and what
//!   using one needs (`dev/check.ts`).
//!
//! This imports nothing of the runtime: the developer tools read it, and
//! loading the runtime's protocol would take their standard output.

import type { Action } from "../../../shared/bindings/Action";
import type { AudioKind } from "../../../shared/bindings/AudioKind";
import type { Kind } from "../../../shared/bindings/Kind";
import type { Module } from "../../../shared/bindings/Module";
import { needs } from "../../../shared/bindings/needs";
import type { Permission } from "../../../shared/bindings/Permission";
import type { Rect } from "../../../shared/bindings/Rect";
import type { WindowLayout } from "../../../shared/bindings/WindowLayout";
import type { ExtMethod, ExtParams } from "../../../shared/wire";

/// A function that is its request: its arguments make the parameters, and
/// what the host answers is what it returns (nothing, where the host
/// answers null).
export interface Asks<M extends ExtMethod, A extends unknown[]> {
  method: M;
  params?: (...args: A) => ExtParams<M>[0];
}

/// A function with behaviour of its own, written in its module's file. The
/// request is the one it is there for, whose permission it needs.
export interface ByHand<M extends ExtMethod> {
  method: M;
  byHand: true;
  /// For a function of Raycast's that Heyo gives more options: it is
  /// Raycast's, and asks nothing of the host, until given one of these.
  given?: readonly string[];
}

export type Declared = Asks<any, any> | ByHand<any>;

/// The requests that take no parameters.
type Bare = { [M in ExtMethod]: ExtParams<M> extends [] ? M : never }[ExtMethod];

function asks<M extends Bare>(method: M): Asks<M, []>;
function asks<M extends Exclude<ExtMethod, Bare>, A extends unknown[]>(method: M, params: (...args: A) => ExtParams<M>[0]): Asks<M, A>;
function asks(method: ExtMethod, params?: (...args: any[]) => any): Asks<ExtMethod, any[]> {
  return { method, params };
}

const byHand = <M extends ExtMethod>(method: M): ByHand<M> => ({ method, byHand: true });
const whenGiven = <M extends ExtMethod>(method: M, ...given: string[]): ByHand<M> => ({ method, byHand: true, given });

/// How well this session can do what a module is for.
const supported = (module: Module) => asks("ext.supported", () => ({ module }));

export type MemorySource = "clipboard" | "screenshot" | "dictation" | "file" | "web" | `ext:${string}`;

export interface MemoryQuery {
  /// Words to find; may name a time, a source or a kind ("links from firefox last week").
  text?: string;
  sources?: MemorySource[];
  kinds?: Kind[];
  since?: Date;
  until?: Date;
  /// 50 unless given, 300 at most.
  limit?: number;
  /// Also find what means the same as `text` without containing its words,
  /// where the user has turned finding by meaning on (elsewhere this
  /// changes nothing). The answer waits for it: a second or two the first
  /// time, a few milliseconds after. What was found so is marked `byMeaning`.
  byMeaning?: boolean;
}

/// Heyo's own modules.
export const modules = {
  /// The windows of other applications.
  Windows: {
    supported: supported("windows"),
    /// The most recently used first; none of Heyo's own.
    list: asks("ext.windows.list"),
    /// The window the user is working in: with the palette up, the one that
    /// was in front before it.
    active: asks("ext.windows.active"),
    activate: asks("ext.windows.activate", (id: number) => ({ id })),
    close: asks("ext.windows.close", (id: number) => ({ id })),
    arrange: asks("ext.windows.arrange", (id: number, layout: WindowLayout) => ({ id, layout })),
    /// Physical pixels.
    setBounds: asks("ext.windows.setBounds", (id: number, bounds: Rect) => ({ id, bounds })),
    monitors: asks("ext.windows.monitors"),
  },

  /// Pictures of the screen and the text in them.
  Screen: {
    supported: supported("screen"),
    capture: byHand("ext.screen.capture"),
    recognizeText: byHand("ext.screen.recognizeText"),
  },

  /// What is said into the microphone, as text for the extension.
  Dictation: {
    supported: supported("dictation"),
    /// Whether the speech engine has been downloaded (Heyo Settings → Dictation).
    installed: asks("ext.dictation.installed"),
    listen: byHand("ext.dictation.listen"),
  },

  /// What Heyo remembers for the user. The clipboard history is the one
  /// source there is so far, and nothing can be added: copies marked as
  /// secrets were never recorded, so none is ever returned.
  Memory: {
    supported: supported("memory"),
    /// Without `text`, the most recent. There is no asking for everything.
    search: asks("ext.memory.search", ({ text, sources, kinds, since, until, limit, byMeaning }: MemoryQuery = {}) => ({
      text,
      sources,
      kinds,
      since: since?.getTime(),
      until: until?.getTime(),
      limit,
      byMeaning,
    })),
    get: asks("ext.memory.get", (id: string) => ({ id })),
  },

  /// What the clipboard history is beside a source of `Memory`: entries can
  /// be put back on the clipboard. Reading it is `Memory.search`.
  ClipboardHistory: {
    supported: supported("clipboardHistory"),
    /// Puts an entry back on the clipboard.
    copy: asks("ext.clipboardHistory.copy", (id: string) => ({ id })),
  },

  Files: {
    supported: supported("files"),
    /// Files and folders under the home folder, by name. Paths only.
    search: asks("ext.files.search", (query: string, options?: { limit?: number }) => ({ query, limit: options?.limit })),
  },

  System: {
    supported: supported("system"),
    /// Answers with what there is to tell the user ("Volume 40%"), if
    /// anything. Restarting, shutting down and logging out are confirmed by
    /// the user every time, and fail with `cancelled` when they say no.
    run: asks("ext.system.run", (action: Action) => ({ action })),
    appearance: asks("ext.system.appearance"),
    /// A short-lived desktop notification.
    notify: asks("ext.system.notify", (title: string, body?: string) => ({ title, body })),
    /// The speakers and the microphones, with which of each sound goes through.
    audioDevices: asks("ext.system.audioDevices"),
    /// Makes a device the one sound goes through, by its `name`.
    useAudioDevice: asks("ext.system.useAudioDevice", (kind: AudioKind, name: string) => ({ kind, name })),
  },

  /// Something in progress, shown by Heyo while the command is away
  /// (docs/live-activities.md, section 6).
  LiveActivity: {
    supported: supported("activity"),
    start: byHand("ext.activity.start"),
    list: byHand("ext.activity.list"),
    get: byHand("ext.activity.get"),
    /// How activities show on this desktop, to fit the content to.
    presentation: asks("ext.activity.presentation"),
    onEvent: byHand("ext.activity.onEvent"),
  },
};

/// What modules of Raycast's gain.
export const gained = {
  AI: {
    provider: byHand("ext.ai.provider"),
    /// Asking for the provider is how the host is asked whether the
    /// extension may use what Heyo adds.
    ask: whenGiven("ext.ai.provider", "system", "images", "messages", "material"),
  },
  Clipboard: {
    paste: whenGiven("ext.clipboard.paste", "restoreClipboard"),
  },
};

/// What else is asked of the host in an extension's name, by what the host
/// calls it when it refuses: what a handle of `LiveActivity` does, what
/// ends something a function began (which is free, and has no name), and
/// what Raycast's API reaches without a module of Heyo's (`../api/windows.ts`).
export const beside = {
  "ext.unsubscribe": null,
  "ext.dictation.stop": null,
  "ext.activity.update": "LiveActivity.Handle.update",
  "ext.activity.end": "LiveActivity.Handle.end",
  "ext.frontmostApplication": "getFrontmostApplication",
} satisfies Partial<Record<ExtMethod, string | null>>;

export type ModuleName = keyof typeof modules;

export interface Member {
  module: string;
  name: string;
  /// As an author writes it: `Windows.list`.
  member: string;
  method: ExtMethod;
  /// What the manifest must list for it to be answered, if anything.
  permission: Permission | null;
  /// See `ByHand.given`.
  given?: readonly string[];
}

function membersOf(group: Record<string, Record<string, Declared>>): Member[] {
  return Object.entries(group).flatMap(([module, functions]) =>
    Object.entries(functions).map(([name, declared]) => {
      const method: ExtMethod = declared.method;
      const given = "given" in declared ? declared.given : undefined;
      return { module, name, member: `${module}.${name}`, method, permission: needs[method]?.permission ?? null, ...(given ? { given } : {}) };
    }),
  );
}

/// Every function declared, Heyo's own modules first.
export const members: Member[] = [...membersOf(modules), ...membersOf(gained)];

/// The permissions the functions of a module need between them.
export function permissionsOf(module: ModuleName): Permission[] {
  const all = members.filter((member) => member.module === module).flatMap((member) => member.permission ?? []);
  return [...new Set(all)];
}
