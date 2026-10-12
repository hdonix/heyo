# Heyo on Wayland

On X11 any application may list and move other applications' windows, send
keystrokes, watch the clipboard, read the screen and grab keys. On Wayland
none of that is allowed to an ordinary client: each compositor decides what
it offers, and through what. This page says what Heyo needs, how GNOME is
served today, and what the other desktops could offer.

## What Heyo needs

| Capability | Used for | In the code (`platform/desktop.rs`) |
| --- | --- | --- |
| List windows, with title, application and frame | Switch Windows, the application last in front, picking a window in a screenshot | `windows`, `windows_shown`, `active_window`, `active_class` |
| Activate, close | Switch Windows, quitting applications | `activate`, `close` |
| Move and resize, knowing each monitor's free area | Window layouts (halves, thirds, next display) | `arrange` |
| A keystroke into the focused application | Pasting from the clipboard history, snippets, dictation | `paste` |
| The selected text | Extensions' `getSelectedText`, AI commands | `selected_text` |
| Clipboard changes while Heyo is in the background, and its content | Clipboard history | `watch_clipboard`, `clipboard_text`, `clipboard_image` |
| The screen's pixels, without a prompt each time | Screenshots, text recognition | `grab` |
| A window over everything on one chosen monitor, with the keyboard | The screenshot overlay | `place_overlay` |
| Global hotkeys | Opening the palette, per-command hotkeys | `hotkeys` |
| One key's press *and release*, while another application has the focus | Hold to talk | `watch_hold` |
| Its own passing windows kept out of the dock, the switcher and the overview; and brought to the front without the user having touched them | The palette, the dictation pill, the overlay | `activate_own`, `own_focused` |
| Which monitor the pointer is on | Capturing the monitor being looked at | `pointer_monitor` |

Without any of it (the `Generic` adapter) Heyo still launches things, runs
extensions, keeps a clipboard history by polling, pastes through `wtype` or
`ydotool` if installed, and captures through `gnome-screenshot`. The window
commands are left out of the search.

Hotkeys need nothing from the compositor but a way to run a command: every
hotkey is `heyo`, `heyo --clipboard` or `heyo --open <item id>`, handed to
the running instance. On GNOME Heyo writes them as custom shortcuts with
`gsettings` (`platform/hotkeys.rs`); elsewhere the user can bind the same
commands in the compositor's own configuration.

## GNOME: a Shell extension

`gnome-extension/heyo@heyo.app` runs inside GNOME Shell and exports
`/dev/heyo/Shell` (`dev.heyo.Shell`) on the session bus. `platform/gnome/`
is its client: the `Gnome` adapter. `gnome-extension/README.md` describes
the interface for the extension's reviewers, including who may call it (the
process that owns Heyo's bus name, checked on every call).

### Versions of GNOME

The extension declares GNOME 45 to 50. It is tested on 50 (the container in
`scripts/wayland-test/`); 45 to 49 were checked by reading the headers and
the Shell's JavaScript of each release branch (`gnome-45` … `gnome-50` of
mutter and gnome-shell). What was compared, and found the same in all six
unless noted:

- Extension entry points: `Extension`, `InjectionManager` (`extensions/extension.js`), `Config.PACKAGE_VERSION`.
- `Meta.Window`: `get_id` (64-bit; see `idOf`), `get_title`, `get_wm_class`, `get_monitor`, `get_frame_rect`, `located_on_workspace`, `minimized`, `skip_taskbar`, `is_fullscreen`, `unmake_fullscreen`, `delete`, `move_resize_frame`, `get_compositor_private`.
  **`maximize`/`unmaximize` take a direction up to 48 and nothing from 49** (`TAKES_FLAGS`).
  **`hide_from_window_list`/`show_in_window_list` exist from 49** (looked for at `enable()`; before that, see "Heyo's own windows").
- `Meta.Display`: `get_tab_list`, `sort_windows_by_stacking`, `get_n_monitors`, `get_monitor_geometry`, `get_monitor_scale`, `get_selection`, `focus_window`, `grab_accelerator`, `ungrab_accelerator`, the `accelerator-activated` signal (the action comes first in all six).
  The order of `get_tab_list` is the same (not minimized first, most recent first); 49 added a third list type, which is not used.
- `Meta.Workspace.get_work_area_for_monitor`, `Meta.Selection` (`owner-changed`, `get_mimetypes`), `Meta.external_binding_name_for_action`, `Meta.Backend.get_core_idle_monitor`, `Meta.IdleMonitor.get_idletime`.
- Clutter: `get_default_backend().get_default_seat().create_virtual_device()`, `VirtualInputDevice.notify_keyval` (time in microseconds), the modifier masks, `Actor.get_resource_scale` (on the stage).
- St and Shell: `St.Clipboard` (`get_text`, `set_text`, `get_content`), `Shell.Screenshot.screenshot_area`, `Shell.AppSystem.get_running`, `Shell.WindowTracker.get_window_app`, `global.get_pointer`, `global.get_current_time`, `global.window_manager`'s `map` signal.
- `Main.activateWindow`, `Main.wm.allowKeybinding`.
- That `ShellApp` follows `notify::skip-taskbar` (so an application whose windows are all hidden stops counting as running), and that `Dash`, `altTab.js`, `workspace.js` and `workspaceThumbnail.js` read `get_running()` and `skip_taskbar` the way the fallback for 45–48 expects.
- JavaScript: nothing newer than the engine of GNOME 45 (SpiderMonkey 115).

What reading cannot show, and so remains unverified below GNOME 50: that
the calls behave the same (above all the held key, which depends on how the
Shell reports accelerators on modifiers; the virtual keyboard; and the
fallback for hiding windows, which was run on GNOME 50 with the newer
method masked, not on an older Shell), and anything a distribution patches
(Ubuntu's Dock is a separate extension; it also asks `get_running()`, but
was not run).

### Updating the extension

The extension has one number, `const API` in `extension.js`, which goes up
by one with every change and which `Version()` returns. The interface only
grows: methods, signals and JSON fields are added, never changed or
removed. So:

- A **newer** extension than Heyo knows is used as it is.
- An **older** one is used too, for what it has. Fields it does not send
  are optional on Heyo's side (`#[serde(default)]` in `gnome/mod.rs`), and
  each says what is assumed without it. Version 1 lacks: the units
  (`density`), the pointer's monitor, the `Ready` signal, and keeping
  Heyo's windows out of lists.
- `gnome::OLDEST` is the oldest version Heyo works with at all.

Settings → General shows where it stands (`gnome/extension.rs`, `standing`):

| State | Meaning | Shown |
| --- | --- | --- |
| none | Not GNOME on Wayland | nothing |
| missing | Not installed, or switched off | "Install" |
| outdated | Installed, older than the one Heyo carries; it keeps serving if it runs | "Update" |
| pending | The carried one (or a newer) is in place and switched on, the session has not loaded it | "Log out and back in" |
| active | Running, not older than the carried one | nothing |

**An update cannot take effect without logging out**, on any of GNOME 45 to
50. The Shell reads a user's extensions when it starts; since 45 an
extension is an ES module, which stays cached once imported, and the Shell
refuses to load a different version of an extension it has already loaded
(`extensionSystem.js`: "A different version was loaded previously. You need
to log out for changes to take effect."). On Wayland the Shell cannot be
restarted without ending the session. A first installation waits for the
next login for the first of these reasons: nothing an application can call
makes the Shell look at its extensions directory again. Until then an older
extension that is running keeps working, which is why Heyo tolerates one.

### Units: high-DPI and several monitors

Every position the extension gives or takes (window frames, monitors, work
areas, the area to capture) is in the Shell's own coordinates. What those
are depends on how Mutter lays monitors out:

- **In pixels** ("physical"; integer scales only; what GNOME 46 does until
  fractional scaling is switched on, which is how an Ubuntu 24.04 session
  is expected to start): a 4K monitor at 200 % is 3840 units wide.
- **Scaled** ("logical"; needed for fractional scales; what the GNOME 50
  container does): the same monitor is 1920 units wide, a window's frame is
  in those units, and a capture of N units has N × scale pixels.

`Monitors()` says which with `density`: pixels per unit on that monitor.
(The extension finds out from the stage's resource scale, which is 1 when
laid out in pixels.)

The toolkit counts differently again. GTK 3 only knows whole scales, and
Tauri's "physical" monitor rectangle is GTK's rectangle times that scale.
Laid out in pixels this equals the Shell's; scaled, it is the Shell's units
times the scale *rounded up* (at 150 % GTK draws at 200 % and the Shell
shrinks it). `gnome/mod.rs` converts both ways (`Monitor::toolkit`,
`stage_area`, `toolkit_frame`), and three things follow:

- A capture has the monitor's real pixels, fewer than the toolkit counts
  under fractional scaling. The overlay stretches it over the monitor
  (`platform/overlay.rs`), and window frames are fitted to it
  (`screenshot.rs`, `fitted`). The picture saved is pixel for pixel.
- The overlay is made fullscreen on the captured monitor
  (`fullscreen_on_monitor`), since a Wayland window cannot place itself.
- The monitor to capture is the one the Shell says the pointer is on; the
  toolkit only knows the pointer over Heyo's own windows.

Window layouts stay in the Shell's units throughout and need no conversion.

`scripts/wayland-test/test.sh` checks all of this in four layouts: two
monitors at 100 %; 200 % beside 100 % (scaled); 150 % beside 125 %; and
200 % beside 100 % laid out in pixels.

### Heyo's own windows

The palette, the dictation pill, the screenshot overlay and pinned
screenshots are not applications to switch to. On X11 the skip-taskbar hint
says so; a Wayland client has no such request. The extension recognises
them (application id `heyo` and one of four titles) when they are mapped
and:

- from GNOME 49, calls `Meta.Window.hide_from_window_list()`, which is what
  the hint does on X11: the window leaves the Dash, both switchers and the
  overview, and an application with no other window stops counting as
  running;
- up to GNOME 48, where that does not exist, makes `skip_taskbar` answer yes
  on those windows' JavaScript objects (the switchers and the overview read
  it) and leaves the application out of `Shell.AppSystem.get_running()`
  (the Dash asks that) while all its windows are of that kind.

The settings window is an ordinary window and is listed. It is not the
web view's but Slint's ([slint-settings.md](slint-settings.md)), a Wayland
client of its own beside GTK: it is drawn on the CPU, which on Wayland
leaves it no transparency, so it has no shadow and square corners there;
it cannot bring itself forward, so the extension does that (`ActivateOwn`,
as for the web view's windows); and it is given Heyo's application id, by
which the extension knows Heyo's windows.

### The lock screen

The Shell switches extensions off while the screen is locked and on again
after. The extension keeps no promise across that: it emits `Ready` when
switched on, and Heyo asks again for the key it wants watched.

## Other desktops

None of this is implemented. The `Desktop` trait is the place for each: an
adapter per row of the table below, chosen in `desktop::choose`.

### KDE Plasma (KWin)

| Capability | Through | Notes |
| --- | --- | --- |
| Windows: list, activate, close, move, resize, work areas | A KWin script, loaded at run time over D-Bus (`org.kde.KWin`, `/Scripting`: `loadScript`, then `run`) and answering through `callDBus` to a small service Heyo exports | No installation and no login needed; any session application may do it. The script API differs between Plasma 5 (`workspace.clientList()`, `activeClient`) and 6 (`windowList()`, `activeWindow`). `kdotool` works this way. |
| Clipboard changes and content | The data-control protocol (`ext-data-control-v1`, or the older `wlr-data-control-unstable-v1`), which KWin offers for clipboard managers | Shared with wlroots, below. `wl-clipboard-rs`, already a dependency, speaks it. |
| Selected text | The primary selection through the same protocol | |
| Own windows out of the task bar, the overlay over a chosen monitor | `wlr-layer-shell`, which KWin implements (through the `gtk-layer-shell` library for GTK 3) | Shared with wlroots. Without it: a KWin script can set `skipTaskbar` and `skipSwitcher` on Heyo's windows. |
| Hotkeys | The GlobalShortcuts portal, or KGlobalAccel over D-Bus | |
| Paste keystroke | The RemoteDesktop portal (a consent dialog, which can be remembered); or `ydotool` | KWin has no scripting call for synthetic keys, and its fake-input protocol is reserved to applications that declare it in an installed desktop file. |
| Screen capture | `org.kde.KWin.ScreenShot2` over D-Bus, which KWin gives only to applications whose installed desktop file lists it (`X-KDE-DBUS-Restricted-Interfaces`); else the Screenshot portal | The first needs Heyo installed with a desktop file, which packages do. |
| Hold to talk | The GlobalShortcuts portal reports release as well as press | Untried. |

Cost: moderate. The KWin script and its D-Bus round trip are the new part
(two API generations to cover); clipboard and layer-shell are shared work.

### wlroots compositors (Sway, Hyprland, river, labwc, Wayfire; niri and COSMIC implement most of the same protocols)

| Capability | Protocol | Notes |
| --- | --- | --- |
| Windows: list (title, application id, state), activate, close, maximize | `wlr-foreign-toplevel-management-unstable-v1` | **No geometry and no move or resize.** `ext-foreign-toplevel-list-v1` lists only. |
| Window layouts, frames, work areas | None. Each compositor's own IPC: Sway's (i3-compatible) socket, Hyprland's `hyprctl` socket | On tiling compositors "left half" mostly has no meaning; worth doing only for floating windows, if at all. |
| Paste keystroke | `virtual-keyboard-unstable-v1` | What `wtype` uses, which Heyo already runs if it is installed; speaking the protocol directly removes the dependency. |
| Clipboard changes and content, selected text | `ext-data-control-v1` / `wlr-data-control-unstable-v1` | As for KDE. |
| Own windows, the overlay | `wlr-layer-shell-unstable-v1`: a surface on a chosen output, on the overlay layer, with exclusive keyboard | The right tool for both the palette and the overlay; never in a task list; focus without asking. |
| Screen capture | `wlr-screencopy-unstable-v1`, or its successor `ext-image-copy-capture-v1` | No prompt; per output; pixels in a shared-memory buffer. |
| Hotkeys | None as a protocol. The compositor's configuration runs `heyo …`; Hyprland's portal implements GlobalShortcuts | Document the lines to add for Sway and Hyprland. |
| Hold to talk | Nothing (Hyprland's GlobalShortcuts portal aside) | |

Cost: one adapter that binds whatever the compositor's registry offers and
reports the rest as unsupported; `wayland-client` with the `wayland-protocols-wlr`
bindings. The protocols are small, but each is a Wayland client state
machine, and there are four of them. Window layouts are a separate piece of
work per compositor.

### XDG desktop portals (any desktop, and the only way from inside a sandbox)

| Portal | Gives | Limits |
| --- | --- | --- |
| GlobalShortcuts | Hotkeys with press and release | The user confirms the bindings in the desktop's dialog; Heyo cannot set a key by itself. GNOME from 48, KDE, Hyprland; not the wlroots portal. An application outside a sandbox must register an application id that matches an installed desktop file. |
| RemoteDesktop | Synthetic keys (the paste keystroke) | A consent dialog when a session starts; newer portals can remember it with a restore token. The session shows as "remote control" in the desktop's indicators. |
| Screenshot | A picture of the whole screen as a file | GNOME and KDE ask for permission once per application. No area: Heyo crops, as it does with `gnome-screenshot`. |
| ScreenCast | A PipeWire stream of a monitor or window | Much machinery for a still picture; a dialog chooses the source. Only worth it where Screenshot is unavailable. |
| Clipboard | Reading and setting the clipboard | Only inside a RemoteDesktop session. Not a way to follow the clipboard in general. |

No portal lists, activates or moves other applications' windows, gives the
selected text, or places a window over the screen.

### Recommended order

1. **Data-control clipboard** in the `Generic` adapter: clipboard history
   without polling and the selected text, on KDE and every wlroots
   compositor at once, with a dependency already in the tree. Small.
2. **The Screenshot portal** as the capture of last resort instead of
   `gnome-screenshot`: every desktop, nothing to install. Small.
3. **Layer-shell** for the palette, the pill and the overlay: fixes focus,
   placement and task lists on KDE and wlroots together. Medium (a new
   library, and the windows are created by Tauri).
4. **KDE**: the KWin script for windows and layouts. Medium; the largest
   group of users after GNOME.
5. **GlobalShortcuts portal** for hotkeys where there is no `gsettings`
   way, and to try hold-to-talk with it. Medium; needs the application id
   registered.
6. **wlroots**: foreign-toplevel for Switch Windows, screencopy, the
   virtual keyboard spoken directly. Medium to large for a smaller
   audience that is used to configuring things by hand.
7. **RemoteDesktop portal** for pasting, last: the consent dialog and the
   "being controlled" indicator are a poor fit for a launcher.
