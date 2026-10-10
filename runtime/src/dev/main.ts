//! The developer tools behind `heyo build`, `heyo dev`, `heyo migrate` and
//! `heyo ext check`. The `heyo` program starts this with Node.js
//! (`src-tauri/src/cli.rs`); it also runs on its own: `node heyo-dev.cjs
//! build .`. Required as a module it does nothing and exports the checker.

import path from "node:path";

import { build, ManifestError } from "./build";
import { checkExtension, formatReport } from "./check";
import { develop, notify } from "./develop";
import { applyMigration, diffOf, planMigration } from "./migrate";
import { registered, unregister } from "./registry";

export { checkExtension, checkManifest, checkSource, formatReport, RULES, type Finding, type Report, type Rule, type Severity } from "./check";
export { validateManifest } from "./manifest";
export { rewriteImports } from "./migrate";

export const USAGE = `Usage:
  heyo build [dir] [-o <out>] [--sourcemap]   Build an extension (into <dir>/dist)
  heyo dev [dir]                              Build on every change and load it into the running Heyo
  heyo dev --list                             The extensions in development
  heyo dev --remove <dir|name>                Take a development extension out of Heyo
  heyo migrate [dir] [--dry-run]              Rewrite @raycast/api imports to @heyo/api
  heyo ext check [dir] [--json]               Report what will not work on Heyo

[dir] is the extension's source directory (with its package.json); the current one if left out.
`;

export type Invocation =
  | { command: "build"; dir: string; out?: string; sourcemap: boolean }
  | { command: "dev"; dir: string }
  | { command: "dev-list" }
  | { command: "dev-remove"; key: string }
  | { command: "migrate"; dir: string; dryRun: boolean }
  | { command: "check"; dir: string; json: boolean }
  | { command: "help" }
  | { command: "usage"; problem: string };

/// What the arguments after `heyo` ask for.
export function parseArgs(args: string[]): Invocation {
  const usage = (problem: string): Invocation => ({ command: "usage", problem });
  let [command, ...rest] = args;
  if (command === undefined || command === "help" || args.includes("--help") || args.includes("-h")) return { command: "help" };
  if (command === "ext") {
    if (rest[0] !== "check") return usage(rest[0] ? `Unknown command: ext ${rest[0]}` : "ext needs a command: heyo ext check");
    command = "check";
    rest = rest.slice(1);
  } else if (command === "check") {
    return usage("Unknown command: check (it is heyo ext check)");
  }

  const flags = new Map<string, string | true>();
  const positional: string[] = [];
  const valued = new Set(command === "build" ? ["-o", "--out"] : command === "dev" ? ["--remove"] : []);
  const known: Record<string, string[]> = {
    build: ["-o", "--out", "--sourcemap"],
    dev: ["--remove", "--list"],
    migrate: ["--dry-run"],
    check: ["--json"],
  };
  if (!(command in known)) return usage(`Unknown command: ${command}`);
  for (let i = 0; i < rest.length; i++) {
    const arg = rest[i];
    if (!arg.startsWith("-") || arg === "-") positional.push(arg);
    else if (!known[command].includes(arg)) return usage(`Unknown option for ${command}: ${arg}`);
    else if (valued.has(arg)) {
      if (rest[i + 1] === undefined) return usage(`${arg} needs a value`);
      flags.set(arg, rest[++i]);
    } else flags.set(arg, true);
  }
  if (positional.length > 1) return usage(`One directory at a time (got ${positional.join(", ")})`);
  const dir = positional[0] ?? ".";

  switch (command) {
    case "build": {
      const out = flags.get("-o") ?? flags.get("--out");
      return { command: "build", dir, out: typeof out === "string" ? out : undefined, sourcemap: flags.has("--sourcemap") };
    }
    case "dev":
      if (flags.has("--list")) return { command: "dev-list" };
      if (flags.has("--remove")) return { command: "dev-remove", key: String(flags.get("--remove")) };
      return { command: "dev", dir };
    case "migrate":
      return { command: "migrate", dir, dryRun: flags.has("--dry-run") };
    default:
      return { command: "check", dir, json: flags.has("--json") };
  }
}

async function run(args: string[]): Promise<number> {
  const out = (text: string) => void process.stdout.write(text);
  const error = (text: string) => void process.stderr.write(text);
  const color = !!process.stdout.isTTY && !process.env.NO_COLOR;
  const invocation = parseArgs(args);
  try {
    switch (invocation.command) {
      case "help":
        out(USAGE);
        return 0;
      case "usage":
        error(`${invocation.problem}\n\n${USAGE}`);
        return 2;
      case "build": {
        const dir = path.resolve(invocation.dir);
        const built = await build(dir, invocation.out ?? path.join(dir, "dist"), { sourcemap: invocation.sourcemap, say: (m) => error(`${m}\n`), color });
        for (const warning of built.warnings) error(`Warning: ${warning}\n`);
        out(`Built ${built.commands.length} command${built.commands.length === 1 ? "" : "s"} (${built.commands.join(", ")}) into ${built.out}\n`);
        return 0;
      }
      case "dev":
        return await develop(invocation.dir, { out, error, color });
      case "dev-list": {
        const list = registered();
        if (list.length === 0) out("No extensions in development.\n");
        for (const entry of list) out(`${entry.name}\t${entry.source}\n`);
        return 0;
      }
      case "dev-remove": {
        const removed = unregister(invocation.key);
        if (removed.length === 0) {
          error(`${invocation.key} is not an extension in development (heyo dev --list shows them).\n`);
          return 1;
        }
        for (const entry of removed) {
          notify(entry.dir);
          out(`Removed ${entry.name} from Heyo. Its sources in ${entry.source} are untouched.\n`);
        }
        return 0;
      }
      case "migrate": {
        const dir = path.resolve(invocation.dir);
        const changes = planMigration(dir);
        if (changes.length === 0) {
          out("Nothing to change: no @raycast/api imports, and the manifest has its heyo field.\n");
          return 0;
        }
        if (invocation.dryRun) {
          out(diffOf(changes));
          return 0;
        }
        applyMigration(dir, changes);
        const imports = changes.reduce((sum, change) => sum + change.imports, 0);
        const files = changes.filter((change) => change.imports > 0).length;
        out(`Rewrote ${imports} import${imports === 1 ? "" : "s"} in ${files} file${files === 1 ? "" : "s"}`);
        out(changes.some((change) => change.file === "package.json") ? "; package.json has the heyo field and @heyo/api.\n" : ".\n");
        out("Next: install the new dependency (npm install), then heyo ext check.\n");
        return 0;
      }
      case "check": {
        const report = checkExtension(invocation.dir);
        out(invocation.json ? JSON.stringify(report, null, 2) + "\n" : formatReport(report, color));
        return report.summary.error > 0 ? 1 : 0;
      }
    }
  } catch (e) {
    if (e instanceof ManifestError) {
      for (const problem of e.problems) error(`${problem.severity === "error" ? "Error" : "Warning"}: ${problem.message}\n`);
    } else {
      error(`${e instanceof Error ? e.message : e}\n`);
    }
    return 1;
  }
}

// Not when imported for its functions (the tests, a report over many extensions).
if (typeof require !== "undefined" && typeof module !== "undefined" && require.main === module) {
  run(process.argv.slice(2)).then((code) => {
    // Set, not exit(): what was written to a pipe is still on its way.
    process.exitCode = code;
  });
}
