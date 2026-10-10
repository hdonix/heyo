//! JSON-lines protocol with the Heyo host over stdin/stdout. The messages
//! themselves are declared in `shared/wire.ts`.
//!
//! Requests with a `method` prefixed `host.` are answered by the Rust process,
//! and so are those an extension makes through `@heyo/api` (`ext.`, sent with
//! `ask`, which says whose they are); other requests (e.g. `ui.confirm`) are
//! forwarded to the webview.

import { createInterface } from "node:readline";
import type { Call, ExtCall, Incoming, Outgoing } from "../../shared/wire";
import type { Route } from "../../shared/bindings/Route";
import { current } from "./context";
import { envelope } from "./heyo/caller";
import { fromReply, HeyoError } from "./heyo/error";
import { mailbox } from "./heyo/mailbox";

const write = process.stdout.write.bind(process.stdout);
// Captured before extensions can replace it.
const exit = process.exit.bind(process);
// Extensions log freely; keep stdout for the protocol.
process.stdout.write = process.stderr.write.bind(process.stderr) as typeof process.stdout.write;

export function send(message: Outgoing) {
  write(JSON.stringify(message) + "\n");
}

let nextRequest = 0;
const pending = new Map<number, { resolve(v: unknown): void; reject(e: Error): void }>();

export function request<T = unknown>(...[method, params]: Call): Promise<T> {
  const id = ++nextRequest;
  return new Promise<T>((resolve, reject) => {
    pending.set(id, { resolve: resolve as (v: unknown) => void, reject });
    send({ type: "request", id, method, params });
  });
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
  const id = ++nextRequest;
  return new Promise<T>((resolve, reject) => {
    pending.set(id, { resolve: resolve as (v: unknown) => void, reject });
    send({ type: "request", id, method, ...whose, params });
  });
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

export function start() {
  const lines = createInterface({ input: process.stdin });
  lines.on("line", (line) => {
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
  });
  // The host closed our stdin: Heyo exited.
  lines.on("close", () => exit(0));
}
