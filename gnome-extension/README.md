# Heyo's GNOME Shell extension

`heyo@heyo.app/` is the part of Heyo that runs inside GNOME Shell. On Wayland
an application cannot see or arrange other applications' windows, send them
a keystroke, notice a copy while it is in the background, or read the
screen. The Shell can; this extension does those things when Heyo asks, over
D-Bus. Heyo carries the two files inside its binary and installs them from
Settings → General (`src-tauri/src/platform/gnome/extension.rs`).
`docs/wayland.md` has the whole picture.

It is two files of plain JavaScript, no build step, no settings schema, no
preferences window, no stylesheet, no translations.

## For reviewers (extensions.gnome.org)

**What it does.** It exports one object, `/dev/heyo/Shell` (interface
`dev.heyo.Shell`), on the Shell's own session-bus connection. Its methods
list windows and monitors, activate, close, maximize or move a window, press
the paste shortcut through a virtual keyboard, read and set the clipboard,
save an area of the screen to a file, and watch one modifier key being held
(for push-to-talk). It emits a signal when the clipboard changes. It also
keeps Heyo's own short-lived windows (the palette, the dictation pill, the
screenshot overlay) out of the Dash, Alt+Tab and the overview. Without Heyo
installed it does nothing: no method can be called and nothing is shown.

**Who may call it.** These are powers no ordinary application has on
Wayland, so the interface is not open to the session. Every method goes
through `_guard`, which calls `_isHeyo(invocation)`:

- Heyo owns the well-known bus name `dev.heyo.app.SingleInstance` for as
  long as it runs (it is its single-instance lock).
- For each call, the extension asks the bus daemon
  (`org.freedesktop.DBus.GetConnectionUnixProcessID`) for the process id
  behind the caller's unique name and for the process id behind that
  well-known name, and goes on only if they are the same process. (By
  process, not by connection, because Heyo talks over more than one
  connection.)
- Anyone else gets `dev.heyo.Shell.Error.Denied`. If nobody owns the name,
  everyone does.

What this does and does not protect against: another application cannot use
the interface while Heyo runs, and cannot take the name while Heyo holds it.
An application that claims the name `dev.heyo.app.SingleInstance` when Heyo
is *not* running would be accepted: a session bus has no way to tell
programs of the same user apart, and anything running as the user outside a
sandbox can already do more than this interface offers (for instance by
installing an extension of its own). Sandboxed applications (Flatpak, Snap)
cannot own that name unless their permissions grant it. The held key is
dropped when the name loses its owner (`Gio.bus_watch_name`).

**Things a review usually asks about.**

- *Nothing at import time.* The module's top level only declares constants;
  everything is set up in `enable()` and undone in `disable()`: the D-Bus
  object, the `accelerator-activated`, `owner-changed` and window-manager
  `map` handlers, the bus-name watch, the grabbed accelerator, the virtual
  keyboard, the timeout source of a held key, and the overrides below.
- *Overrides.* Up to GNOME 48 a Wayland window cannot be told to stay out
  of window lists, so the extension (a) defines `skip_taskbar` /
  `is_skip_taskbar` on the JavaScript objects of Heyo's own windows, and (b)
  overrides `Shell.AppSystem.prototype.get_running` through the Shell's
  `InjectionManager` to leave out the application whose windows are all of
  that kind. Both are removed in `disable()`. From GNOME 49 on neither is
  done: `Meta.Window.hide_from_window_list()` is used instead.
- *Clipboard.* It reads the clipboard only when Heyo asks, and tells Heyo
  that it changed (with the MIME types, not the content). Heyo is a
  clipboard manager among other things; the description says so.
- *Synthetic input.* One thing only: the paste shortcut (Ctrl+V, or
  Ctrl+Shift+V for terminals), through `Clutter.VirtualInputDevice`.
- *Files.* It writes two kinds of file, at paths Heyo gives, both in the
  user's runtime directory: a screenshot and the clipboard's image.
- *Session modes.* Only `user`: it is off on the lock screen.
- *No* network access, subprocesses, GSettings, `eval`, timers left
  running, or binaries.

**Versions.** GNOME 45 to 50. The only version-dependent code:
`Meta.Window.maximize()`/`unmaximize()` lost their argument in 49
(`TAKES_FLAGS`), and `hide_from_window_list()` exists from 49 (looked for at
`enable()`).

## Publishing

    gnome-extension/package.sh

writes `build/heyo@heyo.app.shell-extension.zip` (it refuses until
`metadata.json` is complete). What is left for a person to do:

1. Add `"url"` to `metadata.json`: the public page of Heyo's source. The
   site requires it and the script checks for it; none is set because the
   project has no public address yet.
2. The uuid `heyo@heyo.app` implies the domain `heyo.app`. Use it only if
   the project controls that domain; otherwise change the uuid (here, in
   `extension.rs`, in `scripts/wayland-test/`) before the first upload, as
   it cannot be changed afterwards.
3. Upload the zip at <https://extensions.gnome.org/upload/> with an account,
   and answer the review. The section above is written to be pasted there.
4. The site numbers versions itself and rewrites `"version"` in
   `metadata.json`. Heyo does not look at that number: it reads `const API`
   from `extension.js` (see `extension.rs`), which must go up by one with
   every change to the extension.
5. Decide who updates whom. Heyo installs the copy it carries when that is
   newer and leaves a newer installed one alone, so a copy from the site and
   Heyo's own can coexist; but a user with the site's copy gets updates
   from two sides. Keeping the site's copy and the carried one the same
   release avoids surprises.
