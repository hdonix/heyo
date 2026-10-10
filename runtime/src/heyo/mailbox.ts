//! Where the host's events wait for whoever subscribed to them.
//!
//! A request that subscribes is answered with a number, and the host then
//! sends events with that number. The first of them may be read before the
//! answer has reached the code that asked, so events nobody hears yet are
//! kept a moment, and handed over in order when someone does.

export function mailbox(limit = 64) {
  const listeners = new Map<number, (data: unknown) => void>();
  const early = new Map<number, unknown[]>();

  function deliver(subscription: number, data: unknown) {
    const listener = listeners.get(subscription);
    if (listener) return listener(data);
    // Kept for a subscription whose answer is on its way, not for ever:
    // one nobody comes for gives way to newer ones.
    if (!early.has(subscription) && early.size >= limit) early.delete(early.keys().next().value!);
    const kept = early.get(subscription) ?? [];
    if (kept.length < limit) kept.push(data);
    early.set(subscription, kept);
  }

  /// Hears the events of a subscription, those that came early first;
  /// returns how to stop hearing them.
  function hear(subscription: number, listener: (data: unknown) => void): () => void {
    listeners.set(subscription, listener);
    const kept = early.get(subscription);
    early.delete(subscription);
    kept?.forEach((data) => deliver(subscription, data));
    return () => {
      if (listeners.get(subscription) === listener) listeners.delete(subscription);
    };
  }

  return { deliver, hear };
}
