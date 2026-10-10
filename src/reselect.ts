//! Keeping a list's selection through a change of its rows: the root search
//! fetches its results again whenever the index reloads, and the row the
//! user was on should still be the one selected afterwards.

/// What is selected: the row's key, and where it was when it was chosen.
/// Without a key, the first row, whichever that comes to be (nothing was
/// chosen yet: the best result is the selection).
export interface Held {
  key: string | null;
  index: number;
}

export const FIRST: Held = { key: null, index: 0 };

/// The index of the held row among `keys`: the row with its key while that
/// is there, else the row now nearest to where it was.
export function locate(keys: string[], held: Held): number {
  if (!keys.length || held.key === null) return 0;
  const found = keys.indexOf(held.key);
  return found !== -1 ? found : Math.min(held.index, keys.length - 1);
}

/// Holds the row at `index` (or the nearest there is).
export function hold(keys: string[], index: number): Held {
  if (!keys.length) return FIRST;
  const at = Math.min(Math.max(index, 0), keys.length - 1);
  return { key: keys[at], index: at };
}
