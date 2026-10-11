//! Asking the host on an extension's behalf, with the answer's type: what
//! a request is answered with is said once, by its method (`ExtReplies` in
//! `shared/wire.ts`), and not where it is asked.
//!
//! And the functions of `@heyo/api` that are nothing but their request,
//! made from their declarations (`members.ts`).

import type { ExtCall, ExtMethod, ExtParams, ExtReplies } from "../../../shared/wire";
import * as protocol from "../protocol";
import type { Asks, ByHand, Declared } from "./members";

/// A method and its parameters as the protocol takes them. Callers are
/// held to the pair by their own signatures, method by method, which the
/// union of all pairs cannot be shown from.
const call = (method: ExtMethod, params: unknown) => [method, params] as unknown as ExtCall;

/// Asks in the name of the command that is running. Fails with a `HeyoError`.
export function ask<M extends ExtMethod>(method: M, ...params: ExtParams<M>): Promise<ExtReplies[M]> {
  return protocol.ask<ExtReplies[M]>(...call(method, params[0]));
}

/// The same, for what an extension reached through Raycast's API rather
/// than Heyo's own.
export function askThroughRaycast<M extends ExtMethod>(method: M, ...params: ExtParams<M>): Promise<ExtReplies[M]> {
  return protocol.askThroughRaycast<ExtReplies[M]>(...call(method, params[0]));
}

/// What a function hands back of the host's answer: null is nothing.
type Answer<Reply> = [Reply] extends [null] ? void : null extends Reply ? Exclude<Reply, null> | undefined : Reply;

/// The names of the functions a declaration leaves to be written by hand.
type Left<D> = { [K in keyof D]: D[K] extends ByHand<any> ? K : never }[keyof D];
type Made<D> = { [K in Exclude<keyof D, Left<D>>]: D[K] extends Asks<infer M, infer A> ? (...args: A) => Promise<Answer<ExtReplies[M]>> : never };
type Written<D> = Record<Left<D>, (...args: never[]) => unknown>;

/// The functions of a module: those that are their request are made, and
/// those the declaration leaves to be written by hand are given here, all
/// of them and no other.
export function implement<D extends Record<string, Declared>>(declared: [Left<D>] extends [never] ? D : never): Made<D>;
export function implement<D extends Record<string, Declared>, W extends Written<D>>(declared: D, written: W & Record<Exclude<keyof W, Left<D>>, never>): Made<D> & W;
export function implement(declared: Record<string, Declared>, written: object = {}) {
  const made: Record<string, unknown> = {};
  for (const [name, member] of Object.entries(declared)) {
    if ("byHand" in member) continue;
    made[name] = async (...args: unknown[]) => (await protocol.ask(...call(member.method, member.params?.(...args)))) ?? undefined;
  }
  return { ...made, ...written };
}
