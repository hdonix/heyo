//! Subscriptions: requests the host answers with a number and then follows
//! with events (docs/extension-api.md, 6.2).
//!
//! A subscription belongs to the command that made it. Its listener is
//! called as that command, and when the command ends the subscription is
//! ended for it; the host ends whatever is left when the runtime exits.

import type { Subscription } from "../../../shared/bindings/Subscription";
import type { ExtMethod, ExtParams, ExtReplies } from "../../../shared/wire";
import { current, runAs, whenEnded } from "../context";
import { hear } from "../protocol";
import { ask } from "./asking";

export interface Subscribed {
  /// The number the host knows it by.
  id: number;
  /// The source said it is done: no more events, nothing to tell the host.
  done(): void;
  /// Ends it at the host, which stops what it was to.
  cancel(): Promise<void>;
}

/// The requests that subscribe.
type Subscribing = { [M in ExtMethod]: ExtReplies[M] extends Subscription ? M : never }[ExtMethod];

/// Subscribes as the command that is running. `listener` gets each event.
export async function subscribe<M extends Subscribing>(call: [method: M, ...params: ExtParams<M>], listener: (data: unknown) => void): Promise<Subscribed> {
  const command = current.value;
  const { subscription }: Subscription = await ask(...call);
  let open = true;
  let forget = () => {};
  const deaf = hear(subscription, (data) => {
    if (open) runAs(command, () => listener(data));
  });
  const done = () => {
    open = false;
    deaf();
    forget();
  };
  const cancel = async () => {
    if (!open) return;
    done();
    // The host may have ended it meanwhile, which is as good.
    await runAs(command, () => ask("ext.unsubscribe", { subscription })).catch(() => {});
  };
  // The command may have ended while the answer was on its way.
  if (command) forget = whenEnded(command, () => void cancel());
  return { id: subscription, done, cancel };
}
