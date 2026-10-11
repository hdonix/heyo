//! Heyo's extension runtime: runs Raycast extensions (as built by `ray build`
//! or downloaded from the Raycast Store) in Node and streams their UI to the
//! webview through the Heyo host.
//!
//! This file is the process: what it does to Node before any extension
//! loads, and the pipes. The runtime itself is `runtime.ts`.

import { explainMacPrograms } from "./compat";
import { teeConsole } from "./dev/console";
import { start } from "./runtime";
import { reportError } from "./session";
import { pipes } from "./transport";

const host = pipes();
// Captured before extensions can replace it.
const exit = process.exit.bind(process);

process.on("unhandledRejection", reportError);
process.on("uncaughtException", reportError);
// An extension calling process.exit() must not take the runtime down.
process.exit = ((code?: number) => {
  console.error(`heyo-runtime: ignored process.exit(${code ?? ""}) from an extension`);
}) as typeof process.exit;

explainMacPrograms();
teeConsole();

// One program is both processes; the host says which it started
// (`runtime.rs`). Its closing our stdin is Heyo exiting.
start(host, { background: !!process.env.HEYO_BACKGROUND }).then(() => exit(0));
