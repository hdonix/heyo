//! `Dictation`: what is said into the microphone, as text for the
//! extension (permission `microphone`). Nothing is pasted anywhere.

import type { ErrorCode } from "../../../shared/bindings/ErrorCode";
import type { Support } from "../../../shared/bindings/Support";
import { current, runAs } from "../context";
import { ask } from "../protocol";
import { HeyoError } from "./error";
import { subscribe, type Subscribed } from "./events";

export interface ListenOptions {
  language?: "auto" | "zh" | "en" | "ja" | "ko" | "yue";
  /// Whether Heyo's dictation window shows. It does unless this is false.
  indicator?: boolean;
  /// Called with each sentence as soon as it is recognized.
  onSegment?: (text: string) => void;
  /// Aborting discards the recording.
  signal?: AbortSignal;
}

/// What the host sends while it listens (`heard_event` in ext.rs).
type Heard = { kind: "segment"; text: string } | { kind: "result"; text: string } | { kind: "error"; code: ErrorCode; message: string };

export const Dictation = {
  supported: () => ask<Support>("ext.supported", { module: "dictation" }),
  /// Whether the speech engine has been downloaded (Heyo Settings → Dictation).
  installed: () => ask<boolean>("ext.dictation.installed"),

  /// Starts recording. It goes on until `stop()` is called, the user ends
  /// it (the dictation hotkey, the window's button), or `signal` aborts;
  /// `result` is all that was said. One recording at a time: while another
  /// is going, `result` fails with `failed`.
  listen(options: ListenOptions = {}): { stop(): void; result: Promise<string> } {
    const { language, indicator, onSegment, signal } = options;
    let subscribed: Subscribed | undefined;
    let settled = false;
    let stopping = false;
    /// The host said the recording is over (its events can outrun the
    /// answer that says which subscription they are of).
    let over = false;
    // Whoever calls `stop()`, and from wherever, it is this command that stops.
    const command = current.value;
    const stop = (subscription: number) => void runAs(command, () => ask("ext.dictation.stop", { subscription })).catch(() => {});

    const result = new Promise<string>((resolve, reject) => {
      const settle = (settling: () => void) => {
        if (settled) return;
        settled = true;
        signal?.removeEventListener("abort", abort);
        settling();
      };
      const abort = () => {
        settle(() => reject(new HeyoError("cancelled", "Listening was called off.")));
        void subscribed?.cancel();
      };
      if (signal?.aborted) return abort();
      signal?.addEventListener("abort", abort, { once: true });

      subscribe(["ext.dictation.listen", { language, indicator }], (data) => {
        const heard = data as Heard;
        if (heard.kind === "segment") return void (settled || onSegment?.(heard.text));
        over = true;
        subscribed?.done();
        if (heard.kind === "result") settle(() => resolve(heard.text));
        else settle(() => reject(new HeyoError(heard.code, heard.message)));
      }).then(
        (started) => {
          subscribed = started;
          // Over already, or aborted while the recording was starting.
          if (over) return started.done();
          if (settled) return void started.cancel();
          if (stopping) stop(started.id);
        },
        (error) => settle(() => reject(error)),
      );
    });

    return {
      stop() {
        if (settled) return;
        if (subscribed) stop(subscribed.id);
        else stopping = true;
      },
      result,
    };
  },
};
