//! Connectors (docs/extension-api.md, 7.11): what a command of mode
//! `connector` exports, and how it says what went wrong. They run in a
//! process of their own (`runtime/src/connections/`), which hands them
//! their accounts and writes what they bring into Heyo's store.

import type { ConnectorErrorKind } from "../../../shared/bindings/ConnectorErrorKind";
import type { ConnectorItem } from "../../../shared/bindings/ConnectorItem";

/// One thing a connector brings: a mail, an event, a message, a document,
/// a task.
export type Item = ConnectorItem;

/// An account, as its connector is given it.
export interface Account {
  id: string;
  /// As the user calls it.
  name: string;
  /// The values of the connector's fields (`heyo.fields`) that are not secrets.
  config: Record<string, unknown>;
  /// The value of a field of type `secret`, if the user set one.
  secret(name: string): Promise<string | undefined>;
}

/// What a sync is given to do its work with.
export interface SyncIo {
  /// Writes items into the store, in place of those of the same ids;
  /// resolves once they are written.
  put(items: Item[]): Promise<void>;
  /// Removes items from the store, by their ids.
  remove(ids: string[]): Promise<void>;
  /// How far it is, from 0 to 1, and what it is doing.
  progress(fraction: number, text?: string): void;
  /// Aborted when Heyo stops the sync: the account was edited or removed,
  /// or it took too long.
  signal: AbortSignal;
}

export interface WatchIo {
  /// Something changed: Heyo syncs the account (at most once a minute).
  changed(): void;
  /// Aborted when Heyo stops the watch.
  signal: AbortSignal;
}

/// What a connector command's file exports by default.
export interface Connector {
  /// Resolves if the account works as it is set up; throws (a
  /// `ConnectorError`) if not.
  check(account: Account): Promise<void>;
  /// Brings what changed since `cursor` (everything, without one) and
  /// resolves with the cursor to go on from next time. Heyo keeps that
  /// cursor only once everything it put is written.
  sync(account: Account, cursor: string | undefined, io: SyncIo): Promise<string>;
  /// Watches the account for changes, until `io.signal` is aborted.
  watch?(account: Account, io: WatchIo): Promise<void>;
}

/// Why a connector failed, which decides what Heyo does next.
export type ConnectorFailure = Extract<ConnectorErrorKind, "auth" | "unavailable" | "failed">;

const meaning: Record<ConnectorFailure, string> = {
  auth: "The credentials were refused.",
  unavailable: "The service could not be reached.",
  failed: "The connector failed.",
};

/// A failure a connector throws: `auth` waits for the user to see to the
/// credentials; `unavailable` and `failed` are tried again after a while.
/// Anything else thrown counts as `failed`.
export class ConnectorError extends Error {
  readonly kind: ConnectorFailure;

  constructor(kind: ConnectorFailure, message?: string) {
    super(message ?? meaning[kind] ?? meaning.failed);
    this.name = "ConnectorError";
    this.kind = kind in meaning ? kind : "failed";
  }
}
