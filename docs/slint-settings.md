# The settings window is drawn by Slint (2026-10-12)

The settings were a web view's window. They are now drawn by
[Slint](https://slint.dev) with Skia on the CPU, inside the host, and the
web ones are gone. This is the record of that decision: what was
measured, what it rests on, and what was not verified. How the window is
built is said at the top of `src-tauri/src/slint_windows/mod.rs`.

## Why

WebKit cannot use the GPU on the development machine (NVIDIA's driver
with `nvidia-drm modeset=0`), and a web view takes most of a second to
open a window. Slint was first measured on Heyo's overlay windows (the
notes are on the branch `slint-settings`,
`experiments/overlay-bench/results/frameworks/slint/NOTES.md`): drawn by
Skia on the CPU it kept up without a GPU, built for Windows from Linux,
and took an input method. GPUI had been tried on the settings before and
not taken up (`docs/gpui-evaluation.md` on the branch `gpui-settings`).

## What was measured

Against the web settings as they were at `9a86c62`, release builds, in a
sandbox (Xvfb 1920×1200 at 1×, GNOME Shell compositing in software), with
the screen itself as the clock (`overlay-bench --watch` on pixels); 5 to 7
runs each. What they wrote is in `experiments/slint-settings/results/`
(`open.tsv`, `react.tsv`, `scroll.tsv`); the scripts that measured, which
need the web settings and the watching program of the overlay experiment,
are on the branch `slint-settings` (`experiments/slint-settings/scripts/measure-*.sh`).

| | Slint | web |
|---|---|---|
| Asked for to on screen, the first time in a run | 138 ms | 778 ms |
| again | 67 to 77 ms | 755 to 762 ms |
| A click on a switch to the first frame that shows it, median / worst | 28 / 30 ms | 34 / 39 ms |
| A click on a page in the sidebar | 16 / 22 ms | 24 / 85 ms |
| A letter typed into the sidebar's search | 18 / 31 ms | 23 / 24 ms |
| Scrolling the applications: the screen shows the list somewhere new every, median / 95th / worst | 17 / 20 / 50 ms | 17 / 20 / 80 ms |
| Slint's own time for a frame while scrolling, median / 95th / worst | 4.2 / 5.4 / 6.3 ms (on four small cores 4.9 / 6.1 / 7.4) | |
| Processor time for that scroll, all of Heyo's processes | 0.52 s | 0.81 s |
| Memory of all of Heyo's processes: before / open / closed again | 278 / 282 / 282 MB | 278 / 341 / 269 MB |

Two things about these figures.

- **Slint reacted later than the web view until it was worked around.**
  As it comes, Slint paces drawing on X11 and Windows with a timer that
  first fires a frame's time after something changed, so a click was seen
  43 ms later and a typed letter 34 ms later, against 35 and 20 for the
  web view. The window now asks winit for a frame at the next turn of the
  loop after a key, a click or the wheel (`window.rs`, `draw_soon`); the
  table has the figures with that.
- **The sandbox flatters the web view.** What makes WebKit slow on the
  real desktop (4K, through the compositor, without the GPU) is not there
  in Xvfb. Nothing was measured on the real desktop.

## The fonts

Slint's text stack asks the system for fonts, and an English Windows
answered with one that has no 发, 这 or 议. So Heyo carries its own, in
the program, put before the system's (`src-tauri/fonts/`, each with its
licence; `slint_windows/fonts.rs`): Inter variable (876,576 bytes, SIL
OFL 1.1) and Noto Sans SC variable, CFF2 outlines (15,054,748 bytes, SIL
OFL 1.1 with the reserved name "Source", which it does not use).

The same typeface as a variable TTF is 17.8 MB, as one static weight
8.3 MB, and cut to the 6,763 characters of GB 2312 3.2 MB. The cut was
not taken: what is outside it (镕, 喆, 堃, 啰, every traditional character)
turns up in names and in what people dictate, and would fall back to the
system's fonts, which is the fault the font is there to cure. Once
Chinese has been drawn the font costs about twice its size in memory
(Skia copies it); until then nothing. The font could be a file beside
the program instead (Fontique takes bytes from anywhere), which would let
the web view use the same file through the asset protocol; it is inside
because that needs nothing of the packaging.

## Windows

The same `.slint` and Rust; only taking a lost window away (`guard.rs`)
and asking for the work area (`area.rs`) are Windows' own. Cross-compiled
from Linux for `aarch64-pc-windows-msvc`
(`scripts/package/build.sh`), and run on a
Windows 11 ARM machine with no graphics card: it draws with Skia on the
CPU and with Slint's own renderer, and a panic on Slint's thread left
Heyo running.

Skia comes prebuilt against Microsoft's C++ library as a DLL, so
heyo.exe takes `MSVCP140.dll` at start and **does not start on a Windows
that never had the Visual C++ redistributable installed**. Linking the
library in is refused (lld-link: "mismatch detected for 'RuntimeLibrary'",
Skia's objects are MD, `libcpmt` is MT), and so is asking for the DLL
late (lld-link: "cannot delay-load … due to import of data"). What works
is the DLLs beside heyo.exe: `scripts/package/vcruntime.sh <arch> <dir>`
fetches `msvcp140.dll` and `vcruntime140.dll` (and on x86_64
`vcruntime140_1.dll`) from Visual Studio's own package, pinned and
checked. Microsoft's "Visual Studio 2022 Redistribution" page (dated
2025-11-11) lists the files under `VC\redist` as ones that may be
distributed with a program, unmodified, by someone with a licensed copy
of Visual Studio, subject to its licence terms; that is a reading, not
legal advice. The installer does not carry them yet.

heyo.exe for ARM went from 10.3 MB to 44.5 MB: the fonts 15.9, Slint with
winit, its text stack and its own renderer 10.7, Skia 7.7.

## The dictation pill

The second window Slint draws, in place of a web view kept for a strip of
black with five bars in it (`slint_windows/pill/`). It looks as it did
and follows the rules its web tests held it to, which are tests of
`pill/model.rs` now; it also arrives and goes, grows with a spring, and
fades words that are clipped.

Measured as the settings were (GNOME Shell on Xvfb at 1×, release
builds, the screen as the clock, six recordings each), from `heyo
--dictate` being started, which is what the hotkey does:

| | Slint | web |
|---|---|---|
| First pixels of the pill | 41–55 ms | 44–63 ms |
| Fully arrived | 89–103 ms | the same as its first pixels: it popped |
| First recording of a run, Slint not started ahead | 92 ms | |
| Heyo and all it starts, idle | 234 MB | 296 MB |
| Heyo's own process, idle | 182 MB | 176 MB |

It is no faster than the web pill was: nearly all of the 50 ms is the
second process handing over and the compositor. What it saves is the
web view's process. Slint's thread is started when Heyo starts, and the
pill's window made then, so that the first recording is like the rest;
that is where the 6 MB go. The numbers and how they were taken are in
`experiments/slint-settings/` (`results/pill.tsv`, `scripts/measure-pill.sh`,
which needs the watching program of the overlay experiment).

What the window took:

- **Above everything and never focused is not something `.slint` can
  say.** On X11 the window goes past the window manager
  (override-redirect), told to winit through a hook that is not told
  which window it is for.
- **Slint makes a hidden window anew when it shows it**, without what the
  hook said. So to Slint the pill is shown once and stays shown, and
  winit takes it off the screen and puts it back.
- **A window that fades in from nothing is seen late**: its first frames
  are too faint. The pill arrives from 60%.

Checked in the sandbox (`experiments/slint-settings/scripts/pill.sh`, and
`scripts/gui-test/dictate-live.sh`, which passes): the keyboard stays
with the application when the pill appears and when it is clicked; a
click finishes; the button, which shows when the pointer is on the pill,
cancels; it is above a fullscreen window; and with Slint's thread lost in
the middle of a recording the pill is gone from the screen, the words
still arrive, and the next recording works without one.

## What Slint made hard

- **Its list assumes rows of one height.** `ListView` keeps its place by
  the number of its first row and an average height, and does not forget
  it when its model is replaced: the next page began at its second row
  until the window told it exactly which lines went and came. The scroll
  bar's thumb is an estimate for pages whose lines differ much.
- **A text's height was worked out before a control beside it had taken
  its place**, so descriptions were cut off; the text's width is written
  out as arithmetic (`Row` in `ui/widgets.slint`).
- **A binding that depends on itself panics at run time**, not when
  compiling.
- **A spring is a curve of fixed duration**, with its bounce a number in
  the source: no velocity, so one interrupted starts again from rest, and
  none can come from the tokens. Nothing animates out: what is removed is
  gone at once.
- **Text is plainer than the web's**: no font features (Inter draws `->`
  as an arrow, and it cannot be turned off), a line height that is a
  factor of the font's own, no tracking in em.
- **One shadow an element, none inset.**
- **The key event has the character, not the key**: hotkeys are recorded
  from winit's events, through an interface Slint marks unstable, as is
  the one the fonts are registered through.
- **One renderer and one event loop for the life of the process**: if
  Slint's thread is lost, the settings cannot come back until Heyo is
  restarted, and the palette says so.
- **On Windows what is drawn beyond the screen's edge is lost**, and
  moving the window onto the screen does not bring it back: the window
  draws itself again whole when it is moved.
- **A text stopped following its words** (Slint 1.18.1). A text is drawn
  from shaped paragraphs Slint keeps, and only those depend on what the
  text says. Once 1024 texts have been shaped it drops the paragraphs of
  every text not drawn in the last two frames, which with only what
  changed being drawn is nearly all of them, and a text whose words then
  change is not drawn again: after a long page had been scrolled through,
  the title kept the last page's name. Every text here is one component,
  which makes the font it asks for depend on its words (`Line` in
  `ui/widgets.slint`). To be taken out when Slint no longer needs it.
- **A menu is as tall as what is in it**, and is cut off at the window's
  edge: the menus here show ten rows and a half and scroll, and take the
  keys while they are open.
- **On Wayland a window drawn on the CPU cannot be seen through**
  (softbuffer hands the compositor XRGB): the margin the shadow falls in
  would be black, so there the window has none, and square corners. A
  Wayland window is also made anew each time it is shown, and the second
  time without the application id a hook gave the first: Slint is told
  the id itself (`slint::set_xdg_app_id`).

## Not verified

Clicked in the sandbox (GNOME Shell on X11, release build), and checked
against the files Heyo wrote: every page; a preference of every type, of
an extension and of a command; revoking a permission; uninstalling; the
clipboard's and snippets' cards; the API key; the Shortcuts filter and
aliases; Pinyin composing in a field, in the vocabulary and in a list's
search; both kinds of menu by the keys alone; moving, closing and
reopening the window; a screen of 800×600. In GNOME 50 on Wayland
(`scripts/wayland-test/`): the window opens, comes forward from behind
another window, keeps its class when shown again, and takes Pinyin with
the candidates under the cursor.

- The three downloads were tried with a stand-in for the helpers
  (`scripts/gui-test/install-stub.cjs`), which reports progress and
  fetches nothing: progress, the end, and a failure are shown. A real
  download from this window was not made.
- Updating was tried with a build that has the updater and a version
  served from this computer
  (`experiments/slint-settings/scripts/updates.sh`): checking, the offer,
  being up to date, and no server answering. What was served is refused
  for its signature, by design: an update being installed and Heyo
  starting again was not seen.
- Windows with every page; `x86_64-pc-windows-msvc` beyond `cargo check`.
- Wayland: other compositors than GNOME's, and what happens to a window
  lost there (it may stay on screen as it was last drawn).
- A screen reader. AccessKit is compiled in and the controls say what
  they are. A menu has no typing a letter to go to a choice, which a
  `<select>` had.
- Following the system's light and dark while the window is open, which
  on Linux needs the desktop's portal; without one the host's own answer
  is used each time the window is shown.
- The real desktop, at 2×.
