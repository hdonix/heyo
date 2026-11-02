//! The connections runtime's conversation with the host: the host asks it
//! to check, sync, watch and stop (`connector.*`), and it asks the host to
//! write items, for secrets, and tells it of changes and progress
//! (`host.*`). Both ask and both answer, each numbering its own requests;
//! the messages are declared in `src-tauri/src/wire.rs`.
//!
//! - What a sync puts and removes goes to the host in batches of at most
//!   `ITEMS_AT_ONCE`, one after the other, each answered once it is
//!   written: the sync's cursor goes back only after its last batch.
//! - A sync or watch the host stops is answered `cancelled` at once and its
//!   signal aborted; whatever it does after is not heard.
//! - A request to the host that is not answered within a minute fails.

import type { ConnectorAccount } from "../../../shared/bindings/ConnectorAccount";
import type { ConnectorCall } from "../../../shared/bindings/ConnectorCall";
import type { ConnectorCommand } from "../../../shared/bindings/ConnectorCommand";
import type { ConnectorError as WireError } from "../../../shared/bindings/ConnectorError";
import type { ConnectorRequest } from "../../../shared/bindings/ConnectorRequest";
import { speak } from "../../../shared/i18n";
import { ConnectorError, type Account, type Connector, type Item, type Tool } from "../heyo/connector";
import type { Transport } from "../transport";

/// The most items one `host.items.put` carries (`ITEMS_AT_ONCE` in wire.rs).
export const ITEMS_AT_ONCE = 200;

export const limits = {
  /// How long a request to the host may go unanswered.
  request: 60_000,
};

/// Finds the connector a command names: an extension's, or Heyo's own.
export type Loader = (command: ConnectorCommand) => Connector;

type Reply = { type: "reply"; id: number; result?: unknown; error?: unknown };
type Asked = ConnectorCall & { type: "request"; id: number };

/// What went wrong, as the host reads it.
function wireError(error: unknown, signal?: AbortSignal): WireError {
  if (signal?.aborted) return { kind: "cancelled", message: "Heyo stopped it." };
  if (error instanceof ConnectorError) return { kind: error.kind, message: error.message };
  // One thrown by a copy of the class from elsewhere (`@heyo/api` outside Heyo).
  const { name, kind, message } = (error ?? {}) as { name?: string; kind?: string; message?: string };
  if (name === "ConnectorError" && (kind === "auth" || kind === "unavailable" || kind === "failed")) return { kind, message: message ?? kind };
  return { kind: "failed", message: error instanceof Error ? error.message : String(error) };
}

/// Holds the conversation over `host` until the host hangs up.
export function serve(host: Transport, load: Loader): Promise<void> {
  const send = (message: unknown) => host.send(JSON.stringify(message));
  const reply = (id: number, outcome: { result: unknown } | { error: WireError }) => send({ type: "reply", id, ...outcome });

  // What the host has not answered yet.
  let nextRequest = 0;
  const waiting = new Map<number, { resolve(value: unknown): void; reject(error: Error): void; timer: ReturnType<typeof setTimeout> }>();
  function request<T>(call: ConnectorRequest): Promise<T> {
    const id = ++nextRequest;
    return new Promise<T>((resolve, reject) => {
      const timer = setTimeout(() => {
        waiting.delete(id);
        reject(new Error(`Heyo did not answer ${call.method} in time.`));
      }, limits.request);
      waiting.set(id, { resolve: resolve as (value: unknown) => void, reject, timer });
      send({ type: "request", id, ...call });
    });
  }
  /// What needs no answer: told, and its answer not waited for.
  const tell = (call: ConnectorRequest) => void request(call).catch(() => {});

  // The syncs and watches running, by the number the host asked with.
  const running = new Map<number, AbortController>();

  function account(given: ConnectorAccount): Account {
    return {
      id: given.id,
      name: given.name,
      config: given.config,
      secret: async (name) => (await request<string | null>({ method: "host.connector.secret", params: { account: given.id, name } })) ?? undefined,
    };
  }

  /// Runs what the host asked for until it is done or stopped, and answers.
  async function run(id: number, work: (signal: AbortSignal) => Promise<unknown>) {
    const controller = new AbortController();
    running.set(id, controller);
    try {
      const result = await work(controller.signal);
      if (running.delete(id)) reply(id, { result: result ?? null });
    } catch (error) {
      if (running.delete(id)) reply(id, { error: wireError(error, controller.signal) });
    }
  }

  async function sync(given: ConnectorAccount, cursor: string | undefined, signal: AbortSignal) {
    const connector = load(given.connector);
    const stopped = () => {
      if (signal.aborted) throw signal.reason;
    };
    // One batch after the other: each is written before the next is sent.
    let batches = Promise.resolve();
    const batched = <T>(all: T[], each: (part: T[]) => Promise<unknown>) => {
      const sending = batches.then(async () => {
        for (let from = 0; from < all.length; from += ITEMS_AT_ONCE) {
          stopped();
          await each(all.slice(from, from + ITEMS_AT_ONCE));
        }
      });
      batches = sending.catch(() => {});
      return sending;
    };
    const io = {
      put: (items: Item[]) => batched(items, (part) => request({ method: "host.items.put", params: { account: given.id, items: part } })),
      remove: (ids: string[]) => batched(ids, (part) => request({ method: "host.items.remove", params: { account: given.id, ids: part } })),
      progress: (fraction: number, text?: string) => {
        if (!signal.aborted) tell({ method: "host.connector.progress", params: { account: given.id, fraction, ...(text === undefined ? {} : { text }) } });
      },
      tools: (tools: Tool[]) => {
        if (!signal.aborted) tell({ method: "host.connector.tools", params: { account: given.id, tools } });
      },
      signal,
    };
    const next = await connector.sync(account(given), cursor, io);
    // What it put without waiting is written before the cursor goes back.
    await batches;
    stopped();
    if (typeof next !== "string") throw new ConnectorError("failed", "The connector's sync did not resolve with a cursor (a string).");
    return { cursor: next };
  }

  async function watch(given: ConnectorAccount, signal: AbortSignal) {
    const connector = load(given.connector);
    if (!connector.watch) return null;
    await connector.watch(account(given), { changed: () => void (signal.aborted || tell({ method: "host.connector.changed", params: { account: given.id } })), signal });
    return null;
  }

  function asked(message: Asked) {
    switch (message.method) {
      case "connector.check":
        return void run(message.id, async () => {
          await load(message.params.account.connector).check(account(message.params.account));
          return null;
        });
      case "connector.sync":
        return void run(message.id, (signal) => sync(message.params.account, message.params.cursor, signal));
      case "connector.watch":
        return void run(message.id, (signal) => watch(message.params.account, signal));
      case "connector.stop": {
        const controller = running.get(message.params.request);
        if (controller && running.delete(message.params.request)) {
          controller.abort(new ConnectorError("failed", "Heyo stopped it."));
          reply(message.params.request, { error: { kind: "cancelled", message: "Heyo stopped it." } });
        }
        return reply(message.id, { result: null });
      }
      case "connector.language":
        speak(message.params.translations);
        return reply(message.id, { result: null });
    }
  }

  return new Promise((over) =>
    host.receive(
      (line) => {
        let message: Asked | Reply;
        try {
          message = JSON.parse(line);
        } catch {
          console.error("heyo-connections: not a message:", line);
          return;
        }
        if (message.type === "reply") {
          const waiter = waiting.get(message.id);
          if (!waiter) return;
          waiting.delete(message.id);
          clearTimeout(waiter.timer);
          if (message.error != null) waiter.reject(new Error(typeof message.error === "string" ? message.error : JSON.stringify(message.error)));
          else waiter.resolve(message.result);
        } else if (message.type === "request") {
          try {
            asked(message);
          } catch (error) {
            reply(message.id, { error: wireError(error) });
          }
        }
      },
      () => {
        for (const controller of running.values()) controller.abort(new ConnectorError("failed", "Heyo went away."));
        running.clear();
        for (const { reject, timer } of waiting.values()) {
          clearTimeout(timer);
          reject(new Error("Heyo went away before it answered."));
        }
        waiting.clear();
        over();
      },
    ),
  );
}
