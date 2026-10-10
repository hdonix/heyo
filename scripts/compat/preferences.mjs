//! What a command must be given before it runs — required preferences and
//! arguments — and harmless made-up values for them, so that a launch gets
//! past Heyo's own "fill these in first" form to the extension's code.

import { APPLICATIONS } from "./host.mjs";

/// As `runtime/src/context.ts` has it: the value a preference has before
/// the user sets one.
function defaultValue(pref) {
  if (pref.default !== undefined) return pref.default;
  if (pref.type === "checkbox") return false;
  if (pref.type === "dropdown") return pref.data?.[0]?.value;
  return undefined;
}

/// The preferences Heyo asks for before it runs `command`: required, and
/// with no default. Each says whether it belongs to the extension or to
/// the command.
export function requiredPreferences(manifest, command) {
  const cmd = (manifest.commands ?? []).find((c) => c.name === command);
  const scoped = [
    ...(manifest.preferences ?? []).map((pref) => ({ pref, scope: "extension" })),
    ...(cmd?.preferences ?? []).map((pref) => ({ pref, scope: "command" })),
  ];
  return scoped
    .filter(({ pref }) => pref.required && (defaultValue(pref) === undefined || defaultValue(pref) === ""))
    .map(({ pref, scope }) => ({ name: pref.name, type: pref.type, title: pref.title ?? pref.label ?? pref.name, scope, secret: pref.type === "password" }));
}

/// A value that has the shape the preference seems to want and means
/// nothing. A secret gets one too (it is not a secret): the command then
/// runs as far as the service that turns it away.
export function dummyValue(pref, home = "/home/user") {
  // "serverUrl" and "server_url" are the words "server url".
  const hint = `${pref.name} ${pref.title ?? ""} ${pref.placeholder ?? ""}`.replace(/([a-z])([A-Z])/g, "$1 $2").replace(/[_-]/g, " ").toLowerCase();
  switch (pref.type) {
    case "password":
      return "heyo-compat-dummy-secret";
    case "checkbox":
      return false;
    case "dropdown":
      return pref.data?.[0]?.value ?? "";
    case "appPicker":
      return APPLICATIONS[0].path;
    case "file":
      return `${home}/Documents/notes.txt`;
    case "directory":
      return `${home}/Documents`;
    default:
      if (/\b(url|uri|host|server|instance|domain|endpoint|site|workspace)\b|url$/.test(hint)) return "https://example.invalid";
      if (/e-?mail/.test(hint)) return "user@example.com";
      if (/\b(path|dir|directory|folder|vault|file|location)\b|path$|dir$/.test(hint)) return `${home}/Documents`;
      if (/\b(port|limit|count|max|min|number|interval|size|days|hours|minutes|seconds)\b/.test(hint)) return "10";
      return "heyo-compat";
  }
}

/// The preferences file for an extension (`<config>/preferences/<name>.json`)
/// with every required preference of `command` given a dummy value, and
/// which ones were given.
export function dummyPreferences(manifest, command, home) {
  const cmd = (manifest.commands ?? []).find((c) => c.name === command);
  const required = requiredPreferences(manifest, command);
  const stored = { extension: {}, commands: {} };
  const find = (scope, name) => (scope === "extension" ? manifest.preferences : cmd?.preferences).find((p) => p.name === name);
  for (const { name, scope } of required) {
    const value = dummyValue(find(scope, name), home);
    if (scope === "extension") stored.extension[name] = value;
    else (stored.commands[command] ??= {})[name] = value;
  }
  return { stored, required };
}

/// Something that could have been typed for one argument.
export function dummyArgument(arg) {
  if (arg.type === "dropdown") return String(arg.data?.[0]?.value ?? "");
  const hint = `${arg.name} ${arg.placeholder ?? ""}`.toLowerCase();
  if (/hour|minute|second|number|count|amount|port|size|length|quantity|days|\bnum\b|\bn\b|\bid\b/.test(hint)) return "1";
  if (/\burl\b|link|website/.test(hint)) return "https://example.com";
  return "test";
}

/// Something for each argument the command takes, as if typed in the
/// search bar.
export function dummyArguments(command) {
  const args = command?.arguments ?? [];
  if (args.length === 0) return undefined;
  return Object.fromEntries(args.map((arg) => [arg.name, dummyArgument(arg)]));
}
