// The text Heyo shows, gathered for translating: `pnpm i18n` reads every
// string marked in the sources into `locales/heyo.pot`, and brings each
// language's `locales/<language>/LC_MESSAGES/heyo.po` up to it (new strings
// empty, strings no longer used gone, translations kept).
//
// A string is marked by the call it is in, the English text being the key:
//
//   Rust        t("…")  tf("… {0}", &[…])  tc("context", "…")  tn("one", "{n} many", n, &[…])
//               marked("…") in a table, shown later with translate(…)
//   TypeScript  t("…", …)  tc("context", "…", …)  tn("one", "{n} many", n, …)
//   Slint       @tr("…", …)  @tr("context" => "…", …)  @tr("one" | "{n} many" % n, …)
//
// The text must be a literal, since it is read here and not run. Placeholders
// are Slint's in all three: `{}` for the next argument, `{0}` for one by its
// place, `{n}` for the number a plural is chosen by.
//
// `i18n.test.mjs` holds the template to the sources and each translation to
// its template.

import { existsSync, readdirSync, readFileSync, writeFileSync } from "node:fs";
import { dirname, join, relative } from "node:path";
import { fileURLToPath } from "node:url";

export const ROOT = join(dirname(fileURLToPath(import.meta.url)), "..");
export const LOCALES = join(ROOT, "locales");
export const TEMPLATE = join(LOCALES, "heyo.pot");

/// Where marked strings are looked for, and how each kind of file is read.
const SOURCES = [
  { under: "src-tauri/src", ends: [".rs"], read: rust },
  { under: "src-tauri/src", ends: [".slint"], read: slint },
  { under: "src", ends: [".ts", ".tsx"], read: script },
  { under: "runtime/src", ends: [".ts", ".tsx"], read: script },
  { under: "shared", ends: [".ts"], read: script },
];

/// What each marking call takes before its arguments: which of its leading
/// strings is the context, the text and the plural.
const CALLS = {
  t: ["text"],
  tf: ["text"],
  tc: ["context", "text"],
  tn: ["text", "plural"],
  marked: ["text"],
};

function files(directory, ends) {
  return readdirSync(directory, { withFileTypes: true })
    .sort((a, b) => (a.name < b.name ? -1 : 1))
    .flatMap((entry) => {
      const path = join(directory, entry.name);
      if (entry.isDirectory()) return entry.name === "bindings" ? [] : files(path, ends);
      return ends.some((end) => entry.name.endsWith(end)) && !/\.test\.tsx?$/.test(entry.name) ? [path] : [];
    });
}

const ESCAPES = { n: "\n", t: "\t", r: "\r", 0: "\0" };

/// The string literal that starts at `at` (on its quote): what it says and
/// where it ends. Undefined for one that is not plain text (a template
/// with something in it, or a quote that was no string's: one of a
/// script's ends with its line).
function literal(source, at, lines = true) {
  const quote = source[at];
  let said = "";
  for (let i = at + 1; i < source.length; i++) {
    const c = source[i];
    if (c === quote) return { said, end: i + 1 };
    if (quote === "`" ? c === "$" && source[i + 1] === "{" : c === "\n" && !lines) return undefined;
    if (c !== "\\") {
      said += c;
      continue;
    }
    const next = source[++i];
    if (next === "u") {
      const braced = source[i + 1] === "{";
      const close = braced ? source.indexOf("}", i) : i + 4;
      said += String.fromCodePoint(parseInt(source.slice(i + (braced ? 2 : 1), braced ? close : close + 1), 16));
      i = close;
    } else if (next === "\n") {
      // Rust: the line goes on, without what the next one is indented by.
      while (/\s/.test(source[i + 1])) i++;
    } else {
      said += ESCAPES[next] ?? next;
    }
  }
  return undefined;
}

/// Walks a source's code, past its comments and its strings, and hands
/// `found` every place a word begins outside them. From `from`, to the
/// brace that closes what a template put in itself when `within` one;
/// returns where it stopped.
function walk(source, options, found, from = 0, within = false) {
  const { quotes, rust, script } = options;
  let open = 0;
  for (let i = from; i < source.length; ) {
    const c = source[i];
    if (within && (c === "{" || c === "}")) {
      open += c === "{" ? 1 : -1;
      if (open < 0) return i + 1;
      i++;
    } else if (c === "/" && source[i + 1] === "/") {
      i = source.indexOf("\n", i);
      if (i < 0) break;
    } else if (c === "/" && source[i + 1] === "*") {
      i = source.indexOf("*/", i) + 2;
      if (i < 2) break;
    } else if (script && c === "/" && /(^|[(,=:[!&|?{};]|\breturn)\s*$/.test(source.slice(Math.max(0, i - 40), i))) {
      // A regular expression, which may have a quote in it: to its end
      // on this line.
      const expression = /^\/(\\.|\[(\\.|[^\]\n])*\]|[^/\\\n])+\//.exec(source.slice(i, source.indexOf("\n", i) >>> 0));
      i += expression ? expression[0].length : 1;
    } else if (rust && c === "'") {
      // A character ('"', '\'') or a lifetime: never a string.
      const character = /^'(\\.[^']*|[^\\'])'/.exec(source.slice(i, i + 12));
      i += character ? character[0].length : 1;
    } else if (rust && c === "r" && /^r#*"/.test(source.slice(i, i + 8)) && !/\w/.test(source[i - 1] ?? "")) {
      const hashes = /^r(#*)/.exec(source.slice(i, i + 8))[1];
      const end = source.indexOf(`"${hashes}`, i + hashes.length + 2);
      i = end < 0 ? source.length : end + hashes.length + 1;
    } else if (quotes.includes(c)) {
      i = literal(source, i, !script)?.end ?? (c === "`" ? templated(source, i, options, found) : i + 1);
    } else if (/[A-Za-z_@]/.test(c) && !/[\w.$]/.test(source[i - 1] ?? "")) {
      const word = /^@?\w+/.exec(source.slice(i, i + 40))?.[0] ?? c;
      i = found(word, i, i + word.length) ?? i + word.length;
    } else {
      i++;
    }
  }
  return source.length;
}

/// Walks the code a template has in it (`${…}`), and returns where the
/// template ends.
function templated(source, at, options, found) {
  for (let i = at + 1; i < source.length; ) {
    if (source[i] === "\\") i += 2;
    else if (source[i] === "`") return i + 1;
    else if (source[i] === "$" && source[i + 1] === "{") i = walk(source, options, found, i + 2, true);
    else i++;
  }
  return source.length;
}

const space = (source, at) => at + /^\s*/.exec(source.slice(at))[0].length;

function line(source, at) {
  return source.slice(0, at).split("\n").length;
}

/// The calls of `CALLS` in a source of Rust or TypeScript.
function calls(source, path, options) {
  const out = [];
  walk(source, options, (word, at, after) => {
    const takes = Object.hasOwn(CALLS, word) && CALLS[word];
    if (!takes ||source[after] !== "(" || /\b(fn|function)\s+$/.test(source.slice(Math.max(0, at - 12), at))) return undefined;
    const message = {};
    let i = after + 1;
    for (const part of takes) {
      i = space(source, i);
      const string = options.quotes.includes(source[i]) ? literal(source, i, !options.script) : undefined;
      if (!string) throw new Error(`${path}:${line(source, at)}: ${word}(…) takes its ${part} as a literal string`);
      message[part] = string.said;
      i = space(source, string.end);
      if (source[i] === ",") i++;
    }
    out.push(message);
    return i;
  });
  return out;
}

function rust(source, path) {
  return calls(source, path, { quotes: ['"'], rust: true });
}

function script(source, path) {
  return calls(source, path, { quotes: ['"', "'", "`"], script: true });
}

/// `@tr(…)` in Slint's language.
function slint(source, path) {
  const out = [];
  walk(source, { quotes: ['"'] }, (word, at, after) => {
    if (word !== "@tr" || source[after] !== "(") return undefined;
    const fail = () => {
      throw new Error(`${path}:${line(source, at)}: @tr(…) takes literal strings`);
    };
    const string = (i) => (source[i] === '"' && literal(source, i)) || fail();
    let first = string(space(source, after + 1));
    let i = space(source, first.end);
    const message = {};
    if (source.startsWith("=>", i)) {
      message.context = first.said;
      first = string(space(source, i + 2));
      i = space(source, first.end);
    }
    message.text = first.said;
    if (source[i] === "|") {
      const plural = string(space(source, i + 1));
      message.plural = plural.said;
      i = plural.end;
    }
    out.push(message);
    return i;
  });
  return out;
}

/// What makes two messages one: gettext's own key.
export const key = (message) => (message.context ? `${message.context}\u0004${message.text}` : message.text);

/// The sources: each file's name, what it says, and how it is read. Of a
/// Rust file, all but its tests: they are its end, and what they say is
/// not shown.
export function sources() {
  return SOURCES.flatMap(({ under, ends, read }) =>
    files(join(ROOT, under), ends).map((path) => {
      const text = readFileSync(path, "utf8");
      const tests = read === rust ? text.search(/^#\[cfg\(test\)\]\s*mod tests\b/m) : -1;
      return { file: relative(ROOT, path).replaceAll("\\", "/"), text: tests < 0 ? text : text.slice(0, tests), read };
    }),
  );
}

/// Every marked string of the sources, in the order of the files and of
/// each file's lines, once each, with the files it is in.
export function extract() {
  const messages = new Map();
  for (const { file, text, read } of sources()) {
    for (const found of read(text, file)) {
      const message = messages.get(key(found)) ?? { ...found, files: [] };
      if ((message.plural ?? "") !== (found.plural ?? "")) throw new Error(`${file}: "${found.text}" is marked with a plural in one place and without, or with another, elsewhere`);
      if (!message.files.includes(file)) message.files.push(file);
      messages.set(key(found), message);
    }
  }
  return [...messages.values()];
}

// --- The files -----------------------------------------------------------------

const quoted = (text) => `"${text.replace(/[\\"\n\t]/g, (c) => ({ "\\": "\\\\", '"': '\\"', "\n": "\\n", "\t": "\\t" })[c])}"`;

/// One message as a `.po` file has it. `forms` are its translations: one,
/// or one for each of the language's plural forms.
function entry(message, forms) {
  const lines = message.files.map((file) => `#: ${file}`);
  if (message.context) lines.push(`msgctxt ${quoted(message.context)}`);
  lines.push(`msgid ${quoted(message.text)}`);
  if (message.plural === undefined) lines.push(`msgstr ${quoted(forms[0] ?? "")}`);
  else lines.push(`msgid_plural ${quoted(message.plural)}`, ...forms.map((form, i) => `msgstr[${i}] ${quoted(form)}`));
  return lines.join("\n");
}

const header = (fields) => `msgid ""\nmsgstr ""\n${Object.entries(fields).map(([name, value]) => quoted(`${name}: ${value}\n`)).join("\n")}`;

/// The template: every message, untranslated.
export function template(messages) {
  const fields = { "Project-Id-Version": "heyo", "MIME-Version": "1.0", "Content-Type": "text/plain; charset=UTF-8", "Content-Transfer-Encoding": "8bit" };
  return [header(fields), ...messages.map((message) => entry(message, message.plural === undefined ? [""] : ["", ""]))].join("\n\n") + "\n";
}

/// A `.po` file read: its header's fields, and its messages with their
/// translations (`forms`).
export function parse(po) {
  const messages = [];
  let message, into;
  const said = (text) => literal(text, text.indexOf('"'))?.said ?? "";
  for (const row of po.split("\n")) {
    const [, name, index] = /^(msgctxt|msgid_plural|msgid|msgstr)(?:\[(\d+)\])? /.exec(row) ?? [];
    if (name === "msgctxt" || (name === "msgid" && !message?.open)) {
      message = { forms: [], open: true };
      messages.push(message);
    }
    if (name === "msgid") message.open = false;
    if (name) {
      into = { msgctxt: "context", msgid: "text", msgid_plural: "plural" }[name] ?? Number(index ?? 0);
      if (typeof into === "number") message.forms[into] = said(row);
      else message[into] = said(row);
    } else if (row.startsWith('"') && message) {
      if (typeof into === "number") message.forms[into] += said(row);
      else message[into] += said(row);
    } else if (/^#,.*\bfuzzy\b/.test(row)) {
      throw new Error("a fuzzy translation: decide it, and take the flag away");
    }
  }
  const head = messages[0]?.text === "" ? messages.shift() : undefined;
  const fields = Object.fromEntries((head?.forms[0] ?? "").split("\n").filter(Boolean).map((field) => [field.slice(0, field.indexOf(":")), field.slice(field.indexOf(":") + 1).trim()]));
  return { fields, messages };
}

/// How many forms a language's plural has, from its header.
export const forms = (fields) => Number(/nplurals\s*=\s*(\d+)/.exec(fields["Plural-Forms"] ?? "")?.[1] ?? 2);

/// A language's file brought up to the template: the template's messages
/// in its order, each with the translation the file had.
export function merged(po, messages) {
  const { fields, messages: had } = parse(po);
  const known = new Map(had.map((message) => [key(message), message]));
  const count = forms(fields);
  const entries = messages.map((message) => {
    const was = known.get(key(message));
    const kept = was && (was.plural === undefined) === (message.plural === undefined) ? was.forms : [];
    return entry(message, message.plural === undefined ? kept : Array.from({ length: count }, (_, i) => kept[i] ?? ""));
  });
  return [header(fields), ...entries].join("\n\n") + "\n";
}

/// The placeholders of a text, sorted: `{}` is the next argument's place.
export function placeholders(text) {
  let next = 0;
  return [...text.matchAll(/\{([^{}]*)\}/g)].map(([, name]) => (name === "" ? String(next++) : name)).sort();
}

/// Each language's file: its name and its path.
export function languages() {
  return readdirSync(LOCALES, { withFileTypes: true })
    .filter((entry) => entry.isDirectory())
    .map((entry) => ({ language: entry.name, path: join(LOCALES, entry.name, "LC_MESSAGES", "heyo.po") }))
    .filter(({ path }) => existsSync(path));
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  const messages = extract();
  writeFileSync(TEMPLATE, template(messages));
  console.log(`${relative(ROOT, TEMPLATE)}: ${messages.length} strings`);
  for (const { path } of languages()) {
    const now = merged(readFileSync(path, "utf8"), messages);
    writeFileSync(path, now);
    const left = parse(now).messages.filter((message) => message.forms.some((form) => !form)).length;
    console.log(`${relative(ROOT, path)}: ${left} to translate`);
  }
}
