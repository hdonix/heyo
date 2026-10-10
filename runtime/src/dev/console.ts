//! The runtime's side of `heyo dev`: what a development extension's
//! commands print, and the errors they do not catch, are also appended to
//! the extension's log file, which `heyo dev` follows (`develop.ts`).
//!
//! Development builds carry source maps; Node is asked to use them, so the
//! stack of an error names the lines of the source and not of the bundle.

import fs from "node:fs";
import util from "node:util";

import { current } from "../context";
import { logFile, registered } from "./registry";

/// Built directory → log file, for the extensions in development.
let logs: Map<string, string> | undefined;

function known(): Map<string, string> {
  if (!logs) {
    logs = new Map(registered().map((entry) => [entry.dir, logFile(entry.dir)]));
    // Before their modules load: a map is read when its module is.
    if (logs.size > 0) process.setSourceMapsEnabled(true);
  }
  return logs;
}

/// Reads the list of development extensions again; `heyo dev` changed it.
export function refresh() {
  logs = undefined;
  known();
}

const LEVELS = ["log", "info", "debug", "warn", "error"] as const;
type Level = (typeof LEVELS)[number];

/// One call of `console[level]` as it is written to the log.
export function format(level: Level, command: string, args: unknown[], now = new Date()): string {
  const text = util.format(...args);
  const mark = level === "error" ? " error:" : level === "warn" ? " warning:" : "";
  return `${now.toTimeString().slice(0, 8)} [${command}]${mark} ${text}\n`;
}

/// From now on, what a development extension's command writes with
/// `console` goes to its log too. Everything still goes where it went.
export function teeConsole() {
  known();
  for (const level of LEVELS) {
    const original = console[level].bind(console);
    console[level] = (...args: unknown[]) => {
      original(...args);
      try {
        if (known().size === 0) return;
        const context = current.value;
        const log = context && known().get(context.extension.dir);
        if (log) fs.appendFileSync(log, format(level, context.command, args));
      } catch {
        // Logging must never be what breaks a command.
      }
    };
  }
}
