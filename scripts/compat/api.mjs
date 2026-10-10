//! Which parts of `@raycast/api` a built command reaches for, and which of
//! them Heyo's runtime does not have. What Heyo has is not read from its
//! sources: a small command is launched in the runtime bundle itself and
//! writes down the API object it was given (`PROBE`), so the list is the
//! one extensions really meet.

/// A no-view command that writes the shape of the API it is given to
/// `$HOME/api.json`: an object per namespace, component or function (a
/// function is marked `$fn`), `1` for any other value, and `$open` for
/// what answers to every name (a Proxy) or loops back on itself.
export const PROBE = `
const api = require("@raycast/api");
const util = require("node:util");
const fs = require("node:fs");
const path = require("node:path");
const SKIP = new Set(["length", "name", "prototype", "arguments", "caller"]);
const above = new Set();
function walk(value, depth) {
  if (value === null || (typeof value !== "object" && typeof value !== "function")) return 1;
  if (Array.isArray(value)) return 1;
  if (util.types.isProxy(value) || depth > 7 || above.has(value)) return { $open: 1 };
  above.add(value);
  const out = typeof value === "function" ? { $fn: 1 } : {};
  for (const key of Object.getOwnPropertyNames(value)) {
    if (typeof value === "function" && SKIP.has(key)) continue;
    let child;
    try { child = value[key]; } catch { out[key] = 1; continue; }
    out[key] = walk(child, depth + 1);
  }
  above.delete(value);
  return out;
}
exports.default = async () => {
  fs.writeFileSync(path.join(process.env.HOME, "api.json"), JSON.stringify(walk(api, 0)));
};
`;

export const PROBE_MANIFEST = { name: "heyo-compat-probe", title: "Probe", commands: [{ name: "probe", title: "Probe", mode: "no-view" }] };

const escape = (text) => text.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
/// What every function has, whatever it is.
const OF_FUNCTIONS = new Set(["call", "apply", "bind", "name", "length", "toString", "displayName", "prototype"]);
/// Missing ones of these show less, and break nothing.
const COSMETIC = /^(?:Icon|Color|Image\.Mask|Keyboard\.Shortcut\.Common)\./;

/// The members of the API a built command names: `List.Item.Detail`,
/// `showToast`, … Each with whether the name it was reached through can be
/// trusted: a bundler's `import_api` is the API wherever it stands, a
/// minifier's one letter is reused for other things.
export function usedMembers(source) {
  const locals = new Set();
  for (const match of source.matchAll(/([\w$]+)\s*=\s*(?:[\w$]+\()?require\(["']@raycast\/api["']\)/g)) locals.add(match[1]);
  const used = new Map();
  for (const local of locals) {
    const trusted = local.length >= 3;
    for (const match of source.matchAll(new RegExp(`(?<![\\w$.])${escape(local)}((?:\\.[A-Za-z_$][\\w$]*){1,8})`, "g"))) {
      const chain = match[1].slice(1).replace(/^default\./, "");
      if (chain && chain !== "default") used.set(chain, (used.get(chain) ?? false) || trusted);
    }
  }
  return used;
}

/// The members among `used` that `surface` (what `PROBE` wrote) does not
/// have, each named up to the first part that is missing.
export function missingMembers(used, surface) {
  const missing = new Set();
  for (const [chain, trusted] of used) {
    const parts = chain.split(".");
    let node = surface;
    for (let i = 0; i < parts.length; i++) {
      // A plain value, or something that answers to any name.
      if (node === 1 || node.$open) break;
      if (Object.hasOwn(node, parts[i])) {
        node = node[parts[i]];
        continue;
      }
      if (node.$fn && OF_FUNCTIONS.has(parts[i])) break;
      // Through an untrusted name only what starts as the API does counts.
      if (trusted || i > 0) missing.add(parts.slice(0, i + 1).join("."));
      break;
    }
  }
  return [...missing].sort();
}

/// Missing members split into those that break a command and those that
/// only show less (an icon, a colour).
export function sortMissing(missing) {
  return { breaking: missing.filter((name) => !COSMETIC.test(name)), cosmetic: missing.filter((name) => COSMETIC.test(name)) };
}

/// The applications a command's AppleScript (or JXA) speaks to: what it
/// would need an equivalent for anywhere but on macOS.
export function appleScriptTargets(source) {
  const targets = new Set();
  for (const match of source.matchAll(/tell\s+(?:application|app)\s+(?:id\s+)?\\?["'`]([^"'`\\\n$]{1,60})\\?["'`]/gi)) targets.add(match[1].trim());
  for (const match of source.matchAll(/(?<![\w$.])Application\(\s*\\?["']([^"'\\\n$]{1,60})\\?["']\s*\)/g)) targets.add(match[1].trim());
  // `tell application "${name}"`: whichever application the user chose.
  if (/tell\s+(?:application|app)\s+(?:id\s+)?\\?["'`]?\$\{/i.test(source)) targets.add("(named at run time)");
  return [...targets].filter(Boolean).sort();
}

/// The applications AppleScript speaks to, with the extensions and
/// commands that do, the most first.
export function rankTargets(extensions) {
  const table = new Map();
  for (const extension of extensions) {
    for (const command of extension.commands ?? []) {
      for (const name of command.api?.tells ?? []) {
        const row = table.get(name) ?? { name, extensions: new Set(), commands: 0 };
        row.extensions.add(extension.name);
        row.commands++;
        table.set(name, row);
      }
    }
  }
  return [...table.values()].map((row) => ({ ...row, extensions: [...row.extensions] })).sort((a, b) => b.extensions.length - a.extensions.length || b.commands - a.commands || a.name.localeCompare(b.name));
}

/// Every missing member with the extensions and commands that name it,
/// and how many of those commands did not launch cleanly.
export function rankApi(extensions) {
  const table = new Map();
  for (const extension of extensions) {
    for (const command of extension.commands ?? []) {
      for (const [kind, names] of Object.entries(command.api ?? {})) {
        if (kind !== "breaking" && kind !== "cosmetic") continue;
        for (const name of names) {
          const row = table.get(name) ?? { name, cosmetic: kind === "cosmetic", extensions: new Set(), commands: 0, notClean: 0 };
          row.extensions.add(extension.name);
          row.commands++;
          if (command.verdict?.status !== "works") row.notClean++;
          table.set(name, row);
        }
      }
    }
  }
  return [...table.values()]
    .map((row) => ({ ...row, extensions: [...row.extensions] }))
    .sort((a, b) => Number(a.cosmetic) - Number(b.cosmetic) || b.extensions.length - a.extensions.length || b.commands - a.commands || a.name.localeCompare(b.name));
}
