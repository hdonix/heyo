# A GNOME-on-Wayland session for testing

Current Ubuntu (26.04: GNOME 50) has no X11 session. This directory builds
a container with that desktop running headless, so Heyo can be tried on it
from any machine, without touching your own session.

    scripts/wayland-test/run.sh build      # once; about 1.5 GB
    scripts/wayland-test/run.sh start
    scripts/wayland-test/run.sh exec /heyo/scripts/wayland-test/in/start-heyo.sh
    scripts/wayland-test/run.sh exec /heyo/src-tauri/target/release/heyo      # toggle the palette
    scripts/wayland-test/run.sh exec /heyo/scripts/wayland-test/in/screenshot.sh /tmp/s.png
    docker cp heyo-wayland:/tmp/s.png .
    scripts/wayland-test/run.sh stop

    scripts/wayland-test/test.sh           # all the checks, in every monitor layout

`run.sh` mounts the checkout it is in (read-only, at `/heyo`); the binary
built on the host there (`pnpm tauri build --no-bundle`) runs as it is.

Sessions can be started in other states, to try what Heyo's settings do
about the extension: `run.sh start bare` (not installed), `run.sh start old`
(the first version of the extension, as after updating Heyo alone; with
`HEYO_OLD_EXTENSION=COMMIT`, the extension of that commit), then
`run.sh relogin` once Heyo has installed or updated it. `MONITORS="1920x1080
1920x1200" run.sh start` gives two monitors. `HEYO_TEST_SPEECH=DIR` lends
a speech engine (read only) for dictation; `test.sh` lends the one
installed for you, and dictates a recording made with
`../gui-test/say.py` (Flite). Pictures the checks take are copied out to
`src-tauri/target/wayland-test/shots/`, to be looked at: whether Chinese
text fits is judged there, by eye.

`in/` has the tools for use inside the session:

- `start-heyo.sh` starts Heyo hidden, logging to `/tmp/heyo.log`.
- `screenshot.sh PATH` saves the whole virtual screen.
- `windows.sh` lists windows: title, class, geometry, focus.
- `input.py` types, presses keys, clicks and drags like a person would
  (through Mutter's RemoteDesktop interface). Its `click X Y` finds the
  place by pushing the pointer into a corner first, which is only right on
  a single monitor at 100 %; elsewhere `check.py` puts the pointer through
  the Shell.
- `monitors.py` sets the monitors' scales, laid out scaled or in pixels;
  `layout.sh` prints what the Shell made of it.
- `shell.py` evaluates JavaScript in the Shell (harness only).
- `colour.py` is a window of one colour, to find again in a capture.
- `check.py` checks what Heyo does through its extension against what the
  Shell says: `lists` (the palette is in no list of windows, the settings
  window is, comes forward from behind another window and is still Heyo's
  when shown again; also with the way GNOME 45–48 need, tried on this Shell),
  `layout`, `capture` (the right monitor, its own pixels, a window picked
  with a click and saved pixel for pixel) and `relock` (the extension
  switched off and on, as the lock screen does); and what goes through the
  host there: `language` (简体中文 chosen while the settings window is
  open, every window in it), `ai` (AI Chat against a stub model on the
  session's 127.0.0.1, `../gui-test/llm-stub.py`: a follow-up, Stop
  Answering closing the connection, Escape not), `dictate`, `placement`
  (the palette and the settings window on the pointer's monitor),
  `settings` (moved, a click after, a press in its shadow's margin going
  through, closed, back), `ime` (Pinyin through IBus into its search
  field), `cold` (the first Slint window with Mesa's shader cache empty),
  and `upgrade` and `upgraded` (an extension of version 3 updated, before
  and after logging in again; `test.sh upgrade` alone).
- `atspi.py` asks a Slint window where its controls are (AT-SPI, through
  AccessKit), in the window's own pixels: a Wayland client does not know
  where its window is, the Shell does.
- `input.py stay` keeps a pointer for as long as it runs: the headless
  session has none of its own, and winit lets go of a window's text input
  when the seat loses its pointer, as it does each time an `input.py`
  call ends.

How it is put together, and what that means for what a test shows:

- GNOME Shell runs with `--headless --wayland --no-x11` on a virtual
  monitor, rendered in software. There is no XWayland, so anything that
  still needs X11 fails here as it would for an X11-free desktop.
- There is no systemd; `python-dbusmock` stands in for the login manager,
  which the Shell will not start without. Screen locking, power and
  network management are therefore absent.
- `harness-extension/` switches off the Shell's restrictions on its
  screenshot and scripting interfaces, for the tools above. Heyo must not
  depend on that: on a real desktop those calls are refused. A test that
  passes only because of the harness has shown nothing.
- No GPU and no window animations to look at: what a transition looks like
  still needs a person at a real desktop.
- It is GNOME 50 only. The extension also declares 45 to 49, which were
  checked by reading (`docs/wayland.md`), not by running.

## Sway: where a compositor offers the layer shell

GNOME's does not, and Heyo's own windows are done another way where one
does (`src-tauri/src/slint_windows/layer.rs`). `sway.sh` is a second
session, headless Sway, on an image built on this one:

    scripts/wayland-test/sway.sh build     # after run.sh build
    scripts/wayland-test/sway.sh test      # in/check-sway.py and in/check-sway-more.py at scale 1 and 2, then `safe` and `lost`
    scripts/wayland-test/sway.sh more ai   # a part of in/check-sway-more.py: language, ai, dictate, settings, ime, outputs
    scripts/wayland-test/sway.sh safe      # the overlay cannot trap anyone (in/safe-sway.py)
    scripts/wayland-test/sway.sh lost      # nothing is left on the screen when what draws it stops (in/lost-sway.py)
    SCALE=2 MONITORS=3840x2160 scripts/wayland-test/sway.sh drag   # how soon the overlay follows the pointer

A headless Sway has no pointer and no keyboard. The checks bring their
own (`in/swaybed.py`): a virtual pointer (`in/pointer.py`), keys typed by
`wtype`, and an input method that composes what it is told to
(`in/ime.py`). A layer surface is in no tree, so what is on the screen is
judged from pictures of it (`grim`).

What this bed cannot show: whether dictated words reach a window. Heyo
pastes them here (the clipboard, then Ctrl+V typed with `wtype`), and no
GTK window of the bed takes a paste, not even one done with `wl-copy` and
`wtype` by hand; so `check-sway-more.py dictate` checks what Heyo put on
the clipboard. `wtype` cannot type a shifted letter with Control (its
keyboard has no second level), nor Control and a letter into AI Chat's
reply bar, so Stop Answering is chosen from the actions of Ask AI.
