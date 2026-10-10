//! `Files`, `System` and what `Clipboard.paste` gains (permissions
//! `files.search`, `system` and `input.paste`).

import type { Action } from "../../../shared/bindings/Action";
import type { AudioDevice } from "../../../shared/bindings/AudioDevice";
import type { Support } from "../../../shared/bindings/Support";
import { Clipboard as raycastClipboard, clipboardPayload, closeMainWindow, type ClipboardContent } from "../api/system";
import { ask } from "../protocol";

export type { Action as SystemAction, AudioDevice };

export interface FoundFiles {
  files: { path: string; isDirectory: boolean }[];
  /// The index is still being built: there may be more.
  scanning: boolean;
  /// How many files and folders there are to search.
  total: number;
}

export const Files = {
  supported: () => ask<Support>("ext.supported", { module: "files" }),
  /// Files and folders under the home folder, by name. Paths only.
  search(query: string, options?: { limit?: number }): Promise<FoundFiles> {
    return ask<FoundFiles>("ext.files.search", { query, limit: options?.limit });
  },
};

export const System = {
  supported: () => ask<Support>("ext.supported", { module: "system" }),
  /// Answers with what there is to tell the user ("Volume 40%"), if anything.
  /// Restarting, shutting down and logging out are confirmed by the user
  /// every time, and fail with `cancelled` when they say no.
  async run(action: Action): Promise<string | undefined> {
    return (await ask<string | null>("ext.system.run", { action })) ?? undefined;
  },
  appearance: () => ask<"light" | "dark">("ext.system.appearance"),
  /// A short-lived desktop notification.
  async notify(title: string, body?: string): Promise<void> {
    await ask("ext.system.notify", { title, body });
  },
  /// The speakers and the microphones, with which of each sound goes through.
  audioDevices: () => ask<AudioDevice[]>("ext.system.audioDevices"),
  /// Makes a device the one sound goes through, by its `name`.
  async useAudioDevice(kind: AudioDevice["kind"], name: string): Promise<void> {
    await ask("ext.system.useAudioDevice", { kind, name });
  },
};

export const Clipboard = {
  ...raycastClipboard,
  /// As Raycast's, and with `restoreClipboard` the text that was on the
  /// clipboard is put back once the paste is done.
  async paste(content: ClipboardContent, options?: { restoreClipboard?: boolean }): Promise<void> {
    if (!options?.restoreClipboard) return raycastClipboard.paste(content);
    await closeMainWindow();
    await ask("ext.clipboard.paste", clipboardPayload(content));
  },
};
