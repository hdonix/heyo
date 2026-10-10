//! The reading of one extension that takes time, in a process of its own:
//! megabytes of bundle are gone through in one go, and the launches next
//! to it must not wait for that.
//!
//!   node check-worker.mjs check <heyo-dev.cjs> <extension directory>
//!     the static checker's report, as JSON
//!   node check-worker.mjs api <surface.json> <extension directory>
//!     per command, the API members it names that Heyo does not have

import fs from "node:fs";
import { createRequire } from "node:module";
import path from "node:path";

import { appleScriptTargets, missingMembers, sortMissing, usedMembers } from "./api.mjs";

const [mode, given, dir] = process.argv.slice(2);

if (mode === "check") {
  const { checkExtension } = createRequire(import.meta.url)(given);
  process.stdout.write(JSON.stringify(checkExtension(dir)));
} else if (mode === "api") {
  const surface = JSON.parse(fs.readFileSync(given, "utf8"));
  const manifest = JSON.parse(fs.readFileSync(path.join(dir, "package.json"), "utf8"));
  const out = {};
  for (const command of manifest.commands ?? []) {
    let source;
    try {
      source = fs.readFileSync(path.join(dir, `${command.name}.js`), "utf8");
    } catch {
      continue;
    }
    const used = usedMembers(source);
    out[command.name] = { used: used.size, ...sortMissing(missingMembers(used, surface)), tells: appleScriptTargets(source) };
  }
  process.stdout.write(JSON.stringify(out));
} else {
  process.stderr.write("check-worker: expected check or api\n");
  process.exitCode = 2;
}
