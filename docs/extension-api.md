# The Heyo extension API: specification (draft v0.3)

Status: draft, revised to match what phases A–E implemented. Date: 2026-10-11.

This document defines `@heyo/api`. It is a **superset** of `@raycast/api`:
every export of Raycast's API is there under the same name and with the same
signature, so migrating an extension only takes changing
`import … from "@raycast/api"` to `import … from "@heyo/api"`. On top of
that it adds what Heyo itself can do (windows, the screen and text
recognition, dictation, local memory, the file index, system actions,
activities in progress, results in the root search).

A Raycast extension that does not change its imports runs as before; in Heyo
both package names point to the same implementation.

What changed from v0.1 to v0.2: `@heyo/api` went from "only what is added" to
"a superset of Raycast's API"; sections 1, 3 and 8 were rewritten for that,
and the modules in section 7 that share a name with one of Raycast's now
extend the original.

What changed from v0.2 to v0.3: once phases A–E were done (section 11), the
specification was brought in line with the implementation, and each place
that changed is marked "Changed". The gaps that showed when the built-in
commands moved to this API were all filled in the API: `MemoryItem` gained
the size of an image, the name and icon of the application it came from, the
time of the first copy and the number of copies, and a mark for "found by
meaning", and the fields that only the clipboard source has became optional;
`Memory.search` can find by meaning, and its upper limit is now 300;
`ClipboardHistory.search` is gone, and all reading goes through
`Memory.search`; `Files.search` returns `total`; `System` gained audio
devices; the order of `Windows.list()` is written down; `AI.ask` takes the
conversation so far (`messages`), text to work on (`material`) and a
callback with the whole answer (`onText`), Heyo's own Ask AI, AI Chat and
selected-text actions now go through it, and every failure is a `HeyoError`.
Also brought in line with the implementation: a refused permission is only
remembered for 60 seconds, and there is no "Only This Once"; `offline` is
always false; `apiVersion` does not block installing. Questions 5 and 7 of
section 12 have been answered by the implementation.
The `LiveActivity` module (7.9) and the `activity` permission (5.1), which
had been left out, are now here too; its design is in
`docs/live-activities.md`.

Section 12, at the end, lists the questions that need your decision; they
affect the implementation but not the overall structure.

---

## 1. Goals and non-goals

**Goals**

1. **Migrating takes changing one import.** Every export of `@raycast/api`
   is in `@heyo/api` with the same name, the same signature and the same
   behavior. Where the same behavior is not possible (concepts only macOS
   has) there is a definite, documented degradation, not a missing export or
   a crash.
2. Third-party extensions can do what only built-in commands can do today.
   The built-in commands move to the same API, and no private channel is
   left.
3. A migrated extension can still run in Raycast: it gains abilities on
   Heyo and degrades normally on Raycast.
4. Whether each ability is available in the current session (X11, GNOME
   Wayland, Windows, others) is something an extension can find out before
   it calls.
5. Before an extension reads user data that Heyo keeps (the clipboard
   history, memory, what is on the screen, the microphone), the user has
   explicitly agreed.
6. The API can evolve without breaking extensions that are already
   published.

**Non-goals**

- No promise that "any extension works once its imports are changed". API
  compatibility solves the API; an extension that itself calls macOS
  programs, AppleScript or paths under `/Applications` still has to be
  ported. `heyo ext check` is there to find such problems (section 9).
- No wrappers for D-Bus, systemd or the registry. In Node.js an extension
  can use existing libraries or command-line tools directly.
- No sandbox in v1 (see 5.4).
- Extensions do not register global hotkeys. The user binds hotkeys to
  commands in the settings.

---

## 2. Design principles

- **Modules by ability, not by platform.** An extension writes
  `Windows.list()`, not `X11.listWindows()`.
- **Ask first, then act.** Every module has `supported()`, which returns
  `"full" | "limited" | "none"`, one to one with the host's `Supported`
  (`src-tauri/src/platform/desktop.rs`).
- **Failures have types.** Every error is a `HeyoError` with a `code`; an
  extension never has to parse the wording.
- **The host is the only one that acts.** The extension runtime only
  forwards requests; windows, screen capture and indexing are all done in
  the host, so the differences between Windows and Wayland do not leak into
  extensions.
- **Declared in one place.** Request and reply types are declared in
  `src-tauri/src/wire.rs` and generated into `shared/bindings/` by
  `cargo test`; the types of `@heyo/api` come from there.
  Changed: every function Heyo adds (its module, its name, the request it
  sends) is declared once, in `runtime/src/heyo/members.ts`. The permission
  it needs is the host's to say (`needed` in `permissions.rs`, written to
  `shared/bindings/needs.ts` by `cargo test`), and the type of the reply is
  recorded per request in `ExtReplies` in `shared/wire.ts`. Functions that
  only forward a request are generated straight from the declaration; the
  types package, the stubs for outside Heyo, the modules and permissions
  `heyo ext check` knows, and the table of calls in the tests are all
  checked against this declaration, and one too few or one too many fails
  to compile or fails a test.

---

## 3. Package, loading and coexistence

### 3.1 The package

- The package is named `@heyo/api` and is published to npm.
- It exports two parts:
  1. **The Raycast part**: every export of `@raycast/api`, with the same
     names, signatures and types (the list is in section 8).
  2. **The Heyo part**: the new modules defined in section 7, and the
     extensions to some of Raycast's modules.
- In Heyo the runtime intercepts module loading (the `provided` table in
  `runtime/src/runtime.ts`), and `@raycast/api` and `@heyo/api` are both
  replaced by **the same object**. This is required: extensions often
  depend on `@raycast/utils` as well, which imports `@raycast/api`
  internally; only when both names point to the same implementation is
  there one navigation stack, one set of toasts and one cache, not two.

The behavior in the three cases:

| How the extension is written | In Heyo | In Raycast |
|---|---|---|
| Uses only `@raycast/api` (how the store is today) | Runs as before, without the added abilities | Runs as before |
| Moves to `@heyo/api`, uses only the Raycast part | Runs as before | Runs as before (see below) |
| Moves to `@heyo/api`, uses the Heyo part | Everything is available | The Raycast part works; the Heyo part degrades |

So that the second and third cases also run in Raycast, `@heyo/api` on npm
is put together like this:

```ts
// @heyo/api on npm (only really loaded outside Heyo)
export * from "@raycast/api";          // a peerDependency, provided by Raycast
export const isHeyo = false;
export const Windows = { supported: async () => "none", /* the rest throw HeyoError("unsupported") */ };
// …the other Heyo modules likewise
```

```ts
import { isHeyo, List, Windows } from "@heyo/api";

if (isHeyo && (await Windows.supported()) !== "none") {
  const windows = await Windows.list();
}
```

### 3.1.1 Migrating an extension

1. `npm install @heyo/api`.
2. Replace `"@raycast/api"` in the sources with `"@heyo/api"`.
   `heyo migrate <dir>` does this step, and adds the `heyo` field to
   `package.json`.
3. `@raycast/utils` needs no change: it goes on importing `@raycast/api`,
   and in Heyo it gets the same implementation.
4. `heyo ext check` reports the problems outside the API: calls to macOS
   programs, AppleScript, hard-coded macOS paths.
5. Build with `heyo build` (it marks `@heyo/api`, `@raycast/api` and `react`
   as external, and what it produces is the same as `ray build`).

An extension that skips step 2 is not affected; the point of step 2 is to
get the types of the Heyo part, and to declare permissions.

### 3.1.2 Where the types come from

There are two ways to do the types of the Raycast part, and you need to
decide (question 7 of section 12):

- **Re-export**: `@heyo/api` lists `@raycast/api` as a peerDependency and
  the types are a plain `export *`. Developers still have to install
  `@raycast/api`, but the types always agree with Raycast's and we do not
  have to maintain them.
- **Our own types**: we write an equivalent set of type declarations
  independently. Developers install only one package, but we have to follow
  Raycast's releases; the licence its `package.json` states is MIT, and I
  have not checked whether the type declarations come under other terms.

The specification takes "re-export" as the default.

### 3.2 The manifest

An extension adds a `heyo` field to its `package.json`. An extension without
this field cannot use the parts of the API that need a permission.

```json
{
  "name": "window-sessions",
  "commands": [{ "name": "restore", "title": "Restore Layout", "mode": "no-view" }],
  "heyo": {
    "apiVersion": 1,
    "permissions": ["windows", "memory.read"],
    "platforms": ["linux", "windows"]
  }
}
```

| Field | Meaning |
|---|---|
| `apiVersion` | The major version the extension was written against. Changed: when the major version Heyo supports is lower, installing is **not blocked** (installing follows the Raycast Store's flow, which does not read this field); calls that need a permission fail with `unsupported`, saying "written for a newer Heyo". |
| `permissions` | The permissions the extension may come to use (see section 5). A permission that is not listed is refused at the call, without asking. Changed: this list is not shown at install time; the user is asked when a permission is first used. |
| `platforms` | Optional. When it does not include the current platform, the extension's commands do not enter the root search. |

### 3.3 Command modes

`view`, `no-view` and `menu-bar` stay as they are, and one is added:

- `provider`: contributes results to the root search (see 7.10).

---

## 4. Versions and compatibility

- `apiVersion` is an integer major version. Within one major version things
  are only added, never changed: new functions, new optional parameters,
  new fields in what is returned.
- Removing something or changing what it means takes a new major version.
  Heyo supports the current major version and the one before it at the same
  time.
- Fine-grained detection uses `has()`, not comparing version numbers:

```ts
import { has } from "@heyo/api";
if (has("Screen.recognizeText")) { /* … */ }
```

- `environment.heyoVersion` gives Heyo's version string, for logs and
  problem reports only.

---

## 5. Permissions

### 5.1 The table of permissions

| Permission | Allows | Default |
|---|---|---|
| `windows` | Listing, activating, closing and moving other applications' windows | Asked on first use |
| `screen.capture` | Capturing an image of the screen or of a window | Never asked when the user selects the area each time; asked the first time for the whole screen or a given window |
| `screen.text` | Recognizing the text in an image | Not asked (it only processes images the extension itself supplies) |
| `microphone` | Recording and transcribing | Asked on first use |
| `clipboard.history` | Reading the clipboard history | Asked on first use |
| `memory.read` | Searching local memory (across sources) | Asked on first use |
| `memory.write` | Writing the extension's own entries to local memory | Asked on first use |
| `files.search` | Querying the file index (only paths are returned) | Not asked |
| `system` | Lock screen, sleep, volume, appearance, emptying the trash | Asked on first use; restart, shut down and log out are confirmed every time |
| `input.paste` | Pasting text into the frontmost application | Not asked (the same as Raycast's `Clipboard.paste`) |
| `ai` | Using the model the user has configured | Not asked (the same as Raycast's `AI.ask`). Changed: the ways of calling it that Raycast already has need no manifest; to use the options Heyo adds (`system`, `messages`, `images`, `material`) or `AI.provider()`, the manifest must list `ai`, otherwise the call fails with `denied` |
| `search.provider` | Contributing results to the root search, and so seeing every query the user types | Asked at install time |
| `activity` | Changed, new: showing what the extension is doing in the palette's "In Progress" section, and telling the user when it is done (7.9) | Not asked; the settings have a switch for each extension and one for all, and when it is off the call fails with `denied`. Only a command the user ran can start a new activity |

### 5.2 Asking and storing

- Asking is a confirmation in the palette that names the extension and says
  in plain words what the permission is. Changed: the only choices are
  "Allow" and "Deny"; there is no "Only This Once" (question 5 of
  section 12).
- Changed: only "Allow" is written to
  `$XDG_CONFIG_HOME/heyo/permissions.json` (recorded per extension name,
  file mode 0600). "Deny" is not written to disk and is only remembered in
  memory for 60 seconds: within that time, the same extension asking for the
  same permission again fails with `denied` straight away, without a prompt
  (otherwise an extension that asks on every render would flood the screen
  with confirmations), and after that it is asked again when it is used
  again. The extension's page in the settings window lists the permissions
  that have been allowed and can revoke them.
- There is only one confirmation at a time; when two requests want the same
  permission at once, the second waits for the answer to the first. Five
  minutes without an answer counts as the user having left, and fails with
  `denied`.
- When running in the background (menu bar and scheduled commands) there is
  nobody to answer: a call that has not been allowed fails with `denied`
  straight away, without a prompt.
- Known deviation (to be fixed): uninstalling an extension from Manage
  Extensions does not clear its permission records; installing an extension
  of the same name again carries them over.

### 5.3 Where it is checked

- The runtime attaches the name of the extension that asked to every
  request; the host looks up the table of permissions before it acts.
- The host is the checkpoint, because the host is the one holding the data.

### 5.4 The honest boundary

Today every extension and the runtime are in the same Node.js process, and
an extension can read and write files and start child processes. So:

- What the permission system constrains is **the data Heyo keeps and the
  actions Heyo carries out on an extension's behalf**, not everything an
  extension can do on this computer.
- A malicious extension could in theory forge where a request comes from.
  The permissions of v1 are "informed consent and auditability", not
  security isolation.
- Real isolation takes one process per extension, with the host telling the
  origin by process. This is listed as v2 (see section 11), and the request
  format in this specification already leaves room for it: the origin is
  decided by the channel, not by a field in the message.

The wording in the settings and at install time must agree with this, and
must not describe permissions as a sandbox.

---

## 6. Wire protocol

### 6.1 Requests

Extension requests are named apart from the built-in `host.*` requests,
with the prefix `ext.`, so that the internal interface is free to change.

```jsonc
// runtime → host
{ "type": "request", "id": 41, "method": "ext.windows.list", "extension": "window-sessions", "params": {} }
// host → runtime
{ "type": "reply", "id": 41, "result": [ /* … */ ] }
{ "type": "reply", "id": 41, "error": { "code": "denied", "message": "Window access was not allowed." } }
```

- In Rust this is a new enum, `ExtRequest`
  (`#[serde(tag = "method", content = "params")]`), beside the existing
  `HostRequest`, and its TypeScript types are exported the same way.
- The `error` in existing replies is a string; replies to `ext.*` change it
  to `{ code, message }`. `host.*` stays as it is.

### 6.2 Events

Interfaces that subscribe need the host to send messages of its own accord,
which is a new direction in the protocol:

```jsonc
// runtime → host: start subscribing
{ "type": "request", "id": 42, "method": "ext.windows.watch", "extension": "…", "params": {} }
// host → runtime: the reply gives the subscription number
{ "type": "reply", "id": 42, "result": { "subscription": 7 } }
// host → runtime: the events after that
{ "type": "event", "subscription": 7, "data": { "kind": "focused", "window": { /* … */ } } }
// runtime → host: end
{ "type": "request", "id": 43, "method": "ext.unsubscribe", "params": { "subscription": 7 } }
```

- A subscription belongs to the command session that started it; when the
  session ends the runtime cancels them all, and the host cleans up when the
  runtime exits.
- Streamed results (the interim results of dictation) use the same
  mechanism.

### 6.3 Error codes

| `code` | Meaning | What the extension should do |
|---|---|---|
| `unsupported` | The current session cannot do it | Hide the feature or say why |
| `denied` | The user has not allowed it | Say which permission is needed |
| `cancelled` | The user cancelled part way (for example, left the area selection) | End quietly |
| `unavailable` | A component it needs is not installed (the speech engine, the recognition engine) | Lead the user to the settings |
| `invalid` | The parameters are wrong | Fix the call |
| `failed` | Any other failure; `message` gives the reason | Show it to the user |

---

## 7. The interface

What follows is what `@heyo/api` adds or extends beyond the Raycast part,
with signatures in TypeScript. What is marked "v1" goes into the first
version; the rest comes later.

### 7.1 Environment and abilities (v1)

```ts
export const isHeyo: boolean;
export function has(feature: string): boolean;

/** Every field of Raycast's environment, plus the following. */
export const environment: RaycastEnvironment & {
  heyoVersion: string;
  apiVersion: number;
  platform: "linux" | "windows";
  session: "x11" | "wayland" | "windows" | "unknown";
  offline: boolean; // the user has turned on "nothing leaves this computer"
};

export type Support = "full" | "limited" | "none";

export class HeyoError extends Error {
  code: "unsupported" | "denied" | "cancelled" | "unavailable" | "invalid" | "failed";
}
```

When `offline` is true, an extension should avoid the network; Heyo's own
`AI` will only use the local model. Changed: Heyo does not have this switch
yet, and `offline` is always `false`; to know whether the model is on this
computer, ask `AI.provider()` (7.8).

Changed: every Heyo module of section 7 (`Windows`, `Screen`, `Dictation`,
`Memory`, `ClipboardHistory`, `Files`, `System`, `LiveActivity`) has
`supported(): Promise<Support>`, which is not written out for each one
below. `Memory` and `ClipboardHistory` answer `"limited"` in a session with
no clipboard watching (the history can still be read, it just no longer
grows); `Files` and `System` are always `"full"`; `LiveActivity` is always
`"limited"` for now (7.9). Asking `supported()` needs no permission.

### 7.2 Windows (v1, permission `windows`)

Corresponds to `platform::windows`. The existing built-in command Switch
Windows and the window layouts move to it.

```ts
export interface Window {
  id: number;
  title: string;
  /** The application's window class, for example "firefox". */
  class: string;
  /** The window the user is working in: while the palette is open, the one that was in front before the palette appeared. */
  active: boolean;
  /** Physical pixels. Changed: now provided; undefined when the desktop does not report positions. */
  bounds?: Rect;
  /** The index in monitors(). */
  monitor?: number;
}

export interface Rect { x: number; y: number; width: number; height: number }

export type Layout =
  | "left-half" | "right-half" | "top-half" | "bottom-half"
  | "top-left" | "top-right" | "bottom-left" | "bottom-right"
  | "first-third" | "center-third" | "last-third" | "first-two-thirds" | "last-two-thirds"
  | "almost-maximize" | "maximize" | "center" | "next-display"
  /** Twelfths of the work area: x, y, width, height. */
  | { fraction: [number, number, number, number] };

export const Windows: {
  supported(): Promise<Support>;
  /**
   * Changed: most recently used first, so the first item is the window the
   * user is working in (`active` is true), and the rest are in order of
   * recent use; Heyo's own windows are not included.
   */
  list(): Promise<Window[]>;
  /** The window that was in front before Heyo appeared. */
  active(): Promise<Window | undefined>;
  /** The palette makes way for that window, as with Heyo's own switching. */
  activate(id: number): Promise<void>;
  close(id: number): Promise<void>;
  arrange(id: number, layout: Layout): Promise<void>;
  /** Changed: now provided. An exact position, for "save and restore a layout". */
  setBounds(id: number, bounds: Rect): Promise<void>;
  monitors(): Promise<Rect[]>;
  /** v1.1 (phase F): a window gains focus, opens, closes. Returns a function that cancels. */
  watch(listener: (event: WindowEvent) => void): () => void;
};
```

Notes: `bounds`, `setBounds` and `monitors` are implemented on all three
adapters (measured on X11; on GNOME Wayland and Windows they have only been
compiled). `watch` needs the event channel of 6.2 and is still v1.1. Where
the order comes from: what Switch Windows wants is "back to the previous
one", so it moves the first item (the current window) to the end of the
list; the API itself does not make that decision for a command.

### 7.3 Screen (v1, permissions `screen.capture`, `screen.text`)

Corresponds to `platform::capture` and `ocr.rs`.

```ts
export type CaptureTarget =
  | "selection"          // the user selects an area; reuses the screenshot overlay
  | "screen"             // the monitor the pointer is on
  | { window: number }
  | { rect: Rect };

export interface Capture {
  /** A PNG file in the extension's cache directory; Heyo cleans it up after the command ends. */
  path: string;
  width: number;
  height: number;
}

export interface RecognizedText {
  /** The whole text, joined in reading order. */
  text: string;
  /** v1.1: each line and its position. */
  lines?: { text: string; box: Rect; confidence: number }[];
}

export const Screen: {
  supported(): Promise<Support>;
  capture(target: CaptureTarget): Promise<Capture>;
  /** Fails with "unavailable" when the recognition engine is not installed. */
  recognizeText(image: string | CaptureTarget): Promise<RecognizedText>;
};
```

- `"selection"` is selected by the user's own hand, which is itself
  consent, so nothing more is asked.
- `"screen"`, `{ window }` and `{ rect }` get what is on the screen without
  the user doing anything, and are asked about on first use.

### 7.4 Dictation (v1, permission `microphone`)

Corresponds to `dictation.rs`. There is only one recording at a time; a call
while one is under way fails with `failed`.

```ts
export interface ListenOptions {
  language?: "auto" | "zh" | "en" | "ja" | "ko" | "yue";
  /** Whether to show Heyo's dictation indicator. Shown by default. */
  indicator?: boolean;
  /** Called once for each sentence recognized. */
  onSegment?: (text: string) => void;
  signal?: AbortSignal;
}

export const Dictation: {
  supported(): Promise<Support>;
  installed(): Promise<boolean>;
  /** Starts recording, until stop() of the returned object is called, the user ends it with the hotkey, or signal is aborted. */
  listen(options?: ListenOptions): { stop(): void; result: Promise<string> };
  /** v1.1: transcribes an audio file (16 kHz mono WAV). */
  transcribe(file: string, options?: { language?: string }): Promise<string>;
};
```

The difference from the existing dictation: `listen()` **hands the text to
the extension**, and does not paste it into the frontmost application.

### 7.5 Memory and ClipboardHistory (v1, permissions `memory.read`, `memory.write`, `clipboard.history`)

`Memory` is the search of memory across sources. It corresponds to today's
`clipboard/store.rs` and `clipboard/vectors.rs`, and gains sources as the
later phases of memory search arrive; in v1 the only source is the
clipboard. Changed: `ClipboardHistory` is no longer a second way in for
reading: reading the history is `Memory.search` (while the clipboard is the
only source there is no need to filter by `sources`); what it is left with
is the action only the clipboard has: putting an entry back on the
clipboard. Heyo's own Clipboard History command is written this way
(pinning, deleting, clearing and pausing recording are still private
requests of its own, because that is "keeping the history", not "reading
the history").

```ts
export type MemorySource = "clipboard" | "screenshot" | "dictation" | "file" | "web" | `ext:${string}`;

export interface MemoryItem {
  /** The source plus the number within the source, such as "clipboard:412". */
  id: string;
  source: MemorySource;
  kind: "text" | "link" | "color" | "file" | "image";
  text?: string;
  /** The PNG file of an image entry. */
  image?: string;
  /** Changed: the size of the image in pixels. */
  width?: number;
  height?: number;
  /** Which application it came from (window class). */
  app?: string;
  /** Changed: that application's name (the window class when no installed application matches) and icon file. */
  appName?: string;
  appIcon?: string;
  /** The timestamp of the last copy, in milliseconds. */
  at: number;
  /** Changed: the time of the first copy and the number of copies. The clipboard source counts; other sources may not have them, hence optional. */
  firstAt?: number;
  timesCopied?: number;
  pinned: boolean;
  /** Changed: found by meaning: the words of the query are not in the entry. */
  byMeaning?: boolean;
}

export interface MemoryQuery {
  /** A query in natural language; it may contain a time, a source and a kind ("上周 从 chrome 复制的 链接": links copied from chrome last week). */
  text?: string;
  sources?: MemorySource[];
  kinds?: MemoryItem["kind"][];
  since?: Date;
  until?: Date;
  limit?: number; // 50 by default. Changed: at most 300 (the number of entries Heyo's own history view shows)
  /**
   * Changed: also find entries that mean the same as `text` without
   * containing its words, provided the user has turned on "Find by Meaning"
   * in the settings (without it this option has no effect). The result
   * waits for it: a second or two the first time (loading the model), a few
   * milliseconds after that. What is found this way is marked `byMeaning`.
   */
  byMeaning?: boolean;
}

export interface MemoryResult {
  items: MemoryItem[];
  /** What the query was read as, so that the interface can show "Searching: last week · links". */
  understood: { terms: string[]; app?: string; kind?: MemoryItem["kind"]; from?: number; to?: number };
  /** The conditions that were read out matched nothing, and the search was run again as plain text. */
  relaxed: boolean;
}

export const Memory: {
  supported(): Promise<Support>;
  /** Changed: query may be left out: without `text` it is the most recent. */
  search(query?: MemoryQuery): Promise<MemoryResult>;
  get(id: string): Promise<MemoryItem | undefined>;
  /** Undecided (question 4 of section 12), not implemented in v1: writes the extension's own entry, with the source recorded as `ext:<extension name>`; an extension can only remove what it wrote. */
  add(item: { text: string; kind?: MemoryItem["kind"]; at?: Date; meta?: Record<string, string> }): Promise<string>;
  remove(id: string): Promise<void>;
};

export const ClipboardHistory: {
  supported(): Promise<Support>;
  /** Puts an entry of the history back on the clipboard. */
  copy(id: string): Promise<void>;
};
```

- What was copied marked as secret (`concealed`) never enters the history,
  and so is never returned either.
- `Memory.search` offers no "export everything": a query with no `text` and
  no time range returns at most the `limit` most recent entries.
- Changed: `ClipboardHistory.search(text, { kind, limit })` is gone. It was
  only a subset of `Memory.search({ text, kinds: [kind], limit })`, the
  built-in commands did not use it, and two ways in for reading would only
  give the table of permissions one more row to explain.
  `has("ClipboardHistory.search")` is false.

### 7.6 Files (v1, permission `files.search`)

Corresponds to `files.rs`. It only returns paths and does not read
contents.

```ts
export const Files: {
  supported(): Promise<Support>;
  search(query: string, options?: { limit?: number }): Promise<{
    files: { path: string; isDirectory: boolean }[];
    /** The index is still scanning, and the results may be incomplete. */
    scanning: boolean;
    /** Changed: how many files and directories the index has in all, so that the interface can show "Searching 183,204 items". */
    total: number;
  }>;
};
```

### 7.7 System and Clipboard (v1, permissions `system`, `input.paste`)

Corresponds to `platform/system.rs` and `platform/input.rs`.

```ts
export type SystemAction =
  | "lock-screen" | "sleep" | "restart" | "shut-down" | "log-out"
  | "empty-trash" | "toggle-dark-mode" | "volume-up" | "volume-down" | "toggle-mute";

/** Changed: a speaker or a microphone. */
export interface AudioDevice {
  /** Its name in the sound server; useAudioDevice takes it. */
  name: string;
  /** The description shown to the user. */
  description: string;
  kind: "output" | "input";
  /** Whether sound goes through it now. */
  current: boolean;
}

export const System: {
  supported(): Promise<Support>;
  /** The return value is a sentence that can be shown to the user ("Volume 40%"), or undefined when there is none. */
  run(action: SystemAction): Promise<string | undefined>;
  appearance(): Promise<"light" | "dark">;
  /** A brief desktop notification. */
  notify(title: string, body?: string): Promise<void>;
  /** Changed: the speakers and microphones, with the one in use marked for each. */
  audioDevices(): Promise<AudioDevice[]>;
  /** Changed: switches which device sound goes through, by `name`. */
  useAudioDevice(kind: AudioDevice["kind"], name: string): Promise<void>;
};
```

`restart`, `shut-down` and `log-out` end the session, so the host shows a
confirmation that an extension cannot get around: it asks every time and
does not remember the answer; when the user chooses "No" the call fails with
`cancelled`. Changed: Heyo's own system commands have already asked once in
their own words, and the host does not ask a second time.
`appearance` and `notify` need the `system` permission but are not asked
about (read-only, harmless).
Changed: audio devices used to be done by Heyo's own commands calling
`pactl` directly; phase E took them into the host and opened them to
extensions. Ending a process was not taken in (it is not in the table of
permissions and is still a private request).

For pasting and for reading the selected text Raycast already has
interfaces (`Clipboard.paste`, `getSelectedText`). No new names are made;
the original only gains an optional parameter:

```ts
export const Clipboard: RaycastClipboard & {
  /** Raycast's signature is unchanged; one more option: after pasting, restore the clipboard to what it held before. */
  paste(content: string | Clipboard.Content, options?: { restoreClipboard?: boolean }): Promise<void>;
};
```

Pasting in a terminal uses Ctrl+Shift+V; the host works that out, and an
extension does not need to know.

### 7.8 AI (v1, permission `ai`)

Extends Raycast's `AI`. The signature and the return value of
`AI.ask(prompt, { model, creativity, signal })` (a Promise that can also
`.on("data", …)`) stay unchanged. Changed: Heyo's own Ask AI, AI Chat and
selected-text actions are written with this very `ask` (under the identity
`@heyo`, see 5.4 and `permissions::identity`), and the runtime no longer has
another road to the model.

```ts
export const AI: RaycastAI & {
  /** The current configuration: whose model it is and whether it is local. For an extension to decide whether to send sensitive content. */
  provider(): Promise<{ kind: "claude" | "local" | "claude-code" | "codex"; model: string; local: boolean }>;
  ask(prompt: string, options?: RaycastAI.AskOptions & {
    /** Replaces the default instructions Heyo gives extensions ("You are a helpful assistant inside a desktop launcher extension…"). */
    system?: string;
    /** Paths of image files (PNG, JPEG, GIF, WebP); fails with "unsupported" when the model does not take images. */
    images?: string[];
    /**
     * Changed: the conversation so far, earliest first, question and answer
     * alternating; prompt is the user's next line. If they do not alternate,
     * or do not end with an answer, it fails with "invalid".
     */
    messages?: AI.Message[];
    /**
     * Changed: prompt is text to work on (translate, rewrite…) and not
     * something to answer: it is fenced off and treated as material, and the
     * model does what `system` says and does not obey what the material says
     * (small local models need this layer in particular). Cannot be used
     * together with `messages`.
     */
    material?: boolean;
    /** Changed: gets the whole answer so far each time there is more (`on("data")` gives the increments). */
    onText?: (text: string) => void;
  }): Promise<string> & { on(event: "data", listener: (chunk: string) => void): void };
};

export namespace AI {
  /** Changed: an earlier question or answer of the conversation. */
  interface Message { role: "user" | "assistant"; content: string }
}
```

- Changed (implemented): `creativity` maps to the local model's temperature
  (`none` 0, `low` 0.5, `medium` 1, `high` 1.5, `maximum` 2, or a number
  from 0 to 2); Claude is sent no temperature. `model` is accepted, but the
  call still runs with the model the user chose in the settings, because
  Raycast's model names have no counterpart here. This is a documented
  degradation.
- `provider().kind` is `claude-code` or `codex` when the user has chosen
  the model of the command-line tool Claude Code or Codex on this computer,
  signed in with their own account: `model` is the one set for it (empty for
  the tool's default), and `local` is false, since the tool sends what it is
  asked to Anthropic or OpenAI. Both take `images`; neither has a
  temperature, and an answer from Codex comes whole rather than as it is
  written.
- `images` makes "take a screenshot, then ask" two lines:
  `Screen.capture("selection")`, then
  `AI.ask(question, { images: [capture.path] })`.
- Changed: the three ways of using it correspond to three shapes of what is
  sent to the model. Only `prompt` (or with `system`): one user message;
  `messages`: the earlier turns go first as they are, and `prompt` is the
  new one, which is how AI Chat hands the whole conversation to the model;
  `material`: `prompt` is not a message but material fenced in `<text>`,
  and `system` says what to do with it, which is how the selected-text
  actions do it.
- Changed: every failure is a `HeyoError`: aborting with `signal` is
  `cancelled`; wrong parameters are `invalid`; a local model that does not
  take images is `unsupported`; the rest are `failed`, and `message` is a
  sentence for the user ("No Claude API key is set. Add one in Heyo
  Settings → Advanced."). Raycast's `ask` does not specify the shape of a
  failure, so this does not break compatibility.
- Changed: the rhythm of streaming is the same as for Heyo's own commands:
  each time the model puts out a piece, `onText` and `on("data")` are each
  called once; when the interface redraws is up to the caller (AI Chat
  takes it every 80 milliseconds, and in a long conversation only the
  answer being written is sent again).
- After the palette closes, the answer goes on being generated and is
  written into the conversation (this is the behavior of Ask AI and AI
  Chat, not something the API specifies; an undecided item outside
  section 12, see `TODO.md`).
- Outside Heyo (`packages/api/index.js`): `system`, a transcript of
  `messages` and the fence of `material` are all put before the prompt and
  handed to Raycast's `ask`; `onText` adds up what `on("data")` gives;
  `images` are refused with `unsupported`.

### 7.9 LiveActivity (v1, permission `activity`)

Changed, new. Corresponds to `activities.rs`. While a command is not in the
palette, Heyo shows "what is in progress" for it: in the palette's "In
Progress" section and (when the desktop supports it) in the status bar.
Timers and progress that advances with time are counted by Heyo itself, so
the command does not have to keep running.
Where the design comes from, how each desktop presents it and what the
limits rest on are in `docs/live-activities.md` (the interface is its
section 6); here only the interface is written down.

```ts
export namespace LiveActivity {
  /** The part that does not change. */
  interface Attributes {
    /** Unique within one extension. Calling start again with the same id is an update. */
    id: string;
    /** What this is ("Build", "Timer"). */
    name: string;
    icon: Image.ImageLike;
    tint?: Color.ColorLike;
    /** What to do when the activity itself is chosen. */
    action?: Action;
    /** Whether the user can end it. true by default. */
    dismissible?: boolean;
    /** What to do when it cannot be shown on this desktop. "notify" by default. */
    fallback?: "notify" | "drop";
  }

  /** The part that changes. */
  interface Content {
    /** What the collapsed state shows. When left out, it is title, truncated. */
    compact?: Value;
    title: string;
    subtitle?: string;
    /** The body of the expanded state, one of three. */
    body?: { progress: Progress } | { timer: Timer } | { text: string };
    /** At most three; they appear in the expanded state and in the palette's action panel. */
    actions?: Button[];
    /** Needs the user to deal with it (confirm, answer): moved to the front and marked. */
    needsAttention?: boolean;
    /** After this the content may be inaccurate, and it is shown faded. It does not end the activity. */
    staleAt?: Date;
    /** The order among several activities of the same extension, 0–100. */
    relevance?: number;
  }

  /** A small piece of content that changes by itself. */
  type Value = string | { timer: Timer } | { progress: Progress } | { icon: Image.ImageLike };

  /** A span of time plus a moment of pausing, counted by Heyo. */
  interface Timer {
    start: Date;
    /** Left out, it counts up and has no end. */
    end?: Date;
    /** The moment it was paused at; left out, it is running. */
    pausedAt?: Date;
    /** With an end it counts down by default. */
    countsDown?: boolean;
    /** "12:05" or "12 min". */
    style?: "clock" | "relative";
  }

  interface Progress {
    /** 0–1. Left out, and with no interval, the progress is indeterminate. */
    value?: number;
    /** Advances by itself with time. */
    interval?: Pick<Timer, "start" | "end" | "pausedAt">;
  }

  interface Button {
    title: string;
    icon?: Image.ImageLike;
    style?: "regular" | "destructive";
    action: Action;
  }

  /** Heyo carries out the first three directly; `event` is sent back to the extension. */
  type Action =
    | { open: string }
    | { copy: string }
    | { launch: { command: string; arguments?: Record<string, string>; context?: unknown } }
    | { event: string; data?: unknown }
    | { end: true };

  interface StartOptions {
    /** Shown briefly once when it appears. true by default. */
    announce?: boolean;
  }

  interface Alert {
    /** Shows this update briefly. */
    expand?: boolean;
  }

  interface Handle {
    readonly id: string;
    /** Once a second per activity; what exceeds that is merged. */
    update(content: Partial<Content>, options?: { alert?: Alert }): Promise<void>;
    /** The final content is shown for a moment ("brief", the default) or disappears at once ("none"). */
    end(final?: Partial<Content>, options?: { keep?: "none" | "brief" }): Promise<void>;
  }

  type Event =
    | { kind: "action"; id: string; event: string; data?: unknown }
    | { kind: "dismissed"; id: string }
    | { kind: "expired"; id: string }
    | { kind: "elapsed"; id: string; at: number };

  interface Presentation {
    /** This is about the status bar; the section in the palette is always there. */
    support: Support;
    /** How many characters fit in the collapsed state. */
    compactCharacters: number;
    /** Whether the expanded state is shown, and whether its buttons are. */
    expanded: boolean;
    buttons: boolean;
    /** The version of the layouts, raised when layouts are added later. */
    version: number;
  }
}

export const LiveActivity: {
  supported(): Promise<Support>;
  /** Starts an activity, or updates the one with the same id. */
  start(attributes: LiveActivity.Attributes, content: LiveActivity.Content, options?: LiveActivity.StartOptions): Promise<LiveActivity.Handle>;
  /** The activities this extension has in progress. */
  list(): Promise<LiveActivity.Handle[]>;
  get(id: string): Promise<LiveActivity.Handle | undefined>;
  /** What an activity looks like on this desktop, so that an extension can adjust its content to it. */
  presentation(): Promise<LiveActivity.Presentation>;
  /** Receives the events of this extension's activities while the command runs. Returns a function that cancels. */
  onEvent(listener: (event: LiveActivity.Event) => void): Promise<() => void>;
};
```

The request each function sends and the permission it needs (taken from the
declaration in `runtime/src/heyo/members.ts` and the host's `needed`):

| Function | Request | Permission |
|---|---|---|
| `supported()` | `ext.supported` (`module: "activity"`) | none |
| `start` | `ext.activity.start` | `activity` |
| `list` | `ext.activity.list` | `activity` |
| `get` | `ext.activity.get` | `activity` |
| `presentation` | `ext.activity.presentation` | none |
| `onEvent` | `ext.activity.onEvent` (a subscription, see 6.2) | `activity` |
| `Handle.update`, `Handle.end` | `ext.activity.update`, `ext.activity.end` | `activity` |

- **Permission**: listing `activity` in the manifest is enough, and nothing
  is asked on first use. The user can turn it off per extension in the
  settings, or turn it off altogether; when it is off the call fails with
  `denied`, and the same goes for Heyo's own activities. When the permission
  is not listed, the host's refusal names what the author called:
  `LiveActivity.start needs the "activity" permission; add it to "heyo.permissions" in package.json.`
- **Who can start one**: only a command the user ran, or the handler
  command that is run after the user chose a button on an activity. A
  command running in the background on a schedule that calls `start` for a
  new activity fails with `denied`; updating an activity it already has is
  not restricted. The runtime writes how the command was started
  (`launchType`) into the request; the extension does not pass it.
- **Limits**: 3 activities at a time per extension, 4 KB for attributes
  plus content, 3 buttons, and beyond that the call fails with `invalid`;
  the collapsed state holds about 10 characters and what is beyond that is
  truncated (for the exact value ask `presentation().compactCharacters`).
- **Order**: dates on the wire are milliseconds. The runtime gives every
  `start` and `update` a sequence number that only grows, and the host
  drops an update whose number is not greater than the one already
  applied, so when the command that started an activity and the command
  that handles its buttons update at the same time, the older does not
  overwrite the newer. Wherever a handle is called, it sends its requests in
  the name of the command that created it.
- **Events**: a running command subscribes with `onEvent`; when no command
  is listening, the host runs in the background the command the manifest
  marks `"heyo": { "handles": "activity" }`, and the event is in its
  `launchContext.activity`. `elapsed` is a countdown reaching its end.
- **As it is now**: `supported()` answers `"limited"`, and `presentation()`
  answers `support: "none"` with `expanded` and `buttons` false: activities
  are shown in the palette's "In Progress" section, and what the user needs
  to know is sent as a notification as well; the status bar is not drawn
  yet.
- Outside Heyo (`packages/api/index.js`): `supported()` answers `"none"`,
  and the rest are refused with `unsupported`.

### 7.10 Root search result providers (v1.1, permission `search.provider`)

An ability Raycast does not have: an extension can make its own items appear
in the root search without being opened.

The manifest declares a command of mode `provider`:

```json
{ "name": "issues", "title": "Issues", "mode": "provider", "heyo": { "debounce": 120, "minLength": 2 } }
```

The command file's default export is a function:

```ts
import type { SearchProvider } from "@heyo/api";

const provider: SearchProvider = async (query, { signal }) => [
  {
    id: "HEYO-12",
    title: "Crash when the palette is toggled twice",
    subtitle: "HEYO-12 · open",
    icon: "bug-16",
    /** 0–100; on the same scale as the fuzzy-match scores of built-in items. */
    score: 70,
    action: { open: "https://example.com/HEYO-12" },
  },
];
export default provider;
```

The constraints, all of them so as not to slow the root search down:

- Providers run in the background runtime process, and they and the command
  in the palette do not affect each other.
- A keystroke never waits for providers: the built-in results are shown
  first, a provider's results are merged in if they arrive within 250 ms,
  and are dropped if they are later.
- When a new query arrives, the `signal` of the previous one is aborted.
- At most 5 items per provider per query; the items go in a section named
  after the extension's title, after the built-in results unless their
  score is higher.
- A provider that times out or throws 5 times in a row is disabled for this
  session, and marked in the settings.
- `action` can only be: opening a link or a file, copying text, pasting
  text, or launching another command of this extension (with arguments if
  wanted). It cannot be an arbitrary function, because the result has to be
  passed across processes to the interface.

### 7.11 Later (not in v1)

| Interface | Use | Depends on |
|---|---|---|
| `Clipboard.watch` | Triggering an extension on a copy | The event channel of 6.2, the background process |
| `Hotkeys.onHold` | An extension's own interaction in the manner of hold-to-talk | The design of permissions and of handling conflicts |
| `Overlay` | An extension drawing on the screen (a ruler, a color picker) | A new kind of window |
| `Browser.tabs` | Searching tabs | A companion browser extension |

---

## 8. Coverage of Raycast's API

"Migrating takes changing one import" requires the Raycast part to have no
gaps. The table below is the present state, put together from what
`runtime/src/api/` implements now. It is at the level of exports; whether
every property of every component is implemented I have not checked one by
one, and that is exactly what the conformance tests of 8.3 are to answer
automatically.

### 8.1 Present state

| Export | Present state | What is to be filled in |
|---|---|---|
| `List`, `Grid`, `Detail`, `Form`, `ActionPanel`, `MenuBarExtra`, `useNavigation` | Implemented | Checking at the level of properties (8.3) |
| `Action` and `CopyToClipboard`, `Paste`, `Open`, `OpenInBrowser`, `OpenWith`, `ShowInFinder`, `Trash`, `Push`, `SubmitForm` | Implemented | `OpenWith` does not yet let the user choose an application |
| `Action.CreateSnippet`, `Action.CreateQuicklink` | Implemented. Changed: opens Heyo's own Create Snippet and Create Quicklink forms, filled in with what was passed | A quicklink's `application` and `icon` have nowhere to go (it always opens with the default application) |
| `Action.PickDate` | Implemented. Changed: the date is picked in the action panel | — |
| `Action.ToggleQuickLook` | Implemented. Changed: images and text are shown in a view pushed over the list | Files of other types are handed to the desktop's default application |
| `showToast`, `Toast`, `showHUD`, `confirmAlert`, `Alert` | Implemented | The options of `showHUD` are ignored |
| `Clipboard` (`copy`, `paste`, `clear`, `read`, `readText`) | Implemented | — |
| `LocalStorage`, `Cache` | Implemented | — |
| `getPreferenceValues`, `openExtensionPreferences`, `openCommandPreferences` | Implemented | — |
| `OAuth.PKCEClient` | Implemented | — |
| `open`, `trash`, `showInFinder`, `getApplications`, `getDefaultApplication` | Implemented | The parameter of `getApplications(path)` is ignored |
| `getSelectedText` | Implemented | — |
| `closeMainWindow`, `popToRoot`, `clearSearchBar` | Implemented | The options (`clearRootSearch`, `popToRootType`) are ignored |
| `launchCommand` | Implemented. Changed: it can also launch commands of other installed extensions, with the user's consent the first time (asked once in the palette and remembered per caller; in the menu bar and the background there is nobody to ask, and it fails without consent) | A command with a view cannot be launched in the background; only the command in the palette, or a menu bar command whose menu item the user chose, can open another command into the palette |
| `environment` | Implemented. Changed: `appearance` follows the light or dark of Heyo's interface (the host tells the runtime when it starts, and it is updated as the setting changes) | `raycastVersion` is a fixed string; `textSize` is always `"medium"`; `launchContext` and the three `entryPoint*` fields are not there yet (8.3) |
| `AI.ask` | Implemented (changed: including `creativity` → temperature, and the `system`, `messages`, `images`, `material` and `onText` of 7.8) | `model` runs with the model in the settings; see 7.8 |
| `Icon`, `Color`, `Image`, `Keyboard` | Implemented | The table of icons is updated with Raycast's versions |
| `getFrontmostApplication` | Implemented (2026-10-10) | When no installed application matches, only the window class name is returned |
| `WindowManagement` (4 functions) | Implemented (2026-10-10), measured on X11 | A "desktop" is counted per monitor; `"fullscreen"` is maximize; the unit is physical pixels; a window's `application` field is not filled in |
| `updateCommandMetadata` | Does nothing | Have the root search show a command's dynamic subtitle |
| `getSelectedFinderItems` | Returns an empty array | The files selected in the file manager; on GNOME this has to go through the Shell extension, and other sessions return empty |
| `BrowserExtension` | Throws | Needs a companion browser extension (7.11) |
| `Tool` (confirming the tools of AI extensions) | Not there | Depends on whether Raycast's AI extensions are supported |
| Deprecated exports (`copyTextToClipboard`, the old Action components, `getLocalStorageItem` and so on) | Implemented. Changed: filled in as aliases of the existing implementation (8.3) | — |

### 8.2 Where the same behavior is not possible

The following are macOS concepts; on Heyo each has a definite degradation,
written into the documentation:

| Interface | Behavior on Heyo |
|---|---|
| `Application.bundleId` | The name of the desktop file (Linux) or the path of the program (Windows) |
| `showInFinder` | Opens the containing directory in the file manager |
| The title of `Action.ShowInFinder` | "Show in Files" |
| `cmd` in `Keyboard.Shortcut` | Mapped to Ctrl (already so) |
| `environment.raycastVersion` | A fixed version number, higher than the minimum extensions commonly require |
| The `model` of `AI.ask` | Ignored; the model in the settings is used |
| `getDesktops` | Each monitor counts as one desktop; there are no fullscreen desktops |
| `"fullscreen"` in `setWindowBounds` | Maximize |
| The position and size of a window | Physical pixels (points on macOS) |

### 8.3 Conformance tests

**Present state (2026-10-10, against `@raycast/api` 2.7.2)**: the test at
the level of names is written and passes, in
`runtime/src/api/conformance.test.ts`. It compares the names of every export
and of its members, three levels deep, and does not compare function
signatures or component properties. Of the gaps the first run found, the
old aliases (`CopyToClipboardAction`, `FormTextField`,
`getLocalStorageItem` and others, 37 in all), the values of `AI.Model` and
`Color.Brown` have been filled in. The 16 names still missing are recorded
in `conformance.gaps.json`, in three groups:

- The old-style entry point `render()` (1).
- What Raycast marks as unstable or for internal use (7): `unstable_AI`,
  `useUnstableAI`, `AI.experimental_decide`, `AI.refreshModels`,
  `captureMemorySnapshot`, `specialKeys`, `useActionPanel`.
- Newer ones that have not been studied yet (8):
  `environment.launchContext` and the three `entryPoint*` fields, OAuth by
  client metadata document, and two static constants of `Cache`.

Note that a name being there does not mean the behavior is complete: the
names that "throw" or "say it is not supported" in 8.1 are all there, so
they are not among these 16.

**Component properties**: a second test, `runtime/src/api/props.test.ts`,
takes the property names of every Raycast component from the types and
checks whether each has ever appeared in the code that renders the
component. A property that never appears is certainly ignored. After the
first run three things were fixed (the old property names `enableFiltering`,
`accessoryTitle` and `accessoryIcon`, and the `transient` of
`Action.CopyToClipboard`), and the rest were recorded in `props.gaps.json`.
Changed: only three are left now: `Form.enableDrafts`,
`Form.TextArea.enableMarkdown` and `MenuBarExtra.Item.alternate`;
`storeValue`, the options of `Form.FilePicker`, `quickLook` and
`Action.CreateSnippet.snippet`, which were recorded at first, have since
been connected and are no longer in the list.

This test can only find the properties that are "certainly not handled". A
property whose name does appear may still be handled only in part; that
takes comparing component by component, and cannot be counted.

Comparing function signatures has not been done yet:

- The real `@raycast/api` is installed as a development dependency, and
  only its type declarations are used.
- A type test asserts that our implementation can be assigned to it:
  `const check: typeof import("@raycast/api") = heyoApi;`. One export too
  few, one function too few, or a wrong parameter, and `tsc` fails. The
  properties of components are compared the same way, component by
  component.
- What degrades and what is missing for now is listed in an allowlist, and
  every item in the allowlist must correspond to a row of 8.1 or 8.2.
- When Raycast releases a new version this development dependency is
  upgraded, and the places where the test fails are the new interfaces.

### 8.4 Permissions

The Raycast part of the API does not require the `heyo` manifest field,
otherwise none of the store's extensions could run. The permissions it maps
to (`WindowManagement` → `windows`) are asked about on first use all the
same, so that an extension without a manifest cannot get around the user.

---

## 9. Developer experience

- `heyo migrate <dir>`: changes the imports of `@raycast/api` to
  `@heyo/api`, and adds the `heyo` manifest field.
- `heyo build <dir>`: builds the extension, producing the same as
  `ray build` (one `.js` per command, with the API and React left for the
  runtime to provide). Whether Raycast's own build tool `ray` runs on Linux
  I have not verified; with `heyo build` nothing depends on it.
- `heyo dev <dir>`: watches and builds the extension, loads it into the
  running Heyo, and shows runtime errors in the terminal.
- `heyo ext check <dir>`: checks the manifest, `apiVersion`, and whether
  the declared permissions cover the interfaces the code uses; and finds
  the porting problems outside the API (calls to macOS programs,
  AppleScript, hard-coded macOS paths).
- Error messages meant for extension authors include the name of the
  interface and the permission needed, for example
  `Windows.list needs the "windows" permission; add it to "heyo.permissions" in package.json.`
- Example extensions go in `examples/`, one per module, and are also what
  the end-to-end tests run on.

---

## 10. Testing

| Layer | How it is tested |
|---|---|
| Request format | Unit tests in `wire.rs`: every `ext.*` is read in, unknown methods are refused |
| Permission decisions | A pure function `permitted(extension, permission, grants) -> Decision`, with table-driven tests |
| Platform behavior | With the existing `desktop::fake::Fake`, asserting what the host asked of the desktop |
| Raycast conformance | The type test of 8.3, run in `tsc` |
| The runtime shim | vitest: the request each function of `@heyo/api` sends agrees with the types; the behavior of the stubs outside Heyo |
| Migration | Take a few store extensions, run `heyo migrate` and `heyo build`, and assert the behavior is the same before and after the imports change |
| End to end | `scripts/gui-test/` loads the extensions of `examples/` and runs them in the sandbox |
| Provider time limits | A deliberately slow provider, asserting that the root search is not slowed down and that it is disabled after several timeouts |

---

## 11. Implementation plan

Changed: A–E are done (2026-10-10 to 10-11), and the table gained a
"Status" column.

| Phase | Status | What | Files involved |
|---|---|---|---|
| A | Done | The `ExtRequest` enum, `{ code, message }` errors, requests carrying the extension's name and directory | `wire.rs`, `ext.rs`, `runtime.rs`, `runtime/src/protocol.ts` |
| B | Done | Permissions: reading the manifest, the deciding function `decide`, the prompt, storage, the settings page; a refusal is remembered for 60 seconds (5.2) | `permissions.rs`, `extensions.rs`, `src/Settings.tsx`, `src/App.tsx` |
| C | Done | The `@heyo/api` shim and the types package; both package names point to the same object in the `provided` table; the type test `types.check.ts` fastens `index.d.ts` and the runtime together | `runtime/src/heyo/`, `runtime/src/runtime.ts`, `packages/api/` |
| C2 | Done | The Raycast conformance tests (8.3), and filling in the gaps of 8.1 by them | `runtime/src/api/conformance.test.ts`, `props.test.ts` |
| C3 | Done | `heyo build`, `heyo migrate`, `heyo ext check` | `packages/cli/` |
| D | Done | The v1 modules: Windows, Screen, Dictation, Memory, Files, System, Input, AI | `ext.rs` calling the existing `platform::*`, `clipboard`, `files`, `ocr`, `dictation` |
| E | Done | The built-in commands move to `@heyo/api`: switching windows, the system commands, audio devices, file search, reading and copying from the clipboard history, with 15 `host.*` requests deleted; v0.3 also moved Ask AI, AI Chat and the selected-text actions to `AI.ask` | `runtime/src/builtins/*`, `host.rs` |
| F | Not done | The event channel (6.2): `Windows.watch`, and every interface in 7.11 where "the host sends messages of its own accord". `bounds` and `setBounds` were done along with D and no longer belong to F | `wire.rs`, `platform/desktop.rs` and the three adapters, the GNOME extension, `runtime/src/heyo/events.ts` (the runtime's side of subscriptions is already there) |
| G | Not done | Root search providers (7.10): the `provider` command mode, the `search.provider` permission, time limits and disabling in the background process | `root.rs`, `runtime/src/background.ts`, `src/RootSearch.tsx` |
| H (v2) | Not done | One process per extension, with the host telling the origin by process; only then do permissions become a boundary (5.4). "The origin is decided by the channel" of `ExtCaller` already leaves room for this | `runtime.rs`, `helper.rs` |

A–E (with C2 and C3) are v1; F–G are v1.1. Phase E proved the design: where
the built-in commands were easy to write with this API it stayed as it was,
and where they were not, the API was changed (the fields of `MemoryItem`,
`byMeaning`, `total`, audio devices, the `messages` and `material` of
`AI.ask`); this version of the specification is the result of that list.
What was not carried over into the API: pinning, deleting and clearing the
history, ending a process, saving conversations. They are "keeping", not
"reading", and stay as private requests of Heyo's.

---

## 12. Open questions

1. **The package name**: `@heyo/api` needs the `@heyo` organization to be
   registered on npm, and I have not verified whether it is available. The
   alternative is `heyo-api`. Changed: the package is not published yet
   (`"private": true`), so there is no hurry.
2. **Store extensions without a `heyo` manifest**: when they use windows
   through Raycast's API, are they asked the first time (what this document
   chooses), or allowed by default so as to interrupt less? Changed: the
   implementation now asks the first time (`Route::Raycast`, 8.4).
3. **`search.provider` sees everything typed**: is consent once at install
   time acceptable, or must the user turn each one on in the settings? (To
   be settled before phase G.)
4. **`Memory.add`**: is writing opened in v1 already? It lets an extension
   put its own data into the unified search, but it also means that what is
   in the index is no longer all decided by Heyo. Changed: v1 does not
   implement it, and `has("Memory.add")` is false.
5. ~~**"Only This Once" for permissions**~~ Changed: answered by the
   implementation: there is only "Allow" and "Deny", and a refusal is
   remembered for 60 seconds (5.2). To go back to three choices, `Decision`
   in `permissions.rs` would need one more variant.
6. **Bringing process isolation (phase H) forward to v1**: the cost is the
   memory of one more Node.js process per extension (I cannot estimate the
   actual figure before measuring), and the gain is that permissions become
   a real boundary.
7. ~~**The types of the Raycast part**~~ Changed: answered by the
   implementation: re-export (`export * from "@raycast/api"` in
   `packages/api/index.d.ts`, with `@raycast/api` as a peerDependency).
8. **Raycast's AI extensions (`Tool`)**: whether to support them in v1.
   They are a newer kind of Raycast extension, and without them "every
   ability" falls somewhat short.
9. **`BrowserExtension`**: it can only be implemented if we publish a
   browser extension of our own. Does it go into v1, or is it listed as a
   known gap? Changed: it is a known gap now, there by name and throwing
   when called.
10. Changed, new: **the `material` of `AI.ask`**. It was added for the
    selected-text actions (a small local model will obey instructions in
    the material, and needs the fence and the task repeated), and
    extensions will meet the same problem. But Raycast has no counterpart,
    and in Raycast it can only degrade to putting a fence before and after
    the prompt. Keep it, or let extensions put the fence together
    themselves?
