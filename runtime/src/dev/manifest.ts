//! An extension's manifest (`package.json`) as the developer tools read it:
//! what a source directory must have for Heyo to build and load it.

/// The names both of which the runtime answers with its own API object.
export const API_MODULES = ["@raycast/api", "@heyo/api"];
/// What a built command leaves for the runtime to provide (`main.ts`).
export const PROVIDED = [...API_MODULES, "react", "react/jsx-runtime", "react/jsx-dev-runtime"];

export const MODES = ["view", "no-view", "menu-bar"];
/// Raycast's convention: a command is `src/<name>` with one of these endings.
export const ENTRY_EXTENSIONS = [".tsx", ".ts", ".jsx", ".js"];
/// The major version of the Heyo API this Heyo provides.
export const API_VERSION = 1;
/// The permissions an extension may declare (docs/extension-api.md, 5.1).
export const PERMISSIONS = [
  "windows",
  "screen.capture",
  "screen.text",
  "microphone",
  "clipboard.history",
  "memory.read",
  "memory.write",
  "files.search",
  "system",
  "input.paste",
  "ai",
  "search.provider",
];

export interface Problem {
  severity: "error" | "warning";
  message: string;
}

export interface CommandEntry {
  name: string;
  /// The source file, relative to the extension's directory, `/`-separated.
  entry: string;
}

export interface Validation {
  problems: Problem[];
  /// The commands that can be built: those with a name and a source file.
  commands: CommandEntry[];
}

const isRecord = (value: unknown): value is Record<string, unknown> => typeof value === "object" && value !== null && !Array.isArray(value);
const isText = (value: unknown): value is string => typeof value === "string" && value.trim() !== "";

/// An icon in the manifest is a file under `assets/`; Raycast also takes
/// the `@dark` variant alone.
function iconExists(icon: string, exists: (file: string) => boolean) {
  const dot = icon.lastIndexOf(".");
  const dark = dot > 0 ? `${icon.slice(0, dot)}@dark${icon.slice(dot)}` : `${icon}@dark`;
  return exists(`assets/${icon}`) || exists(`assets/${dark}`);
}

/// Checks a manifest against the files of its directory. `exists` answers
/// for a path relative to that directory, `/`-separated.
export function validateManifest(manifest: unknown, exists: (file: string) => boolean): Validation {
  const problems: Problem[] = [];
  const commands: CommandEntry[] = [];
  const error = (message: string) => void problems.push({ severity: "error", message });
  const warning = (message: string) => void problems.push({ severity: "warning", message });

  if (!isRecord(manifest)) return { problems: [{ severity: "error", message: "package.json is not an object" }], commands };

  // Installing makes a directory of the name.
  if (!isText(manifest.name)) error(`"name" is missing`);
  else if (!/^[A-Za-z0-9][\w-]*$/.test(manifest.name)) error(`"name" must be letters, digits, "-" and "_" (it is "${manifest.name}")`);
  if (!isText(manifest.title)) error(`"title" is missing`);

  if (manifest.icon === undefined) warning(`"icon" is missing: the extension's commands will have no icon`);
  else if (!isText(manifest.icon)) error(`"icon" must be the name of a file in assets/`);
  else if (!iconExists(manifest.icon, exists)) error(`the icon assets/${manifest.icon} does not exist`);

  if (!Array.isArray(manifest.commands) || manifest.commands.length === 0) {
    error(`"commands" is missing or empty: there is nothing to run`);
    return { problems, commands };
  }

  const seen = new Set<string>();
  manifest.commands.forEach((command: unknown, index: number) => {
    if (!isRecord(command) || !isText(command.name)) return error(`commands[${index}] has no "name"`);
    const name = command.name;
    const at = `command "${name}"`;
    if (seen.has(name)) return error(`${at} is declared twice`);
    seen.add(name);
    if (!/^[\w-]+$/.test(name)) return error(`${at}: the name becomes a file name and may only have letters, digits, "-" and "_"`);
    if (!isText(command.title)) error(`${at} has no "title"`);

    if (!isText(command.mode)) error(`${at} has no "mode" (${MODES.join(", ")})`);
    else if (!MODES.includes(command.mode)) error(`${at} has the mode "${command.mode}", which Heyo does not know (${MODES.join(", ")})`);

    if (command.icon !== undefined) {
      if (!isText(command.icon)) error(`${at}: "icon" must be the name of a file in assets/`);
      else if (!iconExists(command.icon, exists)) error(`${at}: the icon assets/${command.icon} does not exist`);
    }
    if (command.interval !== undefined) {
      if (!isText(command.interval) || !/^\d+[smhd]$/.test(command.interval.trim())) warning(`${at}: "interval" should look like 30s, 10m, 1h or 1d`);
      else if (command.mode === "view") warning(`${at}: a command with a view does not run on an interval`);
    }
    for (const argument of Array.isArray(command.arguments) ? command.arguments : []) {
      if (!isRecord(argument) || !isText(argument.name)) error(`${at}: an argument has no "name"`);
      else if (argument.type !== undefined && !["text", "password", "dropdown"].includes(String(argument.type))) {
        warning(`${at}: argument "${argument.name}" has the type "${String(argument.type)}"; Heyo shows it as text`);
      }
    }

    const entry = ENTRY_EXTENSIONS.map((ending) => `src/${name}${ending}`).find(exists);
    if (!entry) return error(`${at} has no source: expected src/${name}.tsx (or .ts, .jsx, .js)`);
    commands.push({ name, entry });
  });

  if (manifest.heyo !== undefined) {
    if (!isRecord(manifest.heyo)) error(`"heyo" must be an object`);
    else {
      const { apiVersion, permissions } = manifest.heyo;
      if (typeof apiVersion !== "number" || !Number.isInteger(apiVersion) || apiVersion < 1) error(`"heyo.apiVersion" must be a whole number, 1 or more`);
      else if (apiVersion > API_VERSION) error(`"heyo.apiVersion" is ${apiVersion}, but this Heyo provides version ${API_VERSION}`);
      if (permissions !== undefined && !Array.isArray(permissions)) error(`"heyo.permissions" must be a list`);
      for (const permission of Array.isArray(permissions) ? permissions : []) {
        if (!PERMISSIONS.includes(String(permission))) warning(`"heyo.permissions" names "${String(permission)}", which is not a permission Heyo has`);
      }
    }
  }
  return { problems, commands };
}
