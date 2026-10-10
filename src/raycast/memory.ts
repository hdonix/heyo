//! What a view was showing when another was pushed over it, so that coming
//! back finds it as it was left: the search text, the selected item, how
//! far it was scrolled, what a form had filled in.
//!
//! The runtime keeps the views underneath alive (the extension's own state
//! survives) but sends only the one on top, and the webview draws each
//! afresh. Views are numbered in the order they were pushed, so the ones
//! above the view being shown are gone and are forgotten.

export interface ViewMemory {
  query: string;
  /// The selected item's node, which is the same node on the way back.
  selectedId: number | null;
  /// The `selectedItemId` the extension was asking for when the view was left.
  wanted?: unknown;
  scrollTop: number;
  /// The search bar dropdown's selection, and which dropdown it was.
  dropdown?: { node: number; value: string | undefined };
  /// A form: its values by field id, the field that had the focus, and for
  /// each file picker which choice was last taken from the runtime.
  values?: Record<string, unknown>;
  focused?: number;
  picked: Record<string, string>;
}

export class Memories {
  private views = new Map<number, ViewMemory>();

  /// The memory of the view numbered `key`, which is now on top.
  of(key: number): ViewMemory {
    for (const other of this.views.keys()) {
      if (other > key) this.views.delete(other);
    }
    let memory = this.views.get(key);
    if (!memory) this.views.set(key, (memory = { query: "", selectedId: null, scrollTop: 0, picked: {} }));
    return memory;
  }
}
