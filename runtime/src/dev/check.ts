//! `heyo ext check`: what in an extension will not work on Heyo.
//!
//! Two kinds of thing: parts of the Raycast API that Heyo does not have
//! (read from the lists the conformance tests keep, `api/conformance.gaps.json`
//! and `api/props.gaps.json`), and code that only makes sense on macOS
//! whatever the API: AppleScript, `open -a`, macOS programs and paths,
//! branches for `darwin` alone, native addons, Swift helpers.
//!
//! It reads text; it does not run or type-check anything. A finding is a
//! place worth looking at, not a proof.
//!
//! `checkExtension` and `checkSource` are also what a compatibility report
//! over many extensions calls: keep their signatures.

import fs from "node:fs";
import path from "node:path";

import apiGaps from "../api/conformance.gaps.json";
import propGaps from "../api/props.gaps.json";
import { modules, permissionsOf, type ModuleName } from "../heyo/members";
import { existsIn, readPackage, sourceFiles } from "./files";
import { lex, withoutComments, type Token } from "./lex";
import { API_MODULES, validateManifest } from "./manifest";

export type Severity = "error" | "warning" | "info";

export interface Finding {
  /// A stable name for the kind of problem (see `RULES`).
  rule: Rule;
  severity: Severity;
  message: string;
  /// Relative to the extension's directory, `/`-separated. Absent for
  /// findings about the extension as a whole.
  file?: string;
  /// Both count from 1.
  line?: number;
  column?: number;
  /// The line of source, trimmed.
  excerpt?: string;
}

export interface Report {
  extension: { name: string; title: string; dir: string };
  /// How many source files were read.
  files: number;
  findings: Finding[];
  summary: Record<Severity, number>;
}

/// Every rule, with what it means.
export const RULES = {
  manifest: "The manifest is missing something Heyo needs, or names something that is not there",
  applescript: "AppleScript and JXA exist only on macOS",
  "open-flags": "`open` with macOS flags (-a, -b, -R…); other systems have no such program",
  "mac-program": "Starts a program that only macOS has",
  "mac-path": "A path that only exists on macOS",
  "darwin-only": "Checks for macOS and has nothing for Linux or Windows",
  swift: "Swift helper code, which is built for and runs on macOS only",
  "native-addon": "A dependency with compiled code, built for one system and processor",
  "mac-binary": "A macOS program shipped in the extension",
  "api-missing": "Part of the Raycast API that Heyo does not have",
  "api-unsupported": "Part of the Raycast API that Heyo has but cannot do",
  "prop-ignored": "A component property Heyo accepts and ignores",
  "permission-missing": "Uses a Heyo module whose permission the manifest does not declare",
  platforms: "The manifest's `platforms` do not include one Heyo runs on",
  "ai-tools": "AI tools, which Heyo does not run",
} as const;

export type Rule = keyof typeof RULES;

/// Programs that come with macOS and that nothing else has under that name.
/// (`open` is handled apart: with a plain file or address it has equivalents.)
const MAC_PROGRAMS = new Set([
  "osascript", "pbcopy", "pbpaste", "defaults", "mdfind", "mdls", "say", "pmset", "networksetup", "launchctl", "screencapture",
  "sips", "caffeinate", "diskutil", "system_profiler", "scutil", "plutil", "afplay", "security", "ditto", "hdiutil", "sw_vers",
  "softwareupdate", "shortcuts", "qlmanage", "tmutil", "airport", "automator", "dscl", "spctl", "codesign", "xattr", "osacompile",
  "PlistBuddy", "lsregister", "mdutil",
]);

/// What on a line says a string on it is a command being run.
const RUNS = /\b(?:exec|execSync|execFile|execFileSync|spawn|spawnSync|execa|execaSync|execaCommand|runCommand|shell|useExec)\b|\$`/;

const MAC_PATHS: [RegExp, string][] = [
  [/(?:^|[\s"'`=:(,])\/Applications(?:\/|$|[\s"'`])/, "/Applications"],
  [/~\/Library\b|\/Users\/[^/"'`\s]+\/Library\b|^Library\/|\/Library\/(?:Application Support|Preferences|Caches|Containers|Group Containers|Keychains|LaunchAgents|Safari|Mail|Messages)\b/, "~/Library"],
  [/^Application Support$|^Application Support\//, "~/Library/Application Support"],
  [/\/System\/(?:Library|Applications|Volumes)\b/, "/System"],
  [/\/opt\/homebrew\b|\/usr\/local\/Cellar\b|\/usr\/local\/Homebrew\b/, "Homebrew's directory"],
  [/(?:^|["'`\s])\/Volumes\//, "/Volumes"],
];

/// What Heyo has of these and how it falls short. Kept by hand from
/// `runtime/src/api/index.ts`: the first two are answered with an error
/// (`system.unsupported`), the rest are accepted and do less than in Raycast.
const UNSUPPORTED: Record<string, { severity: Severity; message: string }> = {
  "BrowserExtension.getContent": { severity: "error", message: "BrowserExtension.getContent throws on Heyo: there is no browser extension to ask" },
  "BrowserExtension.getTabs": { severity: "error", message: "BrowserExtension.getTabs throws on Heyo: there is no browser extension to ask" },
  updateCommandMetadata: { severity: "info", message: "updateCommandMetadata does nothing on Heyo: the root search shows the subtitle from the manifest" },
};

const flat = (lists: Record<string, string[]>) => Object.values(lists).flat();

interface Imports {
  /// Local name → the name it was imported under (`List`, or `*` for the
  /// whole module, which `import * as` and `require()` give).
  names: Map<string, string>;
  /// Where each local name was imported.
  at: Map<string, number>;
  /// Whether any of it came from `@heyo/api`.
  heyo: Set<string>;
}

const escape = (text: string) => text.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
const MODULE = `["'](${API_MODULES.map(escape).join("|")})["']`;

/// What a source takes from the API modules, under which names.
function apiImports(code: string): Imports {
  const imports: Imports = { names: new Map(), at: new Map(), heyo: new Set() };
  const add = (local: string, imported: string, index: number, module: string) => {
    imports.names.set(local, imported);
    imports.at.set(local, index);
    if (module === "@heyo/api") imports.heyo.add(local);
  };
  const braces = (list: string, index: number, module: string, separator: RegExp) => {
    for (const part of list.split(",")) {
      const [imported, local] = part.trim().replace(/^type\s+/, "").split(separator).map((s) => s.trim());
      if (imported && /^[\w$]+$/.test(imported)) add(local && /^[\w$]+$/.test(local) ? local : imported, imported, index, module);
    }
  };
  for (const match of code.matchAll(new RegExp(`\\bimport\\s+(?:type\\s+)?([^;"']*?)\\s*\\bfrom\\s*${MODULE}`, "g"))) {
    const [, clause, module] = match;
    const namespace = /\*\s*as\s+([\w$]+)/.exec(clause);
    if (namespace) add(namespace[1], "*", match.index, module);
    const named = /\{([^}]*)\}/.exec(clause);
    if (named) braces(named[1], match.index, module, /\s+as\s+/);
    const byDefault = /^\s*([\w$]+)\s*(?:,|$)/.exec(clause);
    // The default export is the whole API object.
    if (byDefault) add(byDefault[1], "*", match.index, module);
  }
  // `const { List } = require("@raycast/api")`, and what a bundler writes:
  // `var import_api = require("@raycast/api")`, `__toESM(require(…))`.
  for (const match of code.matchAll(new RegExp(`\\b(?:const|let|var)\\s+(\\{[^}]*\\}|[\\w$]+)\\s*=\\s*(?:[\\w$]+\\()?require\\(${MODULE}\\)`, "g"))) {
    const [, target, module] = match;
    if (target.startsWith("{")) braces(target.slice(1, -1), match.index, module, /\s*:\s*/);
    else add(target, "*", match.index, module);
  }
  return imports;
}

/// The expressions that reach the API member `member` ("AI.ask") through
/// what the source imported, as regular expressions.
function reaching(imports: Imports, member: string): RegExp[] {
  const [root, ...rest] = member.split(".");
  const tail = rest.map((part) => `\\s*\\.\\s*${escape(part)}`).join("");
  const found: RegExp[] = [];
  for (const [local, imported] of imports.names) {
    if (imported === "*") found.push(new RegExp(`(?<![\\w$.])${escape(local)}\\s*\\.\\s*${escape(root)}${tail}(?![\\w$])`, "g"));
    else if (imported === root && rest.length) found.push(new RegExp(`(?<![\\w$.])${escape(local)}${tail}(?![\\w$])`, "g"));
  }
  return found;
}

/// The top-level attribute names of the JSX element opening at `start`
/// (the index just past its tag name), each with where it is.
function attributes(code: string, start: number): { name: string; index: number }[] {
  const found: { name: string; index: number }[] = [];
  let depth = 0;
  for (let i = start; i < code.length; i++) {
    const c = code[i];
    if (c === "{") depth++;
    else if (c === "}") depth--;
    else if (depth > 0) continue;
    else if (c === '"' || c === "'") i = code.indexOf(c, i + 1) < 0 ? code.length : code.indexOf(c, i + 1);
    else if (c === ">") break;
    else if (/[A-Za-z_$]/.test(c) && /\s/.test(code[i - 1] ?? " ")) {
      const name = /^[\w$-]+/.exec(code.slice(i))![0];
      found.push({ name, index: i });
      i += name.length - 1;
    }
  }
  return found;
}

/// What in one source file will not work on Heyo. `file` is only used to
/// name the findings; `source` is its text (TypeScript, JavaScript or a
/// built bundle).
export function checkSource(file: string, source: string): Finding[] {
  const tokens = lex(source);
  const code = withoutComments(source, tokens);
  const lineStarts = [0];
  for (let i = 0; i < code.length; i++) if (code[i] === "\n") lineStarts.push(i + 1);
  const lineOf = (index: number) => {
    let low = 0;
    let high = lineStarts.length - 1;
    while (low < high) {
      const middle = (low + high + 1) >> 1;
      if (lineStarts[middle] <= index) low = middle;
      else high = middle - 1;
    }
    return low;
  };

  const findings: Finding[] = [];
  const seen = new Set<string>();
  const report = (rule: Rule, severity: Severity, message: string, index: number) => {
    const line = lineOf(index);
    // One finding of a kind per line says it.
    const key = `${rule}:${line}:${message}`;
    if (seen.has(key)) return;
    seen.add(key);
    findings.push({ rule, severity, message, file, line: line + 1, column: index - lineStarts[line] + 1, excerpt: source.split("\n", line + 1)[line]?.trim().slice(0, 200) });
  };
  const each = (pattern: RegExp, found: (match: RegExpExecArray) => void) => {
    for (const match of code.matchAll(pattern)) found(match as RegExpExecArray);
  };

  // --- macOS, whatever the API ------------------------------------------

  each(/\b(?:runAppleScript|runAppleScriptSync|runJxa|osascript|osacompile)\b|run-applescript|\btell\s+application\s+\\?["']/g, (match) =>
    report("applescript", "error", "AppleScript runs only on macOS; on Heyo this fails", match.index),
  );
  each(/(?<![\w$.-])open\s+-[abRgnetjWFh]\b/g, (match) =>
    report("open-flags", "error", `\`${match[0]}\`: these are flags of macOS's open; use open() from the API, which works everywhere`, match.index),
  );
  each(/\bfrom\s*["']swift:[^"']*["']/g, (match) => report("swift", "error", "imports a Swift helper, which is built for and runs on macOS only", match.index));

  const strings = tokens.filter((token: Token) => token.kind === "string" || token.kind === "template");
  for (const token of strings) {
    const text = source.slice(token.start + 1, token.end - 1);
    for (const [pattern, name] of MAC_PATHS) {
      const match = pattern.exec(text);
      if (match) report("mac-path", "warning", `${name} exists only on macOS`, token.start + 1 + match.index + (match[0].length - match[0].trimStart().length));
    }

    // A program named by its macOS path is one wherever it stands.
    const absolute = /^\/(?:usr\/(?:local\/)?s?bin|s?bin|usr\/libexec)\/([\w.-]+)/.exec(text);
    const program = absolute?.[1] ?? /^([\w.-]+)(?=\s|$)/.exec(text)?.[1];
    const line = lineOf(token.start);
    // The call often starts a line or two above its arguments. Only so far
    // back, though: a minified bundle is one line of megabytes, and taking
    // the whole of it for every string in it does not end.
    const NEAR = 400;
    const around = () => code.slice(Math.max(lineStarts[Math.max(line - 2, 0)], token.start - NEAR), Math.min((lineStarts[line + 1] ?? code.length + 1) - 1, token.end + NEAR));
    const sameLine = () => code.slice(Math.max(lineStarts[line], token.start - NEAR), Math.min((lineStarts[line + 1] ?? code.length + 1) - 1, token.end + NEAR));
    if (program && MAC_PROGRAMS.has(program) && (absolute || RUNS.test(around()))) {
      // AppleScript has its own, plainer, finding.
      if (program !== "osascript" && program !== "osacompile") report("mac-program", "warning", `${program} is a program only macOS has`, token.start + 1);
    } else if (text === "open" && RUNS.test(sameLine()) && /["']-[abRgnetjWFh]["']/.test(sameLine())) {
      report("open-flags", "error", "open with macOS flags; use open() from the API, which works everywhere", token.start + 1);
    }
  }

  const alternative = /["'](?:linux|win32)["']/.test(code);
  if (!alternative) {
    each(/\b(?:process\s*\.\s*platform|(?:os\s*\.\s*)?platform\(\))\s*(===?|!==?)\s*["']darwin["']|["']darwin["']\s*(===?|!==?)\s*(?:process\s*\.\s*platform|(?:os\s*\.\s*)?platform\(\))/g, (match) => {
      const negated = (match[1] ?? match[2]).startsWith("!");
      report(
        "darwin-only",
        "warning",
        negated
          ? "turns away everything that is not macOS; nothing in this file is for Linux or Windows"
          : "this branch is for macOS, and nothing in this file is for Linux (process.platform is \"linux\") or Windows (\"win32\")",
        match.index,
      );
    });
  }

  // --- The API ----------------------------------------------------------

  const imports = apiImports(code);
  if (imports.names.size === 0) return findings;

  const usesOf = (member: string, found: (index: number) => void) => {
    const [root, ...rest] = member.split(".");
    // A name on its own is "used" where it is imported.
    if (rest.length === 0) for (const [local, imported] of imports.names) if (imported === root) found(imports.at.get(local)!);
    for (const pattern of reaching(imports, member)) each(pattern, (match) => found(match.index));
  };

  for (const [reason, members] of Object.entries(apiGaps as Record<string, string[]>)) {
    for (const member of members) {
      usesOf(member, (index) => report("api-missing", "error", `${member} is not in Heyo's API (${reason.toLowerCase()})`, index));
    }
  }
  for (const [member, { severity, message }] of Object.entries(UNSUPPORTED)) {
    usesOf(member, (index) => report("api-unsupported", severity, message, index));
  }

  for (const [component, props] of Object.entries(propGaps as Record<string, string[]>)) {
    const [root, ...rest] = component.split(".");
    const tail = rest.map((part) => `\\.${escape(part)}`).join("");
    for (const [local, imported] of imports.names) {
      const tag = imported === "*" ? `${escape(local)}\\.${escape(root)}${tail}` : imported === root ? `${escape(local)}${tail}` : null;
      if (!tag) continue;
      each(new RegExp(`<${tag}(?=[\\s/>])`, "g"), (match) => {
        for (const attribute of attributes(code, match.index + match[0].length)) {
          if (props.includes(attribute.name)) report("prop-ignored", "info", `${component} takes ${attribute.name} and ignores it on Heyo`, attribute.index);
        }
      });
    }
  }
  return findings;
}

/// The modules of Heyo's (those `heyo/members.ts` declares) that a source
/// uses from `@heyo/api`.
export function heyoModulesUsed(source: string): ModuleName[] {
  const imports = apiImports(withoutComments(source));
  const used = new Set<ModuleName>();
  const own = Object.keys(modules) as ModuleName[];
  for (const [local, imported] of imports.names) {
    if (!imports.heyo.has(local)) continue;
    if (imported === "*") {
      for (const module of own) if (new RegExp(`(?<![\\w$.])${escape(local)}\\s*\\.\\s*${module}\\b`).test(source)) used.add(module);
    } else if (own.some((module) => module === imported)) used.add(imported as ModuleName);
  }
  return [...used].sort();
}

/// What the manifest itself says that will not do. `exists` answers for a
/// path relative to the extension's directory.
export function checkManifest(manifest: unknown, exists: (file: string) => boolean): Finding[] {
  const findings: Finding[] = validateManifest(manifest, exists).problems.map((problem) => ({
    rule: "manifest",
    severity: problem.severity,
    message: problem.message,
    file: "package.json",
  }));
  const m = (typeof manifest === "object" && manifest !== null ? manifest : {}) as Record<string, any>;
  if (Array.isArray(m.platforms) && m.platforms.length > 0) {
    const named = m.platforms.map((p: unknown) => String(p).toLowerCase());
    if (!named.includes("linux") && !named.includes("windows")) {
      findings.push({ rule: "platforms", severity: "info", file: "package.json", message: `"platforms" is ${JSON.stringify(m.platforms)}: the author did not have Linux or Windows in mind` });
    }
  }
  if ((Array.isArray(m.tools) && m.tools.length > 0) || m.ai !== undefined) {
    findings.push({ rule: "ai-tools", severity: "info", file: "package.json", message: "declares AI tools; Heyo runs the extension's commands and not its tools" });
  }
  return findings;
}

function isMacProgram(file: string) {
  try {
    const head = Buffer.alloc(4);
    const fd = fs.openSync(file, "r");
    try {
      fs.readSync(fd, head, 0, 4, 0);
    } finally {
      fs.closeSync(fd);
    }
    return ["cafebabe", "bebafeca", "feedface", "cefaedfe", "feedfacf", "cffaedfe"].includes(head.toString("hex"));
  } catch {
    return false;
  }
}

/// Files under `dir` (relative to `base`), to a depth, not following links.
function filesUnder(base: string, dir: string, depth: number, limit = 2000): string[] {
  const out: string[] = [];
  const walk = (relative: string, left: number) => {
    let entries: fs.Dirent[];
    try {
      entries = fs.readdirSync(path.join(base, relative), { withFileTypes: true });
    } catch {
      return;
    }
    for (const entry of entries) {
      if (out.length >= limit) return;
      const child = `${relative}/${entry.name}`;
      if (entry.isDirectory()) {
        if (left > 0 && entry.name !== "node_modules") walk(child, left - 1);
      } else if (entry.isFile()) out.push(child);
    }
  };
  walk(dir, depth);
  return out;
}

/// The packages the extension ships (its dependencies and theirs) that
/// hold compiled code. What the runtime provides is not shipped.
function nativeAddons(dir: string, manifest: Record<string, any>): string[] {
  const native = new Set<string>();
  const visited = new Set<string>();
  const provided = new Set([...API_MODULES, "react"]);
  const visit = (name: string, from: string) => {
    if (provided.has(name)) return;
    // As Node resolves: the nearest node_modules upwards, not past the extension.
    let at = from;
    let found: string | undefined;
    for (;;) {
      const candidate = path.join(at, "node_modules", name);
      if (fs.existsSync(path.join(candidate, "package.json"))) {
        found = candidate;
        break;
      }
      if (path.relative(dir, at) === "" || path.dirname(at) === at) break;
      at = path.dirname(at);
    }
    if (!found) return;
    let real: string;
    try {
      real = fs.realpathSync(found);
    } catch {
      return;
    }
    if (visited.has(real)) return;
    visited.add(real);
    let pkg: Record<string, any>;
    try {
      pkg = JSON.parse(fs.readFileSync(path.join(real, "package.json"), "utf8"));
    } catch {
      return;
    }
    const builds = pkg.gypfile === true || fs.existsSync(path.join(real, "binding.gyp"));
    if (builds || filesUnder(real, ".", 3).some((file) => file.endsWith(".node"))) native.add(name);
    for (const dependency of Object.keys({ ...pkg.dependencies, ...pkg.optionalDependencies })) visit(dependency, real);
  };
  for (const name of Object.keys(manifest.dependencies ?? {})) visit(name, dir);
  return [...native].sort();
}

/// A built command with the code of its dependencies blanked out, every
/// character of the rest still where it was. A bundler that does not
/// minify (esbuild, and so `heyo build` and `ray build`) writes the path of
/// each file above its code; what is under `node_modules` is libraries
/// carried whole, used or not. `@raycast/utils` alone would have every
/// extension that bundles it reported for AppleScript.
export function ownCode(bundle: string): string {
  let foreign = false;
  return bundle
    .split("\n")
    .map((line) => {
      const file = /^\/\/ (\S+\.(?:[cm]?[jt]sx?|json))$/.exec(line)?.[1];
      if (file) foreign = file.includes("node_modules/");
      return foreign ? line.replace(/[^\n]/g, " ") : line;
    })
    .join("\n");
}

/// Everything that will not work on Heyo in the extension in `dir`: a
/// source directory (its `src/` and other sources are read), or a built
/// extension as Heyo installs them (its `.js` files are, less the
/// libraries bundled into them).
/// Throws if `dir` has no readable `package.json`.
export function checkExtension(dir: string): Report {
  dir = path.resolve(dir);
  const { manifest } = readPackage(dir);
  const exists = existsIn(dir);
  // Built: no sources, and the commands' files beside the manifest.
  const names: string[] = (Array.isArray(manifest?.commands) ? manifest.commands : []).map((c: any) => String(c?.name));
  const built = !fs.existsSync(path.join(dir, "src")) && names.some((name) => exists(`${name}.js`));
  const source = /^src\/([\w-]+)\.tsx$/;
  const findings = checkManifest(manifest, (file) => exists(file) || (built && source.test(file) && exists(`${source.exec(file)![1]}.js`)));

  const files = sourceFiles(dir);
  /// What is used of Heyo's, with the permissions any one of which will do.
  const used = new Map<string, readonly string[]>();
  for (const file of files) {
    let source: string;
    try {
      source = fs.readFileSync(path.join(dir, file), "utf8");
    } catch {
      continue;
    }
    findings.push(...checkSource(file, built ? ownCode(source) : source));
    for (const module of heyoModulesUsed(source)) used.set(module, permissionsOf(module));
  }

  const declared: unknown[] = Array.isArray(manifest?.heyo?.permissions) ? manifest.heyo.permissions : [];
  for (const [uses, needs] of used) {
    if (!needs.some((permission) => declared.includes(permission))) {
      findings.push({
        rule: "permission-missing",
        severity: "warning",
        file: "package.json",
        message: `uses ${uses}, which needs the ${needs.map((p) => `"${p}"`).join(" or ")} permission; add it to "heyo.permissions"`,
      });
    }
  }

  if (fs.existsSync(path.join(dir, "swift"))) {
    findings.push({ rule: "swift", severity: "error", file: "swift", message: "has Swift helper code, which is built for and runs on macOS only" });
  }
  for (const file of filesUnder(dir, "assets", 4)) {
    if (isMacProgram(path.join(dir, file))) {
      findings.push({ rule: "mac-binary", severity: "error", file: file.replace(/^\.?\//, ""), message: "is a macOS program and cannot run on Linux or Windows" });
    }
  }
  if (Object.keys(manifest?.dependencies ?? {}).length > 0 && !fs.existsSync(path.join(dir, "node_modules")) && fs.existsSync(path.join(dir, "src"))) {
    findings.push({ rule: "native-addon", severity: "info", message: "dependencies are not installed (no node_modules), so they were not looked at for compiled code" });
  }
  for (const name of nativeAddons(dir, manifest ?? {})) {
    findings.push({
      rule: "native-addon",
      severity: "warning",
      file: "package.json",
      message: `${name} has compiled code: it works only where it was built for, and a built command cannot carry it`,
    });
  }

  const order: Severity[] = ["error", "warning", "info"];
  findings.sort(
    (a, b) => order.indexOf(a.severity) - order.indexOf(b.severity) || (a.file ?? "").localeCompare(b.file ?? "") || (a.line ?? 0) - (b.line ?? 0),
  );
  const summary = { error: 0, warning: 0, info: 0 };
  for (const finding of findings) summary[finding.severity]++;
  return {
    extension: { name: String(manifest?.name ?? ""), title: String(manifest?.title ?? manifest?.name ?? ""), dir },
    files: files.length,
    findings,
    summary,
  };
}

/// The report for a person to read.
export function formatReport(report: Report, color = false): string {
  const paint = (code: number, text: string) => (color ? `\x1b[${code}m${text}\x1b[0m` : text);
  const label: Record<Severity, string> = { error: paint(31, "error  "), warning: paint(33, "warning"), info: paint(36, "info   ") };
  const lines = [`${paint(1, report.extension.title || report.extension.name)} — ${report.files} source file${report.files === 1 ? "" : "s"} read`];
  if (report.findings.length === 0) {
    lines.push("", "Nothing found that would not work on Heyo.");
    return lines.join("\n") + "\n";
  }
  for (const finding of report.findings) {
    const where = finding.file ? `${finding.file}${finding.line ? `:${finding.line}:${finding.column}` : ""}` : "";
    lines.push("", `${label[finding.severity]} ${paint(2, `[${finding.rule}]`)} ${where}`, `        ${finding.message}`);
    if (finding.excerpt) lines.push(paint(2, `        | ${finding.excerpt}`));
  }
  const { error, warning, info } = report.summary;
  const count = (n: number, word: string) => `${n} ${word}${n === 1 ? "" : "s"}`;
  lines.push("", `${count(error, "error")}, ${count(warning, "warning")}, ${count(info, "note")}.`);
  return lines.join("\n") + "\n";
}

/// The names in the gap lists, for tests and for reports that want to say
/// what was looked for.
export const knownGaps = { api: flat(apiGaps as Record<string, string[]>), props: propGaps as Record<string, string[]> };
