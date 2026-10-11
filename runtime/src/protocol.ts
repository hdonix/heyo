//! The runtime's conversation with the Heyo host: JSON lines over a
//! transport (`transport.ts`). The messages themselves are declared in
//! `shared/wire.ts`.
//!
//! Requests with a `method` prefixed `host.` are answered by the Rust process,
//! and so are those an extension makes through `@heyo/api` (`ext.`, sent with
//! `ask`, which says whose they are); other requests (e.g. `ui.confirm`) are
//! forwarded to the webview.
//!
//! Loading this does nothing. Modules say what the host's messages mean to
//! them with `on` as they load; the conversation begins with `connect`.

import type { Call, ExtCall, ExtCaller, Incoming, Outgoing } from "../../shared/wire";
import type { Route } from "../../shared/bindings/Route";
import { current } from "./context";
import { envelope } from "./heyo/caller";
import { fromReply, HeyoError } from "./heyo/error";
import { mailbox } from "./heyo/mailbox";
import type { Transport } from "./transport";

let host: Transport | undefined;

export function send(message: Outgoing) {
  if (!host) throw new Error(`heyo-runtime: no host to tell "${message.type}"`);
  host.send(JSON.stringify(message));
}

let nextRequest = 0;
/// The requests not answered yet. An extension's is kept with whose it is.
const pending = new Map<number, { resolve(v: unknown): void; reject(e: Error): void; whose?: ExtCaller }>();

function call<T>(method: string, params: unknown, whose?: ExtCaller): Promise<T> {
  const id = ++nextRequest;
  return new Promise<T>((resolve, reject) => {
    pending.set(id, { resolve: resolve as (v: unknown) => void, reject, whose });
    try {
      send({ type: "request", id, method, ...whose, params });
    } catch (e) {
      pending.delete(id);
      throw e;
    }
  });
}

/// Nobody is left to answer: what still waits fails, an extension's request
/// with a `HeyoError` like every failure of one.
function abandon() {
  const gone = "Heyo went away before it answered.";
  for (const [id, { reject, whose }] of pending) {
    pending.delete(id);
    reject(whose ? new HeyoError("failed", gone) : new Error(gone));
  }
}

export function request<T = unknown>(...[method, params]: Call): Promise<T> {
  return call<T>(method, params);
}

/// Asks the host for something on behalf of the extension whose command is
/// running: the request says which, from the command's context and not from
/// anything the extension passed. Fails with a `HeyoError`.
export function ask<T = unknown>(...[method, params]: ExtCall): Promise<T> {
  return askVia<T>(undefined, method, params);
}

/// The same, for what an extension reached through Raycast's API rather
/// than Heyo's own.
export function askThroughRaycast<T = unknown>(...[method, params]: ExtCall): Promise<T> {
  return askVia<T>("raycast", method, params);
}

function askVia<T>(via: Route | undefined, method: ExtCall[0], params: unknown): Promise<T> {
  const whose = envelope(current.value, via);
  if (!whose) return Promise.reject(new HeyoError("invalid", `${method} was asked for outside any command.`));
  return call<T>(method, params, whose);
}

// The events of subscriptions (`heyo/events.ts` subscribes), by the number
// the host answered the subscribing request with.
const events = mailbox();

/// Hears the events of a subscription; returns how to stop hearing them.
export const hear = events.hear;

type Handler<T extends Incoming["type"]> = (message: Extract<Incoming, { type: T }>) => void;
const handlers = new Map<string, (message: any) => void>();

export function on<T extends Exclude<Incoming["type"], "reply" | "event">>(type: T, handler: Handler<T>) {
  handlers.set(type, handler);
}

function read(line: string) {
  let message: Incoming;
  try {
    message = JSON.parse(line);
  } catch {
    console.error("heyo-runtime: invalid message", line);
    return;
  }
  if (message.type === "reply") {
    const waiter = pending.get(message.id);
    pending.delete(message.id);
    if (message.error != null) waiter?.reject(fromReply(message.error));
    else waiter?.resolve(message.result);
    return;
  }
  try {
    if (message.type === "event") events.deliver(message.subscription, message.data);
    else handlers.get(message.type)?.(message);
  } catch (e) {
    console.error(e);
  }
}

/// Begins the conversation over `transport`, ending one that was going on
/// over another. Resolves when the host hangs up.
export function connect(transport: Transport): Promise<void> {
  if (host !== transport) abandon();
  host = transport;
  const ours = () => host === transport;
  return new Promise((over) =>
    transport.receive(
      (line) => {
        if (ours()) read(line);
      },
      () => {
        if (ours()) {
          host = undefined;
          abandon();
        }
        over();
      },
    ),
  );
}
