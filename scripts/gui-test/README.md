# The GUI sandbox

What only shows in a window is tried here: a build of Heyo on an X server
of its own (Xvfb), with a session bus, a window manager, a session that
publishes its display as a real one does, and home directories of its own.
It is driven with `xdotool` and photographed. Nothing of the desktop you
work on is touched, and your own Heyo can stay running.

There is one sandbox. Every script here, and the measuring scripts kept in
`experiments/slint-settings/scripts/`, start from `env.sh` and go through
`sandbox.sh`.

## Commands

| `sandbox.sh …` | |
|---|---|
| `up [xfwm\|gnome]` | the session and Heyo, hidden |
| `session [xfwm\|gnome]` | the session only |
| `heyo [VAR=value…]` | Heyo, hidden, in the session that runs (its log is `app.log`) |
| `stop NAME…` | ends what was started under those names (`heyo`, …) |
| `down` | ends everything the sandbox's scripts started |
| `scale N` | the session's scale becomes N, under what runs |
| `screen WIDTHxHEIGHT` | the screen's size becomes that, under what runs |
| `monitors N` | the screen is N monitors side by side (1: one again) |
| `check` | what the session publishes of its display |
| `ask …` | where a control of a Slint window is, from the window itself (below) |
| `ibus [ENGINE]` | an input method of the sandbox's own (GNOME Shell starts one itself) |
| `look` | the windows on screen, who has the keyboard, Heyo's settings and memory |
| `census` | every process in the sandbox, written down or not |
| `run COMMAND…` | a command in the sandbox's environment |

`xfwm` is xfwm4 (compositing with `HEYO_TEST_COMPOSITING=1`); `gnome` is
GNOME Shell, the compositor Heyo is used under, drawing in software.

| Variable | Default | What it sets |
|---|---|---|
| `HEYO_TEST_DIR` | `/tmp/heyo-gui-test` | where the sandbox keeps its home directories, the ledger (`pids`), logs, pictures (`shots/`) and what scripts wrote down (`results/`) |
| `HEYO_TEST_DISPLAY` | `:99` | the X display; two sandboxes at once need one each, and a directory each |
| `HEYO_TEST_BUILD` | `debug` | `release` or another directory under `src-tauri/target` whose `heyo` carries its own interface (no dev server is started for it), or the path of a `heyo` built elsewhere |
| `HEYO_TEST_FRONTEND` | | `built` serves a production build of the interface to the debug binary, for measuring; `none` starts no server |
| `HEYO_TEST_SESSION` | `xfwm` | the session `up` and `session` start when none is named |
| `HEYO_TEST_SCALE` | `1` | the scale the session starts at |
| `SCREEN` | `1920x1080` | the size of the screen, and the largest `screen` can make it |
| `HEYO_TEST_WARMUP` | 6, or 15 for a debug build | seconds Heyo is given after it says it started |
| `HEYO_TEST_RESULTS` | `$HEYO_TEST_DIR/results` | where a script that writes down what it saw puts it |

It needs `Xvfb`, `xdotool`, `xwininfo`, `xrandr`, `xrdb`, `dbus-daemon`,
`xfwm4` with `xfsettingsd` and `xfconf-query` (or `gnome-shell` with
`gsd-xsettings`), Python with Pillow, and the system's Python with the
GLib bindings (`python3-gi`, `gir1.2-atspi-2.0`). The scripts that dictate
also need the speech engine installed for your own Heyo (they read it
through a link, and write nothing there), Flite (`libflite1`) to speak,
and, for what goes through the input method, IBus with `ibus-libpinyin`,
`gnome-terminal` and `gsd-media-keys`.

## What is whose

- **Processes.** Every process is started through `start` (`env.sh`),
  which has `ledger.py` write its number down with when it was started.
  `down` and `stop` end those, each only while it is still that process,
  and what they started themselves. Nothing is ever ended for its name or
  its environment. What the session's bus starts by itself (the settings
  service, the accessibility bus, the Shell's helpers) ends with the bus;
  what is left after `down` is said, with its number, and not touched.
- **Files.** `HOME` and the XDG directories are the sandbox's, so what
  Heyo or the Shell keeps stays in `$HEYO_TEST_DIR/home`. Settings are
  `home/config/heyo/settings.json`: write them before starting Heyo
  (`seed` in `env.sh` writes a set to start from).
- **The display.** `env.sh` refuses `:0` and `:1`, and `session` refuses a
  display that is in use by something it did not start.

## The display it publishes

The scale is said as a desktop says it and nowhere else. `env.sh` unsets
`GDK_SCALE` and `WINIT_X11_SCALE_FACTOR`: with those set a program never
has to find the scale, and every fault in finding it stays hidden (which
is how a half-size screenshot toolbar on a real 200 % display passed here
for a while).

- Under GNOME Shell the Shell lays its monitor out at the scale, the
  sandbox's own dconf has it (`org.gnome.desktop.interface
  scaling-factor`), and GNOME's settings daemon publishes it: XSETTINGS
  (`Gdk/WindowScalingFactor`, `Gdk/UnscaledDPI`, `Xft/DPI`,
  `Gtk/CursorThemeSize`) and the resource database. At scale 2 `xrdb
  -query` reads `Xft.dpi: 192` and `Xcursor.size: 48`, as a GNOME desktop
  at 200 % does; at 1, 96 and 24.
- Under xfwm4, Xfce's settings daemon is given the same figures to
  publish, name for name, from the sandbox's own xfconf. (An Xfce desktop
  at 2× says less: its `Xft.dpi` stays 96. The sandbox stands for the
  GNOME desktop under another window manager.) The daemon's clipboard
  manager is left off: it is Xfce's own. Without `xfsettingsd` only the
  resource database says it.

`sandbox.sh scale N` changes it under what runs, as connecting a monitor
of another density does. `sandbox.sh check` prints what a session
publishes (`session.py check` only reads, so it can be run on your own
desktop to compare).

How far Xvfb goes: it has one output and the memory it was started with.
`screen` can make the screen any size up to `SCREEN` (a mode is added for
it) and back; the Shell lays it out anew and programs are told (RandR).
A second output cannot be added, so a second monitor as a desktop has it,
with a scale of its own, cannot be had. `monitors N` splits the screen
into RandR 1.5 monitors, and that goes only this far: `xrandr` lists
them, GTK takes the first for its one monitor (a share of the screen) and
drops the others, which have no output, and the Shell and winit, who read
outputs, see nothing change.

## Asking a window where a thing is

Slint's windows are on the session's accessibility bus (AT-SPI, through
AccessKit), so a script asks instead of carrying the layout as numbers:

```sh
ask tree "Heyo Settings"                                   # role, name, rectangle, states
ask click "list item" "Dictation" "Heyo Settings"          # a page, by its entry in the sidebar
ask where "toggle button" "Compact Mode" "Heyo Settings"   # X Y W H on the screen
```

(`ask` in a script that sources `env.sh`; by hand, `sandbox.sh ask …`.)

AccessKit stays quiet until an assistive tool is there; the first `ask`
says one is (`org.a11y.Status`, on the sandbox's bus) and Heyo's windows
appear within a second. The settings window names its rows, switches,
buttons and fields. It says only what is in view: a control further down
a page is there once the page is scrolled to it.

The screenshot overlay and the dictation pill are asked too. The toolbar
is a panel named Tools, its buttons are named (Rectangle, Arrow, Pen,
Text, Blur; Red, Yellow, Green, Blue; Undo; Copy Text, Pin, Save, Copy,
Cancel), the tools and the colours saying which is chosen, and the
selection is a panel named Selection. The pill's body is a button named
Dictation and its X one named Cancel. `env.sh` has them as `toolbar
BUTTON`, `toolbar_box` and `pill_at body|cancel`; no script carries a
window's layout as numbers.

## The scripts

A script is run by its path. The first group starts the sandbox, does its
thing, prints what it saw and stops the sandbox again. The second uses the
session that runs if one does, and leaves it running, so that several can
share a session (GNOME Shell at 3840x2160 takes a while to start); with
none running each starts and ends its own. They write what they saw to
`$HEYO_TEST_RESULTS`.

```sh
scripts/gui-test/look.sh light before       # pictures of the root search, a command and the settings
scripts/gui-test/regress.sh                 # the platform layer on X11, end to end

SCREEN=3840x2160 HEYO_TEST_SCALE=2 scripts/gui-test/sandbox.sh session gnome
scripts/gui-test/screenshot-safe.sh         # these share that session
scripts/gui-test/display-change.sh 2 1
scripts/gui-test/sandbox.sh down
```

| Script | What it tries |
|---|---|
| `look.sh`, `compact.sh`, `select.sh` | pictures of the interface: the themes, compact mode, the selection moving |
| `language.sh` | the language changed while Heyo runs: the root search, the clipboard history with its actions and the settings pictured in English, then in 简体中文 chosen in Settings → General, then the settings in English again |
| `settings.sh` | the settings window: every page pictured a screen at a time in both themes, a group opened on Shortcuts, Copy Diagnostics pressed and read back, a permission taken back |
| `open.sh`, `arguments.sh`, `views.sh` | opening commands, arguments typed in the search bar, what an extension's views do |
| `regress.sh`, `clipboard.sh`, `snap.sh`, `expand.sh`, `dictate.sh` | the platform layer on X11: pasting, layouts, the clipboard history, the screenshot overlay, snippets expanding, dictation |
| `dictate-live.sh` | dictated words going into another application as they are spoken, by pasting (xfwm, no input method) and through the input method (GNOME Shell, IBus with Pinyin): the script's head lists each recording and what must hold |
| `heyo-api.sh`, `windows-api.sh`, `activities.sh` | `@heyo/api` as an extension sees it, the permission question, live activities |
| `ai-chat.sh` | Ask AI and AI Chat against `llm-stub.py`, a server that answers with canned text |
| `cli-models.sh` | Claude Code and Codex as the model, through `cli-stub.py` in their place on the PATH: the settings' AI card, Ask AI continued in AI Chat, a dictation polished; the real tools are never run |
| `dev.sh` | `heyo dev` against a running Heyo |
| `scroll.sh` | how smoothly lists scroll, measured in the web view |
| `readme-shots.sh`, `readme-dictation.sh` | the README's pictures ([development.md](../../docs/development.md#the-readmes-pictures)) |
| `demo-film.sh [capture\|edit\|all]` | the film that shows Heyo around ([below](#the-film)) |
| | **Sharing a session:** |
| `display-change.sh FROM TO` | the display changed under a running Heyo (the scale, the screen's size, the monitors), every window used before and after and judged: sizes, a held click on a switch, the pointer's size against a GTK window's, the toolbar as drawn, a part saved pixel for pixel, the pill's place, the palette centred. Expected to pass in both directions |
| `settings-window.sh` | the settings window as a window: moved by its top edge, Tab and Space, Esc, asked for again |
| `settings-lost.sh panic\|no-start` | Slint lost or never started: Heyo goes on, and says so |
| `settings-do.sh …` | does things to the settings window on screen, for the two above and by hand |
| `pill.sh [NAME] [lost]` | the dictation pill never takes the keyboard, a click finishes, its button cancels, it is above a fullscreen window; and with Slint lost dictation goes on |
| `screenshot.sh` | the overlay's pixels against the desktop it froze: inside the selection, dimmed outside, the desktop again afterwards |
| `screenshot-tools.sh` | each tool, undo, save and copy checked against the screen, a window selected by a click, Escape and right click, the pointer's shapes, pin |
| `screenshot-pin.sh` | pinned captures: where they are, what they show, dragged, closed three ways |
| `screenshot-safe.sh [escape\|panic\|stall\|kill]` | the overlay cannot trap whoever uses it: the keyboard comes back whatever happens to what draws it |
| `slint-lost.sh` | nothing of Heyo's is left on the screen when what draws it stops answering: the pill and two pinned captures are taken off, and Heyo goes on |
| `updates.sh` | serves an update on this computer for About's "Check for Updates" to find |

Two things to know when writing one. Under GNOME Shell a notification is
a banner at the top of the screen ("Screenshot copied"), which stays
until the Shell has seen the pointer move: a script that compares the
screen with what it was calls `banners off` first (`env.sh`). And the
test window (`scripts/wayland-test/in/window.py`) writes its text and its
process number to `$WINDOW_DIR/window-APP_ID.txt`, which is the sandbox's
own directory: each sandbox reads its own window.

The parts they share: `grab.py` (a picture of the screen), `screen-read.py`
(the clipboard's picture, the pointer's shape and size, two pictures
compared), `screenshot-pixels.py`, `toolbar.py`, `seed.py` (settings to
start from), `say.py` and `say-zh.cjs` (speech in place of a microphone,
in English and in Chinese with English in it), `clip.py`,
`llm-stub.py`, `cli-stub.py`, `menubar.py`, and `extensions/`, the small extensions some
of them install.

## The film

`demo-film.sh` makes the film that shows Heyo around, in two stages, so
that the cutting can be changed without filming again:

```sh
scripts/gui-test/demo-film.sh            # both: $HEYO_TEST_DIR/film/heyo.mp4 and heyo.webm
scripts/gui-test/demo-film.sh capture    # films the sandbox: film/capture.mkv, film/moments.jsonl
scripts/gui-test/demo-film.sh edit       # makes the film of what was filmed, again
```

- **Capture.** The real Heyo in the sandbox's GNOME session at 3840x2160
  and scale 2, used as a person would use it, one scene after another
  (`scene_…` in the script), the whole screen filmed. Beside the film it
  writes the moments: when each thing happened and where on the screen it
  is, asked of the windows (`mark`). Nothing in the edit carries a place as
  a number that the capture could have told it.
- **Plan.** `film/plan.py` lists, beat by beat, everything that is seen
  to happen on one: the sections, the cuts, the sweeps between scenes, the
  windows going to their places, each word of each caption, each letter
  typed in the opening and on the last card. The edit puts the pictures
  there and the music is written from the same list. At 120 beats a minute
  and 24 pictures a second, every sixteenth of a beat is 3 pictures.
- **Music.** `film/music.py` writes the music: a sound for each thing in
  the plan (an impact on the palette, a tick for each letter typed, a clap
  under each caption word, a crash on each cut, a hat for each blink of the
  caret) over drums, bass, chords, an arpeggio and a hook, each sound
  computed (oscillators, filtered noise, envelopes), mixed with a
  kick-keyed duck, reverb, delay and a true-peak limiter to -14 LUFS.
  Nothing is fetched or sampled, so no licence comes with it. Run alone it
  prints its loudness, peaks, spectrum and how far the notes an onset
  detector finds are from the grid.
- **Edit.** `film/edit.py` is the cutting: a list of shots (`storyboard`),
  each so many beats long, saying which stretch of the capture it shows,
  how fast, and where the camera is. What must happen on a beat is found in
  the capture by the picture in which the screen changes, and the picture
  is held or hurried so that it lands there (each letter of the opening's
  search is held from the picture it appears in). It writes the plate (the
  capture retimed to the film, still 4K), the sound (the music's chords
  turned down under what is dictated, its hits less), and `data.js` for the
  composition.
- **Composition.** `film/composition` is a
  [HyperFrames](https://github.com/heygen-com/hyperframes) project: HTML
  with GSAP, rendered picture by picture in headless Chromium and encoded by
  FFmpeg. It moves the camera over the plate, punches it on the kicks,
  flashes each window where it lands, and sets the type: the title, the
  captions whose words arrive on the beat, the sweeps between scenes and
  the end card.
- **Measured.** `film/sync.py` finds each thing of the plan in the rendered
  film's pictures and the notes in its sound with an onset detector, and
  writes `film/sync.txt`: for each, the note heard with it, how far it is
  from the picture, and how strong against the median note. Nobody needs
  to listen to know whether it is on the beat. `film/cuts.txt` says how far
  each cut is from its beat.

The edit runs under a Python of its own (numpy, scipy, librosa, and an
ffmpeg with H.264 from `imageio-ffmpeg`), a virtual environment that
`demo-film.sh` makes with `uv` the first time, in `$HEYO_FILM_CACHE/venv`,
and under HyperFrames at a fixed version, which it puts there with npm
(Node 22 or later). HyperFrames needs `ffprobe` and a headless Chromium;
where its own download has none (Linux on ARM), name one:
`HEYO_FILM_BROWSER` (Playwright's `chrome-headless-shell` serves) and
`HEYO_FILM_FFPROBE`. `HEYO_FILM_DRAFT=1` makes a quick film;
`HEYO_FILM_STILLS=1` also writes the first, middle and last picture of
every shot and each caption at its fullest to `film/stills/`, to look the
film over.
