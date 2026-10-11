# Using Heyo

Every feature, with its keys and where it keeps things. The
[README](../README.md) is the short version. What a feature needs of the
desktop is said where it matters; [which desktops give
what](../README.md#where-it-runs) is in the README.

## The palette

- Type to search applications, commands, quicklinks and snippets; `↑`/`↓`
  (or `Ctrl+P`/`Ctrl+N`) to move, `Enter` to open.
- `Esc` clears the query, or hides the window when the query is empty.
  The window also hides when it loses focus (Settings → General).
- `Ctrl+K` opens the actions of the selected row.
- `Ctrl+,` (or **Heyo Settings** in the search) opens the settings window.
- `Tab` with something typed asks the AI model about it (see
  [Ask AI](#ask-ai-and-ai-chat)); on a command that takes arguments, `Tab`
  moves into its fields.

### Root search

- Results learn from use: what you open often ranks higher, and the empty
  search shows **Favorites** and **Suggestions**.
- `Ctrl+K` on a result: add to favorites (`Ctrl+Shift+F`), set an alias
  (`Ctrl+Shift+A`), set a hotkey (`Ctrl+Shift+H`), reset its ranking.
- Below the results, **Use "…" with…** offers what you typed to Google, to
  Ask AI, to Search Files and to your quicklinks that take a query, so
  `Enter` on a search with no results searches the web.
- Opening an application that is already running goes to its window
  instead of starting it again. Asked once more while that window is in
  front, it moves on to the application's next window, or, if it has only
  one, back to the window you were in before: an application's hotkey
  flips between it and your other work.

### Calculator

`2^10`, `15% of 80`, `5 km in mi`, `100 f to c`, `100 usd in cny` (rates
from open.er-api.com), `time in tokyo`, `3pm pst in beijing`. `Enter`
copies the answer.

### Hotkeys

Heyo is single-instance: running `heyo` again toggles the window of the
one that is running. A hotkey is therefore a desktop shortcut that runs
`heyo`.

- On GNOME (X11 and Wayland), Heyo's settings record these shortcuts for
  you, as GNOME custom shortcuts: General → Heyo Hotkey, and the hotkeys
  of Clipboard History and dictation.
- On other desktops, bind a shortcut to `heyo` in the desktop's own
  settings.
- On Windows, Heyo registers the hotkeys itself; the palette's is
  `Alt+Space` until another is chosen.

Any application, command (Heyo's own or an extension's), quicklink or
snippet can have its own hotkey: `Ctrl+K` → **Set Hotkey** in the root
search, or the Hotkey column in Settings → Shortcuts. Such a hotkey runs
`heyo --open <item id>`: applications launch without showing Heyo,
commands open in the palette, and snippets paste into the focused window.

### Command line

| Command | What it does |
|---|---|
| `heyo` | starts Heyo, or toggles the palette of the one running |
| `heyo --hidden` | starts it in the background |
| `heyo --clipboard` | opens Clipboard History |
| `heyo --dictate` | starts or finishes dictation |
| `heyo --open <item id>` | opens an item: `app:<desktop file>`, `cmd:<extension>:<command>`, `heyo:<command>`, `quicklink:<id>`, `snippet:<id>` |
| `heyo dev`, `build`, `migrate`, `ext check` | the tools for writing extensions ([developing-extensions.md](developing-extensions.md)) |

## Clipboard History

Heyo records what you copy (text, links, colors, file paths and images)
while it runs; open **Clipboard History** to search it, preview entries,
and paste (`Enter`), copy (`Ctrl+Enter`), pin or delete them. History is
kept for 3 months by default (pinned entries forever) in
`~/.local/share/heyo/clipboard/`, readable only by you.

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

Entries can also be found by what they mean ("that server login command"
finds an `ssh` line), in Chinese and English, with a model that runs on
your computer. It is off until you turn on **Find by Meaning** in the
settings, which downloads the model (about 135 MB, or 250 MB when text
recognition is not installed).

Not recorded: what a password manager marks as secret, anything copied
while a password manager's window is focused (KeePassXC, 1Password,
Bitwarden and the like), copies made in the applications you name in the
settings, what an extension copies as concealed, and Heyo's own pasting
during dictation. **Pause or Resume Clipboard History** stops recording
until you run it again.

`heyo --clipboard-eval [FILE]` measures the search: how often the right
entry is in the first 1, 5 and 10 results, by kind of query, and how long
a search takes. Without a file it uses the searches you picked a result
from, which the history records (and forgets with the entry); the file
format is described in `src-tauri/src/clipboard/eval.rs`.

## Quicklinks and snippets

**Create Quicklink** saves a URL (or path) template such as
`https://github.com/search?q={query}`; give it an alias like `gh` and type
`gh heyo` to open it directly. **Create Snippet** saves text to paste,
with an optional keyword that selects it in the root search. Both support
placeholders: `{query}`/`{argument}`, `{clipboard}`, `{date}`, `{time}`,
`{datetime}`, `{day}`, `{uuid}` and `{date format="yyyy-MM-dd"}`. They are
stored in `~/.local/share/heyo/quicklinks.json` and `snippets.json`.

Snippets can also expand as you type: with **Expand Snippets as You Type**
turned on in the settings (it is off by default), a snippet's keyword
typed in any application is replaced by its text. Heyo then listens to
the keyboard (nothing is grabbed, stored or logged) and leaves password
managers and the applications you exclude alone. X11 only. **Pause or
Resume Snippet Expansion** stops it until you run it again.

## Windows

**Switch Windows** lists the open windows, the one you were last in first,
so running it and pressing `Enter` goes back and forth between two
windows. `Ctrl+X` closes a window.

The window layout commands move the focused window: **Left Half**, **Right
Half**, top and bottom halves, the four quarters, thirds and two thirds,
**Maximize**, **Almost Maximize**, **Center** and **Move to Next
Display**. Give the ones you use a hotkey in Settings.

## Screenshots

**Capture Screenshot** freezes the screen: drag to select an area (drag it
or its handles to adjust) or click a window to take exactly that, mark it
up with rectangles, arrows, the pen or text, blur what should stay private
(anything you added can be dragged to another place), then copy it
(`Enter`), save it to `Pictures/Screenshots` (`Ctrl+S`), pin it on top of
other windows, or copy the text in it. A pinned capture can be dragged
around; double-click or right-click closes it.

**Capture Text from Screen** copies the text in the area you drag over,
using PaddleOCR on your computer (Chinese and English; a one-time download
of about 340 MB in Settings → Screenshots).

## Dictation

Hold the dictation hotkey while you speak and let go, or tap it, speak,
and tap it again: the text goes into the window you are working in.

A single key can do the same: choose a **Hold-to-Talk Key** (Right Alt,
Left Alt, Right Ctrl, …) in Settings → Dictation, hold it and speak, let
go to finish. The key keeps its normal meaning: a quick tap does nothing,
and so does using it in a shortcut such as Alt+Tab. A small window at the
bottom of the screen shows the microphone level, and the words as they are
recognized whenever they are not appearing in your application already.
Click it to finish; its × cancels.

- Speech is recognized on your computer with
  [SenseVoice](https://github.com/FunAudioLLM/SenseVoice) (Mandarin,
  Cantonese, English, Japanese, Korean; mixed Chinese and English works).
  Set it up once in Settings → Dictation; the download is about 270 MB.
- With **Polish with AI** set to *Clean Up* or *Translate to English*,
  the transcript (not the audio) is sent to the model when you finish,
  and the result is what stays.
- A vocabulary of your own words (names, jargon) is applied to what was
  recognized: Settings → Dictation.
- Text goes in through the clipboard, which is restored afterwards;
  terminals get `Ctrl+Shift+V`.
- `heyo --dictate` starts or finishes dictation, for binding your own
  shortcut.

### Type as you speak

On X11 the words go into the application while you are still speaking:
each appears a moment after it is recognized, and when a later word makes
the recognizer change an earlier one, the text is corrected where it
stands. A finished sentence is what the recognizer decided in the end.
With polishing, what you said is there as it was heard until you finish,
and the polished text then takes its place. This is **Type as You Speak**
in Settings → Dictation, and it is on unless you turn it off.

Correcting means erasing, and Heyo erases only what it wrote itself in the
recording under way. To be sure of that it follows the keyboard and the
mouse while you dictate (it listens and keeps nothing), and holds to this:

- Once you press a key or click during a recording, nothing more is
  erased. What is in the application stays as it is, and each sentence you
  finish from then on is pasted at the cursor. Polished text that can no
  longer take the place of what was heard is left out, and the dictation
  window says so. Not counted as a key of yours: the key held to talk,
  Shift, Ctrl and the like by themselves, the dictation hotkey set in
  Heyo's settings, the mouse wheel, and a click on the dictation window.
  A shortcut for `heyo --dictate` that you bound elsewhere is a key like
  any other.
- Cancelling takes the recording's words out again, if nothing else has
  touched the text since; otherwise they stay, and the window says that
  they did.
- While you hold the dictation hotkey down, the desktop has the keyboard
  to itself: the dictation window shows the words meanwhile, and they go
  into the application when you let go.
- Text is corrected only while it is made of characters every
  application erases one at a time (letters, digits, punctuation, Chinese,
  Japanese and Korean writing, simple emoji). A sentence with a combining
  accent or an emoji put together from several is written once, and from
  then on sentences are added without correcting.

In some places each sentence is pasted when you pause after it, and
polished text once at the end, as with the setting off:

- terminals (a shell would cope with being corrected, an editor running
  in the terminal might not), where several lines also go in as one;
- password managers, the applications you excluded from snippet expansion,
  and Heyo's own windows;
- editors with modes (gVim, Emacs), spreadsheets, and windows onto another
  computer (remote desktops, virtual machines), where Backspace and paste
  do not simply erase and insert at a cursor;
- GNOME on Wayland and Windows, where Heyo cannot hear whether you typed
  in between and so must not erase.

## AI

The AI commands use Claude (an API key in Settings → Advanced, or
`ANTHROPIC_API_KEY` in the environment) or a model on your own computer.
Without either they say so and do nothing else.

### Ask AI and AI Chat

Type a question in the root search and press `Tab`: the answer streams in.
`Enter` asks a follow-up that keeps the conversation so far; the answer
can be copied or pasted. **Ask AI** is also a command of its own, and **AI
Chat** goes back to a conversation and continues it. Conversations are
kept on your computer.

### Selected Text Actions

Select text in any application and run **Selected Text Actions** (give it
a hotkey): Heyo reads the selection without you copying it and offers to
translate it, improve the writing, fix spelling and grammar, shorten,
explain or summarize it, or to search the web for it. The answer streams
in; `Enter` replaces the selection with it (or copies it, for
explanations). `Ctrl+N` adds an action of your own: a name and the
instruction the model gets, for example "Translate the text into natural
Japanese".

### Local model

In Settings → Advanced → AI choose **Local Model** and point it at any
OpenAI-compatible server (vLLM, LM Studio, Ollama, llama.cpp); the default
address is `http://127.0.0.1:8000/v1`. The text then never leaves your
computer.

`scripts/local-model.sh start` runs one with vLLM in Docker: Qwen3-4B
Instruct by default, about 8 GB to download, listening only on this
computer. `stop`, `status` and `logs` do what they say. vLLM reserves GPU
memory up front, 12% by default (`HEYO_LLM_MEMORY`), which matters on
machines where the GPU shares memory with the system. The container does
not restart by itself after a reboot.

A small model is quick and fine for translating and tidying text, but it
follows instructions less reliably than Claude on long or tricky input.

## Live activities

Something in progress stays visible while its command is away: the
**In Progress** section at the top of the palette, and a notification
when it ends or needs you. **Timer** is the built-in one: type `timer`,
`Tab`, a length of time (`25m`, `1h 10m`, `90s`) and `Enter`; it counts
down in the palette, with **Pause** and **+1 min**. Extensions written for
Heyo can show their own. Settings → General turns the section off.

## System and utilities

- **Lock Screen**, **Sleep**, **Restart**, **Shut Down**, **Log Out** and
  **Empty Trash** (the last four ask first), **Toggle Dark Mode**, **Volume
  Up**/**Down**, **Toggle Mute**, and **Toggle Keep Awake**, which stops
  the computer from sleeping until you turn it off or quit Heyo.
- **Search Emoji & Symbols**: search in English or Chinese; `Enter` pastes.
- **Search Files**: fuzzy search over the names of the files and folders
  under your home folder (hidden folders, `node_modules` and the like are
  skipped). The names are read into memory when the command opens (a
  fraction of a second, about 80 MB per million files) and released a few
  minutes after you last used it; there is no index on disk.
- **Kill Process**: search by name, command line or listening port (`3000`).
- **Switch Audio Device**: pick the default speaker and microphone.
- **Search Browser Bookmarks** and **Search Browser History**: Firefox,
  Chrome, Chromium, Brave, Edge and Vivaldi. Read from a copy of the
  browser's own files; needs Node.js 22.5 or newer.
- **Paste as Plain Text** pastes what you copied without its formatting;
  give it a hotkey.

## Raycast extensions

Run **Search Raycast Store** to browse the store and press `Enter` to
install an extension; its commands then appear in the root search.
**Manage Extensions** checks the store for updates (update one or all),
configures preferences and uninstalls.

Extensions are the same builds Raycast downloads, unpacked into
`~/.local/share/heyo/extensions/<name>/`. An extension built locally with
`heyo build` (or `ray build -e dist`) can be copied there too.

Inside a command: `Enter` runs the primary action, `Ctrl+Enter` the second
one (or submits a form), `Ctrl+K` opens all actions, and `Esc` goes back.
Shortcuts written for macOS map `⌘` to `Ctrl`.

What works: `List`, `Grid`, `Detail`, `Form`, action panels, navigation,
toasts, alerts, clipboard, `open`, `LocalStorage`, `Cache`, preferences,
arguments, OAuth, menu bar commands, background refresh, window
management, `AI.ask` and the `@raycast/utils` hooks. Not supported: the
browser extension API, and extensions that only work on macOS
(AppleScript, Swift binaries). [compatibility.md](compatibility.md)
measures how the 100 most-downloaded extensions fare.

- **Menu bar commands** appear as icons in the top bar (GNOME needs the
  AppIndicator extension, which Ubuntu ships enabled). Run one from the
  root search once to turn it on; from then on it comes back when Heyo
  starts and refreshes at the interval its extension asks for. Commands
  that refresh in the background work the same way: they start doing so
  after you have run them once. Disable a command in Settings to stop it.
- **`AI.ask`** answers with the model chosen in Settings → Advanced → AI.
- **Signing in (OAuth)**: an extension that connects to a service opens
  its sign-in page in your browser. Those services send you back through
  a `raycast://` link, so Heyo registers itself as the handler for
  `raycast://` and `com.raycast:` links
  (`~/.local/share/applications/heyo-links.desktop`); the browser may ask
  once whether to open it. Tokens are stored per extension under
  `~/.local/share/heyo/support/`, readable only by you.
- **Permissions**: reaching other applications' windows, the screen, the
  microphone and the like is yours to allow. The palette asks the first
  time an extension wants one; Allow is kept until you take it back on the
  extension's page in the settings. This is consent and a record, not a
  sandbox: an extension is a Node.js program and can read your files.

## Settings and where things are kept

The settings window is laid out like Raycast's: a sidebar with **General**
(open at login, the hotkey, the theme, window behavior), **Shortcuts**
(aliases and hotkeys for every command and application), **Dictation**,
**Screenshots**, **Advanced** (Node.js, the AI model) and **About**
(the version, Copy Diagnostics), followed by **Applications**, **Heyo
Commands** and one page per extension (enable or disable commands,
preferences, permissions, uninstall).

| What | Where |
|---|---|
| Settings | `~/.config/heyo/settings.json` |
| Extension preferences | `~/.config/heyo/preferences/` |
| Extensions | `~/.local/share/heyo/extensions/` |
| What extensions store (`environment.supportPath`, `LocalStorage`, tokens) | `~/.local/share/heyo/support/` |
| Clipboard history | `~/.local/share/heyo/clipboard/` |
| Quicklinks and snippets | `~/.local/share/heyo/quicklinks.json`, `snippets.json` |
| An extension's `Cache`, captures, the log (`heyo.log`) | `~/.cache/heyo/` |

On Windows, settings and data are in `%APPDATA%\heyo`, caches and the log
in `%LOCALAPPDATA%\heyo`.

## What Heyo uses when it is installed

None of these is needed to launch applications; each serves the features
named.

| Program | For |
|---|---|
| Node.js 22 or newer (`node` on `PATH`, or its path in Settings → Advanced) | extensions and Heyo's own view commands, dictation, text recognition |
| `xdotool` | the paste keystroke on X11: Clipboard History, snippets, emoji, dictation |
| `wtype` or `ydotool` | the same on Wayland, where Heyo's GNOME Shell extension is not there to do it |
| `parec` (PulseAudio or PipeWire) or `arecord` | recording, for dictation |
| `pactl` | the volume commands, Switch Audio Device, choosing the microphone |
| `notify-send` | notifications |
| `gnome-screenshot` | capturing, where Heyo cannot read the screen itself |
| `npm` | installing the text-recognition engine |
