//! Help for extensions written with only macOS (and Windows) in mind.
//!
//! Some extensions download a command-line program and pick the build by
//! platform, knowing only those two. On Linux they end up with the macOS one,
//! which cannot run. Where the same program exists for Linux, a fix-up here
//! puts that build in the place the extension looks, before the command runs.
//! Anything else that tries to start a macOS program gets a clear error
//! instead of a cryptic one.

import childProcess, { execFile } from "node:child_process";
import { createHash } from "node:crypto";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { promisify } from "node:util";

import { supportPath, type Extension } from "./context";
import { isMacProgram } from "./macos";
import { t } from "../../shared/i18n";

const run = promisify(execFile);

interface Fixup {
  /// What is being set up, for the progress message.
  title: string;
  /// Whether there is anything to do.
  needed(extension: Extension): boolean;
  apply(extension: Extension): Promise<void>;
}

/// Ookla's Speedtest CLI, which the Speedtest extension downloads for macOS.
const speedtest: Fixup = (() => {
  const version = "1.2.0";
  // Pinned, as the extension pins its own downloads.
  const builds: Record<string, { name: string; sha256: string }> = {
    arm64: { name: "linux-aarch64", sha256: "3953d231da3783e2bf8904b6dd72767c5c6e533e163d3742fd0437affa431bd3" },
    x64: { name: "linux-x86_64", sha256: "5690596c54ff9bed63fa3732f818a05dbc2db19ad36ed68f21ca5f64d5cfeeb7" },
  };
  const target = (extension: Extension) => path.join(supportPath(extension.manifest.name), "cli", "speedtest");
  return {
    get title() {
      return t("Downloading Speedtest for Linux…");
    },
    // Missing (first run) or the macOS build the extension fetched earlier.
    needed: (extension) => !fs.existsSync(target(extension)) || isMacProgram(target(extension)),
    async apply(extension) {
      const build = builds[process.arch];
      if (!build) throw new Error(t("Speedtest has no Linux build for this processor ({0}).", process.arch));
      const response = await fetch(`https://install.speedtest.net/app/cli/ookla-speedtest-${version}-${build.name}.tgz`);
      if (!response.ok) throw new Error(t("Could not download Speedtest ({0}).", response.status));
      const archive = Buffer.from(await response.arrayBuffer());
      if (createHash("sha256").update(archive).digest("hex") !== build.sha256) {
        throw new Error(t("The Speedtest download does not match the expected file; not using it."));
      }
      const directory = path.dirname(target(extension));
      fs.mkdirSync(directory, { recursive: true });
      const file = path.join(fs.mkdtempSync(path.join(os.tmpdir(), "heyo-speedtest-")), "speedtest.tgz");
      try {
        fs.writeFileSync(file, archive);
        fs.rmSync(target(extension), { force: true });
        await run("tar", ["-xzf", file, "-C", directory, "speedtest"]);
        fs.chmodSync(target(extension), 0o755);
      } finally {
        fs.rmSync(path.dirname(file), { recursive: true, force: true });
      }
    },
  };
})();

const FIXUPS: Record<string, Fixup> = { speedtest };

/// The fix-up an extension needs before it can run on Linux, if any.
export function pendingFixup(extension: Extension): { title: string; apply(): Promise<void> } | undefined {
  if (process.platform !== "linux") return undefined;
  const fixup = FIXUPS[extension.manifest.name];
  if (!fixup?.needed(extension)) return undefined;
  return { title: fixup.title, apply: () => fixup.apply(extension) };
}

/// Makes starting a macOS program fail with an explanation.
export function explainMacPrograms() {
  if (process.platform !== "linux") return;
  const check = (command: unknown) => {
    if (typeof command === "string" && path.isAbsolute(command) && isMacProgram(command)) {
      throw new Error(`${path.basename(command)} is a macOS program and cannot run on Linux. This extension does not support Linux.`);
    }
  };
  for (const name of ["spawn", "spawnSync", "execFile", "execFileSync"] as const) {
    const original = childProcess[name] as (...args: unknown[]) => unknown;
    (childProcess as any)[name] = function (this: unknown, command: unknown, ...rest: unknown[]) {
      check(command);
      return original.call(this, command, ...rest);
    };
  }
}
