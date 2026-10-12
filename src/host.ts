//! The host, as a window uses it: its commands, its events, the files it
//! serves, and where a failure is shown.
//!
//! The commands and events are the host's to declare
//! (`src-tauri/src/commands/bindings.rs`, written to
//! `shared/bindings/commands.ts`), so a call with a name the host does not
//! have, an argument it does not take or a result it does not give does not
//! compile.
//!
//! What carries a call across is a `Bridge`: Tauri's in the application,
//! and in a test the one `fakeHost` puts in its place (`./host.fake.ts`).

import { convertFileSrc, invoke } from "@tauri-apps/api/core";
import { listen as tauriListen } from "@tauri-apps/api/event";

import type { Commands, Events } from "../shared/bindings/commands";
import { event } from "../shared/bindings/events";

export type Command = keyof Commands;
export type Args<C extends Command> = Commands[C]["args"];
export type Answer<C extends Command> = Commands[C]["result"];
export type HostEvent = keyof Events;

/// The arguments as a call is written: none for a command that takes none.
type Given<C extends Command> = Args<C> extends null ? [] : [args: Args<C>];

export interface Bridge {
  invoke(command: string, args?: Record<string, unknown>): Promise<unknown>;
  /// Resolves to what stops the listening.
  listen(name: string, handler: (payload: unknown) => void): Promise<() => void>;
  assetUrl(path: string): string;
}

const tauri: Bridge = {
  invoke,
  listen: (name, handler) => tauriListen(name, (heard) => handler(heard.payload)),
  assetUrl: (path) => convertFileSrc(path),
};

let bridge = tauri;

/// Puts another bridge in Tauri's place.
export function connect(other: Bridge) {
  bridge = other;
}

/// Calls a command of the host's. Rejects with what the host says went
/// wrong: handle that where it changes what happens next, and otherwise say
/// `.catch(reported(…))` or, where it is meant to pass unnoticed, `.catch(ignored)`.
export function call<C extends Command>(command: C, ...given: Given<C>): Promise<Answer<C>> {
  return bridge.invoke(command, (given as [Record<string, unknown>?])[0]) as Promise<Answer<C>>;
}

/// Listens for an event of the host's until the function returned is called.
export function listen<E extends HostEvent>(name: E, handler: (payload: Events[E]) => void): () => void {
  const listening = bridge.listen(event[name], handler as (payload: unknown) => void);
  return () => void listening.then((stop) => stop());
}

/// The URL a file on this computer is loaded from.
export function assetUrl(path: string) {
  return bridge.assetUrl(path);
}

// --- Failures ----------------------------------------------------------------

export interface Failure {
  message: string;
  /// What was being done, where the window has room to say: "Could not open settings".
  title?: string;
}

/// A window with nowhere to show a failure (a pinned capture) leaves it
/// in the log.
const logged = ({ title, message }: Failure) => console.error(title ? `${title}: ${message}` : message);

let report = logged;

/// Says where this window shows failures (its toast, its error line),
/// until the function returned is called.
export function reportFailuresIn(place: (failure: Failure) => void): () => void {
  report = place;
  return () => {
    if (report === place) report = logged;
  };
}

/// Shows a failure in this window.
export function reportFailure(message: string, title?: string) {
  report({ message, title });
}

/// For `.catch`: the failure is shown in this window.
export function reported(title?: string) {
  return (error: unknown) => reportFailure(String(error), title);
}

/// For `.catch`: the failure is meant to pass unnoticed.
export function ignored() {}
