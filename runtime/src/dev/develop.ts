//! `heyo dev`: builds an extension whenever its sources change and has the
//! running Heyo load the result, while printing what its commands say.
//!
//! The link with Heyo is two files and one message:
//!  - the extension is built into Heyo's data directory and named in
//!    `development.json` there (`registry.ts`), which is all Heyo needs to
//!    list its commands, now and after a restart;
//!  - after each build Heyo is told to look again, by running `heyo
//!    --dev-reload <dir>`: Heyo is single-instance, so the one that is
//!    running gets those arguments (`cli.rs`) and restarts the extension's
//!    command if it is the one open;
//!  - the runtime appends what the extension's commands print, and their
//!    errors, to a log file beside the built directory (`console.ts`), and
//!    this follows the file. A file rather than a socket: it works the same
//!    on Linux and Windows, needs no connection to be made or lost, and
//!    what was printed while nobody watched is still there.

import { spawn } from "node:child_process";
import fs from "node:fs";
import path from "node:path";

import type { BuildContext, BuildResult } from "esbuild";

import { copyStatic, esbuildOptions, formatMessages, ManifestError, prepare, type Prepared } from "./build";
import { findEsbuild } from "./esbuild";
import { buildDir, logFile, register } from "./registry";

/// Calls `changed` with what is appended to `file`, from now on. Returns
/// how to stop.
export function follow(file: string, changed: (text: string) => void, interval = 150): () => void {
  let offset = 0;
  const read = () => {
    let size: number;
    try {
      size = fs.statSync(file).size;
    } catch {
      return;
    }
    // Emptied or started afresh.
    if (size < offset) offset = 0;
    if (size === offset) return;
    const buffer = Buffer.alloc(size - offset);
    const fd = fs.openSync(file, "r");
    try {
      const got = fs.readSync(fd, buffer, 0, buffer.length, offset);
      // Only whole lines: the rest comes with the next look.
      const end = buffer.lastIndexOf(0x0a, got - 1) + 1;
      if (end === 0) return;
      offset += end;
      changed(buffer.toString("utf8", 0, end));
    } finally {
      fs.closeSync(fd);
    }
  };
  const timer = setInterval(read, interval);
  return () => {
    clearInterval(timer);
    read();
  };
}

/// Tells the running Heyo that the extension built in `dir` changed.
/// False when this was not started by `heyo` and so does not know it.
export function notify(dir: string): boolean {
  const heyo = process.env.HEYO_BIN;
  if (!heyo) return false;
  // `--hidden`: if Heyo was not running this starts it, without its window.
  const child = spawn(heyo, ["--hidden", "--dev-reload", dir], { detached: true, stdio: "ignore" });
  child.on("error", () => {});
  child.unref();
  return true;
}

const time = () => new Date().toTimeString().slice(0, 8);

export interface Terminal {
  out(text: string): void;
  error(text: string): void;
  color: boolean;
}

/// Runs until interrupted. Resolves to the process's exit code.
export async function develop(source: string, terminal: Terminal): Promise<number> {
  source = path.resolve(source);
  const dim = (text: string) => (terminal.color ? `\x1b[2m${text}\x1b[0m` : text);
  const red = (text: string) => (terminal.color ? `\x1b[31m${text}\x1b[0m` : text);
  const say = (text: string) => terminal.out(`${dim(time())} ${text}\n`);

  let prepared: Prepared;
  try {
    prepared = prepare(source);
  } catch (e) {
    terminal.error(`${e instanceof Error ? e.message : e}\n`);
    return 1;
  }
  const api = await findEsbuild(source, say);
  const out = buildDir(prepared.name);
  const log = logFile(out);
  fs.mkdirSync(out, { recursive: true });
  // This session's output only.
  fs.writeFileSync(log, "");
  register({ name: prepared.name, source, dir: out });

  let told = false;
  const built = (result: BuildResult, started: number) => {
    if (result.errors.length) {
      terminal.error(formatMessages(api, result.errors, "error", terminal.color));
      return say(red("Build failed; Heyo keeps the last one that worked."));
    }
    if (result.warnings.length) terminal.error(formatMessages(api, result.warnings, "warning", terminal.color));
    try {
      copyStatic(source, out, prepared);
    } catch (e) {
      return say(red(`Could not copy the manifest and assets: ${e instanceof Error ? e.message : e}`));
    }
    const commands = prepared.commands.length;
    const reloaded = notify(out);
    say(`Built ${commands} command${commands === 1 ? "" : "s"} in ${Date.now() - started} ms${reloaded ? "; Heyo reloaded it" : ""}`);
    if (!reloaded && !told) {
      told = true;
      say("Not started by the heyo program, so Heyo was not told. It lists the extension the next time it reloads its commands.");
    }
  };

  let context: BuildContext | undefined;
  const start = async () => {
    await context?.dispose();
    let started = Date.now();
    context = await api.context({
      ...esbuildOptions(source, prepared.commands, out, true),
      plugins: [
        {
          name: "heyo-dev",
          setup(build) {
            build.onStart(() => void (started = Date.now()));
            build.onEnd((result) => built(result, started));
          },
        },
      ],
    });
    // esbuild watches what the commands import, wherever it is.
    await context.watch();
  };

  // The manifest and the assets are not part of any bundle: watched here.
  let timer: NodeJS.Timeout | undefined;
  const manifestChanged = () => {
    clearTimeout(timer);
    timer = setTimeout(async () => {
      try {
        const next = prepare(source);
        if (next.name !== prepared.name) {
          return say(red(`The extension's name changed to "${next.name}". Stop and run heyo dev again.`));
        }
        prepared = next;
        for (const warning of next.warnings) say(`Warning: ${warning}`);
        await start();
      } catch (e) {
        const problems = e instanceof ManifestError ? e.message : String(e instanceof Error ? e.message : e);
        say(red(`package.json: ${problems.split("\n").join(`\n${" ".repeat(9)}`)}`));
      }
    }, 100);
  };
  const watchers: fs.FSWatcher[] = [];
  const watch = (target: string, options: fs.WatchOptions, relevant: (file: string) => boolean) => {
    try {
      const watcher = fs.watch(target, options, (_event, file) => {
        if (!file || relevant(String(file))) manifestChanged();
      });
      watchers.push(watcher.on("error", () => {}));
    } catch {
      // No such directory: nothing to watch.
    }
  };
  // The directory, not the file: editors replace a file to save it.
  watch(source, {}, (file) => file === "package.json");
  watch(path.join(source, "assets"), { recursive: true }, () => true);

  const unfollow = follow(log, (text) => terminal.out(text));
  say(`Developing ${prepared.name} from ${source}`);
  for (const warning of prepared.warnings) say(`Warning: ${warning}`);
  await start();

  await new Promise<void>((resolve) => {
    process.once("SIGINT", resolve);
    process.once("SIGTERM", resolve);
  });
  clearTimeout(timer);
  watchers.forEach((watcher) => watcher.close());
  await context?.dispose();
  await api.stop();
  unfollow();
  terminal.out(`\n${prepared.name} stays in Heyo as last built. To take it out: heyo dev --remove ${source}\n`);
  return 0;
}
