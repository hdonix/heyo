#!/usr/bin/env node
//! Which of the most-downloaded Raycast extensions run on Heyo: downloads
//! them from the Store, reads each with the static checker, launches every
//! command in the real runtime bundle against a stand-in host, and writes
//! what it saw (results.json, index.html, README.md).
//!
//!   node scripts/compat/run.mjs [--count 100] [--cache DIR] [--out DIR]
//!
//! See docs/compatibility.md.

import { execFile, execFileSync } from "node:child_process";
import { createHash } from "node:crypto";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";

import { PROBE, PROBE_MANIFEST, rankApi, rankTargets } from "./api.mjs";
import { judge, rankBlockers, rankStatic } from "./classify.mjs";
import { launchCommand } from "./drive.mjs";
import { renderHtml, renderMarkdown } from "./report.mjs";
import { bwrapWorks } from "./sandbox.mjs";
import { createStore, mostDownloaded, slim, USER_AGENT } from "./store.mjs";

const repo = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..", "..");
/// Raised when `check-worker.mjs api` reads more out of a bundle: cached
/// records from before are then read again (not launched again).
const READING = 2;

export const USAGE = `Usage: node scripts/compat/run.mjs [options]

  --count N          how many extensions, the most downloaded first (100)
  --only a,b         only these (Store names), whatever their rank
  --cache DIR        where downloads and finished results are kept
                     (default: ${path.join(os.tmpdir(), "heyo-compat")})
  --out DIR          where the report is written (docs/compatibility)
  --network MODE     on, off or both: whether launched commands may reach
                     the network (on). "both" launches each command twice.
  --sandbox MODE     auto, bwrap or none (auto: bwrap when it works)
  --jobs N           extensions launched side by side (4)
  --timeout MS       how long a command has to settle (15000)
  --memory MB        memory a launch may use before it is stopped (1024)
  --pause MS         pause between requests to the Store (500)
  --refresh          ask the Store for the listing again
  --force            launch again what the cache has results for
  --report-only      write the report from the cached results; launch nothing
  --runtime FILE     the runtime bundle (runtime/dist/heyo-runtime.cjs)
`;

export function parseOptions(argv) {
  const options = {
    count: 100,
    only: undefined,
    cache: path.join(os.tmpdir(), "heyo-compat"),
    out: path.join(repo, "docs", "compatibility"),
    network: "on",
    sandbox: "auto",
    jobs: 4,
    timeout: 15_000,
    memory: 1024,
    pause: 500,
    refresh: false,
    force: false,
    reportOnly: false,
    runtime: path.join(repo, "runtime", "dist", "heyo-runtime.cjs"),
  };
  const numbers = new Set(["count", "jobs", "timeout", "memory", "pause"]);
  for (let i = 0; i < argv.length; i++) {
    const name = argv[i].replace(/^--/, "").replace(/-([a-z])/g, (_m, c) => c.toUpperCase());
    if (!argv[i].startsWith("--") || !(name in options)) throw new Error(`unknown option ${argv[i]}`);
    if (typeof options[name] === "boolean") {
      options[name] = true;
      continue;
    }
    const value = argv[++i];
    if (value === undefined) throw new Error(`${argv[i - 1]} needs a value`);
    if (numbers.has(name)) {
      if (!/^\d+$/.test(value)) throw new Error(`${argv[i - 1]} takes a number`);
      options[name] = Number(value);
    } else if (name === "only") options.only = value.split(",").filter(Boolean);
    else options[name] = name === "network" || name === "sandbox" ? value : path.resolve(value);
  }
  if (!["on", "off", "both"].includes(options.network)) throw new Error("--network takes on, off or both");
  if (!["auto", "bwrap", "none"].includes(options.sandbox)) throw new Error("--sandbox takes auto, bwrap or none");
  return options;
}

/// Whether `dir` is, or is inside, one of Heyo's own directories of the
/// user running this: nothing of this tool's may land there.
export function insideHeyo(dir, home = os.homedir(), env = process.env) {
  const roots = [
    path.join(env.XDG_DATA_HOME || path.join(home, ".local", "share"), "heyo"),
    path.join(env.XDG_CONFIG_HOME || path.join(home, ".config"), "heyo"),
    path.join(env.XDG_CACHE_HOME || path.join(home, ".cache"), "heyo"),
  ];
  const resolved = path.resolve(dir);
  return roots.some((root) => resolved === root || resolved.startsWith(root + path.sep));
}

/// The findings of the checker for one command's file, counted by rule,
/// with a few of the errors and warnings to show.
function staticOf(findings) {
  const counts = {};
  for (const finding of findings) {
    counts[finding.rule] ??= {};
    counts[finding.rule][finding.severity] = (counts[finding.rule][finding.severity] ?? 0) + 1;
  }
  const seen = new Set();
  const samples = [];
  for (const finding of findings) {
    if (finding.severity === "info" || samples.length >= 6) continue;
    // One of each kind is enough to look at.
    const key = `${finding.rule}:${finding.message}`;
    if (seen.has(key)) continue;
    seen.add(key);
    samples.push({ rule: finding.rule, severity: finding.severity, message: finding.message, line: finding.line, excerpt: finding.excerpt?.slice(0, 140) });
  }
  return { counts, samples };
}

/// A run with what is only worth keeping when something went wrong left out.
function trimmed(run) {
  const clean = !run.error && run.outcome !== "crashed" && run.outcome !== "timeout" && !(run.toasts ?? []).some((t) => t.style === "FAILURE");
  return { ...run, stderr: clean ? undefined : run.stderr?.slice(-700) };
}

/// A record with what describes the machine it was made on taken out of
/// every text in it: the directories of the run, and the network
/// addresses an extension may have put on screen (one of the hundred
/// lists them: that is its job). The report is for publishing.
export function scrub(record, cacheDir) {
  const at = cacheDir.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
  const text = JSON.stringify(record)
    .replace(new RegExp(`${at}/work/\\d+/[^/"\\\\ ]+/runs/[^/"\\\\ ]+`, "g"), "<scratch>")
    .replace(new RegExp(`${at}/work/\\d+/[^/"\\\\ ]+/extensions/[^/"\\\\ ]+`, "g"), "<extension>")
    .replace(/\b(?:[0-9a-f]{2}:){5}[0-9a-f]{2}\b/gi, "<hardware address>")
    .replace(/\b(?:\d{1,3}\.){3}\d{1,3}\b/g, "<ip address>")
    .replace(/\b(?:[0-9a-f]{1,4}:){4,7}[0-9a-f]{1,4}\b/gi, "<ip address>");
  return JSON.parse(text);
}

async function examine({ rank, listing, options, store, check, apiOf, sandbox, work, log }) {
  const dir = await store.unpacked(listing);
  const manifest = JSON.parse(fs.readFileSync(path.join(dir, "package.json"), "utf8"));
  // The launches do not wait on the checker: without it there is less to
  // go by, and the record says so.
  let staticProblem;
  const report = await check(dir).catch((e) => {
    staticProblem = e instanceof Error ? e.message : String(e);
    log(`${listing.name}: ${staticProblem}`);
    return { findings: [] };
  });
  const api = await apiOf(dir);
  const byFile = new Map();
  for (const finding of report.findings) {
    const file = finding.file ?? "";
    if (!byFile.has(file)) byFile.set(file, []);
    byFile.get(file).push(finding);
  }

  // The extension where it may write to itself (some unpack or chmod
  // their own programs), away from the cached download.
  const home = path.join(work, listing.name);
  fs.rmSync(home, { recursive: true, force: true });
  const extensionDir = path.join(home, "extensions", manifest.name);
  fs.cpSync(dir, extensionDir, { recursive: true });

  const modes = options.network === "both" ? [true, false] : [options.network === "on"];
  const commands = [];
  for (const command of manifest.commands ?? []) {
    const findings = byFile.get(`${command.name}.js`) ?? [];
    const runs = {};
    for (const network of modes) {
      const scratch = path.join(home, "runs", `${command.name}-${network ? "net" : "nonet"}`);
      try {
        runs[network ? "online" : "offline"] = trimmed(
          await launchCommand({ runtimeFile: options.runtime, extensionDir, manifest, command, scratch, network, sandbox, timeoutMs: options.timeout, memoryMb: options.memory }),
        );
      } catch (e) {
        // The driver itself failed; the run goes on.
        log(`${listing.name}/${command.name}: ${e instanceof Error ? e.message : e}`);
      }
      fs.rmSync(scratch, { recursive: true, force: true });
    }
    const run = runs.online ?? runs.offline;
    commands.push({
      name: command.name,
      title: command.title,
      mode: command.mode,
      static: staticOf(findings).counts,
      findings: staticOf(findings).samples,
      // The API members it names that this runtime does not have.
      api: api[command.name],
      run,
      ...(runs.online && runs.offline ? { offlineRun: runs.offline } : {}),
    });
  }
  fs.rmSync(home, { recursive: true, force: true });

  const commandFiles = new Set((manifest.commands ?? []).map((c) => `${c.name}.js`));
  const general = report.findings.filter((f) => !commandFiles.has(f.file ?? "") && f.severity !== "info");
  // Without verdicts: those are worked out from this record each time the
  // report is written (`judge`), so a better reading needs no new launch.
  return scrub(
    {
      rank,
      ...slim(listing),
      manifestName: manifest.name,
      preferences: (manifest.preferences ?? []).length,
      // What the checker found outside any command's file (the manifest, assets).
      findings: staticOf(general).samples,
      ...(staticProblem ? { staticProblem } : {}),
      reading: READING,
      commands,
    },
    options.cache,
  );
}

/// What Heyo's API holds, as the runtime bundle itself reports it: a
/// command launched there writes it down (`api.mjs`). Kept per bundle.
async function apiSurface({ options, runtimeSha, sandbox, work }) {
  const file = path.join(options.cache, `api-surface.${runtimeSha}.json`);
  if (fs.existsSync(file)) return file;
  const home = path.join(work, "_probe");
  const extensionDir = path.join(home, "extensions", PROBE_MANIFEST.name);
  fs.mkdirSync(extensionDir, { recursive: true });
  fs.writeFileSync(path.join(extensionDir, "package.json"), JSON.stringify(PROBE_MANIFEST));
  fs.writeFileSync(path.join(extensionDir, "probe.js"), PROBE);
  const scratch = path.join(home, "run");
  const run = await launchCommand({ runtimeFile: options.runtime, extensionDir, manifest: PROBE_MANIFEST, command: PROBE_MANIFEST.commands[0], scratch, network: sandbox !== "bwrap", sandbox, timeoutMs: 10_000 });
  const written = path.join(scratch, "home", "api.json");
  if (!fs.existsSync(written)) throw new Error(`could not read the runtime's API (${run.outcome}): ${run.error?.message ?? run.stderr ?? "no output"}`);
  fs.copyFileSync(written, file);
  fs.rmSync(home, { recursive: true, force: true });
  return file;
}

function tally(extensions, pick = (e) => e.status) {
  const counts = { works: 0, configure: 0, partly: 0, unknown: 0, fails: 0 };
  for (const extension of extensions) counts[pick(extension)]++;
  return counts;
}

async function main() {
  const options = parseOptions(process.argv.slice(2));
  const log = (text) => process.stderr.write(`${text}\n`);
  if (insideHeyo(options.cache)) throw new Error(`--cache ${options.cache} is inside Heyo's own directories; choose another`);
  if (!fs.existsSync(options.runtime)) throw new Error(`${options.runtime} is not there: run \`pnpm build:runtime\` first`);
  fs.mkdirSync(options.cache, { recursive: true });

  const sandbox = options.sandbox === "auto" ? (bwrapWorks() ? "bwrap" : "none") : options.sandbox;
  if (sandbox === "bwrap" && !bwrapWorks()) throw new Error("bubblewrap cannot make a sandbox here (try --sandbox none)");
  if (sandbox === "none" && options.network !== "on") throw new Error("the network can only be cut with bubblewrap");
  if (sandbox === "none") log("No sandbox: extensions run as this user, with only the environment changed.");

  const checker = path.join(path.dirname(options.runtime), "heyo-dev.cjs");
  const reading = (what, args) =>
    new Promise((resolve, reject) => {
      const worker = path.join(path.dirname(fileURLToPath(import.meta.url)), "check-worker.mjs");
      execFile(process.execPath, [worker, ...args], { maxBuffer: 256 * 1024 * 1024, timeout: 120_000 }, (error, stdout) => {
        if (error) return reject(new Error(error.killed ? `${what} did not finish in 120 s` : `${what} failed: ${String(error.stderr ?? error.message).trim().split("\n").pop()}`));
        resolve(JSON.parse(stdout));
      });
    });
  const checkExtension = (dir) => reading("the static check", ["check", checker, dir]);
  const runtimeSha = createHash("sha256").update(fs.readFileSync(options.runtime)).digest("hex").slice(0, 12);
  const version = JSON.parse(fs.readFileSync(path.join(repo, "src-tauri", "tauri.conf.json"), "utf8")).version;
  let commit = "unknown";
  try {
    commit = execFileSync("git", ["rev-parse", "--short", "HEAD"], { cwd: repo, encoding: "utf8" }).trim();
  } catch {
    // Not a checkout.
  }

  const store = createStore({ cacheDir: options.cache, pauseMs: options.pause, refresh: options.refresh, log });
  const { listings, fetchedAt } = await store.listing();
  const ranked = mostDownloaded(listings, listings.length);
  const picked = options.only ? ranked.filter((l) => options.only.includes(l.name)) : ranked.slice(0, options.count);
  log(`${listings.length} extensions in the Store; examining ${picked.length}`);

  // A result is good for the same extension, runtime and way of running.
  const settings = createHash("sha256").update(JSON.stringify([runtimeSha, options.network, sandbox, options.timeout, options.memory, 4])).digest("hex").slice(0, 10);
  const resultFile = (listing) => path.join(options.cache, "results", `${listing.owner?.handle}__${listing.name}@${(listing.commit_sha ?? "unknown").slice(0, 12)}.${settings}.json`.replace(/[^\w.@-]/g, "_"));
  fs.mkdirSync(path.join(options.cache, "results"), { recursive: true });
  const work = path.join(options.cache, "work", String(process.pid));

  const surface = await apiSurface({ options, runtimeSha, sandbox, work });
  const apiOf = (dir) =>
    reading("reading the API a command uses", ["api", surface, dir]).catch((e) => {
      log(e.message);
      return {};
    });

  const extensions = new Array(picked.length);
  const queue = picked.map((listing, index) => ({ listing, index }));
  const worker = async () => {
    for (let job = queue.shift(); job; job = queue.shift()) {
      const { listing, index } = job;
      const rank = ranked.indexOf(listing) + 1;
      const file = resultFile(listing);
      if (!options.force && fs.existsSync(file)) {
        let record = scrub(JSON.parse(fs.readFileSync(file, "utf8")), options.cache);
        // A record from before the bundles were read this way: reading
        // them again needs no launch.
        if (record.reading !== READING) {
          const api = await apiOf(await store.unpacked(listing));
          record = { ...record, reading: READING, commands: record.commands.map((command) => ({ ...command, api: api[command.name] })) };
          fs.writeFileSync(file, JSON.stringify(record));
        }
        extensions[index] = judge({ ...record, rank });
        continue;
      }
      if (options.reportOnly) continue;
      try {
        const started = Date.now();
        const record = await examine({ rank, listing, options, store, check: checkExtension, apiOf, sandbox, work, log });
        fs.writeFileSync(file, JSON.stringify(record));
        const result = judge(record);
        extensions[index] = result;
        log(`${String(rank).padStart(4)} ${listing.name}: ${result.status}${result.reason ? ` (${result.reason})` : ""} — ${result.commands.length} commands, ${Math.round((Date.now() - started) / 1000)} s`);
      } catch (e) {
        log(`${String(rank).padStart(4)} ${listing.name}: could not be examined: ${e instanceof Error ? e.message : e}`);
        extensions[index] = { rank, ...slim(listing), status: "unknown", reason: "not-run", counts: {}, commands: [], findings: [], problem: String(e instanceof Error ? e.message : e) };
      }
    }
  };
  await Promise.all(Array.from({ length: options.reportOnly ? 1 : options.jobs }, worker));
  fs.rmSync(work, { recursive: true, force: true });

  // The second launch of each command is kept in the cache; the report
  // carries only what it came to.
  const done = extensions.filter(Boolean).map((e) => ({ ...e, commands: e.commands.map(({ offlineRun: _, ...command }) => command) }));
  const commands = done.flatMap((e) => e.commands);
  const listed = (test) => done.filter((e) => test((e.platforms ?? []).map((p) => p.toLowerCase()))).length;
  const results = {
    schema: 1,
    generatedAt: new Date().toISOString(),
    heyo: { version, commit, runtimeSha256: runtimeSha },
    method: {
      sample: options.only ? `chosen by name: ${options.only.join(", ")}` : `the ${done.length} most-downloaded extensions of the Raycast Store`,
      ordering: "download_count, descending, over the whole public listing (the Store's own order is not by downloads and it takes no sort parameter)",
      catalogue: listings.length,
      listingFetchedAt: fetchedAt,
      userAgent: USER_AGENT,
      network: options.network,
      sandbox,
      timeoutMs: options.timeout,
      memoryMb: options.memory,
      node: process.version,
      platform: `${process.platform} ${process.arch}`,
    },
    summary: {
      extensions: { total: done.length, ...tally(done) },
      commands: { total: commands.length, ...tally(commands, (c) => c.verdict.status) },
      ...(options.network === "both"
        ? { offline: { extensions: tally(done, (e) => e.offline?.status ?? e.status), commands: tally(commands, (c) => (c.offline?.verdict ?? c.verdict).status) } }
        : {}),
      modes: commands.reduce((n, c) => ({ ...n, [c.mode]: (n[c.mode] ?? 0) + 1 }), {}),
      // What the authors told the Store their extension is for.
      listedFor: { macOnly: listed((p) => p.length === 1 && p[0] === "macos"), windowsToo: listed((p) => p.includes("windows")), notSaid: listed((p) => p.length === 0) },
    },
    blockers: rankBlockers(done),
    staticRules: rankStatic(done),
    api: rankApi(done),
    appleScript: rankTargets(done),
    extensions: done,
  };

  fs.mkdirSync(options.out, { recursive: true });
  fs.writeFileSync(path.join(options.out, "results.json"), JSON.stringify(results));
  fs.writeFileSync(path.join(options.out, "index.html"), renderHtml(results));
  fs.writeFileSync(path.join(options.out, "README.md"), renderMarkdown(results));
  log(`Wrote ${path.join(options.out, "{results.json,index.html,README.md}")}`);
  log(JSON.stringify(results.summary));
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  main().catch((e) => {
    process.stderr.write(`${e instanceof Error ? e.message : e}\n${USAGE}`);
    process.exitCode = 1;
  });
}
