//! A host for tests: the other end of the runtime's transport, where Heyo
//! would be. It keeps what the runtime says, answers what it asks by
//! method, and tells it what the host and the webview tell.
//!
//! On a pair in memory it is the host of the runtime's own modules
//! (`testing.ts`); on a child's pipes, of the built program
//! (`heyo/wire.test.ts`). Nothing of the runtime is loaded from here but
//! the shape of its errors, so a test may set the environment first.

import type { ExtCaller, ExtRequest, HostReply, HostRequest, Incoming, UiRequests } from "../../shared/wire";
import { HeyoError } from "./heyo/error";
import type { Transport } from "./transport";

type Asked = HostRequest | ExtRequest | { [M in keyof UiRequests]: { method: M; params: UiRequests[M] } }[keyof UiRequests];

/// A request as it was read, without its number. One of an extension's
/// (`ext.*`) says whose it was.
export type Request = { method: string; params?: any } & Partial<ExtCaller>;

/// How the host answers, by method: with what the function returns or
/// resolves to, or refusing with what it throws. What has no entry is
/// answered with nothing.
export type Answers = {
  [R in Asked as R["method"]]?: (params: R extends { params: infer P } ? P : undefined, request: Request) => unknown;
};

/// What went wrong, as the host writes it: a sentence to `host.*` and the
/// webview's requests, a code and a sentence to `ext.*`.
function refusal(method: string, error: unknown): NonNullable<HostReply["error"]> {
  const message = error instanceof Error ? error.message : String(error);
  if (!method.startsWith("ext.")) return message;
  return { code: error instanceof HeyoError ? error.code : "failed", message };
}

export function testHost(transport: Transport) {
  /// What the runtime told, but for its requests, in order.
  const sent: any[] = [];
  const requests: Request[] = [];
  const answers: Answers = {};

  function tell(message: Incoming) {
    transport.send(JSON.stringify(message));
  }

  function answer({ id, ...request }: Request & { id: number }) {
    requests.push(request);
    const refuse = (error: unknown) => tell({ type: "reply", id, error: refusal(request.method, error) });
    try {
      const given = (answers as Record<string, ((params: unknown, request: Request) => unknown) | undefined>)[request.method];
      const result = given?.(request.params, request);
      if (result instanceof Promise) result.then((value) => tell({ type: "reply", id, result: value }), refuse);
      else tell({ type: "reply", id, result });
    } catch (e) {
      refuse(e);
    }
  }

  transport.receive(
    (line) => {
      const message = JSON.parse(line);
      if (message.type !== "request") return void sent.push(message);
      const { type: _, ...request } = message;
      answer(request);
    },
    () => {},
  );

  return {
    sent,
    requests,
    answers,
    /// A message from the host, or from the webview through it.
    tell,
    /// Forgets what was said and how to answer.
    reset() {
      sent.length = 0;
      requests.length = 0;
      for (const method of Object.keys(answers)) delete (answers as Record<string, unknown>)[method];
    },
  };
}
