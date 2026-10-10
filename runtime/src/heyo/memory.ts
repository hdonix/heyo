//! `Memory` and `ClipboardHistory`: what Heyo remembers for the user
//! (permissions `memory.read` and `clipboard.history`). The clipboard
//! history is the one source there is so far, and nothing can be added:
//! copies marked as secrets were never recorded, so none is ever returned.

import type { Kind } from "../../../shared/bindings/Kind";
import type { MemoryItem } from "../../../shared/bindings/MemoryItem";
import type { MemoryResult } from "../../../shared/bindings/MemoryResult";
import type { Support } from "../../../shared/bindings/Support";
import { ask } from "../protocol";

export type { MemoryItem, MemoryResult };
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

export const Memory = {
  supported: () => ask<Support>("ext.supported", { module: "memory" }),
  /// Without `text`, the most recent. There is no asking for everything.
  search(query: MemoryQuery = {}): Promise<MemoryResult> {
    const { text, sources, kinds, since, until, limit, byMeaning } = query;
    return ask<MemoryResult>("ext.memory.search", { text, sources, kinds, since: since?.getTime(), until: until?.getTime(), limit, byMeaning });
  },
  async get(id: string): Promise<MemoryItem | undefined> {
    return (await ask<MemoryItem | null>("ext.memory.get", { id })) ?? undefined;
  },
};

/// What the clipboard history is beside a source of `Memory`: entries can
/// be put back on the clipboard. Reading it is `Memory.search`.
export const ClipboardHistory = {
  supported: () => ask<Support>("ext.supported", { module: "clipboardHistory" }),
  /// Puts an entry back on the clipboard.
  async copy(id: string): Promise<void> {
    await ask("ext.clipboardHistory.copy", { id });
  },
};
