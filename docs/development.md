# Developing Heyo

Building Heyo, running it, testing it and finding out what went wrong. How
the code is laid out is in [architecture.md](architecture.md); how commits
are written is in [CONTRIBUTING.md](../CONTRIBUTING.md).

## What you need

- Rust (stable), Node.js 22 or newer, and [pnpm](https://pnpm.io) (CI uses
  pnpm 11).
- On Linux, the libraries Tauri and Heyo build against. On Ubuntu and
  Debian:

  ```sh
  sudo apt install build-essential curl wget file libssl-dev \
    libwebkit2gtk-4.1-dev libayatana-appindicator3-dev librsvg2-dev \
    libxdo-dev libxkbcommon-dev
  ```

- On Windows, what [Tauri asks for](https://tauri.app/start/prerequisites/)
  (the MSVC build tools and WebView2).

Extensions, dictation and text recognition run in Node.js: `node` on
`PATH`, or the path in Settings → Advanced, or `HEYO_NODE` in the
environment, which overrides both.

## Build and run

```sh
pnpm install
pnpm tauri dev
```

`pnpm tauri dev` bundles the runtime (`pnpm build:runtime`, into
`runtime/dist/`), starts Vite on port 1420 and a debug build of the host.
The web view reloads as files under `src/` change, and the host is built
again when `src-tauri/` changes. A debug build loads the runtime's scripts
from `runtime/dist/` in the repository: after changing anything under
`runtime/`, run `pnpm build:runtime` and start Heyo again.

Two things to know:

- Heyo is single-instance. If another Heyo is running in the same session
  (one you installed, say), a second one hands it its arguments and
  exits. Quit the other first, or use [the sandbox](#the-gui-sandbox),
  which has a session bus of its own.
- A development build uses the same settings and data as an installed one
  (`~/.config/heyo`, `~/.local/share/heyo`). The sandbox does not.

## Tests

| Command | What it runs |
|---|---|
| `cargo test` in `src-tauri/` | the host's tests; also writes `shared/bindings/` |
| `pnpm test` | Vitest: the web view (`src/`), the runtime (`runtime/src/`), `shared/` and the compatibility tool (`scripts/compat/`) |
| `pnpm exec tsc --noEmit` | type-checks the web view |
| `pnpm exec tsc --noEmit -p runtime` | type-checks the runtime |
| `pnpm build` | both type checks, the runtime's bundles and the web view's build |

Two files are type-checked and never run, and exist to fail the build:
`src/host.check.ts` (a call to the host that the host does not have must
not compile) and `runtime/src/heyo/types.check.ts` (the types extension
authors see must be what the runtime provides).

CI (`.github/workflows/ci.yml`) runs on every push to `master` and every
pull request:

- **Commit messages**: `scripts/check-commits.sh` over what the push or
  pull request adds.
- **Linux** (Ubuntu 24.04, Node.js 22): `pnpm build`, `pnpm test`,
  `cargo test --locked`, and then `git diff --exit-code -- shared/bindings`:
  the generated types must be committed as the host declares them.
- **Windows**: `pnpm build:runtime` and `cargo check --locked`. The
  Windows code is compiled, not tested, there.

To check that Windows still compiles without a Windows machine, from
Linux, with the Rust target installed and
[llvm-mingw](https://github.com/mstorsjo/llvm-mingw) on `PATH`:

```sh
cd src-tauri && cargo check --target aarch64-pc-windows-gnullvm   # or x86_64-…
```

## Generated files

These are committed, and made again by a command rather than edited:

| What | Made by | From |
|---|---|---|
| `shared/bindings/*.ts`: the TypeScript of the host's types, its commands and events, Heyo's own commands, the permission each request needs | `cargo test` in `src-tauri/` | `wire.rs`, `settings.rs`, `commands/bindings.rs`, `extensions.rs`, `permissions.rs` and the other types marked `#[ts(export)]` |
| `src-tauri/icons/*` and `src-tauri/icons/source/{icon,glyph}.png` | `python3 scripts/app-icon.py` (numpy, Pillow; runs `tauri icon`) | `src-tauri/icons/source/artwork.webp` |
| `src/assets/shadows/*.png`: the shadow round Heyo's windows | `python3 scripts/window-shadow.py` (Pillow) | the numbers at its top, which mirror `--shadow-window` in `src/styles/base/_tokens.scss` |
| `docs/images/*.png`: the README's pictures | `scripts/gui-test/readme-shots.sh` | the interface as it is ([below](#the-readmes-pictures)) |
| `docs/compatibility/`: which Raycast extensions run | `node scripts/compat/run.mjs` | the Raycast Store ([compatibility.md](compatibility.md)) |

## The GUI sandbox

What only shows in a window is tried in `scripts/gui-test/`: a build of
Heyo on an X server of its own (Xvfb), with a session bus and home
directories of its own, driven with `xdotool` and photographed. Nothing of
the desktop you work on is touched, and your own Heyo can stay running.

It needs `Xvfb`, `xdotool`, `xwininfo`, `dbus-daemon`, `xfwm4` (or
`gnome-shell` for a real compositor) and Python with Pillow. Build first:

```sh
pnpm install && pnpm build:runtime
(cd src-tauri && cargo build)
```

Then a script is run by its path. Each starts the sandbox, does its
thing, prints what it saw and stops the sandbox again:

```sh
scripts/gui-test/look.sh light before     # pictures of the root search, a command and the settings
scripts/gui-test/regress.sh               # the platform layer on X11, end to end
```

The parts they are made of:

- `env.sh` is the sandbox's environment, sourced by every script: the
  display, the bus, `XDG_DATA_HOME`, `XDG_CONFIG_HOME`, `XDG_CACHE_HOME`,
  and `heyo`, which talks to the Heyo inside.
- `up.sh [xfwm|gnome]` starts the X server, the bus, a window manager,
  the Vite dev server (a debug build loads the interface from it: nothing
  else may be on port 1420) and Heyo, hidden. `gnome` is GNOME Shell,
  which composites: the palette's transparent margin and shadow need it to
  look right in a picture. Settings are
  `$HEYO_TEST_DIR/home/config/heyo/settings.json`; write them before
  starting.
- `down.py` stops what `up.sh` started, and only that.
- `grab.py FILE [LEFT TOP RIGHT BOTTOM]` takes a picture of the sandbox's
  screen.

| Variable | Default | What it sets |
|---|---|---|
| `HEYO_TEST_DIR` | `/tmp/heyo-gui-test` | where the sandbox keeps its home directories, logs (`app.log`, `vite.log`, `shell.log`) and pictures (`shots/`) |
| `HEYO_TEST_DISPLAY` | `:99` | the X display; two sandboxes at once need one each, and a directory each |
| `HEYO_TEST_BUILD` | `debug` | `release` runs `src-tauri/target/release/heyo`, which carries its own interface (build it with `pnpm tauri build --no-bundle`), and starts no dev server |
| `HEYO_TEST_FRONTEND` | | `built` serves a production build of the interface to the debug binary, for measuring; `none` starts no server |
| `SCREEN` | `1920x1080` | the size of the screen |

The scripts, by what they try:

| Script | What it tries |
|---|---|
| `look.sh`, `compact.sh`, `settings.sh`, `select.sh` | pictures of the interface: the themes, compact mode, every settings page, the selection moving |
| `open.sh`, `arguments.sh`, `views.sh` | opening commands, arguments typed in the search bar, what an extension's views do (forms, the file picker, typing into a slow extension) |
| `regress.sh`, `clipboard.sh`, `snap.sh`, `expand.sh`, `dictate.sh` | the platform layer on X11: pasting, layouts, the clipboard history, the screenshot overlay, snippets expanding, dictation |
| `heyo-api.sh`, `windows-api.sh`, `activities.sh` | `@heyo/api` as an extension sees it, the permission question, live activities |
| `ai-chat.sh` | Ask AI and AI Chat against `llm-stub.py`, a server that answers with canned text |
| `dev.sh` | `heyo dev` against a running Heyo |
| `scroll.sh` | how smoothly lists scroll, measured in the web view |

`extensions/` holds the small extensions some of them install.

### The README's pictures

`scripts/gui-test/readme-shots.sh` takes the pictures in `docs/images/`
again, in both themes, so that they can follow the interface:

```sh
scripts/gui-test/readme-shots.sh           # or: readme-shots.sh dark
```

It starts a sandbox from nothing for each theme: an empty home directory,
a short list of applications, a backdrop drawn by `readme-backdrop.py`,
and a little sample data (favourites, a few things copied). The session
runs at twice the size, so the pictures are 2× and the README shows them
at half their width. It needs GNOME Shell, the network (it installs one
extension from the Raycast Store and converts a currency) and a few
minutes.

## GNOME on Wayland, in a container

`scripts/wayland-test/` builds a container with GNOME 50 running headless
on Wayland, for trying the Shell extension and the `Gnome` adapter without
a Wayland session of your own:

```sh
pnpm tauri build --no-bundle
scripts/wayland-test/run.sh build       # once; about 1.5 GB
scripts/wayland-test/test.sh            # all the checks, in every monitor layout
```

[scripts/wayland-test/README.md](../scripts/wayland-test/README.md) says
how to drive a session by hand, and what a test there does and does not
show: there is no GPU, no XWayland and no login manager, and only GNOME 50.

## Windows, in a virtual machine

`scripts/windows-test/` runs Windows 11 in Docker (QEMU with KVM) and
cross-compiles Heyo for it from Linux:

```sh
scripts/windows-test/run.sh start       # the first start installs Windows
scripts/windows-test/build.sh           # ARCH=x86_64 for Intel/AMD; needs llvm-mingw
```

then, inside the machine (in a browser at `http://127.0.0.1:8006`),
`\\host.lan\Data\scripts\windows-test\in\update.bat` copies the build in
and starts it, and `log.bat` shows the end of the log.
[CHECKLIST.md](../scripts/windows-test/CHECKLIST.md) is what remains to be
tried by hand there.

## When something goes wrong

- **The log** is `~/.cache/heyo/heyo.log` (`%LOCALAPPDATA%\heyo\heyo.log`
  on Windows): what Heyo and its Node.js helpers write to standard error,
  and what a panic leaves. Started from a terminal, Heyo writes it there
  as well. Past a size it is set aside as one older copy and begun again.
- **Copy Diagnostics**, in Settings → About, copies a report for a bug:
  the version, the system and session, what the session can do
  (`platform::capabilities()`), how the interface is drawn, the Node.js
  and the web view found, and the end of the log, with what looks like a
  secret taken out and the home directory written as `~`.
- **The web view** of a debug build has its inspector, as Tauri's debug
  builds do: right-click, Inspect Element.
- **A script of your own in place of a helper**: `HEYO_RUNTIME`,
  `HEYO_DICTATION`, `HEYO_OCR`, `HEYO_EMBED` and `HEYO_DEV` each name a
  file to run instead of the bundled `heyo-*.cjs`.

## Installing your build

```sh
scripts/install.sh --build       # scripts/install.sh --uninstall removes it
```

builds with `pnpm tauri build --no-bundle` and installs the program, its
helper scripts, a desktop entry and the icons for the current user, under
`~/.local` (or `$PREFIX`). It does not restart a running Heyo: quit it and
start it again.

Install through the script rather than by copying a binary, because of two
traps it guards against:

- A binary made by `cargo build --release` alone has no interface of its
  own. Only `pnpm tauri build` embeds the built web view; cargo by itself
  produces a program that loads whatever is on the dev server's port, or a
  blank window. Such a binary says so when it starts, and `install.sh`
  refuses it.
- The program and its helper scripts must come from the same build. Each
  `runtime/dist/heyo-*.cjs` begins with a line naming the version it was
  built for; Heyo refuses a script of another version, and `install.sh`
  installs neither unless they agree.

The packages (deb, rpm, AppImage, the Windows installer) are built in two
steps, `scripts/package/build.sh` and `bundle.sh`, by hand or by
`.github/workflows/package.yml` for all four targets at once:
[packaging/README.md](../packaging/README.md) has how, and what was
verified.

## Other tools

- `node scripts/compat/run.mjs` launches the most-downloaded extensions of
  the Raycast Store in the runtime and writes down what happened
  ([compatibility.md](compatibility.md)).
- `scripts/local-model.sh start|stop|status|logs` runs a small model with
  vLLM in Docker, as an OpenAI-compatible server for Heyo's AI features
  (Settings → Advanced → AI → Local Model).
