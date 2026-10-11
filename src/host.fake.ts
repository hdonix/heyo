//! The host, played in a test: it answers each command as the test says,
//! keeps the calls that were made, and says what the test has it say. It
//! sits where Tauri does (`connect` in `./host.ts`), so what is tested calls
//! and listens exactly as it does in the application.
//!
//! The answers are typed by the host's own table: a test cannot answer a
//! command that does not exist, or with something the host would not send.

import { answersNothing, type Events } from "../shared/bindings/commands";
import { event } from "../shared/bindings/events";
import { connect, type Answer, type Args, type Command, type HostEvent } from "./host";

/// How a command is answered: with this, or with what this makes of the
/// arguments. Throwing is the host refusing, in the words thrown.
export type Answering<C extends Command> = Answer<C> | ((args: Args<C>) => Answer<C> | Promise<Answer<C>>);

export type Answers = { [C in Command]?: Answering<C> };

/// A call as it was made: the command, and its arguments when it takes any.
export type Call = { [C in Command]: Args<C> extends null ? [command: C] : [command: C, args: Args<C>] }[Command];

export interface FakeHost {
  /// Every call made, in order.
  readonly calls: Call[];
  /// The arguments of each call of one command, in order.
  callsOf<C extends Command>(command: C): Args<C>[];
  /// Forgets the calls made so far.
  forget(): void;
  /// Answers a command this way from now on.
  answer<C extends Command>(command: C, answering: Answering<C>): void;
  /// Has a command fail from now on, as the host would with these words.
  refuse(command: Command, why: string): void;
  /// An event of the host's, to whoever listens for it.
  emit<E extends HostEvent>(name: E, ...carried: Events[E] extends null ? [] : [payload: Events[E]]): void;
}

/// Puts a host played by `answers` in Tauri's place. A command that is not
/// answered fails, unless it is one that answers nothing anyway.
export function fakeHost(answers: Answers = {}): FakeHost {
  const answering = new Map<string, unknown>(Object.entries(answers));
  const calls: Call[] = [];
  const listeners = new Map<string, Set<(payload: unknown) => void>>();
  const nothing: readonly string[] = answersNothing;

  connect({
    async invoke(command, args) {
      calls.push((args === undefined ? [command] : [command, args]) as Call);
      if (!answering.has(command)) {
        if (nothing.includes(command)) return null;
        throw new Error(`the fake host was not told what ${command} answers`);
      }
      const answer = answering.get(command);
      return typeof answer === "function" ? answer(args ?? null) : answer;
    },
    async listen(name, handler) {
      const heard = listeners.get(name) ?? new Set();
      listeners.set(name, heard.add(handler));
      return () => void heard.delete(handler);
    },
    assetUrl: (path) => `asset://${path}`,
  });

  return {
    calls,
    callsOf: <C extends Command>(command: C) => calls.filter((made) => made[0] === command).map((made) => (made[1] ?? null) as Args<C>),
    forget: () => void calls.splice(0),
    answer: (command, how) => void answering.set(command, how),
    refuse: (command, why) =>
      void answering.set(command, () => {
        throw why;
      }),
    emit: (name, ...carried) => listeners.get(event[name])?.forEach((handler) => handler(carried[0] ?? null)),
  };
}
