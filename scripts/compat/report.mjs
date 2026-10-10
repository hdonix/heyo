//! The report, from the results: one self-contained HTML page (its data
//! inside it, no file fetched from anywhere) and a Markdown summary.

import { REASONS, STATUSES } from "./classify.mjs";

const ORDER = ["works", "configure", "partly", "unknown", "fails"];

const escape = (value) =>
  String(value ?? "")
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;");

const compact = (n) => Intl.NumberFormat("en", { notation: "compact" }).format(n ?? 0);
const percent = (n, total) => (total ? `${Math.round((n / total) * 100)}%` : "0%");
const day = (iso) => String(iso ?? "").slice(0, 10);

/// What the verdicts mean, in the words the page and the summary share.
export const MEANING = {
  works: "The command showed something of its own (a list with items, a text, a form), or ran to its end, with no error and no failure message — and the static check found nothing in its code that cannot work on Heyo. Only the first screen was looked at; no action was pressed.",
  configure: "It stops at something only its user can give: a required preference (an API key, a server address), an account, a sign-in in the browser, an AI model. Required preferences were filled with made-up values; what happens with real ones was not tested.",
  partly: "It launches, but its code holds parts that cannot work here (AppleScript, macOS programs, a missing API) which the launch did not reach; or it showed its content and a failure message besides.",
  unknown: "Nothing that says either way: an empty view, a view still loading or nothing at all when time was up, a failure message of the extension's own, or a program or another application's files that the test machine does not have.",
  fails: "An error was thrown or the runtime process ended; or it failed for a reason that is Heyo's or the platform's: macOS-only code, a missing API, an error inside the runtime.",
};

export const LIMITS = [
  "A launch is not a use. Each command was started once and watched until it went quiet; no action was pressed, no form submitted. An empty list that searches as you type had “test” typed into it, and that is all the typing there was.",
  "The host is a stand-in. Requests to the Heyo host (applications, clipboard, selection, windows, opening things) were answered with small made-up data and nothing was really opened, pasted or changed. What the real host does with them is not tested here.",
  "No real accounts. Required preferences were filled with made-up values, sign-ins were not completed. “Needs configuration” therefore says where the test stopped, not that the extension works once configured.",
  "The test machine is a bare Linux system: none of the applications that extensions drive (browsers, editors, password managers, Homebrew, Docker) is installed, and it has no desktop session. An extension that reads another application's files finds none.",
  "The static check reads text. It finds AppleScript and macOS programs in a command's own code whether or not the launch reaches them; it can miss what is built at run time, and it can flag code that never runs on Linux.",
  "Extensions were run for real, as Node.js programs. With bubblewrap they saw only the system's programs, the runtime and a scratch directory; that is a restriction, not a security boundary anyone should rely on, and with the network allowed they could reach it.",
  "One machine, one day. Services answer differently from one day and one address to the next.",
];

/// The data the page carries: the results, less what nobody reads there.
function forPage(results) {
  return {
    extensions: results.extensions.map((e) => ({
      rank: e.rank,
      name: e.name,
      title: e.title,
      owner: e.owner,
      downloads: e.downloads,
      platforms: e.platforms,
      url: e.storeUrl,
      status: e.status,
      reason: e.reason,
      counts: e.counts,
      offline: e.offline?.status,
      problem: e.problem,
      staticProblem: e.staticProblem,
      findings: (e.findings ?? []).slice(0, 4),
      commands: (e.commands ?? []).map((c) => ({
        name: c.name,
        title: c.title,
        mode: c.mode,
        status: c.verdict.status,
        reason: c.verdict.reason,
        detail: c.verdict.detail,
        macSuspect: c.verdict.macSuspect,
        evidence: c.verdict.evidence,
        offline: c.offline ? `${c.offline.verdict.status}${c.offline.verdict.reason ? ` (${c.offline.verdict.reason})` : ""}` : undefined,
        host: Object.keys(c.run?.host ?? {}),
        prefs: (c.run?.preferences ?? []).map((p) => `${p.name}${p.secret ? " (secret)" : ""}`),
        args: c.run?.arguments,
        error: c.run?.error && { message: c.run.error.message.slice(0, 300), frame: c.run.error.frame, raisedIn: c.run.error.raisedIn },
        absent: c.api?.breaking?.length ? c.api.breaking : undefined,
        findings: (c.findings ?? []).slice(0, 3).map((f) => ({ rule: f.rule, severity: f.severity, message: f.message, line: f.line })),
      })),
    })),
  };
}

const CSS = `
:root{--bg:#fbfbfa;--panel:#fff;--text:#1d1d1f;--muted:#666a70;--line:#e3e3e0;--accent:#3157d5;
--works:#1a7f47;--configure:#2a62c9;--partly:#a66a00;--unknown:#6b6f76;--fails:#c0352b;
--works-bg:#e3f4ea;--configure-bg:#e4ecfb;--partly-bg:#fbefd6;--unknown-bg:#ececeb;--fails-bg:#fbe4e1}
@media (prefers-color-scheme:dark){:root{--bg:#161719;--panel:#1e2023;--text:#e8e8e6;--muted:#9a9ea6;--line:#32353a;--accent:#8aa5ff;
--works:#5fd08e;--configure:#86aaf5;--partly:#e6b450;--unknown:#a3a7ae;--fails:#f0847b;
--works-bg:#17352a;--configure-bg:#1b2b4a;--partly-bg:#3a2e12;--unknown-bg:#2a2c30;--fails-bg:#44201d}}
*{box-sizing:border-box}
body{margin:0;background:var(--bg);color:var(--text);font:15px/1.5 system-ui,-apple-system,"Segoe UI",Roboto,sans-serif}
main{max-width:1080px;margin:0 auto;padding:32px 16px 64px}
h1{font-size:28px;line-height:1.2;margin:0 0 8px}
h2{font-size:19px;margin:40px 0 12px}
p{margin:8px 0}
a{color:var(--accent)}
.muted{color:var(--muted)}
.lead{font-size:16px;max-width:70ch}
code{font:13px ui-monospace,SFMono-Regular,Menlo,Consolas,monospace;overflow-wrap:anywhere}
.cards{display:grid;grid-template-columns:repeat(auto-fit,minmax(150px,1fr));gap:10px;margin:16px 0}
.card{background:var(--panel);border:1px solid var(--line);border-radius:10px;padding:12px 14px}
.card b{display:block;font-size:26px;line-height:1.1;font-variant-numeric:tabular-nums}
.card span{font-size:13px}
.bar{display:flex;height:12px;border-radius:6px;overflow:hidden;gap:2px;margin:6px 0 4px}
.bar i{display:block;min-width:2px}
.pill{display:inline-block;padding:1px 8px;border-radius:999px;font-size:12px;font-weight:600;white-space:nowrap}
${ORDER.map((s) => `.s-${s}{color:var(--${s});background:var(--${s}-bg)}.b-${s}{background:var(--${s})}.c-${s}{color:var(--${s})}`).join("\n")}
.scroll{overflow-x:auto}
table{border-collapse:collapse;width:100%;font-size:14px}
th,td{text-align:left;padding:7px 10px;border-bottom:1px solid var(--line);vertical-align:top}
th{font-size:12px;color:var(--muted);font-weight:600;white-space:nowrap}
td.n,th.n{text-align:right;font-variant-numeric:tabular-nums;white-space:nowrap}
.controls{display:flex;flex-wrap:wrap;gap:8px;margin:12px 0;align-items:center}
input[type=search],select{font:inherit;color:inherit;background:var(--panel);border:1px solid var(--line);border-radius:8px;padding:7px 10px}
input[type=search]{flex:1 1 220px;min-width:0}
.chip{font:inherit;font-size:13px;cursor:pointer;border:1px solid var(--line);background:var(--panel);color:var(--text);border-radius:999px;padding:4px 11px}
.chip[aria-pressed=true]{border-color:var(--accent);box-shadow:inset 0 0 0 1px var(--accent)}
.ext{background:var(--panel);border:1px solid var(--line);border-radius:10px;margin:8px 0}
.ext>summary{cursor:pointer;list-style:none;display:grid;grid-template-columns:3.2em minmax(0,1fr) auto;gap:4px 12px;padding:10px 14px;align-items:center}
.ext>summary::-webkit-details-marker{display:none}
.ext .name{font-weight:600}
.ext .sub{grid-column:2/4;font-size:13px}
.ext .body{padding:4px 14px 14px;border-top:1px solid var(--line)}
.ev{font-size:13px;overflow-wrap:anywhere}
.tiny{font-size:12px}
ol.limits li{margin:6px 0;max-width:80ch}
dl.meaning{display:grid;grid-template-columns:max-content 1fr;gap:8px 14px;max-width:90ch}
dl.meaning dd{margin:0}
@media (max-width:640px){.ext>summary{grid-template-columns:2.6em minmax(0,1fr)}.ext>summary .pill{grid-column:2;justify-self:start}.ext .sub{grid-column:2}dl.meaning{grid-template-columns:1fr}}
`;

const SCRIPT = `
const DATA = JSON.parse(document.getElementById("data").textContent);
const STATUSES = ${JSON.stringify(STATUSES)};
const REASONS = ${JSON.stringify(REASONS)};
const el = (tag, attrs, ...children) => {
  const node = document.createElement(tag);
  for (const [k, v] of Object.entries(attrs || {})) { if (v == null) continue; if (k === "class") node.className = v; else if (k === "text") node.textContent = v; else node.setAttribute(k, v); }
  for (const child of children.flat()) if (child != null) node.append(child);
  return node;
};
const pill = (status) => el("span", { class: "pill s-" + status, text: STATUSES[status] });
const compact = (n) => Intl.NumberFormat("en", { notation: "compact" }).format(n || 0);
const state = { status: new Set(), reason: "", query: "" };

function commandRow(c) {
  const notes = [];
  if (c.detail) notes.push(c.detail);
  if (c.macSuspect) notes.push("its code also has macOS-only parts");
  if (c.prefs.length) notes.push("made-up preferences: " + c.prefs.join(", "));
  if (c.args) notes.push("made-up arguments: " + JSON.stringify(c.args));
  if (c.host.length) notes.push("asked the host: " + c.host.map((h) => h.replace(/^host\\./, "")).join(", "));
  if (c.error && c.error.raisedIn) notes.push("raised in: " + (c.error.raisedIn === "runtime" ? "Heyo's runtime" : c.error.raisedIn === "extension" ? "the extension" : c.error.raisedIn));
  if (c.offline) notes.push("without network: " + c.offline);
  if (c.absent) notes.push("names API members Heyo lacks: " + c.absent.join(", "));
  for (const f of c.findings) notes.push("static " + f.severity + " [" + f.rule + "]" + (f.line ? " line " + f.line : "") + ": " + f.message);
  return el("tr", null,
    el("td", null, el("div", { text: c.title }), el("code", { class: "muted", text: c.name + " · " + c.mode })),
    el("td", null, pill(c.status), c.reason ? el("div", { class: "tiny muted", text: REASONS[c.reason] || c.reason }) : null),
    el("td", { class: "ev" }, el("div", { text: c.evidence }), notes.map((n) => el("div", { class: "tiny muted", text: n }))));
}

function extension(e) {
  const c = e.counts || {};
  const total = e.commands.length;
  const parts = ["works", "configure", "partly", "unknown", "fails"].filter((s) => c[s]).map((s) => c[s] + " " + STATUSES[s].toLowerCase());
  const body = el("div", { class: "body" });
  const details = el("details", { class: "ext" },
    el("summary", null,
      el("span", { class: "muted", text: "#" + e.rank }),
      el("span", { class: "name", text: e.title }),
      pill(e.status),
      el("span", { class: "sub muted", text: compact(e.downloads) + " downloads · " + total + " command" + (total === 1 ? "" : "s") + (parts.length ? ": " + parts.join(", ") : "") + (e.reason ? " · mostly: " + (REASONS[e.reason] || e.reason) : "") })),
    body);
  // Built when opened: a hundred tables nobody looks at cost a slow page.
  details.addEventListener("toggle", () => {
    if (!details.open || body.childNodes.length) return;
    body.append(el("p", { class: "tiny muted" }, el("a", { href: e.url, text: e.owner + "/" + e.name, rel: "noopener" }), " · Store platforms: " + ((e.platforms || []).join(", ") || "not said") + (e.offline ? " · without network: " + STATUSES[e.offline].toLowerCase() : "")));
    if (e.problem) body.append(el("p", { text: "Could not be examined: " + e.problem }));
    if (e.staticProblem) body.append(el("p", { class: "tiny muted", text: "No static findings: " + e.staticProblem + "." }));
    for (const f of e.findings || []) body.append(el("p", { class: "tiny muted", text: "static " + f.severity + " [" + f.rule + "]: " + f.message }));
    if (e.commands.length) body.append(el("div", { class: "scroll" }, el("table", null,
      el("thead", null, el("tr", null, el("th", { text: "Command" }), el("th", { text: "Verdict" }), el("th", { text: "What was seen" }))),
      el("tbody", null, e.commands.map(commandRow)))));
  });
  return details;
}

function matches(e) {
  if (state.status.size && !state.status.has(e.status)) return false;
  if (state.reason && e.reason !== state.reason && !e.commands.some((c) => c.reason === state.reason)) return false;
  if (state.query) {
    const hay = [e.name, e.title, e.owner, ...e.commands.flatMap((c) => [c.name, c.title, c.evidence, c.detail])].join("\\n").toLowerCase();
    if (!state.query.toLowerCase().split(/\\s+/).every((word) => hay.includes(word))) return false;
  }
  return true;
}

function render() {
  const list = document.getElementById("list");
  const shown = DATA.extensions.filter(matches);
  list.replaceChildren(...shown.map(extension));
  document.getElementById("shown").textContent = shown.length + " of " + DATA.extensions.length + " extensions";
}

const chips = document.getElementById("chips");
for (const status of Object.keys(STATUSES)) {
  const n = DATA.extensions.filter((e) => e.status === status).length;
  const chip = el("button", { class: "chip", type: "button", "aria-pressed": "false", text: STATUSES[status] + " " + n });
  chip.addEventListener("click", () => {
    state.status.has(status) ? state.status.delete(status) : state.status.add(status);
    chip.setAttribute("aria-pressed", String(state.status.has(status)));
    render();
  });
  chips.append(chip);
}
const reasons = document.getElementById("reason");
const present = new Set(DATA.extensions.flatMap((e) => e.commands.map((c) => c.reason)).filter(Boolean));
for (const reason of Object.keys(REASONS)) if (present.has(reason)) reasons.append(el("option", { value: reason, text: REASONS[reason] }));
reasons.addEventListener("change", () => { state.reason = reasons.value; render(); });
document.getElementById("search").addEventListener("input", (event) => { state.query = event.target.value.trim(); render(); });
render();
`;

function cards(counts, total, noun) {
  const bar = ORDER.map((s) => (counts[s] ? `<i class="b-${s}" style="flex:${counts[s]}" title="${escape(STATUSES[s])}: ${counts[s]}"></i>` : "")).join("");
  const cells = ORDER.map((s) => `<div class="card"><b class="c-${s}">${counts[s] ?? 0}</b><span>${escape(STATUSES[s])}<br><span class="muted">${percent(counts[s] ?? 0, total)} of ${total} ${noun}</span></span></div>`).join("");
  return `<div class="bar" role="img" aria-label="${escape(ORDER.map((s) => `${STATUSES[s]}: ${counts[s] ?? 0}`).join(", "))}">${bar}</div><div class="cards">${cells}</div>`;
}

/// The reasons in a word or two, for the table that goes into detail.
const SHORT = { "macos-only": "macOS only", "missing-api": "Missing API", "missing-module": "Missing module", "heyo-runtime": "Error in Heyo's runtime", "extension-error": "Extension's own error", crash: "Process ended" };

const blockerText = (blocker) => {
  const [reason, ...rest] = blocker.split(": ");
  return `${SHORT[reason] ?? REASONS[reason] ?? reason}${rest.length ? `: ${rest.join(": ")}` : ""}`;
};

/// The blockers one would act on: not "it needs an account".
const ACTIONABLE = new Set(["macos-only", "missing-api", "missing-module", "heyo-runtime", "extension-error", "crash"]);

/// Blockers counted by their reason alone.
export function byReason(results) {
  const table = new Map();
  for (const extension of results.extensions) {
    const seen = new Set();
    for (const command of extension.commands ?? []) {
      const reason = command.verdict.status === "works" ? undefined : command.verdict.reason;
      if (!reason) continue;
      const row = table.get(reason) ?? { reason, extensions: 0, commands: 0 };
      row.commands++;
      if (!seen.has(reason)) row.extensions++;
      seen.add(reason);
      table.set(reason, row);
    }
  }
  return [...table.values()].sort((a, b) => b.extensions - a.extensions || b.commands - a.commands);
}

/// How many commands, of how many extensions, asked each thing of the host.
export function hostUse(results) {
  const table = new Map();
  for (const extension of results.extensions) {
    const seen = new Set();
    for (const command of extension.commands ?? []) {
      for (const method of Object.keys(command.run?.host ?? {})) {
        const row = table.get(method) ?? { method, extensions: 0, commands: 0 };
        row.commands++;
        if (!seen.has(method)) row.extensions++;
        seen.add(method);
        table.set(method, row);
      }
    }
  }
  return [...table.values()].sort((a, b) => b.extensions - a.extensions || b.commands - a.commands);
}

const API_NOTE =
  "Every member of @raycast/api that a command's bundle names (List.Item.Detail, getSelectedFinderItems, …) was looked up in the API object the runtime really hands to extensions. These are the ones that are not there. A minified bundle hides which top-level names it uses, so this undercounts.";

const networkText = (method) => (method.network === "both" ? "allowed, and each command launched a second time without" : method.network === "on" ? "allowed" : "cut");

const TELLS_NOTE =
  "The applications that the AppleScript (and JXA) in the commands' bundles speaks to — “tell application …” — read from the text of the bundles, bundled libraries included. It says what an extension would need an equivalent for, not that the launch reached it.";

/// What the authors told the Store, which is not what was tested.
const listedFor = (summary) =>
  `For comparison, what the authors told the Store: ${summary.listedFor.macOnly} of the ${summary.extensions.total} are listed for macOS only, ${summary.listedFor.windowsToo} for Windows too. None is listed for Linux; the Store has no such listing.`;

export function renderHtml(results) {
  const { summary, method, heyo } = results;
  const e = summary.extensions;
  const c = summary.commands;
  const rows = (list, cells) => list.map((row) => `<tr>${cells(row).join("")}</tr>`).join("");
  const names = (list) => `${escape(list.slice(0, 8).join(", "))}${list.length > 8 ? `, +${list.length - 8}` : ""}`;
  const offline = summary.offline
    ? `<h2>The same, with the network cut</h2><p class="muted">Every command was launched a second time with no network at all. Most extensions are front ends to a service, so this mostly shows which ones do not need one for their first screen.</p>${cards(summary.offline.extensions, e.total, "extensions")}`
    : "";
  return `<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>Heyo compatibility with Raycast extensions</title>
<style>${CSS}</style>
</head>
<body>
<main>
<h1>Which Raycast extensions run on Heyo</h1>
<p class="lead">${escape(method.sample[0].toUpperCase() + method.sample.slice(1))}, each command launched once in Heyo's extension runtime on Linux and watched. This is what was seen — not a promise.</p>
<p class="muted">Heyo ${escape(heyo.version)} (commit <code>${escape(heyo.commit)}</code>, runtime <code>${escape(heyo.runtimeSha256)}</code>) · run on ${escape(day(results.generatedAt))} · ${escape(method.platform)}, Node.js ${escape(method.node)} · Store listing of ${escape(day(method.listingFetchedAt))} (${method.catalogue} extensions)</p>

<h2>Extensions</h2>
${cards(e, e.total, "extensions")}
${summary.listedFor ? `<p class="muted">${escape(listedFor(summary))}</p>` : ""}
<h2>Commands</h2>
<p class="muted">${c.total} commands: ${Object.entries(summary.modes).map(([mode, n]) => `${n} ${escape(mode)}`).join(", ")}.</p>
${cards(c, c.total, "commands")}
${offline}

<h2>What the verdicts mean</h2>
<dl class="meaning">
${ORDER.map((s) => `<dt><span class="pill s-${s}">${escape(STATUSES[s])}</span></dt><dd>${escape(MEANING[s])}</dd>`).join("\n")}
</dl>
<p class="muted">An extension “launches cleanly” when all its commands do; “needs configuration” when all its commands either launch cleanly or stop at configuration; “fails” when none gets anywhere; “partly” for every mix in between.</p>

<h2>What stands in the way</h2>
<p class="muted">Each command that does not launch cleanly has one reason. Counted by reason:</p>
<div class="scroll"><table><thead><tr><th>Reason</th><th class="n">Extensions</th><th class="n">Commands</th></tr></thead><tbody>
${rows(byReason(results), (r) => [`<td>${escape(REASONS[r.reason] ?? r.reason)}</td>`, `<td class="n">${r.extensions}</td>`, `<td class="n">${r.commands}</td>`])}
</tbody></table></div>
<p class="muted">The same, as exactly as the evidence names it — only the reasons something could be done about. “Only this” counts the extensions with nothing else in their way.</p>
<div class="scroll"><table><thead><tr><th>Blocker</th><th class="n">Extensions</th><th class="n">Only this</th><th class="n">Commands</th><th>Which</th></tr></thead><tbody>
${rows(results.blockers.filter((b) => ACTIONABLE.has(b.reason)).slice(0, 40), (b) => [`<td>${escape(blockerText(b.blocker))}</td>`, `<td class="n">${b.extensions.length}</td>`, `<td class="n">${b.sole.length}</td>`, `<td class="n">${b.commands}</td>`, `<td class="tiny muted">${names(b.extensions)}</td>`])}
</tbody></table></div>

<h2>Parts of the API that Heyo does not have</h2>
<p class="muted">${escape(API_NOTE)}</p>
${
  (results.api ?? []).length
    ? `<div class="scroll"><table><thead><tr><th>Member</th><th class="n">Extensions</th><th class="n">Commands</th><th class="n">Of those, not clean</th><th>Which</th></tr></thead><tbody>
${rows(results.api, (a) => [`<td><code>${escape(a.name)}</code>${a.cosmetic ? ' <span class="muted tiny">shows less, breaks nothing</span>' : ""}</td>`, `<td class="n">${a.extensions.length}</td>`, `<td class="n">${a.commands}</td>`, `<td class="n">${a.notClean}</td>`, `<td class="tiny muted">${names(a.extensions)}</td>`])}
</tbody></table></div>`
    : "<p>None of the commands names a member the runtime lacks.</p>"
}

${
  (results.appleScript ?? []).length
    ? `<h2>What AppleScript is used for</h2>
<p class="muted">${escape(TELLS_NOTE)}</p>
<div class="scroll"><table><thead><tr><th>Application told</th><th class="n">Extensions</th><th class="n">Commands</th><th>Which</th></tr></thead><tbody>
${rows(results.appleScript.slice(0, 30), (t) => [`<td>${escape(t.name)}</td>`, `<td class="n">${t.extensions.length}</td>`, `<td class="n">${t.commands}</td>`, `<td class="tiny muted">${names(t.extensions)}</td>`])}
</tbody></table></div>`
    : ""
}

<h2>What the static check found</h2>
<p class="muted">Errors and warnings of <code>heyo ext check</code> in the commands' own code (bundled libraries left out), whether or not the launch reached that code.</p>
<div class="scroll"><table><thead><tr><th>Rule</th><th class="n">Extensions</th><th class="n">Commands</th></tr></thead><tbody>
${rows(results.staticRules, (r) => [`<td><code>${escape(r.rule)}</code></td>`, `<td class="n">${r.extensions}</td>`, `<td class="n">${r.commands}</td>`])}
</tbody></table></div>

<h2>What commands asked of the host</h2>
<div class="scroll"><table><thead><tr><th>Request</th><th class="n">Extensions</th><th class="n">Commands</th></tr></thead><tbody>
${rows(hostUse(results), (r) => [`<td><code>${escape(r.method)}</code></td>`, `<td class="n">${r.extensions}</td>`, `<td class="n">${r.commands}</td>`])}
</tbody></table></div>

<h2>Every extension</h2>
<div class="controls"><input id="search" type="search" placeholder="Search extensions, commands, errors…" aria-label="Search"><select id="reason" aria-label="Reason"><option value="">Any reason</option></select></div>
<div class="controls" id="chips"></div>
<p class="muted tiny" id="shown"></p>
<div id="list"></div>
<noscript><p>The list of extensions needs JavaScript; the same data is in results.json and README.md beside this page.</p></noscript>

<h2>How this was done, and what it cannot tell</h2>
<p>Sample: ${escape(method.sample)}. Ordering: ${escape(method.ordering)}.</p>
<p>Each command ran in a process of its own with ${escape(String(method.timeoutMs / 1000))} s to settle and ${escape(String(method.memoryMb))} MiB of memory; isolation: ${method.sandbox === "bwrap" ? "bubblewrap (no access to the user's files)" : "none beyond a clean environment"}; network: ${escape(networkText(method))}.</p>
<ol class="limits">
${LIMITS.map((limit) => `<li>${escape(limit)}</li>`).join("\n")}
</ol>
<p class="muted">Made with <code>scripts/compat/run.mjs</code>; see <code>docs/compatibility.md</code>.</p>
</main>
<script type="application/json" id="data">${JSON.stringify(forPage(results)).replace(/</g, "\\u003c")}</script>
<script>${SCRIPT}</script>
</body>
</html>
`;
}

export function renderMarkdown(results) {
  const { summary, method, heyo } = results;
  const e = summary.extensions;
  const c = summary.commands;
  const cell = (text) => String(text ?? "").replace(/\|/g, "\\|").replace(/\s+/g, " ");
  const line = (counts, total) => ORDER.map((s) => `| ${STATUSES[s]} | ${counts[s] ?? 0} | ${percent(counts[s] ?? 0, total)} |`).join("\n");
  const out = [];
  out.push("# Which Raycast extensions run on Heyo", "");
  out.push(`${method.sample[0].toUpperCase() + method.sample.slice(1)}, each command launched once in Heyo's extension runtime on Linux and watched. This is what was seen, not a promise. The page with every command and its evidence is [index.html](index.html); the data is [results.json](results.json).`, "");
  out.push(`- Heyo ${heyo.version}, commit \`${heyo.commit}\`, runtime \`${heyo.runtimeSha256}\``);
  out.push(`- Run on ${day(results.generatedAt)}, ${method.platform}, Node.js ${method.node}`);
  out.push(`- Store listing of ${day(method.listingFetchedAt)}: ${method.catalogue} extensions. Ordering: ${method.ordering}.`);
  out.push(`- Each command: its own process, ${method.timeoutMs / 1000} s to settle, ${method.memoryMb} MiB; isolation: ${method.sandbox === "bwrap" ? "bubblewrap" : "none beyond a clean environment"}; network: ${networkText(method)}.`, "");
  out.push(`## Extensions (${e.total})`, "", "| Verdict | Extensions | Share |", "|---|---:|---:|", line(e, e.total), "");
  if (summary.listedFor) out.push(listedFor(summary), "");
  out.push(`## Commands (${c.total})`, "", "| Verdict | Commands | Share |", "|---|---:|---:|", line(c, c.total), "");
  if (summary.offline) out.push("## Extensions, with the network cut", "", "| Verdict | Extensions | Share |", "|---|---:|---:|", line(summary.offline.extensions, e.total), "");
  out.push("## What the verdicts mean", "");
  for (const s of ORDER) out.push(`- **${STATUSES[s]}** — ${MEANING[s]}`);
  out.push("", "## What stands in the way", "", "| Reason | Extensions | Commands |", "|---|---:|---:|");
  for (const r of byReason(results)) out.push(`| ${cell(REASONS[r.reason] ?? r.reason)} | ${r.extensions} | ${r.commands} |`);
  out.push("", "As exactly as the evidence names it (only what something could be done about; “only this” counts the extensions with nothing else in their way):", "", "| Blocker | Extensions | Only this | Commands | Which |", "|---|---:|---:|---:|---|");
  for (const b of results.blockers.filter((b) => ACTIONABLE.has(b.reason)).slice(0, 25)) {
    out.push(`| ${cell(blockerText(b.blocker))} | ${b.extensions.length} | ${b.sole.length} | ${b.commands} | ${cell(b.extensions.slice(0, 6).join(", "))}${b.extensions.length > 6 ? `, +${b.extensions.length - 6}` : ""} |`);
  }
  out.push("", "## Parts of the API that Heyo does not have", "", API_NOTE, "");
  if ((results.api ?? []).length) {
    out.push("| Member | Extensions | Commands | Of those, not clean | Which |", "|---|---:|---:|---:|---|");
    for (const a of results.api) out.push(`| \`${a.name}\`${a.cosmetic ? " (shows less, breaks nothing)" : ""} | ${a.extensions.length} | ${a.commands} | ${a.notClean} | ${cell(a.extensions.slice(0, 6).join(", "))}${a.extensions.length > 6 ? `, +${a.extensions.length - 6}` : ""} |`);
  } else out.push("None of the commands names a member the runtime lacks.");
  if ((results.appleScript ?? []).length) {
    out.push("", "## What AppleScript is used for", "", TELLS_NOTE, "", "| Application told | Extensions | Commands | Which |", "|---|---:|---:|---|");
    for (const t of results.appleScript.slice(0, 20)) out.push(`| ${cell(t.name)} | ${t.extensions.length} | ${t.commands} | ${cell(t.extensions.slice(0, 6).join(", "))}${t.extensions.length > 6 ? `, +${t.extensions.length - 6}` : ""} |`);
  }
  out.push("", "## What the static check found", "", "| Rule | Extensions | Commands |", "|---|---:|---:|");
  for (const r of results.staticRules) out.push(`| \`${r.rule}\` | ${r.extensions} | ${r.commands} |`);
  out.push("", "## Every extension", "", "| # | Extension | Downloads | Verdict | Commands (clean / configure / partly / inconclusive / fail) | Mostly |", "|---:|---|---:|---|---|---|");
  for (const x of results.extensions) {
    const n = x.counts ?? {};
    out.push(`| ${x.rank} | [${cell(x.title)}](${x.storeUrl}) | ${compact(x.downloads)} | ${STATUSES[x.status]} | ${ORDER.map((s) => n[s] ?? 0).join(" / ")} | ${cell(x.reason ? (REASONS[x.reason] ?? x.reason) : "")} |`);
  }
  out.push("", "## What this cannot tell", "");
  LIMITS.forEach((limit, i) => out.push(`${i + 1}. ${limit}`));
  out.push("", "Made with `scripts/compat/run.mjs`; see [../compatibility.md](../compatibility.md).", "");
  return out.join("\n");
}
