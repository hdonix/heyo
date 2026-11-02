//! What Heyo's own connectors share: reading an account's settings, the
//! text an item carries, and saying what went wrong in the terms the
//! scheduler acts on (`ConnectorError`'s kinds).

import { convert } from "html-to-text";

import { t } from "../../../../shared/i18n";
import { ConnectorError, type Account } from "../../heyo/connector";

/// The most text an item carries (the store cuts there too).
export const TEXT_LIMIT = 8000;

/// A field of the account that is text, trimmed; empty when not set.
export function text(account: Account, name: string): string {
  const value = account.config[name];
  return typeof value === "string" ? value.trim() : "";
}

/// A field that is a number, or `fallback` when it is not set or not one.
export function number(account: Account, name: string, fallback: number): number {
  const value = account.config[name];
  return typeof value === "number" && Number.isFinite(value) ? value : fallback;
}

/// A field that is a list of text, without the empty entries.
export function list(account: Account, name: string, fallback: string[] = []): string[] {
  const value = account.config[name];
  if (!Array.isArray(value)) return fallback;
  const all = value.filter((entry): entry is string => typeof entry === "string").map((entry) => entry.trim()).filter(Boolean);
  return all.length ? all : fallback;
}

/// A field that is required: its text, or a failure the user sees to.
export function required(account: Account, name: string, title: string): string {
  const value = text(account, name);
  if (!value) throw new ConnectorError("failed", t("{0} is not set.", title));
  return value;
}

/// The secret field `name`, or a failure of kind `auth` without it.
export async function secret(account: Account, name: string): Promise<string> {
  const value = await account.secret(name);
  if (!value) throw new ConnectorError("auth", t("The password is not set."));
  return value;
}

/// `body` as plain text, cut to what an item carries; HTML made text.
export function plain(body: string | undefined, html = false): string | undefined {
  if (!body) return undefined;
  const written = html ? convert(body, { wordwrap: false, selectors: [{ selector: "a", options: { ignoreHref: true } }, { selector: "img", format: "skip" }] }) : body;
  const trimmed = written.replace(/\r\n?/g, "\n").replace(/\n{3,}/g, "\n\n").trim();
  return trimmed ? trimmed.slice(0, TEXT_LIMIT) : undefined;
}

/// Error codes of a connection that did not get through: the service is
/// not there for now, not wrongly set up.
const UNREACHABLE = new Set(["ECONNREFUSED", "ECONNRESET", "ENOTFOUND", "EAI_AGAIN", "ETIMEDOUT", "EHOSTUNREACH", "ENETUNREACH", "EPIPE", "ECONNABORTED", "UND_ERR_CONNECT_TIMEOUT", "UND_ERR_SOCKET", "NoConnection", "ETIMEOUT"]);

function codeOf(error: unknown): string | undefined {
  for (let at = error as any, depth = 0; at && depth < 4; at = at.cause, depth++) {
    if (typeof at.code === "string") return at.code;
  }
  return undefined;
}

/// What went wrong, as a `ConnectorError`: one already is kept; an HTTP
/// status of 401 or 403, or what `refused` recognizes, is `auth`; a
/// connection that did not get through is `unavailable`; anything else
/// `failed`.
export function failure(error: unknown, refused: (error: any) => boolean = () => false): ConnectorError {
  if (error instanceof ConnectorError) return error;
  const message = error instanceof Error ? error.message : String(error);
  const status = (error as any)?.status ?? (error as any)?.code;
  if (status === 401 || status === 403 || refused(error)) return new ConnectorError("auth", t("The server refused the credentials: {0}", message));
  const code = codeOf(error);
  if ((code && UNREACHABLE.has(code)) || /fetch failed|socket hang up|timed? ?out/i.test(message)) {
    return new ConnectorError("unavailable", t("The server could not be reached: {0}", message));
  }
  return new ConnectorError("failed", message);
}

/// Throws the reason `signal` was aborted with, if it was.
export function stopped(signal: AbortSignal) {
  if (signal.aborted) throw signal.reason ?? new ConnectorError("failed", "Heyo stopped it.");
}

/// Resolves after `ms`, or as soon as `signal` aborts.
export function pause(ms: number, signal: AbortSignal): Promise<void> {
  return new Promise((done) => {
    if (signal.aborted) return done();
    const timer = setTimeout(finish, ms);
    signal.addEventListener("abort", finish, { once: true });
    function finish() {
      clearTimeout(timer);
      signal.removeEventListener("abort", finish);
      done();
    }
  });
}

/// The cursor a connector wrote, read back; `undefined` (a sync from
/// nothing) when there is none or it cannot be read.
export function cursorOf<T>(cursor: string | undefined): T | undefined {
  if (!cursor) return undefined;
  try {
    return JSON.parse(cursor) as T;
  } catch {
    return undefined;
  }
}
