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
(the first version of the extension, as after updating Heyo alone), then
`run.sh relogin` once Heyo has installed or updated it. `MONITORS="1920x1080
1920x1200" run.sh start` gives two monitors.

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
  window is; also with the way GNOME 45–48 need, tried on this Shell),
  `layout`, `capture` (the right monitor, its own pixels, a window picked
  with a click and saved pixel for pixel) and `relock` (the extension
  switched off and on, as the lock screen does).

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
