// The types of `@heyo/api`: everything of `@raycast/api`, and what Heyo adds.
//
// Inside Heyo the module is provided by Heyo itself (and `@raycast/api` is
// the same module). Outside it, `index.js` beside this file passes
// Raycast's API through and makes Heyo's own part answer "not here".
//
// These declarations are held to what Heyo provides by a type test in its
// repository (`runtime/src/heyo/types.check.ts`).

import * as raycast from "@raycast/api";

export * from "@raycast/api";

// --- The environment, and asking what there is ------------------------------

/** True in Heyo, false anywhere else (in Raycast). */
export declare const isHeyo: boolean;

/**
 * Whether a part of the API is there and does something, by its name as
 * imported: `has("Screen.recognizeText")`. Outside Heyo nothing of Heyo's is.
 */
export declare function has(feature: string): boolean;

/** How well the session supports something. */
export type Support = "full" | "limited" | "none";

/** Raycast's `environment`, and what Heyo says of itself. */
export declare const environment: raycast.Environment & {
  /** For logs and reports; ask `has()` what there is, not this. Empty outside Heyo. */
  heyoVersion: string;
  /** The version of this API that is provided; 0 outside Heyo. */
  apiVersion: number;
  /** `"macos"` only outside Heyo. */
  platform: "linux" | "windows" | "macos";
  session: "x11" | "wayland" | "windows" | "unknown";
  /** The user wants nothing to leave this computer. (Heyo has no such switch yet: false.) */
  offline: boolean;
};

export type HeyoErrorCode =
  /** This session cannot do it, or this is not Heyo. Hide the feature, or say why. */
  | "unsupported"
  /** The user has not allowed it. Say which permission is needed. */
  | "denied"
  /** The user called it off. End quietly. */
  | "cancelled"
  /** Something it needs is not installed (the speech or text-recognition engine). Point to the settings. */
  | "unavailable"
  /** The call was wrong. */
  | "invalid"
  /** Anything else; `message` says what. */
  | "failed";

/** How every call of Heyo's part of the API fails. */
export declare class HeyoError extends Error {
  code: HeyoErrorCode;
  constructor(code: HeyoErrorCode, message?: string);
}

// --- Windows (permission "windows") --------------------------------------------

/** Physical pixels. */
export interface Rect {
  x: number;
  y: number;
  width: number;
  height: number;
}

/** A window of another application. */
export interface Window {
  id: number;
  title: string;
  /** The application's window class, for example "firefox". */
  class: string;
  /** Whether it is the window the user is working in. */
  active: boolean;
  /** Absent where the desktop does not say. */
  bounds?: Rect;
  /** Which of `Windows.monitors()` it is on. */
  monitor?: number;
}

export type Layout =
  | "left-half"
  | "right-half"
  | "top-half"
  | "bottom-half"
  | "top-left"
  | "top-right"
  | "bottom-left"
  | "bottom-right"
  | "first-third"
  | "center-third"
  | "last-third"
  | "first-two-thirds"
  | "last-two-thirds"
  | "almost-maximize"
  | "maximize"
  | "center"
  | "next-display"
  /** A part of the work area in twelfths: x, y, width, height. */
  | { fraction: [number, number, number, number] };

export declare const Windows: {
  supported(): Promise<Support>;
  /** The most recently used first; none of Heyo's own. */
  list(): Promise<Window[]>;
  /** The window the user is working in: with Heyo's palette up, the one that was in front before it. */
  active(): Promise<Window | undefined>;
  activate(id: number): Promise<void>;
  close(id: number): Promise<void>;
  arrange(id: number, layout: Layout): Promise<void>;
  /** Puts the window's frame exactly there. */
  setBounds(id: number, bounds: Rect): Promise<void>;
  monitors(): Promise<Rect[]>;
};

// --- Screen (permissions "screen.capture", "screen.text") ----------------------

export type CaptureTarget =
  /** An area the user selects. Needs no asking: selecting is the user's leave. */
  | "selection"
  /** The monitor the pointer is on. */
  | "screen"
  /** What is on screen where that window is. */
  | { window: number }
  | { rect: Rect };

export interface Capture {
  /** A PNG file in the extension's cache directory. Heyo removes it when the command has ended. */
  path: string;
  width: number;
  height: number;
}

export interface RecognizedText {
  /** All of it, in reading order. */
  text: string;
}

export declare const Screen: {
  supported(): Promise<Support>;
  /** Fails with "cancelled" when the user gives up a selection. */
  capture(target: CaptureTarget): Promise<Capture>;
  /** The text in an image file, or in what `capture` would take. Fails with "unavailable" until the engine is downloaded. */
  recognizeText(image: string | CaptureTarget): Promise<RecognizedText>;
};

// --- Dictation (permission "microphone") ---------------------------------------

export interface ListenOptions {
  language?: "auto" | "zh" | "en" | "ja" | "ko" | "yue";
  /** Whether Heyo's dictation window shows. It does unless this is false. */
  indicator?: boolean;
  /** Called with each sentence as soon as it is recognized. */
  onSegment?: (text: string) => void;
  /** Aborting discards the recording; `result` then fails with "cancelled". */
  signal?: AbortSignal;
}

export declare const Dictation: {
  supported(): Promise<Support>;
  /** Whether the speech engine has been downloaded. */
  installed(): Promise<boolean>;
  /**
   * Starts recording, until `stop()` is called, the user ends it, or
   * `signal` aborts. The text comes to the extension; nothing is pasted.
   * One recording at a time: while another is going, `result` fails with "failed".
   */
  listen(options?: ListenOptions): { stop(): void; result: Promise<string> };
};

// --- Memory and the clipboard history ------------------------------------------
// (permissions "memory.read", "clipboard.history")

/** Where something remembered came from. So far only the clipboard. */
export type MemorySource = "clipboard" | "screenshot" | "dictation" | "file" | "web" | `ext:${string}`;

export interface MemoryItem {
  id: string;
  source: MemorySource;
  kind: "text" | "link" | "color" | "file" | "image";
  text?: string;
  /** The PNG file of an image. */
  image?: string;
  /** The size of an image, in pixels. */
  width?: number;
  height?: number;
  /** The window class of the application it came from. */
  app?: string;
  /** That application's name (the window class when nothing installed has it) and the file of its icon. */
  appName?: string;
  appIcon?: string;
  /** When it was last copied: milliseconds since the Unix epoch. */
  at: number;
  /** When it was first copied, and how many times it has been: the clipboard history counts; another source may not. */
  firstAt?: number;
  timesCopied?: number;
  pinned: boolean;
  /** It was found by what it means: the words asked for are not in it. */
  byMeaning?: boolean;
}

export interface MemoryQuery {
  /** Words to find; may name a time, a source or a kind ("links from firefox last week"). */
  text?: string;
  sources?: MemorySource[];
  kinds?: MemoryItem["kind"][];
  since?: Date;
  until?: Date;
  /** 50 unless given, 300 at most. */
  limit?: number;
  /**
   * Also find what means the same as `text` without containing its words,
   * where the user has turned finding by meaning on (elsewhere this changes
   * nothing). The answer waits for it: a second or two the first time, a
   * few milliseconds after. What was found so is marked `byMeaning`.
   */
  byMeaning?: boolean;
}

export interface MemoryResult {
  items: MemoryItem[];
  /** How the query was read, to show what is being searched for. */
  understood: { terms: string[]; app?: string; kind?: MemoryItem["kind"]; from?: number; to?: number };
  /** What was read out of the query matched nothing, and it was taken as plain words instead. */
  relaxed: boolean;
}

export declare const Memory: {
  supported(): Promise<Support>;
  /** Without `text`, the most recent. There is no asking for everything. */
  search(query?: MemoryQuery): Promise<MemoryResult>;
  get(id: string): Promise<MemoryItem | undefined>;
};

/** What the clipboard history is beside a source of `Memory`. Reading it is `Memory.search`. */
export declare const ClipboardHistory: {
  supported(): Promise<Support>;
  /** Puts an entry back on the clipboard. */
  copy(id: string): Promise<void>;
};

// --- Files (permission "files.search") -----------------------------------------

export declare const Files: {
  supported(): Promise<Support>;
  /** Files and folders under the home folder, by name. Paths only; nothing is read. */
  search(
    query: string,
    options?: { limit?: number },
  ): Promise<{
    files: { path: string; isDirectory: boolean }[];
    /** The index is still being built: there may be more. */
    scanning: boolean;
    /** How many files and folders there are to search. */
    total: number;
  }>;
};

// --- System (permission "system") ------------------------------------------------

export type SystemAction =
  | "lock-screen"
  | "sleep"
  | "restart"
  | "shut-down"
  | "log-out"
  | "empty-trash"
  | "toggle-dark-mode"
  | "volume-up"
  | "volume-down"
  | "toggle-mute";

/** A speaker or a microphone. */
export interface AudioDevice {
  /** What the sound server calls it; what `useAudioDevice` takes. */
  name: string;
  /** What to show the user. */
  description: string;
  kind: "output" | "input";
  /** Whether sound goes through it now. */
  current: boolean;
}

export declare const System: {
  supported(): Promise<Support>;
  /**
   * Answers with what there is to tell the user ("Volume 40%"), if anything.
   * The user confirms "restart", "shut-down" and "log-out" every time; when
   * they say no it fails with "cancelled".
   */
  run(action: SystemAction): Promise<string | undefined>;
  appearance(): Promise<"light" | "dark">;
  /** A short-lived desktop notification. */
  notify(title: string, body?: string): Promise<void>;
  /** The speakers and the microphones, with which of each sound goes through. */
  audioDevices(): Promise<AudioDevice[]>;
  /** Makes a device the one sound goes through, by its `name`. */
  useAudioDevice(kind: AudioDevice["kind"], name: string): Promise<void>;
};

// --- Live activities (permission "activity") ------------------------------------

/**
 * Something in progress that Heyo shows while the command is away: in the
 * palette's "In Progress" section, and (where this desktop can) in its
 * status bar. Timers and timed progress are counted by Heyo itself, so the
 * command need not stay running. Only a command the user ran, or the
 * handler of an action they chose, can start one; a command run on a
 * schedule can only update what it has.
 */
export declare namespace LiveActivity {
  /** The part that does not change. */
  interface Attributes {
    /** Unique within the extension. Starting the same id again is an update. */
    id: string;
    /** What it is ("Build", "Timer"). */
    name: string;
    icon: raycast.Image.ImageLike;
    tint?: raycast.Color.ColorLike;
    /** What choosing the activity itself does. */
    action?: Action;
    /** Whether the user may end it. True unless said otherwise. */
    dismissible?: boolean;
    /** What to do where this desktop cannot show it. "notify" unless said otherwise. */
    fallback?: "notify" | "drop";
  }

  /** The part that changes. */
  interface Content {
    /** What the collapsed form shows; the title, cut short, when left out. */
    compact?: Value;
    title: string;
    subtitle?: string;
    /** The expanded form's body, one of the three. */
    body?: { progress: Progress } | { timer: Timer } | { text: string };
    /** Three at most; shown in the expanded form and the palette's action panel. */
    actions?: Button[];
    /** Needs the user (to confirm, to answer): shown first and marked. */
    needsAttention?: boolean;
    /** After this the content may be wrong: shown dimmed. Does not end it. */
    staleAt?: Date;
    /** Among this extension's own activities, 0–100. */
    relevance?: number;
  }

  /** A small piece of content that changes by itself. */
  type Value = string | { timer: Timer } | { progress: Progress } | { icon: raycast.Image.ImageLike };

  /** A span of time Heyo counts along, with where it was paused. */
  interface Timer {
    start: Date;
    /** Left out: counting up, with no end. */
    end?: Date;
    /** Where it stands still; left out, it runs. */
    pausedAt?: Date;
    /** With an end, counting down unless false. */
    countsDown?: boolean;
    /** "12:05" or "12 min". */
    style?: "clock" | "relative";
  }

  interface Progress {
    /** 0–1. Left out with no interval, the progress is indeterminate. */
    value?: number;
    /** Moves along by itself with the time. */
    interval?: Pick<Timer, "start" | "end" | "pausedAt">;
  }

  interface Button {
    title: string;
    icon?: raycast.Image.ImageLike;
    style?: "regular" | "destructive";
    action: Action;
  }

  /** What a button does. The first three Heyo does by itself; `event` comes back to the extension. */
  type Action =
    | { open: string }
    | { copy: string }
    | { launch: { command: string; arguments?: Record<string, string>; context?: unknown } }
    | { event: string; data?: unknown }
    | { end: true };

  interface StartOptions {
    /** Show it briefly as it appears. True unless said otherwise. */
    announce?: boolean;
  }

  interface Alert {
    /** Show this update briefly. */
    expand?: boolean;
  }

  interface Handle {
    readonly id: string;
    /** Changes what is shown. One a second per activity; the rest are merged. */
    update(content: Partial<Content>, options?: { alert?: Alert }): Promise<void>;
    /** Shows the final content for a moment ("brief", the default), or not at all. */
    end(final?: Partial<Content>, options?: { keep?: "none" | "brief" }): Promise<void>;
  }

  /**
   * What comes back to the extension: a button or the activity chosen
   * (`action`), the user ending it, Heyo ending it after too long, and a
   * countdown reaching its end. Delivered to `onEvent` listeners, or else
   * by running the command marked `"heyo": { "handles": "activity" }` in
   * the manifest with the event as its `launchContext.activity`.
   */
  type Event =
    | { kind: "action"; id: string; event: string; data?: unknown }
    | { kind: "dismissed"; id: string }
    | { kind: "expired"; id: string }
    | { kind: "elapsed"; id: string; at: number };

  interface Presentation {
    /** Of the status bar; the palette's section is always there. */
    support: Support;
    /** How many characters the compact value may have. */
    compactCharacters: number;
    /** Whether the expanded form is shown, and whether its buttons are. */
    expanded: boolean;
    buttons: boolean;
    /** The version of the layouts, for later additions. */
    version: number;
  }
}

export declare const LiveActivity: {
  supported(): Promise<Support>;
  /** Starts an activity, or updates the one of that id. Fails with "denied" from a command run on a schedule, "invalid" over the limits (three per extension, 4 KB). */
  start(attributes: LiveActivity.Attributes, content: LiveActivity.Content, options?: LiveActivity.StartOptions): Promise<LiveActivity.Handle>;
  /** The extension's activities in progress. */
  list(): Promise<LiveActivity.Handle[]>;
  get(id: string): Promise<LiveActivity.Handle | undefined>;
  /** How activities show on this desktop, to fit the content to. */
  presentation(): Promise<LiveActivity.Presentation>;
  /** Hears the events of the extension's activities while the command runs. Returns how to stop. */
  onEvent(listener: (event: LiveActivity.Event) => void): Promise<() => void>;
};

// --- What Raycast's own modules gain ---------------------------------------------

/** Raycast's `Clipboard`; `paste` can put back what was on the clipboard (permission "input.paste"). */
export declare const Clipboard: Omit<typeof raycast.Clipboard, "paste"> & {
  paste(content: string | number | Clipboard.Content, options?: { restoreClipboard?: boolean }): Promise<void>;
};
export declare namespace Clipboard {
  type Content = raycast.Clipboard.Content;
  type ReadContent = raycast.Clipboard.ReadContent;
  type CopyOptions = raycast.Clipboard.CopyOptions;
}

/** Raycast's `AI`, answered by the model set up in Heyo (permission "ai" for what is added). */
export declare const AI: Omit<typeof raycast.AI, "ask"> & {
  /** Which model answers and whether what is asked stays on this computer. */
  provider(): Promise<{ kind: "claude" | "local"; model: string; local: boolean }>;
  /**
   * As Raycast's. `model` is accepted and the model set up in Heyo answers;
   * `creativity` is the temperature of a local model. Fails with a
   * `HeyoError`: "cancelled" when `signal` aborts, "failed" (with the
   * reason in words for the user) when the model does.
   */
  ask(
    prompt: string,
    options?: AI.AskOptions,
  ): Promise<string> & {
    on(event: "data", listener: (chunk: string) => void): void;
  };
};
export declare namespace AI {
  /** An earlier question or answer of a conversation. */
  interface Message {
    role: "user" | "assistant";
    content: string;
  }
  type AskOptions = raycast.AI.AskOptions & {
    /** Instead of the instructions Heyo gives the model for extensions. */
    system?: string;
    /** Image files to go with the prompt. Fails with "unsupported" where the model cannot be given images. */
    images?: string[];
    /**
     * The conversation so far, oldest first, a user's message then the
     * assistant's answer in turn; the prompt is the user's next message.
     */
    messages?: Message[];
    /**
     * The prompt is text to work on (translate, rewrite, …), not a message
     * to answer: it is fenced so that the model treats it as material and
     * follows `system` rather than anything the text says. Not with `messages`.
     */
    material?: boolean;
    /** Called with the whole answer so far, as it grows (`on("data")` gives the pieces). */
    onText?: (text: string) => void;
  };
  type Creativity = raycast.AI.Creativity;
  type Model = raycast.AI.Model;
  type ModelSelector = raycast.AI.ModelSelector;
  type RegisteredModel = raycast.AI.RegisteredModel;
  type GetModels = raycast.AI.GetModels;
  type StreamCompletion = raycast.AI.StreamCompletion;
  type ModelMessage = raycast.AI.ModelMessage;
  type ModelTool = raycast.AI.ModelTool;
  type ModelToolSet = raycast.AI.ModelToolSet;
  type ModelProviderOptions = raycast.AI.ModelProviderOptions;
  type ModelMessageContent = raycast.AI.ModelMessageContent;
  type ModelRequest = raycast.AI.ModelRequest;
  type ModelTextStream = raycast.AI.ModelTextStream;
  type ModelStreamPart = raycast.AI.ModelStreamPart;
  type ModelFullStream = raycast.AI.ModelFullStream;
  type ModelStream = raycast.AI.ModelStream;
}
