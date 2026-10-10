# Heyo

[![CI](https://github.com/hdonix/heyo/actions/workflows/ci.yml/badge.svg)](https://github.com/hdonix/heyo/actions/workflows/ci.yml)

A cross-platform command palette launcher. It launches desktop applications
and runs [Raycast](https://www.raycast.com) extensions on Linux.

Heyo is early. It is developed and used daily on Ubuntu 24.04 (GNOME on
X11, arm64); GNOME on Wayland is tested in a container, and the Windows
build starts and launches applications in a virtual machine. How the 100
most-downloaded extensions of the Raycast Store fare is measured in
[docs/compatibility.md](docs/compatibility.md). Heyo is not affiliated with
Raycast.

## Install

There are no released packages yet: build from source, or build the
packages as [packaging/README.md](packaging/README.md) describes.

Heyo needs WebKitGTK 4.1 and, for extensions, dictation and text
recognition, Node.js 22 or newer: `node` on `PATH`, or its path in
Settings → Advanced. The launcher itself works without Node.js.

- **Debian, Ubuntu**: `sudo apt install ./Heyo_<version>_<arch>.deb`. The
  package only recommends Node.js, because most releases ship an older one
  than Heyo needs; install a current one from nodejs.org or with a version
  manager.
- **Fedora, openSUSE**: `sudo dnf install ./Heyo-<version>-1.<arch>.rpm`.
- **From source**, for the current user, under `~/.local`:

  ```sh
  pnpm install
  scripts/install.sh --build       # scripts/install.sh --uninstall removes it
  ```

How the packages are built, and the state of the AppImage, the Windows
installer, the AUR package and Flatpak, is in
[packaging/README.md](packaging/README.md).

## Development

System packages (Ubuntu/Debian):

```sh
sudo apt install libwebkit2gtk-4.1-dev libayatana-appindicator3-dev librsvg2-dev \
  build-essential curl wget file libxdo-dev libssl-dev
```

Extensions run in Node.js, so `node` (22 or newer) must be on `PATH`
(or set `HEYO_NODE`). Then:

```sh
pnpm install
pnpm tauri dev
```

The tests: `cargo test` in `src-tauri/`, `pnpm test` for the web view and
the runtime, and `scripts/gui-test/` for what only shows in a window (an X
server, a session bus and a home directory of its own, so that the desktop
it runs on is left alone).

## Usage

- Type to fuzzy-search applications; `↑`/`↓` (or `Ctrl+P`/`Ctrl+N`) to move, `Enter` to launch.
- `Esc` clears the query, or hides the window when the query is empty.
- The window hides when it loses focus.
- `Ctrl+,` (or **Heyo Settings** in the search) opens the settings window,
  laid out like Raycast's: a sidebar with **General** (open at login, hotkey,
  window behavior), **Shortcuts** (aliases and hotkeys for every command and
  application), **Advanced** (Node.js) and **About**, followed by one page per
  extension (enable or disable commands, preferences, uninstall), plus
  **Applications** and **Heyo Commands**. Settings are saved to
  `~/.config/heyo/settings.json`.

### Root search

- Results learn from use: what you open often ranks higher, and the empty
  search shows **Favorites** and **Suggestions**.
- `Ctrl+K` on a result: add to favorites (`Ctrl+Shift+F`), set an alias
  (`Ctrl+Shift+A`), reset its ranking.
- Calculator: `2^10`, `15% of 80`, `5 km in mi`, `100 f to c`,
  `100 usd in cny` (rates from open.er-api.com, refreshed every 6 hours),
  `time in tokyo`, `3pm pst in beijing`. `Enter` copies the answer.

Below the results, **Use "…" with…** offers what you typed to Google, to
Search Files and to your quicklinks that take a query — so `Enter` on a
search with no results searches the web.

Opening an application that is already running goes to its window instead
of starting it again. Asked once more while that window is in front, it
moves on to the application's next window, or, if it has only one, back to
the window you were in before — so an application's hotkey flips between it
and your other work. (X11 only.)

### Quicklinks and snippets

**Create Quicklink** saves a URL (or path) template such as
`https://github.com/search?q={query}`; give it an alias like `gh` and type
`gh heyo` to open it directly. **Create Snippet** saves text to paste, with
an optional keyword that selects it in the root search. Both support
placeholders: `{query}`/`{argument}`, `{clipboard}`, `{date}`, `{time}`,
`{datetime}`, `{day}`, `{uuid}` and `{date format="yyyy-MM-dd"}`. They are
stored in `~/.local/share/heyo/quicklinks.json` and `snippets.json`.

### Selected Text Actions

Select text in any application and run **Selected Text Actions** (give it a
hotkey): Heyo reads the selection without you copying it and offers to
translate it, improve the writing, fix spelling and grammar, shorten, explain
or summarize it with Claude, or to search the web for it. Claude's answer
streams in; `Enter` replaces the selection with it (or copies it, for
explanations). These actions use Claude (an API key in Settings → Advanced,
or `ANTHROPIC_API_KEY`) or a model on your own computer — see
[Local model](#local-model).

`Ctrl+N` adds an action of your own: a name and the instruction Claude gets,
for example "Translate the text into natural Japanese".

### Ask AI

Type a question in the root search and press `Tab`: the answer streams in.
`Enter` asks a follow-up that keeps the conversation so far; the answer can
be copied or pasted. **Ask AI** is also a command of its own. It uses the
same model as Selected Text Actions.

### Local model

Selected Text Actions and dictation polishing can use a model running on
your computer instead of Claude, so the text never leaves it. In Settings →
Advanced → AI choose **Local Model** and point it at any OpenAI-compatible
server (vLLM, LM Studio, Ollama, llama.cpp); the default address is
`http://127.0.0.1:8000/v1`.

`scripts/local-model.sh start` runs one with vLLM in Docker: Qwen3-4B
Instruct by default, about 8 GB to download, listening only on this
computer. `stop`, `status` and `logs` do what they say. vLLM reserves GPU
memory up front — 12% by default (`HEYO_LLM_MEMORY`), which matters on
machines where the GPU shares memory with the system. The container does
not restart by itself after a reboot.

A small model is quick and fine for translating and tidying text, but it
follows instructions less reliably than Claude on long or tricky input.

### Screenshots

**Capture Screenshot** freezes the screen: drag to select an area (drag it or
its handles to adjust), mark it up with rectangles, arrows, the pen or text,
blur what should stay private (anything you added can be dragged to another
place), then copy it (`Enter`), save it to
`Pictures/Screenshots` (`Ctrl+S`), pin it on top of other windows, or copy the
text in it. A pinned capture can be dragged around; double-click or
right-click closes it.

**Capture Text from Screen** copies the text in the area you drag over, using
PaddleOCR on your computer (Chinese and English; a one-time download of about
340 MB in Settings → Screenshots).

Both capture straight from the X server; elsewhere `gnome-screenshot` is used.

### Dictation

Hold the dictation hotkey while you speak and let go — or tap it, speak, and
tap it again: the text is pasted into the window you are working in.

A single key can do the same: choose a **Hold-to-Talk Key** (Right Alt, Left
Alt, Right Ctrl, …) in Settings → Dictation, hold it and speak, let go to
finish. The key keeps its normal meaning: a quick tap does nothing, and so
does using it in a shortcut such as Alt+Tab. (X11 only.) A small window at the bottom of the screen
shows the microphone level and the words as they are recognized.

- Speech is recognized on your computer with
  [SenseVoice](https://github.com/FunAudioLLM/SenseVoice) (Mandarin,
  Cantonese, English, Japanese, Korean; mixed Chinese and English works). Set
  it up once in Settings → Dictation; the download is about 270 MB.
- By default each sentence is pasted when you pause. With **Polish with
  Claude** set to *Clean Up* or *Translate to English*, the transcript (not
  the audio) is sent to Claude when you finish and the result is pasted once.
  This needs a Claude API key (in the settings, or `ANTHROPIC_API_KEY`).
- Text is pasted through the clipboard, which is restored afterwards;
  terminals get `Ctrl+Shift+V`.
- `heyo --dictate` starts or finishes dictation, for binding your own
  shortcut. Needs `parec` (PulseAudio/PipeWire) or `arecord`, and `xdotool`
  (X11) or `wtype` (Wayland).

### Windows

**Switch Windows** lists the open windows, the one you were last in first, so
running it and pressing `Enter` goes back and forth between two windows.
`Ctrl+X` closes a window.

The window layout commands move the focused window: **Left Half**, **Right
Half**, top and bottom halves, the four quarters, thirds and two thirds,
**Maximize**, **Almost Maximize**, **Center** and **Move to Next Display**.
Give the ones you use a hotkey in Settings. These need X11.

### System and utilities

- **Lock Screen**, **Sleep**, **Restart**, **Shut Down**, **Log Out** and
  **Empty Trash** (the last four ask first), **Toggle Dark Mode**, **Volume
  Up**/**Down**, **Toggle Mute**, and **Toggle Keep Awake**, which stops the
  computer from sleeping until you turn it off or quit Heyo.
- **Search Emoji & Symbols**: search in English or Chinese; `Enter` pastes.
- **Search Files**: fuzzy search over the names of the files and folders
  under your home folder (hidden folders, `node_modules` and the like are
  skipped). The names are read into memory when the command opens — a
  fraction of a second, about 80 MB per million files — and released a few
  minutes after you last used it; there is no index on disk.
- **Kill Process**: search by name, command line or listening port (`3000`).
- **Switch Audio Device**: pick the default speaker and microphone.
- **Search Browser Bookmarks** and **Search Browser History**: Firefox,
  Chrome, Chromium, Brave, Edge and Vivaldi. Read from a copy of the
  browser's own files; needs Node.js 22.5 or newer.

**Paste as Plain Text** pastes what you copied without its formatting; give
it a hotkey.

### Clipboard History

Heyo records what you copy (text, links, colors, file paths and images)
while it runs; open **Clipboard History** to search it, preview entries, and
paste (`Enter`), copy (`Ctrl+Enter`), pin or delete them. Copies that password
managers mark as secret are skipped. History is kept for 3 months by default
(pinned entries forever) in `~/.local/share/heyo/clipboard/`. Pasting into
the previous window uses `xdotool` (X11) or `wtype` (Wayland).

The search understands more than words:

- Chinese of any length (`发票`, `票`), and pieces of URLs, paths and
  identifiers (`issues/42`, `0.3.17`). Every word typed must occur.
- When: `昨天`, `昨天下午`, `上周三`, `最近三天`, `10月1号`, `yesterday`,
  `last week`, `2 days ago`, `2026-10-03`…
- Where from: `from firefox`, `来自firefox`, `code里的`, `app:code` (the
  application copied from).
- What: a query that is only a kind (`链接`, `图片`, `images`, `昨天的链接`).

How the query was read is shown above the results. A reading that finds
nothing is dropped, and the query searched for as plain words. Results are
newest first.

`heyo --clipboard` opens Clipboard History directly.

`heyo --clipboard-eval [FILE]` measures the search: how often the right
entry is in the first 1, 5 and 10 results, by kind of query, and how long a
search takes. Without a file it uses the searches you picked a result from,
which the history records (and forgets with the entry); the file format is
described in `src-tauri/src/clipboard/eval.rs`.

Not recorded: what a password manager marks as secret, anything copied
while a password manager's window is focused (KeePassXC, 1Password,
Bitwarden and the like), what an extension copies as concealed, and Heyo's
own pasting during dictation. The history is readable only by you.

### Hotkey

Wayland has no global hotkeys, so Heyo is single-instance: running `heyo`
again toggles the existing window. Bind a desktop shortcut to it, e.g. on GNOME:

Settings → Keyboard → Custom Shortcuts → command `/path/to/heyo`, shortcut `Alt+Space`.

On GNOME, Heyo's settings window records these shortcuts for you (General →
Heyo Hotkey, and the Clipboard History hotkey under Extensions).

Any application, command (built-in or from an extension), quicklink or
snippet can have its own hotkey: `Ctrl+K → Set Hotkey` (`Ctrl+Shift+H`) in
the root search, or the Hotkey column in Settings → Extensions, which also
lists applications. A hotkey runs `heyo --open <item id>`: applications
launch without showing Heyo, commands open in the palette, and snippets
paste into the focused window.

Start it in the background with `heyo --hidden`.

## Raycast extensions

Run **Search Raycast Store** to browse the store and press `Enter` to install
an extension; its commands then appear in the root search. **Manage
Extensions** checks the store for updates (update one or all), configures
preferences and uninstalls.

Extensions are the same builds Raycast downloads, unpacked into
`~/.local/share/heyo/extensions/<name>/`. An extension built locally with
`ray build -e dist` can be copied there too.

Inside a command: `Enter` runs the primary action, `Ctrl+Enter` the second
one (or submits a form), `Ctrl+K` opens all actions, and `Esc` goes back.
Shortcuts written for macOS map `⌘` to `Ctrl`.

What works: `List`, `Grid`, `Detail`, `Form`, action panels, navigation,
toasts, alerts, clipboard, `open`, `LocalStorage`, `Cache`, preferences,
arguments, OAuth, menu bar commands, background refresh, `AI.ask` and
`@raycast/utils` hooks. Not supported: the browser extension API, window
management, and extensions that only work on macOS (AppleScript, Swift
binaries).

Menu bar commands appear as icons in the top bar (GNOME needs the
AppIndicator extension, which Ubuntu ships enabled). Run one from the root
search once to turn it on; from then on it comes back when Heyo starts and
refreshes at the interval its extension asks for. Commands that refresh in
the background work the same way: they start doing so after you have run
them once. Disable a command in Settings to stop it. `AI.ask` answers with
the model chosen in Settings → Advanced → AI.

Signing in (OAuth): an extension that connects to a service opens its
sign-in page in your browser. Those services send you back through a
`raycast://` link, so Heyo registers itself as the handler for `raycast://`
and `com.raycast:` links (`~/.local/share/applications/heyo-links.desktop`);
the browser may ask once whether to open it. Tokens are stored per extension
under `~/.local/share/heyo/support/`, readable only by you.

Data lives in `~/.config/heyo/preferences/` (preferences),
`~/.local/share/heyo/support/` (`environment.supportPath`, `LocalStorage`)
and `~/.cache/heyo/` (`Cache`).

## Developing extensions

The `heyo` program builds and runs an extension from its sources, without
Raycast's `ray` tool. In the extension's directory (after `npm install`):

| Command | What it does |
|---|---|
| `heyo dev` | Builds on every change and loads the result into the running Heyo: its commands are in the root search marked **Development**, the one open in the palette restarts, and what it logs (with errors pointing at your source lines) is printed in the terminal. It stays in Heyo until `heyo dev --remove` |
| `heyo build` | Builds into `dist/` what Heyo and Raycast load: one `<command>.js` per command, with `@raycast/api`, `@heyo/api` and React left for Heyo to provide |
| `heyo ext check` | Reports what will not work on Heyo: AppleScript, `open -a`, macOS programs and paths, native addons, API members Heyo lacks. `--json` for programs |
| `heyo migrate` | Rewrites `@raycast/api` imports to `@heyo/api` and adds the `heyo` manifest field. `--dry-run` shows the diff |

[docs/developing-extensions.md](docs/developing-extensions.md) has the details.

## Layout

```
src/              UI (React)
  App.tsx         hosts the root search or a running command
  RootSearch.tsx  root search: results, calculator, favorites, aliases
  calculator.ts   arithmetic, units, currency and time zones
  hotkey.tsx      hotkey recorder
  Dictation.tsx   dictation progress window
  Screenshot.tsx  screenshot overlay and pinned captures
  Settings.tsx    settings window
  raycast/        renders extension views (List, Grid, Detail, Form, actions)
shared/           placeholder expansion used by the UI and the runtime
runtime/src/      extension runtime (Node), bundled to runtime/dist/
  dictation.ts    dictation helper: recording, speech recognition, polishing
  ocr.ts          text-recognition helper (PaddleOCR)
  ai.ts           Claude or a local model, for Heyo's own commands
  background.ts   menu bar and background commands (a process of their own)
  main.ts         entry: provides @raycast/api and React to extensions
  renderer.ts     React reconciler whose tree is streamed to the UI
  session.ts      runs one command at a time, sends incremental renders
  api/            the @raycast/api implementation
  builtins/       Heyo's own commands (store, clipboard, emoji, system, …)
src-tauri/src/
  lib.rs          startup, shared state, the application index
  commands/       what the webview can invoke: palette, settings, media
  host.rs         what the runtime asks of the host (`host.*`)
  palette.rs      the palette window: show, hide, toggle
  cli.rs          command-line flags and opening an item by id
  platform/       everything that touches the desktop (see below)
  apps.rs         .desktop discovery and launching
  extensions.rs   installed extension discovery
  runtime.rs      the Node processes and their message relay
  settings.rs     settings file and the autostart entry
  clipboard.rs    clipboard history: what is recorded
  clipboard/      its storage and search (SQLite), reading queries, measuring
  dictation.rs    dictation: helper process, progress window, pasting
  screenshot.rs   capturing, the selection overlay's outcome, pinning
  ocr.rs          text recognition for screenshots
  files.rs        file search: in-memory scan of the home directory
  links.rs        raycast:// link handler registration (OAuth redirects)
  menubar.rs      menu bar commands as tray icons, background schedule
  root.rs         root search index and ranking (fuzzy, frecency, aliases)
  usage.rs        usage counts for ranking
  library.rs      quicklinks and snippets
  search.rs       fuzzy matching (nucleo)
```

### Settings

A setting is declared once, as a field of `Settings` in
`src-tauri/src/settings.rs`. The windows' TypeScript types are generated
from the Rust ones into `shared/bindings/` (by `cargo test`; commit what
it writes). A window changes settings by sending only the fields that
change (`change_settings`); the host merges them into the rest, saves, does
what the change calls for and tells every window. So adding a setting is
adding the field, its default and its control.

### Styles and themes

The interface has a light and a dark scheme, following the system unless
Settings → General → Theme says otherwise. Both are one two-tone design:
neutrals for everything, one accent for what is active.

The stylesheet is SCSS, in `src/styles/`:

- `abstracts/_scales.scss` holds the scales as Sass maps, the single source
  for every colour (as a light and dark pair), the 4 px spacing grid, the
  type scale (each size with its line height and letter spacing), radii,
  control heights, shadows, durations and easings.
- `abstracts/_mixins.scss` is what components are assembled from: `text`,
  `truncate`, `fade-end`, `scroll-fade`, `interactive` (resting, hovered,
  pressed, disabled), `field`, `select`, `card`, `floating`, `focus-ring`,
  `enter`.
- `base/_tokens.scss` turns the scales into custom properties. Colours are
  emitted as CSS `light-dark()` and follow `color-scheme`, so there is no
  second stylesheet and no script deciding colours; `src/theme.ts` only sets
  `data-theme` when the setting overrides the system. `base/_motion.scss`
  has the keyframes, `base/_reset.scss` the page.
- `components/` has a partial per part of the interface. They use the
  tokens and mixins and no colours or sizes of their own.

Details that go with it: the palette's window is larger than its panel by a
transparent margin for the panel's shadow (a click there hides it); the
selected row is marked by one element that slides between rows
(`SelectionIndicator.tsx`); the icon at the start of a row is an
application's own, a symbol on a tile, or an initial (`ItemIcon.tsx`);
lists that are loading show skeleton rows; views, the action panel and
dialogs arrive with a short animation. What floats over the screen (the
screenshot tools, the dictation pill) stays dark. Colours that extensions
name (`Color.Blue`, `{ light, dark }`) map to tokens too. More contrast and
less motion are given where the system asks for them.

### The platform layer

`platform/` is where Heyo meets the desktop it runs on. Its modules of
plain functions are what the rest of Heyo uses: `windows`, `input` (paste,
selected text), `clipboard_watch`, `capture`, `overlay`, `keys`
(hold-to-talk), `hotkeys`, `focus`, `common` (notifications). They hold
what is the same everywhere and ask the session's adapter for the rest.

The seam is one trait, `desktop::Desktop`: everything Heyo asks of a
desktop. Each session has one adapter (`x11::X11`, `gnome::Gnome` for GNOME
on Wayland, `win::Windows`, and `desktop::Generic` where there is none),
chosen once. What an adapter does not implement stays at the trait's
default, which says "not supported"; `platform::capabilities()` reports
it, so commands that cannot work are left out of the search and the
settings say why. Supporting another desktop is one more adapter; a
capability new to all of them is one more method, and the compiler finds
nothing else to change. Tests put `desktop::fake::Fake` in its place.

`scripts/gui-test/regress.sh` exercises the X11 adapter end to end in a
sandbox (paste, a window layout, the clipboard, the screenshot overlay).

#### GNOME on Wayland

On Wayland an application cannot see other windows, type into them, follow
the clipboard while unfocused, or read the screen; the Shell can. Heyo ships
a small Shell extension, `gnome-extension/heyo@heyo.app` (GNOME 45–50),
that does those things for it over D-Bus, and answers only the process that
owns Heyo's bus name. Heyo carries the extension: Settings → General
offers to install it, and to update it when a newer Heyo carries a newer
one, after which you log out and in again (GNOME only loads a new or
updated extension when a session starts; an older one keeps working until
then). `docs/wayland.md` has the details, and what other Wayland desktops
could offer.

With it, window switching and layouts, pasting, selected text, clipboard
history and screenshots work as on X11. The hold-to-talk key works with the
modifier keys (Alt, Ctrl, Right Shift): the Shell reports the press, and
tells from the modifier state when it is let go. Without the extension (or on another Wayland desktop) the
window commands are unavailable, the clipboard is polled, pasting needs
`wtype` or `ydotool`, and capturing goes through `gnome-screenshot`.

`scripts/wayland-test/` runs a GNOME Wayland session in Docker to test this.

`scripts/gui-test/` drives a debug build in a sandbox of its own (an X
server, a private bus, its own home directories), under a plain window
manager or a real GNOME Shell, and takes pictures of it: `look.sh light L`.

#### Windows

Heyo runs on Windows, with part of what it does on Linux (`platform/win/`,
through the Win32 API):

- the palette, shown and hidden by a global hotkey (Alt+Space until another
  is chosen; hotkeys are kept in `hotkeys.json` beside the settings);
- the Start menu's applications, and launching them;
- Heyo's own commands and extensions, which run in Node.js: a `node.exe`
  beside `heyo.exe` is used, so nothing needs installing;
- switching between windows and arranging them (halves, quarters, …);
- pasting into the application in front; clipboard history.

Not there yet: screenshots and text recognition, the selected text, the
hold-to-talk key and dictation, application icons, an installer. Settings
and data are in `%APPDATA%\heyo`, caches and the log (`heyo.log`) in
`%LOCALAPPDATA%\heyo`.

`scripts/windows-test/` runs a Windows 11 machine in Docker and
cross-compiles Heyo for it from Linux: `run.sh start`, `build.sh`, then
inside the machine `\\host.lan\Data\scripts\windows-test\in\update.bat`.

The runtime talks JSON lines over stdin/stdout. Rust answers `host.*`
requests (clipboard, window, reloading the index) and relays everything else
between the runtime and the webview.

## Licence

[GPL-3.0](LICENSE).
