//! Turns what was seen of a command (the record `drive.mjs` makes, and the
//! static checker's findings for its file) into a verdict, and the
//! verdicts of an extension's commands into one for the extension.
//!
//! The verdicts say no more than the evidence does. A launch shows the
//! first screen: "works" means the command launched and showed something
//! of its own (or ran to its end) without an error, and nothing in its
//! code is known not to work here. It does not mean every action in it
//! was tried.

import { createRequire } from "node:module";

/// The checker's rules (`runtime/src/dev/check.ts`) that mean "written for
/// macOS".
export const MAC_RULES = new Set(["applescript", "open-flags", "mac-program", "mac-path", "darwin-only", "swift", "mac-binary"]);
const API_RULES = new Set(["api-missing", "api-unsupported"]);

/// Programs only macOS has: the checker's list, out of the developer
/// tools as they were built (`pnpm build:runtime`).
const { MAC_PROGRAMS: ONLY_MAC } = createRequire(import.meta.url)("../../runtime/dist/heyo-dev.cjs");
// Not macOS's alone, though an extension that calls them was written for it.
const SHARED_PROGRAMS = new Set(["brew", "killall", "sysctl"]);
/// What an error names that an extension written for macOS starts: beside
/// those, `open` (which the checker reads apart, by its flags) and `swift`
/// (which other systems can have, and no extension starts for them).
const MAC_PROGRAMS = new Set([...ONLY_MAC, "open", "swift", ...SHARED_PROGRAMS]);

export const STATUSES = {
  works: "Launches cleanly",
  configure: "Needs configuration",
  partly: "Partly",
  unknown: "Inconclusive",
  fails: "Fails",
};

export const REASONS = {
  "macos-only": "Written for macOS (AppleScript, macOS programs or paths)",
  "windows-only": "Says it works only on Windows (the test ran on Linux)",
  "missing-api": "Uses part of the Raycast API that Heyo does not have",
  "heyo-runtime": "An error raised inside Heyo's runtime",
  "missing-module": "Needs a module that is not in its bundle",
  "extension-error": "An error of the extension's own (thrown in its code, or shown as a failure toast)",
  crash: "The runtime process ended",
  "needs-preferences": "Has required preferences; launched with made-up values",
  "needs-credentials": "Needs an account, a key or a token",
  "needs-oauth": "Needs a sign-in in the browser (OAuth)",
  "needs-network": "Could not reach the network",
  "missing-program": "Needs a program that is not installed on the test machine",
  "missing-file": "Needs files of another application, absent on the test machine",
  "needs-ai": "Asks for an AI model; none is set up in the test",
  "needs-input": "Turned away the made-up arguments it was launched with",
  "extension-reported": "Showed a failure message of its own, which may be right for an empty machine",
  "says-missing": "Showed a message that something it needs is not there",
  "needs-app": "Hands over to another application through that application's own kind of link",
  "empty-view": "Showed an empty view",
  "still-loading": "Still loading when time was up",
  timeout: "Showed nothing and did not end before time was up",
  "no-menu": "A menu bar command that chose to show nothing",
  "not-run": "Was not launched",
};

const basename = (program) => program.replace(/^["']|["']$/g, "").replace(/^.*\//, "");

/// What kind of trouble an error or a failure toast speaks of, from its
/// text alone. `raisedIn` is where its stack starts ("runtime",
/// "extension", …) when it has one.
export function errorReason(text, { raisedIn } = {}) {
  const t = String(text ?? "");
  const macProgram = (program) => {
    const name = basename(program);
    if (MAC_PROGRAMS.has(name) && !SHARED_PROGRAMS.has(name)) return { reason: "macos-only", detail: name === "osascript" ? "AppleScript" : name };
    return { reason: "missing-program", detail: name };
  };

  if (/osascript|applescript|\bJXA\b/i.test(t)) return { reason: "macos-only", detail: "AppleScript" };
  // What Heyo's runtime says when an extension starts a Mach-O file
  // (`explainMacPrograms` in compat.ts).
  if (/is a macOS program|does not support Linux/.test(t)) return { reason: "macos-only", detail: "a macOS program it ships or downloads" };
  if (/(?:only|currently) (?:works|runs|supported|available)(?: only)? on macOS|(?:available|supported) only on macOS|macOS only|requires macOS/i.test(t)) return { reason: "macos-only", detail: "says it is for macOS" };
  if (/(?:only|currently) (?:works|runs|supported|available)(?: only)? on Windows|(?:available|supported) only on Windows|Windows only|requires Windows/i.test(t)) return { reason: "windows-only" };

  const spawned = /spawn(?:Sync)? (\S+) ENOENT/.exec(t) ?? /command not found: ([\w./-]+)/.exec(t) ?? /(?:^|[\s:])([\w./-]+): (?:command )?not found/m.exec(t);
  if (spawned) return macProgram(spawned[1]);
  const failed = /Command failed(?: with [^:]+)?: (?:\/\S+\/)?([\w.-]+)/.exec(t);
  if (failed && MAC_PROGRAMS.has(failed[1])) return macProgram(failed[1]);

  const absent = /ENOENT[^']*'([^']+)'/.exec(t);
  if (absent) {
    if (/\/Applications(?:\/|$)|\/Library(?:\/|$)|\.app(?:\/|$)|\/Users\/|\/Volumes\/|\/System\//.test(absent[1])) return { reason: "macos-only", detail: "a macOS path" };
    return { reason: "missing-file", detail: absent[1].replace(/^.*<scratch>\/home/, "~").replace(/^\/.*\/home\/(?=\.|[A-Z])/, "~/") };
  }

  const module = /Cannot find module '([^']+)'/.exec(t);
  if (module) return { reason: "missing-module", detail: module[1] };

  // How a bundle calls the API: `(0, import_api.getX)(…)`.
  const member = /import_api\d*\.([\w.]+)\)? is not a (?:function|constructor)/.exec(t) ?? /(?:^|[\s(])(?:api|raycast)\w*\.([\w.]+) is not a (?:function|constructor)/i.exec(t);
  if (member) return { reason: "missing-api", detail: member[1] };
  // What `system.unsupported` in the runtime's API throws.
  const unsupported = /^(.+?) (?:is|are) not (?:supported|available) (?:on|in|by) Heyo/i.exec(t);
  if (unsupported) return { reason: "missing-api", detail: unsupported[1] };

  if (raisedIn === "runtime") return { reason: "heyo-runtime", detail: normalize(t) };

  if (/\b(?:401|403)\b|unauthori[sz]ed|forbidden|(?:invalid|missing|bad|no|wrong|incorrect) (?:api[ -_]?key|token|credentials|access token|password)|api[ -_]?key|access token|not (?:logged|signed) in|log ?in |sign ?in |authenticat|credential/i.test(t)) {
    return { reason: "needs-credentials" };
  }
  if (/ENOTFOUND|EAI_AGAIN|ECONNREFUSED|ECONNRESET|ETIMEDOUT|ENETUNREACH|fetch failed|getaddrinfo|socket hang up|network|timed? ?out|offline|internet/i.test(t)) return { reason: "needs-network" };

  return { reason: "extension-error", detail: normalize(t) };
}

/// An error's text with what differs from one extension to the next taken
/// out, so that the same error groups together.
export function normalize(text) {
  return String(text ?? "")
    .split("\n")[0]
    .replace(/<(?:scratch|extension|heyo)>[^\s'")]*/g, "<path>")
    .replace(/\/[\w.@-]+(?:\/[\w.@-]+)+/g, "<path>")
    .replace(/\b\d+\b/g, "N")
    .replace(/\s+/g, " ")
    .trim()
    .slice(0, 100);
}

const seconds = (ms) => `${(ms / 1000).toFixed(1)} s`;
const quoted = (titles) => (titles?.length ? ` (${titles.slice(0, 3).map((t) => `“${t}”`).join(", ")}${titles.length > 3 ? ", …" : ""})` : "");

/// What was on screen at the end, in words.
export function describe(run) {
  if (!run) return "not launched";
  const parts = [];
  const view = run.view;
  if (run.outcome === "crashed") parts.push(`the runtime process ended (${JSON.stringify(run.exit ?? {})})`);
  else if (run.outcome === "oauth") parts.push(`asked to sign in to ${run.oauth?.provider ?? "a service"} in the browser`);
  else if (run.outcome === "view" && view) {
    if (view.errorScreen) parts.push("the runtime's error screen in place of the view, which threw while rendering");
    else if (view.root === "List" || view.root === "Grid") parts.push(`${view.root} with ${view.items} item${view.items === 1 ? "" : "s"}${quoted(view.titles)}${view.emptyView ? `, empty view “${view.emptyView}”` : ""}`);
    else if (view.root === "Detail") parts.push(`Detail with ${view.markdown} characters of text`);
    else if (view.root === "Form") parts.push(`Form with ${view.fields} field${view.fields === 1 ? "" : "s"}`);
    else parts.push(`a view whose root is ${view.root ?? "nothing"}`);
    if (view.isLoading) parts.push("still loading");
    if (run.typed) parts.push(`after typing “${run.typed}”`);
  } else if (run.outcome === "finished") parts.push("ran to its end");
  else if (run.outcome === "menu") parts.push(`menu bar item${run.menu?.title ? ` “${run.menu.title}”` : ""} with ${run.menu?.items ?? 0} menu item${run.menu?.items === 1 ? "" : "s"}`);
  else if (run.outcome === "no-menu") parts.push("ran and put nothing in the menu bar");
  else parts.push("showed nothing and did not end");
  parts.push(`in ${seconds(run.ms)}`);
  let text = parts.join(", ");
  if (run.error) text += `; threw: ${run.error.message.split("\n")[0]}${run.error.frame ? ` (at ${run.error.frame})` : ""}`;
  for (const toast of (run.toasts ?? []).filter((t) => t.style === "FAILURE" && !(run.error && t.title === "Error")).slice(0, 2)) {
    text += `; failure toast: ${toast.title}${toast.message ? ` — ${toast.message}` : ""}`;
  }
  const did = (run.effects ?? []).filter((e) => e.method !== "host.hud").map((e) => e.method.replace(/^host\./, ""));
  if (did.length) text += `; asked the host to: ${[...new Set(did)].join(", ")}`;
  const hud = (run.effects ?? []).find((e) => e.method === "host.hud");
  if (hud) text += `; HUD: ${hud.params}`;
  return text;
}

/// How a command that shows no error says it cannot do its work.
const SAYS_MISSING = /\berror\b|not installed|n[o']t (?:be )?found|\bno (?:[\w-]+ ){1,4}(?:was |were )?found|you will need|please (?:install|use|set|create)|failed|unable to|could ?n[o']t|cannot|can't|unsupported|not supported|❌|🚨/i;
/// Links any desktop opens.
const WEB = new Set(["http", "https", "file", "mailto"]);

/// How an error reads when what was called or rendered is not there.
const NOT_THERE = /is not a (?:function|constructor)|Minified React error #130|Element type is invalid|Cannot read properties of undefined/;

/// The verdict on one command. `findings` are the static checker's for
/// the command's own file; `absent` the API members its bundle names that
/// the runtime does not have (`api.mjs`).
export function classifyCommand({ run, findings = [], absent = [] }) {
  if (!run) return { status: "unknown", reason: "not-run", evidence: "not launched" };
  const errors = findings.filter((f) => f.severity === "error");
  const mac = errors.filter((f) => MAC_RULES.has(f.rule));
  const api = [...new Set([...absent, ...errors.filter((f) => API_RULES.has(f.rule)).map((f) => f.message?.split(" ")[0] || f.rule)])];
  const dummy = (run.preferences?.length ?? 0) > 0;
  const evidence = describe(run);
  // An error of the extension's own in code that also has macOS-only parts
  // is quite possibly one more of them; said, not concluded.
  const suspect = (reason) => reason === "extension-error" && findings.some((f) => MAC_RULES.has(f.rule) && f.severity !== "info");
  const verdict = (status, reason, detail) => ({ status, reason, ...(detail ? { detail } : {}), ...(suspect(reason) ? { macSuspect: true } : {}), evidence });

  if (run.outcome === "crashed") return verdict("fails", "crash", run.exit?.memory ?? (run.stopped === "never-ready" ? "never started" : `exit ${run.exit?.code ?? run.exit?.signal ?? "?"}`));
  if (run.outcome === "oauth") return verdict("configure", "needs-oauth", run.oauth?.provider);

  const failure = (run.toasts ?? []).find((toast) => toast.style === "FAILURE");
  const content = !!run.view?.content || (run.outcome === "menu" && (run.menu?.items ?? 0) > 0);
  const macRules = [...new Set(mac.map((f) => f.rule))].join(", ");
  // What a command says in passing (a HUD, its log) can name the trouble
  // its message does not. Only macOS is taken from there: it cannot be
  // mistaken for something else.
  const hinted = (text) => {
    const hint = errorReason(text);
    return hint.reason === "macos-only" ? hint : undefined;
  };
  const huds = (run.effects ?? []).filter((e) => e.method === "host.hud").map((e) => e.params).join("\n");

  if (run.error || failure) {
    const text = run.error?.message ?? `${failure.title} ${failure.message ?? ""}`;
    const { reason, detail } = errorReason(text, { raisedIn: run.error?.raisedIn });
    // Something was not there, and the bundle names parts of the API that
    // are not: those are what was not there, as far as can be told.
    if (run.error && absent.length && NOT_THERE.test(text) && !["macos-only", "missing-api", "missing-module"].includes(reason)) {
      return verdict("fails", "missing-api", absent.join(", "));
    }
    // Asked for a model and there is none: Heyo's AI is not set up here.
    if (run.host?.["host.ai"] && !["macos-only", "missing-api", "missing-module"].includes(reason)) return verdict("configure", "needs-ai");
    // The made-up argument came back in the error: it was the argument.
    const quoted = Object.values(run.arguments ?? {}).some((value) => value && [`"${value}"`, `'${value}'`, `“${value}”`].some((q) => text.includes(q)));
    if (quoted && reason === "extension-error") return verdict("unknown", "needs-input", detail);
    switch (reason) {
      case "macos-only":
      case "windows-only":
      case "missing-api":
      case "missing-module":
      case "heyo-runtime":
        return verdict("fails", reason, detail);
      case "needs-credentials":
        return verdict("configure", reason);
      case "needs-network":
        // With a made-up address or key in its preferences, not reaching
        // anything says nothing about the network.
        return dummy ? verdict("configure", "needs-preferences") : verdict("unknown", reason);
      case "missing-program":
      case "missing-file":
        return verdict("unknown", reason, detail);
      default: {
        // It showed its content and complained about something besides.
        const hard = content && !run.error ? "partly" : "fails";
        const hint = hinted(`${huds}\n${run.stderr ?? ""}`);
        if (hint) return verdict(hard, hint.reason, hint.detail);
        if (dummy) return verdict("configure", "needs-preferences", "failed with made-up values; not known whether it works with real ones");
        // A failure with nothing to name it, in code that calls AppleScript
        // or the like: most likely that, and said to be a guess.
        if (mac.length) return { ...verdict(hard, "macos-only", `probably: its code has ${macRules}`), inferred: true };
        if (run.error) return verdict("fails", reason, detail);
        // "test" was not what the command wanted to be given.
        if (run.arguments) return verdict("unknown", "needs-input", detail);
        // A message of the extension's own ("No timers found") may be the
        // right answer to an empty machine as well as a defect.
        return verdict(content ? "partly" : "unknown", "extension-reported", detail);
      }
    }
  }

  // No error, but it said so in passing.
  const hint = hinted(huds);
  if (hint) return verdict("fails", hint.reason, hint.detail);
  const nothing =
    run.outcome === "timeout" ? "timeout" : run.outcome === "no-menu" ? "no-menu" : run.outcome === "view" && run.view?.isLoading ? "still-loading" : run.outcome === "view" && !content ? "empty-view" : undefined;
  if (nothing) {
    // With a made-up key or address, showing nothing is where it stops.
    if (dummy) return verdict("configure", "needs-preferences", "showed nothing with made-up values; not known whether it works with real ones");
    return verdict("unknown", nothing, nothing === "empty-view" ? run.view?.emptyView : undefined);
  }

  // No error, and something on screen — which may be the command saying
  // that what it needs is not there ("yt-dlp is not installed").
  const said = `${run.view?.root === "Detail" ? (run.view.excerpt ?? "") : ""}\n${huds}`;
  if (SAYS_MISSING.test(said)) {
    if (errorReason(said).reason === "needs-credentials") return verdict("configure", "needs-credentials");
    return verdict("unknown", "says-missing", said.trim().split("\n")[0].replace(/^#+\s*/, "").slice(0, 80));
  }
  // It handed over to another application by that application's own kind
  // of link; whether anything answers such a link here was not tested.
  const scheme = (run.effects ?? []).map((e) => (e.method === "host.open" ? /"target":"([a-zA-Z][\w+.-]*):/.exec(e.params)?.[1] : undefined)).find((s) => s && !WEB.has(s.toLowerCase()));
  if (scheme) return verdict("unknown", "needs-app", `${scheme.toLowerCase()}://`);

  // A clean launch. What the code is known to hold still counts.
  if (mac.length) return verdict("partly", "macos-only", `launches, but its code has ${macRules}`);
  if (api.length) return verdict("partly", "missing-api", api.join(", "));
  if (dummy) return verdict("configure", "needs-preferences");
  return verdict("works", undefined);
}

/// The findings a stored command record stands for: the ones kept to show,
/// and one for every rule that was only counted.
function findingsOf(command) {
  const findings = [...(command.findings ?? [])];
  for (const [rule, counts] of Object.entries(command.static ?? {})) {
    for (const severity of Object.keys(counts)) {
      if (!findings.some((f) => f.rule === rule && f.severity === severity)) findings.push({ rule, severity, message: "" });
    }
  }
  return findings;
}

/// An extension's record with every verdict worked out (again) from what
/// was seen: the launches are kept, the reading of them can change.
export function judge(extension) {
  const commands = (extension.commands ?? []).map((command) => {
    const findings = findingsOf(command);
    const absent = command.api?.breaking ?? [];
    const offline = command.offlineRun ? { outcome: command.offlineRun.outcome, verdict: classifyCommand({ run: command.offlineRun, findings, absent }) } : undefined;
    return { ...command, verdict: classifyCommand({ run: command.run, findings, absent }), ...(offline ? { offline } : {}) };
  });
  if (extension.problem) return { ...extension, commands };
  const both = commands.some((c) => c.offline);
  return {
    ...extension,
    ...classifyExtension(commands),
    ...(both ? { offline: classifyExtension(commands.map((c) => ({ verdict: c.offline?.verdict ?? c.verdict }))) } : {}),
    commands,
  };
}

/// The verdict on an extension, from those of its commands.
export function classifyExtension(commands) {
  const counts = { works: 0, configure: 0, partly: 0, unknown: 0, fails: 0 };
  const reasons = {};
  for (const { verdict } of commands) {
    counts[verdict.status]++;
    if (verdict.reason) reasons[verdict.reason] = (reasons[verdict.reason] ?? 0) + 1;
  }
  const total = commands.length;
  const good = counts.works + counts.configure;
  let status;
  if (total === 0) status = "unknown";
  else if (counts.works === total) status = "works";
  else if (good === total) status = "configure";
  else if (counts.fails === total) status = "fails";
  else if (good + counts.partly > 0) status = "partly";
  else if (counts.fails > 0) status = "fails";
  else status = "unknown";
  // The commonest reason among the commands that are not plainly working.
  const reason = Object.entries(reasons).sort((a, b) => b[1] - a[1])[0]?.[0];
  return { status, reason, counts };
}

/// What stands between a command and "works", as a name to count by.
export function blockerOf(verdict) {
  if (verdict.status === "works" || !verdict.reason) return undefined;
  const named = ["macos-only", "missing-api", "missing-module", "missing-program", "heyo-runtime", "extension-error", "crash", "needs-oauth", "needs-app"].includes(verdict.reason);
  if (!named || !verdict.detail) return verdict.reason;
  // What the static check alone says, whatever it found, is one blocker.
  if (/^launches, but/.test(verdict.detail)) return `${verdict.reason}: in code the launch did not reach`;
  if (/^probably:/.test(verdict.detail)) return `${verdict.reason}: probably (a failure in code with macOS-only parts)`;
  return `${verdict.reason}: ${verdict.detail}`;
}

/// Every blocker, with the extensions it holds back; the ones that hold
/// back the most first. `sole` are the extensions for which it is the only
/// thing between them and working: what fixing it alone would turn.
export function rankBlockers(extensions) {
  const table = new Map();
  for (const extension of extensions) {
    const own = new Map();
    for (const command of extension.commands) {
      const key = blockerOf(command.verdict);
      if (key) own.set(key, (own.get(key) ?? 0) + 1);
    }
    for (const [key, commands] of own) {
      const row = table.get(key) ?? { blocker: key, reason: key.split(":")[0], extensions: [], commands: 0, sole: [] };
      row.extensions.push(extension.name);
      row.commands += commands;
      if (own.size === 1) row.sole.push(extension.name);
      table.set(key, row);
    }
  }
  return [...table.values()].sort((a, b) => b.extensions.length - a.extensions.length || b.commands - a.commands || a.blocker.localeCompare(b.blocker));
}

/// How many extensions and commands each static rule was found in
/// (errors and warnings; notes are not counted).
export function rankStatic(extensions) {
  const table = new Map();
  for (const extension of extensions) {
    const seen = new Set();
    for (const command of extension.commands) {
      for (const [rule, count] of Object.entries(command.static ?? {})) {
        if (!count.error && !count.warning) continue;
        const row = table.get(rule) ?? { rule, extensions: 0, commands: 0 };
        row.commands++;
        if (!seen.has(rule)) row.extensions++;
        seen.add(rule);
        table.set(rule, row);
      }
    }
  }
  return [...table.values()].sort((a, b) => b.extensions - a.extensions || b.commands - a.commands);
}
