# Developing extensions on Heyo

Heyo runs extensions written for Raycast. The `heyo` program has four
commands for writing one, or for bringing an existing one over, without
Raycast's own `ray` tool:

| Command | What it does |
|---|---|
| `heyo dev [dir]` | Builds the extension on every change and loads it into the running Heyo; prints what its commands log |
| `heyo build [dir]` | Builds the extension into the form Heyo (and Raycast) loads |
| `heyo ext check [dir]` | Reports what in the extension will not work on Heyo |
| `heyo migrate [dir]` | Rewrites `@raycast/api` imports to `@heyo/api` |

`[dir]` is the extension's source directory, the one with its
`package.json`; the current directory if left out. All four need Node.js 22
or newer, the same one Heyo runs extensions with (Settings → Advanced).

An extension is laid out as Raycast lays them out:

```
my-extension/
  package.json        the manifest: name, title, icon, commands, preferences
  src/<command>.tsx   one file per command, named after the command (.tsx, .ts, .jsx or .js)
  assets/             icons and other files
```

## `heyo dev`

```
cd my-extension
npm install
heyo dev
```

The extension is built and its commands appear in the root search, marked
**Development**. Every time a source file, the manifest or an asset changes
it is built again and Heyo reloads it; if one of its commands is open in the
palette, that command starts again. Leave it running in a terminal:

```
12:01:07 Developing my-extension from /home/me/my-extension
12:01:07 Built 2 commands in 41 ms; Heyo reloaded it
12:01:15 [search] fetched 30 items
12:01:15 [search] error: TypeError: Cannot read properties of undefined (reading 'title')
    at Row (/home/me/my-extension/src/search.tsx:42:27)
```

What the extension writes with `console.log`, `console.warn` and
`console.error`, and the errors it does not catch, are printed there, each
with the command it came from. Stack traces name the lines of your sources,
not of the built file. A build that fails prints esbuild's errors and leaves
the last build that worked in Heyo.

`Ctrl+C` stops watching. The extension **stays in Heyo as last built**,
also after Heyo restarts, until you take it out:

```
heyo dev --list              the extensions in development
heyo dev --remove [dir]      take one out (by directory or by name)
```

or with **Manage Extensions → Stop Developing**, or in Settings →
Extensions. Removing it deletes what was built, never your sources.
A development extension is never looked up in or updated from the Raycast
Store, and it keeps its preferences and stored data under the name in its
manifest, shared with an installed extension of the same name.

How it is put together, for when something does not work:

- The extension is built into `~/.local/share/heyo/development/<name>/`
  and listed in `~/.local/share/heyo/development.json`. That is all Heyo
  reads to list its commands.
- After each build `heyo dev` runs `heyo --hidden --dev-reload <built dir>`.
  Heyo is single-instance, so the running one receives that and reloads. If
  Heyo was not running, this starts it, hidden.
- The runtime appends the extension's console to
  `~/.local/share/heyo/development/<name>.log`, which `heyo dev` follows.

Not reloaded by `heyo dev`: a menu bar or background command that is
already running keeps the code it started with until Heyo restarts.

## `heyo build`

```
heyo build [dir] [-o <out>] [--sourcemap]
```

Builds into `<dir>/dist` (or `<out>`): the manifest, the assets, and one
CommonJS file per command, `<command>.js`, with everything the command
imports bundled in, except what Heyo provides when it loads the command:
`@raycast/api`, `@heyo/api`, `react` and `react/jsx-runtime`. This is what
`ray build -e dist` makes and what the Raycast Store hands out. To install
the result by hand, copy it to `~/.local/share/heyo/extensions/<name>/`.

The manifest is checked first, and the build refused if a command has no
source file (`src/<name>.tsx`, `.ts`, `.jsx` or `.js`), has a mode other
than `view`, `no-view` or `menu-bar`, or the manifest names an icon that is
not in `assets/`.

The building is done by [esbuild](https://esbuild.github.io), which is a
program compiled per system and so cannot be part of `heyo` itself the way
the rest of the tools are. It is taken from the extension's own
`node_modules`: `@raycast/api` depends on esbuild, so an extension whose
dependencies are installed (`npm install`) has it, and building needs no
network. Only if there is none, esbuild is installed once into
`~/.local/share/heyo/tools/esbuild/`, one exact version from a lockfile that
ships with Heyo, checked against its hashes, without running install scripts
(the same way Heyo installs its speech and text-recognition engines).

## `heyo ext check`

```
heyo ext check [dir] [--json]
```

Reads the sources (it runs nothing) and reports what will not work on Heyo:

| Rule | What it finds |
|---|---|
| `applescript` | `runAppleScript`, `osascript`, `tell application …` |
| `open-flags` | `open -a` and `open` with other macOS flags |
| `mac-program` | Programs only macOS has being started: `pbcopy`, `defaults`, `mdfind`, `say`, … |
| `mac-path` | `/Applications`, `~/Library`, `/System/Library`, `/opt/homebrew`, … |
| `darwin-only` | `process.platform === "darwin"` in a file with nothing for `linux` or `win32` |
| `swift` | Swift helper code |
| `mac-binary` | A macOS executable shipped in `assets/` |
| `native-addon` | A dependency (or one of its dependencies) with compiled code |
| `api-missing` | Parts of the Raycast API Heyo does not have |
| `api-unsupported` | Parts Heyo has but cannot do (`BrowserExtension`) or that do nothing (`updateCommandMetadata`) |
| `prop-ignored` | Component properties Heyo accepts and ignores |
| `manifest` | What `heyo build` would refuse or warn about |
| `permission-missing` | A Heyo module used without its permission in `heyo.permissions` |
| `platforms`, `ai-tools` | The manifest names only macOS; it declares AI tools, which Heyo does not run |

Each finding is an error, a warning or a note, with the file, line and
column. The command exits with 1 if there are errors. `--json` prints the
same as one object: `{ extension, files, findings: [{ rule, severity,
message, file, line, column, excerpt }], summary }`.

The API rules read the same lists Heyo's conformance tests keep
(`runtime/src/api/conformance.gaps.json`, `props.gaps.json`), so they are as
current as the Heyo the command came with. The checks are textual: a finding
is a place to look at, and code that builds a command line piece by piece
can get past them.

It also works on a built extension, such as one installed from the store
(`heyo ext check ~/.local/share/heyo/extensions/<name>`), though built code
is harder to read: the property checks need the JSX of the sources.

## `heyo migrate`

```
heyo migrate [dir] [--dry-run]
```

`@heyo/api` is everything `@raycast/api` exports plus Heyo's own modules;
inside Heyo both names are the same object. Migrating:

- rewrites every import of `@raycast/api` in the sources to `@heyo/api`:
  `import … from`, `import type`, `export … from`, `require()`, `import()`,
  and `vi.mock`/`jest.mock`. Comments, other strings and other packages are
  left alone, `@raycast/utils` in particular: it goes on importing
  `@raycast/api` and gets the same object;
- adds `"heyo": { "apiVersion": 1, "permissions": [] }` to `package.json`;
- adds `@heyo/api` to `dependencies`. `@raycast/api` stays: the types come
  from it.

`--dry-run` prints the changes as a diff and writes nothing. Run it on a
clean checkout so the result is easy to review, then `npm install` and
`heyo ext check`.

An extension does not have to be migrated to run on Heyo. Migrating is for
using Heyo's own modules, which need permissions declared in the manifest.

## Using the tools without the `heyo` program

The four commands are one Node.js program, `heyo-dev.cjs`, which `heyo`
starts (installed beside Heyo's other helper scripts; `runtime/dist/` in a
checkout). It runs on its own too:

```
node heyo-dev.cjs build .
```

Run that way, `dev` cannot tell the running Heyo about a build; the
extension appears when Heyo next reloads its commands.

Required as a module it runs nothing and exports the checker:

```js
const { checkExtension, checkSource } = require("./heyo-dev.cjs");
const report = checkExtension("/path/to/extension");
```
