//! How a call of `@heyo/api` fails: with a code, so that an extension need
//! not read the words.

import type { ErrorCode } from "../../../shared/bindings/ErrorCode";

export type { ErrorCode };

const meaning: Record<ErrorCode, string> = {
  unsupported: "This is not supported in this session.",
  denied: "This was not allowed.",
  cancelled: "This was called off.",
  unavailable: "Something this needs is not installed.",
  invalid: "The request was not valid.",
  failed: "This failed.",
};

export class HeyoError extends Error {
  readonly code: ErrorCode;

  constructor(code: ErrorCode, message?: string) {
    super(message ?? meaning[code] ?? meaning.failed);
    this.name = "HeyoError";
    this.code = code;
  }
}

/// What the host's reply says went wrong, as an error to throw.
export function fromReply(error: unknown): Error {
  if (typeof error === "string") return new Error(error);
  const { code, message } = (error ?? {}) as { code?: ErrorCode; message?: string };
  return new HeyoError(code && code in meaning ? code : "failed", message);
}
