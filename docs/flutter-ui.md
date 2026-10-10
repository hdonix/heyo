# The Flutter interface

Heyo has two interfaces. The web view (`src/`, React in Tauri's windows) is
the one it has always had and the default. The other is drawn by Flutter
(`ui/`): where the web view can only draw on the processor (WebKitGTK with
NVIDIA's driver on Linux), Flutter draws with the graphics card. It is chosen
when Heyo starts:

    heyo --ui=flutter          # or HEYO_UI=flutter in the environment

Everything that is not a window (the index, the clipboard, the extension
runtime, hotkeys, the tray, the platform layer) is the same program either
way. In Flutter mode no web view is created at all.

**Where it stands.** The foundation is in place and one path through it works
end to end: the palette comes up, searches the real index, opens applications
and commands, and hides; the settings, the screenshot overlay, a pinned
capture and the dictation pill have their windows, with placeholders in them.
The views themselves (the full root search, what extensions show, the
settings pages, dictation, the screenshot tools) are still the web view's
alone. Linux only: there is no Windows runner yet.

## How it is put together

```
                          heyo  (Rust; src-tauri/)
 ┌──────────────────────────────────────────────────────────────────────┐
 │ index · clipboard · runtime (Node) · hotkeys · tray · platform layer │
 │                                                                      │
 │ commands  one function each (#[tauri::command])                      │
 │    ▲              ▲                                                  │
 │    │ Tauri        │ shell/dispatch.rs: the same functions,           │
 │    │ invoke       │ by the same names, from the same JSON            │
 │    │              │                                                  │
 │ shell/mod.rs   "the interface": window(label) → show, hide, focus,   │
 │    │           emit; open_settings, open_pin; which one is in use    │
 │    ├── shell/webview.rs   Tauri windows (the default)                │
 │    └── shell/flutter.rs   one heyo-ui process for each window        │
 │           │  commands/access.rs: which window may invoke what        │
 └───────────┼──────────────────────────────────────────────────────────┘
             │ JSON, a line each way, over the process's stdin/stdout
             │ (shell/bridge.rs)
   ┌─────────┴────────┬──────────────────┬───────────────┬─────────────┐
   ▼                  ▼                  ▼               ▼             ▼
 heyo-ui            heyo-ui            heyo-ui         heyo-ui       heyo-ui
 --window=main      --window=settings  --window=       --window=     --window=pin
 the palette        (while open)       dictation       screenshot    --label=pin-N
 (always, hidden)                      (kept once      (kept once    (while open)
                                       used)           used)

 Each heyo-ui (ui/):
   linux/runner/        the GTK window; method channel `heyo/window`
   lib/host/            Host: invoke(), events; HeyoWindow; models
   lib/theme/           tokens, colour scheme, fonts
   lib/widgets/         what every view is made of
   lib/<view>/          palette, extension, settings, dictation, screenshot, island
```

**One process for each window.** Flutter's several-windows support is not on
the stable channel (3.41: `enable-windowing` is unavailable), and a process
that hangs or dies takes one window with it, not Heyo. The cost is memory:
see [Measured](#measured).

**The host started the process, so it knows which window is asking.** A
command invoked over the bridge is held to the same table as the web view's
windows (`commands/access.rs`): the palette cannot change settings, the
dictation pill cannot open anything.

**The same commands, the same events.** A command is one Rust function. The
web view reaches it through Tauri; `heyo-ui` through `shell/dispatch.rs`,
which calls the same function with arguments read from the same JSON, and
runs it the way Tauri does (in order, or on a thread of its own). Tests hold
the table to the list of registered commands and to how each is declared, so
a command added for one interface exists for the other. Events emitted to
every window (`app.emit`) are heard in one place (`listen_any`) and passed to
every process; an event for one window goes through `Window::emit` and
reaches that process alone. (What the runtime says, `heyo://runtime`, goes to
the palette's process only: it is whole views, many times a second.)

## Running it

    scripts/build-ui.sh                  # or: pnpm build:ui    → ui/build/linux/<arch>/release/bundle/
    (cd src-tauri && cargo build)
    HEYO_UI=flutter src-tauri/target/debug/heyo

Flutter must be installed (`FLUTTER=/path/to/flutter scripts/build-ui.sh` if
it is not on `PATH`). Nothing else needs it: `pnpm tauri build`, the tests
and the web view do not know `ui/` exists.

Where `heyo` looks for `heyo-ui`:

1. `HEYO_UI_BIN`, if set (not checked for its version: yours to keep in step);
2. a debug build: `ui/build/linux/<arch>/release/bundle/heyo-ui` in the repository;
3. otherwise `ui/heyo-ui` in the resource directory, beside the helper
   scripts (`~/.local/lib/Heyo/ui/` from `scripts/install.sh`, which installs
   it when it has been built).

Heyo runs `heyo-ui --version` first and refuses one built for another version
(the runner is compiled with the version in `tauri.conf.json`). If the
Flutter interface cannot be used, Heyo says why, in its log and as a
notification, and starts with the web view.

For working on a view without Heyo:

    ui/build/linux/arm64/release/bundle/heyo-ui --standalone --window=main
    … --window=settings | dictation | screenshot | pin | island
    HEYO_UI_APPEARANCE=dark … --standalone --window=main
    HEYO_UI_COVER=0,0,1920,1080 … --standalone --window=screenshot   # as the overlay: takes the keyboard

The host is then a `FakeHost` with made-up rows (`lib/dev/standalone.dart`).
`flutter run -d linux -a --standalone -a --window=main` should give hot reload
(not tried).

For looking at what goes on:

| | |
|---|---|
| `HEYO_UI_DEBUG=1` | Heyo logs what each window reports and how long showing took; each window logs what the host tells it |
| `--bench=keys` or `scroll` (standalone), `HEYO_UI_BENCH=…` (under Heyo) | the palette drives itself once shown and prints frame times as a line starting `BENCH` |

## The bridge

One message a line, JSON, over the process's stdin (from Heyo) and stdout (to
Heyo). `shell/bridge.rs` and `ui/lib/host/host.dart` are the two ends.

From a window:

    {"id": 7, "invoke": "search", "args": {"query": "se"}}     → {"id": 7, "result": […]} or {"id": 7, "error": "…"}
    {"window": "ready"}                                         first frame drawn
    {"window": "focused"} / {"window": "blurred"}               it gained / lost the keyboard
    {"window": "close"}                                         the user asked to close it (Alt+F4)
    {"window": "shown", "ms": 12}                               drawn after a "show"
    {"pong": 3}

To a window:

    {"event": "heyo://shown", "payload": null}
    {"window": "show"} "hide" "focus" "center" "close"
    {"window": "frame", "x": …, "y": …, "width": …, "height": …, "logical": false}
    {"window": "title", "title": "…"}
    {"window": "overlay", "x": …, "y": …, "width": …, "height": …, "backdrop": true}
    {"ping": 3}

**stdout belongs to the bridge.** Anything else written there is not a
message. `main.dart` sends Dart's `print` to stderr, which joins Heyo's log;
do not write to `stdout` yourself.

When Heyo exits, or closes a window for good, the process's stdin closes and
the process ends: Dart sees the end of the stream, and a thread in the runner
watches for it too, so that a process whose Dart side hung still ends with
Heyo.

## Windows

A window is known by its label, the same as the web view's: `main`,
`settings`, `dictation`, `screenshot`, `pin-N`. `heyo-ui` is told
`--window=<kind> --label=<label> --init=<JSON>`; `--init` holds what the
window is like (`title`, `width`, `height`, `focus`, `above`, `taskbar`,
taken from `tauri.conf.json` so that one place says how large the palette is)
and what the view is started with (a pinned capture's `path`).

| Window | Process lives | Notes |
|---|---|---|
| `main` | from Heyo's start, hidden; started again if it dies | so that the palette appears at once |
| `dictation`, `screenshot` | from first use, then kept hidden | first use waits for the process (about 0.3 s) |
| `settings`, `pin-N` | while the window is open | closing the window ends the process |

**The host decides when a window shows and hides**, because it has to know
(to toggle the palette, to put a question up in it). A view never hides its
own window: it invokes `hide`, as the web view does, or `WindowScope.of(context).requestClose()`.

- *Losing the keyboard.* The runner reports it; the host waits 150 ms, checks
  that the window did not get it back and (where the desktop knows better
  than the toolkit) asks the desktop, then hides the palette if `hideOnBlur`
  is on. This is `platform/focus.rs`'s rule, in `shell/flutter.rs`.
- *Closing (Alt+F4).* The runner asks Dart, Dart asks the host: the palette
  and the dictation pill hide, the screenshot overlay cancels, the settings
  and pinned captures end.
- *Nothing is done to a window before Flutter's first frame.* What the host
  says meanwhile is queued (`HeyoWindow`), and events are held until the views
  listen (`HostConnection.open`). A window mapped before the first frame
  stays empty for good; the host often speaks at once (it starts the overlay's
  process when there is a capture to show).
- *The screenshot overlay* (`overlay` → the runner's `cover`) is put over a
  monitor above everything. On X11 it bypasses the window manager and grabs
  the keyboard, as the web view's does. `backdrop: true` says the host already
  shows the dimmed capture in a window of its own underneath; `false` (no
  compositor, Wayland, xfwm4) means the view must paint the capture itself.
  Whatever the overlay shows **must always let go at Escape**. If its process
  stops answering pings for five seconds while it is up, Heyo kills it, which
  releases the keyboard.

### The runner's channel

`ui/linux/runner/heyo_window.cc` is everything GTK or X11. Dart reaches it
through the method channel `heyo/window` (`lib/host/window.dart` lists the
methods); a Windows runner has to offer the same channel:

`configure {title, width, height, focus, above, taskbar, type, resizable}`,
`show`, `hide`, `focus`, `center`, `close`, `startMove`,
`setFrame {x, y, width, height, logical}`, `geometry`, `setInputRegion {rects}`,
`cover {x, y, width, height}`; and back: `focusChanged(bool)`, `closeRequested()`.

Views use `HeyoWindow` (from `WindowScope.of(context)`): `setFrame`,
`geometry()`, `setInputRegion()` (click-through outside the given rectangles),
`startMove()` (a title bar Heyo draws), `focused`, `visible`, `covering`.

`setInputRegion` is what lets a window be larger than what it shows: the
island's window has room for its card to grow into, and only the pill takes
the pointer (`lib/island/island.dart`). Under GNOME Shell a click beside the
pill reaches the window underneath, and the window under the pointer keeps
the keyboard. xfwm4 does not pass clicks through a window it manages,
whatever its input region: there such a window needs to be one the window
manager leaves alone, which the channel does not offer yet outside `cover`.

### Scale

The experiment that preceded this found its window at twice the web view's
size on a 4K display. With the same toolkit settings the two are the same
size: started with `GDK_SCALE=2` in the sandbox, both the web view's palette
and Flutter's are 1628 × 1068 physical pixels. They differ only when the two
processes see different settings (for one, a `heyo` started at login before
the desktop had announced its scale, and a `heyo-ui` started later). So on
X11 the host passes its own scale to every `heyo-ui` it starts, as
`GDK_SCALE`: the Flutter interface is the size the web view would be,
whatever each would have worked out alone. (On Wayland each monitor has its
own scale and the toolkit follows it.) What happened on that display itself
could not be looked at from here.

## Where things go

```
ui/
  lib/
    main.dart            arguments, the host, the window, runApp
    app.dart             which view a window shows (viewFor)
    host/                the host: Host, HostConnection, FakeHost; HeyoWindow;
                         commands.dart (typed commands), models.dart;
                         events.g.dart, commands.g.dart (written by cargo test)
    theme/               tokens.dart, theme.dart, fonts.dart
    widgets/             shared: text, image, item_icon, list, panel, overlays,
                         selection, text_field, keys, placeholder
    palette/             the root search                      ← one owner each
    extension/           what a command shows (lists, grids, details, forms)
    settings/            the settings window
    dictation/           the dictation pill
    screenshot/          the overlay, and pinned captures (pin.dart)
    island/              live activities
    dev/                 bench.dart, standalone.dart
  linux/runner/          the GTK window and its channel
  test/                  mirrors lib/; support/fixtures.dart
  tool/gen_icons.dart    writes lib/widgets/raycast_icons.g.dart
  assets/                Inter.ttf; shadows/ (written by scripts/window-shadow.py)
```

**To work on a view, stay in its directory** (and its directory under
`test/`). `app.dart` already routes each kind of window to the widget at the
root of its directory (`PaletteWindow`, `SettingsWindow`, `DictationWindow`,
`ScreenshotOverlay`, `PinWindow`, `IslandWindow`; the palette shows
`ExtensionView` while a command has it): replace the placeholder, keep the
name. If something shared is missing, add a file to `widgets/` rather than
changing one that is there, and say so.

## Conventions

**The host.** `HostScope.of(context)` gives the `Host`.

```dart
final items = await host.search(query);                 // typed: lib/host/commands.dart
await host.invoke('some_command', {'argName': value});   // or by name; args in camelCase, as the web view's
host.on(HeyoEvent.indexReady).listen((payload) { … });   // names: lib/host/events.g.dart
```

A command that fails, or is not this window's to invoke, throws `HostError`
with the host's words. Which window may invoke what is in
`lib/host/commands.g.dart` (generated from `commands/access.rs`).

**A new command or event** is added where the web view's are: the function
and `generate_handler!`, a row in `commands/access.rs`, a row in
`shell/dispatch.rs` (the tests say so if you forget), then a method in
`lib/host/commands.dart` (a Dart test says so if you forget). An event is a
line in `wire.rs`; `cargo test` writes it into `events.g.dart`.

**Types.** Dart's models are written by hand and held to the Rust types by
example. `cargo test` writes one of everything a window is sent or sends into
`shared/fixtures/*.json` (`shell/fixtures.rs`); a Dart test reads each into
its model and must write the same JSON back (`expectRoundTrip` in
`test/support/fixtures.dart`), and another fails if a fixture is read by no
test. For a type of your view's own: add it to `fixtures()` in
`shell/fixtures.rs` (the module that owns the type gives a sample:
`#[cfg(test)] pub(crate) fn fixture()`), run `cargo test`, write the model in
your directory and a round trip in your tests. What the runtime sends
(`RuntimeMessage`, the tree of a view) is declared only in TypeScript
(`shared/wire.ts`) and has no fixture yet: whoever draws extension views will
want the runtime's tests to write some.

**Colours, sizes, text.** `HeyoTheme.of(context).colors` and
`lib/theme/tokens.dart` are the web view's tokens (`src/styles`), and a test
reads the stylesheet and fails if they differ: change a token there and here
together. No colours or sizes of a view's own where a token exists. Use
`HeyoText` for text, not `Text`: it sets the size's style and fixes the line
height, without which a line with Chinese in it is taller than one without.
What floats over the screen (dictation, screenshot, pin, island) is dark
whatever the scheme (`floats()` in `app.dart`); the rest follows the
`appearance` setting, live.

**Fonts.** Inter is bundled (one file, every weight; `theme.text` asks for
the weight). Flutter does not fall back to a system font for a character
Inter lacks, so `fonts.dart` asks fontconfig at start which fonts the system
would use for Chinese, Japanese, Korean and emoji (the user's own language
first) and names them in every style; a list of usual ones serves until it
has answered. Use `theme.text(…)` or `HeyoText`; a `TextStyle` built by hand
draws boxes for Chinese.

**Icons and images.** `HeyoImage(image)` draws a Raycast `ImageLike` by the
web view's rules (`src/raycast/Image.tsx`); `ItemIcon` is the icon of a row.
Raycast's icon names map to Lucide icons through
`lib/widgets/raycast_icons.g.dart`, generated from the web view's
`src/raycast/iconAliases.ts` (`dart run tool/gen_icons.dart`; a test fails
when it is stale). Application icons and assets that are SVG files go
through `flutter_svg`.

**Keys.**

- *Input methods.* Flutter shows a key to the framework before the input
  method sees it. While text is being composed, Enter, Escape and the arrows
  are the input method's: a handler that acts on them takes them away. Put
  key handling on the field (`HeyoTextField(onKey: …)`, or
  `keysUnlessComposing(controller, …)`), which is not asked while
  `controller.value.composing.isValid`. Never handle keys for a view that has
  a text field in a way that goes round this.
- *Presses.* Ask `isPress(event)` (`widgets/keys.dart`), not
  `event is KeyDownEvent`. A window hidden by a key never sees the key go
  up, and Flutter reports the next press of it, after the window is shown
  again, as a repeat: the first Escape or Enter after every show would be
  ignored.

**Lists.** `widgets/selection.dart` is `src/selection.ts`: `step`,
`shortfall`, `reveal`, and `PointerScope`/`UnderPointer`, by which a row is
selected only when the pointer really moved and a list scrolls its selection
into view only for the keyboard. `ListView.builder` with `itemExtentBuilder`
keeps long lists cheap.

**Cheap to draw.** Design for ordinary hardware. `ShaderMask`, `Opacity` over
a subtree, `BackdropFilter` and `saveLayer` draw their contents twice;
without a graphics card that is most of a frame. (Fading the end of the
palette's list and of each row's subtitle with `ShaderMask`s took a frame
from 4 ms to 9 ms of drawing in the sandbox; laying the panel's colour over
the list's end, and `TextOverflow.fade`, cost nothing.) Measure with
`--bench`.

## Testing

    cd ui && flutter analyze && flutter test       # or scripts/build-ui.sh --test
    (cd src-tauri && cargo test)                   # also writes fixtures, events.g.dart, commands.g.dart
    scripts/gui-test/flutter-shell.sh              # the whole path, in the sandbox

- Views are tested as widgets with a `FakeHost` (`lib/host/fake.dart`): say
  what each command answers, `emit` events, read `invoked`. `HeyoApp(host:,
  window: HeyoWindow(host), launch: Launch(kind: …))` is the whole
  application. See `test/palette/palette_test.dart`.
- `flutter test` uses a test font in which every glyph is a square: do not
  assert on text widths.
- `scripts/gui-test/flutter-shell.sh` starts Heyo in Flutter mode on a
  display of its own and checks the path end to end; it also prints frame
  times and memory. The sandbox scripts run on `:99` unless
  `HEYO_TEST_DISPLAY` (and `HEYO_TEST_DIR`) say otherwise. Look at the
  pictures it leaves in `$HEYO_TEST_DIR/shots/`.

## Measured

On the development machine (aarch64), in the sandbox: Xvfb, no graphics
card, so Flutter draws with Mesa's software renderer, as the web view draws
there and on the desktop. 2026-10-11, Flutter 3.41.9.

Other work was going on on the machine at the time, and the numbers moved
with it: take them as the order of things.

*Time.* From the host asking the palette to show until it has drawn: 13 to
56 ms over a dozen shows, most of them near 20 (debug Heyo,
`HEYO_UI_DEBUG`). From starting a `heyo-ui` to its first frame: 220 to 380
ms, which is what the first use of the settings, the overlay or a pinned
capture waits for.

*Frames* (`--bench`, 2000 rows, median / 95th percentile; a quiet run):

| | whole frame | of which drawing |
|---|---|---|
| an arrow key held | 7.8 / 11.6 ms | 4.2 / 6.0 ms |
| scrolling | 7.8 / 9.8 ms | 3.8 / 5.1 ms |

(The experiment's palette, with made-up rows and none of the icons, shadows
or fades: 7.3 / 10.3 and 3.6 / 4.5 for a held key in the same sandbox. The
web view there spends 11 to 13 ms on what follows each arrow key, measured
differently: `scripts/gui-test/scroll.sh`. A run while the machine was busy
gave 10.2 / 25.8 for the held key.)

*Memory* (release Heyo; resident, and proportional: what processes share
counted once):

| | web view | Flutter |
|---|---|---|
| started, palette hidden | 838 MB / 294 MB | 319 MB / 194 MB |
| palette shown, a search made | 884 MB / 331 MB | 356 MB / 229 MB |
| settings open as well | 1102 MB / 399 MB | 616 MB / 363 MB |

The web view's is four or five WebKit processes that share most of what they hold;
Flutter's is `heyo` (about 20 MB) and about 170 MB for each `heyo-ui`, most of
it the software renderer's (heap and anonymous memory; Dart's own heap is 10
MB). **Each further window costs Flutter a whole process**, where it costs
the web view a part of one: with the palette, the settings, the dictation
pill and the screenshot overlay all up, Flutter would hold more than the web
view in this sandbox. With a graphics card the renderer's share is different
and has not been measured (the desktop could not be used for this work).

## Other platforms and sessions

**Heyo's own windows.** The platform layer tells Heyo's windows from other
applications' (the window to return to, snapping in screenshots, whether Heyo
has the focus).

- *X11* goes by the window's class (`WM_CLASS` is `heyo`, `Heyo`;
  `x11/windows.rs` `is_own`). The runner sets the same program name and class
  as the `heyo` program has (`main.cc`), so `heyo-ui` windows are Heyo's own.
  Checked in the sandbox.
- *GNOME on Wayland.* The Shell extension's `isOwn` goes by the window's
  class too, which on Wayland is the application id: GTK takes it from the
  program name, which the runner sets to `heyo`. That should hold, and was not
  run. `_isHeyo` (who may call the extension's D-Bus interface) goes by the
  process that owns Heyo's bus name: `heyo-ui` never calls it, the host does.
  `ActivateOwn(title)` finds the window by its title, which the runner sets
  from the same titles the web view's windows have. `desktop.own_focused()` is
  asked by the host as before.
- *Windows* goes by the process (`GetCurrentProcessId` in `platform/win`).
  `heyo-ui` is another process: `own_in_front`, the window list and
  `active_external` will need the ids of the processes Heyo started
  (`shell/flutter.rs` has them), or the windows' class. And a Windows runner
  (`ui/windows/runner`) offering `heyo/window`. Nothing of this is written.

**xfwm4 with its compositor on** draws the host's backdrop over the Flutter
overlay whatever the order of the two windows; there the host does not make
the backdrop (`platform/overlay.rs`), and the overlay must paint the capture.
GNOME Shell keeps the order. (xfwm4 also ignores input regions: see
`setInputRegion` above.)

**Wayland in general** has not been run with the Flutter interface: focus,
centring on the pointer's monitor (a client cannot place its own windows
there; the web view relies on the Shell extension for the same things) and
the overlay (`cover` asks for fullscreen on the monitor, as the web view's
does there) are written by analogy with the web view's and unproven.

## What will be hard

| In the web view | In Flutter | Recommendation |
|---|---|---|
| **HTML in Markdown** (`Detail`: `marked`, then DOMPurify; extensions put `<img>`, tables, `<details>`, inline HTML in it) | No HTML engine. `flutter_markdown`-style packages draw Markdown and drop or mangle HTML. | Parse Markdown to a tree and draw it natively with the tokens; support the HTML that extensions really use (`<img width>`, `<br>`, `<kbd>`, `<details>`, tables), show the rest as text. Collect what the installed extensions emit before deciding the subset. Do not embed a web view for it: that brings WebKit back. |
| **Selecting and copying text** anywhere on a page | Only inside `SelectableText`/`SelectionArea`, and across widgets only within one `SelectionArea` | Wrap detail views and metadata in one `SelectionArea`; check that Ctrl+C and the primary selection work on X11. |
| **Native context menus** | The web view has none of the toolkit's either (three `onContextMenu` handlers, drawn by Heyo) | Draw them, as now: `ActionPanel` is the same surface. |
| **Drag and drop** (dragging a file or an image out of a list) | Flutter has no drag between applications; `super_drag_and_drop` adds it with native code for each platform | The web view does not do it either today. Leave it until asked for; then that package, behind the runner. |
| **Accessibility** | Flutter builds a semantics tree, and its GTK embedder exports it over AT-SPI; nothing here has been run with a screen reader. The web view's own is thin (14 `aria-`/`role` attributes). | Label what is not text (`Semantics` on icons, rows, the selection); test with Orca before saying it works. |
| **Input methods** | Works through GTK's (`GtkIMContext`: IBus, Fcitx); the pre-edit is drawn by Flutter. Committed Chinese text was typed and drawn in the sandbox; **composing with a real input method was not run** (X11 or Wayland). The candidate window's place depends on Flutter telling GTK where the caret is. | Type Chinese with IBus and Fcitx on X11 and on GNOME Wayland before anything else is built on the text field. The rule about keys while composing (above) is the part that is ours. |
| **The screenshot overlay's native backdrop** | Kept: the host paints the frozen screen in its own window, and the Flutter overlay sits on it, transparent. Where that cannot be (no compositor, Wayland, xfwm4) the overlay paints the capture from the file it is sent, which Flutter can do with the graphics card. | Draw selection and tools over a transparent view; paint the capture (`RawImage` from the BMP) only when `backdrop` is false. Cropping and annotating need the pixels in Dart anyway (`dart:ui` `Image`, `toByteData`), where the web view used a canvas. |
| **Images of every kind** (`<img>` shows ICO, XPM-less SVG with CSS, animated WebP, remote images with the page's cache) | PNG, JPEG, GIF, WebP, BMP natively; SVG through `flutter_svg` (no CSS, no scripts, a subset of filters); no ICO, no XPM | Application icons in `.xpm` or `.ico` fall back to the initial tile; see how many there are before doing more. |
| **Filled icons** (Raycast's `…-filled-16`: the web view fills the Lucide outline) | An icon font cannot be filled | Drawn in outline for now. If it matters, ship Lucide's SVGs and draw them with `flutter_svg` (fill is then a colour). |
| **CSS effects** (`light-dark()`, `color-mix`, masks, `backdrop-filter`) | Done by hand, colour by colour; blur and masks cost a second pass | The tokens are ported; mix colours with `Color.lerp`; avoid masks and blur (see *Cheap to draw*). |
| **Video, audio, web content in an extension's view** | Not in the API Heyo implements | Nothing to do. |

## Commands and events

Every command the web view can invoke can be invoked over the bridge (48; a
test holds the two lists equal) and has a typed method in
`lib/host/commands.dart`.

*Run end to end from the Flutter interface in the sandbox* (the call made by
a Flutter view, the real command run, its effect seen):
`search`, `launch` (an application; a command of the runtime's), `hide`,
`get_settings`, `record_usage`, `open_settings`, `runtime_send` (closing a
command), `screenshot_finish` (cancel, pin).

*Dispatched only* (in the table, typed in Dart, never called by a view that
exists yet):
`get_item`, `list_quicklinks`, `set_favorite`, `move_favorite`, `set_alias`,
`reset_ranking`, `read_clipboard`, `copy_text`, `open_target`, `show_window`,
`snippet_expanded`, `set_item_hotkey`, `change_settings`,
`set_anthropic_api_key`, `open_extensions_folder`, `list_extensions`,
`set_extension_preferences`, `uninstall_extension`, `clear_clipboard_history`,
`get_hotkeys`, `set_hotkey`, `item_hotkeys`, `list_applications`,
`get_capabilities`, `install_shell_extension`, `local_models`, `ocr_status`,
`ocr_install`, `dictation_status`, `dictation_install`, `copy_diagnostics`,
`clipboard_sources`, `clipboard_meaning_status`, `clipboard_meaning_install`,
`check_for_update`, `install_update`, `extension_permissions`,
`revoke_extension_permission`, `dictation_toggle`, `dictation_cancel`.

*Events seen arriving in a Flutter window:* `heyo://shown` (to the palette
alone), `heyo://index-ready` (to every window), `heyo://runtime` (a command's
view), `heyo://screenshot` (to the overlay alone, held until its view
listened). *Routed the same way but not seen:* `heyo://settings-changed`
(the theme follows it in a widget test), `heyo://open-item`,
`heyo://dictation`, `heyo://dictation-install`, `heyo://ocr-install`,
`heyo://expand-snippet`, `heyo://clipboard-meaning-install`.

## Not done

- The views (see *Where it stands*).
- Windows: a runner, and own-window recognition by process.
- Wayland: not run.
- The dictation pill's window was never shown by a real dictation (no speech
  engine in the sandbox); placing it is the same `set_frame` the pinned
  capture uses.
- Composing text with an input method: not run.
- A graphics card: every number above is the software renderer's.
- Packages (deb, rpm, AppImage) do not carry `heyo-ui`; see
  `packaging/README.md`.
- One process for all windows. The embedder can attach several views to one
  engine (`fl_view_new_for_engine`), which would share the renderer, the
  fonts and Dart's heap between windows. It is the answer to the memory each
  window costs, and a different runner: not tried.
